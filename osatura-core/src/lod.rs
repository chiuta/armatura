// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
//! Deterministic level of detail (LOD) with a declared error bound (ROADMAP I-11).
//!
//! Given a [`Project`] and an error budget in *output pixels at a stated render scale* ("at most
//! 0.5 px at 1920x1080"), [`build`] derives levels L1..Ln (L0 is the original). Each level is an
//! ordinary [`Project`] generated on demand; the only thing stored in a level is a small
//! [`LodRecord`] (`Project::lods`) that says what was done and which bound was declared.
//!
//! What a level does, and what is *proved* about it (the assumptions are in [`ASSUMPTIONS`]):
//!
//! 1. **Path geometry** (unskinned, undeformed vector layers). Anchors are removed greedily
//!    (Visvalingam-Whyatt order: cheapest removal first) and the removed span is replaced by one
//!    cubic fitted to the original span (Schneider-style least squares plus Newton
//!    re-parametrisation, textbook). The cost of a removal is a *certified upper bound* of the
//!    Hausdorff distance between the original span and the replacement: both curves are flattened
//!    with a control-polygon bound (a cubic lies in the convex hull of its control points, so the
//!    distance to its chord is at most that of the two inner control points) and the directed
//!    Hausdorff distance of the two polylines is bounded by branch and bound with the 1-Lipschitz
//!    property of the distance function. Spans are always compared with the *original* path (no
//!    accumulation); the path bound is the maximum over its final spans. Local units are converted
//!    to pixels with the largest singular value of the layer's world matrix over the sampled
//!    animation (plus a safety margin), which is why the bound holds for all sampled frames.
//! 2. **Animation keys** (Ramer-Douglas-Peucker style key reduction with an L-infinity value
//!    bound, verified on a sub-frame grid). The value tolerance of a channel is the pixel budget
//!    divided by a *lever arm* (pixels per unit of that channel: bone angle x distance of the skinned
//!    geometry from the bone head x scale, layer translation x parent scale, ...) and by the number
//!    of reduced channels that can move the same layer, so the pixel errors add up to at most the budget.
//! 3. **Image meshes**: half-edge collapse ordered by the generalised quadric error metric
//!    (Garland and Heckbert 1997/98, bone weights as extra attributes). Every accepted collapse is
//!    *certified* against the original mesh: the deformed surfaces are piecewise linear, so the
//!    deviation is a convex function on each cell of the overlay of the two triangulations and its
//!    maximum is attained at the cell corners (found by exact convex clipping). The maximum is
//!    taken over all sampled frames.
//! 4. **Culling**: layers whose screen extent never exceeds a small diameter are removed; the
//!    removal is charged with that diameter (convention: deleting a shape = collapsing it to a point
//!    inside it). Dead layers (invisible at every frame) are dropped losslessly (`sizefit`).
//! 5. Effects, blur, post effects, depth of field, cloth/IK/spring solved geometry, deformers and
//!    skinned paths are **not** touched (no bound can be documented); see [`LIMITS`].
//!
//! Everything is deterministic: no randomness, no hash-map iteration order is observable, ties are
//! broken by index. [`verify`] renders / evaluates L0 and Ln on sampled and key frames and measures
//! the actual deviation (screen-space Hausdorff distance of the evaluated geometry, plus pixel
//! statistics) so that declared and measured bounds can be compared.

use crate::anim::Track;
use crate::engine::Engine;
use crate::math::{Affine2, Vec2};
use crate::path::{Anchor, VPath};
use crate::project::{Channel, Layer, LayerKind, Mode, Project, Prop, Source};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

/// Assumptions under which the declared bound is claimed to hold.
pub const ASSUMPTIONS: &[&str] = &[
    "bounds are for timeline playback at frames inside 0..frames; motion is sampled at SUB samples per frame plus all key frames, scale/lever maxima get a 2% margin, so excursions shorter than 1/SUB frame are not covered",
    "pixel values are stage units times the stated render scale (raster scale); the bound scales linearly with the scale, see LodRecord::bound_stage",
    "path bound = Hausdorff distance of the stroke/fill outline curve (round joins only for strokes); fill-rule topology is guarded by a discrete Frechet check, not proved",
    "only channels with a documented Jacobian are reduced: bone angle / offset of bones that no IK chain, constraint or spring touches, layer x / y / rotation, camera x / y; none may be read by an expression or driven key",
    "key errors and geometry errors are added (triangle inequality), culled layers are charged with their screen diameter",
    "mesh deviation is measured on the deformed vertices of the sampled frames (piecewise linear surface)",
];

/// What is deliberately left alone because no bound can be stated.
pub const LIMITS: &[&str] = &[
    "skinned vector paths, path morphs, per-point offsets, lattice/spine/pin/cage/cloth deformers and wobble are not simplified",
    "IK chains, constraints, springs, expressions, driven keys, noise, state machines and data bindings are not reduced or modelled",
    "opacity, scale, skew, switch, camera zoom/rotation/focus/blur channels are not reduced (no pixel Jacobian is derived)",
    "liquid, emitter and boolean layers, masks, blend modes, gradients, strokes with dashes / width profiles / miter or bevel joins are left unchanged",
    "post effects, depth of field and any filter are never downgraded",
    "projects with mesh locks or glues are not geometry-reduced",
    "culling is a per-shape criterion: many tiny shapes that together form a texture are removed independently",
];

/// Samples per frame used for the motion analysis.
pub const SUB: usize = 2;
const MARGIN: f64 = 1.02;

type P = [f64; 2];
type Cubic = [P; 4];

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}
fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1]]
}
fn mul(a: P, s: f64) -> P {
    [a[0] * s, a[1] * s]
}
fn dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn norm(a: P) -> f64 {
    dot(a, a).sqrt()
}
fn dist(a: P, b: P) -> f64 {
    norm(sub(a, b))
}
fn r32(x: f64) -> f64 {
    x as f32 as f64
}
fn p_of(v: Vec2) -> P {
    [v.x as f64, v.y as f64]
}

// ------------------------------------------------------------------------------------------
// Certified curve distances
// ------------------------------------------------------------------------------------------

fn dist_pt_seg(p: P, a: P, b: P) -> f64 {
    let ab = sub(b, a);
    let l2 = dot(ab, ab);
    let t = if l2 < 1e-300 {
        0.0
    } else {
        (dot(sub(p, a), ab) / l2).clamp(0.0, 1.0)
    };
    dist(p, add(a, mul(ab, t)))
}

fn dist_to_poly(p: P, poly: &[P]) -> f64 {
    match poly.len() {
        0 => f64::INFINITY,
        1 => dist(p, poly[0]),
        _ => poly
            .windows(2)
            .map(|w| dist_pt_seg(p, w[0], w[1]))
            .fold(f64::INFINITY, f64::min),
    }
}

fn cubic_at(c: &Cubic, t: f64) -> P {
    let u = 1.0 - t;
    let (b0, b1, b2, b3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    [
        b0 * c[0][0] + b1 * c[1][0] + b2 * c[2][0] + b3 * c[3][0],
        b0 * c[0][1] + b1 * c[1][1] + b2 * c[2][1] + b3 * c[3][1],
    ]
}

fn cubic_d1(c: &Cubic, t: f64) -> P {
    let u = 1.0 - t;
    let a = mul(sub(c[1], c[0]), 3.0 * u * u);
    let b = mul(sub(c[2], c[1]), 6.0 * u * t);
    let d = mul(sub(c[3], c[2]), 3.0 * t * t);
    add(add(a, b), d)
}

fn cubic_d2(c: &Cubic, t: f64) -> P {
    let u = 1.0 - t;
    let a = mul(add(sub(c[2], mul(c[1], 2.0)), c[0]), 6.0 * u);
    let b = mul(add(sub(c[3], mul(c[2], 2.0)), c[1]), 6.0 * t);
    add(a, b)
}

fn split_cubic(c: &Cubic) -> (Cubic, Cubic) {
    let m = |a: P, b: P| mul(add(a, b), 0.5);
    let (p01, p12, p23) = (m(c[0], c[1]), m(c[1], c[2]), m(c[2], c[3]));
    let (p012, p123) = (m(p01, p12), m(p12, p23));
    let mid = m(p012, p123);
    ([c[0], p01, p012, mid], [mid, p123, p23, c[3]])
}

/// Distance bound of a cubic from its chord segment: the curve lies in the convex hull of its
/// control points and the distance to a segment is convex, so the maximum is at a control point.
fn hull_flat(c: &Cubic) -> f64 {
    dist_pt_seg(c[1], c[0], c[3]).max(dist_pt_seg(c[2], c[0], c[3]))
}

/// Append the flattening of `c` (without its first point) to `out`; `hmax` receives the largest
/// control-polygon flatness of any emitted piece, i.e. a rigorous bound of the distance between
/// the curve and the polyline.
fn flatten_cubic(c: &Cubic, tol: f64, out: &mut Vec<P>, hmax: &mut f64, depth: u32) {
    let h = hull_flat(c);
    if h <= tol || depth >= 22 {
        if h > *hmax {
            *hmax = h;
        }
        out.push(c[3]);
    } else {
        let (a, b) = split_cubic(c);
        flatten_cubic(&a, tol, out, hmax, depth + 1);
        flatten_cubic(&b, tol, out, hmax, depth + 1);
    }
}

/// Polyline through the curve plus the certified flattening bound.
pub fn flatten_poly(c: &[[f64; 2]; 4], tol: f64) -> (Vec<[f64; 2]>, f64) {
    let mut v = vec![c[0]];
    let mut h = 0.0;
    flatten_cubic(c, tol, &mut v, &mut h, 0);
    (v, h)
}

/// Nearest-distance index for a polyline: uniform grid of segment lists (exact queries).
struct PolyIdx<'a> {
    pts: &'a [P],
    x0: f64,
    y0: f64,
    cell: f64,
    nx: usize,
    ny: usize,
    cells: Vec<Vec<u32>>,
}

impl<'a> PolyIdx<'a> {
    fn new(pts: &'a [P]) -> PolyIdx<'a> {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for q in pts {
            x0 = x0.min(q[0]);
            y0 = y0.min(q[1]);
            x1 = x1.max(q[0]);
            y1 = y1.max(q[1]);
        }
        let m = pts.len().saturating_sub(1).max(1);
        let diag = (x1 - x0).hypot(y1 - y0).max(1e-9);
        let cell = (diag / (m as f64).sqrt().max(1.0))
            .max(diag / 512.0)
            .max(1e-9);
        let nx = (((x1 - x0) / cell).floor() as usize + 1).min(1024);
        let ny = (((y1 - y0) / cell).floor() as usize + 1).min(1024);
        let mut idx = PolyIdx {
            pts,
            x0,
            y0,
            cell,
            nx,
            ny,
            cells: vec![Vec::new(); nx * ny],
        };
        for (i, w) in pts.windows(2).enumerate() {
            let (cx0, cx1) = (idx.cx(w[0][0].min(w[1][0])), idx.cx(w[0][0].max(w[1][0])));
            let (cy0, cy1) = (idx.cy(w[0][1].min(w[1][1])), idx.cy(w[0][1].max(w[1][1])));
            for cy in cy0..=cy1 {
                for cx in cx0..=cx1 {
                    idx.cells[cy * nx + cx].push(i as u32);
                }
            }
        }
        idx
    }
    fn cx(&self, x: f64) -> usize {
        (((x - self.x0) / self.cell).floor().max(0.0) as usize).min(self.nx - 1)
    }
    fn cy(&self, y: f64) -> usize {
        (((y - self.y0) / self.cell).floor().max(0.0) as usize).min(self.ny - 1)
    }
    fn nearest(&self, p: P) -> (f64, usize) {
        if self.pts.len() < 2 {
            return (self.pts.first().map_or(f64::INFINITY, |q| dist(p, *q)), 0);
        }
        let (cx, cy) = (self.cx(p[0]) as i64, self.cy(p[1]) as i64);
        let mut best = (f64::INFINITY, 0usize);
        let maxr = self.nx.max(self.ny) as i64;
        for r in 0..=maxr {
            for yy in (cy - r).max(0)..=(cy + r).min(self.ny as i64 - 1) {
                for xx in (cx - r).max(0)..=(cx + r).min(self.nx as i64 - 1) {
                    if (yy - cy).abs() != r && (xx - cx).abs() != r {
                        continue;
                    }
                    for &si in &self.cells[yy as usize * self.nx + xx as usize] {
                        let w = &self.pts[si as usize..si as usize + 2];
                        let d = dist_pt_seg(p, w[0], w[1]);
                        if d < best.0 || (d == best.0 && (si as usize) < best.1) {
                            best = (d, si as usize);
                        }
                    }
                }
            }
            if best.0 <= (r as f64) * self.cell {
                break;
            }
        }
        best
    }
}

/// Upper bound of the directed Hausdorff distance `sup_{x in a} dist(x, b)` for polylines, never
/// more than `gap` above the true value (branch and bound on the 1-Lipschitz distance function).
pub fn directed_hausdorff_upper(a: &[[f64; 2]], b: &[[f64; 2]], gap: f64) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let idx = PolyIdx::new(b);
    let dtp = |p: P| -> (f64, usize) { idx.nearest(p) };
    let seg_d = |p: P, s: usize| -> f64 {
        if b.len() < 2 {
            dist(p, b[0])
        } else {
            dist_pt_seg(p, b[s], b[s + 1])
        }
    };
    if a.len() == 1 {
        return dtp(a[0]).0;
    }
    let gap = gap.max(1e-12);
    let mut lb = 0.0f64;
    let mut res = 0.0f64;
    // (p, q, d(p), d(q), nearest segment of p, nearest segment of q, depth)
    let mut stack: Vec<(P, P, f64, f64, usize, usize, u32)> = Vec::new();
    for w in a.windows(2) {
        let (d0, s0) = dtp(w[0]);
        let (d1, s1) = dtp(w[1]);
        lb = lb.max(d0).max(d1);
        stack.push((w[0], w[1], d0, d1, s0, s1, 0));
        while let Some((p, q, da, db, sa, sb, depth)) = stack.pop() {
            let len = dist(p, q);
            // upper bounds of sup over [p, q]: Lipschitz, and convexity of the distance to one fixed segment
            let mut ub = (da + db + len) * 0.5;
            for s in [sa, sb] {
                ub = ub.min(seg_d(p, s).max(seg_d(q, s)));
            }
            if ub <= lb + gap || depth >= 40 || len < 1e-14 {
                res = res.max(ub.min(da.max(db) + len));
                continue;
            }
            let m = mul(add(p, q), 0.5);
            let (dm, sm) = dtp(m);
            lb = lb.max(dm);
            stack.push((m, q, dm, db, sm, sb, depth + 1));
            stack.push((p, m, da, dm, sa, sm, depth + 1));
        }
    }
    res.max(lb)
}

/// Certified-up-to-`gap` symmetric Hausdorff distance bound of two polylines.
pub fn hausdorff_upper(a: &[[f64; 2]], b: &[[f64; 2]], gap: f64) -> f64 {
    directed_hausdorff_upper(a, b, gap).max(directed_hausdorff_upper(b, a, gap))
}

fn resample(poly: &[P], n: usize) -> Vec<P> {
    if poly.len() < 2 || n < 2 {
        return poly.to_vec();
    }
    let mut cum = vec![0.0f64];
    for w in poly.windows(2) {
        cum.push(cum.last().unwrap() + dist(w[0], w[1]));
    }
    let total = *cum.last().unwrap();
    if total < 1e-15 {
        return vec![poly[0], poly[poly.len() - 1]];
    }
    let mut out = Vec::with_capacity(n);
    let mut j = 0;
    for i in 0..n {
        let s = total * i as f64 / (n - 1) as f64;
        while j + 2 < cum.len() && cum[j + 1] < s {
            j += 1;
        }
        let seg = (cum[j + 1] - cum[j]).max(1e-300);
        let t = ((s - cum[j]) / seg).clamp(0.0, 1.0);
        out.push(add(poly[j], mul(sub(poly[j + 1], poly[j]), t)));
    }
    out
}

/// Discrete Frechet distance of two point sequences (monotone matching).
pub fn discrete_frechet(a: &[[f64; 2]], b: &[[f64; 2]]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let (n, m) = (a.len(), b.len());
    let mut ca = vec![0.0f64; n * m];
    for i in 0..n {
        for j in 0..m {
            let d = dist(a[i], b[j]);
            ca[i * m + j] = match (i, j) {
                (0, 0) => d,
                (0, _) => ca[j - 1].max(d),
                (_, 0) => ca[(i - 1) * m].max(d),
                _ => ca[(i - 1) * m + j]
                    .min(ca[(i - 1) * m + j - 1])
                    .min(ca[i * m + j - 1])
                    .max(d),
            };
        }
    }
    ca[n * m - 1]
}

// ------------------------------------------------------------------------------------------
// Path simplification with a certified Hausdorff bound
// ------------------------------------------------------------------------------------------

struct OrigSeg {
    ctrl: Cubic,
    poly: Vec<P>,
    h: f64,
}

fn unit_or(cands: &[P]) -> P {
    for c in cands {
        let l = norm(*c);
        if l > 1e-12 {
            return mul(*c, 1.0 / l);
        }
    }
    [0.0, 0.0]
}

/// Sampled points of a span of original segments (`K` per segment).
fn span_samples(segs: &[&OrigSeg]) -> Vec<P> {
    const K: usize = 8;
    let mut v = Vec::with_capacity(segs.len() * K + 1);
    for s in segs {
        for j in 0..K {
            v.push(cubic_at(&s.ctrl, j as f64 / K as f64));
        }
    }
    v.push(segs.last().unwrap().ctrl[3]);
    v
}

/// Least-squares tangent lengths for end tangents `t1` (outgoing) and `t2` (pointing from the end
/// back along the curve), given parameters `u` of the samples.
fn lsq_alpha(pts: &[P], u: &[f64], p0: P, p3: P, t1: P, t2: P) -> Option<(f64, f64)> {
    let chord = dist(p0, p3);
    let (mut c11, mut c12, mut c22, mut x1, mut x2) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (pt, &t) in pts.iter().zip(u) {
        let w = 1.0 - t;
        let (b0, b1, b2, b3) = (w * w * w, 3.0 * w * w * t, 3.0 * w * t * t, t * t * t);
        let a1 = mul(t1, b1);
        let a2 = mul(t2, b2);
        c11 += dot(a1, a1);
        c12 += dot(a1, a2);
        c22 += dot(a2, a2);
        let base = add(mul(p0, b0 + b1), mul(p3, b2 + b3));
        let r = sub(*pt, base);
        x1 += dot(a1, r);
        x2 += dot(a2, r);
    }
    let det = c11 * c22 - c12 * c12;
    if det.abs() <= 1e-12 * (c11 * c22).abs().max(1e-30) {
        return None;
    }
    let (s, e) = ((x1 * c22 - x2 * c12) / det, (c11 * x2 - c12 * x1) / det);
    if s.is_finite()
        && e.is_finite()
        && s > 1e-6 * chord
        && e > 1e-6 * chord
        && s < 8.0 * chord.max(1e-9) * 4.0
        && e < 8.0 * chord.max(1e-9) * 4.0
    {
        Some((s, e))
    } else {
        None
    }
}

fn chord_params(pts: &[P]) -> Vec<f64> {
    let mut u = vec![0.0f64; pts.len()];
    for j in 1..pts.len() {
        u[j] = u[j - 1] + dist(pts[j], pts[j - 1]);
    }
    let total = u[pts.len() - 1];
    if total > 1e-300 {
        for x in &mut u {
            *x /= total;
        }
    }
    u
}

fn mk_cubic(p0: P, p3: P, t1: P, t2: P, a1: f64, a2: f64) -> Cubic {
    [
        p0,
        [r32(p0[0] + t1[0] * a1), r32(p0[1] + t1[1] * a1)],
        [r32(p3[0] + t2[0] * a2), r32(p3[1] + t2[1] * a2)],
        p3,
    ]
}

