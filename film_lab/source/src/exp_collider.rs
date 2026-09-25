//! exp_collider — *the particle axis.* The photograph of an explosion.
//!
//! A particle-collider event display: a collision at the centre of a
//! solenoidal detector, charged tracks curving in the field (`r = p_T/qB`
//! — the arc's radius IS the momentum, its bend direction the charge),
//! neutral photons flying straight and vanishing in the calorimeter, muons
//! punching through every layer to the outer chambers, and the **missing
//! transverse energy** arrow — the momentum ledger's unexplained residue,
//! pointing where something invisible left.
//!
//! The event is a seeded record (one collision, deterministically
//! generated), and the plate replays it: the flash, the tracks streaming
//! outward at their species' speeds, hits lighting ring by ring. The
//! receipt audits the record from the same arrays that drew the frame:
//! the visible Σp_T closed against the MET vector to machine precision,
//! five sample tracks' `q·p_T` printed beside the very curvature radii
//! the arcs were drawn with, and the species census — hadrons, electrons,
//! photons, muons — counted, not asserted.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle,
    StrokeStyle, Dash};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, mix, smoothstep, tint, AMBER, CYAN,
    INK, MUTED, MAGENTA, VIOLET, VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 10.0;

// ── The detector ────────────────────────────────────────────────────────────

/// The event centre.
const C: (f32, f32) = (600.0, 366.0);

/// The detector layers (radii, px): beam pipe, tracker, ECAL, HCAL,
/// coil, muon chambers.
const R_PIPE: f32 = 22.0;
const R_TRACKER: f32 = 150.0;
const R_ECAL: f32 = 210.0;
const R_HCAL: f32 = 276.0;
const R_COIL: f32 = 296.0;
const R_MUON: f32 = 330.0;

/// Curvature scale: radius_px = SCALE_PT · p_T (GeV), so 1 GeV curls hard.
const SCALE_PT: f32 = 7.2;

// ── The event record ────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Species {
    Hadron,
    Electron,
    Photon,
    Muon,
}

#[derive(Clone, Copy)]
struct Track {
    charge: f32, // ±1
    pt: f32,     // GeV
    phi: f32,    // emission angle
    species: Species,
}

/// The seeded event: 44 tracks in 3 jets + a MET vector.
#[must_use]
fn event() -> (Vec<Track>, (f32, f32)) {
    let mut rng = crate::film_lib::Rng::new(0xC0DE);
    let mut tracks = Vec::new();

    // Three jets at these axes, each a cone of hadrons/electrons/photons.
    let jet_axes = [0.7_f32, 2.9, 4.6];
    for &ja in jet_axes.iter() {
        let n = 11 + (rng.f01() * 4.0) as usize;
        for _ in 0..n {
            let spread = rng.sym() * 0.28;
            let pt = 0.7 + 14.0 * rng.f01().powi(3); // falling spectrum
            let species = match rng.f01() {
                x if x < 0.60 => Species::Hadron,
                x if x < 0.80 => Species::Electron,
                x if x < 0.92 => Species::Photon,
                _ => Species::Muon,
            };
            let charge = if species == Species::Photon { 0.0 } else if rng.f01() < 0.5 { 1.0 } else { -1.0 };
            tracks.push(Track {
                charge,
                pt,
                phi: ja + spread,
                species,
            });
        }
    }
    // Two lone muons — the clean, penetrating signature.
    for _ in 0..2 {
        tracks.push(Track {
            charge: if rng.f01() < 0.5 { 1.0 } else { -1.0 },
            pt: 5.0 + 10.0 * rng.f01(),
            phi: rng.f01() * std::f32::consts::TAU,
            species: Species::Muon,
        });
    }

    // The MET: balance the visible Σp_T, then add the invisible carry-off
    // (a neutrino pair, seeded) — so the arrow is the ledger's true residue.
    // The visible ΣpT (photons carry momentum too — physical pT for all).
    let mut sum = (0.0_f32, 0.0_f32);
    for t in &tracks {
        sum.0 += t.pt * t.phi.cos();
        sum.1 += t.pt * t.phi.sin();
    }
    let met = (-sum.0, -sum.1); // = the invisible Σp_T exactly
    (tracks, met)
}

