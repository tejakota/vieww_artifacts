//! The node graph — operator networks that cook on demand, and event
//! graphs that execute.
//!
//! # Two graph models from the document
//!
//! **Dataflow with dirty flags** (§2.8 L1–L2, §2.14 L3, §5.1 "Dependency
//! Graph", §5.2 "Dirty-Propagation"): TouchDesigner's operators and
//! Blender's depsgraph recompute a node *only when an input actually
//! changed* — "extremely efficient for interactive installations where most
//! parameters remain static". [`Graph`] is that: typed operators wired
//! output→input, a parameter or time change marks the node and everything
//! downstream dirty, and [`Graph::pull`] cooks exactly the dirty nodes the
//! requested output depends on, in topological order, caching the rest.
//! Every node counts its cooks, so the efficiency is a number a test can
//! assert, not a claim.
//!
//! **Execution graphs** (§2.17 L2, Unreal Blueprints): white exec wires
//! decide *what runs in which order* — an event fires, a branch picks a
//! path, a sequence runs its outputs in turn, a loop repeats — while data
//! pins are pulled from the dataflow graph. [`ExecGraph`] runs those against
//! a [`Graph`] and a variable store, returning the trace of what executed.
//!
//! Cycles are refused at connect time, so every graph is a DAG and cooking
//! always terminates.

use std::collections::BTreeMap;
use std::fmt;

use vieww_animation::expr::{Expr, Scope, Val};
use vieww_animation::noise::Perlin;

/// A value on a wire.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Float(f32),
    Bool(bool),
    Vec2(f32, f32),
    Text(String),
    /// A channel of samples (a CHOP's output).
    Channel(Vec<f32>),
}

impl Value {
    /// As a number: booleans are 0/1, vectors their x, channels their
    /// first sample, text 0.
    #[must_use]
    pub fn as_f32(&self) -> f32 {
        match self {
            Self::Float(f) => *f,
            Self::Bool(b) => f32::from(u8::from(*b)),
            Self::Vec2(x, _) => *x,
            Self::Channel(c) => c.first().copied().unwrap_or(0.0),
            Self::Text(_) => 0.0,
        }
    }

    #[must_use]
    pub fn truthy(&self) -> bool {
        self.as_f32() != 0.0
    }
}

/// Handle to a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub usize);

/// What cooking can see besides inputs and parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CookContext {
    /// The graph's clock, seconds.
    pub time: f32,
    /// How many times this node has cooked before.
    pub previous_cooks: u64,
}

/// An operator: named inputs and outputs, parameters, and a cook function.
pub trait Operator: fmt::Debug {
    fn kind(&self) -> &'static str;
    fn inputs(&self) -> Vec<&'static str>;
    fn outputs(&self) -> Vec<&'static str>;
    /// Compute outputs from inputs (missing inputs arrive as `Float(0.0)`)
    /// and parameters.
    fn cook(
        &mut self,
        inputs: &[Value],
        params: &BTreeMap<String, Value>,
        ctx: &CookContext,
    ) -> Vec<Value>;
    /// Re-cook whenever the clock moves (a Timer CHOP, an LFO).
    fn time_dependent(&self) -> bool {
        false
    }
}

struct Slot {
    op: Box<dyn Operator>,
    name: String,
    params: BTreeMap<String, Value>,
    cache: Vec<Value>,
    dirty: bool,
    cooks: u64,
}

/// A wire from an output to an input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wire {
    pub from: NodeId,
    pub output: usize,
    pub to: NodeId,
    pub input: usize,
}

/// Why an edit was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphError {
    NoSuchPort(String),
    Cycle,
    NoSuchNode(String),
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchPort(p) => write!(f, "no port {p}"),
            Self::Cycle => f.write_str("that wire would make a cycle"),
            Self::NoSuchNode(n) => write!(f, "no node {n}"),
        }
    }
}

impl std::error::Error for GraphError {}

/// The dataflow network. See the [crate docs](crate).
#[derive(Default)]
pub struct Graph {
    slots: Vec<Slot>,
    wires: Vec<Wire>,
    time: f32,
    /// CHOP exports: `(from, output index, to, parameter)`.
    exports: Vec<(NodeId, usize, NodeId, String)>,
    /// Cooks performed by the most recent `pull`/`cook_all`.
    pub last_cooks: u64,
}

