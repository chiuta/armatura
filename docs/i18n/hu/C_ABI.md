# ARMATURA C ABI

*ABI 1-es verzió (`ARMATURA_ABI_VERSION`, `armatura_abi_version()`). Crate: `armatura-capi`, header: `armatura-capi/armatura.h`. Célközönség: integrátorok, akik ARMATURA-projekteket szeretnének betölteni és lejátszani C, C++, Python, Go, C#, Swift vagy bármely, C foreign-function interface-szel rendelkező nyelvből. Dokumentáció: CC-BY-4.0; kódminták: MPL-2.0.*

A C ABI egy kicsi, egyszerű C-s burkoló a stabil Rust facade ([API.md](API.md)) fölött. Segítségével egy gazdaprogram megnyithat egy projektet, kiolvashatja annak adatait és a csonttranszformációkat, RGBA-ba renderelhet képkockákat, és futtathat **állapotgépeket** (interaktív lejátszás, görgetés, részletességi szint). A facade 1.4 óta alapvető **szerkesztésekre** (rétegek és csontok létrehozása, kulcsok beállítása, fizika bake-elése, visszavonás) és **memóriába exportálásra** (GIF, glTF, Lottie, HTML, APNG, PNG, EXR, projekt-JSON) is képes. Verziókezelést nem tud, és az eszközöket sem futtatja: ezekhez használd a Rust API-t vagy a parancssort ([CLI_REFERENCE.md](CLI_REFERENCE.md)).

