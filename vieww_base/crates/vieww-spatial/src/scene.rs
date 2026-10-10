//! Spatial scene data, cameras, materials, lights and a small Wavefront OBJ
//! loader. These types describe a scene; they do not choose a graphics API.

use vieww_foundation::{Color, Image};

use crate::math::{Quat, Transform3, Vec2, Vec3};

/// A vertex before model/view/projection transforms.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vertex3 {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
}
impl Vertex3 {
    #[must_use]
    pub const fn new(position: Vec3, normal: Vec3, uv: Vec2) -> Self {
        Self {
            position,
            normal,
            uv,
        }
    }
}

/// A triangle mesh. Indices are a flat triangle list; every three indices are
/// one triangle, and each index addresses `vertices`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh3 {
    pub vertices: Vec<Vertex3>,
    pub indices: Vec<u32>,
}

/// Why a mesh could not be accepted by the renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeshError {
    Empty,
    IndexCountNotTriangles(usize),
    IndexOutOfBounds { index: u32, vertices: usize },
    NonFiniteVertex(usize),
    Obj { line: usize, message: String },
}
impl std::fmt::Display for MeshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "a mesh needs at least one vertex and one triangle"),
            Self::IndexCountNotTriangles(count) => {
                write!(f, "mesh index count {count} is not divisible by three")
            }
            Self::IndexOutOfBounds { index, vertices } => {
                write!(f, "mesh index {index} is outside {vertices} vertices")
            }
            Self::NonFiniteVertex(index) => {
                write!(f, "mesh vertex {index} contains a non-finite coordinate")
            }
            Self::Obj { line, message } => write!(f, "OBJ line {line}: {message}"),
        }
    }
}
impl std::error::Error for MeshError {}

impl Mesh3 {
    pub fn new(vertices: Vec<Vertex3>, indices: Vec<u32>) -> Result<Self, MeshError> {
        let mesh = Self { vertices, indices };
        mesh.validate()?;
        Ok(mesh)
    }

    pub fn validate(&self) -> Result<(), MeshError> {
        if self.vertices.is_empty() || self.indices.is_empty() {
            return Err(MeshError::Empty);
        }
        if !self.indices.len().is_multiple_of(3) {
            return Err(MeshError::IndexCountNotTriangles(self.indices.len()));
        }
        for (index, vertex) in self.vertices.iter().enumerate() {
            if !vertex.position.is_finite()
                || !vertex.normal.is_finite()
                || !vertex.uv.x.is_finite()
                || !vertex.uv.y.is_finite()
            {
                return Err(MeshError::NonFiniteVertex(index));
            }
        }
        for index in &self.indices {
            if *index as usize >= self.vertices.len() {
                return Err(MeshError::IndexOutOfBounds {
                    index: *index,
                    vertices: self.vertices.len(),
                });
            }
        }
        Ok(())
    }

