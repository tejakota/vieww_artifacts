//! master — the two-pass harness.
//!
//! **Pass 1 · census.** Walks every frame of the film, builds its tree and
//! counts the command stream the framework emits — shapes, glyph runs,
//! glyphs, layers, blurred layers, strokes, shadows. Raster samples at full
//! master resolution give the frame-time receipt, and the samples taken
//! inside S06's first-paint window give the `alive in N seconds` probe.
//! Writes `manifest.txt` in the house `key=value` format.
//!
//! **Pass 2 · master.** Renders every frame at 1920×1080·60 through
//! `NativeRenderer` and streams raw RGBA into ffmpeg's stdin — no frame PNG
//! ever parks on disk except the sixteen a contact sheet needs, and those
//! are deleted once tiled. (The 4K plate's OOM is the standing warning:
//! 19,800 frames at 8.3 MB each is 164 GB nobody has.)
//!
//! Per-scene segments are encoded separately and concatenated with a stream
//! copy, so a run resumes at any scene boundary and the assembled master is
//! byte-faithful to the renders.
//!
//! Both passes go through the same fixed clock: a frame is a pure function
//! of its film-time, so the census's counts and the master's pixels agree by
//! construction.

use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcCommand, Stdio};
use std::time::Duration;

use vieww_foundation::Size;
use vieww_paint::native::NativeRenderer;
use vieww_paint::Command;
use vieww_render::FrameDriver;
use vieww_widget::WidgetNode;

use crate::film::{
    count_crates, ladder_at, scenes, total_frames, total_seconds, Ctx, Probe, SceneDef,
};
use crate::kit::{clock_mmss, thousands, BG, FPS, H, W};

/// Where the film is built. Deliberately **outside** the artifacts
/// repository: frames are scratch, and the repo takes only the master, the
/// sheets and the source.
#[must_use]
pub fn work_root() -> PathBuf {
    std::env::var("KEYNOTE_OUT")
        .map_or_else(|_| PathBuf::from("/home/claude/render/keynote"), PathBuf::from)
}

fn manifest_path() -> PathBuf {
    work_root().join("manifest.txt")
}

/// Entry point — dispatch on the CLI word.
pub fn run(mode: &str) -> Result<(), Box<dyn std::error::Error>> {
    match mode {
        "census" => census(),
        "master" => master(),
        "verify" => crate::film::verify(),
        "chapters" => {
            chapters();
            Ok(())
        }
        other => preview_scene(other.trim_start_matches("scene:")),
    }
}

/// Print the act table — the chapter list, derived from the score.
fn chapters() {
    let all = scenes();
    println!("keynote · {} scenes · {} · {} frames", all.len(), clock_mmss(total_seconds()), total_frames());
    let mut acc = 0.0f32;
    let mut act = None;
    for s in &all {
        if act != Some(s.act) {
            println!("\n  {} · {} {}", s.act.roman(), s.act.name(), s.act.kanji());
            act = Some(s.act);
        }
        println!(
            "    {} {:<11} {:>7} → {:>7}  {:>5.0}s  {:>5} frames",
            s.id,
            s.name,
            clock_mmss(acc),
            clock_mmss(acc + s.seconds),
            s.seconds,
            s.frames()
        );
        acc += s.seconds;
    }
}

// ── The shared render seam ──────────────────────────────────────────────────

fn make_driver() -> FrameDriver {
    let mut driver = FrameDriver::new(Size::new(W, H));
    // The film renders text. Without this the store is embedded-only
    // subsets, and every caption in the film is a panic.
    if std::env::var("KEYNOTE_EMBEDDED_FONTS").is_err() {
        driver.use_system_fonts();
    }
    driver
}

fn make_renderer() -> NativeRenderer {
    let mut renderer = NativeRenderer::new();
    // U-06's discipline: a frame wanting more than 32 blurred layers is a
    // grouping bug, not an ambition. The guard names it in debug builds; the
    // census carries the count in every build.
    renderer.guard_filtered_layers_below(32);
    renderer
}

