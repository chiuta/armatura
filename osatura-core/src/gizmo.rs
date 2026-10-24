// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
//! Canvas transform gizmos: the maths behind drag handles and the edit plan they produce.
//!
//! A drag is described by a handle, the pointer position when it started and the pointer offset
//! since. [`drag_matrix`] turns that into one affine map `D` in canvas space (plus snapping
//! guides); [`plan`] converts `D` into property edits for every selected layer / bone / control
//! point, and [`commit`] applies them as **one** [`Op::Batch`], so one drag is one undo step.
//!
//! * translate: axis constraint (`constrain`), snap of the selection box's edges and centre lines
//!   to a grid, to other layers' bounds and centres and to the canvas, within a pixel tolerance;
//! * rotate: around the box centre (or a given pivot), `constrain` snaps the angle to 15 degrees;
//! * scale: 8 handles on a box that follows a single layer's own axes; edges scale one axis,
//!   corners both; `constrain` keeps proportions, `center` scales around the centre; dragging
//!   across the opposite edge flips (negative scale);
//! * bone ends: the head moves the bone, the tip rotates (and stretches unless `constrain`);
//! * control points: anchors, bezier handles, deformer points and free-mesh vertices.
//!
//! Auto-key: a layer / point property that already has a keyed channel is keyed at the current
//! frame; with `auto_key` any edited property is. Otherwise a layer edit changes the static
//! transform. Bone edits always key the pose (the static value would be the rest pose).
//!
//! Limits: the key value assumes the keyed channel is the replace-mode source of the property and
//! that any other contributing channel runs after it; snapping is done in canvas (camera-applied)
//! space; scale does not snap; handles of skinned or wobbling paths are edited in rest space.

use crate::anim::Ease;
use crate::edit::{Editor, Op};
use crate::engine::{Engine, Frame};
use crate::math::{Affine2, Vec2};
use crate::pick::{self, bone_ends, box_frame, BoxFrame, PointKind, Selection, Target};
use crate::project::{Channel, Deformer, LayerKind, Prop, Source, Transform2D};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Distance of the rotate knob from the box edge, canvas units.
pub const ROTATE_OFFSET: f32 = 26.0;
/// Smallest box extent the gizmo shows, canvas units.
pub const MIN_EXTENT: f32 = 10.0;

// ---------------------------------------------------------------------------------------------
// Handles
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScaleHandle {
    Nw,
    N,
    Ne,
    E,
    Se,
    S,
    Sw,
    W,
}

pub const SCALE_HANDLES: [ScaleHandle; 8] = [
    ScaleHandle::Nw,
    ScaleHandle::Ne,
    ScaleHandle::Se,
    ScaleHandle::Sw,
    ScaleHandle::N,
    ScaleHandle::E,
    ScaleHandle::S,
    ScaleHandle::W,
];

impl ScaleHandle {
    /// Position in the box's unit square.
    pub fn local(self) -> (f32, f32) {
        match self {
            ScaleHandle::Nw => (0.0, 0.0),
            ScaleHandle::N => (0.5, 0.0),
            ScaleHandle::Ne => (1.0, 0.0),
            ScaleHandle::E => (1.0, 0.5),
            ScaleHandle::Se => (1.0, 1.0),
            ScaleHandle::S => (0.5, 1.0),
            ScaleHandle::Sw => (0.0, 1.0),
            ScaleHandle::W => (0.0, 0.5),
        }
    }
    pub fn is_corner(self) -> bool {
        matches!(
            self,
            ScaleHandle::Nw | ScaleHandle::Ne | ScaleHandle::Se | ScaleHandle::Sw
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Handle {
    Move,
    Rotate,
    Scale { at: ScaleHandle },
    BoneHead,
    BoneTip,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HandlePos {
    pub handle: Handle,
    pub pos: Vec2,
    /// CSS cursor name for hover feedback.
    pub cursor: String,
}

fn cursor_for_dir(d: Vec2) -> &'static str {
    let a = d.y.atan2(d.x).to_degrees().rem_euclid(180.0);
    if !(22.5..157.5).contains(&a) {
        "ew-resize"
    } else if a < 67.5 {
        "nwse-resize"
    } else if a < 112.5 {
        "ns-resize"
    } else {
        "nesw-resize"
    }
}

/// Handle positions for a box gizmo: 8 scale handles and the rotate knob above the top edge.
pub fn box_handles(bx: &BoxFrame) -> Vec<HandlePos> {
    let c = bx.centre();
    let mut v: Vec<HandlePos> = SCALE_HANDLES
        .iter()
        .map(|&h| {
            let (s, t) = h.local();
            let p = bx.point(s, t);
            HandlePos {
                handle: Handle::Scale { at: h },
                pos: p,
                cursor: cursor_for_dir(p - c).into(),
            }
        })
        .collect();
    let lv = bx.v.length().max(1e-6);
    let up = bx.v * (-1.0 / lv);
    v.push(HandlePos {
        handle: Handle::Rotate,
        pos: bx.point(0.5, 0.0) + up * ROTATE_OFFSET,
        cursor: "grab".into(),
    });
    v
}

/// Which handle (if any) lies under `p`. The rotate knob and corners win over edges; `Move` is
/// reported for a point inside the box.
pub fn hit_handle(bx: &BoxFrame, p: Vec2, radius: f32) -> Option<Handle> {
    let hs = box_handles(bx);
    let order = |h: &Handle| match h {
        Handle::Rotate => 0,
        Handle::Scale { at } if at.is_corner() => 1,
        _ => 2,
    };
    let mut best: Option<(&HandlePos, f32)> = None;
    for h in &hs {
        let d = h.pos.distance(p);
        if d <= radius {
            let better = match best {
                None => true,
                Some((b, bd)) => {
                    order(&h.handle) < order(&b.handle)
                        || (order(&h.handle) == order(&b.handle) && d < bd)
                }
            };
            if better {
                best = Some((h, d));
            }
        }
    }
    if let Some((h, _)) = best {
        return Some(h.handle);
    }
    if bx.contains(p) {
        return Some(Handle::Move);
    }
    None
}

/// What to draw for the current selection.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GizmoView {
    /// `none`, `box`, `bone` or `points`.
    pub kind: String,
    pub corners: Option<[Vec2; 4]>,
    pub handles: Vec<HandlePos>,
    pub points: Vec<Vec2>,
}

pub fn gizmo_view(engine: &Engine, frame: &Frame, sel: &Selection) -> GizmoView {
    let mut g = GizmoView {
        kind: "none".into(),
        ..Default::default()
    };
    if sel.is_empty() {
        return g;
    }
    if sel.items.iter().all(|t| matches!(t, Target::Point { .. })) {
        g.kind = "points".into();
        g.points = sel
            .items
            .iter()
            .flat_map(|t| pick::target_points(engine, frame, t))
            .collect();
        return g;
    }
    if let [Target::Bone { name }] = sel.items.as_slice() {
        if let Some((h, t)) = bone_ends(engine, frame, name) {
            g.kind = "bone".into();
            g.handles = vec![
                HandlePos {
                    handle: Handle::BoneHead,
                    pos: h,
                    cursor: "move".into(),
                },
                HandlePos {
                    handle: Handle::BoneTip,
                    pos: t,
                    cursor: "crosshair".into(),
                },
            ];
            return g;
        }
    }
    if sel.items.iter().any(|t| matches!(t, Target::Point { .. })) {
        return g;
    }
    if let Some(bx) = box_frame(engine, frame, sel, MIN_EXTENT) {
        g.kind = "box".into();
        g.corners = Some(bx.corners());
        g.handles = box_handles(&bx);
    }
    g
}

// ---------------------------------------------------------------------------------------------
// Snapping
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SnapOptions {
    /// Grid spacing in canvas units; `None` = no grid snapping.
    pub grid: Option<f32>,
    /// Snap to other layers' bounds and centres.
    pub objects: bool,
    /// Snap to the canvas edges and centre.
    pub canvas: bool,
    /// Capture distance in canvas units.
    pub tolerance: f32,
}

impl Default for SnapOptions {
    fn default() -> Self {
        SnapOptions {
            grid: None,
            objects: true,
            canvas: true,
            tolerance: 6.0,
        }
    }
}

/// One snap line: a coordinate and the extent of the thing it came from along the other axis.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapLine {
    pub at: f32,
    pub from: f32,
    pub to: f32,
    /// `object`, `canvas` or `grid`.
    pub kind: String,
}

#[derive(Clone, Debug, Default)]
pub struct SnapTargets {
    pub xs: Vec<SnapLine>,
    pub ys: Vec<SnapLine>,
}

/// A line to draw while snapping (`axis` "x" = vertical line at x = `at`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Guide {
    pub axis: String,
    pub at: f32,
    pub from: f32,
    pub to: f32,
    pub kind: String,
}

/// Snap candidates from the canvas and from every visible layer outside the selection.
pub fn snap_targets(
    engine: &Engine,
    frame: &Frame,
    sel: &Selection,
    o: &SnapOptions,
) -> SnapTargets {
    let mut t = SnapTargets::default();
    let add = |t: &mut SnapTargets, lo: Vec2, hi: Vec2, kind: &str| {
        for x in [lo.x, (lo.x + hi.x) * 0.5, hi.x] {
            t.xs.push(SnapLine {
                at: x,
                from: lo.y,
                to: hi.y,
                kind: kind.into(),
            });
        }
        for y in [lo.y, (lo.y + hi.y) * 0.5, hi.y] {
            t.ys.push(SnapLine {
                at: y,
                from: lo.x,
                to: hi.x,
                kind: kind.into(),
            });
        }
    };
    if o.canvas {
        let p = &engine.project;
        let c: Vec<Vec2> = [
            Vec2::ZERO,
            Vec2::new(p.width, 0.0),
            Vec2::new(p.width, p.height),
            Vec2::new(0.0, p.height),
        ]
        .iter()
        .map(|q| frame.camera.apply(*q))
        .collect();
        let lo = Vec2::new(
            c.iter().map(|q| q.x).fold(f32::INFINITY, f32::min),
            c.iter().map(|q| q.y).fold(f32::INFINITY, f32::min),
        );
        let hi = Vec2::new(
            c.iter().map(|q| q.x).fold(f32::NEG_INFINITY, f32::max),
            c.iter().map(|q| q.y).fold(f32::NEG_INFINITY, f32::max),
        );
        add(&mut t, lo, hi, "canvas");
    }
    if o.objects {
        let selected: Vec<&str> = sel.layers();
        for id in pick::draw_order(frame) {
            let name = &frame.nodes[id].name;
            if selected
                .iter()
                .any(|s| name == s || name.starts_with(&format!("{s}/")))
            {
                continue;
            }
            let pts: Vec<Vec2> = pick::node_shapes(frame, id)
                .into_iter()
                .flat_map(|s| s.pts)
                .collect();
            if pts.is_empty() {
                continue;
            }
            let lo = Vec2::new(
                pts.iter().map(|q| q.x).fold(f32::INFINITY, f32::min),
                pts.iter().map(|q| q.y).fold(f32::INFINITY, f32::min),
            );
            let hi = Vec2::new(
                pts.iter().map(|q| q.x).fold(f32::NEG_INFINITY, f32::max),
                pts.iter().map(|q| q.y).fold(f32::NEG_INFINITY, f32::max),
            );
            add(&mut t, lo, hi, "object");
        }
    }
    t
}

