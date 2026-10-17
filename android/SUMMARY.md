# vieww on AWS Device Farm

run: `arn:aws:devicefarm:us-west-2:278061313767:run:393108fd-d195-4dad-acb4-12581f7a212f/3ca6025b-931b-47af-ad2a-fbbe4202bb29` — FAILED

| phone | tier | Android | SoC / GPU | RAM | APK | cold start cpu / gpu (ms, median of 3) | frame time cpu | frame time gpu | frames drawn by the GPU, cpu pass / gpu pass | PSS cpu / gpu (MB) | suite cpu / gpu |
|---|---|---|---|---|---|---|---|---|---|---|---|
| LG Stylo 5 | low | 9 | msm8953<br>Qualcomm, Adreno (TM) 506, OpenGL ES 3.2 V@331.0 (GIT@50f1b2 | 2.7 GB | 2.6 MB | 360 / 326 | median 36.110729ms, worst 67.395677ms, budget 16.666668ms (240 samples) | median 40.848594ms, worst 47.520834ms, budget 16.666668ms (240 samples) | 0 of 600 / 600 of 600 | 78 / 129 | FAIL / FAIL |
| Samsung Galaxy S20 (Unlocked) | mid | 13 | SM8250 (kona)<br>Qualcomm, Adreno (TM) 650, OpenGL ES 3.2 V@0502.0 (GIT@193b2 | 10.4 GB | 2.6 MB | 161 / 144 | median 11.889845ms, worst 18.615156ms, budget 16.666666ms (240 samples) | median 7.225468ms, worst 13.83276ms, budget 16.666666ms (240 samples) | 0 of 600 / 600 of 600 | 130 / 149 | PASS / PASS |
| Samsung Galaxy S26 Ultra | high | 16 | SM8850 (canoe)<br>Qualcomm, Adreno (TM) 840, OpenGL ES 3.2 V@0842.19.3 (GIT@ba | 10.9 GB | 2.6 MB | 85 / 87 | median 5.424688ms, worst 19.722187ms, budget 8.333333ms (240 samples) | median 2.1775ms, worst 11.276926ms, budget 8.333333ms (240 samples) | 0 of 600 / 600 of 600 | 77 / 157 | PASS / PASS |

**LG Stylo 5**
- CPU path: median 36.110729ms, worst 67.395677ms, budget 16.666668ms (240 samples)
  - start-up: before run 261 ms | app setup 0.1 | event loop started 0.0 | resumed 43.0 | window created 0.1 | present ready 0.6 | accessibility and show 0.0 | tree built 5.3 | window ready 0.6 | until first draw 0.6 | app before-frame hooks 0.0 | first build and layout 12.9 | first raster and present 156.3 = 480.4 ms from process start
  - stages: medians ms: scale 0.07 plan 0.00 cpu 30.71 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.36 present 3.80
  - stages: p95 ms: scale 0.10 plan 0.00 cpu 39.35 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.47 present 5.33
  - worst frame: work 67.40 ms, outside the renderer 1.66 ms, on cpu2; stages ms: scale 0.07 plan 0.00 cpu 60.20 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.37 present 5.09
  - raster by core: cpu2 88 frames 30.43 ms | cpu0 67 frames 31.02 ms | cpu5 37 frames 30.48 ms | cpu1 31 frames 30.45 ms | cpu4 10 frames 33.02 ms | cpu6 5 frames 29.35 ms | cpu3 1 frames 38.81 ms | cpu7 1 frames 40.81 ms
  - clocks at report: cpu0 1.80/1.80 GHz | cpu1 1.80/1.80 GHz | cpu2 1.80/1.80 GHz | cpu3 1.80/1.80 GHz | cpu4 1.80/1.80 GHz | cpu5 1.80/1.80 GHz | cpu6 1.80/1.80 GHz | cpu7 1.80/1.80 GHz
- GPU path: median 40.848594ms, worst 47.520834ms, budget 16.666668ms (240 samples)
  - start-up: before run 262 ms | app setup 0.3 | event loop started 0.0 | resumed 41.0 | window created 0.1 | present ready 120.7 | accessibility and show 0.0 | tree built 6.1 | window ready 0.5 | until first draw 0.1 | app before-frame hooks 0.0 | first build and layout 8.9 | first raster and present 106.0 = 545.7 ms from process start
  - stages: medians ms: scale 0.20 plan 1.53 cpu 0.00 gpu_upload 1.26 gpu_record 0.81 gpu_submit 0.35 gpu_wait 34.25 gpu_readback 0.00 acquire 0.45 present 0.55
  - stages: p95 ms: scale 0.37 plan 2.80 cpu 0.00 gpu_upload 2.48 gpu_record 1.18 gpu_submit 0.54 gpu_wait 34.74 gpu_readback 0.00 acquire 1.14 present 0.73
  - worst frame: work 47.52 ms, outside the renderer 1.84 ms; stages ms: scale 0.14 plan 2.52 cpu 0.00 gpu_upload 2.48 gpu_record 0.76 gpu_submit 4.47 gpu_wait 34.32 gpu_readback 0.00 acquire 0.44 present 0.56
  - clocks at report: cpu0 1.80/1.80 GHz | cpu1 1.80/1.80 GHz | cpu2 1.80/1.80 GHz | cpu3 1.80/1.80 GHz | cpu4 1.80/1.80 GHz | cpu5 1.80/1.80 GHz | cpu6 1.80/1.80 GHz | cpu7 1.80/1.80 GHz
  - pipeline cache: started from 39458 bytes; saved 39458 bytes after the first GPU frame
- chosen by measurement (auto): median 36.182292ms, worst 68.955832ms, budget 16.666668ms (240 samples)
  - calibration launches: launch 1: 10-17 18:06:53.601 18719 18736 I vieww   : vieww_platform_winit::rasterizer_choice: vieww: rasterizer cpu, measuring (first launch with this build here) 10-17 18:06:55.347 18719 18736 I vieww   : vieww_platform_winit::rasterizer_choice: vieww: rasterizer cpu for now; the gpu is measured next launch (measured: cpu 22.63 ms; budget 16.67 ms) launch 2: 10-17 18:07:02.834 18785 18802 I vieww   : vieww_platform_winit::rasterizer_choice: vieww: rasterizer gpu, measuring (the cpu path measured 22.63 ms last launch) 10-17 18:07:05.301 18785 18802 I vieww   : vieww_platform_winit::rasterizer_choice: vieww: rasterizer cpu (measured: cpu 22.63 ms, gpu 39.64 ms; budget 16.67 ms) launch 3: 10-17 18:07:11.867 18849 18866 I vieww   : vieww_platform_winit::rasterizer_choice: vieww: rasterizer cpu (calibrated earlier: cpu 22.63 ms, gpu 39.64 ms)
  - start-up: before run 263 ms | app setup 0.3 | event loop started 0.0 | resumed 31.3 | window created 0.1 | present ready 0.5 | accessibility and show 0.0 | tree built 5.5 | window ready 0.6 | until first draw 0.1 | app before-frame hooks 0.0 | first build and layout 7.9 | first raster and present 151.1 = 460.9 ms from process start
  - stages: medians ms: scale 0.07 plan 0.00 cpu 30.94 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.35 present 3.75
  - stages: p95 ms: scale 0.09 plan 0.00 cpu 38.76 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.49 present 4.96
  - worst frame: work 68.96 ms, outside the renderer 1.98 ms, on cpu5; stages ms: scale 0.14 plan 0.00 cpu 60.64 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.30 present 5.90
  - raster by core: cpu3 132 frames 30.60 ms | cpu1 57 frames 29.95 ms | cpu7 21 frames 33.15 ms | cpu5 12 frames 34.94 ms | cpu6 10 frames 34.93 ms | cpu2 6 frames 36.23 ms | cpu4 2 frames 37.00 ms
  - clocks at report: cpu0 1.80/1.80 GHz | cpu1 1.80/1.80 GHz | cpu2 1.80/1.80 GHz | cpu3 1.80/1.80 GHz | cpu4 1.80/1.80 GHz | cpu5 1.80/1.80 GHz | cpu6 1.80/1.80 GHz | cpu7 1.80/1.80 GHz
- CPU path presented through Vulkan (diagnostic): median 42.960781ms, worst 80.336146ms, budget 16.666668ms (240 samples)
  - start-up: before run 273 ms | app setup 0.1 | event loop started 0.0 | resumed 43.7 | window created 0.1 | present ready 100.0 | accessibility and show 0.0 | tree built 4.8 | window ready 0.5 | until first draw 0.1 | app before-frame hooks 0.0 | first build and layout 7.7 | first raster and present 148.7 = 578.5 ms from process start
  - stages: medians ms: scale 0.08 plan 0.00 cpu 20.32 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.00 present 19.50
  - stages: p95 ms: scale 0.17 plan 0.00 cpu 32.03 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.00 present 24.60
  - worst frame: work 80.34 ms, outside the renderer 2.35 ms, on cpu2; stages ms: scale 0.18 plan 0.00 cpu 53.07 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.00 present 24.74
  - raster by core: cpu5 79 frames 20.16 ms | cpu4 63 frames 19.68 ms | cpu6 36 frames 20.66 ms | cpu7 36 frames 21.63 ms | cpu0 20 frames 21.84 ms | cpu1 4 frames 31.85 ms | cpu2 1 frames 53.07 ms | cpu3 1 frames 20.73 ms
  - clocks at report: cpu0 1.69/1.80 GHz | cpu1 1.69/1.80 GHz | cpu2 1.69/1.80 GHz | cpu3 1.69/1.80 GHz | cpu4 1.69/1.80 GHz | cpu5 1.69/1.80 GHz | cpu6 1.69/1.80 GHz | cpu7 1.69/1.80 GHz
- CPU path with an Android performance hint (diagnostic): median 36.792188ms, worst 58.045624ms, budget 16.666668ms (240 samples)
  - start-up: before run 282 ms | app setup 0.4 | event loop started 0.0 | resumed 35.4 | window created 0.0 | present ready 0.7 | accessibility and show 0.0 | tree built 8.2 | window ready 0.5 | until first draw 0.1 | app before-frame hooks 0.0 | first build and layout 8.9 | first raster and present 156.1 = 492.9 ms from process start
  - stages: medians ms: scale 0.07 plan 0.00 cpu 31.53 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.37 present 3.73
  - stages: p95 ms: scale 0.10 plan 0.00 cpu 38.96 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.57 present 5.03
  - worst frame: work 58.05 ms, outside the renderer 1.49 ms, on cpu5; stages ms: scale 0.08 plan 0.00 cpu 51.46 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.34 present 4.67
  - raster by core: cpu2 96 frames 30.88 ms | cpu5 71 frames 31.40 ms | cpu7 23 frames 31.62 ms | cpu0 18 frames 36.36 ms | cpu4 14 frames 32.74 ms | cpu6 13 frames 32.27 ms | cpu3 5 frames 32.61 ms
  - clocks at report: cpu0 1.80/1.80 GHz | cpu1 1.80/1.80 GHz | cpu2 1.80/1.80 GHz | cpu3 1.80/1.80 GHz | cpu4 1.80/1.80 GHz | cpu5 1.80/1.80 GHz | cpu6 1.80/1.80 GHz | cpu7 1.80/1.80 GHz
  - performance hint: unavailable (Android 12 or older: no APerformanceHint)
- failed checks: no_frame_stalled the_typical_frame_is_inside_its_budget the_typical_frame_is_inside_its_budget

**Samsung Galaxy S20 (Unlocked)**
- CPU path: median 11.889845ms, worst 18.615156ms, budget 16.666666ms (240 samples)
  - start-up: before run 113 ms | app setup 0.1 | event loop started 0.0 | resumed 15.2 | window created 0.0 | present ready 0.1 | accessibility and show 0.0 | tree built 1.3 | window ready 4.0 | until first draw 0.0 | app before-frame hooks 0.3 | first build and layout 1.4 | first raster and present 37.3 = 173.1 ms from process start
  - stages: medians ms: scale 0.02 plan 0.00 cpu 8.68 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 1.08 present 1.67
  - stages: p95 ms: scale 0.03 plan 0.00 cpu 10.23 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 1.59 present 2.34
  - worst frame: work 18.62 ms, outside the renderer 0.77 ms, on cpu6; stages ms: scale 0.02 plan 0.00 cpu 14.91 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 1.08 present 1.83
  - raster by core: cpu4 93 frames 8.78 ms | cpu5 74 frames 8.57 ms | cpu6 50 frames 8.68 ms | cpu7 23 frames 8.88 ms
  - clocks at report: cpu0 1.08/1.80 GHz | cpu1 1.08/1.80 GHz | cpu2 1.08/1.80 GHz | cpu3 1.08/1.80 GHz | cpu4 2.42/2.42 GHz | cpu5 2.42/2.42 GHz | cpu6 2.42/2.42 GHz | cpu7 0.84/2.84 GHz
- GPU path: median 7.225468ms, worst 13.83276ms, budget 16.666666ms (240 samples)
  - start-up: before run 106 ms | app setup 0.1 | event loop started 0.0 | resumed 16.5 | window created 0.0 | present ready 28.8 | accessibility and show 0.0 | tree built 1.1 | window ready 0.7 | until first draw 0.0 | app before-frame hooks 0.3 | first build and layout 1.5 | first raster and present 21.0 = 175.8 ms from process start
  - stages: medians ms: scale 0.08 plan 0.70 cpu 0.00 gpu_upload 0.49 gpu_record 0.15 gpu_submit 0.15 gpu_wait 4.01 gpu_readback 0.00 acquire 0.05 present 0.11
  - stages: p95 ms: scale 0.21 plan 1.76 cpu 0.00 gpu_upload 1.02 gpu_record 0.36 gpu_submit 0.31 gpu_wait 5.17 gpu_readback 0.00 acquire 0.11 present 0.24
  - worst frame: work 13.83 ms, outside the renderer 5.59 ms; stages ms: scale 0.08 plan 0.67 cpu 0.00 gpu_upload 0.51 gpu_record 0.17 gpu_submit 0.15 gpu_wait 6.46 gpu_readback 0.00 acquire 0.07 present 0.13
  - clocks at report: cpu0 1.17/1.80 GHz | cpu1 1.17/1.80 GHz | cpu2 1.17/1.80 GHz | cpu3 1.17/1.80 GHz | cpu4 2.42/2.42 GHz | cpu5 2.42/2.42 GHz | cpu6 2.42/2.42 GHz | cpu7 2.65/2.84 GHz
  - pipeline cache: started from 38020 bytes; saved 38020 bytes after the first GPU frame
- chosen by measurement (auto): median 12.051302ms, worst 20.467761ms, budget 16.666666ms (240 samples)
  - renderer: cpu (calibrated earlier: cpu 8.80 ms)
  - calibration launches: launch 1: 10-17 18:06:19.067  5242  5259 I vieww   : vieww_platform_winit::rasterizer_choice: vieww: rasterizer cpu, measuring (first launch with this build here) 10-17 18:06:20.159  5242  5259 I vieww   : vieww_platform_winit::rasterizer_choice: vieww: rasterizer cpu (measured: cpu 8.81 ms; budget 16.67 ms) launch 2: 10-17 18:06:27.846  5333  5354 I vieww   : vieww_platform_winit::rasterizer_choice: vieww: rasterizer cpu (calibrated earlier: cpu 8.80 ms) launch 3: 10-17 18:06:36.578  5445  5462 I vieww   : vieww_platform_winit::rasterizer_choice: vieww: rasterizer cpu (calibrated earlier: cpu 8.80 ms)
  - start-up: before run 127 ms | app setup 0.1 | event loop started 0.0 | resumed 16.1 | window created 0.0 | present ready 0.2 | accessibility and show 0.0 | tree built 1.2 | window ready 3.8 | until first draw 0.0 | app before-frame hooks 1.3 | first build and layout 2.3 | first raster and present 49.7 = 201.3 ms from process start
  - stages: medians ms: scale 0.02 plan 0.00 cpu 8.82 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 1.04 present 1.66
  - stages: p95 ms: scale 0.02 plan 0.00 cpu 10.17 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 1.63 present 2.34
  - worst frame: work 20.47 ms, outside the renderer 0.75 ms, on cpu5; stages ms: scale 0.02 plan 0.00 cpu 15.78 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 1.30 present 2.63
  - raster by core: cpu4 99 frames 8.94 ms | cpu6 66 frames 8.79 ms | cpu5 65 frames 8.53 ms | cpu7 10 frames 8.52 ms
  - clocks at report: cpu0 1.08/1.80 GHz | cpu1 1.08/1.80 GHz | cpu2 1.08/1.80 GHz | cpu3 1.08/1.80 GHz | cpu4 2.42/2.42 GHz | cpu5 2.42/2.42 GHz | cpu6 2.42/2.42 GHz | cpu7 0.84/2.84 GHz
- CPU path presented through Vulkan (diagnostic): median 11.492552ms, worst 20.71677ms, budget 16.666666ms (240 samples)
  - start-up: before run 101 ms | app setup 0.1 | event loop started 0.0 | resumed 16.6 | window created 0.0 | present ready 23.6 | accessibility and show 0.0 | tree built 1.1 | window ready 0.6 | until first draw 0.0 | app before-frame hooks 0.2 | first build and layout 1.4 | first raster and present 36.0 = 181.0 ms from process start
  - stages: medians ms: scale 0.02 plan 0.00 cpu 7.92 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.00 present 3.23
  - stages: p95 ms: scale 0.02 plan 0.00 cpu 9.36 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.00 present 4.53
  - worst frame: work 20.72 ms, outside the renderer 0.93 ms, on cpu4; stages ms: scale 0.08 plan 0.00 cpu 16.58 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.00 present 3.12
  - raster by core: cpu7 151 frames 7.73 ms | cpu5 32 frames 8.12 ms | cpu4 30 frames 8.16 ms | cpu6 27 frames 8.38 ms
  - clocks at report: cpu0 1.08/1.80 GHz | cpu1 1.08/1.80 GHz | cpu2 1.08/1.80 GHz | cpu3 1.08/1.80 GHz | cpu4 0.71/2.42 GHz | cpu5 0.71/2.42 GHz | cpu6 0.71/2.42 GHz | cpu7 2.84/2.84 GHz
- CPU path with an Android performance hint (diagnostic): median 11.312917ms, worst 18.286198ms, budget 16.666666ms (240 samples)
  - start-up: before run 97 ms | app setup 0.1 | event loop started 0.0 | resumed 15.2 | window created 0.0 | present ready 0.1 | accessibility and show 0.0 | tree built 1.2 | window ready 0.6 | until first draw 0.0 | app before-frame hooks 0.3 | first build and layout 1.5 | first raster and present 41.0 = 157.2 ms from process start
  - stages: medians ms: scale 0.02 plan 0.00 cpu 8.79 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.64 present 1.41
  - stages: p95 ms: scale 0.03 plan 0.00 cpu 10.12 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 1.18 present 2.09
  - worst frame: work 18.29 ms, outside the renderer 0.97 ms, on cpu4; stages ms: scale 0.02 plan 0.00 cpu 15.10 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.63 present 1.56
  - raster by core: cpu6 114 frames 8.90 ms | cpu5 59 frames 8.55 ms | cpu4 56 frames 8.72 ms | cpu7 11 frames 9.04 ms
  - clocks at report: cpu0 1.25/1.80 GHz | cpu1 1.25/1.80 GHz | cpu2 1.25/1.80 GHz | cpu3 1.25/1.80 GHz | cpu4 2.42/2.42 GHz | cpu5 2.42/2.42 GHz | cpu6 2.42/2.42 GHz | cpu7 0.84/2.84 GHz
  - performance hint: unavailable (the system declined a session)

**Samsung Galaxy S26 Ultra**
- CPU path: median 5.424688ms, worst 19.722187ms, budget 8.333333ms (240 samples)
  - start-up: before run 59 ms | app setup 0.0 | event loop started 0.0 | resumed 8.4 | window created 0.0 | present ready 0.0 | accessibility and show 0.0 | tree built 0.9 | window ready 3.4 | until first draw 0.0 | app before-frame hooks 0.2 | first build and layout 1.0 | first raster and present 14.1 = 87.1 ms from process start
  - stages: medians ms: scale 0.01 plan 0.00 cpu 5.14 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.02 present 0.18
  - stages: p95 ms: scale 0.02 plan 0.00 cpu 7.55 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.04 present 0.28
  - worst frame: work 19.72 ms, outside the renderer 0.26 ms, on cpu7; stages ms: scale 0.01 plan 0.00 cpu 18.86 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.04 present 0.55
  - raster by core: cpu6 124 frames 5.05 ms | cpu7 101 frames 5.14 ms | cpu4 6 frames 7.53 ms | cpu1 4 frames 7.55 ms | cpu0 3 frames 7.34 ms | cpu3 1 frames 8.19 ms | cpu5 1 frames 7.43 ms
  - clocks at report: cpu0 2.61/3.63 GHz | cpu1 2.61/3.63 GHz | cpu2 2.61/3.63 GHz | cpu3 2.61/3.63 GHz | cpu4 2.61/3.63 GHz | cpu5 2.61/3.63 GHz | cpu6 0.88/4.74 GHz | cpu7 0.88/4.74 GHz
- GPU path: median 2.1775ms, worst 11.276926ms, budget 8.333333ms (240 samples)
  - start-up: before run 64 ms | app setup 0.0 | event loop started 0.0 | resumed 12.5 | window created 0.0 | present ready 5.8 | accessibility and show 0.0 | tree built 1.0 | window ready 0.4 | until first draw 0.0 | app before-frame hooks 0.3 | first build and layout 0.9 | first raster and present 11.7 = 97.2 ms from process start
  - stages: medians ms: scale 0.02 plan 0.18 cpu 0.00 gpu_upload 0.53 gpu_record 0.02 gpu_submit 0.05 gpu_wait 1.03 gpu_readback 0.00 acquire 0.01 present 0.02
  - stages: p95 ms: scale 0.03 plan 0.23 cpu 0.00 gpu_upload 2.29 gpu_record 0.03 gpu_submit 0.08 gpu_wait 4.73 gpu_readback 0.00 acquire 0.01 present 0.04
  - worst frame: work 11.28 ms, outside the renderer 0.32 ms; stages ms: scale 0.02 plan 0.18 cpu 0.00 gpu_upload 0.69 gpu_record 0.02 gpu_submit 0.08 gpu_wait 9.86 gpu_readback 0.00 acquire 0.01 present 0.09
  - clocks at report: cpu0 1.44/3.63 GHz | cpu1 1.44/3.63 GHz | cpu2 1.44/3.63 GHz | cpu3 1.44/3.63 GHz | cpu4 1.44/3.63 GHz | cpu5 1.44/3.63 GHz | cpu6 0.88/4.74 GHz | cpu7 0.88/4.74 GHz
  - pipeline cache: started from 39551 bytes; saved 39551 bytes after the first GPU frame
- chosen by measurement (auto): median 2.362136ms, worst 7.127448ms, budget 8.333333ms (240 samples)
  - renderer: gpu (calibrated earlier: cpu 5.46 ms, gpu 1.56 ms)
  - calibration launches: launch 1: 10-17 18:05:59.645 24889 24907 I vieww   : vieww_platform_winit::rasterizer_choice: vieww: rasterizer cpu, measuring (first launch with this build here) 10-17 18:06:00.167 24889 24907 I vieww   : vieww_platform_winit::rasterizer_choice: vieww: rasterizer cpu for now; the gpu is measured next launch (measured: cpu 5.46 ms; budget 8.33 ms) launch 2: 10-17 18:06:08.112 25163 25180 I vieww   : vieww_platform_winit::rasterizer_choice: vieww: rasterizer gpu, measuring (the cpu path measured 5.46 ms last launch) 10-17 18:06:08.292 25163 25180 I vieww   : vieww_platform_winit::rasterizer_choice: vieww: rasterizer gpu (measured: cpu 5.46 ms, gpu 1.56 ms; budget 8.33 ms) launch 3: 10-17 18:06:16.713 25370 25387 I vieww   : vieww_platform_winit::rasterizer_choice: vieww: rasterizer gpu (calibrated earlier: cpu 5.46 ms, gpu 1.56 ms)
  - start-up: before run 70 ms | app setup 0.0 | event loop started 0.0 | resumed 9.8 | window created 0.0 | present ready 5.6 | accessibility and show 0.0 | tree built 1.2 | window ready 0.5 | until first draw 0.0 | app before-frame hooks 0.1 | first build and layout 1.0 | first raster and present 12.5 = 101.1 ms from process start
  - stages: medians ms: scale 0.02 plan 0.19 cpu 0.00 gpu_upload 0.56 gpu_record 0.02 gpu_submit 0.06 gpu_wait 1.11 gpu_readback 0.00 acquire 0.01 present 0.02
  - stages: p95 ms: scale 0.03 plan 0.25 cpu 0.00 gpu_upload 2.46 gpu_record 0.04 gpu_submit 0.09 gpu_wait 5.29 gpu_readback 0.00 acquire 0.01 present 0.04
  - worst frame: work 7.13 ms, outside the renderer 0.23 ms; stages ms: scale 0.02 plan 0.21 cpu 0.00 gpu_upload 0.83 gpu_record 0.03 gpu_submit 0.14 gpu_wait 5.63 gpu_readback 0.00 acquire 0.01 present 0.03
  - clocks at report: cpu0 1.23/3.63 GHz | cpu1 1.23/3.63 GHz | cpu2 1.23/3.63 GHz | cpu3 1.23/3.63 GHz | cpu4 1.23/3.63 GHz | cpu5 1.23/3.63 GHz | cpu6 0.88/4.74 GHz | cpu7 0.88/4.74 GHz
  - pipeline cache: started from 39551 bytes; saved 39551 bytes after the first GPU frame
- CPU path presented through Vulkan (diagnostic): median 4.291094ms, worst 8.343959ms, budget 8.333333ms (240 samples)
  - start-up: before run 55 ms | app setup 0.2 | event loop started 0.0 | resumed 7.8 | window created 0.0 | present ready 5.3 | accessibility and show 0.0 | tree built 0.8 | window ready 0.4 | until first draw 0.0 | app before-frame hooks 0.1 | first build and layout 1.4 | first raster and present 14.0 = 85.1 ms from process start
  - stages: medians ms: scale 0.01 plan 0.00 cpu 3.07 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.00 present 1.11
  - stages: p95 ms: scale 0.01 plan 0.00 cpu 3.66 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.00 present 2.60
  - worst frame: work 8.34 ms, outside the renderer 0.23 ms, on cpu1; stages ms: scale 0.01 plan 0.00 cpu 6.83 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.00 present 1.27
  - raster by core: cpu6 237 frames 3.05 ms | cpu1 3 frames 5.75 ms
  - clocks at report: cpu0 3.63/3.63 GHz | cpu1 3.63/3.63 GHz | cpu2 3.63/3.63 GHz | cpu3 3.63/3.63 GHz | cpu4 3.63/3.63 GHz | cpu5 3.63/3.63 GHz | cpu6 0.88/4.74 GHz | cpu7 0.88/4.74 GHz
- CPU path with an Android performance hint (diagnostic): median 5.495416ms, worst 21.012917ms, budget 8.333333ms (240 samples)
  - start-up: before run 58 ms | app setup 0.0 | event loop started 0.0 | resumed 9.0 | window created 0.0 | present ready 0.0 | accessibility and show 0.0 | tree built 1.1 | window ready 1.9 | until first draw 0.2 | app before-frame hooks 0.2 | first build and layout 1.4 | first raster and present 16.9 = 89.0 ms from process start
  - stages: medians ms: scale 0.01 plan 0.00 cpu 5.17 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.02 present 0.19
  - stages: p95 ms: scale 0.01 plan 0.00 cpu 7.94 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.04 present 0.27
  - worst frame: work 21.01 ms, outside the renderer 0.27 ms, on cpu6; stages ms: scale 0.01 plan 0.00 cpu 20.26 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.03 present 0.44
  - raster by core: cpu6 125 frames 4.90 ms | cpu7 99 frames 5.22 ms | cpu5 7 frames 7.35 ms | cpu1 5 frames 7.68 ms | cpu4 2 frames 8.87 ms | cpu0 1 frames 7.23 ms | cpu3 1 frames 12.63 ms
  - clocks at report: cpu0 2.23/3.63 GHz | cpu1 2.23/3.63 GHz | cpu2 2.23/3.63 GHz | cpu3 2.23/3.63 GHz | cpu4 2.23/3.63 GHz | cpu5 2.23/3.63 GHz | cpu6 0.88/4.74 GHz | cpu7 0.88/4.74 GHz
  - performance hint: on (target 8.33 ms)

Targets from the audit, for reading the table: product UI p95 under 15 ms and cold start under 120 ms on a mid-tier phone; release APK under 6 MB.
