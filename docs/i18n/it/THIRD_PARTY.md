# Licenze di terze parti (generato)

*Generato da `scripts/licence-audit.sh` a partire da `cargo metadata --all-features` (tutti i target, dipendenze normali + di build, dev-dependencies escluse). Non modificare a mano. Non è consulenza legale; le espressioni SPDX sono quelle che ogni crate dichiara nel proprio `Cargo.toml` e non sono state verificate in modo indipendente rispetto ai file sorgente. I crate del workspace sono a loro volta MPL-2.0 (core, capi, wasm, gpu) e AGPL-3.0-or-later (armatura-cli); vedi LICENSE.*

**160 crate di terze parti** (nome+versione distinti). Gruppo A (da rivedere): 1; gruppo B (licenze permissive / di pubblico dominio / sui dati degne di nota): 9; gruppo C (stile MIT / Apache-2.0): 150.

## A. Da rivedere: un termine copyleft senza alternativa interamente permissiva

| Crate | Versione | Espressione SPDX | Origine | Richiesto da (workspace) | Nota |
|---|---|---|---|---|---|
| smartstring | 1.0.1 | MPL-2.0+ | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | termine copyleft o non riconosciuto; nessuna alternativa interamente permissiva nell'espressione; MPL = copyleft (debole) a livello di file |

## B. Licenze permissive, di pubblico dominio o sui file di dati degne di nota (nessuna opzione MIT/Apache offerta)

| Crate | Versione | Espressione SPDX | Origine | Richiesto da (workspace) | Nota |
|---|---|---|---|---|---|
| arrayref | 0.3.9 | BSD-2-Clause | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | solo termini BSD-2-Clause |
| foldhash | 0.2.0 | Zlib | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | solo termini Zlib |
| libloading | 0.8.9 | ISC | registry crates.io | armatura-cli, armatura-gpu | solo termini ISC |
| slotmap | 1.1.1 | Zlib | registry crates.io | armatura-cli, armatura-gpu | solo termini Zlib |
| tiny-keccak | 2.0.2 | CC0-1.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | only CC0-1.0 terms |
| tiny-skia | 0.12.0 | BSD-3-Clause | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | solo termini BSD-3-Clause |
| tiny-skia-path | 0.12.0 | BSD-3-Clause | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | solo termini BSD-3-Clause |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | richiede anche Unicode-3.0 (AND): termini sui file di dati in aggiunta a MIT/Apache |
| zlib-rs | 0.6.8 | Zlib | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | solo termini Zlib |

## C. MIT / Apache-2.0 (o una doppia licenza che le include)

| Crate | Versione | Espressione SPDX | Origine | Richiesto da (workspace) | Nota |
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


## Fonti degli algoritmi (idee e costanti, non codice copiato)
- **Approssimazione di curve di Bézier** (`autotrace.rs`, `lod.rs`): la struttura dell'algoritmo segue P. J. Schneider, "An Algorithm for Automatically Fitting Digitized Curves", *Graphics Gems I* (A. Glassner, a cura di, Academic Press, 1990); il codice è stato scritto in modo indipendente. La distribuzione del codice di Graphics Gems dichiara, nel proprio README/nota di licenza, che il suo codice può essere usato in qualsiasi programma, prodotto o libreria, commerciale o meno, che l'attribuzione è gradita ma non obbligatoria e che viene fornito così com'è, senza responsabilità; è escluso solo attribuirsi come propri il testo del codice e rivenderlo. Non abbiamo copiato quel codice; questa nota registra i termini permissivi nel caso un revisore lo chieda (verifica la formulazione rispetto al README del repository di Graphics Gems).
- **Costanti di hash** (`noise.rs`): finalizzatore xorshift-multiply da `lowbias32` di Chris Wellons (pubblico dominio, Hash Prospector, 2018); 0x9E3779B1 (2^32 / sezione aurea, Knuth); 0x85EBCA6B (finalizzatore di MurmurHash3, Austin Appleby, pubblico dominio).
- **Easing**: `Ease::Bounce` ed `Ease::Back` sono derivazioni proprie (CLEANROOM.md); le costanti BSD-3-Clause di Robert Penner non sono più usate.
- Gli altri algoritmi sono elencati per modulo, con i relativi articoli e manuali, in CLEANROOM.md.

## Modello incluso (attribuzione)
`armatura-core/models/face23_synth.bin` (un file di pesi da 650 KB del rilevatore di volti per `facedet`) è (c) 2026 Alexandru-Ionuț Chiuță (Alexio), con licenza CC-BY-4.0 (https://creativecommons.org/licenses/by/4.0/; attribuire il merito e indicare le modifiche). È stato addestrato **esclusivamente su volti generati dal generatore di volti sintetici di questo progetto** (`facegen.rs`); non sono stati usati fotografie, dataset di terzi né pesi pre-addestrati di terzi.
## Build senza il crate MPL-2.0+ (smartstring)

La tabella qui sopra è l'unione sull'intero workspace con tutte le feature, dove la CLI predefinita abilita `scripting` e Cargo unifica le feature. Verificato per pacchetto con `cargo tree -i smartstring -e normal,build`:

| Build | smartstring nell'albero? |
|---|---|
| `-p armatura-core --no-default-features` | no |
| `-p armatura-cli --no-default-features` | no |
| `-p armatura-capi` | no |
| `-p armatura-wasm` | no |
| `-p armatura-cli` | sì |
