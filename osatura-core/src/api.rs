// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
//! The supported, stable public API of `osatura-core`.
//!
//! Only this module and [`crate::prelude`] are covered by the stability promise below. Every other
//! module of the crate (and the root-level re-exports) is an *internal, unstable* implementation
//! detail used by the CLI, the studio and the C ABI; it may change in any release and is hidden
//! from the generated documentation.
//!
//! # Stability policy
//!
//! [`API_VERSION`] follows semantic versioning for this facade only:
//!
//! * **patch** - bug fixes, no signature changes;
//! * **minor** - additions only: new functions, methods, enum variants and struct fields. Public
//!   enums and structs of the facade are `#[non_exhaustive]`, so downstream code must use a
//!   wildcard match arm and cannot build them with a struct literal (use the constructors);
//! * **major** - anything removed or changed incompatibly. The checked-in `api_surface.txt` lists
//!   every public item of this file and a test fails whenever the list changes, so each change to
//!   the surface is a deliberate, reviewed edit.
//!
//! Errors are values: every fallible function returns [`ApiError`], bad input never panics, and a
//! panic inside the engine is caught and reported as [`ApiError::Internal`].
//!
//! # Example
//!
//! ```
//! use osatura_core::prelude::*;
//!
//! # fn main() -> Result<(), ApiError> {
//! // A project is plain JSON. `Animation::demo()` returns a small built-in one.
//! let json = Animation::demo().to_json();
//! let mut anim = Animation::from_json(&json)?;
//! let info = anim.info();
//! assert!(info.frames > 0 && info.fps > 0.0);
//!
//! // Query the rig.
//! let first_bone = anim.bone_names()[0].clone();
//! let pose = anim.bone_world(0.0)?; // 6 floats per bone: a b c d tx ty
//! assert_eq!(pose.len(), anim.bone_names().len());
//!
//! // Edit through the undoable editor (validated; locks are respected).
//! anim.set_key(&first_bone, "bone_angle", 4.0, 30.0, Interpolation::EaseInOut)?;
//! assert!(anim.undo());
//!
//! // Render.
//! let image = anim.render_rgba(0.0, &RenderSettings::default())?;
//! assert_eq!(image.pixels.len(), image.width as usize * image.height as usize * 4);
//! let svg = anim.render_svg(0.0, false)?;
//! assert!(svg.starts_with("<svg"));
//! let png = anim.render_png(0.0, &RenderSettings::default())?;
//! assert_eq!(&png[1..4], b"PNG");
//!
//! // Bad input is an error value, never a panic.
//! assert!(matches!(Animation::from_json("{ nope"), Err(ApiError::InvalidJson(_))));
//! assert!(anim.render_rgba(f32::NAN, &RenderSettings::default()).is_err());
//! # Ok(()) }
//! ```

use crate::anim::Ease;
use crate::edit::Editor;
use crate::project::{Prop, Project};
use crate::raster::{render_png, render_rgba, RasterOptions};
use crate::svg::{render_frame, RenderOptions};
use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};

/// Version of this facade (semver, see the module docs). Independent of the crate version.
pub const API_VERSION: &str = "1.4.0";
/// Major component of [`API_VERSION`]; bumped only on incompatible changes.
pub const API_VERSION_MAJOR: u32 = 1;
/// Largest accepted output side in pixels (guards against absurd allocations).
pub const MAX_RENDER_SIDE: u32 = 16384;
/// Largest accepted project JSON in bytes.
pub const MAX_JSON_BYTES: usize = 256 * 1024 * 1024;
/// Level-of-detail grid used by `Player::set_screen_size`: level 1 is bounded by this many stage
/// units, each further level by twice the previous one.
const LOD_TOLERANCE: f64 = 0.125;
const LOD_LEVELS: usize = 8;

/// Every error the facade can return.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ApiError {
    /// The text is not valid JSON or does not describe a project (or is too large / too new).
    InvalidJson(String),
    /// The project parsed but failed validation; one message per problem.
    InvalidProject(Vec<String>),
    /// A caller argument was rejected (non-finite number, unknown property, scale out of range...).
    InvalidArgument(String),
    /// An edit was rejected (for example a locked channel); the project is unchanged.
    EditRejected(Vec<String>),
    /// An encoder failed or the format is not supported for this project.
    Export(String),
    /// An unexpected internal failure (a caught panic). Please report it as a bug.
    Internal(String),
    /// (1.3) The project file has uncommitted changes and the operation would overwrite them.
    DirtyTree(String),
    /// (1.3) A merge, cherry-pick or revert is still in progress: resolve it, commit or abort first.
    MergeInProgress(String),
    /// (1.3) Conflicts are still unresolved; one pointer per conflict (see [`vcs_conflicts`]).
    Conflicts(Vec<String>),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::InvalidJson(m) => write!(f, "invalid project JSON: {m}"),
            ApiError::InvalidProject(v) => write!(f, "invalid project: {}", v.join("; ")),
            ApiError::InvalidArgument(m) => write!(f, "invalid argument: {m}"),
            ApiError::EditRejected(v) => write!(f, "edit rejected: {}", v.join("; ")),
            ApiError::Export(m) => write!(f, "export failed: {m}"),
            ApiError::Internal(m) => write!(f, "internal error: {m}"),
            ApiError::DirtyTree(m) => write!(f, "uncommitted changes: {m}"),
            ApiError::MergeInProgress(m) => write!(f, "merge in progress: {m}"),
            ApiError::Conflicts(v) => write!(f, "unresolved conflicts: {}", v.join(", ")),
        }
    }
}

impl std::error::Error for ApiError {}

/// Interpolation of the segment that starts at a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Interpolation {
    Linear,
    /// Hold the value until the next key.
    Step,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// Catmull-Rom spline through neighbouring keys.
    Smooth,
}

impl Interpolation {
    fn ease(self) -> Ease {
        match self {
            Interpolation::Linear => Ease::Linear,
            Interpolation::Step => Ease::Step,
            Interpolation::EaseIn => Ease::EaseIn,
            Interpolation::EaseOut => Ease::EaseOut,
            Interpolation::EaseInOut => Ease::EaseInOut,
            Interpolation::Smooth => Ease::Smooth,
        }
    }
}

/// Basic facts about a loaded animation.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct AnimationInfo {
    pub name: String,
    /// Stage size in stage units.
    pub width: f32,
    pub height: f32,
    pub fps: f32,
    /// Number of frames; valid frame positions run `0..frames`.
    pub frames: u32,
}

/// Options for raster rendering.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct RenderSettings {
    /// Output pixels per stage unit, within 0.05..=4.
    pub scale: f32,
    /// Draw the skeleton overlay.
    pub show_bones: bool,
    /// Leave the background transparent.
    pub transparent: bool,
}

impl RenderSettings {
    /// Settings with the given scale (other options off). Use this and the `with_*` methods:
    /// the struct is `#[non_exhaustive]`, so it cannot be built with a struct literal.
    pub fn new(scale: f32) -> RenderSettings {
        RenderSettings { scale, show_bones: false, transparent: false }
    }
    pub fn with_bones(mut self, show_bones: bool) -> RenderSettings {
        self.show_bones = show_bones;
        self
    }
    pub fn with_transparent(mut self, transparent: bool) -> RenderSettings {
        self.transparent = transparent;
        self
    }
}

impl Default for RenderSettings {
    fn default() -> Self {
        RenderSettings { scale: 1.0, show_bones: false, transparent: false }
    }
}

/// A rendered picture: straight-alpha RGBA8, row-major, top row first.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Whole-animation export targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExportFormat {
    /// Looping GIF of the whole timeline.
    Gif,
    /// Binary glTF 2.0.
    Glb,
    /// Lottie-style JSON document.
    Lottie,
    /// One self-contained HTML player page.
    Html,
    /// (1.4) Looping truecolour APNG of the whole timeline (`scale` within 0.05..=4).
    Apng,
}

// ---------------------------------------------------------------------------------------------
// Tools (1.2): level of detail, size-targeted export, mocap retarget, tracing, fuzzing, CI, history
// ---------------------------------------------------------------------------------------------

/// One derived level of detail (see [`Animation::lod_levels`]).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct LodLevel {
    /// 0 is the original, 1.. are the reduced levels.
    pub level: usize,
    /// Declared error bound in output pixels at the render scale the levels were built for.
    pub declared_px: f64,
    /// The same bound in stage units; multiply by a display scale to get pixels there.
    pub bound_stage: f64,
    /// Path anchors, layers and keys that remain.
    pub path_anchors: usize,
    pub keys: usize,
    pub layers: usize,
    /// The level as project JSON (load it with [`Animation::from_json`]).
    pub json: String,
}

impl LodLevel {
    /// Declared bound in pixels when shown at `screen_scale` output pixels per stage unit.
    pub fn bound_at(&self, screen_scale: f64) -> f64 {
        self.bound_stage * screen_scale
    }
}

/// Target format of [`Animation::fit_to_size`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FitFormat {
    Lottie,
    Html,
    Gif,
    Apng,
    /// OSATURA project JSON (keys are reduced).
    Project,
}

/// Result of [`Animation::fit_to_size`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct SizeFit {
    /// Whether the budget was met (when not, `bytes` is the smallest file found).
    pub met: bool,
    pub size: usize,
    pub bytes: Vec<u8>,
    /// Lowest structural similarity (0..1) of the sampled frames against the original render.
    pub ssim_min: f64,
    /// Why the budget could not be met, if it could not.
    pub reason: Option<String>,
}

/// Options for [`Animation::retarget_bvh`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MocapSettings {
    /// Drop horizontal root travel (keep the bob).
    pub in_place: bool,
    /// Flip the figure left-right.
    pub mirror: bool,
    /// Moving-average radius in output frames (0 = off).
    pub smooth: usize,
}

impl MocapSettings {
    pub fn new() -> MocapSettings {
        MocapSettings { in_place: false, mirror: false, smooth: 0 }
    }
    pub fn with_in_place(mut self, in_place: bool) -> MocapSettings {
        self.in_place = in_place;
        self
    }
    pub fn with_mirror(mut self, mirror: bool) -> MocapSettings {
        self.mirror = mirror;
        self
    }
    pub fn with_smooth(mut self, smooth: usize) -> MocapSettings {
        self.smooth = smooth;
        self
    }
}

impl Default for MocapSettings {
    fn default() -> Self {
        MocapSettings::new()
    }
}

/// What [`Animation::retarget_bvh`] did.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MocapReport {
    /// `(BVH joint, rig bone)` pairs that were matched.
    pub matched: Vec<(String, String)>,
    pub unmatched_joints: Vec<String>,
    pub unmatched_bones: Vec<String>,
    /// Frames of animation written and bones driven.
    pub frames: usize,
    pub bones_used: usize,
    /// `target.prop` entries not written because the channel is locked.
    pub blocked: Vec<String>,
    pub notes: Vec<String>,
}

/// Summary of [`Animation::fuzz`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct FuzzSummary {
    pub samples: usize,
    pub failures: usize,
    /// 0..1
    pub failure_rate: f32,
    pub max_severity: f32,
    /// The complete report as JSON.
    pub json: String,
}

/// One commit of a project history (see [`vcs_log`]).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct VcsCommit {
    pub id: String,
    pub author: String,
    /// Seconds since the Unix epoch.
    pub time: u64,
    pub message: String,
}

/// Result of [`Animation::bake_physics`] (1.4).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct PhysicsBake {
    /// Channels written.
    pub added: usize,
    /// Bones whose existing keyed angles were kept (`overwrite = false`) or whose channel is locked.
    pub skipped: Vec<String>,
}

/// One branch (see [`vcs_branches`], 1.4).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct VcsBranch {
    pub name: String,
    /// Commit id the branch points to.
    pub commit: String,
    /// Whether it is the checked-out branch.
    pub current: bool,
}

/// One semantic change between two states of a project (see [`vcs_diff`], 1.4).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct VcsChange {
    /// `"Added"`, `"Removed"`, `"Changed"`, `"Moved"` or `"Retimed"`.
    pub kind: String,
    pub path: String,
    pub description: String,
}

/// Working-tree state (see [`vcs_status`], 1.4).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct VcsStatus {
    pub branch: Option<String>,
    pub head: Option<String>,
    /// Uncommitted changes against HEAD (everything when there is no commit yet).
    pub changes: Vec<VcsChange>,
    /// `"merge"`, `"cherry-pick"` or `"revert"` when one is unfinished.
    pub merge: Option<String>,
    /// Paths of unresolved conflicts.
    pub conflicts: Vec<String>,
}

/// One commit with what it changed (see [`vcs_show`], 1.4).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct VcsShow {
    pub commit: VcsCommit,
    pub changes: Vec<VcsChange>,
}

/// Result of [`vcs_gc`] (1.4).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct VcsGc {
    pub kept: usize,
    pub removed: usize,
}

/// Result of [`ci_run`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct CiOutcome {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    /// Checks that could not be evaluated.
    pub errors: usize,
    /// 0 all good, 1 a check failed, 2 a check could not be evaluated.
    pub exit_code: i32,
    /// The complete report as JSON.
    pub json: String,
    /// (1.3) Checks whose goldens were rewritten (`CiMode::Update`).
    pub updated: usize,
    /// (1.3) Checks whose failing or missing goldens were accepted (`CiMode::Bless`).
    pub blessed: usize,
}

/// Trace a bitmap (PNG or JPEG bytes) into an SVG document of filled vector regions with at most
/// `colors` (2..=64) colours.
///
/// ```
/// use osatura_core::prelude::*;
/// # fn main() -> Result<(), ApiError> {
/// let png = Animation::demo().render_png(0.0, &RenderSettings::new(0.25))?;
/// let svg = trace_image_svg(&png, 6)?;
/// assert!(svg.starts_with("<svg") || svg.contains("<svg"));
/// assert!(trace_image_svg(b"not an image", 6).is_err());
/// # Ok(()) }
/// ```
pub fn trace_image_svg(image: &[u8], colors: usize) -> Result<String, ApiError> {
    guard(|| {
        let res = trace_bytes(image, colors)?;
        Ok(crate::autotrace::to_svg(&res))
    })
}

fn trace_bytes(image: &[u8], colors: usize) -> Result<crate::autotrace::TraceResult, ApiError> {
    if !(2..=64).contains(&colors) {
        return Err(ApiError::InvalidArgument("colors must be within 2..=64".into()));
    }
    let img = crate::autotrace::decode_image(image).map_err(ApiError::InvalidArgument)?;
    let o = crate::autotrace::TraceOptions { colors, ..Default::default() };
    o.validate().map_err(ApiError::InvalidArgument)?;
    crate::autotrace::trace(&img, &o).map_err(ApiError::InvalidArgument)
}

/// The commits of a project's version history, newest first (read-only). `path` is the project
/// file or a directory inside its repository; `limit` is within 1..=10000. An empty history gives
/// an empty list.
pub fn vcs_log(path: &str, limit: usize) -> Result<Vec<VcsCommit>, ApiError> {
    if !(1..=10_000).contains(&limit) {
        return Err(ApiError::InvalidArgument("limit must be within 1..=10000".into()));
    }
    guard(|| {
        let repo = crate::vcs::Repo::open(std::path::Path::new(path)).map_err(ApiError::InvalidArgument)?;
        if repo.head_commit().map_err(ApiError::InvalidArgument)?.is_none() {
            return Ok(Vec::new());
        }
        let log = repo.log("HEAD", limit).map_err(ApiError::InvalidArgument)?;
        Ok(log.into_iter().map(|(id, c)| VcsCommit { id, author: c.author, time: c.time, message: c.message }).collect())
    })
}

