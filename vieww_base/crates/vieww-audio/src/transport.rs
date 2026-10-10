//! Musical time: tempo maps, meters, bars/beats/ticks, a looping transport,
//! and a sample-accurate look-ahead scheduler.
//!
//! The crate's clock is the sample index. Music's clock is the *beat*, and
//! the two are related by a tempo that can change — in steps or in ramps
//! (accelerando, ritardando). [`TempoMap`] is that relation, exact in both
//! directions: a linear ramp in BPM over beats integrates to a logarithm,
//! and the inverse is an exponential, so [`TempoMap::seconds_at`] and
//! [`TempoMap::beat_at`] are closed-form, not numerically stepped.
//! [`Meter`] adds time-signature changes and the bar/beat/tick view a DAW's
//! counter shows (960 ticks per quarter, the modern PPQ).
//!
//! [`Scheduler`] answers the question a real-time engine asks every block:
//! *which events fall inside the next N samples, and at which sample?* It
//! walks the transport forward block by block, wrapping at the loop end
//! within a block when it has to, and reports each event's integer sample
//! offset and its sub-sample remainder — so an event at beat 3.000001 lands
//! where it should, not on the nearest block boundary.

/// Ticks per quarter note.
pub const PPQ: u32 = 960;

/// A tempo point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoPoint {
    /// Where, in quarter-note beats.
    pub beat: f64,
    /// Beats per minute from here.
    pub bpm: f64,
    /// Ramp linearly (in BPM per beat) to the next point instead of holding.
    pub ramp: bool,
}

/// Tempo over the song.
#[derive(Debug, Clone, PartialEq)]
pub struct TempoMap {
    points: Vec<TempoPoint>,
    /// `seconds_at(points[i].beat)`, cached.
    starts: Vec<f64>,
}

impl TempoMap {
    /// A constant tempo.
    #[must_use]
    pub fn constant(bpm: f64) -> Self {
        Self::new(vec![TempoPoint {
            beat: 0.0,
            bpm,
            ramp: false,
        }])
    }

    /// A map from points (sorted by beat; the first is moved to beat 0).
    #[must_use]
    pub fn new(mut points: Vec<TempoPoint>) -> Self {
        points.sort_by(|a, b| a.beat.total_cmp(&b.beat));
        if points.is_empty() {
            points.push(TempoPoint {
                beat: 0.0,
                bpm: 120.0,
                ramp: false,
            });
        }
        points[0].beat = 0.0;
        for p in &mut points {
            p.bpm = p.bpm.max(1e-3);
        }
        let mut m = Self {
            points,
            starts: Vec::new(),
        };
        let mut acc = 0.0;
        let mut starts = vec![0.0];
        for i in 0..m.points.len() - 1 {
            acc += m.segment_seconds(i, m.points[i + 1].beat - m.points[i].beat);
            starts.push(acc);
        }
        m.starts = starts;
        m
    }

    /// (start bpm, slope in bpm per beat) of segment `i`.
    fn segment(&self, i: usize) -> (f64, f64) {
        let p = self.points[i];
        match self.points.get(i + 1) {
            Some(n) if p.ramp && n.beat > p.beat => (p.bpm, (n.bpm - p.bpm) / (n.beat - p.beat)),
            _ => (p.bpm, 0.0),
        }
    }

    /// Seconds spent covering `db` beats from the start of segment `i`.
    fn segment_seconds(&self, i: usize, db: f64) -> f64 {
        let (a, k) = self.segment(i);
        if k.abs() < 1e-12 {
            60.0 * db / a
        } else {
            60.0 / k * ((a + k * db) / a).ln()
        }
    }

    fn index_for_beat(&self, beat: f64) -> usize {
        self.points
            .partition_point(|p| p.beat <= beat)
            .saturating_sub(1)
    }

    /// The tempo at `beat`.
    #[must_use]
    pub fn bpm_at(&self, beat: f64) -> f64 {
        let i = self.index_for_beat(beat);
        let (a, k) = self.segment(i);
        a + k * (beat - self.points[i].beat).max(0.0)
    }

    /// Seconds from beat 0 to `beat`.
    #[must_use]
    pub fn seconds_at(&self, beat: f64) -> f64 {
        if beat <= 0.0 {
            return beat * 60.0 / self.points[0].bpm;
        }
        let i = self.index_for_beat(beat);
        self.starts[i] + self.segment_seconds(i, beat - self.points[i].beat)
    }

