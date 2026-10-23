## target/release/a_current --motion --labels
date: 2026-10-23 22:59 IST

### A current Painter (motion=true, threads=default(≤4)) — fixture `app`, labels 300, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.6 | 12.98 | 0.53 | 0.90 | 0.94 | 1.02 | 25 / 25 | nodes 592 lines 0 arrows 0 labels 92 labels≤300 |
| 1920x1080@1 → 1920x1080 | pan-fit | 0.6 | 12.98 | 0.62 | 0.70 | 0.72 | 1.61 | 26 / 26 | nodes 3000 lines 346 arrows 0 labels 0 labels≤0 |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.6 | 12.98 | 3.37 | 9.84 | 12.50 | 12.79 | 44 / 44 | nodes 3000 lines 346 arrows 0 labels 0 labels≤300 |
| 2560x1440@1 → 2560x1440 | pan | 0.2 | 7.77 | 1.03 | 1.52 | 3.35 | 3.45 | 38 / 43 | nodes 912 lines 0 arrows 0 labels 93 labels≤300 |
| 2560x1440@1 → 2560x1440 | pan-fit | 0.2 | 7.77 | 1.23 | 1.49 | 1.70 | 3.44 | 41 / 43 | nodes 3000 lines 346 arrows 0 labels 0 labels≤0 |
| 2560x1440@1 → 2560x1440 | zoom-out | 0.2 | 7.77 | 2.14 | 7.34 | 10.60 | 11.68 | 59 / 59 | nodes 3000 lines 346 arrows 0 labels 0 labels≤300 |
| 3840x2160@1 → 3840x2160 | pan | 0.1 | 20.75 | 3.08 | 3.59 | 3.65 | 3.79 | 77 / 77 | nodes 1714 lines 3 arrows 3 labels 93 labels≤300 |
| 3840x2160@1 → 3840x2160 | pan-fit | 0.1 | 20.75 | 4.20 | 4.75 | 4.92 | 20.45 | 82 / 82 | nodes 3000 lines 346 arrows 210 labels 96 labels≤0 |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.1 | 20.75 | 4.39 | 12.75 | 15.46 | 15.76 | 118 / 118 | nodes 3000 lines 346 arrows 0 labels 0 labels≤300 |
| 1920x1080@2 → 3840x2160 | pan | 0.1 | 16.65 | 3.40 | 3.89 | 4.00 | 4.02 | 78 / 117 | nodes 591 lines 0 arrows 0 labels 92 labels≤300 |
| 1920x1080@2 → 3840x2160 | pan-fit | 0.1 | 16.65 | 3.96 | 4.48 | 5.19 | 6.88 | 82 / 117 | nodes 3000 lines 346 arrows 0 labels 0 labels≤0 |
| 1920x1080@2 → 3840x2160 | zoom-out | 0.1 | 16.65 | 8.32 | 15.85 | 16.95 | 20.70 | 123 / 123 | nodes 3000 lines 346 arrows 0 labels 0 labels≤300 |
| 2560x1440@2 → 5120x2880 | pan | 0.1 | 28.08 | 7.04 | 7.52 | 8.56 | 9.27 | 103 / 123 | nodes 912 lines 0 arrows 0 labels 93 labels≤300 |
| 2560x1440@2 → 5120x2880 | pan-fit | 0.1 | 28.08 | 7.82 | 8.55 | 8.66 | 12.41 | 111 / 123 | nodes 3000 lines 346 arrows 0 labels 0 labels≤0 |
| 2560x1440@2 → 5120x2880 | zoom-out | 0.1 | 28.08 | 9.98 | 18.06 | 23.06 | 23.49 | 168 / 168 | nodes 3000 lines 346 arrows 0 labels 0 labels≤300 |
| 3840x2160@2 → 7680x4320 | pan | 0.1 | 70.35 | 14.62 | 15.55 | 15.58 | 15.66 | 190 / 190 | nodes 1713 lines 3 arrows 3 labels 93 labels≤300 |
| 3840x2160@2 → 7680x4320 | pan-fit | 0.1 | 70.35 | 16.47 | 17.40 | 17.80 | 38.52 | 203 / 203 | nodes 3000 lines 346 arrows 210 labels 96 labels≤0 |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.1 | 70.35 | 17.46 | 30.71 | 32.58 | 35.14 | 291 / 291 | nodes 3000 lines 346 arrows 0 labels 0 labels≤300 |
