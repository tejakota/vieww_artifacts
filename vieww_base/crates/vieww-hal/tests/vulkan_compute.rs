//! Compute-shader suite — the WebGPU row's compute capability on vieww's own
//! Vulkan backend: WGSL kernels translated by naga, run on the device, read
//! back, and checked against the same arithmetic on the CPU.
//!
//! `#[ignore]`d like the other Vulkan suites: run with
//! `cargo test -p vieww-hal --features vulkan -- --ignored --test-threads=1`.

#![cfg(feature = "vulkan")]

use vieww_hal::vulkan::{ComputeBuffer, ComputeKernel, VulkanDevice};

fn gpu() -> Option<VulkanDevice> {
    match VulkanDevice::new() {
        Ok(d) => Some(d),
        Err(e) => {
            eprintln!("skipping: no Vulkan device ({e})");
            None
        }
    }
}

#[test]
#[ignore = "needs a Vulkan loader + ICD; run with `cargo test -p vieww-hal --features vulkan -- --ignored --test-threads=1`"]
fn saxpy_matches_the_cpu() {
    let Some(gpu) = gpu() else { return };
    let wgsl = "
        @group(0) @binding(0) var<storage, read> x: array<f32>;
        @group(0) @binding(1) var<storage, read_write> y: array<f32>;
        @compute @workgroup_size(64)
        fn main(@builtin(global_invocation_id) id: vec3<u32>) {
            let i = id.x;
            if (i < arrayLength(&y)) { y[i] = 2.5 * x[i] + y[i]; }
        }";
    let n = 1000;
    let xs: Vec<f32> = (0..n).map(|i| i as f32 * 0.5).collect();
    let ys: Vec<f32> = (0..n).map(|i| (i as f32).sin()).collect();
    let k = ComputeKernel::new(&gpu, wgsl, "main", 2).expect("kernel");
    let bx = ComputeBuffer::from_f32(&gpu, &xs).expect("x");
    let by = ComputeBuffer::from_f32(&gpu, &ys).expect("y");
    k.dispatch(&gpu, &[&bx, &by], [(n as u32).div_ceil(64), 1, 1]).expect("dispatch");
    let out = by.read_f32(&gpu).expect("read");
    for i in 0..n {
        assert!((out[i] - (2.5 * xs[i] + ys[i])).abs() < 1e-4, "{i}");
    }
}

#[test]
#[ignore = "needs a Vulkan loader + ICD; run with `cargo test -p vieww-hal --features vulkan -- --ignored --test-threads=1`"]
fn a_gpu_particle_system_integrates_like_the_cpu_over_many_steps() {
    let Some(gpu) = gpu() else { return };
    // Position/velocity pairs as vec4 (xy pos, zw vel); a floor bounce.
    let wgsl = "
        struct P { dt: f32, g: f32, damp: f32, pad: f32 };
        var<immediate> pc: P;
        @group(0) @binding(0) var<storage, read_write> parts: array<vec4<f32>>;
        @compute @workgroup_size(64)
        fn step(@builtin(global_invocation_id) id: vec3<u32>) {
            let i = id.x;
            if (i >= arrayLength(&parts)) { return; }
            var p = parts[i];
            p.w = p.w - pc.g * pc.dt;
            p.x = p.x + p.z * pc.dt;
            p.y = p.y + p.w * pc.dt;
            if (p.y < 0.0) { p.y = -p.y; p.w = -p.w * pc.damp; }
            parts[i] = p;
        }";
    let n = 4096usize;
    let mut cpu: Vec<[f32; 4]> = (0..n)
        .map(|i| {
            let f = i as f32;
            [f * 0.01, 1.0 + (f * 0.37).sin().abs() * 3.0, (f * 0.13).cos(), (f * 0.71).sin() * 2.0]
        })
        .collect();
    let flat: Vec<f32> = cpu.iter().flatten().copied().collect();
    let k = ComputeKernel::with_push_constants(&gpu, wgsl, "step", 1, 16).expect("kernel");
    let buf = ComputeBuffer::from_f32(&gpu, &flat).expect("buf");
    let (dt, g, damp) = (1.0f32 / 120.0, 9.81f32, 0.8f32);
    let push: Vec<u8> = [dt, g, damp, 0.0].iter().flat_map(|v| v.to_le_bytes()).collect();
    let steps: Vec<([u32; 3], Vec<u8>)> = (0..240).map(|_| ([(n as u32).div_ceil(64), 1, 1], push.clone())).collect();
    k.dispatch_many(&gpu, &[&buf], &steps).expect("dispatch");
    for _ in 0..240 {
        for p in &mut cpu {
            p[3] -= g * dt;
            p[0] += p[2] * dt;
            p[1] += p[3] * dt;
            if p[1] < 0.0 {
                p[1] = -p[1];
                p[3] = -p[3] * damp;
            }
        }
    }
    let out = buf.read_f32(&gpu).expect("read");
    let mut worst = 0.0f32;
    for (i, p) in cpu.iter().enumerate() {
        for c in 0..4 {
            worst = worst.max((out[i * 4 + c] - p[c]).abs());
        }
    }
    assert!(worst < 1e-3, "worst divergence {worst}");
}

