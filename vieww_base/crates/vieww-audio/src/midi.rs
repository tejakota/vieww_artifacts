//! MIDI — messages, running-status streams and Standard MIDI Files
//! (TouchDesigner's MIDI In/Out CHOPs, Processing's `themidibus`,
//! openFrameworks' `ofxMidi`, §2.10/§2.11/§2.12).
//!
//! [`Message`] covers the channel voice messages, the common system
//! messages and SysEx. [`Parser`] consumes a live byte stream (running
//! status, interleaved realtime bytes). [`Smf`] reads and writes type 0/1
//! Standard MIDI Files with tempo maps, so a note's tick converts to
//! seconds via [`Smf::tick_to_seconds`]. [`MidiState`] folds a stream into
//! "what is held now": pressed notes, CC values, pitch bend — the shape a
//! visual patch actually reads. Device I/O is the platform's (CoreMIDI,
//! ALSA, WinMM); this crate is everything above the port.

use std::collections::BTreeMap;

/// A MIDI message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    NoteOff { channel: u8, key: u8, velocity: u8 },
    NoteOn { channel: u8, key: u8, velocity: u8 },
    PolyPressure { channel: u8, key: u8, pressure: u8 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    ProgramChange { channel: u8, program: u8 },
    ChannelPressure { channel: u8, pressure: u8 },
    /// −8192..=8191.
    PitchBend { channel: u8, value: i16 },
    SysEx(Vec<u8>),
    TimingClock,
    Start,
    Continue,
    Stop,
    ActiveSensing,
    Reset,
    SongPosition(u16),
}

impl Message {
    /// Wire bytes (status first).
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let ch = |s: u8, c: &u8| s | (c & 0x0f);
        match self {
            Self::NoteOff { channel, key, velocity } => vec![ch(0x80, channel), *key & 0x7f, *velocity & 0x7f],
            Self::NoteOn { channel, key, velocity } => vec![ch(0x90, channel), *key & 0x7f, *velocity & 0x7f],
            Self::PolyPressure { channel, key, pressure } => vec![ch(0xa0, channel), *key & 0x7f, *pressure & 0x7f],
            Self::ControlChange { channel, controller, value } => vec![ch(0xb0, channel), *controller & 0x7f, *value & 0x7f],
            Self::ProgramChange { channel, program } => vec![ch(0xc0, channel), *program & 0x7f],
            Self::ChannelPressure { channel, pressure } => vec![ch(0xd0, channel), *pressure & 0x7f],
            Self::PitchBend { channel, value } => {
                #[allow(clippy::cast_sign_loss)]
                let v = (i32::from(*value) + 8192).clamp(0, 16383) as u16;
                #[allow(clippy::cast_possible_truncation)]
                let (lo, hi) = ((v & 0x7f) as u8, (v >> 7) as u8);
                vec![ch(0xe0, channel), lo, hi]
            }
            Self::SysEx(d) => {
                let mut v = vec![0xf0];
                v.extend(d.iter().map(|b| b & 0x7f));
                v.push(0xf7);
                v
            }
            Self::TimingClock => vec![0xf8],
            Self::Start => vec![0xfa],
            Self::Continue => vec![0xfb],
            Self::Stop => vec![0xfc],
            Self::ActiveSensing => vec![0xfe],
            Self::Reset => vec![0xff],
            Self::SongPosition(p) => {
                #[allow(clippy::cast_possible_truncation)]
                let (lo, hi) = ((p & 0x7f) as u8, ((p >> 7) & 0x7f) as u8);
                vec![0xf2, lo, hi]
            }
        }
    }

    /// Note on with velocity 0 is a note off, by convention.
    #[must_use]
    pub fn normalized(self) -> Self {
        match self {
            Self::NoteOn { channel, key, velocity: 0 } => Self::NoteOff { channel, key, velocity: 64 },
            m => m,
        }
    }

    fn data_len(status: u8) -> usize {
        match status & 0xf0 {
            0xc0 | 0xd0 => 1,
            0x80..=0xe0 => 2,
            _ => match status {
                0xf2 => 2,
                0xf1 | 0xf3 => 1,
                _ => 0,
            },
        }
    }

    fn from_parts(status: u8, d: &[u8]) -> Option<Self> {
        let c = status & 0x0f;
        Some(match status & 0xf0 {
            0x80 => Self::NoteOff { channel: c, key: d[0], velocity: d[1] },
            0x90 => Self::NoteOn { channel: c, key: d[0], velocity: d[1] },
            0xa0 => Self::PolyPressure { channel: c, key: d[0], pressure: d[1] },
            0xb0 => Self::ControlChange { channel: c, controller: d[0], value: d[1] },
            0xc0 => Self::ProgramChange { channel: c, program: d[0] },
            0xd0 => Self::ChannelPressure { channel: c, pressure: d[0] },
            0xe0 => {
                #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
                let v = ((u16::from(d[1]) << 7 | u16::from(d[0])) as i32 - 8192) as i16;
                Self::PitchBend { channel: c, value: v }
            }
            _ => match status {
                0xf2 => Self::SongPosition(u16::from(d[1]) << 7 | u16::from(d[0])),
                0xf8 => Self::TimingClock,
                0xfa => Self::Start,
                0xfb => Self::Continue,
                0xfc => Self::Stop,
                0xfe => Self::ActiveSensing,
                0xff => Self::Reset,
                _ => return None,
            },
        })
    }
}

