//! The look — the film's finishing pass, run on every frame after the
//! rasterizer and the dissolve, by `vieww-effects`' optics in linear light.
//!
//! What a camera adds to a picture that a canvas does not: highlights that
//! scatter into the lens (bloom), the faint red bleed of light through film
//! (halation), the falloff toward the corners (cos⁴ vignetting), grain, and
//! a grade. Each is gentle on its own — a viewer should feel the frame got
//! richer, not see an effect.
//!
//! **And it fixes the banding the old light rule existed for.** The film
//! used to forbid radial glows because a soft ramp over near-black
//! quantised into steps in an 8-bit H.264 encode. The finish writes back
//! through an ordered dither and lays a luminance-weighted grain over the
//! whole frame, so the encoder sees texture rather than flat steps — the
//! glows the rule forbade are now the bloom's job, done physically.
//!
//! **Light events** are the few places the story already turns on light:
//! the spark of the cold open, the question's ring, the movement
//! boundaries (a luminous swell through the dissolve), the flight home and
//! the end card's reveal, where the mark throws a lens flare and light
//! rays across the wordmark.

use vieww_effects::cpu::optics::{self, Finish, Linear};

/// Is the finishing pass on? (`SF_FINISH=0` turns it off for A/B.)
pub(crate) fn enabled() -> bool {
    std::env::var("SF_FINISH").map_or(true, |v| v != "0")
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The base look, every frame.
fn base() -> Finish {
    Finish {
        bloom_threshold: 0.42,
        bloom_knee: 0.25,
        bloom_intensity: 0.32,
        bloom_radius: 0.85,
        bloom_tint: [0.95, 0.9, 1.0],
        bloom_chroma_key: 0.92,
        halation: 0.0,
        streaks: 0.0,
        vignette: 0.16,
        grain: 0.03,
        ..Finish::default()
    }
}

/// The finish for scene `id` at `sec` into it (`seconds` long), with the
/// entrance dissolve's eased weight `cut` (`None` once whole) and whether
/// this scene opens a movement. `w`, `h` are the raster size.
pub(crate) fn finish_for(
    id: &str,
    sec: f32,
    seconds: f32,
    cut: Option<f32>,
    opens_movement: bool,
    w: f32,
    h: f32,
) -> Finish {
    let mut f = base();
    // Movement boundaries: light swells through the dissolve and settles.
    if let (Some(e), true) = (cut, opens_movement) {
        let swell = (e * std::f32::consts::PI).sin();
        f.bloom_intensity += 0.55 * swell;
        f.bloom_threshold -= 0.2 * swell;
        f.bloom_chroma_key -= 0.5 * swell;
        f.exposure = 1.0 + 0.12 * swell;
    }
    match id {
        // The spark: the one dot is a light source.
        "Z00" => {
            f.bloom_intensity += 0.3 * smooth(0.6, 2.4, sec);
        }
        "Z05" => {
            f.bloom_intensity += 0.25 * smooth(7.0, 10.0, sec);
        }
        "Z10D" => {
            // Real 3D gets a real lens: anamorphic streaks off the
            // speculars, a little more scatter.
            f.bloom_intensity += 0.12;
            f.streaks = 0.05;
        }
        // Real app screenshots and the semantics overlay are dense with
        // saturated colour: keep their scatter low so they read sharp.
        "Z19B" | "Z13B" | "Z10C" => {
            f.bloom_intensity *= 0.35;
        }
        "Z20B" => {
            f.bloom_intensity += 0.25;
        }
        "Z21" => {
            // The reveal: the mark flares as the wordmark lands, and its
            // light rays sweep the title.
            let mark = (w * 0.5, h * 0.355);
            let flare = smooth(0.4, 1.6, sec) * (1.0 - smooth(4.0, 9.0, sec) * 0.65);
            if flare > 0.01 {
                // The flare's source drifts slightly — light, not a sticker.
                let drift = (sec * 0.35).sin() * w * 0.012;
                f.flare = Some(((mark.0 + drift, mark.1), 0.09 * flare));
                f.rays = Some((mark, 0.18 * flare));
            }
            f.bloom_intensity += 0.3 * smooth(0.5, 2.0, sec);
            f.streaks = 0.06 * flare;
        }
        "Z22" => {
            let mark = (w * 0.5, h * 0.355);
            let fade = 1.0 - smooth(0.0, seconds, sec);
            f.flare = Some((mark, 0.055 * fade));
            f.bloom_intensity += 0.15 * fade;
        }
        _ => {}
    }
    f
}

/// Apply `finish` to an RGBA8 frame in place.
pub(crate) fn apply(finish: &Finish, px: &mut [u8], w: usize, h: usize, frame_index: u32) {
    finish.apply_rgba8(px, w, h, frame_index.wrapping_mul(2_654_435_761));
}

/// For the census: the per-frame finishing cost on this machine, measured
/// on a real frame.
pub(crate) fn time_once(px: &[u8], w: usize, h: usize) -> f32 {
    let mut c = px.to_vec();
    let t = std::time::Instant::now();
    apply(&base(), &mut c, w, h, 1);
    let _ = Linear::new(1, 1);
    let _ = optics::srgb_to_linear(0);
    t.elapsed().as_secs_f32() * 1000.0
}
