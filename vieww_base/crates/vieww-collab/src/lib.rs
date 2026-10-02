//! Live collaboration — the "Collaborative Design Tool Integration" row
//! of the capability matrix (Spline, Rive, Figma: several people editing
//! one document at once, "Figma-like collaborative editing").
//!
//! # The model
//!
//! Every replica edits locally with no waiting and broadcasts
//! **operations**; any replica that has applied the same set of operations
//! — in *any* order, with duplicates — holds the same document. That is the
//! CRDT guarantee (strong eventual consistency), and it is what lets a
//! canvas stay responsive while the network is slow or absent.
//!
//! | type | semantics |
//! |---|---|
//! | [`LwwMap`] | per key, last writer wins by Lamport timestamp, replica id breaks ties; deletes are tombstones so a stale write cannot resurrect a key |
//! | [`OrSet`] | observed-remove set: add wins over a concurrent remove |
//! | [`Counter`] | a PN-counter — increments from everyone add up |
//! | [`Text`] | RGA (replicated growable array): characters addressed by unique ids, concurrent inserts at one position ordered by id, deletes as tombstones — collaborative text |
//! | [`Doc`] | a map, a set, a counter and a text under one clock, with an operation log, [`VectorClock`] sync ([`Doc::ops_since`]) and a JSON wire format |
//! | [`Presence`] | ephemeral per-replica cursors and selections |
//!
//! Transport is not here — operations are values; send them over
//! `vieww-network`, a WebSocket, or a shared file.

use std::collections::{BTreeMap, BTreeSet};

use vieww_foundation::json::Json;

/// A replica's identity.
pub type ReplicaId = u32;

/// A Lamport timestamp with the replica as tie-breaker: a total order on
/// operations that respects causality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Stamp {
    pub counter: u64,
    pub replica: ReplicaId,
}

/// Per-replica high-water marks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VectorClock(pub BTreeMap<ReplicaId, u64>);

impl VectorClock {
    #[must_use]
    pub fn get(&self, r: ReplicaId) -> u64 {
        self.0.get(&r).copied().unwrap_or(0)
    }
    fn observe(&mut self, s: Stamp) {
        let e = self.0.entry(s.replica).or_insert(0);
        *e = (*e).max(s.counter);
    }
    /// Whether this clock has seen everything `other` has.
    #[must_use]
    pub fn dominates(&self, other: &Self) -> bool {
        other.0.iter().all(|(r, c)| self.get(*r) >= *c)
    }
}

/// One replicated change.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Set {
        stamp: Stamp,
        key: String,
        value: Option<Json>,
    },
    SetAdd {
        stamp: Stamp,
        element: String,
    },
    SetRemove {
        stamp: Stamp,
        element: String,
        observed: Vec<Stamp>,
    },
    Count {
        stamp: Stamp,
        delta: i64,
    },
    Insert {
        stamp: Stamp,
        after: Option<Stamp>,
        ch: char,
    },
    Delete {
        stamp: Stamp,
        target: Stamp,
    },
}

impl Op {
    #[must_use]
    pub const fn stamp(&self) -> Stamp {
        match self {
            Self::Set { stamp, .. }
            | Self::SetAdd { stamp, .. }
            | Self::SetRemove { stamp, .. }
            | Self::Count { stamp, .. }
            | Self::Insert { stamp, .. }
            | Self::Delete { stamp, .. } => *stamp,
        }
    }

    /// The wire format.
    #[must_use]
    pub fn to_json(&self) -> Json {
        let st = |s: &Stamp| Json::numbers([s.counter as f64, f64::from(s.replica)]);
        match self {
            Self::Set { stamp, key, value } => Json::object([
                ("op", Json::from("set")),
                ("s", st(stamp)),
                ("k", Json::from(key.as_str())),
                ("v", value.clone().unwrap_or(Json::Null)),
                ("del", Json::Bool(value.is_none())),
            ]),
            Self::SetAdd { stamp, element } => Json::object([
                ("op", Json::from("add")),
                ("s", st(stamp)),
                ("e", Json::from(element.as_str())),
            ]),
            Self::SetRemove {
                stamp,
                element,
                observed,
            } => Json::object([
                ("op", Json::from("rm")),
                ("s", st(stamp)),
                ("e", Json::from(element.as_str())),
                ("obs", Json::Array(observed.iter().map(st).collect())),
            ]),
            #[allow(clippy::cast_precision_loss)]
            Self::Count { stamp, delta } => Json::object([
                ("op", Json::from("count")),
                ("s", st(stamp)),
                ("d", Json::from(*delta as f64)),
            ]),
            Self::Insert { stamp, after, ch } => Json::object([
                ("op", Json::from("ins")),
                ("s", st(stamp)),
                ("a", after.as_ref().map_or(Json::Null, st)),
                ("c", Json::from(ch.to_string())),
            ]),
            Self::Delete { stamp, target } => Json::object([
                ("op", Json::from("del")),
                ("s", st(stamp)),
                ("t", st(target)),
            ]),
        }
    }