/// Note number → frequency (A4 = 69 = 440 Hz).
#[must_use]
pub fn note_to_hz(note: f32) -> f32 {
    440.0 * 2f32.powf((note - 69.0) / 12.0)
}

/// Note number → name ("C4" is 60).
#[must_use]
pub fn note_name(note: u8) -> String {
    const N: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", N[usize::from(note % 12)], i32::from(note / 12) - 1)
}

/// A streaming parser for a live MIDI byte stream.
#[derive(Debug, Default, Clone)]
pub struct Parser {
    running: Option<u8>,
    data: Vec<u8>,
    sysex: Option<Vec<u8>>,
}

impl Parser {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed bytes; returns completed messages.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Message> {
        let mut out = Vec::new();
        for &b in bytes {
            if b >= 0xf8 {
                // Realtime: may interleave anything, doesn't touch state.
                if let Some(m) = Message::from_parts(b, &[]) {
                    out.push(m);
                }
                continue;
            }
            if b == 0xf0 {
                self.sysex = Some(Vec::new());
                continue;
            }
            if b == 0xf7 {
                if let Some(s) = self.sysex.take() {
                    out.push(Message::SysEx(s));
                }
                continue;
            }
            if let Some(s) = &mut self.sysex {
                if b < 0x80 {
                    s.push(b);
                    continue;
                }
                self.sysex = None;
            }
            if b >= 0x80 {
                self.running = Some(b);
                self.data.clear();
                if Message::data_len(b) == 0 {
                    out.extend(Message::from_parts(b, &[]));
                    self.running = None;
                }
                continue;
            }
            let Some(st) = self.running else { continue };
            self.data.push(b);
            if self.data.len() == Message::data_len(st) {
                out.extend(Message::from_parts(st, &self.data));
                self.data.clear();
                if st >= 0xf0 {
                    self.running = None;
                }
            }
        }
        out
    }
}

/// Held state from a message stream.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct MidiState {
    /// (channel, key) → velocity for held notes.
    pub notes: BTreeMap<(u8, u8), u8>,
    /// (channel, controller) → value.
    pub cc: BTreeMap<(u8, u8), u8>,
    /// Per-channel pitch bend, −1..1.
    pub bend: [f32; 16],
    pub clock_ticks: u64,
    pub playing: bool,
}

impl MidiState {
    pub fn apply(&mut self, m: &Message) {
        match m.clone().normalized() {
            Message::NoteOn { channel, key, velocity } => {
                self.notes.insert((channel, key), velocity);
            }
            Message::NoteOff { channel, key, .. } => {
                self.notes.remove(&(channel, key));
            }
            Message::ControlChange { channel, controller, value } => {
                self.cc.insert((channel, controller), value);
            }
            Message::PitchBend { channel, value } => {
                self.bend[usize::from(channel & 15)] = f32::from(value) / 8192.0;
            }
            Message::TimingClock => self.clock_ticks += 1,
            Message::Start => {
                self.playing = true;
                self.clock_ticks = 0;
            }
            Message::Continue => self.playing = true,
            Message::Stop => self.playing = false,
            _ => {}
        }
    }