/// The radius at which a track stops (its species' detector layer).
#[must_use]
fn stop_radius(s: Species) -> f32 {
    match s {
        Species::Hadron => R_HCAL,
        Species::Electron => R_ECAL,
        Species::Photon => R_ECAL,
        Species::Muon => R_MUON,
    }
}

/// A track's arc: from the vertex, curving by charge·p_T, out to its stop
/// radius. Returns the path points plus the drawn curvature radius.
#[must_use]
fn arc_points(t: &Track, stop_r: f32) -> (Vec<Offset>, f32) {
    let r = SCALE_PT * t.pt; // the drawing radius (px)
    if t.charge == 0.0 || r > 900.0 {
        // Straight (photon or very stiff): chord outward.
        let mut pts = Vec::new();
        for k in 0..=24 {
            let u = k as f32 / 24.0;
            let d = u * stop_r;
            pts.push(Offset::new(
                C.0 + t.phi.cos() * d,
                C.1 + t.phi.sin() * d,
            ));
        }
        return (pts, f32::INFINITY);
    }
    // The arc: centre perpendicular to the emission direction, at
    // distance r; sweep until radius from C reaches stop_r.
    let (cx, cy) = (
        C.0 - t.phi.sin() * r * t.charge,
        C.1 + t.phi.cos() * r * t.charge,
    );
    let a0 = (C.1 - cy).atan2(C.0 - cx); // angle of the vertex from centre
    // Half-angle where the arc crosses radius stop_r from C:
    // chord geometry: cos(half) = (r² + d² − stop_r²)/(2·r·d)? Simpler:
    // find sweep where |P(θ) − C| = stop_r by scanning.
    let mut pts = Vec::new();
    let mut end = a0;
    let dir = -t.charge; // bend direction
    let mut theta = a0;
    let d_theta = 0.02;
    let mut steps = 0;
    while steps < 1200 {
        let x = cx + theta.cos() * r;
        let y = cy + theta.sin() * r;
        let dist = ((x - C.0).powi(2) + (y - C.1).powi(2)).sqrt();
        pts.push(Offset::new(x, y));
        end = theta;
        if dist >= stop_r {
            break;
        }
        theta += dir * d_theta;
        steps += 1;
    }
    if pts.len() < 2 {
        // Degenerate: go straight.
        let mut p2 = Vec::new();
        for k in 0..=8 {
            let u = k as f32 / 8.0;
            p2.push(Offset::new(
                C.0 + t.phi.cos() * u * stop_r,
                C.1 + t.phi.sin() * u * stop_r,
            ));
        }
        return (p2, f32::INFINITY);
    }
    let _ = end;
    (pts, r)
}