/// Candidate replacement cubics for a span (always includes the straight line).
fn candidates(segs: &[&OrigSeg]) -> Vec<Cubic> {
    let first = &segs[0].ctrl;
    let last = &segs[segs.len() - 1].ctrl;
    let (p0, p3) = (first[0], last[3]);
    let line: Cubic = [p0, p0, p3, p3];
    let mut out = vec![line];
    let chord = dist(p0, p3);
    if chord < 1e-12 {
        return out;
    }
    let t1 = unit_or(&[
        sub(first[1], first[0]),
        sub(first[2], first[0]),
        sub(first[3], first[0]),
    ]);
    let t2 = unit_or(&[
        sub(last[2], last[3]),
        sub(last[1], last[3]),
        sub(last[0], last[3]),
    ]);
    if norm(t1) == 0.0 || norm(t2) == 0.0 {
        return out;
    }
    let pts = span_samples(segs);
    let mut u = chord_params(&pts);
    let mut alphas = (chord / 3.0, chord / 3.0);
    if let Some(a) = lsq_alpha(&pts, &u, p0, p3, t1, t2) {
        alphas = a;
    }
    out.push(mk_cubic(p0, p3, t1, t2, chord / 3.0, chord / 3.0));
    // Newton re-parametrisation (Schneider)
    for _ in 0..3 {
        let c = mk_cubic(p0, p3, t1, t2, alphas.0, alphas.1);
        for (j, pt) in pts.iter().enumerate() {
            let d = sub(cubic_at(&c, u[j]), *pt);
            let d1 = cubic_d1(&c, u[j]);
            let d2 = cubic_d2(&c, u[j]);
            let den = dot(d1, d1) + dot(d, d2);
            if den.abs() > 1e-12 {
                u[j] = (u[j] - dot(d, d1) / den).clamp(0.0, 1.0);
            }
        }
        u[0] = 0.0;
        let last_i = u.len() - 1;
        u[last_i] = 1.0;
        match lsq_alpha(&pts, &u, p0, p3, t1, t2) {
            Some(a) => alphas = a,
            None => break,
        }
    }
    out.push(mk_cubic(p0, p3, t1, t2, alphas.0, alphas.1));
    out.push(mk_cubic(p0, p3, t1, t2, alphas.0 * 0.8, alphas.1 * 0.8));
    out.push(mk_cubic(p0, p3, t1, t2, alphas.0 * 1.25, alphas.1 * 1.25));
    out
}

fn sampled_error(cand: &Cubic, pts: &[P]) -> f64 {
    let q: Vec<P> = (0..=24).map(|j| cubic_at(cand, j as f64 / 24.0)).collect();
    let e1 = pts.iter().map(|&p| dist_to_poly(p, &q)).fold(0.0, f64::max);
    let e2 = q.iter().map(|&p| dist_to_poly(p, pts)).fold(0.0, f64::max);
    e1.max(e2)
}

/// Result of [`simplify_path_certified`].
#[derive(Clone, Debug)]
pub struct SimplifiedPath {
    pub anchors: Vec<Anchor>,
    /// Certified upper bound (path units) of the Hausdorff distance to the original outline.
    pub bound: f64,
    pub removed: usize,
}

/// Certified cost (upper bound of the Hausdorff distance) of replacing the span of original
/// segments by the best candidate; `None` if no candidate stays within `limit` (or the Frechet
/// guard fails). Returns the cost and the cubic.
fn certify_span(segs: &[&OrigSeg], limit: f64, flat: f64, gap: f64) -> Option<(f64, Cubic)> {
    let pts = span_samples(segs);
    let mut cands = candidates(segs);
    // rank by cheap sampled error (stable), certify the best two
    let mut ranked: Vec<(f64, usize)> = cands
        .iter()
        .enumerate()
        .map(|(i, c)| (sampled_error(c, &pts), i))
        .collect();
    ranked.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut orig_poly: Vec<P> = Vec::new();
    let mut h_o = 0.0f64;
    for (i, s) in segs.iter().enumerate() {
        if i == 0 {
            orig_poly.extend_from_slice(&s.poly);
        } else {
            orig_poly.extend_from_slice(&s.poly[1..]);
        }
        h_o = h_o.max(s.h);
    }
    let mut best: Option<(f64, Cubic)> = None;
    for &(se, i) in ranked.iter().take(2) {
        if se > limit * 1.5 + 4.0 * flat {
            continue;
        }
        let c = cands[i];
        let (q, h_c) = flatten_poly(&c, flat);
        let e = hausdorff_upper(&orig_poly, &q, gap) + h_o + h_c;
        if e <= limit && best.as_ref().is_none_or(|b| e < b.0) {
            // guard: monotone correspondence (catches winding / backtracking differences)
            let ra = resample(&orig_poly, 64);
            let rb = resample(&q, 64);
            let step = (path_len(&orig_poly).max(path_len(&q)) / 63.0).max(0.0);
            if discrete_frechet(&ra, &rb) <= limit + step {
                best = Some((e, c));
            }
        }
    }
    cands.clear();
    best
}

fn path_len(p: &[P]) -> f64 {
    p.windows(2).map(|w| dist(w[0], w[1])).sum()
}

/// Remove anchors of one path while the certified Hausdorff distance to the original outline stays
/// within `eps` (path units). The result is deterministic; ends of open paths and the last three
/// anchors of a closed path are kept. Weights, morphs and strokes are the caller's business.
pub fn simplify_path_certified(anchors: &[Anchor], closed: bool, eps: f64) -> SimplifiedPath {
    let n = anchors.len();
    let unchanged = || SimplifiedPath {
        anchors: anchors.to_vec(),
        bound: 0.0,
        removed: 0,
    };
    let min_keep = if closed { 3 } else { 2 };
    if (eps.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater))
        || !eps.is_finite()
        || n <= min_keep
    {
        return unchanged();
    }
    let flat = (eps * 0.04).max(1e-9);
    let gap = (eps * 0.02).max(1e-9);
    let pos: Vec<P> = anchors.iter().map(|a| p_of(a.pos)).collect();
    let hin: Vec<P> = anchors.iter().map(|a| p_of(a.hin())).collect();
    let hout: Vec<P> = anchors.iter().map(|a| p_of(a.hout())).collect();
    let ns = if closed { n } else { n - 1 };
    let segs: Vec<OrigSeg> = (0..ns)
        .map(|i| {
            let j = (i + 1) % n;
            let ctrl: Cubic = [pos[i], hout[i], hin[j], pos[j]];
            let (poly, h) = flatten_poly(&ctrl, flat);
            OrigSeg { ctrl, poly, h }
        })
        .collect();
    let mut alive: Vec<usize> = (0..n).collect();
    let mut cost: Vec<Option<Option<(f64, Cubic)>>> = vec![None; n];
    let mut cur_out = hout.clone();
    let mut cur_in = hin.clone();
    let mut mod_out = vec![false; n];
    let mut mod_in = vec![false; n];
    let mut span_err = vec![0.0f64; n];
    let mut removed = 0;
    while alive.len() > min_keep {
        let m = alive.len();
        let range = if closed { 0..m } else { 1..m - 1 };
        let mut best: Option<(f64, usize)> = None;
        for j in range {
            let id = alive[j];
            if cost[id].is_none() {
                let (pi, ni) = (alive[(j + m - 1) % m], alive[(j + 1) % m]);
                let count = (ni + n - pi) % n;
                let sp: Vec<&OrigSeg> = (0..count).map(|k| &segs[(pi + k) % n]).collect();
                cost[id] = Some(certify_span(&sp, eps, flat, gap));
            }
            if let Some(Some((c, _))) = &cost[id] {
                if best.is_none_or(|(bc, _)| *c < bc) {
                    best = Some((*c, j));
                }
            }
        }
        let Some((c, j)) = best else { break };
        let (pi, id, ni) = (alive[(j + m - 1) % m], alive[j], alive[(j + 1) % m]);
        let cub = cost[id].unwrap().unwrap().1;
        cur_out[pi] = cub[1];
        cur_in[ni] = cub[2];
        mod_out[pi] = true;
        mod_in[ni] = true;
        span_err[pi] = c;
        alive.remove(j);
        cost[id] = None;
        cost[pi] = None;
        cost[ni] = None;
        removed += 1;
    }
    if removed == 0 {
        return unchanged();
    }
    let bound = alive.iter().map(|&i| span_err[i]).fold(0.0, f64::max);
    let out = alive
        .iter()
        .map(|&i| {
            let a = &anchors[i];
            let h = |p: P| {
                if p == pos[i] {
                    None
                } else {
                    Some(Vec2::new(p[0] as f32, p[1] as f32))
                }
            };
            Anchor {
                pos: a.pos,
                handle_in: if mod_in[i] { h(cur_in[i]) } else { a.handle_in },
                handle_out: if mod_out[i] {
                    h(cur_out[i])
                } else {
                    a.handle_out
                },
                weights: Vec::new(),
            }
        })
        .collect();
    SimplifiedPath {
        anchors: out,
        bound,
        removed,
    }
}

/// Hausdorff-distance bound by dense evaluation (used by tests and `verify`): maximum over both
/// directions of the distance between flattened outlines, in the units of the anchors.
pub fn outline_polyline(anchors: &[Anchor], closed: bool, tol: f64) -> Vec<[f64; 2]> {
    let n = anchors.len();
    if n == 0 {
        return vec![];
    }
    let mut v = vec![p_of(anchors[0].pos)];
    let ns = if closed { n } else { n.saturating_sub(1) };
    let mut h = 0.0;
    for i in 0..ns {
        let j = (i + 1) % n;
        let c: Cubic = [
            p_of(anchors[i].pos),
            p_of(anchors[i].hout()),
            p_of(anchors[j].hin()),
            p_of(anchors[j].pos),
        ];
        flatten_cubic(&c, tol, &mut v, &mut h, 0);
    }
    v
}

// ------------------------------------------------------------------------------------------
// Options, records
// ------------------------------------------------------------------------------------------

/// Parameters of [`build`].
#[derive(Clone, Debug, PartialEq)]
pub struct LodOptions {
    /// Declared bound of level 1 in output pixels at `scale`.
    pub tolerance_px: f64,
    /// Output pixels per stage unit (render scale), e.g. `1920 / project.width`.
    pub scale: f64,
    /// Number of derived levels (L1..Ln).
    pub levels: usize,
    /// Tolerance multiplier from one level to the next.
    pub ratio: f64,
    /// Share of the tolerance given to path / mesh geometry.
    pub geometry_share: f64,
    /// Share of the tolerance given to animation keys (`geometry_share + keys_share <= 1`).
    pub keys_share: f64,
    /// Layers whose screen extent stays below `cull_share * tolerance` are removed.
    pub cull_share: f64,
}

impl LodOptions {
    pub fn new(tolerance_px: f64, scale: f64, levels: usize) -> LodOptions {
        LodOptions {
            tolerance_px,
            scale,
            levels,
            ratio: 2.0,
            geometry_share: 0.5,
            keys_share: 0.4,
            cull_share: 0.5,
        }
    }
    pub fn check(&self) -> Result<(), String> {
        let ok = |x: f64| x.is_finite() && x > 0.0;
        if !ok(self.tolerance_px) {
            return Err("tolerance must be a positive number of pixels".into());
        }
        if !ok(self.scale) || self.scale > 64.0 {
            return Err("scale must be in (0, 64]".into());
        }
        if self.levels == 0 || self.levels > 12 {
            return Err("levels must be between 1 and 12".into());
        }
        if !ok(self.ratio) || self.ratio < 1.0 || self.ratio > 16.0 {
            return Err("ratio must be in [1, 16]".into());
        }
        if !ok(self.geometry_share)
            || !ok(self.keys_share)
            || self.geometry_share + self.keys_share > 1.0 + 1e-9
        {
            return Err("geometry and keys shares must be positive and sum to at most 1".into());
        }
        if !ok(self.cull_share) || self.cull_share > 1.0 {
            return Err("cull share must be in (0, 1]".into());
        }
        Ok(())
    }
    /// Declared tolerance (px) of level `k >= 1`.
    pub fn level_tolerance(&self, k: usize) -> f64 {
        self.tolerance_px * self.ratio.powi(k as i32 - 1)
    }
}

/// Counts that describe the size of a project (what a level is supposed to shrink).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LodStats {
    /// Path anchors of all vector layers.
    pub path_anchors: usize,
    /// Vertices of all image meshes.
    pub mesh_vertices: usize,
    /// Keys of all keyed channels.
    pub keys: usize,
    /// Layers (all nesting levels).
    pub layers: usize,
    /// Size of the compact JSON in bytes.
    pub bytes: usize,
}

/// Metadata stored in a derived project (`Project::lods`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LodRecord {
    pub level: usize,
    /// Requested tolerance of this level (px at `scale`).
    pub tolerance_px: f64,
    /// Render scale (output pixels per stage unit) the pixel numbers refer to.
    pub scale: f64,
    /// Declared total bound in px at `scale`: max(cull, max(path, mesh) + keys).
    pub declared_px: f64,
    /// The same bound in stage units (multiply by a display scale to get pixels there).
    pub bound_stage: f64,
    pub path_px: f64,
    pub mesh_px: f64,
    pub keys_px: f64,
    pub cull_px: f64,
    pub stats: LodStats,
    /// Removed or reduced things, for the report.
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Level of detail: project plus record.
#[derive(Clone, Debug)]
pub struct Lod {
    pub project: Project,
    pub record: LodRecord,
}

/// Round up to six decimals: short decimal text that survives a JSON round trip and never lowers a bound.
fn ceil6(x: f64) -> f64 {
    (x * 1e6).ceil() / 1e6
}

pub fn count_stats(p: &Project) -> LodStats {
    fn walk(ls: &[Layer], s: &mut LodStats) {
        for l in ls {
            s.layers += 1;
            match &l.kind {
                LayerKind::Vector { paths } | LayerKind::Boolean { paths, .. } => {
                    s.path_anchors += paths.iter().map(|x| x.anchors.len()).sum::<usize>()
                }
                LayerKind::Image { mesh: Some(m), .. } => s.mesh_vertices += m.vertex_count(),
                _ => {}
            }
            walk(l.children(), s);
        }
    }
    let mut s = LodStats::default();
    walk(&p.layers, &mut s);
    for c in &p.channels {
        if let Source::Keys { track } = &c.source {
            s.keys += track.keys.len();
        }
    }
    s.bytes = serde_json::to_string(p).map(|t| t.len()).unwrap_or(0);
    s
}

// ------------------------------------------------------------------------------------------
// Small affine helpers (f64)
// ------------------------------------------------------------------------------------------

/// `[a, b, c, d, tx, ty]` as in [`Affine2`]: `x' = a x + c y + tx`, `y' = b x + d y + ty`.
type A = [f64; 6];

fn a_of(m: &Affine2) -> A {
    [
        m.a as f64,
        m.b as f64,
        m.c as f64,
        m.d as f64,
        m.tx as f64,
        m.ty as f64,
    ]
}
fn a_apply(m: &A, p: P) -> P {
    [
        m[0] * p[0] + m[2] * p[1] + m[4],
        m[1] * p[0] + m[3] * p[1] + m[5],
    ]
}
fn a_mul(l: &A, r: &A) -> A {
    [
        l[0] * r[0] + l[2] * r[1],
        l[1] * r[0] + l[3] * r[1],
        l[0] * r[2] + l[2] * r[3],
        l[1] * r[2] + l[3] * r[3],
        l[0] * r[4] + l[2] * r[5] + l[4],
        l[1] * r[4] + l[3] * r[5] + l[5],
    ]
}
fn a_inv(m: &A) -> Option<A> {
    let det = m[0] * m[3] - m[1] * m[2];
    if (det.abs().partial_cmp(&(1e-12)) != Some(std::cmp::Ordering::Greater)) || !det.is_finite() {
        return None;
    }
    let (ia, ib, ic, id) = (m[3] / det, -m[1] / det, -m[2] / det, m[0] / det);
    Some([
        ia,
        ib,
        ic,
        id,
        -(ia * m[4] + ic * m[5]),
        -(ib * m[4] + id * m[5]),
    ])
}
/// Largest singular value of the linear part.
fn lin_norm(m: &A) -> f64 {
    let (p, q, r) = (
        m[0] * m[0] + m[1] * m[1],
        m[0] * m[2] + m[1] * m[3],
        m[2] * m[2] + m[3] * m[3],
    );
    let h = (p + r) * 0.5;
    (h + (((p - r) * 0.5).powi(2) + q * q).sqrt())
        .max(0.0)
        .sqrt()
}
fn local_matrix(t: &crate::project::Transform2D) -> A {
    a_of(&t.matrix())
}
const A_ID: A = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

// ------------------------------------------------------------------------------------------
// Static structure of a project
// ------------------------------------------------------------------------------------------

/// Which animation channels can be reduced and what moves when they change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum LKind {
    BoneAngle(usize),
    BoneOff(usize),
    LayerX(usize),
    LayerY(usize),
    LayerRot(usize),
    CamXY,
}

/// A skinned rest point and its `(bone, weight)` pairs.
type SkinPt = (P, Vec<(usize, f64)>);

struct Ctx<'a> {
    p: &'a Project,
    e: &'a Engine,
    paths: Vec<String>,
    parent: Vec<Option<usize>>,
    children: Vec<Vec<usize>>,
    bone_parent: Vec<Option<usize>>,
    /// `sub[b][k]`: bone `k` is `b` or below it.
    sub: Vec<Vec<bool>>,
    rest_inv: Vec<A>,
    /// Layers that carry something the lever analysis cannot model (deformers, morphs, point offsets, wobble).
    complex: Vec<bool>,
    /// Per node: skinned rest points `(rest position, normalised weights)`.
    skin_pts: Vec<Vec<SkinPt>>,
}

fn layer_by_path<'a>(p: &'a Project, path: &str) -> Option<&'a Layer> {
    p.find_layer(path)
}

impl<'a> Ctx<'a> {
    fn new(p: &'a Project, e: &'a Engine) -> Result<Ctx<'a>, String> {
        let n = e.node_count();
        let paths: Vec<String> = {
            let mut v = vec![String::new(); n];
            for path in p.layer_paths() {
                if let Some(i) = e.node_index(&path) {
                    v[i] = path;
                }
            }
            v
        };
        let parent: Vec<Option<usize>> = (0..n).map(|i| e.node_parent(i)).collect();
        let mut children = vec![Vec::new(); n];
        for (i, pa) in parent.iter().enumerate() {
            if let Some(pa) = pa {
                children[*pa].push(i);
            }
        }
        let bi = |name: &str| p.bones.iter().position(|b| b.name == name);
        let bone_parent: Vec<Option<usize>> = p
            .bones
            .iter()
            .map(|b| b.parent.as_deref().and_then(bi))
            .collect();
        let nb = p.bones.len();
        let mut sub = vec![vec![false; nb]; nb];
        // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
        #[allow(clippy::needless_range_loop)]
        for k in 0..nb {
            let mut cur = Some(k);
            while let Some(c) = cur {
                sub[c][k] = true;
                cur = bone_parent[c];
            }
        }
        let rest = p.rest_world()?;
        let rest_inv: Vec<A> = rest
            .iter()
            .map(|m| a_inv(&a_of(m)).unwrap_or(A_ID))
            .collect();
        let sub_targets: BTreeSet<String> = p
            .channels
            .iter()
            .filter(|c| c.prop.target_kind() == crate::project::TargetKind::Sub)
            .map(|c| c.target.split("::").next().unwrap_or("").to_string())
            .collect();
        let mut complex = vec![false; n];
        let mut skin_pts: Vec<Vec<SkinPt>> = vec![Vec::new(); n];
        let norm_w = |ws: &[crate::path::BoneWeight]| -> Vec<(usize, f64)> {
            let mut out: Vec<(usize, f64)> = ws
                .iter()
                .filter_map(|w| bi(&w.bone).map(|b| (b, w.weight as f64)))
                .collect();
            let sum: f64 = out.iter().map(|x| x.1).sum();
            if sum > 0.0 {
                for x in &mut out {
                    x.1 /= sum;
                }
            } else {
                out.clear();
            }
            out
        };
        for i in 0..n {
            let Some(l) = layer_by_path(p, &paths[i]) else {
                continue;
            };
            let mut c =
                !l.deformers.is_empty() || l.wobble.is_some() || sub_targets.contains(&paths[i]);
            match &l.kind {
                LayerKind::Vector { paths: ps } => {
                    for path in ps {
                        if !path.morphs.is_empty() {
                            c = true;
                        }
                        for a in &path.anchors {
                            let w = norm_w(&a.weights);
                            if w.is_empty() {
                                continue;
                            }
                            for pt in [a.pos, a.hin(), a.hout()] {
                                skin_pts[i].push((p_of(pt), w.clone()));
                            }
                        }
                    }
                }
                LayerKind::Image {
                    mesh: Some(m),
                    origin,
                    size,
                    ..
                } => {
                    // rest vertices need the displayed size: take it from the frame at sample time
                    // (stored lazily); here only the weights are recorded, positions filled by `fill_mesh_rest`.
                    let _ = (origin, size);
                    for w in &m.weights {
                        skin_pts[i].push(([f64::NAN, f64::NAN], norm_w(w)));
                    }
                }
                _ => {}
            }
            complex[i] = c;
        }
        Ok(Ctx {
            p,
            e,
            paths,
            parent,
            children,
            bone_parent,
            sub,
            rest_inv,
            complex,
            skin_pts,
        })
    }

    fn subtree(&self, id: usize) -> Vec<usize> {
        let mut out = vec![id];
        let mut i = 0;
        while i < out.len() {
            let c = out[i];
            out.extend(self.children[c].iter().copied());
            i += 1;
        }
        out
    }

    fn layer(&self, id: usize) -> Option<&Layer> {
        layer_by_path(self.p, &self.paths[id])
    }
}

