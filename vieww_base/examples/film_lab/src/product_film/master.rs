//! master — the film's two-pass harness: the census (pass 1) and the
//! master render (pass 2), plus the single-scene preview mode.
//!
//! **The rig.** One `FrameDriver` at 1920×1080 *logical* carries the whole
//! film. Pure scenes set their tree per frame. Studio scenes mount the
//! **actual `viewwstudio` app** — `script::mount` installs the real
//! element runtime, opens the real workspace, wires the real toolchain —
//! and the session script drives it, action by action, at fixed
//! film-times. The Shell is mounted inside a Stack with the scene's
//! overlay chrome on top; the studio state (buffers, preview, compile
//! history, the demo's route) lives in the studio's own signals, outside
//! every scene tree — which is why the session survives cuts.
//!
//! **SCALE_FACTOR.** The master renders the *same* logical frame at any
//! device resolution: `SCALE_FACTOR` (env, default 1) multiplies the
//! raster size, and the command list is scaled *after* compositing via
//! `Scene::scaled` — glyph outlines are scan-converted at device
//! resolution rather than magnified. `1` = 1920×1080 (the base),
//! `2` = 3840×2160 (4K), `4` = 7680×4320 (8K). The driver, the layout,
//! the script's taps and the studio session never see the scale: they
//! are all in logical points, so a 4K master and a 1080p master are the
//! *same film*, rasterised denser. The manifest records the factor it
//! was rendered at.
//!
//! **Pass 1 · census.** Walks every frame of the film, applies the
//! script, builds the tree, and counts the command stream it emits —
//! including the real studio's own draw commands. Raster samples at
//! full master resolution (12 per scene) give the frame-time receipt;
//! C03's live-edit window gives the **alive probe** — the measured
//! edit→pixels cost of one real keystroke through the real studio. The
//! number is the film's own distance receipt, quoted on the end card.
//! Compiles are settled off-clock (`Settle`), so the census's counts
//! and the master's pixels agree by construction.
//!
//! **Pass 2 · master.** Renders all frames at 60 fps and streams raw
//! RGBA straight into ffmpeg's stdin (`libx264 · crf 18 · yuv420p`),
//! per-scene segments, concat at the end. Sixteen strided frames per
//! scene tile into the house contact sheet (`fps=10, scale=480:-1,
//! tile=4x4`). The MP4 carries no audio — the cut works muted by
//! construction, and vieww has no sound to put on it.
//!
//! **Determinism.** The film re-renders to the byte on this bench: the
//! fixed clock, the frame-indexed script, the off-clock settles and the
//! embedded font store (plus the site's Geist, both in-tree) make a
//! frame a function of the checkout. The manifest names its bench.

use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::process::{Command as ProcCommand, Stdio};
use std::time::Duration;

use vieww_foundation::Size;
use vieww_paint::native::{NativeRenderer, SceneReport};
use vieww_paint::Command;
use vieww_render::FrameDriver;
use vieww_widget::WidgetNode;
use viewwstudio::state::Studio;

use super::script;
use super::{scenes, total_frames, Ctx, Kind, Probe, SceneDef, FPS, H, W};

/// Where the film's artifacts live while being built.
///
/// `PRODUCT_FILM_OUT` overrides it. The default was an absolute path into
/// one machine's home directory, which made the review loop
/// (`film_lab pf:<scene>`) unrunnable on any other checkout — the previews
/// were written somewhere that did not exist and the command failed before
/// it drew a frame. The env var is the portable door; the default stays so
/// the bench that rendered the master keeps rendering it to the same place.
pub(crate) fn work_root() -> PathBuf {
    std::env::var_os("PRODUCT_FILM_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/home/z/my-project/download/product_film"))
}

fn manifest_path() -> PathBuf {
    work_root().join("manifest.txt")
}

/// The device-resolution multiplier this run rasterises at — the
/// SCALE_FACTOR. Read from the environment once; `1` is the 1080p base,
/// `2` renders 4K, `4` renders 8K. Everything above the raster (layout,
/// script, session) stays in logical points.
pub(crate) fn scale_factor() -> f32 {
    std::env::var("SCALE_FACTOR")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|f| *f > 0.0 && *f <= 8.0)
        .unwrap_or(1.0)
}

/// The raster dimensions this run writes, in device pixels.
pub(crate) fn raster_size() -> (u32, u32) {
    let s = scale_factor();
    ((W * s) as u32, (H * s) as u32)
}

/// Entry point — dispatch on the CLI word.
pub(crate) fn run(mode: &str) -> Result<(), Box<dyn std::error::Error>> {
    match mode {
        "censuspf" => census(),
        "masterpf" => master(),
        "pfcal" => calibrate(),
        other => {
            let name = other.trim_start_matches("pf:").to_ascii_lowercase();
            preview_scene(&name)
        }
    }
}

// ── The calibration probe — verify the script's tap coordinates ────────────

