# Demo: „Buddy”, pas cu pas

Un personaj complet (cap cu față, trunchi, brațe, picioare, gură cu cinci forme), construit din fișier gol până la export
cu **comenzile reale** ale lui `armatura`. Același omuleț apare în capturile din `docs/img/` (mers, schelet, studio).

| Pas | Comandă | Fișier |
|---|---|---|
| 1 proiect gol | `armatura check buddy.json` | `00-buddy.json` |
| 2 forme | `armatura script … shapes.rhai` | `01-shapes.rhai` |
| 3 față și gură | `armatura script … face.rhai` | `02-face.rhai` |
| 4 schelet (11 oase) | `armatura script … bones.rhai`, `armatura render … --bones` | `03-bones.rhai` |
| 5 mers generat | `armatura walk …` | (generator procedural) |
| 6 salut cu IK | `armatura script … wave.rhai` | `05-wave.rhai` |
| 7 buclă | `armatura loop-solve …` | |
| 8 gura | `armatura script … mouth.rhai` | `06-mouth.rhai` |
| 9 verificări | `armatura lint`, `armatura stats` | |
| 10 export | `armatura export … .gif / .html / .json` | |

Buclă înainte de gură: `loop-solve` amestecă cheile și strică valorile întregi ale unui strat `switch`, așa că gura se adaugă după.

## Regenerare

```
cargo build --release -p armatura-cli
node demo/build-demo.mjs                 # rulează comenzile reale; scrie demo/out/*.json
node demo/inject-studio.mjs              # turul din Studio  → armatura-cli/src/studio.html
node demo/inject-page.mjs --page ../armatura-lp/index.html   # pagina de prezentare (ramura landing-page)
```

`build-demo.mjs` nu conține nimic scris de mână în ce se arată: ieșirile comenzilor și SVG-urile sunt cele ale motorului.
`demo/out/demo-*.json` sunt generate (ignorate de git); `studio.html` conține turul gata injectat.

## În Studio

Butonul **Demo** (sau comanda „Guided demo” din paletă) deschide panoul. Fiecare pas încarcă în editor proiectul produs de comanda
respectivă, prin `POST /api/op` (`set` pe calea `""`, un singur pas de Undo). Nimic nu se scrie pe disc; **Save** e dezactivat cât timp
demo-ul e încărcat, iar **Restore my project** readuce proiectul care era deschis.
