/* OSATURA C ABI (version 1). Plain C, no dependencies.
 *
 * Ownership: osatura_open returns a handle you must pass to osatura_close exactly once.
 * Threads: a handle may be used from one thread at a time; the last-error text is per thread.
 * Errors: functions returning int give 0 on success and a negative value on failure;
 *         osatura_last_error() then describes it. The text is replaced only by the next FAILING call
 *         on that thread (the old pointer is invalid from then on); a successful call never clears it,
 *         so read it right after a failure and trust the return value, not the text, to know whether
 *         the last call worked.
 */
#ifndef OSATURA_H
#define OSATURA_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct OsaturaEngine OsaturaEngine;

#define OSATURA_ABI_VERSION 1

/* Crate version string, e.g. "0.1.0". Static, never freed. */
const char *osatura_version(void);
uint32_t osatura_abi_version(void);

/* Parse and validate a project from JSON (UTF-8, `len` bytes, no terminator needed).
 * Returns NULL on error. */
OsaturaEngine *osatura_open(const char *json, size_t len);
void osatura_close(OsaturaEngine *engine); /* NULL is allowed */
const char *osatura_last_error(void);       /* "" until the first failure on this thread; then the text of the
                                                latest failure (not cleared by successes, see above) */

uint32_t osatura_frame_count(const OsaturaEngine *engine);
float osatura_fps(const OsaturaEngine *engine);
/* Stage size in stage units, rounded to the nearest integer. A render at `scale` is
 * max(1, round(stage_size * scale)) pixels in each direction (osatura_render_rgba reports the exact size). */
uint32_t osatura_width(const OsaturaEngine *engine);
uint32_t osatura_height(const OsaturaEngine *engine);

uint32_t osatura_bone_count(const OsaturaEngine *engine);
/* Copy bone `index` name into `out` (NUL-terminated, truncated to `cap`). Returns the full name
 * length (excluding NUL), or a negative value for a bad index. */
int osatura_bone_name(const OsaturaEngine *engine, uint32_t index, char *out, size_t cap);

/* World transform of every bone at `frame`: 6 floats per bone [a b c d tx ty], where a point maps to
 * (a*x + c*y + tx, b*x + d*y + ty). `cap_floats` must be at least 6 * bone_count. */
int osatura_bone_world(const OsaturaEngine *engine, float frame, float *out, size_t cap_floats);

/* Render `frame` at `scale` (0.05..4) into straight-alpha RGBA8.
 * Returns 0 on success, -2 if `out` is NULL or `cap` is smaller than w*h*4, other negative values
 * on error (null engine, frame or scale not finite / out of range, render failure).
 * *w and *h (either may be NULL) are written once the frame and scale have been accepted: on success
 * and on the buffer-too-small result -2 (that is how you ask for the size: call with out = NULL, then
 * allocate w*h*4 bytes), and also when the render itself then fails. They are NOT written, and keep
 * their old values, when the engine is null or frame/scale are rejected. */
int osatura_render_rgba(const OsaturaEngine *engine, float frame, float scale, uint8_t *out, size_t cap, uint32_t *w, uint32_t *h);


/* ---- Interactive playback: state machines (additive; ABI version unchanged) ----
 * A project may carry state machines ("machines"). A player runs one of them: set inputs by name,
 * advance time, read the state / events, render the current pose. Strings are NUL-terminated UTF-8.
 * Int functions return 0 on success, negative on error (osatura_last_error()). A player owns a copy
 * of the project, so it stays valid after osatura_close of the engine. */
typedef struct OsaturaPlayer OsaturaPlayer;

uint32_t osatura_machine_count(const OsaturaEngine *engine);
/* Name of machine `index` (same copy rules as osatura_bone_name). */
int osatura_machine_name(const OsaturaEngine *engine, uint32_t index, char *out, size_t cap);

OsaturaPlayer *osatura_player_open(const OsaturaEngine *engine, const char *machine); /* NULL on error */
void osatura_player_close(OsaturaPlayer *player);                                      /* NULL is allowed */

int osatura_player_set_bool(OsaturaPlayer *player, const char *name, int value);
int osatura_player_set_number(OsaturaPlayer *player, const char *name, double value);
int osatura_player_fire(OsaturaPlayer *player, const char *trigger);
/* External value for data bindings, e.g. "player.health". */
int osatura_player_set_value(OsaturaPlayer *player, const char *path, double value);
/* Advance by dt seconds, 0..10. Deterministic. */
int osatura_player_advance(OsaturaPlayer *player, double dt);
/* Current state name of a layer: returns its length, -1 for an unknown layer. */
int osatura_player_state(OsaturaPlayer *player, const char *layer, char *out, size_t cap);
/* Pending events as a JSON array [{"time":..,"layer":..,"kind":"entered"|"exited","state":..}].
 * Returns the full length; if it does not fit in `cap` (incl. NUL) nothing is consumed - call again
 * with a larger buffer (out == NULL queries the size). */
int osatura_player_events(OsaturaPlayer *player, char *out, size_t cap);
/* Same contract as osatura_render_rgba, for the current pose. */
int osatura_player_render_rgba(OsaturaPlayer *player, float scale, uint8_t *out, size_t cap, uint32_t *w, uint32_t *h);