/// Mount the studio, drive the session to each scripted tap, and
/// **hit-test the real tree** to find the coordinates the script should
/// carry — the same discipline as the lab's `kzcal`: the committed
/// constants are this probe's receipts, never guesses.
///
/// 1. The live demo's first tappable row: drive to just before C04's
///    tap, then scan the preview region with real `PointerEvent`s until
///    one changes `live_state.route` — that position is the row.
/// 2. The compiled counter's *Add one*: drive to the settled compile,
///    rasterise once, find the accent-filled button by its own pixels,
///    tap its centre, and verify the phone's pixels changed (the
///    count's label is inside that region).
fn calibrate() -> Result<(), Box<dyn std::error::Error>> {
    let out = work_root().join("calibrate");
    std::fs::create_dir_all(&out)?;
    let mut rig = Rig::new();
    let mut renderer = NativeRenderer::new();

    // ── 1 · the live row — scan for the route-changing tap ──────────
    // Drive to just before C04's tap (abs 166.0), with the script fully
    // applied to that moment.
    let studio = rig.studio().clone();
    let names: Vec<String> = studio
        .buffers
        .get()
        .iter()
        .map(|b| b.name.clone())
        .collect();
    println!("BUFFERS = {:?}", names);
    script::apply_up_to(&mut rig.driver, &studio, 166.0, &mut rig.cursor);
    let empty_probe = Probe::default();
    let scene = &scenes()[12]; // C04 · the_tap
    let start_abs = super::scene_start(12);
    let _ctx = rig.build_frame(scene, start_abs, 120, &empty_probe);
    let before = studio.live_state.route.get();
    println!("route before the scan: {:?}", before);

    let mut found_row: Option<(f32, f32)> = None;
    let mut clock = 166.6f64;
    'scan: for y in (240..620).step_by(14) {
        for x in (1450..1850).step_by(18) {
            clock += 0.05;
            let at = vieww_foundation::Offset::new(x as f32, y as f32);
            rig.driver
                .handle_pointer(&vieww_foundation::PointerEvent::down(
                    vieww_foundation::PointerId(1),
                    at,
                    Duration::from_secs_f64(clock),
                ));
            rig.driver.draw_frame_at(Duration::from_secs_f64(clock));
            rig.driver
                .handle_pointer(&vieww_foundation::PointerEvent::up(
                    vieww_foundation::PointerId(1),
                    at,
                    Duration::from_secs_f64(clock + 0.02),
                ));
            rig.driver
                .draw_frame_at(Duration::from_secs_f64(clock + 0.03));
            if studio.live_state.route.get() != before {
                found_row = Some((x as f32, y as f32));
                // Reset the route so later scans start clean.
                studio.live_state.route.set(before);
                break 'scan;
            }
        }
    }
    println!("TAP_LIVE_ROW = {:?}", found_row);
    let (png, _) =
        renderer.render_to_png(rig.driver.scene(), W as u32, H as u32, rig.background)?;
    std::fs::write(out.join("01-live-home.png"), png)?;

    // ── 2 · the compiled counter's Add one — pixel-hunt the button ──
    // Drive to the settled compile (abs 177.4, just after Settle).
    script::apply_up_to(&mut rig.driver, &studio, 177.4, &mut rig.cursor);
    rig.driver.draw_frame_at(Duration::from_secs_f64(177.4));
    let (pixels, _) = raster(&mut renderer, &rig.driver, rig.background)?;
    let (rw, rh) = raster_size();

    // The accent-filled button: scan the right half for the studio's
    // filled-button blue — distinctly blue, brighter than the chrome.
    let data = pixels.data();
    let at_px = |x: u32, y: u32| {
        let i = ((y * rw + x) * 4) as usize;
        (data[i] as i32, data[i + 1] as i32, data[i + 2] as i32)
    };
    let mut xs: Vec<u32> = Vec::new();
    let mut ys: Vec<u32> = Vec::new();
    let x0 = (rw as f32 * 0.55) as u32;
    for y in ((rh as f32 * 0.18) as u32..(rh as f32 * 0.85) as u32).step_by(2) {
        for x in (x0..rw - 4).step_by(2) {
            let (r, g, b) = at_px(x, y);
            if b > 180 && b - r > 60 && b - g > 60 && r < 160 {
                xs.push(x);
                ys.push(y);
            }
        }
    }
    let button: Option<(f32, f32)> = if xs.is_empty() {
        None
    } else {
        xs.sort();
        ys.sort();
        Some((xs[xs.len() / 2] as f32, ys[ys.len() / 2] as f32))
    };
    println!("ADD_ONE_BUTTON (pixel-hunted, logical) = {:?}", button);

    // Snapshot before the tap.
    let (png, _) =
        renderer.render_to_png(rig.driver.scene(), W as u32, H as u32, rig.background)?;
    std::fs::write(out.join("02-counter-before.png"), png)?;

    // Tap the hunted centre and verify the phone's pixels changed.
    if let Some((bx, by)) = button {
        let at = vieww_foundation::Offset::new(bx, by);
        rig.driver
            .handle_pointer(&vieww_foundation::PointerEvent::down(
                vieww_foundation::PointerId(1),
                at,
                Duration::from_secs_f64(178.5),
            ));
        rig.driver.draw_frame_at(Duration::from_secs_f64(178.5));
        rig.driver
            .handle_pointer(&vieww_foundation::PointerEvent::up(
                vieww_foundation::PointerId(1),
                at,
                Duration::from_secs_f64(178.52),
            ));
        rig.driver.draw_frame_at(Duration::from_secs_f64(178.6));
        let (after, _) = raster(&mut renderer, &rig.driver, rig.background)?;
        // Compare the phone's region: a band around the button.
        let mut changed = 0usize;
        let data2 = after.data();
        let (lx, rx) = (
            (bx - 160.0).max(0.0) as u32,
            (bx + 160.0).min(rw as f32 - 1.0) as u32,
        );
        let (ly, ry) = (
            (by - 120.0).max(0.0) as u32,
            (by + 120.0).min(rh as f32 - 1.0) as u32,
        );
        for y in (ly..ry).step_by(3) {
            for x in (lx..rx).step_by(3) {
                let i = ((y * rw + x) * 4) as usize;
                for k in 0..3 {
                    if (data[i + k] as i32 - data2[i + k] as i32).abs() > 24 {
                        changed += 1;
                        break;
                    }
                }
            }
        }
        println!(
            "tap at {:?} → {} changed pixel samples in the phone band {}",
            at,
            changed,
            changed > 40
        );
        let (png, _) =
            renderer.render_to_png(rig.driver.scene(), W as u32, H as u32, rig.background)?;
        std::fs::write(out.join("03-counter-after.png"), png)?;
    }

    // ── 3 · the dense-frame drive — the master's own path, verified ──
    // A fresh rig, driven frame-by-frame exactly as the master drives
    // them (script → set_root → draw), through both taps. This is the
    // proof that the atomic-pair rule in `apply_up_to` holds: the route
    // changes after C04's tap, and the count's label changes after
    // C05's.
    //
    // The first rig is dropped first: the studio's compile session
    // claims its seed for the process's lifetime, and a second mount on
    // the same seed would come up without a toolchain.
    drop(rig);
    drop(studio);
    {
        let mut rig = Rig::new();
        let studio = rig.studio().clone();
        let empty_probe = Probe::default();
        // C04 · the_tap (index 12): drive all 480 frames.
        let scene = &scenes()[12];
        let start_abs = super::scene_start(12);
        let n = scene.frames();
        for i in 0..n {
            let _ = rig.build_frame(scene, start_abs, i, &empty_probe);
        }
        println!(
            "dense C04 drive · route after the tap: {:?} (was {:?})",
            studio.live_state.route.get(),
            before
        );
        let (png, _) =
            renderer.render_to_png(rig.driver.scene(), W as u32, H as u32, rig.background)?;
        std::fs::write(out.join("04-dense-c04.png"), png)?;

        // C05 · say_to_rust (index 13): drive all 960 frames, through
        // the compile and the Add one tap.
        let scene = &scenes()[13];
        let start_abs = super::scene_start(13);
        let n = scene.frames();
        for i in 0..n {
            let _ = rig.build_frame(scene, start_abs, i, &empty_probe);
        }
        let (after, _) = raster(&mut renderer, &rig.driver, rig.background)?;
        // Compare against the pre-tap state: re-render the same frame's
        // tree is post-tap, so diff the label band around the button.
        let data2 = after.data();
        let bx = 1594.0f32;
        let by = 346.0f32;
        let (lx, rx) = (
            (bx - 170.0).max(0.0) as u32,
            (bx + 170.0).min(rw as f32 - 1.0) as u32,
        );
        let (ly, ry) = (
            (by - 110.0).max(0.0) as u32,
            (by + 40.0).min(rh as f32 - 1.0) as u32,
        );
        let mut changed = 0usize;
        for y in (ly..ry).step_by(2) {
            for x in (lx..rx).step_by(2) {
                let idx = ((y * rw + x) * 4) as usize;
                // Diff against the pre-tap snapshot (02), which had
                // count = 0.
                let pre = (data[idx] as i32, data[idx + 1] as i32, data[idx + 2] as i32);
                let post = (
                    data2[idx] as i32,
                    data2[idx + 1] as i32,
                    data2[idx + 2] as i32,
                );
                if (pre.0 - post.0).abs() > 30
                    || (pre.1 - post.1).abs() > 30
                    || (pre.2 - post.2).abs() > 30
                {
                    changed += 1;
                }
            }
        }
        println!(
            "dense C05 drive · label band changed samples vs pre-tap: {} ({} > 6)",
            changed,
            changed > 6
        );
        let (png, _) =
            renderer.render_to_png(rig.driver.scene(), W as u32, H as u32, rig.background)?;
        std::fs::write(out.join("05-dense-c05.png"), png)?;
    }
    println!("  receipts → {}", out.display());
    Ok(())
}

