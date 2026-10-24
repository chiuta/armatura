// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
//! Timeline / dope-sheet model and key-editing operations.
//!
//! * [`build`] turns a project into a [`TimelineModel`]: one track per animation channel
//!   (`tracks[i].ch == i`), grouped hierarchically (kind -> bone parent chain / layer path /
//!   deformer path), with the channel metadata (lock, shy, solo, colour) from `chanmeta`.
//! * Keys are addressed by [`KeyRef`] = (channel index, exact frame). Selection helpers are pure
//!   functions on `Vec<KeyRef>` (box over rows and time, toggle, by channel / group).
//! * Every edit is planned by [`plan`] from a [`TlOp`] and returns an [`EditPlan`] holding one
//!   `Op::Batch` for [`crate::edit::Editor::apply`], so it is one undo step and is validated by the
//!   editor like any other edit. Planning never mutates the project; any error (locked channel,
//!   frame outside the project, collision under `Refuse`, stale key) refuses the whole op.
//!
//! Clean-room note: generic dope-sheet maths (translate / scale about a pivot / clip copy and
//! paste with channel remap / ripple insert and delete / loop closing). The data model and the
//! operation set are this project's own; nothing follows any product's behaviour, naming or
//! shortcuts.

use crate::anim::{Ease, Extrapolate, Key, Track};
use crate::chanmeta;
use crate::edit::Op;
use crate::project::{Channel, Mode, Project, Prop, Source, TargetKind};
use crate::timeline_graph::{self as g, HandleMode, HandlePos, Side, TangentKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------------------------------------------------------- model

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct KeyInfo {
    pub frame: f32,
    pub value: f32,
    pub ease: Ease,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TrackInfo {
    /// Channel index in `Project::channels` (equals the position in `TimelineModel::tracks`).
    pub ch: usize,
    pub target: String,
    pub prop: Prop,
    pub label: String,
    /// `keys`, `expr`, `noise` or `driven`.
    pub source: &'static str,
    /// Key channels that are not skin-collapse helpers; only these accept key edits.
    pub editable: bool,
    pub locked: bool,
    pub shy: bool,
    pub solo: bool,
    pub color: Option<String>,
    pub mode: Mode,
    pub weight: f32,
    pub extrapolate: Option<Extrapolate>,
    /// Index into `TimelineModel::groups`.
    pub group: usize,
    pub keys: Vec<KeyInfo>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GroupInfo {
    pub id: usize,
    pub label: String,
    /// `bone`, `layer`, `camera`, `ik`, `constraint` or `sub` (the kind headers are depth 0).
    pub kind: &'static str,
    pub depth: usize,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// Channels that hang directly off this group.
    pub tracks: Vec<usize>,
    /// Sorted union of the key frames of this group and everything below it (summary diamonds).
    pub frames: Vec<f32>,
    /// Every editable track below this group is locked (and there is at least one).
    pub locked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "row", content = "index", rename_all = "snake_case")]
pub enum Row {
    Group(usize),
    Track(usize),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TimelineModel {
    pub frames: u32,
    pub fps: f32,
    pub groups: Vec<GroupInfo>,
    pub tracks: Vec<TrackInfo>,
    /// Display order, depth first: a group row, its own tracks, then its child groups.
    pub rows: Vec<Row>,
}

fn kind_name(k: TargetKind) -> &'static str {
    match k {
        TargetKind::Bone => "bone",
        TargetKind::Layer => "layer",
        TargetKind::Camera => "camera",
        TargetKind::Ik => "ik",
        TargetKind::Constraint => "constraint",
        TargetKind::Sub => "sub",
    }
}

fn kind_label(k: TargetKind) -> &'static str {
    match k {
        TargetKind::Bone => "Bones",
        TargetKind::Layer => "Layers",
        TargetKind::Camera => "Camera",
        TargetKind::Ik => "IK chains",
        TargetKind::Constraint => "Constraints",
        TargetKind::Sub => "Deformers and paths",
    }
}

const KIND_ORDER: [TargetKind; 6] = [
    TargetKind::Bone,
    TargetKind::Layer,
    TargetKind::Camera,
    TargetKind::Ik,
    TargetKind::Constraint,
    TargetKind::Sub,
];

fn prop_name(p: Prop) -> String {
    serde_json::to_value(p)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| format!("{p:?}"))
}

fn is_keyed(c: &Channel) -> bool {
    matches!(c.source, Source::Keys { .. }) && !crate::skins::is_collapse_channel(c)
}

/// (group label, group path) chain from the kind header down to the group that owns the channel.
fn chain_of(p: &Project, c: &Channel, kind: TargetKind) -> Vec<(String, String)> {
    match kind {
        TargetKind::Bone => {
            let mut chain = vec![c.target.clone()];
            let mut cur = p.bones.iter().find(|b| b.name == c.target);
            let mut guard = 0;
            while let Some(parent) = cur.and_then(|b| b.parent.as_ref()) {
                chain.push(parent.clone());
                cur = p.bones.iter().find(|b| &b.name == parent);
                guard += 1;
                if guard > p.bones.len() {
                    break;
                }
            }
            chain.reverse();
            chain.into_iter().map(|n| (n.clone(), n)).collect()
        }
        TargetKind::Layer => {
            let mut out = Vec::new();
            let mut acc = String::new();
            for seg in c.target.split('/') {
                if !acc.is_empty() {
                    acc.push('/');
                }
                acc.push_str(seg);
                out.push((seg.to_string(), acc.clone()));
            }
            out
        }
        TargetKind::Camera => Vec::new(),
        TargetKind::Ik | TargetKind::Constraint => vec![(c.target.clone(), c.target.clone())],
        TargetKind::Sub => {
            let parts: Vec<&str> = c.target.split("::").collect();
            let upto = if parts.len() >= 2 {
                parts.len() - 1
            } else {
                parts.len()
            };
            let mut out = Vec::new();
            for i in 0..upto {
                out.push((parts[i].to_string(), parts[..=i].join("::")));
            }
            out
        }
    }
}

fn meta_for(p: &Project, target: &str, prop: Prop) -> (bool, bool, bool, Option<String>) {
    let (mut lock, mut shy, mut solo) = (false, false, false);
    let mut color: Option<(bool, String)> = None; // (specific to the prop, colour)
    for m in &p.channel_meta {
        if m.target != target || m.prop.is_some_and(|q| q != prop) {
            continue;
        }
        lock |= m.lock;
        shy |= m.shy;
        solo |= m.solo;
        if let Some(c) = &m.color {
            let specific = m.prop.is_some();
            if color.as_ref().is_none_or(|(s, _)| specific || !*s) {
                color = Some((specific, c.clone()));
            }
        }
    }
    (
        lock || chanmeta::layer_locked(p, target, prop),
        shy,
        solo,
        color.map(|c| c.1),
    )
}

/// Build the timeline model. Deterministic: order follows the kind order, then channel order.
pub fn build(p: &Project) -> TimelineModel {
    let mut groups: Vec<GroupInfo> = Vec::new();
    let mut tracks: Vec<TrackInfo> = Vec::new();
    let mut index: BTreeMap<(usize, String), usize> = BTreeMap::new();
    let mut root_of: BTreeMap<usize, usize> = BTreeMap::new();
    // channel -> group, filled kind by kind but stored in channel order afterwards
    let mut owner: Vec<usize> = vec![0; p.channels.len()];
    for (ki, kind) in KIND_ORDER.iter().enumerate() {
        for (ci, c) in p.channels.iter().enumerate() {
            if c.prop.target_kind() != *kind {
                continue;
            }
            let root = *root_of.entry(ki).or_insert_with(|| {
                groups.push(GroupInfo {
                    id: groups.len(),
                    label: kind_label(*kind).into(),
                    kind: kind_name(*kind),
                    depth: 0,
                    parent: None,
                    children: vec![],
                    tracks: vec![],
                    frames: vec![],
                    locked: false,
                });
                groups.len() - 1
            });
            let mut parent = root;
            for (label, path) in chain_of(p, c, *kind) {
                let id = match index.get(&(ki, path.clone())) {
                    Some(&i) => i,
                    None => {
                        let id = groups.len();
                        groups.push(GroupInfo {
                            id,
                            label,
                            kind: kind_name(*kind),
                            depth: groups[parent].depth + 1,
                            parent: Some(parent),
                            children: vec![],
                            tracks: vec![],
                            frames: vec![],
                            locked: false,
                        });
                        groups[parent].children.push(id);
                        index.insert((ki, path), id);
                        id
                    }
                };
                parent = id;
            }
            groups[parent].tracks.push(ci);
            owner[ci] = parent;
        }
    }
    for (ci, c) in p.channels.iter().enumerate() {
        let (lock, shy, solo, color) = meta_for(p, &c.target, c.prop);
        let (source, keys, extrap) = match &c.source {
            Source::Keys { track } => (
                "keys",
                track
                    .keys
                    .iter()
                    .map(|k| KeyInfo {
                        frame: k.frame,
                        value: k.value,
                        ease: k.ease,
                    })
                    .collect(),
                Some(track.extrapolate),
            ),
            Source::Expr { .. } => ("expr", vec![], None),
            Source::Noise { .. } => ("noise", vec![], None),
            Source::Driven { .. } => ("driven", vec![], None),
        };
        let mut label = prop_name(c.prop);
        if c.prop.target_kind() == TargetKind::Sub {
            if let Some(last) = c
                .target
                .rsplit("::")
                .next()
                .filter(|_| c.target.contains("::"))
            {
                label = format!("{last} {label}");
            }
        }
        tracks.push(TrackInfo {
            ch: ci,
            target: c.target.clone(),
            prop: c.prop,
            label,
            source,
            editable: is_keyed(c),
            locked: lock,
            shy,
            solo,
            color,
            mode: c.mode,
            weight: c.weight,
            extrapolate: extrap,
            group: owner[ci],
            keys,
        });
    }
    // summaries bottom-up (children always have larger ids than their parents)
    for gi in (0..groups.len()).rev() {
        let mut fr: Vec<f32> = groups[gi]
            .tracks
            .iter()
            .flat_map(|&t| tracks[t].keys.iter().map(|k| k.frame))
            .collect();
        for &c in &groups[gi].children.clone() {
            fr.extend(groups[c].frames.iter().copied());
        }
        fr.sort_by(|a, b| a.total_cmp(b));
        fr.dedup();
        groups[gi].frames = fr;
        let mut editable = 0;
        let mut locked = 0;
        let mut stack = vec![gi];
        while let Some(x) = stack.pop() {
            for &t in &groups[x].tracks {
                if tracks[t].editable {
                    editable += 1;
                    if tracks[t].locked {
                        locked += 1;
                    }
                }
            }
            stack.extend(groups[x].children.iter().copied());
        }
        groups[gi].locked = editable > 0 && editable == locked;
    }
    let mut rows = Vec::new();
    fn walk(g: usize, groups: &[GroupInfo], rows: &mut Vec<Row>) {
        rows.push(Row::Group(g));
        for &t in &groups[g].tracks {
            rows.push(Row::Track(t));
        }
        for &c in &groups[g].children {
            walk(c, groups, rows);
        }
    }
    for g in groups.iter().filter(|g| g.parent.is_none()) {
        walk(g.id, &groups, &mut rows);
    }
    TimelineModel {
        frames: p.frames,
        fps: p.fps,
        groups,
        tracks,
        rows,
    }
}

// ---------------------------------------------------------------- selection

/// One key: channel index plus its exact frame.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyRef {
    pub ch: usize,
    pub frame: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelMode {
    #[default]
    Replace,
    Add,
    Toggle,
    Subtract,
}

impl SelMode {
    pub fn parse(s: &str) -> Option<SelMode> {
        serde_json::from_value(serde_json::Value::String(s.into())).ok()
    }
}

/// Sort by (channel, frame) and drop duplicates.
pub fn normalize(sel: &mut Vec<KeyRef>) {
    sel.sort_by(|a, b| a.ch.cmp(&b.ch).then(a.frame.total_cmp(&b.frame)));
    sel.dedup_by(|b, a| a.ch == b.ch && a.frame == b.frame);
}