/// Run an animation-CI manifest in verify mode (nothing is re-recorded). Relative paths in the
/// manifest resolve next to it. A failing check is not an error: look at [`CiOutcome::exit_code`].
pub fn ci_run(manifest_path: &str) -> Result<CiOutcome, ApiError> {
    guard(|| {
        let r = crate::ci::run(std::path::Path::new(manifest_path), &crate::ci::RunOptions::default()).map_err(ApiError::InvalidArgument)?;
        Ok(ci_outcome(&r))
    })
}

fn ci_outcome(r: &crate::ci::CiReport) -> CiOutcome {
    CiOutcome {
        total: r.summary.total,
        passed: r.summary.passed,
        failed: r.summary.failed,
        errors: r.summary.errors,
        exit_code: r.exit_code(),
        json: r.to_json(),
        updated: r.summary.updated,
        blessed: r.summary.blessed,
    }
}

// ---------------------------------------------------------------------------------------------
// Version control (write operations), 1.3
// ---------------------------------------------------------------------------------------------

/// One unresolved conflict of a merge, cherry-pick or revert (see [`VcsMergeOutcome`]).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct VcsConflict {
    /// Stable identity pointer of the conflicting item; pass it to [`vcs_resolve`].
    pub path: String,
    /// `"BothChanged"`, `"DeleteEdit"`, `"KeyRetimed"`, `"OrderConflict"` or `"ArrayConflict"`
    /// (more may follow in later minor versions).
    pub kind: String,
    /// Human-readable one-line description.
    pub description: String,
    /// The value as JSON text in the common ancestor, in the current branch and in the other one;
    /// `None` when the item does not exist there.
    pub base: Option<String>,
    pub ours: Option<String>,
    pub theirs: Option<String>,
}

/// Result of [`vcs_merge`], [`vcs_cherry_pick`] and [`vcs_revert`]. Conflicts are a normal result,
/// not an error: the project file then holds the merged state with the conflicting items at their
/// current-branch value.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum VcsMergeOutcome {
    /// The other revision is already part of the current history; nothing changed.
    UpToDate,
    /// The branch simply moved forward to this commit.
    FastForward(String),
    /// A new commit with this id was created.
    Merged(String),
    /// Resolve every conflict with [`vcs_resolve`], then call [`vcs_commit`] (or [`vcs_abort`]).
    Conflicted(Vec<VcsConflict>),
}

/// How [`vcs_resolve`] settles one conflict.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum VcsChoice {
    /// Keep the current branch's version.
    Ours,
    /// Take the other revision's version.
    Theirs,
    /// Use this JSON text as the value.
    Value(String),
}

fn vcs_open(path: &str) -> Result<crate::vcs::Repo, ApiError> {
    crate::vcs::Repo::open(std::path::Path::new(path)).map_err(ApiError::InvalidArgument)
}

fn vcs_str<T>(r: Result<T, String>) -> Result<T, ApiError> {
    r.map_err(ApiError::InvalidArgument)
}

fn vcs_no_merge(repo: &crate::vcs::Repo) -> Result<(), ApiError> {
    match vcs_str(repo.merge_state())? {
        Some(st) => Err(ApiError::MergeInProgress(format!("a {} is unfinished ({} conflict(s) left); resolve and commit, or call vcs_abort", st.kind, st.conflicts.len()))),
        None => Ok(()),
    }
}

fn vcs_clean(repo: &crate::vcs::Repo) -> Result<(), ApiError> {
    if vcs_str(repo.head_commit())?.is_some() && vcs_str(repo.is_dirty())? {
        return Err(ApiError::DirtyTree("the project file differs from the last commit: commit it first".into()));
    }
    Ok(())
}

fn vcs_conflict(c: &crate::semdiff::Conflict) -> VcsConflict {
    let slot = |s: &crate::semdiff::Slot| match s {
        crate::semdiff::Slot::Value(v) => Some(v.to_string()),
        _ => None,
    };
    let kind = serde_json::to_value(c.kind).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_else(|| format!("{:?}", c.kind));
    VcsConflict { path: c.path.clone(), kind, description: c.description.clone(), base: slot(&c.base), ours: slot(&c.ours), theirs: slot(&c.theirs) }
}

fn vcs_outcome(o: crate::vcs::MergeOutcome) -> VcsMergeOutcome {
    match o {
        crate::vcs::MergeOutcome::UpToDate => VcsMergeOutcome::UpToDate,
        crate::vcs::MergeOutcome::FastForward(h) => VcsMergeOutcome::FastForward(h),
        crate::vcs::MergeOutcome::Merged(h) => VcsMergeOutcome::Merged(h),
        crate::vcs::MergeOutcome::Conflicted(cs) => VcsMergeOutcome::Conflicted(cs.iter().map(vcs_conflict).collect()),
    }
}

/// Start version control for the project file `project_file` (it must exist and be JSON): creates
/// the history next to it, on branch `main`, with no commit yet. `author` is the default author
/// of later commits. Fails if a history already exists there.
///
/// ```
/// use osatura_core::prelude::*;
/// # fn main() -> Result<(), ApiError> {
/// # let dir = std::env::temp_dir().join(format!("osatura-doc-init-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir).unwrap();
/// # let file = dir.join("p.json");
/// # std::fs::write(&file, Animation::demo().to_json()).unwrap();
/// let path = file.to_str().unwrap();
/// vcs_init(path, Some("ana"))?;
/// assert!(vcs_log(path, 5)?.is_empty());           // no commit yet
/// assert!(vcs_init(path, None).is_err());          // already initialised
/// # std::fs::remove_dir_all(&dir).unwrap();
/// # Ok(()) }
/// ```
pub fn vcs_init(project_file: &str, author: Option<&str>) -> Result<(), ApiError> {
    guard(|| vcs_str(crate::vcs::Repo::init(std::path::Path::new(project_file), author)).map(|_| ()))
}

/// Commit the project file on the current branch and return the new commit id. With a finished
/// merge in progress this creates the merge commit (the message then defaults to the merge's).
/// Errors: [`ApiError::Conflicts`] while conflicts are unresolved, [`ApiError::InvalidArgument`]
/// when there is nothing to commit or no message was given.
///
/// ```
/// use osatura_core::prelude::*;
/// # fn main() -> Result<(), ApiError> {
/// # let dir = std::env::temp_dir().join(format!("osatura-doc-commit-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir).unwrap();
/// # let file = dir.join("p.json");
/// # std::fs::write(&file, Animation::demo().to_json()).unwrap();
/// let path = file.to_str().unwrap();
/// vcs_init(path, Some("ana"))?;
/// let first = vcs_commit(path, Some("first"), None)?;
/// assert_eq!(vcs_log(path, 5)?[0].id, first);
/// assert!(vcs_commit(path, Some("again"), None).is_err()); // unchanged
/// # std::fs::remove_dir_all(&dir).unwrap();
/// # Ok(()) }
/// ```
pub fn vcs_commit(path: &str, message: Option<&str>, author: Option<&str>) -> Result<String, ApiError> {
    guard(|| {
        let repo = vcs_open(path)?;
        if let Some(st) = vcs_str(repo.merge_state())? {
            if !st.conflicts.is_empty() {
                return Err(ApiError::Conflicts(st.conflicts.iter().map(|c| c.path.clone()).collect()));
            }
        }
        vcs_str(repo.commit(message, author))
    })
}

/// Create branch `name` at revision `at` (a branch, tag or commit id; default: the current
/// commit) without switching to it. Returns the commit it points at.
///
/// ```
/// use osatura_core::prelude::*;
/// # fn main() -> Result<(), ApiError> {
/// # let dir = std::env::temp_dir().join(format!("osatura-doc-branch-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir).unwrap();
/// # let file = dir.join("p.json");
/// # std::fs::write(&file, Animation::demo().to_json()).unwrap();
/// let path = file.to_str().unwrap();
/// vcs_init(path, Some("ana"))?;
/// let c = vcs_commit(path, Some("first"), None)?;
/// assert_eq!(vcs_branch(path, "feature", None)?, c);
/// assert!(vcs_branch(path, "feature", None).is_err());   // exists
/// assert!(vcs_branch(path, "bad name", None).is_err());  // invalid name
/// # std::fs::remove_dir_all(&dir).unwrap();
/// # Ok(()) }
/// ```
pub fn vcs_branch(path: &str, name: &str, at: Option<&str>) -> Result<String, ApiError> {
    guard(|| vcs_str(vcs_open(path)?.branch_create(name, at)))
}

/// Tag a revision (default: the current commit) with `name`. Returns the commit id.
///
/// ```
/// use osatura_core::prelude::*;
/// # fn main() -> Result<(), ApiError> {
/// # let dir = std::env::temp_dir().join(format!("osatura-doc-tag-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir).unwrap();
/// # let file = dir.join("p.json");
/// # std::fs::write(&file, Animation::demo().to_json()).unwrap();
/// let path = file.to_str().unwrap();
/// vcs_init(path, Some("ana"))?;
/// let c = vcs_commit(path, Some("first"), None)?;
/// assert_eq!(vcs_tag(path, "v1", None)?, c);
/// assert!(vcs_tag(path, "v1", None).is_err());           // tags are not moved
/// # std::fs::remove_dir_all(&dir).unwrap();
/// # Ok(()) }
/// ```
pub fn vcs_tag(path: &str, name: &str, at: Option<&str>) -> Result<String, ApiError> {
    guard(|| vcs_str(vcs_open(path)?.tag_create(name, at)))
}

/// Switch the project file to a branch (stays on it) or to any other revision (detached). Returns
/// the commit now checked out. Errors: [`ApiError::DirtyTree`] if the file has uncommitted changes
/// and `force` is false, [`ApiError::MergeInProgress`] during an unfinished merge.
///
/// ```
/// use osatura_core::prelude::*;
/// # fn main() -> Result<(), ApiError> {
/// # let dir = std::env::temp_dir().join(format!("osatura-doc-checkout-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir).unwrap();
/// # let file = dir.join("p.json");
/// # std::fs::write(&file, Animation::demo().to_json()).unwrap();
/// let path = file.to_str().unwrap();
/// vcs_init(path, Some("ana"))?;
/// vcs_commit(path, Some("first"), None)?;
/// vcs_branch(path, "feature", None)?;
/// // edit the file without committing
/// let mut a = Animation::from_json(&std::fs::read_to_string(path).unwrap())?;
/// let bone = a.bone_names()[0].clone();
/// a.set_key(&bone, "bone_angle", 7.0, 12.0, Interpolation::Linear)?;
/// std::fs::write(path, a.to_json()).unwrap();
/// assert!(matches!(vcs_checkout(path, "feature", false), Err(ApiError::DirtyTree(_))));
/// vcs_checkout(path, "feature", true)?;            // force discards the edit
/// # std::fs::remove_dir_all(&dir).unwrap();
/// # Ok(()) }
/// ```
pub fn vcs_checkout(path: &str, target: &str, force: bool) -> Result<String, ApiError> {
    guard(|| {
        let repo = vcs_open(path)?;
        vcs_no_merge(&repo)?;
        if !force {
            vcs_clean(&repo)?;
        }
        vcs_str(repo.checkout(target, force))
    })
}

/// Merge revision `rev` into the current branch. Fast-forwards when possible (unless `no_ff`).
/// Conflicts are returned as [`VcsMergeOutcome::Conflicted`], with the file left in the merged
/// state. Errors: [`ApiError::DirtyTree`], [`ApiError::MergeInProgress`].
///
/// ```
/// use osatura_core::prelude::*;
/// # fn main() -> Result<(), ApiError> {
/// # let dir = std::env::temp_dir().join(format!("osatura-doc-merge-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir).unwrap();
/// # let file = dir.join("p.json");
/// # std::fs::write(&file, Animation::demo().to_json()).unwrap();
/// let path = file.to_str().unwrap();
/// let edit = |angle: f32| -> Result<(), ApiError> {
///     let mut a = Animation::from_json(&std::fs::read_to_string(path).unwrap())?;
///     let bone = a.bone_names()[0].clone();
///     a.set_key(&bone, "bone_angle", 5.0, angle, Interpolation::Linear)?;
///     std::fs::write(path, a.to_json()).unwrap();
///     Ok(())
/// };
/// vcs_init(path, Some("ana"))?;
/// vcs_commit(path, Some("base"), None)?;
/// vcs_branch(path, "other", None)?;
/// edit(20.0)?;
/// vcs_commit(path, Some("main edit"), None)?;
/// vcs_checkout(path, "other", false)?;
/// edit(40.0)?;                                      // the same key, another value
/// vcs_commit(path, Some("other edit"), None)?;
/// vcs_checkout(path, "main", false)?;
/// match vcs_merge(path, "other", false, None, None)? {
///     VcsMergeOutcome::Conflicted(conflicts) => {
///         assert!(!conflicts.is_empty());
///         // while unresolved, committing is refused with a typed error
///         assert!(matches!(vcs_commit(path, None, None), Err(ApiError::Conflicts(_))));
///         for c in &conflicts {
///             vcs_resolve(path, &c.path, &VcsChoice::Theirs)?;
///         }
///         vcs_commit(path, None, None)?;            // the merge commit
///     }
///     other => panic!("expected a conflict, got {other:?}"),
/// }
/// assert_eq!(vcs_log(path, 10)?.len(), 4);
/// # std::fs::remove_dir_all(&dir).unwrap();
/// # Ok(()) }
/// ```
pub fn vcs_merge(path: &str, rev: &str, no_ff: bool, message: Option<&str>, author: Option<&str>) -> Result<VcsMergeOutcome, ApiError> {
    guard(|| {
        let repo = vcs_open(path)?;
        vcs_no_merge(&repo)?;
        vcs_clean(&repo)?;
        vcs_str(repo.merge(rev, no_ff, message, author)).map(vcs_outcome)
    })
}

/// The conflicts still unresolved in the project's pending merge, cherry-pick or revert (empty
/// when none is in progress).
///
/// ```
/// use osatura_core::prelude::*;
/// # fn main() -> Result<(), ApiError> {
/// # let dir = std::env::temp_dir().join(format!("osatura-doc-conflicts-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir).unwrap();
/// # let file = dir.join("p.json");
/// # std::fs::write(&file, Animation::demo().to_json()).unwrap();
/// let path = file.to_str().unwrap();
/// vcs_init(path, None)?;
/// assert!(vcs_conflicts(path)?.is_empty());
/// # std::fs::remove_dir_all(&dir).unwrap();
/// # Ok(()) }
/// ```
pub fn vcs_conflicts(path: &str) -> Result<Vec<VcsConflict>, ApiError> {
    guard(|| Ok(vcs_str(vcs_open(path)?.merge_state())?.map(|s| s.conflicts.iter().map(vcs_conflict).collect()).unwrap_or_default()))
}

