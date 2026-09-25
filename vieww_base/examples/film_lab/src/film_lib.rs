//! film_lib — the launch film's shared foundation.
//!
//! Everything the film's element experiments agree on lives here: the palette,
//! the clock discipline, the deterministic RNG, the easing curves, the render
//! harness, the contact-sheet writer and the anim.gif assembler. An experiment
//! file should contain *content*, never plumbing.
//!
//! **The house rules, inherited from the graph (§3, §4.6):**
//! - No number in a caption is typed by a human; anything printed is measured.
//! - Every experiment renders through vieww's own rasterizer — nothing in post.
//! - Determinism: `t` in `[0, 1]` sampled by the harness; RNG seeded per
//!   experiment; two runs of the same experiment are byte-identical.

use std::path::{Path, PathBuf};
use std::time::Duration;

use vieww_foundation::{Color, Size};
use vieww_paint::native::NativeRenderer;
use vieww_render::FrameDriver;
use vieww_widget::WidgetNode;

pub const CANVAS_W: f32 = 1280.0;
pub const CANVAS_H: f32 = 720.0;
pub const CANVAS: Size = Size::new(CANVAS_W, CANVAS_H);

// ── The palette — from the author's own reference sheets ───────────────────
//
// sheet-bx / sheet-vf read as: near-black grounds, off-white ink, an electric
// violet accent, deep purple / magenta / cyan nebula tones. These constants
// are the film's; scenes may vary them, experiments should not.

pub const BG: Color = Color::rgb(10, 10, 12);
pub const BG_DEEP: Color = Color::rgb(6, 6, 9);
pub const SURFACE: Color = Color::rgb(18, 18, 22);
pub const INK: Color = Color::rgb(240, 240, 242);
pub const MUTED: Color = Color::rgb(139, 148, 158);
pub const FAINT: Color = Color::rgb(90, 98, 110);

pub const VIOLET: Color = Color::rgb(139, 92, 246);
pub const VIOLET_SOFT: Color = Color::rgb(167, 139, 250);
pub const VIOLET_DEEP: Color = Color::rgb(76, 29, 149);
pub const MAGENTA: Color = Color::rgb(190, 24, 93);
pub const CYAN: Color = Color::rgb(14, 165, 233);
pub const CYAN_SOFT: Color = Color::rgb(103, 232, 249);
pub const MINT: Color = Color::rgb(52, 211, 153);
pub const AMBER: Color = Color::rgb(245, 158, 11);
pub const RED: Color = Color::rgb(248, 81, 73);

pub fn alpha(c: Color, a: f32) -> Color {
    Color::rgba(c.r, c.g, c.b, (a * 255.0).clamp(0.0, 255.0) as u8)
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    a.lerp(b, t.clamp(0.0, 1.0))
}

/// A rect from origin + size — the (x, y, w, h) habit. `Rect::new` takes
/// *edges* (left, top, right, bottom); this is the conversion the film's
/// own boards keep needing. (Origin-anchored rects are identical either
/// way; offset rects are not — this is the safe spelling.)
pub fn xywh(x: f32, y: f32, w: f32, h: f32) -> vieww_foundation::Rect {
    vieww_foundation::Rect::new(x, y, x + w.max(0.0), y + h.max(0.0))
}

/// Lighten toward white (a "tint").
pub fn tint(c: Color, t: f32) -> Color {
    mix(c, Color::WHITE, t)
}

/// Darken toward black (a "shade").
pub fn shade(c: Color, t: f32) -> Color {
    mix(c, Color::BLACK, t)
}

/// Multiply a color's channels by a scalar — the Lambert term's friend.
pub fn scaled(c: Color, k: f32) -> Color {
    Color::rgb(
        (c.r as f32 * k).clamp(0.0, 255.0) as u8,
        (c.g as f32 * k).clamp(0.0, 255.0) as u8,
        (c.b as f32 * k).clamp(0.0, 255.0) as u8,
    )
}

