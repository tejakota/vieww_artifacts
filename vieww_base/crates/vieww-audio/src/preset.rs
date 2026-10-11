//! Presets and plugin-style state.
//!
//! The shape is the one CLAP and VST3 settled on. A processor describes its
//! parameters once ([`ParamInfo`]: a stable numeric id, a range, a default,
//! a [`Taper`] mapping the range onto a host's normalised 0..1, a unit and
//! text conversion), exposes them through [`Parametric`], and everything
//! else — saving, loading, comparing, undoing, morphing — is generic over
//! that trait.
//!
//! A [`State`] is a preset: a name, a version, parameter values by id, and
//! named binary blobs for whatever is not a number (a loaded sample's path,
//! a drawn wavetable). It round-trips through two formats, both written
//! here:
//!
//! * a **binary** chunk format (`VWST`, version, tagged chunks, CRC-32) for
//!   a host's project file — the analogue of a plugin's state chunk;
//! * a **text** format (`key = value` lines, blobs in base64) that diffs
//!   and reviews well, for preset libraries in source control.
//!
//! Loading is forward and backward compatible: ids a processor no longer
//! has are ignored, ids missing from an old preset take their defaults, and
//! every value is clamped to the current range.
//!
//! Around that: [`Bank`] (categories, tags, search, stepping), [`AbCompare`],
//! [`History`] (undo/redo with drag gestures coalesced into one step) and
//! [`morph`] (interpolation in normalised space, stepped parameters
//! switching at the midpoint).

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// How a parameter's range maps onto normalised 0..1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Taper {
    /// Linear.
    Linear,
    /// Logarithmic (frequencies, times): requires `min > 0`.
    Log,
    /// `n = ((v − min) / (max − min))^(1/exp)`: `exp > 1` gives the low end
    /// more travel.
    Power(f64),
    /// Integer steps from `min` to `max` (choices, switches).
    Stepped,
}

/// A parameter's description.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamInfo {
    /// Stable id: what presets and automation store. Never reuse one.
    pub id: u32,
    /// Display name.
    pub name: String,
    /// Minimum.
    pub min: f64,
    /// Maximum.
    pub max: f64,
    /// Default.
    pub default: f64,
    /// Taper.
    pub taper: Taper,
    /// Unit shown after the value ("dB", "Hz", "%", "ms" or "").
    pub unit: &'static str,
    /// Labels for a stepped parameter's values.
    pub choices: Vec<String>,
    /// Whether a host may automate it.
    pub automatable: bool,
}

impl ParamInfo {
    /// A linear parameter.
    #[must_use]
    pub fn new(id: u32, name: &str, min: f64, max: f64, default: f64) -> Self {
        Self {
            id,
            name: name.to_owned(),
            min,
            max,
            default: default.clamp(min, max),
            taper: Taper::Linear,
            unit: "",
            choices: Vec::new(),
            automatable: true,
        }
    }

    /// A choice among labels (stored as 0..n−1).
    #[must_use]
    pub fn choice(id: u32, name: &str, choices: &[&str], default: usize) -> Self {
        #[expect(clippy::cast_precision_loss, reason = "choice counts")]
        let mut p = Self::new(
            id,
            name,
            0.0,
            choices.len().saturating_sub(1) as f64,
            default as f64,
        );
        p.taper = Taper::Stepped;
        p.choices = choices.iter().map(|c| (*c).to_owned()).collect();
        p
    }

    /// An on/off switch.
    #[must_use]
    pub fn toggle(id: u32, name: &str, default: bool) -> Self {
        Self::choice(id, name, &["Off", "On"], usize::from(default))
    }

    /// With a taper.
    #[must_use]
    pub const fn taper(mut self, taper: Taper) -> Self {
        self.taper = taper;
        self
    }

    /// With a unit.
    #[must_use]
    pub const fn unit(mut self, unit: &'static str) -> Self {
        self.unit = unit;
        self
    }

    /// Clamp (and for stepped parameters, round) a value into range.
    #[must_use]
    pub fn clamp(&self, v: f64) -> f64 {
        let v = if v.is_nan() { self.default } else { v };
        let v = v.clamp(self.min, self.max);
        if self.taper == Taper::Stepped {
            v.round()
        } else {
            v
        }
    }

    /// Plain value → normalised 0..1.
    #[must_use]
    pub fn normalize(&self, v: f64) -> f64 {
        let v = self.clamp(v);
        let span = self.max - self.min;
        if span <= 0.0 {
            return 0.0;
        }
        match self.taper {
            Taper::Linear | Taper::Stepped => (v - self.min) / span,
            Taper::Log => (v / self.min).ln() / (self.max / self.min).ln(),
            Taper::Power(e) => ((v - self.min) / span).powf(1.0 / e),
        }
    }

