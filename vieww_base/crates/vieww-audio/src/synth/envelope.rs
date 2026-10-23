//! Envelope generators for notes — ADSR, DAHDSR, and any number of stages.
//!
//! The crate root's [`Envelope`](crate::Envelope) is attack/release because a
//! UI sound is a pulse. A *note* is held: it rises, settles to a sustain
//! level, waits there for as long as the key is down, and releases from
//! wherever it is when the key comes up. That is ADSR, and ADSR is a special
//! case of the general thing every modern synth exposes — a list of stages,
//! each a target level reached over a time along a curve, with one stage
//! marked "hold here while gated" and optionally a range that loops while
//! gated (Vital's and Serum's looping envelopes, the Kawai K5's loop points).
//!
//! [`Stages`] is that general thing. [`Adsr`] and [`Dahdsr`] build one, and
//! [`EnvelopeGen`] runs one: a per-sample state machine with
//! [`gate_on`](EnvelopeGen::gate_on) / [`gate_off`](EnvelopeGen::gate_off),
//! velocity scaling of level and attack time, and release-from-current-level
//! so a key lifted mid-attack never steps.

/// The shape of a stage's ramp.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Curve {
    /// A straight line.
    #[default]
    Linear,
    /// `k > 0` bows the ramp late (slow start, fast finish — "exponential
    /// attack"), `k < 0` bows it early (fast start, slow finish — the
    /// natural decay of a struck string). `|k|` around 4–6 is analog-like.
    Exp(f32),
    /// Hold the start level, then jump at the stage's end.
    Step,
}

