//! Custom surfaces — Three.js' `ShaderMaterial` and `NodeMaterial` (§2.2 L5),
//! Blender's shader nodes, Unity's Shader Graph.
//!
//! A [`FragmentShader`] is a function the rasterizer calls for every shaded
//! sample with a [`Fragment`] — world position, normal, UV, view direction,
//! screen position, time, and the colour the built-in lighting model already
//! computed (`lit`). It returns linear RGBA. Writing one is writing a
//! fragment program, in Rust, run on the reference rasterizer.
//!
//! A [`ShaderGraph`] is the node-editor form of the same thing: a DAG of
//! [`ShaderNode`]s (inputs, constants, arithmetic, `mix`, `fresnel`, noise,
//! checker, stripes, gradients, `smoothstep`, …) whose output node is
//! compiled to a `FragmentShader` by [`ShaderGraph::compile`]. Graphs are
//! data, so they can be authored, saved and edited like any other asset.
//!
//! ```
//! use vieww_3d::shader::{ShaderGraph, ShaderNode as N, Input};
//!
//! // Rim light: lit colour + cyan × fresnel⁴.
//! let mut g = ShaderGraph::new();
//! let lit = g.add(N::Input(Input::Lit));
//! let rim = g.add(N::Fresnel { power: 4.0 });
//! let cyan = g.add(N::Const([0.2, 0.9, 1.0, 1.0]));
//! let glow = g.add(N::Mul(cyan, rim));
//! let out = g.add(N::Add(lit, glow));
//! let shader = g.compile(out).unwrap();
//! # let _ = shader;
//! ```

use std::fmt;
use std::sync::Arc;

use crate::math::Vec3;
use crate::scene::Rgb;

/// Everything a fragment program can read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fragment {
    pub world: Vec3,
    pub normal: Vec3,
    pub uv: [f32; 2],
    /// Unit vector from the surface to the eye.
    pub view: Vec3,
    /// Pixel position in the (supersampled) target.
    pub screen: [f32; 2],
    /// Seconds, from [`Renderer::time`](crate::Renderer).
    pub time: f32,
    /// The built-in lighting model's result for this sample (linear).
    pub lit: Rgb,
    /// The material's base colour (after any texture).
    pub albedo: Rgb,
}

/// A fragment program: `Fragment → linear RGBA`.
#[derive(Clone)]
pub struct FragmentShader {
    f: Arc<dyn Fn(&Fragment) -> [f32; 4] + Send + Sync>,
    name: String,
}

impl FragmentShader {
    pub fn new(name: &str, f: impl Fn(&Fragment) -> [f32; 4] + Send + Sync + 'static) -> Self {
        Self {
            f: Arc::new(f),
            name: name.to_owned(),
        }
    }

    #[must_use]
    pub fn run(&self, frag: &Fragment) -> [f32; 4] {
        (self.f)(frag)
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Debug for FragmentShader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FragmentShader({})", self.name)
    }
}

/// Two shaders are equal when they are the same program object.
impl PartialEq for FragmentShader {
    fn eq(&self, o: &Self) -> bool {
        Arc::ptr_eq(&self.f, &o.f)
    }
}

/// A graph input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Lit,
    Albedo,
    /// `(u, v, 0, 1)`.
    Uv,
    /// `(x, y, z, 1)` world.
    World,
    /// Normal in `0..1` per component.
    Normal,
    /// `(t, t, t, 1)`.
    Time,
    /// `n·v` broadcast.
    NdotV,
}

/// A node index in a graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Slot(pub usize);