    /// Normalised 0..1 → plain value.
    #[must_use]
    pub fn denormalize(&self, n: f64) -> f64 {
        let n = n.clamp(0.0, 1.0);
        let span = self.max - self.min;
        self.clamp(match self.taper {
            Taper::Linear | Taper::Stepped => self.min + n * span,
            Taper::Log => self.min * (self.max / self.min).powf(n),
            Taper::Power(e) => self.min + n.powf(e) * span,
        })
    }

    /// The value as text, as a host shows it.
    #[must_use]
    pub fn display(&self, v: f64) -> String {
        let v = self.clamp(v);
        if !self.choices.is_empty() {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "stepped index"
            )]
            let i = v as usize;
            return self.choices.get(i).cloned().unwrap_or_default();
        }
        let digits = match v.abs() {
            a if a >= 1_000.0 => 0,
            a if a >= 100.0 => 1,
            a if a >= 10.0 => 2,
            _ => 3,
        };
        let mut s = format!("{v:.digits$}");
        if !self.unit.is_empty() {
            s.push(' ');
            s.push_str(self.unit);
        }
        s
    }

    /// Text → value (a choice label, or a number with or without the unit).
    #[must_use]
    pub fn parse(&self, text: &str) -> Option<f64> {
        let t = text.trim();
        if let Some(i) = self.choices.iter().position(|c| c.eq_ignore_ascii_case(t)) {
            #[expect(clippy::cast_precision_loss, reason = "choice index")]
            return Some(i as f64);
        }
        let t = t.strip_suffix(self.unit).unwrap_or(t).trim();
        t.parse::<f64>().ok().map(|v| self.clamp(v))
    }
}

/// Something with parameters.
pub trait Parametric {
    /// The parameters, in display order.
    fn param_info(&self) -> Vec<ParamInfo>;
    /// A parameter's current value (plain units).
    fn get_param(&self, id: u32) -> Option<f64>;
    /// Set a parameter (plain units; the caller has clamped it).
    fn set_param(&mut self, id: u32, value: f64);
}

/// A saved state: a preset, or a plugin's state chunk.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct State {
    /// Preset name.
    pub name: String,
    /// The processor's state version (for migrations).
    pub version: u32,
    /// Parameter values by id.
    pub params: BTreeMap<u32, f64>,
    /// Non-numeric state by key.
    pub blobs: BTreeMap<String, Vec<u8>>,
}

/// A state that could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    /// Not this format.
    NotState,
    /// Ends early.
    Truncated,
    /// The checksum does not match.
    Checksum,
    /// A line or chunk that makes no sense.
    Malformed(String),
}

impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotState => f.write_str("not a vieww state"),
            Self::Truncated => f.write_str("state is truncated"),
            Self::Checksum => f.write_str("state checksum mismatch"),
            Self::Malformed(m) => write!(f, "malformed state: {m}"),
        }
    }
}

impl std::error::Error for StateError {}

/// CRC-32 (IEEE 802.3, reflected) — the one zip and PNG use.
#[must_use]
pub fn crc32(data: &[u8]) -> u32 {
    static T: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let t = T.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut c = u32::try_from(i).unwrap_or(0);
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            *e = c;
        }
        t
    });
    !data
        .iter()
        .fold(!0u32, |c, b| t[usize::from((c as u8) ^ b)] ^ (c >> 8))
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 (RFC 4648, padded).
#[must_use]
pub fn base64_encode(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for k in 0..4 {
            if k <= c.len() {
                s.push(char::from(B64[((n >> (18 - 6 * k)) & 63) as usize]));
            } else {
                s.push('=');
            }
        }
    }
    s
}

