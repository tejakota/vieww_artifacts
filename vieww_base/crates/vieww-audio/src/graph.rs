//! A node-based audio graph — Web Audio's `AudioNode` model.
//!
//! Nodes are sources (oscillators, buffers, constants), processors (any
//! [`Processor`] per channel, any [`Stereo`] effect), gains with automatable
//! [`Param`]s, panners, delays and closures. Connections run from any node
//! to any node, each with its own gain — which is all a *send* is — and a
//! node's input is the sum of everything connected to it, up- or down-mixed
//! to the node's channel count with the [`channels`](crate::channels) rules.
//! A *bus* is a gain node with several inputs.
//!
//! # Rendering
//!
//! The graph renders in fixed blocks ([`BLOCK`] frames — Web Audio's render
//! quantum). Every buffer is allocated when a node is added, so
//! [`Graph::render_block`] allocates nothing: it can run on a real-time
//! audio thread. Order is a topological sort computed at connection time.
//!
//! # Feedback
//!
//! As in Web Audio, a cycle is legal only if it passes through a
//! [`DelayNode`]. Delay nodes run first in every block and read their
//! inputs' *previous* block, so they add exactly one block of latency, which
//! they subtract from their own delay — a delay of 300 ms is 300 ms whether
//! or not it sits in a loop. A cycle without a delay is refused at
//! [`Graph::connect`].

use crate::automation::Param;
use crate::channels::{remix, Layout};
use crate::dsp::Processor;
use crate::fx::Stereo;
use crate::synth::Generator;

/// Frames per render block.
pub const BLOCK: usize = 128;

/// A node's identity in its graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(usize);

/// What a node knows about the block it is rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Block {
    /// Sample rate.
    pub rate: f32,
    /// Time of the block's first frame, in seconds.
    pub time: f64,
    /// Index of the block's first frame.
    pub frame: u64,
}

/// A graph node.
pub trait Node: Send {
    /// Input channel count (0 for sources).
    fn inputs(&self) -> usize;
    /// Output channel count.
    fn outputs(&self) -> usize;
    /// Render one block: `input` is planar (`inputs()` channels of
    /// [`BLOCK`]), `output` likewise.
    fn process(&mut self, input: &[[f32; BLOCK]], output: &mut [[f32; BLOCK]], block: &Block);
    /// True only for nodes that may close a feedback cycle.
    fn is_delay(&self) -> bool {
        false
    }
}

struct Slot {
    node: Box<dyn Node>,
    input: Vec<[f32; BLOCK]>,
    output: Vec<[f32; BLOCK]>,
    /// Scratch for remixing one upstream frame.
    frame_in: Vec<f32>,
    frame_out: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Edge {
    from: usize,
    to: usize,
    gain: f32,
}

/// Why a connection was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphError {
    /// The connection would close a cycle with no delay node in it.
    CycleWithoutDelay,
    /// A node id from another graph, or removed.
    UnknownNode,
    /// The target takes no input.
    NoInput,
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::CycleWithoutDelay => "a feedback cycle needs a DelayNode in it",
            Self::UnknownNode => "unknown node",
            Self::NoInput => "that node takes no input",
        })
    }
}

impl std::error::Error for GraphError {}

/// An audio graph.
pub struct Graph {
    slots: Vec<Slot>,
    edges: Vec<Edge>,
    order: Vec<usize>,
    output: Option<usize>,
    rate: f32,
    frame: u64,
}

impl std::fmt::Debug for Graph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Graph")
            .field("nodes", &self.slots.len())
            .field("edges", &self.edges)
            .field("order", &self.order)
            .finish_non_exhaustive()
    }
}

impl Graph {
    /// An empty graph at `rate`.
    #[must_use]
    pub const fn new(rate: f32) -> Self {
        Self {
            slots: Vec::new(),
            edges: Vec::new(),
            order: Vec::new(),
            output: None,
            rate,
            frame: 0,
        }
    }

    /// The sample rate.
    #[must_use]
    pub const fn rate(&self) -> f32 {
        self.rate
    }

    /// Add a node.
    pub fn add(&mut self, node: impl Node + 'static) -> NodeId {
        let i = node.inputs();
        let o = node.outputs();
        self.slots.push(Slot {
            node: Box::new(node),
            input: vec![[0.0; BLOCK]; i],
            output: vec![[0.0; BLOCK]; o],
            frame_in: vec![0.0; o.max(8)],
            frame_out: vec![0.0; i.max(8)],
        });
        let id = self.slots.len() - 1;
        self.order.push(id);
        self.reorder().ok();
        NodeId(id)
    }

