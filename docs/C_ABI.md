# ARMATURA C ABI

*ABI version 1 (`ARMATURA_ABI_VERSION`, `armatura_abi_version()`). Crate `armatura-capi`, header `armatura-capi/armatura.h`. Audience: integrators who want to load and play ARMATURA projects from C, C++, Python, Go, C#, Swift or any language with a C foreign-function interface. Documentation CC-BY-4.0; code samples MPL-2.0.*

The C ABI is a small, plain-C wrapper over the stable Rust facade ([API.md](API.md)). With it a host program can open a project, read its facts and bone transforms, render frames to RGBA and run **state machines** (interactive playback, scrolling, level of detail). Since facade 1.4 it can also make basic **edits** (create layers and bones, set keys, bake physics, undo) and **export to memory** (GIF, glTF, Lottie, HTML, APNG, PNG, EXR, project JSON). It cannot version-control or run the tools: use the Rust API or the command line ([CLI_REFERENCE.md](CLI_REFERENCE.md)) for those.

Contents: [Build and link](#build-and-link) - [Conventions](#conventions) - [Function reference](#function-reference) - [Ownership and lifetime](#ownership-and-lifetime) - [Error handling](#error-handling) - [Threading](#threading) - [Complete C example](#complete-c-example) - [Scrolling and level of detail in C](#scrolling-and-level-of-detail-in-c) - [Python (ctypes) example](#python-ctypes-example) - [Other languages](#other-languages) - [Versioning](#versioning) - [Known quirks](#known-quirks)

## Build and link

The crate builds three library types (`crate-type = ["cdylib", "staticlib", "rlib"]`); it depends on `armatura-core` with `default-features = false`, so it contains no scripting engine and no MPL third-party crate.

```sh
cargo build -p armatura-capi             # debug:   target/debug/libarmatura_capi.{so,a}
cargo build --release -p armatura-capi   # release: target/release/libarmatura_capi.{so,a}
```

| Platform | Dynamic library | Static library |
|---|---|---|
| Linux | `libarmatura_capi.so` | `libarmatura_capi.a` |
| macOS | `libarmatura_capi.dylib` | `libarmatura_capi.a` |
| Windows | `armatura_capi.dll` (+ import library) | `armatura_capi.lib` |

**Tested:** Linux x86-64, gcc, both the shared and the static library (below). macOS and Windows are the standard Rust output names for those targets but were **not built or run** by the author. A debug `.so` is about 150 MB (debug info); build `--release` for anything you ship (the size of the release library was not measured here).

Compile and link against the shared library:

```sh
cc -std=c99 -Wall -Wextra -Iarmatura-capi -o example example.c -Ltarget/debug -larmatura_capi
LD_LIBRARY_PATH=target/debug ./example project.json frame.ppm      # or install the .so and use rpath / ldconfig
```

Static linking (verified on Linux; the Rust standard library needs these system libraries):

```sh
cc -std=c99 -Iarmatura-capi -o example example.c target/debug/libarmatura_capi.a -lpthread -ldl -lm
```

`armatura.h` has `extern "C"` guards for C++, needs only `<stddef.h>` and `<stdint.h>`, and is the only file you include. Copy it next to your sources. There is no pkg-config file, no install target and no versioned SONAME; vendor the library with your application.

Panics are caught at every entry point and turned into error codes, which relies on the library being built with the default `panic = "unwind"`. Do not build it with `panic = "abort"` (then a bug in the engine would abort your process instead of returning -1). The workspace does not set that profile option.

## Conventions

* Strings are UTF-8. Input strings (`const char *`) are NUL-terminated, except the project JSON, which is passed as pointer plus length and needs no terminator.
* Functions that return `int` give **0 on success and a negative value on failure**. `-1` is a general error; `-2` additionally means "your output buffer is too small" and only occurs in the two render functions.
* Functions that fill a character buffer take `(char *out, size_t cap)` and return the **full length** of the text (excluding the NUL). The text is truncated to `cap - 1` bytes and NUL-terminated when `cap > 0`. Pass `out == NULL` (with any `cap`) to ask for the length only. A negative return is an error (bad index, unknown name).
* Float arguments must be finite; frames outside the timeline are clamped; `scale` must be within 0.05 to 4.
* Every handle argument may be `NULL` for the getters (they return 0 and set the error text) and is an error for the rest.
* The output RGBA is **straight alpha** (not premultiplied), 8 bits per channel, row-major, top row first, `width * height * 4` bytes.
* Bone world transforms are six floats per bone `[a b c d tx ty]`; a bone-local point `(x, y)` maps to `(a*x + c*y + tx, b*x + d*y + ty)`.

## Function reference

53 functions. `E` = `ArmaturaEngine *`, `P` = `ArmaturaPlayer *`.

### Library

| Function | Description |
|---|---|
| `const char *armatura_version(void)` | version string of the `armatura-capi` crate (currently `"0.1.0"`); static, never freed |
| `uint32_t armatura_abi_version(void)` | ABI version of the loaded library; compare with `ARMATURA_ABI_VERSION` from the header you compiled against |
| `const char *armatura_last_error(void)` | text of the last error **on this thread**; empty string if none yet; see [Error handling](#error-handling) |

### Engine: open, query, render

| Function | Description |
|---|---|
| `E *armatura_open(const char *json, size_t len)` | parse and validate a project (UTF-8, `len` bytes, 1 to 256 MiB); returns `NULL` on error. The library copies what it needs; you may free `json` immediately. Relative image files in the project are looked up in the process's current directory, absolute paths and `..` are refused (see [SECURITY.md](SECURITY.md)); data-URI images always work. |
| `void armatura_close(E *)` | free the engine; `NULL` is allowed; call exactly once per handle |
| `uint32_t armatura_frame_count(const E *)` | number of frames |
| `float armatura_fps(const E *)` | frames per second |
| `uint32_t armatura_width(const E *)`, `armatura_height(const E *)` | stage size in stage units, rounded to an integer |
| `uint32_t armatura_bone_count(const E *)` | number of bones |
| `int armatura_bone_name(const E *, uint32_t index, char *out, size_t cap)` | name of bone `index` (rig order); length, or -1 for a bad index |
| `int armatura_bone_world(const E *, float frame, float *out, size_t cap_floats)` | world matrices of every bone; `cap_floats` must be at least `6 * bone_count`, else -1 |
| `int armatura_render_rgba(const E *, float frame, float scale, uint8_t *out, size_t cap, uint32_t *w, uint32_t *h)` | render a frame; output size `round(width*scale) x round(height*scale)`; returns 0, **-2 if `cap` is smaller than `w*h*4`** (and `*w`, `*h` are set, so you can allocate and call again; pass `out == NULL` to ask), -1 on other errors. `w` and `h` may be `NULL`. |

### Player: state machines

A project may contain `machines` (runtime state machines) and `bindings` (external values driving properties). A player runs one machine: set inputs by name, advance time, read states and events, render the current pose. See [API.md](API.md) for the semantics (they are the `Player` methods).

| Function | Description |
|---|---|
| `uint32_t armatura_machine_count(const E *)` | machines in the project |
| `int armatura_machine_name(const E *, uint32_t index, char *out, size_t cap)` | name of machine `index` (character-buffer rules) |
| `P *armatura_player_open(const E *, const char *machine)` | start the machine at time 0; `NULL` on error (unknown machine, ...). The player owns a **copy** of the project: it stays valid after `armatura_close` of the engine. |
| `void armatura_player_close(P *)` | free the player; `NULL` allowed |
| `int armatura_player_set_bool(P *, const char *name, int value)` | bool input (`value != 0`) |
| `int armatura_player_set_number(P *, const char *name, double value)` | number input (finite) |
| `int armatura_player_fire(P *, const char *trigger)` | fire a trigger; it is consumed by the transition that tests it |
| `int armatura_player_set_value(P *, const char *path, double value)` | external value for data bindings, e.g. `"player.health"`; also drives a scroll region whose `input` is this path (pixels along its main axis) |
| `int armatura_player_advance(P *, double dt)` | advance by `dt` seconds, `0..=10`; deterministic (long steps are split into at most 1/60 s pieces) |
| `int armatura_player_state(P *, const char *layer, char *out, size_t cap)` | current state name of a machine layer; length, or -1 for an unknown layer |
| `int armatura_player_events(P *, char *out, size_t cap)` | pending events as a JSON array `[{"time":..,"layer":..,"kind":"entered"\|"exited","state":..}]`. Returns the full length. **The events are consumed only if the whole text fit (`cap` > length)**; otherwise they are kept, and you call again with a bigger buffer. `out == NULL` queries the size and consumes nothing. |
| `int armatura_player_render_rgba(P *, float scale, uint8_t *out, size_t cap, uint32_t *w, uint32_t *h)` | same contract as `armatura_render_rgba`, for the current pose; the size follows the player's canvas |

### Player: layout, scrolling, level of detail

| Function | Description |
|---|---|
| `int armatura_player_set_canvas(P *, float width, float height)` | responsive layout: canvas size in stage units (finite, positive, at most 32768); containers re-solve, renders get this size |
| `int armatura_player_canvas_size(P *, float *w, float *h)` | current canvas size; either pointer may be `NULL` |
| `uint32_t armatura_player_scroll_count(P *)` | scroll regions of the project |
| `int armatura_player_scroll_id(P *, uint32_t index, char *out, size_t cap)` | id of region `index` (character-buffer rules) |
| `int armatura_player_scroll_drag(P *, const char *id, float x, float y)` | raw pointer offset in pixels (content moved up / left); past the ends it clamps or rubber-bands |
| `int armatura_player_scroll_release(P *, const char *id, float vx, float vy)` | release velocity in px/s; `armatura_player_advance` then flings, snaps and springs back |
| `int armatura_player_scroll_offset(P *, const char *id, float *x, float *y)` | displayed offset; -1 for an unknown id; pointers may be `NULL` |
| `int armatura_player_scroll_max(P *, const char *id, float *x, float *y)` | largest useful offset at the current canvas size |
| `int armatura_player_scroll_settled(P *, const char *id)` | 1 at rest, 0 while moving, -1 unknown id |
| `int armatura_player_set_screen(P *, float width_px, float height_px, float max_error_px)` | tell the player how large it is shown; it renders the coarsest prebuilt level of detail whose declared error bound stays within `max_error_px` pixels. Levels are built on first use (this call can take a moment on a large project, long in a debug build). |
| `int armatura_player_lod_level(P *)` | level in use (0 = original, also without a screen size); negative for a `NULL` player |

### Editing and export to memory (additive, facade 1.4)

Edits are validated and undoable; a failed edit changes nothing, returns -1 and sets the error text. They take a non-const `E *` (one thread at a time, as always). Colours are `#rgb`, `#rrggbb` or `#rrggbbaa`. `parent` may be `NULL`.

| Function | Description |
|---|---|
| `int armatura_add_rect_layer(E *, name, x, y, w, h, fill, parent)` | append a filled rectangle layer; `parent` names a top-level group layer, `NULL` appends at the top level |
| `int armatura_add_ellipse_layer(E *, name, cx, cy, rx, ry, fill, parent)` | the same for an ellipse |
| `int armatura_add_group_layer(E *, name)` | append an empty group layer |
| `int armatura_add_bone(E *, name, parent, length, x, y, angle)` | append a bone; the parent must exist (`NULL` = root); angle in degrees |
| `int armatura_set_key(E *, target, prop, frame, value, interpolation)` | one key on `target.prop`; interpolation 0 linear, 1 step, 2 ease-in, 3 ease-out, 4 ease-in-out, 5 smooth |
| `int armatura_bake_physics(E *, chain, preset, overwrite)` | simulate a comma-separated bone chain (root first) with `hair`, `tail`, `rope`, `antenna` or `cape`; returns the channels written (0 when existing keyed bones were kept) |
| `int armatura_undo(E *)`, `int armatura_redo(E *)` | 1 done, 0 nothing to do, negative on error |
| `uint32_t armatura_layer_count(const E *)`, `int armatura_layer_name(const E *, index, out, cap)` | layer paths (`group/child`), character-buffer rules |
| `B *armatura_export(const E *, format, scale)` | whole-timeline export, `B` = `ArmaturaBlob *`; format 0 GIF, 1 glTF binary, 2 Lottie JSON, 3 HTML player, 4 APNG; `scale` 0.05 to 4 for GIF and APNG; `NULL` on error |
| `B *armatura_render_png(const E *, frame, scale)` | one frame as PNG |
| `B *armatura_render_exr(const E *, frame, scale, float32)` | one frame as OpenEXR (linear, premultiplied; non-zero `float32` stores FLOAT, otherwise HALF) |
| `B *armatura_to_json(const E *)` | the (edited) project as UTF-8 JSON, no terminator |
| `const uint8_t *armatura_blob_data(const B *)`, `size_t armatura_blob_size(const B *)` | the bytes of a blob, valid until it is freed; `NULL` / 0 for a `NULL` blob |
| `void armatura_blob_free(B *)` | free a blob exactly once; `NULL` is allowed |

## Ownership and lifetime

* **Strings and arrays are copied into buffers you provide; export results are blobs you free.** Every string and array result is copied into a buffer *you* provide. The only library-owned memory is an `ArmaturaBlob` from `armatura_export`, `armatura_render_png`, `armatura_render_exr` or `armatura_to_json`: read it with `armatura_blob_data` / `armatura_blob_size`, then release it once with `armatura_blob_free`. There is no generic `armatura_free`. The only pointers the library returns are `armatura_version()` (static) and `armatura_last_error()` (see below), and the two opaque handles.
* **Handles.** `armatura_open` returns an engine handle; `armatura_player_open` returns a player handle. Each must be closed exactly once with its own `_close`; closing `NULL` is fine; using a handle after closing it, or closing it twice, is undefined behaviour.
* **Engine and players are independent.** A player owns a copy of the project, so closing the engine first is allowed and common (the example does it). Several players may be opened from one engine.
* **Your buffers.** Input pointers need only be valid for the duration of the call; the library keeps no pointer to them. Output buffers must really have the capacity you pass; the library writes at most `cap` bytes (strings) or `cap_floats` floats.
* **`armatura_last_error()`** returns a pointer into a thread-local string. It stays valid until the next call on the same thread that *fails* (that call overwrites it) or until the thread exits. Copy it if you need it longer. Do not free it.

## Error handling

1. Always test the return value (`NULL` for the two `_open` functions and `armatura_player_open`, a negative `int` otherwise, `0` from a getter given a `NULL` handle).
2. Then read `armatura_last_error()` on the same thread for the reason: for example `invalid project JSON: ...`, `output buffer needs 1228800 bytes`, `invalid argument: input 'excited' is not a number`.
3. **The error text is only set by failures and is never cleared by a success.** After a later successful call it still holds the old message. Never use "the string is non-empty" as a failure test.
4. Error texts come from the Rust facade (`ApiError` `Display`); they are for people, not a contract. The C ABI does not expose the error *kind*.
5. A panic inside the engine is caught and reported as `internal error (panic) inside armatura` with the usual failure return; the handle remains usable in principle, but treat it as a bug and report it.

## Threading

* Each handle may be used from **one thread at a time** (the header's contract). You may move a handle between threads as long as calls never overlap; different handles can run in parallel on different threads (the underlying Rust types are `Send + Sync`, but the C contract stays conservative).
* The last-error string is **per thread**: read it on the thread that made the failing call.
* There are no callbacks and the library starts no threads of its own.

## Complete C example

Load a project, print facts and bone positions, render a frame to a PPM file (asking for the size first with `NULL`, then allocating exactly), run the project's first state machine, read its events with the size-query pattern and show the error path. Verified with gcc, also under AddressSanitizer/UBSan (no findings), against both the shared and the static library.

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

Built and run against the `armatura demo` project extended with a state machine (`hero`, input `excited`, states `idle` and `wave`):

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

The repository also ships `armatura-capi/tests/smoke.c` (open, render, checksum) and the Rust tests `armatura-capi/tests/abi.rs` (null pointers, bad indices, buffer sizes, panic safety).

## Scrolling and level of detail in C

A project with a layout container and a scroll region (the scroll test project of the Rust tests: ten 20-unit rows in a column, a viewport pinned to the left, top and bottom edges). Verified:

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

Output: `scroll 'list': max=(0,80) dragged offset=(0,80) settled=1` (dragging to 500 clamps at the maximum when elasticity is off), then `set_screen rc=0, lod level=7` (with up to 4 px of error accepted, the coarsest prebuilt level qualifies on this tiny project; the number depends on the project) and `render size 100x120`. Note the explicit `rc` variable in the source: the order in which C evaluates function arguments is unspecified, so never put the call that changes the player and the call that reads it in one `printf`.

## Python (ctypes) example

Standard library only. Declare `argtypes` and `restype` for every function you call: ctypes assumes `int` otherwise and truncates 64-bit pointers. Verified on Python 3 against the debug library; its PPM output is byte-identical to the C example's.

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

## Other languages

Anything that can call a C function works the same way: declare the 53 functions, hold the opaque pointers as `void *`, and follow the buffer conventions above. Notes that are specific to common hosts (untested here, only consequences of the contract above): in C#/.NET use `[DllImport]` with `CallingConvention.Cdecl` and marshal strings as UTF-8 byte arrays; in Go with cgo, keep Go pointers out of the calls except for buffers that live for the call; in Rust prefer the facade directly instead of this ABI.

## Versioning

* `ARMATURA_ABI_VERSION` / `armatura_abi_version()` is currently **1**. It changes only when an existing function is removed or its meaning changes. All additions so far (the player functions in facade 1.1, layout, scrolling and level of detail in 1.2) were additive and left it at 1, so a program built against an older header keeps working with a newer library, and a newer header against an older library fails at link or load time on the missing symbol. Check `armatura_abi_version()` at startup and compare with the macro.
* `armatura_version()` is the `armatura-capi` crate version (0.1.0), not the facade version. There is no C function that reports the facade version (`API_VERSION`).
* Facade 1.4 added the editing basics and export-to-memory functions above (additive, ABI version still 1). Not exposed in the C ABI: SVG rendering, deleting keys, version control, animation CI, tracing, mocap, fuzzing, size-targeted export, scripts. They are available in Rust and through the CLI.

## Known quirks

The header comments were corrected to match the behaviour (they used to disagree); the points worth knowing:

* `armatura_render_rgba` writes `*w` / `*h` on success and on -2 (buffer too small), and also when a later render error happens after `frame` and `scale` were accepted. It does **not** write them when the engine is null or when `frame` / `scale` are rejected (-1). Regression test: `documented_size_outputs_and_last_error_semantics` in `armatura-capi/tests/abi.rs`.
* The last-error text is thread-local, overwritten only by the next *failing* call, and never cleared by a success. Copy it if you need it longer.
* `armatura_width` / `armatura_height` round the float stage size to an integer; the real render size is `round(size * scale)`, so use the `w` / `h` returned by the render call.
* The header's note about scroll regions following `armatura_player_set_value` sits between the scroll and level-of-detail declarations; it describes `armatura_player_set_value`.
