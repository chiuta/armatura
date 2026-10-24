#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
# End-to-end pipeline test through the CLI (osatura-cli/tests/e2e.rs). Release build by default because
# the debug build is roughly an order of magnitude slower (see the timing table printed at the end).
# Usage: scripts/e2e.sh [--debug]
set -euo pipefail
cd "$(dirname "$0")/.."
profile="--release"; [ "${1:-}" = "--debug" ] && profile=""
export OSATURA_E2E_TIMINGS="${OSATURA_E2E_TIMINGS:-$(mktemp)}"
cargo test $profile -p osatura-cli --test e2e -- --nocapture
echo "--- per-stage timings ---"; cat "$OSATURA_E2E_TIMINGS"
