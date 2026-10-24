// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
use crate::expr::{self, Ctx, Expr};
use crate::math::{Affine2, Vec2};
use crate::noise::{fbm1, noise1};
use crate::path::{Polyline, VPath};
use crate::project::{
    compose_transform, ConstraintKind, Deformer, ImageSource, Layer, LayerKind, Mode, Project, Prop, Source,
    TargetKind, Wobble,
};
use crate::rig::{fk, resolve_parents, soft_distance, solve_fabrik, solve_two_bone_scaled, wrap_pi, BonePose, SpringState};
use crate::style::Blend;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tiny_skia as sk;

// ---------------------------------------------------------------------------------------------
// Resolved (index-based) structures
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct IkResolved {
    bones: Vec<usize>,
    target: usize,
    bend: f32,
    iterations: u32,
    soft: f32,
    stretch: f32,
}

#[derive(Clone, Copy, Debug)]
enum CKind {
    Aim { target: usize, offset: f32 },
    CopyRotation { source: usize },
    Pin { target: usize },
}

#[derive(Clone, Copy, Debug)]
struct ConstraintResolved {
    bone: usize,
    kind: CKind,
}

#[derive(Clone, Copy, Debug)]
struct SpringResolved {
    bone: usize,
    frequency: f32,
    damping: f32,
    warmup: bool,
}

struct ChanResolved {
    prop: Prop,
    /// Bone / node / ik / constraint index, 0 (camera), or a path-slot index.
    slot: usize,
    /// Index into `project.channels`.
    chan: usize,
    expr: Option<Expr>,
    /// Driven keys: the input value to read.
    input: Option<(Prop, usize)>,
}

struct NodeInfo {
    path: String,
    parent: Option<usize>,
    children: Vec<usize>,
    /// Index path into `project.layers` / `children()`.
    idx_path: Vec<usize>,
    bone: Option<usize>,
    mask: Option<(usize, bool)>,
    /// Top-level ancestor (its `parallax` governs the whole subtree).
    root: usize,
}

/// Geometry bookkeeping for one vector layer.
#[derive(Default)]
struct NodeGeo {
    /// per path: slot of each morph's weight
    morph_slots: Vec<Vec<usize>>,
    /// per path, per anchor: `[dx slot, dy slot]`
    point_slots: Vec<Vec<[usize; 2]>>,
    /// per path: `[fill.from, fill.to, stroke.from, stroke.to]` gradient handle slots `[dx, dy]` (only for gradient paints)
    grad_slots: Vec<[Option<[usize; 2]>; 4]>,
    deformers: Vec<DefInfo>,
}

enum DefInfo {
    Lattice { cols: usize, rows: usize, rect: [f32; 4], slots: Vec<[usize; 2]> },
    Cloth { cols: usize, rows: usize, rect: [f32; 4], slots: Vec<[usize; 2]> },
    Spine { rest_pts: Vec<Vec2>, rest_poly: Polyline, slots: Vec<[usize; 2]>, radius: Option<f32> },
    Pin { solver: crate::pinwarp::PinSolver, slots: Vec<[usize; 2]> },
    /// `mesh_w`: mean-value weights of every rest mesh vertex (empty list = outside the cage),
    /// cached here when the cage is the layer's first deformer on an image layer.
    Cage { solver: crate::cage::CageSolver, slots: Vec<[usize; 2]>, mesh_w: Vec<Vec<f64>> },
}

#[derive(Clone, Copy, Debug)]
struct LayerProps {
    x: f32,
    y: f32,
    rot: f32,
    sx: f32,
    sy: f32,
    skew: f32,
    opacity: f32,
    switch: f32,
    wobble: f32,
}

#[derive(Clone, Copy, Debug)]
struct CamProps {
    x: f32,
    y: f32,
    zoom: f32,
    rot: f32,
    focus: f32,
    blur: f32,
}

#[derive(Clone)]
struct State {
    bones: Vec<BonePose>,
    layers: Vec<LayerProps>,
    cam: CamProps,
    ik_mix: Vec<f32>,
    cweight: Vec<f32>,
    pvals: Vec<f32>,
}

impl State {
    /// Read a value in channel units (degrees for angles).
    fn read(&self, prop: Prop, slot: usize) -> f32 {
        match prop {
            Prop::BoneAngle => self.bones[slot].angle.to_degrees(),
            Prop::BoneScale => self.bones[slot].scale,
            Prop::BoneOffsetX => self.bones[slot].offset.x,
            Prop::BoneOffsetY => self.bones[slot].offset.y,
            Prop::LayerOpacity => self.layers[slot].opacity,
            Prop::LayerX => self.layers[slot].x,
            Prop::LayerY => self.layers[slot].y,
            Prop::LayerRotation => self.layers[slot].rot,
            Prop::LayerScaleX => self.layers[slot].sx,
            Prop::LayerScaleY => self.layers[slot].sy,
            Prop::LayerSkew => self.layers[slot].skew,
            Prop::LayerSwitch => self.layers[slot].switch,
            Prop::LayerWobble => self.layers[slot].wobble,
            Prop::CameraX => self.cam.x,
            Prop::CameraY => self.cam.y,
            Prop::CameraZoom => self.cam.zoom,
            Prop::CameraRotation => self.cam.rot,
            Prop::CameraFocus => self.cam.focus,
            Prop::CameraBlur => self.cam.blur,
            Prop::IkMix => self.ik_mix[slot],
            Prop::ConstraintWeight => self.cweight[slot],
            Prop::PathMorph
            | Prop::PointDx
            | Prop::PointDy
            | Prop::LatticeDx
            | Prop::LatticeDy
            | Prop::SpineDx
            | Prop::SpineDy
            | Prop::PinDx
            | Prop::PinDy
            | Prop::CageDx
            | Prop::CageDy
            | Prop::FxBlur
            | Prop::FxDx
            | Prop::FxDy
            | Prop::FxSoft
            | Prop::FxAmount
            | Prop::GradDx
            | Prop::GradDy => self.pvals[slot],
        }
    }

    fn write(&mut self, prop: Prop, slot: usize, v: f32) {
        match prop {
            Prop::BoneAngle => self.bones[slot].angle = v.to_radians(),
            Prop::BoneScale => self.bones[slot].scale = v,
            Prop::BoneOffsetX => self.bones[slot].offset.x = v,
            Prop::BoneOffsetY => self.bones[slot].offset.y = v,
            Prop::LayerOpacity => self.layers[slot].opacity = v.clamp(0.0, 1.0),
            Prop::LayerX => self.layers[slot].x = v,
            Prop::LayerY => self.layers[slot].y = v,
            Prop::LayerRotation => self.layers[slot].rot = v,
            Prop::LayerScaleX => self.layers[slot].sx = v,
            Prop::LayerScaleY => self.layers[slot].sy = v,
            Prop::LayerSkew => self.layers[slot].skew = v,
            Prop::LayerSwitch => self.layers[slot].switch = v,
            Prop::LayerWobble => self.layers[slot].wobble = v,
            Prop::CameraX => self.cam.x = v,
            Prop::CameraY => self.cam.y = v,
            Prop::CameraZoom => self.cam.zoom = v.max(0.01),
            Prop::CameraRotation => self.cam.rot = v,
            Prop::CameraFocus => self.cam.focus = v,
            Prop::CameraBlur => self.cam.blur = v.max(0.0),
            Prop::IkMix => self.ik_mix[slot] = v.clamp(0.0, 1.0),
            Prop::ConstraintWeight => self.cweight[slot] = v.clamp(0.0, 1.0),
            Prop::PathMorph
            | Prop::PointDx
            | Prop::PointDy
            | Prop::LatticeDx
            | Prop::LatticeDy
            | Prop::SpineDx
            | Prop::SpineDy
            | Prop::PinDx
            | Prop::PinDy
            | Prop::CageDx
            | Prop::CageDy
            | Prop::FxBlur
            | Prop::FxDx
            | Prop::FxDy
            | Prop::FxSoft
            | Prop::FxAmount
            | Prop::GradDx
            | Prop::GradDy => self.pvals[slot] = v,
        }
    }
}

/// Moves the handles of a gradient: linear `from` / `to` by their offsets; radial centre by `from`, radius by `to.x`.
fn shift_gradient(p: &mut crate::style::Paint, from: Vec2, to: Vec2) {
    match p {
        crate::style::Paint::Linear { from: f, to: t, .. } => {
            *f += from;
            *t += to;
        }
        crate::style::Paint::Radial { center, radius, .. } => {
            *center += from;
            *radius = (*radius + to.x).max(0.0);
        }
        crate::style::Paint::Solid(_) => {}
    }
}

// ---------------------------------------------------------------------------------------------
// Public output types
// ---------------------------------------------------------------------------------------------

/// A decoded image plus its original PNG bytes (for SVG embedding).
pub struct ImageData {
    pub pixmap: sk::Pixmap,
    pub png: Vec<u8>,
}

impl std::fmt::Debug for ImageData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ImageData({}×{})", self.pixmap.width(), self.pixmap.height())
    }
}

/// A deformed triangle mesh (positions in layer space, UVs in image pixels).
#[derive(Clone, Debug)]
pub struct MeshOut {
    pub positions: Vec<Vec2>,
    pub uvs: Vec<Vec2>,
    pub tris: Vec<[u32; 3]>,
}

#[derive(Clone, Debug)]
pub struct ImageDraw {
    pub data: Arc<ImageData>,
    pub origin: Vec2,
    pub size: Vec2,
    pub mesh: Option<MeshOut>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Vector,
    Group,
    Switch,
    Image,
}

/// One layer, posed and ready to draw.
#[derive(Clone, Debug)]
pub struct RenderNode {
    pub id: usize,
    /// `/`-joined layer path.
    pub name: String,
    pub kind: NodeKind,
    /// Layer space → screen (includes camera). Geometry in `paths` is in layer space.
    pub world: Affine2,
    pub opacity: f32,
    pub blend: Blend,
    pub visible: bool,
    /// `(mask node id, inverted)`.
    pub mask: Option<(usize, bool)>,
    /// Deformed geometry (vector layers).
    pub paths: Vec<VPath>,
    /// Image layers.
    pub image: Option<ImageDraw>,
    /// Children to draw, bottom to top. For switch layers only the selected child.
    pub children: Vec<usize>,
    /// Enabled layer effects with their values at this frame (see `layerfx`); empty for most layers.
    pub effects: Vec<crate::layerfx::LayerEffect>,
}

/// Everything needed to draw one frame.
pub struct Frame {
    /// Bone world transforms (stage space).
    pub world: Vec<Affine2>,
    /// Camera matrix at parallax 1 (stage → screen); use it to draw stage-space overlays.
    pub camera: Affine2,
    pub nodes: Vec<RenderNode>,
    pub roots: Vec<usize>,
    /// Set when a computed number was not finite (a value overflowed `f32`, for instance a deep chain of large
    /// scales). The affected nodes are hidden and their transforms reset so nothing downstream sees NaN or
    /// infinity; this says where it happened. Exporters that can report errors refuse such a frame
    /// ([`Frame::check_finite`]).
    pub nonfinite: Option<String>,
}

impl Frame {
    /// `Err("non-finite value in <place>")` if evaluation had to repair a non-finite number.
    pub fn check_finite(&self) -> Result<(), String> {
        match &self.nonfinite {
            Some(w) => Err(format!("non-finite value in {w}")),
            None => Ok(()),
        }
    }
}

fn aff_finite(m: &Affine2) -> bool {
    [m.a, m.b, m.c, m.d, m.tx, m.ty].iter().all(|v| v.is_finite())
}

/// Largest absolute value of any floating point number in a project (coordinates, scales, offsets, key values, ...).
pub const MAX_MAGNITUDE: f64 = 1.0e7;

fn find_big(v: &serde_json::Value, path: &mut String) -> bool {
    match v {
        serde_json::Value::Number(n) => n.is_f64() && n.as_f64().is_some_and(|x| !matches!(x.abs().partial_cmp(&MAX_MAGNITUDE), Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal))),
        serde_json::Value::Array(a) => a.iter().enumerate().any(|(i, x)| {
            let keep = path.len();
            path.push_str(&format!("/{i}"));
            if find_big(x, path) {
                return true;
            }
            path.truncate(keep);
            false
        }),
        serde_json::Value::Object(m) => m.iter().any(|(k, x)| {
            let keep = path.len();
            path.push_str(&format!("/{k}"));
            if find_big(x, path) {
                return true;
            }
            path.truncate(keep);
            false
        }),
        _ => false,
    }
}

/// Every floating point number in the project must lie within +-[`MAX_MAGNITUDE`] (so products of a few of them
/// still fit in `f32`); returns the JSON pointer of the first one that does not.
pub fn check_magnitudes(project: &Project) -> Result<(), String> {
    let v = serde_json::to_value(project).map_err(|e| e.to_string())?;
    check_json_magnitudes(&v)
}

