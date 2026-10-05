//! master — this film's two-pass harness: the census (pass 1) and the
//! master render (pass 2), plus the single-scene preview and the layout
//! measure.
//!
//! **The v5 rig.** A frame is four draw lists composed in one order:
//!
//! 1. **ground** — the scene's room, full-bleed, screen space. Header,
//!    body and footer are this one surface.
//! 2. **world** — the scene's content, fitted uniformly into
//!    [`frame::BODY`] (see [`frame::fit`]). In the studio act the world is
//!    the *real application* — whole, or as plates of its own draw list —
//!    with the scene's overlay on top.
//! 3. **chrome** — the centred header, the footer's receipts and the rail,
//!    screen space, never scaled.
//!
//! There is no camera any more, and no join throws a frame across the
//! screen. What a cut does instead is the **frame dissolve**: the
//! previous scene's final frame and the new scene's live frame are
//! blended as one A→B cross-dissolve — no dip to dark between them, no
//! wipes, no sliding lines, no rectangles. The outgoing scene holds its
//! brightness to its last frame (there is no tail fade); the entrance
//! *is* the dissolve, run over `CUT_IN` and widened at the movement
//! boundaries, where acts change like chapters and scenes like
//! sentences. Where the incoming world is a carded window — the studio
//! act — the window grows out of the previous scene's content box while
//! its corners curve in: the rectangle becoming the curved rectangle,
//! the contents scaling within it as the frames dissolve around it.
//!
//! **Pass 1 · census.** Walks every frame, applies the session, counts the
//! command stream — including the real studio's own draw commands — and
//! raster-samples each scene for the frame-time receipt. **Pass 2 ·
//! master.** Streams RGBA into ffmpeg per scene; sixteen strided frames
//! tile into the scene's contact sheet; the segments concat into
//! `studio_film.mp4`.

use std::collections::HashMap;
use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::process::{Command as ProcCommand, Stdio};
use std::time::{Duration, Instant};

use vieww_foundation::{BlendMode, Color, Offset, Rect, Shadow, Transform};
use vieww_paint::native::NativeRenderer;
use vieww_paint::{Canvas, Command, Scene, Stroke};
use vieww_render::FrameDriver;
use vieww_widget::prelude::*;
use viewwstudio::state::Studio;

use crate::film_lib::{clamp01, ease_out_cubic};
use crate::product_film as pf;
use crate::product_film::script;
use crate::product_film::{Ctx, Kind, Probe, SceneDef};
use super::frame::{self, Plate};
use super::{frames_of, ladder_at, scenes, scene_start, total_frames, FPS};

/// Where this film's artifacts live while being built. `STUDIO_FILM_OUT`
/// overrides it — the portable door.
pub fn work_root() -> PathBuf {
    std::env::var_os("STUDIO_FILM_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/home/z/my-project/download/studio_film"))
}

fn manifest_path() -> PathBuf {
    work_root().join("manifest.txt")
}

/// The census's raster samples, one `scene,commands,ms` row each.
pub fn samples_path() -> PathBuf {
    work_root().join("samples.txt")
}

/// The census's raster samples as (draw commands, milliseconds) — empty
/// until a census has run. Read once per process.
pub fn frame_samples() -> &'static [(f32, f32)] {
    static S: std::sync::OnceLock<Vec<(f32, f32)>> = std::sync::OnceLock::new();
    S.get_or_init(|| {
        std::fs::read_to_string(samples_path())
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let mut it = l.split(',');
                let _id = it.next()?;
                let cmds: f32 = it.next()?.trim().parse().ok()?;
                let ms: f32 = it.next()?.trim().parse().ok()?;
                Some((cmds, ms))
            })
            .collect()
    })
}

/// The device-resolution multiplier in force for this render — see
/// [`super::SCALE_FACTOR`]. The `SCALE_FACTOR` environment variable
/// overrides the constant for a one-off pass.
pub fn scale_factor() -> f32 {
    std::env::var("SCALE_FACTOR")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|f| *f > 0.0 && *f <= 8.0)
        .unwrap_or(super::SCALE_FACTOR)
}

fn raster_size() -> (u32, u32) {
    let s = scale_factor();
    ((pf::W * s) as u32, (pf::H * s) as u32)
}

/// Entry point — dispatch on the CLI word.
pub fn run(mode: &str) -> Result<(), Box<dyn std::error::Error>> {
    match mode {
        "censussf" => census(),
        "mastersf" => master(),
        "sfprobe" => super::probe::run(),
        "scoresf" => score_and_mux(&work_root().join("studio_film.mp4")),
        other if other.starts_with("sfmeasure") => measure(other.trim_start_matches("sfmeasure").trim_start_matches(':')),
        other => {
            let name = other.trim_start_matches("sf:").to_ascii_lowercase();
            preview(&name)
        }
    }
}

// ── The cut ─────────────────────────────────────────────────────────────────

/// The entrance dissolve, seconds — widened ×1.5 at the movement
/// boundaries by [`cut_len`] (the acts change like chapters, the scenes
/// inside them like sentences).
pub const CUT_IN: f32 = 0.45;

/// How long a movement's label fades at its edges, seconds. No scene
/// fades out at its tail any more — the next scene's dissolve is the
/// cut, so the outgoing frame holds to full brightness.
pub const CUT_OUT: f32 = 0.35;

// ── The dissolve grammar ────────────────────────────────────────────────────

/// A cut in progress — the previous scene dissolving into this one. The
/// dissolve itself is a pixel blend in [`finish_frame`] (the A of the
/// A→B is the previous scene's final frame, persisted as PNG so a render
/// can resume across processes); what lives here is the *geometry*:
/// where the incoming window grows from.
#[derive(Clone, Copy)]
struct Cut {
    /// The previous scene's fitted content box, screen space — the
    /// rectangle a carded window grows out of.
    prev_box: Rect,
    /// The previous scene's window radius — a studio window's 14 at
    /// scale, or 0 for a pure scene's sharp rectangle.
    prev_radius: f32,
    /// The dissolve's eased progress this frame: 0 = the previous frame
    /// whole, 1 = the new frame whole.
    e: f32,
    /// Whether the window morph runs. It is for *entering* a carded
    /// world — a studio window growing out of a pure scene's content
    /// box. Studio→studio cuts morph every incoming plate out of the
    /// predecessor's whole-window box, which reads as the new scene's
    /// panes exploding out of a frame the audience already left — and,
    /// with the pixel blend running on top, as stray content from the
    /// scene before (Z16's device list ghosting through Z17's panel).
    /// Inside the act the dissolve carries the cut alone.
    morph: bool,
}

