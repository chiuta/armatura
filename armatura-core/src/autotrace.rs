// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
//! Auto-trace: bitmap (PNG / JPEG) to vector layers, entirely local and deterministic.
//!
//! Classical, textbook methods only; there is no learned model, no pretrained data and no network:
//!
//! * colour quantisation: weighted k-means (Lloyd iterations, k-means++ seeding from a seeded
//!   xorshift generator) or median cut, over a 5-bit-per-channel histogram; images that already
//!   have at most `colors` distinct colours keep them exactly;
//! * clean-up of the label map: 3x3 majority filter and removal of connected components below
//!   `min_area` pixels (merged into the neighbour they share the most border with);
//! * 4-connected component labelling and border following on the pixel cracks. This is the
//!   border-following idea of Suzuki and Abe (1985), "Topological structural analysis of digitized
//!   binary images by border following", done on cracks between pixels instead of on pixel centres,
//!   so outer borders (positive signed area) and hole borders (negative) fall out of the orientation
//!   and the polygon area is exactly the pixel area;
//! * corner detection by the turning angle over a +-k sample window, smoothing of the open runs between
//!   corners, then least-squares cubic Bezier fitting with Newton-Raphson re-parameterisation and
//!   recursive splitting at the worst point after Schneider (1990), "An Algorithm for Automatically
//!   Fitting Digitized Curves" (Graphics Gems);
//! * optional polygon output with Douglas-Peucker (1973) or Visvalingam-Whyatt (1993) simplification;
//! * line-art mode: luminance threshold (Otsu 1979 when none is given), Zhang-Suen (1984) thinning,
//!   skeleton graph walk with spur pruning, stroke width from a 3-4 chamfer distance transform.
//!
//! Holes: a region whose hole is transparent in the source gets that hole as a cut-out (even-odd fill
//! in the SVG, a Boolean difference layer in a project). A hole that is filled by other traced regions
//! is *not* cut; the smaller region is stacked on top (areas sort the z-order), which avoids hairline
//! seams between neighbours.
//!
//! Honest limits: output quality is that of a quantise-and-contour tracer. Photographs and soft
//! gradients become flat colour bands, thin anti-aliased lines can be lost to the despeckle step, and
//! the transparent/stacked hole rule is a heuristic (majority of the hole border).

use crate::math::Vec2;
use crate::path::{Anchor, VPath};
use crate::project::{Layer, Project};
use crate::style::{Cap, Join, Paint, Rgba, StrokeStyle};
use std::collections::{BTreeMap, HashSet};

// ---------------------------------------------------------------------------------------------
// options and result types
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Fill,
    Line,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quant {
    KMeans,
    MedianCut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZOrder {
    /// Large outlines at the back (correct for nested regions). Default.
    Area,
    /// Light colours at the back, dark in front (can put a nested region under its parent).
    Luminance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    Bezier,
    /// Douglas-Peucker simplified polygon.
    Polygon,
    /// Visvalingam-Whyatt simplified polygon (`tol` is the smallest triangle area kept, in px squared, times 2).
    PolygonVw,
}

#[derive(Clone, Debug)]
pub struct TraceOptions {
    pub mode: Mode,
    pub colors: usize,
    pub quant: Quant,
    pub seed: u64,
    /// Smallest region kept, in pixels (despeckle).
    pub min_area: f32,
    /// Curve fitting tolerance in pixels.
    pub tol: f32,
    /// Smoothing passes of the [1 2 1] filter on the outline (0 = none).
    pub smooth: usize,
    /// Turning angle in degrees over the corner window above which a point is a corner.
    pub corner_angle: f32,
    /// Corner window in samples.
    pub corner_window: usize,
    /// Majority filter passes on the label map before tracing (removes one-pixel slivers).
    pub denoise: usize,
    pub zorder: ZOrder,
    pub fit: Fit,
    /// Line mode: luminance threshold 0..255 (None = Otsu). Ink is darker than the threshold.
    pub threshold: Option<u8>,
    /// Line mode: ink is lighter than the background.
    pub invert: bool,
    /// Line mode: skeleton branches shorter than this many pixels that end in a free tip are removed.
    pub prune: usize,
}

impl Default for TraceOptions {
    fn default() -> Self {
        TraceOptions {
            mode: Mode::Fill,
            colors: 8,
            quant: Quant::KMeans,
            seed: 1,
            min_area: 8.0,
            tol: 1.0,
            smooth: 3,
            corner_angle: 55.0,
            corner_window: 5,
            denoise: 0,
            zorder: ZOrder::Area,
            fit: Fit::Bezier,
            threshold: None,
            invert: false,
            prune: 4,
        }
    }
}

impl TraceOptions {
    pub fn validate(&self) -> Result<(), String> {
        if self.colors < 2 || self.colors > 64 {
            return Err("colors must be 2..64".into());
        }
        if !self.min_area.is_finite() || self.min_area < 0.0 {
            return Err("min area must be >= 0".into());
        }
        if !self.tol.is_finite() || self.tol <= 0.0 || self.tol > 50.0 {
            return Err("tolerance must be in (0, 50]".into());
        }
        if self.smooth > 50 || self.denoise > 5 || self.prune > 1000 {
            return Err("smooth <= 50, denoise <= 5, prune <= 1000".into());
        }
        if !self.corner_angle.is_finite() || self.corner_angle <= 5.0 || self.corner_angle >= 175.0 {
            return Err("corner angle must be in (5, 175) degrees".into());
        }
        if self.corner_window < 2 || self.corner_window > 40 {
            return Err("corner window must be 2..40".into());
        }
        Ok(())
    }
}

/// Decoded bitmap, straight (non-premultiplied) RGBA.
#[derive(Clone, Debug)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn from_rgba(w: usize, h: usize, rgba: Vec<u8>) -> Result<Image, String> {
        if w == 0 || h == 0 || rgba.len() != w * h * 4 {
            return Err("image size does not match its pixel data".into());
        }
        if w * h > 16_000_000 {
            return Err("image is larger than 16 million pixels".into());
        }
        Ok(Image { w, h, rgba })
    }
}

/// One traced area: an outline and the holes that must be cut out (transparent holes only).
#[derive(Clone, Debug)]
pub struct Region {
    pub color: [u8; 3],
    pub outer: VPath,
    pub holes: Vec<VPath>,
    /// Pixels of the region itself.
    pub pixels: usize,
    /// Area inside the outline in px squared (holes not subtracted); the z-order key.
    pub outer_area: f32,
}

/// One centre line of the line-art mode.
#[derive(Clone, Debug)]
pub struct Stroke {
    pub path: VPath,
    pub width: f32,
}

#[derive(Clone, Debug)]
pub struct TraceResult {
    pub width: usize,
    pub height: usize,
    pub mode: Mode,
    pub palette: Vec<[u8; 3]>,
    /// Back to front.
    pub regions: Vec<Region>,
    pub strokes: Vec<Stroke>,
    pub stroke_color: [u8; 3],
}

impl TraceResult {
    pub fn path_count(&self) -> usize {
        self.regions.iter().map(|r| 1 + r.holes.len()).sum::<usize>() + self.strokes.len()
    }
    pub fn anchor_count(&self) -> usize {
        self.regions.iter().map(|r| r.outer.anchors.len() + r.holes.iter().map(|h| h.anchors.len()).sum::<usize>()).sum::<usize>()
            + self.strokes.iter().map(|s| s.path.anchors.len()).sum::<usize>()
    }
}

// ---------------------------------------------------------------------------------------------
// decoding
// ---------------------------------------------------------------------------------------------

/// Decode PNG or baseline/progressive JPEG (8-bit grey or RGB).
pub fn decode_image(bytes: &[u8]) -> Result<Image, String> {
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        let mut d = jpeg_decoder::Decoder::new(bytes);
        d.read_info().map_err(|e| format!("invalid JPEG: {e}"))?;
        let info = d.info().ok_or("invalid JPEG")?;
        crate::assets::check_dims(info.width as u32, info.height as u32)?;
        let (w, h) = (info.width as usize, info.height as usize);
        let px = d.decode().map_err(|e| format!("invalid JPEG: {e}"))?;
        let rgba: Vec<u8> = match info.pixel_format {
            jpeg_decoder::PixelFormat::L8 => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
            jpeg_decoder::PixelFormat::RGB24 => px.chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
            _ => return Err("only 8-bit grey and RGB JPEGs are supported".into()),
        };
        return Image::from_rgba(w, h, rgba);
    }
    let pm = crate::assets::decode_png_checked(bytes).map_err(|e| format!("not a readable PNG or JPEG: {e}"))?;
    let mut rgba = Vec::with_capacity(pm.pixels().len() * 4);
    for px in pm.pixels() {
        let c = px.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Image::from_rgba(pm.width() as usize, pm.height() as usize, rgba)
}

// ---------------------------------------------------------------------------------------------
// small deterministic helpers
// ---------------------------------------------------------------------------------------------

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03 | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn lum(c: [u8; 3]) -> f64 {
    0.2126 * c[0] as f64 + 0.7152 * c[1] as f64 + 0.0722 * c[2] as f64
}

// ---------------------------------------------------------------------------------------------
// colour quantisation
// ---------------------------------------------------------------------------------------------

const TRANSP: u16 = u16::MAX;

type V3 = [f64; 3];

fn d2(a: &V3, b: &V3) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