    /// Choose the node whose output [`render_block`](Self::render_block)
    /// returns (the "destination").
    pub fn set_output(&mut self, id: NodeId) {
        self.output = Some(id.0);
    }

    /// Connect `from` → `to` at unity gain.
    ///
    /// # Errors
    /// See [`GraphError`].
    pub fn connect(&mut self, from: NodeId, to: NodeId) -> Result<(), GraphError> {
        self.connect_gain(from, to, 1.0)
    }

    /// Connect with a gain — a send.
    ///
    /// # Errors
    /// See [`GraphError`].
    pub fn connect_gain(&mut self, from: NodeId, to: NodeId, gain: f32) -> Result<(), GraphError> {
        if from.0 >= self.slots.len() || to.0 >= self.slots.len() {
            return Err(GraphError::UnknownNode);
        }
        if self.slots[to.0].node.inputs() == 0 {
            return Err(GraphError::NoInput);
        }
        self.edges.push(Edge {
            from: from.0,
            to: to.0,
            gain,
        });
        if let Err(e) = self.reorder() {
            self.edges.pop();
            self.reorder().ok();
            return Err(e);
        }
        Ok(())
    }

    /// Remove every connection `from` → `to`.
    pub fn disconnect(&mut self, from: NodeId, to: NodeId) {
        self.edges.retain(|e| !(e.from == from.0 && e.to == to.0));
        self.reorder().ok();
    }

    /// Change a connection's gain (a send level).
    pub fn set_send(&mut self, from: NodeId, to: NodeId, gain: f32) {
        for e in &mut self.edges {
            if e.from == from.0 && e.to == to.0 {
                e.gain = gain;
            }
        }
    }

    /// Topological order with delay nodes first; edges *into* delay nodes
    /// do not constrain the order (delays read the previous block).
    fn reorder(&mut self) -> Result<(), GraphError> {
        let n = self.slots.len();
        let is_delay: Vec<bool> = self.slots.iter().map(|s| s.node.is_delay()).collect();
        let mut indeg = vec![0usize; n];
        for e in &self.edges {
            if !is_delay[e.to] {
                indeg[e.to] += 1;
            }
        }
        let mut order: Vec<usize> = (0..n).filter(|i| is_delay[*i]).collect();
        let mut ready: Vec<usize> = (0..n).filter(|i| !is_delay[*i] && indeg[*i] == 0).collect();
        // Delay nodes' outputs are available from the start.
        for &d in &order {
            for e in self.edges.iter().filter(|e| e.from == d && !is_delay[e.to]) {
                indeg[e.to] -= 1;
                if indeg[e.to] == 0 {
                    ready.push(e.to);
                }
            }
        }
        ready.sort_unstable();
        ready.dedup();
        let mut placed = vec![false; n];
        for &d in &order {
            placed[d] = true;
        }
        while let Some(i) = ready.pop() {
            if placed[i] {
                continue;
            }
            placed[i] = true;
            order.push(i);
            for e in self.edges.iter().filter(|e| e.from == i && !is_delay[e.to]) {
                indeg[e.to] -= 1;
                if indeg[e.to] == 0 {
                    ready.push(e.to);
                }
            }
        }
        if order.len() != n {
            return Err(GraphError::CycleWithoutDelay);
        }
        self.order = order;
        Ok(())
    }

    /// Render one block and return the output node's channels.
    pub fn render_block(&mut self) -> &[[f32; BLOCK]] {
        #[expect(clippy::cast_precision_loss, reason = "frame counter")]
        let block = Block {
            rate: self.rate,
            time: self.frame as f64 / f64::from(self.rate),
            frame: self.frame,
        };
        for oi in 0..self.order.len() {
            let i = self.order[oi];
            self.gather(i);
            let slot = &mut self.slots[i];
            slot.node.process(&slot.input, &mut slot.output, &block);
        }
        self.frame += BLOCK as u64;
        match self.output {
            Some(o) => &self.slots[o].output,
            None => &[],
        }
    }

