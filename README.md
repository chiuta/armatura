<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio) -->
# ARMATURA

**A 2D vector rigging and animation engine in Rust: bones, IK, skinning, deformers, a state machine runtime, a command-line toolbox, a browser studio and embeddable libraries, in one workspace.** Romanian version: [README.ro.md](README.ro.md).

## Summary and philosophy

ARMATURA lets you build a character or a scene from vector shapes and images, rig it with bones and constraints, animate it with keys, clips and physics, and send it out as GIF, video, Lottie, glTF, a self-contained HTML player or an interactive runtime. It is made of small, deterministic pieces: the same project and the same seed give the same pixels, every tool reads and writes a documented JSON project, and every command works from a terminal, so animation can be scripted, reviewed, diffed and tested like source code.

Principles that shape the project:

* **Independent implementation.** Written from public specifications, papers and textbook mathematics. Competitor feature lists were consulted for *scope*; no third-party source code was copied, and the project neither reads nor writes any proprietary format. It is not a strict clean-room process (see [CLEANROOM.md](CLEANROOM.md), including its research disclosure) and it is not affiliated with, derived from or compatible with Moho or any other commercial product. Code was largely written with an AI model.
* **Honest status.** Every feature is described together with what it does *not* do ([docs/LIMITATIONS.md](docs/LIMITATIONS.md), [ROADMAP.md](ROADMAP.md) section 9).
* **Safe by default.** No panics on input, finite-value checks, resource caps, atomic writes, asset paths confined to the project folder ([docs/SECURITY.md](docs/SECURITY.md)).
* **Open formats first.** OpenRaster, OCA, SVG, Lottie, glTF, APNG, WebM, EXR; no proprietary file formats by design.
* **Small dependency footprint** with a licence policy checked by tests and `cargo deny`.

## Status

> **Pre-release, one environment.** All crates are version `0.1.0` and nothing has been released; "v0.5 / v0.6 / v0.7" are *feature levels* in the history, not release numbers ([CHANGELOG.md](CHANGELOG.md)).
>
> | Verified | Not verified |
> |---|---|
> | 1368 tests in `cargo test --workspace` plus 19 doc-tests pass (22 Oct 2026, Linux x86-64); four headless-Chromium smoke tests of the studio pass (124, 60, 89, 35 checks for the ui, collab, overlays and suggest scripts); the CLI, the Rust facade and the C ABI (also under ASan/UBSan) were run with the examples in the docs | **The WebAssembly module has never been built or run** (the wasm32 target could not be installed; [docs/WASM.md](docs/WASM.md)); the studio was never used by a person in production, nor in a browser other than Chromium; no real GPU (only a software adapter), webcam, photograph or motion-capture file was ever used; Windows and macOS untested; no security audit, lawyer review or patent search |
>
> Read [docs/LIMITATIONS.md](docs/LIMITATIONS.md) (73 numbered limits with severity and workaround) before you depend on anything.

## Features by area

Details and exact options: [docs/CLI_REFERENCE.md](docs/CLI_REFERENCE.md); what is partial or missing: [docs/LIMITATIONS.md](docs/LIMITATIONS.md).

* **Drawing model.** Vector paths, fills, strokes (dashes, variable width), linear and radial gradients, masks, blend modes, per-layer effects (blur, shadow, tint, fill), image layers with mesh warp, liquid metaballs, polygon booleans, deterministic particles, multiplane camera with saved shots, depth of field.
* **Rigging.** Bones with forward kinematics; two-bone, soft, stretch and FABRIK IK; limits, constraints, springs; skinning with weight tools; lattice, spine, morph, wobble and cage deformers; mesh tracing from alpha; warp pivots and glue; drivers (sliders, joysticks, view swap); skin sets; auto-rig from layer names; eye clipping; layout containers and scroll regions.
* **Animation.** Keys with Bézier/step/spring/bounce easing, expressions and driven channels, switch layers, takes and a layered clip stack, loop and in-place conversion, foot lock, magnets, path following, idle and overlap generators, chain/hair/cloth physics, lip-sync from audio or a transcript, speech-aware head motion, BVH retargeting, live input (MIDI, mouse, gamepad, webcam landmarks you supply), audio tracks with trim, fade and mix.
* **Runtime.** State machines with 1-D blend trees, data binding, scroll physics, level of detail with an error bound, a Rhai scripting sandbox, an agent (MCP) server over patch operations.
* **Output.** SVG, PNG, PNG sequences and sprite sheets, GIF, APNG, MP4/WebM (through ffmpeg), self-contained HTML player, Lottie and dotLottie, glTF 2.0 binary, OpenEXR/16-bit PNG, OCA; size-targeted export (`export-fit`). Import: SVG, OpenRaster, Lottie, BVH, PNG/GIF/APNG sequences.
* **Quality tooling.** `check`, `lint`, semantic `diff`, pose fuzzing with heat maps, animation CI with perceptual diff and golden frames ([docs/CI.md](docs/CI.md)), version control with semantic merge (`vcs`), safety checks for flashing and motion.
* **Studio.** Browser editor served by the CLI: canvas gizmos, timeline, dope sheet and graph editor, overlays, command palette and shortcuts, undo/redo, real-time multi-user editing with an optional token and a persistent log ([docs/STUDIO.md](docs/STUDIO.md)).
* **Libraries.** Stable Rust facade ([docs/API.md](docs/API.md)), C ABI ([docs/C_ABI.md](docs/C_ABI.md)), WebAssembly module and JavaScript wrapper (unbuilt; [docs/WASM.md](docs/WASM.md)), optional wgpu renderer.

