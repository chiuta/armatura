# Licencias de terceros (generado)

*Generado por `scripts/licence-audit.sh` a partir de `cargo metadata --all-features` (todos los destinos, dependencias normales y de compilación; se excluyen las dependencias de desarrollo). No lo edites a mano. Esto no es asesoramiento jurídico; las expresiones SPDX son las que cada crate declara en su `Cargo.toml` y no se verificaron de forma independiente con los archivos fuente. Los propios crates del workspace son MPL-2.0 (core, capi, wasm, gpu) y AGPL-3.0-or-later (armatura-cli); véase LICENSE.*

**362 crates de terceros** (nombre+versión distintos). Grupo A (requiere revisión): 6; grupo B (licencias permisivas, de dominio público o de datos destacables): 27; grupo C (estilo MIT / Apache-2.0): 329.

## A. Requiere revisión: un término de copyleft sin alternativa totalmente permisiva

| Crate | Versión | Expresión SPDX | Origen | Incorporado por (workspace) | Nota |
|---|---|---|---|---|---|
| cssparser | 0.37.0 | MPL-2.0 | registro crates.io | armatura-gui | término de copyleft o no reconocido; sin alternativa totalmente permisiva en la expresión; MPL = copyleft a nivel de archivo (débil) |
| cssparser-macros | 0.7.1 | MPL-2.0 | registro crates.io | armatura-gui | término de copyleft o no reconocido; sin alternativa totalmente permisiva en la expresión; MPL = copyleft a nivel de archivo (débil) |
| dtoa-short | 0.3.5 | MPL-2.0 | registro crates.io | armatura-gui | término de copyleft o no reconocido; sin alternativa totalmente permisiva en la expresión; MPL = copyleft a nivel de archivo (débil) |
| option-ext | 0.2.0 | MPL-2.0 | registro crates.io | armatura-gui | término de copyleft o no reconocido; sin alternativa totalmente permisiva en la expresión; MPL = copyleft a nivel de archivo (débil) |
| selectors | 0.38.0 | MPL-2.0 | registro crates.io | armatura-gui | término de copyleft o no reconocido; sin alternativa totalmente permisiva en la expresión; MPL = copyleft a nivel de archivo (débil) |
| smartstring | 1.0.1 | MPL-2.0+ | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | término de copyleft o no reconocido; sin alternativa totalmente permisiva en la expresión; MPL = copyleft a nivel de archivo (débil) |

## B. Licencias permisivas, de dominio público o de archivos de datos destacables (sin opción MIT/Apache)

| Crate | Versión | Expresión SPDX | Origen | Incorporado por (workspace) | Nota |
|---|---|---|---|---|---|
| arrayref | 0.3.9 | BSD-2-Clause | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | solo términos BSD-2-Clause |
| foldhash | 0.2.0 | Zlib | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm | solo términos Zlib |
| icu_collections | 2.4.0 | Unicode-3.0 | registro crates.io | armatura-gui | solo términos Unicode-3.0 |
| icu_locale_core | 2.4.0 | Unicode-3.0 | registro crates.io | armatura-gui | solo términos Unicode-3.0 |
| icu_normalizer | 2.4.0 | Unicode-3.0 | registro crates.io | armatura-gui | solo términos Unicode-3.0 |
| icu_normalizer_data | 2.4.0 | Unicode-3.0 | registro crates.io | armatura-gui | solo términos Unicode-3.0 |
| icu_properties | 2.4.0 | Unicode-3.0 | registro crates.io | armatura-gui | solo términos Unicode-3.0 |
| icu_properties_data | 2.4.0 | Unicode-3.0 | registro crates.io | armatura-gui | solo términos Unicode-3.0 |
| icu_provider | 2.4.0 | Unicode-3.0 | registro crates.io | armatura-gui | solo términos Unicode-3.0 |
| libloading | 0.8.9 | ISC | registro crates.io | armatura-cli, armatura-gpu | solo términos ISC |
| litemap | 0.8.3 | Unicode-3.0 | registro crates.io | armatura-gui | solo términos Unicode-3.0 |
| potential_utf | 0.1.6 | Unicode-3.0 | registro crates.io | armatura-gui | solo términos Unicode-3.0 |
| slotmap | 1.1.1 | Zlib | registro crates.io | armatura-cli, armatura-gpu | solo términos Zlib |
| tiny-keccak | 2.0.2 | CC0-1.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | solo términos CC0-1.0 |
| tiny-skia | 0.12.0 | BSD-3-Clause | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | solo términos BSD-3-Clause |
| tiny-skia-path | 0.12.0 | BSD-3-Clause | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | solo términos BSD-3-Clause |
| tinystr | 0.8.5 | Unicode-3.0 | registro crates.io | armatura-gui | solo términos Unicode-3.0 |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm | requiere además Unicode-3.0 (AND): términos de archivos de datos adicionales a MIT/Apache |
| writeable | 0.6.4 | Unicode-3.0 | registro crates.io | armatura-gui | solo términos Unicode-3.0 |
| yoke | 0.8.4 | Unicode-3.0 | registro crates.io | armatura-gui | solo términos Unicode-3.0 |
| yoke-derive | 0.8.4 | Unicode-3.0 | registro crates.io | armatura-gui | solo términos Unicode-3.0 |
| zerofrom | 0.1.8 | Unicode-3.0 | registro crates.io | armatura-gui | solo términos Unicode-3.0 |
| zerofrom-derive | 0.1.8 | Unicode-3.0 | registro crates.io | armatura-gui | solo términos Unicode-3.0 |
| zerotrie | 0.2.5 | Unicode-3.0 | registro crates.io | armatura-gui | solo términos Unicode-3.0 |
| zerovec | 0.11.9 | Unicode-3.0 | registro crates.io | armatura-gui | solo términos Unicode-3.0 |
| zerovec-derive | 0.11.7 | Unicode-3.0 | registro crates.io | armatura-gui | solo términos Unicode-3.0 |
| zlib-rs | 0.6.8 | Zlib | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm | solo términos Zlib |

