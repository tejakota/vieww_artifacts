//! Expressions — After Effects' expression engine, Blender's drivers,
//! TouchDesigner's parameter expressions.
//!
//! # The capability
//!
//! After Effects (§2.15 L5, "Expression Engine") lets any property be a
//! *formula* instead of keyframes: `wiggle(3, 20)`, `loopOut("pingpong")`,
//! `linear(time, 0, 2, 0, 360)`, `value + [0, Math.sin(time*4)*10]`.
//! Blender's drivers are the same idea across objects (`var * 2 + 0.5`), and
//! TouchDesigner parameters take Python expressions. It is procedural
//! animation without code changes — the formula is *data* on the property.
//!
//! This module is a small, total language for that:
//!
//! * numbers and **vectors** (`[x, y]`), with element-wise arithmetic and
//!   scalar broadcast, indexing (`value[0]`);
//! * `+ - * / % ^`, comparisons, `&& || !`, `cond ? a : b`;
//! * the scope: `time`, `value`, `index`, `thisComp`-free — plus any
//!   variables the caller binds (a driver's inputs);
//! * the AE vocabulary: `wiggle`, `loopOut`/`loopIn`, `linear`, `ease`,
//!   `easeIn`, `easeOut`, `valueAtTime`, `random`, `noise`, `clamp`, `length`,
//!   `normalize`, `degreesToRadians`/`radiansToDegrees`, and the math
//!   library (`sin`, `atan2`, `pow`, …; `Math.sin` is accepted too).
//!
//! It is parsed once into an [`Expr`] and evaluated per frame; evaluation
//! never panics — a domain error is `NaN`, a type error an [`ExprError`].
//! `random` and `wiggle` are **seeded** (by `index` and a per-expression
//! seed), so a render is reproducible, which After Effects also guarantees.
//!
//! ```
//! use vieww_animation::expr::{Expr, Scope, Val};
//!
//! let e = Expr::parse("linear(time, 0, 2, 0, 360)").unwrap();
//! let v = e.eval(&Scope::at(1.0)).unwrap();
//! assert_eq!(v, Val::Num(180.0));
//!
//! let e = Expr::parse("value + [0, sin(time * PI) * 10]").unwrap();
//! let scope = Scope::at(0.5).with_value(Val::Vec(vec![100.0, 100.0]));
//! assert_eq!(e.eval(&scope).unwrap(), Val::Vec(vec![100.0, 110.0]));
//! ```

use std::collections::BTreeMap;
use std::fmt;

use crate::noise::Perlin;

/// A value: a number or a vector of numbers.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Num(f32),
    Vec(Vec<f32>),
    Str(String),
}

impl Val {
    /// The number, or the first component.
    #[must_use]
    pub fn num(&self) -> f32 {
        match self {
            Self::Num(n) => *n,
            Self::Vec(v) => v.first().copied().unwrap_or(0.0),
            Self::Str(_) => f32::NAN,
        }
    }

    fn truthy(&self) -> bool {
        match self {
            Self::Num(n) => *n != 0.0 && !n.is_nan(),
            Self::Vec(v) => !v.is_empty(),
            Self::Str(s) => !s.is_empty(),
        }
    }

    fn zip(&self, other: &Self, f: impl Fn(f32, f32) -> f32) -> Result<Self, ExprError> {
        Ok(match (self, other) {
            (Self::Num(a), Self::Num(b)) => Self::Num(f(*a, *b)),
            (Self::Vec(a), Self::Num(b)) => Self::Vec(a.iter().map(|x| f(*x, *b)).collect()),
            (Self::Num(a), Self::Vec(b)) => Self::Vec(b.iter().map(|x| f(*a, *x)).collect()),
            (Self::Vec(a), Self::Vec(b)) => {
                let n = a.len().max(b.len());
                Self::Vec(
                    (0..n)
                        .map(|i| f(a.get(i).copied().unwrap_or(0.0), b.get(i).copied().unwrap_or(0.0)))
                        .collect(),
                )
            }
            _ => return Err(ExprError("arithmetic on a string".into())),
        })
    }