/// How long scene `si`'s entrance dissolve runs — `CUT_IN`, widened at
/// the movement boundaries: the acts change like chapters, the scenes
/// inside them like sentences.
fn cut_len(si: usize) -> f32 {
    let all = scenes();
    let boundary = si == 0 || super::movement_of(all[si - 1].id) != super::movement_of(all[si].id);
    CUT_IN * if boundary { 1.5 } else { 1.0 }
}

/// The previous scene's fitted content box, screen space — where an
/// incoming carded window begins. Its corners are square: a pure
/// scene's world is a rectangle.
fn prev_window(si: usize) -> Rect {
    let all = scenes();
    let content = frame::content_box(all[si - 1].id);
    let (fs, off) = frame::fit(content, super::layout::MAX_SCALE);
    Transform::new(fs, 0.0, 0.0, fs, off.dx, off.dy).apply_rect(content)
}

/// The previous scene's window radius — 14 at studio scale for a studio
/// scene, 0 for a pure one.
fn prev_radius(si: usize) -> f32 {
    let all = scenes();
    if all[si - 1].kind == Kind::Studio {
        let (fs, _) = frame::fit(frame::content_box(all[si - 1].id), super::layout::MAX_SCALE);
        14.0 * fs
    } else {
        0.0
    }
}

/// The linear interpolation of two rectangles — the window morph's
/// arithmetic.
fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    Rect::new(
        a.left + (b.left - a.left) * t,
        a.top + (b.top - a.top) * t,
        a.right + (b.right - a.right) * t,
        a.bottom + (b.bottom - a.bottom) * t,
    )
}

/// The frame's alpha at `sec` of a scene `seconds` long. Only the cold
/// open fades up from the dark; every later cut is the A→B dissolve
/// ([`Cut`]), so the frame itself never dips — the outgoing scene holds
/// its brightness to its last frame, and the entrance is the dissolve.
fn envelope(index: usize, sec: f32) -> f32 {
    if index == 0 && sec < CUT_IN {
        ease_out_cubic(clamp01(sec / CUT_IN))
    } else {
        1.0
    }
}

/// The act label's opacity: it only fades at a movement's edges, so
/// inside a movement it holds while the scenes dissolve under it.
fn act_alpha(index: usize, seconds: f32, sec: f32, a: f32) -> f32 {
    let list = scenes();
    let m = super::movement_of(list[index].id);
    let first = index == 0 || super::movement_of(list[index - 1].id) != m;
    let last = index + 1 < list.len() && super::movement_of(list[index + 1].id) != m;
    let mut out = 1.0f32;
    if first {
        out = out.min(a.max(clamp01((sec - 0.1) / 0.5)));
        out = out.min(clamp01((sec - 0.1) / 0.5));
    }
    if last && seconds - sec < CUT_OUT {
        out = out.min(a);
    }
    out
}

// ── The rig ─────────────────────────────────────────────────────────────────

struct Rig {
    /// The world: a pure scene's tree, or the real studio's Shell.
    driver: FrameDriver,
    /// The studio act's overlay (and any `frame::over` node).
    over_driver: FrameDriver,
    /// The room.
    ground_driver: FrameDriver,
    /// World-space nodes drawn under the plates (`frame::under`).
    under_driver: FrameDriver,
    has_under: bool,
    /// The header, the footer, the rail.
    chrome_driver: FrameDriver,
    studio: Option<Studio>,
    cursor: usize,
    /// Named snapshots of the studio's draw list, for plates that quote a
    /// state other than the live one (see [`super::script::snapshots`]).
    snaps: HashMap<&'static str, Scene>,
    snaps_for: Option<&'static str>,
    // This frame's composition.
    kind: Kind,
    plates: Vec<Plate>,
    has_over: bool,
    alpha: f32,
    world: Transform,
    /// The cut in force while this scene dissolves in (None once whole).
    cut: Option<Cut>,
    /// The previous scene's final frame, RGBA at raster size — the A of
    /// the A→B dissolve. Set by the master once per scene; absent in the
    /// census and in previews without a prior master run.
    prev_px: Option<Vec<u8>>,
}

impl Rig {
    fn new() -> Rig {
        let mk = || {
            let mut d = FrameDriver::new(pf::CANVAS);
            d.set_fonts(pf::fonts());
            d
        };
        Rig {
            driver: mk(),
            over_driver: mk(),
            ground_driver: mk(),
            under_driver: mk(),
            has_under: false,
            chrome_driver: mk(),
            studio: None,
            cursor: 0,
            snaps: HashMap::new(),
            snaps_for: None,
            kind: Kind::Pure,
            plates: Vec::new(),
            has_over: false,
            alpha: 1.0,
            world: Transform::IDENTITY,
            cut: None,
            prev_px: None,
        }
    }

    /// Load the previous scene's final frame for the entrance dissolve —
    /// the A of the A→B blend. `None` when there is nothing to dissolve
    /// from (the cold open, the seamless hold, a fresh census).
    fn set_prev_frame(&mut self, px: Option<Vec<u8>>) {
        self.prev_px = px;
    }

    fn ensure_studio(&mut self) {
        if self.studio.is_none() {
            let studio = script::mount(&mut self.driver);
            // The preview's phone before anything is rendered into it —
            // the freshly mounted studio's own state. Z14 quotes it.
            for k in 0..3 {
                self.driver.set_root(shell_only(&studio));
                self.driver.draw_frame_at(Duration::from_secs_f64(0.01 * k as f64));
            }
            self.snaps.insert("blank", self.driver.scene().clone());
            self.studio = Some(studio);
        }
    }

    /// Take the snapshots a studio scene quotes, once, on entering it. The
    /// live state is restored afterwards, so the session carries on as if
    /// the snapshots had never been taken.
    fn take_snapshots(&mut self, id: &'static str, abs: f32) {
        if self.snaps_for == Some(id) {
            return;
        }
        // Snapshots are kept across scenes (keys are unique film-wide), so
        // a later scene can quote a state an earlier one captured.
        self.snaps_for = Some(id);
        let plan = super::script::snapshots(id);
        if plan.is_empty() {
            return;
        }
        let studio = self.studio.clone().expect("studio mounted");
        // The live state the snapshots must not disturb, restored exactly
        // after each one (whatever the scene's session has done so far).
        let saved = (
            studio.right_tab.get(),
            studio.platform.get(),
            studio.preview_dark.get(),
            studio.device.get(),
            studio.active_buffer.get(),
        );
        for snap in plan {
            studio.right_tab.set(viewwstudio::state::RightTab::Preview);
            for a in &snap.apply {
                super::script::apply_one(&mut self.driver, &studio, a, abs);
            }
            for k in 0..4 {
                self.driver.set_root(shell_only(&studio));
                self.driver.draw_frame_at(Duration::from_secs_f64(abs as f64 + 0.01 * k as f64));
            }
            self.snaps.insert(snap.key, self.driver.scene().clone());
            let _ = &snap.restore;
            studio.right_tab.set(saved.0);
            studio.platform.set(saved.1);
            studio.preview_dark.set(saved.2);
            studio.device.set(saved.3);
            studio.active_buffer.set(saved.4);
        }
        self.driver.set_root(shell_only(&studio));
        self.driver.draw_frame_at(Duration::from_secs_f64(abs as f64));
    }

