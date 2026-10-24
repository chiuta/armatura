// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
//! Runtime state machines for interactive playback.
//!
//! (Not to be confused with [`crate::statemachine`], which turns an input *event log* into baked
//! channels offline. This module is the live counterpart: a host feeds inputs and a time step and
//! reads back the evaluated frame.)
//!
//! Own design, textbook ingredients only (finite-state machine, 1-D blend tree, one easing
//! crossfade, normalised-weight averaging that equals the clip stack's replace-blend maths):
//!
//! * **Inputs** are named `bool`, `number` or `trigger` values. A trigger is a one-shot: it stays
//!   set until a transition that tests it fires (it is then consumed) or, unless `hold` is set,
//!   until the end of the `step` call after it was fired.
//! * **States** play a source: a clip of `Project.clips`, a frame `segment` of the project's own
//!   keyed channels, a marker `region`, or a 1-D `blend` of two or more of those driven by a number
//!   input. Playback is `loop`, `once` or `ping_pong` at a `speed`; `blend_in` is the default
//!   crossfade length into the state.
//! * **Transitions** go from a state (or `"*"` = any state) to a state when all conditions hold
//!   (`input op value`, AND), optionally only once the state has played to `exit_time` (0..1 of its
//!   length). The first matching transition in declaration order wins. They crossfade with
//!   `duration` seconds and an easing curve; an interrupted crossfade is frozen into a snapshot
//!   and fades on from there.
//! * **Layers** run in parallel inside one machine; each owns a disjoint set of
//!   `(target, property)` pairs (checked by validation) selected by an optional glob `mask`.
//!
//! Blending: every state yields a *pose* (target, property -> value, coverage weight). Poses are
//! mixed with `v = sum(w_i * c_i * v_i) / sum(w_i * c_i)` and coverage `sum(w_i * c_i)`, which is
//! what stacking replace-mode clips gives. The result is written as constant replace-mode channels
//! appended to the project; the engine evaluates them as usual ([`MachineRuntime::engine`]).
//!
//! Determinism: the runtime is a pure function of the call sequence (`set_*`, `fire`, `step(dt)`).
//! `step` subdivides large `dt` into pieces of at most 1/60 s, so results do not depend on how a
//! host chunks time beyond that resolution.