fn kmeans(pts: &[(V3, f64)], k: usize, seed: u64) -> Vec<V3> {
    let k = k.min(pts.len());
    let mut rng = Rng::new(seed);
    let total: f64 = pts.iter().map(|p| p.1).sum();
    // weighted k-means++ seeding
    let mut pick = rng.unit() * total;
    let mut first = 0;
    for (i, p) in pts.iter().enumerate() {
        pick -= p.1;
        if pick <= 0.0 {
            first = i;
            break;
        }
    }
    let mut c: Vec<V3> = vec![pts[first].0];
    let mut dist: Vec<f64> = pts.iter().map(|p| d2(&p.0, &c[0])).collect();
    while c.len() < k {
        let s: f64 = pts.iter().zip(&dist).map(|(p, d)| p.1 * d).sum();
        if s <= 0.0 {
            break;
        }
        let mut pick = rng.unit() * s;
        let mut chosen = pts.len() - 1;
        for (i, (p, d)) in pts.iter().zip(&dist).enumerate() {
            pick -= p.1 * d;
            if pick <= 0.0 && *d > 0.0 {
                chosen = i;
                break;
            }
        }
        c.push(pts[chosen].0);
        for (i, p) in pts.iter().enumerate() {
            dist[i] = dist[i].min(d2(&p.0, c.last().unwrap()));
        }
    }
    // Lloyd iterations
    let mut assign = vec![usize::MAX; pts.len()];
    for _ in 0..40 {
        let mut changed = false;
        for (i, p) in pts.iter().enumerate() {
            let mut best = (f64::MAX, 0);
            for (j, cj) in c.iter().enumerate() {
                let d = d2(&p.0, cj);
                if d < best.0 {
                    best = (d, j);
                }
            }
            if assign[i] != best.1 {
                assign[i] = best.1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
        let mut sum = vec![[0.0f64; 3]; c.len()];
        let mut wt = vec![0.0f64; c.len()];
        for (i, p) in pts.iter().enumerate() {
            let j = assign[i];
            for (s, &x) in sum[j].iter_mut().zip(&p.0) {
                *s += x * p.1;
            }
            wt[j] += p.1;
        }
        for j in 0..c.len() {
            if wt[j] > 0.0 {
                c[j] = [sum[j][0] / wt[j], sum[j][1] / wt[j], sum[j][2] / wt[j]];
            }
        }
    }
    c
}

fn median_cut(pts: &[(V3, f64)], k: usize) -> Vec<V3> {
    let mut boxes: Vec<Vec<usize>> = vec![(0..pts.len()).collect()];
    let range = |b: &Vec<usize>| -> (usize, f64) {
        let mut best = (0, 0.0);
        for ch in 0..3 {
            let lo = b.iter().map(|&i| pts[i].0[ch]).fold(f64::MAX, f64::min);
            let hi = b.iter().map(|&i| pts[i].0[ch]).fold(f64::MIN, f64::max);
            if hi - lo > best.1 {
                best = (ch, hi - lo);
            }
        }
        best
    };
    while boxes.len() < k {
        let mut pick: Option<(usize, f64)> = None;
        for (bi, b) in boxes.iter().enumerate() {
            if b.len() < 2 {
                continue;
            }
            let r = range(b).1;
            if r > 0.0 && pick.is_none_or(|p| r > p.1) {
                pick = Some((bi, r));
            }
        }
        let Some((bi, _)) = pick else { break };
        let mut b = boxes.swap_remove(bi);
        let ch = range(&b).0;
        b.sort_by(|&x, &y| pts[x].0[ch].total_cmp(&pts[y].0[ch]).then(x.cmp(&y)));
        let total: f64 = b.iter().map(|&i| pts[i].1).sum();
        let mut acc = 0.0;
        let mut cut = 1;
        for (n, &i) in b.iter().enumerate() {
            acc += pts[i].1;
            if acc >= total / 2.0 {
                cut = (n + 1).clamp(1, b.len() - 1);
                break;
            }
        }
        let right = b.split_off(cut);
        boxes.push(b);
        boxes.push(right);
    }
    boxes
        .iter()
        .map(|b| {
            let w: f64 = b.iter().map(|&i| pts[i].1).sum();
            let mut m = [0.0; 3];
            for &i in b {
                for (mc, &x) in m.iter_mut().zip(&pts[i].0) {
                    *mc += x * pts[i].1;
                }
            }
            [m[0] / w, m[1] / w, m[2] / w]
        })
        .collect()
}

/// Palette (sorted dark to light) and per-pixel labels (`TRANSP` for alpha < 128).
pub fn quantise(img: &Image, k: usize, method: Quant, seed: u64) -> (Vec<[u8; 3]>, Vec<u16>) {
    let n = img.w * img.h;
    let opaque = |i: usize| img.rgba[i * 4 + 3] >= 128;
    let col = |i: usize| [img.rgba[i * 4], img.rgba[i * 4 + 1], img.rgba[i * 4 + 2]];
    let mut exact: BTreeMap<[u8; 3], ()> = BTreeMap::new();
    for i in 0..n {
        if opaque(i) {
            exact.insert(col(i), ());
            if exact.len() > k {
                break;
            }
        }
    }
    let mut labels = vec![TRANSP; n];
    if exact.len() <= k {
        let mut pal: Vec<[u8; 3]> = exact.keys().copied().collect();
        pal.sort_by(|a, b| lum(*a).total_cmp(&lum(*b)).then(a.cmp(b)));
        let idx: BTreeMap<[u8; 3], u16> = pal.iter().enumerate().map(|(i, c)| (*c, i as u16)).collect();
        for i in 0..n {
            if opaque(i) {
                labels[i] = idx[&col(i)];
            }
        }
        return (pal, labels);
    }
    // 5-bit histogram
    let bin = |c: [u8; 3]| ((c[0] as usize >> 3) << 10) | ((c[1] as usize >> 3) << 5) | (c[2] as usize >> 3);
    let mut cnt = vec![0.0f64; 32768];
    let mut sum = vec![[0.0f64; 3]; 32768];
    for i in 0..n {
        if opaque(i) {
            let c = col(i);
            let b = bin(c);
            cnt[b] += 1.0;
            for ch in 0..3 {
                sum[b][ch] += c[ch] as f64;
            }
        }
    }
    let bins: Vec<usize> = (0..32768).filter(|&b| cnt[b] > 0.0).collect();
    let pts: Vec<(V3, f64)> = bins.iter().map(|&b| ([sum[b][0] / cnt[b], sum[b][1] / cnt[b], sum[b][2] / cnt[b]], cnt[b])).collect();
    let centres = match method {
        Quant::KMeans => kmeans(&pts, k, seed),
        Quant::MedianCut => median_cut(&pts, k),
    };
    let mut bin_label = vec![0usize; 32768];
    let mut psum = vec![[0.0f64; 3]; centres.len()];
    let mut pwt = vec![0.0f64; centres.len()];
    for (pi, &b) in bins.iter().enumerate() {
        let mut best = (f64::MAX, 0);
        for (j, c) in centres.iter().enumerate() {
            let d = d2(&pts[pi].0, c);
            if d < best.0 {
                best = (d, j);
            }
        }
        bin_label[b] = best.1;
        for ch in 0..3 {
            psum[best.1][ch] += sum[b][ch];
        }
        pwt[best.1] += cnt[b];
    }
    // drop empty clusters, sort dark to light
    let mut pal: Vec<(usize, [u8; 3])> = (0..centres.len())
        .filter(|&j| pwt[j] > 0.0)
        .map(|j| (j, [(psum[j][0] / pwt[j]).round() as u8, (psum[j][1] / pwt[j]).round() as u8, (psum[j][2] / pwt[j]).round() as u8]))
        .collect();
    pal.sort_by(|a, b| lum(a.1).total_cmp(&lum(b.1)).then(a.1.cmp(&b.1)));
    let mut remap = vec![0u16; centres.len()];
    for (new, (old, _)) in pal.iter().enumerate() {
        remap[*old] = new as u16;
    }
    for i in 0..n {
        if opaque(i) {
            labels[i] = remap[bin_label[bin(col(i))]];
        }
    }
    (pal.iter().map(|p| p.1).collect(), labels)
}

// ---------------------------------------------------------------------------------------------
// label map clean-up and components
// ---------------------------------------------------------------------------------------------

/// 3x3 majority filter: a pixel changes only when another label has strictly more votes than its own.
pub fn majority_filter(lab: &mut [u16], w: usize, h: usize, passes: usize) {
    for _ in 0..passes {
        let src = lab.to_vec();
        for y in 0..h {
            for x in 0..w {
                let own = src[y * w + x];
                let mut votes: Vec<(u16, u8)> = Vec::with_capacity(9);
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                            continue;
                        }
                        let l = src[ny as usize * w + nx as usize];
                        match votes.iter_mut().find(|v| v.0 == l) {
                            Some(v) => v.1 += 1,
                            None => votes.push((l, 1)),
                        }
                    }
                }
                let own_n = votes.iter().find(|v| v.0 == own).map_or(0, |v| v.1);
                let mut best = (own_n, own);
                for &(l, n) in &votes {
                    if n > best.0 || (n == best.0 && n > own_n && l < best.1) {
                        best = (n, l);
                    }
                }
                lab[y * w + x] = best.1;
            }
        }
    }
}

/// 4-connected components of equal labels. Returns `(component id per pixel, pixel counts)`.
pub fn components4(lab: &[u16], w: usize, h: usize) -> (Vec<u32>, Vec<usize>) {
    let mut comp = vec![u32::MAX; w * h];
    let mut sizes = Vec::new();
    let mut stack = Vec::new();
    for s in 0..w * h {
        if comp[s] != u32::MAX {
            continue;
        }
        let id = sizes.len() as u32;
        let l = lab[s];
        comp[s] = id;
        stack.push(s);
        let mut n = 0;
        while let Some(p) = stack.pop() {
            n += 1;
            let (x, y) = (p % w, p / w);
            let mut visit = |q: usize| {
                if comp[q] == u32::MAX && lab[q] == l {
                    comp[q] = id;
                    stack.push(q);
                }
            };
            if x > 0 {
                visit(p - 1);
            }
            if x + 1 < w {
                visit(p + 1);
            }
            if y > 0 {
                visit(p - w);
            }
            if y + 1 < h {
                visit(p + w);
            }
        }
        sizes.push(n);
    }
    (comp, sizes)
}

/// Merge every component smaller than `min_area` pixels into the neighbouring label it shares the most
/// border with (several rounds, smallest first).
pub fn despeckle(lab: &mut [u16], w: usize, h: usize, min_area: usize) {
    if min_area <= 1 {
        return;
    }
    for _ in 0..8 {
        let (comp, sizes) = components4(lab, w, h);
        let mut small: Vec<usize> = (0..sizes.len()).filter(|&c| sizes[c] < min_area).collect();
        if small.is_empty() {
            return;
        }
        small.sort_by_key(|&c| (sizes[c], c));
        let mut pixels: Vec<Vec<usize>> = vec![Vec::new(); sizes.len()];
        for (p, &c) in comp.iter().enumerate() {
            if sizes[c as usize] < min_area {
                pixels[c as usize].push(p);
            }
        }
        let mut changed = false;
        for c in small {
            let own = lab[pixels[c][0]];
            let mut tally: BTreeMap<u16, usize> = BTreeMap::new();
            for &p in &pixels[c] {
                let (x, y) = (p % w, p / w);
                let mut look = |q: usize| {
                    if comp[q] != c as u32 || lab[q] != own {
                        // neighbour outside this component (its current label may have been merged this round)
                        if comp[q] != c as u32 {
                            *tally.entry(lab[q]).or_insert(0) += 1;
                        }
                    }
                };
                if x > 0 {
                    look(p - 1);
                }
                if x + 1 < w {
                    look(p + 1);
                }
                if y > 0 {
                    look(p - w);
                }
                if y + 1 < h {
                    look(p + w);
                }
            }
            tally.remove(&own);
            if let Some((&best, _)) = tally.iter().max_by_key(|(l, n)| (**n, std::cmp::Reverse(**l))) {
                for &p in &pixels[c] {
                    lab[p] = best;
                }
                changed = true;
            }
        }
        if !changed {
            return;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// border following on pixel cracks
// ---------------------------------------------------------------------------------------------

/// Closed loop of integer corner coordinates (x right, y down). Outer borders have positive signed
/// area (clockwise on screen), hole borders negative.
#[derive(Clone, Debug)]
pub struct Loop {
    pub pts: Vec<(i32, i32)>,
}

fn signed_area_i(p: &[(i32, i32)]) -> i64 {
    let mut s = 0i64;
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        s += a.0 as i64 * b.1 as i64 - b.0 as i64 * a.1 as i64;
    }
    s
}

const DX: [i32; 4] = [1, 0, -1, 0];
const DY: [i32; 4] = [0, 1, 0, -1];

/// All border loops of the pixels where `inside(x, y)` is true inside the window `[0,w) x [0,h)`.
/// Foreground stays on the right of the walking direction; at pinch points the walk turns toward the
/// foreground, so foreground is 4-connected and background 8-connected.
pub fn border_loops(w: usize, h: usize, inside: &dyn Fn(usize, usize) -> bool) -> Vec<Loop> {
    let vw = w + 1;
    let mut edge = vec![0u8; vw * (h + 1)];
    let fg = |x: i32, y: i32| x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && inside(x as usize, y as usize);
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            if !fg(x, y) {
                continue;
            }
            if !fg(x, y - 1) {
                edge[y as usize * vw + x as usize] |= 1; // right, from (x,y)
            }
            if !fg(x + 1, y) {
                edge[y as usize * vw + x as usize + 1] |= 2; // down, from (x+1,y)
            }
            if !fg(x, y + 1) {
                edge[(y as usize + 1) * vw + x as usize + 1] |= 4; // left, from (x+1,y+1)
            }
            if !fg(x - 1, y) {
                edge[(y as usize + 1) * vw + x as usize] |= 8; // up, from (x,y+1)
            }
        }
    }
    let mut loops = Vec::new();
    for sy in 0..=h {
        for sx in 0..=w {
            for sd in 0..4 {
                if edge[sy * vw + sx] & (1 << sd) == 0 {
                    continue;
                }
                let (mut x, mut y, mut d) = (sx as i32, sy as i32, sd);
                let mut pts = Vec::new();
                let mut prev_d = usize::MAX;
                loop {
                    edge[y as usize * vw + x as usize] &= !(1 << d);
                    if d != prev_d {
                        pts.push((x, y));
                    }
                    prev_d = d;
                    x += DX[d];
                    y += DY[d];
                    let mut here = edge[y as usize * vw + x as usize];
                    if x == sx as i32 && y == sy as i32 {
                        here |= 1 << sd;
                    }
                    let mut next = None;
                    for nd in [(d + 1) % 4, d, (d + 3) % 4] {
                        if here & (1 << nd) != 0 {
                            next = Some(nd);
                            break;
                        }
                    }
                    match next {
                        Some(nd) => d = nd,
                        None => break,
                    }
                    if x == sx as i32 && y == sy as i32 && d == sd {
                        break;
                    }
                }
                // the first point may be a straight continuation of the closing edge
                if pts.len() >= 2 && prev_d == sd && pts.len() > 2 {
                    // closing edge has the same direction as the first: first point is collinear
                    pts.remove(0);
                }
                if pts.len() >= 4 {
                    loops.push(Loop { pts });
                }
            }
        }
    }
    loops
}

// ---------------------------------------------------------------------------------------------
// geometry: f64 points, Schneider fitting
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
struct P {
    x: f64,
    y: f64,
}
fn p(x: f64, y: f64) -> P {
    P { x, y }
}
impl std::ops::Add for P {
    type Output = P;
    fn add(self, o: P) -> P {
        p(self.x + o.x, self.y + o.y)
    }
}
impl std::ops::Sub for P {
    type Output = P;
    fn sub(self, o: P) -> P {
        p(self.x - o.x, self.y - o.y)
    }
}
impl std::ops::Mul<f64> for P {
    type Output = P;
    fn mul(self, k: f64) -> P {
        p(self.x * k, self.y * k)
    }
}
impl P {
    fn dot(self, o: P) -> f64 {
        self.x * o.x + self.y * o.y
    }
    fn len(self) -> f64 {
        self.x.hypot(self.y)
    }
    fn norm(self) -> P {
        let l = self.len();
        if l < 1e-12 {
            p(0.0, 0.0)
        } else {
            self * (1.0 / l)
        }
    }
    fn v2(self) -> Vec2 {
        Vec2::new(self.x as f32, self.y as f32)
    }
}

