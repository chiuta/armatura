/* ARMATURA C ABI (version 1). Plain C, no dependencies.
 *
 * Ownership: armatura_open returns a handle you must pass to armatura_close exactly once.
 * Threads: a handle may be used from one thread at a time; the last-error text is per thread.
 * Errors: functions returning int give 0 on success and a negative value on failure;
 *         armatura_last_error() then describes it. The text is replaced only by the next FAILING call
 *         on that thread (the old pointer is invalid from then on); a successful call never clears it,
 *         so read it right after a failure and trust the return value, not the text, to know whether
 *         the last call worked.
 */
#ifndef ARMATURA_H
#define ARMATURA_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct ArmaturaEngine ArmaturaEngine;

#define ARMATURA_ABI_VERSION 1

/* Crate version string, e.g. "0.1.0". Static, never freed. */
const char *armatura_version(void);
uint32_t armatura_abi_version(void);

/* Parse and validate a project from JSON (UTF-8, `len` bytes, no terminator needed).
 * Returns NULL on error. */
ArmaturaEngine *armatura_open(const char *json, size_t len);
void armatura_close(ArmaturaEngine *engine); /* NULL is allowed */
const char *armatura_last_error(void);       /* "" until the first failure on this thread; then the text of the
                                                latest failure (not cleared by successes, see above) */

uint32_t armatura_frame_count(const ArmaturaEngine *engine);
float armatura_fps(const ArmaturaEngine *engine);
/* Stage size in stage units, rounded to the nearest integer. A render at `scale` is
 * max(1, round(stage_size * scale)) pixels in each direction (armatura_render_rgba reports the exact size). */
uint32_t armatura_width(const ArmaturaEngine *engine);
uint32_t armatura_height(const ArmaturaEngine *engine);

uint32_t armatura_bone_count(const ArmaturaEngine *engine);
/* Copy bone `index` name into `out` (NUL-terminated, truncated to `cap`). Returns the full name
 * length (excluding NUL), or a negative value for a bad index. */
int armatura_bone_name(const ArmaturaEngine *engine, uint32_t index, char *out, size_t cap);

/* World transform of every bone at `frame`: 6 floats per bone [a b c d tx ty], where a point maps to
 * (a*x + c*y + tx, b*x + d*y + ty). `cap_floats` must be at least 6 * bone_count. */
int armatura_bone_world(const ArmaturaEngine *engine, float frame, float *out, size_t cap_floats);

/* Render `frame` at `scale` (0.05..4) into straight-alpha RGBA8.
 * Returns 0 on success, -2 if `out` is NULL or `cap` is smaller than w*h*4, other negative values
 * on error (null engine, frame or scale not finite / out of range, render failure).
 * *w and *h (either may be NULL) are written once the frame and scale have been accepted: on success
 * and on the buffer-too-small result -2 (that is how you ask for the size: call with out = NULL, then
 * allocate w*h*4 bytes), and also when the render itself then fails. They are NOT written, and keep
 * their old values, when the engine is null or frame/scale are rejected. */
int armatura_render_rgba(const ArmaturaEngine *engine, float frame, float scale, uint8_t *out, size_t cap, uint32_t *w, uint32_t *h);


/* ---- Interactive playback: state machines (additive; ABI version unchanged) ----
 * A project may carry state machines ("machines"). A player runs one of them: set inputs by name,
 * advance time, read the state / events, render the current pose. Strings are NUL-terminated UTF-8.
 * Int functions return 0 on success, negative on error (armatura_last_error()). A player owns a copy
 * of the project, so it stays valid after armatura_close of the engine. */
typedef struct ArmaturaPlayer ArmaturaPlayer;

uint32_t armatura_machine_count(const ArmaturaEngine *engine);
/* Name of machine `index` (same copy rules as armatura_bone_name). */
int armatura_machine_name(const ArmaturaEngine *engine, uint32_t index, char *out, size_t cap);

ArmaturaPlayer *armatura_player_open(const ArmaturaEngine *engine, const char *machine); /* NULL on error */
void armatura_player_close(ArmaturaPlayer *player);                                      /* NULL is allowed */

int armatura_player_set_bool(ArmaturaPlayer *player, const char *name, int value);
int armatura_player_set_number(ArmaturaPlayer *player, const char *name, double value);
int armatura_player_fire(ArmaturaPlayer *player, const char *trigger);
/* External value for data bindings, e.g. "player.health". */
int armatura_player_set_value(ArmaturaPlayer *player, const char *path, double value);
/* Advance by dt seconds, 0..10. Deterministic. */
int armatura_player_advance(ArmaturaPlayer *player, double dt);
/* Current state name of a layer: returns its length, -1 for an unknown layer. */
int armatura_player_state(ArmaturaPlayer *player, const char *layer, char *out, size_t cap);
/* Pending events as a JSON array [{"time":..,"layer":..,"kind":"entered"|"exited","state":..}].
 * Returns the full length; if it does not fit in `cap` (incl. NUL) nothing is consumed - call again
 * with a larger buffer (out == NULL queries the size). */
int armatura_player_events(ArmaturaPlayer *player, char *out, size_t cap);
/* Same contract as armatura_render_rgba, for the current pose. */
int armatura_player_render_rgba(ArmaturaPlayer *player, float scale, uint8_t *out, size_t cap, uint32_t *w, uint32_t *h);

