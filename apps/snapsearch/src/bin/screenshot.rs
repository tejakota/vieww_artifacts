//! Renders SnapSearch through `vieww`'s CPU backend — no display, no GPU,
//! no window — so the UI can be checked from a picture in an environment
//! with neither a screen nor a graphics adapter. Same pattern as the
//! framework's own `examples/screenshot`.
//!
//! ```console
//! cargo run --bin screenshot --features cpu            # writes ./shots/*.png
//! cargo run --bin screenshot --features cpu -- /tmp/out
//! ```

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use vieww::foundation::{Color, Size};
use vieww::paint::cpu::CpuRenderer;
use vieww::render::FrameDriver;

use snapsearch::state::SearchMode;

// An iPhone-14-class viewport. `docs/DESIGN.md`'s adaptive metrics respond to
// `TargetPlatform::current()` (the host OS this binary is built for), not to
// this size — the point here is just a representative phone canvas.
const W: f32 = 390.0;
const H: f32 = 844.0;

fn main() {
    let out: PathBuf = std::env::args().nth(1).unwrap_or_else(|| "shots".into()).into();
    std::fs::create_dir_all(&out).expect("creating the output directory");

    let mut driver = FrameDriver::new(Size::new(W, H));
    let mut renderer = CpuRenderer::new();

    // `draw_frame()` (no args) always ticks at `Duration::ZERO` — it's a
    // single-instant convenience for tests, not a running clock. Every
    // `Animated`/`AnimationController` in the tree is driven by real elapsed
    // time, so a headless run has to feed `draw_frame_at` its own advancing
    // clock the way `App::run`'s real event loop feeds it vsync timestamps.
    let mut elapsed = Duration::ZERO;
    let mut tick = |driver: &mut FrameDriver, step: Duration| {
        elapsed += step;
        driver.draw_frame_at(elapsed);
    };

    let slot = Rc::new(RefCell::new(None));
    snapsearch::app::build_root(&mut driver, &slot);
    let state = slot.borrow().clone().expect("build_root always fills the slot");

    // Real photos are read on worker threads, so the shot has to wait for
    // them the way the running app does — by pumping frames until the
    // loader goes idle.
    for _ in 0..200 {
        state.pump_loader();
        tick(&mut driver, Duration::from_millis(16));
        if state.loading.peek() == 0 && !state.photos.peek().is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    println!(
        "  (library: {} photos from {:?})",
        state.photos.peek().len(),
        state.origin.peek()
    );

    // Frame 1: the library, at rest.
    tick(&mut driver, Duration::ZERO);
    write(&mut renderer, &mut driver, &out, "01-library.png");

    // Frame 2: the FAB tapped — the search sheet sliding up over its 320ms
    // animation; 10 steps of 40ms covers it.
    state.open_sheet();
    for _ in 0..10 {
        tick(&mut driver, Duration::from_millis(40));
    }
    write(&mut renderer, &mut driver, &out, "02-search-sheet.png");

    // Frame 3: "Use a photo" mode, reference picker visible.
    state.set_mode(SearchMode::Photo);
    tick(&mut driver, Duration::from_millis(16));
    write(&mut renderer, &mut driver, &out, "03-photo-mode.png");

    // Frame 4: back to text search, mid-search — the progress dialog up.
    state.set_mode(SearchMode::Text);
    state.text_query.set("a sunset over water".to_string());
    state.run_search();
    // `advance_clock` is what moves the progress bar, and it reads a real
    // `Instant` — so this needs both a genuine sleep and the call itself.
    // Without the call the shot showed a bar frozen at 0%.
    for _ in 0..11 {
        std::thread::sleep(Duration::from_millis(50));
        state.advance_clock();
        tick(&mut driver, Duration::from_millis(50));
    }
    println!("  (progress at capture: {:.0}%)", state.progress.peek() * 100.0);
    write(&mut renderer, &mut driver, &out, "04-searching.png");

    // `advance_clock` reads real `Instant::now()` for the search-progress
    // bar (see `state.rs`) — that part needs genuine sleeps regardless of
    // the render driver's own virtual clock above.
    for _ in 0..30 {
        std::thread::sleep(Duration::from_millis(50));
        state.advance_clock();
        tick(&mut driver, Duration::from_millis(50));
    }
    write(&mut renderer, &mut driver, &out, "05-results.png");

    // Frame 6: the staggered entrance, caught part-way through. Replaying
    // the timeline and sampling a few frames in is the only way to see a
    // stagger in a still.
    if let Some(player) = state.reveal_player.borrow().as_ref() {
        player.play();
    }
    for _ in 0..3 {
        tick(&mut driver, Duration::from_millis(40));
    }
    let revealed: Vec<String> = state
        .reveal
        .iter()
        .take(6)
        .map(|s| format!("{:.2}", s.peek()))
        .collect();
    println!("  (reveal, first six: {})", revealed.join(" "));
    write(&mut renderer, &mut driver, &out, "06-stagger.png");

    // Frame 7: scrolled down, so the header has collapsed.
    // Back to the whole library first — four results are shorter than the
    // viewport, so there is nothing to scroll and the header cannot move.
    state.clear_search();
    // One frame first: the scrollable reports its extents from layout, and
    // dragging before it has done so scrolls against a zero maximum.
    tick(&mut driver, Duration::from_millis(16));
    state.scroll.drag(-260.0);
    state.sync_scroll();
    println!("  (header collapse: {:.2})", state.header.peek());
    tick(&mut driver, Duration::from_millis(16));
    write(&mut renderer, &mut driver, &out, "07-scrolled.png");

    // Frame 8: a photo open full screen.
    state.scroll.drag(400.0); // back to the top first
    state.sync_scroll();
    if let Some(first) = state.photos.peek().first().cloned() {
        state.open_detail(first);
        // Two separate waits, and conflating them was a bug: the decode
        // finishing is not the animation finishing. Breaking out of the
        // first loop the moment the pixels arrived caught the overlay at
        // about 5% opacity.
        for _ in 0..60 {
            state.pump_loader();
            tick(&mut driver, Duration::from_millis(16));
            if state.detail_full.peek().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        for _ in 0..20 {
            tick(&mut driver, Duration::from_millis(30));
        }
        println!(
            "  (detail: full-res decoded {})",
            state.detail_full.peek().is_some()
        );
        write(&mut renderer, &mut driver, &out, "08-detail.png");
    }

    println!("wrote {}", out.display());
}

fn write(renderer: &mut CpuRenderer, driver: &mut FrameDriver, out: &std::path::Path, name: &str) {
    let (png, report) = renderer
        .render_to_png(driver.scene(), W as u32, H as u32, Color::rgb(0x12, 0x10, 0x20))
        .expect("rasterising through the CPU backend");
    let path = out.join(name);
    std::fs::write(&path, png).expect("writing the PNG");
    println!(
        "  {name}: {} shapes, {} glyph runs ({} glyphs), {} clips, {} shadows, {} layers",
        report.shapes, report.glyph_runs, report.glyphs, report.clips, report.shadows, report.layers
    );
}
