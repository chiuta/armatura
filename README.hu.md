<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio) -->
# ARMATURA

**Rustban írt 2D vektoros riggelő és animációs motor: csontok, IK, skinning, deformerek, állapotgép-futtatókörnyezet, parancssori eszköztár, böngészős stúdió és beágyazható könyvtárak egyetlen workspace-ben.** Román nyelvű változat: [README.ro.md](README.ro.md).

## Összefoglalás és filozófia

Az ARMATURA-val vektoros alakzatokból és képekből építhetsz karaktert vagy jelenetet, csontokkal és kényszerekkel riggelheted, kulcsokkal, klipekkel és fizikával animálhatod, majd GIF-ként, videóként, Lottie-ként, glTF-ként, önálló HTML-lejátszóként vagy interaktív futtatókörnyezetként exportálhatod. Kis, determinisztikus elemekből épül fel: ugyanaz a projekt és ugyanaz a seed ugyanazokat a pixeleket adja, minden eszköz egy dokumentált JSON-projektet olvas és ír, és minden parancs terminálból is működik, így az animációt úgy lehet szkriptelni, átnézni, diffelni és tesztelni, mint a forráskódot.

A projektet alakító alapelvek:

* **Független megvalósítás.** Nyilvános specifikációkból, tanulmányokból és tankönyvi matematikából íródott. A versenytársak funkciólistáit a *hatókör* meghatározásához vettük figyelembe; harmadik fél forráskódját nem másoltuk, és a projekt semmilyen zárt formátumot nem olvas, sem nem ír. Nem szigorú clean-room folyamat (lásd [CLEANROOM.md](docs/i18n/hu/CLEANROOM.md), a kutatási közzétételt is beleértve), és nem áll kapcsolatban a Moho-val vagy más kereskedelmi termékkel, nem származik azokból, és nem kompatibilis velük. A kód nagyrészt MI-modellel készült.
* **Őszinte állapotjelzés.** Minden funkciót azzal együtt írunk le, hogy mit *nem* tud ([docs/LIMITATIONS.md](docs/i18n/hu/LIMITATIONS.md), [ROADMAP.md](docs/i18n/hu/ROADMAP.md) 9. szakasz).
* **Alapértelmezetten biztonságos.** Bemenet hatására nincs pánik, a értékek végességét ellenőrzi, erőforrás-korlátok vannak, az írás atomi, az eszközútvonalak a projektmappába vannak zárva ([docs/SECURITY.md](docs/i18n/hu/SECURITY.md)).
* **Elsőbbség a nyílt formátumoknak.** OpenRaster, OCA, SVG, Lottie, glTF, APNG, WebM, EXR; tervezetten nincsenek zárt fájlformátumok.
* **Kis függőségi lábnyom**, a licencpolitikát tesztek és a `cargo deny` ellenőrzik.

## Állapot

> **Előzetes kiadás, egyetlen környezet.** Minden crate verziója `0.1.0`, és semmi sem jelent még meg; a „v0.5 / v0.6 / v0.7” a történetben *funkciószintet* jelöl, nem kiadási számot ([CHANGELOG.md](docs/i18n/hu/CHANGELOG.md)).
>
> | Ellenőrizve | Nem ellenőrizve |
> |---|---|
> | A `cargo test --workspace` 1368 tesztje és 19 doc-tesztje sikeres (2026. okt. 22., Linux x86-64); a stúdió négy headless Chromium füstteszje sikeres (124, 60, 89, illetve 35 ellenőrzés az ui, collab, overlays és suggest szkriptekhez); a CLI-t, a Rust homlokzatot és a C ABI-t (ASan/UBSan alatt is) lefuttattuk a dokumentációban szereplő példákkal | **A WebAssembly modult még soha nem fordították le és nem futtatták** (a wasm32 target telepítése nem sikerült; [docs/WASM.md](docs/i18n/hu/WASM.md)); a stúdiót ember még nem használta éles környezetben, és Chromiumtól eltérő böngészőben sem; valódi GPU-val (csak szoftveres adapterrel), webkamerával, fényképpel vagy mozgásrögzítő fájllal még nem próbálták; Windows és macOS nincs tesztelve; nem volt biztonsági audit, jogi felülvizsgálat és szabadalmi keresés |
>
> Mielőtt bármire ráépítenél, olvasd el a [docs/LIMITATIONS.md](docs/i18n/hu/LIMITATIONS.md) fájlt (73 számozott korlát súlyossággal és kerülőmegoldással).

