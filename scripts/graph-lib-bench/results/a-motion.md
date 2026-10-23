## target/release/a_current --motion --labels
date: 2026-10-23 22:37 IST

### A current Painter (motion=true, threads=default(≤4)) — fixture `app`, labels 300, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.5 | 26.96 | 1.12 | 1.72 | 1.97 | 2.80 | 27 / 27 | nodes 1887 lines 43 arrows 0 labels 33 labels≤300 |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.5 | 26.96 | 5.23 | 10.16 | 10.52 | 10.79 | 43 / 43 | nodes 3000 lines 244 arrows 0 labels 0 labels≤300 |
| 2560x1440@1 → 2560x1440 | pan | 0.1 | 12.22 | 2.51 | 3.00 | 3.15 | 3.29 | 45 / 45 | nodes 2629 lines 53 arrows 50 labels 77 labels≤300 |
| 2560x1440@1 → 2560x1440 | zoom-out | 0.1 | 12.22 | 3.73 | 8.07 | 8.44 | 8.60 | 61 / 61 | nodes 3000 lines 346 arrows 0 labels 0 labels≤300 |
| 3840x2160@1 → 3840x2160 | pan | 0.1 | 25.66 | 6.32 | 6.62 | 6.65 | 6.85 | 86 / 86 | nodes 2764 lines 139 arrows 135 labels 74 labels≤300 |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.1 | 25.66 | 6.29 | 13.90 | 14.08 | 14.27 | 117 / 117 | nodes 3000 lines 346 arrows 0 labels 0 labels≤300 |
| 1920x1080@2 → 3840x2160 | pan | 0.1 | 31.17 | 7.34 | 8.46 | 8.76 | 9.22 | 87 / 117 | nodes 1884 lines 43 arrows 0 labels 33 labels≤300 |
| 1920x1080@2 → 3840x2160 | zoom-out | 0.1 | 31.17 | 12.07 | 16.98 | 18.55 | 18.67 | 110 / 117 | nodes 3000 lines 244 arrows 0 labels 0 labels≤300 |
| 2560x1440@2 → 5120x2880 | pan | 0.1 | 43.84 | 13.15 | 13.86 | 13.94 | 14.27 | 119 / 119 | nodes 2628 lines 53 arrows 50 labels 77 labels≤300 |
| 2560x1440@2 → 5120x2880 | zoom-out | 0.1 | 43.84 | 12.18 | 19.23 | 19.91 | 21.28 | 166 / 166 | nodes 3000 lines 346 arrows 0 labels 0 labels≤300 |
| 3840x2160@2 → 7680x4320 | pan | 0.1 | 75.47 | 25.54 | 26.54 | 26.63 | 27.02 | 208 / 208 | nodes 2764 lines 139 arrows 135 labels 74 labels≤300 |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.1 | 75.47 | 19.50 | 35.79 | 36.77 | 37.78 | 298 / 298 | nodes 3000 lines 346 arrows 0 labels 0 labels≤300 |
