## target/release/a_current --motion --labels
date: 2026-10-23 22:07 IST

### A current Painter (motion=true, threads=default(≤4)) — fixture `app`, labels 300, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.4 | 32.01 | 2.43 | 3.44 | 3.77 | 4.04 | 27 / 27 | nodes 158 lines 43 arrows 40 labels 51 labels≤300 |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.4 | 32.01 | 5.91 | 10.59 | 11.61 | 12.54 | 43 / 43 | nodes 3000 lines 244 arrows 0 labels 0 labels≤300 |
| 2560x1440@1 → 2560x1440 | pan | 0.1 | 14.64 | 2.04 | 3.05 | 3.14 | 3.46 | 45 / 45 | nodes 860 lines 53 arrows 50 labels 63 labels≤300 |
| 2560x1440@1 → 2560x1440 | zoom-out | 0.1 | 14.64 | 3.81 | 8.13 | 8.48 | 8.88 | 61 / 61 | nodes 3000 lines 346 arrows 0 labels 0 labels≤300 |
| 3840x2160@1 → 3840x2160 | pan | 0.1 | 26.52 | 5.75 | 6.49 | 6.58 | 6.66 | 86 / 86 | nodes 2512 lines 94 arrows 90 labels 71 labels≤300 |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.1 | 26.52 | 5.82 | 13.28 | 15.68 | 17.18 | 117 / 117 | nodes 3000 lines 346 arrows 0 labels 0 labels≤300 |
| 1920x1080@2 → 3840x2160 | pan | 0.2 | 26.97 | 6.35 | 7.24 | 7.62 | 8.48 | 86 / 117 | nodes 156 lines 43 arrows 40 labels 51 labels≤300 |
| 1920x1080@2 → 3840x2160 | zoom-out | 0.2 | 26.97 | 12.07 | 18.06 | 18.90 | 20.05 | 112 / 117 | nodes 3000 lines 244 arrows 0 labels 0 labels≤300 |
| 2560x1440@2 → 5120x2880 | pan | 0.1 | 44.29 | 11.10 | 13.10 | 13.38 | 13.41 | 120 / 120 | nodes 860 lines 53 arrows 50 labels 63 labels≤300 |
| 2560x1440@2 → 5120x2880 | zoom-out | 0.1 | 44.29 | 11.85 | 19.01 | 19.24 | 19.35 | 167 / 167 | nodes 3000 lines 346 arrows 0 labels 0 labels≤300 |
| 3840x2160@2 → 7680x4320 | pan | 0.1 | 75.36 | 23.43 | 25.30 | 25.55 | 25.62 | 210 / 210 | nodes 2511 lines 94 arrows 90 labels 71 labels≤300 |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.1 | 75.36 | 19.90 | 34.75 | 35.29 | 35.39 | 300 / 300 | nodes 3000 lines 346 arrows 0 labels 0 labels≤300 |
