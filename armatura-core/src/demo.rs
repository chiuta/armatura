// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
//! Built-in demo: an FK-waved arm, an IK-driven leg, multiplane parallax camera,
//! gradients, blend modes, variable-width ink, a switch-layer mouth, expression /
//! noise / driven channels.

use crate::anim::{Ease, Extrapolate, Track};
use crate::math::{Affine2, Vec2};
use crate::path::{Anchor, BoneWeight, VPath};
use crate::project::{Bone, Channel, IkChain, Layer, Mode, Project, Prop};
use crate::style::{Blend, Cap, Paint, Rgba, StrokeStyle};

fn bone(name: &str, parent: Option<&str>, length: f32, off: (f32, f32), rest: f32) -> Bone {
    Bone::new(name, parent, length, Vec2::new(off.0, off.1), rest)
}

/// Closed, smoothed outline around a bone chain (rest pose), tapering from
/// `r0` at the first head to `r1` at the last tip.
pub fn limb_outline(name: &str, rest: &[Affine2], bones: &[Bone], ids: &[usize], r0: f32, r1: f32) -> VPath {
    let dir = |i: usize| Vec2::from_angle(rest[i].rotation());
    let mut samples: Vec<(Vec2, Vec2)> = Vec::new();
    for (k, &i) in ids.iter().enumerate() {
        let head = rest[i].apply(Vec2::ZERO);
        let d = if k == 0 { dir(i) } else { (dir(ids[k - 1]) + dir(i)).normalized() };
        samples.push((head, d));
        samples.push((rest[i].apply(Vec2::new(bones[i].length * 0.5, 0.0)), dir(i)));
    }
    let last = *ids.last().unwrap();
    samples.push((rest[last].apply(Vec2::new(bones[last].length, 0.0)), dir(last)));

    let n = samples.len();
    let radius = |k: usize| r0 + (r1 - r0) * (k as f32 / (n - 1) as f32);
    let normal = |d: Vec2| Vec2::new(-d.y, d.x);
    let top: Vec<Vec2> = samples.iter().enumerate().map(|(k, &(c, d))| c + normal(d) * radius(k)).collect();
    let bot: Vec<Vec2> = samples.iter().enumerate().map(|(k, &(c, d))| c - normal(d) * radius(k)).collect();
    let pts: Vec<Vec2> = top.into_iter().chain(bot.into_iter().rev()).collect();
    VPath::smooth_through(name, &pts, true, 1.0)
}

fn hex(s: &str) -> Rgba {
    Rgba::hex(s)
}

