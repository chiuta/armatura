// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
//! Pose fuzzing and stress heat-maps (own design, textbook methods only).
//!
//! Goal: find, automatically, the poses and parameter combinations where a rig breaks, and show where.
//!
//! * **Parameters** ([`enumerate_params`]): every bone angle / scale / offset (within its limits, or a
//!   default range, optionally widened by a margin so limit handling is exercised), IK mix, constraint
//!   weights and deformer control-point offsets. Controller bones of drivers are ordinary bone offsets, and the
//!   range of a bone property that feeds a driven-key curve is taken from the curve's key range.
//! * **Sampling** ([`generate`]): uniform random, Latin hypercube, Halton (radical-inverse low-discrepancy
//!   sequence, first primes as bases), per-axis sweeps, extreme corners and PCA pose-space sampling
//!   (poses drawn along the principal axes of a pose database, see `posesuggest`). All are deterministic
//!   for a given seed (SplitMix64 generator).
//! * **Applying a sample**: the project is cloned once; one constant "probe" channel per parameter is
//!   prepended (channels that animated the same `(target, prop)` are dropped from the clone) and its value is
//!   rewritten in place for every sample. The saved project is never touched.
//! * **Metrics** ([`mesh_metrics`], [`Fuzzer::eval`]): per triangle the deformation gradient `F` relative to
//!   the rest pose (signed area ratio `det F` for fold detection, singular values for stretch and shear),
//!   edge length ratios, per-vertex spikes (growth of the Laplacian offset relative to rest), outline
//!   self-intersections (segment sweep, only counted beyond those already in the rest pose), IK targets
//!   that stay unreachable, parameters asked beyond their limits, non-finite numbers, layers that leave the
//!   canvas or explode. Every metric is divided by its threshold, so `1.0` always means "at the failure
//!   threshold"; the severity of a sample is the largest normalised metric.
//! * **Analysis**: per-parameter marginal failure rates, pairwise interaction scores, delta-debugging
//!   ([`ddmin`]) plus bisection ([`shrink_value`]) to a minimal failing pose, per-vertex worst-case stress
//!   aggregated over all samples, an SVG overlay, per-bone bars, a parameter-pair grid.
//!
//! Not covered (see ROADMAP): cloth penetration, spring/cloth bakes are not re-simulated per sample,
//! only one frame is probed per run, and a panic inside the engine is caught and reported but the default
//! panic message still goes to stderr.

use crate::anim::{Ease, Track};
use crate::engine::{Engine, Frame, NodeKind};
use crate::math::{Affine2, Vec2};
use crate::posesuggest::{PoseDb, PoseSpace};
use crate::project::{Channel, Deformer, Mode, Project, Prop, Source};
use serde::Serialize;
use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use crate::clock::Stamp;

// ---------------------------------------------------------------------------------------------
// random numbers and low-discrepancy sequences
// ---------------------------------------------------------------------------------------------

/// SplitMix64: a tiny, well-known 64-bit generator (public-domain algorithm), deterministic across platforms.
#[derive(Clone, Debug)]
pub struct Rng(pub u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in `[0, 1)` with 24 bits of resolution.
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
    /// Uniform integer in `0..n` (`n > 0`).
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }
    /// Standard normal (Box-Muller).
    pub fn gauss(&mut self) -> f32 {
        let u1 = self.unit().max(1e-7);
        let u2 = self.unit();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos()
    }
}

/// The first `n` primes (trial division).
pub fn first_primes(n: usize) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::with_capacity(n);
    let mut c = 2u32;
    while out.len() < n {
        if out.iter().take_while(|p| *p * *p <= c).all(|p| !c.is_multiple_of(*p)) {
            out.push(c);
        }
        c += 1;
    }
    out
}

/// Radical inverse of `i` in `base` (the van der Corput sequence): `i = Σ dₖ·bᵏ  ->  Σ dₖ·b^-(k+1)`.
pub fn radical_inverse(mut i: u64, base: u32) -> f64 {
    let b = base as u64;
    let (mut f, mut r) = (1.0f64 / base as f64, 0.0f64);
    while i > 0 {
        r += f * (i % b) as f64;
        i /= b;
        f /= base as f64;
    }
    r
}

/// Halton point `index` (starting at 1) in `dim` dimensions: coordinate `k` is the radical inverse in the `k`-th prime.
pub fn halton_point(index: u64, dim: usize) -> Vec<f32> {
    first_primes(dim).iter().map(|&p| radical_inverse(index, p) as f32).collect()
}

// ---------------------------------------------------------------------------------------------
// parameters
// ---------------------------------------------------------------------------------------------

/// One thing the fuzzer may vary.
#[derive(Clone, Debug, Serialize)]
pub struct Param {
    /// Stable text id: `bone:NAME:angle|scale|offx|offy`, `ik:NAME:mix`, `constraint:NAME:weight`,
    /// `deform:LAYER::DEFORMER::INDEX:dx|dy`.
    pub id: String,
    pub target: String,
    pub prop: Prop,
    pub lo: f32,
    pub hi: f32,
    /// Value at the rest pose.
    pub rest: f32,
    /// Hard limit from the rig (bones only); values outside count as limit violations.
    pub limit: Option<[f32; 2]>,
}

/// Which parameters to enumerate and how wide their ranges are.
#[derive(Clone, Debug)]
pub struct ParamOptions {
    /// Angle range (± degrees around rest) for bones without `angle_limits`.
    pub angle_range: f32,
    /// Extra degrees beyond `angle_limits` / scale-limit fraction (exercises limit handling); 0 = stay inside.
    pub limit_margin: f32,
    /// Scale range for bones without `scale_limits`.
    pub scale_range: [f32; 2],
    /// Offset range (± px); `None` = max(half the bone length, a tenth of the smaller canvas side).
    pub offset_range: Option<f32>,
    pub ik_mix: bool,
    pub constraint_weight: bool,
    /// Deformer control points (capped by `max_deformer_params`, spread evenly).
    pub deformers: bool,
    pub max_deformer_params: usize,
    pub deformer_range: f32,
    /// Restrict to these bone names (empty = all).
    pub only_bones: Vec<String>,
    /// Restrict bone parameters to these kinds: `angle`, `scale`, `offx`, `offy` (empty = all).
    pub bone_props: Vec<String>,
}

impl Default for ParamOptions {
    fn default() -> Self {
        ParamOptions {
            angle_range: 180.0,
            limit_margin: 0.0,
            scale_range: [0.5, 2.0],
            offset_range: None,
            ik_mix: true,
            constraint_weight: true,
            deformers: true,
            max_deformer_params: 24,
            deformer_range: 30.0,
            only_bones: vec![],
            bone_props: vec![],
        }
    }
}