impl fmt::Debug for Graph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Graph")
            .field("nodes", &self.slots.len())
            .field("wires", &self.wires.len())
            .field("time", &self.time)
            .finish_non_exhaustive()
    }
}

impl Graph {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an operator under a display name.
    pub fn add(&mut self, name: &str, op: impl Operator + 'static) -> NodeId {
        self.slots.push(Slot {
            op: Box::new(op),
            name: name.to_owned(),
            params: BTreeMap::new(),
            cache: Vec::new(),
            dirty: true,
            cooks: 0,
        });
        NodeId(self.slots.len() - 1)
    }

    /// Find a node by name.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<NodeId> {
        self.slots.iter().position(|s| s.name == name).map(NodeId)
    }

    #[must_use]
    pub fn name(&self, id: NodeId) -> &str {
        &self.slots[id.0].name
    }

    #[must_use]
    pub fn kind(&self, id: NodeId) -> &'static str {
        self.slots[id.0].op.kind()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    #[must_use]
    pub fn wires(&self) -> &[Wire] {
        &self.wires
    }

    /// How many times a node has cooked.
    #[must_use]
    pub fn cooks(&self, id: NodeId) -> u64 {
        self.slots[id.0].cooks
    }

    #[must_use]
    pub fn is_dirty(&self, id: NodeId) -> bool {
        self.slots[id.0].dirty
    }

    fn port(names: &[&str], port: &str) -> Result<usize, GraphError> {
        names
            .iter()
            .position(|n| *n == port)
            .ok_or_else(|| GraphError::NoSuchPort(port.to_owned()))
    }

    /// Wire `from.output` into `to.input` (replacing any wire already on
    /// that input). Refuses cycles.
    ///
    /// # Errors
    ///
    /// Unknown port names or a wire that would close a cycle.
    pub fn connect(
        &mut self,
        from: NodeId,
        output: &str,
        to: NodeId,
        input: &str,
    ) -> Result<(), GraphError> {
        let o = Self::port(&self.slots[from.0].op.outputs(), output)?;
        let i = Self::port(&self.slots[to.0].op.inputs(), input)?;
        if from == to || self.reaches(to, from) {
            return Err(GraphError::Cycle);
        }
        self.wires.retain(|w| !(w.to == to && w.input == i));
        self.wires.push(Wire {
            from,
            output: o,
            to,
            input: i,
        });
        self.mark_dirty(to);
        Ok(())
    }

    /// Remove the wire into `to.input`.
    ///
    /// # Errors
    ///
    /// Unknown input name.
    pub fn disconnect(&mut self, to: NodeId, input: &str) -> Result<(), GraphError> {
        let i = Self::port(&self.slots[to.0].op.inputs(), input)?;
        self.wires.retain(|w| !(w.to == to && w.input == i));
        self.mark_dirty(to);
        Ok(())
    }

    /// Whether `b` is downstream of `a`.
    fn reaches(&self, a: NodeId, b: NodeId) -> bool {
        let mut stack = vec![a];
        let mut seen = vec![false; self.slots.len()];
        while let Some(n) = stack.pop() {
            if n == b {
                return true;
            }
            if std::mem::replace(&mut seen[n.0], true) {
                continue;
            }
            stack.extend(self.wires.iter().filter(|w| w.from == n).map(|w| w.to));
        }
        false
    }

    /// Mark a node and everything downstream dirty.
    pub fn mark_dirty(&mut self, id: NodeId) {
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            if self.slots[n.0].dirty && n != id {
                continue;
            }
            self.slots[n.0].dirty = true;
            stack.extend(self.wires.iter().filter(|w| w.from == n).map(|w| w.to));
            stack.extend(self.exports.iter().filter(|e| e.0 == n).map(|e| e.2));
        }
    }

    /// Set a parameter (dirties the node only if the value changed — the
    /// point of dirty flags).
    pub fn set_param(&mut self, id: NodeId, name: &str, value: Value) {
        if self.slots[id.0].params.get(name) == Some(&value) {
            return;
        }
        self.slots[id.0].params.insert(name.to_owned(), value);
        self.mark_dirty(id);
    }

    #[must_use]
    pub fn param(&self, id: NodeId, name: &str) -> Option<&Value> {
        self.slots[id.0].params.get(name)
    }

    /// **Export** a channel to a parameter — TouchDesigner's CHOP export
    /// (§2.8 L7): `from.output` drives `to`'s parameter `param` every cook,
    /// so any operator's setting can be animated by any channel without a
    /// wire into an input. The source becomes a dependency: it cooks first,
    /// and dirtying it dirties the target.
    ///
    /// # Errors
    /// Unknown output.
    pub fn export(&mut self, from: NodeId, output: &str, to: NodeId, param: &str) -> Result<(), GraphError> {
        let o = Self::port(&self.slots[from.0].op.outputs(), output)?;
        self.exports.retain(|e| !(e.2 == to && e.3 == param));
        self.exports.push((from, o, to, param.to_owned()));
        self.mark_dirty(to);
        Ok(())
    }

    /// Remove an export (the parameter keeps its last value).
    pub fn unexport(&mut self, to: NodeId, param: &str) {
        self.exports.retain(|e| !(e.2 == to && e.3 == param));
    }

    /// Move the clock; time-dependent operators (and what they feed) go
    /// dirty.
    pub fn set_time(&mut self, t: f32) {
        if (t - self.time).abs() < f32::EPSILON {
            return;
        }
        self.time = t;
        let timed: Vec<NodeId> = (0..self.slots.len())
            .filter(|&i| self.slots[i].op.time_dependent())
            .map(NodeId)
            .collect();
        for id in timed {
            self.mark_dirty(id);
        }
    }

    /// The nodes `target` depends on, in topological order (target last).
    fn upstream_order(&self, target: NodeId) -> Vec<NodeId> {
        let mut order = Vec::new();
        let mut state = vec![0u8; self.slots.len()];
        let mut stack = vec![(target, false)];
        while let Some((n, done)) = stack.pop() {
            if done {
                order.push(n);
                continue;
            }
            if state[n.0] != 0 {
                continue;
            }
            state[n.0] = 1;
            stack.push((n, true));
            for w in self.wires.iter().filter(|w| w.to == n) {
                if state[w.from.0] == 0 {
                    stack.push((w.from, false));
                }
            }
            for e in self.exports.iter().filter(|e| e.2 == n) {
                if state[e.0 .0] == 0 {
                    stack.push((e.0, false));
                }
            }
        }
        order
    }

    fn cook(&mut self, n: NodeId) {
        let arity = self.slots[n.0].op.inputs().len();
        let mut inputs = vec![Value::Float(0.0); arity];
        for w in self.wires.iter().filter(|w| w.to == n) {
            if let Some(v) = self.slots[w.from.0].cache.get(w.output) {
                inputs[w.input] = v.clone();
            }
        }
        let exported: Vec<(String, Value)> = self
            .exports
            .iter()
            .filter(|e| e.2 == n)
            .filter_map(|e| self.slots[e.0 .0].cache.get(e.1).map(|v| (e.3.clone(), v.clone())))
            .collect();
        let slot = &mut self.slots[n.0];
        for (k, v) in exported {
            slot.params.insert(k, v);
        }
        let ctx = CookContext {
            time: self.time,
            previous_cooks: slot.cooks,
        };
        slot.cache = slot.op.cook(&inputs, &slot.params, &ctx);
        slot.cooks += 1;
        slot.dirty = false;
        self.last_cooks += 1;
    }

    /// The value on `id.output`, cooking only what is dirty upstream.
    ///
    /// # Errors
    ///
    /// Unknown output name.
    pub fn pull(&mut self, id: NodeId, output: &str) -> Result<Value, GraphError> {
        let o = Self::port(&self.slots[id.0].op.outputs(), output)?;
        self.last_cooks = 0;
        for n in self.upstream_order(id) {
            if self.slots[n.0].dirty {
                self.cook(n);
            }
        }
        Ok(self.slots[id.0]
            .cache
            .get(o)
            .cloned()
            .unwrap_or(Value::Float(0.0)))
    }

    /// Cook every dirty node (a frame of a TouchDesigner project).
    pub fn cook_all(&mut self) {
        self.last_cooks = 0;
        let sinks: Vec<NodeId> = (0..self.slots.len())
            .map(NodeId)
            .filter(|n| !self.wires.iter().any(|w| w.from == *n))
            .collect();
        for s in sinks {
            for n in self.upstream_order(s) {
                if self.slots[n.0].dirty {
                    self.cook(n);
                }
            }
        }
    }
}