    /// A CC as 0..1.
    #[must_use]
    pub fn cc01(&self, channel: u8, controller: u8) -> f32 {
        f32::from(self.cc.get(&(channel, controller)).copied().unwrap_or(0)) / 127.0
    }
}

/// A timed event in a track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackEvent {
    Midi(Message),
    /// Microseconds per quarter note.
    Tempo(u32),
    TimeSignature { numerator: u8, denominator_pow2: u8 },
    TrackName(String),
    Text(String),
    Marker(String),
    /// Another meta event, kept as raw.
    Meta { kind: u8, data: Vec<u8> },
}

/// A Standard MIDI File.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Smf {
    pub format: u16,
    /// Ticks per quarter note.
    pub division: u16,
    /// Each track: (absolute tick, event), sorted.
    pub tracks: Vec<Vec<(u64, TrackEvent)>>,
}

fn read_vlq(b: &[u8], i: &mut usize) -> Result<u32, String> {
    let mut v: u32 = 0;
    for _ in 0..4 {
        let c = *b.get(*i).ok_or("truncated varint")?;
        *i += 1;
        v = (v << 7) | u32::from(c & 0x7f);
        if c & 0x80 == 0 {
            return Ok(v);
        }
    }
    Err("varint too long".into())
}

fn write_vlq(mut v: u32, out: &mut Vec<u8>) {
    let mut buf = vec![(v & 0x7f) as u8];
    v >>= 7;
    while v > 0 {
        buf.push(((v & 0x7f) as u8) | 0x80);
        v >>= 7;
    }
    buf.reverse();
    out.extend(buf);
}

impl Smf {
    /// An empty type-1 file.
    #[must_use]
    pub const fn new(division: u16) -> Self {
        Self {
            format: 1,
            division,
            tracks: Vec::new(),
        }
    }

    /// Parse `.mid` bytes.
    ///
    /// # Errors
    /// On malformed chunks.
    pub fn parse(b: &[u8]) -> Result<Self, String> {
        if b.len() < 14 || &b[0..4] != b"MThd" {
            return Err("not a MIDI file".into());
        }
        let u16at = |i: usize| u16::from_be_bytes([b[i], b[i + 1]]);
        let hlen = u32::from_be_bytes([b[4], b[5], b[6], b[7]]) as usize;
        let (format, ntracks, division) = (u16at(8), u16at(10), u16at(12));
        if division & 0x8000 != 0 {
            return Err("SMPTE time division is not supported".into());
        }
        let mut i = 8 + hlen;
        let mut tracks = Vec::new();
        while tracks.len() < usize::from(ntracks) && i + 8 <= b.len() {
            let id = &b[i..i + 4];
            let len = u32::from_be_bytes([b[i + 4], b[i + 5], b[i + 6], b[i + 7]]) as usize;
            let body = b.get(i + 8..i + 8 + len).ok_or("truncated track")?;
            i += 8 + len;
            if id != b"MTrk" {
                continue;
            }
            tracks.push(Self::parse_track(body)?);
        }
        Ok(Self { format, division, tracks })
    }

    fn parse_track(b: &[u8]) -> Result<Vec<(u64, TrackEvent)>, String> {
        let mut i = 0;
        let mut tick = 0u64;
        let mut running = 0u8;
        let mut ev = Vec::new();
        while i < b.len() {
            tick += u64::from(read_vlq(b, &mut i)?);
            let mut st = *b.get(i).ok_or("truncated event")?;
            if st < 0x80 {
                st = running;
            } else {
                i += 1;
            }
            match st {
                0xff => {
                    let kind = *b.get(i).ok_or("truncated meta")?;
                    i += 1;
                    let len = read_vlq(b, &mut i)? as usize;
                    let d = b.get(i..i + len).ok_or("truncated meta data")?.to_vec();
                    i += len;
                    let text = || String::from_utf8_lossy(&d).into_owned();
                    let e = match kind {
                        0x2f => break,
                        0x51 if len == 3 => TrackEvent::Tempo(u32::from(d[0]) << 16 | u32::from(d[1]) << 8 | u32::from(d[2])),
                        0x58 if len >= 2 => TrackEvent::TimeSignature { numerator: d[0], denominator_pow2: d[1] },
                        0x03 => TrackEvent::TrackName(text()),
                        0x01 => TrackEvent::Text(text()),
                        0x06 => TrackEvent::Marker(text()),
                        _ => TrackEvent::Meta { kind, data: d },
                    };
                    ev.push((tick, e));
                }
                0xf0 | 0xf7 => {
                    let len = read_vlq(b, &mut i)? as usize;
                    let mut d = b.get(i..i + len).ok_or("truncated sysex")?.to_vec();
                    i += len;
                    if d.last() == Some(&0xf7) {
                        d.pop();
                    }
                    ev.push((tick, TrackEvent::Midi(Message::SysEx(d))));
                }
                s if s >= 0x80 => {
                    running = s;
                    let n = Message::data_len(s);
                    let d = b.get(i..i + n).ok_or("truncated message")?;
                    i += n;
                    if let Some(m) = Message::from_parts(s, d) {
                        ev.push((tick, TrackEvent::Midi(m)));
                    }
                }
                _ => return Err("data byte without running status".into()),
            }
        }
        Ok(ev)
    }

