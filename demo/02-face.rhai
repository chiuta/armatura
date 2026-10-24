// Pasul 2: fața. Ochii sunt cercuri; gura are cinci forme, grupate într-un strat „switch”
// (ordinea cerută de comenzile de lip-sync: repaus, mică, deschisă, rotundă, largă).
add_circle("eye_l", 210, 88, 3.4, "#3d405b");
add_circle("eye_r", 190, 88, 3.4, "#3d405b");
add_line("m_rest", [190, 104, 200, 106, 210, 104], "#3d405b", 2, false);
add_circle("m_small", 200, 105, 3.2, "#3d405b");
add_circle("m_open", 200, 106, 7.5, "#3d405b");
add_circle("m_round", 200, 105, 5, "#7a2e2e");
add_polygon("m_wide", [188, 102, 200, 105, 212, 102, 207, 111, 200, 113, 193, 111], "#3d405b");
add_switch("mouth", ["m_rest", "m_small", "m_open", "m_round", "m_wide"]);