impl Curve {
    /// Map progress `x` in `0..=1` to ramp progress in `0..=1`.
    #[must_use]
    pub fn shape(self, x: f32) -> f32 {
        let x = x.clamp(0.0, 1.0);
        match self {
            Self::Linear => x,
            Self::Exp(k) if k.abs() < 1e-4 => x,
            Self::Exp(k) => ((k * x).exp() - 1.0) / (k.exp() - 1.0),
            Self::Step => {
                if x >= 1.0 {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

/// One stage: ramp to `target` over `seconds` along `curve`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stage {
    /// The level reached at the stage's end (before velocity scaling).
    pub target: f32,
    /// How long the ramp takes.
    pub seconds: f32,
    /// The ramp's shape.
    pub curve: Curve,
}

impl Stage {
    /// A linear stage.
    #[must_use]
    pub const fn linear(target: f32, seconds: f32) -> Self {
        Self {
            target,
            seconds,
            curve: Curve::Linear,
        }
    }

    /// A stage with a curve.
    #[must_use]
    pub const fn curved(target: f32, seconds: f32, curve: Curve) -> Self {
        Self {
            target,
            seconds,
            curve,
        }
    }
}

/// How note velocity (0..=1) reshapes the envelope.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Velocity {
    /// 0: velocity ignored for level. 1: level is proportional to velocity.
    pub to_level: f32,
    /// 0: attack time ignored. 1: a full-velocity note's first stage takes
    /// a tenth of its time — harder strikes speak faster.
    pub to_attack: f32,
}

/// A multi-stage envelope description.
#[derive(Debug, Clone, PartialEq)]
pub struct Stages {
    /// The stages, in order. The envelope starts at level 0.
    pub stages: Vec<Stage>,
    /// Hold at the end of this stage while the gate is on. Stages after it
    /// are the release, entered on [`EnvelopeGen::gate_off`].
    pub sustain: Option<usize>,
    /// While gated, after finishing stage `.1` go back to stage `.0`. If
    /// there is no sustain, the release starts after `.1`.
    pub loop_range: Option<(usize, usize)>,
    /// Velocity response.
    pub velocity: Velocity,
}

impl Stages {
    /// A one-shot envelope: run every stage once, ignore the gate.
    #[must_use]
    pub const fn one_shot(stages: Vec<Stage>) -> Self {
        Self {
            stages,
            sustain: None,
            loop_range: None,
            velocity: Velocity {
                to_level: 0.0,
                to_attack: 0.0,
            },
        }
    }

    /// Set the velocity response.
    #[must_use]
    pub const fn velocity(mut self, velocity: Velocity) -> Self {
        self.velocity = velocity;
        self
    }

    /// Set a loop range.
    #[must_use]
    pub const fn looping(mut self, from: usize, to: usize) -> Self {
        self.loop_range = Some((from, to));
        self
    }

    fn release_start(&self) -> Option<usize> {
        self.sustain
            .map(|s| s + 1)
            .or_else(|| self.loop_range.map(|(_, b)| b + 1))
    }
}

/// Attack, decay, sustain level, release.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Adsr {
    /// Seconds from 0 to peak.
    pub attack: f32,
    /// Seconds from peak to the sustain level.
    pub decay: f32,
    /// The held level, `0..=1`.
    pub sustain: f32,
    /// Seconds from wherever the note was to 0.
    pub release: f32,
}

impl Adsr {
    /// An ADSR.
    #[must_use]
    pub const fn new(attack: f32, decay: f32, sustain: f32, release: f32) -> Self {
        Self {
            attack,
            decay,
            sustain,
            release,
        }
    }

    /// Linear stages — what most hardware's "ADSR" shows on its panel.
    #[must_use]
    pub fn stages(self) -> Stages {
        self.with_curves(Curve::Linear, Curve::Linear, Curve::Linear)
    }

    /// The analog-modelled shape: a slightly late-bowed attack and
    /// exponential decay and release.
    #[must_use]
    pub fn analog(self) -> Stages {
        self.with_curves(Curve::Exp(-1.5), Curve::Exp(-5.0), Curve::Exp(-5.0))
    }

    /// Stages with explicit curves.
    #[must_use]
    pub fn with_curves(self, attack: Curve, decay: Curve, release: Curve) -> Stages {
        Stages {
            stages: vec![
                Stage::curved(1.0, self.attack, attack),
                Stage::curved(self.sustain, self.decay, decay),
                Stage::curved(0.0, self.release, release),
            ],
            sustain: Some(1),
            loop_range: None,
            velocity: Velocity::default(),
        }
    }
}

/// Delay, attack, hold, decay, sustain, release.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dahdsr {
    /// Silence before the attack.
    pub delay: f32,
    /// Seconds to peak.
    pub attack: f32,
    /// Seconds held at peak.
    pub hold: f32,
    /// Seconds to sustain.
    pub decay: f32,
    /// Held level.
    pub sustain: f32,
    /// Seconds to 0.
    pub release: f32,
}

impl Dahdsr {
    /// The stages.
    #[must_use]
    pub fn stages(self) -> Stages {
        Stages {
            stages: vec![
                Stage::linear(0.0, self.delay),
                Stage::linear(1.0, self.attack),
                Stage::linear(1.0, self.hold),
                Stage::curved(self.sustain, self.decay, Curve::Exp(-4.0)),
                Stage::curved(0.0, self.release, Curve::Exp(-4.0)),
            ],
            sustain: Some(3),
            loop_range: None,
            velocity: Velocity::default(),
        }
    }
}

/// Where a running envelope is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Not started, or finished.
    Idle,
    /// Ramping through stage `n`.
    Stage(usize),
    /// Holding at the sustain stage's level.
    Sustain,
}

/// A running envelope.
#[derive(Debug, Clone)]
pub struct EnvelopeGen {
    desc: Stages,
    rate: f32,
    phase: Phase,
    gated: bool,
    pos: usize,
    len: usize,
    from: f32,
    level: f32,
    level_scale: f32,
    attack_scale: f32,
    /// An exponential fade to zero from [`EnvelopeGen::kill`]: the
    /// per-sample multiplier while fading, `f32::MAX` when not.
    fade_k: f32,
}

impl EnvelopeGen {
    /// An idle envelope at `rate` Hz.
    #[must_use]
    pub fn new(desc: Stages, rate: f32) -> Self {
        Self {
            desc,
            rate,
            phase: Phase::Idle,
            gated: false,
            pos: 0,
            len: 0,
            from: 0.0,
            level: 0.0,
            level_scale: 1.0,
            attack_scale: 1.0,
            fade_k: f32::MAX,
        }
    }