    /// Sum every edge into node `i`'s input, remixing channel counts.
    fn gather(&mut self, i: usize) {
        let ins = self.slots[i].input.len();
        for ch in &mut self.slots[i].input {
            ch.fill(0.0);
        }
        if ins == 0 {
            return;
        }
        let to_layout = Layout::for_channels(ins);
        for ei in 0..self.edges.len() {
            let e = self.edges[ei];
            if e.to != i {
                continue;
            }
            let outs = self.slots[e.from].output.len();
            if outs == ins {
                // Fast path: same channel count.
                let (src, dst) = if e.from < i {
                    let (a, b) = self.slots.split_at_mut(i);
                    (&a[e.from].output, &mut b[0].input)
                } else if e.from > i {
                    let (a, b) = self.slots.split_at_mut(e.from);
                    (&b[0].output, &mut a[i].input)
                } else {
                    continue; // self-loops only through delay nodes' own state
                };
                for (d, s) in dst.iter_mut().zip(src) {
                    for (x, y) in d.iter_mut().zip(s) {
                        *x += y * e.gain;
                    }
                }
            } else {
                let from_layout = Layout::for_channels(outs);
                for f in 0..BLOCK {
                    for c in 0..outs {
                        let v = self.slots[e.from].output[c][f];
                        self.slots[i].frame_in[c] = v;
                    }
                    let slot = &mut self.slots[i];
                    remix(
                        from_layout,
                        to_layout,
                        &slot.frame_in[..outs],
                        &mut slot.frame_out[..ins],
                    );
                    for c in 0..ins {
                        slot.input[c][f] += slot.frame_out[c] * e.gain;
                    }
                }
            }
        }
    }

    /// Render whole blocks into interleaved output (any length; the last
    /// partial block is rendered and truncated). Allocation-free.
    pub fn render_interleaved(&mut self, out: &mut [f32], channels: usize) {
        let ch = channels.max(1);
        let mut written = 0;
        let total = out.len() / ch;
        while written < total {
            let n = (total - written).min(BLOCK);
            let block = self.render_block();
            for f in 0..n {
                for c in 0..ch {
                    let v = block.get(c).or_else(|| block.first()).map_or(0.0, |b| b[f]);
                    out[(written + f) * ch + c] = v;
                }
            }
            written += n;
        }
    }
}

/// A mono source from any [`Generator`].
pub struct SourceNode<G: Generator + Send>(pub G);

impl<G: Generator + Send> std::fmt::Debug for SourceNode<G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SourceNode")
    }
}

impl<G: Generator + Send> Node for SourceNode<G> {
    fn inputs(&self) -> usize {
        0
    }
    fn outputs(&self) -> usize {
        1
    }
    fn process(&mut self, _: &[[f32; BLOCK]], out: &mut [[f32; BLOCK]], _: &Block) {
        self.0.fill(&mut out[0]);
    }
}

/// Plays a buffer, optionally looping — `AudioBufferSourceNode`.
#[derive(Debug, Clone)]
pub struct BufferNode {
    data: Vec<f32>,
    channels: usize,
    pos: usize,
    /// Loop forever.
    pub looping: bool,
}

impl BufferNode {
    /// From samples (mono or stereo).
    #[must_use]
    pub fn new(s: &crate::Samples) -> Self {
        Self {
            data: s.data.clone(),
            channels: usize::from(s.channels.max(1)),
            pos: 0,
            looping: false,
        }
    }
}

impl Node for BufferNode {
    fn inputs(&self) -> usize {
        0
    }
    fn outputs(&self) -> usize {
        self.channels
    }
    fn process(&mut self, _: &[[f32; BLOCK]], out: &mut [[f32; BLOCK]], _: &Block) {
        let frames = self.data.len() / self.channels;
        for f in 0..BLOCK {
            if self.pos >= frames {
                if self.looping && frames > 0 {
                    self.pos = 0;
                } else {
                    for o in out.iter_mut() {
                        o[f] = 0.0;
                    }
                    continue;
                }
            }
            for (c, o) in out.iter_mut().enumerate() {
                o[f] = self.data[self.pos * self.channels + c];
            }
            self.pos += 1;
        }
    }
}

/// A gain stage with an automatable gain — `GainNode`. Also the bus.
#[derive(Debug, Clone)]
pub struct GainNode {
    /// The gain parameter.
    pub gain: Param,
    channels: usize,
    scratch: [f32; BLOCK],
}

impl GainNode {
    /// A gain over `channels`.
    #[must_use]
    pub fn new(gain: f32, channels: usize) -> Self {
        Self {
            gain: Param::new(gain, -1e6, 1e6),
            channels: channels.max(1),
            scratch: [0.0; BLOCK],
        }
    }
}

