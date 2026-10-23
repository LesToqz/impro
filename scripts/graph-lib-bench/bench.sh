#!/usr/bin/env bash
# Reproduce the measurements. Heavy: run as
#   flock <home>/projects/impro-wt/.heavy.lock systemd-run --user --scope -q \
#     -p MemoryMax=3500M -p MemorySwapMax=0 ./bench.sh <phase>...
# Phases: build a b c d d-sw femtovg wgpu layout   (raw output goes to results/<phase>.md)
set -u
cd "$(dirname "$0")"
mkdir -p results
BIN=target/release
SUBSET=1920x1080@1,3840x2160@1,3840x2160@2   # reduced matrix for stress variants
run() { # name, command...
  local out="results/$1.md"; shift
  echo "## $*" > "$out"
  echo "date: $(TZ=Asia/Kolkata date '+%Y-%m-%d %H:%M IST')" >> "$out"
  timeout "${T:-900}" "$@" >> "$out" 2> >(tee -a "$out.err" >&2) || echo "[exit $?] $*" | tee -a "$out"
  echo "[done] $out"
}
for phase in "$@"; do
  case "$phase" in
    build)
      cargo build --release --keep-going \
        --features cur,labels,skia,gl,vg,wgpu,layout,glarea ;;
    a)
      run a-plain        $BIN/a_current
      run a-labels       $BIN/a_current --labels
      run a-motion       $BIN/a_current --motion --labels
      run a-dense        $BIN/a_current --fixture dense --res $SUBSET ;;
    b)
      run b-plain        $BIN/b_cairo
      run b-labels       $BIN/b_cairo --labels
      run b-dense        $BIN/b_cairo --fixture dense --res $SUBSET ;;
    c)
      run c-plain        $BIN/c_skia
      run c-cached       $BIN/c_skia --cached
      run c-labels       $BIN/c_skia --labels
      run c-dense        $BIN/c_skia --fixture dense --res $SUBSET ;;
    d) # real VM GPU through the render node (default Mesa choice)
      export T=60  # the GPU is shared with the user's desktop: at most 60 s per run
      run d-hw           $BIN/d_gl
      run d-hw-readback  $BIN/d_gl --readback
      run d-hw-labels    $BIN/d_gl --labels --readback
      run d-hw-dense     $BIN/d_gl --fixture dense --readback --res $SUBSET ;;
    d-sw) # forced software rasteriser
      export LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe T=400
      run d-sw           $BIN/d_gl
      run d-sw-readback  $BIN/d_gl --readback
      run d-sw-labels    $BIN/d_gl --labels --readback
      run d-sw-dense     $BIN/d_gl --fixture dense --readback --res $SUBSET ;;
    femtovg)
      export T=60
      run femtovg-hw     $BIN/d_femtovg --labels --res 1920x1080@1,3840x2160@1,3840x2160@2 ;;
    wgpu)
      export T=60
      run wgpu           $BIN/e_wgpu --res 1920x1080@1,3840x2160@1,3840x2160@2 ;;
    layout)
      run layout-app     $BIN/f_layout app
      run layout-dense   $BIN/f_layout dense ;;
    gui-x) # GTK toplevel on a virtual X server (GLX, llvmpipe); never the user's session
      unset WAYLAND_DISPLAY DISPLAY; export GDK_BACKEND=x11 T=150
      for size in 1920x1080 3840x2160; do
        for mode in gl blit cairo current; do
          [ "$size" = 3840x2160 ] && [ "$mode" = cairo ] && continue
          PROBE_MODE=$mode PROBE_SIZE=$size run probe-x-$mode-$size \
            xvfb-run -a -s '-screen 0 3840x2160x24' dbus-run-session -- $BIN/glarea_probe
        done
      done ;;
    gui-wl) # GTK toplevel on the private headless Wayland compositor
      unset WAYLAND_DISPLAY DISPLAY; export T=150
      WL="$(git rev-parse --show-toplevel)/.ceo/scratch/run-wl.sh"   # copy of run-wl.sh from the graph-click QA archive
      for mode in gl blit cairo current; do
        PROBE_MODE=$mode PROBE_SIZE=1600x900 run probe-wl-$mode \
          dbus-run-session -- bash "$WL" $BIN/glarea_probe
      done ;;
    *) echo "unknown phase $phase" >&2; exit 2 ;;
  esac
done