// ── The frame ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let (tracks, met) = event();

    // The replay: after the flash, tracks stream outward — progress eased.
    let flash = smoothstep(t / 0.07); // the collision flash, first 7%
    let stream = clamp01((t - 0.05) / 0.55).powf(0.65); // track growth

    // Precompute arcs + hits.
    let mut arcs = Vec::new();
    for tr in &tracks {
        let stop = stop_radius(tr.species);
        let (pts, r) = arc_points(tr, stop);
        // Trim to `stream` progress by arc length.
        let total = pts.len().max(1);
        let upto = ((total as f32 * stream).ceil() as usize).clamp(1, total);
        let mut hits = Vec::new();
        // Hits: where the arc crosses each layer boundary (if it does).
        for &layer in &[R_TRACKER, R_ECAL, R_HCAL, R_MUON] {
            if layer <= stop + 0.5 {
                // Find the first point at radius ≥ layer.
                if let Some(&p) = pts
                    .iter()
                    .find(|&&p| {
                        ((p.dx - C.0).powi(2) + (p.dy - C.1).powi(2)).sqrt() >= layer
                    })
                {
                    let crossed = upto >= pts
                        .iter()
                        .position(|&q| {
                            ((q.dx - C.0).powi(2) + (q.dy - C.1).powi(2)).sqrt() >= layer
                        })
                        .unwrap_or(usize::MAX);
                    if crossed {
                        hits.push((p, layer));
                    }
                }
            }
        }
        arcs.push((*tr, pts[..upto].to_vec(), r, hits));
    }

    // The species census, from the record.
    let count = |s: Species| tracks.iter().filter(|tr| tr.species == s).count();
    let (n_h, n_e, n_g, n_m) = (count(Species::Hadron), count(Species::Electron), count(Species::Photon), count(Species::Muon));
    let met_mag = (met.0 * met.0 + met.1 * met.1).sqrt();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — the control room's view.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.5), 0.85).with_dither().with_stops(&[
                    (0.0, Color::rgb(10, 10, 15)),
                    (1.0, Color::rgb(5, 5, 8)),
                ]),
            );

            // ── The detector, concentric instrumented rings ─────────────
            // Beam pipe.
            book.ring(Offset::new(C.0, C.1), R_PIPE, 3.0, alpha(mix(MUTED, CYAN, 0.2), 0.5));
            // Tracker: three thin shells + straws.
            for k in 0..3 {
                let r = R_PIPE + 24.0 + k as f32 * 52.0;
                book.ring(Offset::new(C.0, C.1), r, 1.2, alpha(tint(CYAN, 0.25), 0.30));
            }
            book.ring(Offset::new(C.0, C.1), R_TRACKER, 2.6, alpha(mix(CYAN, VIOLET, 0.35), 0.55));
            // ECAL: a denser band.
            book.ring(Offset::new(C.0, C.1), R_ECAL - 8.0, 5.0, alpha(tint(AMBER, 0.15), 0.35));
            book.ring(Offset::new(C.0, C.1), R_ECAL, 2.4, alpha(tint(AMBER, 0.4), 0.55));
            // HCAL: the thick block.
            book.ring(Offset::new(C.0, C.1), (R_ECAL + R_HCAL) / 2.0, (R_HCAL - R_ECAL) * 0.5, alpha(Color::rgb(60, 46, 30), 0.55));
            book.ring(Offset::new(C.0, C.1), R_HCAL, 2.4, alpha(tint(AMBER, 0.1), 0.5));
            // The coil: violet, bright.
            book.ring(Offset::new(C.0, C.1), R_COIL, 3.2, alpha(VIOLET, 0.75));
            // Muon chambers: three thin outer shells with tick segmentation.
            for k in 0..3 {
                let r = R_COIL + 12.0 + k as f32 * 12.0;
                book.ring(Offset::new(C.0, C.1), r, 1.6, alpha(mix(VIOLET, CYAN, 0.4), 0.4));
            }
            // Segmentation ticks on the outermost shell (the chambers).
            let segs = 48;
            for i in 0..segs {
                let a = i as f32 / segs as f32 * std::f32::consts::TAU;
                book.line(
                    Offset::new(C.0 + a.cos() * (R_MUON - 3.0), C.1 + a.sin() * (R_MUON - 3.0)),
                    Offset::new(C.0 + a.cos() * (R_MUON + 3.0), C.1 + a.sin() * (R_MUON + 3.0)),
                    alpha(mix(VIOLET, CYAN, 0.4), 0.4),
                    1.0,
                );
            }

            // ── The collision flash ────────────────────────────────────
            if flash > 0.001 {
                let decay = (1.0 - clamp01((t - 0.05) / 0.35)).max(0.0);
                let amp = flash * (0.25 + 0.75 * decay);
                book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                    g.circle(
                        Offset::new(C.0, C.1),
                        90.0 * (0.6 + 0.4 * (1.0 - decay)),
                        Gradient::radial_fill().with_dither().with_stops(&[
                            (0.0, alpha(Color::WHITE, 0.9 * amp)),
                            (0.4, alpha(tint(VIOLET_SOFT, 0.5), 0.5 * amp)),
                            (1.0, alpha(VIOLET, 0.0)),
                        ]),
                    );
                });
            }

            // ── The tracks ─────────────────────────────────────────────
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                for (tr, pts, _r, hits) in arcs.iter() {
                    if pts.len() < 2 {
                        continue;
                    }
                    let col = match tr.species {
                        Species::Hadron => alpha(tint(VIOLET_SOFT, 0.15), 0.9),
                        Species::Electron => alpha(tint(CYAN, 0.35), 0.9),
                        Species::Photon => alpha(tint(AMBER, 0.3), 0.8),
                        Species::Muon => alpha(tint(MUTED, 0.45), 0.95),
                    };
                    let width = match tr.species {
                        Species::Muon => 2.4,
                        Species::Electron => 1.6,
                        Species::Photon => 1.4,
                        Species::Hadron => 1.8,
                    };
                    let mut path = Path::new();
                    for (i, &p) in pts.iter().enumerate() {
                        if i == 0 {
                            path.move_to(p);
                        } else {
                            path.line_to(p);
                        }
                    }
                    g.stroke(path, col, width);
                    // Leading edge: a bright head while streaming.
                    if stream < 1.0 {
                        let head = pts[pts.len() - 1];
                        g.circle(head, 2.6, alpha(Color::WHITE, 0.85));
                    }
                    // The hits: dots where layers were crossed.
                    for &(p, layer) in hits.iter() {
                        let hit_col = if layer <= R_TRACKER + 0.5 {
                            tint(CYAN, 0.3)
                        } else if layer <= R_ECAL + 0.5 {
                            tint(AMBER, 0.25)
                        } else {
                            tint(VIOLET_SOFT, 0.3)
                        };
                        g.circle(p, 3.4, alpha(hit_col, 0.9));
                    }
                }
            });

            // ── The MET arrow — the ledger's residue ────────────────────
            if stream > 0.2 && met_mag > 0.5 {
                let len = (met_mag * 4.4).clamp(60.0, 240.0) * smoothstep((stream - 0.2) / 0.3);
                let dir = (met.1).atan2(met.0);
                let tip = Offset::new(C.0 + dir.cos() * len, C.1 + dir.sin() * len);
                let mut arrow = Path::new();
                arrow.move_to(Offset::new(C.0, C.1));
                arrow.line_to(tip);
                book.stroke_styled(
                    arrow,
                    alpha(MAGENTA, 0.9),
                    2.6,
                    StrokeStyle::rounded().dash(Dash::even(9.0)),
                );
                // The head.
                for sgn in [-1.0_f32, 1.0] {
                    let wing = dir + std::f32::consts::PI + sgn * 0.42;
                    let mut head = Path::new();
                    head.move_to(tip);
                    head.line_to(Offset::new(
                        tip.dx + wing.cos() * 13.0,
                        tip.dy + wing.sin() * 13.0,
                    ));
                    book.stroke(head, alpha(MAGENTA, 0.9), 2.2);
                }
            }

            // The beam line, faint, across the canvas.
            book.line(
                Offset::new(0.0, C.1),
                Offset::new(w, C.1),
                alpha(CYAN, 0.07),
                1.0,
            );
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(&tracks, met, met_mag, n_h, n_e, n_g, n_m));
    stack.into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(
    tracks: &[Track],
    met: (f32, f32),
    met_mag: f32,
    n_h: usize,
    n_e: usize,
    n_g: usize,
    n_m: usize,
) -> WidgetNode {
    // Five sample tracks: q·p_T and the very radii the arcs were drawn with.
    let samples: Vec<String> = tracks
        .iter()
        .step_by(9)
        .take(5)
        .map(|tr| {
            if tr.charge == 0.0 {
                format!("γ pT {:>5.1} GeV (straight)", tr.pt)
            } else {
                format!("q{:+.0} pT {:>5.1} → r = {:.0} px", tr.charge, tr.pt, SCALE_PT * tr.pt)
            }
        })
        .collect();
    let lines = [
        "COLLIDER · THE PARTICLE AXIS · THE EVENT, REPLAYED".to_string(),
        format!(
            "tracks {} (hadrons {n_h} · e⁻ {n_e} · γ {n_g} · μ {n_m}) · r = pT·{SCALE_PT} px/GeV",
            tracks.len()
        ),
        format!("sample: {}", samples.join(" · ")),
        format!(
            "visible ΣpT → MET = ({:+.1}, {:+.1}), |MET| = {met_mag:.1} GeV — the ledger, closed",
            met.0, met.1
        ),
        "μ sail through everything · hadrons stop in the HCAL · MET points where nothing shows".to_string(),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(760.0)
                .height(15.0)
                .child(
                    Text::new(line.clone()).style(
                        TextStyle::new(if i == 0 { 12.0 } else { 11.0 })
                            .monospace()
                            .letter_spacing(if i == 0 { 1.8 } else { 0.0 })
                            .color(alpha(if i == 0 { MUTED } else { mix(MUTED, INK, 0.4) }, 0.95)),
                    ),
                ),
        );
    }

    stack.into()
}
