//! filmkit — the A+ kit: everything this film added for its second cut.
//!
//! Four rooms, one module:
//!
//! * **The camera.** A per-scene camera plan ([`sf_camera`]) with real
//!   moves — drifts, pushes, tracks, a dolly, a pull-back — and the cut
//!   handles ([`shot_sf`]) that make every join enter moving and leave
//!   accelerating. The v1 film watched from a tripod; this one moves.
//!
//! * **The joins.** A transition table ([`enter_join`] / [`exit_join`]):
//!   pushes that carry across the cut (the outgoing scene slides out the
//!   same door the incoming one slides in from), a digital glitch, and
//!   an iris. Implemented in the master — [`Slide`] becomes a screen-space
//!   translate composed with the camera, [`JoinFx::Glitch`] and
//!   [`JoinFx::Iris`] become post-raster pixel passes ([`post_fx`]).
//!
//! * **The flows.** Vector graphic flows: arc-length-sampled cubics,
//!   self-drawing strokes ([`grow_thread`]), gradient dashes flowing
//!   along a path ([`flow_thread`]), comet riders ([`rider`]), tapered
//!   ribbons, orbiting particles, projected 3D grid floors and cube
//!   orbits. The framework gives the shapes; this file gives them time.
//!
//! * **The studio kit.** The two-tone wordmark ([`wordmark`] — `vieww`
//!   in ink, `studio` in the brand's purple, by demand), self-drawing
//!   corner brackets, callout threads, the token flood, and the light
//!   sweep.
//!
//! Everything here is a pure function of the frame's clock — the film's
//! own contract holds: a frame is a function of its index, and the
//! census can re-derive any of it.

use vieww_foundation::{
    Color, Dash, Gradient, Offset, Path, Rect, Size, Sketchbook, StrokeStyle, TextAlign, Transform,
};
use vieww_widget::prelude::*;
use vieww_widget::{RichText, Span};

use crate::film_lib::{clamp01, ease_in_out, ease_out_cubic, ease_out_expo, spring_out, Rng};
use crate::product_film as pf;
use super::{BRAND_FAR, BRAND_NEAR, CANVAS, H, INK, W};

// ── The camera ──────────────────────────────────────────────────────────────

/// The handle either side of a cut — a hair longer than the product
/// film's, because this film's moves travel further.
pub const CUT_IN: f32 = 0.42;
pub const CUT_OUT: f32 = 0.30;

/// This film's camera plan — one arm per scene, one idea per arm.
///
/// Every zoom stays at or above 1.0 except the pullback (Z12), whose
/// room is painted with overscan margin precisely so the camera may
/// leave the canvas. The studio's opening scene holds a dead-still
/// camera for its first 0.4 s — the brief asks for the *exact* studio
/// on the first frame, and the first frame obliges: identity transform,
/// no overlay, no ramp.
pub fn sf_camera(id: &str, t: f32, sec: f32) -> pf::Cam {
    match id {
        // The picture on its stage: a slow drift toward the card, the
        // audience leaning in before they know why.
        "Z01" => pf::cam_lerp(
            pf::Cam::at(1.030, 820.0, 520.0),
            pf::Cam::at(1.065, 1010.0, 500.0),
            ease_in_out(t),
        ),

        // Push toward the gauge — the cost meter earns the focus, while
        // the gates keep their place in the frame.
        // The push toward the gauge is bounded by what it must not
        // crop: the gate list starts at x=160 and the gauge ends at
        // x=1730, so at zoom 1.05 the held point can only live between
        // 816 and 1074. The v2 cut held 1250, and sliced every gate
        // label in half for the back three quarters of the scene.
        "Z02" => pf::cam_lerp(
            pf::Cam::zoom(1.0),
            pf::Cam::at(1.048, 1046.0, 540.0),
            ease_in_out(clamp01((t - 0.10) / 0.76)),
        ),

        // Dolly into the point of light. The 3D grid floor underneath
        // moves with it — the one shot where the world itself has depth.
        "Z03" => pf::cam_lerp(
            pf::Cam::at(1.00, W * 0.5, H * 0.46),
            pf::Cam::at(1.14, W * 0.5, H * 0.45),
            ease_in_out(clamp01(t / 0.92)),
        ),

        // Pan across the crate constellation, engine heart to studio node —
        // framed so the wordmark and the graph both survive the move.
        "Z04" => pf::cam_lerp(
            pf::Cam::at(1.045, 660.0, 545.0),
            pf::Cam::at(1.055, 1020.0, 520.0),
            ease_in_out(t),
        ),

        // Track the pipeline left to right with the packets — the
        // metaphor is serial, so the shot is. The travel ends framed on
        // the whole line, first station included.
        // The pipeline's five stations span x 150–1758, which leaves a
        // lateral track no room: at any zoom that frames the line, the
        // held point can only move a few dozen pixels before the first
        // station leaves the frame — and in the v2 cut it did, for the
        // back half of the scene. The move that *is* available is a
        // lean: the shot pushes in on the line as the packets start to
        // travel, and the packets carry the lateral motion instead.
        "Z05" => {
            let lean = ease_in_out(clamp01((t - 0.12) / 0.72));
            pf::cam_lerp(
                pf::Cam::at(1.000, 960.0, 540.0),
                pf::Cam::at(1.098, 954.0, 524.0),
                lean,
            )
        }

        // A gentle fall down the three lanes.
        "Z06" => pf::cam_lerp(
            pf::Cam::at(1.040, 960.0, 470.0),
            pf::Cam::at(1.075, 960.0, 565.0),
            ease_in_out(t),
        ),

        // The studio: exact and still for the first beat, then the only
        // push the product's pixels ever ride — framed so the corner
        // brackets (inset at 36) survive the move.
        "Z07" => {
            if sec < 0.40 {
                pf::Cam::STILL
            } else {
                pf::cam_lerp(
                    pf::Cam::STILL,
                    pf::Cam::at(1.035, 960.0, 525.0),
                    ease_out_cubic(clamp01((sec - 0.40) / 1.7)),
                )
            }
        }

        // Into the editor — where the edit happens is where we look.
        // Bounded so the shell (x 118–1802) keeps both its ends: at
        // zoom 1.06 the held point lives between 906 and 1014.
        "Z08" => pf::cam_lerp(
            pf::Cam::zoom(1.0),
            pf::Cam::at(1.058, 950.0, 556.0),
            ease_in_out(clamp01((t - 0.06) / 0.60)),
        ),

        // Track the compile, then feel the taps.
        "Z09" => {
            let base = pf::cam_lerp(
                pf::Cam::at(1.046, 958.0, 544.0),
                pf::Cam::at(1.000, 960.0, 540.0),
                ease_in_out(t),
            );
            // The presses are the explanation's own, at 8.0 s and 9.4 s
            // into the scene — the v2 numbers were absolute film seconds
            // from when this scene rode the live session, and never fired.
            let (d1x, d1y) = pf::cam_shake(sec - 8.0, 6.0);
            let (d2x, d2y) = pf::cam_shake(sec - 9.4, 4.5);
            base.nudged(d1x + d2x, d1y + d2y)
        }

        // The fleet spans x 184–1620, so a lateral pass would walk the
        // first device out of frame. The move that fits is a lean into
        // the three of them as the third lands.
        "Z10" => pf::cam_lerp(
            pf::Cam::at(1.000, 960.0, 540.0),
            pf::Cam::at(1.062, 958.0, 556.0),
            ease_in_out(clamp01((t - 0.08) / 0.70)),
        ),

        // The ledger falls through its three registers: framed on the
        // hero cards, easing down and out until the whole audit — cards,
        // gauge, and the film's own row — is in one frame at the end.
        // The travel is small by construction: the registers themselves
        // carry the motion, and the camera only follows the reading eye.
        "Z11" => pf::cam_lerp(
            pf::Cam::at(1.075, 960.0, 506.0),
            pf::Cam::at(1.020, 960.0, 546.0),
            ease_in_out(clamp01((t - 0.06) / 0.80)),
        ),

        // The pullback, actually pulling back — below 1.0 is allowed
        // here, the room is painted overscan for exactly this reason.
        "Z12" => pf::cam_lerp(
            pf::Cam::at(1.140, 960.0, 545.0),
            pf::Cam::at(0.960, 960.0, 540.0),
            ease_out_expo(clamp01((t - 0.05) / 0.80)),
        ),

        // A last settle onto the mark.
        "Z13" => pf::cam_lerp(
            pf::Cam::at(1.045, 960.0, 470.0),
            pf::Cam::at(1.000, 960.0, 480.0),
            ease_out_expo(clamp01(t / 0.50)),
        ),

        // The hold: still — and pinned to exactly where the end card's
        // camera came to rest, so the cut between them does not exist.
        "Z14" => pf::Cam::at(1.0, 960.0, 480.0),

        _ => pf::Cam::STILL,
    }
}