/// Settle one conflict (by [`VcsConflict::path`]). Returns how many conflicts are left; when it
/// reaches 0 call [`vcs_commit`]. A [`VcsChoice::Value`] that is not valid JSON is rejected.
///
/// ```
/// use osatura_core::prelude::*;
/// # fn main() -> Result<(), ApiError> {
/// # let dir = std::env::temp_dir().join(format!("osatura-doc-resolve-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir).unwrap();
/// # let file = dir.join("p.json");
/// # std::fs::write(&file, Animation::demo().to_json()).unwrap();
/// let path = file.to_str().unwrap();
/// vcs_init(path, None)?;
/// // nothing to resolve outside a merge
/// assert!(vcs_resolve(path, "/x", &VcsChoice::Ours).is_err());
/// assert!(vcs_resolve(path, "/x", &VcsChoice::Value("{ nope".into())).is_err());
/// # std::fs::remove_dir_all(&dir).unwrap();
/// # Ok(()) }
/// ```
pub fn vcs_resolve(path: &str, conflict_path: &str, choice: &VcsChoice) -> Result<usize, ApiError> {
    guard(|| {
        let c = match choice {
            VcsChoice::Ours => crate::semdiff::Choice::Ours,
            VcsChoice::Theirs => crate::semdiff::Choice::Theirs,
            VcsChoice::Value(t) => crate::semdiff::Choice::Value(serde_json::from_str(t).map_err(|e| ApiError::InvalidJson(e.to_string()))?),
        };
        vcs_str(vcs_open(path)?.resolve(conflict_path, &c))
    })
}

/// Drop an unfinished merge, cherry-pick or revert and restore the last commit.
///
/// ```
/// use osatura_core::prelude::*;
/// # fn main() -> Result<(), ApiError> {
/// # let dir = std::env::temp_dir().join(format!("osatura-doc-abort-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir).unwrap();
/// # let file = dir.join("p.json");
/// # std::fs::write(&file, Animation::demo().to_json()).unwrap();
/// let path = file.to_str().unwrap();
/// vcs_init(path, None)?;
/// assert!(vcs_abort(path).is_err());               // no merge in progress
/// # std::fs::remove_dir_all(&dir).unwrap();
/// # Ok(()) }
/// ```
pub fn vcs_abort(path: &str) -> Result<(), ApiError> {
    guard(|| vcs_str(vcs_open(path)?.abort()))
}

/// Replay the change commit `rev` made (relative to its first parent) onto the current branch.
/// Same result and errors as [`vcs_merge`].
///
/// ```
/// use osatura_core::prelude::*;
/// # fn main() -> Result<(), ApiError> {
/// # let dir = std::env::temp_dir().join(format!("osatura-doc-pick-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir).unwrap();
/// # let file = dir.join("p.json");
/// # std::fs::write(&file, Animation::demo().to_json()).unwrap();
/// let path = file.to_str().unwrap();
/// vcs_init(path, Some("ana"))?;
/// vcs_commit(path, Some("base"), None)?;
/// vcs_branch(path, "side", None)?;
/// vcs_checkout(path, "side", false)?;
/// let mut a = Animation::from_json(&std::fs::read_to_string(path).unwrap())?;
/// let bone = a.bone_names()[0].clone();
/// a.set_key(&bone, "bone_angle", 5.0, 33.0, Interpolation::Linear)?;
/// std::fs::write(path, a.to_json()).unwrap();
/// let picked = vcs_commit(path, Some("tweak"), None)?;
/// vcs_checkout(path, "main", false)?;
/// assert!(matches!(vcs_cherry_pick(path, &picked, None)?, VcsMergeOutcome::Merged(_)));
/// assert_eq!(vcs_log(path, 5)?[0].message, "tweak");
/// # std::fs::remove_dir_all(&dir).unwrap();
/// # Ok(()) }
/// ```
pub fn vcs_cherry_pick(path: &str, rev: &str, author: Option<&str>) -> Result<VcsMergeOutcome, ApiError> {
    guard(|| {
        let repo = vcs_open(path)?;
        vcs_no_merge(&repo)?;
        vcs_clean(&repo)?;
        vcs_str(repo.cherry_pick(rev, author)).map(vcs_outcome)
    })
}

/// Undo the change commit `rev` made on the current branch, as a new commit. Same result and
/// errors as [`vcs_merge`].
///
/// ```
/// use osatura_core::prelude::*;
/// # fn main() -> Result<(), ApiError> {
/// # let dir = std::env::temp_dir().join(format!("osatura-doc-revert-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir).unwrap();
/// # let file = dir.join("p.json");
/// # std::fs::write(&file, Animation::demo().to_json()).unwrap();
/// let path = file.to_str().unwrap();
/// vcs_init(path, Some("ana"))?;
/// vcs_commit(path, Some("base"), None)?;
/// let mut a = Animation::from_json(&std::fs::read_to_string(path).unwrap())?;
/// let bone = a.bone_names()[0].clone();
/// a.set_key(&bone, "bone_angle", 5.0, 33.0, Interpolation::Linear)?;
/// std::fs::write(path, a.to_json()).unwrap();
/// let bad = vcs_commit(path, Some("oops"), None)?;
/// assert!(matches!(vcs_revert(path, &bad, None)?, VcsMergeOutcome::Merged(_)));
/// assert_eq!(vcs_log(path, 5)?.len(), 3);
/// # std::fs::remove_dir_all(&dir).unwrap();
/// # Ok(()) }
/// ```
pub fn vcs_revert(path: &str, rev: &str, author: Option<&str>) -> Result<VcsMergeOutcome, ApiError> {
    guard(|| {
        let repo = vcs_open(path)?;
        vcs_no_merge(&repo)?;
        vcs_clean(&repo)?;
        vcs_str(repo.revert(rev, author)).map(vcs_outcome)
    })
}

fn vcs_changes(v: Vec<crate::semdiff::Change>) -> Vec<VcsChange> {
    v.into_iter().map(|c| VcsChange { kind: format!("{:?}", c.kind), path: c.path, description: c.description }).collect()
}

/// (1.4) List the branches, sorted by name. An empty repository has none.
pub fn vcs_branches(path: &str) -> Result<Vec<VcsBranch>, ApiError> {
    guard(|| Ok(vcs_str(vcs_open(path)?.branches())?.into_iter().map(|(name, commit, current)| VcsBranch { name, commit, current }).collect()))
}

/// (1.4) Delete a branch. The checked-out branch and unknown names are refused. Commits stay in
/// the store until [`vcs_gc`].
pub fn vcs_branch_delete(path: &str, name: &str) -> Result<(), ApiError> {
    guard(|| vcs_str(vcs_open(path)?.branch_delete(name)))
}

/// (1.4) Branch, HEAD, uncommitted semantic changes and any unfinished merge.
pub fn vcs_status(path: &str) -> Result<VcsStatus, ApiError> {
    guard(|| {
        let st = vcs_str(vcs_open(path)?.status())?;
        Ok(VcsStatus { branch: st.branch, head: st.head, changes: vcs_changes(st.changes), merge: st.merge, conflicts: st.conflicts.into_iter().map(|c| c.path).collect() })
    })
}

/// (1.4) Semantic differences between two revisions; `None` stands for the working file, so
/// `vcs_diff(p, Some("HEAD"), None)` is "what did I change since the last commit".
pub fn vcs_diff(path: &str, a: Option<&str>, b: Option<&str>) -> Result<Vec<VcsChange>, ApiError> {
    guard(|| Ok(vcs_changes(vcs_str(vcs_open(path)?.diff(a, b))?)))
}

/// (1.4) One commit and what it changed against its first parent.
pub fn vcs_show(path: &str, rev: &str) -> Result<VcsShow, ApiError> {
    guard(|| {
        let (id, c, ch) = vcs_str(vcs_open(path)?.show(rev))?;
        Ok(VcsShow { commit: VcsCommit { id, author: c.author, time: c.time, message: c.message }, changes: vcs_changes(ch) })
    })
}

/// (1.4) Delete stored objects no branch, tag or HEAD reaches. Aborts without deleting anything
/// if the history cannot be read.
pub fn vcs_gc(path: &str) -> Result<VcsGc, ApiError> {
    guard(|| {
        let r = vcs_str(vcs_open(path)?.gc())?;
        Ok(VcsGc { kept: r.kept, removed: r.removed })
    })
}

/// (1.4) Integrity check of the store: the list of problems, empty when healthy.
pub fn vcs_fsck(path: &str) -> Result<Vec<String>, ApiError> {
    guard(|| Ok(vcs_open(path)?.fsck()))
}

// ---------------------------------------------------------------------------------------------
// Animation CI (write operations), 1.3
// ---------------------------------------------------------------------------------------------

/// What [`ci_run_mode`] does with the goldens (recorded reference frames).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CiMode {
    /// Compare only; nothing is written (same as [`ci_run`]).
    Verify,
    /// Re-record every golden of the render checks from the current render.
    Update,
    /// Accept only the failing or missing goldens; passing ones stay untouched.
    Bless,
}

/// Result of [`ci_init`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct CiInit {
    /// The manifest file that was written.
    pub manifest: String,
    /// `(kind, name)` of every generated check.
    pub checks: Vec<(String, String)>,
    /// The first run that recorded the goldens, `None` when `write_goldens` was false.
    pub recorded: Option<CiOutcome>,
}

/// Run an animation-CI manifest in the given [`CiMode`]. Like [`ci_run`], failing checks are
/// reported in the outcome, not as an error; an unreadable or invalid manifest is
/// [`ApiError::InvalidArgument`].
///
/// ```
/// use osatura_core::prelude::*;
/// # fn main() -> Result<(), ApiError> {
/// # let dir = std::env::temp_dir().join(format!("osatura-doc-cirun-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir).unwrap();
/// # std::fs::write(dir.join("p.json"), Animation::demo().to_json()).unwrap();
/// # let manifest = dir.join("ci.json");
/// # std::fs::write(&manifest, r#"{"schema":"osatura-ci/1","project":"p.json","goldens":"goldens","checks":[{"kind":"render","name":"f","frames":[0],"scale":0.25}]}"#).unwrap();
/// let m = manifest.to_str().unwrap();
/// assert_eq!(ci_run_mode(m, CiMode::Verify)?.exit_code, 1);   // no golden recorded yet
/// let rec = ci_run_mode(m, CiMode::Update)?;
/// assert_eq!(rec.exit_code, 0, "{}", rec.json);
/// assert_eq!(ci_run_mode(m, CiMode::Verify)?.passed, 1);
/// assert!(ci_run_mode("/nonexistent-osatura.json", CiMode::Update).is_err());
/// # std::fs::remove_dir_all(&dir).unwrap();
/// # Ok(()) }
/// ```
pub fn ci_run_mode(manifest_path: &str, mode: CiMode) -> Result<CiOutcome, ApiError> {
    guard(|| {
        let o = crate::ci::RunOptions { update: mode == CiMode::Update, bless: mode == CiMode::Bless, ..Default::default() };
        let r = crate::ci::run(std::path::Path::new(manifest_path), &o).map_err(ApiError::InvalidArgument)?;
        Ok(ci_outcome(&r))
    })
}

/// Accept the current render of every failing or missing golden (shorthand for
/// [`ci_run_mode`] with [`CiMode::Bless`]).
///
/// ```
/// use osatura_core::prelude::*;
/// # fn main() -> Result<(), ApiError> {
/// # let dir = std::env::temp_dir().join(format!("osatura-doc-cibless-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir).unwrap();
/// # std::fs::write(dir.join("p.json"), Animation::demo().to_json()).unwrap();
/// # let manifest = dir.join("ci.json");
/// # std::fs::write(&manifest, r#"{"schema":"osatura-ci/1","project":"p.json","goldens":"goldens","checks":[{"kind":"render","name":"f","frames":[0],"scale":0.25}]}"#).unwrap();
/// let m = manifest.to_str().unwrap();
/// let o = ci_bless(m)?;                       // missing golden is accepted
/// assert_eq!(o.exit_code, 0, "{}", o.json);
/// assert_eq!(ci_run(m)?.passed, 1);
/// # std::fs::remove_dir_all(&dir).unwrap();
/// # Ok(()) }
/// ```
pub fn ci_bless(manifest_path: &str) -> Result<CiOutcome, ApiError> {
    ci_run_mode(manifest_path, CiMode::Bless)
}

/// Create a manifest with default checks for `project_file`, calibrated on the project as it is
/// now. The manifest goes to `manifest` (default: `osatura-ci.json` next to the project, `ci::DEFAULT_MANIFEST`); an existing
/// file is never overwritten. With `write_goldens` the reference frames are recorded right away.
///
/// ```
/// use osatura_core::prelude::*;
/// # fn main() -> Result<(), ApiError> {
/// # let dir = std::env::temp_dir().join(format!("osatura-doc-ciinit-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir).unwrap();
/// # let file = dir.join("p.json");
/// # std::fs::write(&file, Animation::demo().to_json()).unwrap();
/// let init = ci_init(file.to_str().unwrap(), None, true)?;
/// assert!(init.checks.iter().any(|(kind, _)| kind == "validate"));
/// assert_eq!(init.recorded.as_ref().map(|r| r.errors), Some(0));
/// assert!(ci_init(file.to_str().unwrap(), None, false).is_err());   // manifest exists
/// assert_eq!(ci_run(&init.manifest)?.errors, 0);
/// # std::fs::remove_dir_all(&dir).unwrap();
/// # Ok(()) }
/// ```
pub fn ci_init(project_file: &str, manifest: Option<&str>, write_goldens: bool) -> Result<CiInit, ApiError> {
    guard(|| {
        let proj = std::path::Path::new(project_file);
        let out = match manifest {
            Some(m) => std::path::PathBuf::from(m),
            None => proj.parent().filter(|d| !d.as_os_str().is_empty()).map(|d| d.join(crate::ci::DEFAULT_MANIFEST)).unwrap_or_else(|| std::path::PathBuf::from(crate::ci::DEFAULT_MANIFEST)),
        };
        if out.exists() {
            return Err(ApiError::InvalidArgument(format!("{} already exists", out.display())));
        }
        let m = crate::ci::init_manifest(proj, &out).map_err(ApiError::InvalidArgument)?;
        let text = serde_json::to_string_pretty(&m).map_err(|e| ApiError::Internal(e.to_string()))?;
        crate::fsutil::write_atomic(&out, text.as_bytes()).map_err(|e| ApiError::InvalidArgument(format!("cannot write {e}")))?;
        let checks = m.checks.iter().map(|c| (c.kind().to_string(), c.name().to_string())).collect();
        let recorded = if write_goldens {
            let r = crate::ci::run(&out, &crate::ci::RunOptions { update: true, ..Default::default() }).map_err(ApiError::InvalidArgument)?;
            Some(ci_outcome(&r))
        } else {
            None
        };
        Ok(CiInit { manifest: out.to_string_lossy().to_string(), checks, recorded })
    })
}

/// A running state machine of an [`Animation`] (interactive playback). Create it with
/// [`Animation::player`], feed it inputs by name, advance time and render. The animation itself is
/// not changed; the player works on a copy of the project.
pub struct Player {
    rt: crate::machine::MachineRuntime,
    /// Copy of the project used to solve layouts and to build level-of-detail levels.
    proj: Project,
    /// Number of channels the project itself has (pose channels follow them).
    base_channels: usize,
    canvas: (f32, f32),
    scrolls: Vec<(String, crate::containers::ScrollRuntime)>,
    /// Level-of-detail levels (built on first use, then cached) and the one in use.
    lod_levels: Option<Vec<crate::lod::Lod>>,
    lod_pick: usize,
    lod_screen: Option<(f32, f32, f32)>,
}

/// A loaded, validated animation plus its undo history.
pub struct Animation {
    editor: Editor,
}

fn guard<T>(f: impl FnOnce() -> Result<T, ApiError>) -> Result<T, ApiError> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(_) => Err(ApiError::Internal("panic inside osatura-core".into())),
    }
}

