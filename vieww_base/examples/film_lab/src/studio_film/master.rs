//! master — this film's two-pass harness: the census (pass 1) and the
//! master render (pass 2), plus the single-scene preview mode.
//!
//! The v2 rig: every frame is built, then **shot** — the world's scene
//! is lifted through the camera plan's transform (`filmkit::shot_sf`),
//! composed with the join's screen-space slide (`filmkit::join_state`),
//! and only then rastered. The chrome — captions, chips, the rail —
//! composites at the identity, screen-fixed, the way a real film's
//! titles don't move when the camera does. Post-raster, the join's
//! pixel effects (the glitch, the iris) run on the buffer.
//!
//! **Pass 1 · census.** Walks every frame, applies this film's session,
//! counts the command stream — including the real studio's own draw
//! commands — and raster-samples 12 frames per scene for the frame-time
//! receipt. Z08's live-edit window is the alive probe. Writes the house
//! manifest, which the ledger and end-card scenes then quote. **Pass 2
//! · master.** Streams raw RGBA into ffmpeg (libx264 · crf 18 ·
//! yuv420p) per-scene; sixteen strided frames tile into the house
//! contact sheet; the segments concat into `studio_film.mp4`. No audio
//! — the film works muted by construction.

use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::process::{Command as ProcCommand, Stdio};
use std::time::{Duration, Instant};

use vieww_paint::native::NativeRenderer;
use vieww_paint::Command;
use vieww_render::FrameDriver;
use viewwstudio::state::Studio;
use vieww_foundation::{Color, Transform};
use vieww_widget::Opacity;

use crate::product_film as pf;
use crate::product_film::script;
use crate::product_film::{Ctx, Kind, Probe, SceneDef};
use super::filmkit::{self, JoinFx};
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

/// The device-resolution multiplier — the house SCALE_FACTOR door.
pub fn scale_factor() -> f32 {
    std::env::var("SCALE_FACTOR")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|f| *f > 0.0 && *f <= 8.0)
        .unwrap_or(1.0)
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
        other => {
            let name = other.trim_start_matches("sf:").to_ascii_lowercase();
            preview(&name)
        }
    }
}

// ── The rig ─────────────────────────────────────────────────────────────────

struct Rig {
    driver: FrameDriver,
    chrome_driver: FrameDriver,
    studio: Option<Studio>,
    cursor: usize,
    background: Color,
    /// This frame's shot — camera and luminance ramp.
    shot: pf::Shot,
    /// This frame's join state — slide and pixel fx.
    join: filmkit::JoinState,
    /// The absolute frame index, for deterministic pixel fx.
    frame_i: u64,
}

impl Rig {
    fn new() -> Rig {
        let mut driver = FrameDriver::new(pf::CANVAS);
        let mut chrome_driver = FrameDriver::new(pf::CANVAS);
        let fonts = pf::fonts();
        driver.set_fonts(fonts);
        chrome_driver.set_fonts(pf::fonts());
        Rig {
            driver,
            chrome_driver,
            studio: None,
            cursor: 0,
            background: pf::GROUND,
            shot: pf::Shot { cam: pf::Cam::STILL, alpha: 1.0 },
            join: filmkit::JoinState {
                slide: filmkit::Slide::NONE,
                fx: JoinFx::None,
                p: 1.0,
                enter: false,
            },
            frame_i: 0,
        }
    }

    /// The real studio, mounted once. `script::mount` installs the
    /// element runtime, opens the workspace, wires the toolchain.
    fn studio(&mut self) -> &mut Studio {
        if self.studio.is_none() {
            self.studio = Some(script::mount(&mut self.driver));
        }
        self.studio.as_mut().unwrap()
    }