## Quick start

Requirements: a recent stable Rust toolchain. `ffmpeg` is needed only for MP4/WebM export and for mp3/ogg audio; it is never bundled. Every command below was run for this README (debug build, Linux). Release builds (`cargo build --release`) are much faster for rendering and are used the same way with `target/release`.

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

`voice.wav` is any mono or stereo WAV file you provide. The studio listens on `127.0.0.1` only and refuses requests for other host names; to share it, read [docs/SECURITY.md](docs/SECURITY.md) first.

Use the libraries:

```sh
cargo build -p armatura-capi                      # target/debug/libarmatura_capi.so and .a
gcc -std=c99 -o smoke armatura-capi/tests/smoke.c -Ltarget/debug -larmatura_capi
LD_LIBRARY_PATH=target/debug ./smoke demo.json    # abi 1, 48 frames, 320x240, rc=0, checksum=...
```

The Rust facade is `armatura_core::prelude` ([docs/API.md](docs/API.md)); more C and Python examples are in [docs/C_ABI.md](docs/C_ABI.md).

Tests (about 14 minutes in a debug build on a shared machine; see [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)):

```sh
cargo test --workspace
cargo test --doc -p armatura-core
cargo test -p armatura-cli --test spdx_audit
```

## Repository layout

| Path | Contents | Licence |
|---|---|---|
| `armatura-core/` | engine, formats, tools, stable facade `api` | MPL-2.0 |
| `armatura-cli/` | the `armatura` binary, studio and collaboration server, end-to-end and smoke tests | AGPL-3.0-or-later |
| `armatura-capi/` | C ABI (`armatura.h`, shared and static library) | MPL-2.0 |
| `armatura-wasm/` | WebAssembly ABI, JavaScript wrapper, demo page (unbuilt) | MPL-2.0 |
| `armatura-gpu/` | optional wgpu renderer (not a default member) | MPL-2.0 |
| `docs/` | documentation (CC-BY-4.0) and generated third-party files | CC-BY-4.0 |
| `scripts/` | `e2e.sh`, `licence-audit.sh` | MPL-2.0 |
| `LICENSES/`, `REUSE.toml`, `deny.toml` | licence texts, licence metadata, dependency policy | - |
| `ROADMAP.md`, `FEATURES.md`, `CLEANROOM.md`, `LICENSING.md`, `TRADEMARK.md`, `CONTRIBUTING.md`, `CHANGELOG.md` | project documents | CC-BY-4.0 |

## Documentation map

Start at [docs/INDEX.md](docs/INDEX.md) (all documents, audiences, reading paths).

