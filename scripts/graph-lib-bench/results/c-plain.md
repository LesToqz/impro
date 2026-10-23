## target/release/c_skia
date: 2026-10-23 23:04 IST

### C tiny-skia (cached layer + integer translate: false) — fixture `app`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.0 | 8.11 | 3.24 | 3.78 | 3.87 | 3.98 | 17 / 17 | scene renders 61 blits 0 label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | pan-fit | 0.0 | 8.11 | 7.98 | 8.85 | 9.04 | 9.35 | 18 / 19 | scene renders 121 blits 0 label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.0 | 8.11 | 7.38 | 9.72 | 9.76 | 10.59 | 19 / 19 | scene renders 181 blits 0 label renders 0 (0 ms) |
| 2560x1440@1 → 2560x1440 | pan | 0.0 | 12.90 | 6.50 | 9.78 | 10.37 | 10.68 | 25 / 25 | scene renders 61 blits 0 label renders 0 (0 ms) |
| 2560x1440@1 → 2560x1440 | pan-fit | 0.0 | 12.90 | 8.30 | 9.77 | 10.18 | 10.92 | 25 / 25 | scene renders 121 blits 0 label renders 0 (0 ms) |
| 2560x1440@1 → 2560x1440 | zoom-out | 0.0 | 12.90 | 7.71 | 10.42 | 10.85 | 11.27 | 25 / 25 | scene renders 181 blits 0 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan | 0.0 | 27.63 | 13.55 | 15.66 | 15.97 | 16.38 | 43 / 43 | scene renders 61 blits 0 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan-fit | 0.0 | 27.63 | 11.03 | 12.02 | 12.14 | 12.47 | 43 / 43 | scene renders 121 blits 0 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.0 | 27.63 | 11.13 | 15.18 | 15.44 | 15.46 | 43 / 43 | scene renders 181 blits 0 label renders 0 (0 ms) |
| 1920x1080@2 → 3840x2160 | pan | 0.1 | 22.62 | 8.31 | 8.91 | 8.99 | 9.00 | 41 / 43 | scene renders 61 blits 0 label renders 0 (0 ms) |
| 1920x1080@2 → 3840x2160 | pan-fit | 0.1 | 22.62 | 13.03 | 14.09 | 14.15 | 14.30 | 43 / 43 | scene renders 121 blits 0 label renders 0 (0 ms) |
| 1920x1080@2 → 3840x2160 | zoom-out | 0.1 | 22.62 | 13.47 | 18.72 | 19.34 | 19.90 | 43 / 43 | scene renders 181 blits 0 label renders 0 (0 ms) |
| 2560x1440@2 → 5120x2880 | pan | 0.0 | 41.57 | 16.42 | 17.53 | 17.60 | 19.09 | 99 / 99 | scene renders 61 blits 0 label renders 0 (0 ms) |
| 2560x1440@2 → 5120x2880 | pan-fit | 0.0 | 41.57 | 17.41 | 18.58 | 19.22 | 19.23 | 99 / 99 | scene renders 121 blits 0 label renders 0 (0 ms) |
| 2560x1440@2 → 5120x2880 | zoom-out | 0.0 | 41.57 | 17.76 | 24.53 | 24.71 | 25.33 | 99 / 99 | scene renders 181 blits 0 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan | 0.0 | 134.75 | 32.89 | 34.43 | 34.56 | 34.82 | 170 / 170 | scene renders 61 blits 0 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan-fit | 0.0 | 134.75 | 27.73 | 28.62 | 28.78 | 29.01 | 170 / 170 | scene renders 121 blits 0 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.0 | 134.75 | 27.05 | 34.69 | 35.50 | 37.20 | 170 / 170 | scene renders 181 blits 0 label renders 0 (0 ms) |