    fn map(&self, f: impl Fn(f32) -> f32) -> Self {
        match self {
            Self::Num(a) => Self::Num(f(*a)),
            Self::Vec(v) => Self::Vec(v.iter().map(|x| f(*x)).collect()),
            Self::Str(s) => Self::Str(s.clone()),
        }
    }
}

impl From<f32> for Val {
    fn from(n: f32) -> Self {
        Self::Num(n)
    }
}

/// Why an expression did not parse or evaluate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprError(pub String);

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ExprError {}

/// Where an expression is evaluated: the clock, the property's own value,
/// bound variables, and (optionally) the property's keyframed curve.
pub struct Scope<'a> {
    pub time: f32,
    pub value: Val,
    pub index: f32,
    pub seed: u64,
    pub vars: BTreeMap<String, Val>,
    /// The property's un-expressed value at any time — for `valueAtTime`,
    /// `loopOut`, `wiggle` around the keyframes.
    pub curve: Option<&'a dyn Fn(f32) -> Val>,
    /// First and last keyframe times, for `loopOut`/`loopIn`.
    pub key_range: Option<(f32, f32)>,
}

impl fmt::Debug for Scope<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scope")
            .field("time", &self.time)
            .field("value", &self.value)
            .field("vars", &self.vars)
            .finish_non_exhaustive()
    }
}

impl<'a> Scope<'a> {
    /// A scope at `time` with value 0.
    #[must_use]
    pub fn at(time: f32) -> Self {
        Self {
            time,
            value: Val::Num(0.0),
            index: 1.0,
            seed: 0,
            vars: BTreeMap::new(),
            curve: None,
            key_range: None,
        }
    }

    #[must_use]
    pub fn with_value(mut self, v: Val) -> Self {
        self.value = v;
        self
    }

    #[must_use]
    pub fn with_var(mut self, name: &str, v: impl Into<Val>) -> Self {
        self.vars.insert(name.to_owned(), v.into());
        self
    }

    #[must_use]
    pub const fn with_index(mut self, index: f32) -> Self {
        self.index = index;
        self
    }

    #[must_use]
    pub const fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Attach the property's keyframed curve and its key range.
    #[must_use]
    pub fn with_curve(mut self, curve: &'a dyn Fn(f32) -> Val, first: f32, last: f32) -> Self {
        self.curve = Some(curve);
        self.key_range = Some((first, last));
        self
    }

