## target/release/c_skia --cached
date: 2026-10-23 23:04 IST

### C tiny-skia (cached layer + integer translate: true) — fixture `app`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.0 | 35.31 | 7.43 | 8.46 | 20.58 | 20.73 | 41 / 42 | scene renders 3 blits 58 label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | pan-fit | 0.0 | 35.31 | 7.34 | 8.27 | 17.16 | 17.58 | 42 / 42 | scene renders 5 blits 116 label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.0 | 35.31 | 18.39 | 23.00 | 24.71 | 26.19 | 42 / 43 | scene renders 65 blits 116 label renders 0 (0 ms) |
| 2560x1440@1 → 2560x1440 | pan | 0.4 | 49.12 | 13.17 | 14.19 | 29.61 | 30.07 | 59 / 59 | scene renders 3 blits 58 label renders 0 (0 ms) |
| 2560x1440@1 → 2560x1440 | pan-fit | 0.4 | 49.12 | 13.14 | 14.18 | 24.04 | 24.47 | 59 / 59 | scene renders 5 blits 116 label renders 0 (0 ms) |
| 2560x1440@1 → 2560x1440 | zoom-out | 0.4 | 49.12 | 24.69 | 29.86 | 30.03 | 30.54 | 59 / 59 | scene renders 65 blits 116 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan | 0.0 | 95.19 | 29.25 | 31.37 | 51.32 | 52.11 | 116 / 116 | scene renders 3 blits 58 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan-fit | 0.0 | 95.19 | 29.33 | 30.54 | 43.15 | 43.15 | 116 / 116 | scene renders 5 blits 116 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.0 | 95.19 | 45.37 | 50.16 | 50.96 | 50.99 | 116 / 116 | scene renders 65 blits 116 label renders 0 (0 ms) |
| 1920x1080@2 → 3840x2160 | pan | 1.4 | 105.98 | 29.95 | 31.72 | 58.04 | 58.82 | 137 / 137 | scene renders 3 blits 58 label renders 0 (0 ms) |
| 1920x1080@2 → 3840x2160 | pan-fit | 1.4 | 105.98 | 29.70 | 31.12 | 48.47 | 49.36 | 138 / 138 | scene renders 5 blits 116 label renders 0 (0 ms) |
| 1920x1080@2 → 3840x2160 | zoom-out | 1.4 | 105.98 | 51.97 | 62.99 | 64.87 | 68.78 | 138 / 138 | scene renders 65 blits 116 label renders 0 (0 ms) |
| 2560x1440@2 → 5120x2880 | pan | 0.0 | 226.03 | 53.29 | 55.07 | 89.40 | 89.61 | 234 / 234 | scene renders 3 blits 58 label renders 0 (0 ms) |
| 2560x1440@2 → 5120x2880 | pan-fit | 0.0 | 226.03 | 53.09 | 55.04 | 77.10 | 78.22 | 234 / 234 | scene renders 5 blits 116 label renders 0 (0 ms) |
| 2560x1440@2 → 5120x2880 | zoom-out | 0.0 | 226.03 | 79.89 | 91.14 | 92.43 | 93.95 | 234 / 234 | scene renders 65 blits 116 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan | 0.0 | 405.28 | 118.28 | 120.82 | 167.42 | 168.75 | 406 / 406 | scene renders 3 blits 58 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan-fit | 0.0 | 405.28 | 119.39 | 124.01 | 151.43 | 158.89 | 406 / 406 | scene renders 5 blits 116 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.0 | 405.28 | 157.01 | 170.07 | 172.21 | 172.82 | 406 / 406 | scene renders 65 blits 116 label renders 0 (0 ms) |