## Funkciók területenként

A részletek és a pontos opciók: [docs/CLI_REFERENCE.md](docs/i18n/hu/CLI_REFERENCE.md); ami részleges vagy hiányzik: [docs/LIMITATIONS.md](docs/i18n/hu/LIMITATIONS.md).

* **Rajzmodell.** Vektoros útvonalak, kitöltések, körvonalak (szaggatott, változó vastagságú), lineáris és radiális színátmenetek, maszkok, keverési módok, rétegenkénti effektek (elmosás, árnyék, színezés, kitöltés), képrétegek mesh warppal, folyékony metaballok, sokszög-boole-műveletek, determinisztikus részecskék, többsíkú kamera mentett beállításokkal, mélységélesség.
* **Riggelés.** Csontok előre kinematikával; kétcsontos, soft, stretch és FABRIK IK; korlátok, kényszerek, rugók; skinning súlyeszközökkel; lattice, spine, morph, wobble és cage deformerek; mesh-követés alfából; warp pivotok és glue; driverek (csúszkák, joystickok, nézetváltás); skin-készletek; automatikus riggelés réteg nevekből; szem-vágás; elrendezési konténerek és görgetési régiók.
* **Animáció.** Kulcsok Bézier/lépcsős/rugó/pattogó átmenettel, kifejezések és vezérelt csatornák, kapcsolórétegek, take-ek és rétegzett klipköteg, ciklus- és helyben történő átalakítás, lábrögzítés, mágnesek, útvonalkövetés, idle és overlap generátorok, lánc-/haj-/ruhafizika, lip-sync hangból vagy átiratból, beszédérzékeny fejmozgás, BVH-retargeting, élő bemenet (MIDI, egér, gamepad, általad megadott webkamerás landmarkok), hangsávok vágással, ki- és beúsztatással, keveréssel.
* **Futtatókörnyezet.** Állapotgépek 1D blend tree-kkel, adatkötés, görgetési fizika, részletességi szint hibahatárral, Rhai szkript-sandbox, patch-műveletek fölötti ágens (MCP) szerver.
* **Kimenet.** SVG, PNG, PNG-sorozatok és sprite sheetek, GIF, APNG, MP4/WebM (ffmpeg-gel), önálló HTML-lejátszó, Lottie és dotLottie, glTF 2.0 bináris, OpenEXR/16 bites PNG, OCA; méretre célzott export (`export-fit`). Import: SVG, OpenRaster, Lottie, BVH, PNG/GIF/APNG-sorozatok.
* **Minőségbiztosítási eszközök.** `check`, `lint`, szemantikus `diff`, póz-fuzzing hőtérképekkel, animációs CI észlelési diff-fel és referenciakockákkal ([docs/CI.md](docs/i18n/hu/CI.md)), verziókezelés szemantikus merge-dzsel (`vcs`), villogás- és mozgásbiztonsági ellenőrzések.
* **Stúdió.** A CLI által kiszolgált böngészős szerkesztő: vászon-gizmók, idővonal, dope sheet és görbeszerkesztő, overlayek, parancspaletta és gyorsbillentyűk, visszavonás/újra, valós idejű többfelhasználós szerkesztés opcionális tokennel és tartós naplóval ([docs/STUDIO.md](docs/i18n/hu/STUDIO.md)).
* **Könyvtárak.** Stabil Rust homlokzat ([docs/API.md](docs/i18n/hu/API.md)), C ABI ([docs/C_ABI.md](docs/i18n/hu/C_ABI.md)), WebAssembly modul és JavaScript wrapper (lefordítatlan; [docs/WASM.md](docs/i18n/hu/WASM.md)), opcionális wgpu renderer.

## Gyors kezdés