    /// Build one frame: apply the session, build the scene tree through
    /// its shot, draw. The shot is a function of the clock alone, so it
    /// is known before the tree exists — the camera the scene will be
    /// seen through is the camera the scene is built with.
    fn build_frame(&mut self, s: &SceneDef, si: usize, start_abs: f32, i: usize, probe: &Probe) {
        let sec = i as f32 / FPS;
        let t = (sec / s.seconds).min(1.0);
        let abs = start_abs + sec;
        let ladder = ladder_at(abs);

        let sh = filmkit::shot_sf(s.id, si, s.seconds, t, sec);
        let join = filmkit::join_state(s.id, s.seconds, sec);
        self.shot = sh;
        self.join = join;
        self.frame_i = (start_abs * FPS).round() as u64 + i as u64;

        pf::clear_chrome();
        let a = sh.alpha.clamp(0.0, 1.0);
        let ramping = a < 0.999;

        let ctx = Ctx {
            t,
            sec,
            abs,
            ladder,
            probe,
            cam: sh.cam,
        };
        let overlay = (s.build)(&ctx);
        let root = match s.kind {
            Kind::Pure => {
                self.background = pf::GROUND;
                if ramping {
                    Opacity::new(a).child(overlay).into()
                } else {
                    overlay
                }
            }
            Kind::Studio => {
                // The real app, mounted once — lazily, so pure scenes
                // never pay for it.
                if self.studio.is_none() {
                    self.studio = Some(script::mount(&mut self.driver));
                }
                let win = viewwstudio::StudioTheme::dark().window;
                self.background = if ramping {
                    pf::mix(pf::GROUND, win, a)
                } else {
                    win
                };
                let Rig {
                    driver,
                    studio,
                    cursor,
                    ..
                } = self;
                super::script::apply_session_up_to(driver, studio.as_mut().unwrap(), abs, cursor);
                let shell = script::shell_root(studio.as_ref().unwrap(), overlay);
                if ramping {
                    Opacity::new(a).child(shell).into()
                } else {
                    shell
                }
            }
        };
        self.driver.set_root(root);
        self.driver
            .draw_frame_at(Duration::from_secs_f64(abs.max(0.0) as f64));

        // The film's own voice — captions, chips, the rail — on its own
        // driver, so the world's scene stays the world's. The chrome
        // rides the same luminance ramp the world does.
        let chrome_tree = pf::take_chrome();
        let chrome_root: vieww_widget::WidgetNode = if ramping {
            Opacity::new(a).child(chrome_tree).into()
        } else {
            chrome_tree
        };
        self.chrome_driver.set_root(chrome_root);
        self.chrome_driver
            .draw_frame_at(Duration::from_secs_f64(abs.max(0.0) as f64));
    }
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

/// The join's screen-space slide, resolved to a transform for this
/// frame. An entering join starts fully off and decelerates home; an
/// exiting one accelerates fully off. The camera composes underneath.
fn join_offset(join: &filmkit::JoinState) -> Option<vieww_foundation::Offset> {
    if join.slide.is_none() {
        return None;
    }
    Some(if join.enter {
        join.slide.offset_at(1.0 - join.p)
    } else {
        join.slide.offset_at(join.p)
    })
}

/// Raster the two scenes into one picture — the world through the
/// camera and the join's slide, the chrome screen-fixed over it.
fn raster(
    rig: &Rig,
    renderer: &mut NativeRenderer,
) -> Result<(vieww_paint::native::Pixels, vieww_paint::native::SceneReport), Box<dyn std::error::Error>>
{
    let (rw, rh) = raster_size();
    let s = scale_factor();
    let world_t = match join_offset(&rig.join) {
        Some(o) => rig.shot.cam.transform().then(Transform::translate(o)),
        None => rig.shot.cam.transform(),
    };
    let still_world = world_t == Transform::IDENTITY;
    let mut frame = vieww_paint::Scene::default();
    if still_world {
        frame.append(rig.driver.scene(), Transform::IDENTITY);
    } else {
        frame.append(rig.driver.scene(), world_t);
    }
    frame.append(rig.chrome_driver.scene(), Transform::IDENTITY);
    let scaled = if (s - 1.0).abs() < 1.0e-4 {
        frame
    } else {
        frame.scaled(s)
    };
    Ok(renderer.render_to_pixels(&scaled, rw, rh, rig.background)?)
}

/// The join's pixel-fx strength for this frame — an entering glitch
/// fades as the join completes, an exiting one grows; an iris inverts
/// the same way. `None` when the pass would be a no-op.
fn fx_strength(join: &filmkit::JoinState) -> Option<f32> {
    if join.fx == JoinFx::None {
        return None;
    }
    let raw = if join.enter { 1.0 - join.p } else { join.p };
    let s = raw.clamp(0.0, 1.0);
    (s > 0.004).then_some(s)
}

/// Render + post-process: the raster, then the join's pixel pass.
/// Returns the finished RGBA buffer and the scene report.
fn finish_frame(
    rig: &Rig,
    renderer: &mut NativeRenderer,
) -> Result<(Vec<u8>, vieww_paint::native::SceneReport), Box<dyn std::error::Error>> {
    let (pixels, report) = raster(rig, renderer)?;
    let (rw, rh) = raster_size();
    let mut buf = pixels.data().to_vec();
    if let Some(strength) = fx_strength(&rig.join) {
        filmkit::post_fx(&mut buf, rw, rh, rig.join.fx, strength, rig.frame_i);
    }
    Ok((buf, report))
}

// ── Pass 1 — the census ─────────────────────────────────────────────────────

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

