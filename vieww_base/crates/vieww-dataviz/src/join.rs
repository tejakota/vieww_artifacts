//! Keyed data joins with transitions — D3's `selection.data(d, key)`,
//! `.join(enter, update, exit)` and `.transition()`.
//!
//! The capability matrix's "Data-Driven Visualization" row (§ Core
//! Capability Matrix): *"enters/updates/exits pattern animates changes
//! between data joins"*. [`join`] computes the three sets by key;
//! [`Marks`] keeps a live set of visual marks and, on every new dataset,
//! starts entering marks from an *enter state*, tweens persisting marks
//! from wherever they are now (mid-transition included) to their new
//! state, and fades exiting marks to an *exit state* before removing them.

use std::collections::HashMap;
use std::hash::Hash;

use vieww_foundation::{Color, Offset};

/// The three sets of a join, in input order.
#[derive(Debug, Clone, PartialEq)]
pub struct Join<K, T> {
    pub enter: Vec<(K, T)>,
    /// `(key, old, new)`.
    pub update: Vec<(K, T, T)>,
    pub exit: Vec<(K, T)>,
}

/// Join `new` onto `old` by key.
#[must_use]
pub fn join<K: Eq + Hash + Clone, T: Clone>(old: &[(K, T)], new: &[(K, T)]) -> Join<K, T> {
    let old_map: HashMap<&K, &T> = old.iter().map(|(k, v)| (k, v)).collect();
    let new_keys: std::collections::HashSet<&K> = new.iter().map(|(k, _)| k).collect();
    let mut j = Join {
        enter: Vec::new(),
        update: Vec::new(),
        exit: Vec::new(),
    };
    for (k, v) in new {
        match old_map.get(k) {
            Some(o) => j.update.push((k.clone(), (*o).clone(), v.clone())),
            None => j.enter.push((k.clone(), v.clone())),
        }
    }
    for (k, v) in old {
        if !new_keys.contains(k) {
            j.exit.push((k.clone(), v.clone()));
        }
    }
    j
}

/// Something a transition can interpolate.
pub trait Mix: Clone {
    #[must_use]
    fn mix(&self, other: &Self, t: f32) -> Self;
}

impl Mix for f32 {
    fn mix(&self, o: &Self, t: f32) -> Self {
        self + (o - self) * t
    }
}
impl Mix for Offset {
    fn mix(&self, o: &Self, t: f32) -> Self {
        Offset::new(self.dx + (o.dx - self.dx) * t, self.dy + (o.dy - self.dy) * t)
    }
}
impl Mix for Color {
    fn mix(&self, o: &Self, t: f32) -> Self {
        self.lerp_oklab(*o, t)
    }
}
impl<A: Mix, B: Mix> Mix for (A, B) {
    fn mix(&self, o: &Self, t: f32) -> Self {
        (self.0.mix(&o.0, t), self.1.mix(&o.1, t))
    }
}
impl<const N: usize> Mix for [f32; N] {
    fn mix(&self, o: &Self, t: f32) -> Self {
        let mut r = *self;
        for (a, b) in r.iter_mut().zip(o) {
            *a += (b - *a) * t;
        }
        r
    }
}

/// Which part of the join a mark is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Entering,
    Updating,
    Exiting,
    Settled,
}

#[derive(Debug, Clone)]
struct Mark<T> {
    from: T,
    to: T,
    age: f32,
    phase: Phase,
    order: usize,
}

/// A keyed set of marks animating between datasets.
pub struct Marks<K, T> {
    marks: HashMap<K, Mark<T>>,
    pub duration: f32,
    /// Seconds between successive marks starting (D3's `.delay((d, i) =>
    /// i * stagger)`).
    pub stagger: f32,
    ease: fn(f32) -> f32,
}

impl<K: std::fmt::Debug, T: std::fmt::Debug> std::fmt::Debug for Marks<K, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Marks").field("len", &self.marks.len()).field("duration", &self.duration).finish_non_exhaustive()
    }
}

