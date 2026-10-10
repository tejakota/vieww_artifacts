//! SFZ — the open, text-based sample-instrument format.
//!
//! An SFZ file is a list of headers (`<control>`, `<global>`, `<master>`,
//! `<group>`, `<region>`) each followed by `opcode=value` pairs; a region
//! inherits every opcode set above it and overrides what it repeats. This
//! parser implements the opcodes that decide *which sample plays and how*:
//! mapping (`key`, `lokey`, `hikey`, `lovel`, `hivel`,
//! `pitch_keycenter`), tuning (`tune`, `transpose`, `pitch_keytrack`), level
//! and placement (`volume`, `pan`, `amp_veltrack`), the sample window
//! (`offset`, `end`), loops (`loop_mode`, `loop_start`, `loop_end`,
//! `loop_type`, `loop_crossfade`), round robin (`seq_length`,
//! `seq_position`), keyswitches (`sw_lokey`, `sw_hikey`, `sw_last`,
//! `sw_default`), release triggers (`trigger`), choke groups (`group`,
//! `off_by`), the amplitude envelope (`ampeg_*`), and `<control>`'s
//! `default_path`, `note_offset` and `octave_offset`. Unknown opcodes are
//! collected, not silently dropped, so a caller can see what was ignored.
//!
//! Loading audio is the caller's: [`parse`] takes a loader closure from a
//! sample path to [`SampleData`], so the parser does no I/O and tests need
//! no files; [`load`] wires it to the file system and the crate's WAV
//! reader.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::envelope::Adsr;
use super::sampler::{Instrument, LoopMode, SampleData, Trigger, Zone};

/// A parse failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SfzError {
    /// A region names a sample the loader could not supply.
    MissingSample(String),
    /// A value could not be read for an opcode.
    BadValue {
        /// The opcode.
        opcode: String,
        /// Its value.
        value: String,
    },
}

impl std::fmt::Display for SfzError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSample(s) => write!(f, "sample not found: {s}"),
            Self::BadValue { opcode, value } => write!(f, "bad value for {opcode}: {value}"),
        }
    }
}

impl std::error::Error for SfzError {}

/// The parsed instrument plus the opcodes it did not understand.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    /// The instrument.
    pub instrument: Instrument,
    /// Opcode names that were present but not implemented.
    pub ignored: Vec<String>,
}

/// Parse a note name (`c4` = 60, `c#4`, `db4`) or a number.
#[must_use]
pub fn note(s: &str) -> Option<i32> {
    if let Ok(n) = s.parse::<i32>() {
        return Some(n);
    }
    let s = s.to_ascii_lowercase();
    let mut chars = s.chars();
    let base = match chars.next()? {
        'c' => 0,
        'd' => 2,
        'e' => 4,
        'f' => 5,
        'g' => 7,
        'a' => 9,
        'b' => 11,
        _ => return None,
    };
    let rest: String = chars.collect();
    let (acc, oct) = if let Some(r) = rest.strip_prefix('#') {
        (1, r)
    } else if let Some(r) = rest.strip_prefix('b') {
        (-1, r)
    } else {
        (0, rest.as_str())
    };
    let o: i32 = oct.parse().ok()?;
    Some((o + 1) * 12 + base + acc)
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'/') {
            for d in chars.by_ref() {
                if d == '\n' {
                    out.push('\n');
                    break;
                }
            }
        } else if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            let mut prev = ' ';
            for d in chars.by_ref() {
                if prev == '*' && d == '/' {
                    break;
                }
                prev = d;
            }
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

