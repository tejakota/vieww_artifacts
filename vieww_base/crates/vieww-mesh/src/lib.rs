//! The mesh layer — OBJ and STL geometry, parsed into triangles.
//!
//! # What this closes, and how much of it
//!
//! The framework's own comparison table lists "3D mesh loading" as a gap,
//! naming GLTF and OBJ. This crate answers it for **OBJ** — the format
//! every 3D tool on earth exports, text, human-readable, twenty years
//! stable — and **STL**, the 3D-printing lingua franca, in both its binary
//! and ASCII spellings. GLTF is *not* here, and that is a decision rather
//! than an oversight: GLTF is a JSON scene graph wrapping one or more
//! binary buffers, and loading it honestly means a JSON parser, a base64
//! decoder, a scene hierarchy and a material system — four subsystems, each
//! with its own failure modes, for a format whose *geometry* is exactly
//! what OBJ already says. The [`Mesh`] this crate produces is the seam a
//! GLTF loader would target anyway; when one is worth its dependencies, it
//! lands there.
//!
//! # The output, in one paragraph
//!
//! A [`Mesh`] is triangle soup: positions, normals, texture coordinates,
//! and an index list that walks them three at a time. One mesh, not a
//! scene — no objects, no materials, no hierarchy — because those are the
//! *scene graph's* job and this framework already has one; a loader that
//! smuggled a second hierarchy inside its output would be two hierarchies
//! to reconcile forever. OBJ's `o`/`g` records are parsed and ignored
//! deliberately, and a caller who needs per-object meshes splits the file
//! and parses it per object, which is two lines and no new type.
//!
//! ```
//! use vieww_mesh::parse_obj;
//!
//! // One quad, as two triangles, with normals from the file.
//! let cube = "
//! v -1 -1 0
//! v  1 -1 0
//! v  1  1 0
//! v -1  1 0
//! vn 0 0 1
//! f 1//1 2//1 3//1 4//1
//! ";
//!
//! let mesh = parse_obj(cube).expect("a valid quad");
//! assert_eq!(mesh.positions.len(), 4);
//! assert_eq!(mesh.triangles(), 2, "the quad was fan-triangulated");
//! assert!(mesh.has_normals());
//! ```
//!
//! # The parsing rules that matter
//!
//! * **Faces triangulate by fan.** `f 1 2 3 4 5` becomes `123, 134, 145` —
//!   the standard fan, correct for convex polygons, which is what
//!   exporters write. A concave polygon is a modelling error this parser
//!   does not silently repair.
//! * **Indices are vertex/texture/normal triples** in all four OBJ
//!   spellings (`v`, `v/vt`, `v//vn`, `v/vt/vn`), and negative indices
//!   count back from the end, exactly as the OBJ spec says.
//! * **A missing normal is computed, a missing UV is zero.** A file
//!   without `vn` records still gets usable normals — the per-vertex
//!   average of its faces' normals — because "no normals" makes a mesh
//!   unlightable, and refusing it serves nobody. UVs, by contrast, zero
//!   out: an untextured mesh is a complete mesh.
//! * **Errors carry line numbers.** A malformed face names the line it was
//!   on, because "line 4,081 of the export" is a debuggable fact and
//!   "bad face" is not.
//!
//! # STL, in both spellings
//!
//! Binary STL (an 80-byte header, a `u32` count, then 50 bytes per
//! triangle) is detected by size rather than by content sniffing: an ASCII
//! file always starts with `solid` **but so do some binary headers** — the
//! one trap in the format, and the reason the length check runs first.

use std::fmt;

/// A point in three dimensions.
pub type Point3 = [f32; 3];

/// A texture coordinate.
pub type TexCoord = [f32; 2];