/// Snap a translation of the extent `(lo, hi)`. Returns the adjusted delta and the guides.
pub fn snap_move(
    lo: Vec2,
    hi: Vec2,
    delta: Vec2,
    constrain: bool,
    o: &SnapOptions,
    t: &SnapTargets,
) -> (Vec2, Vec<Guide>) {
    let mut d = delta;
    let mut locked = [false, false];
    if constrain {
        if d.x.abs() >= d.y.abs() {
            d.y = 0.0;
            locked[1] = true;
        } else {
            d.x = 0.0;
            locked[0] = true;
        }
    }
    let mut picked: [Option<SnapLine>; 2] = [None, None];
    for axis in 0..2 {
        if locked[axis] {
            continue;
        }
        let (l0, l1, cands) = if axis == 0 {
            (lo.x, hi.x, &t.xs)
        } else {
            (lo.y, hi.y, &t.ys)
        };
        let off = if axis == 0 { d.x } else { d.y };
        let lines = [l0 + off, (l0 + l1) * 0.5 + off, l1 + off];
        let mut best: Option<(f32, SnapLine)> = None;
        for l in lines {
            for c in cands.iter() {
                let diff = c.at - l;
                if diff.abs() <= o.tolerance
                    && best
                        .as_ref()
                        .map(|(b, _)| diff.abs() < b.abs())
                        .unwrap_or(true)
                {
                    best = Some((diff, c.clone()));
                }
            }
            if let Some(g) = o.grid.filter(|g| *g > 0.0) {
                let diff = (l / g).round() * g - l;
                if diff.abs() <= o.tolerance
                    && best
                        .as_ref()
                        .map(|(b, _)| diff.abs() < b.abs())
                        .unwrap_or(true)
                {
                    best = Some((
                        diff,
                        SnapLine {
                            at: l + diff,
                            from: f32::NAN,
                            to: f32::NAN,
                            kind: "grid".into(),
                        },
                    ));
                }
            }
        }
        if let Some((diff, line)) = best {
            if axis == 0 {
                d.x += diff;
            } else {
                d.y += diff;
            }
            picked[axis] = Some(line);
        }
    }
    let mut guides = Vec::new();
    for (axis, p) in picked.iter().enumerate() {
        let Some(l) = p else { continue };
        // extent of the moving box along the other axis, after the move
        let (m0, m1) = if axis == 0 {
            (lo.y + d.y, hi.y + d.y)
        } else {
            (lo.x + d.x, hi.x + d.x)
        };
        let (from, to) = if l.from.is_nan() {
            (m0 - 20.0, m1 + 20.0)
        } else {
            (m0.min(l.from), m1.max(l.to))
        };
        guides.push(Guide {
            axis: if axis == 0 { "x".into() } else { "y".into() },
            at: l.at,
            from,
            to,
            kind: l.kind.clone(),
        });
    }
    (d, guides)
}

// ---------------------------------------------------------------------------------------------
// Drag maths
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Modifiers {
    /// Axis lock (move), 15 degree steps (rotate), proportional (scale), rotate only (bone tip).
    pub constrain: bool,
    /// Scale around the centre instead of the opposite handle.
    pub center: bool,
    /// Disable snapping.
    pub no_snap: bool,
}

#[derive(Clone, Debug)]
pub struct DragMatrix {
    pub matrix: Affine2,
    pub guides: Vec<Guide>,
    pub summary: String,
}

fn wrap_pi(a: f32) -> f32 {
    crate::rig::wrap_pi(a)
}

/// The canvas-space map for dragging `handle` from `start` by `delta`. `bx` is the selection box
/// (for bone handles pass the bone's ends in `bone` instead).
#[allow(clippy::too_many_arguments)]
pub fn drag_matrix(
    bx: Option<&BoxFrame>,
    bone: Option<(Vec2, Vec2)>,
    handle: Handle,
    start: Vec2,
    delta: Vec2,
    m: &Modifiers,
    snap: &SnapOptions,
    targets: &SnapTargets,
    pivot: Option<Vec2>,
) -> Result<DragMatrix, String> {
    let snap_on = !m.no_snap;
    match handle {
        Handle::Move | Handle::BoneHead => {
            let (lo, hi) = match (handle, bone, bx) {
                (Handle::BoneHead, Some((h, _)), _) => (h, h),
                (_, _, Some(b)) => {
                    let c = b.corners();
                    (
                        Vec2::new(
                            c.iter().map(|q| q.x).fold(f32::INFINITY, f32::min),
                            c.iter().map(|q| q.y).fold(f32::INFINITY, f32::min),
                        ),
                        Vec2::new(
                            c.iter().map(|q| q.x).fold(f32::NEG_INFINITY, f32::max),
                            c.iter().map(|q| q.y).fold(f32::NEG_INFINITY, f32::max),
                        ),
                    )
                }
                _ => return Err("nothing to move".into()),
            };
            let (d, guides) = if snap_on {
                snap_move(lo, hi, delta, m.constrain, snap, targets)
            } else if m.constrain {
                snap_move(
                    lo,
                    hi,
                    delta,
                    true,
                    &SnapOptions {
                        grid: None,
                        objects: false,
                        canvas: false,
                        tolerance: 0.0,
                    },
                    &SnapTargets::default(),
                )
            } else {
                (delta, vec![])
            };
            Ok(DragMatrix {
                matrix: Affine2::translate(d),
                guides,
                summary: format!("move {:.1}, {:.1}", d.x, d.y),
            })
        }
        Handle::Rotate => {
            let b = bx.ok_or("rotate needs a box")?;
            let pv = pivot.unwrap_or_else(|| b.centre());
            let (v0, v1) = (start - pv, start + delta - pv);
            if v0.length() < 1e-3 || v1.length() < 1e-6 {
                return Err("pointer is on the pivot".into());
            }
            let mut ang = wrap_pi(v1.y.atan2(v1.x) - v0.y.atan2(v0.x));
            if m.constrain {
                let step = 15f32.to_radians();
                ang = (ang / step).round() * step;
            }
            let mat = Affine2::translate(pv)
                .mul(Affine2::rotate(ang))
                .mul(Affine2::translate(-pv));
            Ok(DragMatrix {
                matrix: mat,
                guides: vec![],
                summary: format!("rotate {:.1} deg", ang.to_degrees()),
            })
        }
        Handle::Scale { at } => {
            let b = bx.ok_or("scale needs a box")?;
            if !b.nondegenerate() {
                return Err("selection has no extent to scale".into());
            }
            let basis = b.basis();
            let inv = basis.inverse().ok_or("degenerate box")?;
            let (hx, hy) = at.local();
            let (q0, q1) = (inv.apply(start), inv.apply(start + delta));
            let pl = if m.center {
                (0.5, 0.5)
            } else {
                (1.0 - hx, 1.0 - hy)
            };
            let pv = basis.apply(Vec2::new(pl.0, pl.1));
            let factor = |q0: f32, q1: f32, p: f32| -> Result<f32, String> {
                let den = q0 - p;
                if den.abs() < 1e-6 {
                    return Err("pointer started on the pivot".into());
                }
                Ok(clamp_factor((q1 - p) / den))
            };
            let mut sx = if hx == 0.5 {
                1.0
            } else {
                factor(q0.x, q1.x, pl.0)?
            };
            let mut sy = if hy == 0.5 {
                1.0
            } else {
                factor(q0.y, q1.y, pl.1)?
            };
            if m.constrain {
                let s = if at.is_corner() {
                    let (a, c) = (start - pv, start + delta - pv);
                    let l2 = a.dot(a);
                    if l2 < 1e-9 {
                        return Err("pointer started on the pivot".into());
                    }
                    clamp_factor(c.dot(a) / l2)
                } else if hx == 0.5 {
                    sy
                } else {
                    sx
                };
                sx = s;
                sy = s;
            }
            let pl_v = Vec2::new(pl.0, pl.1);
            let mat = basis
                .mul(Affine2::translate(pl_v))
                .mul(Affine2::scale_xy(sx, sy))
                .mul(Affine2::translate(-pl_v))
                .mul(inv);
            Ok(DragMatrix {
                matrix: mat,
                guides: vec![],
                summary: format!("scale {sx:.3} x {sy:.3}"),
            })
        }
        Handle::BoneTip => {
            let (h, t) = bone.ok_or("bone tip needs a bone")?;
            let v0 = t - h;
            let v1 = v0 + delta;
            if v0.length() < 1e-6 || v1.length() < 1e-6 {
                return Err("bone has no length".into());
            }
            let (a0, a1) = (v0.y.atan2(v0.x), v1.y.atan2(v1.x));
            let mut ang = wrap_pi(a1 - a0);
            let mut s = v1.length() / v0.length();
            if m.constrain {
                let step = 15f32.to_radians();
                ang = wrap_pi((a1 / step).round() * step - a0);
                s = 1.0;
            }
            let mat = Affine2::translate(h)
                .mul(Affine2::rotate(ang))
                .mul(Affine2::scale(s))
                .mul(Affine2::translate(-h));
            Ok(DragMatrix {
                matrix: mat,
                guides: vec![],
                summary: format!(
                    "bone tip: rotate {:.1} deg, length x{:.3}",
                    ang.to_degrees(),
                    s
                ),
            })
        }
    }
}

fn clamp_factor(s: f32) -> f32 {
    if s.abs() < 1e-3 {
        if s < 0.0 {
            -1e-3
        } else {
            1e-3
        }
    } else {
        s
    }
}

/// Split a local matrix into layer transform properties (`T(pos) R Shear S T(-anchor)`),
/// choosing the rotation closest to `prev_rot_deg` so key values stay continuous and a mirror shows
/// up as a negative scale rather than a half-turn.
pub fn decompose(m: &Affine2, anchor: Vec2, prev_rot_deg: f32) -> Result<Transform2D, String> {
    let prev = prev_rot_deg.to_radians();
    let a0 = m.b.atan2(m.a);
    let cand = if wrap_pi(a0 - prev).abs() <= wrap_pi(a0 + std::f32::consts::PI - prev).abs() {
        a0
    } else {
        a0 + std::f32::consts::PI
    };
    let rot = prev + wrap_pi(cand - prev);
    let (s, c) = rot.sin_cos();
    let sx = m.a * c + m.b * s;
    let sy = -m.c * s + m.d * c;
    let k = m.c * c + m.d * s;
    if sy.abs() < 1e-6 || sx.abs() < 1e-6 {
        return Err("transform would collapse the layer".into());
    }
    let skew = (k / sy).atan();
    let pos = Vec2::new(
        m.tx + m.a * anchor.x + m.c * anchor.y,
        m.ty + m.b * anchor.x + m.d * anchor.y,
    );
    Ok(Transform2D {
        position: pos,
        rotation: rot.to_degrees(),
        scale: Vec2::new(sx, sy),
        skew: skew.to_degrees(),
        anchor,
    })
}

// ---------------------------------------------------------------------------------------------
// Edit planning
// ---------------------------------------------------------------------------------------------

/// One property change, for reporting.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EditInfo {
    pub target: String,
    pub prop: String,
    pub from: f32,
    pub to: f32,
    /// `key` (keyframe at the current frame) or `static`.
    pub mode: String,
}

pub struct Plan {
    pub ops: Vec<Op>,
    pub edits: Vec<EditInfo>,
}

fn eps_changed(a: f32, b: f32) -> bool {
    (a - b).abs() > 1e-4
}

fn keyed_index(engine: &Engine, target: &str, prop: Prop) -> Option<usize> {
    engine.project.channels.iter().position(|c| {
        c.target == target
            && c.prop == prop
            && matches!(c.source, Source::Keys { .. })
            && !crate::skins::is_collapse_channel(c)
    })
}

