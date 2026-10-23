// The unlit 3D forward pass — `vieww-3d`'s `GpuRenderer`'s first increment,
// dispatched by `vieww-hal::vulkan::VulkanDevice::render_unlit3d_to_pixels`.
//
// What comes in: world-space triangles (positions at @location(0), linear
// colours at @location(1) — one flat material colour per vertex, repeated),
// and a view-projection matrix in a uniform. What goes out: the depth-tested
// nearest surface colour, sRGB-encoded, alpha 1 — the same bytes
// `vieww_3d::render::Renderer::render` produces for a scene whose materials
// are all `Shading::Basic`, which is the parity suite's oracle.
//
// # The two conventions the vertex shader reconciles
//
// vieww-3d's `Mat4::perspective`/`orthographic` are GL-style: NDC y is up and
// NDC z spans [-1, 1]. WGSL is authored in WebGPU's clip space, where y is
// also up — so the matrix multiplies straight through with no flip here;
// naga's `ADJUST_COORDINATE_SPACE` (on in `vieww_shaders::compiler`'s Vulkan
// options, the same flag `solid.wgsl` relies on) flips y on the way out for
// Vulkan, which is what makes the framebuffer rows land in the same order
// the CPU rasterizer's (0.5 - ndc.y * 0.5) mapping puts them.
//
// z is the one axis that does need help: WGSL/WebGPU and Vulkan both want
// depth in [0, 1], so the remap (clip.z + clip.w) * 0.5 — done in clip space,
// before the divide, so it stays perspective-correct — moves the near plane
// from -w to 0 and far from +w to w while preserving near < far, which is the
// order the pipeline's LESS depth compare and the CPU's per-sample depth test
// agree on.

struct Uniforms {
    // Column-major — the exact layout vieww-3d's Mat4 flattens to (m is
    // stored column-at-a-time), so the bytes a caller uploads with no
    // transposition multiply like `Mat4::mul_vec4` does.
    vp: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> u: Uniforms;

struct VertexIn {
    @location(0) world: vec3<f32>,
    @location(1) color: vec3<f32>,
};

struct VertexOut {
    @location(0) color: vec3<f32>,
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    let clip = u.vp * vec4<f32>(v.world, 1.0);
    var out: VertexOut;
    out.position = vec4<f32>(clip.x, clip.y, (clip.z + clip.w) * 0.5, clip.w);
    out.color = v.color;
    return out;
}

@fragment
fn fs_main(f: VertexOut) -> @location(0) vec4<f32> {
    // Linear colour, alpha 1: the target holds linear f16 and the readback
    // encodes — the CPU resolve chain is: linear colour -> srgb() (12.92
    // below 0.0031308, else 1.055 * c^(1/2.4) - 0.055) -> * 255 rounded,
    // which the shared encode does on the CPU exactly as `to_image` does.
    // (The shader encoded when the target was UNORM RGBA8; the target moved
    // to f16 so the lit pass could blend in linear space, and the encode
    // moved with the readback.)
    return vec4<f32>(f.color, 1.0);
}