// ── The rig — one driver, the studio mounted when the film needs it ─────────

/// The film's whole render rig: the driver, the studio (mounted lazily,
/// once), and the script cursor.
struct Rig {
    driver: FrameDriver,
    /// A second driver, for the film's own chrome.
    ///
    /// The chrome cannot share the scene driver. Swapping that driver's
    /// root to draw the captions makes it repaint its background over the
    /// whole canvas, and appending *that* over the world buried the studio
    /// under a full-frame fill — the app came out at a third of its real
    /// luminance. A driver per space keeps each one's background its own.
    chrome_driver: FrameDriver,
    studio: Option<Studio>,
    cursor: usize,
    /// The background the current frame's scene asked for.
    background: vieww_foundation::Color,
}

impl Rig {
    fn new() -> Rig {
        let mut driver = FrameDriver::new(Size::new(W, H));
        driver.set_fonts(super::fonts());
        let mut chrome_driver = FrameDriver::new(Size::new(W, H));
        chrome_driver.set_fonts(super::fonts());
        Rig {
            driver,
            chrome_driver,
            studio: None,
            cursor: 0,
            background: super::GROUND,
        }
    }

    /// The studio, mounting it if this is the first studio frame of the
    /// film. Mounted once — the session survives every cut after that.
    fn studio(&mut self) -> &Studio {
        if self.studio.is_none() {
            self.studio = Some(script::mount(&mut self.driver));
        }
        self.studio.as_ref().expect("the studio, just mounted")
    }

