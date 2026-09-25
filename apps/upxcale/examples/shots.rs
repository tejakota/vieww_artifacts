//! Render every screen state to a PNG, with no window and no GPU.
//!
//! ```console
//! cargo run --example shots --features cpu --release -- shots/
//! ```
//!
//! This is the smoke test. If all five images come out, then the theme
//! resolved, the assets decoded, the masonry packed, the routes pushed, the
//! upscaler ran, and the whole tree laid out and rasterised — which is most of
//! the app minus the finger.
//!
//! It is also how the app is checked on a machine with no display. vieww's
//! CPU backend rasterises the same `Scene` the GPU one does, so a layout bug
//! shows up here exactly as it would on a phone; what it cannot tell you is
//! whether the frame budget was met, which is a question for
//! `ci/device-suite.sh` on real hardware.
//!
//! The spawner is [`Inline`], so the upscale runs on this thread and has
//! finished by the time `start_render` returns. That is the same code path the
//! app takes with [`Threads`](vieww_foundation::task::Threads) — the seam is
//! the point.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use upxcale::photos::{self, Library};
use upxcale::screens::landing::{
    compare_route, landing_route, picker_route, progress_route, Landing,
};
use upxcale::state::AppState;
use upxcale::SURFACE;

use vieww_asset::{AssetBundle, DirectoryBundle};
use vieww_element::NavigatorController;
use vieww_foundation::task::{FrameWaker, Inline, NoWaker, Spawn};
use vieww_paint::cpu::CpuRenderer;
use vieww_render::FrameDriver;
use vieww_widget::prelude::*;

fn main() {
    let out: PathBuf = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "shots".into())
        .into();
    std::fs::create_dir_all(&out).expect("creating the output directory");

    let mut driver = FrameDriver::new(SURFACE);
    let mut renderer = CpuRenderer::new();
    let runtime = driver.elements().runtime().clone();

    let bundle: Rc<dyn AssetBundle> = Rc::new(DirectoryBundle::at(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets"
    )));
    let library = Rc::new(Library::new(bundle));

    let nav = NavigatorController::new(
        &runtime,
        Route::new("bootstrap", Rc::new(|_| SizedBox::shrink().into())),
    );

    let spawner: Rc<dyn Spawn> = Rc::new(Inline);
    let waker: Arc<dyn FrameWaker> = Arc::new(NoWaker);
    let state = AppState::new(runtime, library, nav.clone(), spawner, waker);

    let landing = Landing {
        state: state.clone(),
    };
    nav.replace(landing_route(landing.clone()));
    nav.attach(driver.tickers());
    state.grid_scroll.attach(driver.tickers());
    state.picker_scroll.attach(driver.tickers());
    driver.set_root(landing);

    // ── 1. the landing grid, as the app opens ───────────────────────────────
    shoot(&mut driver, &mut renderer, &out, "1-landing");

    // ── 2. the picker, with three photographs ticked ────────────────────────
    state.clear_picks();
    for id in ["a1", "a5", "a10"] {
        state.toggle_pick(id);
    }
    nav.push(picker_route(state.clone()));
    shoot(&mut driver, &mut renderer, &out, "2-picker");

    // ── 3. the progress overlay, mid-batch ──────────────────────────────────
    // Pushed *before* the render starts so the card is caught with the bar part
    // way along rather than already complete.
    nav.replace(progress_route(state.clone()));
    state.progress.set(upxcale::state::RenderState {
        stages_done: 3,
        stages_total: 6,
        photos_done: 1,
        photos_total: 3,
        status: "Photo 2 of 3".to_string(),
    });
    shoot(&mut driver, &mut renderer, &out, "3-progress");

    // ── 4. the grid after the render ────────────────────────────────────────
    // `Inline` means the whole batch has run by the time this returns.
    assert!(state.start_render(), "the render should have started");
    let finished = state.poll_render();
    assert!(finished, "an inline render should complete within one poll");
    shoot(&mut driver, &mut renderer, &out, "4-rendered");

    // ── 5. before / after ───────────────────────────────────────────────────
    let photo = photos::by_id("a1").expect("a1 is in the catalogue");
    state.open_compare(photo.id);
    state.set_divider(0.45);
    nav.push(compare_route(&state));
    shoot(&mut driver, &mut renderer, &out, "5-compare");

    // ── and the proof the upscaler did something ────────────────────────────
    let source = state.library.source(photo).expect("decoding the source");
    let result = state.library.result(photo).expect("the upscaled result");

    // Compare like with like. Acutance is a *per-pixel* measure, so a 4x
    // enlargement always scores lower than its source — the same edge is spread
    // over four times as many pixels. The meaningful comparison is against a
    // plain enlargement of identical size: that isolates what the sharpening
    // pass recovered from what the resampling cost.
    let plain = upxcale::upscale::resample(&source, result.width(), result.height());
    println!(
        "{}: {}x{} -> {}x{}   acutance at 4x: plain {:.3}, sharpened {:.3} (+{:.0}%)",
        photo.slug,
        source.width(),
        source.height(),
        result.width(),
        result.height(),
        upxcale::upscale::acutance(&plain),
        upxcale::upscale::acutance(&result),
        (upxcale::upscale::acutance(&result) / upxcale::upscale::acutance(&plain) - 1.0) * 100.0,
    );
    let path = out.join("upscale-detail.png");
    upxcale::export::write_png_to(&result, &path).expect("writing the upscaled photograph");
    println!("wrote {}", path.display());
}

/// Draw one frame and rasterise it.
fn shoot(driver: &mut FrameDriver, renderer: &mut CpuRenderer, out: &std::path::Path, name: &str) {
    // Twice: the first frame mounts the tree and the second paints it with
    // every route transition settled. A single frame catches routes mid-fade,
    // which makes a screenshot a poor record of what the screen looks like.
    driver.draw_frame();
    driver.draw_frame();

    let (png, report) = renderer
        .render_to_png(
            driver.scene(),
            SURFACE.width as u32,
            SURFACE.height as u32,
            upxcale::theme::scheme().surface,
        )
        .expect("rasterising");

    let path = out.join(format!("{name}.png"));
    std::fs::write(&path, png).expect("writing the PNG");
    println!(
        "wrote {} — {} shapes, {} glyph runs, {} clips, {} layers",
        path.display(),
        report.shapes,
        report.glyph_runs,
        report.clips,
        report.layers
    );
}
