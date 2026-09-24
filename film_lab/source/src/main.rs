//! film_lab — the launch film's berserk laboratory.
//!
//! ```console
//! cargo run --release -p film_lab                # all experiments
//! cargo run --release -p film_lab -- light       # one experiment
//! ```
//!
//! Each experiment renders a deterministic frame sequence through vieww's
//! own rasterizer (no display, no GPU), writes PNGs + a metrics receipt
//! (shapes counted by the renderer — no number typed by a human), and
//! assembles a 4x4 @ 10fps contact sheet with ffmpeg for visual audit.
//!
//! The experiments:
//! - `light`   — the calibration: "Light." recreated from the author's sheet
//! - `mesh`    — a single line becomes a shaded, rotating 3D world
//! - `ocean`   — a drop falls, strikes, and becomes an entire ocean
//! - `kinetic` — the text-kinetics family: type-on, hour-counter, odometer,
//!   stat count-ups (E-01/E-02, the ladder, S11)
//! - `circuit` — the session line E-17: self-drawing path, node blooms,
//!   fork ×3 (E-22), the axis (E-20 F2)
//! - `globe`   — the reveal spine E-20 F1–F4: the 7, the ghost part, the
//!   pull-back, three planes fanning with tree glyphs
//! - `receipts` — S11: the benchmark card assembles in 3D around real
//!   widget props (Chip/LinearProgress/LineChart/Badge) + CI timeline E-19
//! - `aurora`  — light as material: sweep-cone, Screen-blended ribbons,
//!   E-21 crafted degradation (FilterChain ramp + grain + dust)
//! - `spring`  — E-03/E-04: the wordmark drops on an underdamped spring,
//!   the underline overshoots on a stiffer one — the springs drawn as
//!   their own receipt
//! - `scrub`   — E-08/E-09: ten writes, one rebuild — scheduler coalescing
//!   made visible; build_count() badge, writes-vs-rebuilds ratio live
//! - `damage`  — E-10: one region lights — a mock studio surface, four
//!   edits, the PerformanceOverlay strip with damage area measured
//! - `rackfocus` — E-16: the blur ramp between buffer and preview — the
//!   focus pull as cinematography, focal bar printed live
//! - `morph`   — E-06: say → rust, state held — the language switch with
//!   the session clock never rebuilding through the morph
//! - `endcard` — E-15: the end card + the sting — wordmark, install line,
//!   manifest line, one accent firing, the hold
//! - `wordmark` — X-03: the mark as real glyph outlines (U-03 door) —
//!   stroke-on scan reveal, phase-advancing chrome (U-08 re-sort), a
//!   Plus-blended glint (U-01), a mirrored reflection; U-10 dial in the
//!   receipt
//! - `shatter` — X-08: the card breaks — 64 Voronoi shards, each a window
//!   onto the artwork (U-11 transform-outside/clip-inside), the fastest
//!   decile through `Filtered::with_blur_angle` (U-14), Plus dust
//! - `beams` — X-05: eleven volumetric shafts and 6,000 dust motes through
//!   one Plus-blended blurred group (U-01 economy, U-06 pricing)
//! - `liquid` — U-13's marching squares: six metaballs and a falling drop
//!   contoured, chained into closed loops (U-20), glass-filled
//! - `unfold` — P-01: three planes fan out of a spine through
//!   `Transform3::project_rect` (U-04) — the E-20 F4 beat, tree glyphs
//!   projected pointwise, behind-camera drops counted
//! - `hero` — the worst frame at 1920×1080: grid, shafts, dust, a 3,200-
//!   quad knot, the liquid mass, the outline wordmark, a backdrop-blurred
//!   caption — the budget receipt at master resolution

mod exp_aurora;
mod exp_beams;
mod exp_circuit;
mod exp_damage;
mod exp_endcard;
mod exp_globe;
mod exp_hero;
mod exp_kinetic;
mod exp_liquid;
mod exp_light;
mod exp_mesh;
mod exp_morph;
mod exp_ocean;
mod exp_rackfocus;
mod exp_unfold;
mod exp_receipts;
mod exp_scrub;
mod exp_shatter;
mod exp_spring;
mod exp_wordmark;
mod film_lib;
mod three_d;

use film_lib::{contact_sheet, out_root, render_with, Experiment};