fn check_frame(a: &Animation, frame: f32) -> Result<f32, ApiError> {
    if !frame.is_finite() {
        return Err(ApiError::InvalidArgument("frame must be a finite number".into()));
    }
    Ok(frame.clamp(0.0, a.editor.project().frames.saturating_sub(1) as f32))
}

impl Animation {
    /// Parse and validate a project from JSON text.
    pub fn from_json(json: &str) -> Result<Animation, ApiError> {
        if json.len() > MAX_JSON_BYTES {
            return Err(ApiError::InvalidJson("project is larger than MAX_JSON_BYTES".into()));
        }
        guard(|| {
            let project = Project::from_json(json).map_err(|e| ApiError::InvalidJson(e.to_string()))?;
            Self::from_project(project)
        })
    }

    fn from_project(project: Project) -> Result<Animation, ApiError> {
        let editor = Editor::new(project).map_err(ApiError::InvalidProject)?;
        Ok(Animation { editor })
    }

    /// Check a project JSON text without keeping it.
    pub fn validate_json(json: &str) -> Result<(), ApiError> {
        Animation::from_json(json).map(|_| ())
    }

    /// A small built-in example animation (useful for tests and first steps).
    pub fn demo() -> Animation {
        Animation { editor: Editor::new(crate::demo::project()).expect("built-in demo project is valid") }
    }

    /// The current project as pretty-printed JSON.
    pub fn to_json(&self) -> String {
        self.editor.project().to_json_pretty()
    }

    pub fn info(&self) -> AnimationInfo {
        let p = self.editor.project();
        AnimationInfo { name: p.name.clone(), width: p.width, height: p.height, fps: p.fps, frames: p.frames }
    }

    pub fn frame_count(&self) -> u32 {
        self.editor.project().frames
    }

    pub fn fps(&self) -> f32 {
        self.editor.project().fps
    }

    /// Bone names in rig order (the order of [`Animation::bone_world`]).
    pub fn bone_names(&self) -> Vec<String> {
        self.editor.project().bones.iter().map(|b| b.name.clone()).collect()
    }

    /// Layer paths (`group/child`), depth first.
    pub fn layer_names(&self) -> Vec<String> {
        self.editor.project().layer_paths()
    }

    /// World matrices of all bones at `frame` (clamped to the timeline), six floats per bone:
    /// `a b c d tx ty`. Fails only for a non-finite frame.
    pub fn bone_world(&self, frame: f32) -> Result<Vec<[f32; 6]>, ApiError> {
        let f = check_frame(self, frame)?;
        guard(|| Ok(self.editor.engine().evaluate(f).world.iter().map(|m| [m.a, m.b, m.c, m.d, m.tx, m.ty]).collect()))
    }

    /// Names of the runtime state machines stored in the project.
    pub fn machine_names(&self) -> Vec<String> {
        self.editor.project().machines.iter().map(|m| m.name.clone()).collect()
    }

    /// Start the named state machine at time 0 (states at their entry, inputs at their defaults).
    pub fn player(&self, machine: &str) -> Result<Player, ApiError> {
        guard(|| {
            let p = self.editor.project();
            let rt = crate::machine::MachineRuntime::new(p, machine).map_err(ApiError::InvalidArgument)?;
            let mut pl = Player {
                rt,
                proj: p.clone(),
                base_channels: p.channels.len(),
                canvas: (p.width, p.height),
                scrolls: Vec::new(),
                lod_levels: None,
                lod_pick: 0,
                lod_screen: None,
            };
            pl.init_scrolls()?;
            Ok(pl)
        })
    }

    fn raster_opts(&self, s: &RenderSettings) -> Result<RasterOptions, ApiError> {
        if !s.scale.is_finite() || !(0.05..=4.0).contains(&s.scale) {
            return Err(ApiError::InvalidArgument("scale must be within 0.05..=4".into()));
        }
        let p = self.editor.project();
        let (w, h) = ((p.width * s.scale).round(), (p.height * s.scale).round());
        if w > MAX_RENDER_SIDE as f32 || h > MAX_RENDER_SIDE as f32 {
            return Err(ApiError::InvalidArgument(format!("output larger than {MAX_RENDER_SIDE} pixels per side")));
        }
        Ok(RasterOptions { scale: s.scale, show_bones: s.show_bones, transparent: s.transparent })
    }

    /// Render one frame to straight-alpha RGBA8.
    pub fn render_rgba(&self, frame: f32, settings: &RenderSettings) -> Result<Image, ApiError> {
        let f = check_frame(self, frame)?;
        let o = self.raster_opts(settings)?;
        guard(|| {
            let (width, height, pixels) = render_rgba(self.editor.engine(), f, &o);
            Ok(Image { width, height, pixels })
        })
    }

    /// Render one frame to a PNG file in memory.
    pub fn render_png(&self, frame: f32, settings: &RenderSettings) -> Result<Vec<u8>, ApiError> {
        let f = check_frame(self, frame)?;
        let o = self.raster_opts(settings)?;
        guard(|| render_png(self.editor.engine(), f, &o).map_err(ApiError::Export))
    }

    /// Render one frame to an SVG document.
    pub fn render_svg(&self, frame: f32, show_bones: bool) -> Result<String, ApiError> {
        let f = check_frame(self, frame)?;
        guard(|| Ok(render_frame(self.editor.engine(), f, &RenderOptions { show_bones })))
    }

    /// Encode the whole animation. `scale` applies to GIF only (within 0.05..=4).
    pub fn export(&self, format: ExportFormat, scale: f32) -> Result<Vec<u8>, ApiError> {
        let e = self.editor.engine();
        match format {
            ExportFormat::Gif => {
                let o = self.raster_opts(&RenderSettings { scale, ..RenderSettings::default() })?;
                guard(|| crate::export::gif_bytes(e, o.scale).map_err(ApiError::Export))
            }
            ExportFormat::Glb => guard(|| crate::gltf::export_glb(e).map_err(ApiError::Export)),
            ExportFormat::Lottie => guard(|| Ok(crate::export::lottie_json(e).to_string().into_bytes())),
            ExportFormat::Html => guard(|| Ok(crate::export::html_player(e).into_bytes())),
            ExportFormat::Apng => {
                let o = self.raster_opts(&RenderSettings { scale, transparent: true, ..RenderSettings::default() })?;
                guard(|| {
                    crate::raster::check_sequence(&e.project, o.scale).map_err(ApiError::Export)?;
                    let (mut w, mut h) = (0usize, 0usize);
                    let mut frames = Vec::with_capacity(e.project.frames as usize);
                    for f in 0..e.project.frames {
                        let (fw, fh, px) = render_rgba(e, f as f32, &o);
                        (w, h) = (fw as usize, fh as usize);
                        frames.push(px);
                    }
                    crate::sizefit::apng_encode_rgba(w, h, &frames, e.project.fps, 1).map_err(ApiError::Export)
                })
            }
        }
    }

    /// Set (or add) a keyframe on `target.prop`. `prop` is the snake_case property name used in
    /// project JSON, for example `"bone_angle"`. Validated, undoable, respects channel locks.
    pub fn set_key(&mut self, target: &str, prop: &str, frame: f32, value: f32, interpolation: Interpolation) -> Result<(), ApiError> {
        let (prop, frame) = self.key_args(prop, frame, value)?;
        guard(|| self.editor.set_key(target, prop, frame, value, interpolation.ease()).map_err(ApiError::EditRejected))
    }

    /// Delete the key at `frame` on `target.prop`; returns whether a key was removed.
    pub fn delete_key(&mut self, target: &str, prop: &str, frame: f32) -> Result<bool, ApiError> {
        let (prop, frame) = self.key_args(prop, frame, 0.0)?;
        guard(|| self.editor.delete_key(target, prop, frame).map_err(ApiError::EditRejected))
    }

    // ----- (1.4) editing basics: create layers and bones -----

    /// Append a filled rectangle layer. With `parent` = the name of a top-level group layer the
    /// layer goes into that group instead. Validated and undoable; `fill` is `#rgb`, `#rrggbb` or
    /// `#rrggbbaa`.
    #[allow(clippy::too_many_arguments)]
    pub fn add_rect_layer(&mut self, name: &str, x: f32, y: f32, w: f32, h: f32, fill: &str, parent: Option<&str>) -> Result<(), ApiError> {
        if ![x, y, w, h].iter().all(|v| v.is_finite()) || w <= 0.0 || h <= 0.0 {
            return Err(ApiError::InvalidArgument("x, y must be finite and w, h positive finite numbers".into()));
        }
        let path = crate::path::VPath::rect("shape", x, y, w, h);
        self.add_vector_layer(name, path, fill, parent)
    }

    /// Append a filled ellipse layer (see [`Animation::add_rect_layer`]).
    #[allow(clippy::too_many_arguments)]
    pub fn add_ellipse_layer(&mut self, name: &str, cx: f32, cy: f32, rx: f32, ry: f32, fill: &str, parent: Option<&str>) -> Result<(), ApiError> {
        if ![cx, cy, rx, ry].iter().all(|v| v.is_finite()) || rx <= 0.0 || ry <= 0.0 {
            return Err(ApiError::InvalidArgument("cx, cy must be finite and rx, ry positive finite numbers".into()));
        }
        let path = crate::path::VPath::ellipse("shape", crate::math::Vec2::new(cx, cy), rx, ry);
        self.add_vector_layer(name, path, fill, parent)
    }

    /// Append an empty group layer (children inherit its transform). Add children with the
    /// `parent` argument of the other `add_*_layer` methods.
    pub fn add_group_layer(&mut self, name: &str) -> Result<(), ApiError> {
        self.insert_layer(crate::project::Layer::group(name, Vec::new()), None)
    }

    fn add_vector_layer(&mut self, name: &str, path: crate::path::VPath, fill: &str, parent: Option<&str>) -> Result<(), ApiError> {
        if crate::style::Rgba::parse(fill).is_none() {
            return Err(ApiError::InvalidArgument(format!("bad colour '{fill}' (use #rgb, #rrggbb or #rrggbbaa)")));
        }
        self.insert_layer(crate::project::Layer::vector(name, vec![path.with_fill(fill)]), parent)
    }

    fn insert_layer(&mut self, layer: crate::project::Layer, parent: Option<&str>) -> Result<(), ApiError> {
        if layer.name.trim().is_empty() {
            return Err(ApiError::InvalidArgument("layer name must not be empty".into()));
        }
        if layer.name.contains('/') {
            return Err(ApiError::InvalidArgument("layer name must not contain '/'".into()));
        }
        if self.layer_names().iter().any(|n| n == &layer.name || n.rsplit('/').next() == Some(layer.name.as_str())) {
            return Err(ApiError::InvalidArgument(format!("a layer named '{}' already exists", layer.name)));
        }
        let at = match parent {
            None => "/layers/-".to_string(),
            Some(g) => {
                let i = self
                    .editor
                    .project()
                    .layers
                    .iter()
                    .position(|l| l.name == g && matches!(l.kind, crate::project::LayerKind::Group { .. }))
                    .ok_or_else(|| ApiError::InvalidArgument(format!("no top-level group layer named '{g}'")))?;
                format!("/layers/{i}/layers/-")
            }
        };
        let value = serde_json::to_value(&layer).map_err(|e| ApiError::Internal(e.to_string()))?;
        guard(|| self.editor.apply(crate::edit::Op::Insert { path: at, value }).map_err(ApiError::EditRejected))
    }

    /// Append a bone. `parent` must already exist (parents precede children). `x`, `y` is the head
    /// position in the parent's space (world space for a root); `angle` is the rest rotation in
    /// degrees. Validated and undoable.
    pub fn add_bone(&mut self, name: &str, parent: Option<&str>, length: f32, x: f32, y: f32, angle: f32) -> Result<(), ApiError> {
        if ![length, x, y, angle].iter().all(|v| v.is_finite()) || length <= 0.0 {
            return Err(ApiError::InvalidArgument("length must be positive and all numbers finite".into()));
        }
        if name.trim().is_empty() {
            return Err(ApiError::InvalidArgument("bone name must not be empty".into()));
        }
        if self.bone_names().iter().any(|n| n == name) {
            return Err(ApiError::InvalidArgument(format!("a bone named '{name}' already exists")));
        }
        if let Some(p) = parent {
            if !self.bone_names().iter().any(|n| n == p) {
                return Err(ApiError::InvalidArgument(format!("unknown parent bone '{p}'")));
            }
        }
        let bone = crate::project::Bone::new(name, parent, length, crate::math::Vec2::new(x, y), angle);
        let value = serde_json::to_value(&bone).map_err(|e| ApiError::Internal(e.to_string()))?;
        guard(|| self.editor.apply(crate::edit::Op::Insert { path: "/bones/-".into(), value }).map_err(ApiError::EditRejected))
    }

    // ----- (1.4) physics bake and EXR -----

    /// Simulate a bone chain (each bone the child of the previous, root first) with a physics
    /// preset (`"hair"`, `"tail"`, `"rope"`, `"antenna"`, `"cape"`) and write the result as angle
    /// channels, as one undo step. With `overwrite = false`, bones that already have hand-made
    /// angle keys keep them (they are listed in [`PhysicsBake::skipped`]).
    pub fn bake_physics(&mut self, chain: &[&str], preset: &str, overwrite: bool) -> Result<PhysicsBake, ApiError> {
        let opts = crate::physics::preset(preset).ok_or_else(|| ApiError::InvalidArgument(format!("unknown preset '{preset}' (hair, tail, rope, antenna, cape)")))?;
        if chain.is_empty() {
            return Err(ApiError::InvalidArgument("the chain must name at least one bone".into()));
        }
        let names: Vec<String> = chain.iter().map(|s| s.to_string()).collect();
        guard(|| {
            let mut p = self.editor.project().clone();
            let sim = crate::physics::simulate_chain(&p, &names, &opts).map_err(ApiError::InvalidArgument)?;
            let (added, skipped) = crate::physics::merge_channels(&mut p, sim, overwrite);
            if added > 0 {
                let value = serde_json::to_value(&p.channels).map_err(|e| ApiError::Internal(e.to_string()))?;
                self.editor.apply(crate::edit::Op::Set { path: "/channels".into(), value }).map_err(ApiError::EditRejected)?;
            }
            Ok(PhysicsBake { added, skipped })
        })
    }

    /// Render one frame to an OpenEXR file in memory: linear light, premultiplied alpha, rendered
    /// by the float compositor. `float32 = false` stores HALF, `true` stores FLOAT.
    pub fn render_exr(&self, frame: f32, settings: &RenderSettings, float32: bool) -> Result<Vec<u8>, ApiError> {
        let f = check_frame(self, frame)?;
        let o = self.raster_opts(settings)?;
        guard(|| {
            let img = crate::fraster::render_float(self.editor.engine(), f, &o);
            let prec = if float32 { crate::hdr::Precision::Float } else { crate::hdr::Precision::Half };
            crate::hdr::encode_exr_buf(img.width, img.height, &img.rgba, prec).map_err(ApiError::Export)
        })
    }

    fn key_args(&self, prop: &str, frame: f32, value: f32) -> Result<(Prop, f32), ApiError> {
        let p = Prop::parse(prop).ok_or_else(|| ApiError::InvalidArgument(format!("unknown property '{prop}'")))?;
        if !frame.is_finite() || !value.is_finite() {
            return Err(ApiError::InvalidArgument("frame and value must be finite numbers".into()));
        }
        Ok((p, frame))
    }