    /// Parse the wire format.
    ///
    /// # Errors
    ///
    /// A malformed operation.
    pub fn from_json(j: &Json) -> Result<Self, String> {
        let stamp_of = |v: Option<&Json>| -> Result<Stamp, String> {
            let a = v.and_then(Json::as_array).ok_or("stamp")?;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Ok(Stamp {
                counter: a.first().and_then(Json::as_f64).ok_or("counter")? as u64,
                replica: a.get(1).and_then(Json::as_f64).ok_or("replica")? as u32,
            })
        };
        let s = stamp_of(j.get("s"))?;
        let text = |k: &str| {
            j.get(k)
                .and_then(Json::as_str)
                .map(str::to_owned)
                .ok_or_else(|| format!("missing {k}"))
        };
        Ok(match j.get("op").and_then(Json::as_str).ok_or("op")? {
            "set" => Self::Set {
                stamp: s,
                key: text("k")?,
                value: if j.get("del").and_then(Json::as_bool).unwrap_or(false) {
                    None
                } else {
                    j.get("v").cloned()
                },
            },
            "add" => Self::SetAdd {
                stamp: s,
                element: text("e")?,
            },
            "rm" => Self::SetRemove {
                stamp: s,
                element: text("e")?,
                observed: j
                    .get("obs")
                    .and_then(Json::as_array)
                    .unwrap_or(&[])
                    .iter()
                    .map(|x| stamp_of(Some(x)))
                    .collect::<Result<_, _>>()?,
            },
            #[allow(clippy::cast_possible_truncation)]
            "count" => Self::Count {
                stamp: s,
                delta: j.get("d").and_then(Json::as_f64).ok_or("d")? as i64,
            },
            "ins" => Self::Insert {
                stamp: s,
                after: match j.get("a") {
                    Some(Json::Null) | None => None,
                    a => Some(stamp_of(a)?),
                },
                ch: text("c")?.chars().next().ok_or("empty char")?,
            },
            "del" => Self::Delete {
                stamp: s,
                target: stamp_of(j.get("t"))?,
            },
            other => return Err(format!("unknown op {other}")),
        })
    }
}

/// Last-writer-wins map.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LwwMap {
    entries: BTreeMap<String, (Stamp, Option<Json>)>,
}

impl LwwMap {
    fn apply(&mut self, stamp: Stamp, key: &str, value: Option<Json>) {
        match self.entries.get(key) {
            Some((old, _)) if *old >= stamp => {}
            _ => {
                self.entries.insert(key.to_owned(), (stamp, value));
            }
        }
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Json> {
        self.entries.get(key).and_then(|(_, v)| v.as_ref())
    }

    /// Live keys and values.
    #[must_use]
    pub fn entries(&self) -> Vec<(&str, &Json)> {
        self.entries
            .iter()
            .filter_map(|(k, (_, v))| v.as_ref().map(|v| (k.as_str(), v)))
            .collect()
    }
}

/// Observed-remove (add-wins) set.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OrSet {
    adds: BTreeMap<String, BTreeSet<Stamp>>,
    removed: BTreeSet<Stamp>,
}

impl OrSet {
    fn add(&mut self, stamp: Stamp, e: &str) {
        if !self.removed.contains(&stamp) {
            self.adds.entry(e.to_owned()).or_default().insert(stamp);
        }
    }
    fn remove(&mut self, e: &str, observed: &[Stamp]) {
        for s in observed {
            self.removed.insert(*s);
        }
        if let Some(tags) = self.adds.get_mut(e) {
            for s in observed {
                tags.remove(s);
            }
        }
    }
    #[must_use]
    pub fn contains(&self, e: &str) -> bool {
        self.adds.get(e).is_some_and(|t| !t.is_empty())
    }
    #[must_use]
    pub fn elements(&self) -> Vec<&str> {
        self.adds
            .iter()
            .filter(|(_, t)| !t.is_empty())
            .map(|(k, _)| k.as_str())
            .collect()
    }
    fn tags(&self, e: &str) -> Vec<Stamp> {
        self.adds
            .get(e)
            .map(|t| t.iter().copied().collect())
            .unwrap_or_default()
    }
}

/// PN-counter.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Counter {
    seen: BTreeSet<Stamp>,
    value: i64,
}