Követelmények: egy friss stabil Rust eszközlánc. Az `ffmpeg` csak az MP4/WebM exporthoz és az mp3/ogg hanghoz kell; soha nem csomagoljuk vele. Az alábbi összes parancsot lefuttattuk ehhez a README-hez (debug build, Linux). A kiadási buildek (`cargo build --release`) renderelésnél sokkal gyorsabbak, és ugyanígy használhatók a `target/release` mappával.

```sh
cargo build -p armatura-cli                       # builds target/debug/armatura
export PATH="$PWD/target/debug:$PATH"             # so that `armatura` is found

armatura --version                                # version, licence, source offer, support links
armatura demo demo.json                           # write the built-in rig demo (640x480, 48 frames)
armatura check demo.json                          # validate: ok: 'ARMATURA demo' 640x480 @ 24 fps, 48 frames, 6 bones...
armatura lint demo.json                           # static rig/animation checks: 0 finding(s), 0 error(s)
armatura render demo.json 12 frame12.png --bones  # one frame, with the skeleton overlay
armatura render demo.json 12 frame12.svg          # the same frame as vector output
armatura frames demo.json frames_out --png        # every frame as a PNG
armatura export demo.json demo.gif                # also .html (player), .json (Lottie), .lottie, .mp4 (ffmpeg)
armatura demo-fx fx.json                          # second demo: liquid, booleans, particles
armatura lipsync demo.json voice.wav mouth out.json   # mouth shapes from a WAV file (switch layer "mouth")
armatura studio demo.json --port 8080             # browser editor at http://127.0.0.1:8080
armatura ci init demo.json && armatura ci run     # animation CI: goldens + checks (about 40 s in a debug build)
```

A `voice.wav` bármilyen, általad megadott mono vagy sztereó WAV-fájl. A stúdió csak a `127.0.0.1` címen figyel, és más gazdanevekre érkező kéréseket elutasít; ha meg akarod osztani, előbb olvasd el a [docs/SECURITY.md](docs/i18n/hu/SECURITY.md) fájlt.

A könyvtárak használata:

```sh
cargo build -p armatura-capi                      # target/debug/libarmatura_capi.so and .a
gcc -std=c99 -o smoke armatura-capi/tests/smoke.c -Ltarget/debug -larmatura_capi
LD_LIBRARY_PATH=target/debug ./smoke demo.json    # abi 1, 48 frames, 320x240, rc=0, checksum=...
```

A Rust homlokzat az `armatura_core::prelude` ([docs/API.md](docs/i18n/hu/API.md)); további C- és Python-példák a [docs/C_ABI.md](docs/i18n/hu/C_ABI.md) fájlban vannak.

Tesztek (nagyjából 14 perc debug buildben egy megosztott gépen; lásd [docs/DEVELOPMENT.md](docs/i18n/hu/DEVELOPMENT.md)):

```sh
cargo test --workspace
cargo test --doc -p armatura-core
cargo test -p armatura-cli --test spdx_audit
```

## Tárolóstruktúra

| Útvonal | Tartalom | Licenc |
|---|---|---|
| `armatura-core/` | motor, formátumok, eszközök, a stabil `api` homlokzat | MPL-2.0 |
| `armatura-cli/` | az `armatura` bináris, a stúdió- és együttműködési szerver, végpontok közötti és füsttesztek | AGPL-3.0-or-later |
| `armatura-capi/` | C ABI (`armatura.h`, megosztott és statikus könyvtár) | MPL-2.0 |
| `armatura-wasm/` | WebAssembly ABI, JavaScript wrapper, demóoldal (lefordítatlan) | MPL-2.0 |
| `armatura-gpu/` | opcionális wgpu renderer (nem alapértelmezett tag) | MPL-2.0 |
| `docs/` | dokumentáció (CC-BY-4.0) és generált harmadik féltől származó fájlok | CC-BY-4.0 |
| `scripts/` | `e2e.sh`, `licence-audit.sh` | MPL-2.0 |
| `LICENSES/`, `REUSE.toml`, `deny.toml` | licencszövegek, licenc-metaadatok, függőségi szabályzat | - |
| `ROADMAP.md`, `FEATURES.md`, `CLEANROOM.md`, `LICENSING.md`, `TRADEMARK.md`, `CONTRIBUTING.md`, `CHANGELOG.md` | projektdokumentumok | CC-BY-4.0 |