/// [`check_magnitudes`] on JSON text that is already parsed. Reading a file goes through this, because a literal
/// such as `1e300` becomes infinity in an `f32` field and would then serialise as `null`, out of sight.
pub fn check_json_magnitudes(v: &serde_json::Value) -> Result<(), String> {
    let mut path = String::new();
    if find_big(v, &mut path) {
        let mut cur = v;
        for part in path.split('/').skip(1) {
            cur = match cur {
                serde_json::Value::Array(a) => &a[part.parse::<usize>().unwrap_or(0)],
                serde_json::Value::Object(m) => &m[part],
                x => x,
            };
        }
        return Err(format!("value {cur} at {path} is outside the supported range of +-{MAX_MAGNITUDE:e} (coordinates, scales, offsets and key values must stay below it)"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------------------------

/// Per anchor: `(bone, normalised weight)` pairs.
type AnchorWeights = Vec<(usize, f32)>;

/// A validated project with all names resolved to indices.
pub struct Engine {
    pub project: Project,
    /// Draw layers flagged `reference` (editor previews); exports leave this off.
    pub show_reference: bool,
    parent: Vec<Option<usize>>,
    ik: Vec<IkResolved>,
    constraints: Vec<ConstraintResolved>,
    springs: Vec<SpringResolved>,
    /// frame → spring → absolute local pose angle (radians)
    baked: Vec<Vec<f32>>,
    inv_rest: Vec<Affine2>,
    nodes: Vec<NodeInfo>,
    roots: Vec<usize>,
    /// node → path → anchor → (bone, normalised weight); empty for non-vector nodes
    skin: Vec<Vec<Vec<AnchorWeights>>>,
    /// node → mesh vertex → (bone, weight) for image meshes
    mesh_skin: Vec<Vec<Vec<(usize, f32)>>>,
    geo: Vec<NodeGeo>,
    /// node → per effect (index in `Layer::effects`) → animatable `(prop, slot)` pairs
    fx_slots: Vec<Vec<Vec<(Prop, usize)>>>,
    images: Vec<Option<Arc<ImageData>>>,
    chans: Vec<ChanResolved>,
    bone_ix: HashMap<String, usize>,
    node_ix: HashMap<String, usize>,
    /// `(target, prop)` → slot, for every addressable path/deformer sub-target
    slot_ix: HashMap<(String, Prop), usize>,
    pslot_init: Vec<f32>,
    /// Resolved mesh locks and glues (`links.rs`); `None` when the project has none.
    links: Option<crate::links::Resolved>,
    /// Container layout per node: `[dx, dy, kx, ky]` (`containers.rs`); empty when unused.
    layout: Vec<Option<[f32; 4]>>,
}

fn layer_at<'a>(p: &'a Project, idx: &[usize]) -> &'a Layer {
    let mut cur = &p.layers[idx[0]];
    for &i in &idx[1..] {
        cur = &cur.children()[i];
    }
    cur
}

#[allow(clippy::too_many_arguments)]
fn build_nodes(
    layers: &[Layer],
    parent: Option<usize>,
    prefix: &str,
    idx_prefix: &[usize],
    root: Option<usize>,
    nodes: &mut Vec<NodeInfo>,
    node_ix: &mut HashMap<String, usize>,
    errs: &mut Vec<String>,
) {
    for (i, l) in layers.iter().enumerate() {
        let path = if prefix.is_empty() { l.name.clone() } else { format!("{prefix}/{}", l.name) };
        let id = nodes.len();
        let mut idx_path = idx_prefix.to_vec();
        idx_path.push(i);
        if l.name.is_empty() || l.name.contains('/') || l.name.contains("::") {
            errs.push(format!("layer name '{}' must be non-empty and contain neither '/' nor '::'", l.name));
        }
        if node_ix.insert(path.clone(), id).is_some() {
            errs.push(format!("duplicate layer path '{path}'"));
        }
        nodes.push(NodeInfo {
            path: path.clone(),
            parent,
            children: vec![],
            idx_path: idx_path.clone(),
            bone: None,
            mask: None,
            root: root.unwrap_or(id),
        });
        if let Some(p) = parent {
            nodes[p].children.push(id);
        }
        build_nodes(l.children(), Some(id), &path, &idx_path, Some(root.unwrap_or(id)), nodes, node_ix, errs);
    }
}

/// Structural checks of a free-form mesh (finite coordinates, valid indices, matching UV count).
fn validate_free_mesh(f: &crate::project::FreeMesh) -> Result<(), String> {
    if f.vertices.len() < 3 || f.triangles.is_empty() {
        return Err("needs at least 3 vertices and 1 triangle".into());
    }
    if f.vertices.len() > 200_000 || f.triangles.len() > 400_000 {
        return Err("too large (max 200000 vertices / 400000 triangles)".into());
    }
    if f.vertices.iter().any(|v| !v.x.is_finite() || !v.y.is_finite()) {
        return Err("vertex coordinates must be finite".into());
    }
    if !f.uvs.is_empty() && (f.uvs.len() != f.vertices.len() || f.uvs.iter().any(|v| !v.x.is_finite() || !v.y.is_finite())) {
        return Err("uvs must be empty or one finite pair per vertex".into());
    }
    let n = f.vertices.len() as u32;
    if let Some(t) = f.triangles.iter().find(|t| t.iter().any(|&i| i >= n)) {
        return Err(format!("triangle {:?} refers to a missing vertex (have {n})", t));
    }
    Ok(())
}

fn load_image(src: &ImageSource, asset_dir: &std::path::Path) -> Result<ImageData, String> {
    let png = match (&src.png_base64, &src.file) {
        (Some(b64), _) => crate::base64::decode(b64)?,
        (None, Some(f)) => crate::assets::read(asset_dir, f, crate::assets::MAX_IMAGE_BYTES, "image")?,
        (None, None) => return Err("image has neither 'png_base64' nor 'file'".into()),
    };
    let pixmap = crate::assets::decode_png_checked(&png)?;
    Ok(ImageData { pixmap, png })
}

fn bernstein(n: usize, i: usize, t: f32) -> f32 {
    let mut c = 1.0f32;
    for k in 0..i {
        c = c * (n - k) as f32 / (k + 1) as f32;
    }
    c * t.powi(i as i32) * (1.0 - t).powi((n - i) as i32)
}

fn wobble_disp(p: Vec2, tick: f32, w: &Wobble, amp: f32) -> Vec2 {
    let k = w.scale;
    Vec2::new(
        noise1(p.y * k + tick * 13.37, w.seed),
        noise1(p.x * k + tick * 7.77 + 100.0, w.seed.wrapping_add(1)),
    ) * amp
}

/// Resource caps that keep a corrupt or hostile file from exhausting memory or time.
pub const MAX_FRAMES: u32 = 200_000;
pub const MAX_STAGE: f32 = 32_768.0;
pub const MAX_BONES: usize = 4096;

impl Engine {
    /// Validate and compile (assets resolve relative to `assets::default_dir()`: the current directory unless the command line set it to the loaded project's directory).
    pub fn new(project: Project) -> Result<Engine, Vec<String>> {
        Engine::with_asset_dir(project, &crate::assets::default_dir())
    }

    /// Validate and compile. Returns every problem found, not just the first.
    pub fn with_asset_dir(mut project: Project, asset_dir: &std::path::Path) -> Result<Engine, Vec<String>> {
        let mut errs: Vec<String> = Vec::new();

        if !(project.fps > 0.0 && project.fps <= 1000.0) {
            errs.push("fps must be within (0, 1000]".into());
        }
        if project.frames == 0 || project.frames > MAX_FRAMES {
            errs.push(format!("frames must be within 1..={MAX_FRAMES}"));
        }
        if let Err(m) = check_magnitudes(&project) {
            errs.push(m);
        }
        if !(project.width > 0.0 && project.height > 0.0) {
            errs.push("width and height must be > 0".into());
        } else if project.width > MAX_STAGE || project.height > MAX_STAGE {
            errs.push(format!("width and height must be at most {MAX_STAGE}"));
        }
        if !project.camera.focus.is_finite() || !project.camera.blur.is_finite() || project.camera.blur < 0.0 {
            errs.push("camera focus must be finite and camera blur a finite number >= 0".into());
        }
        errs.extend(crate::soundtrack::validate(&project));
        errs.extend(crate::editordata::check(&project).errors);
        errs.extend(crate::postfx::validate_layer_normal_maps(&project));
        errs.extend(crate::machine::validate(&project));
        let cerrs = crate::containers::validate(&project);
        if cerrs.is_empty() {
            crate::containers::apply_clip_masks(&mut project);
        } else {
            errs.extend(cerrs);
        }
        if project.bones.len() > MAX_BONES {
            errs.push(format!("at most {MAX_BONES} bones"));
            return Err(errs);
        }

        let parent = match resolve_parents(&project.bones) {
            Ok(p) => p,
            Err(e) => {
                errs.push(e);
                return Err(errs);
            }
        };
        for b in &project.bones {
            if !(b.length.is_finite() && b.length >= 0.0) {
                errs.push(format!("bone '{}' has invalid length", b.name));
            }
            if let Some([lo, hi]) = b.angle_limits {
                if !matches!(lo.partial_cmp(&hi), Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)) {
                    errs.push(format!("bone '{}': angle_limits min must be <= max", b.name));
                }
            }
            if let Some([lo, hi]) = b.scale_limits {
                if !(lo <= hi && lo > 0.0) {
                    errs.push(format!("bone '{}': scale_limits must satisfy 0 < min <= max", b.name));
                }
            }
        }
        let bone_ix: HashMap<String, usize> =
            project.bones.iter().enumerate().map(|(i, b)| (b.name.clone(), i)).collect();

        // Layer tree → arena (preorder).
        let mut nodes: Vec<NodeInfo> = Vec::new();
        let mut node_ix: HashMap<String, usize> = HashMap::new();
        build_nodes(&project.layers, None, "", &[], None, &mut nodes, &mut node_ix, &mut errs);
        let roots: Vec<usize> = (0..nodes.len()).filter(|&i| nodes[i].parent.is_none()).collect();

        // Per-layer references: bone attachment, masks.
        // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
        #[allow(clippy::needless_range_loop)]
        for id in 0..nodes.len() {
            let l = layer_at(&project, &nodes[id].idx_path);
            if let Some(b) = &l.bone {
                match bone_ix.get(b) {
                    Some(&bi) => nodes[id].bone = Some(bi),
                    None => errs.push(format!("layer '{}': unknown bone '{}'", nodes[id].path, b)),
                }
            }
            if let Some(m) = &l.mask {
                match node_ix.get(&m.layer) {
                    Some(&mi) if mi != id => nodes[id].mask = Some((mi, m.invert)),
                    Some(_) => errs.push(format!("layer '{}' cannot mask itself", nodes[id].path)),
                    None => errs.push(format!("layer '{}': unknown mask layer '{}'", nodes[id].path, m.layer)),
                }
            }
        }
        for id in 0..nodes.len() {
            let (mut cur, mut seen) = (id, HashSet::new());
            while let Some((m, _)) = nodes[cur].mask {
                if !seen.insert(m) || m == id {
                    errs.push(format!("mask cycle involving layer '{}'", nodes[id].path));
                    break;
                }
                cur = m;
            }
        }

        // IK chains
        let mut ik = Vec::new();
        for c in &project.ik {
            let mut ids = Vec::new();
            for n in &c.bones {
                match bone_ix.get(n) {
                    Some(&i) => ids.push(i),
                    None => errs.push(format!("ik '{}': unknown bone '{}'", c.name, n)),
                }
            }
            if !(0.0..=0.9).contains(&c.soft) || !(1.0..=4.0).contains(&c.stretch) {
                errs.push(format!("ik '{}': soft must be within 0..0.9 and stretch within 1..4", c.name));
            }
            let target = bone_ix.get(&c.target).copied();
            if target.is_none() {
                errs.push(format!("ik '{}': unknown target bone '{}'", c.name, c.target));
            }
            if ids.len() == c.bones.len() {
                if ids.len() < 2 {
                    errs.push(format!("ik '{}': needs at least two bones", c.name));
                } else if ids.windows(2).any(|w| parent[w[1]] != Some(w[0])) {
                    errs.push(format!("ik '{}': each bone must be a direct child of the previous one", c.name));
                } else if target.map(|t| ids.contains(&t)).unwrap_or(false) {
                    errs.push(format!("ik '{}': target must not be part of the chain", c.name));
                } else if let Some(target) = target {
                    ik.push(IkResolved { bones: ids, target, bend: c.bend, iterations: c.iterations.max(1), soft: c.soft, stretch: c.stretch });
                    continue;
                }
            }
            // keep indices aligned with project.ik even on error (we return Err below anyway)
            ik.push(IkResolved { bones: vec![], target: 0, bend: 1.0, iterations: 1, soft: 0.0, stretch: 1.0 });
        }
        let ik_ix: HashMap<&str, usize> = project.ik.iter().enumerate().map(|(i, c)| (c.name.as_str(), i)).collect();

        // Constraints
        let mut constraints = Vec::new();
        for c in &project.constraints {
            let b = bone_ix.get(&c.bone).copied();
            if b.is_none() {
                errs.push(format!("constraint '{}': unknown bone '{}'", c.name, c.bone));
            }
            let other = |n: &str, errs: &mut Vec<String>| {
                let r = bone_ix.get(n).copied();
                if r.is_none() {
                    errs.push(format!("constraint '{}': unknown bone '{}'", c.name, n));
                }
                r
            };
            let kind = match &c.kind {
                ConstraintKind::Aim { target, offset } => {
                    other(target, &mut errs).map(|t| CKind::Aim { target: t, offset: *offset })
                }
                ConstraintKind::CopyRotation { source } => {
                    other(source, &mut errs).map(|s| CKind::CopyRotation { source: s })
                }
                ConstraintKind::Pin { target } => other(target, &mut errs).map(|t| CKind::Pin { target: t }),
            };
            if let (Some(bone), Some(kind)) = (b, kind) {
                let partner = match kind {
                    CKind::Aim { target, .. } | CKind::Pin { target } => target,
                    CKind::CopyRotation { source } => source,
                };
                if partner == bone {
                    errs.push(format!("constraint '{}': bone cannot constrain itself", c.name));
                }
                constraints.push(ConstraintResolved { bone, kind });
            } else {
                constraints.push(ConstraintResolved { bone: 0, kind: CKind::Pin { target: 0 } });
            }
        }
        let cons_ix: HashMap<&str, usize> =
            project.constraints.iter().enumerate().map(|(i, c)| (c.name.as_str(), i)).collect();

        // Springs
        let mut springs = Vec::new();
        for s in &project.springs {
            match bone_ix.get(&s.bone) {
                Some(&b) => {
                    if !(s.frequency > 0.0 && s.frequency <= 30.0) || !(s.damping >= 0.0 && s.damping.is_finite()) {
                        errs.push(format!("spring on '{}': frequency must be in (0, 30] Hz and damping >= 0", s.bone));
                    }
                    if springs.iter().any(|x: &SpringResolved| x.bone == b) {
                        errs.push(format!("spring on '{}': bone already has a spring", s.bone));
                    }
                    springs.push(SpringResolved { bone: b, frequency: s.frequency, damping: s.damping, warmup: s.warmup });
                }
                None => errs.push(format!("spring: unknown bone '{}'", s.bone)),
            }
        }
        // Springs are baked in bone-index order so parents settle before children.
        springs.sort_by_key(|s| s.bone);

        // Skin weights, geometry slots, images, meshes
        let mut skin = Vec::with_capacity(nodes.len());
        let mut mesh_skin = Vec::with_capacity(nodes.len());
        let mut geo: Vec<NodeGeo> = Vec::with_capacity(nodes.len());
        let mut fx_slots: Vec<Vec<Vec<(Prop, usize)>>> = Vec::with_capacity(nodes.len());
        let mut images: Vec<Option<Arc<ImageData>>> = Vec::with_capacity(nodes.len());
        let mut slot_ix: HashMap<(String, Prop), usize> = HashMap::new();
        let mut pslot_init: Vec<f32> = Vec::new();
        let mut new_slot = |key: (String, Prop), init: f32, errs: &mut Vec<String>| -> usize {
            let i = pslot_init.len();
            pslot_init.push(init);
            if slot_ix.insert(key.clone(), i).is_some() {
                errs.push(format!("duplicate animatable target '{}' ({:?})", key.0, key.1));
            }
            i
        };
        let normalise = |ws: &[crate::path::BoneWeight], what: &str, errs: &mut Vec<String>| -> Vec<(usize, f32)> {
            let mut out: Vec<(usize, f32)> = Vec::new();
            for w in ws {
                match bone_ix.get(w.bone.as_str()) {
                    Some(&bi) if w.weight.is_finite() && w.weight >= 0.0 => out.push((bi, w.weight)),
                    Some(_) => errs.push(format!("{what}: invalid weight for '{}'", w.bone)),
                    None => errs.push(format!("{what}: unknown bone '{}'", w.bone)),
                }
            }
            let sum: f32 = out.iter().map(|w| w.1).sum();
            if sum > 0.0 {
                for w in &mut out {
                    w.1 /= sum;
                }
            } else {
                out.clear();
            }
            out
        };

        for node in &nodes {
            let l = layer_at(&project, &node.idx_path);
            let mut lp = Vec::new();
            let mut g = NodeGeo::default();
            let mut ms = Vec::new();
            let mut img = None;

            match &l.kind {
                LayerKind::Vector { paths } => {
                    let mut seen_paths = HashSet::new();
                    for (pi, p) in paths.iter().enumerate() {
                        let pname = if p.name.is_empty() { format!("path{pi}") } else { p.name.clone() };
                        if p.name.contains("::") {
                            errs.push(format!("layer '{}': path name '{}' must not contain '::'", node.path, p.name));
                        }
                        if !seen_paths.insert(pname.clone()) {
                            errs.push(format!("layer '{}': duplicate path name '{}'", node.path, pname));
                        }
                        let mut pa = Vec::new();
                        for (ai, a) in p.anchors.iter().enumerate() {
                            let what = format!("layer '{}' path '{}' anchor {}", node.path, pname, ai);
                            pa.push(normalise(&a.weights, &what, &mut errs));
                        }
                        lp.push(pa);

                        let mut mslots = Vec::new();
                        for m in &p.morphs {
                            if m.anchors.len() != p.anchors.len() {
                                errs.push(format!(
                                    "layer '{}' path '{}': morph '{}' has {} anchors, base has {}",
                                    node.path,
                                    pname,
                                    m.name,
                                    m.anchors.len(),
                                    p.anchors.len()
                                ));
                            }
                            mslots.push(new_slot(
                                (format!("{}::{}::{}", node.path, pname, m.name), Prop::PathMorph),
                                0.0,
                                &mut errs,
                            ));
                        }
                        g.morph_slots.push(mslots);
                        let mut pslots = Vec::new();
                        for ai in 0..p.anchors.len() {
                            let key = format!("{}::{}::{}", node.path, pname, ai);
                            let dx = new_slot((key.clone(), Prop::PointDx), 0.0, &mut errs);
                            let dy = new_slot((key, Prop::PointDy), 0.0, &mut errs);
                            pslots.push([dx, dy]);
                        }
                        g.point_slots.push(pslots);
                        let mut gs: [Option<[usize; 2]>; 4] = [None; 4];
                        let is_grad = |pt: &crate::style::Paint| matches!(pt, crate::style::Paint::Linear { .. } | crate::style::Paint::Radial { .. });
                        for (k, (has, name)) in [(p.fill.as_ref().is_some_and(is_grad), "fill.from"), (p.fill.as_ref().is_some_and(is_grad), "fill.to"), (p.stroke.as_ref().is_some_and(|s| is_grad(&s.paint)), "stroke.from"), (p.stroke.as_ref().is_some_and(|s| is_grad(&s.paint)), "stroke.to")].into_iter().enumerate() {
                            if has {
                                let key = format!("{}::{}::{}", node.path, pname, name);
                                let dx = new_slot((key.clone(), Prop::GradDx), 0.0, &mut errs);
                                let dy = new_slot((key, Prop::GradDy), 0.0, &mut errs);
                                gs[k] = Some([dx, dy]);
                            }
                        }
                        g.grad_slots.push(gs);
                    }
                }
                LayerKind::Image { source, mesh, .. } => match load_image(source, asset_dir) {
                    Ok(d) => {
                        if let Some(m) = mesh {
                            let n = m.vertex_count();
                            if let Some(fm) = &m.free {
                                if let Err(e) = validate_free_mesh(fm) {
                                    errs.push(format!("layer '{}': free mesh: {e}", node.path));
                                }
                            }
                            if m.free.is_none() && (m.cols == 0 || m.rows == 0) {
                                errs.push(format!("layer '{}': mesh needs cols >= 1 and rows >= 1", node.path));
                            } else if !m.weights.is_empty() && m.weights.len() != n {
                                errs.push(format!(
                                    "layer '{}': mesh has {} weight lists, expected {}",
                                    node.path,
                                    m.weights.len(),
                                    n
                                ));
                            } else {
                                for (vi, w) in m.weights.iter().enumerate() {
                                    let what = format!("layer '{}' mesh vertex {}", node.path, vi);
                                    ms.push(normalise(w, &what, &mut errs));
                                }
                            }
                        }
                        img = Some(Arc::new(d));
                    }
                    Err(e) => errs.push(format!("layer '{}': {e}", node.path)),
                },
                LayerKind::Liquid { blobs, threshold, cell, .. } => {
                    if blobs.is_empty() {
                        errs.push(format!("layer '{}': liquid needs at least one blob", node.path));
                    }
                    if ((*threshold).partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)) || !matches!((*cell).partial_cmp(&0.25), Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)) {
                        errs.push(format!("layer '{}': liquid needs threshold > 0 and cell >= 0.25", node.path));
                    }
                    for b in blobs {
                        if b.radius.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
                            errs.push(format!("layer '{}': blob radius must be > 0", node.path));
                        }
                        if let Some(n) = &b.bone {
                            if !bone_ix.contains_key(n) {
                                errs.push(format!("layer '{}': blob bound to unknown bone '{n}'", node.path));
                            }
                        }
                    }
                }
                LayerKind::Boolean { paths, .. } => {
                    if paths.len() < 2 {
                        errs.push(format!("layer '{}': boolean needs at least two paths", node.path));
                    }
                }
                LayerKind::Emitter { spec } => {
                    if let Err(m) = spec.validate() {
                        errs.push(format!("layer '{}': emitter: {m}", node.path));
                    }
                    if let Some(n) = &spec.bone {
                        if !bone_ix.contains_key(n) {
                            errs.push(format!("layer '{}': emitter bound to unknown bone '{n}'", node.path));
                        }
                    }
                }
                _ => {}
            }

            // Deformers (vector and image layers)
            let mut seen_def = HashSet::new();
            for d in &l.deformers {
                if !seen_def.insert(d.name().to_string()) {
                    errs.push(format!("layer '{}': duplicate deformer name '{}'", node.path, d.name()));
                }
                if d.name().contains("::") {
                    errs.push(format!("layer '{}': deformer name '{}' must not contain '::'", node.path, d.name()));
                }
                match d {
                    Deformer::Lattice { name, cols, rows, rect, offsets } | Deformer::Cloth { name, cols, rows, rect, offsets } => {
                        let is_cloth = matches!(d, Deformer::Cloth { .. });
                        let (cols, rows) = (*cols, *rows);
                        let n = (cols + 1) * (rows + 1);
                        if cols == 0 || rows == 0 || !(rect[2] > 0.0 && rect[3] > 0.0) {
                            errs.push(format!(
                                "layer '{}' lattice '{}': needs cols,rows >= 1 and a rect with positive size",
                                node.path, name
                            ));
                            continue;
                        }
                        if !offsets.is_empty() && offsets.len() != n {
                            errs.push(format!("layer '{}' lattice '{}': offsets must have {} entries", node.path, name, n));
                            continue;
                        }
                        let mut slots = Vec::with_capacity(n);
                        for r in 0..=rows {
                            for c in 0..=cols {
                                let off = offsets.get(r * (cols + 1) + c).copied().unwrap_or(Vec2::ZERO);
                                let key = format!("{}::{}::{},{}", node.path, name, c, r);
                                let dx = new_slot((key.clone(), Prop::LatticeDx), off.x, &mut errs);
                                let dy = new_slot((key, Prop::LatticeDy), off.y, &mut errs);
                                slots.push([dx, dy]);
                            }
                        }
                        g.deformers.push(if is_cloth {
                            DefInfo::Cloth { cols, rows, rect: *rect, slots }
                        } else {
                            DefInfo::Lattice { cols, rows, rect: *rect, slots }
                        });
                    }
                    Deformer::Spine { name, rest, offsets, radius } => {
                        if rest.len() < 2 {
                            errs.push(format!("layer '{}' spine '{}': needs at least two points", node.path, name));
                            continue;
                        }
                        if !offsets.is_empty() && offsets.len() != rest.len() {
                            errs.push(format!("layer '{}' spine '{}': offsets must match the point count", node.path, name));
                            continue;
                        }
                        let mut slots = Vec::with_capacity(rest.len());
                        for (i, _) in rest.iter().enumerate() {
                            let off = offsets.get(i).copied().unwrap_or(Vec2::ZERO);
                            let key = format!("{}::{}::{}", node.path, name, i);
                            let dx = new_slot((key.clone(), Prop::SpineDx), off.x, &mut errs);
                            let dy = new_slot((key, Prop::SpineDy), off.y, &mut errs);
                            slots.push([dx, dy]);
                        }
                        let rest_poly = VPath::smooth_through("", rest, false, 1.0).polyline(0.5);
                        g.deformers.push(DefInfo::Spine { rest_pts: rest.clone(), rest_poly, slots, radius: *radius });
                    }
                    Deformer::Pin { name, pins, offsets, radius } => {
                        if !offsets.is_empty() && offsets.len() != pins.len() {
                            errs.push(format!("layer '{}' pin warp '{}': offsets must match the pin count", node.path, name));
                            continue;
                        }
                        let solver = match crate::pinwarp::PinSolver::new(pins, *radius) {
                            Ok(s) => s,
                            Err(m) => {
                                errs.push(format!("layer '{}' pin warp '{}': {m}", node.path, name));
                                continue;
                            }
                        };
                        let mut slots = Vec::with_capacity(pins.len());
                        for i in 0..pins.len() {
                            let off = offsets.get(i).copied().unwrap_or(Vec2::ZERO);
                            let key = format!("{}::{}::{}", node.path, name, i);
                            let dx = new_slot((key.clone(), Prop::PinDx), off.x, &mut errs);
                            let dy = new_slot((key, Prop::PinDy), off.y, &mut errs);
                            slots.push([dx, dy]);
                        }
                        g.deformers.push(DefInfo::Pin { solver, slots });
                    }
                    Deformer::Cage { name, rest, offsets } => {
                        if !offsets.is_empty() && offsets.len() != rest.len() {
                            errs.push(format!("layer '{}' cage '{}': offsets must match the vertex count", node.path, name));
                            continue;
                        }
                        let solver = match crate::cage::CageSolver::new(rest) {
                            Ok(s) => s,
                            Err(m) => {
                                errs.push(format!("layer '{}' cage '{}': {m}", node.path, name));
                                continue;
                            }
                        };
                        let mut slots = Vec::with_capacity(rest.len());
                        for i in 0..rest.len() {
                            let off = offsets.get(i).copied().unwrap_or(Vec2::ZERO);
                            let key = format!("{}::{}::{}", node.path, name, i);
                            let dx = new_slot((key.clone(), Prop::CageDx), off.x, &mut errs);
                            let dy = new_slot((key, Prop::CageDy), off.y, &mut errs);
                            slots.push([dx, dy]);
                        }
                        // weights of the rest mesh are fixed: compute them once (first deformer only,
                        // because later deformers see already displaced points)
                        let mut mesh_w = Vec::new();
                        if g.deformers.is_empty() {
                            if let (LayerKind::Image { origin, size, mesh, .. }, Some(d)) = (&l.kind, &img) {
                                let (pw, ph) = (d.pixmap.width() as f32, d.pixmap.height() as f32);
                                let sz = size.unwrap_or(Vec2::new(pw, ph));
                                let (c, r) = match mesh {
                                    Some(m) => (m.cols.max(1), m.rows.max(1)),
                                    None => (16, 16),
                                };
                                let rest_v = match mesh {
                                    Some(m) if m.free.is_some() => m.rest_vertices(*origin, sz),
                                    _ => crate::project::mesh_vertices(*origin, sz, c, r),
                                };
                                if rest_v.len() * rest.len() <= 4_000_000 {
                                    let mut w = Vec::new();
                                    for v in rest_v {
                                        mesh_w.push(if solver.weights(v, &mut w) { w.clone() } else { Vec::new() });
                                    }
                                }
                            }
                        }
                        g.deformers.push(DefInfo::Cage { solver, slots, mesh_w });
                    }
                }
            }
            if !l.deformers.is_empty() && !matches!(l.kind, LayerKind::Vector { .. } | LayerKind::Image { .. }) {
                errs.push(format!("layer '{}': deformers only apply to vector and image layers", node.path));
            }

            errs.extend(crate::layerfx::validate_list(&format!("layer '{}'", node.path), &l.effects));
            let mut fxs = Vec::new();
            for (ei, e) in l.effects.iter().enumerate() {
                let key = crate::layerfx::target(&node.path, ei);
                let mut v = Vec::new();
                for &pr in e.props() {
                    let init = e.get(pr).filter(|x| x.is_finite()).unwrap_or(0.0);
                    v.push((pr, new_slot((key.clone(), pr), init, &mut errs)));
                }
                fxs.push(v);
            }
            fx_slots.push(fxs);

            skin.push(lp);
            mesh_skin.push(ms);
            geo.push(g);
            images.push(img);
        }

        // Channels
        let resolve = |target: &str, prop: Prop| -> Option<usize> {
            match prop.target_kind() {
                TargetKind::Bone => bone_ix.get(target).copied(),
                TargetKind::Layer => node_ix.get(target).copied(),
                TargetKind::Camera => Some(0),
                TargetKind::Ik => ik_ix.get(target).copied(),
                TargetKind::Constraint => cons_ix.get(target).copied(),
                TargetKind::Sub => slot_ix.get(&(target.to_string(), prop)).copied(),
            }
        };
        let mut chans = Vec::new();
        for (ci, c) in project.channels.iter().enumerate() {
            let Some(slot) = resolve(&c.target, c.prop) else {
                errs.push(format!("channel {ci}: unknown target '{}' for {:?}", c.target, c.prop));
                continue;
            };
            if c.prop == Prop::LayerSwitch {
                let l = layer_at(&project, &nodes[slot].idx_path);
                if !matches!(l.kind, LayerKind::Switch { .. }) {
                    errs.push(format!("channel {ci}: layer_switch needs a switch layer, '{}' is not one", c.target));
                }
            }
            if !c.weight.is_finite() {
                errs.push(format!("channel {ci}: weight must be finite"));
            }
            let check_track = |t: &crate::anim::Track, errs: &mut Vec<String>| {
                let sorted = t.keys.windows(2).all(|w| w[0].frame < w[1].frame);
                let finite = t.keys.iter().all(|k| k.frame.is_finite() && k.value.is_finite());
                if !sorted || !finite {
                    errs.push(format!("channel {ci} ('{}'): keys must be finite and strictly increasing in frame", c.target));
                }
            };
            let (mut parsed, mut input) = (None, None);
            match &c.source {
                Source::Keys { track } => check_track(track, &mut errs),
                Source::Expr { expr: src } => match expr::parse(src) {
                    Ok(e) => {
                        let mut gets = Vec::new();
                        e.gets(&mut gets);
                        for (t, p) in gets {
                            match Prop::parse(&p) {
                                Some(pp) if resolve(&t, pp).is_some() => {}
                                Some(_) => errs.push(format!("channel {ci}: get(\"{t}\", \"{p}\"): unknown target")),
                                None => errs.push(format!("channel {ci}: get(\"{t}\", \"{p}\"): unknown property")),
                            }
                        }
                        parsed = Some(e);
                    }
                    Err(m) => errs.push(format!("channel {ci} ('{}'): expression error: {m}", c.target)),
                },
                Source::Noise { amp, freq, .. } => {
                    if !(amp.is_finite() && freq.is_finite()) {
                        errs.push(format!("channel {ci}: noise amp/freq must be finite"));
                    }
                }
                Source::Driven { input: it, input_prop, curve } => {
                    check_track(curve, &mut errs);
                    match resolve(it, *input_prop) {
                        Some(s) => input = Some((*input_prop, s)),
                        None => errs.push(format!("channel {ci}: unknown driver input '{it}' for {input_prop:?}")),
                    }
                }
            }
            chans.push(ChanResolved { prop: c.prop, slot, chan: ci, expr: parsed, input });
        }

        if !errs.is_empty() {
            return Err(errs);
        }

        let rest = fk(&project.bones, &parent, &vec![BonePose::default(); project.bones.len()]);
        let inv_rest = rest.iter().map(|m| m.inverse().unwrap_or(Affine2::IDENTITY)).collect();

        let trial = crate::machine::trial_errors(&project, asset_dir);
        if !trial.is_empty() {
            return Err(trial);
        }
        let mut engine = Engine {
            project,
            show_reference: false,
            parent,
            ik,
            constraints,
            springs,
            baked: Vec::new(),
            inv_rest,
            nodes,
            roots,
            skin,
            mesh_skin,
            geo,
            fx_slots,
            images,
            chans,
            bone_ix,
            node_ix,
            slot_ix,
            pslot_init,
            links: None,
            layout: Vec::new(),
        };
        engine.bake_springs();
        engine.links = crate::links::resolve(&engine)?;
        if !engine.project.containers.is_empty() {
            engine.layout = crate::containers::node_offsets(&engine).map_err(|e| vec![e])?;
        }
        Ok(engine)
    }

    pub fn bone_index(&self, name: &str) -> Option<usize> {
        self.bone_ix.get(name).copied()
    }
    pub fn node_index(&self, path: &str) -> Option<usize> {
        self.node_ix.get(path).copied()
    }
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Index path of node `id` into `project.layers` / `children()` (for building JSON pointers).
    pub fn node_idx_path(&self, id: usize) -> &[usize] {
        &self.nodes[id].idx_path
    }
    /// Parent node of `id` (`None` for top-level layers).
    pub fn node_parent(&self, id: usize) -> Option<usize> {
        self.nodes[id].parent
    }
    /// The layer's evaluated local transform at `frame` (all channels, layout offsets and the
    /// static transform combined; the anchor is the layer's). Used by the canvas gizmos.
    pub fn layer_local_at(&self, frame: f32, id: usize) -> crate::project::Transform2D {
        let (mut st, _) = self.pose_pipeline(frame);
        if let Some(Some([dx, dy, kx, ky])) = self.layout.get(id) {
            let lp = &mut st.layers[id];
            lp.x += dx;
            lp.y += dy;
            lp.sx *= kx;
            lp.sy *= ky;
        }
        let lp = &st.layers[id];
        crate::project::Transform2D {
            position: Vec2::new(lp.x, lp.y),
            rotation: lp.rot,
            scale: Vec2::new(lp.sx, lp.sy),
            skew: lp.skew,
            anchor: self.layer_ref(id).transform.anchor,
        }
    }

    /// Rest-pose geometry of node `id` for `links.rs` (layer space, static transforms only).
    pub(crate) fn link_unit(&self, id: usize) -> Option<crate::links::UnitRest> {
        let layer = self.layer_ref(id);
        let mut stage = Affine2::IDENTITY;
        let mut chain = vec![id];
        while let Some(pa) = self.nodes[*chain.last().unwrap()].parent {
            chain.push(pa);
        }
        for &n in chain.iter().rev() {
            stage = stage.mul(self.layer_ref(n).transform.matrix());
        }
        match &layer.kind {
            LayerKind::Vector { paths } => Some(crate::links::UnitRest {
                stage,
                paths: paths.iter().map(|p| p.anchors.iter().map(|a| a.pos).collect()).collect(),
                closed: paths.iter().map(|p| p.closed).collect(),
                tris: Vec::new(),
                is_image: false,
                has_mesh: true,
            }),
            LayerKind::Image { origin, size, mesh, .. } => {
                let data = self.images[id].as_ref()?;
                let size = size.unwrap_or(Vec2::new(data.pixmap.width() as f32, data.pixmap.height() as f32));
                let (verts, tris) = match mesh {
                    Some(m) => (m.rest_vertices(*origin, size), m.tri_indices()),
                    None => (Vec::new(), Vec::new()),
                };
                Some(crate::links::UnitRest { stage, paths: vec![verts], closed: vec![false], tris, is_image: true, has_mesh: mesh.is_some() })
            }
            _ => None,
        }
    }
    pub(crate) fn link_root_parallax(&self, id: usize) -> f32 {
        self.layer_ref(self.nodes[id].root).parallax
    }

    /// Enabled effects of node `id` with channel-driven values filled in.
    fn node_effects(&self, id: usize, layer: &Layer, st: &State) -> Vec<crate::layerfx::LayerEffect> {
        let mut out = Vec::new();
        for (ei, e) in layer.effects.iter().enumerate() {
            if !e.enabled {
                continue;
            }
            let mut e = e.clone();
            if let Some(slots) = self.fx_slots.get(id).and_then(|s| s.get(ei)) {
                for &(pr, slot) in slots {
                    e.set(pr, st.pvals[slot]);
                }
            }
            out.push(e);
        }
        out
    }

    fn layer_ref(&self, id: usize) -> &Layer {
        layer_at(&self.project, &self.nodes[id].idx_path)
    }

    fn initial_state(&self) -> State {
        let p = &self.project;
        let layers = (0..self.nodes.len())
            .map(|id| {
                let l = self.layer_ref(id);
                LayerProps {
                    x: l.transform.position.x,
                    y: l.transform.position.y,
                    rot: l.transform.rotation,
                    sx: l.transform.scale.x,
                    sy: l.transform.scale.y,
                    skew: l.transform.skew,
                    opacity: l.opacity.clamp(0.0, 1.0),
                    switch: match &l.kind {
                        LayerKind::Switch { selected, .. } => *selected as f32,
                        _ => 0.0,
                    },
                    wobble: l.wobble.as_ref().map(|w| w.amp).unwrap_or(0.0),
                }
            })
            .collect();
        let centre = Vec2::new(p.width * 0.5, p.height * 0.5);
        let pos = p.camera.position.unwrap_or(centre);
        State {
            bones: vec![BonePose::default(); p.bones.len()],
            layers,
            cam: CamProps { x: pos.x, y: pos.y, zoom: p.camera.zoom.max(0.01), rot: p.camera.rotation, focus: p.camera.focus, blur: p.camera.blur.max(0.0) },
            ik_mix: p.ik.iter().map(|c| c.mix.clamp(0.0, 1.0)).collect(),
            cweight: p.constraints.iter().map(|c| c.weight.clamp(0.0, 1.0)).collect(),
            pvals: self.pslot_init.clone(),
        }
    }

    /// Current value of `target.prop` (used by expressions); 0 if unknown.
    fn lookup(&self, st: &State, target: &str, prop: &str) -> f32 {
        let Some(prop) = Prop::parse(prop) else { return 0.0 };
        let slot = match prop.target_kind() {
            TargetKind::Bone => self.bone_ix.get(target).copied(),
            TargetKind::Layer => self.node_ix.get(target).copied(),
            TargetKind::Camera => Some(0),
            TargetKind::Ik => self.project.ik.iter().position(|c| c.name == target),
            TargetKind::Constraint => self.project.constraints.iter().position(|c| c.name == target),
            TargetKind::Sub => self.slot_ix.get(&(target.to_string(), prop)).copied(),
        };
        slot.map(|s| st.read(prop, s)).unwrap_or(0.0)
    }

    fn apply_channels(&self, st: &mut State, frame: f32) {
        let p = &self.project;
        for c in &self.chans {
            let ch = &p.channels[c.chan];
            let v: Option<f32> = match &ch.source {
                Source::Keys { track } => track.eval(frame),
                Source::Expr { .. } => {
                    let get = |t: &str, pr: &str| self.lookup(st, t, pr);
                    let ctx = Ctx { frame, fps: p.fps, frames: p.frames as f32, get: &get };
                    c.expr.as_ref().map(|e| e.eval(&ctx))
                }
                Source::Noise { amp, freq, seed, octaves } => Some(amp * fbm1(frame / p.fps * freq, *seed, *octaves)),
                Source::Driven { curve, .. } => {
                    let (ip, is) = c.input.expect("resolved at compile time");
                    curve.eval(st.read(ip, is))
                }
            };
            let Some(v) = v else { continue };
            if !v.is_finite() {
                continue;
            }
            let cur = st.read(c.prop, c.slot);
            let w = ch.weight;
            let new = match ch.mode {
                Mode::Replace => cur + (v - cur) * w,
                Mode::Add => cur + v * w,
                Mode::Multiply => cur * (1.0 + (v - 1.0) * w),
            };
            st.write(c.prop, c.slot, new);
        }
    }

    fn camera_matrix(&self, cam: &CamProps, parallax: f32) -> Affine2 {
        let p = &self.project;
        let centre = Vec2::new(p.width * 0.5, p.height * 0.5);
        let zoom = (1.0 + (cam.zoom - 1.0) * parallax).max(0.01);
        let pos = centre + (Vec2::new(cam.x, cam.y) - centre) * parallax;
        Affine2::translate(centre)
            .mul(Affine2::rotate(-cam.rot.to_radians() * parallax))
            .mul(Affine2::scale(zoom))
            .mul(Affine2::translate(-pos))
    }

    fn clamp_limits(&self, poses: &mut [BonePose]) {
        for (b, p) in self.project.bones.iter().zip(poses.iter_mut()) {
            if let Some([lo, hi]) = b.angle_limits {
                p.angle = p.angle.clamp(lo.to_radians(), hi.to_radians());
            }
            if let Some([lo, hi]) = b.scale_limits {
                p.scale = p.scale.clamp(lo, hi);
            }
        }
    }

    /// Solve one IK chain, writing blended pose angles (and, for stretchy chains, the root
    /// bone's scale) into `poses`.
    fn solve_ik(&self, chain: &IkResolved, mix: f32, poses: &mut [BonePose], world: &[Affine2]) {
        let bones = &self.project.bones;
        let raw_target = world[chain.target].apply(Vec2::ZERO);
        let root_head = world[chain.bones[0]].apply(Vec2::ZERO);
        let last = *chain.bones.last().unwrap();
        let mut joints: Vec<Vec2> = chain.bones.iter().map(|&b| world[b].apply(Vec2::ZERO)).collect();
        joints.push(world[last].apply(Vec2::new(bones[last].length, 0.0)));
        let n = chain.bones.len();
        let lengths: Vec<f32> = (0..n).map(|i| joints[i].distance(joints[i + 1])).collect();
        let reach: f32 = lengths.iter().sum();
        let to = raw_target - root_head;
        let dist = to.length();
        // stretch: lengthen the chain up to `stretch`×; otherwise soften the approach to full extension
        let mut scale = 1.0f32;
        let mut target = raw_target;
        if chain.stretch > 1.0 && chain.soft > 0.0 && reach > 1e-4 && dist > 1e-6 {
            // soft + stretch (wave 7; both were exclusive before and the chain popped from "held back" to "straight"
            // at full reach): the soft zone is the last `soft` fraction of the STRETCHED reach, so the chain lengthens
            // continuously and eases into its maximum length
            let eased = soft_distance(dist, reach * chain.stretch, chain.soft);
            scale = (eased / reach).max(1.0);
            target = root_head + to.normalized() * eased;
        } else if chain.stretch > 1.0 && reach > 1e-4 && dist > reach {
            scale = (dist / reach).min(chain.stretch);
        } else if chain.soft > 0.0 && reach > 1e-4 && dist > 1e-6 {
            target = root_head + to.normalized() * soft_distance(dist, reach, chain.soft);
        }
        let old: Vec<f32> = chain.bones.iter().map(|&b| poses[b].angle).collect();
        let new: Vec<f32> = if n == 2 {
            match solve_two_bone_scaled(bones, &self.parent, world, chain.bones[0], chain.bones[1], target, chain.bend, scale) {
                Some((a, b)) => vec![a, b],
                None => return,
            }
        } else {
            if lengths.iter().any(|l| *l < 1e-4) {
                return;
            }
            let scaled: Vec<f32> = lengths.iter().map(|l| l * scale).collect();
            solve_fabrik(&mut joints, &scaled, target, (chain.iterations as usize).max(crate::rig::FABRIK_MAX_ITERS), crate::rig::FABRIK_TOLERANCE);
            let root = chain.bones[0];
            let mut parent_rot = self.parent[root].map(|p| world[p].rotation()).unwrap_or(0.0);
            let mut out = Vec::with_capacity(n);
            for i in 0..n {
                let d = joints[i + 1] - joints[i];
                let wa = d.y.atan2(d.x);
                out.push(wa - parent_rot - bones[chain.bones[i]].rest_angle.to_radians());
                parent_rot = wa;
            }
            out
        };
        if scale > 1.0 {
            poses[chain.bones[0]].scale *= 1.0 + (scale - 1.0) * mix;
        }
        for (i, &b) in chain.bones.iter().enumerate() {
            poses[b].angle = old[i] + wrap_pi(new[i] - old[i]) * mix;
        }
    }

    fn apply_constraint(&self, c: &ConstraintResolved, weight: f32, poses: &mut [BonePose], world: &[Affine2]) {
        let head = world[c.bone].apply(Vec2::ZERO);
        match c.kind {
            CKind::Aim { target, offset } => {
                let t = world[target].apply(Vec2::ZERO) - head;
                if t.length() < 1e-6 {
                    return;
                }
                let desired = t.y.atan2(t.x) + offset.to_radians();
                poses[c.bone].angle += wrap_pi(desired - world[c.bone].rotation()) * weight;
            }
            CKind::CopyRotation { source } => {
                let desired = world[source].rotation();
                poses[c.bone].angle += wrap_pi(desired - world[c.bone].rotation()) * weight;
            }
            CKind::Pin { target } => {
                let dw = (world[target].apply(Vec2::ZERO) - head) * weight;
                let local = match self.parent[c.bone] {
                    Some(p) => {
                        let inv = world[p].inverse().unwrap_or(Affine2::IDENTITY);
                        Vec2::new(inv.a * dw.x + inv.c * dw.y, inv.b * dw.x + inv.d * dw.y)
                    }
                    None => dw,
                };
                poses[c.bone].offset += local;
            }
        }
    }

    /// Channels → limits → IK → constraints. (Springs are applied afterwards.)
    fn pose_pipeline(&self, frame: f32) -> (State, Vec<Affine2>) {
        let p = &self.project;
        let mut st = self.initial_state();
        self.apply_channels(&mut st, frame);
        self.clamp_limits(&mut st.bones);
        let mut world = fk(&p.bones, &self.parent, &st.bones);
        for (i, chain) in self.ik.iter().enumerate() {
            let mix = st.ik_mix[i];
            if mix <= 0.0 {
                continue;
            }
            self.solve_ik(chain, mix, &mut st.bones, &world);
            self.clamp_limits(&mut st.bones);
            world = fk(&p.bones, &self.parent, &st.bones);
        }
        for (i, c) in self.constraints.iter().enumerate() {
            let w = st.cweight[i];
            if w <= 0.0 {
                continue;
            }
            self.apply_constraint(c, w, &mut st.bones, &world);
            self.clamp_limits(&mut st.bones);
            world = fk(&p.bones, &self.parent, &st.bones);
        }
        (st, world)
    }

    fn bake_springs(&mut self) {
        if self.springs.is_empty() {
            return;
        }
        let n = self.project.frames as usize;
        let dt = 1.0 / self.project.fps;
        let passes = if self.springs.iter().any(|s| s.warmup) { 2 } else { 1 };
        let mut sim = vec![SpringState::default(); self.springs.len()];
        let mut baked = vec![vec![0.0f32; self.springs.len()]; n];
        for pass in 0..passes {
            // the index is a coordinate/identifier used beyond one slice (parallel arrays, offsets, neighbours); an iterator would obscure it
            #[allow(clippy::needless_range_loop)]
            for f in 0..n {
                let (mut st, mut world) = self.pose_pipeline(f as f32);
                for (k, sp) in self.springs.iter().enumerate() {
                    let bi = sp.bone;
                    let target = world[bi].rotation();
                    if f == 0 && (pass == 0 || !sp.warmup) {
                        sim[k] = SpringState::at_rest(target);
                    }
                    sim[k].step(target, sp.frequency, sp.damping, dt, 8);
                    let parent_rot = self.parent[bi].map(|p| world[p].rotation()).unwrap_or(0.0);
                    st.bones[bi].angle = sim[k].x - parent_rot - self.project.bones[bi].rest_angle.to_radians();
                    world = fk(&self.project.bones, &self.parent, &st.bones);
                    if pass == passes - 1 {
                        baked[f][k] = st.bones[bi].angle;
                    }
                }
            }
        }
        self.baked = baked;
    }

    fn baked_angle(&self, k: usize, frame: f32) -> f32 {
        let n = self.baked.len();
        let f = frame.clamp(0.0, (n - 1) as f32);
        let i = f.floor() as usize;
        let t = f - i as f32;
        let a = self.baked[i][k];
        if i + 1 >= n {
            return a;
        }
        a + (self.baked[i + 1][k] - a) * t
    }

    // ---- geometry ---------------------------------------------------------------------------

    fn lattice_disp(rect: [f32; 4], cols: usize, rows: usize, offs: &[Vec2], p: Vec2) -> Vec2 {
        let u = ((p.x - rect[0]) / rect[2]).clamp(0.0, 1.0);
        let v = ((p.y - rect[1]) / rect[3]).clamp(0.0, 1.0);
        let mut d = Vec2::ZERO;
        for r in 0..=rows {
            let bv = bernstein(rows, r, v);
            for c in 0..=cols {
                d += offs[r * (cols + 1) + c] * (bernstein(cols, c, u) * bv);
            }
        }
        d
    }

    /// Bilinear per-vertex displacement (exact at the grid vertices).
    fn cloth_disp(rect: [f32; 4], cols: usize, rows: usize, offs: &[Vec2], p: Vec2) -> Vec2 {
        let u = ((p.x - rect[0]) / rect[2]).clamp(0.0, 1.0) * cols as f32;
        let v = ((p.y - rect[1]) / rect[3]).clamp(0.0, 1.0) * rows as f32;
        let (c, r) = ((u.floor() as usize).min(cols - 1), (v.floor() as usize).min(rows - 1));
        let (fu, fv) = (u - c as f32, v - r as f32);
        let at = |cc: usize, rr: usize| offs[rr * (cols + 1) + cc];
        let top = at(c, r) * (1.0 - fu) + at(c + 1, r) * fu;
        let bot = at(c, r + 1) * (1.0 - fu) + at(c + 1, r + 1) * fu;
        top * (1.0 - fv) + bot * fv
    }

    fn spine_disp(rest_pts: &[Vec2], rest_poly: &Polyline, offs: &[Vec2], radius: Option<f32>, p: Vec2) -> Vec2 {
        let def_pts: Vec<Vec2> = rest_pts.iter().zip(offs).map(|(&r, &o)| r + o).collect();
        let def_poly = VPath::smooth_through("", &def_pts, false, 1.0).polyline(0.5);
        let (rest_len, def_len) = (rest_poly.length(), def_poly.length());
        if rest_len < 1e-6 || def_len < 1e-6 {
            return Vec2::ZERO;
        }
        let (u, v) = rest_poly.project(p);
        let (q0, t0) = rest_poly.sample(u);
        let (q1, t1) = def_poly.sample(u * def_len / rest_len);
        let (n0, n1) = (Vec2::new(-t0.y, t0.x), Vec2::new(-t1.y, t1.x));
        let w = match radius {
            None => 1.0,
            Some(r) if r > 0.0 => {
                let x = ((v.abs() - 0.5 * r) / (0.5 * r)).clamp(0.0, 1.0);
                1.0 - x * x * (3.0 - 2.0 * x)
            }
            Some(_) => 0.0,
        };
        ((q1 - q0) + (n1 * v - n0 * v)) * w
    }

    /// Apply all deformers, then wobble, to a point (rest space).
    fn deform_point(&self, id: usize, st: &State, tick: f32, p: Vec2, mesh_vertex: Option<usize>) -> Vec2 {
        let mut out = p;
        for (di, d) in self.geo[id].deformers.iter().enumerate() {
            let disp = match d {
                DefInfo::Lattice { cols, rows, rect, slots } => {
                    let offs: Vec<Vec2> = slots.iter().map(|s| Vec2::new(st.pvals[s[0]], st.pvals[s[1]])).collect();
                    Self::lattice_disp(*rect, *cols, *rows, &offs, out)
                }
                DefInfo::Cloth { cols, rows, rect, slots } => {
                    let offs: Vec<Vec2> = slots.iter().map(|s| Vec2::new(st.pvals[s[0]], st.pvals[s[1]])).collect();
                    Self::cloth_disp(*rect, *cols, *rows, &offs, out)
                }
                DefInfo::Spine { rest_pts, rest_poly, slots, radius } => {
                    let offs: Vec<Vec2> = slots.iter().map(|s| Vec2::new(st.pvals[s[0]], st.pvals[s[1]])).collect();
                    Self::spine_disp(rest_pts, rest_poly, &offs, *radius, out)
                }
                DefInfo::Pin { solver, slots } => {
                    let offs: Vec<Vec2> = slots.iter().map(|s| Vec2::new(st.pvals[s[0]], st.pvals[s[1]])).collect();
                    solver.displacement(&offs, out)
                }
                DefInfo::Cage { solver, slots, mesh_w } => {
                    let offs: Vec<Vec2> = slots.iter().map(|s| Vec2::new(st.pvals[s[0]], st.pvals[s[1]])).collect();
                    match mesh_vertex.filter(|_| di == 0).and_then(|vi| mesh_w.get(vi)) {
                        Some(w) => crate::cage::CageSolver::apply_weights(w, &offs),
                        None => solver.displacement(&offs, out),
                    }
                }
            };
            out += disp;
        }
        if let Some(w) = &self.layer_ref(id).wobble {
            let amp = st.layers[id].wobble;
            if amp != 0.0 {
                out += wobble_disp(out, tick, w, amp);
            }
        }
        out
    }

    fn vector_geometry(&self, id: usize, st: &State, skin_mats: &[Affine2], tick: f32) -> Vec<VPath> {
        let LayerKind::Vector { paths } = &self.layer_ref(id).kind else { return Vec::new() };
        let has_deform = !self.geo[id].deformers.is_empty() || st.layers[id].wobble != 0.0;
        paths
            .iter()
            .enumerate()
            .map(|(pi, path)| {
                let mut o = path.clone();
                // morphs (additive blend shapes)
                for (mi, &slot) in self.geo[id].morph_slots[pi].iter().enumerate() {
                    let w = st.pvals[slot];
                    if w == 0.0 {
                        continue;
                    }
                    let m = &path.morphs[mi];
                    for (ai, a) in o.anchors.iter_mut().enumerate() {
                        let (b, t) = (&path.anchors[ai], &m.anchors[ai]);
                        let (hin, hout) = (a.hin() + (t.hin() - b.hin()) * w, a.hout() + (t.hout() - b.hout()) * w);
                        a.pos += (t.pos - b.pos) * w;
                        if b.handle_in.is_some() || t.handle_in.is_some() {
                            a.handle_in = Some(hin);
                        }
                        if b.handle_out.is_some() || t.handle_out.is_some() {
                            a.handle_out = Some(hout);
                        }
                    }
                }
                // per-anchor offsets
                for (ai, a) in o.anchors.iter_mut().enumerate() {
                    let s = self.geo[id].point_slots[pi][ai];
                    let d = Vec2::new(st.pvals[s[0]], st.pvals[s[1]]);
                    if d != Vec2::ZERO {
                        a.map_points(&mut |pt| pt + d);
                    }
                }
                // animated gradient handles
                if let Some(gs) = self.geo[id].grad_slots.get(pi) {
                    let d = |k: usize| gs[k].map(|s| Vec2::new(st.pvals[s[0]], st.pvals[s[1]])).unwrap_or(Vec2::ZERO);
                    if let Some(f) = &mut o.fill {
                        shift_gradient(f, d(0), d(1));
                    }
                    if let Some(sk) = &mut o.stroke {
                        shift_gradient(&mut sk.paint, d(2), d(3));
                    }
                }
                // deformers + wobble (anchor displacement is shared with its handles → smooth curves)
                if has_deform {
                    for a in o.anchors.iter_mut() {
                        let d = self.deform_point(id, st, tick, a.pos, None) - a.pos;
                        a.map_points(&mut |pt| pt + d);
                    }
                }
                // skinning
                for (ai, a) in o.anchors.iter_mut().enumerate() {
                    let ws = &self.skin[id][pi][ai];
                    if ws.is_empty() {
                        continue;
                    }
                    a.map_points(&mut |pt| {
                        let mut acc = Vec2::ZERO;
                        for &(bi, w) in ws {
                            acc += skin_mats[bi].apply(pt) * w;
                        }
                        acc
                    });
                }
                o
            })
            .collect()
    }

    fn image_draw(&self, id: usize, st: &State, skin_mats: &[Affine2], tick: f32) -> Option<ImageDraw> {
        let data = self.images[id].clone()?;
        let LayerKind::Image { origin, size, mesh, .. } = &self.layer_ref(id).kind else { return None };
        let (pw, ph) = (data.pixmap.width() as f32, data.pixmap.height() as f32);
        let size = size.unwrap_or(Vec2::new(pw, ph));
        let has_deform = !self.geo[id].deformers.is_empty() || st.layers[id].wobble != 0.0;
        let mesh_out = if mesh.is_some() || has_deform {
            let free = mesh.as_ref().and_then(|m| m.free.as_ref());
            let (cols, rows) = match mesh {
                Some(m) => (m.cols.max(1), m.rows.max(1)),
                None => (16, 16),
            };
            let rest = match free {
                Some(f) => f.vertices.clone(),
                None => crate::project::mesh_vertices(*origin, size, cols, rows),
            };
            let uvs: Vec<Vec2> = match free {
                Some(f) if f.uvs.len() == f.vertices.len() => f.uvs.clone(),
                _ => rest.iter().map(|v| Vec2::new((v.x - origin.x) / size.x * pw, (v.y - origin.y) / size.y * ph)).collect(),
            };
            let positions: Vec<Vec2> = rest
                .iter()
                .enumerate()
                .map(|(vi, &v)| {
                    let mut p = self.deform_point(id, st, tick, v, Some(vi));
                    if let Some(ws) = self.mesh_skin[id].get(vi).filter(|w| !w.is_empty()) {
                        let mut acc = Vec2::ZERO;
                        for &(bi, w) in ws {
                            acc += skin_mats[bi].apply(p) * w;
                        }
                        p = acc;
                    }
                    p
                })
                .collect();
            let tris = match free {
                Some(f) => f.triangles.clone(),
                None => {
                    let mut tris = Vec::with_capacity(cols * rows * 2);
                    for r in 0..rows {
                        for c in 0..cols {
                            let i = (r * (cols + 1) + c) as u32;
                            let j = i + 1;
                            let k = i + (cols + 1) as u32;
                            let l = k + 1;
                            tris.push([i, j, k]);
                            tris.push([j, l, k]);
                        }
                    }
                    tris
                }
            };
            Some(MeshOut { positions, uvs, tris })
        } else {
            None
        };
        Some(ImageDraw { data, origin: *origin, size, mesh: mesh_out })
    }

    /// Pose, solve IK, skin and return everything drawable at `frame`.
    /// Depth-of-field `(focus, blur strength)` at `frame`, after camera channels (`camera_focus`,
    /// `camera_blur`) have been applied.
    pub fn dof_at(&self, frame: f32) -> (f32, f32) {
        let (st, _) = self.pose_pipeline(frame);
        (st.cam.focus, st.cam.blur)
    }

    /// Weight of every path morph at `frame`, indexed `[node id][path index][morph index]`
    /// (empty for layers that are not vector layers). Used by exporters that keep morphs as targets.
    pub fn morph_weights(&self, frame: f32) -> Vec<Vec<Vec<f32>>> {
        let (st, _) = self.pose_pipeline(frame);
        self.geo.iter().map(|g| g.morph_slots.iter().map(|ms| ms.iter().map(|&s| st.pvals[s]).collect()).collect()).collect()
    }

    pub fn evaluate(&self, frame: f32) -> Frame {
        let p = &self.project;
        let (mut st, mut world) = self.pose_pipeline(frame);
        for (id, o) in self.layout.iter().enumerate() {
            if let Some([dx, dy, kx, ky]) = o {
                let lp = &mut st.layers[id];
                lp.x += dx;
                lp.y += dy;
                lp.sx *= kx;
                lp.sy *= ky;
            }
        }

        if !self.springs.is_empty() {
            for (k, sp) in self.springs.iter().enumerate() {
                st.bones[sp.bone].angle = self.baked_angle(k, frame);
            }
            world = fk(&p.bones, &self.parent, &st.bones);
        }
        let skin_mats: Vec<Affine2> = world.iter().zip(&self.inv_rest).map(|(w, inv)| w.mul(*inv)).collect();

        let cams: HashMap<usize, Affine2> =
            self.roots.iter().map(|&r| (r, self.camera_matrix(&st.cam, self.layer_ref(r).parallax))).collect();

        let mut stage: Vec<Affine2> = Vec::with_capacity(self.nodes.len());
        let mut eff_attach: Vec<Affine2> = Vec::with_capacity(self.nodes.len());
        let mut chain_no_bone: Vec<Affine2> = Vec::with_capacity(self.nodes.len());
        let mut out: Vec<RenderNode> = Vec::with_capacity(self.nodes.len());
        for (id, info) in self.nodes.iter().enumerate() {
            let layer = self.layer_ref(id);
            let lp = &st.layers[id];
            let local =
                compose_transform(Vec2::new(lp.x, lp.y), lp.rot, Vec2::new(lp.sx, lp.sy), lp.skew, layer.transform.anchor);
            // The skin matrix of a bone is a STAGE-space motion (bone world * inverse rest), so it is applied after the
            // layer's whole parent chain: `world = skin * chain * local`. Applying it inside a scaled parent group (the
            // old `parent_stage * skin * local`) scaled the posed travel by the group's scale (L-71). A layer without a
            // bone of its own follows the bone of its nearest bone-attached ancestor; its own bone replaces that of an
            // ancestor (the two motions are not stacked).
            let eff = info.bone.map(|b| skin_mats[b]).or_else(|| info.parent.map(|pa| eff_attach[pa])).unwrap_or(Affine2::IDENTITY);
            let chain = match info.parent {
                Some(pa) => chain_no_bone[pa].mul(local),
                None => local,
            };
            eff_attach.push(eff);
            chain_no_bone.push(chain);
            let m = eff.mul(chain);
            stage.push(m);
            let screen = cams[&info.root].mul(m);
            let layer_tick = match &layer.wobble {
                Some(w) => (frame / p.fps * w.rate).floor(),
                None => 0.0,
            };

            let mut image = None;
            let (kind, paths, children) = match &layer.kind {
                LayerKind::Vector { .. } => {
                    (NodeKind::Vector, self.vector_geometry(id, &st, &skin_mats, layer_tick), Vec::new())
                }
                LayerKind::Group { .. } => (NodeKind::Group, Vec::new(), info.children.clone()),
                LayerKind::Switch { .. } => {
                    let n = info.children.len();
                    let sel = if n == 0 { None } else { Some((lp.switch.round().max(0.0) as usize).min(n - 1)) };
                    (NodeKind::Switch, Vec::new(), sel.map(|s| vec![info.children[s]]).unwrap_or_default())
                }
                LayerKind::Image { .. } => {
                    image = self.image_draw(id, &st, &skin_mats, layer_tick);
                    (NodeKind::Image, Vec::new(), Vec::new())
                }
                LayerKind::Liquid { blobs, threshold, cell, fill, stroke } => {
                    let pts: Vec<(Vec2, f32)> = blobs
                        .iter()
                        .map(|b| match b.bone.as_ref().and_then(|n| self.bone_ix.get(n)) {
                            Some(&bi) => (world[bi].apply(b.pos), b.radius * world[bi].x_scale().abs()),
                            None => (b.pos, b.radius),
                        })
                        .collect();
                    (NodeKind::Vector, crate::fx::liquid_paths(&pts, *threshold, *cell, fill, stroke), Vec::new())
                }
                LayerKind::Boolean { op, paths } => {
                    (NodeKind::Vector, crate::fx::boolean_paths(paths, *op, 0.25), Vec::new())
                }
                LayerKind::Emitter { spec } => {
                    let origin = match spec.bone.as_ref().and_then(|n| self.bone_ix.get(n)) {
                        Some(&bi) => world[bi].apply(spec.pos),
                        None => spec.pos,
                    };
                    let ps = spec.particles(frame / p.fps, origin);
                    (NodeKind::Vector, crate::fx::particle_paths(&ps), Vec::new())
                }
            };

            out.push(RenderNode {
                id,
                name: info.path.clone(),
                kind,
                world: screen,
                opacity: lp.opacity,
                blend: layer.blend,
                visible: layer.visible && (!layer.reference || self.show_reference),
                mask: info.mask,
                paths,
                image,
                children,
                effects: self.node_effects(id, layer, &st),
            });
        }

        if let Some(l) = &self.links {
            l.apply(&mut out, &stage);
        }
        let camera = self.camera_matrix(&st.cam, 1.0);
        let mut f = Frame { world, camera, nodes: out, roots: self.roots.clone(), nonfinite: None };
        self.repair_nonfinite(&mut f);
        f
    }

    /// Hide nodes with non-finite numbers and reset non-finite bone transforms, remembering the first place found.
    fn repair_nonfinite(&self, f: &mut Frame) {
        let mut first: Option<String> = None;
        for (i, w) in f.world.iter_mut().enumerate() {
            if !aff_finite(w) {
                first.get_or_insert_with(|| format!("bone '{}' (transform)", self.project.bones.get(i).map(|b| b.name.as_str()).unwrap_or("?")));
                *w = Affine2::IDENTITY;
            }
        }
        if !aff_finite(&f.camera) {
            first.get_or_insert_with(|| "the camera".to_string());
            f.camera = Affine2::IDENTITY;
        }
        for n in f.nodes.iter_mut() {
            let bad_geometry = n.paths.iter().any(|p| p.anchors.iter().any(|a| !(a.pos.x.is_finite() && a.pos.y.is_finite())));
            let what = if !aff_finite(&n.world) {
                Some("transform")
            } else if !n.opacity.is_finite() {
                Some("opacity")
            } else if bad_geometry {
                Some("geometry")
            } else {
                None
            };
            if let Some(w) = what {
                first.get_or_insert_with(|| format!("layer '{}' ({w})", n.name));
                n.world = Affine2::IDENTITY;
                n.opacity = 0.0;
                n.visible = false;
                n.paths.clear();
                n.image = None;
            }
        }
        f.nonfinite = first;
    }

    /// Evaluate the frame and refuse it if a number overflowed (see [`Frame::nonfinite`]).
    pub fn evaluate_checked(&self, frame: f32) -> Result<Frame, String> {
        let f = self.evaluate(frame);
        f.check_finite().map_err(|e| format!("{e} at frame {frame}"))?;
        Ok(f)
    }

    /// Evaluate every frame and report the first one with a non-finite number. Exporters call this before
    /// writing anything.
    pub fn check_all_frames_finite(&self) -> Result<(), String> {
        for fr in 0..self.project.frames {
            self.evaluate_checked(fr as f32)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::{Ease, Track};
    use crate::path::{Anchor, BoneWeight};
    use crate::project::{Bone, Channel, IkChain, MaskRef, Transform2D};

    fn bone(name: &str, parent: Option<&str>, len: f32, off: Vec2, rest: f32) -> Bone {
        Bone::new(name, parent, len, off, rest)
    }

    fn empty_project() -> Project {
        Project::new("t", 100.0, 100.0, 24.0, 10)
    }

    fn tip_of(e: &Engine, f: &Frame, name: &str) -> Vec2 {
        let i = e.project.bones.iter().position(|b| b.name == name).unwrap();
        f.world[i].apply(Vec2::new(e.project.bones[i].length, 0.0))
    }

    fn arm_with_target(tx: f32, ty: f32) -> Project {
        let mut p = empty_project();
        p.bones = vec![
            bone("a", None, 100.0, Vec2::ZERO, 0.0),
            bone("b", Some("a"), 100.0, Vec2::new(100.0, 0.0), 0.0),
            bone("t", None, 5.0, Vec2::new(tx, ty), 0.0),
        ];
        p.ik = vec![IkChain::two_bone("arm", "a", "b", "t", 1.0)];
        p
    }

    fn two(a: f32, b: f32) -> Track {
        Track::with_keys(&[(0.0, a, Ease::Linear), (10.0, b, Ease::Linear)])
    }

    // ---- rigging (carried over) ----

    #[test]
    fn ik_reaches_reachable_target() {
        let e = Engine::new(arm_with_target(120.0, 80.0)).unwrap();
        let f = e.evaluate(0.0);
        let tip = tip_of(&e, &f, "b");
        assert!(tip.distance(Vec2::new(120.0, 80.0)) < 0.05, "tip at {tip:?}");
        let joint = f.world[1].apply(Vec2::ZERO);
        assert!((joint.length() - 100.0).abs() < 0.05);
    }

    #[test]
    fn ik_clamps_unreachable_target_in_a_straight_line() {
        let e = Engine::new(arm_with_target(0.0, 500.0)).unwrap();
        let f = e.evaluate(0.0);
        let tip = tip_of(&e, &f, "b");
        assert!(tip.distance(Vec2::new(0.0, 200.0)) < 0.5, "tip at {tip:?}");
    }

    #[test]
    fn ik_bend_flips_the_joint_side() {
        let mut p = arm_with_target(120.0, 0.0);
        p.ik[0].bend = -1.0;
        let down = Engine::new(p.clone()).unwrap().evaluate(0.0).world[1].apply(Vec2::ZERO).y;
        p.ik[0].bend = 1.0;
        let up = Engine::new(p).unwrap().evaluate(0.0).world[1].apply(Vec2::ZERO).y;
        assert!(down * up < 0.0, "elbows on opposite sides: {down} vs {up}");
    }

    #[test]
    fn full_weight_anchor_follows_its_bone() {
        let mut p = empty_project();
        p.bones = vec![bone("a", None, 50.0, Vec2::new(10.0, 10.0), 0.0)];
        let mut anchor = Anchor::corner(Vec2::new(60.0, 10.0));
        anchor.weights = vec![BoneWeight { bone: "a".into(), weight: 3.0 }];
        p.layers = vec![Layer::vector("L", vec![VPath::new("p", false, vec![anchor])])];
        p.channels = vec![Channel::keys("a", Prop::BoneAngle, two(0.0, 90.0))];
        let e = Engine::new(p).unwrap();
        let rest = e.evaluate(0.0).nodes[0].paths[0].anchors[0].pos;
        assert!(rest.distance(Vec2::new(60.0, 10.0)) < 1e-3);
        let turned = e.evaluate(10.0).nodes[0].paths[0].anchors[0].pos;
        assert!(turned.distance(Vec2::new(10.0, 60.0)) < 1e-3, "{turned:?}");
    }

    #[test]
    fn auto_skin_binds_to_the_nearest_bone_and_recurses_into_groups() {
        let mut p = empty_project();
        p.bones = vec![
            bone("left", None, 40.0, Vec2::new(0.0, 0.0), 0.0),
            bone("right", None, 40.0, Vec2::new(200.0, 0.0), 0.0),
        ];
        p.layers = vec![Layer::group(
            "G",
            vec![Layer::vector("L", vec![VPath::new("p", false, vec![Anchor::corner(Vec2::new(20.0, 2.0))])])],
        )];
        p.auto_skin("G", 2).unwrap();
        let LayerKind::Group { layers } = &p.layers[0].kind else { panic!() };
        let LayerKind::Vector { paths } = &layers[0].kind else { panic!() };
        let w = &paths[0].anchors[0].weights;
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].bone, "left");
        assert!(w[0].weight > 0.99);
        assert!((w.iter().map(|x| x.weight).sum::<f32>() - 1.0).abs() < 1e-5);
    }

    // ---- validation ----

    #[test]
    fn validation_reports_all_problems() {
        let mut p = arm_with_target(0.0, 0.0);
        p.ik[0].target = "nope".into();
        p.channels = vec![Channel::keys("ghost", Prop::BoneAngle, Track::new())];
        let errs = Engine::new(p).err().unwrap();
        assert!(errs.len() >= 2, "{errs:?}");
    }

    #[test]
    fn child_listed_before_parent_is_rejected() {
        let mut p = empty_project();
        p.bones = vec![bone("c", Some("p"), 1.0, Vec2::ZERO, 0.0), bone("p", None, 1.0, Vec2::ZERO, 0.0)];
        assert!(Engine::new(p).is_err());
    }

    #[test]
    fn bad_layer_references_are_rejected() {
        let mut p = empty_project();
        let mut a = Layer::vector("a", vec![]);
        a.mask = Some(MaskRef { layer: "missing".into(), invert: false });
        let mut b = Layer::vector("b", vec![]);
        b.bone = Some("nobone".into());
        p.layers = vec![a, b, Layer::vector("b", vec![])];
        let errs = Engine::new(p).err().unwrap();
        assert!(errs.iter().any(|e| e.contains("unknown mask")));
        assert!(errs.iter().any(|e| e.contains("unknown bone")));
        assert!(errs.iter().any(|e| e.contains("duplicate layer")));
    }

    #[test]
    fn mask_cycles_and_self_masks_are_rejected() {
        let mut p = empty_project();
        let mut a = Layer::vector("a", vec![]);
        a.mask = Some(MaskRef { layer: "b".into(), invert: false });
        let mut b = Layer::vector("b", vec![]);
        b.mask = Some(MaskRef { layer: "a".into(), invert: false });
        let mut c = Layer::vector("c", vec![]);
        c.mask = Some(MaskRef { layer: "c".into(), invert: false });
        p.layers = vec![a, b, c];
        let errs = Engine::new(p).err().unwrap();
        assert!(errs.iter().any(|e| e.contains("cycle")), "{errs:?}");
        assert!(errs.iter().any(|e| e.contains("itself")), "{errs:?}");
    }

    #[test]
    fn layer_switch_on_a_non_switch_layer_is_rejected() {
        let mut p = empty_project();
        p.layers = vec![Layer::vector("v", vec![])];
        p.channels = vec![Channel::keys("v", Prop::LayerSwitch, Track::new())];
        assert!(Engine::new(p).is_err());
    }

    #[test]
    fn bad_expressions_and_get_references_are_rejected() {
        let mut p = arm_with_target(0.0, 0.0);
        p.channels = vec![
            Channel::expr("a", Prop::BoneAngle, "1 +", Mode::Replace),
            Channel::expr("a", Prop::BoneAngle, "get(\"zzz\", \"bone_angle\")", Mode::Replace),
            Channel::expr("a", Prop::BoneAngle, "get(\"a\", \"nonsense\")", Mode::Replace),
        ];
        let errs = Engine::new(p).err().unwrap();
        assert_eq!(errs.len(), 3, "{errs:?}");
    }

    // ---- layers, transforms, camera ----

    fn rect_layer(name: &str) -> Layer {
        Layer::vector(name, vec![VPath::rect("r", 0.0, 0.0, 10.0, 10.0)])
    }

    #[test]
    fn group_transform_is_inherited_by_children() {
        let mut p = empty_project();
        let mut g = Layer::group("g", vec![rect_layer("c")]);
        g.transform = Transform2D { position: Vec2::new(30.0, 0.0), scale: Vec2::new(2.0, 2.0), ..Default::default() };
        p.layers = vec![g];
        let e = Engine::new(p).unwrap();
        let f = e.evaluate(0.0);
        let child = &f.nodes[e.node_index("g/c").unwrap()];
        assert!(child.world.apply(Vec2::new(5.0, 5.0)).distance(Vec2::new(40.0, 10.0)) < 1e-4);
    }

    #[test]
    fn layer_channels_animate_transform_and_opacity() {
        let mut p = empty_project();
        p.layers = vec![rect_layer("r")];
        p.channels = vec![
            Channel::keys("r", Prop::LayerX, two(0.0, 50.0)),
            Channel::keys("r", Prop::LayerRotation, two(0.0, 90.0)),
            Channel::keys("r", Prop::LayerOpacity, two(1.0, 0.0)),
        ];
        let e = Engine::new(p).unwrap();
        let f = e.evaluate(5.0);
        let n = &f.nodes[0];
        assert!((n.opacity - 0.5).abs() < 1e-5);
        // rotate 45° then move +25 in x: (1,0) -> (cos45, sin45) + (25,0)
        let w = n.world.apply(Vec2::new(1.0, 0.0));
        assert!(w.distance(Vec2::new(25.0 + std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2)) < 1e-3, "{w:?}");
    }

    #[test]
    fn bone_attached_layer_follows_its_bone() {
        let mut p = empty_project();
        p.bones = vec![bone("a", None, 50.0, Vec2::new(10.0, 10.0), 0.0)];
        let mut hat = rect_layer("hat");
        hat.bone = Some("a".into());
        p.layers = vec![hat];
        p.channels = vec![Channel::keys("a", Prop::BoneAngle, two(0.0, 90.0))];
        let e = Engine::new(p).unwrap();
        let at0 = e.evaluate(0.0).nodes[0].world.apply(Vec2::new(60.0, 10.0));
        assert!(at0.distance(Vec2::new(60.0, 10.0)) < 1e-3);
        let at10 = e.evaluate(10.0).nodes[0].world.apply(Vec2::new(60.0, 10.0));
        assert!(at10.distance(Vec2::new(10.0, 60.0)) < 1e-3, "{at10:?}");
    }

    /// L-71 (fixed in wave 7): a bone-attached layer inside a transformed group composes `skin * parent_stage * local`.
    /// The skin matrix (bone world * inverse rest) is a stage-space motion, so a group scaled 2 no longer doubles the
    /// distance the child travels; at rest nothing changes.
    #[test]
    fn bone_attach_under_a_transformed_group_composes_bone_after_the_group() {
        let build = |attach: bool| {
            let mut p = empty_project();
            p.bones = vec![bone("a", None, 50.0, Vec2::new(10.0, 10.0), 0.0)];
            let mut child = rect_layer("c");
            if attach {
                child.bone = Some("a".into());
            }
            let mut g = Layer::group("g", vec![child]);
            g.transform.position = Vec2::new(100.0, 0.0);
            g.transform.scale = Vec2::new(2.0, 2.0);
            p.layers = vec![g];
            p.channels = vec![Channel::keys("a", Prop::BoneAngle, two(0.0, 90.0))];
            Engine::new(p).unwrap()
        };
        let (with, without) = (build(true), build(false));
        let q = Vec2::new(60.0, 10.0);
        let node = |e: &Engine, f: f32| e.evaluate(f).nodes.iter().find(|n| n.name == "g/c").unwrap().world;
        // at rest: identical (the bone adds nothing)
        assert!(node(&with, 0.0).apply(q).distance(node(&without, 0.0).apply(q)) < 1e-4);
        // posed: the skin matrix is applied to the child's stage position, once
        let skin = with.evaluate(10.0).world[0].mul(with.inv_rest[0]);
        let group = node(&without, 10.0);
        let expected = skin.apply(group.apply(q));
        assert!(node(&with, 10.0).apply(q).distance(expected) < 1e-3, "world = skin * parent_stage * local");
        // the travel equals the travel of the same point attached with an identity group: not doubled
        let travel = node(&with, 10.0).apply(q).distance(node(&with, 0.0).apply(q));
        let direct = skin.apply(node(&with, 0.0).apply(q)).distance(node(&with, 0.0).apply(q));
        assert!((travel - direct).abs() < 1e-3, "travel {travel} vs bone motion {direct}");
    }

    /// A layer without a bone follows its bone-attached ancestor; a layer's own bone replaces the ancestor's.
    #[test]
    fn bone_attach_is_inherited_and_a_child_bone_replaces_the_parents() {
        let mut p = empty_project();
        p.bones = vec![bone("a", None, 50.0, Vec2::new(10.0, 10.0), 0.0), bone("b", None, 50.0, Vec2::new(10.0, 80.0), 0.0)];
        let mut inner = Layer::group("inner", vec![rect_layer("x")]);
        inner.bone = Some("b".into());
        let plain = rect_layer("y");
        let mut outer = Layer::group("outer", vec![inner, plain]);
        outer.bone = Some("a".into());
        p.layers = vec![outer];
        p.channels = vec![Channel::keys("a", Prop::BoneAngle, two(0.0, 90.0))];
        let e = Engine::new(p).unwrap();
        let w = |name: &str, f: f32| e.evaluate(f).nodes.iter().find(|n| n.name == name).unwrap().world;
        let q = Vec2::new(60.0, 10.0);
        let skin_a = e.evaluate(10.0).world[0].mul(e.inv_rest[0]);
        // y has no bone of its own: follows a
        assert!(w("outer/y", 10.0).apply(q).distance(skin_a.apply(w("outer/y", 0.0).apply(q))) < 1e-3);
        // x sits under inner (bone b, which does not move): it does not also take a's motion
        assert!(w("outer/inner/x", 10.0).apply(q).distance(w("outer/inner/x", 0.0).apply(q)) < 1e-3);
    }

    #[test]
    fn switch_layer_selects_one_child_with_step_keys() {
        let mut p = empty_project();
        p.layers = vec![Layer::switch("mouth", vec![rect_layer("a"), rect_layer("b"), rect_layer("c")])];
        p.channels = vec![Channel::keys(
            "mouth",
            Prop::LayerSwitch,
            Track::with_keys(&[(0.0, 0.0, Ease::Step), (4.0, 2.0, Ease::Step), (8.0, 1.0, Ease::Step)]),
        )];
        let e = Engine::new(p).unwrap();
        let shown = |f: f32| {
            let fr = e.evaluate(f);
            let ch = fr.nodes[0].children.clone();
            assert_eq!(ch.len(), 1);
            fr.nodes[ch[0]].name.clone()
        };
        assert_eq!(shown(0.0), "mouth/a");
        assert_eq!(shown(3.9), "mouth/a");
        assert_eq!(shown(4.0), "mouth/c");
        assert_eq!(shown(9.0), "mouth/b");
    }

    #[test]
    fn camera_pan_zoom_and_parallax() {
        let mut p = empty_project(); // 100×100 stage
        let mut far = rect_layer("far");
        far.parallax = 0.5;
        let mut hud = rect_layer("hud");
        hud.parallax = 0.0;
        p.layers = vec![far, rect_layer("near"), hud];
        p.channels = vec![Channel::keys("", Prop::CameraX, two(50.0, 70.0))];
        let e = Engine::new(p).unwrap();
        let f = e.evaluate(10.0); // camera moved +20 in x
        let sx = |name: &str| f.nodes[e.node_index(name).unwrap()].world.apply(Vec2::ZERO).x;
        assert!((sx("near") - -20.0).abs() < 1e-4, "near scrolls fully");
        assert!((sx("far") - -10.0).abs() < 1e-4, "far scrolls at half speed");
        assert!((sx("hud") - 0.0).abs() < 1e-4, "hud is pinned to the screen");

        let mut p2 = empty_project();
        p2.layers = vec![rect_layer("r")];
        p2.camera.zoom = 2.0;
        let e2 = Engine::new(p2).unwrap();
        // zoom about the stage centre: stage centre stays put, origin goes to -50
        let w = &e2.evaluate(0.0).nodes[0].world;
        assert!(w.apply(Vec2::new(50.0, 50.0)).distance(Vec2::new(50.0, 50.0)) < 1e-4);
        assert!(w.apply(Vec2::ZERO).distance(Vec2::new(-50.0, -50.0)) < 1e-4);
    }

    // ---- procedural channels ----

    #[test]
    fn expression_channels_see_time_and_other_values() {
        let mut p = arm_with_target(120.0, 0.0);
        p.ik.clear();
        p.channels = vec![
            Channel::keys("a", Prop::BoneAngle, two(0.0, 40.0)),
            // b mirrors a at half strength, driven by an expression reading a's current angle
            Channel::expr("b", Prop::BoneAngle, "get(\"a\", \"bone_angle\") * -0.5", Mode::Replace),
            Channel::expr("t", Prop::BoneOffsetY, "10 * sin(pi * t)", Mode::Replace),
        ];
        let e = Engine::new(p).unwrap();
        let f = e.evaluate(10.0);
        assert!((f.world[1].rotation().to_degrees() - (40.0 - 20.0)).abs() < 1e-3);
        let f5 = e.evaluate(1.0); // 10 frames → t = 0.1 → sin(0.1π)·10
        assert!((f5.world[2].apply(Vec2::ZERO).y - 10.0 * (0.1f32 * std::f32::consts::PI).sin()).abs() < 1e-3);
    }

    #[test]
    fn noise_adds_on_top_of_keys_and_is_deterministic() {
        let mut p = arm_with_target(120.0, 0.0);
        p.ik.clear();
        p.channels = vec![
            Channel::keys("a", Prop::BoneAngle, two(0.0, 0.0)),
            Channel::noise("a", Prop::BoneAngle, 10.0, 3.0, 5),
        ];
        let e = Engine::new(p.clone()).unwrap();
        let ang = |f: f32| e.evaluate(f).world[0].rotation().to_degrees();
        assert!((0..10).any(|f| ang(f as f32).abs() > 0.5), "noise moves the bone");
        assert!((0..10).all(|f| ang(f as f32).abs() <= 10.0 + 1e-3));
        assert_eq!(ang(3.0), ang(3.0));
        assert_eq!(ang(3.0), Engine::new(p).unwrap().evaluate(3.0).world[0].rotation().to_degrees());
    }

    #[test]
    fn driven_keys_map_one_value_to_another() {
        let mut p = arm_with_target(120.0, 0.0);
        p.ik.clear();
        p.layers = vec![rect_layer("bulge")];
        // elbow bend (b's angle, 0..90°) drives the bulge layer's x-scale 1 → 2
        p.channels = vec![
            Channel::keys("b", Prop::BoneAngle, two(0.0, 90.0)),
            Channel::driven(
                "bulge",
                Prop::LayerScaleX,
                "b",
                Prop::BoneAngle,
                Track::with_keys(&[(0.0, 1.0, Ease::Linear), (90.0, 2.0, Ease::Linear)]),
            ),
        ];
        let e = Engine::new(p).unwrap();
        let sx = |f: f32| e.evaluate(f).nodes[0].world.apply(Vec2::new(1.0, 0.0)).x;
        assert!((sx(0.0) - 1.0).abs() < 1e-4);
        assert!((sx(5.0) - 1.5).abs() < 1e-3);
        assert!((sx(10.0) - 2.0).abs() < 1e-4);
    }

    #[test]
    fn channel_modes_and_weight() {
        let mut p = arm_with_target(120.0, 0.0);
        p.ik.clear();
        p.channels = vec![
            Channel::keys("a", Prop::BoneScale, two(2.0, 2.0)),
            Channel::expr("a", Prop::BoneScale, "3", Mode::Multiply),
            Channel::expr("a", Prop::BoneOffsetX, "10", Mode::Replace).with_weight(0.5),
            Channel::expr("a", Prop::BoneOffsetX, "4", Mode::Add),
        ];
        let e = Engine::new(p).unwrap();
        let f = e.evaluate(0.0);
        assert!((f.world[0].x_scale() - 6.0).abs() < 1e-4, "2 × 3");
        assert!((f.world[0].apply(Vec2::ZERO).x - 9.0).abs() < 1e-4, "0→5 (50% toward 10) then +4");
    }

    #[test]
    fn evaluate_is_a_pure_function_of_frame() {
        let mut p = arm_with_target(120.0, 40.0);
        p.channels = vec![Channel::noise("t", Prop::BoneOffsetX, 20.0, 2.0, 1)];
        let e = Engine::new(p).unwrap();
        let a = e.evaluate(7.5).world[1];
        let _ = e.evaluate(2.0);
        let b = e.evaluate(7.5).world[1];
        assert_eq!(a, b);
    }

    // ---- large finite values (R1) ----

    fn one_anchor_project() -> Project {
        let mut p = empty_project();
        p.layers = vec![Layer::vector("L", vec![VPath::new("p", false, vec![Anchor::corner(Vec2::new(5.0, 5.0))])])];
        p
    }

    #[test]
    fn channel_expressions_with_absurd_nesting_are_rejected_not_fatal() {
        let mut p = arm_with_target(0.0, 0.0);
        p.channels = vec![
            Channel::expr("a", Prop::BoneAngle, &format!("{}1{}", "(".repeat(20_000), ")".repeat(20_000)), Mode::Replace),
            Channel::expr("a", Prop::BoneAngle, &format!("{}1", "-".repeat(200_000)), Mode::Replace),
        ];
        let errs = Engine::new(p).err().unwrap();
        assert_eq!(errs.len(), 2, "{errs:?}");
    }

    #[test]
    fn huge_but_finite_numbers_are_refused_with_the_place() {
        // each used to give exit 0 with NaN / inf / null in the output
        let mut p = empty_project();
        p.camera.zoom = 3.0e38;
        let err = Engine::new(p).err().unwrap().join("; ");
        assert!(err.contains("/camera/zoom") && err.contains("outside the supported range"), "{err}");

        let mut p = one_anchor_project();
        p.layers[0].transform.scale = Vec2::new(3.0e38, 1.0);
        assert!(Engine::new(p).is_err());

        let mut p = empty_project();
        p.bones = vec![bone("a", None, 10.0, Vec2::new(3.0e38, 0.0), 0.0)];
        assert!(Engine::new(p).is_err());

        let mut p = empty_project();
        p.layers = vec![Layer::vector("L", vec![VPath::new("p", false, vec![Anchor::corner(Vec2::new(3.0e38, 0.0))])])];
        let err = Engine::new(p).err().unwrap().join("; ");
        assert!(err.contains("anchors"), "{err}");

        let mut p = one_anchor_project();
        p.layers[0].transform.position = Vec2::new(3.4e38, 0.0);
        assert!(Engine::new(p).is_err());
    }

    #[test]
    fn huge_literals_in_json_are_refused_at_load_not_turned_into_infinity() {
        // 1e300 does not fit an f32: serde makes it infinity, which then serialises as null and slips past any later scan
        let base = serde_json::to_value(one_anchor_project()).unwrap();
        let cases: [(&str, serde_json::Value); 4] = [
            ("/camera/zoom", serde_json::json!(1e300)),
            ("/layers/0/transform/scale", serde_json::json!([1e300, 1.0])),
            ("/layers/0/paths/0/anchors/0/pos", serde_json::json!([1e300, 0.0])),
            ("/bones", serde_json::json!([{"name": "a", "parent": null, "length": 5.0, "rest_offset": [1e308, 0.0], "rest_angle": 0.0}])),
        ];
        for (ptr, val) in cases {
            let mut v = base.clone();
            let target = if ptr == "/bones" { Some(&mut v) } else { None };
            match target {
                Some(t) => t["bones"] = val,
                None => {
                    // layers/0/paths is only valid for vector layers: set through the pointer
                    *v.pointer_mut(ptr).unwrap_or_else(|| panic!("{ptr} missing in {base}")) = val;
                }
            }
            let err = Project::from_json(&v.to_string()).err().unwrap_or_else(|| panic!("{ptr} was accepted")).to_string();
            assert!(err.contains("outside the supported range"), "{ptr}: {err}");
        }
        assert!(Project::from_json(&serde_json::to_string(&base).unwrap()).is_ok());
    }

    #[test]
    fn ordinary_large_numbers_still_load() {
        let mut p = one_anchor_project();
        p.layers[0].transform.position = Vec2::new(9.0e6, -9.0e6);
        p.camera.zoom = 100.0;
        assert!(Engine::new(p).is_ok());
    }

    #[test]
    fn overflow_through_nested_scales_is_reported_not_exported_as_nan() {
        // every number is within range, the product of six nested scales is not an f32
        let mut inner = Layer::vector("L", vec![VPath::new("p", false, vec![Anchor::corner(Vec2::new(5.0, 5.0))])]);
        inner.transform.scale = Vec2::new(1e7, 1e7);
        let mut cur = inner;
        for i in 0..6 {
            let mut g = Layer::group(&format!("g{i}"), vec![cur]);
            g.transform.scale = Vec2::new(1e7, 1e7);
            cur = g;
        }
        let mut p = empty_project();
        p.layers = vec![cur];
        let e = Engine::new(p).unwrap();
        let f = e.evaluate(0.0);
        assert!(f.nonfinite.is_some());
        for n in &f.nodes {
            let m = n.world;
            assert!([m.a, m.b, m.c, m.d, m.tx, m.ty].iter().all(|v| v.is_finite()), "repaired frames carry no NaN");
        }
        let err = e.evaluate_checked(0.0).err().unwrap();
        assert!(err.starts_with("non-finite value in ") && err.contains("at frame 0"), "{err}");
        assert!(e.check_all_frames_finite().unwrap_err().starts_with("non-finite value in "));
        // a normal project passes
        assert!(Engine::new(one_anchor_project()).unwrap().check_all_frames_finite().is_ok());
    }
}

#[cfg(test)]
mod reference_tests {
    use super::*;
    use crate::path::VPath;
    use crate::project::Layer;

    #[test]
    fn reference_layers_are_hidden_unless_the_editor_asks() {
        let mut p = Project::new("r", 50.0, 50.0, 24.0, 2);
        let mut l = Layer::vector("sketch", vec![VPath::rect("r", 0.0, 0.0, 10.0, 10.0).with_fill("#ff0000")]);
        l.reference = true;
        p.layers = vec![l];
        let mut e = Engine::new(p.clone()).unwrap();
        assert!(!e.evaluate(0.0).nodes[0].visible);
        e.show_reference = true;
        assert!(e.evaluate(0.0).nodes[0].visible);
        let j = p.to_json_pretty();
        assert!(j.contains("\"reference\": true"));
        assert!(!Project::new("x", 1.0, 1.0, 24.0, 1).to_json_pretty().contains("reference"), "default is not serialised");
    }
}