/// Combine the current selection with `hits` according to `mode`.
pub fn select_apply(current: &[KeyRef], hits: &[KeyRef], mode: SelMode) -> Vec<KeyRef> {
    let has = |v: &[KeyRef], k: &KeyRef| v.iter().any(|x| x.ch == k.ch && x.frame == k.frame);
    let mut out: Vec<KeyRef> = match mode {
        SelMode::Replace => hits.to_vec(),
        SelMode::Add => current.iter().chain(hits).copied().collect(),
        SelMode::Subtract => current.iter().filter(|k| !has(hits, k)).copied().collect(),
        SelMode::Toggle => current
            .iter()
            .filter(|k| !has(hits, k))
            .chain(hits.iter().filter(|k| !has(current, k)))
            .copied()
            .collect(),
    };
    normalize(&mut out);
    out
}

fn track_keys(
    m: &TimelineModel,
    t: usize,
    f0: f32,
    f1: f32,
    include_locked: bool,
    out: &mut Vec<KeyRef>,
) {
    let tr = &m.tracks[t];
    if !tr.editable || (tr.locked && !include_locked) {
        return;
    }
    out.extend(
        tr.keys
            .iter()
            .filter(|k| k.frame >= f0 && k.frame <= f1)
            .map(|k| KeyRef {
                ch: t,
                frame: k.frame,
            }),
    );
}

fn descendants(m: &TimelineModel, g: usize, out: &mut Vec<usize>) {
    out.extend(m.groups[g].tracks.iter().copied());
    for &c in &m.groups[g].children {
        descendants(m, c, out);
    }
}

/// Keys inside `[f0, f1]` on the display rows `row0..=row1` (either order). A group row stands for
/// every track below it. Locked tracks are skipped unless `include_locked`.
pub fn select_box(
    m: &TimelineModel,
    f0: f32,
    f1: f32,
    row0: usize,
    row1: usize,
    include_locked: bool,
) -> Vec<KeyRef> {
    let (f0, f1) = (f0.min(f1), f0.max(f1));
    let (r0, r1) = (row0.min(row1), row0.max(row1));
    let mut chans: Vec<usize> = Vec::new();
    for r in m.rows.iter().skip(r0).take(r1.saturating_sub(r0) + 1) {
        match *r {
            Row::Track(t) => chans.push(t),
            Row::Group(g) => descendants(m, g, &mut chans),
        }
    }
    chans.sort_unstable();
    chans.dedup();
    let mut out = Vec::new();
    for t in chans {
        track_keys(m, t, f0, f1, include_locked, &mut out);
    }
    normalize(&mut out);
    out
}

/// Every key of one channel.
pub fn select_channel(m: &TimelineModel, ch: usize, include_locked: bool) -> Vec<KeyRef> {
    let mut out = Vec::new();
    if ch < m.tracks.len() {
        track_keys(
            m,
            ch,
            f32::NEG_INFINITY,
            f32::INFINITY,
            include_locked,
            &mut out,
        );
    }
    out
}

/// Every key below a group.
pub fn select_group(m: &TimelineModel, group: usize, include_locked: bool) -> Vec<KeyRef> {
    let mut out = Vec::new();
    if group < m.groups.len() {
        let mut chans = Vec::new();
        descendants(m, group, &mut chans);
        for t in chans {
            track_keys(
                m,
                t,
                f32::NEG_INFINITY,
                f32::INFINITY,
                include_locked,
                &mut out,
            );
        }
    }
    normalize(&mut out);
    out
}

/// Every key of every track.
pub fn select_all(m: &TimelineModel, include_locked: bool) -> Vec<KeyRef> {
    let mut out = Vec::new();
    for t in 0..m.tracks.len() {
        track_keys(
            m,
            t,
            f32::NEG_INFINITY,
            f32::INFINITY,
            include_locked,
            &mut out,
        );
    }
    out
}

/// Drop references that no longer point at a key (after an edit elsewhere).
pub fn prune(m: &TimelineModel, sel: &[KeyRef]) -> Vec<KeyRef> {
    sel.iter()
        .filter(|r| {
            m.tracks
                .get(r.ch)
                .is_some_and(|t| t.keys.iter().any(|k| k.frame == r.frame))
        })
        .copied()
        .collect()
}

// ---------------------------------------------------------------- options, plan, clip

/// What to do when an edit puts a key on a frame that already holds one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Collision {
    /// The incoming key overwrites the key already there.
    #[default]
    Replace,
    /// The key already there stays; the incoming key is dropped (frames are merged).
    Merge,
    /// Refuse the whole edit.
    Refuse,
}

impl Collision {
    pub fn parse(s: &str) -> Option<Collision> {
        serde_json::from_value(serde_json::Value::String(s.into())).ok()
    }
}

/// Allowed time range for edited keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bounds {
    /// `0 ..= project.frames` (the closing key of a loop sits on `frames`).
    #[default]
    Project,
    Free,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditOpts {
    /// Round moved / created key frames to whole frames.
    pub snap: bool,
    pub collision: Collision,
    pub bounds: Bounds,
}

impl Default for EditOpts {
    fn default() -> Self {
        EditOpts {
            snap: true,
            collision: Collision::Replace,
            bounds: Bounds::Project,
        }
    }
}

/// Keys copied out of a project, frames relative to the earliest copied key.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub tracks: Vec<ClipTrack>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipTrack {
    pub target: String,
    pub prop: Prop,
    pub keys: Vec<Key>,
}

impl Clip {
    /// Point every clip track that targets `from` at `to` instead (e.g. left arm -> right arm).
    pub fn retarget(&mut self, from: &str, to: &str) -> usize {
        let mut n = 0;
        for t in &mut self.tracks {
            if t.target == from {
                t.target = to.into();
                n += 1;
            }
        }
        n
    }
    pub fn key_count(&self) -> usize {
        self.tracks.iter().map(|t| t.keys.len()).sum()
    }
}

/// Destination override for [`TlOp::Paste`]: clip track `(from_target, from_prop)` lands on `(to_target, to_prop)`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Remap {
    pub from_target: String,
    pub from_prop: Prop,
    pub to_target: String,
    pub to_prop: Prop,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct EditPlan {
    /// The edit (a `Batch`); `None` when nothing changes (copy, zero move).
    pub op: Option<Op>,
    /// Where the edited keys are afterwards (indices valid after the op is applied).
    pub selection: Vec<KeyRef>,
    pub changed: usize,
    pub notes: Vec<String>,
    pub clip: Option<Clip>,
}

// ---------------------------------------------------------------- working copy

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Loc {
    Existing(usize),
    Fresh(usize),
}

struct Work<'a> {
    p: &'a Project,
    edited: BTreeMap<usize, Track>,
    fresh: Vec<(String, Prop, Track)>,
    extra: Vec<Op>,
    notes: Vec<String>,
}

fn keys_of(c: &Channel) -> Option<&Track> {
    match &c.source {
        Source::Keys { track } => Some(track),
        _ => None,
    }
}

impl<'a> Work<'a> {
    fn new(p: &'a Project) -> Self {
        Work {
            p,
            edited: BTreeMap::new(),
            fresh: Vec::new(),
            extra: Vec::new(),
            notes: Vec::new(),
        }
    }

    fn check(&self, ch: usize) -> Result<&'a Channel, String> {
        let c = self
            .p
            .channels
            .get(ch)
            .ok_or_else(|| format!("no channel {ch}"))?;
        if keys_of(c).is_none() {
            return Err(format!(
                "channel {ch} ({}.{}) is not a keyed channel",
                c.target,
                prop_name(c.prop)
            ));
        }
        if crate::skins::is_collapse_channel(c) {
            return Err(format!(
                "channel {ch} ({}) is a skin helper and cannot be edited here",
                c.target
            ));
        }
        chanmeta::assert_unlocked(self.p, &c.target, c.prop)?;
        Ok(c)
    }

    fn existing(&mut self, ch: usize) -> Result<&mut Track, String> {
        let c = self.check(ch)?;
        Ok(self
            .edited
            .entry(ch)
            .or_insert_with(|| keys_of(c).cloned().unwrap_or_default()))
    }

    fn locate(&mut self, target: &str, prop: Prop) -> Result<Loc, String> {
        chanmeta::assert_unlocked(self.p, target, prop)?;
        if let Some(i) = self
            .p
            .channels
            .iter()
            .position(|c| c.target == target && c.prop == prop && is_keyed(c))
        {
            self.existing(i)?;
            return Ok(Loc::Existing(i));
        }
        if let Some(j) = self
            .fresh
            .iter()
            .position(|(t, pr, _)| t == target && *pr == prop)
        {
            return Ok(Loc::Fresh(j));
        }
        self.fresh.push((target.into(), prop, Track::new()));
        Ok(Loc::Fresh(self.fresh.len() - 1))
    }

    fn track_at(&mut self, l: Loc) -> &mut Track {
        match l {
            Loc::Existing(i) => self.edited.get_mut(&i).expect("located before use"),
            Loc::Fresh(j) => &mut self.fresh[j].2,
        }
    }

    fn finish(self, sel: Vec<(Loc, f32)>, changed: usize) -> Result<EditPlan, String> {
        let mut ops: Vec<Op> = Vec::new();
        let mut removed: Vec<usize> = Vec::new();
        for (i, t) in &self.edited {
            let orig = keys_of(&self.p.channels[*i]).expect("checked");
            if t.keys.is_empty() {
                removed.push(*i);
            } else if t != orig {
                ops.push(Op::Set {
                    path: format!("/channels/{i}/track"),
                    value: serde_json::to_value(t).map_err(|e| e.to_string())?,
                });
            }
        }
        let mut fresh_pos: BTreeMap<usize, usize> = BTreeMap::new();
        for (j, (target, prop, t)) in self.fresh.iter().enumerate() {
            if t.keys.is_empty() {
                continue;
            }
            fresh_pos.insert(j, fresh_pos.len());
            let ch = Channel::keys(target, *prop, t.clone());
            ops.push(Op::Insert {
                path: "/channels/-".into(),
                value: serde_json::to_value(&ch).map_err(|e| e.to_string())?,
            });
        }
        ops.extend(self.extra);
        removed.sort_unstable_by(|a, b| b.cmp(a));
        for i in &removed {
            ops.push(Op::Remove {
                path: format!("/channels/{i}"),
            });
        }
        let n_after = self.p.channels.len() - removed.len();
        let mut selection: Vec<KeyRef> = sel
            .into_iter()
            .filter_map(|(l, f)| match l {
                Loc::Existing(i) if !removed.contains(&i) => Some(KeyRef {
                    ch: i - removed.iter().filter(|&&r| r < i).count(),
                    frame: f,
                }),
                Loc::Existing(_) => None,
                Loc::Fresh(j) => fresh_pos.get(&j).map(|&k| KeyRef {
                    ch: n_after + k,
                    frame: f,
                }),
            })
            .collect();
        normalize(&mut selection);
        Ok(EditPlan {
            op: if ops.is_empty() {
                None
            } else {
                Some(Op::Batch { ops })
            },
            selection,
            changed,
            notes: self.notes,
            clip: None,
        })
    }
}

fn keyed_track(p: &Project, ch: usize) -> Result<&Track, String> {
    let c = p
        .channels
        .get(ch)
        .ok_or_else(|| format!("no channel {ch}"))?;
    if !is_keyed(c) {
        return Err(format!(
            "channel {ch} ({}.{}) is not an editable keyed channel",
            c.target,
            prop_name(c.prop)
        ));
    }
    Ok(keys_of(c).expect("keyed"))
}

