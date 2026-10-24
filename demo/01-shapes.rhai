// Pasul 1: piesele omulețului, desenate din spate în față. Totul e vector: dreptunghiuri și cercuri.
add_gradient_rect("sky", 0, 0, 400, 300, "#bfe3f5", "#fdf3d8");
add_rect("ground", 0, 300, 400, 60, "#7fa66b");
// partea dreaptă a personajului (în spatele trunchiului)
add_rect("thigh_r", 183, 200, 14, 50, "#4a4e78");
add_rect("shin_r", 183, 250, 14, 50, "#2f3249");
add_rect("arm_upper_r", 168, 133, 12, 50, "#c9654b");
add_rect("arm_lower_r", 168, 183, 12, 45, "#f2cc8f");
// trunchi și cap
add_rect("torso", 178, 120, 44, 78, "#e07a5f");
add_circle("head", 200, 92, 26, "#f2cc8f");
// partea stângă (în fața trunchiului)
add_rect("thigh_l", 203, 200, 14, 50, "#5f6495");
add_rect("shin_l", 203, 250, 14, 50, "#3d405b");
add_rect("arm_upper_l", 220, 133, 12, 50, "#e07a5f");
add_rect("arm_lower_l", 220, 183, 12, 45, "#f2cc8f");
