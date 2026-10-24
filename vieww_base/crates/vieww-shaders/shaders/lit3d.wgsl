// The lit 3D forward pass — `vieww-3d`'s `GpuRenderer`'s second increment,
// dispatched by `vieww_hal::vulkan::VulkanDevice::render_lit3d_to_pixels`.
//
// It is `vieww_3d::render`'s `shade` for the three shadings it covers, written
// out line for line so the GPU sums the same terms in the same order:
//
//   Basic    albedo + emissive
//   Lambert  sum over lights, in scene traversal order, of
//              Ambient      albedo * color * (intensity * exposure)
//              Hemisphere   albedo * lerp(ground, sky, 0.5 * (n.y + 1))
//                                  * (intensity * exposure)
//              Directional  (color * intensity) * (exposure * visibility)
//                                  * (albedo * max(n . -dir, 0))
//              Point        color * (intensity * fall / (1 + dist^2 * 0.02))
//                                  * exposure * (albedo * max(n . l, 0))
//              Spot         color * (intensity * fall * cone^2)
//                                  * exposure * (albedo * max(n . l, 0))
//            + emissive
//   Phong    the same sum with, per directional/point/spot light, the CPU's
//              spec = pow(max(n . halfvector, 0), max(shininess, 1)) * specular
//            added in white under the same radiance — contribution
//              radiance * (albedo * ndl + vec3(spec))
//            under point and spot lights whose falloff and cone are the CPU's
//            formulas verbatim (see `distance_falloff` and the spot branch),
//            + emissive
//
// Textures multiply into `albedo` and set the starting alpha exactly as the
// CPU's `shade` does — `albedo = albedo.mul(texel.rgb)`, `alpha = texel.a` —
// sampled with the material's own filter (nearest or bilinear, repeat
// addressing, v = 0 the top row) at mip level 0, which is the CPU's
// `Texture::sample` arithmetic.
//
// Then the output is the shaded colour in LINEAR light, alpha `texel.a ×
// opacity` alongside it, and nothing is sRGB-encoded here: the target holds
// linear f16, the CPU oracle lerps linear colours before encoding once at
// `to_image`, and the readback encodes the same way. Blending — when a
// batch draws with it on — is straight-alpha source-over in that linear
// space, which is `NativeTarget::lerp(color, a)` to the bit.
//
// # The transparent split
//
// The CPU's per-fragment rule is: `a >= 1.0` writes colour AND depth, `a <
// 1.0` blends the colour and writes no depth. Fixed-function blending cannot
// switch depth writes per fragment, so each batch draws TWICE — once with
// `push.alpha_pass = 0` (keep `a >= 1.0`, depth writes on, blending off)
// and once with `push.alpha_pass = 1` (keep `a < 1.0`, depth writes off,
// blending on) — two disjoint fragment sets, the same pixels as the CPU's
// single pass. A material that cannot produce one side of the split skips
// that draw; see `vieww-3d/src/gpu.rs`.
//
// Image-based lighting, GI probes, fog and skyboxes are not here: a scene
// that has them falls back to the CPU for the whole frame (see
// `vieww-3d/src/gpu.rs`).
//
// # Shadows
//
// `visibility` is the CPU's `ShadowMap::visibility`: the world point through
// the light's orthographic view-projection, 3x3 percentage-closer filtering
// on texels found by truncating the texel coordinate (as the CPU's `as i64`
// does), and the CPU's slope-scaled bias. The depth map itself is the CPU
// renderer's own `shadow_pass` output, uploaded as a storage buffer — so the
// two renderers make the same shadow decision for the same point; rendering
// the map on the GPU is a later step that would have to match it. Only the
// shadow-casting directional light uses it, exactly as the CPU's shade loop
// does: point and spot lights carry `shadowed = 1.0` there.
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
    // 0 ambient, 1 hemisphere, 2 directional, 3 point, 4 spot.
    color_kind: vec4<f32>,
    // xyz = world direction the light travels (directional, spot),
    // w = intensity.
    dir_intensity: vec4<f32>,
    // rgb = ground colour (hemisphere), w = 1 if this light uses the shadow map.
    ground_shadow: vec4<f32>,
    // xyz = world position (point, spot), w = range (0 = no distance falloff).
    pos_range: vec4<f32>,
    // x = cos of the spot's outer angle, y = cos of its inner angle
    // (angle * (1 - penumbra)); zw unused.
    cone: vec4<f32>,
};

struct Uniforms {
    vp: mat4x4<f32>,
    shadow_vp: mat4x4<f32>,
    // xyz = camera position, w = exposure.
    eye_exposure: vec4<f32>,
    // x = light count, y = shadow map size, z = 1 if there is a shadow map.
    params: vec4<f32>,
    lights: array<Light, 8>,
};