/// Split into `(header, opcodes)` sections.
fn sections(text: &str) -> Vec<(String, Vec<(String, String)>)> {
    let text = strip_comments(text);
    let mut out: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let end = text[i..].find('>').map_or(bytes.len(), |e| i + e);
            out.push((text[i + 1..end].trim().to_string(), Vec::new()));
            i = end + 1;
            continue;
        }
        if bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' {
            // An opcode: name '=' value, value running to the next opcode
            // start (whitespace, word, '=') or header or end of line.
            let Some(eq) = text[i..].find('=') else { break };
            let name = text[i..i + eq].trim().to_string();
            let vstart = i + eq + 1;
            let mut j = vstart;
            let mut vend = bytes.len();
            while j < bytes.len() {
                if bytes[j] == b'<' || bytes[j] == b'\n' || bytes[j] == b'\r' {
                    vend = j;
                    break;
                }
                if bytes[j].is_ascii_whitespace() {
                    // Look ahead: a word followed by '=' starts a new opcode.
                    let k = j + text[j..].len() - text[j..].trim_start().len();
                    let word_end = text[k..]
                        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                        .map_or(bytes.len(), |e| k + e);
                    if word_end > k && word_end < bytes.len() && bytes[word_end] == b'=' {
                        vend = j;
                        break;
                    }
                }
                j += 1;
            }
            let value = text[vstart..vend].trim().to_string();
            if let Some(last) = out.last_mut() {
                last.1.push((name, value));
            }
            i = vend;
            continue;
        }
        i += 1;
    }
    out
}

const KNOWN: &[&str] = &[
    "sample",
    "lokey",
    "hikey",
    "key",
    "lovel",
    "hivel",
    "pitch_keycenter",
    "tune",
    "transpose",
    "pitch_keytrack",
    "volume",
    "pan",
    "amp_veltrack",
    "offset",
    "end",
    "loop_mode",
    "loopmode",
    "loop_start",
    "loopstart",
    "loop_end",
    "loopend",
    "loop_type",
    "loop_crossfade",
    "seq_length",
    "seq_position",
    "sw_lokey",
    "sw_hikey",
    "sw_last",
    "sw_default",
    "trigger",
    "group",
    "off_by",
    "ampeg_attack",
    "ampeg_decay",
    "ampeg_sustain",
    "ampeg_release",
    "default_path",
    "note_offset",
    "octave_offset",
];

/// Parse SFZ text, loading samples through `loader` (given the path as
/// written, joined to `default_path`).
///
/// # Errors
/// A missing sample or an unreadable value.
pub fn parse(
    text: &str,
    mut loader: impl FnMut(&str) -> Option<Arc<SampleData>>,
) -> Result<Parsed, SfzError> {
    let mut control: BTreeMap<String, String> = BTreeMap::new();
    let mut global: BTreeMap<String, String> = BTreeMap::new();
    let mut master: BTreeMap<String, String> = BTreeMap::new();
    let mut group: BTreeMap<String, String> = BTreeMap::new();
    let mut inst = Instrument::default();
    let mut ignored = std::collections::BTreeSet::new();
    let mut cache: BTreeMap<String, Arc<SampleData>> = BTreeMap::new();
    let mut sw_range: Option<(u8, u8)> = None;

    for (header, ops) in sections(text) {
        for (k, _) in &ops {
            if !KNOWN.contains(&k.as_str()) {
                ignored.insert(k.clone());
            }
        }
        let map: BTreeMap<String, String> = ops.into_iter().collect();
        match header.as_str() {
            "control" => control.extend(map),
            "global" => {
                global = map;
                master.clear();
                group.clear();
            }
            "master" => {
                master = map;
                group.clear();
            }
            "group" => group = map,
            "region" => {
                let mut o = global.clone();
                o.extend(master.clone());
                o.extend(group.clone());
                o.extend(map);
                let zone = region(&o, &control, &mut loader, &mut cache)?;
                if let Some(lo) = o.get("sw_lokey").and_then(|v| note(v)) {
                    let hi = o.get("sw_hikey").and_then(|v| note(v)).unwrap_or(lo);
                    let (lo, hi) = (clamp_key(lo), clamp_key(hi));
                    sw_range = Some(sw_range.map_or((lo, hi), |(a, b)| (a.min(lo), b.max(hi))));
                }
                if let Some(d) = o.get("sw_default").and_then(|v| note(v)) {
                    inst.default_keyswitch = Some(clamp_key(d));
                }
                inst.zones.push(zone);
            }
            _ => {}
        }
    }
    inst.keyswitch_range = sw_range;
    Ok(Parsed {
        instrument: inst,
        ignored: ignored.into_iter().collect(),
    })
}

