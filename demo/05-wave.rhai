// Pasul 5: brațul drept salută. Nu cheiem unghiurile umărului și cotului: un os de control (hand_ctrl)
// e ținta unui lanț IK cu două oase, iar solverul calculează restul.
add_bone("hand_ctrl", "", 10, 174, 228, 0);
add_ik("wave_ik", ["arm_upper_r", "arm_lower_r"], "hand_ctrl", 1);
set_key("hand_ctrl", "bone_offset_x", 0, -30, "ease_in_out");
set_key("hand_ctrl", "bone_offset_y", 0, -105, "ease_in_out");
set_key("hand_ctrl", "bone_offset_x", 6, -48, "ease_in_out");
set_key("hand_ctrl", "bone_offset_y", 6, -118, "ease_in_out");
set_key("hand_ctrl", "bone_offset_x", 12, -30, "ease_in_out");
set_key("hand_ctrl", "bone_offset_y", 12, -105, "ease_in_out");
set_key("hand_ctrl", "bone_offset_x", 18, -48, "ease_in_out");
set_key("hand_ctrl", "bone_offset_y", 18, -118, "ease_in_out");
set_key("hand_ctrl", "bone_offset_x", 24, -30, "ease_in_out");
set_key("hand_ctrl", "bone_offset_y", 24, -105, "ease_in_out");