    /// Build one frame.
    fn build_frame(&mut self, s: &SceneDef, si: usize, start_abs: f32, i: usize, probe: &Probe) {
        let sec = i as f32 / FPS;
        let t = (sec / s.seconds).min(1.0);
        let abs = start_abs + sec;
        let a = envelope(si, sec);

        frame::reset();
        pf::clear_chrome();
        let ctx = Ctx { t, sec, abs, ladder: ladder_at(abs), probe, cam: pf::Cam::STILL };
        let tree = (s.build)(&ctx);
        self.kind = s.kind;

        match s.kind {
            Kind::Pure => {
                self.driver.set_root(tree);
                self.driver.draw_frame_at(Duration::from_secs_f64(abs.max(0.0) as f64));
                let over = frame::take_over();
                self.has_over = !over.is_empty();
                if self.has_over {
                    let mut st = Stack::new();
                    for n in over {
                        st = st.push(Positioned::fill().child(n));
                    }
                    let root: WidgetNode = st.into();
                    self.over_driver.set_root(root);
                    self.over_driver.draw_frame_at(Duration::from_secs_f64(abs.max(0.0) as f64));
                }
            }
            Kind::Studio => {
                self.ensure_studio();
                {
                    let Rig { driver, studio, cursor, .. } = self;
                    super::script::apply_session_up_to(driver, studio.as_mut().unwrap(), abs, cursor);
                }
                self.take_snapshots(s.id, abs);
                let studio = self.studio.clone().unwrap();
                // The studio's own per-frame hooks, in the order its window
                // loop runs them (`apps/viewwstudio/src/main.rs`, steps 5–6):
                // read the render tree for the Inspector and the damage and
                // semantics for the overlays, off the frame just drawn. Each
                // is an early return while its view is closed, so scenes that
                // open none of them are untouched.
                studio.capture_tree(&self.driver);
                studio.capture_overlays(&mut self.driver);
                self.driver.set_root(shell_only(&studio));
                self.driver.draw_frame_at(Duration::from_secs_f64(abs.max(0.0) as f64));
                let mut st = Stack::new().push(Positioned::fill().child(tree));
                for n in frame::take_over() {
                    st = st.push(Positioned::fill().child(n));
                }
                self.has_over = true;
                let root: WidgetNode = st.into();
                self.over_driver.set_root(root);
                self.over_driver.draw_frame_at(Duration::from_secs_f64(abs.max(0.0) as f64));
            }
        }
        let unders = frame::take_under();
        self.has_under = !unders.is_empty();
        if self.has_under {
            let mut st = Stack::new();
            for n in unders {
                st = st.push(Positioned::fill().child(n));
            }
            let root: WidgetNode = st.into();
            self.under_driver.set_root(root);
            self.under_driver.draw_frame_at(Duration::from_secs_f64(abs.max(0.0) as f64));
        }
        self.plates = frame::take_plates();
        if s.kind == Kind::Studio && self.plates.is_empty() {
            self.plates.push(Plate {
                src: super::layout::APP,
                dst: super::layout::APP,
                alpha: 1.0,
                radius: 14.0,
                snap: None,
                card: true,
                bare: false,
                morph: None, stretch: false,
            });
        }

        // The fit — one uniform transform, no settle pop: the entrance is
        // the dissolve, not a zoom.
        let content = frame::take_boxed().unwrap_or_else(|| frame::content_box(s.id));
        let (fs, off) = frame::fit(content, super::layout::MAX_SCALE);
        self.world = Transform::new(fs, 0.0, 0.0, fs, off.dx, off.dy);
        self.alpha = a;

        // The cut: every scene after the cold open dissolves out of its
        // predecessor's final frame — the pixel blend in `finish_frame`
        // carries the A→B, and a carded world grows out of the previous
        // scene's content box while it runs (see `compose`) — but only
        // when the previous scene was not itself a carded world: inside
        // the studio act the dissolve alone carries the cut. The hold is
        // seamless: it continues the end card's picture without a cut.
        let seamless = s.id == "Z22";
        let len = cut_len(si);
        let morph = si > 0 && scenes()[si - 1].kind != Kind::Studio;
        self.cut = (si > 0 && !seamless && sec < len).then(|| Cut {
            prev_box: prev_window(si),
            prev_radius: prev_radius(si),
            e: ease_out_cubic(clamp01(sec / len)),
            morph,
        });

        // The ground.
        let grounds = frame::take_ground();
        let ground_root: WidgetNode = if grounds.is_empty() {
            frame::default_ground(t)
        } else {
            let mut st = Stack::new();
            for n in grounds {
                st = st.push(Positioned::fill().child(n));
            }
            st.into()
        };
        self.ground_driver.set_root(ground_root);
        self.ground_driver.draw_frame_at(Duration::from_secs_f64(abs.max(0.0) as f64));

        // The chrome: header and receipts ride the dissolve; the act label
        // holds inside a movement; the rail never fades.
        let act_a = act_alpha(si, s.seconds, sec, a);
        let header = frame::header(super::movement_of(s.id), super::movement_name(s.id), act_a);
        let chrome = Stack::new()
            .push(Positioned::fill().child(Opacity::new(a).child(
                Stack::new()
                    .push(Positioned::fill().child(strip_act(header, a, act_a)))
                    .push(Positioned::fill().child(frame::footer_receipts()))
                    .push(Positioned::fill().child(pf::take_chrome())),
            )))
            .push(frame::rail(abs));
        let root: WidgetNode = chrome.into();
        self.chrome_driver.set_root(root);
        self.chrome_driver.draw_frame_at(Duration::from_secs_f64(abs.max(0.0) as f64));
    }
}

