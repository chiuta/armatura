// Pasul 1: piesele omulețului, desenate din spate în față. Totul e vector: dreptunghiuri și cercuri.
add_gradient_rect("sky", 0, 0, 400, 300, "#bfe3f5", "#fdf3d8");
add_rect("ground", 0, 300, 400, 60, "#7fa66b");
// partea dreaptă a personajului (în spatele trunchiului)
add_rect("thigh_r", 183, 200, 14, 50, "#4a4e78");
add_rect("shin_r", 183, 250, 14, 50, "#2f3249");
add_rect("foot_r", 181, 291, 25, 9, "#25273a");
add_circle("toe_r", 204, 295.5, 4.5, "#25273a");
add_rect("arm_upper_r", 168, 133, 12, 50, "#c9654b");
add_rect("arm_lower_r", 168, 183, 12, 45, "#f2cc8f");
// trunchi și cap
add_rect("torso", 178, 120, 44, 78, "#e07a5f");
add_circle("head", 200, 92, 26, "#f2cc8f");
// partea stângă (în fața trunchiului)
add_rect("thigh_l", 203, 200, 14, 50, "#5f6495");
add_rect("shin_l", 203, 250, 14, 50, "#3d405b");
add_rect("foot_l", 201, 291, 25, 9, "#2a2c3d");
add_circle("toe_l", 224, 295.5, 4.5, "#2a2c3d");
add_rect("arm_upper_l", 220, 133, 12, 50, "#e07a5f");
add_rect("arm_lower_l", 220, 183, 12, 45, "#f2cc8f");
// mâna stângă: o palmă și cinci degete desfăcute (atârnă în jos, în repaus)
add_circle("palm_l", 226, 236, 8.5, "#f2cc8f");
let lf = [["thumb", 5.0, 66.0, 14.0, 5.0], ["index", 4.6, 24.0, 17.0, 4.4], ["middle", 1.5, 8.0, 19.0, 4.4], ["ring", -1.8, -9.0, 17.0, 4.4], ["pinky", -5.0, -26.0, 13.0, 4.0]];
for f in lf {
  let a = f[2] * PI() / 180.0;
  let dx = sin(a); let dy = cos(a); let px = dy; let py = -dx;
  let bx = 226.0 + f[1]; let by = 237.0; let hw = f[4] / 2.0; let L = f[3];
  let mx = bx + dx * L * 0.7; let my = by + dy * L * 0.7; let tx = bx + dx * L; let ty = by + dy * L;
  add_blob("finger_l_" + f[0], [bx - px * hw, by - py * hw, bx + px * hw, by + py * hw, mx + px * hw, my + py * hw, tx + px * hw * 0.55, ty + py * hw * 0.55, tx - px * hw * 0.55, ty - py * hw * 0.55, mx - px * hw, my - py * hw], "#f2cc8f");
}
