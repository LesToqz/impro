## target/release/b_cairo --fixture packed --res 1920x1080@1,3840x2160@1,3840x2160@2
date: 2026-10-23 23:01 IST

### B plain cairo (node aa Default) — fixture `packed`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.1 | 20.61 | 22.26 | 23.66 | 24.31 | 24.86 | 23 / 24 | nodes 1883 edges 43 label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | pan-fit | 0.1 | 20.61 | 13.51 | 16.99 | 22.74 | 24.46 | 24 / 24 | nodes 3000 edges 346 label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.1 | 20.61 | 14.34 | 24.87 | 25.42 | 25.66 | 24 / 25 | nodes 3000 edges 346 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan | 0.0 | 39.57 | 24.82 | 29.08 | 30.40 | 33.01 | 49 / 49 | nodes 2764 edges 139 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan-fit | 0.0 | 39.57 | 21.49 | 24.71 | 26.47 | 27.26 | 49 / 49 | nodes 3000 edges 346 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.0 | 39.57 | 15.66 | 27.74 | 29.23 | 29.36 | 49 / 49 | nodes 3000 edges 346 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan | 0.0 | 107.13 | 57.55 | 87.11 | 101.80 | 116.41 | 144 / 144 | nodes 2764 edges 139 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan-fit | 0.0 | 107.13 | 51.08 | 81.79 | 92.97 | 93.43 | 144 / 144 | nodes 3000 edges 346 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.0 | 107.13 | 36.11 | 74.24 | 75.48 | 76.51 | 148 / 148 | nodes 3000 edges 346 label renders 0 (0 ms) |
