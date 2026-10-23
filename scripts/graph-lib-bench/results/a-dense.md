## target/release/a_current --fixture dense --res 1920x1080@1,3840x2160@1,3840x2160@2
date: 2026-10-23 22:07 IST

### A current Painter (motion=false, threads=default(≤4)) — fixture `dense`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.4 | 59.35 | 10.92 | 49.51 | 51.48 | 52.08 | 42 / 42 | nodes 396 lines 1259 arrows 1180 labels 0 |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.4 | 59.35 | 10.52 | 15.70 | 16.12 | 16.58 | 43 / 43 | nodes 3000 lines 1765 arrows 0 labels 0 |
| 3840x2160@1 → 3840x2160 | pan | 0.1 | 133.65 | 28.87 | 97.24 | 104.46 | 107.46 | 115 / 115 | nodes 2505 lines 7115 arrows 6872 labels 0 |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.1 | 133.65 | 16.67 | 56.72 | 59.22 | 60.75 | 126 / 126 | nodes 3000 lines 1765 arrows 0 labels 0 |
| 3840x2160@2 → 7680x4320 | pan | 0.1 | 180.58 | 89.64 | 212.79 | 230.40 | 236.00 | 424 / 424 | nodes 2505 lines 7115 arrows 6872 labels 0 |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.1 | 180.58 | 49.98 | 152.13 | 157.40 | 162.28 | 458 / 458 | nodes 3000 lines 1765 arrows 0 labels 0 |
