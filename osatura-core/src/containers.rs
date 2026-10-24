// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
//! Layout containers and scroll regions for responsive scenes.
//!
//! A [`Container`] arranges an ordered list of members (layers, nested containers, spacers) along
//! a main axis (row or column) inside a rectangle. The distribution maths is the usual
//! one-dimensional "flexible box" arithmetic, written here from scratch with this project's own
//! schema:
//!
//! * every member has a *basis* size (explicit `width`/`height`, else the bounding box of the
//!   layer's geometry, else the intrinsic size of a nested container) clamped by optional
//!   min / max;
//! * lines are filled greedily when `wrap` is on (otherwise there is one line);
//! * free space on a line is shared by `grow` weights, a shortfall is taken back by
//!   `shrink * basis` weights, and members that hit a min / max are frozen and the rest is
//!   re-shared (a loop that terminates after at most one pass per member);
//! * what is left over is placed by `justify` (start, center, end, space_between, space_around);
//! * across the axis a member is placed by `align` (start, center, end, stretch), per member
//!   overridable.
//!
//! The container rectangle comes from a [`RectRule`]: fixed, pinned to canvas edges with margins
//! and optional size, an aspect-ratio fit / fill of the canvas, or "hug" (the content's own
//! size). Nested containers take their rectangle from their parent's slot.
//!
//! The result is, per member layer, a translation (and, for members with `resize`, a scale
//! factor) that the engine adds to the layer's animated transform on every evaluation - a
//! non-keyed, deterministic pass, like glue in [`crate::links`]. Sizes are measured on the *rest*
//! pose (static transforms), so animation of position / rotation / scale is layered on top of
//! the slot without re-flowing.
//!
//! A [`Scroll`] puts a content container behind a viewport rectangle: the content is shifted by
//! a clamped offset, optionally with a deterministic rubber-band overscroll, and the viewport
//! can clip the content through the ordinary layer-mask mechanism (a rectangle layer named by
//! `clip_layer`, which is fitted to the viewport). [`ScrollRuntime`] simulates dragging, release,
//! momentum, snap points and the elastic return.
//!
//! Limits: sizes and slots live in the parent space of the members (stage coordinates for
//! top-level layers; camera and parallax are not considered); stretch needs an unrotated,
//! unskewed, measurable layer; measurement uses flattened outlines of vector layers and groups
//! of them (image, emitter, liquid and boolean layers need explicit `width` and `height`, and
//! then the box origin is the layer's position).

