//! master — the film's harness: the census (pass 1) and the master
//! render (pass 2), the single-scene preview, and the teaser cut.
//!
//! **The rig.** A frame is up to three scenes composed in one order:
//!
//! 1. **ground** — the room (`kit::room`), and under the Wright act the
//!    window's shadow. Full-bleed, screen space.
//! 2. **the world** — a pure scene's tree, or (the Wright act) the real
//!    window's own scene, appended at its placement transform so every
//!    glyph re-rasterises crisp at the placed size.
//! 3. **chrome** — the scene's own overlay (the `build` function's
//!    return), the movement label, the footer's receipt, the window's
//!    hairline.
//!
//! **The dissolve grammar.** The previous scene's final frame and the
//! new scene's live frame cross-dissolve — no dip to dark, no wipes —
//! held `CUT_IN` seconds, widened ×1.6 at the movement boundaries.
//! Where the incoming world is the window (the Wright act), the window
//! grows out of the previous scene's content box while its corners
//! curve in: the rectangle becoming the rounded rectangle.
//!
//! **The finishing pass.** Every frame goes through `vieww-effects`'
//! optics in linear light: a gentle bloom, a vignette, a grain — a
//! camera's worth of richness, never an effect you see.

use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::process::{Command as ProcCommand, Stdio};
use std::time::{Duration, Instant};

use vieww_effects::cpu::optics::Finish;
use vieww_foundation::{Color, Rect, Size, Transform};
use vieww_paint::native::NativeRenderer;
use vieww_paint::{Command, Scene};
use vieww_render::FrameDriver;
use vieww_widget::prelude::*;

use crate::film::{self, movement_of, movement_title, scenes, Kind, SceneDef, FPS, H, W};
use crate::m2_run;
use crate::wright_app::{self, Script};

/// Where this film's artifacts live while being built. `WRIGHT_FILM_OUT`
/// overrides it — the portable door.
pub(crate) fn work_root() -> PathBuf {
    wright_app::work_root()
}

/// The entrance dissolve, seconds — widened at the movement boundaries.
const CUT_IN: f32 = 0.45;

fn cut_len(si: usize) -> f32 {
    let all = scenes();
    let boundary = si == 0 || movement_of(all[si - 1].id) != movement_of(all[si].id);
    CUT_IN * if boundary { 1.6 } else { 1.0 }
}

// ── the rig ─────────────────────────────────────────────────────────────────

/// The two-pass state: drivers for the layers, the mounted window, the
/// dissolve's A frame.
struct Rig {
    /// The room.
    ground: FrameDriver,
    /// A pure scene's tree.
    pure: FrameDriver,
    /// The chrome: labels, receipts, the window's hairline.
    chrome: FrameDriver,
    /// The real window, mounted once for the whole Wright act.
    window: Option<wright_app::Mounted>,
    /// The previous scene's final frame, RGBA at raster size — the A of
    /// the A→B dissolve.
    prev: Option<Vec<u8>>,
}

impl Rig {
    fn new() -> Rig {
        Rig {
            ground: FrameDriver::new(Size::new(W, H)),
            pure: FrameDriver::new(Size::new(W, H)),
            chrome: FrameDriver::new(Size::new(W, H)),
            window: None,
            prev: None,
        }
    }

    fn ensure_window(&mut self) {
        if self.window.is_none() {
            self.window = Some(wright_app::Mounted::new());
        }
    }

    /// The Wright act's script for scene `id`.
    fn script_for(id: &str) -> Script {
        match id {
            "W06" => m2_run::w06_script(),
            "W07" => m2_run::w07_script(),
            "W08" => m2_run::w08_script(),
            "W10" => crate::m3_proof::w10_script(),
            _ => Script::default(),
        }
    }

