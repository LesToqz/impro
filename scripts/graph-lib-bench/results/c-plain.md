## target/release/c_skia
date: 2026-10-23 22:40 IST

### C tiny-skia (cached layer + integer translate: false) — fixture `app`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.0 | 14.37 | 12.80 | 14.28 | 14.34 | 14.41 | 19 / 19 | scene renders 61 blits 0 label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.0 | 14.37 | 9.03 | 12.29 | 12.96 | 13.27 | 18 / 19 | scene renders 121 blits 0 label renders 0 (0 ms) |
| 2560x1440@1 → 2560x1440 | pan | 0.0 | 21.34 | 13.05 | 14.06 | 14.83 | 15.04 | 25 / 25 | scene renders 61 blits 0 label renders 0 (0 ms) |
| 2560x1440@1 → 2560x1440 | zoom-out | 0.0 | 21.34 | 8.69 | 14.46 | 15.43 | 16.15 | 25 / 25 | scene renders 121 blits 0 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan | 0.0 | 28.86 | 16.50 | 17.69 | 18.10 | 18.42 | 43 / 43 | scene renders 61 blits 0 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.0 | 28.86 | 10.46 | 19.22 | 20.97 | 21.25 | 43 / 43 | scene renders 121 blits 0 label renders 0 (0 ms) |
| 1920x1080@2 → 3840x2160 | pan | 0.2 | 31.72 | 25.36 | 27.52 | 27.65 | 28.09 | 43 / 43 | scene renders 61 blits 0 label renders 0 (0 ms) |
| 1920x1080@2 → 3840x2160 | zoom-out | 0.2 | 31.72 | 15.55 | 26.54 | 27.20 | 27.36 | 44 / 44 | scene renders 121 blits 0 label renders 0 (0 ms) |
| 2560x1440@2 → 5120x2880 | pan | 0.0 | 51.08 | 28.93 | 31.42 | 31.90 | 31.95 | 101 / 101 | scene renders 61 blits 0 label renders 0 (0 ms) |
| 2560x1440@2 → 5120x2880 | zoom-out | 0.0 | 51.08 | 18.45 | 33.75 | 35.34 | 35.44 | 101 / 101 | scene renders 121 blits 0 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan | 0.0 | 82.15 | 39.09 | 40.86 | 42.05 | 49.34 | 171 / 171 | scene renders 61 blits 0 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.0 | 82.15 | 23.31 | 46.53 | 49.99 | 50.55 | 171 / 171 | scene renders 121 blits 0 label renders 0 (0 ms) |