impl Counter {
    #[must_use]
    pub const fn value(&self) -> i64 {
        self.value
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Elem {
    id: Stamp,
    ch: char,
    deleted: bool,
}

/// RGA text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Text {
    elems: Vec<Elem>,
    /// Deletes that arrived before their target.
    pending_deletes: BTreeSet<Stamp>,
    /// Inserts whose `after` has not arrived yet.
    pending_inserts: Vec<(Stamp, Option<Stamp>, char)>,
}

impl Text {
    fn index_of(&self, id: Stamp) -> Option<usize> {
        self.elems.iter().position(|e| e.id == id)
    }

    fn integrate(&mut self, id: Stamp, after: Option<Stamp>, ch: char) -> bool {
        if self.index_of(id).is_some() {
            return true; // duplicate
        }
        let start = match after {
            None => 0,
            Some(a) => match self.index_of(a) {
                Some(i) => i + 1,
                None => return false,
            },
        };
        // Skip over elements inserted concurrently at the same place with
        // larger ids (they sort first), and their descendants.
        let mut i = start;
        while i < self.elems.len() && self.elems[i].id > id {
            i += 1;
        }
        let deleted = self.pending_deletes.remove(&id);
        self.elems.insert(i, Elem { id, ch, deleted });
        true
    }

    fn insert(&mut self, id: Stamp, after: Option<Stamp>, ch: char) {
        if !self.integrate(id, after, ch) {
            self.pending_inserts.push((id, after, ch));
            return;
        }
        // Retry anything waiting on what just arrived.
        loop {
            let before = self.pending_inserts.len();
            let waiting = std::mem::take(&mut self.pending_inserts);
            for (i, a, c) in waiting {
                if !self.integrate(i, a, c) {
                    self.pending_inserts.push((i, a, c));
                }
            }
            if self.pending_inserts.len() == before {
                break;
            }
        }
    }

    fn delete(&mut self, target: Stamp) {
        match self.index_of(target) {
            Some(i) => self.elems[i].deleted = true,
            None => {
                self.pending_deletes.insert(target);
            }
        }
    }

    /// The visible string.
    #[must_use]
    pub fn value(&self) -> String {
        self.elems
            .iter()
            .filter(|e| !e.deleted)
            .map(|e| e.ch)
            .collect()
    }

    /// Id of the `n`th visible character.
    fn visible_id(&self, n: usize) -> Option<Stamp> {
        self.elems
            .iter()
            .filter(|e| !e.deleted)
            .nth(n)
            .map(|e| e.id)
    }

    /// Visible index of an element id (for mapping a cursor anchored to an
    /// id back to a position after remote edits).
    #[must_use]
    pub fn position_of(&self, id: Stamp) -> Option<usize> {
        let i = self.index_of(id)?;
        Some(self.elems[..i].iter().filter(|e| !e.deleted).count())
    }
}

/// A shared document. See the [crate docs](crate).
#[derive(Debug, Clone)]
pub struct Doc {
    pub replica: ReplicaId,
    clock: u64,
    seen: VectorClock,
    log: Vec<Op>,
    pub map: LwwMap,
    pub set: OrSet,
    pub counter: Counter,
    pub text: Text,
}

impl Doc {
    #[must_use]
    pub fn new(replica: ReplicaId) -> Self {
        Self {
            replica,
            clock: 0,
            seen: VectorClock::default(),
            log: Vec::new(),
            map: LwwMap::default(),
            set: OrSet::default(),
            counter: Counter::default(),
            text: Text::default(),
        }
    }

    fn tick(&mut self) -> Stamp {
        self.clock += 1;
        Stamp {
            counter: self.clock,
            replica: self.replica,
        }
    }

    /// Everything this replica has applied.
    #[must_use]
    pub const fn clock(&self) -> &VectorClock {
        &self.seen
    }

    fn local(&mut self, op: Op) -> Op {
        self.apply(&op);
        op
    }

    /// Set a key (`None` deletes).
    pub fn set(&mut self, key: &str, value: Option<Json>) -> Op {
        let stamp = self.tick();
        self.local(Op::Set {
            stamp,
            key: key.to_owned(),
            value,
        })
    }

    pub fn add(&mut self, element: &str) -> Op {
        let stamp = self.tick();
        self.local(Op::SetAdd {
            stamp,
            element: element.to_owned(),
        })
    }

    pub fn remove(&mut self, element: &str) -> Op {
        let stamp = self.tick();
        let observed = self.set.tags(element);
        self.local(Op::SetRemove {
            stamp,
            element: element.to_owned(),
            observed,
        })
    }

