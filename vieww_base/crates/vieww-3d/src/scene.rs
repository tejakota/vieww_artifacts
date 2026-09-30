//! The scene graph: nodes with TRS transforms, meshes with materials,
//! lights, cameras — and a declarative reconciler over it.
//!
//! # Three.js' `Object3D`, with the dirty flag
//!
//! A [`Scene`] is an arena of [`Node`]s. Each node has a local position,
//! rotation (quaternion) and scale, a parent and children, and optional
//! content — a mesh, an instanced mesh, a LOD group or a light. World
//! matrices are cached and recomputed only for subtrees whose local
//! transform changed since the last [`Scene::update_world`] — Three.js'
//! `updateMatrixWorld` with its `matrixWorldNeedsUpdate` flag, which the
//! document names as Three.js' performance-critical choice (Part 6).
//!
//! # React Three Fiber's reconciler, in one method
//!
//! R3F's whole contribution (§2.3) is that a scene can be *described* each
//! frame — `<mesh position={…}>` — and a reconciler diffs the description
//! against the live objects: create what is new, update props on what
//! persists, dispose what is gone. [`Scene::reconcile`] is that, keyed by
//! [`NodeDesc::key`]: it returns counts of created, updated and removed
//! nodes so a caller (or a test) can see that an unchanged description
//! touches nothing.

use std::collections::BTreeMap;
use std::sync::Arc;

use vieww_foundation::Color;
use vieww_mesh::Mesh;

use crate::geometry::bounding_sphere;
use crate::math::{Mat4, Quat, Vec3};

/// Linear-light RGB in 0..=1 (and beyond, for emission).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Rgb {
    pub const BLACK: Self = Self::new(0.0, 0.0, 0.0);
    pub const WHITE: Self = Self::new(1.0, 1.0, 1.0);

    #[must_use]
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    /// From an sRGB [`Color`], linearised.
    #[must_use]
    pub fn from_color(c: Color) -> Self {
        let l = c.to_linear();
        Self::new(l[0], l[1], l[2])
    }

    #[must_use]
    pub fn scale(self, s: f32) -> Self {
        Self::new(self.r * s, self.g * s, self.b * s)
    }

    /// Component-wise product (filtering light by a surface colour).
    #[must_use]
    #[allow(clippy::should_implement_trait)]
    pub fn mul(self, o: Self) -> Self {
        Self::new(self.r * o.r, self.g * o.g, self.b * o.b)
    }

    #[must_use]
    #[allow(clippy::should_implement_trait)]
    pub fn add(self, o: Self) -> Self {
        Self::new(self.r + o.r, self.g + o.g, self.b + o.b)
    }

    #[must_use]
    pub fn lerp(self, o: Self, t: f32) -> Self {
        Self::new(
            self.r + (o.r - self.r) * t,
            self.g + (o.g - self.g) * t,
            self.b + (o.b - self.b) * t,
        )
    }

    #[must_use]
    pub fn luminance(self) -> f32 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }
}

/// How texels are fetched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    Nearest,
    #[default]
    Bilinear,
}

/// An RGBA8 image sampled by UV, repeating.
#[derive(Debug, Clone, PartialEq)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    /// Linear-light texels (converted once from sRGB at construction).
    texels: Arc<Vec<[f32; 4]>>,
    pub filter: Filter,
}

impl Texture {
    /// From sRGB RGBA8 pixels.
    #[must_use]
    pub fn from_rgba8(width: u32, height: u32, pixels: &[u8]) -> Self {
        let texels = pixels
            .chunks_exact(4)
            .map(|p| {
                let c = Color::rgba(p[0], p[1], p[2], p[3]).to_linear();
                [c[0], c[1], c[2], f32::from(p[3]) / 255.0]
            })
            .collect();
        Self {
            width,
            height,
            texels: Arc::new(texels),
            filter: Filter::Bilinear,
        }
    }

    /// From a foundation [`Image`](vieww_foundation::Image).
    #[must_use]
    pub fn from_image(image: &vieww_foundation::Image) -> Self {
        Self::from_rgba8(image.width(), image.height(), image.pixels())
    }