## C. MIT / Apache-2.0 (o una doble licencia que las incluya)

| Crate | Versión | Expresión SPDX | Origen | Incorporado por (workspace) | Nota |
|---|---|---|---|---|---|
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| ahash | 0.8.12 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| allocator-api2 | 0.2.21 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| android_system_properties | 0.1.6 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| ash | 0.38.0+1.3.281 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| atk | 0.18.2 | MIT | registro crates.io | armatura-gui |  |
| atk-sys | 0.18.2 | MIT | registro crates.io | armatura-gui |  |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| base64 | 0.23.1 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| bit-set | 0.10.0 | Apache-2.0 OR MIT | registro crates.io | armatura-cli, armatura-gpu |  |
| bit-set | 0.8.0 | Apache-2.0 OR MIT | registro crates.io | armatura-gui |  |
| bit-vec | 0.8.0 | Apache-2.0 OR MIT | registro crates.io | armatura-gui |  |
| bit-vec | 0.9.1 | Apache-2.0 OR MIT | registro crates.io | armatura-cli, armatura-gpu |  |
| bitflags | 1.3.2 | MIT/Apache-2.0 | registro crates.io | armatura-gui |  |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| block2 | 0.6.2 | MIT | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| bytemuck_derive | 1.12.1 | Zlib OR Apache-2.0 OR MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| bytes | 1.12.1 | MIT | registro crates.io | armatura-gui |  |
| cairo-rs | 0.18.5 | MIT | registro crates.io | armatura-gui |  |
| cairo-sys-rs | 0.18.2 | MIT | registro crates.io | armatura-gui |  |
| cc | 1.10.0 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| cesu8 | 1.1.0 | Apache-2.0/MIT | registro crates.io | armatura-gui |  |
| cfg-expr | 0.15.8 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| cfg_aliases | 0.2.2 | MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| codespan-reporting | 0.13.1 | Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| color_quant | 1.1.0 | MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| combine | 4.6.9 | MIT | registro crates.io | armatura-gui |  |
| const-random | 0.1.18 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| const-random-macro | 0.1.16 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| cookie | 0.18.2 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| core-foundation | 0.10.1 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| core-graphics | 0.25.0 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| core-graphics-types | 0.2.0 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| crc32fast | 1.5.2 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| crossbeam-channel | 0.5.17 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| crunchy | 0.2.4 | MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| dbus | 0.9.12 | Apache-2.0/MIT | registro crates.io | armatura-gui |  |
| deranged | 0.5.9 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| derive_more | 2.1.1 | MIT | registro crates.io | armatura-gui |  |
| derive_more-impl | 2.1.1 | MIT | registro crates.io | armatura-gui |  |
| digest | 0.10.7 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| dirs | 7.0.0 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| dirs-sys | 0.5.0 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| dispatch2 | 0.3.1 | Zlib OR Apache-2.0 OR MIT | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| dlib | 0.5.3 | MIT | registro crates.io | armatura-cli, armatura-gpu |  |
| dlopen2 | 0.8.2 | MIT | registro crates.io | armatura-gui |  |
| dlopen2_derive | 0.4.3 | MIT | registro crates.io | armatura-gui |  |
| document-features | 0.2.12 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| dom_query | 0.28.0 | MIT | registro crates.io | armatura-gui |  |
| dpi | 0.1.2 | Apache-2.0 AND MIT | registro crates.io | armatura-gui |  |
| dtoa | 1.0.11 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| dunce | 1.0.5 | CC0-1.0 OR MIT-0 OR Apache-2.0 | registro crates.io | armatura-gui |  |
| equivalent | 1.0.3 | Apache-2.0 OR MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| fastrand | 2.5.0 | Apache-2.0 OR MIT | registro crates.io | armatura-gui |  |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| field-offset | 0.3.6 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| find-msvc-tools | 0.1.15 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| flate2 | 1.1.10 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| foreign-types | 0.5.0 | MIT/Apache-2.0 | registro crates.io | armatura-gui |  |
| foreign-types-macros | 0.2.4 | MIT/Apache-2.0 | registro crates.io | armatura-gui |  |
| foreign-types-shared | 0.3.1 | MIT/Apache-2.0 | registro crates.io | armatura-gui |  |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| futures-executor | 0.3.34 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| futures-io | 0.3.34 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| gdk | 0.18.2 | MIT | registro crates.io | armatura-gui |  |
| gdk-pixbuf | 0.18.5 | MIT | registro crates.io | armatura-gui |  |
| gdk-pixbuf-sys | 0.18.0 | MIT | registro crates.io | armatura-gui |  |
| gdk-sys | 0.18.2 | MIT | registro crates.io | armatura-gui |  |
| gdkwayland-sys | 0.18.2 | MIT | registro crates.io | armatura-gui |  |
| gdkx11 | 0.18.2 | MIT | registro crates.io | armatura-gui |  |
| gdkx11-sys | 0.18.2 | MIT | registro crates.io | armatura-gui |  |
| generic-array | 0.14.7 | MIT | registro crates.io | armatura-gui |  |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| gif | 0.14.2 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| gio | 0.18.4 | MIT | registro crates.io | armatura-gui |  |
| gio-sys | 0.18.1 | MIT | registro crates.io | armatura-gui |  |
| gl_generator | 0.14.0 | Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| glib | 0.18.5 | MIT | registro crates.io | armatura-gui |  |
| glib-macros | 0.18.5 | MIT | registro crates.io | armatura-gui |  |
| glib-sys | 0.18.1 | MIT | registro crates.io | armatura-gui |  |
| glow | 0.17.0 | MIT OR Apache-2.0 OR Zlib | registro crates.io | armatura-cli, armatura-gpu |  |
| glutin_wgl_sys | 0.6.1 | Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| gobject-sys | 0.18.0 | MIT | registro crates.io | armatura-gui |  |
| gpu-allocator | 0.28.0 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| gtk | 0.18.2 | MIT | registro crates.io | armatura-gui |  |
| gtk-sys | 0.18.2 | MIT | registro crates.io | armatura-gui |  |
| gtk3-macros | 0.18.2 | MIT | registro crates.io | armatura-gui |  |
| half | 2.7.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| heck | 0.4.1 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| heck | 0.5.0 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| hound | 3.5.1 | Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| html5ever | 0.39.0 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| http | 1.5.0 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| i_float | 5.0.0 | MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| i_key_sort | 0.12.0 | MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| i_overlay | 9.1.0 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| i_shape | 5.0.0 | MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| i_tree | 0.20.0 | MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| idna | 1.1.0 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT | registro crates.io | armatura-gui |  |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| itoa | 1.0.18 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| javascriptcore-rs | 1.1.2 | MIT | registro crates.io | armatura-gui |  |
| javascriptcore-rs-sys | 1.1.1 | MIT | registro crates.io | armatura-gui |  |
| jni | 0.21.1 | MIT/Apache-2.0 | registro crates.io | armatura-gui |  |
| jni-sys | 0.3.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| jni-sys | 0.4.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| jni-sys-macros | 0.4.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| jpeg-decoder | 0.3.2 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| js-sys | 0.3.106 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| khronos-egl | 6.0.0 | MIT/Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| khronos_api | 3.1.0 | Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| libc | 0.2.191 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| libdbus-sys | 0.2.7 | Apache-2.0/MIT | registro crates.io | armatura-gui |  |
| libm | 0.2.16 | MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| libredox | 0.1.25 | MIT | registro crates.io | armatura-gui |  |
| litrs | 1.0.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| log | 0.4.34 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| markup5ever | 0.39.0 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| memchr | 2.8.3 | Unlicense OR MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| memoffset | 0.9.1 | MIT | registro crates.io | armatura-gui |  |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| naga | 30.0.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| naga-types | 30.0.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| ndk | 0.9.0 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| ndk-context | 0.1.1 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| ndk-sys | 0.6.0+11769913 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| new_debug_unreachable | 1.0.6 | MIT | registro crates.io | armatura-gui |  |
| nix | 0.31.3 | MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| num-conv | 0.2.2 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| num_enum | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| num_enum_derive | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| objc2 | 0.6.5 | MIT | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| objc2-app-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registro crates.io | armatura-gui |  |
| objc2-cloud-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registro crates.io | armatura-gui |  |
| objc2-core-data | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registro crates.io | armatura-gui |  |
| objc2-core-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| objc2-core-graphics | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| objc2-core-image | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registro crates.io | armatura-gui |  |
| objc2-core-location | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registro crates.io | armatura-gui |  |
| objc2-core-text | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registro crates.io | armatura-gui |  |
| objc2-encode | 4.1.0 | MIT | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| objc2-exception-helper | 0.1.1 | Zlib OR Apache-2.0 OR MIT | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| objc2-foundation | 0.3.2 | MIT | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| objc2-io-surface | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| objc2-metal | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| objc2-quartz-core | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| objc2-ui-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registro crates.io | armatura-gui |  |
| objc2-user-notifications | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registro crates.io | armatura-gui |  |
| objc2-web-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registro crates.io | armatura-gui |  |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| ordered-float | 5.5.0 | MIT | registro crates.io | armatura-cli, armatura-gpu |  |
| pango | 0.18.3 | MIT | registro crates.io | armatura-gui |  |
| pango-sys | 0.18.0 | MIT | registro crates.io | armatura-gui |  |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| phf | 0.13.1 | MIT | registro crates.io | armatura-gui |  |
| phf_codegen | 0.13.1 | MIT | registro crates.io | armatura-gui |  |
| phf_generator | 0.13.1 | MIT | registro crates.io | armatura-gui |  |
| phf_macros | 0.13.1 | MIT | registro crates.io | armatura-gui |  |
| phf_shared | 0.13.1 | MIT | registro crates.io | armatura-gui |  |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| png | 0.18.1 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| pollster | 1.0.1 | Apache-2.0/MIT | registro crates.io | armatura-cli, armatura-gpu |  |
| portable-atomic | 1.15.0 | Apache-2.0 OR MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| portable-atomic-util | 0.2.8 | Apache-2.0 OR MIT | registro crates.io | armatura-cli, armatura-gpu |  |
| powerfmt | 0.2.1 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| precomputed-hash | 0.1.1 | MIT | registro crates.io | armatura-gui |  |
| presser | 0.3.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| proc-macro-crate | 1.3.1 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| proc-macro-crate | 2.0.2 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| proc-macro-crate | 3.5.0 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| proc-macro-error | 1.0.4 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| proc-macro-error-attr | 1.0.4 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| profiling | 1.0.18 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| quote | 1.0.47 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| range-alloc | 0.1.5 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| raw-window-metal | 1.1.0 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| redox_syscall | 0.5.18 | MIT | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| redox_users | 0.5.3 | MIT | registro crates.io | armatura-gui |  |
| renderdoc-sys | 1.1.0 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| rhai | 1.26.1 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| rhai_codegen | 3.2.0 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| rustc-hash | 1.1.0 | Apache-2.0/MIT | registro crates.io | armatura-cli, armatura-gpu |  |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT | registro crates.io | armatura-gui |  |
| rustc_version | 0.4.1 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| rustversion | 1.0.23 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| same-file | 1.0.6 | Unlicense/MIT | registro crates.io | armatura-gui |  |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| semver | 1.0.28 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| serde | 1.0.229 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| serde_spanned | 0.6.9 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| servo_arc | 0.4.3 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| shlex | 2.0.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| simd-adler32 | 0.3.10 | MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| siphasher | 1.0.4 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| slab | 0.4.12 | MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| smallvec | 1.16.3 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| soup3 | 0.5.0 | MIT | registro crates.io | armatura-gui |  |
| soup3-sys | 0.5.0 | MIT | registro crates.io | armatura-gui |  |
| spirv | 0.4.0+sdk-1.4.341.0 | Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| strict-num | 0.1.1 | MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| string_cache | 0.9.0 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| string_cache_codegen | 0.6.1 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| syn | 1.0.109 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| syn | 2.0.119 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| syn | 3.0.9 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| synstructure | 0.14.0 | MIT | registro crates.io | armatura-gui |  |
| system-deps | 6.2.2 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| tao | 0.37.2 | Apache-2.0 | registro crates.io | armatura-gui |  |
| tao-macros | 0.1.4 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| target-lexicon | 0.12.16 | Apache-2.0 WITH LLVM-exception | registro crates.io | armatura-gui |  |
| tendril | 0.5.1 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| thin-vec | 0.2.21 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| thiserror | 1.0.69 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| thiserror-impl | 1.0.69 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| time | 0.3.55 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| time-core | 0.1.9 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| time-macros | 0.2.32 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| tokio | 1.54.0 | MIT | registro crates.io | armatura-cli, armatura-gpu |  |
| toml | 0.8.2 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| toml_datetime | 0.6.3 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| toml_datetime | 1.1.2+spec-1.1.0 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| toml_edit | 0.19.15 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| toml_edit | 0.20.2 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| toml_edit | 0.25.17+spec-1.1.0 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| toml_parser | 1.1.5+spec-1.1.0 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| typed-path | 0.12.4 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| typenum | 1.20.1 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| unicode-width | 0.2.2 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| url | 2.5.8 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT | registro crates.io | armatura-gui |  |
| version-compare | 0.2.1 | MIT | registro crates.io | armatura-gui |  |
| version_check | 0.9.5 | MIT/Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-gui, armatura-wasm |  |
| walkdir | 2.5.0 | Unlicense/MIT | registro crates.io | armatura-gui |  |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| wasm-bindgen | 0.2.129 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| wasm-bindgen-futures | 0.4.79 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| wasm-bindgen-macro | 0.2.129 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| wasm-bindgen-macro-support | 0.2.129 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| wasm-bindgen-shared | 0.2.129 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| wayland-sys | 0.31.11 | MIT | registro crates.io | armatura-cli, armatura-gpu |  |
| web-sys | 0.3.106 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| web-time | 1.1.0 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| web_atoms | 0.2.6 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| webkit2gtk | 2.0.2 | MIT | registro crates.io | armatura-gui |  |
| webkit2gtk-sys | 2.0.2 | MIT | registro crates.io | armatura-gui |  |
| webview2-com | 0.39.1 | MIT | registro crates.io | armatura-gui |  |
| webview2-com-macros | 0.8.1 | MIT | registro crates.io | armatura-gui |  |
| webview2-com-sys | 0.39.1 | MIT | registro crates.io | armatura-gui |  |
| weezl | 0.1.12 | MIT OR Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| wgpu | 30.0.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| wgpu-core | 30.0.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| wgpu-core-deps-apple | 30.0.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| wgpu-core-deps-emscripten | 30.0.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| wgpu-core-deps-windows-linux-android | 30.0.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| wgpu-hal | 30.0.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| wgpu-naga-bridge | 30.0.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| wgpu-types | 30.0.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu |  |
| winapi | 0.3.9 | MIT/Apache-2.0 | registro crates.io | armatura-gui |  |
| winapi-i686-pc-windows-gnu | 0.4.0 | MIT/Apache-2.0 | registro crates.io | armatura-gui |  |
| winapi-util | 0.1.11 | Unlicense OR MIT | registro crates.io | armatura-gui |  |
| winapi-x86_64-pc-windows-gnu | 0.4.0 | MIT/Apache-2.0 | registro crates.io | armatura-gui |  |
| windows | 0.62.2 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| windows-collections | 0.3.2 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| windows-core | 0.62.2 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| windows-future | 0.3.2 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| windows-numerics | 0.3.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| windows-result | 0.4.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| windows-sys | 0.45.0 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| windows-targets | 0.42.2 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| windows-threading | 0.2.1 | MIT OR Apache-2.0 | registro crates.io | armatura-cli, armatura-gpu, armatura-gui |  |
| windows-version | 0.1.7 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| windows_aarch64_gnullvm | 0.42.2 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| windows_aarch64_msvc | 0.42.2 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| windows_i686_gnu | 0.42.2 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| windows_i686_msvc | 0.42.2 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| windows_x86_64_gnu | 0.42.2 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| windows_x86_64_gnullvm | 0.42.2 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| windows_x86_64_msvc | 0.42.2 | MIT OR Apache-2.0 | registro crates.io | armatura-gui |  |
| winnow | 0.5.40 | MIT | registro crates.io | armatura-gui |  |
| winnow | 1.0.4 | MIT | registro crates.io | armatura-gui |  |
| wry | 0.57.1 | Apache-2.0 OR MIT | registro crates.io | armatura-gui |  |
| x11 | 2.21.0 | MIT | registro crates.io | armatura-gui |  |
| x11-dl | 2.21.0 | MIT | registro crates.io | armatura-gui |  |
| xml-rs | 0.8.29 | MIT | registro crates.io | armatura-cli, armatura-gpu |  |
| zerocopy | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| zerocopy-derive | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| zip | 9.0.5 | MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| zmij | 1.0.23 | MIT | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |
| zopfli | 0.8.4 | Apache-2.0 | registro crates.io | armatura-capi, armatura-cli, armatura-core, armatura-gpu, armatura-wasm |  |