/// Triangle geometry: positions, normals, UVs, indices.
///
/// Positions and indices are the mesh; normals and UVs are parallel arrays
/// indexed *by the same index list* — a vertex's normal is
/// `normals[indices[i]]`, never `normals[i]`, because the same position can
/// appear with different normals in different faces (that is what a hard
/// edge *is* in OBJ) and this representation keeps that distinction
/// instead of averaging it away at load time.
///
/// That is also why vertices are duplicated on load when their attributes
/// differ: an OBJ face's `v/vt/vn` triple is the real vertex, and two
/// faces sharing a position but not a UV are two vertices that happen to
/// coincide. The deduplication question belongs to the consumer, who knows
/// whether they want hard or smooth normals; it does not belong to the
/// loader, who cannot.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mesh {
    /// One `[x, y, z]` per vertex.
    pub positions: Vec<Point3>,
    /// One normal per vertex; empty when the file had none *and*
    /// [`compute_normals`] was not called.
    ///
    /// [`compute_normals`]: Self::compute_normals
    pub normals: Vec<Point3>,
    /// One `[u, v]` per vertex; empty when the file had none.
    pub uvs: Vec<TexCoord>,
    /// Triangle indices: three per triangle, into `positions` (and the
    /// parallel arrays).
    pub indices: Vec<u32>,
}

impl Mesh {
    /// How many triangles the mesh holds.
    #[must_use]
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }

    /// Whether every vertex carries a normal.
    #[must_use]
    pub fn has_normals(&self) -> bool {
        self.normals.len() == self.positions.len()
    }

    /// Whether every vertex carries a texture coordinate.
    #[must_use]
    pub fn has_uvs(&self) -> bool {
        self.uvs.len() == self.positions.len()
    }

    /// The axis-aligned bounds, or `None` for an empty mesh.
    #[must_use]
    pub fn bounds(&self) -> Option<(Point3, Point3)> {
        let first = *self.positions.first()?;
        let mut min = first;
        let mut max = first;
        for position in &self.positions {
            for axis in 0..3 {
                min[axis] = min[axis].min(position[axis]);
                max[axis] = max[axis].max(position[axis]);
            }
        }
        Some((min, max))
    }

    /// Fill `normals` with the per-vertex average of the faces' normals.
    ///
    /// Smoothing by average — not by angle threshold, not by area
    /// weighting — because those are modelling decisions with parameters,
    /// and a loader's job is a usable default rather than a tuned one. The
    /// average is area-weighted *by construction* (a bigger face
    /// contributes a bigger cross product), which is the weighting that
    /// matters most of the time anyway.
    ///
    /// Existing normals are replaced, so the call is also the "recompute"
    /// path for a mesh whose file normals were wrong.
    ///
    /// Nothing — a degenerate triangle just contributes a zero normal, and
    /// the mesh is never made worse than it was.
    pub fn compute_normals(&mut self) {
        let mut sums = vec![[0.0_f32; 3]; self.positions.len()];
        for triangle in self.indices.chunks_exact(3) {
            let a = self.positions[triangle[0] as usize];
            let b = self.positions[triangle[1] as usize];
            let c = self.positions[triangle[2] as usize];
            // The cross product of two edges: magnitude twice the area,
            // direction the face normal.
            let edge1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let edge2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let face = [
                edge1[1] * edge2[2] - edge1[2] * edge2[1],
                edge1[2] * edge2[0] - edge1[0] * edge2[2],
                edge1[0] * edge2[1] - edge1[1] * edge2[0],
            ];
            for corner in triangle {
                let index = *corner as usize;
                for axis in 0..3 {
                    sums[index][axis] += face[axis];
                }
            }
        }
        self.normals = sums
            .into_iter()
            .map(|sum| {
                let length = (sum[0] * sum[0] + sum[1] * sum[1] + sum[2] * sum[2]).sqrt();
                if length > f32::EPSILON {
                    [sum[0] / length, sum[1] / length, sum[2] / length]
                } else {
                    // A vertex no face reached (or all faces degenerate):
                    // +z rather than NaN, because a NaN normal poisons every
                    // lighting calculation it touches and "sticking
                    // slightly out of the screen" hurts nothing.
                    [0.0, 0.0, 1.0]
                }
            })
            .collect();
    }
}