    /// Build one frame of scene `s` at in-scene frame `i`: apply the
    /// script, mount (or keep) the studio, set the root, draw.
    fn build_frame<'a>(
        &mut self,
        s: &SceneDef,
        start_abs: f32,
        i: usize,
        probe: &'a Probe,
    ) -> Ctx<'a> {
        self.build_frame_shot(s, start_abs, i, probe, 1.0)
    }

    /// [`build_frame`](Self::build_frame), with the cut's luminance ramp
    /// applied.
    ///
    /// `alpha` rides two things at once. It fades the scene's own tree,
    /// and it blends the **background** toward the film's ground — which
    /// is the half that matters at the studio boundaries. The studio
    /// clears to `StudioTheme::dark().window` and the graphics acts clear
    /// to `GROUND`, so a hard cut between them moves mean frame luminance
    /// by twenty-two levels in a single frame no matter what the trees do.
    /// Ramping the clear colour with the tree turns that step into a
    /// third-of-a-second slope, and the two halves of the film start
    /// looking like one exposure.
    fn build_frame_shot<'a>(
        &mut self,
        s: &SceneDef,
        start_abs: f32,
        i: usize,
        probe: &'a Probe,
        alpha: f32,
    ) -> Ctx<'a> {
        self.build_frame_cam(s, start_abs, i, probe, alpha, super::Cam::STILL)
    }

    /// [`build_frame_shot`](Self::build_frame_shot), told where the camera
    /// is — so a scene can place things in depth against the move.
    fn build_frame_cam<'a>(
        &mut self,
        s: &SceneDef,
        start_abs: f32,
        i: usize,
        probe: &'a Probe,
        alpha: f32,
        cam: super::Cam,
    ) -> Ctx<'a> {
        let _n = s.frames();
        let sec = i as f32 / FPS;
        let t = (sec / s.seconds).min(1.0);
        let abs = start_abs + sec;
        let ctx = Ctx {
            t,
            sec,
            abs,
            ladder: super::ladder_at(abs),
            probe,
            cam,
        };

        // The script only exists in the studio act; applying it before
        // the studio exists would mount the app early — so only apply
        // while a scene is a studio scene (or the studio is already
        // mounted and actions are still pending).
        let a = alpha.clamp(0.0, 1.0);
        let ramping = a < 0.999;
        // Anything the previous frame registered and never drained.
        super::clear_chrome();
        match s.kind {
            Kind::Studio => {
                let _ = self.studio();
                script::apply_up_to(
                    &mut self.driver,
                    self.studio.as_ref().unwrap(),
                    abs,
                    &mut self.cursor,
                );
                let overlay: WidgetNode = (s.build)(&ctx);
                let mut root = script::shell_root(self.studio.as_ref().unwrap(), overlay);
                if ramping {
                    root = vieww_widget::Opacity::new(a).child(root).into();
                }
                let win = viewwstudio::StudioTheme::dark().window;
                self.background = if ramping {
                    crate::film_lib::mix(super::GROUND, win, a)
                } else {
                    win
                };
                self.driver.set_root(root);
            }
            Kind::Pure => {
                let mut tree: WidgetNode = (s.build)(&ctx);
                if ramping {
                    tree = vieww_widget::Opacity::new(a).child(tree).into();
                }
                self.background = super::GROUND;
                self.driver.set_root(tree);
            }
        }
        self.driver
            .draw_frame_at(Duration::from_secs_f64(abs as f64));
        ctx
    }

    /// Build a frame and split it into its two spaces: the world the
    /// camera looks at, and the screen-space chrome composited over it.
    ///
    /// Two layout passes, one raster. The chrome tree is a handful of
    /// text runs and a pill, so laying it out separately costs almost
    /// nothing next to the frame it rides on — and it is the only way the
    /// captions can stay put while the picture moves.
    fn build_split<'a>(
        &mut self,
        s: &SceneDef,
        start_abs: f32,
        i: usize,
        probe: &'a Probe,
        index: usize,
    ) -> (Ctx<'a>, vieww_paint::Scene, super::Shot) {
        // The shot is a function of the clock alone, so it is known
        // before the tree exists — which means one build pass, and a
        // scene that can read the camera it is being seen through.
        let sec = i as f32 / FPS;
        let t = (sec / s.seconds).min(1.0);
        let sh = super::shot(s.id, index, s.seconds, t, sec);
        let alpha = sh.alpha;
        super::clear_chrome();
        let ctx = self.build_frame_cam(s, start_abs, i, probe, alpha, sh.cam);
        let world = self.driver.scene().clone();
        let chrome_tree = super::take_chrome();
        let abs = start_abs + i as f32 / FPS;
        let a = alpha.clamp(0.0, 1.0);
        let root: WidgetNode = if a < 0.999 {
            vieww_widget::Opacity::new(a).child(chrome_tree).into()
        } else {
            chrome_tree
        };
        self.chrome_driver.set_root(root);
        self.chrome_driver
            .draw_frame_at(Duration::from_secs_f64(abs as f64));
        (ctx, world, sh)
    }
}