fn prop_name(p: Prop) -> String {
    serde_json::to_value(p)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

/// Op that sets a key at `frame` so that the property's total becomes `new_total`.
fn key_op(
    engine: &Engine,
    target: &str,
    prop: Prop,
    frame: f32,
    old_total: f32,
    new_total: f32,
    base: f32,
) -> Result<Op, String> {
    crate::chanmeta::assert_unlocked(&engine.project, target, prop)?;
    match keyed_index(engine, target, prop) {
        Some(i) => {
            let mut ch = engine.project.channels[i].clone();
            let Source::Keys { track } = &mut ch.source else {
                unreachable!()
            };
            let cur = track.eval(frame).unwrap_or(base);
            let v = new_total - (old_total - cur);
            let ease = track
                .keys
                .iter()
                .find(|k| k.frame == frame)
                .map(|k| k.ease)
                .unwrap_or(Ease::Linear);
            track.set_key(frame, v, ease);
            Ok(Op::Set {
                path: format!("/channels/{i}"),
                value: serde_json::to_value(&ch).map_err(|e| e.to_string())?,
            })
        }
        None => {
            let ch = Channel::keys(
                target,
                prop,
                crate::anim::Track::with_keys(&[(frame, new_total, Ease::Linear)]),
            );
            Ok(Op::Insert {
                path: "/channels/-".into(),
                value: serde_json::to_value(&ch).map_err(|e| e.to_string())?,
            })
        }
    }
}

fn layer_ptr(engine: &Engine, id: usize) -> String {
    let ix = engine.node_idx_path(id);
    let mut s = format!("/layers/{}", ix[0]);
    for i in &ix[1..] {
        s.push_str(&format!("/layers/{i}"));
    }
    s
}

fn has_selected_ancestor(path: &str, all: &[&str]) -> bool {
    all.iter()
        .any(|a| *a != path && path.starts_with(&format!("{a}/")))
}

fn bone_driven(engine: &Engine, name: &str) -> Option<String> {
    let p = &engine.project;
    if let Some(c) = p.ik.iter().find(|c| c.bones.iter().any(|b| b == name)) {
        return Some(format!(
            "bone '{name}' is driven by IK chain '{}'; drag its target bone instead",
            c.name
        ));
    }
    if let Some(c) = p.constraints.iter().find(|c| c.bone == name) {
        return Some(format!(
            "bone '{name}' is driven by constraint '{}'",
            c.name
        ));
    }
    if p.springs.iter().any(|s| s.bone == name) {
        return Some(format!(
            "bone '{name}' has a spring; its rotation is simulated"
        ));
    }
    None
}

/// Turn the canvas map `d` into edits for everything in `sel` at `frame_no`.
pub fn plan(
    ed: &Editor,
    frame_no: f32,
    sel: &Selection,
    d: &Affine2,
    auto_key: bool,
    locked: &[String],
) -> Result<Plan, String> {
    let engine = ed.engine();
    let frame = engine.evaluate(frame_no);
    let mut ops: Vec<Op> = Vec::new();
    let mut edits: Vec<EditInfo> = Vec::new();

    let has_points = sel.items.iter().any(|t| matches!(t, Target::Point { .. }));
    if has_points && !sel.items.iter().all(|t| matches!(t, Target::Point { .. })) {
        return Err("control points cannot be edited together with layers or bones".into());
    }

    if has_points {
        plan_points(
            engine, &frame, frame_no, sel, d, auto_key, locked, &mut ops, &mut edits,
        )?;
    } else {
        let layers = sel.layers();
        for path in &layers {
            if has_selected_ancestor(path, &layers) {
                continue;
            }
            if locked
                .iter()
                .any(|l| l == path || path.starts_with(&format!("{l}/")))
                || engine.project.is_layer_locked(path)
            {
                return Err(format!("layer '{path}' is locked"));
            }
            plan_layer(
                engine, &frame, frame_no, path, d, auto_key, &mut ops, &mut edits,
            )?;
        }
        let bones = sel.bones();
        for name in &bones {
            // skip bones under another selected bone: they follow it
            let mut anc = engine
                .project
                .bones
                .iter()
                .find(|b| b.name == *name)
                .and_then(|b| b.parent.clone());
            let mut skip = false;
            while let Some(a) = anc {
                if bones.contains(&a.as_str()) {
                    skip = true;
                    break;
                }
                anc = engine
                    .project
                    .bones
                    .iter()
                    .find(|b| b.name == a)
                    .and_then(|b| b.parent.clone());
            }
            if skip {
                continue;
            }
            if locked.iter().any(|l| l == &format!("bone:{name}")) {
                return Err(format!("bone '{name}' is locked"));
            }
            plan_bone(engine, &frame, frame_no, name, d, &mut ops, &mut edits)?;
        }
    }
    Ok(Plan { ops, edits })
}

#[allow(clippy::too_many_arguments)]
fn plan_layer(
    engine: &Engine,
    frame: &Frame,
    frame_no: f32,
    path: &str,
    d: &Affine2,
    auto_key: bool,
    ops: &mut Vec<Op>,
    edits: &mut Vec<EditInfo>,
) -> Result<(), String> {
    let id = engine
        .node_index(path)
        .ok_or_else(|| format!("no layer '{path}'"))?;
    let layer = engine
        .project
        .find_layer(path)
        .ok_or_else(|| format!("no layer '{path}'"))?;
    let node = &frame.nodes[id];
    let old = engine.layer_local_at(frame_no, id);
    let lm = old.matrix();
    let inv_lm = lm
        .inverse()
        .ok_or_else(|| format!("layer '{path}' has a singular transform"))?;
    let parent = node.world.mul(inv_lm);
    let pinv = parent.inverse().ok_or("parent transform is singular")?;
    let new = decompose(&pinv.mul(*d).mul(parent).mul(lm), old.anchor, old.rotation)
        .map_err(|e| format!("layer '{path}': {e}"))?;
    let st = &layer.transform;
    let ptr = layer_ptr(engine, id);

    let props: [(Prop, f32, f32, f32); 6] = [
        (Prop::LayerX, old.position.x, new.position.x, st.position.x),
        (Prop::LayerY, old.position.y, new.position.y, st.position.y),
        (Prop::LayerRotation, old.rotation, new.rotation, st.rotation),
        (Prop::LayerScaleX, old.scale.x, new.scale.x, st.scale.x),
        (Prop::LayerScaleY, old.scale.y, new.scale.y, st.scale.y),
        (Prop::LayerSkew, old.skew, new.skew, st.skew),
    ];
    let mut spos = st.position;
    let mut sscale = st.scale;
    let (mut pos_static, mut scale_static) = (false, false);
    for (prop, o, n, base) in props {
        if !eps_changed(o, n) {
            continue;
        }
        let keyed = keyed_index(engine, path, prop).is_some();
        if keyed || auto_key {
            ops.push(key_op(engine, path, prop, frame_no, o, n, base)?);
            edits.push(EditInfo {
                target: path.into(),
                prop: prop_name(prop),
                from: o,
                to: n,
                mode: "key".into(),
            });
        } else {
            crate::chanmeta::assert_unlocked(&engine.project, path, prop)?;
            // static value: shift by the same amount the evaluated value moves
            let v = base + (n - o);
            match prop {
                Prop::LayerX => {
                    spos.x = v;
                    pos_static = true;
                }
                Prop::LayerY => {
                    spos.y = v;
                    pos_static = true;
                }
                Prop::LayerScaleX => {
                    sscale.x = v;
                    scale_static = true;
                }
                Prop::LayerScaleY => {
                    sscale.y = v;
                    scale_static = true;
                }
                Prop::LayerRotation => ops.push(Op::Set {
                    path: format!("{ptr}/transform/rotation"),
                    value: json!(v),
                }),
                _ => ops.push(Op::Set {
                    path: format!("{ptr}/transform/skew"),
                    value: json!(v),
                }),
            }
            edits.push(EditInfo {
                target: path.into(),
                prop: prop_name(prop),
                from: o,
                to: v,
                mode: "static".into(),
            });
        }
    }
    if pos_static {
        ops.push(Op::Set {
            path: format!("{ptr}/transform/position"),
            value: json!([spos.x, spos.y]),
        });
    }
    if scale_static {
        ops.push(Op::Set {
            path: format!("{ptr}/transform/scale"),
            value: json!([sscale.x, sscale.y]),
        });
    }
    Ok(())
}

fn plan_bone(
    engine: &Engine,
    frame: &Frame,
    frame_no: f32,
    name: &str,
    d: &Affine2,
    ops: &mut Vec<Op>,
    edits: &mut Vec<EditInfo>,
) -> Result<(), String> {
    if let Some(why) = bone_driven(engine, name) {
        return Err(why);
    }
    let bi = engine
        .bone_index(name)
        .ok_or_else(|| format!("no bone '{name}'"))?;
    let bone = &engine.project.bones[bi];
    let parent_w = bone
        .parent
        .as_ref()
        .and_then(|p| engine.bone_index(p))
        .map(|pi| frame.world[pi])
        .unwrap_or(Affine2::IDENTITY);
    let pinv = parent_w
        .inverse()
        .ok_or("parent bone transform is singular")?;
    let cam_inv = frame.camera.inverse().ok_or("camera is singular")?;
    let w = frame.world[bi];
    let (head0, tip0) = (w.apply(Vec2::ZERO), w.apply(Vec2::new(bone.length, 0.0)));
    // the map acts in canvas space; bring head / tip there and back
    let (h1, t1) = (
        cam_inv.apply(d.apply(frame.camera.apply(head0))),
        cam_inv.apply(d.apply(frame.camera.apply(tip0))),
    );

    let pscale = parent_w.x_scale().max(1e-6);
    let off_old = pinv.apply(head0) - bone.rest_offset;
    let off_new = pinv.apply(h1) - bone.rest_offset;
    let ang_old = (w.rotation() - parent_w.rotation()).to_degrees() - bone.rest_angle;
    let dir = t1 - h1;
    let ang_target = (dir.y.atan2(dir.x) - parent_w.rotation()).to_degrees() - bone.rest_angle;
    // keep key values continuous: pick the representative nearest to the current value
    let ang_new = ang_old + wrap_pi((ang_target - ang_old).to_radians()).to_degrees();
    let sc_old = w.x_scale() / pscale;
    let sc_new = if bone.length > 1e-6 {
        dir.length() / (bone.length * pscale)
    } else {
        sc_old
    };

    let items: [(Prop, f32, f32, f32); 4] = [
        (Prop::BoneOffsetX, off_old.x, off_new.x, 0.0),
        (Prop::BoneOffsetY, off_old.y, off_new.y, 0.0),
        (Prop::BoneAngle, ang_old, ang_new, 0.0),
        (Prop::BoneScale, sc_old, sc_new, 1.0),
    ];
    for (prop, o, n, base) in items {
        let tol = if prop == Prop::BoneScale {
            1e-3 * o.abs().max(1.0)
        } else {
            1e-3
        };
        if (o - n).abs() <= tol {
            continue;
        }
        ops.push(key_op(engine, name, prop, frame_no, o, n, base)?);
        edits.push(EditInfo {
            target: name.into(),
            prop: prop_name(prop),
            from: o,
            to: n,
            mode: "key".into(),
        });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn plan_points(
    engine: &Engine,
    frame: &Frame,
    frame_no: f32,
    sel: &Selection,
    d: &Affine2,
    auto_key: bool,
    locked: &[String],
    ops: &mut Vec<Op>,
    edits: &mut Vec<EditInfo>,
) -> Result<(), String> {
    for t in &sel.items {
        let Target::Point { layer: lp, point } = t else {
            continue;
        };
        if locked
            .iter()
            .any(|l| lp == l || lp.starts_with(&format!("{l}/")))
            || engine.project.is_layer_locked(lp)
        {
            return Err(format!("layer '{lp}' is locked"));
        }
        let id = engine
            .node_index(lp)
            .ok_or_else(|| format!("no layer '{lp}'"))?;
        let layer = engine
            .project
            .find_layer(lp)
            .ok_or_else(|| format!("no layer '{lp}'"))?;
        let node = &frame.nodes[id];
        let winv = node.world.inverse().ok_or("layer transform is singular")?;
        let cur = pick::layer_points(engine, frame, lp)
            .into_iter()
            .find(|(k, _)| k == point)
            .map(|(_, p)| p)
            .ok_or_else(|| format!("control point not found on '{lp}'"))?;
        let delta = winv.apply(d.apply(cur)) - winv.apply(cur);
        if delta.length() < 1e-5 {
            continue;
        }
        let ptr = layer_ptr(engine, id);
        let vec_json = |v: Vec2| json!([v.x, v.y]);

        // keyed offset helper: channels `Dx`/`Dy` on "layer::name::sub"
        let offset_edit = |ops: &mut Vec<Op>,
                           edits: &mut Vec<EditInfo>,
                           name: &str,
                           dx: Prop,
                           dy: Prop,
                           stat: Vec2,
                           static_ops: &mut dyn FnMut() -> Vec<Op>|
         -> Result<(), String> {
            let tgt = format!("{lp}::{name}");
            let key_x = keyed_index(engine, &tgt, dx);
            let key_y = keyed_index(engine, &tgt, dy);
            let eval = |ix: Option<usize>, fallback: f32| -> f32 {
                ix.and_then(|i| match &engine.project.channels[i].source {
                    Source::Keys { track } => track.eval(frame_no),
                    _ => None,
                })
                .unwrap_or(fallback)
            };
            let (ox, oy) = (eval(key_x, stat.x), eval(key_y, stat.y));
            if key_x.is_some() || key_y.is_some() || auto_key {
                for (prop, o, delta_c) in [(dx, ox, delta.x), (dy, oy, delta.y)] {
                    if delta_c.abs() < 1e-6 {
                        continue;
                    }
                    ops.push(key_op(engine, &tgt, prop, frame_no, o, o + delta_c, o)?);
                    edits.push(EditInfo {
                        target: tgt.clone(),
                        prop: prop_name(prop),
                        from: o,
                        to: o + delta_c,
                        mode: "key".into(),
                    });
                }
            } else {
                ops.extend(static_ops());
                edits.push(EditInfo {
                    target: tgt,
                    prop: "static_offset".into(),
                    from: 0.0,
                    to: delta.length(),
                    mode: "static".into(),
                });
            }
            Ok(())
        };

        match point {
            PointKind::Anchor { path, index } => {
                let LayerKind::Vector { paths } = &layer.kind else {
                    return Err("not a vector layer".into());
                };
                let p = paths.get(*path).ok_or("no such path")?;
                let a = p.anchors.get(*index).ok_or("no such anchor")?;
                let pname = if p.name.is_empty() {
                    format!("path{path}")
                } else {
                    p.name.clone()
                };
                let tgt = format!("{lp}::{pname}::{index}");
                let (kx, ky) = (
                    keyed_index(engine, &tgt, Prop::PointDx),
                    keyed_index(engine, &tgt, Prop::PointDy),
                );
                if kx.is_some() || ky.is_some() || auto_key {
                    let ev = |ix: Option<usize>| {
                        ix.and_then(|i| match &engine.project.channels[i].source {
                            Source::Keys { track } => track.eval(frame_no),
                            _ => None,
                        })
                    };
                    for (prop, o, dc) in [
                        (Prop::PointDx, ev(kx).unwrap_or(0.0), delta.x),
                        (Prop::PointDy, ev(ky).unwrap_or(0.0), delta.y),
                    ] {
                        if dc.abs() < 1e-6 {
                            continue;
                        }
                        ops.push(key_op(engine, &tgt, prop, frame_no, o, o + dc, 0.0)?);
                        edits.push(EditInfo {
                            target: tgt.clone(),
                            prop: prop_name(prop),
                            from: o,
                            to: o + dc,
                            mode: "key".into(),
                        });
                    }
                } else {
                    let base = format!("{ptr}/paths/{path}/anchors/{index}");
                    ops.push(Op::Set {
                        path: format!("{base}/pos"),
                        value: vec_json(a.pos + delta),
                    });
                    if let Some(h) = a.handle_in {
                        ops.push(Op::Set {
                            path: format!("{base}/handle_in"),
                            value: vec_json(h + delta),
                        });
                    }
                    if let Some(h) = a.handle_out {
                        ops.push(Op::Set {
                            path: format!("{base}/handle_out"),
                            value: vec_json(h + delta),
                        });
                    }
                    edits.push(EditInfo {
                        target: tgt,
                        prop: "anchor".into(),
                        from: 0.0,
                        to: delta.length(),
                        mode: "static".into(),
                    });
                }
            }
            PointKind::HandleIn { path, index } | PointKind::HandleOut { path, index } => {
                let LayerKind::Vector { paths } = &layer.kind else {
                    return Err("not a vector layer".into());
                };
                let a = paths
                    .get(*path)
                    .and_then(|p| p.anchors.get(*index))
                    .ok_or("no such anchor")?;
                let (key, h) = if matches!(point, PointKind::HandleIn { .. }) {
                    ("handle_in", a.handle_in)
                } else {
                    ("handle_out", a.handle_out)
                };
                let h = h.ok_or("handle is not set")?;
                ops.push(Op::Set {
                    path: format!("{ptr}/paths/{path}/anchors/{index}/{key}"),
                    value: vec_json(h + delta),
                });
                edits.push(EditInfo {
                    target: format!("{lp}::path{path}::{index}"),
                    prop: key.into(),
                    from: 0.0,
                    to: delta.length(),
                    mode: "static".into(),
                });
            }
            PointKind::MeshVertex { index } => {
                let LayerKind::Image { mesh: Some(m), .. } = &layer.kind else {
                    return Err("not a mesh layer".into());
                };
                let f = m
                    .free
                    .as_ref()
                    .ok_or("only free-form meshes have editable vertices")?;
                let v = *f.vertices.get(*index).ok_or("no such vertex")?;
                ops.push(Op::Set {
                    path: format!("{ptr}/mesh/free/vertices/{index}"),
                    value: vec_json(v + delta),
                });
                edits.push(EditInfo {
                    target: format!("{lp}::mesh::{index}"),
                    prop: "vertex".into(),
                    from: 0.0,
                    to: delta.length(),
                    mode: "static".into(),
                });
            }
            PointKind::Cage { deformer, index }
            | PointKind::Pin { deformer, index }
            | PointKind::Spine { deformer, index }
            | PointKind::Lattice { deformer, index } => {
                let def = layer.deformers.get(*deformer).ok_or("no such deformer")?;
                let (name, offsets, n, dxp, dyp, sub) = match (def, point) {
                    (
                        Deformer::Cage {
                            name,
                            offsets,
                            rest,
                        },
                        PointKind::Cage { .. },
                    ) => (
                        name.clone(),
                        offsets.clone(),
                        rest.len(),
                        Prop::CageDx,
                        Prop::CageDy,
                        index.to_string(),
                    ),
                    (
                        Deformer::Pin {
                            name,
                            offsets,
                            pins,
                            ..
                        },
                        PointKind::Pin { .. },
                    ) => (
                        name.clone(),
                        offsets.clone(),
                        pins.len(),
                        Prop::PinDx,
                        Prop::PinDy,
                        index.to_string(),
                    ),
                    (
                        Deformer::Spine {
                            name,
                            offsets,
                            rest,
                            ..
                        },
                        PointKind::Spine { .. },
                    ) => (
                        name.clone(),
                        offsets.clone(),
                        rest.len(),
                        Prop::SpineDx,
                        Prop::SpineDy,
                        index.to_string(),
                    ),
                    (
                        Deformer::Lattice {
                            name,
                            offsets,
                            cols,
                            rows,
                            ..
                        },
                        PointKind::Lattice { .. },
                    ) => (
                        name.clone(),
                        offsets.clone(),
                        (cols + 1) * (rows + 1),
                        Prop::LatticeDx,
                        Prop::LatticeDy,
                        format!("{},{}", index % (cols + 1), index / (cols + 1)),
                    ),
                    _ => return Err("point kind does not match the deformer".into()),
                };
                if *index >= n {
                    return Err("no such deformer point".into());
                }
                let stat = offsets.get(*index).copied().unwrap_or(Vec2::ZERO);
                let off_ptr = format!("{ptr}/deformers/{deformer}/offsets");
                let mut static_ops = || -> Vec<Op> {
                    let mut all = if offsets.len() == n {
                        offsets.clone()
                    } else {
                        vec![Vec2::ZERO; n]
                    };
                    all[*index] += delta;
                    vec![Op::Set {
                        path: off_ptr.clone(),
                        value: json!(all.iter().map(|v| [v.x, v.y]).collect::<Vec<_>>()),
                    }]
                };
                offset_edit(
                    ops,
                    edits,
                    &format!("{name}::{sub}"),
                    dxp,
                    dyp,
                    stat,
                    &mut static_ops,
                )?;
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------------------------

/// One op batch that removes the selected layers (and the channels that target them).
pub fn delete_op(ed: &Editor, sel: &Selection) -> Result<Op, String> {
    let engine = ed.engine();
    if sel.items.iter().any(|t| !matches!(t, Target::Layer { .. })) {
        return Err("only layers can be deleted from the canvas".into());
    }
    let layers = sel.layers();
    if let Some(l) = layers.iter().find(|l| engine.project.is_layer_locked(l)) {
        return Err(format!("layer '{l}' is locked"));
    }
    let mut victims: Vec<(Vec<usize>, String)> = Vec::new();
    for p in &layers {
        if has_selected_ancestor(p, &layers) {
            continue;
        }
        let id = engine
            .node_index(p)
            .ok_or_else(|| format!("no layer '{p}'"))?;
        victims.push((engine.node_idx_path(id).to_vec(), p.to_string()));
    }
    if victims.is_empty() {
        return Err("nothing selected".into());
    }
    let mut ops: Vec<Op> = Vec::new();
    // channels first (they sit at the end of the document order, indices independent of layers)
    let mut chan_ix: Vec<usize> = engine
        .project
        .channels
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            victims.iter().any(|(_, p)| {
                c.target == *p
                    || c.target.starts_with(&format!("{p}/"))
                    || c.target.starts_with(&format!("{p}::"))
            })
        })
        .map(|(i, _)| i)
        .collect();
    chan_ix.sort_unstable_by(|a, b| b.cmp(a));
    for i in chan_ix {
        ops.push(Op::Remove {
            path: format!("/channels/{i}"),
        });
    }
    // then layers, last index first so earlier pointers stay valid
    victims.sort_by(|a, b| b.0.cmp(&a.0));
    for (ix, _) in victims {
        let mut s = format!("/layers/{}", ix[0]);
        for i in &ix[1..] {
            s.push_str(&format!("/layers/{i}"));
        }
        ops.push(Op::Remove { path: s });
    }
    Ok(Op::Batch { ops })
}

// ---------------------------------------------------------------------------------------------
// Request / preview / commit
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize)]
pub struct DragRequest {
    pub frame: f32,
    pub selection: Vec<Target>,
    pub handle: Handle,
    pub start: Vec2,
    pub delta: Vec2,
    #[serde(default)]
    pub modifiers: Modifiers,
    #[serde(default)]
    pub snap: SnapOptions,
    #[serde(default)]
    pub auto_key: bool,
    /// Locked layer paths and `bone:<name>` entries.
    #[serde(default)]
    pub locked: Vec<String>,
    #[serde(default)]
    pub pivot: Option<Vec2>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Outline {
    pub pts: Vec<Vec2>,
    pub closed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct DragPreview {
    /// `[a, b, c, d, tx, ty]`.
    pub matrix: [f32; 6],
    pub summary: String,
    pub guides: Vec<Guide>,
    /// Selected geometry after the map, for a ghost overlay.
    pub outline: Vec<Outline>,
    pub box_after: Option<[Vec2; 4]>,
    pub edits: Vec<EditInfo>,
    pub changed: bool,
}

struct Prepared {
    d: DragMatrix,
    plan: Plan,
    sel: Selection,
    outline: Vec<Outline>,
    box_after: Option<[Vec2; 4]>,
}

fn prepare(ed: &Editor, req: &DragRequest) -> Result<Prepared, String> {
    if !req.frame.is_finite()
        || !req.start.x.is_finite()
        || !req.start.y.is_finite()
        || !req.delta.x.is_finite()
        || !req.delta.y.is_finite()
    {
        return Err("non-finite number in request".into());
    }
    let engine = ed.engine();
    let frame = engine.evaluate(req.frame);
    let mut sel = Selection::from_items(req.selection.clone());
    sel.prune(engine);
    if sel.is_empty() {
        return Err("nothing selected".into());
    }
    let points_only = sel.items.iter().all(|t| matches!(t, Target::Point { .. }));
    let bone = match (&req.handle, sel.items.as_slice()) {
        (Handle::BoneHead | Handle::BoneTip, [Target::Bone { name }]) => {
            Some(bone_ends(engine, &frame, name).ok_or("bone not found")?)
        }
        (Handle::BoneHead | Handle::BoneTip, _) => {
            return Err("bone handles need exactly one selected bone".into())
        }
        _ => None,
    };
    let bx = if points_only || bone.is_some() {
        None
    } else {
        box_frame(engine, &frame, &sel, MIN_EXTENT)
    };
    if points_only && !matches!(req.handle, Handle::Move) {
        return Err("control points can only be moved".into());
    }
    if bx.is_none() && bone.is_none() && !points_only {
        return Err("selection has no geometry".into());
    }
    let targets = if req.modifiers.no_snap {
        SnapTargets::default()
    } else {
        snap_targets(engine, &frame, &sel, &req.snap)
    };
    // points move like a degenerate box around their own position
    let pt_box;
    let bx_ref = if points_only {
        let pts: Vec<Vec2> = sel
            .items
            .iter()
            .flat_map(|t| pick::target_points(engine, &frame, t))
            .collect();
        let lo = Vec2::new(
            pts.iter().map(|q| q.x).fold(f32::INFINITY, f32::min),
            pts.iter().map(|q| q.y).fold(f32::INFINITY, f32::min),
        );
        let hi = Vec2::new(
            pts.iter().map(|q| q.x).fold(f32::NEG_INFINITY, f32::max),
            pts.iter().map(|q| q.y).fold(f32::NEG_INFINITY, f32::max),
        );
        pt_box = BoxFrame::from_aabb(lo, hi);
        Some(&pt_box)
    } else {
        bx.as_ref()
    };
    let d = drag_matrix(
        bx_ref,
        bone,
        req.handle,
        req.start,
        req.delta,
        &req.modifiers,
        &req.snap,
        &targets,
        req.pivot,
    )?;
    let plan = plan(ed, req.frame, &sel, &d.matrix, req.auto_key, &req.locked)?;

    let mut outline = Vec::new();
    for t in &sel.items {
        match t {
            Target::Layer { path } => {
                let prefix = format!("{path}/");
                for (id, n) in frame.nodes.iter().enumerate() {
                    if &n.name == path || n.name.starts_with(&prefix) {
                        for s in pick::node_shapes(&frame, id) {
                            outline.push(Outline {
                                pts: s.pts.iter().map(|q| d.matrix.apply(*q)).collect(),
                                closed: s.closed,
                            });
                        }
                    }
                }
            }
            Target::Bone { name } => {
                if let Some((h, t)) = bone_ends(engine, &frame, name) {
                    outline.push(Outline {
                        pts: vec![d.matrix.apply(h), d.matrix.apply(t)],
                        closed: false,
                    });
                }
            }
            Target::Point { .. } => {}
        }
    }
    let box_after = bx.map(|b| {
        let c = b.corners();
        [
            d.matrix.apply(c[0]),
            d.matrix.apply(c[1]),
            d.matrix.apply(c[2]),
            d.matrix.apply(c[3]),
        ]
    });
    Ok(Prepared {
        d,
        plan,
        sel,
        outline,
        box_after,
    })
}

fn preview_of(p: &Prepared) -> DragPreview {
    let m = p.d.matrix;
    DragPreview {
        matrix: [m.a, m.b, m.c, m.d, m.tx, m.ty],
        summary: p.d.summary.clone(),
        guides: p.d.guides.clone(),
        outline: p.outline.clone(),
        box_after: p.box_after,
        edits: p.plan.edits.clone(),
        changed: !p.plan.ops.is_empty(),
    }
}

/// What the drag would do, without touching the project.
pub fn preview(ed: &Editor, req: &DragRequest) -> Result<DragPreview, String> {
    Ok(preview_of(&prepare(ed, req)?))
}

/// Apply the drag as one undoable batch. A drag that changes nothing leaves no history entry.
pub fn commit(ed: &mut Editor, req: &DragRequest) -> Result<DragPreview, Vec<String>> {
    let p = prepare(ed, req).map_err(|e| vec![e])?;
    let out = preview_of(&p);
    if !p.plan.ops.is_empty() {
        ed.apply(Op::Batch { ops: p.plan.ops })?;
    }
    let _ = p.sel;
    Ok(out)
}

/// Nudge the selection by `(dx, dy)` canvas units as one undoable step (no snapping).
pub fn nudge(
    ed: &mut Editor,
    frame: f32,
    selection: &[Target],
    delta: Vec2,
    auto_key: bool,
    locked: &[String],
) -> Result<DragPreview, Vec<String>> {
    let engine = ed.engine();
    let mut sel = Selection::from_items(selection.to_vec());
    sel.prune(engine);
    if sel.is_empty() {
        return Err(vec!["nothing selected".into()]);
    }
    let d = Affine2::translate(delta);
    let plan = plan(ed, frame, &sel, &d, auto_key, locked).map_err(|e| vec![e])?;
    let out = DragPreview {
        matrix: [1.0, 0.0, 0.0, 1.0, delta.x, delta.y],
        summary: format!("nudge {:.1}, {:.1}", delta.x, delta.y),
        guides: vec![],
        outline: vec![],
        box_after: None,
        edits: plan.edits.clone(),
        changed: !plan.ops.is_empty(),
    };
    if !plan.ops.is_empty() {
        ed.apply(Op::Batch { ops: plan.ops })?;
    }
    Ok(out)
}

/// Delete the selected layers as one undoable step.
pub fn delete(ed: &mut Editor, selection: &[Target]) -> Result<(), Vec<String>> {
    let mut sel = Selection::from_items(selection.to_vec());
    sel.prune(ed.engine());
    let op = delete_op(ed, &sel).map_err(|e| vec![e])?;
    ed.apply(op)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::Track;
    use crate::path::{Anchor, VPath};
    use crate::project::{Bone, IkChain, Layer, Project};

    fn v(x: f32, y: f32) -> Vec2 {
        Vec2::new(x, y)
    }
    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }
    fn rect(name: &str, x: f32, y: f32, w: f32, h: f32) -> Layer {
        Layer::vector(name, vec![VPath::rect("r", x, y, w, h).with_fill("#c33")])
    }
    fn editor(layers: Vec<Layer>) -> Editor {
        let mut p = Project::new("t", 400.0, 300.0, 24.0, 11);
        p.layers = layers;
        Editor::new(p).unwrap()
    }
    fn req(sel: &[Target], handle: Handle, start: Vec2, delta: Vec2) -> DragRequest {
        DragRequest {
            frame: 0.0,
            selection: sel.to_vec(),
            handle,
            start,
            delta,
            modifiers: Modifiers {
                no_snap: true,
                ..Default::default()
            },
            snap: SnapOptions::default(),
            auto_key: false,
            locked: vec![],
            pivot: None,
        }
    }
    fn l(name: &str) -> Target {
        Target::layer(name)
    }
    fn pts(ed: &Editor, t: &Target) -> Vec<Vec2> {
        pick::target_points(ed.engine(), &ed.engine().evaluate(0.0), t)
    }
    /// After a commit the selection's geometry must equal `d` applied to its old geometry.
    fn assert_moved_by(before: &[Vec2], after: &[Vec2], d: &Affine2) {
        assert_eq!(before.len(), after.len());
        for (b, a) in before.iter().zip(after) {
            assert!(
                d.apply(*b).distance(*a) < 5e-3,
                "{:?} -> {:?}, expected {:?}",
                b,
                a,
                d.apply(*b)
            );
        }
    }

    #[test]
    fn translate_commit_numbers_and_single_undo() {
        let mut ed = editor(vec![rect("a", 0.0, 0.0, 100.0, 50.0)]);
        ed.apply(Op::Set {
            path: "/layers/0/transform/position".into(),
            value: json!([50.0, 50.0]),
        })
        .unwrap();
        let before = ed.doc().clone();
        let r = commit(
            &mut ed,
            &req(&[l("a")], Handle::Move, v(60.0, 60.0), v(10.0, 5.0)),
        )
        .unwrap();
        assert!(
            r.changed && r.summary.contains("move 10.0, 5.0"),
            "{}",
            r.summary
        );
        let p = &ed.project().layers[0].transform.position;
        assert!(near(p.x, 60.0) && near(p.y, 55.0), "{p:?}");
        assert!(ed.undo());
        assert_eq!(ed.doc(), &before, "undo restores the exact document");
        assert!(!ed.can_undo() || ed.doc() == &before);
        // one history entry per drag: undo once more must have nothing of ours to undo
        let mut ed2 = editor(vec![rect("a", 0.0, 0.0, 10.0, 10.0)]);
        commit(
            &mut ed2,
            &req(&[l("a")], Handle::Move, v(5.0, 5.0), v(3.0, 4.0)),
        )
        .unwrap();
        assert!(ed2.undo() && !ed2.can_undo());
    }

    #[test]
    fn axis_constraint_picks_the_dominant_axis() {
        let mut ed = editor(vec![rect("a", 0.0, 0.0, 10.0, 10.0)]);
        let mut r = req(&[l("a")], Handle::Move, v(5.0, 5.0), v(30.0, 7.0));
        r.modifiers.constrain = true;
        commit(&mut ed, &r).unwrap();
        let p = ed.project().layers[0].transform.position;
        assert!(near(p.x, 30.0) && near(p.y, 0.0), "{p:?}");
        let mut r = req(&[l("a")], Handle::Move, v(5.0, 5.0), v(2.0, -40.0));
        r.modifiers.constrain = true;
        commit(&mut ed, &r).unwrap();
        let p = ed.project().layers[0].transform.position;
        assert!(near(p.x, 30.0) && near(p.y, -40.0), "{p:?}");
    }

    #[test]
    fn snapping_to_object_bounds_canvas_and_grid_with_tolerance() {
        // a: 0..40 square; b: wide target whose left edge is at x = 200
        let ed = editor(vec![
            rect("a", 0.0, 0.0, 40.0, 40.0),
            rect("b", 200.0, 100.0, 80.0, 80.0),
        ]);
        let mut r = req(&[l("a")], Handle::Move, v(20.0, 20.0), v(157.0, 0.0));
        r.modifiers.no_snap = false;
        // right edge of a would be at 197 -> snaps to b's left edge 200 (diff 3 <= 6)
        let p = preview(&ed, &r).unwrap();
        assert!(near(p.matrix[4], 160.0), "{:?}", p.matrix);
        assert!(
            p.guides
                .iter()
                .any(|g| g.axis == "x" && near(g.at, 200.0) && g.kind == "object"),
            "{:?}",
            p.guides
        );
        // out of tolerance: no snap
        let r2 = DragRequest {
            delta: v(150.0, 0.0),
            ..r.clone()
        };
        let p2 = preview(&ed, &r2).unwrap();
        assert!(
            near(p2.matrix[4], 150.0) && p2.guides.iter().all(|g| g.axis != "x"),
            "{:?} {:?}",
            p2.matrix,
            p2.guides
        );
        // canvas edge: right edge at 400 (canvas width) within tolerance
        let r3 = DragRequest {
            delta: v(357.0, 0.0),
            ..r.clone()
        };
        let p3 = preview(&ed, &r3).unwrap();
        assert!(
            near(p3.matrix[4], 360.0) && p3.guides.iter().any(|g| g.kind == "canvas"),
            "{:?}",
            p3
        );
        // grid only
        let mut r4 = r.clone();
        r4.snap = SnapOptions {
            grid: Some(25.0),
            objects: false,
            canvas: false,
            tolerance: 6.0,
        };
        r4.delta = v(21.0, 0.0);
        let p4 = preview(&ed, &r4).unwrap();
        assert!(
            near(p4.matrix[4], 25.0),
            "left edge 21 snaps to grid 25: {:?}",
            p4.matrix
        );
        assert!(p4.guides.iter().any(|g| g.kind == "grid"));
        // modifier turns snapping off
        let mut r5 = r.clone();
        r5.modifiers.no_snap = true;
        assert!(near(preview(&ed, &r5).unwrap().matrix[4], 157.0));
    }

    #[test]
    fn rotate_around_centre_numbers_and_angle_snap() {
        let mut ed = editor(vec![rect("a", 0.0, 0.0, 100.0, 100.0)]);
        // grab the right-middle of the centre (50,50), swing it to straight down: +90 degrees
        let r = req(&[l("a")], Handle::Rotate, v(100.0, 50.0), v(-50.0, 50.0));
        let pr = preview(&ed, &r).unwrap();
        assert!(pr.summary.contains("90.0"), "{}", pr.summary);
        let before = pts(&ed, &l("a"));
        commit(&mut ed, &r).unwrap();
        let t = &ed.project().layers[0].transform;
        assert!(near(t.rotation, 90.0), "{}", t.rotation);
        assert!(
            near(t.position.x, 100.0) && near(t.position.y, 0.0),
            "centre stays put: {:?}",
            t.position
        );
        let after = pts(&ed, &l("a"));
        assert_moved_by(
            &before,
            &after,
            &Affine2::translate(v(50.0, 50.0))
                .mul(Affine2::rotate(std::f32::consts::FRAC_PI_2))
                .mul(Affine2::translate(v(-50.0, -50.0))),
        );
        // 15 degree snap: a 40 degree swing becomes 45
        let ed2 = editor(vec![rect("a", 0.0, 0.0, 100.0, 100.0)]);
        let a = 40f32.to_radians();
        let mut r2 = req(
            &[l("a")],
            Handle::Rotate,
            v(100.0, 50.0),
            v(50.0 * a.cos() - 50.0, 50.0 * a.sin()),
        );
        r2.modifiers.constrain = true;
        assert!(preview(&ed2, &r2).unwrap().summary.contains("45.0"));
        // pointer exactly on the pivot is rejected
        assert!(preview(
            &ed2,
            &req(&[l("a")], Handle::Rotate, v(50.0, 50.0), v(1.0, 1.0))
        )
        .is_err());
    }

    #[test]
    fn scale_corner_edge_centre_uniform_and_flip() {
        let se = Handle::Scale {
            at: ScaleHandle::Se,
        };
        let mut ed = editor(vec![rect("a", 0.0, 0.0, 100.0, 100.0)]);
        // corner, pivot at the opposite corner: doubles
        commit(
            &mut ed,
            &req(&[l("a")], se, v(100.0, 100.0), v(100.0, 100.0)),
        )
        .unwrap();
        let t = ed.project().layers[0].transform.clone();
        assert!(
            near(t.scale.x, 2.0) && near(t.scale.y, 2.0) && near(t.position.x, 0.0),
            "{t:?}"
        );
        ed.undo();
        // around the centre (alt)
        let mut r = req(&[l("a")], se, v(100.0, 100.0), v(50.0, 50.0));
        r.modifiers.center = true;
        commit(&mut ed, &r).unwrap();
        let t = ed.project().layers[0].transform.clone();
        assert!(
            near(t.scale.x, 2.0) && near(t.position.x, -50.0) && near(t.position.y, -50.0),
            "{t:?}"
        );
        ed.undo();
        // east edge scales x only
        commit(
            &mut ed,
            &req(
                &[l("a")],
                Handle::Scale { at: ScaleHandle::E },
                v(100.0, 50.0),
                v(50.0, 20.0),
            ),
        )
        .unwrap();
        let t = ed.project().layers[0].transform.clone();
        assert!(near(t.scale.x, 1.5) && near(t.scale.y, 1.0), "{t:?}");
        ed.undo();
        // edge + constrain = proportional about the opposite edge's centre
        let mut r = req(
            &[l("a")],
            Handle::Scale { at: ScaleHandle::E },
            v(100.0, 50.0),
            v(100.0, 0.0),
        );
        r.modifiers.constrain = true;
        commit(&mut ed, &r).unwrap();
        let t = ed.project().layers[0].transform.clone();
        assert!(
            near(t.scale.x, 2.0) && near(t.scale.y, 2.0) && near(t.position.y, -50.0),
            "{t:?}"
        );
        ed.undo();
        // corner + constrain keeps proportions from the diagonal projection
        let mut r = req(&[l("a")], se, v(100.0, 100.0), v(100.0, 0.0));
        r.modifiers.constrain = true;
        commit(&mut ed, &r).unwrap();
        let t = ed.project().layers[0].transform.clone();
        assert!(near(t.scale.x, t.scale.y) && t.scale.x > 1.0, "{t:?}");
        ed.undo();
        // dragging the east edge across the west edge flips x
        let before = pts(&ed, &l("a"));
        let r = req(
            &[l("a")],
            Handle::Scale { at: ScaleHandle::E },
            v(100.0, 50.0),
            v(-200.0, 0.0),
        );
        let d = drag_matrix(
            box_frame(
                ed.engine(),
                &ed.engine().evaluate(0.0),
                &Selection::from_items(vec![l("a")]),
                1.0,
            )
            .as_ref(),
            None,
            r.handle,
            r.start,
            r.delta,
            &r.modifiers,
            &r.snap,
            &SnapTargets::default(),
            None,
        )
        .unwrap();
        commit(&mut ed, &r).unwrap();
        let t = ed.project().layers[0].transform.clone();
        assert!(
            near(t.scale.x, -1.0) && near(t.scale.y, 1.0) && near(t.rotation, 0.0),
            "flip shows as a negative scale: {t:?}"
        );
        assert_moved_by(&before, &pts(&ed, &l("a")), &d.matrix);
        // the west edge stayed where it was
        assert!(near(t.position.x, 0.0));
        // a handle that starts on its pivot is an error, not a NaN
        let fresh = editor(vec![rect("a", 0.0, 0.0, 100.0, 100.0)]);
        let bad = req(
            &[l("a")],
            Handle::Scale {
                at: ScaleHandle::Nw,
            },
            v(100.0, 100.0),
            v(1.0, 1.0),
        );
        assert!(preview(&fresh, &bad).is_err());
    }

    #[test]
    fn scale_inside_a_rotated_group_follows_the_layer_axes() {
        let mut g = Layer::group("g", vec![rect("k", 0.0, 0.0, 100.0, 40.0)]);
        g.transform.rotation = 90.0;
        g.transform.position = v(200.0, 50.0);
        let mut ed = editor(vec![g]);
        let f = ed.engine().evaluate(0.0);
        let bx = box_frame(ed.engine(), &f, &Selection::from_items(vec![l("g/k")]), 1.0).unwrap();
        // east handle of the oriented box sits at the right end of the rotated layer
        let e = bx.point(1.0, 0.5);
        assert!(e.distance(v(180.0, 150.0)) < 1e-3, "{e:?}");
        let before = pts(&ed, &l("g/k"));
        let r = req(
            &[l("g/k")],
            Handle::Scale { at: ScaleHandle::E },
            e,
            v(0.0, 100.0),
        );
        let dm = preview(&ed, &r).unwrap().matrix;
        commit(&mut ed, &r).unwrap();
        let t = &ed.project().layers[0].layers_ref()[0].transform;
        assert!(near(t.scale.x, 2.0) && near(t.scale.y, 1.0), "{t:?} {dm:?}");
        let d = Affine2 {
            a: dm[0],
            b: dm[1],
            c: dm[2],
            d: dm[3],
            tx: dm[4],
            ty: dm[5],
        };
        assert_moved_by(&before, &pts(&ed, &l("g/k")), &d);
    }

    trait LayersRef {
        fn layers_ref(&self) -> &[Layer];
    }
    impl LayersRef for Layer {
        fn layers_ref(&self) -> &[Layer] {
            self.children()
        }
    }

    #[test]
    fn move_in_nested_rotated_scaled_parent_moves_by_exactly_the_pointer_delta() {
        let mut inner = rect("k", 0.0, 0.0, 30.0, 30.0);
        inner.transform.rotation = 20.0;
        inner.transform.skew = 10.0;
        let mut g = Layer::group("g", vec![inner]);
        g.transform.rotation = 37.0;
        g.transform.scale = v(2.0, 0.5);
        g.transform.position = v(150.0, 120.0);
        let mut ed = editor(vec![g]);
        let before = pts(&ed, &l("g/k"));
        commit(
            &mut ed,
            &req(&[l("g/k")], Handle::Move, v(0.0, 0.0), v(13.0, -7.0)),
        )
        .unwrap();
        assert_moved_by(
            &before,
            &pts(&ed, &l("g/k")),
            &Affine2::translate(v(13.0, -7.0)),
        );
        // rotation of the same layer about the canvas pivot: rigid in canvas space
        let before = pts(&ed, &l("g/k"));
        let r = req(&[l("g/k")], Handle::Rotate, v(300.0, 0.0), v(0.0, 100.0));
        let dm = preview(&ed, &r).unwrap().matrix;
        commit(&mut ed, &r).unwrap();
        let d = Affine2 {
            a: dm[0],
            b: dm[1],
            c: dm[2],
            d: dm[3],
            tx: dm[4],
            ty: dm[5],
        };
        assert_moved_by(&before, &pts(&ed, &l("g/k")), &d);
    }

    #[test]
    fn group_and_child_both_selected_moves_once() {
        let g = Layer::group("g", vec![rect("k", 0.0, 0.0, 10.0, 10.0)]);
        let mut ed = editor(vec![g]);
        let before = pts(&ed, &l("g/k"));
        commit(
            &mut ed,
            &req(&[l("g"), l("g/k")], Handle::Move, v(0.0, 0.0), v(20.0, 0.0)),
        )
        .unwrap();
        assert_moved_by(
            &before,
            &pts(&ed, &l("g/k")),
            &Affine2::translate(v(20.0, 0.0)),
        );
    }

    #[test]
    fn auto_key_behaviour() {
        let mut p = Project::new("t", 400.0, 300.0, 24.0, 11);
        p.layers = vec![
            rect("a", 0.0, 0.0, 10.0, 10.0),
            rect("b", 0.0, 0.0, 10.0, 10.0),
        ];
        p.channels = vec![Channel::keys(
            "a",
            Prop::LayerX,
            Track::with_keys(&[(0.0, 0.0, Ease::Linear), (10.0, 100.0, Ease::Linear)]),
        )];
        let mut ed = Editor::new(p).unwrap();
        // animated property: keyed at the current frame even with auto-key off; other props stay static
        let mut r = req(&[l("a")], Handle::Move, v(5.0, 5.0), v(10.0, 3.0));
        r.frame = 5.0;
        let out = commit(&mut ed, &r).unwrap();
        let ch = &ed.project().channels[0];
        let Source::Keys { track } = &ch.source else {
            panic!()
        };
        assert_eq!(track.keys.len(), 3);
        assert!(near(track.eval(5.0).unwrap(), 60.0), "{:?}", track.keys);
        assert!(
            near(track.eval(0.0).unwrap(), 0.0) && near(track.eval(10.0).unwrap(), 100.0),
            "other keys untouched"
        );
        assert!(near(ed.engine().evaluate(5.0).nodes[0].world.tx, 60.0));
        assert!(out
            .edits
            .iter()
            .any(|e| e.prop == "layer_x" && e.mode == "key"));
        assert!(
            out.edits
                .iter()
                .any(|e| e.prop == "layer_y" && e.mode == "static"),
            "{:?}",
            out.edits
        );
        assert!(near(ed.project().layers[0].transform.position.y, 3.0));
        assert_eq!(ed.project().channels.len(), 1, "no channel created for y");
        // not animated, auto-key off: static edit, no channels
        let n = ed.project().channels.len();
        commit(
            &mut ed,
            &req(&[l("b")], Handle::Move, v(1.0, 1.0), v(5.0, 0.0)),
        )
        .unwrap();
        assert_eq!(ed.project().channels.len(), n);
        assert!(near(ed.project().layers[1].transform.position.x, 5.0));
        // not animated, auto-key on: channel created, static value untouched
        let mut r = req(&[l("b")], Handle::Move, v(1.0, 1.0), v(0.0, 8.0));
        r.auto_key = true;
        r.frame = 3.0;
        commit(&mut ed, &r).unwrap();
        assert_eq!(ed.project().channels.len(), n + 1);
        let c = ed.project().channels.last().unwrap();
        assert!(c.target == "b" && c.prop == Prop::LayerY);
        let Source::Keys { track } = &c.source else {
            panic!()
        };
        assert!(near(track.eval(3.0).unwrap(), 8.0) && track.keys[0].frame == 3.0);
        assert!(near(ed.project().layers[1].transform.position.y, 0.0));
        // undo removes the channel again
        assert!(ed.undo());
        assert_eq!(ed.project().channels.len(), n);
    }

    #[test]
    fn locked_channel_and_locked_layer_refuse_the_whole_drag() {
        let mut p = Project::new("t", 400.0, 300.0, 24.0, 11);
        p.layers = vec![
            rect("a", 0.0, 0.0, 10.0, 10.0),
            rect("b", 0.0, 0.0, 10.0, 10.0),
        ];
        p.channels = vec![Channel::keys(
            "a",
            Prop::LayerX,
            Track::with_keys(&[(0.0, 0.0, Ease::Linear), (10.0, 100.0, Ease::Linear)]),
        )];
        crate::chanmeta::set_meta(
            &mut p,
            "a",
            Some(Prop::LayerX),
            &crate::chanmeta::Change {
                lock: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let mut ed = Editor::new(p).unwrap();
        let before = ed.doc().clone();
        let e = commit(
            &mut ed,
            &req(&[l("b"), l("a")], Handle::Move, v(1.0, 1.0), v(5.0, 5.0)),
        )
        .unwrap_err();
        assert!(e.join(" ").contains("locked"), "{e:?}");
        assert_eq!(
            ed.doc(),
            &before,
            "nothing applied, not even the unlocked layer"
        );
        assert!(!ed.can_undo());
        let mut r = req(&[l("b")], Handle::Move, v(1.0, 1.0), v(5.0, 5.0));
        r.locked = vec!["b".into()];
        assert!(commit(&mut ed, &r)
            .unwrap_err()
            .join(" ")
            .contains("locked"));
    }

    #[test]
    fn zero_drag_leaves_no_history_and_bad_input_is_an_error() {
        let mut ed = editor(vec![rect("a", 0.0, 0.0, 10.0, 10.0)]);
        let out = commit(
            &mut ed,
            &req(&[l("a")], Handle::Move, v(1.0, 1.0), v(0.0, 0.0)),
        )
        .unwrap();
        assert!(!out.changed && !ed.can_undo());
        assert!(commit(&mut ed, &req(&[], Handle::Move, v(0.0, 0.0), v(1.0, 1.0))).is_err());
        assert!(commit(
            &mut ed,
            &req(&[l("nope")], Handle::Move, v(0.0, 0.0), v(1.0, 1.0))
        )
        .is_err());
        assert!(commit(
            &mut ed,
            &req(&[l("a")], Handle::Move, v(0.0, 0.0), v(f32::NAN, 1.0))
        )
        .is_err());
        assert!(commit(
            &mut ed,
            &req(&[l("a")], Handle::BoneTip, v(0.0, 0.0), v(1.0, 1.0))
        )
        .is_err());
        // a line has no extent along one axis: the box is padded, scaling its thin axis is finite
        let line = Layer::vector(
            "ln",
            vec![VPath::new(
                "p",
                false,
                vec![Anchor::corner(v(0.0, 50.0)), Anchor::corner(v(100.0, 50.0))],
            )
            .with_stroke("#000", 2.0)],
        );
        let ed2 = editor(vec![line]);
        let pr = preview(
            &ed2,
            &req(
                &[l("ln")],
                Handle::Scale { at: ScaleHandle::E },
                v(100.0, 50.0),
                v(100.0, 0.0),
            ),
        )
        .unwrap();
        assert!(pr.matrix.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn decompose_round_trips_and_prefers_negative_scale_for_mirrors() {
        let t0 = Transform2D {
            position: v(30.0, -12.0),
            rotation: 33.0,
            scale: v(1.5, 0.7),
            skew: 12.0,
            anchor: v(5.0, 9.0),
        };
        let t1 = decompose(&t0.matrix(), t0.anchor, 33.0).unwrap();
        assert!(
            near(t1.rotation, 33.0)
                && near(t1.scale.x, 1.5)
                && near(t1.scale.y, 0.7)
                && near(t1.skew, 12.0)
                && t1.position.distance(t0.position) < 1e-3,
            "{t1:?}"
        );
        // mirror in x
        let m = Affine2::scale_xy(-1.0, 1.0);
        let t = decompose(&m, Vec2::ZERO, 0.0).unwrap();
        assert!(
            near(t.scale.x, -1.0) && near(t.scale.y, 1.0) && near(t.rotation, 0.0),
            "{t:?}"
        );
        // continuity across a full turn
        let big = Transform2D {
            rotation: 725.0,
            ..Default::default()
        };
        let t = decompose(&big.matrix(), Vec2::ZERO, 720.0).unwrap();
        assert!(near(t.rotation, 725.0), "{}", t.rotation);
        assert!(decompose(&Affine2::scale_xy(0.0, 1.0), Vec2::ZERO, 0.0).is_err());
    }

    fn bone_project() -> Editor {
        let mut p = Project::new("t", 400.0, 300.0, 24.0, 11);
        p.bones = vec![
            Bone::new("b", None, 100.0, v(50.0, 50.0), 0.0),
            Bone::new("c", Some("b"), 50.0, v(100.0, 0.0), 0.0),
        ];
        Editor::new(p).unwrap()
    }

    #[test]
    fn bone_tip_rotates_and_stretches_head_moves_and_keys_are_one_undo() {
        let mut ed = bone_project();
        let before = ed.doc().clone();
        // tip at (150,50); pull it to (150,150): 45 degrees, length 100 -> 141.42
        let r = req(
            &[Target::bone("b")],
            Handle::BoneTip,
            v(150.0, 50.0),
            v(0.0, 100.0),
        );
        let out = commit(&mut ed, &r).unwrap();
        assert!(
            out.edits
                .iter()
                .any(|e| e.prop == "bone_angle" && near(e.to, 45.0)),
            "{:?}",
            out.edits
        );
        assert!(out
            .edits
            .iter()
            .any(|e| e.prop == "bone_scale" && near(e.to, 2f32.sqrt())));
        let f = ed.engine().evaluate(0.0);
        let (h, t) = bone_ends(ed.engine(), &f, "b").unwrap();
        assert!(
            h.distance(v(50.0, 50.0)) < 1e-2 && t.distance(v(150.0, 150.0)) < 1e-2,
            "{h:?} {t:?}"
        );
        assert!(ed.undo() && ed.doc() == &before && !ed.can_undo());
        // constrain: rotate only, snapped to 15 degree steps, no stretch key
        let mut r = req(
            &[Target::bone("b")],
            Handle::BoneTip,
            v(150.0, 50.0),
            v(0.0, 60.0),
        );
        r.modifiers.constrain = true;
        let out = commit(&mut ed, &r).unwrap();
        assert!(
            out.edits.iter().all(|e| e.prop != "bone_scale"),
            "{:?}",
            out.edits
        );
        let ang = out
            .edits
            .iter()
            .find(|e| e.prop == "bone_angle")
            .unwrap()
            .to;
        assert!(near(ang, 30.0), "atan(60/100)=31 deg snaps to 30: {ang}");
        ed.undo();
        // head moves the whole bone: offsets keyed, angle untouched
        let r = req(
            &[Target::bone("b")],
            Handle::BoneHead,
            v(50.0, 50.0),
            v(10.0, -5.0),
        );
        let out = commit(&mut ed, &r).unwrap();
        assert!(
            out.edits.iter().all(|e| e.prop != "bone_angle"),
            "{:?}",
            out.edits
        );
        let f = ed.engine().evaluate(0.0);
        let (h, _) = bone_ends(ed.engine(), &f, "b").unwrap();
        assert!(h.distance(v(60.0, 45.0)) < 1e-2, "{h:?}");
        // the child bone follows (it is attached to the parent's tip)
        let (ch, _) = bone_ends(ed.engine(), &f, "c").unwrap();
        assert!(ch.distance(v(160.0, 45.0)) < 1e-2, "{ch:?}");
    }

    #[test]
    fn child_bone_in_a_rotated_parent_and_ik_refusal() {
        let mut ed = bone_project();
        ed.set_key("b", Prop::BoneAngle, 0.0, 90.0, Ease::Linear)
            .unwrap();
        // b now points down; c hangs from its tip at (50,150), pointing down
        let f = ed.engine().evaluate(0.0);
        let (ch, ct) = bone_ends(ed.engine(), &f, "c").unwrap();
        assert!(
            ch.distance(v(50.0, 150.0)) < 1e-2 && ct.distance(v(50.0, 200.0)) < 1e-2,
            "{ch:?} {ct:?}"
        );
        let r = req(&[Target::bone("c")], Handle::BoneTip, ct, v(50.0, 0.0)); // swing tip to (100,200)
        commit(&mut ed, &r).unwrap();
        let f = ed.engine().evaluate(0.0);
        let (_, ct2) = bone_ends(ed.engine(), &f, "c").unwrap();
        assert!(ct2.distance(v(100.0, 200.0)) < 1e-2, "{ct2:?}");

        let mut p = ed.project().clone();
        p.ik = vec![IkChain::two_bone("arm", "b", "c", "t", 1.0)];
        p.bones
            .push(Bone::new("t", None, 5.0, v(200.0, 100.0), 0.0));
        let mut ed2 = Editor::new(p).unwrap();
        let e = commit(
            &mut ed2,
            &req(
                &[Target::bone("b")],
                Handle::BoneHead,
                v(50.0, 50.0),
                v(5.0, 5.0),
            ),
        )
        .unwrap_err();
        assert!(e.join(" ").contains("IK"), "{e:?}");
        // the IK target itself is free to move
        commit(
            &mut ed2,
            &req(
                &[Target::bone("t")],
                Handle::BoneHead,
                v(200.0, 100.0),
                v(5.0, 5.0),
            ),
        )
        .unwrap();
    }

    #[test]
    fn point_drags_static_keyed_and_deformer_points() {
        let mut layer = rect("a", 10.0, 10.0, 100.0, 100.0);
        layer.deformers.push(Deformer::Cage {
            name: "cg".into(),
            rest: vec![v(0.0, 0.0), v(200.0, 0.0), v(200.0, 200.0), v(0.0, 200.0)],
            offsets: vec![],
        });
        let mut ed = editor(vec![layer]);
        let pt = |k: PointKind| Target::Point {
            layer: "a".into(),
            point: k,
        };
        // static anchor move (handles, if any, travel with it)
        let a0 = pt(PointKind::Anchor { path: 0, index: 0 });
        commit(
            &mut ed,
            &req(
                std::slice::from_ref(&a0),
                Handle::Move,
                v(10.0, 10.0),
                v(7.0, 3.0),
            ),
        )
        .unwrap();
        let LayerKind::Vector { paths } = &ed.project().layers[0].kind else {
            panic!()
        };
        assert!(paths[0].anchors[0].pos.distance(v(17.0, 13.0)) < 1e-3);
        assert!(ed.undo());
        // auto-key: PointDx/PointDy channels created at the frame
        let mut r = req(
            std::slice::from_ref(&a0),
            Handle::Move,
            v(10.0, 10.0),
            v(7.0, 3.0),
        );
        r.auto_key = true;
        r.frame = 4.0;
        commit(&mut ed, &r).unwrap();
        assert!(ed
            .project()
            .channels
            .iter()
            .any(|c| c.prop == Prop::PointDx && c.target == "a::r::0"));
        let f = ed.engine().evaluate(4.0);
        assert!(f.nodes[0].paths[0].anchors[0].pos.distance(v(17.0, 13.0)) < 1e-3);
        ed.undo();
        // cage point: static offsets array is created in full
        let c1 = pt(PointKind::Cage {
            deformer: 0,
            index: 1,
        });
        commit(
            &mut ed,
            &req(
                std::slice::from_ref(&c1),
                Handle::Move,
                v(200.0, 0.0),
                v(0.0, 12.0),
            ),
        )
        .unwrap();
        let Deformer::Cage { offsets, .. } = &ed.project().layers[0].deformers[0] else {
            panic!()
        };
        assert_eq!(offsets.len(), 4);
        assert!(offsets[1].distance(v(0.0, 12.0)) < 1e-3 && offsets[0].length() < 1e-6);
        ed.undo();
        let mut r = req(&[c1], Handle::Move, v(200.0, 0.0), v(0.0, 12.0));
        r.auto_key = true;
        commit(&mut ed, &r).unwrap();
        assert!(ed
            .project()
            .channels
            .iter()
            .any(|c| c.prop == Prop::CageDy && c.target == "a::cg::1"));
        // points cannot be mixed with layers, nor rotated
        assert!(commit(
            &mut ed,
            &req(
                &[a0.clone(), l("a")],
                Handle::Move,
                v(0.0, 0.0),
                v(1.0, 1.0)
            )
        )
        .is_err());
        assert!(commit(
            &mut ed,
            &req(&[a0], Handle::Rotate, v(0.0, 0.0), v(1.0, 1.0))
        )
        .is_err());
    }

    #[test]
    fn nudge_and_delete_are_single_undo_steps() {
        let mut p = Project::new("t", 400.0, 300.0, 24.0, 11);
        p.layers = vec![
            rect("a", 0.0, 0.0, 10.0, 10.0),
            rect("b", 20.0, 0.0, 10.0, 10.0),
            rect("c", 40.0, 0.0, 10.0, 10.0),
        ];
        p.channels = vec![
            Channel::keys(
                "b",
                Prop::LayerX,
                Track::with_keys(&[(0.0, 0.0, Ease::Linear), (10.0, 5.0, Ease::Linear)]),
            ),
            Channel::keys(
                "c",
                Prop::LayerY,
                Track::with_keys(&[(0.0, 0.0, Ease::Linear)]),
            ),
        ];
        let mut ed = Editor::new(p).unwrap();
        let before = ed.doc().clone();
        nudge(&mut ed, 0.0, &[l("a")], v(1.0, 0.0), false, &[]).unwrap();
        assert!(near(ed.project().layers[0].transform.position.x, 1.0));
        assert!(ed.undo() && ed.doc() == &before);
        delete(&mut ed, &[l("b"), l("a")]).unwrap();
        assert_eq!(ed.project().layers.len(), 1);
        assert_eq!(ed.project().layers[0].name, "c");
        assert_eq!(
            ed.project().channels.len(),
            1,
            "channels of deleted layers go too"
        );
        assert!(ed.undo() && ed.doc() == &before, "delete is one step");
        assert!(delete(&mut ed, &[]).is_err());
        assert!(delete(&mut ed, &[Target::bone("x")]).is_err());
    }

    #[test]
    fn deleting_a_layer_that_is_still_referenced_is_rejected_and_rolled_back() {
        let mut p = Project::new("t", 400.0, 300.0, 24.0, 11);
        let mut m = rect("m", 0.0, 0.0, 10.0, 10.0);
        m.mask = Some(crate::project::MaskRef {
            layer: "a".into(),
            invert: false,
        });
        p.layers = vec![rect("a", 0.0, 0.0, 10.0, 10.0), m];
        let mut ed = Editor::new(p).unwrap();
        let before = ed.doc().clone();
        assert!(delete(&mut ed, &[l("a")]).is_err());
        assert_eq!(ed.doc(), &before);
    }

    #[test]
    fn handle_hit_testing_priorities_and_cursors() {
        let bx = BoxFrame::from_aabb(v(0.0, 0.0), v(100.0, 100.0));
        assert_eq!(
            hit_handle(&bx, v(100.0, 100.0), 6.0),
            Some(Handle::Scale {
                at: ScaleHandle::Se
            })
        );
        assert_eq!(
            hit_handle(&bx, v(50.0, 1.0), 6.0),
            Some(Handle::Scale { at: ScaleHandle::N })
        );
        assert_eq!(hit_handle(&bx, v(50.0, 50.0), 6.0), Some(Handle::Move));
        assert_eq!(
            hit_handle(&bx, v(50.0, -ROTATE_OFFSET), 6.0),
            Some(Handle::Rotate)
        );
        assert_eq!(hit_handle(&bx, v(150.0, 50.0), 6.0), None);
        // tiny box: corners win over edges
        let tiny = BoxFrame::from_aabb(v(0.0, 0.0), v(4.0, 4.0));
        assert_eq!(
            hit_handle(&tiny, v(0.0, 0.0), 6.0),
            Some(Handle::Scale {
                at: ScaleHandle::Nw
            })
        );
        let hs = box_handles(&bx);
        let cur = |h: ScaleHandle| {
            hs.iter()
                .find(|x| x.handle == Handle::Scale { at: h })
                .unwrap()
                .cursor
                .clone()
        };
        assert_eq!(
            (
                cur(ScaleHandle::N).as_str(),
                cur(ScaleHandle::E).as_str(),
                cur(ScaleHandle::Se).as_str(),
                cur(ScaleHandle::Ne).as_str()
            ),
            ("ns-resize", "ew-resize", "nwse-resize", "nesw-resize")
        );
        assert_eq!(hs.len(), 9);
        // a rotated box rotates the cursor classes
        let rot = BoxFrame {
            origin: v(0.0, 0.0),
            u: v(0.0, 100.0),
            v: v(-100.0, 0.0),
        };
        let hs = box_handles(&rot);
        assert_eq!(
            hs.iter()
                .find(|x| x.handle == Handle::Scale { at: ScaleHandle::E })
                .unwrap()
                .cursor,
            "ns-resize"
        );
    }

    #[test]
    fn gizmo_view_kinds() {
        let ed = editor(vec![rect("a", 0.0, 0.0, 10.0, 10.0)]);
        let f = ed.engine().evaluate(0.0);
        assert_eq!(gizmo_view(ed.engine(), &f, &Selection::new()).kind, "none");
        let g = gizmo_view(ed.engine(), &f, &Selection::from_items(vec![l("a")]));
        assert_eq!((g.kind.as_str(), g.handles.len()), ("box", 9));
        let g = gizmo_view(
            ed.engine(),
            &f,
            &Selection::from_items(vec![Target::Point {
                layer: "a".into(),
                point: PointKind::Anchor { path: 0, index: 0 },
            }]),
        );
        assert_eq!((g.kind.as_str(), g.points.len()), ("points", 1));
        let b = bone_project();
        let fb = b.engine().evaluate(0.0);
        let g = gizmo_view(
            b.engine(),
            &fb,
            &Selection::from_items(vec![Target::bone("b")]),
        );
        assert_eq!((g.kind.as_str(), g.handles.len()), ("bone", 2));
    }

    #[test]
    fn preview_does_not_touch_the_project_and_is_deterministic() {
        let mut ed = editor(vec![
            rect("a", 0.0, 0.0, 100.0, 100.0),
            rect("b", 150.0, 0.0, 50.0, 50.0),
        ]);
        let before = ed.doc().clone();
        let mut r = req(&[l("a")], Handle::Move, v(10.0, 10.0), v(47.0, 3.0));
        r.modifiers.no_snap = false;
        let p1 = serde_json::to_string(&preview(&ed, &r).unwrap()).unwrap();
        let p2 = serde_json::to_string(&preview(&ed, &r).unwrap()).unwrap();
        assert_eq!(p1, p2);
        assert_eq!(ed.doc(), &before);
        assert!(!ed.can_undo());
        let pv = preview(&ed, &r).unwrap();
        assert!(!pv.outline.is_empty() && pv.box_after.is_some() && pv.changed);
        let c1 = commit(&mut ed, &r).unwrap();
        assert_eq!(
            serde_json::to_string(&c1).unwrap(),
            p1,
            "the commit does what the preview showed"
        );
    }

    #[test]
    fn requests_deserialize_from_api_json() {
        let j = json!({"frame": 2, "selection": [{"kind":"layer","path":"a"}], "handle": {"type":"scale","at":"se"}, "start": [1,2], "delta": [3,4], "modifiers": {"constrain": true}});
        let r: DragRequest = serde_json::from_value(j).unwrap();
        assert_eq!(
            r.handle,
            Handle::Scale {
                at: ScaleHandle::Se
            }
        );
        assert!(r.modifiers.constrain && !r.modifiers.center && !r.auto_key && r.snap.objects);
        let j = json!({"frame": 0, "selection": [], "handle": {"type":"nonsense"}, "start": [0,0], "delta": [0,0]});
        assert!(serde_json::from_value::<DragRequest>(j).is_err());
    }

    #[test]
    fn a_locked_layer_refuses_drags_nudges_point_edits_and_deletes_until_unlocked() {
        let mut ed = editor(vec![
            rect("a", 0.0, 0.0, 10.0, 10.0),
            rect("b", 50.0, 0.0, 10.0, 10.0),
        ]);
        ed.apply(Op::Set {
            path: "/layers/0/locked".into(),
            value: serde_json::json!(true),
        })
        .unwrap();
        let before = ed.doc().clone();
        let e = commit(
            &mut ed,
            &req(&[l("a"), l("b")], Handle::Move, v(1.0, 1.0), v(5.0, 5.0)),
        )
        .unwrap_err();
        assert!(e.join(" ").contains("'a' is locked"), "{e:?}");
        assert!(nudge(&mut ed, 0.0, &[l("a")], v(1.0, 0.0), false, &[])
            .unwrap_err()
            .join(" ")
            .contains("locked"));
        assert!(delete(&mut ed, &[l("a")])
            .unwrap_err()
            .join(" ")
            .contains("locked"));
        let pt = Target::Point {
            layer: "a".into(),
            point: crate::pick::PointKind::Anchor { path: 0, index: 0 },
        };
        assert!(nudge(&mut ed, 0.0, &[pt], v(1.0, 0.0), false, &[])
            .unwrap_err()
            .join(" ")
            .contains("locked"));
        assert_eq!(ed.doc(), &before);
        // the unlocked neighbour is fine, and unlocking (a plain Op, as the panel does) allows the edit again
        nudge(&mut ed, 0.0, &[l("b")], v(1.0, 0.0), false, &[]).unwrap();
        ed.apply(Op::Set {
            path: "/layers/0/locked".into(),
            value: serde_json::json!(false),
        })
        .unwrap();
        nudge(&mut ed, 0.0, &[l("a")], v(1.0, 0.0), false, &[]).unwrap();
        delete(&mut ed, &[l("a")]).unwrap();
        // a locked group protects its child
        let mut ed2 = editor(vec![Layer::group(
            "g",
            vec![rect("c", 0.0, 0.0, 10.0, 10.0)],
        )]);
        ed2.apply(Op::Set {
            path: "/layers/0/locked".into(),
            value: serde_json::json!(true),
        })
        .unwrap();
        assert!(nudge(&mut ed2, 0.0, &[l("g/c")], v(1.0, 0.0), false, &[]).is_err());
    }
}
