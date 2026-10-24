// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
//! Motion capture (BVH) import and retargeting onto a 2D rig.
//!
//! * [`parse_bvh`]: parser written from the public BVH description (HIERARCHY with ROOT / JOINT /
//!   End Site, OFFSET, CHANNELS in any order, MOTION with `Frames:` / `Frame Time:` and one row of
//!   values per frame). Tolerant of CRLF, tabs, repeated blanks, braces glued to words and keyword
//!   case; errors carry the line number.
//! * Forward kinematics in 3D: every channel is an elementary right-handed rotation (degrees) or
//!   translation and the local transform is `T(offset) * C1 * C2 * ...` in the listed order, so
//!   `Zrotation Xrotation Yrotation` means `Rz * Rx * Ry`.
//! * [`View`] projection to the stage plane (front = XY, side = ZY, top = XZ, or a yaw about the
//!   vertical axis) and per-bone 2D angles.
//! * [`match_bones`]: automatic mapping of BVH joints to rig bones (normalised names plus synonym
//!   tables, then hierarchy topology and limb-length sanity as fallback) with a confidence table
//!   and a manual override map.
//! * [`retarget`]: rest-relative 2D angle deltas applied to the rig's bone angle channels, scaled
//!   root translation, resampling to the project frame rate, smoothing and optional foot lock.
//!   The result is a list of appended keyed channels (non-destructive, channel locks respected),
//!   which [`bake`] adds directly or [`bake_op`] expresses as one undoable editor operation.
//!
//! Limits: a 2D projection discards depth (a bone pointing at the camera has no angle and holds its
//! last one), twist about a bone's own axis is ignored, there is no finger retargeting beyond what
//! the rig names, and the rig bone keeps its own length (only angles and root travel are
//! transferred).

use crate::anim::{Ease, Track};
use crate::edit::Op;
use crate::project::{Channel, Project, Prop};
use std::collections::{BTreeMap, BTreeSet};

pub type V3 = [f64; 3];

const MAX_JOINTS: usize = 4096;
const MAX_DEPTH: usize = 512;
const MAX_FRAMES: usize = 1_000_000;
const MAX_VALUES: usize = 200_000_000;

// ---------------------------------------------------------------------------------------------
// BVH data model and parser
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chan {
    Xpos,
    Ypos,
    Zpos,
    Xrot,
    Yrot,
    Zrot,
}

impl Chan {
    fn parse(s: &str) -> Option<Chan> {
        Some(match s.to_ascii_lowercase().as_str() {
            "xposition" => Chan::Xpos,
            "yposition" => Chan::Ypos,
            "zposition" => Chan::Zpos,
            "xrotation" => Chan::Xrot,
            "yrotation" => Chan::Yrot,
            "zrotation" => Chan::Zrot,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Joint {
    pub name: String,
    pub parent: Option<usize>,
    pub offset: V3,
    pub channels: Vec<Chan>,
    /// Index of this joint's first value in a frame row.
    pub chan_start: usize,
    pub end_site: Option<V3>,
    pub children: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bvh {
    /// Parents always precede their children (file order).
    pub joints: Vec<Joint>,
    /// Seconds per frame.
    pub frame_time: f64,
    pub frames: usize,
    pub num_channels: usize,
    /// `frames * num_channels` values, row per frame.
    pub data: Vec<f64>,
}

struct Tok<'a> {
    line: usize,
    s: &'a str,
}

fn tokenize(text: &str) -> Vec<Tok<'_>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut out = Vec::new();
    for (ln, line) in text.lines().enumerate() {
        for sub in line.split('\r') {
            for w in sub.split_whitespace() {
                // braces are separate tokens even when glued to a word
                let mut last = 0;
                for (i, c) in w.char_indices() {
                    if c == '{' || c == '}' {
                        if i > last {
                            out.push(Tok { line: ln + 1, s: &w[last..i] });
                        }
                        out.push(Tok { line: ln + 1, s: &w[i..i + 1] });
                        last = i + 1;
                    }
                }
                if last < w.len() {
                    out.push(Tok { line: ln + 1, s: &w[last..] });
                }
            }
        }
    }
    out
}

struct Parser<'a> {
    t: Vec<Tok<'a>>,
    i: usize,
    joints: Vec<Joint>,
    nch: usize,
}

impl<'a> Parser<'a> {
    fn last_line(&self) -> usize {
        self.t.last().map_or(1, |t| t.line)
    }
    fn peek(&self) -> Option<&Tok<'a>> {
        self.t.get(self.i)
    }
    fn next(&mut self, ctx: &str) -> Result<(usize, &'a str), String> {
        match self.t.get(self.i) {
            Some(t) => {
                self.i += 1;
                Ok((t.line, t.s))
            }
            None => Err(format!("line {}: unexpected end of file while reading {ctx}", self.last_line())),
        }
    }
    fn expect(&mut self, word: &str) -> Result<usize, String> {
        let (l, s) = self.next(word)?;
        if s.eq_ignore_ascii_case(word) {
            Ok(l)
        } else {
            Err(format!("line {l}: expected '{word}', found '{s}'"))
        }
    }
    fn num(&mut self, ctx: &str) -> Result<f64, String> {
        let (l, s) = self.next(ctx)?;
        let v: f64 = s.parse().map_err(|_| format!("line {l}: '{s}' is not a number ({ctx})"))?;
        if !v.is_finite() {
            return Err(format!("line {l}: '{s}' is not a finite number ({ctx})"));
        }
        Ok(v)
    }
    fn offset(&mut self) -> Result<V3, String> {
        self.expect("OFFSET")?;
        Ok([self.num("OFFSET x")?, self.num("OFFSET y")?, self.num("OFFSET z")?])
    }
    fn joint(&mut self, parent: Option<usize>, kw_line: usize, depth: usize) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err(format!("line {kw_line}: hierarchy nested deeper than {MAX_DEPTH}"));
        }
        if self.joints.len() >= MAX_JOINTS {
            return Err(format!("line {kw_line}: more than {MAX_JOINTS} joints"));
        }
        let mut name = Vec::new();
        while let Some(t) = self.peek() {
            if t.line != kw_line || t.s == "{" || t.s == "}" {
                break;
            }
            name.push(t.s);
            self.i += 1;
        }
        if name.is_empty() {
            return Err(format!("line {kw_line}: joint has no name"));
        }
        let name = name.join(" ");
        let idx = self.joints.len();
        self.joints.push(Joint { name: name.clone(), parent, offset: [0.0; 3], channels: vec![], chan_start: 0, end_site: None, children: vec![] });
        if let Some(p) = parent {
            self.joints[p].children.push(idx);
        }
        self.expect("{")?;
        let mut have_offset = false;
        loop {
            let (l, s) = self.next(&format!("joint '{name}'"))?;
            match s.to_ascii_lowercase().as_str() {
                "offset" => {
                    self.i -= 1;
                    self.joints[idx].offset = self.offset()?;
                    have_offset = true;
                }
                "channels" => {
                    let (nl, ns) = self.next("channel count")?;
                    let n: usize = ns.parse().map_err(|_| format!("line {nl}: '{ns}' is not a channel count"))?;
                    if n > 24 {
                        return Err(format!("line {nl}: {n} channels on one joint is not plausible"));
                    }
                    let mut ch = Vec::with_capacity(n);
                    for _ in 0..n {
                        let (cl, cs) = self.next("channel name")?;
                        ch.push(Chan::parse(cs).ok_or_else(|| format!("line {cl}: unknown channel '{cs}'"))?);
                    }
                    self.joints[idx].chan_start = self.nch;
                    self.nch += n;
                    self.joints[idx].channels = ch;
                }
                "joint" => self.joint(Some(idx), l, depth + 1)?,
                "end" => {
                    let sl = self.expect("Site")?;
                    // optional name on the same line is ignored
                    while self.peek().is_some_and(|t| t.line == sl && t.s != "{") {
                        self.i += 1;
                    }
                    self.expect("{")?;
                    let off = self.offset()?;
                    self.expect("}")?;
                    self.joints[idx].end_site = Some(off);
                }
                "}" => {
                    if !have_offset {
                        return Err(format!("line {l}: joint '{name}' has no OFFSET"));
                    }
                    return Ok(());
                }
                _ => return Err(format!("line {l}: unexpected '{s}' inside joint '{name}'")),
            }
        }
    }
    /// `Frames: N` / `Frame Time: T`, tolerant of spacing around the colon.
    fn labeled(&mut self, words: &[&str]) -> Result<(usize, &'a str), String> {
        let mut value: Option<(usize, &'a str)> = None;
        for (wi, w) in words.iter().enumerate() {
            let (l, s) = self.next(&format!("'{}'", words.join(" ")))?;
            let lower = s.to_ascii_lowercase();
            if !lower.starts_with(w) {
                return Err(format!("line {l}: expected '{}:', found '{s}'", words.join(" ")));
            }
            let rest = &s[w.len()..];
            if wi + 1 == words.len() {
                let rest = rest.strip_prefix(':').unwrap_or(rest);
                if !rest.is_empty() {
                    value = Some((l, rest));
                } else if self.peek().is_some_and(|t| t.s.starts_with(':')) {
                    let (nl, ns) = self.next("value")?;
                    if ns.len() > 1 {
                        value = Some((nl, &ns[1..]));
                    }
                }
            }
        }
        match value {
            Some(v) => Ok(v),
            None => self.next("value"),
        }
    }
}

/// Parse BVH text.
pub fn parse_bvh(text: &str) -> Result<Bvh, String> {
    let mut p = Parser { t: tokenize(text), i: 0, joints: vec![], nch: 0 };
    if p.t.is_empty() {
        return Err("line 1: empty file".into());
    }
    p.expect("HIERARCHY")?;
    let mut roots = 0;
    loop {
        match p.peek() {
            None => return Err(format!("line {}: missing MOTION section", p.last_line())),
            Some(t) if t.s.eq_ignore_ascii_case("root") => {
                let l = t.line;
                p.i += 1;
                p.joint(None, l, 0)?;
                roots += 1;
            }
            Some(t) if t.s.eq_ignore_ascii_case("motion") => {
                if roots == 0 {
                    return Err(format!("line {}: no ROOT before MOTION", t.line));
                }
                p.i += 1;
                break;
            }
            Some(t) => return Err(format!("line {}: expected ROOT or MOTION, found '{}'", t.line, t.s)),
        }
    }
    let (fl, fv) = p.labeled(&["frames"])?;
    let frames: usize = fv.parse().map_err(|_| format!("line {fl}: '{fv}' is not a frame count"))?;
    if frames > MAX_FRAMES {
        return Err(format!("line {fl}: {frames} frames exceeds the limit of {MAX_FRAMES}"));
    }
    let (tl, tv) = p.labeled(&["frame", "time"])?;
    let frame_time: f64 = tv.parse().map_err(|_| format!("line {tl}: '{tv}' is not a frame time"))?;
    if !(frame_time.is_finite() && frame_time > 0.0) {
        return Err(format!("line {tl}: frame time must be positive, got '{tv}'"));
    }
    let nch = p.nch;
    let expected = frames.checked_mul(nch).filter(|&n| n <= MAX_VALUES).ok_or("motion data too large")?;
    let rest = &p.t[p.i..];
    if rest.len() < expected {
        let l = rest.last().map_or(tl, |t| t.line);
        return Err(format!("line {l}: motion data has {} values, expected {expected} ({frames} frames x {nch} channels)", rest.len()));
    }
    if rest.len() > expected {
        return Err(format!("line {}: extra data after {frames} frames", rest[expected].line));
    }
    let mut data = Vec::with_capacity(expected);
    for t in rest {
        let v: f64 = t.s.parse().map_err(|_| format!("line {}: '{}' is not a number", t.line, t.s))?;
        if !v.is_finite() {
            return Err(format!("line {}: '{}' is not a finite number", t.line, t.s));
        }
        data.push(v);
    }
    Ok(Bvh { joints: p.joints, frame_time, frames, num_channels: nch, data })
}

// ---------------------------------------------------------------------------------------------
// Forward kinematics
// ---------------------------------------------------------------------------------------------

type M = [[f64; 4]; 3];

fn mmul(a: &M, b: &M) -> M {
    let mut r = [[0.0; 4]; 3];
    for i in 0..3 {
        for j in 0..4 {
            let mut s = 0.0;
            for k in 0..3 {
                s += a[i][k] * b[k][j];
            }
            if j == 3 {
                s += a[i][3];
            }
            r[i][j] = s;
        }
    }
    r
}

fn trans(v: V3) -> M {
    [[1.0, 0.0, 0.0, v[0]], [0.0, 1.0, 0.0, v[1]], [0.0, 0.0, 1.0, v[2]]]
}

/// Right-handed elementary rotation about axis 0/1/2 by `deg` degrees.
fn rotm(axis: usize, deg: f64) -> M {
    let (s, c) = deg.to_radians().sin_cos();
    match axis {
        0 => [[1.0, 0.0, 0.0, 0.0], [0.0, c, -s, 0.0], [0.0, s, c, 0.0]],
        1 => [[c, 0.0, s, 0.0], [0.0, 1.0, 0.0, 0.0], [-s, 0.0, c, 0.0]],
        _ => [[c, -s, 0.0, 0.0], [s, c, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]],
    }
}

/// World positions of every joint head and End Site for one pose.
#[derive(Clone, Debug)]
pub struct Pose {
    pub pos: Vec<V3>,
    pub end: Vec<Option<V3>>,
}

/// Where a bone points: a joint, an End Site, or the parent direction continued.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tip {
    Joint(usize),
    End(usize),
    Extrap(usize),
}