/// Count one frame's command stream into the probe.
fn count_scene_commands(scene_cmds: &[Command], p: &mut Probe) {
    for cmd in scene_cmds {
        match cmd {
            Command::FillRect { .. } => p.shapes += 1,
            Command::FillPath { .. } => p.shapes += 1,
            Command::StrokePath { .. } => {
                p.shapes += 1;
                p.strokes += 1;
            }
            Command::DrawGlyphs { run, .. } => {
                p.glyph_runs += 1;
                p.glyphs += run.glyphs.len() as u64;
            }
            Command::DrawShadow { .. } => {
                p.shapes += 1;
                p.shadows += 1;
            }
            Command::DrawImage { .. } => {}
            Command::PushLayer { filter, .. } => {
                p.layers += 1;
                if !filter.is_noop() {
                    p.filtered += 1;
                }
            }
            Command::PopLayer => {}
        }
    }
}

/// Rasterise the rig's current scene at this run's device resolution —
/// the SCALE_FACTOR seam, in one place: the logical command list is
/// scaled *after* compositing (`Scene::scaled`), so glyph outlines and
/// hairlines are scan-converted at device resolution, never magnified.
fn raster(
    renderer: &mut NativeRenderer,
    driver: &FrameDriver,
    background: vieww_foundation::Color,
) -> Result<(vieww_paint::native::Pixels, SceneReport), vieww_paint::native::RendererError> {
    raster_through(renderer, driver, background, super::Cam::STILL)
}

/// [`raster`], through a camera.
///
/// The camera rides the same seam `SCALE_FACTOR` does and for the same
/// reason: the frame is already a command list, so moving it is a
/// coordinate change on that list rather than a second paint pass.
/// `Scene::append` is documented for exactly this — lifting a recording
/// into another space without replaying paint — and it composes with
/// `Scene::scaled`, so a 4K master and a 1080p master remain the *same
/// film*, camera included.
///
/// Order matters: the camera is a **logical** move (it is authored in
/// 1920×1080 points, like the layout and the script's taps), so it applies
/// first and the device scale applies to the result. Doing it the other
/// way would make the camera's framing depend on the raster resolution.
///
/// A still camera costs nothing — the command list is handed to the
/// renderer untouched, which is what the whole studio act does.
fn raster_through(
    renderer: &mut NativeRenderer,
    driver: &FrameDriver,
    background: vieww_foundation::Color,
    cam: super::Cam,
) -> Result<(vieww_paint::native::Pixels, SceneReport), vieww_paint::native::RendererError> {
    let s = scale_factor();
    let (rw, rh) = raster_size();
    let scale_is_one = (s - 1.0).abs() < f32::EPSILON;

    match (cam.is_still(), scale_is_one) {
        (true, true) => renderer.render_to_pixels(driver.scene(), rw, rh, background),
        (true, false) => {
            let scaled = driver.scene().scaled(s);
            renderer.render_to_pixels(&scaled, rw, rh, background)
        }
        (false, true) => {
            let moved = through_camera(driver.scene(), cam);
            renderer.render_to_pixels(&moved, rw, rh, background)
        }
        (false, false) => {
            let moved = through_camera(driver.scene(), cam);
            let scaled = moved.scaled(s);
            renderer.render_to_pixels(&scaled, rw, rh, background)
        }
    }
}

/// Lift a frame's command list through the camera's transform.
fn through_camera(scene: &vieww_paint::Scene, cam: super::Cam) -> vieww_paint::Scene {
    let mut moved = vieww_paint::Scene::default();
    moved.append(scene, cam.transform());
    moved
}

