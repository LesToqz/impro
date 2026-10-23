## target/release/e_wgpu --res 1920x1080@1,3840x2160@1,3840x2160@2
date: 2026-10-23 22:57 IST
adapter: llvmpipe (LLVM 21.1.8, 256 bits) / backend Vulkan / type Cpu / driver `llvmpipe` Mesa 26.0.8-1ubuntu0.3 (LLVM 21.1.8)
adapter: SVGA3D; build: RELEASE;  LLVM; / backend Gl / type Other / driver `` 4.3 (Core Profile) Mesa 26.0.8-1ubuntu0.3

### E wgpu offscreen — fixture `app`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 21.7 | 16.07 | 1.97 | 2.42 | 2.55 | 2.55 | 113 / 113 | llvmpipe (LLVM 21.1.8, 256 bits) / Vulkan |
| 1920x1080@1 → 1920x1080 | pan-fit | 21.7 | 16.07 | 2.35 | 2.83 | 2.97 | 3.12 | 113 / 113 | llvmpipe (LLVM 21.1.8, 256 bits) / Vulkan |
| 1920x1080@1 → 1920x1080 | zoom-out | 21.7 | 16.07 | 2.24 | 2.58 | 2.78 | 2.82 | 113 / 113 | llvmpipe (LLVM 21.1.8, 256 bits) / Vulkan |
| 3840x2160@1 → 3840x2160 | pan | 18.0 | 21.80 | 4.59 | 5.29 | 5.34 | 5.35 | 145 / 145 | llvmpipe (LLVM 21.1.8, 256 bits) / Vulkan |
| 3840x2160@1 → 3840x2160 | pan-fit | 18.0 | 21.80 | 4.51 | 4.99 | 5.17 | 5.27 | 144 / 144 | llvmpipe (LLVM 21.1.8, 256 bits) / Vulkan |
| 3840x2160@1 → 3840x2160 | zoom-out | 18.0 | 21.80 | 4.40 | 5.16 | 5.85 | 6.14 | 145 / 145 | llvmpipe (LLVM 21.1.8, 256 bits) / Vulkan |
| 3840x2160@2 → 7680x4320 | pan | 17.6 | 108.22 | 14.09 | 14.85 | 15.46 | 15.74 | 248 / 248 | llvmpipe (LLVM 21.1.8, 256 bits) / Vulkan |
| 3840x2160@2 → 7680x4320 | pan-fit | 17.6 | 108.22 | 13.54 | 14.53 | 14.88 | 15.26 | 247 / 247 | llvmpipe (LLVM 21.1.8, 256 bits) / Vulkan |
| 3840x2160@2 → 7680x4320 | zoom-out | 17.6 | 108.22 | 13.69 | 14.71 | 15.43 | 15.45 | 246 / 247 | llvmpipe (LLVM 21.1.8, 256 bits) / Vulkan |
