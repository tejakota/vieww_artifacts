//! A multitrack mixer: channel strips, buses, sends, sidechains, VCAs,
//! solo/mute logic, plugin delay compensation and metering.
//!
//! The model is a console's. Every **track** has one input (stereo, or
//! mono panned into stereo), an insert chain of [`Stereo`] effects (any of
//! which may take a sidechain [`Key`]), a fader, a pan and width control,
//! phase invert, mute/solo/solo-safe, a VCA assignment, a main output to
//! the master or a **bus**, and any number of pre- or post-fader sends to
//! buses. Buses are tracks without an input: they sum what is routed to
//! them, may route on to other buses, and the master is the bus at the
//! end of everything.
//!
//! Routing is a graph, and three things follow from that:
//!
//! * Routing changes ([`Mixer::route`], [`Mixer::add_send`],
//!   [`Mixer::add_insert`] …) are validated when they are made: a change
//!   that would make a cycle is refused with [`MixerError::Cycle`] and the
//!   mixer is left as it was. They allocate, so they belong to the control
//!   side of an application, between blocks.
//! * **Delay compensation** is computed from the graph: each effect reports
//!   its [`Stereo::latency`]; every path into a bus is delayed so that all
//!   arrive together, the way a DAW's PDC aligns a track with a lookahead
//!   limiter against one without.
//! * **Solo** follows signal flow: soloing a track keeps the buses it feeds
//!   audible, soloing a bus keeps the tracks that feed it audible, and
//!   *solo-safe* channels (a reverb return, typically) are never muted by
//!   someone else's solo.
//!
//! Parameter changes (fader, pan, mute, send levels …) take effect on the
//! next block, ramped linearly across it so nothing clicks, and processing
//! — [`Mixer::process`] — never allocates.

use crate::fx::Stereo;
use crate::meter::{LoudnessMeter, PeakMeter};

/// Frames processed per internal block; longer calls are split.
pub const MAX_BLOCK: usize = 256;

/// An insert effect.
pub type Insert = Box<dyn Stereo + Send>;

/// A channel of the mixer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    /// An input track.
    Track(usize),
    /// A bus.
    Bus(usize),
    /// The master bus.
    Master,
}

/// Where a channel's main output (or a send) goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dest {
    /// The master bus.
    Master,
    /// A bus.
    Bus(usize),
}

/// The source of a sidechain key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    /// A track's raw input.
    Input(usize),
    /// A bus's signal after its inserts (keys a bus or the master; the
    /// keying bus is processed first).
    Bus(usize),
}

/// Where a send taps the channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tap {
    /// After the inserts, before fader and pan.
    PreFader,
    /// After fader and pan.
    PostFader,
}

/// How pan distributes a signal between the outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanLaw {
    /// Stereo balance: unity at centre, the far side fades to silence.
    Balance,
    /// Sine/cosine, −3 dB at centre (the usual mono-source law).
    ConstantPower,
    /// Sine/cosine raised 3 dB: unity at centre, +3 dB hard to one side.
    CompensatedConstantPower,
    /// Linear, −6 dB at centre.
    Linear,
}

impl PanLaw {
    /// Left and right gains for `pan` in −1..=1.
    #[must_use]
    pub fn gains(self, pan: f32) -> (f32, f32) {
        let p = pan.clamp(-1.0, 1.0);
        let theta = (p + 1.0) * std::f32::consts::FRAC_PI_4;
        match self {
            Self::Balance => ((1.0 - p).min(1.0), (1.0 + p).min(1.0)),
            Self::ConstantPower => (theta.cos(), theta.sin()),
            Self::CompensatedConstantPower => (
                theta.cos() * std::f32::consts::SQRT_2,
                theta.sin() * std::f32::consts::SQRT_2,
            ),
            Self::Linear => ((1.0 - p) / 2.0, (1.0 + p) / 2.0),
        }
    }
}

/// A channel's parameters: change them freely, between or during blocks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    /// Fader, dB.
    pub fader_db: f32,
    /// Pan, −1 (left) ..= 1 (right).
    pub pan: f32,
    /// The pan law.
    pub pan_law: PanLaw,
    /// Stereo width: 0 mono, 1 as recorded, 2 doubled side.
    pub width: f32,
    /// Mute.
    pub mute: bool,
    /// Solo.
    pub solo: bool,
    /// Never muted by another channel's solo.
    pub solo_safe: bool,
    /// Polarity inversion per channel.
    pub invert: [bool; 2],
    /// The VCA this channel follows.
    pub vca: Option<usize>,
    /// The track input is one channel, panned into stereo.
    pub mono_input: bool,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            fader_db: 0.0,
            pan: 0.0,
            pan_law: PanLaw::Balance,
            width: 1.0,
            mute: false,
            solo: false,
            solo_safe: false,
            invert: [false; 2],
            vca: None,
            mono_input: false,
        }
    }
}

/// A refused routing change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixerError {
    /// The change would feed a bus back into itself.
    Cycle,
    /// No such track, bus, send, insert or VCA.
    NoSuch(&'static str),
    /// A track cannot be keyed from a bus (the bus is downstream of it).
    KeyFromBusOnTrack,
}

impl std::fmt::Display for MixerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cycle => f.write_str("routing would create a feedback cycle"),
            Self::NoSuch(what) => write!(f, "no such {what}"),
            Self::KeyFromBusOnTrack => f.write_str("a track cannot be keyed from a bus"),
        }
    }
}

impl std::error::Error for MixerError {}

/// A fixed stereo delay line for compensation (sized at routing time).
#[derive(Debug, Clone, Default, PartialEq)]
struct Comp {
    buf: Vec<f32>,
    pos: usize,
}

impl Comp {
    fn new(frames: usize) -> Self {
        Self {
            buf: vec![0.0; frames * 2],
            pos: 0,
        }
    }

    fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        if self.buf.is_empty() {
            return (l, r);
        }
        let out = (self.buf[self.pos], self.buf[self.pos + 1]);
        self.buf[self.pos] = l;
        self.buf[self.pos + 1] = r;
        self.pos = (self.pos + 2) % self.buf.len();
        out
    }
}

struct Slot {
    fx: Insert,
    key: Option<Key>,
    bypass: bool,
    bypass_delay: Comp,
    /// This insert's own sidechain buffer, filled from its key's signal
    /// before the node's inserts run. One shared buffer — what this
    /// replaced — served a channel's second keyed insert the first one's
    /// key, whatever its own said.
    key_buf: Vec<f32>,
}

impl std::fmt::Debug for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slot")
            .field("key", &self.key)
            .field("bypass", &self.bypass)
            .field("latency", &self.fx.latency())
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct Out {
    dest: Dest,
    tap: Tap,
    level_db: f32,
    gain_now: f32,
    comp: Comp,
}

