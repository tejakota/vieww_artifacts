//! Deterministic CPU triangle rasterizer for ViewW's spatial scene IR.
//!
//! This is an independent software path, not a wrapper around a game engine or
//! graphics framework. The resulting RGBA image is an ordinary ViewW image and
//! can be handed to the existing 2D image/compositor path.

use vieww_foundation::{Color, Image};

use crate::{
    Camera3, Light3, Material3, MeshInstance3, Scene3, ShadingModel, TextureAddressMode,
    TextureFilter, Vec2, Vec3,
};

/// Conservative bound protecting callers from accidental multi-gigabyte targets.
pub const DEFAULT_MAX_PIXELS: usize = 16 * 1024 * 1024;

/// Rasterization controls. This currently caps allocation; quality is defined
/// by the deterministic pixel-center triangle rule and no hidden runtime state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RasterConfig {
    pub max_pixels: usize,
}
impl Default for RasterConfig {
    fn default() -> Self {
        Self {
            max_pixels: DEFAULT_MAX_PIXELS,
        }
    }
}

/// Errors that can be diagnosed before or during a spatial render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RasterError {
    EmptyTarget,
    TargetTooLarge {
        pixels: usize,
        limit: usize,
    },
    InvalidTargetBuffer,
    InvalidCamera(crate::CameraError),
    InvalidSceneGeometry {
        object_index: usize,
        message: String,
    },
    InvalidSceneTransform {
        object_index: usize,
    },
    InvalidMaterial {
        object_index: usize,
    },
    InvalidLight {
        light_index: usize,
    },
}
impl std::fmt::Display for RasterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyTarget => f.write_str("spatial render target dimensions must be non-zero"),
            Self::TargetTooLarge { pixels, limit } => write!(
                f,
                "spatial render target has {pixels} pixels; configured limit is {limit}"
            ),
            Self::InvalidTargetBuffer => {
                f.write_str("spatial render target buffer has an invalid length")
            }
            Self::InvalidCamera(error) => write!(f, "spatial camera: {error}"),
            Self::InvalidSceneGeometry {
                object_index,
                message,
            } => write!(f, "scene object {object_index}: {message}"),
            Self::InvalidSceneTransform { object_index } => write!(
                f,
                "scene object {object_index} has a non-finite or degenerate transform"
            ),
            Self::InvalidMaterial { object_index } => write!(
                f,
                "scene object {object_index} has invalid metallic/roughness values"
            ),
            Self::InvalidLight { light_index } => {
                write!(f, "scene light {light_index} has invalid parameters")
            }
        }
    }
}
impl std::error::Error for RasterError {}

