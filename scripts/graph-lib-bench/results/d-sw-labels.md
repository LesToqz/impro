## target/release/d_gl --labels --readback
date: 2026-10-23 22:58 IST

### D raw GL/glow instanced (readback=true, finish=true) — fixture `app`, labels 300, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 56.3 | 64.38 | 3.78 | 4.99 | 5.24 | 5.34 | 146 / 146 | label renders 466 (27 ms) labels≤300 |
| 1920x1080@1 → 1920x1080 | pan-fit | 56.3 | 64.38 | 3.79 | 4.21 | 4.33 | 4.46 | 148 / 148 | label renders 466 (27 ms) labels≤0 |
| 1920x1080@1 → 1920x1080 | zoom-out | 56.3 | 64.38 | 3.34 | 6.75 | 7.35 | 7.45 | 155 / 155 | label renders 1690 (50 ms) labels≤300 |
| 2560x1440@1 → 2560x1440 | pan | 31.0 | 26.01 | 6.39 | 7.33 | 8.07 | 9.22 | 174 / 174 | label renders 1175 (32 ms) labels≤300 |
| 2560x1440@1 → 2560x1440 | pan-fit | 31.0 | 26.01 | 4.44 | 4.81 | 4.87 | 5.31 | 175 / 175 | label renders 1175 (32 ms) labels≤0 |
| 2560x1440@1 → 2560x1440 | zoom-out | 31.0 | 26.01 | 4.97 | 7.76 | 8.48 | 8.52 | 180 / 180 | label renders 1955 (51 ms) labels≤300 |
| 3840x2160@1 → 3840x2160 | pan | 22.5 | 36.13 | 10.08 | 10.86 | 11.64 | 12.86 | 220 / 220 | label renders 952 (22 ms) labels≤300 |
| 3840x2160@1 → 3840x2160 | pan-fit | 22.5 | 36.13 | 8.56 | 8.99 | 9.39 | 9.40 | 220 / 220 | label renders 952 (22 ms) labels≤0 |
| 3840x2160@1 → 3840x2160 | zoom-out | 22.5 | 36.13 | 9.05 | 11.73 | 11.86 | 13.31 | 220 / 220 | label renders 2018 (43 ms) labels≤300 |
| 1920x1080@2 → 3840x2160 | pan | 19.9 | 43.26 | 10.11 | 11.66 | 11.76 | 12.68 | 223 / 223 | label renders 466 (22 ms) labels≤300 |
| 1920x1080@2 → 3840x2160 | pan-fit | 19.9 | 43.26 | 8.63 | 9.08 | 9.36 | 9.67 | 223 / 223 | label renders 466 (22 ms) labels≤0 |
| 1920x1080@2 → 3840x2160 | zoom-out | 19.9 | 43.26 | 8.90 | 12.78 | 12.90 | 13.78 | 248 / 248 | label renders 1690 (55 ms) labels≤300 |
| 2560x1440@2 → 5120x2880 | pan | 41.5 | 51.27 | 14.27 | 17.31 | 17.73 | 18.59 | 290 / 290 | label renders 1175 (26 ms) labels≤300 |
| 2560x1440@2 → 5120x2880 | pan-fit | 41.5 | 51.27 | 13.83 | 14.81 | 15.27 | 15.84 | 290 / 290 | label renders 1175 (26 ms) labels≤0 |
| 2560x1440@2 → 5120x2880 | zoom-out | 41.5 | 51.27 | 14.14 | 16.18 | 16.51 | 17.06 | 307 / 307 | label renders 1955 (47 ms) labels≤300 |
| 3840x2160@2 → 7680x4320 | pan | 70.9 | 92.92 | 28.40 | 30.86 | 32.46 | 33.69 | 449 / 449 | label renders 952 (21 ms) labels≤300 |
| 3840x2160@2 → 7680x4320 | pan-fit | 70.9 | 92.92 | 27.72 | 28.66 | 28.90 | 28.96 | 449 / 449 | label renders 952 (21 ms) labels≤0 |
| 3840x2160@2 → 7680x4320 | zoom-out | 70.9 | 92.92 | 28.31 | 30.03 | 30.38 | 31.44 | 452 / 452 | label renders 2018 (41 ms) labels≤300 |