    /// Build frame `i` of scene `si`. The scene's own clock and the
    /// film's absolute clock both drive it.
    fn build_frame(&mut self, s: &SceneDef, si: usize, i: usize, demo: &wright_app::Demo) {
        let abs = (film::scene_start(si) as f32 + i as f32) / FPS;
        let sec = i as f32 / FPS;
        let ctx = film::Ctx {
            t: sec / s.seconds,
            sec,
            demo,
        };

        // 1 · the ground: the room, always; under the window, its shadow.
        {
            let clock = abs;
            let is_wright = s.kind == Kind::Wright;
            let win = wright_app::window_place();
            let win_rect = wright_app::window_rect(win.0, win.1, win.2);
            self.ground
                .set_root(Positioned::fill().child(Painting::sized(
                    Size::new(W, H),
                    PaintWith::new(move |book: &mut Sketchbook, size| {
                        if is_wright {
                            m2_run::ground_under_window(
                                book,
                                size.width,
                                size.height,
                                clock,
                                win_rect,
                            );
                        } else {
                            crate::kit::room(book, size.width, size.height, clock);
                        }
                    }),
                )));
            self.ground
                .draw_frame_at(Duration::from_secs_f64(abs as f64));
        }

        // 2 · the world.
        match s.kind {
            Kind::Pure => {
                let node = (s.build)(&ctx);
                self.pure.set_root(node);
                self.pure.draw_frame_at(Duration::from_secs_f64(abs as f64));
            }
            Kind::Wright => {
                self.ensure_window();
                let script = Self::script_for(s.id);
                let mounted = self.window.as_mut().unwrap();
                // The script is the scene's clock; the film's clock drives
                // the window's own animations. Both are set per frame.
                let _ = mounted.draw(&script, sec, abs);
            }
        }

        // 3 · the chrome: the scene's own overlay (for Wright scenes),
        // the movement label, the footer.
        {
            let m = movement_of(s.id);
            let title = movement_title(m);
            let label_a = 1.0f32;
            let footer = footer_text(s, demo);
            let is_wright = s.kind == Kind::Wright;
            let win = wright_app::window_place();
            let win_rect = wright_app::window_rect(win.0, win.1, win.2);
            let mut layers: Vec<WidgetNode> = Vec::new();
            if is_wright {
                // The scene's own chrome (callouts, receipts).
                let over = (s.build)(&ctx);
                layers.push(over);
            }
            let ground_layers = layers;
            let _ = ground_layers;
            // The movement label + the window's hairline, on the canvas.
            let label_a2 = label_a;
            self.chrome.set_root(
                Stack::new()
                    .push(Positioned::fill().child(Painting::sized(
                        Size::new(W, H),
                        PaintWith::new(move |book: &mut Sketchbook, _size| {
                            if is_wright {
                                m2_run::window_edge(book, win_rect);
                            }
                            // The movement label's rule.
                            book.rect(
                                Rect::new(72.0, 88.0, 72.0 + 44.0 * label_a2, 89.5),
                                Color {
                                    a: (label_a2 * 255.0) as u8,
                                    ..film::ACCENT
                                },
                            );
                        }),
                    )))
                    .push(crate::kit::dlabel(
                        72.0,
                        44.0,
                        700.0,
                        36.0,
                        title,
                        16.0,
                        film::INK,
                        TextAlign::Left,
                        label_a,
                    ))
                    .push(crate::kit::mlabel(
                        W - 1120.0,
                        1028.0,
                        1050.0,
                        36.0,
                        footer,
                        15.5,
                        Color {
                            a: 215,
                            ..film::MUTED
                        },
                        TextAlign::Right,
                        1.0,
                    )),
            );
            self.chrome
                .draw_frame_at(Duration::from_secs_f64(abs as f64));
        }
    }

    /// Compose this frame's final scene: ground + world + chrome, with
    /// the entrance dissolve's pixel blend over the previous scene's
    /// final frame.
    fn compose(&mut self, s: &SceneDef, si: usize, i: usize) -> (Scene, Option<f32>) {
        let abs = (film::scene_start(si) as f32 + i as f32) / FPS;
        let sec = i as f32 / FPS;
        let mut frame = Scene::default();
        let full = Rect::new(0.0, 0.0, W, H);
        frame.append(self.ground.scene(), Transform::IDENTITY);
        match s.kind {
            Kind::Pure => frame.append(self.pure.scene(), Transform::IDENTITY),
            Kind::Wright => {
                // The window grows out of the previous scene's content
                // box during the entrance dissolve, then holds.
                let (scale, dx, dy) = wright_app::window_place();
                let e = if sec < cut_len(si) {
                    film::ease(sec, cut_len(si))
                } else {
                    1.0
                };
                // Grow from a centred 0.72× to the full placement.
                let k = 0.72 + 0.28 * e;
                let sc = scale * k;
                let w = wright_app::APP_W * sc;
                let h = wright_app::APP_H * sc;
                let cx = dx + (wright_app::APP_W * scale) * 0.5;
                let cy = dy + (wright_app::APP_H * scale) * 0.5;
                let ox = cx - w * 0.5;
                let oy = cy - h * 0.5;
                frame.append(
                    self.window.as_ref().unwrap().driver.scene(),
                    Transform::new(sc, 0.0, 0.0, sc, ox, oy),
                );
            }
        }
        frame.append(self.chrome.scene(), Transform::IDENTITY);
        let _ = full;
        // The dissolve's weight, eased, while the entrance runs.
        let cut = if sec < cut_len(si) && si > 0 {
            Some(film::ease(sec, cut_len(si)))
        } else {
            None
        };
        let _ = abs;
        (frame, cut)
    }
}