/// Decode base64 (padding optional, whitespace ignored).
#[must_use]
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0;
    for ch in text.bytes() {
        if ch == b'=' || ch.is_ascii_whitespace() {
            continue;
        }
        let v = u32::try_from(B64.iter().position(|b| *b == ch)?).ok()?;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

impl State {
    /// Capture a processor's state.
    #[must_use]
    pub fn capture(p: &impl Parametric, name: &str) -> Self {
        Self {
            name: name.to_owned(),
            version: 1,
            params: p
                .param_info()
                .iter()
                .filter_map(|i| p.get_param(i.id).map(|v| (i.id, v)))
                .collect(),
            blobs: BTreeMap::new(),
        }
    }

    /// Apply to a processor: unknown ids ignored, missing ids defaulted,
    /// values clamped.
    pub fn apply(&self, p: &mut impl Parametric) {
        for info in p.param_info() {
            let v = self.params.get(&info.id).copied().unwrap_or(info.default);
            p.set_param(info.id, info.clamp(v));
        }
    }

    /// Encode as a binary chunk.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = b"VWST".to_vec();
        out.extend(1u16.to_le_bytes()); // format version
        out.extend(self.version.to_le_bytes());
        let mut chunk = |tag: &[u8; 4], body: &[u8]| {
            out.extend(tag);
            out.extend(u32::try_from(body.len()).unwrap_or(u32::MAX).to_le_bytes());
            out.extend(body);
        };
        chunk(b"NAME", self.name.as_bytes());
        let mut parm = Vec::with_capacity(self.params.len() * 12);
        for (id, v) in &self.params {
            parm.extend(id.to_le_bytes());
            parm.extend(v.to_le_bytes());
        }
        chunk(b"PARM", &parm);
        for (k, v) in &self.blobs {
            let mut b = u16::try_from(k.len()).unwrap_or(0).to_le_bytes().to_vec();
            b.extend(k.as_bytes());
            b.extend(v);
            chunk(b"BLOB", &b);
        }
        let crc = crc32(&out);
        out.extend(b"CRC ");
        out.extend(4u32.to_le_bytes());
        out.extend(crc.to_le_bytes());
        out
    }

    /// Decode a binary chunk. Unknown chunks are skipped.
    ///
    /// # Errors
    /// Not a state, truncated, or a checksum mismatch.
    pub fn from_bytes(b: &[u8]) -> Result<Self, StateError> {
        if b.len() < 10 || &b[..4] != b"VWST" {
            return Err(StateError::NotState);
        }
        let mut s = Self {
            version: u32::from_le_bytes([b[6], b[7], b[8], b[9]]),
            ..Self::default()
        };
        let mut at = 10;
        let mut verified = false;
        while at + 8 <= b.len() {
            let tag = &b[at..at + 4];
            let len = u32::from_le_bytes([b[at + 4], b[at + 5], b[at + 6], b[at + 7]]) as usize;
            let body = b.get(at + 8..at + 8 + len).ok_or(StateError::Truncated)?;
            match tag {
                b"NAME" => s.name = String::from_utf8_lossy(body).into_owned(),
                b"PARM" => {
                    for e in body.as_chunks::<12>().0 {
                        let id = u32::from_le_bytes([e[0], e[1], e[2], e[3]]);
                        let v =
                            f64::from_le_bytes([e[4], e[5], e[6], e[7], e[8], e[9], e[10], e[11]]);
                        s.params.insert(id, v);
                    }
                }
                b"BLOB" if body.len() >= 2 => {
                    let kl = usize::from(u16::from_le_bytes([body[0], body[1]]));
                    let key = body.get(2..2 + kl).ok_or(StateError::Truncated)?;
                    s.blobs.insert(
                        String::from_utf8_lossy(key).into_owned(),
                        body[2 + kl..].to_vec(),
                    );
                }
                b"CRC " if len == 4 => {
                    let want = u32::from_le_bytes([body[0], body[1], body[2], body[3]]);
                    if crc32(&b[..at]) != want {
                        return Err(StateError::Checksum);
                    }
                    verified = true;
                }
                _ => {}
            }
            at += 8 + len;
        }
        if verified {
            Ok(s)
        } else {
            Err(StateError::Truncated)
        }
    }

    /// Encode as text. `infos` (optional) adds each parameter's name as a
    /// comment, so a preset file is readable.
    #[must_use]
    pub fn to_text(&self, infos: &[ParamInfo]) -> String {
        let mut s = String::from("# vieww preset\n");
        let _ = writeln!(s, "name = {}", self.name.replace('\n', " "));
        let _ = writeln!(s, "version = {}", self.version);
        for (id, v) in &self.params {
            if let Some(i) = infos.iter().find(|i| i.id == *id) {
                let _ = writeln!(s, "# {}: {}", i.name, i.display(*v));
            }
            // `{:?}` prints the shortest text that parses back exactly.
            let _ = writeln!(s, "param.{id} = {v:?}");
        }
        for (k, v) in &self.blobs {
            let _ = writeln!(s, "blob.{k} = {}", base64_encode(v));
        }
        s
    }

    /// Decode text. Unknown keys are ignored (newer writers may add them).
    ///
    /// # Errors
    /// A line that is not `key = value`, or a value that does not parse.
    pub fn from_text(text: &str) -> Result<Self, StateError> {
        let mut s = Self::default();
        let mut seen_any = false;
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let bad = || StateError::Malformed(format!("line {}: {line}", n + 1));
            let (k, v) = line.split_once('=').ok_or_else(bad)?;
            let (k, v) = (k.trim(), v.trim());
            seen_any = true;
            if k == "name" {
                v.clone_into(&mut s.name);
            } else if k == "version" {
                s.version = v.parse().map_err(|_| bad())?;
            } else if let Some(id) = k.strip_prefix("param.") {
                s.params.insert(
                    id.parse().map_err(|_| bad())?,
                    v.parse().map_err(|_| bad())?,
                );
            } else if let Some(key) = k.strip_prefix("blob.") {
                s.blobs
                    .insert(key.to_owned(), base64_decode(v).ok_or_else(bad)?);
            }
        }
        if seen_any {
            Ok(s)
        } else {
            Err(StateError::NotState)
        }
    }
}