## Dokumentációs térkép

Kezdd a [docs/INDEX.md](docs/i18n/hu/INDEX.md) fájlnál (minden dokumentum, célközönség, olvasási útvonalak).

| Oldal | Kinek / mire |
|---|---|
| [docs/GETTING_STARTED.md](docs/i18n/hu/GETTING_STARTED.md) | első projekt, renderelés, export |
| [docs/CLI_REFERENCE.md](docs/i18n/hu/CLI_REFERENCE.md) | minden parancs és opció |
| [docs/STUDIO.md](docs/i18n/hu/STUDIO.md) | a böngészős szerkesztő |
| [docs/RIGGING_GUIDE.md](docs/i18n/hu/RIGGING_GUIDE.md) | riggek építése |
| [docs/ANIMATION_GUIDE.md](docs/i18n/hu/ANIMATION_GUIDE.md) | animálás, klipek, fizika, hang |
| [docs/PROJECT_FORMAT.md](docs/i18n/hu/PROJECT_FORMAT.md) | a JSON projektformátum |
| [docs/EXPORT_FORMATS.md](docs/i18n/hu/EXPORT_FORMATS.md) | mit visz és mit hagy el az egyes formátum |
| [docs/ARCHITECTURE.md](docs/i18n/hu/ARCHITECTURE.md) | felépítés és modulok |
| [docs/GLOSSARY.md](docs/i18n/hu/GLOSSARY.md) | fogalmak |
| [docs/FAQ.md](docs/i18n/hu/FAQ.md) | gyakori kérdések |
| [docs/API.md](docs/i18n/hu/API.md) | Rust homlokzat 1.4 |
| [docs/C_ABI.md](docs/i18n/hu/C_ABI.md) | C interfész |
| [docs/WASM.md](docs/i18n/hu/WASM.md) | WebAssembly (soha nem fordították le) |
| [docs/CI.md](docs/i18n/hu/CI.md) | animációs CI |
| [docs/DEVELOPMENT.md](docs/i18n/hu/DEVELOPMENT.md) | build, teszt, konvenciók, kiadás |
| [docs/SECURITY.md](docs/i18n/hu/SECURITY.md) | fenyegetésmodell és megerősítés |
| [docs/LIMITATIONS.md](docs/i18n/hu/LIMITATIONS.md) | ismert korlátok |
| [docs/PATENT_NOTES.md](docs/i18n/hu/PATENT_NOTES.md), [docs/THIRD_PARTY.md](docs/i18n/hu/THIRD_PARTY.md), [docs/THIRD_PARTY_NOTICES.md](docs/THIRD_PARTY_NOTICES.md) | szabadalmak, függőségek licencei |
| [CHANGELOG.md](docs/i18n/hu/CHANGELOG.md), [ROADMAP.md](docs/i18n/hu/ROADMAP.md), [FEATURES.md](docs/i18n/hu/FEATURES.md) | előzmények, terv és részletes állapot, funkciómátrix |
| [CLEANROOM.md](docs/i18n/hu/CLEANROOM.md), [LICENSING.md](docs/i18n/hu/LICENSING.md), [TRADEMARK.md](docs/i18n/hu/TRADEMARK.md), [CONTRIBUTING.md](docs/i18n/hu/CONTRIBUTING.md) | eredet, licencek, névhasználati szabályzat, közreműködési szabályok |

## Licencösszefoglaló

