//! glTF 2.0 — `.gltf` (JSON with embedded or external buffers) and `.glb`
//! (the binary container).
//!
//! # Why now
//!
//! This crate's header used to defer glTF: "a JSON parser, a base64
//! decoder, a scene hierarchy and a material system — four subsystems".
//! Since then the foundation grew the JSON reader (`vieww_foundation::json`)
//! and `vieww-3d` grew the scene graph and the material system, so the cost
//! the deferral priced is paid; what is left is the format itself, which is
//! this file. glTF is what Three.js' `GLTFLoader`, R3F's `useGLTF`, Godot's
//! importer and Blender's exporter all speak, so it is the format a 3D
//! framework is asked for first.
//!
//! # What is read
//!
//! | part | support |
//! |---|---|
//! | buffers | `data:` URIs (base64), external files through a resolver closure, the GLB `BIN` chunk |
//! | accessors | every component type (`i8`…`f32`), normalised integers, byte strides, `SCALAR`…`MAT4` |
//! | primitives | `POSITION`, `NORMAL`, `TEXCOORD_0`, indices; modes triangles, strip and fan (points and lines refused by name) |
//! | materials | `pbrMetallicRoughness` factors, `emissiveFactor`, `alphaMode`, `doubleSided`, `name` |
//! | nodes | hierarchy, `translation`/`rotation`/`scale` or `matrix`, `mesh`, `name` |
//! | scenes | every scene's roots and the default `scene` |
//! | animations | node TRS channels with `LINEAR`, `STEP` and `CUBICSPLINE` samplers, sampled by [`Gltf::sample`] |
//!
//! Not read, and named so nobody hunts for them: skins and morph targets
//! (skinned 3D characters — the 2D bone system lives in `vieww-animation`),
//! textures and images (a material's `baseColorTexture` index is kept in
//! [`GltfMaterial::base_color_texture`] for a caller with an image decoder),
//! cameras, lights (`KHR_lights_punctual`) and every other extension.

use std::fmt;

use vieww_foundation::json::Json;

use crate::Mesh;

/// Why a glTF did not load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GltfError(pub String);

impl fmt::Display for GltfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "glTF: {}", self.0)
    }
}

impl std::error::Error for GltfError {}

fn err<T>(msg: impl Into<String>) -> Result<T, GltfError> {
    Err(GltfError(msg.into()))
}

/// A PBR material's factors.
#[derive(Debug, Clone, PartialEq)]
pub struct GltfMaterial {
    pub name: String,
    /// Linear RGBA.
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: [f32; 3],
    pub double_sided: bool,
    /// `alphaMode: BLEND`.
    pub blend: bool,
    /// The `baseColorTexture.index`, if any (not decoded here).
    pub base_color_texture: Option<usize>,
}

impl Default for GltfMaterial {
    fn default() -> Self {
        Self {
            name: String::new(),
            base_color: [1.0, 1.0, 1.0, 1.0],
            metallic: 1.0,
            roughness: 1.0,
            emissive: [0.0; 3],
            double_sided: false,
            blend: false,
            base_color_texture: None,
        }
    }
}

/// One drawable piece of a mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct Primitive {
    pub mesh: Mesh,
    pub material: Option<usize>,
}

/// A glTF mesh: one or more primitives.
#[derive(Debug, Clone, PartialEq)]
pub struct GltfMesh {
    pub name: String,
    pub primitives: Vec<Primitive>,
}

/// A node of the hierarchy.
#[derive(Debug, Clone, PartialEq)]
pub struct GltfNode {
    pub name: String,
    pub mesh: Option<usize>,
    pub children: Vec<usize>,
    pub translation: [f32; 3],
    /// Quaternion `[x, y, z, w]`.
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    /// Column-major, when the file gave a matrix instead of TRS.
    pub matrix: Option<[f32; 16]>,
}

/// Which property an animation channel drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Path {
    Translation,
    Rotation,
    Scale,
}

/// How a sampler interpolates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    Linear,
    Step,
    CubicSpline,
}

/// One animated property of one node.
#[derive(Debug, Clone, PartialEq)]
pub struct Channel {
    pub node: usize,
    pub path: Path,
    pub interpolation: Interpolation,
    pub times: Vec<f32>,
    /// One value per key (per key *triple* — in-tangent, value,
    /// out-tangent — for cubic splines), padded to four components.
    pub values: Vec<[f32; 4]>,
}

/// A named set of channels.
#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    pub name: String,
    pub channels: Vec<Channel>,
}

impl Animation {
    /// The last key time of any channel.
    #[must_use]
    pub fn duration(&self) -> f32 {
        self.channels
            .iter()
            .filter_map(|c| c.times.last().copied())
            .fold(0.0, f32::max)
    }
}