    pub fn count(&mut self, delta: i64) -> Op {
        let stamp = self.tick();
        self.local(Op::Count { stamp, delta })
    }

    /// Insert text at a visible position.
    pub fn insert(&mut self, pos: usize, s: &str) -> Vec<Op> {
        let mut after = if pos == 0 {
            None
        } else {
            self.text.visible_id(pos - 1)
        };
        let mut ops = Vec::new();
        for ch in s.chars() {
            let stamp = self.tick();
            ops.push(self.local(Op::Insert { stamp, after, ch }));
            after = Some(stamp);
        }
        ops
    }

    /// Delete `len` visible characters from `pos`.
    pub fn delete(&mut self, pos: usize, len: usize) -> Vec<Op> {
        let targets: Vec<Stamp> = (pos..pos + len)
            .filter_map(|i| self.text.visible_id(i))
            .collect();
        targets
            .into_iter()
            .map(|target| {
                let stamp = self.tick();
                self.local(Op::Delete { stamp, target })
            })
            .collect()
    }

    /// Apply a (local or remote) operation. Idempotent.
    pub fn apply(&mut self, op: &Op) {
        let s = op.stamp();
        if self.log.iter().any(|o| o.stamp() == s) {
            return;
        }
        self.clock = self.clock.max(s.counter);
        self.seen.observe(s);
        match op {
            Op::Set { key, value, .. } => self.map.apply(s, key, value.clone()),
            Op::SetAdd { element, .. } => self.set.add(s, element),
            Op::SetRemove {
                element, observed, ..
            } => self.set.remove(element, observed),
            Op::Count { delta, .. } => {
                if self.counter.seen.insert(s) {
                    self.counter.value += delta;
                }
            }
            Op::Insert { after, ch, .. } => self.text.insert(s, *after, *ch),
            Op::Delete { target, .. } => self.text.delete(*target),
        }
        self.log.push(op.clone());
    }

    /// Operations the holder of `clock` has not seen — the sync delta.
    #[must_use]
    pub fn ops_since(&self, clock: &VectorClock) -> Vec<Op> {
        self.log
            .iter()
            .filter(|o| o.stamp().counter > clock.get(o.stamp().replica))
            .cloned()
            .collect()
    }

    /// Pull everything new from `other` (a sync round).
    pub fn merge_from(&mut self, other: &Self) -> usize {
        let ops = other.ops_since(&self.seen);
        for o in &ops {
            self.apply(o);
        }
        ops.len()
    }

    /// Everything observable, for comparing replicas.
    #[must_use]
    pub fn snapshot(&self) -> Json {
        Json::object([
            (
                "map",
                Json::Object(
                    self.map
                        .entries()
                        .into_iter()
                        .map(|(k, v)| (k.to_owned(), v.clone()))
                        .collect(),
                ),
            ),
            (
                "set",
                Json::Array(self.set.elements().into_iter().map(Json::from).collect()),
            ),
            #[allow(clippy::cast_precision_loss)]
            ("counter", Json::from(self.counter.value() as f64)),
            ("text", Json::from(self.text.value())),
        ])
    }
}

/// Ephemeral presence: where each collaborator's cursor is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Presence {
    cursors: BTreeMap<ReplicaId, (u64, String, Option<Stamp>)>,
}

impl Presence {
    /// Record `replica`'s cursor (a name and the text element it follows);
    /// the newest `seq` wins.
    pub fn update(&mut self, replica: ReplicaId, seq: u64, name: &str, anchor: Option<Stamp>) {
        let newer = self.cursors.get(&replica).is_none_or(|(s, _, _)| seq > *s);
        if newer {
            self.cursors.insert(replica, (seq, name.to_owned(), anchor));
        }
    }

    /// Everyone's name and current visible cursor position in `text`.
    #[must_use]
    pub fn positions(&self, text: &Text) -> Vec<(ReplicaId, String, usize)> {
        self.cursors
            .iter()
            .map(|(r, (_, n, a))| {
                (
                    *r,
                    n.clone(),
                    a.and_then(|id| text.position_of(id).map(|p| p + 1))
                        .unwrap_or(0),
                )
            })
            .collect()
    }
}