fn registry() -> Vec<Experiment> {
    vec![
        Experiment {
            name: "light",
            seconds: exp_light::SECONDS,
            frames: 16,
            build: exp_light::frame,
        },
        Experiment {
            name: "mesh",
            seconds: exp_mesh::SECONDS,
            frames: 16,
            build: exp_mesh::frame,
        },
        Experiment {
            name: "ocean",
            seconds: exp_ocean::SECONDS,
            frames: 16,
            build: exp_ocean::frame,
        },
        Experiment {
            name: "kinetic",
            seconds: exp_kinetic::SECONDS,
            frames: 16,
            build: exp_kinetic::frame,
        },
        Experiment {
            name: "circuit",
            seconds: exp_circuit::SECONDS,
            frames: 16,
            build: exp_circuit::frame,
        },
        Experiment {
            name: "globe",
            seconds: exp_globe::SECONDS,
            frames: 16,
            build: exp_globe::frame,
        },
        Experiment {
            name: "receipts",
            seconds: exp_receipts::SECONDS,
            frames: 16,
            build: exp_receipts::frame,
        },
        Experiment {
            name: "aurora",
            seconds: exp_aurora::SECONDS,
            frames: 16,
            build: exp_aurora::frame,
        },
        Experiment {
            name: "spring",
            seconds: exp_spring::SECONDS,
            frames: 16,
            build: exp_spring::frame,
        },
        Experiment {
            name: "scrub",
            seconds: exp_scrub::SECONDS,
            frames: 16,
            build: exp_scrub::frame,
        },
        Experiment {
            name: "damage",
            seconds: exp_damage::SECONDS,
            frames: 16,
            build: exp_damage::frame,
        },
        Experiment {
            name: "rackfocus",
            seconds: exp_rackfocus::SECONDS,
            frames: 16,
            build: exp_rackfocus::frame,
        },
        Experiment {
            name: "morph",
            seconds: exp_morph::SECONDS,
            frames: 16,
            build: exp_morph::frame,
        },
        Experiment {
            name: "endcard",
            seconds: exp_endcard::SECONDS,
            frames: 16,
            build: exp_endcard::frame,
        },
        Experiment {
            name: "beams",
            seconds: exp_beams::SECONDS,
            frames: 16,
            build: exp_beams::frame,
        },
        Experiment {
            name: "liquid",
            seconds: exp_liquid::SECONDS,
            frames: 16,
            build: exp_liquid::frame,
        },
        Experiment {
            name: "unfold",
            seconds: exp_unfold::SECONDS,
            frames: 16,
            build: exp_unfold::frame,
        },
        Experiment {
            name: "shatter",
            seconds: exp_shatter::SECONDS,
            frames: 16,
            build: exp_shatter::frame,
        },
        Experiment {
            name: "wordmark",
            seconds: exp_wordmark::SECONDS,
            frames: 16,
            build: exp_wordmark::frame,
        },
        Experiment {
            name: "hero",
            seconds: exp_hero::SECONDS,
            frames: 8,
            build: exp_hero::frame,
        },
    ]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filter = std::env::args().nth(1);
    let experiments: Vec<Experiment> = registry()
        .into_iter()
        .filter(|e| filter.as_deref().is_none_or(|f| e.name.contains(f)))
        .collect();

    if experiments.is_empty() {
        eprintln!("no experiment matches {filter:?}");
        return Ok(());
    }

    let root = out_root();
    println!("film_lab — {} experiment(s) → {}", experiments.len(), root.display());

    let mut failures = 0;
    for experiment in &experiments {
        let dir = root.join(experiment.name);
        println!("\n▶ {}", experiment.name);
        // The hero frame renders at true master resolution — the budget
        // receipt is the point; everything else stays at lab standard.
        let canvas = if experiment.name == "hero" {
            vieww_foundation::Size::new(1920.0, 1080.0)
        } else {
            film_lib::CANVAS
        };
        match render_with(experiment, &dir, canvas) {
            Ok(receipt) => {
                receipt.print(experiment.name);
                let tile = if experiment.name == "hero" { "4x2" } else { "4x4" };
                match contact_sheet(&dir, tile) {
                    Some(sheet) => println!("    sheet: {}", sheet.display()),
                    None => println!("    sheet: FAILED (ffmpeg)"),
                }
            }
            Err(e) => {
                failures += 1;
                eprintln!("    FAILED: {e}");
            }
        }
    }

    if failures > 0 {
        Err(format!("{failures} experiment(s) failed").into())
    } else {
        Ok(())
    }
}