/// Raster the two spaces as one frame: the world through the camera, the
/// film's own chrome over the top of it, untouched.
///
/// Both are already command lists, so composing them is two `append`s and
/// a single raster — the chrome does not cost a second pass over the
/// pixels, only a second (very small) layout.
fn raster_split(
    renderer: &mut NativeRenderer,
    world: &vieww_paint::Scene,
    chrome: &vieww_paint::Scene,
    background: vieww_foundation::Color,
    cam: super::Cam,
) -> Result<(vieww_paint::native::Pixels, SceneReport), vieww_paint::native::RendererError> {
    let mut frame = vieww_paint::Scene::default();
    frame.append(world, cam.transform());
    frame.append(chrome, vieww_foundation::Transform::IDENTITY);

    let s = scale_factor();
    let (rw, rh) = raster_size();
    if (s - 1.0).abs() < f32::EPSILON {
        renderer.render_to_pixels(&frame, rw, rh, background)
    } else {
        let scaled = frame.scaled(s);
        renderer.render_to_pixels(&scaled, rw, rh, background)
    }
}

// ── Pass 1 — the census ─────────────────────────────────────────────────────

pub(crate) fn census() -> Result<(), Box<dyn std::error::Error>> {
    let root = work_root();
    std::fs::create_dir_all(&root)?;

    let all = scenes();
    let total = total_frames();
    println!(
        "product_film census — {} scenes · {} frames (derived, never typed) · SCALE_FACTOR {}",
        all.len(),
        total,
        scale_factor()
    );

    let mut probe = Probe::default();
    probe.frames = total as u64;

    let mut rig = Rig::new();
    let mut renderer = NativeRenderer::new();
    renderer.guard_filtered_layers_below(32);

    // The raster samples: 12 per scene → the frame-time receipt.
    let mut frame_ms_samples: Vec<f64> = Vec::new();
    // The alive probe: C03's live-edit window — the measured cost of one
    // real keystroke, edit to pixels, at master resolution.
    let mut alive_samples: Vec<f64> = Vec::new();
    let empty_probe = Probe::default();

    let t0 = std::time::Instant::now();
    let mut counted_frames = 0usize;

    for (si, s) in all.iter().enumerate() {
        let start_abs: f32 = all.iter().take(si).map(|x| x.seconds).sum();
        let n = s.frames();
        let sample_every = (n / 12).max(1);
        for i in 0..n {
            // The alive window measures the whole edit→pixels pipeline:
            // script application (the keystroke itself), tree build,
            // draw and raster — one stopwatch around one real frame.
            let alive_window = s.id == "C03" && (0.20..0.45).contains(&(i as f32 / n as f32));
            let sample = alive_window || (i % sample_every == 0);

            if sample {
                // The census measures the frame the master will ship —
                // camera, chrome split and all — so the frame-time
                // receipt on the end card stays true to what was rendered.
                let t_start = std::time::Instant::now();
                let (_ctx, world, sh) = rig.build_split(s, start_abs, i, &empty_probe, si);
                let (pixels, _report) = raster_split(
                    &mut renderer,
                    &world,
                    rig.chrome_driver.scene(),
                    rig.background,
                    sh.cam,
                )?;
                let took = t_start.elapsed().as_secs_f64() * 1000.0;
                frame_ms_samples.push(took);
                if alive_window {
                    alive_samples.push(took);
                }
                let _ = pixels;
                // Both spaces. The frame is the world *and* the chrome
                // over it, so counting only `driver.scene()` — which
                // after the split holds the chrome alone — would report a
                // film made of captions.
                count_scene_commands(world.commands(), &mut probe);
                count_scene_commands(rig.chrome_driver.scene().commands(), &mut probe);
                counted_frames += 1;
            } else {
                let (_ctx, world, _sh) = rig.build_split(s, start_abs, i, &empty_probe, si);
                count_scene_commands(world.commands(), &mut probe);
                count_scene_commands(rig.chrome_driver.scene().commands(), &mut probe);
                counted_frames += 1;
            }
        }
        println!(
            "  {} {} · {} frames · census {:.1}s elapsed",
            s.id,
            s.name,
            n,
            t0.elapsed().as_secs_f32()
        );
    }

    // The medians — measured, never picked.
    let mut sorted = frame_ms_samples.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median_ms = sorted.get(sorted.len() / 2).copied().unwrap_or(0.0);
    probe.frame_ms = median_ms as f32;

    let mut alive_sorted = alive_samples.clone();
    alive_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let alive_median = alive_sorted
        .get(alive_sorted.len() / 2)
        .copied()
        .unwrap_or(0.0);
    probe.alive_seconds = (alive_median / 1000.0) as f32;

    probe.bench = Probe::bench_identity(renderer.cached_fonts() as u32);

    // The house-format manifest.
    let body = format!(
        "frames={}\nshapes={}\nglyph_runs={}\nglyphs={}\nlayers={}\nfiltered_layers={}\nstrokes={}\nshadows={}\nalive_seconds={:.3}\nframe_ms={:.2}\nscale_factor={}\nbench={}\n",
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
        probe.bench,
    );
    std::fs::write(manifest_path(), body)?;
    println!(
        "\ncensus complete — {} frames counted · {} raster samples · median {:.2} ms",
        counted_frames,
        frame_ms_samples.len(),
        median_ms
    );
    println!(
        "  shapes {} · glyph runs {} · layers {} (filtered {}) · strokes {}",
        probe.shapes, probe.glyph_runs, probe.layers, probe.filtered, probe.strokes
    );
    println!(
        "  alive N = {:.3} s · bench: {}",
        probe.alive_seconds, probe.bench
    );
    println!("  manifest → {}", manifest_path().display());
    Ok(())
}

