//! MIDI Tuning Standard: carrying a tuning over MIDI.
//!
//! A score in just intonation or Carnatic ratios played on another synth,
//! or written to a Standard MIDI File, loses its tuning unless the tuning
//! travels with it. The MIDI Tuning Standard (MMA, 1992; extended 1999)
//! does that with universal system-exclusive messages. Two are here, both
//! real-time (they apply to sounding notes):
//!
//! * **Single-note tuning change** (`7F <dev> 08 02`): any key to any
//!   frequency, as a semitone plus a 14-bit fraction — a resolution of
//!   100/16384 ≈ 0.0061 cents. Retunes keys of a tuning *program*.
//! * **Scale/octave tuning, 2-byte form** (`7F <dev> 08 09`): twelve offsets
//!   of ±100 cents (resolution 100/8192 ≈ 0.012 cents) applied to every
//!   octave, on a chosen set of MIDI channels. One message sets a whole
//!   just-intoned or raga scale.
//!
//! Payloads are [`Message::SysEx`] bodies: without the `F0`/`F7` framing,
//! as the rest of the crate stores them. [`parse`] reads either kind back,
//! and [`MtsChange::apply`] writes it into a [`TuningTable`], so a stream
//! of tuning messages retunes a [`Poly`](crate::synth::voice::Poly) live.

use crate::midi::Message;
use crate::synth::tuning::TuningTable;

/// "All devices".
pub const ALL_DEVICES: u8 = 0x7F;

/// A decoded tuning message.
#[derive(Debug, Clone, PartialEq)]
pub enum MtsChange {
    /// Keys of tuning `program` retuned: `(key, equal-tempered pitch)`.
    SingleNote {
        /// The tuning program addressed.
        program: u8,
        /// Each key and the 12-TET pitch (69 = 440 Hz) it should sound.
        keys: Vec<(u8, f32)>,
    },
    /// Twelve per-pitch-class offsets in cents on the channels in `channels`
    /// (bit `n` = MIDI channel `n`, 0-based).
    Octave {
        /// Channel mask.
        channels: u16,
        /// Cents offsets for C, C♯, D … B.
        cents: [f32; 12],
    },
}

/// A single-note tuning change: each `(key, pitch)` makes `key` sound at
/// the 12-TET pitch `pitch` (fractional; 69.0 = A440).
///
/// # Panics
/// More than 127 keys in one message (the count is one data byte).
#[must_use]
pub fn single_note(device: u8, program: u8, keys: &[(u8, f32)]) -> Message {
    assert!(keys.len() <= 127, "one message retunes at most 127 keys");
    let mut d = vec![0x7F, device & 0x7F, 0x08, 0x02, program & 0x7F];
    #[expect(clippy::cast_possible_truncation, reason = "asserted ≤ 127")]
    d.push(keys.len() as u8);
    for &(key, pitch) in keys {
        let p = pitch.clamp(0.0, 127.999_9);
        let semi = p.floor();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to 0..=16383"
        )]
        let frac = (((p - semi) * 16_384.0).round() as u32).min(16_383);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "0..=127"
        )]
        let semi = semi as u8;
        #[expect(clippy::cast_possible_truncation, reason = "7-bit fields")]
        d.extend([
            key & 0x7F,
            semi,
            (frac >> 7) as u8 & 0x7F,
            frac as u8 & 0x7F,
        ]);
    }
    Message::SysEx(d)
}

/// A scale/octave tuning (2-byte form): `cents[i]` offsets pitch class
/// `i` (0 = C) by up to ±100 cents, in every octave, on the channels set
/// in `channels` (bit `n` = channel `n`, 0-based).
#[must_use]
pub fn octave(device: u8, channels: u16, cents: &[f32; 12]) -> Message {
    let (ff, gg, hh) = (
        ((channels >> 14) & 0x03) as u8,
        ((channels >> 7) & 0x7F) as u8,
        (channels & 0x7F) as u8,
    );
    let mut d = vec![0x7F, device & 0x7F, 0x08, 0x09, ff, gg, hh];
    for &c in cents {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to 0..=16383"
        )]
        let v = (((c.clamp(-100.0, 100.0) + 100.0) / 200.0 * 16_384.0).round() as u32).min(16_383);
        #[expect(clippy::cast_possible_truncation, reason = "7-bit fields")]
        d.extend([(v >> 7) as u8 & 0x7F, v as u8 & 0x7F]);
    }
    Message::SysEx(d)
}

/// Read a tuning message; `None` for any other system exclusive.
#[must_use]
pub fn parse(payload: &[u8]) -> Option<MtsChange> {
    match payload {
        [0x7F, _, 0x08, 0x02, program, count, rest @ ..] => {
            let n = usize::from(*count);
            if rest.len() < n * 4 {
                return None;
            }
            let keys = rest
                .as_chunks::<4>()
                .0
                .iter()
                .take(n)
                // 7F 7F 7F is "no change" for that key.
                .filter(|q| q[1..] != [0x7F, 0x7F, 0x7F])
                .map(|q| {
                    let frac = (u32::from(q[2]) << 7) | u32::from(q[3]);
                    #[expect(clippy::cast_precision_loss, reason = "14-bit")]
                    let pitch = f32::from(q[1]) + frac as f32 / 16_384.0;
                    (q[0], pitch)
                })
                .collect();
            Some(MtsChange::SingleNote {
                program: *program,
                keys,
            })
        }
        [0x7F, _, 0x08, 0x09, ff, gg, hh, rest @ ..] if rest.len() >= 24 => {
            let channels = (u16::from(*ff & 0x03) << 14)
                | (u16::from(*gg & 0x7F) << 7)
                | u16::from(*hh & 0x7F);
            let mut cents = [0.0f32; 12];
            for (c, q) in cents.iter_mut().zip(rest.as_chunks::<2>().0) {
                let v = (u32::from(q[0]) << 7) | u32::from(q[1]);
                #[expect(clippy::cast_precision_loss, reason = "14-bit")]
                let v = v as f32;
                *c = v / 16_384.0 * 200.0 - 100.0;
            }
            Some(MtsChange::Octave { channels, cents })
        }
        _ => None,
    }
}