    /// A checkerboard of `cells × cells` squares — the UV test texture.
    #[must_use]
    pub fn checker(size: u32, cells: u32, a: Color, b: Color) -> Self {
        let mut px = Vec::with_capacity((size * size * 4) as usize);
        let cell = (size / cells.max(1)).max(1);
        for y in 0..size {
            for x in 0..size {
                let c = if ((x / cell) + (y / cell)) % 2 == 0 { a } else { b };
                px.extend_from_slice(&[c.r, c.g, c.b, c.a]);
            }
        }
        Self::from_rgba8(size, size, &px)
    }

    fn texel(&self, x: i64, y: i64) -> [f32; 4] {
        let w = i64::from(self.width);
        let h = i64::from(self.height);
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let i = (y.rem_euclid(h) * w + x.rem_euclid(w)) as usize;
        self.texels[i]
    }

    /// Sample at `(u, v)`; v = 0 is the top row (glTF's convention).
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    pub fn sample(&self, u: f32, v: f32) -> [f32; 4] {
        if self.width == 0 || self.height == 0 {
            return [1.0; 4];
        }
        let x = u * self.width as f32 - 0.5;
        let y = v * self.height as f32 - 0.5;
        match self.filter {
            Filter::Nearest => self.texel(x.round() as i64, y.round() as i64),
            Filter::Bilinear => {
                let (x0, y0) = (x.floor(), y.floor());
                let (fx, fy) = (x - x0, y - y0);
                let (x0, y0) = (x0 as i64, y0 as i64);
                let a = self.texel(x0, y0);
                let b = self.texel(x0 + 1, y0);
                let c = self.texel(x0, y0 + 1);
                let d = self.texel(x0 + 1, y0 + 1);
                let mut out = [0.0; 4];
                for k in 0..4 {
                    let top = a[k] + (b[k] - a[k]) * fx;
                    let bot = c[k] + (d[k] - c[k]) * fx;
                    out[k] = top + (bot - top) * fy;
                }
                out
            }
        }
    }
}

/// The shading model — Three.js' material classes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shading {
    /// `MeshBasicMaterial`: colour only, no lights.
    Basic,
    /// `MeshLambertMaterial`: diffuse.
    Lambert,
    /// `MeshPhongMaterial`: diffuse + Blinn–Phong specular.
    Phong { shininess: f32, specular: f32 },
    /// `MeshStandardMaterial`: metallic–roughness with a GGX specular lobe.
    Standard { metallic: f32, roughness: f32 },
    /// `MeshToonMaterial`: diffuse quantised to `bands`.
    Toon { bands: u32 },
    /// `MeshNormalMaterial`: the view-space normal as colour.
    Normal,
}

/// What a surface looks like.
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    pub shading: Shading,
    pub color: Rgb,
    pub emissive: Rgb,
    pub map: Option<Texture>,
    /// 1 is opaque; below 1 the mesh is drawn in the transparent pass.
    pub opacity: f32,
    pub double_sided: bool,
    pub wireframe: bool,
    /// Face normals instead of interpolated ones.
    pub flat: bool,
    pub cast_shadow: bool,
    pub receive_shadow: bool,
}

impl Material {
    fn with(shading: Shading, color: Color) -> Self {
        Self {
            shading,
            color: Rgb::from_color(color),
            emissive: Rgb::BLACK,
            map: None,
            opacity: 1.0,
            double_sided: false,
            wireframe: false,
            flat: false,
            cast_shadow: true,
            receive_shadow: true,
        }
    }

    #[must_use]
    pub fn basic(color: Color) -> Self {
        Self::with(Shading::Basic, color)
    }
    #[must_use]
    pub fn lambert(color: Color) -> Self {
        Self::with(Shading::Lambert, color)
    }
    #[must_use]
    pub fn phong(color: Color, shininess: f32) -> Self {
        Self::with(Shading::Phong { shininess, specular: 0.5 }, color)
    }
    #[must_use]
    pub fn standard(color: Color, metallic: f32, roughness: f32) -> Self {
        Self::with(Shading::Standard { metallic, roughness }, color)
    }
    #[must_use]
    pub fn toon(color: Color, bands: u32) -> Self {
        Self::with(Shading::Toon { bands }, color)
    }
    #[must_use]
    pub fn normal() -> Self {
        Self::with(Shading::Normal, Color::WHITE)
    }