// ── Pass 2 — the master ─────────────────────────────────────────────────────

pub(crate) fn master() -> Result<(), Box<dyn std::error::Error>> {
    let root = work_root();
    std::fs::create_dir_all(root.join("sheets"))?;
    std::fs::create_dir_all(root.join("segments"))?;
    std::fs::create_dir_all(root.join("frames"))?;

    // The census must exist — the film reads its own audit.
    let probe = match Probe::load(&manifest_path()) {
        Some(p) => p,
        None => {
            println!("no manifest — running the census first");
            census()?;
            Probe::load(&manifest_path()).ok_or("census produced no manifest")?
        }
    };
    println!(
        "product_film master — manifest in hand: frames {} · shapes {} · alive {:.3}s · SCALE_FACTOR {}",
        probe.frames, probe.shapes, probe.alive_seconds, scale_factor()
    );

    let all = scenes();
    let total = total_frames();
    let (rw, rh) = raster_size();
    let scale = scale_factor();
    println!(
        "  raster {}x{} ({}× the 1920×1080 logical base)",
        rw, rh, scale
    );

    let mut rig = Rig::new();
    let mut renderer = NativeRenderer::new();
    renderer.guard_filtered_layers_below(32);

    let t0 = std::time::Instant::now();
    let mut done_sheets = 0usize;

    // Per-scene segments, concat at the end — a run can resume at any
    // scene boundary, and the master is only assembled once every
    // segment and sheet exists.
    let seg_dir = root.join("segments");

    // **The studio act is one continuous session** — the rig (and the
    // studio it mounts) is never reset across scene boundaries, so the
    // script cursor keeps its place and the session state persists.
    // Resuming mid-act is handled by replaying from the act's start.
    let mut first_unrendered: Option<usize> = None;
    for (si, s) in all.iter().enumerate() {
        let seg = seg_dir.join(format!("{:02}_{}.mp4", si + 1, s.name));
        let sheet = root.join("sheets").join(format!("{}_{}.png", s.id, s.name));
        if !(seg.exists() && sheet.exists()) {
            first_unrendered = Some(si);
            break;
        }
    }

    for (si, s) in all.iter().enumerate() {
        let start_abs: f32 = all.iter().take(si).map(|x| x.seconds).sum();
        let n = s.frames();
        let seg = seg_dir.join(format!("{:02}_{}.mp4", si + 1, s.name));
        let sheet = root.join("sheets").join(format!("{}_{}.png", s.id, s.name));

        // Resume: a scene is done when its segment and its sheet both exist.
        if seg.exists() && sheet.exists() {
            println!(
                "  {} {} · segment + sheet already rendered, skipping",
                s.id, s.name
            );
            done_sheets += 1;
            continue;
        }

        // A resume into the middle of the studio act: replay the script
        // from the film's start to this scene's start (no raster — the
        // actions are cheap, the compiles are cached) so the session is
        // exactly where a continuous run would have it.
        if first_unrendered.is_some() && s.kind == Kind::Studio && rig.studio.is_none() {
            println!(
                "  resuming into the studio act at {} — replaying the session",
                s.id
            );
            let _ = rig.studio();
            script::apply_up_to(
                &mut rig.driver,
                rig.studio.as_ref().unwrap(),
                start_abs,
                &mut rig.cursor,
            );
            rig.driver
                .draw_frame_at(Duration::from_secs_f64(start_abs as f64));
        }

        let scene_dir = root
            .join("frames")
            .join(format!("{:02}_{}", si + 1, s.name));
        std::fs::remove_dir_all(&scene_dir).ok();
        std::fs::create_dir_all(&scene_dir)?;
        let sheet_stride = (n / 16).max(1);
        let scene_start = std::time::Instant::now();

        let mut ffmpeg = ProcCommand::new("ffmpeg")
            .args([
                "-y",
                "-loglevel",
                "error",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgba",
                "-s",
                &format!("{rw}x{rh}"),
                "-framerate",
                "60",
                "-i",
                "-",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "18",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&seg)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let mut stdin = ffmpeg.stdin.take().ok_or("no ffmpeg stdin")?;

        for i in 0..n {
            // Two passes over the frame: once to learn where the shot is
            // (the ctx the scene needs), then once more with the cut's
            // ramp applied, because the ramp changes the clear colour and
            // the clear colour is decided while the tree is built.
            // Two passes over the frame: once to learn where the shot is,
            // then once more with the cut's ramp applied, because the ramp
            // changes the clear colour and the clear colour is decided
            // while the tree is built.
            let (_ctx, world, sh) = rig.build_split(s, start_abs, i, &probe, si);
            let (pixels, _report) = raster_split(
                &mut renderer,
                &world,
                rig.chrome_driver.scene(),
                rig.background,
                sh.cam,
            )?;
            stdin.write_all(pixels.data())?;

            // The sheet frames — sixteen strided, parked only for the tile.
            if i % sheet_stride == 0 {
                let k = i / sheet_stride;
                if k < 16 {
                    let img = image::RgbaImage::from_raw(rw, rh, pixels.data().to_vec())
                        .ok_or("invalid frame")?;
                    img.save(scene_dir.join(format!("frame_{k:03}.png")))?;
                }
            }
            if i % 240 == 0 && i > 0 {
                let el = t0.elapsed().as_secs_f32();
                println!(
                    "  {} {} · {:>3}% · {:.1} min in",
                    s.id,
                    s.name,
                    (i * 100 / n),
                    el / 60.0
                );
            }
        }
        drop(stdin);
        let status = ffmpeg.wait()?;
        if !status.success() {
            return Err(format!("ffmpeg exited {status} on {}", s.id).into());
        }

        // The house contact sheet — fps=10, scale=480:-1, tile=4x4.
        let status = ProcCommand::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-framerate", "10"])
            .arg("-i")
            .arg(scene_dir.join("frame_%03d.png"))
            .arg("-vf")
            .arg("scale=480:-1,tile=4x4")
            .arg("-frames:v")
            .arg("1")
            .arg(&sheet)
            .status();
        match status {
            Ok(st) if st.success() => println!(
                "  {} {} · done in {:.1}s · sheet {}",
                s.id,
                s.name,
                scene_start.elapsed().as_secs_f32(),
                sheet.display()
            ),
            _ => println!("  {} {} · sheet FAILED", s.id, s.name),
        }
        let _ = std::fs::remove_dir_all(&scene_dir);
        done_sheets += 1;
    }

    // Assemble — concat the segments (stream copy: same codec, same
    // settings, byte-faithful to the renders).
    let list = seg_dir.join("list.txt");
    let mut body = String::new();
    for (si, s) in all.iter().enumerate() {
        body.push_str(&format!(
            "file '{}/segments/{:02}_{}.mp4'\n",
            root.display(),
            si + 1,
            s.name
        ));
    }
    std::fs::write(&list, body)?;
    let mp4 = root.join("product_film.mp4");
    let status = ProcCommand::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "concat", "-safe", "0"])
        .arg("-i")
        .arg(&list)
        .arg("-c")
        .arg("copy")
        .arg("-movflags")
        .arg("+faststart")
        .arg(&mp4)
        .status()?;
    if !status.success() {
        return Err("concat failed".into());
    }

    println!(
        "\nmaster assembled — {} scene segments · {:.1} min wall · {} sheets · {} frames",
        all.len(),
        t0.elapsed().as_secs_f32() / 60.0,
        done_sheets,
        total
    );
    println!("  film → {}", mp4.display());
    println!("  sheets → {}", root.join("sheets").display());
    Ok(())
}