/// Cubic Bezier segment.
#[derive(Clone, Copy, Debug)]
pub struct Bez {
    p: [P; 4],
    straight: bool,
}

fn bez_at(b: &[P; 4], t: f64) -> P {
    let m = 1.0 - t;
    b[0] * (m * m * m) + b[1] * (3.0 * m * m * t) + b[2] * (3.0 * m * t * t) + b[3] * (t * t * t)
}
fn bez_d1(b: &[P; 4], t: f64) -> P {
    let m = 1.0 - t;
    (b[1] - b[0]) * (3.0 * m * m) + (b[2] - b[1]) * (6.0 * m * t) + (b[3] - b[2]) * (3.0 * t * t)
}
fn bez_d2(b: &[P; 4], t: f64) -> P {
    let m = 1.0 - t;
    (b[2] - b[1] * 2.0 + b[0]) * (6.0 * m) + (b[3] - b[2] * 2.0 + b[1]) * (6.0 * t)
}

fn chord_params(d: &[P]) -> Vec<f64> {
    let mut u = vec![0.0; d.len()];
    for i in 1..d.len() {
        u[i] = u[i - 1] + (d[i] - d[i - 1]).len();
    }
    let total = u[d.len() - 1];
    if total > 0.0 {
        for v in u.iter_mut() {
            *v /= total;
        }
    }
    u
}

fn generate_bezier(d: &[P], u: &[f64], t1: P, t2: P) -> [P; 4] {
    let n = d.len();
    let (d0, d3) = (d[0], d[n - 1]);
    let (mut c00, mut c01, mut c11, mut x0, mut x1) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for i in 0..n {
        let t = u[i];
        let m = 1.0 - t;
        let (b0, b1, b2, b3) = (m * m * m, 3.0 * m * m * t, 3.0 * m * t * t, t * t * t);
        let a1 = t1 * b1;
        let a2 = t2 * b2;
        c00 += a1.dot(a1);
        c01 += a1.dot(a2);
        c11 += a2.dot(a2);
        let tmp = d[i] - (d0 * (b0 + b1) + d3 * (b2 + b3));
        x0 += a1.dot(tmp);
        x1 += a2.dot(tmp);
    }
    let det = c00 * c11 - c01 * c01;
    let (al, ar) = if det.abs() > 1e-12 { ((x0 * c11 - x1 * c01) / det, (c00 * x1 - c01 * x0) / det) } else { (0.0, 0.0) };
    let seg = (d3 - d0).len();
    let eps = 1e-6 * seg;
    if al < eps || ar < eps || !al.is_finite() || !ar.is_finite() {
        let dist = seg / 3.0;
        return [d0, d0 + t1 * dist, d3 + t2 * dist, d3];
    }
    [d0, d0 + t1 * al, d3 + t2 * ar, d3]
}

fn reparam(b: &[P; 4], d: &[P], u: &[f64]) -> Vec<f64> {
    d.iter()
        .zip(u)
        .map(|(&pt, &t)| {
            let q = bez_at(b, t);
            let q1 = bez_d1(b, t);
            let q2 = bez_d2(b, t);
            let num = (q - pt).dot(q1);
            let den = q1.dot(q1) + (q - pt).dot(q2);
            if den.abs() < 1e-12 {
                t
            } else {
                (t - num / den).clamp(0.0, 1.0)
            }
        })
        .collect()
}

fn max_err(b: &[P; 4], d: &[P], u: &[f64]) -> (f64, usize) {
    let mut worst = (0.0, d.len() / 2);
    for i in 1..d.len() - 1 {
        let e = (bez_at(b, u[i]) - d[i]).dot(bez_at(b, u[i]) - d[i]);
        if e >= worst.0 {
            worst = (e, i);
        }
    }
    worst
}

/// Schneider's recursive fit. `t1` points into the curve at the start, `t2` points into the curve at the end.
fn fit_cubic(d: &[P], t1: P, t2: P, err2: f64, out: &mut Vec<Bez>) {
    let n = d.len();
    if n == 2 {
        let dist = (d[1] - d[0]).len() / 3.0;
        out.push(Bez { p: [d[0], d[0] + t1 * dist, d[1] + t2 * dist, d[1]], straight: false });
        return;
    }
    let mut u = chord_params(d);
    let mut b = generate_bezier(d, &u, t1, t2);
    let (mut e, mut split) = max_err(&b, d, &u);
    if e < err2 {
        out.push(Bez { p: b, straight: false });
        return;
    }
    // Newton-Raphson re-parameterisation (Schneider tries it when the error is within a small factor of the
    // tolerance; here the gate is wider and the loop stops when it no longer improves)
    if e < err2 * 400.0 {
        for _ in 0..12 {
            let prev = e;
            u = reparam(&b, d, &u);
            b = generate_bezier(d, &u, t1, t2);
            let r = max_err(&b, d, &u);
            e = r.0;
            split = r.1;
            if e < err2 {
                out.push(Bez { p: b, straight: false });
                return;
            }
            if e > prev * 0.99 {
                break;
            }
        }
    }
    let split = split.clamp(1, n - 2);
    let centre = (d[split - 1] - d[split + 1]).norm();
    let centre = if centre.len() < 0.5 { (d[n - 1] - d[0]).norm() * -1.0 } else { centre };
    fit_cubic(&d[..=split], t1, centre, err2, out);
    fit_cubic(&d[split..], centre * -1.0, t2, err2, out);
}

fn dist_to_chord(a: P, b: P, q: P) -> f64 {
    let ab = b - a;
    let l = ab.len();
    if l < 1e-12 {
        return (q - a).len();
    }
    ((q - a).x * ab.y - (q - a).y * ab.x).abs() / l
}

// ---------------------------------------------------------------------------------------------
// polygon simplification
// ---------------------------------------------------------------------------------------------

/// Douglas-Peucker (1973) on an open polyline; the end points are always kept.
pub fn douglas_peucker(pts: &[Vec2], tol: f32) -> Vec<Vec2> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0usize, pts.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let mut worst = (0.0f64, a);
        for i in a + 1..b {
            let d = dist_to_chord(p(pts[a].x as f64, pts[a].y as f64), p(pts[b].x as f64, pts[b].y as f64), p(pts[i].x as f64, pts[i].y as f64));
            if d > worst.0 {
                worst = (d, i);
            }
        }
        if worst.0 > tol as f64 {
            keep[worst.1] = true;
            stack.push((a, worst.1));
            stack.push((worst.1, b));
        }
    }
    pts.iter().zip(&keep).filter(|(_, k)| **k).map(|(q, _)| *q).collect()
}

/// Closed-polygon Douglas-Peucker: split at the two points that are farthest apart.
pub fn douglas_peucker_closed(pts: &[Vec2], tol: f32) -> Vec<Vec2> {
    let n = pts.len();
    if n < 4 {
        return pts.to_vec();
    }
    let mut far = (0.0f32, 1usize);
    for i in 1..n {
        let d = pts[0].distance(pts[i]);
        if d > far.0 {
            far = (d, i);
        }
    }
    let mut a: Vec<Vec2> = pts[..=far.1].to_vec();
    let mut b: Vec<Vec2> = pts[far.1..].to_vec();
    b.push(pts[0]);
    a = douglas_peucker(&a, tol);
    b = douglas_peucker(&b, tol);
    a.pop();
    b.pop();
    a.extend(b);
    a
}

/// Visvalingam-Whyatt (1993) on a closed polygon: repeatedly drop the vertex whose triangle with its
/// neighbours has the smallest area until every remaining triangle has area >= `min_area`.
pub fn visvalingam_closed(pts: &[Vec2], min_area: f32) -> Vec<Vec2> {
    let mut v: Vec<Vec2> = pts.to_vec();
    let tri = |a: Vec2, b: Vec2, c: Vec2| ((b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)).abs() * 0.5;
    while v.len() > 3 {
        let n = v.len();
        let mut best = (f32::MAX, 0);
        for i in 0..n {
            let a = tri(v[(i + n - 1) % n], v[i], v[(i + 1) % n]);
            if a < best.0 {
                best = (a, i);
            }
        }
        if best.0 >= min_area {
            break;
        }
        v.remove(best.1);
    }
    v
}

// ---------------------------------------------------------------------------------------------
// chain fitting (closed loops and open runs)
// ---------------------------------------------------------------------------------------------

struct Chain {
    pts: Vec<P>,
    closed: bool,
}

impl Chain {
    fn n(&self) -> usize {
        self.pts.len()
    }
    fn at(&self, i: isize) -> P {
        let n = self.n() as isize;
        if self.closed {
            self.pts[i.rem_euclid(n) as usize]
        } else {
            self.pts[i.clamp(0, n - 1) as usize]
        }
    }
}

fn resample_closed(poly: &[(i32, i32)]) -> Vec<P> {
    let n = poly.len();
    let per: f64 = (0..n).map(|i| ((poly[(i + 1) % n].0 - poly[i].0) as f64).hypot((poly[(i + 1) % n].1 - poly[i].1) as f64)).sum();
    let step = 1.0f64.min(per / 12.0);
    let mut out = Vec::new();
    for i in 0..n {
        let a = p(poly[i].0 as f64, poly[i].1 as f64);
        let b = p(poly[(i + 1) % n].0 as f64, poly[(i + 1) % n].1 as f64);
        let len = (b - a).len();
        let m = (len / step).ceil().max(1.0) as usize;
        for j in 0..m {
            out.push(a + (b - a) * (j as f64 / m as f64));
        }
    }
    out
}

fn detect_corners(c: &Chain, k: usize, angle_deg: f64) -> Vec<bool> {
    let n = c.n();
    let mut dev = vec![0.0f64; n];
    let range: Vec<usize> = if c.closed { (0..n).collect() } else { (k..n.saturating_sub(k)).collect() };
    for &i in &range {
        let u = c.at(i as isize) - c.at(i as isize - k as isize);
        let v = c.at(i as isize + k as isize) - c.at(i as isize);
        let (lu, lv) = (u.len(), v.len());
        if lu < 1e-9 || lv < 1e-9 {
            continue;
        }
        dev[i] = (u.dot(v) / (lu * lv)).clamp(-1.0, 1.0).acos().to_degrees();
    }
    let mut flag = vec![false; n];
    for &i in &range {
        if dev[i] < angle_deg {
            continue;
        }
        let mut is_max = true;
        for off in 1..=k as isize {
            for j in [i as isize - off, i as isize + off] {
                let jj = if c.closed { j.rem_euclid(n as isize) as usize } else if j < 0 || j >= n as isize { continue } else { j as usize };
                if dev[jj] > dev[i] || (dev[jj] == dev[i] && jj < i && (c.closed || true)) {
                    is_max = false;
                }
            }
        }
        flag[i] = is_max;
    }
    flag
}

fn smooth_chain(c: &mut Chain, pinned: &[bool], passes: usize) {
    for _ in 0..passes {
        let src: Vec<P> = c.pts.clone();
        let cc = Chain { pts: src, closed: c.closed };
        // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
        #[allow(clippy::needless_range_loop)]
        for i in 0..c.n() {
            let pin = pinned[i] || (!c.closed && (i == 0 || i == c.n() - 1));
            if !pin {
                c.pts[i] = (cc.at(i as isize - 1) + cc.at(i as isize) * 2.0 + cc.at(i as isize + 1)) * 0.25;
            }
        }
    }
}