/// The header carries the act label at its own opacity; the whole header
/// then rides the dissolve. Dividing the label back out keeps the label
/// steady across a cut inside one movement.
fn strip_act(header: WidgetNode, _a: f32, _act_a: f32) -> WidgetNode {
    header
}

/// The real studio's Shell, alone — the film's overlays are drawn by the
/// overlay driver so plates can quote the application without them.
fn shell_only(studio: &Studio) -> WidgetNode {
    script::shell_root(studio, Stack::new().into())
}

/// Count the command stream into the probe — the census's yardstick.
fn count_scene_commands(commands: &[Command], probe: &mut Probe) {
    for c in commands {
        match c {
            Command::FillRect { .. } => probe.shapes += 1,
            Command::FillPath { .. } => probe.shapes += 1,
            Command::StrokePath { .. } => {
                probe.shapes += 1;
                probe.strokes += 1;
            }
            Command::DrawGlyphs { run, .. } => {
                probe.glyph_runs += 1;
                probe.glyphs += run.glyphs.len() as u64;
            }
            Command::DrawShadow { .. } => {
                probe.shapes += 1;
                probe.shadows += 1;
            }
            Command::DrawImage { .. } => {}
            Command::PushLayer { filter, .. } => {
                probe.layers += 1;
                if !filter.is_noop() {
                    probe.filtered += 1;
                }
            }
            Command::PopLayer => {}
        }
    }
}

/// The studio's window colour — a plate's backdrop.
fn window_color() -> Color {
    viewwstudio::StudioTheme::dark().window
}

/// Compose the frame's draw lists into one scene.
fn compose(rig: &Rig) -> Scene {
    let mut frame_scene = Scene::default();
    let a = rig.alpha;
    let w = rig.world;
    let full = Rect::new(0.0, 0.0, pf::W, pf::H);

    // 1 · ground — carries the frame's alpha (the cold open's fade-up; a
    // later cut's change of light rides the pixel dissolve instead).
    group(&mut frame_scene, full, a, |f| f.append(rig.ground_driver.scene(), Transform::IDENTITY));

    // 2 · world — composed into a staging scene first so the whole world
    // takes the frame's opacity at once (the cold open's fade-up; every
    // later cut is the pixel dissolve, not a fade).
    let mut world = Scene::default();
    {
        let f = &mut world;
        if rig.kind == Kind::Pure {
            f.append(rig.driver.scene(), w);
        }
        if rig.has_under {
            f.append(rig.under_driver.scene(), w);
        }
        for p in &rig.plates {
            let src_scene = match p.snap {
                Some(key) => rig.snaps.get(key).unwrap_or_else(|| rig.driver.scene()),
                None => rig.driver.scene(),
            };
            // The window, screen space — and, while a cut dissolves, the
            // morph: a carded world begins as the previous scene's
            // content box (a sharp rectangle) and grows into its own
            // curved window, corners easing in, contents scaling within.
            // Bare plates never morph — they are placed, not windowed.
            // A bare plate is a device, and a device is its own frame:
            // the plate's `src` is the pane the device sits in, so the
            // stage behind it — its fill, its zoom readout, its chip —
            // would ride along as a box around the box. The studio
            // records the device's silhouette as the shadow its body
            // casts; the plate crops to exactly that rounded rectangle
            // and keeps the pane's own mapping, so a tablet still lands
            // bigger than a phone on the same stage.
            let k = p.dst.width() / p.src.width().max(1.0);
            let place = |sil: Rect| Rect::new(
                p.dst.left + (sil.left - p.src.left) * k,
                p.dst.top + (sil.top - p.src.top) * k,
                p.dst.left + (sil.right - p.src.left) * k,
                p.dst.top + (sil.bottom - p.src.top) * k,
            );
            let (p_src, p_dst, p_radius) = if p.bare {
                match device_silhouette(src_scene, p.src) {
                    Some((sil, cr)) => {
                        let mut d = place(sil);
                        let mut cr = cr * k;
                        // The reshape: this device's outline travels toward
                        // the other snapshot's by `m`.
                        if let Some((other, m)) = p.morph {
                            let o = rig.snaps.get(other).and_then(|sc| device_silhouette(sc, p.src));
                            if let Some((osil, ocr)) = o {
                                d = lerp_rect(d, place(osil), m);
                                cr += (ocr * k - cr) * m;
                            }
                        }
                        (sil, d, cr)
                    }
                    None => (p.src, p.dst, p.radius),
                }
            } else {
                (p.src, p.dst, p.radius)
            };
            let mut dst = w.apply_rect(p_dst);
            let mut r = p_radius * w.a;
            if let Some(cut) = &rig.cut {
                if p.card && cut.morph && cut.e < 0.999 {
                    dst = lerp_rect(cut.prev_box, dst, cut.e);
                    r = cut.prev_radius + (r - cut.prev_radius) * cut.e;
                }
            }
            // Uniform everywhere but mid-reshape, where the contents ride
            // the outline's own aspect for the half-second it changes.
            let sx = dst.width() / p_src.width().max(1.0);
            let sy = if p.morph.is_some() || p.stretch { dst.height() / p_src.height().max(1.0) } else { sx };
            let t = Transform::new(sx, 0.0, 0.0, sy, dst.left - p_src.left * sx, dst.top - p_src.top * sy);
            if p.alpha <= 0.004 {
                continue;
            }
            f.push_layer(dst.inflate(60.0), p.alpha, BlendMode::Normal);
            if p.card {
                f.draw_shadow(
                    Rect::new(dst.left, dst.top + 8.0, dst.right, dst.bottom + 8.0),
                    r,
                    Shadow::new(pf::alpha(Color::BLACK, 0.55), Offset::new(0.0, 14.0), 40.0),
                );
            }
            // A bare plate places its pixels directly — no backdrop, no
            // rim: the device's own silhouette on the scene's ground.
            if !p.bare {
                f.fill_rrect(dst, r, window_color().into());
            }
            f.append(&round_clip(src_scene, t, dst, r, p.bare), Transform::IDENTITY);
            if p.card {
                f.stroke_rrect(dst, r, Stroke::new(1.0), pf::alpha(Color::WHITE, 0.10).into());
            }
            f.pop_layer();
        }
        if rig.has_over {
            f.append(rig.over_driver.scene(), w);
        }
    }
    // The world rides the frame's own alpha — 1 everywhere but the cold
    // open's fade-up. At a cut the world does not fade (that would dip
    // the picture); the dissolve is the pixel blend in `finish_frame`,
    // and the window morph above is the entrance.
    let wa = rig.alpha;
    if wa >= 0.999 {
        frame_scene.append(&world, Transform::IDENTITY);
    } else if wa > 0.004 {
        frame_scene.push_layer(full, wa, BlendMode::Normal);
        frame_scene.append(&world, Transform::IDENTITY);
        frame_scene.pop_layer();
    }

    // 3 · chrome.
    frame_scene.append(rig.chrome_driver.scene(), Transform::IDENTITY);
    frame_scene
}