* **MPL-2.0** az `armatura-core`, `armatura-capi`, `armatura-wasm`, `armatura-gpu` könyvtárakra és a `scripts/` mappára: beleköthetők a saját programodba, nyílt vagy zárt forrásúba egyaránt; az ezekben a fájlokban végzett módosításokat MPL alatt kell közzétenni.
* **AGPL-3.0-or-later** az `armatura-cli`-re (a parancssor és a stúdió). Ha módosítod, és másoknak hálózaton keresztül hozzáférést adsz hozzá, a 13. szakasz előírja, hogy fel kell kínálnod nekik a forráskódodat: az `armatura studio --source-url URL` közzéteszi, hol található; a `/source` és az `armatura --version` tartalmazza az ajánlatot.
* **CC-BY-4.0** a dokumentációra és a szintetikus eszközökre (a mellékelt szintetikus arcfelismerő modellt is beleértve).
* **A projektjeid és a renderek a tieid maradnak.**
* A név és a logó nem tartozik ezen licencek hatálya alá ([TRADEMARK.md](docs/i18n/hu/TRADEMARK.md)). A szabadalmi helyzetet nem értékeltük ([docs/PATENT_NOTES.md](docs/i18n/hu/PATENT_NOTES.md)). A beküldött kód kereskedelmi vagy kettős licencelése közreműködői licencszerződés nélkül nem lehetséges, és ilyen nincs.
* Harmadik féltől származó crate-ek: 160 az utolsó auditkor, mind megengedő licencű, kivéve egy gyenge copyleft crate-et az opcionális `scripting` funkció mögött ([docs/THIRD_PARTY.md](docs/i18n/hu/THIRD_PARTY.md)). Az ffmpeg-et soha nem csomagoljuk.

Lásd: [LICENSE](LICENSE), [LICENSES/](LICENSES/) és [LICENSING.md](docs/i18n/hu/LICENSING.md). Mindez nem jogi tanács; a jogi felülvizsgálat még hátravan.

## Közreműködés

A közreműködést szívesen fogadjuk, annak a résznek a licence alatt, amelyet módosít. Olvasd el a [CONTRIBUTING.md](docs/i18n/hu/CONTRIBUTING.md) fájlt (minden commithoz kell egy `Signed-off-by` sor, DCO 1.1), a [CLEANROOM.md](docs/i18n/hu/CLEANROOM.md) fájlt (csak nyilvános forrásokból implementálj; ne másolj zárt vagy összeférhetetlen licencű kódból) és a [docs/DEVELOPMENT.md](docs/i18n/hu/DEVELOPMENT.md) fájlt (konvenciók, új modul ellenőrzőlistája, tesztek). Jó első hozzájárulások: egy javítás a [docs/LIMITATIONS.md](docs/i18n/hu/LIMITATIONS.md) listájából, a WebAssembly-ellenőrzőlista tényleges lefuttatása, tesztek Windowson vagy macOS-en, valódi GPU-n végzett benchmark.

Biztonsági problémák: ne nyiss nyilvános issue-t; kövesd a [docs/SECURITY.md](docs/i18n/hu/SECURITY.md) 10. szakaszát.

## Támogatás

Az ARMATURA-t Alexandru-Ionuț Chiuță (Alexio) fejleszti. Ha hasznosnak találod, támogathatod a munkát:

* Buy Me a Coffee: <https://buymeacoffee.com/echo.of.the.strings>
* Patreon: <https://patreon.com/c/alexio_tf>

A karbantartó elérhetősége (az `armatura --version` is kiírja): alexio@trom.tf.

## Köszönetnyilvánítás

* A nyilvános tanulmányok és tankönyvek szerzői, amelyeket az algoritmusok követnek, modulonként megnevezve a [CLEANROOM.md](docs/i18n/hu/CLEANROOM.md) fájlban (Bézier- és spline-matematika, inverz kinematika, lineáris keverésű skinning, mean-value koordináták, görbeillesztés és mások).
* A nyílt specifikációk, amelyeket az ARMATURA olvas és ír: SVG, OpenRaster, OCA, Lottie/dotLottie, glTF, APNG, WebM, OpenEXR, BVH, PNG, GIF.
* A Rust crate-ek, amelyekre épít, licenceikkel együtt felsorolva a [docs/THIRD_PARTY.md](docs/i18n/hu/THIRD_PARTY.md) fájlban, különösen a `tiny-skia`, `serde`, `png`, `zip`, `rhai` és `wgpu`.
* Mindenki, aki a fenti hivatkozásokon keresztül támogatja a projektet.
