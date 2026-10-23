## target/release/c_skia --fixture dense --res 1920x1080@1,3840x2160@1,3840x2160@2
date: 2026-10-23 22:41 IST

### C tiny-skia (cached layer + integer translate: false) — fixture `dense`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.0 | 47.01 | 54.55 | 62.20 | 64.44 | 71.70 | 18 / 18 | scene renders 61 blits 0 label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.0 | 47.01 | 25.83 | 61.12 | 61.22 | 62.30 | 17 / 18 | scene renders 121 blits 0 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan | 0.0 | 91.58 | 80.32 | 83.67 | 85.15 | 85.43 | 42 / 42 | scene renders 61 blits 0 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.0 | 91.58 | 26.94 | 132.15 | 141.64 | 142.73 | 43 / 43 | scene renders 121 blits 0 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan | 0.0 | 250.70 | 201.84 | 205.99 | 206.16 | 206.59 | 139 / 139 | scene renders 61 blits 0 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.0 | 250.70 | 57.27 | 305.22 | 318.89 | 326.92 | 139 / 139 | scene renders 121 blits 0 label renders 0 (0 ms) |
