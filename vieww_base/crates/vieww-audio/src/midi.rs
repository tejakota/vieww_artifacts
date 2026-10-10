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
    NoteOff {
        channel: u8,
        key: u8,
        velocity: u8,
    },
    NoteOn {
        channel: u8,
        key: u8,
        velocity: u8,
    },
    PolyPressure {
        channel: u8,
        key: u8,
        pressure: u8,
    },
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
    },
    ProgramChange {
        channel: u8,
        program: u8,
    },
    ChannelPressure {
        channel: u8,
        pressure: u8,
    },
    /// −8192..=8191.
    PitchBend {
        channel: u8,
        value: i16,
    },
    SysEx(Vec<u8>),
    /// MIDI Time Code quarter frame: `piece << 4 | nibble`.
    QuarterFrame(u8),
    /// Song select.
    SongSelect(u8),
    /// Tune request.
    TuneRequest,
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
            Self::NoteOff {
                channel,
                key,
                velocity,
            } => vec![ch(0x80, channel), *key & 0x7f, *velocity & 0x7f],
            Self::NoteOn {
                channel,
                key,
                velocity,
            } => vec![ch(0x90, channel), *key & 0x7f, *velocity & 0x7f],
            Self::PolyPressure {
                channel,
                key,
                pressure,
            } => vec![ch(0xa0, channel), *key & 0x7f, *pressure & 0x7f],
            Self::ControlChange {
                channel,
                controller,
                value,
            } => vec![ch(0xb0, channel), *controller & 0x7f, *value & 0x7f],
            Self::ProgramChange { channel, program } => vec![ch(0xc0, channel), *program & 0x7f],
            Self::ChannelPressure { channel, pressure } => {
                vec![ch(0xd0, channel), *pressure & 0x7f]
            }
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
            Self::QuarterFrame(v) => vec![0xf1, v & 0x7f],
            Self::SongSelect(v) => vec![0xf3, v & 0x7f],
            Self::TuneRequest => vec![0xf6],
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
            Self::NoteOn {
                channel,
                key,
                velocity: 0,
            } => Self::NoteOff {
                channel,
                key,
                velocity: 64,
            },
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
            0x80 => Self::NoteOff {
                channel: c,
                key: d[0],
                velocity: d[1],
            },
            0x90 => Self::NoteOn {
                channel: c,
                key: d[0],
                velocity: d[1],
            },
            0xa0 => Self::PolyPressure {
                channel: c,
                key: d[0],
                pressure: d[1],
            },
            0xb0 => Self::ControlChange {
                channel: c,
                controller: d[0],
                value: d[1],
            },
            0xc0 => Self::ProgramChange {
                channel: c,
                program: d[0],
            },
            0xd0 => Self::ChannelPressure {
                channel: c,
                pressure: d[0],
            },
            0xe0 => {
                #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
                let v = ((u16::from(d[1]) << 7 | u16::from(d[0])) as i32 - 8192) as i16;
                Self::PitchBend {
                    channel: c,
                    value: v,
                }
            }
            _ => match status {
                0xf2 => Self::SongPosition(u16::from(d[1]) << 7 | u16::from(d[0])),
                0xf1 => Self::QuarterFrame(d[0]),
                0xf3 => Self::SongSelect(d[0]),
                0xf6 => Self::TuneRequest,
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
    const N: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
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
            Message::NoteOn {
                channel,
                key,
                velocity,
            } => {
                self.notes.insert((channel, key), velocity);
            }
            Message::NoteOff { channel, key, .. } => {
                self.notes.remove(&(channel, key));
            }
            Message::ControlChange {
                channel,
                controller,
                value,
            } => {
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
    TimeSignature {
        numerator: u8,
        denominator_pow2: u8,
    },
    TrackName(String),
    Text(String),
    Marker(String),
    /// Another meta event, kept as raw.
    Meta {
        kind: u8,
        data: Vec<u8>,
    },
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
        Ok(Self {
            format,
            division,
            tracks,
        })
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
                        0x51 if len == 3 => TrackEvent::Tempo(
                            u32::from(d[0]) << 16 | u32::from(d[1]) << 8 | u32::from(d[2]),
                        ),
                        0x58 if len >= 2 => TrackEvent::TimeSignature {
                            numerator: d[0],
                            denominator_pow2: d[1],
                        },
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
                    TrackEvent::TimeSignature {
                        numerator,
                        denominator_pow2,
                    } => meta(0x58, &[*numerator, *denominator_pow2, 24, 8], &mut body),
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
    /// starting with the 120 bpm default. For a type-2 file — independent
    /// patterns — use [`tempo_map_of`](Self::tempo_map_of) instead.
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

    /// One track's own tempo map (type 2: each pattern carries its own).
    #[must_use]
    pub fn tempo_map_of(&self, track: usize) -> Vec<(u64, u32)> {
        let mut m = vec![(0u64, 500_000u32)];
        if let Some(t) = self.tracks.get(track) {
            for (tick, e) in t {
                if let TrackEvent::Tempo(us) = e {
                    m.push((*tick, *us));
                }
            }
        }
        m.sort_by_key(|x| x.0);
        m
    }

    /// SMPTE timing, when the division says so: (frames per second, ticks
    /// per frame). 29 means 29.97 (drop-frame).
    #[must_use]
    pub fn smpte(&self) -> Option<(f64, u32)> {
        if self.division & 0x8000 == 0 {
            return None;
        }
        let fps = -i32::from(self.division.to_be_bytes()[0].cast_signed());
        let tpf = u32::from(self.division & 0xFF);
        let rate = if fps == 29 {
            30_000.0 / 1001.0
        } else {
            f64::from(fps)
        };
        Some((rate, tpf))
    }

    fn seconds_with(&self, map: &[(u64, u32)], tick: u64) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "tick counts")]
        if let Some((fps, tpf)) = self.smpte() {
            return tick as f64 / (fps * f64::from(tpf.max(1)));
        }
        let mut secs = 0.0;
        let mut last_tick = 0u64;
        let mut us = 500_000u32;
        for &(t, tempo) in map {
            if t >= tick {
                break;
            }
            secs += (t - last_tick) as f64 * f64::from(us) / 1e6 / f64::from(self.division);
            last_tick = t;
            us = tempo;
        }
        secs + (tick - last_tick) as f64 * f64::from(us) / 1e6 / f64::from(self.division)
    }

    /// A tick in `track` → seconds, with that track's own tempo for type 2
    /// files and the merged map otherwise.
    #[must_use]
    pub fn tick_to_seconds_in(&self, track: usize, tick: u64) -> f64 {
        if self.format == 2 {
            self.seconds_with(&self.tempo_map_of(track), tick)
        } else {
            self.tick_to_seconds(tick)
        }
    }

    /// Pattern `i` of a type-2 file as a type-0 file of its own.
    #[must_use]
    pub fn pattern(&self, i: usize) -> Option<Self> {
        Some(Self {
            format: 0,
            division: self.division,
            tracks: vec![self.tracks.get(i)?.clone()],
        })
    }

    /// A type-0 file from notes in beats — MIDI clip export.
    #[must_use]
    pub fn from_notes(notes: &[ClipNote], ppq: u16, bpm: f64) -> Self {
        let mut t: Vec<(u64, TrackEvent)> = Vec::with_capacity(notes.len() * 2 + 1);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "tempo in µs"
        )]
        t.push((
            0,
            TrackEvent::Tempo((60_000_000.0 / bpm.max(1.0)).round() as u32),
        ));
        let tick = |b: f64| -> u64 {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "non-negative ticks"
            )]
            let v = (b.max(0.0) * f64::from(ppq)).round() as u64;
            v
        };
        for n in notes {
            let on = tick(n.start);
            let off = tick(n.start + n.length).max(on + 1);
            t.push((
                off,
                TrackEvent::Midi(Message::NoteOff {
                    channel: n.channel,
                    key: n.key,
                    velocity: 64,
                }),
            ));
            t.push((
                on,
                TrackEvent::Midi(Message::NoteOn {
                    channel: n.channel,
                    key: n.key,
                    velocity: n.velocity.max(1),
                }),
            ));
        }
        // Offs before ons at the same tick, so a repeated note retriggers.
        t.sort_by_key(|(tk, e)| (*tk, !matches!(e, TrackEvent::Midi(Message::NoteOff { .. }))));
        Self {
            format: 0,
            division: ppq,
            tracks: vec![t],
        }
    }

    /// Absolute tick → seconds, honouring every tempo change (or SMPTE
    /// timing).
    #[must_use]
    pub fn tick_to_seconds(&self, tick: u64) -> f64 {
        if self.smpte().is_some() {
            return self.seconds_with(&[], tick);
        }
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
    /// ons with offs. Type-2 patterns are each timed by their own tempo
    /// from zero.
    #[must_use]
    pub fn notes(&self) -> Vec<(f64, f64, u8, u8, u8)> {
        let mut out = Vec::new();
        for (ti, t) in self.tracks.iter().enumerate() {
            let mut open: BTreeMap<(u8, u8), (u64, u8)> = BTreeMap::new();
            for (tick, e) in t {
                if let TrackEvent::Midi(m) = e {
                    match m.clone().normalized() {
                        Message::NoteOn {
                            channel,
                            key,
                            velocity,
                        } => {
                            open.insert((channel, key), (*tick, velocity));
                        }
                        Message::NoteOff { channel, key, .. } => {
                            if let Some((s, v)) = open.remove(&(channel, key)) {
                                let a = self.tick_to_seconds_in(ti, s);
                                out.push((
                                    a,
                                    self.tick_to_seconds_in(ti, *tick) - a,
                                    channel,
                                    key,
                                    v,
                                ));
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

/// A note in a clip, in beats.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipNote {
    /// Start, in quarter-note beats.
    pub start: f64,
    /// Length, in beats.
    pub length: f64,
    /// Channel 0..=15.
    pub channel: u8,
    /// Key.
    pub key: u8,
    /// Velocity.
    pub velocity: u8,
}

/// Follows an incoming MIDI clock (24 pulses per quarter): tempo, play
/// state and song position — "sync to external".
#[derive(Debug, Clone, Default)]
pub struct ClockIn {
    last: Option<f64>,
    interval: Option<f64>,
    /// Pulses since Start (or the song position).
    pub pulses: u64,
    /// Whether the master is playing.
    pub playing: bool,
}

impl ClockIn {
    /// Feed a message that arrived at `time` seconds.
    pub fn feed(&mut self, m: &Message, time: f64) {
        match m {
            Message::TimingClock => {
                if let Some(l) = self.last {
                    let dt = time - l;
                    if dt > 0.0 && dt < 0.5 {
                        // Exponential smoothing: steady against jitter, still
                        // follows a tempo change within a beat or two.
                        self.interval = Some(self.interval.map_or(dt, |i| i + (dt - i) * 0.1));
                    }
                }
                self.last = Some(time);
                if self.playing {
                    self.pulses += 1;
                }
            }
            Message::Start => {
                self.playing = true;
                self.pulses = 0;
            }
            Message::Continue => self.playing = true,
            Message::Stop => self.playing = false,
            Message::SongPosition(sixteenths) => self.pulses = u64::from(*sixteenths) * 6,
            _ => {}
        }
    }

    /// The master's tempo, once two pulses have arrived.
    #[must_use]
    pub fn bpm(&self) -> Option<f64> {
        self.interval.map(|i| 60.0 / (i * 24.0))
    }

    /// Position in quarter-note beats.
    #[must_use]
    pub fn beat(&self) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "pulse counts")]
        let b = self.pulses as f64 / 24.0;
        b
    }
}

/// MIDI Time Code frame rates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtcRate {
    /// 24 fps (film).
    Fps24,
    /// 25 fps (PAL).
    Fps25,
    /// 29.97 fps drop-frame (NTSC).
    Fps2997Drop,
    /// 30 fps.
    Fps30,
}

impl MtcRate {
    /// Frames per second as counted (drop-frame counts 30).
    #[must_use]
    pub const fn nominal(self) -> u32 {
        match self {
            Self::Fps24 => 24,
            Self::Fps25 => 25,
            Self::Fps2997Drop | Self::Fps30 => 30,
        }
    }

    const fn code(self) -> u8 {
        match self {
            Self::Fps24 => 0,
            Self::Fps25 => 1,
            Self::Fps2997Drop => 2,
            Self::Fps30 => 3,
        }
    }

    const fn from_code(c: u8) -> Self {
        match c & 3 {
            0 => Self::Fps24,
            1 => Self::Fps25,
            2 => Self::Fps2997Drop,
            _ => Self::Fps30,
        }
    }
}

/// An SMPTE time code position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeCode {
    /// Hours 0..=23.
    pub hours: u8,
    /// Minutes.
    pub minutes: u8,
    /// Seconds.
    pub seconds: u8,
    /// Frames.
    pub frames: u8,
    /// Rate.
    pub rate: MtcRate,
}

impl TimeCode {
    /// The time code at `seconds` (drop-frame numbering for 29.97).
    #[must_use]
    pub fn from_seconds(seconds: f64, rate: MtcRate) -> Self {
        let s = seconds.max(0.0);
        let nominal = u64::from(rate.nominal());
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "frame counts"
        )]
        let mut frames = if rate == MtcRate::Fps2997Drop {
            (s * 30_000.0 / 1001.0).floor() as u64
        } else {
            (s * nominal as f64).floor() as u64
        };
        if rate == MtcRate::Fps2997Drop {
            // Re-insert the two frame numbers skipped each minute except
            // every tenth (SMPTE 12M drop-frame).
            let per10 = 17_982; // frames in 10 minutes
            let d = frames / per10;
            let m = frames % per10;
            frames += 18 * d + if m > 2 { 2 * ((m - 2) / 1798) } else { 0 };
        }
        let f = frames % nominal;
        let total_s = frames / nominal;
        Self {
            hours: u8::try_from((total_s / 3600) % 24).unwrap_or(0),
            minutes: u8::try_from((total_s / 60) % 60).unwrap_or(0),
            seconds: u8::try_from(total_s % 60).unwrap_or(0),
            frames: u8::try_from(f).unwrap_or(0),
            rate,
        }
    }

    /// Seconds from 00:00:00:00.
    #[must_use]
    pub fn to_seconds(&self) -> f64 {
        let nominal = u64::from(self.rate.nominal());
        let total_min = u64::from(self.hours) * 60 + u64::from(self.minutes);
        let mut frames =
            (total_min * 60 + u64::from(self.seconds)) * nominal + u64::from(self.frames);
        if self.rate == MtcRate::Fps2997Drop {
            frames -= 2 * (total_min - total_min / 10);
            #[expect(clippy::cast_precision_loss, reason = "frame counts")]
            return frames as f64 * 1001.0 / 30_000.0;
        }
        #[expect(clippy::cast_precision_loss, reason = "frame counts")]
        let s = frames as f64 / nominal as f64;
        s
    }

    /// The eight quarter-frame messages describing this time.
    #[must_use]
    pub fn quarter_frames(&self) -> [Message; 8] {
        let hr = self.hours | (self.rate.code() << 5);
        let nib = [
            self.frames & 0xF,
            self.frames >> 4,
            self.seconds & 0xF,
            self.seconds >> 4,
            self.minutes & 0xF,
            self.minutes >> 4,
            hr & 0xF,
            hr >> 4,
        ];
        std::array::from_fn(|i| {
            Message::QuarterFrame((u8::try_from(i).unwrap_or(0) << 4) | (nib[i] & 0xF))
        })
    }

    /// The full-frame SysEx (`F0 7F 7F 01 01 hh mm ss ff F7`).
    #[must_use]
    pub fn full_frame(&self) -> Message {
        Message::SysEx(vec![
            0x7F,
            0x7F,
            0x01,
            0x01,
            self.hours | (self.rate.code() << 5),
            self.minutes,
            self.seconds,
            self.frames,
        ])
    }

    fn advance(mut self, frames: u8) -> Self {
        let mut s = self.to_seconds();
        if self.rate == MtcRate::Fps2997Drop {
            s += f64::from(frames) * 1001.0 / 30_000.0;
        } else {
            s += f64::from(frames) / f64::from(self.rate.nominal());
        }
        let r = self.rate;
        self = Self::from_seconds(s + 1e-9, r);
        self
    }
}

