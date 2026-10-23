// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
//! Pose suggestions learned from the project's own poses.
//!
//! Everything here is classical and small-data. No pretrained weights, no network, no deep learning: a
//! suggestion is only as good as the pose library it was learned from (a few dozen poses give a few
//! principal axes and nothing more).
//!
//! * **Pose database** ([`PoseDb`]): vectors of bone-rotation deltas (degrees) sampled at the key frames of
//!   the project's keyed `bone_angle` channels and of every take of a motion library (BVH retargets baked
//!   into a project or recorded into a library therefore count too). Each sequence keeps its order, so the
//!   database also knows which pose followed which.
//! * **Pose space** ([`PoseSpace`]): principal component analysis (covariance matrix, cyclic Jacobi
//!   eigenvalue iteration; a power iteration is provided as an independent check).
//! * **Between** ([`Suggester::between`]): an in-between pose for two key poses that stays on the pose
//!   manifold: linear blend in angle space (reference), blend in PCA space, the PCA blend pulled toward a
//!   Gaussian-kernel average of its nearest library poses, and the library pose that makes the smallest
//!   detour between the two.
//! * **Completion** ([`Suggester::complete`]): given a few fixed angles and/or tip targets (stage
//!   coordinates, evaluated with a forward-kinematics model checked against the engine), the most plausible
//!   full pose: regularised least squares in PCA space (Tikhonov / MAP with the PCA eigenvalues as prior
//!   variances), Gauss-Newton with Levenberg-Marquardt damping from several starts. For angle-only
//!   constraints the problem is linear and solved exactly. A nearest-library-pose baseline and, if a
//!   user-supplied [`Mlp`] is given, a network candidate are returned beside it.
//! * **Next** ([`Suggester::next`]): a kernel-smoothed first-order Markov / nearest-successor model: the
//!   library poses nearest to the current one vote with Gaussian weights for their successors' motion.
//!
//! Scores rank candidates and are heuristics, not probabilities: plausibility is `1 / (1 + max(0, d - s) / s)`
//! with `d` the RMS-degree distance to the nearest library pose and `s` the library's typical nearest-neighbour
//! spacing (a pose within the library's own sampling resolution counts as on the manifold).
//!
//! Applying a candidate ([`apply_op`]) returns one [`Op::Batch`], so a single undo reverts it; channels
//! that are locked ([`crate::chanmeta`]) are never written and are reported.

use crate::anim::{Ease, Track};
use crate::edit::Op;
use crate::motionlib::Library;
use crate::project::{Channel, Mode, Project, Prop, Source};
use crate::tinymlp::{self, Dense, Mlp, Rng, TrainOpts};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------------------------
// database
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PoseMeta {
    pub seq: usize,
    /// Position inside the sequence (0-based).
    pub idx: usize,
    /// Frame in the source (project or take).
    pub frame: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SeqInfo {
    pub name: String,
    /// "project" or "library".
    pub source: String,
    pub poses: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PoseDb {
    #[serde(default = "db_format")]
    pub format: String,
    #[serde(default = "one")]
    pub version: u32,
    pub bones: Vec<String>,
    pub poses: Vec<Vec<f32>>,
    pub meta: Vec<PoseMeta>,
    pub sequences: Vec<SeqInfo>,
}

fn db_format() -> String {
    "armatura-posedb".into()
}
fn one() -> u32 {
    1
}

impl PoseDb {
    pub fn dim(&self) -> usize {
        self.bones.len()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.format != "armatura-posedb" || self.version != 1 {
            return Err(format!("not an armatura-posedb version 1 file (format '{}', version {})", self.format, self.version));
        }
        if self.bones.is_empty() {
            return Err("pose database has no bones".into());
        }
        if self.poses.len() != self.meta.len() {
            return Err("poses and meta differ in length".into());
        }
        for (i, p) in self.poses.iter().enumerate() {
            if p.len() != self.bones.len() || p.iter().any(|v| !v.is_finite()) {
                return Err(format!("pose {i} has the wrong size or a non-finite value"));
            }
        }
        for m in &self.meta {
            if m.seq >= self.sequences.len() {
                return Err("pose refers to a missing sequence".into());
            }
        }
        Ok(())
    }

    pub fn from_json(s: &str) -> Result<PoseDb, String> {
        let db: PoseDb = serde_json::from_str(s).map_err(|e| format!("bad pose database: {e}"))?;
        db.validate()?;
        Ok(db)
    }
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("pose db serialises")
    }

    /// Index of the pose that follows `i` in its sequence.
    pub fn successor(&self, i: usize) -> Option<usize> {
        let m = &self.meta[i];
        let j = i + 1;
        (j < self.poses.len() && self.meta[j].seq == m.seq && self.meta[j].idx == m.idx + 1).then_some(j)
    }
}

type NamedTracks = Vec<(String, Track)>;

fn project_tracks(p: &Project) -> NamedTracks {
    p.channels
        .iter()
        .filter(|c| c.prop == Prop::BoneAngle && c.mode == Mode::Replace && !crate::skins::is_collapse_channel(c))
        .filter_map(|c| match &c.source {
            Source::Keys { track } if !track.keys.is_empty() => Some((c.target.clone(), track.clone())),
            _ => None,
        })
        .collect()
}

fn take_tracks(t: &crate::motionlib::Take) -> NamedTracks {
    t.tracks
        .iter()
        .filter(|tr| tr.prop == Prop::BoneAngle && !tr.keys.is_empty())
        .map(|tr| (tr.target.clone(), Track { keys: tr.keys.clone(), extrapolate: Default::default() }))
        .collect()
}

/// Build the database from a project's keyed bone rotations and/or a motion library.
/// `step`: sample every `step` frames instead of at the key frames. Bones a sequence does not animate are 0.
pub fn build_db(project: Option<&Project>, lib: Option<&Library>, step: Option<f32>) -> Result<PoseDb, String> {
    if let Some(s) = step {
        if (s.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)) || !s.is_finite() {
            return Err("step must be a positive number of frames".into());
        }
    }
    let mut seqs: Vec<(String, &'static str, NamedTracks)> = Vec::new();
    if let Some(p) = project {
        let t = project_tracks(p);
        if !t.is_empty() {
            seqs.push(("project".into(), "project", t));
        }
    }
    if let Some(l) = lib {
        for take in &l.takes {
            let t = take_tracks(take);
            if !t.is_empty() {
                seqs.push((take.name.clone(), "library", t));
            }
        }
    }
    if seqs.is_empty() {
        return Err("no keyed bone rotations found (bone_angle channels in the project or takes in the library)".into());
    }
    let mut bones: Vec<String> = Vec::new();
    // project bones first, in their order, so database columns follow the rig
    if let Some(p) = project {
        for b in &p.bones {
            if seqs.iter().any(|s| s.2.iter().any(|(t, _)| *t == b.name)) && !bones.contains(&b.name) {
                bones.push(b.name.clone());
            }
        }
    }
    for s in &seqs {
        for (t, _) in &s.2 {
            if !bones.contains(t) {
                bones.push(t.clone());
            }
        }
    }
    let mut db = PoseDb { format: db_format(), version: 1, bones, poses: vec![], meta: vec![], sequences: vec![] };
    for (name, source, tracks) in &seqs {
        let mut frames: Vec<f32> = Vec::new();
        match step {
            None => {
                for (_, t) in tracks {
                    frames.extend(t.keys.iter().map(|k| k.frame));
                }
                frames.sort_by(|a, b| a.total_cmp(b));
                frames.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
            }
            Some(s) => {
                let lo = tracks.iter().filter_map(|(_, t)| t.first_frame()).fold(f32::MAX, f32::min);
                let hi = tracks.iter().filter_map(|(_, t)| t.last_frame()).fold(f32::MIN, f32::max);
                let mut f = lo;
                while f < hi {
                    frames.push(f);
                    f += s;
                    if frames.len() > 100_000 {
                        return Err("step produces more than 100000 poses".into());
                    }
                }
                frames.push(hi);
            }
        }
        if frames.iter().any(|f| !f.is_finite()) {
            return Err("non-finite key frame".into());
        }
        let seq = db.sequences.len();
        for (idx, &f) in frames.iter().enumerate() {
            let pose: Vec<f32> = db.bones.iter().map(|b| tracks.iter().find(|(t, _)| t == b).and_then(|(_, tr)| tr.eval(f)).unwrap_or(0.0)).collect();
            if pose.iter().any(|v| !v.is_finite()) {
                return Err("non-finite angle in the source".into());
            }
            db.poses.push(pose);
            db.meta.push(PoseMeta { seq, idx, frame: f });
        }
        db.sequences.push(SeqInfo { name: name.clone(), source: (*source).into(), poses: frames.len() });
    }
    if db.poses.len() < 2 {
        return Err("a pose database needs at least two poses".into());
    }
    Ok(db)
}

/// The project's current pose (bone-rotation deltas) of the database bones at `frame`: keyed Replace channels,
/// 0 for bones without one (expressions, noise and additive channels are not evaluated).
pub fn pose_at(project: &Project, bones: &[String], frame: f32) -> Vec<f32> {
    bones
        .iter()
        .map(|b| {
            project
                .channels
                .iter()
                .find(|c| c.target == *b && c.prop == Prop::BoneAngle && c.mode == Mode::Replace && matches!(c.source, Source::Keys { .. }) && !crate::skins::is_collapse_channel(c))
                .and_then(|c| match &c.source {
                    Source::Keys { track } => track.eval(frame),
                    _ => None,
                })
                .unwrap_or(0.0)
        })
        .collect()
}

pub fn rms(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() {
        return 0.0;
    }
    (a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f32>() / a.len() as f32).sqrt()
}

// ---------------------------------------------------------------------------------------------
// PCA
// ---------------------------------------------------------------------------------------------

/// Cyclic Jacobi eigenvalue algorithm for a symmetric matrix. Returns eigenvalues (unsorted) and the
/// eigenvectors as columns of `v` (`v[row][col]`).
pub fn jacobi_eigen(mut a: Vec<Vec<f64>>) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = a.len();
    let mut v = vec![vec![0.0; n]; n];
    // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
    #[allow(clippy::needless_range_loop)]
    for i in 0..n {
        v[i][i] = 1.0;
    }
    for _ in 0..80 {
        let off: f64 = (0..n).flat_map(|i| (0..n).filter(move |&j| j != i).map(move |j| (i, j))).map(|(i, j)| a[i][j] * a[i][j]).sum();
        let diag: f64 = (0..n).map(|i| a[i][i] * a[i][i]).sum::<f64>().max(1e-300);
        if off <= 1e-24 * diag {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                if a[p][q].abs() < 1e-300 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
                #[allow(clippy::needless_range_loop)]
                for k in 0..n {
                    let (akp, akq) = (a[k][p], a[k][q]);
                    a[k][p] = c * akp - s * akq;
                    a[k][q] = s * akp + c * akq;
                }
                // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
                #[allow(clippy::needless_range_loop)]
                for k in 0..n {
                    let (apk, aqk) = (a[p][k], a[q][k]);
                    a[p][k] = c * apk - s * aqk;
                    a[q][k] = s * apk + c * aqk;
                }
                // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
                #[allow(clippy::needless_range_loop)]
                for k in 0..n {
                    let (vkp, vkq) = (v[k][p], v[k][q]);
                    v[k][p] = c * vkp - s * vkq;
                    v[k][q] = s * vkp + c * vkq;
                }
            }
        }
    }
    ((0..n).map(|i| a[i][i]).collect(), v)
}