    #[must_use]
    pub fn map(mut self, t: Texture) -> Self {
        self.map = Some(t);
        self
    }
    #[must_use]
    pub const fn emissive(mut self, e: Rgb) -> Self {
        self.emissive = e;
        self
    }
    #[must_use]
    pub const fn opacity(mut self, o: f32) -> Self {
        self.opacity = o;
        self
    }
    #[must_use]
    pub const fn wireframe(mut self) -> Self {
        self.wireframe = true;
        self
    }
    #[must_use]
    pub const fn flat(mut self) -> Self {
        self.flat = true;
        self
    }
    #[must_use]
    pub const fn double_sided(mut self) -> Self {
        self.double_sided = true;
        self
    }
}

/// A light source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Light {
    Ambient { color: Rgb, intensity: f32 },
    /// Sky colour from above, ground colour from below.
    Hemisphere { sky: Rgb, ground: Rgb, intensity: f32 },
    /// Parallel rays travelling along `direction` (the node's rotation
    /// applies). Casts a shadow map when `shadow` is set.
    Directional { color: Rgb, intensity: f32, direction: Vec3, shadow: bool },
    /// From the node's world position, falling off to zero at `range`.
    Point { color: Rgb, intensity: f32, range: f32 },
    /// A cone from the node's position along `direction`.
    Spot { color: Rgb, intensity: f32, range: f32, direction: Vec3, angle: f32, penumbra: f32 },
}

/// What a node carries.
#[derive(Debug, Clone)]
pub enum Content {
    Empty,
    Mesh { mesh: Arc<Mesh>, material: Arc<Material>, bounds: (Vec3, f32) },
    /// One mesh drawn at many local transforms — `InstancedMesh`.
    Instanced { mesh: Arc<Mesh>, material: Arc<Material>, bounds: (Vec3, f32), instances: Vec<Mat4> },
    /// Meshes chosen by camera distance — `LOD` / Unity's `LODGroup`.
    Lod { levels: Vec<(f32, Arc<Mesh>)>, material: Arc<Material>, bounds: (Vec3, f32) },
    Light(Light),
}

impl Content {
    /// A mesh with a material (the bounding sphere is computed here, once).
    #[must_use]
    pub fn mesh(mesh: Mesh, material: Material) -> Self {
        let bounds = bounding_sphere(&mesh);
        Self::Mesh {
            mesh: Arc::new(mesh),
            material: Arc::new(material),
            bounds,
        }
    }

    #[must_use]
    pub fn instanced(mesh: Mesh, material: Material, instances: Vec<Mat4>) -> Self {
        let bounds = bounding_sphere(&mesh);
        Self::Instanced {
            mesh: Arc::new(mesh),
            material: Arc::new(material),
            bounds,
            instances,
        }
    }

    /// Levels as `(max distance, mesh)`, nearest first; the last level
    /// serves every distance beyond.
    #[must_use]
    pub fn lod(levels: Vec<(f32, Mesh)>, material: Material) -> Self {
        let bounds = levels.first().map_or((Vec3::ZERO, 0.0), |(_, m)| bounding_sphere(m));
        Self::Lod {
            levels: levels.into_iter().map(|(d, m)| (d, Arc::new(m))).collect(),
            material: Arc::new(material),
            bounds,
        }
    }
}

/// Index of a node in its scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub usize);

/// One object in the graph.
#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    pub visible: bool,
    pub content: Content,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    world: Mat4,
    dirty: bool,
    alive: bool,
    key: Option<String>,
}

impl Node {
    #[must_use]
    pub fn new(name: &str, content: Content) -> Self {
        Self {
            name: name.to_owned(),
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
            visible: true,
            content,
            parent: None,
            children: Vec::new(),
            world: Mat4::IDENTITY,
            dirty: true,
            alive: true,
            key: None,
        }
    }

    #[must_use]
    pub const fn at(mut self, p: Vec3) -> Self {
        self.position = p;
        self
    }

    #[must_use]
    pub const fn rotated(mut self, q: Quat) -> Self {
        self.rotation = q;
        self
    }

    #[must_use]
    pub const fn scaled(mut self, s: Vec3) -> Self {
        self.scale = s;
        self
    }

    /// The local matrix.
    #[must_use]
    pub fn local(&self) -> Mat4 {
        Mat4::compose(self.position, self.rotation, self.scale)
    }

    /// The cached world matrix (valid after [`Scene::update_world`]).
    #[must_use]
    pub const fn world(&self) -> Mat4 {
        self.world
    }

    #[must_use]
    pub const fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    #[must_use]
    pub fn children(&self) -> &[NodeId] {
        &self.children
    }
}