// ---------------------------------------------------------------- operators

fn p(params: &BTreeMap<String, Value>, k: &str, d: f32) -> f32 {
    params.get(k).map_or(d, Value::as_f32)
}

/// Outputs its `value` parameter.
#[derive(Debug, Default, Clone)]
pub struct Constant;
impl Operator for Constant {
    fn kind(&self) -> &'static str {
        "Constant"
    }
    fn inputs(&self) -> Vec<&'static str> {
        vec![]
    }
    fn outputs(&self) -> Vec<&'static str> {
        vec!["out"]
    }
    fn cook(
        &mut self,
        _: &[Value],
        params: &BTreeMap<String, Value>,
        _: &CookContext,
    ) -> Vec<Value> {
        vec![params.get("value").cloned().unwrap_or(Value::Float(0.0))]
    }
}

/// The graph clock, times `speed` (a Timer / AbsTime CHOP).
#[derive(Debug, Default, Clone)]
pub struct Time;
impl Operator for Time {
    fn kind(&self) -> &'static str {
        "Time"
    }
    fn inputs(&self) -> Vec<&'static str> {
        vec![]
    }
    fn outputs(&self) -> Vec<&'static str> {
        vec!["seconds"]
    }
    fn cook(
        &mut self,
        _: &[Value],
        params: &BTreeMap<String, Value>,
        ctx: &CookContext,
    ) -> Vec<Value> {
        vec![Value::Float(ctx.time * p(params, "speed", 1.0))]
    }
    fn time_dependent(&self) -> bool {
        true
    }
}