/// The device inside `within` — the largest rounded silhouette the studio
/// casts a shadow for there (the preview's device body records its own
/// outline as a `DrawShadow` caster), with its corner, in the app's
/// 1920×1080 space. `None` when the region holds no device.
fn device_silhouette(scene: &Scene, within: Rect) -> Option<(Rect, f32)> {
    let mut best: Option<(Rect, f32)> = None;
    for c in scene.commands() {
        if let Command::DrawShadow { rect, radius, transform, .. } = c {
            let r = transform.apply_rect(*rect);
            let inside = r.left >= within.left - 1.0
                && r.top >= within.top - 1.0
                && r.right <= within.right + 1.0
                && r.bottom <= within.bottom + 1.0;
            let big = r.width() * r.height() > within.width() * within.height() * 0.12;
            let not_pane = r.width() < within.width() * 0.995 || r.height() < within.height() * 0.995;
            if inside && big && not_pane && *radius >= 4.0 {
                let cr = *radius * transform.a.abs().max(1e-3);
                if best.map_or(true, |(b, _)| r.width() * r.height() > b.width() * b.height()) {
                    best = Some((r, cr));
                }
            }
        }
    }
    best
}

/// `src` through `t`, confined to the rounded rectangle `dst` — the plate's
/// own corners, so a quoted pane reads as a card rather than a crop.
fn round_clip(src: &Scene, t: Transform, dst: Rect, r: f32, strict: bool) -> Scene {
    let mut out = Scene::default();
    let path = vieww_foundation::Path::rounded_rect(dst, r.max(0.0));
    let slack = 3.0 * t.a.abs().max(0.25);
    for c in src.commands() {
        let mut c = c.transformed(t);
        // A device plate takes the device and nothing that merely
        // overlaps it: the pane's fill, the readout and the chip the
        // stage floats over the bezel all reach past the silhouette, and
        // a clip would leave their slivers on the glass.
        if strict && !matches!(c, Command::PushLayer { .. } | Command::PopLayer) {
            // What the command can actually touch: its reach, through
            // its own clip (a scrolled list inside the screen reaches far
            // past the glass but is clipped to it).
            let mut b = c.bounds();
            if let Some(cb) = c.clip().bounds() {
                b = Rect::new(b.left.max(cb.left), b.top.max(cb.top), b.right.min(cb.right), b.bottom.min(cb.bottom));
            }
            if b.left < dst.left - slack || b.top < dst.top - slack || b.right > dst.right + slack || b.bottom > dst.bottom + slack {
                continue;
            }
        }
        // A shadow cast by something outside the plate would still blur
        // into it; the plate quotes a region, not its neighbours' shadows.
        if let Command::DrawShadow { rect, transform, .. } = &c {
            let r = transform.apply_rect(*rect);
            if r.right < dst.left || r.left > dst.right || r.bottom < dst.top || r.top > dst.bottom
                || r.left < dst.left - 2.0 || r.right > dst.right + 2.0 || r.top < dst.top - 2.0 || r.bottom > dst.bottom + 2.0
            {
                continue;
            }
        }
        match &mut c {
            Command::FillRect { clip, .. }
            | Command::FillPath { clip, .. }
            | Command::StrokePath { clip, .. }
            | Command::DrawShadow { clip, .. }
            | Command::DrawGlyphs { clip, .. }
            | Command::DrawImage { clip, .. }
            | Command::PushLayer { clip, .. } => {
                clip.add_rect(dst);
                if r > 0.5 {
                    clip.add_path(path.clone());
                }
            }
            Command::PopLayer => {}
        }
        out.push_command(c);
    }
    out
}

/// Draw `body` as one group at opacity `a` (no group when opaque).
fn group(f: &mut Scene, bounds: Rect, a: f32, body: impl FnOnce(&mut Scene)) {
    if a >= 0.999 {
        body(f);
    } else if a > 0.004 {
        f.push_layer(bounds, a, BlendMode::Normal);
        body(f);
        f.pop_layer();
    }
}

fn clear_color() -> Color {
    Color::rgb(11, 10, 14)
}

/// Raster the composed frame — then, while a cut dissolves, blend it with
/// the previous scene's final frame: the A→B dissolve itself, one blend
/// carrying both directions. No dip to dark, no wipe: the outgoing frame
/// fades out exactly as the incoming frame fades in, at eased weight
/// `cut.e`. The blend is raster-side on purpose — the previous frame
/// exists as pixels (persisted per scene as PNG, so a render can resume
/// across processes), and pixels are what a dissolve dissolves.
fn finish_frame(rig: &Rig, renderer: &mut NativeRenderer) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let (rw, rh) = raster_size();
    let s = scale_factor();
    let scene = compose(rig);
    let scaled = if (s - 1.0).abs() < 1.0e-4 { scene } else { scene.scaled(s) };
    let (pixels, _report) = renderer.render_to_pixels(&scaled, rw, rh, clear_color())?;
    let mut px = pixels.data().to_vec();
    if let Some(cut) = &rig.cut {
        if cut.e < 0.999 {
            if let Some(prev) = rig.prev_px.as_ref() {
                // A size mismatch (a `SCALE_FACTOR` change between runs)
                // simply forgoes the blend for that scene.
                if prev.len() == px.len() {
                    // Integer lerp, 8.8 fixed point — fast enough to run on
                    // every frame of the dissolve window.
                    let w_new = (cut.e * 256.0).round() as u16;
                    let w_old = 256 - w_new;
                    for i in (0..px.len()).step_by(4) {
                        px[i] = ((prev[i] as u16 * w_old + px[i] as u16 * w_new) >> 8) as u8;
                        px[i + 1] = ((prev[i + 1] as u16 * w_old + px[i + 1] as u16 * w_new) >> 8) as u8;
                        px[i + 2] = ((prev[i + 2] as u16 * w_old + px[i + 2] as u16 * w_new) >> 8) as u8;
                    }
                }
            }
        }
    }
    Ok(px)
}

// ── Pass 1 — the census ─────────────────────────────────────────────────────