/// How the camera projects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Projection {
    /// Vertical field of view in radians.
    Perspective { fov_y: f32, near: f32, far: f32 },
    /// Half the visible height in world units.
    Orthographic { half_height: f32, near: f32, far: f32 },
}

/// A viewpoint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub projection: Projection,
}

impl Camera {
    #[must_use]
    pub const fn perspective(position: Vec3, target: Vec3, fov_y: f32) -> Self {
        Self {
            position,
            target,
            up: Vec3::Y,
            projection: Projection::Perspective { fov_y, near: 0.1, far: 1000.0 },
        }
    }

    #[must_use]
    pub const fn orthographic(position: Vec3, target: Vec3, half_height: f32) -> Self {
        Self {
            position,
            target,
            up: Vec3::Y,
            projection: Projection::Orthographic { half_height, near: 0.1, far: 1000.0 },
        }
    }

    #[must_use]
    pub fn view(&self) -> Mat4 {
        Mat4::look_at(self.position, self.target, self.up)
    }

    #[must_use]
    pub fn projection(&self, aspect: f32) -> Mat4 {
        match self.projection {
            Projection::Perspective { fov_y, near, far } => Mat4::perspective(fov_y, aspect, near, far),
            Projection::Orthographic { half_height, near, far } => {
                let hw = half_height * aspect;
                Mat4::orthographic(-hw, hw, -half_height, half_height, near, far)
            }
        }
    }
}

/// Orbit controls — drei's `OrbitControls`: yaw and pitch around a target
/// at a distance, driven by drag and wheel deltas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitControls {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    /// Radians per pixel of drag.
    pub speed: f32,
}

impl OrbitControls {
    #[must_use]
    pub const fn new(target: Vec3, distance: f32) -> Self {
        Self {
            target,
            distance,
            yaw: 0.0,
            pitch: 0.3,
            min_distance: 0.5,
            max_distance: 500.0,
            speed: 0.01,
        }
    }

    /// A pointer drag of `(dx, dy)` pixels.
    pub fn drag(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * self.speed;
        self.pitch = (self.pitch + dy * self.speed).clamp(-1.55, 1.55);
    }

    /// A wheel step: positive zooms out, as a scale factor per notch.
    pub fn zoom(&mut self, notches: f32) {
        self.distance = (self.distance * 1.1f32.powf(notches)).clamp(self.min_distance, self.max_distance);
    }

    /// Where the camera sits.
    #[must_use]
    pub fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + Vec3::new(sy * cp, sp, cy * cp) * self.distance
    }

    /// A perspective camera at the orbit position.
    #[must_use]
    pub fn camera(&self, fov_y: f32) -> Camera {
        Camera::perspective(self.eye(), self.target, fov_y)
    }
}

/// A declarative node description, for [`Scene::reconcile`].
#[derive(Debug, Clone)]
pub struct NodeDesc {
    pub key: String,
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    pub visible: bool,
    /// The content, and a version the caller bumps when it changes (so an
    /// unchanged mesh is not re-uploaded — the prop identity R3F compares).
    pub content: (u64, Content),
    pub children: Vec<NodeDesc>,
}

impl NodeDesc {
    #[must_use]
    pub fn new(key: &str, content: Content) -> Self {
        Self {
            key: key.to_owned(),
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
            visible: true,
            content: (0, content),
            children: Vec::new(),
        }
    }
    #[must_use]
    pub const fn at(mut self, p: Vec3) -> Self {
        self.position = p;
        self
    }
    #[must_use]
    pub const fn rotated(mut self, q: Quat) -> Self {
        self.rotation = q;
        self
    }
    #[must_use]
    pub const fn scaled(mut self, s: Vec3) -> Self {
        self.scale = s;
        self
    }
    #[must_use]
    pub const fn version(mut self, v: u64) -> Self {
        self.content.0 = v;
        self
    }
    #[must_use]
    pub fn child(mut self, c: Self) -> Self {
        self.children.push(c);
        self
    }
}

/// What a reconcile did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReconcileStats {
    pub created: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub removed: usize,
}

/// The graph. See the [module docs](self).
#[derive(Debug, Clone, Default)]
pub struct Scene {
    nodes: Vec<Node>,
    roots: Vec<NodeId>,
    pub background: Rgb,
    /// Linear fog from `near` to `far` toward `color`.
    pub fog: Option<(Rgb, f32, f32)>,
    content_versions: BTreeMap<String, u64>,
    /// World-matrix recomputations in the last `update_world` — the
    /// dirty flag's receipt.
    pub last_world_updates: usize,
}