/// A binary arithmetic operator (Math CHOP / Math node).
#[derive(Debug, Clone, Copy)]
pub enum Math {
    Add,
    Subtract,
    Multiply,
    Divide,
    Max,
    Min,
    Power,
}
impl Operator for Math {
    fn kind(&self) -> &'static str {
        "Math"
    }
    fn inputs(&self) -> Vec<&'static str> {
        vec!["a", "b"]
    }
    fn outputs(&self) -> Vec<&'static str> {
        vec!["out"]
    }
    fn cook(&mut self, i: &[Value], _: &BTreeMap<String, Value>, _: &CookContext) -> Vec<Value> {
        let (a, b) = (i[0].as_f32(), i[1].as_f32());
        vec![Value::Float(match self {
            Self::Add => a + b,
            Self::Subtract => a - b,
            Self::Multiply => a * b,
            Self::Divide => a / b,
            Self::Max => a.max(b),
            Self::Min => a.min(b),
            Self::Power => a.powf(b),
        })]
    }
}

/// A sine LFO driven by its input (seconds): `amplitude · sin(2π f t)`.
#[derive(Debug, Default, Clone)]
pub struct Lfo;
impl Operator for Lfo {
    fn kind(&self) -> &'static str {
        "LFO"
    }
    fn inputs(&self) -> Vec<&'static str> {
        vec!["time"]
    }
    fn outputs(&self) -> Vec<&'static str> {
        vec!["out"]
    }
    fn cook(
        &mut self,
        i: &[Value],
        params: &BTreeMap<String, Value>,
        _: &CookContext,
    ) -> Vec<Value> {
        let f = p(params, "frequency", 1.0);
        let a = p(params, "amplitude", 1.0);
        vec![Value::Float(
            a * (std::f32::consts::TAU * f * i[0].as_f32()).sin() + p(params, "offset", 0.0),
        )]
    }
}

