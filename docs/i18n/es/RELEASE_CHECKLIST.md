<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio) -->
# Lista de comprobación de publicación (lado del titular)

*Pasos que solo puede dar el titular, en orden. Las herramientas del proyecto no ejecutaron nada de esto: el entorno de compilación no tenía acceso a GitHub, ni destino wasm32, ni GPU real, ni abogado. Esto no es asesoramiento jurídico. Los elementos marcados con **[blocker]** deben hacerse antes de un anuncio público.*

## 1. Antes de publicar nada

- [ ] **[blocker]** Confirma o sustituye el contacto de seguridad (`alexio@trom.tf` es un marcador de posición en `docs/SECURITY.md`, en `--version` y en los documentos de licencias).
- [ ] **[blocker]** Busca el nombre «ARMATURA» en los registros de marcas que te interesen (EUIPO, USPTO, OSIM) y en tiendas de aplicaciones / registros de paquetes. Véase `TRADEMARK.md`. Si hay conflicto, cambia el nombre antes de la primera versión (el nombre aparece en el binario, los crates y la documentación).
- [ ] **[blocker]** Pide a un abogado que revise `LICENSING.md`, la división MPL-2.0 / AGPL-3.0, `docs/THIRD_PARTY.md` y `docs/PATENT_NOTES.md`. Decide si quieres doble licencia; si es así, hace falta un CLA *antes* de aceptar contribuciones externas (hoy: solo DCO).
- [ ] Comprueba las condiciones de la referencia de ajuste de curvas de Graphics Gems (citada de memoria en `CLEANROOM.md`); conserva la redacción de implementación independiente solo si es exacta.
- [ ] Ejecuta `reuse lint` y `cargo deny check` en tu máquina (el proyecto solo tiene su propia prueba `spdx_audit`).
- [ ] Comprueba que en el árbol no haya datos personales, rutas locales ni tokens (`grep -r "/home/" .`, `grep -ri token`), y que el modelo incluido y los recursos de la demo sean todos autogenerados.

## 2. Repositorio

- [ ] Crea el repositorio en GitHub (o Codeberg / tu propio Forgejo); sube el árbol **sin** `target/` ni `out/`.
- [ ] Conserva `LICENSE`, `LICENSES/`, `REUSE.toml`, `.github/FUNDING.yml` (Buy Me a Coffee `echo.of.the.strings`, Patreon `alexio_tf`).
- [ ] Activa la notificación privada de vulnerabilidades (pestaña Security) y haz que `docs/SECURITY.md` apunte a ella.
- [ ] Pasa la URL del repositorio con `--source-url` al ejecutar el estudio (oferta de código fuente de la AGPL, ruta `/source`) y prueba la oferta desde una instalación limpia.

## 3. Primera ejecución de CI (nunca ejecutada hasta ahora)

- [ ] Sube el código y vigila `.github/workflows/ci.yml`: pruebas en Linux, Windows, macOS y `clippy -D warnings`; el trabajo de compilación wasm32; el trabajo de cargo-deny y REUSE.
- [ ] Espera fallos en la primera ejecución en Windows/macOS (rutas, permisos, diferencias de píxeles en bordes con antialiasing, L-04 / L-26). Corrígelos o relájalos con tolerancias, y luego anota el resultado en `docs/LIMITATIONS.md`.
- [ ] Si el trabajo wasm32 pasa, ejecuta la lista de verificación de `docs/WASM.md` en un navegador real y actualiza L-01 / L-02.

## 4. Pruebas con personas

- [ ] Entrega el estudio a entre 3 y 5 animadores durante una semana con una tarea corta (hacer el rig de un personaje, crear un ciclo de caminar, exportar un GIF). Anota dónde se atascan; registra cada caso como una incidencia.
- [ ] Prueba en navegadores reales (Chrome, Firefox, Safari), en un dispositivo táctil y en una sesión solo con teclado.
- [ ] Prueba las funciones de detector de rostros / mocap / voz con datos reales; informa con honestidad de que son limitadas (L-12, L-13).

## 5. Seguridad del despliegue (si alojas el estudio o la colaboración)

- [ ] Ponlo detrás de un proxy inverso que termine TLS (receta: `docs/SECURITY.md` sección 12); nunca expongas a internet el puerto HTTP sin cifrar.
- [ ] Usa tokens por participante (`collab-token`), establece caducidad y enlaza a localhost detrás del proxy.
- [ ] Encarga una revisión de seguridad independiente antes de ofrecer colaboración alojada a desconocidos.

## 6. Artefactos de la versión

- [ ] Etiqueta una versión, actualiza `CHANGELOG.md`, compila los binarios de la versión en tus propias máquinas (el entorno del proyecto nunca compiló con `--release`), publica las sumas de comprobación.
- [ ] Adjunta el código fuente correspondiente de las partes AGPL (basta con la propia etiqueta; mantenla disponible mientras distribuyas binarios).
- [ ] Incluye `docs/THIRD_PARTY_NOTICES.md` (regenéralo con `scripts/licence-audit.sh`) con cada binario.
- [ ] Crates.io / npm / registros de paquetes: comprueba que los nombres estén libres y que los campos de licencia coincidan con `LICENSING.md`.

## 7. Anuncio y donaciones

- [ ] Primera pantalla del README: qué funciona y qué no (enlaza `docs/LIMITATIONS.md`); no afirmes «de clase Moho» sin matizarlo.
- [ ] Muestra las capturas de `docs/img/` y un GIF corto hecho con el proyecto de demostración.
- [ ] Añade los enlaces de donación (Buy Me a Coffee, Patreon) al README y al cuadro «Acerca de» del estudio; no prometas funciones a cambio de donaciones salvo que puedas cumplir la promesa.
- [ ] Publica donde están los animadores (comunidades de rigging 2D / animación indie); pide informes de errores, no elogios.
- [ ] Reserva tiempo para responder rápido a las primeras incidencias; las dos primeras semanas deciden si los colaboradores se quedan.