/// Interpolate two states in normalised space. Stepped parameters switch at
/// the midpoint; ids only one side has are taken from that side.
#[must_use]
pub fn morph(a: &State, b: &State, t: f64, infos: &[ParamInfo]) -> State {
    let t = t.clamp(0.0, 1.0);
    let mut out = if t < 0.5 { a.clone() } else { b.clone() };
    for info in infos {
        let va = a.params.get(&info.id).copied().unwrap_or(info.default);
        let vb = b.params.get(&info.id).copied().unwrap_or(info.default);
        let v = if info.taper == Taper::Stepped {
            if t < 0.5 {
                va
            } else {
                vb
            }
        } else {
            let n = info.normalize(va) + (info.normalize(vb) - info.normalize(va)) * t;
            info.denormalize(n)
        };
        out.params.insert(info.id, v);
    }
    out.name = format!("{} ↔ {} ({:.0}%)", a.name, b.name, t * 100.0);
    out
}

/// A preset with its library metadata.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Preset {
    /// The state.
    pub state: State,
    /// Category ("Bass", "Pad" …).
    pub category: String,
    /// Tags.
    pub tags: Vec<String>,
    /// Author.
    pub author: String,
    /// Marked as a favourite.
    pub favourite: bool,
}

/// A preset library.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Bank {
    /// The presets.
    pub presets: Vec<Preset>,
    current: Option<usize>,
}

impl Bank {
    /// An empty bank.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a preset; returns its index.
    pub fn add(&mut self, p: Preset) -> usize {
        self.presets.push(p);
        self.presets.len() - 1
    }

    /// Indices matching a query: every word must appear in the name,
    /// category, author or a tag (case-insensitive).
    #[must_use]
    pub fn search(&self, query: &str) -> Vec<usize> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        self.presets
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                let hay = format!(
                    "{} {} {} {}",
                    p.state.name,
                    p.category,
                    p.author,
                    p.tags.join(" ")
                )
                .to_lowercase();
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Indices in a category.
    #[must_use]
    pub fn in_category(&self, category: &str) -> Vec<usize> {
        (0..self.presets.len())
            .filter(|i| self.presets[*i].category.eq_ignore_ascii_case(category))
            .collect()
    }

    /// The categories present, sorted.
    #[must_use]
    pub fn categories(&self) -> Vec<String> {
        let mut c: Vec<String> = self.presets.iter().map(|p| p.category.clone()).collect();
        c.sort();
        c.dedup();
        c
    }

    /// Select a preset and apply it.
    pub fn select(&mut self, i: usize, target: &mut impl Parametric) -> Option<&Preset> {
        let p = self.presets.get(i)?;
        p.state.apply(target);
        self.current = Some(i);
        Some(p)
    }

    /// Step forward (`+1`) or back (`−1`), wrapping, and apply.
    pub fn step(&mut self, by: isize, target: &mut impl Parametric) -> Option<&Preset> {
        let n = self.presets.len();
        if n == 0 {
            return None;
        }
        let cur = self.current.map_or(0, |c| {
            let n = isize::try_from(n).unwrap_or(isize::MAX);
            let c = isize::try_from(c).unwrap_or(0);
            usize::try_from((c + by).rem_euclid(n)).unwrap_or(0)
        });
        self.select(cur, target)
    }

    /// The selected preset's index.
    #[must_use]
    pub const fn current(&self) -> Option<usize> {
        self.current
    }

    /// The whole bank as one text document (presets separated by `---`).
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut s = String::new();
        for p in &self.presets {
            s.push_str("---\n");
            let _ = writeln!(s, "category = {}", p.category);
            let _ = writeln!(s, "tags = {}", p.tags.join(", "));
            let _ = writeln!(s, "author = {}", p.author);
            let _ = writeln!(s, "favourite = {}", p.favourite);
            s.push_str(&p.state.to_text(&[]));
        }
        s
    }

    /// Read a bank written by [`to_text`](Self::to_text).
    ///
    /// # Errors
    /// A preset that does not parse.
    pub fn from_text(text: &str) -> Result<Self, StateError> {
        let mut bank = Self::new();
        for part in text.split("---\n").filter(|p| !p.trim().is_empty()) {
            let mut p = Preset {
                state: State::from_text(part)?,
                ..Preset::default()
            };
            for line in part.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    let v = v.trim();
                    match k.trim() {
                        "category" => v.clone_into(&mut p.category),
                        "author" => v.clone_into(&mut p.author),
                        "favourite" => p.favourite = v == "true",
                        "tags" => {
                            p.tags = v
                                .split(',')
                                .map(str::trim)
                                .filter(|t| !t.is_empty())
                                .map(str::to_owned)
                                .collect();
                        }
                        _ => {}
                    }
                }
            }
            bank.add(p);
        }
        Ok(bank)
    }
}

