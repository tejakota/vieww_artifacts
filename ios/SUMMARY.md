# vieww on AWS Device Farm

run: `arn:aws:devicefarm:us-west-2:278061313767:run:393108fd-d195-4dad-acb4-12581f7a212f/9978a0d1-a10d-4457-9eb0-5bfd75870394` — PASSED

| phone | tier | Android | SoC / GPU | RAM | APK | cold start cpu / gpu (ms, median of 3) | frame time cpu | frame time gpu | frames drawn by the GPU, cpu pass / gpu pass | PSS cpu / gpu (MB) | suite cpu / gpu |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Apple iPhone 13 | low | 16.0 | ?<br>? | ? | 4.6 MB | – / – | median 8.152166ms, worst 11.083625ms, budget 16.666668ms (240 samples) | median 16.170792ms, worst 27.140417ms, budget 16.666668ms (240 samples) | 0 of 600 / 600 of 600 | – / – | PASS / PASS |
| Apple iPhone 15 Plus | mid | 17.4 | ?<br>? | ? | 4.6 MB | 357 / 374 | median 5.740667ms, worst 11.0575ms, budget 16.666668ms (240 samples) | median 16.272917ms, worst 32.812584ms, budget 16.666668ms (240 samples) | 0 of 600 / 600 of 600 | – / – | PASS / PASS |
| Apple iPhone 18 Pro Max | high | 27.0 | ?<br>? | ? | 4.6 MB | 275 / 292 | median 5.227166ms, worst 10.545542ms, budget 8.333334ms (240 samples) | median 8.120667ms, worst 19.048041ms, budget 8.333334ms (240 samples) | 0 of 600 / 600 of 600 | – / – | PASS / PASS |

**Apple iPhone 13**
- CPU path: median 8.152166ms, worst 11.083625ms, budget 16.666668ms (240 samples)
  - start-up: before run 30 ms | app setup 2.0 | event loop started 54.7 | resumed 248.3 | window created 22.2 | present ready 1.4 | accessibility and show 0.6 | tree built 10.0 | window ready 0.2 | until first draw 0.9 | app before-frame hooks 0.1 | first build and layout 3.8 | first raster and present 27.6 = 401.7 ms from process start
  - stages: medians ms: scale 0.02 plan 0.00 cpu 4.66 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.00 present 3.29
  - stages: p95 ms: scale 0.02 plan 0.00 cpu 5.07 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.00 present 3.88
  - worst frame: work 11.08 ms, outside the renderer 0.19 ms; stages ms: scale 0.02 plan 0.00 cpu 7.82 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.00 present 3.05
- GPU path: median 16.170792ms, worst 27.140417ms, budget 16.666668ms (240 samples)
  - start-up: before run 12 ms | app setup 0.3 | event loop started 66.4 | resumed 247.1 | window created 20.7 | present ready 263.9 | accessibility and show 0.2 | tree built 1.6 | window ready 0.1 | until first draw 3.4 | app before-frame hooks 0.0 | first build and layout 1.1 | first raster and present 330.0 = 946.7 ms from process start
  - stages: medians ms: scale 0.10 plan 0.48 cpu 0.00 gpu_upload 0.02 gpu_record 0.01 gpu_submit 6.91 gpu_wait 7.62 gpu_readback 0.00 acquire 0.05 present 0.08
  - stages: p95 ms: scale 0.11 plan 0.52 cpu 0.00 gpu_upload 0.03 gpu_record 0.02 gpu_submit 8.53 gpu_wait 8.53 gpu_readback 0.00 acquire 0.07 present 0.10
  - worst frame: work 27.14 ms, outside the renderer 0.16 ms; stages ms: scale 0.01 plan 0.06 cpu 0.00 gpu_upload 0.01 gpu_record 0.00 gpu_submit 19.12 gpu_wait 7.72 gpu_readback 0.00 acquire 0.01 present 0.04
  - pipeline cache: started from 0 bytes; saved 41701 bytes after the first GPU frame
