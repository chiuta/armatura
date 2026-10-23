# ARMATURA-C-ABI

*ABI-Version 1 (`ARMATURA_ABI_VERSION`, `armatura_abi_version()`). Crate `armatura-capi`, Header `armatura-capi/armatura.h`. Zielgruppe: Integratoren, die ARMATURA-Projekte aus C, C++, Python, Go, C#, Swift oder jeder anderen Sprache mit C-Foreign-Function-Interface laden und abspielen wollen. Dokumentation CC-BY-4.0; Codebeispiele MPL-2.0.*

Die C-ABI ist ein kleiner Wrapper in reinem C über der stabilen Rust-Fassade ([API.md](API.md)). Damit kann ein Hostprogramm ein Projekt öffnen, dessen Fakten und Knochentransformationen auslesen, Frames als RGBA rendern und **Zustandsautomaten** ausführen (interaktive Wiedergabe, Scrollen, Detailstufen). Seit Fassade 1.4 kann es außerdem einfache **Bearbeitungen** vornehmen (Ebenen und Knochen erstellen, Keys setzen, Physik backen, Undo) und **in den Speicher exportieren** (GIF, glTF, Lottie, HTML, APNG, PNG, EXR, Projekt-JSON). Versionskontrolle und die Werkzeuge kann es nicht ausführen: Verwende dafür die Rust-API oder die Kommandozeile ([CLI_REFERENCE.md](CLI_REFERENCE.md)).