    /// Serialise to `.mid` bytes.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = b"MThd".to_vec();
        out.extend(6u32.to_be_bytes());
        out.extend(self.format.to_be_bytes());
        #[allow(clippy::cast_possible_truncation)]
        out.extend((self.tracks.len() as u16).to_be_bytes());
        out.extend(self.division.to_be_bytes());
        for t in &self.tracks {
            let mut body = Vec::new();
            let mut last = 0u64;
            let mut events: Vec<&(u64, TrackEvent)> = t.iter().collect();
            events.sort_by_key(|e| e.0);
            for (tick, e) in events {
                #[allow(clippy::cast_possible_truncation)]
                write_vlq((tick - last) as u32, &mut body);
                last = *tick;
                let meta = |kind: u8, d: &[u8], body: &mut Vec<u8>| {
                    body.extend([0xff, kind]);
                    #[allow(clippy::cast_possible_truncation)]
                    write_vlq(d.len() as u32, body);
                    body.extend(d);
                };
                match e {
                    TrackEvent::Midi(Message::SysEx(d)) => {
                        body.push(0xf0);
                        #[allow(clippy::cast_possible_truncation)]
                        write_vlq(d.len() as u32 + 1, &mut body);
                        body.extend(d);
                        body.push(0xf7);
                    }
                    TrackEvent::Midi(m) => body.extend(m.to_bytes()),
                    TrackEvent::Tempo(us) => meta(0x51, &us.to_be_bytes()[1..], &mut body),
                    TrackEvent::TimeSignature { numerator, denominator_pow2 } => meta(0x58, &[*numerator, *denominator_pow2, 24, 8], &mut body),
                    TrackEvent::TrackName(s) => meta(0x03, s.as_bytes(), &mut body),
                    TrackEvent::Text(s) => meta(0x01, s.as_bytes(), &mut body),
                    TrackEvent::Marker(s) => meta(0x06, s.as_bytes(), &mut body),
                    TrackEvent::Meta { kind, data } => meta(*kind, data, &mut body),
                }
            }
            body.extend([0x00, 0xff, 0x2f, 0x00]);
            out.extend(b"MTrk");
            #[allow(clippy::cast_possible_truncation)]
            out.extend((body.len() as u32).to_be_bytes());
            out.extend(body);
        }
        out
    }

    /// Tempo changes across all tracks: (tick, µs per quarter), sorted,
    /// starting with the 120 bpm default.
    #[must_use]
    pub fn tempo_map(&self) -> Vec<(u64, u32)> {
        let mut m = vec![(0u64, 500_000u32)];
        for t in &self.tracks {
            for (tick, e) in t {
                if let TrackEvent::Tempo(us) = e {
                    m.push((*tick, *us));
                }
            }
        }
        m.sort_by_key(|x| x.0);
        m
    }

    /// Absolute tick → seconds, honouring every tempo change.
    #[must_use]
    pub fn tick_to_seconds(&self, tick: u64) -> f64 {
        let map = self.tempo_map();
        let mut secs = 0.0;
        let mut last_tick = 0u64;
        let mut us = 500_000u32;
        for &(t, tempo) in &map {
            if t >= tick {
                break;
            }
            secs += (t - last_tick) as f64 * f64::from(us) / 1e6 / f64::from(self.division);
            last_tick = t;
            us = tempo;
        }
        secs + (tick - last_tick) as f64 * f64::from(us) / 1e6 / f64::from(self.division)
    }

    /// Every note as (start s, duration s, channel, key, velocity), pairing
    /// ons with offs.
    #[must_use]
    pub fn notes(&self) -> Vec<(f64, f64, u8, u8, u8)> {
        let mut out = Vec::new();
        for t in &self.tracks {
            let mut open: BTreeMap<(u8, u8), (u64, u8)> = BTreeMap::new();
            for (tick, e) in t {
                if let TrackEvent::Midi(m) = e {
                    match m.clone().normalized() {
                        Message::NoteOn { channel, key, velocity } => {
                            open.insert((channel, key), (*tick, velocity));
                        }
                        Message::NoteOff { channel, key, .. } => {
                            if let Some((s, v)) = open.remove(&(channel, key)) {
                                let a = self.tick_to_seconds(s);
                                out.push((a, self.tick_to_seconds(*tick) - a, channel, key, v));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip_through_the_parser_with_running_status() {
        let msgs = [
            Message::NoteOn { channel: 2, key: 60, velocity: 100 },
            Message::ControlChange { channel: 0, controller: 7, value: 99 },
            Message::PitchBend { channel: 1, value: -4000 },
            Message::SysEx(vec![0x7e, 0x01]),
            Message::ProgramChange { channel: 9, program: 5 },
        ];
        let bytes: Vec<u8> = msgs.iter().flat_map(Message::to_bytes).collect();
        assert_eq!(Parser::new().feed(&bytes), msgs);
        // Running status + an interleaved clock byte.
        let got = Parser::new().feed(&[0x90, 60, 100, 0xf8, 64, 90, 67, 0]);
        assert_eq!(got.len(), 4);
        assert_eq!(got[1], Message::TimingClock);
        assert_eq!(got[3].clone().normalized(), Message::NoteOff { channel: 0, key: 67, velocity: 64 });
    }

    #[test]
    fn state_tracks_held_notes_cc_and_bend() {
        let mut s = MidiState::default();
        for m in Parser::new().feed(&[0x90, 60, 100, 0x90, 64, 80, 0x80, 60, 0, 0xb0, 1, 127, 0xe0, 0, 0x60]) {
            s.apply(&m);
        }
        assert_eq!(s.notes.keys().copied().collect::<Vec<_>>(), [(0, 64)]);
        assert_eq!(s.cc01(0, 1), 1.0);
        assert!((s.bend[0] - 0.5).abs() < 0.01);
    }

    #[test]
    fn smf_round_trip_and_tempo_map() {
        let mut f = Smf::new(480);
        f.tracks.push(vec![
            (0, TrackEvent::TrackName("lead".into())),
            (0, TrackEvent::Tempo(500_000)),
            (0, TrackEvent::Midi(Message::NoteOn { channel: 0, key: 60, velocity: 90 })),
            (480, TrackEvent::Midi(Message::NoteOff { channel: 0, key: 60, velocity: 0 })),
            (960, TrackEvent::Tempo(250_000)),
            (960, TrackEvent::Midi(Message::NoteOn { channel: 0, key: 67, velocity: 70 })),
            (1440, TrackEvent::Midi(Message::NoteOn { channel: 0, key: 67, velocity: 0 })),
        ]);
        let bytes = f.to_bytes();
        let back = Smf::parse(&bytes).unwrap();
        assert_eq!(back, f);
        assert!((back.tick_to_seconds(960) - 1.0).abs() < 1e-9);
        assert!((back.tick_to_seconds(1440) - 1.25).abs() < 1e-9, "doubled tempo after 1 s");
        let n = back.notes();
        assert_eq!(n.len(), 2);
        assert!((n[1].1 - 0.25).abs() < 1e-9);
    }

    #[test]
    fn note_helpers() {
        assert!((note_to_hz(69.0) - 440.0).abs() < 1e-3);
        assert!((note_to_hz(81.0) - 880.0).abs() < 1e-2);
        assert_eq!(note_name(60), "C4");
        assert_eq!(note_name(70), "A#4");
    }

    #[test]
    fn vlq_edges() {
        for v in [0u32, 127, 128, 16383, 16384, 0x0fff_ffff] {
            let mut b = Vec::new();
            write_vlq(v, &mut b);
            let mut i = 0;
            assert_eq!(read_vlq(&b, &mut i).unwrap(), v);
        }
    }
}