    /// Derive level-of-detail levels with a declared error bound: level 1 stays within
    /// `tolerance_px` output pixels at `scale` pixels per stage unit, each further level within
    /// twice the previous one; `levels` is 1..=12. Level 0 is the original. Deterministic; the
    /// bound covers timeline playback (see the `Player::set_screen_size` note for machines).
    ///
    /// ```
    /// use osatura_core::prelude::*;
    /// # fn main() -> Result<(), ApiError> {
    /// let anim = Animation::demo();
    /// let levels = anim.lod_levels(0.5, 1.0, 3)?;
    /// assert_eq!(levels.len(), 4); // the original plus three
    /// assert_eq!(levels[0].declared_px, 0.0);
    /// assert!(levels[3].declared_px >= levels[1].declared_px);
    /// let coarse = Animation::from_json(&levels[3].json)?; // every level is a normal project
    /// assert_eq!(coarse.frame_count(), anim.frame_count());
    /// # Ok(()) }
    /// ```
    pub fn lod_levels(&self, tolerance_px: f64, scale: f64, levels: usize) -> Result<Vec<LodLevel>, ApiError> {
        let o = crate::lod::LodOptions::new(tolerance_px, scale, levels);
        o.check().map_err(ApiError::InvalidArgument)?;
        guard(|| {
            let v = crate::lod::build(self.editor.project(), &o).map_err(ApiError::InvalidArgument)?;
            Ok(v.into_iter()
                .map(|l| LodLevel {
                    level: l.record.level,
                    declared_px: l.record.declared_px,
                    bound_stage: l.record.bound_stage,
                    path_anchors: l.record.stats.path_anchors,
                    keys: l.record.stats.keys,
                    layers: l.record.stats.layers,
                    json: l.project.to_json_pretty(),
                })
                .collect())
        })
    }

    /// Find the smallest export of `format` within `max_bytes` (1 KB..=1 GB) by lowering the
    /// quality knobs of that format step by step. When the budget cannot be met the smallest
    /// result is returned with `met == false`.
    pub fn fit_to_size(&self, format: FitFormat, max_bytes: usize) -> Result<SizeFit, ApiError> {
        if !(1000..=1_000_000_000).contains(&max_bytes) {
            return Err(ApiError::InvalidArgument("max_bytes must be within 1000..=1000000000".into()));
        }
        let f = match format {
            FitFormat::Lottie => crate::sizefit::Format::Lottie,
            FitFormat::Html => crate::sizefit::Format::Html,
            FitFormat::Gif => crate::sizefit::Format::Gif,
            FitFormat::Apng => crate::sizefit::Format::Apng,
            FitFormat::Project => crate::sizefit::Format::Project,
        };
        guard(|| {
            let r = crate::sizefit::fit(self.editor.project(), crate::sizefit::FitOptions::new(f, max_bytes)).map_err(ApiError::Export)?;
            Ok(SizeFit { met: r.met, size: r.bytes.len(), ssim_min: r.quality.ssim_min, reason: r.reason, bytes: r.bytes })
        })
    }

    /// Retarget a BVH motion-capture clip (text) onto this rig as keyed channels, in one undoable
    /// edit. Bones are matched by name and topology; locked channels are skipped and reported.
    pub fn retarget_bvh(&mut self, bvh: &str, settings: &MocapSettings) -> Result<MocapReport, ApiError> {
        guard(|| {
            let b = crate::mocap::parse_bvh(bvh).map_err(ApiError::InvalidArgument)?;
            let p = self.editor.project().clone();
            let m = crate::mocap::match_bones(&b, &p, None).map_err(ApiError::InvalidArgument)?;
            let o = crate::mocap::RetargetOptions { in_place: settings.in_place, mirror: settings.mirror, smooth: settings.smooth, ..Default::default() };
            let r = crate::mocap::retarget(&p, &b, &m, &o).map_err(ApiError::InvalidArgument)?;
            let op = crate::mocap::bake_op(&p, &r).map_err(ApiError::Internal)?;
            self.editor.apply(op).map_err(ApiError::EditRejected)?;
            Ok(MocapReport {
                matched: m.matches.iter().map(|x| (x.bvh_name.clone(), x.rig_name.clone())).collect(),
                unmatched_joints: m.unmatched_bvh.clone(),
                unmatched_bones: m.unmatched_rig.clone(),
                frames: r.out_frames,
                bones_used: r.bones_used,
                blocked: r.blocked.clone(),
                notes: r.notes.clone(),
            })
        })
    }

    /// Run a pose fuzz campaign (`samples` within 1..=100000 random poses from `seed`) and report
    /// where the rig breaks. The animation is only read.
    ///
    /// ```
    /// use osatura_core::prelude::*;
    /// # fn main() -> Result<(), ApiError> {
    /// let r = Animation::demo().fuzz(20, 1)?;
    /// assert_eq!(r.samples, 20);
    /// assert!(r.json.contains("failure_rate"));
    /// assert!(Animation::demo().fuzz(0, 1).is_err());
    /// # Ok(()) }
    /// ```
    pub fn fuzz(&self, samples: usize, seed: u64) -> Result<FuzzSummary, ApiError> {
        if !(1..=100_000).contains(&samples) {
            return Err(ApiError::InvalidArgument("samples must be within 1..=100000".into()));
        }
        guard(|| {
            let o = crate::fuzz::FuzzOptions { samples, seed, ..Default::default() };
            let r = crate::fuzz::run(self.editor.project(), None, &o).map_err(ApiError::InvalidArgument)?;
            Ok(FuzzSummary { samples: r.summary.samples, failures: r.summary.failures, failure_rate: r.summary.failure_rate, max_severity: r.summary.max_severity, json: r.to_json() })
        })
    }

    /// Build an animation from a bitmap: the image is traced into filled vector layers (at most
    /// `colors`, 2..=64) on a stage of the image size.
    pub fn from_image(image: &[u8], colors: usize, name: &str) -> Result<Animation, ApiError> {
        guard(|| {
            let res = trace_bytes(image, colors)?;
            Self::from_project(crate::autotrace::to_project(&res, name))
        })
    }

    /// Undo the last edit; false when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        self.editor.undo()
    }

    /// Redo the last undone edit; false when there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        self.editor.redo()
    }
}

impl Player {
    /// Set a bool input.
    pub fn set_bool(&mut self, name: &str, value: bool) -> Result<(), ApiError> {
        self.rt.set_bool(name, value).map_err(ApiError::InvalidArgument)
    }

    /// Set a number input (finite).
    pub fn set_number(&mut self, name: &str, value: f64) -> Result<(), ApiError> {
        self.rt.set_number(name, value).map_err(ApiError::InvalidArgument)
    }

    /// Fire a trigger input; it is consumed by the transition that tests it.
    pub fn fire(&mut self, name: &str) -> Result<(), ApiError> {
        self.rt.fire(name).map_err(ApiError::InvalidArgument)
    }

    /// Publish an external value (data binding path such as `player.health`).
    /// A scroll region whose `input` is this path takes the value as its offset in pixels along
    /// its main axis (as if dragged there; the scroll is then at rest).
    pub fn set_value(&mut self, path: &str, value: f64) -> Result<(), ApiError> {
        self.rt.set_value(path, value).map_err(ApiError::InvalidArgument)?;
        let mut hits: Vec<(String, [f32; 2])> = Vec::new();
        for s in self.proj.scrolls.iter().filter(|s| s.input.as_deref() == Some(path)) {
            let mut raw = [0.0f32; 2];
            match s.axis {
                crate::containers::ScrollAxis::X => raw[0] = value as f32,
                _ => raw[1] = value as f32,
            }
            hits.push((s.id.clone(), raw));
        }
        for (id, raw) in hits {
            if let Some((_, r)) = self.scrolls.iter_mut().find(|(i, _)| *i == id) {
                r.drag_to(raw);
            }
        }
        Ok(())
    }

    /// Advance the machine (and any scroll regions that are springing back or flinging) by `dt`
    /// seconds (0..=10).
    pub fn advance(&mut self, dt: f64) -> Result<(), ApiError> {
        guard(|| {
            self.rt.step(dt).map_err(ApiError::InvalidArgument)?;
            for (_, r) in &mut self.scrolls {
                r.step(dt as f32);
            }
            Ok(())
        })
    }

    fn init_scrolls(&mut self) -> Result<(), ApiError> {
        self.scrolls.clear();
        if self.proj.scrolls.is_empty() {
            return Ok(());
        }
        let sol = crate::containers::solve(&self.proj, self.canvas.0, self.canvas.1).map_err(ApiError::InvalidArgument)?;
        for sc in &self.proj.scrolls {
            let max = sol.scrolls.iter().find(|i| i.id == sc.id).map_or([0.0; 2], |i| i.max);
            self.scrolls.push((sc.id.clone(), crate::containers::ScrollRuntime::new(sc, max)));
        }
        Ok(())
    }

    /// Lay the project out for another canvas size (stage units), as a responsive window resize:
    /// containers re-solve, scroll limits follow, and renders get that size. Fails for a size that
    /// is not finite, not positive or larger than the engine allows.
    pub fn set_canvas_size(&mut self, width: f32, height: f32) -> Result<(), ApiError> {
        crate::containers::responsive(&self.proj, width, height).map_err(ApiError::InvalidArgument)?;
        let old = self.canvas;
        self.canvas = (width, height);
        if let Err(e) = self.refresh_scroll_limits() {
            self.canvas = old;
            return Err(e);
        }
        Ok(())
    }

    fn refresh_scroll_limits(&mut self) -> Result<(), ApiError> {
        if self.scrolls.is_empty() {
            return Ok(());
        }
        let sol = crate::containers::solve(&self.proj, self.canvas.0, self.canvas.1).map_err(ApiError::InvalidArgument)?;
        for (id, r) in &mut self.scrolls {
            if let Some(i) = sol.scrolls.iter().find(|i| i.id == *id) {
                r.set_max(i.max);
            }
        }
        Ok(())
    }

    /// Current canvas size in stage units (the project's own size until [`Player::set_canvas_size`]).
    pub fn canvas_size(&self) -> (f32, f32) {
        self.canvas
    }

    /// Ids of the project's scroll regions.
    pub fn scroll_ids(&self) -> Vec<String> {
        self.scrolls.iter().map(|(i, _)| i.clone()).collect()
    }

    fn scroll_mut(&mut self, id: &str) -> Result<&mut crate::containers::ScrollRuntime, ApiError> {
        self.scrolls.iter_mut().find(|(i, _)| i == id).map(|(_, r)| r).ok_or_else(|| ApiError::InvalidArgument(format!("unknown scroll '{id}'")))
    }

    /// Finger / pointer drag: the raw offset in pixels the user asks for (content moved up / left
    /// by that much). Past the ends the displayed offset is rubber-banded or clamped.
    pub fn scroll_drag(&mut self, id: &str, x: f32, y: f32) -> Result<(), ApiError> {
        if !x.is_finite() || !y.is_finite() {
            return Err(ApiError::InvalidArgument("scroll offset must be finite".into()));
        }
        self.scroll_mut(id)?.drag_to([x, y]);
        Ok(())
    }

    /// Let go with a release velocity in pixels per second; [`Player::advance`] then lets the
    /// region fling and settle (snap points, spring back).
    pub fn scroll_release(&mut self, id: &str, vx: f32, vy: f32) -> Result<(), ApiError> {
        if !vx.is_finite() || !vy.is_finite() {
            return Err(ApiError::InvalidArgument("release velocity must be finite".into()));
        }
        self.scroll_mut(id)?.release([vx, vy]);
        Ok(())
    }

    /// Displayed offset of a scroll region in pixels, or `None` for an unknown id.
    pub fn scroll_offset(&self, id: &str) -> Option<[f32; 2]> {
        self.scrolls.iter().find(|(i, _)| i == id).map(|(_, r)| r.pos)
    }

    /// Largest useful offset of a scroll region at the current canvas size.
    pub fn scroll_max(&self, id: &str) -> Option<[f32; 2]> {
        self.scrolls.iter().find(|(i, _)| i == id).map(|(_, r)| r.max())
    }

    /// Whether a scroll region has come to rest.
    pub fn scroll_settled(&self, id: &str) -> Option<bool> {
        self.scrolls.iter().find(|(i, _)| i == id).map(|(_, r)| r.is_settled())
    }

    /// Tell the player how large it is shown on screen so that it can pick a cheaper level of
    /// detail: the coarsest level whose declared error bound, at that screen scale, is at most
    /// `max_error_px` pixels. The levels are built on first use (deterministically) and cached.
    /// Only geometry and timeline keys are reduced; state-machine poses are not modelled by the
    /// bound (see `lod::LIMITS`). With `max_error_px` of 0 only levels whose declared bound is 0 (lossless ones) qualify.
    pub fn set_screen_size(&mut self, width_px: f32, height_px: f32, max_error_px: f32) -> Result<(), ApiError> {
        if !(width_px.is_finite() && height_px.is_finite() && width_px > 0.0 && height_px > 0.0) {
            return Err(ApiError::InvalidArgument("screen size must be finite and > 0".into()));
        }
        if !max_error_px.is_finite() || max_error_px < 0.0 {
            return Err(ApiError::InvalidArgument("max_error_px must be finite and >= 0".into()));
        }
        guard(|| {
            if self.lod_levels.is_none() {
                let lv = crate::lod::build(&self.proj, &crate::lod::LodOptions::new(LOD_TOLERANCE, 1.0, LOD_LEVELS)).map_err(ApiError::InvalidArgument)?;
                self.lod_levels = Some(lv);
            }
            let scale = crate::lod::fit_scale(&Project { width: self.canvas.0, height: self.canvas.1, ..self.proj.clone() }, width_px as f64, height_px as f64);
            let recs: Vec<_> = self.lod_levels.as_ref().map(|l| l.iter().map(|x| x.record.clone()).collect()).unwrap_or_default();
            self.lod_pick = crate::lod::pick_level(&recs, scale, max_error_px as f64);
            self.lod_screen = Some((width_px, height_px, max_error_px));
            Ok(())
        })
    }

    /// Forget the screen size: render the original again.
    pub fn clear_screen_size(&mut self) {
        self.lod_screen = None;
        self.lod_pick = 0;
    }

    /// Level of detail in use (0 = the original).
    pub fn lod_level(&self) -> usize {
        if self.lod_screen.is_some() {
            self.lod_pick
        } else {
            0
        }
    }

    /// Declared error bound in pixels of the level in use at the screen size given to
    /// [`Player::set_screen_size`]; 0 for the original or when no screen size is set.
    pub fn lod_bound_px(&self) -> f32 {
        let (Some((w, h, _)), Some(lv)) = (self.lod_screen, self.lod_levels.as_ref()) else { return 0.0 };
        let scale = crate::lod::fit_scale(&Project { width: self.canvas.0, height: self.canvas.1, ..self.proj.clone() }, w as f64, h as f64);
        lv.get(self.lod_pick).map_or(0.0, |l| (l.record.bound_stage * scale) as f32)
    }

    /// The project edits that make the playback engine: level of detail, canvas size, scroll offsets.
    fn shape(&self, p: &mut Project, dropped: &[usize]) {
        if self.lod_level() > 0 {
            if let Some(l) = self.lod_levels.as_ref().and_then(|v| v.get(self.lod_pick)) {
                let pose: Vec<crate::project::Channel> = p.channels.split_off(self.base_channels.min(p.channels.len()));
                let mut q = l.project.clone();
                q.machines.clear();
                q.bindings.clear();
                q.channels.extend(pose);
                let mut i = 0usize;
                q.channels.retain(|_| {
                    i += 1;
                    !dropped.contains(&(i - 1))
                });
                *p = q;
            }
        }
        p.width = self.canvas.0;
        p.height = self.canvas.1;
        for s in &mut p.scrolls {
            if let Some((_, r)) = self.scrolls.iter().find(|(i, _)| *i == s.id) {
                for a in 0..2 {
                    s.offset[a] = crate::containers::raw_offset_for(r.pos[a], r.max()[a], s.elastic.as_ref());
                }
            }
        }
    }

