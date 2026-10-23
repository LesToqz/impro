## target/release/c_skia --labels
date: 2026-10-23 22:41 IST

### C tiny-skia (cached layer + integer translate: false) — fixture `app`, labels 300, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.0 | 37.26 | 16.09 | 19.20 | 19.60 | 20.01 | 48 / 48 | scene renders 61 blits 0 label renders 1947 (44 ms) labels≤300 |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.0 | 37.26 | 12.52 | 17.58 | 17.73 | 18.62 | 52 / 53 | scene renders 121 blits 0 label renders 2340 (52 ms) labels≤300 |
| 2560x1440@1 → 2560x1440 | pan | 0.0 | 25.75 | 16.08 | 22.08 | 22.39 | 23.47 | 58 / 58 | scene renders 61 blits 0 label renders 393 (4 ms) labels≤300 |
| 2560x1440@1 → 2560x1440 | zoom-out | 0.0 | 25.75 | 11.67 | 19.36 | 19.88 | 20.94 | 58 / 58 | scene renders 121 blits 0 label renders 1446 (17 ms) labels≤300 |
| 3840x2160@1 → 3840x2160 | pan | 0.0 | 63.36 | 20.48 | 23.81 | 24.67 | 30.94 | 76 / 76 | scene renders 61 blits 0 label renders 389 (7 ms) labels≤300 |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.0 | 63.36 | 13.25 | 23.80 | 25.43 | 26.83 | 76 / 76 | scene renders 121 blits 0 label renders 929 (14 ms) labels≤300 |
| 1920x1080@2 → 3840x2160 | pan | 0.4 | 52.31 | 38.78 | 51.77 | 53.27 | 55.06 | 131 / 131 | scene renders 61 blits 0 label renders 1947 (61 ms) labels≤300 |
| 1920x1080@2 → 3840x2160 | zoom-out | 0.4 | 52.31 | 26.92 | 43.51 | 46.00 | 47.07 | 150 / 150 | scene renders 121 blits 0 label renders 2340 (74 ms) labels≤300 |
| 2560x1440@2 → 5120x2880 | pan | 4.2 | 48.46 | 42.84 | 51.46 | 57.18 | 57.35 | 150 / 150 | scene renders 61 blits 0 label renders 393 (6 ms) labels≤300 |
| 2560x1440@2 → 5120x2880 | zoom-out | 4.2 | 48.46 | 27.27 | 52.39 | 54.36 | 55.47 | 150 / 150 | scene renders 121 blits 0 label renders 1446 (29 ms) labels≤300 |
| 3840x2160@2 → 7680x4320 | pan | 0.0 | 104.10 | 50.75 | 54.00 | 54.95 | 55.45 | 213 / 213 | scene renders 61 blits 0 label renders 389 (9 ms) labels≤300 |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.0 | 104.10 | 32.69 | 56.75 | 61.01 | 63.92 | 213 / 213 | scene renders 121 blits 0 label renders 929 (19 ms) labels≤300 |