- chosen by measurement (auto): median 8.596ms, worst 11.801501ms, budget 16.666668ms (240 samples)
  - renderer: cpu (measured: cpu 7.92 ms; budget 16.67 ms)
  - start-up: before run 11 ms | app setup 0.3 | event loop started 52.9 | resumed 261.4 | window created 24.3 | present ready 0.7 | accessibility and show 0.3 | tree built 3.7 | window ready 0.3 | until first draw 1.3 | app before-frame hooks 0.0 | first build and layout 2.5 | first raster and present 24.3 = 382.8 ms from process start
  - stages: medians ms: scale 0.02 plan 0.00 cpu 4.93 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.00 present 3.50
  - stages: p95 ms: scale 0.02 plan 0.00 cpu 5.58 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.00 present 4.07
  - worst frame: work 11.80 ms, outside the renderer 0.18 ms; stages ms: scale 0.02 plan 0.00 cpu 7.87 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.00 present 3.73

**Apple iPhone 15 Plus**
- CPU path: median 5.740667ms, worst 11.0575ms, budget 16.666668ms (240 samples)
  - start-up: before run 10 ms | app setup 2.7 | event loop started 84.0 | resumed 227.5 | window created 17.9 | present ready 0.3 | accessibility and show 0.4 | tree built 2.2 | window ready 3.1 | until first draw 0.2 | app before-frame hooks 0.0 | first build and layout 1.0 | first raster and present 18.3 = 367.9 ms from process start
  - stages: medians ms: scale 0.01 plan 0.00 cpu 2.99 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.10 present 2.40
  - stages: p95 ms: scale 0.02 plan 0.00 cpu 5.65 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.17 present 4.21
  - worst frame: work 11.06 ms, outside the renderer 0.26 ms; stages ms: scale 0.02 plan 0.00 cpu 6.39 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.16 present 4.23
- GPU path: median 16.272917ms, worst 32.812584ms, budget 16.666668ms (240 samples)
  - start-up: before run 11 ms | app setup 2.6 | event loop started 85.9 | resumed 223.5 | window created 15.1 | present ready 12.7 | accessibility and show 0.1 | tree built 1.4 | window ready 3.2 | until first draw 1.0 | app before-frame hooks 0.0 | first build and layout 0.9 | first raster and present 21.9 = 378.9 ms from process start
  - stages: medians ms: scale 0.07 plan 0.43 cpu 0.00 gpu_upload 0.02 gpu_record 0.01 gpu_submit 6.91 gpu_wait 8.04 gpu_readback 0.00 acquire 0.03 present 0.05
  - stages: p95 ms: scale 0.07 plan 0.49 cpu 0.00 gpu_upload 0.03 gpu_record 0.02 gpu_submit 8.33 gpu_wait 10.38 gpu_readback 0.00 acquire 0.05 present 0.08
  - worst frame: work 32.81 ms, outside the renderer 0.35 ms; stages ms: scale 0.03 plan 0.22 cpu 0.00 gpu_upload 0.01 gpu_record 0.01 gpu_submit 21.52 gpu_wait 10.59 gpu_readback 0.00 acquire 0.02 present 0.06
  - pipeline cache: started from 41701 bytes; saved 41701 bytes after the first GPU frame
- chosen by measurement (auto): median 5.913542ms, worst 11.259542ms, budget 16.666668ms (240 samples)
  - renderer: cpu (calibrated earlier: cpu 5.31 ms)
  - start-up: before run 11 ms | app setup 2.6 | event loop started 82.2 | resumed 234.0 | window created 19.3 | present ready 0.7 | accessibility and show 0.2 | tree built 2.4 | window ready 3.6 | until first draw 0.2 | app before-frame hooks 0.0 | first build and layout 0.9 | first raster and present 16.9 = 373.7 ms from process start
  - stages: medians ms: scale 0.01 plan 0.00 cpu 3.01 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.10 present 2.62
  - stages: p95 ms: scale 0.02 plan 0.00 cpu 5.46 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.16 present 4.15
  - worst frame: work 11.26 ms, outside the renderer 0.30 ms; stages ms: scale 0.03 plan 0.00 cpu 6.59 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.16 present 4.18