/// The footer's one-line receipt for a scene.
fn footer_text(s: &SceneDef, demo: &wright_app::Demo) -> String {
    match s.id {
        "W00" => "cold open · the question".into(),
        "W01" => "the request, typed as typed".into(),
        "W02" => "the blueprint, assembled from the request".into(),
        "W03" => "the confirm gate, code-generated".into(),
        "W04" => "the compiler's own staging".into(),
        "W05" => format!("guardrails, drawn — this run cost ${:.4}", demo.result.cost),
        "W06" => format!("the real window · {} real events", demo.events.len()),
        "W07" => "the Mailer's ask, dragged right".into(),
        "W08" => "Space to peek · E keeps it".into(),
        "W09" => format!(
            "wright verify · {} checks, all real",
            demo.receipt.checks.len()
        ),
        "W10" => "delivered · and the receipt in the window".into(),
        "W11" => "one change · only what changed runs".into(),
        "W12" => "the hold".into(),
        _ => s.name.into(),
    }
}

// ── the finishing pass ──────────────────────────────────────────────────────

/// Is the finishing pass on? (`WF_FINISH=0` turns it off for A/B.)
fn finish_on() -> bool {
    std::env::var("WF_FINISH").map_or(true, |v| v != "0")
}

fn base_finish() -> Finish {
    Finish {
        bloom_threshold: 0.5,
        bloom_knee: 0.3,
        bloom_intensity: 0.22,
        bloom_radius: 0.8,
        vignette: 0.14,
        grain: 0.028,
        ..Finish::default()
    }
}

/// The finishing pass over one frame's pixels, in place. `frame` seeds
/// the grain (the studio film's own arithmetic).
fn finish(px: &mut [u8], w: usize, h: usize, cut: Option<f32>, frame: u32) {
    if !finish_on() {
        return;
    }
    let mut f = base_finish();
    if let Some(e) = cut {
        // A light swell through the dissolve — the movement boundary
        // breathes, the studio film's own grammar.
        let swell = (e * std::f32::consts::PI).sin();
        f.bloom_intensity += 0.30 * swell;
        f.exposure = 1.0 + 0.06 * swell;
    }
    f.apply_rgba8(px, w, h, frame.wrapping_mul(2_654_435_761));
}

// ── pass 1 · the census ─────────────────────────────────────────────────────