fn dist3(a: V3, b: V3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

impl Bvh {
    pub fn fps(&self) -> f64 {
        1.0 / self.frame_time
    }
    pub fn duration(&self) -> f64 {
        self.frames.saturating_sub(1) as f64 * self.frame_time
    }
    pub fn joint_index(&self, name: &str) -> Option<usize> {
        self.joints.iter().position(|j| j.name == name)
    }

    /// Pose of a frame, or the rest pose (all channels zero) for `None`.
    pub fn pose(&self, frame: Option<usize>) -> Pose {
        let row: Option<&[f64]> = frame.filter(|&f| f < self.frames).map(|f| &self.data[f * self.num_channels..(f + 1) * self.num_channels]);
        let mut world: Vec<M> = Vec::with_capacity(self.joints.len());
        for j in &self.joints {
            let mut m = trans(j.offset);
            for (k, ch) in j.channels.iter().enumerate() {
                let v = row.map_or(0.0, |r| r[j.chan_start + k]);
                let c = match ch {
                    Chan::Xpos => trans([v, 0.0, 0.0]),
                    Chan::Ypos => trans([0.0, v, 0.0]),
                    Chan::Zpos => trans([0.0, 0.0, v]),
                    Chan::Xrot => rotm(0, v),
                    Chan::Yrot => rotm(1, v),
                    Chan::Zrot => rotm(2, v),
                };
                m = mmul(&m, &c);
            }
            let w = match j.parent {
                Some(p) => mmul(&world[p], &m),
                None => m,
            };
            world.push(w);
        }
        let pos = world.iter().map(|m| [m[0][3], m[1][3], m[2][3]]).collect();
        let end = self
            .joints
            .iter()
            .zip(&world)
            .map(|(j, w)| {
                j.end_site.map(|e| {
                    let m = mmul(w, &trans(e));
                    [m[0][3], m[1][3], m[2][3]]
                })
            })
            .collect();
        Pose { pos, end }
    }

    /// Default tip of a joint: its longest-reaching child, else its End Site, else the parent
    /// direction continued.
    pub fn default_tip(&self, j: usize) -> Tip {
        let reach = self.reach();
        self.default_tip_with(j, &reach)
    }

    fn reach(&self) -> Vec<f64> {
        let mut h = vec![0.0f64; self.joints.len()];
        for j in (0..self.joints.len()).rev() {
            let jt = &self.joints[j];
            let mut best = jt.end_site.map_or(0.0, |e| dist3(e, [0.0; 3]));
            for &c in &jt.children {
                best = best.max(dist3(self.joints[c].offset, [0.0; 3]) + h[c]);
            }
            h[j] = best;
        }
        h
    }

    fn default_tip_with(&self, j: usize, reach: &[f64]) -> Tip {
        let jt = &self.joints[j];
        let mut best: Option<(usize, f64)> = None;
        for &c in &jt.children {
            let r = dist3(self.joints[c].offset, [0.0; 3]) + reach[c];
            if best.is_none_or(|(_, br)| r > br + 1e-12) {
                best = Some((c, r));
            }
        }
        match best {
            Some((c, _)) => Tip::Joint(c),
            None if jt.end_site.is_some() => Tip::End(j),
            None => Tip::Extrap(j),
        }
    }

    pub fn tip_point(&self, pose: &Pose, tip: Tip) -> Option<V3> {
        match tip {
            Tip::Joint(c) => Some(pose.pos[c]),
            Tip::End(j) => pose.end[j],
            Tip::Extrap(j) => {
                let p = self.joints[j].parent?;
                let (a, b) = (pose.pos[p], pose.pos[j]);
                Some([2.0 * b[0] - a[0], 2.0 * b[1] - a[1], 2.0 * b[2] - a[2]])
            }
        }
    }

    /// Short human-readable description for `mocap-info`.
    pub fn summary(&self) -> String {
        let mut s = String::new();
        let n_end = self.joints.iter().filter(|j| j.end_site.is_some()).count();
        s.push_str(&format!("{} joints ({} end sites), {} channels per frame\n", self.joints.len(), n_end, self.num_channels));
        s.push_str(&format!("{} frames at {:.4} s ({:.3} fps), duration {:.3} s\n", self.frames, self.frame_time, self.fps(), self.duration()));
        fn walk(b: &Bvh, j: usize, depth: usize, s: &mut String) {
            let jt = &b.joints[j];
            let ch: Vec<&str> = jt
                .channels
                .iter()
                .map(|c| match c {
                    Chan::Xpos => "Xpos",
                    Chan::Ypos => "Ypos",
                    Chan::Zpos => "Zpos",
                    Chan::Xrot => "Xrot",
                    Chan::Yrot => "Yrot",
                    Chan::Zrot => "Zrot",
                })
                .collect();
            s.push_str(&format!("{}{}  offset ({:.3}, {:.3}, {:.3})  [{}]\n", "  ".repeat(depth), jt.name, jt.offset[0], jt.offset[1], jt.offset[2], ch.join(" ")));
            for &c in &jt.children {
                walk(b, c, depth + 1, s);
            }
        }
        for (i, j) in self.joints.iter().enumerate() {
            if j.parent.is_none() {
                walk(self, i, 0, &mut s);
            }
        }
        if self.frames > 0 {
            let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
            for f in 0..self.frames {
                for p in self.pose(Some(f)).pos {
                    for k in 0..3 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
            }
            s.push_str(&format!("joint extent over the clip: x {:.2}..{:.2}, y {:.2}..{:.2}, z {:.2}..{:.2}\n", lo[0], hi[0], lo[1], hi[1], lo[2], hi[2]));
        }
        s
    }
}

// ---------------------------------------------------------------------------------------------
// Projection to 2D
// ---------------------------------------------------------------------------------------------

/// How the 3D skeleton is flattened. BVH is y-up; the result uses stage coordinates (y down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum View {
    /// Looking at a character that faces +Z: stage x = x.
    Front,
    /// stage x = z (a character facing +Z faces right).
    Side,
    /// Seen from above: stage x = x, stage y = -z (forward is up the screen).
    Top,
    /// Front view after turning the skeleton by this many degrees about the vertical axis.
    Yaw(f64),
}

impl View {
    pub fn parse(s: &str) -> Result<View, String> {
        match s {
            "front" => Ok(View::Front),
            "side" => Ok(View::Side),
            "top" => Ok(View::Top),
            _ => {
                let d = s.strip_prefix("yaw=").or_else(|| s.strip_prefix("yaw:")).ok_or_else(|| format!("unknown view '{s}' (front, side, top or yaw=DEG)"))?;
                let v: f64 = d.parse().map_err(|_| format!("bad yaw angle '{d}'"))?;
                if !v.is_finite() {
                    return Err(format!("bad yaw angle '{d}'"));
                }
                Ok(View::Yaw(v))
            }
        }
    }
    pub fn project(&self, p: V3) -> [f64; 2] {
        match *self {
            View::Front => [p[0], -p[1]],
            View::Side => [p[2], -p[1]],
            View::Top => [p[0], -p[2]],
            View::Yaw(d) => {
                let (s, c) = d.to_radians().sin_cos();
                [p[0] * c + p[2] * s, -p[1]]
            }
        }
    }
}

/// Angle in degrees (stage convention, y down, so positive turns clockwise on screen) of the
/// bone `a -> b` after projection. `None` when the bone points (nearly) along the view axis.
pub fn angle_2d(a: V3, b: V3, view: View, mirror: bool) -> Option<f64> {
    let (pa, pb) = (view.project(a), view.project(b));
    let (mut dx, dy) = (pb[0] - pa[0], pb[1] - pa[1]);
    if mirror {
        dx = -dx;
    }
    let l3 = dist3(a, b);
    let l2 = (dx * dx + dy * dy).sqrt();
    if l3 < 1e-9 || l2 < 0.05 * l3 {
        return None;
    }
    Some(dy.atan2(dx).to_degrees())
}

/// Per-frame 2D data for the whole BVH.
#[derive(Clone, Debug)]
pub struct Projected {
    pub frame_time: f64,
    /// `points[frame][joint]`
    pub points: Vec<Vec<[f64; 2]>>,
    /// `angles[frame][joint]`: angle of the bone from the joint to its default tip.
    pub angles: Vec<Vec<Option<f64>>>,
}

pub fn project_motion(b: &Bvh, view: View) -> Projected {
    let reach = b.reach();
    let tips: Vec<Tip> = (0..b.joints.len()).map(|j| b.default_tip_with(j, &reach)).collect();
    let mut points = Vec::with_capacity(b.frames);
    let mut angles = Vec::with_capacity(b.frames);
    for f in 0..b.frames {
        let pose = b.pose(Some(f));
        points.push(pose.pos.iter().map(|&p| view.project(p)).collect());
        angles.push((0..b.joints.len()).map(|j| b.tip_point(&pose, tips[j]).and_then(|t| angle_2d(pose.pos[j], t, view, false))).collect());
    }
    Projected { frame_time: b.frame_time, points, angles }
}

// ---------------------------------------------------------------------------------------------
// Names: normalisation and role classification
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Hips,
    Spine,
    Neck,
    Head,
    Clavicle,
    UpperArm,
    ForeArm,
    Hand,
    Thigh,
    Shin,
    Foot,
    Toe,
    Finger,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tag {
    pub role: Role,
    pub side: Option<Side>,
    pub conf: f32,
}

const NOISE: &[&str] = &["bip", "biped", "mixamorig", "def", "org", "mch", "jnt", "joint", "bone", "bn", "sk", "skel", "skeleton", "rig", "armature", "cc", "base", "char", "character"];
const LEFT: &[&str] = &["left", "l", "lt", "lft"];
const RIGHT: &[&str] = &["right", "r", "rt", "rgt"];
const UPW: &[&str] = &["up", "upper", "upr"];
const LOWW: &[&str] = &["low", "lower", "lwr", "bottom"];
const THIGH_W: &[&str] = &["thigh", "femur", "upleg", "upperleg", "uplegs", "upperlegs"];
const SHIN_W: &[&str] = &["shin", "calf", "tibia", "lowleg", "lowerleg", "lowerlegs"];
const UPARM_W: &[&str] = &["upperarm", "uparm", "humerus"];
const FORE_W: &[&str] = &["forearm", "lowarm", "lowerarm", "radius"];
const HAND_W: &[&str] = &["hand", "hands", "wrist", "palm"];
const CLAV_W: &[&str] = &["shoulder", "clavicle", "collar", "scapula", "clav"];
const FOOT_W: &[&str] = &["foot", "feet", "ankle"];
const TOE_W: &[&str] = &["toe", "toes", "toebase", "ball"];
const HIPS_W: &[&str] = &["hips", "hip", "pelvis", "pelvic", "root", "cog"];
const SPINE_W: &[&str] = &["spine", "chest", "abdomen", "torso", "back", "waist", "stomach", "thorax", "trunk", "belly", "lowerback", "upperback", "ribcage", "sternum"];
const NECK_W: &[&str] = &["neck", "lowerneck", "upperneck"];
const HEAD_W: &[&str] = &["head", "skull", "cranium"];
const FINGER_W: &[&str] = &["thumb", "index", "middle", "ring", "pinky", "pinkie", "little", "finger", "fingers", "digit"];
const OTHER_W: &[&str] = &[
    "jaw", "eye", "eyes", "eyelid", "brow", "ear", "nose", "tongue", "teeth", "hair", "cheek", "lip", "lips", "mouth", "face", "tail", "wing", "cape", "skirt", "cloth", "breast", "twist", "roll",
    "corrective", "helper", "end", "site", "nub", "tip", "null", "dummy", "prop", "weapon", "gun", "sword", "ik", "pole", "target", "ctrl", "control",
];

fn in_vocab(t: &str) -> bool {
    [THIGH_W, SHIN_W, UPARM_W, FORE_W, HAND_W, CLAV_W, FOOT_W, TOE_W, HIPS_W, SPINE_W, NECK_W, HEAD_W, FINGER_W, OTHER_W, UPW, LOWW, &["arm", "arms", "leg", "legs"]].iter().any(|l| l.contains(&t))
}

/// Split a bone name into lower-case tokens: namespaces (`mixamorig:`, `Armature|`) dropped,
/// camelCase, digits and separators split, glued side prefixes (`lthigh`, `lefthand`) separated
/// and rig-prefix noise (`Bip01`, `DEF-`) removed. Numeric tokens are kept (as digit strings).
pub fn normalize_name(name: &str) -> Vec<String> {
    let base = name.rsplit([':', '|']).next().unwrap_or(name);
    let chars: Vec<char> = base.chars().collect();
    let mut raw: Vec<String> = Vec::new();
    let mut cur = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                raw.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if let Some(&pc) = cur.chars().last().as_ref() {
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            let boundary = (pc.is_lowercase() && c.is_uppercase())
                || (pc.is_alphabetic() != c.is_alphabetic())
                || (pc.is_uppercase() && c.is_uppercase() && next_lower);
            if boundary {
                raw.push(std::mem::take(&mut cur));
            }
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        raw.push(cur);
    }
    let mut out: Vec<String> = Vec::new();
    let mut prev_noise = false;
    for t in raw.into_iter().map(|t| t.to_lowercase()) {
        let numeric = t.chars().all(|c| c.is_ascii_digit());
        if numeric {
            if prev_noise {
                continue; // Bip01, mixamorig1
            }
            out.push(t);
            continue;
        }
        if NOISE.contains(&t.as_str()) {
            prev_noise = true;
            continue;
        }
        prev_noise = false;
        // glued side prefix: lthigh, lefthand
        if !in_vocab(&t) && !LEFT.contains(&t.as_str()) && !RIGHT.contains(&t.as_str()) {
            let mut done = false;
            for p in ["left", "right", "l", "r"] {
                if let Some(rest) = t.strip_prefix(p) {
                    if rest.len() >= 2 && in_vocab(rest) {
                        out.push(p.to_string());
                        out.push(rest.to_string());
                        done = true;
                        break;
                    }
                }
            }
            if done {
                continue;
            }
        }
        out.push(t);
    }
    out
}

fn any_of(ws: &[String], list: &[&str]) -> bool {
    ws.iter().any(|w| list.contains(&w.as_str()))
}
fn has(ws: &[String], w: &str) -> bool {
    ws.iter().any(|x| x == w)
}

/// Guess the anatomical role of a bone from its name alone.
pub fn classify(name: &str) -> Tag {
    let toks = normalize_name(name);
    let mut side = None;
    let mut index: Option<u32> = None;
    let mut ws: Vec<String> = Vec::new();
    let non_side = toks.iter().filter(|t| !LEFT.contains(&t.as_str()) && !RIGHT.contains(&t.as_str())).count();
    for t in &toks {
        if t.chars().all(|c| c.is_ascii_digit()) {
            index = t.parse().ok();
        } else if LEFT.contains(&t.as_str()) && non_side > 0 {
            side = Some(Side::Left);
        } else if RIGHT.contains(&t.as_str()) && non_side > 0 {
            side = Some(Side::Right);
        } else {
            ws.push(t.clone());
        }
    }
    let _ = index;
    let t = |role, conf| Tag { role, side, conf };
    let other = Tag { role: Role::Other, side, conf: 0.0 };
    if ws.is_empty() || any_of(&ws, OTHER_W) {
        return other;
    }
    if any_of(&ws, FINGER_W) {
        return t(Role::Finger, 0.9);
    }
    if any_of(&ws, TOE_W) {
        return t(Role::Toe, if has(&ws, "ball") { 0.7 } else { 0.95 });
    }
    if any_of(&ws, FOOT_W) {
        return t(Role::Foot, if has(&ws, "ankle") { 0.85 } else { 0.95 });
    }
    if any_of(&ws, THIGH_W) {
        return t(Role::Thigh, if has(&ws, "femur") { 0.95 } else { 1.0 });
    }
    if any_of(&ws, SHIN_W) {
        return t(Role::Shin, if has(&ws, "tibia") { 0.95 } else { 1.0 });
    }
    if has(&ws, "leg") || has(&ws, "legs") {
        if any_of(&ws, UPW) {
            return t(Role::Thigh, 0.95);
        }
        if any_of(&ws, LOWW) {
            return t(Role::Shin, 0.95);
        }
        return t(Role::Shin, 0.55);
    }
    if any_of(&ws, UPARM_W) {
        return t(Role::UpperArm, 1.0);
    }
    if any_of(&ws, FORE_W) || (has(&ws, "fore") && (has(&ws, "arm") || has(&ws, "arms"))) {
        return t(Role::ForeArm, 1.0);
    }
    if has(&ws, "arm") || has(&ws, "arms") {
        if any_of(&ws, UPW) {
            return t(Role::UpperArm, 0.95);
        }
        if any_of(&ws, LOWW) {
            return t(Role::ForeArm, 0.95);
        }
        return t(Role::UpperArm, 0.6);
    }
    if any_of(&ws, HAND_W) {
        return t(Role::Hand, if has(&ws, "palm") { 0.6 } else { 0.95 });
    }
    if any_of(&ws, CLAV_W) {
        return t(Role::Clavicle, 0.85);
    }
    if any_of(&ws, NECK_W) {
        return t(Role::Neck, 1.0);
    }
    if any_of(&ws, HEAD_W) {
        return t(Role::Head, 1.0);
    }
    if has(&ws, "hips") {
        return t(Role::Hips, 1.0);
    }
    if has(&ws, "pelvis") || has(&ws, "pelvic") {
        return t(Role::Hips, 0.95);
    }
    if has(&ws, "hip") {
        return if side.is_some() { t(Role::Thigh, 0.5) } else { t(Role::Hips, 0.9) };
    }
    if has(&ws, "root") {
        return t(Role::Hips, 0.55);
    }
    if has(&ws, "cog") {
        return t(Role::Hips, 0.5);
    }
    if any_of(&ws, SPINE_W) {
        return t(Role::Spine, if has(&ws, "spine") { 1.0 } else { 0.85 });
    }
    other
}

// ---------------------------------------------------------------------------------------------
// Skeleton abstraction shared by BVH and rig, slot assignment
// ---------------------------------------------------------------------------------------------

struct Skel {
    names: Vec<String>,
    parent: Vec<Option<usize>>,
    children: Vec<Vec<usize>>,
    /// Rest positions, y up.
    pos: Vec<V3>,
    depth: Vec<usize>,
    /// Lateral axis (0 = x, 2 = z) and the sign that points to the character's left.
    lat_axis: usize,
    lat_sign: f64,
    lat_fixed: bool,
}

impl Skel {
    fn new(names: Vec<String>, parent: Vec<Option<usize>>, pos: Vec<V3>, lat_fixed: bool) -> Skel {
        let n = names.len();
        let mut children = vec![vec![]; n];
        let mut depth = vec![0; n];
        for i in 0..n {
            if let Some(p) = parent[i] {
                children[p].push(i);
                depth[i] = depth[p] + 1;
            }
        }
        Skel { names, parent, children, pos, depth, lat_axis: 0, lat_sign: 1.0, lat_fixed }
    }
    fn from_bvh(b: &Bvh) -> Skel {
        let pose = b.pose(None);
        Skel::new(b.joints.iter().map(|j| j.name.clone()).collect(), b.joints.iter().map(|j| j.parent).collect(), pose.pos, false)
    }
    fn from_rig(p: &Project) -> Result<Skel, String> {
        let w = crate::rigedit::rest_world(p)?;
        let idx: BTreeMap<&str, usize> = p.bones.iter().enumerate().map(|(i, b)| (b.name.as_str(), i)).collect();
        let parent = p.bones.iter().map(|b| b.parent.as_deref().and_then(|n| idx.get(n).copied())).collect();
        Ok(Skel::new(p.bones.iter().map(|b| b.name.clone()).collect(), parent, w.iter().map(|(h, _)| [h.x as f64, -(h.y as f64), 0.0]).collect(), true))
    }
    fn lat(&self, j: usize) -> f64 {
        self.pos[j][self.lat_axis] * self.lat_sign
    }
    /// Lateral coordinate of the subtree point farthest sideways from `origin` (signed).
    fn lat_ext(&self, c: usize, origin: usize) -> f64 {
        let o = self.lat(origin);
        self.subtree(c).iter().map(|&q| self.lat(q)).fold(o, |best, v| if (v - o).abs() > (best - o).abs() { v } else { best })
    }
    fn subtree(&self, j: usize) -> Vec<usize> {
        let mut out = vec![j];
        let mut i = 0;
        while i < out.len() {
            let c = out[i];
            out.extend(self.children[c].iter().copied());
            i += 1;
        }
        out
    }
    /// Single-child descent from `j`, including `j`; stops at a leaf or a branching joint (included).
    fn chain(&self, j: usize) -> Vec<usize> {
        let mut out = vec![j];
        let mut c = j;
        while self.children[c].len() == 1 {
            c = self.children[c][0];
            out.push(c);
        }
        out
    }
    fn seg(&self, chain: &[usize], i: usize) -> f64 {
        if i + 1 < chain.len() {
            dist3(self.pos[chain[i]], self.pos[chain[i + 1]])
        } else {
            0.0
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Via {
    Name,
    Topology,
    Manual,
}

impl Via {
    fn label(self) -> &'static str {
        match self {
            Via::Name => "name",
            Via::Topology => "topology",
            Via::Manual => "manual",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Assign {
    joint: usize,
    conf: f32,
    via: Via,
}

#[derive(Clone, Debug, Default)]
struct Slots {
    single: BTreeMap<String, Assign>,
    spine: Vec<Assign>,
}

fn slot_key(role: Role, side: Option<Side>) -> Option<String> {
    let nm = match role {
        Role::Hips => return Some("hips".into()),
        Role::Neck => return Some("neck".into()),
        Role::Head => return Some("head".into()),
        Role::Clavicle => "clavicle",
        Role::UpperArm => "upperarm",
        Role::ForeArm => "forearm",
        Role::Hand => "hand",
        Role::Thigh => "thigh",
        Role::Shin => "shin",
        Role::Foot => "foot",
        Role::Toe => "toe",
        _ => return None,
    };
    Some(format!("{nm}.{}", if side? == Side::Left { "l" } else { "r" }))
}

fn name_slots(s: &Skel) -> Slots {
    let mut out = Slots::default();
    let mut spine: Vec<Assign> = Vec::new();
    let mut tags: Vec<Tag> = s.names.iter().map(|n| classify(n)).collect();
    // context: a bare `Arm` that has a forearm child, or a bare `Leg` under a thigh, is certain
    let base = tags.clone();
    // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
    #[allow(clippy::needless_range_loop)]
    for j in 0..tags.len() {
        let child_is = |r: Role| s.children[j].iter().any(|&c| base[c].role == r);
        let parent_is = |r: Role| s.parent[j].is_some_and(|q| base[q].role == r);
        if tags[j].role == Role::UpperArm && tags[j].conf < 0.9 && child_is(Role::ForeArm) {
            tags[j].conf = 0.9;
        }
        if tags[j].role == Role::Shin && tags[j].conf < 0.9 && parent_is(Role::Thigh) {
            tags[j].conf = 0.9;
        }
    }
    // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
    #[allow(clippy::needless_range_loop)]
    for j in 0..s.names.len() {
        let tag = tags[j];
        if tag.conf <= 0.0 {
            continue;
        }
        if tag.role == Role::Spine {
            spine.push(Assign { joint: j, conf: tag.conf, via: Via::Name });
            continue;
        }
        let Some(key) = slot_key(tag.role, tag.side) else { continue };
        let a = Assign { joint: j, conf: tag.conf, via: Via::Name };
        match out.single.get(&key) {
            Some(old) if (old.conf, std::cmp::Reverse(s.depth[old.joint])) >= (a.conf, std::cmp::Reverse(s.depth[a.joint])) => {}
            _ => {
                out.single.insert(key, a);
            }
        }
    }
    spine.sort_by_key(|a| (s.depth[a.joint], a.joint));
    out.spine = spine;
    out
}

fn topo_slots(s: &mut Skel, hips_hint: Option<usize>) -> Slots {
    let mut out = Slots::default();
    let n = s.names.len();
    if n == 0 {
        return out;
    }
    // hips: hint, else the first branching joint below the root
    let hips = match hips_hint {
        Some(h) => h,
        None => {
            let roots: Vec<usize> = (0..n).filter(|&i| s.parent[i].is_none()).collect();
            let Some(&r) = roots.iter().max_by_key(|&&r| (s.subtree(r).len(), std::cmp::Reverse(r))) else { return out };
            let mut c = r;
            while s.children[c].len() == 1 {
                c = s.children[c][0];
            }
            if s.children[c].len() < 2 {
                return out; // a single chain has no pelvis to find
            }
            c
        }
    };
    let tc = 0.6f32;
    let ts = 0.55f32;
    out.single.insert("hips".into(), Assign { joint: hips, conf: tc, via: Via::Topology });
    let hy = s.pos[hips][1];
    let mut legs: Vec<(usize, f64)> = Vec::new();
    let mut up: Vec<(usize, f64)> = Vec::new();
    for &c in &s.children[hips] {
        let st = s.subtree(c);
        let down = hy - st.iter().map(|&j| s.pos[j][1]).fold(f64::MAX, f64::min);
        let upv = st.iter().map(|&j| s.pos[j][1]).fold(f64::MIN, f64::max) - hy;
        if down > upv {
            legs.push((c, down));
        } else {
            up.push((c, upv));
        }
    }
    legs.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    up.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    // lateral axis of this skeleton from the leg separation
    let total_h = {
        let (lo, hi) = s.pos.iter().fold((f64::MAX, f64::MIN), |(l, h), p| (l.min(p[1]), h.max(p[1])));
        (hi - lo).max(1e-9)
    };
    let mut sided = false;
    if legs.len() >= 2 {
        let (a, b) = (legs[0].0, legs[1].0);
        if !s.lat_fixed {
            let ext = |c: usize, ax: usize| s.subtree(c).iter().map(|&q| s.pos[q][ax] - s.pos[hips][ax]).fold(0.0f64, |m, v| if v.abs() > m.abs() { v } else { m });
            let dx = (ext(a, 0) - ext(b, 0)).abs();
            let dz = (ext(a, 2) - ext(b, 2)).abs();
            if dz > dx {
                s.lat_axis = 2;
                s.lat_sign = -1.0; // facing +x: left is -z
            }
        }
        let (la, lb) = (s.lat_ext(a, hips), s.lat_ext(b, hips));
        sided = (la - lb).abs() > 0.01 * total_h;
        if sided {
            let (l, r) = if la > lb { (a, b) } else { (b, a) };
            for (c, side) in [(l, "l"), (r, "r")] {
                let ch = s.chain(c);
                for (k, role) in ["thigh", "shin", "foot", "toe"].iter().enumerate() {
                    if let Some(&j) = ch.get(k) {
                        out.single.insert(format!("{role}.{side}"), Assign { joint: j, conf: ts, via: Via::Topology });
                    }
                }
            }
        }
    }
    if let Some(&(sp, _)) = up.first() {
        // spine up to the joint where neck and arms branch
        let mut chain = vec![sp];
        let mut c = sp;
        while s.children[c].len() == 1 {
            c = s.children[c][0];
            chain.push(c);
        }
        let top = *chain.last().unwrap();
        let kids = s.children[top].clone();
        if kids.len() >= 3 {
            let spread = |j: usize| s.subtree(j).iter().map(|&q| (s.lat(q) - s.lat(top)).abs()).fold(0.0, f64::max);
            let mut ranked: Vec<(usize, f64)> = kids.iter().map(|&k| (k, spread(k))).collect();
            ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
            let arms = [ranked[0].0, ranked[1].0];
            let neck_c = ranked[2..].iter().max_by(|a, b| s.pos[a.0][1].total_cmp(&s.pos[b.0][1]).then(b.0.cmp(&a.0))).map(|x| x.0);
            out.spine = chain.iter().map(|&j| Assign { joint: j, conf: ts, via: Via::Topology }).collect();
            if let Some(nc) = neck_c {
                let nch = s.chain(nc);
                if nch.len() >= 2 {
                    out.single.insert("neck".into(), Assign { joint: nch[0], conf: ts, via: Via::Topology });
                    out.single.insert("head".into(), Assign { joint: nch[1], conf: ts, via: Via::Topology });
                } else {
                    out.single.insert("head".into(), Assign { joint: nch[0], conf: ts, via: Via::Topology });
                }
            }
            let (l0, l1) = (s.lat_ext(arms[0], top), s.lat_ext(arms[1], top));
            if (l0 - l1).abs() > 0.01 * total_h {
                let (l, r) = if l0 > l1 { (arms[0], arms[1]) } else { (arms[1], arms[0]) };
                for (c, side) in [(l, "l"), (r, "r")] {
                    let ch = s.chain(c);
                    let shift = usize::from(ch.len() >= 4 && s.seg(&ch, 0) < 0.5 * s.seg(&ch, 1));
                    if shift == 1 {
                        out.single.insert(format!("clavicle.{side}"), Assign { joint: ch[0], conf: ts, via: Via::Topology });
                    }
                    for (k, role) in ["upperarm", "forearm", "hand"].iter().enumerate() {
                        if let Some(&j) = ch.get(k + shift) {
                            out.single.insert(format!("{role}.{side}"), Assign { joint: j, conf: ts, via: Via::Topology });
                        }
                    }
                }
            }
        } else {
            out.spine = chain.iter().map(|&j| Assign { joint: j, conf: ts, via: Via::Topology }).collect();
        }
    }
    let _ = sided;
    out
}

fn merge_slots(name: Slots, topo: Slots) -> Slots {
    let mut out = name;
    let mut used: BTreeSet<usize> = out.single.values().map(|a| a.joint).chain(out.spine.iter().map(|a| a.joint)).collect();
    for (k, a) in topo.single {
        if !out.single.contains_key(&k) && !used.contains(&a.joint) {
            used.insert(a.joint);
            out.single.insert(k, a);
        }
    }
    if out.spine.is_empty() {
        out.spine = topo.spine.into_iter().filter(|a| !used.contains(&a.joint)).collect();
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Matching
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct BoneMatch {
    /// Canonical slot (`hips`, `spine.0`, `upperarm.l` ...) or `manual`.
    pub slot: String,
    pub bvh: usize,
    pub rig: usize,
    pub bvh_name: String,
    pub rig_name: String,
    pub confidence: f32,
    /// `name`, `topology` or `manual`.
    pub via: &'static str,
    /// Rig bone length / BVH bone length at rest.
    pub length_ratio: Option<f64>,
    pub note: String,
    pub tip: Tip,
}

#[derive(Clone, Debug, Default)]
pub struct MatchResult {
    pub matches: Vec<BoneMatch>,
    pub unmatched_bvh: Vec<String>,
    pub unmatched_rig: Vec<String>,
    pub warnings: Vec<String>,
}

/// Manual override: BVH joint name -> rig bone name, or `None` to exclude that joint.
pub type ManualMap = BTreeMap<String, Option<String>>;

pub fn parse_map(json: &str) -> Result<ManualMap, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("bad mapping JSON: {e}"))?;
    let obj = v.as_object().ok_or("mapping must be a JSON object {\"bvh joint\": \"rig bone\"}")?;
    let obj = match (obj.len(), obj.get("map").or_else(|| obj.get("bones"))) {
        (1, Some(serde_json::Value::Object(inner))) => inner,
        _ => obj,
    };
    let mut out = ManualMap::new();
    for (k, val) in obj {
        match val {
            serde_json::Value::String(s) if !s.is_empty() => out.insert(k.clone(), Some(s.clone())),
            serde_json::Value::Null => out.insert(k.clone(), None),
            serde_json::Value::String(_) => out.insert(k.clone(), None),
            _ => return Err(format!("mapping for '{k}' must be a bone name or null")),
        };
    }
    Ok(out)
}

fn median(v: &mut [f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    Some(if n % 2 == 1 { v[n / 2] } else { 0.5 * (v[n / 2 - 1] + v[n / 2]) })
}

fn find_joint(b: &Bvh, name: &str) -> Option<usize> {
    b.joint_index(name).or_else(|| {
        let l = name.to_lowercase();
        let hits: Vec<usize> = (0..b.joints.len()).filter(|&i| b.joints[i].name.to_lowercase() == l).collect();
        (hits.len() == 1).then(|| hits[0])
    })
}

/// Match BVH joints to rig bones. See the module docs for the strategy.
pub fn match_bones(bvh: &Bvh, p: &Project, manual: Option<&ManualMap>) -> Result<MatchResult, String> {
    if p.bones.is_empty() {
        return Err("project has no bones".into());
    }
    let mut sb = Skel::from_bvh(bvh);
    let mut sr = Skel::from_rig(p)?;
    let nb = name_slots(&sb);
    let nr = name_slots(&sr);
    let tb = topo_slots(&mut sb, nb.single.get("hips").map(|a| a.joint));
    let tr = topo_slots(&mut sr, nr.single.get("hips").map(|a| a.joint));
    let ab = merge_slots(nb, tb);
    let ar = merge_slots(nr, tr);

    let mut pairs: Vec<(String, Assign, Assign)> = Vec::new();
    let order: Vec<String> = {
        let mut o = vec!["hips".to_string()];
        o.push("spine".into());
        o.extend(["neck", "head"].iter().map(|s| s.to_string()));
        for side in ["l", "r"] {
            for r in ["clavicle", "upperarm", "forearm", "hand"] {
                o.push(format!("{r}.{side}"));
            }
        }
        for side in ["l", "r"] {
            for r in ["thigh", "shin", "foot", "toe"] {
                o.push(format!("{r}.{side}"));
            }
        }
        o
    };
    for key in &order {
        if key == "spine" {
            let (m, n) = (ab.spine.len(), ar.spine.len());
            if m == 0 || n == 0 {
                continue;
            }
            let k = m.min(n);
            let pick = |len: usize, i: usize| -> usize {
                if k == 1 {
                    0
                } else {
                    ((i as f64) * (len as f64 - 1.0) / (k as f64 - 1.0)).round() as usize
                }
            };
            for i in 0..k {
                let (mut a, mut r) = (ab.spine[pick(m, i)], ar.spine[pick(n, i)]);
                if m != n {
                    a.conf *= 0.85;
                    r.conf *= 0.85;
                }
                pairs.push((format!("spine.{i}"), a, r));
            }
        } else if let (Some(a), Some(r)) = (ab.single.get(key), ar.single.get(key)) {
            pairs.push((key.clone(), *a, *r));
        }
    }

    let mut matches: Vec<BoneMatch> = pairs
        .into_iter()
        .map(|(slot, a, r)| BoneMatch {
            slot,
            bvh: a.joint,
            rig: r.joint,
            bvh_name: bvh.joints[a.joint].name.clone(),
            rig_name: p.bones[r.joint].name.clone(),
            confidence: a.conf.min(r.conf),
            via: if a.via == Via::Name && r.via == Via::Name { "name" } else { "topology" },
            length_ratio: None,
            note: String::new(),
            tip: Tip::Extrap(a.joint),
        })
        .collect();

    // manual overrides
    let mut warnings = Vec::new();
    if let Some(map) = manual {
        let mut seen_rig: BTreeMap<usize, String> = BTreeMap::new();
        let mut errs = Vec::new();
        let mut ov: Vec<(usize, Option<usize>)> = Vec::new();
        for (bn, rn) in map {
            let Some(bj) = find_joint(bvh, bn) else {
                errs.push(format!("mapping: no BVH joint '{bn}'"));
                continue;
            };
            let rj = match rn {
                None => None,
                Some(rn) => match p.bones.iter().position(|b| &b.name == rn) {
                    Some(i) => {
                        if let Some(prev) = seen_rig.insert(i, bn.clone()) {
                            errs.push(format!("mapping: rig bone '{rn}' is the target of both '{prev}' and '{bn}'"));
                        }
                        Some(i)
                    }
                    None => {
                        errs.push(format!("mapping: no rig bone '{rn}'"));
                        continue;
                    }
                },
            };
            ov.push((bj, rj));
        }
        if !errs.is_empty() {
            return Err(errs.join("; "));
        }
        // a manual match keeps the canonical slot of the BVH joint (hips, spine.N, upperarm.l, ...): the slot decides the
        // tip, root travel and the "no hips match" warning, so `Hips=bone` must stay the hips
        let bvh_slot: BTreeMap<usize, String> = ab.single.iter().map(|(k, a)| (a.joint, k.clone())).collect();
        for (bj, rj) in ov {
            let inherited = matches.iter().find(|m| m.bvh == bj).map(|m| m.slot.clone()).filter(|sl| sl != "manual").or_else(|| bvh_slot.get(&bj).cloned());
            matches.retain(|m| m.bvh != bj && Some(m.rig) != rj);
            if let Some(rj) = rj {
                let slot = match inherited {
                    Some(sl) if !matches.iter().any(|m| m.slot == sl) => sl,
                    _ => "manual".to_string(),
                };
                matches.push(BoneMatch {
                    slot,
                    bvh: bj,
                    rig: rj,
                    bvh_name: bvh.joints[bj].name.clone(),
                    rig_name: p.bones[rj].name.clone(),
                    confidence: 1.0,
                    via: Via::Manual.label(),
                    length_ratio: None,
                    note: "manual".into(),
                    tip: Tip::Extrap(bj),
                });
            }
        }
    }

    // tips: next matched slot in the chain, else the default tip
    let reach = bvh.reach();
    let by_slot: BTreeMap<String, usize> = matches.iter().map(|m| (m.slot.clone(), m.bvh)).collect();
    let nspine = matches.iter().filter(|m| m.slot.starts_with("spine.")).count();
    for m in matches.iter_mut() {
        let next: Vec<String> = match m.slot.split_once('.') {
            _ if m.slot == "hips" => (0..nspine).map(|i| format!("spine.{i}")).chain(["neck".to_string(), "head".to_string()]).collect(),
            Some(("spine", i)) => {
                let i: usize = i.parse().unwrap_or(0);
                ((i + 1)..nspine).map(|k| format!("spine.{k}")).chain(["neck".to_string(), "head".to_string()]).collect()
            }
            _ if m.slot == "neck" => vec!["head".into()],
            Some(("clavicle", s)) => vec![format!("upperarm.{s}")],
            Some(("upperarm", s)) => vec![format!("forearm.{s}")],
            Some(("forearm", s)) => vec![format!("hand.{s}")],
            Some(("thigh", s)) => vec![format!("shin.{s}")],
            Some(("shin", s)) => vec![format!("foot.{s}")],
            Some(("foot", s)) => vec![format!("toe.{s}")],
            _ => vec![],
        };
        let mut tip = None;
        for k in next {
            if let Some(&j) = by_slot.get(&k) {
                if j != m.bvh && bvh.joints.len() > j {
                    let mut c = bvh.joints[j].parent;
                    let mut desc = false;
                    while let Some(x) = c {
                        if x == m.bvh {
                            desc = true;
                            break;
                        }
                        c = bvh.joints[x].parent;
                    }
                    if desc {
                        tip = Some(Tip::Joint(j));
                        break;
                    }
                }
            }
        }
        m.tip = tip.unwrap_or_else(|| bvh.default_tip_with(m.bvh, &reach));
    }

    // length ratios and sanity penalty
    let rest = bvh.pose(None);
    for m in matches.iter_mut() {
        if let Some(t) = bvh.tip_point(&rest, m.tip) {
            let bl = dist3(rest.pos[m.bvh], t);
            let rl = p.bones[m.rig].length as f64;
            if bl > 1e-9 && rl > 1e-9 {
                m.length_ratio = Some(rl / bl);
            }
        }
    }
    let mut limb: Vec<f64> = matches.iter().filter(|m| m.via != "manual" && ["upperarm", "forearm", "thigh", "shin"].iter().any(|r| m.slot.starts_with(r))).filter_map(|m| m.length_ratio).collect();
    if let Some(med) = median(&mut limb) {
        for m in matches.iter_mut() {
            if m.via == "manual" || !["upperarm", "forearm", "thigh", "shin"].iter().any(|r| m.slot.starts_with(r)) {
                continue;
            }
            if let Some(r) = m.length_ratio {
                let dev = (r / med).max(med / r);
                if dev > 3.0 {
                    m.confidence *= 0.5;
                    m.note = format!("limb length ratio is {dev:.1}x off the median");
                } else if dev > 2.0 {
                    m.confidence *= 0.7;
                    m.note = format!("limb length ratio is {dev:.1}x off the median");
                }
            }
        }
    }

    let mb: BTreeSet<usize> = matches.iter().map(|m| m.bvh).collect();
    let mr: BTreeSet<usize> = matches.iter().map(|m| m.rig).collect();
    let unmatched_bvh = (0..bvh.joints.len()).filter(|i| !mb.contains(i)).map(|i| bvh.joints[i].name.clone()).collect();
    let unmatched_rig = (0..p.bones.len()).filter(|i| !mr.contains(i)).map(|i| p.bones[i].name.clone()).collect();
    if !matches.iter().any(|m| m.slot == "hips") {
        warnings.push("no hips match: root motion will not be transferred".into());
    }
    for side in ["l", "r"] {
        for r in ["upperarm", "thigh"] {
            let k = format!("{r}.{side}");
            if !matches.iter().any(|m| m.slot == k) {
                warnings.push(format!("no match for {k}"));
            }
        }
    }
    if matches.is_empty() {
        warnings.push("no bones matched: supply a --map file".into());
    }
    Ok(MatchResult { matches, unmatched_bvh, unmatched_rig, warnings })
}

/// Plain-text match table.
pub fn format_table(r: &MatchResult) -> String {
    let mut s = String::new();
    s.push_str(&format!("{:<12} {:<26} {:<22} {:>5}  {:<9} {:>7}  {}\n", "slot", "bvh joint", "rig bone", "conf", "via", "len x", "note"));
    for m in &r.matches {
        s.push_str(&format!(
            "{:<12} {:<26} {:<22} {:>5.2}  {:<9} {:>7}  {}\n",
            m.slot,
            m.bvh_name,
            m.rig_name,
            m.confidence,
            m.via,
            m.length_ratio.map_or("-".to_string(), |v| format!("{v:.2}")),
            m.note
        ));
    }
    if !r.unmatched_bvh.is_empty() {
        s.push_str(&format!("unmatched BVH joints: {}\n", r.unmatched_bvh.join(", ")));
    }
    if !r.unmatched_rig.is_empty() {
        s.push_str(&format!("unmatched rig bones: {}\n", r.unmatched_rig.join(", ")));
    }
    for w in &r.warnings {
        s.push_str(&format!("warning: {w}\n"));
    }
    s
}

// ---------------------------------------------------------------------------------------------
// Retargeting
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scale {
    /// Rig leg length / BVH leg length.
    Auto,
    /// Rig units per BVH unit.
    Fixed(f64),
}

#[derive(Clone, Debug)]
pub struct RetargetOptions {
    pub view: View,
    pub scale: Scale,
    /// Output frame rate; `None` = project fps.
    pub fps: Option<f64>,
    /// Moving-average radius in output frames (0 = off).
    pub smooth: usize,
    /// First / last BVH frame (inclusive).
    pub start: Option<usize>,
    pub end: Option<usize>,
    /// BVH frame whose pose is the rest pose; `None` = the zero-rotation hierarchy pose.
    pub rest_frame: Option<usize>,
    /// Flip the figure left-right on the stage.
    pub mirror: bool,
    pub root_motion: bool,
    /// Drop horizontal root travel (keep the bob).
    pub in_place: bool,
    pub weight: f32,
    /// Matches below this confidence are ignored.
    pub min_confidence: f32,
    /// Root controller bone and stage ground y for [`crate::footlock`].
    pub foot_lock: Option<(String, f32)>,
}

impl Default for RetargetOptions {
    fn default() -> Self {
        RetargetOptions { view: View::Front, scale: Scale::Auto, fps: None, smooth: 0, start: None, end: None, rest_frame: None, mirror: false, root_motion: true, in_place: false, weight: 1.0, min_confidence: 0.4, foot_lock: None }
    }
}

#[derive(Clone, Debug)]
pub struct RetargetResult {
    pub channels: Vec<Channel>,
    /// `target.prop` of channels not written because they are locked.
    pub blocked: Vec<String>,
    /// Frames of animation produced.
    pub out_frames: usize,
    /// Project length needed (>= out_frames); `bake` extends the project to this.
    pub project_frames: u32,
    pub scale: f64,
    pub bones_used: usize,
    /// Source frames where some bone pointed along the view axis and held its last angle.
    pub degenerate: usize,
    pub notes: Vec<String>,
}

fn norm360(a: f64) -> f64 {
    let mut x = a % 360.0;
    if x > 180.0 {
        x -= 360.0;
    } else if x <= -180.0 {
        x += 360.0;
    }
    x
}

fn lerp_at(v: &[f64], x: f64) -> f64 {
    let i0 = (x.floor().max(0.0) as usize).min(v.len() - 1);
    let i1 = (i0 + 1).min(v.len() - 1);
    let t = (x - i0 as f64).clamp(0.0, 1.0);
    v[i0] + (v[i1] - v[i0]) * t
}

fn smooth_series(v: &mut [f64], radius: usize) {
    let n = v.len();
    if radius == 0 || n < 3 {
        return;
    }
    let src = v.to_vec();
    for i in 0..n {
        let r = radius.min(i).min(n - 1 - i);
        let sum: f64 = src[i - r..=i + r].iter().sum();
        v[i] = sum / (2 * r + 1) as f64;
    }
}

pub fn retarget(p: &Project, bvh: &Bvh, mr: &MatchResult, o: &RetargetOptions) -> Result<RetargetResult, String> {
    if bvh.frames == 0 {
        return Err("BVH has no frames".into());
    }
    let fps = o.fps.unwrap_or(p.fps as f64);
    if !(fps.is_finite() && fps > 0.0 && fps <= 1000.0) {
        return Err("fps must be in (0, 1000]".into());
    }
    if o.smooth > 1000 {
        return Err("smooth radius must be at most 1000".into());
    }
    if !(o.weight.is_finite() && (0.0..=1.0).contains(&o.weight)) {
        return Err("weight must be in 0..1".into());
    }
    if let Scale::Fixed(s) = o.scale {
        if !(s.is_finite() && s > 0.0) {
            return Err("scale must be positive".into());
        }
    }
    let s0 = o.start.unwrap_or(0);
    let e0 = o.end.unwrap_or(bvh.frames - 1);
    if s0 > e0 || e0 >= bvh.frames {
        return Err(format!("frame range {s0}..{e0} does not fit the {} BVH frames", bvh.frames));
    }
    if let Some(rf) = o.rest_frame {
        if rf >= bvh.frames {
            return Err(format!("rest frame {rf} is outside the {} BVH frames", bvh.frames));
        }
    }
    let used: Vec<&BoneMatch> = mr.matches.iter().filter(|m| m.confidence >= o.min_confidence && m.rig < p.bones.len() && m.bvh < bvh.joints.len()).collect();
    if used.is_empty() {
        return Err("no bone matches to retarget (all below the confidence threshold?)".into());
    }
    let mut notes = Vec::new();
    let nsel = e0 - s0 + 1;
    let rest_pose = bvh.pose(o.rest_frame);
    let rest_ang: Vec<Option<f64>> = used.iter().map(|m| bvh.tip_point(&rest_pose, m.tip).and_then(|t| angle_2d(rest_pose.pos[m.bvh], t, o.view, o.mirror))).collect();
    for (m, r) in used.iter().zip(&rest_ang) {
        if r.is_none() {
            notes.push(format!("{}: the rest pose bone points along the view axis; this bone stays at its rest angle", m.rig_name));
        }
    }
    // per-frame deltas
    let mut series: Vec<Vec<f64>> = vec![Vec::with_capacity(nsel); used.len()];
    let mut prev: Vec<Option<f64>> = vec![None; used.len()];
    let hips_match = used.iter().find(|m| m.slot == "hips");
    let mut hip_xy: Vec<[f64; 2]> = Vec::with_capacity(nsel);
    let mut degenerate = 0usize;
    let proj = |pt: V3| {
        let mut q = o.view.project(pt);
        if o.mirror {
            q[0] = -q[0];
        }
        q
    };
    for f in s0..=e0 {
        let pose = bvh.pose(Some(f));
        for (i, m) in used.iter().enumerate() {
            let ang = bvh.tip_point(&pose, m.tip).and_then(|t| angle_2d(pose.pos[m.bvh], t, o.view, o.mirror));
            let d = match (ang, rest_ang[i]) {
                (Some(a), Some(r)) => {
                    let mut d = norm360(a - r);
                    if let Some(pv) = prev[i] {
                        while d - pv > 180.0 {
                            d -= 360.0;
                        }
                        while d - pv < -180.0 {
                            d += 360.0;
                        }
                    }
                    prev[i] = Some(d);
                    d
                }
                (None, Some(_)) => {
                    degenerate += 1;
                    prev[i].unwrap_or(0.0)
                }
                _ => 0.0,
            };
            series[i].push(d);
        }
        if let Some(h) = hips_match {
            hip_xy.push(proj(pose.pos[h.bvh]));
        }
    }
    if degenerate > 0 {
        notes.push(format!("{degenerate} bone-frames pointed along the view axis and held their last angle (2D projection loses depth)"));
    }
    let hip_ref = match (hips_match, o.rest_frame) {
        (Some(h), Some(_)) => Some(proj(rest_pose.pos[h.bvh])),
        (Some(_), None) => hip_xy.first().copied(),
        _ => None,
    };

    // scale
    let scale = match o.scale {
        Scale::Fixed(s) => s,
        Scale::Auto => {
            let mut ratios = Vec::new();
            for (a, b) in [("thigh", "shin"), ("upperarm", "forearm")] {
                for side in ["l", "r"] {
                    let (ka, kb) = (format!("{a}.{side}"), format!("{b}.{side}"));
                    if let (Some(ma), Some(mb)) = (used.iter().find(|m| m.slot == ka), used.iter().find(|m| m.slot == kb)) {
                        let tip_b = bvh.tip_point(&rest_pose, mb.tip);
                        let bl = dist3(rest_pose.pos[ma.bvh], rest_pose.pos[mb.bvh]) + tip_b.map_or(0.0, |t| dist3(rest_pose.pos[mb.bvh], t));
                        let rl = (p.bones[ma.rig].length + p.bones[mb.rig].length) as f64;
                        if bl > 1e-9 && rl > 1e-9 {
                            ratios.push(rl / bl);
                        }
                    }
                }
                if !ratios.is_empty() {
                    break; // legs preferred; arms only as a fallback
                }
            }
            match median(&mut ratios) {
                Some(s) => s,
                None => {
                    notes.push("no matched limb to derive the root-travel scale from; using 1.0 (pass --scale)".into());
                    1.0
                }
            }
        }
    };

    // resample to output frames
    let span = (e0 - s0) as f64;
    let out_n = if e0 == s0 { 1 } else { (span * bvh.frame_time * fps + 1e-6).floor() as usize + 1 };
    if out_n > 1_000_000 {
        return Err("resulting animation would be longer than 1,000,000 frames".into());
    }
    let step = 1.0 / (fps * bvh.frame_time);
    let pos_of = |k: usize| (k as f64 * step).min(span);
    let mut dl: Vec<Vec<f64>> = series.iter().map(|s| (0..out_n).map(|k| lerp_at(s, pos_of(k))).collect()).collect();
    for d in dl.iter_mut() {
        smooth_series(d, o.smooth);
    }
    let (mut hx, mut hy) = (vec![0.0; out_n], vec![0.0; out_n]);
    if let (Some(r), true) = (hip_ref, hips_match.is_some()) {
        let xs: Vec<f64> = hip_xy.iter().map(|q| q[0] - r[0]).collect();
        let ys: Vec<f64> = hip_xy.iter().map(|q| q[1] - r[1]).collect();
        for k in 0..out_n {
            hx[k] = lerp_at(&xs, pos_of(k)) * scale;
            hy[k] = lerp_at(&ys, pos_of(k)) * scale;
        }
        smooth_series(&mut hx, o.smooth);
        smooth_series(&mut hy, o.smooth);
        if o.in_place {
            hx.iter_mut().for_each(|v| *v = 0.0);
        }
    }

    // rig-space angles
    let rw = crate::rigedit::rest_world(p)?;
    let nbn = p.bones.len();
    let parent: Vec<Option<usize>> = p.bones.iter().map(|b| b.parent.as_deref().and_then(|n| p.bones.iter().position(|x| x.name == n))).collect();
    let mut mi_of: Vec<Option<usize>> = vec![None; nbn];
    for (i, m) in used.iter().enumerate() {
        mi_of[m.rig] = Some(i);
    }
    let mut local: Vec<Vec<f32>> = vec![Vec::with_capacity(out_n); nbn];
    let mut prev_local: Vec<Option<f64>> = vec![None; nbn];
    let mut root_off: Vec<[f32; 2]> = Vec::with_capacity(out_n);
    let hips_rig = hips_match.map(|h| h.rig);
    for k in 0..out_n {
        let mut wa = vec![0.0f64; nbn];
        for b in 0..nbn {
            let pw = parent[b].map_or(0.0, |q| wa[q]);
            let rest_local = p.bones[b].rest_angle as f64;
            let d = match mi_of[b] {
                Some(mi) => {
                    let mut d = norm360(rw[b].1 as f64 + dl[mi][k] - pw - rest_local);
                    if let Some(pv) = prev_local[b] {
                        while d - pv > 180.0 {
                            d -= 360.0;
                        }
                        while d - pv < -180.0 {
                            d += 360.0;
                        }
                    }
                    prev_local[b] = Some(d);
                    d
                }
                None => 0.0,
            };
            if mi_of[b].is_some() {
                local[b].push(d as f32);
            }
            wa[b] = pw + rest_local + d;
        }
        if let (true, Some(hb)) = (o.root_motion, hips_rig) {
            let pw = parent[hb].map_or(0.0, |q| wa[q]).to_radians();
            let (s, c) = pw.sin_cos();
            let (dx, dy) = (hx[k], hy[k]);
            root_off.push([(dx * c + dy * s) as f32, (-dx * s + dy * c) as f32]);
        }
    }

    // channels
    let key = |vals: &[f32]| -> Track {
        let ks: Vec<(f32, f32, Ease)> = vals.iter().enumerate().map(|(k, &v)| (k as f32, v, Ease::Linear)).collect();
        Track::with_keys(&ks)
    };
    let mut chans: Vec<Channel> = Vec::new();
    for b in 0..nbn {
        if mi_of[b].is_some() {
            chans.push(Channel::keys(&p.bones[b].name, Prop::BoneAngle, key(&local[b])).with_weight(o.weight));
        }
    }
    if let (true, Some(hb)) = (o.root_motion, hips_rig) {
        let xs: Vec<f32> = root_off.iter().map(|v| v[0]).collect();
        let ys: Vec<f32> = root_off.iter().map(|v| v[1]).collect();
        chans.push(Channel::keys(&p.bones[hb].name, Prop::BoneOffsetX, key(&xs)).with_weight(o.weight));
        chans.push(Channel::keys(&p.bones[hb].name, Prop::BoneOffsetY, key(&ys)).with_weight(o.weight));
    } else if o.root_motion && hips_rig.is_none() {
        notes.push("no hips match: root travel not transferred".into());
    }
    let bones_used = used.len();
    let (mut chans, blocked) = crate::chanmeta::split_unlocked(p, chans);
    if !blocked.is_empty() {
        notes.push(format!("locked channels skipped: {}", blocked.join(", ")));
    }
    let project_frames = p.frames.max(out_n as u32);
    if project_frames > p.frames {
        notes.push(format!("project length extended from {} to {} frames", p.frames, project_frames));
    }
    if let Some((bone, ground)) = &o.foot_lock {
        let mut tmp = p.clone();
        let base = tmp.channels.len();
        tmp.channels.extend(chans.iter().cloned());
        tmp.frames = project_frames;
        let lo = crate::footlock::LockOptions { ground_y: *ground, ..Default::default() };
        let contacts = crate::footlock::lock_feet(&mut tmp, bone, &lo)?;
        let extra: Vec<Channel> = tmp.channels[base + chans.len()..].to_vec();
        notes.push(format!("foot lock on '{bone}': {} contact(s) pinned", contacts.len()));
        chans.extend(extra);
    }
    Ok(RetargetResult { channels: chans, blocked, out_frames: out_n, project_frames, scale, bones_used, degenerate, notes })
}

/// Append the retargeted channels (and extend the project length if the clip is longer).
pub fn bake(p: &mut Project, r: &RetargetResult) {
    p.channels.extend(r.channels.iter().cloned());
    p.frames = p.frames.max(r.project_frames);
}

/// The same change as one editor operation, so a single undo removes the whole retarget.
pub fn bake_op(p: &Project, r: &RetargetResult) -> Result<Op, String> {
    let mut ops = Vec::new();
    if r.project_frames > p.frames {
        ops.push(Op::Set { path: "/frames".into(), value: serde_json::json!(r.project_frames) });
    }
    for c in &r.channels {
        ops.push(Op::Insert { path: "/channels/-".into(), value: serde_json::to_value(c).map_err(|e| e.to_string())? });
    }
    Ok(Op::Batch { ops })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;
    use crate::math::Vec2;
    use crate::project::Bone;

    // ---------------------------------------------------------------- synthetic fixtures

    /// A stick figure joint: `role` identifies it for animation, `name` is what the file calls it.
    struct J {
        name: String,
        role: &'static str,
        off: V3,
        kids: Vec<J>,
        end: Option<V3>,
    }

    fn j(name: &str, role: &'static str, off: V3, kids: Vec<J>, end: Option<V3>) -> J {
        J { name: name.into(), role, off, kids, end }
    }

    /// Names for the 20-joint figure (3 spine joints, clavicles, 4-joint legs).
    struct Names {
        hips: &'static str,
        spine: [&'static str; 3],
        neck: &'static str,
        head: &'static str,
        clav: [&'static str; 2],
        uarm: [&'static str; 2],
        farm: [&'static str; 2],
        hand: [&'static str; 2],
        thigh: [&'static str; 2],
        shin: [&'static str; 2],
        foot: [&'static str; 2],
        toe: [&'static str; 2],
    }

    const MIXAMO: Names = Names {
        hips: "Hips",
        spine: ["Spine", "Spine1", "Spine2"],
        neck: "Neck",
        head: "Head",
        clav: ["LeftShoulder", "RightShoulder"],
        uarm: ["LeftArm", "RightArm"],
        farm: ["LeftForeArm", "RightForeArm"],
        hand: ["LeftHand", "RightHand"],
        thigh: ["LeftUpLeg", "RightUpLeg"],
        shin: ["LeftLeg", "RightLeg"],
        foot: ["LeftFoot", "RightFoot"],
        toe: ["LeftToeBase", "RightToeBase"],
    };
    const MIXAMO_NS: Names = Names {
        hips: "mixamorig:Hips",
        spine: ["mixamorig:Spine", "mixamorig:Spine1", "mixamorig:Spine2"],
        neck: "mixamorig:Neck",
        head: "mixamorig:Head",
        clav: ["mixamorig:LeftShoulder", "mixamorig:RightShoulder"],
        uarm: ["mixamorig:LeftArm", "mixamorig:RightArm"],
        farm: ["mixamorig:LeftForeArm", "mixamorig:RightForeArm"],
        hand: ["mixamorig:LeftHand", "mixamorig:RightHand"],
        thigh: ["mixamorig:LeftUpLeg", "mixamorig:RightUpLeg"],
        shin: ["mixamorig:LeftLeg", "mixamorig:RightLeg"],
        foot: ["mixamorig:LeftFoot", "mixamorig:RightFoot"],
        toe: ["mixamorig:LeftToeBase", "mixamorig:RightToeBase"],
    };
    const CMU: Names = Names {
        hips: "Hips",
        spine: ["lowerback", "upperback", "thorax"],
        neck: "lowerneck",
        head: "head",
        clav: ["lclavicle", "rclavicle"],
        uarm: ["lhumerus", "rhumerus"],
        farm: ["lradius", "rradius"],
        hand: ["lwrist", "rwrist"],
        thigh: ["lfemur", "rfemur"],
        shin: ["ltibia", "rtibia"],
        foot: ["lfoot", "rfoot"],
        toe: ["ltoes", "rtoes"],
    };
    const BIPED: Names = Names {
        hips: "Bip01 Pelvis",
        spine: ["Bip01 Spine", "Bip01 Spine1", "Bip01 Spine2"],
        neck: "Bip01 Neck",
        head: "Bip01 Head",
        clav: ["Bip01 L Clavicle", "Bip01 R Clavicle"],
        uarm: ["Bip01 L UpperArm", "Bip01 R UpperArm"],
        farm: ["Bip01 L Forearm", "Bip01 R Forearm"],
        hand: ["Bip01 L Hand", "Bip01 R Hand"],
        thigh: ["Bip01 L Thigh", "Bip01 R Thigh"],
        shin: ["Bip01 L Calf", "Bip01 R Calf"],
        foot: ["Bip01 L Foot", "Bip01 R Foot"],
        toe: ["Bip01 L Toe0", "Bip01 R Toe0"],
    };
    const ANON: Names = Names {
        hips: "j00",
        spine: ["j01", "j02", "j03"],
        neck: "j04",
        head: "j05",
        clav: ["j06", "j10"],
        uarm: ["j07", "j11"],
        farm: ["j08", "j12"],
        hand: ["j09", "j13"],
        thigh: ["j14", "j18"],
        shin: ["j15", "j19"],
        foot: ["j16", "j20"],
        toe: ["j17", "j21"],
    };

    /// T-pose figure facing +Z, y up, character-left = +X. Total leg 82, arm 50.
    fn figure(n: &Names) -> J {
        let arm = |s: usize, sg: f64| {
            j(n.clav[s], if s == 0 { "clavL" } else { "clavR" }, [3.0 * sg, 8.0, 0.0], vec![
                j(n.uarm[s], if s == 0 { "uarmL" } else { "uarmR" }, [8.0 * sg, 0.0, 0.0], vec![
                    j(n.farm[s], if s == 0 { "farmL" } else { "farmR" }, [26.0 * sg, 0.0, 0.0], vec![
                        j(n.hand[s], if s == 0 { "handL" } else { "handR" }, [24.0 * sg, 0.0, 0.0], vec![], Some([8.0 * sg, 0.0, 0.0])),
                    ], None),
                ], None),
            ], None)
        };
        let leg = |s: usize, sg: f64| {
            j(n.thigh[s], if s == 0 { "thighL" } else { "thighR" }, [8.0 * sg, -4.0, 0.0], vec![
                j(n.shin[s], if s == 0 { "shinL" } else { "shinR" }, [0.0, -42.0, 0.0], vec![
                    j(n.foot[s], if s == 0 { "footL" } else { "footR" }, [0.0, -40.0, 0.0], vec![
                        j(n.toe[s], if s == 0 { "toeL" } else { "toeR" }, [0.0, -6.0, 10.0], vec![], Some([0.0, 0.0, 6.0])),
                    ], None),
                ], None),
            ], None)
        };
        j(n.hips, "hips", [0.0, 0.0, 0.0], vec![
            j(n.spine[0], "sp0", [0.0, 10.0, 0.0], vec![
                j(n.spine[1], "sp1", [0.0, 12.0, 0.0], vec![
                    j(n.spine[2], "sp2", [0.0, 12.0, 0.0], vec![
                        j(n.neck, "neck", [0.0, 12.0, 0.0], vec![j(n.head, "head", [0.0, 8.0, 0.0], vec![], Some([0.0, 12.0, 0.0]))], None),
                        arm(0, 1.0),
                        arm(1, -1.0),
                    ], None),
                ], None),
            ], None),
            leg(0, 1.0),
            leg(1, -1.0),
        ], None)
    }

    fn emit(n: &J, depth: usize, root: bool, out: &mut String, eol: &str) {
        let pad = "  ".repeat(depth);
        out.push_str(&format!("{pad}{} {}{eol}{pad}{{{eol}", if root { "ROOT" } else { "JOINT" }, n.name));
        out.push_str(&format!("{pad}  OFFSET {} {} {}{eol}", n.off[0], n.off[1], n.off[2]));
        if root {
            out.push_str(&format!("{pad}  CHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation{eol}"));
        } else {
            out.push_str(&format!("{pad}  CHANNELS 3 Zrotation Xrotation Yrotation{eol}"));
        }
        for k in &n.kids {
            emit(k, depth + 1, false, out, eol);
        }
        if let Some(e) = n.end {
            out.push_str(&format!("{pad}  End Site{eol}{pad}  {{{eol}{pad}    OFFSET {} {} {}{eol}{pad}  }}{eol}", e[0], e[1], e[2]));
        }
        out.push_str(&format!("{pad}}}{eol}"));
    }

    fn row(n: &J, f: &dyn Fn(&str) -> [f64; 3], root_pos: V3, root: bool, out: &mut Vec<f64>) {
        if root {
            out.extend(root_pos);
        }
        out.extend(f(n.role));
        for k in &n.kids {
            row(k, f, root_pos, false, out);
        }
    }

    /// BVH text of `frames` frames of `dt` seconds; `rot(role, frame)` = (zrot, xrot, yrot) degrees,
    /// `pos(frame)` = hips position.
    fn make_bvh(n: &Names, frames: usize, dt: f64, rot: &dyn Fn(&str, usize) -> [f64; 3], pos: &dyn Fn(usize) -> V3, eol: &str) -> String {
        let fig = figure(n);
        let mut s = format!("HIERARCHY{eol}");
        emit(&fig, 0, true, &mut s, eol);
        s.push_str(&format!("MOTION{eol}Frames: {frames}{eol}Frame Time: {dt}{eol}"));
        for f in 0..frames {
            let mut r = Vec::new();
            row(&fig, &|role| rot(role, f), pos(f), true, &mut r);
            s.push_str(&r.iter().map(|v| format!("{v}")).collect::<Vec<_>>().join(" "));
            s.push_str(eol);
        }
        s
    }

    fn still(_: &str, _: usize) -> [f64; 3] {
        [0.0; 3]
    }
    fn at_origin(_: usize) -> V3 {
        [0.0, 100.0, 0.0]
    }

    /// Names for the rig side.
    struct RigNames {
        hips: &'static str,
        spine: [&'static str; 2],
        neck: &'static str,
        head: &'static str,
        clav: [&'static str; 2],
        uarm: [&'static str; 2],
        farm: [&'static str; 2],
        hand: [&'static str; 2],
        thigh: [&'static str; 2],
        shin: [&'static str; 2],
        foot: [&'static str; 2],
        toe: [&'static str; 2],
    }
    const BLENDER: RigNames = RigNames {
        hips: "pelvis",
        spine: ["spine", "chest"],
        neck: "neck",
        head: "head",
        clav: ["shoulder.L", "shoulder.R"],
        uarm: ["upper_arm.L", "upper_arm.R"],
        farm: ["forearm.L", "forearm.R"],
        hand: ["hand.L", "hand.R"],
        thigh: ["thigh.L", "thigh.R"],
        shin: ["shin.L", "shin.R"],
        foot: ["foot.L", "foot.R"],
        toe: ["toe.L", "toe.R"],
    };
    const RIG_ANON: RigNames = RigNames {
        hips: "b00",
        spine: ["b01", "b02"],
        neck: "b03",
        head: "b04",
        clav: ["b05", "b09"],
        uarm: ["b06", "b10"],
        farm: ["b07", "b11"],
        hand: ["b08", "b12"],
        thigh: ["b13", "b17"],
        shin: ["b14", "b18"],
        foot: ["b15", "b19"],
        toe: ["b16", "b20"],
    };

    /// Front-facing 2D rig (stage y down): character-left on +x. `k` scales all lengths.
    fn rig(n: &RigNames, k: f32) -> Project {
        let mut p = Project::new("rig", 800.0, 600.0, 30.0, 40);
        let b = |name: &str, parent: Option<&str>, len: f32, off: (f32, f32), rest: f32| Bone::new(name, parent, len * k, Vec2::new(off.0 * k, off.1 * k), rest);
        p.bones.push(Bone::new(n.hips, None, 15.0 * k, Vec2::new(300.0, 300.0), -90.0));
        p.bones.push(b(n.spine[0], Some(n.hips), 30.0, (15.0, 0.0), 0.0));
        p.bones.push(b(n.spine[1], Some(n.spine[0]), 30.0, (30.0, 0.0), 0.0));
        p.bones.push(b(n.neck, Some(n.spine[1]), 10.0, (30.0, 0.0), 0.0));
        p.bones.push(b(n.head, Some(n.neck), 25.0, (10.0, 0.0), 0.0));
        for (s, sg) in [(0usize, 1.0f32), (1, -1.0)] {
            p.bones.push(b(n.clav[s], Some(n.spine[1]), 15.0, (25.0, 0.0), 90.0 * sg));
            let cl = p.bones.len() - 1;
            // left clavicle points to +x (world 0): local +90 under a -90 parent; right: world 180 -> local -90
            p.bones[cl].rest_angle = 90.0 * sg;
            p.bones.push(b(n.uarm[s], Some(n.clav[s]), 40.0, (15.0, 0.0), 0.0));
            p.bones.push(b(n.farm[s], Some(n.uarm[s]), 35.0, (40.0, 0.0), 0.0));
            p.bones.push(b(n.hand[s], Some(n.farm[s]), 15.0, (35.0, 0.0), 0.0));
        }
        for (s, sg) in [(0usize, 1.0f32), (1, -1.0)] {
            p.bones.push(b(n.thigh[s], Some(n.hips), 45.0, (0.0, 8.0 * sg), 180.0));
            p.bones.push(b(n.shin[s], Some(n.thigh[s]), 40.0, (45.0, 0.0), 0.0));
            p.bones.push(b(n.foot[s], Some(n.shin[s]), 8.0, (40.0, 0.0), 0.0));
            p.bones.push(b(n.toe[s], Some(n.foot[s]), 10.0, (8.0, 0.0), -90.0));
        }
        p
    }

    fn world_angle(e: &Engine, frame: f32, bone: usize) -> f64 {
        let w = e.evaluate(frame).world[bone];
        (w.b as f64).atan2(w.a as f64).to_degrees()
    }

    fn ang_diff(a: f64, b: f64) -> f64 {
        norm360(a - b).abs()
    }

    fn bone_idx(p: &Project, n: &str) -> usize {
        p.bones.iter().position(|b| b.name == n).unwrap_or_else(|| panic!("no bone {n}"))
    }

    fn pair<'a>(m: &'a MatchResult, bvh: &str, rig: &str) -> &'a BoneMatch {
        m.matches.iter().find(|x| x.bvh_name == bvh).unwrap_or_else(|| panic!("{bvh} unmatched\n{}", format_table(m))).clone_ref(rig)
    }
    trait CloneRef {
        fn clone_ref(&self, rig: &str) -> &Self;
    }
    impl CloneRef for BoneMatch {
        fn clone_ref(&self, rig: &str) -> &Self {
            assert_eq!(self.rig_name, rig, "{} matched to {} (slot {})", self.bvh_name, self.rig_name, self.slot);
            self
        }
    }

    // ---------------------------------------------------------------- parser

    #[test]
    fn parses_figure_and_reports_structure() {
        let b = parse_bvh(&make_bvh(&MIXAMO, 3, 0.04, &still, &at_origin, "\n")).unwrap();
        assert_eq!(b.joints.len(), 22);
        assert_eq!(b.frames, 3);
        assert_eq!(b.num_channels, 6 + 21 * 3);
        assert!((b.fps() - 25.0).abs() < 1e-9);
        assert_eq!(b.joints[0].parent, None);
        assert_eq!(b.joints[b.joint_index("LeftHand").unwrap()].end_site, Some([8.0, 0.0, 0.0]));
        let s = b.summary();
        assert!(s.contains("22 joints") && s.contains("LeftForeArm") && s.contains("25.000 fps"), "{s}");
    }

    #[test]
    fn tolerates_crlf_tabs_glued_braces_case_and_spaced_names() {
        let src = "HIERARCHY\r\nROOT Hips{\r\n\tOFFSET 0 0 0\r\n\tCHANNELS 3 Xposition Yposition Zposition\r\n\tJOINT Left Hip Joint\r\n\t{\r\n\t\tOFFSET 1 0 0\r\n\t\tCHANNELS 1 zrotation\r\n\t\tEnd Site\r\n\t\t{\r\n\t\t\tOFFSET 0 1 0\r\n\t\t}\r\n\t}\r\n}\r\nMOTION\r\nFrames:\t2\r\nFrame Time:\t0.5\r\n1 2 3 90\r\n\r\n4 5 6 0\r\n";
        let b = parse_bvh(src).unwrap();
        assert_eq!(b.joints[1].name, "Left Hip Joint");
        assert_eq!(b.frames, 2);
        assert_eq!(b.data, vec![1.0, 2.0, 3.0, 90.0, 4.0, 5.0, 6.0, 0.0]);
        // glued label forms and a BOM
        let src2 = format!("\u{feff}{}", src.replace("Frames:\t2", "Frames:2").replace("Frame Time:\t0.5", "Frame Time :0.5"));
        assert_eq!(parse_bvh(&src2).unwrap().frames, 2);
    }

    #[test]
    fn rotation_order_follows_channel_order() {
        let mk = |ch: &str| {
            format!("HIERARCHY\nROOT A\n{{\nOFFSET 0 0 0\nCHANNELS 2 {ch}\nJOINT B\n{{\nOFFSET 1 0 0\nCHANNELS 0\nEnd Site\n{{\nOFFSET 1 0 0\n}}\n}}\n}}\nMOTION\nFrames: 1\nFrame Time: 1\n90 90\n")
        };
        // Rx * Ry applied to (1,0,0): Ry first -> (0,0,-1), then Rx(90) -> (0,1,0)
        let a = parse_bvh(&mk("Xrotation Yrotation")).unwrap().pose(Some(0));
        let q = a.pos[1];
        assert!((q[0]).abs() < 1e-12 && (q[1] - 1.0).abs() < 1e-12 && (q[2]).abs() < 1e-12, "{q:?}");
        // Ry * Rx: Rx leaves (1,0,0), Ry(90) sends it to (0,0,-1)
        let b = parse_bvh(&mk("Yrotation Xrotation")).unwrap().pose(Some(0));
        let q = b.pos[1];
        assert!((q[0]).abs() < 1e-12 && (q[1]).abs() < 1e-12 && (q[2] + 1.0).abs() < 1e-12, "{q:?}");
        // End Site follows the same transform: B is at (0,1,0), the end site adds Rx*Ry*(1,0,0)
        let e = a.end[1].unwrap();
        assert!((e[1] - 2.0).abs() < 1e-12, "{e:?}");
        // zrotation 90 sends +x to +y; position channels interleaved with rotations
        let c = parse_bvh("HIERARCHY\nROOT A\n{\nOFFSET 0 0 0\nCHANNELS 4 Zrotation Xposition Zrotation Yposition\nJOINT B\n{\nOFFSET 1 0 0\nCHANNELS 0\nEnd Site\n{\nOFFSET 1 0 0\n}\n}\n}\nMOTION\nFrames: 1\nFrame Time: 1\n90 2 -90 5\n").unwrap().pose(Some(0));
        // M = Rz(90) T(2,0,0) Rz(-90) T(0,5,0); child at M*(1,0,0) = Rz(90)(T(2,0,0)(Rz(-90)(1,5,0)))
        // Rz(-90)(1,5,0) = (5,-1,0); +2 -> (7,-1,0); Rz(90) -> (1,7,0)
        assert!((c.pos[1][0] - 1.0).abs() < 1e-12 && (c.pos[1][1] - 7.0).abs() < 1e-12, "{:?}", c.pos[1]);
    }

    #[test]
    fn nested_hierarchy_and_multiple_roots() {
        let src = "HIERARCHY\nROOT A\n{\nOFFSET 0 0 0\nCHANNELS 0\nJOINT B\n{\nOFFSET 1 0 0\nCHANNELS 0\nJOINT C\n{\nOFFSET 1 0 0\nCHANNELS 0\nEnd Site\n{\nOFFSET 1 0 0\n}\n}\n}\n}\nROOT D\n{\nOFFSET 0 5 0\nCHANNELS 1 Zrotation\nEnd Site\n{\nOFFSET 0 1 0\n}\n}\nMOTION\nFrames: 1\nFrame Time: 0.1\n0\n";
        let b = parse_bvh(src).unwrap();
        assert_eq!(b.joints.len(), 4);
        assert_eq!(b.joints[2].parent, Some(1));
        assert_eq!(b.joints[3].parent, None);
        assert_eq!(b.pose(Some(0)).pos[2], [2.0, 0.0, 0.0]);
    }

    #[test]
    fn errors_carry_line_numbers() {
        let good = make_bvh(&MIXAMO, 2, 0.04, &still, &at_origin, "\n");
        let lines: Vec<&str> = good.lines().collect();
        let last = lines.len();
        // bad channel name
        let bad = good.replacen("Zrotation", "Wrotation", 1);
        let e = parse_bvh(&bad).unwrap_err();
        assert!(e.starts_with("line 5:") && e.contains("Wrotation"), "{e}");
        // bad number in the data
        let bad = good.replacen("0 100 0", "0 abc 0", 1);
        let e = parse_bvh(&bad).unwrap_err();
        assert!(e.starts_with(&format!("line {}:", last - 1)) && e.contains("abc"), "{e}");
        // short data
        let mut l2 = lines.clone();
        l2.pop();
        let e = parse_bvh(&l2.join("\n")).unwrap_err();
        assert!(e.contains("expected") && e.starts_with(&format!("line {}:", last - 1)), "{e}");
        // extra data
        let e = parse_bvh(&format!("{good}1 2 3\n")).unwrap_err();
        assert!(e.contains("extra data") && e.starts_with(&format!("line {}:", last + 1)), "{e}");
        // missing closing brace -> end of file
        let e = parse_bvh("HIERARCHY\nROOT A\n{\nOFFSET 0 0 0\nCHANNELS 0\n").unwrap_err();
        assert!(e.starts_with("line 5:") && e.contains("end of file"), "{e}");
        // junk inside a joint
        let e = parse_bvh("HIERARCHY\nROOT A\n{\nOFFSET 0 0 0\nBANANA\n}\n").unwrap_err();
        assert!(e.starts_with("line 5:") && e.contains("BANANA"), "{e}");
        // missing OFFSET, bad frame time, no HIERARCHY, empty, zero channels count text
        assert!(parse_bvh("HIERARCHY\nROOT A\n{\nCHANNELS 0\n}\nMOTION\nFrames: 0\nFrame Time: 1\n").unwrap_err().contains("no OFFSET"));
        let e = parse_bvh(&good.replace("Frame Time: 0.04", "Frame Time: 0")).unwrap_err();
        assert!(e.contains("frame time must be positive"), "{e}");
        assert!(parse_bvh("MOTION\n").unwrap_err().starts_with("line 1:"));
        assert!(parse_bvh("   \n").is_err());
        assert!(parse_bvh("HIERARCHY\nROOT A\n{\nOFFSET 0 0 nan\n").unwrap_err().contains("finite"));
        assert!(parse_bvh("HIERARCHY\nROOT\n{\n").unwrap_err().contains("no name"));
        // absurd frame counts never allocate
        let e = parse_bvh(&good.replace("Frames: 2", "Frames: 99999999999")).unwrap_err();
        assert!(e.contains("exceeds"), "{e}");
    }

    #[test]
    fn deep_nesting_is_refused_not_a_stack_overflow() {
        let mut s = String::from("HIERARCHY\nROOT A\n{\nOFFSET 0 0 0\nCHANNELS 0\n");
        for i in 0..700 {
            s.push_str(&format!("JOINT J{i}\n{{\nOFFSET 0 1 0\nCHANNELS 0\n"));
        }
        let e = parse_bvh(&s).unwrap_err();
        assert!(e.contains("nested deeper"), "{e}");
    }

    // ---------------------------------------------------------------- projection

    #[test]
    fn projected_angles_match_known_rotations() {
        // left upper arm rolled about Z by +40 degrees: points up-right, which is -40 on screen
        let rot = |role: &str, f: usize| if role == "uarmL" { [-(f as f64) * 10.0, 0.0, 0.0] } else { [0.0; 3] };
        let b = parse_bvh(&make_bvh(&MIXAMO, 5, 0.04, &rot, &at_origin, "\n")).unwrap();
        let pr = project_motion(&b, View::Front);
        let ia = b.joint_index("LeftArm").unwrap();
        for f in 0..5 {
            let a = pr.angles[f][ia].unwrap();
            assert!((a - f as f64 * 10.0).abs() < 1e-9, "frame {f}: {a}");
        }
        // right arm points to -x: angle 180 at rest
        let ir = b.joint_index("RightArm").unwrap();
        assert!((pr.angles[0][ir].unwrap().abs() - 180.0).abs() < 1e-9);
        // joint points are stage coordinates (y down): head above hips means smaller y
        let (ih, ihead) = (0, b.joint_index("Head").unwrap());
        assert!(pr.points[0][ihead][1] < pr.points[0][ih][1]);
        // a parent rotation is inherited (hips z +30 -> the whole arm is at -30 + its own)
        let rot2 = |role: &str, _f: usize| match role {
            "hips" => [30.0, 0.0, 0.0],
            "uarmL" => [-10.0, 0.0, 0.0],
            _ => [0.0; 3],
        };
        let b2 = parse_bvh(&make_bvh(&MIXAMO, 1, 0.04, &rot2, &at_origin, "\n")).unwrap();
        let a = project_motion(&b2, View::Front).angles[0][b2.joint_index("LeftArm").unwrap()].unwrap();
        assert!((a + 20.0).abs() < 1e-9, "{a}");
    }

    #[test]
    fn views_side_top_and_yaw() {
        let p = [1.0, 2.0, 3.0];
        assert_eq!(View::Front.project(p), [1.0, -2.0]);
        assert_eq!(View::Side.project(p), [3.0, -2.0]);
        assert_eq!(View::Top.project(p), [1.0, -3.0]);
        let y = View::parse("yaw=90").unwrap().project(p);
        assert!((y[0] - 3.0).abs() < 1e-12 && (y[1] + 2.0).abs() < 1e-12);
        let y0 = View::parse("yaw=0").unwrap().project(p);
        assert_eq!(y0, [1.0, -2.0]);
        assert!(View::parse("yaw=abc").is_err() && View::parse("sideways").is_err());
        // an arm along X has no side-view angle (points at the camera) but has one from the front/top
        let b = parse_bvh(&make_bvh(&MIXAMO, 1, 0.04, &still, &at_origin, "\n")).unwrap();
        let ia = b.joint_index("LeftArm").unwrap();
        assert!(project_motion(&b, View::Side).angles[0][ia].is_none());
        assert!((project_motion(&b, View::Top).angles[0][ia].unwrap()).abs() < 1e-9);
        assert!(project_motion(&b, View::Yaw(90.0)).angles[0][ia].is_none());
        assert!(project_motion(&b, View::Yaw(45.0)).angles[0][ia].is_some());
    }

    // ---------------------------------------------------------------- names and matching

    #[test]
    fn name_normalisation_and_classification() {
        assert_eq!(normalize_name("mixamorig:LeftUpLeg"), ["left", "up", "leg"]);
        assert_eq!(normalize_name("Bip01 L Thigh"), ["l", "thigh"]);
        assert_eq!(normalize_name("upper_arm.L"), ["upper", "arm", "l"]);
        assert_eq!(normalize_name("lfemur"), ["l", "femur"]);
        assert_eq!(normalize_name("Spine1"), ["spine", "1"]);
        assert_eq!(normalize_name("Armature|DEF-forearm.R"), ["forearm", "r"]);
        let c = |n: &str| {
            let t = classify(n);
            (t.role, t.side)
        };
        assert_eq!(c("LeftUpLeg"), (Role::Thigh, Some(Side::Left)));
        assert_eq!(c("RightLeg"), (Role::Shin, Some(Side::Right)));
        assert_eq!(c("LeftArm"), (Role::UpperArm, Some(Side::Left)));
        assert_eq!(c("LeftForeArm"), (Role::ForeArm, Some(Side::Left)));
        assert_eq!(c("Bip01 L Calf"), (Role::Shin, Some(Side::Left)));
        assert_eq!(c("lhumerus"), (Role::UpperArm, Some(Side::Left)));
        assert_eq!(c("rradius"), (Role::ForeArm, Some(Side::Right)));
        assert_eq!(c("LeftShoulder"), (Role::Clavicle, Some(Side::Left)));
        assert_eq!(c("LeftForeArm"), (Role::ForeArm, Some(Side::Left)));
        assert_eq!(c("arm_upper_r"), (Role::UpperArm, Some(Side::Right)));
        assert_eq!(c("Pelvis"), (Role::Hips, None));
        assert_eq!(c("Spine2").0, Role::Spine);
        assert_eq!(c("LeftToeBase").0, Role::Toe);
        assert_eq!(c("LeftHandIndex1").0, Role::Finger);
        assert_eq!(c("HeadTop_End").0, Role::Other);
        assert_eq!(c("LeftToe_End").0, Role::Other);
        assert_eq!(c("foot_ik.L").0, Role::Other);
        assert_eq!(c("Bip01").0, Role::Other);
        assert_eq!(c("xyz").0, Role::Other);
        assert!(classify("LeftLeg").conf < classify("LeftUpLeg").conf);
    }

    fn check_full_match(bvh_names: &Names, rig_names: &RigNames, expect_via: &str, min_conf: f32) {
        let b = parse_bvh(&make_bvh(bvh_names, 1, 0.04, &still, &at_origin, "\n")).unwrap();
        let p = rig(rig_names, 1.0);
        let m = match_bones(&b, &p, None).unwrap();
        let n = bvh_names;
        let r = rig_names;
        pair(&m, n.hips, r.hips);
        pair(&m, n.spine[0], r.spine[0]);
        pair(&m, n.spine[2], r.spine[1]); // three spine joints onto two bones: first and last
        for s in 0..2 {
            pair(&m, n.clav[s], r.clav[s]);
            pair(&m, n.uarm[s], r.uarm[s]);
            pair(&m, n.farm[s], r.farm[s]);
            pair(&m, n.hand[s], r.hand[s]);
            pair(&m, n.thigh[s], r.thigh[s]);
            pair(&m, n.shin[s], r.shin[s]);
            pair(&m, n.foot[s], r.foot[s]);
            pair(&m, n.toe[s], r.toe[s]);
        }
        pair(&m, n.head, r.head);
        for x in &m.matches {
            if x.slot == "neck" || x.slot == "head" || x.slot.starts_with("spine") {
                continue;
            }
            assert!(x.confidence >= min_conf, "{}: {}", x.slot, x.confidence);
            assert_eq!(x.via, expect_via, "{} {}", x.slot, x.via);
        }
        // tips follow the chain: upper arm points at the forearm joint
        let ua = m.matches.iter().find(|x| x.slot == "upperarm.l").unwrap();
        assert_eq!(ua.tip, Tip::Joint(b.joint_index(n.farm[0]).unwrap()));
    }

    #[test]
    fn matches_mixamo_style_by_name() {
        check_full_match(&MIXAMO, &BLENDER, "name", 0.85);
    }
    #[test]
    fn matches_mixamo_namespace_by_name() {
        check_full_match(&MIXAMO_NS, &BLENDER, "name", 0.85);
    }
    #[test]
    fn matches_cmu_style_by_name() {
        check_full_match(&CMU, &BLENDER, "name", 0.85);
    }
    #[test]
    fn matches_biped_style_by_name() {
        check_full_match(&BIPED, &BLENDER, "name", 0.85);
    }

    #[test]
    fn matches_anonymous_names_by_topology() {
        // rig has meaningless names; BVH has good ones -> topology on the rig side only
        let b = parse_bvh(&make_bvh(&MIXAMO, 1, 0.04, &still, &at_origin, "\n")).unwrap();
        let m = match_bones(&b, &rig(&RIG_ANON, 1.0), None).unwrap();
        pair(&m, "LeftArm", "b06");
        pair(&m, "RightArm", "b10");
        pair(&m, "LeftShoulder", "b05");
        pair(&m, "LeftForeArm", "b07");
        pair(&m, "LeftHand", "b08");
        pair(&m, "LeftUpLeg", "b13");
        pair(&m, "RightUpLeg", "b17");
        pair(&m, "LeftLeg", "b14");
        pair(&m, "LeftToeBase", "b16");
        pair(&m, "Head", "b04");
        pair(&m, "Neck", "b03");
        pair(&m, "Hips", "b00");
        let la = m.matches.iter().find(|x| x.bvh_name == "LeftArm").unwrap();
        assert_eq!(la.via, "topology");
        assert!(la.confidence <= 0.6);
        // both sides anonymous
        let b2 = parse_bvh(&make_bvh(&ANON, 1, 0.04, &still, &at_origin, "\n")).unwrap();
        let m2 = match_bones(&b2, &rig(&RIG_ANON, 1.0), None).unwrap();
        for (a, r) in [("j07", "b06"), ("j11", "b10"), ("j14", "b13"), ("j18", "b17"), ("j09", "b08"), ("j05", "b04"), ("j00", "b00"), ("j20", "b19"), ("j06", "b05")] {
            pair(&m2, a, r);
        }
    }

    #[test]
    fn limb_length_ratio_reported_and_outlier_penalised() {
        let b = parse_bvh(&make_bvh(&MIXAMO, 1, 0.04, &still, &at_origin, "\n")).unwrap();
        let mut p = rig(&BLENDER, 1.0);
        let m0 = match_bones(&b, &p, None).unwrap();
        let ua = m0.matches.iter().find(|x| x.slot == "upperarm.l").unwrap();
        let base = ua.confidence;
        assert!((ua.length_ratio.unwrap() - 40.0 / 26.0).abs() < 1e-6, "{:?}", ua.length_ratio);
        // make one forearm absurdly long
        let i = bone_idx(&p, "forearm.L");
        p.bones[i].length = 900.0;
        let m1 = match_bones(&b, &p, None).unwrap();
        let fa = m1.matches.iter().find(|x| x.slot == "forearm.l").unwrap();
        assert!(fa.confidence < 0.6 && fa.note.contains("off the median"), "{:?}", fa);
        assert_eq!(m1.matches.iter().find(|x| x.slot == "upperarm.l").unwrap().confidence, base);
        let t = format_table(&m1);
        assert!(t.contains("forearm.l") && t.contains("unmatched BVH joints"));
    }

    #[test]
    fn manual_map_overrides_and_validates() {
        let b = parse_bvh(&make_bvh(&MIXAMO, 1, 0.04, &still, &at_origin, "\n")).unwrap();
        let p = rig(&BLENDER, 1.0);
        // swap the two upper arms and drop the left hand
        let map = parse_map(r#"{"LeftArm": "upper_arm.R", "RightArm": "upper_arm.L", "LeftHand": null}"#).unwrap();
        let m = match_bones(&b, &p, Some(&map)).unwrap();
        let la = m.matches.iter().find(|x| x.bvh_name == "LeftArm").unwrap();
        assert_eq!((la.rig_name.as_str(), la.via, la.confidence), ("upper_arm.R", "manual", 1.0));
        assert!(m.matches.iter().all(|x| x.bvh_name != "LeftHand"));
        assert!(m.matches.iter().any(|x| x.bvh_name == "RightHand"));
        assert!(m.matches.iter().filter(|x| x.rig_name == "upper_arm.R").count() == 1);
        // wrapped form
        assert_eq!(parse_map(r#"{"map": {"A": "b"}}"#).unwrap().len(), 1);
        // errors
        assert!(match_bones(&b, &p, Some(&parse_map(r#"{"Nope": "spine"}"#).unwrap())).unwrap_err().contains("no BVH joint 'Nope'"));
        assert!(match_bones(&b, &p, Some(&parse_map(r#"{"Hips": "nope"}"#).unwrap())).unwrap_err().contains("no rig bone 'nope'"));
        assert!(match_bones(&b, &p, Some(&parse_map(r#"{"Hips": "spine", "Spine": "spine"}"#).unwrap())).unwrap_err().contains("both"));
        assert!(parse_map("[1]").is_err() && parse_map(r#"{"A": 3}"#).is_err() && parse_map("{").is_err());
        assert!(match_bones(&b, &Project::new("e", 10.0, 10.0, 24.0, 5), None).is_err());
    }

    // ---------------------------------------------------------------- retargeting

    fn walk_rot(role: &str, f: usize) -> [f64; 3] {
        let t = f as f64 * 0.3;
        match role {
            "hips" => [4.0 * t.sin(), 0.0, 0.0],
            "sp0" => [3.0 * t.cos(), 0.0, 0.0],
            "uarmL" => [-40.0 * t.sin(), 0.0, 0.0],
            "farmL" => [-20.0 - 15.0 * t.sin(), 0.0, 0.0],
            "uarmR" => [30.0 * t.sin(), 0.0, 0.0],
            "head" => [10.0 * t.cos(), 0.0, 0.0],
            _ => [0.0; 3],
        }
    }
    fn walk_pos(f: usize) -> V3 {
        let t = f as f64 * 0.3;
        [2.0 * f as f64, 100.0 + 3.0 * t.sin(), 0.0]
    }

    #[test]
    fn manual_hips_map_keeps_the_hips_slot_and_root_motion() {
        let b = parse_bvh(&make_bvh(&MIXAMO, 40, 1.0 / 30.0, &walk_rot, &walk_pos, "\n")).unwrap();
        let names = RigNames { hips: "zz_root", ..BLENDER };
        let mut p = rig(&names, 1.0);
        p.frames = 40;
        let map = parse_map(r#"{"Hips":"zz_root"}"#).unwrap();
        let m = match_bones(&b, &p, Some(&map)).unwrap();
        assert!(m.matches.iter().any(|x| x.slot == "hips" && x.rig_name == "zz_root"), "{:?}", m.matches.iter().map(|x| (&x.slot, &x.rig_name)).collect::<Vec<_>>());
        assert!(!m.warnings.iter().any(|w| w.contains("no hips match")), "{:?}", m.warnings);
        let r = retarget(&p, &b, &m, &RetargetOptions::default()).unwrap();
        let root = bone_idx(&p, "zz_root");
        let chans: Vec<_> = r.channels.iter().filter(|c| c.target == "zz_root").collect();
        assert!(chans.iter().any(|c| matches!(c.prop, Prop::BoneOffsetX)), "no root travel");
        assert!(chans.iter().any(|c| matches!(c.prop, Prop::BoneOffsetY)), "no root bob");
        let _ = root;
    }

    #[test]
    fn retarget_front_view_reproduces_bvh_angles_and_root_motion() {
        let b = parse_bvh(&make_bvh(&MIXAMO, 40, 1.0 / 30.0, &walk_rot, &walk_pos, "\n")).unwrap();
        let mut p = rig(&BLENDER, 1.0);
        p.frames = 40;
        let m = match_bones(&b, &p, None).unwrap();
        let r = retarget(&p, &b, &m, &RetargetOptions::default()).unwrap();
        assert_eq!(r.out_frames, 40);
        assert!((r.scale - 85.0 / 82.0).abs() < 1e-9, "{}", r.scale);
        let mut q = p.clone();
        bake(&mut q, &r);
        let e = Engine::new(q.clone()).unwrap();
        // expected world angle = rig rest world angle + 2D rest-relative angle, computed from 3D
        let proj = project_motion(&b, View::Front);
        let rest = project_motion(&parse_bvh(&make_bvh(&MIXAMO, 1, 1.0 / 30.0, &still, &at_origin, "\n")).unwrap(), View::Front);
        let rw = crate::rigedit::rest_world(&p).unwrap();
        for (bj, rb) in [("LeftArm", "upper_arm.L"), ("LeftForeArm", "forearm.L"), ("RightArm", "upper_arm.R"), ("Head", "head"), ("Spine", "spine"), ("Hips", "pelvis"), ("LeftUpLeg", "thigh.L"), ("RightLeg", "shin.R"), ("LeftFoot", "foot.L")] {
            let (ji, ri) = (b.joint_index(bj).unwrap(), bone_idx(&p, rb));
            // the hips/spine tips differ from the default tip only for hips (default: longest child)
            for f in [0usize, 7, 19, 39] {
                let want = match bj {
                    "Hips" => {
                        // hips tip is the spine joint
                        let si = b.joint_index("Spine").unwrap();
                        let pose = b.pose(Some(f));
                        let rp = b.pose(None);
                        norm360(angle_2d(pose.pos[0], pose.pos[si], View::Front, false).unwrap() - angle_2d(rp.pos[0], rp.pos[si], View::Front, false).unwrap())
                    }
                    _ => norm360(proj.angles[f][ji].unwrap() - rest.angles[0][ji].unwrap()),
                };
                let got = world_angle(&e, f as f32, ri) - rw[ri].1 as f64;
                assert!(ang_diff(got, want) < 0.02, "{bj} frame {f}: got {got} want {want}");
            }
        }
        // root travel: +x moves right by scale*dx, bob up is smaller y
        let hb = bone_idx(&p, "pelvis");
        let w0 = e.evaluate(0.0).world[hb];
        let w30 = e.evaluate(30.0).world[hb];
        let dx = 2.0 * 30.0 * r.scale;
        assert!(((w30.tx - w0.tx) as f64 - dx).abs() < 0.01, "{} vs {dx}", w30.tx - w0.tx);
        let t30: f64 = 30.0 * 0.3;
        let dy = -(3.0 * t30.sin()) * r.scale;
        assert!(((w30.ty - w0.ty) as f64 - dy).abs() < 0.01, "{} vs {dy}", w30.ty - w0.ty);
        // channels are appended, original data untouched
        assert_eq!(q.channels.len(), p.channels.len() + r.channels.len());
        assert_eq!(&q.bones, &p.bones);
    }

    #[test]
    fn different_proportions_retarget_to_same_angles() {
        let b = parse_bvh(&make_bvh(&MIXAMO, 20, 1.0 / 30.0, &walk_rot, &walk_pos, "\n")).unwrap();
        let mut angles = Vec::new();
        for k in [0.5f32, 1.0, 2.5] {
            let mut p = rig(&BLENDER, k);
            p.frames = 20;
            let m = match_bones(&b, &p, None).unwrap();
            let r = retarget(&p, &b, &m, &RetargetOptions::default()).unwrap();
            bake(&mut p, &r);
            let e = Engine::new(p.clone()).unwrap();
            angles.push(world_angle(&e, 13.0, bone_idx(&p, "forearm.L")));
            // root travel scales with the leg length
            let hb = bone_idx(&p, "pelvis");
            let travel = e.evaluate(19.0).world[hb].tx - e.evaluate(0.0).world[hb].tx;
            let expect = 2.0 * 19.0 * (k as f64 * 85.0 / 82.0);
            assert!(((travel as f64) - expect).abs() < 0.05 * expect.max(1.0), "k={k}: {travel} vs {expect}");
        }
        assert!(ang_diff(angles[0], angles[1]) < 0.05 && ang_diff(angles[1], angles[2]) < 0.05, "{angles:?}");
    }

    #[test]
    fn side_view_knee_bend() {
        let rot = |role: &str, f: usize| match role {
            "thighL" => [0.0, 5.0 * f as f64, 0.0],
            "shinL" => [0.0, -3.0 * f as f64, 0.0],
            _ => [0.0; 3],
        };
        let b = parse_bvh(&make_bvh(&MIXAMO, 10, 1.0 / 30.0, &rot, &at_origin, "\n")).unwrap();
        let mut p = rig(&BLENDER, 1.0);
        p.frames = 10;
        let m = match_bones(&b, &p, None).unwrap();
        let o = RetargetOptions { view: View::Side, ..Default::default() };
        let r = retarget(&p, &b, &m, &o).unwrap();
        // arms point at the camera in the side view: reported, not a failure
        assert!(r.degenerate == 0, "arms have a rest angle problem, not frame problem");
        assert!(r.notes.iter().any(|n| n.contains("points along the view axis")), "{:?}", r.notes);
        bake(&mut p, &r);
        let e = Engine::new(p.clone()).unwrap();
        let (ti, si) = (bone_idx(&p, "thigh.L"), bone_idx(&p, "shin.L"));
        // thigh Delta = +5f (see derivation: side-view angle is 90 + alpha); shin world = thigh + (-3f)
        for f in [0usize, 4, 9] {
            assert!(ang_diff(world_angle(&e, f as f32, ti), 90.0 + 5.0 * f as f64) < 0.02);
            assert!(ang_diff(world_angle(&e, f as f32, si), 90.0 + 2.0 * f as f64) < 0.02, "{}", world_angle(&e, f as f32, si));
        }
    }

    #[test]
    fn resamples_to_project_rate_and_extends_project() {
        // 60 fps, 61 frames (1 s): arm sweeps linearly 0 -> -60 deg Z (screen +60)
        let rot = |role: &str, f: usize| if role == "uarmL" { [-(f as f64), 0.0, 0.0] } else { [0.0; 3] };
        let b = parse_bvh(&make_bvh(&MIXAMO, 61, 1.0 / 60.0, &rot, &at_origin, "\n")).unwrap();
        let mut p = rig(&BLENDER, 1.0);
        p.fps = 24.0;
        p.frames = 10;
        let m = match_bones(&b, &p, None).unwrap();
        let r = retarget(&p, &b, &m, &RetargetOptions::default()).unwrap();
        assert_eq!(r.out_frames, 25);
        assert_eq!(r.project_frames, 25);
        let ch = r.channels.iter().find(|c| c.target == "upper_arm.L" && c.prop == Prop::BoneAngle).unwrap();
        let crate::project::Source::Keys { track } = &ch.source else { panic!() };
        assert_eq!(track.keys.len(), 25);
        for (k, key) in track.keys.iter().enumerate() {
            // world delta = 60 * k/24; upper_arm.L parent chain is at rest so local delta = same
            assert!((key.value as f64 - 60.0 * k as f64 / 24.0).abs() < 1e-3, "{k}: {}", key.value);
            assert_eq!(key.frame, k as f32);
        }
        let mut q = p.clone();
        bake(&mut q, &r);
        assert_eq!(q.frames, 25);
        // explicit fps and sub-range
        let o = RetargetOptions { fps: Some(10.0), start: Some(12), end: Some(42), ..Default::default() };
        let r2 = retarget(&p, &b, &m, &o).unwrap();
        assert_eq!(r2.out_frames, 6); // 30 source intervals of 1/60 s = 0.5 s -> 0..0.5 s at 10 fps
    }

    #[test]
    fn smoothing_damps_jitter_and_keeps_endpoints() {
        let rot = |role: &str, f: usize| if role == "uarmL" { [if f.is_multiple_of(2) { 0.0 } else { -20.0 }, 0.0, 0.0] } else { [0.0; 3] };
        let b = parse_bvh(&make_bvh(&MIXAMO, 21, 1.0 / 30.0, &rot, &at_origin, "\n")).unwrap();
        let mut p = rig(&BLENDER, 1.0);
        p.frames = 21;
        let m = match_bones(&b, &p, None).unwrap();
        let var = |smooth: usize| {
            let o = RetargetOptions { smooth, ..Default::default() };
            let r = retarget(&p, &b, &m, &o).unwrap();
            let ch = r.channels.iter().find(|c| c.target == "upper_arm.L" && c.prop == Prop::BoneAngle).unwrap();
            let crate::project::Source::Keys { track } = &ch.source else { panic!() };
            let v: Vec<f32> = track.keys.iter().map(|k| k.value).collect();
            (v[0], v[20], v[1..20].iter().map(|x| (x - 10.0).abs()).fold(0.0, f32::max))
        };
        let (a0, a20, j0) = var(0);
        let (b0, b20, j1) = var(1);
        assert!(j0 > 9.0 && j1 < 0.5 * j0 + 3.5, "{j0} {j1}");
        assert_eq!((a0, a20), (b0, b20));
    }

    #[test]
    fn deterministic_and_locks_respected() {
        let b = parse_bvh(&make_bvh(&MIXAMO, 30, 1.0 / 30.0, &walk_rot, &walk_pos, "\n")).unwrap();
        let mut p = rig(&BLENDER, 1.0);
        let m1 = match_bones(&b, &p, None).unwrap();
        let m2 = match_bones(&b, &p, None).unwrap();
        assert_eq!(format_table(&m1), format_table(&m2));
        let r1 = retarget(&p, &b, &m1, &RetargetOptions::default()).unwrap();
        let r2 = retarget(&p, &b, &m2, &RetargetOptions::default()).unwrap();
        assert_eq!(serde_json::to_string(&r1.channels).unwrap(), serde_json::to_string(&r2.channels).unwrap());
        // lock one channel
        p.channels.push(Channel::keys("upper_arm.L", Prop::BoneAngle, Track::with_keys(&[(0.0, 5.0, Ease::Linear)])));
        p.channel_meta.push(crate::chanmeta::ChannelMeta { target: "upper_arm.L".into(), prop: Some(Prop::BoneAngle), lock: true, ..Default::default() });
        let r = retarget(&p, &b, &m1, &RetargetOptions::default()).unwrap();
        assert_eq!(r.blocked, vec!["upper_arm.L.BoneAngle".to_string()]);
        assert!(!r.channels.iter().any(|c| c.target == "upper_arm.L"));
        assert!(r.channels.iter().any(|c| c.target == "upper_arm.R"));
        assert!(r.notes.iter().any(|n| n.contains("locked")));
    }

    #[test]
    fn editor_undo_removes_the_whole_retarget() {
        let b = parse_bvh(&make_bvh(&MIXAMO, 50, 1.0 / 30.0, &walk_rot, &walk_pos, "\n")).unwrap();
        let mut p = rig(&BLENDER, 1.0);
        p.frames = 20;
        let m = match_bones(&b, &p, None).unwrap();
        let r = retarget(&p, &b, &m, &RetargetOptions::default()).unwrap();
        let mut ed = crate::edit::Editor::new(p.clone()).unwrap();
        let before = ed.doc().clone();
        ed.apply(bake_op(ed.project(), &r).unwrap()).unwrap();
        assert_eq!(ed.project().channels.len(), r.channels.len());
        assert_eq!(ed.project().frames, 50);
        let mut direct = p.clone();
        bake(&mut direct, &r);
        assert_eq!(&direct, ed.project());
        assert!(ed.undo());
        assert_eq!(ed.doc(), &before);
        assert!(ed.redo());
        assert_eq!(ed.project().channels.len(), r.channels.len());
    }

    #[test]
    fn options_flags_and_validation() {
        let b = parse_bvh(&make_bvh(&MIXAMO, 10, 1.0 / 30.0, &walk_rot, &walk_pos, "\n")).unwrap();
        let p = rig(&BLENDER, 1.0);
        let m = match_bones(&b, &p, None).unwrap();
        let has = |r: &RetargetResult, t: &str, pr: Prop| r.channels.iter().any(|c| c.target == t && c.prop == pr);
        let base = retarget(&p, &b, &m, &RetargetOptions::default()).unwrap();
        assert!(has(&base, "pelvis", Prop::BoneOffsetX) && has(&base, "pelvis", Prop::BoneOffsetY));
        let nr = retarget(&p, &b, &m, &RetargetOptions { root_motion: false, ..Default::default() }).unwrap();
        assert!(!has(&nr, "pelvis", Prop::BoneOffsetX));
        // in place: x travel zero, bob kept
        let ip = retarget(&p, &b, &m, &RetargetOptions { in_place: true, ..Default::default() }).unwrap();
        let get = |r: &RetargetResult, pr: Prop| {
            let c = r.channels.iter().find(|c| c.target == "pelvis" && c.prop == pr).unwrap();
            let crate::project::Source::Keys { track } = &c.source else { panic!() };
            track.keys.iter().map(|k| k.value).collect::<Vec<f32>>()
        };
        assert!(get(&ip, Prop::BoneOffsetX).iter().all(|v| *v == 0.0));
        assert_eq!(get(&ip, Prop::BoneOffsetY), get(&base, Prop::BoneOffsetY));
        // fixed scale and weight
        let fx = retarget(&p, &b, &m, &RetargetOptions { scale: Scale::Fixed(2.0), weight: 0.5, ..Default::default() }).unwrap();
        assert_eq!(fx.scale, 2.0);
        assert!(fx.channels.iter().all(|c| c.weight == 0.5));
        let x9 = get(&base, Prop::BoneOffsetX)[9];
        assert!((get(&fx, Prop::BoneOffsetX)[9] - x9 * 2.0 / base.scale as f32).abs() < 1e-3);
        // mirror flips the sign of root travel and of angle deltas
        let mi = retarget(&p, &b, &m, &RetargetOptions { mirror: true, ..Default::default() }).unwrap();
        assert!((get(&mi, Prop::BoneOffsetX)[9] + x9).abs() < 1e-3);
        // min confidence filters
        let hi = retarget(&p, &b, &m, &RetargetOptions { min_confidence: 0.99, ..Default::default() }).unwrap();
        assert!(hi.bones_used < base.bones_used);
        assert!(retarget(&p, &b, &m, &RetargetOptions { min_confidence: 2.0, ..Default::default() }).is_err());
        // errors
        for o in [
            RetargetOptions { start: Some(5), end: Some(2), ..Default::default() },
            RetargetOptions { end: Some(99), ..Default::default() },
            RetargetOptions { fps: Some(0.0), ..Default::default() },
            RetargetOptions { fps: Some(f64::NAN), ..Default::default() },
            RetargetOptions { scale: Scale::Fixed(-1.0), ..Default::default() },
            RetargetOptions { weight: 2.0, ..Default::default() },
            RetargetOptions { rest_frame: Some(50), ..Default::default() },
            RetargetOptions { smooth: 5000, ..Default::default() },
        ] {
            assert!(retarget(&p, &b, &m, &o).is_err(), "{o:?}");
        }
        let empty = parse_bvh("HIERARCHY\nROOT A\n{\nOFFSET 0 0 0\nCHANNELS 0\n}\nMOTION\nFrames: 0\nFrame Time: 0.1\n").unwrap();
        assert!(retarget(&p, &empty, &m, &RetargetOptions::default()).unwrap_err().contains("no frames"));
    }

    #[test]
    fn rest_frame_option_uses_that_pose_as_reference() {
        // frame 0 already has the arm raised 30 degrees; with rest_frame 0 the delta starts at zero
        let rot = |role: &str, f: usize| if role == "uarmL" { [-30.0 - f as f64, 0.0, 0.0] } else { [0.0; 3] };
        let b = parse_bvh(&make_bvh(&MIXAMO, 5, 1.0 / 30.0, &rot, &at_origin, "\n")).unwrap();
        let mut p = rig(&BLENDER, 1.0);
        p.frames = 5;
        let m = match_bones(&b, &p, None).unwrap();
        let val = |o: &RetargetOptions| {
            let r = retarget(&p, &b, &m, o).unwrap();
            let c = r.channels.iter().find(|c| c.target == "upper_arm.L" && c.prop == Prop::BoneAngle).unwrap();
            let crate::project::Source::Keys { track } = &c.source else { panic!() };
            (track.keys[0].value, track.keys[4].value)
        };
        let (a, _) = val(&RetargetOptions::default());
        assert!((a - 30.0).abs() < 1e-4);
        let (a, z) = val(&RetargetOptions { rest_frame: Some(0), ..Default::default() });
        assert!(a.abs() < 1e-4 && (z - 4.0).abs() < 1e-4, "{a} {z}");
    }

    #[test]
    fn foot_lock_reuses_footlock_module() {
        let b = parse_bvh(&make_bvh(&MIXAMO, 12, 1.0 / 30.0, &walk_rot, &walk_pos, "\n")).unwrap();
        let mut p = rig(&BLENDER, 1.0);
        p.bones.push(Bone::new("footctl", None, 5.0, Vec2::new(100.0, 200.0), 0.0));
        let m = match_bones(&b, &p, None).unwrap();
        let plain = retarget(&p, &b, &m, &RetargetOptions::default()).unwrap();
        let o = RetargetOptions { foot_lock: Some(("footctl".into(), 200.0)), ..Default::default() };
        let r = retarget(&p, &b, &m, &o).unwrap();
        assert_eq!(r.channels.len(), plain.channels.len() + 2);
        // controller sits on the ground the whole time: no contact change is needed but the call ran
        assert!(r.notes.iter().any(|n| n.contains("foot lock")), "{:?}", r.notes);
        // a child bone is not a valid controller
        let bad = RetargetOptions { foot_lock: Some(("shin.L".into(), 200.0)), ..Default::default() };
        assert!(retarget(&p, &b, &m, &bad).is_err());
    }
}
