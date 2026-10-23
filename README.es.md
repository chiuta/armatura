<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio) -->
# ARMATURA

**Un motor de rigging y animación vectorial 2D en Rust: huesos, IK, skinning, deformadores, un runtime de máquinas de estados, una caja de herramientas de línea de comandos, un estudio en el navegador y bibliotecas integrables, todo en un único workspace.** Versión en rumano: [README.ro.md](README.ro.md).

## Resumen y filosofía

ARMATURA permite construir un personaje o una escena a partir de formas vectoriales e imágenes, dotarlo de rig con huesos y restricciones, animarlo con claves, clips y física, y exportarlo como GIF, vídeo, Lottie, glTF, un reproductor HTML autónomo o un runtime interactivo. Está formado por piezas pequeñas y deterministas: el mismo proyecto y la misma seed producen los mismos píxeles, cada herramienta lee y escribe un proyecto JSON documentado y cada comando funciona desde un terminal, de modo que la animación puede automatizarse con scripts, revisarse, compararse con diff y probarse como el código fuente.

Principios que dan forma al proyecto:

* **Implementación independiente.** Escrita a partir de especificaciones públicas, artículos científicos y matemáticas de manual. Se consultaron las listas de funciones de la competencia para determinar el *alcance*; no se copió código fuente de terceros y el proyecto no lee ni escribe ningún formato propietario. No es un proceso clean-room estricto (véase [CLEANROOM.md](docs/i18n/es/CLEANROOM.md), incluida su divulgación de la investigación) y no está afiliado a Moho ni a ningún otro producto comercial, ni deriva de ellos, ni es compatible con ellos. El código fue escrito en gran parte con un modelo de IA.
* **Estado honesto.** Cada función se describe junto con lo que *no* hace ([docs/LIMITATIONS.md](docs/i18n/es/LIMITATIONS.md), [ROADMAP.md](docs/i18n/es/ROADMAP.md) sección 9).
* **Seguro por defecto.** Sin panics ante la entrada, comprobaciones de valores finitos, límites de recursos, escrituras atómicas, rutas de recursos confinadas a la carpeta del proyecto ([docs/SECURITY.md](docs/i18n/es/SECURITY.md)).
* **Formatos abiertos primero.** OpenRaster, OCA, SVG, Lottie, glTF, APNG, WebM, EXR; ningún formato de archivo propietario por diseño.
* **Pocas dependencias**, con una política de licencias comprobada mediante pruebas y `cargo deny`.

## Estado

> **Versión preliminar, un solo entorno.** Todos los crates están en la versión `0.1.0` y no se ha publicado nada; "v0.5 / v0.6 / v0.7" son *niveles de funciones* del historial, no números de versión ([CHANGELOG.md](CHANGELOG.md)).
>
> | Verificado | No verificado |
> |---|---|
> | 1368 pruebas de `cargo test --workspace` y 19 doc-tests pasan (22 oct 2026, Linux x86-64); cuatro pruebas de humo del estudio con Chromium sin interfaz pasan (124, 60, 89 y 35 comprobaciones para los scripts ui, collab, overlays y suggest); la CLI, la fachada de Rust y la ABI de C (también con ASan/UBSan) se ejecutaron con los ejemplos de la documentación | **El módulo WebAssembly nunca se ha compilado ni ejecutado** (no se pudo instalar el target wasm32; [docs/WASM.md](docs/i18n/es/WASM.md)); el estudio nunca fue usado por una persona en producción, ni en un navegador distinto de Chromium; nunca se usó una GPU real (solo un adaptador por software), ni una webcam, ni una fotografía, ni un archivo de captura de movimiento; Windows y macOS sin probar; sin auditoría de seguridad, revisión por un abogado ni búsqueda de patentes |
>
> Lee [docs/LIMITATIONS.md](docs/i18n/es/LIMITATIONS.md) (73 limitaciones numeradas con su gravedad y alternativa) antes de depender de cualquier cosa.

## Funciones por área

Detalles y opciones exactas: [docs/CLI_REFERENCE.md](docs/i18n/es/CLI_REFERENCE.md); qué es parcial o falta: [docs/LIMITATIONS.md](docs/i18n/es/LIMITATIONS.md).

