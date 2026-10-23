## target/release/b_cairo --fixture dense --res 1920x1080@1,3840x2160@1,3840x2160@2
date: 2026-10-23 22:38 IST

### B plain cairo (node aa Default) — fixture `dense`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.1 | 149.37 | 203.35 | 228.97 | 236.39 | 242.38 | 18 / 21 | nodes 1572 edges 4802 label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.1 | 149.37 | 80.94 | 229.20 | 235.07 | 236.65 | 24 / 24 | nodes 3000 edges 8654 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan | 0.0 | 270.89 | 261.44 | 266.83 | 269.71 | 273.20 | 48 / 48 | nodes 3000 edges 8654 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.0 | 270.89 | 83.48 | 398.90 | 421.60 | 433.54 | 49 / 49 | nodes 3000 edges 8654 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan | 0.0 | 576.25 | 536.43 | 555.93 | 569.90 | 571.47 | 145 / 145 | nodes 3000 edges 8654 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.0 | 576.25 | 167.09 | 803.39 | 841.65 | 908.01 | 145 / 145 | nodes 3000 edges 8654 label renders 0 (0 ms) |
