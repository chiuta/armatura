// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
//! Animation CI (I-06): golden-frame regression with perceptual diff, structural checks, budgets
//! and reports (JSON, JUnit XML, self-contained HTML).
//!
//! A manifest (`osatura-ci.json`) lists named checks:
//!
//! * `render`      frames are rendered and compared with stored goldens (PNG, content-hash named,
//!                 index in `goldens/index.json`) using [`crate::percep`]: per-pixel colour
//!                 difference, SSIM, differing-pixel fraction, anti-aliasing tolerance, ignore
//!                 regions, optional pHash distance. Renderers: `raster` (tiny-skia, 8 bit),
//!                 `float` (the f32 rasteriser, quantised to 8 bit), `svg` (exact text compare).
//! * `validate`    the project compiles (`Engine::with_asset_dir`) and the linter reports nothing
//!                 at or above a severity.
//! * `fuzz`        the I-03 pose fuzzer stays under a failure rate.
//! * `limits`      layer / bone / channel / frame / canvas / file-size ceilings.
//! * `export-size` byte budget of an export (gif, apng, lottie, html, svg-frames).
//! * `determinism` the same project rendered twice from freshly loaded engines is byte-identical.
//! * `loop`        seam between the last and the first frame compared with typical frame steps.
//! * `perf`        ms/frame ceiling; informational unless `enforce` is true (machine dependent).
//!
//! Exit codes (see [`CiReport::exit_code`]): 0 all pass, 1 some check failed, 2 a check or the
//! manifest could not be evaluated.

use crate::engine::Engine;
use crate::percep::{self, CompareOptions, DeMetric, Img, Region};
use crate::project::Project;
use crate::raster::{render_rgba, RasterOptions};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use crate::clock::Stamp;

pub const MANIFEST_SCHEMA: &str = "osatura-ci/1";
pub const REPORT_SCHEMA: &str = "osatura-ci-report/1";
pub const DEFAULT_MANIFEST: &str = "osatura-ci.json";
const MAX_FRAMES_PER_CHECK: usize = 2000;

// ---- manifest ----------------------------------------------------------------------------------

/// Optional thresholds; unset fields fall back to the manifest `defaults`, then to built-ins.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tol {
    /// `de2000` (default) or `de76`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric: Option<String>,
    /// A pixel differs when its colour difference exceeds this (default 1.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pixel_de: Option<f64>,
    /// Largest colour difference allowed among counted pixels (default 8.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_de: Option<f64>,
    /// Smallest allowed mean SSIM (default 0.99).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_ssim: Option<f64>,
    /// Largest allowed fraction of counted pixels (default 0.001).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_diff_fraction: Option<f64>,
    /// Anti-aliasing tolerance: differing groups of at most this many edge pixels are forgiven (default 2, 0 = off).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aa_isolated: Option<usize>,
    /// Anti-aliasing tolerance: edge pixels with a difference up to this are forgiven (default 0 = off).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aa_edge_de: Option<f64>,
    /// Largest allowed pHash Hamming distance (default: not checked).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_phash_distance: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct Resolved {
    pub metric: DeMetric,
    pub pixel_de: f64,
    pub max_de: f64,
    pub min_ssim: f64,
    pub max_diff_fraction: f64,
    pub aa_isolated: usize,
    pub aa_edge_de: f64,
    pub max_phash_distance: Option<u32>,
}

impl Tol {
    fn validate(&self, what: &str) -> Result<(), String> {
        if let Some(m) = &self.metric {
            DeMetric::parse(m).map_err(|e| format!("{what}: {e}"))?;
        }
        let nonneg = |v: Option<f64>, n: &str| -> Result<(), String> {
            match v {
                Some(x) if !(x.is_finite() && x >= 0.0) => Err(format!("{what}: {n} must be a number >= 0")),
                _ => Ok(()),
            }
        };
        nonneg(self.pixel_de, "pixel_de")?;
        nonneg(self.max_de, "max_de")?;
        nonneg(self.aa_edge_de, "aa_edge_de")?;
        if let Some(s) = self.min_ssim {
            if !(s.is_finite() && (-1.0..=1.0).contains(&s)) {
                return Err(format!("{what}: min_ssim must be in -1..1"));
            }
        }
        if let Some(f) = self.max_diff_fraction {
            if !(f.is_finite() && (0.0..=1.0).contains(&f)) {
                return Err(format!("{what}: max_diff_fraction must be in 0..1"));
            }
        }
        if matches!(self.max_phash_distance, Some(d) if d > 64) {
            return Err(format!("{what}: max_phash_distance must be 0..64"));
        }
        if matches!(self.aa_isolated, Some(n) if n > 64) {
            return Err(format!("{what}: aa_isolated must be 0..64"));
        }
        Ok(())
    }

    pub fn resolve(&self, base: &Tol) -> Resolved {
        let metric = self.metric.as_deref().or(base.metric.as_deref()).and_then(|m| DeMetric::parse(m).ok()).unwrap_or(DeMetric::De2000);
        Resolved {
            metric,
            pixel_de: self.pixel_de.or(base.pixel_de).unwrap_or(1.0),
            max_de: self.max_de.or(base.max_de).unwrap_or(8.0),
            min_ssim: self.min_ssim.or(base.min_ssim).unwrap_or(0.99),
            max_diff_fraction: self.max_diff_fraction.or(base.max_diff_fraction).unwrap_or(0.001),
            aa_isolated: self.aa_isolated.or(base.aa_isolated).unwrap_or(2),
            aa_edge_de: self.aa_edge_de.or(base.aa_edge_de).unwrap_or(0.0),
            max_phash_distance: self.max_phash_distance.or(base.max_phash_distance),
        }
    }
}