impl Node for GainNode {
    fn inputs(&self) -> usize {
        self.channels
    }
    fn outputs(&self) -> usize {
        self.channels
    }
    fn process(&mut self, input: &[[f32; BLOCK]], out: &mut [[f32; BLOCK]], b: &Block) {
        self.gain.fill(b.time, f64::from(b.rate), &mut self.scratch);
        for (o, i) in out.iter_mut().zip(input) {
            for ((y, x), g) in o.iter_mut().zip(i).zip(&self.scratch) {
                *y = x * g;
            }
        }
    }
}

/// A stereo equal-power panner — `StereoPannerNode` (mono in → stereo out,
/// or stereo in balanced).
#[derive(Debug, Clone)]
pub struct PanNode {
    /// Pan, −1..=1.
    pub pan: Param,
    inputs: usize,
    scratch: [f32; BLOCK],
}

impl PanNode {
    /// A panner taking `inputs` (1 or 2) channels.
    #[must_use]
    pub fn new(pan: f32, inputs: usize) -> Self {
        Self {
            pan: Param::new(pan, -1.0, 1.0),
            inputs: inputs.clamp(1, 2),
            scratch: [0.0; BLOCK],
        }
    }
}

impl Node for PanNode {
    fn inputs(&self) -> usize {
        self.inputs
    }
    fn outputs(&self) -> usize {
        2
    }
    fn process(&mut self, input: &[[f32; BLOCK]], out: &mut [[f32; BLOCK]], b: &Block) {
        self.pan.fill(b.time, f64::from(b.rate), &mut self.scratch);
        for f in 0..BLOCK {
            let p = self.scratch[f];
            if self.inputs == 1 {
                let (l, r) = crate::synth::equal_power(p);
                out[0][f] = input[0][f] * l;
                out[1][f] = input[0][f] * r;
            } else {
                // Web Audio's stereo rule: shift one channel into the other.
                let x = if p <= 0.0 { p + 1.0 } else { p };
                let (gl, gr) = (
                    (x * std::f32::consts::FRAC_PI_2).cos(),
                    (x * std::f32::consts::FRAC_PI_2).sin(),
                );
                let (il, ir) = (input[0][f], input[1][f]);
                if p <= 0.0 {
                    out[0][f] = il + ir * gl;
                    out[1][f] = ir * gr;
                } else {
                    out[0][f] = il * gl;
                    out[1][f] = ir + il * gr;
                }
            }
        }
    }
}

/// A delay — `DelayNode`, and the only node a feedback cycle may pass
/// through.
#[derive(Debug, Clone)]
pub struct DelayNode {
    lines: Vec<crate::fx::filter::DelayLine>,
    /// Delay in seconds.
    pub time: Param,
    rate: f32,
    scratch: [f32; BLOCK],
}

impl DelayNode {
    /// A delay of `seconds` (at least one block) over `channels`, holding
    /// up to `max_seconds`.
    #[must_use]
    pub fn new(seconds: f32, max_seconds: f32, channels: usize, rate: f32) -> Self {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "lengths"
        )]
        let cap = (max_seconds.max(seconds) * rate) as usize + BLOCK + 8;
        Self {
            lines: (0..channels.max(1))
                .map(|_| crate::fx::filter::DelayLine::new(cap))
                .collect(),
            time: Param::new(seconds, 0.0, max_seconds.max(seconds)),
            rate,
            scratch: [0.0; BLOCK],
        }
    }
}

impl Node for DelayNode {
    fn inputs(&self) -> usize {
        self.lines.len()
    }
    fn outputs(&self) -> usize {
        self.lines.len()
    }
    fn is_delay(&self) -> bool {
        true
    }
    fn process(&mut self, input: &[[f32; BLOCK]], out: &mut [[f32; BLOCK]], b: &Block) {
        // `input` is the previous block (one block of latency), so the line
        // supplies the rest: D − BLOCK samples.
        self.time.fill(b.time, f64::from(b.rate), &mut self.scratch);
        #[expect(clippy::cast_precision_loss, reason = "block size")]
        let block = BLOCK as f32;
        for ((line, i), o) in self.lines.iter_mut().zip(input).zip(out.iter_mut()) {
            for f in 0..BLOCK {
                line.push(i[f]);
                let d = (self.scratch[f] * self.rate - block).max(0.0);
                o[f] = line.tap_frac(d + 1.0);
            }
        }
    }
}

/// Any [`Processor`], one instance per channel.
pub struct ProcessorNode<P: Processor + Send> {
    procs: Vec<P>,
}

impl<P: Processor + Send> std::fmt::Debug for ProcessorNode<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessorNode")
            .field("channels", &self.procs.len())
            .finish()
    }
}

