//! Undo and redo in a shared document — Figma's multiplayer undo, Yjs's
//! `UndoManager`.
//!
//! In a CRDT, undo cannot rewind history (other people's edits are in it).
//! It is a **new edit that inverts one of mine**, broadcast like any other:
//!
//! | my edit | its inverse |
//! |---|---|
//! | set `k` | set `k` back to the value it had before |
//! | add `e` to the set | remove exactly the tag my add created — a concurrent add of `e` by someone else survives |
//! | remove `e` | add `e` |
//! | counter `+d` | counter `−d` |
//! | insert characters | delete exactly those characters (by id) |
//! | delete characters | re-insert each one immediately after its own tombstone, so it lands where it was |
//!
//! Edits made through an [`UndoManager`] are grouped into steps
//! ([`UndoManager::checkpoint`] closes one); [`UndoManager::undo`] applies a
//! step's inverse to the local document and returns the ops to send;
//! [`UndoManager::redo`] inverts the undo. Remote edits are never undone.

use vieww_foundation::json::Json;

use crate::{Doc, Op, Stamp};

#[derive(Debug, Clone, PartialEq)]
enum Inverse {
    Set { key: String, value: Option<Json> },
    RemoveTag { element: String, tag: Stamp },
    Add { element: String },
    Count(i64),
    DeleteIds(Vec<Stamp>),
    Reinsert(Vec<(Stamp, char)>),
}

/// Records my edits and plays their inverses.
#[derive(Debug, Clone, Default)]
pub struct UndoManager {
    open: Vec<Inverse>,
    undo: Vec<Vec<Inverse>>,
    redo: Vec<Vec<Inverse>>,
    /// A character re-inserted by an undo has a new id; older steps that
    /// name the old id follow this map to the living one.
    remap: std::collections::HashMap<Stamp, Stamp>,
}

