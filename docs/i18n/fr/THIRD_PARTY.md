# Licences des composants tiers (généré)

*Généré par `scripts/licence-audit.sh` à partir de `cargo metadata --all-features` (toutes les cibles, dépendances normales + de compilation, dépendances de développement exclues). Ne pas modifier à la main. Ceci n'est pas un avis juridique ; les expressions SPDX sont celles que chaque crate déclare dans son `Cargo.toml`, elles n'ont pas été vérifiées indépendamment par rapport aux fichiers sources. Les crates de l'espace de travail sont elles-mêmes sous MPL-2.0 (core, capi, wasm, gpu) et AGPL-3.0-or-later (armatura-cli) ; voir LICENSE.*

**160 crates tierces** (nom+version distincts). Groupe A (à examiner) : 1 ; groupe B (licences permissives / domaine public / de données notables) : 9 ; groupe C (style MIT / Apache-2.0) : 150.

## A. À examiner : une clause de copyleft sans alternative entièrement permissive

| Crate | Version | Expression SPDX | Source | Requise par (espace de travail) | Note |
|---|---|---|---|---|---|
| smartstring | 1.0.1 | MPL-2.0+ | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | clause de copyleft ou non reconnue ; aucune alternative entièrement permissive dans l'expression ; MPL = copyleft (faible) au niveau du fichier |

## B. Licences permissives, de domaine public ou de fichiers de données notables (aucune option MIT/Apache proposée)

| Crate | Version | Expression SPDX | Source | Requise par (espace de travail) | Note |
|---|---|---|---|---|---|
| arrayref | 0.3.9 | BSD-2-Clause | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | uniquement les termes BSD-2-Clause |
| foldhash | 0.2.0 | Zlib | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | uniquement les termes Zlib |
| libloading | 0.8.9 | ISC | registre crates.io | armatura-cli, armatura-gpu | uniquement les termes ISC |
| slotmap | 1.1.1 | Zlib | registre crates.io | armatura-cli, armatura-gpu | uniquement les termes Zlib |
| tiny-keccak | 2.0.2 | CC0-1.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | uniquement les termes CC0-1.0 |
| tiny-skia | 0.12.0 | BSD-3-Clause | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | uniquement les termes BSD-3-Clause |
| tiny-skia-path | 0.12.0 | BSD-3-Clause | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | uniquement les termes BSD-3-Clause |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | exige aussi Unicode-3.0 (AND) : termes relatifs aux fichiers de données en plus de MIT/Apache |
| zlib-rs | 0.6.8 | Zlib | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | uniquement les termes Zlib |

## C. MIT / Apache-2.0 (ou une double licence les incluant)

