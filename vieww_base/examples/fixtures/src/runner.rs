//! The thing that turns a fixture into a picture and a number.
//!
//! # Why a runner and not `main` doing it inline
//!
//! Every fixture in this gallery has to be measured the *same* way or the
//! numbers cannot be compared, and comparing them is the entire point: a
//! rendering cost is only ever pathological *relative* to a simpler scene
//! that does nearly the same work. The clip bug this gallery was built after
//! looked like "the studio is slow" for as long as nobody had a picture of
//! four hundred rectangles with and without a rounded corner around them,
//! rendered by the same code, timed on the same machine.
//!
//! # One renderer, reused
//!
//! Each fixture gets its own [`NativeRenderer`] but renders through it more
//! than once: the first frame is a warm-up, and the reported time is a later
//! one. That is deliberate and it is the honest number for a *window*, which
//! is what this is a model of — a window keeps its renderer, and therefore
//! its glyph outlines and its clip masks, between frames. Timing the very
//! first frame would report font parsing as if it happened every frame.

use std::path::Path;
use std::time::{Duration, Instant};

use vieww_foundation::{Color, Size};
use vieww_paint::native::{NativeRenderer, SceneReport};
use vieww_render::FrameDriver;
use vieww_render_planner::Executor;
use vieww_widget::WidgetNode;

use crate::receipt::{self, Json};
use crate::shadow::ShadowLog;

/// How many times a still fixture is rendered before the timed runs. The
/// number of timed runs is `--runs` (default `cli::DEFAULT_RUNS`): small,
/// because this is a gallery and the costs it exists to find differ by orders
/// of magnitude rather than by percent — but a receipt meant for comparison
/// should ask for more.
pub(crate) const WARMUP: usize = 1;

/// One entry in the gallery.
pub(crate) struct Fixture {
    /// `tier/name`, which is also the PNG's filename.
    pub name: &'static str,
    /// What this fixture is for — printed beside its numbers so a row in the
    /// report explains itself.
    pub about: &'static str,
    pub size: Size,
    pub background: Color,
    pub build: fn() -> WidgetNode,
}

/// What one fixture cost, once rendered.
pub(crate) struct Measured {
    pub name: &'static str,
    pub about: &'static str,
    pub size: Size,
    /// Best of the timed runs — the floor, which is the number a regression
    /// moves.
    pub best: Duration,
    /// Worst of the timed runs, to show when a fixture is noisy rather than
    /// slow.
    pub worst: Duration,
    /// Every timed run, in the order taken. A receipt carries all of them so a
    /// reader can compute a statistic this table does not print.
    pub runs: Vec<Duration>,
    pub report: SceneReport,
    /// Commands the widget tree produced. Cost per command is the ratio worth
    /// looking at; a scene that is slow because it is *large* is a different
    /// problem from one that is slow per command.
    pub commands: usize,
    /// Layout overflows this fixture's own tree reported while building.
    ///
    /// `vieww_render::overflow::reported` has existed the whole time, with a
    /// doc comment saying a number "is what turns that from something a
    /// reviewer might notice into something a script can refuse" — and nothing
    /// in this gallery read it. So the gallery printed overflow warnings on
    /// stderr, between the pictures, on every run, and they scrolled past
    /// looking exactly like the progress output the same doc warns about.
    ///
    /// Counted per fixture rather than for the whole run because "something
    /// overflowed" is not actionable and "31-stagger overflowed" is.
    pub overflows: usize,
}

impl Measured {
    /// Microseconds of render time per recorded command.
    #[must_use]
    pub(crate) fn per_command_us(&self) -> f64 {
        self.best.as_secs_f64() * 1e6 / self.commands.max(1) as f64
    }

    /// How many of these frames fit in 16.67 ms. Below 1.0 the fixture cannot
    /// hold 60 Hz on this machine by itself, let alone with a real
    /// application's other work beside it.
    #[must_use]
    pub(crate) fn frames_in_budget(&self) -> f64 {
        0.016_667 / self.best.as_secs_f64().max(1e-9)
    }
}