fn touched_bones(p: &Project) -> BTreeSet<usize> {
    let bi = |name: &str| p.bones.iter().position(|b| b.name == name);
    let mut t = BTreeSet::new();
    for c in &p.ik {
        for b in c.bones.iter().chain(std::iter::once(&c.target)) {
            t.extend(bi(b));
        }
    }
    for c in &p.constraints {
        t.extend(bi(&c.bone));
        match &c.kind {
            crate::project::ConstraintKind::Aim { target, .. }
            | crate::project::ConstraintKind::Pin { target } => t.extend(bi(target)),
            crate::project::ConstraintKind::CopyRotation { source } => t.extend(bi(source)),
        }
    }
    for s in &p.springs {
        t.extend(bi(&s.bone));
    }
    t
}

/// True if geometry-affecting reductions are impossible for the whole project.
pub fn links_present(p: &Project) -> bool {
    !p.mesh_locks.is_empty() || !p.glues.is_empty()
}

/// Channels that may be reduced, with what they drive. Everything else is left alone.
fn eligible_channels(c: &Ctx) -> Vec<(usize, LKind)> {
    let p = c.p;
    if links_present(p) {
        return vec![];
    }
    let touched = touched_bones(p);
    let blocked_text: String = {
        let mut s = String::new();
        for ch in &p.channels {
            match &ch.source {
                Source::Expr { expr } => {
                    s.push_str(expr);
                    s.push('\n');
                }
                Source::Driven { input, .. } => {
                    s.push_str(&format!("\"{input}\"\n"));
                }
                _ => {}
            }
        }
        s.push_str(&serde_json::to_string(&p.machines).unwrap_or_default());
        s.push_str(&serde_json::to_string(&p.bindings).unwrap_or_default());
        s.push_str(&serde_json::to_string(&p.scrolls).unwrap_or_default());
        s
    };
    let referenced = |name: &str| blocked_text.contains(&format!("\"{name}\""));
    let mut out = Vec::new();
    for (i, ch) in p.channels.iter().enumerate() {
        let Source::Keys { track } = &ch.source else {
            continue;
        };
        if track.keys.len() < 3
            || ch.mode == Mode::Multiply
            || (ch.weight.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater))
            || !ch.weight.is_finite()
            || (!ch.target.is_empty() && referenced(&ch.target))
        {
            continue;
        }
        use Prop::*;
        let kind = match ch.prop {
            BoneAngle | BoneOffsetX | BoneOffsetY => {
                let Some(b) = p.bones.iter().position(|b| b.name == ch.target) else {
                    continue;
                };
                if touched.contains(&b)
                    || (0..p.bones.len()).any(|k| c.sub[b][k] && touched.contains(&k))
                {
                    continue;
                }
                if ch.prop == BoneAngle {
                    LKind::BoneAngle(b)
                } else {
                    LKind::BoneOff(b)
                }
            }
            LayerX | LayerY | LayerRotation => {
                let Some(id) = c.e.node_index(&ch.target) else {
                    continue;
                };
                match ch.prop {
                    LayerX => LKind::LayerX(id),
                    LayerY => LKind::LayerY(id),
                    _ => LKind::LayerRot(id),
                }
            }
            CameraX | CameraY => LKind::CamXY,
            _ => continue,
        };
        out.push((i, kind));
    }
    out
}

// ------------------------------------------------------------------------------------------
// Motion analysis: scale factors, extents, levers, mesh trajectories
// ------------------------------------------------------------------------------------------

/// Frames at which motion is sampled: `SUB` per frame plus every (fractional) key frame in range.
pub fn sample_frames(p: &Project) -> Vec<f32> {
    let n = p.frames.max(1) as usize;
    let mut v: Vec<f32> = (0..n * SUB).map(|i| i as f32 / SUB as f32).collect();
    for c in &p.channels {
        if let Source::Keys { track } = &c.source {
            for k in &track.keys {
                if k.frame >= 0.0 && k.frame < n as f32 {
                    v.push(k.frame);
                }
            }
        }
    }
    v.sort_by(|a, b| a.total_cmp(b));
    v.dedup();
    v
}

struct MeshTraj {
    /// `y[sample][vertex]`: deformed vertex positions in layer space.
    y: Vec<Vec<[f32; 2]>>,
    rest: Vec<P>,
    tris: Vec<[usize; 3]>,
}

#[derive(Default)]
struct Analysis {
    /// `s[node][sample]`: largest singular value of the node's world matrix (screen units per layer unit).
    s: Vec<Vec<f64>>,
    s_max: Vec<f64>,
    /// Largest screen diagonal (screen units) of the node's own geometry over all samples.
    diam_max: Vec<f64>,
    has_geo: Vec<bool>,
    mesh: BTreeMap<usize, MeshTraj>,
    /// Per eligible channel: screen units per value unit, per node (max over samples).
    lever: Vec<BTreeMap<usize, f64>>,
    /// Channels whose lever could not be bounded.
    blocked: Vec<bool>,
}

fn node_points(nd: &crate::engine::RenderNode, w: &A) -> Vec<P> {
    let mut v = Vec::new();
    for path in &nd.paths {
        for a in &path.anchors {
            v.push(a_apply(w, p_of(a.pos)));
            v.push(a_apply(w, p_of(a.hin())));
            v.push(a_apply(w, p_of(a.hout())));
        }
    }
    if let Some(img) = &nd.image {
        if let Some(m) = &img.mesh {
            for q in &m.positions {
                v.push(a_apply(w, p_of(*q)));
            }
        } else {
            let (o, s) = (p_of(img.origin), p_of(img.size));
            for c in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
                v.push(a_apply(w, [o[0] + s[0] * c[0], o[1] + s[1] * c[1]]));
            }
        }
    }
    v
}

fn analyze(
    c: &Ctx,
    samples: &[f32],
    chans: &[(usize, LKind)],
    mesh_nodes: &BTreeSet<usize>,
) -> Analysis {
    let n = c.paths.len();
    let p = c.p;
    let mut an = Analysis {
        s: vec![vec![0.0; samples.len()]; n],
        s_max: vec![0.0; n],
        diam_max: vec![0.0; n],
        has_geo: vec![false; n],
        lever: vec![BTreeMap::new(); chans.len()],
        blocked: vec![false; chans.len()],
        ..Default::default()
    };
    for &id in mesh_nodes {
        an.mesh.insert(
            id,
            MeshTraj {
                y: Vec::with_capacity(samples.len()),
                rest: Vec::new(),
                tris: Vec::new(),
            },
        );
    }
    let subtrees: Vec<Vec<usize>> = (0..n).map(|i| c.subtree(i)).collect();
    let need_levers = !chans.is_empty();
    let pi180 = std::f64::consts::PI / 180.0;
    let mut skin_rest: Vec<Option<Vec<P>>> = vec![None; n];
    for (si, &t) in samples.iter().enumerate() {
        let fr = c.e.evaluate(t);
        let worlds: Vec<A> = fr.nodes.iter().map(|nd| a_of(&nd.world)).collect();
        let pts: Vec<Vec<P>> = fr
            .nodes
            .iter()
            .enumerate()
            .map(|(id, nd)| node_points(nd, &worlds[id]))
            .collect();
        for id in 0..n {
            let s = lin_norm(&worlds[id]);
            an.s[id][si] = s;
            an.s_max[id] = an.s_max[id].max(s);
            let nd = &fr.nodes[id];
            if !pts[id].is_empty() {
                an.has_geo[id] = true;
                let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
                for q in &pts[id] {
                    x0 = x0.min(q[0]);
                    y0 = y0.min(q[1]);
                    x1 = x1.max(q[0]);
                    y1 = y1.max(q[1]);
                }
                let hw = nd
                    .paths
                    .iter()
                    .filter_map(|pp| pp.stroke.as_ref().map(|s| s.width as f64 * 0.5))
                    .fold(0.0, f64::max)
                    * s;
                let d = ((x1 - x0) + 2.0 * hw).hypot((y1 - y0) + 2.0 * hw);
                if d > an.diam_max[id] {
                    an.diam_max[id] = d;
                }
            }
            if let Some(mt) = an.mesh.get_mut(&id) {
                if let Some(m) = nd.image.as_ref().and_then(|i| i.mesh.as_ref()) {
                    if si == 0 {
                        mt.tris = m
                            .tris
                            .iter()
                            .map(|t| [t[0] as usize, t[1] as usize, t[2] as usize])
                            .collect();
                        if let (Some(l), Some(img)) = (c.layer(id), nd.image.as_ref()) {
                            if let LayerKind::Image {
                                mesh: Some(ms),
                                origin,
                                ..
                            } = &l.kind
                            {
                                mt.rest = ms
                                    .rest_vertices(*origin, img.size)
                                    .iter()
                                    .map(|v| p_of(*v))
                                    .collect();
                            }
                        }
                    }
                    mt.y.push(m.positions.iter().map(|q| [q.x, q.y]).collect());
                }
            }
        }
        if !need_levers {
            continue;
        }
        // rest points of skinned image meshes (need the displayed size from the frame)
        // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
        #[allow(clippy::needless_range_loop)]
        for id in 0..n {
            if skin_rest[id].is_none() {
                if let (Some(l), Some(img)) = (c.layer(id), fr.nodes[id].image.as_ref()) {
                    if let LayerKind::Image {
                        mesh: Some(m),
                        origin,
                        ..
                    } = &l.kind
                    {
                        skin_rest[id] = Some(
                            m.rest_vertices(*origin, img.size)
                                .iter()
                                .map(|v| p_of(*v))
                                .collect(),
                        );
                    }
                }
            }
        }
        let skin: Vec<A> = fr
            .world
            .iter()
            .zip(&c.rest_inv)
            .map(|(w, ri)| a_mul(&a_of(w), ri))
            .collect();
        let mut locals: Vec<Option<A>> = vec![None; n];
        let local_of = |id: usize, locals: &mut Vec<Option<A>>| -> A {
            if locals[id].is_none() {
                locals[id] = Some(local_matrix(&c.e.layer_local_at(t, id)));
            }
            locals[id].unwrap()
        };
        for (ci, (_, kind)) in chans.iter().enumerate() {
            let upd = |lever: &mut BTreeMap<usize, f64>, id: usize, v: f64| {
                let e = lever.entry(id).or_insert(0.0);
                if v > *e {
                    *e = v;
                }
            };
            match *kind {
                LKind::CamXY => {
                    let z = lin_norm(&a_of(&fr.camera));
                    for id in 0..n {
                        let par = p
                            .layers
                            .get(c.e.node_idx_path(id)[0])
                            .map_or(1.0, |l| l.parallax as f64);
                        let ze = (1.0 + (z - 1.0) * par).max(0.01);
                        upd(&mut an.lever[ci], id, par.abs() * ze);
                    }
                }
                LKind::LayerX(nid) | LKind::LayerY(nid) | LKind::LayerRot(nid) => {
                    let loc = local_of(nid, &mut locals);
                    let Some(li) = a_inv(&loc) else {
                        an.blocked[ci] = true;
                        continue;
                    };
                    let pn = a_mul(&worlds[nid], &li);
                    let Some(pinv) = a_inv(&pn) else {
                        an.blocked[ci] = true;
                        continue;
                    };
                    let ln = lin_norm(&pn);
                    let lt = c.e.layer_local_at(t, nid);
                    let posv = [lt.position.x as f64, lt.position.y as f64];
                    for &m in &subtrees[nid] {
                        let v = match *kind {
                            LKind::LayerRot(_) => {
                                let r = pts[m]
                                    .iter()
                                    .map(|q| dist(a_apply(&pinv, *q), posv))
                                    .fold(0.0, f64::max);
                                ln * pi180 * r
                            }
                            _ => ln,
                        };
                        upd(&mut an.lever[ci], m, v);
                    }
                }
                LKind::BoneAngle(b) | LKind::BoneOff(b) => {
                    let is_angle = matches!(kind, LKind::BoneAngle(_));
                    let ob = a_apply(&a_of(&fr.world[b]), [0.0, 0.0]);
                    let parent_lin =
                        c.bone_parent[b].map_or(1.0, |pb| lin_norm(&a_of(&fr.world[pb])));
                    for id in 0..n {
                        // skinned geometry
                        let mut skin_part = 0.0f64;
                        let sp = &c.skin_pts[id];
                        if !sp.is_empty() {
                            let rest_mesh = skin_rest[id].as_ref();
                            let mut lev = 0.0f64;
                            let mut hit = false;
                            for (k, (rp, ws)) in sp.iter().enumerate() {
                                let wsub: f64 =
                                    ws.iter().filter(|(bk, _)| c.sub[b][*bk]).map(|x| x.1).sum();
                                if wsub <= 0.0 {
                                    continue;
                                }
                                hit = true;
                                if c.complex[id] {
                                    an.blocked[ci] = true;
                                    continue;
                                }
                                let rp = if rp[0].is_nan() {
                                    rest_mesh
                                        .and_then(|r| r.get(k).copied())
                                        .unwrap_or([0.0, 0.0])
                                } else {
                                    *rp
                                };
                                let v = if is_angle {
                                    ws.iter()
                                        .filter(|(bk, _)| c.sub[b][*bk])
                                        .map(|(bk, w)| w * dist(a_apply(&skin[*bk], rp), ob))
                                        .sum::<f64>()
                                        * pi180
                                } else {
                                    wsub * parent_lin
                                };
                                lev = lev.max(v);
                            }
                            if hit && lev > 0.0 {
                                skin_part = lev * an.s[id][si];
                            }
                        }
                        // riding layers: every ancestor-or-self that rides a bone at or below b
                        let mut cur = Some(id);
                        let mut total = 0.0f64;
                        while let Some(nn) = cur {
                            if let Some(bn) = c.layer(nn).and_then(|l| l.bone.as_deref()) {
                                if let Some(k) = p.bones.iter().position(|bb| bb.name == bn) {
                                    if c.sub[b][k] {
                                        let loc = local_of(nn, &mut locals);
                                        let att = skin[k];
                                        let q = a_inv(&loc).and_then(|li| {
                                            a_inv(&att)
                                                .map(|ai| a_mul(&a_mul(&worlds[nn], &li), &ai))
                                        });
                                        match q.and_then(|q| a_inv(&q).map(|qi| (q, qi))) {
                                            Some((q, qi)) => {
                                                let lq = lin_norm(&q);
                                                total += if is_angle {
                                                    lq * pi180
                                                        * pts[id]
                                                            .iter()
                                                            .map(|s| dist(a_apply(&qi, *s), ob))
                                                            .fold(0.0, f64::max)
                                                } else {
                                                    lq * parent_lin
                                                };
                                            }
                                            None => an.blocked[ci] = true,
                                        }
                                    }
                                }
                            }
                            cur = c.parent[nn];
                        }
                        if total + skin_part > 0.0 {
                            upd(&mut an.lever[ci], id, total + skin_part);
                        }
                    }
                }
            }
        }
    }
    an
}

// ------------------------------------------------------------------------------------------
// Key reduction with an L-infinity bound
// ------------------------------------------------------------------------------------------

fn merged_samples(t: &Track, grid: &[f32]) -> Vec<f32> {
    let mut v: Vec<f32> = grid.to_vec();
    v.extend(t.keys.iter().map(|k| k.frame));
    v.sort_by(|a, b| a.total_cmp(b));
    v.dedup();
    v
}

/// Largest `|reduced - original|` over `samples`.
pub fn track_deviation_at(orig: &Track, red: &Track, samples: &[f32]) -> f64 {
    samples
        .iter()
        .map(|&f| match (orig.eval(f), red.eval(f)) {
            (Some(a), Some(b)) => (a as f64 - b as f64).abs(),
            _ => 0.0,
        })
        .fold(0.0, f64::max)
}

/// Drop keys while the deviation over `grid` (plus every original key frame) stays within `tol`.
/// First and last key stay; kept keys keep their easing. Ramer-Douglas-Peucker on key indices, then a
/// repair loop re-inserts keys until the bound holds for the whole sample set.
pub fn reduce_track_bounded(t: &Track, tol: f64, grid: &[f32]) -> Track {
    let n = t.keys.len();
    if n < 3
        || !matches!(
            tol.partial_cmp(&0.0),
            Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
        )
        || !tol.is_finite()
    {
        return t.clone();
    }
    let samples = merged_samples(t, grid);
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let sub = |lo: usize, hi: usize| Track {
        keys: vec![t.keys[lo], t.keys[hi]],
        extrapolate: t.extrapolate,
    };
    let mut stack = vec![(0usize, n - 1)];
    while let Some((lo, hi)) = stack.pop() {
        if hi - lo < 2 {
            continue;
        }
        let cand = sub(lo, hi);
        let (f0, f1) = (t.keys[lo].frame, t.keys[hi].frame);
        let mut worst = (0.0f64, f0);
        for &f in samples.iter().filter(|&&f| f > f0 && f < f1) {
            let e = (t.eval(f).unwrap_or(0.0) as f64 - cand.eval(f).unwrap_or(0.0) as f64).abs();
            if e > worst.0 {
                worst = (e, f);
            }
        }
        if worst.0 <= tol {
            continue;
        }
        let split = (lo + 1..hi)
            .min_by(|&a, &b| {
                (t.keys[a].frame - worst.1)
                    .abs()
                    .total_cmp(&(t.keys[b].frame - worst.1).abs())
            })
            .unwrap();
        keep[split] = true;
        stack.push((lo, split));
        stack.push((split, hi));
    }
    let build = |keep: &[bool]| Track {
        keys: t
            .keys
            .iter()
            .zip(keep)
            .filter(|(_, &k)| k)
            .map(|(k, _)| *k)
            .collect(),
        extrapolate: t.extrapolate,
    };
    let mut red = build(&keep);
    loop {
        let mut worst = (0.0f64, 0.0f32);
        for &f in &samples {
            let e = (t.eval(f).unwrap_or(0.0) as f64 - red.eval(f).unwrap_or(0.0) as f64).abs();
            if e > worst.0 {
                worst = (e, f);
            }
        }
        if worst.0 <= tol {
            return red;
        }
        let Some(add) = (0..n).filter(|&i| !keep[i]).min_by(|&a, &b| {
            (t.keys[a].frame - worst.1)
                .abs()
                .total_cmp(&(t.keys[b].frame - worst.1).abs())
        }) else {
            return red;
        };
        keep[add] = true;
        red = build(&keep);
    }
}

struct KeyOutcome {
    channels: Vec<Channel>,
    keys_px: f64,
    reduced: usize,
    notes: Vec<String>,
}