/// Seeded Perlin noise of its input (Noise CHOP).
#[derive(Debug, Default, Clone)]
pub struct Noise;
impl Operator for Noise {
    fn kind(&self) -> &'static str {
        "Noise"
    }
    fn inputs(&self) -> Vec<&'static str> {
        vec!["x"]
    }
    fn outputs(&self) -> Vec<&'static str> {
        vec!["out"]
    }
    fn cook(
        &mut self,
        i: &[Value],
        params: &BTreeMap<String, Value>,
        _: &CookContext,
    ) -> Vec<Value> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let seed = p(params, "seed", 1.0) as u64;
        let n = Perlin::from_seed(seed).noise1(i[0].as_f32() * p(params, "frequency", 1.0));
        vec![Value::Float(n * p(params, "amplitude", 1.0))]
    }
}

/// Remap `in` from `[from_lo, from_hi]` to `[to_lo, to_hi]`.
#[derive(Debug, Default, Clone)]
pub struct Remap;
impl Operator for Remap {
    fn kind(&self) -> &'static str {
        "Remap"
    }
    fn inputs(&self) -> Vec<&'static str> {
        vec!["in"]
    }
    fn outputs(&self) -> Vec<&'static str> {
        vec!["out"]
    }
    fn cook(
        &mut self,
        i: &[Value],
        params: &BTreeMap<String, Value>,
        _: &CookContext,
    ) -> Vec<Value> {
        let (a, b) = (p(params, "from_lo", 0.0), p(params, "from_hi", 1.0));
        let (c, d) = (p(params, "to_lo", 0.0), p(params, "to_hi", 1.0));
        let u = if (b - a).abs() < 1e-12 {
            0.0
        } else {
            (i[0].as_f32() - a) / (b - a)
        };
        vec![Value::Float(c + (d - c) * u)]
    }
}

/// Pick `a` or `b` by `select` (Switch TOP/CHOP).
#[derive(Debug, Default, Clone)]
pub struct Switch;
impl Operator for Switch {
    fn kind(&self) -> &'static str {
        "Switch"
    }
    fn inputs(&self) -> Vec<&'static str> {
        vec!["select", "a", "b"]
    }
    fn outputs(&self) -> Vec<&'static str> {
        vec!["out"]
    }
    fn cook(&mut self, i: &[Value], _: &BTreeMap<String, Value>, _: &CookContext) -> Vec<Value> {
        vec![if i[0].truthy() {
            i[2].clone()
        } else {
            i[1].clone()
        }]
    }
}

/// Sample its input into a rolling channel of `length` samples (a Trail
/// CHOP) — stateful across cooks.
#[derive(Debug, Default, Clone)]
pub struct Trail {
    samples: Vec<f32>,
}
impl Operator for Trail {
    fn kind(&self) -> &'static str {
        "Trail"
    }
    fn inputs(&self) -> Vec<&'static str> {
        vec!["in"]
    }
    fn outputs(&self) -> Vec<&'static str> {
        vec!["channel"]
    }
    fn cook(
        &mut self,
        i: &[Value],
        params: &BTreeMap<String, Value>,
        _: &CookContext,
    ) -> Vec<Value> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let len = p(params, "length", 64.0).max(1.0) as usize;
        self.samples.push(i[0].as_f32());
        if self.samples.len() > len {
            let drop = self.samples.len() - len;
            self.samples.drain(..drop);
        }
        vec![Value::Channel(self.samples.clone())]
    }
}