/// Walk every frame, count the command stream, sample raster times;
/// write the manifest and the samples. `WF_ONLY=<ids>` splits the walk.
pub(crate) fn census() -> Result<(), Box<dyn std::error::Error>> {
    let all = scenes();
    let n_total = film::total_frames();
    let root = work_root();
    std::fs::create_dir_all(&root)?;
    println!(
        "wright_film census — {} scenes · {} frames · every number derived",
        all.len(),
        n_total
    );
    let demo = wright_app::demo();

    let only: Option<Vec<String>> = std::env::var("WF_ONLY").ok().map(|v| {
        v.split(',')
            .map(|s| s.trim().to_ascii_uppercase())
            .collect()
    });
    let mut rig = Rig::new();
    let mut renderer = NativeRenderer::new();
    let mut shapes = 0u64;
    let mut glyph_runs = 0u64;
    let mut layers = 0u64;
    let mut ms_samples: Vec<f32> = Vec::new();
    let mut rows: Vec<String> = Vec::new();
    let mut counted = 0u64;

    for (si, s) in all.iter().enumerate() {
        if only.as_ref().is_some_and(|o| !o.iter().any(|x| x == s.id)) {
            continue;
        }
        let n = s.frames();
        let t0 = Instant::now();
        let sample_every = (n / 12).max(1);
        // The Wright act applies its script per frame.
        let script = Rig::script_for(s.id);
        for i in 0..n {
            let _ = &script;
            rig.build_frame(s, si, i, demo);
            let (frame, _) = rig.compose(s, si, i);
            for c in frame.commands() {
                match c {
                    Command::FillRect { .. }
                    | Command::FillPath { .. }
                    | Command::StrokePath { .. } => shapes += 1,
                    Command::DrawGlyphs { .. } => glyph_runs += 1,
                    Command::PushLayer { .. } => layers += 1,
                    _ => {}
                }
            }
            if i % sample_every == 0 {
                let t = Instant::now();
                let (f2, _) = rig.compose(s, si, i);
                let mut px = render_rgba(&mut renderer, &f2);
                finish(&mut px, W as usize, H as usize, None, i as u32);
                std::hint::black_box(&px);
                let ms = t.elapsed().as_secs_f32() * 1000.0;
                ms_samples.push(ms);
                rows.push(format!("{},{},{}", s.id, f2.commands().len(), ms));
            }
            counted += 1;
        }
        println!(
            "  {} {} · {} frames · census {:.1}s",
            s.id,
            s.name,
            n,
            t0.elapsed().as_secs_f32()
        );
    }
    ms_samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = ms_samples.get(ms_samples.len() / 2).copied().unwrap_or(0.0);
    std::fs::write(
        root.join("manifest.txt"),
        format!(
            "frames={}\nshapes={}\nglyph_runs={}\nlayers={}\nmedian_ms={:.2}\nspine_events={}\nreceipt_checks={}\n",
            counted, shapes, glyph_runs, layers, median, demo.events.len(), demo.receipt.checks.len()
        ),
    )?;
    std::fs::write(root.join("samples.txt"), rows.join("\n") + "\n")?;
    println!(
        "census — {counted} frames · median {median:.1} ms → {}",
        root.display()
    );
    Ok(())
}

// ── pass 2 · the master ─────────────────────────────────────────────────────