/// The camera a scene is entered from and left towards — bigger moves at
/// movement breaks, none at all into the studio's exact first frame.
fn cut_handles(index: usize, base: pf::Cam) -> (pf::Cam, pf::Cam) {
    let movement_break = matches!(index, 3 | 6 | 10 | 11);
    let into_studio = index == 6;
    let studio_act = matches!(index, 6 | 7 | 8 | 9);

    // The studio's first scene enters dead still — its first frame is
    // the brief's "exact studio" and nothing may move it.
    if into_studio {
        return (base, base);
    }

    let (zin, lift) = if into_studio || index == 10 {
        (1.070, 24.0)
    } else if movement_break {
        (1.052, 18.0)
    } else if studio_act {
        (1.030, 12.0)
    } else {
        (1.035, 12.0)
    };

    let enter = pf::Cam {
        zoom: base.zoom * zin,
        at: vieww_foundation::Offset::new(base.at.dx, base.at.dy + lift),
    };
    let leave = pf::Cam {
        zoom: base.zoom * (2.0 - zin).max(0.85),
        at: vieww_foundation::Offset::new(base.at.dx, base.at.dy - lift * 0.8),
    };
    (enter, leave)
}

/// The full shot for a frame: the scene's camera plan with the cut's
/// handles composed on either end. The studio's opening and the film's
/// hold are exempt — one must be exact, the other must be held.
pub fn shot_sf(id: &str, index: usize, seconds: f32, t: f32, sec: f32) -> pf::Shot {
    let base = sf_camera(id, t, sec);
    let (enter, leave) = cut_handles(index, base);
    let first = index == 0;
    // Scenes that must not be moved by a join: the studio's exact first
    // frame, and the end card → hold pair, whose content is identical —
    // the cut between them should not exist at all.
    let pristine = id == "Z07" || id == "Z14";
    let held = id == "Z13" || id == "Z14";
    let last = id == "Z14";

    if !first && !pristine && sec < CUT_IN {
        let e = ease_out_expo(clamp01(sec / CUT_IN));
        return pf::Shot {
            cam: pf::cam_lerp(enter, base, e),
            alpha: clamp01(sec / (CUT_IN * 0.62)),
        };
    }

    let remaining = seconds - sec;
    if !last && !held && remaining < CUT_OUT {
        let l = pf::ease_in_cubic(clamp01(1.0 - remaining / CUT_OUT));
        return pf::Shot {
            cam: pf::cam_lerp(base, leave, l),
            alpha: 1.0 - pf::ease_in_cubic(clamp01(1.0 - remaining / (CUT_OUT * 0.72))),
        };
    }

    pf::Shot { cam: base, alpha: 1.0 }
}

// ── The joins ───────────────────────────────────────────────────────────────

/// A screen-space slide the master composes over the camera — the push
/// that carries across a cut. Units are fractions of the canvas.
#[derive(Clone, Copy, PartialEq)]
pub struct Slide {
    /// Fraction of W to shift at the join's extremes (negative = left).
    pub dx: f32,
    /// Fraction of H to shift (negative = up).
    pub dy: f32,
}

impl Slide {
    pub const NONE: Slide = Slide { dx: 0.0, dy: 0.0 };
    pub fn offset_at(self, p: f32) -> Offset {
        Offset::new(self.dx * W * p, self.dy * H * p)
    }
    pub fn is_none(self) -> bool {
        self.dx.abs() < 1.0e-4 && self.dy.abs() < 1.0e-4
    }
}

/// Post-raster effects — the ones that need pixels, not commands.
#[derive(Clone, Copy, PartialEq)]
pub enum JoinFx {
    None,
    /// Chromatic split + row slices, a compile's worth of static.
    Glitch,
    /// A radial mask collapsing onto (or opening from) a world point.
    Iris { x: f32, y: f32 },
}

/// The state of the incoming join at `sec` — what the master composes.
#[derive(Clone, Copy)]
pub struct JoinState {
    pub slide: Slide,
    pub fx: JoinFx,
    /// How far through the join, 0 → 1 (0 = fully joined-out/in).
    pub p: f32,
    /// True when this is the scene's *entering* join; false for its exit.
    pub enter: bool,
}