    /// The beat reached after `seconds`.
    #[must_use]
    pub fn beat_at(&self, seconds: f64) -> f64 {
        if seconds <= 0.0 {
            return seconds * self.points[0].bpm / 60.0;
        }
        let i = self
            .starts
            .partition_point(|s| *s <= seconds)
            .saturating_sub(1);
        let dt = seconds - self.starts[i];
        let (a, k) = self.segment(i);
        let db = if k.abs() < 1e-12 {
            dt * a / 60.0
        } else {
            a / k * ((k * dt / 60.0).exp() - 1.0)
        };
        self.points[i].beat + db
    }
}

/// A time-signature change at a bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signature {
    /// The bar (0-based) where it takes effect.
    pub bar: u32,
    /// Beats per bar.
    pub numerator: u32,
    /// Beat unit (4 = quarter, 8 = eighth).
    pub denominator: u32,
}

/// Time signatures over the song.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meter {
    sigs: Vec<Signature>,
}

/// A position as a DAW counter shows it (all 1-based except ticks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bbt {
    /// Bar, from 1.
    pub bar: u32,
    /// Beat within the bar, from 1 (in the signature's beat unit).
    pub beat: u32,
    /// Ticks within the beat unit (`PPQ` per quarter).
    pub tick: u32,
}

impl Meter {
    /// A constant signature.
    #[must_use]
    pub fn constant(numerator: u32, denominator: u32) -> Self {
        Self::new(vec![Signature {
            bar: 0,
            numerator,
            denominator,
        }])
    }

    /// From changes (the first is moved to bar 0).
    #[must_use]
    pub fn new(mut sigs: Vec<Signature>) -> Self {
        sigs.sort_by_key(|s| s.bar);
        if sigs.is_empty() {
            sigs.push(Signature {
                bar: 0,
                numerator: 4,
                denominator: 4,
            });
        }
        sigs[0].bar = 0;
        Self { sigs }
    }

    fn bar_quarters(s: Signature) -> f64 {
        f64::from(s.numerator) * 4.0 / f64::from(s.denominator.max(1))
    }

    /// Quarter-note beat where (0-based) `bar` starts.
    #[must_use]
    pub fn beat_of_bar(&self, bar: u32) -> f64 {
        let mut beat = 0.0;
        for (i, s) in self.sigs.iter().enumerate() {
            let end = self.sigs.get(i + 1).map_or(u32::MAX, |n| n.bar);
            if bar < end {
                return beat + f64::from(bar - s.bar) * Self::bar_quarters(*s);
            }
            beat += f64::from(end - s.bar) * Self::bar_quarters(*s);
        }
        beat
    }

    /// The counter position of a quarter-note `beat`.
    #[must_use]
    pub fn bbt(&self, beat: f64) -> Bbt {
        let beat = beat.max(0.0);
        let mut start = 0.0;
        for (i, s) in self.sigs.iter().enumerate() {
            let q = Self::bar_quarters(*s);
            let bars_here = self
                .sigs
                .get(i + 1)
                .map_or(f64::INFINITY, |n| f64::from(n.bar - s.bar));
            let end = start + bars_here * q;
            if beat < end {
                let into = beat - start;
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "bar counts"
                )]
                let bar_in = (into / q).floor() as u32;
                let in_bar = into - f64::from(bar_in) * q;
                let unit = 4.0 / f64::from(s.denominator.max(1));
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "beat counts"
                )]
                let b = (in_bar / unit).floor() as u32;
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "ticks"
                )]
                let tick = ((in_bar - f64::from(b) * unit) * f64::from(PPQ)).round() as u32;
                return Bbt {
                    bar: s.bar + bar_in + 1,
                    beat: b + 1,
                    tick,
                };
            }
            start = end;
        }
        Bbt {
            bar: 1,
            beat: 1,
            tick: 0,
        }
    }

    /// The quarter-note beat of a counter position.
    #[must_use]
    pub fn beat_of(&self, p: Bbt) -> f64 {
        let bar0 = p.bar.saturating_sub(1);
        let s = self
            .sigs
            .iter()
            .rev()
            .find(|s| s.bar <= bar0)
            .copied()
            .unwrap_or(self.sigs[0]);
        let unit = 4.0 / f64::from(s.denominator.max(1));
        self.beat_of_bar(bar0)
            + f64::from(p.beat.saturating_sub(1)) * unit
            + f64::from(p.tick) / f64::from(PPQ)
    }
}

/// Play state and position, in samples, with an optional loop.
#[derive(Debug, Clone, PartialEq)]
pub struct Transport {
    /// Tempo.
    pub tempo: TempoMap,
    /// Meter.
    pub meter: Meter,
    /// Sample rate.
    pub rate: f64,
    /// Playing?
    pub playing: bool,
    /// Loop region in beats.
    pub loop_range: Option<(f64, f64)>,
    position: f64,
}

