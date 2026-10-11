//! After Effects text animators — the Text layer's Animate menu (§2.15 L2
//! `TextLayer`): **animator properties** (position, scale, rotation,
//! opacity, tracking, fill colour, blur) applied to text *units* —
//! characters, characters without spaces, words or lines — through
//! **range selectors** (start, end, offset, the six shapes, ease high/low,
//! randomised order) and **wiggly selectors** (per-unit noise in time).
//!
//! The whole thing is a pure function: `(text, animator, time) →` one
//! [`UnitStyle`] per character. Whatever renders the text (a widget, a
//! sketch, a comp generator) applies the styles; SplitText in `vieww-widget`
//! is the simplest such renderer.

use vieww_foundation::Color;

/// What counts as one unit for the selector's percentages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BasedOn {
    #[default]
    Characters,
    CharactersExcludingSpaces,
    Words,
    Lines,
}

/// The selector's falloff shape across its range (AE's "Shape").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shape {
    #[default]
    Square,
    RampUp,
    RampDown,
    Triangle,
    Round,
    Smooth,
}

/// A range selector. `start`, `end` and `offset` are percentages (0..100)
/// of the unit count; every unit's coverage is how much of it lies inside
/// the range, shaped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RangeSelector {
    pub start: f32,
    pub end: f32,
    pub offset: f32,
    pub based_on: BasedOn,
    pub shape: Shape,
    /// −100..100: concentrates the selection toward the range's high end.
    pub ease_high: f32,
    /// −100..100: … toward its low end.
    pub ease_low: f32,
    /// Shuffle unit order with this seed.
    pub randomize: Option<u32>,
    /// 0..100: the selector's overall weight.
    pub amount: f32,
}

impl Default for RangeSelector {
    fn default() -> Self {
        Self {
            start: 0.0,
            end: 100.0,
            offset: 0.0,
            based_on: BasedOn::Characters,
            shape: Shape::Square,
            ease_high: 0.0,
            ease_low: 0.0,
            randomize: None,
            amount: 100.0,
        }
    }
}

/// A wiggly selector: each unit's weight wanders in time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WigglySelector {
    /// Wiggles per second.
    pub frequency: f32,
    /// 0..100.
    pub amount: f32,
    /// 0..100 — how alike neighbouring units move.
    pub correlation: f32,
    pub seed: u32,
}

/// The animator's property offsets, applied at full selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimatorProps {
    pub position: (f32, f32),
    pub scale: f32,
    /// Degrees.
    pub rotation: f32,
    /// Opacity at full selection (AE's animator opacity: 0 = invisible).
    pub opacity: f32,
    /// Extra advance after each unit, pixels.
    pub tracking: f32,
    pub fill: Option<Color>,
    pub blur: f32,
}

impl Default for AnimatorProps {
    fn default() -> Self {
        Self {
            position: (0.0, 0.0),
            scale: 1.0,
            rotation: 0.0,
            opacity: 1.0,
            tracking: 0.0,
            fill: None,
            blur: 0.0,
        }
    }
}

/// One animator: properties and the selectors that weight them.
#[derive(Debug, Clone, PartialEq)]
pub struct TextAnimator {
    pub props: AnimatorProps,
    pub range: RangeSelector,
    pub wiggly: Option<WigglySelector>,
}

/// One character's resolved style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitStyle {
    pub offset: (f32, f32),
    pub scale: f32,
    pub rotation: f32,
    pub opacity: f32,
    /// Accumulated tracking before this character.
    pub advance: f32,
    pub fill: Option<(Color, f32)>,
    pub blur: f32,
    /// The animator's selection weight on this character (0..1, last
    /// animator wins for reporting).
    pub weight: f32,
}

impl Default for UnitStyle {
    fn default() -> Self {
        Self {
            offset: (0.0, 0.0),
            scale: 1.0,
            rotation: 0.0,
            opacity: 1.0,
            advance: 0.0,
            fill: None,
            blur: 0.0,
            weight: 0.0,
        }
    }
}

/// The unit index of each character (`None` for characters a unit kind
/// skips), and the unit count.
#[must_use]
pub fn units(text: &str, based_on: BasedOn) -> (Vec<Option<usize>>, usize) {
    let mut out = Vec::new();
    let mut n = 0usize;
    match based_on {
        BasedOn::Characters => {
            for _ in text.chars() {
                out.push(Some(n));
                n += 1;
            }
        }
        BasedOn::CharactersExcludingSpaces => {
            for c in text.chars() {
                if c.is_whitespace() {
                    out.push(None);
                } else {
                    out.push(Some(n));
                    n += 1;
                }
            }
        }
        BasedOn::Words => {
            let mut in_word = false;
            for c in text.chars() {
                if c.is_whitespace() {
                    if in_word {
                        n += 1;
                    }
                    in_word = false;
                    out.push(None);
                } else {
                    in_word = true;
                    out.push(Some(n));
                }
            }
            if in_word {
                n += 1;
            }
        }
        BasedOn::Lines => {
            for c in text.chars() {
                if c == '\n' {
                    out.push(None);
                    n += 1;
                } else {
                    out.push(Some(n));
                }
            }
            n += 1;
        }
    }
    (out, n)
}