Inhalt: [Bauen und Linken](#bauen-und-linken) - [Konventionen](#konventionen) - [Funktionsreferenz](#funktionsreferenz) - [Eigentümerschaft und Lebensdauer](#eigentümerschaft-und-lebensdauer) - [Fehlerbehandlung](#fehlerbehandlung) - [Threading](#threading) - [Vollständiges C-Beispiel](#vollständiges-c-beispiel) - [Scrollen und Detailstufen in C](#scrollen-und-detailstufen-in-c) - [Python-Beispiel (ctypes)](#python-beispiel-ctypes) - [Andere Sprachen](#andere-sprachen) - [Versionierung](#versionierung) - [Bekannte Eigenheiten](#bekannte-eigenheiten)

## Bauen und Linken

Das Crate baut drei Bibliothekstypen (`crate-type = ["cdylib", "staticlib", "rlib"]`); es hängt mit `default-features = false` von `armatura-core` ab und enthält daher keine Scripting-Engine und kein MPL-Third-Party-Crate.

```sh
cargo build -p armatura-capi             # debug:   target/debug/libarmatura_capi.{so,a}
cargo build --release -p armatura-capi   # release: target/release/libarmatura_capi.{so,a}
```

| Plattform | Dynamische Bibliothek | Statische Bibliothek |
|---|---|---|
| Linux | `libarmatura_capi.so` | `libarmatura_capi.a` |
| macOS | `libarmatura_capi.dylib` | `libarmatura_capi.a` |
| Windows | `armatura_capi.dll` (+ Importbibliothek) | `armatura_capi.lib` |

**Getestet:** Linux x86-64, gcc, sowohl die gemeinsam genutzte als auch die statische Bibliothek (siehe unten). macOS und Windows sind die üblichen Rust-Ausgabenamen für diese Targets, wurden vom Autor aber **weder gebaut noch ausgeführt**. Eine Debug-`.so` ist etwa 150 MB groß (Debug-Informationen); baue für alles, was du auslieferst, mit `--release` (die Größe der Release-Bibliothek wurde hier nicht gemessen).

Kompilieren und gegen die gemeinsam genutzte Bibliothek linken:

```sh
cc -std=c99 -Wall -Wextra -Iarmatura-capi -o example example.c -Ltarget/debug -larmatura_capi
LD_LIBRARY_PATH=target/debug ./example project.json frame.ppm      # or install the .so and use rpath / ldconfig
```

Statisches Linken (unter Linux verifiziert; die Rust-Standardbibliothek benötigt diese Systembibliotheken):

```sh
cc -std=c99 -Iarmatura-capi -o example example.c target/debug/libarmatura_capi.a -lpthread -ldl -lm
```

`armatura.h` hat `extern "C"`-Guards für C++, braucht nur `<stddef.h>` und `<stdint.h>` und ist die einzige Datei, die du einbindest. Kopiere sie neben deine Quellen. Es gibt keine pkg-config-Datei, kein Install-Target und keinen versionierten SONAME; liefere die Bibliothek zusammen mit deiner Anwendung aus.

Panics werden an jedem Einstiegspunkt abgefangen und in Fehlercodes umgewandelt, was darauf beruht, dass die Bibliothek mit dem Standardwert `panic = "unwind"` gebaut wird. Baue sie nicht mit `panic = "abort"` (dann würde ein Fehler in der Engine deinen Prozess abbrechen, statt -1 zurückzugeben). Der Workspace setzt diese Profiloption nicht.

## Konventionen

* Strings sind UTF-8. Eingabestrings (`const char *`) sind NUL-terminiert, mit Ausnahme des Projekt-JSON, das als Zeiger plus Länge übergeben wird und keinen Terminator braucht.
* Funktionen, die `int` zurückgeben, liefern **0 bei Erfolg und einen negativen Wert bei einem Fehler**. `-1` ist ein allgemeiner Fehler; `-2` bedeutet zusätzlich „dein Ausgabepuffer ist zu klein“ und tritt nur bei den beiden Render-Funktionen auf.
* Funktionen, die einen Zeichenpuffer füllen, nehmen `(char *out, size_t cap)` und geben die **volle Länge** des Textes zurück (ohne das NUL). Der Text wird auf `cap - 1` Bytes gekürzt und bei `cap > 0` NUL-terminiert. Übergib `out == NULL` (mit beliebigem `cap`), um nur die Länge zu erfragen. Ein negativer Rückgabewert ist ein Fehler (ungültiger Index, unbekannter Name).
* Float-Argumente müssen endlich sein; Frames außerhalb der Timeline werden begrenzt; `scale` muss zwischen 0.05 und 4 liegen.
* Jedes Handle-Argument darf bei den Gettern `NULL` sein (sie geben 0 zurück und setzen den Fehlertext) und ist bei allen anderen ein Fehler.
* Das ausgegebene RGBA ist **Straight-Alpha** (nicht premultiplied), 8 Bit pro Kanal, zeilenweise, oberste Zeile zuerst, `width * height * 4` Bytes.
* Welttransformationen von Knochen sind sechs Floats pro Knochen `[a b c d tx ty]`; ein knochenlokaler Punkt `(x, y)` wird auf `(a*x + c*y + tx, b*x + d*y + ty)` abgebildet.

## Funktionsreferenz

53 Funktionen. `E` = `ArmaturaEngine *`, `P` = `ArmaturaPlayer *`.

### Bibliothek

| Funktion | Beschreibung |
|---|---|
| `const char *armatura_version(void)` | Versionsstring des Crates `armatura-capi` (derzeit `"0.1.0"`); statisch, nie freizugeben |
| `uint32_t armatura_abi_version(void)` | ABI-Version der geladenen Bibliothek; mit `ARMATURA_ABI_VERSION` aus dem Header vergleichen, gegen den du kompiliert hast |
| `const char *armatura_last_error(void)` | Text des letzten Fehlers **auf diesem Thread**; leerer String, wenn noch keiner aufgetreten ist; siehe [Fehlerbehandlung](#fehlerbehandlung) |

### Engine: öffnen, abfragen, rendern

| Funktion | Beschreibung |
|---|---|
| `E *armatura_open(const char *json, size_t len)` | parst und validiert ein Projekt (UTF-8, `len` Bytes, 1 bis 256 MiB); gibt bei einem Fehler `NULL` zurück. Die Bibliothek kopiert, was sie braucht; du kannst `json` sofort freigeben. Relative Bilddateien im Projekt werden im aktuellen Verzeichnis des Prozesses gesucht, absolute Pfade und `..` werden abgelehnt (siehe [SECURITY.md](SECURITY.md)); Data-URI-Bilder funktionieren immer. |
| `void armatura_close(E *)` | gibt die Engine frei; `NULL` ist erlaubt; pro Handle genau einmal aufrufen |
| `uint32_t armatura_frame_count(const E *)` | Anzahl der Frames |
| `float armatura_fps(const E *)` | Frames pro Sekunde |
| `uint32_t armatura_width(const E *)`, `armatura_height(const E *)` | Bühnengröße in Bühneneinheiten, auf eine ganze Zahl gerundet |
| `uint32_t armatura_bone_count(const E *)` | Anzahl der Knochen |
| `int armatura_bone_name(const E *, uint32_t index, char *out, size_t cap)` | Name des Knochens `index` (Rig-Reihenfolge); Länge, oder -1 bei ungültigem Index |
| `int armatura_bone_world(const E *, float frame, float *out, size_t cap_floats)` | Weltmatrizen aller Knochen; `cap_floats` muss mindestens `6 * bone_count` betragen, sonst -1 |
| `int armatura_render_rgba(const E *, float frame, float scale, uint8_t *out, size_t cap, uint32_t *w, uint32_t *h)` | rendert einen Frame; Ausgabegröße `round(width*scale) x round(height*scale)`; gibt 0 zurück, **-2, wenn `cap` kleiner als `w*h*4` ist** (dann sind `*w` und `*h` gesetzt, sodass du allozieren und erneut aufrufen kannst; übergib `out == NULL`, um nachzufragen), -1 bei anderen Fehlern. `w` und `h` dürfen `NULL` sein. |

### Player: Zustandsautomaten

Ein Projekt kann `machines` (Laufzeit-Zustandsautomaten) und `bindings` (externe Werte, die Eigenschaften steuern) enthalten. Ein Player führt einen Automaten aus: Eingaben per Name setzen, die Zeit weiterschalten, Zustände und Ereignisse lesen, die aktuelle Pose rendern. Die Semantik (es sind die `Player`-Methoden) findest du in [API.md](API.md).

| Funktion | Beschreibung |
|---|---|
| `uint32_t armatura_machine_count(const E *)` | Automaten im Projekt |
| `int armatura_machine_name(const E *, uint32_t index, char *out, size_t cap)` | Name des Automaten `index` (Regeln für Zeichenpuffer) |
| `P *armatura_player_open(const E *, const char *machine)` | startet den Automaten bei Zeit 0; `NULL` bei einem Fehler (unbekannter Automat, ...). Der Player besitzt eine **Kopie** des Projekts: Er bleibt nach `armatura_close` der Engine gültig. |
| `void armatura_player_close(P *)` | gibt den Player frei; `NULL` erlaubt |
| `int armatura_player_set_bool(P *, const char *name, int value)` | Bool-Eingabe (`value != 0`) |
| `int armatura_player_set_number(P *, const char *name, double value)` | Zahlen-Eingabe (endlich) |
| `int armatura_player_fire(P *, const char *trigger)` | löst einen Trigger aus; er wird von dem Übergang verbraucht, der ihn prüft |
| `int armatura_player_set_value(P *, const char *path, double value)` | externer Wert für Datenbindungen, z. B. `"player.health"`; steuert auch eine Scroll-Region, deren `input` dieser Pfad ist (Pixel entlang ihrer Hauptachse) |
| `int armatura_player_advance(P *, double dt)` | schaltet um `dt` Sekunden weiter, `0..=10`; deterministisch (lange Schritte werden in Stücke von höchstens 1/60 s zerlegt) |
| `int armatura_player_state(P *, const char *layer, char *out, size_t cap)` | aktueller Zustandsname einer Automatenebene; Länge, oder -1 bei unbekannter Ebene |
| `int armatura_player_events(P *, char *out, size_t cap)` | anstehende Ereignisse als JSON-Array `[{"time":..,"layer":..,"kind":"entered"\|"exited","state":..}]`. Gibt die volle Länge zurück. **Die Ereignisse werden nur verbraucht, wenn der gesamte Text hineinpasste (`cap` > Länge)**; andernfalls bleiben sie erhalten, und du rufst erneut mit einem größeren Puffer auf. `out == NULL` erfragt die Größe und verbraucht nichts. |
| `int armatura_player_render_rgba(P *, float scale, uint8_t *out, size_t cap, uint32_t *w, uint32_t *h)` | derselbe Vertrag wie bei `armatura_render_rgba`, für die aktuelle Pose; die Größe folgt dem Canvas des Players |

### Player: Layout, Scrollen, Detailstufen

| Funktion | Beschreibung |
|---|---|
| `int armatura_player_set_canvas(P *, float width, float height)` | responsives Layout: Canvas-Größe in Bühneneinheiten (endlich, positiv, höchstens 32768); Container werden neu gelöst, Renderings erhalten diese Größe |
| `int armatura_player_canvas_size(P *, float *w, float *h)` | aktuelle Canvas-Größe; jeder der beiden Zeiger darf `NULL` sein |
| `uint32_t armatura_player_scroll_count(P *)` | Scroll-Regionen des Projekts |
| `int armatura_player_scroll_id(P *, uint32_t index, char *out, size_t cap)` | ID der Region `index` (Regeln für Zeichenpuffer) |
| `int armatura_player_scroll_drag(P *, const char *id, float x, float y)` | roher Zeiger-Offset in Pixeln (Inhalt nach oben / links verschoben); über die Enden hinaus wird begrenzt oder gummibandartig gedehnt |
| `int armatura_player_scroll_release(P *, const char *id, float vx, float vy)` | Loslass-Geschwindigkeit in px/s; `armatura_player_advance` lässt dann ausgleiten, einrasten und zurückfedern |
| `int armatura_player_scroll_offset(P *, const char *id, float *x, float *y)` | angezeigter Offset; -1 bei unbekannter ID; Zeiger dürfen `NULL` sein |
| `int armatura_player_scroll_max(P *, const char *id, float *x, float *y)` | größter sinnvoller Offset bei der aktuellen Canvas-Größe |
| `int armatura_player_scroll_settled(P *, const char *id)` | 1 in Ruhe, 0 in Bewegung, -1 bei unbekannter ID |
| `int armatura_player_set_screen(P *, float width_px, float height_px, float max_error_px)` | teilt dem Player mit, wie groß er angezeigt wird; er rendert die gröbste vorgebaute Detailstufe, deren deklarierte Fehlerschranke innerhalb von `max_error_px` Pixeln bleibt. Die Stufen werden bei der ersten Verwendung gebaut (dieser Aufruf kann bei einem großen Projekt einen Moment dauern, in einem Debug-Build lange). |
| `int armatura_player_lod_level(P *)` | verwendete Stufe (0 = Original, auch ohne Bildschirmgröße); negativ bei einem `NULL`-Player |

### Bearbeiten und Export in den Speicher (additiv, Fassade 1.4)

Bearbeitungen sind validiert und rückgängig machbar; eine fehlgeschlagene Bearbeitung ändert nichts, gibt -1 zurück und setzt den Fehlertext. Sie nehmen ein nicht-konstantes `E *` (wie immer ein Thread zur Zeit). Farben sind `#rgb`, `#rrggbb` oder `#rrggbbaa`. `parent` darf `NULL` sein.

| Funktion | Beschreibung |
|---|---|
| `int armatura_add_rect_layer(E *, name, x, y, w, h, fill, parent)` | hängt eine gefüllte Rechteckebene an; `parent` benennt eine Gruppenebene der obersten Ebene, bei `NULL` wird auf der obersten Ebene angehängt |
| `int armatura_add_ellipse_layer(E *, name, cx, cy, rx, ry, fill, parent)` | dasselbe für eine Ellipse |
| `int armatura_add_group_layer(E *, name)` | hängt eine leere Gruppenebene an |
| `int armatura_add_bone(E *, name, parent, length, x, y, angle)` | hängt einen Knochen an; der Elternknochen muss existieren (`NULL` = Wurzel); Winkel in Grad |
| `int armatura_set_key(E *, target, prop, frame, value, interpolation)` | ein Key auf `target.prop`; Interpolation 0 linear, 1 step, 2 ease-in, 3 ease-out, 4 ease-in-out, 5 smooth |
| `int armatura_bake_physics(E *, chain, preset, overwrite)` | simuliert eine kommagetrennte Knochenkette (Wurzel zuerst) mit `hair`, `tail`, `rope`, `antenna` oder `cape`; gibt die geschriebenen Kanäle zurück (0, wenn vorhandene gekeyte Knochen erhalten blieben) |
| `int armatura_undo(E *)`, `int armatura_redo(E *)` | 1 erledigt, 0 nichts zu tun, negativ bei einem Fehler |
| `uint32_t armatura_layer_count(const E *)`, `int armatura_layer_name(const E *, index, out, cap)` | Ebenenpfade (`group/child`), Regeln für Zeichenpuffer |
| `B *armatura_export(const E *, format, scale)` | Export der gesamten Timeline, `B` = `ArmaturaBlob *`; Format 0 GIF, 1 binäres glTF, 2 Lottie-JSON, 3 HTML-Player, 4 APNG; `scale` 0.05 bis 4 für GIF und APNG; `NULL` bei einem Fehler |
| `B *armatura_render_png(const E *, frame, scale)` | ein Frame als PNG |
| `B *armatura_render_exr(const E *, frame, scale, float32)` | ein Frame als OpenEXR (linear, premultiplied; ein von null verschiedenes `float32` speichert FLOAT, sonst HALF) |
| `B *armatura_to_json(const E *)` | das (bearbeitete) Projekt als UTF-8-JSON, ohne Terminator |
| `const uint8_t *armatura_blob_data(const B *)`, `size_t armatura_blob_size(const B *)` | die Bytes eines Blobs, gültig bis zu seiner Freigabe; `NULL` / 0 bei einem `NULL`-Blob |
| `void armatura_blob_free(B *)` | gibt einen Blob genau einmal frei; `NULL` ist erlaubt |

## Eigentümerschaft und Lebensdauer

* **Strings und Arrays werden in Puffer kopiert, die du bereitstellst; Export-Ergebnisse sind Blobs, die du freigibst.** Jedes String- und Array-Ergebnis wird in einen Puffer kopiert, den *du* bereitstellst. Der einzige Speicher im Besitz der Bibliothek ist ein `ArmaturaBlob` aus `armatura_export`, `armatura_render_png`, `armatura_render_exr` oder `armatura_to_json`: Lies ihn mit `armatura_blob_data` / `armatura_blob_size` und gib ihn dann einmal mit `armatura_blob_free` frei. Es gibt kein generisches `armatura_free`. Die einzigen Zeiger, die die Bibliothek zurückgibt, sind `armatura_version()` (statisch) und `armatura_last_error()` (siehe unten) sowie die beiden opaken Handles.
* **Handles.** `armatura_open` gibt ein Engine-Handle zurück; `armatura_player_open` gibt ein Player-Handle zurück. Jedes muss genau einmal mit seinem eigenen `_close` geschlossen werden; das Schließen von `NULL` ist in Ordnung; die Verwendung eines Handles nach dem Schließen oder doppeltes Schließen ist undefiniertes Verhalten.
* **Engine und Player sind unabhängig.** Ein Player besitzt eine Kopie des Projekts, daher ist es erlaubt und üblich, die Engine zuerst zu schließen (das Beispiel tut das). Aus einer Engine können mehrere Player geöffnet werden.
* **Deine Puffer.** Eingabezeiger müssen nur für die Dauer des Aufrufs gültig sein; die Bibliothek behält keinen Zeiger darauf. Ausgabepuffer müssen wirklich die Kapazität haben, die du übergibst; die Bibliothek schreibt höchstens `cap` Bytes (Strings) oder `cap_floats` Floats.
* **`armatura_last_error()`** gibt einen Zeiger auf einen Thread-lokalen String zurück. Er bleibt gültig bis zum nächsten *fehlschlagenden* Aufruf auf demselben Thread (dieser Aufruf überschreibt ihn) oder bis der Thread endet. Kopiere ihn, wenn du ihn länger brauchst. Gib ihn nicht frei.

## Fehlerbehandlung

1. Teste immer den Rückgabewert (`NULL` bei den beiden `_open`-Funktionen und bei `armatura_player_open`, sonst ein negatives `int`, `0` von einem Getter, dem ein `NULL`-Handle übergeben wurde).
2. Lies dann `armatura_last_error()` auf demselben Thread für den Grund: zum Beispiel `invalid project JSON: ...`, `output buffer needs 1228800 bytes`, `invalid argument: input 'excited' is not a number`.
3. **Der Fehlertext wird nur durch Fehler gesetzt und nie durch einen Erfolg gelöscht.** Nach einem späteren erfolgreichen Aufruf enthält er noch die alte Meldung. Verwende niemals „der String ist nicht leer“ als Fehlertest.
4. Fehlertexte stammen aus der Rust-Fassade (`ApiError` `Display`); sie sind für Menschen gedacht, kein Vertrag. Die C-ABI stellt die *Art* des Fehlers nicht bereit.
5. Eine Panik innerhalb der Engine wird abgefangen und als `internal error (panic) inside armatura` mit dem üblichen Fehler-Rückgabewert gemeldet; das Handle bleibt prinzipiell verwendbar, aber behandle es als Bug und melde ihn.

## Threading

* Jedes Handle darf **von einem Thread zur Zeit** verwendet werden (der Vertrag des Headers). Du darfst ein Handle zwischen Threads verschieben, solange sich Aufrufe nie überlappen; verschiedene Handles können parallel auf verschiedenen Threads laufen (die zugrunde liegenden Rust-Typen sind `Send + Sync`, aber der C-Vertrag bleibt konservativ).
* Der Last-Error-String ist **pro Thread**: Lies ihn auf dem Thread, der den fehlschlagenden Aufruf gemacht hat.
* Es gibt keine Callbacks, und die Bibliothek startet keine eigenen Threads.

## Vollständiges C-Beispiel

Lädt ein Projekt, gibt Fakten und Knochenpositionen aus, rendert einen Frame in eine PPM-Datei (zuerst mit `NULL` die Größe erfragen, dann exakt allozieren), führt den ersten Zustandsautomaten des Projekts aus, liest dessen Ereignisse mit dem Muster der Größenabfrage und zeigt den Fehlerpfad. Verifiziert mit gcc, auch unter AddressSanitizer/UBSan (keine Befunde), sowohl gegen die gemeinsam genutzte als auch gegen die statische Bibliothek.

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

Gebaut und ausgeführt gegen das Projekt aus `armatura demo`, erweitert um einen Zustandsautomaten (`hero`, Eingabe `excited`, Zustände `idle` und `wave`):

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

Das Repository liefert außerdem `armatura-capi/tests/smoke.c` (öffnen, rendern, Prüfsumme) und die Rust-Tests `armatura-capi/tests/abi.rs` (Nullzeiger, ungültige Indizes, Puffergrößen, Panik-Sicherheit) mit.

## Scrollen und Detailstufen in C

Ein Projekt mit einem Layout-Container und einer Scroll-Region (das Scroll-Testprojekt der Rust-Tests: zehn Zeilen à 20 Einheiten in einer Spalte, ein Viewport, der an den linken, oberen und unteren Rand gepinnt ist). Verifiziert:

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

Ausgabe: `scroll 'list': max=(0,80) dragged offset=(0,80) settled=1` (das Ziehen auf 500 begrenzt beim Maximum, wenn die Elastizität aus ist), dann `set_screen rc=0, lod level=7` (bei akzeptierten bis zu 4 px Fehler qualifiziert sich auf diesem winzigen Projekt die gröbste vorgebaute Stufe; die Zahl hängt vom Projekt ab) und `render size 100x120`. Beachte die explizite Variable `rc` im Quelltext: Die Reihenfolge, in der C Funktionsargumente auswertet, ist nicht festgelegt, setze daher niemals den Aufruf, der den Player ändert, und den Aufruf, der ihn liest, in dasselbe `printf`.

## Python-Beispiel (ctypes)

Nur die Standardbibliothek. Deklariere `argtypes` und `restype` für jede Funktion, die du aufrufst: ctypes nimmt sonst `int` an und kürzt 64-Bit-Zeiger. Verifiziert unter Python 3 gegen die Debug-Bibliothek; die PPM-Ausgabe ist Byte für Byte identisch mit der des C-Beispiels.

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

## Andere Sprachen

Alles, was eine C-Funktion aufrufen kann, funktioniert auf dieselbe Weise: Deklariere die 53 Funktionen, halte die opaken Zeiger als `void *` und befolge die oben genannten Puffer-Konventionen. Hinweise, die für verbreitete Hosts spezifisch sind (hier ungetestet, nur Folgerungen aus dem obigen Vertrag): Verwende in C#/.NET `[DllImport]` mit `CallingConvention.Cdecl` und marshalle Strings als UTF-8-Byte-Arrays; halte in Go mit cgo Go-Zeiger aus den Aufrufen heraus, außer bei Puffern, die für die Dauer des Aufrufs leben; verwende in Rust lieber direkt die Fassade statt dieser ABI.

## Versionierung

* `ARMATURA_ABI_VERSION` / `armatura_abi_version()` ist derzeit **1**. Sie ändert sich nur, wenn eine bestehende Funktion entfernt wird oder sich ihre Bedeutung ändert. Alle bisherigen Ergänzungen (die Player-Funktionen in Fassade 1.1, Layout, Scrollen und Detailstufen in 1.2) waren additiv und ließen sie bei 1, sodass ein gegen einen älteren Header gebautes Programm mit einer neueren Bibliothek weiterläuft, während ein neuerer Header gegen eine ältere Bibliothek beim Linken oder Laden am fehlenden Symbol scheitert. Prüfe `armatura_abi_version()` beim Start und vergleiche sie mit dem Makro.
* `armatura_version()` ist die Version des Crates `armatura-capi` (0.1.0), nicht die Fassadenversion. Es gibt keine C-Funktion, die die Fassadenversion (`API_VERSION`) meldet.
* Fassade 1.4 hat die oben genannten Funktionen für grundlegendes Bearbeiten und den Export in den Speicher hinzugefügt (additiv, ABI-Version weiterhin 1). In der C-ABI nicht bereitgestellt: SVG-Rendering, Löschen von Keys, Versionskontrolle, Animations-CI, Tracing, Mocap, Fuzzing, größenorientierter Export, Skripte. Sie sind in Rust und über die CLI verfügbar.

## Bekannte Eigenheiten

Die Kommentare im Header wurden korrigiert, damit sie zum Verhalten passen (zuvor widersprachen sie sich); die wissenswerten Punkte:

* `armatura_render_rgba` schreibt `*w` / `*h` bei Erfolg und bei -2 (Puffer zu klein) und auch dann, wenn nach der Annahme von `frame` und `scale` ein späterer Render-Fehler auftritt. Es schreibt sie **nicht**, wenn die Engine null ist oder wenn `frame` / `scale` abgelehnt werden (-1). Regressionstest: `documented_size_outputs_and_last_error_semantics` in `armatura-capi/tests/abi.rs`.
* Der Last-Error-Text ist Thread-lokal, wird nur vom nächsten *fehlschlagenden* Aufruf überschrieben und nie durch einen Erfolg gelöscht. Kopiere ihn, wenn du ihn länger brauchst.
* `armatura_width` / `armatura_height` runden die Float-Bühnengröße auf eine ganze Zahl; die tatsächliche Render-Größe ist `round(size * scale)`, verwende daher die vom Render-Aufruf zurückgegebenen `w` / `h`.
* Der Hinweis im Header, dass Scroll-Regionen `armatura_player_set_value` folgen, steht zwischen den Deklarationen für Scrollen und Detailstufen; er beschreibt `armatura_player_set_value`.