impl MtsChange {
    /// Write the change into `table`: a single-note change sets those
    /// keys; an octave change sets every key to its 12-TET pitch plus its
    /// pitch class's offset.
    pub fn apply(&self, table: &mut TuningTable) {
        match self {
            Self::SingleNote { keys, .. } => {
                for &(k, p) in keys {
                    table.set_key(k, p);
                }
            }
            Self::Octave { cents, .. } => {
                for k in 0..128u8 {
                    table.set_key(k, f32::from(k) + cents[usize::from(k % 12)] / 100.0);
                }
            }
        }
    }
}

/// The octave message that plays `cents_from_tonic` (cents of each of
/// twelve pitch classes above `tonic_class`, 0 = C) as offsets from 12-TET
/// — the one-message way to send a just or raga scale to an external synth.
#[must_use]
pub fn octave_from_scale(
    device: u8,
    channels: u16,
    tonic_class: u8,
    cents_from_tonic: &[f32; 12],
) -> Message {
    let mut off = [0.0f32; 12];
    for (i, c) in cents_from_tonic.iter().enumerate() {
        #[expect(clippy::cast_precision_loss, reason = "0..12")]
        let et = i as f32 * 100.0;
        off[(usize::from(tonic_class % 12) + i) % 12] = c - et;
    }
    octave(device, channels, &off)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(m: &Message) -> &[u8] {
        match m {
            Message::SysEx(d) => d,
            _ => panic!("not sysex"),
        }
    }

    #[test]
    fn single_note_round_trips_to_a_hundredth_of_a_cent() {
        let keys = [(60u8, 60.0f32), (64, 63.863_14), (67, 69.019_55), (0, 0.5)];
        let m = single_note(ALL_DEVICES, 0, &keys);
        let d = payload(&m);
        assert_eq!(&d[..6], &[0x7F, 0x7F, 0x08, 0x02, 0, 4]);
        let Some(MtsChange::SingleNote { keys: back, .. }) = parse(d) else {
            panic!("parse");
        };
        for ((k, p), (k2, p2)) in keys.iter().zip(&back) {
            assert_eq!(k, k2);
            assert!((p - p2).abs() * 100.0 < 0.01, "{p} vs {p2}");
        }
        // Bytes as the standard spells them: A440 is 45 00 00.
        let a = single_note(ALL_DEVICES, 0, &[(69, 69.0)]);
        assert_eq!(&payload(&a)[6..], &[69, 69, 0, 0]);
    }

    #[test]
    fn octave_round_trips_and_masks_channels() {
        let cents = [
            0.0, 11.7, 3.9, 15.6, -13.7, -2.0, -9.8, 2.0, 13.7, -15.6, 17.6, -11.7,
        ];
        let m = octave(ALL_DEVICES, 0b1000_0000_0000_0101, &cents);
        let d = payload(&m);
        assert_eq!(&d[4..7], &[0b10, 0, 0b101]);
        let Some(MtsChange::Octave {
            channels,
            cents: back,
        }) = parse(d)
        else {
            panic!("parse");
        };
        assert_eq!(channels, 0b1000_0000_0000_0101);
        for (a, b) in cents.iter().zip(back) {
            assert!((a - b).abs() < 0.02, "{a} vs {b}");
        }
        // Zero cents is the centre code 0x2000 = 40 00.
        let z = octave(ALL_DEVICES, 1, &[0.0; 12]);
        assert_eq!(&payload(&z)[7..9], &[0x40, 0x00]);
    }

    #[test]
    fn applied_changes_retune_a_table() {
        let mut t = TuningTable::default();
        parse(payload(&single_note(ALL_DEVICES, 0, &[(64, 63.863_14)])))
            .unwrap()
            .apply(&mut t);
        assert!((t.key(64) - 63.863_14).abs() < 1e-3);
        assert!((t.key(65) - 65.0).abs() < 1e-6, "other keys untouched");

        // A just major scale on D (pitch class 2), one message.
        let just = [
            0.0, 0.0, 203.9, 0.0, 386.3, 498.0, 0.0, 702.0, 0.0, 884.4, 0.0, 1088.3,
        ];
        let mut t = TuningTable::default();
        parse(payload(&octave_from_scale(ALL_DEVICES, 0xFFFF, 2, &just)))
            .unwrap()
            .apply(&mut t);
        // F♯ (D + 4 semitones, class 6) is a just third: 13.7 cents flat.
        assert!((t.key(66) - (66.0 - 0.137)).abs() < 1e-3, "{}", t.key(66));
        assert!((t.key(62) - 62.0).abs() < 1e-3, "the tonic stays");
    }

    #[test]
    fn other_sysex_is_not_a_tuning() {
        assert_eq!(parse(&[0x7F, 0x7F, 0x01, 0x01, 0, 0, 0, 0]), None);
        assert_eq!(
            parse(&[0x7F, 0x7F, 0x08, 0x02, 0, 5, 1, 2]),
            None,
            "truncated"
        );
    }
}
