#!/usr/bin/env bash
# Compares the cairo DrawingArea path ("current") with the GtkGLArea path ("gl")
# on THE DISPLAY YOU RUN IT ON: two windows of 1920x1080, about 2 s each, then
# it prints the frame-clock intervals. Run it from a normal desktop terminal.
# The first run builds the probe (a few minutes); later runs take about 5 s.
set -eu
cd "$(dirname "$0")"
cargo build --release --features glarea --bin glarea_probe -q
for m in current gl; do
  PROBE_SIZE=${PROBE_SIZE:-1920x1080} PROBE_MODE=$m target/release/glarea_probe 2>/dev/null |
    grep -E 'mode|GL_RENDERER|GLArea realize error|frame-clock'
done
