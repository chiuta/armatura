# Animation CI (`osatura ci`)

Treat an animation like testable code: a manifest lists checks, a CI job runs them, and a failing
visual change comes with a report that shows what moved. Everything is local, offline and
deterministic; the metrics are textbook ones (CIEDE2000, SSIM, MSE/PSNR, a DCT hash).

```
osatura ci init  <project.json> [--out osatura-ci.json] [--no-goldens]
osatura ci run   [manifest] [--update] [--bless] [--prune] [--only a,b]
                  [--junit out.xml] [--html out.html] [--json out.json] [--artifacts dir] [--quiet]
osatura ci bless [manifest] [--only a,b] [--prune]
osatura ci diff  a.png b.png [--metric ssim|de2000|de76|mse] [--tol T] [--pixel-de D] [--aa N]
                  [--ignore x,y,w,h;x,y,w,h] [--out heat.png] [--side s.png] [--overlay o.png] [--flicker f.gif]
```

Exit codes: **0** every check passed (or only info / updated / blessed), **1** at least one check
failed (or `ci diff` found a difference), **2** something could not be evaluated (missing project,
bad manifest, unreadable or tampered golden, unknown option).

## Quick start

```
osatura ci init animation.json        # writes osatura-ci.json and goldens/ next to the project
osatura ci run --html report.html     # verify; exit 1 and a report on regressions
osatura ci bless                      # after reviewing the report: accept the new look
git add osatura-ci.json goldens       # commit the goldens with the change that caused them
```

`init` calibrates the manifest on the project as it is now: about six evenly spaced frames at a
proxy scale (longest side at most 480 px), a float-renderer check on two of them, determinism,
validate, a pose-fuzz failure-rate ceiling (measured rate plus 2 points; the demo rig fails most
full-range samples, so its ceiling is 100 % and the check guards nothing until you lower it),
limits with 50 % headroom, a GIF size budget (+50 %), a loop-seam check (only when the project
loops cleanly today), and an informational render-speed budget (4 x the measured median, which
depends on the machine and on debug versus release builds).

## Manifest (`osatura-ci.json`)

```json
{
  "schema": "osatura-ci/1",
  "project": "animation.json",
  "goldens": "goldens",
  "defaults": { "max_de": 6.0 },
  "checks": [
    { "kind": "validate", "name": "validate" },
    { "kind": "render", "name": "frames", "frames": [0, 12, 24, 47], "scale": 0.5,
      "thresholds": { "min_ssim": 0.995 }, "ignore": [[500, 0, 140, 40]] },
    { "kind": "render", "name": "tablet", "step": 12, "size": "1024x768", "scale": 0.5 },
    { "kind": "render", "name": "svg-text", "frames": [0], "renderer": "svg" },
    { "kind": "determinism", "name": "determinism", "frames": [0, 47] },
    { "kind": "fuzz", "name": "pose-fuzz", "samples": 200, "max_fail_rate": 0.05 },
    { "kind": "limits", "name": "limits", "max_layers": 200, "max_file_bytes": 2000000 },
    { "kind": "export-size", "name": "gif", "format": "gif", "max_bytes": 800000, "scale": 0.5 },
    { "kind": "loop", "name": "loop-seam" },
    { "kind": "perf", "name": "speed", "budget_ms": 40, "tolerance": 0.5, "enforce": false }
  ]
}
```

Paths are relative to the manifest. Unknown fields and kinds are errors; all problems in a manifest
are reported together (names must be 1-64 characters of letters, digits, `-`, `_`, `.`; names are
unique; every check needs a project; ranges are checked).