    fn build_engine(&self) -> Result<crate::engine::Engine, ApiError> {
        let mut dropped: Vec<usize> = Vec::new();
        loop {
            match self.rt.engine_with(|p| self.shape(p, &dropped)) {
                Ok(e) => return Ok(e),
                Err(m) => {
                    // a reduced level may have removed a layer that a pose channel animates: drop those channels
                    let floor = self.lod_levels.as_ref().and_then(|v| v.get(self.lod_pick)).map_or(usize::MAX, |l| l.project.channels.len());
                    let mut fresh = Vec::new();
                    if self.lod_level() > 0 {
                        for part in m.split("channel ").skip(1) {
                            if let Some(n) = part.split(':').next().and_then(|d| d.trim().parse::<usize>().ok()) {
                                if n >= floor && part.contains("unknown target") {
                                    // n counts the channels that are still present; map it back to the original position
                                    let mut left = n;
                                    let mut orig = 0usize;
                                    loop {
                                        if !dropped.contains(&orig) {
                                            if left == 0 {
                                                break;
                                            }
                                            left -= 1;
                                        }
                                        orig += 1;
                                    }
                                    if !fresh.contains(&orig) {
                                        fresh.push(orig);
                                    }
                                }
                            }
                        }
                    }
                    if fresh.is_empty() {
                        return Err(ApiError::InvalidProject(vec![m]));
                    }
                    // indices are positions in the shaped project; they stay valid because we drop highest first
                    dropped.extend(fresh);
                }
            }
        }
    }

    /// Current state of a layer.
    pub fn state(&self, layer: &str) -> Option<String> {
        self.rt.state_of(layer).map(str::to_string)
    }

    /// Events (state entered / exited) not yet taken, as a JSON array, without consuming them.
    pub fn events_json(&self) -> String {
        serde_json::to_string(self.rt.events()).unwrap_or_else(|_| "[]".into())
    }

    /// Discard the pending events.
    pub fn clear_events(&mut self) {
        self.rt.clear_events();
    }

    /// Events (state entered / exited) since the last call, as a JSON array.
    pub fn take_events_json(&mut self) -> String {
        serde_json::to_string(&self.rt.take_events()).unwrap_or_else(|_| "[]".into())
    }

    /// Render the current pose to straight-alpha RGBA8.
    pub fn render_rgba(&self, settings: &RenderSettings) -> Result<Image, ApiError> {
        if !settings.scale.is_finite() || !(0.05..=4.0).contains(&settings.scale) {
            return Err(ApiError::InvalidArgument("scale must be within 0.05..=4".into()));
        }
        if (self.canvas.0 * settings.scale).round() > MAX_RENDER_SIDE as f32 || (self.canvas.1 * settings.scale).round() > MAX_RENDER_SIDE as f32 {
            return Err(ApiError::InvalidArgument(format!("output larger than {MAX_RENDER_SIDE} pixels per side")));
        }
        guard(|| {
            let e = self.build_engine()?;
            let o = RasterOptions { scale: settings.scale, show_bones: settings.show_bones, transparent: settings.transparent };
            let (width, height, pixels) = render_rgba(&e, self.rt.base_frame(), &o);
            Ok(Image { width, height, pixels })
        })
    }

    /// Render the current pose to an SVG document.
    pub fn render_svg(&self, show_bones: bool) -> Result<String, ApiError> {
        guard(|| {
            let e = self.build_engine()?;
            Ok(render_frame(&e, self.rt.base_frame(), &RenderOptions { show_bones }))
        })
    }