    /// Make this an idle envelope of shape `adsr` — what
    /// [`new`](Self::new)`(adsr.stages(), rate)` builds, but written into
    /// this envelope's existing stage storage, so re-arming a voice for a
    /// new note on the audio thread allocates nothing.
    pub fn rearm_adsr(&mut self, adsr: Adsr) {
        let fresh = |a, b, c| {
            [
                Stage::curved(1.0, adsr.attack, a),
                Stage::curved(adsr.sustain, adsr.decay, b),
                Stage::curved(0.0, adsr.release, c),
            ]
        };
        self.desc.stages.clear();
        self.desc
            .stages
            .extend(fresh(Curve::Linear, Curve::Linear, Curve::Linear));
        self.desc.sustain = Some(1);
        self.desc.loop_range = None;
        self.desc.velocity = Velocity::default();
        self.phase = Phase::Idle;
        self.gated = false;
        self.pos = 0;
        self.len = 0;
        self.from = 0.0;
        self.level = 0.0;
        self.level_scale = 1.0;
        self.attack_scale = 1.0;
        self.fade_k = f32::MAX;
    }

    /// Start (or retrigger) at full velocity.
    pub fn gate_on(&mut self) {
        self.gate_on_velocity(1.0);
    }

    /// Start (or retrigger) with a velocity in `0..=1`. Retriggering ramps
    /// from the current level rather than snapping to 0, so a fast repeat
    /// does not click.
    pub fn gate_on_velocity(&mut self, velocity: f32) {
        let v = velocity.clamp(0.0, 1.0);
        let vel = self.desc.velocity;
        self.level_scale = 1.0 - vel.to_level.clamp(0.0, 1.0) * (1.0 - v);
        self.attack_scale = (1.0 - 0.9 * vel.to_attack.clamp(0.0, 1.0) * v).max(0.05);
        self.gated = true;
        // A retrigger attacks from wherever the level is — including
        // mid-kill-fade — so the fade is absorbed, not truncated.
        self.fade_k = f32::MAX;
        self.enter(0);
    }

    /// Release: jump to the release stages from the current level.
    /// One-shot envelopes (no sustain, no loop) ignore this.
    pub fn gate_off(&mut self) {
        self.gated = false;
        if let Some(r) = self.desc.release_start() {
            let in_pre_release = match self.phase {
                Phase::Stage(i) => i < r,
                Phase::Sustain => true,
                Phase::Idle => false,
            };
            if in_pre_release {
                self.enter(r);
            }
        }
    }

    /// Stop dead at level 0.
    pub fn reset(&mut self) {
        self.phase = Phase::Idle;
        self.level = 0.0;
        self.gated = false;
        self.fade_k = f32::MAX;
    }

    /// Stop over roughly `secs`: an exponential fade to zero from the
    /// current level, on a clock of its own — faster than the patch's
    /// release, softer than a reset. A stolen voice's hard cut is a click
    /// at whatever amplitude it had; its own release can be hundreds of
    /// milliseconds; this is the few-millisecond fade between them. A
    /// later retrigger still attacks from wherever the fade had got to,
    /// so stealing into a new note crossfades instead of stepping.
    pub fn kill(&mut self, secs: f32) {
        if self.phase == Phase::Idle {
            return; // already silent: nothing to fade
        }
        self.gated = false;
        self.fade_k = if secs > 0.0 {
            (-1.0 / (secs.max(1e-4) * self.rate)).exp()
        } else {
            0.0
        };
    }

    /// Whether the envelope is still producing (not idle).
    #[must_use]
    pub const fn is_active(&self) -> bool {
        !matches!(self.phase, Phase::Idle)
    }

    /// The current phase.
    #[must_use]
    pub const fn phase(&self) -> Phase {
        self.phase
    }

    /// The last produced level.
    #[must_use]
    pub const fn level(&self) -> f32 {
        self.level
    }

    fn enter(&mut self, i: usize) {
        if i >= self.desc.stages.len() {
            self.phase = Phase::Idle;
            return;
        }
        let st = self.desc.stages[i];
        let mut secs = st.seconds;
        if i == 0 {
            secs *= self.attack_scale;
        }
        self.phase = Phase::Stage(i);
        self.pos = 0;
        self.len = super::secs_to_samples(secs, self.rate);
        self.from = self.level;
        if self.len == 0 {
            self.level = st.target * self.level_scale;
            self.finish(i);
        }
    }

    fn finish(&mut self, i: usize) {
        if self.gated {
            if let Some((a, b)) = self.desc.loop_range {
                if i == b {
                    // Guard against a loop of zero-length stages spinning.
                    let all_zero = self.desc.stages[a..=b].iter().all(|s| s.seconds <= 0.0);
                    if !all_zero {
                        self.enter(a);
                        return;
                    }
                }
            }
            if self.desc.sustain == Some(i) {
                self.phase = Phase::Sustain;
                return;
            }
        }
        self.enter(i + 1);
    }