// ── The clock ───────────────────────────────────────────────────────────────
//
// Experiments are pure functions of `t` in `[0, 1]` over a scene duration.
// The harness owns the mapping to wall-clock film time; nothing inside an
// experiment may consult the real clock — determinism is the film's spine.

/// The wait's cadence: a 24 Hz clock sampled inside a 60 Hz render.
/// Returns the *held* 24 Hz value for film-time `t` (seconds) — the judder
/// is real 2-3-2-3 hold pattern, not a rate change.
pub fn held_24_in_60(t: f32) -> f32 {
    let step = (t * 24.0).floor() / 24.0;
    (step * 60.0).round() / 60.0
}

// ── Deterministic RNG ───────────────────────────────────────────────────────
//
/// xorshift64* — tiny, seedable, reproducible. The film's noise never comes
/// from the machine's entropy; grain re-renders to the byte.
#[derive(Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed | 1,
        }
    }

    pub fn u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    /// Uniform in `[0, 1)`.
    pub fn f01(&mut self) -> f32 {
        (self.u64() >> 11) as f32 / (1u64 << 53) as f32
    }

    /// Uniform in `[-1, 1]`.
    pub fn sym(&mut self) -> f32 {
        self.f01() * 2.0 - 1.0
    }
}

// ── Easing ──────────────────────────────────────────────────────────────────

pub fn clamp01(t: f32) -> f32 {
    t.clamp(0.0, 1.0)
}

pub fn smoothstep(t: f32) -> f32 {
    let t = clamp01(t);
    t * t * (3.0 - 2.0 * t)
}

pub fn ease_out_cubic(t: f32) -> f32 {
    let t = clamp01(t);
    1.0 - (1.0 - t).powi(3)
}

pub fn ease_out_expo(t: f32) -> f32 {
    let t = clamp01(t);
    if t >= 1.0 { 1.0 } else { 1.0 - 2.0f32.powf(-10.0 * t) }
}

pub fn ease_in_out(t: f32) -> f32 {
    let t = clamp01(t);
    if t < 0.5 { 2.0 * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(2) / 2.0 }
}

pub fn ease_out_back(t: f32) -> f32 {
    let t = clamp01(t);
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

/// An *analytic* underdamped spring settle — for when a closed form beats a
/// ticker (scrub windows, dash-phase ramps). The real `SpringAnimation` is
/// used wherever interruptibility matters; this is its scrub-safe shadow.
pub fn spring_out(t: f32, omega: f32, zeta: f32) -> f32 {
    let t = clamp01(t);
    let decay = (-zeta * omega * t).exp();
    1.0 - decay * ((1.0 - zeta * zeta).sqrt() * omega * t).cos()
}

/// Triangle window in `[0, 1]` — the ripple envelope.
pub fn tri(t: f32) -> f32 {
    let t = t.fract();
    if t < 0.5 { t * 2.0 } else { 2.0 - t * 2.0 }
}

// ── The render harness ──────────────────────────────────────────────────────

/// One experiment in the registry.
pub struct Experiment {
    pub name: &'static str,
    /// Film-time seconds this experiment spans.
    pub seconds: f32,
    /// Frames to render (harness samples `t = i / (frames - 1)`).
    pub frames: usize,
    /// `t` in `[0, 1]` → the frame's widget tree.
    pub build: fn(f32) -> WidgetNode,
    /// Optional pixel probe, run once on the **last** rendered frame — the
    /// instrument plates' door onto the actual output buffer, so a receipt
    /// can state measured pixel facts (the U-15 corner probes) rather than
    /// geometry it was told. Returns printed lines; every number in them
    /// was read out of the rasterizer's own output.
    pub probe: Option<fn(&image::RgbaImage) -> Vec<String>>,
    /// Optional per-frame hook — the endurance instrument (round 7): called
    /// after every frame with the receipt, the frame index, and the measured
    /// build/raster split, so a plate can record a *time series* (RSS, drift)
    /// rather than only a summary. The series lands in metrics.txt as
    /// `series=` lines — the receipt is allowed to be a curve.
    pub frame_hook: Option<fn(&mut Receipt, usize, f64, f64)>,
}

impl Experiment {
    /// The usual spelling — no probe, no series.
    pub const fn plain(
        name: &'static str,
        seconds: f32,
        frames: usize,
        build: fn(f32) -> WidgetNode,
    ) -> Self {
        Self {
            name,
            seconds,
            frames,
            build,
            probe: None,
            frame_hook: None,
        }
    }
}

/// Resident set size in KiB, read from the kernel — the endurance axis'
/// ground truth. `0` off Linux (the bench is Linux; the number is honest
/// about its own absence rather than about someone else's kernel).
#[must_use]
pub fn rss_kib() -> u64 {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kb: u64 = rest
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse()
                .unwrap_or(0);
            return kb;
        }
    }
    0
}

