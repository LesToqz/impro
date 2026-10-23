## target/release/e_wgpu --res 1920x1080@1,3840x2160@1,3840x2160@2
date: 2026-10-23 22:55 IST
adapter: llvmpipe (LLVM 21.1.8, 256 bits) / backend Vulkan / type Cpu / driver `llvmpipe` Mesa 26.0.8-1ubuntu0.3 (LLVM 21.1.8)
adapter: SVGA3D; build: RELEASE;  LLVM; / backend Gl / type Other / driver `` 4.3 (Core Profile) Mesa 26.0.8-1ubuntu0.3

### E wgpu offscreen — fixture `app`, labels off, 60 frames per scenario
| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| 1920x1080@1 → 1920x1080 | pan | 56.1 | 309.40 | 1.91 | 2.23 | 2.42 | 2.69 | 116 / 116 | llvmpipe (LLVM 21.1.8, 256 bits) / Vulkan |
| 1920x1080@1 → 1920x1080 | pan-fit | 56.1 | 309.40 | 2.41 | 2.75 | 2.91 | 3.07 | 117 / 117 | llvmpipe (LLVM 21.1.8, 256 bits) / Vulkan |
| 1920x1080@1 → 1920x1080 | zoom-out | 56.1 | 309.40 | 2.16 | 2.57 | 2.63 | 2.72 | 117 / 117 | llvmpipe (LLVM 21.1.8, 256 bits) / Vulkan |
| 3840x2160@1 → 3840x2160 | pan | 21.1 | 23.37 | 4.69 | 5.08 | 5.21 | 5.95 | 147 / 147 | llvmpipe (LLVM 21.1.8, 256 bits) / Vulkan |
| 3840x2160@1 → 3840x2160 | pan-fit | 21.1 | 23.37 | 4.55 | 5.17 | 5.31 | 5.42 | 147 / 147 | llvmpipe (LLVM 21.1.8, 256 bits) / Vulkan |
| 3840x2160@1 → 3840x2160 | zoom-out | 21.1 | 23.37 | 4.49 | 5.23 | 5.31 | 5.43 | 147 / 147 | llvmpipe (LLVM 21.1.8, 256 bits) / Vulkan |
| 3840x2160@2 → 7680x4320 | pan | 16.1 | 114.99 | 14.15 | 14.99 | 15.35 | 16.72 | 250 / 250 | llvmpipe (LLVM 21.1.8, 256 bits) / Vulkan |
| 3840x2160@2 → 7680x4320 | pan-fit | 16.1 | 114.99 | 13.62 | 14.62 | 14.97 | 16.61 | 249 / 249 | llvmpipe (LLVM 21.1.8, 256 bits) / Vulkan |
| 3840x2160@2 → 7680x4320 | zoom-out | 16.1 | 114.99 | 13.87 | 15.38 | 17.26 | 19.99 | 249 / 249 | llvmpipe (LLVM 21.1.8, 256 bits) / Vulkan |
