// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
//! glTF 2.0 binary (.glb) export: the bone hierarchy with sampled animation, plus the
//! frame-0 vector shapes as triangle meshes. Only core glTF 2.0 features are used.
//!
//! What is exported, stated plainly:
//! * Bones and their animation are exact (sampled every frame, linear interpolation, shear dropped).
//! * Fills become flat triangle meshes (ear clipping). Shapes are *rigid*: a layer with a `bone`
//!   follows that bone node; other shapes stay put. Shapes whose anchors all carry bone weights are
//!   skinned meshes (JOINTS_0/WEIGHTS_0, 4 influences per vertex, weights interpolated along each
//!   segment). Layer transform animation (non-bone) and opacity animation are not exported.
//! * Strokes are triangulated into their own meshes (width, width profile, butt/round/square caps,
//!   miter/round/bevel joins with a miter limit of 4, dashes). Stroke vertices inherit the bone
//!   weights of the path point they were offset from. Translucent strokes can look darker where
//!   the generated triangles overlap (sharp inner corners, round joins), because glTF blends each
//!   triangle on its own.
//! * Gradients are exact for linear and radial paints: glTF has no gradient, so a small generated
//!   PNG (256x1 for linear, 256x256 for radial) is used as a base-colour texture with TEXCOORD_0
//!   computed from the layer-space position, so colours interpolate correctly across large
//!   triangles. Stops are interpolated in sRGB, straight alpha. Spread mode is pad (clamp).
//! * Layer masks (own or inherited from a group) are baked by clipping the triangles against the
//!   mask layer's triangulated fill, exactly, but only when the mask source is a vector layer with
//!   opaque fills, no strokes and full opacity, and only for the frame-0 mask shape (an animated
//!   mask does not move in the file). Clipped layers are exported rigid, not skinned.
//! * Image layers become a textured quad or the deformation mesh (embedded PNG, skinned when the
//!   mesh has complete weights). Masks on meshed images are not baked.
//! * Path morphs become glTF morph targets with animated weights, but only after a per-frame check
//!   that `base + sum(weight * delta)` reproduces the engine's geometry (same topology, fixed
//!   subdivision per segment) and the triangulation stays valid; otherwise the path stays a static
//!   frame-0 shape and the reason is returned in the report.
//!
//! The stage's y axis points down, so y is negated (glTF is y-up).

use crate::engine::{Frame, NodeKind};
use crate::hdr::srgb_to_linear;
use crate::math::{Affine2, Vec2};
use crate::path::{profile_at, VPath};
use crate::project::{Layer, LayerKind};
use crate::style::{Cap, Join, Paint, Rgba, StrokeStyle};
use crate::Engine;
use serde_json::{json, Value};
use std::collections::HashMap;

const MAX_ANIM_VALUES: usize = 4_000_000;
/// Most vertices of one polygon that `triangulate` accepts (more: the result is empty and the caller reports it).
pub const MAX_TRIANGULATE_POINTS: usize = 200_000;

/// Ear-clipping triangulation of a simple polygon (no holes). Returns index triples.
///
/// Linked-list ear clipping that goes on from where it stopped instead of starting over, and tests an ear only
/// against the reflex vertices (the only ones that can lie inside it): a convex polygon of n points costs O(n),
/// a general one O(n r) with r reflex vertices. (The first version rescanned all vertices for every ear, O(n^2)
/// and worse: 60000 anchors took 40 s.) Polygons above [`MAX_TRIANGULATE_POINTS`] points, or so tangled that the
/// reflex tests would exceed a fixed work budget, give an empty result.
pub fn triangulate(pts: &[Vec2]) -> Vec<[usize; 3]> {
    let n = pts.len();
    if !(3..=MAX_TRIANGULATE_POINTS).contains(&n) {
        return vec![];
    }
    let area: f32 = (0..n).map(|i| pts[i].x * pts[(i + 1) % n].y - pts[(i + 1) % n].x * pts[i].y).sum::<f32>() * 0.5;
    let order: Vec<usize> = if area >= 0.0 { (0..n).collect() } else { (0..n).rev().collect() };
    // f64: with f32 the cross product of neighbouring points of a fine polygon is rounding noise and half of its
    // convex corners would look reflex
    let cross = |a: Vec2, b: Vec2, c: Vec2| (b.x as f64 - a.x as f64) * (c.y as f64 - a.y as f64) - (b.y as f64 - a.y as f64) * (c.x as f64 - a.x as f64);
    let inside = |p: Vec2, a: Vec2, b: Vec2, c: Vec2| cross(a, b, p) >= 0.0 && cross(b, c, p) >= 0.0 && cross(c, a, p) >= 0.0;
    let at = |k: usize| pts[order[k]];
    let mut prev: Vec<usize> = (0..n).map(|k| (k + n - 1) % n).collect();
    let mut next: Vec<usize> = (0..n).map(|k| (k + 1) % n).collect();
    let convex = |k: usize, prev: &[usize], next: &[usize]| cross(at(prev[k]), at(k), at(next[k])) > 1e-12;
    let mut is_reflex: Vec<bool> = (0..n).map(|k| !convex(k, &prev, &next)).collect();
    // The reflex vertices sit in a uniform grid, so an ear is tested only against the few that are near it. (Even a
    // finely subdivided circle has many reflex corners in f32: coordinate rounding is larger than its sagitta.)
    let nr = is_reflex.iter().filter(|r| **r).count();
    let (mut lo, mut hi) = (Vec2::new(f32::MAX, f32::MAX), Vec2::new(f32::MIN, f32::MIN));
    for p in pts {
        lo = Vec2::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Vec2::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    let g = ((nr as f32).sqrt().ceil() as usize).clamp(1, 1024);
    let (w, h) = ((hi.x - lo.x).max(1e-9), (hi.y - lo.y).max(1e-9));
    let cell_of = |p: Vec2| -> (usize, usize) { ((((p.x - lo.x) / w * g as f32) as usize).min(g - 1), (((p.y - lo.y) / h * g as f32) as usize).min(g - 1)) };
    let mut cells: Vec<Vec<u32>> = vec![Vec::new(); g * g];
    // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
    #[allow(clippy::needless_range_loop)]
    for k in 0..n {
        if is_reflex[k] {
            let (cx, cy) = cell_of(at(k));
            cells[cy * g + cx].push(k as u32);
        }
    }
    let mut removed = vec![false; n];
    let mut out = Vec::with_capacity(n.saturating_sub(2));
    let (mut alive, mut cur, mut fails) = (n, 0usize, 0usize);
    let mut work = 0usize;
    while alive > 3 {
        let (a, c) = (prev[cur], next[cur]);
        let ear = !is_reflex[cur] && {
            let (pa, pb, pc) = (at(a), at(cur), at(c));
            let (x0, y0) = cell_of(Vec2::new(pa.x.min(pb.x).min(pc.x), pa.y.min(pb.y).min(pc.y)));
            let (x1, y1) = cell_of(Vec2::new(pa.x.max(pb.x).max(pc.x), pa.y.max(pb.y).max(pc.y)));
            let mut blocked = false;
            'scan: for cy in y0..=y1 {
                for cx in x0..=x1 {
                    let cell = &cells[cy * g + cx];
                    work += cell.len() + 1;
                    for &j in cell {
                        let j = j as usize;
                        if !removed[j] && is_reflex[j] && j != a && j != cur && j != c && inside(at(j), pa, pb, pc) {
                            blocked = true;
                            break 'scan;
                        }
                    }
                }
            }
            if work > 600_000_000 {
                return vec![]; // too complex to finish in reasonable time: the caller reports "cannot be triangulated"
            }
            !blocked
        };
        if ear {
            out.push([order[a], order[cur], order[c]]);
            removed[cur] = true;
            next[a] = c;
            prev[c] = a;
            alive -= 1;
            for k in [a, c] {
                if is_reflex[k] && convex(k, &prev, &next) {
                    is_reflex[k] = false;
                }
            }
            // skip the neighbour: clipping every other corner halves the polygon per pass (clipping one after the
            // other would grow a long fan whose triangles span ever more of the grid)
            cur = if alive > 3 { next[c] } else { c };
            fails = 0;
        } else {
            cur = next[cur];
            fails += 1;
            if fails > alive {
                // self-intersecting or degenerate input: fan what is left so we always terminate
                let mut rest = vec![cur];
                let mut k = next[cur];
                while k != cur {
                    rest.push(k);
                    k = next[k];
                }
                for w in 1..rest.len() - 1 {
                    out.push([order[rest[0]], order[rest[w]], order[rest[w + 1]]]);
                }
                return out;
            }
        }
    }
    if alive == 3 {
        out.push([order[prev[cur]], order[cur], order[next[cur]]]);
    }
    out
}