/// The measured outcome of one rendered experiment — printed, never guessed.
#[derive(Default)]
pub struct Receipt {
    pub frames: usize,
    pub shapes: u64,
    pub glyph_runs: u64,
    pub layers: u64,
    /// Blurred (filtered) layers — U-06's number, the one the guard
    /// asserts and the receipts now carry.
    pub filtered_layers: u64,
    /// Fills whose path carried open subpaths — the silent-petal census.
    pub open_subpath_fills: u64,
    /// Lines read out of the output buffer by the experiment's probe, if
    /// it has one — measured pixel facts.
    pub probe_lines: Vec<String>,
    /// The time series — round 7's endurance instrument. Written to
    /// metrics.txt as `series=` lines, after the summary fields.
    pub series: Vec<String>,
    /// Tree construction + layout (set_root + draw_frame_at), split from
    /// the raster — the simulation axis asked where the frame budget
    /// actually goes, and "build" was the half nobody had measured.
    pub build_ms_total: f64,
    pub build_ms_worst: f64,
    pub render_ms_total: f64,
    pub render_ms_worst: f64,
}

impl Receipt {
    pub fn print(&self, name: &str) {
        println!("  {name}");
        println!(
            "    frames {}/{} · shapes {} (mean {:.1}) · glyph_runs {} · layers {}",
            self.frames,
            self.frames,
            self.shapes,
            self.shapes as f64 / self.frames.max(1) as f64,
            self.glyph_runs,
            self.layers
        );
        println!(
            "    filtered {} · open_subpath_fills {}",
            self.filtered_layers, self.open_subpath_fills
        );
        println!(
            "    build mean {:.2} ms · worst {:.2} ms",
            self.build_ms_total / self.frames.max(1) as f64,
            self.build_ms_worst
        );
        for line in &self.probe_lines {
            println!("    probe: {line}");
        }
        println!(
            "    render mean {:.2} ms · worst {:.2} ms",
            self.render_ms_total / self.frames.max(1) as f64,
            self.render_ms_worst
        );
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let mut body = format!(
            "frames={}\nshapes={}\nglyph_runs={}\nlayers={}\nfiltered_layers={}\nopen_subpath_fills={}\nbuild_mean_ms={:.3}\nbuild_worst_ms={:.3}\nrender_mean_ms={:.3}\nrender_worst_ms={:.3}\n",
            self.frames, self.shapes, self.glyph_runs, self.layers,
            self.filtered_layers, self.open_subpath_fills,
            self.build_ms_total / self.frames.max(1) as f64,
            self.build_ms_worst,
            self.render_ms_total / self.frames.max(1) as f64,
            self.render_ms_worst,
        );
        for line in &self.probe_lines {
            body.push_str(&format!("probe={line}\n"));
        }
        for line in &self.series {
            body.push_str(&format!("series={line}\n"));
        }
        std::fs::write(dir.join("metrics.txt"), body)
    }
}

/// Render one experiment end-to-end: frames → PNGs → metrics.
/// Returns the receipt; the caller owns contact-sheet assembly (ffmpeg).
pub fn render(experiment: &Experiment, out_dir: &Path) -> Result<Receipt, Box<dyn std::error::Error>> {
    render_with(experiment, out_dir, CANVAS)
}