    /// Parse the useful geometry subset of Wavefront OBJ without bringing in
    /// an importer framework. Supports positions, UVs, normals, negative
    /// indices and polygon faces (fan-triangulated); material-library files are
    /// intentionally left to the caller so the material contract stays typed.
    pub fn from_obj(source: &str) -> Result<Self, MeshError> {
        use std::collections::HashMap;
        let mut positions = Vec::<Vec3>::new();
        let mut normals = Vec::<Vec3>::new();
        let mut uvs = Vec::<Vec2>::new();
        let mut vertices = Vec::<Vertex3>::new();
        let mut indices = Vec::<u32>::new();
        let mut remap = HashMap::<(usize, Option<usize>, Option<usize>), u32>::new();

        fn parse_f32(token: Option<&str>, line: usize, name: &str) -> Result<f32, MeshError> {
            let token = token.ok_or_else(|| MeshError::Obj {
                line,
                message: format!("missing {name}"),
            })?;
            let value = token.parse::<f32>().map_err(|_| MeshError::Obj {
                line,
                message: format!("invalid {name} {token:?}"),
            })?;
            if !value.is_finite() {
                return Err(MeshError::Obj {
                    line,
                    message: format!("non-finite {name}"),
                });
            }
            Ok(value)
        }
        fn resolve_index(
            token: &str,
            len: usize,
            line: usize,
            kind: &str,
        ) -> Result<usize, MeshError> {
            let raw = token.parse::<i64>().map_err(|_| MeshError::Obj {
                line,
                message: format!("invalid {kind} index {token:?}"),
            })?;
            if raw == 0 {
                return Err(MeshError::Obj {
                    line,
                    message: format!("OBJ {kind} indices are one-based; zero is invalid"),
                });
            }
            // Widen before adding a negative relative index; `i64::MIN + len`
            // must not overflow in debug builds for hostile asset input.
            let resolved = if raw > 0 {
                i128::from(raw) - 1
            } else {
                len as i128 + i128::from(raw)
            };
            if resolved < 0 || resolved >= len as i128 {
                return Err(MeshError::Obj {
                    line,
                    message: format!("{kind} index {raw} is out of range for {len} entries"),
                });
            }
            Ok(resolved as usize)
        }
        for (line_index, raw_line) in source.lines().enumerate() {
            let line_number = line_index + 1;
            let line = raw_line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split_whitespace();
            match parts.next().unwrap_or("") {
                "v" => positions.push(Vec3::new(
                    parse_f32(parts.next(), line_number, "x")?,
                    parse_f32(parts.next(), line_number, "y")?,
                    parse_f32(parts.next(), line_number, "z")?,
                )),
                "vn" => normals.push(
                    Vec3::new(
                        parse_f32(parts.next(), line_number, "normal x")?,
                        parse_f32(parts.next(), line_number, "normal y")?,
                        parse_f32(parts.next(), line_number, "normal z")?,
                    )
                    .normalized(),
                ),
                "vt" => uvs.push(Vec2::new(
                    parse_f32(parts.next(), line_number, "UV u")?,
                    parse_f32(parts.next(), line_number, "UV v")?,
                )),
                "f" => {
                    let mut face = Vec::<u32>::new();
                    for part in parts {
                        let mut fields = part.split('/');
                        let p = resolve_index(
                            fields.next().unwrap_or(""),
                            positions.len(),
                            line_number,
                            "position",
                        )?;
                        let uv_token = fields.next();
                        let n_token = fields.next();
                        if fields.next().is_some() {
                            return Err(MeshError::Obj { line: line_number, message: format!("face vertex {part:?} has more than three slash-separated fields") });
                        }
                        let uv = match uv_token {
                            Some(token) if !token.is_empty() => {
                                Some(resolve_index(token, uvs.len(), line_number, "UV")?)
                            }
                            _ => None,
                        };
                        let n = match n_token {
                            Some(token) if !token.is_empty() => {
                                Some(resolve_index(token, normals.len(), line_number, "normal")?)
                            }
                            _ => None,
                        };
                        let key = (p, uv, n);
                        let index = if let Some(existing) = remap.get(&key) {
                            *existing
                        } else {
                            let index =
                                u32::try_from(vertices.len()).map_err(|_| MeshError::Obj {
                                    line: line_number,
                                    message: "too many unique vertices".into(),
                                })?;
                            vertices.push(Vertex3::new(
                                positions[p],
                                n.map(|i| normals[i]).unwrap_or(Vec3::ZERO),
                                uv.map(|i| uvs[i]).unwrap_or(Vec2::new(0.0, 0.0)),
                            ));
                            remap.insert(key, index);
                            index
                        };
                        face.push(index);
                    }
                    if face.len() < 3 {
                        return Err(MeshError::Obj {
                            line: line_number,
                            message: "a face needs at least three vertices".into(),
                        });
                    }
                    for triangle in face[1..].windows(2) {
                        indices.extend_from_slice(&[face[0], triangle[0], triangle[1]]);
                    }
                }
                // `o`, `g`, `s`, `usemtl`, and `mtllib` describe asset metadata
                // rather than geometry. Unknown directives are ignored to keep
                // the geometry reader forward-compatible with common OBJ files.
                _ => {}
            }
        }
        Self::new(vertices, indices)
    }