    let mut rig = Rig::new();
    let mut renderer = NativeRenderer::new();
    renderer.guard_filtered_layers_below(32);

    let mut probe = Probe::default();
    let mut frame_ms_samples: Vec<f32> = Vec::new();
    let mut alive_samples: Vec<f64> = Vec::new();
    let mut counted_frames = 0u64;

    for (si, s) in all.iter().enumerate() {
        let n = frames_of(s);
        let start_abs = scene_start(si);
        let sample_every = (n / 12).max(1);
        let t0 = Instant::now();

        for i in 0..n {
            let sec = i as f32 / FPS;
            let abs = start_abs + sec;

            // The alive probe — Z08's live-edit window: one stopwatch
            // around the whole real pipeline, session apply to pixels.
            if s.id == "Z08" {
                let frac = i as f32 / n as f32;
                if (0.16..0.45).contains(&frac) {
                    let t_alive = Instant::now();
                    rig.build_frame(s, si, start_abs, i, &probe);
                    let (pixels, _) = raster(&rig, &mut renderer)?;
                    std::hint::black_box(&pixels);
                    alive_samples.push(t_alive.elapsed().as_secs_f64() * 1000.0);
                } else {
                    rig.build_frame(s, si, start_abs, i, &probe);
                }
            } else {
                rig.build_frame(s, si, start_abs, i, &probe);
            }

            count_scene_commands(rig.driver.scene().commands(), &mut probe);
            count_scene_commands(rig.chrome_driver.scene().commands(), &mut probe);

            if i % sample_every == 0 {
                // The frame-time receipt is the whole frame — build,
                // draw and raster — the number the master actually pays.
                let t_ms = Instant::now();
                rig.build_frame(s, si, start_abs, i, &probe);
                let (pixels, _) = raster(&rig, &mut renderer)?;
                std::hint::black_box(&pixels);
                frame_ms_samples.push((t_ms.elapsed().as_secs_f64() * 1000.0) as f32);
            }
            counted_frames += 1;
        }
        println!(
            "  {} {} · {} frames · census {:.1}s elapsed",
            s.id,
            s.name,
            n,
            t0.elapsed().as_secs_f32()
        );
    }

    frame_ms_samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median_ms = frame_ms_samples
        .get(frame_ms_samples.len() / 2)
        .copied()
        .unwrap_or(0.0);
    probe.frames = counted_frames;
    probe.frame_ms = median_ms;

    let mut alive_sorted = alive_samples.clone();
    alive_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let alive_median = alive_sorted.get(alive_sorted.len() / 2).copied().unwrap_or(0.0);
    probe.alive_seconds = (alive_median / 1000.0) as f32;

    probe.bench = Probe::bench_identity(renderer.cached_fonts() as u32);

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
    std::fs::create_dir_all(work_root())?;
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
    println!("  alive median {:.3} s · bench: {}", probe.alive_seconds, probe.bench);
    println!("  manifest → {}", manifest_path().display());
    Ok(())
}

// ── Pass 2 — the master ─────────────────────────────────────────────────────