/// The id of the character just before visible position `pos` (a cursor
/// anchor that survives remote edits).
#[must_use]
pub fn anchor_at(text: &Text, pos: usize) -> Option<Stamp> {
    if pos == 0 {
        None
    } else {
        text.visible_id(pos - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deterministic shuffle.
    fn shuffle<T>(v: &mut [T], seed: u64) {
        let mut s = seed | 1;
        for i in (1..v.len()).rev() {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            #[allow(clippy::cast_possible_truncation)]
            let j = (s % (i as u64 + 1)) as usize;
            v.swap(i, j);
        }
    }

    #[test]
    fn concurrent_text_edits_converge_in_any_order() {
        let mut a = Doc::new(1);
        let mut b = Doc::new(2);
        let base = a.insert(0, "hello");
        for o in &base {
            b.apply(o);
        }
        // Concurrent: A appends, B inserts at the front and deletes an "l".
        let mut ops = a.insert(5, " world");
        ops.extend(b.insert(0, ">> "));
        ops.extend(b.delete(5, 1));
        for seed in 1..20 {
            let mut c = Doc::new(3);
            let mut all: Vec<Op> = base.iter().chain(ops.iter()).cloned().collect();
            shuffle(&mut all, seed);
            // Duplicates too.
            let dup = all[0].clone();
            all.push(dup);
            for o in &all {
                c.apply(o);
            }
            assert_eq!(c.text.value(), ">> helo world", "seed {seed}");
        }
        a.merge_from(&b);
        b.merge_from(&a);
        assert_eq!(a.snapshot(), b.snapshot());
    }

    #[test]
    fn concurrent_inserts_at_one_spot_interleave_deterministically() {
        let mut a = Doc::new(1);
        let mut b = Doc::new(2);
        let x = a.insert(0, "AAA");
        let y = b.insert(0, "BBB");
        for o in &y {
            a.apply(o);
        }
        for o in &x {
            b.apply(o);
        }
        assert_eq!(a.text.value(), b.text.value());
        let v = a.text.value();
        assert!(v == "AAABBB" || v == "BBBAAA", "runs stay contiguous: {v}");
    }

    #[test]
    fn lww_map_and_tombstones() {
        let mut a = Doc::new(1);
        let mut b = Doc::new(2);
        let s1 = a.set("color", Some(Json::from("red")));
        b.apply(&s1);
        let s2 = b.set("color", Some(Json::from("blue")));
        let s3 = a.set("color", None); // concurrent delete
        a.apply(&s2);
        b.apply(&s3);
        assert_eq!(a.snapshot(), b.snapshot());
        // Stamps (2,2) vs (2,1): replica 2 wins the tie.
        assert_eq!(a.map.get("color"), Some(&Json::from("blue")));
    }

    #[test]
    fn or_set_add_wins_over_concurrent_remove_and_counter_sums() {
        let mut a = Doc::new(1);
        let mut b = Doc::new(2);
        let add = a.add("layer-1");
        b.apply(&add);
        let rm = a.remove("layer-1");
        let readd = b.add("layer-1");
        a.apply(&readd);
        b.apply(&rm);
        assert!(a.set.contains("layer-1") && b.set.contains("layer-1"));
        let c1 = a.count(5);
        let c2 = b.count(-2);
        a.apply(&c2);
        b.apply(&c1);
        a.apply(&c2); // duplicate
        assert_eq!((a.counter.value(), b.counter.value()), (3, 3));
    }

    #[test]
    fn sync_by_vector_clock_sends_only_the_delta() {
        let mut a = Doc::new(1);
        let mut b = Doc::new(2);
        a.insert(0, "abc");
        assert_eq!(b.merge_from(&a), 3);
        a.insert(3, "d");
        assert_eq!(b.merge_from(&a), 1, "only the new op");
        assert_eq!(b.merge_from(&a), 0);
        assert!(b.clock().dominates(a.clock()));
    }

    #[test]
    fn ops_survive_the_wire() {
        let mut a = Doc::new(7);
        let mut ops = a.insert(0, "hé");
        ops.push(a.set("k", Some(Json::from(3.5))));
        ops.push(a.set("gone", None));
        ops.push(a.add("x"));
        ops.push(a.remove("x"));
        ops.push(a.count(-4));
        ops.extend(a.delete(0, 1));
        for o in &ops {
            let wire = o.to_json().to_string();
            assert_eq!(&Op::from_json(&Json::parse(&wire).unwrap()).unwrap(), o);
        }
    }

    #[test]
    fn presence_cursors_follow_their_anchor_through_remote_edits() {
        let mut a = Doc::new(1);
        a.insert(0, "hello");
        let anchor = anchor_at(&a.text, 3); // after "hel"
        let mut p = Presence::default();
        p.update(2, 1, "bob", anchor);
        a.insert(0, "oh ");
        assert_eq!(p.positions(&a.text), vec![(2, "bob".to_owned(), 6)]);
        p.update(2, 0, "stale", None);
        assert_eq!(p.positions(&a.text)[0].1, "bob", "older updates ignored");
    }
}
