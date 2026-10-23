## target/release/c_skia --fixture dense --res 1920x1080@1,3840x2160@1,3840x2160@2
date: 2026-10-23 23:05 IST

### C tiny-skia (cached layer + integer translate: false) — fixture `dense`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.0 | 46.51 | 54.38 | 61.52 | 61.97 | 64.10 | 18 / 18 | scene renders 61 blits 0 label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | pan-fit | 0.0 | 46.51 | 47.10 | 48.55 | 48.75 | 49.17 | 17 / 18 | scene renders 121 blits 0 label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.0 | 46.51 | 26.12 | 60.28 | 60.83 | 61.40 | 17 / 18 | scene renders 181 blits 0 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan | 0.0 | 102.19 | 79.64 | 87.12 | 88.25 | 90.11 | 42 / 42 | scene renders 61 blits 0 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan-fit | 0.0 | 102.19 | 102.04 | 105.80 | 106.93 | 108.45 | 42 / 42 | scene renders 121 blits 0 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.0 | 102.19 | 26.34 | 127.52 | 141.39 | 143.51 | 43 / 43 | scene renders 181 blits 0 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan | 0.0 | 266.85 | 202.16 | 211.53 | 213.35 | 219.22 | 139 / 139 | scene renders 61 blits 0 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan-fit | 0.0 | 266.85 | 256.61 | 261.72 | 262.29 | 265.44 | 139 / 139 | scene renders 121 blits 0 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.0 | 266.85 | 56.46 | 305.33 | 325.22 | 329.54 | 139 / 139 | scene renders 181 blits 0 label renders 0 (0 ms) |