/// An expression over inputs `a`, `b`, `c` and `time`, from the `expr`
/// parameter (a TouchDesigner parameter expression, a Blender driver).
#[derive(Debug, Default, Clone)]
pub struct Expression {
    compiled: Option<(String, Expr)>,
}
impl Operator for Expression {
    fn kind(&self) -> &'static str {
        "Expression"
    }
    fn inputs(&self) -> Vec<&'static str> {
        vec!["a", "b", "c"]
    }
    fn outputs(&self) -> Vec<&'static str> {
        vec!["out"]
    }
    fn cook(
        &mut self,
        i: &[Value],
        params: &BTreeMap<String, Value>,
        ctx: &CookContext,
    ) -> Vec<Value> {
        let src = match params.get("expr") {
            Some(Value::Text(t)) => t.clone(),
            _ => "0".to_owned(),
        };
        if self.compiled.as_ref().is_none_or(|(s, _)| *s != src) {
            self.compiled = Expr::parse(&src).ok().map(|e| (src.clone(), e));
        }
        let scope = Scope::at(ctx.time)
            .with_var("a", Val::Num(i[0].as_f32()))
            .with_var("b", Val::Num(i[1].as_f32()))
            .with_var("c", Val::Num(i[2].as_f32()));
        let v = self
            .compiled
            .as_ref()
            .and_then(|(_, e)| e.eval_num(&scope).ok())
            .unwrap_or(f32::NAN);
        vec![Value::Float(v)]
    }
    fn time_dependent(&self) -> bool {
        true
    }
}

// ------------------------------------------------------------ exec graphs

/// A node of an event graph.
#[derive(Debug, Clone, PartialEq)]
pub enum ExecNode {
    /// Entry point (`Event BeginPlay`, `Event Tick`, a custom event).
    Event(String),
    /// Continue on `then` or `else` by a data pin.
    Branch { condition: (NodeId, String) },
    /// Run each output in order.
    Sequence(usize),
    /// Run `body` `count` times (the index is in variable `index_var`).
    ForLoop {
        count: (NodeId, String),
        index_var: String,
    },
    /// Store a data pin in a variable.
    SetVariable {
        name: String,
        value: (NodeId, String),
    },
    /// Call out to the host (`Print String`, a game function).
    Call(String),
}

/// Handle to an exec node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExecId(pub usize);

/// A Blueprint-style event graph over a dataflow [`Graph`].
#[derive(Debug, Clone, Default)]
pub struct ExecGraph {
    nodes: Vec<ExecNode>,
    /// `(from, exec output index) → to`.
    links: BTreeMap<(ExecId, usize), ExecId>,
    pub variables: BTreeMap<String, Value>,
}