fn clamp_key(k: i32) -> u8 {
    u8::try_from(k.clamp(0, 127)).unwrap_or(0)
}

fn region(
    o: &BTreeMap<String, String>,
    control: &BTreeMap<String, String>,
    loader: &mut impl FnMut(&str) -> Option<Arc<SampleData>>,
    cache: &mut BTreeMap<String, Arc<SampleData>>,
) -> Result<Zone, SfzError> {
    let f = |k: &str| -> Result<Option<f32>, SfzError> {
        o.get(k).map_or(Ok(None), |v| {
            v.parse::<f32>().map(Some).map_err(|_| SfzError::BadValue {
                opcode: k.into(),
                value: v.clone(),
            })
        })
    };
    let n = |k: &str| -> Result<Option<i32>, SfzError> {
        o.get(k).map_or(Ok(None), |v| {
            note(v).map(Some).ok_or_else(|| SfzError::BadValue {
                opcode: k.into(),
                value: v.clone(),
            })
        })
    };
    let us = |k: &str| -> Result<Option<usize>, SfzError> {
        o.get(k).map_or(Ok(None), |v| {
            v.parse::<usize>()
                .map(Some)
                .map_err(|_| SfzError::BadValue {
                    opcode: k.into(),
                    value: v.clone(),
                })
        })
    };
    let shift = control
        .get("note_offset")
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(0)
        + 12 * control
            .get("octave_offset")
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(0);
    let key = |v: i32| clamp_key(v + shift);

    let name = o.get("sample").cloned().unwrap_or_default();
    let path = match control.get("default_path") {
        Some(dp) => format!("{}{}", dp.replace('\\', "/"), name.replace('\\', "/")),
        None => name.replace('\\', "/"),
    };
    let sample = if let Some(s) = cache.get(&path) {
        s.clone()
    } else {
        let s = loader(&path).ok_or_else(|| SfzError::MissingSample(path.clone()))?;
        cache.insert(path.clone(), s.clone());
        s
    };
    let mut z = Zone::new(sample, 60.0);
    if let Some(k) = n("key")? {
        z.lo_key = key(k);
        z.hi_key = key(k);
        z.root = f32::from(key(k));
    }
    if let Some(k) = n("lokey")? {
        z.lo_key = key(k);
    }
    if let Some(k) = n("hikey")? {
        z.hi_key = key(k);
    }
    if let Some(k) = n("pitch_keycenter")? {
        z.root = f32::from(key(k));
    }
    if let Some(v) = n("lovel")? {
        z.lo_vel = clamp_key(v).max(1);
    }
    if let Some(v) = n("hivel")? {
        z.hi_vel = clamp_key(v);
    }
    z.tune = f("tune")?.unwrap_or(0.0) + 100.0 * f("transpose")?.unwrap_or(0.0);
    z.keytrack = f("pitch_keytrack")?.unwrap_or(100.0);
    z.volume = f("volume")?.unwrap_or(0.0);
    z.pan = f("pan")?.unwrap_or(0.0) / 100.0;
    z.vel_track = f("amp_veltrack")?.unwrap_or(100.0) / 100.0;
    z.offset = us("offset")?.unwrap_or(0);
    if let Some(e) = us("end")? {
        z.end = e + 1;
    }
    let mode = o
        .get("loop_mode")
        .or_else(|| o.get("loopmode"))
        .map(String::as_str);
    z.loop_mode = match mode {
        Some("one_shot") => LoopMode::OneShot,
        Some("loop_continuous") => LoopMode::Continuous,
        Some("loop_sustain") => LoopMode::Sustain,
        _ => LoopMode::None,
    };
    z.loop_start = us("loop_start")?.or(us("loopstart")?).unwrap_or(0);
    z.loop_end = us("loop_end")?
        .or(us("loopend")?)
        .map_or(z.sample.frames(), |e| e + 1);
    z.ping_pong = o.get("loop_type").is_some_and(|t| t == "alternate");
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "frame counts"
    )]
    {
        z.crossfade = (f("loop_crossfade")?.unwrap_or(0.0).max(0.0) * z.sample.rate) as usize;
    }
    if let Some(l) = us("seq_length")? {
        z.seq_length = u32::try_from(l.max(1)).unwrap_or(1);
    }
    if let Some(p) = us("seq_position")? {
        z.seq_position = u32::try_from(p.max(1)).unwrap_or(1);
    }
    if let Some(k) = n("sw_last")? {
        z.keyswitch = Some(clamp_key(k));
    }
    if o.get("trigger").is_some_and(|t| t == "release") {
        z.trigger = Trigger::Release;
    }
    if let Some(g) = us("group")? {
        z.group = u32::try_from(g).unwrap_or(0);
    }
    if let Some(g) = us("off_by")? {
        z.off_by = u32::try_from(g).unwrap_or(0);
    }
    z.amp = Adsr::new(
        f("ampeg_attack")?.unwrap_or(0.001),
        f("ampeg_decay")?.unwrap_or(0.0),
        f("ampeg_sustain")?.unwrap_or(100.0) / 100.0,
        f("ampeg_release")?.unwrap_or(0.001).max(0.001),
    );
    Ok(z)
}