/// Count one frame's command stream into the probe. This is the audit: the
/// film's manifest is what the framework was actually asked to draw.
fn count_commands(cmds: &[Command], p: &mut Probe) {
    for cmd in cmds {
        match cmd {
            Command::FillRect { .. } | Command::FillPath { .. } => p.shapes += 1,
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
            Command::DrawImage { .. } => p.images += 1,
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

/// Build one frame of scene `s` at in-scene frame `i`.
fn build_frame<'a>(
    driver: &mut FrameDriver,
    s: &SceneDef,
    start_abs: f32,
    i: usize,
    probe: &'a Probe,
) -> Ctx<'a> {
    let sec = i as f32 / FPS;
    let abs = start_abs + sec;
    let ctx = Ctx {
        t: (sec / s.seconds).min(1.0),
        sec,
        abs,
        frame: (abs * FPS).round() as u32,
        spine: ladder_at(abs),
        probe,
    };
    let tree: WidgetNode = (s.build)(&ctx);
    driver.set_root(tree);
    driver.draw_frame_at(Duration::from_secs_f64(f64::from(abs)));
    ctx
}

fn scene_starts(all: &[SceneDef]) -> Vec<f32> {
    let mut acc = 0.0;
    let mut out = Vec::with_capacity(all.len());
    for s in all {
        out.push(acc);
        acc += s.seconds;
    }
    out
}

// ── Pass 1 — the census ─────────────────────────────────────────────────────

pub fn census() -> Result<(), Box<dyn std::error::Error>> {
    let root = work_root();
    std::fs::create_dir_all(&root)?;

    let all = scenes();
    let starts = scene_starts(&all);
    let total = total_frames();
    println!(
        "keynote census — {} scenes · {} frames · {} (derived, never typed)",
        all.len(),
        total,
        clock_mmss(total_seconds())
    );

    let mut probe = Probe {
        frames: total as u64,
        crates: count_crates(),
        ..Probe::default()
    };

    let mut driver = make_driver();
    let mut renderer = make_renderer();

    // Raster samples: twelve per scene → the frame-time receipt.
    let mut frame_ms: Vec<f64> = Vec::new();
    // The alive probe: the samples taken while S06's preview comes alive —
    // the edit→visible latency this bench actually pays, full pipeline, at
    // master resolution.
    let mut alive: Vec<f64> = Vec::new();
    let empty = Probe::default();

    let t0 = std::time::Instant::now();

    for (si, s) in all.iter().enumerate() {
        let n = s.frames();
        let every = (n / 12).max(1);
        for i in 0..n {
            let _ctx = build_frame(&mut driver, s, starts[si], i, &empty);
            count_commands(driver.scene().commands(), &mut probe);
            if i % every == 0 {
                let start = std::time::Instant::now();
                let (_pixels, _report) =
                    renderer.render_to_pixels(driver.scene(), W as u32, H as u32, BG)?;
                let took = start.elapsed().as_secs_f64() * 1000.0;
                frame_ms.push(took);
                // S06's alive window — the caption's own probe.
                if s.id == "S06" {
                    let f = i as f32 / n as f32;
                    if (0.40..0.70).contains(&f) {
                        alive.push(took);
                    }
                }
            }
        }
        println!(
            "  {} {:<11} · {:>5} frames · {:.1}s elapsed",
            s.id,
            s.name,
            n,
            t0.elapsed().as_secs_f32()
        );
    }

    let mut sorted = frame_ms.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = sorted.get(sorted.len() / 2).copied().unwrap_or(0.0);
    let worst = sorted.last().copied().unwrap_or(0.0);
    probe.frame_ms = median as f32;
    probe.worst_ms = worst as f32;

    let mut asorted = alive;
    asorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let amedian = asorted.get(asorted.len() / 2).copied().unwrap_or(median);
    probe.alive_seconds = (amedian / 1000.0) as f32;

    probe.bench = Probe::bench_identity(renderer.cached_fonts() as u32);
    probe.save(&manifest_path())?;

    println!(
        "\ncensus complete — {} frames counted · {} raster samples · median {median:.2} ms · worst {worst:.2} ms",
        total,
        sorted.len()
    );
    println!(
        "  shapes {} · glyph runs {} · glyphs {} · layers {} (filtered {}) · strokes {} · shadows {}",
        thousands(probe.shapes),
        thousands(probe.glyph_runs),
        thousands(probe.glyphs),
        thousands(probe.layers),
        thousands(probe.filtered),
        thousands(probe.strokes),
        thousands(probe.shadows),
    );
    println!("  alive N = {:.3} s · crates {} · bench: {}", probe.alive_seconds, probe.crates, probe.bench);
    println!("  manifest → {}", manifest_path().display());
    Ok(())
}

// ── Pass 2 — the master ─────────────────────────────────────────────────────

pub fn master() -> Result<(), Box<dyn std::error::Error>> {
    let root = work_root();
    std::fs::create_dir_all(root.join("sheets"))?;
    std::fs::create_dir_all(root.join("segments"))?;
    std::fs::create_dir_all(root.join("frames"))?;

    // The film reads its own audit. No manifest, no master — a caption
    // without a receipt is the one thing the ledger forbids.
    let probe = match Probe::load(&manifest_path()) {
        Some(p) if p.is_measured() => p,
        _ => {
            println!("no measured manifest — running the census first");
            census()?;
            let p = Probe::load(&manifest_path()).ok_or("census produced no manifest")?;
            if !p.is_measured() {
                return Err("the census produced an unmeasured manifest".into());
            }
            p
        }
    };
    println!(
        "keynote master — manifest in hand: {} frames · {} shapes · alive {:.3}s · {} crates",
        probe.frames,
        thousands(probe.shapes),
        probe.alive_seconds,
        probe.crates
    );

    let all = scenes();
    let starts = scene_starts(&all);
    let mut driver = make_driver();
    let mut renderer = make_renderer();
    let t0 = std::time::Instant::now();

    for (si, s) in all.iter().enumerate() {
        let n = s.frames();
        let seg = root.join("segments").join(format!("{:02}_{}.mp4", si + 1, s.name));
        let sheet = root.join("sheets").join(format!("{}_{}.png", s.id, s.name));

        if seg.exists() && sheet.exists() {
            println!("  {} {:<11} · already rendered, skipping", s.id, s.name);
            continue;
        }

        let scene_dir = root.join("frames").join(format!("{:02}_{}", si + 1, s.name));
        std::fs::remove_dir_all(&scene_dir).ok();
        std::fs::create_dir_all(&scene_dir)?;
        let stride = (n / 16).max(1);
        let scene_t0 = std::time::Instant::now();

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
            let _ctx = build_frame(&mut driver, s, starts[si], i, &probe);
            let (pixels, _report) =
                renderer.render_to_pixels(driver.scene(), W as u32, H as u32, BG)?;
            stdin.write_all(pixels.data())?;

            if i % stride == 0 {
                let k = i / stride;
                if k < 16 {
                    let img =
                        image::RgbaImage::from_raw(W as u32, H as u32, pixels.data().to_vec())
                            .ok_or("invalid frame")?;
                    img.save(scene_dir.join(format!("frame_{k:03}.png")))?;
                }
            }
            if i % 300 == 0 && i > 0 {
                println!(
                    "    {} · {:>3}% · {:.1} min in",
                    s.id,
                    i * 100 / n,
                    t0.elapsed().as_secs_f32() / 60.0
                );
            }
        }
        drop(stdin);
        if !ffmpeg.wait()?.success() {
            return Err(format!("ffmpeg failed on {}", s.id).into());
        }

        // The house contact sheet — fps=10, scale=480:-1, tile=4x4.
        let st = ProcCommand::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-framerate", "10"])
            .arg("-i")
            .arg(scene_dir.join("frame_%03d.png"))
            .arg("-vf")
            .arg("scale=480:-1,tile=4x4")
            .arg("-frames:v")
            .arg("1")
            .arg(&sheet)
            .status();
        match st {
            Ok(x) if x.success() => println!(
                "  {} {:<11} · {:>5} frames in {:.1}s · sheet ok",
                s.id,
                s.name,
                n,
                scene_t0.elapsed().as_secs_f32()
            ),
            _ => println!("  {} {:<11} · sheet FAILED", s.id, s.name),
        }
        // The frames were scratch. They do not survive the sheet.
        let _ = std::fs::remove_dir_all(&scene_dir);
    }

    // Assemble — concat the segments with a stream copy.
    let list = root.join("segments").join("list.txt");
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
    let mp4 = root.join("keynote.mp4");
    let st = ProcCommand::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "concat", "-safe", "0"])
        .arg("-i")
        .arg(&list)
        .arg("-c")
        .arg("copy")
        .arg("-movflags")
        .arg("+faststart")
        .arg(&mp4)
        .status()?;
    if !st.success() {
        return Err("concat failed".into());
    }

    println!(
        "\nmaster assembled — {} segments · {} frames · {:.1} min wall",
        all.len(),
        total_frames(),
        t0.elapsed().as_secs_f32() / 60.0
    );
    println!("  film   → {}", mp4.display());
    println!("  sheets → {}", root.join("sheets").display());
    Ok(())
}

