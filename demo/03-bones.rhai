// Pasul 3: scheletul, 11 oase. Unghiul 0 arată spre +x, iar unghiurile pozitive merg în sensul acelor de ceasornic.
// Rădăcina „hips” arată în sus (-90); fiecare os copil își măsoară poziția și unghiul în spațiul părintelui.
add_bone("hips", "", 10, 200, 205, -90);
add_bone("torso", "hips", 75, 10, 0, 0);
add_bone("head", "torso", 40, 75, 0, 0);
add_bone("arm_upper_l", "torso", 50, 62, 26, 180);
add_bone("arm_lower_l", "arm_upper_l", 45, 50, 0, 0);
add_bone("arm_upper_r", "torso", 50, 62, -26, 180);
add_bone("arm_lower_r", "arm_upper_r", 45, 50, 0, 0);
add_bone("thigh_l", "hips", 50, 5, 10, 180);
add_bone("shin_l", "thigh_l", 50, 50, 0, 0);
add_bone("thigh_r", "hips", 50, 5, -10, 180);
add_bone("shin_r", "thigh_r", 50, 50, 0, 0);
// fiecare piesă „călărește” osul ei, rigid
set_layer("torso", "bone", "torso");
set_layer("head", "bone", "head");
set_layer("eye_l", "bone", "head");
set_layer("eye_r", "bone", "head");
set_layer("mouth", "bone", "head");
set_layer("arm_upper_l", "bone", "arm_upper_l");
set_layer("arm_lower_l", "bone", "arm_lower_l");
set_layer("arm_upper_r", "bone", "arm_upper_r");
set_layer("arm_lower_r", "bone", "arm_lower_r");
set_layer("thigh_l", "bone", "thigh_l");
set_layer("shin_l", "bone", "shin_l");
set_layer("thigh_r", "bone", "thigh_r");
set_layer("shin_r", "bone", "shin_r");
// laba piciorului și mâna stângă urmează rigid osul de care sunt lipite
set_layer("foot_l", "bone", "shin_l"); set_layer("toe_l", "bone", "shin_l");
set_layer("foot_r", "bone", "shin_r"); set_layer("toe_r", "bone", "shin_r");
set_layer("palm_l", "bone", "arm_lower_l");
for n in ["thumb", "index", "middle", "ring", "pinky"] { set_layer("finger_l_" + n, "bone", "arm_lower_l"); }