/// A/B comparison: two states, one live.
#[derive(Debug, Clone, PartialEq)]
pub struct AbCompare {
    a: State,
    b: State,
    on_b: bool,
}

impl AbCompare {
    /// Start with both slots holding the processor's current state.
    #[must_use]
    pub fn new(p: &impl Parametric) -> Self {
        let s = State::capture(p, "A");
        Self {
            a: s.clone(),
            b: State {
                name: "B".into(),
                ..s
            },
            on_b: false,
        }
    }

    /// Switch slots: the live state is stored, the other applied.
    pub fn toggle(&mut self, p: &mut impl Parametric) {
        let live = State::capture(p, if self.on_b { "B" } else { "A" });
        if self.on_b {
            self.b = live;
            self.a.apply(p);
        } else {
            self.a = live;
            self.b.apply(p);
        }
        self.on_b = !self.on_b;
    }

    /// Copy the live slot over the other.
    pub fn copy_to_other(&mut self, p: &impl Parametric) {
        let live = State::capture(p, "");
        if self.on_b {
            self.a = State {
                name: "A".into(),
                ..live
            };
        } else {
            self.b = State {
                name: "B".into(),
                ..live
            };
        }
    }

    /// Whether B is live.
    #[must_use]
    pub const fn on_b(&self) -> bool {
        self.on_b
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Change {
    id: u32,
    from: f64,
    to: f64,
}

/// Undo/redo of parameter changes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct History {
    undo: Vec<Vec<Change>>,
    redo: Vec<Vec<Change>>,
    gesture: Option<Vec<Change>>,
    limit: usize,
}

impl History {
    /// Keep at most `limit` steps.
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            ..Self::default()
        }
    }

    /// Begin a gesture (a knob drag): its changes become one step.
    pub fn begin_gesture(&mut self) {
        self.gesture.get_or_insert_with(Vec::new);
    }

    /// End the gesture.
    pub fn end_gesture(&mut self) {
        if let Some(g) = self.gesture.take() {
            self.push(g);
        }
    }

    fn push(&mut self, step: Vec<Change>) {
        if step.is_empty() {
            return;
        }
        self.undo.push(step);
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Set a parameter through the history.
    pub fn set(&mut self, p: &mut impl Parametric, id: u32, value: f64) {
        let Some(from) = p.get_param(id) else {
            return;
        };
        p.set_param(id, value);
        let c = Change {
            id,
            from,
            to: value,
        };
        if let Some(g) = &mut self.gesture {
            // Coalesce: keep the first `from` and the last `to` per id.
            if let Some(e) = g.iter_mut().find(|e| e.id == id) {
                e.to = value;
            } else {
                g.push(c);
            }
        } else {
            self.push(vec![c]);
        }
    }

    /// Undo one step; false if there is none.
    pub fn undo(&mut self, p: &mut impl Parametric) -> bool {
        let Some(step) = self.undo.pop() else {
            return false;
        };
        for c in step.iter().rev() {
            p.set_param(c.id, c.from);
        }
        self.redo.push(step);
        true
    }

    /// Redo one step; false if there is none.
    pub fn redo(&mut self, p: &mut impl Parametric) -> bool {
        let Some(step) = self.redo.pop() else {
            return false;
        };
        for c in &step {
            p.set_param(c.id, c.to);
        }
        self.undo.push(step);
        true
    }

    /// Steps available to undo and redo.
    #[must_use]
    pub fn depth(&self) -> (usize, usize) {
        (self.undo.len(), self.redo.len())
    }
}

