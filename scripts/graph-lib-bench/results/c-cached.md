## target/release/c_skia --cached
date: 2026-10-23 22:40 IST

### C tiny-skia (cached layer + integer translate: true) — fixture `app`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.0 | 38.43 | 7.41 | 9.73 | 25.02 | 25.38 | 41 / 42 | scene renders 3 blits 58 label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.0 | 38.43 | 17.61 | 26.69 | 28.96 | 29.76 | 42 / 43 | scene renders 63 blits 58 label renders 0 (0 ms) |
| 2560x1440@1 → 2560x1440 | pan | 0.3 | 49.35 | 13.18 | 14.09 | 32.09 | 33.64 | 59 / 59 | scene renders 3 blits 58 label renders 0 (0 ms) |
| 2560x1440@1 → 2560x1440 | zoom-out | 0.3 | 49.35 | 24.58 | 33.31 | 34.51 | 34.95 | 59 / 59 | scene renders 63 blits 58 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan | 0.0 | 86.19 | 29.44 | 31.68 | 51.07 | 51.47 | 116 / 116 | scene renders 3 blits 58 label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.0 | 86.19 | 42.85 | 55.71 | 58.12 | 58.58 | 116 / 116 | scene renders 63 blits 58 label renders 0 (0 ms) |
| 1920x1080@2 → 3840x2160 | pan | 1.1 | 105.79 | 29.73 | 31.67 | 64.61 | 65.08 | 137 / 137 | scene renders 3 blits 58 label renders 0 (0 ms) |
| 1920x1080@2 → 3840x2160 | zoom-out | 1.1 | 105.79 | 50.72 | 74.73 | 76.66 | 77.28 | 139 / 139 | scene renders 63 blits 58 label renders 0 (0 ms) |
| 2560x1440@2 → 5120x2880 | pan | 0.1 | 168.09 | 52.88 | 54.83 | 93.81 | 94.36 | 236 / 236 | scene renders 3 blits 58 label renders 0 (0 ms) |
| 2560x1440@2 → 5120x2880 | zoom-out | 0.1 | 168.09 | 77.66 | 101.68 | 103.47 | 106.06 | 236 / 236 | scene renders 63 blits 58 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan | 0.0 | 331.46 | 122.30 | 135.39 | 172.70 | 182.70 | 408 / 408 | scene renders 3 blits 58 label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.0 | 331.46 | 155.03 | 183.50 | 184.66 | 186.00 | 408 / 408 | scene renders 63 blits 58 label renders 0 (0 ms) |