@group(0) @binding(0)
var<uniform> u: Uniforms;

@group(0) @binding(1)
var<storage, read> shadow_depth: array<f32>;

// The batch's texture — the CPU oracle's own linear-light texels, uploaded
// as RGBA16F. A batch without one binds a 1x1 dummy and marks its vertices
// `has_map = 0`, so it is never sampled meaningfully.
@group(0) @binding(2)
var t_map: texture_2d<f32>;
@group(0) @binding(3)
var s_map: sampler;

// The batch's half of the transparent split: 0 keeps `a >= 1.0` fragments
// (depth writes, no blending), 1 keeps `a < 1.0` (blending, no depth
// writes). See the module doc.
struct Push {
    alpha_pass: f32,
};

var<immediate> push: Push;

struct VertexIn {
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) albedo: vec3<f32>,
    @location(3) emissive: vec3<f32>,
    // 0 Basic, 1 Lambert, 2 Phong.
    @location(4) shading: f32,
    // 1 if the material receives shadows.
    @location(5) receive: f32,
    // Phong's exponent and white-specular scale, in the vertex record's two
    // spare floats (the pads of the albedo and emissive vec4s).
    @location(6) shininess: f32,
    @location(7) specular: f32,
    // The material's texture coordinates — (0,0) when the mesh has none.
    @location(8) uv: vec2<f32>,
    // The transparent split's inputs: the material's opacity, and whether a
    // texture is bound.
    @location(9) opacity: f32,
    @location(10) has_map: f32,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) @interpolate(flat) albedo: vec3<f32>,
    @location(3) @interpolate(flat) emissive: vec3<f32>,
    // x = shading, y = receive, z = shininess, w = specular.
    @location(4) @interpolate(flat) flags: vec4<f32>,
    // Perspective-correct texture coordinates.
    @location(5) uv: vec2<f32>,
    // x = the material's opacity, y = 1 if a texture is bound.
    @location(6) @interpolate(flat) alpha: vec2<f32>,
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
    out.flags = vec4<f32>(v.shading, v.receive, v.shininess, v.specular);
    out.uv = v.uv;
    out.alpha = vec2<f32>(v.opacity, v.has_map);
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

// The CPU's distance falloff for point and spot lights, `saturate(1.0 -
// (dist / range).powi(4)).powi(2)` with the powis unrolled into the
// multiplies a small-exponent powi lowers to: (1 - t^4)^2. A `range` of 0
// means no falloff, exactly as the CPU's `if range > 0.0` branch reads.
fn distance_falloff(dist: f32, range: f32) -> f32 {
    if (range <= 0.0) {
        return 1.0;
    }
    let t = dist / range;
    let t2 = t * t;
    let t4 = t2 * t2;
    let s = 1.0 - t4;
    return s * s;
}

// One directional/point/spot light's Phong contribution — the CPU's
// `radiance.mul(albedo.scale(ndl).add(Rgb::WHITE.scale(spec)))`: the diffuse
// term Lambert computes plus the specular highlight in white, both under the
// light's radiance. `radiance_scale` is everything but `color * intensity`:
// exposure, shadow visibility for a directional, `fall / (1 + dist^2 * 0.02)`
// for a point, `fall * cone^2` for a spot — so the caller applies the CPU's
// per-kind formula and this stays the one place the shared Phong arithmetic
// lives.
fn phong_contribution(
    color: vec3<f32>,
    intensity_scale: f32,
    dir: vec3<f32>,
    radiance_scale: f32,
    n: vec3<f32>,
    v: vec3<f32>,
    albedo: vec3<f32>,
    shininess: f32,
    specular: f32,
) -> vec3<f32> {
    let ndl = max(dot(n, dir), 0.0);
    if (ndl <= 0.0) {
        return vec3<f32>(0.0);
    }
    let hv = normalize(dir + v);
    let s = pow(max(dot(n, hv), 0.0), max(shininess, 1.0)) * specular;
    let radiance = (color * intensity_scale) * radiance_scale;
    return radiance * (albedo * ndl + vec3<f32>(s));
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
        if (kind < 2.5) {
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
            continue;
        }
        // Point and spot lights reach Lambert materials too — the CPU's
        // `shade` gives every lit shading the same radiance per light kind
        // and only the per-shading contribution differs (`albedo * ndl`
        // here). Without these branches a point light (its `dir_intensity`
        // xyz is zero) lit nothing and a spot was drawn as an unbounded
        // directional. The arithmetic is the CPU's, in its order:
        // `color.scale(intensity * fall / (1 + dist^2 * 0.02))` for a point,
        // `color.scale(intensity * fall * cone * cone)` for a spot, then
        // `.scale(exposure)`.
        let d = l.pos_range.xyz - p;
        let dist = length(d);
        let ld = normalize(d);
        let fall = distance_falloff(dist, l.pos_range.w);
        var scale = intensity * fall / (1.0 + dist * dist * 0.02);
        if (kind > 3.5) {
            let cos = dot(-ld, l.dir_intensity.xyz);
            let outer = l.cone.x;
            let inner = l.cone.y;
            let cone = clamp((cos - outer) / max(inner - outer, 1e-4), 0.0, 1.0);
            scale = intensity * fall * cone * cone;
        }
        let ndl = max(dot(n, ld), 0.0);
        if (ndl <= 0.0) {
            continue;
        }
        let radiance = (color * scale) * exposure;
        total = total + radiance * (albedo * ndl);
    }
    return total;
}

