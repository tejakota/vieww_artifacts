//! SoundFont 2 — the binary sample-bank format of General MIDI players.
//!
//! A `.sf2` is a RIFF file with three lists: `INFO`, `sdta` (16-bit sample
//! data, optionally with an `sm24` low byte for 24-bit), and `pdta` — nine
//! tables (`phdr pbag pmod pgen inst ibag imod igen shdr`) describing
//! *presets* made of zones that point at *instruments* made of zones that
//! point at *samples*. Each zone carries *generators*: key and velocity
//! ranges, root key, tuning, attenuation, pan, loop mode and offsets, and the
//! volume envelope.
//!
//! The rules implemented here are the spec's (§8–§9):
//!
//! * a zone's first-in-list *global* zone (one without a terminal
//!   `instrument`/`sampleID` generator) supplies defaults;
//! * instrument-level generators are *absolute*; preset-level ones are
//!   *added* to them (tuning, attenuation, pan, envelope timecents), and
//!   key/velocity ranges *intersect*;
//! * envelope times are timecents (`2^(tc/1200)` s), attenuation and
//!   sustain are centibels, pan is tenths of a percent;
//! * sample-address offsets combine fine and coarse (×32768) parts.
//!
//! [`SoundFont::instrument`] resolves a bank/program to a sampler
//! [`Instrument`]. SF3 — the same container with Ogg-Vorbis-compressed
//! samples — is decoded through the crate's own Vorbis decoder.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::envelope::Adsr;
use super::sampler::{Instrument, LoopMode, SampleData, Zone};

/// Why a file was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sf2Error {
    /// Not a RIFF `sfbk`.
    NotSoundFont,
    /// A chunk ran past the end of the file or a table is malformed.
    Truncated(&'static str),
    /// A compressed (SF3) sample failed to decode.
    Codec(String),
}

impl std::fmt::Display for Sf2Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotSoundFont => f.write_str("not a SoundFont (RIFF sfbk)"),
            Self::Truncated(w) => write!(f, "truncated or malformed: {w}"),
            Self::Codec(e) => write!(f, "compressed sample: {e}"),
        }
    }
}

impl std::error::Error for Sf2Error {}

/// A sample header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleHeader {
    /// Name.
    pub name: String,
    /// Start (frames into `smpl`, or bytes for SF3).
    pub start: u32,
    /// End.
    pub end: u32,
    /// Loop start.
    pub loop_start: u32,
    /// Loop end.
    pub loop_end: u32,
    /// Rate.
    pub rate: u32,
    /// Recorded MIDI key.
    pub original_pitch: u8,
    /// Correction in cents.
    pub correction: i8,
    /// Type flags (0x10 = Ogg Vorbis compressed).
    pub kind: u16,
}

/// A preset's identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetInfo {
    /// Name.
    pub name: String,
    /// Program number (0..=127).
    pub program: u16,
    /// Bank number.
    pub bank: u16,
}

type Gens = BTreeMap<u16, u16>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ZoneGens {
    gens: Gens,
}

/// A parsed SoundFont.
#[derive(Debug, Clone)]
pub struct SoundFont {
    /// Presets in file order.
    pub presets: Vec<PresetInfo>,
    preset_zones: Vec<Vec<ZoneGens>>,
    instrument_zones: Vec<Vec<ZoneGens>>,
    /// Sample headers.
    pub samples: Vec<SampleHeader>,
    smpl: Vec<i16>,
    sm24: Vec<u8>,
    raw: Vec<u8>,
    decoded: BTreeMap<usize, Arc<SampleData>>,
}

fn u16le(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

fn u32le(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

fn name20(b: &[u8]) -> String {
    let end = b.iter().position(|c| *c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).trim().to_string()
}

/// A RIFF chunk: its four-character id and its body.
type Chunk<'a> = ([u8; 4], &'a [u8]);

/// Walk RIFF sub-chunks of `b` (after a list type).
fn chunks(b: &[u8]) -> Result<Vec<Chunk<'_>>, Sf2Error> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 8 <= b.len() {
        let id = [b[i], b[i + 1], b[i + 2], b[i + 3]];
        let len = u32le(b, i + 4) as usize;
        let start = i + 8;
        let end = start.checked_add(len).ok_or(Sf2Error::Truncated("chunk"))?;
        if end > b.len() {
            return Err(Sf2Error::Truncated("chunk"));
        }
        out.push((id, &b[start..end]));
        i = end + (len & 1);
    }
    Ok(out)
}