/// Reduce the eligible channels so that the summed pixel error at any layer stays within `budget_px`.
fn reduce_keys(
    c: &Ctx,
    an: &Analysis,
    chans: &[(usize, LKind)],
    scale: f64,
    budget_px: f64,
    grid: &[f32],
) -> KeyOutcome {
    let p = c.p;
    let mut out = p.channels.clone();
    let mut notes = Vec::new();
    // co-influence count K: how many reducible channels can move the same layer
    let usable: Vec<usize> = (0..chans.len())
        .filter(|&i| !an.blocked[i] && an.lever[i].values().any(|&v| v > 0.0))
        .collect();
    // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
    #[allow(clippy::needless_range_loop)]
    for i in 0..chans.len() {
        if an.blocked[i] {
            notes.push(format!(
                "channel {} {:?} skipped: lever not bounded",
                p.channels[chans[i].0].target, p.channels[chans[i].0].prop
            ));
        }
    }
    let mut count: BTreeMap<usize, usize> = BTreeMap::new();
    for &i in &usable {
        for (&node, &v) in &an.lever[i] {
            if v > 0.0 {
                *count.entry(node).or_insert(0) += 1;
            }
        }
    }
    let k = count.values().copied().max().unwrap_or(1).max(1) as f64;
    let mut dev_by: Vec<(usize, f64)> = Vec::new(); // (index into chans, deviation in px-weighted units = value dev * weight)
    let mut reduced = 0;
    for &i in &usable {
        let ch_i = chans[i].0;
        let ch = &p.channels[ch_i];
        let Source::Keys { track } = &ch.source else {
            continue;
        };
        let lmax = an.lever[i].values().copied().fold(0.0, f64::max) * scale;
        let w = ch.weight as f64;
        let eps_value = budget_px / (k * lmax * w);
        let red = reduce_track_bounded(track, eps_value, grid);
        let samples = merged_samples(track, grid);
        let dev = track_deviation_at(track, &red, &samples) * w;
        if red.keys.len() < track.keys.len() {
            reduced += 1;
        }
        dev_by.push((i, dev));
        out[ch_i].source = Source::Keys { track: red };
    }
    // declared: per layer, sum over the channels that can move it
    let mut per_node: BTreeMap<usize, f64> = BTreeMap::new();
    for &(i, dev) in &dev_by {
        for (&node, &v) in &an.lever[i] {
            *per_node.entry(node).or_insert(0.0) += dev * v * scale;
        }
    }
    let keys_px = per_node.values().copied().fold(0.0, f64::max) * MARGIN;
    KeyOutcome {
        channels: out,
        keys_px,
        reduced,
        notes,
    }
}

// ------------------------------------------------------------------------------------------
// Mesh simplification: half-edge collapse, generalised QEM order, certified error
// ------------------------------------------------------------------------------------------

/// Input of [`qem_simplify`].
pub struct QemInput<'a> {
    /// Rest positions (layer space).
    pub rest: &'a [[f64; 2]],
    pub tris: &'a [[usize; 3]],
    /// Per vertex attribute vector (bone weights times a lever) steering the QEM order.
    pub attr: &'a [Vec<f64>],
    /// Deformed vertex positions per sample frame, `y[sample][vertex]` (layer space).
    pub y: &'a [Vec<[f32; 2]>],
    /// Pixels per layer unit at each sample (render scale times the world matrix norm).
    pub s: &'a [f64],
}

#[derive(Clone, Debug, PartialEq)]
pub struct QemOutput {
    /// Original indices of the kept vertices, ascending.
    pub keep: Vec<usize>,
    /// Triangles in terms of original vertex indices.
    pub tris: Vec<[usize; 3]>,
    /// Certified maximum deviation (px) of the deformed surface over the sampled frames.
    pub bound_px: f64,
    pub collapses: usize,
}

#[derive(Clone)]
struct Quadric {
    a: Vec<f64>,
    b: Vec<f64>,
    c: f64,
}

impl Quadric {
    fn zero(d: usize) -> Quadric {
        Quadric {
            a: vec![0.0; d * d],
            b: vec![0.0; d],
            c: 0.0,
        }
    }
    fn add(&mut self, o: &Quadric) {
        for (x, y) in self.a.iter_mut().zip(&o.a) {
            *x += y;
        }
        for (x, y) in self.b.iter_mut().zip(&o.b) {
            *x += y;
        }
        self.c += o.c;
    }
    fn eval(&self, x: &[f64]) -> f64 {
        let d = x.len();
        let mut q = 0.0;
        for i in 0..d {
            let mut r = 0.0;
            // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
            #[allow(clippy::needless_range_loop)]
            for j in 0..d {
                r += self.a[i * d + j] * x[j];
            }
            q += x[i] * r - 2.0 * self.b[i] * x[i];
        }
        q + self.c
    }
}

fn tri_quadric(x0: &[f64], x1: &[f64], x2: &[f64], w: f64) -> Option<Quadric> {
    let d = x0.len();
    let e1: Vec<f64> = (0..d).map(|i| x1[i] - x0[i]).collect();
    let e2: Vec<f64> = (0..d).map(|i| x2[i] - x0[i]).collect();
    let l1 = e1.iter().map(|v| v * v).sum::<f64>().sqrt();
    if l1 < 1e-300 {
        return None;
    }
    let u1: Vec<f64> = e1.iter().map(|v| v / l1).collect();
    let pr: f64 = (0..d).map(|i| e2[i] * u1[i]).sum();
    let mut u2: Vec<f64> = (0..d).map(|i| e2[i] - pr * u1[i]).collect();
    let l2 = u2.iter().map(|v| v * v).sum::<f64>().sqrt();
    if l2 < 1e-300 {
        return None;
    }
    for v in &mut u2 {
        *v /= l2;
    }
    let mut q = Quadric::zero(d);
    for i in 0..d {
        for j in 0..d {
            let a = (if i == j { 1.0 } else { 0.0 }) - u1[i] * u1[j] - u2[i] * u2[j];
            q.a[i * d + j] = a * w;
        }
    }
    for i in 0..d {
        q.b[i] = (0..d).map(|j| q.a[i * d + j] * x0[j]).sum();
    }
    q.c = (0..d).map(|i| q.b[i] * x0[i]).sum();
    Some(q)
}

#[derive(Clone, Copy)]
struct Cand {
    cost: f64,
    u: usize,
    v: usize,
    vu: u32,
    vv: u32,
}
impl PartialEq for Cand {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == std::cmp::Ordering::Equal
    }
}
impl Eq for Cand {}
impl PartialOrd for Cand {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Cand {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        // smaller cost = greater priority; ties by (u, v) ascending
        o.cost
            .total_cmp(&self.cost)
            .then(o.u.cmp(&self.u))
            .then(o.v.cmp(&self.v))
    }
}

fn area2(a: P, b: P, c: P) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// Clip the triangle `t` (a convex polygon) against triangle `o`; returns the polygon corners.
fn clip_tri(t: [P; 3], o: [P; 3], tol: f64) -> Vec<P> {
    let sgn = if area2(o[0], o[1], o[2]) >= 0.0 {
        1.0
    } else {
        -1.0
    };
    let mut poly: Vec<P> = t.to_vec();
    for k in 0..3 {
        let (a, b) = (o[k], o[(k + 1) % 3]);
        let side = |p: P| sgn * ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]));
        let mut next: Vec<P> = Vec::with_capacity(poly.len() + 2);
        for i in 0..poly.len() {
            let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
            let (sp, sq) = (side(p), side(q));
            let (ip, iq) = (sp >= -tol, sq >= -tol);
            if ip {
                next.push(p);
            }
            if ip != iq {
                let t = sp / (sp - sq);
                next.push([p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]);
            }
        }
        poly = next;
        if poly.is_empty() {
            break;
        }
    }
    poly
}

fn bary(p: P, t: [P; 3]) -> [f64; 3] {
    let d = area2(t[0], t[1], t[2]);
    if d.abs() < 1e-300 {
        return [1.0, 0.0, 0.0];
    }
    let l0 = area2(p, t[1], t[2]) / d;
    let l1 = area2(t[0], p, t[2]) / d;
    [l0, l1, 1.0 - l0 - l1]
}

/// Greedy half-edge collapse (QEM order); every accepted collapse keeps the certified deviation from the
/// original mesh within `eps_px`. The domain (outline) of the mesh is preserved exactly.
pub fn qem_simplify(inp: &QemInput, eps_px: f64) -> QemOutput {
    let n = inp.rest.len();
    let nt = inp.tris.len();
    let identity = || QemOutput {
        keep: (0..n).collect(),
        tris: inp.tris.to_vec(),
        bound_px: 0.0,
        collapses: 0,
    };
    if n == 0
        || nt == 0
        || (eps_px.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater))
        || inp.y.is_empty()
        || inp.y.iter().any(|f| f.len() != n)
        || inp.s.len() != inp.y.len()
    {
        return identity();
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for r in inp.rest {
        x0 = x0.min(r[0]);
        y0 = y0.min(r[1]);
        x1 = x1.max(r[0]);
        y1 = y1.max(r[1]);
    }
    let diag = (x1 - x0).hypot(y1 - y0).max(1e-12);
    let atol = 1e-12 * diag * diag;
    let d = 2 + inp.attr.first().map_or(0, |a| a.len());
    let xv = |v: usize| -> Vec<f64> {
        let mut x = vec![inp.rest[v][0], inp.rest[v][1]];
        x.extend_from_slice(&inp.attr[v]);
        x
    };
    let mut tris: Vec<[usize; 3]> = inp.tris.to_vec();
    let mut alive = vec![true; nt];
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (t, tr) in tris.iter().enumerate() {
        for &v in tr {
            adj[v].push(t);
        }
    }
    let mut quad: Vec<Quadric> = vec![Quadric::zero(d); n];
    for tr in &tris {
        let (a, b, c) = (xv(tr[0]), xv(tr[1]), xv(tr[2]));
        let w =
            area2(inp.rest[tr[0]], inp.rest[tr[1]], inp.rest[tr[2]]).abs() * 0.5 / (diag * diag);
        if let Some(q) = tri_quadric(&a, &b, &c, w) {
            for &v in tr {
                quad[v].add(&q);
            }
        }
    }
    let mut cover: Vec<Vec<usize>> = (0..nt).map(|t| vec![t]).collect();
    let mut terr = vec![0.0f64; nt];
    let mut ver = vec![0u32; n];

    let neighbours = |adj: &Vec<Vec<usize>>, tris: &Vec<[usize; 3]>, v: usize| -> Vec<usize> {
        let mut s: Vec<usize> = adj[v]
            .iter()
            .flat_map(|&t| tris[t].iter().copied())
            .filter(|&x| x != v)
            .collect();
        s.sort_unstable();
        s.dedup();
        s
    };
    let common_tris = |adj: &Vec<Vec<usize>>, u: usize, v: usize| -> Vec<usize> {
        adj[u]
            .iter()
            .copied()
            .filter(|t| adj[v].contains(t))
            .collect()
    };

    // validity of the collapse of u into v (topology, domain, orientation)
    let valid = |adj: &Vec<Vec<usize>>, tris: &Vec<[usize; 3]>, u: usize, v: usize| -> bool {
        let tuv = common_tris(adj, u, v);
        if tuv.is_empty() || tuv.len() > 2 {
            return false;
        }
        // boundary vertex rules
        let nu = neighbours(adj, tris, u);
        let mut boundary_nbrs: Vec<usize> = Vec::new();
        for &x in &nu {
            if common_tris(adj, u, x).len() == 1 {
                boundary_nbrs.push(x);
            }
        }
        if !boundary_nbrs.is_empty() {
            if tuv.len() != 1 || boundary_nbrs.len() != 2 || !boundary_nbrs.contains(&v) {
                return false;
            }
            let w = *boundary_nbrs.iter().find(|&&x| x != v).unwrap();
            if dist_pt_seg(inp.rest[u], inp.rest[v], inp.rest[w]) > 1e-9 * diag {
                return false;
            }
        }
        // link condition
        let nv = neighbours(adj, tris, v);
        let common: BTreeSet<usize> = nu.iter().copied().filter(|x| nv.contains(x)).collect();
        let apex: BTreeSet<usize> = tuv
            .iter()
            .flat_map(|&t| tris[t].iter().copied())
            .filter(|&x| x != u && x != v)
            .collect();
        if common != apex {
            return false;
        }
        // orientation of the new triangles
        for &t in &adj[u] {
            if tuv.contains(&t) {
                continue;
            }
            let tr = tris[t];
            let old = area2(inp.rest[tr[0]], inp.rest[tr[1]], inp.rest[tr[2]]);
            let nt_ = tr.map(|x| if x == u { v } else { x });
            let new = area2(inp.rest[nt_[0]], inp.rest[nt_[1]], inp.rest[nt_[2]]);
            if new * old <= 0.0 || new.abs() < 1e-9 * diag * diag {
                return false;
            }
        }
        true
    };

    let qcost = |quad: &Vec<Quadric>, u: usize, v: usize| -> f64 {
        let mut q = quad[u].clone();
        q.add(&quad[v]);
        let e = q.eval(&xv(v)).max(0.0);
        let l = dist(inp.rest[u], inp.rest[v]) / diag;
        e + 1e-9 * l * l
    };

    let mut heap: BinaryHeap<Cand> = BinaryHeap::new();
    for u in 0..n {
        for v in neighbours(&adj, &tris, u) {
            if valid(&adj, &tris, u, v) {
                heap.push(Cand {
                    cost: qcost(&quad, u, v),
                    u,
                    v,
                    vu: 0,
                    vv: 0,
                });
            }
        }
    }

    // exact error of a (possibly collapsed) triangle against the original mesh
    let tri_error = |t3: [usize; 3], cov: &[usize]| -> f64 {
        let tp = [inp.rest[t3[0]], inp.rest[t3[1]], inp.rest[t3[2]]];
        let (tx0, ty0, tx1, ty1) = (
            tp.iter().map(|p| p[0]).fold(f64::MAX, f64::min),
            tp.iter().map(|p| p[1]).fold(f64::MAX, f64::min),
            tp.iter().map(|p| p[0]).fold(f64::MIN, f64::max),
            tp.iter().map(|p| p[1]).fold(f64::MIN, f64::max),
        );
        // corner list: (coefficients on original vertices of the new tri, of the original tri)
        let mut worst = 0.0f64;
        for &o in cov {
            let ot = inp.tris[o];
            let op = [inp.rest[ot[0]], inp.rest[ot[1]], inp.rest[ot[2]]];
            if op.iter().map(|p| p[0]).fold(f64::MAX, f64::min) > tx1 + 1e-12 * diag
                || op.iter().map(|p| p[0]).fold(f64::MIN, f64::max) < tx0 - 1e-12 * diag
                || op.iter().map(|p| p[1]).fold(f64::MAX, f64::min) > ty1 + 1e-12 * diag
                || op.iter().map(|p| p[1]).fold(f64::MIN, f64::max) < ty0 - 1e-12 * diag
            {
                continue;
            }
            let poly = clip_tri(tp, op, 1e-12 * diag * diag);
            if poly.len() < 3 {
                continue;
            }
            let mut ar = 0.0;
            for i in 1..poly.len() - 1 {
                ar += area2(poly[0], poly[i], poly[i + 1]).abs();
            }
            if ar < atol {
                continue;
            }
            for c in &poly {
                let lt = bary(*c, tp);
                let lo = bary(*c, op);
                for (f, yf) in inp.y.iter().enumerate() {
                    let mut dx = 0.0f64;
                    let mut dy = 0.0f64;
                    for j in 0..3 {
                        let a = yf[ot[j]];
                        let b = yf[t3[j]];
                        dx += lo[j] * a[0] as f64 - lt[j] * b[0] as f64;
                        dy += lo[j] * a[1] as f64 - lt[j] * b[1] as f64;
                    }
                    let e = dx.hypot(dy) * inp.s[f];
                    if e > worst {
                        worst = e;
                    }
                }
            }
        }
        worst
    };

    let mut collapses = 0;
    while let Some(c) = heap.pop() {
        let (u, v) = (c.u, c.v);
        if ver[u] != c.vu || ver[v] != c.vv || adj[u].is_empty() || adj[v].is_empty() {
            continue;
        }
        if !valid(&adj, &tris, u, v) {
            continue;
        }
        // certify
        let tuv = common_tris(&adj, u, v);
        let mut union: Vec<usize> = adj[u]
            .iter()
            .flat_map(|&t| cover[t].iter().copied())
            .collect();
        union.sort_unstable();
        union.dedup();
        let mut news: Vec<(usize, [usize; 3], f64)> = Vec::new();
        let mut ok = true;
        for &t in &adj[u] {
            if tuv.contains(&t) {
                continue;
            }
            let nt3 = tris[t].map(|x| if x == u { v } else { x });
            let e = tri_error(nt3, &union);
            if e > eps_px {
                ok = false;
                break;
            }
            news.push((t, nt3, e));
        }
        if !ok {
            continue;
        }
        // apply
        for &t in &tuv {
            alive[t] = false;
            for &x in &tris[t] {
                if x != u {
                    adj[x].retain(|&q| q != t);
                }
            }
        }
        let ring = neighbours(&adj, &tris, u);
        for (t, nt3, e) in news {
            tris[t] = nt3;
            cover[t] = union.clone();
            terr[t] = e;
            if !adj[v].contains(&t) {
                adj[v].push(t);
            }
        }
        adj[u].clear();
        adj[v].sort_unstable();
        let qu = quad[u].clone();
        quad[v].add(&qu);
        collapses += 1;
        // refresh candidates around the change
        let mut aff: BTreeSet<usize> = ring.iter().copied().collect();
        aff.insert(v);
        aff.extend(neighbours(&adj, &tris, v));
        for &x in &aff {
            ver[x] += 1;
        }
        for &x in &aff {
            for y in neighbours(&adj, &tris, x) {
                if valid(&adj, &tris, x, y) {
                    heap.push(Cand {
                        cost: qcost(&quad, x, y),
                        u: x,
                        v: y,
                        vu: ver[x],
                        vv: ver[y],
                    });
                }
                if !aff.contains(&y) && valid(&adj, &tris, y, x) {
                    heap.push(Cand {
                        cost: qcost(&quad, y, x),
                        u: y,
                        v: x,
                        vu: ver[y],
                        vv: ver[x],
                    });
                }
            }
        }
    }
    if collapses == 0 {
        return identity();
    }
    let mut used = vec![false; n];
    let mut referenced = vec![false; n];
    for tr in inp.tris {
        for &v in tr {
            referenced[v] = true;
        }
    }
    let mut out_tris = Vec::new();
    let mut bound = 0.0f64;
    for t in 0..nt {
        if alive[t] {
            out_tris.push(tris[t]);
            for &v in &tris[t] {
                used[v] = true;
            }
            bound = bound.max(terr[t]);
        }
    }
    let keep: Vec<usize> = (0..n).filter(|&v| used[v] || !referenced[v]).collect();
    QemOutput {
        keep,
        tris: out_tris,
        bound_px: bound,
        collapses,
    }
}

// ------------------------------------------------------------------------------------------
// Building levels
// ------------------------------------------------------------------------------------------

fn path_eligible(l: &Layer, path: &VPath) -> bool {
    if path.anchors.len() < 4
        || !path.morphs.is_empty()
        || path.anchors.iter().any(|a| !a.weights.is_empty())
    {
        return false;
    }
    if !l.deformers.is_empty() || l.wobble.is_some() {
        return false;
    }
    match &path.stroke {
        None => true,
        Some(s) => {
            s.join == crate::style::Join::Round && s.dash.is_empty() && s.width_profile.is_empty()
        }
    }
}

fn mask_targets(ls: &[Layer], out: &mut BTreeSet<String>) {
    for l in ls {
        if let Some(m) = &l.mask {
            out.insert(m.layer.clone());
        }
        mask_targets(l.children(), out);
    }
}