pub fn master() -> Result<(), Box<dyn std::error::Error>> {
    let root = work_root();
    let seg_dir = root.join("segments");
    std::fs::create_dir_all(root.join("sheets"))?;
    std::fs::create_dir_all(&seg_dir)?;

    // The census must exist — the film reads its own audit.
    let manifest = manifest_path();
    let probe = match Probe::load(&manifest) {
        Some(p) => p,
        None => {
            println!("no manifest — running the census first");
            census()?;
            Probe::load(&manifest).ok_or("census produced no manifest")?
        }
    };

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

        // Resume: a scene is done when its segment and its sheet both exist.
        if seg.exists() && sheet.exists() {
            println!("  {} {} · segment + sheet already rendered, skipping", s.id, s.name);
            done_sheets += 1;
            continue;
        }

        // Resuming into the studio act: replay the session from the top
        // (no raster) so the state at `start_abs` is the state the linear
        // run would have had.
        if s.kind == Kind::Studio && rig.studio.is_none() {
            println!("  entering the studio act at {} — mounting the real app", s.id);
            let _ = rig.studio();
            let mut warm = 0usize;
            super::script::apply_session_up_to(&mut rig.driver, rig.studio.as_mut().unwrap(), start_abs, &mut warm);
            rig.driver.draw_frame_at(Duration::from_secs_f64(start_abs as f64));
        }

        let scene_dir = root.join("frames").join(format!("{:02}_{}", si + 1, s.name));
        std::fs::remove_dir_all(&scene_dir).ok();
        std::fs::create_dir_all(&scene_dir)?;
        let sheet_stride = (n / 16).max(1);
        let scene_t0 = Instant::now();

        let mut ffmpeg = ProcCommand::new("ffmpeg")
            .args([
                "-y", "-loglevel", "error",
                "-f", "rawvideo", "-pix_fmt", "rgba",
                "-s", &format!("{rw}x{rh}"), "-framerate", &format!("{FPS}"),
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
            rig.build_frame(s, si, start_abs, i, &probe);
            let (buf, _report) = finish_frame(&rig, &mut renderer)?;
            stdin.write_all(&buf)?;

            // The sheet frames — sixteen strided, parked only for the tile.
            if i % sheet_stride == 0 {
                let k = i / sheet_stride;
                if k < 16 {
                    let img = image::RgbaImage::from_raw(rw, rh, buf.clone())
                        .ok_or("invalid frame")?;
                    img.save(scene_dir.join(format!("frame_{k:03}.png")))?;
                }
            }
            if i % 150 == 0 && i > 0 {
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
                scene_t0.elapsed().as_secs_f32(),
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
    let mp4 = root.join("studio_film.mp4");
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
        total_frames()
    );
    println!("  mp4 → {}", mp4.display());
    Ok(())
}

// ── The preview — one scene, sixteen frames, one sheet ─────────────────────

fn preview(name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let all = scenes();
    let (si, s) = all
        .iter()
        .enumerate()
        .find(|(_, s)| {
            s.id.eq_ignore_ascii_case(name)
                || s.name.to_ascii_lowercase().contains(&name.to_ascii_lowercase())
        })
        .map(|(si, s)| (si, s.clone()))
        .ok_or_else(|| format!("no scene matches {name:?} — try: {}", all.iter().map(|s| s.id).collect::<Vec<_>>().join(" ")))?;

    let probe = Probe::load(&manifest_path()).unwrap_or_default();
    let root = work_root().join("preview").join(s.id);
    std::fs::create_dir_all(&root)?;
    let (rw, rh) = raster_size();
    let n = frames_of(&s);
    let start_abs = scene_start(si);

    let mut renderer = NativeRenderer::new();
    renderer.guard_filtered_layers_below(32);
    let mut rig = Rig::new();

    let mut last_report = None;
    for k in 0..16 {
        let i = if k == 15 { n - 1 } else { k * (n - 1) / 15 };
        let t0 = Instant::now();
        rig.build_frame(&s, si, start_abs, i, &probe);
        let (buf, report) = finish_frame(&rig, &mut renderer)?;
        let ms = t0.elapsed().as_secs_f64();
        println!(
            "  {} frame {i:>4} · {} shapes · {} glyph runs · {} layers ({:.1} ms)",
            s.id, report.shapes, report.glyph_runs, report.layers, ms
        );
        last_report = Some(report);
        let img = image::RgbaImage::from_raw(rw, rh, buf).ok_or("invalid frame")?;
        img.save(root.join(format!("frame_{k:03}.png")))?;
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
        .status()?;
    if !status.success() {
        return Err("preview sheet failed".into());
    }
    let _ = last_report;
    println!("preview → {}", root.display());
    Ok(())
}
