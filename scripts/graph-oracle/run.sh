#!/usr/bin/env bash
# Differential check of the engine against the reference graph logic.
#   IMPRO_REFERENCE_DIR=<reference checkout> bash scripts/graph-oracle/run.sh [compare.py options]
# IMPRO_CARGO_WRAP may hold a command prefix for the build (e.g. a memory cap).
set -euo pipefail
cd "$(dirname "$0")/../.."
: "${IMPRO_REFERENCE_DIR:?set IMPRO_REFERENCE_DIR to the reference checkout}"
export PYTHONDONTWRITEBYTECODE=1
# shellcheck disable=SC2086
${IMPRO_CARGO_WRAP:-} cargo build -q -p impro-graph --example graph_dump
exec python3 -I scripts/graph-oracle/compare.py --dump target/debug/examples/graph_dump "$@"