fn one() -> f32 {
    1.0
}
fn half() -> f32 {
    0.5
}
fn raster_name() -> String {
    "raster".into()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderCheck {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Explicit frame numbers. Without `frames` and `step`: first, middle and last frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frames: Option<Vec<u32>>,
    /// Every n-th frame starting at 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<u32>,
    /// Output pixels per stage unit (a small value is a proxy render).
    #[serde(default = "one")]
    pub scale: f32,
    /// Canvas size `WxH` (layout containers re-solve), applied before `scale`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// `raster`, `float` or `svg`.
    #[serde(default = "raster_name")]
    pub renderer: String,
    #[serde(default)]
    pub thresholds: Tol,
    /// Rectangles `[x, y, w, h]` in stage units that are not compared.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignore: Vec<[f32; 4]>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidateCheck {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Lowest linter severity that fails: `error` (default), `warn`, `info` or `off`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lint: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FuzzCheck {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub samples: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<f32>,
    /// Largest allowed failure rate, 0..1 (default 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_fail_rate: Option<f32>,
    /// `false` = report the failure rate as information and never fail (`init` writes this when the
    /// project already fails most poses, where a threshold could not catch a regression). Default true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LimitsCheck {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_layers: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bones: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_channels: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_frames: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_canvas_pixels: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_file_bytes: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportSizeCheck {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// `gif`, `apng`, `lottie`, `html` or `svg-frames` (sum of all frames as SVG text).
    pub format: String,
    pub max_bytes: u64,
    #[serde(default = "half")]
    pub scale: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeterminismCheck {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frames: Option<Vec<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<u32>,
    #[serde(default = "half")]
    pub scale: f32,
    /// Default: raster, float and svg.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub renderers: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopCheck {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default = "half")]
    pub scale: f32,
    /// The seam (last -> first) may be at most this many times the mean frame-to-frame change (default 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_seam_ratio: Option<f64>,
    /// Optional absolute ceiling for the seam, mean absolute channel difference in percent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_seam_mad: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerfCheck {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default = "one")]
    pub scale: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frames: Option<Vec<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<u32>,
    pub budget_ms: f64,
    /// Relative slack above the budget before it counts as over (default 0.5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<f64>,
    /// Fail when over budget (default false: informational).
    #[serde(default)]
    pub enforce: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Check {
    Render(RenderCheck),
    Validate(ValidateCheck),
    Fuzz(FuzzCheck),
    Limits(LimitsCheck),
    ExportSize(ExportSizeCheck),
    Determinism(DeterminismCheck),
    Loop(LoopCheck),
    Perf(PerfCheck),
}

impl Check {
    pub fn name(&self) -> &str {
        match self {
            Check::Render(c) => &c.name,
            Check::Validate(c) => &c.name,
            Check::Fuzz(c) => &c.name,
            Check::Limits(c) => &c.name,
            Check::ExportSize(c) => &c.name,
            Check::Determinism(c) => &c.name,
            Check::Loop(c) => &c.name,
            Check::Perf(c) => &c.name,
        }
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Check::Render(_) => "render",
            Check::Validate(_) => "validate",
            Check::Fuzz(_) => "fuzz",
            Check::Limits(_) => "limits",
            Check::ExportSize(_) => "export-size",
            Check::Determinism(_) => "determinism",
            Check::Loop(_) => "loop",
            Check::Perf(_) => "perf",
        }
    }
    fn project<'a>(&'a self, default: &'a Option<String>) -> Option<&'a str> {
        let own = match self {
            Check::Render(c) => &c.project,
            Check::Validate(c) => &c.project,
            Check::Fuzz(c) => &c.project,
            Check::Limits(c) => &c.project,
            Check::ExportSize(c) => &c.project,
            Check::Determinism(c) => &c.project,
            Check::Loop(c) => &c.project,
            Check::Perf(c) => &c.project,
        };
        own.as_deref().or(default.as_deref())
    }
}

fn d_goldens() -> String {
    "goldens".into()
}
fn d_schema() -> String {
    MANIFEST_SCHEMA.into()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    #[serde(default = "d_schema")]
    pub schema: String,
    /// Default project (path relative to the manifest) for checks without their own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Golden directory relative to the manifest (default `goldens`).
    #[serde(default = "d_goldens")]
    pub goldens: String,
    /// Thresholds applied to every render check unless the check overrides them.
    #[serde(default)]
    pub defaults: Tol,
    pub checks: Vec<Check>,
}

fn safe_name(n: &str) -> bool {
    !n.is_empty() && n.len() <= 64 && n.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) && !n.starts_with('.')
}

fn validate_scale(s: f32, what: &str) -> Result<(), String> {
    if !(s.is_finite() && s > 0.0 && s <= 16.0) {
        return Err(format!("{what}: scale must be in (0, 16]"));
    }
    Ok(())
}

fn validate_frames(frames: &Option<Vec<u32>>, step: &Option<u32>, what: &str) -> Result<(), String> {
    if frames.is_some() && step.is_some() {
        return Err(format!("{what}: give either 'frames' or 'step', not both"));
    }
    if matches!(frames, Some(f) if f.is_empty()) {
        return Err(format!("{what}: 'frames' is empty"));
    }
    if matches!(step, Some(0)) {
        return Err(format!("{what}: 'step' must be at least 1"));
    }
    Ok(())
}

/// Paths in a manifest are relative to the manifest's directory: not empty, not absolute, no `..`, no NUL.
pub fn manifest_path_ok(what: &str, p: &str) -> Result<(), String> {
    if p.is_empty() || p.contains('\0') || p.starts_with('/') || p.starts_with('\\') || Path::new(p).is_absolute() || p.as_bytes().get(1) == Some(&b':') || p.split(['/', '\\']).any(|c| c == "..") {
        return Err(format!("{what} must be a relative path inside the manifest directory (no '..', no absolute path), got '{p}'"));
    }
    Ok(())
}

impl Manifest {
    /// Parse and validate. All problems found in the manifest itself are reported together.
    pub fn parse(text: &str) -> Result<Manifest, String> {
        let m: Manifest = serde_json::from_str(text).map_err(|e| format!("invalid manifest: {e}"))?;
        m.validate()?;
        Ok(m)
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut errs: Vec<String> = Vec::new();
        if self.schema != MANIFEST_SCHEMA {
            errs.push(format!("schema must be '{MANIFEST_SCHEMA}', got '{}'", self.schema));
        }
        if let Err(e) = manifest_path_ok("goldens", &self.goldens) {
            errs.push(e);
        }
        if let Some(p) = &self.project {
            if let Err(e) = manifest_path_ok("project", p) {
                errs.push(e);
            }
        }
        if let Err(e) = self.defaults.validate("defaults") {
            errs.push(e);
        }
        if self.checks.is_empty() {
            errs.push("checks is empty".into());
        }
        let mut seen = std::collections::HashSet::new();
        for c in &self.checks {
            let n = c.name();
            let what = format!("check '{n}'");
            if !safe_name(n) {
                errs.push(format!("check name '{n}' must be 1-64 characters of letters, digits, '-', '_' or '.' and not start with '.'"));
                continue;
            }
            if !seen.insert(n.to_string()) {
                errs.push(format!("duplicate check name '{n}'"));
            }
            match c.project(&self.project) {
                None => errs.push(format!("{what}: no project (set 'project' on the check or in the manifest)")),
                Some(p) => {
                    if let Err(e) = manifest_path_ok(&format!("{what}: project"), p) {
                        errs.push(e);
                    }
                }
            }
            let r: Result<(), String> = (|| {
                match c {
                    Check::Render(r) => {
                        validate_scale(r.scale, &what)?;
                        validate_frames(&r.frames, &r.step, &what)?;
                        r.thresholds.validate(&what)?;
                        if !["raster", "float", "svg"].contains(&r.renderer.as_str()) {
                            return Err(format!("{what}: unknown renderer '{}' (raster, float, svg)", r.renderer));
                        }
                        if let Some(s) = &r.size {
                            crate::containers::parse_size(s).map_err(|e| format!("{what}: {e}"))?;
                        }
                        for g in &r.ignore {
                            if !g.iter().all(|v| v.is_finite()) || g[2] <= 0.0 || g[3] <= 0.0 {
                                return Err(format!("{what}: ignore rectangles are [x, y, w, h] with w, h > 0"));
                            }
                        }
                        if r.renderer == "svg" && !r.ignore.is_empty() {
                            return Err(format!("{what}: ignore regions need a pixel renderer (raster or float)"));
                        }
                    }
                    Check::Validate(v) => {
                        if let Some(l) = &v.lint {
                            if !["error", "warn", "info", "off"].contains(&l.as_str()) {
                                return Err(format!("{what}: lint must be error, warn, info or off"));
                            }
                        }
                    }
                    Check::Fuzz(f) => {
                        if matches!(f.samples, Some(0)) || matches!(f.samples, Some(n) if n > 100_000) {
                            return Err(format!("{what}: samples must be 1..100000"));
                        }
                        if matches!(f.max_fail_rate, Some(r) if !(0.0..=1.0).contains(&r)) {
                            return Err(format!("{what}: max_fail_rate must be in 0..1"));
                        }
                        if matches!(f.frame, Some(x) if !x.is_finite() || x < 0.0) {
                            return Err(format!("{what}: frame must be >= 0"));
                        }
                    }
                    Check::Limits(l) => {
                        if l.max_layers.is_none() && l.max_bones.is_none() && l.max_channels.is_none() && l.max_frames.is_none() && l.max_canvas_pixels.is_none() && l.max_file_bytes.is_none() {
                            return Err(format!("{what}: set at least one limit"));
                        }
                    }
                    Check::ExportSize(e) => {
                        validate_scale(e.scale, &what)?;
                        if !["gif", "apng", "lottie", "html", "svg-frames"].contains(&e.format.as_str()) {
                            return Err(format!("{what}: unknown format '{}' (gif, apng, lottie, html, svg-frames)", e.format));
                        }
                        if e.max_bytes == 0 {
                            return Err(format!("{what}: max_bytes must be at least 1"));
                        }
                    }
                    Check::Determinism(d) => {
                        validate_scale(d.scale, &what)?;
                        validate_frames(&d.frames, &d.step, &what)?;
                        for r in &d.renderers {
                            if !["raster", "float", "svg"].contains(&r.as_str()) {
                                return Err(format!("{what}: unknown renderer '{r}' (raster, float, svg)"));
                            }
                        }
                    }
                    Check::Loop(l) => {
                        validate_scale(l.scale, &what)?;
                        if matches!(l.max_seam_ratio, Some(r) if !(r.is_finite() && r >= 0.0)) || matches!(l.max_seam_mad, Some(r) if !(r.is_finite() && r >= 0.0)) {
                            return Err(format!("{what}: seam limits must be numbers >= 0"));
                        }
                    }
                    Check::Perf(p) => {
                        validate_scale(p.scale, &what)?;
                        validate_frames(&p.frames, &p.step, &what)?;
                        if !(p.budget_ms.is_finite() && p.budget_ms > 0.0) {
                            return Err(format!("{what}: budget_ms must be a number > 0"));
                        }
                        if matches!(p.tolerance, Some(t) if !(t.is_finite() && t >= 0.0)) {
                            return Err(format!("{what}: tolerance must be a number >= 0"));
                        }
                    }
                }
                Ok(())
            })();
            if let Err(e) = r {
                errs.push(e);
            }
        }
        if errs.is_empty() {
            Ok(())
        } else {
            Err(format!("invalid manifest:\n  - {}", errs.join("\n  - ")))
        }
    }
}

// ---- results -----------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pass,
    Fail,
    Error,
    /// Informational result (never fails the run).
    Info,
    /// A golden was written by `--update`.
    Updated,
    /// A failing frame was accepted by `bless`.
    Blessed,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Fail => "fail",
            Status::Error => "error",
            Status::Info => "info",
            Status::Updated => "updated",
            Status::Blessed => "blessed",
        }
    }
    fn rank(self) -> u8 {
        match self {
            Status::Error => 5,
            Status::Fail => 4,
            Status::Blessed => 3,
            Status::Updated => 2,
            Status::Info => 1,
            Status::Pass => 0,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Metrics {
    pub ssim: f64,
    pub ssim_rgb: f64,
    pub mse: f64,
    pub psnr: f64,
    pub max_de: f64,
    pub max_de_raw: f64,
    pub mean_de: f64,
    pub diff_pixels: usize,
    pub forgiven_pixels: usize,
    pub ignored_pixels: usize,
    pub diff_fraction: f64,
    pub phash_distance: u32,
}

/// PNG bytes kept in memory for the report and the artifacts directory.
#[derive(Clone, Debug, Default)]
pub struct FrameImages {
    pub golden: Option<Vec<u8>>,
    pub actual: Option<Vec<u8>>,
    pub heat: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FrameResult {
    pub frame: u32,
    pub status: Status,
    pub message: String,
    /// `true` when the frame matched the golden byte for byte.
    pub exact: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub golden: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<Metrics>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<Region>,
    #[serde(skip_serializing_if = "is_zero")]
    pub regions_total: usize,
    #[serde(skip)]
    pub images: FrameImages,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

#[derive(Clone, Debug, Serialize)]
pub struct CheckResult {
    pub name: String,
    pub kind: String,
    pub status: Status,
    pub message: String,
    pub duration_ms: f64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub frames: Vec<FrameResult>,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub details: Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct Summary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub errors: usize,
    pub info: usize,
    pub updated: usize,
    pub blessed: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CiReport {
    pub schema: String,
    pub manifest: String,
    pub mode: String,
    pub duration_ms: f64,
    pub summary: Summary,
    pub checks: Vec<CheckResult>,
}

impl CiReport {
    /// 0 all good, 1 at least one check failed, 2 at least one check could not be evaluated.
    pub fn exit_code(&self) -> i32 {
        if self.summary.errors > 0 {
            2
        } else if self.summary.failed > 0 {
            1
        } else {
            0
        }
    }

    fn summarize(checks: &[CheckResult]) -> Summary {
        let n = |s: Status| checks.iter().filter(|c| c.status == s).count();
        Summary { total: checks.len(), passed: n(Status::Pass), failed: n(Status::Fail), errors: n(Status::Error), info: n(Status::Info), updated: n(Status::Updated), blessed: n(Status::Blessed) }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

// ---- golden store ------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GoldenRef {
    pub hash: String,
    pub renderer: String,
    #[serde(default)]
    pub w: usize,
    #[serde(default)]
    pub h: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GoldenIndex {
    #[serde(default)]
    pub schema: String,
    /// check name -> zero-padded frame -> golden
    #[serde(default)]
    pub entries: BTreeMap<String, BTreeMap<String, GoldenRef>>,
}

const INDEX_SCHEMA: &str = "osatura-goldens/1";

pub struct GoldenStore {
    dir: PathBuf,
    pub index: GoldenIndex,
    dirty: bool,
}

fn fkey(frame: u32) -> String {
    format!("{frame:05}")
}

fn golden_name_ok(hash: &str) -> Result<(), String> {
    if hash.len() < 24 || hash.len() > 64 || !hash.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) {
        return Err(format!("invalid golden hash {hash:?} (need 24-64 lowercase hex digits)"));
    }
    Ok(())
}

impl GoldenStore {
    pub fn open(dir: &Path) -> Result<GoldenStore, String> {
        let p = dir.join("index.json");
        let index = if p.exists() {
            let t = std::fs::read_to_string(&p).map_err(|e| format!("cannot read {}: {e}", p.display()))?;
            let i: GoldenIndex = serde_json::from_str(&t).map_err(|e| format!("invalid golden index {}: {e}", p.display()))?;
            if i.schema != INDEX_SCHEMA {
                return Err(format!("golden index {} has schema '{}', expected '{INDEX_SCHEMA}'", p.display(), i.schema));
            }
            for (check, frames) in &i.entries {
                for (frame, r) in frames {
                    golden_name_ok(&r.hash).map_err(|e| format!("golden index {}: check '{check}' frame {frame}: {e}", p.display()))?;
                }
            }
            i
        } else {
            GoldenIndex { schema: INDEX_SCHEMA.into(), entries: BTreeMap::new() }
        };
        Ok(GoldenStore { dir: dir.to_path_buf(), index, dirty: false })
    }

    pub fn get(&self, check: &str, frame: u32) -> Option<&GoldenRef> {
        self.index.entries.get(check).and_then(|m| m.get(&fkey(frame)))
    }

    /// The file of a golden. The hash comes from a file in the repository (the index), so it is checked:
    /// 24 to 64 lowercase hex digits, nothing that could name another path (`../x`, `/etc/passwd`).
    fn file(&self, r: &GoldenRef) -> Result<PathBuf, String> {
        golden_name_ok(&r.hash)?;
        Ok(self.dir.join(format!("{}.{}", r.hash, if r.renderer == "svg" { "svg" } else { "png" })))
    }

    /// Load a golden image, checking that the file still has the pixels its name promises.
    pub fn load_img(&self, r: &GoldenRef) -> Result<Img, String> {
        let p = self.file(r)?;
        let bytes = crate::assets::read_path(&p, crate::assets::MAX_IMAGE_BYTES, "golden")?;
        let img = Img::from_png(&bytes).map_err(|e| format!("golden {}: {e}", p.display()))?;
        if !img.content_hash().starts_with(&r.hash) {
            return Err(format!("golden {} was modified: its pixels do not match its name", p.display()));
        }
        Ok(img)
    }

    pub fn load_text(&self, r: &GoldenRef) -> Result<String, String> {
        let p = self.file(r)?;
        let t = String::from_utf8(crate::assets::read_path(&p, crate::assets::MAX_DATA_BYTES, "golden")?).map_err(|_| format!("golden {} is not UTF-8 text", p.display()))?;
        if !crate::vcs::sha256_hex(t.as_bytes()).starts_with(&r.hash) {
            return Err(format!("golden {} was modified: its text does not match its name", p.display()));
        }
        Ok(t)
    }

    fn put(&mut self, check: &str, frame: u32, r: GoldenRef, bytes: &[u8]) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("cannot create {}: {e}", self.dir.display()))?;
        let p = self.file(&r)?;
        if !p.exists() {
            crate::fsutil::write_atomic(&p, bytes)?;
        }
        self.index.entries.entry(check.to_string()).or_default().insert(fkey(frame), r);
        self.dirty = true;
        Ok(())
    }

    pub fn put_img(&mut self, check: &str, frame: u32, renderer: &str, img: &Img) -> Result<String, String> {
        let hash = img.content_hash()[..24].to_string();
        self.put(check, frame, GoldenRef { hash: hash.clone(), renderer: renderer.into(), w: img.w, h: img.h }, &img.to_png()?)?;
        Ok(hash)
    }

    pub fn put_text(&mut self, check: &str, frame: u32, text: &str) -> Result<String, String> {
        let hash = crate::vcs::sha256_hex(text.as_bytes())[..24].to_string();
        self.put(check, frame, GoldenRef { hash: hash.clone(), renderer: "svg".into(), w: 0, h: 0 }, text.as_bytes())?;
        Ok(hash)
    }

    /// Drop index entries of checks that are no longer in the manifest or frames that are no longer listed.
    fn retain(&mut self, check: &str, frames: &[u32]) {
        if let Some(m) = self.index.entries.get_mut(check) {
            let keep: std::collections::HashSet<String> = frames.iter().map(|&f| fkey(f)).collect();
            let before = m.len();
            m.retain(|k, _| keep.contains(k));
            if m.len() != before {
                self.dirty = true;
            }
        }
    }

    pub fn save(&mut self) -> Result<(), String> {
        if self.dirty {
            std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
            let p = self.dir.join("index.json");
            crate::fsutil::write_atomic(&p, serde_json::to_string_pretty(&self.index).unwrap().as_bytes())?;
            self.dirty = false;
        }
        Ok(())
    }

    /// Delete golden files that no index entry refers to. Returns the removed file names.
    pub fn prune(&self) -> Result<Vec<String>, String> {
        let used: std::collections::HashSet<PathBuf> = self.index.entries.values().flat_map(|m| m.values()).filter_map(|r| self.file(r).ok()).collect();
        let mut gone = Vec::new();
        let rd = match std::fs::read_dir(&self.dir) {
            Ok(r) => r,
            Err(_) => return Ok(gone),
        };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            let hashed = name.len() > 25 && name.as_bytes()[24] == b'.' && name[..24].chars().all(|c| c.is_ascii_hexdigit()) && (name.ends_with(".png") || name.ends_with(".svg"));
            if hashed && !used.contains(&p) {
                std::fs::remove_file(&p).map_err(|e| format!("cannot remove {}: {e}", p.display()))?;
                gone.push(name);
            }
        }
        gone.sort();
        Ok(gone)
    }
}

// ---- running -----------------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct RunOptions {
    /// Rewrite every golden of the selected render checks from the current render.
    pub update: bool,
    /// Accept only failing or missing goldens; passing ones are left untouched.
    pub bless: bool,
    /// Delete golden files nothing refers to.
    pub prune: bool,
    /// Run only checks with these names (empty = all).
    pub only: Vec<String>,
    /// Write failing frames (golden, actual, diff, side by side) as PNG files below this directory.
    pub artifacts: Option<PathBuf>,
}

fn load_engine(base: &Path, project: &str, size: Option<&str>) -> Result<(Engine, u64), String> {
    let path = crate::assets::resolve(base, project)?;
    load_engine_at(&path, size)
}

/// Load the project at `path` (already resolved, or named by the user on the command line).
fn load_engine_at(path: &Path, size: Option<&str>) -> Result<(Engine, u64), String> {
    let path = path.to_path_buf();
    let text = String::from_utf8(crate::assets::read_path(&path, crate::assets::MAX_DATA_BYTES, "project")?).map_err(|_| format!("project {} is not UTF-8", path.display()))?;
    let mut p = Project::from_json(&text).map_err(|e| format!("invalid project JSON in {}: {e}", path.display()))?;
    if let Some(s) = size {
        let (w, h) = crate::containers::parse_size(s)?;
        p = crate::containers::responsive(&p, w, h)?;
    }
    let dir = path.parent().map(|d| d.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
    let e = Engine::with_asset_dir(p, &dir).map_err(|errs| format!("{} is not a valid project: {}", path.display(), errs.join("; ")))?;
    Ok((e, text.len() as u64))
}

fn pick_frames(frames: &Option<Vec<u32>>, step: &Option<u32>, total: u32) -> Result<Vec<u32>, String> {
    let total = total.max(1);
    let mut v: Vec<u32> = if let Some(f) = frames {
        if let Some(bad) = f.iter().find(|&&x| x >= total) {
            return Err(format!("frame {bad} is outside the project's 0..{} frames", total - 1));
        }
        f.clone()
    } else if let Some(s) = step {
        (0..total).step_by(*s as usize).collect()
    } else {
        vec![0, total / 2, total - 1]
    };
    v.sort_unstable();
    v.dedup();
    if v.len() > MAX_FRAMES_PER_CHECK {
        return Err(format!("{} frames selected, at most {MAX_FRAMES_PER_CHECK} per check", v.len()));
    }
    Ok(v)
}

fn render_img(e: &Engine, renderer: &str, frame: u32, scale: f32) -> Result<Img, String> {
    let o = RasterOptions { scale, show_bones: false, transparent: false };
    match renderer {
        "raster" => {
            let (w, h, px) = render_rgba(e, frame as f32, &o);
            Img::new(w as usize, h as usize, px)
        }
        "float" => {
            let f = crate::fraster::render_float(e, frame as f32, &o);
            Img::new(f.width as usize, f.height as usize, f.to_straight_u8())
        }
        other => Err(format!("renderer '{other}' has no pixel output")),
    }
}

fn render_svg(e: &Engine, frame: u32) -> String {
    crate::svg::render_frame(e, frame as f32, &crate::svg::RenderOptions::default())
}

fn finish(name: &str, kind: &str, status: Status, message: String, t0: Stamp, frames: Vec<FrameResult>, details: Value) -> CheckResult {
    CheckResult { name: name.into(), kind: kind.into(), status, message, duration_ms: t0.elapsed().as_secs_f64() * 1000.0, frames, details }
}

fn errored(name: &str, kind: &str, t0: Stamp, msg: String) -> CheckResult {
    finish(name, kind, Status::Error, msg, t0, vec![], Value::Null)
}

fn compare_opts(t: &Resolved, ignore_px: Vec<[i64; 4]>) -> CompareOptions {
    CompareOptions { metric: t.metric, pixel_de: t.pixel_de, aa_isolated: t.aa_isolated, aa_edge_de: t.aa_edge_de, ignore: ignore_px, ..CompareOptions::default() }
}

fn metrics_of(d: &percep::Diff) -> Metrics {
    Metrics {
        ssim: d.ssim,
        ssim_rgb: d.ssim_rgb,
        mse: d.mse,
        psnr: d.psnr,
        max_de: d.max_de,
        max_de_raw: d.max_de_raw,
        mean_de: d.mean_de,
        diff_pixels: d.diff_pixels,
        forgiven_pixels: d.forgiven_pixels,
        ignored_pixels: d.ignored_pixels,
        diff_fraction: d.diff_fraction,
        phash_distance: d.phash_distance,
    }
}

fn frame_ok(frame: u32, status: Status, message: &str, golden: Option<String>) -> FrameResult {
    FrameResult { frame, status, message: message.into(), exact: false, golden, metrics: None, regions: vec![], regions_total: 0, images: FrameImages::default() }
}

/// The reasons (empty = within all thresholds) why a comparison fails the thresholds.
pub fn judge(d: &percep::Diff, t: &Resolved) -> Vec<String> {
    let mut why = Vec::new();
    if d.ssim < t.min_ssim {
        why.push(format!("SSIM {:.4} < {:.4}", d.ssim, t.min_ssim));
    }
    if d.ssim_rgb < t.min_ssim {
        why.push(format!("channel SSIM (min of R, G, B) {:.4} < {:.4}", d.ssim_rgb, t.min_ssim));
    }
    if d.max_de > t.max_de {
        why.push(format!("max {} {:.2} > {:.2}", t.metric.name(), d.max_de, t.max_de));
    }
    if d.diff_fraction > t.max_diff_fraction {
        why.push(format!("{:.4}% of pixels differ ({} px) > {:.4}%", d.diff_fraction * 100.0, d.diff_pixels, t.max_diff_fraction * 100.0));
    }
    if let Some(m) = t.max_phash_distance {
        if d.phash_distance > m {
            why.push(format!("pHash distance {} > {m}", d.phash_distance));
        }
    }
    why
}

fn safe_write(dir: &Path, name: &str, bytes: &[u8]) {
    if std::fs::create_dir_all(dir).is_ok() {
        // atomic and symlink-refusing like every other output; a refused artifact is reported, not dropped silently
        if let Err(e) = crate::fsutil::write_atomic(&dir.join(name), bytes) {
            eprintln!("warning: artifact not written: {e}");
        }
    }
}

fn run_render(c: &RenderCheck, base: &Path, project: &str, defaults: &Tol, store: &mut GoldenStore, o: &RunOptions) -> CheckResult {
    let t0 = Stamp::now();
    let (e, _) = match load_engine(base, project, c.size.as_deref()) {
        Ok(x) => x,
        Err(m) => return errored(&c.name, "render", t0, m),
    };
    let frames = match pick_frames(&c.frames, &c.step, e.project.frames) {
        Ok(f) => f,
        Err(m) => return errored(&c.name, "render", t0, m),
    };
    let th = c.thresholds.resolve(defaults);
    let writing = o.update || o.bless;
    if o.update {
        store.retain(&c.name, &frames);
    }
    let mut out = Vec::new();
    for &f in &frames {
        let fr = if c.renderer == "svg" { svg_frame(&e, &c.name, f, store, o) } else { pixel_frame(&e, c, f, &th, store, o) };
        out.push(match fr {
            Ok(r) => r,
            Err(m) => frame_ok(f, Status::Error, &m, None),
        });
        if let (Some(dir), true) = (&o.artifacts, out.last().map(|r| matches!(r.status, Status::Fail | Status::Blessed)).unwrap_or(false)) {
            let r = out.last().unwrap();
            let d = dir.join(&c.name);
            for (suffix, b) in [("golden", &r.images.golden), ("actual", &r.images.actual), ("diff", &r.images.heat)] {
                if let Some(b) = b {
                    safe_write(&d, &format!("f{:05}-{suffix}.png", f), b);
                }
            }
        }
    }
    let worst = out.iter().map(|r| r.status).max_by_key(|s| s.rank()).unwrap_or(Status::Pass);
    let bad = out.iter().filter(|r| r.status == Status::Fail).count();
    let msg = match worst {
        Status::Error => format!("{} frame(s) could not be evaluated", out.iter().filter(|r| r.status == Status::Error).count()),
        Status::Fail => format!("{bad} of {} frame(s) differ from the golden", out.len()),
        Status::Blessed => format!("{} golden(s) accepted", out.iter().filter(|r| r.status == Status::Blessed).count()),
        Status::Updated => format!("{} golden(s) written", out.iter().filter(|r| r.status == Status::Updated).count()),
        _ => format!("{} frame(s) match", out.len()),
    };
    let _ = writing;
    finish(&c.name, "render", worst, msg, t0, out, json!({"renderer": c.renderer, "scale": c.scale, "metric": th.metric.name(), "thresholds": {
        "pixel_de": th.pixel_de, "max_de": th.max_de, "min_ssim": th.min_ssim, "max_diff_fraction": th.max_diff_fraction,
        "aa_isolated": th.aa_isolated, "aa_edge_de": th.aa_edge_de, "max_phash_distance": th.max_phash_distance}}))
}

fn svg_frame(e: &Engine, name: &str, f: u32, store: &mut GoldenStore, o: &RunOptions) -> Result<FrameResult, String> {
    let now = render_svg(e, f);
    let hash = crate::vcs::sha256_hex(now.as_bytes())[..24].to_string();
    let have = store.get(name, f).cloned();
    if o.update {
        let changed = have.as_ref().map(|g| g.hash != hash).unwrap_or(true);
        store.put_text(name, f, &now)?;
        let mut r = frame_ok(f, if changed { Status::Updated } else { Status::Pass }, if changed { "golden written" } else { "unchanged" }, Some(hash));
        r.exact = !changed;
        return Ok(r);
    }
    let Some(g) = have else {
        if o.bless {
            store.put_text(name, f, &now)?;
            return Ok(frame_ok(f, Status::Blessed, "new golden accepted", Some(hash)));
        }
        return Ok(frame_ok(f, Status::Fail, "no golden stored (run with --update or bless)", None));
    };
    if g.hash == hash {
        let mut r = frame_ok(f, Status::Pass, "identical", Some(hash));
        r.exact = true;
        return Ok(r);
    }
    let old = store.load_text(&g)?;
    let line = old.lines().zip(now.lines()).position(|(a, b)| a != b).map(|i| i + 1).unwrap_or(old.lines().count().min(now.lines().count()) + 1);
    let msg = format!("SVG text differs from line {line} (golden {} lines, now {} lines)", old.lines().count(), now.lines().count());
    if o.bless {
        store.put_text(name, f, &now)?;
        return Ok(frame_ok(f, Status::Blessed, &msg, Some(hash)));
    }
    Ok(frame_ok(f, Status::Fail, &msg, Some(g.hash)))
}

fn pixel_frame(e: &Engine, c: &RenderCheck, f: u32, th: &Resolved, store: &mut GoldenStore, o: &RunOptions) -> Result<FrameResult, String> {
    let now = render_img(e, &c.renderer, f, c.scale)?;
    let hash = now.content_hash()[..24].to_string();
    let have = store.get(&c.name, f).cloned();
    if o.update {
        let changed = have.as_ref().map(|g| g.hash != hash).unwrap_or(true);
        store.put_img(&c.name, f, &c.renderer, &now)?;
        let mut r = frame_ok(f, if changed { Status::Updated } else { Status::Pass }, if changed { "golden written" } else { "unchanged" }, Some(hash));
        r.exact = !changed;
        return Ok(r);
    }
    let Some(g) = have else {
        if o.bless {
            store.put_img(&c.name, f, &c.renderer, &now)?;
            return Ok(frame_ok(f, Status::Blessed, "new golden accepted", Some(hash)));
        }
        return Ok(frame_ok(f, Status::Fail, "no golden stored (run with --update or bless)", None));
    };
    if g.hash == hash {
        let mut r = frame_ok(f, Status::Pass, "identical", Some(hash));
        r.exact = true;
        return Ok(r);
    }
    let gold = store.load_img(&g)?;
    let mut r = frame_ok(f, Status::Fail, "", Some(g.hash.clone()));
    r.images.golden = Some(gold.to_png()?);
    r.images.actual = Some(now.to_png()?);
    if (gold.w, gold.h) != (now.w, now.h) {
        r.message = format!("size changed: golden {}x{}, now {}x{}", gold.w, gold.h, now.w, now.h);
    } else {
        let ignore: Vec<[i64; 4]> = c
            .ignore
            .iter()
            .map(|g| {
                let (x0, y0) = ((g[0] * c.scale).floor() as i64, (g[1] * c.scale).floor() as i64);
                let (x1, y1) = (((g[0] + g[2]) * c.scale).ceil() as i64, ((g[1] + g[3]) * c.scale).ceil() as i64);
                [x0, y0, x1 - x0, y1 - y0]
            })
            .collect();
        let d = percep::compare(&gold, &now, &compare_opts(th, ignore))?;
        let why = judge(&d, th);
        r.metrics = Some(metrics_of(&d));
        r.regions = d.regions.clone();
        r.regions_total = d.regions_total;
        r.images.heat = Some(percep::heatmap(&gold, &d).to_png()?);
        if why.is_empty() {
            r.status = Status::Pass;
            r.message = if d.diff_pixels == 0 && d.forgiven_pixels > 0 { format!("within tolerance ({} anti-aliasing pixel(s) forgiven)", d.forgiven_pixels) } else { "within tolerance".into() };
            r.images = FrameImages::default();
            return Ok(r);
        }
        r.message = why.join("; ");
    }
    if o.bless {
        store.put_img(&c.name, f, &c.renderer, &now)?;
        r.status = Status::Blessed;
        r.message = format!("accepted: {}", r.message);
    }
    Ok(r)
}

fn run_validate(c: &ValidateCheck, base: &Path, project: &str) -> CheckResult {
    let t0 = Stamp::now();
    let (e, _) = match load_engine(base, project, None) {
        Ok(x) => x,
        Err(m) => return finish(&c.name, "validate", Status::Fail, m, t0, vec![], Value::Null),
    };
    let level = c.lint.as_deref().unwrap_or("error");
    let findings = crate::lint::lint(&e.project, crate::assets::resolve(base, project).ok().as_deref().and_then(|p| p.parent()));
    let min = match level {
        "error" => Some(crate::lint::Severity::Error),
        "warn" => Some(crate::lint::Severity::Warn),
        "info" => Some(crate::lint::Severity::Info),
        _ => None,
    };
    let bad: Vec<&crate::lint::Finding> = findings.iter().filter(|f| min.map(|m| f.severity >= m).unwrap_or(false)).collect();
    let details = json!({"findings": findings.len(), "failing": bad.len(), "lint": level});
    if bad.is_empty() {
        finish(&c.name, "validate", Status::Pass, format!("project compiles; {} lint finding(s), none at '{level}' or above", findings.len()), t0, vec![], details)
    } else {
        let list: Vec<String> = bad.iter().take(5).map(|f| format!("{} [{}] {}", f.code, f.target, f.message)).collect();
        finish(&c.name, "validate", Status::Fail, format!("{} lint finding(s) at '{level}' or above: {}", bad.len(), list.join("; ")), t0, vec![], details)
    }
}

fn run_fuzz(c: &FuzzCheck, base: &Path, project: &str) -> CheckResult {
    let t0 = Stamp::now();
    let path = match crate::assets::resolve(base, project) {
        Ok(p) => p,
        Err(e) => return errored(&c.name, "fuzz", t0, e),
    };
    let text = match crate::assets::read_path(&path, crate::assets::MAX_DATA_BYTES, "project").and_then(|b| String::from_utf8(b).map_err(|_| "project is not UTF-8".to_string())) {
        Ok(t) => t,
        Err(e) => return errored(&c.name, "fuzz", t0, e),
    };
    let p = match Project::from_json(&text) {
        Ok(p) => p,
        Err(e) => return errored(&c.name, "fuzz", t0, format!("invalid project JSON: {e}")),
    };
    let mut fo = crate::fuzz::FuzzOptions { samples: c.samples.unwrap_or(100), seed: c.seed.unwrap_or(1), ..Default::default() };
    fo.frame = c.frame.unwrap_or(0.0);
    let rep = match crate::fuzz::run(&p, path.parent(), &fo) {
        Ok(r) => r,
        Err(e) => return errored(&c.name, "fuzz", t0, e),
    };
    let max = c.max_fail_rate.unwrap_or(0.0);
    let s = &rep.summary;
    let details = json!({"samples": s.samples, "failures": s.failures, "failure_rate": s.failure_rate, "max_fail_rate": max, "by_kind": s.by_kind, "seed": fo.seed});
    // a gate at 100 % (or switched off) cannot fail: report it as information, never as a pass
    if c.gate == Some(false) || max >= 1.0 {
        let why = if c.gate == Some(false) { "non-gating (gate = false)" } else { "max_fail_rate is 100 %, so this cannot fail" };
        return finish(&c.name, "fuzz", Status::Info, format!("{} of {} pose samples fail ({:.2}%); not a gate: {why}", s.failures, s.samples, s.failure_rate * 100.0), t0, vec![], details);
    }
    if s.failure_rate > max {
        finish(&c.name, "fuzz", Status::Fail, format!("{} of {} pose samples fail ({:.2}% > {:.2}%)", s.failures, s.samples, s.failure_rate * 100.0, max * 100.0), t0, vec![], details)
    } else {
        finish(&c.name, "fuzz", Status::Pass, format!("{} of {} pose samples fail ({:.2}% <= {:.2}%)", s.failures, s.samples, s.failure_rate * 100.0, max * 100.0), t0, vec![], details)
    }
}

fn run_limits(c: &LimitsCheck, base: &Path, project: &str) -> CheckResult {
    let t0 = Stamp::now();
    let (e, bytes) = match load_engine(base, project, None) {
        Ok(x) => x,
        Err(m) => return errored(&c.name, "limits", t0, m),
    };
    let p = &e.project;
    let mut over = Vec::new();
    let mut chk = |name: &str, have: u64, max: Option<u64>| {
        if let Some(m) = max {
            if have > m {
                over.push(format!("{name} {have} > {m}"));
            }
        }
    };
    chk("layers", e.node_count() as u64, c.max_layers.map(|v| v as u64));
    chk("bones", p.bones.len() as u64, c.max_bones.map(|v| v as u64));
    chk("channels", p.channels.len() as u64, c.max_channels.map(|v| v as u64));
    chk("frames", p.frames as u64, c.max_frames.map(|v| v as u64));
    chk("canvas pixels", (p.width * p.height) as u64, c.max_canvas_pixels);
    chk("file bytes", bytes, c.max_file_bytes);
    let details = json!({"layers": e.node_count(), "bones": p.bones.len(), "channels": p.channels.len(), "frames": p.frames, "canvas_pixels": (p.width * p.height) as u64, "file_bytes": bytes});
    if over.is_empty() {
        finish(&c.name, "limits", Status::Pass, "within all limits".into(), t0, vec![], details)
    } else {
        finish(&c.name, "limits", Status::Fail, over.join("; "), t0, vec![], details)
    }
}

fn run_export_size(c: &ExportSizeCheck, base: &Path, project: &str) -> CheckResult {
    let t0 = Stamp::now();
    let (e, _) = match load_engine(base, project, None) {
        Ok(x) => x,
        Err(m) => return errored(&c.name, "export-size", t0, m),
    };
    let bytes: Result<u64, String> = match c.format.as_str() {
        "gif" => crate::export::gif_bytes(&e, c.scale).map(|b| b.len() as u64),
        "apng" => crate::open_formats::apng_bytes(&e, c.scale).map(|b| b.len() as u64),
        "lottie" => Ok(crate::export::lottie_json(&e).to_string().len() as u64),
        "html" => Ok(crate::export::html_player(&e).len() as u64),
        _ => Ok((0..e.project.frames).map(|f| render_svg(&e, f).len() as u64).sum()),
    };
    match bytes {
        Err(m) => errored(&c.name, "export-size", t0, m),
        Ok(b) => {
            let st = if b > c.max_bytes { Status::Fail } else { Status::Pass };
            finish(&c.name, "export-size", st, format!("{} export is {b} bytes (budget {})", c.format, c.max_bytes), t0, vec![], json!({"format": c.format, "bytes": b, "max_bytes": c.max_bytes}))
        }
    }
}

fn digest(e: &Engine, renderer: &str, frame: u32, scale: f32) -> Result<String, String> {
    if renderer == "svg" {
        Ok(crate::vcs::sha256_hex(render_svg(e, frame).as_bytes()))
    } else {
        Ok(render_img(e, renderer, frame, scale)?.content_hash())
    }
}

fn run_determinism(c: &DeterminismCheck, base: &Path, project: &str) -> CheckResult {
    let t0 = Stamp::now();
    let (e1, _) = match load_engine(base, project, None) {
        Ok(x) => x,
        Err(m) => return errored(&c.name, "determinism", t0, m),
    };
    let (e2, _) = match load_engine(base, project, None) {
        Ok(x) => x,
        Err(m) => return errored(&c.name, "determinism", t0, m),
    };
    let frames = match pick_frames(&c.frames, &c.step, e1.project.frames) {
        Ok(f) => f,
        Err(m) => return errored(&c.name, "determinism", t0, m),
    };
    let rends: Vec<String> = if c.renderers.is_empty() { vec!["raster".into(), "float".into(), "svg".into()] } else { c.renderers.clone() };
    let mut bad = Vec::new();
    let mut n = 0;
    for r in &rends {
        for &f in &frames {
            n += 1;
            // twice on the first engine (same instance reused) and once on the second (fresh load)
            let a = digest(&e1, r, f, c.scale);
            let b = digest(&e1, r, f, c.scale);
            let d = digest(&e2, r, f, c.scale);
            match (a, b, d) {
                (Ok(a), Ok(b), Ok(d)) => {
                    if a != b || a != d {
                        bad.push(format!("{r} frame {f}"));
                    }
                }
                (Err(m), _, _) | (_, Err(m), _) | (_, _, Err(m)) => return errored(&c.name, "determinism", t0, m),
            }
        }
    }
    let details = json!({"renders_compared": n, "renderers": rends, "frames": frames});
    if bad.is_empty() {
        finish(&c.name, "determinism", Status::Pass, format!("{n} render(s) repeated byte-identically"), t0, vec![], details)
    } else {
        finish(&c.name, "determinism", Status::Fail, format!("not reproducible: {}", bad.join(", ")), t0, vec![], details)
    }
}

/// Mean absolute difference of the four 8-bit channels, in percent of full scale.
fn mad(a: &Img, b: &Img) -> f64 {
    a.rgba.iter().zip(&b.rgba).map(|(&x, &y)| (x as f64 - y as f64).abs()).sum::<f64>() / a.rgba.len() as f64 / 255.0 * 100.0
}

/// `(seam, mean step, max step)` of the animation as mean absolute differences in percent.
pub fn loop_stats(e: &Engine, scale: f32) -> Result<(f64, f64, f64), String> {
    let n = e.project.frames;
    if n < 2 {
        return Err("a loop check needs at least 2 frames".into());
    }
    if n as usize > 600 {
        return Err(format!("{n} frames: loop checks render every frame, at most 600"));
    }
    let imgs: Result<Vec<Img>, String> = (0..n).map(|f| render_img(e, "raster", f, scale)).collect();
    let imgs = imgs?;
    let steps: Vec<f64> = imgs.windows(2).map(|w| mad(&w[0], &w[1])).collect();
    let seam = mad(&imgs[imgs.len() - 1], &imgs[0]);
    Ok((seam, steps.iter().sum::<f64>() / steps.len() as f64, steps.iter().cloned().fold(0.0, f64::max)))
}

fn run_loop(c: &LoopCheck, base: &Path, project: &str) -> CheckResult {
    let t0 = Stamp::now();
    let (e, _) = match load_engine(base, project, None) {
        Ok(x) => x,
        Err(m) => return errored(&c.name, "loop", t0, m),
    };
    let (seam, mean, max) = match loop_stats(&e, c.scale) {
        Ok(x) => x,
        Err(m) => return errored(&c.name, "loop", t0, m),
    };
    let ratio = c.max_seam_ratio.unwrap_or(2.0);
    let allowed = ratio * mean.max(0.01);
    let mut why = Vec::new();
    if seam > allowed {
        why.push(format!("seam {seam:.3}% > {ratio} x mean step {mean:.3}%"));
    }
    if matches!(c.max_seam_mad, Some(m) if seam > m) {
        why.push(format!("seam {seam:.3}% > absolute limit {:.3}%", c.max_seam_mad.unwrap()));
    }
    let details = json!({"seam_mad_pct": seam, "mean_step_mad_pct": mean, "max_step_mad_pct": max, "max_seam_ratio": ratio});
    if why.is_empty() {
        finish(&c.name, "loop", Status::Pass, format!("seam {seam:.3}% vs mean step {mean:.3}%"), t0, vec![], details)
    } else {
        finish(&c.name, "loop", Status::Fail, why.join("; "), t0, vec![], details)
    }
}

fn run_perf(c: &PerfCheck, base: &Path, project: &str) -> CheckResult {
    let t0 = Stamp::now();
    let (e, _) = match load_engine(base, project, None) {
        Ok(x) => x,
        Err(m) => return errored(&c.name, "perf", t0, m),
    };
    let frames = match &c.frames {
        None if c.step.is_none() => {
            let n = e.project.frames.max(1);
            let k = n.min(12);
            (0..k).map(|i| i * n / k).collect()
        }
        _ => match pick_frames(&c.frames, &c.step, e.project.frames) {
            Ok(f) => f,
            Err(m) => return errored(&c.name, "perf", t0, m),
        },
    };
    let o = RasterOptions { scale: c.scale, show_bones: false, transparent: false };
    for &f in &frames {
        let _ = render_rgba(&e, f as f32, &o); // warm-up
    }
    let mut ms: Vec<f64> = frames
        .iter()
        .map(|&f| {
            let t = Stamp::now();
            let _ = render_rgba(&e, f as f32, &o);
            t.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    ms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = ms[ms.len() / 2];
    let limit = c.budget_ms * (1.0 + c.tolerance.unwrap_or(0.5));
    let over = median > limit;
    let details = json!({"median_ms": median, "max_ms": ms[ms.len() - 1], "budget_ms": c.budget_ms, "limit_ms": limit, "frames_timed": ms.len(), "enforced": c.enforce});
    let msg = format!("median {median:.2} ms/frame, budget {} ms (+{:.0}% tolerance = {limit:.2} ms){}", c.budget_ms, c.tolerance.unwrap_or(0.5) * 100.0, if over && !c.enforce { "; over budget, informational only" } else { "" });
    let st = if over && c.enforce { Status::Fail } else if c.enforce { Status::Pass } else { Status::Info };
    finish(&c.name, "perf", st, msg, t0, vec![], details)
}

/// Run the manifest. `Err` means the manifest itself or the golden index is unusable (exit code 2).
pub fn run(manifest_path: &Path, o: &RunOptions) -> Result<CiReport, String> {
    let t0 = Stamp::now();
    let text = std::fs::read_to_string(manifest_path).map_err(|e| format!("cannot read manifest {}: {e}", manifest_path.display()))?;
    let m = Manifest::parse(&text)?;
    if o.update && o.bless {
        return Err("--update and --bless exclude each other".into());
    }
    for n in &o.only {
        if !m.checks.iter().any(|c| c.name() == n) {
            return Err(format!("--only: no check named '{n}' (have: {})", m.checks.iter().map(|c| c.name()).collect::<Vec<_>>().join(", ")));
        }
    }
    let base = manifest_path.parent().filter(|p| !p.as_os_str().is_empty()).map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
    let mut store = GoldenStore::open(&base.join(&m.goldens))?;
    let mut results = Vec::new();
    for c in &m.checks {
        if !o.only.is_empty() && !o.only.iter().any(|n| n == c.name()) {
            continue;
        }
        let project = c.project(&m.project).unwrap_or_default().to_string();
        results.push(match c {
            Check::Render(r) => run_render(r, &base, &project, &m.defaults, &mut store, o),
            Check::Validate(v) => run_validate(v, &base, &project),
            Check::Fuzz(f) => run_fuzz(f, &base, &project),
            Check::Limits(l) => run_limits(l, &base, &project),
            Check::ExportSize(x) => run_export_size(x, &base, &project),
            Check::Determinism(d) => run_determinism(d, &base, &project),
            Check::Loop(l) => run_loop(l, &base, &project),
            Check::Perf(p) => run_perf(p, &base, &project),
        });
    }
    store.save()?;
    if o.prune {
        store.prune()?;
    }
    let mode = if o.update { "update" } else if o.bless { "bless" } else { "verify" };
    Ok(CiReport {
        schema: REPORT_SCHEMA.into(),
        manifest: manifest_path.display().to_string(),
        mode: mode.into(),
        duration_ms: t0.elapsed().as_secs_f64() * 1000.0,
        summary: CiReport::summarize(&results),
        checks: results,
    })
}

// ---- init --------------------------------------------------------------------------------------

/// Build a manifest with sensible default checks for `project`, calibrated on the project as it is now
/// (limits get 50 % headroom, the fuzz and perf budgets are set from a measurement, the loop check is
/// added only when the project currently loops cleanly).
pub fn init_manifest(project_path: &Path, manifest_path: &Path) -> Result<Manifest, String> {
    let base = manifest_path.parent().filter(|p| !p.as_os_str().is_empty()).map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
    let rel = match (std::fs::canonicalize(project_path), std::fs::canonicalize(&base)) {
        (Ok(p), Ok(b)) => p.strip_prefix(&b).map(|r| r.to_path_buf()).unwrap_or(p),
        _ => project_path.to_path_buf(),
    };
    let rel_s = rel.to_string_lossy().replace('\\', "/");
    let (e, bytes) = load_engine_at(project_path, None)?;
    let p = &e.project;
    let frames = p.frames.max(1);
    let k = frames.min(6);
    let mut pick: Vec<u32> = (0..k).map(|i| if k == 1 { 0 } else { i * (frames - 1) / (k - 1) }).collect();
    pick.dedup();
    let maxdim = p.width.max(p.height).max(1.0);
    let scale = (480.0 / maxdim).min(1.0);
    let scale = (scale * 1000.0).round() / 1000.0;
    let mut checks = vec![Check::Validate(ValidateCheck { name: "validate".into(), project: None, lint: None })];
    checks.push(Check::Render(RenderCheck { name: "frames".into(), project: None, frames: Some(pick.clone()), step: None, scale, size: None, renderer: "raster".into(), thresholds: Tol::default(), ignore: vec![] }));
    let mid = pick.get(pick.len() / 2).copied().unwrap_or(0);
    checks.push(Check::Render(RenderCheck { name: "frames-float".into(), project: None, frames: Some(vec![0, mid]), step: None, scale, size: None, renderer: "float".into(), thresholds: Tol { max_de: Some(10.0), min_ssim: Some(0.98), ..Tol::default() }, ignore: vec![] }));
    checks.push(Check::Determinism(DeterminismCheck { name: "determinism".into(), project: None, frames: Some(vec![0, frames - 1]), step: None, scale: 0.5, renderers: vec![] }));
    // fuzz: measure the current failure rate and allow a small margin
    let fo = crate::fuzz::FuzzOptions { samples: 100, seed: 1, ..Default::default() };
    if let Ok(rep) = crate::fuzz::run(&e.project, Path::new(project_path).parent(), &fo) {
        let rate = rep.summary.failure_rate;
        let max = ((rate + 0.02) * 100.0).ceil() / 100.0;
        // when most poses already fail, no threshold can catch a regression: write an information-only check
        let gate = if rate > 0.5 || max >= 1.0 { Some(false) } else { None };
        checks.push(Check::Fuzz(FuzzCheck { name: "pose-fuzz".into(), project: None, samples: Some(100), seed: Some(1), frame: None, max_fail_rate: Some(max.min(1.0)), gate }));
    }
    let hr = |v: usize| ((v as f64 * 1.5).ceil() as usize).max(v + 1);
    checks.push(Check::Limits(LimitsCheck {
        name: "limits".into(),
        project: None,
        max_layers: Some(hr(e.node_count())),
        max_bones: Some(hr(p.bones.len())),
        max_channels: Some(hr(p.channels.len())),
        max_frames: Some(hr(p.frames as usize) as u32),
        max_canvas_pixels: None,
        max_file_bytes: Some(hr(bytes as usize) as u64),
    }));
    if let Ok(b) = crate::export::gif_bytes(&e, 0.5) {
        checks.push(Check::ExportSize(ExportSizeCheck { name: "gif-size".into(), project: None, format: "gif".into(), max_bytes: (b.len() as f64 * 1.5).ceil() as u64, scale: 0.5 }));
    }
    if (2..=600).contains(&frames) {
        if let Ok((seam, mean, _)) = loop_stats(&e, 0.5) {
            if seam <= 2.0 * mean.max(0.01) {
                checks.push(Check::Loop(LoopCheck { name: "loop-seam".into(), project: None, scale: 0.5, max_seam_ratio: None, max_seam_mad: None }));
            }
        }
    }
    // perf: median of up to 6 frames, budget 4x with a minimum of 5 ms
    let o = RasterOptions { scale, show_bones: false, transparent: false };
    let mut ms: Vec<f64> = pick
        .iter()
        .map(|&f| {
            let _ = render_rgba(&e, f as f32, &o);
            let t = Stamp::now();
            let _ = render_rgba(&e, f as f32, &o);
            t.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    ms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let budget = (ms[ms.len() / 2] * 4.0).ceil().max(5.0);
    checks.push(Check::Perf(PerfCheck { name: "render-speed".into(), project: None, scale, frames: Some(pick), step: None, budget_ms: budget, tolerance: Some(0.5), enforce: false }));
    let m = Manifest { schema: MANIFEST_SCHEMA.into(), project: Some(rel_s), goldens: "goldens".into(), defaults: Tol::default(), checks };
    m.validate()?;
    Ok(m)
}

// ---- reports -----------------------------------------------------------------------------------

fn xml_esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => o.push(c),
        }
    }
    o
}

/// JUnit XML: one `testcase` per check; failing frames are listed in the failure text.
pub fn junit_xml(r: &CiReport) -> String {
    let failures = r.checks.iter().filter(|c| c.status == Status::Fail).count();
    let errors = r.checks.iter().filter(|c| c.status == Status::Error).count();
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    s.push_str(&format!(
        "<testsuites name=\"osatura-ci\" tests=\"{}\" failures=\"{failures}\" errors=\"{errors}\" time=\"{:.3}\">\n",
        r.checks.len(),
        r.duration_ms / 1000.0
    ));
    s.push_str(&format!(
        "  <testsuite name=\"osatura-ci\" tests=\"{}\" failures=\"{failures}\" errors=\"{errors}\" skipped=\"0\" time=\"{:.3}\">\n",
        r.checks.len(),
        r.duration_ms / 1000.0
    ));
    for c in &r.checks {
        s.push_str(&format!("    <testcase classname=\"osatura.{}\" name=\"{}\" time=\"{:.3}\"", xml_esc(&c.kind), xml_esc(&c.name), c.duration_ms / 1000.0));
        let mut body = String::new();
        for f in c.frames.iter().filter(|f| matches!(f.status, Status::Fail | Status::Error)) {
            body.push_str(&format!("frame {}: {}\n", f.frame, f.message));
            for g in f.regions.iter().take(5) {
                body.push_str(&format!("  region x={} y={} w={} h={} pixels={} max_de={:.2}\n", g.x, g.y, g.w, g.h, g.pixels, g.max_de));
            }
        }
        match c.status {
            Status::Fail => s.push_str(&format!(">\n      <failure message=\"{}\" type=\"{}\">{}</failure>\n    </testcase>\n", xml_esc(&c.message), xml_esc(&c.kind), xml_esc(&body))),
            Status::Error => s.push_str(&format!(">\n      <error message=\"{}\" type=\"{}\">{}</error>\n    </testcase>\n", xml_esc(&c.message), xml_esc(&c.kind), xml_esc(&body))),
            _ => s.push_str(&format!(">\n      <system-out>{}</system-out>\n    </testcase>\n", xml_esc(&format!("{}: {}", c.status.name(), c.message)))),
        }
    }
    s.push_str("  </testsuite>\n</testsuites>\n");
    s
}

fn h(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn data_uri(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", crate::base64::encode(png))
}

/// One self-contained HTML page: summary, check table, and for every failing frame the golden,
/// the current render and the difference map as inlined PNG data URIs. No external resources.
pub fn html_report(r: &CiReport) -> String {
    let mut s = String::new();
    s.push_str("<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Osatura CI report</title>\n<style>\n");
    s.push_str(":root{--bg:#fff;--fg:#1c1f24;--mut:#5b6573;--line:#d9dee5;--card:#f6f8fa;--pass:#1a7f37;--fail:#cf222e;--err:#9a6700;--info:#0969da;--acc:#8250df}\n@media(prefers-color-scheme:dark){:root{--bg:#0d1117;--fg:#e6edf3;--mut:#9198a1;--line:#30363d;--card:#161b22;--pass:#3fb950;--fail:#ff7b72;--err:#d29922;--info:#58a6ff;--acc:#bc8cff}}\n");
    s.push_str("body{margin:0;padding:16px;background:var(--bg);color:var(--fg);font:14px/1.45 system-ui,-apple-system,Segoe UI,Roboto,sans-serif}main{max-width:1100px;margin:0 auto}h1{font-size:1.4rem;margin:.2em 0}h2{font-size:1.05rem;margin:1.4em 0 .5em}\n");
    s.push_str(".sum{display:flex;flex-wrap:wrap;gap:8px;margin:12px 0}.sum div{background:var(--card);border:1px solid var(--line);border-radius:8px;padding:8px 14px;min-width:84px}.sum b{display:block;font-size:1.4rem}\n");
    s.push_str("table{border-collapse:collapse;width:100%}th,td{text-align:left;padding:6px 8px;border-bottom:1px solid var(--line);vertical-align:top}th{color:var(--mut);font-weight:600}th.num,td.num{text-align:right;font-variant-numeric:tabular-nums}\n");
    s.push_str(".b{display:inline-block;padding:1px 8px;border-radius:99px;font-size:.8rem;font-weight:600;border:1px solid currentColor}.pass{color:var(--pass)}.fail{color:var(--fail)}.error{color:var(--err)}.info,.updated,.blessed{color:var(--info)}\n");
    s.push_str("details{background:var(--card);border:1px solid var(--line);border-radius:8px;margin:8px 0;padding:6px 12px}summary{cursor:pointer;font-weight:600;padding:4px 0}.mut{color:var(--mut)}code{font:12px ui-monospace,Menlo,monospace}\n");
    s.push_str(".imgs{display:flex;flex-wrap:wrap;gap:12px;margin:8px 0}figure{margin:0}figcaption{font-size:.8rem;color:var(--mut);margin-bottom:2px}.imgs img{max-width:100%;height:auto;border:1px solid var(--line);image-rendering:pixelated;background:repeating-conic-gradient(#ccc 0 25%,#fff 0 50%) 0 0/12px 12px}\n");
    s.push_str("button{font:inherit;border:1px solid var(--line);background:var(--bg);color:var(--fg);border-radius:6px;padding:2px 10px;cursor:pointer}\n</style></head><body><main>\n");
    s.push_str("<h1>Osatura CI report</h1>\n");
    s.push_str(&format!("<p class=\"mut\">manifest <code>{}</code> &middot; mode {} &middot; {:.2} s &middot; exit code {}</p>\n", h(&r.manifest), h(&r.mode), r.duration_ms / 1000.0, r.exit_code()));
    let sm = &r.summary;
    s.push_str(&format!(
        "<div class=\"sum\"><div><b>{}</b>checks</div><div class=\"pass\"><b>{}</b>passed</div><div class=\"fail\"><b>{}</b>failed</div><div class=\"error\"><b>{}</b>errors</div><div class=\"info\"><b>{}</b>info / updated / blessed</div></div>\n",
        sm.total,
        sm.passed,
        sm.failed,
        sm.errors,
        sm.info + sm.updated + sm.blessed
    ));
    s.push_str("<table><thead><tr><th>Check</th><th>Kind</th><th>Status</th><th>Result</th><th class=\"num\">ms</th></tr></thead><tbody>\n");
    for c in &r.checks {
        s.push_str(&format!(
            "<tr><td><a href=\"#c-{0}\">{0}</a></td><td>{1}</td><td><span class=\"b {2}\">{2}</span></td><td>{3}</td><td class=\"num\">{4:.0}</td></tr>\n",
            h(&c.name),
            h(&c.kind),
            c.status.name(),
            h(&c.message),
            c.duration_ms
        ));
    }
    s.push_str("</tbody></table>\n<h2>Details</h2>\n");
    let mut img_count = 0;
    for c in &r.checks {
        let open = if matches!(c.status, Status::Fail | Status::Error) { " open" } else { "" };
        s.push_str(&format!("<details id=\"c-{}\"{open}><summary><span class=\"b {}\">{}</span> {} <span class=\"mut\">({})</span></summary>\n<p>{}</p>\n", h(&c.name), c.status.name(), c.status.name(), h(&c.name), h(&c.kind), h(&c.message)));
        if !c.frames.is_empty() {
            s.push_str("<table><thead><tr><th>Frame</th><th>Status</th><th class=\"num\">SSIM</th><th class=\"num\">max dE</th><th class=\"num\">diff px</th><th class=\"num\">forgiven</th><th>Note</th></tr></thead><tbody>\n");
            for f in &c.frames {
                match &f.metrics {
                    Some(m) => s.push_str(&format!(
                        "<tr><td>{}</td><td><span class=\"b {}\">{}</span></td><td class=\"num\">{:.4}</td><td class=\"num\">{:.2}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td>{}</td></tr>\n",
                        f.frame, f.status.name(), f.status.name(), m.ssim, m.max_de, m.diff_pixels, m.forgiven_pixels, h(&f.message)
                    )),
                    None => s.push_str(&format!("<tr><td>{}</td><td><span class=\"b {}\">{}</span></td><td class=\"num\">{}</td><td class=\"num\"></td><td class=\"num\"></td><td class=\"num\"></td><td>{}</td></tr>\n", f.frame, f.status.name(), f.status.name(), if f.exact { "1" } else { "" }, h(&f.message))),
                }
            }
            s.push_str("</tbody></table>\n");
            for f in c.frames.iter().filter(|f| matches!(f.status, Status::Fail | Status::Blessed) && f.images.actual.is_some()) {
                if img_count >= 60 {
                    s.push_str("<p class=\"mut\">further images omitted (60 per report)</p>\n");
                    break;
                }
                img_count += 1;
                s.push_str(&format!("<details{open}><summary>frame {} &mdash; {}</summary>\n<div class=\"imgs\">", f.frame, h(&f.message)));
                let fig = |cap: &str, b: &Option<Vec<u8>>, id: &str| -> String {
                    b.as_ref().map(|b| format!("<figure><figcaption>{cap}</figcaption><img id=\"{id}\" alt=\"{cap}\" src=\"{}\"></figure>", data_uri(b))).unwrap_or_default()
                };
                let id = format!("fl-{}-{}", h(&c.name), f.frame);
                s.push_str(&fig("golden", &f.images.golden, &format!("{id}-g")));
                s.push_str(&fig("current", &f.images.actual, &format!("{id}-a")));
                s.push_str(&fig("difference", &f.images.heat, &format!("{id}-d")));
                s.push_str("</div>");
                if f.images.golden.is_some() {
                    s.push_str(&format!("<p><button type=\"button\" onclick=\"var g=document.getElementById('{id}-g'),a=document.getElementById('{id}-a'),t=g.src;g.src=a.src;a.src=t\">swap golden / current</button> <span class=\"mut\">click to flicker between the two</span></p>"));
                }
                if !f.regions.is_empty() {
                    s.push_str("<table><thead><tr><th>Region</th><th class=\"num\">x</th><th class=\"num\">y</th><th class=\"num\">w</th><th class=\"num\">h</th><th class=\"num\">pixels</th><th class=\"num\">max dE</th></tr></thead><tbody>");
                    for (i, g) in f.regions.iter().enumerate().take(10) {
                        s.push_str(&format!("<tr><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{:.2}</td></tr>", i + 1, g.x, g.y, g.w, g.h, g.pixels, g.max_de));
                    }
                    s.push_str("</tbody></table>");
                    if f.regions_total > 10 {
                        s.push_str(&format!("<p class=\"mut\">{} regions in total, 10 shown</p>", f.regions_total));
                    }
                }
                s.push_str("</details>\n");
            }
        }
        if !c.details.is_null() {
            s.push_str(&format!("<p class=\"mut\"><code>{}</code></p>\n", h(&c.details.to_string())));
        }
        s.push_str("</details>\n");
    }
    s.push_str("</main></body></html>\n");
    s
}

// ---- tests -------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;

    fn tmpdir(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::ScratchDir::new(&format!("osatura-ci-{tag}"))
    }

    #[test]
    fn golden_hash_from_the_index_cannot_name_another_path() {
        let d = tmpdir("hashname");
        for bad in ["../../etc/passwd", "/etc/passwd", "", "abc", "ABCDEF0123456789ABCDEF01", &"a".repeat(70), "0123456789abcdef0123456/", "0123456789abcdef01234567/../x"] {
            assert!(golden_name_ok(bad).is_err(), "{bad:?} must be refused");
        }
        assert!(golden_name_ok(&"ab12".repeat(6)).is_ok());
        // through the index file: opening a store whose index names a path outside the directory fails
        std::fs::write(
            d.join("index.json"),
            format!(r#"{{"schema":"{INDEX_SCHEMA}","entries":{{"c":{{"0":{{"hash":"../../../etc/passwd","renderer":"raster","w":1,"h":1}}}}}}}}"#),
        )
        .unwrap();
        let err = GoldenStore::open(&d).err().expect("must be refused");
        assert!(err.contains("golden index"), "{err}");
        // and a direct reference is refused before any file is touched
        let store = GoldenStore { dir: d.clone(), index: GoldenIndex::default(), dirty: false };
        let r = GoldenRef { hash: "../../../etc/passwd".into(), renderer: "raster".into(), w: 1, h: 1 };
        assert!(store.load_img(&r).is_err());
        assert!(store.load_text(&r).is_err());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn manifest_paths_must_stay_inside_the_manifest_directory() {
        for bad in ["/etc/passwd", "../x.json", "a/../../x", "a\\..\\b", "", "C:\\x", "a\0b"] {
            assert!(manifest_path_ok("project", bad).is_err(), "{bad:?}");
        }
        for ok in ["demo.json", "sub/dir/p.json", "./p.json"] {
            assert!(manifest_path_ok("project", ok).is_ok(), "{ok:?}");
        }
    }

    fn write_demo(dir: &Path) -> PathBuf {
        let p = dir.join("demo.json");
        // 8 frames instead of 48 keeps the tests fast in debug builds
        let mut pr = demo::project();
        pr.frames = 8;
        std::fs::write(&p, serde_json::to_string(&pr).unwrap()).unwrap();
        p
    }

    fn small_manifest(dir: &Path) -> PathBuf {
        let m = json!({"schema": MANIFEST_SCHEMA, "project": "demo.json", "checks": [
            {"kind": "validate", "name": "validate"},
            {"kind": "render", "name": "frames", "frames": [0, 6], "scale": 0.1},
            {"kind": "render", "name": "svg", "frames": [0], "renderer": "svg"},
            {"kind": "determinism", "name": "det", "frames": [0, 5], "scale": 0.1},
            {"kind": "limits", "name": "limits", "max_layers": 500, "max_bones": 100},
            {"kind": "export-size", "name": "gif", "format": "gif", "max_bytes": 5000000, "scale": 0.1},
            {"kind": "perf", "name": "speed", "budget_ms": 1000.0, "scale": 0.1, "frames": [0]}
        ]});
        let p = dir.join(DEFAULT_MANIFEST);
        std::fs::write(&p, serde_json::to_string_pretty(&m).unwrap()).unwrap();
        p
    }

    #[test]
    fn manifest_validation_reports_every_problem() {
        let bad = |v: Value| Manifest::parse(&v.to_string()).unwrap_err();
        assert!(bad(json!({"checks": []})).contains("checks is empty"));
        let e = bad(json!({"project": "p.json", "checks": [
            {"kind": "render", "name": "a b"},
            {"kind": "render", "name": "x", "scale": 0.0},
            {"kind": "render", "name": "x", "frames": [1], "step": 2},
            {"kind": "render", "name": "y", "thresholds": {"min_ssim": 2.0}},
            {"kind": "render", "name": "z", "renderer": "gpu"},
            {"kind": "render", "name": "w", "thresholds": {"metric": "lab"}},
            {"kind": "render", "name": "v", "ignore": [[0, 0, 0, 5]]},
            {"kind": "limits", "name": "l"},
            {"kind": "export-size", "name": "e", "format": "mp4", "max_bytes": 5},
            {"kind": "perf", "name": "p", "budget_ms": -1.0}
        ]}));
        for want in ["must be 1-64 characters", "scale must be in (0, 16]", "duplicate check name 'x'", "either 'frames' or 'step'", "min_ssim must be in -1..1", "unknown renderer 'gpu'", "unknown colour metric", "ignore rectangles", "at least one limit", "unknown format 'mp4'", "budget_ms must be"] {
            assert!(e.contains(want), "missing '{want}' in\n{e}");
        }
        assert!(bad(json!({"checks": [{"kind": "render", "name": "a"}]})).contains("no project"));
        assert!(bad(json!({"schema": "other/9", "project": "p", "checks": [{"kind": "validate", "name": "a"}]})).contains("schema must be"));
        assert!(bad(json!({"goldens": "../x", "project": "p", "checks": [{"kind": "validate", "name": "a"}]})).contains("goldens must be a relative path"));
        // unknown fields and kinds are errors, not silently ignored
        assert!(Manifest::parse(r#"{"project":"p","checks":[{"kind":"render","name":"a","scael":1}]}"#).is_err());
        assert!(Manifest::parse(r#"{"project":"p","checks":[{"kind":"teleport","name":"a"}]}"#).is_err());
        assert!(Manifest::parse("{not json").unwrap_err().starts_with("invalid manifest"));
        let ok = Manifest::parse(r#"{"project":"p.json","checks":[{"kind":"render","name":"a"}]}"#).unwrap();
        assert_eq!(ok.goldens, "goldens");
        assert_eq!(ok.schema, MANIFEST_SCHEMA);
        let again = Manifest::parse(&serde_json::to_string(&ok).unwrap()).unwrap();
        assert_eq!(ok, again, "manifest round-trips");
    }

    #[test]
    fn golden_update_then_compare_round_trip() {
        let dir = tmpdir("roundtrip");
        write_demo(&dir);
        let mp = small_manifest(&dir);
        // no goldens yet: render checks fail, nothing is written
        let r0 = run(&mp, &RunOptions::default()).unwrap();
        assert_eq!(r0.exit_code(), 1);
        assert!(r0.checks.iter().find(|c| c.name == "frames").unwrap().frames[0].message.contains("no golden"));
        assert!(!dir.join("goldens").exists());
        // update writes content-hash named files and an index
        let r1 = run(&mp, &RunOptions { update: true, ..Default::default() }).unwrap();
        assert_eq!(r1.exit_code(), 0, "{}", r1.to_json());
        assert_eq!(r1.mode, "update");
        let idx: GoldenIndex = serde_json::from_str(&std::fs::read_to_string(dir.join("goldens/index.json")).unwrap()).unwrap();
        assert_eq!(idx.entries["frames"].len(), 2);
        let g = &idx.entries["frames"]["00006"];
        assert_eq!(g.hash.len(), 24);
        assert!(dir.join("goldens").join(format!("{}.png", g.hash)).exists());
        assert!(dir.join("goldens").join(format!("{}.svg", idx.entries["svg"]["00000"].hash)).exists());
        let before = std::fs::read(dir.join("goldens/index.json")).unwrap();
        // verify passes, exactly
        let r2 = run(&mp, &RunOptions::default()).unwrap();
        assert_eq!(r2.exit_code(), 0, "{}", r2.to_json());
        assert!(r2.checks.iter().find(|c| c.name == "frames").unwrap().frames.iter().all(|f| f.exact));
        // a second update changes nothing on disk
        let r3 = run(&mp, &RunOptions { update: true, ..Default::default() }).unwrap();
        assert_eq!(r3.summary.updated, 0);
        assert_eq!(std::fs::read(dir.join("goldens/index.json")).unwrap(), before);
        // --only
        let r4 = run(&mp, &RunOptions { only: vec!["frames".into()], ..Default::default() }).unwrap();
        assert_eq!(r4.checks.len(), 1);
        assert!(run(&mp, &RunOptions { only: vec!["nope".into()], ..Default::default() }).unwrap_err().contains("no check named 'nope'"));
        assert!(run(&mp, &RunOptions { update: true, bless: true, ..Default::default() }).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn move_a_layer(dir: &Path) {
        let p = dir.join("demo.json");
        let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        let layer = v["layers"].as_array_mut().unwrap().iter_mut().find(|l| l["name"] == "sun").expect("demo has a sun layer");
        let pos = layer["transform"]["position"].as_array().unwrap().clone();
        layer["transform"]["position"] = json!([pos[0].as_f64().unwrap() + 60.0, pos[1].as_f64().unwrap() + 30.0]);
        std::fs::write(&p, serde_json::to_string(&v).unwrap()).unwrap();
    }

    #[test]
    fn a_changed_project_fails_with_regions_and_bless_accepts_it() {
        let dir = tmpdir("regress");
        write_demo(&dir);
        let mp = small_manifest(&dir);
        run(&mp, &RunOptions { update: true, ..Default::default() }).unwrap();
        move_a_layer(&dir);
        let arts = dir.join("art");
        let r = run(&mp, &RunOptions { artifacts: Some(arts.clone()), ..Default::default() }).unwrap();
        assert_eq!(r.exit_code(), 1, "{}", r.to_json());
        let c = r.checks.iter().find(|c| c.name == "frames").unwrap();
        assert_eq!(c.status, Status::Fail);
        let f = c.frames.iter().find(|f| f.status == Status::Fail).unwrap();
        let m = f.metrics.as_ref().unwrap();
        assert!(m.diff_pixels > 0 && m.ssim < 1.0 && m.max_de > 1.0, "{m:?}");
        assert!(!f.regions.is_empty());
        assert!(f.images.golden.is_some() && f.images.actual.is_some() && f.images.heat.is_some());
        assert!(arts.join("frames").join(format!("f{:05}-diff.png", f.frame)).exists());
        // the other checks are unaffected by the visual change
        assert_eq!(r.checks.iter().find(|c| c.name == "det").unwrap().status, Status::Pass);
        // bless accepts exactly the failing frames and then verify passes
        let b = run(&mp, &RunOptions { bless: true, ..Default::default() }).unwrap();
        assert_eq!(b.exit_code(), 0);
        assert!(b.summary.blessed >= 1);
        assert!(run(&mp, &RunOptions::default()).unwrap().exit_code() == 0);
        // prune removes the now unreferenced old goldens
        let n_before = std::fs::read_dir(dir.join("goldens")).unwrap().count();
        run(&mp, &RunOptions { prune: true, ..Default::default() }).unwrap();
        let n_after = std::fs::read_dir(dir.join("goldens")).unwrap().count();
        assert!(n_after < n_before, "{n_after} < {n_before}");
        assert!(run(&mp, &RunOptions::default()).unwrap().exit_code() == 0, "prune kept every referenced golden");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tampered_goldens_and_broken_inputs_are_detected() {
        let dir = tmpdir("tamper");
        write_demo(&dir);
        let mp = small_manifest(&dir);
        run(&mp, &RunOptions { update: true, ..Default::default() }).unwrap();
        // overwrite one golden with other pixels: its name no longer matches
        let idx: GoldenIndex = serde_json::from_str(&std::fs::read_to_string(dir.join("goldens/index.json")).unwrap()).unwrap();
        let g = &idx.entries["frames"]["00006"];
        let other = Img::solid(10, 10, [1, 2, 3, 255]);
        std::fs::write(dir.join("goldens").join(format!("{}.png", g.hash)), other.to_png().unwrap()).unwrap();
        // make the frame differ so the golden is actually read
        move_a_layer(&dir);
        let r = run(&mp, &RunOptions::default()).unwrap();
        let f = r.checks.iter().find(|c| c.name == "frames").unwrap().frames.iter().find(|f| f.frame == 6).unwrap();
        assert_eq!(f.status, Status::Error);
        assert!(f.message.contains("was modified"), "{}", f.message);
        assert_eq!(r.exit_code(), 2);
        // missing project: the check errors (exit 2), the run continues
        std::fs::remove_file(dir.join("demo.json")).unwrap();
        let r = run(&mp, &RunOptions::default()).unwrap();
        assert_eq!(r.exit_code(), 2);
        assert!(r.checks.iter().all(|c| c.status == Status::Error || c.status == Status::Fail));
        // missing manifest and a corrupt golden index are hard errors
        assert!(run(&dir.join("none.json"), &RunOptions::default()).unwrap_err().contains("cannot read manifest"));
        std::fs::write(dir.join("goldens/index.json"), "{\"schema\":\"x\"}").unwrap();
        assert!(run(&mp, &RunOptions::default()).unwrap_err().contains("schema"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn frames_outside_the_project_are_an_error_not_a_panic() {
        let dir = tmpdir("range");
        write_demo(&dir);
        let m = json!({"project": "demo.json", "checks": [{"kind": "render", "name": "far", "frames": [9999], "scale": 0.1}]});
        let mp = dir.join(DEFAULT_MANIFEST);
        std::fs::write(&mp, m.to_string()).unwrap();
        let r = run(&mp, &RunOptions::default()).unwrap();
        assert_eq!(r.exit_code(), 2);
        assert!(r.checks[0].message.contains("outside the project"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn size_change_is_reported_as_a_failure() {
        let dir = tmpdir("size");
        write_demo(&dir);
        let mk = |scale: f32| {
            let m = json!({"project": "demo.json", "checks": [{"kind": "render", "name": "s", "frames": [0], "scale": scale}]});
            let p = dir.join(DEFAULT_MANIFEST);
            std::fs::write(&p, m.to_string()).unwrap();
            p
        };
        run(&mk(0.1), &RunOptions { update: true, ..Default::default() }).unwrap();
        let r = run(&mk(0.2), &RunOptions::default()).unwrap();
        assert_eq!(r.exit_code(), 1);
        assert!(r.checks[0].frames[0].message.contains("size changed"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn thresholds_decide_between_pass_and_fail() {
        let dir = tmpdir("thresh");
        write_demo(&dir);
        let mk = |t: Value| {
            let m = json!({"project": "demo.json", "checks": [{"kind": "render", "name": "s", "frames": [0], "scale": 0.1, "thresholds": t}]});
            let p = dir.join(DEFAULT_MANIFEST);
            std::fs::write(&p, m.to_string()).unwrap();
            p
        };
        run(&mk(json!({})), &RunOptions { update: true, ..Default::default() }).unwrap();
        // move the sun by a few stage units: a small local change
        let p = dir.join("demo.json");
        let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        let sun = v["layers"].as_array_mut().unwrap().iter_mut().find(|l| l["name"] == "sun").unwrap();
        let pos = sun["transform"]["position"].as_array().unwrap().clone();
        sun["transform"]["position"] = json!([pos[0].as_f64().unwrap() + 3.0, pos[1].as_f64().unwrap()]);
        std::fs::write(&p, v.to_string()).unwrap();
        let strict = run(&mk(json!({"pixel_de": 0.0, "max_de": 0.0})), &RunOptions::default()).unwrap();
        assert_eq!(strict.exit_code(), 1);
        let lax = run(&mk(json!({"pixel_de": 50.0, "max_de": 50.0, "min_ssim": 0.5, "max_diff_fraction": 1.0})), &RunOptions::default()).unwrap();
        assert_eq!(lax.exit_code(), 0, "{}", lax.to_json());
        assert!(!lax.checks[0].frames[0].exact);
        assert!(lax.checks[0].frames[0].metrics.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ignore_regions_hide_a_change_in_a_render_check() {
        let dir = tmpdir("mask");
        write_demo(&dir);
        let mk = |ignore: Value| {
            let m = json!({"project": "demo.json", "checks": [{"kind": "render", "name": "s", "frames": [0], "scale": 0.5, "ignore": ignore}]});
            let p = dir.join(DEFAULT_MANIFEST);
            std::fs::write(&p, m.to_string()).unwrap();
            p
        };
        let e = crate::engine::Engine::new(demo::project()).unwrap();
        let (w, h) = (e.project.width, e.project.height);
        run(&mk(json!([])), &RunOptions { update: true, ..Default::default() }).unwrap();
        move_a_layer(&dir);
        assert_eq!(run(&mk(json!([])), &RunOptions::default()).unwrap().exit_code(), 1);
        let r = run(&mk(json!([[0.0, 0.0, w, h]])), &RunOptions::default()).unwrap();
        assert_eq!(r.exit_code(), 0, "{}", r.to_json());
        assert_eq!(r.checks[0].frames[0].metrics.as_ref().unwrap().diff_pixels, 0);
        assert!(r.checks[0].frames[0].metrics.as_ref().unwrap().ignored_pixels > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn structural_checks_pass_and_fail_for_the_right_reasons() {
        let dir = tmpdir("struct");
        write_demo(&dir);
        let m = json!({"project": "demo.json", "checks": [
            {"kind": "limits", "name": "tight", "max_layers": 1},
            {"kind": "limits", "name": "roomy", "max_layers": 1000, "max_file_bytes": 10000000},
            {"kind": "export-size", "name": "tiny", "format": "gif", "max_bytes": 10, "scale": 0.1},
            {"kind": "export-size", "name": "lottie", "format": "lottie", "max_bytes": 10000000},
            {"kind": "export-size", "name": "html", "format": "html", "max_bytes": 10000000},
            {"kind": "export-size", "name": "apng", "format": "apng", "max_bytes": 10000000, "scale": 0.1},
            {"kind": "export-size", "name": "svgs", "format": "svg-frames", "max_bytes": 10000000},
            {"kind": "perf", "name": "slow-info", "budget_ms": 0.0001, "tolerance": 0.0, "scale": 0.1, "frames": [0]},
            {"kind": "perf", "name": "slow-enforced", "budget_ms": 0.0001, "tolerance": 0.0, "scale": 0.1, "frames": [0], "enforce": true},
            {"kind": "perf", "name": "fast-enforced", "budget_ms": 100000.0, "scale": 0.1, "frames": [0], "enforce": true},
            {"kind": "validate", "name": "valid", "lint": "off"},
            {"kind": "fuzz", "name": "fuzz", "samples": 20, "max_fail_rate": 1.0}
        ]});
        let mp = dir.join(DEFAULT_MANIFEST);
        std::fs::write(&mp, m.to_string()).unwrap();
        let r = run(&mp, &RunOptions::default()).unwrap();
        let st = |n: &str| r.checks.iter().find(|c| c.name == n).unwrap().status;
        assert_eq!(st("tight"), Status::Fail);
        assert!(r.checks.iter().find(|c| c.name == "tight").unwrap().message.contains("layers"));
        assert_eq!(st("roomy"), Status::Pass);
        assert_eq!(st("tiny"), Status::Fail);
        for n in ["lottie", "html", "apng", "svgs", "fast-enforced", "valid"] {
            assert_eq!(st(n), Status::Pass, "{n}: {}", r.checks.iter().find(|c| c.name == n).unwrap().message);
        }
        assert_eq!(st("fuzz"), Status::Info, "a 100 % fuzz gate cannot fail: information, not a pass");
        assert_eq!(st("slow-info"), Status::Info, "performance is informational unless enforced");
        assert_eq!(st("slow-enforced"), Status::Fail);
        assert_eq!(r.exit_code(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fuzz_check_fails_when_the_rate_is_exceeded() {
        let dir = tmpdir("fuzz");
        // a project whose fuzzing is expected to produce some failures is hard to build; instead use rate 0 on a
        // project and an impossible negative-free bound: run twice with max_fail_rate 0 and 1 and compare semantics
        write_demo(&dir);
        let mk = |rate: f32| {
            let m = json!({"project": "demo.json", "checks": [{"kind": "fuzz", "name": "f", "samples": 30, "max_fail_rate": rate}]});
            let p = dir.join(DEFAULT_MANIFEST);
            std::fs::write(&p, m.to_string()).unwrap();
            p
        };
        // C12b: a gate at 100 % can never fail, so it is reported as information, never as a pass
        let loose = run(&mk(1.0), &RunOptions::default()).unwrap();
        assert_eq!(loose.checks[0].status, Status::Info, "{}", loose.checks[0].message);
        assert!(loose.checks[0].message.contains("cannot fail"));
        assert_eq!(loose.exit_code(), 0);
        let rate = loose.checks[0].details["failure_rate"].as_f64().unwrap();
        let tight = run(&mk(0.0), &RunOptions::default()).unwrap();
        assert_eq!(tight.checks[0].status, if rate > 0.0 { Status::Fail } else { Status::Pass });
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Documented manifest semantics of a `fuzz` check (docs/CI.md): it GATES by default (`gate` unset = true) with
    /// `max_fail_rate` defaulting to 0; only `"gate": false` or `max_fail_rate >= 1` turn it into information.
    #[test]
    fn fuzz_check_gates_by_default_and_gate_false_only_informs() {
        let dir = tmpdir("fuzzgate");
        write_demo(&dir);
        let run_with = |extra: serde_json::Value| {
            let mut c = json!({"kind": "fuzz", "name": "f", "samples": 30});
            for (k, v) in extra.as_object().unwrap() {
                c[k] = v.clone();
            }
            let p = dir.join(DEFAULT_MANIFEST);
            std::fs::write(&p, json!({"project": "demo.json", "checks": [c]}).to_string()).unwrap();
            run(&p, &RunOptions::default()).unwrap()
        };
        // neither `gate` nor `max_fail_rate` given: a gate with the default rate 0
        let dflt = run_with(json!({}));
        let rate = dflt.checks[0].details["failure_rate"].as_f64().unwrap();
        assert_eq!(dflt.checks[0].details["max_fail_rate"].as_f64(), Some(0.0));
        if rate > 0.0 {
            assert_eq!(dflt.checks[0].status, Status::Fail, "{}", dflt.checks[0].message);
            assert_eq!(dflt.exit_code(), 1);
        } else {
            assert_eq!(dflt.checks[0].status, Status::Pass);
        }
        // `gate: true` is the same as the default
        assert_eq!(run_with(json!({"gate": true})).checks[0].status, dflt.checks[0].status);
        // `gate: false`: reported, never failing, never a pass
        let off = run_with(json!({"gate": false}));
        assert_eq!(off.checks[0].status, Status::Info);
        assert!(off.checks[0].message.contains("gate = false"), "{}", off.checks[0].message);
        assert_eq!(off.exit_code(), 0);
        // a rate bound at or above the measured failure rate passes
        if rate < 0.98 {
            let ok = run_with(json!({"max_fail_rate": rate + 0.01}));
            assert_eq!(ok.checks[0].status, Status::Pass, "{}", ok.checks[0].message);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// C12b: `init` on a project that already fails most poses writes an information-only fuzz check.
    #[test]
    fn init_writes_a_non_gating_fuzz_check_when_most_poses_fail() {
        let dir = tmpdir("initfuzz");
        write_demo(&dir);
        let m = init_manifest(&dir.join("demo.json"), &dir.join(DEFAULT_MANIFEST)).unwrap();
        let f = m.checks.iter().find_map(|c| if let Check::Fuzz(f) = c { Some(f.clone()) } else { None }).expect("fuzz check");
        let e = Engine::new(demo::project()).unwrap();
        let rate = crate::fuzz::run(&e.project, None, &crate::fuzz::FuzzOptions { samples: 100, seed: 1, ..Default::default() }).unwrap().summary.failure_rate;
        if rate > 0.5 {
            assert_eq!(f.gate, Some(false), "rate {rate}");
        } else {
            assert!(f.max_fail_rate.unwrap() < 1.0 && f.gate.is_none(), "rate {rate}");
        }
        // whichever it is, the written manifest can never end in a PASS that cannot fail
        std::fs::write(dir.join(DEFAULT_MANIFEST), serde_json::to_string(&m).unwrap()).unwrap();
        let r = run(&dir.join(DEFAULT_MANIFEST), &RunOptions { only: vec!["pose-fuzz".into()], ..Default::default() }).unwrap();
        let st = r.checks.iter().find(|c| c.name == "pose-fuzz").unwrap().status;
        assert!(st == Status::Info || st == Status::Pass);
        if f.max_fail_rate.unwrap() >= 1.0 {
            assert_eq!(st, Status::Info);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn loop_check_separates_a_loop_from_a_one_way_motion() {
        let dir = tmpdir("loop");
        let mut p = demo::project();
        // a static scene always loops
        p.channels.clear();
        let path = dir.join("static.json");
        std::fs::write(&path, serde_json::to_string(&p).unwrap()).unwrap();
        let m = json!({"project": "static.json", "checks": [{"kind": "loop", "name": "l"}]});
        let mp = dir.join(DEFAULT_MANIFEST);
        std::fs::write(&mp, m.to_string()).unwrap();
        assert_eq!(run(&mp, &RunOptions::default()).unwrap().checks[0].status, Status::Pass);
        // statistics on the demo: the seam is a number, the check is deterministic
        let e = Engine::new(demo::project()).unwrap();
        let a = loop_stats(&e, 0.1).unwrap();
        let b = loop_stats(&e, 0.1).unwrap();
        assert_eq!(a, b);
        assert!(a.0 >= 0.0 && a.1 >= 0.0 && a.2 >= a.1);
        // a one-frame project cannot loop-check
        let mut one = demo::project();
        one.frames = 1;
        assert!(loop_stats(&Engine::new(one).unwrap(), 0.1).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn init_creates_a_manifest_that_passes_on_the_project_it_was_made_from() {
        let dir = tmpdir("init");
        let proj = write_demo(&dir);
        // a short, small copy keeps this test fast in debug builds
        let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&proj).unwrap()).unwrap();
        v["frames"] = json!(8);
        v["width"] = json!(160.0);
        v["height"] = json!(120.0);
        std::fs::write(&proj, v.to_string()).unwrap();
        let mp = dir.join(DEFAULT_MANIFEST);
        let m = init_manifest(&proj, &mp).unwrap();
        assert_eq!(m.project.as_deref(), Some("demo.json"));
        let kinds: Vec<&str> = m.checks.iter().map(|c| c.kind()).collect();
        for k in ["validate", "render", "determinism", "fuzz", "limits", "export-size", "perf"] {
            assert!(kinds.contains(&k), "init should add a {k} check: {kinds:?}");
        }
        std::fs::write(&mp, serde_json::to_string_pretty(&m).unwrap()).unwrap();
        run(&mp, &RunOptions { update: true, ..Default::default() }).unwrap();
        let r = run(&mp, &RunOptions::default()).unwrap();
        assert_eq!(r.exit_code(), 0, "{}", r.to_json());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn runs_are_deterministic_apart_from_timings() {
        let dir = tmpdir("repeat");
        write_demo(&dir);
        let mp = small_manifest(&dir);
        run(&mp, &RunOptions { update: true, ..Default::default() }).unwrap();
        let strip = |r: &CiReport| -> Value {
            let mut v = serde_json::to_value(r).unwrap();
            v["duration_ms"] = json!(0);
            for c in v["checks"].as_array_mut().unwrap() {
                c["duration_ms"] = json!(0);
                if c["kind"] == "perf" {
                    c["details"] = json!(null);
                    c["message"] = json!("");
                }
            }
            v
        };
        let a = run(&mp, &RunOptions::default()).unwrap();
        let b = run(&mp, &RunOptions::default()).unwrap();
        assert_eq!(strip(&a), strip(&b));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn failing_report() -> (crate::fsutil::ScratchDir, CiReport) {
        let dir = tmpdir("report");
        write_demo(&dir);
        let mp = small_manifest(&dir);
        run(&mp, &RunOptions { update: true, ..Default::default() }).unwrap();
        move_a_layer(&dir);
        let r = run(&mp, &RunOptions::default()).unwrap();
        (dir, r)
    }

    #[test]
    fn junit_xml_is_well_formed_and_counts_match() {
        let (dir, mut r) = failing_report();
        // add awkward characters to the messages
        r.checks[0].message = "a <b> & \"c\" 'd' \u{1}".into();
        let x = junit_xml(&r);
        assert!(x.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        // minimal well-formedness check with a tiny tag stack
        let mut stack: Vec<String> = Vec::new();
        let mut i = 0;
        let b = x.as_bytes();
        while i < b.len() {
            if b[i] == b'<' {
                let j = x[i..].find('>').unwrap() + i;
                let tag = &x[i + 1..j];
                if tag.starts_with('?') {
                } else if let Some(name) = tag.strip_prefix('/') {
                    assert_eq!(stack.pop().as_deref(), Some(name), "closing tag mismatch");
                } else if !tag.ends_with('/') {
                    stack.push(tag.split_whitespace().next().unwrap().to_string());
                }
                i = j;
            } else if b[i] == b'&' {
                let j = x[i..].find(';').expect("entity must end") + i;
                assert!(["&amp", "&lt", "&gt", "&quot", "&apos"].contains(&&x[i..j]), "bad entity {}", &x[i..=j]);
            }
            i += 1;
        }
        assert!(stack.is_empty());
        assert!(!x.contains('\u{1}'));
        assert_eq!(x.matches("<testcase ").count(), r.checks.len());
        assert_eq!(x.matches("<failure ").count(), r.summary.failed);
        assert!(x.contains("<testsuites name=\"osatura-ci\"") && x.contains(&format!("failures=\"{}\"", r.summary.failed)));
        assert!(x.contains("frame 0") || x.contains("frame 6"), "failure text names the frame");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn html_report_is_self_contained_and_shows_failures() {
        let (dir, r) = failing_report();
        let h = html_report(&r);
        assert!(h.starts_with("<!doctype html>"));
        assert!(h.contains("data:image/png;base64,"), "images are inlined");
        assert!(h.matches("data:image/png;base64,").count() >= 3, "golden, current and difference");
        for bad in ["src=\"http", "href=\"http", "url(http", "<link ", "<script src", "@import"] {
            assert!(!h.contains(bad), "external reference '{bad}'");
        }
        assert!(h.contains("Osatura CI report") && h.contains("exit code 1"));
        assert!(h.contains("frames") && h.contains("class=\"b fail\""));
        assert!(h.contains("<table") && h.contains("Region"));
        assert!(h.len() < 16_000_000);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exit_codes_follow_the_worst_result() {
        let mk = |s: &[Status]| {
            let checks: Vec<CheckResult> = s.iter().enumerate().map(|(i, &st)| CheckResult { name: format!("c{i}"), kind: "limits".into(), status: st, message: String::new(), duration_ms: 0.0, frames: vec![], details: Value::Null }).collect();
            CiReport { schema: REPORT_SCHEMA.into(), manifest: "m".into(), mode: "verify".into(), duration_ms: 0.0, summary: CiReport::summarize(&checks), checks }
        };
        assert_eq!(mk(&[Status::Pass, Status::Info, Status::Updated, Status::Blessed]).exit_code(), 0);
        assert_eq!(mk(&[Status::Pass, Status::Fail]).exit_code(), 1);
        assert_eq!(mk(&[Status::Fail, Status::Error]).exit_code(), 2);
        assert_eq!(mk(&[]).exit_code(), 0);
        let j: Value = serde_json::from_str(&mk(&[Status::Fail]).to_json()).unwrap();
        assert_eq!(j["schema"], REPORT_SCHEMA);
        assert_eq!(j["summary"]["failed"], 1);
    }

    #[test]
    fn float_renderer_goldens_work_like_raster_ones() {
        let dir = tmpdir("float");
        write_demo(&dir);
        let m = json!({"project": "demo.json", "checks": [{"kind": "render", "name": "fl", "frames": [3], "scale": 0.1, "renderer": "float"}]});
        let mp = dir.join(DEFAULT_MANIFEST);
        std::fs::write(&mp, m.to_string()).unwrap();
        run(&mp, &RunOptions { update: true, ..Default::default() }).unwrap();
        assert_eq!(run(&mp, &RunOptions::default()).unwrap().exit_code(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
