//! Snapshots — Jetpack Compose's `Snapshot` system (§2.11 L5), a
//! transaction over signals.
//!
//! A [`Snapshot`] buffers writes: inside it, [`Snapshot::read`] sees the
//! snapshot's own writes and, for everything else, the value as of the
//! first time the snapshot touched that signal; outside it, nobody sees
//! anything until [`Snapshot::apply`] commits **all** writes at once. If
//! any signal the snapshot read or wrote was written by someone else in the
//! meantime, `apply` refuses with a [`Conflict`] and commits nothing —
//! optimistic concurrency, which is what lets Compose run composition off
//! the main thread and throw the result away when it raced. The rebuilds a
//! commit causes coalesce into one frame like any other writes.
//!
//! Honest scope: isolation begins at a signal's *first touch* inside the
//! snapshot, not at the snapshot's creation (vieww's signals keep one
//! version of their value, not a history); the conflict check makes that
//! safe, because a write that slipped in before first touch is detected at
//! `apply` only if it happened after first touch — a write before it is
//! simply part of the state the snapshot started from.

use std::any::Any;
use std::collections::HashMap;
use std::rc::Rc;

use crate::{Runtime, Signal, SignalId};

/// A refused commit: these signals changed underneath the snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict(pub Vec<SignalId>);

impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} signal(s) changed during the snapshot", self.0.len())
    }
}

impl std::error::Error for Conflict {}

type Commit = Box<dyn FnOnce()>;

/// See the [module docs](self).
pub struct Snapshot {
    runtime: Runtime,
    /// signal → version at first touch.
    base: HashMap<SignalId, u64>,
    /// signal → buffered value (`Rc<dyn Any>` holding a `T`).
    values: HashMap<SignalId, Rc<dyn Any>>,
    /// signal → the deferred write, in first-write order.
    commits: Vec<(SignalId, Commit)>,
}

impl std::fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Snapshot")
            .field("touched", &self.base.len())
            .field("writes", &self.commits.len())
            .finish()
    }
}

impl Snapshot {
    /// Begin a snapshot over `runtime`'s signals.
    #[must_use]
    pub fn new(runtime: &Runtime) -> Self {
        Self {
            runtime: runtime.clone(),
            base: HashMap::new(),
            values: HashMap::new(),
            commits: Vec::new(),
        }
    }

    fn touch<T: Clone + 'static>(&mut self, s: &Signal<T>) {
        let id = s.id();
        if !self.base.contains_key(&id) {
            self.base.insert(id, self.runtime.version(id));
            self.values.insert(id, Rc::new(s.peek()));
        }
    }

    /// The snapshot's view of `s` (never subscribes).
    pub fn read<T: Clone + 'static>(&mut self, s: &Signal<T>) -> T {
        self.touch(s);
        self.values[&s.id()].downcast_ref::<T>().cloned().unwrap_or_else(|| s.peek())
    }

    /// Buffer a write, visible to this snapshot only until `apply`.
    pub fn write<T: Clone + 'static>(&mut self, s: &Signal<T>, value: T) {
        self.touch(s);
        let id = s.id();
        let latest: Rc<dyn Any> = Rc::new(value);
        self.values.insert(id, latest.clone());
        let sig = s.clone();
        let commit: Commit = Box::new(move || {
            if let Some(v) = latest.downcast_ref::<T>() {
                sig.set(v.clone());
            }
        });
        match self.commits.iter_mut().find(|(k, _)| *k == id) {
            Some(slot) => slot.1 = commit,
            None => self.commits.push((id, commit)),
        }
    }

    /// How many signals this snapshot has written.
    #[must_use]
    pub fn writes(&self) -> usize {
        self.commits.len()
    }

    /// Commit every write atomically, or nothing on a conflict.
    ///
    /// # Errors
    /// [`Conflict`] naming the signals written by someone else since this
    /// snapshot first touched them.
    pub fn apply(self) -> Result<usize, Conflict> {
        let mut bad: Vec<SignalId> = self
            .base
            .iter()
            .filter(|(id, v)| self.runtime.version(**id) != **v)
            .map(|(id, _)| *id)
            .collect();
        if !bad.is_empty() {
            bad.sort();
            return Err(Conflict(bad));
        }
        let n = self.commits.len();
        for (_, c) in self.commits {
            c();
        }
        Ok(n)
    }

    /// Drop every buffered write.
    pub fn discard(self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_are_invisible_until_applied_then_all_at_once() {
        let rt = Runtime::new();
        let (a, b) = (rt.signal(1), rt.signal(String::from("x")));
        let mut s = Snapshot::new(&rt);
        s.write(&a, 10);
        s.write(&b, "y".to_owned());
        s.write(&a, 11);
        assert_eq!(s.read(&a), 11, "the snapshot sees its own latest write");
        assert_eq!(a.peek(), 1, "nobody else does");
        assert_eq!(s.apply(), Ok(2));
        assert_eq!((a.peek(), b.peek()), (11, "y".to_owned()));
    }

    #[test]
    fn a_concurrent_write_makes_apply_refuse_and_commit_nothing() {
        let rt = Runtime::new();
        let (a, b) = (rt.signal(0), rt.signal(0));
        let mut s = Snapshot::new(&rt);
        let seen = s.read(&a);
        s.write(&b, seen + 1);
        a.set(5); // someone else, meanwhile
        let err = s.apply().unwrap_err();
        assert_eq!(err.0, vec![a.id()]);
        assert_eq!(b.peek(), 0, "nothing was committed");
    }

    #[test]
    fn reads_are_isolated_after_first_touch_and_discard_drops_writes() {
        let rt = Runtime::new();
        let a = rt.signal(1);
        let mut s = Snapshot::new(&rt);
        assert_eq!(s.read(&a), 1);
        a.set(2);
        assert_eq!(s.read(&a), 1, "isolated from the later outside write");
        let mut t = Snapshot::new(&rt);
        t.write(&a, 99);
        t.discard();
        assert_eq!(a.peek(), 2);
        assert_eq!(rt.version(a.id()), 1);
    }
}