/// A loaded asset.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Gltf {
    pub meshes: Vec<GltfMesh>,
    pub materials: Vec<GltfMaterial>,
    pub nodes: Vec<GltfNode>,
    /// Root node indices of each scene.
    pub scenes: Vec<Vec<usize>>,
    pub default_scene: Option<usize>,
    pub animations: Vec<Animation>,
}

/// A node's sampled transform override.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct NodePose {
    pub translation: Option<[f32; 3]>,
    pub rotation: Option<[f32; 4]>,
    pub scale: Option<[f32; 3]>,
}

fn nlerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    // Quaternions: shortest path, then normalise (glTF's recommended
    // approximation of slerp for dense keys).
    let dot: f32 = (0..4).map(|i| a[i] * b[i]).sum();
    let s = if dot < 0.0 { -1.0 } else { 1.0 };
    let mut r = [0.0; 4];
    for i in 0..4 {
        r[i] = a[i] + (b[i] * s - a[i]) * t;
    }
    let l = r.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    r.map(|x| x / l)
}

impl Channel {
    /// The channel's value at `t` (clamped to its keys).
    #[must_use]
    pub fn sample(&self, t: f32) -> [f32; 4] {
        let n = self.times.len();
        if n == 0 {
            return [0.0; 4];
        }
        let cubic = self.interpolation == Interpolation::CubicSpline;
        let value = |k: usize| {
            if cubic {
                self.values[k * 3 + 1]
            } else {
                self.values[k]
            }
        };
        if t <= self.times[0] {
            return value(0);
        }
        if t >= self.times[n - 1] {
            return value(n - 1);
        }
        let i = self.times.partition_point(|&x| x <= t) - 1;
        let (t0, t1) = (self.times[i], self.times[i + 1]);
        let dt = t1 - t0;
        let u = if dt > 0.0 { (t - t0) / dt } else { 0.0 };
        match self.interpolation {
            Interpolation::Step => value(i),
            Interpolation::Linear => {
                let (a, b) = (value(i), value(i + 1));
                if self.path == Path::Rotation {
                    nlerp(a, b, u)
                } else {
                    let mut r = [0.0; 4];
                    for k in 0..4 {
                        r[k] = a[k] + (b[k] - a[k]) * u;
                    }
                    r
                }
            }
            Interpolation::CubicSpline => {
                // Hermite with tangents scaled by the key interval.
                let p0 = self.values[i * 3 + 1];
                let m0 = self.values[i * 3 + 2];
                let m1 = self.values[(i + 1) * 3];
                let p1 = self.values[(i + 1) * 3 + 1];
                let (u2, u3) = (u * u, u * u * u);
                let h00 = 2.0 * u3 - 3.0 * u2 + 1.0;
                let h10 = u3 - 2.0 * u2 + u;
                let h01 = -2.0 * u3 + 3.0 * u2;
                let h11 = u3 - u2;
                let mut r = [0.0; 4];
                for k in 0..4 {
                    r[k] = h00 * p0[k] + h10 * dt * m0[k] + h01 * p1[k] + h11 * dt * m1[k];
                }
                if self.path == Path::Rotation {
                    let l = r.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
                    r = r.map(|x| x / l);
                }
                r
            }
        }
    }
}

impl Gltf {
    /// Sample animation `index` at `t` seconds: one override per node.
    #[must_use]
    pub fn sample(&self, index: usize, t: f32) -> Vec<NodePose> {
        let mut out = vec![NodePose::default(); self.nodes.len()];
        let Some(anim) = self.animations.get(index) else {
            return out;
        };
        for c in &anim.channels {
            let Some(pose) = out.get_mut(c.node) else {
                continue;
            };
            let v = c.sample(t);
            match c.path {
                Path::Translation => pose.translation = Some([v[0], v[1], v[2]]),
                Path::Rotation => pose.rotation = Some(v),
                Path::Scale => pose.scale = Some([v[0], v[1], v[2]]),
            }
        }
        out
    }

    /// Root nodes of the default (or first) scene; every parentless node
    /// if the file declares no scenes.
    #[must_use]
    pub fn roots(&self) -> Vec<usize> {
        if let Some(s) = self
            .default_scene
            .and_then(|i| self.scenes.get(i))
            .or_else(|| self.scenes.first())
        {
            return s.clone();
        }
        let mut child = vec![false; self.nodes.len()];
        for n in &self.nodes {
            for &c in &n.children {
                if let Some(slot) = child.get_mut(c) {
                    *slot = true;
                }
            }
        }
        (0..self.nodes.len()).filter(|i| !child[*i]).collect()
    }
}