* **Modelo de dibujo.** Trazados vectoriales, rellenos, contornos (guiones, ancho variable), degradados lineales y radiales, máscaras, modos de fusión, efectos por capa (desenfoque, sombra, tinte, relleno), capas de imagen con deformación de malla, metaballs líquidas, booleanas de polígonos, partículas deterministas, cámara multiplano con tomas guardadas, profundidad de campo.
* **Rigging.** Huesos con cinemática directa; IK de dos huesos, suave, con estiramiento y FABRIK; límites, restricciones, muelles; skinning con herramientas de pesos; deformadores de lattice, spine, morph, wobble y cage; trazado de malla a partir del alfa; pivotes de deformación y glue; drivers (deslizadores, joysticks, cambio de vista); conjuntos de skin; auto-rig a partir de nombres de capa; recorte de ojos; contenedores de maquetación y regiones de desplazamiento.
* **Animación.** Claves con suavizado Bézier/escalón/muelle/rebote, expresiones y canales dirigidos, capas de conmutación, takes y una pila de clips por capas, conversión a bucle y a sitio, bloqueo de pies, imanes, seguimiento de trayectoria, generadores de reposo y de solapamiento, física de cadenas/pelo/tela, lip-sync a partir de audio o de una transcripción, movimiento de cabeza adaptado al habla, retargeting de BVH, entrada en vivo (MIDI, ratón, gamepad, puntos de referencia de webcam que tú proporcionas), pistas de audio con recorte, fundido y mezcla.
* **Runtime.** Máquinas de estados con blend trees 1-D, enlace de datos, física de desplazamiento, nivel de detalle con cota de error, un sandbox de scripting Rhai, un servidor de agente (MCP) sobre operaciones de parche.
* **Salida.** SVG, PNG, secuencias PNG y hojas de sprites, GIF, APNG, MP4/WebM (mediante ffmpeg), reproductor HTML autónomo, Lottie y dotLottie, glTF 2.0 binario, OpenEXR/PNG de 16 bits, OCA; exportación con tamaño objetivo (`export-fit`). Importación: SVG, OpenRaster, Lottie, BVH, secuencias PNG/GIF/APNG.
* **Herramientas de calidad.** `check`, `lint`, `diff` semántico, fuzzing de poses con mapas de calor, CI de animación con diff perceptual y fotogramas de referencia ([docs/CI.md](docs/i18n/es/CI.md)), control de versiones con merge semántico (`vcs`), comprobaciones de seguridad para destellos y movimiento.
* **Estudio.** Editor en el navegador servido por la CLI: gizmos en el lienzo, línea de tiempo, dope sheet y editor de curvas, overlays, paleta de comandos y atajos, deshacer/rehacer, edición multiusuario en tiempo real con token opcional y registro persistente ([docs/STUDIO.md](docs/i18n/es/STUDIO.md)).
* **Bibliotecas.** Fachada estable de Rust ([docs/API.md](docs/i18n/es/API.md)), ABI de C ([docs/C_ABI.md](docs/i18n/es/C_ABI.md)), módulo WebAssembly y envoltorio de JavaScript (sin compilar; [docs/WASM.md](docs/i18n/es/WASM.md)), renderizador wgpu opcional.

## Inicio rápido

Requisitos: una toolchain estable reciente de Rust. `ffmpeg` solo se necesita para exportar MP4/WebM y para audio mp3/ogg; nunca se incluye en el paquete. Todos los comandos siguientes se ejecutaron para este README (compilación debug, Linux). Las compilaciones release (`cargo build --release`) son mucho más rápidas para renderizar y se usan igual con `target/release`.

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

`voice.wav` es cualquier archivo WAV mono o estéreo que proporciones. El estudio escucha solo en `127.0.0.1` y rechaza las peticiones dirigidas a otros nombres de host; para compartirlo, lee antes [docs/SECURITY.md](docs/i18n/es/SECURITY.md).

Usa las bibliotecas:

```sh
cargo build -p armatura-capi                      # target/debug/libarmatura_capi.so and .a
gcc -std=c99 -o smoke armatura-capi/tests/smoke.c -Ltarget/debug -larmatura_capi
LD_LIBRARY_PATH=target/debug ./smoke demo.json    # abi 1, 48 frames, 320x240, rc=0, checksum=...
```

La fachada de Rust es `armatura_core::prelude` ([docs/API.md](docs/i18n/es/API.md)); hay más ejemplos en C y Python en [docs/C_ABI.md](docs/i18n/es/C_ABI.md).

Pruebas (unos 14 minutos en una compilación debug en una máquina compartida; véase [docs/DEVELOPMENT.md](docs/i18n/es/DEVELOPMENT.md)):