/// Fit a chain with Bezier segments. Returns the segments in order; for a closed chain the last
/// segment ends where the first starts.
fn fit_chain(mut c: Chain, o: &TraceOptions) -> Vec<Bez> {
    let n = c.n();
    if n < 2 {
        return Vec::new();
    }
    let k = o.corner_window.min((n / 6).max(2));
    let corner = if n >= 8 { detect_corners(&c, k, o.corner_angle as f64) } else { vec![true; n] };
    smooth_chain(&mut c, &corner, o.smooth);
    let err2 = (o.tol as f64) * (o.tol as f64);
    // cuts: indices where sections start/end. `hard[i]` = corner (one-sided tangents).
    let mut cuts: Vec<(usize, bool)> = Vec::new();
    if c.closed {
        for (i, _) in corner.iter().enumerate().take(n).filter(|(_, &k)| k) {
            cuts.push((i, true));
        }
        if cuts.is_empty() {
            let parts = if n >= 16 { 4 } else { 2 };
            for q in 0..parts {
                cuts.push((q * n / parts, false));
            }
        } else if cuts.len() == 1 && n >= 8 {
            cuts.push(((cuts[0].0 + n / 2) % n, false));
            cuts.sort();
        }
    } else {
        cuts.push((0, true));
        for (i, _) in corner.iter().enumerate().take(n - 1).skip(1).filter(|(_, &k)| k) {
            cuts.push((i, true));
        }
        cuts.push((n - 1, true));
    }
    let sections = if c.closed { cuts.len() } else { cuts.len() - 1 };
    let mut out = Vec::new();
    for s in 0..sections {
        let (a, ha) = cuts[s];
        let (b0, hb) = cuts[(s + 1) % cuts.len()];
        let b = if c.closed && b0 <= a { b0 + n } else { b0 };
        let data: Vec<P> = (a..=b).map(|i| c.at(i as isize)).collect();
        if data.len() < 2 {
            continue;
        }
        let m = 3.min(data.len() - 1).max(1);
        let straight = data.iter().all(|&q| dist_to_chord(data[0], data[data.len() - 1], q) <= (0.35f64).min(o.tol as f64 * 0.4));
        if straight {
            let (p0, p3) = (data[0], data[data.len() - 1]);
            out.push(Bez { p: [p0, p0, p3, p3], straight: true });
            continue;
        }
        let central = |i: isize| (c.at(i + m as isize) - c.at(i - m as isize)).norm();
        let t1 = if ha { (data[m] - data[0]).norm() } else { central(a as isize) };
        let t2 = if hb { (data[data.len() - 1 - m] - data[data.len() - 1]).norm() } else { central(b as isize) * -1.0 };
        let t1 = if t1.len() < 0.5 { (data[data.len() - 1] - data[0]).norm() } else { t1 };
        let t2 = if t2.len() < 0.5 { (data[0] - data[data.len() - 1]).norm() } else { t2 };
        fit_cubic(&data, t1, t2, err2, &mut out);
    }
    out
}

fn anchors_from(bez: &[Bez], closed: bool) -> Vec<Anchor> {
    let n = bez.len();
    if n == 0 {
        return Vec::new();
    }
    let mut a: Vec<Anchor> = Vec::with_capacity(n + 1);
    for (i, b) in bez.iter().enumerate() {
        let mut an = Anchor::corner(b.p[0].v2());
        if !b.straight {
            an.handle_out = Some(b.p[1].v2());
        }
        if i > 0 && !bez[i - 1].straight {
            an.handle_in = Some(bez[i - 1].p[2].v2());
        }
        a.push(an);
    }
    if closed {
        if !bez[n - 1].straight {
            a[0].handle_in = Some(bez[n - 1].p[2].v2());
        }
    } else {
        let last = bez[n - 1];
        let mut an = Anchor::corner(last.p[3].v2());
        if !last.straight {
            an.handle_in = Some(last.p[2].v2());
        }
        a.push(an);
    }
    a
}

fn polygon_anchors(poly: &[(i32, i32)], o: &TraceOptions) -> Vec<Anchor> {
    let pts: Vec<Vec2> = poly.iter().map(|&(x, y)| Vec2::new(x as f32, y as f32)).collect();
    let s = match o.fit {
        Fit::PolygonVw => visvalingam_closed(&pts, o.tol * 2.0),
        _ => douglas_peucker_closed(&pts, o.tol),
    };
    s.into_iter().map(Anchor::corner).collect()
}

fn loop_to_anchors(l: &Loop, o: &TraceOptions) -> Vec<Anchor> {
    if o.fit != Fit::Bezier || l.pts.len() < 5 {
        return polygon_anchors(&l.pts, o);
    }
    let chain = Chain { pts: resample_closed(&l.pts), closed: true };
    let bez = fit_chain(chain, o);
    if bez.is_empty() {
        return polygon_anchors(&l.pts, o);
    }
    anchors_from(&bez, true)
}

// ---------------------------------------------------------------------------------------------
// fill mode
// ---------------------------------------------------------------------------------------------

fn point_in_poly(pt: (f64, f64), poly: &[(i32, i32)]) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (ay, by) = (a.1 as f64, b.1 as f64);
        if (ay > pt.1) != (by > pt.1) {
            let x = a.0 as f64 + (pt.1 - ay) / (by - ay) * (b.0 - a.0) as f64;
            if pt.0 < x {
                inside = !inside;
            }
        }
    }
    inside
}

/// Trace a bitmap into filled regions or centre lines.
pub fn trace(img: &Image, o: &TraceOptions) -> Result<TraceResult, String> {
    o.validate()?;
    match o.mode {
        Mode::Fill => trace_fill(img, o),
        Mode::Line => trace_line(img, o),
    }
}

fn trace_fill(img: &Image, o: &TraceOptions) -> Result<TraceResult, String> {
    let (w, h) = (img.w, img.h);
    let (palette, mut lab) = quantise(img, o.colors, o.quant, o.seed);
    majority_filter(&mut lab, w, h, o.denoise);
    despeckle(&mut lab, w, h, o.min_area.ceil() as usize);
    let (comp, sizes) = components4(&lab, w, h);
    // bounding boxes
    let nc = sizes.len();
    let mut bb = vec![(usize::MAX, usize::MAX, 0usize, 0usize); nc];
    let mut firsts = vec![usize::MAX; nc];
    for y in 0..h {
        for x in 0..w {
            let c = comp[y * w + x] as usize;
            if firsts[c] == usize::MAX {
                firsts[c] = y * w + x;
            }
            let b = &mut bb[c];
            b.0 = b.0.min(x);
            b.1 = b.1.min(y);
            b.2 = b.2.max(x);
            b.3 = b.3.max(y);
        }
    }
    let mut regions: Vec<(Region, u16)> = Vec::new();
    for c in 0..nc {
        let first = firsts[c];
        if first == usize::MAX {
            continue;
        }
        let label = lab[first];
        if label == TRANSP {
            continue;
        }
        let (x0, y0, x1, y1) = bb[c];
        let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
        let inside = |x: usize, y: usize| comp[(y0 + y) * w + x0 + x] == c as u32;
        let loops = border_loops(bw, bh, &inside);
        let mut outers: Vec<&Loop> = Vec::new();
        let mut holes: Vec<&Loop> = Vec::new();
        for l in &loops {
            if signed_area_i(&l.pts) > 0 {
                outers.push(l);
            } else {
                holes.push(l);
            }
        }
        let off = (x0 as f32, y0 as f32);
        for ol in outers.iter() {
            let mut cuts: Vec<VPath> = Vec::new();
            // holes that are mostly transparent are cut out; the rest are stacked over
            for hl in &holes {
                let fgp = {
                    let (vx, vy) = hl.pts[0];
                    let d = if hl.pts[1].0 > vx { 0 } else if hl.pts[1].1 > vy { 1 } else if hl.pts[1].0 < vx { 2 } else { 3 };
                    let (fx, fy) = [(vx, vy), (vx - 1, vy), (vx - 1, vy - 1), (vx, vy - 1)][d];
                    (fx as f64 + 0.5, fy as f64 + 0.5)
                };
                let belongs = outers.len() == 1 || point_in_poly(fgp, &ol.pts);
                if !belongs {
                    continue;
                }
                // fraction of the hole border facing transparent pixels
                let (mut tr, mut tot) = (0usize, 0usize);
                let m = hl.pts.len();
                for i in 0..m {
                    let (a, b) = (hl.pts[i], hl.pts[(i + 1) % m]);
                    let len = ((b.0 - a.0).abs() + (b.1 - a.1).abs()) as usize;
                    let d = if b.0 > a.0 { 0 } else if b.1 > a.1 { 1 } else if b.0 < a.0 { 2 } else { 3 };
                    for s in 0..len {
                        let (vx, vy) = (a.0 + DX[d] * s as i32, a.1 + DY[d] * s as i32);
                        let (ox, oy) = [(vx, vy - 1), (vx, vy), (vx - 1, vy), (vx - 1, vy - 1)][d];
                        let (gx, gy) = (ox + x0 as i32, oy + y0 as i32);
                        tot += 1;
                        if gx >= 0 && gy >= 0 && (gx as usize) < w && (gy as usize) < h && lab[gy as usize * w + gx as usize] == TRANSP {
                            tr += 1;
                        }
                    }
                }
                if tot > 0 && tr * 2 >= tot {
                    let anchors = loop_to_anchors(hl, o);
                    if anchors.len() >= 2 {
                        let mut path = VPath::new("hole", true, anchors);
                        shift(&mut path, off);
                        cuts.push(path);
                    }
                }
            }
            let anchors = loop_to_anchors(ol, o);
            if anchors.len() < 2 {
                continue;
            }
            let mut outer = VPath::new("outer", true, anchors);
            shift(&mut outer, off);
            let oa = signed_area_i(&ol.pts) as f32 * 0.5;
            let col = palette[label as usize];
            regions.push((Region { color: col, outer, holes: cuts, pixels: sizes[c], outer_area: oa }, label));
        }
    }
    match o.zorder {
        ZOrder::Area => regions.sort_by(|a, b| b.0.outer_area.total_cmp(&a.0.outer_area).then(a.1.cmp(&b.1))),
        ZOrder::Luminance => regions.sort_by(|a, b| lum(b.0.color).total_cmp(&lum(a.0.color)).then(b.0.outer_area.total_cmp(&a.0.outer_area))),
    }
    Ok(TraceResult { width: w, height: h, mode: Mode::Fill, palette, regions: regions.into_iter().map(|r| r.0).collect(), strokes: Vec::new(), stroke_color: [0, 0, 0] })
}

fn shift(path: &mut VPath, off: (f32, f32)) {
    let d = Vec2::new(off.0, off.1);
    for a in path.anchors.iter_mut() {
        a.pos += d;
        a.handle_in = a.handle_in.map(|h| h + d);
        a.handle_out = a.handle_out.map(|h| h + d);
    }
}

// ---------------------------------------------------------------------------------------------
// line mode: threshold, thinning, skeleton graph
// ---------------------------------------------------------------------------------------------

/// Otsu (1979) threshold of a 256-bin histogram.
pub fn otsu(hist: &[usize; 256]) -> u8 {
    let total: usize = hist.iter().sum();
    if total == 0 {
        return 128;
    }
    let sum_all: f64 = hist.iter().enumerate().map(|(i, &c)| i as f64 * c as f64).sum();
    let (mut wb, mut sb, mut best, mut thr) = (0.0, 0.0, -1.0, 128u8);
    // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
    #[allow(clippy::needless_range_loop)]
    for t in 0..256 {
        wb += hist[t] as f64;
        if wb == 0.0 {
            continue;
        }
        let wf = total as f64 - wb;
        if wf == 0.0 {
            break;
        }
        sb += t as f64 * hist[t] as f64;
        let (mb, mf) = (sb / wb, (sum_all - sb) / wf);
        let between = wb * wf * (mb - mf) * (mb - mf);
        if between > best {
            best = between;
            thr = t as u8;
        }
    }
    thr
}

/// Zhang-Suen (1984) thinning of a binary image (true = ink). Returns the skeleton.
pub fn zhang_suen(bin: &[bool], w: usize, h: usize) -> Vec<bool> {
    let mut img = bin.to_vec();
    let get = |im: &[bool], x: i32, y: i32| -> u8 { (x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && im[y as usize * w + x as usize]) as u8 };
    loop {
        let mut changed = false;
        for step in 0..2 {
            let mut del = Vec::new();
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    if get(&img, x, y) == 0 {
                        continue;
                    }
                    // P2..P9 clockwise from north
                    let n = [get(&img, x, y - 1), get(&img, x + 1, y - 1), get(&img, x + 1, y), get(&img, x + 1, y + 1), get(&img, x, y + 1), get(&img, x - 1, y + 1), get(&img, x - 1, y), get(&img, x - 1, y - 1)];
                    let b: u8 = n.iter().sum();
                    if !(2..=6).contains(&b) {
                        continue;
                    }
                    let a = (0..8).filter(|&i| n[i] == 0 && n[(i + 1) % 8] == 1).count();
                    if a != 1 {
                        continue;
                    }
                    let (p2, p4, p6, p8) = (n[0], n[2], n[4], n[6]);
                    let ok = if step == 0 { p2 * p4 * p6 == 0 && p4 * p6 * p8 == 0 } else { p2 * p4 * p8 == 0 && p2 * p6 * p8 == 0 };
                    if ok {
                        del.push(y as usize * w + x as usize);
                    }
                }
            }
            if !del.is_empty() {
                changed = true;
            }
            for i in del {
                img[i] = false;
            }
        }
        if !changed {
            break;
        }
    }
    // remove redundant L-corner pixels so 8-connected lines have two neighbours
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            if get(&img, x, y) == 0 {
                continue;
            }
            let dirs: [(i32, i32); 8] = [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)];
            for q in 0..4 {
                // orthogonal neighbours a = dirs[2q], b = dirs[(2q+2)%8], diagonal between = dirs[2q+1]
                let (a, d, b) = (dirs[2 * q], dirs[2 * q + 1], dirs[(2 * q + 2) % 8]);
                if get(&img, x + a.0, y + a.1) == 1 && get(&img, x + b.0, y + b.1) == 1 {
                    let mut others = 0;
                    for (i, dd) in dirs.iter().enumerate() {
                        if i == 2 * q || i == (2 * q + 2) % 8 || (*dd == d) {
                            continue;
                        }
                        others += get(&img, x + dd.0, y + dd.1);
                    }
                    if others == 0 {
                        img[y as usize * w + x as usize] = false;
                        break;
                    }
                }
            }
        }
    }
    img
}

