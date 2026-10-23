<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio) -->
# Entwicklungsleitfaden

*Für alle, die ARMATURA bauen, ändern, testen und veröffentlichen. Dokumentation unter CC-BY-4.0. Alles hier stammt aus dem Quellbaum, aus `ROADMAP.md` Abschnitt 9, [CI.md](CI.md), [CLEANROOM.md](CLEANROOM.md), [CONTRIBUTING.md](CONTRIBUTING.md) und [LICENSING.md](LICENSING.md); mit „run“ markierte Befehle wurden beim Schreiben dieser Seite ausgeführt (Linux x86-64, rustc 1.97.0, Offline-Registry).*

Inhalt: [Umgebung](#1-umgebung) - [Build- und Testbefehle](#2-build--und-testbefehle) - [Die Testsuiten](#3-die-testsuiten) - [Coding-Konventionen](#4-coding-konventionen) - [Checkliste für ein neues Modul oder einen neuen Befehl](#5-checkliste-für-ein-neues-modul-oder-einen-neuen-befehl) - [Clean-Room-Regeln und DCO](#6-clean-room-regeln-und-dco) - [Release-Checkliste](#7-release-checkliste) - [Continuous Integration](#8-continuous-integration) - [Debugging](#9-debugging) - [Profiling](#10-profiling)

## 1. Umgebung

| Bedarf | Wozu | Hinweise |
|---|---|---|
| Rust (stable, Workspace mit Edition 2021) | alles | entwickelt mit rustc 1.97.0; `Cargo.lock` ist eingecheckt, Builds sind für `--locked` geeignet |
| Ein C-Compiler (`cc`) | das C-ABI-Beispiel bzw. der C-ABI-Test (optional) | siehe [C_ABI.md](C_ABI.md) |
| `ffmpeg` / `ffprobe` im `PATH` | Videoexport, Audiodekodierung von mp3/ogg, `dof-export` nach .mp4/.webm, `soundtrack` | **nie mitgeliefert und zum Bauen nie erforderlich**; Befehle, die es brauchen, weisen darauf hin und schlagen ohne Erfolgsmeldung fehl |
| Node.js >= 18 sowie `playwright-core` + Chromium | die vier Studio-Smoke-Tests; der Test des WASM-Wrappers | Standardwerte: `/opt/npm-tools/node_modules/playwright-core`, `/opt/pw-browsers/chromium-1194/chrome-linux/chrome`; überschreibbar mit `PLAYWRIGHT_CORE`, `CHROME` |
| `cargo-deny` | Richtlinie für Lizenzen, Sperrlisten und Quellen | `scripts/licence-audit.sh` führt es aus, wenn es installiert ist (`--no-deny` überspringt es) |
| `python3` | `scripts/licence-audit.sh` erzeugt Dokumente mit einem Inline-Skript | läuft mit `python3 -I` |
| Ein Vulkan- oder GL-Adapter | nur für echte GPU-Messwerte von `armatura-gpu` | Tests weichen auf einen Software-Adapter aus; siehe Abschnitt 2 |
| `rustup target wasm32-unknown-unknown`, `wasm-opt` | nur für [WASM.md](WASM.md) | **in der Entwicklungsumgebung nie installiert** |

Aufbau des Workspace (zum Design siehe [ARCHITECTURE.md](ARCHITECTURE.md)):

| Crate | Lizenz | Rolle |
|---|---|---|
| `armatura-core` | MPL-2.0 | Engine, Formate, Werkzeuge, die stabile `api`-Fassade ([API.md](API.md)); Features: `scripting` (Standard, Rhai) |
| `armatura-cli` | AGPL-3.0-or-later | die Binärdatei `armatura`, Studio-Server, Collab-Server; Features: `scripting` (Standard), `gpu` |
| `armatura-capi` | MPL-2.0 | C-ABI ([C_ABI.md](C_ABI.md)) |
| `armatura-wasm` | MPL-2.0 | WebAssembly-ABI + JS-Wrapper ([WASM.md](WASM.md)) |
| `armatura-gpu` | MPL-2.0 | optionaler wgpu-Renderer; **kein** Standard-Member |

Dateien auf Repository-Ebene: `ROADMAP.md` (Abschnitt 9 ist das datierte Feature- und Review-Log), `FEATURES.md`, `CLEANROOM.md`, `LICENSING.md`, `REUSE.toml`, `deny.toml`, `LICENSES/`, `scripts/`.

## 2. Build- und Testbefehle

```sh
cargo build                                  # default members: core, cli, capi, wasm (gpu is opt-in)
cargo test --workspace                       # run: 1368 tests + doc-tests (section 3)
cargo test -p armatura-core                  # one crate (the engine and the facade tests)
cargo test -p armatura-core api::            # a module or a name filter
cargo test -p armatura-cli --test e2e        # one integration test file
cargo test --doc -p armatura-core            # doc-tests only (the facade examples)
cargo test -p armatura-cli --test spdx_audit # SPDX header audit (0.01 s)
cargo clippy --workspace --all-targets -- -D warnings   # zero warnings (gated in CI since wave 8)
scripts/licence-audit.sh                     # regenerate THIRD_PARTY*, SPDX check, cargo deny
scripts/e2e.sh                               # the end-to-end pipeline in release mode (--debug for debug)
```

Features und optionale Teile:

```sh
cargo build -p armatura-cli --no-default-features         # no Rhai (no `script` command, no smartstring)
cargo build -p armatura-core --no-default-features        # what capi, wasm and gpu use
cargo build -p armatura-cli --features gpu                # adds `render-gpu` (wgpu)
cargo test  -p armatura-gpu                               # GPU tests (31: 18 unit + 13 parity); NOT in the workspace default
ARMATURA_GPU_BACKEND=vulkan armatura render-gpu ...     # restrict the wgpu backend: vulkan or gl
```

Faustregeln: Verwende Debug-Builds für die Testsuiten (das Repository wurde mit Blick auf den Speicherplatz entwickelt: kopiere `target/` nicht); nutze `--release` nur bei Fragen zur Laufzeit. Der Fassaden-Snapshot wird mit `ARMATURA_BLESS=1 cargo test -p armatura-core api::` freigegeben (Abschnitt 5). `ARMATURA_UI_CONFIG` wählt in Tests die UI-Einstellungsdatei des Studios, `ARMATURA_AUTHOR` den Autorennamen bei Versionskontroll-Commits, und `ARMATURA_E2E_TIMINGS=<file>` bewirkt, dass der E2E-Test seine Zeiten pro Stufe schreibt.

## 3. Die Testsuiten

Die Zahlen stammen aus `ROADMAP.md` Abschnitt 9 und aus einem vollständigen Lauf von `cargo test --workspace --offline` am 22. Okt. 2026 für diese Seite (Debug-Profil, eine mit anderen Jobs geteilte Maschine; die Zeiten sind daher nur Richtwerte und **auf einer unbelasteten Maschine schneller**):

| Suite | Tests | Zeit (dieser Lauf) | Was sie abdeckt |
|---|---:|---:|---|
| `armatura-core` Unit (`src/lib.rs`) | 1167 | 349 s | jedes Modul; enthält `api::tests::api_surface_snapshot` |
| `armatura-core/tests/prelude_surface.rs` | 6 | 0.7 s | die Fassade von außerhalb des Crates (`#[non_exhaustive]`, Fehlerabbildung, eine repräsentative Auswahl an Aufrufen) |
| `armatura-cli` Unit (`src/main.rs`, Studio, Collab, http) | 80 | 42 s | Optionsparsing, USAGE gegenüber Parser, HTTP-Server, Richtlinie, Collab-Log |
| `tests/e2e.rs` | 2 | 214 s | die gesamte Pipeline über die echte Binärdatei (laut ROADMAP etwa 19 s in einem unbelasteten Debug-Build und 40 s im Release; hier war die Maschine ausgelastet) |
| `tests/fixes.rs`, `tests/wave4.rs`, `tests/robust.rs` | 7, 15, 10 | 19 s, 1.2 s, 1.2 s | Regressionen der Review-Wellen und Tests mit feindlichen Eingaben |
| `tests/doc_mismatch.rs` | 20 | 1.8 s | Regressionen zwischen Dokumentation und Verhalten: atomare Ausgaben, strikte Optionen, die Akzeptanzmatrix für `--json`/`--quiet`/`--allow-empty`, Skalierungsregeln von `golden` |
| `tests/source_offer.rs` | 3 | 0.2 s | AGPL-Abschnitt-13-Angebot unter `/source` |
| `tests/face_cli.rs` | 1 | 0.8 s | Face-Tracking-CLI |
| `armatura-core/tests/wasm_safety.rs` | 2 | 0.1 s | kein direktes `Instant` / `SystemTime` / `process::id` / `thread::spawn` außerhalb von `clock.rs` (Quelltext-Scan) |
| `armatura-cli/tests/wave7.rs` | 7 | 7 s | Welle 7: Optionen pro Unterbefehl, `follow` / `make-loop`, `color-space`, `collab-token`, Secret-Datei des Studios, Frame-Bereich von `dof` / `render-hdr` / `fx` |
| `tests/spdx_audit.rs` | 2 | 0.01 s | jede `.rs/.js/.mjs/.sh`-Datei hat eine SPDX-Zeile, die der Lizenz ihres Crates entspricht; REUSE.toml deckt den Rest ab |
| `armatura-capi/tests/abi.rs` | 7 | 1.1 s | die C-ABI von der Rust-Seite aus (C-Smoke-Test: `armatura-capi/tests/smoke.c`, von Hand kompiliert, siehe [C_ABI.md](C_ABI.md)) |
| `armatura-wasm` Unit | 8 | 1.6 s | die ABI-Funktionen auf dem Host |
| `armatura-gpu` Unit + `tests/parity.rs` | 18 + 13 | 0.2 s + 6.5 s | Parität GPU gegenüber CPU (Software-Adapter, wenn keine GPU vorhanden ist) |
| Doc-Tests (`armatura-core`) | 19 | 171 s (jeder kompiliert ein Programm) | die Fassaden-Beispiele (`api.rs`, `lib.rs`); `armatura-capi` und `armatura-wasm` haben keine |

Gesamt: 1368 Tests in den Unit- und Integrationstest-Binärdateien (die Zahl aus der ROADMAP; nachgezählt anhand eines vollständigen Laufs von `cargo test --workspace` am 22. Okt. 2026 nach Welle 7, die Zeiten pro Suite oben stammen aus einem früheren Lauf), dazu 19 Doc-Tests; das gesamte `cargo test --workspace --offline` dauerte auf der geteilten Maschine 836 s.

Suiten außerhalb von `cargo test`:

* **Studio-Smoke-Tests (Chromium).** Vier Node-Skripte steuern das echte Studio in Headless-Chromium mit `playwright-core`: `studio_ui_smoke.js` (124 Prüfungen), `studio_collab_smoke.js` (60, zwei Seiten auf einer Maschine), `studio_overlays_smoke.js` (89), `studio_suggest_smoke.js` (35); alle vier wurden in Welle 8 ausgeführt und bestanden (Zahlen so, wie die Skripte sie ausgeben). Sie werden **nicht** von `cargo test` ausgeführt und sind standardmäßig nicht Teil der CI. Ausführen:

  ```sh
  cargo build -p armatura-cli
  node armatura-cli/tests/studio_ui_smoke.js        # same for collab / overlays / suggest
  ```

  Umgebung: `ARMATURA_BIN` (Standard `target/debug/armatura`), `PLAYWRIGHT_CORE`, `CHROME` und für das Overlay-Skript `SHOTS` (Verzeichnis für Screenshots). Die Skripte räumen ihre temporären Dateien auf.
* **Test des WASM-Wrappers.** `node armatura-wasm/js/test.mjs` (nur Mock, es sei denn, du übergibst einen Projektpfad und hast das Modul gebaut; siehe [WASM.md](WASM.md)).
* **Lizenzaudit.** `scripts/licence-audit.sh`: baut `docs/THIRD_PARTY.md` und `docs/THIRD_PARTY_NOTICES.md` aus `cargo metadata --locked --all-features` neu (normale und Build-Abhängigkeiten, Dev-Abhängigkeiten ausgenommen), prüft die SPDX-Zeile jeder Quelldatei gegen die `license` ihres Crates und führt dann `cargo deny check licenses bans sources` aus, sofern nicht `--no-deny` gesetzt ist. `deny.toml` erlaubt permissive Lizenzen und MPL-2.0 nur für namentlich genannte Crates.
* **clippy.** `cargo clippy --workspace --all-targets -- -D warnings` endet mit Exit-Code 0 ohne Warnungen (Welle 8) und ist in der CI ein Gate. Wo ein Lint für den Fall falsch ist, trägt der Code ein eng gefasstes `#[allow(clippy::...)]` mit Kommentar (Index-Schleifen über parallele Arrays, `too_many_arguments` bei stabilen Signaturen, `manual_clamp` dort, wo die NaN-Behandlung abweicht, inhärente Namen `next` / `mul`, die öffentliche API sind); füge keine Allows auf Crate-Ebene hinzu.
* **CI-Anbindung.** Das Repository liefert `armatura ci` für *Animations*-Prüfungen ([CI.md](CI.md)); das ist etwas anderes als die CI des Codes. Das Projekt hat keine Code-CI-Konfiguration im Baum; Abschnitt 8 gibt ein minimales Rezept.

Tests hinzufügen: Lege Engine-Tests neben den Code (`#[cfg(test)]`), Verhaltenstests der CLI in `armatura-cli/tests/`, und jeder behobene Fehler bekommt einen Regressionstest (die Review-Wellen in [CHANGELOG.md](CHANGELOG.md) listen auf, was wo behoben wurde). Tests dürfen keine temporären Dateien hinterlassen (nutze Drop-Guards) und dürfen weder von der Uhr noch vom Netzwerk abhängen. Zeitbudgets werden mit einer deterministischen Testuhr getestet.

## 4. Coding-Konventionen

* **Keine Panics bei Eingaben.** Alles, was ein Projekt, eine Datei, ein Argument oder ein Netzwerkbyte liest, gibt einen Fehler zurück. Indizierung, Slicing, `unwrap` und arithmetischer Überlauf bei nicht vertrauenswürdigen Werten sind Fehler. Die Fassade (`api.rs`) fängt zusätzlich Panics ab und gibt `ApiError::Internal` zurück (nur dort, wo das Ziel unwindet), aber das ist die letzte Verteidigungslinie, kein Design.
* **Richtlinie für endliche Werte.** Zahlen aus Dateien und Argumenten werden auf NaN und Unendlich sowie auf vernünftige Größenordnungen geprüft (die Bereiche der Fassade stehen in [API.md](API.md): Beträge bis 1e7, fps in (0, 1000], Frames 1..=200000, Bühne bis 32768, 4096 Knochen, Renderseite bis 16384 px). Lehne ab, statt zu begrenzen, es sei denn, die Funktion ist als begrenzend dokumentiert (Frames werden auf die Timeline begrenzt).
* **Größen prüfen, bevor allokiert wird.** Pixelpuffer, Mesh-Größen und Sample-Anzahlen werden mit geprüfter Arithmetik berechnet und vor `vec![..]` abgelehnt.
* **Atomare Schreibvorgänge.** Schreibe Ergebnisse mit `fsutil::write_atomic` (temporäre Datei im selben Verzeichnis, `fsync`, Umbenennen; verweigert symbolische Links, außer bei `--follow-symlinks`; fällt auf ein direktes, per `fsync` gesichertes Überschreiben zurück, dokumentiert als nicht atomar, wenn das Verzeichnis nicht beschreibbar ist). Nie `std::fs::write` direkt auf eine Datei des Benutzers. Temporäre Verzeichnisse kommen aus `fsutil` (Modus 0700, Dateien 0600). Zufallsbytes kommen von `fsutil::random_bytes` (`/dev/urandom`).
* **Assets laufen über `assets::resolve`.** Relative Pfade von Bildern, Normal-Maps und Sounds werden gegen das prozessweite Asset-Verzeichnis aufgelöst (`assets::default_dir`, gesetzt aus dem Ordner des Eingabeprojekts); absolute Pfade und `..` werden abgelehnt, außer bei `--allow-external-assets`; Bilddateien sind auf 64 MB begrenzt, Audio auf 256 MB. Befehle, die ein abgeleitetes Projekt schreiben, validieren gegen das Verzeichnis des *Eingabe*projekts und schreiben relative Asset-Pfade um oder warnen davor (`write_json`).
* **Deterministischer Zufall.** Jedes zufällige Verhalten (Fuzzing, Rauschen, Jitter, Simulation) nutzt den eigenen Seed-Generator der Engine mit einer Option `--seed` und ist bitgenau reproduzierbar. Kein `rand`-Crate, keine Systemzeit als Seed.
* **Println und geschlossene Pipes.** Die CLI überlagert `println!`/`print!` mit Makros, die bei einer geschlossenen Pipe still beenden (`armatura ... | head` darf nicht in Panik geraten). Verwende sie in `main.rs` weiterhin.
* **Fehler sagen, was zu tun ist.** Meldungen nennen, was falsch war, welcher Wert, und den erlaubten Bereich oder die Abhilfe (die Tests der Fassade prüfen genau diesen Stil).
<!-- REUSE-IgnoreStart -->
* **SPDX-Header.** Jede `.rs`-, `.js`-, `.mjs`- und `.sh`-Datei beginnt mit `SPDX-License-Identifier:` entsprechend der Lizenz ihres Crates (MPL-2.0 für core/capi/wasm/gpu/scripts, AGPL-3.0-or-later für die CLI) und einer Copyright-Zeile; Markdown-Dokumente tragen den Kommentar-Header für CC-BY-4.0, so wie diese Datei. Andere Dateien (JSON, Bilder, die Lizenztexte) deckt `REUSE.toml` ab; füge ein neues Pfadmuster dort ein, wenn du eines hinzufügst. Sonst schlägt `cargo test -p armatura-cli --test spdx_audit` fehl.
<!-- REUSE-IgnoreEnd -->
* **Abhängigkeiten.** Wenige, reines Rust, permissive Lizenz. Ein neues Crate braucht eine Prüfung durch `deny.toml` (`scripts/licence-audit.sh`), eine Zeile in `docs/THIRD_PARTY.md` (neu erzeugt) und eine Begründung im ROADMAP-Eintrag. `armatura-core` wird von capi, wasm, gpu und der CLI ohne Standard-Features gebaut, also gehört alles Schwere hinter ein Feature.
* **Zuerst die Fassade.** Eine neue für Benutzer sichtbare Fähigkeit der Engine, die andere Sprachen erreichen sollen, gehört in `armatura-core/src/api.rs` mit einer Snapshot-Zeile, einem Doc-Test und einem Unit-Test; die C- und wasm-Schichten wrappen nur die Fassade.

## 5. Checkliste für ein neues Modul oder einen neuen Befehl

1. **Modul** in `armatura-core/src/<name>.rs`, in `lib.rs` registriert; SPDX-Header; Unit-Tests (gültige, Grenz- und feindliche Eingaben, Determinismus); keine Panics.
2. **CLI-Befehl** in `armatura-cli/src/main.rs`: ein `USAGE`-Eintrag (exakte Schreibweise jeder Option), der Dispatch-Zweig und die Optionstabellen: Jede Option mit Wert muss in `VALUE_OPTS` stehen; Flags ohne Wert in einem `CMD_OPTS`-Eintrag des Befehls, wo die globale Tabelle falsch wäre (zum Beispiel ist `ci --prune` ein Flag, `export-oca --layers` und `autotrace --png` nehmen Werte, `keyedit --free` ist ein Flag). Die Unit-Tests USAGE gegenüber Parser schlagen fehl, wenn Text und Parser nicht übereinstimmen. Unterstütze die globalen Flags (`--json`, `--quiet`, `--allow-empty`, `--allow-external-assets`, `--follow-symlinks`, `--max-input-bytes`), `<command> --help` und `--key=value`.
3. **Ausgabe schreiben** nur mit `fsutil::write_atomic` oder `write_json`; Befehle, die nichts erzeugen, enden mit einem Exit-Code ungleich null, außer bei `--allow-empty`.
4. **Dokumentation**: der Befehl in [CLI_REFERENCE.md](CLI_REFERENCE.md), wenn du einen hinzufügst, ein Hinweis in einem Leitfaden, wo es passt, und der Eintrag in `ROADMAP.md` Abschnitt 9 (was hinzugefügt wurde, was *nicht* erledigt ist: Diese Liste speist [LIMITATIONS.md](LIMITATIONS.md)).
5. **Zeile in CLEANROOM.md**: Halte für jeden neuen Algorithmus die öffentliche Quelle fest, nach der er implementiert wurde (Lehrbuch, Standard, Paper), sowie die unabhängige Herleitung; keine Zeile, kein Merge.
6. **Fassadenoberfläche neu freigeben**, wenn sich `api.rs` geändert hat: Führe `ARMATURA_BLESS=1 cargo test -p armatura-core api::` aus, prüfe den Diff von `armatura-core/src/api_surface.txt` (eine entfernte oder geänderte Zeile ist ein Breaking Change und erfordert eine Major-Version der Fassade; eine hinzugefügte Zeile ist ein Minor), und führe die Tests dann erneut ohne die Variable aus. Aktualisiere [API.md](API.md) sowie die C-/WASM-Wrapper und ihre Header, wenn sie es bereitstellen sollen.
7. **End-to-End**: Nimm den Befehl in die E2E-Pipeline auf, wenn er zum Produktionspfad gehört; füge den Studio-Skripten eine Smoke-Prüfung hinzu, wenn er eine UI hat.
8. **Ausführen**: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `scripts/licence-audit.sh` (wenn sich Abhängigkeiten oder Header geändert haben).

## 6. Clean-Room-Regeln und DCO

Das Projekt ist eine unabhängige Implementierung. Kurz gefasst (verbindlich sind [CLEANROOM.md](CLEANROOM.md) und [CONTRIBUTING.md](CONTRIBUTING.md)):

* Implementiere nach öffentlichen Standards, Lehrbüchern und Papers und zitiere sie. Lies, dekompiliere, kopiere oder Reverse-Engineere keinen Code, keine Dateien und keine Schnittstellen proprietärer Animationswerkzeuge, und lies oder füge keinen Quellcode konkurrierender Werkzeuge ein, während du ein Feature schreibst. Füge keinen Code aus Projekten mit inkompatibler Lizenz ein.
* Halte jede neue Methode in der Tabelle von `CLEANROOM.md` mit ihrer öffentlichen Quelle fest.
* Patenthinweise stehen in [PATENT_NOTES.md](PATENT_NOTES.md); ein Feature, das einen patentsensiblen Bereich berührt, wird zuerst dort besprochen. Das ist keine Rechtsberatung.
* Beiträge werden unter der Lizenz des Teils angenommen, den sie ändern (inbound gleich outbound): MPL-2.0 für die Bibliotheken und Skripte, AGPL-3.0-or-later für `armatura-cli`, CC-BY-4.0 für Dokumentation und synthetische Assets.
* **Sign-off.** Jeder Commit braucht `Signed-off-by: Name <email>` (`git commit -s`), womit das Developer Certificate of Origin 1.1 bescheinigt wird (Text in CONTRIBUTING.md). Das Sign-off bescheinigt nur die Herkunft: Es gewährt **kein** Recht zur Neulizenzierung, und solange es kein CLA gibt, ist keine kommerzielle oder Dual-Lizenz für beigetragenen Code möglich.
* Name und Logo sind durch [TRADEMARK.md](TRADEMARK.md) geregelt.

## 7. Release-Checkliste

Bisher wurde nichts veröffentlicht (alle Crates stehen auf 0.1.0; die Versionsbezeichnungen „v0.5/v0.6/v0.7“ in README, FEATURES und ROADMAP sind Feature-Stufen, keine Crate-Versionen; siehe [CHANGELOG.md](CHANGELOG.md)). Wenn es so weit ist:

1. **Version.** Lege die Version jedes Crates fest. Die Fassade hat ihre eigene Version (`API_VERSION`, heute 1.4.0: Major bei Entfernung oder Änderung einer Oberflächenzeile, Minor bei Ergänzung), und `ARMATURA_ABI_VERSION` / `arm_abi_version()` (beide 1) ändern sich nur bei einem Breaking Change der C- oder wasm-ABI. Gleiche die `Cargo.toml`-Versionen, die Ausgabe von `armatura --version`, die Statusbox im README und das CHANGELOG ab.
2. **Grüne Gates.** `cargo test --workspace`, `cargo test --doc -p armatura-core`, `cargo test -p armatura-cli --test spdx_audit`, `cargo clippy --workspace --all-targets -- -D warnings`, die vier Chromium-Smoke-Tests, `scripts/e2e.sh` (Release), `cargo test -p armatura-gpu`, wenn sich der GPU-Code geändert hat.
3. **Hinweise zu Drittanbietern.** Führe `scripts/licence-audit.sh` aus: Es erzeugt `docs/THIRD_PARTY.md` (Inventar) und `docs/THIRD_PARTY_NOTICES.md` (Lizenztexte von allem, was ausgeliefert wird) neu, prüft die SPDX-Zeilen und führt `cargo deny` aus. Committe die neu erzeugten Dateien. Eine Binärdistribution muss das Notices-Bundle neben der Binärdatei mitliefern.
4. **Lizenzdateien.** `LICENSE`, `LICENSES/` (Texte jeder verwendeten Lizenz) und `REUSE.toml` stimmen mit [LICENSING.md](LICENSING.md) überein; die CC-BY-4.0-Namensnennung für die Dokumentation ist intakt.
5. **Kein ffmpeg.** Liefere ffmpeg in Release-Artefakten weder mit, noch linke oder lade es. Es wird zur Laufzeit im `PATH` erkannt, und die Dokumentation sagt, dass der Benutzer es selbst installiert.
6. **AGPL-Quellcode-Angebot.** Die Binärdatei von `armatura-cli` liefert ihr eigenes Quellcode-Angebot unter `/source` aus (AGPL Abschnitt 13) und gibt mit `--version` einen Hinweis aus; Betreiber modifizierter Studio-Server setzen `--source-url` auf den Ort, an dem ihr modifizierter Quellcode liegt. Tests: `tests/source_offer.rs`. Stelle bei einem Binär-Release den zugehörigen Quellcode (Tag, Tarball) unter der URL bereit, die Benutzer erreichen können.
7. **Dokumentation.** Aktualisiere die Statusbox im README, [LIMITATIONS.md](LIMITATIONS.md) und das CHANGELOG (verschiebe *Unreleased* unter die neue Version mit Datum).
8. **Taggen und veröffentlichen** mit einem signierten Tag; hänge Prüfsummen an. Veröffentliche keine Crates unter Namen, die dir nicht gehören; für Forks gelten die Regeln aus TRADEMARK.md.

## 8. Continuous Integration

Zwei verschiedene Dinge teilen sich das Wort:

* **Animations-CI** (`armatura ci`): Prüfungen von *Projekten* (Lint, Render-Goldens, Performance, Determinismus). Vollständig beschrieben in [CI.md](CI.md): Manifest `armatura-ci.json`, Schwellenwerte, Goldens (`--update` gegenüber `bless`), Berichte, GitHub-Actions- und reine Shell-Beispiele (dort als „example, not run“ markiert).
* **Code-CI** für dieses Repository: `.github/workflows/ci.yml` (in Welle 7 hinzugefügt) hat drei Jobs: `test` (Matrix ubuntu / windows / macos: `cargo test --workspace --locked` und `cargo clippy --workspace --all-targets --locked -- -D warnings`), `wasm32` (installiert das Target `wasm32-unknown-unknown`, baut `armatura-wasm`, kopiert das Modul neben `armatura.js` und führt `node armatura-wasm/js/test.mjs` aus, das dann auch seinen Teil mit dem echten Modul ausführt) und `licences` (`cargo-deny check` und `reuse lint`). **Der Workflow wurde noch nie ausgeführt**: Die Entwicklungsumgebung hat kein GitHub Actions, kein Windows oder macOS und kann das wasm32-Target nicht installieren; rechne also damit, beim ersten Lauf Kleinigkeiten beheben zu müssen (Action-Versionen, Runner-Images, ein fehlschlagender Test unter Windows oder macOS, der erste echte wasm-Build). Der Workflow führt `cargo test --doc` nicht separat aus (`cargo test --workspace` enthält Doc-Tests), ebenso wenig das End-to-End-Skript, die Chromium-Smoke-Tests oder das GPU-Crate. Die Befehle, aus denen er aufgebaut ist, so wie sie beim Schreiben dieser Dokumentation ausgeführt wurden:

  ```sh
  cargo test --workspace --locked
  cargo test --doc -p armatura-core --locked
  cargo clippy --workspace --all-targets --locked -- -D warnings
  scripts/licence-audit.sh            # needs python3 and (optional) cargo-deny
  ```

  Cache `~/.cargo/registry` und `target/` zwischen den Läufen, gib dem E2E-Test in Debug-Builds mehrere Minuten, und führe die Chromium-Smoke-Tests nur auf Runnern aus, die Chromium und `playwright-core` haben.

## 9. Debugging

* **Backtraces.** `RUST_BACKTRACE=1 armatura ...` bei jedem Panic (ein Panic bei einer Eingabe ist ein Fehler; melde ihn bitte mit dem Projekt, das ihn auslöst).
* **Maschinenlesbare Ausgabe.** `--json` bei Befehlen, die es unterstützen; `--quiet` für Skripte. Exit-Codes: 0 bei Erfolg, 1 bei Fehler (fehlgeschlagene Prüfung, verfehltes Budget, falsche Verwendung; run: ein unbekannter Befehl und ein fehlendes Argument enden beide mit 1); `armatura ci` trennt 1 (eine Prüfung ist fehlgeschlagen) von 2 (Fehler).
* **Ein Projekt untersuchen.** `armatura check`, `armatura lint` (Exit 1 bei Fehlern) und `armatura diff a.json b.json` liefern Struktur und Probleme ohne Rendern; `armatura render project.json N out.png` rendert einen Frame, und `out.svg` ergibt Vektorausgabe, die du im Browser öffnen kannst (`--bones` zeigt das Skelett).
* **Studio.** Starte `armatura studio project.json --port 0` (gibt den tatsächlichen Port aus). Die Entwicklerwerkzeuge des Browsers zeigen die JSON-API: Anfragen sind einfache `POST /api/...` mit `Content-Type: application/json`. Ein 421 bedeutet einen Host-Header, der nicht in `--allow-host` steht; ein 403 bedeutet, dass eine Prüfung von Origin oder `Sec-Fetch-Site` fehlgeschlagen ist (nutze `--public-origin` hinter einem Proxy); 415 betrifft den Content-Type; 429 ist das Token-Back-off.
* **Collab-Log.** Das Log einer geteilten Sitzung (`--collab-log`) ist Text, eine Zeile `<hash> <json>` pro Operation; eine unterbrochene Kette wird beim Laden gemeldet.
* **Ein Test schlägt nur manchmal fehl.** Suche nach der Uhr, Namen temporärer Verzeichnisse und der Iterationsreihenfolge von `HashMap`; Engine-Code ist per Konstruktion deterministisch, sodass sporadische Fehler fast immer auf mangelnde Testhygiene zurückgehen.
* **Fassaden-Snapshot schlägt fehl.** Du hast eine öffentliche Signatur geändert: siehe Abschnitt 5, Punkt 6.
* **GPU.** `armatura render-gpu <project> <out.png> --adapter software|auto --bench N` (nur in Builds mit `--features gpu`) vergleicht mit dem CPU-Renderer (ein Software-Adapter gibt aus, dass seine Zeiten nichts bedeuten); `ARMATURA_GPU_BACKEND` wählt das Backend.
* **Sanitizer für die C-ABI.** Kompiliere `smoke.c` oder deinen eigenen Aufrufer mit `-fsanitize=address,undefined` gegen die statische Bibliothek (während der Dokumentationsarbeit erledigt, sauber; siehe [C_ABI.md](C_ABI.md)).

## 10. Profiling

* **Zeiten pro Stufe der Produktionspipeline:** `scripts/e2e.sh` gibt eine Tabelle aus (`ARMATURA_E2E_TIMINGS` benennt die Datei); die Release-Zahlen in `ROADMAP.md` Abschnitt 9 stammen daher.
* **GPU gegenüber CPU:** `armatura render-gpu <project> <out.png> --bench N`.
* **Budget für Größe und Geschwindigkeit:** `armatura size-report`, `armatura export-fit --max-seconds` (das Zeitbudget wird innerhalb der Kandidatenschleifen geprüft; ein einzelner Aufruf von ffmpeg oder dem Quantisierer ist nicht unterbrechbar).
* **Renderkosten eines Projekts:** `armatura ci` mit einem Performance-Schwellenwert ([CI.md](CI.md)) erfasst die Zeit pro Frame auf deiner Maschine; Zeiten hängen von der Maschine ab, vergleiche also auf derselben Maschine.
* **Allgemeine Werkzeuge:** Baue mit `--release` und nutze `perf record`/`perf report` oder `cargo flamegraph` (nicht Teil des Projekts; separat installieren). Debug-Builds sind etwa eine Größenordnung langsamer und sagen nichts über die echte Geschwindigkeit.