impl ExecGraph {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, node: ExecNode) -> ExecId {
        self.nodes.push(node);
        ExecId(self.nodes.len() - 1)
    }

    /// Wire exec output `pin` of `from` (0 = `then`/first; Branch: 0 then,
    /// 1 else; ForLoop: 0 body, 1 completed) to `to`.
    pub fn then(&mut self, from: ExecId, pin: usize, to: ExecId) {
        self.links.insert((from, pin), to);
    }

    /// Fire the event named `event`; returns the trace of calls and
    /// variable writes, in execution order.
    ///
    /// # Errors
    ///
    /// A data pin that names an unknown output.
    pub fn fire(&mut self, event: &str, data: &mut Graph) -> Result<Vec<String>, GraphError> {
        let start = self
            .nodes
            .iter()
            .position(|n| *n == ExecNode::Event(event.to_owned()))
            .ok_or_else(|| GraphError::NoSuchNode(event.to_owned()))?;
        let mut trace = Vec::new();
        self.run(ExecId(start), data, &mut trace, 0, None)?;
        Ok(trace)
    }

    fn run(
        &mut self,
        at: ExecId,
        data: &mut Graph,
        trace: &mut Vec<String>,
        depth: usize,
        index: Option<usize>,
    ) -> Result<(), GraphError> {
        if depth > 10_000 {
            return Ok(()); // runaway guard
        }
        let next = |s: &Self, pin: usize| s.links.get(&(at, pin)).copied();
        match self.nodes[at.0].clone() {
            ExecNode::Event(_) => {
                if let Some(n) = next(self, 0) {
                    self.run(n, data, trace, depth + 1, index)?;
                }
            }
            ExecNode::Branch { condition } => {
                let v = data.pull(condition.0, &condition.1)?;
                let pin = usize::from(!v.truthy());
                if let Some(n) = next(self, pin) {
                    self.run(n, data, trace, depth + 1, index)?;
                }
            }
            ExecNode::Sequence(outs) => {
                for pin in 0..outs {
                    if let Some(n) = next(self, pin) {
                        self.run(n, data, trace, depth + 1, index)?;
                    }
                }
            }
            ExecNode::ForLoop { count, index_var } => {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let n = data.pull(count.0, &count.1)?.as_f32().max(0.0) as usize;
                for i in 0..n {
                    #[allow(clippy::cast_precision_loss)]
                    self.variables
                        .insert(index_var.clone(), Value::Float(i as f32));
                    if let Some(b) = next(self, 0) {
                        self.run(b, data, trace, depth + 1, Some(i))?;
                    }
                }
                if let Some(d) = next(self, 1) {
                    self.run(d, data, trace, depth + 1, index)?;
                }
            }
            ExecNode::SetVariable { name, value } => {
                let v = data.pull(value.0, &value.1)?;
                trace.push(format!("set {name} = {:.3}", v.as_f32()));
                self.variables.insert(name, v);
                if let Some(n) = next(self, 0) {
                    self.run(n, data, trace, depth + 1, index)?;
                }
            }
            ExecNode::Call(name) => {
                let suffix = index.map(|i| format!(" #{i}")).unwrap_or_default();
                trace.push(format!("call {name}{suffix}"));
                if let Some(n) = next(self, 0) {
                    self.run(n, data, trace, depth + 1, index)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain() -> (Graph, NodeId, NodeId, NodeId, NodeId) {
        let mut g = Graph::new();
        let a = g.add("a", Constant);
        let b = g.add("b", Constant);
        let sum = g.add("sum", Math::Add);
        let dbl = g.add("double", Math::Multiply);
        let two = g.add("two", Constant);
        g.set_param(a, "value", Value::Float(1.0));
        g.set_param(b, "value", Value::Float(2.0));
        g.set_param(two, "value", Value::Float(2.0));
        g.connect(a, "out", sum, "a").unwrap();
        g.connect(b, "out", sum, "b").unwrap();
        g.connect(sum, "out", dbl, "a").unwrap();
        g.connect(two, "out", dbl, "b").unwrap();
        (g, a, b, sum, dbl)
    }

    #[test]
    fn pull_cooks_upstream_in_order() {
        let (mut g, _, _, _, dbl) = chain();
        assert_eq!(g.pull(dbl, "out").unwrap(), Value::Float(6.0));
        assert_eq!(g.last_cooks, 5);
    }

    #[test]
    fn nothing_recooks_until_something_changes_and_only_downstream_does() {
        let (mut g, a, b, sum, dbl) = chain();
        g.pull(dbl, "out").unwrap();
        g.pull(dbl, "out").unwrap();
        assert_eq!(g.last_cooks, 0, "clean graph: zero cooks");
        g.set_param(a, "value", Value::Float(1.0));
        assert!(!g.is_dirty(sum), "setting the same value dirties nothing");
        g.set_param(a, "value", Value::Float(4.0));
        assert!(g.is_dirty(sum) && g.is_dirty(dbl) && !g.is_dirty(b));
        assert_eq!(g.pull(dbl, "out").unwrap(), Value::Float(12.0));
        assert_eq!(g.last_cooks, 3, "a, sum, double — not b, not two");
        assert_eq!(g.cooks(b), 1);
    }

    #[test]
    fn cycles_are_refused() {
        let (mut g, a, _, sum, dbl) = chain();
        assert_eq!(g.connect(dbl, "out", sum, "a"), Err(GraphError::Cycle));
        assert_eq!(g.connect(sum, "out", sum, "b"), Err(GraphError::Cycle));
        assert!(matches!(
            g.connect(a, "nope", sum, "a"),
            Err(GraphError::NoSuchPort(_))
        ));
    }

    #[test]
    fn time_dirties_only_time_dependent_branches() {
        let mut g = Graph::new();
        let t = g.add("time", Time);
        let lfo = g.add("lfo", Lfo);
        let k = g.add("k", Constant);
        let mix = g.add("mix", Math::Add);
        g.connect(t, "seconds", lfo, "time").unwrap();
        g.connect(lfo, "out", mix, "a").unwrap();
        g.connect(k, "out", mix, "b").unwrap();
        g.set_param(lfo, "frequency", Value::Float(0.25));
        g.set_time(1.0);
        assert!((g.pull(mix, "out").unwrap().as_f32() - 1.0).abs() < 1e-5);
        g.set_time(2.0);
        g.pull(mix, "out").unwrap();
        assert_eq!(
            g.last_cooks, 3,
            "time, lfo, mix — the constant stays cached"
        );
        assert_eq!(g.cooks(k), 1);
    }

    #[test]
    fn expressions_trails_switches_and_remaps() {
        let mut g = Graph::new();
        let t = g.add("time", Time);
        let e = g.add("e", Expression::default());
        g.set_param(e, "expr", Value::Text("a * 10 + sin(0)".into()));
        g.connect(t, "seconds", e, "a").unwrap();
        let trail = g.add("trail", Trail::default());
        g.set_param(trail, "length", Value::Float(3.0));
        g.connect(e, "out", trail, "in").unwrap();
        for s in 1..=5 {
            #[allow(clippy::cast_precision_loss)]
            g.set_time(s as f32);
            g.cook_all();
        }
        assert_eq!(
            g.pull(trail, "channel").unwrap(),
            Value::Channel(vec![30.0, 40.0, 50.0])
        );
        let r = g.add("r", Remap);
        g.set_param(r, "from_hi", Value::Float(100.0));
        g.set_param(r, "to_hi", Value::Float(1.0));
        g.connect(e, "out", r, "in").unwrap();
        assert!((g.pull(r, "out").unwrap().as_f32() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn exec_graph_branches_loops_and_sets_variables() {
        let mut data = Graph::new();
        let cond = data.add("cond", Constant);
        data.set_param(cond, "value", Value::Bool(true));
        let three = data.add("three", Constant);
        data.set_param(three, "value", Value::Float(3.0));
        let mut ex = ExecGraph::new();
        let begin = ex.add(ExecNode::Event("BeginPlay".into()));
        let seq = ex.add(ExecNode::Sequence(2));
        let branch = ex.add(ExecNode::Branch {
            condition: (cond, "out".into()),
        });
        let yes = ex.add(ExecNode::Call("Open Door".into()));
        let no = ex.add(ExecNode::Call("Lock Door".into()));
        let lp = ex.add(ExecNode::ForLoop {
            count: (three, "out".into()),
            index_var: "index".into(),
        });
        let body = ex.add(ExecNode::Call("Spawn".into()));
        let set = ex.add(ExecNode::SetVariable {
            name: "spawned".into(),
            value: (three, "out".into()),
        });
        ex.then(begin, 0, seq);
        ex.then(seq, 0, branch);
        ex.then(branch, 0, yes);
        ex.then(branch, 1, no);
        ex.then(seq, 1, lp);
        ex.then(lp, 0, body);
        ex.then(lp, 1, set);
        let trace = ex.fire("BeginPlay", &mut data).unwrap();
        assert_eq!(
            trace,
            [
                "call Open Door",
                "call Spawn #0",
                "call Spawn #1",
                "call Spawn #2",
                "set spawned = 3.000"
            ]
        );
        data.set_param(cond, "value", Value::Bool(false));
        let trace = ex.fire("BeginPlay", &mut data).unwrap();
        assert_eq!(trace[0], "call Lock Door");
        assert!(ex.fire("Nope", &mut data).is_err());
    }

    #[test]
    fn a_channel_exported_to_a_parameter_drives_it() {
        let mut g = Graph::new();
        let t = g.add("time", Time);
        let c = g.add("const", Constant);
        g.export(t, "seconds", c, "value").unwrap();
        g.set_time(2.5);
        assert_eq!(g.pull(c, "out").unwrap().as_f32(), 2.5);
        g.set_time(4.0);
        assert_eq!(g.pull(c, "out").unwrap().as_f32(), 4.0, "the time change dirtied the target");
        assert_eq!(g.param(c, "value").map(Value::as_f32), Some(4.0));
        g.unexport(c, "value");
        g.set_time(9.0);
        assert_eq!(g.pull(c, "out").unwrap().as_f32(), 4.0, "unexported: the last value stays");
        assert!(g.export(t, "nope", c, "value").is_err());
    }
}