    /// World matrices of all bones in the current pose (six floats per bone).
    pub fn bone_world(&self) -> Result<Vec<[f32; 6]>, ApiError> {
        guard(|| {
            let e = self.build_engine()?;
            Ok(e.evaluate(self.rt.base_frame()).world.iter().map(|m| [m.a, m.b, m.c, m.d, m.tx, m.ty]).collect())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_errors_are_typed() {
        assert!(matches!(Animation::from_json(""), Err(ApiError::InvalidJson(_))));
        assert!(matches!(Animation::from_json("[1,2]"), Err(ApiError::InvalidJson(_))));
        assert!(matches!(Animation::from_json("{\"format\":9999,\"name\":\"x\"}"), Err(ApiError::InvalidJson(_))));
        let mut p = crate::demo::project();
        p.fps = -1.0;
        let bad = p.to_json_pretty();
        match Animation::from_json(&bad) {
            Err(ApiError::InvalidProject(v)) => assert!(!v.is_empty()),
            Err(e) => panic!("wrong error {e}"),
            Ok(_) => panic!("accepted a bad fps"),
        }
        assert!(Animation::validate_json(&Animation::demo().to_json()).is_ok());
    }

    #[test]
    fn queries_match_the_project() {
        let a = Animation::demo();
        let i = a.info();
        assert_eq!(i.frames, a.frame_count());
        assert_eq!(i.fps, a.fps());
        assert!(!a.bone_names().is_empty() && !a.layer_names().is_empty());
        let w = a.bone_world(1e9).unwrap(); // clamped to the last frame
        assert_eq!(w.len(), a.bone_names().len());
        assert!(a.bone_world(f32::INFINITY).is_err());
    }

    #[test]
    fn render_validates_and_works() {
        let a = Animation::demo();
        let s = RenderSettings { scale: 0.5, ..RenderSettings::default() };
        let img = a.render_rgba(0.0, &s).unwrap();
        assert_eq!(img.pixels.len(), img.width as usize * img.height as usize * 4);
        for bad in [0.0, -1.0, 5.0, f32::NAN] {
            let s = RenderSettings { scale: bad, ..RenderSettings::default() };
            assert!(matches!(a.render_rgba(0.0, &s), Err(ApiError::InvalidArgument(_))), "scale {bad}");
            assert!(a.render_png(0.0, &s).is_err());
        }
        assert!(a.render_svg(f32::NAN, false).is_err());
        assert!(a.render_svg(0.0, true).unwrap().contains("<svg"));
        assert_eq!(&a.render_png(2.0, &s_default()).unwrap()[1..4], b"PNG");
    }

    fn s_default() -> RenderSettings {
        RenderSettings { scale: 0.25, ..RenderSettings::default() }
    }

    #[test]
    fn edits_validate_undo_and_lock() {
        let mut a = Animation::demo();
        let bone = a.bone_names()[0].clone();
        let before = a.to_json();
        a.set_key(&bone, "bone_angle", 3.0, 12.0, Interpolation::Smooth).unwrap();
        assert_ne!(a.to_json(), before);
        assert!(a.delete_key(&bone, "bone_angle", 3.0).unwrap());
        assert!(!a.delete_key(&bone, "bone_angle", 77.0).unwrap());
        assert!(a.undo() && a.undo());
        assert_eq!(a.to_json(), before);
        assert!(!a.undo());
        assert!(a.redo());
        assert!(matches!(a.set_key(&bone, "no_such_prop", 0.0, 1.0, Interpolation::Linear), Err(ApiError::InvalidArgument(_))));
        assert!(a.set_key(&bone, "bone_angle", f32::NAN, 1.0, Interpolation::Linear).is_err());
        assert!(a.set_key(&bone, "bone_angle", 1.0, f32::INFINITY, Interpolation::Linear).is_err());
        // unknown target is rejected by validation, project unchanged
        let j = a.to_json();
        assert!(a.set_key("no_such_bone", "bone_angle", 1.0, 1.0, Interpolation::Linear).is_err());
        assert_eq!(a.to_json(), j);
    }

    #[test]
    fn exports_produce_bytes() {
        let a = Animation::demo();
        assert!(a.export(ExportFormat::Gif, 0.25).unwrap().starts_with(b"GIF8"));
        assert!(a.export(ExportFormat::Glb, 1.0).unwrap().starts_with(b"glTF"));
        assert!(a.export(ExportFormat::Lottie, 1.0).unwrap().starts_with(b"{"));
        assert!(a.export(ExportFormat::Html, 1.0).unwrap().len() > 100);
        assert!(a.export(ExportFormat::Gif, 99.0).is_err());
    }

    /// Demo project plus two clips and a machine "m" (state a -> b when `go`).
    fn machine_project() -> Project {
        let mut p = crate::demo::project();
        let bone = p.bones[0].name.clone();
        let mk = |n: &str, v: f32| {
            let tr = crate::motionlib::TakeTrack { target: bone.clone(), prop: Prop::BoneAngle, keys: vec![crate::anim::Key { frame: 0.0, value: v, ease: Ease::Linear }, crate::anim::Key { frame: 10.0, value: v, ease: Ease::Linear }] };
            crate::clipstack::Clip::new(n, crate::motionlib::Take { name: n.into(), length: 10.0, tracks: vec![tr] })
        };
        p.clips = vec![mk("a", 0.0), mk("b", 30.0)];
        let st = |n: &str| crate::machine::State { name: n.into(), source: crate::machine::StateSource::Clip { clip: n.into() }, mode: Default::default(), speed: 1.0, blend_in: 0.0 };
        p.machines.push(crate::machine::Machine {
            name: "m".into(),
            inputs: vec![crate::machine::Input { name: "go".into(), kind: crate::machine::InputKind::Bool, default: 0.0, hold: false }],
            layers: vec![crate::machine::Layer {
                name: "l".into(),
                mask: vec![],
                entry: "a".into(),
                states: vec![st("a"), st("b")],
                transitions: vec![crate::machine::Transition {
                    from: "a".into(),
                    to: "b".into(),
                    conditions: vec![crate::machine::Condition { input: "go".into(), op: Default::default(), value: 1.0 }],
                    exit_time: None,
                    duration: Some(0.0),
                    ease: Ease::Linear,
                }],
            }],
        });
        p
    }

    #[test]
    fn player_runs_a_machine() {
        let p = machine_project();
        let a = Animation::from_json(&p.to_json_pretty()).unwrap();
        assert_eq!(a.machine_names(), ["m"]);
        assert!(a.player("nope").is_err());
        let mut pl = a.player("m").unwrap();
        let before = pl.bone_world().unwrap()[0];
        pl.advance(0.1).unwrap();
        assert_eq!(pl.state("l").as_deref(), Some("a"));
        pl.set_bool("go", true).unwrap();
        pl.advance(0.1).unwrap();
        assert_eq!(pl.state("l").as_deref(), Some("b"));
        assert_ne!(pl.bone_world().unwrap()[0], before);
        assert!(pl.take_events_json().contains("\"state\":\"b\""));
        assert!(pl.render_svg(false).unwrap().starts_with("<svg"));
        assert_eq!(pl.render_rgba(&RenderSettings::new(0.25)).unwrap().pixels.len() % 4, 0);
        assert!(pl.set_bool("zzz", true).is_err() && pl.set_number("go", 1.0).is_err() && pl.fire("go").is_err());
        assert!(pl.advance(f64::NAN).is_err() && pl.set_value("bad path", 1.0).is_err());
        assert!(pl.render_rgba(&RenderSettings::new(9.0)).is_err());
    }

    /// machine project + a column of ten 100x20 rows inside a 100x50 scroll region.
    fn scroll_player_project() -> Project {
        use crate::containers::{Container, Dir, Member, RectRule, Scroll};
        use crate::path::VPath;
        use crate::project::Layer;
        let mut p = machine_project();
        for i in 0..10 {
            p.layers.push(Layer::vector(&format!("row{i}"), vec![VPath::rect("r", 0.0, 0.0, 100.0, 20.0).with_fill("#336699")]));
        }
        let mut c = Container::new("col", Dir::Column, (0..10).map(|i| Member::layer(&format!("row{i}"))).collect());
        c.rect = RectRule::default();
        p.containers.push(c);
        let mut s = Scroll::new("list", "col");
        s.viewport = RectRule::Fixed { x: 0.0, y: 0.0, w: 100.0, h: 50.0 };
        s.snap_y = vec![0.0, 40.0, 80.0, 120.0, 150.0];
        s.input = Some("ui.scroll".into());
        p.scrolls.push(s);
        p
    }

    fn row_ys(pl: &Player) -> Vec<f32> {
        let e = pl.build_engine().unwrap();
        (0..10).map(|i| e.layer_local_at(pl.rt.base_frame(), e.node_index(&format!("row{i}")).unwrap()).position.y).collect()
    }

    #[test]
    fn player_scrolls_drags_and_flings() {
        let a = Animation::from_json(&scroll_player_project().to_json_pretty()).unwrap();
        let mut pl = a.player("m").unwrap();
        assert_eq!(pl.scroll_ids(), ["list"]);
        assert!((pl.scroll_max("list").unwrap()[1] - 150.0).abs() < 1e-3);
        assert_eq!(pl.scroll_offset("list"), Some([0.0, 0.0]));
        assert!(pl.scroll_offset("nope").is_none() && pl.scroll_drag("nope", 0.0, 0.0).is_err());
        assert!(pl.scroll_drag("list", f32::NAN, 0.0).is_err() && pl.scroll_release("list", 0.0, f32::INFINITY).is_err());
        let y0 = row_ys(&pl);
        pl.scroll_drag("list", 0.0, 70.0).unwrap();
        let y1 = row_ys(&pl);
        assert!((y0[0] - y1[0] - 70.0).abs() < 1e-3 && (y0[3] - y1[3] - 70.0).abs() < 1e-3, "{y0:?} {y1:?}");
        // past the end the offset clamps
        pl.scroll_drag("list", 0.0, 900.0).unwrap();
        assert!((pl.scroll_offset("list").unwrap()[1] - 150.0).abs() < 1e-3);
        // release with a fling lands on a snap point once advanced
        pl.scroll_drag("list", 0.0, 30.0).unwrap();
        pl.scroll_release("list", 0.0, 100.0).unwrap();
        assert_eq!(pl.scroll_settled("list"), Some(false));
        for _ in 0..300 {
            pl.advance(1.0 / 60.0).unwrap();
        }
        assert_eq!(pl.scroll_settled("list"), Some(true));
        assert_eq!(pl.scroll_offset("list"), Some([0.0, 40.0]));
        // the linked input path drives the offset
        pl.set_value("ui.scroll", 100.0).unwrap();
        assert_eq!(pl.scroll_offset("list"), Some([0.0, 100.0]));
        assert!(pl.set_value("ui.scroll", f64::NAN).is_err());
        // the machine keeps working alongside
        pl.set_bool("go", true).unwrap();
        pl.advance(0.1).unwrap();
        assert_eq!(pl.state("l").as_deref(), Some("b"));
        // determinism: same calls, same offsets
        let run = || {
            let mut q = a.player("m").unwrap();
            q.scroll_drag("list", 0.0, 55.0).unwrap();
            q.scroll_release("list", 0.0, -400.0).unwrap();
            for _ in 0..90 {
                q.advance(1.0 / 60.0).unwrap();
            }
            q.scroll_offset("list")
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn player_canvas_size_relayouts_and_validates() {
        let mut p = scroll_player_project();
        // the viewport follows the canvas height (top 0, bottom 0), 100 wide
        p.scrolls[0].viewport = crate::containers::RectRule::Pin { left: Some(0.0), top: Some(0.0), right: None, bottom: Some(0.0), width: Some(100.0), height: None };
        let a = Animation::from_json(&p.to_json_pretty()).unwrap();
        let mut pl = a.player("m").unwrap();
        let (w0, h0) = pl.canvas_size();
        assert_eq!((w0, h0), (a.info().width, a.info().height));
        for bad in [(0.0, 10.0), (-1.0, 10.0), (f32::NAN, 10.0), (10.0, f32::INFINITY), (1e9, 10.0)] {
            assert!(pl.set_canvas_size(bad.0, bad.1).is_err(), "{bad:?}");
        }
        assert_eq!(pl.canvas_size(), (w0, h0));
        pl.set_canvas_size(w0, 120.0).unwrap();
        assert!((pl.scroll_max("list").unwrap()[1] - 80.0).abs() < 1e-3, "{:?}", pl.scroll_max("list"));
        // the position is re-clamped when the limit shrinks, and renders get the new size
        pl.scroll_drag("list", 0.0, 80.0).unwrap();
        pl.set_canvas_size(w0 / 2.0, 150.0).unwrap();
        assert!((pl.scroll_max("list").unwrap()[1] - 50.0).abs() < 1e-3);
        assert!((pl.scroll_offset("list").unwrap()[1] - 50.0).abs() < 1e-3);
        let img = pl.render_rgba(&RenderSettings::new(1.0)).unwrap();
        assert_eq!((img.width, img.height), ((w0 / 2.0).round() as u32, 150));
        assert!(pl.render_svg(false).unwrap().starts_with("<svg"));
        assert!(pl.render_rgba(&RenderSettings::new(4.0)).is_ok());
        // back to the original size
        pl.set_canvas_size(w0, h0).unwrap();
        assert_eq!(pl.canvas_size(), (w0, h0));
    }

    #[test]
    fn player_picks_a_level_of_detail_within_the_requested_bound() {
        use crate::math::Vec2;
        use crate::path::VPath;
        use crate::project::Layer;
        let mut p = machine_project();
        let pts: Vec<Vec2> = (0..96)
            .map(|i| {
                let t = i as f32 / 96.0 * std::f32::consts::TAU;
                let r = 40.0 + 0.3 * (t * 5.0).sin();
                Vec2::new(100.0 + r * t.cos(), 100.0 + r * t.sin())
            })
            .collect();
        p.layers.push(Layer::vector("blob", vec![VPath::smooth_through("c", &pts, true, 1.0).with_fill("#336699")]));
        let a = Animation::from_json(&p.to_json_pretty()).unwrap();
        let mut pl = a.player("m").unwrap();
        assert_eq!(pl.lod_level(), 0);
        assert_eq!(pl.lod_bound_px(), 0.0);
        let full = pl.render_rgba(&RenderSettings::new(1.0)).unwrap();
        assert!(pl.set_screen_size(0.0, 10.0, 1.0).is_err() && pl.set_screen_size(10.0, 10.0, -1.0).is_err() && pl.set_screen_size(f32::NAN, 10.0, 1.0).is_err());
        // exact request: nothing may be reduced
        let (w, h) = (a.info().width, a.info().height);
        pl.set_screen_size(w, h, 0.0).unwrap();
        assert_eq!(pl.lod_level(), 0);
        // a generous request picks a coarser level whose declared bound stays within it
        for max_px in [0.25f32, 1.0, 4.0] {
            pl.set_screen_size(w, h, max_px).unwrap();
            assert!(pl.lod_bound_px() <= max_px + 1e-6, "bound {} > {max_px}", pl.lod_bound_px());
            let img = pl.render_rgba(&RenderSettings::new(1.0)).unwrap();
            assert_eq!((img.width, img.height), (full.width, full.height));
        }
        pl.set_screen_size(w, h, 4.0).unwrap();
        assert!(pl.lod_level() > 0, "a 4 px budget must allow a reduced level");
        let coarse = pl.lod_level();
        // a smaller screen tolerates at least as coarse a level
        pl.set_screen_size(w / 4.0, h / 4.0, 4.0).unwrap();
        assert!(pl.lod_level() >= coarse);
        // levels are cached: the second call does not rebuild
        let ptr = pl.lod_levels.as_ref().unwrap().as_ptr();
        pl.set_screen_size(w, h, 1.0).unwrap();
        assert_eq!(ptr, pl.lod_levels.as_ref().unwrap().as_ptr());
        // the machine still drives the pose on a reduced level
        pl.set_screen_size(w, h, 4.0).unwrap();
        let before = pl.bone_world().unwrap()[0];
        pl.set_bool("go", true).unwrap();
        pl.advance(0.1).unwrap();
        assert_ne!(pl.bone_world().unwrap()[0], before);
        pl.clear_screen_size();
        assert_eq!(pl.lod_level(), 0);
    }

    #[test]
    fn lod_levels_are_valid_projects_with_a_declared_bound() {
        let a = Animation::demo();
        let lv = a.lod_levels(1.0, 1.0, 3).unwrap();
        assert_eq!(lv.len(), 4);
        assert_eq!(lv[0].level, 0);
        for (i, l) in lv.iter().enumerate() {
            assert_eq!(l.level, i);
            assert!(Animation::from_json(&l.json).is_ok());
            if i > 0 {
                assert!(l.declared_px <= 1.0 * 2f64.powi(i as i32 - 1) + 1e-9);
                assert!((l.bound_at(2.0) - l.bound_stage * 2.0).abs() < 1e-12);
                assert!(l.layers <= lv[0].layers && l.keys <= lv[0].keys);
            }
        }
        for bad in [(0.0, 1.0, 2usize), (-1.0, 1.0, 2), (1.0, 0.0, 2), (1.0, 1.0, 0), (1.0, 1.0, 99), (f64::NAN, 1.0, 2)] {
            assert!(matches!(a.lod_levels(bad.0, bad.1, bad.2), Err(ApiError::InvalidArgument(_))), "{bad:?}");
        }
        assert_eq!(a.lod_levels(1.0, 1.0, 3).unwrap(), lv, "deterministic");
    }

    #[test]
    fn fit_to_size_meets_a_generous_budget_and_reports_an_impossible_one() {
        use crate::path::VPath;
        use crate::project::Layer;
        // a small animation keeps this fast in debug builds
        let mut p = Project::new("tiny", 32.0, 32.0, 12.0, 6);
        for i in 0..4 {
            p.layers.push(Layer::vector(&format!("l{i}"), vec![VPath::rect("r", 2.0 + i as f32 * 6.0, 4.0, 8.0, 8.0).with_fill("#336699")]));
        }
        let a = Animation::from_json(&p.to_json_pretty()).unwrap();
        let r = a.fit_to_size(FitFormat::Lottie, 10_000_000).unwrap();
        assert!(r.met && r.size == r.bytes.len() && r.size <= 10_000_000 && r.ssim_min > 0.5, "{r:?}");
        let tiny = a.fit_to_size(FitFormat::Project, 1000).unwrap();
        assert!(tiny.size > 0 && (tiny.met == (tiny.size <= 1000)));
        assert!(a.fit_to_size(FitFormat::Html, 10).is_err() && a.fit_to_size(FitFormat::Gif, usize::MAX).is_err());
    }

    fn mini_bvh() -> String {
        let mut s = String::from("HIERARCHY\nROOT Hips\n{\n OFFSET 0 100 0\n CHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation\n");
        s.push_str(" JOINT Spine\n {\n  OFFSET 0 12 0\n  CHANNELS 3 Zrotation Xrotation Yrotation\n  JOINT Head\n  {\n   OFFSET 0 12 0\n   CHANNELS 3 Zrotation Xrotation Yrotation\n   End Site\n   {\n    OFFSET 0 8 0\n   }\n  }\n }\n}\nMOTION\nFrames: 4\nFrame Time: 0.0333333\n");
        for f in 0..4 {
            s.push_str(&format!("0 100 0 0 0 0 {} 0 0 0 0 0\n", f * 10));
        }
        s
    }

    #[test]
    fn retarget_bvh_writes_channels_in_one_undoable_edit() {
        use crate::math::Vec2;
        use crate::project::Bone;
        let mut p = Project::new("rig", 400.0, 400.0, 30.0, 10);
        p.bones.push(Bone::new("hips", None, 20.0, Vec2::new(200.0, 250.0), -90.0));
        p.bones.push(Bone::new("spine", Some("hips"), 40.0, Vec2::new(20.0, 0.0), 0.0));
        p.bones.push(Bone::new("head", Some("spine"), 30.0, Vec2::new(40.0, 0.0), 0.0));
        let mut a = Animation::from_json(&p.to_json_pretty()).unwrap();
        let before = a.to_json();
        let r = a.retarget_bvh(&mini_bvh(), &MocapSettings::new().with_smooth(1)).unwrap();
        assert!(r.frames >= 3, "{r:?}");
        assert!(r.matched.iter().any(|(j, b)| j == "Spine" && b == "spine"), "{r:?}");
        assert_ne!(a.to_json(), before);
        assert!(a.undo());
        assert_eq!(a.to_json(), before);
        assert!(a.retarget_bvh("not a bvh", &MocapSettings::default()).is_err());
        assert_eq!(a.to_json(), before);
        // a rig without matching bones is an error, not a panic
        let mut d = Animation::demo();
        let _ = d.retarget_bvh(&mini_bvh(), &MocapSettings::new().with_in_place(true).with_mirror(true));
    }

    #[test]
    fn tracing_and_fuzzing_validate_input() {
        let png = Animation::demo().render_png(0.0, &RenderSettings::new(0.25)).unwrap();
        let a = Animation::from_image(&png, 6, "traced").unwrap();
        assert!(!a.layer_names().is_empty());
        assert!(a.render_rgba(0.0, &RenderSettings::new(0.5)).is_ok());
        for bad in [0usize, 1, 65] {
            assert!(matches!(Animation::from_image(&png, bad, "x"), Err(ApiError::InvalidArgument(_))));
            assert!(trace_image_svg(&png, bad).is_err());
        }
        assert!(Animation::from_image(b"", 4, "x").is_err() && Animation::from_image(&png[..20], 4, "x").is_err());
        let f = Animation::demo().fuzz(30, 7).unwrap();
        assert_eq!(f.samples, 30);
        assert!(f.failure_rate >= 0.0 && f.failure_rate <= 1.0 && f.failures <= 30);
        let g = Animation::demo().fuzz(30, 7).unwrap();
        assert_eq!((g.failures, g.max_severity), (f.failures, f.max_severity), "same seed, same result");
        assert!(Animation::demo().fuzz(100_001, 1).is_err());
    }

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::ScratchDir::new(&format!("osatura-api-{tag}"))
    }

    #[test]
    fn vcs_log_reads_history_and_ci_run_reports() {
        let d = scratch("vcs");
        let f = d.join("p.json");
        std::fs::write(&f, Animation::demo().to_json()).unwrap();
        let mut repo = crate::vcs::Repo::init(&f, Some("ana")).unwrap();
        repo.fixed_time = Some(1_000);
        assert!(vcs_log(f.to_str().unwrap(), 10).unwrap().is_empty(), "no commits yet");
        repo.commit(Some("first"), None).unwrap();
        let mut a = Animation::from_json(&std::fs::read_to_string(&f).unwrap()).unwrap();
        let bone = a.bone_names()[0].clone();
        a.set_key(&bone, "bone_angle", 5.0, 33.0, Interpolation::Linear).unwrap();
        std::fs::write(&f, a.to_json()).unwrap();
        repo.fixed_time = Some(2_000);
        repo.commit(Some("second"), None).unwrap();
        let log = vcs_log(d.to_str().unwrap(), 10).unwrap();
        assert_eq!(log.iter().map(|c| c.message.as_str()).collect::<Vec<_>>(), ["second", "first"]);
        assert_eq!(log[0].author, "ana");
        assert_eq!(vcs_log(f.to_str().unwrap(), 1).unwrap().len(), 1);
        assert!(vcs_log(f.to_str().unwrap(), 0).is_err());
        assert!(vcs_log(d.join("nowhere").to_str().unwrap(), 5).is_err());
        // CI: a manifest with one structural check
        let m = d.join("ci.json");
        std::fs::write(&m, r#"{"schema":"osatura-ci/1","project":"p.json","goldens":"goldens","checks":[{"kind":"validate","name":"v"}]}"#).unwrap();
        match ci_run(m.to_str().unwrap()) {
            Ok(o) => {
                assert_eq!((o.total, o.passed, o.failed, o.errors, o.exit_code), (1, 1, 0, 0, 0), "{}", o.json);
                assert!(o.json.contains("\"v\""));
            }
            Err(e) => panic!("{e}"),
        }
        assert!(ci_run(d.join("missing.json").to_str().unwrap()).is_err());
        std::fs::write(&m, "{ nope").unwrap();
        assert!(ci_run(m.to_str().unwrap()).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn vcs_write_roundtrip_with_conflicts_and_typed_errors() {
        let d = scratch("vcswrite");
        let f = d.join("p.json");
        std::fs::write(&f, Animation::demo().to_json()).unwrap();
        let p = f.to_str().unwrap();
        let edit = |angle: f32, frame: f32| {
            let mut a = Animation::from_json(&std::fs::read_to_string(&f).unwrap()).unwrap();
            let bone = a.bone_names()[0].clone();
            a.set_key(&bone, "bone_angle", frame, angle, Interpolation::Linear).unwrap();
            std::fs::write(&f, a.to_json()).unwrap();
        };
        assert!(vcs_commit(p, Some("x"), None).is_err(), "no repository yet");
        vcs_init(p, Some("ana")).unwrap();
        assert!(vcs_init(p, None).is_err());
        let base = vcs_commit(p, Some("base"), None).unwrap();
        assert!(vcs_commit(p, Some("same"), None).is_err());
        assert_eq!(vcs_tag(p, "v1", None).unwrap(), base);
        assert!(vcs_tag(p, "bad name", None).is_err());
        vcs_branch(p, "other", Some("v1")).unwrap();
        edit(20.0, 5.0);
        // dirty tree: typed error for checkout, merge, cherry-pick, revert
        assert!(matches!(vcs_checkout(p, "other", false), Err(ApiError::DirtyTree(_))));
        assert!(matches!(vcs_merge(p, "other", false, None, None), Err(ApiError::DirtyTree(_))));
        assert!(matches!(vcs_cherry_pick(p, &base, None), Err(ApiError::DirtyTree(_))));
        assert!(matches!(vcs_revert(p, &base, None), Err(ApiError::DirtyTree(_))));
        vcs_commit(p, Some("main edit"), None).unwrap();
        vcs_checkout(p, "other", false).unwrap();
        edit(40.0, 5.0);
        vcs_commit(p, Some("other edit"), None).unwrap();
        vcs_checkout(p, "main", false).unwrap();
        let conflicts = match vcs_merge(p, "other", false, None, None).unwrap() {
            VcsMergeOutcome::Conflicted(c) => c,
            o => panic!("{o:?}"),
        };
        assert!(!conflicts.is_empty() && conflicts.iter().all(|c| !c.path.is_empty() && !c.kind.is_empty()));
        assert!(conflicts.iter().any(|c| c.ours.is_some() && c.theirs.is_some()));
        assert_eq!(vcs_conflicts(p).unwrap(), conflicts);
        // while merging: typed errors
        assert!(matches!(vcs_commit(p, None, None), Err(ApiError::Conflicts(v)) if v.len() == conflicts.len()));
        assert!(matches!(vcs_checkout(p, "other", true), Err(ApiError::MergeInProgress(_))));
        assert!(matches!(vcs_merge(p, "other", false, None, None), Err(ApiError::MergeInProgress(_))));
        assert!(matches!(vcs_revert(p, &base, None), Err(ApiError::MergeInProgress(_))));
        assert!(vcs_resolve(p, "/nowhere", &VcsChoice::Ours).is_err());
        assert!(matches!(vcs_resolve(p, &conflicts[0].path, &VcsChoice::Value("{ nope".into())), Err(ApiError::InvalidJson(_))));
        // abort restores the committed state
        vcs_abort(p).unwrap();
        assert!(vcs_conflicts(p).unwrap().is_empty());
        assert!(vcs_abort(p).is_err());
        // merge again and resolve each side
        let VcsMergeOutcome::Conflicted(c2) = vcs_merge(p, "other", false, None, None).unwrap() else { panic!() };
        let mut left = c2.len();
        for c in &c2 {
            left = vcs_resolve(p, &c.path, &VcsChoice::Ours).unwrap();
        }
        assert_eq!(left, 0);
        let merged = vcs_commit(p, None, None).unwrap();
        assert_eq!(vcs_log(p, 1).unwrap()[0].id, merged);
        assert_eq!(vcs_merge(p, "other", false, None, None).unwrap(), VcsMergeOutcome::UpToDate);
        // revert the merge's first-parent change, then cherry-pick it back via the original commit
        let log = vcs_log(p, 10).unwrap();
        let main_edit = log.iter().find(|c| c.message == "main edit").unwrap().id.clone();
        assert!(matches!(vcs_revert(p, &main_edit, None).unwrap(), VcsMergeOutcome::Merged(_) | VcsMergeOutcome::Conflicted(_)));
        let _ = vcs_abort(p);
        // fast-forward
        vcs_branch(p, "ahead", None).unwrap();
        vcs_checkout(p, "ahead", false).unwrap();
        edit(1.0, 9.0);
        let ahead = vcs_commit(p, Some("ahead"), None).unwrap();
        vcs_checkout(p, "main", false).unwrap();
        assert_eq!(vcs_merge(p, "ahead", false, None, None).unwrap(), VcsMergeOutcome::FastForward(ahead));
        assert!(vcs_merge(p, "no-such-rev", false, None, None).is_err());
        assert!(vcs_branch(p, "x", Some("no-such-rev")).is_err());
        let _ = std::fs::remove_dir_all(&d);
        assert!(!d.exists());
    }

    #[test]
    fn ci_modes_init_and_bless() {
        let d = scratch("ciwrite");
        let f = d.join("p.json");
        std::fs::write(&f, Animation::demo().to_json()).unwrap();
        let m = d.join("ci.json");
        std::fs::write(&m, r#"{"schema":"osatura-ci/1","project":"p.json","goldens":"goldens","checks":[{"kind":"render","name":"f","frames":[0,3],"scale":0.25}]}"#).unwrap();
        let ms = m.to_str().unwrap();
        let v = ci_run_mode(ms, CiMode::Verify).unwrap();
        assert_eq!((v.failed, v.exit_code), (1, 1), "{}", v.json);
        let u = ci_run_mode(ms, CiMode::Update).unwrap();
        assert_eq!((u.exit_code, u.updated), (0, 1), "{}", u.json);
        assert_eq!(ci_run(ms).unwrap().passed, 1);
        // change the animation: verify fails, bless accepts, verify passes
        let mut a = Animation::from_json(&std::fs::read_to_string(&f).unwrap()).unwrap();
        let bone = a.bone_names()[0].clone();
        for fr in 0..4 {
            a.set_key(&bone, "bone_angle", fr as f32, 80.0 + fr as f32 * 10.0, Interpolation::Linear).unwrap();
        }
        std::fs::write(&f, a.to_json()).unwrap();
        assert_eq!(ci_run(ms).unwrap().exit_code, 1);
        let b = ci_bless(ms).unwrap();
        assert_eq!((b.exit_code, b.blessed), (0, 1), "{}", b.json);
        assert_eq!(ci_run(ms).unwrap().exit_code, 0);
        // init: default manifest, no overwrite, optional goldens
        let f2 = d.join("q.json");
        std::fs::write(&f2, Animation::demo().to_json()).unwrap();
        let i = ci_init(f2.to_str().unwrap(), Some(d.join("q-ci.json").to_str().unwrap()), false).unwrap();
        assert!(i.recorded.is_none() && i.checks.len() >= 4 && std::path::Path::new(&i.manifest).exists());
        assert!(ci_init(f2.to_str().unwrap(), Some(&i.manifest), false).is_err());
        assert!(ci_init(d.join("none.json").to_str().unwrap(), Some(d.join("z.json").to_str().unwrap()), false).is_err());
        assert!(!d.join("z.json").exists());
        assert!(ci_run_mode(d.join("missing.json").to_str().unwrap(), CiMode::Bless).is_err());
        let _ = std::fs::remove_dir_all(&d);
        assert!(!d.exists());
    }

    #[test]
    fn new_error_variants_display() {
        assert_eq!(ApiError::DirtyTree("x".into()).to_string(), "uncommitted changes: x");
        assert_eq!(ApiError::MergeInProgress("y".into()).to_string(), "merge in progress: y");
        assert_eq!(ApiError::Conflicts(vec!["a".into(), "b".into()]).to_string(), "unresolved conflicts: a, b");
    }

    #[test]
    fn error_display_and_trait() {
        let e: Box<dyn std::error::Error> = Box::new(ApiError::InvalidProject(vec!["a".into(), "b".into()]));
        assert_eq!(e.to_string(), "invalid project: a; b");
        assert!(API_VERSION.starts_with(&format!("{API_VERSION_MAJOR}.")));
    }

    // ----- 1.4 additions -----

    #[test]
    fn create_layers_and_bones_are_validated_and_undoable() {
        let mut a = Animation::demo();
        let n0 = a.layer_names().len();
        a.add_group_layer("grp").unwrap();
        a.add_rect_layer("box", 1.0, 2.0, 30.0, 20.0, "#336699", Some("grp")).unwrap();
        a.add_ellipse_layer("dot", 50.0, 50.0, 5.0, 8.0, "#f00", None).unwrap();
        assert_eq!(a.layer_names().len(), n0 + 3);
        assert!(a.layer_names().iter().any(|n| n == "grp/box"));
        assert!(a.render_rgba(0.0, &RenderSettings::new(0.2)).is_ok());
        // round-trips through JSON
        assert!(Animation::from_json(&a.to_json()).is_ok());
        for bad in [
            a.add_rect_layer("box", 0.0, 0.0, 1.0, 1.0, "#fff", None),          // duplicate name
            a.add_rect_layer("x/y", 0.0, 0.0, 1.0, 1.0, "#fff", None),          // slash
            a.add_rect_layer("", 0.0, 0.0, 1.0, 1.0, "#fff", None),             // empty
            a.add_rect_layer("n1", 0.0, 0.0, -1.0, 1.0, "#fff", None),          // size
            a.add_rect_layer("n2", f32::NAN, 0.0, 1.0, 1.0, "#fff", None),      // NaN
            a.add_rect_layer("n3", 0.0, 0.0, 1.0, 1.0, "red-ish", None),        // colour
            a.add_rect_layer("n4", 0.0, 0.0, 1.0, 1.0, "#fff", Some("dot")),    // parent is not a group
            a.add_ellipse_layer("n5", 0.0, 0.0, 0.0, 1.0, "#fff", None),
        ] {
            assert!(matches!(bad, Err(ApiError::InvalidArgument(_))), "{bad:?}");
        }
        assert_eq!(a.layer_names().len(), n0 + 3, "failed adds change nothing");
        assert!(a.undo() && a.undo() && a.undo());
        assert_eq!(a.layer_names().len(), n0);

        let b0 = a.bone_names().len();
        let root = a.bone_names()[0].clone();
        a.add_bone("tip", Some(&root), 12.0, 3.0, 0.0, 15.0).unwrap();
        assert_eq!(a.bone_names().len(), b0 + 1);
        a.set_key("tip", "bone_angle", 4.0, 10.0, Interpolation::Linear).unwrap();
        assert!(a.add_bone("tip", None, 1.0, 0.0, 0.0, 0.0).is_err());
        assert!(a.add_bone("t2", Some("nope"), 1.0, 0.0, 0.0, 0.0).is_err());
        assert!(a.add_bone("t3", None, 0.0, 0.0, 0.0, 0.0).is_err());
        assert!(a.add_bone("t4", None, 1.0, f32::INFINITY, 0.0, 0.0).is_err());
    }

    #[test]
    fn apng_and_exr_exports() {
        let a = Animation::demo();
        let apng = a.export(ExportFormat::Apng, 0.25).unwrap();
        assert_eq!(&apng[1..4], b"PNG");
        assert!(apng.windows(4).any(|w| w == b"acTL"), "animation control chunk");
        let (_, _, frames) = crate::sizefit::apng_decode(&apng).unwrap();
        assert_eq!(frames.len(), a.frame_count() as usize);
        assert!(a.export(ExportFormat::Apng, 0.0).is_err());
        let half = a.render_exr(0.0, &RenderSettings::new(0.25), false).unwrap();
        let full = a.render_exr(0.0, &RenderSettings::new(0.25), true).unwrap();
        assert_eq!(&half[0..4], &[0x76, 0x2f, 0x31, 0x01], "EXR magic");
        assert_eq!(&full[0..4], &[0x76, 0x2f, 0x31, 0x01]);
        assert!(full.len() > half.len());
        assert!(a.render_exr(f32::NAN, &RenderSettings::default(), false).is_err());
        assert!(a.render_exr(0.0, &RenderSettings::new(99.0), false).is_err());
    }

    #[test]
    fn physics_bake_adds_channels_once_and_is_undoable() {
        let mut a = Animation::demo();
        a.add_bone("h1", None, 20.0, 10.0, 10.0, 90.0).unwrap();
        a.add_bone("h2", Some("h1"), 20.0, 20.0, 0.0, 0.0).unwrap();
        let before = a.to_json();
        let r = a.bake_physics(&["h1", "h2"], "hair", false).unwrap();
        assert_eq!(r.added, 2);
        assert!(a.to_json() != before);
        let again = a.bake_physics(&["h1", "h2"], "hair", false).unwrap();
        assert_eq!(again.added, 0, "existing keys win");
        assert_eq!(again.skipped.len(), 2);
        assert_eq!(a.bake_physics(&["h1", "h2"], "cape", true).unwrap().added, 2);
        assert!(a.undo() && a.undo());
        assert_eq!(a.to_json(), before);
        assert!(a.bake_physics(&["h1"], "lava", false).is_err());
        assert!(a.bake_physics(&[], "hair", false).is_err());
        assert!(a.bake_physics(&["ghost"], "hair", false).is_err());
    }

    #[test]
    fn vcs_branch_list_delete_status_diff_show_gc_fsck() {
        let dir = std::env::temp_dir().join(format!("osatura-api14-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("p.json");
        std::fs::write(&file, Animation::demo().to_json()).unwrap();
        let path = file.to_str().unwrap();
        vcs_init(path, Some("ana")).unwrap();
        assert!(vcs_branches(path).unwrap().is_empty() || vcs_branches(path).unwrap().iter().all(|b| b.current));
        let c1 = vcs_commit(path, Some("first"), None).unwrap();
        let st = vcs_status(path).unwrap();
        assert_eq!(st.head.as_deref(), Some(c1.as_str()));
        assert!(st.changes.is_empty() && st.merge.is_none() && st.conflicts.is_empty());

        // edit the working file
        let mut a = Animation::from_json(&std::fs::read_to_string(&file).unwrap()).unwrap();
        a.add_rect_layer("extra", 0.0, 0.0, 5.0, 5.0, "#0f0", None).unwrap();
        std::fs::write(&file, a.to_json()).unwrap();
        let st = vcs_status(path).unwrap();
        assert!(st.changes.iter().any(|c| c.kind == "Added" && c.path.contains("extra") || c.description.contains("extra")), "{:?}", st.changes);
        let d = vcs_diff(path, Some("HEAD"), None).unwrap();
        assert_eq!(d, st.changes);
        assert!(vcs_diff(path, Some("HEAD"), Some("HEAD")).unwrap().is_empty());
        let c2 = vcs_commit(path, Some("second"), None).unwrap();
        let shown = vcs_show(path, &c2).unwrap();
        assert_eq!(shown.commit.id, c2);
        assert_eq!(shown.commit.message, "second");
        assert!(!shown.changes.is_empty());
        assert!(!vcs_show(path, &c1).unwrap().changes.is_empty(), "a root commit shows everything");
        assert!(vcs_show(path, "nope").is_err());

        vcs_branch(path, "feature", Some(&c1)).unwrap();
        let bs = vcs_branches(path).unwrap();
        assert!(bs.iter().any(|b| b.name == "feature" && !b.current && b.commit == c1));
        let cur = bs.iter().find(|b| b.current).unwrap().name.clone();
        assert!(vcs_branch_delete(path, &cur).is_err(), "cannot delete the checked-out branch");
        assert!(vcs_branch_delete(path, "ghost").is_err());
        vcs_branch_delete(path, "feature").unwrap();
        assert!(!vcs_branches(path).unwrap().iter().any(|b| b.name == "feature"));

        assert!(vcs_fsck(path).unwrap().is_empty());
        let g = vcs_gc(path).unwrap();
        assert!(g.kept > 0);
        assert!(vcs_fsck(path).unwrap().is_empty());
        assert!(vcs_status("/nonexistent-osatura-dir/p.json").is_err());
        assert!(vcs_gc("/nonexistent-osatura-dir/p.json").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Surface snapshot: the public items of this file must equal `api_surface.txt`.
    /// Regenerate deliberately with `OSATURA_BLESS=1 cargo test -p osatura-core api::`.
    #[test]
    fn api_surface_snapshot() {
        let src = include_str!("api.rs");
        let mut items: Vec<String> = Vec::new();
        let mut ctx = String::new();
        let mut in_enum = false;
        for line in src.lines() {
            if line.starts_with("#[cfg(test)]") {
                break;
            }
            if !line.starts_with(' ') && !line.starts_with('#') && !line.starts_with("//") && !line.is_empty() && !line.starts_with('}') {
                in_enum = line.starts_with("pub enum ");
                ctx = if line.starts_with("impl") || line.starts_with("pub struct") || line.starts_with("pub enum") {
                    line.trim_end_matches('{').trim().to_string()
                } else {
                    String::new()
                };
            }
            let t = line.trim_end();
            let top = !line.starts_with(' ');
            if line.trim_end() == "#[non_exhaustive]" {
                items.push(line.trim_end().to_string());
            }
            let item = if top && t.starts_with("pub ") {
                Some(t.trim_end_matches('{').trim_end_matches(';').trim().to_string())
            } else if !top && line.starts_with("    ") && !line.starts_with("     ") {
                let b = t.trim_start();
                if b.starts_with("pub ") {
                    Some(format!("{ctx} :: {}", b.trim_end_matches('{').trim_end_matches(',').trim_end_matches(';').trim()))
                } else if in_enum && b.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                    Some(format!("{ctx} :: {}", b.trim_end_matches(',')))
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(i) = item {
                if !i.contains("pub use") || top {
                    items.push(i);
                }
            }
        }
        let got = items.join("\n") + "\n";
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/api_surface.txt");
        if std::env::var("OSATURA_BLESS").is_ok() {
            std::fs::write(path, &got).unwrap();
        }
        let want = include_str!("api_surface.txt");
        assert_eq!(got, want, "public API surface changed: if intended, bump API_VERSION and rerun with OSATURA_BLESS=1");
    }
}
