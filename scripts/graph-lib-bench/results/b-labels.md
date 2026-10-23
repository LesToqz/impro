## target/release/b_cairo --labels
date: 2026-10-23 22:38 IST

### B plain cairo (node aa Default) — fixture `app`, labels 300, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.2 | 39.21 | 20.42 | 22.33 | 23.10 | 25.31 | 53 / 53 | nodes 1883 edges 43 label renders 1947 (38 ms) labels≤300 |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.2 | 39.21 | 12.81 | 21.02 | 21.63 | 22.51 | 59 / 59 | nodes 3000 edges 346 label renders 2340 (44 ms) labels≤300 |
| 2560x1440@1 → 2560x1440 | pan | 0.0 | 27.93 | 19.20 | 22.33 | 23.90 | 26.47 | 65 / 65 | nodes 2627 edges 53 label renders 393 (4 ms) labels≤300 |
| 2560x1440@1 → 2560x1440 | zoom-out | 0.0 | 27.93 | 14.30 | 22.00 | 23.07 | 23.45 | 65 / 65 | nodes 3000 edges 346 label renders 1446 (15 ms) labels≤300 |
| 3840x2160@1 → 3840x2160 | pan | 0.0 | 41.02 | 24.04 | 26.80 | 26.85 | 29.97 | 82 / 82 | nodes 2764 edges 139 label renders 389 (6 ms) labels≤300 |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.0 | 41.02 | 15.84 | 26.88 | 30.07 | 30.16 | 82 / 82 | nodes 3000 edges 346 label renders 929 (12 ms) labels≤300 |
| 1920x1080@2 → 3840x2160 | pan | 0.5 | 91.07 | 73.08 | 78.77 | 79.19 | 79.53 | 137 / 137 | nodes 1883 edges 43 label renders 1947 (58 ms) labels≤300 |
| 1920x1080@2 → 3840x2160 | zoom-out | 0.5 | 91.07 | 67.57 | 79.96 | 81.58 | 81.77 | 157 / 157 | nodes 3000 edges 346 label renders 2340 (72 ms) labels≤300 |
| 2560x1440@2 → 5120x2880 | pan | 0.0 | 104.17 | 75.25 | 77.46 | 77.73 | 79.23 | 213 / 213 | nodes 2627 edges 53 label renders 393 (8 ms) labels≤300 |
| 2560x1440@2 → 5120x2880 | zoom-out | 0.0 | 104.17 | 66.06 | 90.08 | 91.40 | 92.15 | 213 / 213 | nodes 3000 edges 346 label renders 1446 (35 ms) labels≤300 |
| 3840x2160@2 → 7680x4320 | pan | 0.0 | 142.59 | 93.89 | 96.27 | 97.61 | 98.61 | 283 / 283 | nodes 2764 edges 139 label renders 389 (11 ms) labels≤300 |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.0 | 142.59 | 70.70 | 98.48 | 102.10 | 105.97 | 283 / 283 | nodes 3000 edges 346 label renders 929 (23 ms) labels≤300 |
