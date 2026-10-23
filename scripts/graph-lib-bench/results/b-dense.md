## target/release/b_cairo --fixture dense --res 1920x1080@1,3840x2160@1,3840x2160@2
date: 2026-10-23 23:01 IST

### B plain cairo (node aa Default) — fixture `dense`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.1 | 161.91 | 210.11 | 240.40 | 248.56 | 261.23 | 18 / 21 | nodes 1572 edges 4802 label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | pan-fit | 0.1 | 161.91 | 161.42 | 166.16 | 167.28 | 170.26 | 24 / 24 | nodes 3000 edges 8654 label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.1 | 161.91 | 84.69 | 224.18 | 228.73 | 232.55 | 24 / 24 | nodes 3000 edges 8654 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan | 0.0 | 300.84 | 274.05 | 326.73 | 341.68 | 364.75 | 48 / 48 | nodes 3000 edges 8654 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan-fit | 0.0 | 300.84 | 343.36 | 441.41 | 455.68 | 481.49 | 48 / 48 | nodes 3000 edges 8654 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.0 | 300.84 | 90.60 | 426.80 | 433.40 | 445.63 | 49 / 49 | nodes 3000 edges 8654 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan | 0.0 | 645.43 | 552.37 | 588.69 | 598.66 | 603.97 | 145 / 145 | nodes 3000 edges 8654 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan-fit | 0.0 | 645.43 | 662.78 | 672.38 | 674.80 | 686.51 | 145 / 145 | nodes 3000 edges 8654 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.0 | 645.43 | 170.85 | 809.89 | 847.37 | 865.19 | 145 / 145 | nodes 3000 edges 8654 label renders 0 (0 ms) |
