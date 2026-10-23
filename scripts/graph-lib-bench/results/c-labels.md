## target/release/c_skia --labels
date: 2026-10-23 23:05 IST

### C tiny-skia (cached layer + integer translate: false) — fixture `app`, labels 300, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 0.0 | 56.23 | 5.99 | 6.82 | 6.93 | 7.06 | 29 / 29 | scene renders 61 blits 0 label renders 466 (49 ms) labels≤300 |
| 1920x1080@1 → 1920x1080 | pan-fit | 0.0 | 56.23 | 7.92 | 8.78 | 9.12 | 9.17 | 31 / 31 | scene renders 121 blits 0 label renders 466 (49 ms) labels≤0 |
| 1920x1080@1 → 1920x1080 | zoom-out | 0.0 | 56.23 | 8.76 | 13.22 | 13.70 | 14.03 | 45 / 45 | scene renders 181 blits 0 label renders 1690 (64 ms) labels≤300 |
| 2560x1440@1 → 2560x1440 | pan | 0.0 | 18.14 | 9.34 | 11.00 | 11.44 | 11.48 | 51 / 51 | scene renders 61 blits 0 label renders 1175 (18 ms) labels≤300 |
| 2560x1440@1 → 2560x1440 | pan-fit | 0.0 | 18.14 | 8.14 | 8.82 | 8.98 | 9.12 | 51 / 51 | scene renders 121 blits 0 label renders 1175 (18 ms) labels≤0 |
| 2560x1440@1 → 2560x1440 | zoom-out | 0.0 | 18.14 | 8.54 | 14.43 | 15.65 | 16.31 | 55 / 55 | scene renders 181 blits 0 label renders 1955 (30 ms) labels≤300 |
| 3840x2160@1 → 3840x2160 | pan | 0.0 | 35.65 | 17.01 | 18.16 | 18.52 | 18.60 | 72 / 72 | scene renders 61 blits 0 label renders 952 (14 ms) labels≤300 |
| 3840x2160@1 → 3840x2160 | pan-fit | 0.0 | 35.65 | 11.15 | 12.21 | 12.33 | 12.62 | 72 / 72 | scene renders 121 blits 0 label renders 952 (14 ms) labels≤0 |
| 3840x2160@1 → 3840x2160 | zoom-out | 0.0 | 35.65 | 12.89 | 20.60 | 20.89 | 20.99 | 73 / 73 | scene renders 181 blits 0 label renders 2018 (31 ms) labels≤300 |
| 1920x1080@2 → 3840x2160 | pan | 0.4 | 42.71 | 19.40 | 20.45 | 20.83 | 21.88 | 68 / 73 | scene renders 61 blits 0 label renders 466 (20 ms) labels≤300 |
| 1920x1080@2 → 3840x2160 | pan-fit | 0.4 | 42.71 | 12.98 | 13.72 | 14.02 | 14.49 | 70 / 73 | scene renders 121 blits 0 label renders 466 (20 ms) labels≤0 |
| 1920x1080@2 → 3840x2160 | zoom-out | 0.4 | 42.71 | 16.03 | 32.33 | 32.98 | 33.64 | 122 / 122 | scene renders 181 blits 0 label renders 1690 (61 ms) labels≤300 |
| 2560x1440@2 → 5120x2880 | pan | 0.0 | 57.62 | 26.78 | 28.01 | 29.24 | 29.54 | 178 / 178 | scene renders 61 blits 0 label renders 1175 (31 ms) labels≤300 |
| 2560x1440@2 → 5120x2880 | pan-fit | 0.0 | 57.62 | 16.93 | 17.74 | 17.94 | 18.11 | 178 / 178 | scene renders 121 blits 0 label renders 1175 (31 ms) labels≤0 |
| 2560x1440@2 → 5120x2880 | zoom-out | 0.0 | 57.62 | 21.01 | 35.24 | 35.70 | 35.96 | 178 / 178 | scene renders 181 blits 0 label renders 1955 (51 ms) labels≤300 |
| 3840x2160@2 → 7680x4320 | pan | 0.0 | 100.64 | 41.52 | 42.87 | 43.24 | 43.40 | 248 / 248 | scene renders 61 blits 0 label renders 952 (22 ms) labels≤300 |
| 3840x2160@2 → 7680x4320 | pan-fit | 0.0 | 100.64 | 26.44 | 27.71 | 28.82 | 28.82 | 248 / 248 | scene renders 121 blits 0 label renders 952 (22 ms) labels≤0 |
| 3840x2160@2 → 7680x4320 | zoom-out | 0.0 | 100.64 | 34.45 | 44.16 | 44.96 | 45.42 | 248 / 248 | scene renders 181 blits 0 label renders 2018 (46 ms) labels≤300 |
