## target/release/d_gl --labels --readback
date: 2026-10-23 22:54 IST

### D raw GL/glow instanced (readback=true, finish=true) — fixture `app`, labels 300, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 45.1 | 53.36 | 4.29 | 4.77 | 4.96 | 7.25 | 99 / 99 | label renders 466 (26 ms) labels≤300 |
| 1920x1080@1 → 1920x1080 | pan-fit | 45.1 | 53.36 | 4.00 | 4.34 | 4.44 | 4.48 | 99 / 99 | label renders 466 (26 ms) labels≤0 |
| 1920x1080@1 → 1920x1080 | zoom-out | 45.1 | 53.36 | 4.18 | 6.24 | 6.64 | 8.09 | 107 / 107 | label renders 1690 (47 ms) labels≤300 |
| 2560x1440@1 → 2560x1440 | pan | 9.5 | 31.02 | 6.58 | 7.83 | 8.73 | 9.12 | 112 / 112 | label renders 1175 (21 ms) labels≤300 |
| 2560x1440@1 → 2560x1440 | pan-fit | 9.5 | 31.02 | 6.19 | 6.58 | 7.24 | 8.26 | 112 / 112 | label renders 1175 (21 ms) labels≤0 |
| 2560x1440@1 → 2560x1440 | zoom-out | 9.5 | 31.02 | 6.46 | 7.72 | 8.31 | 8.50 | 115 / 115 | label renders 1955 (35 ms) labels≤300 |
| 3840x2160@1 → 3840x2160 | pan | 11.1 | 49.30 | 12.68 | 13.61 | 13.71 | 18.25 | 133 / 133 | label renders 952 (17 ms) labels≤300 |
| 3840x2160@1 → 3840x2160 | pan-fit | 11.1 | 49.30 | 12.19 | 13.00 | 13.44 | 14.51 | 133 / 133 | label renders 952 (17 ms) labels≤0 |
| 3840x2160@1 → 3840x2160 | zoom-out | 11.1 | 49.30 | 12.74 | 13.78 | 13.96 | 14.83 | 133 / 133 | label renders 2018 (33 ms) labels≤300 |
| 1920x1080@2 → 3840x2160 | pan | 11.6 | 56.70 | 12.31 | 13.19 | 13.32 | 14.49 | 132 / 133 | label renders 466 (22 ms) labels≤300 |
| 1920x1080@2 → 3840x2160 | pan-fit | 11.6 | 56.70 | 12.05 | 12.93 | 13.18 | 14.77 | 132 / 133 | label renders 466 (22 ms) labels≤0 |
| 1920x1080@2 → 3840x2160 | zoom-out | 11.6 | 56.70 | 12.24 | 15.15 | 15.77 | 17.77 | 160 / 160 | label renders 1690 (56 ms) labels≤300 |
| 2560x1440@2 → 5120x2880 | pan | 9.3 | 73.67 | 21.20 | 22.02 | 23.32 | 27.80 | 216 / 216 | label renders 1175 (25 ms) labels≤300 |
| 2560x1440@2 → 5120x2880 | pan-fit | 9.3 | 73.67 | 20.78 | 21.76 | 21.84 | 21.89 | 216 / 216 | label renders 1175 (25 ms) labels≤0 |
| 2560x1440@2 → 5120x2880 | zoom-out | 9.3 | 73.67 | 21.25 | 22.09 | 22.38 | 22.64 | 216 / 216 | label renders 1955 (39 ms) labels≤300 |
| 3840x2160@2 → 7680x4320 | pan | 14.4 | 243.58 | 44.59 | 45.53 | 47.39 | 48.96 | 286 / 286 | label renders 952 (20 ms) labels≤300 |
| 3840x2160@2 → 7680x4320 | pan-fit | 14.4 | 243.58 | 44.13 | 45.71 | 46.03 | 46.90 | 286 / 286 | label renders 952 (20 ms) labels≤0 |
| 3840x2160@2 → 7680x4320 | zoom-out | 14.4 | 243.58 | 44.35 | 45.90 | 46.27 | 46.66 | 286 / 286 | label renders 2018 (37 ms) labels≤300 |