#[derive(Debug)]
struct Node {
    name: String,
    params: Params,
    inserts: Vec<Slot>,
    outs: Vec<Out>,
    pre: Vec<f32>,
    post: Vec<f32>,
    gains_now: (f32, f32),
    on_now: f32,
    primed: bool,
    meter: [PeakMeter; 2],
    latency: usize,
}

impl Node {
    fn new(name: &str, rate: f32, dest: Option<Dest>) -> Self {
        Self {
            name: name.to_owned(),
            params: Params::default(),
            inserts: Vec::new(),
            outs: dest
                .map(|d| Out {
                    dest: d,
                    tap: Tap::PostFader,
                    level_db: 0.0,
                    gain_now: 1.0,
                    comp: Comp::default(),
                })
                .into_iter()
                .collect(),
            pre: vec![0.0; MAX_BLOCK * 2],
            post: vec![0.0; MAX_BLOCK * 2],
            gains_now: (1.0, 1.0),
            on_now: 1.0,
            primed: false,
            meter: [PeakMeter::new(1.5, 20.0, rate); 2],
            latency: 0,
        }
    }

    fn insert_latency(&self) -> usize {
        self.inserts.iter().map(|s| s.fx.latency()).sum()
    }
}

fn db(x: f32) -> f32 {
    if x <= -144.0 {
        0.0
    } else {
        10f32.powf(x / 20.0)
    }
}

/// Fill a solo summary — the shared body of the per-block refresh and the
/// per-query `audible`: whether any solo is down at all, which tracks
/// reach a soloed bus (`track`), and which buses a soloed track or soloed
/// bus reaches (`bus`). Quadratic in channel count; the render path pays
/// it once per block, the UI per query — not per channel per block, as
/// the old per-channel rescan did.
fn solo_into(
    tracks: &[Node],
    buses: &[Node],
    feeds: &[bool],
    track: &mut Vec<bool>,
    bus: &mut Vec<bool>,
) -> bool {
    let (nt, nb) = (tracks.len(), buses.len());
    let any = tracks.iter().any(|n| n.params.solo) || buses.iter().any(|n| n.params.solo);
    track.clear();
    track.resize(nt, false);
    bus.clear();
    bus.resize(nb, false);
    if any {
        for t in 0..nt {
            track[t] = (0..nb).any(|b| buses[b].params.solo && feeds[t * nb + b]);
        }
        for b in 0..nb {
            bus[b] = (0..nt).any(|t| tracks[t].params.solo && feeds[t * nb + b])
                || (0..nb).any(|s| {
                    buses[s].params.solo && (feeds[(nt + s) * nb + b] || feeds[(nt + b) * nb + s])
                });
        }
    }
    any
}

/// The mixer.
#[derive(Debug)]
pub struct Mixer {
    rate: f32,
    tracks: Vec<Node>,
    buses: Vec<Node>,
    master: Node,
    vcas: Vec<f32>,
    bus_order: Vec<usize>,
    bus_in: Vec<Vec<f32>>,
    master_in: Vec<f32>,
    /// Each track's input format, refreshed once per block so an insert's
    /// input key can read any track's input without re-borrowing the
    /// track being processed.
    track_mono: Vec<bool>,
    /// Solo reachability for the block in flight: whether any solo is
    /// down, which tracks reach a soloed bus, and which buses are reached
    /// by a soloed track or soloed bus. Refreshed once per block — the
    /// per-channel rescan this replaced made every block O(channels²).
    solo_any: bool,
    solo_track: Vec<bool>,
    solo_bus: Vec<bool>,
    /// `feeds[node * n_bus + b]`: node (tracks then buses) reaches bus b.
    feeds: Vec<bool>,
    loudness: LoudnessMeter,
}

impl Mixer {
    /// An empty mixer (just the master) at `rate` Hz.
    #[must_use]
    pub fn new(rate: u32) -> Self {
        #[expect(clippy::cast_precision_loss, reason = "sample rates")]
        let r = rate as f32;
        Self {
            rate: r,
            tracks: Vec::new(),
            buses: Vec::new(),
            master: Node::new("Master", r, None),
            vcas: Vec::new(),
            bus_order: Vec::new(),
            bus_in: Vec::new(),
            master_in: vec![0.0; MAX_BLOCK * 2],
            track_mono: Vec::new(),
            solo_any: false,
            solo_track: Vec::new(),
            solo_bus: Vec::new(),
            feeds: Vec::new(),
            loudness: LoudnessMeter::new(crate::channels::Layout::Stereo, rate),
        }
    }

    /// Sample rate.
    #[must_use]
    pub const fn rate(&self) -> f32 {
        self.rate
    }

    /// Add a track routed to the master; returns its channel.
    pub fn add_track(&mut self, name: &str) -> Channel {
        self.tracks
            .push(Node::new(name, self.rate, Some(Dest::Master)));
        self.rebuild().ok();
        Channel::Track(self.tracks.len() - 1)
    }

    /// Add a bus routed to the master; returns its channel.
    pub fn add_bus(&mut self, name: &str) -> Channel {
        self.buses
            .push(Node::new(name, self.rate, Some(Dest::Master)));
        self.bus_in.push(vec![0.0; MAX_BLOCK * 2]);
        self.rebuild().ok();
        Channel::Bus(self.buses.len() - 1)
    }

    /// Add a VCA at 0 dB; returns its index.
    pub fn add_vca(&mut self) -> usize {
        self.vcas.push(0.0);
        self.vcas.len() - 1
    }

    /// Set a VCA's level.
    ///
    /// # Errors
    /// No such VCA.
    pub fn set_vca(&mut self, vca: usize, db: f32) -> Result<(), MixerError> {
        *self.vcas.get_mut(vca).ok_or(MixerError::NoSuch("VCA"))? = db;
        Ok(())
    }

    /// Number of VCAs.
    #[must_use]
    pub fn vca_count(&self) -> usize {
        self.vcas.len()
    }

    /// A VCA's level, dB.
    #[must_use]
    pub fn vca(&self, vca: usize) -> Option<f32> {
        self.vcas.get(vca).copied()
    }

    /// Number of tracks and buses.
    #[must_use]
    pub fn counts(&self) -> (usize, usize) {
        (self.tracks.len(), self.buses.len())
    }

    fn node(&self, ch: Channel) -> Result<&Node, MixerError> {
        match ch {
            Channel::Track(i) => self.tracks.get(i).ok_or(MixerError::NoSuch("track")),
            Channel::Bus(i) => self.buses.get(i).ok_or(MixerError::NoSuch("bus")),
            Channel::Master => Ok(&self.master),
        }
    }

    fn node_mut(&mut self, ch: Channel) -> Result<&mut Node, MixerError> {
        match ch {
            Channel::Track(i) => self.tracks.get_mut(i).ok_or(MixerError::NoSuch("track")),
            Channel::Bus(i) => self.buses.get_mut(i).ok_or(MixerError::NoSuch("bus")),
            Channel::Master => Ok(&mut self.master),
        }
    }

