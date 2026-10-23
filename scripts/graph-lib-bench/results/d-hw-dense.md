## target/release/d_gl --fixture dense --readback --res 1920x1080@1,3840x2160@1,3840x2160@2
date: 2026-10-23 22:54 IST

### D raw GL/glow instanced (readback=true, finish=true) — fixture `dense`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 35.9 | 12.83 | 4.22 | 4.45 | 4.49 | 4.55 | 90 / 90 | label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | pan-fit | 35.9 | 12.83 | 4.26 | 4.51 | 4.56 | 4.62 | 90 / 90 | label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | zoom-out | 35.9 | 12.83 | 4.05 | 4.50 | 4.56 | 4.59 | 90 / 90 | label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan | 9.1 | 36.36 | 12.67 | 13.21 | 13.25 | 13.45 | 114 / 114 | label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan-fit | 9.1 | 36.36 | 12.70 | 13.24 | 13.46 | 13.59 | 114 / 114 | label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | zoom-out | 9.1 | 36.36 | 12.36 | 13.19 | 13.23 | 13.36 | 114 / 114 | label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan | 8.6 | 137.64 | 44.83 | 46.27 | 46.93 | 49.63 | 210 / 210 | label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan-fit | 8.6 | 137.64 | 45.10 | 46.38 | 46.82 | 48.29 | 210 / 210 | label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | zoom-out | 8.6 | 137.64 | 44.46 | 45.94 | 45.98 | 47.73 | 210 / 210 | label renders 0 (0 ms) |