// ── The preview mode — one scene, sixteen frames, a sheet ──────────────────

fn preview_scene(name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let probe = Probe::load(&manifest_path()).unwrap_or_default();
    let all = scenes();
    let found = all
        .iter()
        .find(|s| s.id.to_ascii_lowercase().contains(name) || s.name.contains(name))
        .ok_or_else(|| format!("no scene matches {name:?}"))?;

    let root = work_root()
        .join("preview")
        .join(found.id.to_ascii_lowercase());
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root)?;

    let si = all.iter().position(|s| s.id == found.id).unwrap_or(0);
    let start_abs: f32 = all.iter().take(si).map(|s| s.seconds).sum();
    let (rw, rh) = raster_size();

    let mut rig = Rig::new();
    let mut renderer = NativeRenderer::new();
    let n = 16;
    println!(
        "preview {} {} · {} frames at {}x{} (SCALE_FACTOR {})",
        found.id,
        found.name,
        n,
        rw,
        rh,
        scale_factor()
    );
    for k in 0..n {
        // Sixteen evenly spaced beats, including the hold at the end.
        let i = (k as f32 / (n - 1) as f32 * (found.frames() - 1) as f32).round() as usize;
        let t_render = std::time::Instant::now();
        let (_ctx, world, sh) = rig.build_split(found, start_abs, i, &probe, si);
        let (pixels, report) = raster_split(
            &mut renderer,
            &world,
            rig.chrome_driver.scene(),
            rig.background,
            sh.cam,
        )?;
        let ms = t_render.elapsed().as_secs_f64() * 1000.0;
        let img =
            image::RgbaImage::from_raw(rw, rh, pixels.data().to_vec()).ok_or("invalid frame")?;
        img.save(root.join(format!("frame_{k:03}.png")))?;
        println!(
            "  frame {:>2} · t {:.3} · shapes {} · layers {} · {:.1} ms",
            k, _ctx.t, report.shapes, report.layers, ms
        );
    }
    let status = ProcCommand::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-framerate", "10"])
        .arg("-i")
        .arg(root.join("frame_%03d.png"))
        .arg("-vf")
        .arg("scale=480:-1,tile=4x4")
        .arg("-frames:v")
        .arg("1")
        .arg(root.join("sheet.png"))
        .status();
    if matches!(status, Ok(s) if s.success()) {
        println!("  sheet → {}", root.join("sheet.png").display());
    }
    Ok(())
}
