// Pasul 6: salutul, cu mână cu cinci degete desfăcute (palma spre privitor, ca în gestul Trade-Free).
// 1) Mâna: o palmă și cinci degete, desenate în repaus (mâna atârnă în jos, cu degetele în evantai).
//    Degetul mare e pe partea stângă: când antebrațul se ridică, mâna se rotește cu ~180° și degetul mare ajunge spre corp.
let wx = 174.0; let wy = 228.0;                 // încheietura în repaus = capătul antebrațului drept
let skin = "#f2cc8f";
add_circle("palm", wx, wy + 8.0, 8.5, skin);
// (nume, abaterea bazei pe palmă, unghiul față de „în jos” în grade, lungimea, lățimea)
let fingers = [
  ["thumb",  -5.0, -66.0, 14.0, 5.0],
  ["index",  -4.6, -24.0, 17.0, 4.4],
  ["middle", -1.5,  -8.0, 19.0, 4.4],
  ["ring",    1.8,   9.0, 17.0, 4.4],
  ["pinky",   5.0,  26.0, 13.0, 4.0]
];
for f in fingers {
  let a = f[2] * PI() / 180.0;
  let dx = sin(a); let dy = cos(a);              // direcția degetului (în jos = +y)
  let px = dy; let py = -dx;                      // perpendiculara
  let bx = wx + f[1]; let by = wy + 9.0;          // baza degetului, în interiorul palmei
  let hw = f[4] / 2.0; let L = f[3];
  let mx = bx + dx * L * 0.7; let my = by + dy * L * 0.7;
  let tx = bx + dx * L; let ty = by + dy * L;
  add_blob("finger_" + f[0], [
    bx - px * hw, by - py * hw,  bx + px * hw, by + py * hw,
    mx + px * hw, my + py * hw,  tx + px * hw * 0.55, ty + py * hw * 0.55,
    tx - px * hw * 0.55, ty - py * hw * 0.55,  mx - px * hw, my - py * hw
  ], skin);
}
// 2) Osul mâinii, copil al antebrațului; toate piesele mâinii îl urmează rigid.
add_bone("hand_r", "arm_lower_r", 30, 45, 0, 0);
set_layer("palm", "bone", "hand_r");
for n in ["thumb", "index", "middle", "ring", "pinky"] { set_layer("finger_" + n, "bone", "hand_r"); }
// 3) IK: cotul se ridică în afară, iar antebrațul atârnă în jos cu mâna deschisă.
add_bone("hand_ctrl", "", 10, 174, 228, 0);
add_ik("wave_ik", ["arm_upper_r", "arm_lower_r"], "hand_ctrl", 1);
// 4) Salutul: încheietura se leagănă stânga-dreapta (ca un pendul, nu pe o diagonală), iar palma o urmărește cu o mică întârziere.
let xs = [-49.0, -61.0, -49.0, -37.0, -49.0];
let ys = [-58.0, -60.0, -58.0, -60.0, -58.0];
for i in 0..5 {
  set_key("hand_ctrl", "bone_offset_x", i * 6, xs[i], "ease_in_out");
  set_key("hand_ctrl", "bone_offset_y", i * 6, ys[i], "ease_in_out");
}
set_key("hand_r", "bone_angle", 0, 0, "ease_in_out");
set_key("hand_r", "bone_angle", 3, -10, "ease_in_out");
set_key("hand_r", "bone_angle", 9, 10, "ease_in_out");
set_key("hand_r", "bone_angle", 15, -10, "ease_in_out");
set_key("hand_r", "bone_angle", 21, 10, "ease_in_out");
set_key("hand_r", "bone_angle", 24, 0, "ease_in_out");