/// The crate receipt's gate: the film's named list and the workspace's
/// own manifest must be the same set, and every crate the film calls new
/// must exist. A film that shows a stale crate count is refused here,
/// before a single frame is rendered.
fn check_crates() -> Result<(), Box<dyn std::error::Error>> {
    let ws = super::workspace_crates();
    let mut a: Vec<&str> = ws.iter().map(|c| c.trim_start_matches("vieww-")).collect();
    let mut b: Vec<&str> = pf::CRATES.iter().map(|c| c.trim_start_matches("vieww-")).collect();
    a.sort_unstable();
    b.sort_unstable();
    if a != b {
        return Err(format!("the film's crate list has drifted from vieww_base/Cargo.toml: manifest {a:?} vs film {b:?}").into());
    }
    for n in super::NEW_CRATES {
        if !ws.contains(&n) {
            return Err(format!("NEW_CRATES names {n}, which the workspace does not have").into());
        }
    }
    println!("  crates · {} in the manifest, {} new — the film's list agrees", ws.len(), super::NEW_CRATES.len());
    Ok(())
}

pub fn census() -> Result<(), Box<dyn std::error::Error>> {
    let all = scenes();
    let n_total = total_frames();
    println!(
        "studio_film census — {} scenes · {} frames (derived, never typed) · SCALE_FACTOR {}",
        all.len(),
        n_total,
        scale_factor()
    );
    std::fs::create_dir_all(work_root())?;
    check_crates()?;

    let mut rig = Rig::new();
    let mut renderer = NativeRenderer::new();
    renderer.guard_filtered_layers_below(32);

    let mut probe = Probe::default();
    let mut frame_ms_samples: Vec<f32> = Vec::new();
    // Every raster sample as (scene, draw commands, milliseconds) — the
    // data behind the "what's new" scene's scatter chart.
    let mut sample_rows: Vec<String> = Vec::new();
    let mut alive_samples: Vec<f64> = Vec::new();
    let mut counted_frames = 0u64;

    for (si, s) in all.iter().enumerate() {
        let n = frames_of(s);
        let start_abs = scene_start(si);
        let sample_every = (n / 12).max(1);
        let t0 = Instant::now();

        for i in 0..n {
            // The alive probe — the live-edit scene: one stopwatch around
            // the whole real pipeline, session apply to pixels.
            let alive = s.id == super::ALIVE_SCENE && {
                let frac = i as f32 / n as f32;
                (0.30..0.60).contains(&frac) && i % 4 == 0
            };
            if alive {
                let t_alive = Instant::now();
                rig.build_frame(s, si, start_abs, i, &probe);
                let px = finish_frame(&rig, &mut renderer)?;
                std::hint::black_box(&px);
                alive_samples.push(t_alive.elapsed().as_secs_f64() * 1000.0);
            } else {
                rig.build_frame(s, si, start_abs, i, &probe);
            }

            count_scene_commands(compose(&rig).commands(), &mut probe);

            if i % sample_every == 0 {
                let t_ms = Instant::now();
                rig.build_frame(s, si, start_abs, i, &probe);
                let px = finish_frame(&rig, &mut renderer)?;
                std::hint::black_box(&px);
                let ms = (t_ms.elapsed().as_secs_f64() * 1000.0) as f32;
                frame_ms_samples.push(ms);
                sample_rows.push(format!("{},{},{:.2}", s.id, compose(&rig).commands().len(), ms));
            }
            counted_frames += 1;
        }
        println!("  {} {} · {} frames · census {:.1}s", s.id, s.name, n, t0.elapsed().as_secs_f32());
    }

    frame_ms_samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median_ms = frame_ms_samples.get(frame_ms_samples.len() / 2).copied().unwrap_or(0.0);
    probe.frames = counted_frames;
    probe.frame_ms = median_ms;

    let mut alive_sorted = alive_samples.clone();
    alive_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let alive_median = alive_sorted.get(alive_sorted.len() / 2).copied().unwrap_or(0.0);
    probe.alive_seconds = (alive_median / 1000.0) as f32;
    probe.bench = Probe::bench_identity(renderer.cached_fonts() as u32);

    let body = format!(
        "frames={}\nshapes={}\nglyph_runs={}\nglyphs={}\nlayers={}\nfiltered_layers={}\nstrokes={}\nshadows={}\nalive_seconds={:.3}\nframe_ms={:.2}\nscale_factor={}\nfps={}\nbench={}\n",
        probe.frames,
        probe.shapes,
        probe.glyph_runs,
        probe.glyphs,
        probe.layers,
        probe.filtered,
        probe.strokes,
        probe.shadows,
        probe.alive_seconds,
        probe.frame_ms,
        scale_factor(),
        FPS,
        probe.bench,
    );
    std::fs::write(manifest_path(), body)?;
    std::fs::write(samples_path(), sample_rows.join("\n") + "\n")?;
    println!(
        "\ncensus complete — {} frames · {} raster samples · median {:.2} ms",
        counted_frames,
        frame_ms_samples.len(),
        median_ms
    );
    println!(
        "  shapes {} · glyph runs {} · layers {} (filtered {}) · strokes {}",
        probe.shapes, probe.glyph_runs, probe.layers, probe.filtered, probe.strokes
    );
    println!("  alive median {:.3} s · bench: {}", probe.alive_seconds, probe.bench);
    println!("  manifest → {}", manifest_path().display());
    Ok(())
}

// ── Pass 2 — the master ─────────────────────────────────────────────────────