impl<P: Processor + Send> ProcessorNode<P> {
    /// `channels` instances from a factory.
    pub fn new(channels: usize, make: impl FnMut() -> P) -> Self {
        Self {
            procs: std::iter::repeat_with(make).take(channels.max(1)).collect(),
        }
    }
}

impl<P: Processor + Send> Node for ProcessorNode<P> {
    fn inputs(&self) -> usize {
        self.procs.len()
    }
    fn outputs(&self) -> usize {
        self.procs.len()
    }
    fn process(&mut self, input: &[[f32; BLOCK]], out: &mut [[f32; BLOCK]], _: &Block) {
        for ((p, i), o) in self.procs.iter_mut().zip(input).zip(out.iter_mut()) {
            for (y, x) in o.iter_mut().zip(i) {
                *y = p.process(*x);
            }
        }
    }
}

/// Any [`Stereo`] effect.
pub struct StereoNode<S: Stereo + Send>(pub S);

impl<S: Stereo + Send> std::fmt::Debug for StereoNode<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StereoNode")
    }
}

impl<S: Stereo + Send> Node for StereoNode<S> {
    fn inputs(&self) -> usize {
        2
    }
    fn outputs(&self) -> usize {
        2
    }
    fn process(&mut self, input: &[[f32; BLOCK]], out: &mut [[f32; BLOCK]], _: &Block) {
        for f in 0..BLOCK {
            let (l, r) = self.0.process_stereo(input[0][f], input[1][f]);
            out[0][f] = l;
            out[1][f] = r;
        }
    }
}

/// A closure node — `AudioWorkletNode`'s shape, without the worklet.
pub struct FnNode<F>
where
    F: FnMut(&[[f32; BLOCK]], &mut [[f32; BLOCK]], &Block) + Send,
{
    ins: usize,
    outs: usize,
    f: F,
}

impl<F> std::fmt::Debug for FnNode<F>
where
    F: FnMut(&[[f32; BLOCK]], &mut [[f32; BLOCK]], &Block) + Send,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FnNode")
            .field("ins", &self.ins)
            .field("outs", &self.outs)
            .finish()
    }
}

impl<F> FnNode<F>
where
    F: FnMut(&[[f32; BLOCK]], &mut [[f32; BLOCK]], &Block) + Send,
{
    /// A node of `ins` inputs and `outs` outputs running `f`.
    pub const fn new(ins: usize, outs: usize, f: F) -> Self {
        Self { ins, outs, f }
    }
}

