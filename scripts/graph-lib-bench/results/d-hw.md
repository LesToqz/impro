## target/release/d_gl
date: 2026-10-23 22:54 IST

### D raw GL/glow instanced (readback=false, finish=true) — fixture `app`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 94.0 | 262.61 | 0.36 | 0.45 | 0.64 | 0.67 | 81 / 81 | label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | pan-fit | 94.0 | 262.61 | 0.37 | 0.47 | 0.81 | 0.92 | 81 / 81 | label renders 0 (0 ms) |
| 1920x1080@1 → 1920x1080 | zoom-out | 94.0 | 262.61 | 0.38 | 0.46 | 0.51 | 0.53 | 82 / 82 | label renders 0 (0 ms) |
| 2560x1440@1 → 2560x1440 | pan | 11.1 | 7.45 | 0.40 | 0.54 | 0.59 | 0.69 | 82 / 82 | label renders 0 (0 ms) |
| 2560x1440@1 → 2560x1440 | pan-fit | 11.1 | 7.45 | 0.39 | 0.46 | 0.53 | 0.55 | 82 / 82 | label renders 0 (0 ms) |
| 2560x1440@1 → 2560x1440 | zoom-out | 11.1 | 7.45 | 0.38 | 0.47 | 0.52 | 0.56 | 82 / 82 | label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan | 10.4 | 8.89 | 0.39 | 0.58 | 0.63 | 2.18 | 82 / 82 | label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | pan-fit | 10.4 | 8.89 | 0.38 | 0.54 | 0.55 | 2.01 | 82 / 82 | label renders 0 (0 ms) |
| 3840x2160@1 → 3840x2160 | zoom-out | 10.4 | 8.89 | 0.39 | 0.60 | 0.61 | 0.79 | 82 / 82 | label renders 0 (0 ms) |
| 1920x1080@2 → 3840x2160 | pan | 8.6 | 10.60 | 0.39 | 0.47 | 0.50 | 0.53 | 82 / 82 | label renders 0 (0 ms) |
| 1920x1080@2 → 3840x2160 | pan-fit | 8.6 | 10.60 | 0.39 | 0.48 | 0.48 | 0.55 | 82 / 82 | label renders 0 (0 ms) |
| 1920x1080@2 → 3840x2160 | zoom-out | 8.6 | 10.60 | 0.40 | 0.50 | 0.72 | 0.81 | 82 / 82 | label renders 0 (0 ms) |
| 2560x1440@2 → 5120x2880 | pan | 10.3 | 12.49 | 0.41 | 0.55 | 0.57 | 0.57 | 82 / 82 | label renders 0 (0 ms) |
| 2560x1440@2 → 5120x2880 | pan-fit | 10.3 | 12.49 | 0.43 | 0.52 | 0.57 | 0.61 | 82 / 82 | label renders 0 (0 ms) |
| 2560x1440@2 → 5120x2880 | zoom-out | 10.3 | 12.49 | 0.42 | 0.54 | 0.55 | 0.61 | 82 / 82 | label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan | 8.6 | 22.16 | 0.64 | 0.99 | 1.14 | 1.14 | 82 / 82 | label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | pan-fit | 8.6 | 22.16 | 0.63 | 1.06 | 1.41 | 1.52 | 82 / 82 | label renders 0 (0 ms) |
| 3840x2160@2 → 7680x4320 | zoom-out | 8.6 | 22.16 | 0.55 | 0.67 | 0.69 | 0.78 | 82 / 82 | label renders 0 (0 ms) |