/* ---- Layout, scrolling and level of detail (additive; ABI version unchanged) ----
 * Containers and scroll regions are part of the project (see docs/API.md). The player keeps the
 * state of each scroll region (drag, fling, snap points, rubber band) and re-solves the layout for
 * the canvas size you give it. Int functions return 0 on success, negative on error. */
/* Canvas size in stage units; renders (scale 1) get that size. */
int armatura_player_set_canvas(ArmaturaPlayer *player, float width, float height);
int armatura_player_canvas_size(ArmaturaPlayer *player, float *w, float *h);        /* pointers may be NULL */
uint32_t armatura_player_scroll_count(ArmaturaPlayer *player);
int armatura_player_scroll_id(ArmaturaPlayer *player, uint32_t index, char *out, size_t cap);
/* Raw pointer offset in pixels (content moved up / left); past the ends it clamps or rubber-bands. */
int armatura_player_scroll_drag(ArmaturaPlayer *player, const char *id, float x, float y);
/* Release velocity in pixels per second; armatura_player_advance flings and settles. */
int armatura_player_scroll_release(ArmaturaPlayer *player, const char *id, float vx, float vy);
int armatura_player_scroll_offset(ArmaturaPlayer *player, const char *id, float *x, float *y);
int armatura_player_scroll_max(ArmaturaPlayer *player, const char *id, float *x, float *y);
int armatura_player_scroll_settled(ArmaturaPlayer *player, const char *id);         /* 1, 0 or -1 */
/* A scroll region whose "input" path equals the path given to armatura_player_set_value follows that
 * value (pixels along its main axis). */
/* Screen size in pixels and the largest error you accept there: the player renders the coarsest
 * precomputed level of detail whose declared bound fits. Levels are built on first use. */
int armatura_player_set_screen(ArmaturaPlayer *player, float width_px, float height_px, float max_error_px);
int armatura_player_lod_level(ArmaturaPlayer *player);                                /* 0 = original */

/* ---- Editing and export to memory (additive; ABI version unchanged) ----
 * Edits are validated and undoable. A failed edit changes nothing and returns -1. Edit functions take
 * a non-const engine: use a handle from one thread at a time. Strings are NUL-terminated UTF-8; where a
 * parameter says "or NULL" the NULL is valid. Colours are "#rgb", "#rrggbb" or "#rrggbbaa". */
typedef struct ArmaturaBlob ArmaturaBlob;

/* Append a filled rectangle / ellipse layer. `parent` (or NULL) names a top-level group layer. */
int armatura_add_rect_layer(ArmaturaEngine *engine, const char *name, float x, float y, float w, float h, const char *fill, const char *parent);
int armatura_add_ellipse_layer(ArmaturaEngine *engine, const char *name, float cx, float cy, float rx, float ry, const char *fill, const char *parent);
int armatura_add_group_layer(ArmaturaEngine *engine, const char *name);
/* Append a bone; `parent` (or NULL for a root) must exist. Angle in degrees. */
int armatura_add_bone(ArmaturaEngine *engine, const char *name, const char *parent, float length, float x, float y, float angle);
/* Set a key on `target.prop` (prop is the snake_case JSON name, e.g. "bone_angle").
 * interpolation: 0 linear, 1 step, 2 ease-in, 3 ease-out, 4 ease-in-out, 5 smooth. */
int armatura_set_key(ArmaturaEngine *engine, const char *target, const char *prop, float frame, float value, int interpolation);
/* Simulate a comma-separated bone chain (root first, e.g. "h1,h2,h3") with a preset ("hair", "tail",
 * "rope", "antenna", "cape") and store the result as angle channels (one undo step). Returns the number
 * of channels written (0 when existing keyed bones were kept and overwrite is 0) or negative. */
int armatura_bake_physics(ArmaturaEngine *engine, const char *chain, const char *preset, int overwrite);
int armatura_undo(ArmaturaEngine *engine); /* 1 undone, 0 nothing to undo, negative error */
int armatura_redo(ArmaturaEngine *engine); /* 1 redone, 0 nothing to redo, negative error */

uint32_t armatura_layer_count(const ArmaturaEngine *engine);
/* Path of layer `index` ("group/child"); same copy rules as armatura_bone_name. */
int armatura_layer_name(const ArmaturaEngine *engine, uint32_t index, char *out, size_t cap);

/* Exports return an owned byte buffer, or NULL on error (armatura_last_error()). Read it with
 * armatura_blob_data / armatura_blob_size and release it with armatura_blob_free (NULL is allowed). */
/* format: 0 GIF, 1 binary glTF, 2 Lottie JSON, 3 HTML player, 4 APNG. scale (0.05..4) applies to GIF/APNG. */
ArmaturaBlob *armatura_export(const ArmaturaEngine *engine, int format, float scale);
ArmaturaBlob *armatura_render_png(const ArmaturaEngine *engine, float frame, float scale);
/* OpenEXR, linear light, premultiplied; float32 != 0 stores FLOAT, otherwise HALF. */
ArmaturaBlob *armatura_render_exr(const ArmaturaEngine *engine, float frame, float scale, int float32);
ArmaturaBlob *armatura_to_json(const ArmaturaEngine *engine); /* project JSON, UTF-8, no terminator */
const uint8_t *armatura_blob_data(const ArmaturaBlob *blob);
size_t armatura_blob_size(const ArmaturaBlob *blob);
void armatura_blob_free(ArmaturaBlob *blob);

#ifdef __cplusplus
}
#endif
#endif
