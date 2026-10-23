<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio) -->
# Listă de verificare pentru lansare (partea proprietarului)

*Pași pe care îi poate face doar proprietarul, în ordine. Nimic de aici nu a fost executat de uneltele proiectului: mediul de build nu a avut acces la GitHub, nici țintă wasm32, nici GPU real, nici avocat. Nu constituie consultanță juridică. Elementele marcate cu **[blocker]** trebuie făcute înainte de un anunț public.*

## 1. Înainte de a publica orice

- [ ] **[blocker]** Confirmă sau înlocuiește contactul de securitate (`alexio@trom.tf` este un substituent în `docs/SECURITY.md`, în `--version` și în documentele de licență).
- [ ] **[blocker]** Caută numele „ARMATURA” în registrele de mărci care te interesează (EUIPO, USPTO, OSIM) și în magazinele de aplicații / registrele de pachete. Vezi `TRADEMARK.md`. Dacă apare o suprapunere, redenumește înainte de prima lansare (numele apare în binar, crate-uri, documentație).
- [ ] **[blocker]** Cere unui avocat să verifice `LICENSING.md`, împărțirea MPL-2.0 / AGPL-3.0, `docs/THIRD_PARTY.md` și `docs/PATENT_NOTES.md`. Decide dacă vrei licențiere duală; dacă da, este nevoie de un CLA *înainte* de a accepta contribuții din exterior (în prezent: doar DCO).
- [ ] Verifică termenii referinței Graphics Gems pentru potrivirea curbelor (citată din memorie în `CLEANROOM.md`); păstrează formularea despre implementare independentă doar dacă este corectă.
- [ ] Rulează `reuse lint` și `cargo deny check` pe mașina ta (proiectul are doar propriul test `spdx_audit`).
- [ ] Verifică că în arbore să nu existe date personale, căi locale sau token-uri (`grep -r "/home/" .`, `grep -ri token`) și că modelul inclus și resursele demo să fie toate generate de tine.

## 2. Depozit

- [ ] Creează depozitul GitHub (sau Codeberg / Forgejo propriu); încarcă arborele **fără** `target/` și `out/`.
- [ ] Păstrează `LICENSE`, `LICENSES/`, `REUSE.toml`, `.github/FUNDING.yml` (Buy Me a Coffee `echo.of.the.strings`, Patreon `alexio_tf`).
- [ ] Activează raportarea privată a vulnerabilităților (fila Security) și trimite către ea din `docs/SECURITY.md`.
- [ ] Transmite URL-ul depozitului cu `--source-url` la rularea studioului (oferta de cod sursă AGPL, ruta `/source`) și testează oferta pe o instalare nouă.

## 3. Prima rulare CI (nerulată până acum)

- [ ] Fă push și urmărește `.github/workflows/ci.yml`: testele pe Linux, Windows, macOS și `clippy -D warnings`; jobul de build wasm32; jobul cargo-deny și REUSE.
- [ ] Așteaptă-te la eșecuri la prima rulare pe Windows/macOS (căi, permisiuni, diferențe de pixeli la marginile anti-aliasate, L-04 / L-26). Repară sau relaxează cu toleranțe, apoi consemnează rezultatul în `docs/LIMITATIONS.md`.
- [ ] Dacă jobul wasm32 trece, rulează lista de verificare din `docs/WASM.md` într-un browser real și actualizează L-01 / L-02.

## 4. Testare cu oameni

- [ ] Dă studioul la 3–5 animatori pentru o săptămână, cu o sarcină scurtă (rig pentru un personaj, un ciclu de mers, export GIF). Notează unde se blochează; deschide câte un issue pentru fiecare.
- [ ] Testează pe browsere reale (Chrome, Firefox, Safari), un dispozitiv tactil și o sesiune doar cu tastatura.
- [ ] Încearcă funcțiile de detector de fețe / mocap / voce cu date reale; raportează sincer că sunt limitate (L-12, L-13).

## 5. Siguranța implementării (dacă găzduiești studioul sau colaborarea)

- [ ] Pune-l în spatele unui reverse proxy care termină TLS (rețeta: `docs/SECURITY.md` secțiunea 12); nu expune niciodată portul HTTP simplu pe internet.
- [ ] Folosește token-uri pe participant (`collab-token`), setează expirarea, leagă la localhost în spatele proxy-ului.
- [ ] Comandă o verificare de securitate independentă înainte de a oferi colaborare găzduită unor necunoscuți.

## 6. Artefacte de lansare

- [ ] Etichetează o versiune, actualizează `CHANGELOG.md`, construiește binarele de lansare pe mașinile tale (mediul proiectului nu a construit niciodată `--release`), publică sumele de control.
- [ ] Atașează codul sursă corespunzător pentru părțile AGPL (eticheta în sine este suficientă; păstrează-l disponibil cât timp distribui binare).
- [ ] Livrează `docs/THIRD_PARTY_NOTICES.md` (regenerează cu `scripts/licence-audit.sh`) împreună cu fiecare binar.
- [ ] Crates.io / npm / registre de pachete: verifică dacă numele sunt libere și dacă câmpurile de licență corespund cu `LICENSING.md`.

## 7. Anunț și donații

- [ ] Primul ecran din README: ce funcționează, ce nu (leagă `docs/LIMITATIONS.md`); nu pretinde „clasa Moho” fără rezerve.
- [ ] Arată capturile de ecran din `docs/img/` și un GIF scurt făcut cu proiectul demo.
- [ ] Adaugă linkurile de donații (Buy Me a Coffee, Patreon) în README și în caseta About a studioului; nu promite funcționalități în schimbul donațiilor decât dacă poți ține promisiunea.
- [ ] Postează acolo unde sunt animatorii (comunități de rigging 2D / animație indie); cere rapoarte de bug-uri, nu laude.
- [ ] Rezervă timp pentru a răspunde rapid la primele issue-uri; primele două săptămâni decid dacă contribuitorii rămân.