/// Render every scene to its own segment (RGBA → x264), the picks to a
/// contact sheet, and concat the segments into `wright_film.mp4`.
/// `WF_ONLY=<ids>` renders a subset (resume-friendly: a segment that
/// already exists, with its sheet, is skipped).
pub(crate) fn master() -> Result<(), Box<dyn std::error::Error>> {
    let root = work_root();
    let seg_dir = root.join("segments");
    std::fs::create_dir_all(root.join("sheets"))?;
    std::fs::create_dir_all(root.join("cut"))?;
    std::fs::create_dir_all(&seg_dir)?;

    let demo = wright_app::demo();
    let only: Option<Vec<String>> = std::env::var("WF_ONLY").ok().map(|v| {
        v.split(',')
            .map(|s| s.trim().to_ascii_uppercase())
            .collect()
    });
    let all = scenes();
    let mut renderer = NativeRenderer::new();
    let mut rig = Rig::new();
    let t0 = Instant::now();

    for (si, s) in all.iter().enumerate() {
        let n = s.frames();
        let seg = seg_dir.join(format!("{:02}_{}.mp4", si + 1, s.name));
        let sheet = root.join("sheets").join(format!("{}_{}.png", s.id, s.name));
        let wanted = only.as_ref().is_none_or(|o| o.iter().any(|x| x == s.id));
        if !wanted || seg.exists() && sheet.exists() {
            println!("  {} {} · skipping", s.id, s.name);
            continue;
        }

        // The A of this scene's entrance dissolve.
        if si > 0 {
            let prev = &all[si - 1];
            let path = root
                .join("cut")
                .join(format!("{:02}_{}.png", si, prev.name));
            let px = std::fs::read(&path)
                .ok()
                .and_then(|b| image::load_from_memory(&b).ok())
                .map(|img| img.to_rgba8().into_raw());
            rig.prev = px;
        } else {
            rig.prev = None;
        }

        let scene_dir = root
            .join("frames")
            .join(format!("{:02}_{}", si + 1, s.name));
        std::fs::remove_dir_all(&scene_dir).ok();
        std::fs::create_dir_all(&scene_dir)?;
        let scene_t0 = Instant::now();

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
                &format!("{}x{}", W as u32, H as u32),
                "-framerate",
                &format!("{FPS}"),
            ])
            .args([
                "-i", "-", "-c:v", "libx264", "-preset", "medium", "-crf", "16", "-tune", "film",
                "-pix_fmt", "yuv420p",
            ])
            // aq-mode=3: the encoder's half of the light rule — dark
            // ramps keep their bits (the studio film's own lesson).
            .args(["-x264-params", "aq-mode=3"])
            .arg(&seg)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let mut stdin = ffmpeg.stdin.take().ok_or("no ffmpeg stdin")?;

        let picks = sheet_picks(n);
        let script = Rig::script_for(s.id);
        let mut last_buf: Option<Vec<u8>> = None;
        for i in 0..n {
            let _ = &script;
            rig.build_frame(s, si, i, demo);
            let (frame, cut) = rig.compose(s, si, i);
            let mut buf = render_rgba(&mut renderer, &frame);
            // The dissolve: blend this frame over the previous scene's
            // final frame while the cut runs.
            if let (Some(e), Some(a)) = (cut, rig.prev.as_ref()) {
                blend(&mut buf, a, e);
            }
            finish(
                &mut buf,
                W as usize,
                H as usize,
                cut,
                (film::scene_start(si) + i) as u32,
            );
            stdin.write_all(&buf)?;
            if let Some(k) = picks.iter().position(|p| *p == i) {
                let img = image::RgbaImage::from_raw(W as u32, H as u32, buf.clone())
                    .ok_or("invalid frame")?;
                img.save(scene_dir.join(format!("frame_{k:03}.png")))?;
            }
            if i + 1 == n {
                last_buf = Some(buf);
            }
            if i % 240 == 0 && i > 0 {
                println!(
                    "  {} {} · {:>3}% · {:.1} min in",
                    s.id,
                    s.name,
                    i * 100 / n,
                    t0.elapsed().as_secs_f32() / 60.0
                );
            }
        }
        drop(stdin);
        let status = ffmpeg.wait()?;
        if !status.success() {
            return Err(format!("ffmpeg exited {status} on {}", s.id).into());
        }
        if let Some(buf) = last_buf {
            if si + 1 < all.len() {
                let img =
                    image::RgbaImage::from_raw(W as u32, H as u32, buf).ok_or("invalid frame")?;
                img.save(
                    root.join("cut")
                        .join(format!("{:02}_{}.png", si + 1, s.name)),
                )?;
            }
        }
        tile_sheet(&scene_dir, &sheet);
        let _ = std::fs::remove_dir_all(&scene_dir);
        println!(
            "  {} {} · done in {:.1}s",
            s.id,
            s.name,
            scene_t0.elapsed().as_secs_f32()
        );
    }

    // Assemble — concat the segments (stream copy).
    concat(&seg_dir, &root.join("wright_film.mp4"), &all)?;
    // The teaser: the film's own first movement + the run + the receipt
    // + the end card — the 60-second door.
    let teaser: Vec<SceneDef> = all
        .iter()
        .filter(|s| ["W00", "W01", "W06", "W09", "W11", "W12"].contains(&s.id))
        .cloned()
        .collect();
    concat(&seg_dir, &root.join("wright_teaser.mp4"), &teaser)?;
    println!(
        "\nmaster assembled — {:.1} min wall → {}",
        t0.elapsed().as_secs_f32() / 60.0,
        root.display()
    );
    Ok(())
}