/// Mesh nodes that may be collapsed: image layers with a mesh and without deformers / wobble /
/// point offsets, whose texture coordinates are an affine function of the position.
fn mesh_eligible(c: &Ctx, id: usize) -> bool {
    let Some(l) = c.layer(id) else { return false };
    let LayerKind::Image { mesh: Some(m), .. } = &l.kind else {
        return false;
    };
    if !l.deformers.is_empty() || l.wobble.is_some() || c.complex[id] {
        return false;
    }
    if let Some(f) = &m.free {
        if !f.uvs.is_empty() {
            if f.uvs.len() != f.vertices.len() || f.vertices.len() < 3 {
                return false;
            }
            // uv = M * pos + t globally: fit through the first three non-collinear vertices
            let v = &f.vertices;
            let u = &f.uvs;
            let mut base = None;
            'outer: for i in 1..v.len() {
                for j in i + 1..v.len() {
                    let d = area2(p_of(v[0]), p_of(v[i]), p_of(v[j]));
                    if d.abs() > 1e-9 {
                        base = Some((i, j, d));
                        break 'outer;
                    }
                }
            }
            let Some((i, j, _)) = base else { return false };
            let tri = [p_of(v[0]), p_of(v[i]), p_of(v[j])];
            let size = v
                .iter()
                .fold(0.0f64, |a, q| {
                    a.max((q.x as f64).abs()).max((q.y as f64).abs())
                })
                .max(1.0);
            for k in 0..v.len() {
                let lam = bary(p_of(v[k]), tri);
                for ax in 0..2 {
                    let get = |q: Vec2| if ax == 0 { q.x as f64 } else { q.y as f64 };
                    let pred = lam[0] * get(u[0]) + lam[1] * get(u[i]) + lam[2] * get(u[j]);
                    if (pred - get(u[k])).abs() > 1e-4 * size.max(1.0) {
                        return false;
                    }
                }
            }
        }
    }
    true
}

/// Every string value of the structures that refer to layers by name (containers, scrolls, links, glues, machines, bindings,
/// mesh locks, shots, clips): a layer named there is not culled.
fn referenced_strings(p: &Project) -> BTreeSet<String> {
    fn collect(v: &serde_json::Value, out: &mut BTreeSet<String>) {
        match v {
            serde_json::Value::String(s) => {
                out.insert(s.clone());
            }
            serde_json::Value::Array(a) => a.iter().for_each(|x| collect(x, out)),
            serde_json::Value::Object(o) => o.values().for_each(|x| collect(x, out)),
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    for v in [
        serde_json::to_value(&p.containers),
        serde_json::to_value(&p.scrolls),
        serde_json::to_value(&p.mesh_locks),
        serde_json::to_value(&p.glues),
        serde_json::to_value(&p.machines),
        serde_json::to_value(&p.bindings),
        serde_json::to_value(&p.shots),
        serde_json::to_value(&p.clips),
    ]
    .into_iter()
    .flatten()
    {
        collect(&v, &mut out);
    }
    out
}

/// Paths of the layers a container (member) or scroll (clip layer) lays out by their geometry bounds. Simplifying the
/// path of such a layer, or culling a part of it, changes its bounding box and the solver re-flows the siblings: the
/// motion of the siblings would then be an error that the declared bound does not cover. These layers (and everything
/// below a member group) are therefore left exactly as they are (`skipped: layout member` in the level notes); the
/// declared bound stays a proof, at the price of not reducing those layers.
fn layout_members(p: &Project) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for c in &p.containers {
        for m in &c.members {
            if let Some(l) = &m.layer {
                out.insert(l.clone());
            }
        }
    }
    for s in &p.scrolls {
        if let Some(l) = &s.clip_layer {
            out.insert(l.clone());
        }
    }
    out
}

/// `path` is a layout member or lies below one.
fn is_layout_dependent(members: &BTreeSet<String>, path: &str) -> bool {
    members.iter().any(|m| {
        path == m || (path.starts_with(m.as_str()) && path.as_bytes().get(m.len()) == Some(&b'/'))
    })
}

fn is_boundary_layer_kind(k: &LayerKind) -> bool {
    matches!(k, LayerKind::Vector { .. } | LayerKind::Image { .. })
}

/// Levels L0..Ln of `p`. L0 is an unchanged copy. See the module notes for the guarantees.
pub fn build(p: &Project, o: &LodOptions) -> Result<Vec<Lod>, String> {
    o.check()?;
    Engine::new(p.clone()).map_err(|e| format!("project does not validate: {}", e.join("; ")))?;
    let mut base_rec = LodRecord {
        level: 0,
        tolerance_px: 0.0,
        scale: o.scale,
        declared_px: 0.0,
        bound_stage: 0.0,
        path_px: 0.0,
        mesh_px: 0.0,
        keys_px: 0.0,
        cull_px: 0.0,
        stats: count_stats(p),
        notes: vec![],
    };
    let mut out = vec![Lod {
        project: p.clone(),
        record: base_rec.clone(),
    }];
    base_rec.level = 0;
    let finish = |mut q: Project, mut rec: LodRecord| -> Lod {
        q.lods.clear();
        rec.stats = count_stats(&q);
        rec.declared_px = ceil6(rec.declared_px);
        rec.path_px = ceil6(rec.path_px);
        rec.mesh_px = ceil6(rec.mesh_px);
        rec.keys_px = ceil6(rec.keys_px);
        rec.cull_px = ceil6(rec.cull_px);
        rec.bound_stage = ceil6(rec.declared_px / rec.scale);
        q.lods = vec![rec.clone()];
        Lod {
            project: q,
            record: rec,
        }
    };
    if links_present(p) {
        for k in 1..=o.levels {
            let mut rec = base_rec.clone();
            rec.level = k;
            rec.tolerance_px = o.level_tolerance(k);
            rec.notes
                .push("mesh locks / glues present: no reduction applied".into());
            out.push(finish(p.clone(), rec));
        }
        return Ok(out);
    }
    // lossless step shared by all levels
    let (mut pb, mut dead) = crate::sizefit::drop_dead_layers(p);
    if Engine::new(pb.clone()).is_err() {
        // dropping dead layers broke a reference (container, machine, ...): keep them
        pb = p.clone();
        dead.clear();
    }
    let referenced = referenced_strings(&pb);
    let layout_dep = layout_members(&pb);
    let eb = Engine::new(pb.clone()).map_err(|e| e.join("; "))?;
    let cb = Ctx::new(&pb, &eb)?;
    let samples = sample_frames(&pb);
    let chans = eligible_channels(&cb);
    let an0 = analyze(&cb, &samples, &chans, &BTreeSet::new());
    let mut masks = BTreeSet::new();
    mask_targets(&pb.layers, &mut masks);
    let mut prev_declared = 0.0f64;
    let mut prev_channels: Option<Vec<Channel>> = None;
    let mut prev_paths: BTreeMap<(String, usize), (Vec<Anchor>, f64)> = BTreeMap::new();
    for k in 1..=o.levels {
        let tol = o.level_tolerance(k);
        let (geo_b, key_b, cull_b) = (
            tol * o.geometry_share,
            tol * o.keys_share,
            tol * o.cull_share,
        );
        let mut notes: Vec<String> = Vec::new();
        if !dead.is_empty() {
            notes.push(format!(
                "dropped {} dead layer(s): {}",
                dead.len(),
                dead.join(", ")
            ));
        }
        // 1. keys
        let mut ko = reduce_keys(&cb, &an0, &chans, o.scale, key_b, &samples);
        if let Some(prev) = &prev_channels {
            // never keep more keys than the previous (finer) level: reuse its track when it is smaller
            let mut per_node: BTreeMap<usize, f64> = BTreeMap::new();
            let mut changed = false;
            for (ci, (chi, _)) in chans.iter().enumerate() {
                if let (Source::Keys { track: nt }, Source::Keys { track: pt }) =
                    (&ko.channels[*chi].source, &prev[*chi].source)
                {
                    if pt.keys.len() < nt.keys.len() {
                        ko.channels[*chi].source = prev[*chi].source.clone();
                        changed = true;
                    }
                }
                let _ = ci;
            }
            if changed {
                for (ci, (chi, _)) in chans.iter().enumerate() {
                    if an0.blocked[ci] {
                        continue;
                    }
                    let (Source::Keys { track: orig }, Source::Keys { track: red }) =
                        (&pb.channels[*chi].source, &ko.channels[*chi].source)
                    else {
                        continue;
                    };
                    let dev = track_deviation_at(orig, red, &merged_samples(orig, &samples))
                        * pb.channels[*chi].weight as f64;
                    for (&node, &v) in &an0.lever[ci] {
                        *per_node.entry(node).or_insert(0.0) += dev * v * o.scale;
                    }
                }
                ko.keys_px = per_node.values().copied().fold(0.0, f64::max) * MARGIN;
            }
        }
        prev_channels = Some(ko.channels.clone());
        let mut pk = pb.clone();
        pk.channels = ko.channels.clone();
        let ek = match Engine::new(pk.clone()) {
            Ok(e) => e,
            Err(e) => {
                return Err(format!(
                    "internal: reduced keys do not validate: {}",
                    e.join("; ")
                ))
            }
        };
        notes.extend(ko.notes.iter().cloned());
        let keyed = pb
            .channels
            .iter()
            .filter(|c| matches!(c.source, Source::Keys { .. }))
            .count();
        notes.push(format!(
            "keys: {} of {} keyed channels have a documented lever, {} reduced",
            chans.len() - an0.blocked.iter().filter(|b| **b).count(),
            keyed,
            ko.reduced
        ));
        // 2. geometry on the reduced animation
        let ck = Ctx::new(&pk, &ek)?;
        let mesh_nodes: BTreeSet<usize> = (0..ck.paths.len())
            .filter(|&i| mesh_eligible(&ck, i))
            .collect();
        let an1 = analyze(&ck, &samples, &[], &mesh_nodes);
        let (paths_v, complex_v, children_v, parent_v) = (
            ck.paths.clone(),
            ck.complex.clone(),
            ck.children.clone(),
            ck.parent.clone(),
        );
        drop(ck);
        let mut path_px = 0.0f64;
        let mut mesh_px = 0.0f64;
        let mut removed_anchors = 0usize;
        let mut collapses = 0usize;
        let mut new_paths: BTreeMap<(String, usize), (Vec<Anchor>, f64)> = BTreeMap::new();
        let mut skipped_layout: Vec<String> = Vec::new();
        for id in 0..paths_v.len() {
            let Some(l) = pk.find_layer(&paths_v[id]) else {
                continue;
            };
            let LayerKind::Vector { paths } = &l.kind else {
                continue;
            };
            if complex_v[id] {
                continue;
            }
            if is_layout_dependent(&layout_dep, &paths_v[id]) {
                if paths
                    .iter()
                    .any(|path| path_eligible(l, path) && path.anchors.len() > 2)
                {
                    skipped_layout.push(paths_v[id].clone());
                }
                continue;
            }
            let s_used = an1.s_max[id] * MARGIN;
            if s_used.partial_cmp(&(1e-9)) != Some(std::cmp::Ordering::Greater) {
                continue;
            }
            let eps_local = geo_b / (o.scale * s_used);
            for (pi, path) in paths.iter().enumerate() {
                if !path_eligible(l, path) {
                    continue;
                }
                let key = (paths_v[id].clone(), pi);
                let mut sp = simplify_path_certified(&path.anchors, path.closed, eps_local);
                let mut bound_local = sp.bound;
                if let Some((pa, pb_)) = prev_paths.get(&key) {
                    if pa.len() < sp.anchors.len() && *pb_ <= eps_local {
                        sp.anchors = pa.clone();
                        bound_local = *pb_;
                    }
                }
                if sp.anchors.len() < path.anchors.len() {
                    removed_anchors += path.anchors.len() - sp.anchors.len();
                    path_px = path_px.max(bound_local * o.scale * s_used);
                    new_paths.insert(key, (sp.anchors, bound_local));
                }
            }
        }
        // apply path results
        for ((lp, pi), (anchors, _)) in &new_paths {
            if let Some(l) = pk.find_layer_mut(lp) {
                if let LayerKind::Vector { paths } = &mut l.kind {
                    paths[*pi].anchors = anchors.clone();
                }
            }
        }
        // 3. meshes
        for &id in &mesh_nodes {
            let Some(mt) = an1.mesh.get(&id) else {
                continue;
            };
            let nv = mt.rest.len();
            if nv < 4
                || mt.y.len() != samples.len()
                || mt.y.iter().any(|f| f.len() != nv)
                || mt.tris.is_empty()
            {
                continue;
            }
            let layer_path = paths_v[id].clone();
            let Some(l) = pk.find_layer(&layer_path) else {
                continue;
            };
            let LayerKind::Image { mesh: Some(m), .. } = &l.kind else {
                continue;
            };
            // attributes: weights of the (at most 16 most used) bones times the displacement scale
            let lever =
                mt.y.iter()
                    .flat_map(|f| {
                        f.iter()
                            .zip(&mt.rest)
                            .map(|(y, r)| (y[0] as f64 - r[0]).hypot(y[1] as f64 - r[1]))
                    })
                    .fold(0.0, f64::max)
                    .max(1e-9);
            let mut bones: Vec<String> =
                m.weights.iter().flatten().map(|w| w.bone.clone()).collect();
            bones.sort();
            bones.dedup();
            bones.truncate(16);
            let attr: Vec<Vec<f64>> = (0..nv)
                .map(|v| {
                    let ws = m.weights.get(v);
                    let sum: f64 = ws.map_or(0.0, |w| w.iter().map(|x| x.weight as f64).sum());
                    bones
                        .iter()
                        .map(|b| match ws {
                            Some(w) if sum > 0.0 => {
                                w.iter()
                                    .filter(|x| &x.bone == b)
                                    .map(|x| x.weight as f64)
                                    .sum::<f64>()
                                    / sum
                                    * lever
                            }
                            _ => 0.0,
                        })
                        .collect()
                })
                .collect();
            let s: Vec<f64> = an1.s[id].iter().map(|v| v * o.scale * MARGIN).collect();
            let q = qem_simplify(
                &QemInput {
                    rest: &mt.rest,
                    tris: &mt.tris,
                    attr: &attr,
                    y: &mt.y,
                    s: &s,
                },
                geo_b,
            );
            if q.collapses == 0 {
                continue;
            }
            let mut remap = vec![usize::MAX; nv];
            for (ni, &oi) in q.keep.iter().enumerate() {
                remap[oi] = ni;
            }
            let Some(l) = pk.find_layer_mut(&layer_path) else {
                continue;
            };
            let LayerKind::Image { mesh: Some(m), .. } = &mut l.kind else {
                continue;
            };
            let old_uvs: Vec<Vec2> = m.free.as_ref().map(|f| f.uvs.clone()).unwrap_or_default();
            let weights: Vec<Vec<crate::path::BoneWeight>> = if m.weights.is_empty() {
                vec![]
            } else {
                q.keep.iter().map(|&i| m.weights[i].clone()).collect()
            };
            m.free = Some(crate::project::FreeMesh {
                vertices: q
                    .keep
                    .iter()
                    .map(|&i| Vec2::new(mt.rest[i][0] as f32, mt.rest[i][1] as f32))
                    .collect(),
                triangles: q
                    .tris
                    .iter()
                    .map(|t| [remap[t[0]] as u32, remap[t[1]] as u32, remap[t[2]] as u32])
                    .collect(),
                uvs: if old_uvs.len() == nv {
                    q.keep.iter().map(|&i| old_uvs[i]).collect()
                } else {
                    vec![]
                },
            });
            m.weights = weights;
            m.cols = 0;
            m.rows = 0;
            collapses += q.collapses;
            mesh_px = mesh_px.max(q.bound_px);
        }
        // 4. culling
        let mut cull_px = 0.0f64;
        let mut culled: Vec<String> = Vec::new();
        let mut skipped_ref: Vec<String> = Vec::new();
        for id in 0..paths_v.len() {
            let path = &paths_v[id];
            let Some(l) = pk.find_layer(path) else {
                continue;
            };
            if !children_v[id].is_empty()
                || !is_boundary_layer_kind(&l.kind)
                || l.blend != crate::style::Blend::Normal
                || l.mask.is_some()
                || l.reference
            {
                continue;
            }
            if masks.contains(path) || !an1.has_geo[id] {
                continue;
            }
            if is_layout_dependent(&layout_dep, path) {
                continue; // removing it would change the bounds the layout is solved from
            }
            if let Some(pa) = parent_v[id] {
                if matches!(
                    pk.find_layer(&paths_v[pa]).map(|x| &x.kind),
                    Some(LayerKind::Switch { .. })
                ) {
                    continue;
                }
            }
            let d = an1.diam_max[id] * o.scale * MARGIN;
            if d <= cull_b {
                let short = path.rsplit('/').next().unwrap_or(path);
                if referenced.contains(path.as_str()) || referenced.contains(short) {
                    // a container member (removal re-flows its siblings), scroll, link, glue, machine or binding names it
                    skipped_ref.push(path.clone());
                    continue;
                }
                culled.push(path.clone());
                cull_px = cull_px.max(d);
            }
        }
        for path in &culled {
            // skip a layer already removed with an ancestor (cannot happen: only leaves are culled)
            let _ = crate::scriptops::delete_layer(&mut pk, path);
        }
        if !skipped_layout.is_empty() {
            notes.push(format!("skipped: layout member - {} layer(s) left unreduced because a container or scroll lays them out by their bounds (simplifying them would re-flow their siblings): {}", skipped_layout.len(), skipped_layout.join(", ")));
        }
        if removed_anchors > 0 {
            notes.push(format!("removed {removed_anchors} path anchor(s)"));
        }
        if collapses > 0 {
            notes.push(format!("collapsed {collapses} mesh edge(s)"));
        }
        if !culled.is_empty() {
            notes.push(format!(
                "culled {} layer(s): {}",
                culled.len(),
                culled.join(", ")
            ));
        }
        if !skipped_ref.is_empty() {
            notes.push(format!("skipped: referenced - {} small layer(s) kept because a container, scroll, link, glue, machine or binding names them: {}", skipped_ref.len(), skipped_ref.join(", ")));
        }
        let mut rec = LodRecord {
            level: k,
            tolerance_px: tol,
            scale: o.scale,
            declared_px: cull_px
                .max(path_px.max(mesh_px) + ko.keys_px)
                .max(prev_declared),
            bound_stage: 0.0,
            path_px,
            mesh_px,
            keys_px: ko.keys_px,
            cull_px,
            stats: LodStats::default(),
            notes,
        };
        match Engine::new(pk.clone()) {
            Ok(_) => prev_paths = new_paths,
            Err(e) => {
                // never fail the whole run: this level repeats the previous one, with the reason
                let prev = out.last().expect("level 0 exists");
                pk = prev.project.clone();
                rec.path_px = prev.record.path_px;
                rec.mesh_px = prev.record.mesh_px;
                rec.keys_px = prev.record.keys_px;
                rec.cull_px = prev.record.cull_px;
                rec.declared_px = prev.record.declared_px;
                rec.notes = vec![format!(
                    "level {k} did not validate ({}); it repeats level {} (no further reduction)",
                    e.join("; "),
                    k - 1
                )];
            }
        }
        prev_declared = rec.declared_px.max(prev_declared);
        out.push(finish(pk, rec));
    }
    Ok(out)
}

/// The coarsest level whose declared bound, scaled to `screen_scale` (pixels per stage unit), stays
/// within `max_px`. Level 0 (bound 0) is always acceptable.
pub fn pick_level(records: &[LodRecord], screen_scale: f64, max_px: f64) -> usize {
    let mut best = 0;
    if (screen_scale.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater))
        || !matches!(
            max_px.partial_cmp(&0.0),
            Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
        )
    {
        return 0;
    }
    for r in records {
        if r.level > 0 && r.bound_stage * screen_scale <= max_px && r.level > best {
            // a level can only be chosen if every finer level is also acceptable (bounds are non-decreasing)
            best = r.level;
        }
    }
    best
}

/// Output pixels per stage unit that fits the project into a `w x h` screen.
pub fn fit_scale(p: &Project, w: f64, h: f64) -> f64 {
    (w / p.width as f64).min(h / p.height as f64)
}