**Apple iPhone 18 Pro Max**
- CPU path: median 5.227166ms, worst 10.545542ms, budget 8.333334ms (240 samples)
  - start-up: before run 8 ms | app setup 2.0 | event loop started 82.1 | resumed 166.7 | window created 5.5 | present ready 0.1 | accessibility and show 0.1 | tree built 1.1 | window ready 2.7 | until first draw 0.7 | app before-frame hooks 0.0 | first build and layout 0.7 | first raster and present 10.9 = 280.4 ms from process start
  - stages: medians ms: scale 0.02 plan 0.00 cpu 3.62 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.12 present 1.27
  - stages: p95 ms: scale 0.02 plan 0.00 cpu 4.69 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.16 present 3.33
  - worst frame: work 10.55 ms, outside the renderer 0.38 ms; stages ms: scale 0.02 plan 0.00 cpu 7.56 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.21 present 2.37
- GPU path: median 8.120667ms, worst 19.048041ms, budget 8.333334ms (240 samples)
  - start-up: before run 8 ms | app setup 2.0 | event loop started 76.0 | resumed 176.9 | window created 5.8 | present ready 7.2 | accessibility and show 0.1 | tree built 1.0 | window ready 2.8 | until first draw 1.4 | app before-frame hooks 0.0 | first build and layout 0.7 | first raster and present 14.0 = 296.1 ms from process start
  - stages: medians ms: scale 0.03 plan 0.18 cpu 0.00 gpu_upload 0.01 gpu_record 0.01 gpu_submit 5.51 gpu_wait 1.80 gpu_readback 0.00 acquire 0.03 present 0.03
  - stages: p95 ms: scale 0.05 plan 0.26 cpu 0.00 gpu_upload 0.03 gpu_record 0.02 gpu_submit 14.01 gpu_wait 3.97 gpu_readback 0.00 acquire 0.04 present 0.06
  - worst frame: work 19.05 ms, outside the renderer 0.17 ms; stages ms: scale 0.02 plan 0.09 cpu 0.00 gpu_upload 0.01 gpu_record 0.00 gpu_submit 16.85 gpu_wait 1.86 gpu_readback 0.00 acquire 0.01 present 0.04
  - pipeline cache: started from 41701 bytes; saved 41701 bytes after the first GPU frame
- chosen by measurement (auto): median 3.023751ms, worst 11.040375ms, budget 8.333334ms (240 samples)
  - renderer: cpu (calibrated earlier: cpu 2.58 ms)
  - start-up: before run 9 ms | app setup 2.0 | event loop started 89.9 | resumed 170.2 | window created 4.2 | present ready 0.2 | accessibility and show 0.1 | tree built 0.8 | window ready 2.3 | until first draw 0.9 | app before-frame hooks 0.0 | first build and layout 0.6 | first raster and present 9.4 = 289.5 ms from process start
  - stages: medians ms: scale 0.01 plan 0.00 cpu 1.61 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.06 present 1.29
  - stages: p95 ms: scale 0.02 plan 0.00 cpu 3.76 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.14 present 3.23
  - worst frame: work 11.04 ms, outside the renderer 0.17 ms; stages ms: scale 0.02 plan 0.00 cpu 4.76 gpu_upload 0.00 gpu_record 0.00 gpu_submit 0.00 gpu_wait 0.00 gpu_readback 0.00 acquire 0.25 present 5.83

Targets from the audit, for reading the table: product UI p95 under 15 ms and cold start under 120 ms on a mid-tier phone; release APK under 6 MB.