| Crate | Version | Expression SPDX | Source | Requise par (espace de travail) | Note |
|---|---|---|---|---|---|
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| ahash | 0.8.12 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| allocator-api2 | 0.2.21 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| android_system_properties | 0.1.6 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| arbitrary | 1.5.0 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| ash | 0.38.0+1.3.281 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| bit-set | 0.10.0 | Apache-2.0 OR MIT | registre crates.io | armatura-cli, armatura-gpu |  |
| bit-vec | 0.9.1 | Apache-2.0 OR MIT | registre crates.io | armatura-cli, armatura-gpu |  |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| bytemuck_derive | 1.12.1 | Zlib OR Apache-2.0 OR MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| cfg_aliases | 0.2.2 | MIT | registre crates.io | armatura-cli, armatura-gpu |  |
| codespan-reporting | 0.13.1 | Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| color_quant | 1.1.0 | MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| const-random | 0.1.18 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| const-random-macro | 0.1.16 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| crc32fast | 1.5.2 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| crunchy | 0.2.4 | MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| derive_arbitrary | 1.5.0 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| dispatch2 | 0.3.1 | Zlib OR Apache-2.0 OR MIT | registre crates.io | armatura-cli, armatura-gpu |  |
| dlib | 0.5.3 | MIT | registre crates.io | armatura-cli, armatura-gpu |  |
| document-features | 0.2.12 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| equivalent | 1.0.3 | Apache-2.0 OR MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| flate2 | 1.1.10 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| gif | 0.14.2 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| gl_generator | 0.14.0 | Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| glow | 0.17.0 | MIT OR Apache-2.0 OR Zlib | registre crates.io | armatura-cli, armatura-gpu |  |
| glutin_wgl_sys | 0.6.1 | Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| gpu-allocator | 0.28.0 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| half | 2.7.1 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| hound | 3.5.1 | Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| i_float | 5.0.0 | MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| i_key_sort | 0.12.0 | MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| i_overlay | 9.1.0 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| i_shape | 5.0.0 | MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| i_tree | 0.20.0 | MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| itoa | 1.0.18 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| jni-sys | 0.3.1 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| jni-sys | 0.4.1 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| jni-sys-macros | 0.4.1 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| jpeg-decoder | 0.3.2 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| js-sys | 0.3.106 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| khronos-egl | 6.0.0 | MIT/Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| khronos_api | 3.1.0 | Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| libc | 0.2.191 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| libm | 0.2.16 | MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| litrs | 1.0.1 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| log | 0.4.34 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| memchr | 2.8.3 | Unlicense OR MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| naga | 30.0.1 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| naga-types | 30.0.1 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| ndk-sys | 0.6.0+11769913 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| objc2 | 0.6.5 | MIT | registre crates.io | armatura-cli, armatura-gpu |  |
| objc2-core-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registre crates.io | armatura-cli, armatura-gpu |  |
| objc2-encode | 4.1.0 | MIT | registre crates.io | armatura-cli, armatura-gpu |  |
| objc2-foundation | 0.3.2 | MIT | registre crates.io | armatura-cli, armatura-gpu |  |
| objc2-metal | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registre crates.io | armatura-cli, armatura-gpu |  |
| objc2-quartz-core | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registre crates.io | armatura-cli, armatura-gpu |  |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| ordered-float | 5.5.0 | MIT | registre crates.io | armatura-cli, armatura-gpu |  |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| png | 0.18.1 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| pollster | 1.0.1 | Apache-2.0/MIT | registre crates.io | armatura-cli, armatura-gpu |  |
| portable-atomic | 1.15.0 | Apache-2.0 OR MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| portable-atomic-util | 0.2.8 | Apache-2.0 OR MIT | registre crates.io | armatura-cli, armatura-gpu |  |
| presser | 0.3.1 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| profiling | 1.0.18 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| quote | 1.0.47 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib | registre crates.io | armatura-cli, armatura-gpu |  |
| raw-window-metal | 1.1.0 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| redox_syscall | 0.5.18 | MIT | registre crates.io | armatura-cli, armatura-gpu |  |
| renderdoc-sys | 1.1.0 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| rhai | 1.26.1 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| rhai_codegen | 3.2.0 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| rustc-hash | 1.1.0 | Apache-2.0/MIT | registre crates.io | armatura-cli, armatura-gpu |  |
| rustversion | 1.0.23 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| serde | 1.0.229 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| simd-adler32 | 0.3.10 | MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| slab | 0.4.12 | MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| smallvec | 1.16.3 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| spirv | 0.4.0+sdk-1.4.341.0 | Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| strict-num | 0.1.1 | MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| syn | 2.0.119 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| syn | 3.0.9 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| thin-vec | 0.2.21 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| tokio | 1.54.0 | MIT | registre crates.io | armatura-cli, armatura-gpu |  |
| unicode-width | 0.2.2 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| version_check | 0.9.5 | MIT/Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| wasm-bindgen | 0.2.129 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| wasm-bindgen-futures | 0.4.79 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| wasm-bindgen-macro | 0.2.129 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| wasm-bindgen-macro-support | 0.2.129 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| wasm-bindgen-shared | 0.2.129 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| wayland-sys | 0.31.11 | MIT | registre crates.io | armatura-cli, armatura-gpu |  |
| web-sys | 0.3.106 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| web-time | 1.1.0 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| weezl | 0.1.12 | MIT OR Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| wgpu | 30.0.1 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| wgpu-core | 30.0.1 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| wgpu-core-deps-emscripten | 30.0.1 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| wgpu-core-deps-windows-linux-android | 30.0.1 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| wgpu-hal | 30.0.1 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| wgpu-naga-bridge | 30.0.1 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| wgpu-types | 30.0.1 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| windows | 0.62.2 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| windows-collections | 0.3.2 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| windows-core | 0.62.2 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| windows-future | 0.3.2 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| windows-numerics | 0.3.1 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| windows-result | 0.4.1 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| windows-threading | 0.2.1 | MIT OR Apache-2.0 | registre crates.io | armatura-cli, armatura-gpu |  |
| xml-rs | 0.8.29 | MIT | registre crates.io | armatura-cli, armatura-gpu |  |
| zerocopy | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| zerocopy-derive | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| zip | 2.6.1 | MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| zmij | 1.0.23 | MIT | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| zopfli | 0.8.4 | Apache-2.0 | registre crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |


## Sources des algorithmes (idées et constantes, pas de code copié)
- **Ajustement de courbes de Bézier** (`autotrace.rs`, `lod.rs`) : la structure de l'algorithme suit P. J. Schneider, « An Algorithm for Automatically Fitting Digitized Curves », *Graphics Gems I* (A. Glassner, dir., Academic Press, 1990) ; le code a été écrit de façon indépendante. La distribution du code de Graphics Gems indique, dans son README/sa note de licence, que son code peut être utilisé dans tout programme, produit ou bibliothèque, commercial ou non, que la mention de la source est appréciée mais non obligatoire, et qu'il est fourni tel quel, sans responsabilité ; seul le fait de s'approprier le texte du code et de le revendre est exclu. Nous n'avons pas copié ce code ; cette note consigne les termes permissifs au cas où un relecteur le demanderait (vérifiez la formulation dans le README du dépôt Graphics Gems).
- **Constantes de hachage** (`noise.rs`) : finaliseur xorshift-multiply de `lowbias32` de Chris Wellons (domaine public, Hash Prospector, 2018) ; 0x9E3779B1 (2^32 / nombre d'or, Knuth) ; 0x85EBCA6B (finaliseur de MurmurHash3, Austin Appleby, domaine public).
- **Easing** : `Ease::Bounce` et `Ease::Back` sont des dérivations propres (CLEANROOM.md) ; les constantes BSD-3-Clause de Robert Penner ne sont plus utilisées.
- Les autres algorithmes sont répertoriés par module, avec leurs articles et manuels, dans CLEANROOM.md.

## Modèle fourni (attribution)
`armatura-core/models/face23_synth.bin` (un fichier de poids de 650 Ko d'un détecteur de visages pour `facedet`) est (c) 2026 Alexandru-Ionuț Chiuță (Alexio), sous licence CC-BY-4.0 (https://creativecommons.org/licenses/by/4.0/ ; mentionnez l'auteur et indiquez les modifications). Il a été entraîné **uniquement sur des visages générés par le générateur de visages synthétiques propre à ce projet** (`facegen.rs`) ; aucune photographie, aucun jeu de données tiers et aucun poids pré-entraîné tiers n'ont été utilisés.
## Compilations sans la crate MPL-2.0+ (smartstring)

Le tableau ci-dessus est l'union sur l'ensemble de l'espace de travail avec toutes les fonctionnalités, où la CLI par défaut active `scripting` et où Cargo unifie les fonctionnalités. Vérifié par paquet avec `cargo tree -i smartstring -e normal,build` :

| Compilation | smartstring dans l'arbre ? |
|---|---|
| `-p armatura-core --no-default-features` | non |
| `-p armatura-cli --no-default-features` | non |
| `-p armatura-capi` | non |
| `-p armatura-wasm` | non |
| `-p armatura-cli` | oui |