Tartalom: [Build és linkelés](#build-és-linkelés) - [Konvenciók](#konvenciók) - [Függvényreferencia](#függvényreferencia) - [Tulajdonlás és élettartam](#tulajdonlás-és-élettartam) - [Hibakezelés](#hibakezelés) - [Szálkezelés](#szálkezelés) - [Teljes C példa](#teljes-c-példa) - [Görgetés és részletességi szint C-ben](#görgetés-és-részletességi-szint-c-ben) - [Python (ctypes) példa](#python-ctypes-példa) - [Más nyelvek](#más-nyelvek) - [Verziózás](#verziózás) - [Ismert furcsaságok](#ismert-furcsaságok)

## Build és linkelés

A crate háromféle könyvtárat épít (`crate-type = ["cdylib", "staticlib", "rlib"]`); az `armatura-core`-tól `default-features = false` mellett függ, így nem tartalmaz szkriptmotort és MPL-licencű harmadik féltől származó crate-et sem.

```sh
cargo build -p armatura-capi             # debug:   target/debug/libarmatura_capi.{so,a}
cargo build --release -p armatura-capi   # release: target/release/libarmatura_capi.{so,a}
```

| Platform | Dinamikus könyvtár | Statikus könyvtár |
|---|---|---|
| Linux | `libarmatura_capi.so` | `libarmatura_capi.a` |
| macOS | `libarmatura_capi.dylib` | `libarmatura_capi.a` |
| Windows | `armatura_capi.dll` (+ import könyvtár) | `armatura_capi.lib` |

**Tesztelve:** Linux x86-64, gcc, mind a megosztott, mind a statikus könyvtárral (lásd alább). A macOS és a Windows esetén ezek a Rust szokásos kimeneti nevei az adott célokra, de a szerző **nem építette és nem futtatta** őket. Egy debug `.so` kb. 150 MB (debug info); amit szállítasz, azt `--release` móddal építsd (a release könyvtár méretét itt nem mértük).

Fordítás és linkelés a megosztott könyvtárral:

```sh
cc -std=c99 -Wall -Wextra -Iarmatura-capi -o example example.c -Ltarget/debug -larmatura_capi
LD_LIBRARY_PATH=target/debug ./example project.json frame.ppm      # or install the .so and use rpath / ldconfig
```

Statikus linkelés (Linuxon ellenőrizve; a Rust szabványkönyvtárnak ezek a rendszerkönyvtárak kellenek):

```sh
cc -std=c99 -Iarmatura-capi -o example example.c target/debug/libarmatura_capi.a -lpthread -ldl -lm
```

Az `armatura.h` rendelkezik `extern "C"` védőelemekkel a C++-hoz, csak a `<stddef.h>` és a `<stdint.h>` kell hozzá, és ez az egyetlen fájl, amelyet beszerkesztesz. Másold a forrásaid mellé. Nincs pkg-config fájl, telepítési cél és verziózott SONAME; a könyvtárat az alkalmazásoddal együtt szállítsd (vendor).

A pánikokat minden belépési ponton elkapjuk és hibakóddá alakítjuk, ami azon alapul, hogy a könyvtár az alapértelmezett `panic = "unwind"` beállítással készül. Ne építsd `panic = "abort"` móddal (ekkor a motor egy hibája -1 visszaadása helyett megszakítaná a folyamatodat). A workspace nem állítja be ezt a profilopciót.

## Konvenciók

* A sztringek UTF-8 kódolásúak. A bemeneti sztringek (`const char *`) NUL-terminátorosak, kivéve a projekt-JSON-t, amelyet mutató plusz hossz formájában kell átadni, és nem kell terminátor.
* Az `int`-et visszaadó függvények **0-t adnak siker esetén, és negatív értéket hiba esetén**. A `-1` általános hiba; a `-2` ezen túl azt jelenti, hogy „a kimeneti puffered túl kicsi”, és csak a két render-függvénynél fordul elő.
* A karakterpuffert kitöltő függvények `(char *out, size_t cap)` paramétereket vesznek, és a szöveg **teljes hosszát** adják vissza (a NUL nélkül). A szöveget `cap - 1` bájtra csonkítják, és `cap > 0` esetén NUL-lal zárják. Add át az `out == NULL` értéket (bármilyen `cap` mellett), ha csak a hosszra vagy kíváncsi. A negatív visszatérési érték hiba (rossz index, ismeretlen név).
* A lebegőpontos argumentumoknak végesnek kell lenniük; az idővonalon kívüli képkockákat szorítja; a `scale` 0,05 és 4 között kell legyen.
* Minden handle-argumentum lehet `NULL` a getterek esetén (ezek 0-t adnak és beállítják a hibaszöveget), a többi függvénynél ez hiba.
* A kimeneti RGBA **egyenes alfás** (nem premultiplikált), csatornánként 8 bit, soronként, a felső sor elöl, `width * height * 4` bájt.
* A csontok világtranszformációi csontonként hat float, `[a b c d tx ty]`; egy csont-lokális `(x, y)` pont a `(a*x + c*y + tx, b*x + d*y + ty)` pontra képződik le.

## Függvényreferencia

53 függvény. `E` = `ArmaturaEngine *`, `P` = `ArmaturaPlayer *`.

### Könyvtár

| Függvény | Leírás |
|---|---|
| `const char *armatura_version(void)` | az `armatura-capi` crate verziósztringje (jelenleg `"0.1.0"`); statikus, sosem szabadítandó fel |
| `uint32_t armatura_abi_version(void)` | a betöltött könyvtár ABI-verziója; hasonlítsd össze annak a headernek az `ARMATURA_ABI_VERSION` értékével, amellyel fordítottál |
| `const char *armatura_last_error(void)` | az utolsó hiba szövege **ezen a szálon**; üres sztring, ha még nem volt; lásd [Hibakezelés](#hibakezelés) |

### Engine: megnyitás, lekérdezés, renderelés

| Függvény | Leírás |
|---|---|
| `E *armatura_open(const char *json, size_t len)` | egy projekt feldolgozása és validálása (UTF-8, `len` bájt, 1 bájttól 256 MiB-ig); hiba esetén `NULL`. A könyvtár lemásolja, amire szüksége van; a `json`-t azonnal felszabadíthatod. A projektben lévő relatív képfájlokat a folyamat aktuális könyvtárában keresi, az abszolút útvonalakat és a `..` elemet elutasítja (lásd [SECURITY.md](SECURITY.md)); a data-URI képek mindig működnek. |
| `void armatura_close(E *)` | az engine felszabadítása; a `NULL` megengedett; handle-önként pontosan egyszer hívd |
| `uint32_t armatura_frame_count(const E *)` | a képkockák száma |
| `float armatura_fps(const E *)` | képkocka/másodperc |
| `uint32_t armatura_width(const E *)`, `armatura_height(const E *)` | a színpad mérete színpad-egységekben, egészre kerekítve |
| `uint32_t armatura_bone_count(const E *)` | a csontok száma |
| `int armatura_bone_name(const E *, uint32_t index, char *out, size_t cap)` | az `index`-edik csont neve (rig-sorrend); a hossz, vagy -1 rossz indexre |
| `int armatura_bone_world(const E *, float frame, float *out, size_t cap_floats)` | minden csont világmátrixa; a `cap_floats` legalább `6 * bone_count` kell legyen, különben -1 |
| `int armatura_render_rgba(const E *, float frame, float scale, uint8_t *out, size_t cap, uint32_t *w, uint32_t *h)` | egy képkocka renderelése; a kimenet mérete `round(width*scale) x round(height*scale)`; 0-t ad, **-2-t, ha a `cap` kisebb a `w*h*4` értéknél** (és a `*w`, `*h` be van állítva, így lefoglalhatsz és újra hívhatsz; az `out == NULL` átadásával kérdezhetsz), egyéb hibánál -1-et. A `w` és a `h` lehet `NULL`. |

### Player: állapotgépek

Egy projekt tartalmazhat `machines` (futásidejű állapotgépek) és `bindings` (a tulajdonságokat vezérlő külső értékek) elemeket. Egy player egy gépet futtat: név szerint állítod a bemeneteket, előreléptetsz az időben, kiolvasod az állapotokat és az eseményeket, és renderelsz az aktuális pózból. A szemantikát lásd az [API.md](API.md) fájlban (ezek a `Player` metódusai).

| Függvény | Leírás |
|---|---|
| `uint32_t armatura_machine_count(const E *)` | a projekt gépei |
| `int armatura_machine_name(const E *, uint32_t index, char *out, size_t cap)` | az `index`-edik gép neve (karakterpuffer-szabályok) |
| `P *armatura_player_open(const E *, const char *machine)` | a gép indítása a 0. időpontban; hiba esetén `NULL` (ismeretlen gép, ...). A player a projekt egy **másolatát** birtokolja: az engine `armatura_close` hívása után is érvényes marad. |
| `void armatura_player_close(P *)` | a player felszabadítása; a `NULL` megengedett |
| `int armatura_player_set_bool(P *, const char *name, int value)` | bool bemenet (`value != 0`) |
| `int armatura_player_set_number(P *, const char *name, double value)` | szám bemenet (véges) |
| `int armatura_player_fire(P *, const char *trigger)` | egy trigger elsütése; azt az átmenet fogyasztja el, amelyik teszteli |
| `int armatura_player_set_value(P *, const char *path, double value)` | külső érték az adatkötésekhez, pl. `"player.health"`; azt a scroll-régiót is vezérli, amelynek `input` értéke ez az útvonal (pixel a főtengelye mentén) |
| `int armatura_player_advance(P *, double dt)` | előrelépés `dt` másodperccel, `0..=10`; determinisztikus (a hosszú lépéseket legfeljebb 1/60 s-os darabokra bontja) |
| `int armatura_player_state(P *, const char *layer, char *out, size_t cap)` | egy gépréteg aktuális állapotának neve; a hossz, vagy -1 ismeretlen rétegre |
| `int armatura_player_events(P *, char *out, size_t cap)` | a függő események JSON-tömbként `[{"time":..,"layer":..,"kind":"entered"\|"exited","state":..}]`. A teljes hosszat adja vissza. **Az eseményeket csak akkor fogyasztja el, ha az egész szöveg elfért (`cap` > hossz)**; különben megmaradnak, és nagyobb pufferrel hívd újra. Az `out == NULL` a méretet kérdezi le, és semmit sem fogyaszt el. |
| `int armatura_player_render_rgba(P *, float scale, uint8_t *out, size_t cap, uint32_t *w, uint32_t *h)` | ugyanaz a szerződés, mint az `armatura_render_rgba`-nál, az aktuális pózra; a méret a player vásznát követi |

### Player: elrendezés, görgetés, részletességi szint

| Függvény | Leírás |
|---|---|
| `int armatura_player_set_canvas(P *, float width, float height)` | reszponzív elrendezés: a vászon mérete színpad-egységekben (véges, pozitív, legfeljebb 32768); a konténerek újraszámolódnak, a renderek ezt a méretet kapják |
| `int armatura_player_canvas_size(P *, float *w, float *h)` | az aktuális vászonméret; bármelyik mutató lehet `NULL` |
| `uint32_t armatura_player_scroll_count(P *)` | a projekt scroll-régiói |
| `int armatura_player_scroll_id(P *, uint32_t index, char *out, size_t cap)` | az `index`-edik régió id-ja (karakterpuffer-szabályok) |
| `int armatura_player_scroll_drag(P *, const char *id, float x, float y)` | nyers mutató-eltolás pixelben (a tartalom feljebb / balra mozdul); a végeken túl szorít vagy gumiszalag-szerűen húz |
| `int armatura_player_scroll_release(P *, const char *id, float vx, float vy)` | elengedési sebesség px/s-ban; az `armatura_player_advance` ezután lendít, beilleszt és rugóval visszaugrat |
| `int armatura_player_scroll_offset(P *, const char *id, float *x, float *y)` | a megjelenített eltolás; -1 ismeretlen id-ra; a mutatók lehetnek `NULL` |
| `int armatura_player_scroll_max(P *, const char *id, float *x, float *y)` | a legnagyobb hasznos eltolás az aktuális vászonméreten |
| `int armatura_player_scroll_settled(P *, const char *id)` | 1 nyugalomban, 0 mozgás közben, -1 ismeretlen id |
| `int armatura_player_set_screen(P *, float width_px, float height_px, float max_error_px)` | megmondja a playernek, mekkorán jelenik meg; a legdurvább előre épített részletességi szintet rendereli, amelynek deklarált hibakorlátja `max_error_px` pixelen belül marad. A szintek első használatkor épülnek (ez a hívás nagy projekten egy ideig tarthat, debug buildben sokáig). |
| `int armatura_player_lod_level(P *)` | a használatban lévő szint (0 = eredeti, képernyőméret nélkül is); negatív `NULL` playerre |

### Szerkesztés és exportálás memóriába (additív, facade 1.4)

A szerkesztések validáltak és visszavonhatók; egy elbukó szerkesztés semmit sem változtat, -1-et ad és beállítja a hibaszöveget. Nem const `E *` típust vesznek (egyszerre egy szál, mint mindig). A színek `#rgb`, `#rrggbb` vagy `#rrggbbaa` alakúak. A `parent` lehet `NULL`.

| Függvény | Leírás |
|---|---|
| `int armatura_add_rect_layer(E *, name, x, y, w, h, fill, parent)` | kitöltött téglalap-réteg hozzáfűzése; a `parent` egy legfelső szintű csoportréteget nevez meg, a `NULL` a legfelső szintre fűz |
| `int armatura_add_ellipse_layer(E *, name, cx, cy, rx, ry, fill, parent)` | ugyanez ellipszisre |
| `int armatura_add_group_layer(E *, name)` | üres csoportréteg hozzáfűzése |
| `int armatura_add_bone(E *, name, parent, length, x, y, angle)` | csont hozzáfűzése; a szülőnek léteznie kell (`NULL` = gyökér); a szög fokban |
| `int armatura_set_key(E *, target, prop, frame, value, interpolation)` | egy kulcs a `target.prop` tulajdonságon; interpoláció: 0 lineáris, 1 step, 2 ease-in, 3 ease-out, 4 ease-in-out, 5 smooth |
| `int armatura_bake_physics(E *, chain, preset, overwrite)` | egy vesszővel elválasztott csontlánc szimulálása (a gyökér elöl) `hair`, `tail`, `rope`, `antenna` vagy `cape` előbeállítással; a kiírt csatornák számát adja (0-t, ha a meglévő kulcsolt csontokat megtartotta) |
| `int armatura_undo(E *)`, `int armatura_redo(E *)` | 1 megtörtént, 0 nincs teendő, negatív hiba esetén |
| `uint32_t armatura_layer_count(const E *)`, `int armatura_layer_name(const E *, index, out, cap)` | rétegútvonalak (`group/child`), karakterpuffer-szabályok |
| `B *armatura_export(const E *, format, scale)` | teljes idővonal exportálása, `B` = `ArmaturaBlob *`; format: 0 GIF, 1 glTF bináris, 2 Lottie JSON, 3 HTML lejátszó, 4 APNG; a `scale` 0,05 és 4 között GIF-nél és APNG-nél; hiba esetén `NULL` |
| `B *armatura_render_png(const E *, frame, scale)` | egy képkocka PNG-ként |
| `B *armatura_render_exr(const E *, frame, scale, float32)` | egy képkocka OpenEXR-ként (lineáris, premultiplikált; a nem nulla `float32` FLOAT-ot tárol, egyébként HALF-ot) |
| `B *armatura_to_json(const E *)` | a (szerkesztett) projekt UTF-8 JSON-ként, terminátor nélkül |
| `const uint8_t *armatura_blob_data(const B *)`, `size_t armatura_blob_size(const B *)` | egy blob bájtjai, a felszabadításáig érvényesek; `NULL` / 0 `NULL` blobra |
| `void armatura_blob_free(B *)` | egy blob felszabadítása pontosan egyszer; a `NULL` megengedett |

## Tulajdonlás és élettartam

* **A sztringek és tömbök az általad adott pufferekbe másolódnak; az export eredményei blobok, amelyeket te szabadítasz fel.** Minden sztring- és tömberedmény az *általad* megadott pufferbe másolódik. Az egyetlen, a könyvtár tulajdonában lévő memória egy `ArmaturaBlob` az `armatura_export`, az `armatura_render_png`, az `armatura_render_exr` vagy az `armatura_to_json` függvényből: olvasd ki az `armatura_blob_data` / `armatura_blob_size` függvénnyel, majd egyszer szabadítsd fel az `armatura_blob_free` függvénnyel. Nincs általános `armatura_free`. A könyvtár által visszaadott egyetlen mutatók az `armatura_version()` (statikus) és az `armatura_last_error()` (lásd lent), valamint a két átlátszatlan handle.
* **Handle-ök.** Az `armatura_open` egy engine-handle-t ad vissza; az `armatura_player_open` egy player-handle-t. Mindegyiket pontosan egyszer kell lezárni a saját `_close` függvényével; a `NULL` lezárása rendben van; egy handle lezárás utáni használata vagy kétszeri lezárása nem definiált viselkedés.
* **Az engine és a playerek függetlenek.** A player a projekt egy másolatát birtokolja, így az engine előbbi lezárása megengedett és gyakori (a példa is ezt teszi). Egy engine-ből több player is megnyitható.
* **A te pufferei.** A bemeneti mutatóknak csak a hívás idejére kell érvényesnek lenniük; a könyvtár nem tart meg rájuk mutatót. A kimeneti puffereknek valóban rendelkezniük kell az átadott kapacitással; a könyvtár legfeljebb `cap` bájtot (sztringek) vagy `cap_floats` floatot ír.
* Az **`armatura_last_error()`** egy szálonkénti (thread-local) sztringre mutató mutatót ad. Addig érvényes, amíg ugyanazon a szálon egy következő, *elbukó* hívás felül nem írja, vagy amíg a szál ki nem lép. Másold le, ha tovább kell. Ne szabadítsd fel.

## Hibakezelés

1. Mindig teszteld a visszatérési értéket (`NULL` a két `_open` függvénynél és az `armatura_player_open`-nél, egyébként negatív `int`, `NULL` handle-lel hívott getternél `0`).
2. Ezután olvasd el az `armatura_last_error()` értékét ugyanazon a szálon az okért: például `invalid project JSON: ...`, `output buffer needs 1228800 bytes`, `invalid argument: input 'excited' is not a number`.
3. **A hibaszöveget csak a hibák állítják be, és siker sosem törli.** Egy későbbi sikeres hívás után is a régi üzenetet tartalmazza. Soha ne használd a „a sztring nem üres” feltételt hibateszként.
4. A hibaszövegek a Rust facade-ből jönnek (`ApiError` `Display`); embereknek szólnak, nem szerződés. A C ABI nem teszi elérhetővé a hiba *fajtáját*.
5. A motoron belüli pánikot elkapjuk, és `internal error (panic) inside armatura` üzenettel jelezzük a szokásos hibás visszatérés mellett; a handle elvileg használható marad, de kezeld hibaként, és jelentsd.

## Szálkezelés

* Minden handle-t **egyszerre egy szálról** lehet használni (a header szerződése). Egy handle-t szálak között mozgathatsz, amíg a hívások sosem fedik át egymást; a különböző handle-ök párhuzamosan futhatnak különböző szálakon (az alatta lévő Rust típusok `Send + Sync`, de a C szerződés óvatos marad).
* Az utolsó hiba sztringje **szálonkénti**: azon a szálon olvasd, amelyik a hibás hívást tette.
* Nincsenek callbackek, és a könyvtár nem indít saját szálakat.

## Teljes C példa

Egy projekt betöltése, az adatok és a csontpozíciók kiírása, egy képkocka renderelése PPM-fájlba (előbb a méretet kérdezve le `NULL`-lal, majd pontosan lefoglalva), a projekt első állapotgépének futtatása, az eseményeinek kiolvasása a méretlekérdezéses mintával, és a hibaág bemutatása. Gcc-vel ellenőrizve, AddressSanitizer/UBSan alatt is (nincs találat), mind a megosztott, mind a statikus könyvtárral.

```c
/* ARMATURA C ABI example: load, inspect, render a PPM, run a state machine.
 * Build:  cc -std=c99 -Wall -Wextra -Iarmatura-capi -o example example.c -Ltarget/debug -larmatura_capi
 * Run:    LD_LIBRARY_PATH=target/debug ./example project.json frame.ppm [machine-name]
 */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "armatura.h"

static char *read_file(const char *path, size_t *len) {
    FILE *f = fopen(path, "rb");
    if (!f) return NULL;
    fseek(f, 0, SEEK_END);
    long n = ftell(f);
    fseek(f, 0, SEEK_SET);
    char *buf = malloc(n > 0 ? (size_t)n : 1);
    if (!buf || fread(buf, 1, (size_t)n, f) != (size_t)n) { fclose(f); free(buf); return NULL; }
    fclose(f);
    *len = (size_t)n;
    return buf;
}

int main(int argc, char **argv) {
    if (argc < 3) { fprintf(stderr, "usage: %s project.json out.ppm [machine]\n", argv[0]); return 2; }
    if (armatura_abi_version() != ARMATURA_ABI_VERSION) { fprintf(stderr, "header and library disagree\n"); return 2; }
    printf("armatura %s, ABI %u\n", armatura_version(), armatura_abi_version());

    size_t len = 0;
    char *json = read_file(argv[1], &len);
    if (!json) { fprintf(stderr, "cannot read %s\n", argv[1]); return 3; }
    ArmaturaEngine *e = armatura_open(json, len);
    free(json);                                   /* the engine keeps its own copy */
    if (!e) { fprintf(stderr, "open failed: %s\n", armatura_last_error()); return 4; }
    printf("%ux%u, %u frames @ %.1f fps\n", armatura_width(e), armatura_height(e), armatura_frame_count(e), armatura_fps(e));

    /* bones: copy the name, then the world matrices of frame 10 */
    uint32_t nb = armatura_bone_count(e);
    float *world = malloc(sizeof(float) * 6 * (nb ? nb : 1));
    if (armatura_bone_world(e, 10.0f, world, 6 * (size_t)nb) != 0) { fprintf(stderr, "%s\n", armatura_last_error()); return 5; }
    for (uint32_t i = 0; i < nb; i++) {
        char name[64];
        armatura_bone_name(e, i, name, sizeof name);
        printf("  bone %-10s tx=%7.2f ty=%7.2f\n", name, world[i * 6 + 4], world[i * 6 + 5]);
    }
    free(world);

    /* render: ask for the size first (out == NULL), then allocate exactly */
    uint32_t w = 0, h = 0;
    int rc = armatura_render_rgba(e, 10.0f, 0.5f, NULL, 0, &w, &h);
    if (rc != -2) { fprintf(stderr, "unexpected rc %d: %s\n", rc, armatura_last_error()); return 6; }
    size_t need = (size_t)w * h * 4;
    uint8_t *px = malloc(need);
    if (armatura_render_rgba(e, 10.0f, 0.5f, px, need, &w, &h) != 0) { fprintf(stderr, "%s\n", armatura_last_error()); return 6; }
    FILE *out = fopen(argv[2], "wb");
    if (!out) return 7;
    fprintf(out, "P6\n%u %u\n255\n", w, h);       /* RGBA straight alpha -> RGB over white */
    for (size_t i = 0; i < (size_t)w * h; i++) {
        unsigned a = px[i * 4 + 3];
        for (int c = 0; c < 3; c++) fputc((px[i * 4 + c] * a + 255 * (255 - a)) / 255, out);
    }
    fclose(out);
    free(px);
    printf("wrote %s (%ux%u)\n", argv[2], w, h);

    /* state machine (only when the project has one) */
    if (armatura_machine_count(e) > 0) {
        char mname[64];
        armatura_machine_name(e, 0, mname, sizeof mname);
        const char *wanted = argc > 3 ? argv[3] : mname;
        ArmaturaPlayer *p = armatura_player_open(e, wanted);
        if (!p) { fprintf(stderr, "player: %s\n", armatura_last_error()); return 8; }
        armatura_close(e);                        /* allowed: the player owns a copy of the project */
        armatura_player_set_bool(p, "excited", 1);
        for (int i = 0; i < 30; i++) armatura_player_advance(p, 1.0 / 30.0);
        char state[64];
        armatura_player_state(p, "body", state, sizeof state);
        printf("machine %s: layer body is in state '%s'\n", wanted, state);
        int need_ev = armatura_player_events(p, NULL, 0);   /* query the JSON size; nothing consumed */
        char *ev = malloc((size_t)need_ev + 1);
        armatura_player_events(p, ev, (size_t)need_ev + 1); /* fits: events are consumed now */
        printf("events: %s\n", ev);
        free(ev);
        /* a wrong call reports through armatura_last_error() */
        if (armatura_player_set_number(p, "excited", 1.0) != 0) printf("expected error: %s\n", armatura_last_error());
        armatura_player_close(p);
    } else {
        armatura_close(e);
    }
    return 0;
}
```

Lefordítva és futtatva az `armatura demo` projekten, kiegészítve egy állapotgéppel (`hero`, `excited` bemenet, `idle` és `wave` állapotok):

```text
armatura 0.1.0, ABI 1
640x480, 48 frames @ 24.0 fps
  bone arm_up     tx= 150.00 ty= 330.00
  ...
wrote frame.ppm (320x240)
machine hero: layer body is in state 'wave'
events: [{"time":0.0,"layer":"body","kind":"entered","state":"idle"},{"time":0.016666666666666666,"layer":"body","kind":"exited","state":"idle"},{"time":0.016666666666666666,"layer":"body","kind":"entered","state":"wave"}]
expected error: invalid argument: input 'excited' is not a number
```

A repository tartalmazza az `armatura-capi/tests/smoke.c` fájlt (megnyitás, renderelés, ellenőrzőösszeg) és az `armatura-capi/tests/abi.rs` Rust-teszteket (null mutatók, rossz indexek, pufferméretek, pánikbiztonság) is.

## Görgetés és részletességi szint C-ben

Egy projekt elrendezési konténerrel és egy scroll-régióval (a Rust-tesztek scroll-tesztprojektje: tíz 20 egységes sor egy oszlopban, egy nézetablak a bal, felső és alsó élhez rögzítve). Ellenőrizve:

```c
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "armatura.h"
int main(int argc, char **argv) {
    FILE *f = fopen(argv[1], "rb"); fseek(f, 0, SEEK_END); long n = ftell(f); fseek(f, 0, SEEK_SET);
    char *json = malloc(n); if (fread(json, 1, n, f) != (size_t)n) return 1; fclose(f);
    ArmaturaEngine *e = armatura_open(json, n); free(json);
    if (!e) { printf("%s\n", armatura_last_error()); return 2; }
    ArmaturaPlayer *p = armatura_player_open(e, "hero");
    armatura_close(e);
    if (!p) { printf("%s\n", armatura_last_error()); return 3; }
    armatura_player_set_canvas(p, 100.0f, 120.0f);               /* containers re-solve for this size */
    char id[32];
    for (uint32_t i = 0; i < armatura_player_scroll_count(p); i++) {
        armatura_player_scroll_id(p, i, id, sizeof id);
        float mx, my, ox, oy;
        armatura_player_scroll_max(p, id, &mx, &my);
        armatura_player_scroll_drag(p, id, 0.0f, 500.0f);        /* far past the end: rubber band / clamp */
        armatura_player_scroll_offset(p, id, &ox, &oy);
        printf("scroll '%s': max=(%g,%g) dragged offset=(%g,%g) settled=%d\n", id, mx, my, ox, oy, armatura_player_scroll_settled(p, id));
        armatura_player_scroll_release(p, id, 0.0f, 0.0f);
        for (int k = 0; k < 60; k++) armatura_player_advance(p, 1.0 / 60.0);
        armatura_player_scroll_offset(p, id, &ox, &oy);
        printf("  after release: offset=(%g,%g) settled=%d\n", ox, oy, armatura_player_scroll_settled(p, id));
    }
    int rc = armatura_player_set_screen(p, 100.0f, 120.0f, 4.0f);   /* accept up to 4 px of error */
    printf("set_screen rc=%d, lod level=%d\n", rc, armatura_player_lod_level(p));
    uint32_t w, h; armatura_player_render_rgba(p, 1.0f, NULL, 0, &w, &h);
    printf("render size %ux%u\n", w, h);
    armatura_player_close(p);
    return 0;
}
```

Kimenet: `scroll 'list': max=(0,80) dragged offset=(0,80) settled=1` (az 500-ig húzás a maximumnál szorít, ha a rugalmasság ki van kapcsolva), majd `set_screen rc=0, lod level=7` (4 px-ig elfogadott hibával ezen az apró projekten a legdurvább előre épített szint megfelel; a szám a projekttől függ) és `render size 100x120`. Figyeld meg a forrásban az explicit `rc` változót: a C nem határozza meg a függvényargumentumok kiértékelési sorrendjét, ezért soha ne tedd a playert módosító és az azt olvasó hívást egyetlen `printf`-be.

## Python (ctypes) példa

Csak a szabványkönyvtár. Minden hívott függvényhez deklaráld az `argtypes` és a `restype` értékét: a ctypes egyébként `int`-et feltételez, és csonkítja a 64 bites mutatókat. Python 3-on, a debug könyvtárral ellenőrizve; a PPM-kimenete bájtra megegyezik a C példáéval.

```python
#!/usr/bin/env python3
"""ARMATURA C ABI from Python with ctypes (standard library only).

    python3 example.py libarmatura_capi.so project.json out.ppm
"""
import ctypes as C
import sys

lib = C.CDLL(sys.argv[1])

# --- declare every signature you use (ctypes defaults to int and would truncate pointers) ---
lib.armatura_version.restype = C.c_char_p
lib.armatura_abi_version.restype = C.c_uint32
lib.armatura_last_error.restype = C.c_char_p
lib.armatura_open.argtypes = [C.c_char_p, C.c_size_t]
lib.armatura_open.restype = C.c_void_p                      # opaque handle
lib.armatura_close.argtypes = [C.c_void_p]
for name, res in (("frame_count", C.c_uint32), ("fps", C.c_float), ("width", C.c_uint32),
                  ("height", C.c_uint32), ("bone_count", C.c_uint32), ("machine_count", C.c_uint32)):
    f = getattr(lib, "armatura_" + name)
    f.argtypes = [C.c_void_p]
    f.restype = res
lib.armatura_bone_name.argtypes = [C.c_void_p, C.c_uint32, C.c_char_p, C.c_size_t]
lib.armatura_bone_name.restype = C.c_int
lib.armatura_bone_world.argtypes = [C.c_void_p, C.c_float, C.POINTER(C.c_float), C.c_size_t]
lib.armatura_bone_world.restype = C.c_int
lib.armatura_render_rgba.argtypes = [C.c_void_p, C.c_float, C.c_float, C.c_void_p, C.c_size_t,
                                     C.POINTER(C.c_uint32), C.POINTER(C.c_uint32)]
lib.armatura_render_rgba.restype = C.c_int


def last_error() -> str:
    return (lib.armatura_last_error() or b"").decode("utf-8", "replace")


class Engine:
    """Owns one handle; use as a context manager so armatura_close runs exactly once."""

    def __init__(self, json_bytes: bytes):
        self.h = lib.armatura_open(json_bytes, len(json_bytes))
        if not self.h:
            raise ValueError(last_error())

    def close(self):
        if self.h:
            lib.armatura_close(self.h)
            self.h = None

    __enter__ = lambda self: self
    __exit__ = lambda self, *a: self.close()

    def bones(self):
        out = []
        for i in range(lib.armatura_bone_count(self.h)):
            buf = C.create_string_buffer(128)
            lib.armatura_bone_name(self.h, i, buf, len(buf))
            out.append(buf.value.decode())
        return out

    def bone_world(self, frame: float):
        n = lib.armatura_bone_count(self.h)
        arr = (C.c_float * (6 * n))()
        if lib.armatura_bone_world(self.h, frame, arr, len(arr)) != 0:
            raise RuntimeError(last_error())
        return [tuple(arr[i * 6:i * 6 + 6]) for i in range(n)]

    def render(self, frame: float, scale: float = 1.0):
        w, h = C.c_uint32(), C.c_uint32()
        rc = lib.armatura_render_rgba(self.h, frame, scale, None, 0, C.byref(w), C.byref(h))
        if rc != -2:                                       # -2 = "buffer too small", and w/h are filled in
            raise RuntimeError(last_error())
        buf = (C.c_uint8 * (w.value * h.value * 4))()
        if lib.armatura_render_rgba(self.h, frame, scale, buf, len(buf), C.byref(w), C.byref(h)) != 0:
            raise RuntimeError(last_error())
        return w.value, h.value, bytes(buf)


if __name__ == "__main__":
    print("armatura", lib.armatura_version().decode(), "ABI", lib.armatura_abi_version())
    data = open(sys.argv[2], "rb").read()
    try:
        Engine(b"{ not json")
    except ValueError as e:
        print("expected error:", e)
    with Engine(data) as e:
        print(lib.armatura_width(e.h), "x", lib.armatura_height(e.h), lib.armatura_frame_count(e.h), "frames")
        print("bones:", e.bones())
        print("arm_up world @10:", [round(v, 2) for v in e.bone_world(10.0)[0]])
        w, h, rgba = e.render(10.0, 0.5)
        with open(sys.argv[3], "wb") as f:
            f.write(b"P6\n%d %d\n255\n" % (w, h))
            for i in range(0, len(rgba), 4):
                r, g, b, a = rgba[i:i + 4]
                f.write(bytes((c * a + 255 * (255 - a)) // 255 for c in (r, g, b)))
        print("wrote", sys.argv[3], w, h)
```

## Más nyelvek

Bármi, ami C függvényt tud hívni, ugyanígy működik: deklaráld az 53 függvényt, tartsd az átlátszatlan mutatókat `void *`-ként, és kövesd a fenti pufferkonvenciókat. A gyakori gazdákra jellemző megjegyzések (itt nem tesztelve, csak a fenti szerződés következményei): C#/.NET-ben használd a `[DllImport]`-ot `CallingConvention.Cdecl` mellett, és a sztringeket UTF-8 bájttömbként marshalold; Go-ban cgo-val tartsd a Go-mutatókat a hívásokon kívül, kivéve a hívás idejére élő puffereket; Rustban az ABI helyett a facade-et használd közvetlenül.

## Verziózás

* Az `ARMATURA_ABI_VERSION` / `armatura_abi_version()` jelenleg **1**. Csak akkor változik, ha egy meglévő függvényt eltávolítanak vagy megváltozik a jelentése. Minden eddigi kiegészítés (a player függvények a facade 1.1-ben, az elrendezés, görgetés és részletességi szint az 1.2-ben) additív volt, és 1-en hagyta, így egy régebbi headerrel épített program egy újabb könyvtárral is tovább működik, egy újabb header egy régebbi könyvtárral pedig a hiányzó szimbólum miatt linkelési vagy betöltési időben elbukik. Indításkor ellenőrizd az `armatura_abi_version()` értékét, és hasonlítsd össze a makróval.
* Az `armatura_version()` az `armatura-capi` crate verziója (0.1.0), nem a facade verziója. Nincs C függvény, amely a facade verzióját (`API_VERSION`) jelentené.
* A facade 1.4 hozzáadta a fenti szerkesztési alapokat és a memóriába exportáló függvényeket (additív, az ABI-verzió továbbra is 1). A C ABI-ban nem érhető el: az SVG-renderelés, a kulcsok törlése, a verziókezelés, az animáció-CI, a vektorizálás, a mocap, a fuzzing, a méretcélzott exportálás, a szkriptek. Ezek Rustban és a CLI-n keresztül elérhetők.

## Ismert furcsaságok

A header megjegyzéseit a viselkedéshez igazítottuk (korábban eltértek); a tudnivalók:

* Az `armatura_render_rgba` a `*w` / `*h` értéket sikeres hívásnál és -2-nél (puffer túl kicsi) is beírja, valamint akkor is, ha a `frame` és a `scale` elfogadása után később renderelési hiba lép fel. **Nem** írja be, ha az engine null, vagy ha a `frame` / `scale` elutasításra kerül (-1). Regressziós teszt: `documented_size_outputs_and_last_error_semantics` az `armatura-capi/tests/abi.rs` fájlban.
* Az utolsó hiba szövege szálonkénti, csak a következő *elbukó* hívás írja felül, és siker sosem törli. Másold le, ha tovább kell.
* Az `armatura_width` / `armatura_height` egészre kerekíti a lebegőpontos színpadméretet; a valódi renderméret `round(size * scale)`, ezért használd a renderhívás által visszaadott `w` / `h` értékeket.
* A header megjegyzése arról, hogy a scroll-régiók követik az `armatura_player_set_value` értékét, a görgetés- és a részletességi szint-deklarációk között áll; az `armatura_player_set_value` függvényt írja le.