/// Load an SFZ file from disk, reading its WAV samples relative to the
/// file's directory.
///
/// # Errors
/// I/O errors become [`SfzError::MissingSample`] for samples; an unreadable
/// SFZ file is an `io::Error`.
pub fn load(path: &std::path::Path) -> std::io::Result<Result<Parsed, SfzError>> {
    let text = std::fs::read_to_string(path)?;
    let dir = path
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    Ok(parse(&text, |p| {
        let bytes = std::fs::read(dir.join(p)).ok()?;
        let pcm = crate::wav::read_wav(&bytes).ok()?;
        #[expect(clippy::cast_precision_loss, reason = "sample rates")]
        let rate = pcm.samples.rate as f32;
        Some(Arc::new(SampleData {
            channels: usize::from(pcm.samples.channels),
            data: pcm.samples.data,
            rate,
        }))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(p: &str) -> Option<Arc<SampleData>> {
        let len = if p.contains("long") { 4_800 } else { 480 };
        Some(Arc::new(SampleData::mono(vec![0.5; len], 48_000.0)))
    }

    #[test]
    fn note_names() {
        assert_eq!(note("c4"), Some(60));
        assert_eq!(note("C#4"), Some(61));
        assert_eq!(note("db4"), Some(61));
        assert_eq!(note("a-1"), Some(9));
        assert_eq!(note("127"), Some(127));
        assert_eq!(note("h4"), None);
    }

    #[test]
    fn inheritance_overrides_and_paths_with_spaces() {
        let text = r"
            // a piano
            <control> default_path=Samples/Grand Piano/
            <global> ampeg_release=0.4 volume=-3
            <group> lovel=1 hivel=64 amp_veltrack=50
            <region> sample=C4 soft long.wav key=c4 tune=-5
            <region> sample=D4 soft.wav lokey=d4 hikey=e4 pitch_keycenter=d4 volume=-6
            <group> lovel=65 hivel=127
            /* block
               comment */
            <region> sample=C4 loud.wav key=60 pan=-50 loop_mode=loop_sustain loop_start=100 loop_end=399
        ";
        let mut asked = Vec::new();
        let p = parse(text, |s| {
            asked.push(s.to_string());
            loader(s)
        })
        .unwrap();
        let z = &p.instrument.zones;
        assert_eq!(z.len(), 3);
        assert_eq!(asked[0], "Samples/Grand Piano/C4 soft long.wav");
        assert_eq!((z[0].lo_key, z[0].hi_key, z[0].root), (60, 60, 60.0));
        assert_eq!((z[0].lo_vel, z[0].hi_vel), (1, 64));
        assert!((z[0].tune + 5.0).abs() < 1e-6 && (z[0].volume + 3.0).abs() < 1e-6);
        assert!((z[0].vel_track - 0.5).abs() < 1e-6);
        assert!((z[0].amp.release - 0.4).abs() < 1e-6, "global inherited");
        assert!((z[1].volume + 6.0).abs() < 1e-6, "region overrides global");
        assert_eq!((z[1].lo_key, z[1].hi_key, z[1].root), (62, 64, 62.0));
        assert_eq!((z[2].lo_vel, z[2].hi_vel), (65, 127), "new group resets");
        assert!(
            (z[2].vel_track - 1.0).abs() < 1e-6,
            "group opcodes do not leak"
        );
        assert_eq!(z[2].loop_mode, LoopMode::Sustain);
        assert_eq!(
            (z[2].loop_start, z[2].loop_end),
            (100, 400),
            "inclusive end"
        );
        assert!((z[2].pan + 0.5).abs() < 1e-6);
    }

    #[test]
    fn round_robin_keyswitch_release_and_choke() {
        let text = "
            <global> sw_lokey=c1 sw_hikey=d1 sw_default=c1
            <group> sw_last=c1 seq_length=2
            <region> sample=a.wav seq_position=1
            <region> sample=b.wav seq_position=2
            <group> sw_last=d1
            <region> sample=c.wav trigger=release group=3 off_by=3
        ";
        let p = parse(text, loader).unwrap();
        let i = &p.instrument;
        assert_eq!(i.keyswitch_range, Some((24, 26)));
        assert_eq!(i.default_keyswitch, Some(24));
        assert_eq!(i.zones[0].seq_length, 2);
        assert_eq!(i.zones[1].seq_position, 2);
        assert_eq!(i.zones[2].trigger, Trigger::Release);
        assert_eq!((i.zones[2].group, i.zones[2].off_by), (3, 3));
        assert_eq!(i.zones[2].keyswitch, Some(26));
    }

    #[test]
    fn unknown_opcodes_are_reported_and_missing_samples_fail() {
        let p = parse("<region> sample=a.wav fil_type=lpf_2p cutoff=500", loader).unwrap();
        assert_eq!(
            p.ignored,
            vec!["cutoff".to_string(), "fil_type".to_string()]
        );
        assert_eq!(
            parse("<region> sample=x.wav", |_| None),
            Err(SfzError::MissingSample("x.wav".into()))
        );
        assert!(matches!(
            parse("<region> sample=a.wav volume=loud", loader),
            Err(SfzError::BadValue { .. })
        ));
    }

    #[test]
    fn parsed_instrument_plays() {
        let text = "<region> sample=long.wav key=60 ampeg_release=0.01";
        let p = parse(text, loader).unwrap();
        let mut s = super::super::sampler::Sampler::new(p.instrument, 4, 48_000.0);
        s.note_on(0, 60, 127);
        let x = s.render(1_000);
        assert!(x.iter().any(|v| v.abs() > 0.3));
        s.note_on(0, 61, 127);
        assert_eq!(s.active(), 1, "61 is outside the region");
    }

    #[test]
    fn control_offsets_shift_keys() {
        let p = parse(
            "<control> octave_offset=1 note_offset=-2 <region> sample=a.wav key=60",
            loader,
        )
        .unwrap();
        assert_eq!(p.instrument.zones[0].lo_key, 70);
    }
}