/// The mixer as a [`Parametric`]: every channel's fader, pan, width,
/// mute, solo, solo-safe and polarity, and every VCA — so a session's mix
/// recalls through the same state machinery as a plugin.
///
/// Ids: track `t` field `f` is `t * 16 + f`, bus `b` is
/// `(4096 + b) * 16 + f`, the master `8192 * 16 + f`, VCA `v` is
/// `12288 * 16 + v`. Fields: 0 fader, 1 pan, 2 width, 3 mute, 4 solo,
/// 5 solo-safe, 6 invert L, 7 invert R.
impl Parametric for crate::mixer::Mixer {
    fn param_info(&self) -> Vec<ParamInfo> {
        use crate::mixer::Channel;
        let (nt, nb) = self.counts();
        let chans = (0..nt)
            .map(|t| (Channel::Track(t), u32::try_from(t).unwrap_or(0)))
            .chain((0..nb).map(|b| (Channel::Bus(b), 4_096 + u32::try_from(b).unwrap_or(0))))
            .chain(std::iter::once((Channel::Master, 8_192)));
        let mut out = Vec::new();
        for (ch, base) in chans {
            let name = self.name(ch).unwrap_or("").to_owned();
            let id = |f: u32| base * 16 + f;
            out.push(ParamInfo::new(id(0), &format!("{name} fader"), -144.0, 12.0, 0.0).unit("dB"));
            out.push(ParamInfo::new(
                id(1),
                &format!("{name} pan"),
                -1.0,
                1.0,
                0.0,
            ));
            out.push(ParamInfo::new(
                id(2),
                &format!("{name} width"),
                0.0,
                2.0,
                1.0,
            ));
            out.push(ParamInfo::toggle(id(3), &format!("{name} mute"), false));
            out.push(ParamInfo::toggle(id(4), &format!("{name} solo"), false));
            out.push(ParamInfo::toggle(
                id(5),
                &format!("{name} solo safe"),
                false,
            ));
            out.push(ParamInfo::toggle(id(6), &format!("{name} invert L"), false));
            out.push(ParamInfo::toggle(id(7), &format!("{name} invert R"), false));
        }
        for v in 0..self.vca_count() {
            let v32 = u32::try_from(v).unwrap_or(0);
            out.push(
                ParamInfo::new(
                    12_288 * 16 + v32,
                    &format!("VCA {}", v + 1),
                    -144.0,
                    12.0,
                    0.0,
                )
                .unit("dB"),
            );
        }
        out
    }

    fn get_param(&self, id: u32) -> Option<f64> {
        if id >= 12_288 * 16 {
            return self
                .vca(usize::try_from(id - 12_288 * 16).ok()?)
                .map(f64::from);
        }
        let (ch, f) = mixer_channel(id)?;
        let p = self.params(ch).ok()?;
        let b = |x: bool| if x { 1.0 } else { 0.0 };
        Some(match f {
            0 => f64::from(p.fader_db),
            1 => f64::from(p.pan),
            2 => f64::from(p.width),
            3 => b(p.mute),
            4 => b(p.solo),
            5 => b(p.solo_safe),
            6 => b(p.invert[0]),
            7 => b(p.invert[1]),
            _ => return None,
        })
    }

    fn set_param(&mut self, id: u32, value: f64) {
        #[expect(clippy::cast_possible_truncation, reason = "parameter values")]
        let v = value as f32;
        if id >= 12_288 * 16 {
            if let Ok(i) = usize::try_from(id - 12_288 * 16) {
                self.set_vca(i, v).ok();
            }
            return;
        }
        let Some((ch, f)) = mixer_channel(id) else {
            return;
        };
        let Ok(p) = self.params_mut(ch) else {
            return;
        };
        let on = value >= 0.5;
        match f {
            0 => p.fader_db = v,
            1 => p.pan = v,
            2 => p.width = v,
            3 => p.mute = on,
            4 => p.solo = on,
            5 => p.solo_safe = on,
            6 => p.invert[0] = on,
            7 => p.invert[1] = on,
            _ => {}
        }
    }
}