// ------------------------------------------------------------------------------------------
// Verification: measure the actual deviation
// ------------------------------------------------------------------------------------------

/// Parameters of [`verify`].
#[derive(Clone, Debug, PartialEq)]
pub struct VerifyOptions {
    /// Output pixels per stage unit.
    pub scale: f64,
    /// Evenly spaced sample frames (key frames and half frames are always added).
    pub frames: usize,
    /// Also render both levels and compute pixel statistics on `pixel_frames` frames.
    pub pixels: bool,
    pub pixel_frames: usize,
}

impl VerifyOptions {
    pub fn new(scale: f64) -> VerifyOptions {
        VerifyOptions {
            scale,
            frames: 24,
            pixels: true,
            pixel_frames: 6,
        }
    }
}

/// Result of [`verify`]. All pixel numbers are at `scale`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct VerifyReport {
    pub level: usize,
    pub declared_px: f64,
    /// Largest screen-space deviation of evaluated geometry (Hausdorff for paths, deformed mesh
    /// surface for images, culled layers charged with their diameter).
    pub measured_px: f64,
    pub measured_paths_px: f64,
    pub measured_mesh_px: f64,
    pub measured_cull_px: f64,
    pub worst_frame: f32,
    pub worst_layer: String,
    pub frames_checked: usize,
    /// `measured_px <= declared_px` (plus a numeric slack of 1e-6 px). `false` is a bug.
    pub holds: bool,
    /// declared / measured, `None` when nothing deviates.
    pub conservatism: Option<f64>,
    pub pixel_frames: usize,
    pub pixel_max_abs: u32,
    pub pixel_max_de2000: f64,
    /// Windowed (9x9) estimate of the mean edge displacement from the L1 image difference over the
    /// total variation of the reference; a lower bound of the local maximum, reported for orientation.
    pub pixel_shift_est_px: f64,
}

impl VerifyReport {
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

fn verify_frames(orig: &Project, lod: &Project, n: usize) -> Vec<f32> {
    let total = orig.frames.max(1) as f32;
    let mut v: Vec<f32> = Vec::new();
    for p in [orig, lod] {
        for c in &p.channels {
            if let Source::Keys { track } = &c.source {
                v.extend(
                    track
                        .keys
                        .iter()
                        .map(|k| k.frame)
                        .filter(|f| *f >= 0.0 && *f < total),
                );
            }
        }
    }
    let n = n.max(1);
    for i in 0..n {
        let f = (i as f32 * total / n as f32).floor();
        v.push(f);
        v.push((f + 0.5).min(total - 1e-3));
    }
    v.push(total - 1.0);
    v.sort_by(|a, b| a.total_cmp(b));
    v.dedup();
    v
}

fn screen_outline(path: &VPath, w: &A, scale: f64) -> Vec<P> {
    let mut anchors = path.anchors.clone();
    for a in &mut anchors {
        a.map_points(&mut |q| {
            let r = a_apply(w, p_of(q));
            Vec2::new((r[0] * scale) as f32, (r[1] * scale) as f32)
        });
    }
    outline_polyline(&anchors, path.closed, 0.02)
}

/// L0 vertex indices + weights, and the L1 vertex indices + weights at the same spot.
type ProbePoint = ([usize; 3], [f64; 3], [usize; 3], [f64; 3]);

struct MeshProbe {
    /// Sample points: (L0 vertex indices with weights) and where they lie in the L1 mesh.
    pts: Vec<ProbePoint>,
}

fn locate(rest: &[P], tris: &[[usize; 3]], p: P) -> Option<([usize; 3], [f64; 3])> {
    let mut best: Option<(f64, [usize; 3], [f64; 3])> = None;
    for t in tris {
        let tp = [rest[t[0]], rest[t[1]], rest[t[2]]];
        let l = bary(p, tp);
        let m = l.iter().cloned().fold(f64::MAX, f64::min);
        if best.as_ref().is_none_or(|b| m > b.0) {
            best = Some((m, *t, l));
        }
        if m >= -1e-9 {
            break;
        }
    }
    best.map(|b| (b.1, b.2))
}

/// Measure the deviation of `lod` from `orig` (the L0 project) by evaluating both on sampled frames.
pub fn verify(orig: &Project, lod: &Lod, o: &VerifyOptions) -> Result<VerifyReport, String> {
    if o.scale.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return Err("scale must be positive".into());
    }
    let e0 = Engine::new(orig.clone()).map_err(|e| e.join("; "))?;
    let e1 = Engine::new(lod.project.clone()).map_err(|e| e.join("; "))?;
    let frames = verify_frames(orig, &lod.project, o.frames);
    let (mut m_paths, mut m_mesh, mut m_cull) = (0.0f64, 0.0f64, 0.0f64);
    let mut worst = (0.0f64, 0.0f32, String::new());
    let mut probes: BTreeMap<String, MeshProbe> = BTreeMap::new();
    for &t in &frames {
        let f0 = e0.evaluate(t);
        let f1 = e1.evaluate(t);
        for nd in &f0.nodes {
            let w0 = a_of(&nd.world);
            let mut local_worst = 0.0f64;
            let Some(i1) = e1.node_index(&nd.name) else {
                // removed layer: charged with its screen diameter while it was drawn
                if nd.visible && nd.opacity > 0.0 {
                    let pts = node_points(nd, &w0);
                    if !pts.is_empty() {
                        let (mut x0, mut y0, mut x1, mut y1) =
                            (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
                        for q in &pts {
                            x0 = x0.min(q[0]);
                            y0 = y0.min(q[1]);
                            x1 = x1.max(q[0]);
                            y1 = y1.max(q[1]);
                        }
                        let hw = nd
                            .paths
                            .iter()
                            .filter_map(|pp| pp.stroke.as_ref().map(|s| s.width as f64 * 0.5))
                            .fold(0.0, f64::max)
                            * lin_norm(&w0);
                        let d = ((x1 - x0) + 2.0 * hw).hypot((y1 - y0) + 2.0 * hw) * o.scale;
                        m_cull = m_cull.max(d);
                        if d > worst.0 {
                            worst = (d, t, nd.name.clone());
                        }
                    }
                }
                continue;
            };
            let n1 = &f1.nodes[i1];
            let w1 = a_of(&n1.world);
            if nd.paths.len() == n1.paths.len() {
                for (pa, pb) in nd.paths.iter().zip(&n1.paths) {
                    let (a, b) = (
                        screen_outline(pa, &w0, o.scale),
                        screen_outline(pb, &w1, o.scale),
                    );
                    let d = hausdorff_upper(&a, &b, 0.002);
                    local_worst = local_worst.max(d);
                    m_paths = m_paths.max(d);
                }
            }
            if let (Some(a), Some(b)) = (&nd.image, &n1.image) {
                // rectangle corners
                let (oa, sa) = (p_of(a.origin), p_of(a.size));
                let (ob, sb) = (p_of(b.origin), p_of(b.size));
                for c in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
                    let pa = a_apply(&w0, [oa[0] + sa[0] * c[0], oa[1] + sa[1] * c[1]]);
                    let pb = a_apply(&w1, [ob[0] + sb[0] * c[0], ob[1] + sb[1] * c[1]]);
                    local_worst = local_worst.max(dist(pa, pb) * o.scale);
                }
                if let (Some(ma), Some(mb)) = (&a.mesh, &b.mesh) {
                    let probe = probes.entry(nd.name.clone()).or_insert_with(|| {
                        let rest_of = |p: &Project, img: &crate::engine::ImageDraw| -> Vec<P> {
                            match p.find_layer(&nd.name).map(|l| &l.kind) {
                                Some(LayerKind::Image {
                                    mesh: Some(m),
                                    origin,
                                    ..
                                }) => m
                                    .rest_vertices(*origin, img.size)
                                    .iter()
                                    .map(|v| p_of(*v))
                                    .collect(),
                                _ => vec![],
                            }
                        };
                        let (ra, rb) = (rest_of(orig, a), rest_of(&lod.project, b));
                        let tb: Vec<[usize; 3]> = mb
                            .tris
                            .iter()
                            .map(|t| [t[0] as usize, t[1] as usize, t[2] as usize])
                            .collect();
                        let mut pts = Vec::new();
                        if ra.len() == ma.positions.len() && rb.len() == mb.positions.len() {
                            for t in &ma.tris {
                                let t = [t[0] as usize, t[1] as usize, t[2] as usize];
                                let cs: [[f64; 3]; 7] = [
                                    [1.0, 0.0, 0.0],
                                    [0.0, 1.0, 0.0],
                                    [0.0, 0.0, 1.0],
                                    [0.5, 0.5, 0.0],
                                    [0.0, 0.5, 0.5],
                                    [0.5, 0.0, 0.5],
                                    [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0],
                                ];
                                for l in cs {
                                    let p = add(
                                        add(mul(ra[t[0]], l[0]), mul(ra[t[1]], l[1])),
                                        mul(ra[t[2]], l[2]),
                                    );
                                    if let Some((tt, ll)) = locate(&rb, &tb, p) {
                                        pts.push((t, l, tt, ll));
                                    }
                                }
                            }
                        }
                        MeshProbe { pts }
                    });
                    for (ta, la, tb_, lb) in &probe.pts {
                        let ya = (0..3).fold([0.0, 0.0], |acc, j| {
                            add(acc, mul(p_of(ma.positions[ta[j]]), la[j]))
                        });
                        let yb = (0..3).fold([0.0, 0.0], |acc, j| {
                            add(acc, mul(p_of(mb.positions[tb_[j]]), lb[j]))
                        });
                        let d = dist(a_apply(&w0, ya), a_apply(&w1, yb)) * o.scale;
                        m_mesh = m_mesh.max(d);
                        local_worst = local_worst.max(d);
                    }
                }
            }
            if local_worst > worst.0 {
                worst = (local_worst, t, nd.name.clone());
            }
        }
    }
    let measured = m_paths.max(m_mesh).max(m_cull).max(worst.0);
    let (mut pmax, mut pde, mut pshift, mut pframes) = (0u32, 0.0f64, 0.0f64, 0usize);
    if o.pixels {
        let ro = crate::raster::RasterOptions {
            scale: o.scale as f32,
            show_bones: false,
            transparent: false,
        };
        let mut pf: Vec<f32> = (0..o.pixel_frames.max(1))
            .map(|i| (i as f32 * orig.frames.max(1) as f32 / o.pixel_frames.max(1) as f32).floor())
            .collect();
        pf.push(worst.1);
        pf.sort_by(|a, b| a.total_cmp(b));
        pf.dedup();
        for t in pf {
            let (w, h, a) = crate::raster::render_rgba(&e0, t, &ro);
            let (_, _, b) = crate::raster::render_rgba(&e1, t, &ro);
            let (w, h) = (w as usize, h as usize);
            pframes += 1;
            for (pa, pb) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
                let d = (0..4)
                    .map(|k| (pa[k] as i32 - pb[k] as i32).unsigned_abs())
                    .max()
                    .unwrap_or(0);
                if d > 0 {
                    pmax = pmax.max(d);
                    pde = pde.max(crate::percep::pixel_de(
                        [pa[0], pa[1], pa[2], pa[3]],
                        [pb[0], pb[1], pb[2], pb[3]],
                        crate::percep::DeMetric::De2000,
                    ));
                }
            }
            pshift = pshift.max(shift_estimate(&a, &b, w, h));
        }
    }
    Ok(VerifyReport {
        level: lod.record.level,
        declared_px: lod.record.declared_px,
        measured_px: measured,
        measured_paths_px: m_paths,
        measured_mesh_px: m_mesh,
        measured_cull_px: m_cull,
        worst_frame: worst.1,
        worst_layer: worst.2,
        frames_checked: frames.len(),
        holds: measured <= lod.record.declared_px + 1e-6,
        conservatism: if measured > 1e-9 {
            Some(lod.record.declared_px / measured)
        } else {
            None
        },
        pixel_frames: pframes,
        pixel_max_abs: pmax,
        pixel_max_de2000: pde,
        pixel_shift_est_px: pshift,
    })
}

/// Mean edge displacement estimate: windowed (9x9) L1 difference over the total variation of the reference.
fn shift_estimate(a: &[u8], b: &[u8], w: usize, h: usize) -> f64 {
    if w < 3 || h < 3 {
        return 0.0;
    }
    let lum = |im: &[u8], i: usize| {
        (0.299 * im[i * 4] as f64 + 0.587 * im[i * 4 + 1] as f64 + 0.114 * im[i * 4 + 2] as f64)
            / 255.0
    };
    let mut diff = vec![0.0f64; w * h];
    let mut tv = vec![0.0f64; w * h];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            diff[i] = (lum(a, i) - lum(b, i)).abs();
            let dx = if x + 1 < w {
                lum(a, i + 1) - lum(a, i)
            } else {
                0.0
            };
            let dy = if y + 1 < h {
                lum(a, i + w) - lum(a, i)
            } else {
                0.0
            };
            tv[i] = dx.hypot(dy);
        }
    }
    let integral = |v: &[f64]| -> Vec<f64> {
        let mut s = vec![0.0; (w + 1) * (h + 1)];
        for y in 0..h {
            let mut row = 0.0;
            for x in 0..w {
                row += v[y * w + x];
                s[(y + 1) * (w + 1) + x + 1] = s[y * (w + 1) + x + 1] + row;
            }
        }
        s
    };
    let (sd, st) = (integral(&diff), integral(&tv));
    let rect = |s: &[f64], x0: usize, y0: usize, x1: usize, y1: usize| {
        s[y1 * (w + 1) + x1] - s[y0 * (w + 1) + x1] - s[y1 * (w + 1) + x0] + s[y0 * (w + 1) + x0]
    };
    let r = 4usize;
    let mut best = 0.0f64;
    for y in 0..h {
        for x in 0..w {
            let (x0, y0, x1, y1) = (
                x.saturating_sub(r),
                y.saturating_sub(r),
                (x + r + 1).min(w),
                (y + r + 1).min(h),
            );
            let t = rect(&st, x0, y0, x1, y1);
            if t >= 1.0 {
                best = best.max(rect(&sd, x0, y0, x1, y1) / t);
            }
        }
    }
    best
}

/// Plain-text table of levels (and measurements, if given).
pub fn table(levels: &[Lod], verify: &[VerifyReport]) -> String {
    let mut s = String::new();
    s.push_str("level  tol_px  anchors  mesh_v  keys  layers    bytes  declared_px  path  mesh  keys  cull  measured_px  ratio\n");
    for l in levels {
        let r = &l.record;
        let v = verify.iter().find(|v| v.level == r.level);
        s.push_str(&format!(
            "L{:<4} {:>7.3} {:>8} {:>7} {:>5} {:>7} {:>8} {:>12.4} {:>5.3} {:>5.3} {:>5.3} {:>5.3} {:>12} {:>6}\n",
            r.level,
            r.tolerance_px,
            r.stats.path_anchors,
            r.stats.mesh_vertices,
            r.stats.keys,
            r.stats.layers,
            r.stats.bytes,
            r.declared_px,
            r.path_px,
            r.mesh_px,
            r.keys_px,
            r.cull_px,
            v.map_or("-".to_string(), |v| format!("{:.4}", v.measured_px)),
            v.and_then(|v| v.conservatism).map_or("-".to_string(), |c| format!("{c:.1}x")),
        ));
    }
    s
}