    /// Build a flat-shaded unit cube. Each face owns its vertices so its normal
    /// does not get averaged across a hard edge.
    #[must_use]
    pub fn cube() -> Self {
        let faces = [
            (
                Vec3::new(0.0, 0.0, 1.0),
                [
                    Vec3::new(-0.5, -0.5, 0.5),
                    Vec3::new(0.5, -0.5, 0.5),
                    Vec3::new(0.5, 0.5, 0.5),
                    Vec3::new(-0.5, 0.5, 0.5),
                ],
            ),
            (
                Vec3::new(0.0, 0.0, -1.0),
                [
                    Vec3::new(0.5, -0.5, -0.5),
                    Vec3::new(-0.5, -0.5, -0.5),
                    Vec3::new(-0.5, 0.5, -0.5),
                    Vec3::new(0.5, 0.5, -0.5),
                ],
            ),
            (
                Vec3::new(1.0, 0.0, 0.0),
                [
                    Vec3::new(0.5, -0.5, 0.5),
                    Vec3::new(0.5, -0.5, -0.5),
                    Vec3::new(0.5, 0.5, -0.5),
                    Vec3::new(0.5, 0.5, 0.5),
                ],
            ),
            (
                Vec3::new(-1.0, 0.0, 0.0),
                [
                    Vec3::new(-0.5, -0.5, -0.5),
                    Vec3::new(-0.5, -0.5, 0.5),
                    Vec3::new(-0.5, 0.5, 0.5),
                    Vec3::new(-0.5, 0.5, -0.5),
                ],
            ),
            (
                Vec3::new(0.0, 1.0, 0.0),
                [
                    Vec3::new(-0.5, 0.5, 0.5),
                    Vec3::new(0.5, 0.5, 0.5),
                    Vec3::new(0.5, 0.5, -0.5),
                    Vec3::new(-0.5, 0.5, -0.5),
                ],
            ),
            (
                Vec3::new(0.0, -1.0, 0.0),
                [
                    Vec3::new(-0.5, -0.5, -0.5),
                    Vec3::new(0.5, -0.5, -0.5),
                    Vec3::new(0.5, -0.5, 0.5),
                    Vec3::new(-0.5, -0.5, 0.5),
                ],
            ),
        ];
        let mut vertices = Vec::with_capacity(24);
        let mut indices = Vec::with_capacity(36);
        for (normal, corners) in faces {
            let base = vertices.len() as u32;
            vertices.extend(
                corners
                    .into_iter()
                    .zip([
                        Vec2::new(0.0, 1.0),
                        Vec2::new(1.0, 1.0),
                        Vec2::new(1.0, 0.0),
                        Vec2::new(0.0, 0.0),
                    ])
                    .map(|(position, uv)| Vertex3::new(position, normal, uv)),
            );
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        Self { vertices, indices }
    }
}

/// Shading model for a material. `Pbr` is a compact metallic/roughness model,
/// not a promise of an offline path-tracing renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShadingModel {
    /// Return albedo without lighting; useful for UI-like 3D labels and gizmos.
    Unlit,
    /// Lambert diffuse response, without a specular lobe.
    Lambert,
    /// Blinn–Phong specular highlight, useful for simple real-time assets.
    BlinnPhong,
    /// Energy-aware metallic/roughness real-time approximation.
    #[default]
    Pbr,
}

/// Filtering used by a textured 3D material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextureFilter {
    /// Select one texel. Useful for pixel-art textures.
    Nearest,
    /// Interpolate neighboring texels in linear-light space.
    #[default]
    Bilinear,
}

/// UV coordinates outside `0..=1` either wrap or clamp to the texture edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextureAddressMode {
    #[default]
    Repeat,
    Clamp,
}

