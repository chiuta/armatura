// Pip: bones, attachments and a walk cycle for the layers imported from pip.svg (osatura import-svg --split).
// add_bone(name, parent ("" = root), length, x, y, angle): x, y and angle are in the parent's space (a root: the stage).
add_bone("body", "", 108, 200, 240, -90);            // pelvis to neck, pointing up
add_bone("head", "body", 60, 108, 0, 0);
for s in ["", "_far"] {
    add_bone("arm_up" + s, "body", 54, 92, 0, 180);  // shoulder, pointing down
    add_bone("arm_low" + s, "arm_up" + s, 50, 54, 0, 0);
    add_bone("thigh" + s, "body", 66, 0, 0, 180);
    add_bone("shin" + s, "thigh" + s, 56, 66, 0, 0);
    add_bone("foot" + s, "shin" + s, 30, 56, 0, -90); // pointing forward
}
// attach every imported layer to its bone: the shapes keep their place and move with the bone
for l in [["torso", "body"], ["head", "head"], ["ear", "head"], ["eye", "head"]] { set_layer(l[0], "bone", l[1]); }
for s in ["", "_far"] {
    set_layer("arm" + s + "_up", "bone", "arm_up" + s);
    set_layer("arm" + s + "_low", "bone", "arm_low" + s);
    set_layer("leg" + s + "_up", "bone", "thigh" + s);
    set_layer("leg" + s + "_low", "bone", "shin" + s);
    set_layer("foot" + s, "bone", "foot" + s);
}
// the walk: one cycle in 24 frames; the far side is the near side half a cycle later
let f = [0, 6, 12, 18, 24];
let thigh = [-25, 0, 25, 0, -25];
let shin = [5, 5, 10, 40, 5];
let arm = [20, 0, -20, 0, 20];
let elbow = [-10, -22, -35, -22, -10];
for i in 0..5 {
    for side in [0, 1] {
        let s = if side == 0 { "" } else { "_far" };
        let j = (i + 2 * side) % 4;                  // 12 frames later = 2 keys on (the cycle has 4 distinct keys)
        set_key("thigh" + s, "bone_angle", f[i], thigh[j], "smooth");
        set_key("shin" + s, "bone_angle", f[i], shin[j], "smooth");
        set_key("foot" + s, "bone_angle", f[i], 0 - thigh[j] - shin[j], "smooth");
        set_key("arm_up" + s, "bone_angle", f[i], arm[j], "smooth");
        set_key("arm_low" + s, "bone_angle", f[i], elbow[j], "smooth");
    }
}
let bob = [0, -5, 0, -5, 0];
let nod = [0, 2, 0, 2, 0];
for i in 0..5 {
    set_key("body", "bone_offset_y", f[i], bob[i], "smooth");
    set_key("head", "bone_angle", f[i], nod[i], "smooth");
}
