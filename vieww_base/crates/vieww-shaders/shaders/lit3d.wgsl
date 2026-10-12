// The lit 3D forward pass — `vieww-3d`'s `GpuRenderer`'s second increment,
// dispatched by `vieww-hal::vulkan::VulkanDevice::render_lit3d_to_pixels`.
//
// It is `vieww_3d::render`'s `shade` for the two shadings it covers, written
// out line for line so the GPU sums the same terms in the same order:
//
//   Basic    albedo + emissive
//   Lambert  sum over lights, in scene traversal order, of
//              Ambient      albedo * color * (intensity * exposure)
//              Hemisphere   albedo * lerp(ground, sky, 0.5 * (n.y + 1))
//                                  * (intensity * exposure)
//              Directional  (color * intensity) * (exposure * visibility)
//                                  * (albedo * max(n . -dir, 0))
//            + emissive
//
// then the same sRGB encode `unlit3d.wgsl` uses. Image-based lighting, GI
// probes, point and spot lights are not here: a scene that has them falls
// back to the CPU for the whole frame (see `vieww-3d/src/gpu.rs`).
//
// # Shadows
//
// `visibility` is the CPU's `ShadowMap::visibility`: the world point through
// the light's orthographic view-projection, 3x3 percentage-closer filtering
// on texels found by truncating the texel coordinate (as the CPU's `as i64`
// does), and the CPU's slope-scaled bias. The depth map itself is the CPU
// renderer's own `shadow_pass` output, uploaded as a storage buffer — so the
// two renderers make the same shadow decision for the same point; rendering
// the map on the GPU is a later step that would have to match it.
//
// # Back faces
//
// Decided per triangle on the CPU while packing (`gpu.rs`): a back face of a
// single-sided material is not packed at all, and a back face of a
// double-sided one arrives with its normals already flipped — the CPU
// rasterizer's `if back { n = -n }`. So there is no culling state here, and
// no facing test.
//
// Coordinate conventions are `unlit3d.wgsl`'s; see that file.

struct Light {
    // rgb = colour (the sky colour for a hemisphere light), a = kind:
    // 0 ambient, 1 hemisphere, 2 directional.
    color_kind: vec4<f32>,
    // xyz = world direction the light travels (directional), w = intensity.
    dir_intensity: vec4<f32>,
    // rgb = ground colour (hemisphere), w = 1 if this light uses the shadow map.
    ground_shadow: vec4<f32>,
};

struct Uniforms {
    vp: mat4x4<f32>,
    shadow_vp: mat4x4<f32>,
    // xyz = camera position (unused by Lambert, kept for the next shadings),
    // w = exposure.
    eye_exposure: vec4<f32>,
    // x = light count, y = shadow map size, z = 1 if there is a shadow map.
    params: vec4<f32>,
    lights: array<Light, 8>,
};

@group(0) @binding(0)
var<uniform> u: Uniforms;

@group(0) @binding(1)
var<storage, read> shadow_depth: array<f32>;

struct VertexIn {
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) albedo: vec3<f32>,
    @location(3) emissive: vec3<f32>,
    // 1 for Lambert, 0 for Basic.
    @location(4) lambert: f32,
    // 1 if the material receives shadows.
    @location(5) receive: f32,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) @interpolate(flat) albedo: vec3<f32>,
    @location(3) @interpolate(flat) emissive: vec3<f32>,
    // x = lambert, y = receive.
    @location(4) @interpolate(flat) flags: vec2<f32>,
};

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    let clip = u.vp * vec4<f32>(v.world, 1.0);
    var out: VertexOut;
    out.position = vec4<f32>(clip.x, clip.y, (clip.z + clip.w) * 0.5, clip.w);
    out.world = v.world;
    out.normal = v.normal;
    out.albedo = v.albedo;
    out.emissive = v.emissive;
    out.flags = vec2<f32>(v.lambert, v.receive);
    return out;
}

fn visibility(p: vec3<f32>, bias: f32) -> f32 {
    let c = u.shadow_vp * vec4<f32>(p, 1.0);
    if (c.x < -1.0 || c.x > 1.0 || c.y < -1.0 || c.y > 1.0 || c.z > 1.0) {
        return 1.0;
    }
    let s = u.params.y;
    let size = i32(s);
    let px = (c.x * 0.5 + 0.5) * s;
    let py = (0.5 - c.y * 0.5) * s;
    // Truncation toward zero, as the CPU's `px as i64`.
    let ix = i32(px);
    let iy = i32(py);
    var lit = 0.0;
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let sx = clamp(ix + dx, 0, size - 1);
            let sy = clamp(iy + dy, 0, size - 1);
            if (c.z - bias <= shadow_depth[sy * size + sx]) {
                lit += 1.0;
            }
        }
    }
    return lit / 9.0;
}

fn lambert(albedo: vec3<f32>, p: vec3<f32>, n: vec3<f32>, receive: bool) -> vec3<f32> {
    let exposure = u.eye_exposure.w;
    var total = vec3<f32>(0.0);
    let count = i32(u.params.x);
    for (var i = 0; i < count; i++) {
        let l = u.lights[i];
        let kind = l.color_kind.a;
        let color = l.color_kind.rgb;
        let intensity = l.dir_intensity.w;
        if (kind < 0.5) {
            total = total + albedo * color * (intensity * exposure);
            continue;
        }
        if (kind < 1.5) {
            let k = 0.5 * (n.y + 1.0);
            let ground = l.ground_shadow.rgb;
            let sky = color;
            total = total + albedo * (ground + (sky - ground) * k) * (intensity * exposure);
            continue;
        }
        let dir = l.dir_intensity.xyz;
        var vis = 1.0;
        if (l.ground_shadow.w > 0.5 && u.params.z > 0.5 && receive) {
            let ndl0 = max(dot(n, -dir), 0.0);
            vis = visibility(p, 0.002 + 0.01 * (1.0 - ndl0));
        }
        let ndl = max(dot(n, -dir), 0.0);
        if (ndl <= 0.0 || vis <= 0.0) {
            continue;
        }
        let radiance = (color * intensity) * (exposure * vis);
        total = total + radiance * (albedo * ndl);
    }
    return total;
}

@fragment
fn fs_main(f: VertexOut) -> @location(0) vec4<f32> {
    var out: vec3<f32>;
    if (f.flags.x > 0.5) {
        let n = normalize(f.normal);
        out = lambert(f.albedo, f.world, n, f.flags.y > 0.5) + f.emissive;
    } else {
        out = f.albedo + f.emissive;
    }
    let c = clamp(out, vec3<f32>(0.0), vec3<f32>(1.0));
    let linear = c * 12.92;
    let gamma = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    let s = select(linear, gamma, c > vec3<f32>(0.0031308));
    return vec4<f32>(s, 1.0);
}
