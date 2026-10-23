## target/release/c_skia --fixture packed --res 1920x1080@1,3840x2160@1,3840x2160@2
date: 2026-10-23 23:05 IST

### C tiny-skia (cached layer + integer translate: false) — fixture `packed`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.0 | 15.62 | 12.78 | 13.79 | 14.26 | 14.42 | 19 / 19 | scene renders 61 blits 0 label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | pan-fit | 0.0 | 15.62 | 9.36 | 10.41 | 10.44 | 10.52 | 18 / 19 | scene renders 121 blits 0 label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.0 | 15.62 | 8.80 | 12.00 | 12.28 | 12.29 | 18 / 19 | scene renders 181 blits 0 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan | 0.0 | 32.62 | 17.56 | 18.33 | 19.12 | 19.44 | 43 / 43 | scene renders 61 blits 0 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan-fit | 0.0 | 32.62 | 14.11 | 15.10 | 15.43 | 15.45 | 43 / 43 | scene renders 121 blits 0 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.0 | 32.62 | 10.61 | 19.76 | 21.38 | 21.53 | 43 / 43 | scene renders 181 blits 0 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan | 0.0 | 99.96 | 41.18 | 43.23 | 43.48 | 43.98 | 138 / 138 | scene renders 61 blits 0 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan-fit | 0.0 | 99.96 | 34.39 | 35.40 | 35.77 | 37.89 | 138 / 138 | scene renders 121 blits 0 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.0 | 99.96 | 23.66 | 50.36 | 54.81 | 55.03 | 140 / 140 | scene renders 181 blits 0 label renders 0 (0 ms) |