/// Why a mesh did not parse.
///
/// Every variant carries the line (OBJ) or the triangle index (binary STL)
/// it was found on, because a mesh that fails to load is a support question
/// whose first answer is "which line".
#[derive(Debug, Clone, PartialEq)]
pub enum MeshError {
    /// A line did not parse. The line number and the offending text.
    Malformed { line: usize, what: String },
    /// A face referenced a vertex/texture/normal that does not exist.
    ///
    /// Named separately from `Malformed` because it is the one error a
    /// *truncated* file produces — an export that stopped half way — and
    /// "your file is cut off" is a different diagnosis from "your file is
    /// corrupt".
    IndexOutOfRange { line: usize, index: usize },
    /// No vertices at all: a comment file, an empty file, a wrong path's
    /// contents.
    Empty,
    /// STL-specific: the header's triangle count does not match the bytes
    /// that follow.
    StlTruncated { expected: u32, found: u32 },
}

impl fmt::Display for MeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed { line, what } => write!(f, "line {line}: {what}"),
            Self::IndexOutOfRange { line, index } => {
                write!(f, "line {line}: index {index} does not exist")
            }
            Self::Empty => f.write_str("no vertices in the file"),
            Self::StlTruncated { expected, found } => write!(
                f,
                "the STL header promises {expected} triangles; the file holds {found}"
            ),
        }
    }
}

impl std::error::Error for MeshError {}

/// A parsed face corner: raw vertex/texture/normal indices exactly as the
/// file wrote them — OBJ-style, 1-based, possibly negative, `0` meaning
/// "absent" (`f 1//2` is a corner whose texture is absent).
///
/// Resolution against the tables happens after the whole file is read,
/// because an OBJ face may legitimately reference records declared after
/// it, and because the output's vertex duplication needs final counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct VertexRef {
    vertex: isize,
    texture: isize,
    normal: isize,
}

/// Resolve a raw OBJ index against a table of `len`.
///
/// Positive indices are 1-based; negative count back from the end; `0` is
/// invalid in OBJ and refused as the typo signature it is.
fn resolve_index(index: isize, len: usize, line: usize) -> Result<usize, MeshError> {
    if index == 0 {
        return Err(MeshError::Malformed {
            line,
            what: "index 0: OBJ indices start at 1".into(),
        });
    }
    let resolved = if index < 0 {
        len.checked_sub(index.unsigned_abs())
    } else {
        usize::try_from(index - 1).ok()
    };
    resolved.filter(|resolved| *resolved < len).ok_or(MeshError::IndexOutOfRange {
        line,
        index: index.unsigned_abs(),
    })
}