fn cubic_in_out(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

impl<K: Eq + Hash + Clone + Ord, T: Mix> Marks<K, T> {
    /// Transitions of `duration` seconds with D3's default cubic ease.
    #[must_use]
    pub fn new(duration: f32) -> Self {
        Self {
            marks: HashMap::new(),
            duration,
            stagger: 0.0,
            ease: cubic_in_out,
        }
    }

    /// Bind a new dataset. Entering marks start from `enter(datum)`;
    /// exiting marks head to `exit(current)`.
    pub fn data(&mut self, data: &[(K, T)], enter: impl Fn(&T) -> T, exit: impl Fn(&T) -> T) {
        let current: Vec<(K, T)> = self
            .marks
            .iter()
            .filter(|(_, m)| m.phase != Phase::Exiting)
            .map(|(k, m)| (k.clone(), self.value(m)))
            .collect();
        let j = join(&current, data);
        for (i, (k, v)) in data.iter().enumerate() {
            let _ = v;
            if let Some(m) = self.marks.get_mut(k) {
                m.order = i;
            }
        }
        for (i, (k, v)) in j.enter.into_iter().enumerate() {
            let order = data.iter().position(|(dk, _)| *dk == k).unwrap_or(i);
            // A key coming back while exiting re-enters from where it is.
            let from = match self.marks.get(&k) {
                Some(m) => self.value(m),
                None => enter(&v),
            };
            self.marks.insert(k, Mark { from, to: v, age: 0.0, phase: Phase::Entering, order });
        }
        for (k, _, new) in j.update {
            let now = self.marks.get(&k).map(|m| self.value(m));
            if let (Some(m), Some(now)) = (self.marks.get_mut(&k), now) {
                m.from = now;
                m.to = new;
                m.age = 0.0;
                m.phase = Phase::Updating;
            }
        }
        for (k, v) in j.exit {
            if let Some(m) = self.marks.get_mut(&k) {
                m.from = v.clone();
                m.to = exit(&v);
                m.age = 0.0;
                m.phase = Phase::Exiting;
            }
        }
    }

    fn progress(&self, m: &Mark<T>) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let delay = self.stagger * m.order as f32;
        if self.duration <= 0.0 {
            return 1.0;
        }
        ((m.age - delay) / self.duration).clamp(0.0, 1.0)
    }

    fn value(&self, m: &Mark<T>) -> T {
        m.from.mix(&m.to, (self.ease)(self.progress(m)))
    }

    /// Advance the clock; finished exits are removed.
    pub fn advance(&mut self, dt: f32) {
        for m in self.marks.values_mut() {
            m.age += dt;
        }
        let done: Vec<K> = self
            .marks
            .iter()
            .filter(|(_, m)| self.progress(m) >= 1.0)
            .map(|(k, _)| k.clone())
            .collect();
        for k in done {
            if self.marks.get(&k).is_some_and(|m| m.phase == Phase::Exiting) {
                self.marks.remove(&k);
            } else if let Some(m) = self.marks.get_mut(&k) {
                m.phase = Phase::Settled;
            }
        }
    }

    /// Every live mark `(key, value now, phase)`, sorted by key.
    #[must_use]
    pub fn current(&self) -> Vec<(K, T, Phase)> {
        let mut v: Vec<(K, T, Phase)> = self.marks.iter().map(|(k, m)| (k.clone(), self.value(m), m.phase)).collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.marks.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.marks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_splits_by_key() {
        let old = vec![("a", 1.0f32), ("b", 2.0)];
        let new = vec![("b", 5.0f32), ("c", 3.0)];
        let j = join(&old, &new);
        assert_eq!(j.enter, vec![("c", 3.0)]);
        assert_eq!(j.update, vec![("b", 2.0, 5.0)]);
        assert_eq!(j.exit, vec![("a", 1.0)]);
    }

    #[test]
    fn marks_enter_update_and_exit_over_time() {
        let mut m: Marks<&str, f32> = Marks::new(1.0);
        m.data(&[("a", 10.0), ("b", 20.0)], |_| 0.0, |_| 0.0);
        m.advance(0.5);
        let mid = m.current();
        assert_eq!(mid[0].2, Phase::Entering);
        assert!((mid[0].1 - 5.0).abs() < 1e-4, "halfway in (eased 0.5 = 0.5)");
        m.advance(0.6);
        assert!(m.current().iter().all(|x| x.2 == Phase::Settled));
        m.data(&[("b", 40.0), ("c", 7.0)], |_| 0.0, |_| 0.0);
        m.advance(0.5);
        let now = m.current();
        assert_eq!(now.len(), 3, "a is still fading out");
        assert_eq!(now[0].2, Phase::Exiting);
        assert!((now[1].1 - 30.0).abs() < 1e-3, "b halfway 20 → 40");
        m.advance(0.6);
        assert_eq!(m.len(), 2, "a removed after its exit");
    }

    #[test]
    fn interrupted_updates_continue_from_where_they_are() {
        let mut m: Marks<u32, f32> = Marks::new(1.0);
        m.data(&[(1, 0.0)], |v| *v, |v| *v);
        m.advance(2.0);
        m.data(&[(1, 100.0)], |v| *v, |v| *v);
        m.advance(0.5);
        let mid = m.current()[0].1;
        m.data(&[(1, 0.0)], |v| *v, |v| *v);
        assert!((m.current()[0].1 - mid).abs() < 1e-4, "no jump on interruption");
    }

    #[test]
    fn stagger_delays_by_order() {
        let mut m: Marks<u32, f32> = Marks::new(1.0);
        m.stagger = 0.5;
        m.data(&[(1, 10.0), (2, 10.0)], |_| 0.0, |_| 0.0);
        m.advance(0.5);
        let c = m.current();
        assert!(c[0].1 > 0.0 && c[1].1 == 0.0);
    }
}