#[derive(Debug, Clone, Copy)]
struct ClipVertex {
    view: Vec3,
    world: Vec3,
    normal: Vec3,
    uv: Vec2,
}
impl ClipVertex {
    fn lerp(self, rhs: Self, t: f32) -> Self {
        Self {
            view: self.view.lerp(rhs.view, t),
            world: self.world.lerp(rhs.world, t),
            normal: self.normal.lerp(rhs.normal, t).normalized(),
            uv: self.uv.lerp(rhs.uv, t),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ScreenVertex {
    x: f32,
    y: f32,
    z: f32,
    inv_z: f32,
    world: Vec3,
    normal: Vec3,
    uv: Vec2,
}

/// Render a scene to a newly allocated RGBA8 image using ViewW's CPU path.
///
/// Callers can then use `vieww_foundation::Image` with the existing 2D widget,
/// paint command, asset or GPU compositor. No window or graphics context is
/// needed, which also makes the renderer suitable for headless tests and film
/// pipelines.
pub fn render(
    scene: &Scene3,
    camera: Camera3,
    width: u32,
    height: u32,
) -> Result<Image, RasterError> {
    render_with_config(scene, camera, width, height, RasterConfig::default())
}

/// Render with an explicit target-allocation ceiling.
pub fn render_with_config(
    scene: &Scene3,
    camera: Camera3,
    width: u32,
    height: u32,
    config: RasterConfig,
) -> Result<Image, RasterError> {
    if width == 0 || height == 0 {
        return Err(RasterError::EmptyTarget);
    }
    let pixels =
        (width as usize)
            .checked_mul(height as usize)
            .ok_or(RasterError::TargetTooLarge {
                pixels: usize::MAX,
                limit: config.max_pixels,
            })?;
    if pixels > config.max_pixels {
        return Err(RasterError::TargetTooLarge {
            pixels,
            limit: config.max_pixels,
        });
    }
    let byte_len = pixels.checked_mul(4).ok_or(RasterError::TargetTooLarge {
        pixels,
        limit: config.max_pixels,
    })?;
    let (opaque, transparent) = prepare_draw_order(scene, camera)?;
    let mut rgba = vec![0_u8; byte_len];
    let mut depth = vec![f32::INFINITY; pixels];
    let background = color_to_rgba8(scene.background);
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.copy_from_slice(&background);
    }

    let aspect = width as f32 / height as f32;
    for index in opaque.into_iter().chain(transparent) {
        draw_object(
            scene,
            &scene.objects[index],
            camera,
            aspect,
            width,
            height,
            &mut rgba,
            &mut depth,
        );
    }
    Ok(Image::from_rgba8(rgba, width, height))
}

/// Render into caller-owned buffers (RGBA8 plus one depth value per pixel).
/// This retains the large target allocations across frames; it may still use
/// temporary storage for object ordering and polygon clipping.
pub fn render_into(
    scene: &Scene3,
    camera: Camera3,
    width: u32,
    height: u32,
    rgba: &mut [u8],
    depth: &mut [f32],
) -> Result<(), RasterError> {
    render_into_with_config(
        scene,
        camera,
        width,
        height,
        rgba,
        depth,
        RasterConfig::default(),
    )
}

/// Reuse caller-owned buffers with an explicit pixel ceiling, using the same
/// bounds and scene validation as [`render_with_config`].
pub fn render_into_with_config(
    scene: &Scene3,
    camera: Camera3,
    width: u32,
    height: u32,
    rgba: &mut [u8],
    depth: &mut [f32],
    config: RasterConfig,
) -> Result<(), RasterError> {
    if width == 0 || height == 0 {
        return Err(RasterError::EmptyTarget);
    }
    let pixels =
        (width as usize)
            .checked_mul(height as usize)
            .ok_or(RasterError::TargetTooLarge {
                pixels: usize::MAX,
                limit: config.max_pixels,
            })?;
    if pixels > config.max_pixels {
        return Err(RasterError::TargetTooLarge {
            pixels,
            limit: config.max_pixels,
        });
    }
    let bytes = pixels.checked_mul(4).ok_or(RasterError::TargetTooLarge {
        pixels,
        limit: config.max_pixels,
    })?;
    if rgba.len() != bytes || depth.len() != pixels {
        return Err(RasterError::InvalidTargetBuffer);
    }
    let (opaque, transparent) = prepare_draw_order(scene, camera)?;
    let clear = color_to_rgba8(scene.background);
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.copy_from_slice(&clear);
    }
    depth.fill(f32::INFINITY);
    let aspect = width as f32 / height as f32;
    for index in opaque.into_iter().chain(transparent) {
        draw_object(
            scene,
            &scene.objects[index],
            camera,
            aspect,
            width,
            height,
            rgba,
            depth,
        );
    }
    Ok(())
}

/// Validate before drawing and render alpha-bearing objects back-to-front after
/// opaque geometry. This is a deterministic object-level sort; intersecting or
/// concave translucent meshes still need finer sorting or an order-independent
/// transparency technique.
fn prepare_draw_order(
    scene: &Scene3,
    camera: Camera3,
) -> Result<(Vec<usize>, Vec<usize>), RasterError> {
    camera.validate().map_err(RasterError::InvalidCamera)?;
    for (light_index, light) in scene.lights.iter().copied().enumerate() {
        if !light.is_valid() {
            return Err(RasterError::InvalidLight { light_index });
        }
    }
    let mut opaque = Vec::with_capacity(scene.objects.len());
    let mut transparent = Vec::new();
    for (index, object) in scene.objects.iter().enumerate() {
        object
            .mesh
            .validate()
            .map_err(|error| RasterError::InvalidSceneGeometry {
                object_index: index,
                message: error.to_string(),
            })?;
        if !object.transform.is_valid()
            || object.mesh.vertices.iter().any(|vertex| {
                let world = object.transform.transform_point(vertex.position);
                !world.is_finite() || !camera.camera_space(world).is_finite()
            })
        {
            return Err(RasterError::InvalidSceneTransform {
                object_index: index,
            });
        }
        if !object.material.is_valid() {
            return Err(RasterError::InvalidMaterial {
                object_index: index,
            });
        }
        if object.material.albedo.a < 255 || object.material.texture.is_some() {
            transparent.push(index);
        } else {
            opaque.push(index);
        }
    }
    // Sort by each mesh's transformed local-bounds center, not merely the
    // instance origin; imported models often have a non-zero local origin.
    let object_depth = |index: usize| {
        let object = &scene.objects[index];
        let mut minimum = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
        let mut maximum = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        for vertex in &object.mesh.vertices {
            minimum = minimum.min(vertex.position);
            maximum = maximum.max(vertex.position);
        }
        let local_center = minimum * 0.5 + maximum * 0.5;
        camera
            .camera_space(object.transform.transform_point(local_center))
            .z
    };
    transparent.sort_by(|a, b| {
        let depth_a = object_depth(*a);
        let depth_b = object_depth(*b);
        depth_b.total_cmp(&depth_a).then_with(|| a.cmp(b))
    });
    Ok((opaque, transparent))
}

#[allow(clippy::too_many_arguments)] // Explicit inputs keep scene, camera and target ownership visible.
fn draw_object(
    scene: &Scene3,
    object: &MeshInstance3,
    camera: Camera3,
    aspect: f32,
    width: u32,
    height: u32,
    rgba: &mut [u8],
    depth: &mut [f32],
) {
    let (near, far) = camera.projection.clip_planes();
    for triangle in object.mesh.indices.chunks_exact(3) {
        let source = [
            object.mesh.vertices[triangle[0] as usize],
            object.mesh.vertices[triangle[1] as usize],
            object.mesh.vertices[triangle[2] as usize],
        ];
        let world = source.map(|vertex| object.transform.transform_point(vertex.position));
        if !world.iter().all(|p| p.is_finite()) {
            continue;
        }
        let face_normal = (world[1] - world[0])
            .cross(world[2] - world[0])
            .normalized();
        if face_normal == Vec3::ZERO {
            continue;
        }
        let center = (world[0] + world[1] + world[2]) / 3.0;
        // ViewW uses a right-handed camera with positive camera-space Z forward.
        if !object.material.double_sided && face_normal.dot(camera.eye - center) <= 0.0 {
            continue;
        }
        let mut polygon = Vec::with_capacity(5);
        for (index, vertex) in source.into_iter().enumerate() {
            let normal = if vertex.normal == Vec3::ZERO {
                face_normal
            } else {
                object.transform.transform_normal(vertex.normal)
            };
            polygon.push(ClipVertex {
                view: camera.camera_space(world[index]),
                world: world[index],
                normal,
                uv: vertex.uv,
            });
        }
        polygon = clip_plane(&polygon, near, true);
        if polygon.len() < 3 {
            continue;
        }
        polygon = clip_plane(&polygon, far, false);
        if polygon.len() < 3 {
            continue;
        }
        for i in 1..polygon.len() - 1 {
            let Some(a) = project(polygon[0], camera, aspect, width, height) else {
                continue;
            };
            let Some(b) = project(polygon[i], camera, aspect, width, height) else {
                continue;
            };
            let Some(c) = project(polygon[i + 1], camera, aspect, width, height) else {
                continue;
            };
            raster_triangle(scene, object, camera, [a, b, c], width, height, rgba, depth);
        }
    }
}

fn clip_plane(poly: &[ClipVertex], plane: f32, keep_greater: bool) -> Vec<ClipVertex> {
    if poly.is_empty() {
        return Vec::new();
    }
    let inside = |z: f32| if keep_greater { z >= plane } else { z <= plane };
    let mut result = Vec::with_capacity(poly.len() + 1);
    let mut previous = *poly.last().expect("non-empty polygon checked");
    let mut previous_inside = inside(previous.view.z);
    for current in poly.iter().copied() {
        let current_inside = inside(current.view.z);
        if current_inside != previous_inside {
            let denominator = current.view.z - previous.view.z;
            if denominator.abs() > 1.0e-8 {
                let t = ((plane - previous.view.z) / denominator).clamp(0.0, 1.0);
                result.push(previous.lerp(current, t));
            }
        }
        if current_inside {
            result.push(current);
        }
        previous = current;
        previous_inside = current_inside;
    }
    result
}

fn project(
    vertex: ClipVertex,
    camera: Camera3,
    aspect: f32,
    width: u32,
    height: u32,
) -> Option<ScreenVertex> {
    let z = vertex.view.z;
    if !z.is_finite() || z <= 0.0 {
        return None;
    }
    let (nx, ny) = match camera.projection {
        crate::Projection3::Perspective {
            vertical_fov_radians,
            ..
        } => {
            let f = 1.0 / (vertical_fov_radians * 0.5).tan();
            (vertex.view.x * f / (z * aspect), vertex.view.y * f / z)
        }
        crate::Projection3::Orthographic { vertical_size, .. } => (
            vertex.view.x * 2.0 / (vertical_size * aspect),
            vertex.view.y * 2.0 / vertical_size,
        ),
    };
    let sx = (nx * 0.5 + 0.5) * width as f32;
    let sy = (0.5 - ny * 0.5) * height as f32;
    (sx.is_finite() && sy.is_finite()).then_some(ScreenVertex {
        x: sx,
        y: sy,
        z,
        inv_z: 1.0 / z,
        world: vertex.world,
        normal: vertex.normal,
        uv: vertex.uv,
    })
}

fn edge(a: &ScreenVertex, b: &ScreenVertex, x: f32, y: f32) -> f32 {
    (x - a.x) * (b.y - a.y) - (y - a.y) * (b.x - a.x)
}

#[allow(clippy::too_many_arguments)] // The raster kernel mutates two disjoint caller-owned target buffers.
fn raster_triangle(
    scene: &Scene3,
    object: &MeshInstance3,
    camera: Camera3,
    vertices: [ScreenVertex; 3],
    width: u32,
    height: u32,
    rgba: &mut [u8],
    depth: &mut [f32],
) {
    let [a, b, c] = vertices;
    let area = edge(&a, &b, c.x, c.y);
    if !area.is_finite() || area.abs() < 1.0e-7 {
        return;
    }
    let min_x = a.x.min(b.x).min(c.x).floor().max(0.0) as i32;
    let max_x = a.x.max(b.x).max(c.x).ceil().min(width as f32 - 1.0) as i32;
    let min_y = a.y.min(b.y).min(c.y).floor().max(0.0) as i32;
    let max_y = a.y.max(b.y).max(c.y).ceil().min(height as f32 - 1.0) as i32;
    if min_x > max_x || min_y > max_y {
        return;
    }
    let perspective = matches!(camera.projection, crate::Projection3::Perspective { .. });
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let w0 = edge(&b, &c, px, py) / area;
            let w1 = edge(&c, &a, px, py) / area;
            let w2 = edge(&a, &b, px, py) / area;
            if w0 < -1.0e-6 || w1 < -1.0e-6 || w2 < -1.0e-6 {
                continue;
            }
            let index = y as usize * width as usize + x as usize;
            let reciprocal_depth = if perspective {
                w0 * a.inv_z + w1 * b.inv_z + w2 * c.inv_z
            } else {
                0.0
            };
            let z = if perspective {
                if reciprocal_depth <= 0.0 {
                    continue;
                }
                1.0 / reciprocal_depth
            } else {
                w0 * a.z + w1 * b.z + w2 * c.z
            };
            if !z.is_finite() || z >= depth[index] {
                continue;
            }
            let (p0, p1, p2) = if perspective {
                (
                    w0 * a.inv_z / reciprocal_depth,
                    w1 * b.inv_z / reciprocal_depth,
                    w2 * c.inv_z / reciprocal_depth,
                )
            } else {
                (w0, w1, w2)
            };
            let world = a.world * p0 + b.world * p1 + c.world * p2;
            let mut normal = (a.normal * p0 + b.normal * p1 + c.normal * p2).normalized();
            if normal == Vec3::ZERO {
                normal = (b.world - a.world).cross(c.world - a.world).normalized();
            }
            let uv = a.uv * p0 + b.uv * p1 + c.uv * p2;
            let mut albedo = color_to_linear(object.material.albedo);
            if let Some(texture) = &object.material.texture {
                let texel = sample_texture(
                    texture,
                    uv,
                    object.material.texture_filter,
                    object.material.address_mode,
                );
                albedo.0 *= texel.0;
                albedo.1 *= texel.1;
                albedo.2 *= texel.2;
                albedo.3 *= texel.3;
            }
            let shaded = shade(scene, object, camera, world, normal, albedo);
            let destination = &mut rgba[index * 4..index * 4 + 4];
            blend_pixel(destination, shaded);
            // Opaque depth writes are exact. For translucent primitives this is
            // deliberately the simple painter path; see parity notes for limits.
            if shaded[3] >= 0.999 {
                depth[index] = z;
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct LinearColor(f32, f32, f32, f32);
fn channel_to_linear(channel: u8) -> f32 {
    let c = f32::from(channel) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
fn linear_to_channel(channel: f32) -> u8 {
    let c = if channel.is_finite() {
        channel.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let srgb = if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (srgb * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8
}
fn color_to_linear(color: Color) -> LinearColor {
    let r = channel_to_linear(color.r);
    let g = channel_to_linear(color.g);
    let b = channel_to_linear(color.b);
    let matrix = color.space.to_linear_srgb();
    LinearColor(
        matrix[0][0] * r + matrix[0][1] * g + matrix[0][2] * b,
        matrix[1][0] * r + matrix[1][1] * g + matrix[1][2] * b,
        matrix[2][0] * r + matrix[2][1] * g + matrix[2][2] * b,
        f32::from(color.a) / 255.0,
    )
}
fn color_to_rgba8(color: Color) -> [u8; 4] {
    let linear = color_to_linear(color);
    [
        linear_to_channel(linear.0),
        linear_to_channel(linear.1),
        linear_to_channel(linear.2),
        color.a,
    ]
}
fn sample_texture(
    image: &Image,
    uv: Vec2,
    filter: TextureFilter,
    address: TextureAddressMode,
) -> LinearColor {
    let width = image.width() as usize;
    let height = image.height() as usize;
    if width == 0 || height == 0 {
        return LinearColor(1.0, 1.0, 1.0, 1.0);
    }
    let bytes = image.pixels();
    let coord = |value: f32| match address {
        TextureAddressMode::Repeat => value.rem_euclid(1.0),
        TextureAddressMode::Clamp => value.clamp(0.0, 1.0),
    };
    let read = |x: isize, y: isize| -> LinearColor {
        let address_index = |index: isize, size: usize| -> usize {
            match address {
                TextureAddressMode::Repeat => index.rem_euclid(size as isize) as usize,
                TextureAddressMode::Clamp => index.clamp(0, size as isize - 1) as usize,
            }
        };
        let x = address_index(x, width);
        let y = address_index(y, height);
        let offset = (y * width + x) * 4;
        if offset + 3 >= bytes.len() {
            return LinearColor(1.0, 1.0, 1.0, 1.0);
        }
        LinearColor(
            channel_to_linear(bytes[offset]),
            channel_to_linear(bytes[offset + 1]),
            channel_to_linear(bytes[offset + 2]),
            f32::from(bytes[offset + 3]) / 255.0,
        )
    };
    let u = coord(uv.x);
    // Images store the first row at the top, whereas mesh V commonly grows up.
    let v = coord(1.0 - uv.y);
    match filter {
        TextureFilter::Nearest => {
            let x = (u * width as f32).floor() as isize;
            let y = (v * height as f32).floor() as isize;
            read(x, y)
        }
        TextureFilter::Bilinear => {
            let (fx, fy) = match address {
                TextureAddressMode::Repeat => (u * width as f32 - 0.5, v * height as f32 - 0.5),
                TextureAddressMode::Clamp => (
                    u * width.saturating_sub(1) as f32,
                    v * height.saturating_sub(1) as f32,
                ),
            };
            let x0 = fx.floor() as isize;
            let y0 = fy.floor() as isize;
            let tx = fx - fx.floor();
            let ty = fy - fy.floor();
            let top_left = read(x0, y0);
            let top_right = read(x0 + 1, y0);
            let bottom_left = read(x0, y0 + 1);
            let bottom_right = read(x0 + 1, y0 + 1);
            let mix = |a: f32, b: f32, t: f32| a + (b - a) * t;
            let channel = |a: f32, b: f32, c: f32, d: f32| mix(mix(a, b, tx), mix(c, d, tx), ty);
            LinearColor(
                channel(top_left.0, top_right.0, bottom_left.0, bottom_right.0),
                channel(top_left.1, top_right.1, bottom_left.1, bottom_right.1),
                channel(top_left.2, top_right.2, bottom_left.2, bottom_right.2),
                channel(top_left.3, top_right.3, bottom_left.3, bottom_right.3),
            )
        }
    }
}

fn color_rgb(color: Color) -> Vec3 {
    let linear = color_to_linear(color);
    Vec3::new(linear.0, linear.1, linear.2)
}
fn shade(
    scene: &Scene3,
    object: &MeshInstance3,
    camera: Camera3,
    position: Vec3,
    normal: Vec3,
    albedo: LinearColor,
) -> [f32; 4] {
    let mut rgb = Vec3::new(albedo.0, albedo.1, albedo.2);
    let alpha = albedo.3;
    if object.material.model != ShadingModel::Unlit {
        let view = (camera.eye - position).normalized();
        let mut illumination = Vec3::ZERO;
        let mut specular = Vec3::ZERO;
        for light in &scene.lights {
            match *light {
                Light3::Ambient { color, intensity } => {
                    illumination = illumination + color_rgb(color) * intensity.max(0.0);
                }
                Light3::Directional {
                    direction,
                    color,
                    intensity,
                } => {
                    let to_light = (-direction).normalized();
                    accumulate_light(
                        &mut illumination,
                        &mut specular,
                        &object.material,
                        albedo,
                        normal,
                        view,
                        to_light,
                        color_rgb(color),
                        intensity.max(0.0),
                        1.0,
                    );
                }
                Light3::Point {
                    position: light_position,
                    color,
                    intensity,
                    range,
                } => {
                    let delta = light_position - position;
                    let distance = delta.length();
                    let attenuation = range_attenuation(distance, range);
                    accumulate_light(
                        &mut illumination,
                        &mut specular,
                        &object.material,
                        albedo,
                        normal,
                        view,
                        delta.normalized(),
                        color_rgb(color),
                        intensity.max(0.0),
                        attenuation,
                    );
                }
                Light3::Spot {
                    position: light_position,
                    direction,
                    inner_angle,
                    outer_angle,
                    color,
                    intensity,
                    range,
                } => {
                    let delta = light_position - position;
                    let distance = delta.length();
                    let toward_surface = (-delta).normalized();
                    let cosine = direction.normalized().dot(toward_surface);
                    let inner = inner_angle.cos();
                    let outer = outer_angle.cos();
                    let cone = if (inner - outer).abs() < 1.0e-6 {
                        if cosine >= inner {
                            1.0
                        } else {
                            0.0
                        }
                    } else {
                        ((cosine - outer) / (inner - outer)).clamp(0.0, 1.0)
                    };
                    let attenuation = range_attenuation(distance, range) * cone;
                    accumulate_light(
                        &mut illumination,
                        &mut specular,
                        &object.material,
                        albedo,
                        normal,
                        view,
                        delta.normalized(),
                        color_rgb(color),
                        intensity.max(0.0),
                        attenuation,
                    );
                }
            }
        }
        rgb = match object.material.model {
            ShadingModel::Unlit => rgb,
            ShadingModel::Lambert => rgb.component_mul(illumination),
            ShadingModel::BlinnPhong => rgb.component_mul(illumination) + specular,
            ShadingModel::Pbr => {
                let metallic = object.material.metallic.clamp(0.0, 1.0);
                let diffuse = rgb.component_mul(illumination) * (1.0 - metallic);
                diffuse + specular
            }
        };
    }
    [rgb.x, rgb.y, rgb.z, alpha]
}

fn range_attenuation(distance: f32, range: f32) -> f32 {
    if !range.is_finite() || range <= 0.0 {
        return 0.0;
    }
    let normalized = (distance / range).clamp(0.0, 1.0);
    let edge = (1.0 - normalized * normalized).max(0.0);
    edge * edge / (1.0 + distance * distance * 0.05)
}

#[allow(clippy::too_many_arguments)]
fn accumulate_light(
    diffuse_accumulator: &mut Vec3,
    specular_accumulator: &mut Vec3,
    material: &Material3,
    albedo: LinearColor,
    normal: Vec3,
    view: Vec3,
    to_light: Vec3,
    light_color: Vec3,
    intensity: f32,
    attenuation: f32,
) {
    if attenuation <= 0.0 || intensity <= 0.0 {
        return;
    }
    let ndotl = normal.dot(to_light).max(0.0);
    if ndotl <= 0.0 {
        return;
    }
    let radiance = light_color * (intensity * attenuation);
    *diffuse_accumulator = *diffuse_accumulator + radiance * ndotl;
    if matches!(material.model, ShadingModel::BlinnPhong | ShadingModel::Pbr) {
        let half_vector = (to_light + view).normalized();
        let exponent = if material.model == ShadingModel::BlinnPhong {
            32.0
        } else {
            (2.0 / material.roughness.clamp(0.04, 1.0).powi(2) - 2.0).max(1.0)
        };
        let highlight = normal.dot(half_vector).max(0.0).powf(exponent) * ndotl;
        let metallic = material.metallic.clamp(0.0, 1.0);
        let base = Vec3::new(albedo.0, albedo.1, albedo.2);
        let f0 = Vec3::new(0.04, 0.04, 0.04).lerp(base, metallic);
        *specular_accumulator = *specular_accumulator + f0.component_mul(radiance) * highlight;
    }
}

fn blend_pixel(destination: &mut [u8], source: [f32; 4]) {
    let src_alpha = source[3].clamp(0.0, 1.0);
    if src_alpha <= 0.0 {
        return;
    }
    if src_alpha >= 0.999 {
        destination[0] = linear_to_channel(source[0]);
        destination[1] = linear_to_channel(source[1]);
        destination[2] = linear_to_channel(source[2]);
        destination[3] = 255;
        return;
    }
    let dst_alpha = f32::from(destination[3]) / 255.0;
    let out_alpha = src_alpha + dst_alpha * (1.0 - src_alpha);
    if out_alpha <= 1.0e-6 {
        destination.fill(0);
        return;
    }
    let dst = [
        channel_to_linear(destination[0]),
        channel_to_linear(destination[1]),
        channel_to_linear(destination[2]),
    ];
    for (channel, output) in destination.iter_mut().take(3).enumerate() {
        let out = (source[channel] * src_alpha + dst[channel] * dst_alpha * (1.0 - src_alpha))
            / out_alpha;
        *output = linear_to_channel(out);
    }
    destination[3] = (out_alpha * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Material3, Mesh3, MeshInstance3, Projection3, Vec3};

    #[test]
    fn cube_produces_a_non_background_pixel() {
        let camera = Camera3::look_at(
            Vec3::new(0.0, 0.0, -3.0),
            Vec3::ZERO,
            Vec3::Y,
            Projection3::perspective(1.0, 0.1, 100.0).unwrap(),
        )
        .unwrap();
        let mut scene = Scene3::new();
        scene.background = Color::rgba(0, 0, 0, 255);
        scene.add(
            MeshInstance3::new(
                Mesh3::cube(),
                Material3::new(Color::rgba(255, 0, 0, 255)).unlit(),
            )
            .with_transform(crate::Transform3::IDENTITY),
        );
        let image = render(&scene, camera, 64, 64).unwrap();
        assert!(image
            .pixels()
            .chunks_exact(4)
            .any(|p| p[0] > 200 && p[1] < 20));
    }

    #[test]
    fn target_limits_and_invalid_dimensions_are_reported() {
        let camera = Camera3::look_at(
            Vec3::new(0.0, 0.0, -3.0),
            Vec3::ZERO,
            Vec3::Y,
            Projection3::perspective(1.0, 0.1, 100.0).unwrap(),
        )
        .unwrap();
        let scene = Scene3::new();
        assert_eq!(
            render(&scene, camera, 0, 4).unwrap_err(),
            RasterError::EmptyTarget
        );
        assert!(matches!(
            render_with_config(&scene, camera, 32, 32, RasterConfig { max_pixels: 10 }),
            Err(RasterError::TargetTooLarge { .. })
        ));
        let mut rgba = vec![0; 32 * 32 * 4];
        let mut depth = vec![0.0; 32 * 32];
        assert!(matches!(
            render_into_with_config(
                &scene,
                camera,
                32,
                32,
                &mut rgba,
                &mut depth,
                RasterConfig { max_pixels: 10 }
            ),
            Err(RasterError::TargetTooLarge { .. })
        ));
    }

    #[test]
    fn invalid_public_scene_fields_return_errors_instead_of_silent_skips() {
        let camera = Camera3::look_at(
            Vec3::new(0.0, 0.0, -3.0),
            Vec3::ZERO,
            Vec3::Y,
            Projection3::perspective(1.0, 0.1, 100.0).unwrap(),
        )
        .unwrap();
        let mut scene = Scene3::new();
        scene.objects.push(
            MeshInstance3::new(Mesh3::cube(), Material3::default()).with_transform(
                crate::Transform3::new(
                    Vec3::new(f32::NAN, 0.0, 0.0),
                    crate::Quat::IDENTITY,
                    Vec3::new(1.0, 1.0, 1.0),
                ),
            ),
        );
        assert!(matches!(
            render(&scene, camera, 16, 16),
            Err(RasterError::InvalidSceneTransform { object_index: 0 })
        ));

        let mut scene = Scene3::new();
        scene.lights.push(Light3::Ambient {
            color: Color::WHITE,
            intensity: f32::NAN,
        });
        assert!(matches!(
            render(&scene, camera, 16, 16),
            Err(RasterError::InvalidLight { .. })
        ));
    }

    #[test]
    fn degenerate_public_transform_is_rejected_before_rasterization() {
        let camera = Camera3::look_at(
            Vec3::new(0.0, 0.0, -3.0),
            Vec3::ZERO,
            Vec3::Y,
            Projection3::perspective(1.0, 0.1, 100.0).unwrap(),
        )
        .unwrap();
        let mut scene = Scene3::new();
        let invalid = crate::Transform3::new(
            Vec3::ZERO,
            crate::Quat::new(0.0, 0.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 1.0),
        );
        scene.add(MeshInstance3::new(Mesh3::cube(), Material3::default()).with_transform(invalid));
        assert!(matches!(
            render(&scene, camera, 16, 16),
            Err(RasterError::InvalidSceneTransform { object_index: 0 })
        ));
    }

    #[test]
    fn texture_sampling_honors_nearest_and_bilinear_modes() {
        let image = Image::from_rgba8(
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
            2,
            2,
        );
        let nearest = sample_texture(
            &image,
            Vec2::new(0.25, 0.75),
            TextureFilter::Nearest,
            TextureAddressMode::Clamp,
        );
        assert!(nearest.0 > 0.99 && nearest.1 < 0.01 && nearest.2 < 0.01);
        let bilinear = sample_texture(
            &image,
            Vec2::new(0.5, 0.5),
            TextureFilter::Bilinear,
            TextureAddressMode::Clamp,
        );
        for channel in [bilinear.0, bilinear.1, bilinear.2] {
            let encoded = linear_to_channel(channel);
            assert!((180..=200).contains(&encoded));
        }
    }
}
