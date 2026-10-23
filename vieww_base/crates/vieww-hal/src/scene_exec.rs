//! The backend-agnostic execution of a [`ScenePlan`]: the traversal that
//! turns a plan into an ordered list of render operations, extracted from
//! `vulkan::scene`'s `execute` so every backend runs the *same* frame.
//!
//! # Why this module exists
//!
//! `vulkan::scene::SceneRenderer` holds two things: Vulkan resource
//! management, and a plan→passes traversal — *which* render passes a frame
//! needs, in what order, with which scissored regions, blend pipelines and
//! push-constant payloads. Only the first part is Vulkan's. The second is
//! pure logic over [`ScenePlan`] and `post.wgsl`'s mode table, and it is the
//! part a Metal or D3D12 port would otherwise have to copy line for line —
//! at which point the three backends execute three gradually diverging
//! definitions of the same frame, and "the GPU renderers agree with the CPU
//! oracle" quietly stops meaning one thing.
//!
//! So the traversal lives here, once, and [`frame_passes`] is the one
//! function that produces it. `vulkan::scene` interprets the result into
//! Vulkan commands; `metal::scene` and `d3d12::scene` interpret the same
//! stream into Metal and D3D12 ones. The interpreter contract is
//! [`SceneOp`]'s docs; the resource contract (target formats, atlas
//! formats, the WGSL sources and their binding conventions) is the
//! [`crate::vulkan::scene`] module doc's, shared verbatim by the ports.
//!
//! # What is here and what is not
//!
//! Everything the three backends must agree on is here: the clip algebra a
//! damaged frame may redraw ([`frame_passes`]' `damage` handling, including
//! the retained-frame guard and the skip of whole untouched layers), the
//! `post.wgsl` push-constant layout ([`PostPush`]) and mode table
//! ([`mode`]), the 28 blend modes' numbering ([`blend_index`]), the
//! layer/backdrop-filter/shadow pass construction, and the per-op
//! [`PassKind`] a backend's timestamp marks are charged to.
//!
//! Everything backend-owned is not: target allocation and lifetimes, the
//! readback (and its premultiply-straight conversion), present, overlap and
//! the swapchain blit, per-frame timing beyond the kind each op belongs to.
//! A backend marks [`PassKind::Blit`] itself when the op stream ends and it
//! copies the finished frame out — Vulkan's timestamp for it, the ports'
//! CPU-side equivalent.
//!
//! # Verification
//!
//! The traversal is pure logic, so its own tests run anywhere — no GPU, no
//! backend feature — and pin the shapes the parity suites exercise on real
//! hardware: nested layers, damaged frames, skipped layers, backdrop
//! filters, blend snapshots, masks and shadows. The Vulkan parity suites
//! (`tests/vulkan_scene.rs`, `tests/vulkan_compositor.rs`) remain the
//! pixel-level arbiter, on hardware; they run against this traversal since
//! `vulkan::scene::execute` became an interpreter over its output.

use vieww_foundation::{BlendMode, Color, Rect};
use vieww_gpu::{FilterOp, MaskRef, PixelRect, ScenePlan, Step};

/// Which image a pass reads or writes — one frame, one scratch twin per
/// nesting level, and the 1x1 transparent stand-in bound wherever a pass
/// reads nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetId {
    /// `L[n]`: the frame at 0, the nested layer at `n`.
    Level(usize),
    /// `S[n]`: the ping-pong partner of `L[n]`.
    Scratch(usize),
    /// The 1x1 transparent target.
    Dummy,
}

/// The kind of work an op is — the same kinds [`crate::vulkan::GpuPassTimes`]
/// buckets a frame's GPU timestamps into, so every backend's profiler
/// charges the same stream the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassKind {
    /// Geometry into a target.
    Draw,
    /// Layer begins, masks, composites.
    Layers,
    /// Blurs and colour matrices.
    Filters,
    /// Shadow tints.
    Shadows,
    /// The frame's copy out (marked by the backend, not an op kind).
    Blit,
}

/// Push constants for `post.wgsl`: eight `vec4<f32>`, 128 bytes — the
/// minimum push-constant size every Vulkan implementation guarantees, and
/// (not coincidentally) exactly the `set_fragment_bytes`/root-constant
/// budget the ports bind it through.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
#[repr(C)]
pub struct PostPush {
    /// `x0, y0, x1, y1` of the region `src` is readable in.
    pub region: [f32; 4],
    /// mode, radius, blend mode, alpha.
    pub args: [f32; 4],
    /// Mode-specific parameters: matrix rows, mask offsets, shadow colour.
    pub p: [[f32; 4]; 6],
}

/// `post.wgsl`'s mode table.
pub mod mode {
    pub const COPY: f32 = 0.0;
    pub const BOX_H: f32 = 1.0;
    pub const BOX_V: f32 = 2.0;
    pub const MATRIX: f32 = 3.0;
    pub const MASK: f32 = 4.0;
    pub const BLEND: f32 = 5.0;
    pub const OVER: f32 = 6.0;
    pub const SHADOW: f32 = 7.0;
}

/// `post.wgsl`'s blend-mode numbering: `BlendMode`'s declaration order.
#[must_use]
pub const fn blend_index(mode: BlendMode) -> f32 {
    use BlendMode::*;
    match mode {
        Normal => 0.0,
        Clear => 1.0,
        Src => 2.0,
        Dst => 3.0,
        DstOver => 4.0,
        SrcIn => 5.0,
        DstIn => 6.0,
        SrcOut => 7.0,
        DstOut => 8.0,
        SrcAtop => 9.0,
        DstAtop => 10.0,
        Xor => 11.0,
        Plus => 12.0,
        Multiply => 13.0,
        Screen => 14.0,
        Overlay => 15.0,
        Darken => 16.0,
        Lighten => 17.0,
        ColorDodge => 18.0,
        ColorBurn => 19.0,
        HardLight => 20.0,
        SoftLight => 21.0,
        Difference => 22.0,
        Exclusion => 23.0,
        Hue => 24.0,
        Saturation => 25.0,
        Color => 26.0,
        Luminosity => 27.0,
    }
}