const GEN_INSTRUMENT: u16 = 41;
const GEN_KEY_RANGE: u16 = 43;
const GEN_VEL_RANGE: u16 = 44;
const GEN_SAMPLE_ID: u16 = 53;

fn zones_of(
    bags: &[u8],
    gens: &[u8],
    first: usize,
    last: usize,
    terminal: u16,
) -> Result<Vec<ZoneGens>, Sf2Error> {
    let mut out = Vec::new();
    for b in first..last {
        if (b + 1) * 4 + 2 > bags.len() {
            return Err(Sf2Error::Truncated("bag"));
        }
        let g0 = u16le(bags, b * 4) as usize;
        let g1 = u16le(bags, (b + 1) * 4) as usize;
        let mut z = ZoneGens::default();
        for g in g0..g1 {
            if g * 4 + 4 > gens.len() {
                return Err(Sf2Error::Truncated("gen"));
            }
            z.gens.insert(u16le(gens, g * 4), u16le(gens, g * 4 + 2));
        }
        out.push(z);
    }
    // A first zone without the terminal generator is the global zone; move
    // it to a known place by marking it (key u16::MAX).
    if let Some(f) = out.first_mut() {
        if !f.gens.contains_key(&terminal) {
            f.gens.insert(u16::MAX, 1);
        }
    }
    Ok(out)
}

impl SoundFont {
    /// Parse a `.sf2` or `.sf3`.
    ///
    /// # Errors
    /// See [`Sf2Error`].
    pub fn parse(bytes: &[u8]) -> Result<Self, Sf2Error> {
        if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"sfbk" {
            return Err(Sf2Error::NotSoundFont);
        }
        let mut smpl = Vec::new();
        let mut sm24 = Vec::new();
        let mut raw = Vec::new();
        let mut tables: BTreeMap<[u8; 4], Vec<u8>> = BTreeMap::new();
        for (id, body) in chunks(&bytes[12..])? {
            if &id != b"LIST" || body.len() < 4 {
                continue;
            }
            let kind = &body[0..4];
            for (sid, sbody) in chunks(&body[4..])? {
                match (kind, &sid) {
                    (b"sdta", b"smpl") => {
                        raw = sbody.to_vec();
                        smpl = sbody
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|c| i16::from_le_bytes([c[0], c[1]]))
                            .collect();
                    }
                    (b"sdta", b"sm24") => sm24 = sbody.to_vec(),
                    (b"pdta", _) => {
                        tables.insert(sid, sbody.to_vec());
                    }
                    _ => {}
                }
            }
        }
        let t = |k: &[u8; 4]| {
            tables
                .get(k)
                .cloned()
                .ok_or(Sf2Error::Truncated("pdta table"))
        };
        let (phdr, pbag, pgen) = (t(b"phdr")?, t(b"pbag")?, t(b"pgen")?);
        let (inst, ibag, igen, shdr) = (t(b"inst")?, t(b"ibag")?, t(b"igen")?, t(b"shdr")?);