    /// A channel's parameters.
    ///
    /// # Errors
    /// No such channel.
    pub fn params(&self, ch: Channel) -> Result<&Params, MixerError> {
        Ok(&self.node(ch)?.params)
    }

    /// A channel's parameters, to change.
    ///
    /// # Errors
    /// No such channel.
    pub fn params_mut(&mut self, ch: Channel) -> Result<&mut Params, MixerError> {
        Ok(&mut self.node_mut(ch)?.params)
    }

    /// A channel's name.
    ///
    /// # Errors
    /// No such channel.
    pub fn name(&self, ch: Channel) -> Result<&str, MixerError> {
        Ok(&self.node(ch)?.name)
    }

    /// Apply a routing change, reverting it if the result is invalid.
    fn change(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<(), MixerError>,
    ) -> Result<(), MixerError> {
        f(self)?;
        self.rebuild()
    }

    /// Route a channel's main output.
    ///
    /// # Errors
    /// No such channel or bus, or a cycle (the old route is kept).
    pub fn route(&mut self, ch: Channel, dest: Dest) -> Result<(), MixerError> {
        if ch == Channel::Master {
            return Err(MixerError::NoSuch("output on the master"));
        }
        let old = self.node(ch)?.outs[0].dest;
        self.node_mut(ch)?.outs[0].dest = dest;
        self.rebuild().inspect_err(|_| {
            if let Ok(n) = self.node_mut(ch) {
                n.outs[0].dest = old;
            }
            self.rebuild().ok();
        })
    }

    /// Add a send; returns its index (send `0` is the main output, so the
    /// first send is `1`).
    ///
    /// # Errors
    /// No such channel or bus, or a cycle (nothing is added).
    pub fn add_send(
        &mut self,
        ch: Channel,
        bus: usize,
        level_db: f32,
        tap: Tap,
    ) -> Result<usize, MixerError> {
        if ch == Channel::Master {
            return Err(MixerError::NoSuch("send on the master"));
        }
        let n = self.node_mut(ch)?;
        n.outs.push(Out {
            dest: Dest::Bus(bus),
            tap,
            level_db,
            gain_now: db(level_db),
            comp: Comp::default(),
        });
        let idx = n.outs.len() - 1;
        self.rebuild().map(|()| idx).inspect_err(|_| {
            if let Ok(n) = self.node_mut(ch) {
                n.outs.pop();
            }
            self.rebuild().ok();
        })
    }

    /// Change a send's level (no reallocation).
    ///
    /// # Errors
    /// No such channel or send.
    pub fn set_send_level(
        &mut self,
        ch: Channel,
        send: usize,
        level_db: f32,
    ) -> Result<(), MixerError> {
        self.node_mut(ch)?
            .outs
            .get_mut(send)
            .ok_or(MixerError::NoSuch("send"))?
            .level_db = level_db;
        Ok(())
    }

    /// Remove a send (not the main output).
    ///
    /// # Errors
    /// No such channel or send.
    pub fn remove_send(&mut self, ch: Channel, send: usize) -> Result<(), MixerError> {
        self.change(|m| {
            let n = m.node_mut(ch)?;
            if send == 0 || send >= n.outs.len() {
                return Err(MixerError::NoSuch("send"));
            }
            n.outs.remove(send);
            Ok(())
        })
    }

    /// Append an insert effect, optionally keyed; returns its slot.
    ///
    /// # Errors
    /// No such channel, an invalid key, or a key that makes a cycle.
    pub fn add_insert(
        &mut self,
        ch: Channel,
        fx: Insert,
        key: Option<Key>,
    ) -> Result<usize, MixerError> {
        if matches!(ch, Channel::Track(_)) && matches!(key, Some(Key::Bus(_))) {
            return Err(MixerError::KeyFromBusOnTrack);
        }
        let n = self.node_mut(ch)?;
        n.inserts.push(Slot {
            fx,
            key,
            bypass: false,
            bypass_delay: Comp::default(),
            key_buf: vec![0.0; MAX_BLOCK * 2],
        });
        let idx = n.inserts.len() - 1;
        self.rebuild().map(|()| idx).inspect_err(|_| {
            if let Ok(n) = self.node_mut(ch) {
                n.inserts.pop();
            }
            self.rebuild().ok();
        })
    }

    /// Remove an insert, returning the effect.
    ///
    /// # Errors
    /// No such channel or slot.
    pub fn remove_insert(&mut self, ch: Channel, slot: usize) -> Result<Insert, MixerError> {
        let n = self.node_mut(ch)?;
        if slot >= n.inserts.len() {
            return Err(MixerError::NoSuch("insert"));
        }
        let s = n.inserts.remove(slot);
        self.rebuild()?;
        Ok(s.fx)
    }

    /// Bypass or re-enable an insert. A bypassed insert is replaced by a
    /// delay of its latency, so compensation — and the timing of the
    /// whole mix — does not move.
    ///
    /// # Errors
    /// No such channel or slot.
    pub fn set_bypass(&mut self, ch: Channel, slot: usize, bypass: bool) -> Result<(), MixerError> {
        self.node_mut(ch)?
            .inserts
            .get_mut(slot)
            .ok_or(MixerError::NoSuch("insert"))?
            .bypass = bypass;
        Ok(())
    }

    /// The insert in a slot, to adjust.
    ///
    /// # Errors
    /// No such channel or slot.
    pub fn insert_mut(&mut self, ch: Channel, slot: usize) -> Result<&mut Insert, MixerError> {
        Ok(&mut self
            .node_mut(ch)?
            .inserts
            .get_mut(slot)
            .ok_or(MixerError::NoSuch("insert"))?
            .fx)
    }