/// Validate a selection against the project; group the frames per channel.
fn gather(p: &Project, sel: &[KeyRef]) -> Result<BTreeMap<usize, Vec<f32>>, String> {
    if sel.is_empty() {
        return Err("no keys selected".into());
    }
    let mut by: BTreeMap<usize, Vec<f32>> = BTreeMap::new();
    for r in sel {
        let t = keyed_track(p, r.ch)?;
        if !r.frame.is_finite() || !t.keys.iter().any(|k| k.frame == r.frame) {
            return Err(format!(
                "stale key reference (channel {}, frame {})",
                r.ch, r.frame
            ));
        }
        let v = by.entry(r.ch).or_default();
        if !v.contains(&r.frame) {
            v.push(r.frame);
        }
    }
    for v in by.values_mut() {
        v.sort_by(|a, b| a.total_cmp(b));
    }
    Ok(by)
}

fn take_keys(t: &mut Track, frames: &[f32]) -> Vec<Key> {
    let mut taken = Vec::new();
    t.keys.retain(|k| {
        if frames.contains(&k.frame) {
            taken.push(*k);
            false
        } else {
            true
        }
    });
    taken
}

fn snapf(o: &EditOpts, f: f32) -> f32 {
    if o.snap {
        let r = f.round();
        if r == 0.0 {
            0.0
        } else {
            r
        }
    } else {
        f
    }
}

fn check_frame(p: &Project, o: &EditOpts, f: f32, what: &str) -> Result<(), String> {
    if !f.is_finite() {
        return Err(format!("{what}: frame is not finite"));
    }
    if o.bounds == Bounds::Project && (f < 0.0 || f > p.frames as f32) {
        return Err(format!(
            "{what}: frame {f} is outside the project range 0..={}",
            p.frames
        ));
    }
    Ok(())
}

/// Put `incoming` keys into `t`; returns the frames that were actually written.
fn place(
    t: &mut Track,
    mut incoming: Vec<Key>,
    c: Collision,
    notes: &mut Vec<String>,
) -> Result<Vec<f32>, String> {
    incoming.sort_by(|a, b| a.frame.total_cmp(&b.frame));
    let mut uniq: Vec<Key> = Vec::new();
    for k in incoming {
        if uniq.last().is_some_and(|m| m.frame == k.frame) {
            if c == Collision::Refuse {
                return Err(format!("two keys would land on frame {}", k.frame));
            }
            notes.push(format!(
                "two keys landed on frame {}: the earlier one was kept",
                k.frame
            ));
            continue;
        }
        uniq.push(k);
    }
    let mut written = Vec::new();
    for k in uniq {
        match t.keys.binary_search_by(|x| x.frame.total_cmp(&k.frame)) {
            Ok(i) => match c {
                Collision::Replace => {
                    t.keys[i] = k;
                    written.push(k.frame);
                }
                Collision::Merge => notes.push(format!(
                    "frame {} already had a key: kept it, dropped the incoming one",
                    k.frame
                )),
                Collision::Refuse => {
                    return Err(format!("a key already exists at frame {}", k.frame))
                }
            },
            Err(i) => {
                t.keys.insert(i, k);
                written.push(k.frame);
            }
        }
    }
    Ok(written)
}

fn finite(v: f32, what: &str) -> Result<(), String> {
    if v.is_finite() {
        Ok(())
    } else {
        Err(format!("{what} must be a finite number"))
    }
}

fn validate_ease(e: &Ease) -> Result<(), String> {
    match *e {
        Ease::CubicBezier { x1, y1, x2, y2 } => {
            if ![x1, y1, x2, y2].iter().all(|v| v.is_finite())
                || !(0.0..=1.0).contains(&x1)
                || !(0.0..=1.0).contains(&x2)
            {
                return Err("cubic-bezier needs finite values with x1 and x2 in 0..1".into());
            }
        }
        Ease::Spring { bounces, decay } => finite(bounces + decay, "spring parameters")?,
        Ease::Back { overshoot } => finite(overshoot, "back overshoot")?,
        _ => {}
    }
    Ok(())
}

// ---------------------------------------------------------------- operations

/// Move the selected keys by `dt` frames (snapped when `opts.snap`) and `dv` in value.
pub fn move_keys(
    p: &Project,
    sel: &[KeyRef],
    dt: f32,
    dv: f32,
    o: &EditOpts,
) -> Result<EditPlan, String> {
    finite(dt, "dt")?;
    finite(dv, "dv")?;
    let by = gather(p, sel)?;
    let mut w = Work::new(p);
    let mut out = Vec::new();
    let mut changed = 0;
    for (ch, frames) in &by {
        let t = w.existing(*ch)?;
        let mut moved = take_keys(t, frames);
        for k in &mut moved {
            if dt != 0.0 {
                k.frame = snapf(o, k.frame + dt);
            }
            k.value += dv;
            finite(k.value, "moved value")?;
        }
        for k in &moved {
            check_frame(p, o, k.frame, "move")?;
        }
        let mut notes = Vec::new();
        let written = place(t, moved, o.collision, &mut notes)?;
        w.notes.extend(notes);
        changed += written.len();
        out.extend(written.into_iter().map(|f| (Loc::Existing(*ch), f)));
    }
    if dt == 0.0 && dv == 0.0 {
        return Ok(EditPlan {
            selection: sel.to_vec(),
            ..Default::default()
        });
    }
    w.finish(out, changed)
}

/// Scale the spacing of the selected keys about `pivot` (factor 2 = twice as slow).
pub fn scale_keys(
    p: &Project,
    sel: &[KeyRef],
    pivot: f32,
    factor: f32,
    o: &EditOpts,
) -> Result<EditPlan, String> {
    finite(pivot, "pivot")?;
    if (factor.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)) || !factor.is_finite() {
        return Err("scale factor must be a positive finite number".into());
    }
    let by = gather(p, sel)?;
    let mut w = Work::new(p);
    let mut out = Vec::new();
    let mut changed = 0;
    for (ch, frames) in &by {
        let t = w.existing(*ch)?;
        let mut moved = take_keys(t, frames);
        for k in &mut moved {
            k.frame = snapf(o, pivot + (k.frame - pivot) * factor);
            check_frame(p, o, k.frame, "scale")?;
        }
        let mut notes = Vec::new();
        let written = place(t, moved, o.collision, &mut notes)?;
        w.notes.extend(notes);
        changed += written.len();
        out.extend(written.into_iter().map(|f| (Loc::Existing(*ch), f)));
    }
    w.finish(out, changed)
}

/// Copy the selected keys into a clip (not an edit).
pub fn copy_keys(p: &Project, sel: &[KeyRef]) -> Result<Clip, String> {
    let by = gather(p, sel)?;
    let origin = by
        .values()
        .flat_map(|v| v.iter().copied())
        .fold(f32::INFINITY, f32::min);
    let mut tracks: Vec<ClipTrack> = Vec::new();
    for (ch, frames) in &by {
        let c = &p.channels[*ch];
        let t = keyed_track(p, *ch)?;
        let keys: Vec<Key> = t
            .keys
            .iter()
            .filter(|k| frames.contains(&k.frame))
            .map(|k| Key {
                frame: k.frame - origin,
                ..*k
            })
            .collect();
        match tracks
            .iter_mut()
            .find(|x| x.target == c.target && x.prop == c.prop)
        {
            Some(x) => {
                x.keys.extend(keys);
                x.keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
                x.keys.dedup_by(|b, a| a.frame == b.frame);
            }
            None => tracks.push(ClipTrack {
                target: c.target.clone(),
                prop: c.prop,
                keys,
            }),
        }
    }
    Ok(Clip { tracks })
}

fn validate_clip(c: &Clip) -> Result<(), String> {
    if c.tracks.is_empty() || c.key_count() == 0 {
        return Err("the clip is empty".into());
    }
    if c.key_count() > 100_000 {
        return Err("the clip is too large".into());
    }
    for t in &c.tracks {
        for k in &t.keys {
            if !k.frame.is_finite() || !k.value.is_finite() {
                return Err("the clip has a non-finite key".into());
            }
            validate_ease(&k.ease)?;
        }
    }
    Ok(())
}

/// Paste a clip with its first key at frame `at`. `remap` redirects clip tracks to other channels
/// (the property may change only within the same target kind).
pub fn paste_clip(
    p: &Project,
    clip: &Clip,
    at: f32,
    remap: &[Remap],
    o: &EditOpts,
) -> Result<EditPlan, String> {
    finite(at, "at")?;
    validate_clip(clip)?;
    for r in remap {
        if !clip
            .tracks
            .iter()
            .any(|t| t.target == r.from_target && t.prop == r.from_prop)
        {
            return Err(format!(
                "remap source {}.{} is not in the clip",
                r.from_target,
                prop_name(r.from_prop)
            ));
        }
        if r.from_prop.target_kind() != r.to_prop.target_kind() {
            return Err(format!(
                "cannot remap {} onto {}: different kinds of target",
                prop_name(r.from_prop),
                prop_name(r.to_prop)
            ));
        }
    }
    let mut w = Work::new(p);
    let mut out = Vec::new();
    let mut changed = 0;
    for ct in &clip.tracks {
        let (target, prop) = match remap
            .iter()
            .find(|r| r.from_target == ct.target && r.from_prop == ct.prop)
        {
            Some(r) => (r.to_target.clone(), r.to_prop),
            None => (ct.target.clone(), ct.prop),
        };
        let mut keys: Vec<Key> = ct
            .keys
            .iter()
            .map(|k| Key {
                frame: snapf(o, at + k.frame),
                ..*k
            })
            .collect();
        for k in &keys {
            check_frame(p, o, k.frame, "paste")?;
        }
        keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        let loc = w.locate(&target, prop)?;
        let mut notes = Vec::new();
        let written = place(w.track_at(loc), keys, o.collision, &mut notes)?;
        w.notes.extend(notes);
        changed += written.len();
        out.extend(written.into_iter().map(|f| (loc, f)));
    }
    w.finish(out, changed)
}

/// Duplicate the selected keys `offset` frames later (or earlier) on the same channels.
pub fn duplicate_keys(
    p: &Project,
    sel: &[KeyRef],
    offset: f32,
    o: &EditOpts,
) -> Result<EditPlan, String> {
    finite(offset, "offset")?;
    let off = snapf(o, offset);
    if off == 0.0 {
        return Err("duplicate offset must not be zero".into());
    }
    let by = gather(p, sel)?;
    let mut w = Work::new(p);
    let mut out = Vec::new();
    let mut changed = 0;
    for (ch, frames) in &by {
        let t = w.existing(*ch)?;
        let copies: Vec<Key> = t
            .keys
            .iter()
            .filter(|k| frames.contains(&k.frame))
            .map(|k| Key {
                frame: snapf(o, k.frame + off),
                ..*k
            })
            .collect();
        for k in &copies {
            check_frame(p, o, k.frame, "duplicate")?;
        }
        // copies may land on keys of the selection itself: those are not moved, so they collide normally
        let mut notes = Vec::new();
        let written = place(t, copies, o.collision, &mut notes)?;
        w.notes.extend(notes);
        changed += written.len();
        out.extend(written.into_iter().map(|f| (Loc::Existing(*ch), f)));
    }
    w.finish(out, changed)
}

/// Delete the selected keys; a channel left without keys is removed (as `Editor::delete_key` does).
pub fn delete_keys(p: &Project, sel: &[KeyRef]) -> Result<EditPlan, String> {
    let by = gather(p, sel)?;
    let mut w = Work::new(p);
    let mut changed = 0;
    for (ch, frames) in &by {
        let t = w.existing(*ch)?;
        changed += take_keys(t, frames).len();
    }
    w.finish(Vec::new(), changed)
}

/// Set the interpolation of the segment leaving each selected key.
pub fn set_ease(p: &Project, sel: &[KeyRef], ease: Ease) -> Result<EditPlan, String> {
    validate_ease(&ease)?;
    let by = gather(p, sel)?;
    let mut w = Work::new(p);
    let mut out = Vec::new();
    let mut changed = 0;
    for (ch, frames) in &by {
        let t = w.existing(*ch)?;
        for k in t.keys.iter_mut().filter(|k| frames.contains(&k.frame)) {
            k.ease = ease;
            changed += 1;
        }
        out.extend(frames.iter().map(|&f| (Loc::Existing(*ch), f)));
    }
    w.finish(out, changed)
}