impl JoinState {
    pub fn is_quiet(&self) -> bool {
        self.slide.is_none() && self.fx == JoinFx::None
    }
}

/// A scene's incoming join: how it arrives.
fn enter_join(id: &str, sec: f32) -> JoinState {
    let dur = 0.46;
    let p = ease_out_expo(clamp01(sec / dur));
    let enter = true;
    match id {
        // Z01 opens the film — it arrives by its own content, not a join.
        "Z01" => JoinState { slide: Slide::NONE, fx: JoinFx::None, p: 1.0, enter },
        // Z02 rides Z01's push out of frame: in from the right.
        "Z02" => JoinState { slide: Slide { dx: 1.0, dy: 0.0 }, fx: JoinFx::None, p, enter },
        // Z03 rides Z02's upward push: in from below.
        "Z03" => JoinState { slide: Slide { dx: 0.0, dy: 1.0 }, fx: JoinFx::None, p, enter },
        // Z04 opens out of the question's iris.
        "Z04" => JoinState {
            slide: Slide::NONE,
            fx: JoinFx::Iris { x: W * 0.5, y: H * 0.45 },
            p: ease_out_cubic(clamp01(sec / 0.55)),
            enter,
        },
        // Z05 continues the left-to-right journey.
        "Z05" => JoinState { slide: Slide { dx: 1.0, dy: 0.0 }, fx: JoinFx::None, p, enter },
        // Z06 rides Z05's upward push.
        "Z06" => JoinState { slide: Slide { dx: 0.0, dy: 1.0 }, fx: JoinFx::None, p, enter },
        // Z07 — the exact studio. No join at all; the cut *is* the beat.
        "Z07" => JoinState { slide: Slide::NONE, fx: JoinFx::None, p: 1.0, enter },
        // Z08 continues the studio act's lateral grammar.
        "Z08" => JoinState { slide: Slide { dx: 1.0, dy: 0.0 }, fx: JoinFx::None, p, enter },
        // Z09 — the compile arrives with static.
        "Z09" => JoinState { slide: Slide { dx: 0.0, dy: 1.0 }, fx: JoinFx::Glitch, p, enter },
        // Z10 continues the push chain.
        "Z10" => JoinState { slide: Slide { dx: 1.0, dy: 0.0 }, fx: JoinFx::None, p, enter },
        // Z11 — the ledger dissolves in; the numbers need no theatre.
        "Z11" => JoinState { slide: Slide::NONE, fx: JoinFx::None, p: 1.0, enter },
        // Z12 — the release, entered gently.
        "Z12" => JoinState { slide: Slide::NONE, fx: JoinFx::None, p: 1.0, enter },
        // Z13 — the end card, likewise.
        "Z13" => JoinState { slide: Slide::NONE, fx: JoinFx::None, p: 1.0, enter },
        _ => JoinState { slide: Slide::NONE, fx: JoinFx::None, p: 1.0, enter },
    }
}

/// A scene's outgoing join: how it leaves.
fn exit_join(id: &str, seconds: f32, sec: f32) -> JoinState {
    let remaining = seconds - sec;
    let dur = 0.34;
    // p counts *through* the leave: 0 = not started, 1 = fully gone.
    let p = pf::ease_in_cubic(clamp01(1.0 - remaining / dur));
    let enter = false;
    match id {
        // Z01 pushes left out; Z02 enters from the right. One door.
        "Z01" => JoinState { slide: Slide { dx: -1.15, dy: 0.0 }, fx: JoinFx::None, p, enter },
        // Z02 pushes up out; Z03 enters from below.
        "Z02" => JoinState { slide: Slide { dx: 0.0, dy: -1.15 }, fx: JoinFx::None, p, enter },
        // Z03 — the question closes like an eye, onto the light itself.
        "Z03" => JoinState {
            slide: Slide::NONE,
            fx: JoinFx::Iris { x: W * 0.5, y: H * 0.44 },
            p: pf::ease_in_cubic(clamp01(1.0 - remaining / 0.55)),
            enter,
        },
        // Z04 pushes left out; Z05 enters from the right.
        "Z04" => JoinState { slide: Slide { dx: -1.15, dy: 0.0 }, fx: JoinFx::None, p, enter },
        // Z05 pushes up out; Z06 enters from below.
        "Z05" => JoinState { slide: Slide { dx: 0.0, dy: -1.15 }, fx: JoinFx::None, p, enter },
        // Z06 dissolves into the studio — the luminance ramp alone.
        "Z06" => JoinState { slide: Slide::NONE, fx: JoinFx::None, p: 0.0, enter },
        // Z07 pushes left out; Z08 enters from the right.
        "Z07" => JoinState { slide: Slide { dx: -1.15, dy: 0.0 }, fx: JoinFx::None, p, enter },
        // Z08 pushes up out; Z09 enters from below — with static.
        "Z08" => JoinState { slide: Slide { dx: 0.0, dy: -1.15 }, fx: JoinFx::None, p, enter },
        // Z09 pushes left out; Z10 enters from the right.
        "Z09" => JoinState { slide: Slide { dx: -1.15, dy: 0.0 }, fx: JoinFx::None, p, enter },
        // Z10 dissolves into the ledger.
        "Z10" => JoinState { slide: Slide::NONE, fx: JoinFx::None, p: 0.0, enter },
        // Z11 — the receipts glitch apart into the release's dark.
        "Z11" => JoinState {
            slide: Slide::NONE,
            fx: JoinFx::Glitch,
            p: pf::ease_in_cubic(clamp01(1.0 - remaining / 0.50)),
            enter,
        },
        _ => JoinState { slide: Slide::NONE, fx: JoinFx::None, p: 0.0, enter },
    }
}

/// The composed join state for frame `sec` of scene `id`: the enter
/// join runs first; the exit join owns the scene's tail. They never
/// overlap at this film's lengths.
pub fn join_state(id: &str, seconds: f32, sec: f32) -> JoinState {
    let enter = enter_join(id, sec);
    if !enter.is_quiet() || (id != "Z01" && sec < 0.60) {
        return enter;
    }
    let exit = exit_join(id, seconds, sec);
    if exit.p > 0.001 {
        return exit;
    }
    JoinState { slide: Slide::NONE, fx: JoinFx::None, p: 1.0, enter: false }
}

// ── Post-raster pixel passes ────────────────────────────────────────────────

