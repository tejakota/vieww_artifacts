//! master — the film's two-pass harness: the census (pass 1) and the master
//! render (pass 2), plus the single-scene preview mode.
//!
//! **Pass 1 · census.** Walks every frame of the film, builds its tree, and
//! counts the command stream it emits — shapes, glyph runs, layers, blurred
//! layers, strokes, shadows — the audit S15's manifest bars display. Raster
//! samples at full master resolution (12 per scene) give the frame-time
//! receipt; three samples around S04's first paint give the "alive in N
//! seconds" probe. Writes `manifest.txt`, house `key=value` format.
//!
//! **Pass 2 · master.** Renders all frames at 1920×1080·60 through
//! `NativeRenderer` and streams raw RGBA straight into ffmpeg's stdin
//! (`libx264 · crf 18 · yuv420p`) — no frame PNG parking on disk (the
//! hero4k OOM lesson, worn as plumbing). Sixteen strided frames per scene
//! park only long enough to tile into the house contact sheet
//! (`fps=10, scale=480:-1, tile=4x4`). The MP4 carries no audio track —
//! sound is the author's, later; the cut works muted by construction.
//!
//! Both passes go through the same fixed clock: a frame is a pure function
//! of its film-time, so the census's counts and the master's pixels agree
//! by construction, and the master re-renders to the byte on this bench.

use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::process::{Command as ProcCommand, Stdio};
use std::time::Duration;

use vieww_foundation::Size;
use vieww_paint::native::NativeRenderer;
use vieww_paint::Command;
use vieww_render::FrameDriver;
use vieww_widget::WidgetNode;

use crate::film_lib::BG;

use super::{scenes, total_frames, Ctx, Probe, SceneDef, FPS, H, W};

/// Where the keynote's artifacts live while being built.
pub(crate) fn work_root() -> PathBuf {
    PathBuf::from("/home/z/my-project/download/keynote")
}

fn manifest_path() -> PathBuf {
    work_root().join("manifest.txt")
}

/// Entry point — dispatch on the CLI word.
pub(crate) fn run(mode: &str) -> Result<(), Box<dyn std::error::Error>> {
    match mode {
        "census" => census(),
        "master" => master(),
        other => {
            let name = other.trim_start_matches("kn:").to_ascii_lowercase();
            preview_scene(&name)
        }
    }
}

// ── The shared render seam ──────────────────────────────────────────────────

fn make_driver() -> FrameDriver {
    let mut driver = FrameDriver::new(Size::new(W, H));
    driver.use_system_fonts();
    driver
}

fn make_renderer() -> NativeRenderer {
    let mut renderer = NativeRenderer::new();
    renderer.guard_filtered_layers_below(32);
    renderer
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

/// Build one frame of scene `s` at in-scene frame `i` onto the driver.
fn build_frame<'a>(
    driver: &mut FrameDriver,
    s: &SceneDef,
    start_abs: f32,
    i: usize,
    probe: &'a Probe,
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
    };
    let tree: WidgetNode = (s.build)(&ctx);
    driver.set_root(tree);
    driver.draw_frame_at(Duration::from_secs_f64(abs as f64));
    ctx
}

// ── Pass 1 — the census ─────────────────────────────────────────────────────

pub(crate) fn census() -> Result<(), Box<dyn std::error::Error>> {
    let root = work_root();
    std::fs::create_dir_all(&root)?;

    let all = scenes();
    let total = total_frames();
    println!(
        "keynote census — {} scenes · {} frames (derived, never typed)",
        all.len(),
        total
    );

    let mut probe = Probe::default();
    probe.frames = total as u64;

    let mut driver = make_driver();
    let mut renderer = make_renderer();

    // The raster samples: 12 per scene → the frame-time receipt.
    let mut frame_ms_samples: Vec<f64> = Vec::new();
    // The alive probe: three samples around S04's first paint (the preview
    // blooming alive) — the edit→visible latency this bench actually pays,
    // full pipeline, at master resolution.
    let mut alive_samples: Vec<f64> = Vec::new();
    let empty_probe = Probe::default();

    let t0 = std::time::Instant::now();
    let mut counted_frames = 0usize;

    for (si, s) in all.iter().enumerate() {
        let start_abs: f32 = all.iter().take(si).map(|x| x.seconds).sum();
        let n = s.frames();
        let sample_every = (n / 12).max(1);
        for i in 0..n {
            let _ctx = build_frame(&mut driver, s, start_abs, i, &empty_probe);
            count_scene_commands(driver.scene().commands(), &mut probe);
            counted_frames += 1;

            // The raster receipt — sampled, at full master resolution.
            if i % sample_every == 0 {
                let t_start = std::time::Instant::now();
                let (pixels, _report) =
                    renderer.render_to_pixels(driver.scene(), W as u32, H as u32, BG)?;
                let took = t_start.elapsed().as_secs_f64() * 1000.0;
                frame_ms_samples.push(took);
                let _ = pixels;
                // S04's alive window — the probe samples (T-01).
                if s.id == "S04" && (0.28..0.42).contains(&(i as f32 / n as f32)) {
                    alive_samples.push(took);
                }
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
        "keynote master — manifest in hand: frames {} · shapes {} · alive {:.3}s",
        probe.frames, probe.shapes, probe.alive_seconds
    );

    let all = scenes();
    let _total = total_frames();

    let mut driver = make_driver();
    let mut renderer = make_renderer();

    let t0 = std::time::Instant::now();
    let mut done_sheets = 0usize;

    // Per-scene segments, concat at the end — the 4 GB bench's OOM lesson
    // (chunk the pass) applied to wall-clock too: a run can resume at any
    // scene boundary, and the master is only assembled once every segment
    // and sheet exists.
    let seg_dir = root.join("segments");

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
                "1920x1080",
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
            let _ctx = build_frame(&mut driver, s, start_abs, i, &probe);
            let (pixels, _report) =
                renderer.render_to_pixels(driver.scene(), W as u32, H as u32, BG)?;
            stdin.write_all(pixels.data())?;

            // The sheet frames — sixteen strided, parked only for the tile.
            if i % sheet_stride == 0 {
                let k = i / sheet_stride;
                if k < 16 {
                    let img =
                        image::RgbaImage::from_raw(W as u32, H as u32, pixels.data().to_vec())
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
    let mp4 = root.join("keynote_master.mp4");
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
        "\nmaster assembled — {} scene segments · {:.1} min wall · {} sheets",
        all.len(),
        t0.elapsed().as_secs_f32() / 60.0,
        done_sheets
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
    let start_abs: f32 = all.iter().take(si).map(|x| x.seconds).sum();

    let mut driver = make_driver();
    let mut renderer = make_renderer();
    let n = 16;
    println!(
        "preview {} {} · {} frames at 1920×1080",
        found.id, found.name, n
    );
    for k in 0..n {
        // Sixteen evenly spaced beats, including the hold at the end.
        let i = (k as f32 / (n - 1) as f32 * (found.frames() - 1) as f32).round() as usize;
        let _ctx = build_frame(&mut driver, found, start_abs, i, &probe);
        let t_render = std::time::Instant::now();
        let (pixels, report) = renderer.render_to_pixels(driver.scene(), W as u32, H as u32, BG)?;
        let ms = t_render.elapsed().as_secs_f64() * 1000.0;
        let img = image::RgbaImage::from_raw(W as u32, H as u32, pixels.data().to_vec())
            .ok_or("invalid frame")?;
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