impl Transport {
    /// Stopped at 0, 120 BPM, 4/4.
    #[must_use]
    pub fn new(rate: f64) -> Self {
        Self {
            tempo: TempoMap::constant(120.0),
            meter: Meter::constant(4, 4),
            rate,
            playing: false,
            loop_range: None,
            position: 0.0,
        }
    }

    /// Position in beats.
    #[must_use]
    pub fn beat(&self) -> f64 {
        self.tempo.beat_at(self.position / self.rate)
    }

    /// Seek to a beat.
    pub fn seek(&mut self, beat: f64) {
        self.position = self.tempo.seconds_at(beat) * self.rate;
    }

    /// The sample position (fractional).
    #[must_use]
    pub const fn samples(&self) -> f64 {
        self.position
    }

    /// Advance by `frames` without scheduling (wraps at the loop end).
    pub fn advance(&mut self, frames: usize) {
        let mut s = Scheduler;
        let _ = s.block::<()>(self, frames, &[]);
    }
}

/// An event in beats.
#[derive(Debug, Clone, PartialEq)]
pub struct Timed<E> {
    /// When.
    pub beat: f64,
    /// What.
    pub event: E,
}

/// An event placed in a block.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed<E> {
    /// Sample offset within the block.
    pub offset: usize,
    /// Sub-sample remainder in `0..1`.
    pub fraction: f64,
    /// The beat it was scheduled at.
    pub beat: f64,
    /// The event.
    pub event: E,
}

/// The look-ahead scheduler.
#[derive(Debug, Clone, Copy, Default)]
pub struct Scheduler;

impl Scheduler {
    /// Advance `t` by `frames` and return the events (from `events`, sorted
    /// by beat) that fall in that span, with their offsets. Loops wrap
    /// inside the block.
    pub fn block<E: Clone>(
        &mut self,
        t: &mut Transport,
        frames: usize,
        events: &[Timed<E>],
    ) -> Vec<Placed<E>> {
        let mut out = Vec::new();
        if !t.playing {
            return out;
        }
        #[expect(clippy::cast_precision_loss, reason = "block sizes")]
        let mut remaining = frames as f64;
        let mut done = 0.0f64;
        while remaining > 0.0 {
            let start_s = t.position;
            let mut end_s = start_s + remaining;
            let mut wrapped = false;
            if let Some((lb0, lb1)) = t.loop_range {
                let loop_end = t.tempo.seconds_at(lb1) * t.rate;
                if lb1 > lb0 && start_s < loop_end && end_s >= loop_end {
                    end_s = loop_end;
                    wrapped = true;
                }
            }
            let b0 = t.tempo.beat_at(start_s / t.rate);
            let b1 = t.tempo.beat_at(end_s / t.rate);
            let lo = events.partition_point(|e| e.beat < b0);
            for e in &events[lo..] {
                if e.beat >= b1 {
                    break;
                }
                let at = t.tempo.seconds_at(e.beat) * t.rate - start_s + done;
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "within block"
                )]
                let offset = at.floor().max(0.0) as usize;
                out.push(Placed {
                    offset,
                    fraction: at - at.floor(),
                    beat: e.beat,
                    event: e.event.clone(),
                });
            }
            let span = end_s - start_s;
            done += span;
            remaining -= span;
            if wrapped {
                if let Some((lb0, _)) = t.loop_range {
                    t.position = t.tempo.seconds_at(lb0) * t.rate;
                }
            } else {
                t.position = end_s;
            }
            if span <= 0.0 && !wrapped {
                break;
            }
        }
        out
    }
}