/// Apply a join's pixel effect to a rendered RGBA buffer. `strength`
/// is the effect's own envelope — the master derives it from the join
/// direction (an entering glitch fades as the join completes; an
/// exiting one grows). Deterministic in the frame's clock: the
/// glitch's bands are seeded by the absolute frame index, so the
/// census and the master agree frame for frame.
pub fn post_fx(buf: &mut [u8], rw: u32, rh: u32, fx: JoinFx, strength: f32, frame_i: u64) {
    match fx {
        JoinFx::None => {}
        JoinFx::Glitch => glitch_pass(buf, rw, rh, strength, frame_i),
        JoinFx::Iris { x, y } => iris_pass(buf, rw, rh, x, y, strength),
    }
}

/// The glitch: chromatic aberration whose split widens with `strength`,
/// plus horizontal band slices with deterministic offsets, plus a
/// bright tear line. Only lives in a join window.
fn glitch_pass(buf: &mut [u8], rw: u32, rh: u32, strength: f32, frame_i: u64) {
    let (rw, rh) = (rw as usize, rh as usize);
    let strength = strength.clamp(0.0, 1.0);
    if strength <= 0.02 {
        return;
    }
    let split = (2.0 + 14.0 * strength) as isize;
    let mut rng = Rng::new(0x61A7 ^ frame_i.wrapping_mul(0x9E3779B9));

    // Chromatic split: sample R from +split, B from −split.
    let src = buf.to_vec();
    for y in 0..rh {
        let row = y * rw * 4;
        for x in 0..rw {
            let xr = (x as isize + split).clamp(0, rw as isize - 1) as usize;
            let xb = (x as isize - split).clamp(0, rw as isize - 1) as usize;
            let d = row + x * 4;
            buf[d] = src[row + xr * 4];
            buf[d + 1] = src[d + 1];
            buf[d + 2] = src[row + xb * 4 + 2];
        }
    }

    // Band slices: 4 bands shift by deterministic amounts.
    let bands = 4;
    for b in 0..bands {
        let y0 = (rng.f01() * (rh - 24) as f32) as usize;
        let bh = (18.0 + rng.f01() * 70.0) as usize;
        let shift = ((rng.sym() * 90.0 * strength) as isize) * 4;
        for y in y0..(y0 + bh).min(rh) {
            let row = y * rw * 4;
            let row_buf: Vec<u8> = buf[row..row + rw * 4].to_vec();
            for x in 0..rw * 4 {
                let xs = (x as isize + shift).rem_euclid((rw * 4) as isize) as usize;
                buf[row + x] = row_buf[xs];
            }
        }
    }

    // The tear — one bright scanline near the split's edge.
    let tear_y = (rh as f32 * (0.2 + 0.6 * rng.f01())) as usize;
    if tear_y < rh {
        let row = tear_y * rw * 4;
        for x in 0..rw * 4 {
            buf[row + x] = buf[row + x].saturating_add(36);
        }
    }
}

/// The iris: a radial mask about a world point. `strength` is 1 when
/// fully open; radius runs 0 → frame diagonal with a feathered edge.
fn iris_pass(buf: &mut [u8], rw: u32, rh: u32, cx: f32, cy: f32, strength: f32) {
    let (rw, rh) = (rw as f32, rh as f32);
    let max_r = (rw.hypot(rh)) * 0.62;
    let r = max_r * clamp01(strength);
    let feather = 26.0_f32.max(max_r * 0.02);
    let (cx, cy) = (cx * rw / W, cy * rh / H);
    for y in 0..rh as usize {
        let dy = y as f32 + 0.5 - cy;
        let row = y * rw as usize * 4;
        for x in 0..rw as usize {
            let dx = x as f32 + 0.5 - cx;
            let d = dx.hypot(dy);
            if d <= r - feather {
                continue;
            }
            let mask = clamp01((d - (r - feather)) / (2.0 * feather));
            let k = 1.0 - mask;
            let i = row + x * 4;
            buf[i] = (buf[i] as f32 * k) as u8;
            buf[i + 1] = (buf[i + 1] as f32 * k) as u8;
            buf[i + 2] = (buf[i + 2] as f32 * k) as u8;
        }
    }
}

// ── Vector flows ────────────────────────────────────────────────────────────

/// Flatten a cubic bezier into a polyline of `n` points.
pub fn cubic_pts(p0: Offset, c1: Offset, c2: Offset, p1: Offset, n: usize) -> Vec<Offset> {
    let n = n.max(2);
    (0..=n)
        .map(|i| {
            let u = i as f32 / n as f32;
            let v = 1.0 - u;
            let a = v * v * v;
            let b = 3.0 * v * v * u;
            let c = 3.0 * v * u * u;
            let d = u * u * u;
            Offset::new(
                a * p0.dx + b * c1.dx + c * c2.dx + d * p1.dx,
                a * p0.dy + b * c1.dy + c * c2.dy + d * p1.dy,
            )
        })
        .collect()
}

/// An S-curve thread between two points — the film's standard connector.
pub fn thread_pts(from: Offset, to: Offset, bend: f32) -> Vec<Offset> {
    let dx = to.dx - from.dx;
    let dy = to.dy - from.dy;
    let c1 = Offset::new(from.dx + dx * 0.42, from.dy + bend);
    let c2 = Offset::new(to.dx - dx * 0.42, to.dy - bend);
    cubic_pts(from, c1, c2, to, 44)
}

fn poly_len(pts: &[Offset]) -> f32 {
    pts.windows(2)
        .map(|w| (w[1].dx - w[0].dx).hypot(w[1].dy - w[0].dy))
        .sum()
}

/// A point `s` of the way along the polyline (by arc length).
pub fn point_at(pts: &[Offset], s: f32) -> Offset {
    if pts.is_empty() {
        return Offset::new(0.0, 0.0);
    }
    if pts.len() == 1 || s <= 0.0 {
        return pts[0];
    }
    let total = poly_len(pts);
    let target = total * s.clamp(0.0, 1.0);
    let mut acc = 0.0;
    for w in pts.windows(2) {
        let seg = (w[1].dx - w[0].dx).hypot(w[1].dy - w[0].dy);
        if acc + seg >= target {
            let u = if seg < 1.0e-6 { 0.0 } else { (target - acc) / seg };
            return Offset::new(
                w[0].dx + (w[1].dx - w[0].dx) * u,
                w[0].dy + (w[1].dy - w[0].dy) * u,
            );
        }
        acc += seg;
    }
    *pts.last().unwrap()
}