use crate::anim::{Ease, Track};
use crate::clipstack::glob;
use crate::engine::{Engine, Frame};
use crate::project::{Channel, Mode, Project, Prop, Source};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------------------------
// Data model (serialised in Project.machines)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    Bool,
    Number,
    Trigger,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Input {
    pub name: String,
    pub kind: InputKind,
    /// Initial value (bool: 0/1).
    #[serde(default)]
    pub default: f64,
    /// Triggers only: stay set until consumed instead of lasting one `step`.
    #[serde(default)]
    pub hold: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cmp {
    #[default]
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Cmp {
    fn test(self, a: f64, b: f64) -> bool {
        match self {
            Cmp::Eq => a == b,
            Cmp::Ne => a != b,
            Cmp::Lt => a < b,
            Cmp::Le => a <= b,
            Cmp::Gt => a > b,
            Cmp::Ge => a >= b,
        }
    }
}

fn one64() -> f64 {
    1.0
}
fn one32() -> f32 {
    1.0
}

/// `input op value`. A bool is 0/1 and a trigger is 1 while set, so `{input: "jump"}` alone
/// (op `eq`, value 1) means "the trigger is set" / "the flag is true".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub input: String,
    #[serde(default)]
    pub op: Cmp,
    #[serde(default = "one64")]
    pub value: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayMode {
    #[default]
    Loop,
    Once,
    PingPong,
}

/// What a state plays.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StateSource {
    /// A clip of `Project.clips` (its embedded take, masks, rename and weights).
    Clip { clip: String },
    /// Frames `start..end` of the project's keyed replace-mode channels.
    Segment { start: f32, end: f32 },
    /// A region marker (`Project.markers` entry with an `end`).
    Region { marker: String },
    /// 1-D blend tree: children (leaf sources) mixed by a number input.
    Blend {
        input: String,
        children: Vec<BlendChild>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlendChild {
    /// Input value at which this child has full weight.
    pub threshold: f32,
    pub source: StateSource,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub name: String,
    pub source: StateSource,
    #[serde(default)]
    pub mode: PlayMode,
    #[serde(default = "one32")]
    pub speed: f32,
    /// Default crossfade (seconds) into this state when a transition gives no `duration`.
    #[serde(default)]
    pub blend_in: f32,
}

/// Unknown keys are an error here (not just a warning): a typo such as `conditions` for `when` would otherwise
/// silently turn a guarded transition into one that always fires.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    /// State name, or `"*"` for any state (never matches a transition into the current state).
    pub from: String,
    pub to: String,
    #[serde(default, rename = "when", skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<Condition>,
    /// Only once the state has played this fraction (0..1) of its length.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_time: Option<f32>,
    /// Crossfade seconds; defaults to the target state's `blend_in`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f32>,
    #[serde(default)]
    pub ease: Ease,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    /// Glob patterns of targets this layer drives (empty = all).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mask: Vec<String>,
    /// Name of the state entered at start.
    pub entry: String,
    pub states: Vec<State>,
    #[serde(default)]
    pub transitions: Vec<Transition>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Machine {
    pub name: String,
    #[serde(default)]
    pub inputs: Vec<Input>,
    pub layers: Vec<Layer>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Entered,
    Exited,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MachineEvent {
    /// Runtime time in seconds.
    pub time: f64,
    pub layer: String,
    pub kind: EventKind,
    pub state: String,
}

pub const MAX_STEP: f64 = 10.0;
const SUBSTEP: f64 = 1.0 / 60.0;
const MAX_HOPS: usize = 8;

// ---------------------------------------------------------------------------------------------
// Resolving sources
// ---------------------------------------------------------------------------------------------

type TrackSet = Vec<(String, Prop, Track, f32)>;

struct Leaf {
    /// Length in frames.
    len: f32,
    /// Added to the local frame when sampling (segments are absolute).
    offset: f32,
    tracks: TrackSet,
}

fn mask_ok(mask: &[String], target: &str) -> bool {
    mask.is_empty() || mask.iter().any(|g| glob(g, target))
}

fn resolve_leaf(p: &Project, src: &StateSource, mask: &[String]) -> Result<Leaf, String> {
    match src {
        StateSource::Clip { clip } => {
            let c = p
                .clips
                .iter()
                .find(|c| &c.name == clip)
                .ok_or_else(|| format!("unknown clip '{clip}'"))?;
            if !(c.take.length.is_finite() && c.take.length > 0.0) {
                return Err(format!("clip '{clip}' has no positive length"));
            }
            let mut tracks = Vec::new();
            for tt in &c.take.tracks {
                let target = c
                    .rename
                    .iter()
                    .find(|(f, _)| f == &tt.target)
                    .map_or(tt.target.as_str(), |(_, t)| t.as_str())
                    .to_string();
                if tt.keys.is_empty() || !c.passes_mask(&target) || !mask_ok(mask, &target) {
                    continue;
                }
                let mut w = c.weight;
                if let Some(r) = c
                    .track_weights
                    .iter()
                    .find(|r| glob(&r.pattern, &target) && r.prop.is_none_or(|pp| pp == tt.prop))
                {
                    w *= r.weight;
                }
                let t = Track::with_keys(
                    &tt.keys
                        .iter()
                        .map(|k| (k.frame, k.value, k.ease))
                        .collect::<Vec<_>>(),
                );
                tracks.push((target, tt.prop, t, w));
            }
            Ok(Leaf {
                len: c.take.length,
                offset: 0.0,
                tracks,
            })
        }
        StateSource::Segment { start, end } => {
            if !(start.is_finite() && end.is_finite() && *start >= 0.0 && end > start) {
                return Err(format!("segment {start}..{end} needs 0 <= start < end"));
            }
            if *end > p.frames as f32 {
                return Err(format!(
                    "segment {start}..{end} lies beyond the {} project frames",
                    p.frames
                ));
            }
            Ok(Leaf {
                len: end - start,
                offset: *start,
                tracks: segment_tracks(p, mask),
            })
        }
        StateSource::Region { marker } => {
            let m = p
                .markers
                .iter()
                .find(|m| &m.name == marker)
                .ok_or_else(|| format!("unknown marker '{marker}'"))?;
            let end = m
                .end
                .ok_or_else(|| format!("marker '{marker}' is not a region (no end)"))?;
            resolve_leaf(
                p,
                &StateSource::Segment {
                    start: m.frame,
                    end,
                },
                mask,
            )
        }
        StateSource::Blend { .. } => Err("a blend tree cannot be nested".into()),
    }
}

/// Keyed replace-mode channels of the project (later channels override earlier ones).
fn segment_tracks(p: &Project, mask: &[String]) -> TrackSet {
    let mut out: TrackSet = Vec::new();
    for c in &p.channels {
        if let (Source::Keys { track }, Mode::Replace) = (&c.source, c.mode) {
            if track.keys.is_empty() || !mask_ok(mask, &c.target) {
                continue;
            }
            let item = (
                c.target.clone(),
                c.prop,
                track.clone(),
                c.weight.clamp(0.0, 1.0),
            );
            match out.iter().position(|o| o.0 == c.target && o.1 == c.prop) {
                Some(i) => out[i] = item,
                None => out.push(item),
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Poses
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pose {
    /// target, prop, value, coverage weight (0..1)
    items: Vec<(String, Prop, f32, f32)>,
    ix: HashMap<(String, Prop), usize>,
}

impl Pose {
    fn add(&mut self, target: &str, prop: Prop, v: f32, w: f32) {
        match self.ix.get(&(target.to_string(), prop)) {
            Some(&i) => {
                self.items[i].2 = v;
                self.items[i].3 = w;
            }
            None => {
                self.ix.insert((target.to_string(), prop), self.items.len());
                self.items.push((target.to_string(), prop, v, w));
            }
        }
    }

    /// Value and coverage of one entry.
    pub fn get(&self, target: &str, prop: Prop) -> Option<(f32, f32)> {
        self.ix
            .get(&(target.to_string(), prop))
            .map(|&i| (self.items[i].2, self.items[i].3))
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Weighted mix; weights are expected to sum to 1.
    fn mix(parts: &[(&Pose, f32)]) -> Pose {
        let mut out = Pose::default();
        let mut keys: Vec<(String, Prop)> = Vec::new();
        let mut seen: HashSet<(String, Prop)> = HashSet::new();
        for (p, w) in parts {
            if *w <= 0.0 {
                continue;
            }
            for (t, pr, ..) in &p.items {
                if seen.insert((t.clone(), *pr)) {
                    keys.push((t.clone(), *pr));
                }
            }
        }
        for (t, pr) in keys {
            let (mut num, mut cov) = (0.0f32, 0.0f32);
            for (p, w) in parts {
                if *w <= 0.0 {
                    continue;
                }
                if let Some((v, c)) = p.get(&t, pr) {
                    num += w * c * v;
                    cov += w * c;
                }
            }
            if cov > 1e-9 {
                out.add(&t, pr, num / cov, cov.min(1.0));
            }
        }
        out
    }

    fn channels(&self) -> Vec<Channel> {
        self.items
            .iter()
            .map(|(t, p, v, w)| {
                let mut c = Channel::keys(t, *p, Track::with_keys(&[(0.0, *v, Ease::Linear)]));
                c.weight = *w;
                c
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------------------------

/// Weights of the sorted `thresholds` for input value `v`: the two neighbouring children share
/// the weight linearly; outside the range the end child has weight 1.
pub fn blend_weights(thresholds: &[f32], v: f32) -> Vec<f32> {
    let n = thresholds.len();
    let mut w = vec![0.0; n];
    if n == 0 {
        return w;
    }
    if v <= thresholds[0] {
        w[0] = 1.0;
    } else if v >= thresholds[n - 1] {
        w[n - 1] = 1.0;
    } else {
        for i in 0..n - 1 {
            if v >= thresholds[i] && v <= thresholds[i + 1] {
                let t = (v - thresholds[i]) / (thresholds[i + 1] - thresholds[i]);
                w[i] = 1.0 - t;
                w[i + 1] = t;
                break;
            }
        }
    }
    w
}

fn source_leaves(p: &Project, src: &StateSource, mask: &[String]) -> Result<Vec<Leaf>, String> {
    match src {
        StateSource::Blend { children, .. } => children
            .iter()
            .map(|c| resolve_leaf(p, &c.source, mask))
            .collect(),
        s => Ok(vec![resolve_leaf(p, s, mask)?]),
    }
}

/// Structural validation of every machine in `p.machines` (names, references, reachability,
/// transition cycles, layer disjointness). Property targets are checked by [`trial_errors`].
pub fn validate(p: &Project) -> Vec<String> {
    let mut errs = Vec::new();
    let mut names = HashSet::new();
    for m in &p.machines {
        if m.name.trim().is_empty() {
            errs.push("machine needs a name".into());
        } else if !names.insert(m.name.clone()) {
            errs.push(format!("duplicate machine name '{}'", m.name));
        }
        errs.extend(validate_machine(p, m));
    }
    errs.extend(crate::binding::validate(p));
    errs
}

pub fn validate_machine(p: &Project, m: &Machine) -> Vec<String> {
    let mut errs = Vec::new();
    let mn = &m.name;
    let mut inputs: HashMap<&str, InputKind> = HashMap::new();
    for i in &m.inputs {
        if i.name.trim().is_empty() {
            errs.push(format!("machine '{mn}': input needs a name"));
        } else if inputs.insert(&i.name, i.kind).is_some() {
            errs.push(format!("machine '{mn}': duplicate input '{}'", i.name));
        }
        if !i.default.is_finite() {
            errs.push(format!(
                "machine '{mn}': input '{}' default must be finite",
                i.name
            ));
        }
    }
    if m.layers.is_empty() {
        errs.push(format!("machine '{mn}': needs at least one layer"));
    }
    let mut lnames = HashSet::new();
    let mut layer_targets: Vec<(String, HashSet<(String, Prop)>)> = Vec::new();
    for l in &m.layers {
        let ln = format!("machine '{mn}' layer '{}'", l.name);
        if l.name.trim().is_empty() {
            errs.push(format!("machine '{mn}': layer needs a name"));
        } else if !lnames.insert(l.name.clone()) {
            errs.push(format!("{ln}: duplicate layer name"));
        }
        if l.mask.iter().any(|g| g.is_empty()) {
            errs.push(format!("{ln}: empty mask pattern"));
        }
        if l.states.is_empty() {
            errs.push(format!("{ln}: needs at least one state"));
            continue;
        }
        let mut states: HashMap<&str, usize> = HashMap::new();
        let mut targets: HashSet<(String, Prop)> = HashSet::new();
        for (si, s) in l.states.iter().enumerate() {
            let sn = format!("{ln} state '{}'", s.name);
            if s.name.trim().is_empty() || s.name == "*" {
                errs.push(format!("{ln}: state needs a name other than '*'"));
            } else if states.insert(&s.name, si).is_some() {
                errs.push(format!("{ln}: duplicate state name '{}'", s.name));
            }
            if !(s.speed.is_finite() && s.speed > 0.0) {
                errs.push(format!("{sn}: speed must be a finite number > 0"));
            }
            if !(s.blend_in.is_finite() && s.blend_in >= 0.0) {
                errs.push(format!("{sn}: blend_in must be a finite number >= 0"));
            }
            if let StateSource::Blend { input, children } = &s.source {
                match inputs.get(input.as_str()) {
                    None => errs.push(format!("{sn}: blend input '{input}' does not exist")),
                    Some(InputKind::Number) => {}
                    Some(_) => errs.push(format!(
                        "{sn}: blend input '{input}' must be a number input"
                    )),
                }
                if children.len() < 2 {
                    errs.push(format!("{sn}: a blend tree needs at least two children"));
                }
                if !children.iter().all(|c| c.threshold.is_finite())
                    || !children.windows(2).all(|w| w[0].threshold < w[1].threshold)
                {
                    errs.push(format!(
                        "{sn}: blend thresholds must be finite and strictly increasing"
                    ));
                }
            }
            match source_leaves(p, &s.source, &l.mask) {
                Ok(leaves) => {
                    for lf in leaves {
                        for (t, pr, ..) in lf.tracks {
                            targets.insert((t, pr));
                        }
                    }
                }
                Err(e) => errs.push(format!("{sn}: {e}")),
            }
        }
        if !states.contains_key(l.entry.as_str()) {
            errs.push(format!("{ln}: entry state '{}' does not exist", l.entry));
        }
        // transitions
        let mut edges_ok = true;
        for (ti, t) in l.transitions.iter().enumerate() {
            let tn = format!("{ln} transition {} ({} -> {})", ti + 1, t.from, t.to);
            if t.from != "*" && !states.contains_key(t.from.as_str()) {
                errs.push(format!("{tn}: unknown source state '{}'", t.from));
                edges_ok = false;
            }
            if !states.contains_key(t.to.as_str()) {
                errs.push(format!("{tn}: transition to missing state '{}'", t.to));
                edges_ok = false;
            }
            for c in &t.conditions {
                if !inputs.contains_key(c.input.as_str()) {
                    errs.push(format!("{tn}: condition on unknown input '{}'", c.input));
                }
                if !c.value.is_finite() {
                    errs.push(format!("{tn}: condition value must be finite"));
                }
            }
            if let Some(x) = t.exit_time {
                if !(x.is_finite() && (0.0..=1.0).contains(&x)) {
                    errs.push(format!("{tn}: exit_time must be within 0..1"));
                }
            }
            if let Some(d) = t.duration {
                if !(d.is_finite() && d >= 0.0) {
                    errs.push(format!("{tn}: duration must be a finite number >= 0"));
                }
            }
        }
        if edges_ok && states.contains_key(l.entry.as_str()) {
            // reachability
            let n = l.states.len();
            let mut seen = vec![false; n];
            let mut stack = vec![states[l.entry.as_str()]];
            seen[stack[0]] = true;
            while let Some(s) = stack.pop() {
                for t in &l.transitions {
                    let from_ok = t.from == "*" || states[t.from.as_str()] == s;
                    let to = states[t.to.as_str()];
                    if from_ok && !seen[to] {
                        seen[to] = true;
                        stack.push(to);
                    }
                }
            }
            for (i, s) in l.states.iter().enumerate() {
                if !seen[i] {
                    errs.push(format!("{ln}: state '{}' is unreachable", s.name));
                }
            }
            // cycles of unconditional transitions (they would fire on every step)
            let mut adj: Vec<Vec<usize>> = vec![vec![]; n];
            for t in &l.transitions {
                if t.conditions.is_empty() && t.exit_time.is_none_or(|x| x <= 0.0) {
                    let to = states[t.to.as_str()];
                    if t.from == "*" {
                        for (f, a) in adj.iter_mut().enumerate() {
                            if f != to {
                                a.push(to);
                            }
                        }
                    } else {
                        adj[states[t.from.as_str()]].push(to);
                    }
                }
            }
            if let Some(s) = find_cycle(&adj) {
                errs.push(format!(
                    "{ln}: unconditional transitions form a cycle through state '{}' (add a condition or an exit_time; zero-duration cycles would loop forever)",
                    l.states[s].name
                ));
            }
        }
        layer_targets.push((l.name.clone(), targets));
    }
    for i in 0..layer_targets.len() {
        for j in i + 1..layer_targets.len() {
            if let Some((t, pr)) = layer_targets[i]
                .1
                .intersection(&layer_targets[j].1)
                .min_by(|a, b| a.0.cmp(&b.0))
            {
                errs.push(format!(
                    "machine '{mn}': layers '{}' and '{}' both drive {t}.{pr:?}; layers need disjoint channel groups (use a mask)",
                    layer_targets[i].0, layer_targets[j].0
                ));
            }
        }
    }
    errs
}

fn find_cycle(adj: &[Vec<usize>]) -> Option<usize> {
    // 0 = new, 1 = on stack, 2 = done
    let mut color = vec![0u8; adj.len()];
    fn dfs(u: usize, adj: &[Vec<usize>], color: &mut [u8]) -> Option<usize> {
        color[u] = 1;
        for &v in &adj[u] {
            if color[v] == 1 {
                return Some(v);
            }
            if color[v] == 0 {
                if let Some(c) = dfs(v, adj, color) {
                    return Some(c);
                }
            }
        }
        color[u] = 2;
        None
    }
    for s in 0..adj.len() {
        if color[s] == 0 {
            if let Some(c) = dfs(s, adj, &mut color) {
                return Some(c);
            }
        }
    }
    None
}

/// Compile the project once with a constant channel for every property the machines and bindings
/// can write, so unknown bones / layers / parameters are reported with the engine's own messages.
/// Returns nothing when there are no machines or bindings. Called by `Engine` after its own checks.
pub fn trial_errors(p: &Project, asset_dir: &std::path::Path) -> Vec<String> {
    if p.machines.is_empty() && p.bindings.is_empty() {
        return vec![];
    }
    let mut probe = p.clone();
    probe.machines.clear();
    probe.bindings.clear();
    let mut seen: HashSet<(String, Prop)> = HashSet::new();
    let mut add = |probe: &mut Project, t: &str, pr: Prop| {
        if seen.insert((t.to_string(), pr)) {
            probe.channels.push(Channel::keys(
                t,
                pr,
                Track::with_keys(&[(0.0, 0.0, Ease::Linear)]),
            ));
        }
    };
    for m in &p.machines {
        for l in &m.layers {
            for s in &l.states {
                if let Ok(leaves) = source_leaves(p, &s.source, &l.mask) {
                    for lf in leaves {
                        for (t, pr, ..) in lf.tracks {
                            add(&mut probe, &t, pr);
                        }
                    }
                }
            }
        }
    }
    for b in &p.bindings {
        if let crate::binding::Target::Prop { target, prop } = &b.target {
            add(&mut probe, target, *prop);
        }
    }
    match Engine::with_asset_dir(probe, asset_dir) {
        Ok(_) => vec![],
        Err(e) => e
            .into_iter()
            .map(|m| format!("machine/binding targets: {m}"))
            .collect(),
    }
}

// ---------------------------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------------------------

struct RtCond {
    input: usize,
    op: Cmp,
    value: f64,
}

struct RtTrans {
    from: Option<usize>,
    to: usize,
    conds: Vec<RtCond>,
    exit: Option<f32>,
    /// Effective crossfade, seconds.
    dur: f64,
    ease: Ease,
}

enum RtKind {
    Leaf(Leaf),
    Blend {
        input: usize,
        thresholds: Vec<f32>,
        children: Vec<Leaf>,
    },
}

struct RtState {
    name: String,
    mode: PlayMode,
    speed: f64,
    kind: RtKind,
}

#[derive(Clone, Copy)]
struct Play {
    state: usize,
    /// Position in loops of the state's length (0 at entry).
    pos: f64,
}

enum FadeFrom {
    Live(Play),
    Snap(Pose),
}

struct Fade {
    from: FadeFrom,
    elapsed: f64,
    dur: f64,
    ease: Ease,
}

struct LayerRt {
    name: String,
    states: Vec<RtState>,
    trans: Vec<RtTrans>,
    cur: Play,
    fade: Option<Fade>,
}

/// A live machine: feed inputs, advance time, read the pose / frame.
pub struct MachineRuntime {
    machine_name: String,
    base: Project,
    asset_dir: std::path::PathBuf,
    fps: f32,
    base_frame: f32,
    in_names: Vec<String>,
    in_kind: Vec<InputKind>,
    in_hold: Vec<bool>,
    values: Vec<f64>,
    layers: Vec<LayerRt>,
    time: f64,
    events: Vec<MachineEvent>,
    bindings: crate::binding::BindingRuntime,
}

fn phase_of(mode: PlayMode, pos: f64) -> f64 {
    match mode {
        PlayMode::Loop => pos - pos.floor(),
        PlayMode::Once => pos.clamp(0.0, 1.0),
        PlayMode::PingPong => pos.rem_euclid(2.0) / 2.0,
    }
}

/// Position within the source (0..1) that is sampled.
fn sample_phase(mode: PlayMode, pos: f64) -> f32 {
    match mode {
        PlayMode::Loop => (pos - pos.floor()) as f32,
        PlayMode::Once => pos.clamp(0.0, 1.0) as f32,
        PlayMode::PingPong => {
            let m = pos.rem_euclid(2.0);
            (if m <= 1.0 { m } else { 2.0 - m }) as f32
        }
    }
}

impl MachineRuntime {
    /// Build the runtime of the named machine (asset directory: current directory).
    pub fn new(project: &Project, machine: &str) -> Result<MachineRuntime, String> {
        MachineRuntime::with_asset_dir(project, machine, std::path::Path::new("."))
    }

    pub fn with_asset_dir(
        project: &Project,
        machine: &str,
        asset_dir: &std::path::Path,
    ) -> Result<MachineRuntime, String> {
        let m = project
            .machines
            .iter()
            .find(|m| m.name == machine)
            .ok_or_else(|| format!("unknown machine '{machine}'"))?;
        let mut errs = validate_machine(project, m);
        errs.extend(crate::binding::validate(project));
        if !errs.is_empty() {
            return Err(errs.join("; "));
        }
        let in_ix: HashMap<&str, usize> = m
            .inputs
            .iter()
            .enumerate()
            .map(|(i, x)| (x.name.as_str(), i))
            .collect();
        let mut layers = Vec::new();
        for l in &m.layers {
            let sx: HashMap<&str, usize> = l
                .states
                .iter()
                .enumerate()
                .map(|(i, s)| (s.name.as_str(), i))
                .collect();
            let mut states = Vec::new();
            for s in &l.states {
                let kind = match &s.source {
                    StateSource::Blend { input, children } => RtKind::Blend {
                        input: in_ix[input.as_str()],
                        thresholds: children.iter().map(|c| c.threshold).collect(),
                        children: children
                            .iter()
                            .map(|c| resolve_leaf(project, &c.source, &l.mask))
                            .collect::<Result<_, _>>()?,
                    },
                    other => RtKind::Leaf(resolve_leaf(project, other, &l.mask)?),
                };
                states.push(RtState {
                    name: s.name.clone(),
                    mode: s.mode,
                    speed: s.speed as f64,
                    kind,
                });
            }
            let trans = l
                .transitions
                .iter()
                .map(|t| {
                    let to = sx[t.to.as_str()];
                    RtTrans {
                        from: if t.from == "*" {
                            None
                        } else {
                            Some(sx[t.from.as_str()])
                        },
                        to,
                        conds: t
                            .conditions
                            .iter()
                            .map(|c| RtCond {
                                input: in_ix[c.input.as_str()],
                                op: c.op,
                                value: c.value,
                            })
                            .collect(),
                        exit: t.exit_time,
                        dur: t.duration.unwrap_or(l.states[to].blend_in) as f64,
                        ease: t.ease,
                    }
                })
                .collect();
            layers.push(LayerRt {
                name: l.name.clone(),
                states,
                trans,
                cur: Play {
                    state: sx[l.entry.as_str()],
                    pos: 0.0,
                },
                fade: None,
            });
        }
        let mut base = project.clone();
        base.machines.clear();
        base.bindings.clear();
        let mut rt = MachineRuntime {
            machine_name: m.name.clone(),
            base,
            asset_dir: asset_dir.to_path_buf(),
            fps: project.fps,
            base_frame: 0.0,
            in_names: m.inputs.iter().map(|i| i.name.clone()).collect(),
            in_kind: m.inputs.iter().map(|i| i.kind).collect(),
            in_hold: m.inputs.iter().map(|i| i.hold).collect(),
            values: m
                .inputs
                .iter()
                .map(|i| {
                    if i.kind == InputKind::Trigger {
                        0.0
                    } else {
                        i.default
                    }
                })
                .collect(),
            layers,
            time: 0.0,
            events: Vec::new(),
            bindings: crate::binding::BindingRuntime::new(&project.bindings)?,
        };
        for l in 0..rt.layers.len() {
            let name = rt.layers[l].states[rt.layers[l].cur.state].name.clone();
            rt.events.push(MachineEvent {
                time: 0.0,
                layer: rt.layers[l].name.clone(),
                kind: EventKind::Entered,
                state: name,
            });
        }
        Ok(rt)
    }

    pub fn machine_name(&self) -> &str {
        &self.machine_name
    }
    pub fn time(&self) -> f64 {
        self.time
    }
    /// Project frame at which the project's own channels are evaluated (default 0).
    pub fn set_base_frame(&mut self, f: f32) {
        self.base_frame = f;
    }

    fn input_ix(&self, name: &str) -> Result<usize, String> {
        self.in_names
            .iter()
            .position(|n| n == name)
            .ok_or_else(|| format!("machine '{}' has no input '{name}'", self.machine_name))
    }

    pub fn set_bool(&mut self, name: &str, v: bool) -> Result<(), String> {
        let i = self.input_ix(name)?;
        if self.in_kind[i] != InputKind::Bool {
            return Err(format!("input '{name}' is not a bool"));
        }
        self.values[i] = if v { 1.0 } else { 0.0 };
        Ok(())
    }

    pub fn set_number(&mut self, name: &str, v: f64) -> Result<(), String> {
        let i = self.input_ix(name)?;
        if self.in_kind[i] != InputKind::Number {
            return Err(format!("input '{name}' is not a number"));
        }
        if !v.is_finite() {
            return Err(format!("input '{name}': value must be finite"));
        }
        self.values[i] = v;
        Ok(())
    }

    pub fn fire(&mut self, name: &str) -> Result<(), String> {
        let i = self.input_ix(name)?;
        if self.in_kind[i] != InputKind::Trigger {
            return Err(format!("input '{name}' is not a trigger"));
        }
        self.values[i] = 1.0;
        Ok(())
    }

    /// Set an input of any kind from a number (bool: `!= 0`; trigger: fires when `!= 0`).
    pub fn set_input(&mut self, name: &str, v: f64) -> Result<(), String> {
        match self.in_kind[self.input_ix(name)?] {
            InputKind::Bool => self.set_bool(name, v != 0.0),
            InputKind::Number => self.set_number(name, v),
            InputKind::Trigger => {
                if v != 0.0 {
                    self.fire(name)
                } else {
                    Ok(())
                }
            }
        }
    }

    pub fn input_value(&self, name: &str) -> Option<f64> {
        self.in_names
            .iter()
            .position(|n| n == name)
            .map(|i| self.values[i])
    }

    /// Publish an external value for the project's data bindings (applied on the next `step`).
    pub fn set_value(&mut self, path: &str, v: f64) -> Result<(), String> {
        self.bindings.set_value(path, v)
    }

    /// Name of the current state of a layer.
    pub fn state_of(&self, layer: &str) -> Option<&str> {
        self.layers
            .iter()
            .find(|l| l.name == layer)
            .map(|l| l.states[l.cur.state].name.as_str())
    }

    /// `(layer, state)` for every layer.
    pub fn states(&self) -> Vec<(String, String)> {
        self.layers
            .iter()
            .map(|l| (l.name.clone(), l.states[l.cur.state].name.clone()))
            .collect()
    }

    /// Whether the layer is currently crossfading.
    pub fn is_fading(&self, layer: &str) -> bool {
        self.layers
            .iter()
            .find(|l| l.name == layer)
            .is_some_and(|l| l.fade.is_some())
    }

    /// Events not yet taken, oldest first.
    pub fn events(&self) -> &[MachineEvent] {
        &self.events
    }

    pub fn clear_events(&mut self) {
        self.events.clear();
    }

    /// Events since the last call (entered / exited), oldest first.
    pub fn take_events(&mut self) -> Vec<MachineEvent> {
        std::mem::take(&mut self.events)
    }

    // ---- sampling ----

    fn leaf_pose(leaf: &Leaf, phase: f32) -> Pose {
        let mut pose = Pose::default();
        let local = leaf.offset + phase * leaf.len;
        for (t, p, track, w) in &leaf.tracks {
            if let Some(v) = track.eval(local) {
                if v.is_finite() {
                    pose.add(t, *p, v, *w);
                }
            }
        }
        pose
    }

    fn weights_of(&self, thresholds: &[f32], input: usize) -> Vec<f32> {
        blend_weights(thresholds, self.values[input] as f32)
    }

    fn state_len(&self, st: &RtState) -> f64 {
        match &st.kind {
            RtKind::Leaf(l) => l.len as f64,
            RtKind::Blend {
                input,
                thresholds,
                children,
            } => {
                let w = self.weights_of(thresholds, *input);
                children
                    .iter()
                    .zip(&w)
                    .map(|(c, w)| c.len as f64 * *w as f64)
                    .sum()
            }
        }
    }

    fn play_pose(&self, l: &LayerRt, play: Play) -> Pose {
        let st = &l.states[play.state];
        let phase = sample_phase(st.mode, play.pos);
        match &st.kind {
            RtKind::Leaf(leaf) => MachineRuntime::leaf_pose(leaf, phase),
            RtKind::Blend {
                input,
                thresholds,
                children,
            } => {
                let w = self.weights_of(thresholds, *input);
                let poses: Vec<Pose> = children
                    .iter()
                    .zip(&w)
                    .map(|(c, w)| {
                        if *w > 0.0 {
                            MachineRuntime::leaf_pose(c, phase)
                        } else {
                            Pose::default()
                        }
                    })
                    .collect();
                let parts: Vec<(&Pose, f32)> = poses.iter().zip(&w).map(|(p, w)| (p, *w)).collect();
                Pose::mix(&parts)
            }
        }
    }

    fn layer_pose(&self, li: usize) -> Pose {
        let l = &self.layers[li];
        let cur = self.play_pose(l, l.cur);
        match &l.fade {
            None => cur,
            Some(f) => {
                let from = match &f.from {
                    FadeFrom::Live(p) => self.play_pose(l, *p),
                    FadeFrom::Snap(s) => s.clone(),
                };
                let u = if f.dur > 0.0 {
                    f.ease.apply((f.elapsed / f.dur) as f32).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                Pose::mix(&[(&from, 1.0 - u), (&cur, u)])
            }
        }
    }

    /// Pose of all layers together.
    pub fn pose(&self) -> Pose {
        let mut out = Pose::default();
        for li in 0..self.layers.len() {
            for (t, p, v, w) in self.layer_pose(li).items {
                out.add(&t, p, v, w);
            }
        }
        out
    }

    /// Channels the runtime adds to the project: machine pose, then data-binding properties.
    pub fn pose_channels(&self) -> Vec<Channel> {
        let mut c = self.pose().channels();
        c.extend(self.bindings.channels());
        c
    }

    /// Project (without machines / bindings) with the current pose channels appended.
    pub fn project_now(&self) -> Project {
        let mut p = self.base.clone();
        p.channels.extend(self.pose_channels());
        p
    }

    /// Engine for the current state; evaluate it at [`MachineRuntime::base_frame`] (or render with
    /// the SVG / raster renderers).
    pub fn engine(&self) -> Result<Engine, String> {
        Engine::with_asset_dir(self.project_now(), &self.asset_dir).map_err(|e| e.join("; "))
    }

    /// Like [`MachineRuntime::engine`] but `edit` may change the project copy first (used by the
    /// facade to apply a canvas size and scroll offsets without touching the machine state).
    pub fn engine_with(&self, edit: impl FnOnce(&mut Project)) -> Result<Engine, String> {
        let mut p = self.project_now();
        edit(&mut p);
        Engine::with_asset_dir(p, &self.asset_dir).map_err(|e| e.join("; "))
    }

    pub fn base_frame(&self) -> f32 {
        self.base_frame
    }

    /// Evaluated frame at the machine's current local times.
    pub fn frame(&self) -> Result<Frame, String> {
        Ok(self.engine()?.evaluate(self.base_frame))
    }

    // ---- stepping ----

    /// Advance by `dt` seconds (0 <= dt <= 10). Bindings are applied first, then time passes in
    /// substeps of at most 1/60 s with transitions checked after each.
    pub fn step(&mut self, dt: f64) -> Result<(), String> {
        if !(dt.is_finite() && (0.0..=MAX_STEP).contains(&dt)) {
            return Err(format!("dt must be within 0..={MAX_STEP} seconds"));
        }
        self.bindings.update(dt);
        for (m, input, v, edge) in self.bindings.input_outputs() {
            if m != self.machine_name {
                continue;
            }
            if let Ok(i) = self.input_ix(&input) {
                match self.in_kind[i] {
                    InputKind::Bool => self.values[i] = if v != 0.0 { 1.0 } else { 0.0 },
                    InputKind::Number => self.values[i] = v,
                    InputKind::Trigger => {
                        if edge {
                            self.values[i] = 1.0;
                        }
                    }
                }
            }
        }
        let n = if dt == 0.0 {
            1
        } else {
            (dt / SUBSTEP).ceil() as usize
        };
        let h = dt / n as f64;
        for _ in 0..n {
            self.substep(h);
        }
        for i in 0..self.values.len() {
            if self.in_kind[i] == InputKind::Trigger && !self.in_hold[i] {
                self.values[i] = 0.0;
            }
        }
        Ok(())
    }

    fn advance(&self, l: &LayerRt, play: &mut Play, h: f64) {
        let st = &l.states[play.state];
        let len = self.state_len(st);
        if len > 0.0 {
            play.pos += h * st.speed * self.fps as f64 / len;
        }
    }

    fn substep(&mut self, h: f64) {
        self.time += h;
        for li in 0..self.layers.len() {
            // advance time
            let mut cur = self.layers[li].cur;
            self.advance(&self.layers[li], &mut cur, h);
            let mut fade_done = false;
            let mut live = None;
            if let Some(f) = &self.layers[li].fade {
                if let FadeFrom::Live(p) = f.from {
                    let mut p = p;
                    self.advance(&self.layers[li], &mut p, h);
                    live = Some(p);
                }
                fade_done = f.elapsed + h >= f.dur;
            }
            let l = &mut self.layers[li];
            l.cur = cur;
            if let Some(f) = &mut l.fade {
                f.elapsed += h;
                if let Some(p) = live {
                    f.from = FadeFrom::Live(p);
                }
            }
            if fade_done {
                l.fade = None;
            }
            // transitions (zero-duration ones may chain)
            for _ in 0..MAX_HOPS {
                let Some(ti) = self.find_transition(li) else {
                    break;
                };
                let instant = self.fire_transition(li, ti);
                if !instant {
                    break;
                }
            }
        }
    }

    fn find_transition(&self, li: usize) -> Option<usize> {
        let l = &self.layers[li];
        let st = &l.states[l.cur.state];
        let phase = phase_of(st.mode, l.cur.pos);
        'next: for (i, t) in l.trans.iter().enumerate() {
            match t.from {
                Some(f) if f != l.cur.state => continue,
                None if t.to == l.cur.state => continue,
                _ => {}
            }
            if let Some(x) = t.exit {
                if phase < x as f64 - 1e-9 {
                    continue;
                }
            }
            for c in &t.conds {
                if !c.op.test(self.values[c.input], c.value) {
                    continue 'next;
                }
            }
            return Some(i);
        }
        None
    }

    /// Returns whether the transition was instantaneous.
    fn fire_transition(&mut self, li: usize, ti: usize) -> bool {
        let snap = if self.layers[li].fade.is_some() {
            Some(self.layer_pose(li))
        } else {
            None
        };
        let consumed: Vec<usize> = self.layers[li].trans[ti]
            .conds
            .iter()
            .map(|c| c.input)
            .filter(|&i| self.in_kind[i] == InputKind::Trigger)
            .collect();
        for i in consumed {
            self.values[i] = 0.0;
        }
        let time = self.time;
        let l = &mut self.layers[li];
        let (to, dur, ease) = (l.trans[ti].to, l.trans[ti].dur, l.trans[ti].ease);
        let old = l.cur;
        self.events.push(MachineEvent {
            time,
            layer: l.name.clone(),
            kind: EventKind::Exited,
            state: l.states[old.state].name.clone(),
        });
        self.events.push(MachineEvent {
            time,
            layer: l.name.clone(),
            kind: EventKind::Entered,
            state: l.states[to].name.clone(),
        });
        l.cur = Play {
            state: to,
            pos: 0.0,
        };
        if dur > 0.0 {
            l.fade = Some(Fade {
                from: snap.map_or(FadeFrom::Live(old), FadeFrom::Snap),
                elapsed: 0.0,
                dur,
                ease,
            });
            false
        } else {
            l.fade = None;
            true
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Scripted simulation (used by `machine-sim`, the C ABI and tests)
// ---------------------------------------------------------------------------------------------

/// One timed batch of inputs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ScriptEvent {
    /// Seconds from the start.
    pub t: f64,
    /// Input name -> value (bool: 0/1 or true/false; number).
    #[serde(default)]
    pub set: std::collections::BTreeMap<String, serde_json::Value>,
    /// Triggers to fire.
    #[serde(default)]
    pub trigger: Vec<String>,
    /// External value path -> value for data bindings.
    #[serde(default)]
    pub value: std::collections::BTreeMap<String, f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Script {
    /// Machine name (default: the project's only machine).
    #[serde(default)]
    pub machine: Option<String>,
    /// Total length in seconds (default: last event + 1 s).
    #[serde(default)]
    pub duration: Option<f64>,
    pub events: Vec<ScriptEvent>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Tick {
    pub tick: usize,
    pub time: f64,
    pub states: Vec<(String, String)>,
    pub fading: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SimLog {
    pub machine: String,
    pub fps: f32,
    pub ticks: Vec<Tick>,
    pub events: Vec<MachineEvent>,
}

pub const MAX_SIM_TICKS: usize = 1_000_000;

/// Run a script at a fixed rate: tick 0 applies the events with `t <= 0` and steps by 0, tick k
/// applies events with `(k-1)/fps < t <= k/fps` and steps by `1/fps`. `per_tick` sees the runtime
/// after every tick (render frames there).
pub fn simulate(
    project: &Project,
    script: &Script,
    fps: f32,
    asset_dir: &std::path::Path,
    mut per_tick: impl FnMut(usize, &MachineRuntime) -> Result<(), String>,
) -> Result<SimLog, String> {
    if !(fps.is_finite() && fps > 0.0 && fps <= 1000.0) {
        return Err("fps must be within (0, 1000]".into());
    }
    let name = match &script.machine {
        Some(n) => n.clone(),
        None => match project.machines.as_slice() {
            [m] => m.name.clone(),
            [] => return Err("project has no machines".into()),
            _ => return Err("project has several machines: name one in the script".into()),
        },
    };
    let mut rt = MachineRuntime::with_asset_dir(project, &name, asset_dir)?;
    let mut evs = script.events.clone();
    if evs.iter().any(|e| !e.t.is_finite() || e.t < 0.0) {
        return Err("script event times must be finite and >= 0".into());
    }
    evs.sort_by(|a, b| a.t.total_cmp(&b.t));
    let last = evs.last().map_or(0.0, |e| e.t);
    let duration = script.duration.unwrap_or(last + 1.0);
    if !(duration.is_finite() && duration >= 0.0) {
        return Err("duration must be finite and >= 0".into());
    }
    let n = (duration * fps as f64).floor() as usize + 1;
    if n > MAX_SIM_TICKS {
        return Err(format!(
            "simulation would need more than {MAX_SIM_TICKS} ticks"
        ));
    }
    let dt = 1.0 / fps as f64;
    let mut next = 0;
    let mut cur = 0.0f64;
    let mut log = SimLog {
        machine: name,
        fps,
        ticks: vec![],
        events: vec![],
    };
    for k in 0..n {
        let t_end = k as f64 * dt;
        while next < evs.len() && evs[next].t <= t_end + 1e-9 {
            let e = &evs[next];
            // Advance to the event's own time first, so an input never acts before it was given
            // (it used to be applied at the start of the tick that contains it).
            let te = e.t.clamp(cur, t_end.max(cur));
            if te > cur {
                rt.step(te - cur)?;
                log.events.extend(rt.take_events());
                cur = te;
            }
            for (n, v) in &e.set {
                let x = match v {
                    serde_json::Value::Bool(b) => *b as u8 as f64,
                    serde_json::Value::Number(x) => x.as_f64().unwrap_or(f64::NAN),
                    _ => {
                        return Err(format!(
                            "script input '{n}': value must be a number or boolean"
                        ))
                    }
                };
                rt.set_input(n, x)?;
            }
            for n in &e.trigger {
                rt.fire(n)?;
            }
            for (p, v) in &e.value {
                rt.set_value(p, *v)?;
            }
            next += 1;
        }
        rt.step((t_end - cur).max(0.0))?;
        cur = cur.max(t_end);
        log.events.extend(rt.take_events());
        let fading = rt
            .layers
            .iter()
            .filter(|l| l.fade.is_some())
            .map(|l| l.name.clone())
            .collect();
        log.ticks.push(Tick {
            tick: k,
            time: t_end,
            states: rt.states(),
            fading,
        });
        per_tick(k, &rt)?;
    }
    Ok(log)
}

// ---------------------------------------------------------------------------------------------
// Editing helpers
// ---------------------------------------------------------------------------------------------

/// Add a machine (replacing nothing): the project must stay valid.
pub fn add(p: &mut Project, m: Machine) -> Result<(), String> {
    if p.machines.iter().any(|x| x.name == m.name) {
        return Err(format!("machine '{}' already exists", m.name));
    }
    let mut trial = p.clone();
    trial.machines.push(m.clone());
    let errs = validate(&trial);
    if !errs.is_empty() {
        return Err(errs.join("\n"));
    }
    p.machines.push(m);
    Ok(())
}

pub fn remove(p: &mut Project, name: &str) -> Result<(), String> {
    let i = p
        .machines
        .iter()
        .position(|m| m.name == name)
        .ok_or_else(|| format!("no machine '{name}'"))?;
    if let Some(b) = p.bindings.iter().find(
        |b| matches!(&b.target, crate::binding::Target::Input { machine, .. } if machine == name),
    ) {
        return Err(format!(
            "binding '{}' still drives machine '{name}'",
            b.name
        ));
    }
    p.machines.remove(i);
    Ok(())
}

pub fn add_binding(p: &mut Project, b: crate::binding::Binding) -> Result<(), String> {
    if p.bindings.iter().any(|x| x.name == b.name) {
        return Err(format!("binding '{}' already exists", b.name));
    }
    let mut trial = p.clone();
    trial.bindings.push(b.clone());
    let errs = crate::binding::validate(&trial);
    if !errs.is_empty() {
        return Err(errs.join("\n"));
    }
    let t = trial_errors(&trial, std::path::Path::new("."));
    if !t.is_empty() {
        return Err(t.join("\n"));
    }
    p.bindings.push(b);
    Ok(())
}

pub fn remove_binding(p: &mut Project, name: &str) -> Result<(), String> {
    let i = p
        .bindings
        .iter()
        .position(|b| b.name == name)
        .ok_or_else(|| format!("no binding '{name}'"))?;
    p.bindings.remove(i);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::Key;
    use crate::binding::{Binding, Remap, Target};
    use crate::clipstack::Clip;
    use crate::motionlib::{Take, TakeTrack};

    fn k(f: f32, v: f32) -> Key {
        Key {
            frame: f,
            value: v,
            ease: Ease::Linear,
        }
    }

    fn take(name: &str, len: f32, target: &str, prop: Prop, a: f32, b: f32) -> Take {
        Take {
            name: name.into(),
            length: len,
            tracks: vec![TakeTrack {
                target: target.into(),
                prop,
                keys: vec![k(0.0, a), k(len, b)],
            }],
        }
    }

    /// 24 fps project with a bone "arm", layers "body" and "eye", clips idle (angle 0), walk
    /// (angle 10..20, 24 frames), run (angle 40, 24 frames), wave (once, 12 frames, angle 90).
    fn project() -> Project {
        let mut p = Project::new("m", 100.0, 100.0, 24.0, 48);
        p.bones.push(crate::project::Bone::new(
            "arm",
            None,
            10.0,
            crate::math::Vec2::new(0.0, 0.0),
            0.0,
        ));
        p.bones.push(crate::project::Bone::new(
            "leg",
            None,
            10.0,
            crate::math::Vec2::new(0.0, 0.0),
            0.0,
        ));
        let mk = |n: &str, t: Take| Clip::new(n, t);
        p.clips = vec![
            mk("idle", take("idle", 24.0, "arm", Prop::BoneAngle, 0.0, 0.0)),
            mk(
                "walk",
                take("walk", 24.0, "arm", Prop::BoneAngle, 10.0, 20.0),
            ),
            mk("run", take("run", 24.0, "arm", Prop::BoneAngle, 40.0, 40.0)),
            mk(
                "wave",
                take("wave", 12.0, "arm", Prop::BoneAngle, 90.0, 90.0),
            ),
            mk(
                "legsit",
                take("legsit", 24.0, "leg", Prop::BoneAngle, 5.0, 5.0),
            ),
        ];
        p
    }

    fn st(name: &str, clip: &str) -> State {
        State {
            name: name.into(),
            source: StateSource::Clip { clip: clip.into() },
            mode: PlayMode::Loop,
            speed: 1.0,
            blend_in: 0.0,
        }
    }
    fn tr(from: &str, to: &str, conds: Vec<Condition>) -> Transition {
        Transition {
            from: from.into(),
            to: to.into(),
            conditions: conds,
            exit_time: None,
            duration: Some(0.0),
            ease: Ease::Linear,
        }
    }
    fn cond(input: &str, op: Cmp, value: f64) -> Condition {
        Condition {
            input: input.into(),
            op,
            value,
        }
    }
    fn inp(name: &str, kind: InputKind) -> Input {
        Input {
            name: name.into(),
            kind,
            default: 0.0,
            hold: false,
        }
    }
    fn layer(name: &str, entry: &str, states: Vec<State>, trans: Vec<Transition>) -> Layer {
        Layer {
            name: name.into(),
            mask: vec![],
            entry: entry.into(),
            states,
            transitions: trans,
        }
    }

    fn base_machine() -> Machine {
        Machine {
            name: "m".into(),
            inputs: vec![
                inp("moving", InputKind::Bool),
                inp("speed", InputKind::Number),
                inp("wave", InputKind::Trigger),
            ],
            layers: vec![layer(
                "body",
                "idle",
                vec![
                    st("idle", "idle"),
                    st("walk", "walk"),
                    State {
                        mode: PlayMode::Once,
                        ..st("wave", "wave")
                    },
                ],
                vec![
                    tr("idle", "walk", vec![cond("moving", Cmp::Eq, 1.0)]),
                    tr("walk", "idle", vec![cond("moving", Cmp::Eq, 0.0)]),
                    tr("*", "wave", vec![cond("wave", Cmp::Eq, 1.0)]),
                    Transition {
                        exit_time: Some(1.0),
                        ..tr("wave", "idle", vec![])
                    },
                ],
            )],
        }
    }

    #[test]
    fn unknown_keys_in_a_transition_are_an_error_not_an_always_firing_transition() {
        // `conditions` is not the key (it is `when`): this used to load as an unconditional transition
        let good = r#"{"from":"idle","to":"walk","when":[{"input":"moving"}]}"#;
        assert!(serde_json::from_str::<Transition>(good).is_ok());
        let typo = r#"{"from":"idle","to":"walk","conditions":[{"input":"moving"}]}"#;
        let err = serde_json::from_str::<Transition>(typo)
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown field `conditions`"), "{err}");
        let bad_cond = r#"{"from":"idle","to":"walk","when":[{"input":"moving","vaule":1}]}"#;
        assert!(serde_json::from_str::<Transition>(bad_cond)
            .unwrap_err()
            .to_string()
            .contains("vaule"));
        // and through a whole project file
        let mut p = project();
        p.machines.push(base_machine());
        let text = p.to_json_pretty().replace("\"when\"", "\"conditions\"");
        let err = Project::from_json(&text)
            .expect_err("must not load")
            .to_string();
        assert!(err.contains("conditions"), "{err}");
    }

    fn rt_of(m: Machine, mut p: Project) -> MachineRuntime {
        p.machines.push(m);
        MachineRuntime::new(&p, "m").unwrap()
    }

    fn angle(rt: &MachineRuntime) -> f32 {
        rt.pose()
            .get("arm", Prop::BoneAngle)
            .map(|x| x.0)
            .unwrap_or(f32::NAN)
    }

    #[test]
    fn transitions_by_inputs() {
        let mut rt = rt_of(base_machine(), project());
        assert_eq!(rt.state_of("body"), Some("idle"));
        rt.step(0.1).unwrap();
        assert_eq!(rt.state_of("body"), Some("idle"));
        rt.set_bool("moving", true).unwrap();
        rt.step(0.05).unwrap();
        assert_eq!(rt.state_of("body"), Some("walk"));
        rt.step(0.25).unwrap(); // entered after the first 1/60 s substep, so 0.2833 s into the 1 s clip
        assert!((angle(&rt) - 12.8333).abs() < 0.01, "{}", angle(&rt));
        rt.set_bool("moving", false).unwrap();
        rt.step(0.02).unwrap();
        assert_eq!(rt.state_of("body"), Some("idle"));
        let ev = rt.take_events();
        let kinds: Vec<String> = ev
            .iter()
            .map(|e| format!("{:?}:{}", e.kind, e.state))
            .collect();
        assert_eq!(
            kinds,
            [
                "Entered:idle",
                "Exited:idle",
                "Entered:walk",
                "Exited:walk",
                "Entered:idle"
            ]
        );
        assert!(rt.take_events().is_empty());
    }

    #[test]
    fn trigger_is_consumed_once_and_expires() {
        let mut rt = rt_of(base_machine(), project());
        rt.fire("wave").unwrap();
        rt.step(0.05).unwrap();
        assert_eq!(rt.state_of("body"), Some("wave"));
        assert_eq!(rt.input_value("wave"), Some(0.0));
        // finishing the once clip (0.5 s) goes back to idle and the consumed trigger does not refire
        rt.step(0.7).unwrap();
        assert_eq!(rt.state_of("body"), Some("idle"));
        rt.step(0.5).unwrap();
        assert_eq!(rt.state_of("body"), Some("idle"));
        // a trigger nobody tests lasts one step only
        let mut m = base_machine();
        m.layers[0].transitions[2].conditions = vec![cond("moving", Cmp::Eq, 1.0)];
        let mut rt = rt_of(m, project());
        rt.fire("wave").unwrap();
        assert_eq!(rt.input_value("wave"), Some(1.0));
        rt.step(0.1).unwrap();
        assert_eq!(rt.input_value("wave"), Some(0.0));
        // hold keeps it
        let mut m = base_machine();
        m.inputs[2].hold = true;
        m.layers[0].transitions[2].conditions = vec![cond("moving", Cmp::Eq, 1.0)];
        let mut rt = rt_of(m, project());
        rt.fire("wave").unwrap();
        rt.step(0.1).unwrap();
        assert_eq!(rt.input_value("wave"), Some(1.0));
        assert!(rt.fire("moving").is_err());
        assert!(rt.fire("nope").is_err());
    }

    #[test]
    fn any_state_and_priority() {
        let mut rt = rt_of(base_machine(), project());
        rt.set_bool("moving", true).unwrap();
        rt.step(0.05).unwrap();
        assert_eq!(rt.state_of("body"), Some("walk"));
        rt.fire("wave").unwrap();
        rt.step(0.05).unwrap();
        // from walk via "*"
        assert_eq!(rt.state_of("body"), Some("wave"));
        // exit time 1.0: wave plays 0.5 s, then idle -> walk chain happens next step since moving is still true
        rt.step(0.6).unwrap();
        assert!(matches!(rt.state_of("body"), Some("idle") | Some("walk")));
        rt.step(0.05).unwrap();
        assert_eq!(rt.state_of("body"), Some("walk"));
        // an any-state transition never targets the current state: no restart
        rt.fire("wave").unwrap();
        rt.step(0.05).unwrap();
        let pos_wave = rt.layers[0].cur.pos;
        rt.fire("wave").unwrap();
        rt.step(0.05).unwrap();
        assert!(rt.layers[0].cur.pos > pos_wave);
    }

    #[test]
    fn exit_time_waits_for_the_fraction() {
        let mut m = base_machine();
        m.layers[0].states.truncate(2);
        m.layers[0].entry = "walk".into();
        m.layers[0].transitions = vec![Transition {
            exit_time: Some(0.5),
            ..tr("walk", "idle", vec![])
        }];
        let mut rt = rt_of(m, project());
        rt.step(0.4).unwrap(); // 0.4 s of a 1 s clip
        assert_eq!(rt.state_of("body"), Some("walk"));
        rt.step(0.15).unwrap(); // past 0.5
        assert_eq!(rt.state_of("body"), Some("idle"));
        // exit time combined with a condition needs both
        let mut m = base_machine();
        m.layers[0].states.truncate(2);
        m.layers[0].transitions = vec![Transition {
            exit_time: Some(0.5),
            ..tr("idle", "walk", vec![cond("moving", Cmp::Eq, 1.0)])
        }];
        let mut rt = rt_of(m, project());
        rt.set_bool("moving", true).unwrap();
        rt.step(0.3).unwrap();
        assert_eq!(rt.state_of("body"), Some("idle"));
        rt.step(0.3).unwrap();
        assert_eq!(rt.state_of("body"), Some("walk"));
    }

    #[test]
    fn crossfade_blends_and_finishes() {
        let mut m = base_machine();
        m.layers[0].transitions[0].duration = Some(1.0);
        m.layers[0].transitions[0].ease = Ease::Linear;
        let mut rt = rt_of(m, project());
        rt.set_bool("moving", true).unwrap();
        rt.step(0.0).unwrap();
        assert!(rt.is_fading("body"));
        rt.step(0.5).unwrap();
        // idle = 0, walk at 0.5 s = 15; halfway through the fade -> 7.5
        assert!((angle(&rt) - 7.5).abs() < 0.3, "{}", angle(&rt));
        rt.step(0.6).unwrap();
        assert!(!rt.is_fading("body"));
        // interrupt a fade: a snapshot is taken and fades toward the new state
        let mut m = base_machine();
        m.layers[0].transitions[0].duration = Some(1.0);
        m.layers[0].transitions[1].duration = Some(1.0);
        let mut rt = rt_of(m, project());
        rt.set_bool("moving", true).unwrap();
        rt.step(0.5).unwrap();
        let before = angle(&rt);
        rt.set_bool("moving", false).unwrap();
        rt.step(0.0).unwrap();
        assert!(
            (angle(&rt) - before).abs() < 0.3,
            "no jump at the interrupt: {} vs {}",
            angle(&rt),
            before
        );
        rt.step(1.1).unwrap();
        assert!(angle(&rt).abs() < 1e-3);
    }

    #[test]
    fn blend_tree_weights() {
        assert_eq!(blend_weights(&[0.0, 1.0, 2.0], -1.0), vec![1.0, 0.0, 0.0]);
        assert_eq!(blend_weights(&[0.0, 1.0, 2.0], 3.0), vec![0.0, 0.0, 1.0]);
        let w = blend_weights(&[0.0, 1.0, 2.0], 1.25);
        assert_eq!(w, vec![0.0, 0.75, 0.25]);
        let mut p = project();
        let blend = State {
            name: "loco".into(),
            source: StateSource::Blend {
                input: "speed".into(),
                children: vec![
                    BlendChild {
                        threshold: 0.0,
                        source: StateSource::Clip {
                            clip: "idle".into(),
                        },
                    },
                    BlendChild {
                        threshold: 1.0,
                        source: StateSource::Clip {
                            clip: "walk".into(),
                        },
                    },
                    BlendChild {
                        threshold: 2.0,
                        source: StateSource::Clip { clip: "run".into() },
                    },
                ],
            },
            mode: PlayMode::Loop,
            speed: 1.0,
            blend_in: 0.0,
        };
        let m = Machine {
            name: "m".into(),
            inputs: vec![inp("speed", InputKind::Number)],
            layers: vec![layer("body", "loco", vec![blend], vec![])],
        };
        p.machines.push(m);
        let mut rt = MachineRuntime::new(&p, "m").unwrap();
        rt.set_number("speed", 0.0).unwrap();
        assert!((angle(&rt) - 0.0).abs() < 1e-4);
        rt.set_number("speed", 1.0).unwrap();
        assert!((angle(&rt) - 10.0).abs() < 1e-4); // walk at phase 0
        rt.set_number("speed", 1.5).unwrap();
        assert!((angle(&rt) - 25.0).abs() < 1e-4); // mean of walk (10) and run (40)
        rt.set_number("speed", 0.5).unwrap();
        assert!((angle(&rt) - 5.0).abs() < 1e-4);
        rt.set_number("speed", 9.0).unwrap();
        assert!((angle(&rt) - 40.0).abs() < 1e-4);
        // all clips are 24 frames, so the state keeps its 1 s period
        rt.step(0.5).unwrap();
        assert!((angle(&rt) - 40.0).abs() < 1e-4);
        rt.set_number("speed", 1.0).unwrap();
        assert!((angle(&rt) - 15.0).abs() < 0.3, "{}", angle(&rt));
        assert!(rt.set_number("nope", 1.0).is_err());
        assert!(rt.set_number("speed", f64::NAN).is_err());
    }

    #[test]
    fn blend_tree_clip_weights_use_stack_maths() {
        // a half-weight clip pulls toward the project value only by its coverage
        let mut p = project();
        p.clips[2].weight = 0.5; // run at half coverage
        let blend = State {
            name: "loco".into(),
            source: StateSource::Blend {
                input: "speed".into(),
                children: vec![
                    BlendChild {
                        threshold: 0.0,
                        source: StateSource::Clip {
                            clip: "walk".into(),
                        },
                    },
                    BlendChild {
                        threshold: 1.0,
                        source: StateSource::Clip { clip: "run".into() },
                    },
                ],
            },
            mode: PlayMode::Loop,
            speed: 1.0,
            blend_in: 0.0,
        };
        p.machines.push(Machine {
            name: "m".into(),
            inputs: vec![inp("speed", InputKind::Number)],
            layers: vec![layer("b", "loco", vec![blend], vec![])],
        });
        let mut rt = MachineRuntime::new(&p, "m").unwrap();
        rt.set_number("speed", 1.0).unwrap();
        let (v, w) = rt.pose().get("arm", Prop::BoneAngle).unwrap();
        assert!((v - 40.0).abs() < 1e-4 && (w - 0.5).abs() < 1e-6);
        rt.set_number("speed", 0.5).unwrap();
        let (v, w) = rt.pose().get("arm", Prop::BoneAngle).unwrap();
        // normalised average of walk(10, cov 1)*0.5 and run(40, cov .5)*0.5
        assert!((v - (0.5 * 10.0 + 0.25 * 40.0) / 0.75).abs() < 1e-4 && (w - 0.75).abs() < 1e-6);
    }

    #[test]
    fn ping_pong_and_speed() {
        let mut p = project();
        let s = State {
            mode: PlayMode::PingPong,
            speed: 2.0,
            ..st("walk", "walk")
        };
        p.machines.push(Machine {
            name: "m".into(),
            inputs: vec![],
            layers: vec![layer("b", "walk", vec![s], vec![])],
        });
        let mut rt = MachineRuntime::new(&p, "m").unwrap();
        rt.step(0.25).unwrap(); // 0.5 loops forward
        assert!((angle(&rt) - 15.0).abs() < 0.2);
        rt.step(0.5).unwrap(); // pos 1.5 -> mirrored back to 0.5
        assert!((angle(&rt) - 15.0).abs() < 0.2);
        rt.step(0.25).unwrap(); // pos 2.0 -> start
        assert!((angle(&rt) - 10.0).abs() < 0.2);
    }

    #[test]
    fn parallel_layers_on_disjoint_groups() {
        let mut p = project();
        let mut m = base_machine();
        m.layers[0].mask = vec!["arm".into()];
        m.layers
            .push(layer("legs", "sit", vec![st("sit", "legsit")], vec![]));
        m.layers[1].mask = vec!["leg".into()];
        p.machines.push(m);
        assert!(validate(&p).is_empty(), "{:?}", validate(&p));
        let mut rt = MachineRuntime::new(&p, "m").unwrap();
        rt.set_bool("moving", true).unwrap();
        rt.step(0.3).unwrap();
        assert_eq!(rt.state_of("body"), Some("walk"));
        assert_eq!(rt.state_of("legs"), Some("sit"));
        let pose = rt.pose();
        assert!(pose.get("arm", Prop::BoneAngle).is_some());
        assert_eq!(pose.get("leg", Prop::BoneAngle).unwrap().0, 5.0);
        // overlapping layers are rejected
        let mut q = p.clone();
        q.machines[0].layers[1].mask.clear();
        q.machines[0].layers[1].states[0].source = StateSource::Clip {
            clip: "walk".into(),
        };
        let e = validate(&q).join("\n");
        assert!(e.contains("disjoint channel groups"), "{e}");
    }

    #[test]
    fn validation_errors() {
        let base = |m: Machine| {
            let mut p = project();
            p.machines.push(m);
            validate(&p).join("\n")
        };
        assert_eq!(base(base_machine()), "");
        // unknown clip
        let mut m = base_machine();
        m.layers[0].states[0] = st("idle", "ghost");
        assert!(base(m).contains("unknown clip 'ghost'"));
        // duplicate names
        let mut m = base_machine();
        m.layers[0].states.push(st("idle", "idle"));
        m.inputs.push(inp("moving", InputKind::Bool));
        let e = base(m);
        assert!(
            e.contains("duplicate state name 'idle'") && e.contains("duplicate input 'moving'"),
            "{e}"
        );
        // missing target / source state, unknown input
        let mut m = base_machine();
        m.layers[0].transitions.push(tr("idle", "nowhere", vec![]));
        m.layers[0].transitions.push(tr("zip", "idle", vec![]));
        m.layers[0]
            .transitions
            .push(tr("idle", "walk", vec![cond("ghost", Cmp::Eq, 1.0)]));
        let e = base(m);
        assert!(
            e.contains("transition to missing state 'nowhere'")
                && e.contains("unknown source state 'zip'")
                && e.contains("unknown input 'ghost'"),
            "{e}"
        );
        // unreachable state
        let mut m = base_machine();
        m.layers[0].states.push(st("lonely", "run"));
        assert!(base(m).contains("state 'lonely' is unreachable"));
        // zero-duration unconditional cycle
        let mut m = base_machine();
        m.layers[0].transitions.push(tr("walk", "wave", vec![]));
        m.layers[0].transitions.push(tr("wave", "walk", vec![]));
        assert!(base(m).contains("form a cycle"));
        // but a cycle that waits for an exit time is fine, and so is one with conditions
        let mut m = base_machine();
        m.layers[0].transitions.push(Transition {
            exit_time: Some(1.0),
            ..tr("wave", "walk", vec![])
        });
        assert_eq!(base(m), "");
        // bad entry, bad numbers, bad blend
        let mut m = base_machine();
        m.layers[0].entry = "zzz".into();
        m.layers[0].states[0].speed = 0.0;
        m.layers[0].transitions[0].exit_time = Some(2.0);
        m.layers[0].transitions[1].duration = Some(-1.0);
        let e = base(m);
        assert!(
            e.contains("entry state 'zzz'")
                && e.contains("speed must")
                && e.contains("exit_time")
                && e.contains("duration must"),
            "{e}"
        );
        let mut m = base_machine();
        m.layers[0].states[0].source = StateSource::Blend {
            input: "moving".into(),
            children: vec![
                BlendChild {
                    threshold: 1.0,
                    source: StateSource::Clip {
                        clip: "idle".into(),
                    },
                },
                BlendChild {
                    threshold: 0.0,
                    source: StateSource::Clip {
                        clip: "idle".into(),
                    },
                },
            ],
        };
        let e = base(m);
        assert!(
            e.contains("must be a number input") && e.contains("strictly increasing"),
            "{e}"
        );
        // segments outside the project, regions that are not regions
        let mut m = base_machine();
        m.layers[0].states[0].source = StateSource::Segment {
            start: 10.0,
            end: 999.0,
        };
        assert!(base(m).contains("beyond"));
        let mut p = project();
        p.markers.push(crate::keyops::Marker {
            name: "pt".into(),
            frame: 3.0,
            end: None,
        });
        let mut m = base_machine();
        m.layers[0].states[0].source = StateSource::Region {
            marker: "pt".into(),
        };
        p.machines.push(m);
        assert!(validate(&p).join("\n").contains("not a region"));
        // a machine named like another
        let mut p = project();
        p.machines.push(base_machine());
        p.machines.push(base_machine());
        assert!(validate(&p).join("\n").contains("duplicate machine name"));
    }

    #[test]
    fn engine_new_rejects_bad_machines_and_unknown_targets() {
        let mut p = project();
        p.machines.push(base_machine());
        assert!(Engine::new(p.clone()).is_ok());
        let mut bad = p.clone();
        bad.machines[0].layers[0].states[0] = st("idle", "ghost");
        let e = Engine::new(bad).err().unwrap().join("\n");
        assert!(e.contains("unknown clip 'ghost'"), "{e}");
        // a clip that animates a bone the project does not have is caught by the trial compile
        let mut bad = p.clone();
        bad.clips[1].take.tracks[0].target = "no_such_bone".into();
        let e = Engine::new(bad).err().unwrap().join("\n");
        assert!(e.contains("machine/binding targets"), "{e}");
        // JSON round trip and omission when empty
        let json = p.to_json_pretty();
        let back = Project::from_json(&json).unwrap();
        assert_eq!(back.machines, p.machines);
        assert!(!project().to_json_pretty().contains("machines"));
        assert!(!project().to_json_pretty().contains("bindings"));
    }

    #[test]
    fn frame_is_rendered_from_the_pose() {
        let mut p = project();
        p.machines.push(base_machine());
        let mut rt = MachineRuntime::new(&p, "m").unwrap();
        rt.set_bool("moving", true).unwrap();
        rt.step(0.25).unwrap();
        let f = rt.frame().unwrap();
        // bone "arm" is rotated 12.5 degrees: world matrix rotation
        let rot = f.world[0].rotation().to_degrees();
        assert!((rot - 12.5).abs() < 0.3, "{rot}");
    }

    #[test]
    fn segments_and_regions_play_project_channels() {
        let mut p = project();
        p.channels.push(Channel::keys(
            "arm",
            Prop::BoneAngle,
            Track::with_keys(&[(0.0, 0.0, Ease::Linear), (24.0, 48.0, Ease::Linear)]),
        ));
        p.markers.push(crate::keyops::Marker {
            name: "second".into(),
            frame: 12.0,
            end: Some(24.0),
        });
        let s1 = State {
            source: StateSource::Segment {
                start: 0.0,
                end: 24.0,
            },
            ..st("seg", "")
        };
        let s2 = State {
            source: StateSource::Region {
                marker: "second".into(),
            },
            ..st("reg", "")
        };
        p.machines.push(Machine {
            name: "m".into(),
            inputs: vec![inp("go", InputKind::Trigger)],
            layers: vec![layer(
                "a",
                "seg",
                vec![s1, s2],
                vec![tr("seg", "reg", vec![cond("go", Cmp::Eq, 1.0)])],
            )],
        });
        let mut rt = MachineRuntime::new(&p, "m").unwrap();
        rt.step(0.5).unwrap();
        assert!((angle(&rt) - 24.0).abs() < 0.2);
        rt.fire("go").unwrap();
        rt.step(0.0).unwrap();
        assert_eq!(rt.state_of("a"), Some("reg"));
        assert!((angle(&rt) - 24.0).abs() < 0.2); // frame 12 of the project curve
        rt.step(0.25).unwrap();
        assert!((angle(&rt) - 36.0).abs() < 0.3);
    }

    #[test]
    fn deterministic_and_chunk_independent() {
        let run = |chunks: &[f64]| {
            let mut rt = rt_of(base_machine(), project());
            rt.set_bool("moving", true).unwrap();
            let mut out = vec![];
            for &c in chunks {
                rt.step(c).unwrap();
                out.push((rt.state_of("body").unwrap().to_string(), angle(&rt)));
            }
            (
                rt.time(),
                out.last().unwrap().clone(),
                rt.take_events().len(),
            )
        };
        let a = run(&[0.1, 0.2, 0.3, 0.4]);
        let b = run(&[0.1, 0.2, 0.3, 0.4]);
        assert_eq!(a, b);
        let c = run(&[0.5, 0.5]); // same total time, other chunking
        assert!((a.0 - c.0).abs() < 1e-9 && a.1 .0 == c.1 .0 && (a.1 .1 - c.1 .1).abs() < 0.3);
        let mut rt = rt_of(base_machine(), project());
        assert!(rt.step(-1.0).is_err() && rt.step(f64::NAN).is_err() && rt.step(11.0).is_err());
    }

    #[test]
    fn script_simulation_logs_states() {
        let mut p = project();
        p.machines.push(base_machine());
        let script = Script {
            machine: None,
            duration: Some(2.0),
            events: vec![
                ScriptEvent {
                    t: 0.5,
                    set: [("moving".to_string(), serde_json::json!(true))]
                        .into_iter()
                        .collect(),
                    ..Default::default()
                },
                ScriptEvent {
                    t: 1.0,
                    trigger: vec!["wave".into()],
                    ..Default::default()
                },
            ],
        };
        let mut seen = 0;
        let log = simulate(&p, &script, 24.0, std::path::Path::new("."), |_, _| {
            seen += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(seen, 49);
        assert_eq!(log.ticks.len(), 49);
        assert_eq!(log.ticks[0].states[0].1, "idle");
        assert_eq!(log.ticks[13].states[0].1, "walk"); // t = 13/24 > 0.5
        assert_eq!(log.ticks[25].states[0].1, "wave");
        assert!(log
            .events
            .iter()
            .any(|e| e.state == "wave" && e.kind == EventKind::Entered));
        // causality: no state is left before the input that causes it was given
        let left_idle = log
            .events
            .iter()
            .find(|e| e.state == "idle" && e.kind == EventKind::Exited)
            .unwrap();
        assert!(
            left_idle.time >= 0.5 - 1e-9,
            "left idle at {} before the input at 0.5",
            left_idle.time
        );
        let entered_wave = log
            .events
            .iter()
            .find(|e| e.state == "wave" && e.kind == EventKind::Entered)
            .unwrap();
        assert!(
            entered_wave.time >= 1.0 - 1e-9,
            "entered wave at {} before the trigger at 1.0",
            entered_wave.time
        );
        let log2 = simulate(&p, &script, 24.0, std::path::Path::new("."), |_, _| Ok(())).unwrap();
        assert_eq!(
            serde_json::to_string(&log).unwrap(),
            serde_json::to_string(&log2).unwrap()
        );
        // errors
        let bad = Script {
            events: vec![ScriptEvent {
                t: 0.0,
                set: [("ghost".to_string(), serde_json::json!(1))]
                    .into_iter()
                    .collect(),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(simulate(&p, &bad, 24.0, std::path::Path::new("."), |_, _| Ok(())).is_err());
        assert!(simulate(
            &project(),
            &script,
            24.0,
            std::path::Path::new("."),
            |_, _| Ok(())
        )
        .is_err());
    }

    #[test]
    fn bindings_drive_properties_and_inputs() {
        let mut p = project();
        p.machines.push(base_machine());
        let mut hp = Binding::new(
            "hp",
            "player.health",
            Target::Prop {
                target: "leg".into(),
                prop: Prop::BoneAngle,
            },
        );
        hp.remap = Some(Remap {
            in_min: 0.0,
            in_max: 100.0,
            out_min: -30.0,
            out_max: 30.0,
            clamp: true,
            curve: Ease::Linear,
        });
        let mv = Binding::new(
            "mv",
            "player.moving",
            Target::Input {
                machine: "m".into(),
                input: "moving".into(),
            },
        );
        let mut spd = Binding::new(
            "spd",
            "player.fire",
            Target::Input {
                machine: "m".into(),
                input: "wave".into(),
            },
        );
        spd.expr = Some("value * 2".into());
        p.bindings = vec![hp, mv, spd];
        assert!(Engine::new(p.clone()).is_ok());
        let mut rt = MachineRuntime::new(&p, "m").unwrap();
        rt.set_value("player.health", 75.0).unwrap();
        rt.set_value("player.moving", 1.0).unwrap();
        rt.step(0.1).unwrap();
        assert_eq!(rt.state_of("body"), Some("walk"));
        let ch = rt.pose_channels();
        let leg = ch.iter().find(|c| c.target == "leg").unwrap();
        match &leg.source {
            Source::Keys { track } => assert!((track.eval(0.0).unwrap() - 15.0).abs() < 1e-4),
            _ => panic!(),
        }
        // rendered frame has the bound property
        let f = rt.frame().unwrap();
        assert!((f.world[1].rotation().to_degrees() - 15.0).abs() < 0.3);
        // trigger binding fires on the rising edge only
        rt.set_value("player.fire", 0.0).unwrap();
        rt.step(0.05).unwrap();
        assert_eq!(rt.state_of("body"), Some("walk"));
        rt.set_value("player.fire", 1.0).unwrap();
        rt.step(0.05).unwrap();
        assert_eq!(rt.state_of("body"), Some("wave"));
        // a binding to an unknown bone is rejected when the engine is built
        let mut bad = p.clone();
        bad.bindings[0].target = Target::Prop {
            target: "ghost".into(),
            prop: Prop::BoneAngle,
        };
        assert!(Engine::new(bad)
            .err()
            .unwrap()
            .join("\n")
            .contains("machine/binding targets"));
        // editing helpers
        let mut q = project();
        assert!(add_binding(
            &mut q,
            Binding::new(
                "x",
                "a.b",
                Target::Prop {
                    target: "ghost".into(),
                    prop: Prop::BoneAngle
                }
            )
        )
        .is_err());
        assert!(add_binding(
            &mut q,
            Binding::new(
                "x",
                "a.b",
                Target::Prop {
                    target: "arm".into(),
                    prop: Prop::BoneAngle
                }
            )
        )
        .is_ok());
        assert!(add_binding(
            &mut q,
            Binding::new(
                "x",
                "a.c",
                Target::Prop {
                    target: "arm".into(),
                    prop: Prop::BoneAngle
                }
            )
        )
        .is_err());
        remove_binding(&mut q, "x").unwrap();
        assert!(remove_binding(&mut q, "x").is_err());
        add(&mut q, base_machine()).unwrap();
        assert!(add(&mut q, base_machine()).is_err());
        remove(&mut q, "m").unwrap();
        assert!(remove(&mut q, "m").is_err());
    }
}