/// MIDI clock pulses (24 per quarter) falling in a block, as sample
/// offsets — for clock-out to hardware.
#[must_use]
pub fn clock_pulses(t: &Transport, frames: usize) -> Vec<usize> {
    let b0 = t.beat();
    #[expect(clippy::cast_precision_loss, reason = "block sizes")]
    let b1 = t.tempo.beat_at((t.samples() + frames as f64) / t.rate);
    let first = (b0 * 24.0).ceil();
    let mut out = Vec::new();
    let mut k = first;
    while k / 24.0 < b1 {
        let at = t.tempo.seconds_at(k / 24.0) * t.rate - t.samples();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "within block"
        )]
        out.push(at.max(0.0).floor() as usize);
        k += 1.0;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_tempo_round_trips() {
        let m = TempoMap::constant(120.0);
        assert!((m.seconds_at(8.0) - 4.0).abs() < 1e-12);
        assert!((m.beat_at(4.0) - 8.0).abs() < 1e-12);
    }

    #[test]
    fn step_changes_integrate() {
        let m = TempoMap::new(vec![
            TempoPoint {
                beat: 0.0,
                bpm: 120.0,
                ramp: false,
            },
            TempoPoint {
                beat: 4.0,
                bpm: 60.0,
                ramp: false,
            },
        ]);
        assert!((m.seconds_at(4.0) - 2.0).abs() < 1e-12);
        assert!((m.seconds_at(6.0) - 4.0).abs() < 1e-12);
        assert!((m.beat_at(4.0) - 6.0).abs() < 1e-12);
    }

    #[test]
    fn ramps_are_exact_and_invertible() {
        let m = TempoMap::new(vec![
            TempoPoint {
                beat: 0.0,
                bpm: 60.0,
                ramp: true,
            },
            TempoPoint {
                beat: 8.0,
                bpm: 180.0,
                ramp: false,
            },
        ]);
        // Numerically integrate dt = 60/bpm db.
        let mut t = 0.0;
        let n = 100_000;
        for i in 0..n {
            let b = (f64::from(i) + 0.5) * 8.0 / f64::from(n);
            t += 60.0 / m.bpm_at(b) * 8.0 / f64::from(n);
        }
        assert!(
            (m.seconds_at(8.0) - t).abs() < 1e-6,
            "{} vs {t}",
            m.seconds_at(8.0)
        );
        for b in [0.5, 3.3, 7.9, 12.0] {
            assert!((m.beat_at(m.seconds_at(b)) - b).abs() < 1e-9);
        }
        assert!((m.bpm_at(4.0) - 120.0).abs() < 1e-9);
    }

    #[test]
    fn meter_changes_and_counter() {
        let m = Meter::new(vec![
            Signature {
                bar: 0,
                numerator: 4,
                denominator: 4,
            },
            Signature {
                bar: 2,
                numerator: 7,
                denominator: 8,
            },
        ]);
        assert_eq!(m.beat_of_bar(2), 8.0);
        assert_eq!(m.beat_of_bar(3), 11.5);
        assert_eq!(
            m.bbt(0.0),
            Bbt {
                bar: 1,
                beat: 1,
                tick: 0
            }
        );
        assert_eq!(
            m.bbt(5.25),
            Bbt {
                bar: 2,
                beat: 2,
                tick: 240
            }
        );
        assert_eq!(
            m.bbt(9.0),
            Bbt {
                bar: 3,
                beat: 3,
                tick: 0
            },
            "eighth-note beats"
        );
        for b in [0.0, 5.25, 9.0, 12.75] {
            assert!((m.beat_of(m.bbt(b)) - b).abs() < 1e-9, "{b}");
        }
    }

    #[test]
    fn scheduler_places_events_sample_accurately() {
        let mut t = Transport::new(48_000.0);
        t.playing = true;
        let events = vec![
            Timed {
                beat: 0.0,
                event: 'a',
            },
            Timed {
                beat: 0.5,
                event: 'b',
            },
            Timed {
                beat: 1.000_01,
                event: 'c',
            },
        ];
        let mut s = Scheduler;
        let mut all = Vec::new();
        let mut base = 0usize;
        for _ in 0..30 {
            for p in s.block(&mut t, 1000, &events) {
                all.push((base + p.offset, p.fraction, p.event));
            }
            base += 1000;
        }
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].0, 0);
        assert_eq!(all[1].0, 12_000, "half a beat at 120 BPM");
        assert_eq!(all[2].0, 24_000);
        assert!((all[2].1 - 0.24).abs() < 1e-6, "sub-sample: {}", all[2].1);
    }

    #[test]
    fn loops_wrap_inside_a_block() {
        let mut t = Transport::new(1000.0);
        t.playing = true;
        t.loop_range = Some((0.0, 2.0)); // 1 s at 120 BPM
        let events = vec![
            Timed {
                beat: 0.0,
                event: 1,
            },
            Timed {
                beat: 1.0,
                event: 2,
            },
        ];
        let mut s = Scheduler;
        let placed = s.block(&mut t, 2500, &events);
        let offsets: Vec<(usize, i32)> = placed.iter().map(|p| (p.offset, p.event)).collect();
        assert_eq!(
            offsets,
            vec![(0, 1), (500, 2), (1000, 1), (1500, 2), (2000, 1)]
        );
        assert!((t.beat() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn stopped_transport_schedules_nothing() {
        let mut t = Transport::new(48_000.0);
        let mut s = Scheduler;
        assert!(s
            .block(
                &mut t,
                512,
                &[Timed {
                    beat: 0.0,
                    event: ()
                }]
            )
            .is_empty());
    }

    #[test]
    fn clock_out_is_24_per_quarter() {
        let mut t = Transport::new(48_000.0);
        t.playing = true;
        let p = clock_pulses(&t, 24_000);
        assert_eq!(p.len(), 24);
        assert_eq!(p[1], 1000);
        t.seek(4.0);
        assert!((t.beat() - 4.0).abs() < 1e-9);
    }
}