fn path_through(pts: &[Offset]) -> Path {
    let mut path = Path::new();
    if let Some(first) = pts.first() {
        path.move_to(*first);
        for p in &pts[1..] {
            path.line_to(*p);
        }
    }
    path
}

/// A self-drawing stroke: the dash-length trick — the framework's own
/// `Dash` does the arithmetic, round caps round the growing tip.
pub fn grow_stroke(book: &mut Sketchbook, pts: &[Offset], frac: f32, color: Color, width: f32, a: f32) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.01 || frac <= 0.005 || pts.len() < 2 {
        return;
    }
    let total = poly_len(pts);
    let draw = total * clamp01(frac);
    let style = StrokeStyle::rounded().dash(Dash::new(vec![draw, total + 1.0]));
    book.stroke_styled(path_through(pts), pf::alpha(color, a), width, style);
}

/// A comet rider: a glowing dot at `s` of the way along `pts`, with a
/// fading trail trailing behind it.
pub fn rider(book: &mut Sketchbook, pts: &[Offset], s: f32, color: Color, r: f32, a: f32) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.01 || pts.len() < 2 {
        return;
    }
    let trail = 8;
    for k in (1..=trail).rev() {
        let back = (s - k as f32 * 0.012).max(0.0);
        let p = point_at(pts, back);
        let fade = 1.0 - k as f32 / (trail + 1) as f32;
        book.circle(p, r * fade, pf::alpha(color, 0.30 * fade * a));
    }
    let head = point_at(pts, s.clamp(0.0, 1.0));
    book.layer(a, 7.0, None, |b| {
        b.circle(head, r * 1.5, pf::alpha(color, 0.55));
    });
    book.circle(head, r, pf::alpha(Color::WHITE, 0.9 * a));
}

/// A flowing thread: the full path drawn faint, gradient dashes riding
/// it toward the destination, one bright head leading the pack. This is
/// the film's "data in transit" — used wherever something flows.
pub fn flow_thread(
    book: &mut Sketchbook,
    from: Offset,
    to: Offset,
    bend: f32,
    t: f32,
    color: Color,
    a: f32,
    width: f32,
) {
    let pts = thread_pts(from, to, bend);
    flow_along(book, &pts, t, color, a, width, 0.22);
}

/// [`flow_thread`] over an arbitrary polyline — the flow engine the
/// multi-station scenes (the compile pipeline, the rails) share.
pub fn flow_along(
    book: &mut Sketchbook,
    pts: &[Offset],
    t: f32,
    color: Color,
    a: f32,
    width: f32,
    speed: f32,
) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.01 || pts.len() < 2 {
        return;
    }
    // The faint carrier.
    book.stroke(path_through(pts), pf::alpha(color, 0.14 * a), width * 0.7);
    // The flowing dashes.
    let n = 9;
    for k in 0..n {
        let u = ((k as f32 / n as f32) + t * speed) % 1.0;
        let u2 = (u + 0.045).min(1.0);
        let p0 = point_at(pts, u);
        let p1 = point_at(pts, u2);
        book.line(p0, p1, pf::alpha(color, 0.65 * a), width);
    }
    // The head.
    rider(book, pts, (t * speed * 1.15) % 1.0, color, width * 1.1, a);
}

/// A tapered ribbon along `pts` — width breathes along its length.
/// Drawn as short segments whose width follows a sine envelope.
pub fn ribbon(book: &mut Sketchbook, pts: &[Offset], width: f32, t: f32, color: Color, a: f32) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.01 || pts.len() < 3 {
        return;
    }
    let total = poly_len(pts);
    let mut acc = 0.0;
    for w in pts.windows(2) {
        let seg = (w[1].dx - w[0].dx).hypot(w[1].dy - w[0].dy);
        let u = if total < 1.0 { 0.0 } else { acc / total };
        acc += seg;
        let env = 0.35 + 0.65 * (u * std::f32::consts::TAU * 1.5 + t * 1.3).sin().abs();
        book.line(w[0], w[1], pf::alpha(color, 0.5 * a * env), width * env);
    }
}

/// Particles orbiting an ellipse — orbits with trails, not orbits with
/// dots. `speed` is radians per second of film time.
#[allow(clippy::too_many_arguments)]
pub fn orbit_dots(
    book: &mut Sketchbook,
    center: Offset,
    rx: f32,
    ry: f32,
    n: usize,
    t: f32,
    speed: f32,
    color: Color,
    a: f32,
    r: f32,
) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.01 {
        return;
    }
    for k in 0..n {
        let base = t * speed + k as f32 * std::f32::consts::TAU / n as f32;
        for step in 0..5 {
            let ang = base - step as f32 * 0.055;
            let p = Offset::new(center.dx + rx * ang.cos(), center.dy + ry * ang.sin());
            let fade = 1.0 - step as f32 / 5.0;
            book.circle(p, r * fade, pf::alpha(color, (0.16 + 0.5 * fade) * a));
        }
    }
}

// ── 3D kit ──────────────────────────────────────────────────────────────────

/// A projected grid floor — the vector way to do depth: lines, not
/// faces. Lines along x and z on the y=0 plane, faded by fog depth.
pub fn grid_floor_lines(
    book: &mut Sketchbook,
    cam: &crate::three_d::Camera,
    canvas: Size,
    half_x: f32,
    z_near: f32,
    z_far: f32,
    step: f32,
    color: Color,
    a: f32,
    fog_near: f32,
    fog_far: f32,
) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.01 {
        return;
    }
    let mut line = |book: &mut Sketchbook, p0: crate::three_d::Vec3, p1: crate::three_d::Vec3, alpha: f32| {
        if let (Some((a0, d0, _)), Some((a1, _, _))) =
            (cam.project(p0, canvas), cam.project(p1, canvas))
        {
            let fog = clamp01((d0 - fog_near) / (fog_far - fog_near));
            book.line(a0, a1, pf::alpha(color, alpha * (1.0 - fog) * a), 1.1);
        }
    };
    let mut z = z_near;
    while z <= z_far {
        line(
            book,
            crate::three_d::Vec3::new(-half_x, 0.0, z),
            crate::three_d::Vec3::new(half_x, 0.0, z),
            0.5,
        );
        z += step;
    }
    let mut x = -half_x;
    while x <= half_x {
        line(
            book,
            crate::three_d::Vec3::new(x, 0.0, z_near),
            crate::three_d::Vec3::new(x, 0.0, z_far),
            0.3,
        );
        x += step;
    }
}

