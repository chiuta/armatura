<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio) -->
# Kiadási ellenőrzőlista (tulajdonosi oldal)

*Olyan lépések, amelyeket csak a tulajdonos végezhet el, sorrendben. A projekt eszközei semmit sem hajtottak végre belőlük: a build környezetnek nem volt GitHub-hozzáférése, wasm32 targetje, valódi GPU-ja és jogásza. Nem jogi tanács. A **[blocker]** jelölésű tételeket nyilvános bejelentés előtt el kell végezni.*

## 1. Bármi közzététele előtt

- [ ] **[blocker]** Erősítsd meg vagy cseréld le a biztonsági kapcsolattartót (az `alexio@trom.tf` helyőrző a `docs/SECURITY.md`-ben, a `--version` kimenetében és a licencdokumentumokban).
- [ ] **[blocker]** Keresd meg az „ARMATURA” nevet a számodra fontos védjegynyilvántartásokban (EUIPO, USPTO, OSIM), valamint az alkalmazásboltokban / csomagnyilvántartókban. Lásd `TRADEMARK.md`. Ha ütközik, nevezd át az első kiadás előtt (a név megjelenik a binárisban, a crate-ekben, a dokumentációban).
- [ ] **[blocker]** Kérj meg egy jogászt, hogy vizsgálja át a `LICENSING.md`-t, az MPL-2.0 / AGPL-3.0 felosztást, a `docs/THIRD_PARTY.md`-t és a `docs/PATENT_NOTES.md`-t. Döntsd el, szeretnél-e kettős licencelést; ha igen, CLA kell *mielőtt* külső hozzájárulásokat fogadsz el (ma: csak DCO).
- [ ] Ellenőrizd a Graphics Gems görbeillesztési hivatkozás feltételeit (a `CLEANROOM.md`-ben emlékezetből idézve); a független megvalósításra vonatkozó megfogalmazást csak akkor tartsd meg, ha pontos.
- [ ] Futtasd a `reuse lint`-et és a `cargo deny check`-et a saját gépeden (a projektnek csak a saját `spdx_audit` tesztje van).
- [ ] Ellenőrizd, hogy a fában nincs személyes adat, helyi útvonal vagy token (`grep -r "/home/" .`, `grep -ri token`), és hogy a mellékelt modell és a demó asset-ek mind saját generálásúak.

## 2. Repó

- [ ] Hozd létre a GitHub (vagy Codeberg / saját Forgejo) repót; told fel a fát a `target/` és az `out/` **nélkül**.
- [ ] Tartsd meg a `LICENSE`, `LICENSES/`, `REUSE.toml`, `.github/FUNDING.yml` fájlokat (Buy Me a Coffee `echo.of.the.strings`, Patreon `alexio_tf`).
- [ ] Kapcsold be a privát sebezhetőség-bejelentést (Security fül), és irányítsd rá a `docs/SECURITY.md`-t.
- [ ] A studio indításakor add meg a repó URL-jét a `--source-url` kapcsolóval (AGPL forrásajánlat, `/source` útvonal), és teszteld az ajánlatot friss telepítésből.

## 3. Az első CI-futás (eddig soha nem futott)

- [ ] Told fel és kövesd a `.github/workflows/ci.yml`-t: Linux, Windows, macOS tesztek és `clippy -D warnings`; a wasm32 build job; a cargo-deny és REUSE job.
- [ ] Számíts első futásos hibákra Windowson/macOS-en (útvonalak, jogosultságok, pixelkülönbségek az élsimított éleken, L-04 / L-26). Javítsd vagy lazítsd toleranciákkal, majd jegyezd fel az eredményt a `docs/LIMITATIONS.md`-ben.
- [ ] Ha a wasm32 job átmegy, futtasd le a `docs/WASM.md` ellenőrzőlistáját egy valódi böngészőben, és frissítsd az L-01 / L-02 tételt.

## 4. Emberi tesztelés

- [ ] Add oda a studiót 3–5 animátornak egy hétre egy rövid feladattal (rigelj meg egy karaktert, készíts járásciklust, exportálj egy GIF-et). Jegyezd fel, hol akadnak el; mindegyikből nyiss egy issue-t.
- [ ] Teszteld valódi böngészőkben (Chrome, Firefox, Safari), érintőképernyős eszközön és csak billentyűzettel.
- [ ] Próbáld ki az arcdetektor / mocap / hang funkciókat valódi adatokkal; őszintén jelezd, hogy korlátozottak (L-12, L-13).

## 5. Üzemeltetési biztonság (ha te üzemelteted a studiót vagy az együttműködést)

- [ ] Tedd TLS-t lezáró reverse proxy mögé (recept: `docs/SECURITY.md` 12. szakasz); a sima HTTP portot soha ne tedd ki az internetre.
- [ ] Használj résztvevőnkénti tokeneket (`collab-token`), állíts be lejáratot, a proxy mögött kösd localhostra.
- [ ] Rendelj független biztonsági felülvizsgálatot, mielőtt üzemeltetett együttműködést kínálsz idegeneknek.

## 6. Kiadási artefaktumok

- [ ] Címkézz fel egy verziót, frissítsd a `CHANGELOG.md`-t, építs release binárisokat a saját gépeiden (a projekt környezete soha nem épített `--release` módban), tegyél közzé ellenőrzőösszegeket.
- [ ] Mellékeld az AGPL-es részekhez tartozó forrást (maga a címke elég; tartsd elérhetően, amíg binárisokat terjesztesz).
- [ ] Minden binárishoz mellékeld a `docs/THIRD_PARTY_NOTICES.md`-t (generáld újra a `scripts/licence-audit.sh`-val).
- [ ] Crates.io / npm / csomagnyilvántartók: ellenőrizd, hogy a nevek szabadok, és a licencmezők egyeznek a `LICENSING.md`-vel.

## 7. Bejelentés és adományok

- [ ] A README első képernyője: mi működik, mi nem (linkeld a `docs/LIMITATIONS.md`-t); ne állítsd fenntartás nélkül, hogy „Moho-class”.
- [ ] Mutasd meg a `docs/img/` képernyőképeit és egy rövid GIF-et, amelyet a demóprojekttel készítettél.
- [ ] Tedd ki az adományozási linkeket (Buy Me a Coffee, Patreon) a README-be és a studio Névjegy ablakába; ne ígérj funkciókat adományok fejében, hacsak nem tudod betartani az ígéretet.
- [ ] Posztolj ott, ahol az animátorok vannak (2D rigeléssel / indie animációval foglalkozó közösségek); hibajelentéseket kérj, ne dicséretet.
- [ ] Szánj időt arra, hogy gyorsan válaszolj az első issue-kra; az első két hét dönti el, hogy a hozzájárulók maradnak-e.