/// One shader-graph node. Values are RGBA vectors; scalars broadcast.
#[derive(Debug, Clone, PartialEq)]
pub enum ShaderNode {
    Input(Input),
    Const([f32; 4]),
    Add(Slot, Slot),
    Sub(Slot, Slot),
    Mul(Slot, Slot),
    /// `mix(a, b, t.x)`.
    Mix(Slot, Slot, Slot),
    /// Per component sine of `a · freq`.
    Sin(Slot, f32),
    /// `smoothstep(e0, e1, a)` per component.
    Smoothstep(Slot, f32, f32),
    /// `(1 − n·v)^power`, broadcast.
    Fresnel { power: f32 },
    /// Value noise of the world position scaled by `scale`, animated by
    /// time × `speed`, in `0..1`.
    Noise { scale: f32, speed: f32 },
    /// UV checkerboard, `cells` per unit, 0 or 1.
    Checker { cells: f32 },
    /// Stripes across a world axis (0 = x, 1 = y, 2 = z): `0..1` triangle wave.
    Stripes { axis: u8, frequency: f32 },
    /// Map the x of `a` through a two-colour ramp.
    Ramp(Slot, [f32; 4], [f32; 4]),
    /// Swizzle one component across all four.
    Splat(Slot, u8),
    /// `1 − a`.
    OneMinus(Slot),
    /// `a^p` per component.
    Pow(Slot, f32),
}

/// Why a graph did not compile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphError(pub String);

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "shader graph: {}", self.0)
    }
}

impl std::error::Error for GraphError {}

/// A node material.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShaderGraph {
    nodes: Vec<ShaderNode>,
}

fn hash3(x: i32, y: i32, z: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343)
        ^ (y as u32).wrapping_mul(0xd816_3841)
        ^ (z as u32).wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    #[allow(clippy::cast_precision_loss)]
    let v = (h & 0xff_ffff) as f32 / 16_777_215.0;
    v
}

/// Trilinear value noise in `0..1`.
#[must_use]
pub fn value_noise(p: Vec3) -> f32 {
    let (fx, fy, fz) = (p.x.floor(), p.y.floor(), p.z.floor());
    #[allow(clippy::cast_possible_truncation)]
    let (ix, iy, iz) = (fx as i32, fy as i32, fz as i32);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (u, v, w) = (s(p.x - fx), s(p.y - fy), s(p.z - fz));
    let l = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let c = |dx, dy, dz| hash3(ix + dx, iy + dy, iz + dz);
    l(
        l(l(c(0, 0, 0), c(1, 0, 0), u), l(c(0, 1, 0), c(1, 1, 0), u), v),
        l(l(c(0, 0, 1), c(1, 0, 1), u), l(c(0, 1, 1), c(1, 1, 1), u), v),
        w,
    )
}

impl ShaderGraph {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, n: ShaderNode) -> Slot {
        self.nodes.push(n);
        Slot(self.nodes.len() - 1)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Evaluate `out` for one fragment (the interpreter `compile` wraps).
    ///
    /// # Errors
    /// A slot out of range or a cycle.
    pub fn eval(&self, out: Slot, f: &Fragment) -> Result<[f32; 4], GraphError> {
        let mut memo: Vec<Option<[f32; 4]>> = vec![None; self.nodes.len()];
        let mut visiting = vec![false; self.nodes.len()];
        self.eval_rec(out, f, &mut memo, &mut visiting)
    }

