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

mod exp_aurora;
mod exp_circuit;
mod exp_globe;
mod exp_kinetic;
mod exp_light;
mod exp_mesh;
mod exp_ocean;
mod exp_receipts;
mod film_lib;
mod three_d;

use film_lib::{contact_sheet, out_root, render, Experiment};

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
        match render(experiment, &dir) {
            Ok(receipt) => {
                receipt.print(experiment.name);
                match contact_sheet(&dir, "4x4") {
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