struct Bin {
    data: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl Bin {
    /// A bare buffer view (used for embedded images). Returns its index.
    fn view(&mut self, bytes: &[u8]) -> usize {
        while !self.data.len().is_multiple_of(4) {
            self.data.push(0);
        }
        let off = self.data.len();
        self.data.extend_from_slice(bytes);
        self.views.push(json!({"buffer": 0, "byteOffset": off, "byteLength": bytes.len()}));
        self.views.len() - 1
    }

    fn add(&mut self, bytes: &[u8], component: u32, count: usize, ty: &str, min_max: Option<(Vec<f32>, Vec<f32>)>, target: Option<u32>) -> usize {
        while !self.data.len().is_multiple_of(4) {
            self.data.push(0);
        }
        let off = self.data.len();
        self.data.extend_from_slice(bytes);
        let mut view = json!({"buffer": 0, "byteOffset": off, "byteLength": bytes.len()});
        if let Some(t) = target {
            view["target"] = json!(t);
        }
        self.views.push(view);
        let mut acc = json!({"bufferView": self.views.len() - 1, "componentType": component, "count": count, "type": ty});
        if let Some((mn, mx)) = min_max {
            acc["min"] = json!(mn);
            acc["max"] = json!(mx);
        }
        self.accessors.push(acc);
        self.accessors.len() - 1
    }
}

fn f32s(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// Local TRS (glTF, y-up) of `m` relative to `parent` (both stage space, y-down).
fn trs(m: Affine2, parent: Option<Affine2>) -> ([f32; 3], [f32; 4], [f32; 3]) {
    let l = match parent.and_then(|p| p.inverse()) {
        Some(inv) => inv.mul(m),
        None => m,
    };
    let rot = l.b.atan2(l.a);
    let sx = l.a.hypot(l.b).max(1e-6);
    let sy = (l.a * l.d - l.b * l.c) / sx;
    let half = -rot * 0.5; // y flip negates the angle
    ([l.tx, -l.ty, 0.0], [0.0, 0.0, half.sin(), half.cos()], [sx, sy, 1.0])
}

fn collect_shapes<'a>(ls: &'a [Layer], prefix: &str, out: &mut Vec<(&'a Layer, String)>) {
    for l in ls {
        let path = if prefix.is_empty() { l.name.clone() } else { format!("{prefix}/{}", l.name) };
        if matches!(l.kind, LayerKind::Vector { .. } | LayerKind::Boolean { .. } | LayerKind::Image { .. }) {
            out.push((l, path.clone()));
        }
        if let LayerKind::Group { layers } | LayerKind::Switch { layers, .. } = &l.kind {
            collect_shapes(layers, &path, out);
        }
    }
}

fn bin_ibm(bin: &mut Bin, ibm: &[f32], nb: usize) -> usize {
    bin.add(&f32s(ibm), 5126, nb, "MAT4", None, None)
}

// ---------------------------------------------------------------------------------------------
// small geometry helpers
// ---------------------------------------------------------------------------------------------

fn cr(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}

fn nrm(d: Vec2) -> Vec2 {
    Vec2::new(-d.y, d.x)
}

type Mix = ([u8; 4], [f32; 4]);

/// Top four influences, normalised. `mix` is `(bone index, weight)` with duplicates already merged.
fn pack_mix(mut mix: Vec<(usize, f32)>) -> Mix {
    mix.sort_by(|x, y| y.1.total_cmp(&x.1));
    mix.truncate(4);
    if mix.iter().map(|m| m.1).sum::<f32>() <= 1e-6 {
        mix = vec![(0, 1.0)];
    }
    let sum: f32 = mix.iter().map(|m| m.1).sum();
    let (mut j, mut w) = ([0u8; 4], [0f32; 4]);
    for k in 0..4 {
        let (bi, bw) = mix.get(k).copied().unwrap_or((0, 0.0));
        // a zero weight must point at joint 0 (glTF validators insist on it)
        j[k] = if bw > 0.0 { bi as u8 } else { 0 };
        w[k] = bw / sum;
    }
    (j, w)
}

fn add_mix(mix: &mut Vec<(usize, f32)>, bi: usize, w: f32) {
    match mix.iter_mut().find(|m| m.0 == bi) {
        Some(m) => m.1 += w,
        None => mix.push((bi, w)),
    }
}

/// Skin mix per flattened point of `wp` (the source path): weights interpolated along the segment.
fn skin_table(wp: &VPath, tp: &[(Vec2, usize, f32)], bone_ix: &HashMap<&str, usize>) -> Vec<Mix> {
    tp.iter()
        .map(|&(_, seg, t)| {
            let a0 = &wp.anchors[seg % wp.anchors.len()];
            let a1 = &wp.anchors[(seg + 1) % wp.anchors.len()];
            let mut mix = Vec::new();
            for (a, k) in [(a0, 1.0 - t), (a1, t)] {
                for bw in &a.weights {
                    if let Some(&bi) = bone_ix.get(bw.bone.as_str()) {
                        add_mix(&mut mix, bi, bw.weight * k);
                    }
                }
            }
            pack_mix(mix)
        })
        .collect()
}

/// Triangle soup built by the stroker; `src` maps each vertex to the flattened path point it
/// derives from (for skinning).
#[derive(Default)]
struct Soup {
    pos: Vec<Vec2>,
    src: Vec<usize>,
    tris: Vec<[u32; 3]>,
    note: Option<&'static str>,
}

impl Soup {
    fn vert(&mut self, p: Vec2, src: usize) -> u32 {
        self.pos.push(p);
        self.src.push(src);
        (self.pos.len() - 1) as u32
    }
    /// Adds a triangle, wound counter-clockwise in layer space (positive cross).
    fn tri(&mut self, a: u32, b: u32, c: u32) {
        let (pa, pb, pc) = (self.pos[a as usize], self.pos[b as usize], self.pos[c as usize]);
        let z = cr(pb - pa, pc - pa);
        if z.abs() < 1e-12 {
            return;
        }
        self.tris.push(if z > 0.0 { [a, b, c] } else { [a, c, b] });
    }
}

type Pt = (Vec2, f32, usize); // position, arc length from the path start, source point index

const COS_SMOOTH: f32 = 0.906; // turns below ~25 degrees are treated as smooth (mitred ribbon)
const MITER_LIMIT: f32 = 4.0;
const MAX_DASHES: f32 = 20_000.0;

fn dash_runs(pts: &[Vec2], cum: &[f32], total: f32, closed: bool, dash: &[f32], offset: f32) -> Vec<(Vec<Pt>, bool)> {
    let n = pts.len();
    let ne = if closed { n } else { n - 1 };
    let cycle: f32 = dash.iter().sum();
    let mut idx = 0usize;
    let mut rem = dash[0];
    let mut off = offset.rem_euclid(cycle);
    while off > 0.0 {
        if off >= rem {
            off -= rem;
            idx = (idx + 1) % dash.len();
            rem = dash[idx];
        } else {
            rem -= off;
            off = 0.0;
        }
    }
    let mut runs: Vec<(Vec<Pt>, bool)> = Vec::new();
    let mut cur: Option<Vec<Pt>> = None;
    let finish = |cur: &mut Option<Vec<Pt>>, runs: &mut Vec<(Vec<Pt>, bool)>| {
        if let Some(mut r) = cur.take() {
            r.dedup_by(|a, b| a.0.distance(b.0) < 1e-4);
            if r.len() >= 2 {
                runs.push((r, false));
            }
        }
    };
    for e in 0..ne {
        let (a, b) = (e, (e + 1) % n);
        let len = pts[a].distance(pts[b]);
        if len < 1e-9 {
            continue;
        }
        let at = |u: f32| -> Pt { (pts[a].lerp(pts[b], u / len), (cum[a] + u).min(total), if u < len * 0.5 { a } else { b }) };
        let mut u = 0.0f32;
        while u < len - 1e-7 {
            let step = rem.min(len - u);
            if idx.is_multiple_of(2) {
                if cur.is_none() {
                    cur = Some(vec![at(u)]);
                }
                cur.as_mut().unwrap().push(at(u + step));
            }
            u += step;
            rem -= step;
            if rem <= 1e-7 {
                if idx.is_multiple_of(2) {
                    finish(&mut cur, &mut runs);
                }
                idx = (idx + 1) % dash.len();
                rem = dash[idx];
            }
        }
    }
    finish(&mut cur, &mut runs);
    runs
}

/// Triangulate a stroke of `style` along the polyline `pts` (layer space).
fn build_stroke(pts: &[Vec2], closed: bool, style: &StrokeStyle) -> Option<Soup> {
    let n = pts.len();
    if n < 2 || (style.width.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)) || !style.width.is_finite() {
        return None;
    }
    let mut cum = vec![0.0f32; n];
    for i in 1..n {
        cum[i] = cum[i - 1] + pts[i].distance(pts[i - 1]);
    }
    let total = (cum[n - 1] + if closed { pts[n - 1].distance(pts[0]) } else { 0.0 }).max(1e-6);
    let mut soup = Soup::default();
    let dash_ok = style.dash.len() >= 2 && style.dash.len().is_multiple_of(2) && style.dash.iter().all(|d| *d > 0.0 && d.is_finite());
    let cycle: f32 = style.dash.iter().sum();
    let mut runs: Vec<(Vec<Pt>, bool)> = Vec::new();
    if dash_ok && total / cycle * (style.dash.len() / 2) as f32 <= MAX_DASHES {
        runs = dash_runs(pts, &cum, total, closed, &style.dash, style.dash_offset);
    } else {
        if dash_ok {
            soup.note = Some("a stroke with more than 20000 dashes was exported solid");
        }
        runs.push(((0..n).map(|i| (pts[i], cum[i], i)).collect(), closed));
    }
    for (q, c) in runs {
        add_run(&mut soup, q, c, style, total);
    }
    Some(soup)
}

fn add_run(out: &mut Soup, mut q: Vec<Pt>, mut wrap: bool, style: &StrokeStyle, total: f32) {
    q.dedup_by(|a, b| a.0.distance(b.0) < 1e-4);
    if wrap {
        if q.len() >= 2 && q[0].0.distance(q[q.len() - 1].0) < 1e-4 {
            q.pop();
        }
        if q.len() < 3 {
            wrap = false;
        }
    }
    if q.len() < 2 {
        return;
    }
    let dir_of = |q: &[Pt], a: usize, b: usize| (q[b].0 - q[a].0).normalized();
    if wrap {
        let m = q.len();
        let corner = |k: usize| dir_of(&q, (k + m - 1) % m, k).dot(dir_of(&q, k, (k + 1) % m)) < COS_SMOOTH;
        if let Some(k0) = (0..m).find(|&k| corner(k)) {
            q.rotate_left(k0);
        }
        q.push(q[0]);
    }
    let last = q.len() - 1;
    let dir = |a: usize, b: usize| dir_of(&q, a, b);
    let d_in = |j: usize| -> Option<Vec2> {
        if j > 0 {
            Some(dir(j - 1, j))
        } else if wrap {
            Some(dir(last - 1, last))
        } else {
            None
        }
    };
    let d_out = |j: usize| -> Option<Vec2> {
        if j < last {
            Some(dir(j, j + 1))
        } else if wrap {
            Some(dir(0, 1))
        } else {
            None
        }
    };
    let is_corner = |j: usize| matches!((d_in(j), d_out(j)), (Some(a), Some(b)) if a.dot(b) < COS_SMOOTH);
    let hw = |j: usize| 0.5 * style.width * profile_at(&style.width_profile, q[j].1 / total).max(0.0);
    let mut splits = vec![0usize];
    splits.extend((1..last).filter(|&j| is_corner(j)));
    splits.push(last);
    for w in splits.windows(2) {
        let (a, b) = (w[0], w[1]);
        let mut prev: Option<(u32, u32)> = None;
        // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
        #[allow(clippy::needless_range_loop)]
        for j in a..=b {
            let (m, k) = if is_corner(j) {
                match (j == a, d_in(j), d_out(j)) {
                    (true, _, Some(d)) => (nrm(d), 1.0),
                    (_, Some(d), _) => (nrm(d), 1.0),
                    _ => continue,
                }
            } else {
                match (d_in(j), d_out(j)) {
                    (Some(di), Some(dout)) => {
                        let mut m = (nrm(di) + nrm(dout)).normalized();
                        if m == Vec2::ZERO {
                            m = nrm(dout);
                        }
                        (m, 1.0 / m.dot(nrm(dout)).max(0.5))
                    }
                    (None, Some(d)) | (Some(d), None) => (nrm(d), 1.0),
                    _ => continue,
                }
            };
            let off = m * (hw(j) * k);
            let l = out.vert(q[j].0 + off, q[j].2);
            let r = out.vert(q[j].0 - off, q[j].2);
            if let Some((pl, pr)) = prev {
                out.tri(pl, pr, l);
                out.tri(pr, r, l);
            }
            prev = Some((l, r));
        }
    }
    // joins at the corners
    let first_join = if wrap { 0 } else { 1 };
    // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
    #[allow(clippy::needless_range_loop)]
    for j in first_join..last {
        if !is_corner(j) {
            continue;
        }
        let (Some(di), Some(dout)) = (d_in(j), d_out(j)) else { continue };
        let (v, h, src) = (q[j].0, hw(j), q[j].2);
        let sigma = if cr(di, dout) > 0.0 { -1.0 } else { 1.0 };
        let (n0, n1) = (nrm(di) * sigma, nrm(dout) * sigma);
        let vc = out.vert(v, src);
        let va = out.vert(v + n0 * h, src);
        let vb = out.vert(v + n1 * h, src);
        match style.join {
            Join::Bevel => out.tri(vc, va, vb),
            Join::Miter => {
                let denom = 1.0 + n0.dot(n1);
                if denom > 1e-6 && (2.0 / denom).sqrt() <= MITER_LIMIT {
                    let vt = out.vert(v + (n0 + n1) * (h / denom), src);
                    out.tri(vc, va, vt);
                    out.tri(vc, vt, vb);
                } else {
                    out.tri(vc, va, vb);
                }
            }
            Join::Round => {
                let a0 = n0.y.atan2(n0.x);
                let delta = cr(n0, n1).atan2(n0.dot(n1));
                let steps = ((delta.abs() / (std::f32::consts::PI / 12.0)).ceil() as usize).max(1);
                let mut prev = va;
                for s in 1..=steps {
                    let cur = if s == steps { vb } else { out.vert(v + Vec2::from_angle(a0 + delta * s as f32 / steps as f32) * h, src) };
                    out.tri(vc, prev, cur);
                    prev = cur;
                }
            }
        }
    }
    // caps
    if !wrap {
        for (j, d) in [(0usize, d_out(0).map(|d| -d)), (last, d_in(last))] {
            let Some(d) = d else { continue };
            let (v, h, src) = (q[j].0, hw(j), q[j].2);
            let n = nrm(d);
            match style.cap {
                Cap::Butt => {}
                Cap::Square => {
                    let (a, b) = (out.vert(v + n * h, src), out.vert(v - n * h, src));
                    let (c, e) = (out.vert(v + n * h + d * h, src), out.vert(v - n * h + d * h, src));
                    out.tri(a, b, c);
                    out.tri(b, e, c);
                }
                Cap::Round => {
                    let vc = out.vert(v, src);
                    let a0 = n.y.atan2(n.x);
                    let steps = 12;
                    let mut prev = out.vert(v + n * h, src);
                    for s in 1..=steps {
                        let cur = out.vert(v + Vec2::from_angle(a0 - std::f32::consts::PI * s as f32 / steps as f32) * h, src);
                        out.tri(vc, prev, cur);
                        prev = cur;
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// mask baking: exact clipping of triangles against a triangulated mask shape
// ---------------------------------------------------------------------------------------------

const CLIP_EPS: f32 = 1e-6;

fn poly_area(p: &[Vec2]) -> f32 {
    (0..p.len()).map(|i| cr(p[i], p[(i + 1) % p.len()])).sum::<f32>() * 0.5
}

/// Sutherland-Hodgman against one half plane: keep the left of a->b (or the right).
fn clip_half(poly: &[Vec2], a: Vec2, b: Vec2, keep_left: bool) -> Vec<Vec2> {
    let s = |p: Vec2| if keep_left { cr(b - a, p - a) } else { -cr(b - a, p - a) };
    let mut out = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let (c, n) = (poly[i], poly[(i + 1) % poly.len()]);
        let (sc, sn) = (s(c), s(n));
        if sc >= 0.0 {
            out.push(c);
        }
        if (sc >= 0.0) != (sn >= 0.0) {
            let t = sc / (sc - sn);
            out.push(c.lerp(n, t));
        }
    }
    out
}

fn clip_convex(poly: &[Vec2], tri: &[Vec2; 3]) -> Vec<Vec2> {
    let mut cur = poly.to_vec();
    for e in 0..3 {
        cur = clip_half(&cur, tri[e], tri[(e + 1) % 3], true);
        if cur.len() < 3 {
            return vec![];
        }
    }
    cur
}

/// `poly` minus the convex triangle `tri`, as disjoint convex pieces.
fn subtract_convex(poly: &[Vec2], tri: &[Vec2; 3]) -> Vec<Vec<Vec2>> {
    let mut pieces = Vec::new();
    let mut cur = poly.to_vec();
    for e in 0..3 {
        let outside = clip_half(&cur, tri[e], tri[(e + 1) % 3], false);
        if outside.len() >= 3 && poly_area(&outside).abs() > CLIP_EPS {
            pieces.push(outside);
        }
        cur = clip_half(&cur, tri[e], tri[(e + 1) % 3], true);
        if cur.len() < 3 {
            break;
        }
    }
    pieces
}

fn bbox(p: &[Vec2]) -> (Vec2, Vec2) {
    let mut lo = Vec2::new(f32::MAX, f32::MAX);
    let mut hi = Vec2::new(f32::MIN, f32::MIN);
    for q in p {
        lo = Vec2::new(lo.x.min(q.x), lo.y.min(q.y));
        hi = Vec2::new(hi.x.max(q.x), hi.y.max(q.y));
    }
    (lo, hi)
}

/// Clip a triangle mesh against masks (`(counter-clockwise triangles, inverted)`), exactly.
/// `None` if the result would exceed `limit` pieces.
fn clip_mesh(pos: &[Vec2], tris: &[[u32; 3]], masks: &[(Vec<[Vec2; 3]>, bool)], limit: usize) -> Option<(Vec<Vec2>, Vec<[u32; 3]>)> {
    let mut cur: Vec<Vec<Vec2>> = tris
        .iter()
        .map(|t| {
            let mut v = vec![pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]];
            if poly_area(&v) < 0.0 {
                v.reverse();
            }
            v
        })
        .filter(|v| poly_area(v).abs() > 0.0)
        .collect();
    for (mt, invert) in masks {
        let mbox: Vec<(Vec2, Vec2)> = mt.iter().map(|t| bbox(t)).collect();
        let mut next: Vec<Vec<Vec2>> = Vec::new();
        for poly in cur {
            let mut rem = vec![poly];
            let mut inter: Vec<Vec<Vec2>> = Vec::new();
            for (m, mb) in mt.iter().zip(&mbox) {
                let mut nr = Vec::new();
                for r in rem {
                    let rb = bbox(&r);
                    if rb.1.x < mb.0.x || rb.0.x > mb.1.x || rb.1.y < mb.0.y || rb.0.y > mb.1.y {
                        nr.push(r);
                        continue;
                    }
                    let i = clip_convex(&r, m);
                    if i.len() >= 3 && poly_area(&i).abs() > CLIP_EPS {
                        inter.push(i);
                    }
                    nr.extend(subtract_convex(&r, m));
                }
                rem = nr;
                if rem.is_empty() {
                    break;
                }
            }
            next.extend(if *invert { rem } else { inter });
            if next.len() > limit {
                return None;
            }
        }
        cur = next;
    }
    let (mut p, mut t) = (Vec::new(), Vec::new());
    for poly in cur {
        let base = p.len() as u32;
        p.extend_from_slice(&poly);
        for k in 1..poly.len() - 1 {
            t.push([base, base + k as u32, base + k as u32 + 1]);
        }
    }
    Some((p, t))
}

// ---------------------------------------------------------------------------------------------
// gradients as textures
// ---------------------------------------------------------------------------------------------

const GRAD_N: usize = 256;

#[derive(Clone, Copy)]
enum GradMap {
    Linear { from: Vec2, to: Vec2 },
    Radial { center: Vec2, radius: f32 },
}

fn grad_map(p: &Paint) -> Option<GradMap> {
    match p {
        Paint::Linear { from, to, stops } if stops.len() >= 2 && from.distance(*to) > 1e-6 => Some(GradMap::Linear { from: *from, to: *to }),
        Paint::Radial { center, radius, stops } if stops.len() >= 2 && *radius > 1e-3 => Some(GradMap::Radial { center: *center, radius: *radius }),
        _ => None,
    }
}

fn grad_uv(m: &GradMap, p: Vec2) -> [f32; 2] {
    let n = GRAD_N as f32;
    match m {
        GradMap::Linear { from, to } => {
            let d = *to - *from;
            let u = (p - *from).dot(d) / d.dot(d);
            [(0.5 + u * (n - 1.0)) / n, 0.5]
        }
        GradMap::Radial { center, radius } => {
            let s = (p - *center) * (1.0 / radius);
            [(0.5 + (s.x + 1.0) * 0.5 * (n - 1.0)) / n, (0.5 + (s.y + 1.0) * 0.5 * (n - 1.0)) / n]
        }
    }
}

fn color_at(stops: &[crate::style::Stop], t: f32) -> Rgba {
    let mut s: Vec<_> = stops.to_vec();
    s.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    if t <= s[0].offset {
        return s[0].color;
    }
    for w in s.windows(2) {
        if t <= w[1].offset {
            let span = w[1].offset - w[0].offset;
            let u = if span <= 1e-9 { 1.0 } else { (t - w[0].offset) / span };
            return w[0].color.lerp(w[1].color, u);
        }
    }
    s[s.len() - 1].color
}

fn encode_rgba_png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().expect("png header");
        wr.write_image_data(rgba).expect("png data");
    }
    out
}

/// Gradient lookup texture. Texel centres hold the colour at `k/(N-1)` (linear) or at the
/// matching point of the unit square (radial), so bilinear sampling reproduces the paint.
fn gradient_png(p: &Paint) -> Vec<u8> {
    let n = GRAD_N;
    match p {
        Paint::Linear { stops, .. } => {
            let mut px = Vec::with_capacity(n * 4);
            for k in 0..n {
                px.extend_from_slice(&color_at(stops, k as f32 / (n - 1) as f32).to_u8());
            }
            encode_rgba_png(n as u32, 1, &px)
        }
        Paint::Radial { stops, .. } => {
            let mut px = Vec::with_capacity(n * n * 4);
            for j in 0..n {
                for i in 0..n {
                    let (sx, sy) = (-1.0 + 2.0 * i as f32 / (n - 1) as f32, -1.0 + 2.0 * j as f32 / (n - 1) as f32);
                    px.extend_from_slice(&color_at(stops, sx.hypot(sy)).to_u8());
                }
            }
            encode_rgba_png(n as u32, n as u32, &px)
        }
        Paint::Solid(c) => encode_rgba_png(1, 1, &c.to_u8()),
    }
}

fn stops_translucent(p: &Paint) -> bool {
    match p {
        Paint::Solid(c) => c.a < 0.999,
        Paint::Linear { stops, .. } | Paint::Radial { stops, .. } => stops.iter().any(|s| s.color.a < 0.999),
    }
}

// ---------------------------------------------------------------------------------------------
// morph targets: fixed subdivision so topology never changes
// ---------------------------------------------------------------------------------------------

fn seg_pts(p: &VPath, i: usize) -> [Vec2; 4] {
    let a = &p.anchors[i];
    let b = &p.anchors[(i + 1) % p.anchors.len()];
    [a.pos, a.hout(), b.hin(), b.pos]
}

fn seg_count_for(p: &VPath, i: usize, tol: f32) -> usize {
    let [p0, c1, c2, p1] = seg_pts(p, i);
    let ctrl = p0.distance(c1) + c1.distance(c2) + c2.distance(p1);
    ((ctrl / tol.max(1e-3)).sqrt() * 0.6).ceil().clamp(1.0, 48.0) as usize
}

fn flatten_fixed(p: &VPath, ns: &[usize]) -> Vec<Vec2> {
    let mut out = Vec::new();
    let Some(first) = p.anchors.first() else { return out };
    out.push(first.pos);
    for (i, &n) in ns.iter().enumerate().take(p.segment_count()) {
        let [p0, c1, c2, p1] = seg_pts(p, i);
        for k in 1..=n {
            let t = k as f32 / n as f32;
            let m = 1.0 - t;
            out.push(p0 * (m * m * m) + c1 * (3.0 * m * m * t) + c2 * (3.0 * m * t * t) + p1 * (t * t * t));
        }
    }
    if p.closed && out.len() > 1 {
        out.pop();
    }
    out
}

struct MorphData {
    names: Vec<String>,
    deltas: Vec<Vec<Vec2>>,
    w0: Vec<f32>,
    /// frame-major: `frames * names.len()` weights
    frames: Vec<f32>,
}

type MorphCache = Option<Vec<(Frame, Vec<Vec<Vec<f32>>>)>>;

/// Try to express `src`'s morphs as glTF targets. Ok = (base points, triangles, data).
type Tri = [u32; 3];

fn try_morph(engine: &Engine, cache: &mut MorphCache, nid: usize, pi: usize, src: &VPath) -> Result<(Vec<Vec2>, Vec<Tri>, MorphData), String> {
    let nf = engine.project.frames as usize;
    if nf > 1500 || src.anchors.len() < 3 {
        return Err("too many frames (or too few anchors) to verify morph targets".into());
    }
    let morph_paths: Vec<VPath> = src.morphs.iter().map(|m| VPath { anchors: m.anchors.clone(), morphs: vec![], ..src.clone() }).collect();
    let ns: Vec<usize> = (0..src.segment_count())
        .map(|i| {
            let mut n = seg_count_for(src, i, 0.5);
            for mp in &morph_paths {
                n = n.max(seg_count_for(mp, i, 0.5));
            }
            n
        })
        .collect();
    let base = flatten_fixed(src, &ns);
    let deltas: Vec<Vec<Vec2>> = morph_paths.iter().map(|mp| flatten_fixed(mp, &ns).iter().zip(&base).map(|(a, b)| *a - *b).collect()).collect();
    let cache = cache.get_or_insert_with(|| (0..nf).map(|f| (engine.evaluate(f as f32), engine.morph_weights(f as f32))).collect());
    let nt = src.morphs.len();
    let mut frames = Vec::with_capacity(nf * nt);
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for (f, (fr, mw)) in cache.iter().enumerate() {
        let w = mw.get(nid).and_then(|x| x.get(pi)).cloned().unwrap_or_default();
        if w.len() != nt {
            return Err("morph weights unavailable".into());
        }
        let actual = flatten_fixed(&fr.nodes[nid].paths[pi], &ns);
        if actual.len() != base.len() {
            return Err(format!("topology differs at frame {f}"));
        }
        let recon: Vec<Vec2> = (0..base.len())
            .map(|v| {
                let mut p = base[v];
                for (k, d) in deltas.iter().enumerate() {
                    p += d[v] * w[k];
                }
                p
            })
            .collect();
        let err = recon.iter().zip(&actual).map(|(a, b)| a.distance(*b)).fold(0.0, f32::max);
        if err > 0.05 {
            return Err(format!("geometry at frame {f} is not base + weighted deltas (error {err:.3}); the layer is also deformed by skinning, a deformer, wobble or point offsets"));
        }
        if f == 0 {
            tris = triangulate(&actual).into_iter().map(|t| [t[0] as u32, t[1] as u32, t[2] as u32]).collect();
            if tris.is_empty() {
                return Err("fill cannot be triangulated".into());
            }
        }
        for t in &tris {
            let (a, b, c) = (recon[t[0] as usize], recon[t[1] as usize], recon[t[2] as usize]);
            if cr(b - a, c - a) < -0.1 {
                return Err(format!("the frame-0 triangulation folds over at frame {f}"));
            }
        }
        frames.extend_from_slice(&w);
    }
    if frames.iter().all(|w| w.abs() < 1e-6) {
        return Err("morph weights are zero on every frame".into());
    }
    let w0 = frames[..nt].to_vec();
    Ok((base, tris, MorphData { names: src.morphs.iter().map(|m| m.name.clone()).collect(), deltas, w0, frames }))
}

// ---------------------------------------------------------------------------------------------
// export
// ---------------------------------------------------------------------------------------------

enum PaintSrc {
    Paint(Paint),
    Image { png: Vec<u8>, key: String, has_alpha: bool },
}

enum UvSrc {
    /// computed from the paint (gradients) or absent (solid)
    Paint,
    /// affine over a rectangle in layer space
    Quad { origin: Vec2, size: Vec2 },
    Explicit(Vec<[f32; 2]>),
}

struct Prim {
    name: String,
    pos: Vec<Vec2>, // layer space
    tris: Vec<[u32; 3]>,
    skin: Option<Vec<Mix>>,
    paint: PaintSrc,
    uv: UvSrc,
    opacity: f32,
    morph: Option<MorphData>,
}

struct Out {
    bin: Bin,
    nodes: Vec<Value>,
    scene_nodes: Vec<usize>,
    meshes: Vec<Value>,
    materials: Vec<Value>,
    textures: Vec<Value>,
    images: Vec<Value>,
    tex_cache: HashMap<String, usize>,
    shape_count: usize,
    skinned_any: bool,
    morph_anims: Vec<(usize, Vec<f32>, usize)>, // node, frame-major weights, targets
}

fn bounds3(flat: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let (mut mn, mut mx) = (vec![f32::MAX; 3], vec![f32::MIN; 3]);
    for v in flat.chunks_exact(3) {
        for k in 0..3 {
            mn[k] = mn[k].min(v[k]);
            mx[k] = mx[k].max(v[k]);
        }
    }
    (mn, mx)
}

impl Out {
    fn texture(&mut self, key: String, png: impl FnOnce() -> Vec<u8>) -> usize {
        if let Some(&t) = self.tex_cache.get(&key) {
            return t;
        }
        let bytes = png();
        let bv = self.bin.view(&bytes);
        self.images.push(json!({"bufferView": bv, "mimeType": "image/png"}));
        self.textures.push(json!({"sampler": 0, "source": self.images.len() - 1}));
        self.tex_cache.insert(key, self.textures.len() - 1);
        self.textures.len() - 1
    }

    /// Write one primitive as a mesh node. `place` maps layer space to the node's space.
    fn emit(&mut self, pr: Prim, to_stage: Affine2, to_host: Affine2, host: Option<usize>) {
        let skinned = pr.skin.is_some();
        let place = if skinned { to_stage } else { to_host };
        let z = if skinned { self.shape_count as f32 * 0.01 } else { 0.0 };
        let gp = |q: Vec2| {
            let p = place.apply(q);
            [p.x, -p.y, z]
        };
        let flat: Vec<f32> = pr.pos.iter().flat_map(|q| gp(*q)).collect();
        let bounds = bounds3(&flat);
        let nv = pr.pos.len();
        let pos = self.bin.add(&f32s(&flat), 5126, nv, "VEC3", Some(bounds), Some(34962));
        let mut attrs = json!({"POSITION": pos});
        if let Some(sk) = &pr.skin {
            self.skinned_any = true;
            let joints: Vec<u8> = sk.iter().flat_map(|m| m.0).collect();
            let wts: Vec<f32> = sk.iter().flat_map(|m| m.1).collect();
            attrs["JOINTS_0"] = json!(self.bin.add(&joints, 5121, nv, "VEC4", None, Some(34962)));
            attrs["WEIGHTS_0"] = json!(self.bin.add(&f32s(&wts), 5126, nv, "VEC4", None, Some(34962)));
        }
        // material + texture coordinates
        let (mut gmap, mut tex): (Option<GradMap>, Option<usize>) = (None, None);
        let mut translucent = pr.opacity < 0.999;
        let mut factor = [1.0, 1.0, 1.0, pr.opacity];
        match &pr.paint {
            PaintSrc::Paint(p) => {
                gmap = grad_map(p);
                if gmap.is_some() {
                    tex = Some(self.texture(format!("grad:{}", serde_json::to_string(p).unwrap_or_default()), || gradient_png(p)));
                    translucent |= stops_translucent(p);
                } else {
                    let c = p.degenerate_color().unwrap_or_else(|| p.base_color());
                    factor = [srgb_to_linear(c.r), srgb_to_linear(c.g), srgb_to_linear(c.b), c.a * pr.opacity];
                    translucent = factor[3] < 0.999;
                }
            }
            PaintSrc::Image { png, key, has_alpha } => {
                tex = Some(self.texture(key.clone(), || png.clone()));
                translucent |= *has_alpha;
            }
        }
        let uv_of = |q: Vec2| -> [f32; 2] {
            match (&pr.uv, &gmap) {
                (UvSrc::Quad { origin, size }, _) => [(q.x - origin.x) / size.x, (q.y - origin.y) / size.y],
                (_, Some(m)) => grad_uv(m, q),
                _ => [0.0, 0.0],
            }
        };
        let has_uv = tex.is_some();
        if has_uv {
            let uvs: Vec<f32> = match &pr.uv {
                UvSrc::Explicit(v) => v.iter().flatten().copied().collect(),
                _ => pr.pos.iter().flat_map(|q| uv_of(*q)).collect(),
            };
            attrs["TEXCOORD_0"] = json!(self.bin.add(&f32s(&uvs), 5126, nv, "VEC2", None, Some(34962)));
        }
        let idx_bytes: Vec<u8> = pr.tris.iter().flat_map(|t| t.iter().rev()).flat_map(|&i| i.to_le_bytes()).collect(); // reverse: y flip flips winding
        let ind = self.bin.add(&idx_bytes, 5125, pr.tris.len() * 3, "SCALAR", None, Some(34963));
        let mut mat = json!({"pbrMetallicRoughness": {"baseColorFactor": factor, "metallicFactor": 0.0, "roughnessFactor": 1.0}, "doubleSided": true});
        if let Some(t) = tex {
            mat["pbrMetallicRoughness"]["baseColorTexture"] = json!({"index": t, "texCoord": 0});
        }
        if translucent {
            mat["alphaMode"] = json!("BLEND");
        }
        self.materials.push(mat);
        let mut prim = json!({"attributes": attrs, "indices": ind, "material": self.materials.len() - 1});
        let mut mesh = json!({"name": pr.name});
        let nt = pr.morph.as_ref().map_or(0, |m| m.names.len());
        if let Some(md) = &pr.morph {
            let mut targets = Vec::new();
            for d in &md.deltas {
                let dflat: Vec<f32> = pr.pos.iter().zip(d).flat_map(|(b, dd)| {
                    let (a, c) = (gp(*b), gp(*b + *dd));
                    [c[0] - a[0], c[1] - a[1], 0.0]
                }).collect();
                let mut t = json!({"POSITION": self.bin.add(&f32s(&dflat), 5126, nv, "VEC3", Some(bounds3(&dflat)), Some(34962))});
                if has_uv && gmap.is_some() {
                    let duv: Vec<f32> = pr.pos.iter().zip(d).flat_map(|(b, dd)| {
                        let (a, c) = (uv_of(*b), uv_of(*b + *dd));
                        [c[0] - a[0], c[1] - a[1]]
                    }).collect();
                    t["TEXCOORD_0"] = json!(self.bin.add(&f32s(&duv), 5126, nv, "VEC2", None, Some(34962)));
                }
                targets.push(t);
            }
            prim["targets"] = json!(targets);
            mesh["weights"] = json!(md.w0);
            mesh["extras"] = json!({"targetNames": md.names});
        }
        mesh["primitives"] = json!([prim]);
        self.meshes.push(mesh);
        let mi = self.meshes.len() - 1;
        if skinned {
            self.nodes.push(json!({"name": format!("skinned {}", pr.name), "mesh": mi, "skin": 0}));
            self.shape_count += 1;
            self.scene_nodes.push(self.nodes.len() - 1);
            return;
        }
        self.nodes.push(json!({"name": format!("shape {}", pr.name), "mesh": mi, "translation": [0.0, 0.0, self.shape_count as f32 * 0.01]}));
        self.shape_count += 1;
        let ni = self.nodes.len() - 1;
        if let Some(md) = pr.morph {
            self.morph_anims.push((ni, md.frames, nt));
        }
        match host {
            Some(h) => match self.nodes[h].get_mut("children").and_then(Value::as_array_mut) {
                Some(a) => a.push(json!(ni)),
                None => self.nodes[h]["children"] = json!([ni]),
            },
            None => self.scene_nodes.push(ni),
        }
    }
}

fn note(notes: &mut Vec<String>, s: String) {
    if !notes.contains(&s) {
        notes.push(s);
    }
}

/// Things glTF cannot carry (or this exporter does not write) that the project uses: reported so a silent loss never looks like success.
fn dropped_features(p: &crate::project::Project, notes: &mut Vec<String>) {
    use crate::project::Prop;
    fn walk(ls: &[Layer], prefix: &str, notes: &mut Vec<String>) {
        for l in ls {
            let path = if prefix.is_empty() { l.name.clone() } else { format!("{prefix}/{}", l.name) };
            if l.blend != crate::style::Blend::Normal {
                note(notes, format!("blend mode {:?} of '{path}' is not exported (glTF materials only alpha-blend); it is drawn as normal", l.blend));
            }
            if !l.effects.is_empty() {
                note(notes, format!("layer effects of '{path}' are not exported"));
            }
            if !l.deformers.is_empty() {
                note(notes, format!("deformers of '{path}' are exported only at their frame-0 shape (the deformation is not animated in the file)"));
            }
            walk(l.children(), &path, notes);
        }
    }
    walk(&p.layers, "", notes);
    for c in &p.channels {
        let (what, msg) = match c.prop {
            Prop::LayerOpacity => ("opacity", "animated opacity of '{}' is not exported (frozen at frame 0)"),
            Prop::LatticeDx | Prop::LatticeDy | Prop::SpineDx | Prop::SpineDy | Prop::PinDx | Prop::PinDy | Prop::CageDx | Prop::CageDy => ("deformer", "animated deformer '{}' is not exported (frozen at frame 0)"),
            Prop::PointDx | Prop::PointDy => ("point", "animated path point '{}' is not exported (frozen at frame 0; only morphs become morph targets)"),
            Prop::LayerX | Prop::LayerY | Prop::LayerRotation | Prop::LayerScaleX | Prop::LayerScaleY | Prop::LayerSkew | Prop::LayerSwitch | Prop::LayerWobble => ("layer", "animated layer transform/switch/wobble of '{}' is not exported (only bone animation is)"),
            Prop::FxBlur | Prop::FxDx | Prop::FxDy | Prop::FxSoft | Prop::FxAmount => ("effect", "animated layer effect '{}' is not exported"),
            Prop::GradDx | Prop::GradDy => ("gradient", "animated gradient handle '{}' is not exported (frozen at frame 0)"),
            _ => continue,
        };
        let _ = what;
        note(notes, msg.replace("{}", &c.target));
    }
}

fn dedup_flat(mut tp: Vec<(Vec2, usize, f32)>, closed_hint: bool) -> Vec<(Vec2, usize, f32)> {
    tp.dedup_by(|a, b| a.0.distance(b.0) < 1e-4);
    if tp.len() >= 2 && tp[0].0.distance(tp.last().unwrap().0) < 1e-4 && closed_hint {
        tp.pop();
    }
    tp
}

fn to_u32(t: Vec<[usize; 3]>) -> Vec<[u32; 3]> {
    t.into_iter().map(|t| [t[0] as u32, t[1] as u32, t[2] as u32]).collect()
}

/// Build the .glb bytes.
pub fn export_glb(engine: &Engine) -> Result<Vec<u8>, String> {
    export_glb_report(engine).map(|r| r.0)
}

/// Build the .glb bytes and a list of things that could not be exported exactly.
pub fn export_glb_report(engine: &Engine) -> Result<(Vec<u8>, Vec<String>), String> {
    let p = &engine.project;
    let nb = p.bones.len();
    if nb == 0 {
        return Err("the project has no bones to export".into());
    }
    if nb * p.frames as usize * 10 > MAX_ANIM_VALUES {
        return Err("animation too large for glTF export".into());
    }
    let mut notes: Vec<String> = Vec::new();
    dropped_features(p, &mut notes);
    let parent: Vec<Option<usize>> = p.bones.iter().map(|b| b.parent.as_ref().and_then(|n| p.bones.iter().position(|x| &x.name == n))).collect();
    let bone_ix: HashMap<&str, usize> = p.bones.iter().enumerate().map(|(i, b)| (b.name.as_str(), i)).collect();
    let f0 = engine.evaluate(0.0);
    let mut out = Out {
        bin: Bin { data: Vec::new(), views: vec![], accessors: vec![] },
        nodes: Vec::new(),
        scene_nodes: Vec::new(),
        meshes: Vec::new(),
        materials: Vec::new(),
        textures: Vec::new(),
        images: Vec::new(),
        tex_cache: HashMap::new(),
        shape_count: 0,
        skinned_any: false,
        morph_anims: Vec::new(),
    };
    let mut children: Vec<Vec<usize>> = vec![vec![]; nb];
    let mut roots = Vec::new();
    for (i, pa) in parent.iter().enumerate() {
        match pa {
            Some(q) => children[*q].push(i),
            None => roots.push(i),
        }
    }
    for i in 0..nb {
        let (t, r, s) = trs(f0.world[i], parent[i].map(|q| f0.world[q]));
        let mut n = json!({"name": p.bones[i].name, "translation": t, "rotation": r, "scale": s});
        if !children[i].is_empty() {
            n["children"] = json!(children[i]);
        }
        out.nodes.push(n);
    }
    out.scene_nodes = roots.clone();

    // shapes (frame 0)
    let mut shapes = Vec::new();
    collect_shapes(&p.layers, "", &mut shapes);
    let node_by_name: HashMap<&str, usize> = f0.nodes.iter().map(|n| (n.name.as_str(), n.id)).collect();
    let cam_inv = f0.camera.inverse().unwrap_or(Affine2::IDENTITY);
    let mut morph_cache: MorphCache = None;
    for (layer, lpath) in shapes {
        let Some(&nid) = node_by_name.get(lpath.as_str()) else { continue };
        let node = &f0.nodes[nid];
        if node.kind == NodeKind::Switch || node.kind == NodeKind::Group {
            continue;
        }
        // the node and all its ancestors: visibility, selected switch child, opacity, masks
        let mut chain = vec![nid];
        let mut name = lpath.as_str();
        while let Some((pre, _)) = name.rsplit_once('/') {
            if let Some(&a) = node_by_name.get(pre) {
                chain.push(a);
            }
            name = pre;
        }
        let mut drawn = true;
        let mut opacity = 1.0f32;
        let mut mask_refs: Vec<(usize, bool)> = Vec::new();
        for (k, &id) in chain.iter().enumerate() {
            let n = &f0.nodes[id];
            drawn &= n.visible;
            opacity *= n.opacity;
            if k > 0 && n.kind == NodeKind::Switch && !n.children.contains(&chain[k - 1]) {
                drawn = false;
            }
            if let Some(m) = n.mask {
                mask_refs.push(m);
            }
        }
        if !drawn {
            continue;
        }
        let to_stage = cam_inv.mul(node.world);
        let host = layer.bone.as_ref().and_then(|b| bone_ix.get(b.as_str()).copied());
        let to_host = match host.and_then(|h| f0.world[h].inverse()) {
            Some(inv) => inv.mul(to_stage),
            None => to_stage,
        };

        // masks -> triangle lists in this layer's space
        let mut masks: Vec<(Vec<[Vec2; 3]>, bool)> = Vec::new();
        for (mid, invert) in mask_refs {
            let m = &f0.nodes[mid];
            let mut mop = m.opacity;
            let mut mname = m.name.as_str();
            while let Some((pre, _)) = mname.rsplit_once('/') {
                if let Some(&a) = node_by_name.get(pre) {
                    mop *= f0.nodes[a].opacity;
                }
                mname = pre;
            }
            let opaque = |pt: &Paint| match pt {
                Paint::Solid(c) => c.a >= 0.999,
                Paint::Linear { stops, .. } | Paint::Radial { stops, .. } => stops.iter().all(|s| s.color.a >= 0.999),
            };
            let eligible = m.kind == NodeKind::Vector && mop >= 0.999 && m.paths.iter().all(|pa| pa.stroke.is_none() && pa.fill.as_ref().is_none_or(opaque));
            let Some(inv) = node.world.inverse().filter(|_| eligible) else {
                note(&mut notes, format!("mask '{}' on '{}' was not baked (mask source must be a vector layer with opaque fills, no strokes and full opacity); the layer is exported unmasked", m.name, lpath));
                continue;
            };
            let mm = inv.mul(m.world);
            let mut tris = Vec::new();
            for pa in m.paths.iter().filter(|pa| pa.fill.is_some()) {
                let pts: Vec<Vec2> = dedup_flat(pa.flatten_t(0.25), true).iter().map(|x| mm.apply(x.0)).collect();
                for t in triangulate(&pts) {
                    let mut tr = [pts[t[0]], pts[t[1]], pts[t[2]]];
                    if cr(tr[1] - tr[0], tr[2] - tr[0]) < 0.0 {
                        tr.swap(1, 2);
                    }
                    tris.push(tr);
                }
            }
            masks.push((tris, invert));
        }
        if !masks.is_empty() {
            note(&mut notes, format!("'{}' has layer masks: baked at frame 0 and exported rigid (not skinned, not morphed)", lpath));
        }

        match &layer.kind {
            LayerKind::Image { mesh: spec, .. } => {
                let Some(img) = &node.image else { continue };
                let (pw, ph) = (img.data.pixmap.width() as f32, img.data.pixmap.height() as f32);
                let (mut pos, mut tris, mut uv, mut skin) = match &img.mesh {
                    Some(m) => {
                        let uv: Vec<[f32; 2]> = m.uvs.iter().map(|u| [u.x / pw, u.y / ph]).collect();
                        let skin = spec.as_ref().filter(|s| nb <= 255 && s.weights.len() == m.positions.len() && s.weights.iter().all(|w| !w.is_empty())).map(|s| {
                            s.weights.iter().map(|ws| {
                                let mut mix = Vec::new();
                                for bw in ws {
                                    if let Some(&bi) = bone_ix.get(bw.bone.as_str()) {
                                        add_mix(&mut mix, bi, bw.weight);
                                    }
                                }
                                pack_mix(mix)
                            }).collect::<Vec<Mix>>()
                        });
                        (m.positions.clone(), m.tris.clone(), UvSrc::Explicit(uv), skin)
                    }
                    None => {
                        let (o, s) = (img.origin, img.size);
                        let pos = vec![o, o + Vec2::new(s.x, 0.0), o + s, o + Vec2::new(0.0, s.y)];
                        (pos, vec![[0, 1, 2], [0, 2, 3]], UvSrc::Quad { origin: o, size: s }, None)
                    }
                };
                if !masks.is_empty() {
                    if matches!(uv, UvSrc::Quad { .. }) {
                        match clip_mesh(&pos, &tris, &masks, 100_000) {
                            Some((p2, t2)) => {
                                pos = p2;
                                tris = t2;
                                skin = None;
                            }
                            None => note(&mut notes, format!("mask on '{lpath}' produced too many pieces; exported unmasked")),
                        }
                    } else {
                        note(&mut notes, format!("mask on meshed image '{lpath}' was not baked; exported unmasked"));
                    }
                }
                if tris.is_empty() {
                    continue;
                }
                if matches!(uv, UvSrc::Explicit(_)) {
                    if let UvSrc::Explicit(v) = &uv {
                        if v.len() != pos.len() {
                            continue;
                        }
                    }
                } else if skin.is_some() {
                    skin = None;
                }
                let key = {
                    use std::hash::{Hash, Hasher};
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    img.data.png.hash(&mut h);
                    format!("img:{}:{:x}", img.data.png.len(), h.finish())
                };
                let has_alpha = img.data.pixmap.data().chunks_exact(4).any(|px| px[3] < 255);
                if let UvSrc::Quad { .. } = uv {
                    uv = UvSrc::Quad { origin: img.origin, size: img.size };
                }
                out.emit(Prim { name: node.name.clone(), pos, tris, skin, paint: PaintSrc::Image { png: img.data.png.clone(), key, has_alpha }, uv, opacity, morph: None }, to_stage, to_host, host);
            }
            _ => {
                let src_paths: &[VPath] = match &layer.kind {
                    LayerKind::Vector { paths } => paths,
                    _ => &[],
                };
                for (pi, path) in node.paths.iter().enumerate() {
                    let wpath = src_paths.get(pi).filter(|s| s.anchors.len() == path.anchors.len());
                    let skinned_ok = masks.is_empty() && nb <= 255 && !path.anchors.is_empty() && wpath.is_some_and(|s| s.anchors.iter().all(|a| !a.weights.is_empty()));
                    let pname = if path.name.is_empty() { format!("path{pi}") } else { path.name.clone() };
                    // ---- fill
                    if let Some(fill) = &path.fill {
                        let mut tp = dedup_flat(path.flatten_t(1.0), true);
                        let mut pts: Vec<Vec2> = tp.iter().map(|x| x.0).collect();
                        let mut tris = to_u32(triangulate(&pts));
                        let mut skin: Option<Vec<Mix>> = None;
                        let mut morph = None;
                        if !tris.is_empty() {
                            if skinned_ok {
                                skin = Some(skin_table(wpath.expect("skinned implies weights"), &tp, &bone_ix));
                            } else if masks.is_empty() {
                                if let Some(sp) = wpath.filter(|s| !s.morphs.is_empty() && s.fill.is_some()) {
                                    match try_morph(engine, &mut morph_cache, nid, pi, sp) {
                                        Ok((base, t, md)) => {
                                            pts = base;
                                            tris = t;
                                            morph = Some(md);
                                        }
                                        Err(e) => note(&mut notes, format!("morphs of '{lpath}' path '{pname}' not exported as targets: {e}")),
                                    }
                                }
                            }
                            if !masks.is_empty() {
                                match clip_mesh(&pts, &tris, &masks, 200_000) {
                                    Some((p2, t2)) => {
                                        pts = p2;
                                        tris = t2;
                                    }
                                    None => note(&mut notes, format!("mask on '{lpath}' produced too many pieces; exported unmasked")),
                                }
                            }
                            tp.clear();
                            if !tris.is_empty() {
                                out.emit(Prim { name: format!("{}:{}", node.name, pname), pos: pts, tris, skin, paint: PaintSrc::Paint(fill.clone()), uv: UvSrc::Paint, opacity, morph }, to_stage, to_host, host);
                            }
                        }
                    }
                    // ---- stroke
                    if let Some(st) = &path.stroke {
                        let tp = dedup_flat(path.flatten_t(0.25), path.closed);
                        let pts: Vec<Vec2> = tp.iter().map(|x| x.0).collect();
                        let Some(mut soup) = build_stroke(&pts, path.closed, st) else { continue };
                        if let Some(n) = soup.note {
                            note(&mut notes, format!("'{lpath}': {n}"));
                        }
                        if !path.morphs.is_empty() || wpath.is_some_and(|s| !s.morphs.is_empty()) {
                            note(&mut notes, format!("stroke of morphing path '{lpath}' path '{pname}' is exported as its frame-0 shape"));
                        }
                        let mut skin: Option<Vec<Mix>> = None;
                        if skinned_ok {
                            let table = skin_table(wpath.expect("skinned implies weights"), &tp, &bone_ix);
                            skin = Some(soup.src.iter().map(|&s| table[s.min(table.len() - 1)]).collect());
                        }
                        if !masks.is_empty() {
                            match clip_mesh(&soup.pos, &soup.tris, &masks, 400_000) {
                                Some((p2, t2)) => {
                                    soup.pos = p2;
                                    soup.tris = t2;
                                }
                                None => note(&mut notes, format!("mask on stroke of '{lpath}' produced too many pieces; exported unmasked")),
                            }
                        }
                        if soup.tris.is_empty() {
                            continue;
                        }
                        out.emit(Prim { name: format!("{}:{}:stroke", node.name, pname), pos: soup.pos, tris: soup.tris, skin, paint: PaintSrc::Paint(st.paint.clone()), uv: UvSrc::Paint, opacity, morph: None }, to_stage, to_host, host);
                    }
                }
            }
        }
    }

    // animation
    let times: Vec<f32> = (0..p.frames).map(|f| f as f32 / p.fps).collect();
    let mut tr = vec![Vec::new(); nb];
    let mut ro = vec![Vec::new(); nb];
    let mut sc = vec![Vec::new(); nb];
    for f in 0..p.frames {
        let fr = engine.evaluate(f as f32);
        for i in 0..nb {
            let (t, r, s) = trs(fr.world[i], parent[i].map(|q| fr.world[q]));
            tr[i].extend_from_slice(&t);
            ro[i].extend_from_slice(&r);
            sc[i].extend_from_slice(&s);
        }
    }
    let tmax = times.last().copied().unwrap_or(0.0);
    let t_acc = out.bin.add(&f32s(&times), 5126, times.len(), "SCALAR", Some((vec![0.0], vec![tmax])), None);
    let (mut samplers, mut channels) = (Vec::new(), Vec::new());
    for i in 0..nb {
        for (path, data, ty) in [("translation", &tr[i], "VEC3"), ("rotation", &ro[i], "VEC4"), ("scale", &sc[i], "VEC3")] {
            let acc = out.bin.add(&f32s(data), 5126, times.len(), ty, None, None);
            samplers.push(json!({"input": t_acc, "output": acc, "interpolation": "LINEAR"}));
            channels.push(json!({"sampler": samplers.len() - 1, "target": {"node": i, "path": path}}));
        }
    }
    for (ni, data, nt) in std::mem::take(&mut out.morph_anims) {
        let acc = out.bin.add(&f32s(&data), 5126, times.len() * nt, "SCALAR", None, None);
        samplers.push(json!({"input": t_acc, "output": acc, "interpolation": "LINEAR"}));
        channels.push(json!({"sampler": samplers.len() - 1, "target": {"node": ni, "path": "weights"}}));
    }

    if out.skinned_any {
        // inverse bind matrices from the frame-0 pose, in glTF space (y up), column-major
        let mut ibm = Vec::with_capacity(nb * 16);
        for i in 0..nb {
            let w = f0.world[i];
            let g = |x: f32, y: f32| {
                let q = w.apply(Vec2::new(x, -y));
                Vec2::new(q.x, -q.y)
            };
            let o = g(0.0, 0.0);
            let (c0, c1) = (g(1.0, 0.0) - o, g(0.0, 1.0) - o);
            let det = c0.x * c1.y - c1.x * c0.y;
            let det = if det.abs() < 1e-9 { 1e-9 } else { det };
            let (i00, i01, i10, i11) = (c1.y / det, -c1.x / det, -c0.y / det, c0.x / det);
            let (tx, ty) = (-(i00 * o.x + i01 * o.y), -(i10 * o.x + i11 * o.y));
            ibm.extend_from_slice(&[i00, i10, 0.0, 0.0, i01, i11, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, tx, ty, 0.0, 1.0]);
        }
        bin_ibm(&mut out.bin, &ibm, nb);
    }
    let ibm_acc = out.bin.accessors.len() - 1;
    // a skin needs one common root: with several root bones, hang them under a plain node
    let mut skeleton = roots[0];
    if out.skinned_any && roots.len() > 1 {
        out.nodes.push(json!({"name": "armature", "children": roots}));
        skeleton = out.nodes.len() - 1;
        out.scene_nodes.retain(|n| !roots.contains(n));
        out.scene_nodes.insert(0, skeleton);
    }
    let mut doc = json!({
        "asset": {"version": "2.0", "generator": "osatura"},
        "scene": 0,
        "scenes": [{"nodes": out.scene_nodes}],
        "nodes": out.nodes,
        "animations": [{"name": "main", "samplers": samplers, "channels": channels}],
        "bufferViews": out.bin.views,
        "accessors": out.bin.accessors,
        "buffers": [{"byteLength": out.bin.data.len()}],
    });
    if out.skinned_any {
        doc["skins"] = json!([{"joints": (0..nb).collect::<Vec<_>>(), "inverseBindMatrices": ibm_acc, "skeleton": skeleton}]);
    }
    if !out.meshes.is_empty() {
        doc["meshes"] = json!(out.meshes);
        doc["materials"] = json!(out.materials);
    }
    if !out.textures.is_empty() {
        doc["textures"] = json!(out.textures);
        doc["images"] = json!(out.images);
        doc["samplers"] = json!([{"magFilter": 9729, "minFilter": 9729, "wrapS": 33071, "wrapT": 33071}]);
    }
    let mut json_bytes = serde_json::to_vec(&doc).map_err(|e| e.to_string())?;
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }
    let mut data = out.bin.data;
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
    let total = 12 + 8 + json_bytes.len() + 8 + data.len();
    let mut glb = Vec::with_capacity(total);
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&(total as u32).to_le_bytes());
    glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"JSON");
    glb.extend_from_slice(&json_bytes);
    glb.extend_from_slice(&(data.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"BIN\0");
    glb.extend_from_slice(&data);
    Ok((glb, notes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;

    fn parse(glb: &[u8]) -> (Value, Vec<u8>) {
        assert_eq!(&glb[..4], b"glTF");
        assert_eq!(u32::from_le_bytes(glb[4..8].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(glb[8..12].try_into().unwrap()) as usize, glb.len());
        let jl = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
        assert_eq!(&glb[16..20], b"JSON");
        let json: Value = serde_json::from_slice(&glb[20..20 + jl]).unwrap();
        let bo = 20 + jl;
        let bl = u32::from_le_bytes(glb[bo..bo + 4].try_into().unwrap()) as usize;
        assert_eq!(&glb[bo + 4..bo + 8], b"BIN\0");
        assert_eq!(bo + 8 + bl, glb.len());
        (json, glb[bo + 8..].to_vec())
    }

    #[test]
    fn triangulates_concave_and_either_winding() {
        let l_shape = [Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 1.0), Vec2::new(1.0, 1.0), Vec2::new(1.0, 4.0), Vec2::new(0.0, 4.0)];
        for pts in [l_shape.to_vec(), l_shape.iter().rev().copied().collect()] {
            let t = triangulate(&pts);
            assert_eq!(t.len(), 4);
            let area: f32 = t.iter().map(|[a, b, c]| ((pts[*b].x - pts[*a].x) * (pts[*c].y - pts[*a].y) - (pts[*b].y - pts[*a].y) * (pts[*c].x - pts[*a].x)).abs() * 0.5).sum();
            assert!((area - 7.0).abs() < 1e-4, "area {area}");
        }
        assert!(triangulate(&[Vec2::ZERO, Vec2::new(1.0, 0.0)]).is_empty());
    }

    #[test]
    fn triangulation_of_huge_polygons_is_fast_and_exact() {
        let t0 = crate::clock::Stamp::now();
        // convex: 60000 points (the old rescanning version needed about 40 s)
        let n = 60_000;
        let circle: Vec<Vec2> = (0..n).map(|i| { let a = i as f32 * std::f32::consts::TAU / n as f32; Vec2::new(300.0 * a.cos(), 300.0 * a.sin()) }).collect();
        let t = triangulate(&circle);
        assert_eq!(t.len(), n - 2);
        // a comb: many reflex vertices
        let teeth = 3000;
        let mut comb = vec![Vec2::new(0.0, 0.0)];
        for i in 0..teeth {
            let x = i as f32 * 2.0;
            comb.extend([Vec2::new(x, 10.0), Vec2::new(x + 1.0, 10.0), Vec2::new(x + 1.0, 1.0), Vec2::new(x + 2.0, 1.0)]);
        }
        comb.push(Vec2::new(teeth as f32 * 2.0, 0.0));
        comb.reverse();
        let tc = triangulate(&comb);
        assert_eq!(tc.len(), comb.len() - 2);
        let area: f32 = tc.iter().map(|[a, b, c]| ((comb[*b].x - comb[*a].x) * (comb[*c].y - comb[*a].y) - (comb[*b].y - comb[*a].y) * (comb[*c].x - comb[*a].x)).abs() * 0.5).sum();
        let want = (0..teeth).map(|_| 1.0 * 9.0 + 1.0 * 1.0).sum::<f32>() + 1.0;
        assert!(area > 0.0 && (area - want).abs() / want < 0.2, "{area} vs {want}");
        assert!(t0.elapsed().as_secs() < 20, "took {:?}", t0.elapsed());
        // beyond the cap: refused (empty), not slow
        let huge: Vec<Vec2> = (0..MAX_TRIANGULATE_POINTS + 1).map(|i| Vec2::new(i as f32, (i * i % 7) as f32)).collect();
        assert!(triangulate(&huge).is_empty());
    }

    #[test]
    fn demo_exports_consistent_glb() {
        let e = Engine::new(demo::project()).unwrap();
        let glb = export_glb(&e).unwrap();
        let (j, bin) = parse(&glb);
        assert_eq!(j["asset"]["version"], "2.0");
        let nbones = e.project.bones.len();
        assert!(j["nodes"].as_array().unwrap().len() >= nbones);
        assert_eq!(j["animations"][0]["channels"].as_array().unwrap().len(), nbones * 3);
        assert_eq!(j["buffers"][0]["byteLength"].as_u64().unwrap() as usize, bin.len());
        for v in j["bufferViews"].as_array().unwrap() {
            assert!(v["byteOffset"].as_u64().unwrap() + v["byteLength"].as_u64().unwrap() <= bin.len() as u64);
        }
        for a in j["accessors"].as_array().unwrap() {
            let comps = match a["type"].as_str().unwrap() { "SCALAR" => 1, "VEC2" => 2, "VEC3" => 3, "VEC4" => 4, "MAT4" => 16, t => panic!("{t}") };
            let bytes = match a["componentType"].as_u64().unwrap() { 5121 => 1, 5126 | 5125 => 4, c => panic!("{c}") };
            let size = a["count"].as_u64().unwrap() * comps * bytes;
            assert!(size <= j["bufferViews"][a["bufferView"].as_u64().unwrap() as usize]["byteLength"].as_u64().unwrap());
        }
        assert!(j["meshes"].as_array().is_some_and(|m| !m.is_empty()), "demo has filled shapes");
        // the demo's skinned limbs come out as skinned meshes
        let skin = &j["skins"][0];
        assert_eq!(skin["joints"].as_array().unwrap().len(), nbones);
        let skinned: Vec<&Value> = j["nodes"].as_array().unwrap().iter().filter(|n| n.get("skin").is_some()).collect();
        assert!(!skinned.is_empty());
        let mesh = &j["meshes"][skinned[0]["mesh"].as_u64().unwrap() as usize];
        let at = &mesh["primitives"][0]["attributes"];
        assert!(at.get("JOINTS_0").is_some() && at.get("WEIGHTS_0").is_some());
        // weights of every vertex sum to 1 and joints are valid bone indices
        let wacc = &j["accessors"][at["WEIGHTS_0"].as_u64().unwrap() as usize];
        let wv = &j["bufferViews"][wacc["bufferView"].as_u64().unwrap() as usize];
        let off = wv["byteOffset"].as_u64().unwrap() as usize;
        for v in 0..wacc["count"].as_u64().unwrap() as usize {
            let s: f32 = (0..4).map(|k| f32::from_le_bytes(bin[off + (v * 4 + k) * 4..off + (v * 4 + k) * 4 + 4].try_into().unwrap())).sum();
            assert!((s - 1.0).abs() < 1e-4, "vertex {v} weights sum {s}");
        }
        let jacc = &j["accessors"][at["JOINTS_0"].as_u64().unwrap() as usize];
        let jo = j["bufferViews"][jacc["bufferView"].as_u64().unwrap() as usize]["byteOffset"].as_u64().unwrap() as usize;
        assert!(bin[jo..jo + jacc["count"].as_u64().unwrap() as usize * 4].iter().all(|&b| (b as usize) < nbones));
    }

    #[test]
    fn rotation_sign_and_errors() {
        let ([_, ty, _], r, _) = {
            let m = Affine2 { a: 0.0, b: 1.0, c: -1.0, d: 0.0, tx: 5.0, ty: 7.0 }; // +90 deg in y-down space
            trs(m, None)
        };
        assert_eq!(ty, -7.0);
        assert!(r[2] < 0.0, "clockwise on screen = negative about +z in y-up");
        let mut p = demo::project();
        p.bones.clear();
        p.ik.clear();
        p.channels.clear();
        p.springs.clear();
        p.constraints.clear();
        p.layers.iter_mut().for_each(|l| l.bone = None);
        if let Ok(e) = Engine::new(p) {
            assert!(export_glb(&e).is_err());
        }
    }

    // ---- helpers for the feature tests ----------------------------------------------------

    use crate::anim::{Ease, Track};
    use crate::path::{Anchor, BoneWeight};
    use crate::project::{Bone, Channel, MaskRef, MeshSpec, Project, Prop};
    use crate::style::{Paint, StrokeStyle};

    fn engine_with(layers: Vec<Layer>, frames: u32, channels: Vec<Channel>) -> Engine {
        let mut p = Project::new("t", 200.0, 200.0, 10.0, frames);
        p.bones.push(Bone::new("root", None, 10.0, Vec2::ZERO, 0.0));
        p.layers = layers;
        p.channels = channels;
        Engine::new(p).unwrap()
    }

    fn acc_bytes<'a>(j: &Value, bin: &'a [u8], i: usize) -> (&'a [u8], usize) {
        let a = &j["accessors"][i];
        let v = &j["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
        let off = v["byteOffset"].as_u64().unwrap() as usize + a["byteOffset"].as_u64().unwrap_or(0) as usize;
        let count = a["count"].as_u64().unwrap() as usize;
        let comps = match a["type"].as_str().unwrap() { "SCALAR" => 1, "VEC2" => 2, "VEC3" => 3, "VEC4" => 4, _ => 16 };
        let bytes = if a["componentType"] == 5121 { 1 } else { 4 };
        (&bin[off..off + count * comps * bytes], comps)
    }

    fn acc_f(j: &Value, bin: &[u8], i: usize) -> Vec<f32> {
        acc_bytes(j, bin, i).0.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect()
    }

    fn acc_u(j: &Value, bin: &[u8], i: usize) -> Vec<u32> {
        acc_bytes(j, bin, i).0.chunks_exact(4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).collect()
    }

    fn mesh_named<'a>(j: &'a Value, suffix: &str) -> &'a Value {
        j["meshes"].as_array().unwrap().iter().find(|m| m["name"].as_str().unwrap().ends_with(suffix)).unwrap_or_else(|| panic!("no mesh ending '{suffix}'"))
    }

    fn mesh_xy(j: &Value, bin: &[u8], m: &Value) -> (Vec<[f32; 2]>, Vec<u32>) {
        let at = &m["primitives"][0]["attributes"];
        let pos = acc_f(j, bin, at["POSITION"].as_u64().unwrap() as usize);
        let idx = acc_u(j, bin, m["primitives"][0]["indices"].as_u64().unwrap() as usize);
        (pos.chunks_exact(3).map(|p| [p[0], p[1]]).collect(), idx)
    }

    fn area(j: &Value, bin: &[u8], m: &Value) -> f32 {
        let (p, idx) = mesh_xy(j, bin, m);
        idx.chunks_exact(3)
            .map(|t| {
                let (a, b, c) = (p[t[0] as usize], p[t[1] as usize], p[t[2] as usize]);
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() * 0.5
            })
            .sum()
    }

    fn line(name: &str, pts: &[(f32, f32)], st: StrokeStyle) -> Layer {
        let v: Vec<Vec2> = pts.iter().map(|&(x, y)| Vec2::new(x, y)).collect();
        let mut path = VPath::polygon(name, &v);
        path.closed = false;
        path.stroke = Some(st);
        Layer::vector(name, vec![path])
    }

    fn export(layers: Vec<Layer>) -> (Value, Vec<u8>, Vec<String>) {
        let e = engine_with(layers, 2, vec![]);
        let (glb, notes) = export_glb_report(&e).unwrap();
        let (j, bin) = parse(&glb);
        (j, bin, notes)
    }

    fn style(w: f32, cap: Cap, join: Join) -> StrokeStyle {
        let mut s = StrokeStyle::solid("#ff0000", w);
        s.cap = cap;
        s.join = join;
        s
    }

    // ---- strokes ---------------------------------------------------------------------------

    #[test]
    fn stroke_caps_have_the_right_area() {
        for (cap, expect, tol) in [(Cap::Butt, 1000.0, 0.5), (Cap::Square, 1100.0, 0.5), (Cap::Round, 1000.0 + std::f32::consts::PI * 25.0, 1.5)] {
            let (j, bin, _) = export(vec![line("l", &[(10.0, 50.0), (110.0, 50.0)], style(10.0, cap, Join::Round))]);
            let a = area(&j, &bin, mesh_named(&j, ":stroke"));
            assert!((a - expect).abs() < tol, "{cap:?}: area {a}, expected {expect}");
        }
    }

    #[test]
    fn stroke_joins_miter_bevel_round() {
        // an L: two 50 x 10 ribbons (butt caps) plus the join piece; the inner overlap is counted twice
        let pts = [(10.0, 10.0), (60.0, 10.0), (60.0, 60.0)];
        let want = [(Join::Miter, 1025.0), (Join::Bevel, 1012.5), (Join::Round, 1000.0 + std::f32::consts::PI * 25.0 / 4.0)];
        for (join, expect) in want {
            let (j, bin, _) = export(vec![line("l", &pts, style(10.0, Cap::Butt, join))]);
            let a = area(&j, &bin, mesh_named(&j, ":stroke"));
            assert!((a - expect).abs() < 1.0, "{join:?}: area {a}, expected {expect}");
        }
        // a very sharp turn exceeds the miter limit and falls back to a bevel (no long spike)
        let (j, bin, _) = export(vec![line("l", &[(10.0, 50.0), (110.0, 50.0), (10.0, 60.0)], style(4.0, Cap::Butt, Join::Miter))]);
        let (p, _) = mesh_xy(&j, &bin, mesh_named(&j, ":stroke"));
        assert!(p.iter().all(|v| v[0] < 116.0), "no miter spike: {:?}", p.iter().map(|v| v[0]).fold(0.0, f32::max));
    }

    #[test]
    fn stroke_dashes_and_width_profile() {
        let mut st = style(4.0, Cap::Butt, Join::Round);
        st.dash = vec![10.0, 10.0];
        let (j, bin, _) = export(vec![line("l", &[(10.0, 50.0), (110.0, 50.0)], st.clone())]);
        let a = area(&j, &bin, mesh_named(&j, ":stroke"));
        assert!((a - 5.0 * 10.0 * 4.0).abs() < 0.5, "five dashes: {a}");
        st.dash_offset = 5.0; // starts half way through a dash: 5 + 4*10 + 5 = 50 of 100
        let (j, bin, _) = export(vec![line("l", &[(10.0, 50.0), (110.0, 50.0)], st)]);
        assert!((area(&j, &bin, mesh_named(&j, ":stroke")) - 200.0).abs() < 0.5);
        // taper to nothing: triangle-like area = length * width / 2
        let mut st = style(10.0, Cap::Butt, Join::Round);
        st.width_profile = vec![[0.0, 1.0], [1.0, 0.0]];
        let (j, bin, _) = export(vec![line("l", &[(10.0, 50.0), (60.0, 50.0), (110.0, 50.0)], st)]);
        assert!((area(&j, &bin, mesh_named(&j, ":stroke")) - 500.0).abs() < 2.0);
    }

    #[test]
    fn stroker_rejects_bad_input() {
        let st = StrokeStyle::solid("#000", 4.0);
        assert!(build_stroke(&[Vec2::ZERO], false, &st).is_none());
        let mut zero = st.clone();
        zero.width = 0.0;
        assert!(build_stroke(&[Vec2::ZERO, Vec2::new(5.0, 0.0)], false, &zero).is_none());
        zero.width = f32::NAN;
        assert!(build_stroke(&[Vec2::ZERO, Vec2::new(5.0, 0.0)], false, &zero).is_none());
        // closed ring: every triangle has positive area, no NaN
        let ring = VPath::circle("c", Vec2::new(50.0, 50.0), 30.0).polyline(0.25).pts;
        let s = build_stroke(&ring, true, &st).unwrap();
        assert!(s.pos.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
        let a: f32 = s.tris.iter().map(|t| cr(s.pos[t[1] as usize] - s.pos[t[0] as usize], s.pos[t[2] as usize] - s.pos[t[0] as usize]) * 0.5).sum();
        assert!((a - std::f32::consts::TAU * 30.0 * 4.0).abs() < 4.0, "ring area {a}");
    }

    #[test]
    fn translucent_and_inherited_opacity_become_blend() {
        let mut g = Layer::group("g", vec![Layer::vector("a", vec![VPath::rect("r", 0.0, 0.0, 10.0, 10.0).with_fill("#00ff00")])]);
        g.opacity = 0.5;
        let (j, _, _) = export(vec![g, line("l", &[(10.0, 50.0), (110.0, 50.0)], StrokeStyle::solid("#ff000080", 4.0))]);
        let mats = j["materials"].as_array().unwrap();
        assert_eq!(mats.len(), 2);
        assert!(mats.iter().all(|m| m["alphaMode"] == "BLEND"));
        let a0 = mats[0]["pbrMetallicRoughness"]["baseColorFactor"][3].as_f64().unwrap();
        assert!((a0 - 0.5).abs() < 1e-4, "group opacity multiplied in: {a0}");
    }

    #[test]
    fn dropped_features_are_reported() {
        let mut a = Layer::vector("a", vec![VPath::rect("r", 0.0, 0.0, 10.0, 10.0).with_fill("#00ff00")]);
        a.blend = crate::style::Blend::Multiply;
        let ch = vec![Channel::keys("a", Prop::LayerOpacity, Track::with_keys(&[(0.0, 1.0, Ease::Linear), (1.0, 0.0, Ease::Linear)])),
            Channel::keys("a::r::0", Prop::PointDx, Track::with_keys(&[(0.0, 0.0, Ease::Linear), (1.0, 3.0, Ease::Linear)]))];
        let e = engine_with(vec![a], 2, ch);
        let (_, notes) = export_glb_report(&e).unwrap();
        let all = notes.join("\n");
        assert!(all.contains("blend mode") && all.contains("animated opacity of 'a'") && all.contains("animated path point"), "{all}");
        let (_, quiet) = export_glb_report(&engine_with(vec![Layer::vector("b", vec![VPath::rect("r", 0.0, 0.0, 10.0, 10.0).with_fill("#00ff00")])], 2, vec![])).unwrap();
        assert!(quiet.is_empty(), "{quiet:?}");
    }

    // ---- gradients -------------------------------------------------------------------------

    fn decode_png(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
        let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
        dec.set_transformations(png::Transformations::IDENTITY);
        let mut r = dec.read_info().unwrap();
        let mut buf = vec![0; r.output_buffer_size().unwrap()];
        let info = r.next_frame(&mut buf).unwrap();
        (info.width, info.height, buf)
    }

    fn texture_png(j: &Value, bin: &[u8], tex: usize) -> (u32, u32, Vec<u8>) {
        let img = &j["images"][j["textures"][tex]["source"].as_u64().unwrap() as usize];
        let v = &j["bufferViews"][img["bufferView"].as_u64().unwrap() as usize];
        let off = v["byteOffset"].as_u64().unwrap() as usize;
        decode_png(&bin[off..off + v["byteLength"].as_u64().unwrap() as usize])
    }

    fn sample_tex((w, h, px): &(u32, u32, Vec<u8>), uv: [f32; 2]) -> [f32; 4] {
        let (w, h) = (*w as usize, *h as usize);
        let x = (uv[0] * w as f32 - 0.5).clamp(0.0, (w - 1) as f32);
        let y = (uv[1] * h as f32 - 0.5).clamp(0.0, (h - 1) as f32);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let at = |xx: usize, yy: usize, c: usize| px[(yy * w + xx) * 4 + c] as f32 / 255.0;
        let mut o = [0.0; 4];
        // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
        #[allow(clippy::needless_range_loop)]
        for c in 0..4 {
            o[c] = (at(x0, y0, c) * (1.0 - fx) + at(x1, y0, c) * fx) * (1.0 - fy) + (at(x0, y1, c) * (1.0 - fx) + at(x1, y1, c) * fx) * fy;
        }
        o
    }

    fn paint_color(p: &Paint, q: Vec2) -> [f32; 4] {
        let c = match p {
            Paint::Linear { from, to, stops } => {
                let d = *to - *from;
                color_at(stops, ((q - *from).dot(d) / d.dot(d)).clamp(0.0, 1.0))
            }
            Paint::Radial { center, radius, stops } => color_at(stops, (q.distance(*center) / radius).clamp(0.0, 1.0)),
            Paint::Solid(c) => *c,
        };
        [c.r, c.g, c.b, c.a]
    }

    #[test]
    fn linear_and_radial_gradients_match_the_paint_on_every_vertex() {
        let lin = Paint::linear(Vec2::new(20.0, 0.0), Vec2::new(120.0, 0.0), &[(0.0, "#ff0000"), (0.4, "#00ff00"), (1.0, "#0000ff80")]);
        let rad = Paint::radial(Vec2::new(60.0, 100.0), 40.0, &[(0.0, "#ffffff"), (1.0, "#000000")]);
        let mut a = Layer::vector("a", vec![VPath::rect("r", 10.0, 10.0, 120.0, 60.0).with_paint(lin.clone())]);
        // a transformed layer: the gradient lives in layer space, positions are placed through the transform
        a.transform.position = Vec2::new(5.0, 7.0);
        a.transform.scale = Vec2::new(1.5, 0.5);
        let b = Layer::vector("b", vec![VPath::circle("c", Vec2::new(60.0, 100.0), 50.0).with_paint(rad.clone())]);
        let (j, bin, _) = export(vec![a, b]);
        assert_eq!(j["textures"].as_array().unwrap().len(), 2);
        for (suffix, paint, tr) in [("a:r", &lin, Some((Vec2::new(5.0, 7.0), Vec2::new(1.5, 0.5)))), ("b:c", &rad, None)] {
            let m = mesh_named(&j, suffix);
            let at = &m["primitives"][0]["attributes"];
            let (pos, _) = mesh_xy(&j, &bin, m);
            let uv = acc_f(&j, &bin, at["TEXCOORD_0"].as_u64().unwrap() as usize);
            let mat = &j["materials"][m["primitives"][0]["material"].as_u64().unwrap() as usize];
            let tex = texture_png(&j, &bin, mat["pbrMetallicRoughness"]["baseColorTexture"]["index"].as_u64().unwrap() as usize);
            let mut worst = 0.0f32;
            for (v, p) in pos.iter().enumerate() {
                // back to layer space (stage y was negated; the layer transform was position + scale)
                let mut q = Vec2::new(p[0], -p[1]);
                if let Some((t, s)) = tr {
                    q = Vec2::new((q.x - t.x) / s.x, (q.y - t.y) / s.y);
                }
                let got = sample_tex(&tex, [uv[v * 2], uv[v * 2 + 1]]);
                let want = paint_color(paint, q);
                for c in 0..4 {
                    worst = worst.max((got[c] - want[c]).abs());
                }
            }
            assert!(worst < 0.012, "{suffix}: worst channel error {worst}");
        }
    }

    #[test]
    fn degenerate_gradients_fall_back_to_a_flat_colour() {
        let p = Paint::linear(Vec2::new(5.0, 5.0), Vec2::new(5.0, 5.0), &[(0.0, "#ff0000"), (1.0, "#0000ff")]);
        assert!(grad_map(&p).is_none());
        let (j, _, _) = export(vec![Layer::vector("a", vec![VPath::rect("r", 0.0, 0.0, 10.0, 10.0).with_paint(p)])]);
        assert!(j.get("textures").is_none());
        // a zero-length gradient paints its last stop everywhere (as in SVG and the rasteriser)
        let f = &j["materials"][0]["pbrMetallicRoughness"]["baseColorFactor"];
        assert!(f[0].as_f64().unwrap().abs() < 1e-4 && (f[2].as_f64().unwrap() - 1.0).abs() < 1e-4, "{f}");
    }

    // ---- masks -----------------------------------------------------------------------------

    fn masked(mask: Layer, invert: bool, shape: Layer) -> (Value, Vec<u8>, Vec<String>) {
        let mut shape = shape;
        shape.mask = Some(MaskRef { layer: mask.name.clone(), invert });
        export(vec![mask, shape])
    }

    #[test]
    fn masks_are_clipped_exactly() {
        let mk = || {
            let mut hole = Layer::vector("hole", vec![VPath::rect("r", 20.0, 20.0, 40.0, 40.0).with_fill("#ffffff")]);
            hole.visible = false;
            hole
        };
        let shape = || Layer::vector("cut", vec![VPath::rect("r", 0.0, 0.0, 100.0, 100.0).with_fill("#ff0000")]);
        let (j, bin, notes) = masked(mk(), false, shape());
        assert!((area(&j, &bin, mesh_named(&j, "cut:r")) - 1600.0).abs() < 0.1);
        assert!(notes.iter().any(|n| n.contains("baked")));
        let (j, bin, _) = masked(mk(), true, shape());
        assert!((area(&j, &bin, mesh_named(&j, "cut:r")) - 8400.0).abs() < 0.1);
        // a half-covering mask on a stroke
        let s = line("cut", &[(0.0, 50.0), (100.0, 50.0)], style(10.0, Cap::Butt, Join::Round));
        let (j, bin, _) = masked(mk(), false, s);
        assert!((area(&j, &bin, mesh_named(&j, ":stroke")) - 400.0).abs() < 0.1, "stroke clipped to the 40 x 10 overlap");
        // a circle mask
        let mut c = Layer::vector("hole", vec![VPath::circle("c", Vec2::new(50.0, 50.0), 30.0).with_fill("#fff")]);
        c.visible = false;
        let (j, bin, _) = masked(c, false, shape());
        let a = area(&j, &bin, mesh_named(&j, "cut:r"));
        assert!((a - std::f32::consts::PI * 900.0).abs() < 25.0, "circle area {a}");
    }

    #[test]
    fn masks_that_cannot_be_baked_are_reported_not_faked() {
        let mut m = Layer::vector("hole", vec![VPath::rect("r", 20.0, 20.0, 40.0, 40.0).with_fill("#ffffff").with_stroke("#000000", 2.0)]);
        m.visible = false;
        let (j, bin, notes) = masked(m, false, Layer::vector("cut", vec![VPath::rect("r", 0.0, 0.0, 100.0, 100.0).with_fill("#ff0000")]));
        assert!((area(&j, &bin, mesh_named(&j, "cut:r")) - 10000.0).abs() < 0.1, "exported unmasked");
        assert!(notes.iter().any(|n| n.contains("not baked")), "{notes:?}");
        // the piece limit is honoured
        let pos = [Vec2::ZERO, Vec2::new(10.0, 0.0), Vec2::new(0.0, 10.0)];
        let many: Vec<[Vec2; 3]> = (0..50).map(|i| { let o = i as f32 * 0.1; [Vec2::new(o, o), Vec2::new(o + 5.0, o), Vec2::new(o, o + 5.0)] }).collect();
        assert!(clip_mesh(&pos, &[[0, 1, 2]], &[(many, true)], 3).is_none());
    }

    // ---- images ----------------------------------------------------------------------------

    fn tiny_png() -> Vec<u8> {
        let mut px = Vec::new();
        for i in 0..16u8 {
            px.extend_from_slice(&[i * 16, 255 - i * 16, 40, if i == 0 { 0 } else { 255 }]);
        }
        encode_rgba_png(4, 4, &px)
    }

    #[test]
    fn image_layers_become_textured_quads() {
        let png = tiny_png();
        let img = Layer::image_from_png("pic", &png, Vec2::new(30.0, 40.0));
        let (j, bin, _) = export(vec![img]);
        let m = &j["meshes"][0];
        let at = &m["primitives"][0]["attributes"];
        let (pos, idx) = mesh_xy(&j, &bin, m);
        assert_eq!((pos.len(), idx.len()), (4, 6));
        assert!((area(&j, &bin, m) - 16.0).abs() < 1e-3, "4 x 4 px image");
        let uv = acc_f(&j, &bin, at["TEXCOORD_0"].as_u64().unwrap() as usize);
        assert_eq!(uv, vec![0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0]);
        // the embedded bytes are the layer's PNG, untouched
        let v = &j["bufferViews"][j["images"][0]["bufferView"].as_u64().unwrap() as usize];
        let off = v["byteOffset"].as_u64().unwrap() as usize;
        assert_eq!(&bin[off..off + v["byteLength"].as_u64().unwrap() as usize], &png[..]);
        assert_eq!(j["images"][0]["mimeType"], "image/png");
        let mat = &j["materials"][0];
        assert_eq!(mat["alphaMode"], "BLEND", "the image has a transparent pixel");
        assert_eq!(mat["pbrMetallicRoughness"]["baseColorTexture"]["index"], 0);
    }

    #[test]
    fn masked_image_quads_keep_affine_uvs_and_meshed_images_are_skinned() {
        let png = tiny_png();
        let mut hole = Layer::vector("hole", vec![VPath::rect("r", 32.0, 40.0, 1.0, 2.0).with_fill("#fff")]);
        hole.visible = false;
        let (j, bin, _) = masked(hole, false, Layer::image_from_png("pic", &png, Vec2::new(30.0, 40.0)));
        let m = mesh_named(&j, "pic");
        let (pos, _) = mesh_xy(&j, &bin, m);
        let uv = acc_f(&j, &bin, m["primitives"][0]["attributes"]["TEXCOORD_0"].as_u64().unwrap() as usize);
        assert!((area(&j, &bin, m) - 2.0).abs() < 1e-3);
        for (v, p) in pos.iter().enumerate() {
            assert!((uv[v * 2] - (p[0] - 30.0) / 4.0).abs() < 1e-5 && (uv[v * 2 + 1] - (-p[1] - 40.0) / 4.0).abs() < 1e-5);
        }
        // meshed image with complete weights -> JOINTS_0/WEIGHTS_0
        let mut l = Layer::image_from_png("pic", &png, Vec2::new(0.0, 0.0));
        if let LayerKind::Image { mesh, size, .. } = &mut l.kind {
            *size = Some(Vec2::new(40.0, 40.0));
            *mesh = Some(MeshSpec { cols: 2, rows: 2, weights: vec![vec![BoneWeight { bone: "root".into(), weight: 1.0 }]; 9], ..Default::default() });
        }
        let (j, _, _) = export(vec![l]);
        let at = &j["meshes"][0]["primitives"][0]["attributes"];
        assert!(at.get("JOINTS_0").is_some() && at.get("WEIGHTS_0").is_some() && at.get("TEXCOORD_0").is_some());
        assert_eq!(j["nodes"].as_array().unwrap().iter().filter(|n| n.get("skin").is_some()).count(), 1);
    }

    // ---- skinned strokes -------------------------------------------------------------------

    #[test]
    fn strokes_of_weighted_paths_are_skinned() {
        let w = |a: Anchor| Anchor { weights: vec![BoneWeight { bone: "root".into(), weight: 1.0 }], ..a };
        let mut path = VPath::new("p", false, vec![w(Anchor::corner(Vec2::new(0.0, 0.0))), w(Anchor::corner(Vec2::new(50.0, 10.0))), w(Anchor::corner(Vec2::new(90.0, 0.0)))]);
        path.stroke = Some(StrokeStyle::solid("#00f", 6.0));
        let (j, bin, _) = export(vec![Layer::vector("s", vec![path])]);
        let m = mesh_named(&j, ":stroke");
        let at = &m["primitives"][0]["attributes"];
        let n = j["accessors"][at["POSITION"].as_u64().unwrap() as usize]["count"].as_u64().unwrap();
        assert_eq!(j["accessors"][at["WEIGHTS_0"].as_u64().unwrap() as usize]["count"].as_u64().unwrap(), n);
        let wts = acc_f(&j, &bin, at["WEIGHTS_0"].as_u64().unwrap() as usize);
        assert!(wts.chunks_exact(4).all(|c| (c.iter().sum::<f32>() - 1.0).abs() < 1e-4));
    }

    // ---- morph targets ---------------------------------------------------------------------

    fn morph_rect() -> VPath {
        VPath::rect("p", 0.0, 0.0, 10.0, 10.0).with_morph("wide", |a| {
            let mut b = a.clone();
            b.pos.x *= 3.0;
            b
        })
    }

    fn morph_keys() -> Vec<Channel> {
        vec![Channel::keys("s::p::wide", Prop::PathMorph, Track::with_keys(&[(0.0, 0.0, Ease::Linear), (10.0, 1.0, Ease::Linear)]))]
    }

    #[test]
    fn path_morphs_become_targets_with_animated_weights() {
        let mut path = morph_rect();
        path.fill = Some(Paint::linear(Vec2::new(0.0, 0.0), Vec2::new(30.0, 0.0), &[(0.0, "#f00"), (1.0, "#00f")]));
        let e = engine_with(vec![Layer::vector("s", vec![path])], 11, morph_keys());
        let (glb, notes) = export_glb_report(&e).unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        let (j, bin) = parse(&glb);
        let m = &j["meshes"][0];
        let prim = &m["primitives"][0];
        assert_eq!(prim["targets"].as_array().unwrap().len(), 1);
        assert_eq!(m["extras"]["targetNames"][0], "wide");
        assert_eq!(m["weights"][0].as_f64().unwrap(), 0.0);
        // animation channel on the mesh node: linear 0..1 over 11 frames
        let ch = j["animations"][0]["channels"].as_array().unwrap().iter().find(|c| c["target"]["path"] == "weights").unwrap();
        let out = j["animations"][0]["samplers"][ch["sampler"].as_u64().unwrap() as usize]["output"].as_u64().unwrap() as usize;
        let w = acc_f(&j, &bin, out);
        assert_eq!(w.len(), 11);
        assert!(w.iter().enumerate().all(|(i, v)| (v - i as f32 / 10.0).abs() < 1e-5));
        assert!(j["nodes"][ch["target"]["node"].as_u64().unwrap() as usize].get("mesh").is_some());
        // base + w * delta spans x in [0, 10 + 20 w]
        let base = acc_f(&j, &bin, prim["attributes"]["POSITION"].as_u64().unwrap() as usize);
        let delta = acc_f(&j, &bin, prim["targets"][0]["POSITION"].as_u64().unwrap() as usize);
        for (wt, want) in [(0.0f32, 10.0f32), (0.5, 20.0), (1.0, 30.0)] {
            let mx = (0..base.len() / 3).map(|v| base[v * 3] + wt * delta[v * 3]).fold(f32::MIN, f32::max);
            assert!((mx - want).abs() < 1e-3, "w={wt}: {mx}");
        }
        // the gradient texture coordinate morphs with the vertex
        let uv0 = acc_f(&j, &bin, prim["attributes"]["TEXCOORD_0"].as_u64().unwrap() as usize);
        let duv = acc_f(&j, &bin, prim["targets"][0]["TEXCOORD_0"].as_u64().unwrap() as usize);
        let vmax = (0..base.len() / 3).max_by(|&a, &b| (base[a * 3] + delta[a * 3]).total_cmp(&(base[b * 3] + delta[b * 3]))).unwrap();
        let u1 = uv0[vmax * 2] + duv[vmax * 2];
        let LayerKind::Vector { paths } = &e.project.layers[0].kind else { panic!() };
        let map = grad_map(paths[0].fill.as_ref().unwrap()).unwrap();
        assert!((u1 - grad_uv(&map, Vec2::new(30.0, 0.0))[0]).abs() < 1e-4 || (u1 - grad_uv(&map, Vec2::new(30.0, 10.0))[0]).abs() < 1e-4);
    }

    #[test]
    fn morphs_that_are_also_deformed_stay_static_with_a_reason() {
        let path = morph_rect().with_fill("#ff0000");
        let mut ch = morph_keys();
        ch.push(Channel::keys("s::p::0", Prop::PointDy, Track::with_keys(&[(0.0, 0.0, Ease::Linear), (10.0, 5.0, Ease::Linear)])));
        let e = engine_with(vec![Layer::vector("s", vec![path])], 11, ch);
        let (glb, notes) = export_glb_report(&e).unwrap();
        let (j, _) = parse(&glb);
        assert!(j["meshes"][0]["primitives"][0].get("targets").is_none());
        assert!(notes.iter().any(|n| n.contains("not exported as targets")), "{notes:?}");
        // morph never driven: static, with a reason
        let e = engine_with(vec![Layer::vector("s", vec![morph_rect().with_fill("#ff0000")])], 3, vec![]);
        let (_, notes) = export_glb_report(&e).unwrap();
        assert!(notes.iter().any(|n| n.contains("zero on every frame")), "{notes:?}");
    }

    #[test]
    fn skin_has_a_common_root_and_zero_weights_use_joint_zero() {
        let e = Engine::new(demo::project()).unwrap();
        let (j, bin) = parse(&export_glb(&e).unwrap());
        let skel = j["skins"][0]["skeleton"].as_u64().unwrap() as usize;
        let mut seen = vec![skel];
        let mut k = 0;
        while k < seen.len() {
            if let Some(c) = j["nodes"][seen[k]]["children"].as_array() {
                seen.extend(c.iter().map(|x| x.as_u64().unwrap() as usize));
            }
            k += 1;
        }
        for joint in j["skins"][0]["joints"].as_array().unwrap() {
            assert!(seen.contains(&(joint.as_u64().unwrap() as usize)), "joint {joint} not under the skeleton root");
        }
        assert!(!j["skins"][0]["joints"].as_array().unwrap().iter().any(|x| x.as_u64().unwrap() as usize == skel), "skeleton is not a joint");
        for m in j["meshes"].as_array().unwrap() {
            let at = &m["primitives"][0]["attributes"];
            if at.get("JOINTS_0").is_none() {
                continue;
            }
            let w = acc_f(&j, &bin, at["WEIGHTS_0"].as_u64().unwrap() as usize);
            let (jb, _) = acc_bytes(&j, &bin, at["JOINTS_0"].as_u64().unwrap() as usize);
            assert!(w.iter().zip(jb).all(|(w, j)| *w > 0.0 || *j == 0));
        }
    }
}