fn mixer_channel(id: u32) -> Option<(crate::mixer::Channel, u32)> {
    use crate::mixer::Channel;
    let (base, f) = (id / 16, id % 16);
    let ch = match base {
        8_192 => Channel::Master,
        b if b >= 4_096 => Channel::Bus(usize::try_from(b - 4_096).ok()?),
        t => Channel::Track(usize::try_from(t).ok()?),
    };
    Some((ch, f))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default)]
    struct Synth {
        cutoff: f64,
        res: f64,
        wave: f64,
        drive: f64,
    }

    impl Parametric for Synth {
        fn param_info(&self) -> Vec<ParamInfo> {
            vec![
                ParamInfo::new(1, "Cutoff", 20.0, 20_000.0, 1_000.0)
                    .taper(Taper::Log)
                    .unit("Hz"),
                ParamInfo::new(2, "Resonance", 0.0, 1.0, 0.2),
                ParamInfo::choice(3, "Wave", &["Saw", "Square", "Sine"], 0),
                ParamInfo::new(4, "Drive", 0.0, 24.0, 0.0)
                    .taper(Taper::Power(2.0))
                    .unit("dB"),
            ]
        }
        fn get_param(&self, id: u32) -> Option<f64> {
            Some(match id {
                1 => self.cutoff,
                2 => self.res,
                3 => self.wave,
                4 => self.drive,
                _ => return None,
            })
        }
        fn set_param(&mut self, id: u32, v: f64) {
            match id {
                1 => self.cutoff = v,
                2 => self.res = v,
                3 => self.wave = v,
                4 => self.drive = v,
                _ => {}
            }
        }
    }

    fn synth() -> Synth {
        Synth {
            cutoff: 2_000.0,
            res: 0.7,
            wave: 1.0,
            drive: 6.0,
        }
    }

    #[test]
    fn tapers_round_trip_and_log_is_geometric() {
        for info in synth().param_info() {
            for n in [0.0, 0.1, 0.37, 0.5, 0.9, 1.0] {
                let v = info.denormalize(n);
                let back = info.normalize(v);
                if info.taper == Taper::Stepped {
                    assert!((info.denormalize(back) - v).abs() < 1e-9);
                } else {
                    assert!((back - n).abs() < 1e-9, "{}: {n} {back}", info.name);
                }
            }
        }
        let cutoff = &synth().param_info()[0];
        // Half way on a log taper is the geometric mean.
        assert!((cutoff.denormalize(0.5) - (20.0f64 * 20_000.0).sqrt()).abs() < 1e-6);
        let drive = &synth().param_info()[3];
        assert!((drive.denormalize(0.5) - 6.0).abs() < 1e-9);
    }

    #[test]
    fn display_and_parse() {
        let infos = synth().param_info();
        assert_eq!(infos[0].display(1_234.5), "1234 Hz");
        assert_eq!(infos[0].display(440.0), "440.0 Hz");
        assert_eq!(infos[1].display(0.25), "0.250");
        assert_eq!(infos[2].display(2.0), "Sine");
        assert_eq!(infos[2].parse("square"), Some(1.0));
        assert_eq!(infos[0].parse("880 Hz"), Some(880.0));
        assert_eq!(infos[0].parse("99999"), Some(20_000.0));
        assert_eq!(infos[0].parse("loud"), None);
        assert_eq!(infos[2].clamp(1.4), 1.0);
        assert_eq!(infos[1].clamp(f64::NAN), 0.2);
        assert_eq!(ParamInfo::toggle(9, "Bypass", true).display(1.0), "On");
    }

    #[test]
    fn binary_state_round_trips_and_is_checked() {
        let mut s = State::capture(&synth(), "Fat Lead");
        s.blobs.insert("sample".into(), vec![1, 2, 3, 250]);
        let bytes = s.to_bytes();
        assert_eq!(State::from_bytes(&bytes).unwrap(), s);
        let mut bad = bytes.clone();
        bad[20] ^= 0x40;
        assert_eq!(State::from_bytes(&bad), Err(StateError::Checksum));
        assert_eq!(
            State::from_bytes(&bytes[..bytes.len() - 6]),
            Err(StateError::Truncated)
        );
        assert_eq!(
            State::from_bytes(b"RIFFxxxxxxxx"),
            Err(StateError::NotState)
        );
    }

    #[test]
    fn text_state_round_trips_exactly() {
        let mut s = State::capture(&synth(), "Fat Lead");
        s.params.insert(2, 0.1 + 0.2); // not representable in short decimal
        s.blobs.insert("table".into(), (0..=255).collect());
        let t = s.to_text(&synth().param_info());
        assert!(t.contains("# Cutoff: 2000 Hz"));
        assert!(t.contains("# Wave: Square"));
        assert_eq!(State::from_text(&t).unwrap(), s);
        assert!(matches!(
            State::from_text("name = x\nnonsense"),
            Err(StateError::Malformed(_))
        ));
        assert_eq!(State::from_text("# only\n"), Err(StateError::NotState));
        // Unknown keys from a newer writer are ignored.
        let newer = format!("{t}future.key = 7\n");
        assert_eq!(State::from_text(&newer).unwrap(), s);
    }

    #[test]
    fn apply_is_compatible_both_ways() {
        let mut target = Synth::default();
        let mut old = State::default();
        old.params.insert(1, 50_000.0); // out of range now
        old.params.insert(99, 1.0); // a parameter since removed
        old.apply(&mut target);
        assert_eq!(target.cutoff, 20_000.0);
        assert_eq!(target.res, 0.2, "missing ids take their defaults");
        assert_eq!(target.wave, 0.0);
    }

    #[test]
    fn crc_and_base64_known_values() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_decode("Zm9vYg==").unwrap(), b"foob");
        assert_eq!(base64_decode("Zm9v\nYmE").unwrap(), b"fooba");
        assert!(base64_decode("@@").is_none());
    }

    #[test]
    fn morph_interpolates_in_normalised_space() {
        let infos = synth().param_info();
        let mut a = State::capture(&synth(), "a");
        let mut b = a.clone();
        b.name = "b".into();
        a.params.insert(1, 100.0);
        b.params.insert(1, 10_000.0);
        a.params.insert(3, 0.0);
        b.params.insert(3, 2.0);
        let m = morph(&a, &b, 0.5, &infos);
        assert!((m.params[&1] - 1_000.0).abs() < 1e-6, "{}", m.params[&1]);
        assert_eq!(m.params[&3], 2.0);
        assert_eq!(morph(&a, &b, 0.49, &infos).params[&3], 0.0);
        assert_eq!(morph(&a, &b, 0.0, &infos).params[&1], 100.0);
    }

    #[test]
    fn bank_search_step_and_text() {
        let mut bank = Bank::new();
        for (name, cat, tags) in [
            ("Warm Pad", "Pad", vec!["warm", "slow"]),
            ("Acid Bass", "Bass", vec!["303", "squelch"]),
            ("Glass Pad", "Pad", vec!["bright"]),
        ] {
            bank.add(Preset {
                state: State {
                    name: name.into(),
                    ..State::capture(&synth(), name)
                },
                category: cat.into(),
                tags: tags.into_iter().map(String::from).collect(),
                author: "vieww".into(),
                favourite: name == "Acid Bass",
            });
        }
        assert_eq!(bank.search("pad"), vec![0, 2]);
        assert_eq!(bank.search("PAD warm"), vec![0]);
        assert_eq!(bank.search("303"), vec![1]);
        assert_eq!(bank.in_category("bass"), vec![1]);
        assert_eq!(bank.categories(), vec!["Bass", "Pad"]);
        let mut s = Synth::default();
        assert_eq!(bank.step(-1, &mut s).unwrap().state.name, "Warm Pad");
        assert_eq!(bank.step(-1, &mut s).unwrap().state.name, "Glass Pad");
        assert_eq!(bank.step(1, &mut s).unwrap().state.name, "Warm Pad");
        assert_eq!(s.cutoff, 2_000.0);
        let back = Bank::from_text(&bank.to_text()).unwrap();
        assert_eq!(back.presets, bank.presets);
    }

    #[test]
    fn ab_compare() {
        let mut s = synth();
        let mut ab = AbCompare::new(&s);
        ab.toggle(&mut s); // to B (same as A so far)
        s.set_param(1, 500.0);
        ab.toggle(&mut s); // back to A
        assert_eq!(s.cutoff, 2_000.0);
        ab.toggle(&mut s);
        assert_eq!(s.cutoff, 500.0);
        assert!(ab.on_b());
        ab.copy_to_other(&s);
        ab.toggle(&mut s);
        assert_eq!(s.cutoff, 500.0);
    }

    #[test]
    fn history_coalesces_gestures() {
        let mut s = synth();
        let mut h = History::new(10);
        h.set(&mut s, 2, 0.1);
        h.begin_gesture();
        for v in [0.2, 0.3, 0.4, 0.5] {
            h.set(&mut s, 1, v * 10_000.0);
        }
        h.end_gesture();
        assert_eq!(h.depth(), (2, 0));
        assert!(h.undo(&mut s));
        assert_eq!(s.cutoff, 2_000.0, "the whole drag undoes at once");
        assert!(h.undo(&mut s));
        assert_eq!(s.res, 0.7);
        assert!(!h.undo(&mut s));
        assert!(h.redo(&mut s));
        assert!(h.redo(&mut s));
        assert_eq!((s.res, s.cutoff), (0.1, 5_000.0));
        h.undo(&mut s);
        h.set(&mut s, 4, 1.0);
        assert_eq!(h.depth(), (2, 0), "a new change clears redo");
        let mut small = History::new(2);
        for i in 0..5 {
            small.set(&mut s, 2, f64::from(i) / 10.0);
        }
        assert_eq!(small.depth(), (2, 0));
    }

    #[test]
    fn a_mix_recalls_through_state() {
        use crate::mixer::{Channel, Mixer};
        let mut m = Mixer::new(48_000);
        let t = m.add_track("Vox");
        let b = m.add_bus("Verb");
        m.add_vca();
        m.params_mut(t).unwrap().fader_db = -4.5;
        m.params_mut(t).unwrap().pan = 0.25;
        m.params_mut(b).unwrap().mute = true;
        m.params_mut(Channel::Master).unwrap().invert = [false, true];
        m.set_vca(0, -3.0).unwrap();
        let saved = State::capture(&m, "Mix 1");
        assert!(saved
            .to_text(&m.param_info())
            .contains("# Vox fader: -4.500 dB"));
        let bytes = saved.to_bytes();

        let mut fresh = Mixer::new(48_000);
        fresh.add_track("Vox");
        fresh.add_bus("Verb");
        fresh.add_vca();
        State::from_bytes(&bytes).unwrap().apply(&mut fresh);
        assert_eq!(fresh.params(t).unwrap(), m.params(t).unwrap());
        assert!(fresh.params(b).unwrap().mute);
        assert_eq!(fresh.params(Channel::Master).unwrap().invert, [false, true]);
        assert_eq!(fresh.vca(0), Some(-3.0));
    }
}