/// Assembles incoming MTC into time codes.
#[derive(Debug, Clone, Default)]
pub struct MtcReader {
    nib: [u8; 8],
    seen: u8,
}

impl MtcReader {
    /// Feed a message; returns a position when a full frame arrives or the
    /// eighth quarter frame completes a set. Quarter-frame sets describe the
    /// time two frames ago (the set spans two frames), so the reported time
    /// is advanced by two frames, as receivers do.
    pub fn feed(&mut self, m: &Message) -> Option<TimeCode> {
        match m {
            Message::QuarterFrame(v) => {
                let piece = usize::from(v >> 4) & 7;
                self.nib[piece] = v & 0xF;
                self.seen |= 1 << piece;
                if piece == 7 && self.seen == 0xFF {
                    self.seen = 0;
                    let hr = self.nib[6] | (self.nib[7] << 4);
                    let tc = TimeCode {
                        frames: self.nib[0] | (self.nib[1] << 4),
                        seconds: self.nib[2] | (self.nib[3] << 4),
                        minutes: self.nib[4] | (self.nib[5] << 4),
                        hours: hr & 0x1F,
                        rate: MtcRate::from_code(hr >> 5),
                    };
                    return Some(tc.advance(2));
                }
                None
            }
            Message::SysEx(d) if d.len() == 8 && d[..4] == [0x7F, 0x7F, 0x01, 0x01] => {
                Some(TimeCode {
                    hours: d[4] & 0x1F,
                    minutes: d[5],
                    seconds: d[6],
                    frames: d[7],
                    rate: MtcRate::from_code(d[4] >> 5),
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip_through_the_parser_with_running_status() {
        let msgs = [
            Message::NoteOn {
                channel: 2,
                key: 60,
                velocity: 100,
            },
            Message::ControlChange {
                channel: 0,
                controller: 7,
                value: 99,
            },
            Message::PitchBend {
                channel: 1,
                value: -4000,
            },
            Message::SysEx(vec![0x7e, 0x01]),
            Message::ProgramChange {
                channel: 9,
                program: 5,
            },
        ];
        let bytes: Vec<u8> = msgs.iter().flat_map(Message::to_bytes).collect();
        assert_eq!(Parser::new().feed(&bytes), msgs);
        // Running status + an interleaved clock byte.
        let got = Parser::new().feed(&[0x90, 60, 100, 0xf8, 64, 90, 67, 0]);
        assert_eq!(got.len(), 4);
        assert_eq!(got[1], Message::TimingClock);
        assert_eq!(
            got[3].clone().normalized(),
            Message::NoteOff {
                channel: 0,
                key: 67,
                velocity: 64
            }
        );
    }

    #[test]
    fn state_tracks_held_notes_cc_and_bend() {
        let mut s = MidiState::default();
        for m in Parser::new().feed(&[
            0x90, 60, 100, 0x90, 64, 80, 0x80, 60, 0, 0xb0, 1, 127, 0xe0, 0, 0x60,
        ]) {
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
            (
                0,
                TrackEvent::Midi(Message::NoteOn {
                    channel: 0,
                    key: 60,
                    velocity: 90,
                }),
            ),
            (
                480,
                TrackEvent::Midi(Message::NoteOff {
                    channel: 0,
                    key: 60,
                    velocity: 0,
                }),
            ),
            (960, TrackEvent::Tempo(250_000)),
            (
                960,
                TrackEvent::Midi(Message::NoteOn {
                    channel: 0,
                    key: 67,
                    velocity: 70,
                }),
            ),
            (
                1440,
                TrackEvent::Midi(Message::NoteOn {
                    channel: 0,
                    key: 67,
                    velocity: 0,
                }),
            ),
        ]);
        let bytes = f.to_bytes();
        let back = Smf::parse(&bytes).unwrap();
        assert_eq!(back, f);
        assert!((back.tick_to_seconds(960) - 1.0).abs() < 1e-9);
        assert!(
            (back.tick_to_seconds(1440) - 1.25).abs() < 1e-9,
            "doubled tempo after 1 s"
        );
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
    fn system_common_messages_parse() {
        let mut p = Parser::new();
        let got = p.feed(&[0xF1, 0x23, 0xF3, 0x05, 0xF6, 0x90, 60, 100]);
        assert_eq!(
            got,
            vec![
                Message::QuarterFrame(0x23),
                Message::SongSelect(5),
                Message::TuneRequest,
                Message::NoteOn {
                    channel: 0,
                    key: 60,
                    velocity: 100
                }
            ]
        );
        for m in &got {
            assert_eq!(Parser::new().feed(&m.to_bytes()), vec![m.clone()]);
        }
    }

    #[test]
    fn type_2_patterns_keep_their_own_tempo() {
        let mut smf = Smf::new(480);
        smf.format = 2;
        let note = |t: u64, on: bool| {
            let m = if on {
                Message::NoteOn {
                    channel: 0,
                    key: 60,
                    velocity: 90,
                }
            } else {
                Message::NoteOff {
                    channel: 0,
                    key: 60,
                    velocity: 0,
                }
            };
            (t, TrackEvent::Midi(m))
        };
        smf.tracks.push(vec![
            (0, TrackEvent::Tempo(500_000)),
            note(480, true),
            note(960, false),
        ]);
        smf.tracks.push(vec![
            (0, TrackEvent::Tempo(1_000_000)),
            note(480, true),
            note(960, false),
        ]);
        let back = Smf::parse(&smf.to_bytes()).unwrap();
        assert_eq!(back.format, 2);
        let n = back.notes();
        assert_eq!(n.len(), 2);
        // Pattern 0 at 120 BPM: starts at 0.5 s; pattern 1 at 60: at 1.0 s.
        assert!(
            (n[0].0 - 0.5).abs() < 1e-9 && (n[1].0 - 1.0).abs() < 1e-9,
            "{n:?}"
        );
        assert_eq!(back.pattern(1).unwrap().tracks.len(), 1);
    }

    #[test]
    fn smpte_division_is_timed_by_frames() {
        let mut smf = Smf::new(0);
        smf.division = 0xE728; // −25 fps, 40 ticks per frame
        assert_eq!(smf.smpte(), Some((25.0, 40)));
        assert!((smf.tick_to_seconds(1000) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn clips_export_and_read_back() {
        let notes = [
            ClipNote {
                start: 0.0,
                length: 1.0,
                channel: 0,
                key: 60,
                velocity: 100,
            },
            ClipNote {
                start: 1.0,
                length: 0.5,
                channel: 0,
                key: 60,
                velocity: 80,
            },
            ClipNote {
                start: 1.5,
                length: 2.0,
                channel: 1,
                key: 67,
                velocity: 70,
            },
        ];
        let smf = Smf::from_notes(&notes, 960, 90.0);
        let back = Smf::parse(&smf.to_bytes()).unwrap();
        let got = back.notes();
        assert_eq!(got.len(), 3, "a repeated note retriggers: {got:?}");
        let beat = 60.0 / 90.0;
        assert!((got[1].0 - beat).abs() < 1e-6 && (got[1].1 - 0.5 * beat).abs() < 1e-6);
        assert_eq!((got[2].2, got[2].3), (1, 67));
    }

    #[test]
    fn clock_in_follows_tempo_and_position() {
        let mut c = ClockIn::default();
        c.feed(&Message::Start, 0.0);
        let dt = 60.0 / (128.0 * 24.0);
        for i in 0..96 {
            c.feed(&Message::TimingClock, f64::from(i) * dt);
        }
        assert!((c.bpm().unwrap() - 128.0).abs() < 0.01);
        assert!((c.beat() - 4.0).abs() < 1e-9);
        c.feed(&Message::SongPosition(16), 5.0);
        assert!((c.beat() - 4.0).abs() < 1e-9, "16 sixteenths = 4 beats");
        c.feed(&Message::Stop, 5.0);
        assert!(!c.playing);
    }

    #[test]
    fn time_code_round_trips_and_drop_frame_skips() {
        for rate in [
            MtcRate::Fps24,
            MtcRate::Fps25,
            MtcRate::Fps30,
            MtcRate::Fps2997Drop,
        ] {
            for s in [0.0, 1.5, 59.9, 600.0, 3_725.25] {
                let tc = TimeCode::from_seconds(s, rate);
                let back = tc.to_seconds();
                let frame = 1.0 / f64::from(rate.nominal()) * 1.002;
                assert!(
                    back <= s + 1e-9 && s - back < frame,
                    "{rate:?} {s}: {tc:?} -> {back}"
                );
            }
        }
        // Drop-frame: frame 1799 is 00:00:59;29 and frame 1800 is
        // 00:01:00;02 (frames ;00 and ;01 are skipped at the minute).
        let at = |frame: f64| frame * 1001.0 / 30_000.0;
        let a = TimeCode::from_seconds(at(1799.5), MtcRate::Fps2997Drop);
        let b = TimeCode::from_seconds(at(1800.5), MtcRate::Fps2997Drop);
        assert_eq!((a.seconds, a.frames), (59, 29));
        assert_eq!((b.minutes, b.seconds, b.frames), (1, 0, 2));
    }

    #[test]
    fn quarter_frames_and_full_frames_decode() {
        let tc = TimeCode {
            hours: 1,
            minutes: 2,
            seconds: 3,
            frames: 4,
            rate: MtcRate::Fps25,
        };
        let mut r = MtcReader::default();
        let mut got = None;
        for m in tc.quarter_frames() {
            got = r.feed(&m).or(got);
        }
        let g = got.unwrap();
        assert_eq!(
            (g.hours, g.minutes, g.seconds, g.frames),
            (1, 2, 3, 6),
            "plus two frames"
        );
        assert_eq!(r.feed(&tc.full_frame()), Some(tc));
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
