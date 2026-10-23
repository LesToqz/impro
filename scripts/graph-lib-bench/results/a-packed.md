## target/release/a_current --fixture packed --res 1920x1080@1,3840x2160@1,3840x2160@2
date: 2026-10-23 22:59 IST

### A current Painter (motion=false, threads=default(≤4)) — fixture `packed`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.4 | 21.28 | 2.94 | 3.33 | 3.49 | 3.58 | 31 / 31 | nodes 1887 lines 43 arrows 43 labels 0 |
| 1920x1080@1 → 1920x1080 | pan-fit | 0.4 | 21.28 | 2.88 | 3.62 | 3.70 | 3.76 | 42 / 42 | nodes 3000 lines 214 arrows 115 labels 0 |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.4 | 21.28 | 2.69 | 3.76 | 4.15 | 4.48 | 43 / 43 | nodes 3000 lines 50 arrows 0 labels 0 |
| 3840x2160@1 → 3840x2160 | pan | 0.1 | 43.60 | 13.45 | 20.40 | 21.31 | 21.65 | 84 / 84 | nodes 2764 lines 139 arrows 144 labels 0 |
| 3840x2160@1 → 3840x2160 | pan-fit | 0.1 | 43.60 | 17.90 | 23.59 | 25.69 | 26.92 | 131 / 131 | nodes 3000 lines 346 arrows 346 labels 0 |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.1 | 43.60 | 8.99 | 14.51 | 15.12 | 15.19 | 132 / 132 | nodes 3000 lines 50 arrows 0 labels 0 |
| 3840x2160@2 → 7680x4320 | pan | 0.1 | 147.83 | 48.06 | 75.11 | 92.59 | 105.34 | 322 / 322 | nodes 2764 lines 139 arrows 144 labels 0 |
| 3840x2160@2 → 7680x4320 | pan-fit | 0.1 | 147.83 | 50.36 | 78.62 | 79.34 | 80.00 | 481 / 481 | nodes 3000 lines 346 arrows 346 labels 0 |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.1 | 147.83 | 40.32 | 52.72 | 54.39 | 55.84 | 487 / 487 | nodes 3000 lines 50 arrows 0 labels 0 |
