## target/release/d_gl --sync1px --fixture dense --res 1920x1080@1,3840x2160@1,3840x2160@2
date: 2026-10-23 22:57 IST

### D raw GL/glow instanced (readback=false, finish=true) — fixture `dense`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 33.6 | 7.95 | 2.52 | 3.04 | 3.05 | 3.08 | 82 / 82 | label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | pan-fit | 33.6 | 7.95 | 2.48 | 2.67 | 2.85 | 3.05 | 82 / 82 | label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | zoom-out | 33.6 | 7.95 | 2.48 | 2.87 | 3.03 | 3.06 | 82 / 82 | label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan | 9.4 | 15.70 | 8.24 | 8.63 | 8.85 | 8.89 | 82 / 82 | label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan-fit | 9.4 | 15.70 | 8.37 | 9.15 | 9.39 | 9.60 | 82 / 82 | label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | zoom-out | 9.4 | 15.70 | 8.07 | 8.66 | 8.76 | 8.81 | 82 / 82 | label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan | 11.0 | 53.16 | 29.05 | 30.07 | 30.30 | 33.46 | 83 / 83 | label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan-fit | 11.0 | 53.16 | 29.31 | 30.12 | 30.25 | 30.48 | 83 / 83 | label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | zoom-out | 11.0 | 53.16 | 28.72 | 31.07 | 31.74 | 32.33 | 83 / 83 | label renders 0 (0 ms) |