fn phong(
    albedo: vec3<f32>,
    p: vec3<f32>,
    n: vec3<f32>,
    receive: bool,
    shininess: f32,
    specular: f32,
) -> vec3<f32> {
    let exposure = u.eye_exposure.w;
    let eye = u.eye_exposure.xyz;
    // The CPU's `v = (ctx.eye - p).normalize()`, per shaded sample.
    let v = normalize(eye - p);
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
        if (kind < 2.5) {
            // Directional: the shadow visibility is Lambert's; the CPU's
            // `dir` for it is `-l.direction`, toward the surface.
            let dir = l.dir_intensity.xyz;
            var vis = 1.0;
            if (l.ground_shadow.w > 0.5 && u.params.z > 0.5 && receive) {
                let ndl0 = max(dot(n, -dir), 0.0);
                vis = visibility(p, 0.002 + 0.01 * (1.0 - ndl0));
            }
            if (vis <= 0.0) {
                continue;
            }
            total = total + phong_contribution(
                color, intensity, -dir, exposure * vis,
                n, v, albedo, shininess, specular,
            );
            continue;
        }
        if (kind < 3.5) {
            // Point: the CPU's `d = l.position - p`, the distance falloff,
            // and `fall / (1 + dist * dist * 0.02)` under exposure.
            let d = l.pos_range.xyz - p;
            let dist = length(d);
            let fall = distance_falloff(dist, l.pos_range.w);
            let atten = fall / (1.0 + dist * dist * 0.02);
            total = total + phong_contribution(
                color, intensity, normalize(d), exposure * atten,
                n, v, albedo, shininess, specular,
            );
            continue;
        }
        // Spot: the point falloff times the CPU's cone — `saturate((cos -
        // outer) / max(inner - outer, 1e-4))`, squared — where `cos` is the
        // alignment of the direction *to this point* with the light's
        // travel direction. The outer/inner cosines are precomputed on the
        // CPU at pack time, so both renderers evaluate the same numbers.
        let d = l.pos_range.xyz - p;
        let dist = length(d);
        let ld = normalize(d);
        let cos = dot(-ld, l.dir_intensity.xyz);
        let outer = l.cone.x;
        let inner = l.cone.y;
        var cone = clamp((cos - outer) / max(inner - outer, 1e-4), 0.0, 1.0);
        cone = cone * cone;
        let fall = distance_falloff(dist, l.pos_range.w);
        total = total + phong_contribution(
            color, intensity * fall * cone, ld, exposure,
            n, v, albedo, shininess, specular,
        );
    }
    return total;
}

@fragment
fn fs_main(f: VertexOut) -> @location(0) vec4<f32> {
    // The CPU's `shade`: a texture multiplies albedo and sets the starting
    // alpha; `a = texel.a * opacity` is the write-or-blend decision.
    var albedo = f.albedo;
    var base_alpha = 1.0;
    if (f.alpha.y > 0.5) {
        let texel = textureSampleLevel(t_map, s_map, f.uv, 0.0);
        albedo = albedo * texel.rgb;
        base_alpha = texel.a;
    }
    let a = base_alpha * f.alpha.x;
    // This batch's half of the transparent split — see the module doc.
    if (push.alpha_pass < 0.5 && a < 1.0) {
        discard;
    }
    if (push.alpha_pass > 0.5 && a >= 1.0) {
        discard;
    }
    var out: vec3<f32>;
    let shading = f.flags.x;
    if (shading > 1.5) {
        let n = normalize(f.normal);
        out = phong(
            albedo, f.world, n, f.flags.y > 0.5, f.flags.z, f.flags.w,
        ) + f.emissive;
    } else if (shading > 0.5) {
        let n = normalize(f.normal);
        out = lambert(albedo, f.world, n, f.flags.y > 0.5) + f.emissive;
    } else {
        out = albedo + f.emissive;
    }
    // Linear and unclamped — the CPU's target holds the same raw values,
    // and the shared readback encode clamps the way `to_image` does.
    return vec4<f32>(out, a);
}