/// Power iteration for the dominant eigenpair of a symmetric positive semi-definite matrix.
pub fn power_iteration(a: &[Vec<f64>], iters: usize, seed: u64) -> (f64, Vec<f64>) {
    let n = a.len();
    let mut rng = Rng::new(seed);
    let mut x: Vec<f64> = (0..n).map(|_| rng.unit() as f64 + 0.1).collect();
    let mut lambda = 0.0;
    for _ in 0..iters {
        let y: Vec<f64> = (0..n).map(|i| (0..n).map(|j| a[i][j] * x[j]).sum()).collect();
        let norm = y.iter().map(|v| v * v).sum::<f64>().sqrt();
        if norm < 1e-300 {
            return (0.0, x);
        }
        lambda = norm;
        x = y.iter().map(|v| v / norm).collect();
    }
    (lambda, x)
}

#[derive(Clone, Debug, Serialize)]
pub struct PoseSpace {
    pub mean: Vec<f32>,
    /// `k` orthonormal axes of length `d`, strongest first.
    pub comps: Vec<Vec<f32>>,
    /// Variance (deg squared) along each axis.
    pub eig: Vec<f32>,
    pub total_var: f32,
    /// Cumulative explained variance fraction per axis.
    pub explained: Vec<f32>,
}

impl PoseSpace {
    /// Keep the fewest axes explaining `var_keep` of the variance (at most `max_k`, at least 1).
    pub fn fit(poses: &[Vec<f32>], var_keep: f32, max_k: usize) -> Result<PoseSpace, String> {
        if poses.len() < 2 {
            return Err("PCA needs at least two poses".into());
        }
        if !(var_keep > 0.0 && var_keep <= 1.0) || max_k == 0 {
            return Err("variance fraction must be in (0, 1] and max components >= 1".into());
        }
        let d = poses[0].len();
        let n = poses.len();
        let mean: Vec<f64> = (0..d).map(|j| poses.iter().map(|p| p[j] as f64).sum::<f64>() / n as f64).collect();
        let mut cov = vec![vec![0.0f64; d]; d];
        for p in poses {
            for i in 0..d {
                let di = p[i] as f64 - mean[i];
                for j in i..d {
                    cov[i][j] += di * (p[j] as f64 - mean[j]);
                }
            }
        }
        // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
        #[allow(clippy::needless_range_loop)]
        for i in 0..d {
            // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
            #[allow(clippy::needless_range_loop)]
            for j in i..d {
                cov[i][j] /= (n - 1) as f64;
                cov[j][i] = cov[i][j];
            }
        }
        let total: f64 = (0..d).map(|i| cov[i][i]).sum();
        if total < 1e-9 {
            return Err("all poses are identical; there is nothing to learn".into());
        }
        let (vals, vecs) = jacobi_eigen(cov);
        let mut order: Vec<usize> = (0..d).collect();
        order.sort_by(|&a, &b| vals[b].total_cmp(&vals[a]).then(a.cmp(&b)));
        let (mut comps, mut eig, mut explained) = (Vec::new(), Vec::new(), Vec::new());
        let mut acc = 0.0;
        for &c in &order {
            if comps.len() >= max_k || (!comps.is_empty() && (acc / total >= var_keep as f64 || vals[c] < 1e-9 * total)) {
                break;
            }
            let mut axis: Vec<f64> = (0..d).map(|r| vecs[r][c]).collect();
            // sign convention: largest-magnitude entry positive (deterministic)
            let big = axis.iter().cloned().fold(0.0f64, |m, v| if v.abs() > m.abs() { v } else { m });
            if big < 0.0 {
                axis.iter_mut().for_each(|v| *v = -*v);
            }
            acc += vals[c].max(0.0);
            comps.push(axis.iter().map(|&v| v as f32).collect());
            eig.push(vals[c].max(1e-12) as f32);
            explained.push((acc / total) as f32);
        }
        Ok(PoseSpace { mean: mean.iter().map(|&v| v as f32).collect(), comps, eig, total_var: total as f32, explained })
    }

    pub fn k(&self) -> usize {
        self.comps.len()
    }
    pub fn project(&self, x: &[f32]) -> Vec<f32> {
        self.comps.iter().map(|c| c.iter().zip(x).zip(&self.mean).map(|((a, v), m)| a * (v - m)).sum()).collect()
    }
    pub fn reconstruct(&self, z: &[f32]) -> Vec<f32> {
        let mut x = self.mean.clone();
        for (c, &zi) in self.comps.iter().zip(z) {
            for (xj, cj) in x.iter_mut().zip(c) {
                *xj += cj * zi;
            }
        }
        x
    }
}

// ---------------------------------------------------------------------------------------------
// forward kinematics (checked against the engine in the tests)
// ---------------------------------------------------------------------------------------------

/// Stage position of the tip of `bone` when every project bone is rotated by `angles[i]` degrees from its rest
/// angle (angle limits applied; scale and offset channels are not modelled).
pub fn fk_tip(project: &Project, angles: &[f32], bone: usize) -> [f32; 2] {
    let n = project.bones.len();
    let mut world: Vec<(f64, f64, f64)> = Vec::with_capacity(n); // x, y, rotation (radians)
    let index = |name: &str| project.bones.iter().position(|b| b.name == name);
    for (i, b) in project.bones.iter().enumerate() {
        let mut delta = angles.get(i).copied().unwrap_or(0.0);
        if let Some([lo, hi]) = b.angle_limits {
            delta = delta.clamp(lo, hi);
        }
        let (px, py, pr) = match b.parent.as_deref().and_then(index) {
            Some(pi) if pi < world.len() => world[pi],
            _ => (0.0, 0.0, 0.0),
        };
        let (ox, oy) = (b.rest_offset.x as f64, b.rest_offset.y as f64);
        let (s, c) = pr.sin_cos();
        let pos = (px + c * ox - s * oy, py + s * ox + c * oy);
        let rot = pr + ((b.rest_angle + delta) as f64).to_radians();
        world.push((pos.0, pos.1, rot));
    }
    let (x, y, r) = world[bone];
    let (s, c) = r.sin_cos();
    let len = project.bones[bone].length as f64;
    [(x + c * len) as f32, (y + s * len) as f32]
}