#[test]
#[ignore = "needs a Vulkan loader + ICD; run with `cargo test -p vieww-hal --features vulkan -- --ignored --test-threads=1`"]
fn a_workgroup_reduction_uses_shared_memory_and_barriers() {
    let Some(gpu) = gpu() else { return };
    let wgsl = "
        @group(0) @binding(0) var<storage, read> input: array<u32>;
        @group(0) @binding(1) var<storage, read_write> sums: array<u32>;
        var<workgroup> scratch: array<u32, 256>;
        @compute @workgroup_size(256)
        fn reduce(@builtin(local_invocation_id) l: vec3<u32>, @builtin(workgroup_id) g: vec3<u32>) {
            let i = g.x * 256u + l.x;
            scratch[l.x] = input[i];
            workgroupBarrier();
            var stride = 128u;
            loop {
                if (stride == 0u) { break; }
                if (l.x < stride) { scratch[l.x] = scratch[l.x] + scratch[l.x + stride]; }
                workgroupBarrier();
                stride = stride / 2u;
            }
            if (l.x == 0u) { sums[g.x] = scratch[0]; }
        }";
    let groups = 16usize;
    let data: Vec<u32> = (0..groups * 256).map(|i| (i as u32 * 2_654_435_761) >> 20).collect();
    let k = ComputeKernel::new(&gpu, wgsl, "reduce", 2).expect("kernel");
    let input = ComputeBuffer::from_u32(&gpu, &data).expect("in");
    let sums = ComputeBuffer::from_u32(&gpu, &vec![0; groups]).expect("out");
    k.dispatch(&gpu, &[&input, &sums], [groups as u32, 1, 1]).expect("dispatch");
    let got = sums.read_u32(&gpu).expect("read");
    for (gi, s) in got.iter().enumerate() {
        let want: u32 = data[gi * 256..(gi + 1) * 256].iter().sum();
        assert_eq!(*s, want, "group {gi}");
    }
}

#[test]
#[ignore = "needs a Vulkan loader + ICD; run with `cargo test -p vieww-hal --features vulkan -- --ignored --test-threads=1`"]
fn a_wrong_binding_count_is_refused_not_crashed() {
    let Some(gpu) = gpu() else { return };
    let wgsl = "@group(0) @binding(0) var<storage, read_write> v: array<f32>;
        @compute @workgroup_size(1) fn main() { v[0] = 1.0; }";
    let k = ComputeKernel::new(&gpu, wgsl, "main", 1).expect("kernel");
    assert!(k.dispatch(&gpu, &[], [1, 1, 1]).is_err());
    assert!(ComputeKernel::new(&gpu, "not wgsl", "main", 1).is_err());
}