impl UndoManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn push(&mut self, inv: Inverse) {
        self.open.push(inv);
        self.redo.clear();
    }

    /// Close the current step.
    pub fn checkpoint(&mut self) {
        if !self.open.is_empty() {
            self.undo.push(std::mem::take(&mut self.open));
        }
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.open.is_empty() || !self.undo.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn set(&mut self, doc: &mut Doc, key: &str, value: Option<Json>) -> Op {
        let prev = doc.map.get(key).cloned();
        self.push(Inverse::Set { key: key.to_owned(), value: prev });
        doc.set(key, value)
    }

    pub fn add(&mut self, doc: &mut Doc, element: &str) -> Op {
        let op = doc.add(element);
        self.push(Inverse::RemoveTag { element: element.to_owned(), tag: op.stamp() });
        op
    }

    pub fn remove(&mut self, doc: &mut Doc, element: &str) -> Op {
        self.push(Inverse::Add { element: element.to_owned() });
        doc.remove(element)
    }

    pub fn count(&mut self, doc: &mut Doc, delta: i64) -> Op {
        self.push(Inverse::Count(-delta));
        doc.count(delta)
    }

    pub fn insert(&mut self, doc: &mut Doc, pos: usize, s: &str) -> Vec<Op> {
        let ops = doc.insert(pos, s);
        self.push(Inverse::DeleteIds(ops.iter().map(Op::stamp).collect()));
        ops
    }

    pub fn delete(&mut self, doc: &mut Doc, pos: usize, len: usize) -> Vec<Op> {
        let removed: Vec<(Stamp, char)> = (pos..pos + len)
            .filter_map(|i| doc.text.visible_id(i))
            .filter_map(|id| doc.text.elems.iter().find(|e| e.id == id).map(|e| (id, e.ch)))
            .collect();
        self.push(Inverse::Reinsert(removed));
        doc.delete(pos, len)
    }

    /// Apply `inv` to `doc`; return the ops and the inverse of the inverse.
    fn apply(&mut self, doc: &mut Doc, inv: &Inverse) -> (Vec<Op>, Inverse) {
        match inv {
            Inverse::Set { key, value } => {
                let prev = doc.map.get(key).cloned();
                (vec![doc.set(key, value.clone())], Inverse::Set { key: key.clone(), value: prev })
            }
            Inverse::RemoveTag { element, tag } => {
                let stamp = doc.tick();
                let op = Op::SetRemove { stamp, element: element.clone(), observed: vec![*tag] };
                doc.apply(&op);
                (vec![op], Inverse::Add { element: element.clone() })
            }
            Inverse::Add { element } => {
                let op = doc.add(element);
                (vec![op.clone()], Inverse::RemoveTag { element: element.clone(), tag: op.stamp() })
            }
            Inverse::Count(d) => (vec![doc.count(*d)], Inverse::Count(-d)),
            Inverse::DeleteIds(ids) => {
                let mut ops = Vec::new();
                let mut back = Vec::new();
                for id in ids {
                    let mut id = *id;
                    while let Some(n) = self.remap.get(&id) {
                        id = *n;
                    }
                    let id = &id;
                    let Some(e) = doc.text.elems.iter().find(|e| e.id == *id && !e.deleted) else {
                        continue; // someone else already deleted it
                    };
                    back.push((*id, e.ch));
                    let stamp = doc.tick();
                    let op = Op::Delete { stamp, target: *id };
                    doc.apply(&op);
                    ops.push(op);
                }
                (ops, Inverse::Reinsert(back))
            }
            Inverse::Reinsert(chars) => {
                let mut ops = Vec::new();
                let mut ids = Vec::new();
                // Each character goes immediately after its own tombstone.
                // Its new id is newer than everything already there, so RGA
                // places it directly after the anchor: exactly where it was.
                for (old, ch) in chars {
                    let stamp = doc.tick();
                    let op = Op::Insert { stamp, after: Some(*old), ch: *ch };
                    doc.apply(&op);
                    self.remap.insert(*old, stamp);
                    ids.push(stamp);
                    ops.push(op);
                }
                (ops, Inverse::DeleteIds(ids))
            }
        }
    }

    /// Undo the last step; returns the ops to broadcast.
    pub fn undo(&mut self, doc: &mut Doc) -> Vec<Op> {
        self.checkpoint();
        let Some(step) = self.undo.pop() else {
            return Vec::new();
        };
        let mut ops = Vec::new();
        let mut redo = Vec::new();
        for inv in step.iter().rev() {
            let (o, back) = self.apply(doc, inv);
            ops.extend(o);
            redo.push(back);
        }
        redo.reverse();
        self.redo.push(redo);
        ops
    }

    /// Redo the last undone step; returns the ops to broadcast.
    pub fn redo(&mut self, doc: &mut Doc) -> Vec<Op> {
        let Some(step) = self.redo.pop() else {
            return Vec::new();
        };
        let mut ops = Vec::new();
        let mut undo = Vec::new();
        for inv in step.iter().rev() {
            let (o, back) = self.apply(doc, inv);
            ops.extend(o);
            undo.push(back);
        }
        undo.reverse();
        self.undo.push(undo);
        ops
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_undo_redo_round_trips() {
        let mut d = Doc::new(1);
        let mut u = UndoManager::new();
        u.insert(&mut d, 0, "hello world");
        u.checkpoint();
        u.delete(&mut d, 5, 6);
        u.checkpoint();
        assert_eq!(d.text.value(), "hello");
        u.undo(&mut d);
        assert_eq!(d.text.value(), "hello world", "deleted text returns in place");
        u.undo(&mut d);
        assert_eq!(d.text.value(), "");
        u.redo(&mut d);
        assert_eq!(d.text.value(), "hello world");
        u.redo(&mut d);
        assert_eq!(d.text.value(), "hello");
        assert!(!u.can_redo());
    }

    #[test]
    fn undo_is_selective_with_a_collaborator() {
        let (mut a, mut b) = (Doc::new(1), Doc::new(2));
        let mut ua = UndoManager::new();
        ua.insert(&mut a, 0, "abc");
        ua.checkpoint();
        b.merge_from(&a);
        b.insert(3, "XYZ"); // Bob types after Alice
        a.merge_from(&b);
        let ops = ua.undo(&mut a);
        for o in &ops {
            b.apply(o);
        }
        assert_eq!(a.text.value(), "XYZ", "only Alice's text is removed");
        assert_eq!(a.text.value(), b.text.value(), "and both replicas converge");
    }

    #[test]
    fn maps_sets_and_counters_invert() {
        let mut d = Doc::new(1);
        let mut u = UndoManager::new();
        u.set(&mut d, "title", Some(Json::from("v1")));
        u.checkpoint();
        u.set(&mut d, "title", Some(Json::from("v2")));
        u.add(&mut d, "tag");
        u.count(&mut d, 5);
        u.checkpoint();
        u.undo(&mut d);
        assert_eq!(d.map.get("title"), Some(&Json::from("v1")));
        assert!(!d.set.contains("tag"));
        assert_eq!(d.counter.value(), 0);
        u.redo(&mut d);
        assert_eq!(d.map.get("title"), Some(&Json::from("v2")));
        assert!(d.set.contains("tag"));
        assert_eq!(d.counter.value(), 5);
    }

    #[test]
    fn undoing_my_add_keeps_a_concurrent_add() {
        let (mut a, mut b) = (Doc::new(1), Doc::new(2));
        let mut ua = UndoManager::new();
        ua.add(&mut a, "x");
        b.add("x");
        a.merge_from(&b);
        ua.undo(&mut a);
        assert!(a.set.contains("x"), "Bob's add of x survives Alice's undo");
    }
}