/// Give every selected key the same value.
pub fn set_value(p: &Project, sel: &[KeyRef], value: f32) -> Result<EditPlan, String> {
    finite(value, "value")?;
    let by = gather(p, sel)?;
    let mut w = Work::new(p);
    let mut out = Vec::new();
    let mut changed = 0;
    for (ch, frames) in &by {
        let t = w.existing(*ch)?;
        for k in t.keys.iter_mut().filter(|k| frames.contains(&k.frame)) {
            k.value = value;
            changed += 1;
        }
        out.extend(frames.iter().map(|&f| (Loc::Existing(*ch), f)));
    }
    w.finish(out, changed)
}

/// Add one key (value defaults to the channel's current curve value at that frame).
pub fn add_key(
    p: &Project,
    target: &str,
    prop: Prop,
    frame: f32,
    value: Option<f32>,
    ease: Ease,
    o: &EditOpts,
) -> Result<EditPlan, String> {
    validate_ease(&ease)?;
    let frame = snapf(o, frame);
    check_frame(p, o, frame, "add key")?;
    let mut w = Work::new(p);
    let loc = w.locate(target, prop)?;
    let v = match value {
        Some(v) => v,
        None => w
            .track_at(loc)
            .eval(frame)
            .ok_or_else(|| "a value is required for a channel that has no keys yet".to_string())?,
    };
    finite(v, "value")?;
    w.track_at(loc).set_key(frame, v, ease);
    w.finish(vec![(loc, frame)], 1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetimeMode {
    /// Open a gap of `count` frames at `at`; later keys move right.
    Insert,
    /// Remove frames `at .. at+count`: keys inside are deleted, later keys move left.
    Delete,
}

/// Ripple edit. `scope` lists channels; `None` means every unlocked keyed channel (locked ones are
/// skipped and reported in the notes). With `adjust_frames` the project length changes with it.
pub fn ripple(
    p: &Project,
    at: f32,
    count: f32,
    mode: RetimeMode,
    scope: Option<&[usize]>,
    adjust_frames: bool,
    o: &EditOpts,
) -> Result<EditPlan, String> {
    finite(at, "at")?;
    finite(count, "count")?;
    let (at, count) = (snapf(o, at), snapf(o, count));
    if count <= 0.0 {
        return Err("count must be positive".into());
    }
    let mut w = Work::new(p);
    let chans: Vec<usize> = match scope {
        Some(l) => l.to_vec(),
        None => {
            let mut v = Vec::new();
            for (i, c) in p.channels.iter().enumerate() {
                if !is_keyed(c) {
                    continue;
                }
                if chanmeta::is_locked(p, &c.target, c.prop) {
                    w.notes.push(format!(
                        "skipped locked channel {} {}",
                        c.target,
                        prop_name(c.prop)
                    ));
                } else {
                    v.push(i);
                }
            }
            v
        }
    };
    let new_frames = if adjust_frames {
        let nf = match mode {
            RetimeMode::Insert => p.frames as f32 + count,
            RetimeMode::Delete => p.frames as f32 - count,
        };
        if nf < 1.0 {
            return Err("the project would have no frames left".into());
        }
        w.extra.push(Op::Set {
            path: "/frames".into(),
            value: serde_json::json!(nf as u32),
        });
        nf as u32
    } else {
        p.frames
    };
    let po = Project {
        frames: new_frames,
        ..shallow(p)
    };
    let mut changed = 0;
    for ch in chans {
        let t = w.existing(ch)?;
        match mode {
            RetimeMode::Insert => {
                for k in t.keys.iter_mut().filter(|k| k.frame >= at) {
                    k.frame += count;
                    check_frame(&po, o, k.frame, "ripple insert")?;
                    changed += 1;
                }
            }
            RetimeMode::Delete => {
                let before = t.keys.len();
                t.keys.retain(|k| !(k.frame >= at && k.frame < at + count));
                changed += before - t.keys.len();
                for k in t.keys.iter_mut().filter(|k| k.frame >= at + count) {
                    k.frame -= count;
                    changed += 1;
                }
            }
        }
    }
    w.finish(Vec::new(), changed)
}

/// Only `frames` matters to `check_frame`; avoid cloning the whole project.
fn shallow(p: &Project) -> Project {
    Project::new("", p.width, p.height, p.fps, p.frames)
}

/// Shift whole channels in time.
pub fn offset_channels(
    p: &Project,
    chans: &[usize],
    dt: f32,
    o: &EditOpts,
) -> Result<EditPlan, String> {
    finite(dt, "dt")?;
    if chans.is_empty() {
        return Err("no channels given".into());
    }
    let dt = snapf(o, dt);
    let mut w = Work::new(p);
    let mut changed = 0;
    for &ch in chans {
        let t = w.existing(ch)?;
        for k in &mut t.keys {
            k.frame += dt;
            check_frame(p, o, k.frame, "offset")?;
            changed += 1;
        }
    }
    w.finish(Vec::new(), changed)
}

/// Set what channels do outside their key range.
pub fn set_extrapolate(p: &Project, chans: &[usize], e: Extrapolate) -> Result<EditPlan, String> {
    if chans.is_empty() {
        return Err("no channels given".into());
    }
    let mut w = Work::new(p);
    let mut changed = 0;
    for &ch in chans {
        w.existing(ch)?.extrapolate = e;
        changed += 1;
    }
    w.finish(Vec::new(), changed)
}

/// Make channels loop: optionally close the cycle with a key on `end` holding the first key's value,
/// then set the extrapolation (default `Loop`).
pub fn make_loop(
    p: &Project,
    chans: &[usize],
    end: Option<f32>,
    e: Extrapolate,
    o: &EditOpts,
) -> Result<EditPlan, String> {
    if chans.is_empty() {
        return Err("no channels given".into());
    }
    let mut w = Work::new(p);
    let mut out = Vec::new();
    let mut changed = 0;
    for &ch in chans {
        let t = w.existing(ch)?;
        if t.keys.len() < 2 {
            return Err(format!("channel {ch} needs at least two keys to loop"));
        }
        if let Some(end) = end {
            let end = snapf(o, end);
            check_frame(p, o, end, "loop end")?;
            let first = t.keys[0];
            if end <= first.frame {
                return Err(format!(
                    "loop end {end} must be after the first key ({})",
                    first.frame
                ));
            }
            if t.keys.last().is_some_and(|l| l.frame > end) {
                return Err(format!(
                    "loop end {end} is before the last key of channel {ch}"
                ));
            }
            t.set_key(end, first.value, first.ease);
            out.push((Loc::Existing(ch), end));
        }
        t.extrapolate = e;
        changed += 1;
    }
    w.finish(out, changed)
}

/// Move one Bezier handle of the key at `frame` on channel `ch`.
pub fn move_handle(
    p: &Project,
    ch: usize,
    frame: f32,
    side: Side,
    pos: HandlePos,
    mode: HandleMode,
) -> Result<EditPlan, String> {
    keyed_track(p, ch)?;
    let mut w = Work::new(p);
    let t = w.existing(ch)?;
    let i = t
        .keys
        .iter()
        .position(|k| k.frame == frame)
        .ok_or_else(|| format!("no key at frame {frame} on channel {ch}"))?;
    g::set_handle(t, i, side, pos, mode)?;
    w.finish(vec![(Loc::Existing(ch), frame)], 1)
}

/// Auto-smooth the segments leaving the selected keys.
pub fn smooth_keys(p: &Project, sel: &[KeyRef], kind: TangentKind) -> Result<EditPlan, String> {
    let by = gather(p, sel)?;
    let mut w = Work::new(p);
    let mut out = Vec::new();
    let mut changed = 0;
    for (ch, frames) in &by {
        let t = w.existing(*ch)?;
        let starts: Vec<usize> = t
            .keys
            .iter()
            .enumerate()
            .filter(|(_, k)| frames.contains(&k.frame))
            .map(|(i, _)| i)
            .collect();
        changed += g::smooth_segments(t, &starts, kind);
        out.extend(frames.iter().map(|&f| (Loc::Existing(*ch), f)));
    }
    w.finish(out, changed)
}

/// Channel flags (lock / shy / solo / colour) as an edit, through `chanmeta::set_meta`.
pub fn set_flags(
    p: &Project,
    target: &str,
    prop: Option<Prop>,
    c: &chanmeta::Change,
) -> Result<EditPlan, String> {
    let mut q = shallow(p);
    q.channels = p.channels.clone();
    q.channel_meta = p.channel_meta.clone();
    chanmeta::set_meta(&mut q, target, prop, c)?;
    let v = serde_json::to_value(&q.channel_meta).map_err(|e| e.to_string())?;
    Ok(EditPlan {
        op: Some(Op::Batch {
            ops: vec![Op::Set {
                path: "/channel_meta".into(),
                value: v,
            }],
        }),
        changed: 1,
        ..Default::default()
    })
}

// ---------------------------------------------------------------- one JSON-able op enum

/// Everything the studio and the CLI can ask of the timeline, as JSON (`{"op": "move_keys", ...}`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum TlOp {
    MoveKeys {
        keys: Vec<KeyRef>,
        #[serde(default)]
        dt: f32,
        #[serde(default)]
        dv: f32,
        #[serde(default)]
        opts: EditOpts,
    },
    ScaleKeys {
        keys: Vec<KeyRef>,
        pivot: f32,
        factor: f32,
        #[serde(default)]
        opts: EditOpts,
    },
    Copy {
        keys: Vec<KeyRef>,
    },
    Paste {
        clip: Clip,
        at: f32,
        #[serde(default)]
        remap: Vec<Remap>,
        #[serde(default)]
        opts: EditOpts,
    },
    Duplicate {
        keys: Vec<KeyRef>,
        offset: f32,
        #[serde(default)]
        opts: EditOpts,
    },
    Delete {
        keys: Vec<KeyRef>,
    },
    SetEase {
        keys: Vec<KeyRef>,
        ease: Ease,
    },
    SetValue {
        keys: Vec<KeyRef>,
        value: f32,
    },
    AddKey {
        target: String,
        prop: Prop,
        frame: f32,
        #[serde(default)]
        value: Option<f32>,
        #[serde(default)]
        ease: Option<Ease>,
        #[serde(default)]
        opts: EditOpts,
    },
    Ripple {
        at: f32,
        count: f32,
        mode: RetimeMode,
        #[serde(default)]
        scope: Option<Vec<usize>>,
        #[serde(default)]
        adjust_frames: bool,
        #[serde(default)]
        opts: EditOpts,
    },
    Offset {
        channels: Vec<usize>,
        dt: f32,
        #[serde(default)]
        opts: EditOpts,
    },
    Extrapolate {
        channels: Vec<usize>,
        extrapolate: Extrapolate,
    },
    MakeLoop {
        channels: Vec<usize>,
        #[serde(default)]
        end: Option<f32>,
        #[serde(default)]
        extrapolate: Option<Extrapolate>,
        #[serde(default)]
        opts: EditOpts,
    },
    Handle {
        ch: usize,
        frame: f32,
        side: Side,
        t: f32,
        v: f32,
        #[serde(default)]
        mode: HandleMode,
    },
    Smooth {
        keys: Vec<KeyRef>,
        kind: TangentKind,
    },
    Flags {
        target: String,
        #[serde(default)]
        prop: Option<Prop>,
        #[serde(default)]
        lock: Option<bool>,
        #[serde(default)]
        shy: Option<bool>,
        #[serde(default)]
        solo: Option<bool>,
        #[serde(default)]
        color: Option<String>,
    },
}

/// Plan any [`TlOp`] against the project.
pub fn plan(p: &Project, op: &TlOp) -> Result<EditPlan, String> {
    match op {
        TlOp::MoveKeys { keys, dt, dv, opts } => move_keys(p, keys, *dt, *dv, opts),
        TlOp::ScaleKeys {
            keys,
            pivot,
            factor,
            opts,
        } => scale_keys(p, keys, *pivot, *factor, opts),
        TlOp::Copy { keys } => Ok(EditPlan {
            clip: Some(copy_keys(p, keys)?),
            selection: keys.clone(),
            ..Default::default()
        }),
        TlOp::Paste {
            clip,
            at,
            remap,
            opts,
        } => paste_clip(p, clip, *at, remap, opts),
        TlOp::Duplicate { keys, offset, opts } => duplicate_keys(p, keys, *offset, opts),
        TlOp::Delete { keys } => delete_keys(p, keys),
        TlOp::SetEase { keys, ease } => set_ease(p, keys, *ease),
        TlOp::SetValue { keys, value } => set_value(p, keys, *value),
        TlOp::AddKey {
            target,
            prop,
            frame,
            value,
            ease,
            opts,
        } => add_key(
            p,
            target,
            *prop,
            *frame,
            *value,
            ease.unwrap_or(Ease::Linear),
            opts,
        ),
        TlOp::Ripple {
            at,
            count,
            mode,
            scope,
            adjust_frames,
            opts,
        } => ripple(
            p,
            *at,
            *count,
            *mode,
            scope.as_deref(),
            *adjust_frames,
            opts,
        ),
        TlOp::Offset { channels, dt, opts } => offset_channels(p, channels, *dt, opts),
        TlOp::Extrapolate {
            channels,
            extrapolate,
        } => set_extrapolate(p, channels, *extrapolate),
        TlOp::MakeLoop {
            channels,
            end,
            extrapolate,
            opts,
        } => make_loop(
            p,
            channels,
            *end,
            extrapolate.unwrap_or(Extrapolate::Loop),
            opts,
        ),
        TlOp::Handle {
            ch,
            frame,
            side,
            t,
            v,
            mode,
        } => move_handle(p, *ch, *frame, *side, HandlePos { t: *t, v: *v }, *mode),
        TlOp::Smooth { keys, kind } => smooth_keys(p, keys, *kind),
        TlOp::Flags {
            target,
            prop,
            lock,
            shy,
            solo,
            color,
        } => set_flags(
            p,
            target,
            *prop,
            &chanmeta::Change {
                color: color.clone(),
                shy: *shy,
                solo: *solo,
                lock: *lock,
            },
        ),
    }
}

/// Plan and apply in one call (one undo step). Returns the plan; a plan without an op is not applied.
pub fn apply(ed: &mut crate::edit::Editor, op: &TlOp) -> Result<EditPlan, Vec<String>> {
    let pl = plan(ed.project(), op).map_err(|e| vec![e])?;
    if let Some(o) = &pl.op {
        ed.apply(o.clone())?;
    }
    Ok(pl)
}

// ---------------------------------------------------------------- graph view

#[derive(Clone, Debug, Serialize)]
pub struct GraphKey {
    pub frame: f32,
    pub value: f32,
    pub ease: Ease,
    #[serde(rename = "in")]
    pub inn: Option<HandlePos>,
    pub out: Option<HandlePos>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GraphChannel {
    pub ch: usize,
    pub target: String,
    pub prop: Prop,
    pub color: Option<String>,
    pub locked: bool,
    pub samples: Vec<[f32; 2]>,
    pub keys: Vec<GraphKey>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GraphView {
    pub f0: f32,
    pub f1: f32,
    /// Auto-fitted `[lo, hi]` over all returned channels.
    pub range: [f32; 2],
    pub channels: Vec<GraphChannel>,
    pub skipped: Vec<String>,
}

/// Curves, keys and handles for the graph editor. Non-keyed channels are listed in `skipped`.
pub fn graph_view(
    p: &Project,
    chans: &[usize],
    f0: f32,
    f1: f32,
    samples: usize,
) -> Result<GraphView, String> {
    if (f1.partial_cmp(&f0) != Some(std::cmp::Ordering::Greater))
        || !f0.is_finite()
        || !f1.is_finite()
    {
        return Err("graph range must be f0 < f1".into());
    }
    let m = build(p);
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    let mut out = Vec::new();
    let mut skipped = Vec::new();
    for &ch in chans.iter().take(64) {
        let c = p
            .channels
            .get(ch)
            .ok_or_else(|| format!("no channel {ch}"))?;
        let Some(t) = keys_of(c).filter(|_| is_keyed(c)) else {
            skipped.push(format!(
                "channel {ch} ({}.{}) has no editable curve",
                c.target,
                prop_name(c.prop)
            ));
            continue;
        };
        if let Some((a, b)) = g::value_range(t, f0, f1, samples, 0.0) {
            lo = lo.min(a);
            hi = hi.max(b);
        }
        let hs = g::handles(t);
        out.push(GraphChannel {
            ch,
            target: c.target.clone(),
            prop: c.prop,
            color: m.tracks[ch].color.clone(),
            locked: m.tracks[ch].locked,
            samples: g::sample_track(t, f0, f1, samples)
                .into_iter()
                .map(|(a, b)| [a, b])
                .collect(),
            keys: t
                .keys
                .iter()
                .zip(hs)
                .map(|(k, h)| GraphKey {
                    frame: k.frame,
                    value: k.value,
                    ease: k.ease,
                    inn: h.inn,
                    out: h.out,
                })
                .collect(),
        });
    }
    let range = if lo.is_finite() && hi.is_finite() {
        let pad = if hi - lo > 1e-6 {
            (hi - lo) * 0.08
        } else {
            0.5
        };
        [lo - pad, hi + pad]
    } else {
        [-1.0, 1.0]
    };
    Ok(GraphView {
        f0,
        f1,
        range,
        channels: out,
        skipped,
    })
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::Editor;
    use crate::math::Vec2;
    use crate::project::Bone;

    fn lin(k: &[(f32, f32)]) -> Track {
        Track::with_keys(
            &k.iter()
                .map(|&(f, v)| (f, v, Ease::Linear))
                .collect::<Vec<_>>(),
        )
    }

    /// Rig: root -> arm bones with channels, a layer, a camera.
    fn proj() -> Project {
        let mut p = Project::new("t", 100.0, 100.0, 24.0, 48);
        p.bones = vec![
            Bone::new("root", None, 10.0, Vec2::ZERO, 0.0),
            Bone::new("arm", Some("root"), 10.0, Vec2::ZERO, 0.0),
            Bone::new("arm_r", Some("root"), 10.0, Vec2::ZERO, 0.0),
        ];
        p.channels = vec![
            Channel::keys(
                "arm",
                Prop::BoneAngle,
                lin(&[(0.0, 0.0), (12.0, 30.0), (24.0, 60.0)]),
            ),
            Channel::keys("arm", Prop::BoneScale, lin(&[(0.0, 1.0), (24.0, 2.0)])),
            Channel::keys("root", Prop::BoneOffsetX, lin(&[(0.0, 0.0), (48.0, 10.0)])),
            Channel::expr("arm_r", Prop::BoneAngle, "0", Mode::Add),
            Channel::keys("", Prop::CameraZoom, lin(&[(0.0, 1.0), (48.0, 1.5)])),
        ];
        p
    }

    fn kr(ch: usize, frame: f32) -> KeyRef {
        KeyRef { ch, frame }
    }

    fn keys_of_ch(p: &Project, ch: usize) -> Vec<(f32, f32)> {
        keys_of(&p.channels[ch])
            .unwrap()
            .keys
            .iter()
            .map(|k| (k.frame, k.value))
            .collect()
    }

    fn run(e: &mut Editor, op: TlOp) -> EditPlan {
        apply(e, &op).unwrap()
    }

    #[test]
    fn model_groups_hierarchically_with_metadata() {
        let mut p = proj();
        chanmeta::set_meta(
            &mut p,
            "arm",
            Some(Prop::BoneAngle),
            &chanmeta::Change {
                lock: Some(true),
                color: Some("#ff8800".into()),
                ..Default::default()
            },
        )
        .unwrap();
        chanmeta::set_meta(
            &mut p,
            "root",
            None,
            &chanmeta::Change {
                shy: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let m = build(&p);
        assert_eq!(m.tracks.len(), 5);
        assert!(m.tracks.iter().enumerate().all(|(i, t)| t.ch == i));
        let labels: Vec<_> = m
            .groups
            .iter()
            .map(|g| (g.label.as_str(), g.depth, g.parent))
            .collect();
        // Bones > root > arm / arm_r ; Camera
        assert_eq!(labels[0], ("Bones", 0, None));
        assert!(labels.contains(&("root", 1, Some(0))));
        assert!(labels.iter().any(|l| l.0 == "arm" && l.1 == 2));
        assert!(labels.iter().any(|l| l.0 == "arm_r" && l.1 == 2));
        assert!(labels.iter().any(|l| l.0 == "Camera" && l.1 == 0));
        let arm = &m.tracks[0];
        assert!(arm.locked && arm.editable && arm.color.as_deref() == Some("#ff8800"));
        assert!(!m.tracks[1].locked, "lock is per prop");
        assert!(m.tracks[2].shy, "target-wide shy");
        assert_eq!(m.tracks[3].source, "expr");
        assert!(!m.tracks[3].editable && m.tracks[3].keys.is_empty());
        assert_eq!(m.tracks[0].keys.len(), 3);
        assert_eq!(m.tracks[0].label, "bone_angle");
        // group summary: the Bones header sees every bone key frame
        assert_eq!(m.groups[0].frames, vec![0.0, 12.0, 24.0, 48.0]);
        // depth-first rows: header, root's own track, then children
        assert_eq!(m.rows[0], Row::Group(0));
        assert_eq!(m.rows.len(), m.groups.len() + m.tracks.len());
        let ri = m.rows.iter().position(|r| *r == Row::Track(2)).unwrap();
        let ai = m.rows.iter().position(|r| *r == Row::Track(0)).unwrap();
        assert!(ri < ai, "parent's tracks come before the child bone's");
        // deterministic
        assert_eq!(
            serde_json::to_string(&build(&p)).unwrap(),
            serde_json::to_string(&build(&p)).unwrap()
        );
    }

    #[test]
    fn model_group_lock_summary_and_layer_paths() {
        let mut p = proj();
        chanmeta::set_meta(
            &mut p,
            "arm",
            None,
            &chanmeta::Change {
                lock: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let m = build(&p);
        let arm_g = m.groups.iter().find(|g| g.label == "arm").unwrap();
        assert!(arm_g.locked);
        assert!(!m.groups.iter().find(|g| g.label == "Bones").unwrap().locked);
        // layer path "grp/leaf" nests
        let mut q = Project::new("l", 10.0, 10.0, 24.0, 10);
        q.channels = vec![
            Channel::keys("grp/leaf", Prop::LayerX, lin(&[(0.0, 0.0), (5.0, 1.0)])),
            Channel::keys("grp", Prop::LayerOpacity, lin(&[(0.0, 1.0)])),
        ];
        let m = build(&q);
        let leaf = m.groups.iter().find(|g| g.label == "leaf").unwrap();
        assert_eq!(m.groups[leaf.parent.unwrap()].label, "grp");
        assert_eq!(m.tracks[1].group, leaf.parent.unwrap());
        // sub targets: layer::deformer::idx
        let mut s = Project::new("s", 10.0, 10.0, 24.0, 10);
        s.channels = vec![Channel::keys(
            "L::lat::1,2",
            Prop::LatticeDx,
            lin(&[(0.0, 0.0)]),
        )];
        let m = build(&s);
        assert_eq!(m.tracks[0].label, "1,2 lattice_dx");
        assert_eq!(m.groups[m.tracks[0].group].label, "lat");
    }

    #[test]
    fn selection_box_toggle_channel_and_prune() {
        let p = proj();
        let m = build(&p);
        let ri = m.rows.iter().position(|r| *r == Row::Track(0)).unwrap();
        let b = select_box(&m, 10.0, 30.0, ri, ri, false);
        assert_eq!(b, vec![kr(0, 12.0), kr(0, 24.0)]);
        // reversed arguments, group row selects descendants (bones header covers channels 0,1,2)
        let all = select_box(&m, 100.0, -5.0, 0, ri, false);
        assert!(
            all.contains(&kr(0, 0.0))
                && all.contains(&kr(1, 24.0))
                && all.contains(&kr(2, 48.0))
                && !all.iter().any(|k| k.ch == 4)
        );
        assert_eq!(select_channel(&m, 1, false), vec![kr(1, 0.0), kr(1, 24.0)]);
        assert_eq!(select_all(&m, false).len(), 3 + 2 + 2 + 2);
        let g = m.tracks[0].group;
        assert_eq!(
            select_group(&m, g, false).len(),
            5,
            "arm: three angle keys and two scale keys"
        );
        assert_eq!(
            select_group(&m, m.tracks[2].group, false).len(),
            7,
            "root includes its child bones"
        );
        // modes
        let a = vec![kr(0, 0.0), kr(0, 12.0)];
        let h = vec![kr(0, 12.0), kr(1, 0.0)];
        assert_eq!(
            select_apply(&a, &h, SelMode::Replace),
            vec![kr(0, 12.0), kr(1, 0.0)]
        );
        assert_eq!(
            select_apply(&a, &h, SelMode::Add),
            vec![kr(0, 0.0), kr(0, 12.0), kr(1, 0.0)]
        );
        assert_eq!(
            select_apply(&a, &h, SelMode::Toggle),
            vec![kr(0, 0.0), kr(1, 0.0)]
        );
        assert_eq!(select_apply(&a, &h, SelMode::Subtract), vec![kr(0, 0.0)]);
        assert_eq!(SelMode::parse("toggle"), Some(SelMode::Toggle));
        assert_eq!(
            prune(&m, &[kr(0, 12.0), kr(0, 13.0), kr(9, 0.0)]),
            vec![kr(0, 12.0)]
        );
        // locked channels are skipped by default
        let mut q = proj();
        chanmeta::set_meta(
            &mut q,
            "arm",
            Some(Prop::BoneAngle),
            &chanmeta::Change {
                lock: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let mq = build(&q);
        assert!(select_channel(&mq, 0, false).is_empty());
        assert_eq!(select_channel(&mq, 0, true).len(), 3);
    }

    #[test]
    fn move_keys_exact_snap_bounds_and_undo() {
        let mut e = Editor::new(proj()).unwrap();
        let before = e.doc().clone();
        let pl = run(
            &mut e,
            TlOp::MoveKeys {
                keys: vec![kr(0, 12.0), kr(0, 24.0)],
                dt: 3.4,
                dv: 5.0,
                opts: EditOpts::default(),
            },
        );
        assert_eq!(
            keys_of_ch(e.project(), 0),
            vec![(0.0, 0.0), (15.0, 35.0), (27.0, 65.0)],
            "3.4 snaps to 3"
        );
        assert_eq!(pl.selection, vec![kr(0, 15.0), kr(0, 27.0)]);
        assert_eq!(pl.changed, 2);
        assert!(e.undo());
        assert_eq!(e.doc(), &before, "undo restores the document exactly");
        // no snap keeps fractions
        let o = EditOpts {
            snap: false,
            ..Default::default()
        };
        run(
            &mut e,
            TlOp::MoveKeys {
                keys: vec![kr(0, 12.0)],
                dt: 0.5,
                dv: 0.0,
                opts: o,
            },
        );
        assert_eq!(keys_of_ch(e.project(), 0)[1], (12.5, 30.0));
        e.undo();
        // value only: frames are not snapped
        let o = EditOpts {
            snap: true,
            ..Default::default()
        };
        let pl = move_keys(e.project(), &[kr(0, 12.0)], 0.0, 1.0, &o).unwrap();
        assert_eq!(pl.selection, vec![kr(0, 12.0)]);
        // bounds
        let err = move_keys(e.project(), &[kr(0, 24.0)], 30.0, 0.0, &o).unwrap_err();
        assert!(err.contains("outside"), "{err}");
        assert!(move_keys(e.project(), &[kr(0, 0.0)], -1.0, 0.0, &o).is_err());
        let free = EditOpts {
            bounds: Bounds::Free,
            ..Default::default()
        };
        assert!(move_keys(e.project(), &[kr(0, 0.0)], -1.0, 0.0, &free).is_ok());
        // zero move is a no-op without an op
        let z = move_keys(e.project(), &[kr(0, 0.0)], 0.0, 0.0, &o).unwrap();
        assert!(z.op.is_none());
        // validation of inputs
        assert!(move_keys(e.project(), &[kr(0, 0.0)], f32::NAN, 0.0, &o).is_err());
        assert!(move_keys(e.project(), &[], 1.0, 0.0, &o)
            .unwrap_err()
            .contains("no keys"));
        assert!(move_keys(e.project(), &[kr(0, 7.0)], 1.0, 0.0, &o)
            .unwrap_err()
            .contains("stale"));
        assert!(move_keys(e.project(), &[kr(99, 0.0)], 1.0, 0.0, &o).is_err());
        assert!(
            move_keys(e.project(), &[kr(3, 0.0)], 1.0, 0.0, &o)
                .unwrap_err()
                .contains("not an editable keyed"),
            "expression channel"
        );
    }

    #[test]
    fn collision_modes_replace_merge_refuse() {
        let p = proj();
        let sel = [kr(0, 0.0)];
        // move key 0 onto key 12
        let rep = move_keys(&p, &sel, 12.0, 0.0, &EditOpts::default()).unwrap();
        let mut e = Editor::new(p.clone()).unwrap();
        e.apply(rep.op.unwrap()).unwrap();
        assert_eq!(
            keys_of_ch(e.project(), 0),
            vec![(12.0, 0.0), (24.0, 60.0)],
            "moved key replaced the one at 12"
        );
        let merge = move_keys(
            &p,
            &sel,
            12.0,
            0.0,
            &EditOpts {
                collision: Collision::Merge,
                ..Default::default()
            },
        )
        .unwrap();
        let mut e = Editor::new(p.clone()).unwrap();
        e.apply(merge.op.clone().unwrap()).unwrap();
        assert_eq!(
            keys_of_ch(e.project(), 0),
            vec![(12.0, 30.0), (24.0, 60.0)],
            "existing key kept, incoming dropped"
        );
        assert!(merge.notes.iter().any(|n| n.contains("kept it")));
        assert!(merge.selection.is_empty());
        let r = move_keys(
            &p,
            &sel,
            12.0,
            0.0,
            &EditOpts {
                collision: Collision::Refuse,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(r.contains("already exists"), "{r}");
        // two moved keys landing on one frame: scale 0.01 about 0 with snap collapses 0,12 -> 0,0
        let sc = scale_keys(
            &p,
            &[kr(0, 0.0), kr(0, 12.0)],
            0.0,
            0.01,
            &EditOpts::default(),
        )
        .unwrap();
        assert!(sc.notes.iter().any(|n| n.contains("earlier one was kept")));
        assert!(scale_keys(
            &p,
            &[kr(0, 0.0), kr(0, 12.0)],
            0.0,
            0.01,
            &EditOpts {
                collision: Collision::Refuse,
                ..Default::default()
            }
        )
        .is_err());
    }

    #[test]
    fn scale_about_pivot_is_exact() {
        let mut e = Editor::new(proj()).unwrap();
        run(
            &mut e,
            TlOp::ScaleKeys {
                keys: vec![kr(0, 12.0), kr(0, 24.0)],
                pivot: 12.0,
                factor: 0.5,
                opts: EditOpts::default(),
            },
        );
        assert_eq!(
            keys_of_ch(e.project(), 0),
            vec![(0.0, 0.0), (12.0, 30.0), (18.0, 60.0)]
        );
        assert!(scale_keys(e.project(), &[kr(0, 0.0)], 0.0, 0.0, &EditOpts::default()).is_err());
        assert!(scale_keys(
            e.project(),
            &[kr(0, 0.0)],
            0.0,
            f32::INFINITY,
            &EditOpts::default()
        )
        .is_err());
        assert!(scale_keys(
            e.project(),
            &[kr(0, 0.0)],
            f32::NAN,
            2.0,
            &EditOpts::default()
        )
        .is_err());
        // scaling past the end is refused with project bounds
        assert!(scale_keys(e.project(), &[kr(0, 18.0)], 0.0, 10.0, &EditOpts::default()).is_err());
    }

    #[test]
    fn copy_paste_with_remap_and_new_channel() {
        let mut e = Editor::new(proj()).unwrap();
        let before = e.doc().clone();
        let clip = copy_keys(e.project(), &[kr(0, 12.0), kr(0, 24.0), kr(1, 24.0)]).unwrap();
        assert_eq!(clip.tracks.len(), 2);
        assert_eq!(
            clip.tracks[0]
                .keys
                .iter()
                .map(|k| k.frame)
                .collect::<Vec<_>>(),
            vec![0.0, 12.0]
        );
        assert_eq!(
            clip.tracks[1].keys[0].frame, 12.0,
            "relative to the earliest copied key"
        );
        // identity paste at 30 lands inside the same channel
        let pl = run(
            &mut e,
            TlOp::Paste {
                clip: clip.clone(),
                at: 30.0,
                remap: vec![],
                opts: EditOpts::default(),
            },
        );
        assert_eq!(
            keys_of_ch(e.project(), 0),
            vec![
                (0.0, 0.0),
                (12.0, 30.0),
                (24.0, 60.0),
                (30.0, 30.0),
                (42.0, 60.0)
            ]
        );
        assert_eq!(pl.changed, 3);
        e.undo();
        assert_eq!(e.doc(), &before);
        // remap arm.angle -> arm_r.angle (no keyed channel there: a new channel is created, selection points at it)
        let rm = Remap {
            from_target: "arm".into(),
            from_prop: Prop::BoneAngle,
            to_target: "arm_r".into(),
            to_prop: Prop::BoneAngle,
        };
        let n = e.project().channels.len();
        let pl = run(
            &mut e,
            TlOp::Paste {
                clip: clip.clone(),
                at: 0.0,
                remap: vec![rm],
                opts: EditOpts::default(),
            },
        );
        assert_eq!(e.project().channels.len(), n + 1);
        let c = &e.project().channels[n];
        assert_eq!((c.target.as_str(), c.prop), ("arm_r", Prop::BoneAngle));
        assert_eq!(keys_of_ch(e.project(), n), vec![(0.0, 30.0), (12.0, 60.0)]);
        assert!(
            pl.selection.contains(&kr(n, 0.0))
                && pl.selection.contains(&kr(n, 12.0))
                && pl.selection.contains(&kr(1, 12.0))
        );
        e.undo();
        assert_eq!(e.project().channels.len(), n);
        // retarget helper
        let mut c2 = clip.clone();
        assert_eq!(c2.retarget("arm", "arm_r"), 2);
        // errors
        let bad = Remap {
            from_target: "nope".into(),
            from_prop: Prop::BoneAngle,
            to_target: "arm".into(),
            to_prop: Prop::BoneAngle,
        };
        assert!(paste_clip(e.project(), &clip, 0.0, &[bad], &EditOpts::default()).is_err());
        let kind = Remap {
            from_target: "arm".into(),
            from_prop: Prop::BoneAngle,
            to_target: "arm".into(),
            to_prop: Prop::LayerX,
        };
        assert!(
            paste_clip(e.project(), &clip, 0.0, &[kind], &EditOpts::default())
                .unwrap_err()
                .contains("different kinds")
        );
        assert!(paste_clip(
            e.project(),
            &Clip { tracks: vec![] },
            0.0,
            &[],
            &EditOpts::default()
        )
        .is_err());
        assert!(
            paste_clip(e.project(), &clip, 40.0, &[], &EditOpts::default()).is_err(),
            "would run past the project end"
        );
        let mut nan = clip.clone();
        nan.tracks[0].keys[0].value = f32::NAN;
        assert!(paste_clip(e.project(), &nan, 0.0, &[], &EditOpts::default()).is_err());
        // merge mode on paste keeps existing keys
        let m = paste_clip(
            e.project(),
            &clip,
            12.0,
            &[],
            &EditOpts {
                collision: Collision::Merge,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(m.changed, 0);
        assert!(m.op.is_none());
    }

    #[test]
    fn duplicate_delete_ease_value_and_add_key() {
        let mut e = Editor::new(proj()).unwrap();
        let before = e.doc().clone();
        run(
            &mut e,
            TlOp::Duplicate {
                keys: vec![kr(0, 0.0), kr(0, 12.0)],
                offset: 30.0,
                opts: EditOpts::default(),
            },
        );
        assert_eq!(
            keys_of_ch(e.project(), 0),
            vec![
                (0.0, 0.0),
                (12.0, 30.0),
                (24.0, 60.0),
                (30.0, 0.0),
                (42.0, 30.0)
            ]
        );
        e.undo();
        assert!(
            duplicate_keys(e.project(), &[kr(0, 0.0)], 0.2, &EditOpts::default())
                .unwrap_err()
                .contains("zero")
        );
        // delete: partial and whole channel (channel is removed, indices shift, undo restores)
        run(
            &mut e,
            TlOp::Delete {
                keys: vec![kr(0, 12.0)],
            },
        );
        assert_eq!(keys_of_ch(e.project(), 0), vec![(0.0, 0.0), (24.0, 60.0)]);
        e.undo();
        let n = e.project().channels.len();
        let pl = run(
            &mut e,
            TlOp::Delete {
                keys: vec![kr(1, 0.0), kr(1, 24.0), kr(2, 0.0)],
            },
        );
        assert_eq!(e.project().channels.len(), n - 1);
        assert!(e
            .project()
            .channels
            .iter()
            .all(|c| !(c.target == "arm" && c.prop == Prop::BoneScale)));
        assert_eq!(pl.changed, 3);
        assert!(pl.selection.is_empty());
        e.undo();
        assert_eq!(e.doc(), &before);
        // ease
        let bz = Ease::CubicBezier {
            x1: 0.2,
            y1: 0.0,
            x2: 0.8,
            y2: 1.0,
        };
        run(
            &mut e,
            TlOp::SetEase {
                keys: vec![kr(0, 0.0), kr(0, 12.0)],
                ease: bz,
            },
        );
        let t = keys_of(&e.project().channels[0]).unwrap();
        assert_eq!(
            (t.keys[0].ease, t.keys[1].ease, t.keys[2].ease),
            (bz, bz, Ease::Linear)
        );
        let bad = Ease::CubicBezier {
            x1: 2.0,
            y1: 0.0,
            x2: 0.8,
            y2: 1.0,
        };
        assert!(set_ease(e.project(), &[kr(0, 0.0)], bad).is_err());
        // value
        run(
            &mut e,
            TlOp::SetValue {
                keys: vec![kr(0, 0.0), kr(0, 24.0)],
                value: 7.0,
            },
        );
        assert_eq!(
            keys_of_ch(e.project(), 0),
            vec![(0.0, 7.0), (12.0, 30.0), (24.0, 7.0)]
        );
        assert!(set_value(e.project(), &[kr(0, 0.0)], f32::NAN).is_err());
        // add key: value defaults to the evaluated curve
        let pl = run(
            &mut e,
            TlOp::AddKey {
                target: "root".into(),
                prop: Prop::BoneOffsetX,
                frame: 24.0,
                value: None,
                ease: None,
                opts: EditOpts::default(),
            },
        );
        assert_eq!(keys_of_ch(e.project(), 2)[1], (24.0, 5.0));
        assert_eq!(pl.selection, vec![kr(2, 24.0)]);
        // and a brand-new channel needs an explicit value
        assert!(add_key(
            e.project(),
            "arm_r",
            Prop::BoneScale,
            3.0,
            None,
            Ease::Linear,
            &EditOpts::default()
        )
        .is_err());
        let pl = add_key(
            e.project(),
            "arm_r",
            Prop::BoneScale,
            3.2,
            Some(2.0),
            Ease::Linear,
            &EditOpts::default(),
        )
        .unwrap();
        assert_eq!(pl.selection, vec![kr(e.project().channels.len(), 3.0)]);
        assert!(add_key(
            e.project(),
            "arm",
            Prop::BoneAngle,
            99.0,
            Some(1.0),
            Ease::Linear,
            &EditOpts::default()
        )
        .is_err());
    }

    #[test]
    fn locked_channels_are_refused_atomically() {
        let mut p = proj();
        chanmeta::set_meta(
            &mut p,
            "arm",
            Some(Prop::BoneScale),
            &chanmeta::Change {
                lock: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let mut e = Editor::new(p).unwrap();
        let before = e.doc().clone();
        // channel 0 is free, channel 1 is locked: nothing may change
        let sel = [kr(0, 12.0), kr(1, 0.0)];
        for op in [
            TlOp::MoveKeys {
                keys: sel.to_vec(),
                dt: 1.0,
                dv: 0.0,
                opts: EditOpts::default(),
            },
            TlOp::ScaleKeys {
                keys: sel.to_vec(),
                pivot: 0.0,
                factor: 2.0,
                opts: EditOpts::default(),
            },
            TlOp::Delete { keys: sel.to_vec() },
            TlOp::SetEase {
                keys: sel.to_vec(),
                ease: Ease::Step,
            },
            TlOp::SetValue {
                keys: sel.to_vec(),
                value: 1.0,
            },
            TlOp::Duplicate {
                keys: sel.to_vec(),
                offset: 5.0,
                opts: EditOpts::default(),
            },
            TlOp::Smooth {
                keys: sel.to_vec(),
                kind: TangentKind::Monotone,
            },
            TlOp::Offset {
                channels: vec![1],
                dt: 1.0,
                opts: EditOpts::default(),
            },
            TlOp::Extrapolate {
                channels: vec![1],
                extrapolate: Extrapolate::Loop,
            },
            TlOp::MakeLoop {
                channels: vec![1],
                end: None,
                extrapolate: None,
                opts: EditOpts::default(),
            },
            TlOp::AddKey {
                target: "arm".into(),
                prop: Prop::BoneScale,
                frame: 3.0,
                value: Some(1.0),
                ease: None,
                opts: EditOpts::default(),
            },
            TlOp::Handle {
                ch: 1,
                frame: 0.0,
                side: Side::Out,
                t: 1.0,
                v: 1.0,
                mode: HandleMode::Free,
            },
            TlOp::Ripple {
                at: 0.0,
                count: 2.0,
                mode: RetimeMode::Insert,
                scope: Some(vec![1]),
                adjust_frames: false,
                opts: EditOpts::default(),
            },
        ] {
            let err = apply(&mut e, &op).unwrap_err().join(" ");
            assert!(err.contains("locked"), "{op:?}: {err}");
            assert_eq!(e.doc(), &before);
        }
        // pasting onto a locked destination is refused too
        let clip = copy_keys(e.project(), &[kr(0, 12.0)]).unwrap();
        let rm = Remap {
            from_target: "arm".into(),
            from_prop: Prop::BoneAngle,
            to_target: "arm".into(),
            to_prop: Prop::BoneScale,
        };
        assert!(
            paste_clip(e.project(), &clip, 0.0, &[rm], &EditOpts::default())
                .unwrap_err()
                .contains("locked")
        );
        assert!(!e.can_undo(), "refused edits leave no history");
    }

    #[test]
    fn ripple_insert_delete_scope_and_frames() {
        let mut p = proj();
        chanmeta::set_meta(
            &mut p,
            "root",
            Some(Prop::BoneOffsetX),
            &chanmeta::Change {
                lock: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let mut e = Editor::new(p).unwrap();
        let before = e.doc().clone();
        // insert 6 frames at 12 (all unlocked channels; locked root.x skipped, reported)
        let pl = run(
            &mut e,
            TlOp::Ripple {
                at: 12.0,
                count: 6.0,
                mode: RetimeMode::Insert,
                scope: None,
                adjust_frames: true,
                opts: EditOpts::default(),
            },
        );
        assert_eq!(
            keys_of_ch(e.project(), 0),
            vec![(0.0, 0.0), (18.0, 30.0), (30.0, 60.0)]
        );
        assert_eq!(keys_of_ch(e.project(), 1), vec![(0.0, 1.0), (30.0, 2.0)]);
        assert_eq!(
            keys_of_ch(e.project(), 2),
            vec![(0.0, 0.0), (48.0, 10.0)],
            "locked channel untouched"
        );
        assert!(pl.notes.iter().any(|n| n.contains("skipped locked")));
        assert_eq!(e.project().frames, 54);
        assert_eq!(keys_of_ch(e.project(), 4)[1].0, 54.0);
        e.undo();
        assert_eq!(e.doc(), &before);
        // without adjust_frames the camera key at 48 would be pushed past the end
        let o = EditOpts::default();
        let err = ripple(e.project(), 12.0, 6.0, RetimeMode::Insert, None, false, &o);
        assert!(err.unwrap_err().contains("outside"));
        // free bounds allow it
        let free = EditOpts {
            bounds: Bounds::Free,
            ..Default::default()
        };
        let pl = ripple(
            e.project(),
            12.0,
            6.0,
            RetimeMode::Insert,
            None,
            false,
            &free,
        )
        .unwrap();
        e.apply(pl.op.unwrap()).unwrap();
        assert_eq!(e.project().frames, 48);
        assert_eq!(keys_of_ch(e.project(), 4)[1].0, 54.0);
        e.undo();
        // delete frames 12..24: key at 12 removed, key at 24 moves to 12, 48 -> 36
        run(
            &mut e,
            TlOp::Ripple {
                at: 12.0,
                count: 12.0,
                mode: RetimeMode::Delete,
                scope: None,
                adjust_frames: true,
                opts: EditOpts::default(),
            },
        );
        assert_eq!(keys_of_ch(e.project(), 0), vec![(0.0, 0.0), (12.0, 60.0)]);
        assert_eq!(e.project().frames, 36);
        assert_eq!(keys_of_ch(e.project(), 4)[1].0, 36.0);
        e.undo();
        // errors
        assert!(ripple(e.project(), 0.0, 0.0, RetimeMode::Insert, None, false, &o).is_err());
        assert!(
            ripple(e.project(), 0.0, 48.0, RetimeMode::Delete, None, true, &o)
                .unwrap_err()
                .contains("no frames")
        );
        assert!(ripple(
            e.project(),
            f32::NAN,
            2.0,
            RetimeMode::Insert,
            None,
            false,
            &o
        )
        .is_err());
        // explicit scope: only listed channels
        run(
            &mut e,
            TlOp::Ripple {
                at: 0.0,
                count: 2.0,
                mode: RetimeMode::Insert,
                scope: Some(vec![1]),
                adjust_frames: false,
                opts: EditOpts::default(),
            },
        );
        assert_eq!(keys_of_ch(e.project(), 1), vec![(2.0, 1.0), (26.0, 2.0)]);
        assert_eq!(keys_of_ch(e.project(), 0)[0].0, 0.0);
    }

    #[test]
    fn offset_extrapolate_and_loop_helpers() {
        let mut e = Editor::new(proj()).unwrap();
        run(
            &mut e,
            TlOp::Offset {
                channels: vec![0],
                dt: 4.0,
                opts: EditOpts::default(),
            },
        );
        assert_eq!(
            keys_of_ch(e.project(), 0)
                .iter()
                .map(|k| k.0)
                .collect::<Vec<_>>(),
            vec![4.0, 16.0, 28.0]
        );
        assert!(offset_channels(e.project(), &[0], 30.0, &EditOpts::default()).is_err());
        assert!(offset_channels(e.project(), &[], 1.0, &EditOpts::default()).is_err());
        e.undo();
        run(
            &mut e,
            TlOp::Extrapolate {
                channels: vec![0, 1],
                extrapolate: Extrapolate::PingPong,
            },
        );
        assert_eq!(
            keys_of(&e.project().channels[1]).unwrap().extrapolate,
            Extrapolate::PingPong
        );
        // make_loop closes the cycle with the first key's value
        let pl = run(
            &mut e,
            TlOp::MakeLoop {
                channels: vec![0],
                end: Some(48.0),
                extrapolate: None,
                opts: EditOpts::default(),
            },
        );
        let t = keys_of(&e.project().channels[0]).unwrap();
        assert_eq!(t.keys.last().map(|k| (k.frame, k.value)), Some((48.0, 0.0)));
        assert_eq!(t.extrapolate, Extrapolate::Loop);
        assert_eq!(pl.selection, vec![kr(0, 48.0)]);
        assert!(
            (t.eval(50.0).unwrap() - t.eval(2.0).unwrap()).abs() < 1e-4,
            "looped"
        );
        assert!(make_loop(
            e.project(),
            &[0],
            Some(-1.0),
            Extrapolate::Loop,
            &EditOpts::default()
        )
        .is_err());
        assert!(
            make_loop(
                e.project(),
                &[0],
                Some(10.0),
                Extrapolate::Loop,
                &EditOpts::default()
            )
            .is_err(),
            "before the last key"
        );
        let one = Project {
            channels: vec![Channel::keys("arm", Prop::BoneAngle, lin(&[(0.0, 1.0)]))],
            ..proj()
        };
        assert!(
            make_loop(&one, &[0], None, Extrapolate::Loop, &EditOpts::default())
                .unwrap_err()
                .contains("two keys")
        );
    }

    #[test]
    fn handle_and_smooth_ops_go_through_the_editor() {
        let mut e = Editor::new(proj()).unwrap();
        let before = e.doc().clone();
        run(
            &mut e,
            TlOp::Handle {
                ch: 0,
                frame: 0.0,
                side: Side::Out,
                t: 6.0,
                v: 0.0,
                mode: HandleMode::Free,
            },
        );
        match keys_of(&e.project().channels[0]).unwrap().keys[0].ease {
            Ease::CubicBezier { x1, y1, .. } => assert!((x1 - 0.5).abs() < 1e-6 && y1.abs() < 1e-6),
            x => panic!("{x:?}"),
        }
        e.undo();
        assert_eq!(e.doc(), &before);
        assert!(move_handle(
            e.project(),
            0,
            5.0,
            Side::Out,
            HandlePos { t: 1.0, v: 1.0 },
            HandleMode::Free
        )
        .is_err());
        assert!(move_handle(
            e.project(),
            0,
            24.0,
            Side::Out,
            HandlePos { t: 1.0, v: 1.0 },
            HandleMode::Free
        )
        .is_err());
        run(
            &mut e,
            TlOp::Smooth {
                keys: vec![kr(0, 0.0), kr(0, 12.0)],
                kind: TangentKind::Monotone,
            },
        );
        let t = keys_of(&e.project().channels[0]).unwrap();
        assert!(
            matches!(t.keys[0].ease, Ease::CubicBezier { .. })
                && matches!(t.keys[1].ease, Ease::CubicBezier { .. })
        );
        assert!(matches!(t.keys[2].ease, Ease::Linear));
        // collinear data stays a straight line under monotone smoothing
        assert!((t.eval(6.0).unwrap() - 15.0).abs() < 0.05);
    }

    #[test]
    fn flags_op_sets_lock_and_undoes() {
        let mut e = Editor::new(proj()).unwrap();
        let before = e.doc().clone();
        let pl = run(
            &mut e,
            TlOp::Flags {
                target: "arm".into(),
                prop: Some(Prop::BoneAngle),
                lock: Some(true),
                shy: None,
                solo: None,
                color: Some("#00ff00".into()),
            },
        );
        assert_eq!(pl.changed, 1);
        assert!(chanmeta::is_locked(e.project(), "arm", Prop::BoneAngle));
        assert!(build(e.project()).tracks[0].locked);
        assert!(apply(
            &mut e,
            &TlOp::Delete {
                keys: vec![kr(0, 0.0)]
            }
        )
        .is_err());
        e.undo();
        assert_eq!(e.doc(), &before);
        assert!(set_flags(
            e.project(),
            "ghost",
            None,
            &chanmeta::Change {
                lock: Some(true),
                ..Default::default()
            }
        )
        .is_err());
        assert!(set_flags(
            e.project(),
            "arm",
            None,
            &chanmeta::Change {
                color: Some("not a colour".into()),
                ..Default::default()
            }
        )
        .is_err());
    }

    #[test]
    fn ops_parse_from_json_and_copy_is_not_an_edit() {
        let j = r#"{"op":"move_keys","keys":[{"ch":0,"frame":12}],"dt":2}"#;
        let op: TlOp = serde_json::from_str(j).unwrap();
        assert_eq!(
            op,
            TlOp::MoveKeys {
                keys: vec![kr(0, 12.0)],
                dt: 2.0,
                dv: 0.0,
                opts: EditOpts::default()
            }
        );
        let j = r#"{"op":"move_keys","keys":[{"ch":0,"frame":12}],"dt":2,"opts":{"collision":"refuse"}}"#;
        match serde_json::from_str::<TlOp>(j).unwrap() {
            TlOp::MoveKeys { opts, .. } => {
                assert!(opts.snap && opts.collision == Collision::Refuse)
            }
            o => panic!("{o:?}"),
        }
        assert!(serde_json::from_str::<TlOp>(r#"{"op":"explode"}"#).is_err());
        let mut e = Editor::new(proj()).unwrap();
        let pl = run(
            &mut e,
            TlOp::Copy {
                keys: vec![kr(0, 0.0)],
            },
        );
        assert!(pl.op.is_none() && pl.clip.is_some());
        assert!(!e.can_undo());
        // clips round-trip through JSON
        let c = pl.clip.unwrap();
        let back: Clip = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(c, back);
        assert_eq!(Collision::parse("merge"), Some(Collision::Merge));
    }

    #[test]
    fn operations_are_deterministic() {
        let p = proj();
        let ops = [
            TlOp::MoveKeys {
                keys: vec![kr(0, 12.0), kr(2, 0.0), kr(1, 24.0)],
                dt: 5.0,
                dv: 1.0,
                opts: EditOpts::default(),
            },
            TlOp::Delete {
                keys: vec![kr(4, 0.0), kr(0, 24.0)],
            },
            TlOp::Ripple {
                at: 3.0,
                count: 2.0,
                mode: RetimeMode::Insert,
                scope: None,
                adjust_frames: true,
                opts: EditOpts::default(),
            },
        ];
        for op in &ops {
            let a = serde_json::to_string(&plan(&p, op).unwrap()).unwrap();
            let b = serde_json::to_string(&plan(&p, op).unwrap()).unwrap();
            assert_eq!(a, b);
        }
        // selection order in the request does not matter
        let a = plan(
            &p,
            &TlOp::Delete {
                keys: vec![kr(0, 12.0), kr(0, 0.0)],
            },
        )
        .unwrap();
        let b = plan(
            &p,
            &TlOp::Delete {
                keys: vec![kr(0, 0.0), kr(0, 12.0), kr(0, 0.0)],
            },
        )
        .unwrap();
        assert_eq!(
            serde_json::to_string(&a.op).unwrap(),
            serde_json::to_string(&b.op).unwrap()
        );
    }

    #[test]
    fn graph_view_samples_keys_and_handles() {
        let p = proj();
        let v = graph_view(&p, &[0, 3, 2], 0.0, 48.0, 49).unwrap();
        assert_eq!(v.channels.len(), 2);
        assert_eq!(v.skipped.len(), 1);
        assert!(v.range[0] < 0.0 && v.range[1] > 60.0, "{:?}", v.range);
        assert_eq!(v.channels[0].keys.len(), 3);
        assert!(v.channels[0].keys[0].inn.is_none() && v.channels[0].keys[0].out.is_some());
        assert!(v.channels[0].samples.len() >= 49);
        assert!(graph_view(&p, &[0], 5.0, 5.0, 10).is_err());
        assert!(graph_view(&p, &[77], 0.0, 1.0, 10).is_err());
        let j = serde_json::to_value(&v).unwrap();
        assert!(j["channels"][0]["keys"][1]["in"].is_object());
        assert_eq!(
            serde_json::to_string(&v).unwrap(),
            serde_json::to_string(&graph_view(&p, &[0, 3, 2], 0.0, 48.0, 49).unwrap()).unwrap()
        );
        assert_eq!(graph_view(&p, &[], 0.0, 1.0, 4).unwrap().range, [-1.0, 1.0]);
    }

    #[test]
    fn demo_project_builds_and_every_edit_keeps_it_valid() {
        let p = crate::demo::project();
        let m = build(&p);
        assert_eq!(m.tracks.len(), p.channels.len());
        assert!(m.tracks.iter().any(|t| t.source == "expr") && m.tracks.iter().any(|t| t.editable));
        let all = select_all(&m, false);
        assert!(!all.is_empty());
        let mut e = Editor::new(p).unwrap();
        let before = e.doc().clone();
        run(
            &mut e,
            TlOp::MoveKeys {
                keys: all.clone(),
                dt: 0.0,
                dv: 1.0,
                opts: EditOpts::default(),
            },
        );
        e.undo();
        run(
            &mut e,
            TlOp::Smooth {
                keys: all.clone(),
                kind: TangentKind::Monotone,
            },
        );
        e.undo();
        run(
            &mut e,
            TlOp::Ripple {
                at: 10.0,
                count: 4.0,
                mode: RetimeMode::Delete,
                scope: None,
                adjust_frames: true,
                opts: EditOpts::default(),
            },
        );
        e.undo();
        assert_eq!(e.doc(), &before);
    }

    #[test]
    fn channels_of_a_locked_layer_are_locked_in_the_model_and_refuse_edits() {
        let mut p = Project::new("t", 100.0, 100.0, 24.0, 48);
        p.layers = vec![
            crate::project::Layer::group("g", vec![crate::project::Layer::vector("v", vec![])]),
            crate::project::Layer::vector("w", vec![]),
        ];
        p.channels = vec![
            Channel::keys("g/v", Prop::LayerX, lin(&[(0.0, 0.0), (12.0, 30.0)])),
            Channel::keys("w", Prop::LayerX, lin(&[(0.0, 0.0), (12.0, 30.0)])),
            Channel::keys("", Prop::CameraZoom, lin(&[(0.0, 1.0)])),
        ];
        p.find_layer_mut("g").unwrap().locked = true;
        let m = build(&p);
        assert_eq!(
            m.tracks.iter().map(|t| t.locked).collect::<Vec<_>>(),
            vec![true, false, false],
            "group lock reaches the child layer only"
        );
        assert!(
            chanmeta::layer_locked(&p, "g/v::lat::0,0", Prop::LatticeDx)
                && !chanmeta::layer_locked(&p, "w::lat::0,0", Prop::LatticeDx)
                && !chanmeta::layer_locked(&p, "g", Prop::BoneAngle)
        );
        assert!(chanmeta::assert_unlocked(&p, "g/v", Prop::LayerX)
            .unwrap_err()
            .contains("layer 'g/v' is locked"));
        assert!(chanmeta::assert_unlocked(&p, "w", Prop::LayerX).is_ok());
        let mut e = Editor::new(p).unwrap();
        let before = e.doc().clone();
        let err = apply(
            &mut e,
            &TlOp::MoveKeys {
                keys: vec![kr(0, 12.0)],
                dt: 2.0,
                dv: 0.0,
                opts: Default::default(),
            },
        )
        .unwrap_err();
        assert!(err.join(" ").contains("locked"), "{err:?}");
        assert_eq!(e.doc(), &before);
        run(
            &mut e,
            TlOp::MoveKeys {
                keys: vec![kr(1, 12.0)],
                dt: 2.0,
                dv: 0.0,
                opts: Default::default(),
            },
        );
        // selecting by box skips the locked tracks unless asked
        let m = build(e.project());
        assert!(select_all(&m, false).iter().all(|k| k.ch != 0));
        assert!(select_all(&m, true).iter().any(|k| k.ch == 0));
        // unlock through a plain Op and the same edit works
        e.apply(Op::Set {
            path: "/layers/0/locked".into(),
            value: serde_json::json!(false),
        })
        .unwrap();
        run(
            &mut e,
            TlOp::MoveKeys {
                keys: vec![kr(0, 12.0)],
                dt: 2.0,
                dv: 0.0,
                opts: Default::default(),
            },
        );
    }
}