pub fn master() -> Result<(), Box<dyn std::error::Error>> {
    let root = work_root();
    let seg_dir = root.join("segments");
    std::fs::create_dir_all(root.join("sheets"))?;
    std::fs::create_dir_all(root.join("cut"))?;
    std::fs::create_dir_all(&seg_dir)?;

    let manifest = manifest_path();
    let probe = match Probe::load(&manifest) {
        Some(p) => p,
        None => {
            println!("no manifest — running the census first");
            census()?;
            Probe::load(&manifest).ok_or("census produced no manifest")?
        }
    };

    // `SF_ONLY=Z11,Z12` renders just those scenes (resume-friendly).
    let only: Option<Vec<String>> = std::env::var("SF_ONLY")
        .ok()
        .map(|v| v.split(',').map(|s| s.trim().to_ascii_uppercase()).collect());

    let all = scenes();
    let (rw, rh) = raster_size();
    let mut renderer = NativeRenderer::new();
    renderer.guard_filtered_layers_below(32);
    let mut rig = Rig::new();
    let t0 = Instant::now();
    let mut done_sheets = 0usize;

    for (si, s) in all.iter().enumerate() {
        let n = frames_of(s);
        let start_abs = scene_start(si);
        let seg = seg_dir.join(format!("{:02}_{}.mp4", si + 1, s.name));
        let sheet = root.join("sheets").join(format!("{}_{}.png", s.id, s.name));

        let wanted = only.as_ref().is_none_or(|o| o.iter().any(|x| x == s.id));
        if !wanted || (only.is_none() && seg.exists() && sheet.exists()) {
            println!("  {} {} · skipping", s.id, s.name);
            done_sheets += 1;
            continue;
        }

        // Entering the studio act mid-run: replay the session from the top
        // (no raster) so the state at `start_abs` is the linear run's.
        if s.kind == Kind::Studio && rig.studio.is_none() {
            println!("  entering the studio act at {} — mounting the real app", s.id);
            rig.ensure_studio();
            let mut warm = 0usize;
            super::script::apply_session_up_to(&mut rig.driver, rig.studio.as_mut().unwrap(), start_abs, &mut warm);
            rig.cursor = warm;
            let studio = rig.studio.clone().unwrap();
            rig.driver.set_root(shell_only(&studio));
            rig.driver.draw_frame_at(Duration::from_secs_f64(start_abs as f64));
        }

        // The A of this scene's entrance dissolve: the previous scene's
        // final frame, as this run or an earlier one left it (the cut/
        // directory is what lets a render resume across processes — see
        // `finish_frame`). The hold has none: it continues the end card
        // seamlessly.
        if si > 0 && s.id != "Z22" {
            let prev = &all[si - 1];
            let path = root.join("cut").join(format!("{:02}_{}.png", si, prev.name));
            let px = std::fs::read(&path)
                .ok()
                .and_then(|b| image::load_from_memory(&b).ok())
                .map(|img| img.to_rgba8().into_raw());
            if px.is_none() {
                println!("  {} {} · no predecessor frame — plain entrance", s.id, s.name);
            }
            rig.set_prev_frame(px);
        } else {
            rig.set_prev_frame(None);
        }

        let scene_dir = root.join("frames").join(format!("{:02}_{}", si + 1, s.name));
        std::fs::remove_dir_all(&scene_dir).ok();
        std::fs::create_dir_all(&scene_dir)?;
        let scene_t0 = Instant::now();

        let mut ffmpeg = ProcCommand::new("ffmpeg")
            .args([
                "-y", "-loglevel", "error",
                "-f", "rawvideo", "-pix_fmt", "rgba",
                "-s", &format!("{rw}x{rh}"), "-framerate", &format!("{FPS}"),
                "-i", "-",
                "-c:v", "libx264", "-preset", "medium", "-crf", "17",
                "-pix_fmt", "yuv420p",
                // `aq-mode=3` (auto-variance AQ with dark bias) is the
                // encoder-side half of the no-radial-glow decision: an
                // 8-bit yuv420 H.264 quantises a soft luminance ramp on a
                // near-black ground into visible steps — the "glitch" the
                // review saw on the old glows. The renderer's side of the
                // fix is that the film no longer draws them (its gradients
                // are linear, dithered, and mostly crisp-edged); this makes
                // the encoder spend its bits where the dark ramps still
                // are, so the floors and vignettes stay smooth too.
                "-x264-params", "aq-mode=3",
            ])
            .arg(&seg)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let mut stdin = ffmpeg.stdin.take().ok_or("no ffmpeg stdin")?;

        let picks = sheet_picks(n);
        let mut last_buf: Option<Vec<u8>> = None;
        for i in 0..n {
            rig.build_frame(s, si, start_abs, i, &probe);
            let buf = finish_frame(&rig, &mut renderer)?;
            stdin.write_all(&buf)?;
            if let Some(k) = picks.iter().position(|p| *p == i) {
                let img = image::RgbaImage::from_raw(rw, rh, buf.clone()).ok_or("invalid frame")?;
                img.save(scene_dir.join(format!("frame_{k:03}.png")))?;
            }
            if i + 1 == n {
                // The A of the *next* scene's dissolve — this scene's final
                // frame, held at full brightness (there is no tail fade).
                last_buf = Some(buf);
            }
            if i % 300 == 0 && i > 0 {
                println!("  {} {} · {:>3}% · {:.1} min in", s.id, s.name, i * 100 / n, t0.elapsed().as_secs_f32() / 60.0);
            }
        }
        drop(stdin);
        let status = ffmpeg.wait()?;
        if !status.success() {
            return Err(format!("ffmpeg exited {status} on {}", s.id).into());
        }
        if let Some(buf) = last_buf {
            if si + 1 < all.len() && all[si + 1].id != "Z22" {
                let img = image::RgbaImage::from_raw(rw, rh, buf).ok_or("invalid frame")?;
                img.save(root.join("cut").join(format!("{:02}_{}.png", si + 1, s.name)))?;
            }
        }
        tile_sheet(&scene_dir, &sheet)?;
        println!("  {} {} · done in {:.1}s", s.id, s.name, scene_t0.elapsed().as_secs_f32());
        let _ = std::fs::remove_dir_all(&scene_dir);
        done_sheets += 1;
    }

    // Assemble — concat the segments (stream copy).
    let list = seg_dir.join("list.txt");
    let mut body = String::new();
    for (si, s) in all.iter().enumerate() {
        body.push_str(&format!("file '{}/segments/{:02}_{}.mp4'\n", root.display(), si + 1, s.name));
    }
    std::fs::write(&list, body)?;
    let mp4 = root.join("studio_film.mp4");
    let status = ProcCommand::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "concat", "-safe", "0"])
        .arg("-i")
        .arg(&list)
        .args(["-c", "copy", "-movflags", "+faststart"])
        .arg(&mp4)
        .status()?;
    if !status.success() {
        return Err("concat failed".into());
    }
    score_and_mux(&mp4)?;
    println!(
        "\nmaster assembled — {} segments · {:.1} min wall · {} sheets · {} frames",
        all.len(),
        t0.elapsed().as_secs_f32() / 60.0,
        done_sheets,
        total_frames()
    );
    println!("  mp4 → {}", mp4.display());
    Ok(())
}