fn shuffle(n: usize, seed: u32) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    let mut s = u64::from(seed).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    for i in (1..n).rev() {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        #[allow(clippy::cast_possible_truncation)]
        let j = (s % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
    v
}

fn ease(t: f32, high: f32, low: f32) -> f32 {
    // Bias the ramp: positive ease pushes values toward its end.
    let t = t.clamp(0.0, 1.0);
    let h = (high / 100.0).clamp(-1.0, 1.0);
    let l = (low / 100.0).clamp(-1.0, 1.0);
    let up = if h >= 0.0 { t.powf(1.0 + 2.0 * h) } else { 1.0 - (1.0 - t).powf(1.0 - 2.0 * h) };
    if l >= 0.0 {
        1.0 - (1.0 - up).powf(1.0 + 2.0 * l)
    } else {
        up.powf(1.0 - 2.0 * l)
    }
}

impl RangeSelector {
    /// The selection weight (0..1) of unit `i` of `n`.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn weight(&self, i: usize, n: usize) -> f32 {
        if n == 0 {
            return 0.0;
        }
        let i = match self.randomize {
            Some(seed) => shuffle(n, seed)[i],
            None => i,
        };
        let (mut s, mut e) = (self.start + self.offset, self.end + self.offset);
        if s > e {
            std::mem::swap(&mut s, &mut e);
        }
        let (s, e) = (s / 100.0 * n as f32, e / 100.0 * n as f32);
        let (a, b) = (i as f32, i as f32 + 1.0);
        let w = match self.shape {
            Shape::Square => (b.min(e) - a.max(s)).clamp(0.0, 1.0),
            _ => {
                // Shaped selections evaluate at the unit centre.
                let c = (a + b) * 0.5;
                let span = (e - s).max(1e-6);
                let t = (c - s) / span;
                if !(0.0..=1.0).contains(&t) {
                    match self.shape {
                        Shape::RampUp if t > 1.0 => 1.0,
                        Shape::RampDown if t < 0.0 => 1.0,
                        _ => 0.0,
                    }
                } else {
                    match self.shape {
                        Shape::RampUp => t,
                        Shape::RampDown => 1.0 - t,
                        Shape::Triangle => 1.0 - (2.0 * t - 1.0).abs(),
                        Shape::Round => (1.0 - (2.0 * t - 1.0).powi(2)).max(0.0).sqrt(),
                        Shape::Smooth => {
                            let k = 1.0 - (2.0 * t - 1.0).abs();
                            k * k * (3.0 - 2.0 * k)
                        }
                        Shape::Square => 1.0,
                    }
                }
            }
        };
        ease(w, self.ease_high, self.ease_low) * (self.amount / 100.0).clamp(-1.0, 1.0)
    }
}

