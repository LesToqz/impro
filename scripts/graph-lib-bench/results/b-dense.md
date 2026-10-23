## target/release/b_cairo --fixture dense --res 1920x1080@1,3840x2160@1,3840x2160@2
date: 2026-10-23 22:08 IST

### B plain cairo (node aa Default) — fixture `dense`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.1 | 229.22 | 149.66 | 225.47 | 226.16 | 230.71 | 19 / 21 | nodes 394 edges 1259 label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.1 | 229.22 | 82.45 | 230.54 | 239.33 | 240.13 | 24 / 24 | nodes 3000 edges 8654 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan | 0.0 | 272.41 | 259.57 | 265.43 | 266.16 | 266.84 | 48 / 48 | nodes 2504 edges 7379 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.0 | 272.41 | 84.19 | 401.20 | 424.39 | 433.37 | 49 / 49 | nodes 3000 edges 8654 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan | 0.0 | 575.33 | 532.64 | 537.94 | 539.12 | 539.14 | 145 / 145 | nodes 2504 edges 7379 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.0 | 575.33 | 166.86 | 795.09 | 839.35 | 859.33 | 145 / 145 | nodes 3000 edges 8654 label renders 0 (0 ms) |
