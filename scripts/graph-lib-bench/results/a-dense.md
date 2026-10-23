## target/release/a_current --fixture dense --res 1920x1080@1,3840x2160@1,3840x2160@2
date: 2026-10-23 22:37 IST

### A current Painter (motion=false, threads=default(≤4)) — fixture `dense`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.4 | 56.23 | 14.90 | 49.24 | 50.47 | 52.64 | 42 / 42 | nodes 1578 lines 4546 arrows 4182 labels 0 |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.4 | 56.23 | 9.74 | 16.27 | 16.46 | 17.82 | 42 / 42 | nodes 3000 lines 1765 arrows 0 labels 0 |
| 3840x2160@1 → 3840x2160 | pan | 0.1 | 122.00 | 29.90 | 94.69 | 100.67 | 105.50 | 115 / 115 | nodes 3000 lines 8389 arrows 8261 labels 0 |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.1 | 122.00 | 17.58 | 58.39 | 59.82 | 61.13 | 126 / 126 | nodes 3000 lines 1765 arrows 0 labels 0 |
| 3840x2160@2 → 7680x4320 | pan | 0.1 | 191.84 | 93.06 | 229.23 | 235.83 | 237.96 | 424 / 424 | nodes 3000 lines 8389 arrows 8261 labels 0 |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.1 | 191.84 | 51.44 | 164.03 | 169.80 | 170.33 | 458 / 458 | nodes 3000 lines 1765 arrows 0 labels 0 |