    /// Validate the graph and recompute order, compensation and solo
    /// reachability.
    fn rebuild(&mut self) -> Result<(), MixerError> {
        let nb = self.buses.len();
        let nt = self.tracks.len();
        let check = |d: Dest| match d {
            Dest::Bus(b) if b >= nb => Err(MixerError::NoSuch("bus")),
            _ => Ok(()),
        };
        for n in self.tracks.iter().chain(&self.buses) {
            for o in &n.outs {
                check(o.dest)?;
            }
            for s in &n.inserts {
                match s.key {
                    Some(Key::Input(t)) if t >= nt => return Err(MixerError::NoSuch("track")),
                    Some(Key::Bus(b)) if b >= nb => return Err(MixerError::NoSuch("bus")),
                    _ => {}
                }
            }
        }
        // Bus → bus dependencies: outputs, and keys (keyer first).
        let mut deps: Vec<Vec<usize>> = vec![Vec::new(); nb]; // deps[b]: must precede b
        for (i, n) in self.buses.iter().enumerate() {
            for o in &n.outs {
                if let Dest::Bus(d) = o.dest {
                    if d == i {
                        return Err(MixerError::Cycle);
                    }
                    deps[d].push(i);
                }
            }
            for s in &n.inserts {
                if let Some(Key::Bus(k)) = s.key {
                    if k == i {
                        return Err(MixerError::Cycle);
                    }
                    deps[i].push(k);
                }
            }
        }
        // Kahn's algorithm.
        let mut indeg: Vec<usize> = deps.iter().map(Vec::len).collect();
        let mut order = Vec::with_capacity(nb);
        let mut ready: Vec<usize> = (0..nb).filter(|b| indeg[*b] == 0).collect();
        ready.reverse();
        while let Some(b) = ready.pop() {
            order.push(b);
            for (d, ds) in deps.iter().enumerate() {
                for &src in ds {
                    if src == b {
                        indeg[d] -= 1;
                        if indeg[d] == 0 {
                            ready.push(d);
                        }
                    }
                }
            }
        }
        if order.len() != nb {
            return Err(MixerError::Cycle);
        }
        self.bus_order = order;

        // Latency: tracks, then buses in order, then the master.
        let mut arrival_bus = vec![0usize; nb];
        let mut arrival_master = 0usize;
        for n in &mut self.tracks {
            n.latency = n.insert_latency();
            for o in &n.outs {
                match o.dest {
                    Dest::Bus(b) => arrival_bus[b] = arrival_bus[b].max(n.latency),
                    Dest::Master => arrival_master = arrival_master.max(n.latency),
                }
            }
        }
        for &b in &self.bus_order {
            let n = &mut self.buses[b];
            n.latency = arrival_bus[b] + n.insert_latency();
            for o in &n.outs {
                match o.dest {
                    Dest::Bus(d) => arrival_bus[d] = arrival_bus[d].max(n.latency),
                    Dest::Master => arrival_master = arrival_master.max(n.latency),
                }
            }
        }
        self.master.latency = arrival_master + self.master.insert_latency();
        let target = |d: Dest| match d {
            Dest::Bus(b) => arrival_bus[b],
            Dest::Master => arrival_master,
        };
        for n in self.tracks.iter_mut().chain(self.buses.iter_mut()) {
            let lat = n.latency;
            for o in &mut n.outs {
                let need = target(o.dest) - lat;
                if o.comp.buf.len() != need * 2 {
                    o.comp = Comp::new(need);
                }
            }
        }
        for n in self
            .tracks
            .iter_mut()
            .chain(self.buses.iter_mut())
            .chain(std::iter::once(&mut self.master))
        {
            for s in &mut n.inserts {
                let need = s.fx.latency();
                if s.bypass_delay.buf.len() != need * 2 {
                    s.bypass_delay = Comp::new(need);
                }
            }
        }

        // Reachability for solo: node (tracks then buses) → bus.
        let mut feeds = vec![false; (nt + nb) * nb];
        let direct = |n: &Node| -> Vec<usize> {
            n.outs
                .iter()
                .filter_map(|o| match o.dest {
                    Dest::Bus(b) => Some(b),
                    Dest::Master => None,
                })
                .collect()
        };
        let bus_direct: Vec<Vec<usize>> = self.buses.iter().map(direct).collect();
        for (i, n) in self.tracks.iter().chain(&self.buses).enumerate() {
            let mut stack = direct(n);
            while let Some(b) = stack.pop() {
                if !feeds[i * nb + b] {
                    feeds[i * nb + b] = true;
                    stack.extend(&bus_direct[b]);
                }
            }
        }
        self.feeds = feeds;
        Ok(())
    }

    /// Total latency of the mix at the master output, in samples.
    #[must_use]
    pub const fn latency(&self) -> usize {
        self.master.latency
    }

    /// Whether a channel is heard, given mutes and solos. A UI query:
    /// answers for the state right now, building this call's own solo
    /// summary — allocating, which is fine off the render path. The render
    /// path cannot afford the quadratic rescan per channel per block, so
    /// `block` refreshes its own summary once and answers in constant
    /// time.
    #[must_use]
    pub fn audible(&self, ch: Channel) -> bool {
        let mut track = Vec::with_capacity(self.tracks.len());
        let mut bus = Vec::with_capacity(self.buses.len());
        let any = self.solo_into(&mut track, &mut bus);
        self.audible_with(ch, any, &track, &bus)
    }

    /// Whether a channel is heard, answered against a solo summary.
    fn audible_with(&self, ch: Channel, any: bool, track: &[bool], bus: &[bool]) -> bool {
        let Ok(node) = self.node(ch) else {
            return false;
        };
        if node.params.mute {
            return false;
        }
        if !any || node.params.solo || node.params.solo_safe {
            return true;
        }
        match ch {
            Channel::Master => true,
            Channel::Track(t) => track[t],
            // Downstream of a soloed channel, or upstream of a soloed bus.
            Channel::Bus(b) => bus[b],
        }
    }

    /// Answer the solo questions `audible` asks, once for every channel:
    /// whether any solo is down at all, which tracks reach a soloed bus,
    /// and which buses a soloed track or soloed bus reaches. Filling is
    /// quadratic in channel count — once per block (or per UI query) that
    /// is nothing; per channel per block, as the old `audible` rescan was,
    /// it dominated the whole render on channel-count-heavy mixes.
    fn solo_into(&self, track: &mut Vec<bool>, bus: &mut Vec<bool>) -> bool {
        solo_into(&self.tracks, &self.buses, &self.feeds, track, bus)
    }

    /// A channel's post-fader peak meter, `(left dB, right dB)`.
    ///
    /// # Errors
    /// No such channel.
    pub fn meter(&self, ch: Channel) -> Result<(f32, f32), MixerError> {
        let n = self.node(ch)?;
        Ok((n.meter[0].db(), n.meter[1].db()))
    }

    /// A channel's held peaks and whether it clipped.
    ///
    /// # Errors
    /// No such channel.
    pub fn meter_held(&self, ch: Channel) -> Result<(f32, f32, bool), MixerError> {
        let n = self.node(ch)?;
        Ok((
            n.meter[0].held_db(),
            n.meter[1].held_db(),
            n.meter[0].clipped() || n.meter[1].clipped(),
        ))
    }

    /// The master's loudness meter (EBU R128).
    #[must_use]
    pub const fn loudness(&self) -> &LoudnessMeter {
        &self.loudness
    }