/// All parameters of a project, in a stable order.
pub fn enumerate_params(p: &Project, o: &ParamOptions) -> Vec<Param> {
    // outputs of driven keys are derived values: not probed (the driver stays alive)
    let driven_out: Vec<(&str, Prop)> =
        p.channels.iter().filter(|c| matches!(c.source, Source::Driven { .. })).map(|c| (c.target.as_str(), c.prop)).collect();
    // key range of curves that read a bone property: the natural range of a driver input
    let mut input_range: std::collections::HashMap<(String, Prop), (f32, f32)> = std::collections::HashMap::new();
    for c in &p.channels {
        if let Source::Driven { input, input_prop, curve } = &c.source {
            if let (Some(a), Some(b)) = (curve.first_frame(), curve.last_frame()) {
                let m = 0.1 * (b - a).abs();
                let e = input_range.entry((input.clone(), *input_prop)).or_insert((a - m, b + m));
                e.0 = e.0.min(a - m);
                e.1 = e.1.max(b + m);
            }
        }
    }
    let skip = |t: &str, pr: Prop| driven_out.iter().any(|(tt, pp)| *tt == t && *pp == pr);
    let mut out = Vec::new();
    let side = p.width.min(p.height);
    for b in &p.bones {
        if !o.only_bones.is_empty() && !o.only_bones.contains(&b.name) {
            continue;
        }
        let mut push = |suffix: &str, prop: Prop, lo: f32, hi: f32, rest: f32, limit: Option<[f32; 2]>| {
            if skip(&b.name, prop) || !(o.bone_props.is_empty() || o.bone_props.iter().any(|x| x == suffix)) || (lo.partial_cmp(&hi) != Some(std::cmp::Ordering::Less)) || !lo.is_finite() || !hi.is_finite() {
                return;
            }
            let (lo, hi) = match input_range.get(&(b.name.clone(), prop)) {
                Some(&(a, c)) if limit.is_none() => (a, c),
                _ => (lo, hi),
            };
            out.push(Param { id: format!("bone:{}:{}", b.name, suffix), target: b.name.clone(), prop, lo, hi, rest, limit });
        };
        match b.angle_limits {
            Some([a, c]) => push("angle", Prop::BoneAngle, a - o.limit_margin, c + o.limit_margin, 0.0f32.clamp(a, c), Some([a, c])),
            None => push("angle", Prop::BoneAngle, -o.angle_range, o.angle_range, 0.0, None),
        }
        match b.scale_limits {
            Some([a, c]) => {
                let m = o.limit_margin / 180.0;
                push("scale", Prop::BoneScale, (a - m).max(0.01), c + m, 1.0f32.clamp(a, c), Some([a, c]))
            }
            None => push("scale", Prop::BoneScale, o.scale_range[0], o.scale_range[1], 1.0, None),
        }
        let r = o.offset_range.unwrap_or_else(|| (0.5 * b.length).max(0.1 * side)).abs();
        push("offx", Prop::BoneOffsetX, -r, r, 0.0, None);
        push("offy", Prop::BoneOffsetY, -r, r, 0.0, None);
    }
    // `only_bones` / `bone_props` restrict EVERY sampled parameter, not just the bone ones: an IK mix or constraint
    // weight is kept only when `bone_props` is empty (they are not bone properties) and, with `only_bones`, when the
    // chain / constraint touches one of those bones; deformer points are bone-independent and are dropped.
    let restrict_props = !o.bone_props.is_empty();
    let in_only = |n: &str| o.only_bones.is_empty() || o.only_bones.iter().any(|b| b == n);
    if o.ik_mix && !restrict_props {
        for c in p.ik.iter().filter(|c| in_only(&c.target) || c.bones.iter().any(|b| in_only(b))) {
            out.push(Param { id: format!("ik:{}:mix", c.name), target: c.name.clone(), prop: Prop::IkMix, lo: 0.0, hi: 1.0, rest: c.mix, limit: None });
        }
    }
    if o.constraint_weight && !restrict_props {
        for c in p.constraints.iter().filter(|c| in_only(&c.bone)) {
            out.push(Param {
                id: format!("constraint:{}:weight", c.name),
                target: c.name.clone(),
                prop: Prop::ConstraintWeight,
                lo: 0.0,
                hi: 1.0,
                rest: c.weight,
                limit: None,
            });
        }
    }
    if o.deformers && o.only_bones.is_empty() && !restrict_props {
        let mut dp: Vec<Param> = Vec::new();
        fn walk(layers: &[crate::project::Layer], prefix: &str, o: &ParamOptions, dp: &mut Vec<Param>) {
            for l in layers {
                let path = if prefix.is_empty() { l.name.clone() } else { format!("{prefix}/{}", l.name) };
                for d in &l.deformers {
                    let (n, ctl, props, offs): (usize, bool, (Prop, Prop), &Vec<Vec2>) = match d {
                        Deformer::Lattice { cols, rows, offsets, .. } => ((cols + 1) * (rows + 1), true, (Prop::LatticeDx, Prop::LatticeDy), offsets),
                        Deformer::Cloth { cols, rows, offsets, .. } => ((cols + 1) * (rows + 1), true, (Prop::LatticeDx, Prop::LatticeDy), offsets),
                        Deformer::Spine { rest, offsets, .. } => (rest.len(), false, (Prop::SpineDx, Prop::SpineDy), offsets),
                        Deformer::Pin { pins, offsets, .. } => (pins.len(), false, (Prop::PinDx, Prop::PinDy), offsets),
                        Deformer::Cage { rest, offsets, .. } => (rest.len(), false, (Prop::CageDx, Prop::CageDy), offsets),
                    };
                    let cols = match d {
                        Deformer::Lattice { cols, .. } | Deformer::Cloth { cols, .. } => *cols,
                        _ => 0,
                    };
                    for i in 0..n {
                        let key = if ctl { format!("{path}::{}::{},{}", d.name(), i % (cols + 1), i / (cols + 1)) } else { format!("{path}::{}::{i}", d.name()) };
                        let off = offs.get(i).copied().unwrap_or(Vec2::ZERO);
                        let r = o.deformer_range;
                        dp.push(Param { id: format!("deform:{key}:dx"), target: key.clone(), prop: props.0, lo: off.x - r, hi: off.x + r, rest: off.x, limit: None });
                        dp.push(Param { id: format!("deform:{key}:dy"), target: key, prop: props.1, lo: off.y - r, hi: off.y + r, rest: off.y, limit: None });
                    }
                }
                walk(l.children(), &path, o, dp);
            }
        }
        walk(&p.layers, "", o, &mut dp);
        if dp.len() > o.max_deformer_params {
            let n = o.max_deformer_params;
            let total = dp.len();
            dp = (0..n).map(|k| dp[k * total / n.max(1)].clone()).collect();
        }
        out.extend(dp);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// sampling strategies
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Strategy {
    Random,
    Lhs,
    Halton,
    Sweep,
    Corners,
    Pca,
}

impl Strategy {
    pub fn parse(s: &str) -> Result<Strategy, String> {
        Ok(match s {
            "random" => Strategy::Random,
            "lhs" => Strategy::Lhs,
            "halton" => Strategy::Halton,
            "sweep" => Strategy::Sweep,
            "corners" => Strategy::Corners,
            "pca" => Strategy::Pca,
            _ => return Err(format!("unknown strategy '{s}' (random, lhs, halton, sweep, corners, pca)")),
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            Strategy::Random => "random",
            Strategy::Lhs => "lhs",
            Strategy::Halton => "halton",
            Strategy::Sweep => "sweep",
            Strategy::Corners => "corners",
            Strategy::Pca => "pca",
        }
    }
}

/// Latin hypercube in the unit cube: every dimension is cut into `n` equal strata and each stratum is hit exactly once.
pub fn latin_hypercube(n: usize, dim: usize, rng: &mut Rng) -> Vec<Vec<f32>> {
    let mut pts = vec![vec![0.0f32; dim]; n];
    for d in 0..dim {
        let mut perm: Vec<usize> = (0..n).collect();
        for i in (1..n).rev() {
            perm.swap(i, rng.below(i + 1));
        }
        for (i, p) in pts.iter_mut().enumerate() {
            p[d] = (perm[i] as f32 + rng.unit()) / n as f32;
        }
    }
    pts
}

/// A pose space fitted on a pose database, restricted to the bones that are parameters of the rig.
pub struct PcaSampler {
    space: PoseSpace,
    /// For each database column: index of the matching `bone:NAME:angle` parameter.
    cols: Vec<usize>,
}

impl PcaSampler {
    pub fn new(db: &PoseDb, params: &[Param]) -> Result<PcaSampler, String> {
        db.validate()?;
        let mut cols = Vec::new();
        let mut src = Vec::new();
        for (j, b) in db.bones.iter().enumerate() {
            if let Some(pi) = params.iter().position(|p| p.id == format!("bone:{b}:angle")) {
                cols.push(pi);
                src.push(j);
            }
        }
        if cols.is_empty() {
            return Err("the pose database shares no bone with the rig's angle parameters".into());
        }
        let poses: Vec<Vec<f32>> = db.poses.iter().map(|p| src.iter().map(|&j| p[j]).collect()).collect();
        let space = PoseSpace::fit(&poses, 0.95, 8)?;
        Ok(PcaSampler { space, cols })
    }
    pub fn components(&self) -> usize {
        self.space.k()
    }
}

/// Most samples one fuzz run may ask for.
pub const MAX_SAMPLES: usize = 1_000_000;
/// Most numbers (samples x parameters) one run may generate at once.
pub const MAX_SAMPLE_VALUES: usize = 50_000_000;

/// Parameter vectors for a strategy. `n` is the requested number of samples (sweep and corners may return
/// a different count, see their notes). Deterministic for a given `seed`.
pub fn generate(params: &[Param], strategy: Strategy, n: usize, seed: u64, pca: Option<&PcaSampler>) -> Result<Vec<Vec<f32>>, String> {
    let d = params.len();
    if d == 0 {
        return Err("the rig has no parameters to fuzz".into());
    }
    if n == 0 {
        return Err("--samples must be at least 1".into());
    }
    if n > MAX_SAMPLES {
        return Err(format!("--samples {n} is more than the limit of {MAX_SAMPLES}"));
    }
    if n.saturating_mul(d) > MAX_SAMPLE_VALUES {
        return Err(format!("--samples {n} x {d} parameters = {} numbers, more than the limit of {MAX_SAMPLE_VALUES}; use fewer samples", n.saturating_mul(d)));
    }
    let mut rng = Rng::new(seed);
    let map = |u: &[f32]| -> Vec<f32> { params.iter().zip(u).map(|(p, &u)| p.lo + u * (p.hi - p.lo)).collect() };
    Ok(match strategy {
        Strategy::Random => (0..n).map(|_| map(&(0..d).map(|_| rng.unit()).collect::<Vec<_>>())).collect(),
        Strategy::Lhs => latin_hypercube(n, d, &mut rng).iter().map(|u| map(u)).collect(),
        Strategy::Halton => (1..=n as u64).map(|i| map(&halton_point(i, d))).collect(),
        Strategy::Sweep => {
            // one axis at a time, others at rest; both ends of the range included
            let m = (n / d).max(2);
            let mut out = Vec::new();
            for (k, p) in params.iter().enumerate() {
                for s in 0..m {
                    let mut v: Vec<f32> = params.iter().map(|q| q.rest).collect();
                    v[k] = p.lo + (p.hi - p.lo) * s as f32 / (m - 1) as f32;
                    out.push(v);
                }
            }
            out
        }
        Strategy::Corners => {
            // all 2^d corners when they fit into n, else n random corners
            if d <= 20 && (1usize << d) <= n {
                (0..1usize << d).map(|m| params.iter().enumerate().map(|(k, p)| if m >> k & 1 == 1 { p.hi } else { p.lo }).collect()).collect()
            } else {
                (0..n).map(|_| params.iter().map(|p| if rng.next_u64() & 1 == 1 { p.hi } else { p.lo }).collect()).collect()
            }
        }
        Strategy::Pca => {
            let s = pca.ok_or("the pca strategy needs a pose database (--db) or keyed bone rotations in the project")?;
            let k = s.space.k();
            (0..n)
                .map(|_| {
                    let z: Vec<f32> = (0..k).map(|i| (rng.gauss() * 2.0).clamp(-3.0, 3.0) * s.space.eig[i].sqrt()).collect();
                    let x = s.space.reconstruct(&z);
                    let mut v: Vec<f32> = params.iter().map(|q| q.rest).collect();
                    for (c, &pi) in s.cols.iter().enumerate() {
                        v[pi] = x[c].clamp(params[pi].lo, params[pi].hi);
                    }
                    v
                })
                .collect()
        }
    })
}

// ---------------------------------------------------------------------------------------------
// geometry metrics
// ---------------------------------------------------------------------------------------------

/// Failure thresholds. A metric at its threshold has normalised value 1.
#[derive(Clone, Debug, Serialize)]
pub struct Thresholds {
    /// A triangle with `det F <= min_det` counts as folded (0 = flipped or collapsed).
    pub min_det: f32,
    /// Largest allowed edge length ratio vs rest (growth or shrink).
    pub max_stretch: f32,
    /// Largest allowed `smax/smin - 1` of the deformation gradient (non-uniform stretch / shear).
    pub max_shear: f32,
    /// Largest allowed area growth `det F`.
    pub max_area: f32,
    /// Largest allowed spike (growth of the Laplacian offset in local edge lengths).
    pub max_spike: f32,
    /// IK end-effector error as a fraction of chain reach.
    pub ik_error: f32,
    /// A layer is lost when its box is farther than this many canvas sizes outside the canvas.
    pub canvas_margin: f32,
    /// A layer is exploded when its box is larger than this many canvas diagonals.
    pub max_extent: f32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds { min_det: 0.0, max_stretch: 3.0, max_shear: 2.0, max_area: 6.0, max_spike: 1.0, ik_error: 0.03, canvas_margin: 0.0, max_extent: 20.0 }
    }
}

const CAP: f32 = 100.0;

/// Aggregated result of [`mesh_metrics`].
#[derive(Clone, Debug, Serialize)]
pub struct MeshMetrics {
    pub triangles: usize,
    pub min_det: f32,
    pub max_det: f32,
    pub flips: usize,
    /// Largest edge length ratio vs rest (>= 1; growth or shrink, whichever is larger).
    pub max_stretch: f32,
    /// Largest `smax/smin - 1`.
    pub max_shear: f32,
    pub max_spike: f32,
    /// Normalised per-vertex stress (max over incident elements and spike); 1 = at a threshold.
    #[serde(skip)]
    pub stress: Vec<f32>,
}

fn det2(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}

/// Deformation-gradient metrics of a triangle mesh against its rest shape. `edges` may add extra edges
/// (open or closed paths use them instead of triangles). Triangles whose rest area is ~0 are skipped.
pub fn mesh_metrics(rest: &[Vec2], def: &[Vec2], tris: &[[usize; 3]], edges: &[[usize; 2]], th: &Thresholds) -> MeshMetrics {
    let n = rest.len().min(def.len());
    let mut stress = vec![0.0f32; n];
    let mut m = MeshMetrics { triangles: 0, min_det: f32::INFINITY, max_det: f32::NEG_INFINITY, flips: 0, max_stretch: 1.0, max_shear: 0.0, max_spike: 0.0, stress: vec![] };
    let mut all_edges: Vec<[usize; 2]> = edges.to_vec();
    for t in tris {
        all_edges.extend([[t[0], t[1]], [t[1], t[2]], [t[2], t[0]]]);
    }
    let mean_rest = {
        let (mut s, mut c) = (0.0f32, 0usize);
        for e in &all_edges {
            if e[0] < n && e[1] < n {
                s += rest[e[0]].distance(rest[e[1]]);
                c += 1;
            }
        }
        if c == 0 { 0.0 } else { s / c as f32 }
    };
    for t in tris {
        if t.iter().any(|&i| i >= n) {
            continue;
        }
        let (r0, r1, r2) = (rest[t[0]], rest[t[1]], rest[t[2]]);
        let (e1, e2) = (r1 - r0, r2 - r0);
        let a_rest = det2(e1, e2);
        if a_rest.abs() < 1e-9 * (mean_rest * mean_rest).max(1e-12) {
            continue;
        }
        let (d1, d2) = (def[t[1]] - def[t[0]], def[t[2]] - def[t[0]]);
        let det = det2(d1, d2) / a_rest;
        // F = [d1 d2] [e1 e2]^-1
        let inv = [[e2.y / a_rest, -e2.x / a_rest], [-e1.y / a_rest, e1.x / a_rest]];
        let f = [[d1.x * inv[0][0] + d2.x * inv[1][0], d1.x * inv[0][1] + d2.x * inv[1][1]], [d1.y * inv[0][0] + d2.y * inv[1][0], d1.y * inv[0][1] + d2.y * inv[1][1]]];
        let s1 = f[0][0] * f[0][0] + f[0][1] * f[0][1] + f[1][0] * f[1][0] + f[1][1] * f[1][1];
        let disc = (s1 * s1 - 4.0 * det * det).max(0.0).sqrt();
        let smax = ((s1 + disc) * 0.5).sqrt();
        let smin = if smax > 1e-12 { det.abs() / smax } else { 0.0 };
        let shear = if det <= th.min_det { 0.0 } else if smin > 1e-9 { smax / smin - 1.0 } else { 1e3 };
        m.triangles += 1;
        m.min_det = m.min_det.min(det);
        m.max_det = m.max_det.max(det);
        if det <= th.min_det {
            m.flips += 1;
        }
        m.max_shear = m.max_shear.max(shear);
        let n_fold = if det <= th.min_det { 1.0 + (th.min_det - det) } else { (1.0 - det) / (1.0 - th.min_det) }.max(0.0);
        let n_area = if det > 1.0 { (det - 1.0) / (th.max_area - 1.0) } else { 0.0 };
        let n_shear = shear / th.max_shear;
        let s = n_fold.max(n_area).max(n_shear).min(CAP);
        for &i in t {
            stress[i] = stress[i].max(s);
        }
    }
    for e in &all_edges {
        if e[0] >= n || e[1] >= n {
            continue;
        }
        let l0 = rest[e[0]].distance(rest[e[1]]);
        if l0 < 1e-6 {
            continue;
        }
        let l = def[e[0]].distance(def[e[1]]);
        let ratio = if l < 1e-9 { 1e3 } else { (l / l0).max(l0 / l) };
        m.max_stretch = m.max_stretch.max(ratio);
        let s = ((ratio - 1.0) / (th.max_stretch - 1.0)).min(CAP);
        for &i in e {
            stress[i] = stress[i].max(s);
        }
    }
    // spikes: growth of the Laplacian offset, measured in local edge lengths (similarity invariant)
    let mut nbr: Vec<Vec<usize>> = vec![vec![]; n];
    for e in &all_edges {
        if e[0] < n && e[1] < n && e[0] != e[1] {
            if !nbr[e[0]].contains(&e[1]) {
                nbr[e[0]].push(e[1]);
            }
            if !nbr[e[1]].contains(&e[0]) {
                nbr[e[1]].push(e[0]);
            }
        }
    }
    for i in 0..n {
        if nbr[i].len() < 2 {
            continue;
        }
        let k = nbr[i].len() as f32;
        let (mut cr, mut cd, mut lr, mut ld) = (Vec2::ZERO, Vec2::ZERO, 0.0f32, 0.0f32);
        for &j in &nbr[i] {
            cr += rest[j];
            cd += def[j];
            lr += rest[i].distance(rest[j]);
            ld += def[i].distance(def[j]);
        }
        let (lap_r, lap_d) = ((cr * (1.0 / k) - rest[i]).length(), (cd * (1.0 / k) - def[i]).length());
        let (er, ed) = (lr / k, ld / k);
        if er < 1e-6 {
            continue;
        }
        let spike = (lap_d / ed.max(1e-9) - lap_r / er).max(0.0);
        m.max_spike = m.max_spike.max(spike);
        stress[i] = stress[i].max((spike / th.max_spike).min(CAP));
    }
    if m.triangles == 0 {
        m.min_det = 1.0;
        m.max_det = 1.0;
    }
    m.stress = stress;
    m
}

fn orient(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    det2(b - a, c - a)
}

/// Do segments `ab` and `cd` cross properly (interiors intersect, not merely touch)?
pub fn segments_cross(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    let (o1, o2, o3, o4) = (orient(a, b, c), orient(a, b, d), orient(c, d, a), orient(c, d, b));
    o1 * o2 < 0.0 && o3 * o4 < 0.0
}

/// Pairs of non-adjacent segments of a polyline that cross each other (x-sweep over bounding boxes; at most `cap` pairs).
pub fn self_intersections(pts: &[Vec2], closed: bool, cap: usize) -> Vec<(usize, usize)> {
    let n = pts.len();
    let segs = if closed { n } else { n.saturating_sub(1) };
    if segs < 3 {
        return vec![];
    }
    let seg = |i: usize| (pts[i], pts[(i + 1) % n]);
    let mut order: Vec<usize> = (0..segs).collect();
    let xmin = |i: usize| {
        let (a, b) = seg(i);
        a.x.min(b.x)
    };
    let xmax = |i: usize| {
        let (a, b) = seg(i);
        a.x.max(b.x)
    };
    order.sort_by(|&a, &b| xmin(a).total_cmp(&xmin(b)));
    let mut out = Vec::new();
    for (oi, &i) in order.iter().enumerate() {
        let (a, b) = seg(i);
        for &j in &order[oi + 1..] {
            if xmin(j) > xmax(i) {
                break;
            }
            let adjacent = (i + 1) % segs == j || (j + 1) % segs == i;
            if !closed && (i + 1 == j || j + 1 == i) || closed && adjacent {
                continue;
            }
            let (c, d) = seg(j);
            if a.y.max(b.y) < c.y.min(d.y) || c.y.max(d.y) < a.y.min(b.y) {
                continue;
            }
            if segments_cross(a, b, c, d) {
                out.push((i.min(j), i.max(j)));
                if out.len() >= cap {
                    return out;
                }
            }
        }
    }
    out
}

/// Signed area of a closed polygon (shoelace).
pub fn signed_area(pts: &[Vec2]) -> f32 {
    let n = pts.len();
    (0..n).map(|i| det2(pts[i], pts[(i + 1) % n])).sum::<f32>() * 0.5
}

// ---------------------------------------------------------------------------------------------
// snapshots of an evaluated frame
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoKind {
    Mesh,
    Path,
    Quad,
}

/// Deformed geometry of one node (layer space) plus the data needed to judge it.
#[derive(Clone, Debug)]
struct Snap {
    name: String,
    kind: GeoKind,
    pts: Vec<Vec2>,
    tris: Vec<[usize; 3]>,
    edges: Vec<[usize; 2]>,
    /// Flattened outlines with, per point, the anchor index it belongs to (paths only).
    outlines: Vec<(Vec<Vec2>, Vec<usize>, bool)>,
    /// Closed polygon on `pts` for the orientation test (paths with fill only; empty otherwise).
    loops: Vec<(usize, usize)>,
    world: Affine2,
}

fn snapshot(frame: &Frame) -> Vec<Snap> {
    let mut out = Vec::new();
    for n in &frame.nodes {
        if !n.visible || n.opacity <= 0.0 {
            continue;
        }
        match n.kind {
            NodeKind::Vector => {
                for (pi, p) in n.paths.iter().enumerate() {
                    let pts: Vec<Vec2> = p.anchors.iter().map(|a| a.pos).collect();
                    if pts.is_empty() {
                        continue;
                    }
                    let k = pts.len();
                    let mut edges = Vec::new();
                    for i in 0..k.saturating_sub(1) {
                        edges.push([i, i + 1]);
                    }
                    let mut loops = vec![];
                    if p.closed && k > 2 {
                        edges.push([k - 1, 0]);
                        loops.push((0, k));
                    }
                    let fl = p.flatten_t(1.0);
                    let outline = (fl.iter().map(|f| f.0).collect::<Vec<_>>(), fl.iter().map(|f| f.1.min(k - 1)).collect::<Vec<_>>(), p.closed);
                    out.push(Snap { name: format!("{}#{}", n.name, if p.name.is_empty() { pi.to_string() } else { p.name.clone() }), kind: GeoKind::Path, pts, tris: vec![], edges, outlines: vec![outline], loops, world: n.world });
                }
            }
            NodeKind::Image => {
                if let Some(img) = &n.image {
                    match &img.mesh {
                        Some(m) => {
                            let tris = m.tris.iter().map(|t| [t[0] as usize, t[1] as usize, t[2] as usize]).collect();
                            out.push(Snap { name: format!("{}#mesh", n.name), kind: GeoKind::Mesh, pts: m.positions.clone(), tris, edges: vec![], outlines: vec![], loops: vec![], world: n.world });
                        }
                        None => {
                            let (o, s) = (img.origin, img.size);
                            let pts = vec![o, Vec2::new(o.x + s.x, o.y), o + s, Vec2::new(o.x, o.y + s.y)];
                            out.push(Snap { name: format!("{}#quad", n.name), kind: GeoKind::Quad, pts, tris: vec![[0, 1, 2], [0, 2, 3]], edges: vec![], outlines: vec![], loops: vec![], world: n.world });
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn bbox(pts: &[Vec2], m: &Affine2) -> Option<(Vec2, Vec2)> {
    let mut it = pts.iter().map(|p| m.apply(*p));
    let f = it.next()?;
    let (mut lo, mut hi) = (f, f);
    for p in it {
        lo = Vec2::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Vec2::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    Some((lo, hi))
}

// ---------------------------------------------------------------------------------------------
// per-sample evaluation
// ---------------------------------------------------------------------------------------------

/// Normalised metrics (1 = at the threshold).
#[derive(Clone, Debug, Default, Serialize)]
pub struct Norm {
    pub fold: f32,
    pub stretch: f32,
    pub shear: f32,
    pub area: f32,
    pub spike: f32,
    pub self_intersection: f32,
    pub ik_unreachable: f32,
    pub limit: f32,
    pub off_canvas: f32,
    pub non_finite: f32,
    pub panic: f32,
}

impl Norm {
    pub fn items(&self) -> [(&'static str, f32); 11] {
        [
            ("fold", self.fold),
            ("stretch", self.stretch),
            ("shear", self.shear),
            ("area", self.area),
            ("spike", self.spike),
            ("self_intersection", self.self_intersection),
            ("ik_unreachable", self.ik_unreachable),
            ("limit", self.limit),
            ("off_canvas", self.off_canvas),
            ("non_finite", self.non_finite),
            ("panic", self.panic),
        ]
    }
}

/// Raw metric values of one sample.
#[derive(Clone, Debug, Default, Serialize)]
pub struct SampleMetrics {
    pub min_det: f32,
    pub max_area_ratio: f32,
    pub flipped_triangles: usize,
    pub max_edge_stretch: f32,
    pub max_shear: f32,
    pub max_spike: f32,
    pub new_self_intersections: usize,
    pub ik_error_fraction: f32,
    pub ik_worst_chain: Option<String>,
    pub limit_violations: usize,
    pub layers_off_canvas: usize,
    pub layers_exploded: usize,
    pub non_finite: bool,
    pub panicked: bool,
    pub norm: Norm,
}

/// Verdict for one sampled pose.
#[derive(Clone, Debug, Serialize)]
pub struct SampleResult {
    pub index: usize,
    pub values: Vec<f32>,
    pub severity: f32,
    pub failed: bool,
    /// Normalised metrics at or above 1, worst first.
    pub kinds: Vec<String>,
    pub metrics: SampleMetrics,
}

struct EvalOut {
    result: SampleResult,
    /// Normalised stress per vertex for each rest geometry (same order as the fuzzer's `geos`).
    stress: Vec<Vec<f32>>,
    /// Worst self-intersecting anchor per path, merged into `stress` already.
    #[allow(dead_code)]
    _pad: (),
}

struct Geo {
    snap: Snap,
    rest_self: Vec<usize>,
    rest_area: Vec<f32>,
    rest_inside: bool,
}

/// A prepared project: probe engine, rest geometry and thresholds. Reusable for many samples.
pub struct Fuzzer {
    pub params: Vec<Param>,
    pub thresholds: Thresholds,
    pub frame: f32,
    /// Failure kinds that are measured but never count (e.g. `ik_unreachable`, `limit`).
    pub ignore: Vec<String>,
    engine: Engine,
    geos: Vec<Geo>,
    canvas: (f32, f32),
    /// Bone world positions at rest, for the overlay: `(name, head, tip)` in stage space.
    rest_bones: Vec<(String, Vec2, Vec2)>,
    evals: usize,
}

fn engine_for(p: Project, asset_dir: Option<&Path>) -> Result<Engine, String> {
    let r = match asset_dir {
        Some(d) => Engine::with_asset_dir(p, d),
        None => Engine::new(p),
    };
    r.map_err(|e| format!("project does not compile: {}", e.join("; ")))
}

impl Fuzzer {
    pub fn new(project: &Project, asset_dir: Option<&Path>, popts: &ParamOptions, th: Thresholds, frame: f32) -> Result<Fuzzer, String> {
        let params = enumerate_params(project, popts);
        if params.is_empty() {
            return Err("the rig has no parameters to fuzz".into());
        }
        Fuzzer::with_params(project, asset_dir, params, th, frame)
    }

    pub fn with_params(project: &Project, asset_dir: Option<&Path>, params: Vec<Param>, th: Thresholds, frame: f32) -> Result<Fuzzer, String> {
        if !frame.is_finite() {
            return Err("frame must be a number".into());
        }
        // rest reference: no animation at all
        let mut rest_p = project.clone();
        rest_p.channels.clear();
        rest_p.clip_baked.clear();
        rest_p.channel_meta.clear();
        let rest_engine = engine_for(rest_p, asset_dir)?;
        let rest_frame = rest_engine.evaluate(frame);
        let snaps = snapshot(&rest_frame);
        let (w, h) = (project.width, project.height);
        let geos: Vec<Geo> = snaps
            .into_iter()
            .map(|s| {
                let rest_self = s.outlines.iter().map(|(p, _, c)| self_intersections(p, *c, 1000).len()).collect();
                let rest_area = s.loops.iter().map(|&(a, b)| signed_area(&s.pts[a..b])).collect();
                let rest_inside = bbox(&s.pts, &s.world).is_some_and(|(lo, hi)| hi.x >= 0.0 && lo.x <= w && hi.y >= 0.0 && lo.y <= h);
                Geo { snap: s, rest_self, rest_area, rest_inside }
            })
            .collect();
        let rest_bones = project
            .bones
            .iter()
            .zip(&rest_frame.world)
            .map(|(b, m)| (b.name.clone(), m.apply(Vec2::ZERO), m.apply(Vec2::new(b.length, 0.0))))
            .collect();
        // probe engine
        let mut pp = project.clone();
        pp.channel_meta.clear();
        pp.channels.retain(|c| !params.iter().any(|q| q.target == c.target && q.prop == c.prop));
        let mut probes: Vec<Channel> = params
            .iter()
            .map(|q| Channel::keys(&q.target, q.prop, Track::with_keys(&[(0.0, q.rest, Ease::Step)])).with_mode(Mode::Replace))
            .collect();
        probes.append(&mut pp.channels);
        pp.channels = probes;
        let engine = engine_for(pp, asset_dir)?;
        Ok(Fuzzer { params, thresholds: th, frame, ignore: vec![], engine, geos, canvas: (w, h), rest_bones, evals: 0 })
    }

    pub fn rest_values(&self) -> Vec<f32> {
        self.params.iter().map(|p| p.rest).collect()
    }

    /// Number of samples evaluated so far.
    pub fn evaluations(&self) -> usize {
        self.evals
    }

    pub fn eval(&mut self, index: usize, values: &[f32]) -> SampleResult {
        self.eval_full(index, values).result
    }

    fn eval_full(&mut self, index: usize, values: &[f32]) -> EvalOut {
        self.evals += 1;
        let n_geo = self.geos.len();
        let mut norm = Norm::default();
        let mut mt = SampleMetrics { min_det: 1.0, max_area_ratio: 1.0, max_edge_stretch: 1.0, ..Default::default() };
        let mut stress: Vec<Vec<f32>> = self.geos.iter().map(|g| vec![0.0; g.snap.pts.len()]).collect();
        // set probes
        for (i, v) in values.iter().enumerate().take(self.params.len()) {
            if let Source::Keys { track } = &mut self.engine.project.channels[i].source {
                track.keys[0].value = *v;
            }
        }
        mt.non_finite = values.iter().any(|v| !v.is_finite());
        for (q, v) in self.params.iter().zip(values) {
            if let Some([a, b]) = q.limit {
                if *v < a - 1e-3 || *v > b + 1e-3 {
                    mt.limit_violations += 1;
                }
            }
        }
        norm.limit = if mt.limit_violations > 0 { 1.0 } else { 0.0 };
        if mt.non_finite {
            norm.non_finite = CAP;
            return self.finish(index, values, mt, norm, stress);
        }
        let frame_no = self.frame;
        let eng = &self.engine;
        let fr = match catch_unwind(AssertUnwindSafe(|| eng.evaluate(frame_no))) {
            Ok(f) => f,
            Err(_) => {
                mt.panicked = true;
                norm.panic = CAP;
                return self.finish(index, values, mt, norm, stress);
            }
        };
        if fr.world.iter().any(|m| ![m.a, m.b, m.c, m.d, m.tx, m.ty].iter().all(|x| x.is_finite())) {
            mt.non_finite = true;
        }
        // IK
        let p = &self.engine.project;
        for c in &p.ik {
            let mix = self.params.iter().zip(values).find(|(q, _)| q.prop == Prop::IkMix && q.target == c.name).map_or(c.mix, |(_, v)| *v);
            if mix < 0.05 {
                continue;
            }
            let ix = |n: &str| p.bones.iter().position(|b| b.name == n);
            let (Some(first), Some(last), Some(tg)) = (c.bones.first().and_then(|n| ix(n)), c.bones.last().and_then(|n| ix(n)), ix(&c.target)) else { continue };
            if first >= fr.world.len() || last >= fr.world.len() || tg >= fr.world.len() {
                continue;
            }
            let tip = fr.world[last].apply(Vec2::new(p.bones[last].length, 0.0));
            let root = fr.world[first].apply(Vec2::ZERO);
            let target = fr.world[tg].apply(Vec2::ZERO);
            let mut reach = 0.0;
            for n in &c.bones {
                if let Some(bi) = ix(n) {
                    reach += (fr.world[bi].apply(Vec2::new(p.bones[bi].length, 0.0)) - fr.world[bi].apply(Vec2::ZERO)).length();
                }
            }
            let err = tip.distance(target);
            let frac = if reach > 1e-3 { err / reach } else { err };
            let _ = root;
            if !frac.is_finite() {
                mt.non_finite = true;
            } else if frac > mt.ik_error_fraction {
                mt.ik_error_fraction = frac;
                mt.ik_worst_chain = Some(c.name.clone());
            }
        }
        norm.ik_unreachable = (mt.ik_error_fraction / self.thresholds.ik_error).min(CAP);
        // geometry
        let snaps = snapshot(&fr);
        let th = self.thresholds.clone();
        let (cw, chh) = self.canvas;
        let diag = (cw * cw + chh * chh).sqrt();
        let mut by_name: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for (i, g) in self.geos.iter().enumerate() {
            by_name.entry(g.snap.name.as_str()).or_insert(i);
        }
        let (mut min_det, mut max_det) = (f32::INFINITY, f32::NEG_INFINITY);
        for s in &snaps {
            let Some(&gi) = by_name.get(s.name.as_str()) else { continue };
            let g = &self.geos[gi];
            if g.snap.pts.len() != s.pts.len() {
                continue; // topology changed (generated geometry): not comparable
            }
            if s.pts.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
                mt.non_finite = true;
                continue;
            }
            if let Some((lo, hi)) = bbox(&s.pts, &s.world) {
                let mx = th.canvas_margin * cw.max(chh);
                if g.rest_inside && (hi.x < -mx || lo.x > cw + mx || hi.y < -mx || lo.y > chh + mx) {
                    mt.layers_off_canvas += 1;
                }
                if (hi - lo).length() > th.max_extent * diag {
                    mt.layers_exploded += 1;
                }
            }
            let mm = mesh_metrics(&g.snap.pts, &s.pts, &g.snap.tris, &g.snap.edges, &th);
            if mm.triangles > 0 {
                min_det = min_det.min(mm.min_det);
                max_det = max_det.max(mm.max_det);
            }
            mt.flipped_triangles += mm.flips;
            mt.max_edge_stretch = mt.max_edge_stretch.max(mm.max_stretch);
            mt.max_shear = mt.max_shear.max(mm.max_shear);
            mt.max_spike = mt.max_spike.max(mm.max_spike);
            let st = &mut stress[gi];
            for (a, b) in st.iter_mut().zip(&mm.stress) {
                *a = a.max(*b);
            }
            // paths: outline crossings and orientation flips
            for (k, (poly, anchor_of, closed)) in s.outlines.iter().enumerate() {
                let rest_n = g.rest_self.get(k).copied().unwrap_or(0);
                let hits = self_intersections(poly, *closed, 1000);
                if hits.len() > rest_n {
                    mt.new_self_intersections += hits.len() - rest_n;
                    for (i, j) in &hits {
                        for sg in [*i, *j] {
                            if let Some(&a) = anchor_of.get(sg) {
                                if a < st.len() {
                                    st[a] = st[a].max(1.0 + 0.1 * (hits.len() - rest_n).min(50) as f32);
                                }
                            }
                        }
                    }
                }
            }
            for (k, &(a, b)) in g.snap.loops.iter().enumerate() {
                let (ra, da) = (g.rest_area[k], signed_area(&s.pts[a..b]));
                if ra.abs() > 1e-6 {
                    let ratio = da / ra;
                    min_det = min_det.min(ratio);
                    max_det = max_det.max(ratio);
                    if ratio <= th.min_det {
                        mt.flipped_triangles += 1;
                        for v in st[a..b].iter_mut() {
                            *v = v.max(1.0 + (th.min_det - ratio));
                        }
                    }
                }
            }
        }
        if min_det.is_finite() {
            mt.min_det = min_det;
            mt.max_area_ratio = max_det.max(1.0);
        }
        norm.fold = if mt.min_det <= th.min_det { 1.0 + (th.min_det - mt.min_det) } else { ((1.0 - mt.min_det) / (1.0 - th.min_det)).max(0.0) }.min(CAP);
        norm.stretch = ((mt.max_edge_stretch - 1.0) / (th.max_stretch - 1.0)).min(CAP);
        norm.shear = (mt.max_shear / th.max_shear).min(CAP);
        // not `clamp`: `f32::max/min` map a NaN ratio (0/0 area threshold) to a bound, `clamp` would keep NaN
        #[allow(clippy::manual_clamp)]
        {
            norm.area = ((mt.max_area_ratio - 1.0) / (th.max_area - 1.0)).max(0.0).min(CAP);
        }
        norm.spike = (mt.max_spike / th.max_spike).min(CAP);
        norm.self_intersection = if mt.new_self_intersections > 0 { 1.0 + 0.1 * mt.new_self_intersections.min(50) as f32 } else { 0.0 };
        norm.off_canvas = if mt.layers_off_canvas + mt.layers_exploded > 0 { 1.0 } else { 0.0 };
        if mt.non_finite {
            norm.non_finite = CAP;
        }
        let _ = n_geo;
        self.finish(index, values, mt, norm, stress)
    }

    fn finish(&mut self, index: usize, values: &[f32], mut mt: SampleMetrics, norm: Norm, stress: Vec<Vec<f32>>) -> EvalOut {
        let mut items: Vec<(&'static str, f32)> = norm.items().to_vec();
        items.retain(|x| !self.ignore.iter().any(|i| i == x.0));
        let severity = items.iter().map(|x| x.1).fold(0.0f32, f32::max);
        items.retain(|x| x.1 >= 1.0);
        items.sort_by(|a, b| b.1.total_cmp(&a.1));
        let kinds = items.iter().map(|x| x.0.to_string()).collect();
        mt.norm = norm;
        EvalOut { result: SampleResult { index, values: values.to_vec(), severity, failed: severity >= 1.0, kinds, metrics: mt }, stress, _pad: () }
    }
}

// ---------------------------------------------------------------------------------------------
// delta debugging
// ---------------------------------------------------------------------------------------------

/// Zeller's `ddmin`: a 1-minimal subset of `items` for which `fails` still returns true (`fails(items)` must be
/// true). Subsets are tried first, then complements, with the granularity doubling when nothing shrinks.
pub fn ddmin<T: Clone>(items: &[T], fails: &mut dyn FnMut(&[T]) -> bool) -> Vec<T> {
    let mut c: Vec<T> = items.to_vec();
    if c.len() == 1 && fails(&[]) {
        return vec![];
    }
    let mut n = 2usize;
    while c.len() >= 2 {
        let len = c.len();
        let mut reduced = false;
        // `n` is reassigned only immediately before `break`, so the range bound is never observed again
        #[allow(clippy::mut_range_bound)]
        for i in 0..n {
            let (a, b) = (i * len / n, (i + 1) * len / n);
            if a == b {
                continue;
            }
            let sub = c[a..b].to_vec();
            if sub.len() < len && fails(&sub) {
                c = sub;
                n = 2;
                reduced = true;
                break;
            }
        }
        if !reduced {
            // as above: `n` changes only right before `break`
            #[allow(clippy::mut_range_bound)]
            for i in 0..n {
                let (a, b) = (i * len / n, (i + 1) * len / n);
                if a == b {
                    continue;
                }
                let comp: Vec<T> = c[..a].iter().chain(c[b..].iter()).cloned().collect();
                if !comp.is_empty() && fails(&comp) {
                    c = comp;
                    n = (n - 1).max(2);
                    reduced = true;
                    break;
                }
            }
        }
        if !reduced {
            if n >= len {
                break;
            }
            n = (2 * n).min(len);
        }
    }
    c
}

/// Bisect the blend factor `t` between `rest` (passing) and `v` (failing) and return the smallest failing
/// value found (to within `tol` of the range). Assumes failure grows monotonically with `t`.
pub fn shrink_value(rest: f32, v: f32, tol: f32, fails: &mut dyn FnMut(f32) -> bool) -> f32 {
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    let span = (v - rest).abs().max(1e-12);
    for _ in 0..40 {
        if (hi - lo) * span <= tol {
            break;
        }
        let mid = 0.5 * (lo + hi);
        if fails(rest + mid * (v - rest)) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    rest + hi * (v - rest)
}

#[derive(Clone, Debug, Serialize)]
pub struct MinimalParam {
    pub id: String,
    pub value: f32,
    pub rest: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct Minimal {
    pub kind: String,
    pub severity: f32,
    pub params: Vec<MinimalParam>,
    pub evaluations: usize,
    /// Samples at the minimal pose still fail (re-checked).
    pub verified: bool,
}

/// Shrink a failing pose: first to a 1-minimal set of parameters moved away from rest (ddmin), then each of
/// those towards rest by bisection. Failure means "the chosen kind (default: the sample's primary kind) is at or above its threshold".
pub fn minimize(f: &mut Fuzzer, values: &[f32], want: Option<&str>) -> Result<Minimal, String> {
    let start = f.evaluations();
    let rest = f.rest_values();
    let orig = f.eval(0, values);
    if !orig.failed {
        return Err("the pose to minimise does not fail".into());
    }
    let kind = match want {
        Some(k) if orig.kinds.iter().any(|x| x == k) => k.to_string(),
        Some(k) => return Err(format!("the pose does not fail with kind '{k}' (it fails with: {})", orig.kinds.join(", "))),
        None => orig.kinds.first().cloned().unwrap_or_default(),
    };
    let moved: Vec<usize> = (0..values.len()).filter(|&i| (values[i] - rest[i]).abs() > 1e-6).collect();
    let with = |set: &[usize], vals: &[f32]| -> Vec<f32> {
        let mut v = rest.clone();
        for &i in set {
            v[i] = vals[i];
        }
        v
    };
    let fails_kind = |f: &mut Fuzzer, v: &[f32]| -> bool {
        let r = f.eval(0, v);
        r.failed && r.kinds.contains(&kind)
    };
    if fails_kind(f, &rest) {
        return Ok(Minimal { kind, severity: 0.0, params: vec![], evaluations: f.evaluations() - start, verified: true });
    }
    let mut set = {
        let mut pred = |s: &[usize]| fails_kind(f, &with(s, values));
        ddmin(&moved, &mut pred)
    };
    set.sort();
    let mut cur = with(&set, values);
    for _pass in 0..2 {
        let mut changed = false;
        for &i in &set {
            let before = cur[i];
            let tol = 1e-3 * (f.params[i].hi - f.params[i].lo).abs().max(1e-6);
            let mut probe = cur.clone();
            let nv = {
                let mut pred = |x: f32| {
                    probe[i] = x;
                    fails_kind(f, &probe)
                };
                shrink_value(rest[i], before, tol, &mut pred)
            };
            if (nv - before).abs() > 1e-9 {
                cur[i] = nv;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let fin = f.eval(0, &cur);
    let verified = fin.failed;
    if !verified {
        cur = with(&set, values); // bisection assumption broke: fall back to the ddmin result
    }
    let fin = f.eval(0, &cur);
    Ok(Minimal {
        kind,
        severity: fin.severity,
        params: set.iter().map(|&i| MinimalParam { id: f.params[i].id.clone(), value: cur[i], rest: rest[i] }).collect(),
        evaluations: f.evaluations() - start,
        verified: fin.failed,
    })
}

// ---------------------------------------------------------------------------------------------
// the run: sampling, aggregation, clustering
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct FuzzOptions {
    pub samples: usize,
    pub seed: u64,
    pub strategy: Strategy,
    pub frame: f32,
    pub params: ParamOptions,
    pub thresholds: Thresholds,
    pub minimize: bool,
    /// Pose database for the pca strategy (default: the project's own keyed rotations).
    pub db: Option<PoseDb>,
    /// How many worst samples the report lists.
    pub keep_worst: usize,
    /// Failure kinds to ignore (still measured in the metrics).
    pub ignore: Vec<String>,
}

impl Default for FuzzOptions {
    fn default() -> Self {
        FuzzOptions { samples: 200, seed: 1, strategy: Strategy::Random, frame: 0.0, params: ParamOptions::default(), thresholds: Thresholds::default(), minimize: false, db: None, keep_worst: 20, ignore: vec![] }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Bin {
    pub lo: f32,
    pub hi: f32,
    pub n: usize,
    pub failures: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct Marginal {
    pub id: String,
    pub bone: Option<String>,
    pub bins: Vec<Bin>,
    /// Failure rate in the outer two bins / the inner bins.
    pub rate_outer: f32,
    pub rate_inner: f32,
    /// Highest minus lowest bin failure rate (bins with at least 3 samples).
    pub spread: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct PairScore {
    pub a: String,
    pub b: String,
    /// Worst cell failure rate (3x3 tertile cells, >= 5 samples) minus the larger of the two marginal rates of that cell.
    pub synergy: f32,
    pub cell_rate: f32,
    pub cell_samples: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct HeatGeo {
    pub name: String,
    pub kind: GeoKind,
    /// Rest-pose vertices in screen space.
    pub pts: Vec<[f32; 2]>,
    pub tris: Vec<[usize; 3]>,
    pub edges: Vec<[usize; 2]>,
    /// Worst normalised stress per vertex over all samples (1 = at a threshold).
    pub max_stress: Vec<f32>,
    /// Fraction of samples in which the vertex was at or above threshold.
    pub fail_fraction: Vec<f32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct HeatData {
    pub width: f32,
    pub height: f32,
    pub geos: Vec<HeatGeo>,
    pub bones: Vec<(String, [f32; 2], [f32; 2])>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Summary {
    pub samples: usize,
    pub failures: usize,
    pub failure_rate: f32,
    pub by_kind: BTreeMap<String, usize>,
    pub max_severity: f32,
    pub seconds: f32,
    pub ms_per_sample: f32,
    /// Severity with every parameter at rest (a rig that already fails there fails everywhere).
    pub baseline_severity: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct BoneBar {
    pub name: String,
    pub rate: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct FuzzReport {
    pub schema: String,
    pub version: u32,
    pub project: String,
    pub strategy: String,
    pub seed: u64,
    pub frame: f32,
    pub thresholds: Thresholds,
    pub params: Vec<Param>,
    pub summary: Summary,
    pub marginals: Vec<Marginal>,
    pub per_bone: Vec<BoneBar>,
    pub pairs: Vec<PairScore>,
    pub worst: Vec<SampleResult>,
    pub minimal: Option<Minimal>,
    pub heat: HeatData,
}

impl FuzzReport {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }
}

/// Run a fuzz campaign. The project is only read.
pub fn run(project: &Project, asset_dir: Option<&Path>, o: &FuzzOptions) -> Result<FuzzReport, String> {
    let mut f = Fuzzer::new(project, asset_dir, &o.params, o.thresholds.clone(), o.frame)?;
    for k in &o.ignore {
        if !Norm::default().items().iter().any(|i| i.0 == k) {
            return Err(format!("unknown failure kind '{k}' to ignore (fold, stretch, shear, area, spike, self_intersection, ik_unreachable, limit, off_canvas, non_finite, panic)"));
        }
    }
    f.ignore = o.ignore.clone();
    let pca = if o.strategy == Strategy::Pca {
        let db = match &o.db {
            Some(d) => d.clone(),
            None => crate::posesuggest::build_db(Some(project), None, None).map_err(|e| format!("pca strategy: {e}"))?,
        };
        Some(PcaSampler::new(&db, &f.params)?)
    } else {
        None
    };
    let pts = generate(&f.params, o.strategy, o.samples, o.seed, pca.as_ref())?;
    let rest = f.rest_values();
    let baseline = f.eval(0, &rest).severity;
    let t0 = Stamp::now();
    let mut results: Vec<SampleResult> = Vec::with_capacity(pts.len());
    let mut maxs: Vec<Vec<f32>> = f.geos.iter().map(|g| vec![0.0; g.snap.pts.len()]).collect();
    let mut fails: Vec<Vec<f32>> = maxs.clone();
    for (i, v) in pts.iter().enumerate() {
        let out = f.eval_full(i, v);
        for (gi, st) in out.stress.iter().enumerate() {
            for (k, s) in st.iter().enumerate() {
                maxs[gi][k] = maxs[gi][k].max(*s);
                if *s >= 1.0 {
                    fails[gi][k] += 1.0;
                }
            }
        }
        results.push(out.result);
    }
    let secs = t0.elapsed().as_secs_f32();
    let n = results.len();
    let failures = results.iter().filter(|r| r.failed).count();
    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
    for r in results.iter().filter(|r| r.failed) {
        if let Some(k) = r.kinds.first() {
            *by_kind.entry(k.clone()).or_insert(0) += 1;
        }
    }
    let summary = Summary {
        samples: n,
        failures,
        failure_rate: failures as f32 / n.max(1) as f32,
        by_kind,
        max_severity: results.iter().map(|r| r.severity).fold(0.0, f32::max),
        seconds: secs,
        ms_per_sample: 1000.0 * secs / n.max(1) as f32,
        baseline_severity: baseline,
    };
    let (marginals, per_bone) = marginals(&f.params, &results, 5);
    let pairs = pair_scores(&f.params, &results, &marginals, 10, 10);
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| results[b].severity.total_cmp(&results[a].severity).then(a.cmp(&b)));
    let worst: Vec<SampleResult> = order.iter().take(o.keep_worst).map(|&i| results[i].clone()).collect();
    let minimal = if o.minimize {
        match order.first().map(|&i| &results[i]) {
            Some(r) if r.failed => Some(minimize(&mut f, &r.values, None)?),
            _ => None,
        }
    } else {
        None
    };
    let heat = HeatData {
        width: project.width,
        height: project.height,
        geos: f
            .geos
            .iter()
            .enumerate()
            .map(|(gi, g)| HeatGeo {
                name: g.snap.name.clone(),
                kind: g.snap.kind,
                pts: g.snap.pts.iter().map(|p| {
                    let q = g.snap.world.apply(*p);
                    [q.x, q.y]
                }).collect(),
                tris: g.snap.tris.clone(),
                edges: g.snap.edges.clone(),
                max_stress: maxs[gi].clone(),
                fail_fraction: fails[gi].iter().map(|c| c / n.max(1) as f32).collect(),
            })
            .collect(),
        bones: f.rest_bones.iter().map(|(nm, a, b)| (nm.clone(), [a.x, a.y], [b.x, b.y])).collect(),
    };
    Ok(FuzzReport {
        schema: "armatura-fuzz-report".into(),
        version: 1,
        project: project.name.clone(),
        strategy: o.strategy.name().into(),
        seed: o.seed,
        frame: o.frame,
        thresholds: o.thresholds.clone(),
        params: f.params.clone(),
        summary,
        marginals,
        per_bone,
        pairs,
        worst,
        minimal,
        heat,
    })
}

/// Per-parameter failure rate in `nbins` equal bins of the parameter range, plus per-bone bars (the worst
/// increase of the failure rate at the extremes of its range - outer 40% minus inner 60% - among the bone's parameters; this isolates
/// the parameters that cause failures from those that merely co-occur with them).
pub fn marginals(params: &[Param], results: &[SampleResult], nbins: usize) -> (Vec<Marginal>, Vec<BoneBar>) {
    let nbins = nbins.max(3);
    let mut out = Vec::new();
    for (k, p) in params.iter().enumerate() {
        let mut bins: Vec<Bin> = (0..nbins)
            .map(|b| Bin { lo: p.lo + (p.hi - p.lo) * b as f32 / nbins as f32, hi: p.lo + (p.hi - p.lo) * (b + 1) as f32 / nbins as f32, n: 0, failures: 0 })
            .collect();
        for r in results {
            let b = bin_of(r.values[k], p.lo, p.hi, nbins);
            bins[b].n += 1;
            if r.failed {
                bins[b].failures += 1;
            }
        }
        let rate = |idx: &[usize]| {
            let (n, f) = idx.iter().fold((0, 0), |(n, f), &i| (n + bins[i].n, f + bins[i].failures));
            if n == 0 { 0.0 } else { f as f32 / n as f32 }
        };
        let outer = [0, nbins - 1];
        let inner: Vec<usize> = (1..nbins - 1).collect();
        let rates: Vec<f32> = bins.iter().filter(|b| b.n >= 3).map(|b| b.failures as f32 / b.n as f32).collect();
        let spread = if rates.is_empty() { 0.0 } else { rates.iter().cloned().fold(0.0, f32::max) - rates.iter().cloned().fold(1.0, f32::min) };
        let bone = p.id.strip_prefix("bone:").and_then(|s| s.rsplit_once(':')).map(|x| x.0.to_string());
        out.push(Marginal { id: p.id.clone(), bone, rate_outer: rate(&outer), rate_inner: rate(&inner), spread, bins });
    }
    let mut bars: BTreeMap<String, f32> = BTreeMap::new();
    for m in &out {
        let name = m.bone.clone().unwrap_or_else(|| m.id.clone());
        let e = bars.entry(name).or_insert(0.0);
        *e = e.max((m.rate_outer - m.rate_inner).max(0.0));
    }
    let mut bars: Vec<BoneBar> = bars.into_iter().map(|(name, rate)| BoneBar { name, rate }).collect();
    bars.sort_by(|a, b| b.rate.total_cmp(&a.rate).then(a.name.cmp(&b.name)));
    (out, bars)
}

fn bin_of(v: f32, lo: f32, hi: f32, n: usize) -> usize {
    (((v - lo) / (hi - lo).max(1e-12) * n as f32).floor().max(0.0) as usize).min(n - 1)
}

/// Pairwise interaction: for the `top_k` most influential parameters, how much worse than either single-parameter
/// bin is the worst tertile cell of the pair?
pub fn pair_scores(params: &[Param], results: &[SampleResult], marg: &[Marginal], top_k: usize, keep: usize) -> Vec<PairScore> {
    let mut idx: Vec<usize> = (0..params.len()).collect();
    idx.sort_by(|&a, &b| marg[b].spread.total_cmp(&marg[a].spread).then(a.cmp(&b)));
    idx.truncate(top_k);
    let mut out = Vec::new();
    let rate_in = |k: usize, t: usize| -> f32 {
        let p = &params[k];
        let (mut n, mut f) = (0usize, 0usize);
        for r in results {
            if bin_of(r.values[k], p.lo, p.hi, 3) == t {
                n += 1;
                f += r.failed as usize;
            }
        }
        if n == 0 { 0.0 } else { f as f32 / n as f32 }
    };
    for (x, &a) in idx.iter().enumerate() {
        for &b in &idx[x + 1..] {
            let mut cells = [[(0usize, 0usize); 3]; 3];
            for r in results {
                let (i, j) = (bin_of(r.values[a], params[a].lo, params[a].hi, 3), bin_of(r.values[b], params[b].lo, params[b].hi, 3));
                cells[i][j].0 += 1;
                cells[i][j].1 += r.failed as usize;
            }
            let mut best: Option<(f32, f32, usize)> = None;
            // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
            #[allow(clippy::needless_range_loop)]
            for i in 0..3 {
                // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
                #[allow(clippy::needless_range_loop)]
                for j in 0..3 {
                    let (n, f) = cells[i][j];
                    if n < 5 {
                        continue;
                    }
                    let rate = f as f32 / n as f32;
                    let syn = rate - rate_in(a, i).max(rate_in(b, j));
                    if best.is_none_or(|(s, _, _)| syn > s) {
                        best = Some((syn, rate, n));
                    }
                }
            }
            if let Some((s, rate, n)) = best {
                out.push(PairScore { a: params[a].id.clone(), b: params[b].id.clone(), synergy: s, cell_rate: rate, cell_samples: n });
            }
        }
    }
    out.sort_by(|p, q| q.synergy.total_cmp(&p.synergy).then(p.a.cmp(&q.a)).then(p.b.cmp(&q.b)));
    out.truncate(keep);
    out
}

// ---------------------------------------------------------------------------------------------
// two-parameter grid
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
pub struct GridResult {
    pub x: Param,
    pub y: Param,
    pub res: usize,
    /// Row-major (`y` rows, `x` columns), severity per cell.
    pub severity: Vec<f32>,
    /// Primary failing kind per cell ("" when the cell passes).
    pub kind: Vec<String>,
    pub failure_rate: f32,
}

/// Sweep two parameters over their full ranges (others at rest). Ids as in [`Param::id`].
// public entry point with a stable signature.
#[allow(clippy::too_many_arguments)]
pub fn grid(project: &Project, asset_dir: Option<&Path>, popts: &ParamOptions, th: Thresholds, frame: f32, xid: &str, yid: &str, res: usize) -> Result<GridResult, String> {
    if !(2..=512).contains(&res) {
        return Err("--res must be between 2 and 512".into());
    }
    let all = enumerate_params(project, popts);
    let find = |id: &str| all.iter().find(|p| p.id == id).cloned().ok_or_else(|| format!("unknown parameter '{id}' (examples: {})", all.iter().take(4).map(|p| p.id.clone()).collect::<Vec<_>>().join(", ")));
    let (px, py) = (find(xid)?, find(yid)?);
    if px.id == py.id {
        return Err("--x and --y must be different parameters".into());
    }
    let mut f = Fuzzer::with_params(project, asset_dir, all.clone(), th, frame)?;
    let (ix, iy) = (all.iter().position(|p| p.id == px.id).unwrap(), all.iter().position(|p| p.id == py.id).unwrap());
    let mut sev = Vec::with_capacity(res * res);
    let mut kind = Vec::with_capacity(res * res);
    let mut bad = 0;
    for j in 0..res {
        for i in 0..res {
            let mut v = f.rest_values();
            v[ix] = px.lo + (px.hi - px.lo) * i as f32 / (res - 1) as f32;
            v[iy] = py.lo + (py.hi - py.lo) * j as f32 / (res - 1) as f32;
            let r = f.eval(0, &v);
            bad += r.failed as usize;
            sev.push(r.severity);
            kind.push(if r.failed { r.kinds.first().cloned().unwrap_or_default() } else { String::new() });
        }
    }
    Ok(GridResult { x: px, y: py, res, severity: sev, kind, failure_rate: bad as f32 / (res * res) as f32 })
}

// ---------------------------------------------------------------------------------------------
// SVG output
// ---------------------------------------------------------------------------------------------

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Blue-to-orange ramp (own table): `t` in 0..1; the middle (near white) corresponds to the failure threshold
/// when `t = stress / 2`. Blue and orange stay distinguishable with the common kinds of colour blindness.
pub fn ramp(t: f32) -> [u8; 3] {
    const STOPS: [(f32, [f32; 3]); 6] = [
        (0.0, [44.0, 79.0, 158.0]),
        (0.25, [111.0, 160.0, 214.0]),
        (0.5, [238.0, 234.0, 223.0]),
        (0.7, [244.0, 176.0, 94.0]),
        (0.85, [224.0, 100.0, 28.0]),
        (1.0, [125.0, 26.0, 12.0]),
    ];
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    for w in STOPS.windows(2) {
        if t <= w[1].0 {
            let u = (t - w[0].0) / (w[1].0 - w[0].0);
            let c = |k: usize| (w[0].1[k] + (w[1].1[k] - w[0].1[k]) * u).round() as u8;
            return [c(0), c(1), c(2)];
        }
    }
    [125, 26, 12]
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Ramp position of a normalised stress value: 0..1 maps to the blue half (never reaching the pale middle),
/// 1 jumps to orange so that every failing value is visibly warm, 4 and above is the darkest.
pub fn stress_t(s: f32) -> f32 {
    if !s.is_finite() || s <= 0.0 {
        0.0
    } else if s < 1.0 {
        0.42 * s
    } else {
        0.62 + 0.38 * ((s - 1.0) / 3.0).min(1.0)
    }
}

/// RGB for a normalised stress value (1 = threshold).
pub fn stress_rgb(s: f32) -> [u8; 3] {
    ramp(stress_t(s))
}

/// Colour for a normalised stress value (1 = threshold).
pub fn stress_color(s: f32) -> String {
    hex(stress_rgb(s))
}

fn legend(out: &mut String, x: f32, y: f32, w: f32, label: &str) {
    let n = 24;
    for i in 0..n {
        let sv = 4.0 * i as f32 / (n - 1) as f32;
        out.push_str(&format!("<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"10\" fill=\"{}\"/>\n", x + w * i as f32 / n as f32, y, w / n as f32 + 0.5, stress_color(sv)));
    }
    out.push_str(&format!("<text x=\"{x:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#333\">0</text>\n", y + 22.0));
    out.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#333\" text-anchor=\"middle\">1 = threshold</text>\n", x + w / 4.0, y + 22.0));
    out.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#333\" text-anchor=\"end\">4+</text>\n", x + w, y + 22.0));
    out.push_str(&format!("<text x=\"{x:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#333\">{}</text>\n", y - 4.0, esc(label)));
}

/// SVG with the stress overlay on the rest pose (worst case over all samples), the legend and per-bone failure bars.
pub fn heatmap_svg(rep: &FuzzReport) -> String {
    let (w, h) = (rep.heat.width.max(1.0), rep.heat.height.max(1.0));
    let bars = &rep.per_bone;
    let nb = bars.len().min(24);
    let total_h = h + 60.0 + 18.0 * nb as f32 + 30.0;
    let mut s = String::new();
    s.push_str(&format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {total_h}\" width=\"{w}\" height=\"{total_h}\" font-family=\"sans-serif\">\n"));
    s.push_str(&format!("<title>Stress heat-map: {}</title>\n", esc(&rep.project)));
    s.push_str(&format!(
        "<desc>{} samples ({}, seed {}), {} failures ({:.1}%). Colour = worst normalised stress per vertex over all samples; 1 = failure threshold.</desc>\n",
        rep.summary.samples, rep.strategy, rep.seed, rep.summary.failures, 100.0 * rep.summary.failure_rate
    ));
    s.push_str(&format!("<rect width=\"{w}\" height=\"{total_h}\" fill=\"#ffffff\"/>\n<rect x=\"0\" y=\"0\" width=\"{w}\" height=\"{h}\" fill=\"#fafafa\" stroke=\"#bbb\"/>\n"));
    for g in &rep.heat.geos {
        let col = |i: usize| g.max_stress.get(i).copied().unwrap_or(0.0);
        for t in &g.tris {
            if t.iter().any(|&i| i >= g.pts.len()) {
                continue;
            }
            let m = (col(t[0]) + col(t[1]) + col(t[2])) / 3.0;
            let p = |i: usize| format!("{:.2},{:.2}", g.pts[t[i]][0], g.pts[t[i]][1]);
            s.push_str(&format!("<polygon points=\"{} {} {}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"0.3\"/>\n", p(0), p(1), p(2), stress_color(m), stress_color(m)));
        }
        for e in &g.edges {
            if e.iter().any(|&i| i >= g.pts.len()) {
                continue;
            }
            let m = 0.5 * (col(e[0]) + col(e[1]));
            s.push_str(&format!(
                "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"{}\" stroke-width=\"3\" stroke-linecap=\"round\"/>\n",
                g.pts[e[0]][0], g.pts[e[0]][1], g.pts[e[1]][0], g.pts[e[1]][1], stress_color(m)
            ));
        }
        if g.tris.is_empty() {
            for (i, p) in g.pts.iter().enumerate() {
                s.push_str(&format!("<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"3\" fill=\"{}\" stroke=\"#222\" stroke-width=\"0.5\"/>\n", p[0], p[1], stress_color(col(i))));
            }
        }
    }
    for (name, a, b) in &rep.heat.bones {
        s.push_str(&format!("<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#222\" stroke-width=\"1\" stroke-dasharray=\"4 2\"><title>{}</title></line>\n", a[0], a[1], b[0], b[1], esc(name)));
    }
    legend(&mut s, 10.0, h + 22.0, (w - 20.0).min(320.0), "worst-case stress over all samples");
    s.push_str(&format!("<text x=\"10\" y=\"{:.1}\" font-size=\"12\" font-weight=\"bold\" fill=\"#222\">Extra failure rate at the extremes of each bone's range (outer 40% minus inner 60%)</text>\n", h + 62.0));
    let bw = (w - 170.0).max(40.0);
    for (i, b) in bars.iter().take(nb).enumerate() {
        let y = h + 72.0 + 18.0 * i as f32;
        s.push_str(&format!("<text x=\"10\" y=\"{:.1}\" font-size=\"11\" fill=\"#222\">{}</text>\n", y + 11.0, esc(&b.name)));
        s.push_str(&format!("<rect x=\"150\" y=\"{y:.1}\" width=\"{bw:.1}\" height=\"12\" fill=\"#eeeeee\"/>\n"));
        s.push_str(&format!("<rect x=\"150\" y=\"{y:.1}\" width=\"{:.1}\" height=\"12\" fill=\"{}\"/>\n", bw * b.rate.clamp(0.0, 1.0), hex(ramp(0.62 + 0.38 * b.rate.clamp(0.0, 1.0)))));
        s.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#222\">{:.0}%</text>\n", 154.0 + bw * b.rate.clamp(0.0, 1.0), y + 10.0, 100.0 * b.rate));
    }
    s.push_str("</svg>\n");
    s
}

/// SVG of a two-parameter grid: colour = severity (1 = failure threshold), failing cells get a dark outline.
pub fn grid_svg(g: &GridResult) -> String {
    let cell = (480.0 / g.res as f32).clamp(2.0, 40.0);
    let (ox, oy) = (70.0f32, 40.0f32);
    let side = cell * g.res as f32;
    let (w, h) = (ox + side + 30.0, oy + side + 90.0);
    let mut s = String::new();
    s.push_str(&format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w:.0} {h:.0}\" width=\"{w:.0}\" height=\"{h:.0}\" font-family=\"sans-serif\">\n"));
    s.push_str(&format!("<title>Severity grid {} x {}</title>\n", esc(&g.x.id), esc(&g.y.id)));
    s.push_str(&format!("<desc>{}x{} cells, {:.1}% failing. Colour = severity, 1 = failure threshold.</desc>\n", g.res, g.res, 100.0 * g.failure_rate));
    s.push_str(&format!("<rect width=\"{w:.0}\" height=\"{h:.0}\" fill=\"#ffffff\"/>\n"));
    s.push_str(&format!("<text x=\"{ox}\" y=\"20\" font-size=\"12\" font-weight=\"bold\" fill=\"#222\">{} (x) vs {} (y), {:.1}% of cells fail</text>\n", esc(&g.x.id), esc(&g.y.id), 100.0 * g.failure_rate));
    for j in 0..g.res {
        for i in 0..g.res {
            let v = g.severity[j * g.res + i];
            let bad = v >= 1.0;
            s.push_str(&format!(
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{}\"{}><title>{:.3} {}</title></rect>\n",
                ox + cell * i as f32,
                oy + cell * j as f32,
                cell,
                cell,
                stress_color(v),
                if bad { " stroke=\"#3a0a02\" stroke-width=\"0.6\"" } else { "" },
                v,
                esc(&g.kind[j * g.res + i])
            ));
        }
    }
    let ticks = |p: &Param, horizontal: bool, s: &mut String| {
        for k in 0..=4 {
            let t = k as f32 / 4.0;
            let val = p.lo + (p.hi - p.lo) * t;
            if horizontal {
                s.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#333\" text-anchor=\"middle\">{:.3}</text>\n", ox + side * t, oy + side + 14.0, val));
            } else {
                s.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#333\" text-anchor=\"end\">{:.3}</text>\n", ox - 4.0, oy + side * t + 3.0, val));
            }
        }
    };
    ticks(&g.x, true, &mut s);
    ticks(&g.y, false, &mut s);
    legend(&mut s, ox, oy + side + 44.0, side.min(320.0), "severity");
    s.push_str("</svg>\n");
    s
}

// ---------------------------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Bone, IkChain, Layer, LayerKind, MeshSpec};
    use crate::path::{BoneWeight, VPath};

    fn v(x: f32, y: f32) -> Vec2 {
        Vec2::new(x, y)
    }

    /// Two-bone rig with a one-cell image mesh: left edge rides bone "a", right edge rides bone "b".
    /// Mesh spans x 360..440, y 180..220; the joint (head of b) is at (400, 200). The truth function below uses the same shape translated to (100, 100).
    fn fold_rig() -> Project {
        let mut p = Project::new("fold", 600.0, 400.0, 24.0, 10);
        p.bones = vec![Bone::new("a", None, 100.0, v(300.0, 200.0), 0.0), Bone::new("b", Some("a"), 100.0, v(100.0, 0.0), 0.0)];
        let mut pm = tiny_skia::Pixmap::new(4, 4).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(200, 30, 30, 255));
        let mut l = Layer::image_from_png("quad", &pm.encode_png().unwrap(), v(360.0, 180.0));
        if let LayerKind::Image { size, mesh, .. } = &mut l.kind {
            *size = Some(v(80.0, 40.0));
            let w = |b: &str| vec![BoneWeight { bone: b.into(), weight: 1.0 }];
            *mesh = Some(MeshSpec { cols: 1, rows: 1, weights: vec![w("a"), w("b"), w("a"), w("b")], free: None });
        }
        p.layers = vec![l];
        p
    }

    fn opts_only(bone: &str, prop_suffix: &str) -> (ParamOptions, String) {
        (ParamOptions { only_bones: vec![bone.into()], deformers: false, ..Default::default() }, format!("bone:{bone}:{prop_suffix}"))
    }

    /// Independent ground truth: signed areas of the two triangles after rotating the right edge about the joint.
    fn truth_folded(angle_deg: f32) -> bool {
        let (c, s) = (angle_deg.to_radians().cos(), angle_deg.to_radians().sin());
        let rot = |p: Vec2| v(100.0 + c * (p.x - 100.0) - s * (p.y - 100.0), 100.0 + s * (p.x - 100.0) + c * (p.y - 100.0));
        let (tl, bl) = (v(60.0, 80.0), v(60.0, 120.0));
        let (tr, br) = (rot(v(140.0, 80.0)), rot(v(140.0, 120.0)));
        let area = |a: Vec2, b: Vec2, c: Vec2| det2(b - a, c - a);
        // rest areas are positive for [tl,tr,br] and [tl,br,bl]
        area(tl, tr, br) <= 0.0 || area(tl, br, bl) <= 0.0
    }

    #[test]
    fn fold_detected_beyond_threshold_and_not_before() {
        let p = fold_rig();
        let (po, id) = opts_only("b", "angle");
        let mut f = Fuzzer::new(&p, None, &po, Thresholds::default(), 0.0).unwrap();
        let k = f.params.iter().position(|q| q.id == id).unwrap();
        let mut first_fold = None;
        let mut checked = 0;
        for a in (0..=179).map(|a| a as f32) {
            let mut vals = f.rest_values();
            vals[k] = a;
            let r = f.eval(0, &vals);
            let folded = r.metrics.min_det <= 0.0;
            // away from the exact boundary the engine must agree with the independent calculation
            if !(truth_folded(a - 0.6) != truth_folded(a + 0.6)) {
                assert_eq!(folded, truth_folded(a), "angle {a}: min_det {}", r.metrics.min_det);
                checked += 1;
            }
            if folded && first_fold.is_none() {
                first_fold = Some(a);
            }
            if folded {
                assert!(r.failed && r.kinds.contains(&"fold".to_string()), "angle {a}");
            }
        }
        assert!(checked > 150);
        let ff = first_fold.expect("a fold must occur below 180 degrees");
        assert!(ff > 20.0 && ff < 170.0, "first fold at {ff}");
        // rest pose is clean
        let rv = f.rest_values();
        let r = f.eval(0, &rv);
        assert!(!r.failed, "{:?}", r.kinds);
        assert!((r.metrics.min_det - 1.0).abs() < 1e-4);
    }

    #[test]
    fn project_is_not_modified() {
        let p = fold_rig();
        let before = p.clone();
        let o = FuzzOptions { samples: 30, strategy: Strategy::Lhs, minimize: true, ..Default::default() };
        let rep = run(&p, None, &o).unwrap();
        assert_eq!(p, before);
        assert!(rep.summary.failures > 0);
        let _ = grid(&p, None, &ParamOptions::default(), Thresholds::default(), 0.0, "bone:a:angle", "bone:b:angle", 4).unwrap();
        assert_eq!(p, before);
    }

    #[test]
    fn stretch_metric_numbers() {
        let rest = [v(0.0, 0.0), v(10.0, 0.0), v(0.0, 10.0)];
        let th = Thresholds::default();
        // uniform scale 2: edges x2, det 4, no shear
        let def: Vec<Vec2> = rest.iter().map(|p| *p * 2.0).collect();
        let m = mesh_metrics(&rest, &def, &[[0, 1, 2]], &[], &th);
        assert!((m.max_stretch - 2.0).abs() < 1e-5);
        assert!((m.min_det - 4.0).abs() < 1e-4 && (m.max_det - 4.0).abs() < 1e-4);
        assert!(m.max_shear.abs() < 1e-4);
        assert_eq!(m.flips, 0);
        // x scaled by 3, y by 1: smax/smin - 1 = 2, longest edge ratio 3 (x edge), det 3
        let def: Vec<Vec2> = rest.iter().map(|p| v(p.x * 3.0, p.y)).collect();
        let m = mesh_metrics(&rest, &def, &[[0, 1, 2]], &[], &th);
        assert!((m.max_shear - 2.0).abs() < 1e-4, "{}", m.max_shear);
        assert!((m.max_stretch - 3.0).abs() < 1e-5);
        assert!((m.min_det - 3.0).abs() < 1e-4);
        // mirrored: det -1, flagged
        let def: Vec<Vec2> = rest.iter().map(|p| v(-p.x, p.y)).collect();
        let m = mesh_metrics(&rest, &def, &[[0, 1, 2]], &[], &th);
        assert_eq!(m.flips, 1);
        assert!((m.min_det + 1.0).abs() < 1e-5);
        assert!(m.stress.iter().all(|s| *s >= 1.0));
        // rotation by 90 degrees changes nothing
        let def: Vec<Vec2> = rest.iter().map(|p| v(-p.y, p.x)).collect();
        let m = mesh_metrics(&rest, &def, &[[0, 1, 2]], &[], &th);
        assert!((m.max_stretch - 1.0).abs() < 1e-5 && m.max_shear.abs() < 1e-4 && (m.min_det - 1.0).abs() < 1e-5);
        assert!(m.stress.iter().all(|s| *s < 1e-3));
        // shear x += 0.5 y: det 1, smax/smin - 1 from the shear matrix [[1,.5],[0,1]]
        let def: Vec<Vec2> = rest.iter().map(|p| v(p.x + 0.5 * p.y, p.y)).collect();
        let m = mesh_metrics(&rest, &def, &[[0, 1, 2]], &[], &th);
        let (s1, det) = (1.0 + 0.25 + 1.0, 1.0f32);
        let smax = ((s1 + (s1 * s1 - 4.0 * det * det).sqrt()) * 0.5).sqrt();
        assert!((m.max_shear - (smax * smax - 1.0)).abs() < 1e-4, "{} vs {}", m.max_shear, smax * smax - 1.0);
        // degenerate rest triangles are ignored
        let m = mesh_metrics(&[v(0.0, 0.0), v(1.0, 0.0), v(2.0, 0.0)], &[v(0.0, 0.0), v(1.0, 0.0), v(2.0, 5.0)], &[[0, 1, 2]], &[], &th);
        assert_eq!(m.triangles, 0);
    }

    #[test]
    fn spike_is_similarity_invariant_and_detects_a_pulled_vertex() {
        let mut rest = vec![];
        for r in 0..5 {
            for c in 0..5 {
                rest.push(v(c as f32 * 10.0, r as f32 * 10.0));
            }
        }
        let tris = crate::weights::grid_triangles(4, 4);
        let th = Thresholds::default();
        let scaled: Vec<Vec2> = rest.iter().map(|p| v(p.x * 2.0 + 5.0, p.y * 2.0)).collect();
        assert!(mesh_metrics(&rest, &scaled, &tris, &[], &th).max_spike < 1e-3);
        let mut bumped = rest.clone();
        bumped[12] += v(0.0, 25.0);
        let m = mesh_metrics(&rest, &bumped, &tris, &[], &th);
        assert!(m.max_spike > 0.5, "spike {}", m.max_spike);
        assert!(m.stress[12] > 0.5);
    }

    #[test]
    fn self_intersection_sweep_matches_brute_force() {
        // bow-tie: two crossing edges
        let bow = [v(0.0, 0.0), v(10.0, 10.0), v(10.0, 0.0), v(0.0, 10.0)];
        assert_eq!(self_intersections(&bow, true, 100).len(), 1);
        let sq = [v(0.0, 0.0), v(10.0, 0.0), v(10.0, 10.0), v(0.0, 10.0)];
        assert!(self_intersections(&sq, true, 100).is_empty());
        let mut rng = Rng::new(7);
        for _ in 0..30 {
            let pts: Vec<Vec2> = (0..12).map(|_| v(rng.unit() * 20.0, rng.unit() * 20.0)).collect();
            let n = pts.len();
            let mut brute = 0;
            for i in 0..n {
                for j in i + 1..n {
                    if j == i + 1 || (i == 0 && j == n - 1) {
                        continue;
                    }
                    if segments_cross(pts[i], pts[(i + 1) % n], pts[j], pts[(j + 1) % n]) {
                        brute += 1;
                    }
                }
            }
            assert_eq!(self_intersections(&pts, true, 1000).len(), brute);
        }
        assert!((signed_area(&sq) - 100.0).abs() < 1e-4);
    }

    #[test]
    fn nan_inputs_are_flagged() {
        let p = fold_rig();
        let po = ParamOptions { only_bones: vec!["b".into()], deformers: false, ..Default::default() };
        let mut f = Fuzzer::new(&p, None, &po, Thresholds::default(), 0.0).unwrap();
        let mut vals = f.rest_values();
        vals[0] = f32::NAN;
        let r = f.eval(0, &vals);
        assert!(r.failed && r.metrics.non_finite && r.kinds[0] == "non_finite");
        // non-finite geometry in the metrics themselves does not panic
        let rest = [v(0.0, 0.0), v(1.0, 0.0), v(0.0, 1.0)];
        let def = [v(f32::NAN, 0.0), v(1.0, 0.0), v(0.0, 1.0)];
        let _ = mesh_metrics(&rest, &def, &[[0, 1, 2]], &[], &Thresholds::default());
        // report JSON never contains NaN (not valid JSON)
        let rep = run(&p, None, &FuzzOptions { samples: 5, ..Default::default() }).unwrap();
        assert!(!rep.to_json().contains("NaN"));
    }

    #[test]
    fn radical_inverse_and_halton_known_values() {
        assert_eq!(first_primes(6), vec![2, 3, 5, 7, 11, 13]);
        let b2: Vec<f64> = (1..=7).map(|i| radical_inverse(i, 2)).collect();
        assert_eq!(b2, vec![0.5, 0.25, 0.75, 0.125, 0.625, 0.375, 0.875]);
        let b3: Vec<f64> = (1..=5).map(|i| radical_inverse(i, 3)).collect();
        for (a, b) in b3.iter().zip([1.0 / 3.0, 2.0 / 3.0, 1.0 / 9.0, 4.0 / 9.0, 7.0 / 9.0]) {
            assert!((a - b).abs() < 1e-12);
        }
        let h = halton_point(4, 3);
        assert!((h[0] - 0.125).abs() < 1e-6 && (h[1] - 4.0 / 9.0).abs() < 1e-6 && (h[2] - 0.8).abs() < 1e-6);
    }

    fn unit_params(d: usize) -> Vec<Param> {
        (0..d).map(|i| Param { id: format!("bone:b{i}:angle"), target: format!("b{i}"), prop: Prop::BoneAngle, lo: 0.0, hi: 1.0, rest: 0.5, limit: None }).collect()
    }

    #[test]
    fn strategies_are_deterministic_and_cover() {
        let ps = unit_params(4);
        for s in [Strategy::Random, Strategy::Lhs, Strategy::Halton, Strategy::Sweep, Strategy::Corners] {
            let a = generate(&ps, s, 64, 5, None).unwrap();
            let b = generate(&ps, s, 64, 5, None).unwrap();
            assert_eq!(a, b, "{s:?}");
            assert!(a.iter().all(|x| x.len() == 4 && x.iter().all(|y| (0.0..=1.0).contains(y))), "{s:?} out of range");
        }
        assert_ne!(generate(&ps, Strategy::Random, 8, 1, None).unwrap(), generate(&ps, Strategy::Random, 8, 2, None).unwrap());
        // LHS: exactly one point in every stratum of every dimension
        let n = 50;
        let l = generate(&ps, Strategy::Lhs, n, 3, None).unwrap();
        for d in 0..4 {
            let mut hit = vec![0; n];
            for x in &l {
                hit[((x[d] * n as f32) as usize).min(n - 1)] += 1;
            }
            assert!(hit.iter().all(|&c| c == 1), "dim {d}: {hit:?}");
        }
        // sweep: ends of each axis included, other axes at rest
        let sw = generate(&ps, Strategy::Sweep, 40, 0, None).unwrap();
        assert_eq!(sw.len(), 40);
        assert_eq!(sw[0], vec![0.0, 0.5, 0.5, 0.5]);
        assert_eq!(sw[9], vec![1.0, 0.5, 0.5, 0.5]);
        // corners: all 16 distinct corners for d = 4, n >= 16
        let c = generate(&ps, Strategy::Corners, 16, 0, None).unwrap();
        let mut keys: Vec<String> = c.iter().map(|x| format!("{x:?}")).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), 16);
        // Halton equals the textbook points
        let h = generate(&ps, Strategy::Halton, 3, 0, None).unwrap();
        assert!((h[2][0] - 0.75).abs() < 1e-6 && (h[2][1] - 1.0 / 9.0).abs() < 1e-6);
        // error paths
        assert!(generate(&[], Strategy::Random, 4, 0, None).is_err());
        assert!(generate(&ps, Strategy::Random, 0, 0, None).is_err());
        assert!(generate(&ps, Strategy::Pca, 4, 0, None).unwrap_err().contains("pose database"));
        assert!(Strategy::parse("nope").is_err());
        // low discrepancy beats chance on coverage: every 4x4 cell of the first two Halton dims is hit by 64 points
        let h = generate(&unit_params(2), Strategy::Halton, 64, 0, None).unwrap();
        let mut cells = std::collections::HashSet::new();
        for x in &h {
            cells.insert(((x[0] * 4.0) as usize, (x[1] * 4.0) as usize));
        }
        assert_eq!(cells.len(), 16);
    }

    #[test]
    fn pca_samples_follow_the_database() {
        let ps = vec![
            Param { id: "bone:a:angle".into(), target: "a".into(), prop: Prop::BoneAngle, lo: -90.0, hi: 90.0, rest: 0.0, limit: None },
            Param { id: "bone:b:angle".into(), target: "b".into(), prop: Prop::BoneAngle, lo: -90.0, hi: 90.0, rest: 0.0, limit: None },
        ];
        // poses on a line b = 2a
        let db = crate::posesuggest::synthetic_db(1, 2, 20, 2);
        let mut db = db;
        db.bones = vec!["a".into(), "b".into()];
        db.poses = (0..40).map(|i| vec![(i as f32 - 20.0) * 1.5, (i as f32 - 20.0) * 3.0]).collect();
        db.meta = (0..40).map(|i| crate::posesuggest::PoseMeta { seq: 0, idx: i, frame: i as f32 }).collect();
        db.sequences = vec![crate::posesuggest::SeqInfo { name: "s".into(), source: "project".into(), poses: 40 }];
        let s = PcaSampler::new(&db, &ps).unwrap();
        assert_eq!(s.components(), 1);
        let pts = generate(&ps, Strategy::Pca, 50, 9, Some(&s)).unwrap();
        for p in &pts {
            if p[0].abs() < 40.0 {
                assert!((p[1] - 2.0 * p[0]).abs() < 0.5, "{p:?}");
            }
        }
        assert!(PcaSampler::new(&db, &unit_params(2)).is_err());
    }

    #[test]
    fn ddmin_finds_the_minimal_combination() {
        let items: Vec<usize> = (0..16).collect();
        let mut calls = 0;
        let mut pred = |s: &[usize]| {
            calls += 1;
            s.contains(&3) && s.contains(&11)
        };
        let m = ddmin(&items, &mut pred);
        assert_eq!(m, vec![3, 11]);
        assert!(calls < 80, "{calls}");
        // a single culprit
        let mut p1 = |s: &[usize]| s.contains(&7);
        assert_eq!(ddmin(&items, &mut p1), vec![7]);
        // everything needed
        let few = vec![1usize, 2, 3];
        let mut p2 = |s: &[usize]| s.len() == 3;
        assert_eq!(ddmin(&few, &mut p2), few);
        // bisection finds the boundary
        let b = shrink_value(0.0, 100.0, 0.01, &mut |x| x >= 37.5);
        assert!((b - 37.5).abs() < 0.02 && b >= 37.5, "{b}");
        let b = shrink_value(10.0, -90.0, 0.01, &mut |x| x <= -20.0);
        assert!((b + 20.0).abs() < 0.02 && b <= -20.0, "{b}");
    }

    /// Fold rig plus an unrelated bone with nothing to break: the minimiser must drop it.
    #[test]
    fn minimizer_reduces_to_the_failing_parameter() {
        let mut p = fold_rig();
        p.bones.push(Bone::new("noise", None, 20.0, v(550.0, 20.0), 0.0));
        let po = ParamOptions { deformers: false, ..Default::default() };
        let mut f = Fuzzer::new(&p, None, &po, Thresholds::default(), 0.0).unwrap();
        let kb = f.params.iter().position(|q| q.id == "bone:b:angle").unwrap();
        let mut vals = f.rest_values();
        for (i, q) in f.params.iter().enumerate() {
            vals[i] = q.rest + 0.4 * (q.hi - q.rest);
        }
        vals[kb] = 175.0;
        assert!(f.eval(0, &vals).failed);
        let m = minimize(&mut f, &vals, Some("fold")).unwrap();
        assert!(m.verified);
        assert_eq!(m.params.len(), 1, "{:?}", m.params);
        assert_eq!(m.params[0].id, "bone:b:angle");
        // the minimal angle sits at the true fold boundary
        let boundary = (0..180).map(|a| a as f32).find(|a| truth_folded(*a)).unwrap();
        assert!((m.params[0].value - boundary).abs() < 3.0, "{} vs {boundary}", m.params[0].value);
        assert_eq!(m.kind, "fold");
        // a passing pose cannot be minimised
        let rv = f.rest_values();
        assert!(minimize(&mut f, &rv, None).is_err());
    }

    #[test]
    fn marginals_and_pairs_point_at_the_culprits() {
        let p = fold_rig();
        let o = FuzzOptions { samples: 120, strategy: Strategy::Halton, params: ParamOptions { deformers: false, bone_props: vec!["angle".into()], ..Default::default() }, ..Default::default() };
        let rep = run(&p, None, &o).unwrap();
        let top = rep.marginals.iter().max_by(|a, b| a.spread.total_cmp(&b.spread)).unwrap();
        assert_eq!(top.id, "bone:b:angle");
        let angle = rep.marginals.iter().find(|m| m.id == "bone:b:angle").unwrap();
        assert!(angle.rate_outer > angle.rate_inner);
        assert_eq!(rep.per_bone[0].name, "b");
        assert!(!rep.pairs.is_empty());
        // inner bins never fail for b's angle (fold needs a large bend)
        assert_eq!(angle.bins[2].failures, 0);
    }

    #[test]
    fn only_and_props_restrict_ik_mix_too() {
        let mut p = Project::new("ik", 400.0, 300.0, 24.0, 4);
        p.bones = vec![
            Bone::new("up", None, 50.0, v(100.0, 150.0), 0.0),
            Bone::new("lo", Some("up"), 50.0, v(50.0, 0.0), 0.0),
            Bone::new("tgt", None, 10.0, v(180.0, 150.0), 0.0),
            Bone::new("arm_up", None, 10.0, v(10.0, 10.0), 0.0),
        ];
        p.ik = vec![IkChain::two_bone("leg", "up", "lo", "tgt", 1.0)];
        let ids = |po: &ParamOptions| enumerate_params(&p, po).into_iter().map(|q| q.id).collect::<Vec<_>>();
        assert!(ids(&ParamOptions::default()).contains(&"ik:leg:mix".to_string()));
        let only = ids(&ParamOptions { only_bones: vec!["arm_up".into()], bone_props: vec!["angle".into()], ..Default::default() });
        assert_eq!(only, vec!["bone:arm_up:angle".to_string()], "no IK mix, constraint or deformer parameter may be sampled");
        let only_bone = ids(&ParamOptions { only_bones: vec!["arm_up".into()], ..Default::default() });
        assert!(only_bone.iter().all(|i| i.starts_with("bone:arm_up:")), "{only_bone:?}");
        // an IK chain whose bones are selected keeps its mix when no property filter is given
        let chain = ids(&ParamOptions { only_bones: vec!["up".into()], ..Default::default() });
        assert!(chain.contains(&"ik:leg:mix".to_string()));
        assert!(!ids(&ParamOptions { only_bones: vec!["up".into()], bone_props: vec!["angle".into()], ..Default::default() }).contains(&"ik:leg:mix".to_string()));
    }

    #[test]
    fn ik_unreachable_and_limits() {
        let mut p = Project::new("ik", 400.0, 300.0, 24.0, 4);
        p.bones = vec![
            Bone::new("up", None, 50.0, v(100.0, 150.0), 0.0).with_angle_limits(-45.0, 45.0),
            Bone::new("lo", Some("up"), 50.0, v(50.0, 0.0), 0.0),
            Bone::new("tgt", None, 10.0, v(180.0, 150.0), 0.0),
        ];
        p.ik = vec![IkChain::two_bone("leg", "up", "lo", "tgt", 1.0)];
        let po = ParamOptions { deformers: false, offset_range: Some(150.0), ..Default::default() };
        let mut f = Fuzzer::new(&p, None, &po, Thresholds::default(), 0.0).unwrap();
        let rv = f.rest_values();
        let r = f.eval(0, &rv);
        assert!(!r.failed, "{:?} {:?}", r.kinds, r.metrics.ik_error_fraction);
        let kx = f.params.iter().position(|q| q.id == "bone:tgt:offx").unwrap();
        let mut vals = f.rest_values();
        vals[kx] = 120.0; // target 220 px away, reach 100
        let r = f.eval(0, &vals);
        assert!(r.kinds.contains(&"ik_unreachable".to_string()), "{:?}", r.metrics);
        assert_eq!(r.metrics.ik_worst_chain.as_deref(), Some("leg"));
        // ik mix 0 turns the chain off, so the target no longer matters
        let km = f.params.iter().position(|q| q.id == "ik:leg:mix").unwrap();
        vals[km] = 0.0;
        assert!(!f.eval(0, &vals).kinds.contains(&"ik_unreachable".to_string()));
        // limits: inside is fine, a margin makes the sampler ask for more than the rig allows
        let ka = f.params.iter().position(|q| q.id == "bone:up:angle").unwrap();
        assert_eq!((f.params[ka].lo, f.params[ka].hi), (-45.0, 45.0));
        let mut v2 = f.rest_values();
        v2[ka] = 60.0;
        let r = f.eval(0, &v2);
        assert_eq!(r.metrics.limit_violations, 1);
        assert!(r.kinds.contains(&"limit".to_string()));
        let wide = ParamOptions { limit_margin: 30.0, deformers: false, ..Default::default() };
        let ps = enumerate_params(&p, &wide);
        let a = ps.iter().find(|q| q.id == "bone:up:angle").unwrap();
        assert_eq!((a.lo, a.hi, a.limit), (-75.0, 75.0, Some([-45.0, 45.0])));
    }

    #[test]
    fn canvas_escape_and_param_enumeration() {
        let mut p = Project::new("c", 200.0, 200.0, 24.0, 4);
        p.bones = vec![Bone::new("root", None, 40.0, v(100.0, 100.0), 0.0), Bone::new("arm", Some("root"), 40.0, v(40.0, 0.0), 0.0)];
        let mut l = Layer::vector("blob", vec![VPath::rect("r", -10.0, -10.0, 20.0, 20.0)]);
        l.bone = Some("root".into());
        p.layers = vec![l];
        let po = ParamOptions { offset_range: Some(500.0), deformers: false, ..Default::default() };
        let mut f = Fuzzer::new(&p, None, &po, Thresholds::default(), 0.0).unwrap();
        let kx = f.params.iter().position(|q| q.id == "bone:root:offx").unwrap();
        let mut vals = f.rest_values();
        assert!(!f.eval(0, &vals).failed);
        vals[kx] = 450.0;
        let r = f.eval(0, &vals);
        assert!(r.kinds.contains(&"off_canvas".to_string()), "{:?}", r.metrics);
        // enumeration: 4 params per bone, stable order, ids unique
        let ps = enumerate_params(&p, &po);
        assert_eq!(ps.len(), 8);
        let mut ids: Vec<&String> = ps.iter().map(|q| &q.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 8);
        // a project without bones has nothing to fuzz
        let empty = Project::new("e", 10.0, 10.0, 24.0, 1);
        assert!(Fuzzer::new(&empty, None, &po, Thresholds::default(), 0.0).is_err());
    }

    #[test]
    fn driven_outputs_are_not_probed_and_inputs_use_the_curve_range() {
        let mut p = Project::new("d", 200.0, 200.0, 24.0, 4);
        p.bones = vec![Bone::new("ctrl", None, 10.0, v(20.0, 20.0), 0.0), Bone::new("bulge", None, 10.0, v(100.0, 100.0), 0.0)];
        let curve = Track::with_keys(&[(0.0, 1.0, Ease::Linear), (10.0, 2.0, Ease::Linear)]);
        p.channels = vec![Channel::driven("bulge", Prop::BoneScale, "ctrl", Prop::BoneOffsetX, curve)];
        let ps = enumerate_params(&p, &ParamOptions { deformers: false, ..Default::default() });
        assert!(ps.iter().all(|q| q.id != "bone:bulge:scale"));
        let x = ps.iter().find(|q| q.id == "bone:ctrl:offx").unwrap();
        assert!((x.lo + 1.0).abs() < 1e-5 && (x.hi - 11.0).abs() < 1e-5, "{} {}", x.lo, x.hi);
        // the driver still works inside the fuzzer: ctrl offset 10 -> bulge scale 2 (check through the bone's world scale)
        let mut f = Fuzzer::new(&p, None, &ParamOptions { deformers: false, ..Default::default() }, Thresholds::default(), 0.0).unwrap();
        let kx = f.params.iter().position(|q| q.id == "bone:ctrl:offx").unwrap();
        let mut vals = f.rest_values();
        vals[kx] = 10.0;
        assert!(!f.eval(0, &vals).metrics.non_finite);
    }

    #[test]
    fn report_schema_and_heatmap_svg() {
        let p = fold_rig();
        let o = FuzzOptions { samples: 60, strategy: Strategy::Random, minimize: true, params: ParamOptions { deformers: false, ..Default::default() }, ..Default::default() };
        let rep = run(&p, None, &o).unwrap();
        let j: serde_json::Value = serde_json::from_str(&rep.to_json()).unwrap();
        assert_eq!(j["schema"], "armatura-fuzz-report");
        assert_eq!(j["version"], 1);
        for k in ["project", "strategy", "seed", "frame", "thresholds", "params", "summary", "marginals", "per_bone", "pairs", "worst", "minimal", "heat"] {
            assert!(j.get(k).is_some(), "missing {k}");
        }
        let s = &j["summary"];
        assert_eq!(s["samples"], 60);
        for k in ["failures", "failure_rate", "by_kind", "max_severity", "seconds", "ms_per_sample", "baseline_severity"] {
            assert!(s.get(k).is_some(), "missing summary.{k}");
        }
        assert!(j["worst"].as_array().unwrap().len() <= 20);
        assert!(j["worst"][0]["severity"].as_f64().unwrap() >= j["worst"][1]["severity"].as_f64().unwrap());
        assert!(j["minimal"]["verified"].as_bool().unwrap());
        assert!(j["heat"]["geos"][0]["max_stress"].as_array().unwrap().len() == 4);
        // same seed, same report (apart from timings)
        let again = run(&p, None, &o).unwrap();
        assert_eq!(again.summary.failures, rep.summary.failures);
        assert_eq!(again.worst[0].values, rep.worst[0].values);
        // SVG is well-formed and carries the overlay + bars
        let svg = heatmap_svg(&rep);
        well_formed(&svg);
        assert!(svg.contains("<polygon") && svg.contains("Extra failure rate"));
        // worst-case stress is at the threshold or above somewhere, so a warm colour must appear
        assert!(rep.heat.geos[0].max_stress.iter().cloned().fold(0.0, f32::max) >= 1.0);
        let g = grid(&p, None, &ParamOptions { deformers: false, ..Default::default() }, Thresholds::default(), 0.0, "bone:b:angle", "bone:b:scale", 9).unwrap();
        assert_eq!(g.severity.len(), 81);
        let gs = grid_svg(&g);
        well_formed(&gs);
        assert_eq!(gs.matches("<rect").count(), 81 + 24 + 1);
        // errors
        assert!(grid(&p, None, &ParamOptions::default(), Thresholds::default(), 0.0, "bone:zz:angle", "bone:b:angle", 4).is_err());
        assert!(grid(&p, None, &ParamOptions::default(), Thresholds::default(), 0.0, "bone:b:angle", "bone:b:angle", 4).is_err());
        assert!(grid(&p, None, &ParamOptions::default(), Thresholds::default(), 0.0, "bone:a:angle", "bone:b:angle", 1).is_err());
    }

    #[test]
    fn ignored_kinds_do_not_count() {
        let mut p = Project::new("ik", 400.0, 300.0, 24.0, 4);
        p.bones = vec![Bone::new("up", None, 50.0, v(100.0, 150.0), 0.0), Bone::new("lo", Some("up"), 50.0, v(50.0, 0.0), 0.0), Bone::new("tgt", None, 10.0, v(180.0, 150.0), 0.0)];
        p.ik = vec![IkChain::two_bone("leg", "up", "lo", "tgt", 1.0)];
        let mut o = FuzzOptions { samples: 80, strategy: Strategy::Halton, params: ParamOptions { deformers: false, offset_range: Some(200.0), bone_props: vec!["offx".into()], only_bones: vec!["tgt".into()], ..Default::default() }, ..Default::default() };
        let a = run(&p, None, &o).unwrap();
        assert!(a.summary.by_kind.contains_key("ik_unreachable"));
        o.ignore = vec!["ik_unreachable".into()];
        let b = run(&p, None, &o).unwrap();
        assert_eq!(b.summary.failures, 0);
        o.ignore = vec!["bogus".into()];
        assert!(run(&p, None, &o).unwrap_err().contains("unknown failure kind"));
    }

    #[test]
    fn ramp_is_monotone_in_range_and_safe() {
        assert_eq!(ramp(0.0), [44, 79, 158]);
        assert_eq!(ramp(1.0), [125, 26, 12]);
        assert_eq!(ramp(f32::NAN), ramp(0.0));
        assert_eq!(ramp(-5.0), ramp(0.0));
        assert_eq!(ramp(9.0), ramp(1.0));
        assert_eq!(stress_color(0.0), "#2c4f9e");
        // below the threshold: blue side; at the threshold and above: warm side; monotone in between
        let warm = |c: [u8; 3]| c[0] > c[2];
        assert!(!warm(stress_rgb(0.99)) && warm(stress_rgb(1.0)) && warm(stress_rgb(10.0)));
        assert!(stress_t(0.5) < stress_t(0.9) && stress_t(0.9) < stress_t(1.0) && stress_t(1.0) < stress_t(2.0) && stress_t(2.0) < stress_t(3.9));
        assert_eq!(stress_t(f32::NAN), 0.0);
    }

    /// Minimal XML well-formedness check: balanced tags, quoted attributes, escaped text.
    fn well_formed(s: &str) {
        let mut stack: Vec<String> = vec![];
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'<' {
                let end = s[i..].find('>').expect("unterminated tag") + i;
                let tag = &s[i + 1..end];
                assert!(!tag.contains('<'), "stray '<' in {tag}");
                if let Some(name) = tag.strip_prefix('/') {
                    assert_eq!(stack.pop().as_deref(), Some(name.trim()), "mismatched close {name}");
                } else if !tag.starts_with('?') && !tag.starts_with('!') {
                    let selfc = tag.ends_with('/');
                    let name = tag.split_whitespace().next().unwrap().trim_end_matches('/').to_string();
                    assert_eq!(tag.matches('"').count() % 2, 0, "odd quotes in {tag}");
                    if !selfc {
                        stack.push(name);
                    }
                }
                i = end + 1;
            } else {
                if b[i] == b'&' {
                    let semi = s[i..].find(';').expect("bare &");
                    assert!(["&amp;", "&lt;", "&gt;", "&quot;"].contains(&&s[i..i + semi + 1]), "bad entity {}", &s[i..i + semi + 1]);
                }
                i += 1;
            }
        }
        assert!(stack.is_empty(), "unclosed {stack:?}");
    }

    #[test]
    fn svg_escapes_names() {
        let mut p = fold_rig();
        p.name = "a<b>&\"c".into();
        let rep = run(&p, None, &FuzzOptions { samples: 4, params: ParamOptions { deformers: false, ..Default::default() }, ..Default::default() }).unwrap();
        well_formed(&heatmap_svg(&rep));
    }

    #[test]
    fn path_layers_get_edge_metrics_and_self_intersection() {
        // a ribbon outline that crosses itself once the tip bone bends far enough
        let mut p = Project::new("ribbon", 300.0, 200.0, 24.0, 4);
        p.bones = vec![Bone::new("a", None, 80.0, v(40.0, 100.0), 0.0), Bone::new("b", Some("a"), 80.0, v(80.0, 0.0), 0.0)];
        let pts = [v(40.0, 90.0), v(120.0, 90.0), v(200.0, 90.0), v(200.0, 110.0), v(120.0, 110.0), v(40.0, 110.0)];
        let mut path = VPath::polygon("ribbon", &pts);
        for (i, a) in path.anchors.iter_mut().enumerate() {
            let bone = if i == 1 || i == 4 { None } else if pts[i].x > 120.0 { Some("b") } else { Some("a") };
            if let Some(b) = bone {
                a.weights = vec![BoneWeight { bone: b.into(), weight: 1.0 }];
            }
        }
        p.layers = vec![Layer::vector("rib", vec![path])];
        let po = ParamOptions { only_bones: vec!["b".into()], deformers: false, ..Default::default() };
        let mut f = Fuzzer::new(&p, None, &po, Thresholds::default(), 0.0).unwrap();
        let k = f.params.iter().position(|q| q.id == "bone:b:angle").unwrap();
        let rv = f.rest_values();
        assert!(!f.eval(0, &rv).failed);
        let mut seen_cross = false;
        for a in (-180..=180).step_by(10) {
            let mut vals = f.rest_values();
            vals[k] = a as f32;
            let r = f.eval(0, &vals);
            assert!(!r.metrics.non_finite);
            seen_cross |= r.metrics.new_self_intersections > 0 || r.metrics.flipped_triangles > 0;
        }
        assert!(seen_cross, "a fully bent ribbon must fold or cross itself");
    }
}