/// 3-4 chamfer distance transform to the nearest non-ink pixel, in pixels.
pub fn chamfer(bin: &[bool], w: usize, h: usize) -> Vec<f32> {
    let big = 1_000_000i32;
    let mut d: Vec<i32> = bin.iter().map(|&b| if b { big } else { 0 }).collect();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if d[i] == 0 {
                continue;
            }
            let mut v = d[i];
            if x > 0 {
                v = v.min(d[i - 1] + 3);
            } else {
                v = v.min(3);
            }
            if y > 0 {
                v = v.min(d[i - w] + 3);
                if x > 0 {
                    v = v.min(d[i - w - 1] + 4);
                }
                if x + 1 < w {
                    v = v.min(d[i - w + 1] + 4);
                }
            } else {
                v = v.min(3);
            }
            d[i] = v;
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            if d[i] == 0 {
                continue;
            }
            let mut v = d[i];
            if x + 1 < w {
                v = v.min(d[i + 1] + 3);
            } else {
                v = v.min(3);
            }
            if y + 1 < h {
                v = v.min(d[i + w] + 3);
                if x + 1 < w {
                    v = v.min(d[i + w + 1] + 4);
                }
                if x > 0 {
                    v = v.min(d[i + w - 1] + 4);
                }
            } else {
                v = v.min(3);
            }
            d[i] = v;
        }
    }
    d.iter().map(|&v| v as f32 / 3.0).collect()
}

struct RawChain {
    pix: Vec<usize>,
    /// node ids at the ends (None = an isolated closed loop)
    ends: Option<(usize, usize)>,
}

fn skeleton_chains(sk: &[bool], w: usize, h: usize, prune: usize) -> Vec<(Vec<P>, bool)> {
    let nb = |i: usize| -> Vec<usize> {
        let (x, y) = ((i % w) as i32, (i / w) as i32);
        let mut v = Vec::new();
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                if (dx, dy) == (0, 0) {
                    continue;
                }
                let (nx, ny) = (x + dx, y + dy);
                if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h && sk[ny as usize * w + nx as usize] {
                    v.push(ny as usize * w + nx as usize);
                }
            }
        }
        v
    };
    let deg: Vec<usize> = (0..w * h).map(|i| if sk[i] { nb(i).len() } else { 0 }).collect();
    // node pixels: degree != 2; cluster the junction pixels (degree >= 3) into single nodes
    let mut node_of = vec![usize::MAX; w * h];
    let mut nodes: Vec<Vec<usize>> = Vec::new();
    for i in 0..w * h {
        if !sk[i] || deg[i] == 2 || node_of[i] != usize::MAX {
            continue;
        }
        let id = nodes.len();
        let mut members = vec![i];
        node_of[i] = id;
        if deg[i] >= 3 {
            let mut stack = vec![i];
            while let Some(q) = stack.pop() {
                for r in nb(q) {
                    if deg[r] >= 3 && node_of[r] == usize::MAX {
                        node_of[r] = id;
                        members.push(r);
                        stack.push(r);
                    }
                }
            }
        }
        nodes.push(members);
    }
    let centre = |id: usize| -> P {
        let m = &nodes[id];
        let (sx, sy) = m.iter().fold((0.0, 0.0), |a, &q| (a.0 + (q % w) as f64 + 0.5, a.1 + (q / w) as f64 + 0.5));
        p(sx / m.len() as f64, sy / m.len() as f64)
    };
    let mut visited = vec![false; w * h];
    let mut chains: Vec<RawChain> = Vec::new();
    let mut direct: HashSet<(usize, usize)> = HashSet::new();
    // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
    #[allow(clippy::needless_range_loop)]
    for id in 0..nodes.len() {
        for &m in &nodes[id].clone() {
            for r in nb(m) {
                if node_of[r] == id {
                    continue;
                }
                if node_of[r] != usize::MAX {
                    // node to node directly
                    let key = (m.min(r), m.max(r));
                    if direct.insert(key) {
                        chains.push(RawChain { pix: vec![], ends: Some((id, node_of[r])) });
                    }
                    continue;
                }
                if visited[r] {
                    continue;
                }
                let mut pix = vec![];
                let (mut prev, mut cur) = (m, r);
                let end;
                loop {
                    visited[cur] = true;
                    pix.push(cur);
                    let next: Vec<usize> = nb(cur).into_iter().filter(|&q| q != prev && !(pix.contains(&q))).collect();
                    // prefer a node neighbour
                    if let Some(&nn) = next.iter().find(|&&q| node_of[q] != usize::MAX) {
                        end = node_of[nn];
                        break;
                    }
                    match next.iter().find(|&&q| !visited[q]) {
                        Some(&q) => {
                            prev = cur;
                            cur = q;
                        }
                        None => {
                            end = id;
                            break;
                        }
                    }
                }
                chains.push(RawChain { pix, ends: Some((id, end)) });
            }
        }
    }
    // isolated loops
    for i in 0..w * h {
        if sk[i] && !visited[i] && node_of[i] == usize::MAX {
            let mut pix = vec![i];
            visited[i] = true;
            let mut cur = i;
            loop {
                let next = nb(cur).into_iter().find(|&q| !visited[q] && node_of[q] == usize::MAX);
                match next {
                    Some(q) => {
                        visited[q] = true;
                        pix.push(q);
                        cur = q;
                    }
                    None => break,
                }
            }
            chains.push(RawChain { pix, ends: None });
        }
    }
    // prune spurs: a chain with a free tip (degree-1 node) shorter than `prune`, attached to a junction
    let node_deg: Vec<usize> = nodes.iter().map(|m| if m.len() == 1 && deg[m[0]] == 1 { 1 } else { 3 }).collect();
    let mut keep: Vec<bool> = vec![true; chains.len()];
    for (ci, c) in chains.iter().enumerate() {
        if let Some((a, b)) = c.ends {
            let tip = (node_deg[a] == 1) != (node_deg[b] == 1);
            if tip && c.pix.len() < prune {
                keep[ci] = false;
            }
        }
    }
    let chains: Vec<RawChain> = chains.into_iter().zip(&keep).filter(|(_, k)| **k).map(|(c, _)| c).collect();
    // incident count per node after pruning, then merge pass-through nodes (exactly two chain ends)
    type Poly = (Vec<P>, Option<(usize, usize)>);
    let mut polys: Vec<Poly> = chains
        .iter()
        .map(|c| {
            let mut v = Vec::new();
            if let Some((a, _)) = c.ends {
                v.push(centre(a));
            }
            v.extend(c.pix.iter().map(|&q| p((q % w) as f64 + 0.5, (q / w) as f64 + 0.5)));
            if let Some((_, b)) = c.ends {
                v.push(centre(b));
            }
            (v, c.ends)
        })
        .collect();
    loop {
        let mut inc: BTreeMap<usize, Vec<(usize, bool)>> = BTreeMap::new();
        for (ci, (_, e)) in polys.iter().enumerate() {
            if let Some((a, b)) = e {
                inc.entry(*a).or_default().push((ci, true));
                inc.entry(*b).or_default().push((ci, false));
            }
        }
        let mut merged = false;
        for (nid, list) in &inc {
            if node_deg[*nid] == 1 || list.len() != 2 || list[0].0 == list[1].0 {
                continue;
            }
            let (c1, s1) = list[0];
            let (c2, s2) = list[1];
            // orient chain 1 to end at the node, chain 2 to start at it
            let (mut v1, e1) = polys[c1].clone();
            let (mut v2, e2) = polys[c2].clone();
            let (e1, e2) = (e1.unwrap(), e2.unwrap());
            let far1 = if s1 { e1.1 } else { e1.0 };
            let far2 = if s2 { e2.1 } else { e2.0 };
            if s1 {
                v1.reverse();
            }
            if !s2 {
                v2.reverse();
            }
            v1.pop();
            v1.extend(v2);
            // far1 .. far2 (v1 starts at far1)
            let (hi, lo) = (c1.max(c2), c1.min(c2));
            polys.remove(hi);
            polys[lo] = (v1, Some((far1, far2)));
            merged = true;
            break;
        }
        if !merged {
            break;
        }
    }
    polys
        .into_iter()
        .map(|(v, e)| {
            let closed = e.is_none_or(|(a, b)| a == b && v.len() > 4);
            (v, closed)
        })
        .collect()
}

fn trace_line(img: &Image, o: &TraceOptions) -> Result<TraceResult, String> {
    let (w, h) = (img.w, img.h);
    let lumv: Vec<f64> = (0..w * h)
        .map(|i| {
            let a = img.rgba[i * 4 + 3] as f64 / 255.0;
            let c = |k: usize| img.rgba[i * 4 + k] as f64 * a + 255.0 * (1.0 - a);
            0.2126 * c(0) + 0.7152 * c(1) + 0.0722 * c(2)
        })
        .collect();
    let mut hist = [0usize; 256];
    for &l in &lumv {
        hist[l.round().clamp(0.0, 255.0) as usize] += 1;
    }
    let thr = o.threshold.unwrap_or_else(|| otsu(&hist)) as f64;
    let mut ink: Vec<bool> = lumv.iter().map(|&l| if o.invert { l > thr } else { l <= thr }).collect();
    // drop tiny ink specks (8-connected)
    let min_a = o.min_area.ceil() as usize;
    if min_a > 1 {
        let mut seen = vec![false; w * h];
        for s in 0..w * h {
            if !ink[s] || seen[s] {
                continue;
            }
            let mut comp = vec![s];
            seen[s] = true;
            let mut i = 0;
            while i < comp.len() {
                let q = comp[i];
                i += 1;
                let (x, y) = ((q % w) as i32, (q / w) as i32);
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let (nx, ny) = (x + dx, y + dy);
                        if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                            let r = ny as usize * w + nx as usize;
                            if ink[r] && !seen[r] {
                                seen[r] = true;
                                comp.push(r);
                            }
                        }
                    }
                }
            }
            if comp.len() < min_a {
                for q in comp {
                    ink[q] = false;
                }
            }
        }
    }
    let dt = chamfer(&ink, w, h);
    let sk = zhang_suen(&ink, w, h);
    let chains = skeleton_chains(&sk, w, h, o.prune);
    // ink colour: mean colour of ink pixels
    let (mut sr, mut sg, mut sb, mut sn) = (0u64, 0u64, 0u64, 0u64);
    for (i, _) in ink.iter().enumerate().take(w * h).filter(|(_, &k)| k) {
        sr += img.rgba[i * 4] as u64;
        sg += img.rgba[i * 4 + 1] as u64;
        sb += img.rgba[i * 4 + 2] as u64;
        sn += 1;
    }
    let ink_col = match (sr.checked_div(sn), sg.checked_div(sn), sb.checked_div(sn)) {
        (Some(r), Some(g), Some(b)) => [r as u8, g as u8, b as u8],
        _ => [0, 0, 0],
    };
    let mut strokes = Vec::new();
    for (pts, closed) in chains {
        if pts.len() < 2 {
            continue;
        }
        // stroke width from the distance transform along the chain
        let mut ws = 0.0f64;
        let mut cnt = 0.0;
        for q in &pts {
            let (x, y) = ((q.x - 0.5).round() as isize, (q.y - 0.5).round() as isize);
            if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                ws += (2.0 * dt[y as usize * w + x as usize] as f64 - 1.0).max(1.0);
                cnt += 1.0;
            }
        }
        let width = if cnt > 0.0 { (ws / cnt * 10.0).round() / 10.0 } else { 1.0 };
        let path = if o.fit == Fit::Bezier && pts.len() >= 3 {
            let bez = fit_chain(Chain { pts: pts.clone(), closed }, o);
            VPath::new("line", closed, anchors_from(&bez, closed))
        } else {
            let v: Vec<Vec2> = pts.iter().map(|q| q.v2()).collect();
            let s = if closed { douglas_peucker_closed(&v, o.tol) } else { douglas_peucker(&v, o.tol) };
            VPath::new("line", closed, s.into_iter().map(Anchor::corner).collect())
        };
        if path.anchors.len() >= 2 {
            strokes.push(Stroke { path, width: width as f32 });
        }
    }
    Ok(TraceResult { width: w, height: h, mode: Mode::Line, palette: vec![ink_col], regions: Vec::new(), strokes, stroke_color: ink_col })
}

