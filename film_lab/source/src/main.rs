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
//! assembles the full receipt set with ffmpeg: a 4x4 @ 10fps contact sheet
//! for visual audit plus a palette-optimised `anim.gif` loop — the motion
//! receipt (GIF over mp4: loops inline in browsers/GitHub with no codec,
//! and measurably the smaller carrier at the lab's flat-colour content).
//! Every render therefore ships anim.gif + sheet.png + metrics.txt.
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
//!
//! Round 6 — the berserk spectrum, one plate per tolerance axis:
//! - `avatar` — a single line becomes a 3D bust (lathe + orbiting light)
//! - `fadeaway` — the text release: chroma split → blur → shard dust
//! - `sea` — a drop becomes an entire ocean (one surface, scale reveal)
//! - `tesseract` — a hypercube rotating through the 4th direction
//! - `blackhole` — Doppler-boosted disk, lensed stars, photon ring
//! - `galaxy` — 60,000 stars: the shape-count record (60,527/frame)
//! - `forest` — the recursion: seed → canopy, apical + phototropic
//! - `city` — ten thousand hash-addressed windows, lit fraction measured
//! - `typo` — the sentence becomes weather; condenses into one drop
//! - `mandel` — the count ceiling as art: 57,602 rects per frame
//! - `hero4k` — the hero tree rasterised at 3840×2160
//!
//! Round 7 — the deep axes, the dimensions rounds 3–6 never measured:
//! - `han` — the script axis: CJK through the shaper (一画开天)
//! - `megapath` — the single-path axis: one ~35,000-segment line
//! - `longplay` — the endurance axis: 256 frames, RSS time series
//! - `swarm` — the simulation axis: 3,000 boids, sim-vs-raster split
//! - `blendmatrix` — the blend-mode axis: all 15 cinematic modes
//! - `filterstack` — the compositor-depth axis: nested filtered groups
//! - `shadowplay` — the blur economy at the U-06 guard (30 of 32)
//! - `prism` — the gradient axis: 256-stop animated re-sorted ramps
//!
//! Round 8 — the wonder engines, the machines the lab imagined:
//! - `eclipse` — the narrative axis: totality, as a documented light sequence
//! - `cymatics` — the frequency axis: 7,000 grains descending to Chladni nodes
//! - `harmony` — the phase axis: 32 pendulums, integer cycle ladder
//! - `fourier` — the synthesis axis: the avatar line as a choir of circles
//! - `startrail` — the exposure axis: time folded into arcs, Plus-accumulated
//! - `bubble` — the optics axis: thin-film interference colours from physics
//! - `orrery` — the mechanism axis: gear ratios that ARE the astronomy
//! - `storm` — the weather axis: supercell + recursive lightning trees
//! - `kaleido` — the symmetry axis: D12, the mirror measured from the raster
//! - `ink` — the diffusion axis: one drop becomes a nebula, then dilutes

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
mod exp_ghosts;
mod exp_settle;
mod exp_dolly;
mod exp_currents;
mod exp_probe;
mod exp_avatar;
mod exp_fadeaway;
mod exp_sea;
mod exp_tesseract;
mod exp_blackhole;
mod exp_galaxy;
mod exp_forest;
mod exp_city;
mod exp_typo;
mod exp_mandel;
mod exp_hero4k;
mod exp_han;
mod exp_megapath;
mod exp_longplay;
mod exp_swarm;
mod exp_blendmatrix;
mod exp_filterstack;
mod exp_shadowplay;
mod exp_prism;
mod exp_eclipse;
mod exp_cymatics;
mod exp_harmony;
mod exp_fourier;
mod exp_startrail;
mod exp_bubble;
mod exp_orrery;
mod exp_storm;
mod exp_kaleido;
mod exp_ink;
mod film_lib;
mod three_d;

use film_lib::{anim_gif_strided, contact_sheet_strided, out_root, render_with, Experiment};

