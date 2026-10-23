# ARMATURA para WebAssembly

*Versión de ABI 1 (`arm_abi_version()`). Crate `armatura-wasm`, envoltorio de JavaScript `armatura-wasm/js/armatura.js`, página de demostración `armatura-wasm/index.html`, script de compilación `armatura-wasm/build-wasm.sh`. Destinatarios: desarrolladores web que quieren reproducir proyectos de ARMATURA en un navegador o en node, y la primera persona que pueda compilar el módulo. Documentación CC-BY-4.0; ejemplos de código MPL-2.0.*

> ## ESTADO: NUNCA COMPILADO, NUNCA EJECUTADO
>
> **El binario `.wasm` nunca se ha compilado, cargado ni ejecutado, en ningún lugar.** El destino de Rust `wasm32-unknown-unknown` no se pudo instalar en el entorno de desarrollo (la descarga de la biblioteca estándar del destino está bloqueada allí; se reintentó al escribir esta página y siguió fallando con `unsuccessful tunnel`). Por tanto, todo lo que esta página dice sobre el navegador es **diseño sin verificar y lectura del código**, no comportamiento observado.
>
> Qué *se* probó, y hasta dónde llega:
>
> | Capa | Prueba | Demuestra | No demuestra |
> |---|---|---|---|
> | Las funciones de ABI de Rust (`arm_*`) | `cargo test -p armatura-wasm`: 8 pruebas unitarias en el **host** (Linux x86-64), llamando directamente a las funciones exportadas | la comprobación de argumentos, los códigos de error, el protocolo del búfer de salida, las máquinas de estados, las llamadas de maquetación / desplazamiento / nivel de detalle, el rechazo de salidas demasiado grandes antes de reservar memoria | que el código compile para `wasm32`, que los pánicos se comporten, nada sobre la memoria ni las exportaciones del módulo |
> | El envoltorio de JavaScript | `node armatura-wasm/js/test.mjs`: el envoltorio frente a un **mock de JavaScript** de las exportaciones (tres secciones de mock: renderizado, máquina de estados, maquetación) | las secuencias de llamadas del envoltorio, el paso de cadenas, la traducción de errores, la copia de resultados | que el módulo real exporte lo que simula el mock (nombres, `memory`, firmas) |
> | La página de demostración | cargada en Chromium sin interfaz **sin** el archivo `.wasm` | las rutas de importación del módulo se resuelven y la página informa «wasm not found (not built yet)» en lugar de fallar | ningún renderizado en un navegador |
> | Dependencias | `cargo tree -p armatura-wasm`: ni `rhai` ni `smartstring` en el árbol | el motor de scripting no forma parte del módulo | que las dependencias restantes compilen para wasm32 |
>
> **Riesgos de dependencias que nadie ha comprobado en wasm32:** (1) la compilación de `zip`, `tiny-skia`, `png`, `gif`, `jpeg-decoder`, `i_overlay` (y sus dependencias) para `wasm32-unknown-unknown`; (2) `std::time::Instant` / `SystemTime`, `process::id` y `thread::spawn`, que provocan un **pánico** en `wasm32-unknown-unknown`: desde la wave 7 el núcleo ya no los llama directamente. Todo uso pasa por `armatura_core::clock` (un tipo de marca de tiempo, `unix_seconds`, `process_id`; el constructor de proxies LOD recurre al hilo que llama cuando no se puede crear un hilo), que en este destino lee un reloj suministrado por el host (la exportación `arm_set_clock(mono_ms, unix_ms)`, alimentada por el envoltorio de JavaScript con `performance.now()` y `Date.now()` antes de `load`, `svg` y `rgba`; un host que nunca la llame obtiene duraciones de 0, de modo que los presupuestos de tiempo nunca expiran) y nunca provoca un pánico. Una prueba (`armatura-core/tests/wasm_safety.rs`) falla cuando aparece un nuevo uso directo fuera de `clock.rs`. Es una garantía solo a nivel de código fuente: **nunca se hizo una compilación para wasm32**, y Rhai (que además necesitaría su feature `no_time`) no está en la compilación; (3) los módulos ligados al sistema de archivos y a procesos compilan pero fallan en tiempo de ejecución: `audio.rs` (apertura de archivos), `soundtrack.rs` (ffprobe), `snapshots.rs`, `open_formats.rs` (directorios de exportación), `agent.rs` (guardar), y la carga de imágenes y mapas de normales con rutas relativas en `engine.rs`, `meshtrace.rs` y `postfx.rs`; devuelven errores en lugar de funcionar en un navegador, y ninguna feature de cargo los controla; (4) las funciones de coma flotante (`sin`, `cos`, `powf`, ...) proceden de una biblioteca matemática distinta en wasm32, por lo que los píxeles renderizados pueden diferir ligeramente de los nativos; (5) pánicos: en `wasm32-unknown-unknown` el valor por defecto es `panic = "abort"`, `catch_unwind` no puede interceptar nada, y un pánico de Rust deja atrapada toda la instancia (`WebAssembly.RuntimeError: unreachable`). Consulta la [lista de verificación](#lista-de-verificación-para-la-primera-persona-que-pueda-compilarlo) al final.

Contenido: [Qué es](#qué-es) - [Compilación](#compilación) - [Modelo de memoria y la ABI](#modelo-de-memoria-y-la-abi) - [Referencia de exportaciones](#referencia-de-exportaciones) - [El envoltorio de JavaScript](#el-envoltorio-de-javascript) - [La página de demostración](#la-página-de-demostración) - [Diferencias con la ABI de C](#diferencias-con-la-abi-de-c) - [Aspectos por pulir conocidos](#aspectos-por-pulir-conocidos) - [Lista de verificación](#lista-de-verificación-para-la-primera-persona-que-pueda-compilarlo)

## Qué es

`armatura-wasm` es un `cdylib` de **funciones `extern "C"` simples** (sin `wasm-bindgen`, sin código de enlace generado) sobre la fachada estable de Rust ([API.md](API.md)). Compilado para `wasm32-unknown-unknown`, ofrece un módulo que puede abrir un proyecto, renderizar fotogramas como texto SVG o como píxeles RGBA y ejecutar máquinas de estados con maquetación, desplazamiento y nivel de detalle. No puede editar, exportar, controlar versiones ni ejecutar herramientas; no tiene sistema de archivos, red, audio ni hilos. `armatura.js` es un módulo ES sin dependencias (navegadores y node >= 18) que oculta el manejo de punteros.

Licencia: MPL-2.0 (crate y envoltorio). El módulo depende de `armatura-core` con `default-features = false`: sin motor de scripting y sin ningún crate de terceros con licencia MPL.

## Compilación

Necesita una máquina donde se pueda instalar el destino (y, opcionalmente, `wasm-opt` de Binaryen para reducir el tamaño).

```sh
rustup target add wasm32-unknown-unknown
./armatura-wasm/build-wasm.sh
```

El script (`armatura-wasm/build-wasm.sh`, un script de shell POSIX) hace exactamente esto, desde la raíz del repositorio:

1. `cargo build --target wasm32-unknown-unknown --release -p armatura-wasm`
2. copia `target/wasm32-unknown-unknown/release/armatura_wasm.wasm` a `armatura-wasm/armatura_wasm.wasm`;
3. si `wasm-opt` está en el `PATH`, reescribe ese archivo con `wasm-opt -Oz`;
4. imprime el tamaño del resultado y una sugerencia para ejecutar la prueba de node o servir la demostración.

**El script nunca se ha ejecutado.** Su sugerencia final dice `node armatura-wasm/js/test.mjs`; con un `.wasm` real presente, la prueba necesita además una ruta de proyecto como primer argumento (`node armatura-wasm/js/test.mjs project.json`), de lo contrario se detiene con «pass a project JSON path as argv[2]». El tamaño del módulo nunca se ha medido.

Después sirve la carpeta por HTTP (los navegadores no cargan módulos ni `.wasm` desde `file://`) y abre la página de demostración:

```sh
cd armatura-wasm && python3 -m http.server 8000      # then open http://localhost:8000/index.html
```

Si usas `WebAssembly.instantiateStreaming` (el envoltorio lo hace cuando le pasas una `Response`), el servidor debe enviar `Content-Type: application/wasm`; la ruta por URL del envoltorio usa `fetch(...).arrayBuffer()` y no tiene ese requisito. Si tu página establece una Content-Security-Policy, debe permitir la compilación de WebAssembly (`'wasm-unsafe-eval'` en `script-src`).

Un destino alternativo, `wasm32-wasip1`, se compila con `-C panic=unwind` para que `catch_unwind` funcione (el código fuente lo menciona); no se ha probado y necesita un runtime o shim de WASI.

## Modelo de memoria y la ABI

El módulo posee una **memoria lineal** (exportada como `memory`). El host nunca comparte punteros con el motor salvo mediante estas reglas:

* **Entrada.** Para pasar bytes (el JSON del proyecto, o un nombre), el host llama a `arm_alloc(len)`, que devuelve un puntero a `len` bytes **puestos a cero** (nulo cuando `len` es 0 o supera 256 MiB). El host escribe en `new Uint8Array(memory.buffer, ptr, len)`, llama a la función con `(ptr, len)` y luego libera el bloque con `arm_free(ptr, len)` usando el **mismo** puntero y longitud. El motor copia lo que necesita; no conserva ningún puntero a tu bloque.
* **Salida.** Los resultados se producen en un búfer **dentro del handle**: tras una llamada que devuelve una longitud `n >= 0`, `arm_out_ptr(h)` / `arm_out_len(h)` dan los bytes (`n` es igual a la longitud), y para las imágenes `arm_out_width(h)` / `arm_out_height(h)` dan el tamaño (0 tras resultados SVG o de texto). El puntero es válido **hasta la siguiente llamada que produzca salida, o hasta `arm_close`**. El host debe copiar los bytes de inmediato (el envoltorio lo hace: `.slice()`). No conserves nunca una vista `Uint8Array` entre llamadas: además de sobrescribirse, cualquier llamada puede hacer crecer la memoria del módulo, lo que desprende `memory.buffer` e invalida todas sus vistas. Vuelve a leer `memory.buffer` después de cada llamada.
* **Errores.** Una función que falla devuelve `-1` (o un handle nulo) y almacena el mensaje; `arm_error_ptr()` / `arm_error_len()` dan sus bytes UTF-8 (sin terminador NUL). El espacio es global al módulo (es de un solo hilo), lo sobrescribe el siguiente fallo y **los éxitos no lo borran**, así que léelo solo justo después de un fallo.
* **Cadenas de entrada.** `(ptr, len)` UTF-8, `len` entre 1 y 4096 (una cadena vacía o más larga es un error).
* **Handles.** `arm_open` devuelve un puntero opaco a un handle (nulo si hay error). Un handle contiene la animación, el player opcional y el búfer de salida. Ciérralo con `arm_close` exactamente una vez; se permite `null`.
* **Límites.** JSON de proyecto de 1 byte a 256 MiB; `arm_render_rgba` se niega antes de reservar memoria si la imagen superaría 256 MiB (`output would be WxH pixels (N bytes), more than the limit of 268435456 bytes: lower the scale`); escala de 0.05 a 4; los fotogramas fuera de la línea de tiempo se limitan; longitudes devueltas como `i32`. El renderizado necesita memoria para la imagen más búferes de trabajo; el espacio de direcciones de 32 bits (4 GiB, menos en algunos navegadores) es el techo real.
* **Sin hilos, sin promesa sobre pánicos.** El módulo es de un solo hilo. Los pánicos solo se capturan en los destinos que hacen unwind; en `wasm32-unknown-unknown` un pánico deja atrapada la instancia y el host debe descartarla y crear una nueva. El motor está escrito para no entrar en pánico con entradas del usuario y comprueba los tamaños antes de reservar memoria, pero esta capa no puede prometer sobrevivir a un error.

## Referencia de exportaciones

36 exportaciones más `memory`. `ptr` y `len` son `i32` en la firma de wasm (`usize` en Rust). `h` es un handle. Devuelve `-1` = error salvo que se indique otra cosa.

| Exportación | Firma (vista de Rust) | Resultado |
|---|---|---|
| `arm_abi_version` | `() -> u32` | 1 |
| `arm_set_clock` | `(mono_ms: f64, unix_ms: f64)` | opcional: da la hora al módulo (`performance.now()`, `Date.now()`); los valores no válidos se ignoran. Añadida después de fijar la versión 1 de la ABI; el envoltorio la llama cuando está presente |
| `arm_alloc` | `(len) -> *mut u8` | bloque puesto a cero, nulo si `len` es 0 o > 256 MiB |
| `arm_free` | `(ptr, len)` | libera un bloque de `arm_alloc` (nulo / longitud cero se ignoran) |
| `arm_open` | `(json_ptr, json_len) -> *mut Handle` | handle o nulo; valida el proyecto |
| `arm_close` | `(h)` | libera el handle |
| `arm_frame_count`, `arm_fps`, `arm_width`, `arm_height` | `(h) -> u32 / f32 / f32 / f32` | `width` / `height` son el tamaño del escenario en unidades del escenario (sin redondear); 0 y un error para un handle nulo |
| `arm_render_svg` | `(h, frame: f32, show_bones: i32) -> i32` | texto SVG en el búfer de salida; su longitud en bytes |
| `arm_render_rgba` | `(h, frame: f32, scale: f32) -> i32` | RGBA8 con alfa directo en el búfer de salida; longitud en bytes (`w*h*4`); tamaño mediante `arm_out_width/height` |
| `arm_out_ptr`, `arm_out_len` | `(h) -> *const u8 / usize` | el búfer de salida (nulo / 0 para un handle nulo) |
| `arm_out_width`, `arm_out_height` | `(h) -> u32` | tamaño de la imagen del último renderizado RGBA (0 tras texto) |
| `arm_machine_count` | `(h) -> u32` | máquinas del proyecto |
| `arm_machine_name` | `(h, index) -> i32` | nombre en el búfer de salida; su longitud; -1 si el índice es incorrecto |
| `arm_player_start` | `(h, name_ptr, name_len) -> i32` | inicia (o reinicia) la máquina; 0 |
| `arm_player_stop` | `(h)` | descarta el player |
| `arm_player_set_bool` | `(h, ptr, len, value: i32) -> i32` | 0 |
| `arm_player_set_number` | `(h, ptr, len, value: f64) -> i32` | 0 |
| `arm_player_fire` | `(h, ptr, len) -> i32` | 0 |
| `arm_player_set_value` | `(h, ptr, len, value: f64) -> i32` | 0; valor externo para bindings / entrada de desplazamiento |
| `arm_player_advance` | `(h, dt: f64) -> i32` | 0; `dt` en `0..=10` segundos |
| `arm_player_state` | `(h, layer_ptr, layer_len) -> i32` | nombre del estado en el búfer de salida; longitud; -1 capa desconocida |
| `arm_player_events` | `(h) -> i32` | eventos pendientes como JSON en el búfer de salida, **consumidos**; longitud |
| `arm_player_render_svg` | `(h, show_bones: i32) -> i32` | SVG de la pose actual en el búfer de salida |
| `arm_player_render_rgba` | `(h, scale: f32) -> i32` | RGBA de la pose actual; tamaño mediante `arm_out_width/height` |
| `arm_player_set_canvas` | `(h, w: f32, h: f32) -> i32` | 0 |
| `arm_player_layout` | `(h) -> i32` | JSON `{"canvas":[w,h],"scrolls":[ids],"lod":level}` en el búfer de salida |
| `arm_player_scroll_drag` | `(h, id_ptr, id_len, x: f32, y: f32) -> i32` | 0 |
| `arm_player_scroll_release` | `(h, id_ptr, id_len, vx: f32, vy: f32) -> i32` | 0 |
| `arm_player_scroll_info` | `(h, id_ptr, id_len) -> i32` | JSON `{"offset":[x,y],"max":[x,y],"settled":bool}` en el búfer de salida |
| `arm_player_set_screen` | `(h, w_px: f32, h_px: f32, max_error_px: f32) -> i32` | 0; elige el nivel de detalle |
| `arm_error_ptr`, `arm_error_len` | `() -> *const u8 / usize` | texto del último error |

Todas las llamadas `arm_player_*` excepto `arm_player_start` necesitan un player iniciado («no player started (call arm_player_start)» en caso contrario). Las llamadas del player son los métodos `Player` de la fachada ([API.md](API.md)).

## El envoltorio de JavaScript

`armatura-wasm/js/armatura.js` exporta `class Armatura` (también como exportación por defecto). Todo método que cambia algo devuelve `this`, de modo que las llamadas se encadenan. Los errores se lanzan como `Error` con el mensaje del motor.

| Miembro | Descripción |
|---|---|
| `static async instantiate(source, imports = {})` | devuelve el objeto de exportaciones sin procesar. `source`: un `ArrayBuffer` o un array tipado (bytes), una `Response` (usa `instantiateStreaming`; necesita el MIME `application/wasm`), o una cadena URL / `URL` (usa `fetch` + `arrayBuffer`). El módulo no tiene importaciones, así que `imports` sigue siendo `{}`. |
| `static async create(source, project)` | `source` es o bien unas exportaciones ya instanciadas (un objeto con `arm_open`) o cualquier cosa que acepte `instantiate`; `project` es texto JSON o un objeto simple (se aplica `JSON.stringify`). Devuelve un `Armatura`. |
| `load(project)` | carga o reemplaza el proyecto; lanza una excepción si falla y entonces **mantiene utilizable el proyecto cargado anteriormente**. Rellena `frames`, `fps`, `width`, `height`. |
| `frames`, `fps`, `width`, `height` | datos de la última llamada a `load` (`width` / `height` en unidades del escenario, floats) |
| `svg(n, showBones = false)` | `string`: documento SVG del fotograma `n` (se admiten fotogramas fraccionarios, limitados) |
| `rgba(n, scale = 1)` | `{ width, height, data }`: `data` es un `Uint8ClampedArray` (RGBA con alfa directo, una copia que te pertenece) |
| `frame(n, { kind = 'image', scale = 1, bones = false } = {})` | `kind: 'svg'` da `{ svg }`; en otro caso `{ image: ImageData }` en un navegador o `{ width, height, data }` donde `ImageData` no existe (node) |
| `machines()` | `string[]` nombres de las máquinas |
| `startPlayer(name)`, `stopPlayer()` | inicia (reinicia) / descarta el player; las entradas vuelven a sus valores por defecto |
| `setBool(name, v)`, `setNumber(name, v)`, `fire(name)`, `setValue(path, v)` | entradas y valores externos; un nombre vacío lanza `empty string`, un nombre de más de 4096 bytes lanza una excepción |
| `advance(dt)` | avanza la máquina y las regiones de desplazamiento `dt` segundos (`0..=10`) |
| `state(layer)` | `string` estado actual de una capa de máquina |
| `events()` | `{ time, layer, kind: 'entered' \| 'exited', state }[]`, **consumidos** por la llamada |
| `setCanvas(w, h)` | maquetación adaptable para un tamaño de lienzo en unidades del escenario |
| `layout()` | `{ canvas: [w, h], scrolls: string[], lod: number }` |
| `scrollDrag(id, x, y)`, `scrollRelease(id, vx = 0, vy = 0)` | arrastre del puntero / velocidad de suelta en px y px/s |
| `scrollInfo(id)` | `{ offset: [x, y], max: [x, y], settled: boolean }` |
| `setScreen(widthPx, heightPx, maxErrorPx = 0.5)` | elige el nivel de detalle para un tamaño de pantalla |
| `playerSvg(showBones = false)`, `playerRgba(scale = 1)` | la pose actual del player |
| `play(canvas, { scale = 1, loop = true, onFrame } = {})` | dibuja la línea de tiempo en un canvas 2D con `requestAnimationFrame` (recurre a `setTimeout` fuera de un navegador); devuelve una función de parada |
| `playMachine(canvas, { scale = 1, onTick } = {})` | avanza la máquina iniciada según el tiempo real transcurrido (como máximo 0,25 s por tick) y la dibuja; `onTick(dt)` se ejecuta antes de cada avance, así que establece allí las entradas; devuelve una función de parada |
| `stop()` | detiene `play` / `playMachine` |
| `close()` | detiene y libera el handle; llámalo al terminar |

El envoltorio realiza las consultas a `x.memory.buffer` dentro de cada llamada y copia los resultados con `.slice()`, de modo que el crecimiento de la memoria no puede dejarlo con una vista obsoleta. Cada objeto `Armatura` posee un handle; crea varios objetos (pueden compartir un mismo `exports`) para mantener varios proyectos. No compartas un mismo objeto entre tareas asíncronas concurrentes que intercalen llamadas y lecturas de resultados: el resultado de cada llamada vive en el búfer de salida del handle hasta la siguiente llamada.

### Ejemplo (secuencia de llamadas)

Esta es la secuencia exacta de llamadas que ejercita la prueba de node. **Se ejecutó contra el mock de JavaScript** (el sustituto de `test.mjs` con respuestas predefinidas: máquina `hero`, capa `body`, entrada booleana `go`, región de desplazamiento `list`), no contra un módulo real.

```js
import { Armatura } from './js/armatura.js';

// 1. Instantiate the module once (a URL in the browser; in node pass the bytes: readFileSync('armatura_wasm.wasm')).
const exports = await Armatura.instantiate('./armatura_wasm.wasm');

// 2. Load a project (JSON text or a plain object). Throws Error(<engine message>) when it is invalid.
const anim = await Armatura.create(exports, projectJsonText);
console.log(`${anim.width}x${anim.height} stage units, ${anim.frames} frames @ ${anim.fps} fps`);

// 3. Render. Frames may be fractional and are clamped to the timeline.
const svg = anim.svg(10, /* showBones */ false);        // string
const { width, height, data } = anim.rgba(10, 0.5);     // data: Uint8ClampedArray, straight-alpha RGBA, a copy you own

// 4. Interactive playback with a state machine.
console.log(anim.machines());                           // ['hero']
anim.startPlayer('hero').setBool('go', true).advance(0.1);
console.log(anim.state('body'), anim.events());         // 'b', [{time, layer, kind, state}, ...] (consumed)
const poseSvg = anim.playerSvg();
const pose = anim.playerRgba(0.5);

// 5. Layout and scrolling.
anim.setCanvas(100, 120);
anim.scrollDrag('list', 0, 50).scrollRelease('list', 0, 100);
console.log(anim.scrollInfo('list'));                   // { offset: [0, 50], max: [0, 80], settled: false }
anim.setScreen(100, 120, 4);                            // pick a level of detail within 4 px
console.log(anim.layout());                             // { canvas: [100, 120], scrolls: ['list'], lod: 2 }

// 6. Always release the handle (it frees the project inside the module's memory).
anim.close();
```

En una página de navegador:

```html
<canvas id="c"></canvas>
<script type="module">
  import { Armatura } from './js/armatura.js';
  const x = await Armatura.instantiate('./armatura_wasm.wasm');
  const text = await (await fetch('./project.json')).text();
  const anim = await Armatura.create(x, text);

  // Timeline playback: requestAnimationFrame loop, loops at the end, returns a stop function.
  const stop = anim.play(document.getElementById('c'), { scale: 1, loop: true });

  // Or run a state machine in real time; onTick(dt) runs before each advance, so set inputs there.
  // anim.startPlayer('hero');
  // const stopMachine = anim.playMachine(canvas, { onTick: () => anim.setBool('go', keyIsDown) });
</script>
```

## La página de demostración

`armatura-wasm/index.html` es una página mínima: importa el envoltorio, instancia `./armatura_wasm.wasm` y ofrece un campo de archivo (proyecto `.json`) y un botón Stop. Al elegir un archivo se crea un `Armatura`, se muestra `WIDTHxHEIGHT, N frames @ FPS fps` y se inicia `play(canvas)`; los errores del motor se muestran en el texto junto a los botones. Si el módulo no se puede obtener o compilar, muestra `wasm not found (not built yet): <reason>`. Probada únicamente en ese estado «sin compilar», en Chromium sin interfaz: la página carga, la importación del módulo se resuelve y aparece el mensaje (con la página 404 del servidor como «módulo», el motivo dice `expected magic word 00 61 73 6d, found 3c 21 44 4f`, véase más abajo). Todo lo que va más allá no está verificado.

## Diferencias con la ABI de C

| | ABI de C ([C_ABI.md](C_ABI.md)) | WebAssembly |
|---|---|---|
| Cadenas de entrada | terminadas en NUL | `(ptr, len)` copiadas con `arm_alloc` |
| Resultados de salida | búferes proporcionados por quien llama | el búfer de salida del handle (`arm_out_ptr/len`), válido hasta la siguiente llamada con salida |
| Consulta de tamaño | un búfer `NULL` devuelve el tamaño | no hace falta: la llamada devuelve la longitud y los datos ya están en el búfer de salida |
| Texto de error | cadena NUL de `armatura_last_error()`, por hilo | bytes de `arm_error_ptr/len`, global |
| Matrices de huesos | `armatura_bone_world` | no exportado |
| SVG | no exportado | `arm_render_svg`, `arm_player_render_svg` |
| Eventos | patrón de consulta de tamaño, se conservan si el búfer es demasiado pequeño | siempre consumidos |
| Tamaño del escenario | enteros redondeados | floats |

## Aspectos por pulir conocidos

Encontrados al leer el código; no se ha visto fallar ninguno:

* `Armatura.instantiate(url)` comprueba `response.ok` y los errores de fetch e informa de la URL y del estado HTTP (pruebas con fetch simulado en `armatura-wasm/js/test.mjs`; ejecutadas solo bajo node, la compilación de wasm en sí no se pudo ejecutar aquí).
* `frame()` con `kind: 'svg'` devuelve `{ svg }`, no una cadena como `svg()`.
* La sugerencia que imprime `build-wasm.sh` ahora incluye el argumento `<project.json>` que necesita `test.mjs`.
* `play()` dibuja convirtiendo cada fotograma a RGBA en la CPU y llamando a `putImageData`; no hay audio, ni pausa, ni búsqueda de posición.
* El envoltorio no exporta declaraciones de TypeScript.

## Lista de verificación para la primera persona que pueda compilarlo

Haz esto en orden; detente en el primer fallo y anota el mensaje exacto. Los elementos marcados (riesgo) corresponden a los riesgos de dependencias de arriba. Cuando tengas resultados, actualiza [LIMITATIONS.md](LIMITATIONS.md) (sección 1), la línea R-090 de la sección 9 de ROADMAP.md y el recuadro de estado de esta página.

1. **Instala el destino y compila.** `rustup target add wasm32-unknown-unknown && ./armatura-wasm/build-wasm.sh`. (riesgo) Comprueba si compilan `zip`, `tiny-skia`, `png`, `gif`, `jpeg-decoder` e `i_overlay`. Anota el compilador y `rustc --version`.
2. **Sin motor de scripting.** `cargo tree --target wasm32-unknown-unknown -p armatura-wasm | grep -ci 'rhai\|smartstring'` debe imprimir 0.
3. **Mira las exportaciones.** En node: `WebAssembly.Module.exports(new WebAssembly.Module(fs.readFileSync('armatura-wasm/armatura_wasm.wasm')))`. Espera `memory` y las 36 funciones de la [tabla de exportaciones](#referencia-de-exportaciones), y **ninguna importación** (`WebAssembly.Module.imports(...)` vacío). Si falta `memory` o aparecen importaciones (por ejemplo `env.*` de una feature de std), hay que adaptar el envoltorio.
4. **Tamaño.** Anota el tamaño antes y después de `wasm-opt -Oz`, y los tamaños con gzip / brotli. Nunca se ha medido nada.
5. **Ejecuta la prueba de node con un proyecto real.** `armatura demo demo.json && node armatura-wasm/js/test.mjs demo.json` debe imprimir `real wasm tests: ok` tras las tres secciones de mock.
6. **Compara con el renderizador nativo.** Para los fotogramas 0, 12 y 47 de la demostración a escalas 0.5 y 1, compara `Armatura.rgba` con `armatura render demo.json N out.png --scale S` decodificado a RGBA. (riesgo) Espera pequeñas diferencias en los bordes con antialiasing (biblioteca matemática distinta); informa de la diferencia absoluta máxima y media y de la fracción de píxeles por encima de 24/255. Una diferencia grande apunta a un error real.
7. **Ejercita el player.** Construye el proyecto del ejemplo de la ABI de C (demo + `take-record` + `clip add` + `machine add`, consulta [C_ABI.md](C_ABI.md)), maneja `startPlayer`, `setBool`, `advance`, `state`, `events`, `playerRgba`; y el proyecto con un contenedor y una región de desplazamiento para `setCanvas`, `scrollDrag`, `scrollRelease`, `scrollInfo`, `setScreen` (esta última construye los niveles de detalle: mide el tiempo, es lenta sin optimizar y nunca se ha ejecutado en wasm).
8. **Caminos a prueba de fallos.** Pasa un proyecto no válido, una cadena vacía, un archivo aleatorio de 5 MB, un proyecto con una ruta de imagen `source.file` y un proyecto con una pista de sonido. Todos los casos deben producir un `Error` lanzado con un mensaje, no un trap. (riesgo) Un trap (`RuntimeError: unreachable`) en cualquiera de ellos es un error que informar; los candidatos más probables son `Instant` / `SystemTime` o llamadas al sistema de archivos alcanzadas por una ruta inesperada.
9. **Renderizado grande.** `rgba(0, 4)` en la demostración (2560 x 1920) y un escenario de 12000 x 12000 a escala 1 (debe rechazarse con el mensaje «more than the limit», no agotar la memoria). Observa el crecimiento de la memoria y confirma que el envoltorio sigue funcionando después de que la memoria creciera (las vistas se vuelven a leer en cada llamada).
10. **Profundidad de pila.** `wasm32-unknown-unknown` da al hilo principal una pila pequeña por defecto (1 MiB en las cadenas de herramientas actuales). Abre un proyecto con grupos muy anidados y una cadena larga de expresiones (consulta `armatura-cli/tests/robust.rs` para entradas hostiles) y confirma que no hay un trap por desbordamiento de pila.
11. **Ejecución en el navegador.** Sirve la carpeta, abre `index.html` en Chromium y Firefox, elige un proyecto, confirma la reproducción, redimensiona, cierra y vuelve a abrir varios proyectos (comprobación de fugas: la memoria no debe crecer sin límite; `close()` libera el handle).
12. **`instantiateStreaming` y CSP.** Sirve con y sin `Content-Type: application/wasm`, y con una CSP que omita `'wasm-unsafe-eval'`; documenta lo que ocurre.
13. **Rendimiento.** Mide el tiempo de `rgba(n, 1)` para la demostración de 640 x 480 (release nativo: unos 14 ms por fotograma) y para un proyecto pesado; anota los fotogramas por segundo de `play()` en el navegador.
14. **Opcional: pánicos.** Decide si el comportamiento de trap ante un pánico es aceptable para tu uso. Si no, prueba la vía `wasm32-wasip1` + `-C panic=unwind` mencionada en el código fuente.