    /// Mix. `inputs[t]` is track `t`'s input — interleaved stereo, or mono
    /// for a `mono_input` track — for the same number of frames as `out`
    /// (interleaved stereo) holds. Missing inputs are silence. Never
    /// allocates.
    pub fn process(&mut self, inputs: &[&[f32]], out: &mut [f32]) {
        let frames = out.len() / 2;
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(MAX_BLOCK);
            self.block(inputs, done, n, &mut out[done * 2..(done + n) * 2]);
            done += n;
        }
    }

    fn block(&mut self, inputs: &[&[f32]], offset: usize, n: usize, out: &mut [f32]) {
        for b in &mut self.bus_in {
            b[..n * 2].fill(0.0);
        }
        self.master_in[..n * 2].fill(0.0);
        // Answered once for the block: every track's input format, and
        // the solo summary the per-channel `audible` rescan used to
        // recompute per channel — quadratically.
        self.track_mono.clear();
        self.track_mono
            .extend(self.tracks.iter().map(|t| t.params.mono_input));
        self.solo_any = solo_into(
            &self.tracks,
            &self.buses,
            &self.feeds,
            &mut self.solo_track,
            &mut self.solo_bus,
        );

        // Tracks.
        for t in 0..self.tracks.len() {
            let on = self.audible_with(
                Channel::Track(t),
                self.solo_any,
                &self.solo_track,
                &self.solo_bus,
            );
            let vca = self.vca_db(&self.tracks[t].params);
            let node = &mut self.tracks[t];
            let mono = node.params.mono_input;
            let src = inputs.get(t).copied().unwrap_or(&[]);
            for f in 0..n {
                let (l, r) = read_input(src, mono, offset + f);
                node.pre[f * 2] = l;
                node.pre[f * 2 + 1] = r;
            }
            run_inserts(node, n, offset, inputs, &self.track_mono);
            finish_node(node, n, on, vca, mono);
            distribute(node, n, &mut self.bus_in, &mut self.master_in);
        }

        // Buses, in dependency order.
        for oi in 0..self.bus_order.len() {
            let b = self.bus_order[oi];
            let on = self.audible_with(
                Channel::Bus(b),
                self.solo_any,
                &self.solo_track,
                &self.solo_bus,
            );
            let vca = self.vca_db(&self.buses[b].params);
            // Each keyed insert's own buffer is filled from its key bus's
            // `pre` — the keying bus is earlier in the order, so that is
            // this block's signal — before this bus is borrowed for
            // processing. The single shared buffer this replaced served
            // every keyed insert on the bus the first one's key, whatever
            // the others asked for.
            let (before, rest) = self.buses.split_at_mut(b);
            let Some((node, after)) = rest.split_first_mut() else {
                continue; // bus indices are validated by rebuild
            };
            node.pre[..n * 2].copy_from_slice(&self.bus_in[b][..n * 2]);
            for s in node.inserts.iter_mut() {
                if let Some(Key::Bus(k)) = s.key {
                    if k == b {
                        continue; // self-keys are refused at add_insert
                    }
                    let src = if k < b {
                        &before[k].pre
                    } else {
                        &after[k - b - 1].pre
                    };
                    s.key_buf[..n * 2].copy_from_slice(&src[..n * 2]);
                }
            }
            run_inserts(node, n, offset, inputs, &self.track_mono);
            finish_node(node, n, on, vca, false);
            distribute(node, n, &mut self.bus_in, &mut self.master_in);
        }

        // Master. Its keyed inserts were silently dead before — every key
        // read zeros; they key off buses (all processed, this block) and
        // raw track inputs, like a bus's.
        let vca = self.vca_db(&self.master.params);
        let m = &mut self.master;
        m.pre[..n * 2].copy_from_slice(&self.master_in[..n * 2]);
        {
            let buses = &self.buses;
            for s in m.inserts.iter_mut() {
                if let Some(Key::Bus(k)) = s.key {
                    if let Some(src) = buses.get(k) {
                        s.key_buf[..n * 2].copy_from_slice(&src.pre[..n * 2]);
                    }
                }
            }
        }
        run_inserts(m, n, offset, inputs, &self.track_mono);
        let on = !m.params.mute;
        finish_node(m, n, on, vca, false);
        out[..n * 2].copy_from_slice(&m.post[..n * 2]);
        self.loudness.push(&out[..n * 2]);
    }

    fn vca_db(&self, p: &Params) -> f32 {
        p.vca.and_then(|v| self.vcas.get(v)).copied().unwrap_or(0.0)
    }
}

/// Frame `i` of an input buffer, as stereo.
fn read_input(src: &[f32], mono: bool, i: usize) -> (f32, f32) {
    if mono {
        let v = src.get(i).copied().unwrap_or(0.0);
        (v, v)
    } else {
        (
            src.get(i * 2).copied().unwrap_or(0.0),
            src.get(i * 2 + 1).copied().unwrap_or(0.0),
        )
    }
}

/// Run a node's inserts over `pre` in place. An input key reads the keying
/// track's raw input, in that track's own format (`mono`); a bus key reads
/// the insert's own `key_buf`, filled from the keying bus's `pre` before
/// this node was borrowed.
fn run_inserts(node: &mut Node, n: usize, offset: usize, inputs: &[&[f32]], mono: &[bool]) {
    for f in 0..n {
        let (mut l, mut r) = (node.pre[f * 2], node.pre[f * 2 + 1]);
        if node.params.invert[0] {
            l = -l;
        }
        if node.params.invert[1] {
            r = -r;
        }
        for s in &mut node.inserts {
            (l, r) = if s.bypass {
                s.bypass_delay.tick(l, r)
            } else if let Some(k) = s.key {
                let key = match k {
                    // A key is read in its own track's format.
                    Key::Input(i) => read_input(
                        inputs.get(i).copied().unwrap_or(&[]),
                        mono.get(i).copied().unwrap_or(false),
                        offset + f,
                    ),
                    // Tracks refuse bus keys at `add_insert`, so this arm
                    // runs for buses and the master, whose key buffers
                    // `block` fills.
                    Key::Bus(_) => (s.key_buf[f * 2], s.key_buf[f * 2 + 1]),
                };
                s.fx.process_stereo_with_key(l, r, key)
            } else {
                s.fx.process_stereo(l, r)
            };
        }
        node.pre[f * 2] = l;
        node.pre[f * 2 + 1] = r;
    }
}

/// Width, fader, pan and mute: `pre` → `post`, ramped across the block.
fn finish_node(node: &mut Node, n: usize, on: bool, vca_db: f32, mono: bool) {
    let p = node.params;
    let law = if mono && p.pan_law == PanLaw::Balance {
        PanLaw::ConstantPower
    } else {
        p.pan_law
    };
    let (pl, pr) = law.gains(p.pan);
    let g = db(p.fader_db + vca_db);
    let target = (g * pl, g * pr);
    let on_target = if on { 1.0 } else { 0.0 };
    if !node.primed {
        node.gains_now = target;
        node.on_now = on_target;
        for o in &mut node.outs {
            o.gain_now = db(o.level_db);
        }
        node.primed = true;
    }
    #[expect(clippy::cast_precision_loss, reason = "block length")]
    let inv = 1.0 / n.max(1) as f32;
    let (g0, on0) = (node.gains_now, node.on_now);
    for f in 0..n {
        #[expect(clippy::cast_precision_loss, reason = "block position")]
        let t = (f + 1) as f32 * inv;
        let on = on0 + (on_target - on0) * t;
        let gl = g0.0 + (target.0 - g0.0) * t;
        let gr = g0.1 + (target.1 - g0.1) * t;
        let (mut l, mut r) = (node.pre[f * 2] * on, node.pre[f * 2 + 1] * on);
        if (p.width - 1.0).abs() > f32::EPSILON {
            let m = (l + r) * 0.5;
            let s = (l - r) * 0.5 * p.width.max(0.0);
            (l, r) = (m + s, m - s);
        }
        node.pre[f * 2] = l;
        node.pre[f * 2 + 1] = r;
        let (ol, or) = (l * gl, r * gr);
        node.post[f * 2] = ol;
        node.post[f * 2 + 1] = or;
        node.meter[0].tick(ol);
        node.meter[1].tick(or);
    }
    node.gains_now = target;
    node.on_now = on_target;
}