| kind | what it checks | fails when |
| --- | --- | --- |
| `render` | frames (`frames` list, or `step`, default first/middle/last), `scale`, `size` (layout containers re-solve), `renderer`: `raster` (tiny-skia), `float` (f32 rasteriser, quantised to 8 bit) or `svg` (exact text compare) | the picture differs from the golden beyond the thresholds, no golden exists, or the size changed |
| `validate` | project compiles; linter findings at `lint` severity (`error` default, `warn`, `info`, `off`) | any such finding or a compile error |
| `fuzz` | the pose fuzzer (`samples`, `seed`, `frame`) | failure rate above `max_fail_rate` (default 0). The check **gates by default** (`gate` unset means `true`); it is only reported, with status `info` and never failing, when `"gate": false` or `max_fail_rate` is 1 or more (a bound that cannot fail is never shown as a pass). The `osatura fuzz` command is a separate report-only tool unless given `--gate` or `--max-fail-rate` |
| `limits` | `max_layers`, `max_bones`, `max_channels`, `max_frames`, `max_canvas_pixels`, `max_file_bytes` | a count exceeds its limit |
| `export-size` | bytes of `gif`, `apng`, `lottie`, `html` or `svg-frames` (existing exporters) | larger than `max_bytes` |
| `determinism` | each frame rendered by the same engine twice and by a freshly loaded engine; SHA-256 of pixels (or of the SVG text) must agree; `renderers` default raster, float, svg | any digest differs |
| `loop` | seam between last and first frame versus the mean frame-to-frame change (mean absolute channel difference in percent); `max_seam_ratio` default 2, optional `max_seam_mad` | seam larger than allowed; at most 600 frames |
| `perf` | median ms per frame, `budget_ms` with relative `tolerance` (default 0.5) | only when `enforce` is true; otherwise status `info` |

### Render thresholds

Set in `defaults` (all render checks) and overridden per check in `thresholds`.

| key | default | meaning |
| --- | --- | --- |
| `metric` | `de2000` | `de2000` (CIEDE2000) or `de76` (CIE76, Euclidean in Lab) |
| `pixel_de` | 1.0 | a pixel counts as different above this colour difference |
| `max_de` | 8.0 | largest colour difference allowed among counted pixels |
| `min_ssim` | 0.99 | smallest allowed mean SSIM |
| `max_diff_fraction` | 0.001 | largest allowed fraction of counted pixels (0.1 %) |
| `aa_isolated` | 2 | anti-aliasing tolerance: counted groups of at most this many 8-connected pixels, all lying on an edge, are forgiven (0 = off) |
| `aa_edge_de` | 0 (off) | edge pixels differing by at most this are forgiven (looser than `pixel_de` on edges only) |
| `max_phash_distance` | not checked | largest Hamming distance of the 64-bit DCT hashes |

A frame whose pixels are byte-identical to the golden passes without any metric. Otherwise the
frame is compared pixel by pixel; every pixel is composited over white and over black and the larger
difference is used, so a change of alpha alone is visible. A pixel is an edge pixel when two opposite
neighbours (clamped at the border) in either image differ by more than 40 in some channel; the pixel's
own value is ignored, so a stray pixel in a flat area is a real difference and the rim of a shape is
not. `ignore` rectangles are `[x, y, w, h]` in stage units; they are equalised in both images before
any metric, so they also do not count in SSIM, MSE or the hash.

For orientation, on the demo project the CPU rasteriser and the float rasteriser (two different code
paths for the same scene) differ by SSIM 0.9997, 0.83 % of pixels above colour difference 1 (386 more
forgiven as anti-aliasing), largest colour difference 11.3 on shape edges. A 40-unit move of the sun
layer gives SSIM 0.993, 6.4 % of pixels, largest colour difference 19.6 in one region.

### Goldens: `--update` versus `bless`

Goldens live in `goldens/` as PNG files named by the first 24 hex digits of a SHA-256 over size and
pixels (identical frames share a file), plus `goldens/index.json` mapping check and frame to file.
SVG goldens are `.svg` files named the same way. A golden is checked against its name when it is
read, so a hand-edited or corrupted file is an error (exit 2), not a silent pass.

* `ci run --update` rewrites every golden of the selected render checks from the current render and
  forgets frames that are no longer listed. Use it when a manifest changed or after an intended
  global change.
* `ci bless` (or `ci run --bless`) accepts only frames that currently fail or have no golden and
  leaves passing ones untouched, so tolerance noise does not churn the files. The report marks them
  `blessed`.
* `--prune` deletes golden files nothing refers to. Commit `goldens/` with the change that caused it.

### Reports

`--json` is the full result (schema `osatura-ci-report/1`; metrics, regions, thresholds used).
`--junit` is JUnit XML with one test case per check. `--html` is one self-contained page (no external
resources): summary, check table, and for each failing frame the golden, current render and a
difference map as inlined PNG data URIs, a swap button to flicker between golden and current, and
the list of differing regions. `--artifacts dir` also writes `dir/<check>/fNNNNN-{golden,actual,diff}.png`
for failing frames. The difference map shows the golden faded, counted differences in a yellow to
dark-red ramp by colour difference, forgiven anti-aliasing in light blue, ignored areas hatched grey
and region boxes in magenta. `ci diff` can also write a side-by-side strip, a red/cyan overlay and a
two-frame flicker GIF; use `--flicker` on a golden/actual pair for a before/after GIF.