impl Scene {
    #[must_use]
    pub fn new() -> Self {
        Self {
            background: Rgb::new(0.02, 0.02, 0.03),
            ..Self::default()
        }
    }

    /// Add a node under `parent` (or as a root).
    pub fn add(&mut self, node: Node, parent: Option<NodeId>) -> NodeId {
        let id = NodeId(self.nodes.len());
        let mut node = node;
        node.parent = parent;
        node.dirty = true;
        self.nodes.push(node);
        match parent {
            Some(p) => self.nodes[p.0].children.push(id),
            None => self.roots.push(id),
        }
        id
    }

    /// Detach and drop a node and its subtree.
    pub fn remove(&mut self, id: NodeId) {
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            stack.extend(self.nodes[n.0].children.clone());
            self.nodes[n.0].alive = false;
            self.nodes[n.0].children.clear();
        }
        match self.nodes[id.0].parent {
            Some(p) => self.nodes[p.0].children.retain(|c| *c != id),
            None => self.roots.retain(|c| *c != id),
        }
    }

    #[must_use]
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0]
    }

    /// Mutable access; marks the node's transform dirty.
    pub fn node_mut(&mut self, id: NodeId) -> &mut Node {
        self.nodes[id.0].dirty = true;
        &mut self.nodes[id.0]
    }

    /// The first live node named `name` — `getObjectByName`.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<NodeId> {
        self.nodes.iter().position(|n| n.alive && n.name == name).map(NodeId)
    }

    /// Live node ids, parents before children.
    #[must_use]
    pub fn traverse(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self.roots.iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            out.push(id);
            stack.extend(self.nodes[id.0].children.iter().rev().copied());
        }
        out
    }

    /// How many live nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.iter().filter(|n| n.alive).count()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Recompute world matrices of dirty subtrees.
    pub fn update_world(&mut self) {
        let mut count = 0;
        let mut stack: Vec<(NodeId, Mat4, bool)> = self.roots.iter().map(|r| (*r, Mat4::IDENTITY, false)).collect();
        while let Some((id, parent_world, parent_changed)) = stack.pop() {
            let node = &mut self.nodes[id.0];
            let changed = parent_changed || node.dirty;
            if changed {
                node.world = parent_world * node.local();
                node.dirty = false;
                count += 1;
            }
            let world = node.world;
            for c in node.children.clone() {
                stack.push((c, world, changed));
            }
        }
        self.last_world_updates = count;
    }

    /// Visible if it and all its ancestors are.
    #[must_use]
    pub fn effectively_visible(&self, id: NodeId) -> bool {
        let mut cur = Some(id);
        while let Some(c) = cur {
            if !self.nodes[c.0].visible {
                return false;
            }
            cur = self.nodes[c.0].parent;
        }
        true
    }

    /// Make the scene match `desc` (roots), keyed. See the module docs.
    pub fn reconcile(&mut self, desc: &[NodeDesc]) -> ReconcileStats {
        let mut stats = ReconcileStats::default();
        let mut seen = Vec::new();
        for d in desc {
            self.reconcile_one(d, None, &mut stats, &mut seen);
        }
        // Remove keyed nodes no longer described.
        let stale: Vec<NodeId> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, n)| n.alive && n.key.is_some() && !seen.contains(&NodeId(*i)))
            .map(|(i, _)| NodeId(i))
            .collect();
        for id in stale {
            if self.nodes[id.0].alive {
                self.remove(id);
                stats.removed += 1;
            }
        }
        stats
    }

    fn reconcile_one(&mut self, d: &NodeDesc, parent: Option<NodeId>, stats: &mut ReconcileStats, seen: &mut Vec<NodeId>) {
        let existing = self
            .nodes
            .iter()
            .position(|n| n.alive && n.key.as_deref() == Some(d.key.as_str()) && n.parent == parent)
            .map(NodeId);
        let id = match existing {
            Some(id) => {
                let old_version = self.content_versions.get(&d.key).copied();
                let n = &self.nodes[id.0];
                let same = n.position == d.position
                    && n.rotation == d.rotation
                    && n.scale == d.scale
                    && n.visible == d.visible
                    && old_version == Some(d.content.0);
                if same {
                    stats.unchanged += 1;
                } else {
                    let n = self.node_mut(id);
                    n.position = d.position;
                    n.rotation = d.rotation;
                    n.scale = d.scale;
                    n.visible = d.visible;
                    if old_version != Some(d.content.0) {
                        n.content = d.content.1.clone();
                    }
                    stats.updated += 1;
                }
                id
            }
            None => {
                let mut node = Node::new(&d.key, d.content.1.clone())
                    .at(d.position)
                    .rotated(d.rotation)
                    .scaled(d.scale);
                node.visible = d.visible;
                node.key = Some(d.key.clone());
                stats.created += 1;
                self.add(node, parent)
            }
        };
        self.content_versions.insert(d.key.clone(), d.content.0);
        seen.push(id);
        for c in &d.children {
            self.reconcile_one(c, Some(id), stats, seen);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::box_mesh;

    #[test]
    fn world_matrices_chain_and_only_dirty_subtrees_recompute() {
        let mut s = Scene::new();
        let a = s.add(Node::new("a", Content::Empty).at(Vec3::new(1.0, 0.0, 0.0)), None);
        let b = s.add(Node::new("b", Content::Empty).at(Vec3::new(0.0, 2.0, 0.0)), Some(a));
        let c = s.add(Node::new("c", Content::Empty), None);
        s.update_world();
        assert_eq!(s.last_world_updates, 3);
        assert_eq!(s.node(b).world().get_translation(), Vec3::new(1.0, 2.0, 0.0));
        s.update_world();
        assert_eq!(s.last_world_updates, 0, "nothing changed, nothing recomputed");
        s.node_mut(a).position = Vec3::new(5.0, 0.0, 0.0);
        s.update_world();
        assert_eq!(s.last_world_updates, 2, "a and its child, not c");
        assert_eq!(s.node(b).world().get_translation(), Vec3::new(5.0, 2.0, 0.0));
        let _ = c;
        assert_eq!(s.find("b"), Some(b));
    }

    #[test]
    fn remove_drops_the_subtree() {
        let mut s = Scene::new();
        let a = s.add(Node::new("a", Content::Empty), None);
        s.add(Node::new("b", Content::Empty), Some(a));
        s.remove(a);
        assert!(s.is_empty());
        assert!(s.find("b").is_none());
    }

    #[test]
    fn reconcile_creates_updates_skips_and_removes_by_key() {
        let mut s = Scene::new();
        let mesh = || Content::mesh(box_mesh(1.0, 1.0, 1.0), Material::lambert(Color::WHITE));
        let desc = |x: f32, with_b: bool| {
            let mut root = NodeDesc::new("root", Content::Empty).child(NodeDesc::new("a", mesh()).at(Vec3::new(x, 0.0, 0.0)));
            if with_b {
                root = root.child(NodeDesc::new("b", mesh()));
            }
            vec![root]
        };
        assert_eq!(s.reconcile(&desc(0.0, true)), ReconcileStats { created: 3, ..Default::default() });
        assert_eq!(s.reconcile(&desc(0.0, true)), ReconcileStats { unchanged: 3, ..Default::default() });
        assert_eq!(s.reconcile(&desc(2.0, true)), ReconcileStats { updated: 1, unchanged: 2, ..Default::default() });
        assert_eq!(s.reconcile(&desc(2.0, false)), ReconcileStats { unchanged: 2, removed: 1, ..Default::default() });
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn orbit_controls_orbit_and_zoom() {
        let mut o = OrbitControls::new(Vec3::ZERO, 10.0);
        o.pitch = 0.0;
        assert!((o.eye() - Vec3::new(0.0, 0.0, 10.0)).length() < 1e-4);
        o.drag(-std::f32::consts::FRAC_PI_2 / o.speed, 0.0);
        assert!((o.eye() - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-3);
        o.zoom(1.0);
        assert!((o.distance - 11.0).abs() < 1e-4);
        o.drag(0.0, 10_000.0);
        assert!(o.pitch <= 1.55);
    }

    #[test]
    fn textures_sample_and_wrap() {
        let t = Texture::checker(4, 2, Color::WHITE, Color::BLACK);
        let mut t = t;
        t.filter = Filter::Nearest;
        assert!(t.sample(0.1, 0.1)[0] > 0.9);
        assert!(t.sample(0.9, 0.1)[0] < 0.1);
        assert_eq!(t.sample(1.1, 0.1), t.sample(0.1, 0.1), "repeats");
    }
}