// ---------------------------------------------------------------------------------------------
// suggestions
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BoneValue {
    pub bone: String,
    pub angle: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct Candidate {
    pub method: String,
    pub label: String,
    pub score: f32,
    pub plausibility: f32,
    /// Bone rotations to write (after `bind`: project bones only, locked and non-free bones removed).
    pub values: Vec<BoneValue>,
    /// RMS of the constraint residuals (degrees for angles, pixels for tip targets), completion only.
    pub constraint_error: Option<f32>,
    /// Frame the pose belongs at (next-pose: current frame plus the typical gap).
    pub frame: Option<f32>,
    pub notes: Vec<String>,
    #[serde(skip)]
    pub pose: Vec<f32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Suggestions {
    pub kind: String,
    pub candidates: Vec<Candidate>,
    pub notes: Vec<String>,
    /// Channels that were left out because they are locked.
    pub locked_skipped: Vec<String>,
    pub bones: Vec<String>,
    pub poses_in_library: usize,
    pub pca_axes: usize,
    pub pca_explained: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Constraint {
    /// Hold a bone's rotation (degrees) at `value`.
    Angle { bone: String, value: f32, #[serde(default = "w1")] weight: f32 },
    /// Put the tip of `bone` at stage position `at`.
    Reach { bone: String, at: [f32; 2], #[serde(default = "w_reach")] weight: f32 },
}

fn w1() -> f32 {
    1.0
}
fn w_reach() -> f32 {
    0.1
}

pub struct Suggester {
    pub db: PoseDb,
    pub space: PoseSpace,
    /// Median nearest-neighbour distance between library poses (RMS degrees, at least 0.5).
    pub spacing: f32,
    /// Median distance between consecutive poses of a sequence (RMS degrees, at least 1).
    pub step: f32,
    zs: Vec<Vec<f32>>,
}

fn median(v: &mut [f32]) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

impl Suggester {
    pub fn new(db: PoseDb) -> Result<Suggester, String> {
        Suggester::with_pca(db, 0.95, 12)
    }

    pub fn with_pca(db: PoseDb, var_keep: f32, max_k: usize) -> Result<Suggester, String> {
        db.validate()?;
        let space = PoseSpace::fit(&db.poses, var_keep, max_k)?;
        let zs: Vec<Vec<f32>> = db.poses.iter().map(|p| space.project(p)).collect();
        let mut nn = Vec::new();
        // large libraries: estimate the spacing from at most 1500 evenly spread query poses
        let stride = (db.poses.len() / 1500).max(1);
        for i in (0..db.poses.len()).step_by(stride) {
            let mut best = f32::MAX;
            for j in 0..db.poses.len() {
                if i != j {
                    let d = rms(&db.poses[i], &db.poses[j]);
                    if d > 1e-4 {
                        best = best.min(d);
                    }
                }
            }
            if best < f32::MAX {
                nn.push(best);
            }
        }
        let spacing = median(&mut nn).max(0.5);
        let mut steps = Vec::new();
        for i in 0..db.poses.len() {
            if let Some(j) = db.successor(i) {
                steps.push(rms(&db.poses[i], &db.poses[j]));
            }
        }
        let step = median(&mut steps).max(1.0);
        Ok(Suggester { db, space, spacing, step, zs })
    }

    fn check_pose(&self, x: &[f32], what: &str) -> Result<(), String> {
        if x.len() != self.db.dim() {
            return Err(format!("{what}: expected {} values, got {}", self.db.dim(), x.len()));
        }
        if x.iter().any(|v| !v.is_finite()) {
            return Err(format!("{what}: non-finite value"));
        }
        Ok(())
    }

    /// Library poses nearest to `x` (RMS degrees), closest first.
    pub fn nearest(&self, x: &[f32], n: usize) -> Vec<(usize, f32)> {
        let mut v: Vec<(usize, f32)> = self.db.poses.iter().enumerate().map(|(i, p)| (i, rms(x, p))).collect();
        v.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        v.truncate(n);
        v
    }

    pub fn plausibility(&self, x: &[f32]) -> f32 {
        let d = self.nearest(x, 1).first().map_or(f32::MAX, |p| p.1);
        1.0 / (1.0 + (d - self.spacing).max(0.0) / self.spacing)
    }

    fn cand(&self, method: &str, label: &str, pose: Vec<f32>, score: f32, plaus: f32) -> Candidate {
        Candidate { method: method.into(), label: label.into(), score, plausibility: plaus, values: vec![], constraint_error: None, frame: None, notes: vec![], pose }
    }

    // ---- between -------------------------------------------------------------------------

    /// In-between poses at fraction `t` of the way from `a` to `b`. `snap` (0..1) is how far the PCA blend is
    /// pulled toward the kernel average of its `k` nearest library poses.
    pub fn between(&self, a: &[f32], b: &[f32], t: f32, snap: f32, k: usize) -> Result<Vec<Candidate>, String> {
        self.check_pose(a, "pose A")?;
        self.check_pose(b, "pose B")?;
        if !(0.0..=1.0).contains(&t) || !(0.0..=1.0).contains(&snap) || k == 0 {
            return Err("t and snap must be in [0, 1] and k >= 1".into());
        }
        let dab = rms(a, b);
        let score_of = |x: &[f32], this: &Suggester| -> (f32, f32) {
            let detour = (rms(a, x) + rms(x, b) - dab).max(0.0) / dab.max(this.spacing);
            let plaus = this.plausibility(x);
            (plaus / (1.0 + detour), plaus)
        };
        let lin: Vec<f32> = a.iter().zip(b).map(|(x, y)| x * (1.0 - t) + y * t).collect();
        let (za, zb) = (self.space.project(a), self.space.project(b));
        let zt: Vec<f32> = za.iter().zip(&zb).map(|(x, y)| x * (1.0 - t) + y * t).collect();
        let pca = self.space.reconstruct(&zt);
        // kernel pull toward the library, neighbours measured in PCA coordinates
        let mut nb: Vec<(usize, f32)> = self.zs.iter().enumerate().map(|(i, z)| (i, z.iter().zip(&zt).map(|(p, q)| (p - q) * (p - q)).sum::<f32>().sqrt())).collect();
        nb.sort_by(|x, y| x.1.total_cmp(&y.1).then(x.0.cmp(&y.0)));
        nb.truncate(k.min(self.db.poses.len()));
        let h = (nb.iter().map(|p| p.1).sum::<f32>() / nb.len() as f32).max(1e-3);
        let ws: Vec<f32> = nb.iter().map(|p| (-(p.1 / h).powi(2) * 0.5).exp()).collect();
        let wsum: f32 = ws.iter().sum::<f32>().max(1e-12);
        let mut kernel = vec![0.0f32; a.len()];
        for ((i, _), w) in nb.iter().zip(&ws) {
            for (kj, pj) in kernel.iter_mut().zip(&self.db.poses[*i]) {
                *kj += w * pj / wsum;
            }
        }
        let manifold: Vec<f32> = pca.iter().zip(&kernel).map(|(p, q)| p * (1.0 - snap) + q * snap).collect();
        // the library pose with the smallest detour near the requested fraction
        let mut best: Option<(f32, usize)> = None;
        for (i, p) in self.db.poses.iter().enumerate() {
            let (da, db_) = (rms(a, p), rms(p, b));
            if da < 1e-3 || db_ < 1e-3 {
                continue;
            }
            let cost = (da + db_ - dab) + (da - t * (da + db_)).abs();
            if best.is_none_or(|bb| cost < bb.0) {
                best = Some((cost, i));
            }
        }
        let mut out = Vec::new();
        for (m, l, p) in [("linear", "linear blend of the angles", lin), ("pca", "blend in PCA space", pca), ("manifold", "PCA blend pulled toward the library", manifold)] {
            let (s, pl) = score_of(&p, self);
            out.push(self.cand(m, l, p, s, pl));
        }
        if let Some((_, i)) = best {
            let p = self.db.poses[i].clone();
            let (s, pl) = score_of(&p, self);
            let mut c = self.cand("library", &format!("library pose {} of '{}'", self.db.meta[i].idx, self.db.sequences[self.db.meta[i].seq].name), p, s, pl);
            c.notes.push("an existing pose from the library, not an interpolation".into());
            out.push(c);
        }
        out.sort_by(|x, y| y.score.total_cmp(&x.score));
        Ok(out)
    }

    // ---- completion ----------------------------------------------------------------------

    /// Most plausible full poses satisfying the constraints. `current` supplies the values of fixed bones (all
    /// bones not in `free`, plus `locked`) and the pose of project bones the database does not know.
    #[allow(clippy::too_many_arguments)]
    pub fn complete(
        &self,
        project: Option<&Project>,
        constraints: &[Constraint],
        current: &[f32],
        base_angles: Option<&[f32]>,
        free: Option<&[String]>,
        locked: &[String],
        prior: f32,
        top: usize,
        model: Option<&Mlp>,
    ) -> Result<Vec<Candidate>, String> {
        self.check_pose(current, "current pose")?;
        if !matches!(prior.partial_cmp(&0.0), Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)) || !prior.is_finite() || top == 0 {
            return Err("prior must be >= 0 and top >= 1".into());
        }
        let d = self.db.dim();
        let ix = |b: &str| self.db.bones.iter().position(|x| x == b);
        let mut fixed = vec![false; d];
        for l in locked {
            if let Some(i) = ix(l) {
                fixed[i] = true;
            }
        }
        if let Some(f) = free {
            for (i, b) in self.db.bones.iter().enumerate() {
                if !f.contains(b) {
                    fixed[i] = true;
                }
            }
        }
        // resolve constraints
        enum Rc {
            A(usize, f32, f32),
            R(usize, [f32; 2], f32), // project bone index, target, weight
        }
        let mut rcs: Vec<Rc> = Vec::new();
        let mut reach_dep: Vec<usize> = Vec::new();
        for c in constraints {
            match c {
                Constraint::Angle { bone, value, weight } => {
                    let i = ix(bone).ok_or_else(|| format!("bone '{bone}' is not in the pose database"))?;
                    if !value.is_finite() || ((*weight).partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)) || !weight.is_finite() {
                        return Err("constraint values must be finite and weights positive".into());
                    }
                    if locked.contains(bone) {
                        return Err(format!("bone '{bone}' is locked and cannot be constrained"));
                    }
                    fixed[i] = false;
                    rcs.push(Rc::A(i, *value, *weight));
                }
                Constraint::Reach { bone, at, weight } => {
                    let p = project.ok_or("a reach constraint needs the project (for its bone lengths)")?;
                    let pi = p.bones.iter().position(|b| b.name == *bone).ok_or_else(|| format!("unknown bone '{bone}'"))?;
                    if at.iter().any(|v| !v.is_finite()) || ((*weight).partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)) || !weight.is_finite() {
                        return Err("constraint values must be finite and weights positive".into());
                    }
                    reach_dep.push(pi);
                    rcs.push(Rc::R(pi, *at, *weight));
                }
            }
        }
        if rcs.is_empty() {
            return Err("give at least one constraint (a fixed angle or a reach target)".into());
        }
        let project_ix: Vec<Option<usize>> = match project {
            Some(p) => self.db.bones.iter().map(|b| p.bones.iter().position(|x| x.name == *b)).collect(),
            None => vec![None; d],
        };
        let limits: Vec<Option<[f32; 2]>> = match project {
            Some(p) => project_ix.iter().map(|i| i.and_then(|i| p.bones[i].angle_limits)).collect(),
            None => vec![None; d],
        };
        let clamp_pose = |x: &mut Vec<f32>| {
            for (v, l) in x.iter_mut().zip(&limits) {
                if let Some([lo, hi]) = l {
                    *v = v.clamp(*lo, *hi);
                }
            }
        };
        let tip_of = |x: &[f32], pi: usize| -> [f32; 2] {
            let p = project.expect("checked");
            let mut ang: Vec<f32> = base_angles.map(|b| b.to_vec()).unwrap_or_else(|| vec![0.0; p.bones.len()]);
            ang.resize(p.bones.len(), 0.0);
            for (j, pj) in project_ix.iter().enumerate() {
                if let Some(pj) = pj {
                    ang[*pj] = x[j];
                }
            }
            fk_tip(p, &ang, pi)
        };
        const W_FIXED: f32 = 1.0e4;
        let k = self.space.k();
        let residuals = |z: &[f32]| -> Vec<f32> {
            let mut x = self.space.reconstruct(z);
            clamp_pose(&mut x);
            let mut r = Vec::new();
            for c in &rcs {
                match c {
                    Rc::A(i, v, w) => r.push(w.sqrt() * (x[*i] - v)),
                    Rc::R(pi, at, w) => {
                        let t = tip_of(&x, *pi);
                        r.push(w.sqrt() * (t[0] - at[0]));
                        r.push(w.sqrt() * (t[1] - at[1]));
                    }
                }
            }
            for i in 0..d {
                if fixed[i] {
                    r.push(W_FIXED.sqrt() * (x[i] - current[i]));
                }
            }
            for (j, zj) in z.iter().enumerate() {
                r.push((prior / self.space.eig[j]).sqrt() * zj);
            }
            r
        };
        let cost = |z: &[f32]| residuals(z).iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>();
        let linear = reach_dep.is_empty();
        let solve = |z0: Vec<f32>| -> Vec<f32> {
            let mut z = z0;
            let mut lam = 1e-2f64;
            let mut c0 = cost(&z);
            for _ in 0..if linear { 8 } else { 60 } {
                let r0 = residuals(&z);
                let m = r0.len();
                // Jacobian by central differences
                let mut jac = vec![vec![0.0f64; k]; m];
                for j in 0..k {
                    let h = 0.02f32;
                    let (mut zp, mut zm) = (z.clone(), z.clone());
                    zp[j] += h;
                    zm[j] -= h;
                    let (rp, rm) = (residuals(&zp), residuals(&zm));
                    for i in 0..m {
                        jac[i][j] = ((rp[i] - rm[i]) / (2.0 * h)) as f64;
                    }
                }
                let mut jtj = vec![vec![0.0f64; k]; k];
                let mut jtr = vec![0.0f64; k];
                for i in 0..m {
                    for a in 0..k {
                        jtr[a] += jac[i][a] * r0[i] as f64;
                        for b in 0..k {
                            jtj[a][b] += jac[i][a] * jac[i][b];
                        }
                    }
                }
                let mut improved = false;
                for _ in 0..12 {
                    let mut a = jtj.clone();
                    for q in 0..k {
                        a[q][q] += lam * (jtj[q][q] + 1e-9);
                    }
                    let Some(delta) = solve_linear(a, jtr.iter().map(|v| -v).collect()) else {
                        lam *= 10.0;
                        continue;
                    };
                    let zn: Vec<f32> = z.iter().zip(&delta).map(|(a, b)| a + *b as f32).collect();
                    let cn = cost(&zn);
                    if cn < c0 {
                        let step: f64 = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
                        z = zn;
                        let gain = c0 - cn;
                        c0 = cn;
                        lam = (lam / 3.0).max(1e-9);
                        improved = true;
                        if step < 1e-4 || gain < 1e-9 {
                            return z;
                        }
                        break;
                    }
                    lam *= 4.0;
                }
                if !improved {
                    break;
                }
            }
            z
        };
        // starts: the mean pose and the library poses that already satisfy the constraints best
        let mut starts: Vec<Vec<f32>> = vec![vec![0.0; k]];
        let mut ranked: Vec<(f64, usize)> = self.zs.iter().enumerate().map(|(i, z)| (cost(z), i)).collect();
        ranked.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for (_, i) in ranked.iter().take(4) {
            starts.push(self.zs[*i].clone());
        }
        let mut sols: Vec<(f64, Vec<f32>)> = Vec::new();
        for s in starts {
            let z = solve(s);
            let c = cost(&z);
            let mut x = self.space.reconstruct(&z);
            clamp_pose(&mut x);
            if sols.iter().all(|(_, y)| rms(y, &x) > 0.5) {
                sols.push((c, x));
            }
        }
        sols.sort_by(|a, b| a.0.total_cmp(&b.0));
        let const_err = |x: &[f32]| -> f32 {
            let (mut s, mut n) = (0.0f32, 0usize);
            for c in &rcs {
                match c {
                    Rc::A(i, v, _) => {
                        s += (x[*i] - v).powi(2);
                        n += 1;
                    }
                    Rc::R(pi, at, _) => {
                        let t = tip_of(x, *pi);
                        s += (t[0] - at[0]).powi(2) + (t[1] - at[1]).powi(2);
                        n += 1;
                    }
                }
            }
            (s / n as f32).sqrt()
        };
        let mut out: Vec<Candidate> = Vec::new();
        for (n, (_, x)) in sols.iter().take(top).enumerate() {
            let e = const_err(x);
            let pl = self.plausibility(x);
            let mut c = self.cand("pca-ls", &format!("regularised least squares in PCA space (start {})", n + 1), x.clone(), pl / (1.0 + e / 5.0), pl);
            c.constraint_error = Some(e);
            out.push(c);
        }
        // baseline: the library pose that fits best
        let lib_best = (0..self.db.poses.len())
            .map(|i| {
                let mut x = self.db.poses[i].clone();
                for j in 0..d {
                    if fixed[j] {
                        x[j] = current[j];
                    }
                }
                clamp_pose(&mut x);
                (const_err(&x), x)
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((e, x)) = lib_best {
            let pl = self.plausibility(&x);
            let mut c = self.cand("library-nn", "best-fitting library pose (nearest-neighbour baseline)", x, pl / (1.0 + e / 5.0), pl);
            c.constraint_error = Some(e);
            out.push(c);
        }
        if let Some(m) = model {
            if !reach_dep.is_empty() {
                // the network sees only angles
                if let Some(c) = out.first_mut() {
                    c.notes.push("the MLP was skipped: it handles fixed angles only, not reach targets".into());
                }
            } else {
                let mut obs: Vec<Option<f32>> = vec![None; d];
                for i in 0..d {
                    if fixed[i] {
                        obs[i] = Some(current[i]);
                    }
                }
                for c in &rcs {
                    if let Rc::A(i, v, _) = c {
                        obs[*i] = Some(*v);
                    }
                }
                let mut x = predict_completion(m, &self.db.bones, &obs)?;
                clamp_pose(&mut x);
                let e = const_err(&x);
                let pl = self.plausibility(&x);
                let mut c = self.cand("mlp", "user-supplied small network", x, pl / (1.0 + e / 5.0), pl);
                c.constraint_error = Some(e);
                out.push(c);
            }
        }
        out.sort_by(|a, b| b.score.total_cmp(&a.score));
        Ok(out)
    }

    // ---- next ----------------------------------------------------------------------------

    /// Candidates for the pose that follows `current`: the motion that followed the nearest library poses, applied
    /// to `current`. `frame` is the current frame; candidates carry `frame + gap` where `gap` is the weighted
    /// frame spacing of the voting successors.
    pub fn next(&self, current: &[f32], frame: f32, k: usize, top: usize) -> Result<Vec<Candidate>, String> {
        self.check_pose(current, "current pose")?;
        if k == 0 || top == 0 {
            return Err("k and top must be at least 1".into());
        }
        let mut voters: Vec<(usize, usize, f32)> = Vec::new(); // (pose, successor, distance)
        for (i, p) in self.db.poses.iter().enumerate() {
            if let Some(j) = self.db.successor(i) {
                voters.push((i, j, rms(current, p)));
            }
        }
        if voters.is_empty() {
            return Err("the library has no consecutive poses (every sequence is a single pose)".into());
        }
        voters.sort_by(|a, b| a.2.total_cmp(&b.2).then(a.0.cmp(&b.0)));
        voters.truncate(k);
        let h = (voters.iter().map(|v| v.2).sum::<f32>() / voters.len() as f32).max(self.spacing * 0.5);
        let ws: Vec<f32> = voters.iter().map(|v| (-(v.2 / h).powi(2) * 0.5).exp()).collect();
        let wsum: f32 = ws.iter().sum::<f32>().max(1e-12);
        let d = current.len();
        let delta_of = |(i, j, _): &(usize, usize, f32)| -> Vec<f32> { (0..d).map(|b| self.db.poses[*j][b] - self.db.poses[*i][b]).collect() };
        let gap_of = |(i, j, _): &(usize, usize, f32)| (self.db.meta[*j].frame - self.db.meta[*i].frame).max(0.0);
        let mut mean_delta = vec![0.0f32; d];
        let mut mean_gap = 0.0f32;
        for (v, w) in voters.iter().zip(&ws) {
            for (m, dv) in mean_delta.iter_mut().zip(delta_of(v)) {
                *m += w * dv / wsum;
            }
            mean_gap += w * gap_of(v) / wsum;
        }
        let spread = (voters.iter().zip(&ws).map(|(v, w)| w * rms(&delta_of(v), &mean_delta).powi(2)).sum::<f32>() / wsum).sqrt();
        let apply = |delta: &[f32]| -> Vec<f32> { current.iter().zip(delta).map(|(c, dv)| c + dv).collect() };
        let far = voters[0].2 / self.spacing;
        let mut out = Vec::new();
        let mp = apply(&mean_delta);
        let mpl = self.plausibility(&mp);
        let mut mc = self.cand("markov-mean", "kernel-weighted mean of the following motions", mp, (1.0 / (1.0 + spread / self.step)) * mpl, mpl);
        mc.frame = Some(frame + mean_gap);
        out.push(mc);
        // individual successors, near-duplicates merged
        let mut singles: Vec<(f32, Vec<f32>, f32, String)> = Vec::new();
        for (v, w) in voters.iter().zip(&ws) {
            let p = apply(&delta_of(v));
            let label = format!("what followed pose {} of '{}'", self.db.meta[v.0].idx, self.db.sequences[self.db.meta[v.0].seq].name);
            match singles.iter_mut().find(|s| rms(&s.1, &p) < 0.5 * self.step.min(self.spacing).max(0.25)) {
                Some(s) => s.0 += w / wsum,
                None => singles.push((w / wsum, p, gap_of(v), label)),
            }
        }
        for (share, p, gap, label) in singles {
            let pl = self.plausibility(&p);
            let mut c = self.cand("successor", &label, p, share * pl, pl);
            c.frame = Some(frame + gap);
            out.push(c);
        }
        if far > 3.0 {
            for c in out.iter_mut() {
                c.notes.push(format!("the current pose is {:.1} deg from the nearest library pose (typical spacing {:.1}); the library may not cover it", voters[0].2, self.spacing));
            }
        }
        out.sort_by(|a, b| b.score.total_cmp(&a.score));
        out.truncate(top);
        Ok(out)
    }

    // ---- binding to a project ------------------------------------------------------------

    /// Fill `values` of each candidate: project bones only, locked channels and bones outside `free` removed.
    /// Returns the names of locked channels that were left out.
    pub fn bind(&self, cands: &mut [Candidate], project: &Project, free: Option<&[String]>) -> Vec<String> {
        let mut skipped: Vec<String> = Vec::new();
        for c in cands.iter_mut() {
            c.values.clear();
            for (b, v) in self.db.bones.iter().zip(&c.pose) {
                if !project.bones.iter().any(|x| x.name == *b) || free.is_some_and(|f| !f.contains(b)) {
                    continue;
                }
                if crate::chanmeta::is_locked(project, b, Prop::BoneAngle) {
                    if !skipped.contains(b) {
                        skipped.push(b.clone());
                    }
                    continue;
                }
                c.values.push(BoneValue { bone: b.clone(), angle: *v });
            }
        }
        skipped
    }
}

/// Gaussian elimination with partial pivoting; `None` when singular.
fn solve_linear(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for c in 0..n {
        let piv = (c..n).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        if a[piv][c].abs() < 1e-14 {
            return None;
        }
        a.swap(c, piv);
        b.swap(c, piv);
        for r in c + 1..n {
            let f = a[r][c] / a[c][c];
            // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
            #[allow(clippy::needless_range_loop)]
            for k in c..n {
                a[r][k] -= f * a[c][k];
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let s: f64 = (r + 1..n).map(|k| a[r][k] * x[k]).sum();
        x[r] = (b[r] - s) / a[r][r];
    }
    Some(x)
}

// ---------------------------------------------------------------------------------------------
// applying a candidate
// ---------------------------------------------------------------------------------------------

/// One undoable batch that keys the candidate's rotations at `frame`. Locked channels are skipped (and returned);
/// an error if nothing can be written.
pub fn apply_op(project: &Project, values: &[BoneValue], frame: f32, ease: Ease) -> Result<(Op, Vec<String>), String> {
    if !frame.is_finite() || frame < 0.0 {
        return Err("frame must be a finite number >= 0".into());
    }
    let mut ops: Vec<Op> = Vec::new();
    let mut skipped = Vec::new();
    // work on a copy so several values for one channel (never expected) stay consistent
    let mut channels: Vec<Channel> = project.channels.clone();
    let mut touched: Vec<usize> = Vec::new();
    let mut added: Vec<Channel> = Vec::new();
    for v in values {
        if !v.angle.is_finite() {
            return Err(format!("angle of '{}' is not finite", v.bone));
        }
        if !project.bones.iter().any(|b| b.name == v.bone) {
            return Err(format!("unknown bone '{}'", v.bone));
        }
        if crate::chanmeta::is_locked(project, &v.bone, Prop::BoneAngle) {
            skipped.push(v.bone.clone());
            continue;
        }
        let pos = channels.iter().position(|c| c.target == v.bone && c.prop == Prop::BoneAngle && matches!(c.source, Source::Keys { .. }) && !crate::skins::is_collapse_channel(c));
        match pos {
            Some(i) => {
                if let Source::Keys { track } = &mut channels[i].source {
                    track.set_key(frame, v.angle, ease);
                }
                if !touched.contains(&i) {
                    touched.push(i);
                }
            }
            None => added.push(Channel::keys(&v.bone, Prop::BoneAngle, Track::with_keys(&[(frame, v.angle, ease)]))),
        }
    }
    if touched.is_empty() && added.is_empty() {
        return Err("nothing to apply: every proposed channel is locked".into());
    }
    let need = frame.ceil() as u32 + 1;
    if need > project.frames {
        ops.push(Op::Set { path: "/frames".into(), value: serde_json::json!(need) });
    }
    for i in touched {
        ops.push(Op::Set { path: format!("/channels/{i}"), value: serde_json::to_value(&channels[i]).map_err(|e| e.to_string())? });
    }
    for c in added {
        ops.push(Op::Insert { path: "/channels/-".into(), value: serde_json::to_value(&c).map_err(|e| e.to_string())? });
    }
    Ok((Op::Batch { ops }, skipped))
}

// ---------------------------------------------------------------------------------------------
// request / response layer shared by the CLI and the studio
// ---------------------------------------------------------------------------------------------

fn d_top() -> usize {
    5
}

#[derive(Clone, Debug, Deserialize)]
pub struct Request {
    /// "between", "complete" or "next".
    pub kind: String,
    /// between: first / second frame and the fraction; complete / next: the frame the pose belongs at.
    #[serde(default)]
    pub a: Option<f32>,
    #[serde(default)]
    pub b: Option<f32>,
    #[serde(default)]
    pub t: Option<f32>,
    #[serde(default)]
    pub frame: Option<f32>,
    #[serde(default)]
    pub constraints: Vec<Constraint>,
    #[serde(default)]
    pub free: Option<Vec<String>>,
    #[serde(default = "d_top")]
    pub top: usize,
    #[serde(default)]
    pub snap: Option<f32>,
    #[serde(default)]
    pub prior: Option<f32>,
    #[serde(default)]
    pub k: Option<usize>,
}

/// Run a request against a project. The caller builds the [`Suggester`] (from the project, a library or a saved database).
pub fn suggest(project: &Project, sg: &Suggester, req: &Request, model: Option<&Mlp>) -> Result<Suggestions, String> {
    let bones = &sg.db.bones;
    let locked: Vec<String> = bones.iter().filter(|b| crate::chanmeta::is_locked(project, b, Prop::BoneAngle)).cloned().collect();
    let mut notes: Vec<String> = Vec::new();
    let missing: Vec<&String> = bones.iter().filter(|b| !project.bones.iter().any(|x| x.name == **b)).collect();
    if !missing.is_empty() {
        notes.push(format!("{} library bone(s) are not in this project and are ignored: {}", missing.len(), missing.iter().take(6).map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
    }
    let frame_ok = |f: Option<f32>, what: &str| -> Result<f32, String> {
        let f = f.ok_or_else(|| format!("{what} is required"))?;
        if !f.is_finite() || f < 0.0 {
            return Err(format!("{what} must be a finite frame >= 0"));
        }
        Ok(f)
    };
    let (kind, mut cands, key_frame) = match req.kind.as_str() {
        "between" => {
            let (fa, fb) = (frame_ok(req.a, "a")?, frame_ok(req.b, "b")?);
            let t = req.t.unwrap_or(0.5);
            let c = sg.between(&pose_at(project, bones, fa), &pose_at(project, bones, fb), t, req.snap.unwrap_or(0.5), req.k.unwrap_or(6))?;
            ("between", c, fa + (fb - fa) * t)
        }
        "complete" => {
            let f = frame_ok(req.frame, "frame")?;
            let cur = pose_at(project, bones, f);
            let base: Vec<f32> = project.bones.iter().map(|b| pose_at(project, std::slice::from_ref(&b.name), f)[0]).collect();
            let c = sg.complete(Some(project), &req.constraints, &cur, Some(&base), req.free.as_deref(), &locked, req.prior.unwrap_or(1.0), req.top.max(1), model)?;
            ("complete", c, f)
        }
        "next" => {
            let f = frame_ok(req.frame, "frame")?;
            let c = sg.next(&pose_at(project, bones, f), f, req.k.unwrap_or(8), req.top.max(1))?;
            ("next", c, f)
        }
        k => return Err(format!("unknown suggestion kind '{k}' (between, complete, next)")),
    };
    cands.truncate(req.top.max(1));
    for c in cands.iter_mut() {
        if c.frame.is_none() {
            c.frame = Some(key_frame);
        }
    }
    let locked_skipped = sg.bind(&mut cands, project, req.free.as_deref());
    if !locked_skipped.is_empty() {
        notes.push(format!("locked channels are left alone: {}", locked_skipped.join(", ")));
    }
    Ok(Suggestions {
        kind: kind.into(),
        candidates: cands,
        notes,
        locked_skipped,
        bones: bones.clone(),
        poses_in_library: sg.db.poses.len(),
        pca_axes: sg.space.k(),
        pca_explained: sg.space.explained.last().copied().unwrap_or(0.0),
    })
}

// ---------------------------------------------------------------------------------------------
// the small-network hook
// ---------------------------------------------------------------------------------------------

/// Predict a full pose from observed bone values with a pose-completion [`Mlp`] (see `tinymlp` for the contract).
/// The model's bone list must equal `bones`.
pub fn predict_completion(m: &Mlp, bones: &[String], observed: &[Option<f32>]) -> Result<Vec<f32>, String> {
    let d = bones.len();
    if m.bones != bones {
        return Err(format!("the model was trained for bones [{}], the library has [{}]", m.bones.join(", "), bones.join(", ")));
    }
    if m.n_in() != 2 * d || m.n_out() != d {
        return Err(format!("the model has {} inputs / {} outputs, a completion model for {d} bones needs {} / {d}", m.n_in(), m.n_out(), 2 * d));
    }
    let mean = |j: usize| m.mean.get(j).copied().unwrap_or(0.0);
    let scale = |j: usize| m.scale.get(j).copied().unwrap_or(1.0);
    let mut x = vec![0.0f32; 2 * d];
    for j in 0..d {
        if let Some(v) = observed[j] {
            x[j] = (v - mean(j)) / scale(j);
            x[d + j] = 1.0;
        }
    }
    let y = m.forward(&x);
    Ok((0..d).map(|j| observed[j].unwrap_or(y[j] * scale(j) + mean(j))).collect())
}

/// Train a pose-completion network on the database: random subsets of bones are observed and the network
/// must restore the whole pose (masked-input regression, MSE weighted 1.0 on hidden bones and 0.2 on observed ones).
pub fn train_completion(db: &PoseDb, o: &TrainOpts) -> Result<(Mlp, Vec<f32>), String> {
    db.validate()?;
    let d = db.dim();
    if !(2..=64).contains(&d) {
        return Err("a completion model needs 2 to 64 bones".into());
    }
    if db.poses.len() < 4 {
        return Err("training needs at least 4 poses".into());
    }
    let n = db.poses.len();
    let mean: Vec<f32> = (0..d).map(|j| db.poses.iter().map(|p| p[j]).sum::<f32>() / n as f32).collect();
    let scale: Vec<f32> = (0..d).map(|j| (db.poses.iter().map(|p| (p[j] - mean[j]).powi(2)).sum::<f32>() / n as f32).sqrt().max(1.0)).collect();
    let poses = &db.poses;
    let (mean_c, scale_c) = (mean.clone(), scale.clone());
    let mut sample = move |rng: &mut Rng| {
        let p = &poses[rng.below(n)];
        let frac = 0.1 + 0.6 * rng.unit();
        let mut mask: Vec<bool> = (0..d).map(|_| rng.unit() < frac).collect();
        if mask.iter().all(|&m| m) {
            mask[rng.below(d)] = false;
        }
        if !mask.iter().any(|&m| m) {
            mask[rng.below(d)] = true;
        }
        let mut x = vec![0.0f32; 2 * d];
        let mut t = vec![0.0f32; d];
        let mut w = vec![0.0f32; d];
        for j in 0..d {
            let nv = (p[j] - mean_c[j]) / scale_c[j];
            t[j] = nv;
            if mask[j] {
                x[j] = nv;
                x[d + j] = 1.0;
                w[j] = 0.2;
            } else {
                w[j] = 1.0;
            }
        }
        (x, t, w)
    };
    let (layers, hist) = tinymlp::train(2 * d, d, o, n.max(32), &mut sample)?;
    let m = Mlp { format: "armatura-mlp".into(), version: 1, bones: db.bones.clone(), mean, scale, layers: layers.into_iter().collect::<Vec<Dense>>() };
    m.validate()?;
    Ok((m, hist))
}

// ---------------------------------------------------------------------------------------------
// synthetic library and honest benchmark
// ---------------------------------------------------------------------------------------------

/// A deterministic gait-like library: `seqs` cycles of `per` poses over `bones` bones; each cycle has its own
/// amplitude, a phase offset and a little noise. The poses lie near a curved two-parameter surface.
pub fn synthetic_db(seed: u64, seqs: usize, per: usize, bones: usize) -> PoseDb {
    let mut rng = Rng::new(seed);
    let names: Vec<String> = (0..bones).map(|i| format!("b{i}")).collect();
    let amp: Vec<f32> = (0..bones).map(|_| 10.0 + 30.0 * rng.unit()).collect();
    let psi: Vec<f32> = (0..bones).map(|_| std::f32::consts::TAU * rng.unit()).collect();
    let off: Vec<f32> = (0..bones).map(|_| 10.0 * (rng.unit() - 0.5)).collect();
    let mut db = PoseDb { format: db_format(), version: 1, bones: names, poses: vec![], meta: vec![], sequences: vec![] };
    for s in 0..seqs {
        let k = 0.5 + 1.0 * rng.unit();
        let phase0 = std::f32::consts::TAU * rng.unit();
        for i in 0..per {
            let phi = phase0 + std::f32::consts::TAU * i as f32 / per as f32;
            let pose: Vec<f32> = (0..bones)
                .map(|j| {
                    let noise = (rng.unit() - 0.5) * 1.0;
                    k * amp[j] * (phi + psi[j]).sin() + off[j] + 0.15 * k * k * amp[j] * (2.0 * phi + psi[j]).sin() + noise
                })
                .collect();
            db.poses.push(pose);
            db.meta.push(PoseMeta { seq: s, idx: i, frame: (i * 4) as f32 });
        }
        db.sequences.push(SeqInfo { name: format!("cycle{s}"), source: "library".into(), poses: per });
    }
    db
}

/// The network configuration the CLI benchmark reports.
pub fn default_bench_opts() -> TrainOpts {
    TrainOpts { hidden: vec![64, 64], epochs: 1500, lr: 0.02, momentum: 0.9, batch: 16, seed: 1 }
}

#[derive(Clone, Debug, Serialize)]
pub struct BenchLine {
    pub task: String,
    pub method: String,
    /// Mean RMS error in degrees against the held-out ground truth (lower is better).
    pub rms_deg: f32,
}

fn split_db(db: &PoseDb, hold: impl Fn(usize) -> bool) -> (PoseDb, PoseDb) {
    let mk = |keep: &dyn Fn(usize) -> bool| {
        let mut out = PoseDb { format: db_format(), version: 1, bones: db.bones.clone(), poses: vec![], meta: vec![], sequences: vec![] };
        for (s, info) in db.sequences.iter().enumerate() {
            if !keep(s) {
                continue;
            }
            let ns = out.sequences.len();
            out.sequences.push(info.clone());
            for (i, m) in db.meta.iter().enumerate().filter(|(_, m)| m.seq == s) {
                out.poses.push(db.poses[i].clone());
                out.meta.push(PoseMeta { seq: ns, idx: m.idx, frame: m.frame });
            }
        }
        out
    };
    (mk(&|s| !hold(s)), mk(&|s| hold(s)))
}

/// Compare the methods on held-out sequences of the synthetic library. Honest by construction: whatever wins, wins.
pub fn benchmark(seed: u64, mlp_opts: &TrainOpts) -> Result<Vec<BenchLine>, String> {
    let all = synthetic_db(seed, 30, 16, 10);
    let (train, test) = split_db(&all, |s| s % 5 == 0);
    let sg = Suggester::new(train.clone())?;
    let d = train.dim();
    let (mlp, _hist) = train_completion(&train, mlp_opts)?;
    let mut rng = Rng::new(seed ^ 0xBEEF);
    let mean_pose = sg.space.mean.clone();
    let mut sums: Vec<(&str, &str, f64, usize)> = vec![
        ("completion", "mean pose", 0.0, 0),
        ("completion", "nearest library pose", 0.0, 0),
        ("completion", "PCA least squares", 0.0, 0),
        ("completion", "MLP", 0.0, 0),
    ];
    for p in &test.poses {
        for _ in 0..3 {
            let mut obs = vec![false; d];
            let mut left = 3;
            while left > 0 {
                let j = rng.below(d);
                if !obs[j] {
                    obs[j] = true;
                    left -= 1;
                }
            }
            let hidden: Vec<usize> = (0..d).filter(|&j| !obs[j]).collect();
            let err = |x: &[f32]| (hidden.iter().map(|&j| (x[j] - p[j]).powi(2)).sum::<f32>() / hidden.len() as f32).sqrt() as f64;
            let cons: Vec<Constraint> = (0..d).filter(|&j| obs[j]).map(|j| Constraint::Angle { bone: sg.db.bones[j].clone(), value: p[j], weight: 1.0 }).collect();
            // mean pose with the observed values pasted in
            let mut m = mean_pose.clone();
            for j in 0..d {
                if obs[j] {
                    m[j] = p[j];
                }
            }
            // nearest library pose by the observed bones only
            let nn = train.poses.iter().min_by(|a, b| {
                let da: f32 = (0..d).filter(|&j| obs[j]).map(|j| (a[j] - p[j]).powi(2)).sum();
                let db_: f32 = (0..d).filter(|&j| obs[j]).map(|j| (b[j] - p[j]).powi(2)).sum();
                da.total_cmp(&db_)
            });
            let cur = vec![0.0; d];
            let cands = sg.complete(None, &cons, &cur, None, None, &[], 1.0, 1, None)?;
            let pca = cands.iter().find(|c| c.method == "pca-ls").ok_or("no PCA candidate")?;
            let observed: Vec<Option<f32>> = (0..d).map(|j| obs[j].then_some(p[j])).collect();
            let net = predict_completion(&mlp, &sg.db.bones, &observed)?;
            let vals = [err(&m), err(nn.expect("non-empty")), err(&pca.pose), err(&net)];
            for (s, v) in sums.iter_mut().zip(vals) {
                s.2 += v;
                s.3 += 1;
            }
        }
    }
    let mut out: Vec<BenchLine> = sums.iter().map(|s| BenchLine { task: s.0.into(), method: s.1.into(), rms_deg: (s.2 / s.3 as f64) as f32 }).collect();
    // between: predict the middle pose of three consecutive test poses
    let mut bsum = [0.0f64; 4];
    let mut bn = 0usize;
    for i in 0..test.poses.len() {
        if i + 2 < test.poses.len() && test.meta[i].seq == test.meta[i + 2].seq {
            let truth = &test.poses[i + 1];
            let c = sg.between(&test.poses[i], &test.poses[i + 2], 0.5, 0.5, 6)?;
            for (slot, m) in ["linear", "pca", "manifold", "library"].iter().enumerate() {
                if let Some(x) = c.iter().find(|x| x.method == *m) {
                    bsum[slot] += rms(&x.pose, truth) as f64;
                }
            }
            bn += 1;
        }
    }
    for (slot, m) in ["linear blend", "PCA blend", "PCA blend pulled to the library", "nearest between library pose"].iter().enumerate() {
        out.push(BenchLine { task: "between".into(), method: (*m).into(), rms_deg: (bsum[slot] / bn.max(1) as f64) as f32 });
    }
    // next: predict the following pose
    let (mut hold, mut mean_e, mut best_e, mut nn) = (0.0f64, 0.0f64, 0.0f64, 0usize);
    for i in 0..test.poses.len() {
        if let Some(j) = test.successor(i) {
            let truth = &test.poses[j];
            hold += rms(&test.poses[i], truth) as f64;
            let c = sg.next(&test.poses[i], test.meta[i].frame, 8, 8)?;
            mean_e += rms(&c.iter().find(|x| x.method == "markov-mean").ok_or("no mean")?.pose, truth) as f64;
            best_e += c.iter().map(|x| rms(&x.pose, truth)).fold(f32::MAX, f32::min) as f64;
            nn += 1;
        }
    }
    let nn = nn.max(1) as f64;
    out.push(BenchLine { task: "next".into(), method: "stay in place".into(), rms_deg: (hold / nn) as f32 });
    out.push(BenchLine { task: "next".into(), method: "Markov mean".into(), rms_deg: (mean_e / nn) as f32 });
    out.push(BenchLine { task: "next".into(), method: "best of the candidates (oracle choice)".into(), rms_deg: (best_e / nn) as f32 });
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::Ease;
    use crate::math::Vec2;
    use crate::project::Bone;

    fn arm() -> Project {
        let mut p = Project::new("arm", 400.0, 300.0, 24.0, 40);
        p.bones = vec![
            Bone::new("u", None, 80.0, Vec2::new(150.0, 120.0), 10.0),
            Bone::new("l", Some("u"), 70.0, Vec2::new(80.0, 0.0), -5.0),
            Bone::new("h", Some("l"), 30.0, Vec2::new(70.0, 0.0), 0.0).with_angle_limits(-60.0, 60.0),
        ];
        p
    }

    /// Reach cycle on the arm: u and l swing together, h follows l.
    fn arm_with_cycle() -> Project {
        let mut p = arm();
        let mut tu = Track::new();
        let mut tl = Track::new();
        let mut th = Track::new();
        for i in 0..=12 {
            let f = i as f32 * 3.0;
            let ph = std::f32::consts::TAU * i as f32 / 12.0;
            tu.set_key(f, 40.0 * ph.sin(), Ease::Linear);
            tl.set_key(f, 30.0 * (ph + 0.8).sin() + 20.0, Ease::Linear);
            th.set_key(f, 0.5 * 30.0 * (ph + 0.8).sin(), Ease::Linear);
        }
        p.channels = vec![Channel::keys("u", Prop::BoneAngle, tu), Channel::keys("l", Prop::BoneAngle, tl), Channel::keys("h", Prop::BoneAngle, th)];
        p
    }

    fn engine_tip(p: &Project, angles: &[f32], bone: usize) -> Vec2 {
        let mut q = p.clone();
        q.channels = p.bones.iter().zip(angles).map(|(b, a)| Channel::keys(&b.name, Prop::BoneAngle, Track::with_keys(&[(0.0, *a, Ease::Linear)]))).collect();
        let e = crate::Engine::new(q).unwrap();
        e.evaluate(0.0).world[bone].apply(Vec2::new(p.bones[bone].length, 0.0))
    }

    #[test]
    fn forward_kinematics_matches_the_engine() {
        let p = arm();
        for angles in [[0.0f32, 0.0, 0.0], [30.0, -45.0, 20.0], [-80.0, 100.0, 59.0], [10.0, 10.0, -200.0]] {
            for bone in 0..3 {
                let want = engine_tip(&p, &angles, bone);
                let got = fk_tip(&p, &angles, bone);
                assert!((want.x - got[0]).abs() < 1e-2 && (want.y - got[1]).abs() < 1e-2, "{angles:?} bone {bone}: {want:?} vs {got:?}");
            }
        }
    }

    #[test]
    fn database_from_project_and_library() {
        let p = arm_with_cycle();
        let db = build_db(Some(&p), None, None).unwrap();
        assert_eq!(db.bones, vec!["u", "l", "h"]);
        assert_eq!(db.poses.len(), 13);
        assert_eq!(db.sequences.len(), 1);
        assert_eq!(db.meta[3], PoseMeta { seq: 0, idx: 3, frame: 9.0 });
        assert!((db.poses[3][0] - 40.0).abs() < 1e-3, "key value at frame 9 (a quarter cycle)");
        assert_eq!(db.successor(0), Some(1));
        assert_eq!(db.successor(12), None);
        db.validate().unwrap();
        assert_eq!(PoseDb::from_json(&db.to_json()).unwrap(), db);
        // stepped sampling
        let dense = build_db(Some(&p), None, Some(1.0)).unwrap();
        assert_eq!(dense.poses.len(), 37);
        // library takes become sequences; missing bones are 0 and extra bones are appended
        let mut lib = Library::default();
        let mut q = arm_with_cycle();
        q.channels.push(Channel::keys("tail", Prop::BoneAngle, Track::with_keys(&[(0.0, 5.0, Ease::Linear), (6.0, 15.0, Ease::Linear)])));
        crate::motionlib::record(&mut lib, &q, "wag", 0.0, 6.0, &["tail".to_string()]).unwrap();
        let both = build_db(Some(&p), Some(&lib), None).unwrap();
        assert_eq!(both.bones, vec!["u", "l", "h", "tail"]);
        assert_eq!(both.sequences.len(), 2);
        assert_eq!(both.sequences[1].name, "wag");
        assert_eq!(both.poses[0][3], 0.0);
        assert_eq!(both.poses.last().unwrap()[3], 15.0);
        assert_eq!(both.poses.last().unwrap()[0], 0.0);
    }

    #[test]
    fn database_errors_are_reported() {
        let p = arm();
        assert!(build_db(Some(&p), None, None).unwrap_err().contains("no keyed bone rotations"));
        assert!(build_db(None, None, None).is_err());
        let q = arm_with_cycle();
        assert!(build_db(Some(&q), None, Some(0.0)).is_err());
        assert!(build_db(Some(&q), None, Some(f32::NAN)).is_err());
        let mut one = arm();
        one.channels = vec![Channel::keys("u", Prop::BoneAngle, Track::with_keys(&[(0.0, 1.0, Ease::Linear)]))];
        assert!(build_db(Some(&one), None, None).unwrap_err().contains("at least two poses"));
        let mut db = build_db(Some(&q), None, None).unwrap();
        db.poses[0].pop();
        assert!(db.validate().is_err());
        assert!(PoseDb::from_json("{").is_err());
        let mut bad = build_db(Some(&q), None, None).unwrap();
        bad.format = "x".into();
        assert!(bad.validate().is_err());
    }

    #[test]
    fn jacobi_and_power_iteration_agree_with_a_known_matrix() {
        let a = vec![vec![2.0, 1.0, 0.0], vec![1.0, 2.0, 0.0], vec![0.0, 0.0, 5.0]];
        let (vals, vecs) = jacobi_eigen(a.clone());
        let mut sorted = vals.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        assert!((sorted[0] - 1.0).abs() < 1e-9 && (sorted[1] - 3.0).abs() < 1e-9 && (sorted[2] - 5.0).abs() < 1e-9, "{sorted:?}");
        // A v = lambda v for every pair
        for c in 0..3 {
            for r in 0..3 {
                let av: f64 = (0..3).map(|k| a[r][k] * vecs[k][c]).sum();
                assert!((av - vals[c] * vecs[r][c]).abs() < 1e-9);
            }
        }
        let (l, v) = power_iteration(&a, 200, 3);
        assert!((l - 5.0).abs() < 1e-6 && v[2].abs() > 0.999);
    }

    #[test]
    fn pca_recovers_known_axes_and_variances() {
        // 6-D data spread along two known orthonormal axes with standard deviations 10 and 3, plus tiny noise
        let u1 = [0.5f32, 0.5, 0.5, 0.5, 0.0, 0.0];
        let u2 = [0.5f32, -0.5, 0.5, -0.5, 0.0, 0.0];
        let mut rng = Rng::new(11);
        let mut gauss = || {
            let (a, b) = (rng.unit().max(1e-6), rng.unit());
            (-2.0 * a.ln()).sqrt() * (std::f32::consts::TAU * b).cos()
        };
        let poses: Vec<Vec<f32>> = (0..600)
            .map(|_| {
                let (a, b) = (10.0 * gauss(), 3.0 * gauss());
                (0..6).map(|j| 7.0 + a * u1[j] + b * u2[j] + 0.01 * gauss()).collect()
            })
            .collect();
        let sp = PoseSpace::fit(&poses, 0.99, 6).unwrap();
        assert_eq!(sp.k(), 2, "two axes carry 99 % of the variance");
        let cos = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>().abs();
        assert!(cos(&sp.comps[0], &u1) > 0.999, "first axis {:?}", sp.comps[0]);
        assert!(cos(&sp.comps[1], &u2) > 0.995, "second axis {:?}", sp.comps[1]);
        assert!((sp.eig[0].sqrt() - 10.0).abs() < 0.8 && (sp.eig[1].sqrt() - 3.0).abs() < 0.3, "{:?}", sp.eig);
        assert!(sp.explained[1] > 0.99 && sp.explained[0] < sp.explained[1]);
        assert!((sp.mean[0] - 7.0).abs() < 1.5);
        // projection and reconstruction round trip for points on the plane
        let x = poses[5].clone();
        let back = sp.reconstruct(&sp.project(&x));
        assert!(rms(&x, &back) < 0.1, "{}", rms(&x, &back));
        // orthonormal axes
        assert!(sp.comps[0].iter().zip(&sp.comps[1]).map(|(a, b)| a * b).sum::<f32>().abs() < 1e-4);
        assert!((sp.comps[0].iter().map(|v| v * v).sum::<f32>() - 1.0).abs() < 1e-4);
        // errors
        assert!(PoseSpace::fit(&poses[..1], 0.9, 3).is_err());
        assert!(PoseSpace::fit(&poses, 0.0, 3).is_err());
        assert!(PoseSpace::fit(&poses, 0.9, 0).is_err());
        assert!(PoseSpace::fit(&vec![vec![1.0, 2.0]; 5], 0.9, 2).unwrap_err().contains("identical"));
    }

    fn circle_db() -> PoseDb {
        // poses on a circle of radius 30 in the plane of bones b0, b1 (a curved one-dimensional manifold), 4 bones
        let mut db = PoseDb { format: db_format(), version: 1, bones: (0..4).map(|i| format!("b{i}")).collect(), poses: vec![], meta: vec![], sequences: vec![SeqInfo { name: "circle".into(), source: "library".into(), poses: 36 }] };
        for i in 0..36 {
            let t = std::f32::consts::TAU * i as f32 / 36.0;
            db.poses.push(vec![30.0 * t.cos(), 30.0 * t.sin(), 0.0, 0.0]);
            db.meta.push(PoseMeta { seq: 0, idx: i, frame: i as f32 * 2.0 });
        }
        db
    }

    #[test]
    fn between_stays_closer_to_the_manifold_than_a_linear_blend() {
        let sg = Suggester::with_pca(circle_db(), 0.999, 4).unwrap();
        let a = sg.db.poses[0].clone();
        let b = sg.db.poses[12].clone(); // 120 degrees apart: the chord midpoint is 15 inside the circle
        let c = sg.between(&a, &b, 0.5, 0.8, 6).unwrap();
        let get = |m: &str| c.iter().find(|x| x.method == m).unwrap();
        let off = |x: &Candidate| (x.pose[0].hypot(x.pose[1]) - 30.0).abs();
        assert!((off(get("linear")) - 15.0).abs() < 0.2, "linear midpoint is 15 off the circle, got {}", off(get("linear")));
        assert!((off(get("pca")) - 15.0).abs() < 0.5, "a PCA blend of a curved manifold is no better");
        assert!(off(get("manifold")) < 5.0, "kernel pull: {}", off(get("manifold")));
        assert!(off(get("library")) < 0.1, "a library pose lies on the circle");
        // the ranking prefers on-manifold candidates
        assert!(c[0].method == "library" || c[0].method == "manifold", "best {:?}", c.iter().map(|x| (&x.method, x.score)).collect::<Vec<_>>());
        assert!(get("linear").score < get("manifold").score);
        assert!(c.windows(2).all(|w| w[0].score >= w[1].score));
        // the library pose is roughly half way: around 60 degrees, i.e. index 6
        let lp = get("library");
        let ang = lp.pose[1].atan2(lp.pose[0]).to_degrees();
        assert!((ang - 60.0).abs() < 12.0, "library between-pose at {ang} degrees");
        // endpoints: t = 0 gives A back for the blends
        let c0 = sg.between(&a, &b, 0.0, 0.0, 6).unwrap();
        assert!(rms(&c0.iter().find(|x| x.method == "linear").unwrap().pose, &a) < 1e-4);
        assert!(rms(&c0.iter().find(|x| x.method == "pca").unwrap().pose, &a) < 0.1);
    }

    #[test]
    fn between_on_a_straight_manifold_keeps_the_linear_blend() {
        // poses on a straight line: nothing to fix
        let mut db = circle_db();
        for (i, p) in db.poses.iter_mut().enumerate() {
            *p = vec![i as f32, 2.0 * i as f32, -(i as f32), 0.5 * i as f32];
        }
        let sg = Suggester::new(db).unwrap();
        let (a, b) = (sg.db.poses[4].clone(), sg.db.poses[14].clone());
        let c = sg.between(&a, &b, 0.5, 0.5, 4).unwrap();
        let lin = c.iter().find(|x| x.method == "linear").unwrap();
        let man = c.iter().find(|x| x.method == "manifold").unwrap();
        assert!(rms(&lin.pose, &sg.db.poses[9]) < 1e-3);
        assert!(rms(&man.pose, &sg.db.poses[9]) < 1.5, "{}", rms(&man.pose, &sg.db.poses[9]));
        assert!(lin.plausibility > 0.9);
    }

    #[test]
    fn between_rejects_bad_input() {
        let sg = Suggester::with_pca(circle_db(), 0.99, 4).unwrap();
        let a = sg.db.poses[0].clone();
        assert!(sg.between(&a[..3], &a, 0.5, 0.5, 4).is_err());
        assert!(sg.between(&a, &a, 1.5, 0.5, 4).is_err());
        assert!(sg.between(&a, &a, 0.5, -0.1, 4).is_err());
        assert!(sg.between(&a, &a, 0.5, 0.5, 0).is_err());
        let mut nan = a.clone();
        nan[0] = f32::NAN;
        assert!(sg.between(&nan, &a, 0.5, 0.5, 4).is_err());
        assert!(sg.between(&a, &a, f32::NAN, 0.5, 4).is_err());
    }

    #[test]
    fn completion_is_exact_on_a_linear_pose_space() {
        // poses = mean + a*u1 + b*u2 exactly; fixing two angles determines the rest
        let u1 = [0.6f32, 0.8, 0.0, 0.0, 0.0];
        let u2 = [0.0f32, 0.0, 0.6, 0.8, 0.0];
        let mut rng = Rng::new(5);
        let mut db = PoseDb { format: db_format(), version: 1, bones: (0..5).map(|i| format!("b{i}")).collect(), poses: vec![], meta: vec![], sequences: vec![SeqInfo { name: "s".into(), source: "library".into(), poses: 80 }] };
        for i in 0..80 {
            let (a, b) = (30.0 * (rng.unit() - 0.5), 20.0 * (rng.unit() - 0.5));
            db.poses.push((0..5).map(|j| 4.0 + a * u1[j] + b * u2[j]).collect());
            db.meta.push(PoseMeta { seq: 0, idx: i, frame: i as f32 });
        }
        let sg = Suggester::with_pca(db, 0.999, 5).unwrap();
        assert_eq!(sg.space.k(), 2);
        let truth = vec![4.0 + 10.0 * 0.6 + 5.0 * 0.0, 4.0 + 10.0 * 0.8, 4.0 + 5.0 * 0.6, 4.0 + 5.0 * 0.8, 4.0];
        let cons = vec![
            Constraint::Angle { bone: "b0".into(), value: truth[0], weight: 1.0 },
            Constraint::Angle { bone: "b3".into(), value: truth[3], weight: 1.0 },
        ];
        let c = sg.complete(None, &cons, &[0.0; 5], None, None, &[], 0.001, 3, None).unwrap();
        let best = c.iter().find(|x| x.method == "pca-ls").unwrap();
        assert!(rms(&best.pose, &truth) < 0.1, "recovered {:?} vs {:?}", best.pose, truth);
        assert!(best.constraint_error.unwrap() < 0.1);
        // the nearest-library baseline cannot be that exact
        let nn = c.iter().find(|x| x.method == "library-nn").unwrap();
        assert!(nn.constraint_error.unwrap() > best.constraint_error.unwrap());
        assert_eq!(c[0].method, "pca-ls", "ranked first");
        assert!(c.windows(2).all(|w| w[0].score >= w[1].score));
    }

    #[test]
    fn completion_with_a_reach_target_hits_it_and_respects_fixed_bones() {
        let p = arm_with_cycle();
        let sg = Suggester::new(build_db(Some(&p), None, Some(1.0)).unwrap()).unwrap();
        // the tip of the hand bone in a pose that is in the library
        let truth = pose_at(&p, &sg.db.bones, 7.0);
        let ang = |x: &[f32]| p.bones.iter().map(|b| x[sg.db.bones.iter().position(|n| *n == b.name).unwrap()]).collect::<Vec<f32>>();
        let target = engine_tip(&p, &ang(&truth), 2);
        let cons = vec![Constraint::Reach { bone: "h".into(), at: [target.x, target.y], weight: 1.0 }];
        let cur = pose_at(&p, &sg.db.bones, 0.0);
        let c = sg.complete(Some(&p), &cons, &cur, Some(&ang(&cur)), None, &[], 0.1, 5, None).unwrap();
        let best = &c[0];
        let reached = engine_tip(&p, &ang(&best.pose), 2);
        assert!(reached.distance(target) < 3.0, "reached {reached:?}, wanted {target:?} (error {:?})", best.constraint_error);
        assert!(best.plausibility > 0.3, "the result stays near the library: {}", best.plausibility);
        // fixing the upper arm to its current value: the free bones still reach as well as they can
        let free = vec!["l".to_string(), "h".to_string()];
        let c2 = sg.complete(Some(&p), &cons, &cur, Some(&ang(&cur)), Some(&free), &[], 0.1, 3, None).unwrap();
        assert!((c2[0].pose[0] - cur[0]).abs() < 0.2, "u stays at {} (got {})", cur[0], c2[0].pose[0]);
        // a locked bone is held too, and constraining it is refused
        let locked = vec!["l".to_string()];
        let c3 = sg.complete(Some(&p), &cons, &cur, Some(&ang(&cur)), None, &locked, 0.1, 3, None).unwrap();
        assert!((c3[0].pose[1] - cur[1]).abs() < 0.2);
        let bad = vec![Constraint::Angle { bone: "l".into(), value: 5.0, weight: 1.0 }];
        assert!(sg.complete(Some(&p), &bad, &cur, None, None, &locked, 0.1, 3, None).unwrap_err().contains("locked"));
        // angle limit of the hand (+-60) is honoured
        let far = vec![Constraint::Angle { bone: "h".into(), value: 170.0, weight: 1.0 }];
        let c4 = sg.complete(Some(&p), &far, &cur, Some(&ang(&cur)), None, &[], 0.1, 1, None).unwrap();
        assert!(c4[0].pose[2] <= 60.0 + 1e-3);
    }

    #[test]
    fn completion_validates_its_input() {
        let sg = Suggester::with_pca(circle_db(), 0.99, 4).unwrap();
        let cur = vec![0.0; 4];
        let a = |b: &str, v: f32, w: f32| vec![Constraint::Angle { bone: b.into(), value: v, weight: w }];
        assert!(sg.complete(None, &[], &cur, None, None, &[], 1.0, 3, None).is_err(), "no constraints");
        assert!(sg.complete(None, &a("zzz", 1.0, 1.0), &cur, None, None, &[], 1.0, 3, None).unwrap_err().contains("not in the pose database"));
        assert!(sg.complete(None, &a("b0", f32::NAN, 1.0), &cur, None, None, &[], 1.0, 3, None).is_err());
        assert!(sg.complete(None, &a("b0", 1.0, 0.0), &cur, None, None, &[], 1.0, 3, None).is_err());
        assert!(sg.complete(None, &a("b0", 1.0, 1.0), &cur[..2], None, None, &[], 1.0, 3, None).is_err());
        assert!(sg.complete(None, &a("b0", 1.0, 1.0), &cur, None, None, &[], -1.0, 3, None).is_err());
        assert!(sg.complete(None, &a("b0", 1.0, 1.0), &cur, None, None, &[], 1.0, 0, None).is_err());
        let reach = vec![Constraint::Reach { bone: "b0".into(), at: [1.0, 1.0], weight: 1.0 }];
        assert!(sg.complete(None, &reach, &cur, None, None, &[], 1.0, 3, None).unwrap_err().contains("needs the project"));
        let p = arm_with_cycle();
        let bad = vec![Constraint::Reach { bone: "nope".into(), at: [1.0, 1.0], weight: 1.0 }];
        let sg2 = Suggester::new(build_db(Some(&p), None, None).unwrap()).unwrap();
        assert!(sg2.complete(Some(&p), &bad, &[0.0; 3], None, None, &[], 1.0, 3, None).is_err());
    }

    #[test]
    fn next_pose_continues_the_motion_of_the_library() {
        let p = arm_with_cycle();
        let sg = Suggester::new(build_db(Some(&p), None, None).unwrap()).unwrap();
        // standing exactly on library pose 3: the best successor is pose 4 and the key goes 3 frames later
        let cur = sg.db.poses[3].clone();
        let c = sg.next(&cur, 20.0, 4, 5).unwrap();
        let truth = &sg.db.poses[4];
        let best = c.iter().map(|x| rms(&x.pose, truth)).fold(f32::MAX, f32::min);
        assert!(best < 3.0, "closest candidate is {best} deg from the true successor");
        let mean = c.iter().find(|x| x.method == "markov-mean").unwrap();
        assert!(rms(&mean.pose, truth) < 6.0, "{}", rms(&mean.pose, truth));
        assert!(rms(&mean.pose, &cur) > 3.0, "it moves");
        assert!((mean.frame.unwrap() - 23.0).abs() < 0.6, "gap of 3 frames: {:?}", mean.frame);
        assert!(c.windows(2).all(|w| w[0].score >= w[1].score));
        assert!(c.len() <= 5);
        // a pose far from everything gets a warning
        let far = vec![500.0, -500.0, 300.0];
        let c = sg.next(&far, 0.0, 4, 3).unwrap();
        assert!(c[0].notes.iter().any(|n| n.contains("library may not cover")));
        // errors
        assert!(sg.next(&cur[..2], 0.0, 4, 3).is_err());
        assert!(sg.next(&cur, 0.0, 0, 3).is_err());
        assert!(sg.next(&cur, 0.0, 4, 0).is_err());
        // a library of single poses has no successors
        let mut one = circle_db();
        one.sequences = (0..36).map(|i| SeqInfo { name: format!("s{i}"), source: "library".into(), poses: 1 }).collect();
        for (i, m) in one.meta.iter_mut().enumerate() {
            m.seq = i;
            m.idx = 0;
        }
        let sg = Suggester::with_pca(one, 0.99, 3).unwrap();
        assert!(sg.next(&[0.0; 4], 0.0, 4, 3).unwrap_err().contains("no consecutive"));
    }

    #[test]
    fn applying_is_one_undo_step_and_respects_locks() {
        let mut p = arm_with_cycle();
        p.frames = 20;
        let vals = vec![BoneValue { bone: "u".into(), angle: 12.5 }, BoneValue { bone: "l".into(), angle: -7.0 }];
        let mut ed = crate::edit::Editor::new(p.clone()).unwrap();
        let before = ed.doc().clone();
        let (op, skipped) = apply_op(ed.project(), &vals, 50.0, Ease::Smooth).unwrap();
        assert!(skipped.is_empty());
        ed.apply(op).unwrap();
        assert_eq!(ed.project().frames, 51, "the timeline grows to hold the key");
        let u = ed.project().channels.iter().find(|c| c.target == "u").unwrap();
        if let Source::Keys { track } = &u.source {
            assert_eq!(track.eval(50.0), Some(12.5));
            assert_eq!(track.keys.len(), 14);
        }
        assert!(ed.undo());
        assert_eq!(*ed.doc(), before, "one undo restores everything");
        assert!(ed.redo());
        // new channel for a bone without keys, plus a lock on another
        let mut q = arm();
        q.frames = 30;
        q.channel_meta.push(crate::chanmeta::ChannelMeta { target: "l".into(), prop: Some(Prop::BoneAngle), lock: true, ..Default::default() });
        q.channels.push(Channel::keys("l", Prop::BoneAngle, Track::with_keys(&[(0.0, 1.0, Ease::Linear)])));
        let (op, skipped) = apply_op(&q, &vals, 10.0, Ease::Linear).unwrap();
        assert_eq!(skipped, vec!["l".to_string()]);
        let mut ed = crate::edit::Editor::new(q.clone()).unwrap();
        ed.apply(op).unwrap();
        assert_eq!(ed.project().channels.len(), 2, "u got a channel, locked l did not change");
        let l = ed.project().channels.iter().find(|c| c.target == "l").unwrap();
        if let Source::Keys { track } = &l.source {
            assert_eq!(track.keys.len(), 1);
        }
        // everything locked: an error, nothing to apply
        let only_l = vec![BoneValue { bone: "l".into(), angle: 3.0 }];
        assert!(apply_op(&q, &only_l, 5.0, Ease::Linear).unwrap_err().contains("locked"));
        // bad input
        assert!(apply_op(&q, &[BoneValue { bone: "zz".into(), angle: 1.0 }], 5.0, Ease::Linear).is_err());
        assert!(apply_op(&q, &vals, -1.0, Ease::Linear).is_err());
        assert!(apply_op(&q, &vals, f32::NAN, Ease::Linear).is_err());
        assert!(apply_op(&q, &[BoneValue { bone: "u".into(), angle: f32::INFINITY }], 1.0, Ease::Linear).is_err());
    }

    #[test]
    fn request_layer_end_to_end_with_locks() {
        let mut p = arm_with_cycle();
        p.channel_meta.push(crate::chanmeta::ChannelMeta { target: "h".into(), prop: Some(Prop::BoneAngle), lock: true, ..Default::default() });
        let sg = Suggester::new(build_db(Some(&p), None, None).unwrap()).unwrap();
        let req = |json: &str| serde_json::from_str::<Request>(json).unwrap();
        let s = suggest(&p, &sg, &req(r#"{"kind":"between","a":0,"b":6,"t":0.5,"top":3}"#), None).unwrap();
        assert_eq!(s.kind, "between");
        assert!(s.candidates.len() <= 3 && !s.candidates.is_empty());
        assert_eq!(s.locked_skipped, vec!["h".to_string()]);
        assert!(s.candidates.iter().all(|c| c.values.iter().all(|v| v.bone != "h")));
        assert!((s.candidates[0].frame.unwrap() - 3.0).abs() < 1e-5);
        assert!(s.notes.iter().any(|n| n.contains("locked")));
        let s = suggest(&p, &sg, &req(r#"{"kind":"complete","frame":3,"constraints":[{"kind":"angle","bone":"u","value":20.0}]}"#), None).unwrap();
        assert!(s.candidates[0].constraint_error.unwrap() < 2.0);
        let s = suggest(&p, &sg, &req(r#"{"kind":"next","frame":6,"top":2}"#), None).unwrap();
        assert!(s.candidates.len() <= 2);
        assert!(s.candidates[0].frame.unwrap() > 6.0);
        // a free list limits what is written
        let s = suggest(&p, &sg, &req(r#"{"kind":"complete","frame":3,"free":["u","l"],"constraints":[{"kind":"angle","bone":"u","value":20.0}]}"#), None).unwrap();
        assert!(s.candidates[0].values.iter().all(|v| v.bone == "u" || v.bone == "l"));
        // errors
        assert!(suggest(&p, &sg, &req(r#"{"kind":"dance"}"#), None).is_err());
        assert!(suggest(&p, &sg, &req(r#"{"kind":"between","a":0}"#), None).unwrap_err().contains("b is required"));
        assert!(suggest(&p, &sg, &req(r#"{"kind":"next","frame":-2}"#), None).is_err());
        assert!(suggest(&p, &sg, &req(r#"{"kind":"complete","frame":0,"constraints":[]}"#), None).is_err());
        // a library bone the project lacks is reported
        let mut q = arm();
        q.bones.pop();
        q.channels = arm_with_cycle().channels.into_iter().take(2).collect();
        let s = suggest(&q, &sg, &req(r#"{"kind":"next","frame":0}"#), None).unwrap();
        assert!(s.notes.iter().any(|n| n.contains("not in this project")));
    }

    #[test]
    fn mlp_completion_trains_deterministically_and_learns_the_toy_mapping() {
        // poses of 4 bones lying on a line: b = [t, 2t, -t, 0.5 t]; any one observed bone fixes the rest
        let mut db = circle_db();
        db.poses = (0..60).map(|i| { let t = -30.0 + i as f32; vec![t, 2.0 * t, -t, 0.5 * t] }).collect();
        db.meta = (0..60).map(|i| PoseMeta { seq: 0, idx: i, frame: i as f32 }).collect();
        db.sequences = vec![SeqInfo { name: "line".into(), source: "library".into(), poses: 60 }];
        let o = TrainOpts { hidden: vec![24], epochs: 150, lr: 0.02, momentum: 0.9, batch: 8, seed: 2 };
        let (m, hist) = train_completion(&db, &o).unwrap();
        let (m2, _) = train_completion(&db, &o).unwrap();
        assert_eq!(m, m2, "deterministic");
        assert!(hist.last().unwrap() < &(hist[0] * 0.2), "loss {} -> {}", hist[0], hist.last().unwrap());
        let y = predict_completion(&m, &db.bones, &[Some(12.0), None, None, None]).unwrap();
        assert_eq!(y[0], 12.0, "observed values pass through");
        let want = [12.0, 24.0, -12.0, 6.0];
        assert!(rms(&y, &want) < 4.0, "predicted {y:?}");
        // far better than the mean pose
        let mean_err = rms(&[0.0, 0.0, 0.0, 0.0], &[0.0, 24.0, -12.0, 6.0]);
        assert!(rms(&y, &want) * 2.0 < mean_err);
        // bad models
        let mut other = m.clone();
        other.bones[0] = "zzz".into();
        assert!(predict_completion(&other, &db.bones, &[Some(1.0), None, None, None]).unwrap_err().contains("trained for bones"));
        let mut thin = m.clone();
        thin.layers[0].n_in = 3;
        thin.layers[0].w.truncate(24 * 3);
        assert!(predict_completion(&thin, &db.bones, &[Some(1.0), None, None, None]).is_err());
        // through the suggester as a candidate
        let sg = Suggester::with_pca(db.clone(), 0.99, 4).unwrap();
        let cons = vec![Constraint::Angle { bone: "b0".into(), value: 12.0, weight: 1.0 }];
        let c = sg.complete(None, &cons, &[0.0; 4], None, None, &[], 1.0, 2, Some(&m)).unwrap();
        assert!(c.iter().any(|x| x.method == "mlp"));
        // training errors
        let mut tiny = db.clone();
        tiny.poses.truncate(3);
        tiny.meta.truncate(3);
        assert!(train_completion(&tiny, &o).is_err());
        assert!(train_completion(&db, &TrainOpts { epochs: 0, ..o.clone() }).is_err());
    }

    #[test]
    fn suggester_scales_and_is_deterministic() {
        let db = synthetic_db(3, 12, 10, 8);
        let a = Suggester::new(db.clone()).unwrap();
        let b = Suggester::new(db).unwrap();
        assert_eq!(a.space.comps, b.space.comps);
        assert_eq!(a.spacing, b.spacing);
        assert!(a.space.k() >= 2 && a.space.k() <= 12);
        let c1 = a.between(&a.db.poses[0], &a.db.poses[2], 0.5, 0.5, 6).unwrap();
        let c2 = b.between(&b.db.poses[0], &b.db.poses[2], 0.5, 0.5, 6).unwrap();
        assert_eq!(c1.iter().map(|c| c.pose.clone()).collect::<Vec<_>>(), c2.iter().map(|c| c.pose.clone()).collect::<Vec<_>>());
    }

    #[test]
    fn benchmark_numbers_make_sense() {
        let quick = TrainOpts { hidden: vec![16], epochs: 20, lr: 0.02, momentum: 0.9, batch: 16, seed: 1 };
        let b = benchmark(1, &quick).unwrap();
        let get = |t: &str, m: &str| b.iter().find(|l| l.task == t && l.method == m).unwrap().rms_deg;
        // any learned method must beat pasting values into the mean pose, otherwise the library taught nothing
        assert!(get("completion", "PCA least squares") < get("completion", "mean pose") * 0.5);
        assert!(get("completion", "nearest library pose") < get("completion", "mean pose"));
        assert!(get("next", "Markov mean") < get("next", "stay in place"));
        assert!(b.iter().all(|l| l.rms_deg.is_finite()));
    }
}