/// One post pass, described backend-neutrally. The backend binds `src` and
/// `aux` as `post.wgsl`'s first two textures (the mask atlas as the third),
/// sets `push` as the immediates block, and draws one full-screen triangle
/// under each scissor of the op's `scissors`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PostPass {
    /// Which target the pass writes.
    pub into: TargetId,
    /// Where it writes, before frame-clip intersection.
    pub scissor: PixelRect,
    /// Where `src` is readable; transparent outside.
    pub read: PixelRect,
    /// What it reads.
    pub src: TargetId,
    /// The destination snapshot a blend reads, or [`TargetId::Dummy`].
    pub aux: TargetId,
    /// Whether the pass draws with the premultiplied-over blend pipeline.
    pub over: bool,
    /// The immediates block.
    pub push: PostPush,
}

impl PostPass {
    /// A pass of `mode` writing `rect` of `into`, reading `src`.
    #[must_use]
    pub const fn new(into: TargetId, rect: PixelRect, src: TargetId, mode: f32) -> Self {
        Self {
            into,
            scissor: rect,
            read: rect,
            src,
            aux: TargetId::Dummy,
            over: false,
            push: PostPush {
                region: [0.0; 4],
                args: [mode, 0.0, 0.0, 0.0],
                p: [[0.0; 4]; 6],
            },
        }
    }
}

/// One operation of a frame, in order. A backend keeps one "open target"
/// across the stream — see [`SceneOp::Begin`].
#[allow(
    clippy::large_enum_variant,
    reason = "PostPass carries the whole 128-byte immediates block by value: \
              a frame is tens of ops, built once and consumed once"
)]
#[derive(Clone, Debug, PartialEq)]
pub enum SceneOp {
    /// End the pass open on any other target and begin one on `target`.
    ///
    /// `clear` is the premultiplied colour the target starts as: `Some`
    /// always (re)starts the pass cleared; `None` keeps what the target
    /// holds, beginning a pass only when none is open on it yet. That pair
    /// is `Recorder::begin`/`ensure_open`'s contract, stated once here so
    /// every interpreter implements the same thing.
    ///
    /// `kind` is the [`PassKind`] this begin is charged to — [`PassKind::Layers`]
    /// for a nested layer's, `None` for the frame's own.
    Begin {
        target: TargetId,
        clear: Option<[f32; 4]>,
        kind: Option<PassKind>,
    },
    /// Clear `rects` of the *currently open* target to `color` — a damaged
    /// frame's own regions of `L[0]`, whose rest is kept.
    ClearRegions {
        rects: Vec<PixelRect>,
        color: [f32; 4],
    },
    /// One indexed draw of `plan.indices[first_index ..][.. index_count]`
    /// through the scene pipeline, into `target` (opened on demand — a pass
    /// already open on it is reused), under `scissor`.
    DrawRun {
        target: TargetId,
        first_index: u32,
        index_count: u32,
        scissor: PixelRect,
    },
    /// One post pass, drawn once per scissor. Never emitted with empty
    /// `scissors` — a pass that touches nothing does not even open its
    /// target, exactly as `vulkan::scene`'s `pass` returns early.
    Post {
        pass: PostPass,
        scissors: Vec<PixelRect>,
        /// The kind of work this pass is — a backend's timestamp marks.
        kind: PassKind,
    },
}

/// What a traversal decided a frame redraws, and what the frame target
/// holds once it finishes — the backend stores `retained` only if the frame
/// completes; on error nothing is kept, exactly as `vulkan::scene` clears
/// its `retained` the moment recording begins.
#[derive(Debug, Clone)]
pub struct FrameOps {
    /// The frame, in order.
    pub ops: Vec<SceneOp>,
    /// How much of the frame target the ops redraw.
    pub redrawn: Redrawn,
    /// What `L[0]` holds after the frame: this size, drawn over this clear.
    pub retained: Retained,
    /// The rectangles a damaged frame redraws, when it kept the previous
    /// frame — the backend's own `frame_clip`, should it need it outside the
    /// ops (Vulkan's probe logic does).
    pub frame_clip: Option<Vec<PixelRect>>,
}

/// How much of the frame target a frame redrew.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Redrawn {
    /// Every pixel: no damage was given, or what the target held could not
    /// be kept (a new size or clear colour, a backdrop filter reaching
    /// outside the damage).
    Full,
    /// Only the damage: this many pixels (each rectangle's area, summed).
    Damage {
        /// Pixels redrawn.
        pixels: u64,
    },
}

impl Redrawn {
    /// Pixels redrawn, given the frame's own size for [`Redrawn::Full`].
    #[must_use]
    pub const fn pixels(self, width: u32, height: u32) -> u64 {
        match self {
            Self::Full => width as u64 * height as u64,
            Self::Damage { pixels } => pixels,
        }
    }
}

/// What a frame target holds between frames: the frame's own finished
/// pixels, at `size`, drawn over `clear`. A damaged frame keeps them only
/// while both still describe the frame being drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retained {
    /// Frame size the target holds.
    pub size: (u32, u32),
    /// The clear colour it was drawn over.
    pub clear: Color,
}

/// More damage rectangles than this and the frame redraws their bounding
/// box instead: every rectangle repeats each top-level draw call under its
/// own scissor, and past a handful the repeated vertex work costs more than
/// the pixels saved.
const MAX_CLIP_RECTS: usize = 8;