    /// The next level.
    pub fn next_level(&mut self) -> f32 {
        if let Phase::Stage(i) = self.phase {
            let st = self.desc.stages[i];
            self.pos += 1;
            #[expect(clippy::cast_precision_loss, reason = "stage lengths are small")]
            let x = self.pos as f32 / self.len as f32;
            let target = st.target * self.level_scale;
            self.level = self.from + (target - self.from) * st.curve.shape(x);
            if self.pos >= self.len {
                self.level = target;
                self.finish(i);
            }
        }
        // The kill fade rides on top of whatever the stages produce, and
        // retires the envelope when it has taken the level to silence.
        if self.fade_k != f32::MAX {
            self.level *= self.fade_k;
            if self.level < 1e-5 {
                self.level = 0.0;
                self.fade_k = f32::MAX;
                self.phase = Phase::Idle;
            }
        }
        // Idle and sustaining envelopes hold wherever the last stage left
        // them — the sustain level, or (finished) normally 0.
        self.level
    }

    /// Render `gate_samples` gated, then released, `total` samples in all.
    pub fn render(&mut self, velocity: f32, gate_samples: usize, total: usize) -> Vec<f32> {
        self.gate_on_velocity(velocity);
        (0..total)
            .map(|i| {
                if i == gate_samples {
                    self.gate_off();
                }
                self.next_level()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: f32 = 1000.0;

    #[test]
    fn adsr_reaches_peak_then_sustain_then_zero() {
        let mut e = EnvelopeGen::new(Adsr::new(0.01, 0.02, 0.5, 0.05).stages(), R);
        let v = e.render(1.0, 100, 200);
        assert!((v[9] - 1.0).abs() < 1e-6, "peak at end of attack: {}", v[9]);
        assert!((v[29] - 0.5).abs() < 1e-6, "sustain after decay");
        assert!((v[99] - 0.5).abs() < 1e-6, "held while gated");
        assert!(v[149].abs() < 1e-6, "silent after release");
        assert!(!e.is_active());
    }

    #[test]
    fn release_mid_attack_is_continuous() {
        let mut e = EnvelopeGen::new(Adsr::new(0.1, 0.1, 0.5, 0.1).stages(), R);
        let v = e.render(1.0, 50, 200);
        // At the release the level is 0.5 and falls from there, no step.
        for w in v.windows(2) {
            assert!((w[1] - w[0]).abs() < 0.02, "step {} -> {}", w[0], w[1]);
        }
        assert!(v[49] > 0.45 && v[49] < 0.55);
    }

    #[test]
    fn retrigger_ramps_from_current_level() {
        let mut e = EnvelopeGen::new(Adsr::new(0.05, 0.0, 1.0, 0.05).stages(), R);
        let _ = e.render(1.0, 60, 70);
        let before = e.level();
        e.gate_on();
        let first = e.next_level();
        assert!((first - before).abs() < 0.05, "{before} -> {first}");
    }

    #[test]
    fn velocity_scales_level_and_attack() {
        let desc = Adsr::new(0.1, 0.0, 1.0, 0.1).stages().velocity(Velocity {
            to_level: 1.0,
            to_attack: 1.0,
        });
        let mut soft = EnvelopeGen::new(desc.clone(), R);
        let s = soft.render(0.5, 300, 300);
        let mut hard = EnvelopeGen::new(desc, R);
        let h = hard.render(1.0, 300, 300);
        assert!((s[200] - 0.5).abs() < 1e-5);
        assert!((h[200] - 1.0).abs() < 1e-5);
        let reach = |v: &[f32], lvl: f32| v.iter().position(|x| *x >= lvl * 0.999).unwrap();
        assert!(reach(&h, 1.0) < reach(&s, 0.5), "harder speaks faster");
    }

    #[test]
    fn exponential_curves_bow_the_right_way() {
        assert!(Curve::Exp(5.0).shape(0.5) < 0.5);
        assert!(Curve::Exp(-5.0).shape(0.5) > 0.5);
        assert!((Curve::Exp(3.0).shape(1.0) - 1.0).abs() < 1e-6);
        assert!(Curve::Exp(3.0).shape(0.0).abs() < 1e-6);
        assert!(Curve::Step.shape(0.99) == 0.0 && Curve::Step.shape(1.0) == 1.0);
    }

    #[test]
    fn dahdsr_is_silent_through_its_delay_and_holds_its_peak() {
        let d = Dahdsr {
            delay: 0.02,
            attack: 0.01,
            hold: 0.02,
            decay: 0.02,
            sustain: 0.3,
            release: 0.01,
        };
        let mut e = EnvelopeGen::new(d.stages(), R);
        let v = e.render(1.0, 200, 260);
        assert!(v[..19].iter().all(|x| *x == 0.0));
        assert!((v[35] - 1.0).abs() < 1e-6, "holding: {}", v[35]);
        assert!((v[150] - 0.3).abs() < 1e-5);
        assert!(v[230].abs() < 1e-6);
    }

    #[test]
    fn looping_envelope_cycles_while_gated_then_releases() {
        let desc = Stages {
            stages: vec![
                Stage::linear(1.0, 0.01),
                Stage::linear(0.2, 0.01),
                Stage::linear(1.0, 0.01),
                Stage::linear(0.0, 0.02),
            ],
            sustain: None,
            loop_range: Some((1, 2)),
            velocity: Velocity::default(),
        };
        let mut e = EnvelopeGen::new(desc, R);
        let v = e.render(1.0, 95, 140);
        // Peaks keep coming back to 1.0 every 20 samples while gated.
        let peaks = v[..95].iter().filter(|x| (**x - 1.0).abs() < 1e-6).count();
        assert!(peaks >= 4, "{peaks} peaks");
        assert!(v[120].abs() < 1e-6, "released after the loop");
    }

    #[test]
    fn one_shot_ignores_gate_off() {
        let mut e = EnvelopeGen::new(
            Stages::one_shot(vec![Stage::linear(1.0, 0.01), Stage::linear(0.0, 0.05)]),
            R,
        );
        let v = e.render(1.0, 2, 80);
        assert!(
            (v[9] - 1.0).abs() < 1e-6,
            "ran its attack despite early release"
        );
        assert!(v[70].abs() < 1e-6);
    }

    #[test]
    fn zero_length_stages_do_not_spin() {
        let desc = Stages {
            stages: vec![Stage::linear(1.0, 0.0), Stage::linear(0.5, 0.0)],
            sustain: None,
            loop_range: Some((0, 1)),
            velocity: Velocity::default(),
        };
        let mut e = EnvelopeGen::new(desc, R);
        e.gate_on();
        assert!(e.next_level().is_finite());
    }

    #[test]
    fn kill_fades_on_its_own_clock_and_is_absorbed_by_a_retrigger() {
        // Sustaining at 0.5 with a 0.05 s release — five times slower than
        // the kill's own fade, so this measures the kill, not the release.
        let mut e = EnvelopeGen::new(Adsr::new(0.01, 0.02, 0.5, 0.05).stages(), R);
        let _ = e.render(1.0, 100, 100);
        assert!((e.level() - 0.5).abs() < 1e-3, "sustaining: {}", e.level());
        e.kill(0.01);
        assert!(e.is_active(), "still sounding");
        // An exponential at the kill's time constant (e^-0.1 per sample at
        // 1 kHz): no step at the kill itself, and a constant ratio after.
        let k = (-0.1f32).exp();
        let a = e.next_level();
        assert!((a - 0.5 * k).abs() < 1e-3, "no step at the kill: {a}");
        let b = e.next_level();
        assert!((b - a * k).abs() < 1e-3, "exponential fade: {a} -> {b}");
        // A retrigger mid-fade climbs from wherever the fade had got to —
        // the fade is absorbed, not left running underneath the new note.
        let mid = e.level();
        e.gate_on();
        let c = e.next_level();
        assert!(
            c > mid,
            "retrigger climbs from the faded level: {mid} -> {c}"
        );
        // A fade left alone retires the envelope to idle in bounded time
        // (a dozen time constants; the level threshold is 1e-5).
        e.kill(0.01);
        for _ in 0..200 {
            let _ = e.next_level();
        }
        assert!(!e.is_active(), "retired");
        assert_eq!(e.level(), 0.0);
        // Killing an idle envelope is a no-op, not a resurrection.
        e.kill(0.01);
        assert_eq!(e.next_level(), 0.0);
    }
}