    fn eval_rec(
        &self,
        s: Slot,
        f: &Fragment,
        memo: &mut Vec<Option<[f32; 4]>>,
        visiting: &mut Vec<bool>,
    ) -> Result<[f32; 4], GraphError> {
        let node = self
            .nodes
            .get(s.0)
            .ok_or_else(|| GraphError(format!("slot {} out of range", s.0)))?;
        if let Some(v) = memo[s.0] {
            return Ok(v);
        }
        if visiting[s.0] {
            return Err(GraphError(format!("cycle through slot {}", s.0)));
        }
        visiting[s.0] = true;
        let mut ev = |x: Slot| self.eval_rec(x, f, memo, visiting);
        let map2 = |a: [f32; 4], b: [f32; 4], op: fn(f32, f32) -> f32| {
            [op(a[0], b[0]), op(a[1], b[1]), op(a[2], b[2]), op(a[3], b[3])]
        };
        let v = match node {
            ShaderNode::Input(i) => match i {
                Input::Lit => [f.lit.r, f.lit.g, f.lit.b, 1.0],
                Input::Albedo => [f.albedo.r, f.albedo.g, f.albedo.b, 1.0],
                Input::Uv => [f.uv[0], f.uv[1], 0.0, 1.0],
                Input::World => [f.world.x, f.world.y, f.world.z, 1.0],
                Input::Normal => [
                    f.normal.x * 0.5 + 0.5,
                    f.normal.y * 0.5 + 0.5,
                    f.normal.z * 0.5 + 0.5,
                    1.0,
                ],
                Input::Time => [f.time; 4],
                Input::NdotV => [f.normal.dot(f.view).max(0.0); 4],
            },
            ShaderNode::Const(c) => *c,
            ShaderNode::Add(a, b) => map2(ev(*a)?, ev(*b)?, |x, y| x + y),
            ShaderNode::Sub(a, b) => map2(ev(*a)?, ev(*b)?, |x, y| x - y),
            ShaderNode::Mul(a, b) => map2(ev(*a)?, ev(*b)?, |x, y| x * y),
            ShaderNode::Mix(a, b, t) => {
                let (a, b, t) = (ev(*a)?, ev(*b)?, ev(*t)?[0]);
                [0, 1, 2, 3].map(|k| a[k] + (b[k] - a[k]) * t)
            }
            ShaderNode::Sin(a, freq) => ev(*a)?.map(|x| (x * freq).sin()),
            ShaderNode::Smoothstep(a, e0, e1) => ev(*a)?.map(|x| {
                let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
                t * t * (3.0 - 2.0 * t)
            }),
            ShaderNode::Fresnel { power } => {
                [(1.0 - f.normal.dot(f.view).abs()).clamp(0.0, 1.0).powf(*power); 4]
            }
            ShaderNode::Noise { scale, speed } => {
                [value_noise(f.world * *scale + Vec3::splat(f.time * speed)); 4]
            }
            ShaderNode::Checker { cells } => {
                #[allow(clippy::cast_possible_truncation)]
                let c = ((f.uv[0] * cells).floor() as i64 + (f.uv[1] * cells).floor() as i64)
                    .rem_euclid(2);
                #[allow(clippy::cast_precision_loss)]
                [c as f32; 4]
            }
            ShaderNode::Stripes { axis, frequency } => {
                let x = match axis {
                    0 => f.world.x,
                    1 => f.world.y,
                    _ => f.world.z,
                } * frequency;
                [1.0 - (2.0 * (x - x.floor()) - 1.0).abs(); 4]
            }
            ShaderNode::Ramp(a, c0, c1) => {
                let t = ev(*a)?[0].clamp(0.0, 1.0);
                [0, 1, 2, 3].map(|k| c0[k] + (c1[k] - c0[k]) * t)
            }
            ShaderNode::Splat(a, k) => [ev(*a)?[usize::from((*k).min(3))]; 4],
            ShaderNode::OneMinus(a) => ev(*a)?.map(|x| 1.0 - x),
            ShaderNode::Pow(a, p) => ev(*a)?.map(|x| x.max(0.0).powf(*p)),
        };
        visiting[s.0] = false;
        memo[s.0] = Some(v);
        Ok(v)
    }

