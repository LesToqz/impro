## target/release/d_gl --sync1px --labels --res 1920x1080@1,3840x2160@1,3840x2160@2
date: 2026-10-23 22:57 IST

### D raw GL/glow instanced (readback=false, finish=true) — fixture `app`, labels 300, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 37.7 | 35.24 | 2.71 | 3.11 | 3.22 | 3.33 | 91 / 91 | label renders 466 (24 ms) labels≤300 |
| 1920x1080@1 → 1920x1080 | pan-fit | 37.7 | 35.24 | 2.39 | 2.95 | 3.11 | 3.61 | 91 / 91 | label renders 466 (24 ms) labels≤0 |
| 1920x1080@1 → 1920x1080 | zoom-out | 37.7 | 35.24 | 2.46 | 5.00 | 5.22 | 6.60 | 99 / 99 | label renders 1690 (45 ms) labels≤300 |
| 3840x2160@1 → 3840x2160 | pan | 8.9 | 25.47 | 8.38 | 9.23 | 9.87 | 12.62 | 95 / 99 | label renders 952 (18 ms) labels≤300 |
| 3840x2160@1 → 3840x2160 | pan-fit | 8.9 | 25.47 | 7.95 | 8.50 | 8.55 | 8.71 | 95 / 99 | label renders 952 (18 ms) labels≤0 |
| 3840x2160@1 → 3840x2160 | zoom-out | 8.9 | 25.47 | 8.37 | 9.91 | 10.27 | 12.55 | 102 / 102 | label renders 2018 (39 ms) labels≤300 |
| 3840x2160@2 → 7680x4320 | pan | 8.8 | 69.97 | 28.96 | 30.71 | 30.85 | 40.95 | 112 / 112 | label renders 952 (36 ms) labels≤300 |
| 3840x2160@2 → 7680x4320 | pan-fit | 8.8 | 69.97 | 28.49 | 29.24 | 29.41 | 29.81 | 112 / 112 | label renders 952 (36 ms) labels≤0 |
| 3840x2160@2 → 7680x4320 | zoom-out | 8.8 | 69.97 | 29.06 | 30.72 | 31.01 | 35.17 | 136 / 136 | label renders 2018 (69 ms) labels≤300 |
