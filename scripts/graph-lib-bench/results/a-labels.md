## target/release/a_current --labels
date: 2026-10-23 22:07 IST

### A current Painter (motion=false, threads=default(≤4)) — fixture `app`, labels 300, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.4 | 27.56 | 4.55 | 7.34 | 9.22 | 14.92 | 40 / 40 | nodes 158 lines 43 arrows 43 labels 197 labels≤300 |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.4 | 27.56 | 10.61 | 28.47 | 33.83 | 34.34 | 60 / 60 | nodes 3000 lines 50 arrows 0 labels 0 labels≤300 |
| 2560x1440@1 → 2560x1440 | pan | 0.1 | 23.74 | 11.17 | 13.62 | 14.59 | 16.69 | 62 / 62 | nodes 860 lines 53 arrows 54 labels 921 labels≤300 |
| 2560x1440@1 → 2560x1440 | zoom-out | 0.1 | 23.74 | 10.82 | 42.50 | 53.87 | 63.50 | 84 / 84 | nodes 3000 lines 50 arrows 0 labels 0 labels≤300 |
| 3840x2160@1 → 3840x2160 | pan | 0.1 | 50.74 | 18.00 | 20.32 | 20.92 | 20.98 | 127 / 127 | nodes 2512 lines 94 arrows 96 labels 2551 labels≤300 |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.1 | 50.74 | 40.72 | 60.81 | 62.23 | 63.35 | 174 / 174 | nodes 3000 lines 50 arrows 0 labels 0 labels≤300 |
| 1920x1080@2 → 3840x2160 | pan | 0.1 | 31.42 | 17.76 | 20.62 | 20.80 | 28.65 | 124 / 174 | nodes 156 lines 43 arrows 43 labels 191 labels≤300 |
| 1920x1080@2 → 3840x2160 | zoom-out | 0.1 | 31.42 | 27.77 | 77.25 | 86.76 | 93.53 | 171 / 174 | nodes 3000 lines 50 arrows 0 labels 0 labels≤300 |
| 2560x1440@2 → 5120x2880 | pan | 0.1 | 73.82 | 34.65 | 39.64 | 41.11 | 42.58 | 197 / 197 | nodes 860 lines 53 arrows 54 labels 915 labels≤300 |
| 2560x1440@2 → 5120x2880 | zoom-out | 0.1 | 73.82 | 32.68 | 128.79 | 130.54 | 143.80 | 264 / 264 | nodes 3000 lines 50 arrows 0 labels 0 labels≤300 |
| 3840x2160@2 → 7680x4320 | pan | 0.1 | 169.99 | 58.05 | 65.41 | 85.78 | 95.62 | 338 / 338 | nodes 2511 lines 94 arrows 96 labels 2548 labels≤300 |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.1 | 169.99 | 109.72 | 179.32 | 191.70 | 194.45 | 527 / 527 | nodes 3000 lines 50 arrows 0 labels 0 labels≤300 |