/// Render at a chosen canvas size — the hero frame's 1920×1080 path; every
/// other experiment goes through [`render`] at the lab standard.
pub fn render_with(
    experiment: &Experiment,
    out_dir: &Path,
    canvas: Size,
) -> Result<Receipt, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(out_dir)?;

    let mut driver = FrameDriver::new(canvas);
    // The film renders text; the default font store is embedded-only subsets.
    // This one line is the difference between legible captions and a panic.
    driver.use_system_fonts();
    let mut renderer = NativeRenderer::new();
    // The U-06 discipline, demonstrated where it lives: a frame that wants
    // more than 32 blurred layers is a bug in the grouping, not an ambition
    // — the guard fires named in debug builds, and the receipt carries the
    // number in every build.
    renderer.guard_filtered_layers_below(32);

    let mut receipt = Receipt {
        frames: experiment.frames,
        ..Receipt::default()
    };

    for i in 0..experiment.frames {
        let t = if experiment.frames > 1 {
            i as f32 / (experiment.frames - 1) as f32
        } else {
            0.0
        };

        // The budget split, measured where it happens: tree construction +
        // layout on one stopwatch, rasterisation on the other. Round 7's
        // simulation axis asked the question; every plate gets the answer.
        let build_start = std::time::Instant::now();
        driver.set_root((experiment.build)(t));
        driver.draw_frame_at(Duration::from_secs_f64((t * experiment.seconds) as f64));
        let build_elapsed = build_start.elapsed().as_secs_f64() * 1000.0;
        receipt.build_ms_total += build_elapsed;
        receipt.build_ms_worst = receipt.build_ms_worst.max(build_elapsed);

        // Command-stream dump for seam hunting (first frame only).
        if i == 0 && std::env::var("FILM_DUMP_CMDS").is_ok() {
            let limit = std::env::var("FILM_DUMP_CMDS")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(40);
            for (ci, cmd) in driver.scene().commands().iter().take(limit).enumerate() {
                let text = format!("{cmd:?}");
                let short = if text.len() > 700 {
                    format!("{} ...", &text[..700])
                } else {
                    text
                };
                println!("  cmd[{ci:03}] {short}");
            }
            println!("  total commands: {}", driver.scene().commands().len());
        }

        let start = std::time::Instant::now();
        let (pixels, report) = renderer.render_to_pixels(
            driver.scene(),
            canvas.width as u32,
            canvas.height as u32,
            BG,
        )?;
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        receipt.render_ms_total += elapsed;
        receipt.render_ms_worst = receipt.render_ms_worst.max(elapsed);

        // The endurance instrument: the plate may want a per-frame record.
        if let Some(hook) = experiment.frame_hook {
            hook(&mut receipt, i, build_elapsed, elapsed);
        }
        receipt.shapes += report.shapes as u64;
        receipt.glyph_runs += report.glyph_runs as u64;
        receipt.layers += report.layers as u64;
        receipt.filtered_layers += report.filtered_layers as u64;
        receipt.open_subpath_fills += report.open_subpath_fills as u64;

        let image = image::RgbaImage::from_raw(
            canvas.width as u32,
            canvas.height as u32,
            pixels.data().to_vec(),
        )
            .ok_or("invalid RGBA frame dimensions")?;
        image.save(out_dir.join(format!("frame_{i:03}.png")))?;

        // The pixel probe, on the last frame only — one read of the real
        // output buffer, so a receipt's pixel facts are measurements.
        if i + 1 == experiment.frames {
            if let Some(probe) = experiment.probe {
                receipt.probe_lines = probe(&image);
            }
        }
    }

    receipt.save(out_dir)?;
    Ok(receipt)
}

/// Assemble the contact sheet with ffmpeg — the house convention
/// (`fps=10, scale=480:-1, tile=4x4`), straight from the collaboration
/// protocol. Runs ffmpeg as a subprocess; ignores failure (the frames are
/// the artifact, the sheet is the review aid).
pub fn contact_sheet(out_dir: &Path, tile: &str) -> Option<PathBuf> {
    contact_sheet_strided(out_dir, tile, 1)
}