## Fuentes de algoritmos (ideas y constantes, no código copiado)
- **Ajuste de curvas de Bézier** (`autotrace.rs`, `lod.rs`): la estructura del algoritmo sigue a P. J. Schneider, "An Algorithm for Automatically Fitting Digitized Curves", *Graphics Gems I* (A. Glassner, ed., Academic Press, 1990); el código se escribió de forma independiente. La distribución de código de Graphics Gems indica, en su nota README/de licencia, que su código puede usarse en cualquier programa, producto o biblioteca, comercial o no, que se agradece la atribución pero no es obligatoria, y que se entrega tal cual y sin responsabilidad; solo se excluye hacer pasar el texto del código por propio y revenderlo. No copiamos ese código; esta nota recoge los términos permisivos por si un revisor lo pregunta (verifica la redacción con el README del repositorio de Graphics Gems).
- **Constantes de hash** (`noise.rs`): finalizador xorshift-multiply de `lowbias32` de Chris Wellons (dominio público, Hash Prospector, 2018); 0x9E3779B1 (2^32 / razón áurea, Knuth); 0x85EBCA6B (finalizador de MurmurHash3, Austin Appleby, dominio público).
- **Easing**: `Ease::Bounce` y `Ease::Back` son derivaciones propias (CLEANROOM.md); las constantes BSD-3-Clause de Robert Penner ya no se usan.
- Los demás algoritmos se enumeran por módulo, con sus artículos y libros de texto, en CLEANROOM.md.

## Modelo incluido (atribución)
`armatura-core/models/face23_synth.bin` (un archivo de pesos de 650 KB del detector de rostros para `facedet`) es (c) 2026 Alexandru-Ionuț Chiuță (Alexio), con licencia CC-BY-4.0 (https://creativecommons.org/licenses/by/4.0/; da el crédito e indica los cambios). Se entrenó **únicamente con rostros generados por el generador de rostros sintéticos de este mismo proyecto** (`facegen.rs`); no se usaron fotografías, ni conjuntos de datos de terceros, ni pesos preentrenados de terceros.
## Compilaciones sin el crate MPL-2.0+ (smartstring)

La tabla anterior es la unión de todo el workspace con todas las características, donde la CLI por defecto activa `scripting` y Cargo unifica las características. Comprobado por paquete con `cargo tree -i smartstring -e normal,build`:

| Compilación | ¿smartstring en el árbol? |
|---|---|
| `-p armatura-core --no-default-features` | no |
| `-p armatura-cli --no-default-features` | no |
| `-p armatura-capi` | no |
| `-p armatura-wasm` | no |
| `-p armatura-cli` | sí |