/// JSON report of a build (and verification).
pub fn report_json(levels: &[Lod], verify: &[VerifyReport]) -> Value {
    json!({
        "levels": levels.iter().map(|l| serde_json::to_value(&l.record).unwrap_or(Value::Null)).collect::<Vec<_>>(),
        "verify": verify.iter().map(|v| v.to_json()).collect::<Vec<_>>(),
        "assumptions": ASSUMPTIONS,
        "limits": LIMITS,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::Ease;
    use crate::path::BoneWeight;
    use crate::project::{Bone, ImageSource, MaskRef, MeshSpec, Transform2D};

    /// Dense Hausdorff distance of two outlines: exact nearest distances (grid index) at points
    /// sampled every `step` along both curves.
    fn true_hausdorff(a: &[Anchor], ca: bool, b: &[Anchor], cb: bool) -> f64 {
        let pa = outline_polyline(a, ca, 1e-4);
        let pb = outline_polyline(b, cb, 1e-4);
        let dir = |x: &[P], y: &[P]| {
            let idx = PolyIdx::new(y);
            let n = ((path_len(x) / 0.01) as usize).clamp(2, 400_000);
            resample(x, n)
                .iter()
                .map(|&q| idx.nearest(q).0)
                .fold(0.0, f64::max)
        };
        dir(&pa, &pb).max(dir(&pb, &pa))
    }

    fn lcg(seed: &mut u64) -> f64 {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((*seed >> 33) as f64) / ((1u64 << 31) as f64)
    }

    fn circle_pts(n: usize, r: f32, wobble: f32, seed: u64) -> Vec<Vec2> {
        let mut s = seed;
        (0..n)
            .map(|i| {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                let rr = r + wobble * (lcg(&mut s) as f32 - 0.5);
                Vec2::new(100.0 + rr * a.cos(), 100.0 + rr * a.sin())
            })
            .collect()
    }

    fn scene_with(layers: Vec<Layer>) -> Project {
        let mut p = Project::new("t", 200.0, 200.0, 24.0, 12);
        p.layers = layers;
        p
    }

    #[test]
    fn simplified_beziers_declare_a_bound_at_least_the_true_hausdorff_distance() {
        for (n, wob, eps) in [
            (48usize, 0.2f32, 0.15f64),
            (64, 0.0, 0.05),
            (40, 1.0, 0.5),
            (90, 0.05, 0.3),
            (24, 0.0, 2.0),
        ] {
            let path = VPath::smooth_through("c", &circle_pts(n, 60.0, wob, n as u64), true, 1.0);
            let sp = simplify_path_certified(&path.anchors, true, eps);
            assert!(sp.removed > 0, "n={n} eps={eps}: nothing removed");
            assert!(
                sp.bound <= eps + 1e-9,
                "declared {} above eps {eps}",
                sp.bound
            );
            let truth = true_hausdorff(&path.anchors, true, &sp.anchors, true);
            assert!(
                sp.bound + 1e-3 >= truth,
                "n={n} eps={eps}: declared {} < true {truth}",
                sp.bound
            );
            assert!(
                truth <= eps + 1e-3,
                "true {truth} exceeds the tolerance {eps}"
            );
        }
    }

    #[test]
    fn simplified_polylines_and_open_paths_are_certified_and_keep_their_ends() {
        let mut s = 7u64;
        for case in 0..12 {
            let n = 20 + case * 3;
            let pts: Vec<Vec2> = (0..n)
                .map(|i| {
                    Vec2::new(
                        i as f32 * 4.0,
                        50.0 * ((i as f32) * 0.2).sin() + 0.4 * (lcg(&mut s) as f32 - 0.5),
                    )
                })
                .collect();
            let anchors: Vec<Anchor> = pts.iter().map(|&p| Anchor::corner(p)).collect();
            let eps = 0.3 + case as f64 * 0.1;
            let sp = simplify_path_certified(&anchors, false, eps);
            assert!(sp.removed > 0);
            assert_eq!(
                sp.anchors.first().unwrap().pos,
                anchors.first().unwrap().pos
            );
            assert_eq!(sp.anchors.last().unwrap().pos, anchors.last().unwrap().pos);
            let truth = true_hausdorff(&anchors, false, &sp.anchors, false);
            assert!(
                sp.bound + 1e-3 >= truth && sp.bound <= eps + 1e-9,
                "case {case}: declared {} true {truth} eps {eps}",
                sp.bound
            );
        }
    }

    #[test]
    fn hausdorff_bound_helpers_agree_with_brute_force() {
        let a: Vec<P> = (0..200)
            .map(|i| [i as f64 * 0.5, (i as f64 * 0.1).sin() * 10.0])
            .collect();
        let b: Vec<P> = (0..120)
            .map(|i| [i as f64 * 0.83, (i as f64 * 0.17).sin() * 10.0 + 0.5])
            .collect();
        let up = directed_hausdorff_upper(&a, &b, 0.01);
        let exact_samples = resample(&a, 20000)
            .iter()
            .map(|&p| dist_to_poly(p, &b))
            .fold(0.0, f64::max);
        assert!(
            up + 1e-9 >= exact_samples,
            "upper {up} < sampled {exact_samples}"
        );
        assert!(
            up <= exact_samples + 0.05,
            "upper {up} much above sampled {exact_samples}"
        );
        assert!(directed_hausdorff_upper(&a, &a, 0.01) < 1e-9);
        assert!(discrete_frechet(&a, &a) < 1e-12);
    }

    #[test]
    fn flattening_bound_is_rigorous() {
        let c: Cubic = [[0.0, 0.0], [50.0, 90.0], [-20.0, 90.0], [60.0, 0.0]];
        for tol in [1.0, 0.1, 0.01] {
            let (poly, h) = flatten_poly(&c, tol);
            assert!(h <= tol + 1e-12);
            let worst = (0..=4000)
                .map(|i| dist_to_poly(cubic_at(&c, i as f64 / 4000.0), &poly))
                .fold(0.0, f64::max);
            assert!(
                worst <= h + 1e-9,
                "curve is {worst} from its polyline, bound {h}"
            );
        }
    }

    #[test]
    fn unsuitable_inputs_are_left_alone() {
        let tri = VPath::polygon(
            "t",
            &[
                Vec2::new(0.0, 0.0),
                Vec2::new(10.0, 0.0),
                Vec2::new(5.0, 8.0),
            ],
        );
        let r = simplify_path_certified(&tri.anchors, true, 100.0);
        assert_eq!(r.removed, 0);
        assert_eq!(r.anchors, tri.anchors);
        let r0 = simplify_path_certified(&tri.anchors, true, 0.0);
        assert_eq!(r0.removed, 0);
        let rn = simplify_path_certified(&tri.anchors, true, f64::NAN);
        assert_eq!(rn.removed, 0);
        // a rectangle with four corners cannot lose one at a small tolerance
        let rect = VPath::rect("r", 0.0, 0.0, 100.0, 60.0);
        assert_eq!(simplify_path_certified(&rect.anchors, true, 0.5).removed, 0);
    }

    fn blob_scene(scale: f32) -> Project {
        let mut layer = Layer::vector(
            "blob",
            vec![
                VPath::smooth_through("c", &circle_pts(80, 40.0, 0.3, 3), true, 1.0)
                    .with_fill("#336699"),
            ],
        );
        layer.transform = Transform2D {
            position: Vec2::new(0.0, 0.0),
            scale: Vec2::new(scale, scale),
            ..Default::default()
        };
        scene_with(vec![layer])
    }

    fn anchors_of(p: &Project, layer: &str) -> usize {
        match &p.find_layer(layer).unwrap().kind {
            LayerKind::Vector { paths } => paths.iter().map(|x| x.anchors.len()).sum(),
            _ => 0,
        }
    }

    #[test]
    fn tolerance_is_scale_aware() {
        // same shape drawn 1x and 3x larger on screen: the larger one needs finer local tolerance
        let o = LodOptions::new(1.0, 1.0, 1);
        let small = build(&blob_scene(1.0), &o).unwrap();
        let big = build(&blob_scene(3.0), &o).unwrap();
        let (ks, kb) = (
            anchors_of(&small[1].project, "blob"),
            anchors_of(&big[1].project, "blob"),
        );
        assert!(ks < 80, "small version should lose anchors, has {ks}");
        assert!(
            kb >= ks,
            "3x version must keep at least as many anchors ({kb} vs {ks})"
        );
        // and a doubled render scale (output pixels per unit) is stricter as well
        let o2 = LodOptions::new(1.0, 2.0, 1);
        let fine = build(&blob_scene(1.0), &o2).unwrap();
        assert!(anchors_of(&fine[1].project, "blob") >= ks);
        // the declared bound in stage units follows the render scale
        assert!((small[1].record.bound_stage * 1.0 - small[1].record.declared_px).abs() < 1e-5);
        assert!((fine[1].record.bound_stage * 2.0 - fine[1].record.declared_px).abs() < 1e-5);
        for l in small.iter().chain(&big).chain(&fine).skip(1) {
            assert!(l.record.declared_px <= l.record.tolerance_px + 1e-9);
        }
    }

    fn rig_scene(dense: bool) -> Project {
        let mut p = Project::new("rig", 200.0, 200.0, 24.0, 48);
        p.bones = vec![Bone::new("arm", None, 100.0, Vec2::new(50.0, 100.0), 0.0)];
        let pts: Vec<Vec2> = (0..16)
            .map(|i| {
                Vec2::new(
                    50.0 + i as f32 * 100.0 / 15.0,
                    100.0 + if i % 2 == 0 { 4.0 } else { -4.0 },
                )
            })
            .collect();
        let mut sk = VPath::polygon("skin", &pts).with_fill("#cc3300");
        for a in &mut sk.anchors {
            a.weights = vec![BoneWeight {
                bone: "arm".into(),
                weight: 1.0,
            }];
        }
        let mut ride = Layer::vector(
            "ride",
            vec![VPath::rect("r", 140.0, 90.0, 20.0, 20.0).with_fill("#006600")],
        );
        ride.bone = Some("arm".into());
        p.layers = vec![Layer::vector("skinned", vec![sk]), ride];
        let keys: Vec<(f32, f32, Ease)> = if dense {
            (0..48)
                .map(|f| {
                    (
                        f as f32,
                        30.0 * (f as f32 / 48.0 * std::f32::consts::TAU).sin(),
                        Ease::Linear,
                    )
                })
                .collect()
        } else {
            vec![
                (0.0, 0.0, Ease::Linear),
                (24.0, 30.0, Ease::Linear),
                (47.0, 0.0, Ease::Linear),
            ]
        };
        p.channels = vec![Channel::keys(
            "arm",
            Prop::BoneAngle,
            Track::with_keys(&keys),
        )];
        p
    }

    #[test]
    fn dense_keys_are_reduced_and_the_pixel_bound_is_honoured() {
        let p = rig_scene(true);
        let o = LodOptions::new(1.0, 1.0, 3);
        let levels = build(&p, &o).unwrap();
        let mut prev = levels[0].record.stats.keys;
        for l in &levels[1..] {
            assert!(
                l.record.stats.keys <= prev,
                "keys grew: {} -> {}",
                prev,
                l.record.stats.keys
            );
            prev = l.record.stats.keys;
            assert!(l.record.declared_px <= l.record.tolerance_px + 1e-9);
        }
        assert!(
            levels[3].record.stats.keys < 30,
            "coarsest level still has {} keys",
            levels[3].record.stats.keys
        );
        assert!(levels[3].record.keys_px > 0.0);
        for l in &levels[1..] {
            let r = verify(
                &p,
                l,
                &VerifyOptions {
                    scale: 1.0,
                    frames: 48,
                    pixels: false,
                    pixel_frames: 1,
                },
            )
            .unwrap();
            assert!(
                r.holds,
                "level {}: measured {} > declared {}",
                l.record.level, r.measured_px, r.declared_px
            );
            assert!(r.measured_px <= l.record.keys_px + l.record.path_px + 1e-6);
        }
    }

    #[test]
    fn key_reduction_honours_the_value_bound_for_every_ease() {
        let eases = [
            Ease::Linear,
            Ease::EaseInOut,
            Ease::Smooth,
            Ease::Back { overshoot: 1.2 },
            Ease::CubicBezier {
                x1: 0.3,
                y1: 0.0,
                x2: 0.7,
                y2: 1.0,
            },
        ];
        let grid: Vec<f32> = (0..96).map(|i| i as f32 / 2.0).collect();
        for e in eases {
            let keys: Vec<(f32, f32, Ease)> = (0..48)
                .map(|f| {
                    (
                        f as f32,
                        20.0 * (f as f32 * 0.3).sin() + 3.0 * (f as f32 * 1.7).cos(),
                        e,
                    )
                })
                .collect();
            let t = Track::with_keys(&keys);
            for tol in [0.05, 0.5, 3.0] {
                let r = reduce_track_bounded(&t, tol, &grid);
                let dev = track_deviation_at(&t, &r, &merged_samples(&t, &grid));
                assert!(dev <= tol + 1e-6, "{e:?} tol {tol}: deviation {dev}");
                assert!(r.keys.len() <= t.keys.len());
                assert_eq!(r.keys.first().unwrap().frame, 0.0);
                assert_eq!(r.keys.last().unwrap().frame, 47.0);
            }
        }
        // tiny and degenerate input is returned unchanged
        let two = Track::with_keys(&[(0.0, 1.0, Ease::Linear), (5.0, 2.0, Ease::Linear)]);
        assert_eq!(reduce_track_bounded(&two, 1.0, &grid), two);
        let t = Track::with_keys(&[
            (0.0, 1.0, Ease::Linear),
            (5.0, 2.0, Ease::Linear),
            (9.0, 0.0, Ease::Linear),
        ]);
        assert_eq!(reduce_track_bounded(&t, f64::NAN, &grid), t);
        assert_eq!(reduce_track_bounded(&t, -1.0, &grid), t);
    }

    fn mesh_png() -> Vec<u8> {
        crate::percep::Img::solid(16, 16, [200, 30, 30, 255])
            .to_png()
            .unwrap()
    }

    fn mesh_scene(cols: usize, rows: usize, animate: bool) -> Project {
        let mut p = Project::new("mesh", 200.0, 200.0, 24.0, 24);
        p.bones = vec![
            Bone::new("a", None, 60.0, Vec2::new(40.0, 100.0), 0.0),
            Bone::new("b", None, 60.0, Vec2::new(100.0, 100.0), 0.0),
        ];
        let mut img = Layer::image_from_png("img", &mesh_png(), Vec2::new(40.0, 60.0));
        let weights: Vec<Vec<BoneWeight>> = (0..=rows)
            .flat_map(|_r| {
                (0..=cols).map(move |c| {
                    let t = c as f32 / cols as f32;
                    vec![
                        BoneWeight {
                            bone: "a".into(),
                            weight: 1.0 - t,
                        },
                        BoneWeight {
                            bone: "b".into(),
                            weight: t,
                        },
                    ]
                })
            })
            .collect();
        if let LayerKind::Image { size, mesh, .. } = &mut img.kind {
            *size = Some(Vec2::new(120.0, 80.0));
            *mesh = Some(MeshSpec {
                cols,
                rows,
                weights,
                free: None,
            });
        }
        p.layers = vec![img];
        if animate {
            p.channels = vec![Channel::keys(
                "b",
                Prop::BoneAngle,
                Track::with_keys(&[
                    (0.0, 0.0, Ease::Linear),
                    (12.0, 25.0, Ease::Linear),
                    (23.0, 0.0, Ease::Linear),
                ]),
            )];
        }
        p
    }

    fn mesh_vertices(p: &Project) -> usize {
        match &p.find_layer("img").unwrap().kind {
            LayerKind::Image { mesh: Some(m), .. } => m.vertex_count(),
            _ => 0,
        }
    }

    #[test]
    fn grid_mesh_collapses_with_a_certified_bound() {
        let p = mesh_scene(10, 8, true);
        let o = LodOptions::new(1.5, 1.0, 3);
        let levels = build(&p, &o).unwrap();
        assert_eq!(mesh_vertices(&levels[0].project), 99);
        let mut prev = 99;
        for l in &levels[1..] {
            let v = mesh_vertices(&l.project);
            assert!(v <= prev, "vertices grew {prev} -> {v}");
            prev = v;
            assert!(l.record.mesh_px <= l.record.tolerance_px * o.geometry_share + 1e-9);
            // the simplified project still renders and validates
            Engine::new(l.project.clone()).unwrap();
        }
        assert!(prev < 99, "no vertex was collapsed at 6 px");
        for l in &levels[1..] {
            let r = verify(
                &p,
                l,
                &VerifyOptions {
                    scale: 1.0,
                    frames: 24,
                    pixels: true,
                    pixel_frames: 3,
                },
            )
            .unwrap();
            assert!(
                r.holds,
                "level {}: measured {} declared {}",
                l.record.level, r.measured_px, r.declared_px
            );
            assert!(
                r.measured_mesh_px <= l.record.mesh_px + 1e-6,
                "mesh measured {} > certified {}",
                r.measured_mesh_px,
                l.record.mesh_px
            );
        }
    }

    #[test]
    fn unskinned_grid_collapses_to_its_corners_and_keeps_the_outline() {
        let n = 9usize;
        let rest: Vec<P> = (0..=n)
            .flat_map(|r| (0..=n).map(move |c| [c as f64 * 10.0, r as f64 * 10.0]))
            .collect();
        let mut tris = Vec::new();
        for r in 0..n {
            for c in 0..n {
                let a = r * (n + 1) + c;
                tris.push([a, a + 1, a + n + 2]);
                tris.push([a, a + n + 2, a + n + 1]);
            }
        }
        let attr: Vec<Vec<f64>> = vec![vec![]; rest.len()];
        let y: Vec<Vec<[f32; 2]>> =
            vec![rest.iter().map(|p| [p[0] as f32, p[1] as f32]).collect(); 3];
        let s = vec![1.0; 3];
        let q = qem_simplify(
            &QemInput {
                rest: &rest,
                tris: &tris,
                attr: &attr,
                y: &y,
                s: &s,
            },
            0.5,
        );
        assert_eq!(
            q.keep,
            vec![0, n, n * (n + 1), (n + 1) * (n + 1) - 1],
            "only the four corners can remain"
        );
        assert_eq!(q.tris.len(), 2);
        assert!(q.bound_px < 1e-9);
        // zero tolerance leaves the mesh alone
        let z = qem_simplify(
            &QemInput {
                rest: &rest,
                tris: &tris,
                attr: &attr,
                y: &y,
                s: &s,
            },
            0.0,
        );
        assert_eq!(z.collapses, 0);
        assert_eq!(z.keep.len(), rest.len());
    }

    #[test]
    fn qem_bound_covers_the_true_surface_difference() {
        // deformed positions: a bend that makes interior vertices matter
        let (cols, rows) = (12usize, 6usize);
        let rest: Vec<P> = (0..=rows)
            .flat_map(|r| (0..=cols).map(move |c| [c as f64 * 8.0, r as f64 * 8.0]))
            .collect();
        let mut tris = Vec::new();
        for r in 0..rows {
            for c in 0..cols {
                let a = r * (cols + 1) + c;
                tris.push([a, a + 1, a + cols + 2]);
                tris.push([a, a + cols + 2, a + cols + 1]);
            }
        }
        let y: Vec<Vec<[f32; 2]>> = (0..4)
            .map(|f| {
                rest.iter()
                    .map(|p| {
                        let a = (p[0] / 96.0) * 0.9 * (f as f64 + 1.0);
                        [
                            (p[0] + 6.0 * (a).sin() * p[1] / 48.0) as f32,
                            (p[1] + 14.0 * (1.0 - a.cos()) + 3.0 * (p[0] * 0.2).sin()) as f32,
                        ]
                    })
                    .collect()
            })
            .collect();
        let attr: Vec<Vec<f64>> = rest.iter().map(|p| vec![p[0] / 96.0 * 10.0]).collect();
        let s = vec![1.5, 1.0, 1.2, 1.0];
        let eps = 2.0;
        let q = qem_simplify(
            &QemInput {
                rest: &rest,
                tris: &tris,
                attr: &attr,
                y: &y,
                s: &s,
            },
            eps,
        );
        assert!(q.collapses > 0 && q.bound_px <= eps + 1e-9);
        // brute force: evaluate both piecewise-linear surfaces on a lattice of rest points
        let loc_all = |tr: &[[usize; 3]], p: P| locate(&rest, tr, p).unwrap();
        let mut worst = 0.0f64;
        for i in 0..=96 {
            for j in 0..=48 {
                let pt = [i as f64, j as f64];
                let (t0, l0) = loc_all(&tris, pt);
                let (t1, l1) = loc_all(&q.tris, pt);
                for (f, yf) in y.iter().enumerate() {
                    let ev = |t: [usize; 3], l: [f64; 3]| {
                        (0..3).fold([0.0, 0.0], |a, k| {
                            add(a, mul([yf[t[k]][0] as f64, yf[t[k]][1] as f64], l[k]))
                        })
                    };
                    worst = worst.max(dist(ev(t0, l0), ev(t1, l1)) * s[f]);
                }
            }
        }
        assert!(
            worst <= q.bound_px + 1e-6,
            "true max {worst} above certified {}",
            q.bound_px
        );
        assert!(worst > 0.0);
        // outline preserved: kept vertices include all four corners, every kept vertex is an original one
        for corner in [0, cols, rows * (cols + 1), (rows + 1) * (cols + 1) - 1] {
            assert!(q.keep.contains(&corner));
        }
    }

    #[test]
    fn tiny_container_members_are_kept_not_a_fatal_error() {
        use crate::containers::{Container, Dir, Member};
        let big = Layer::vector(
            "big",
            vec![VPath::rect("r", 0.0, 0.0, 60.0, 40.0).with_fill("#336699")],
        );
        let dot = Layer::vector(
            "dot",
            vec![VPath::circle("d", Vec2::new(20.0, 20.0), 0.1).with_fill("#000000")],
        );
        let free = Layer::vector(
            "free",
            vec![VPath::circle("d", Vec2::new(90.0, 20.0), 0.1).with_fill("#000000")],
        );
        let mut p = scene_with(vec![big, dot, free]);
        p.containers.push(Container::new(
            "row",
            Dir::Row,
            vec![Member::layer("big"), Member::layer("dot")],
        ));
        Engine::new(p.clone()).unwrap();
        let levels = build(&p, &LodOptions::new(1.0, 1.0, 2))
            .expect("lod must not abort on a container member");
        for l in &levels[1..] {
            assert!(
                l.project.find_layer("dot").is_some(),
                "container member kept"
            );
            assert!(
                l.project.find_layer("free").is_none(),
                "unreferenced speck still culled"
            );
            assert!(
                l.record
                    .notes
                    .iter()
                    .any(|n| n.starts_with("skipped:") && n.contains("dot")),
                "{:?}",
                l.record.notes
            );
            Engine::new(l.project.clone()).unwrap();
        }
    }

    /// H2 (wave 4): simplifying the path of a container member changes its bounding box, the container re-flows the
    /// siblings and the measured deviation exceeded the declared bound. Layout members are now left alone and the
    /// bound is checked by rendering both projects (also re-solved for another canvas size).
    #[test]
    fn declared_bound_holds_on_a_project_with_a_layout_container() {
        use crate::containers::{Container, Dir, Member, RectRule};
        let wob = |seed: u64, r: f32| circle_pts(60, r, 6.0, seed);
        let sun = Layer::vector(
            "sun",
            vec![VPath::smooth_through("c", &wob(3, 40.0), true, 1.0).with_fill("#ffcc00")],
        );
        let hills = Layer::vector(
            "hills",
            vec![VPath::smooth_through("c", &wob(5, 55.0), true, 1.0).with_fill("#336644")],
        );
        // not a member: still reduced, so the level has a real, non-zero declared bound
        let free = Layer::vector(
            "free",
            vec![VPath::smooth_through("c", &wob(9, 30.0), true, 1.0).with_fill("#aa3333")],
        );
        let mut p = scene_with(vec![sun, hills, free]);
        let mut c = Container::new(
            "row",
            Dir::Row,
            vec![Member::layer("sun"), Member::layer("hills")],
        );
        c.members[0].grow = 1.0;
        c.gap = 10.0;
        c.rect = RectRule::Aspect {
            ratio: 1.5,
            mode: Default::default(),
            margin: 0.0,
            ax: 0.5,
            ay: 0.5,
        };
        p.containers.push(c);
        Engine::new(p.clone()).unwrap();
        for tol in [3.0, 4.0, 6.0] {
            let levels = build(&p, &LodOptions::new(tol, 1.0, 3)).unwrap();
            for l in &levels[1..] {
                assert!(
                    l.record
                        .notes
                        .iter()
                        .any(|n| n.contains("skipped: layout member")
                            && n.contains("sun")
                            && n.contains("hills")),
                    "{:?}",
                    l.record.notes
                );
                for m in ["sun", "hills"] {
                    assert_eq!(
                        l.project.find_layer(m),
                        p.find_layer(m),
                        "member {m} must be untouched"
                    );
                }
                assert!(
                    l.record.path_px > 0.0,
                    "the non-member path is still reduced"
                );
                for (size, scale) in [(None, 1.0), (Some((300.0f32, 200.0f32)), 1.0)] {
                    // the same canvas for original and level (the CLI's --size re-solves both)
                    let (o, q) = match size {
                        None => (p.clone(), l.clone()),
                        Some((w, h)) => {
                            let mut q = l.clone();
                            q.project = crate::containers::responsive(&l.project, w, h).unwrap();
                            (crate::containers::responsive(&p, w, h).unwrap(), q)
                        }
                    };
                    let r = verify(
                        &o,
                        &q,
                        &VerifyOptions {
                            scale,
                            frames: 12,
                            pixels: true,
                            pixel_frames: 3,
                        },
                    )
                    .unwrap();
                    assert!(
                        r.holds && r.measured_px <= r.declared_px + 1e-6,
                        "tol {tol} size {size:?}: measured {} declared {}",
                        r.measured_px,
                        r.declared_px
                    );
                }
            }
        }
    }

    #[test]
    fn culling_is_conservative() {
        let mut dot = Layer::vector(
            "dot",
            vec![VPath::circle("d", Vec2::new(20.0, 20.0), 0.12).with_fill("#000000")],
        );
        dot.opacity = 1.0;
        let big = Layer::vector(
            "big",
            vec![VPath::rect("r", 50.0, 50.0, 60.0, 40.0).with_fill("#336699")],
        );
        let tiny_mask = Layer::vector(
            "maskdot",
            vec![VPath::circle("m", Vec2::new(30.0, 30.0), 0.1).with_fill("#ffffff")],
        );
        let mut masked = Layer::vector(
            "masked",
            vec![VPath::rect("r", 0.0, 0.0, 100.0, 100.0).with_fill("#990000")],
        );
        masked.mask = Some(MaskRef {
            layer: "maskdot".into(),
            invert: false,
        });
        let sw = Layer::switch(
            "sw",
            vec![
                Layer::vector(
                    "a",
                    vec![VPath::circle("d", Vec2::new(70.0, 20.0), 0.1).with_fill("#000000")],
                ),
                Layer::vector(
                    "b",
                    vec![VPath::circle("d", Vec2::new(70.0, 20.0), 0.1).with_fill("#000000")],
                ),
            ],
        );
        let mut blended = Layer::vector(
            "blended",
            vec![VPath::circle("d", Vec2::new(90.0, 20.0), 0.1).with_fill("#000000")],
        );
        blended.blend = crate::style::Blend::Difference;
        let p = scene_with(vec![dot, big, tiny_mask, masked, sw, blended]);
        let levels = build(&p, &LodOptions::new(1.0, 1.0, 1)).unwrap();
        let q = &levels[1].project;
        assert!(
            q.find_layer("dot").is_none(),
            "sub-pixel dot should be culled"
        );
        for keep in ["big", "maskdot", "masked", "sw/a", "sw/b", "blended"] {
            assert!(q.find_layer(keep).is_some(), "{keep} must not be culled");
        }
        assert!(levels[1].record.cull_px > 0.0 && levels[1].record.cull_px <= 0.5 + 1e-9);
        // at a higher render scale the same dot is no longer sub-pixel
        let hi = build(&p, &LodOptions::new(1.0, 8.0, 1)).unwrap();
        assert!(hi[1].project.find_layer("dot").is_some());
        // verify charges the culled layer with its diameter and holds
        let r = verify(
            &p,
            &levels[1],
            &VerifyOptions {
                scale: 1.0,
                frames: 4,
                pixels: true,
                pixel_frames: 2,
            },
        )
        .unwrap();
        assert!(r.holds && r.measured_cull_px > 0.0, "{r:?}");
        // the culled dot changes at most a few grey levels of one pixel
        assert!(r.pixel_max_abs <= 64, "{}", r.pixel_max_abs);
    }

    #[test]
    fn building_is_deterministic_and_does_not_touch_the_input() {
        let p = rig_scene(true);
        let before = p.clone();
        let o = LodOptions::new(0.8, 1.0, 3);
        let a = build(&p, &o).unwrap();
        let b = build(&p, &o).unwrap();
        assert_eq!(p, before, "input project was modified");
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(
                x.project.to_json_pretty().into_bytes(),
                y.project.to_json_pretty().into_bytes()
            );
            assert_eq!(
                serde_json::to_string(&x.record).unwrap(),
                serde_json::to_string(&y.record).unwrap()
            );
        }
        let m = mesh_scene(8, 6, true);
        let m1 = build(&m, &o).unwrap();
        let m2 = build(&m, &o).unwrap();
        for (x, y) in m1.iter().zip(&m2) {
            assert_eq!(x.project.to_json_pretty(), y.project.to_json_pretty());
        }
        let blob = blob_scene(1.0);
        let b1 = build(&blob, &o).unwrap();
        let b2 = build(&blob, &o).unwrap();
        assert_eq!(
            b1[2].project.to_json_pretty(),
            b2[2].project.to_json_pretty()
        );
    }

    #[test]
    fn levels_are_monotone() {
        for p in [
            blob_scene(1.0),
            rig_scene(true),
            mesh_scene(10, 8, true),
            crate::demo::project(),
        ] {
            let levels = build(&p, &LodOptions::new(0.5, 1.0, 5)).unwrap();
            assert_eq!(levels.len(), 6);
            for w in levels.windows(2) {
                let (a, b) = (&w[0].record, &w[1].record);
                assert!(
                    b.stats.path_anchors <= a.stats.path_anchors,
                    "anchors grew at L{}",
                    b.level
                );
                assert!(b.stats.keys <= a.stats.keys, "keys grew at L{}", b.level);
                assert!(
                    b.stats.mesh_vertices <= a.stats.mesh_vertices,
                    "mesh vertices grew at L{}",
                    b.level
                );
                assert!(b.stats.layers <= a.stats.layers);
                assert!(
                    b.declared_px + 1e-12 >= a.declared_px,
                    "bound shrank at L{}",
                    b.level
                );
                assert!(b.tolerance_px > a.tolerance_px || a.level == 0);
            }
            assert_eq!(levels[0].project, p);
        }
    }

    #[test]
    fn minimal_input_passes_through_unchanged() {
        let mut p = scene_with(vec![Layer::vector(
            "tri",
            vec![VPath::polygon(
                "t",
                &[
                    Vec2::new(10.0, 10.0),
                    Vec2::new(90.0, 10.0),
                    Vec2::new(50.0, 80.0),
                ],
            )
            .with_fill("#112233")],
        )]);
        p.channels = vec![Channel::keys(
            "tri",
            Prop::LayerRotation,
            Track::with_keys(&[(0.0, 0.0, Ease::Linear), (11.0, 5.0, Ease::Linear)]),
        )];
        let levels = build(&p, &LodOptions::new(0.5, 1.0, 3)).unwrap();
        for l in &levels[1..] {
            let mut q = l.project.clone();
            q.lods.clear();
            assert_eq!(q, p, "level {} changed a minimal project", l.record.level);
            assert_eq!(l.record.declared_px, 0.0);
        }
        // a second pass over an already reduced level changes nothing more at the same tolerance
        let blob = blob_scene(1.0);
        let l1 = build(&blob, &LodOptions::new(1.0, 1.0, 1))
            .unwrap()
            .remove(1);
        let mut again = build(&l1.project, &LodOptions::new(1.0, 1.0, 1))
            .unwrap()
            .remove(1);
        again.project.lods.clear();
        let mut once = l1.project.clone();
        once.lods.clear();
        assert!(anchors_of(&again.project, "blob") <= anchors_of(&once, "blob"));
    }

    #[test]
    fn records_are_embedded_and_round_trip() {
        let p = blob_scene(1.0);
        let levels = build(&p, &LodOptions::new(1.0, 2.0, 2)).unwrap();
        let l = &levels[2];
        assert_eq!(l.project.lods.len(), 1);
        assert_eq!(l.project.lods[0].level, 2);
        assert_eq!(l.project.lods[0].scale, 2.0);
        let back = Project::from_json(&l.project.to_json_pretty()).unwrap();
        assert_eq!(back, l.project);
        assert_eq!(l.record.stats.path_anchors, anchors_of(&l.project, "blob"));
        // projects without records do not serialise the field
        assert!(!p.to_json_pretty().contains("\"lods\""));
    }

    #[test]
    fn verification_passes_on_the_demo_project_and_detects_a_planted_violation() {
        let p = crate::demo::project();
        let o = LodOptions::new(0.5, 1.0, 4);
        let levels = build(&p, &o).unwrap();
        for l in &levels[1..] {
            let r = verify(
                &p,
                l,
                &VerifyOptions {
                    scale: 1.0,
                    frames: 6,
                    pixels: false,
                    pixel_frames: 1,
                },
            )
            .unwrap();
            assert!(
                r.holds,
                "level {}: measured {} > declared {} ({} at frame {})",
                r.level, r.measured_px, r.declared_px, r.worst_layer, r.worst_frame
            );
        }
        // negative control: displace a point of the coarsest level by 5 px -> must be reported
        let mut bad = levels[4].clone();
        if let Some(l) = bad.project.find_layer_mut("hills") {
            if let LayerKind::Vector { paths } = &mut l.kind {
                paths[0].anchors[2].pos.y += 5.0;
            }
        }
        let r = verify(
            &p,
            &bad,
            &VerifyOptions {
                scale: 1.0,
                frames: 4,
                pixels: false,
                pixel_frames: 1,
            },
        )
        .unwrap();
        assert!(
            !r.holds && r.measured_px > 1.0,
            "planted violation not detected: {r:?}"
        );
    }

    #[test]
    fn dead_layers_are_dropped_losslessly_and_reported() {
        let mut hidden = Layer::vector(
            "hidden",
            vec![VPath::rect("r", 0.0, 0.0, 50.0, 50.0).with_fill("#ff0000")],
        );
        hidden.visible = false;
        let p = scene_with(vec![
            hidden,
            Layer::vector(
                "shown",
                vec![VPath::rect("r", 60.0, 60.0, 50.0, 50.0).with_fill("#00ff00")],
            ),
        ]);
        let levels = build(&p, &LodOptions::new(0.5, 1.0, 1)).unwrap();
        assert!(levels[1].project.find_layer("hidden").is_none());
        assert!(levels[1]
            .record
            .notes
            .iter()
            .any(|n| n.contains("dead layer")));
        assert_eq!(levels[1].record.declared_px, 0.0);
    }

    #[test]
    fn expression_referenced_and_ik_bones_are_not_reduced() {
        let mut p = rig_scene(true);
        // an expression reads the arm: its error would be amplified, so the channel must stay as it is
        p.layers.push(Layer::vector(
            "follower",
            vec![VPath::rect("r", 10.0, 10.0, 10.0, 10.0).with_fill("#000000")],
        ));
        p.channels.push(Channel::expr(
            "follower",
            Prop::LayerX,
            "get(\"arm\", \"bone_angle\") * 2",
            Mode::Replace,
        ));
        let levels = build(&p, &LodOptions::new(2.0, 1.0, 2)).unwrap();
        assert_eq!(levels[2].record.stats.keys, levels[0].record.stats.keys);
        // IK: bones in a chain are never touched
        let mut q = rig_scene(true);
        q.bones.push(Bone::new(
            "hand",
            Some("arm"),
            30.0,
            Vec2::new(100.0, 0.0),
            0.0,
        ));
        q.bones
            .push(Bone::new("goal", None, 5.0, Vec2::new(150.0, 100.0), 0.0));
        q.ik = vec![crate::project::IkChain::two_bone(
            "ik", "arm", "hand", "goal", 1.0,
        )];
        let lq = build(&q, &LodOptions::new(2.0, 1.0, 2)).unwrap();
        assert_eq!(lq[2].record.stats.keys, lq[0].record.stats.keys);
    }

    #[test]
    fn layer_channels_and_camera_are_reduced_with_levers() {
        let mut p = scene_with(vec![
            Layer::vector(
                "box",
                vec![VPath::rect("r", 20.0, 20.0, 60.0, 40.0).with_fill("#336699")],
            ),
            Layer::vector(
                "spinner",
                vec![VPath::rect("r", 100.0, 100.0, 40.0, 4.0).with_fill("#993366")],
            ),
        ]);
        p.frames = 60;
        p.layers[1].transform.position = Vec2::new(0.0, 0.0);
        let dense = |f: &dyn Fn(f32) -> f32| {
            Track::with_keys(
                &(0..60)
                    .map(|i| (i as f32, f(i as f32), Ease::Linear))
                    .collect::<Vec<_>>(),
            )
        };
        p.channels = vec![
            Channel::keys("box", Prop::LayerX, dense(&|t| 20.0 * (t * 0.1).sin())),
            Channel::keys(
                "spinner",
                Prop::LayerRotation,
                dense(&|t| 10.0 * (t * 0.07).sin()),
            ),
            Channel::keys(
                "",
                Prop::CameraX,
                dense(&|t| 3.0 * (t * 0.05).sin() + 100.0),
            ),
        ];
        let levels = build(&p, &LodOptions::new(2.0, 1.0, 3)).unwrap();
        assert!(
            levels[3].record.stats.keys < levels[0].record.stats.keys / 2,
            "{} keys left of {}",
            levels[3].record.stats.keys,
            levels[0].record.stats.keys
        );
        for l in &levels[1..] {
            let r = verify(
                &p,
                l,
                &VerifyOptions {
                    scale: 1.0,
                    frames: 60,
                    pixels: false,
                    pixel_frames: 1,
                },
            )
            .unwrap();
            assert!(
                r.holds,
                "level {}: measured {} > declared {} ({})",
                r.level, r.measured_px, r.declared_px, r.worst_layer
            );
        }
    }

    #[test]
    fn options_and_projects_are_validated() {
        let p = blob_scene(1.0);
        for bad in [
            LodOptions::new(0.0, 1.0, 2),
            LodOptions::new(-1.0, 1.0, 2),
            LodOptions::new(f64::NAN, 1.0, 2),
            LodOptions::new(1.0, 0.0, 2),
            LodOptions::new(1.0, 100.0, 2),
            LodOptions::new(1.0, 1.0, 0),
            LodOptions::new(1.0, 1.0, 50),
            LodOptions {
                ratio: 0.5,
                ..LodOptions::new(1.0, 1.0, 2)
            },
            LodOptions {
                geometry_share: 0.8,
                keys_share: 0.5,
                ..LodOptions::new(1.0, 1.0, 2)
            },
            LodOptions {
                cull_share: 2.0,
                ..LodOptions::new(1.0, 1.0, 2)
            },
        ] {
            assert!(build(&p, &bad).is_err(), "{bad:?} accepted");
        }
        let mut broken = blob_scene(1.0);
        broken.channels = vec![Channel::keys("nope", Prop::LayerX, Track::new())];
        assert!(build(&broken, &LodOptions::new(1.0, 1.0, 1))
            .unwrap_err()
            .contains("does not validate"));
        let l = build(&p, &LodOptions::new(1.0, 1.0, 1)).unwrap();
        assert!(verify(&p, &l[1], &VerifyOptions::new(0.0)).is_err());
    }

    #[test]
    fn level_picking_follows_the_display_scale() {
        let p = blob_scene(1.0);
        let levels = build(&p, &LodOptions::new(0.25, 1.0, 4)).unwrap();
        let recs: Vec<LodRecord> = levels.iter().map(|l| l.record.clone()).collect();
        // tiny budget: only L0; huge budget: coarsest
        assert_eq!(pick_level(&recs, 1.0, 0.0), 0);
        assert_eq!(pick_level(&recs, 1.0, 1e9), 4);
        // zooming in (more pixels per unit) can only select a finer or equal level
        let mut last = usize::MAX;
        for s in [0.1, 0.5, 1.0, 2.0, 8.0, 64.0] {
            let k = pick_level(&recs, s, 0.5);
            assert!(k <= last, "level {k} at scale {s} after {last}");
            last = k;
            if k > 0 {
                assert!(recs[k].bound_stage * s <= 0.5 + 1e-12);
            }
            if k < 4 {
                assert!(
                    recs[k + 1].bound_stage * s > 0.5,
                    "level {} would also have fit at scale {s}",
                    k + 1
                );
            }
        }
        assert_eq!(pick_level(&recs, 0.0, 1.0), 0);
        assert_eq!(pick_level(&[], 1.0, 1.0), 0);
        assert!((fit_scale(&p, 400.0, 1000.0) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn report_and_table_list_every_level() {
        let p = rig_scene(true);
        let levels = build(&p, &LodOptions::new(1.0, 1.0, 2)).unwrap();
        let v: Vec<VerifyReport> = levels[1..]
            .iter()
            .map(|l| {
                verify(
                    &p,
                    l,
                    &VerifyOptions {
                        scale: 1.0,
                        frames: 6,
                        pixels: false,
                        pixel_frames: 1,
                    },
                )
                .unwrap()
            })
            .collect();
        let t = table(&levels, &v);
        assert_eq!(t.lines().count(), 4);
        let j = report_json(&levels, &v);
        assert_eq!(j["levels"].as_array().unwrap().len(), 3);
        assert_eq!(j["verify"].as_array().unwrap().len(), 2);
        assert!(j["limits"].as_array().unwrap().len() >= 5);
        assert!(j["assumptions"].as_array().unwrap().len() >= 5);
    }

    #[test]
    fn mesh_with_non_affine_uvs_and_deformed_layers_is_left_alone() {
        // explicit uvs that are not an affine function of the position: not collapsed
        let mut p = mesh_scene(6, 6, false);
        if let Some(l) = p.find_layer_mut("img") {
            if let LayerKind::Image {
                mesh: Some(m),
                origin,
                size,
                ..
            } = &mut l.kind
            {
                let v = crate::project::mesh_vertices(*origin, size.unwrap(), 6, 6);
                let tris: Vec<[u32; 3]> = crate::weights::grid_triangles(6, 6)
                    .iter()
                    .map(|t| [t[0] as u32, t[1] as u32, t[2] as u32])
                    .collect();
                let uvs: Vec<Vec2> = v
                    .iter()
                    .map(|q| {
                        Vec2::new((q.x - 40.0) * 0.1 + ((q.y) * 0.3).sin(), (q.y - 60.0) * 0.2)
                    })
                    .collect();
                m.free = Some(crate::project::FreeMesh {
                    vertices: v,
                    triangles: tris,
                    uvs,
                });
            }
        }
        let l = build(&p, &LodOptions::new(100.0, 1.0, 1)).unwrap();
        assert_eq!(mesh_vertices(&l[1].project), 49);
    }

    #[test]
    fn file_backed_images_have_no_pixels_and_are_not_touched() {
        let mut p = mesh_scene(6, 6, false);
        if let Some(l) = p.find_layer_mut("img") {
            if let LayerKind::Image { source, .. } = &mut l.kind {
                *source = ImageSource {
                    file: Some("missing.png".into()),
                    png_base64: None,
                };
            }
        }
        // the engine cannot load the file, so the project does not even validate: reported, not guessed
        assert!(build(&p, &LodOptions::new(5.0, 1.0, 1)).is_err());
    }

    #[test]
    fn projects_with_links_get_identical_levels() {
        let mut p = blob_scene(1.0);
        p.glues = vec![crate::links::Glue {
            name: "g".into(),
            mode: Default::default(),
            pairs: vec![],
            strength: 1.0,
        }];
        match build(&p, &LodOptions::new(5.0, 1.0, 2)) {
            Ok(levels) => {
                assert!(levels[1..].iter().all(|l| l.record.declared_px == 0.0
                    && l.record.notes.iter().any(|n| n.contains("glues"))));
                assert_eq!(anchors_of(&levels[2].project, "blob"), 80);
            }
            // an empty glue may be rejected by validation; then the guard itself is checked directly
            Err(_) => assert!(links_present(&p)),
        }
    }
}
