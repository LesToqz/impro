#!/usr/bin/env bash
# Clean-build cost per candidate: builds everything once into a scratch target
# dir with --timings and prints the compile time of each candidate's own crates.
# Heavy: run under the shared lock like bench.sh.
set -eu
cd "$(dirname "$0")"
T="$(git rev-parse --show-toplevel)/.ceo/timing-target"
rm -rf "$T"
CARGO_TARGET_DIR="$T" cargo build --release --timings --features cur,labels,skia,gl,vg,wgpu,layout,glarea 2>&1 | tail -3
python3 -I - "$T" <<'PY'
import glob, json, re, sys
html = open(sorted(glob.glob(sys.argv[1] + "/cargo-timings/cargo-timing-*.html"))[-1]).read()
units = json.loads(re.search(r"const UNIT_DATA = (\[.*?\]);\n", html, re.S).group(1))
by = {}
for u in units:
    by[u["name"]] = by.get(u["name"], 0) + u["duration"]
for group, names in {
    "tiny-skia": ["tiny-skia", "tiny-skia-path", "png", "flate2", "miniz_oxide", "fdeflate", "crc32fast", "adler2", "simd-adler32", "arrayref", "arrayvec", "strict-num"],
    "glow+khronos-egl": ["glow", "khronos-egl", "libloading"],
    "femtovg (own + deps not in glow)": ["femtovg", "rustybuzz", "ttf-parser", "unicode-bidi", "unicode-segmentation", "lru", "image", "imgref", "rgb", "fnv", "slotmap", "bitflags", "generational-arena", "unicode-script", "unicode-ccc", "unicode-properties", "smallvec", "hashbrown"],
    "wgpu (all)": [n for n in by if n.startswith(("wgpu", "naga", "ash", "khronos", "gpu-", "pollster", "bytemuck", "hashbrown", "indexmap", "codespan", "spirv", "thiserror", "half", "bit-set", "bit-vec", "rustc-hash", "log", "profiling", "raw-window", "arrayvec", "parking_lot", "once_cell", "bitflags", "glow", "libloading", "naga"))],
    "petgraph+fjadra+force_graph": ["petgraph", "fjadra", "force_graph", "fixedbitset", "foldhash", "hashbrown", "indexmap", "rand"],
}.items():
    print(f"{group}: {sum(by.get(n, 0) for n in names):.1f} s of rustc time (sum over its crates, jobs=3)")
print("total units:", len(units), "sum", f"{sum(by.values()):.0f} s")
PY
