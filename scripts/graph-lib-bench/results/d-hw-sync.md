## target/release/d_gl --sync1px
date: 2026-10-23 22:57 IST

### D raw GL/glow instanced (readback=false, finish=true) — fixture `app`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 34.7 | 6.80 | 2.36 | 2.51 | 2.54 | 2.67 | 82 / 82 | label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | pan-fit | 34.7 | 6.80 | 2.34 | 2.54 | 2.66 | 2.80 | 82 / 82 | label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | zoom-out | 34.7 | 6.80 | 2.33 | 2.49 | 2.85 | 3.74 | 82 / 82 | label renders 0 (0 ms) |
| 2560x1440@1 → 2560x1440 | pan | 9.2 | 8.81 | 4.06 | 4.20 | 4.25 | 4.50 | 82 / 82 | label renders 0 (0 ms) |
| 2560x1440@1 → 2560x1440 | pan-fit | 9.2 | 8.81 | 4.04 | 4.22 | 4.46 | 4.53 | 82 / 82 | label renders 0 (0 ms) |
| 2560x1440@1 → 2560x1440 | zoom-out | 9.2 | 8.81 | 4.04 | 4.44 | 4.46 | 4.56 | 82 / 82 | label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan | 9.8 | 32.42 | 7.99 | 8.41 | 8.47 | 8.64 | 83 / 83 | label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan-fit | 9.8 | 32.42 | 7.96 | 8.41 | 8.48 | 8.70 | 83 / 83 | label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | zoom-out | 9.8 | 32.42 | 7.97 | 8.56 | 8.69 | 8.70 | 83 / 83 | label renders 0 (0 ms) |
| 1920x1080@2 → 3840x2160 | pan | 8.4 | 18.00 | 7.97 | 8.45 | 8.53 | 8.60 | 83 / 83 | label renders 0 (0 ms) |
| 1920x1080@2 → 3840x2160 | pan-fit | 8.4 | 18.00 | 8.01 | 8.36 | 8.55 | 8.71 | 83 / 83 | label renders 0 (0 ms) |
| 1920x1080@2 → 3840x2160 | zoom-out | 8.4 | 18.00 | 7.95 | 8.42 | 8.46 | 8.61 | 83 / 83 | label renders 0 (0 ms) |
| 2560x1440@2 → 5120x2880 | pan | 10.8 | 27.94 | 13.31 | 13.85 | 14.15 | 14.15 | 83 / 83 | label renders 0 (0 ms) |
| 2560x1440@2 → 5120x2880 | pan-fit | 10.8 | 27.94 | 13.31 | 13.86 | 14.10 | 14.24 | 83 / 83 | label renders 0 (0 ms) |
| 2560x1440@2 → 5120x2880 | zoom-out | 10.8 | 27.94 | 13.67 | 14.52 | 14.72 | 14.94 | 83 / 83 | label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan | 10.7 | 55.26 | 28.60 | 29.62 | 31.01 | 31.62 | 83 / 83 | label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan-fit | 10.7 | 55.26 | 28.55 | 29.27 | 29.72 | 29.85 | 83 / 83 | label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | zoom-out | 10.7 | 55.26 | 28.65 | 30.52 | 31.22 | 31.53 | 83 / 83 | label renders 0 (0 ms) |