/// A ring of 3D boxes orbiting a centre — the engine's machine room.
/// Cheap by design: eight boxes, forty-eight quads, real perspective.
/// The engine's parts, orbiting its heart — and then *joining* it.
///
/// `converge` in `0..1` draws the orbits in: at 0 the crates are eight
/// separate bodies at their full radii, at 1 they have fallen to a third
/// of the way out and dimmed, because by then the scene has said the
/// thing they illustrate — *36 crates, one core* — and eight boxes still
/// circling a claim they have already made are decoration. They were
/// decoration in the v2 cut, which is the note this answers.
pub fn cube_orbit(
    book: &mut Sketchbook,
    canvas: Size,
    center: Offset,
    t: f32,
    a: f32,
    fog_color: Color,
    converge: f32,
) {
    let converge = clamp01(converge);
    let shrink = 1.0 - 0.62 * ease_in_out(converge);
    let a = a * (1.0 - 0.72 * converge);
    use crate::three_d::{box_mesh, draw_mesh, Camera, MeshStyle, Vec3};
    let a = a.clamp(0.0, 1.0);
    if a <= 0.01 {
        return;
    }
    let cam = Camera {
        eye: Vec3::new(0.0, 130.0, -620.0),
        target: Vec3::new(0.0, 0.0, 0.0),
        fov: 0.78,
    };
    let style = MeshStyle {
        light_dir: Vec3::new(-0.4, 0.75, 0.5).norm(),
        fog_color,
        fog_start: 420.0,
        fog_end: 1650.0,
        edge_alpha: 0.22,
        specular: 0.35,
        ..MeshStyle::default()
    };
    let specs: [(f32, f32, f32, f32, f32, Color); 8] = [
        (300.0, 40.0, 0.30, 46.0, 0.0, BRAND_NEAR),
        (300.0, 40.0, 0.30, 34.0, 1.9, BRAND_FAR),
        (430.0, -30.0, -0.22, 40.0, 2.6, BRAND_FAR),
        (430.0, -30.0, -0.22, 30.0, 4.4, BRAND_NEAR),
        (180.0, 90.0, 0.45, 26.0, 3.7, BRAND_FAR),
        (520.0, 10.0, -0.35, 34.0, 0.9, BRAND_NEAR),
        (520.0, 10.0, -0.35, 26.0, 3.1, BRAND_FAR),
        (250.0, -80.0, 0.15, 30.0, 5.2, BRAND_NEAR),
    ];
    for (ring, y, tilt, size, phase, color) in specs {
        let ring = ring * shrink;
        let size = size * (1.0 - 0.28 * converge);
        // The orbit also slows as it closes, so the convergence reads as
        // settling rather than as a spin-down.
        let ang = t * (0.28 - 0.14 * converge) + phase;
        let (s, c) = ang.sin_cos();
        let x = ring * c;
        let z = ring * s;
        // The tilt tips the orbit plane; the y bob is the orbit's wave.
        let yy = y + (ang * 1.7).sin() * 26.0;
        let pos = Vec3::new(x, yy, z).rot_x(tilt);
        let half = size * 0.5;
        let mesh = box_mesh(pos, Vec3::new(half, half, half), color);
        draw_mesh_a(book, &mesh, &cam, canvas, &style, a, center);
    }
}

/// [`crate::three_d::draw_mesh`] with a screen offset and a global
/// alpha — the offset puts the world where the 2D composition wants
/// it, the alpha fades the whole orbit as one.
fn draw_mesh_a(
    book: &mut Sketchbook,
    mesh: &crate::three_d::Mesh,
    cam: &crate::three_d::Camera,
    canvas: Size,
    style: &crate::three_d::MeshStyle,
    a: f32,
    center: Offset,
) {
    if a <= 0.01 {
        return;
    }
    // The camera projects into canvas space about the canvas centre; the
    // composition's centre is wherever the 2D layout wants it — so the
    // whole world rides the *difference*, not the centre itself.
    book.transformed(
        Transform::translate(Offset::new(center.dx - canvas.width * 0.5, center.dy - canvas.height * 0.5)),
        |b| {
            book_layer_a(b, a, |bb| crate::three_d::draw_mesh(bb, mesh, cam, canvas, style));
        },
    );
}

/// A helper that wraps a draw in a zero-blur alpha layer when needed.
fn book_layer_a(book: &mut Sketchbook, a: f32, draw: impl FnOnce(&mut Sketchbook)) {
    if a >= 0.999 {
        draw(book);
    } else {
        book.layer(a, 0.0, None, draw);
    }
}

/// A projected 3D ring — two ellipse strokes in a tilted plane, with
/// orbit riders. The end card's halo, drawn in real perspective.
pub fn ring3(
    book: &mut Sketchbook,
    canvas: Size,
    center: Offset,
    radius: f32,
    t: f32,
    a: f32,
) {
    use crate::three_d::{Camera, Vec3};
    let a = a.clamp(0.0, 1.0);
    if a <= 0.01 {
        return;
    }
    let cam = Camera {
        eye: Vec3::new(0.0, 210.0, -430.0),
        target: Vec3::new(0.0, 0.0, 0.0),
        fov: 0.82,
    };
    let tilt = -0.42 + 0.03 * (t * 0.5).sin();
    let spin = t * 0.22;
    // The ring in its own plane, sampled; each point tilted and spun.
    let n = 72;
    let mut near_pts: Vec<Offset> = Vec::new();
    let mut far_pts: Vec<Offset> = Vec::new();
    for i in 0..=n {
        let ang = i as f32 / n as f32 * std::f32::consts::TAU;
        let p = Vec3::new(radius * ang.cos(), 0.0, radius * ang.sin())
            .rot_x(tilt)
            .rot_y(spin * 0.3);
        if let Some((pt, depth, _)) = cam.project(p, canvas) {
            let moved = Offset::new(
                pt.dx + center.dx - canvas.width * 0.5,
                pt.dy + center.dy - canvas.height * 0.5,
            );
            if depth > 300.0 {
                far_pts.push(moved);
            } else {
                near_pts.push(moved);
            }
        }
    }
    if far_pts.len() > 2 {
        book.stroke(path_through(&far_pts), pf::alpha(BRAND_FAR, 0.20 * a), 1.4);
    }
    if near_pts.len() > 2 {
        book.stroke(path_through(&near_pts), pf::alpha(BRAND_NEAR, 0.45 * a), 2.0);
    }
    // Three riders orbit the ring in 3D, projected like everything else.
    for k in 0..3 {
        let ang = spin * 1.6 + k as f32 * std::f32::consts::TAU / 3.0;
        let p = Vec3::new(radius * ang.cos(), 0.0, radius * ang.sin())
            .rot_x(tilt)
            .rot_y(spin * 0.3);
        if let Some((pt, _, _)) = cam.project(p, canvas) {
            let moved = Offset::new(
                pt.dx + center.dx - canvas.width * 0.5,
                pt.dy + center.dy - canvas.height * 0.5,
            );
            book.layer(a, 6.0, None, |b| {
                b.circle(moved, 4.5, pf::alpha(BRAND_FAR, 0.8));
            });
        }
    }
}