use crate::layout::{content_bounds, layer_bounds};
use crate::math::Vec2;
use crate::project::{Layer, MaskRef, Project};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const MAX_DEPTH: usize = 64;
const MAX_MEMBERS: usize = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dir {
    #[default]
    Row,
    Column,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Wrap {
    #[default]
    None,
    Wrap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AspectMode {
    /// The rectangle fits inside the available area.
    #[default]
    Fit,
    /// The rectangle covers the available area (and overflows on one axis).
    Fill,
}

fn half() -> f32 {
    0.5
}

/// Where a root container's rectangle comes from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RectRule {
    /// Absolute rectangle.
    Fixed { x: f32, y: f32, w: f32, h: f32 },
    /// Pinned to canvas edges. Per axis: both margins give the size (canvas minus margins); one
    /// margin plus `width`/`height` pins that edge; a size alone centres; one margin alone makes
    /// the rectangle extend to the opposite edge; nothing set covers the whole canvas.
    Pin {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        left: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        top: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        right: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bottom: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<f32>,
    },
    /// A rectangle of width / height `ratio` placed in the canvas inset by `margin`; `ax`/`ay`
    /// (0..1, default centred) place it in the leftover space.
    Aspect {
        ratio: f32,
        #[serde(default)]
        mode: AspectMode,
        #[serde(default)]
        margin: f32,
        #[serde(default = "half")]
        ax: f32,
        #[serde(default = "half")]
        ay: f32,
    },
    /// Top-left corner given, size from the content.
    Hug {
        #[serde(default)]
        x: f32,
        #[serde(default)]
        y: f32,
    },
}

impl Default for RectRule {
    /// The whole canvas.
    fn default() -> Self {
        RectRule::Pin {
            left: None,
            top: None,
            right: None,
            bottom: None,
            width: None,
            height: None,
        }
    }
}

fn one() -> f32 {
    1.0
}

/// One entry of a container: exactly one of `layer`, `container` or `spacer`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Member {
    /// Layer path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    /// Id of a nested container.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    /// An empty flexible gap (size from `width` / `height` / `grow`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub spacer: bool,
    /// Share of free main-axis space (0 = none).
    #[serde(default)]
    pub grow: f32,
    /// Share of a main-axis shortfall (weighted by size).
    #[serde(default = "one")]
    pub shrink: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_w: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_w: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_h: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_h: Option<f32>,
    /// Cross-axis alignment override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<Align>,
    /// Scale a layer to its slot (growth, shrinkage and cross-axis stretch change its size).
    /// Without it the layer keeps its size and sits at the start of its slot.
    #[serde(default, skip_serializing_if = "is_false")]
    pub resize: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Member {
    fn blank() -> Member {
        Member {
            layer: None,
            container: None,
            spacer: false,
            grow: 0.0,
            shrink: 1.0,
            width: None,
            height: None,
            min_w: None,
            max_w: None,
            min_h: None,
            max_h: None,
            align: None,
            resize: false,
        }
    }
    pub fn layer(path: &str) -> Member {
        Member {
            layer: Some(path.into()),
            ..Member::blank()
        }
    }
    pub fn nested(id: &str) -> Member {
        Member {
            container: Some(id.into()),
            ..Member::blank()
        }
    }
    pub fn spacer(grow: f32) -> Member {
        Member {
            spacer: true,
            grow,
            ..Member::blank()
        }
    }
    fn label(&self) -> String {
        match (&self.layer, &self.container) {
            (Some(l), _) => format!("layer '{l}'"),
            (_, Some(c)) => format!("container '{c}'"),
            _ => "spacer".into(),
        }
    }
    fn fixed(&self, axis: usize) -> Option<f32> {
        if axis == 0 {
            self.width
        } else {
            self.height
        }
    }
    fn limits(&self, axis: usize) -> (f32, f32) {
        let (lo, hi) = if axis == 0 {
            (self.min_w, self.max_w)
        } else {
            (self.min_h, self.max_h)
        };
        (lo.unwrap_or(0.0), hi.unwrap_or(f32::INFINITY))
    }
    fn clamp(&self, axis: usize, v: f32) -> f32 {
        let (lo, hi) = self.limits(axis);
        v.max(lo).min(hi)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Container {
    pub id: String,
    pub members: Vec<Member>,
    #[serde(default)]
    pub axis: Dir,
    /// Space between neighbours on a line.
    #[serde(default)]
    pub gap: f32,
    /// Space between wrapped lines (default: `gap`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_gap: Option<f32>,
    /// Left, top, right, bottom.
    #[serde(default)]
    pub padding: [f32; 4],
    #[serde(default)]
    pub justify: Justify,
    #[serde(default)]
    pub align: Align,
    #[serde(default)]
    pub wrap: Wrap,
    /// Rectangle of a root container (ignored for nested containers and scroll content).
    #[serde(default)]
    pub rect: RectRule,
}

impl Container {
    pub fn new(id: &str, axis: Dir, members: Vec<Member>) -> Container {
        Container {
            id: id.into(),
            members,
            axis,
            gap: 0.0,
            cross_gap: None,
            padding: [0.0; 4],
            justify: Justify::Start,
            align: Align::Start,
            wrap: Wrap::None,
            rect: RectRule::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollAxis {
    #[default]
    Y,
    X,
    Both,
}

fn def_stiffness() -> f32 {
    180.0
}
fn def_zeta() -> f32 {
    0.9
}

/// Rubber-band overscroll.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Elastic {
    /// Largest displayed overscroll in pixels (the pull saturates towards it).
    pub limit: f32,
    #[serde(default = "def_stiffness")]
    pub stiffness: f32,
    #[serde(default = "def_zeta")]
    pub damping_ratio: f32,
}

fn def_fling() -> f32 {
    0.15
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scroll {
    pub id: String,
    /// Viewport rectangle (also the clip rectangle).
    #[serde(default)]
    pub viewport: RectRule,
    /// Id of the content container (must not be a member of another container).
    pub content: String,
    #[serde(default)]
    pub axis: ScrollAxis,
    /// Requested offset in pixels: how far the content is moved up / left.
    #[serde(default)]
    pub offset: [f32; 2],
    /// Overscroll with a rubber band instead of clamping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elastic: Option<Elastic>,
    /// Resting offsets (x axis / y axis) a release settles on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snap_x: Vec<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snap_y: Vec<f32>,
    /// Seconds of release velocity projected to find the landing offset.
    #[serde(default = "def_fling")]
    pub fling: f32,
    /// Rectangle layer (normally hidden) fitted to the viewport and used as a mask for every
    /// layer of the content, which clips the content to the viewport.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip_layer: Option<String>,
    /// External value path that drives the offset (pixels along the main axis); see [`apply_inputs`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
}

impl Scroll {
    pub fn new(id: &str, content: &str) -> Scroll {
        Scroll {
            id: id.into(),
            viewport: RectRule::default(),
            content: content.into(),
            axis: ScrollAxis::Y,
            offset: [0.0; 2],
            elastic: None,
            snap_x: vec![],
            snap_y: vec![],
            fling: def_fling(),
            clip_layer: None,
            input: None,
        }
    }
    fn scrolls(&self, axis: usize) -> bool {
        match self.axis {
            ScrollAxis::Both => true,
            ScrollAxis::X => axis == 0,
            ScrollAxis::Y => axis == 1,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------------------------

/// What the engine adds to one layer: a translation and a scale factor.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub layer: String,
    pub dx: f32,
    pub dy: f32,
    pub kx: f32,
    pub ky: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScrollInfo {
    pub id: String,
    pub viewport: [f32; 4],
    /// Content rectangle including the scroll shift.
    pub content: [f32; 4],
    /// Largest useful offset per axis.
    pub max: [f32; 2],
    /// Offset actually applied (clamped, or rubber-banded when elastic).
    pub offset: [f32; 2],
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Solution {
    pub canvas: (f32, f32),
    /// `(container id, x, y, w, h)` in container order.
    pub rects: Vec<(String, [f32; 4])>,
    pub layers: Vec<Placement>,
    pub scrolls: Vec<ScrollInfo>,
    pub notes: Vec<String>,
}

// ---------------------------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------------------------

fn finite(v: f32) -> bool {
    v.is_finite()
}

fn check_rule(r: &RectRule, who: &str, errs: &mut Vec<String>) {
    match r {
        RectRule::Fixed { x, y, w, h } => {
            if ![*x, *y, *w, *h].iter().all(|v| finite(*v)) {
                errs.push(format!("{who}: rect values must be finite"));
            } else if *w <= 0.0 || *h <= 0.0 {
                errs.push(format!("{who}: fixed rect needs w > 0 and h > 0"));
            }
        }
        RectRule::Pin {
            left,
            top,
            right,
            bottom,
            width,
            height,
        } => {
            for (n, v) in [
                ("left", left),
                ("top", top),
                ("right", right),
                ("bottom", bottom),
                ("width", width),
                ("height", height),
            ] {
                if let Some(v) = v {
                    if !finite(*v) {
                        errs.push(format!("{who}: {n} must be finite"));
                    } else if (n == "width" || n == "height") && *v <= 0.0 {
                        errs.push(format!("{who}: {n} must be > 0"));
                    }
                }
            }
            if left.is_some() && right.is_some() && width.is_some() {
                errs.push(format!(
                    "{who}: left, right and width are all set (over-constrained)"
                ));
            }
            if top.is_some() && bottom.is_some() && height.is_some() {
                errs.push(format!(
                    "{who}: top, bottom and height are all set (over-constrained)"
                ));
            }
        }
        RectRule::Aspect {
            ratio,
            margin,
            ax,
            ay,
            ..
        } => {
            if !(finite(*ratio) && *ratio > 0.0) {
                errs.push(format!("{who}: aspect ratio must be a finite number > 0"));
            }
            if !(finite(*margin) && *margin >= 0.0) {
                errs.push(format!("{who}: margin must be >= 0"));
            }
            if !(finite(*ax) && finite(*ay) && (0.0..=1.0).contains(ax) && (0.0..=1.0).contains(ay))
            {
                errs.push(format!("{who}: ax and ay must be within 0..1"));
            }
        }
        RectRule::Hug { x, y } => {
            if !(finite(*x) && finite(*y)) {
                errs.push(format!("{who}: hug position must be finite"));
            }
        }
    }
}

fn layer_is_stretchable(l: &Layer) -> bool {
    let t = &l.transform;
    t.rotation == 0.0 && t.skew == 0.0 && layer_bounds(l).is_some()
}

/// Structural validation of `containers` and `scrolls`. Returns every problem found.
pub fn validate(p: &Project) -> Vec<String> {
    let mut errs = Vec::new();
    if p.containers.is_empty() && p.scrolls.is_empty() {
        return errs;
    }
    let mut ids: HashMap<&str, usize> = HashMap::new();
    for (i, c) in p.containers.iter().enumerate() {
        if c.id.trim().is_empty() {
            errs.push("container needs an id".into());
        } else if ids.insert(c.id.as_str(), i).is_some() {
            errs.push(format!("duplicate container id '{}'", c.id));
        }
    }
    let total_members: usize = p.containers.iter().map(|c| c.members.len()).sum();
    if total_members > MAX_MEMBERS {
        errs.push(format!("at most {MAX_MEMBERS} container members"));
        return errs;
    }
    let mut layer_owner: HashMap<&str, &str> = HashMap::new();
    let mut cont_owner: HashMap<&str, &str> = HashMap::new();
    for c in &p.containers {
        let who = format!("container '{}'", c.id);
        if !(finite(c.gap) && c.gap >= 0.0) {
            errs.push(format!("{who}: gap must be a finite number >= 0"));
        }
        if let Some(g) = c.cross_gap {
            if !(finite(g) && g >= 0.0) {
                errs.push(format!("{who}: cross_gap must be a finite number >= 0"));
            }
        }
        if !c.padding.iter().all(|v| finite(*v) && *v >= 0.0) {
            errs.push(format!("{who}: padding values must be finite and >= 0"));
        }
        check_rule(&c.rect, &who, &mut errs);
        for m in &c.members {
            let kinds = m.layer.is_some() as u8 + m.container.is_some() as u8 + m.spacer as u8;
            if kinds != 1 {
                errs.push(format!(
                    "{who}: a member needs exactly one of layer, container, spacer"
                ));
                continue;
            }
            let ml = m.label();
            if !(finite(m.grow) && m.grow >= 0.0) || !(finite(m.shrink) && m.shrink >= 0.0) {
                errs.push(format!(
                    "{who}: {ml}: grow and shrink must be finite numbers >= 0"
                ));
            }
            for (n, v) in [("width", m.width), ("height", m.height)] {
                if let Some(v) = v {
                    if !(finite(v) && v > 0.0) {
                        errs.push(format!("{who}: {ml}: {n} must be a finite number > 0"));
                    }
                }
            }
            for (a, (lo, hi)) in [(0, (m.min_w, m.max_w)), (1, (m.min_h, m.max_h))] {
                let n = if a == 0 { "w" } else { "h" };
                if let Some(v) = lo {
                    if !(finite(v) && v >= 0.0) {
                        errs.push(format!("{who}: {ml}: min_{n} must be a finite number >= 0"));
                    }
                }
                if let Some(v) = hi {
                    if !(finite(v) && v > 0.0) {
                        errs.push(format!("{who}: {ml}: max_{n} must be a finite number > 0"));
                    }
                }
                if let (Some(l), Some(h)) = (lo, hi) {
                    if finite(l) && finite(h) && l > h {
                        errs.push(format!("{who}: {ml}: min_{n} is larger than max_{n}"));
                    }
                }
            }
            if let Some(path) = &m.layer {
                match p.find_layer(path) {
                    None => errs.push(format!("{who}: unknown layer '{path}'")),
                    Some(l) => {
                        if let Some(prev) = layer_owner.insert(path.as_str(), c.id.as_str()) {
                            errs.push(format!(
                                "layer '{path}' is a member of both container '{prev}' and '{}'",
                                c.id
                            ));
                        }
                        let measurable = layer_bounds(l).is_some();
                        if !measurable && (m.width.is_none() || m.height.is_none()) {
                            errs.push(format!("{who}: layer '{path}' has no measurable geometry; give width and height"));
                        }
                        if m.resize && !layer_is_stretchable(l) {
                            errs.push(format!("{who}: layer '{path}': resize needs measurable geometry and no rotation or skew"));
                        }
                    }
                }
            }
            if let Some(id) = &m.container {
                if !ids.contains_key(id.as_str()) {
                    errs.push(format!("{who}: unknown container '{id}'"));
                } else if let Some(prev) = cont_owner.insert(id.as_str(), c.id.as_str()) {
                    errs.push(format!(
                        "container '{id}' is nested in both '{prev}' and '{}'",
                        c.id
                    ));
                }
            }
        }
    }
    // Cycles in nesting (including self-nesting).
    let mut state: HashMap<&str, u8> = HashMap::new();
    fn visit<'a>(
        p: &'a Project,
        ids: &HashMap<&str, usize>,
        id: &'a str,
        state: &mut HashMap<&'a str, u8>,
        errs: &mut Vec<String>,
    ) {
        match state.get(id) {
            Some(2) => return,
            Some(1) => {
                errs.push(format!("container nesting cycle through '{id}'"));
                return;
            }
            _ => {}
        }
        state.insert(id, 1);
        if let Some(&i) = ids.get(id) {
            for m in &p.containers[i].members {
                if let Some(ch) = &m.container {
                    if ids.contains_key(ch.as_str()) {
                        visit(p, ids, ch, state, errs);
                    }
                }
            }
        }
        state.insert(id, 2);
    }
    for c in &p.containers {
        visit(p, &ids, &c.id, &mut state, &mut errs);
    }
    if !errs.is_empty() {
        errs.sort();
        errs.dedup();
    }
    // Scrolls.
    let cycle_free = !errs.iter().any(|e| e.contains("cycle"));
    let mut sids = HashSet::new();
    let mut contents: HashMap<&str, &str> = HashMap::new();
    for s in &p.scrolls {
        let who = format!("scroll '{}'", s.id);
        if s.id.trim().is_empty() {
            errs.push("scroll needs an id".into());
        } else if !sids.insert(s.id.as_str()) {
            errs.push(format!("duplicate scroll id '{}'", s.id));
        }
        check_rule(&s.viewport, &who, &mut errs);
        if matches!(s.viewport, RectRule::Hug { .. }) {
            errs.push(format!("{who}: the viewport cannot be a hug rectangle"));
        }
        if !ids.contains_key(s.content.as_str()) {
            errs.push(format!("{who}: unknown content container '{}'", s.content));
        } else {
            if cont_owner.contains_key(s.content.as_str()) {
                errs.push(format!(
                    "{who}: content container '{}' is nested in another container",
                    s.content
                ));
            }
            if let Some(prev) = contents.insert(s.content.as_str(), s.id.as_str()) {
                errs.push(format!(
                    "container '{}' is the content of both scroll '{prev}' and '{}'",
                    s.content, s.id
                ));
            }
        }
        if !s.offset.iter().all(|v| finite(*v)) {
            errs.push(format!("{who}: offset must be finite"));
        }
        if !(finite(s.fling) && s.fling >= 0.0) {
            errs.push(format!("{who}: fling must be a finite number >= 0"));
        }
        if let Some(e) = &s.elastic {
            if !(finite(e.limit)
                && e.limit > 0.0
                && finite(e.stiffness)
                && e.stiffness > 0.0
                && finite(e.damping_ratio)
                && e.damping_ratio > 0.0)
            {
                errs.push(format!(
                    "{who}: elastic limit, stiffness and damping_ratio must be finite numbers > 0"
                ));
            }
        }
        if !s.snap_x.iter().chain(&s.snap_y).all(|v| finite(*v)) {
            errs.push(format!("{who}: snap points must be finite"));
        }
        if let Some(cl) = &s.clip_layer {
            match p.find_layer(cl) {
                None => errs.push(format!("{who}: unknown clip layer '{cl}'")),
                Some(l) => {
                    if !layer_is_stretchable(l) {
                        errs.push(format!("{who}: clip layer '{cl}' needs measurable geometry and no rotation or skew"));
                    }
                    if layer_owner.contains_key(cl.as_str()) {
                        errs.push(format!(
                            "{who}: clip layer '{cl}' must not be a container member"
                        ));
                    }
                }
            }
        }
    }
    if cycle_free && errs.is_empty() {
        for s in &p.scrolls {
            let mut members = Vec::new();
            collect_layers(p, &ids, &s.content, &mut members, 0);
            for path in members {
                if let Some(l) = p.find_layer(path) {
                    let already = s
                        .clip_layer
                        .as_ref()
                        .map(|c| {
                            l.mask.as_ref().map(|m| &m.layer) == Some(c)
                                && !l.mask.as_ref().unwrap().invert
                        })
                        .unwrap_or(false);
                    if s.clip_layer.is_some() && l.mask.is_some() && !already {
                        errs.push(format!("scroll '{}': layer '{path}' already has a mask, so it cannot be clipped", s.id));
                    }
                }
                if Some(path) == s.clip_layer.as_deref() {
                    errs.push(format!(
                        "scroll '{}': the clip layer is part of its own content",
                        s.id
                    ));
                }
            }
        }
    }
    errs.sort();
    errs.dedup();
    errs
}

/// Layer paths of a container and everything nested in it.
fn collect_layers<'a>(
    p: &'a Project,
    ids: &HashMap<&str, usize>,
    id: &str,
    out: &mut Vec<&'a str>,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let Some(&i) = ids.get(id) else { return };
    for m in &p.containers[i].members {
        if let Some(l) = &m.layer {
            out.push(l.as_str());
        }
        if let Some(c) = &m.container {
            collect_layers(p, ids, c, out, depth + 1);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Rectangles
// ---------------------------------------------------------------------------------------------

fn pin_axis(lo: Option<f32>, hi: Option<f32>, size: Option<f32>, total: f32) -> (f32, f32) {
    match (lo, hi, size) {
        (Some(l), Some(h), _) => (l, (total - l - h).max(0.0)),
        (Some(l), None, Some(s)) => (l, s),
        (None, Some(h), Some(s)) => (total - h - s, s),
        (None, None, Some(s)) => ((total - s) * 0.5, s),
        (Some(l), None, None) => (l, (total - l).max(0.0)),
        (None, Some(h), None) => (0.0, (total - h).max(0.0)),
        (None, None, None) => (0.0, total),
    }
}

/// Resolve a rule against a canvas; `hug` is the content size used by `Hug`.
pub fn resolve_rule(rule: &RectRule, cw: f32, ch: f32, hug: [f32; 2]) -> [f32; 4] {
    match rule {
        RectRule::Fixed { x, y, w, h } => [*x, *y, *w, *h],
        RectRule::Pin {
            left,
            top,
            right,
            bottom,
            width,
            height,
        } => {
            let (x, w) = pin_axis(*left, *right, *width, cw);
            let (y, h) = pin_axis(*top, *bottom, *height, ch);
            [x, y, w, h]
        }
        RectRule::Aspect {
            ratio,
            mode,
            margin,
            ax,
            ay,
        } => {
            let (aw, ah) = ((cw - 2.0 * margin).max(0.0), (ch - 2.0 * margin).max(0.0));
            let wider = ah > 0.0 && aw / ah > *ratio;
            let (w, h) = match (mode, wider) {
                (AspectMode::Fit, true) | (AspectMode::Fill, false) => (ah * ratio, ah),
                _ => (aw, aw / ratio),
            };
            [margin + (aw - w) * ax, margin + (ah - h) * ay, w, h]
        }
        RectRule::Hug { x, y } => [*x, *y, hug[0], hug[1]],
    }
}

// ---------------------------------------------------------------------------------------------
// Solver
// ---------------------------------------------------------------------------------------------

struct Solver<'a> {
    p: &'a Project,
    ids: HashMap<&'a str, usize>,
    /// Scroll that owns a container as its content.
    scroll_of: HashMap<&'a str, usize>,
    sol: Solution,
}

/// A measured member.
struct Prep<'a> {
    m: &'a Member,
    nat: [f32; 2],
    bounds: Option<(Vec2, Vec2)>,
}

fn dirx(d: Dir) -> usize {
    if d == Dir::Row {
        0
    } else {
        1
    }
}

/// Share `avail` over items: grow when there is room, shrink (weighted by basis) when there is
/// not; members at their min / max are frozen and the remainder is re-shared.
fn distribute(
    basis: &[f32],
    grow: &[f32],
    shrink: &[f32],
    lo: &[f32],
    hi: &[f32],
    avail: f32,
) -> Vec<f32> {
    let n = basis.len();
    let mut size: Vec<f32> = basis.to_vec();
    let total: f32 = basis.iter().sum();
    let growing = avail > total;
    if !growing && avail >= total {
        return size;
    }
    let weight: Vec<f32> = (0..n)
        .map(|i| {
            if growing {
                grow[i]
            } else {
                shrink[i] * basis[i]
            }
        })
        .collect();
    let mut frozen: Vec<bool> = weight.iter().map(|w| *w <= 0.0).collect();
    for _ in 0..=n {
        let used: f32 = (0..n)
            .map(|i| if frozen[i] { size[i] } else { basis[i] })
            .sum();
        let free = avail - used;
        let wsum: f32 = (0..n).filter(|&i| !frozen[i]).map(|i| weight[i]).sum();
        if wsum <= 0.0 || free.abs() < 1e-6 {
            for i in 0..n {
                if !frozen[i] {
                    size[i] = basis[i];
                }
            }
            break;
        }
        let mut violated = false;
        let mut tent = vec![0.0f32; n];
        for i in 0..n {
            if frozen[i] {
                continue;
            }
            let t = basis[i] + free * weight[i] / wsum;
            let c = t.max(lo[i]).min(hi[i]);
            tent[i] = t;
            if (c - t).abs() > 1e-6 {
                violated = true;
            }
        }
        if !violated {
            for i in 0..n {
                if !frozen[i] {
                    size[i] = tent[i];
                }
            }
            break;
        }
        // Freeze every violator at its clamped value and go round again.
        for i in 0..n {
            if !frozen[i] {
                let c = tent[i].max(lo[i]).min(hi[i]);
                if (c - tent[i]).abs() > 1e-6 {
                    size[i] = c;
                    frozen[i] = true;
                }
            }
        }
    }
    size
}

impl<'a> Solver<'a> {
    fn new(p: &'a Project) -> Solver<'a> {
        let ids = p
            .containers
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id.as_str(), i))
            .collect();
        let scroll_of = p
            .scrolls
            .iter()
            .enumerate()
            .map(|(i, s)| (s.content.as_str(), i))
            .collect();
        Solver {
            p,
            ids,
            scroll_of,
            sol: Solution::default(),
        }
    }

    fn prep(&self, c: &'a Container, depth: usize) -> Vec<Prep<'a>> {
        c.members
            .iter()
            .map(|m| {
                let (mut nat, mut bounds) = ([0.0f32; 2], None);
                if let Some(path) = &m.layer {
                    if let Some(l) = self.p.find_layer(path) {
                        bounds = layer_bounds(l);
                        match bounds {
                            Some((lo, hi)) => nat = [hi.x - lo.x, hi.y - lo.y],
                            None => bounds = None,
                        }
                    }
                } else if let Some(id) = &m.container {
                    nat = self.measure(self.ids[id.as_str()], [None, None], depth + 1);
                }
                // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
                #[allow(clippy::needless_range_loop)]
                for a in 0..2 {
                    if let Some(v) = m.fixed(a) {
                        nat[a] = v;
                    }
                }
                Prep { m, nat, bounds }
            })
            .collect()
    }

    /// Break members into lines along the main axis.
    fn lines(&self, c: &Container, preps: &[Prep], inner_main: Option<f32>) -> Vec<Vec<usize>> {
        let m = dirx(c.axis);
        let limit = match (c.wrap, inner_main) {
            (Wrap::Wrap, Some(l)) => l,
            _ => {
                return if preps.is_empty() {
                    vec![]
                } else {
                    vec![(0..preps.len()).collect()]
                }
            }
        };
        let mut lines: Vec<Vec<usize>> = Vec::new();
        let mut cur: Vec<usize> = Vec::new();
        let mut sum = 0.0f32;
        for (i, pr) in preps.iter().enumerate() {
            let b = pr.m.clamp(m, pr.nat[m]);
            let need = b + if cur.is_empty() { 0.0 } else { c.gap };
            if !cur.is_empty() && sum + need > limit + 1e-4 {
                lines.push(std::mem::take(&mut cur));
                sum = 0.0;
            }
            sum += b + if cur.is_empty() { 0.0 } else { c.gap };
            cur.push(i);
        }
        if !cur.is_empty() {
            lines.push(cur);
        }
        lines
    }

    /// Intrinsic size (including padding) of a container; `avail` limits the wrapping width.
    fn measure(&self, ci: usize, avail: [Option<f32>; 2], depth: usize) -> [f32; 2] {
        if depth > MAX_DEPTH {
            return [0.0, 0.0];
        }
        let c = &self.p.containers[ci];
        let m = dirx(c.axis);
        let x = 1 - m;
        let pad_main = if m == 0 {
            c.padding[0] + c.padding[2]
        } else {
            c.padding[1] + c.padding[3]
        };
        let pad_cross = if m == 0 {
            c.padding[1] + c.padding[3]
        } else {
            c.padding[0] + c.padding[2]
        };
        let preps = self.prep(c, depth);
        let inner_main = avail[m].map(|a| (a - pad_main).max(0.0));
        let lines = self.lines(c, &preps, inner_main);
        let cross_gap = c.cross_gap.unwrap_or(c.gap);
        let (mut main_ext, mut cross_ext) = (0.0f32, 0.0f32);
        for (j, line) in lines.iter().enumerate() {
            let mut sum = 0.0;
            let mut cross = 0.0f32;
            for (k, &i) in line.iter().enumerate() {
                sum += preps[i].m.clamp(m, preps[i].nat[m]) + if k > 0 { c.gap } else { 0.0 };
                cross = cross.max(preps[i].m.clamp(x, preps[i].nat[x]));
            }
            main_ext = main_ext.max(sum);
            cross_ext += cross + if j > 0 { cross_gap } else { 0.0 };
        }
        let mut out = [0.0; 2];
        out[m] = main_ext + pad_main;
        out[x] = cross_ext + pad_cross;
        out
    }

    fn place(&mut self, path: &str, pr: &Prep, slot: [f32; 4]) -> Result<(), String> {
        let l = self
            .p
            .find_layer(path)
            .ok_or_else(|| format!("unknown layer '{path}'"))?;
        let pl = placement(path, l, pr.bounds, slot, pr.m.resize)?;
        self.sol.layers.push(pl);
        Ok(())
    }

    fn layout(&mut self, ci: usize, rect: [f32; 4], depth: usize) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err("container nesting is too deep".into());
        }
        let p = self.p;
        let c: &'a Container = &p.containers[ci];
        self.sol.rects.push((c.id.clone(), rect));
        let m = dirx(c.axis);
        let x = 1 - m;
        let [pl, pt, pr_, pb] = c.padding;
        let inner = [(rect[2] - pl - pr_).max(0.0), (rect[3] - pt - pb).max(0.0)];
        let origin = [rect[0] + pl, rect[1] + pt];
        let preps = self.prep(c, depth);
        let lines = self.lines(c, &preps, Some(inner[m]));
        let cross_gap = c.cross_gap.unwrap_or(c.gap);
        let single = lines.len() == 1 && c.wrap == Wrap::None;
        let mut cross_cursor = 0.0f32;
        for line in &lines {
            let n = line.len();
            let line_cross = if single {
                inner[x]
            } else {
                line.iter()
                    .map(|&i| preps[i].m.clamp(x, preps[i].nat[x]))
                    .fold(0.0f32, f32::max)
            };
            let basis: Vec<f32> = line
                .iter()
                .map(|&i| preps[i].m.clamp(m, preps[i].nat[m]))
                .collect();
            let grow: Vec<f32> = line.iter().map(|&i| preps[i].m.grow).collect();
            let shrink: Vec<f32> = line.iter().map(|&i| preps[i].m.shrink).collect();
            let lo: Vec<f32> = line.iter().map(|&i| preps[i].m.limits(m).0).collect();
            let hi: Vec<f32> = line.iter().map(|&i| preps[i].m.limits(m).1).collect();
            let avail = inner[m] - c.gap * (n.saturating_sub(1)) as f32;
            let sizes = distribute(&basis, &grow, &shrink, &lo, &hi, avail);
            let free = (avail - sizes.iter().sum::<f32>()).max(0.0);
            let (start, extra) = match c.justify {
                Justify::Start => (0.0, 0.0),
                Justify::Center => (free * 0.5, 0.0),
                Justify::End => (free, 0.0),
                Justify::SpaceBetween if n > 1 => (0.0, free / (n - 1) as f32),
                Justify::SpaceBetween => (0.0, 0.0),
                Justify::SpaceAround => (free / n as f32 * 0.5, free / n as f32),
            };
            let mut cursor = start;
            for (k, &i) in line.iter().enumerate() {
                let pr = &preps[i];
                let al = pr.m.align.unwrap_or(c.align);
                let free_cross = pr.m.fixed(x).is_none();
                let resizable = pr.m.resize || pr.m.container.is_some() || pr.m.spacer;
                let cs = if al == Align::Stretch && free_cross && resizable {
                    pr.m.clamp(x, line_cross)
                } else {
                    pr.m.clamp(x, pr.nat[x])
                };
                let coff = match al {
                    Align::Start | Align::Stretch => 0.0,
                    Align::Center => (line_cross - cs) * 0.5,
                    Align::End => line_cross - cs,
                };
                let mut slot = [0.0f32; 4];
                slot[m] = origin[m] + cursor;
                slot[x] = origin[x] + cross_cursor + coff;
                slot[2 + m] = sizes[k];
                slot[2 + x] = cs;
                cursor += sizes[k] + c.gap + extra;
                if let Some(path) = &pr.m.layer {
                    self.place(path, pr, slot)?;
                } else if let Some(id) = &pr.m.container {
                    let child = self.ids[id.as_str()];
                    self.layout(child, slot, depth + 1)?;
                }
            }
            cross_cursor += line_cross + cross_gap;
        }
        Ok(())
    }
}

/// Offset / scale of a layer so that its rest bounding box lands in `slot` (resize) or at the
/// slot origin (no resize).
fn placement(
    path: &str,
    l: &Layer,
    bounds: Option<(Vec2, Vec2)>,
    slot: [f32; 4],
    resize: bool,
) -> Result<Placement, String> {
    let t = &l.transform;
    match bounds {
        Some((lo, hi)) => {
            if resize {
                let (cl, ch) = content_bounds(l)
                    .ok_or_else(|| format!("layer '{path}' has no measurable geometry"))?;
                let (w, h) = (hi.x - lo.x, hi.y - lo.y);
                let kx = if w > 1e-6 {
                    (slot[2] / w).max(1e-4)
                } else {
                    1.0
                };
                let ky = if h > 1e-6 {
                    (slot[3] / h).max(1e-4)
                } else {
                    1.0
                };
                let (sx, sy) = (t.scale.x * kx, t.scale.y * ky);
                let minx = (sx * (cl.x - t.anchor.x)).min(sx * (ch.x - t.anchor.x));
                let miny = (sy * (cl.y - t.anchor.y)).min(sy * (ch.y - t.anchor.y));
                Ok(Placement {
                    layer: path.into(),
                    dx: slot[0] - minx - t.position.x,
                    dy: slot[1] - miny - t.position.y,
                    kx,
                    ky,
                })
            } else {
                Ok(Placement {
                    layer: path.into(),
                    dx: slot[0] - lo.x,
                    dy: slot[1] - lo.y,
                    kx: 1.0,
                    ky: 1.0,
                })
            }
        }
        None => Ok(Placement {
            layer: path.into(),
            dx: slot[0] - t.position.x,
            dy: slot[1] - t.position.y,
            kx: 1.0,
            ky: 1.0,
        }),
    }
}

/// Offset actually used for a raw request: clamped, or rubber-banded past the ends.
pub fn effective_offset(raw: f32, max: f32, elastic: Option<&Elastic>) -> f32 {
    let max = max.max(0.0);
    match elastic {
        None => raw.max(0.0).min(max),
        Some(e) => {
            let rub = |o: f32| e.limit * (1.0 - (-o / e.limit).exp());
            if raw < 0.0 {
                -rub(-raw)
            } else if raw > max {
                max + rub(raw - max)
            } else {
                raw
            }
        }
    }
}

/// Inverse of [`effective_offset`] on the displayed side: the raw request that displays as `shown`
/// (a rubber-banded overscroll is only ever shown below `limit`). Used when a displayed position
/// (for example from a [`ScrollRuntime`]) must be written back as a scroll's requested offset.
pub fn raw_offset_for(shown: f32, max: f32, elastic: Option<&Elastic>) -> f32 {
    let max = max.max(0.0);
    let inv = |r: f32, e: &Elastic| -e.limit * (1.0 - (r / e.limit).min(0.999_999)).ln();
    match elastic {
        None => shown.max(0.0).min(max),
        Some(e) if shown < 0.0 => -inv(-shown, e),
        Some(e) if shown > max => max + inv(shown - max, e),
        Some(_) => shown,
    }
}

/// Solve every container and scroll for a canvas of `w` x `h`.
pub fn solve(p: &Project, w: f32, h: f32) -> Result<Solution, String> {
    let errs = validate(p);
    if !errs.is_empty() {
        return Err(errs.join("\n"));
    }
    if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
        return Err("canvas size must be finite and > 0".into());
    }
    let mut s = Solver::new(p);
    s.sol.canvas = (w, h);
    let members: HashSet<&str> = p
        .containers
        .iter()
        .flat_map(|c| c.members.iter().filter_map(|m| m.container.as_deref()))
        .collect();
    let mut infos: Vec<(usize, ScrollInfo)> = Vec::new();
    for (ci, c) in p.containers.iter().enumerate() {
        if members.contains(c.id.as_str()) {
            continue;
        }
        if let Some(&si) = s.scroll_of.get(c.id.as_str()) {
            let sc = &p.scrolls[si];
            let vp = resolve_rule(&sc.viewport, w, h, [0.0, 0.0]);
            let avail = [
                if sc.scrolls(0) { None } else { Some(vp[2]) },
                if sc.scrolls(1) { None } else { Some(vp[3]) },
            ];
            let intr = s.measure(ci, avail, 0);
            let mut size = [vp[2], vp[3]];
            let mut max = [0.0f32; 2];
            let mut off = [0.0f32; 2];
            for a in 0..2 {
                if sc.scrolls(a) {
                    size[a] = intr[a].max(vp[2 + a]);
                    max[a] = (size[a] - vp[2 + a]).max(0.0);
                    off[a] = effective_offset(sc.offset[a], max[a], sc.elastic.as_ref());
                }
            }
            let rect = [vp[0] - off[0], vp[1] - off[1], size[0], size[1]];
            s.layout(ci, rect, 0)?;
            infos.push((
                si,
                ScrollInfo {
                    id: sc.id.clone(),
                    viewport: vp,
                    content: rect,
                    max,
                    offset: off,
                },
            ));
            if let Some(cl) = &sc.clip_layer {
                let l = p
                    .find_layer(cl)
                    .ok_or_else(|| format!("unknown clip layer '{cl}'"))?;
                let pl = placement(cl, l, layer_bounds(l), vp, true)?;
                s.sol.layers.push(pl);
            }
        } else {
            let hug = s.measure(ci, [None, None], 0);
            let rect = resolve_rule(&c.rect, w, h, hug);
            s.layout(ci, rect, 0)?;
        }
    }
    infos.sort_by_key(|(i, _)| *i);
    s.sol.scrolls = infos.into_iter().map(|(_, i)| i).collect();
    for pl in &s.sol.layers {
        if ![pl.dx, pl.dy, pl.kx, pl.ky].iter().all(|v| v.is_finite()) {
            return Err(format!("non-finite value in the layout of layer '{}' (a size, padding or gap is too large)", pl.layer));
        }
    }
    for sc in &s.sol.scrolls {
        if ![
            sc.viewport.as_slice(),
            sc.content.as_slice(),
            sc.max.as_slice(),
            sc.offset.as_slice(),
        ]
        .concat()
        .iter()
        .all(|v| v.is_finite())
        {
            return Err(format!("non-finite value in the layout of scroll '{}' (a size, padding or gap is too large)", sc.id));
        }
    }
    Ok(s.sol)
}

/// Solve for the project's own canvas size.
pub fn solve_project(p: &Project) -> Result<Solution, String> {
    solve(p, p.width, p.height)
}

/// Layout offsets per engine node id (`[dx, dy, kx, ky]`), for the project's own canvas.
pub(crate) fn node_offsets(
    engine: &crate::engine::Engine,
) -> Result<Vec<Option<[f32; 4]>>, String> {
    let sol = solve_project(&engine.project)?;
    let mut out = vec![None; engine.node_count()];
    for pl in &sol.layers {
        let id = engine
            .node_index(&pl.layer)
            .ok_or_else(|| format!("unknown layer '{}'", pl.layer))?;
        out[id] = Some([pl.dx, pl.dy, pl.kx, pl.ky]);
    }
    Ok(out)
}

/// Add the clip mask to every content layer of a scroll that names a `clip_layer`.
/// Call after [`validate`] succeeded.
pub fn apply_clip_masks(p: &mut Project) {
    let ids: HashMap<&str, usize> = p
        .containers
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), i))
        .collect();
    let mut todo: Vec<(String, String)> = Vec::new();
    for s in &p.scrolls {
        if let Some(cl) = &s.clip_layer {
            let mut v = Vec::new();
            collect_layers(p, &ids, &s.content, &mut v, 0);
            todo.extend(v.into_iter().map(|l| (l.to_string(), cl.clone())));
        }
    }
    for (path, clip) in todo {
        if let Some(l) = p.find_layer_mut(&path) {
            l.mask = Some(MaskRef {
                layer: clip,
                invert: false,
            });
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Responsive canvas
// ---------------------------------------------------------------------------------------------

/// A copy of the project with another canvas size. Containers re-solve against it when the
/// copy is compiled; nothing else in the project changes.
pub fn responsive(p: &Project, w: f32, h: f32) -> Result<Project, String> {
    if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
        return Err("size must be two numbers > 0".into());
    }
    if w > crate::engine::MAX_STAGE || h > crate::engine::MAX_STAGE {
        return Err(format!("size must be at most {}", crate::engine::MAX_STAGE));
    }
    let mut q = p.clone();
    q.width = w;
    q.height = h;
    Ok(q)
}

/// Parse `WxH` (also `W×H`, `W,H`).
pub fn parse_size(s: &str) -> Result<(f32, f32), String> {
    let t = s.replace(['X', '×', ','], "x");
    let (a, b) = t
        .split_once(['x'])
        .ok_or_else(|| format!("size must look like 800x600, got '{s}'"))?;
    let w: f32 = a
        .trim()
        .parse()
        .map_err(|_| format!("bad width in '{s}'"))?;
    let h: f32 = b
        .trim()
        .parse()
        .map_err(|_| format!("bad height in '{s}'"))?;
    if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
        return Err(format!("size must be positive, got '{s}'"));
    }
    Ok((w, h))
}

/// Set scroll offsets from external values: each scroll with an `input` path takes that value
/// (pixels) along its main axis (y for `both`). Unknown paths are ignored.
pub fn apply_inputs(p: &Project, values: &HashMap<String, f64>) -> Project {
    let mut q = p.clone();
    for s in &mut q.scrolls {
        if let Some(v) = s.input.as_ref().and_then(|i| values.get(i)) {
            if v.is_finite() {
                match s.axis {
                    ScrollAxis::X => s.offset[0] = *v as f32,
                    _ => s.offset[1] = *v as f32,
                }
            }
        }
    }
    q
}

/// Set the requested offset of a scroll by id.
pub fn set_offset(p: &mut Project, id: &str, offset: [f32; 2]) -> Result<(), String> {
    if !offset.iter().all(|v| v.is_finite()) {
        return Err("offset must be finite".into());
    }
    let s = p
        .scrolls
        .iter_mut()
        .find(|s| s.id == id)
        .ok_or_else(|| format!("no scroll '{id}'"))?;
    s.offset = offset;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Scroll runtime
// ---------------------------------------------------------------------------------------------

/// Nearest snap point (points are clamped into `0..=max`); no points = plain clamp.
pub fn snap_to(v: f32, points: &[f32], max: f32) -> f32 {
    let max = max.max(0.0);
    let v = v.max(0.0).min(max);
    let mut best: Option<f32> = None;
    for &q in points {
        let q = q.max(0.0).min(max);
        match best {
            Some(b) if (b - v).abs() <= (q - v).abs() => {}
            _ => best = Some(q),
        }
    }
    best.unwrap_or(v)
}

/// Deterministic scrolling simulation: drag (with rubber band), release with velocity (momentum
/// lands on the nearest snap point), spring return. Fixed 1/240 s sub-steps.
#[derive(Clone, Debug)]
pub struct ScrollRuntime {
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    target: Option<[f32; 2]>,
    max: [f32; 2],
    scroll: Scroll,
}

impl ScrollRuntime {
    /// `max` is the largest useful offset per axis (see [`ScrollInfo::max`]).
    pub fn new(scroll: &Scroll, max: [f32; 2]) -> ScrollRuntime {
        let mut r = ScrollRuntime {
            pos: [0.0; 2],
            vel: [0.0; 2],
            target: None,
            max,
            scroll: scroll.clone(),
        };
        r.drag_to(scroll.offset);
        r
    }

    pub fn is_settled(&self) -> bool {
        self.target.is_none()
    }

    /// Replace the largest useful offset (after the canvas or the content changed). The current
    /// position is re-clamped (or left in its rubber band) accordingly.
    pub fn set_max(&mut self, max: [f32; 2]) {
        let raw =
            [0, 1].map(|a| raw_offset_for(self.pos[a], self.max[a], self.scroll.elastic.as_ref()));
        self.max = max;
        if self.target.is_none() {
            self.drag_to(raw);
        } else {
            for a in 0..2 {
                self.pos[a] = self.pos[a].max(0.0).min(max[a].max(0.0));
                if let Some(t) = &mut self.target {
                    t[a] = t[a].max(0.0).min(max[a].max(0.0));
                }
            }
        }
    }

    /// The largest useful offset per axis currently used.
    pub fn max(&self) -> [f32; 2] {
        self.max
    }

    /// Finger / pointer position: the raw offset the user asks for. Past the ends the displayed
    /// position is rubber-banded (elastic) or clamped.
    pub fn drag_to(&mut self, raw: [f32; 2]) {
        // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
        #[allow(clippy::needless_range_loop)]
        for a in 0..2 {
            self.pos[a] = if self.scroll.scrolls(a) {
                effective_offset(
                    if raw[a].is_finite() { raw[a] } else { 0.0 },
                    self.max[a],
                    self.scroll.elastic.as_ref(),
                )
            } else {
                0.0
            };
            self.vel[a] = 0.0;
        }
        self.target = None;
    }

    /// Let go with a release velocity (pixels per second). Momentum is projected `fling` seconds
    /// ahead, clamped and snapped; the position then springs to that landing offset.
    pub fn release(&mut self, velocity: [f32; 2]) {
        let mut t = [0.0f32; 2];
        for a in 0..2 {
            if !self.scroll.scrolls(a) {
                continue;
            }
            let v = if velocity[a].is_finite() {
                velocity[a]
            } else {
                0.0
            };
            let proj = self.pos[a] + v * self.scroll.fling;
            let pts = if a == 0 {
                &self.scroll.snap_x
            } else {
                &self.scroll.snap_y
            };
            t[a] = snap_to(proj, pts, self.max[a]);
        }
        self.vel = [0.0; 2];
        self.target = Some(t);
    }

    pub fn step(&mut self, dt: f32) {
        let Some(t) = self.target else { return };
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
        let (k, zeta) = match &self.scroll.elastic {
            Some(e) => (e.stiffness, e.damping_ratio),
            None => (def_stiffness(), 1.0),
        };
        let c = 2.0 * zeta * k.sqrt();
        let n = ((dt * 240.0).ceil() as usize).clamp(1, 100_000);
        let h = dt / n as f32;
        for _ in 0..n {
            // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
            #[allow(clippy::needless_range_loop)]
            for a in 0..2 {
                let acc = -k * (self.pos[a] - t[a]) - c * self.vel[a];
                self.vel[a] += acc * h;
                self.pos[a] += self.vel[a] * h;
            }
        }
        if (0..2).all(|a| (self.pos[a] - t[a]).abs() < 1e-3 && self.vel[a].abs() < 1e-2) {
            self.pos = t;
            self.vel = [0.0; 2];
            self.target = None;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Editing (project-level; each returns an undoable editor op)
// ---------------------------------------------------------------------------------------------

fn vec_op<T: Serialize>(field: &str, present: bool, v: &T) -> Result<crate::edit::Op, String> {
    let value = serde_json::to_value(v).map_err(|e| e.to_string())?;
    Ok(if present {
        crate::edit::Op::Insert {
            path: format!("/{field}/-"),
            value,
        }
    } else {
        crate::edit::Op::Set {
            path: format!("/{field}"),
            value: serde_json::json!([value]),
        }
    })
}

fn check_trial(trial: &Project) -> Result<(), String> {
    let errs = validate(trial);
    if !errs.is_empty() {
        return Err(errs.join("\n"));
    }
    solve_project(trial).map(|_| ())
}

/// Add a container to a project (validated).
pub fn add_container(p: &mut Project, c: Container) -> Result<(), String> {
    if p.containers.iter().any(|x| x.id == c.id) {
        return Err(format!("container '{}' already exists", c.id));
    }
    let mut t = p.clone();
    t.containers.push(c.clone());
    check_trial(&t)?;
    p.containers.push(c);
    Ok(())
}

/// Remove a container; refused while something still refers to it.
pub fn remove_container(p: &mut Project, id: &str) -> Result<(), String> {
    let i = p
        .containers
        .iter()
        .position(|c| c.id == id)
        .ok_or_else(|| format!("no container '{id}'"))?;
    let mut t = p.clone();
    t.containers.remove(i);
    check_trial(&t)?;
    p.containers.remove(i);
    Ok(())
}

pub fn add_scroll(p: &mut Project, s: Scroll) -> Result<(), String> {
    if p.scrolls.iter().any(|x| x.id == s.id) {
        return Err(format!("scroll '{}' already exists", s.id));
    }
    let mut t = p.clone();
    t.scrolls.push(s.clone());
    check_trial(&t)?;
    p.scrolls.push(s);
    Ok(())
}

pub fn remove_scroll(p: &mut Project, id: &str) -> Result<(), String> {
    let i = p
        .scrolls
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| format!("no scroll '{id}'"))?;
    p.scrolls.remove(i);
    Ok(())
}

/// The same as [`add_container`] as one editor operation (one undo step).
pub fn add_container_op(p: &Project, c: Container) -> Result<crate::edit::Op, String> {
    let mut t = p.clone();
    add_container(&mut t, c.clone())?;
    vec_op("containers", !p.containers.is_empty(), &c)
}

pub fn remove_container_op(p: &Project, id: &str) -> Result<crate::edit::Op, String> {
    let i = p
        .containers
        .iter()
        .position(|c| c.id == id)
        .ok_or_else(|| format!("no container '{id}'"))?;
    let mut t = p.clone();
    remove_container(&mut t, id)?;
    Ok(crate::edit::Op::Remove {
        path: format!("/containers/{i}"),
    })
}

pub fn add_scroll_op(p: &Project, s: Scroll) -> Result<crate::edit::Op, String> {
    let mut t = p.clone();
    add_scroll(&mut t, s.clone())?;
    vec_op("scrolls", !p.scrolls.is_empty(), &s)
}

pub fn remove_scroll_op(p: &Project, id: &str) -> Result<crate::edit::Op, String> {
    let i = p
        .scrolls
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| format!("no scroll '{id}'"))?;
    Ok(crate::edit::Op::Remove {
        path: format!("/scrolls/{i}"),
    })
}

/// SVG overlay (outlines of every container and viewport) for previews.
pub fn overlay_svg(sol: &Solution) -> String {
    let mut s = String::from(
        "<g fill=\"none\" stroke-width=\"1\" font-family=\"monospace\" font-size=\"10\">",
    );
    for (id, r) in &sol.rects {
        s.push_str(&format!(
            "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" stroke=\"#e0409a\" stroke-dasharray=\"4 3\"/><text x=\"{:.2}\" y=\"{:.2}\" fill=\"#e0409a\">{}</text>",
            r[0], r[1], r[2], r[3], r[0] + 2.0, r[1] + 10.0, crate::svg::esc(id)
        ));
    }
    for sc in &sol.scrolls {
        let r = sc.viewport;
        s.push_str(&format!(
            "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" stroke=\"#2090e0\"/>",
            r[0], r[1], r[2], r[3]
        ));
    }
    s.push_str("</g>");
    s
}

/// Insert an overlay group before the closing `</svg>`.
pub fn with_overlay(svg: &str, sol: &Solution) -> String {
    match svg.rfind("</svg>") {
        Some(i) => format!("{}{}{}", &svg[..i], overlay_svg(sol), &svg[i..]),
        None => svg.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::{Ease, Track};
    use crate::edit::Editor;
    use crate::engine::Engine;
    use crate::path::VPath;
    use crate::project::{Channel, Prop};

    fn rect_layer(n: &str, w: f32, h: f32) -> Layer {
        Layer::vector(
            n,
            vec![VPath::rect("r", 0.0, 0.0, w, h).with_fill("#ff0000")],
        )
    }

    fn proj(sizes: &[(f32, f32)]) -> Project {
        let mut p = Project::new("c", 400.0, 200.0, 24.0, 5);
        p.layers = sizes
            .iter()
            .enumerate()
            .map(|(i, (w, h))| rect_layer(&format!("l{i}"), *w, *h))
            .collect();
        p
    }

    fn row(p: &Project, n: usize) -> Container {
        Container::new(
            "c",
            Dir::Row,
            (0..n).map(|i| Member::layer(&format!("l{i}"))).collect(),
        )
        .tap(|c| {
            let _ = p;
            c.rect = RectRule::default();
        })
    }

    trait Tap: Sized {
        fn tap(mut self, f: impl FnOnce(&mut Self)) -> Self {
            f(&mut self);
            self
        }
    }
    impl Tap for Container {}

    fn xs(s: &Solution) -> Vec<f32> {
        s.layers.iter().map(|p| p.dx).collect()
    }
    fn ys(s: &Solution) -> Vec<f32> {
        s.layers.iter().map(|p| p.dy).collect()
    }
    fn near(a: &[f32], b: &[f32]) {
        assert_eq!(a.len(), b.len(), "{a:?} vs {b:?}");
        for (x, y) in a.iter().zip(b) {
            assert!((x - y).abs() < 1e-3, "{a:?} vs {b:?}");
        }
    }

    #[test]
    fn row_with_padding_and_gap_is_exact() {
        let mut p = proj(&[(50.0, 20.0), (60.0, 20.0), (70.0, 20.0)]);
        let mut c = row(&p, 3);
        c.padding = [10.0; 4];
        c.gap = 10.0;
        p.containers.push(c);
        let s = solve_project(&p).unwrap();
        near(&xs(&s), &[10.0, 70.0, 140.0]);
        near(&ys(&s), &[10.0, 10.0, 10.0]);
        assert_eq!(s.rects[0].1, [0.0, 0.0, 400.0, 200.0]);
    }

    #[test]
    fn column_distribution() {
        let mut p = proj(&[(50.0, 20.0), (60.0, 30.0)]);
        let mut c = row(&p, 2);
        c.axis = Dir::Column;
        c.gap = 5.0;
        p.containers.push(c);
        let s = solve_project(&p).unwrap();
        near(&xs(&s), &[0.0, 0.0]);
        near(&ys(&s), &[0.0, 25.0]);
    }

    #[test]
    fn justify_modes() {
        let mk = |j: Justify| {
            let mut p = proj(&[(50.0, 20.0), (60.0, 20.0), (70.0, 20.0)]);
            let mut c = row(&p, 3);
            c.justify = j;
            p.containers.push(c);
            xs(&solve_project(&p).unwrap())
        };
        near(&mk(Justify::Start), &[0.0, 50.0, 110.0]);
        near(&mk(Justify::Center), &[110.0, 160.0, 220.0]);
        near(&mk(Justify::End), &[220.0, 270.0, 330.0]);
        near(&mk(Justify::SpaceBetween), &[0.0, 160.0, 330.0]);
        let third = 220.0 / 3.0;
        near(
            &mk(Justify::SpaceAround),
            &[
                third / 2.0,
                third / 2.0 + 50.0 + third,
                third / 2.0 + 110.0 + 2.0 * third,
            ],
        );
        // one item: space_between falls back to start
        let mut p = proj(&[(50.0, 20.0)]);
        let mut c = row(&p, 1);
        c.justify = Justify::SpaceBetween;
        p.containers.push(c);
        near(&xs(&solve_project(&p).unwrap()), &[0.0]);
    }

    #[test]
    fn cross_axis_alignment_and_stretch() {
        let mk = |a: Align, resize: bool| {
            let mut p = proj(&[(50.0, 20.0)]);
            let mut c = row(&p, 1);
            c.align = a;
            c.members[0].resize = resize;
            p.containers.push(c);
            solve_project(&p).unwrap().layers[0].clone()
        };
        assert!((mk(Align::Start, false).dy - 0.0).abs() < 1e-4);
        assert!((mk(Align::Center, false).dy - 90.0).abs() < 1e-4);
        assert!((mk(Align::End, false).dy - 180.0).abs() < 1e-4);
        let st = mk(Align::Stretch, true);
        assert!((st.ky - 10.0).abs() < 1e-4 && (st.kx - 1.0).abs() < 1e-6 && st.dy.abs() < 1e-4);
        // stretch without resize leaves the layer alone
        let ns = mk(Align::Stretch, false);
        assert!((ns.ky - 1.0).abs() < 1e-6);
    }

    #[test]
    fn member_align_overrides_container() {
        let mut p = proj(&[(50.0, 20.0), (50.0, 20.0)]);
        let mut c = row(&p, 2);
        c.members[1].align = Some(Align::End);
        p.containers.push(c);
        near(&ys(&solve_project(&p).unwrap()), &[0.0, 180.0]);
    }

    #[test]
    fn grow_shares_free_space_by_weight() {
        let mut p = proj(&[(50.0, 20.0), (60.0, 20.0), (70.0, 20.0)]);
        let mut c = row(&p, 3);
        c.members[0].grow = 1.0;
        c.members[2].grow = 2.0;
        for m in &mut c.members {
            m.resize = true;
        }
        p.containers.push(c);
        let s = solve_project(&p).unwrap();
        let w0 = 50.0 + 220.0 / 3.0;
        let w2 = 70.0 + 440.0 / 3.0;
        near(&xs(&s), &[0.0, w0, w0 + 60.0]);
        near(
            &s.layers.iter().map(|l| l.kx).collect::<Vec<_>>(),
            &[w0 / 50.0, 1.0, w2 / 70.0],
        );
    }

    #[test]
    fn grow_respects_max_and_redistributes() {
        let mut p = proj(&[(50.0, 20.0), (50.0, 20.0)]);
        let mut c = row(&p, 2);
        p.width = 300.0;
        for m in &mut c.members {
            m.grow = 1.0;
            m.resize = true;
        }
        c.members[0].max_w = Some(100.0);
        p.containers.push(c);
        let s = solve_project(&p).unwrap();
        near(&xs(&s), &[0.0, 100.0]);
        near(
            &s.layers.iter().map(|l| l.kx).collect::<Vec<_>>(),
            &[2.0, 4.0],
        );
    }

    #[test]
    fn shrink_is_weighted_by_size_and_min_freezes() {
        let mut p = proj(&[(50.0, 20.0), (60.0, 20.0), (70.0, 20.0)]);
        p.width = 100.0;
        let mut c = row(&p, 3);
        for m in &mut c.members {
            m.resize = true;
        }
        p.containers.push(c.clone());
        let s = solve_project(&p).unwrap();
        let k = 100.0 / 180.0;
        near(
            &s.layers.iter().map(|l| l.kx).collect::<Vec<_>>(),
            &[k, k, k],
        );
        near(&xs(&s), &[0.0, 50.0 * k, 110.0 * k]);
        // middle member refuses to shrink below 55
        p.containers[0].members[1].min_w = Some(55.0);
        let s = solve_project(&p).unwrap();
        near(&xs(&s), &[0.0, 18.75, 18.75 + 55.0]);
        // a member with shrink 0 keeps its size
        p.containers[0].members[1].min_w = None;
        p.containers[0].members[0].shrink = 0.0;
        let s = solve_project(&p).unwrap();
        assert!((s.layers[0].kx - 1.0).abs() < 1e-6);
        assert!((xs(&s)[1] - 50.0).abs() < 1e-3);
    }

    #[test]
    fn wrap_breaks_lines() {
        let mut p = proj(&[(40.0, 20.0); 4]);
        p.width = 100.0;
        let mut c = row(&p, 4);
        c.wrap = Wrap::Wrap;
        c.gap = 10.0;
        p.containers.push(c);
        let s = solve_project(&p).unwrap();
        near(&xs(&s), &[0.0, 50.0, 0.0, 50.0]);
        near(&ys(&s), &[0.0, 0.0, 30.0, 30.0]);
        // separate line gap
        p.containers[0].cross_gap = Some(2.0);
        near(&ys(&solve_project(&p).unwrap()), &[0.0, 0.0, 22.0, 22.0]);
    }

    #[test]
    fn nested_containers() {
        let mut p = proj(&[(50.0, 20.0), (50.0, 20.0), (30.0, 40.0)]);
        let mut inner = Container::new(
            "in",
            Dir::Row,
            vec![Member::layer("l0"), Member::layer("l1")],
        );
        inner.gap = 5.0;
        let mut outer = Container::new(
            "out",
            Dir::Column,
            vec![Member::nested("in"), Member::layer("l2")],
        );
        outer.gap = 10.0;
        p.containers = vec![inner, outer.clone()];
        let s = solve_project(&p).unwrap();
        near(&xs(&s), &[0.0, 55.0, 0.0]);
        near(&ys(&s), &[0.0, 0.0, 30.0]);
        assert_eq!(
            s.rects.iter().find(|r| r.0 == "in").unwrap().1,
            [0.0, 0.0, 105.0, 20.0]
        );
        // centred: the intrinsic nested box is centred in the outer width
        p.containers[1].align = Align::Center;
        let s = solve_project(&p).unwrap();
        near(&xs(&s), &[147.5, 202.5, 185.0]);
        // nested stretch gives the inner container the full width, so space_between spreads it
        p.containers[1].align = Align::Stretch;
        p.containers[0].justify = Justify::SpaceBetween;
        let s = solve_project(&p).unwrap();
        near(&xs(&s), &[0.0, 350.0, 0.0]);
    }

    #[test]
    fn spacers_push_members_apart() {
        let mut p = proj(&[(50.0, 20.0), (50.0, 20.0)]);
        let mut c = row(&p, 2);
        c.members.insert(1, Member::spacer(1.0));
        p.containers.push(c);
        near(&xs(&solve_project(&p).unwrap()), &[0.0, 350.0]);
    }

    #[test]
    fn rect_rules_follow_the_canvas() {
        let pin = RectRule::Pin {
            left: Some(10.0),
            top: Some(20.0),
            right: Some(30.0),
            bottom: None,
            width: None,
            height: Some(50.0),
        };
        assert_eq!(
            resolve_rule(&pin, 400.0, 200.0, [0.0; 2]),
            [10.0, 20.0, 360.0, 50.0]
        );
        assert_eq!(
            resolve_rule(&pin, 800.0, 600.0, [0.0; 2]),
            [10.0, 20.0, 760.0, 50.0]
        );
        let br = RectRule::Pin {
            left: None,
            top: None,
            right: Some(10.0),
            bottom: Some(10.0),
            width: Some(100.0),
            height: Some(40.0),
        };
        assert_eq!(
            resolve_rule(&br, 400.0, 200.0, [0.0; 2]),
            [290.0, 150.0, 100.0, 40.0]
        );
        let ctr = RectRule::Pin {
            left: None,
            top: None,
            right: None,
            bottom: None,
            width: Some(100.0),
            height: Some(100.0),
        };
        assert_eq!(
            resolve_rule(&ctr, 400.0, 200.0, [0.0; 2]),
            [150.0, 50.0, 100.0, 100.0]
        );
        let fit = RectRule::Aspect {
            ratio: 16.0 / 9.0,
            mode: AspectMode::Fit,
            margin: 0.0,
            ax: 0.5,
            ay: 0.5,
        };
        let r = resolve_rule(&fit, 400.0, 400.0, [0.0; 2]);
        assert!(
            (r[2] - 400.0).abs() < 1e-3
                && (r[3] - 225.0).abs() < 1e-3
                && (r[1] - 87.5).abs() < 1e-3
        );
        let fill = RectRule::Aspect {
            ratio: 16.0 / 9.0,
            mode: AspectMode::Fill,
            margin: 0.0,
            ax: 0.5,
            ay: 0.5,
        };
        let r = resolve_rule(&fill, 400.0, 400.0, [0.0; 2]);
        assert!(
            (r[3] - 400.0).abs() < 1e-3
                && (r[2] - 711.111).abs() < 1e-2
                && (r[0] + 155.5555).abs() < 1e-2
        );
        let m = RectRule::Aspect {
            ratio: 1.0,
            mode: AspectMode::Fit,
            margin: 10.0,
            ax: 0.0,
            ay: 0.0,
        };
        assert_eq!(
            resolve_rule(&m, 400.0, 200.0, [0.0; 2]),
            [10.0, 10.0, 180.0, 180.0]
        );
        // margins larger than the canvas clamp to zero size
        let big = RectRule::Pin {
            left: Some(300.0),
            top: None,
            right: Some(300.0),
            bottom: None,
            width: None,
            height: None,
        };
        assert_eq!(resolve_rule(&big, 400.0, 200.0, [0.0; 2])[2], 0.0);
    }

    #[test]
    fn container_reflows_when_the_canvas_changes() {
        let mut p = proj(&[(50.0, 20.0), (50.0, 20.0)]);
        let mut c = row(&p, 2);
        c.justify = Justify::SpaceBetween;
        c.align = Align::End;
        c.rect = RectRule::Pin {
            left: Some(10.0),
            right: Some(10.0),
            top: None,
            bottom: Some(10.0),
            width: None,
            height: Some(40.0),
        };
        p.containers.push(c);
        let a = solve_project(&p).unwrap();
        near(&xs(&a), &[10.0, 340.0]);
        near(&ys(&a), &[170.0, 170.0]);
        let q = responsive(&p, 800.0, 600.0).unwrap();
        let b = solve_project(&q).unwrap();
        near(&xs(&b), &[10.0, 740.0]);
        near(&ys(&b), &[570.0, 570.0]);
        // the source project is untouched
        assert_eq!((p.width, p.height), (400.0, 200.0));
        assert!(responsive(&p, 0.0, 10.0).is_err() && responsive(&p, f32::NAN, 10.0).is_err());
        assert_eq!(parse_size("800x600").unwrap(), (800.0, 600.0));
        assert!(
            parse_size("800").is_err() && parse_size("0x5").is_err() && parse_size("axb").is_err()
        );
        // an engine compiled from the responsive copy renders members at the new place
        let e = Engine::new(q).unwrap();
        let f = e.evaluate(0.0);
        let id = e.node_index("l1").unwrap();
        let o = f.nodes[id].world.apply(Vec2::ZERO);
        let e0 = Engine::new(p.clone()).unwrap();
        let o0 = e0.evaluate(0.0).nodes[e0.node_index("l1").unwrap()]
            .world
            .apply(Vec2::ZERO);
        assert!(
            (o.x - o0.x - 400.0).abs() < 1e-2 && (o.y - o0.y - 400.0).abs() < 1e-2,
            "{o:?} {o0:?}"
        );
    }

    #[test]
    fn validation_errors() {
        let base = proj(&[(50.0, 20.0), (50.0, 20.0)]);
        let bad = |f: &dyn Fn(&mut Project)| {
            let mut p = base.clone();
            f(&mut p);
            validate(&p).join("\n")
        };
        assert!(bad(&|p| p.containers.push(Container::new(
            "c",
            Dir::Row,
            vec![Member::layer("ghost")]
        )))
        .contains("unknown layer"));
        assert!(bad(&|p| {
            p.containers
                .push(Container::new("a", Dir::Row, vec![Member::layer("l0")]));
            p.containers
                .push(Container::new("b", Dir::Row, vec![Member::layer("l0")]));
        })
        .contains("both container"));
        assert!(bad(&|p| p.containers.push(Container::new(
            "a",
            Dir::Row,
            vec![Member::layer("l0"), Member::layer("l0")]
        )))
        .contains("both container"));
        assert!(bad(&|p| p.containers.push(Container::new(
            "a",
            Dir::Row,
            vec![Member::nested("a")]
        )))
        .contains("cycle"));
        assert!(bad(&|p| {
            p.containers
                .push(Container::new("a", Dir::Row, vec![Member::nested("b")]));
            p.containers
                .push(Container::new("b", Dir::Row, vec![Member::nested("a")]));
        })
        .contains("cycle"));
        assert!(bad(&|p| {
            p.containers.push(Container::new("a", Dir::Row, vec![]));
            p.containers.push(Container::new("a", Dir::Row, vec![]));
        })
        .contains("duplicate container"));
        let mut m = Member::layer("l0");
        m.width = Some(0.0);
        assert!(bad(&|p| p
            .containers
            .push(Container::new("a", Dir::Row, vec![m.clone()])))
        .contains("width must be"));
        let mut m = Member::layer("l0");
        m.height = Some(-3.0);
        assert!(bad(&|p| p
            .containers
            .push(Container::new("a", Dir::Row, vec![m.clone()])))
        .contains("height must be"));
        let mut m = Member::layer("l0");
        m.grow = f32::NAN;
        assert!(bad(&|p| p
            .containers
            .push(Container::new("a", Dir::Row, vec![m.clone()])))
        .contains("grow"));
        let mut m = Member::layer("l0");
        m.min_w = Some(10.0);
        m.max_w = Some(5.0);
        assert!(bad(&|p| p
            .containers
            .push(Container::new("a", Dir::Row, vec![m.clone()])))
        .contains("larger than max"));
        assert!(bad(&|p| {
            let mut c = Container::new("a", Dir::Row, vec![]);
            c.gap = f32::NAN;
            p.containers.push(c);
        })
        .contains("gap"));
        assert!(bad(&|p| {
            let mut c = Container::new("a", Dir::Row, vec![]);
            c.padding = [0.0, -1.0, 0.0, 0.0];
            p.containers.push(c);
        })
        .contains("padding"));
        assert!(bad(&|p| {
            let mut c = Container::new("a", Dir::Row, vec![]);
            c.rect = RectRule::Fixed {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 10.0,
            };
            p.containers.push(c);
        })
        .contains("fixed rect"));
        assert!(bad(&|p| {
            let mut c = Container::new("a", Dir::Row, vec![]);
            c.rect = RectRule::Pin {
                left: Some(1.0),
                right: Some(1.0),
                width: Some(5.0),
                top: None,
                bottom: None,
                height: None,
            };
            p.containers.push(c);
        })
        .contains("over-constrained"));
        assert!(bad(&|p| {
            let mut c = Container::new("a", Dir::Row, vec![]);
            c.rect = RectRule::Aspect {
                ratio: 0.0,
                mode: AspectMode::Fit,
                margin: 0.0,
                ax: 0.5,
                ay: 0.5,
            };
            p.containers.push(c);
        })
        .contains("aspect ratio"));
        assert!(bad(&|p| p.containers.push(Container::new(
            "a",
            Dir::Row,
            vec![Member {
                layer: None,
                ..Member::spacer(0.0)
            }
            .tap_none()]
        )))
        .contains("exactly one"));
        // unmeasurable layer needs an explicit size
        assert!(bad(&|p| {
            p.layers.push(Layer::group("g", vec![]));
            p.containers
                .push(Container::new("a", Dir::Row, vec![Member::layer("g")]));
        })
        .contains("no measurable geometry"));
        // resize on a rotated layer
        assert!(bad(&|p| {
            p.layers[0].transform.rotation = 10.0;
            let mut m = Member::layer("l0");
            m.resize = true;
            p.containers.push(Container::new("a", Dir::Row, vec![m]));
        })
        .contains("resize needs"));
        // solve refuses invalid input rather than panicking
        let mut p = base.clone();
        p.containers
            .push(Container::new("a", Dir::Row, vec![Member::nested("a")]));
        assert!(solve_project(&p).is_err());
        assert!(solve(&base, f32::NAN, 5.0).is_err());
        // the engine refuses it too
        assert!(Engine::new(p).is_err());
    }

    trait TapNone {
        fn tap_none(self) -> Self;
    }
    impl TapNone for Member {
        fn tap_none(mut self) -> Self {
            self.spacer = false;
            self
        }
    }

    fn scroll_project() -> Project {
        let mut p = proj(&[(100.0, 20.0); 10]);
        let mut c = row(&p, 10);
        c.axis = Dir::Column;
        p.containers.push(c);
        let mut s = Scroll::new("s", "c");
        s.viewport = RectRule::Fixed {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 50.0,
        };
        p.scrolls.push(s);
        p
    }

    #[test]
    fn scroll_clamps_offset() {
        let mut p = scroll_project();
        let s = solve_project(&p).unwrap();
        assert!((s.scrolls[0].max[1] - 150.0).abs() < 1e-3);
        near(&ys(&s)[..3], &[0.0, 20.0, 40.0]);
        set_offset(&mut p, "s", [30.0, 80.0]).unwrap();
        let s = solve_project(&p).unwrap();
        assert_eq!(
            s.scrolls[0].offset,
            [0.0, 80.0],
            "x does not scroll on a vertical region"
        );
        near(&ys(&s)[..3], &[-80.0, -60.0, -40.0]);
        set_offset(&mut p, "s", [0.0, 500.0]).unwrap();
        assert!((solve_project(&p).unwrap().scrolls[0].offset[1] - 150.0).abs() < 1e-3);
        set_offset(&mut p, "s", [0.0, -20.0]).unwrap();
        assert_eq!(solve_project(&p).unwrap().scrolls[0].offset, [0.0, 0.0]);
        assert!(
            set_offset(&mut p, "s", [f32::NAN, 0.0]).is_err()
                && set_offset(&mut p, "zz", [0.0; 2]).is_err()
        );
        // content shorter than the viewport cannot scroll
        let mut q = proj(&[(100.0, 20.0)]);
        q.containers.push(row(&q, 1));
        let mut sc = Scroll::new("s", "c");
        sc.viewport = RectRule::Fixed {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 50.0,
        };
        sc.offset = [0.0, 40.0];
        q.scrolls.push(sc);
        assert_eq!(solve_project(&q).unwrap().scrolls[0].max, [0.0, 0.0]);
    }

    #[test]
    fn scroll_horizontal_and_both_axes() {
        let mut p = proj(&[(100.0, 20.0); 4]);
        p.containers.push(row(&p, 4));
        let mut s = Scroll::new("s", "c");
        s.axis = ScrollAxis::X;
        s.viewport = RectRule::Fixed {
            x: 10.0,
            y: 5.0,
            w: 150.0,
            h: 50.0,
        };
        s.offset = [60.0, 99.0];
        p.scrolls.push(s);
        let r = solve_project(&p).unwrap();
        assert_eq!(r.scrolls[0].max, [250.0, 0.0]);
        near(
            &xs(&r),
            &[10.0 - 60.0, 50.0, 150.0, 250.0]
                .map(|v: f32| v - 0.0)
                .iter()
                .enumerate()
                .map(|(i, _)| 10.0 - 60.0 + 100.0 * i as f32)
                .collect::<Vec<_>>(),
        );
        near(&ys(&r), &[5.0; 4]);
        p.scrolls[0].axis = ScrollAxis::Both;
        p.scrolls[0].offset = [60.0, 99.0];
        let r = solve_project(&p).unwrap();
        assert_eq!(
            r.scrolls[0].offset,
            [60.0, 0.0],
            "content is not taller than the viewport"
        );
    }

    #[test]
    fn scroll_wraps_to_the_viewport_width() {
        let mut p = proj(&[(40.0, 20.0); 6]);
        let mut c = row(&p, 6);
        c.wrap = Wrap::Wrap;
        c.gap = 10.0;
        p.containers.push(c);
        let mut s = Scroll::new("s", "c");
        s.viewport = RectRule::Fixed {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 40.0,
        };
        p.scrolls.push(s);
        let r = solve_project(&p).unwrap();
        // two per line, three lines of 20 with gaps of 10: height 80, so max offset 40
        assert_eq!(r.scrolls[0].max, [0.0, 40.0]);
        near(&ys(&r), &[0.0, 0.0, 30.0, 30.0, 60.0, 60.0]);
    }

    #[test]
    fn scroll_elastic_overscroll_and_runtime() {
        let mut p = scroll_project();
        p.scrolls[0].elastic = Some(Elastic {
            limit: 40.0,
            stiffness: 180.0,
            damping_ratio: 0.9,
        });
        set_offset(&mut p, "s", [0.0, 250.0]).unwrap();
        let o = solve_project(&p).unwrap().scrolls[0].offset[1];
        let want = 150.0 + 40.0 * (1.0 - (-100.0f32 / 40.0).exp());
        assert!((o - want).abs() < 1e-3 && o < 190.0 && o > 150.0, "{o}");
        set_offset(&mut p, "s", [0.0, -30.0]).unwrap();
        let o = solve_project(&p).unwrap().scrolls[0].offset[1];
        assert!(o < 0.0 && o > -30.0, "{o}");
        // monotone saturation
        let a = effective_offset(200.0, 150.0, p.scrolls[0].elastic.as_ref());
        let b = effective_offset(400.0, 150.0, p.scrolls[0].elastic.as_ref());
        assert!(a < b && b < 190.0);
        // runtime: drag past the end, let go, spring back to the clamp and stop exactly
        let mut rt = ScrollRuntime::new(&p.scrolls[0], [0.0, 150.0]);
        rt.drag_to([0.0, 300.0]);
        assert!(rt.pos[1] > 150.0);
        rt.release([0.0, 0.0]);
        for _ in 0..600 {
            rt.step(1.0 / 60.0);
        }
        assert!(rt.is_settled());
        assert_eq!(rt.pos, [0.0, 150.0]);
        // determinism
        let run = || {
            let mut r = ScrollRuntime::new(&p.scrolls[0], [0.0, 150.0]);
            r.drag_to([0.0, -80.0]);
            r.release([0.0, 0.0]);
            let mut v = Vec::new();
            for _ in 0..30 {
                r.step(1.0 / 60.0);
                v.push(r.pos[1].to_bits());
            }
            v
        };
        assert_eq!(run(), run());
        // without elastic the drag is clamped
        p.scrolls[0].elastic = None;
        let mut rt = ScrollRuntime::new(&p.scrolls[0], [0.0, 150.0]);
        rt.drag_to([0.0, 999.0]);
        assert_eq!(rt.pos[1], 150.0);
    }

    #[test]
    fn raw_offset_inverts_effective_offset_and_set_max_reclamps() {
        let el = Elastic {
            limit: 40.0,
            stiffness: 180.0,
            damping_ratio: 0.9,
        };
        for raw in [-90.0f32, -5.0, 0.0, 70.0, 150.0, 151.0, 260.0, 400.0] {
            let shown = effective_offset(raw, 150.0, Some(&el));
            let back = raw_offset_for(shown, 150.0, Some(&el));
            assert!(
                (back - raw).abs() < 0.05 * (1.0 + raw.abs()),
                "{raw} -> {shown} -> {back}"
            );
        }
        assert_eq!(raw_offset_for(-3.0, 150.0, None), 0.0);
        assert_eq!(raw_offset_for(500.0, 150.0, None), 150.0);
        // a saturated value maps to a finite raw offset
        assert!(raw_offset_for(1e9, 150.0, Some(&el)).is_finite());
        let p = scroll_project();
        let mut rt = ScrollRuntime::new(&p.scrolls[0], [0.0, 150.0]);
        rt.drag_to([0.0, 140.0]);
        rt.set_max([0.0, 100.0]);
        assert_eq!(rt.pos[1], 100.0);
        assert_eq!(rt.max(), [0.0, 100.0]);
        rt.set_max([0.0, 300.0]);
        assert_eq!(rt.pos[1], 100.0);
    }

    #[test]
    fn scroll_snap_points_and_momentum() {
        let mut p = scroll_project();
        p.scrolls[0].snap_y = vec![0.0, 50.0, 100.0, 400.0];
        assert_eq!(snap_to(70.0, &p.scrolls[0].snap_y, 150.0), 50.0);
        assert_eq!(snap_to(80.0, &p.scrolls[0].snap_y, 150.0), 100.0);
        assert_eq!(
            snap_to(140.0, &p.scrolls[0].snap_y, 150.0),
            150.0,
            "out of range snap points clamp to the end"
        );
        assert_eq!(snap_to(33.0, &[], 150.0), 33.0);
        let mut rt = ScrollRuntime::new(&p.scrolls[0], [0.0, 150.0]);
        rt.drag_to([0.0, 70.0]);
        rt.release([0.0, 0.0]);
        for _ in 0..600 {
            rt.step(1.0 / 60.0);
        }
        assert_eq!(rt.pos[1], 50.0);
        // a fast flick carries over to the next snap point: 70 + 300 * 0.15 = 115 -> 100
        rt.drag_to([0.0, 70.0]);
        rt.release([0.0, 300.0]);
        for _ in 0..600 {
            rt.step(1.0 / 60.0);
        }
        assert_eq!(rt.pos[1], 100.0);
        // the x axis is inert on a vertical region
        rt.drag_to([40.0, 10.0]);
        assert_eq!(rt.pos[0], 0.0);
    }

    #[test]
    fn scroll_clip_layer_masks_content_and_follows_the_viewport() {
        let mut p = scroll_project();
        let mut clip = rect_layer("clip", 10.0, 10.0);
        clip.visible = false;
        p.layers.push(clip);
        p.scrolls[0].clip_layer = Some("clip".into());
        p.scrolls[0].viewport = RectRule::Pin {
            left: Some(20.0),
            right: Some(20.0),
            top: Some(10.0),
            bottom: None,
            width: None,
            height: Some(50.0),
        };
        let e = Engine::new(p.clone()).unwrap();
        let f = e.evaluate(0.0);
        let cid = e.node_index("clip").unwrap();
        for i in 0..10 {
            let n = &f.nodes[e.node_index(&format!("l{i}")).unwrap()];
            assert_eq!(n.mask, Some((cid, false)));
        }
        // clip layer now covers the viewport: origin (20, 10), scaled 360 x 50 from 10 x 10
        let s = solve_project(&p).unwrap();
        let c = s.layers.iter().find(|l| l.layer == "clip").unwrap();
        near(&[c.dx, c.dy, c.kx, c.ky], &[20.0, 10.0, 36.0, 5.0]);
        // a content layer that is already masked is rejected
        let mut q = p.clone();
        q.layers.push(rect_layer("other", 5.0, 5.0));
        q.layers[0].mask = Some(MaskRef {
            layer: "other".into(),
            invert: false,
        });
        assert!(validate(&q).join("\n").contains("already has a mask"));
        // the clip layer must not be a container member or part of the content
        let mut q = p.clone();
        q.containers[0].members.push(Member::layer("clip"));
        assert!(!validate(&q).is_empty());
    }

    #[test]
    fn scroll_validation() {
        let mut p = scroll_project();
        p.scrolls.push(Scroll::new("s", "c"));
        assert!(validate(&p).join("\n").contains("duplicate scroll"));
        let mut p = scroll_project();
        p.scrolls[0].content = "nope".into();
        assert!(validate(&p).join("\n").contains("unknown content"));
        let mut p = scroll_project();
        p.scrolls.push(Scroll::new("t", "c"));
        assert!(validate(&p).join("\n").contains("content of both"));
        let mut p = scroll_project();
        p.containers
            .push(Container::new("o", Dir::Row, vec![Member::nested("c")]));
        assert!(validate(&p).join("\n").contains("nested in another"));
        let mut p = scroll_project();
        p.scrolls[0].elastic = Some(Elastic {
            limit: 0.0,
            stiffness: 1.0,
            damping_ratio: 1.0,
        });
        assert!(validate(&p).join("\n").contains("elastic"));
        let mut p = scroll_project();
        p.scrolls[0].offset = [f32::INFINITY, 0.0];
        assert!(validate(&p).join("\n").contains("offset"));
        let mut p = scroll_project();
        p.scrolls[0].snap_y = vec![f32::NAN];
        assert!(validate(&p).join("\n").contains("snap"));
        let mut p = scroll_project();
        p.scrolls[0].clip_layer = Some("ghost".into());
        assert!(validate(&p).join("\n").contains("unknown clip"));
        let mut p = scroll_project();
        p.scrolls[0].viewport = RectRule::Hug { x: 0.0, y: 0.0 };
        assert!(validate(&p).join("\n").contains("hug"));
        assert!(Engine::new(p).is_err());
    }

    #[test]
    fn scroll_input_values_drive_offsets() {
        let mut p = scroll_project();
        p.scrolls[0].input = Some("page.scroll".into());
        let mut v = HashMap::new();
        v.insert("page.scroll".to_string(), 60.0);
        let q = apply_inputs(&p, &v);
        assert_eq!(q.scrolls[0].offset, [0.0, 60.0]);
        near(&ys(&solve_project(&q).unwrap())[..2], &[-60.0, -40.0]);
        v.insert("page.scroll".to_string(), f64::NAN);
        assert_eq!(apply_inputs(&p, &v).scrolls[0].offset, [0.0, 0.0]);
    }

    #[test]
    fn layout_is_additive_to_animation_in_the_engine() {
        let mut p = proj(&[(50.0, 20.0), (50.0, 20.0)]);
        let mut c = row(&p, 2);
        c.justify = Justify::End;
        p.containers.push(c);
        p.channels.push(Channel::keys(
            "l0",
            Prop::LayerX,
            Track::with_keys(&[(0.0, 0.0, Ease::Linear), (4.0, 40.0, Ease::Linear)]),
        ));
        let e = Engine::new(p.clone()).unwrap();
        let at = |f: f32, name: &str| {
            let fr = e.evaluate(f);
            fr.nodes[e.node_index(name).unwrap()]
                .world
                .apply(Vec2::ZERO)
        };
        // reference: same animation without containers
        let mut plain = p.clone();
        plain.containers.clear();
        let ep = Engine::new(plain).unwrap();
        let atp = |f: f32, name: &str| {
            ep.evaluate(f).nodes[ep.node_index(name).unwrap()]
                .world
                .apply(Vec2::ZERO)
        };
        for f in [0.0, 2.0, 4.0] {
            let (a, b) = (at(f, "l0"), atp(f, "l0"));
            assert!(
                (a.x - b.x - 300.0).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3,
                "frame {f}: {a:?} {b:?}"
            );
            let (a, b) = (at(f, "l1"), atp(f, "l1"));
            assert!((a.x - b.x - 350.0).abs() < 1e-3);
        }
        // determinism of the compiled result
        let e2 = Engine::new(p).unwrap();
        assert_eq!(
            e2.evaluate(3.0).nodes[0].world,
            e.evaluate(3.0).nodes[0].world
        );
    }

    #[test]
    fn resize_changes_the_rendered_size() {
        let mut p = proj(&[(50.0, 20.0)]);
        let mut c = row(&p, 1);
        c.members[0].resize = true;
        c.members[0].grow = 1.0;
        c.padding = [10.0, 10.0, 10.0, 10.0];
        c.align = Align::Stretch;
        p.containers.push(c);
        // anchor away from the origin and a non-unit static scale must not matter
        p.layers[0].transform.anchor = Vec2::new(25.0, 10.0);
        p.layers[0].transform.position = Vec2::new(100.0, 100.0);
        p.layers[0].transform.scale = Vec2::new(2.0, 1.0);
        let e = Engine::new(p).unwrap();
        let f = e.evaluate(0.0);
        let n = &f.nodes[e.node_index("l0").unwrap()];
        let a = n.world.apply(Vec2::new(0.0, 0.0));
        let b = n.world.apply(Vec2::new(50.0, 20.0));
        assert!(
            (a.x - 10.0).abs() < 1e-2 && (a.y - 10.0).abs() < 1e-2,
            "{a:?}"
        );
        assert!(
            (b.x - 390.0).abs() < 1e-2 && (b.y - 190.0).abs() < 1e-2,
            "{b:?}"
        );
    }

    #[test]
    fn unmeasurable_layers_use_explicit_sizes() {
        let mut p = proj(&[(50.0, 20.0)]);
        p.layers.push(Layer::group("g", vec![]));
        p.layers[1].transform.position = Vec2::new(7.0, 3.0);
        let mut m = Member::layer("g");
        m.width = Some(30.0);
        m.height = Some(10.0);
        p.containers
            .push(Container::new("c", Dir::Row, vec![Member::layer("l0"), m]));
        let s = solve_project(&p).unwrap();
        near(&xs(&s), &[0.0, 50.0 - 7.0]);
        near(&ys(&s), &[0.0, -3.0]);
    }

    #[test]
    fn editor_ops_are_undoable_and_atomic() {
        let p = proj(&[(50.0, 20.0), (50.0, 20.0)]);
        let mut ed = Editor::new(p).unwrap();
        let c = Container::new(
            "c",
            Dir::Row,
            vec![Member::layer("l0"), Member::layer("l1")],
        );
        let op = add_container_op(ed.project(), c.clone()).unwrap();
        ed.apply(op).unwrap();
        assert_eq!(ed.project().containers.len(), 1);
        // second container goes through the append path
        let d = Container::new("d", Dir::Row, vec![]);
        ed.apply(add_container_op(ed.project(), d).unwrap())
            .unwrap();
        assert_eq!(ed.project().containers.len(), 2);
        assert!(ed.undo());
        assert_eq!(ed.project().containers.len(), 1);
        assert!(ed.undo());
        assert!(ed.project().containers.is_empty());
        assert!(ed.redo());
        assert_eq!(ed.project().containers[0], c);
        // duplicates and invalid containers never reach the editor
        assert!(add_container_op(ed.project(), c.clone()).is_err());
        assert!(add_container_op(
            ed.project(),
            Container::new("z", Dir::Row, vec![Member::layer("ghost")])
        )
        .is_err());
        // an op that would leave a scroll dangling is rolled back by the engine check
        ed.apply(add_scroll_op(ed.project(), Scroll::new("s", "c")).unwrap())
            .unwrap();
        assert!(remove_container_op(ed.project(), "c").is_err());
        let dangling = crate::edit::Op::Remove {
            path: "/containers/0".into(),
        };
        assert!(ed.apply(dangling).is_err());
        assert_eq!(ed.project().containers.len(), 1);
        ed.apply(remove_scroll_op(ed.project(), "s").unwrap())
            .unwrap();
        ed.apply(remove_container_op(ed.project(), "c").unwrap())
            .unwrap();
        assert!(ed.project().containers.is_empty());
        assert!(remove_container_op(ed.project(), "c").is_err());
        assert!(remove_scroll_op(ed.project(), "s").is_err());
    }

    #[test]
    fn project_fields_are_omitted_when_empty_and_round_trip() {
        let mut p = proj(&[(50.0, 20.0)]);
        let json = p.to_json_pretty();
        assert!(!json.contains("containers") && !json.contains("scrolls"));
        p.containers.push(row(&p, 1));
        p.scrolls.push(Scroll::new("s", "c"));
        let q = Project::from_json(&p.to_json_pretty()).unwrap();
        assert_eq!(q.containers, p.containers);
        assert_eq!(q.scrolls, p.scrolls);
        // minimal hand-written JSON uses the defaults
        let c: Container = serde_json::from_str(r#"{"id":"x","members":[{"layer":"a","grow":1}],"justify":"space_between","rect":{"kind":"aspect","ratio":1.5}}"#).unwrap();
        assert_eq!(c.members[0].shrink, 1.0);
        assert_eq!(c.justify, Justify::SpaceBetween);
        assert!(serde_json::from_str::<Container>(
            r#"{"id":"x","members":[],"justify":"sideways"}"#
        )
        .is_err());
    }

    #[test]
    fn overlay_marks_every_rectangle() {
        let mut p = scroll_project();
        p.layers.push(rect_layer("extra", 5.0, 5.0));
        let s = solve_project(&p).unwrap();
        let svg = with_overlay("<svg></svg>", &s);
        assert!(svg.starts_with("<svg><g") && svg.ends_with("</g></svg>"));
        assert_eq!(svg.matches("<rect").count(), 2);
    }
}