| Page | For |
|---|---|
| [docs/GETTING_STARTED.md](docs/GETTING_STARTED.md) | first project, render, export |
| [docs/CLI_REFERENCE.md](docs/CLI_REFERENCE.md) | every command and option |
| [docs/STUDIO.md](docs/STUDIO.md) | the browser editor |
| [docs/RIGGING_GUIDE.md](docs/RIGGING_GUIDE.md) | building rigs |
| [docs/ANIMATION_GUIDE.md](docs/ANIMATION_GUIDE.md) | animating, clips, physics, audio |
| [docs/PROJECT_FORMAT.md](docs/PROJECT_FORMAT.md) | the JSON project format |
| [docs/EXPORT_FORMATS.md](docs/EXPORT_FORMATS.md) | what each format carries and drops |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | design and modules |
| [docs/GLOSSARY.md](docs/GLOSSARY.md) | terms |
| [docs/FAQ.md](docs/FAQ.md) | common questions |
| [docs/API.md](docs/API.md) | Rust facade 1.4 |
| [docs/C_ABI.md](docs/C_ABI.md) | C interface |
| [docs/WASM.md](docs/WASM.md) | WebAssembly (never built) |
| [docs/CI.md](docs/CI.md) | animation CI |
| [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) | build, test, conventions, release |
| [docs/SECURITY.md](docs/SECURITY.md) | threat model and hardening |
| [docs/LIMITATIONS.md](docs/LIMITATIONS.md) | known limits |
| [docs/PATENT_NOTES.md](docs/PATENT_NOTES.md), [docs/THIRD_PARTY.md](docs/THIRD_PARTY.md), [docs/THIRD_PARTY_NOTICES.md](docs/THIRD_PARTY_NOTICES.md) | patents, dependency licences |
| [CHANGELOG.md](CHANGELOG.md), [ROADMAP.md](ROADMAP.md), [FEATURES.md](FEATURES.md) | history, plan and detailed status, feature matrix |
| [CLEANROOM.md](CLEANROOM.md), [LICENSING.md](LICENSING.md), [TRADEMARK.md](TRADEMARK.md), [CONTRIBUTING.md](CONTRIBUTING.md) | provenance, licences, name policy, contribution rules |

## Licence summary

* **MPL-2.0** for the libraries `armatura-core`, `armatura-capi`, `armatura-wasm`, `armatura-gpu` and for `scripts/`: you may link them into your own program, open or closed; changes to those files must be published under the MPL.
* **AGPL-3.0-or-later** for `armatura-cli` (the command line and the studio). If you modify it and let others interact with it over a network, section 13 requires you to offer them your source: `armatura studio --source-url URL` publishes where it is; `/source` and `armatura --version` carry the offer.
* **CC-BY-4.0** for documentation and synthetic assets (the synthetic face-detector model included).
* **Your projects and renders stay yours.**
* The name and logo are not covered by these licences ([TRADEMARK.md](TRADEMARK.md)). Patent status is not assessed ([docs/PATENT_NOTES.md](docs/PATENT_NOTES.md)). A commercial or dual licence for contributed code is not possible without a contributor licence agreement, and none exists.
* Third-party crates: 160 at the last audit, all permissive except one weak-copyleft crate behind the optional `scripting` feature ([docs/THIRD_PARTY.md](docs/THIRD_PARTY.md)). ffmpeg is never bundled.

See [LICENSE](LICENSE), [LICENSES/](LICENSES/) and [LICENSING.md](LICENSING.md). None of this is legal advice; a lawyer review is still outstanding.

## Contributing

Contributions are welcome under the licence of the part they change. Read [CONTRIBUTING.md](CONTRIBUTING.md) (every commit needs a `Signed-off-by` line, DCO 1.1), [CLEANROOM.md](CLEANROOM.md) (implement from public sources only; do not copy from proprietary or incompatibly licensed code) and [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) (conventions, checklist for a new module, tests). Good first contributions: a fix from [docs/LIMITATIONS.md](docs/LIMITATIONS.md), a real run of the WebAssembly checklist, tests on Windows or macOS, a real-GPU benchmark.

Security issues: do not open a public issue; follow [docs/SECURITY.md](docs/SECURITY.md) section 10.

## Support

ARMATURA is developed by Alexandru-Ionuț Chiuță (Alexio). If it is useful to you, you can support the work:

* Buy Me a Coffee: <https://buymeacoffee.com/echo.of.the.strings>
* Patreon: <https://patreon.com/c/alexio_tf>

Maintainer contact (also printed by `armatura --version`): alexio@trom.tf.

## Acknowledgements

* The authors of the public papers and textbooks the algorithms follow, named per module in [CLEANROOM.md](CLEANROOM.md) (Bézier and spline mathematics, inverse kinematics, linear-blend skinning, mean-value coordinates, curve fitting and others).
* The open specifications ARMATURA reads and writes: SVG, OpenRaster, OCA, Lottie/dotLottie, glTF, APNG, WebM, OpenEXR, BVH, PNG, GIF.
* The Rust crates it builds on, listed with their licences in [docs/THIRD_PARTY.md](docs/THIRD_PARTY.md), notably `tiny-skia`, `serde`, `png`, `zip`, `rhai` and `wgpu`.
* Everyone who supports the project through the links above.