        let np = phdr.len() / 38;
        let mut presets = Vec::new();
        let mut preset_zones = Vec::new();
        for p in 0..np.saturating_sub(1) {
            let r = &phdr[p * 38..(p + 1) * 38];
            presets.push(PresetInfo {
                name: name20(&r[0..20]),
                program: u16le(r, 20),
                bank: u16le(r, 22),
            });
            let b0 = u16le(r, 24) as usize;
            let b1 = u16le(&phdr, (p + 1) * 38 + 24) as usize;
            preset_zones.push(zones_of(&pbag, &pgen, b0, b1, GEN_INSTRUMENT)?);
        }
        let ni = inst.len() / 22;
        let mut instrument_zones = Vec::new();
        for i in 0..ni.saturating_sub(1) {
            let b0 = u16le(&inst, i * 22 + 20) as usize;
            let b1 = u16le(&inst, (i + 1) * 22 + 20) as usize;
            instrument_zones.push(zones_of(&ibag, &igen, b0, b1, GEN_SAMPLE_ID)?);
        }
        let ns = shdr.len() / 46;
        let samples = (0..ns.saturating_sub(1))
            .map(|s| {
                let r = &shdr[s * 46..(s + 1) * 46];
                SampleHeader {
                    name: name20(&r[0..20]),
                    start: u32le(r, 20),
                    end: u32le(r, 24),
                    loop_start: u32le(r, 28),
                    loop_end: u32le(r, 32),
                    rate: u32le(r, 36),
                    original_pitch: r[40],
                    correction: i8::from_le_bytes([r[41]]),
                    kind: u16le(r, 44),
                }
            })
            .collect();
        Ok(Self {
            presets,
            preset_zones,
            instrument_zones,
            samples,
            smpl,
            sm24,
            raw,
            decoded: BTreeMap::new(),
        })
    }

    fn sample_data(&mut self, id: usize) -> Result<Arc<SampleData>, Sf2Error> {
        if let Some(d) = self.decoded.get(&id) {
            return Ok(d.clone());
        }
        let h = self
            .samples
            .get(id)
            .ok_or(Sf2Error::Truncated("sample id"))?
            .clone();
        #[expect(clippy::cast_precision_loss, reason = "sample rates")]
        let rate = h.rate.max(1) as f32;
        let data = if h.kind & 0x10 != 0 {
            let (s, e) = (h.start as usize, (h.end as usize).min(self.raw.len()));
            let ogg = self
                .raw
                .get(s..e)
                .ok_or(Sf2Error::Truncated("sf3 sample"))?;
            let dec =
                crate::codec::vorbis::decode(ogg).map_err(|e| Sf2Error::Codec(e.to_string()))?;
            let mono: Vec<f32> = if dec.channels > 1 {
                dec.data
                    .chunks(usize::from(dec.channels))
                    .map(|c| c[0])
                    .collect()
            } else {
                dec.data
            };
            SampleData::mono(mono, rate)
        } else {
            let (s, e) = (h.start as usize, (h.end as usize).min(self.smpl.len()));
            let s = s.min(e);
            let has24 = self.sm24.len() >= e;
            let v = (s..e)
                .map(|i| {
                    let hi = f32::from(self.smpl[i]);
                    if has24 {
                        (hi * 256.0 + f32::from(self.sm24[i])) / 8_388_608.0
                    } else {
                        hi / 32_768.0
                    }
                })
                .collect();
            SampleData::mono(v, rate)
        };
        let a = Arc::new(data);
        self.decoded.insert(id, a.clone());
        Ok(a)
    }

    /// The sampler instrument for `bank`/`program`, or `None` if no preset
    /// matches.
    ///
    /// # Errors
    /// A malformed zone or an undecodable sample.
    pub fn instrument(&mut self, bank: u16, program: u16) -> Result<Option<Instrument>, Sf2Error> {
        let Some(pi) = self
            .presets
            .iter()
            .position(|p| p.bank == bank && p.program == program)
        else {
            return Ok(None);
        };
        let pz = self.preset_zones[pi].clone();
        let pglobal = pz
            .iter()
            .find(|z| z.gens.contains_key(&u16::MAX))
            .cloned()
            .unwrap_or_default();
        let mut zones = Vec::new();
        for p in pz.iter().filter(|z| !z.gens.contains_key(&u16::MAX)) {
            let Some(&ii) = p.gens.get(&GEN_INSTRUMENT) else {
                continue;
            };
            let mut pg = pglobal.gens.clone();
            pg.extend(p.gens.iter().map(|(k, v)| (*k, *v)));
            let iz = self
                .instrument_zones
                .get(usize::from(ii))
                .ok_or(Sf2Error::Truncated("instrument index"))?
                .clone();
            let iglobal = iz
                .iter()
                .find(|z| z.gens.contains_key(&u16::MAX))
                .cloned()
                .unwrap_or_default();
            for z in iz.iter().filter(|z| !z.gens.contains_key(&u16::MAX)) {
                let Some(&sid) = z.gens.get(&GEN_SAMPLE_ID) else {
                    continue;
                };
                let mut ig = iglobal.gens.clone();
                ig.extend(z.gens.iter().map(|(k, v)| (*k, *v)));
                if let Some(zone) = self.make_zone(&ig, &pg, usize::from(sid))? {
                    zones.push(zone);
                }
            }
        }
        Ok(Some(Instrument {
            zones,
            ..Instrument::default()
        }))
    }

    fn make_zone(&mut self, ig: &Gens, pg: &Gens, sid: usize) -> Result<Option<Zone>, Sf2Error> {
        let s16 = |g: &Gens, k: u16, d: i16| g.get(&k).map_or(d, |v| v.cast_signed());
        let range = |g: &Gens, k: u16| {
            g.get(&k)
                .map_or((0u8, 127u8), |v| ((v & 0xff) as u8, (v >> 8) as u8))
        };
        // Ranges intersect.
        let (ik, pk) = (range(ig, GEN_KEY_RANGE), range(pg, GEN_KEY_RANGE));
        let (iv, pv) = (range(ig, GEN_VEL_RANGE), range(pg, GEN_VEL_RANGE));
        let (klo, khi) = (ik.0.max(pk.0), ik.1.min(pk.1));
        let (vlo, vhi) = (iv.0.max(pv.0), iv.1.min(pv.1));
        if klo > khi || vlo > vhi {
            return Ok(None);
        }
        // Additive generators: instrument absolute (+ default) + preset offset.
        let add = |k: u16, d: i16| i32::from(s16(ig, k, d)) + i32::from(s16(pg, k, 0));
        let h = self
            .samples
            .get(sid)
            .ok_or(Sf2Error::Truncated("sample id"))?
            .clone();
        let data = self.sample_data(sid)?;
        let off =
            |fine: u16, coarse: u16| i64::from(add(fine, 0)) + 32_768 * i64::from(add(coarse, 0));
        let compressed = h.kind & 0x10 != 0;
        let base = if compressed { 0 } else { i64::from(h.start) };
        let frames = data.frames();
        let clampi = |v: i64| {
            usize::try_from(v.clamp(0, i64::try_from(frames).unwrap_or(i64::MAX))).unwrap_or(0)
        };
        let start = clampi(off(0, 4));
        let end = clampi(i64::try_from(frames).unwrap_or(0) + off(1, 12));
        let ls = clampi(i64::from(h.loop_start) - base + off(2, 45));
        let le = clampi(i64::from(h.loop_end) - base + off(3, 50));
        let root = match s16(ig, 58, -1) {
            r if (0..=127).contains(&r) => f32::from(r),
            _ => f32::from(h.original_pitch.min(127)),
        };
        let tune_cents = 100.0 * add(51, 0) as f32 + add(52, 0) as f32 + f32::from(h.correction);
        let tc = |k: u16| 2f32.powf(add(k, -12_000) as f32 / 1200.0);
        let sustain_cb = add(37, 0).clamp(0, 1440) as f32;
        let mut z = Zone::new(data, root).keys(klo, khi).vels(vlo.max(1), vhi);
        z.tune = tune_cents;
        z.keytrack = f32::from(s16(ig, 56, 100)) + f32::from(s16(pg, 56, 0));
        // initialAttenuation is centibels of cut; SoundFont players
        // conventionally apply 0.4 of it (the EMU hardware's scaling).
        z.volume = -0.4 * add(48, 0).clamp(0, 1440) as f32 / 10.0;
        z.pan = (add(17, 0).clamp(-500, 500) as f32) / 500.0;
        z.offset = start;
        z.end = end;
        z.loop_mode = match s16(ig, 54, 0) & 3 {
            1 => LoopMode::Continuous,
            3 => LoopMode::Sustain,
            _ => LoopMode::None,
        };
        z.loop_start = ls;
        z.loop_end = le.max(ls + 1);
        z.amp = Adsr::new(
            tc(33) + tc(34) + tc(35),
            tc(36),
            10f32.powf(-sustain_cb / 200.0),
            tc(38).max(0.001),
        );
        if let Some(&ex) = ig.get(&57) {
            if ex != 0 {
                z.group = u32::from(ex);
                z.off_by = u32::from(ex);
            }
        }
        Ok(Some(z))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Build a minimal SoundFont: one sine sample, one instrument, presets.
    pub(crate) fn build(
        sample: &[i16],
        rate: u32,
        root: u8,
        inst_gens: &[(u16, u16)],
        preset_gens: &[(u16, u16)],
    ) -> Vec<u8> {
        build_with(sample, None, rate, root, inst_gens, preset_gens)
    }

    /// As [`build`], optionally storing `compressed` (Ogg Vorbis) bytes as
    /// an SF3 sample instead of 16-bit PCM.
    pub(crate) fn build_with(
        sample: &[i16],
        compressed: Option<&[u8]>,
        rate: u32,
        root: u8,
        inst_gens: &[(u16, u16)],
        preset_gens: &[(u16, u16)],
    ) -> Vec<u8> {
        fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
            let mut v = id.to_vec();
            v.extend((u32::try_from(body.len()).unwrap()).to_le_bytes());
            v.extend(body);
            if body.len() % 2 == 1 {
                v.push(0);
            }
            v
        }
        fn list(kind: &[u8; 4], items: &[Vec<u8>]) -> Vec<u8> {
            let mut body = kind.to_vec();
            for i in items {
                body.extend(i);
            }
            chunk(b"LIST", &body)
        }
        fn name(s: &str) -> [u8; 20] {
            let mut n = [0u8; 20];
            n[..s.len()].copy_from_slice(s.as_bytes());
            n
        }
        let mut smpl: Vec<u8> = match compressed {
            Some(c) => c.to_vec(),
            None => sample.iter().flat_map(|v| v.to_le_bytes()).collect(),
        };
        let stored = u32::try_from(compressed.map_or(sample.len(), <[u8]>::len)).unwrap();
        smpl.extend([0u8; 92]); // 46 zero samples, as the spec requires
        let mut phdr = Vec::new();
        for (n, prog, bag) in [("Sine", 0u16, 0u16), ("EOP", 0, 1)] {
            phdr.extend(name(n));
            phdr.extend(prog.to_le_bytes());
            phdr.extend(0u16.to_le_bytes());
            phdr.extend(bag.to_le_bytes());
            phdr.extend([0u8; 12]);
        }
        let mut pgen = Vec::new();
        for (k, v) in preset_gens.iter().chain(&[(GEN_INSTRUMENT, 0)]) {
            pgen.extend(k.to_le_bytes());
            pgen.extend(v.to_le_bytes());
        }
        pgen.extend([0u8; 4]);
        let np = u16::try_from(preset_gens.len() + 1).unwrap();
        let pbag: Vec<u8> = [0u16, 0, np, 0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let mut inst = Vec::new();
        for (n, bag) in [("SineInst", 0u16), ("EOI", 1)] {
            inst.extend(name(n));
            inst.extend(bag.to_le_bytes());
        }
        let mut igen = Vec::new();
        for (k, v) in inst_gens.iter().chain(&[(GEN_SAMPLE_ID, 0)]) {
            igen.extend(k.to_le_bytes());
            igen.extend(v.to_le_bytes());
        }
        igen.extend([0u8; 4]);
        let ni = u16::try_from(inst_gens.len() + 1).unwrap();
        let ibag: Vec<u8> = [0u16, 0, ni, 0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let n = stored;
        let mut shdr = Vec::new();
        shdr.extend(name("sine"));
        let (ls, le) = if compressed.is_some() {
            (8, 4_000)
        } else {
            (8, n - 8)
        };
        for v in [0, n, ls, le, rate] {
            shdr.extend(v.to_le_bytes());
        }
        shdr.extend([root, 0]);
        shdr.extend([0u8, 0]);
        let kind: u16 = if compressed.is_some() { 0x11 } else { 1 };
        shdr.extend(kind.to_le_bytes());
        shdr.extend([0u8; 46]);
        let pmod = vec![0u8; 10];
        let body = [
            list(b"INFO", &[chunk(b"ifil", &[2, 0, 1, 0])]),
            list(b"sdta", &[chunk(b"smpl", &smpl)]),
            list(
                b"pdta",
                &[
                    chunk(b"phdr", &phdr),
                    chunk(b"pbag", &pbag),
                    chunk(b"pmod", &pmod),
                    chunk(b"pgen", &pgen),
                    chunk(b"inst", &inst),
                    chunk(b"ibag", &ibag),
                    chunk(b"imod", &pmod),
                    chunk(b"igen", &igen),
                    chunk(b"shdr", &shdr),
                ],
            ),
        ]
        .concat();
        let mut file = b"RIFF".to_vec();
        file.extend(u32::try_from(body.len() + 4).unwrap().to_le_bytes());
        file.extend(b"sfbk");
        file.extend(body);
        file
    }

    fn sine16(hz: f32, rate: f32, n: usize) -> Vec<i16> {
        (0..n)
            .map(|i| {
                #[expect(
                    clippy::cast_precision_loss,
                    clippy::cast_possible_truncation,
                    reason = "test"
                )]
                let v = ((std::f32::consts::TAU * hz * i as f32 / rate).sin() * 30_000.0) as i16;
                v
            })
            .collect()
    }

    #[test]
    fn parses_and_resolves_a_preset() {
        let file = build(
            &sine16(440.0, 22_050.0, 22_050),
            22_050,
            69,
            &[(54, 1)],
            &[],
        );
        let mut sf = SoundFont::parse(&file).unwrap();
        assert_eq!(sf.presets[0].name, "Sine");
        assert_eq!(sf.samples[0].original_pitch, 69);
        let inst = sf.instrument(0, 0).unwrap().unwrap();
        assert_eq!(inst.zones.len(), 1);
        let z = &inst.zones[0];
        assert!((z.root - 69.0).abs() < 1e-6);
        assert_eq!(z.loop_mode, LoopMode::Continuous);
        assert!((z.sample.rate - 22_050.0).abs() < 1e-3);
        assert!(sf.instrument(0, 5).unwrap().is_none());
    }

    #[test]
    fn preset_generators_add_and_ranges_intersect() {
        // Instrument: keys 40..=80, coarse tune +2. Preset: keys 60..=100,
        // coarse tune +10 → keys 60..=80, tune +12 semitones.
        let inst = [(GEN_KEY_RANGE, 40 | (80 << 8)), (51, 2)];
        let pre = [(GEN_KEY_RANGE, 60 | (100 << 8)), (51, 10)];
        let file = build(&sine16(440.0, 22_050.0, 4_000), 22_050, 69, &inst, &pre);
        let mut sf = SoundFont::parse(&file).unwrap();
        let z = sf.instrument(0, 0).unwrap().unwrap().zones.remove(0);
        assert_eq!((z.lo_key, z.hi_key), (60, 80));
        assert!((z.tune - 1200.0).abs() < 1e-3);
    }

    #[test]
    fn envelope_timecents_and_attenuation() {
        // attack 0 tc = 1 s; release -1200 tc = 0.5 s; attenuation 100 cB.
        let inst = [
            (34, 0),
            (38, (-1200i16).cast_unsigned()),
            (48, 100),
            (37, 200),
        ];
        let file = build(&sine16(440.0, 22_050.0, 4_000), 22_050, 69, &inst, &[]);
        let mut sf = SoundFont::parse(&file).unwrap();
        let z = sf.instrument(0, 0).unwrap().unwrap().zones.remove(0);
        assert!((z.amp.attack - 1.0).abs() < 0.01, "{}", z.amp.attack);
        assert!((z.amp.release - 0.5).abs() < 1e-3);
        assert!((z.amp.sustain - 0.1).abs() < 1e-3, "200 cB = −20 dB");
        assert!((z.volume + 4.0).abs() < 1e-3);
    }

    #[test]
    fn plays_at_the_right_pitch() {
        let file = build(
            &sine16(440.0, 22_050.0, 22_050),
            22_050,
            69,
            &[(54, 1)],
            &[],
        );
        let mut sf = SoundFont::parse(&file).unwrap();
        let inst = sf.instrument(0, 0).unwrap().unwrap();
        let mut s = super::super::sampler::Sampler::new(inst, 4, 48_000.0);
        s.note_on(0, 81, 127);
        let x: Vec<f32> = s
            .render(16_384)
            .as_chunks::<2>()
            .0
            .iter()
            .map(|f| f[0])
            .collect();
        let (m, bin) = crate::synth::testing::spectrum(&x, 48_000.0);
        assert!(crate::synth::testing::level_near(&m, bin, 880.0, 2) > 0.2);
    }

    #[test]
    fn sf3_compressed_samples_decode_through_vorbis() {
        let ogg = include_bytes!("../../tests/fixtures/src.ogg");
        let file = build_with(&[], Some(ogg), 44_100, 60, &[(54, 1)], &[]);
        let mut sf = SoundFont::parse(&file).unwrap();
        assert_eq!(sf.samples[0].kind, 0x11);
        let inst = sf.instrument(0, 0).unwrap().unwrap();
        let z = &inst.zones[0];
        assert_eq!(z.sample.frames(), 17_640, "the Ogg stream, decoded");
        assert_eq!(
            (z.loop_start, z.loop_end),
            (8, 4_000),
            "SF3 loop points are in decoded frames"
        );
        let mut s = super::super::sampler::Sampler::new(inst, 2, 44_100.0);
        s.note_on(0, 60, 127);
        assert!(s.render(4_410).iter().any(|v| v.abs() > 0.05));
    }

    #[test]
    fn rejects_non_soundfonts() {
        assert_eq!(
            SoundFont::parse(b"RIFF\0\0\0\0WAVE").err(),
            Some(Sf2Error::NotSoundFont)
        );
        let mut f = build(&[0; 100], 22_050, 60, &[], &[]);
        f.truncate(f.len() - 30);
        assert!(SoundFont::parse(&f).is_err());
    }
}