// ---------------------------------------------------------------------------------------------
// project integration, SVG
// ---------------------------------------------------------------------------------------------

fn rgba_of(c: [u8; 3]) -> Rgba {
    Rgba::new(c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0)
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Build one group layer holding the traced geometry (back to front).
pub fn to_layer(res: &TraceResult, name: &str) -> Layer {
    let mut kids: Vec<Layer> = Vec::new();
    for (i, r) in res.regions.iter().enumerate() {
        let paint = Paint::Solid(rgba_of(r.color));
        let lname = format!("{}_{}", hex(r.color).trim_start_matches('#'), i);
        if r.holes.is_empty() {
            let mut pth = r.outer.clone();
            pth.name = "shape".into();
            kids.push(Layer::vector(&lname, vec![pth.with_paint(paint)]));
        } else {
            let mut paths = vec![r.outer.clone().with_paint(paint.clone())];
            for (k, h) in r.holes.iter().enumerate() {
                let mut hp = h.clone();
                hp.name = format!("hole{k}");
                paths.push(hp.with_paint(paint.clone()));
            }
            paths[0].name = "outer".into();
            kids.push(Layer::boolean(&lname, crate::fx::BoolOp::Difference, paths));
        }
    }
    if !res.strokes.is_empty() {
        let paths: Vec<VPath> = res
            .strokes
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let mut st = StrokeStyle::new(Paint::Solid(rgba_of(res.stroke_color)), s.width);
                st.cap = Cap::Round;
                st.join = Join::Round;
                let mut pth = s.path.clone();
                pth.name = format!("line{i}");
                pth.with_stroke_style(st)
            })
            .collect();
        kids.push(Layer::vector("lines", paths));
    }
    Layer::group(name, kids)
}

/// A new project, sized like the image, holding the traced group.
pub fn to_project(res: &TraceResult, name: &str) -> Project {
    let mut p = Project::new(name, res.width as f32, res.height as f32, 24.0, 1);
    p.layers.push(to_layer(res, "trace"));
    p
}

/// Append the traced group to an existing project (name made unique). The group is scaled to fit the
/// project's stage when `fit_stage` is set (uniform scale, centred); returns the layer name.
pub fn append_to_project(p: &mut Project, res: &TraceResult, name: &str, fit_stage: bool) -> Result<String, String> {
    if name.trim().is_empty() || name.contains('/') || name.contains("::") {
        return Err("layer name must be non-empty and contain no '/' or '::'".into());
    }
    let mut unique = name.to_string();
    let mut n = 2;
    while p.layers.iter().any(|l| l.name == unique) {
        unique = format!("{name}_{n}");
        n += 1;
    }
    let mut layer = to_layer(res, &unique);
    if fit_stage && res.width > 0 && res.height > 0 {
        let s = (p.width / res.width as f32).min(p.height / res.height as f32);
        if s.is_finite() && s > 0.0 {
            layer.transform.scale = Vec2::new(s, s);
            layer.transform.position = Vec2::new((p.width - res.width as f32 * s) * 0.5, (p.height - res.height as f32 * s) * 0.5);
        }
    }
    p.layers.push(layer);
    Ok(unique)
}

fn svg_path_d(path: &VPath) -> String {
    let a = &path.anchors;
    if a.is_empty() {
        return String::new();
    }
    let mut d = format!("M{:.2} {:.2}", a[0].pos.x, a[0].pos.y);
    let seg = |d: &mut String, from: &Anchor, to: &Anchor| {
        if from.handle_out.is_none() && to.handle_in.is_none() {
            d.push_str(&format!("L{:.2} {:.2}", to.pos.x, to.pos.y));
        } else {
            let (c1, c2) = (from.hout(), to.hin());
            d.push_str(&format!("C{:.2} {:.2} {:.2} {:.2} {:.2} {:.2}", c1.x, c1.y, c2.x, c2.y, to.pos.x, to.pos.y));
        }
    };
    for i in 1..a.len() {
        seg(&mut d, &a[i - 1], &a[i]);
    }
    if path.closed {
        seg(&mut d, &a[a.len() - 1], &a[0]);
        d.push('Z');
    }
    d
}

/// Stand-alone SVG. Holes use `fill-rule="evenodd"`.
pub fn to_svg(res: &TraceResult) -> String {
    let mut s = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n", w = res.width, h = res.height);
    for r in &res.regions {
        let mut d = svg_path_d(&r.outer);
        for h in &r.holes {
            d.push_str(&svg_path_d(h));
        }
        s.push_str(&format!("<path d=\"{}\" fill=\"{}\" fill-rule=\"evenodd\"/>\n", d, hex(r.color)));
    }
    for st in &res.strokes {
        s.push_str(&format!(
            "<path d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>\n",
            svg_path_d(&st.path),
            hex(res.stroke_color),
            st.width
        ));
    }
    s.push_str("</svg>\n");
    s
}

// ---------------------------------------------------------------------------------------------
// fidelity
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct Fidelity {
    /// Mean absolute error over R, G, B (0..255), both images composited over white.
    pub mae: f64,
    pub rmse: f64,
    pub psnr: f64,
    /// Mean structural similarity of the luminance (Wang et al. 2004; 8x8 windows, stride 4, uniform weights).
    pub ssim: f64,
}

fn over_white(rgba: &[u8]) -> Vec<[f64; 3]> {
    rgba.chunks_exact(4)
        .map(|p| {
            let a = p[3] as f64 / 255.0;
            [p[0] as f64 * a + 255.0 * (1.0 - a), p[1] as f64 * a + 255.0 * (1.0 - a), p[2] as f64 * a + 255.0 * (1.0 - a)]
        })
        .collect()
}

/// Compare two same-size straight-RGBA images.
pub fn compare(a: &[u8], b: &[u8], w: usize, h: usize) -> Result<Fidelity, String> {
    if a.len() != w * h * 4 || b.len() != w * h * 4 {
        return Err("images differ in size".into());
    }
    let (pa, pb) = (over_white(a), over_white(b));
    let (mut abs, mut sq) = (0.0, 0.0);
    for (x, y) in pa.iter().zip(&pb) {
        for c in 0..3 {
            let d = x[c] - y[c];
            abs += d.abs();
            sq += d * d;
        }
    }
    let n = (w * h * 3) as f64;
    let mse = sq / n;
    let psnr = if mse <= 1e-12 { 99.0 } else { 10.0 * (255.0f64 * 255.0 / mse).log10() };
    let ly = |v: &[[f64; 3]]| -> Vec<f64> { v.iter().map(|c| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]).collect() };
    let (la, lb) = (ly(&pa), ly(&pb));
    let win = 8usize.min(w).min(h);
    let (c1, c2) = ((0.01f64 * 255.0).powi(2), (0.03f64 * 255.0).powi(2));
    let (mut total, mut cnt) = (0.0, 0.0);
    let mut y = 0;
    while y + win <= h {
        let mut x = 0;
        while x + win <= w {
            let (mut ma, mut mb) = (0.0, 0.0);
            for j in 0..win {
                for i in 0..win {
                    ma += la[(y + j) * w + x + i];
                    mb += lb[(y + j) * w + x + i];
                }
            }
            let m = (win * win) as f64;
            ma /= m;
            mb /= m;
            let (mut va, mut vb, mut cov) = (0.0, 0.0, 0.0);
            for j in 0..win {
                for i in 0..win {
                    let (u, v) = (la[(y + j) * w + x + i] - ma, lb[(y + j) * w + x + i] - mb);
                    va += u * u;
                    vb += v * v;
                    cov += u * v;
                }
            }
            va /= m - 1.0;
            vb /= m - 1.0;
            cov /= m - 1.0;
            total += ((2.0 * ma * mb + c1) * (2.0 * cov + c2)) / ((ma * ma + mb * mb + c1) * (va + vb + c2));
            cnt += 1.0;
            x += 4;
        }
        y += 4;
    }
    Ok(Fidelity { mae: abs / n, rmse: mse.sqrt(), psnr, ssim: if cnt > 0.0 { total / cnt } else { 1.0 } })
}

/// Rasterise the trace with the engine's own renderer and compare it to the source bitmap.
pub fn measure(img: &Image, res: &TraceResult) -> Result<Fidelity, String> {
    let eng = crate::Engine::new(to_project(res, "fidelity")).map_err(|e| e.join("; "))?;
    let (w, h, px) = crate::raster::render_rgba(&eng, 0.0, &crate::raster::RasterOptions { transparent: true, ..Default::default() });
    if w as usize != img.w || h as usize != img.h {
        return Err("rendered size differs from the source".into());
    }
    compare(&img.rgba, &px, img.w, img.h)
}

// ---------------------------------------------------------------------------------------------
// benchmark on synthetic images (the numbers quoted in the docs come from here)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct BenchRow {
    pub name: &'static str,
    pub mode: Mode,
    pub colors: usize,
    pub tol: f32,
    pub fidelity: Fidelity,
    pub regions: usize,
    pub strokes: usize,
    pub anchors: usize,
    pub millis: f64,
}

fn render_source(p: Project) -> Image {
    let eng = crate::Engine::new(p).expect("synthetic source validates");
    let (w, h, px) = crate::raster::render_rgba(&eng, 0.0, &crate::raster::RasterOptions::default());
    Image { w: w as usize, h: h as usize, rgba: px }
}

/// Deterministic synthetic test images: anti-aliased flat shapes, a smooth gradient, text-like strokes,
/// and the shapes with pixel noise.
pub fn synthetic(name: &str) -> Option<Image> {
    let white = Rgba::new(1.0, 1.0, 1.0, 1.0);
    match name {
        "shapes" | "noisy_shapes" => {
            let mut p = Project::new("s", 160.0, 120.0, 24.0, 1);
            p.background = Some(white);
            p.layers = vec![
                Layer::vector("c", vec![VPath::circle("c", Vec2::new(40.0, 40.0), 26.0).with_fill("#d62828")]),
                Layer::vector("r", vec![VPath::rect("r", 85.0, 15.0, 55.0, 40.0).with_fill("#1d4ed8")]),
                Layer::vector("t", vec![VPath::polygon("t", &[Vec2::new(30.0, 105.0), Vec2::new(75.0, 70.0), Vec2::new(110.0, 110.0)]).with_fill("#15803d")]),
                Layer::vector("o", vec![VPath::circle("o", Vec2::new(125.0, 88.0), 22.0).with_stroke("#111111", 8.0)]),
            ];
            let mut img = render_source(p);
            if name == "noisy_shapes" {
                let mut rng = Rng::new(99);
                for px in img.rgba.chunks_exact_mut(4) {
                    for c in px.iter_mut().take(3) {
                        let n = (rng.unit() * 2.0 - 1.0) * 14.0;
                        *c = (*c as f64 + n).clamp(0.0, 255.0) as u8;
                    }
                }
            }
            Some(img)
        }
        "gradient" => {
            let mut p = Project::new("g", 160.0, 80.0, 24.0, 1);
            p.background = Some(white);
            let paint = Paint::linear(Vec2::new(0.0, 0.0), Vec2::new(160.0, 0.0), &[(0.0, "#1e3a8a"), (0.5, "#f59e0b"), (1.0, "#fef08a")]);
            p.layers = vec![Layer::vector("g", vec![VPath::rect("g", 0.0, 0.0, 160.0, 80.0).with_paint(paint)])];
            Some(render_source(p))
        }
        "strokes" => {
            let mut p = Project::new("t", 160.0, 80.0, 24.0, 1);
            p.background = Some(white);
            let line = |n: &str, pts: &[(f32, f32)]| {
                let v: Vec<Vec2> = pts.iter().map(|&(x, y)| Vec2::new(x, y)).collect();
                VPath::new(n, false, v.into_iter().map(Anchor::corner).collect()).with_stroke("#000000", 4.0)
            };
            let mut paths = vec![
                line("A1", &[(10.0, 65.0), (25.0, 15.0), (40.0, 65.0)]),
                line("A2", &[(16.0, 48.0), (34.0, 48.0)]),
                line("H1", &[(55.0, 15.0), (55.0, 65.0)]),
                line("H2", &[(80.0, 15.0), (80.0, 65.0)]),
                line("H3", &[(55.0, 40.0), (80.0, 40.0)]),
            ];
            paths.push(VPath::ellipse("O", Vec2::new(122.0, 40.0), 16.0, 25.0).with_stroke("#000000", 4.0));
            p.layers = vec![Layer::vector("txt", paths)];
            Some(render_source(p))
        }
        _ => None,
    }
}