```sh
cargo test --workspace
cargo test --doc -p armatura-core
cargo test -p armatura-cli --test spdx_audit
```

## Estructura del repositorio

| Ruta | Contenido | Licencia |
|---|---|---|
| `armatura-core/` | motor, formatos, herramientas, fachada estable `api` | MPL-2.0 |
| `armatura-cli/` | el binario `armatura`, estudio y servidor de colaboración, pruebas de extremo a extremo y de humo | AGPL-3.0-or-later |
| `armatura-capi/` | ABI de C (`armatura.h`, biblioteca compartida y estática) | MPL-2.0 |
| `armatura-wasm/` | ABI de WebAssembly, envoltorio de JavaScript, página de demostración (sin compilar) | MPL-2.0 |
| `armatura-gpu/` | renderizador wgpu opcional (no es un miembro por defecto) | MPL-2.0 |
| `docs/` | documentación (CC-BY-4.0) y archivos generados de terceros | CC-BY-4.0 |
| `scripts/` | `e2e.sh`, `licence-audit.sh` | MPL-2.0 |
| `LICENSES/`, `REUSE.toml`, `deny.toml` | textos de licencia, metadatos de licencia, política de dependencias | - |
| `ROADMAP.md`, `FEATURES.md`, `CLEANROOM.md`, `LICENSING.md`, `TRADEMARK.md`, `CONTRIBUTING.md`, `CHANGELOG.md` | documentos del proyecto | CC-BY-4.0 |

## Mapa de la documentación

Empieza en [docs/INDEX.md](docs/i18n/es/INDEX.md) (todos los documentos, destinatarios, rutas de lectura).

| Página | Para |
|---|---|
| [docs/GETTING_STARTED.md](docs/i18n/es/GETTING_STARTED.md) | primer proyecto, renderizado, exportación |
| [docs/CLI_REFERENCE.md](docs/i18n/es/CLI_REFERENCE.md) | todos los comandos y opciones |
| [docs/STUDIO.md](docs/i18n/es/STUDIO.md) | el editor en el navegador |
| [docs/RIGGING_GUIDE.md](docs/i18n/es/RIGGING_GUIDE.md) | construcción de rigs |
| [docs/ANIMATION_GUIDE.md](docs/i18n/es/ANIMATION_GUIDE.md) | animación, clips, física, audio |
| [docs/PROJECT_FORMAT.md](docs/i18n/es/PROJECT_FORMAT.md) | el formato de proyecto JSON |
| [docs/EXPORT_FORMATS.md](docs/i18n/es/EXPORT_FORMATS.md) | qué conserva y qué descarta cada formato |
| [docs/ARCHITECTURE.md](docs/i18n/es/ARCHITECTURE.md) | diseño y módulos |
| [docs/GLOSSARY.md](docs/i18n/es/GLOSSARY.md) | términos |
| [docs/FAQ.md](docs/i18n/es/FAQ.md) | preguntas frecuentes |
| [docs/API.md](docs/i18n/es/API.md) | fachada de Rust 1.4 |
| [docs/C_ABI.md](docs/i18n/es/C_ABI.md) | interfaz de C |
| [docs/WASM.md](docs/i18n/es/WASM.md) | WebAssembly (nunca compilado) |
| [docs/CI.md](docs/i18n/es/CI.md) | CI de animación |
| [docs/DEVELOPMENT.md](docs/i18n/es/DEVELOPMENT.md) | compilación, pruebas, convenciones, publicación |
| [docs/SECURITY.md](docs/i18n/es/SECURITY.md) | modelo de amenazas y refuerzo de la seguridad |
| [docs/LIMITATIONS.md](docs/i18n/es/LIMITATIONS.md) | limitaciones conocidas |
| [docs/PATENT_NOTES.md](docs/i18n/es/PATENT_NOTES.md), [docs/THIRD_PARTY.md](docs/i18n/es/THIRD_PARTY.md), [docs/THIRD_PARTY_NOTICES.md](docs/THIRD_PARTY_NOTICES.md) | patentes, licencias de las dependencias |
| [CHANGELOG.md](CHANGELOG.md), [ROADMAP.md](docs/i18n/es/ROADMAP.md), [FEATURES.md](docs/i18n/es/FEATURES.md) | historial, plan y estado detallado, matriz de funciones |
| [CLEANROOM.md](docs/i18n/es/CLEANROOM.md), [LICENSING.md](docs/i18n/es/LICENSING.md), [TRADEMARK.md](docs/i18n/es/TRADEMARK.md), [CONTRIBUTING.md](docs/i18n/es/CONTRIBUTING.md) | procedencia, licencias, política del nombre, normas de contribución |