/// Surface description. The image texture is sampled in UV space by the CPU
/// renderer; no image framework or 3D engine is required.
#[derive(Debug, Clone)]
pub struct Material3 {
    pub albedo: Color,
    pub texture: Option<Image>,
    pub texture_filter: TextureFilter,
    pub address_mode: TextureAddressMode,
    pub metallic: f32,
    pub roughness: f32,
    pub model: ShadingModel,
    pub double_sided: bool,
}
impl Default for Material3 {
    fn default() -> Self {
        Self::new(Color::rgba(220, 225, 235, 255))
    }
}
impl Material3 {
    #[must_use]
    pub fn new(albedo: Color) -> Self {
        Self {
            albedo,
            texture: None,
            texture_filter: TextureFilter::Bilinear,
            address_mode: TextureAddressMode::Repeat,
            metallic: 0.0,
            roughness: 0.65,
            model: ShadingModel::Pbr,
            double_sided: false,
        }
    }
    #[must_use]
    pub fn unlit(mut self) -> Self {
        self.model = ShadingModel::Unlit;
        self
    }
    #[must_use]
    pub fn with_texture(mut self, texture: Image) -> Self {
        self.texture = Some(texture);
        self
    }
    #[must_use]
    pub fn sampling(mut self, filter: TextureFilter, address_mode: TextureAddressMode) -> Self {
        self.texture_filter = filter;
        self.address_mode = address_mode;
        self
    }
    #[must_use]
    pub fn metallic_roughness(mut self, metallic: f32, roughness: f32) -> Self {
        self.metallic = if metallic.is_finite() {
            metallic.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.roughness = if roughness.is_finite() {
            roughness.clamp(0.04, 1.0)
        } else {
            0.65
        };
        self
    }

    /// Validate public material fields before they enter the rasterizer.
    /// `Material3` intentionally exposes fields for straightforward scene authoring,
    /// so renderer entry points must not assume callers used the builder methods.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.metallic.is_finite()
            && (0.0..=1.0).contains(&self.metallic)
            && self.roughness.is_finite()
            && (0.04..=1.0).contains(&self.roughness)
    }

    #[must_use]
    pub fn double_sided(mut self, enabled: bool) -> Self {
        self.double_sided = enabled;
        self
    }
}

/// One piece of geometry placed in the scene.
#[derive(Debug, Clone)]
pub struct MeshInstance3 {
    pub mesh: Mesh3,
    pub transform: Transform3,
    pub material: Material3,
}
impl MeshInstance3 {
    #[must_use]
    pub fn new(mesh: Mesh3, material: Material3) -> Self {
        Self {
            mesh,
            transform: Transform3::IDENTITY,
            material,
        }
    }
    #[must_use]
    pub fn with_transform(mut self, transform: Transform3) -> Self {
        self.transform = transform;
        self
    }
}

/// Real-time light source. Directional `direction` describes the direction
/// light rays travel (the vector toward the light is its negation).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Light3 {
    Ambient {
        color: Color,
        intensity: f32,
    },
    Directional {
        direction: Vec3,
        color: Color,
        intensity: f32,
    },
    Point {
        position: Vec3,
        color: Color,
        intensity: f32,
        range: f32,
    },
    Spot {
        position: Vec3,
        direction: Vec3,
        inner_angle: f32,
        outer_angle: f32,
        color: Color,
        intensity: f32,
        range: f32,
    },
}
impl Light3 {
    /// Return false for malformed public-field values rather than allowing NaNs
    /// or a degenerate direction to poison an entire frame.
    #[must_use]
    pub fn is_valid(self) -> bool {
        match self {
            Self::Ambient { intensity, .. } => intensity.is_finite() && intensity >= 0.0,
            Self::Directional {
                direction,
                intensity,
                ..
            } => {
                direction.is_finite()
                    && direction.length_squared().is_finite()
                    && direction.length_squared() > 1.0e-12
                    && intensity.is_finite()
                    && intensity >= 0.0
            }
            Self::Point {
                position,
                intensity,
                range,
                ..
            } => {
                position.is_finite()
                    && intensity.is_finite()
                    && intensity >= 0.0
                    && range.is_finite()
                    && range > 0.0
            }
            Self::Spot {
                position,
                direction,
                inner_angle,
                outer_angle,
                intensity,
                range,
                ..
            } => {
                position.is_finite()
                    && direction.is_finite()
                    && direction.length_squared().is_finite()
                    && direction.length_squared() > 1.0e-12
                    && inner_angle.is_finite()
                    && outer_angle.is_finite()
                    && inner_angle >= 0.0
                    && inner_angle <= outer_angle
                    && outer_angle <= std::f32::consts::PI
                    && intensity.is_finite()
                    && intensity >= 0.0
                    && range.is_finite()
                    && range > 0.0
            }
        }
    }
}

