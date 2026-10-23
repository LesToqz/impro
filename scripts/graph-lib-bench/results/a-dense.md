## target/release/a_current --fixture dense --res 1920x1080@1,3840x2160@1,3840x2160@2
date: 2026-10-23 22:59 IST

### A current Painter (motion=false, threads=default(≤4)) — fixture `dense`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.9 | 79.91 | 16.52 | 49.94 | 54.71 | 66.78 | 42 / 42 | nodes 1578 lines 4546 arrows 4182 labels 0 |
| 1920x1080@1 → 1920x1080 | pan-fit | 0.9 | 79.91 | 16.02 | 21.11 | 22.45 | 28.19 | 43 / 43 | nodes 3000 lines 7771 arrows 7623 labels 0 |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.9 | 79.91 | 10.62 | 20.09 | 26.09 | 27.20 | 43 / 43 | nodes 3000 lines 1765 arrows 0 labels 0 |
| 3840x2160@1 → 3840x2160 | pan | 0.1 | 159.21 | 30.08 | 124.30 | 133.12 | 138.03 | 115 / 115 | nodes 3000 lines 8389 arrows 8261 labels 0 |
| 3840x2160@1 → 3840x2160 | pan-fit | 0.1 | 159.21 | 39.28 | 52.98 | 57.27 | 73.91 | 124 / 124 | nodes 3000 lines 8652 arrows 8627 labels 0 |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.1 | 159.21 | 17.75 | 46.29 | 47.40 | 47.67 | 126 / 126 | nodes 3000 lines 1765 arrows 0 labels 0 |
| 3840x2160@2 → 7680x4320 | pan | 0.1 | 208.35 | 100.41 | 216.02 | 234.34 | 236.33 | 424 / 424 | nodes 3000 lines 8389 arrows 8261 labels 0 |
| 3840x2160@2 → 7680x4320 | pan-fit | 0.1 | 208.35 | 110.49 | 141.69 | 156.41 | 197.47 | 452 / 452 | nodes 3000 lines 8652 arrows 8627 labels 0 |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.1 | 208.35 | 81.91 | 195.98 | 228.05 | 410.81 | 458 / 458 | nodes 3000 lines 1765 arrows 0 labels 0 |