/// Turn `plan` into the ordered operations of one frame at `width x height`
/// over `clear`.
///
/// `damage` — in this frame's device pixels — is what differs from the
/// previous frame; the traversal keeps the previous frame's untouched
/// pixels when `retained` says the target still holds that frame (same
/// size, same clear), and redraws everything otherwise. `None` redraws
/// everything, as a first frame does.
#[must_use]
pub fn frame_passes(
    plan: &ScenePlan,
    width: u32,
    height: u32,
    clear: Color,
    damage: Option<&[PixelRect]>,
    retained: Option<Retained>,
) -> FrameOps {
    let width = width.max(1);
    let height = height.max(1);
    let a = f32::from(clear.a) / 255.0;
    let premultiplied_clear = [
        f32::from(clear.r) / 255.0 * a,
        f32::from(clear.g) / 255.0 * a,
        f32::from(clear.b) / 255.0 * a,
        a,
    ];

    let clip = damage.and_then(|d| usable_clip(plan, d, width, height, clear, retained));
    let skip = clip
        .as_deref()
        .map(|c| skippable_layers(plan, c))
        .unwrap_or_default();
    let redrawn = clip.as_ref().map_or(Redrawn::Full, |c| Redrawn::Damage {
        pixels: c
            .iter()
            .map(|r| u64::from(r.width()) * u64::from(r.height()))
            .sum(),
    });

    let frame = PixelRect {
        x0: 0,
        y0: 0,
        x1: width as i32,
        y1: height as i32,
    };

    let mut ops: Vec<SceneOp> = Vec::new();
    match &clip {
        None => ops.push(SceneOp::Begin {
            target: TargetId::Level(0),
            clear: Some(premultiplied_clear),
            kind: None,
        }),
        Some(clip) => {
            ops.push(SceneOp::Begin {
                target: TargetId::Level(0),
                clear: None,
                kind: None,
            });
            if !clip.is_empty() {
                ops.push(SceneOp::ClearRegions {
                    rects: clip.clone(),
                    color: premultiplied_clear,
                });
            }
        }
    }

    // The op-emitting closures need mutable state shared with the traversal
    // (`depth`), so this is one function with helpers, not a struct.
    let mut depth = 0usize;
    let mut index = 0usize;
    while index < plan.steps.len() {
        let step = plan.steps[index];
        if let Some(&pop) = skip.get(&index) {
            index = pop + 1;
            continue;
        }
        index += 1;
        match step {
            Step::Draw {
                first_run,
                run_count,
            } => {
                if plan.vertices.is_empty() || plan.indices.is_empty() {
                    continue;
                }
                let clip = if depth == 0 { clip.as_deref() } else { None };
                for run in &plan.runs[first_run..first_run + run_count] {
                    let scissor = run.scissor.map_or(frame, |r| scissor_of(r, width, height));
                    for s in clipped(scissor, clip) {
                        ops.push(SceneOp::DrawRun {
                            target: TargetId::Level(depth),
                            first_index: run.first_index,
                            index_count: run.index_count,
                            scissor: s,
                        });
                    }
                }
            }
            Step::PushLayer { rect, backdrop } => {
                let parent = depth;
                depth += 1;
                ops.push(SceneOp::Begin {
                    target: TargetId::Level(depth),
                    clear: Some([0.0; 4]),
                    kind: Some(PassKind::Layers),
                });
                if let (Some(filter), false) = (backdrop, rect.is_empty()) {
                    post(
                        &mut ops,
                        &clip,
                        frame,
                        PostPass::new(
                            TargetId::Level(depth),
                            rect,
                            TargetId::Level(parent),
                            mode::COPY,
                        ),
                        PassKind::Layers,
                    );
                    filter_passes(
                        &mut ops,
                        &clip,
                        frame,
                        depth,
                        rect,
                        filter,
                        PassKind::Filters,
                    );
                }
            }
            Step::PopLayer {
                rect,
                composite,
                filter,
                mask,
                blend,
                alpha,
            } => {
                let child = depth;
                depth = depth.saturating_sub(1);
                let mut source = TargetId::Level(child);
                if !rect.is_empty() {
                    if let Some(filter) = filter {
                        filter_passes(
                            &mut ops,
                            &clip,
                            frame,
                            child,
                            rect,
                            filter,
                            PassKind::Filters,
                        );
                    }
                    if let Some(MaskRef { offset }) = mask {
                        let mut pass =
                            PostPass::new(TargetId::Scratch(child), rect, source, mode::MASK);
                        pass.push.p[0] = [offset[0], offset[1], 1.0, 0.0];
                        post(&mut ops, &clip, frame, pass, PassKind::Layers);
                        source = TargetId::Scratch(child);
                    }
                }
                let Some(region) = composite else {
                    continue;
                };
                if blend == BlendMode::Normal {
                    let mut pass =
                        PostPass::new(TargetId::Level(depth), region, source, mode::OVER);
                    pass.read = rect;
                    pass.over = true;
                    pass.push.args[3] = alpha;
                    post(&mut ops, &clip, frame, pass, PassKind::Layers);
                } else {
                    post(
                        &mut ops,
                        &clip,
                        frame,
                        PostPass::new(
                            TargetId::Scratch(depth),
                            region,
                            TargetId::Level(depth),
                            mode::COPY,
                        ),
                        PassKind::Layers,
                    );
                    let mut pass =
                        PostPass::new(TargetId::Level(depth), region, source, mode::BLEND);
                    pass.read = rect;
                    pass.aux = TargetId::Scratch(depth);
                    pass.push.args[2] = blend_index(blend);
                    pass.push.args[3] = alpha;
                    post(&mut ops, &clip, frame, pass, PassKind::Layers);
                }
            }
            Step::Shadow {
                patch,
                scissor,
                coverage,
                color,
                clip: shadow_clip,
            } => {
                if patch.is_empty() || scissor.is_empty() {
                    continue;
                }
                if depth == 0 && clip.as_deref().is_some_and(|c| !touches(c, scissor)) {
                    continue;
                }
                let mut tint = PostPass::new(
                    TargetId::Level(depth),
                    scissor,
                    TargetId::Dummy,
                    mode::SHADOW,
                );
                tint.read = patch;
                tint.over = true;
                tint.push.p[0] = [coverage.offset[0], coverage.offset[1], 1.0, 0.0];
                tint.push.p[2] = color;
                if let Some(clip) = shadow_clip {
                    tint.push.p[1] = [clip.offset[0], clip.offset[1], 1.0, 0.0];
                }
                post(&mut ops, &clip, frame, tint, PassKind::Shadows);
            }
        }
    }

    FrameOps {
        ops,
        redrawn,
        retained: Retained {
            size: (width, height),
            clear,
        },
        frame_clip: clip,
    }
}