/// Projection model and clipping planes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Projection3 {
    Perspective {
        vertical_fov_radians: f32,
        near: f32,
        far: f32,
    },
    Orthographic {
        vertical_size: f32,
        near: f32,
        far: f32,
    },
}
impl Projection3 {
    pub fn perspective(
        vertical_fov_radians: f32,
        near: f32,
        far: f32,
    ) -> Result<Self, CameraError> {
        if !vertical_fov_radians.is_finite()
            || !(0.001..std::f32::consts::PI - 0.001).contains(&vertical_fov_radians)
        {
            return Err(CameraError::InvalidFieldOfView);
        }
        validate_clip_planes(near, far)?;
        Ok(Self::Perspective {
            vertical_fov_radians,
            near,
            far,
        })
    }
    pub fn orthographic(vertical_size: f32, near: f32, far: f32) -> Result<Self, CameraError> {
        if !vertical_size.is_finite() || vertical_size <= 0.0 {
            return Err(CameraError::InvalidOrthographicSize);
        }
        validate_clip_planes(near, far)?;
        Ok(Self::Orthographic {
            vertical_size,
            near,
            far,
        })
    }
    #[must_use]
    pub fn clip_planes(self) -> (f32, f32) {
        match self {
            Self::Perspective { near, far, .. } | Self::Orthographic { near, far, .. } => {
                (near, far)
            }
        }
    }
}
fn validate_clip_planes(near: f32, far: f32) -> Result<(), CameraError> {
    if !near.is_finite() || !far.is_finite() || near <= 0.0 || far <= near {
        Err(CameraError::InvalidClipPlanes)
    } else {
        Ok(())
    }
}

/// Camera validation errors are returned at construction, not hidden as NaNs
/// in a later frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraError {
    InvalidFieldOfView,
    InvalidOrthographicSize,
    InvalidClipPlanes,
    DegenerateView,
    ParallelUpVector,
    NonFinitePose,
    InvalidOrientation,
}
impl std::fmt::Display for CameraError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidFieldOfView => "perspective field of view must be between zero and pi",
            Self::InvalidOrthographicSize => "orthographic vertical size must be positive",
            Self::InvalidClipPlanes => "clip planes require 0 < near < far",
            Self::DegenerateView => "camera eye and target must differ",
            Self::ParallelUpVector => "camera up vector must not be parallel to its view direction",
            Self::NonFinitePose => "camera position, target and up vector must be finite",
            Self::InvalidOrientation => "camera orientation must be finite and non-degenerate",
        };
        f.write_str(message)
    }
}
impl std::error::Error for CameraError {}

/// Camera data in world space. Near/far clipping uses positive distance along
/// `forward`, which keeps the rasterizer independent from graphics-API clip
/// coordinate conventions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera3 {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub projection: Projection3,
}
impl Camera3 {
    pub fn look_at(
        eye: Vec3,
        target: Vec3,
        up: Vec3,
        projection: Projection3,
    ) -> Result<Self, CameraError> {
        let camera = Self {
            eye,
            target,
            up,
            projection,
        };
        camera.validate()?;
        Ok(Self {
            up: up.normalized(),
            ..camera
        })
    }

