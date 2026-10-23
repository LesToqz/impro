## target/release/d_gl --fixture dense --readback --res 1920x1080@1,3840x2160@1,3840x2160@2
date: 2026-10-23 22:58 IST

### D raw GL/glow instanced (readback=true, finish=true) — fixture `dense`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 56.9 | 22.27 | 12.66 | 14.08 | 14.17 | 14.27 | 137 / 138 | label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | pan-fit | 56.9 | 22.27 | 13.29 | 13.82 | 14.01 | 14.11 | 139 / 139 | label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | zoom-out | 56.9 | 22.27 | 10.83 | 13.90 | 14.07 | 14.30 | 139 / 139 | label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan | 42.0 | 41.70 | 16.23 | 16.71 | 16.85 | 17.66 | 198 / 198 | label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan-fit | 42.0 | 41.70 | 17.96 | 18.38 | 18.59 | 19.40 | 198 / 198 | label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | zoom-out | 42.0 | 41.70 | 11.96 | 19.39 | 19.94 | 19.96 | 199 / 199 | label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan | 73.2 | 105.28 | 42.00 | 43.46 | 44.02 | 44.63 | 395 / 395 | label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan-fit | 73.2 | 105.28 | 45.05 | 45.99 | 46.61 | 46.81 | 395 / 395 | label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | zoom-out | 73.2 | 105.28 | 32.65 | 48.51 | 49.19 | 51.05 | 395 / 395 | label renders 0 (0 ms) |