/// Emit `pass` under its frame-clipped scissors, or not at all when it
/// touches nothing. The clip a pass is held to is the damage clip whenever
/// it writes the frame's own level or scratch — a top-level blend's
/// snapshot reads the frame pixel for pixel, so both must stay inside it.
#[allow(clippy::too_many_arguments)]
fn post(
    ops: &mut Vec<SceneOp>,
    frame_clip: &Option<Vec<PixelRect>>,
    frame: PixelRect,
    mut pass: PostPass,
    kind: PassKind,
) {
    debug_assert!(
        pass.into != pass.src && pass.into != pass.aux,
        "a pass may not read its own target"
    );
    let clip = if matches!(pass.into, TargetId::Level(0) | TargetId::Scratch(0)) {
        frame_clip.as_deref()
    } else {
        None
    };
    let scissors = clipped(pass.scissor.intersect(frame), clip);
    if scissors.is_empty() {
        return;
    }
    pass.push.region = [
        pass.read.x0 as f32,
        pass.read.y0 as f32,
        pass.read.x1 as f32,
        pass.read.y1 as f32,
    ];
    ops.push(SceneOp::Post {
        pass,
        scissors,
        kind,
    });
}

/// A filter over `rect` of `L[level]`, result left in `L[level]`: three
/// horizontal+vertical box passes ping-ponging through `S[level]`, then the
/// colour matrix through the scratch, with `edge = 1` — the nearest-edge
/// clamp `effects::blur` uses, CSS's `duplicate` edge mode for `filter` and
/// `backdrop-filter`.
#[allow(clippy::too_many_arguments)]
fn filter_passes(
    ops: &mut Vec<SceneOp>,
    frame_clip: &Option<Vec<PixelRect>>,
    frame: PixelRect,
    level: usize,
    rect: PixelRect,
    filter: FilterOp,
    kind: PassKind,
) {
    if let Some(radius) = filter.box_radius {
        #[allow(clippy::cast_precision_loss)]
        let r = radius as f32;
        let edge = 1.0;
        for _ in 0..3 {
            let mut h = PostPass::new(
                TargetId::Scratch(level),
                rect,
                TargetId::Level(level),
                mode::BOX_H,
            );
            h.push.args[1] = r;
            h.push.args[2] = edge;
            post(ops, frame_clip, frame, h, kind);
            let mut v = PostPass::new(
                TargetId::Level(level),
                rect,
                TargetId::Scratch(level),
                mode::BOX_V,
            );
            v.push.args[1] = r;
            v.push.args[2] = edge;
            post(ops, frame_clip, frame, v, kind);
        }
    }
    if let Some(m) = filter.color_matrix {
        let mut pass = PostPass::new(
            TargetId::Scratch(level),
            rect,
            TargetId::Level(level),
            mode::MATRIX,
        );
        pass.push.p[0] = [m[0], m[1], m[2], m[3]];
        pass.push.p[1] = [m[5], m[6], m[7], m[8]];
        pass.push.p[2] = [m[10], m[11], m[12], m[13]];
        pass.push.p[3] = [m[15], m[16], m[17], m[18]];
        pass.push.p[4] = [m[4], m[9], m[14], m[19]];
        post(ops, frame_clip, frame, pass, kind);
        post(
            ops,
            frame_clip,
            frame,
            PostPass::new(
                TargetId::Level(level),
                rect,
                TargetId::Scratch(level),
                mode::COPY,
            ),
            kind,
        );
    }
}

/// Convert a frame of premultiplied RGBA16F texels into the straight-alpha
/// RGBA8 bytes every backend's readback hands back — `half_to_f32`,
/// unpremultiply, `v * 255 + 0.5`, alpha unchanged.
///
/// The readback conversion is shared for the same reason the traversal is:
/// the target format is part of the contract, so "what the CPU receives for
/// a finished frame" is one definition, not one per backend.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn straight_rgba8_from_rgba16f(pixels: &[u16]) -> Vec<u8> {
    let mut out = vec![0u8; pixels.len()];
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let byte = |v: f32| (v * 255.0 + 0.5) as u8;
    for (pixel, slot) in pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(out.as_chunks_mut::<4>().0)
    {
        let [r, g, b, a] = [pixel[0], pixel[1], pixel[2], pixel[3]].map(half_to_f32);
        let inv = if a > 0.0 { 1.0 / a } else { 0.0 };
        slot.copy_from_slice(&[byte(r * inv), byte(g * inv), byte(b * inv), byte(a)]);
    }
    out
}

/// An IEEE 754 binary16 as an `f32` — the readback conversion's core, and
/// (through it) the only half-float arithmetic any backend needs. Verbatim
/// the arithmetic `vulkan::scene`'s readback always used, so the shared
/// conversion changes nobody's bytes.
#[must_use]
pub fn half_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = i32::from((h >> 10) & 0x1f);
    let mantissa = f32::from(h & 0x3ff);
    match exponent {
        0 => sign * mantissa * 2f32.powi(-24),
        31 if mantissa == 0.0 => sign * f32::INFINITY,
        31 => f32::NAN,
        e => sign * (1.0 + mantissa / 1024.0) * 2f32.powi(e - 15),
    }
}