/// The strided sheet — round 7's endurance axis: a 256-frame run still
/// audits as a 4×4 grid, sampled every `stride` frames so the sheet stays
/// the review surface it always was.
///
/// **The hardlink detour, and why:** reading 256 full-res PNGs through a
/// `select` filter was SIGKILLed on this 4 GB bench — the page cache for
/// ~380 MB of inputs plus the filter's own buffers crossed the cgroup
/// ceiling mid-graph (the hero4k memory lesson, wearing ffmpeg's coat).
/// The fix is the same discipline the OOM finding prescribed for the
/// master: **chunk the pass**. The strided subset is hardlinked into a
/// temp dir (no data duplicated, no cache doubled) and the ordinary
/// one-pass recipe runs on that.
pub fn contact_sheet_strided(out_dir: &Path, tile: &str, stride: usize) -> Option<PathBuf> {
    let sheet = out_dir.join("sheet.png");
    if stride <= 1 {
        let frames = out_dir.join("frame_%03d.png");
        let status = std::process::Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-framerate", "10"])
            .arg("-i").arg(&frames)
            .arg("-vf").arg(format!("scale=480:-1,tile={tile}"))
            .arg("-frames:v").arg("1")
            .arg(&sheet)
            .status();
        return matches!(status, Ok(s) if s.success()).then_some(sheet);
    }

    let tmp = out_dir.join("_stride_tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).ok()?;
    let selected = hardlink_strided(out_dir, &tmp, stride)?;
    let frames = tmp.join("frame_%03d.png");
    let status = std::process::Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-framerate", "10"])
        .arg("-i").arg(&frames)
        .arg("-vf").arg(format!("scale=480:-1,tile={tile}"))
        .arg("-frames:v").arg("1")
        .arg(&sheet)
        .status();
    let _ = std::fs::remove_dir_all(&tmp);
    match status {
        Ok(s) if s.success() => {
            let _ = selected;
            Some(sheet)
        }
        _ => None,
    }
}

/// Hardlink every `stride`-th frame into `tmp` as a contiguous sequence.
/// Returns how many frames were linked. (A hardlink is a directory entry,
/// not a copy: no extra page cache, no extra disk — the OOM lesson,
/// applied as plumbing.)
fn hardlink_strided(out_dir: &Path, tmp: &Path, stride: usize) -> Option<usize> {
    let mut names: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(out_dir) {
        for entry in rd.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with("frame_") && name.ends_with(".png") {
                    names.push(name.to_string());
                }
            }
        }
    }
    names.sort();
    let mut n = 0usize;
    for name in names.iter().step_by(stride) {
        let src = out_dir.join(name);
        let dst = tmp.join(format!("frame_{n:03}.png"));
        if std::fs::hard_link(&src, &dst).is_err() {
            // A filesystem without hardlinks (or a stale entry): copy.
            if std::fs::copy(&src, &dst).is_err() {
                return None;
            }
        }
        n += 1;
    }
    (n > 0).then_some(n)
}

/// Assemble the palette-optimised loop with ffmpeg — the third artifact.
/// Every render ships **anim.gif + sheet.png + metrics.txt**: the GIF is
/// the motion receipt for human eyes (it loops inline in browsers and on
/// GitHub, needs no codec, and at the lab's flat-colour vector content it
/// is the smaller carrier — measured, not assumed); the sheet stays the
/// audit surface, because motion cannot be watched and stills can.
///
/// House recipe, settled through the audit loop: two-pass palette,
/// `palettegen stats_mode=full` (dark plates have big static grounds —
/// diff would starve the palette), `paletteuse
/// dither=floyd_steinberg:diff_mode=rectangle` — error diffusion is the
/// anti-banding dither, and it measured *smaller* than ordered bayer
/// on 34 of the 36 plates — scaled to `width`, looping forever. (One
/// plate, aurora, intentionally carries grain over smooth ramps and
/// exceeds the GIF 256-colour floor at every dither/width tried; its
/// audit surface of record remains the full-res sheet.) Appends the
/// measured `gif=` line to metrics.txt so one receipt covers all
/// three artifacts — the byte count is read off the file, never
/// guessed. The mirror of this for already-rendered frame dirs is
/// `film_lab/tools/make_gifs.sh`.
pub fn anim_gif(out_dir: &Path, fps: u32, width: u32) -> Option<PathBuf> {
    anim_gif_strided(out_dir, fps, width, 1)
}