## GitHub Actions (example, not run)

Documentation only: this workflow was written to match the CLI but has **not** been run on a real CI
service. Adjust the Rust install and paths to your repository.

```yaml
name: animation-ci
on:
  pull_request:
  push:
    branches: [main]
jobs:
  animation:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install Rust
        run: rustup toolchain install stable --profile minimal
      - name: Build osatura
        run: cargo build --release -p osatura-cli
      - name: Animation checks
        run: |
          ./target/release/osatura ci run animation/osatura-ci.json \
            --junit ci-out/junit.xml --html ci-out/report.html --json ci-out/report.json \
            --artifacts ci-out/frames
      - name: Upload report
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: animation-ci-report
          path: ci-out/
      - name: Publish test results
        if: always()
        uses: mikepenz/action-junit-report@v4   # any JUnit consumer works
        with:
          report_paths: ci-out/junit.xml
```

Exit code 1 (visual regression or a budget) and 2 (could not evaluate) both fail the job; the report
is uploaded either way. To accept an intended change, run `osatura ci bless` locally, review the
diff of `goldens/`, and commit it.

## Generic shell script (example, not run)

```sh
#!/bin/sh
# ci.sh - run animation checks; works in any CI that can run a shell and keep files.
set -u
MANIFEST="${1:-animation/osatura-ci.json}"
OUT="${OUT:-ci-out}"
mkdir -p "$OUT"

osatura ci run "$MANIFEST" \
  --junit "$OUT/junit.xml" --html "$OUT/report.html" --json "$OUT/report.json" \
  --artifacts "$OUT/frames"
code=$?

case $code in
  0) echo "animation checks passed" ;;
  1) echo "animation regression: open $OUT/report.html" ;;
  *) echo "animation CI could not run (exit $code); see the log above" ;;
esac
exit $code
```

## Determinism and other machines

What was verified (one Linux x86-64 machine, the CPU rasteriser `tiny-skia` and the f32 rasteriser):

* rendering the same frame twice in one process, and from a freshly loaded engine, gives identical
  bytes (the `determinism` check; raster, float and SVG text);
* goldens written by one process were matched byte for byte by later processes (`init` then `run`,
  `bless` then `run`), for both renderers;
* independent checks of the metrics: all 24 CIEDE2000 pairs of Sharma, Wu and Dalal (2005) used in the
  unit tests reproduce to 5e-5; on the demo frames SSIM (once corrected for the border region that
  scikit-image crops), the largest CIEDE2000 value and the number of differing pixels agree with
  scikit-image 0.26 (0.99330 against 0.99330 after the border correction; 19.648 against 19.648; 11088
  against 11088 pixels), and MSE with numpy.

What was **not** verified: other operating systems, CPUs, compilers or Rust versions. The renderers
use floating point (trigonometry, curve flattening, coverage accumulation), so a different libm,
FMA contraction or SIMD path may move an anti-aliased edge pixel by a level or two. Exact hashes
therefore only help on one build of the tool on one kind of machine; across machines rely on the
tolerances (`pixel_de`, `aa_isolated`, `max_diff_fraction`) and keep the goldens produced by the
same CI image that verifies them. The GPU renderer is not part of this harness.

## Limits

* There is no SVG rasteriser in the workspace, so a render check cannot rasterise the SVG output and
  compare pixels; the `svg` renderer compares the SVG text exactly (any reordering of attributes is a
  failure).
* Bone overlays are not rendered by any CI check; audio is not compared.
* Region merging and the anti-aliasing rule are heuristics (edge test with a fixed step of 40, group
  size limit); a sub-pixel shift of a long edge forms a long thin group and is reported, not forgiven.
* The perceptual thresholds are defaults, not calibrated against animators' judgement.
* The perf check measures wall-clock time on a shared machine; keep it informational unless the
  runner is dedicated.
* `ci init` renders, fuzzes and exports once to calibrate, which is slow in debug builds.
* No pull-request integration (no bot comment, no automatic before/after GIF upload): the pieces
  (`--html`, `--artifacts`, `ci diff --flicker`) are there, the glue to a hosting service is not.