    fn value_at(&self, t: f32) -> Val {
        self.curve.map_or_else(|| self.value.clone(), |c| c(t))
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Num(f32),
    Str(String),
    Var(String),
    Vec(Vec<Node>),
    Unary(char, Box<Node>),
    Binary(&'static str, Box<Node>, Box<Node>),
    Ternary(Box<Node>, Box<Node>, Box<Node>),
    Call(String, Vec<Node>),
    Index(Box<Node>, Box<Node>),
}

/// A parsed expression.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    root: Node,
    source: String,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f32),
    Ident(String),
    Str(String),
    Op(&'static str),
}

fn lex(src: &str) -> Result<Vec<Tok>, ExprError> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    const OPS: [&str; 22] = [
        "&&", "||", "==", "!=", "<=", ">=", "+", "-", "*", "/", "%", "^", "<", ">", "!", "?", ":", "(", ")", "[", "]", ",",
    ];
    while i < b.len() {
        let c = b[i] as char;
        if c.is_whitespace() || c == ';' {
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            let s = i;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
                i += 1;
                if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
                    i += 1;
                }
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
            }
            out.push(Tok::Num(src[s..i].parse().map_err(|_| ExprError(format!("bad number at {s}")))?));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let s = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'.') {
                i += 1;
            }
            out.push(Tok::Ident(src[s..i].to_owned()));
        } else if c == '"' || c == '\'' {
            let q = b[i];
            i += 1;
            let s = i;
            while i < b.len() && b[i] != q {
                i += 1;
            }
            if i >= b.len() {
                return Err(ExprError("unterminated string".into()));
            }
            out.push(Tok::Str(src[s..i].to_owned()));
            i += 1;
        } else if let Some(op) = OPS.iter().find(|op| src[i..].starts_with(**op)) {
            out.push(Tok::Op(op));
            i += op.len();
        } else {
            return Err(ExprError(format!("unexpected '{c}' at {i}")));
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<Tok>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }
    fn eat(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Op(o)) if *o == op) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, op: &str) -> Result<(), ExprError> {
        if self.eat(op) {
            Ok(())
        } else {
            Err(ExprError(format!("expected '{op}'")))
        }
    }

    fn ternary(&mut self) -> Result<Node, ExprError> {
        let cond = self.binary(0)?;
        if self.eat("?") {
            let a = self.ternary()?;
            self.expect(":")?;
            let b = self.ternary()?;
            return Ok(Node::Ternary(Box::new(cond), Box::new(a), Box::new(b)));
        }
        Ok(cond)
    }

    fn binary(&mut self, min: u8) -> Result<Node, ExprError> {
        let mut lhs = self.unary()?;
        while let Some(Tok::Op(op)) = self.peek().cloned() {
            let (prec, right) = match op {
                "||" => (1, false),
                "&&" => (2, false),
                "==" | "!=" => (3, false),
                "<" | ">" | "<=" | ">=" => (4, false),
                "+" | "-" => (5, false),
                "*" | "/" | "%" => (6, false),
                "^" => (8, true),
                _ => break,
            };
            if prec < min {
                break;
            }
            self.at += 1;
            let rhs = self.binary(if right { prec } else { prec + 1 })?;
            lhs = Node::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Node, ExprError> {
        if self.eat("-") {
            return Ok(Node::Unary('-', Box::new(self.binary(7)?)));
        }
        if self.eat("+") {
            return self.binary(7);
        }
        if self.eat("!") {
            return Ok(Node::Unary('!', Box::new(self.binary(7)?)));
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Node, ExprError> {
        let mut node = self.primary()?;
        while self.eat("[") {
            let i = self.ternary()?;
            self.expect("]")?;
            node = Node::Index(Box::new(node), Box::new(i));
        }
        Ok(node)
    }

    fn list(&mut self, close: &str) -> Result<Vec<Node>, ExprError> {
        let mut items = Vec::new();
        if self.eat(close) {
            return Ok(items);
        }
        loop {
            items.push(self.ternary()?);
            if self.eat(close) {
                return Ok(items);
            }
            self.expect(",")?;
        }
    }

    fn primary(&mut self) -> Result<Node, ExprError> {
        let tok = self.peek().cloned().ok_or_else(|| ExprError("unexpected end".into()))?;
        self.at += 1;
        match tok {
            Tok::Num(n) => Ok(Node::Num(n)),
            Tok::Str(s) => Ok(Node::Str(s)),
            Tok::Op("(") => {
                let e = self.ternary()?;
                self.expect(")")?;
                Ok(e)
            }
            Tok::Op("[") => Ok(Node::Vec(self.list("]")?)),
            Tok::Ident(name) => {
                let name = name.strip_prefix("Math.").unwrap_or(&name).to_owned();
                if self.eat("(") {
                    Ok(Node::Call(name, self.list(")")?))
                } else {
                    Ok(Node::Var(name))
                }
            }
            Tok::Op(op) => Err(ExprError(format!("unexpected '{op}'"))),
        }
    }
}

/// A seeded hash to [0, 1).
fn hash01(mut x: u64) -> f32 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^= x >> 33;
    #[allow(clippy::cast_precision_loss)]
    let r = (x >> 40) as f32 / (1u64 << 24) as f32;
    r
}

fn remap(t: f32, t0: f32, t1: f32, v0: &Val, v1: &Val, ease: impl Fn(f32) -> f32) -> Result<Val, ExprError> {
    let u = if (t1 - t0).abs() < 1e-9 { 1.0 } else { ((t - t0) / (t1 - t0)).clamp(0.0, 1.0) };
    let e = ease(u);
    v0.zip(v1, |a, b| a + (b - a) * e)
}

impl Expr {
    /// Parse `source`.
    ///
    /// # Errors
    ///
    /// Lexing or syntax errors, with a position or the expected token.
    pub fn parse(source: &str) -> Result<Self, ExprError> {
        let toks = lex(source)?;
        let mut p = Parser { toks, at: 0 };
        let root = p.ternary()?;
        if p.at != p.toks.len() {
            return Err(ExprError(format!("unexpected trailing input at token {}", p.at)));
        }
        Ok(Self {
            root,
            source: source.to_owned(),
        })
    }

    /// The text this was parsed from.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Evaluate in `scope`.
    ///
    /// # Errors
    ///
    /// Unknown variables or functions, wrong argument counts, arithmetic on
    /// strings.
    pub fn eval(&self, scope: &Scope<'_>) -> Result<Val, ExprError> {
        eval(&self.root, scope)
    }

    /// Evaluate to a number (first component of a vector).
    ///
    /// # Errors
    ///
    /// As [`eval`](Self::eval).
    pub fn eval_num(&self, scope: &Scope<'_>) -> Result<f32, ExprError> {
        self.eval(scope).map(|v| v.num())
    }
}

#[allow(clippy::too_many_lines)]
fn eval(node: &Node, s: &Scope<'_>) -> Result<Val, ExprError> {
    Ok(match node {
        Node::Num(n) => Val::Num(*n),
        Node::Str(t) => Val::Str(t.clone()),
        Node::Var(name) => match name.as_str() {
            "time" => Val::Num(s.time),
            "value" => s.value.clone(),
            "index" => Val::Num(s.index),
            "PI" | "pi" => Val::Num(std::f32::consts::PI),
            "E" => Val::Num(std::f32::consts::E),
            "true" => Val::Num(1.0),
            "false" => Val::Num(0.0),
            other => s
                .vars
                .get(other)
                .cloned()
                .ok_or_else(|| ExprError(format!("unknown variable '{other}'")))?,
        },
        Node::Vec(items) => Val::Vec(items.iter().map(|n| eval(n, s).map(|v| v.num())).collect::<Result<_, _>>()?),
        Node::Unary('-', e) => eval(e, s)?.map(|x| -x),
        Node::Unary(_, e) => Val::Num(f32::from(u8::from(!eval(e, s)?.truthy()))),
        Node::Index(v, i) => {
            let v = eval(v, s)?;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let i = eval(i, s)?.num().max(0.0) as usize;
            match v {
                Val::Vec(items) => Val::Num(items.get(i).copied().unwrap_or(f32::NAN)),
                Val::Num(n) if i == 0 => Val::Num(n),
                _ => Val::Num(f32::NAN),
            }
        }
        Node::Ternary(c, a, b) => {
            if eval(c, s)?.truthy() {
                eval(a, s)?
            } else {
                eval(b, s)?
            }
        }
        Node::Binary(op, a, b) => {
            if *op == "&&" {
                return Ok(Val::Num(f32::from(u8::from(eval(a, s)?.truthy() && eval(b, s)?.truthy()))));
            }
            if *op == "||" {
                return Ok(Val::Num(f32::from(u8::from(eval(a, s)?.truthy() || eval(b, s)?.truthy()))));
            }
            let (x, y) = (eval(a, s)?, eval(b, s)?);
            if let (Val::Str(p), Val::Str(q)) = (&x, &y) {
                return Ok(Val::Num(f32::from(u8::from(match *op {
                    "==" => p == q,
                    "!=" => p != q,
                    _ => return Err(ExprError("strings only compare".into())),
                }))));
            }
            let b2f = |c: bool| f32::from(u8::from(c));
            match *op {
                "+" => x.zip(&y, |p, q| p + q)?,
                "-" => x.zip(&y, |p, q| p - q)?,
                "*" => x.zip(&y, |p, q| p * q)?,
                "/" => x.zip(&y, |p, q| p / q)?,
                "%" => x.zip(&y, |p, q| p % q)?,
                "^" => x.zip(&y, f32::powf)?,
                "<" => Val::Num(b2f(x.num() < y.num())),
                ">" => Val::Num(b2f(x.num() > y.num())),
                "<=" => Val::Num(b2f(x.num() <= y.num())),
                ">=" => Val::Num(b2f(x.num() >= y.num())),
                "==" => Val::Num(b2f(x == y)),
                "!=" => Val::Num(b2f(x != y)),
                _ => unreachable!("parser only produces known operators"),
            }
        }
        Node::Call(name, args) => call(name, args, s)?,
    })
}

#[allow(clippy::too_many_lines)]
fn call(name: &str, args: &[Node], s: &Scope<'_>) -> Result<Val, ExprError> {
    let vals = args.iter().map(|a| eval(a, s)).collect::<Result<Vec<_>, _>>()?;
    let need = |n: usize| {
        if vals.len() < n {
            Err(ExprError(format!("{name} needs {n} argument(s)")))
        } else {
            Ok(())
        }
    };
    let n = |i: usize| vals.get(i).map_or(0.0, Val::num);
    let unary = |f: fn(f32) -> f32| -> Result<Val, ExprError> {
        need(1)?;
        Ok(vals[0].map(f))
    };
    Ok(match name {
        "sin" => unary(f32::sin)?,
        "cos" => unary(f32::cos)?,
        "tan" => unary(f32::tan)?,
        "asin" => unary(f32::asin)?,
        "acos" => unary(f32::acos)?,
        "atan" => unary(f32::atan)?,
        "sqrt" => unary(f32::sqrt)?,
        "abs" => unary(f32::abs)?,
        "floor" => unary(f32::floor)?,
        "ceil" => unary(f32::ceil)?,
        "round" => unary(f32::round)?,
        "exp" => unary(f32::exp)?,
        "log" => unary(f32::ln)?,
        "sign" => unary(f32::signum)?,
        "fract" => unary(f32::fract)?,
        "degreesToRadians" | "radians" => unary(f32::to_radians)?,
        "radiansToDegrees" | "degrees" => unary(f32::to_degrees)?,
        "atan2" => {
            need(2)?;
            Val::Num(n(0).atan2(n(1)))
        }
        "pow" => {
            need(2)?;
            vals[0].zip(&vals[1], f32::powf)?
        }
        "min" => vals.iter().map(Val::num).reduce(f32::min).map(Val::Num).ok_or_else(|| ExprError("min()".into()))?,
        "max" => vals.iter().map(Val::num).reduce(f32::max).map(Val::Num).ok_or_else(|| ExprError("max()".into()))?,
        "clamp" => {
            need(3)?;
            let (lo, hi) = (n(1), n(2));
            vals[0].map(|x| x.clamp(lo.min(hi), hi.max(lo)))
        }
        "mix" | "lerp" => {
            need(3)?;
            let t = n(2);
            vals[0].zip(&vals[1], |a, b| a + (b - a) * t)?
        }
        "step" => {
            need(2)?;
            Val::Num(f32::from(u8::from(n(1) >= n(0))))
        }
        "smoothstep" => {
            need(3)?;
            let t = ((n(2) - n(0)) / (n(1) - n(0))).clamp(0.0, 1.0);
            Val::Num(t * t * (3.0 - 2.0 * t))
        }
        "length" => match (vals.first(), vals.get(1)) {
            (Some(a), Some(b)) => match a.zip(b, |p, q| p - q)? {
                Val::Vec(v) => Val::Num(v.iter().map(|x| x * x).sum::<f32>().sqrt()),
                Val::Num(x) => Val::Num(x.abs()),
                Val::Str(_) => Val::Num(f32::NAN),
            },
            (Some(Val::Vec(v)), None) => Val::Num(v.iter().map(|x| x * x).sum::<f32>().sqrt()),
            (Some(v), None) => Val::Num(v.num().abs()),
            _ => return Err(ExprError("length()".into())),
        },
        "normalize" => match vals.first() {
            Some(Val::Vec(v)) => {
                let l = v.iter().map(|x| x * x).sum::<f32>().sqrt();
                Val::Vec(v.iter().map(|x| if l > 0.0 { x / l } else { 0.0 }).collect())
            }
            Some(v) => Val::Num(v.num().signum()),
            None => return Err(ExprError("normalize()".into())),
        },
        // AE's interpolation family: (t, tMin, tMax, v1, v2) or (t, v1, v2).
        "linear" | "ease" | "easeIn" | "easeOut" => {
            let ease: fn(f32) -> f32 = match name {
                "linear" => |u| u,
                "ease" => |u| u * u * (3.0 - 2.0 * u),
                "easeIn" => |u| u * u,
                _ => |u| 1.0 - (1.0 - u) * (1.0 - u),
            };
            match vals.len() {
                5 => remap(n(0), n(1), n(2), &vals[3], &vals[4], ease)?,
                3 => remap(n(0), 0.0, 1.0, &vals[1], &vals[2], ease)?,
                _ => return Err(ExprError(format!("{name} takes 3 or 5 arguments"))),
            }
        }
        "random" => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let key = s.seed ^ ((s.index as u64) << 20) ^ ((s.time * 1000.0) as u64).wrapping_mul(0x9E37);
            let r = hash01(key);
            match vals.len() {
                0 => Val::Num(r),
                1 => vals[0].map(|m| r * m),
                _ => vals[0].zip(&vals[1], |a, b| a + (b - a) * r)?,
            }
        }
        "noise" => {
            let p = Perlin::from_seed(s.seed);
            Val::Num(if vals.len() >= 2 { p.noise2(n(0), n(1)) } else { p.noise1(n(0)) })
        }
        // wiggle(freq, amp, octaves = 1, amp_mult = 0.5, t = time)
        "wiggle" => {
            need(2)?;
            let freq = n(0);
            let amp = n(1);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let octaves = if vals.len() > 2 { n(2).max(1.0) as u32 } else { 1 };
            let mult = if vals.len() > 3 { n(3) } else { 0.5 };
            let t = if vals.len() > 4 { n(4) } else { s.time };
            let base = s.value_at(t);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let seed = s.seed ^ (s.index as u64).wrapping_mul(0x51_7CC1);
            let dims = match &base {
                Val::Vec(v) => v.len(),
                _ => 1,
            };
            let offs: Vec<f32> = (0..dims)
                .map(|d| {
                    #[allow(clippy::cast_possible_truncation)]
                    let p = Perlin::from_seed(seed.wrapping_add(d as u64 * 7919));
                    let (mut a, mut f, mut sum) = (1.0, 1.0, 0.0);
                    for _ in 0..octaves {
                        sum += a * p.noise1(t * freq * f);
                        a *= mult;
                        f *= 2.0;
                    }
                    sum * amp
                })
                .collect();
            match base {
                Val::Vec(v) => Val::Vec(v.iter().zip(&offs).map(|(x, o)| x + o).collect()),
                other => Val::Num(other.num() + offs[0]),
            }
        }
        "valueAtTime" => {
            need(1)?;
            s.value_at(n(0))
        }
        // loopOut(type = "cycle"), loopIn(type)
        "loopOut" | "loopIn" => {
            let kind = match vals.first() {
                Some(Val::Str(k)) => k.as_str(),
                _ => "cycle",
            };
            let Some((first, last)) = s.key_range else { return Ok(s.value.clone()) };
            let span = last - first;
            if span <= 0.0 {
                return Ok(s.value.clone());
            }
            let t = s.time;
            let outside = if name == "loopOut" { t > last } else { t < first };
            if !outside {
                return Ok(s.value_at(t));
            }
            let rel = t - first;
            let cycle = (rel / span).floor();
            let local = rel - cycle * span;
            match kind {
                "pingpong" => {
                    #[allow(clippy::cast_possible_truncation)]
                    let odd = (cycle as i64).rem_euclid(2) == 1;
                    s.value_at(first + if odd { span - local } else { local })
                }
                "offset" => {
                    let delta = s.value_at(last).zip(&s.value_at(first), |a, b| a - b)?;
                    let v = s.value_at(first + local);
                    v.zip(&delta, |a, d| a + d * cycle)?
                }
                "continue" => {
                    let e = 1e-3;
                    let (edge, dir) = if name == "loopOut" { (last, 1.0) } else { (first, -1.0) };
                    let slope = s.value_at(edge).zip(&s.value_at(edge - dir * e), |a, b| (a - b) / e)?;
                    s.value_at(edge).zip(&slope, |a, m| a + m * (t - edge) * dir * dir)?
                }
                _ => s.value_at(first + local),
            }
        }
        other => return Err(ExprError(format!("unknown function '{other}'"))),
    })
}

/// A Blender-style driver: an expression over named variables, each read
/// from a channel the caller resolves.
#[derive(Debug, Clone)]
pub struct Driver {
    pub expr: Expr,
    /// Variable name → source channel name.
    pub inputs: Vec<(String, String)>,
}

impl Driver {
    /// # Errors
    ///
    /// The expression does not parse.
    pub fn new(expression: &str) -> Result<Self, ExprError> {
        Ok(Self {
            expr: Expr::parse(expression)?,
            inputs: Vec::new(),
        })
    }

    /// Bind `var` to the value of channel `source`.
    #[must_use]
    pub fn input(mut self, var: &str, source: &str) -> Self {
        self.inputs.push((var.to_owned(), source.to_owned()));
        self
    }

    /// Evaluate with channel values from `lookup` at `time`.
    ///
    /// # Errors
    ///
    /// An input channel is missing, or evaluation fails.
    pub fn evaluate(&self, time: f32, lookup: impl Fn(&str) -> Option<f32>) -> Result<f32, ExprError> {
        let mut scope = Scope::at(time);
        for (var, source) in &self.inputs {
            let v = lookup(source).ok_or_else(|| ExprError(format!("driver input '{source}' missing")))?;
            scope.vars.insert(var.clone(), Val::Num(v));
        }
        self.expr.eval_num(&scope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(src: &str, scope: &Scope<'_>) -> Val {
        Expr::parse(src).unwrap().eval(scope).unwrap()
    }

    fn num(src: &str) -> f32 {
        ev(src, &Scope::at(0.0)).num()
    }

    #[test]
    fn arithmetic_precedence_and_associativity() {
        assert_eq!(num("1 + 2 * 3"), 7.0);
        assert_eq!(num("(1 + 2) * 3"), 9.0);
        assert_eq!(num("2 ^ 3 ^ 2"), 512.0, "right-associative power");
        assert_eq!(num("-2 ^ 2"), -4.0, "unary minus binds looser than ^");
        assert_eq!(num("10 % 4 - 1"), 1.0);
        assert_eq!(num("1 < 2 && 3 >= 3 ? 5 : 6"), 5.0);
        assert_eq!(num("!0 + !1"), 1.0);
    }

    #[test]
    fn vectors_broadcast_and_index() {
        assert_eq!(ev("[1, 2] * 3 + [0, 1]", &Scope::at(0.0)), Val::Vec(vec![3.0, 7.0]));
        assert_eq!(num("[4, 5, 6][2]"), 6.0);
        assert_eq!(num("length([3, 4])"), 5.0);
        assert_eq!(num("length([0,0], [3,4])"), 5.0);
        assert_eq!(ev("normalize([0, 2])", &Scope::at(0.0)), Val::Vec(vec![0.0, 1.0]));
    }

    #[test]
    fn math_library_accepts_the_math_prefix() {
        assert!((num("Math.sin(PI / 2)") - 1.0).abs() < 1e-6);
        assert!((num("atan2(1, 1)") - std::f32::consts::FRAC_PI_4).abs() < 1e-6);
        assert_eq!(num("clamp(5, 0, 3)"), 3.0);
        assert_eq!(num("max(1, 7, 3)"), 7.0);
        assert!((num("radiansToDegrees(PI)") - 180.0).abs() < 1e-3);
    }

    #[test]
    fn linear_and_ease_remap_like_after_effects() {
        let s = Scope::at(1.0);
        assert_eq!(ev("linear(time, 0, 2, 0, 100)", &s).num(), 50.0);
        assert_eq!(ev("linear(time, 0, 0.5, 0, 100)", &s).num(), 100.0, "clamped");
        assert_eq!(ev("ease(0.5, [0,0], [10,20])", &s), Val::Vec(vec![5.0, 10.0]));
        assert!(ev("easeIn(0.5, 0, 1)", &s).num() < 0.5 && ev("easeOut(0.5, 0, 1)", &s).num() > 0.5);
    }

    #[test]
    fn wiggle_is_seeded_bounded_and_per_axis() {
        let s = Scope::at(1.3).with_value(Val::Vec(vec![100.0, 100.0])).with_seed(9);
        let a = ev("wiggle(2, 10)", &s);
        let b = ev("wiggle(2, 10)", &s);
        assert_eq!(a, b, "same seed, same answer");
        let Val::Vec(v) = a else { panic!() };
        assert!(v.iter().all(|x| (x - 100.0).abs() <= 10.0));
        assert_ne!(v[0], v[1], "each axis wiggles on its own");
        let other = ev("wiggle(2, 10)", &Scope::at(1.3).with_value(Val::Vec(vec![100.0, 100.0])).with_seed(9).with_index(2.0));
        assert_ne!(other, Val::Vec(v), "layers with different indices differ");
    }

    #[test]
    fn loop_out_cycles_pingpongs_offsets_and_continues() {
        let curve = |t: f32| Val::Num(t * 10.0); // keys at 0 and 1, value 0 → 10
        let at = |t: f32, src: &str| {
            let s = Scope::at(t).with_curve(&curve, 0.0, 1.0);
            Expr::parse(src).unwrap().eval(&s).unwrap().num()
        };
        assert!((at(2.25, "loopOut()") - 2.5).abs() < 1e-4);
        assert!((at(1.25, "loopOut('pingpong')") - 7.5).abs() < 1e-4);
        assert!((at(2.25, "loopOut(\"offset\")") - 22.5).abs() < 1e-3);
        assert!((at(1.5, "loopOut('continue')") - 15.0).abs() < 1e-2);
        assert!((at(0.5, "loopOut()") - 5.0).abs() < 1e-4, "inside the keys: the curve");
        assert!((at(-0.25, "loopIn()") - 7.5).abs() < 1e-4);
    }

    #[test]
    fn random_is_reproducible_and_ranged() {
        let s = Scope::at(0.5).with_seed(3);
        let r = ev("random(10, 20)", &s).num();
        assert!((10.0..20.0).contains(&r));
        assert_eq!(r, ev("random(10, 20)", &s).num());
    }

    #[test]
    fn drivers_read_bound_channels() {
        let d = Driver::new("rot * 0.5 + offset").unwrap().input("rot", "arm.rotation").input("offset", "bias");
        let v = d.evaluate(0.0, |c| match c {
            "arm.rotation" => Some(2.0),
            "bias" => Some(0.25),
            _ => None,
        });
        assert_eq!(v, Ok(1.25));
        assert!(d.evaluate(0.0, |_| None).is_err());
    }

    #[test]
    fn errors_are_errors_not_panics() {
        assert!(Expr::parse("1 +").is_err());
        assert!(Expr::parse("(1").is_err());
        assert!(Expr::parse("1 2").is_err());
        assert!(Expr::parse("$").is_err());
        assert!(Expr::parse("nope(1)").unwrap().eval(&Scope::at(0.0)).is_err());
        assert!(Expr::parse("x").unwrap().eval(&Scope::at(0.0)).is_err());
        assert!(num("sqrt(-1)").is_nan());
    }
}