/// Decode standard base64 (padding optional, whitespace ignored).
///
/// # Errors
///
/// A character outside the alphabet.
pub fn base64_decode(text: &str) -> Result<Vec<u8>, GltfError> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0u32;
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return err(format!("invalid base64 byte 0x{c:02x}")),
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            #[allow(clippy::cast_possible_truncation)]
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

/// Encode as standard padded base64 (for writing `.gltf` files and tests).
#[must_use]
pub fn base64_encode(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Parse a `.gltf` document. `resolve` loads external buffer URIs (return
/// `None` to refuse); `data:` URIs are decoded here.
///
/// # Errors
///
/// Malformed JSON, missing required fields, out-of-range indices or
/// accessor reads past their buffer.
pub fn parse_gltf(
    text: &str,
    resolve: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<Gltf, GltfError> {
    let doc = Json::parse(text).map_err(|e| GltfError(e.to_string()))?;
    load(&doc, None, resolve)
}

/// Parse a `.glb` binary.
///
/// # Errors
///
/// A bad header, chunk layout or any error [`parse_gltf`] can report.
pub fn parse_glb(bytes: &[u8]) -> Result<Gltf, GltfError> {
    let u32_at = |o: usize| -> Result<u32, GltfError> {
        bytes
            .get(o..o + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or_else(|| GltfError("truncated GLB".into()))
    };
    if bytes.get(0..4) != Some(b"glTF") {
        return err("not a GLB (bad magic)");
    }
    if u32_at(4)? != 2 {
        return err("unsupported GLB version");
    }
    let total = u32_at(8)? as usize;
    if total > bytes.len() {
        return err("GLB length exceeds the file");
    }
    let mut at = 12;
    let (mut json, mut bin) = (None, None);
    while at + 8 <= total {
        let len = u32_at(at)? as usize;
        let kind = u32_at(at + 4)?;
        let data = bytes
            .get(at + 8..at + 8 + len)
            .ok_or_else(|| GltfError("chunk past the end".into()))?;
        match kind {
            0x4E4F_534A => json = Some(data),
            0x004E_4942 => bin = Some(data.to_vec()),
            _ => {}
        }
        at += 8 + len.div_ceil(4) * 4;
    }
    let json = json.ok_or_else(|| GltfError("GLB without a JSON chunk".into()))?;
    let text =
        std::str::from_utf8(json).map_err(|_| GltfError("JSON chunk is not UTF-8".into()))?;
    let doc =
        Json::parse(text.trim_end_matches(['\0', ' '])).map_err(|e| GltfError(e.to_string()))?;
    load(&doc, bin, &|_| None)
}

fn arr<'a>(doc: &'a Json, key: &str) -> &'a [Json] {
    doc.get(key).and_then(Json::as_array).unwrap_or(&[])
}

fn idx(j: &Json, key: &str) -> Option<usize> {
    j.get(key).and_then(Json::as_usize)
}

fn floats<const N: usize>(j: &Json, key: &str, default: [f32; N]) -> [f32; N] {
    match j.get(key).and_then(Json::as_f32_vec) {
        Some(v) if v.len() == N => {
            let mut out = default;
            out.copy_from_slice(&v);
            out
        }
        _ => default,
    }
}

struct Buffers {
    buffers: Vec<Vec<u8>>,
    views: Vec<(usize, usize, usize, Option<usize>)>,
}

impl Buffers {
    /// An accessor's elements as `f32`s, `width` per element.
    #[allow(clippy::cast_precision_loss)]
    fn read(&self, doc: &Json, accessor: usize) -> Result<(Vec<f32>, usize), GltfError> {
        let a = arr(doc, "accessors")
            .get(accessor)
            .ok_or_else(|| GltfError(format!("accessor {accessor} out of range")))?;
        let count = idx(a, "count").ok_or_else(|| GltfError("accessor without count".into()))?;
        let width = match a.get("type").and_then(Json::as_str) {
            Some("SCALAR") => 1,
            Some("VEC2") => 2,
            Some("VEC3") => 3,
            Some("VEC4" | "MAT2") => 4,
            Some("MAT3") => 9,
            Some("MAT4") => 16,
            other => return err(format!("accessor type {other:?}")),
        };
        let ctype = idx(a, "componentType").ok_or_else(|| GltfError("componentType".into()))?;
        let size = match ctype {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            other => return err(format!("componentType {other}")),
        };
        let normalized = a.get("normalized").and_then(Json::as_bool).unwrap_or(false);
        let Some(view) = idx(a, "bufferView") else {
            // Sparse-only or zero-filled accessor.
            return Ok((vec![0.0; count * width], width));
        };
        let &(buffer, view_offset, view_len, stride) = self
            .views
            .get(view)
            .ok_or_else(|| GltfError(format!("bufferView {view} out of range")))?;
        let data = self
            .buffers
            .get(buffer)
            .ok_or_else(|| GltfError(format!("buffer {buffer} missing")))?;
        let base = view_offset + idx(a, "byteOffset").unwrap_or(0);
        let stride = stride.unwrap_or(size * width);
        let mut out = Vec::with_capacity(count * width);
        for e in 0..count {
            for c in 0..width {
                let o = base + e * stride + c * size;
                if o + size > view_offset + view_len || o + size > data.len() {
                    return err(format!("accessor {accessor} reads past its bufferView"));
                }
                let b = &data[o..o + size];
                let v = match ctype {
                    5120 => {
                        let x = f32::from(i8::from_le_bytes([b[0]]));
                        if normalized {
                            (x / 127.0).max(-1.0)
                        } else {
                            x
                        }
                    }
                    5121 => {
                        let x = f32::from(b[0]);
                        if normalized {
                            x / 255.0
                        } else {
                            x
                        }
                    }
                    5122 => {
                        let x = f32::from(i16::from_le_bytes([b[0], b[1]]));
                        if normalized {
                            (x / 32767.0).max(-1.0)
                        } else {
                            x
                        }
                    }
                    5123 => {
                        let x = f32::from(u16::from_le_bytes([b[0], b[1]]));
                        if normalized {
                            x / 65535.0
                        } else {
                            x
                        }
                    }
                    5125 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32,
                    _ => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                };
                out.push(v);
            }
        }
        Ok((out, width))
    }
}

#[allow(clippy::too_many_lines)]
fn load(
    doc: &Json,
    glb_bin: Option<Vec<u8>>,
    resolve: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<Gltf, GltfError> {
    if let Some(v) = doc
        .get("asset")
        .and_then(|a| a.get("version"))
        .and_then(Json::as_str)
    {
        if !v.starts_with('2') {
            return err(format!("asset version {v} is not glTF 2"));
        }
    }
    let mut buffers = Vec::new();
    for (i, b) in arr(doc, "buffers").iter().enumerate() {
        let data = match b.get("uri").and_then(Json::as_str) {
            None => glb_bin.clone().ok_or_else(|| {
                GltfError(format!("buffer {i} has no uri and there is no GLB chunk"))
            })?,
            Some(uri) if uri.starts_with("data:") => {
                let comma = uri
                    .find(',')
                    .ok_or_else(|| GltfError("malformed data URI".into()))?;
                if !uri[..comma].ends_with(";base64") {
                    return err("only base64 data URIs are supported");
                }
                base64_decode(&uri[comma + 1..])?
            }
            Some(uri) => resolve(uri)
                .ok_or_else(|| GltfError(format!("buffer {uri} could not be loaded")))?,
        };
        buffers.push(data);
    }
    let views = arr(doc, "bufferViews")
        .iter()
        .map(|v| {
            Ok((
                idx(v, "buffer").ok_or_else(|| GltfError("bufferView.buffer".into()))?,
                idx(v, "byteOffset").unwrap_or(0),
                idx(v, "byteLength").ok_or_else(|| GltfError("bufferView.byteLength".into()))?,
                idx(v, "byteStride"),
            ))
        })
        .collect::<Result<Vec<_>, GltfError>>()?;
    let bufs = Buffers { buffers, views };

    let materials = arr(doc, "materials")
        .iter()
        .map(|m| {
            let pbr = m.get("pbrMetallicRoughness").cloned().unwrap_or_default();
            GltfMaterial {
                name: m
                    .get("name")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_owned(),
                base_color: floats(&pbr, "baseColorFactor", [1.0; 4]),
                metallic: pbr
                    .get("metallicFactor")
                    .and_then(Json::as_f32)
                    .unwrap_or(1.0),
                roughness: pbr
                    .get("roughnessFactor")
                    .and_then(Json::as_f32)
                    .unwrap_or(1.0),
                emissive: floats(m, "emissiveFactor", [0.0; 3]),
                double_sided: m
                    .get("doubleSided")
                    .and_then(Json::as_bool)
                    .unwrap_or(false),
                blend: m.get("alphaMode").and_then(Json::as_str) == Some("BLEND"),
                base_color_texture: pbr.get("baseColorTexture").and_then(|t| idx(t, "index")),
            }
        })
        .collect();

    let mut meshes = Vec::new();
    for m in arr(doc, "meshes") {
        let mut primitives = Vec::new();
        for p in arr(m, "primitives") {
            let attrs = p
                .get("attributes")
                .ok_or_else(|| GltfError("primitive without attributes".into()))?;
            let pos_acc = idx(attrs, "POSITION")
                .ok_or_else(|| GltfError("primitive without POSITION".into()))?;
            let (pos, _) = bufs.read(doc, pos_acc)?;
            let mut mesh = Mesh {
                positions: pos
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|c| [c[0], c[1], c[2]])
                    .collect(),
                ..Mesh::default()
            };
            if let Some(a) = idx(attrs, "NORMAL") {
                mesh.normals = bufs
                    .read(doc, a)?
                    .0
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|c| [c[0], c[1], c[2]])
                    .collect();
            }
            if let Some(a) = idx(attrs, "TEXCOORD_0") {
                mesh.uvs = bufs
                    .read(doc, a)?
                    .0
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| [c[0], c[1]])
                    .collect();
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let raw: Vec<u32> = match idx(p, "indices") {
                Some(a) => bufs.read(doc, a)?.0.into_iter().map(|v| v as u32).collect(),
                None => (0..mesh.positions.len() as u32).collect(),
            };
            let nverts = mesh.positions.len();
            if raw.iter().any(|&i| i as usize >= nverts) {
                return err("an index points past the vertex list");
            }
            mesh.indices = match idx(p, "mode").unwrap_or(4) {
                4 => raw,
                5 => (2..raw.len())
                    .flat_map(|i| {
                        if i % 2 == 0 {
                            [raw[i - 2], raw[i - 1], raw[i]]
                        } else {
                            [raw[i - 1], raw[i - 2], raw[i]]
                        }
                    })
                    .collect(),
                6 => (2..raw.len())
                    .flat_map(|i| [raw[0], raw[i - 1], raw[i]])
                    .collect(),
                m => {
                    return err(format!(
                        "primitive mode {m} (points/lines) is not triangles"
                    ))
                }
            };
            if !mesh.has_normals() {
                mesh.compute_normals();
            }
            primitives.push(Primitive {
                mesh,
                material: idx(p, "material"),
            });
        }
        meshes.push(GltfMesh {
            name: m
                .get("name")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_owned(),
            primitives,
        });
    }

    let nodes = arr(doc, "nodes")
        .iter()
        .map(|n| {
            let matrix = n.get("matrix").and_then(Json::as_f32_vec).and_then(|v| {
                (v.len() == 16).then(|| {
                    let mut m = [0.0; 16];
                    m.copy_from_slice(&v);
                    m
                })
            });
            GltfNode {
                name: n
                    .get("name")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_owned(),
                mesh: idx(n, "mesh"),
                children: arr(n, "children")
                    .iter()
                    .filter_map(Json::as_usize)
                    .collect(),
                translation: floats(n, "translation", [0.0; 3]),
                rotation: floats(n, "rotation", [0.0, 0.0, 0.0, 1.0]),
                scale: floats(n, "scale", [1.0; 3]),
                matrix,
            }
        })
        .collect::<Vec<_>>();
    for n in &nodes {
        if n.mesh.is_some_and(|m| m >= meshes.len()) || n.children.iter().any(|&c| c >= nodes.len())
        {
            return err(format!("node {} refers past the end of a list", n.name));
        }
    }

    let scenes = arr(doc, "scenes")
        .iter()
        .map(|s| arr(s, "nodes").iter().filter_map(Json::as_usize).collect())
        .collect();

    let mut animations = Vec::new();
    for a in arr(doc, "animations") {
        let samplers = arr(a, "samplers");
        let mut channels = Vec::new();
        for c in arr(a, "channels") {
            let target = c
                .get("target")
                .ok_or_else(|| GltfError("channel without target".into()))?;
            let path = match target.get("path").and_then(Json::as_str) {
                Some("translation") => Path::Translation,
                Some("rotation") => Path::Rotation,
                Some("scale") => Path::Scale,
                _ => continue, // morph-target weights: not supported
            };
            let Some(node) = idx(target, "node") else {
                continue;
            };
            let s = samplers
                .get(idx(c, "sampler").ok_or_else(|| GltfError("channel.sampler".into()))?)
                .ok_or_else(|| GltfError("sampler out of range".into()))?;
            let interpolation = match s.get("interpolation").and_then(Json::as_str) {
                Some("STEP") => Interpolation::Step,
                Some("CUBICSPLINE") => Interpolation::CubicSpline,
                _ => Interpolation::Linear,
            };
            let (times, _) = bufs.read(
                doc,
                idx(s, "input").ok_or_else(|| GltfError("sampler.input".into()))?,
            )?;
            let (vals, width) = bufs.read(
                doc,
                idx(s, "output").ok_or_else(|| GltfError("sampler.output".into()))?,
            )?;
            let values = vals
                .chunks_exact(width)
                .map(|c| {
                    let mut v = [0.0; 4];
                    v[..width.min(4)].copy_from_slice(&c[..width.min(4)]);
                    v
                })
                .collect::<Vec<_>>();
            let expect = times.len()
                * if interpolation == Interpolation::CubicSpline {
                    3
                } else {
                    1
                };
            if values.len() != expect {
                return err("animation sampler input/output lengths disagree");
            }
            channels.push(Channel {
                node,
                path,
                interpolation,
                times,
                values,
            });
        }
        animations.push(Animation {
            name: a
                .get("name")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_owned(),
            channels,
        });
    }

    Ok(Gltf {
        meshes,
        materials,
        nodes,
        scenes,
        default_scene: idx(doc, "scene"),
        animations,
    })
}

/// Write a minimal `.gltf` (embedded base64 buffer) for one mesh, with an
/// optional base colour — enough to round-trip geometry and to author
/// example assets in code.
#[must_use]
pub fn write_gltf(mesh: &Mesh, name: &str, base_color: [f32; 4]) -> String {
    let mut bin: Vec<u8> = Vec::new();
    let push_f = |bin: &mut Vec<u8>, v: f32| bin.extend_from_slice(&v.to_le_bytes());
    for p in &mesh.positions {
        for &c in p {
            push_f(&mut bin, c);
        }
    }
    let pos_len = bin.len();
    for n in &mesh.normals {
        for &c in n {
            push_f(&mut bin, c);
        }
    }
    let nrm_len = bin.len() - pos_len;
    let idx_off = bin.len();
    for &i in &mesh.indices {
        bin.extend_from_slice(&i.to_le_bytes());
    }
    let (lo, hi) = mesh.bounds().unwrap_or(([0.0; 3], [0.0; 3]));
    let has_n = mesh.has_normals();
    let mut attrs = vec![("POSITION", Json::from(0usize))];
    if has_n {
        attrs.push(("NORMAL", Json::from(1usize)));
    }
    let mut views = vec![Json::object([
        ("buffer", Json::from(0usize)),
        ("byteOffset", Json::from(0usize)),
        ("byteLength", Json::from(pos_len)),
    ])];
    let mut accessors = vec![Json::object([
        ("bufferView", Json::from(0usize)),
        ("componentType", Json::from(5126usize)),
        ("count", Json::from(mesh.positions.len())),
        ("type", Json::from("VEC3")),
        ("min", Json::numbers(lo.iter().map(|v| f64::from(*v)))),
        ("max", Json::numbers(hi.iter().map(|v| f64::from(*v)))),
    ])];
    if has_n {
        views.push(Json::object([
            ("buffer", Json::from(0usize)),
            ("byteOffset", Json::from(pos_len)),
            ("byteLength", Json::from(nrm_len)),
        ]));
        accessors.push(Json::object([
            ("bufferView", Json::from(1usize)),
            ("componentType", Json::from(5126usize)),
            ("count", Json::from(mesh.normals.len())),
            ("type", Json::from("VEC3")),
        ]));
    }
    let iv = views.len();
    views.push(Json::object([
        ("buffer", Json::from(0usize)),
        ("byteOffset", Json::from(idx_off)),
        ("byteLength", Json::from(bin.len() - idx_off)),
    ]));
    let ia = accessors.len();
    accessors.push(Json::object([
        ("bufferView", Json::from(iv)),
        ("componentType", Json::from(5125usize)),
        ("count", Json::from(mesh.indices.len())),
        ("type", Json::from("SCALAR")),
    ]));
    let doc = Json::object([
        (
            "asset",
            Json::object([
                ("version", Json::from("2.0")),
                ("generator", Json::from("vieww-mesh")),
            ]),
        ),
        ("scene", Json::from(0usize)),
        (
            "scenes",
            Json::Array(vec![Json::object([(
                "nodes",
                Json::Array(vec![Json::from(0usize)]),
            )])]),
        ),
        (
            "nodes",
            Json::Array(vec![Json::object([
                ("name", Json::from(name)),
                ("mesh", Json::from(0usize)),
            ])]),
        ),
        (
            "meshes",
            Json::Array(vec![Json::object([
                ("name", Json::from(name)),
                (
                    "primitives",
                    Json::Array(vec![Json::object([
                        ("attributes", Json::object(attrs)),
                        ("indices", Json::from(ia)),
                        ("material", Json::from(0usize)),
                    ])]),
                ),
            ])]),
        ),
        (
            "materials",
            Json::Array(vec![Json::object([(
                "pbrMetallicRoughness",
                Json::object([
                    (
                        "baseColorFactor",
                        Json::numbers(base_color.iter().map(|v| f64::from(*v))),
                    ),
                    ("metallicFactor", Json::from(0.0f32)),
                    ("roughnessFactor", Json::from(0.6f32)),
                ]),
            )])]),
        ),
        (
            "buffers",
            Json::Array(vec![Json::object([
                ("byteLength", Json::from(bin.len())),
                (
                    "uri",
                    Json::from(format!(
                        "data:application/octet-stream;base64,{}",
                        base64_encode(&bin)
                    )),
                ),
            ])]),
        ),
        ("bufferViews", Json::Array(views)),
        ("accessors", Json::Array(accessors)),
    ]);
    doc.pretty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle_gltf(mode: Option<u32>, extra_anim: bool) -> String {
        // Three positions, u16 indices, one sampler.
        let mut bin = Vec::new();
        for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend_from_slice(&v.to_le_bytes());
        }
        for i in [0u16, 1, 2, 0] {
            bin.extend_from_slice(&i.to_le_bytes()); // padded to 8 bytes
        }
        // Animation: times [0, 1], translations [(0,0,0), (2,0,0)].
        let anim_off = bin.len();
        for v in [0.0f32, 1.0] {
            bin.extend_from_slice(&v.to_le_bytes());
        }
        for v in [0.0f32, 0.0, 0.0, 2.0, 0.0, 0.0] {
            bin.extend_from_slice(&v.to_le_bytes());
        }
        let mode = mode.map(|m| format!(", \"mode\": {m}")).unwrap_or_default();
        let anim = if extra_anim {
            format!(
                r#", "animations": [{{"name": "slide", "samplers": [{{"input": 2, "output": 3, "interpolation": "LINEAR"}}],
                   "channels": [{{"sampler": 0, "target": {{"node": 1, "path": "translation"}}}}]}}],
                 "_": {anim_off}"#
            )
        } else {
            String::new()
        };
        format!(
            r#"{{
  "asset": {{"version": "2.0"}},
  "scene": 0,
  "scenes": [{{"nodes": [0]}}],
  "nodes": [{{"name": "root", "children": [1], "translation": [0, 0, -5]}}, {{"name": "tri", "mesh": 0, "scale": [2, 2, 2]}}],
  "meshes": [{{"name": "t", "primitives": [{{"attributes": {{"POSITION": 0}}, "indices": 1, "material": 0{mode}}}]}}],
  "materials": [{{"name": "red", "pbrMetallicRoughness": {{"baseColorFactor": [1, 0, 0, 1], "metallicFactor": 0.2}}, "doubleSided": true}}],
  "buffers": [{{"byteLength": {len}, "uri": "data:application/octet-stream;base64,{b64}"}}],
  "bufferViews": [
    {{"buffer": 0, "byteOffset": 0, "byteLength": 36}},
    {{"buffer": 0, "byteOffset": 36, "byteLength": 6}},
    {{"buffer": 0, "byteOffset": {anim_off}, "byteLength": 8}},
    {{"buffer": 0, "byteOffset": {v3}, "byteLength": 24}}
  ],
  "accessors": [
    {{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"}},
    {{"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"}},
    {{"bufferView": 2, "componentType": 5126, "count": 2, "type": "SCALAR"}},
    {{"bufferView": 3, "componentType": 5126, "count": 2, "type": "VEC3"}}
  ]{anim}
}}"#,
            len = bin.len(),
            b64 = base64_encode(&bin),
            v3 = anim_off + 8,
        )
    }

    #[test]
    fn base64_round_trips() {
        for s in [
            &b""[..],
            b"f",
            b"fo",
            b"foo",
            b"foob",
            b"fooba",
            b"foobar",
            &[0, 255, 7, 128],
        ] {
            assert_eq!(base64_decode(&base64_encode(s)).unwrap(), s);
        }
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert!(base64_decode("a$b").is_err());
    }

    #[test]
    fn a_triangle_loads_with_hierarchy_and_material() {
        let g = parse_gltf(&triangle_gltf(None, false), &|_| None).unwrap();
        assert_eq!(g.meshes[0].primitives[0].mesh.triangles(), 1);
        assert!(
            g.meshes[0].primitives[0].mesh.has_normals(),
            "normals computed when absent"
        );
        assert_eq!(g.nodes[0].children, [1]);
        assert_eq!(g.nodes[0].translation, [0.0, 0.0, -5.0]);
        assert_eq!(g.nodes[1].scale, [2.0, 2.0, 2.0]);
        assert_eq!(g.materials[0].base_color, [1.0, 0.0, 0.0, 1.0]);
        assert!((g.materials[0].metallic - 0.2).abs() < 1e-6);
        assert!(g.materials[0].double_sided);
        assert_eq!(g.roots(), [0]);
    }

    #[test]
    fn animations_sample_linearly_and_clamp() {
        let g = parse_gltf(&triangle_gltf(None, true), &|_| None).unwrap();
        assert_eq!(g.animations[0].name, "slide");
        assert!((g.animations[0].duration() - 1.0).abs() < 1e-6);
        let p = g.sample(0, 0.25);
        assert_eq!(p[1].translation, Some([0.5, 0.0, 0.0]));
        assert_eq!(g.sample(0, 9.0)[1].translation, Some([2.0, 0.0, 0.0]));
        assert!(p[0].translation.is_none());
    }

    #[test]
    fn rotation_channels_interpolate_on_the_sphere() {
        let c = Channel {
            node: 0,
            path: Path::Rotation,
            interpolation: Interpolation::Linear,
            times: vec![0.0, 1.0],
            values: vec![[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 0.0]],
        };
        let q = c.sample(0.5);
        let len = q.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((len - 1.0).abs() < 1e-5);
        assert!((q[2] - q[3]).abs() < 1e-5, "halfway: 90° about z");
        let step = Channel {
            interpolation: Interpolation::Step,
            ..c
        };
        assert_eq!(step.sample(0.9), [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn strips_and_fans_triangulate() {
        let g = parse_gltf(&triangle_gltf(Some(5), false), &|_| None).unwrap();
        assert_eq!(g.meshes[0].primitives[0].mesh.triangles(), 1);
        assert!(
            parse_gltf(&triangle_gltf(Some(1), false), &|_| None).is_err(),
            "lines are refused"
        );
    }

    #[test]
    fn glb_wraps_the_same_document() {
        let text = triangle_gltf(None, false);
        let doc = Json::parse(&text).unwrap();
        // Move the embedded buffer into the BIN chunk.
        let b64 = doc
            .get("buffers")
            .and_then(|b| b.index(0))
            .and_then(|b| b.get("uri"))
            .and_then(Json::as_str)
            .unwrap();
        let bin = base64_decode(&b64[b64.find(',').unwrap() + 1..]).unwrap();
        let json = text.replace(&format!(", \"uri\": \"{b64}\""), "");
        let mut json_bytes = json.into_bytes();
        while json_bytes.len() % 4 != 0 {
            json_bytes.push(b' ');
        }
        let mut bin_padded = bin.clone();
        while !bin_padded.len().is_multiple_of(4) {
            bin_padded.push(0);
        }
        let total = 12 + 8 + json_bytes.len() + 8 + bin_padded.len();
        let mut glb = b"glTF".to_vec();
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&u32::try_from(total).unwrap().to_le_bytes());
        glb.extend_from_slice(&u32::try_from(json_bytes.len()).unwrap().to_le_bytes());
        glb.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
        glb.extend_from_slice(&json_bytes);
        glb.extend_from_slice(&u32::try_from(bin_padded.len()).unwrap().to_le_bytes());
        glb.extend_from_slice(&0x004E_4942u32.to_le_bytes());
        glb.extend_from_slice(&bin_padded);
        let g = parse_glb(&glb).unwrap();
        assert_eq!(g.meshes[0].primitives[0].mesh.positions[1], [1.0, 0.0, 0.0]);
        assert!(parse_glb(b"nope").is_err());
    }

    #[test]
    fn external_buffers_go_through_the_resolver() {
        let text = triangle_gltf(None, false);
        let doc = Json::parse(&text).unwrap();
        let b64 = doc
            .get("buffers")
            .and_then(|b| b.index(0))
            .and_then(|b| b.get("uri"))
            .and_then(Json::as_str)
            .unwrap()
            .to_owned();
        let bin = base64_decode(&b64[b64.find(',').unwrap() + 1..]).unwrap();
        let ext = text.replace(&b64, "tri.bin");
        assert!(parse_gltf(&ext, &|_| None).is_err());
        let g = parse_gltf(&ext, &|uri| (uri == "tri.bin").then(|| bin.clone())).unwrap();
        assert_eq!(g.meshes.len(), 1);
    }

    #[test]
    fn corrupt_files_are_errors() {
        let text = triangle_gltf(None, false).replace(
            "\"count\": 3, \"type\": \"VEC3\"",
            "\"count\": 30, \"type\": \"VEC3\"",
        );
        assert!(parse_gltf(&text, &|_| None).is_err(), "reads past the view");
        assert!(parse_gltf("{\"asset\": {\"version\": \"1.0\"}}", &|_| None).is_err());
        assert!(parse_gltf("not json", &|_| None).is_err());
    }

    #[test]
    fn written_gltf_parses_back() {
        let mut m = Mesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
            ],
            indices: vec![0, 1, 2, 2, 1, 3],
            ..Mesh::default()
        };
        m.compute_normals();
        let text = write_gltf(&m, "quad", [0.2, 0.4, 0.6, 1.0]);
        let g = parse_gltf(&text, &|_| None).unwrap();
        assert_eq!(g.meshes[0].primitives[0].mesh, m);
        assert_eq!(g.materials[0].base_color, [0.2, 0.4, 0.6, 1.0]);
    }
}