// ── The studio kit ──────────────────────────────────────────────────────────

/// The two-tone wordmark — **`vieww` in ink, `studio` in the brand's
/// purple**. One shaped paragraph, two spans, so the seam between the
/// colours is a glyph boundary, never a layout guess.
pub fn wordmark(size: f32, a: f32) -> vieww_widget::WidgetNode {
    RichText::new(vec![
        Span::new("vieww").color(pf::alpha(INK, 0.97 * a)),
        Span::new("studio").color(pf::alpha(BRAND_NEAR, a)),
    ])
    .style(pf::geist(size).bold().letter_spacing(6.0))
    .align(TextAlign::Center)
    .into()
}

/// Corner brackets that draw themselves — four L-strokes, each grown
/// to `frac`, with a soft accent halo at the tips while still growing.
pub fn corner_brackets(book: &mut Sketchbook, r: Rect, frac: f32, color: Color, a: f32, len: f32) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.01 || frac <= 0.01 {
        return;
    }
    let (l, t, rt, b) = (r.left, r.top, r.right, r.bottom);
    let corners: [[Offset; 3]; 4] = [
        [Offset::new(l, t + len), Offset::new(l, t), Offset::new(l + len, t)],
        [Offset::new(rt - len, t), Offset::new(rt, t), Offset::new(rt, t + len)],
        [Offset::new(rt, b - len), Offset::new(rt, b), Offset::new(rt - len, b)],
        [Offset::new(l + len, b), Offset::new(l, b), Offset::new(l, b - len)],
    ];
    for (i, c) in corners.iter().enumerate() {
        // Each corner draws right-to-top; stagger them a hair.
        let f = clamp01(frac * 1.15 - i as f32 * 0.04);
        grow_stroke(book, c, f, color, 2.2, a);
    }
}

/// The token flood — a brand-coloured wash sweeping the frame once,
/// left to right, the accent asserting itself over everything.
pub fn token_flood(book: &mut Sketchbook, w: f32, h: f32, p: f32, color: Color) {
    let p = clamp01(p);
    if p <= 0.001 || p >= 0.999 {
        return;
    }
    let band = w * 0.55;
    let x = -band + (w + band) * p;
    let g = Gradient::linear(Offset::new(0.0, 0.0), Offset::new(1.0, 0.0)).with_stops(&[
        (0.0, pf::alpha(color, 0.0)),
        (0.5, pf::alpha(color, 0.16)),
        (1.0, pf::alpha(color, 0.0)),
    ]);
    // Resolve the unit-square gradient over the band's own rect.
    book.transformed(
        Transform::translate(Offset::new(x, 0.0)),
        |b| {
            b.rect(Rect::new(0.0, 0.0, band, h), g);
        },
    );
}

/// The light sweep — one diagonal bar of light crossing the studio
/// once, as the wordmark lands. The sheen that says *this is real*.
pub fn light_sweep(book: &mut Sketchbook, w: f32, h: f32, p: f32, a: f32) {
    let p = clamp01(p);
    if p <= 0.001 || p >= 0.999 || a <= 0.01 {
        return;
    }
    let span = w + h;
    let x = -h * 0.4 + span * p;
    book.transformed(
        Transform::translate(Offset::new(x, 0.0)).then(Transform::rotate_around(
            Offset::new(0.0, h * 0.5),
            -0.16,
        )),
        |b| {
            let band_w = 190.0;
            let g = Gradient::horizontal().with_stops(&[
                (0.0, pf::alpha(Color::WHITE, 0.0)),
                (0.5, pf::alpha(Color::WHITE, 0.055 * a)),
                (1.0, pf::alpha(Color::WHITE, 0.0)),
            ]);
            b.rect(Rect::new(0.0, -h * 0.4, band_w, h * 1.8), g);
        },
    );
}

/// An anchor ring with a label stem — the callout's foot. A ring that
/// blooms on `p`, a hairline stem rising to the label plate.
pub fn anchor_foot(book: &mut Sketchbook, at: Offset, p: f32, color: Color, a: f32) {
    let p = clamp01(p);
    if p <= 0.01 || a <= 0.01 {
        return;
    }
    let bloom = spring_out(p, 9.0, 0.6);
    book.ring(at, 10.0 * bloom, 1.6, pf::alpha(color, 0.75 * a));
    book.circle(at, 3.2, pf::alpha(color, 0.95 * a));
    if p < 1.0 {
        book.ring(at, 10.0 + 16.0 * (1.0 - p), 1.0, pf::alpha(color, 0.3 * (1.0 - p) * a));
    }
}

/// The studio act's room — a ground that can overscan. Scenes whose
/// camera dips below zoom 1.0 paint margin so the pullback never
/// reveals the world's edge.
pub fn room(book: &mut Sketchbook, w: f32, h: f32, margin: f32, top: Color, mid: Color, bot: Color) {
    book.rect(
        Rect::new(-margin, -margin, w + margin, h + margin),
        Gradient::vertical().with_dither().with_stops(&[
            (0.0, top),
            (0.55, mid),
            (1.0, bot),
        ]),
    );
}

/// Stars that can overscan with the room — same distribution as
/// [`pf::stars`], drawn over the margined rect.
pub fn stars_wide(book: &mut Sketchbook, w: f32, h: f32, margin: f32, seed: u64, n: usize, t: f32, base: f32) {
    let mut rng = Rng::new(seed);
    for _ in 0..n {
        let x = -margin + rng.f01() * (w + 2.0 * margin);
        let y = -margin + rng.f01() * (h + 2.0 * margin);
        let tw = 0.5 + 0.5 * (t * (0.4 + rng.f01() * 1.4) + rng.f01() * 7.0).sin();
        let r = 0.6 + rng.f01() * 1.3;
        book.circle(Offset::new(x, y), r, pf::alpha(Color::WHITE, base * tw));
    }
}