/// Build, render, time and write one fixture — and, with a shadow log, plan
/// it too (see `shadow.rs`). Planning happens after the timed runs, so it can
/// never be inside a timing.
pub(crate) fn run(
    fixture: &Fixture,
    out: &Path,
    runs: usize,
    shadow: Option<&mut ShadowLog>,
) -> Measured {
    // Attributed to this fixture: cleared before its tree is built, read after.
    vieww_render::overflow::forget_reported();

    let mut driver = FrameDriver::new(fixture.size);
    driver.elements().set_root((fixture.build)());
    driver.draw_frame();
    let overflows = vieww_render::overflow::reported();

    let (width, height) = (fixture.size.width as u32, fixture.size.height as u32);
    let mut renderer = NativeRenderer::new();

    for _ in 0..WARMUP {
        renderer
            .render_to_pixels(driver.scene(), width, height, fixture.background)
            .expect("warm-up render");
    }

    let mut taken = Vec::with_capacity(runs);
    let mut report = SceneReport::default();
    for _ in 0..runs.max(1) {
        let start = Instant::now();
        let (_pixels, this) = renderer
            .render_to_pixels(driver.scene(), width, height, fixture.background)
            .expect("timed render");
        taken.push(start.elapsed());
        report = this;
    }
    let best = taken.iter().copied().min().unwrap_or_default();
    let worst = taken.iter().copied().max().unwrap_or_default();

    if let Some(shadow) = shadow {
        // The median, not the floor: the planner predicts a typical frame.
        let typical = receipt::percentile(&taken, 50.0).unwrap_or(best);
        shadow.record(
            fixture.name,
            driver.scene(),
            (width, height),
            Executor::Cpu,
            typical.as_secs_f64() * 1000.0,
            Some("gallery: CPU rasterizer only".to_owned()),
        );
    }

    let (png, _) = renderer
        .render_to_png(driver.scene(), width, height, fixture.background)
        .expect("encoding the fixture");
    let path = out.join(format!("{}.png", fixture.name.replace('/', "-")));
    std::fs::write(&path, png).expect("writing the fixture PNG");

    Measured {
        name: fixture.name,
        about: fixture.about,
        size: fixture.size,
        best,
        worst,
        runs: taken,
        report,
        commands: driver.scene().len(),
        overflows,
    }
}