// ── Preview — one scene, sixteen frames, a sheet ────────────────────────────

fn preview_scene(name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let probe = Probe::load(&manifest_path()).unwrap_or_else(|| Probe {
        // A preview may run before the census; it says so on the frame
        // rather than printing a number it did not measure.
        frames: 1,
        shapes: 1,
        crates: count_crates(),
        alive_seconds: 0.0,
        ..Probe::default()
    });
    let all = scenes();
    let key = name.to_ascii_lowercase();
    let si = all
        .iter()
        .position(|s| s.id.eq_ignore_ascii_case(&key) || s.name.contains(&key))
        .ok_or_else(|| format!("no scene matches {name:?}"))?;
    let s = &all[si];
    let starts = scene_starts(&all);

    let dir = work_root().join("preview").join(s.id.to_ascii_lowercase());
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir)?;

    let mut driver = make_driver();
    let mut renderer = make_renderer();
    let count = std::env::var("KEYNOTE_PREVIEW_FRAMES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(16);
    println!("preview {} {} · {count} frames at {}×{}", s.id, s.name, W as u32, H as u32);
    let n = s.frames();
    for k in 0..count {
        let i = ((k as f32 / (count.max(2) - 1) as f32) * (n - 1) as f32).round() as usize;
        let ctx = build_frame(&mut driver, s, starts[si], i, &probe);
        let start = std::time::Instant::now();
        let (pixels, report) = renderer.render_to_pixels(driver.scene(), W as u32, H as u32, BG)?;
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        let img = image::RgbaImage::from_raw(W as u32, H as u32, pixels.data().to_vec())
            .ok_or("invalid frame")?;
        img.save(dir.join(format!("frame_{k:03}.png")))?;
        println!(
            "  {k:>2} · t {:.3} · witness {} · shapes {} · layers {} · {ms:.1} ms",
            ctx.t, ctx.spine.witness, report.shapes, report.layers
        );
    }
    let _ = ProcCommand::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-framerate", "10"])
        .arg("-i")
        .arg(dir.join("frame_%03d.png"))
        .arg("-vf")
        .arg("scale=480:-1,tile=4x4")
        .arg("-frames:v")
        .arg("1")
        .arg(dir.join("sheet.png"))
        .status();
    println!("  sheet → {}", dir.join("sheet.png").display());
    Ok(())
}

/// Copy the shippable artifacts into the repository: the master and the
/// sheets, never the frames.
pub fn publish(dest: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let root = work_root();
    std::fs::create_dir_all(dest.join("sheets"))?;
    std::fs::copy(root.join("keynote.mp4"), dest.join("keynote.mp4"))?;
    std::fs::copy(root.join("manifest.txt"), dest.join("manifest.txt"))?;
    let mut n = 0;
    for e in std::fs::read_dir(root.join("sheets"))? {
        let e = e?;
        std::fs::copy(e.path(), dest.join("sheets").join(e.file_name()))?;
        n += 1;
    }
    println!("published → {} · mp4 + manifest + {n} sheets", dest.display());
    Ok(())
}
