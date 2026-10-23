# Drittanbieter-Lizenzen (generiert)

*Generiert von `scripts/licence-audit.sh` aus `cargo metadata --all-features` (alle Targets, normale + Build-Abhängigkeiten, Dev-Abhängigkeiten ausgenommen). Nicht von Hand bearbeiten. Keine Rechtsberatung; die SPDX-Ausdrücke sind das, was jedes Crate in seiner `Cargo.toml` angibt; sie wurden nicht unabhängig anhand der Quelldateien überprüft. Die Workspace-Crates selbst stehen unter MPL-2.0 (core, capi, wasm, gpu) und AGPL-3.0-or-later (armatura-cli); siehe LICENSE.*

**160 Drittanbieter-Crates** (eindeutige Kombination aus Name und Version). Gruppe A (Prüfung nötig): 1; Gruppe B (bemerkenswerte permissive / gemeinfreie / Datenlizenzen): 9; Gruppe C (im Stil von MIT / Apache-2.0): 150.

## A. Prüfung nötig: eine Copyleft-Bedingung ohne vollständig permissive Alternative

| Crate | Version | SPDX-Ausdruck | Quelle | Eingebunden von (Workspace) | Hinweis |
|---|---|---|---|---|---|
| smartstring | 1.0.1 | MPL-2.0+ | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | Copyleft oder nicht erkannte Bedingung; keine vollständig permissive Alternative im Ausdruck; MPL = Copyleft auf Dateiebene (schwach) |

## B. Bemerkenswerte permissive, gemeinfreie oder Datendatei-Lizenzen (keine MIT/Apache-Option angeboten)

| Crate | Version | SPDX-Ausdruck | Quelle | Eingebunden von (Workspace) | Hinweis |
|---|---|---|---|---|---|
| arrayref | 0.3.9 | BSD-2-Clause | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | nur BSD-2-Clause-Bedingungen |
| foldhash | 0.2.0 | Zlib | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | nur Zlib-Bedingungen |
| libloading | 0.8.9 | ISC | registry crates.io | armatura-cli, armatura-gpu | nur ISC-Bedingungen |
| slotmap | 1.1.1 | Zlib | registry crates.io | armatura-cli, armatura-gpu | nur Zlib-Bedingungen |
| tiny-keccak | 2.0.2 | CC0-1.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | nur CC0-1.0-Bedingungen |
| tiny-skia | 0.12.0 | BSD-3-Clause | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | nur BSD-3-Clause-Bedingungen |
| tiny-skia-path | 0.12.0 | BSD-3-Clause | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | nur BSD-3-Clause-Bedingungen |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | erfordert zusätzlich Unicode-3.0 (AND): Datendatei-Bedingungen zusätzlich zu MIT/Apache |
| zlib-rs | 0.6.8 | Zlib | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | nur Zlib-Bedingungen |

## C. MIT / Apache-2.0 (oder eine Dual-Lizenz, die sie einschließt)