/// Concat the named scenes' segments into one mp4 (stream copy).
fn concat(
    seg_dir: &std::path::Path,
    out: &std::path::Path,
    list: &[SceneDef],
) -> Result<(), Box<dyn std::error::Error>> {
    let list_path = seg_dir.join(format!(
        "list-{}.txt",
        out.file_name().unwrap().to_string_lossy()
    ));
    let mut body = String::new();
    for s in list {
        let i = scenes().iter().position(|x| x.id == s.id).unwrap();
        body.push_str(&format!("file '{:02}_{}.mp4'\n", i + 1, s.name));
    }
    std::fs::write(&list_path, body)?;
    let status = ProcCommand::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "concat", "-safe", "0"])
        .arg("-i")
        .arg(&list_path)
        .args(["-c", "copy", "-movflags", "+faststart"])
        .arg(out)
        .status()?;
    if !status.success() {
        return Err("concat failed".into());
    }
    println!(
        "  {} → {}",
        out.file_name().unwrap().to_string_lossy(),
        out.display()
    );
    Ok(())
}

/// Render a scene's frame to raw RGBA.
fn render_rgba(renderer: &mut NativeRenderer, frame: &Scene) -> Vec<u8> {
    let (px, _) = renderer
        .render_to_pixels(frame, W as u32, H as u32, film::ROOM)
        .expect("raster");
    px.data().to_vec()
}

/// A→B blend: `b` (this frame) over `a` (the previous final frame) at
/// weight `e` (0 = all A, 1 = all B).
fn blend(b: &mut [u8], a: &[u8], e: f32) {
    if a.len() != b.len() {
        return;
    }
    let e = e.clamp(0.0, 1.0);
    for (bb, aa) in b.iter_mut().zip(a.iter()) {
        *bb = (*aa as f32 * (1.0 - e) + *bb as f32 * e).round() as u8;
    }
}

/// Which frames tile into the contact sheet (16, spread over the scene).
fn sheet_picks(n: usize) -> Vec<usize> {
    if n <= 16 {
        (0..n).collect()
    } else {
        (0..16).map(|k| k * (n - 1) / 15).collect()
    }
}

/// Tile a scene's picked frames into a 4×4 contact sheet.
fn tile_sheet(dir: &std::path::Path, out: &std::path::Path) -> Option<()> {
    let mut imgs = Vec::new();
    for k in 0..16 {
        let p = dir.join(format!("frame_{k:03}.png"));
        if p.exists() {
            imgs.push(image::open(&p).ok()?.to_rgba8());
        }
    }
    if imgs.is_empty() {
        return None;
    }
    let (tw, th) = (480u32, 270u32);
    let mut sheet = image::RgbaImage::new(tw * 4, th * 4);
    for (k, img) in imgs.iter().take(16).enumerate() {
        let resized = image::imageops::resize(img, tw, th, image::imageops::FilterType::Triangle);
        let (x, y) = ((k % 4) as i64 * tw as i64, (k / 4) as i64 * th as i64);
        image::imageops::overlay(&mut sheet, &resized, x, y);
    }
    sheet.save(out).ok()?;
    Some(())
}

// ── the preview ─────────────────────────────────────────────────────────────

/// One scene, previewed: frames to PNGs under `preview/<id>/` (every
/// 12th) plus the first frame at full size — the audit door.
pub(crate) fn preview(id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let all = scenes();
    let si = all
        .iter()
        .position(|s| s.id.eq_ignore_ascii_case(id) || s.name.eq_ignore_ascii_case(id))
        .ok_or_else(|| format!("no scene {id}"))?;
    let s = &all[si];
    let root = work_root().join("preview").join(s.id);
    std::fs::create_dir_all(&root)?;
    let demo = wright_app::demo();
    let mut renderer = NativeRenderer::new();
    let mut rig = Rig::new();
    let n = s.frames();
    let script = Rig::script_for(s.id);
    let mut written = 0;
    let _ = &script;
    for i in 0..n {
        if i % 12 != 0 && i != n - 1 {
            continue;
        }
        rig.build_frame(s, si, i, demo);
        let (frame, _) = rig.compose(s, si, i);
        let (png, _) = renderer.render_to_png(&frame, W as u32, H as u32, film::ROOM)?;
        std::fs::write(root.join(format!("f_{i:05}.png")), png)?;
        written += 1;
    }
    println!(
        "{} {} · {written} preview frames → {}",
        s.id,
        s.name,
        root.display()
    );
    Ok(())
}

/// Entry point — dispatch on the CLI word.
pub(crate) fn run(mode: &str) -> Result<(), Box<dyn std::error::Error>> {
    match mode {
        "census" => census(),
        "master" => master(),
        other => preview(other),
    }
}