/// The report table — one line per fixture, widest cost last so the eye lands
/// on the outlier.
pub(crate) fn print_table(measured: &[Measured]) {
    // **`worst` is printed, and it is not decoration.**
    //
    // `best` is the floor and it is the right number to compare across a
    // change. But a floor with a ceiling three times higher above it is not a
    // measurement of this code, it is a measurement of what else the machine
    // was doing — and a table that shows only the floor gives no way to tell a
    // real regression from a noisy afternoon. Both fields were already being
    // computed and neither was being shown; `worst` was carrying a doc comment
    // saying it existed "to show when a fixture is noisy rather than slow"
    // while being dead code.
    println!(
        "\n{:<34} {:>9} {:>7} {:>9} {:>9} {:>9} {:>8}  what it covers",
        "fixture", "size", "cmds", "best", "worst", "us/cmd", "fps"
    );
    println!("{}", "-".repeat(136));
    for m in measured {
        let flag = if m.frames_in_budget() < 1.0 {
            "  <-- OVER 16.7ms"
        } else if m.worst.as_secs_f64() > m.best.as_secs_f64() * 2.0 {
            "  (noisy: worst is over twice best)"
        } else {
            ""
        };
        println!(
            "{:<34} {:>4}x{:<4} {:>7} {:>8.2}ms {:>8.2}ms {:>9.1} {:>8.0}  {}{}",
            m.name,
            m.size.width as u32,
            m.size.height as u32,
            m.commands,
            m.best.as_secs_f64() * 1000.0,
            m.worst.as_secs_f64() * 1000.0,
            m.per_command_us(),
            m.frames_in_budget(),
            m.about,
            flag
        );
    }

    let total: f64 = measured.iter().map(|m| m.best.as_secs_f64()).sum();
    let over: Vec<&str> = measured
        .iter()
        .filter(|m| m.frames_in_budget() < 1.0)
        .map(|m| m.name)
        .collect();
    println!("{}", "-".repeat(136));
    println!(
        "{} fixtures, {:.1} ms of rendering in total",
        measured.len(),
        total * 1000.0
    );
    // What the gallery actually asked the renderer to draw, summed. The other
    // half of `Measured` that was being computed and thrown away — and the
    // number that says whether a change to the *fixtures* moved a timing,
    // rather than a change to the renderer.
    let shapes: usize = measured.iter().map(|m| m.report.shapes).sum();
    let glyphs: usize = measured.iter().map(|m| m.report.glyphs).sum();
    let layers: usize = measured.iter().map(|m| m.report.layers).sum();
    let shadows: usize = measured.iter().map(|m| m.report.shadows).sum();
    let skipped: usize = measured.iter().map(|m| m.report.skipped_commands).sum();
    println!(
        "drawn across the gallery: {shapes} shapes, {glyphs} glyphs, \
         {layers} layers, {shadows} shadows ({skipped} commands culled)"
    );
    if over.is_empty() {
        println!("every fixture renders inside a 60 Hz frame budget");
    } else {
        println!("over budget: {}", over.join(", "));
    }

    let overflowing: Vec<String> = measured
        .iter()
        .filter(|m| m.overflows > 0)
        .map(|m| format!("{} ({})", m.name, m.overflows))
        .collect();
    if overflowing.is_empty() {
        println!("no fixture's layout overflows");
    } else {
        println!(
            "LAYOUT OVERFLOW in {}: a fixture whose own layout does not fit is \
             not a picture of what this framework does, it is a picture of a \
             bug — fix the fixture",
            overflowing.join(", ")
        );
    }
}

/// How many fixtures reported a layout overflow.
///
/// Returned so `main` can exit non-zero: see [`Measured::overflows`] for why a
/// printed warning was not enough.
#[must_use]
pub(crate) fn total_overflows(measured: &[Measured]) -> usize {
    measured.iter().map(|m| m.overflows).sum()
}

/// The receipt's `fixtures` and `outcome` for a gallery run.
pub(crate) fn receipt_fields(measured: &[Measured]) -> Vec<(String, Json)> {
    let rows = measured
        .iter()
        .map(|m| {
            Json::obj([
                ("name", Json::str(m.name)),
                ("width", Json::Num(f64::from(m.size.width))),
                ("height", Json::Num(f64::from(m.size.height))),
                ("commands", Json::count(m.commands)),
                ("cpu_runs_ms", Json::ms_list(&m.runs)),
                ("cpu_best_ms", Json::ms(m.best)),
                (
                    "cpu_p50_ms",
                    receipt::percentile(&m.runs, 50.0).map_or(Json::Null, Json::ms),
                ),
                ("cpu_worst_ms", Json::ms(m.worst)),
                ("over_16_7ms", Json::Bool(m.frames_in_budget() < 1.0)),
                ("shapes", Json::count(m.report.shapes)),
                ("glyphs", Json::count(m.report.glyphs)),
                ("layers", Json::count(m.report.layers)),
                ("shadows", Json::count(m.report.shadows)),
                ("culled_commands", Json::count(m.report.skipped_commands)),
                ("layout_overflows", Json::count(m.overflows)),
            ])
        })
        .collect();
    let over: Vec<Json> = measured
        .iter()
        .filter(|m| m.frames_in_budget() < 1.0)
        .map(|m| Json::str(m.name))
        .collect();
    let total: Duration = measured.iter().map(|m| m.best).sum();
    vec![
        ("fixtures".to_owned(), Json::Arr(rows)),
        (
            "outcome".to_owned(),
            Json::obj([
                ("fixtures", Json::count(measured.len())),
                ("total_best_ms", Json::ms(total)),
                ("over_budget", Json::Arr(over)),
                ("layout_overflows", Json::count(total_overflows(measured))),
            ]),
        ),
    ]
}