    /// Revalidate camera fields at use sites. The fields and projection enum are
    /// public for ergonomics, so constructing a struct literal must not bypass
    /// the checks performed by `look_at` and the projection constructors.
    pub fn validate(self) -> Result<(), CameraError> {
        if !self.eye.is_finite() || !self.target.is_finite() || !self.up.is_finite() {
            return Err(CameraError::NonFinitePose);
        }
        match self.projection {
            Projection3::Perspective {
                vertical_fov_radians,
                near,
                far,
            } => {
                Projection3::perspective(vertical_fov_radians, near, far)?;
            }
            Projection3::Orthographic {
                vertical_size,
                near,
                far,
            } => {
                Projection3::orthographic(vertical_size, near, far)?;
            }
        }
        let forward = (self.target - self.eye).normalized();
        if forward == Vec3::ZERO {
            return Err(CameraError::DegenerateView);
        }
        let up = self.up.normalized();
        if up == Vec3::ZERO || forward.cross(up).length_squared() < 1.0e-8 {
            return Err(CameraError::ParallelUpVector);
        }
        Ok(())
    }
    pub fn perspective(
        eye: Vec3,
        target: Vec3,
        up: Vec3,
        vertical_fov_radians: f32,
        near: f32,
        far: f32,
    ) -> Result<Self, CameraError> {
        Self::look_at(
            eye,
            target,
            up,
            Projection3::perspective(vertical_fov_radians, near, far)?,
        )
    }
    pub fn orthographic(
        eye: Vec3,
        target: Vec3,
        up: Vec3,
        vertical_size: f32,
        near: f32,
        far: f32,
    ) -> Result<Self, CameraError> {
        Self::look_at(
            eye,
            target,
            up,
            Projection3::orthographic(vertical_size, near, far)?,
        )
    }
    #[must_use]
    pub fn forward(self) -> Vec3 {
        (self.target - self.eye).normalized()
    }
    #[must_use]
    pub fn right(self) -> Vec3 {
        self.forward().cross(self.up).normalized()
    }
    #[must_use]
    pub fn true_up(self) -> Vec3 {
        self.right().cross(self.forward()).normalized()
    }
    #[must_use]
    pub fn camera_space(self, point: Vec3) -> Vec3 {
        let relative = point - self.eye;
        Vec3::new(
            relative.dot(self.right()),
            relative.dot(self.true_up()),
            relative.dot(self.forward()),
        )
    }
    pub fn with_pose(self, eye: Vec3, target: Vec3, up: Vec3) -> Result<Self, CameraError> {
        Self::look_at(eye, target, up, self.projection)
    }
}

/// The scene rendered into an image that can be passed to ViewW's existing
/// `Image` widget or GPU compositor. All geometry and light state is owned by
/// the caller, not a global renderer singleton.
#[derive(Debug, Clone)]
pub struct Scene3 {
    pub objects: Vec<MeshInstance3>,
    pub lights: Vec<Light3>,
    pub background: Color,
}
impl Default for Scene3 {
    fn default() -> Self {
        Self {
            objects: Vec::new(),
            lights: vec![
                Light3::Ambient {
                    color: Color::rgba(255, 255, 255, 255),
                    intensity: 0.22,
                },
                Light3::Directional {
                    direction: Vec3::new(-0.4, -0.8, -0.6),
                    color: Color::rgba(255, 255, 255, 255),
                    intensity: 0.9,
                },
            ],
            background: Color::rgba(12, 16, 24, 255),
        }
    }
}
impl Scene3 {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add(&mut self, object: MeshInstance3) {
        self.objects.push(object);
    }
    pub fn add_light(&mut self, light: Light3) {
        self.lights.push(light);
    }
}