| Crate | Version | SPDX-Ausdruck | Quelle | Eingebunden von (Workspace) | Hinweis |
|---|---|---|---|---|---|
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| ahash | 0.8.12 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| allocator-api2 | 0.2.21 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| android_system_properties | 0.1.6 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| arbitrary | 1.5.0 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| ash | 0.38.0+1.3.281 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| bit-set | 0.10.0 | Apache-2.0 OR MIT | registry crates.io | armatura-cli, armatura-gpu |  |
| bit-vec | 0.9.1 | Apache-2.0 OR MIT | registry crates.io | armatura-cli, armatura-gpu |  |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| bytemuck_derive | 1.12.1 | Zlib OR Apache-2.0 OR MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| cfg_aliases | 0.2.2 | MIT | registry crates.io | armatura-cli, armatura-gpu |  |
| codespan-reporting | 0.13.1 | Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| color_quant | 1.1.0 | MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| const-random | 0.1.18 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| const-random-macro | 0.1.16 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| crc32fast | 1.5.2 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| crunchy | 0.2.4 | MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| derive_arbitrary | 1.5.0 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| dispatch2 | 0.3.1 | Zlib OR Apache-2.0 OR MIT | registry crates.io | armatura-cli, armatura-gpu |  |
| dlib | 0.5.3 | MIT | registry crates.io | armatura-cli, armatura-gpu |  |
| document-features | 0.2.12 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| equivalent | 1.0.3 | Apache-2.0 OR MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| flate2 | 1.1.10 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| gif | 0.14.2 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| gl_generator | 0.14.0 | Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| glow | 0.17.0 | MIT OR Apache-2.0 OR Zlib | registry crates.io | armatura-cli, armatura-gpu |  |
| glutin_wgl_sys | 0.6.1 | Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| gpu-allocator | 0.28.0 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| half | 2.7.1 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| hound | 3.5.1 | Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| i_float | 5.0.0 | MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| i_key_sort | 0.12.0 | MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| i_overlay | 9.1.0 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| i_shape | 5.0.0 | MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| i_tree | 0.20.0 | MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| itoa | 1.0.18 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| jni-sys | 0.3.1 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| jni-sys | 0.4.1 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| jni-sys-macros | 0.4.1 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| jpeg-decoder | 0.3.2 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| js-sys | 0.3.106 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| khronos-egl | 6.0.0 | MIT/Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| khronos_api | 3.1.0 | Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| libc | 0.2.191 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| libm | 0.2.16 | MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| litrs | 1.0.1 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| log | 0.4.34 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| memchr | 2.8.3 | Unlicense OR MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| naga | 30.0.1 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| naga-types | 30.0.1 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| ndk-sys | 0.6.0+11769913 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| objc2 | 0.6.5 | MIT | registry crates.io | armatura-cli, armatura-gpu |  |
| objc2-core-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registry crates.io | armatura-cli, armatura-gpu |  |
| objc2-encode | 4.1.0 | MIT | registry crates.io | armatura-cli, armatura-gpu |  |
| objc2-foundation | 0.3.2 | MIT | registry crates.io | armatura-cli, armatura-gpu |  |
| objc2-metal | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registry crates.io | armatura-cli, armatura-gpu |  |
| objc2-quartz-core | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registry crates.io | armatura-cli, armatura-gpu |  |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| ordered-float | 5.5.0 | MIT | registry crates.io | armatura-cli, armatura-gpu |  |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| png | 0.18.1 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| pollster | 1.0.1 | Apache-2.0/MIT | registry crates.io | armatura-cli, armatura-gpu |  |
| portable-atomic | 1.15.0 | Apache-2.0 OR MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| portable-atomic-util | 0.2.8 | Apache-2.0 OR MIT | registry crates.io | armatura-cli, armatura-gpu |  |
| presser | 0.3.1 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| profiling | 1.0.18 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| quote | 1.0.47 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib | registry crates.io | armatura-cli, armatura-gpu |  |
| raw-window-metal | 1.1.0 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| redox_syscall | 0.5.18 | MIT | registry crates.io | armatura-cli, armatura-gpu |  |
| renderdoc-sys | 1.1.0 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| rhai | 1.26.1 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| rhai_codegen | 3.2.0 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| rustc-hash | 1.1.0 | Apache-2.0/MIT | registry crates.io | armatura-cli, armatura-gpu |  |
| rustversion | 1.0.23 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| serde | 1.0.229 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| simd-adler32 | 0.3.10 | MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| slab | 0.4.12 | MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| smallvec | 1.16.3 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| spirv | 0.4.0+sdk-1.4.341.0 | Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| strict-num | 0.1.1 | MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| syn | 2.0.119 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| syn | 3.0.9 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| thin-vec | 0.2.21 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| tokio | 1.54.0 | MIT | registry crates.io | armatura-cli, armatura-gpu |  |
| unicode-width | 0.2.2 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| version_check | 0.9.5 | MIT/Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| wasm-bindgen | 0.2.129 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| wasm-bindgen-futures | 0.4.79 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| wasm-bindgen-macro | 0.2.129 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| wasm-bindgen-macro-support | 0.2.129 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| wasm-bindgen-shared | 0.2.129 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| wayland-sys | 0.31.11 | MIT | registry crates.io | armatura-cli, armatura-gpu |  |
| web-sys | 0.3.106 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| web-time | 1.1.0 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| weezl | 0.1.12 | MIT OR Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| wgpu | 30.0.1 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| wgpu-core | 30.0.1 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| wgpu-core-deps-emscripten | 30.0.1 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| wgpu-core-deps-windows-linux-android | 30.0.1 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| wgpu-hal | 30.0.1 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| wgpu-naga-bridge | 30.0.1 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| wgpu-types | 30.0.1 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| windows | 0.62.2 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| windows-collections | 0.3.2 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| windows-core | 0.62.2 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| windows-future | 0.3.2 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| windows-numerics | 0.3.1 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| windows-result | 0.4.1 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| windows-threading | 0.2.1 | MIT OR Apache-2.0 | registry crates.io | armatura-cli, armatura-gpu |  |
| xml-rs | 0.8.29 | MIT | registry crates.io | armatura-cli, armatura-gpu |  |
| zerocopy | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| zerocopy-derive | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| zip | 2.6.1 | MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| zmij | 1.0.23 | MIT | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| zopfli | 0.8.4 | Apache-2.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |


## Algorithmus-Quellen (Ideen und Konstanten, kein kopierter Code)
- **Bézier-Kurvenanpassung** (`autotrace.rs`, `lod.rs`): Die Struktur des Algorithmus folgt P. J. Schneider, „An Algorithm for Automatically Fitting Digitized Curves", *Graphics Gems I* (A. Glassner, Hrsg., Academic Press, 1990); der Code wurde unabhängig geschrieben. Die Code-Distribution von Graphics Gems besagt in ihrem README bzw. Lizenzhinweis, dass ihr Code in jedem Programm, Produkt oder jeder Bibliothek verwendet werden darf, kommerziell oder nicht, dass eine Namensnennung erwünscht, aber nicht erforderlich ist und dass er ohne Haftung „wie besehen" bereitgestellt wird; ausgeschlossen ist nur, den Text des Codes als eigenen auszugeben und weiterzuverkaufen. Wir haben diesen Code nicht kopiert; dieser Hinweis hält die permissiven Bedingungen fest, falls ein Prüfer danach fragt (den Wortlaut bitte anhand des README im Graphics-Gems-Repository überprüfen).
- **Hash-Konstanten** (`noise.rs`): xorshift-multiply-Finalisierer aus `lowbias32` von Chris Wellons (gemeinfrei, Hash Prospector, 2018); 0x9E3779B1 (2^32 / goldener Schnitt, Knuth); 0x85EBCA6B (MurmurHash3-Finalisierer, Austin Appleby, gemeinfrei).
- **Easing**: `Ease::Bounce` und `Ease::Back` sind eigene Herleitungen (CLEANROOM.md); die BSD-3-Clause-Konstanten von Robert Penner werden nicht mehr verwendet.
- Weitere Algorithmen sind pro Modul mit ihren Veröffentlichungen und Lehrbüchern in CLEANROOM.md aufgeführt.

## Mitgelieferte Modelldatei (Namensnennung)
`armatura-core/models/face23_synth.bin` (eine 650 KB große Gewichtsdatei eines Gesichtsdetektors für `facedet`) ist (c) 2026 Alexandru-Ionuț Chiuță (Alexio), lizenziert unter CC-BY-4.0 (https://creativecommons.org/licenses/by/4.0/; Namensnennung und Hinweis auf Änderungen erforderlich). Sie wurde **ausschließlich mit Gesichtern trainiert, die vom projekteigenen synthetischen Gesichtsgenerator** (`facegen.rs`) **erzeugt wurden**; es wurden keine Fotografien, keine Datensätze Dritter und keine vortrainierten Gewichte Dritter verwendet.
## Builds ohne das MPL-2.0+-Crate (smartstring)

Die obige Tabelle ist die Vereinigung über den gesamten Workspace mit allen Features, wobei die Standard-CLI `scripting` aktiviert und Cargo die Features vereinheitlicht. Pro Paket geprüft mit `cargo tree -i smartstring -e normal,build`:

| Build | smartstring im Baum? |
|---|---|
| `-p armatura-core --no-default-features` | nein |
| `-p armatura-cli --no-default-features` | nein |
| `-p armatura-capi` | nein |
| `-p armatura-wasm` | nein |
| `-p armatura-cli` | ja |