pub fn project() -> Project {
    let bones = vec![
        // FK arm
        bone("arm_up", None, 110.0, (150.0, 330.0), -70.0),
        bone("arm_low", Some("arm_up"), 100.0, (110.0, 0.0), -10.0),
        bone("hand", Some("arm_low"), 45.0, (100.0, 0.0), 0.0),
        // IK leg (rest pose points straight down)
        bone("thigh", None, 120.0, (440.0, 170.0), 90.0),
        bone("shin", Some("thigh"), 120.0, (120.0, 0.0), 0.0),
        // free control bone the foot is pulled toward
        bone("foot_ctrl", None, 10.0, (440.0, 385.0), 0.0),
    ];

    let mut p = Project::new("ARMATURA demo", 640.0, 480.0, 24.0, 48);
    p.background = Some(hex("#f6f1e7"));
    p.bones = bones;
    p.ik = vec![IkChain::two_bone("leg", "thigh", "shin", "foot_ctrl", -1.0)];

    let rest = p.rest_world().expect("demo rig is valid");

    // --- backdrop (parallax layers wider than the stage so the camera can pan) ---
    let sky_paint = Paint::linear(
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 480.0),
        &[(0.0, "#9ec5e8"), (0.65, "#f7e3c4"), (1.0, "#f6f1e7")],
    );
    let mut sky = Layer::vector("sky", vec![VPath::rect("sky", -300.0, 0.0, 1240.0, 480.0).with_paint(sky_paint)]);
    sky.parallax = 0.15;
    let mut sun = Layer::vector(
        "sun",
        vec![VPath::circle("sun", Vec2::new(520.0, 90.0), 70.0).with_paint(Paint::radial(
            Vec2::new(520.0, 90.0),
            70.0,
            &[(0.0, "#fffbe6"), (0.5, "#ffd98a"), (1.0, "#ffd98a00")],
        ))],
    );
    sun.parallax = 0.2;
    sun.blend = Blend::Screen;
    let hills_pts: Vec<Vec2> = (0..=12)
        .map(|i| {
            let x = -250.0 + i as f32 * 90.0;
            Vec2::new(x, 350.0 - 40.0 * ((i as f32) * 0.9).sin() - 20.0)
        })
        .chain([Vec2::new(830.0, 480.0), Vec2::new(-250.0, 480.0)])
        .collect();
    let mut hills = Layer::vector(
        "hills",
        vec![VPath::smooth_through("hills", &hills_pts, true, 0.6).with_fill("#c3d3a8").with_stroke("#a7ba8a", 2.0)],
    );
    hills.parallax = 0.5;
    hills.opacity = 0.95;

    // --- foreground rig layers ---
    let arm_fill = Paint::linear(Vec2::new(150.0, 230.0), Vec2::new(150.0, 340.0), &[(0.0, "#f2b98f"), (1.0, "#e08f5f")]);
    let leg_fill = Paint::linear(Vec2::new(380.0, 170.0), Vec2::new(500.0, 170.0), &[(0.0, "#9db8d6"), (1.0, "#6d8fb8")]);
    let arm = limb_outline("arm", &rest, &p.bones, &[0, 1, 2], 24.0, 12.0)
        .with_paint(arm_fill)
        .with_stroke("#3d2c2e", 3.0);
    let leg = limb_outline("leg", &rest, &p.bones, &[3, 4], 26.0, 14.0)
        .with_paint(leg_fill)
        .with_stroke("#3d2c2e", 3.0);

    let mut marker = VPath::circle("foot target", Vec2::new(440.0, 385.0), 8.0)
        .with_fill("#d6336c")
        .with_stroke("#3d2c2e", 2.0);
    for a in &mut marker.anchors {
        a.weights = vec![BoneWeight { bone: "foot_ctrl".into(), weight: 1.0 }];
    }
    let mut dash = StrokeStyle::solid("#8a7f72", 4.0);
    dash.cap = Cap::Butt;
    dash.dash = vec![14.0, 8.0];
    let ground = VPath::new(
        "ground",
        false,
        vec![Anchor::corner(Vec2::new(-200.0, 412.0)), Anchor::corner(Vec2::new(840.0, 412.0))],
    )
    .with_stroke_style(dash);

    // variable-width ink swoosh, swayed by an expression
    let mut ink_stroke = StrokeStyle::new(
        Paint::linear(Vec2::new(60.0, 80.0), Vec2::new(300.0, 120.0), &[(0.0, "#6a4c93"), (1.0, "#d6336c")]),
        16.0,
    );
    ink_stroke.width_profile = vec![[0.0, 0.05], [0.3, 1.0], [0.7, 0.6], [1.0, 0.02]];
    let swoosh = VPath::smooth_through(
        "swoosh",
        &[Vec2::new(60.0, 110.0), Vec2::new(130.0, 70.0), Vec2::new(210.0, 100.0), Vec2::new(300.0, 70.0)],
        false,
        1.0,
    )
    .with_stroke_style(ink_stroke);
    let mut ink = Layer::vector("ink", vec![swoosh]);
    ink.transform.anchor = Vec2::new(60.0, 110.0);
    ink.transform.position = Vec2::new(60.0, 110.0);

    // soft shadow under the leg (multiply)
    let mut shadow = Layer::vector(
        "shadow",
        vec![VPath::ellipse("shadow", Vec2::new(440.0, 412.0), 52.0, 9.0).with_paint(Paint::radial(
            Vec2::new(440.0, 412.0),
            52.0,
            &[(0.0, "#3d2c2eaa"), (1.0, "#3d2c2e00")],
        ))],
    );
    shadow.blend = Blend::Multiply;
    shadow.transform.anchor = Vec2::new(440.0, 412.0);
    shadow.transform.position = Vec2::new(440.0, 412.0);

    // blush on the arm: scale driven by the elbow angle
    let mut blush = Layer::vector(
        "blush",
        vec![VPath::circle("blush", Vec2::ZERO, 9.0).with_paint(Paint::radial(
            Vec2::ZERO,
            9.0,
            &[(0.0, "#ff6b6bcc"), (1.0, "#ff6b6b00")],
        ))],
    );
    blush.bone = Some("arm_low".into());
    blush.transform.position = rest[1].apply(Vec2::new(40.0, 0.0));

    // mouth: a switch layer attached to the hand
    let mouth_shapes = vec![
        Layer::vector("closed", vec![VPath::new(
            "m",
            false,
            vec![Anchor::corner(Vec2::new(-8.0, 0.0)), Anchor::corner(Vec2::new(8.0, 0.0))],
        )
        .with_stroke("#3d2c2e", 3.0)]),
        Layer::vector("open", vec![VPath::ellipse("m", Vec2::ZERO, 8.0, 6.0).with_fill("#3d2c2e")]),
        Layer::vector("smile", vec![VPath::smooth_through(
            "m",
            &[Vec2::new(-9.0, -2.0), Vec2::new(0.0, 5.0), Vec2::new(9.0, -2.0)],
            false,
            1.0,
        )
        .with_stroke("#3d2c2e", 3.0)]),
    ];
    let mut mouth = Layer::switch("mouth", mouth_shapes);
    mouth.bone = Some("hand".into());
    mouth.transform.position = rest[2].apply(Vec2::new(24.0, 0.0));

    p.layers = vec![
        sky,
        sun,
        hills,
        Layer::vector("ground", vec![ground]),
        shadow,
        Layer::vector("arm", vec![arm]),
        Layer::vector("leg", vec![leg]),
        Layer::vector("target", vec![marker]),
        blush,
        mouth,
        ink,
    ];
    p.auto_skin("arm", 2).unwrap();
    p.auto_skin("leg", 2).unwrap();

    // --- animation (every looping channel has first key == last key) ---
    let ease = Ease::EaseInOut;
    let keys = |target: &str, prop: Prop, ks: &[(f32, f32)]| {
        Channel::keys(target, prop, Track::with_keys(&ks.iter().map(|&(f, v)| (f, v, ease)).collect::<Vec<_>>()))
    };
    p.channels = vec![
        keys("arm_up", Prop::BoneAngle, &[(0.0, 0.0), (12.0, -25.0), (24.0, 0.0), (36.0, 15.0), (48.0, 0.0)]),
        keys("arm_low", Prop::BoneAngle, &[(0.0, 0.0), (12.0, 35.0), (24.0, -15.0), (36.0, 30.0), (48.0, 0.0)]),
        keys("hand", Prop::BoneAngle, &[(0.0, 0.0), (12.0, 20.0), (24.0, -20.0), (36.0, 20.0), (48.0, 0.0)]),
        // procedural wiggle layered on the hand (additive; zero-mean so the loop stays tidy)
        Channel::expr("hand", Prop::BoneAngle, "6 * sin(time * 2 * pi * 2)", Mode::Add),
        keys("foot_ctrl", Prop::BoneOffsetX, &[(0.0, -40.0), (12.0, 0.0), (24.0, 40.0), (36.0, 0.0), (48.0, -40.0)]),
        keys("foot_ctrl", Prop::BoneOffsetY, &[(0.0, 0.0), (12.0, -45.0), (24.0, 0.0), (36.0, 0.0), (48.0, 0.0)]),
        keys("target", Prop::LayerOpacity, &[(0.0, 0.4), (24.0, 1.0), (48.0, 0.4)]),
        // shadow shrinks when the foot lifts (expression reads the foot's height)
        Channel::expr("shadow", Prop::LayerScaleX, "1 + get(\"foot_ctrl\", \"bone_offset_y\") / 160", Mode::Replace),
        Channel::expr("shadow", Prop::LayerX, "440 + get(\"foot_ctrl\", \"bone_offset_x\")", Mode::Replace),
        // blush grows as the elbow bends (driven key)
        Channel::driven(
            "blush",
            Prop::LayerScaleX,
            "arm_low",
            Prop::BoneAngle,
            Track::with_keys(&[(-15.0, 0.6, Ease::Linear), (35.0, 2.2, Ease::Linear)]).looped(Extrapolate::Hold),
        ),
        Channel::driven(
            "blush",
            Prop::LayerScaleY,
            "arm_low",
            Prop::BoneAngle,
            Track::with_keys(&[(-15.0, 0.6, Ease::Linear), (35.0, 2.2, Ease::Linear)]),
        ),
        // mouth swaps with step keys
        Channel::keys(
            "mouth",
            Prop::LayerSwitch,
            Track::with_keys(&[(0.0, 0.0, Ease::Step), (10.0, 1.0, Ease::Step), (16.0, 2.0, Ease::Step), (30.0, 1.0, Ease::Step), (38.0, 0.0, Ease::Step), (48.0, 0.0, Ease::Step)]),
        ),
        // ink swoosh sways, loops seamlessly
        Channel::expr("ink", Prop::LayerRotation, "8 * sin(t * 2 * pi)", Mode::Replace),
        // slow camera pan
        keys("", Prop::CameraX, &[(0.0, 320.0), (24.0, 350.0), (48.0, 320.0)]),
    ];
    p
}

