## target/release/b_cairo --labels
date: 2026-10-23 22:08 IST

### B plain cairo (node aa Default) — fixture `app`, labels 300, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.1 | 43.95 | 14.21 | 21.57 | 22.40 | 22.63 | 50 / 50 | nodes 156 edges 43 label renders 2029 (38 ms) labels≤300 |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.1 | 43.95 | 12.91 | 20.88 | 21.66 | 21.80 | 60 / 60 | nodes 3000 edges 346 label renders 2459 (45 ms) labels≤300 |
| 2560x1440@1 → 2560x1440 | pan | 0.0 | 29.35 | 18.71 | 19.97 | 20.06 | 20.48 | 66 / 66 | nodes 860 edges 53 label renders 2023 (22 ms) labels≤300 |
| 2560x1440@1 → 2560x1440 | zoom-out | 0.0 | 29.35 | 13.95 | 21.05 | 21.45 | 22.05 | 66 / 66 | nodes 3000 edges 346 label renders 2348 (27 ms) labels≤300 |
| 3840x2160@1 → 3840x2160 | pan | 0.0 | 39.41 | 22.50 | 23.86 | 23.88 | 24.21 | 84 / 84 | nodes 2511 edges 94 label renders 955 (13 ms) labels≤300 |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.0 | 39.41 | 15.52 | 26.55 | 27.34 | 27.75 | 84 / 84 | nodes 3000 edges 346 label renders 1186 (17 ms) labels≤300 |
| 1920x1080@2 → 3840x2160 | pan | 0.6 | 93.38 | 69.91 | 76.63 | 80.65 | 81.82 | 136 / 136 | nodes 156 edges 43 label renders 2029 (59 ms) labels≤300 |
| 1920x1080@2 → 3840x2160 | zoom-out | 0.6 | 93.38 | 65.09 | 79.85 | 80.85 | 81.65 | 162 / 162 | nodes 3000 edges 346 label renders 2459 (74 ms) labels≤300 |
| 2560x1440@2 → 5120x2880 | pan | 0.0 | 100.34 | 73.44 | 78.91 | 79.94 | 84.13 | 219 / 219 | nodes 860 edges 53 label renders 2023 (52 ms) labels≤300 |
| 2560x1440@2 → 5120x2880 | zoom-out | 0.0 | 100.34 | 66.51 | 86.98 | 90.50 | 92.48 | 219 / 219 | nodes 3000 edges 346 label renders 2348 (64 ms) labels≤300 |
| 3840x2160@2 → 7680x4320 | pan | 0.0 | 139.74 | 90.56 | 95.23 | 95.54 | 96.06 | 289 / 289 | nodes 2511 edges 94 label renders 955 (24 ms) labels≤300 |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.0 | 139.74 | 70.67 | 99.08 | 100.39 | 101.63 | 289 / 289 | nodes 3000 edges 346 label renders 1186 (31 ms) labels≤300 |