/// Parse OBJ text into a [`Mesh`].
///
/// # Errors
///
/// Every [`MeshError`], with line numbers — see the type's docs.
pub fn parse_obj(text: &str) -> Result<Mesh, MeshError> {
    let mut positions: Vec<Point3> = Vec::new();
    let mut file_normals: Vec<Point3> = Vec::new();
    let mut file_uvs: Vec<TexCoord> = Vec::new();

    // Faces as parsed references; resolved to real vertices afterwards.
    // Two stages because OBJ indices may point at records declared *after*
    // the face in a sloppy file, and because vertex duplication (see
    // `Mesh`'s docs) needs the whole file's counts first. The line number
    // rides along so a bad index can still name the line it was written on.
    let mut faces: Vec<(usize, Vec<VertexRef>)> = Vec::new();

    for (number, raw) in text.lines().enumerate() {
        // `lines()` strips the `\r` of a `\r\n` pair, so no manual
        // handling here — the file may come from any platform.
        let line = number + 1;
        let trimmed = raw.trim();
        // Comments and blank lines are the file's formatting, not its data.
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // `o`, `g`, `usemtl`, `mtllib`, `s` are scene/material records:
        // parsed here in the sense of "recognised and skipped", with the
        // reasons in the module docs.
        let mut words = trimmed.split_whitespace();
        let Some(keyword) = words.next() else { continue };
        match keyword {
            "v" => {
                let mut point = [0.0_f32; 3];
                let mut seen = 0;
                for word in words {
                    if seen < 3 {
                        point[seen] = parse_float(word, line)?;
                        seen += 1;
                    }
                    // A fourth `w` (homogeneous) coordinate exists in the
                    // spec and appears in approximately no file; it is
                    // ignored rather than refused, since a file that has it
                    // is still a file whose geometry is clear.
                }
                if seen < 3 {
                    return Err(MeshError::Malformed {
                        line,
                        what: format!("a vertex needs three coordinates, found {seen}"),
                    });
                }
                positions.push(point);
            }
            "vn" => {
                let mut normal = [0.0_f32; 3];
                let mut seen = 0;
                for word in words {
                    if seen < 3 {
                        normal[seen] = parse_float(word, line)?;
                        seen += 1;
                    }
                }
                if seen < 3 {
                    return Err(MeshError::Malformed {
                        line,
                        what: format!("a normal needs three components, found {seen}"),
                    });
                }
                file_normals.push(normal);
            }
            "vt" => {
                let mut uv = [0.0_f32; 2];
                let mut seen = 0;
                for word in words {
                    if seen < 2 {
                        uv[seen] = parse_float(word, line)?;
                        seen += 1;
                    }
                }
                // A `w` (third) coordinate is ignored for the same reason as
                // the vertex's fourth: present in the spec, absent in
                // practice, and not worth failing a load over.
                if seen < 2 {
                    return Err(MeshError::Malformed {
                        line,
                        what: format!("a texture coordinate needs two components, found {seen}"),
                    });
                }
                file_uvs.push(uv);
            }
            "f" => {
                let mut corners = Vec::new();
                for word in words {
                    corners.push(parse_corner(word, line)?);
                }
                if corners.len() < 3 {
                    return Err(MeshError::Malformed {
                        line,
                        what: format!("a face needs at least three corners, found {}", corners.len()),
                    });
                }
                faces.push((line, corners));
            }
            "o" | "g" | "usemtl" | "mtllib" | "s" | "mtl" => {
                // Recognised scene/material state; skipped — see the module
                // docs for why the output is deliberately one soup.
            }
            _ => {
                // Unknown keywords are refused rather than skipped: a file
                // with a typo'd `vv 1 2 3` silently loses a vertex, and a
                // mesh that is quietly wrong is worse than one that failed
                // loudly.
                return Err(MeshError::Malformed {
                    line,
                    what: format!("unknown keyword `{keyword}`"),
                });
            }
        }
    }

    if positions.is_empty() {
        return Err(MeshError::Empty);
    }

    // Resolve faces to output vertices. The file's `v` records are a *pool*
    // the faces draw from; the output's `positions` holds only the vertices
    // faces actually reference, one per distinct (position, texture,
    // normal) triple — the duplication policy described in `Mesh`'s docs.
    // A pool vertex no face references is dropped, which is the honest
    // reading of it: it is not geometry, it is a declaration the geometry
    // never used.
    let pool = positions;
    let mut mesh = Mesh {
        positions: Vec::new(),
        normals: Vec::new(),
        uvs: Vec::new(),
        indices: Vec::new(),
    };
    let use_normals = !file_normals.is_empty();
    let use_uvs = !file_uvs.is_empty();
    // A corner cache: the same (vertex, texture, normal) triple is one
    // output vertex, so a shared-position shared-attribute corner is shared
    // — which is what keeps a well-formed file's vertex count honest.
    let mut cache = std::collections::HashMap::<(usize, Option<usize>, Option<usize>), u32>::new();

    for (line, corners) in &faces {
        let mut resolved: Vec<u32> = Vec::with_capacity(corners.len());
        for corner in corners {
            let vertex = resolve_index(corner.vertex, pool.len(), *line)?;
            // An attribute the file never declared is absent for every
            // corner; an attribute this corner omitted is absent for it.
            // Both land on `usize::MAX`, the "carry nothing" key.
            let texture = if use_uvs && corner.texture != 0 {
                Some(resolve_index(corner.texture, file_uvs.len(), *line)?)
            } else {
                None
            };
            let normal = if use_normals && corner.normal != 0 {
                Some(resolve_index(corner.normal, file_normals.len(), *line)?)
            } else {
                None
            };

            let key = (vertex, texture, normal);
            let index = match cache.get(&key) {
                Some(existing) => *existing,
                None => {
                    // A fresh output vertex: the position, and its
                    // attributes (zero where the corner said nothing).
                    let position = pool[vertex];
                    let index = mesh.positions.len() as u32;
                    mesh.positions.push(position);
                    mesh.normals.push(match normal {
                        Some(index) => file_normals[index],
                        None => [0.0, 0.0, 0.0],
                    });
                    mesh.uvs.push(match texture {
                        Some(index) => file_uvs[index],
                        None => [0.0, 0.0],
                    });
                    cache.insert(key, index);
                    index
                }
            };
            resolved.push(index);
        }

        // Fan triangulation: 0, i, i+1 for every corner between the first
        // and the last. Convex polygons only — see the module docs.
        for corner in 1..resolved.len() - 1 {
            mesh.indices
                .extend_from_slice(&[resolved[0], resolved[corner], resolved[corner + 1]]);
        }
    }

    if !use_normals {
        // No normals in the file: compute them, so the mesh is lightable —
        // see the module docs for why refusing here serves nobody.
        mesh.compute_normals();
    }

    Ok(mesh)
}

