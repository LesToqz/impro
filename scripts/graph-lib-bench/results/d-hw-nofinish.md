## target/release/d_gl --nofinish --res 1920x1080@1,3840x2160@1,3840x2160@2
date: 2026-10-23 22:57 IST

### D raw GL/glow instanced (readback=false, finish=false) — fixture `app`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 33.4 | 0.23 | 0.00 | 0.00 | 0.01 | 0.01 | 82 / 82 | label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | pan-fit | 33.4 | 0.23 | 0.00 | 0.01 | 0.03 | 0.07 | 82 / 82 | label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | zoom-out | 33.4 | 0.23 | 0.00 | 0.00 | 0.00 | 0.03 | 82 / 82 | label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan | 9.8 | 0.16 | 0.00 | 0.00 | 0.00 | 0.00 | 82 / 82 | label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan-fit | 9.8 | 0.16 | 0.00 | 0.00 | 0.04 | 0.09 | 82 / 82 | label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | zoom-out | 9.8 | 0.16 | 0.00 | 0.00 | 0.00 | 0.03 | 82 / 82 | label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan | 8.8 | 0.25 | 0.00 | 0.00 | 0.00 | 0.01 | 83 / 83 | label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan-fit | 8.8 | 0.25 | 0.00 | 0.00 | 0.00 | 0.04 | 83 / 83 | label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | zoom-out | 8.8 | 0.25 | 0.00 | 0.00 | 0.00 | 0.03 | 83 / 83 | label renders 0 (0 ms) |