/* ---- Layout, scrolling and level of detail (additive; ABI version unchanged) ----
 * Containers and scroll regions are part of the project (see docs/API.md). The player keeps the
 * state of each scroll region (drag, fling, snap points, rubber band) and re-solves the layout for
 * the canvas size you give it. Int functions return 0 on success, negative on error. */
/* Canvas size in stage units; renders (scale 1) get that size. */
int osatura_player_set_canvas(OsaturaPlayer *player, float width, float height);
int osatura_player_canvas_size(OsaturaPlayer *player, float *w, float *h);        /* pointers may be NULL */
uint32_t osatura_player_scroll_count(OsaturaPlayer *player);
int osatura_player_scroll_id(OsaturaPlayer *player, uint32_t index, char *out, size_t cap);
/* Raw pointer offset in pixels (content moved up / left); past the ends it clamps or rubber-bands. */
int osatura_player_scroll_drag(OsaturaPlayer *player, const char *id, float x, float y);
/* Release velocity in pixels per second; osatura_player_advance flings and settles. */
int osatura_player_scroll_release(OsaturaPlayer *player, const char *id, float vx, float vy);
int osatura_player_scroll_offset(OsaturaPlayer *player, const char *id, float *x, float *y);
int osatura_player_scroll_max(OsaturaPlayer *player, const char *id, float *x, float *y);
int osatura_player_scroll_settled(OsaturaPlayer *player, const char *id);         /* 1, 0 or -1 */
/* A scroll region whose "input" path equals the path given to osatura_player_set_value follows that
 * value (pixels along its main axis). */
/* Screen size in pixels and the largest error you accept there: the player renders the coarsest
 * precomputed level of detail whose declared bound fits. Levels are built on first use. */
int osatura_player_set_screen(OsaturaPlayer *player, float width_px, float height_px, float max_error_px);
int osatura_player_lod_level(OsaturaPlayer *player);                                /* 0 = original */

/* ---- Editing and export to memory (additive; ABI version unchanged) ----
 * Edits are validated and undoable. A failed edit changes nothing and returns -1. Edit functions take
 * a non-const engine: use a handle from one thread at a time. Strings are NUL-terminated UTF-8; where a
 * parameter says "or NULL" the NULL is valid. Colours are "#rgb", "#rrggbb" or "#rrggbbaa". */
typedef struct OsaturaBlob OsaturaBlob;

/* Append a filled rectangle / ellipse layer. `parent` (or NULL) names a top-level group layer. */
int osatura_add_rect_layer(OsaturaEngine *engine, const char *name, float x, float y, float w, float h, const char *fill, const char *parent);
int osatura_add_ellipse_layer(OsaturaEngine *engine, const char *name, float cx, float cy, float rx, float ry, const char *fill, const char *parent);
int osatura_add_group_layer(OsaturaEngine *engine, const char *name);
/* Append a bone; `parent` (or NULL for a root) must exist. Angle in degrees. */
int osatura_add_bone(OsaturaEngine *engine, const char *name, const char *parent, float length, float x, float y, float angle);
/* Set a key on `target.prop` (prop is the snake_case JSON name, e.g. "bone_angle").
 * interpolation: 0 linear, 1 step, 2 ease-in, 3 ease-out, 4 ease-in-out, 5 smooth. */
int osatura_set_key(OsaturaEngine *engine, const char *target, const char *prop, float frame, float value, int interpolation);
/* Simulate a comma-separated bone chain (root first, e.g. "h1,h2,h3") with a preset ("hair", "tail",
 * "rope", "antenna", "cape") and store the result as angle channels (one undo step). Returns the number
 * of channels written (0 when existing keyed bones were kept and overwrite is 0) or negative. */
int osatura_bake_physics(OsaturaEngine *engine, const char *chain, const char *preset, int overwrite);
int osatura_undo(OsaturaEngine *engine); /* 1 undone, 0 nothing to undo, negative error */
int osatura_redo(OsaturaEngine *engine); /* 1 redone, 0 nothing to redo, negative error */

uint32_t osatura_layer_count(const OsaturaEngine *engine);
/* Path of layer `index` ("group/child"); same copy rules as osatura_bone_name. */
int osatura_layer_name(const OsaturaEngine *engine, uint32_t index, char *out, size_t cap);

/* Exports return an owned byte buffer, or NULL on error (osatura_last_error()). Read it with
 * osatura_blob_data / osatura_blob_size and release it with osatura_blob_free (NULL is allowed). */
/* format: 0 GIF, 1 binary glTF, 2 Lottie JSON, 3 HTML player, 4 APNG. scale (0.05..4) applies to GIF/APNG. */
OsaturaBlob *osatura_export(const OsaturaEngine *engine, int format, float scale);
OsaturaBlob *osatura_render_png(const OsaturaEngine *engine, float frame, float scale);
/* OpenEXR, linear light, premultiplied; float32 != 0 stores FLOAT, otherwise HALF. */
OsaturaBlob *osatura_render_exr(const OsaturaEngine *engine, float frame, float scale, int float32);
OsaturaBlob *osatura_to_json(const OsaturaEngine *engine); /* project JSON, UTF-8, no terminator */
const uint8_t *osatura_blob_data(const OsaturaBlob *blob);
size_t osatura_blob_size(const OsaturaBlob *blob);
void osatura_blob_free(OsaturaBlob *blob);

#ifdef __cplusplus
}
#endif
#endif