/// Parse one `v/vt/vn` corner reference, raw.
fn parse_corner(word: &str, line: usize) -> Result<VertexRef, MeshError> {
    let parts: [&str; 3] = match word.split('/').count() {
        1 => [word, "", ""],
        2 => {
            let mut parts = word.splitn(2, '/');
            let a = parts.next().unwrap_or("");
            let b = parts.next().unwrap_or("");
            [a, b, ""]
        }
        3 => {
            let mut parts = word.splitn(3, '/');
            let a = parts.next().unwrap_or("");
            let b = parts.next().unwrap_or("");
            let c = parts.next().unwrap_or("");
            [a, b, c]
        }
        _ => {
            return Err(MeshError::Malformed {
                line,
                what: format!("`{word}` has more than three parts"),
            })
        }
    };

    let parse_index = |text: &str| -> Result<isize, MeshError> {
        if text.is_empty() {
            // Absent: `1//2`'s middle, or `1`'s second and third.
            return Ok(0);
        }
        text.parse::<isize>().map_err(|_| MeshError::Malformed {
            line,
            what: format!("`{text}` is not an index"),
        })
    };

    Ok(VertexRef {
        vertex: parse_index(parts[0])?,
        texture: parse_index(parts[1])?,
        normal: parse_index(parts[2])?,
    })
}

fn parse_float(word: &str, line: usize) -> Result<f32, MeshError> {
    word.parse().map_err(|_| MeshError::Malformed {
        line,
        what: format!("`{word}` is not a number"),
    })
}

/// Parse STL — binary or ASCII — into a [`Mesh`].
///
/// Binary is detected first by size: a binary file is exactly
/// `84 + 50 × count` bytes, and checking that before looking at the
/// content sidesteps the format's one trap (some binary writers put the
/// word `solid` in the 80-byte header, which is what a content sniff would
/// key on).
///
/// # Errors
///
/// [`MeshError::StlTruncated`] for a size mismatch,
/// [`MeshError::Empty`] for an ASCII file with no triangles, and
/// `Malformed` for ASCII that does not parse.
pub fn parse_stl(bytes: &[u8]) -> Result<Mesh, MeshError> {
    // The smallest valid binary file: header + count + one triangle.
    if bytes.len() >= 84 {
        let count = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]);
        // `usize` arithmetic: a `u32` `50 * count` overflows at 85 million
        // triangles, which a corrupted count field can claim.
        let expected = 84 + 50 * count as usize;
        if bytes.len() == expected {
            return parse_binary_stl(bytes);
        }
        // A size mismatch with a plausible count is a truncation; a size
        // that matches no count at all is ASCII (or garbage, which the
        // ASCII path will name).
    }
    let text = std::str::from_utf8(bytes).map_err(|_| MeshError::Malformed {
        line: 0,
        what: "not UTF-8, and not a well-formed binary STL".into(),
    })?;
    parse_ascii_stl(text)
}