fn hash(i: u32, k: u32, seed: u32) -> f32 {
    let mut h = i.wrapping_mul(0x27d4_eb2d) ^ k.wrapping_mul(0x1656_67b1) ^ seed.wrapping_mul(0x9E37_79B9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    #[allow(clippy::cast_precision_loss)]
    let v = (h & 0xFFFF) as f32 / 65_535.0;
    v * 2.0 - 1.0
}

impl WigglySelector {
    /// The wiggle factor (−1..1, scaled by amount) of unit `i` at `t`.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
    pub fn factor(&self, i: usize, t: f32) -> f32 {
        let x = t * self.frequency.max(0.0);
        let k = x.floor();
        let f = x - k;
        let f = f * f * (3.0 - 2.0 * f);
        let corr = (self.correlation / 100.0).clamp(0.0, 1.0);
        let unit = |kk: u32| {
            let own = hash(i as u32, kk, self.seed);
            let shared = hash(0, kk, self.seed ^ 0xABCD);
            own * (1.0 - corr) + shared * corr
        };
        let a = unit(k as u32);
        let b = unit(k as u32 + 1);
        (a + (b - a) * f) * (self.amount / 100.0)
    }
}

/// Resolve every character's style under `animators` at time `t`
/// (animators stack: offsets add, scales and opacities multiply).
#[must_use]
pub fn resolve(text: &str, animators: &[TextAnimator], t: f32) -> Vec<UnitStyle> {
    let count = text.chars().count();
    let mut out = vec![UnitStyle::default(); count];
    for anim in animators {
        let (unit_of, n) = units(text, anim.range.based_on);
        let mut tracking = 0.0;
        for (ci, u) in unit_of.iter().enumerate() {
            let style = &mut out[ci];
            style.advance += tracking;
            let Some(u) = *u else { continue };
            let mut w = anim.range.weight(u, n);
            if let Some(wig) = &anim.wiggly {
                w *= 1.0 + wig.factor(u, t);
            }
            let p = &anim.props;
            style.offset.0 += p.position.0 * w;
            style.offset.1 += p.position.1 * w;
            style.scale *= 1.0 + (p.scale - 1.0) * w;
            style.rotation += p.rotation * w;
            style.opacity *= 1.0 + (p.opacity - 1.0) * w.clamp(0.0, 1.0);
            style.blur += p.blur * w.max(0.0);
            if let Some(c) = p.fill {
                style.fill = Some((c, w.clamp(0.0, 1.0)));
            }
            style.weight = w;
            tracking += p.tracking * w;
        }
    }
    out
}

/// The classic typewriter/cascade-in: units appear one by one as `progress`
/// runs 0→1, each rising `rise` pixels and fading over `softness` units.
#[must_use]
pub fn cascade_in(rise: f32, softness: f32, based_on: BasedOn, progress: f32) -> TextAnimator {
    // The selector covers what has NOT appeared yet: start sweeps 0 → 100.
    let soft = softness.max(0.0);
    TextAnimator {
        props: AnimatorProps {
            position: (0.0, rise),
            opacity: 0.0,
            ..AnimatorProps::default()
        },
        range: RangeSelector {
            start: progress.clamp(0.0, 1.0) * (100.0 + soft) - soft,
            end: 100.0 + soft,
            shape: if soft > 0.0 { Shape::RampUp } else { Shape::Square },
            based_on,
            ..RangeSelector::default()
        },
        wiggly: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_count_characters_words_and_lines() {
        assert_eq!(units("ab c", BasedOn::Characters).1, 4);
        assert_eq!(units("ab c", BasedOn::CharactersExcludingSpaces).1, 3);
        let (w, n) = units("one two  three", BasedOn::Words);
        assert_eq!(n, 3);
        assert_eq!(w[4], Some(1));
        assert_eq!(w[3], None);
        assert_eq!(units("a\nb\nc", BasedOn::Lines).1, 3);
    }

    #[test]
    fn a_square_range_covers_fractional_units_partially() {
        let r = RangeSelector {
            start: 0.0,
            end: 25.0,
            ..RangeSelector::default()
        };
        // 10 units: the range ends at 2.5 units.
        assert!((r.weight(0, 10) - 1.0).abs() < 1e-6);
        assert!((r.weight(2, 10) - 0.5).abs() < 1e-6);
        assert!(r.weight(3, 10).abs() < 1e-6);
        let shifted = RangeSelector {
            offset: 50.0,
            ..r
        };
        assert!(shifted.weight(0, 10).abs() < 1e-6);
        assert!((shifted.weight(5, 10) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn shapes_peak_where_they_should() {
        let tri = RangeSelector {
            shape: Shape::Triangle,
            ..RangeSelector::default()
        };
        assert!(tri.weight(5, 11) > 0.95);
        assert!(tri.weight(0, 11) < 0.1);
        let up = RangeSelector {
            shape: Shape::RampUp,
            ..RangeSelector::default()
        };
        assert!(up.weight(9, 10) > up.weight(1, 10));
        let round = RangeSelector {
            shape: Shape::Round,
            ..RangeSelector::default()
        };
        assert!(round.weight(5, 11) > 0.99);
    }

    #[test]
    fn randomize_permutes_without_losing_units() {
        let r = RangeSelector {
            end: 30.0,
            randomize: Some(42),
            ..RangeSelector::default()
        };
        let total: f32 = (0..10).map(|i| r.weight(i, 10)).sum();
        assert!((total - 3.0).abs() < 1e-5);
        let picked: Vec<usize> = (0..10).filter(|i| r.weight(*i, 10) > 0.5).collect();
        assert_ne!(picked, vec![0, 1, 2]);
    }

    #[test]
    fn a_cascade_reveals_characters_in_order() {
        let text = "vieww";
        let half = resolve(text, &[cascade_in(20.0, 0.0, BasedOn::Characters, 0.4)], 0.0);
        assert!(half[0].opacity > 0.99 && half[1].opacity > 0.99);
        assert!(half[3].opacity < 0.01 && (half[3].offset.1 - 20.0).abs() < 1e-4);
        let done = resolve(text, &[cascade_in(20.0, 2.0, BasedOn::Characters, 1.0)], 0.0);
        assert!(done.iter().all(|s| s.opacity > 0.99 && s.offset.1.abs() < 1e-3));
    }

    #[test]
    fn wiggly_is_deterministic_bounded_and_correlation_aligns_units() {
        let w = WigglySelector {
            frequency: 2.0,
            amount: 50.0,
            correlation: 0.0,
            seed: 7,
        };
        for i in 0..20 {
            let f = w.factor(i, 1.37);
            assert!(f.abs() <= 0.5 + 1e-6);
            assert_eq!(f, w.factor(i, 1.37));
        }
        let together = WigglySelector {
            correlation: 100.0,
            ..w
        };
        assert!((together.factor(1, 0.8) - together.factor(9, 0.8)).abs() < 1e-6);
    }

    #[test]
    fn tracking_accumulates_and_animators_stack() {
        let a = TextAnimator {
            props: AnimatorProps {
                tracking: 4.0,
                scale: 2.0,
                ..AnimatorProps::default()
            },
            range: RangeSelector::default(),
            wiggly: None,
        };
        let s = resolve("abc", &[a.clone(), a], 0.0);
        assert!((s[2].advance - 16.0).abs() < 1e-5);
        assert!((s[0].scale - 4.0).abs() < 1e-5);
    }
}
