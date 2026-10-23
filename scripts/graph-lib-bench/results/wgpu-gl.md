## target/release/e_wgpu --backend gl --res 1920x1080@1,3840x2160@1,3840x2160@2
date: 2026-10-23 22:57 IST
adapter: SVGA3D; build: RELEASE;  LLVM; / backend Gl / type Other / driver `` 4.3 (Core Profile) Mesa 26.0.8-1ubuntu0.3

### E wgpu offscreen — fixture `app`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 23.0 | 157.96 | 0.58 | 0.94 | 1.37 | 2.63 | 83 / 83 | SVGA3D; build: RELEASE;  LLVM; / Gl |
| 1920x1080@1 → 1920x1080 | pan-fit | 23.0 | 157.96 | 0.58 | 0.87 | 1.05 | 5.40 | 83 / 83 | SVGA3D; build: RELEASE;  LLVM; / Gl |
| 1920x1080@1 → 1920x1080 | zoom-out | 23.0 | 157.96 | 0.58 | 0.85 | 0.87 | 0.89 | 83 / 83 | SVGA3D; build: RELEASE;  LLVM; / Gl |
| 3840x2160@1 → 3840x2160 | pan | 7.7 | 6.28 | 0.73 | 0.89 | 0.93 | 0.96 | 84 / 84 | SVGA3D; build: RELEASE;  LLVM; / Gl |
| 3840x2160@1 → 3840x2160 | pan-fit | 7.7 | 6.28 | 0.71 | 0.88 | 0.90 | 1.05 | 84 / 84 | SVGA3D; build: RELEASE;  LLVM; / Gl |
| 3840x2160@1 → 3840x2160 | zoom-out | 7.7 | 6.28 | 0.73 | 0.89 | 1.19 | 1.23 | 84 / 84 | SVGA3D; build: RELEASE;  LLVM; / Gl |
| 3840x2160@2 → 7680x4320 | pan | 7.2 | 16.74 | 0.95 | 1.15 | 1.20 | 1.27 | 84 / 84 | SVGA3D; build: RELEASE;  LLVM; / Gl |
| 3840x2160@2 → 7680x4320 | pan-fit | 7.2 | 16.74 | 1.00 | 1.11 | 1.15 | 1.35 | 84 / 84 | SVGA3D; build: RELEASE;  LLVM; / Gl |
| 3840x2160@2 → 7680x4320 | zoom-out | 7.2 | 16.74 | 0.96 | 1.13 | 1.14 | 1.64 | 84 / 84 | SVGA3D; build: RELEASE;  LLVM; / Gl |