## Resumen de licencias

* **MPL-2.0** para las bibliotecas `armatura-core`, `armatura-capi`, `armatura-wasm`, `armatura-gpu` y para `scripts/`: puedes enlazarlas en tu propio programa, abierto o cerrado; los cambios en esos archivos deben publicarse bajo la MPL.
* **AGPL-3.0-or-later** para `armatura-cli` (la línea de comandos y el estudio). Si lo modificas y permites que otros interactúen con él a través de una red, la sección 13 te obliga a ofrecerles tu código fuente: `armatura studio --source-url URL` publica dónde se encuentra; `/source` y `armatura --version` incluyen la oferta.
* **CC-BY-4.0** para la documentación y los recursos sintéticos (incluido el modelo sintético de detección de rostros).
* **Tus proyectos y renders siguen siendo tuyos.**
* El nombre y el logotipo no están cubiertos por estas licencias ([TRADEMARK.md](docs/i18n/es/TRADEMARK.md)). El estado de las patentes no se ha evaluado ([docs/PATENT_NOTES.md](docs/i18n/es/PATENT_NOTES.md)). Una licencia comercial o dual para el código aportado no es posible sin un acuerdo de licencia de colaborador, y no existe ninguno.
* Crates de terceros: 160 en la última auditoría, todos permisivos salvo un crate de copyleft débil tras la función opcional `scripting` ([docs/THIRD_PARTY.md](docs/i18n/es/THIRD_PARTY.md)). ffmpeg nunca se incluye en el paquete.

Véanse [LICENSE](LICENSE), [LICENSES/](LICENSES/) y [LICENSING.md](docs/i18n/es/LICENSING.md). Nada de esto es asesoramiento jurídico; sigue pendiente la revisión por un abogado.

## Contribuir

Las contribuciones son bienvenidas bajo la licencia de la parte que modifiquen. Lee [CONTRIBUTING.md](docs/i18n/es/CONTRIBUTING.md) (cada commit necesita una línea `Signed-off-by`, DCO 1.1), [CLEANROOM.md](docs/i18n/es/CLEANROOM.md) (implementa solo a partir de fuentes públicas; no copies de código propietario ni con una licencia incompatible) y [docs/DEVELOPMENT.md](docs/i18n/es/DEVELOPMENT.md) (convenciones, lista de comprobación para un módulo nuevo, pruebas). Buenas primeras contribuciones: una corrección de [docs/LIMITATIONS.md](docs/i18n/es/LIMITATIONS.md), una ejecución real de la lista de comprobación de WebAssembly, pruebas en Windows o macOS, un benchmark en una GPU real.

Problemas de seguridad: no abras una incidencia pública; sigue [docs/SECURITY.md](docs/i18n/es/SECURITY.md) sección 10.

## Apoyo

ARMATURA está desarrollado por Alexandru-Ionuț Chiuță (Alexio). Si te resulta útil, puedes apoyar el trabajo:

* Buy Me a Coffee: <https://buymeacoffee.com/echo.of.the.strings>
* Patreon: <https://patreon.com/c/alexio_tf>

Contacto del mantenedor (también lo imprime `armatura --version`): alexio@trom.tf.

## Agradecimientos

* Los autores de los artículos públicos y los libros de texto que siguen los algoritmos, citados por módulo en [CLEANROOM.md](docs/i18n/es/CLEANROOM.md) (matemáticas de Bézier y splines, cinemática inversa, skinning por mezcla lineal, coordenadas de valor medio, ajuste de curvas y otros).
* Las especificaciones abiertas que ARMATURA lee y escribe: SVG, OpenRaster, OCA, Lottie/dotLottie, glTF, APNG, WebM, OpenEXR, BVH, PNG, GIF.
* Los crates de Rust en los que se apoya, enumerados con sus licencias en [docs/THIRD_PARTY.md](docs/i18n/es/THIRD_PARTY.md), en particular `tiny-skia`, `serde`, `png`, `zip`, `rhai` y `wgpu`.
* Todas las personas que apoyan el proyecto a través de los enlaces anteriores.
