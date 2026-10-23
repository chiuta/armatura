#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
# Build the ARMATURA wasm module. Needs a machine where the target can be installed:
#   rustup target add wasm32-unknown-unknown
# NOTE: this script was NOT run in the authoring environment (target not installable there).
set -eu
cd "$(dirname "$0")/.."
cargo build --target wasm32-unknown-unknown --release -p armatura-wasm
OUT=target/wasm32-unknown-unknown/release/armatura_wasm.wasm
cp "$OUT" armatura-wasm/armatura_wasm.wasm
if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Oz "$OUT" -o armatura-wasm/armatura_wasm.wasm
fi
ls -l armatura-wasm/armatura_wasm.wasm
echo "now: node armatura-wasm/js/test.mjs <project.json>   (test.mjs needs the path of a project, e.g. out/demo.json from: armatura demo out/demo.json)"
echo "  or serve armatura-wasm/ and open index.html"