/// Run the tracer over the synthetic images and measure each result against its source.
pub fn benchmark() -> Vec<BenchRow> {
    let cases: [(&'static str, Mode, usize, f32, usize); 8] = [
        ("shapes", Mode::Fill, 6, 1.0, 0),
        ("shapes", Mode::Fill, 6, 0.5, 0),
        ("noisy_shapes", Mode::Fill, 6, 1.0, 1),
        ("gradient", Mode::Fill, 8, 1.0, 0),
        ("gradient", Mode::Fill, 32, 1.0, 0),
        ("strokes", Mode::Line, 2, 1.0, 0),
        ("strokes", Mode::Fill, 2, 1.0, 0),
        ("strokes", Mode::Fill, 2, 0.5, 0),
    ];
    let mut rows = Vec::new();
    for (name, mode, colors, tol, denoise) in cases {
        let img = synthetic(name).expect("known case");
        let o = TraceOptions { mode, colors, tol, denoise, ..Default::default() };
        let t0 = crate::clock::Stamp::now();
        let res = trace(&img, &o).expect("benchmark trace");
        let millis = t0.elapsed().as_secs_f64() * 1000.0;
        let fidelity = measure(&img, &res).expect("benchmark measure");
        rows.push(BenchRow { name, mode, colors, tol, fidelity, regions: res.regions.len(), strokes: res.strokes.len(), anchors: res.anchor_count(), millis });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(w: usize, h: usize, f: impl Fn(usize, usize) -> [u8; 4]) -> Image {
        let mut v = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                v.extend_from_slice(&f(x, y));
            }
        }
        Image::from_rgba(w, h, v).unwrap()
    }
    const WHITE: [u8; 4] = [255, 255, 255, 255];
    const RED: [u8; 4] = [220, 30, 30, 255];
    const BLUE: [u8; 4] = [30, 40, 200, 255];
    const NONE: [u8; 4] = [0, 0, 0, 0];

    fn bbox(p: &VPath) -> (f32, f32, f32, f32) {
        let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for a in &p.anchors {
            b = (b.0.min(a.pos.x), b.1.min(a.pos.y), b.2.max(a.pos.x), b.3.max(a.pos.y));
        }
        b
    }

    fn disc_img(w: usize, h: usize, cx: f32, cy: f32, r: f32, fg: [u8; 4], bg: [u8; 4]) -> Image {
        make(w, h, |x, y| if ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt() <= r { fg } else { bg })
    }

    #[test]
    fn rectangle_gives_exact_box_and_four_straight_corners() {
        let img = make(60, 40, |x, y| if (10..30).contains(&x) && (10..20).contains(&y) { RED } else { WHITE });
        let r = trace(&img, &TraceOptions::default()).unwrap();
        assert_eq!(r.regions.len(), 2);
        // back to front: white background first, red rectangle on top
        assert_eq!(r.regions[0].color, [255, 255, 255]);
        let red = &r.regions[1];
        assert_eq!(red.color, [220, 30, 30]);
        assert_eq!(red.pixels, 200);
        assert_eq!(bbox(&red.outer), (10.0, 10.0, 30.0, 20.0));
        assert_eq!(red.outer.anchors.len(), 4, "exactly the four corners");
        assert!(red.outer.anchors.iter().all(|a| a.handle_in.is_none() && a.handle_out.is_none()), "straight edges carry no handles");
        assert!((red.outer_area - 200.0).abs() < 1e-3);
        assert!(red.holes.is_empty(), "the white background has no transparent hole to cut");
    }

    #[test]
    fn disc_area_bbox_and_curve_economy() {
        let img = disc_img(80, 80, 40.0, 40.0, 25.0, RED, WHITE);
        let r = trace(&img, &TraceOptions::default()).unwrap();
        assert_eq!(r.regions.len(), 2);
        let d = &r.regions[1];
        let b = bbox(&d.outer);
        assert!((b.0 - 15.0).abs() < 1.5 && (b.2 - 65.0).abs() < 1.5 && (b.1 - 15.0).abs() < 1.5 && (b.3 - 65.0).abs() < 1.5, "bbox {b:?}");
        let exact = std::f32::consts::PI * 625.0;
        assert!((d.pixels as f32 - exact).abs() / exact < 0.02);
        assert!(d.outer.anchors.len() <= 24, "{} anchors for a circle", d.outer.anchors.len());
        assert!(d.outer.anchors.iter().any(|a| a.handle_out.is_some()), "curved, not polygonal");
        let f = measure(&img, &r).unwrap();
        assert!(f.mae < 1.5 && f.ssim > 0.97, "{f:?}");
    }

    #[test]
    fn transparent_hole_is_cut_and_filled_hole_is_stacked() {
        // ring on a transparent background: outer region with one cut-out hole
        let ring = make(80, 80, |x, y| {
            let d = ((x as f32 + 0.5 - 40.0).powi(2) + (y as f32 + 0.5 - 40.0).powi(2)).sqrt();
            if d <= 30.0 && d > 12.0 { RED } else { NONE }
        });
        let r = trace(&ring, &TraceOptions::default()).unwrap();
        assert_eq!(r.regions.len(), 1);
        assert_eq!(r.regions[0].holes.len(), 1);
        let svg = to_svg(&r);
        assert!(svg.contains("evenodd") && svg.matches('M').count() == 2, "{svg}");
        let f = measure(&ring, &r).unwrap();
        assert!(f.mae < 2.0, "{f:?}");
        // render: the centre must stay empty (alpha 0)
        let eng = crate::Engine::new(to_project(&r, "t")).unwrap();
        let (w, _, px) = crate::raster::render_rgba(&eng, 0.0, &crate::raster::RasterOptions { transparent: true, ..Default::default() });
        assert_eq!(px[(40 * w as usize + 40) * 4 + 3], 0, "hole is empty");
        assert!(px[(40 * w as usize + 12) * 4 + 3] > 200, "ring is painted");
        // the same ring on an opaque blue background with a white centre: nothing is cut, three regions stack
        let solid = make(80, 80, |x, y| {
            let d = ((x as f32 + 0.5 - 40.0).powi(2) + (y as f32 + 0.5 - 40.0).powi(2)).sqrt();
            if d <= 12.0 { WHITE } else if d <= 30.0 { RED } else { BLUE }
        });
        let r = trace(&solid, &TraceOptions::default()).unwrap();
        assert_eq!(r.regions.len(), 3);
        assert!(r.regions.iter().all(|x| x.holes.is_empty()));
        assert!(r.regions.windows(2).all(|w| w[0].outer_area >= w[1].outer_area), "enclosing regions first");
        let f = measure(&solid, &r).unwrap();
        assert!(f.mae < 2.0 && f.ssim > 0.97, "{f:?}");
    }

    #[test]
    fn triangle_keeps_three_corners_and_star_keeps_its_points() {
        let tri = make(80, 80, |x, y| {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            // triangle (40,10) (70,65) (10,65)
            let s = |a: (f32, f32), b: (f32, f32)| (b.0 - a.0) * (fy - a.1) - (b.1 - a.1) * (fx - a.0);
            let (d1, d2, d3) = (s((40.0, 10.0), (70.0, 65.0)), s((70.0, 65.0), (10.0, 65.0)), s((10.0, 65.0), (40.0, 10.0)));
            if (d1 >= 0.0 && d2 >= 0.0 && d3 >= 0.0) || (d1 <= 0.0 && d2 <= 0.0 && d3 <= 0.0) { RED } else { WHITE }
        });
        let r = trace(&tri, &TraceOptions::default()).unwrap();
        let t = &r.regions[1];
        let corners = t.outer.anchors.iter().filter(|a| {
            let (i, o) = ((a.pos - a.hin()).normalized(), (a.hout() - a.pos).normalized());
            a.handle_in.is_none() || a.handle_out.is_none() || i.dot(o) < 0.85
        }).count();
        assert!((3..=6).contains(&corners), "{corners} corner anchors");
        let b = bbox(&t.outer);
        assert!((b.0 - 10.0).abs() < 2.0 && (b.2 - 70.0).abs() < 2.0 && (b.1 - 10.0).abs() < 2.0 && (b.3 - 65.0).abs() < 2.0, "{b:?}");
        let f = measure(&tri, &r).unwrap();
        assert!(f.mae < 3.0 && f.ssim > 0.93, "{f:?}");
    }

    #[test]
    fn schneider_fit_stays_within_tolerance_on_a_known_curve() {
        let b = [p(0.0, 0.0), p(30.0, 80.0), p(70.0, -40.0), p(100.0, 30.0)];
        let d: Vec<P> = (0..=100).map(|i| bez_at(&b, i as f64 / 100.0)).collect();
        let t1 = (b[1] - b[0]).norm();
        let t2 = (b[2] - b[3]).norm();
        let mut out = Vec::new();
        fit_cubic(&d, t1, t2, 0.25, &mut out); // tolerance 0.5 px, squared
        assert!(out.len() <= 2, "{} segments", out.len());
        // sample the fitted curve densely: every data point must be within 0.5 px of it
        let mut worst = 0.0f64;
        for q in &d {
            let mut best = f64::MAX;
            for s in &out {
                for k in 0..=400 {
                    best = best.min((bez_at(&s.p, k as f64 / 400.0) - *q).len());
                }
            }
            worst = worst.max(best);
        }
        assert!(worst <= 0.5, "worst distance {worst}");
        // a straight line is a single segment with the data on it
        let line: Vec<P> = (0..20).map(|i| p(i as f64, 2.0 * i as f64)).collect();
        let mut out = Vec::new();
        fit_cubic(&line, (line[1] - line[0]).norm(), (line[18] - line[19]).norm(), 0.01, &mut out);
        assert_eq!(out.len(), 1);
        // two points
        let mut out = Vec::new();
        fit_cubic(&[p(0.0, 0.0), p(3.0, 4.0)], p(0.6, 0.8), p(-0.6, -0.8), 0.01, &mut out);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn deterministic_and_seed_sensitive_only_where_it_should_be() {
        let img = make(64, 64, |x, y| [(x * 4) as u8, (y * 4) as u8, ((x + y) * 2) as u8, 255]);
        let o = TraceOptions { colors: 6, ..Default::default() };
        let a = trace(&img, &o).unwrap();
        let b = trace(&img, &o).unwrap();
        assert_eq!(a.palette, b.palette);
        assert_eq!(a.regions.len(), b.regions.len());
        assert_eq!(to_svg(&a), to_svg(&b), "same input, same output, byte for byte");
        let mc = trace(&img, &TraceOptions { quant: Quant::MedianCut, ..o.clone() }).unwrap();
        assert!(mc.palette.len() <= 6 && mc.palette.len() >= 2);
        assert_eq!(to_svg(&mc), to_svg(&trace(&img, &TraceOptions { quant: Quant::MedianCut, ..o }).unwrap()));
    }

    #[test]
    fn quantiser_keeps_few_colours_exactly_and_reduces_many() {
        let img = make(30, 10, |x, _| if x < 10 { RED } else if x < 20 { BLUE } else { WHITE });
        let (pal, lab) = quantise(&img, 8, Quant::KMeans, 1);
        assert_eq!(pal.len(), 3);
        assert!(pal.contains(&[220, 30, 30]) && pal.contains(&[30, 40, 200]));
        assert_eq!(lab.len(), 300);
        let grad = make(256, 4, |x, _| [x as u8, x as u8, x as u8, 255]);
        let (pal, _) = quantise(&grad, 4, Quant::KMeans, 7);
        assert_eq!(pal.len(), 4);
        assert!(pal.windows(2).all(|w| lum(w[0]) <= lum(w[1])), "sorted dark to light");
        let (pal, lab) = quantise(&make(4, 4, |_, _| NONE), 4, Quant::KMeans, 1);
        assert!(pal.is_empty() && lab.iter().all(|&l| l == TRANSP));
    }

    #[test]
    fn gradient_becomes_bands_with_bounded_error() {
        let grad = make(96, 32, |x, _| { let v = (x * 255 / 95) as u8; [v, v / 2, 255 - v, 255] });
        let o = TraceOptions { colors: 8, ..Default::default() };
        let r = trace(&grad, &o).unwrap();
        assert!(r.regions.len() >= 6 && r.regions.len() <= 12, "{}", r.regions.len());
        let f = measure(&grad, &r).unwrap();
        // 8 bands over 255 levels: half a step is about 16 levels
        assert!(f.mae < 12.0, "{f:?}");
        let f16 = measure(&grad, &trace(&grad, &TraceOptions { colors: 16, ..Default::default() }).unwrap()).unwrap();
        assert!(f16.mae < f.mae, "more colours, less error: {} vs {}", f16.mae, f.mae);
    }

    #[test]
    fn despeckle_and_majority_filter_remove_noise() {
        let mut img = make(40, 40, |_, _| WHITE);
        img.rgba[(20 * 40 + 20) * 4..(20 * 40 + 20) * 4 + 4].copy_from_slice(&RED);
        img.rgba[(5 * 40 + 5) * 4..(5 * 40 + 5) * 4 + 4].copy_from_slice(&BLUE);
        let keep = trace(&img, &TraceOptions { min_area: 0.0, denoise: 0, ..Default::default() }).unwrap();
        assert_eq!(keep.regions.len(), 3, "specks traced when asked to");
        let clean = trace(&img, &TraceOptions { min_area: 4.0, denoise: 0, ..Default::default() }).unwrap();
        assert_eq!(clean.regions.len(), 1);
        let maj = trace(&img, &TraceOptions { min_area: 0.0, denoise: 1, ..Default::default() }).unwrap();
        assert_eq!(maj.regions.len(), 1, "a lone pixel loses the 3x3 vote");
        // a one pixel wide sliver between two areas disappears with denoise, a 4 px band stays
        let sliver = make(40, 20, |x, _| if x < 19 { RED } else if x == 19 { BLUE } else { WHITE });
        assert_eq!(trace(&sliver, &TraceOptions { denoise: 1, ..Default::default() }).unwrap().regions.len(), 3, "a boundary sliver ties the 3x3 vote and stays");
        let inner = make(40, 20, |x, y| if x == 10 && y > 2 && y < 17 { BLUE } else { RED });
        assert_eq!(trace(&inner, &TraceOptions { denoise: 1, ..Default::default() }).unwrap().regions.len(), 1, "a line inside an area loses the vote");
        let band = make(40, 20, |x, _| if x < 14 { RED } else if x < 18 { BLUE } else { WHITE });
        assert_eq!(trace(&band, &TraceOptions { denoise: 1, ..Default::default() }).unwrap().regions.len(), 3);
    }

    #[test]
    fn border_following_finds_outer_and_hole_with_signed_area() {
        let (w, h) = (12, 12);
        let inside = |x: usize, y: usize| (2..10).contains(&x) && (2..10).contains(&y) && !((5..7).contains(&x) && (5..7).contains(&y));
        let loops = border_loops(w, h, &inside);
        assert_eq!(loops.len(), 2);
        let mut areas: Vec<i64> = loops.iter().map(|l| signed_area_i(&l.pts) / 2).collect();
        areas.sort();
        assert_eq!(areas, vec![-4, 64], "hole -4, outer +64");
        // two diagonal pixels stay separate (4-connectivity)
        let d = |x: usize, y: usize| (x, y) == (1, 1) || (x, y) == (2, 2);
        assert_eq!(border_loops(5, 5, &d).len(), 2);
    }

    #[test]
    fn zhang_suen_thins_a_bar_to_one_pixel_and_keeps_connectivity() {
        let (w, h) = (40usize, 15usize);
        let bar: Vec<bool> = (0..w * h).map(|i| { let (x, y) = (i % w, i / w); (3..37).contains(&x) && (5..10).contains(&y) }).collect();
        let sk = zhang_suen(&bar, w, h);
        let n = sk.iter().filter(|&&b| b).count();
        assert!((26..=40).contains(&n), "{n} skeleton pixels");
        for x in 8..32 {
            let col: Vec<usize> = (0..h).filter(|&y| sk[y * w + x]).collect();
            assert_eq!(col.len(), 1, "column {x}: {col:?}");
            assert!((6..=8).contains(&col[0]));
        }
        assert!(sk.iter().zip(&bar).all(|(s, b)| !*s || *b), "skeleton lies inside the shape");
        assert_eq!(zhang_suen(&sk, w, h), sk, "thinning a skeleton changes nothing");
    }

    #[test]
    fn line_mode_traces_a_cross_and_a_ring() {
        // white page, black plus sign 3 px thick
        let plus = make(60, 60, |x, y| if ((28..31).contains(&x) && (8..52).contains(&y)) || ((28..31).contains(&y) && (8..52).contains(&x)) { [0, 0, 0, 255] } else { WHITE });
        let o = TraceOptions { mode: Mode::Line, ..Default::default() };
        let r = trace(&plus, &o).unwrap();
        assert!((4..=6).contains(&r.strokes.len()), "{} strokes", r.strokes.len());
        assert!(r.strokes.iter().all(|s| (s.width - 3.0).abs() <= 1.0), "{:?}", r.strokes.iter().map(|s| s.width).collect::<Vec<_>>());
        let f = measure(&plus, &r).unwrap();
        assert!(f.mae < 12.0 && f.ssim > 0.7, "{f:?}");
        let ring = make(70, 70, |x, y| { let d = ((x as f32 + 0.5 - 35.0).powi(2) + (y as f32 + 0.5 - 35.0).powi(2)).sqrt(); if (22.0..25.0).contains(&d) { [0, 0, 0, 255] } else { WHITE } });
        let r = trace(&ring, &o).unwrap();
        assert_eq!(r.strokes.len(), 1);
        assert!(r.strokes[0].path.closed);
        assert!((r.strokes[0].width - 3.0).abs() <= 1.0);
        let f = measure(&ring, &r).unwrap();
        assert!(f.mae < 10.0 && f.ssim > 0.7, "{f:?}");
        // blank page and specks only
        let blank = make(20, 20, |_, _| WHITE);
        assert!(trace(&blank, &o).unwrap().strokes.is_empty());
        // inverted: white strokes on black
        let inv = make(60, 60, |x, y| if (28..31).contains(&x) && (8..52).contains(&y) { WHITE } else { [0, 0, 0, 255] });
        let r = trace(&inv, &TraceOptions { mode: Mode::Line, invert: true, ..Default::default() }).unwrap();
        assert_eq!(r.strokes.len(), 1);
    }

    #[test]
    fn otsu_splits_two_clusters() {
        let mut h = [0usize; 256];
        h[40] = 100;
        h[200] = 100;
        let t = otsu(&h);
        assert!((40..200).contains(&(t as usize)));
        assert_eq!(otsu(&[0; 256]), 128);
    }

    #[test]
    fn simplifiers_keep_corners_and_respect_tolerance() {
        let pts: Vec<Vec2> = (0..=20).map(|i| Vec2::new(i as f32, if i < 10 { 0.0 } else { (i - 10) as f32 })).collect();
        let s = douglas_peucker(&pts, 0.1);
        assert_eq!(s.len(), 3, "start, bend, end");
        assert_eq!(s[1], Vec2::new(10.0, 0.0));
        let noisy: Vec<Vec2> = (0..=20).map(|i| Vec2::new(i as f32, if i % 2 == 0 { 0.0 } else { 0.2 })).collect();
        assert_eq!(douglas_peucker(&noisy, 0.5).len(), 2);
        let sq = vec![Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.1), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)];
        assert_eq!(douglas_peucker_closed(&sq, 0.5).len(), 4);
        assert_eq!(visvalingam_closed(&sq, 1.0).len(), 4, "the 0.5 area triangle goes");
        assert_eq!(visvalingam_closed(&sq, 0.0).len(), 5);
        let img = disc_img(60, 60, 30.0, 30.0, 20.0, RED, WHITE);
        let poly = trace(&img, &TraceOptions { fit: Fit::Polygon, tol: 1.0, ..Default::default() }).unwrap();
        assert!(poly.regions[1].outer.anchors.iter().all(|a| a.handle_in.is_none() && a.handle_out.is_none()));
        let vw = trace(&img, &TraceOptions { fit: Fit::PolygonVw, ..Default::default() }).unwrap();
        assert!(vw.regions[1].outer.anchors.len() >= 8);
    }

    #[test]
    fn fidelity_metrics_behave() {
        let img = make(32, 32, |x, y| [(x * 8) as u8, (y * 8) as u8, 100, 255]);
        let same = compare(&img.rgba, &img.rgba, 32, 32).unwrap();
        assert!(same.mae == 0.0 && same.ssim > 0.999 && same.psnr >= 99.0);
        let inv: Vec<u8> = img.rgba.chunks_exact(4).flat_map(|p| [255 - p[0], 255 - p[1], 255 - p[2], 255]).collect();
        let d = compare(&img.rgba, &inv, 32, 32).unwrap();
        assert!(d.mae > 50.0 && d.ssim < 0.5 && d.psnr < 20.0, "{d:?}");
        assert!(compare(&img.rgba, &inv[..100], 32, 32).is_err());
    }

    #[test]
    fn project_integration_and_names() {
        let img = disc_img(40, 40, 20.0, 20.0, 10.0, RED, NONE);
        let r = trace(&img, &TraceOptions::default()).unwrap();
        let mut p = Project::new("host", 200.0, 100.0, 24.0, 1);
        assert_eq!(append_to_project(&mut p, &r, "traced", true).unwrap(), "traced");
        assert_eq!(append_to_project(&mut p, &r, "traced", false).unwrap(), "traced_2");
        assert!(append_to_project(&mut p, &r, "a/b", false).is_err());
        assert!(append_to_project(&mut p, &r, " ", false).is_err());
        let l = &p.layers[0];
        assert!((l.transform.scale.x - 2.5).abs() < 1e-5, "40 px image fitted into 100 px stage height");
        crate::Engine::new(p.clone()).expect("project with traced layers validates");
        let j = p.to_json_pretty();
        assert!(Project::from_json(&j).is_ok());
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        assert!(decode_image(b"not an image").is_err());
        assert!(decode_image(&[]).is_err());
        assert!(decode_image(&[0xFF, 0xD8, 0xFF, 0x00]).is_err());
        assert!(Image::from_rgba(0, 4, vec![]).is_err());
        assert!(Image::from_rgba(2, 2, vec![0; 3]).is_err());
        let img = make(8, 8, |_, _| WHITE);
        for bad in [
            TraceOptions { colors: 1, ..Default::default() },
            TraceOptions { colors: 65, ..Default::default() },
            TraceOptions { tol: 0.0, ..Default::default() },
            TraceOptions { tol: f32::NAN, ..Default::default() },
            TraceOptions { min_area: -1.0, ..Default::default() },
            TraceOptions { corner_angle: 0.0, ..Default::default() },
            TraceOptions { corner_window: 1, ..Default::default() },
            TraceOptions { smooth: 1000, ..Default::default() },
        ] {
            assert!(trace(&img, &bad).is_err());
        }
        // a 1x1 image and an all-transparent image
        let one = make(1, 1, |_, _| RED);
        assert_eq!(trace(&one, &TraceOptions::default()).unwrap().regions.len(), 1);
        let none = make(5, 5, |_, _| NONE);
        assert!(trace(&none, &TraceOptions::default()).unwrap().regions.is_empty());
    }

    #[test]
    fn benchmark_runs_and_meets_floor() {
        let rows = benchmark();
        assert_eq!(rows.len(), 8);
        for r in &rows {
            assert!(r.fidelity.mae.is_finite() && r.anchors > 0, "{r:?}");
        }
        let get = |n: &str, m: Mode, c: usize, t: f32| rows.iter().find(|r| r.name == n && r.mode == m && r.colors == c && r.tol == t).unwrap();
        let shapes = get("shapes", Mode::Fill, 6, 1.0);
        assert!(shapes.fidelity.mae < 4.0 && shapes.fidelity.ssim > 0.9, "{shapes:?}");
        let line = get("strokes", Mode::Line, 2, 1.0);
        assert!(line.fidelity.mae < 15.0 && line.fidelity.ssim > 0.6, "{line:?}");
        assert!(get("gradient", Mode::Fill, 32, 1.0).fidelity.mae < get("gradient", Mode::Fill, 8, 1.0).fidelity.mae);
        assert!(synthetic("nope").is_none());
    }

    #[test]
    fn png_round_trip_decodes_with_alpha() {
        let img = disc_img(24, 24, 12.0, 12.0, 8.0, RED, NONE);
        let png = crate::open_formats::encode_rgba_png(24, 24, &img.rgba).unwrap();
        let back = decode_image(&png).unwrap();
        assert_eq!((back.w, back.h), (24, 24));
        assert_eq!(back.rgba[(12 * 24 + 12) * 4..(12 * 24 + 12) * 4 + 4], RED);
        assert_eq!(back.rgba[3], 0);
    }
}