/// A compact helper for composing a pose with a camera's forward/up axes.
pub fn camera_from_pose(
    position: Vec3,
    orientation: Quat,
    projection: Projection3,
) -> Result<Camera3, CameraError> {
    let orientation_length_squared = orientation.length_squared();
    if !position.is_finite() {
        return Err(CameraError::NonFinitePose);
    }
    if !orientation.is_finite()
        || !orientation_length_squared.is_finite()
        || orientation_length_squared <= 1.0e-12
    {
        return Err(CameraError::InvalidOrientation);
    }
    let orientation = orientation.normalized();
    Camera3::look_at(
        position,
        position + orientation.rotate(Vec3::new(0.0, 0.0, -1.0)),
        orientation.rotate(Vec3::Y),
        projection,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_has_twelve_triangles_and_hard_face_normals() {
        let cube = Mesh3::cube();
        cube.validate().unwrap();
        assert_eq!(cube.indices.len(), 36);
        assert_eq!(cube.vertices.len(), 24);
    }

    #[test]
    fn obj_triangulates_quads_and_resolves_negative_indices() {
        let mesh =
            Mesh3::from_obj("v -1 -1 0\nv 1 -1 0\nv 1 1 0\nv -1 1 0\nf -4 -3 -2 -1\n").unwrap();
        assert_eq!(mesh.indices, vec![0, 1, 2, 0, 2, 3]);
    }

    #[test]
    fn obj_reports_bad_indices_with_a_line_number() {
        let error = Mesh3::from_obj("v 0 0 0\nf 1 2 3\n").unwrap_err();
        assert!(error.to_string().contains("OBJ line 2"));
    }

    #[test]
    fn obj_rejects_extra_slash_fields_and_extreme_negative_indices() {
        let vertices = "v 0 0 0\nv 1 0 0\nv 0 1 0\n";
        assert!(Mesh3::from_obj(&format!("{vertices}f 1/1/1/1 2 3\n")).is_err());
        assert!(Mesh3::from_obj(&format!("{vertices}f {} 2 3\n", isize::MIN)).is_err());
    }

    #[test]
    fn invalid_projection_is_not_deferred_to_render_time() {
        assert_eq!(
            Projection3::perspective(0.0, 0.1, 100.0),
            Err(CameraError::InvalidFieldOfView)
        );
        assert_eq!(
            Projection3::orthographic(0.0, 0.1, 10.0),
            Err(CameraError::InvalidOrthographicSize)
        );
    }

    #[test]
    fn public_camera_literals_are_revalidated_at_render_boundaries() {
        let camera = Camera3 {
            eye: Vec3::ZERO,
            target: Vec3::ZERO,
            up: Vec3::Y,
            projection: Projection3::Perspective {
                vertical_fov_radians: 1.0,
                near: 0.1,
                far: 100.0,
            },
        };
        assert_eq!(camera.validate(), Err(CameraError::DegenerateView));
        assert_eq!(
            camera_from_pose(
                Vec3::ZERO,
                Quat::new(f32::MAX, f32::MAX, 0.0, 1.0),
                Projection3::perspective(1.0, 0.1, 100.0).unwrap()
            ),
            Err(CameraError::InvalidOrientation)
        );
    }

    #[test]
    fn material_and_light_values_reject_non_finite_public_fields() {
        let mut material = Material3 {
            metallic: f32::NAN,
            ..Material3::default()
        };
        assert!(!material.is_valid());
        material = Material3::default().metallic_roughness(f32::NAN, f32::INFINITY);
        assert!(material.is_valid());
        assert!(!Light3::Ambient {
            color: Color::WHITE,
            intensity: f32::NAN
        }
        .is_valid());
        assert!(!Light3::Directional {
            direction: Vec3::ZERO,
            color: Color::WHITE,
            intensity: 1.0
        }
        .is_valid());
        assert!(!Light3::Directional {
            direction: Vec3::new(f32::MAX, f32::MAX, f32::MAX),
            color: Color::WHITE,
            intensity: 1.0
        }
        .is_valid());
    }
}