/// The damage a frame may actually redraw: `damage` clamped to the frame,
/// or `None` (redraw everything) when the target does not hold the previous
/// frame at this size and clear, or a top-level backdrop filter reaches
/// outside the damage.
fn usable_clip(
    plan: &ScenePlan,
    damage: &[PixelRect],
    width: u32,
    height: u32,
    clear: Color,
    retained: Option<Retained>,
) -> Option<Vec<PixelRect>> {
    let wanted = Retained {
        size: (width, height),
        clear,
    };
    if retained != Some(wanted) {
        return None;
    }
    #[expect(clippy::cast_possible_wrap, reason = "frame extents")]
    let frame = PixelRect {
        x0: 0,
        y0: 0,
        x1: width as i32,
        y1: height as i32,
    };
    // Disjoint, because every rectangle repeats each top-level draw under
    // its own scissor: where two overlapped, a translucent draw — a group
    // opacity, a shadow's tint, a blend — would be applied twice.
    let mut clip = disjoint(
        damage
            .iter()
            .map(|rect| rect.intersect(frame))
            .filter(|rect| !rect.is_empty()),
    );
    if clip.len() > MAX_CLIP_RECTS {
        let bounds = clip.iter().fold(clip[0], |a, b| PixelRect {
            x0: a.x0.min(b.x0),
            y0: a.y0.min(b.y0),
            x1: a.x1.max(b.x1),
            y1: a.y1.max(b.y1),
        });
        clip = vec![bounds];
    }
    let mut depth = 0usize;
    for step in &plan.steps {
        match *step {
            Step::PushLayer { rect, backdrop } => {
                if depth == 0
                    && backdrop.is_some()
                    && touches(&clip, rect)
                    && !clip.iter().any(|c| contains(*c, rect))
                {
                    return None;
                }
                depth += 1;
            }
            Step::PopLayer { .. } => depth = depth.saturating_sub(1),
            Step::Draw { .. } | Step::Shadow { .. } => {}
        }
    }
    Some(clip)
}

/// For each top-level `PushLayer` whose composite misses `clip`, the index
/// of its `PopLayer`: a damaged frame skips the whole layer.
fn skippable_layers(
    plan: &ScenePlan,
    clip: &[PixelRect],
) -> std::collections::HashMap<usize, usize> {
    let mut skip = std::collections::HashMap::new();
    let mut open: Vec<usize> = Vec::new();
    for (index, step) in plan.steps.iter().enumerate() {
        match *step {
            Step::PushLayer { .. } => open.push(index),
            Step::PopLayer { composite, .. } => {
                if let Some(push) = open.pop() {
                    if open.is_empty() && !composite.is_some_and(|c| touches(clip, c)) {
                        skip.insert(push, index);
                    }
                }
            }
            Step::Draw { .. } | Step::Shadow { .. } => {}
        }
    }
    skip
}

/// A run's `Rect` scissor, clamped to the frame — the floor/ceil/`max(0)`
/// rounding `NativeRenderer` sizes its own buffers with.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "device pixel coordinates: small, non-negative after clamping"
)]
fn scissor_of(rect: Rect, width: u32, height: u32) -> PixelRect {
    let left = (rect.left.floor().max(0.0) as u32).min(width);
    let top = (rect.top.floor().max(0.0) as u32).min(height);
    let right = (rect.right.ceil().max(0.0) as u32).min(width);
    let bottom = (rect.bottom.ceil().max(0.0) as u32).min(height);
    PixelRect {
        x0: left as i32,
        y0: top as i32,
        x1: right as i32,
        y1: bottom as i32,
    }
}

/// `scissor` itself, or its intersection with each of `clip`'s rectangles;
/// empty results dropped either way.
fn clipped(scissor: PixelRect, clip: Option<&[PixelRect]>) -> Vec<PixelRect> {
    let Some(clip) = clip else {
        return if scissor.is_empty() {
            Vec::new()
        } else {
            vec![scissor]
        };
    };
    clip.iter()
        .map(|c| scissor.intersect(*c))
        .filter(|s| !s.is_empty())
        .collect()
}

/// `rects`' union as rectangles that do not overlap: each one minus every
/// rectangle already taken, as up to four bands around the overlap.
pub fn disjoint(rects: impl IntoIterator<Item = PixelRect>) -> Vec<PixelRect> {
    let mut out: Vec<PixelRect> = Vec::new();
    for rect in rects {
        let mut pieces = vec![rect];
        for taken in &out {
            pieces = pieces
                .into_iter()
                .flat_map(|piece| subtract(piece, *taken))
                .collect();
        }
        out.extend(pieces);
    }
    out
}

/// `a` minus `b`: what is left of `a` as up to four non-overlapping bands
/// (above, below, then left and right of the overlap).
fn subtract(a: PixelRect, b: PixelRect) -> Vec<PixelRect> {
    let overlap = a.intersect(b);
    if overlap.is_empty() {
        return vec![a];
    }
    let bands = [
        PixelRect {
            y1: overlap.y0,
            ..a
        },
        PixelRect {
            y0: overlap.y1,
            ..a
        },
        PixelRect {
            y0: overlap.y0,
            y1: overlap.y1,
            x1: overlap.x0,
            ..a
        },
        PixelRect {
            y0: overlap.y0,
            y1: overlap.y1,
            x0: overlap.x1,
            ..a
        },
    ];
    bands.into_iter().filter(|band| !band.is_empty()).collect()
}

/// Whether `rect` overlaps any of `clip`'s rectangles.
fn touches(clip: &[PixelRect], rect: PixelRect) -> bool {
    clip.iter().any(|c| !c.intersect(rect).is_empty())
}

/// Whether `outer` contains all of `inner`.
const fn contains(outer: PixelRect, inner: PixelRect) -> bool {
    outer.x0 <= inner.x0 && outer.y0 <= inner.y0 && outer.x1 >= inner.x1 && outer.y1 >= inner.y1
}

#[cfg(test)]
mod tests {
    //! The traversal is pure logic — every test here runs with no GPU and no
    //! backend feature, which is the point: the shapes the hardware parity
    //! suites exercise are pinned where every platform can see them fail.

    use std::collections::BTreeMap;

    use super::*;
    use vieww_gpu::DrawRun;

    fn rect(x0: i32, y0: i32, x1: i32, y1: i32) -> PixelRect {
        PixelRect { x0, y0, x1, y1 }
    }