    /// Validate (every slot reachable from `out` exists, no cycles) and
    /// return a fragment program.
    ///
    /// # Errors
    /// A bad slot or a cycle.
    pub fn compile(self, out: Slot) -> Result<FragmentShader, GraphError> {
        // Validate once with a neutral fragment.
        let probe = Fragment {
            world: Vec3::ZERO,
            normal: Vec3::Z,
            uv: [0.0, 0.0],
            view: Vec3::Z,
            screen: [0.0, 0.0],
            time: 0.0,
            lit: Rgb::BLACK,
            albedo: Rgb::BLACK,
        };
        self.eval(out, &probe)?;
        let name = format!("graph({} nodes)", self.nodes.len());
        Ok(FragmentShader::new(&name, move |f| {
            self.eval(out, f).unwrap_or([1.0, 0.0, 1.0, 1.0])
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frag() -> Fragment {
        Fragment {
            world: Vec3::new(0.25, 0.5, 0.75),
            normal: Vec3::Z,
            uv: [0.3, 0.6],
            view: Vec3::Z,
            screen: [0.0, 0.0],
            time: 1.0,
            lit: Rgb::new(0.5, 0.25, 0.125),
            albedo: Rgb::new(1.0, 1.0, 1.0),
        }
    }

    #[test]
    fn closures_are_fragment_programs() {
        let s = FragmentShader::new("uv", |f| [f.uv[0], f.uv[1], 0.0, 1.0]);
        assert_eq!(s.run(&frag()), [0.3, 0.6, 0.0, 1.0]);
        assert_eq!(s, s.clone());
        assert_ne!(s, FragmentShader::new("uv", |f| [f.uv[0], f.uv[1], 0.0, 1.0]));
    }

    #[test]
    fn graph_arithmetic_and_inputs() {
        let mut g = ShaderGraph::new();
        let lit = g.add(ShaderNode::Input(Input::Lit));
        let two = g.add(ShaderNode::Const([2.0; 4]));
        let m = g.add(ShaderNode::Mul(lit, two));
        assert_eq!(g.eval(m, &frag()).unwrap(), [1.0, 0.5, 0.25, 2.0]);
        let one_minus = g.add(ShaderNode::OneMinus(lit));
        assert_eq!(g.eval(one_minus, &frag()).unwrap()[0], 0.5);
    }

    #[test]
    fn fresnel_is_zero_facing_and_one_grazing() {
        let mut g = ShaderGraph::new();
        let fr = g.add(ShaderNode::Fresnel { power: 2.0 });
        let mut f = frag();
        assert!(g.eval(fr, &f).unwrap()[0].abs() < 1e-6);
        f.view = Vec3::X;
        assert!((g.eval(fr, &f).unwrap()[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn mix_ramp_checker_stripes() {
        let mut g = ShaderGraph::new();
        let a = g.add(ShaderNode::Const([0.0; 4]));
        let b = g.add(ShaderNode::Const([1.0; 4]));
        let t = g.add(ShaderNode::Const([0.25; 4]));
        let m = g.add(ShaderNode::Mix(a, b, t));
        assert_eq!(g.eval(m, &frag()).unwrap()[0], 0.25);
        let r = g.add(ShaderNode::Ramp(t, [0.0; 4], [4.0; 4]));
        assert_eq!(g.eval(r, &frag()).unwrap()[1], 1.0);
        let c = g.add(ShaderNode::Checker { cells: 2.0 });
        // uv (0.3, 0.6) × 2 → cells (0, 1) → odd.
        assert_eq!(g.eval(c, &frag()).unwrap()[0], 1.0);
        let st = g.add(ShaderNode::Stripes { axis: 0, frequency: 1.0 });
        assert!((g.eval(st, &frag()).unwrap()[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn noise_is_bounded_smooth_and_animated() {
        let mut g = ShaderGraph::new();
        let n = g.add(ShaderNode::Noise { scale: 3.0, speed: 1.0 });
        let mut f = frag();
        let mut prev = g.eval(n, &f).unwrap()[0];
        for i in 1..100 {
            #[allow(clippy::cast_precision_loss)]
            {
                f.world.x = i as f32 * 0.001;
            }
            let v = g.eval(n, &f).unwrap()[0];
            assert!((0.0..=1.0).contains(&v));
            assert!((v - prev).abs() < 0.05, "continuous");
            prev = v;
        }
        let a = g.eval(n, &f).unwrap()[0];
        f.time = 1.37;
        assert_ne!(a, g.eval(n, &f).unwrap()[0]);
    }

    #[test]
    fn cycles_and_bad_slots_are_refused() {
        let mut g = ShaderGraph::new();
        let a = g.add(ShaderNode::Add(Slot(1), Slot(1)));
        g.add(ShaderNode::OneMinus(a));
        assert!(g.clone().compile(a).is_err());
        let mut h = ShaderGraph::new();
        let x = h.add(ShaderNode::OneMinus(Slot(9)));
        assert!(h.compile(x).is_err());
    }

    #[test]
    fn a_compiled_graph_runs_like_the_interpreter() {
        let mut g = ShaderGraph::new();
        let uv = g.add(ShaderNode::Input(Input::Uv));
        let s = g.add(ShaderNode::Smoothstep(uv, 0.2, 0.4));
        let expect = g.eval(s, &frag()).unwrap();
        let prog = g.compile(s).unwrap();
        assert_eq!(prog.run(&frag()), expect);
    }
}
