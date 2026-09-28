//! master — the film's two-pass harness: the census (pass 1) and the
//! master render (pass 2), plus the single-scene preview mode and the
//! calibration probe.
//!
//! **The rig.** One `FrameDriver` at 1920×1080 carries the whole film.
//! Pure scenes set their tree per frame, as ever. **Studio scenes mount
//! the actual `viewwstudio` app** — `script::mount` installs the real
//! element runtime, opens the real workspace, wires the real toolchain —
//! and the session script drives it, action by action, at fixed
//! film-times. The Shell is mounted inside a Stack with the scene's
//! overlay chrome on top; the studio state (buffers, preview, compile
//! history, the demo's route) lives in the studio's own signals,
//! outside every scene tree — which is why the session survives cuts.
//!
//! **Pass 1 · census.** Walks every frame of the film, applies the
//! script, builds the tree, and counts the command stream it emits —
//! now including the real studio's own draw commands. Raster samples at
//! full master resolution (12 per scene) give the frame-time receipt;
//! S12's live-edit window gives the **alive probe** — the measured
//! edit→pixels cost of one real keystroke through the real studio.
//! Compiles are settled off-clock (`Settle`), so the census's counts
//! and the master's pixels agree by construction.
//!
//! **Pass 2 · master.** Renders all frames at 1920×1080·60 and streams
//! raw RGBA straight into ffmpeg's stdin (`libx264 · crf 18 ·
//! yuv420p`), per-scene segments, concat at the end. Sixteen strided
//! frames per scene tile into the house contact sheet
//! (`fps=10, scale=480:-1, tile=4x4`). The MP4 carries no audio — the
//! cut works muted by construction.
//!
//! **Determinism.** The film re-renders to the byte on this bench: the
//! fixed clock, the frame-indexed script, the off-clock settles and the
//! embedded font store (plus the site's Geist, both in-tree) make a
//! frame a function of the checkout. The manifest names its bench.

use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcCommand, Stdio};
use std::time::Duration;

use vieww_foundation::Size;
use vieww_paint::native::NativeRenderer;
use vieww_paint::Command;
use vieww_render::FrameDriver;
use viewwstudio::state::Studio;
use vieww_widget::WidgetNode;

use super::script;
use super::{scenes, total_frames, Ctx, Kind, Probe, SceneDef, FPS, H, W};

/// Where the keynote's artifacts live while being built.
pub fn work_root() -> PathBuf {
    PathBuf::from("/home/z/my-project/download/keynote_z")
}

fn manifest_path() -> PathBuf {
    work_root().join("manifest.txt")
}

/// Entry point — dispatch on the CLI word.
pub fn run(mode: &str) -> Result<(), Box<dyn std::error::Error>> {
    match mode {
        "censusz" => census(),
        "masterz" => master(),
        "kzcal" => calibrate(),
        other => {
            let name = other.trim_start_matches("kz:").to_ascii_lowercase();
            preview_scene(&name)
        }
    }
}

// ── The rig — one driver, the studio mounted when the film needs it ─────────

/// The film's whole render rig: the driver, the studio (mounted lazily,
/// once), and the script cursor.
struct Rig {
    driver: FrameDriver,
    studio: Option<Studio>,
    cursor: usize,
    /// Which driver the studio was mounted on (always ours — one driver).
    background: vieww_foundation::Color,
}