/// Second demo: liquid metaballs bound to bones, a boolean-cut frame and a particle fountain.
pub fn fx_project() -> Project {
    use crate::fx::{Blob, BoolOp, EmitterSpec};
    let mut p = Project::new("ARMATURA fx demo", 640.0, 480.0, 24.0, 72);
    p.background = Some(Rgba::hex("#10182b"));
    p.bones = vec![
        Bone::new("a", None, 10.0, Vec2::new(200.0, 240.0), 0.0),
        Bone::new("b", None, 10.0, Vec2::new(440.0, 240.0), 0.0),
        Bone::new("c", None, 10.0, Vec2::new(320.0, 120.0), 0.0),
    ];
    let b = |bone: &str, r: f32| Blob { pos: Vec2::ZERO, radius: r, bone: Some(bone.into()) };
    p.layers = vec![
        Layer::boolean(
            "window",
            BoolOp::Difference,
            vec![
                VPath::rect("wall", 20.0, 20.0, 600.0, 440.0).with_paint(Paint::linear(Vec2::new(0.0, 0.0), Vec2::new(0.0, 480.0), &[(0.0, "#2b3a67"), (1.0, "#1b2340")])),
                VPath::circle("hole", Vec2::new(320.0, 240.0), 190.0),
            ],
        ),
        {
            let mut l = Layer::liquid(
                "goo",
                vec![b("a", 34.0), b("b", 30.0), b("c", 26.0), Blob { pos: Vec2::new(320.0, 300.0), radius: 40.0, bone: None }],
                Paint::radial(Vec2::new(320.0, 240.0), 200.0, &[(0.0, "#7ef0d0"), (1.0, "#1fa38a")]),
            );
            l.blend = Blend::Normal;
            l
        },
        Layer::emitter(
            "sparks",
            EmitterSpec {
                pos: Vec2::new(320.0, 300.0),
                rate: 60.0,
                lifetime: 1.4,
                lifetime_jitter: 0.3,
                speed: 190.0,
                speed_jitter: 0.4,
                direction: -90.0,
                spread: 50.0,
                gravity: Vec2::new(0.0, 260.0),
                size_start: 4.0,
                size_end: 1.0,
                color_start: Rgba::hex("#fff2a8"),
                color_end: Rgba::hex("#ff5a36").with_alpha(0.0),
                seed: 11,
                ..Default::default()
            },
        ),
    ];
    let loopk = |lo: f32, hi: f32| Track::with_keys(&[(0.0, lo, Ease::Smooth), (36.0, hi, Ease::Smooth), (72.0, lo, Ease::Smooth)]);
    p.channels = vec![
        Channel::keys("a", Prop::BoneOffsetX, loopk(0.0, 110.0)),
        Channel::keys("b", Prop::BoneOffsetX, loopk(0.0, -110.0)),
        Channel::keys("c", Prop::BoneOffsetY, loopk(0.0, 120.0)),
    ];
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;

    #[test]
    fn demo_compiles_and_round_trips_through_json() {
        let p = project();
        let json = p.to_json_pretty();
        let back = Project::from_json(&json).unwrap();
        assert_eq!(p, back);
        assert!(Engine::new(back).is_ok());
    }

    #[test]
    fn demo_foot_tracks_the_control_bone_every_frame() {
        let e = Engine::new(project()).unwrap();
        for f in 0..e.project.frames {
            let fr = e.evaluate(f as f32);
            let shin_tip = fr.world[4].apply(Vec2::new(120.0, 0.0));
            let target = fr.world[5].apply(Vec2::ZERO);
            assert!(shin_tip.distance(target) < 0.1, "frame {f}: {shin_tip:?} vs {target:?}");
        }
    }

    #[test]
    fn demo_uses_the_new_features() {
        let e = Engine::new(project()).unwrap();
        let f0 = e.evaluate(0.0);
        let mouth = f0.nodes[e.node_index("mouth").unwrap()].children[0];
        assert_eq!(f0.nodes[mouth].name, "mouth/closed");
        let f14 = e.evaluate(14.0);
        let mouth = f14.nodes[e.node_index("mouth").unwrap()].children[0];
        assert_eq!(f14.nodes[mouth].name, "mouth/open");
        // camera pan moves near layers more than far ones
        let f24 = e.evaluate(24.0);
        let dx = |name: &str| {
            let i = e.node_index(name).unwrap();
            f24.nodes[i].world.tx - f0.nodes[i].world.tx
        };
        assert!(dx("ground").abs() > dx("hills").abs() && dx("hills").abs() > dx("sky").abs());
    }
}
