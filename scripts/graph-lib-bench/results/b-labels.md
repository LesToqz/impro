## target/release/b_cairo --labels
date: 2026-10-23 23:00 IST

### B plain cairo (node aa Default) — fixture `app`, labels 300, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.1 | 28.85 | 6.27 | 10.13 | 10.44 | 10.85 | 30 / 31 | nodes 591 edges 0 label renders 466 (24 ms) labels≤300 |
| 1920x1080@1 → 1920x1080 | pan-fit | 0.1 | 28.85 | 11.72 | 16.52 | 17.80 | 17.97 | 30 / 32 | nodes 3000 edges 346 label renders 466 (24 ms) labels≤0 |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.1 | 28.85 | 8.89 | 15.92 | 16.41 | 19.98 | 51 / 51 | nodes 3000 edges 346 label renders 1690 (40 ms) labels≤300 |
| 2560x1440@1 → 2560x1440 | pan | 0.0 | 18.65 | 8.94 | 11.02 | 11.09 | 13.06 | 57 / 57 | nodes 911 edges 0 label renders 1175 (17 ms) labels≤300 |
| 2560x1440@1 → 2560x1440 | pan-fit | 0.0 | 18.65 | 10.10 | 11.01 | 11.20 | 13.82 | 57 / 57 | nodes 3000 edges 346 label renders 1175 (17 ms) labels≤0 |
| 2560x1440@1 → 2560x1440 | zoom-out | 0.0 | 18.65 | 9.76 | 16.33 | 17.71 | 18.43 | 60 / 60 | nodes 3000 edges 346 label renders 1955 (27 ms) labels≤300 |
| 3840x2160@1 → 3840x2160 | pan | 0.0 | 32.31 | 21.08 | 34.69 | 37.02 | 40.99 | 78 / 78 | nodes 1711 edges 3 label renders 952 (18 ms) labels≤300 |
| 3840x2160@1 → 3840x2160 | pan-fit | 0.0 | 32.31 | 19.03 | 31.21 | 31.64 | 31.89 | 78 / 78 | nodes 3000 edges 346 label renders 952 (18 ms) labels≤0 |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.0 | 32.31 | 20.07 | 35.29 | 39.65 | 41.13 | 79 / 79 | nodes 3000 edges 346 label renders 2018 (47 ms) labels≤300 |
| 1920x1080@2 → 3840x2160 | pan | 0.4 | 109.96 | 71.56 | 101.58 | 105.57 | 107.01 | 72 / 78 | nodes 591 edges 0 label renders 466 (50 ms) labels≤300 |
| 1920x1080@2 → 3840x2160 | pan-fit | 0.4 | 109.96 | 19.27 | 23.48 | 25.11 | 26.74 | 75 / 78 | nodes 3000 edges 346 label renders 466 (50 ms) labels≤0 |
| 1920x1080@2 → 3840x2160 | zoom-out | 0.4 | 109.96 | 36.96 | 89.33 | 92.78 | 104.50 | 128 / 128 | nodes 3000 edges 346 label renders 1690 (104 ms) labels≤300 |
| 2560x1440@2 → 5120x2880 | pan | 0.0 | 121.93 | 67.92 | 94.00 | 94.85 | 101.95 | 185 / 185 | nodes 911 edges 0 label renders 1175 (72 ms) labels≤300 |
| 2560x1440@2 → 5120x2880 | pan-fit | 0.0 | 121.93 | 24.33 | 27.60 | 28.52 | 28.85 | 185 / 185 | nodes 3000 edges 346 label renders 1175 (72 ms) labels≤0 |
| 2560x1440@2 → 5120x2880 | zoom-out | 0.0 | 121.93 | 55.04 | 75.35 | 78.92 | 79.13 | 185 / 185 | nodes 3000 edges 346 label renders 1955 (98 ms) labels≤300 |
| 3840x2160@2 → 7680x4320 | pan | 0.0 | 137.25 | 81.88 | 84.94 | 85.38 | 90.37 | 255 / 255 | nodes 1711 edges 3 label renders 952 (27 ms) labels≤300 |
| 3840x2160@2 → 7680x4320 | pan-fit | 0.0 | 137.25 | 36.16 | 40.03 | 41.39 | 51.50 | 255 / 255 | nodes 3000 edges 346 label renders 952 (27 ms) labels≤0 |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.0 | 137.25 | 73.06 | 91.02 | 91.61 | 94.72 | 255 / 255 | nodes 3000 edges 346 label renders 2018 (64 ms) labels≤300 |