// ── The stage kit (v3) ──────────────────────────────────────────────────────
//
// The v2 cut annotated the live app by drawing *on* it: cards, chips and
// slabs landed straight over the editor's code and the preview's white.
// Two things competed for every pixel and neither won. The v3 rule is a
// stage: an annotation group first clears a plate for itself — a soft,
// dark, rounded panel with a lit seam — and only then draws. The app is
// never graded (the honesty rule holds); it is *occluded*, deliberately,
// by the film's own layer, the way a lower third occludes a news desk.

/// A stage plate — the dark panel an annotation group clears for itself
/// over the live app. Grows from its own centre on `p`, carries a top
/// accent seam, a hairline edge and a real drop shadow.
///
/// Draw this *first*, then draw the group's content at the same rect.
pub fn stage_plate(book: &mut Sketchbook, r: Rect, p: f32, color: Color, a: f32) {
    let p = clamp01(p);
    if p <= 0.01 || a <= 0.01 {
        return;
    }
    let grow = ease_out_expo(p);
    let (cx, cy) = (r.left + r.width() * 0.5, r.top + r.height() * 0.5);
    let hw = r.width() * 0.5 * (0.88 + 0.12 * grow);
    let hh = r.height() * 0.5 * grow;
    if hh < 1.0 || hw < 1.0 {
        return;
    }
    let body = Rect::new(cx - hw, cy - hh, cx + hw, cy + hh);
    let radius = 18.0;
    book.shadow(
        Rect::new(body.left, body.top + 10.0, body.right, body.bottom + 10.0),
        radius,
        vieww_foundation::Shadow::new(pf::alpha(Color::BLACK, 0.62 * a), Offset::new(0.0, 14.0), 42.0),
    );
    // The plate itself — near-opaque so the app under it stops shouting.
    book.rrect(
        body,
        radius,
        pf::alpha(Color::rgb(0x12, 0x10, 0x14), (0.93 * a).min(1.0)),
    );
    book.rrect(
        body,
        radius,
        Gradient::vertical().with_dither().with_stops(&[
            (0.0, pf::alpha(color, 0.055 * a)),
            (1.0, pf::alpha(Color::BLACK, 0.0)),
        ]),
    );
    book.stroke_rrect(body, radius, pf::alpha(Color::WHITE, 0.07 * a), 1.0);
    // The seam — a lit rule along the plate's top edge, drawn to `grow`.
    let seam_w = body.width() * 0.62 * grow;
    if seam_w > 2.0 {
        book.rect(
            Rect::new(body.left + 22.0, body.top, body.left + 22.0 + seam_w, body.top + 2.0),
            Gradient::horizontal().with_stops(&[
                (0.0, pf::alpha(color, 0.0)),
                (0.22, pf::alpha(color, 0.85 * a)),
                (1.0, pf::alpha(color, 0.0)),
            ]),
        );
    }
}

/// A specular sheen travelling once across a plate — the premium tell.
/// `p` is the sweep's own progress; outside `0..1` it draws nothing.
pub fn plate_sheen(book: &mut Sketchbook, r: Rect, p: f32, a: f32) {
    let p = clamp01(p);
    if p <= 0.001 || p >= 0.999 || a <= 0.01 {
        return;
    }
    let band = r.width() * 0.34;
    let x = r.left - band + (r.width() + band) * p;
    // Fade in and out so the sheen never pops at the plate's edges.
    let env = (p * std::f32::consts::PI).sin();
    book.transformed(Transform::translate(Offset::new(x, r.top)), |b| {
        b.rect(
            Rect::new(0.0, 0.0, band, r.height()),
            Gradient::horizontal().with_stops(&[
                (0.0, pf::alpha(Color::WHITE, 0.0)),
                (0.5, pf::alpha(Color::WHITE, 0.05 * a * env)),
                (1.0, pf::alpha(Color::WHITE, 0.0)),
            ]),
        );
    });
}

/// A measured bar against a budget — the ledger's gauge. `value` and
/// `budget` share units; the bar fills to `value/scale` on `p`, and the
/// budget's line is drawn where it actually falls, labelled by the
/// caller. Over budget is drawn in `over`, under in `under`.
pub fn budget_bar(
    book: &mut Sketchbook,
    r: Rect,
    value: f32,
    budget: f32,
    scale: f32,
    p: f32,
    under: Color,
    over: Color,
    a: f32,
) {
    let p = clamp01(p);
    if a <= 0.01 {
        return;
    }
    let radius = r.height() * 0.5;
    book.rrect(r, radius, pf::alpha(Color::WHITE, 0.05 * a));
    let frac = (value / scale).clamp(0.0, 1.0) * ease_out_expo(p);
    let color = if value > budget { over } else { under };
    let w = r.width() * frac;
    if w > 2.0 {
        let fill = Rect::new(r.left, r.top, r.left + w, r.bottom);
        book.rrect(
            fill,
            radius,
            Gradient::horizontal().with_dither().with_stops(&[
                (0.0, pf::alpha(color, 0.55 * a)),
                (1.0, pf::alpha(color, 0.95 * a)),
            ]),
        );
        // The head — a soft light where the bar stops growing.
        book.layer(a, 10.0, None, |b| {
            b.circle(
                Offset::new(r.left + w, r.top + r.height() * 0.5),
                r.height() * 0.85,
                pf::alpha(color, 0.35),
            );
        });
    }
    // The budget line — where the frame's 60 fps allowance actually is.
    let bx = r.left + r.width() * (budget / scale).clamp(0.0, 1.0);
    book.rect(
        Rect::new(bx - 0.75, r.top - 9.0, bx + 0.75, r.bottom + 9.0),
        pf::alpha(Color::WHITE, 0.32 * a),
    );
}

/// A hairline tick ladder under a gauge — the scale the bars are read
/// against, so a bar means a number and not a feeling.
pub fn gauge_ticks(book: &mut Sketchbook, x0: f32, x1: f32, y: f32, n: usize, a: f32) {
    if a <= 0.01 {
        return;
    }
    for i in 0..=n {
        let f = i as f32 / n as f32;
        let x = x0 + (x1 - x0) * f;
        let major = i % 2 == 0;
        book.rect(
            Rect::new(x - 0.5, y, x + 0.5, y + if major { 7.0 } else { 4.0 }),
            pf::alpha(Color::WHITE, (if major { 0.22 } else { 0.12 }) * a),
        );
    }
}

/// The canvas size, re-exported for the painters that need it.
pub fn canvas() -> Size {
    CANVAS
}

/// Silence the lints for symbols reserved by the scene modules.
#[allow(unused)]
fn _reserved() {
    let _ = (canvas, BRAND_FAR, ease_out_cubic);
}