/// The strided GIF — the endurance plate's motion receipt: 256 rendered
/// frames become a 64-frame loop (stride 4), and the metrics say so, so
/// nobody mistakes the decimation for the render. Same house recipe
/// otherwise; the strided subset rides the same hardlink detour the
/// strided sheet does (the cgroup OOM, avoided rather than met).
pub fn anim_gif_strided(out_dir: &Path, fps: u32, width: u32, stride: usize) -> Option<PathBuf> {
    let gif = out_dir.join("anim.gif");
    let src_dir;
    let tmp;
    if stride <= 1 {
        src_dir = out_dir.to_path_buf();
        tmp = None;
    } else {
        let t = out_dir.join("_stride_tmp");
        let _ = std::fs::remove_dir_all(&t);
        std::fs::create_dir_all(&t).ok()?;
        hardlink_strided(out_dir, &t, stride)?;
        tmp = Some(t.clone());
        src_dir = t;
    }
    let frames = src_dir.join("frame_%03d.png");
    // TWO passes, deliberately: the one-pass `split` graph parks every
    // frame in a queue behind the palette, and on this 4 GB bench that
    // queue is what the OOM-killer found (measured: 64 frames died in
    // the split graph; both halves run clean as separate processes).
    // Palette first, then the frames through it, one at a time.
    let palette = out_dir.join("palette.png");
    let status = std::process::Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error"])
        .arg("-framerate").arg(fps.to_string())
        .arg("-i").arg(&frames)
        .arg("-vf").arg(format!(
            "scale={width}:-1:flags=lanczos,palettegen=stats_mode=full"))
        .arg(&palette)
        .status();
    if !matches!(status, Ok(s) if s.success()) {
        return None;
    }
    let status = std::process::Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error"])
        .arg("-framerate").arg(fps.to_string())
        .arg("-i").arg(&frames)
        .arg("-i").arg(&palette)
        .arg("-lavfi").arg(format!(
            "scale={width}:-1:flags=lanczos[x];[x][1:v]paletteuse=dither=floyd_steinberg:diff_mode=rectangle"))
        .arg("-loop").arg("0")
        .arg(&gif)
        .status();
    let _ = std::fs::remove_file(&palette);
    if let Some(t) = tmp {
        let _ = std::fs::remove_dir_all(&t);
    }
    if !matches!(status, Ok(s) if s.success()) {
        return None;
    }

    // The receipt line — every number measured, none typed (§4.5). The
    // stride rides its own line so the `gif=` format stays stable for the
    // tools that parse it.
    if let (Ok((fw, fh)), Ok(bytes)) = (
        image::image_dimensions(out_dir.join("frame_000.png")),
        std::fs::metadata(&gif).map(|m| m.len()),
    ) {
        let h = (width as f32 * fh as f32 / fw as f32).round() as u32;
        let line = format!("gif=anim.gif,{width}x{h},{fps}fps,{bytes}\n");
        let mut extra = String::new();
        if stride > 1 {
            extra.push_str(&format!("gif_stride={stride}\n"));
        }
        let metrics = out_dir.join("metrics.txt");
        if let Ok(body) = std::fs::read_to_string(&metrics) {
            let stripped: String = body
                .lines()
                .filter(|l| !l.starts_with("gif=") && !l.starts_with("gif_stride="))
                .map(|l| format!("{l}\n"))
                .collect();
            let _ = std::fs::write(&metrics, format!("{stripped}{line}{extra}"));
        }
    }
    Some(gif)
}

/// Where experiment output lives: /home/z/my-project/download/film_lab/<name>.
pub fn out_root() -> PathBuf {
    PathBuf::from("/home/z/my-project/download/film_lab")
}