impl Rig {
    fn new() -> Rig {
        let mut driver = FrameDriver::new(Size::new(W, H));
        driver.set_fonts(super::fonts());
        Rig {
            driver,
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
    fn build_frame<'a>(&mut self, s: &SceneDef, start_abs: f32, i: usize, probe: &'a Probe) -> Ctx<'a> {
        let n = s.frames();
        let sec = i as f32 / FPS;
        let t = (sec / s.seconds).min(1.0);
        let abs = start_abs + sec;
        let ctx = Ctx {
            t,
            sec,
            abs,
            ladder: super::ladder_at(abs),
            probe,
        };

        // The script only exists in the studio act; applying it before
        // the studio exists is a no-op that would mount the app early —
        // so only apply while a scene is a studio scene (or the studio
        // is already mounted and actions are still pending).
        match s.kind {
            Kind::Studio => {
                let _ = self.studio();
                script::apply_up_to(&mut self.driver, self.studio.as_ref().unwrap(), abs, &mut self.cursor);
                let overlay: WidgetNode = (s.build)(&ctx);
                let root = script::shell_root(self.studio.as_ref().unwrap(), overlay);
                self.background = viewwstudio::StudioTheme::dark().window;
                self.driver.set_root(root);
            }
            Kind::Pure => {
                let tree: WidgetNode = (s.build)(&ctx);
                self.background = super::GROUND;
                self.driver.set_root(tree);
            }
        }
        self.driver.draw_frame_at(Duration::from_secs_f64(abs as f64));
        ctx
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

// ── Pass 1 — the census ─────────────────────────────────────────────────────

pub fn census() -> Result<(), Box<dyn std::error::Error>> {
    let root = work_root();
    std::fs::create_dir_all(&root)?;

    let all = scenes();
    let total = total_frames();
    println!("keynote_z census — {} scenes · {} frames (derived, never typed)", all.len(), total);

    let mut probe = Probe::default();
    probe.frames = total as u64;

    let mut rig = Rig::new();
    let mut renderer = NativeRenderer::new();
    renderer.guard_filtered_layers_below(32);

    // The raster samples: 12 per scene → the frame-time receipt.
    let mut frame_ms_samples: Vec<f64> = Vec::new();
    // The alive probe: S12's live-edit window — the measured cost of one
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
            let alive_window = s.id == "S12" && (0.74..0.84).contains(&(i as f32 / n as f32));
            let sample = alive_window || (i % sample_every == 0);

            if sample {
                let t_start = std::time::Instant::now();
                let _ctx = rig.build_frame(s, start_abs, i, &empty_probe);
                let (pixels, _report) =
                    renderer.render_to_pixels(rig.driver.scene(), W as u32, H as u32, rig.background)?;
                let took = t_start.elapsed().as_secs_f64() * 1000.0;
                frame_ms_samples.push(took);
                if alive_window {
                    alive_samples.push(took);
                }
                let _ = pixels;
                count_scene_commands(rig.driver.scene().commands(), &mut probe);
                counted_frames += 1;
            } else {
                let _ctx = rig.build_frame(s, start_abs, i, &empty_probe);
                count_scene_commands(rig.driver.scene().commands(), &mut probe);
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
    let alive_median = alive_sorted.get(alive_sorted.len() / 2).copied().unwrap_or(0.0);
    probe.alive_seconds = (alive_median / 1000.0) as f32;

    probe.bench = Probe::bench_identity(renderer.cached_fonts() as u32);

    // The house-format manifest.
    let body = format!(
        "frames={}\nshapes={}\nglyph_runs={}\nglyphs={}\nlayers={}\nfiltered_layers={}\nstrokes={}\nshadows={}\nalive_seconds={:.3}\nframe_ms={:.2}\nbench={}\n",
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
        probe.bench,
    );
    std::fs::write(manifest_path(), body)?;
    println!(
        "\ncensus complete — {} frames counted · {} raster samples · median {:.2} ms",
        counted_frames,
        frame_ms_samples.len(),
        median_ms
    );
    println!("  shapes {} · glyph runs {} · layers {} (filtered {}) · strokes {}",
        probe.shapes, probe.glyph_runs, probe.layers, probe.filtered, probe.strokes);
    println!("  alive N = {:.3} s · bench: {}", probe.alive_seconds, probe.bench);
    println!("  manifest → {}", manifest_path().display());
    Ok(())
}

// ── Pass 2 — the master ─────────────────────────────────────────────────────

pub fn master() -> Result<(), Box<dyn std::error::Error>> {
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
        "keynote_z master — manifest in hand: frames {} · shapes {} · alive {:.3}s",
        probe.frames, probe.shapes, probe.alive_seconds
    );

    let all = scenes();
    let total = total_frames();

    let mut rig = Rig::new();
    let mut renderer = NativeRenderer::new();
    renderer.guard_filtered_layers_below(32);

    let t0 = std::time::Instant::now();
    let mut done_sheets = 0usize;

    // Per-scene segments, concat at the end — the OOM lesson (chunk the
    // pass) applied to wall-clock too: a run can resume at any scene
    // boundary, and the master is only assembled once every segment and
    // sheet exists.
    let seg_dir = root.join("segments");

    // **The studio act is one continuous session** — the rig (and the
    // studio it mounts) is never reset across scene boundaries, so the
    // script cursor keeps its place and the session state persists.
    // Resuming mid-act is handled by replaying from the act's start: a
    // resumed master re-mounts the studio and re-applies the script up
    // to the resume point before rendering continues (deterministic,
    // because the script is a pure function of film-time).
    let mut resume_abs: f32 = 0.0;
    let mut first_unrendered: Option<usize> = None;
    for (si, s) in all.iter().enumerate() {
        let seg = seg_dir.join(format!("{:02}_{}.mp4", si + 1, s.name));
        let sheet = root.join("sheets").join(format!("{}_{}.png", s.id, s.name));
        if !(seg.exists() && sheet.exists()) {
            first_unrendered = Some(si);
            break;
        }
        resume_abs += s.seconds;
    }

    for (si, s) in all.iter().enumerate() {
        let start_abs: f32 = all.iter().take(si).map(|x| x.seconds).sum();
        let n = s.frames();
        let seg = seg_dir.join(format!("{:02}_{}.mp4", si + 1, s.name));
        let sheet = root.join("sheets").join(format!("{}_{}.png", s.id, s.name));

        // Resume: a scene is done when its segment and its sheet both exist.
        if seg.exists() && sheet.exists() {
            println!("  {} {} · segment + sheet already rendered, skipping", s.id, s.name);
            done_sheets += 1;
            continue;
        }

        // A resume into the middle of the studio act: replay the script
        // from the film's start to this scene's start (no raster — the
        // actions are cheap, the compiles are cached) so the session is
        // exactly where a continuous run would have it.
        if first_unrendered.is_some() && s.kind == Kind::Studio && rig.studio.is_none() {
            println!("  resuming into the studio act at {} — replaying the session", s.id);
            let _ = rig.studio();
            script::apply_up_to(&mut rig.driver, rig.studio.as_ref().unwrap(), start_abs, &mut rig.cursor);
            // Draw once so the replayed state composes.
            rig.driver.draw_frame_at(Duration::from_secs_f64(start_abs as f64));
        }
        let _ = resume_abs;

        let scene_dir = root.join("frames").join(format!("{:02}_{}", si + 1, s.name));
        std::fs::remove_dir_all(&scene_dir).ok();
        std::fs::create_dir_all(&scene_dir)?;
        let sheet_stride = (n / 16).max(1);
        let scene_start = std::time::Instant::now();

        let mut ffmpeg = ProcCommand::new("ffmpeg")
            .args([
                "-y", "-loglevel", "error",
                "-f", "rawvideo", "-pix_fmt", "rgba",
                "-s", "1920x1080", "-framerate", "60",
                "-i", "-",
                "-c:v", "libx264", "-preset", "veryfast", "-crf", "18",
                "-pix_fmt", "yuv420p",
            ])
            .arg(&seg)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let mut stdin = ffmpeg.stdin.take().ok_or("no ffmpeg stdin")?;

        for i in 0..n {
            let _ctx = rig.build_frame(s, start_abs, i, &probe);
            let (pixels, _report) = renderer.render_to_pixels(rig.driver.scene(), W as u32, H as u32, rig.background)?;
            stdin.write_all(pixels.data())?;

            // The sheet frames — sixteen strided, parked only for the tile.
            if i % sheet_stride == 0 {
                let k = i / sheet_stride;
                if k < 16 {
                    let img = image::RgbaImage::from_raw(W as u32, H as u32, pixels.data().to_vec())
                        .ok_or("invalid frame")?;
                    img.save(scene_dir.join(format!("frame_{k:03}.png")))?;
                }
            }
            if i % 240 == 0 && i > 0 {
                let el = t0.elapsed().as_secs_f32();
                println!(
                    "  {} {} · {:>3}% · {:.1} min in",
                    s.id, s.name, (i * 100 / n), el / 60.0
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
            .arg("-i").arg(scene_dir.join("frame_%03d.png"))
            .arg("-vf").arg("scale=480:-1,tile=4x4")
            .arg("-frames:v").arg("1")
            .arg(&sheet)
            .status();
        match status {
            Ok(st) if st.success() => println!(
                "  {} {} · done in {:.1}s · sheet {}",
                s.id, s.name, scene_start.elapsed().as_secs_f32(), sheet.display()
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
    let mp4 = root.join("keynote_z.mp4");
    let status = ProcCommand::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "concat", "-safe", "0"])
        .arg("-i").arg(&list)
        .arg("-c").arg("copy")
        .arg("-movflags").arg("+faststart")
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

    let root = work_root().join("preview").join(found.id.to_ascii_lowercase());
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root)?;

    let si = all.iter().position(|s| s.id == found.id).unwrap_or(0);
    let start_abs: f32 = all.iter().take(si).map(|s| s.seconds).sum();

    let mut rig = Rig::new();
    let mut renderer = NativeRenderer::new();
    let n = 16;
    println!("preview {} {} · {} frames at 1920×1080", found.id, found.name, n);
    for k in 0..n {
        // Sixteen evenly spaced beats, including the hold at the end.
        let i = (k as f32 / (n - 1) as f32 * (found.frames() - 1) as f32).round() as usize;
        let t_render = std::time::Instant::now();
        let _ctx = rig.build_frame(found, start_abs, i, &probe);
        let (pixels, report) = renderer.render_to_pixels(rig.driver.scene(), W as u32, H as u32, rig.background)?;
        let ms = t_render.elapsed().as_secs_f64() * 1000.0;
        let img = image::RgbaImage::from_raw(W as u32, H as u32, pixels.data().to_vec())
            .ok_or("invalid frame")?;
        img.save(root.join(format!("frame_{k:03}.png")))?;
        println!(
            "  frame {:>2} · t {:.3} · shapes {} · layers {} · {:.1} ms",
            k,
            _ctx.t,
            report.shapes,
            report.layers,
            ms
        );
    }
    let status = ProcCommand::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-framerate", "10"])
        .arg("-i").arg(root.join("frame_%03d.png"))
        .arg("-vf").arg("scale=480:-1,tile=4x4")
        .arg("-frames:v").arg("1")
        .arg(root.join("sheet.png"))
        .status();
    if matches!(status, Ok(s) if s.success()) {
        println!("  sheet → {}", root.join("sheet.png").display());
    }
    Ok(())
}

// ── The calibration probe — verify the script's tap coordinates ────────────

/// Mount the studio, drive to the live preview, and hit-test the demo's
/// tappable rows and the compiled counter's button — printing the
/// coordinates the script should carry. Development instrument; the
/// committed constants are its receipts.
fn calibrate() -> Result<(), Box<dyn std::error::Error>> {
    let out = work_root().join("calibrate");
    std::fs::create_dir_all(&out)?;
    let mut rig = Rig::new();
    let mut renderer = NativeRenderer::new();

    // Drive to the live demo, exactly as S12 does.
    let studio = rig.studio().clone();
    // The tab list — the receipt for the script's TAB_* indices.
    let names: Vec<String> = studio.buffers.get().iter().map(|b| b.name.clone()).collect();
    println!("BUFFERS = {:?}", names);
    script::apply_up_to(&mut rig.driver, &studio, 103.3, &mut rig.cursor);
    let ctx = Ctx { t: 0.5, sec: 6.0, abs: 103.3, ladder: 1, probe: &Probe::default() };
    let overlay: WidgetNode = (scenes()[11].build)(&ctx);
    let root = script::shell_root(&studio, overlay);
    rig.background = viewwstudio::StudioTheme::dark().window;
    rig.driver.set_root(root);
    rig.driver.draw_frame_at(Duration::from_secs_f64(103.3));
    snap(&mut rig, &mut renderer, &out, "01-live-home", 103.4)?;

    // Scan for the demo's first tappable row, the way the walkthrough
    // does: tap, watch the route, reset.
    let before = studio.live_state.route.get();
    let mut found: Option<(f32, f32)> = None;
    let mut clock = 104.0f64;
    for y in (140..760).step_by(6) {
        for x in (980..1700).step_by(10) {
            clock += 0.05;
            let at = vieww_foundation::Offset::new(x as f32, y as f32);
            rig.driver.handle_pointer(&vieww_foundation::PointerEvent::down(
                vieww_foundation::PointerId(1),
                at,
                Duration::from_secs_f64(clock),
            ));
            rig.driver.draw_frame_at(Duration::from_secs_f64(clock));
            rig.driver.handle_pointer(&vieww_foundation::PointerEvent::up(
                vieww_foundation::PointerId(1),
                at,
                Duration::from_secs_f64(clock + 0.02),
            ));
            rig.driver.draw_frame_at(Duration::from_secs_f64(clock + 0.03));
            if studio.live_state.route.get() != before {
                found = Some((x as f32, y as f32));
                studio.live_state.route.set(before);
                break;
            }
        }
        if found.is_some() {
            break;
        }
    }
    println!("TAP_LIVE_ROW = {:?}", found);
    if let Some((x, y)) = found {
        let _ = (x, y);
    }

    // Continue to the compiled counter and scan for Add one.
    script::apply_up_to(&mut rig.driver, &studio, 136.0, &mut rig.cursor);
    rig.driver.draw_frame_at(Duration::from_secs_f64(136.0));
    snap(&mut rig, &mut renderer, &out, "02-counter-compiled", 136.1)?;

    // The Add one button: tap the script's verified constant and prove
    // the count moved — render before and after, and let the caller
    // (or the snapshot) carry the receipt.
    {
        let at = script::TAP_ADD_ONE;
        rig.driver.handle_pointer(&vieww_foundation::PointerEvent::down(
            vieww_foundation::PointerId(1),
            at,
            Duration::from_secs_f64(136.5),
        ));
        rig.driver.draw_frame_at(Duration::from_secs_f64(136.5));
        rig.driver.handle_pointer(&vieww_foundation::PointerEvent::up(
            vieww_foundation::PointerId(1),
            at,
            Duration::from_secs_f64(136.52),
        ));
        rig.driver.draw_frame_at(Duration::from_secs_f64(136.6));
        println!("tapped Add one at {:?}", at);
    }
    snap(&mut rig, &mut renderer, &out, "03-after-scan", 137.0)?;
    Ok(())
}

/// One calibration snapshot, at the rig's current film time.
fn snap(
    rig: &mut Rig,
    renderer: &mut NativeRenderer,
    out: &Path,
    name: &str,
    abs: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    for k in 0..3 {
        rig.driver.draw_frame_at(Duration::from_secs_f64(abs + k as f64 * 0.05));
    }
    let (png, _report) = renderer.render_to_png(rig.driver.scene(), W as u32, H as u32, rig.background)?;
    let path = out.join(format!("{name}.png"));
    std::fs::write(&path, png)?;
    println!("  snap → {}", path.display());
    Ok(())
}
