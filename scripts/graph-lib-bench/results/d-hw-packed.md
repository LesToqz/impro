## target/release/d_gl --fixture packed --readback --res 1920x1080@1,3840x2160@1,3840x2160@2
date: 2026-10-23 22:54 IST

### D raw GL/glow instanced (readback=true, finish=true) — fixture `packed`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 35.4 | 16.23 | 4.05 | 4.24 | 4.27 | 4.32 | 90 / 90 | label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | pan-fit | 35.4 | 16.23 | 4.01 | 4.27 | 4.42 | 4.42 | 90 / 90 | label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | zoom-out | 35.4 | 16.23 | 3.98 | 4.35 | 4.41 | 4.53 | 90 / 90 | label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan | 12.2 | 63.70 | 12.31 | 13.01 | 13.33 | 13.99 | 114 / 114 | label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan-fit | 12.2 | 63.70 | 12.14 | 13.06 | 13.43 | 13.47 | 114 / 114 | label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | zoom-out | 12.2 | 63.70 | 12.14 | 12.79 | 13.01 | 13.91 | 114 / 114 | label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan | 11.5 | 132.27 | 44.47 | 45.45 | 45.84 | 46.19 | 209 / 209 | label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan-fit | 11.5 | 132.27 | 44.24 | 45.53 | 46.24 | 46.54 | 209 / 209 | label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | zoom-out | 11.5 | 132.27 | 44.37 | 45.31 | 45.50 | 46.11 | 209 / 209 | label renders 0 (0 ms) |