/// Send a node's outputs (main and sends) to their destinations.
fn distribute(node: &mut Node, n: usize, bus_in: &mut [Vec<f32>], master_in: &mut [f32]) {
    #[expect(clippy::cast_precision_loss, reason = "block length")]
    let inv = 1.0 / n.max(1) as f32;
    for o in &mut node.outs {
        let target = db(o.level_db);
        let g0 = o.gain_now;
        let src = match o.tap {
            Tap::PreFader => &node.pre,
            Tap::PostFader => &node.post,
        };
        let dst: &mut [f32] = match o.dest {
            Dest::Master => master_in,
            Dest::Bus(b) => &mut bus_in[b],
        };
        for f in 0..n {
            #[expect(clippy::cast_precision_loss, reason = "block position")]
            let g = g0 + (target - g0) * (f + 1) as f32 * inv;
            let (l, r) = o.comp.tick(src[f * 2] * g, src[f * 2 + 1] * g);
            dst[f * 2] += l;
            dst[f * 2 + 1] += r;
        }
        o.gain_now = target;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::dynamics::{Dynamics, Limiter};

    fn sine(hz: f32, frames: usize, amp: f32) -> Vec<f32> {
        (0..frames)
            .flat_map(|i| {
                let v = amp * (std::f32::consts::TAU * hz * i as f32 / 48_000.0).sin();
                [v, v]
            })
            .collect()
    }

    fn run(m: &mut Mixer, inputs: &[&[f32]]) -> Vec<f32> {
        let frames = inputs.iter().map(|i| i.len() / 2).max().unwrap_or(0);
        let mut out = vec![0.0; frames * 2];
        m.process(inputs, &mut out);
        out
    }

    fn max_diff(a: &[f32], b: &[f32]) -> f32 {
        a.iter()
            .zip(b)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f32::max)
    }

    #[test]
    fn a_flat_track_passes_its_input_exactly() {
        let mut m = Mixer::new(48_000);
        m.add_track("a");
        let x = sine(440.0, 1_000, 0.5);
        assert_eq!(run(&mut m, &[&x]), x);
    }

    #[test]
    fn fader_and_pan_laws() {
        let mut m = Mixer::new(48_000);
        let t = m.add_track("a");
        m.params_mut(t).unwrap().fader_db = -6.020_6;
        let x = vec![1.0f32; 64];
        let y = run(&mut m, &[&x]);
        assert!((y[0] - 0.5).abs() < 1e-4);
        m.params_mut(t).unwrap().fader_db = 0.0;
        m.params_mut(t).unwrap().pan = -1.0;
        run(&mut m, &[&x]); // ramp
        let y = run(&mut m, &[&x]);
        assert!((y[0] - 1.0).abs() < 1e-6 && y[1].abs() < 1e-6);
        // Laws at centre.
        assert_eq!(PanLaw::Balance.gains(0.0), (1.0, 1.0));
        let (l, r) = PanLaw::ConstantPower.gains(0.0);
        assert!((l - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6 && (r - l).abs() < 1e-6);
        let (l, _) = PanLaw::CompensatedConstantPower.gains(0.0);
        assert!((l - 1.0).abs() < 1e-6);
        assert_eq!(PanLaw::Linear.gains(0.0), (0.5, 0.5));
        // Constant power keeps power constant across the field.
        for p in [-1.0, -0.3, 0.4, 1.0] {
            let (l, r) = PanLaw::ConstantPower.gains(p);
            assert!((l * l + r * r - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn mono_inputs_pan_with_constant_power() {
        let mut m = Mixer::new(48_000);
        let t = m.add_track("mono");
        m.params_mut(t).unwrap().mono_input = true;
        let mut out = vec![0.0; 32];
        m.process(&[&[1.0f32; 16]], &mut out);
        assert!((out[0] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5);
        assert!((out[1] - out[0]).abs() < 1e-7);
    }

    #[test]
    fn width_zero_is_mono_and_invert_cancels() {
        let mut m = Mixer::new(48_000);
        let t = m.add_track("w");
        m.params_mut(t).unwrap().width = 0.0;
        let y = run(&mut m, &[&[1.0, 0.0, 1.0, 0.0]]);
        assert_eq!(y, vec![0.5, 0.5, 0.5, 0.5]);

        let mut m = Mixer::new(48_000);
        let a = m.add_track("a");
        m.add_track("b");
        m.params_mut(a).unwrap().invert = [true, true];
        let x = sine(300.0, 512, 0.7);
        let y = run(&mut m, &[&x, &x]);
        assert!(y.iter().all(|v| v.abs() < 1e-6));
    }

    #[test]
    fn mute_ramps_without_a_click() {
        let mut m = Mixer::new(48_000);
        let t = m.add_track("a");
        let x = vec![1.0f32; 4 * MAX_BLOCK];
        run(&mut m, &[&x]);
        m.params_mut(t).unwrap().mute = true;
        let y = run(&mut m, &[&x]);
        // The first block ramps 1 → 0 across MAX_BLOCK frames…
        assert!(y.windows(2).all(|w| (w[0] - w[1]).abs() < 0.01));
        assert!(y[0] > 0.99 && y[2 * (MAX_BLOCK - 1)] < 0.01);
        // …and from then on it is silent.
        assert!(y[2 * MAX_BLOCK..].iter().all(|v| *v == 0.0));
        assert!(!m.audible(t));
    }

    #[test]
    fn solo_follows_signal_flow() {
        let mut m = Mixer::new(48_000);
        let a = m.add_track("a");
        let b = m.add_track("b");
        let c = m.add_track("c");
        let drums = m.add_bus("drums");
        let verb = m.add_bus("verb");
        m.route(b, Dest::Bus(0)).unwrap();
        m.route(c, Dest::Bus(0)).unwrap();
        m.add_send(a, 1, -6.0, Tap::PostFader).unwrap();
        m.params_mut(verb).unwrap().solo_safe = true;

        m.params_mut(a).unwrap().solo = true;
        assert!(m.audible(a) && !m.audible(b) && !m.audible(c));
        assert!(!m.audible(drums));
        assert!(m.audible(verb), "solo-safe");

        m.params_mut(a).unwrap().solo = false;
        m.params_mut(drums).unwrap().solo = true;
        assert!(!m.audible(a) && m.audible(b) && m.audible(c) && m.audible(drums));

        m.params_mut(drums).unwrap().solo = false;
        m.params_mut(b).unwrap().solo = true;
        assert!(m.audible(drums), "a soloed track keeps its bus");
        assert!(!m.audible(c));

        // And in the audio: only b is heard.
        m.params_mut(verb).unwrap().solo_safe = false;
        let one = vec![1.0f32; 256];
        let zero = vec![0.0f32; 256];
        run(&mut m, &[&zero, &one, &zero]);
        let y = run(&mut m, &[&zero, &one, &zero]);
        assert!((y[10] - 1.0).abs() < 1e-6);
        let y = run(&mut m, &[&one, &zero, &one]);
        // a and c are silent, so is everything.
        assert!(y[200..].iter().all(|v| v.abs() < 1e-6));
    }

    #[test]
    fn pre_and_post_fader_sends() {
        let mut m = Mixer::new(48_000);
        let t = m.add_track("t");
        let pre = m.add_bus("pre");
        let post = m.add_bus("post");
        m.add_send(t, 0, 0.0, Tap::PreFader).unwrap();
        m.add_send(t, 1, 0.0, Tap::PostFader).unwrap();
        m.route(t, Dest::Bus(1)).unwrap();
        m.route(post, Dest::Master).unwrap();
        m.params_mut(t).unwrap().fader_db = -200.0; // fader down
        m.params_mut(post).unwrap().mute = true;
        let one = vec![1.0f32; 256];
        let y = run(&mut m, &[&one]);
        assert!(
            (y[0] - 1.0).abs() < 1e-6,
            "pre-fader send ignores the fader"
        );
        m.params_mut(pre).unwrap().mute = true;
        m.params_mut(post).unwrap().mute = false;
        m.params_mut(t).unwrap().fader_db = 0.0;
        run(&mut m, &[&one]);
        let y = run(&mut m, &[&one]);
        // Main output and post send both land on `post`: 2.0.
        assert!((y[0] - 2.0).abs() < 1e-5, "{}", y[0]);
        m.set_send_level(t, 2, -6.020_6).unwrap();
        run(&mut m, &[&one]);
        let y = run(&mut m, &[&one]);
        assert!((y[0] - 1.5).abs() < 1e-4, "{}", y[0]);
    }

    #[test]
    fn bus_chains_and_cycles_are_refused() {
        let mut m = Mixer::new(48_000);
        let t = m.add_track("t");
        let b0 = m.add_bus("b0");
        let b1 = m.add_bus("b1");
        m.route(t, Dest::Bus(0)).unwrap();
        m.route(b0, Dest::Bus(1)).unwrap();
        m.params_mut(b1).unwrap().fader_db = -6.020_6;
        assert_eq!(m.route(b1, Dest::Bus(0)), Err(MixerError::Cycle));
        assert_eq!(
            m.add_send(b1, 0, 0.0, Tap::PostFader),
            Err(MixerError::Cycle)
        );
        assert_eq!(m.route(b0, Dest::Bus(0)), Err(MixerError::Cycle));
        assert_eq!(m.route(t, Dest::Bus(9)), Err(MixerError::NoSuch("bus")));
        // Still routed t → b0 → b1 → master.
        let y = run(&mut m, &[&[1.0f32; 64]]);
        assert!((y[0] - 0.5).abs() < 1e-4);
        assert_eq!(m.name(b0).unwrap(), "b0");
    }

    #[test]
    fn delay_compensation_aligns_a_lookahead_limiter() {
        let mut m = Mixer::new(48_000);
        let a = m.add_track("limited");
        m.add_track("dry");
        let lim = Limiter::new(0.0, 0.002, 0.05, 48_000.0);
        let lat = lim.latency();
        m.add_insert(a, Box::new(lim), None).unwrap();
        assert_eq!(m.latency(), lat);
        let mut imp = vec![0.0f32; 1_024];
        imp[2 * 100] = 0.5;
        imp[2 * 100 + 1] = 0.5;
        let y = run(&mut m, &[&imp, &imp]);
        // Both impulses arrive at 100 + latency, summed.
        let at = 100 + lat;
        assert!((y[2 * at] - 1.0).abs() < 1e-5, "{}", y[2 * at]);
        let rest: f32 = y.iter().map(|v| v.abs()).sum::<f32>() - y[2 * at] - y[2 * at + 1];
        assert!(rest < 1e-4, "{rest}");
        // Bypass keeps timing.
        m.set_bypass(a, 0, true).unwrap();
        let y = run(&mut m, &[&imp, &imp]);
        assert!((y[2 * at] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn sidechain_ducking() {
        let mut m = Mixer::new(48_000);
        let pad = m.add_track("pad");
        m.add_track("kick");
        m.params_mut(Channel::Track(1)).unwrap().mute = true;
        let comp = Dynamics::compressor(-30.0, 20.0, 0.001, 0.05, 48_000.0);
        m.add_insert(pad, Box::new(comp), Some(Key::Input(1)))
            .unwrap();
        let p = sine(200.0, 9_600, 0.3);
        let quiet = vec![0.0f32; 9_600 * 2];
        let kick = sine(60.0, 9_600, 0.9);
        let free = run(&mut m, &[&p, &quiet]);
        let ducked = run(&mut m, &[&p, &kick]);
        let rms = |v: &[f32]| (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt();
        assert!(
            rms(&ducked[4_800..]) < rms(&free[4_800..]) * 0.3,
            "{} vs {}",
            rms(&ducked[4_800..]),
            rms(&free[4_800..])
        );
        assert_eq!(
            m.add_insert(
                pad,
                Box::new(Limiter::new(0.0, 0.001, 0.1, 48_000.0)),
                Some(Key::Bus(0))
            ),
            Err(MixerError::KeyFromBusOnTrack)
        );
    }

    #[test]
    fn each_keyed_insert_on_a_bus_gets_its_own_key() {
        // Two keyed compressors on one bus, keyed by different buses. The
        // single shared key buffer this fixed served both the first
        // insert's key — so with the first key silent and the second loud,
        // neither ducked; each insert now reads its own.
        let mut m = Mixer::new(48_000);
        let music = m.add_track("music");
        let kick_a = m.add_track("kick a");
        let kick_b = m.add_track("kick b");
        let bed = m.add_bus("bed"); // bus 0: the keyed inserts
        let ka = m.add_bus("ka"); // bus 1: first insert's key
        let kb = m.add_bus("kb"); // bus 2: second insert's key
        let sink = m.add_bus("sink"); // bus 3: muted — the keys go nowhere
        m.route(music, Dest::Bus(0)).unwrap();
        m.route(kick_a, Dest::Bus(1)).unwrap();
        m.route(kick_b, Dest::Bus(2)).unwrap();
        m.route(ka, Dest::Bus(3)).unwrap();
        m.route(kb, Dest::Bus(3)).unwrap();
        m.params_mut(sink).unwrap().mute = true;
        let comp = || Dynamics::compressor(-30.0, 20.0, 0.001, 0.05, 48_000.0);
        m.add_insert(bed, Box::new(comp()), Some(Key::Bus(1)))
            .unwrap();
        m.add_insert(bed, Box::new(comp()), Some(Key::Bus(2)))
            .unwrap();
        let p = sine(200.0, 9_600, 0.3);
        let quiet = vec![0.0f32; 9_600 * 2];
        let kick = sine(60.0, 9_600, 0.9);
        let rms = |v: &[f32]| (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt();
        let free = run(&mut m, &[&p, &quiet, &quiet]);
        // The second insert's key is loud, the first's silent: the bed
        // ducks through the second compressor.
        let ducked_by_b = run(&mut m, &[&p, &quiet, &kick]);
        assert!(
            rms(&ducked_by_b[4_800..]) < rms(&free[4_800..]) * 0.3,
            "second insert's own key: {} vs {}",
            rms(&ducked_by_b[4_800..]),
            rms(&free[4_800..])
        );
        // And symmetrically, the first's key loud and the second's silent.
        let ducked_by_a = run(&mut m, &[&p, &kick, &quiet]);
        assert!(
            rms(&ducked_by_a[4_800..]) < rms(&free[4_800..]) * 0.3,
            "first insert's own key: {} vs {}",
            rms(&ducked_by_a[4_800..]),
            rms(&free[4_800..])
        );
    }

    #[test]
    fn master_inserts_take_keys() {
        // A keyed insert on the master — a broadcast ducker. Every key
        // used to read exact zeros here, whatever it asked for.
        let mut m = Mixer::new(48_000);
        m.add_track("pad");
        let dj = m.add_track("dj"); // muted: keys the master, is not summed
        m.params_mut(dj).unwrap().mute = true;
        let comp = Dynamics::compressor(-30.0, 20.0, 0.001, 0.05, 48_000.0);
        m.add_insert(Channel::Master, Box::new(comp), Some(Key::Input(1)))
            .unwrap();
        let p = sine(200.0, 9_600, 0.3);
        let quiet = vec![0.0f32; 9_600 * 2];
        let speech = sine(400.0, 9_600, 0.9);
        let free = run(&mut m, &[&p, &quiet]);
        let ducked = run(&mut m, &[&p, &speech]);
        let rms = |v: &[f32]| (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt();
        assert!(
            rms(&ducked[4_800..]) < rms(&free[4_800..]) * 0.3,
            "the master ducks on its key: {} vs {}",
            rms(&ducked[4_800..]),
            rms(&free[4_800..])
        );
    }

    #[test]
    fn master_inserts_key_off_buses_too() {
        // Same, keyed by a bus's signal instead of a raw input: the voice
        // bus feeds a muted sink (so the master sums none of it), and its
        // pre keys the ducker.
        let mut m = Mixer::new(48_000);
        m.add_track("pad");
        let dj = m.add_track("dj");
        let voice = m.add_bus("voice"); // bus 0: the key
        let sink = m.add_bus("sink"); // bus 1: muted, voice goes nowhere
        m.route(dj, Dest::Bus(0)).unwrap();
        m.route(voice, Dest::Bus(1)).unwrap();
        m.params_mut(sink).unwrap().mute = true;
        let comp = Dynamics::compressor(-30.0, 20.0, 0.001, 0.05, 48_000.0);
        m.add_insert(Channel::Master, Box::new(comp), Some(Key::Bus(0)))
            .unwrap();
        let p = sine(200.0, 9_600, 0.3);
        let quiet = vec![0.0f32; 9_600 * 2];
        let speech = sine(400.0, 9_600, 0.9);
        let free = run(&mut m, &[&p, &quiet]);
        let ducked = run(&mut m, &[&p, &speech]);
        let rms = |v: &[f32]| (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt();
        assert!(
            rms(&ducked[4_800..]) < rms(&free[4_800..]) * 0.3,
            "the master ducks on the bus key: {} vs {}",
            rms(&ducked[4_800..]),
            rms(&free[4_800..])
        );
    }

    #[test]
    fn vcas_and_meters() {
        let mut m = Mixer::new(48_000);
        let a = m.add_track("a");
        let b = m.add_track("b");
        let v = m.add_vca();
        m.params_mut(a).unwrap().vca = Some(v);
        m.params_mut(b).unwrap().vca = Some(v);
        m.set_vca(v, -6.020_6).unwrap();
        let one = vec![0.5f32; 128];
        let y = run(&mut m, &[&one, &one]);
        assert!((y[0] - 0.5).abs() < 1e-4);
        let (l, r) = m.meter(a).unwrap();
        assert!((l + 12.04).abs() < 0.05 && (r - l).abs() < 1e-4);
        let (l, _) = m.meter(Channel::Master).unwrap();
        assert!((l + 6.02).abs() < 0.05);
        assert!(m.set_vca(9, 0.0).is_err());
        let (_, _, clipped) = m.meter_held(Channel::Master).unwrap();
        assert!(!clipped);
    }

    #[test]
    fn master_loudness_is_metered() {
        let mut m = Mixer::new(48_000);
        m.add_track("tone");
        let amp = 10f32.powf(-23.0 / 20.0);
        let x = sine(1_000.0, 48_000 * 5, amp);
        run(&mut m, &[&x]);
        assert!((m.loudness().integrated() + 23.0).abs() < 0.1);
    }

    #[test]
    fn long_calls_split_into_blocks_and_missing_inputs_are_silence() {
        let mut m = Mixer::new(48_000);
        m.add_track("a");
        m.add_track("b");
        let x = sine(100.0, 3 * MAX_BLOCK + 17, 0.25);
        let mut out = vec![0.0; x.len()];
        m.process(&[&x], &mut out);
        assert!(max_diff(&out, &x) < 1e-7);
        assert_eq!(m.counts(), (2, 0));
    }

    #[test]
    fn inserts_can_be_removed_and_reached() {
        let mut m = Mixer::new(48_000);
        let t = m.add_track("t");
        let s = m
            .add_insert(t, Box::new(Limiter::new(-6.0, 0.001, 0.1, 48_000.0)), None)
            .unwrap();
        assert!(m.latency() > 0);
        assert!(m.insert_mut(t, s).is_ok());
        m.remove_insert(t, s).unwrap();
        assert_eq!(m.latency(), 0);
        assert!(m.remove_insert(t, 0).is_err());
        assert!(m.remove_send(t, 0).is_err());
    }
}