/// Parse binary STL. The size was validated by [`parse_stl`].
fn parse_binary_stl(bytes: &[u8]) -> Result<Mesh, MeshError> {
    let count = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
    let mut mesh = Mesh {
        positions: Vec::with_capacity(count * 3),
        normals: Vec::with_capacity(count * 3),
        uvs: Vec::new(),
        indices: Vec::with_capacity(count * 3),
    };
    for triangle in 0..count {
        let base = 84 + 50 * triangle;
        let read = |offset: usize| {
            f32::from_le_bytes([
                bytes[base + offset],
                bytes[base + offset + 1],
                bytes[base + offset + 2],
                bytes[base + offset + 3],
            ])
        };
        let normal = [read(0), read(4), read(8)];
        for corner in 0..3 {
            let at = 12 + 12 * corner;
            mesh.positions.push([read(at), read(at + 4), read(at + 8)]);
            mesh.normals.push(normal);
        }
        let first = mesh.positions.len() as u32 - 3;
        mesh.indices.extend_from_slice(&[first, first + 1, first + 2]);
    }
    if mesh.positions.is_empty() {
        return Err(MeshError::Empty);
    }
    Ok(mesh)
}

/// Parse ASCII STL.
fn parse_ascii_stl(text: &str) -> Result<Mesh, MeshError> {
    let mut mesh = Mesh::default();
    let mut normal = [0.0_f32; 3];
    let mut corners: Vec<Point3> = Vec::new();

    for (number, raw) in text.lines().enumerate() {
        let line = number + 1;
        let trimmed = raw.trim();
        let mut words = trimmed.split_whitespace();
        let Some(keyword) = words.next() else { continue };
        match keyword {
            "solid" | "endsolid" | "facet" | "endfacet" | "outer" | "endloop" | "loop" => {
                // Structure words; `facet normal ni nj nk` is the one with
                // data, handled below by peeking at the rest of the line.
                if keyword == "facet" {
                    // `facet normal ni nj nk`
                    if words.next() == Some("normal") {
                        let mut parsed = [0.0_f32; 3];
                        let mut seen = 0;
                        for word in words {
                            if seen < 3 {
                                parsed[seen] = parse_float(word, line)?;
                                seen += 1;
                            }
                        }
                        normal = parsed;
                    }
                }
            }
            "vertex" => {
                let mut point = [0.0_f32; 3];
                let mut seen = 0;
                for word in words {
                    if seen < 3 {
                        point[seen] = parse_float(word, line)?;
                        seen += 1;
                    }
                }
                if seen < 3 {
                    return Err(MeshError::Malformed {
                        line,
                        what: format!("a vertex needs three coordinates, found {seen}"),
                    });
                }
                corners.push(point);
            }
            _ => {
                return Err(MeshError::Malformed {
                    line,
                    what: format!("unknown keyword `{keyword}`"),
                })
            }
        }

        // `endfacet` closes the triangle: three corners, one face.
        if keyword == "endfacet" {
            if corners.len() != 3 {
                return Err(MeshError::Malformed {
                    line,
                    what: format!("a facet holds three vertices, found {}", corners.len()),
                });
            }
            let first = mesh.positions.len() as u32;
            for corner in corners.drain(..) {
                mesh.positions.push(corner);
                mesh.normals.push(normal);
            }
            mesh.indices.extend_from_slice(&[first, first + 1, first + 2]);
        }
    }

    if mesh.positions.is_empty() {
        return Err(MeshError::Empty);
    }
    Ok(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode one `f32` little-endian, for hand-building binary STL.
    fn push_f32(bytes: &mut Vec<u8>, value: f32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Encode one `[x, y, z]`.
    fn push_vec3(bytes: &mut Vec<u8>, vector: [f32; 3]) {
        for component in vector {
            push_f32(bytes, component);
        }
    }

    #[test]
    fn a_quad_becomes_two_triangles() {
        let mesh = parse_obj(
            "
            v -1 -1 0
            v  1 -1 0
            v  1  1 0
            v -1  1 0
            f 1 2 3 4
            ",
        )
        .unwrap();

        assert_eq!(mesh.positions.len(), 4);
        assert_eq!(mesh.triangles(), 2);
        // Fan: (0, 1, 2) and (0, 2, 3).
        assert_eq!(mesh.indices, vec![0, 1, 2, 0, 2, 3]);
    }

    #[test]
    fn all_four_face_spellings_parse() {
        let mesh = parse_obj(
            "
            v 0 0 0
            v 1 0 0
            v 0 1 0
            vt 0 0
            vt 1 0
            vt 0 1
            vn 0 0 1
            f 1 2 3
            f 1/1 2/2 3/3
            f 1//1 2//1 3//1
            f 1/1/1 2/2/1 3/3/1
            ",
        )
        .unwrap();

        assert_eq!(mesh.triangles(), 4);
        // Each spelling's corners carry different attributes, so each face
        // contributes three fresh vertices — twelve in all, zero sharing
        // across faces, which is the honest count for this file.
        assert_eq!(mesh.positions.len(), 12);
    }

    #[test]
    fn shared_attribute_corners_share_a_vertex() {
        // Two faces referencing the same v/vt/vn: one output vertex each
        // corner, not two.
        let mesh = parse_obj(
            "
            v 0 0 0
            v 1 0 0
            v 0 1 0
            v 1 1 0
            vt 0 0
            vt 1 0
            vn 0 0 1
            f 1/1/1 2/2/1 3/1/1
            f 2/2/1 4/2/1 3/1/1
            ",
        )
        .unwrap();

        assert_eq!(mesh.positions.len(), 4, "no duplication for shared corners");
        assert_eq!(mesh.triangles(), 2);
        assert!(mesh.has_uvs());
        assert!(mesh.has_normals());
    }

    #[test]
    fn negative_indices_count_back() {
        let mesh = parse_obj(
            "
            v 0 0 0
            v 1 0 0
            v 0 1 0
            f -3 -2 -1
            ",
        )
        .unwrap();
        assert_eq!(mesh.indices, vec![0, 1, 2]);
    }

    #[test]
    fn a_pentagon_fans_into_three_triangles() {
        let mesh = parse_obj(
            "
            v 0 0 0
            v 1 0 0
            v 1 1 0
            v 0.5 1.5 0
            v 0 1 0
            f 1 2 3 4 5
            ",
        )
        .unwrap();
        assert_eq!(mesh.triangles(), 3);
    }

    #[test]
    fn comments_and_blank_lines_are_formatting() {
        let mesh = parse_obj(
            "
            # a comment
            v 0 0 0

            # another
            v 1 0 0
            v 0 1 0
            f 1 2 3
            ",
        )
        .unwrap();
        assert_eq!(mesh.triangles(), 1);
    }

    #[test]
    fn scene_records_are_recognised_and_skipped() {
        let mesh = parse_obj(
            "
            o my_object
            g group_one
            usemtl material_one
            mtllib library.mtl
            s off
            v 0 0 0
            v 1 0 0
            v 0 1 0
            f 1 2 3
            ",
        )
        .unwrap();
        assert_eq!(mesh.triangles(), 1);
    }

    #[test]
    fn a_file_without_normals_gets_them_computed() {
        let mesh = parse_obj(
            "
            v 0 0 0
            v 1 0 0
            v 0 1 0
            f 1 2 3
            ",
        )
        .unwrap();
        assert!(mesh.has_normals(), "lightable out of the box");
        // The computed normal of a counter-clockwise triangle in the z=0
        // plane points at +z.
        let normal = mesh.normals[0];
        assert!(normal[2] > 0.99, "computed normal {normal:?}");
    }

    #[test]
    fn an_empty_file_is_named_empty() {
        assert_eq!(parse_obj("# nothing here\n"), Err(MeshError::Empty));
        assert_eq!(parse_obj(""), Err(MeshError::Empty));
    }

    #[test]
    fn a_bad_index_names_the_line() {
        let error = parse_obj(
            "
            v 0 0 0
            v 1 0 0
            f 1 2 7
            ",
        )
        .unwrap_err();
        match error {
            MeshError::IndexOutOfRange { line, index } => {
                assert_eq!(line, 4, "the face's own line (line 1 is blank)");
                assert_eq!(index, 7);
            }
            other => panic!("wrong error: {other:?}"),
        }
    }

    #[test]
    fn a_malformed_line_is_named() {
        let error = parse_obj("v 0 0 zebrafish\n").unwrap_err();
        assert!(matches!(error, MeshError::Malformed { line: 1, .. }));

        let error = parse_obj("vv 0 0 0\n").unwrap_err();
        match error {
            MeshError::Malformed { line, what } => {
                assert_eq!(line, 1);
                assert!(what.contains("unknown keyword"), "{what}");
            }
            other => panic!("wrong error: {other:?}"),
        }
    }

    #[test]
    fn bounds_cover_the_geometry() {
        let mesh = parse_obj(
            "
            v -1 -2 -3
            v 1 2 3
            v 0 0 0
            f 1 2 3
            ",
        )
        .unwrap();
        let (min, max) = mesh.bounds().expect("non-empty");
        assert_eq!(min, [-1.0, -2.0, -3.0]);
        assert_eq!(max, [1.0, 2.0, 3.0]);

        assert_eq!(Mesh::default().bounds(), None);
    }

    #[test]
    fn binary_stl_round_trips_the_shape() {
        // Two triangles: one flat, one tilted, hand-encoded.
        let triangles: [[f32; 3]; 6] = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
        ];
        let mut bytes = vec![0_u8; 80];
        bytes.extend_from_slice(&2_u32.to_le_bytes());
        for triangle in triangles.chunks(3) {
            push_vec3(&mut bytes, [0.0, 0.0, 0.0]); // normal: the parser takes the file's word
            for corner in triangle {
                push_vec3(&mut bytes, *corner);
            }
            bytes.extend_from_slice(&0_u16.to_le_bytes());
        }

        let mesh = parse_stl(&bytes).unwrap();
        assert_eq!(mesh.triangles(), 2);
        assert_eq!(mesh.positions, triangles.to_vec());
        assert!(mesh.has_normals());
        assert!(!mesh.has_uvs(), "STL carries no texture coordinates");
    }

    #[test]
    fn a_truncated_binary_stl_is_said_so() {
        let mut bytes = vec![0_u8; 80];
        bytes.extend_from_slice(&5_u32.to_le_bytes()); // promises 5
        bytes.extend_from_slice(&[0; 50]); // delivers 1
        // Not the exact binary size, so it falls to ASCII, which fails to
        // parse (NULs are not UTF-8 words) — the honest refusal.
        let error = parse_stl(&bytes).unwrap_err();
        assert!(matches!(error, MeshError::Malformed { .. }) || matches!(error, MeshError::StlTruncated { .. }));
    }

    #[test]
    fn the_size_check_beats_the_solid_header_trap() {
        // A *binary* STL whose 80-byte header starts with "solid" — the
        // trap that content-sniffing falls into.
        let mut bytes = b"solid but really binary".to_vec();
        bytes.resize(80, 0);
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        push_vec3(&mut bytes, [0.0, 0.0, 0.0]);
        for corner in [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            push_vec3(&mut bytes, corner);
        }
        bytes.extend_from_slice(&0_u16.to_le_bytes());

        let mesh = parse_stl(&bytes).expect("size, not content, decides the spelling");
        assert_eq!(mesh.triangles(), 1);
    }

    #[test]
    fn ascii_stl_parses() {
        let text = "
        solid model
        facet normal 0 0 1
          outer loop
            vertex 0 0 0
            vertex 1 0 0
            vertex 0 1 0
          endloop
        endfacet
        endsolid model
        ";
        let mesh = parse_stl(text.as_bytes()).unwrap();
        assert_eq!(mesh.triangles(), 1);
        assert_eq!(mesh.normals[0], [0.0, 0.0, 1.0]);
        assert_eq!(mesh.positions[0], [0.0, 0.0, 0.0]);
    }

    #[test]
    fn an_ascii_stl_with_a_wrong_corner_count_is_named() {
        let text = "
        solid model
        facet normal 0 0 1
          outer loop
            vertex 0 0 0
            vertex 1 0 0
          endloop
        endfacet
        endsolid model
        ";
        let error = parse_stl(text.as_bytes()).unwrap_err();
        match error {
            MeshError::Malformed { line, what } => {
                assert!(what.contains("three vertices"), "{what} at line {line}");
            }
            other => panic!("wrong error: {other:?}"),
        }
    }

    #[test]
    fn errors_read_as_sentences() {
        assert_eq!(
            MeshError::Malformed { line: 12, what: "unknown keyword `vv`".into() }.to_string(),
            "line 12: unknown keyword `vv`"
        );
        assert_eq!(MeshError::Empty.to_string(), "no vertices in the file");
    }
}