impl<F> Node for FnNode<F>
where
    F: FnMut(&[[f32; BLOCK]], &mut [[f32; BLOCK]], &Block) + Send,
{
    fn inputs(&self) -> usize {
        self.ins
    }
    fn outputs(&self) -> usize {
        self.outs
    }
    fn process(&mut self, input: &[[f32; BLOCK]], out: &mut [[f32; BLOCK]], b: &Block) {
        (self.f)(input, out, b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::osc::{Oscillator, Shape};
    use crate::synth::Constant;

    const RATE: f32 = 48_000.0;

    #[test]
    fn source_through_gain_to_output() {
        let mut g = Graph::new(RATE);
        let s = g.add(SourceNode(Constant(1.0)));
        let gain = g.add(GainNode::new(0.5, 1));
        g.connect(s, gain).unwrap();
        g.set_output(gain);
        let out = g.render_block();
        assert!(out[0].iter().all(|v| (*v - 0.5).abs() < 1e-6));
    }

    #[test]
    fn fan_in_sums_and_sends_scale() {
        let mut g = Graph::new(RATE);
        let a = g.add(SourceNode(Constant(0.25)));
        let b = g.add(SourceNode(Constant(0.5)));
        let bus = g.add(GainNode::new(1.0, 1));
        g.connect(a, bus).unwrap();
        g.connect_gain(b, bus, 0.5).unwrap();
        g.set_output(bus);
        assert!((g.render_block()[0][7] - 0.5).abs() < 1e-6);
        g.set_send(b, bus, 1.0);
        assert!((g.render_block()[0][7] - 0.75).abs() < 1e-6);
        g.disconnect(a, bus);
        assert!((g.render_block()[0][7] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn mono_into_stereo_upmixes() {
        let mut g = Graph::new(RATE);
        let s = g.add(SourceNode(Constant(0.3)));
        let st = g.add(GainNode::new(1.0, 2));
        g.connect(s, st).unwrap();
        g.set_output(st);
        let o = g.render_block();
        assert_eq!(o.len(), 2);
        assert!((o[0][0] - 0.3).abs() < 1e-6 && (o[1][0] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn automation_is_sample_accurate_across_blocks() {
        let mut g = Graph::new(1280.0);
        let s = g.add(SourceNode(Constant(1.0)));
        let mut gn = GainNode::new(0.0, 1);
        gn.gain.set_at(0.0, 0.0).linear_to(1.0, 0.2); // 256 frames
        let gid = g.add(gn);
        g.connect(s, gid).unwrap();
        g.set_output(gid);
        let first = g.render_block()[0];
        let second = g.render_block()[0];
        assert!((first[64] - 0.25).abs() < 1e-5);
        assert!((second[0] - 0.5).abs() < 1e-5);
        assert!((second[127] - (255.0 / 256.0)).abs() < 1e-4);
    }

    #[test]
    fn cycle_without_delay_is_refused_with_delay_is_allowed() {
        let mut g = Graph::new(RATE);
        let a = g.add(GainNode::new(1.0, 1));
        let b = g.add(GainNode::new(0.5, 1));
        g.connect(a, b).unwrap();
        assert_eq!(g.connect(b, a), Err(GraphError::CycleWithoutDelay));
        let d = g.add(DelayNode::new(0.01, 0.1, 1, RATE));
        g.connect(b, d).unwrap();
        g.connect(d, a).unwrap();
    }

    #[test]
    fn feedback_echo_repeats_at_the_delay_time_and_decays() {
        // impulse → mix ← delay(0.01 s) ← feedback gain 0.5 ← mix
        let rate = 12_800.0; // 0.01 s = 128 frames = one block exactly
        let mut g = Graph::new(rate);
        let mut fired = false;
        let imp = g.add(FnNode::new(0, 1, move |_, out, _| {
            out[0].fill(0.0);
            if !fired {
                out[0][3] = 1.0;
                fired = true;
            }
        }));
        let mix = g.add(GainNode::new(1.0, 1));
        let fb = g.add(GainNode::new(0.5, 1));
        let d = g.add(DelayNode::new(0.02, 0.1, 1, rate));
        g.connect(imp, mix).unwrap();
        g.connect(mix, fb).unwrap();
        g.connect(fb, d).unwrap();
        g.connect(d, mix).unwrap();
        g.set_output(mix);
        let mut out = Vec::new();
        for _ in 0..8 {
            out.extend_from_slice(&g.render_block()[0]);
        }
        // Echoes every 256 frames (0.02 s), halving.
        assert!((out[3] - 1.0).abs() < 1e-6);
        assert!((out[3 + 256] - 0.5).abs() < 1e-4, "{}", out[3 + 256]);
        assert!((out[3 + 512] - 0.25).abs() < 1e-4);
        let elsewhere: f32 = out
            .iter()
            .enumerate()
            .filter(|(i, _)| (i + 253) % 256 != 0)
            .map(|(_, v)| v.abs())
            .sum();
        assert!(elsewhere < 1e-3, "{elsewhere}");
    }

    #[test]
    fn processor_and_stereo_nodes_run() {
        let mut g = Graph::new(RATE);
        let s = g.add(SourceNode(Oscillator::new(Shape::Sine, 440.0, RATE)));
        let p = g.add(ProcessorNode::new(1, || {
            crate::fx::distortion::Bitcrusher::new(4.0, 1.0)
        }));
        let pan = g.add(PanNode::new(-1.0, 1));
        let verb = g.add(StereoNode(crate::fx::reverb::Freeverb::new(0.5, 0.5, RATE)));
        g.connect(s, p).unwrap();
        g.connect(p, pan).unwrap();
        g.connect(pan, verb).unwrap();
        g.set_output(verb);
        let mut out = vec![0.0; 2 * 4800];
        g.render_interleaved(&mut out, 2);
        let l: f32 = out.iter().step_by(2).map(|v| v.abs()).sum();
        assert!(l > 10.0, "{l}");
    }

    #[test]
    fn hard_panned_mono_is_silent_on_the_other_side() {
        let mut g = Graph::new(RATE);
        let s = g.add(SourceNode(Constant(1.0)));
        let pan = g.add(PanNode::new(1.0, 1));
        g.connect(s, pan).unwrap();
        g.set_output(pan);
        let o = g.render_block();
        assert!(o[0][0].abs() < 1e-6 && (o[1][0] - 1.0).abs() < 1e-6);
    }
}
