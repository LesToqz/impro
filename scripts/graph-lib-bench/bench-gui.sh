#!/usr/bin/env bash
# Present-cost probes on a private 3840x2160 headless Wayland compositor
# (a copy of the QA run-wl.sh with --virtual-monitor 3840x2160, own runtime
# dir). Never touches the user's session. Same locking rules as bench.sh.
set -u
cd "$(dirname "$0")"
mkdir -p results
BIN=target/release
unset WAYLAND_DISPLAY DISPLAY
export T=150
WL="$(git rev-parse --show-toplevel)/.ceo/scratch/run-wl-4k.sh"
run() {
  local out="results/$1.md"; shift
  echo "## $*" > "$out"
  echo "date: $(TZ=Asia/Kolkata date '+%Y-%m-%d %H:%M IST')" >> "$out"
  timeout "${T:-900}" "$@" >> "$out" 2> >(tee -a "$out.err" >&2) || echo "[exit $?] $*" | tee -a "$out"
  echo "[done] $out"
}
# window sizes on the 3840x2160 virtual monitor (GDK_SCALE is ignored by GTK3 on
# Wayland, so every row is device scale 1); override with SIZES="2560x1440"
for size in ${SIZES:-3840x2160 2560x1440 1920x1080}; do
  for mode in blit gl current; do
    PROBE_MODE=$mode PROBE_SIZE=$size run probe-wl4k-$mode-$size \
      dbus-run-session -- bash "$WL" $BIN/glarea_probe
  done
  PROBE_MODE=current PROBE_MOTION=1 PROBE_SIZE=$size run probe-wl4k-current-motion-$size \
    dbus-run-session -- bash "$WL" $BIN/glarea_probe
done
# the proof of concept (scripted pan + zoom, no pointer needed)
[ -n "${SKIP_POC:-}" ] || run poc-wl4k-auto dbus-run-session -- bash "$WL" $BIN/glarea_poc --auto --size 1920x1080
[ -n "${SKIP_POC:-}" ] || GDK_BACKEND=x11 run poc-x-auto xvfb-run -a -s '-screen 0 1920x1080x24' dbus-run-session -- $BIN/glarea_poc --auto --size 1280x720