/// Synthesise the score (`super::score`, vieww-audio end to end) and lay
/// it under the assembled picture: the video stream is copied untouched,
/// the WAV is encoded to AAC beside it.
fn score_and_mux(mp4: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let t0 = Instant::now();
    let wav = super::score::write(&work_root())?;
    println!("  score → {} ({:.1}s, vieww-audio)", wav.display(), t0.elapsed().as_secs_f32());
    if !mp4.exists() {
        return Ok(());
    }
    let tmp = mp4.with_extension("scored.mp4");
    let status = ProcCommand::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(mp4)
        .arg("-i")
        .arg(&wav)
        .args(["-map", "0:v:0", "-map", "1:a:0", "-c:v", "copy", "-c:a", "aac", "-b:a", "192k", "-shortest", "-movflags", "+faststart"])
        .arg(&tmp)
        .status()?;
    if !status.success() {
        return Err("mux failed".into());
    }
    std::fs::rename(&tmp, mp4)?;
    println!("  scored → {}", mp4.display());
    Ok(())
}

/// The sixteen frames a sheet shows: evenly through the scene, skipping
/// the dissolves at either end — the head past the *widest* entrance
/// dissolve (a movement boundary's 1.5× `CUT_IN`) — so every tile is a
/// settled picture.
fn sheet_picks(n: usize) -> Vec<usize> {
    let lo = ((CUT_IN * 1.5 + 0.15) * FPS) as usize;
    let hi = n.saturating_sub(((CUT_OUT + 0.05) * FPS) as usize + 1).max(lo + 16);
    (0..16).map(|k| lo + k * (hi - lo) / 15).map(|i| i.min(n - 1)).collect()
}

/// Tile `frame_000..015.png` into the house 4×4 contact sheet.
fn tile_sheet(dir: &std::path::Path, sheet: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let status = ProcCommand::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-framerate", "10"])
        .arg("-i")
        .arg(dir.join("frame_%03d.png"))
        .args(["-vf", "scale=960:-1:flags=lanczos,tile=4x4:padding=8:color=0x0B0A0E", "-frames:v", "1"])
        .arg(sheet)
        .status()?;
    if !status.success() {
        return Err("sheet failed".into());
    }
    Ok(())
}

// ── The preview — one scene, sixteen frames, one sheet ─────────────────────

fn find_scene(name: &str) -> Result<(usize, SceneDef), Box<dyn std::error::Error>> {
    let all = scenes();
    all.into_iter()
        .enumerate()
        .find(|(_, s)| {
            s.id.eq_ignore_ascii_case(name) || s.name.to_ascii_lowercase().contains(&name.to_ascii_lowercase())
        })
        .map(|(si, s)| (si, s))
        .ok_or_else(|| -> Box<dyn std::error::Error> { format!("no scene matches {name:?}").into() })
}

fn preview(name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (si, s) = find_scene(name)?;
    let probe = Probe::load(&manifest_path()).unwrap_or_default();
    let root = work_root().join("preview").join(s.id);
    std::fs::create_dir_all(&root)?;
    let (rw, rh) = raster_size();
    let n = frames_of(&s);
    let start_abs = scene_start(si);

    let mut renderer = NativeRenderer::new();
    renderer.guard_filtered_layers_below(32);
    let mut rig = Rig::new();
    // The A of the entrance dissolve, when a master run has already left
    // one — the preview then shows the true cut, not just the scene.
    if si > 0 && s.id != "Z22" {
        let prev = &scenes()[si - 1];
        let path = work_root().join("cut").join(format!("{:02}_{}.png", si, prev.name));
        let px = std::fs::read(&path)
            .ok()
            .and_then(|b| image::load_from_memory(&b).ok())
            .map(|img| img.to_rgba8().into_raw());
        rig.set_prev_frame(px);
    }
    if s.kind == Kind::Studio {
        rig.ensure_studio();
        let mut warm = 0usize;
        super::script::apply_session_up_to(&mut rig.driver, rig.studio.as_mut().unwrap(), start_abs, &mut warm);
        rig.cursor = warm;
    }

    // `SF_FRAMES=a,b,c` previews those frame indices instead of the sheet's.
    let picks: Vec<usize> = std::env::var("SF_FRAMES")
        .ok()
        .map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect())
        .unwrap_or_else(|| sheet_picks(n));
    for (k, i) in picks.iter().enumerate() {
        let t0 = Instant::now();
        // Studio scenes must walk their session in order; step through the
        // frames between picks without rastering them.
        rig.build_frame(&s, si, start_abs, *i, &probe);
        let buf = finish_frame(&rig, &mut renderer)?;
        println!("  {} frame {i:>4} ({:.0} ms)", s.id, t0.elapsed().as_secs_f64() * 1000.0);
        let img = image::RgbaImage::from_raw(rw, rh, buf).ok_or("invalid frame")?;
        img.save(root.join(format!("frame_{k:03}.png")))?;
    }
    if picks.len() == 16 {
        tile_sheet(&root, &root.join("sheet.png"))?;
    }
    println!("preview → {}", root.display());
    Ok(())
}

// ── The measure — each scene's ink, for `layout.rs` ────────────────────────

fn measure(which: &str) -> Result<(), Box<dyn std::error::Error>> {
    let all = scenes();
    let probe = Probe::load(&manifest_path()).unwrap_or_default();
    let mut renderer = NativeRenderer::new();
    let mut rig = Rig::new();
    for (si, s) in all.iter().enumerate() {
        if s.kind == Kind::Studio {
            continue;
        }
        if !which.is_empty() && !s.id.eq_ignore_ascii_case(which) {
            continue;
        }
        let n = frames_of(s);
        let start_abs = scene_start(si);
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
        for k in 0..48 {
            let i = (CUT_IN * FPS) as usize + k * (n - (CUT_IN * FPS) as usize - (CUT_OUT * FPS) as usize - 1) / 47;
            rig.build_frame(s, si, start_abs, i, &probe);
            let mut sc = Scene::default();
            sc.append(rig.driver.scene(), Transform::IDENTITY);
            if rig.has_over {
                sc.append(rig.over_driver.scene(), Transform::IDENTITY);
            }
            let (px, _) = renderer.render_to_pixels(&sc, pf::W as u32, pf::H as u32, Color::BLACK)?;
            let d = px.data();
            for y in 0..pf::H as u32 {
                for x in 0..pf::W as u32 {
                    let o = ((y * pf::W as u32 + x) * 4) as usize;
                    if d[o].max(d[o + 1]).max(d[o + 2]) > 14 {
                        x0 = x0.min(x);
                        y0 = y0.min(y);
                        x1 = x1.max(x);
                        y1 = y1.max(y);
                    }
                }
            }
        }
        println!("{} {}: Rect {{ left: {x0}.0, top: {y0}.0, right: {x1}.0, bottom: {y1}.0 }}  ({}×{})", s.id, s.name, x1.saturating_sub(x0), y1.saturating_sub(y0));
    }
    Ok(())
}