fn registry() -> Vec<Experiment> {
    vec![
        Experiment::plain("light", exp_light::SECONDS, 16, exp_light::frame),
        Experiment::plain("mesh", exp_mesh::SECONDS, 16, exp_mesh::frame),
        Experiment::plain("ocean", exp_ocean::SECONDS, 16, exp_ocean::frame),
        Experiment::plain("kinetic", exp_kinetic::SECONDS, 16, exp_kinetic::frame),
        Experiment::plain("circuit", exp_circuit::SECONDS, 16, exp_circuit::frame),
        Experiment::plain("globe", exp_globe::SECONDS, 16, exp_globe::frame),
        Experiment::plain("receipts", exp_receipts::SECONDS, 16, exp_receipts::frame),
        Experiment::plain("aurora", exp_aurora::SECONDS, 16, exp_aurora::frame),
        Experiment::plain("spring", exp_spring::SECONDS, 16, exp_spring::frame),
        Experiment::plain("scrub", exp_scrub::SECONDS, 16, exp_scrub::frame),
        Experiment::plain("damage", exp_damage::SECONDS, 16, exp_damage::frame),
        Experiment::plain("rackfocus", exp_rackfocus::SECONDS, 16, exp_rackfocus::frame),
        Experiment::plain("morph", exp_morph::SECONDS, 16, exp_morph::frame),
        Experiment::plain("endcard", exp_endcard::SECONDS, 16, exp_endcard::frame),
        Experiment::plain("beams", exp_beams::SECONDS, 16, exp_beams::frame),
        Experiment::plain("liquid", exp_liquid::SECONDS, 16, exp_liquid::frame),
        Experiment::plain("unfold", exp_unfold::SECONDS, 16, exp_unfold::frame),
        Experiment::plain("shatter", exp_shatter::SECONDS, 16, exp_shatter::frame),
        Experiment::plain("wordmark", exp_wordmark::SECONDS, 16, exp_wordmark::frame),
        Experiment::plain("ghosts", exp_ghosts::SECONDS, 16, exp_ghosts::frame),
        Experiment::plain("settle", exp_settle::SECONDS, 16, exp_settle::frame),
        Experiment::plain("dolly", exp_dolly::SECONDS, 16, exp_dolly::frame),
        Experiment::plain("currents", exp_currents::SECONDS, 16, exp_currents::frame),
        Experiment {
            name: "probe",
            seconds: exp_probe::SECONDS,
            frames: 16,
            build: exp_probe::frame,
            probe: Some(exp_probe::probe),
            frame_hook: None,
        },
        Experiment::plain("hero", exp_hero::SECONDS, 8, exp_hero::frame),
        // ── Round 6: the berserk spectrum — ten new plates + the 4K hero ──
        Experiment::plain("avatar", exp_avatar::SECONDS, 16, exp_avatar::frame),
        Experiment::plain("fadeaway", exp_fadeaway::SECONDS, 32, exp_fadeaway::frame),
        Experiment::plain("sea", exp_sea::SECONDS, 16, exp_sea::frame),
        Experiment::plain("tesseract", exp_tesseract::SECONDS, 16, exp_tesseract::frame),
        Experiment::plain("blackhole", exp_blackhole::SECONDS, 16, exp_blackhole::frame),
        Experiment::plain("galaxy", exp_galaxy::SECONDS, 16, exp_galaxy::frame),
        Experiment::plain("forest", exp_forest::SECONDS, 16, exp_forest::frame),
        Experiment::plain("city", exp_city::SECONDS, 16, exp_city::frame),
        Experiment::plain("typo", exp_typo::SECONDS, 24, exp_typo::frame),
        Experiment::plain("mandel", exp_mandel::SECONDS, 16, exp_mandel::frame),
        Experiment::plain("hero4k", exp_hero4k::SECONDS, 2, exp_hero4k::frame),
        // ── Round 7: the deep axes — eight new plates, one per dimension ──
        Experiment {
            name: "han",
            seconds: exp_han::SECONDS,
            frames: 24,
            build: exp_han::frame,
            probe: Some(exp_han::probe),
            frame_hook: None,
        },
        Experiment::plain("megapath", exp_megapath::SECONDS, 16, exp_megapath::frame),
        Experiment {
            name: "longplay",
            seconds: exp_longplay::SECONDS,
            frames: 256,
            build: exp_longplay::frame,
            probe: Some(exp_longplay::probe),
            frame_hook: Some(exp_longplay::frame_hook),
        },
        Experiment::plain("swarm", exp_swarm::SECONDS, 16, exp_swarm::frame),
        Experiment::plain("blendmatrix", exp_blendmatrix::SECONDS, 16, exp_blendmatrix::frame),
        Experiment::plain("filterstack", exp_filterstack::SECONDS, 16, exp_filterstack::frame),
        Experiment::plain("shadowplay", exp_shadowplay::SECONDS, 16, exp_shadowplay::frame),
        Experiment::plain("prism", exp_prism::SECONDS, 16, exp_prism::frame),
        // ── Round 8: the wonder engines — ten new plates, one per axis ──
        Experiment::plain("eclipse", exp_eclipse::SECONDS, 24, exp_eclipse::frame),
        Experiment::plain("cymatics", exp_cymatics::SECONDS, 24, exp_cymatics::frame),
        Experiment::plain("harmony", exp_harmony::SECONDS, 32, exp_harmony::frame),
        Experiment::plain("fourier", exp_fourier::SECONDS, 32, exp_fourier::frame),
        Experiment {
            name: "startrail",
            seconds: exp_startrail::SECONDS,
            frames: 16,
            build: exp_startrail::frame,
            probe: Some(exp_startrail::probe),
            frame_hook: None,
        },
        Experiment::plain("bubble", exp_bubble::SECONDS, 16, exp_bubble::frame),
        Experiment::plain("orrery", exp_orrery::SECONDS, 16, exp_orrery::frame),
        Experiment::plain("storm", exp_storm::SECONDS, 16, exp_storm::frame),
        Experiment {
            name: "kaleido",
            seconds: exp_kaleido::SECONDS,
            frames: 16,
            build: exp_kaleido::frame,
            probe: Some(exp_kaleido::probe),
            frame_hook: None,
        },
        Experiment::plain("ink", exp_ink::SECONDS, 24, exp_ink::frame),
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
        } else if experiment.name == "hero4k" {
            vieww_foundation::Size::new(3840.0, 2160.0)
        } else {
            film_lib::CANVAS
        };
        match render_with(experiment, &dir, canvas) {
            Ok(receipt) => {
                receipt.print(experiment.name);
                let tile = match experiment.name.as_ref() {
                    "hero" | "hero4k" => "4x2",
                    "fadeaway" => "4x8",
                    "typo" | "han" => "4x6",
                    _ => "4x4",
                };
                // The endurance plate samples its sheet (256 frames,
                // every 16th — the sheet stays the 16-cell audit surface;
                // the GIF below decimates less, stride 4).
                let sheet_stride = if experiment.name == "longplay" { 16 } else { 1 };
                match contact_sheet_strided(&dir, tile, sheet_stride) {
                    Some(sheet) => println!("    sheet: {}", sheet.display()),
                    None => println!("    sheet: FAILED (ffmpeg)"),
                }
                // The third artifact: the motion receipt. Cadence is
                // per-plate (a 2-frame 4K receipt is a slow A/B flip,
                // not a blink) — the recipe itself is one house line.
                let (gif_fps, gif_stride) = match experiment.name.as_ref() {
                    "hero" => (6, 1),
                    "hero4k" => (2, 1),
                    "longplay" => (12, 4),
                    _ => (12, 1),
                };
                match anim_gif_strided(&dir, gif_fps, 640, gif_stride) {
                    Some(g) => println!("    gif: {}", g.display()),
                    None => println!("    gif: FAILED (ffmpeg)"),
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