    /// A plan of `steps` with one run of `indices`, so a `Draw` step has
    /// something to draw.
    fn plan(steps: Vec<Step>, target_depth: usize) -> ScenePlan {
        ScenePlan {
            vertices: vec![vieww_gpu::Vertex {
                position: [0.0, 0.0],
                color: [1.0, 0.0, 0.0, 1.0],
                uv: [0.0; 2],
                texture_kind: 0.0,
                params: [0.0; 4],
                local: [0.0; 2],
                extra: [0.0; 4],
                shape: [0.0; 4],
                mask: [0.0; 4],
            }],
            indices: vec![0, 0, 0],
            runs: vec![DrawRun {
                first_index: 0,
                index_count: 3,
                scissor: None,
            }],
            steps,
            target_depth,
            unsupported: BTreeMap::new(),
        }
    }

    const W: u32 = 100;
    const H: u32 = 100;
    const CLEAR: Color = Color::BLACK;

    fn begins(ops: &[SceneOp]) -> Vec<TargetId> {
        ops.iter()
            .filter_map(|op| match op {
                SceneOp::Begin { target, .. } => Some(*target),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_flat_frame_is_one_cleared_begin_and_its_draws() {
        let plan = plan(
            vec![Step::Draw {
                first_run: 0,
                run_count: 1,
            }],
            0,
        );
        let frame = frame_passes(&plan, W, H, CLEAR, None, None);
        assert_eq!(begins(&frame.ops), vec![TargetId::Level(0)]);
        assert_eq!(
            frame.ops[1],
            SceneOp::DrawRun {
                target: TargetId::Level(0),
                first_index: 0,
                index_count: 3,
                scissor: rect(0, 0, 100, 100)
            }
        );
        assert_eq!(frame.redrawn, Redrawn::Full);
        assert_eq!(
            frame.retained,
            Retained {
                size: (W, H),
                clear: CLEAR
            }
        );
    }

    #[test]
    fn a_nested_layer_begins_cleared_and_composites_over() {
        let layer = rect(10, 10, 50, 50);
        let plan = plan(
            vec![
                Step::PushLayer {
                    rect: layer,
                    backdrop: None,
                },
                Step::Draw {
                    first_run: 0,
                    run_count: 1,
                },
                Step::PopLayer {
                    rect: layer,
                    composite: Some(layer),
                    filter: None,
                    mask: None,
                    blend: BlendMode::Normal,
                    alpha: 0.5,
                },
            ],
            1,
        );
        let frame = frame_passes(&plan, W, H, CLEAR, None, None);
        assert_eq!(
            begins(&frame.ops),
            vec![TargetId::Level(0), TargetId::Level(1)]
        );
        let SceneOp::Post { pass, .. } = &frame.ops[3] else {
            panic!("the composite is a post pass: {:?}", frame.ops);
        };
        assert_eq!(pass.into, TargetId::Level(0));
        assert_eq!(pass.src, TargetId::Level(1));
        assert_eq!(pass.push.args[0], mode::OVER);
        assert_eq!(pass.push.args[3], 0.5);
        assert!(pass.over);
        // `read` is the layer's rect — transparent outside it.
        assert_eq!(pass.push.region, [10.0, 10.0, 50.0, 50.0]);
    }

    #[test]
    fn a_blend_mode_snapshots_the_destination_first() {
        let layer = rect(0, 0, 100, 100);
        let plan = plan(
            vec![
                Step::PushLayer {
                    rect: layer,
                    backdrop: None,
                },
                Step::Draw {
                    first_run: 0,
                    run_count: 1,
                },
                Step::PopLayer {
                    rect: layer,
                    composite: Some(layer),
                    filter: None,
                    mask: None,
                    blend: BlendMode::Multiply,
                    alpha: 1.0,
                },
            ],
            1,
        );
        let frame = frame_passes(&plan, W, H, CLEAR, None, None);
        let posts: Vec<&PostPass> = frame
            .ops
            .iter()
            .filter_map(|op| match op {
                SceneOp::Post { pass, .. } => Some(pass),
                _ => None,
            })
            .collect();
        assert_eq!(posts.len(), 2, "snapshot then blend");
        assert_eq!(posts[0].into, TargetId::Scratch(0));
        assert_eq!(posts[0].src, TargetId::Level(0));
        assert_eq!(posts[0].push.args[0], mode::COPY);
        assert_eq!(posts[1].into, TargetId::Level(0));
        assert_eq!(posts[1].src, TargetId::Level(1));
        assert_eq!(posts[1].aux, TargetId::Scratch(0));
        assert_eq!(posts[1].push.args[0], mode::BLEND);
        assert_eq!(posts[1].push.args[2], blend_index(BlendMode::Multiply));
    }

    #[test]
    fn a_blur_filter_ping_pongs_three_times_then_returns_home() {
        let layer = rect(0, 0, 100, 100);
        let plan = plan(
            vec![
                Step::PushLayer {
                    rect: layer,
                    backdrop: None,
                },
                Step::Draw {
                    first_run: 0,
                    run_count: 1,
                },
                Step::PopLayer {
                    rect: layer,
                    composite: Some(layer),
                    filter: Some(FilterOp {
                        box_radius: Some(2),
                        color_matrix: None,
                    }),
                    mask: None,
                    blend: BlendMode::Normal,
                    alpha: 1.0,
                },
            ],
            1,
        );
        let frame = frame_passes(&plan, W, H, CLEAR, None, None);
        let blurs: Vec<&PostPass> = frame
            .ops
            .iter()
            .filter_map(|op| match op {
                SceneOp::Post { pass, .. } if pass.push.args[0] != mode::OVER => Some(pass),
                _ => None,
            })
            .collect();
        assert_eq!(blurs.len(), 6, "three horizontal and three vertical");
        for (i, pass) in blurs.iter().enumerate() {
            let horizontal = i % 2 == 0;
            assert_eq!(
                pass.push.args[0],
                if horizontal { mode::BOX_H } else { mode::BOX_V }
            );
            assert_eq!(pass.push.args[1], 2.0);
            assert_eq!(pass.push.args[2], 1.0, "the duplicate edge clamp");
            assert_eq!(
                pass.into,
                if horizontal {
                    TargetId::Scratch(1)
                } else {
                    TargetId::Level(1)
                }
            );
        }
    }

    #[test]
    fn a_backdrop_filter_seeds_the_layer_with_the_parent() {
        let layer = rect(20, 20, 80, 80);
        let plan = plan(
            vec![
                Step::PushLayer {
                    rect: layer,
                    backdrop: Some(FilterOp {
                        box_radius: Some(1),
                        color_matrix: None,
                    }),
                },
                Step::Draw {
                    first_run: 0,
                    run_count: 1,
                },
                Step::PopLayer {
                    rect: layer,
                    composite: Some(layer),
                    filter: None,
                    mask: None,
                    blend: BlendMode::Normal,
                    alpha: 1.0,
                },
            ],
            1,
        );
        let frame = frame_passes(&plan, W, H, CLEAR, None, None);
        let SceneOp::Post { pass, .. } = &frame.ops[2] else {
            panic!(
                "the seed copy precedes the layer's own content: {:?}",
                frame.ops
            );
        };
        assert_eq!(pass.into, TargetId::Level(1));
        assert_eq!(pass.src, TargetId::Level(0));
        assert_eq!(pass.push.args[0], mode::COPY);
    }

    #[test]
    fn a_shadow_is_one_tint_pass_over_arrival_blurred_coverage() {
        let scissor = rect(0, 0, 40, 40);
        let patch = rect(4, 4, 36, 36);
        let plan = plan(
            vec![Step::Shadow {
                patch,
                scissor,
                coverage: MaskRef { offset: [7.0, 9.0] },
                color: [0.1, 0.2, 0.3, 0.5],
                clip: None,
            }],
            0,
        );
        let frame = frame_passes(&plan, W, H, CLEAR, None, None);
        let SceneOp::Post { pass, scissors, .. } = &frame.ops[1] else {
            panic!("a shadow is one post pass");
        };
        assert_eq!(pass.into, TargetId::Level(0));
        assert_eq!(pass.push.args[0], mode::SHADOW);
        assert!(pass.over);
        assert_eq!(pass.push.p[0], [7.0, 9.0, 1.0, 0.0]);
        assert_eq!(pass.push.p[2], [0.1, 0.2, 0.3, 0.5]);
        assert_eq!(scissors, &vec![scissor]);
        assert_eq!(pass.push.region, [4.0, 4.0, 36.0, 36.0]);
    }

    #[test]
    fn a_mask_multiplies_the_layer_through_the_scratch() {
        let layer = rect(0, 0, 60, 60);
        let plan = plan(
            vec![
                Step::PushLayer {
                    rect: layer,
                    backdrop: None,
                },
                Step::Draw {
                    first_run: 0,
                    run_count: 1,
                },
                Step::PopLayer {
                    rect: layer,
                    composite: Some(layer),
                    filter: None,
                    mask: Some(MaskRef { offset: [3.0, 5.0] }),
                    blend: BlendMode::Normal,
                    alpha: 1.0,
                },
            ],
            1,
        );
        let frame = frame_passes(&plan, W, H, CLEAR, None, None);
        let SceneOp::Post { pass, .. } = &frame.ops[3] else {
            panic!("the mask is a post pass before the composite");
        };
        assert_eq!(pass.push.args[0], mode::MASK);
        assert_eq!(pass.into, TargetId::Scratch(1));
        assert_eq!(pass.push.p[0], [3.0, 5.0, 1.0, 0.0]);
        // The composite then reads the masked scratch, not the layer.
        let SceneOp::Post { pass, .. } = &frame.ops[4] else {
            panic!("the composite follows");
        };
        assert_eq!(pass.src, TargetId::Scratch(1));
    }

    #[test]
    fn damage_keeps_the_previous_frame_and_clears_only_itself() {
        let plan = plan(
            vec![Step::Draw {
                first_run: 0,
                run_count: 1,
            }],
            0,
        );
        let retained = Some(Retained {
            size: (W, H),
            clear: CLEAR,
        });
        let damage = rect(10, 20, 30, 40);
        let frame = frame_passes(&plan, W, H, CLEAR, Some(&[damage]), retained);
        assert_eq!(frame.redrawn, Redrawn::Damage { pixels: 20 * 20 });
        assert_eq!(
            frame.ops[0],
            SceneOp::Begin {
                target: TargetId::Level(0),
                clear: None,
                kind: None,
            }
        );
        assert_eq!(
            frame.ops[1],
            SceneOp::ClearRegions {
                rects: vec![damage],
                color: [0.0, 0.0, 0.0, 1.0]
            }
        );
        // The draw is clipped to the damage.
        assert_eq!(
            frame.ops[2],
            SceneOp::DrawRun {
                target: TargetId::Level(0),
                first_index: 0,
                index_count: 3,
                scissor: damage
            }
        );
    }

    #[test]
    fn damage_on_a_retained_frame_of_another_shape_redraws_everything() {
        let plan = plan(
            vec![Step::Draw {
                first_run: 0,
                run_count: 1,
            }],
            0,
        );
        // A different clear colour: the previous frame cannot be kept.
        let retained = Some(Retained {
            size: (W, H),
            clear: Color::WHITE,
        });
        let frame = frame_passes(&plan, W, H, CLEAR, Some(&[rect(0, 0, 1, 1)]), retained);
        assert_eq!(frame.redrawn, Redrawn::Full);
        assert!(matches!(
            frame.ops[0],
            SceneOp::Begin { clear: Some(_), .. }
        ));
        assert!(!frame
            .ops
            .iter()
            .any(|op| matches!(op, SceneOp::ClearRegions { .. })));
    }

    #[test]
    fn a_top_level_layer_outside_the_damage_is_skipped_whole() {
        let inside = rect(0, 0, 20, 20);
        let outside = rect(50, 50, 60, 60);
        let plan = plan(
            vec![
                Step::PushLayer {
                    rect: inside,
                    backdrop: None,
                },
                Step::Draw {
                    first_run: 0,
                    run_count: 1,
                },
                Step::PopLayer {
                    rect: inside,
                    composite: Some(inside),
                    filter: None,
                    mask: None,
                    blend: BlendMode::Normal,
                    alpha: 1.0,
                },
                Step::PushLayer {
                    rect: outside,
                    backdrop: None,
                },
                Step::Draw {
                    first_run: 0,
                    run_count: 1,
                },
                Step::PopLayer {
                    rect: outside,
                    composite: Some(outside),
                    filter: None,
                    mask: None,
                    blend: BlendMode::Normal,
                    alpha: 1.0,
                },
            ],
            1,
        );
        let retained = Some(Retained {
            size: (W, H),
            clear: CLEAR,
        });
        let frame = frame_passes(&plan, W, H, CLEAR, Some(&[inside]), retained);
        // One layer begin: the untouched layer contributed nothing.
        assert_eq!(
            begins(&frame.ops),
            vec![TargetId::Level(0), TargetId::Level(1)],
            "the second layer is skipped whole"
        );
        // And nothing that writes the frame does so outside the damage:
        // depth-0 draws and composites into `L[0]` stay inside it. Draws
        // *inside* a kept layer are deliberately unclipped — the layer's
        // raster is reusable whole; it is the composite that is held to the
        // damage.
        for op in &frame.ops {
            let frame_level = match op {
                SceneOp::DrawRun {
                    target, scissor, ..
                } if *target == TargetId::Level(0) => Some(*scissor),
                SceneOp::Post { pass, scissors, .. }
                    if matches!(pass.into, TargetId::Level(0) | TargetId::Scratch(0)) =>
                {
                    scissors.first().copied()
                }
                _ => None,
            };
            if let Some(scissor) = frame_level {
                assert_eq!(scissor.intersect(inside), scissor);
            }
        }
    }

    #[test]
    fn a_backdrop_filter_reaching_past_the_damage_forces_a_full_frame() {
        let layer = rect(10, 10, 90, 90);
        let plan = plan(
            vec![Step::PushLayer {
                rect: layer,
                backdrop: Some(FilterOp {
                    box_radius: Some(1),
                    color_matrix: None,
                }),
            }],
            1,
        );
        let retained = Some(Retained {
            size: (W, H),
            clear: CLEAR,
        });
        let damage = rect(20, 20, 30, 30);
        let frame = frame_passes(&plan, W, H, CLEAR, Some(&[damage]), retained);
        assert_eq!(frame.redrawn, Redrawn::Full);
    }

    #[test]
    fn overlapping_damage_is_disjointed_before_it_is_cleared() {
        let plan = plan(vec![], 0);
        let retained = Some(Retained {
            size: (W, H),
            clear: CLEAR,
        });
        let a = rect(0, 0, 30, 30);
        let b = rect(10, 10, 40, 40);
        let frame = frame_passes(&plan, W, H, CLEAR, Some(&[a, b]), retained);
        let SceneOp::ClearRegions { rects, .. } = &frame.ops[1] else {
            panic!("the damage is cleared");
        };
        // Disjoint: the overlap is drawn once, not twice.
        for (i, x) in rects.iter().enumerate() {
            for y in &rects[i + 1..] {
                assert!(x.intersect(*y).is_empty(), "{x:?} overlaps {y:?}");
            }
        }
        let total: u64 = rects
            .iter()
            .map(|r| u64::from(r.width()) * u64::from(r.height()))
            .sum();
        let expected = 30 * 30 + 30 * 30 - 20 * 20;
        assert_eq!(total, expected);
    }

    #[test]
    fn a_layer_with_no_composite_draws_nothing_outside_itself() {
        // `composite: None` — the layer is pushed for isolation but its ink
        // never reaches the parent (the planner emits this for a layer whose
        // content is fully clipped away).
        let layer = rect(0, 0, 40, 40);
        let plan = plan(
            vec![
                Step::PushLayer {
                    rect: layer,
                    backdrop: None,
                },
                Step::Draw {
                    first_run: 0,
                    run_count: 1,
                },
                Step::PopLayer {
                    rect: layer,
                    composite: None,
                    filter: None,
                    mask: None,
                    blend: BlendMode::Normal,
                    alpha: 1.0,
                },
            ],
            1,
        );
        let frame = frame_passes(&plan, W, H, CLEAR, None, None);
        assert!(
            !frame
                .ops
                .iter()
                .any(|op| matches!(op, SceneOp::Post { .. })),
            "no composite, no post passes"
        );
    }

    #[test]
    fn a_draw_step_with_no_geometry_emits_nothing() {
        let mut empty = plan(
            vec![Step::Draw {
                first_run: 0,
                run_count: 1,
            }],
            0,
        );
        empty.vertices.clear();
        empty.indices.clear();
        let frame = frame_passes(&empty, W, H, CLEAR, None, None);
        assert_eq!(frame.ops.len(), 1, "only the frame's own begin");
    }

    #[test]
    fn every_blend_mode_has_the_index_its_shader_case_expects() {
        assert_eq!(blend_index(BlendMode::Normal), 0.0);
        assert_eq!(blend_index(BlendMode::Luminosity), 27.0);
        // The full range: 28 modes, no gaps, no repeats.
        let modes = [
            BlendMode::Normal,
            BlendMode::Clear,
            BlendMode::Src,
            BlendMode::Dst,
            BlendMode::DstOver,
            BlendMode::SrcIn,
            BlendMode::DstIn,
            BlendMode::SrcOut,
            BlendMode::DstOut,
            BlendMode::SrcAtop,
            BlendMode::DstAtop,
            BlendMode::Xor,
            BlendMode::Plus,
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Overlay,
            BlendMode::Darken,
            BlendMode::Lighten,
            BlendMode::ColorDodge,
            BlendMode::ColorBurn,
            BlendMode::HardLight,
            BlendMode::SoftLight,
            BlendMode::Difference,
            BlendMode::Exclusion,
            BlendMode::Hue,
            BlendMode::Saturation,
            BlendMode::Color,
            BlendMode::Luminosity,
        ];
        let indices: Vec<f32> = modes.iter().map(|m| blend_index(*m)).collect();
        for (i, index) in indices.iter().enumerate() {
            assert_eq!(*index, i as f32);
        }
    }
}
