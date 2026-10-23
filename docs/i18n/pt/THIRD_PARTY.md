# Licenças de terceiros (gerado)

*Gerado por `scripts/licence-audit.sh` a partir de `cargo metadata --all-features` (todos os alvos, dependências normais e de build, dependências de desenvolvimento excluídas). Não edite manualmente. Isto não é aconselhamento jurídico; as expressões SPDX são o que cada crate declara em seu `Cargo.toml`, e não foram verificadas de forma independente em relação aos arquivos-fonte. Os próprios crates do workspace são MPL-2.0 (core, capi, wasm, gpu) e AGPL-3.0-or-later (armatura-cli); veja LICENSE.*

**160 crates de terceiros** (nome+versão distintos). Grupo A (requer revisão): 1; grupo B (licenças permissivas / de domínio público / de dados dignas de nota): 9; grupo C (estilo MIT / Apache-2.0): 150.

## A. Requer revisão: um termo de copyleft sem alternativa totalmente permissiva

| Crate | Versão | Expressão SPDX | Origem | Incluído por (workspace) | Observação |
|---|---|---|---|---|---|
| smartstring | 1.0.1 | MPL-2.0+ | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | termo de copyleft ou não reconhecido; nenhuma alternativa totalmente permissiva na expressão; MPL = copyleft (fraco) no nível de arquivo |

## B. Licenças permissivas, de domínio público ou de arquivos de dados dignas de nota (sem opção MIT/Apache)

| Crate | Versão | Expressão SPDX | Origem | Incluído por (workspace) | Observação |
|---|---|---|---|---|---|
| arrayref | 0.3.9 | BSD-2-Clause | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | somente os termos da BSD-2-Clause |
| foldhash | 0.2.0 | Zlib | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | somente os termos da Zlib |
| libloading | 0.8.9 | ISC | registry crates.io | armatura-cli, armatura-gpu | somente os termos da ISC |
| slotmap | 1.1.1 | Zlib | registry crates.io | armatura-cli, armatura-gpu | somente os termos da Zlib |
| tiny-keccak | 2.0.2 | CC0-1.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | somente os termos da CC0-1.0 |
| tiny-skia | 0.12.0 | BSD-3-Clause | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | somente os termos da BSD-3-Clause |
| tiny-skia-path | 0.12.0 | BSD-3-Clause | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | somente os termos da BSD-3-Clause |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | exige também a Unicode-3.0 (AND): termos de arquivos de dados além de MIT/Apache |
| zlib-rs | 0.6.8 | Zlib | registry crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | somente os termos da Zlib |

## C. MIT / Apache-2.0 (ou uma licença dupla que as inclua)

| Crate | Versão | Expressão SPDX | Origem | Incluído por (workspace) | Observação |
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


## Fontes dos algoritmos (ideias e constantes, não código copiado)
- **Ajuste de curvas de Bézier** (`autotrace.rs`, `lod.rs`): a estrutura do algoritmo segue P. J. Schneider, "An Algorithm for Automatically Fitting Digitized Curves", *Graphics Gems I* (A. Glassner, ed., Academic Press, 1990); o código foi escrito de forma independente. A distribuição de código do Graphics Gems declara, em seu README/nota de licença, que seu código pode ser usado em qualquer programa, produto ou biblioteca, comercial ou não, que o crédito é bem-vindo mas não obrigatório, e que é fornecido no estado em que se encontra, sem responsabilidade; só estão excluídos reivindicar o texto do código como próprio e revendê-lo. Não copiamos esse código; esta nota registra os termos permissivos caso um revisor pergunte (confira a redação no README do repositório do Graphics Gems).
- **Constantes de hash** (`noise.rs`): finalizador xorshift-multiply do `lowbias32` de Chris Wellons (domínio público, Hash Prospector, 2018); 0x9E3779B1 (2^32 / razão áurea, Knuth); 0x85EBCA6B (finalizador do MurmurHash3, Austin Appleby, domínio público).
- **Easing**: `Ease::Bounce` e `Ease::Back` são derivações próprias (CLEANROOM.md); as constantes BSD-3-Clause de Robert Penner não são mais usadas.
- Os demais algoritmos são listados por módulo, com seus artigos e livros-texto, em CLEANROOM.md.

## Modelo incluído (atribuição)
`armatura-core/models/face23_synth.bin` (um arquivo de pesos de 650 KB do detector de rostos para `facedet`) é (c) 2026 Alexandru-Ionuț Chiuță (Alexio), licenciado sob CC-BY-4.0 (https://creativecommons.org/licenses/by/4.0/; dê o crédito e indique as alterações). Foi treinado **somente com rostos gerados pelo gerador de rostos sintéticos do próprio projeto** (`facegen.rs`); não foram usadas fotografias, conjuntos de dados de terceiros nem pesos pré-treinados de terceiros.
## Compilações sem o crate MPL-2.0+ (smartstring)

A tabela acima é a união de todo o workspace com todos os recursos, em que a CLI padrão ativa `scripting` e o Cargo unifica os recursos. Verificado por pacote com `cargo tree -i smartstring -e normal,build`:

| Compilação | smartstring na árvore? |
|---|---|
| `-p armatura-core --no-default-features` | não |
| `-p armatura-cli --no-default-features` | não |
| `-p armatura-capi` | não |
| `-p armatura-wasm` | não |
| `-p armatura-cli` | sim |
