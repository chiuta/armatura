// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
//! Real-time multi-user editing: a convergent replica of the project JSON tree (R-111).
//!
//! No networking and no clock in here: everything is a pure function of the envelopes a replica has
//! seen, so it can be tested with random delivery orders. Own design from textbook ideas, no code
//! from any other project.
//!
//! Sources of the techniques (all published, none proprietary):
//! * L. Lamport, "Time, Clocks, and the Ordering of Events in a Distributed System", CACM 21(7), 1978:
//!   logical clocks and the happened-before order (used for the `lamport` stamp of every envelope).
//! * C. Fidge 1988 / F. Mattern 1989: vector clocks (`VClock`, causal delivery, duplicate detection).
//! * M. Shapiro, N. Preguica, C. Baquero, M. Zawirski, "Conflict-free Replicated Data Types", SSS 2011
//!   (and INRIA RR-7687, "A comprehensive study of Convergent and Commutative Replicated Data Types"):
//!   last-writer-wins register, tombstones, operation-based CRDT with causal delivery.
//! * H.-G. Roh et al., "Replicated abstract data types: building blocks for collaborative
//!   applications", J. Parallel Distrib. Comput. 71(3), 2011: the replicated growable array (RGA) rule
//!   for placing a concurrent insert into a sequence.
//!
//! # Data model
//! The project JSON is a tree of objects, arrays and scalars. Every object and array gets a stable
//! identity (`Id`), every array element too. An edit is expressed on identities, never on indices:
//! * object key  = LWW register (`Reg`): the write with the larger `Stamp (lamport, replica, index)` wins.
//!   A key delete is a write of `Absent` (delete and set are ordered by stamp, neither "wins").
//! * array       = RGA sequence of elements with tombstones; an insert names the element it goes
//!   after (`None` = head), concurrent inserts after the same element are ordered by `(lamport, id)`
//!   descending; a remove only sets a tombstone, so it REMOVES WINS over any concurrent edit
//!   inside the element (the edit is applied to the hidden element and is invisible), and an
//!   element is never resurrected (an undo of a remove inserts a fresh copy).
//! * array element value = LWW register.
//!
//! A local edit (the existing `Op::{Set, Insert, Remove, Batch}` on RFC 6901 pointers) is translated
//! against the replica's current tree: the pointer's indices become element identities (so a
//! stale index from a user's view is "transformed" by construction), and a `Set` of a whole
//! object/array is turned into a minimal structural diff (per key; arrays by longest common
//! subsequence) so two users editing different keys of the same channel both survive. If the
//! old and new values are unrelated (different scalars, different type, objects with different
//! `id`/`name`/`target`/`prop`) the register is replaced as a whole (LWW).
//!
//! # Delivery
//! `Envelope { replica, seq, lamport, deps, ops }`: `seq` counts that replica's envelopes from 1,
//! `deps` is the sender's vector clock when it created the envelope. `Replica::receive` holds back an
//! envelope until its predecessors from the same replica and all `deps` are applied (causal
//! delivery), drops duplicates by `seq` (idempotent), and applies the rest. Given causal delivery, every
//! replica that has applied the same set of envelopes holds a byte-identical `Doc` (tested with
//! seeded random delivery orders).
//!
//! # Invalid concurrent edits
//! An op that names an unknown identity or the wrong container kind is dropped on application (counted
//! in `dropped`), never a panic. Edits to a removed element or to a node that was replaced by an
//! LWW overwrite are applied to the (now invisible) node, so all replicas agree and the edit simply does
//! not show. Whether the merged JSON is a valid ARMATURA project is NOT part of this module (a
//! server decides; see `collabhub`).
//!
//! # Per-user undo
//! `Replica::undo` reverses only this replica's own most recent edit, as new envelopes: an insert is
//! undone by removing that element (if still there), a remove by inserting a copy next to its old left
//! neighbour, a register write by writing the previous value back, but only if the register still holds
//! this edit's write (if someone wrote after, theirs stays: the undo of that part is skipped).
//!
//! # Compaction
//! `Replica::compact` drops log entries every peer has, and, only when all peers have exactly this
//! replica's clock (quiescent, nothing in flight), purges tombstones and unreachable nodes.

use crate::edit::Op;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub type ReplicaId = u32;
/// Vector clock: highest envelope `seq` applied per replica (missing = 0).
pub type VClock = BTreeMap<ReplicaId, u64>;

const MAX_DEPTH: usize = 400;
const MAX_PENDING: usize = 20_000;
const MAX_PRIMS: usize = 200_000;
const MAX_UNDO: usize = 2_000;

// ---------------------------------------------------------------------------------------------
// identities and stamps

/// Identity of a node or array element: created by envelope `(r, s)` as its `i`-th allocation.
/// `r == 0` is the shared base document (every replica builds it identically).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Id {
    pub r: u32,
    pub s: u64,
    pub i: u32,
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.r, self.s, self.i)
    }
}

impl Id {
    pub fn parse(s: &str) -> Option<Id> {
        let mut it = s.split('.');
        let r = it.next()?.parse().ok()?;
        let sq = it.next()?.parse().ok()?;
        let i = it.next()?.parse().ok()?;
        if it.next().is_some() {
            return None;
        }
        Some(Id { r, s: sq, i })
    }
}

impl Serialize for Id {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Id, D::Error> {
        let s = String::deserialize(d)?;
        Id::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("bad id '{s}'")))
    }
}

/// Total order of writes to a register: Lamport time, then replica, then position inside the envelope.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub struct Stamp {
    pub lamport: u64,
    pub replica: u32,
    pub idx: u32,
}

impl Stamp {
    const ZERO: Stamp = Stamp { lamport: 0, replica: 0, idx: 0 };
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum Val {
    Absent,
    Scalar(Value),
    Ref(Id),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Reg {
    stamp: Stamp,
    val: Val,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Elem {
    id: Id,
    /// Lamport time of the insert (RGA ordering key together with `id`).
    ins: u64,
    reg: Reg,
    /// Envelope `(replica, seq)` of the (smallest) remove, if removed.
    tomb: Option<(u32, u64)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum Cont {
    Obj(BTreeMap<String, Reg>),
    Arr(Vec<Elem>),
}

/// Where a value lives.
#[derive(Clone, Debug, PartialEq)]
enum Slot {
    Root,
    Key(Id, String),
    Elem(Id, Id),
}

// ---------------------------------------------------------------------------------------------
// primitive operations (what travels in an envelope)

/// An identity-addressed edit. `value` payloads are plain JSON; the containers inside get fresh
/// identities derived from the envelope, identically on every replica.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Prim {
    SetRoot { value: Value },
    SetKey { obj: Id, key: String, value: Value },
    DelKey { obj: Id, key: String },
    SetElem { arr: Id, elem: Id, value: Value },
    InsElem { arr: Id, after: Option<Id>, value: Value },
    DelElem { arr: Id, elem: Id },
}

/// The unit that travels between replicas.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub replica: ReplicaId,
    pub seq: u64,
    pub lamport: u64,
    pub deps: VClock,
    pub ops: Vec<Prim>,
}

#[derive(Clone, Copy)]
struct Meta {
    replica: u32,
    seq: u64,
    lamport: u64,
}

struct IdGen {
    r: u32,
    s: u64,
    next: u32,
}

impl IdGen {
    fn peek(&self) -> Id {
        Id { r: self.r, s: self.s, i: self.next }
    }
    fn take(&mut self) -> Id {
        let id = self.peek();
        self.next += 1;
        id
    }
}

/// How to reverse one primitive (kept locally, never sent).
#[derive(Clone, Debug)]
enum Undo {
    Restore { slot: Slot, stamp: Stamp, prev: Option<Value> },
    Remove { arr: Id, elem: Id },
    Reinsert { arr: Id, elem: Id, left: Option<Id>, value: Value },
}

// ---------------------------------------------------------------------------------------------
// the replicated tree

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Doc {
    nodes: BTreeMap<Id, Cont>,
    root: Reg,
}

impl Doc {
    pub fn from_json(v: &Value) -> Doc {
        let mut d = Doc { nodes: BTreeMap::new(), root: Reg { stamp: Stamp::ZERO, val: Val::Absent } };
        let mut gen = IdGen { r: 0, s: 0, next: 0 };
        let val = d.build(v, &mut gen, Stamp::ZERO, 0, 0);
        d.root.val = val;
        d
    }

    fn build(&mut self, v: &Value, gen: &mut IdGen, stamp: Stamp, ins: u64, depth: usize) -> Val {
        if depth > MAX_DEPTH {
            return Val::Scalar(Value::Null);
        }
        match v {
            Value::Object(m) => {
                let id = gen.take();
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                let mut ent = BTreeMap::new();
                for k in keys {
                    let val = self.build(&m[k], gen, stamp, ins, depth + 1);
                    ent.insert(k.clone(), Reg { stamp, val });
                }
                self.nodes.insert(id, Cont::Obj(ent));
                Val::Ref(id)
            }
            Value::Array(a) => {
                let id = gen.take();
                let mut elems = Vec::with_capacity(a.len());
                for x in a {
                    let eid = gen.take();
                    let val = self.build(x, gen, stamp, ins, depth + 1);
                    elems.push(Elem { id: eid, ins, reg: Reg { stamp, val }, tomb: None });
                }
                self.nodes.insert(id, Cont::Arr(elems));
                Val::Ref(id)
            }
            other => Val::Scalar(other.clone()),
        }
    }

    fn val_json(&self, v: &Val, depth: usize) -> Option<Value> {
        if depth > MAX_DEPTH {
            return Some(Value::Null);
        }
        match v {
            Val::Absent => None,
            Val::Scalar(s) => Some(s.clone()),
            Val::Ref(id) => match self.nodes.get(id) {
                Some(Cont::Obj(m)) => {
                    let mut out = serde_json::Map::new();
                    for (k, r) in m {
                        if let Some(x) = self.val_json(&r.val, depth + 1) {
                            out.insert(k.clone(), x);
                        }
                    }
                    Some(Value::Object(out))
                }
                Some(Cont::Arr(es)) => Some(Value::Array(es.iter().filter(|e| e.tomb.is_none()).filter_map(|e| self.val_json(&e.reg.val, depth + 1)).collect())),
                None => Some(Value::Null),
            },
        }
    }

    /// The visible JSON document.
    pub fn to_json(&self) -> Value {
        self.val_json(&self.root.val, 0).unwrap_or(Value::Null)
    }

    fn arr(&self, id: Id) -> Option<&Vec<Elem>> {
        match self.nodes.get(&id) {
            Some(Cont::Arr(v)) => Some(v),
            _ => None,
        }
    }
    fn obj(&self, id: Id) -> Option<&BTreeMap<String, Reg>> {
        match self.nodes.get(&id) {
            Some(Cont::Obj(m)) => Some(m),
            _ => None,
        }
    }

    fn visible(&self, arr: Id) -> Vec<&Elem> {
        self.arr(arr).map(|v| v.iter().filter(|e| e.tomb.is_none()).collect()).unwrap_or_default()
    }

    fn slot_val(&self, slot: &Slot) -> Result<Val, String> {
        match slot {
            Slot::Root => Ok(self.root.val.clone()),
            Slot::Key(o, k) => Ok(self.obj(*o).ok_or("unknown object")?.get(k).map(|r| r.val.clone()).unwrap_or(Val::Absent)),
            Slot::Elem(a, e) => Ok(self.arr(*a).ok_or("unknown array")?.iter().find(|x| x.id == *e).ok_or("unknown element")?.reg.val.clone()),
        }
    }

    fn slot_json(&self, slot: &Slot) -> Option<Value> {
        self.slot_val(slot).ok().and_then(|v| self.val_json(&v, 0))
    }

    fn reg_stamp(&self, slot: &Slot) -> Option<Stamp> {
        match slot {
            Slot::Root => Some(self.root.stamp),
            Slot::Key(o, k) => self.obj(*o).and_then(|m| m.get(k)).map(|r| r.stamp),
            Slot::Elem(a, e) => self.arr(*a).and_then(|v| v.iter().find(|x| x.id == *e)).map(|x| x.reg.stamp),
        }
    }

    fn reg_mut(&mut self, slot: &Slot) -> Result<&mut Reg, String> {
        match slot {
            Slot::Root => Ok(&mut self.root),
            Slot::Key(o, k) => match self.nodes.get_mut(o) {
                Some(Cont::Obj(m)) => Ok(m.entry(k.clone()).or_insert(Reg { stamp: Stamp::ZERO, val: Val::Absent })),
                _ => Err("unknown object".into()),
            },
            Slot::Elem(a, e) => match self.nodes.get_mut(a) {
                Some(Cont::Arr(v)) => v.iter_mut().find(|x| x.id == *e).map(|x| &mut x.reg).ok_or_else(|| "unknown element".to_string()),
                _ => Err("unknown array".into()),
            },
        }
    }

    fn check_slot(&self, slot: &Slot) -> Result<(), String> {
        match slot {
            Slot::Root => Ok(()),
            Slot::Key(o, _) => self.obj(*o).map(|_| ()).ok_or_else(|| "unknown object".into()),
            Slot::Elem(a, e) => {
                let v = self.arr(*a).ok_or("unknown array")?;
                if v.iter().any(|x| x.id == *e) {
                    Ok(())
                } else {
                    Err("unknown element".into())
                }
            }
        }
    }

    /// LWW write (`value == None` deletes). Identities inside the value are always allocated, even
    /// when the write loses, so that every replica allocates identically.
    fn write(&mut self, slot: &Slot, stamp: Stamp, value: Option<&Value>, gen: &mut IdGen, want_undo: bool) -> Result<Vec<Undo>, String> {
        self.check_slot(slot)?;
        let prev = if want_undo { Some(self.slot_json(slot)) } else { None };
        let new = match value {
            Some(v) => self.build(v, gen, stamp, stamp.lamport, 0),
            None => Val::Absent,
        };
        let reg = self.reg_mut(slot)?;
        if stamp > reg.stamp {
            reg.stamp = stamp;
            reg.val = new;
            if let Some(p) = prev {
                return Ok(vec![Undo::Restore { slot: slot.clone(), stamp, prev: p }]);
            }
        }
        Ok(vec![])
    }

    fn apply_prim(&mut self, m: &Meta, idx: u32, gen: &mut IdGen, prim: &Prim, want_undo: bool) -> Result<Vec<Undo>, String> {
        let stamp = Stamp { lamport: m.lamport, replica: m.replica, idx };
        match prim {
            Prim::SetRoot { value } => self.write(&Slot::Root, stamp, Some(value), gen, want_undo),
            Prim::SetKey { obj, key, value } => self.write(&Slot::Key(*obj, key.clone()), stamp, Some(value), gen, want_undo),
            Prim::DelKey { obj, key } => self.write(&Slot::Key(*obj, key.clone()), stamp, None, gen, want_undo),
            Prim::SetElem { arr, elem, value } => self.write(&Slot::Elem(*arr, *elem), stamp, Some(value), gen, want_undo),
            Prim::InsElem { arr, after, value } => {
                let v = self.arr(*arr).ok_or("unknown array")?;
                let mut pos = match after {
                    None => 0,
                    Some(a) => v.iter().position(|e| e.id == *a).ok_or("unknown anchor element")? + 1,
                };
                let id = gen.take();
                let val = self.build(value, gen, stamp, stamp.lamport, 0);
                let v = match self.nodes.get_mut(arr) {
                    Some(Cont::Arr(v)) => v,
                    _ => return Err("unknown array".into()),
                };
                // RGA: pass over elements inserted concurrently with a larger (lamport, id)
                while pos < v.len() && (v[pos].ins, v[pos].id) > (stamp.lamport, id) {
                    pos += 1;
                }
                v.insert(pos, Elem { id, ins: stamp.lamport, reg: Reg { stamp, val }, tomb: None });
                Ok(if want_undo { vec![Undo::Remove { arr: *arr, elem: id }] } else { vec![] })
            }
            Prim::DelElem { arr, elem } => {
                let (value, was_tomb, left) = {
                    let v = self.arr(*arr).ok_or("unknown array")?;
                    let p = v.iter().position(|e| e.id == *elem).ok_or("unknown element")?;
                    let e = &v[p];
                    let left = v[..p].iter().rev().find(|x| x.tomb.is_none()).map(|x| x.id);
                    (if want_undo && e.tomb.is_none() { self.val_json(&e.reg.val, 0) } else { None }, e.tomb.is_some(), left)
                };
                if let Some(Cont::Arr(v)) = self.nodes.get_mut(arr) {
                    if let Some(e) = v.iter_mut().find(|e| e.id == *elem) {
                        let t = (m.replica, m.seq);
                        e.tomb = Some(match e.tomb {
                            Some(old) if old <= t => old,
                            _ => t,
                        });
                    }
                }
                Ok(match (want_undo && !was_tomb, value) {
                    (true, Some(value)) => vec![Undo::Reinsert { arr: *arr, elem: *elem, left, value }],
                    _ => vec![],
                })
            }
        }
    }

    fn reachable(&self) -> BTreeSet<Id> {
        let mut seen = BTreeSet::new();
        let mut stack = Vec::new();
        if let Val::Ref(id) = self.root.val {
            stack.push(id);
        }
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            match self.nodes.get(&id) {
                Some(Cont::Obj(m)) => {
                    for r in m.values() {
                        if let Val::Ref(c) = r.val {
                            stack.push(c);
                        }
                    }
                }
                Some(Cont::Arr(v)) => {
                    for e in v {
                        if let Val::Ref(c) = e.reg.val {
                            stack.push(c);
                        }
                    }
                }
                None => {}
            }
        }
        seen
    }
}

// ---------------------------------------------------------------------------------------------
// local translation: pointer ops / whole-document edits -> primitives

struct Tx {
    meta: Meta,
    idx: u32,
    gen: IdGen,
    prims: Vec<Prim>,
    undos: Vec<Undo>,
}

fn emit(doc: &mut Doc, tx: &mut Tx, prim: Prim) -> Result<(), String> {
    if tx.prims.len() >= MAX_PRIMS {
        return Err("edit is too large".into());
    }
    let u = doc.apply_prim(&tx.meta, tx.idx, &mut tx.gen, &prim, true)?;
    tx.idx += 1;
    tx.undos.extend(u);
    tx.prims.push(prim);
    Ok(())
}

fn unescape(tok: &str) -> String {
    tok.replace("~1", "/").replace("~0", "~")
}

fn tokens(path: &str) -> Result<Vec<String>, String> {
    if path.is_empty() {
        return Ok(vec![]);
    }
    if !path.starts_with('/') {
        return Err(format!("bad JSON pointer '{path}'"));
    }
    Ok(path[1..].split('/').map(unescape).collect())
}

fn ptr_of(toks: &[String]) -> String {
    toks.iter().map(|t| format!("/{}", t.replace('~', "~0").replace('/', "~1"))).collect()
}

fn child_of(doc: &Doc, cur: &Val, tok: &str) -> Option<Val> {
    let Val::Ref(id) = cur else { return None };
    match doc.nodes.get(id)? {
        Cont::Obj(m) => m.get(tok).filter(|r| r.val != Val::Absent).map(|r| r.val.clone()),
        Cont::Arr(_) => {
            let i: usize = tok.parse().ok()?;
            doc.visible(*id).get(i).map(|e| e.reg.val.clone())
        }
    }
}

fn walk(doc: &Doc, toks: &[String]) -> Result<Val, String> {
    let mut cur = doc.root.val.clone();
    for (n, t) in toks.iter().enumerate() {
        cur = child_of(doc, &cur, t).ok_or_else(|| format!("no such path '{}'", ptr_of(&toks[..=n])))?;
    }
    Ok(cur)
}

fn slot_prim(slot: &Slot, value: &Value) -> Prim {
    match slot {
        Slot::Root => Prim::SetRoot { value: value.clone() },
        Slot::Key(o, k) => Prim::SetKey { obj: *o, key: k.clone(), value: value.clone() },
        Slot::Elem(a, e) => Prim::SetElem { arr: *a, elem: *e, value: value.clone() },
    }
}

/// Same kind of thing? Objects that carry a different `id` / `name` / `target` / `prop` are different
/// entities: replacing one by the other is a remove plus an insert, not a field-by-field rewrite.
fn same_identity(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => ["id", "name", "target", "prop"].iter().all(|k| match (x.get(*k), y.get(*k)) {
            (Some(p), Some(q)) => p == q,
            _ => true,
        }),
        (Value::Array(_), Value::Array(_)) => true,
        (Value::Object(_), _) | (_, Value::Object(_)) | (Value::Array(_), _) | (_, Value::Array(_)) => false,
        _ => true,
    }
}

fn diff_slot(doc: &mut Doc, tx: &mut Tx, slot: Slot, new: &Value) -> Result<(), String> {
    let cur = doc.slot_val(&slot)?;
    match (&cur, new) {
        (Val::Ref(id), Value::Object(nm)) if doc.obj(*id).is_some() => {
            let id = *id;
            let existing: BTreeSet<String> = doc.obj(id).map(|m| m.iter().filter(|(_, r)| r.val != Val::Absent).map(|(k, _)| k.clone()).collect()).unwrap_or_default();
            for k in &existing {
                if !nm.contains_key(k) {
                    emit(doc, tx, Prim::DelKey { obj: id, key: k.clone() })?;
                }
            }
            let mut keys: Vec<&String> = nm.keys().collect();
            keys.sort();
            for k in keys {
                if existing.contains(k) {
                    diff_slot(doc, tx, Slot::Key(id, k.clone()), &nm[k])?;
                } else {
                    emit(doc, tx, Prim::SetKey { obj: id, key: k.clone(), value: nm[k].clone() })?;
                }
            }
            Ok(())
        }
        (Val::Ref(id), Value::Array(na)) if doc.arr(*id).is_some() => diff_array(doc, tx, *id, na),
        (Val::Scalar(s), v) if s == v => Ok(()),
        _ => emit(doc, tx, slot_prim(&slot, new)),
    }
}

fn diff_array(doc: &mut Doc, tx: &mut Tx, arr: Id, new: &[Value]) -> Result<(), String> {
    let old: Vec<(Id, Value)> = doc.visible(arr).iter().map(|e| (e.id, doc.val_json(&e.reg.val, 0).unwrap_or(Value::Null))).collect();
    let (n, m) = (old.len(), new.len());
    let mut pre = 0;
    while pre < n && pre < m && old[pre].1 == new[pre] {
        pre += 1;
    }
    let mut suf = 0;
    while suf < n - pre && suf < m - pre && old[n - 1 - suf].1 == new[m - 1 - suf] {
        suf += 1;
    }
    let mut matches: Vec<(usize, usize)> = (0..pre).map(|i| (i, i)).collect();
    let (on, nn) = (n - pre - suf, m - pre - suf);
    if on > 0 && nn > 0 && on.saturating_mul(nn) <= 4_000_000 {
        // longest common subsequence of the middle parts
        let w = nn + 1;
        let mut t = vec![0u32; (on + 1) * w];
        for i in (0..on).rev() {
            for j in (0..nn).rev() {
                t[i * w + j] = if old[pre + i].1 == new[pre + j] { t[(i + 1) * w + j + 1] + 1 } else { t[(i + 1) * w + j].max(t[i * w + j + 1]) };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < on && j < nn {
            if old[pre + i].1 == new[pre + j] {
                matches.push((pre + i, pre + j));
                i += 1;
                j += 1;
            } else if t[(i + 1) * w + j] >= t[i * w + j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    for k in 0..suf {
        matches.push((n - suf + k, m - suf + k));
    }
    matches.push((n, m)); // sentinel
    let (mut oi, mut nj) = (0usize, 0usize);
    let mut anchor: Option<Id> = None;
    for (mi, mj) in matches {
        let (lo, ln) = (mi - oi, mj - nj);
        let k = lo.min(ln);
        for t in 0..k {
            let (oid, ov) = (&old[oi + t].0, &old[oi + t].1);
            let nv = &new[nj + t];
            if same_identity(ov, nv) {
                diff_slot(doc, tx, Slot::Elem(arr, *oid), nv)?;
                anchor = Some(*oid);
            } else {
                emit(doc, tx, Prim::DelElem { arr, elem: *oid })?;
                let nid = tx.gen.peek();
                emit(doc, tx, Prim::InsElem { arr, after: anchor, value: nv.clone() })?;
                anchor = Some(nid);
            }
        }
        for t in k..lo {
            emit(doc, tx, Prim::DelElem { arr, elem: old[oi + t].0 })?;
        }
        for t in k..ln {
            let nid = tx.gen.peek();
            emit(doc, tx, Prim::InsElem { arr, after: anchor, value: new[nj + t].clone() })?;
            anchor = Some(nid);
        }
        if mi < n {
            anchor = Some(old[mi].0);
        }
        oi = mi + 1;
        nj = mj + 1;
    }
    Ok(())
}

fn translate(doc: &mut Doc, tx: &mut Tx, op: &Op) -> Result<(), String> {
    match op {
        Op::Batch { ops } => {
            for o in ops {
                translate(doc, tx, o)?;
            }
            Ok(())
        }
        Op::Set { path, value } => {
            let toks = tokens(path)?;
            if toks.is_empty() {
                return diff_slot(doc, tx, Slot::Root, value);
            }
            let (pt, last) = toks.split_at(toks.len() - 1);
            let tok = &last[0];
            let parent = walk(doc, pt).map_err(|_| format!("no such path '{}'", ptr_of(pt)))?;
            let Val::Ref(pid) = parent else { return Err(format!("'{}' is not a container", ptr_of(pt))) };
            match doc.nodes.get(&pid) {
                Some(Cont::Obj(m)) => {
                    if m.get(tok).map(|r| r.val != Val::Absent).unwrap_or(false) {
                        diff_slot(doc, tx, Slot::Key(pid, tok.clone()), value)
                    } else {
                        emit(doc, tx, Prim::SetKey { obj: pid, key: tok.clone(), value: value.clone() })
                    }
                }
                Some(Cont::Arr(_)) => {
                    let i: usize = tok.parse().map_err(|_| format!("bad array index '{tok}'"))?;
                    let id = doc.visible(pid).get(i).map(|e| e.id).ok_or_else(|| format!("index {i} out of range at '{}'", ptr_of(pt)))?;
                    diff_slot(doc, tx, Slot::Elem(pid, id), value)
                }
                None => Err(format!("'{}' is not a container", ptr_of(pt))),
            }
        }
        Op::Insert { path, value } => {
            let toks = tokens(path)?;
            if toks.is_empty() {
                return Err("path must not be empty here".into());
            }
            let (pt, last) = toks.split_at(toks.len() - 1);
            let tok = &last[0];
            let parent = walk(doc, pt).map_err(|_| format!("no such path '{}'", ptr_of(pt)))?;
            let Val::Ref(pid) = parent else { return Err(format!("'{}' is not a container", ptr_of(pt))) };
            match doc.nodes.get(&pid) {
                Some(Cont::Obj(m)) => {
                    if m.get(tok).map(|r| r.val != Val::Absent).unwrap_or(false) {
                        diff_slot(doc, tx, Slot::Key(pid, tok.clone()), value)
                    } else {
                        emit(doc, tx, Prim::SetKey { obj: pid, key: tok.clone(), value: value.clone() })
                    }
                }
                Some(Cont::Arr(_)) => {
                    let vis = doc.visible(pid);
                    let i = if tok == "-" { vis.len() } else { tok.parse().map_err(|_| format!("bad array index '{tok}'"))? };
                    if i > vis.len() {
                        return Err(format!("insert index {i} out of range at '{}'", ptr_of(pt)));
                    }
                    let after = if i == 0 { None } else { Some(vis[i - 1].id) };
                    emit(doc, tx, Prim::InsElem { arr: pid, after, value: value.clone() })
                }
                None => Err(format!("'{}' is not a container", ptr_of(pt))),
            }
        }
        Op::Remove { path } => {
            let toks = tokens(path)?;
            if toks.is_empty() {
                return Err("path must not be empty here".into());
            }
            let (pt, last) = toks.split_at(toks.len() - 1);
            let tok = &last[0];
            let parent = walk(doc, pt).map_err(|_| format!("no such path '{}'", ptr_of(pt)))?;
            let Val::Ref(pid) = parent else { return Err(format!("'{}' is not a container", ptr_of(pt))) };
            match doc.nodes.get(&pid) {
                Some(Cont::Obj(m)) => {
                    if m.get(tok).map(|r| r.val != Val::Absent).unwrap_or(false) {
                        emit(doc, tx, Prim::DelKey { obj: pid, key: tok.clone() })
                    } else {
                        Err(format!("no key '{tok}' at '{}'", ptr_of(pt)))
                    }
                }
                Some(Cont::Arr(_)) => {
                    let i: usize = tok.parse().map_err(|_| format!("bad array index '{tok}'"))?;
                    let id = doc.visible(pid).get(i).map(|e| e.id).ok_or_else(|| format!("index {i} out of range at '{}'", ptr_of(pt)))?;
                    emit(doc, tx, Prim::DelElem { arr: pid, elem: id })
                }
                None => Err(format!("'{}' is not a container", ptr_of(pt))),
            }
        }
    }
}

/// Merge records so that undoing a multi-step edit restores the original, not an intermediate state.
fn coalesce(undos: Vec<Undo>) -> Vec<Undo> {
    let mut out: Vec<Undo> = Vec::new();
    for u in undos {
        match u {
            Undo::Restore { slot, stamp, prev } => {
                let mut merged = false;
                for x in out.iter_mut() {
                    if let Undo::Restore { slot: s2, stamp: st2, .. } = x {
                        if *s2 == slot {
                            *st2 = stamp;
                            merged = true;
                            break;
                        }
                    }
                }
                if !merged {
                    out.push(Undo::Restore { slot, stamp, prev });
                }
            }
            Undo::Reinsert { arr, elem, left, value } => {
                if let Some(p) = out.iter().position(|x| matches!(x, Undo::Remove { elem: e2, .. } if *e2 == elem)) {
                    out.remove(p);
                } else {
                    out.push(Undo::Reinsert { arr, elem, left, value });
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// Express the inverse of one record against the CURRENT tree (this is the "transform against later ops").
fn plan_inverse(doc: &mut Doc, tx: &mut Tx, u: &Undo) -> Result<(), String> {
    match u {
        Undo::Remove { arr, elem } => {
            let live = doc.arr(*arr).and_then(|v| v.iter().find(|e| e.id == *elem)).map(|e| e.tomb.is_none()).unwrap_or(false);
            if live {
                emit(doc, tx, Prim::DelElem { arr: *arr, elem: *elem })?;
            }
            Ok(())
        }
        Undo::Reinsert { arr, elem, left, value } => {
            let v = doc.arr(*arr).ok_or("array is gone")?;
            let nearest_before = |p: usize| v[..=p].iter().rev().find(|e| e.tomb.is_none()).map(|e| e.id);
            let after = match v.iter().position(|e| e.id == *elem) {
                Some(p) => {
                    if v[p].tomb.is_none() {
                        return Ok(());
                    }
                    nearest_before(p)
                }
                // the tombstone was compacted away: use the old left neighbour if it is still there
                None => match left {
                    None => None,
                    Some(l) => match v.iter().position(|e| e.id == *l) {
                        Some(p) => nearest_before(p),
                        None => v.iter().rev().find(|e| e.tomb.is_none()).map(|e| e.id),
                    },
                },
            };
            emit(doc, tx, Prim::InsElem { arr: *arr, after, value: value.clone() })
        }
        Undo::Restore { slot, stamp, prev } => {
            if doc.reg_stamp(slot) != Some(*stamp) {
                return Ok(()); // somebody wrote after us: their write stays
            }
            if let Slot::Elem(a, e) = slot {
                if doc.arr(*a).and_then(|v| v.iter().find(|x| x.id == *e)).map(|x| x.tomb.is_some()).unwrap_or(true) {
                    return Ok(());
                }
            }
            match (slot, prev) {
                (Slot::Root, Some(v)) => diff_slot(doc, tx, Slot::Root, v),
                (Slot::Root, None) => Ok(()),
                (Slot::Key(o, k), None) => emit(doc, tx, Prim::DelKey { obj: *o, key: k.clone() }),
                (s, Some(v)) => diff_slot(doc, tx, s.clone(), v),
                (Slot::Elem(..), None) => Ok(()),
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// replica

#[derive(Clone, Debug, PartialEq)]
pub enum Delivery {
    /// Newly applied envelopes, in application order (the one received plus any it released).
    Applied(Vec<Envelope>),
    Duplicate,
    /// Held back: a predecessor or a dependency is missing.
    Buffered,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompactStats {
    pub log_dropped: usize,
    pub tombstones_purged: usize,
    pub nodes_collected: usize,
    /// True when tombstones could be purged (every peer had exactly this replica's clock).
    pub purged: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub doc: Doc,
    pub clock: VClock,
    pub lamport: u64,
}

#[derive(Clone)]
pub struct Replica {
    id: ReplicaId,
    lamport: u64,
    applied: VClock,
    doc: Doc,
    pending: Vec<Envelope>,
    log: Vec<Envelope>,
    undo: Vec<Vec<Undo>>,
    redo: Vec<Vec<Undo>>,
    /// Primitives dropped because they named something unknown (should stay 0 with honest peers).
    pub dropped: u64,
}

impl Replica {
    /// A replica holding the shared base document. All replicas must be created from the same base.
    pub fn new(id: ReplicaId, base: &Value) -> Result<Replica, String> {
        if id == 0 {
            return Err("replica id 0 is reserved for the base document".into());
        }
        Ok(Replica { id, lamport: 0, applied: VClock::new(), doc: Doc::from_json(base), pending: vec![], log: vec![], undo: vec![], redo: vec![], dropped: 0 })
    }

    /// Join an existing session from a snapshot (a fresh replica id, or a known one to continue it).
    pub fn from_snapshot(id: ReplicaId, snap: Snapshot) -> Result<Replica, String> {
        if id == 0 {
            return Err("replica id 0 is reserved for the base document".into());
        }
        Ok(Replica { id, lamport: snap.lamport, applied: snap.clock, doc: snap.doc, pending: vec![], log: vec![], undo: vec![], redo: vec![], dropped: 0 })
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot { doc: self.doc.clone(), clock: self.applied.clone(), lamport: self.lamport }
    }

    /// Same replica state seen from another identity (a server-side proxy for one user).
    pub fn fork_as(&self, id: ReplicaId) -> Result<Replica, String> {
        Replica::from_snapshot(id, self.snapshot())
    }

    pub fn id(&self) -> ReplicaId {
        self.id
    }
    pub fn lamport(&self) -> u64 {
        self.lamport
    }
    pub fn clock(&self) -> &VClock {
        &self.applied
    }
    pub fn doc(&self) -> &Doc {
        &self.doc
    }
    pub fn json(&self) -> Value {
        self.doc.to_json()
    }
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
    pub fn log_len(&self) -> usize {
        self.log.len()
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn tombstones(&self) -> usize {
        self.doc.nodes.values().map(|c| if let Cont::Arr(v) = c { v.iter().filter(|e| e.tomb.is_some()).count() } else { 0 }).sum()
    }
    pub fn node_count(&self) -> usize {
        self.doc.nodes.len()
    }

    fn local_tx<F>(&mut self, f: F) -> Result<Option<(Envelope, Vec<Undo>)>, String>
    where
        F: FnOnce(&mut Doc, &mut Tx) -> Result<(), String>,
    {
        let backup = self.doc.clone();
        let seq = self.applied.get(&self.id).copied().unwrap_or(0) + 1;
        let lamport = self.lamport + 1;
        let mut tx = Tx { meta: Meta { replica: self.id, seq, lamport }, idx: 0, gen: IdGen { r: self.id, s: seq, next: 0 }, prims: vec![], undos: vec![] };
        if let Err(e) = f(&mut self.doc, &mut tx) {
            self.doc = backup;
            return Err(e);
        }
        if tx.prims.is_empty() {
            return Ok(None);
        }
        let deps = self.applied.clone();
        self.lamport = lamport;
        self.applied.insert(self.id, seq);
        let env = Envelope { replica: self.id, seq, lamport, deps, ops: tx.prims };
        self.log.push(env.clone());
        Ok(Some((env, coalesce(tx.undos))))
    }

    /// Apply a local edit given as the editor's pointer ops. Returns the envelope to broadcast, or `None`
    /// when the edit changed nothing. On error nothing changed (a batch is atomic).
    pub fn local_op(&mut self, op: &Op) -> Result<Option<Envelope>, String> {
        let r = self.local_tx(|doc, tx| translate(doc, tx, op))?;
        Ok(r.map(|(env, undos)| {
            self.push_undo(undos, true);
            env
        }))
    }

    /// Apply a local edit given as the complete new document (diffed against the current one).
    pub fn local_doc(&mut self, new_doc: &Value) -> Result<Option<Envelope>, String> {
        let r = self.local_tx(|doc, tx| diff_slot(doc, tx, Slot::Root, new_doc))?;
        Ok(r.map(|(env, undos)| {
            self.push_undo(undos, true);
            env
        }))
    }

    fn push_undo(&mut self, undos: Vec<Undo>, clear_redo: bool) {
        if !undos.is_empty() {
            self.undo.push(undos);
            if self.undo.len() > MAX_UNDO {
                self.undo.remove(0);
            }
        }
        if clear_redo {
            self.redo.clear();
        }
    }

    /// Undo this replica's own most recent edit (skipping steps that no longer have any effect).
    pub fn undo(&mut self) -> Result<Option<Envelope>, String> {
        self.step_back(true)
    }
    pub fn redo(&mut self) -> Result<Option<Envelope>, String> {
        self.step_back(false)
    }

    fn step_back(&mut self, undoing: bool) -> Result<Option<Envelope>, String> {
        loop {
            let recs = if undoing { self.undo.pop() } else { self.redo.pop() };
            let Some(recs) = recs else { return Ok(None) };
            let r = self.local_tx(|doc, tx| {
                for u in recs.iter().rev() {
                    // a record whose target is gone is skipped; whatever was emitted before stays consistent
                    let _ = plan_inverse(doc, tx, u);
                }
                Ok(())
            })?;
            if let Some((env, inv)) = r {
                let inv = coalesce(inv);
                if !inv.is_empty() {
                    let stack = if undoing { &mut self.redo } else { &mut self.undo };
                    stack.push(inv);
                }
                return Ok(Some(env));
            }
        }
    }

    fn deliverable(&self, e: &Envelope) -> bool {
        let have = self.applied.get(&e.replica).copied().unwrap_or(0);
        e.seq == have + 1 && e.deps.iter().all(|(r, s)| self.applied.get(r).copied().unwrap_or(0) >= *s || (*r == e.replica && *s < e.seq && *s <= have))
    }

    /// Applied envelopes still in the log, oldest first (after compaction a suffix of the full history).
    pub fn log_entries(&self) -> &[Envelope] {
        &self.log
    }

    /// Apply one deliverable envelope only if `validate` accepts the merged document; otherwise leave the
    /// replica exactly as it was. Used by a server that must never hold an invalid project.
    pub fn receive_checked(&mut self, e: Envelope, validate: &mut dyn FnMut(&Value) -> Result<(), Vec<String>>) -> Result<(), Vec<String>> {
        self.validate(&e).map_err(|x| vec![x])?;
        if !self.deliverable(&e) {
            return Err(vec![format!("envelope {}:{} is not deliverable now (duplicate, or a predecessor is missing)", e.replica, e.seq)]);
        }
        let backup = (self.doc.clone(), self.applied.clone(), self.lamport, self.dropped);
        let log_len = self.log.len();
        self.apply_env(e);
        if let Err(errs) = validate(&self.doc.to_json()) {
            self.doc = backup.0;
            self.applied = backup.1;
            self.lamport = backup.2;
            self.dropped = backup.3;
            self.log.truncate(log_len);
            return Err(errs);
        }
        Ok(())
    }

    /// Would `receive` apply this envelope right now (not a duplicate, all predecessors present)?
    pub fn is_deliverable(&self, e: &Envelope) -> bool {
        self.deliverable(e)
    }

    fn validate(&self, e: &Envelope) -> Result<(), String> {
        if e.replica == 0 || e.seq == 0 {
            return Err("envelope needs replica >= 1 and seq >= 1".into());
        }
        if e.lamport == 0 || e.lamport > (1u64 << 60) {
            return Err("envelope lamport time out of range".into());
        }
        if e.ops.len() > MAX_PRIMS || e.deps.len() > 4096 {
            return Err("envelope is too large".into());
        }
        if e.replica == self.id && e.seq > self.applied.get(&self.id).copied().unwrap_or(0) {
            return Err("an envelope carries this replica's id but was not made by it".into());
        }
        Ok(())
    }

    fn apply_env(&mut self, e: Envelope) {
        let meta = Meta { replica: e.replica, seq: e.seq, lamport: e.lamport };
        let mut gen = IdGen { r: e.replica, s: e.seq, next: 0 };
        for (i, p) in e.ops.iter().enumerate() {
            if self.doc.apply_prim(&meta, i as u32, &mut gen, p, false).is_err() {
                self.dropped += 1;
            }
        }
        self.applied.insert(e.replica, e.seq);
        self.lamport = self.lamport.max(e.lamport);
        self.log.push(e);
    }

    /// Receive an envelope from another replica (or a re-delivery of any envelope).
    pub fn receive(&mut self, e: Envelope) -> Result<Delivery, String> {
        self.validate(&e)?;
        if e.seq <= self.applied.get(&e.replica).copied().unwrap_or(0) {
            return Ok(Delivery::Duplicate);
        }
        if self.pending.iter().any(|p| p.replica == e.replica && p.seq == e.seq) {
            return Ok(Delivery::Buffered);
        }
        if self.pending.len() >= MAX_PENDING {
            return Err("too many envelopes waiting for their predecessors".into());
        }
        self.pending.push(e);
        let mut applied = Vec::new();
        loop {
            let next = self.pending.iter().enumerate().filter(|(_, p)| self.deliverable(p)).min_by_key(|(_, p)| (p.replica, p.seq)).map(|(i, _)| i);
            let Some(i) = next else { break };
            let env = self.pending.remove(i);
            applied.push(env.clone());
            self.apply_env(env);
        }
        Ok(if applied.is_empty() { Delivery::Buffered } else { Delivery::Applied(applied) })
    }

    /// Envelopes a peer with clock `clock` still lacks, in a causal order. `Err` when the log was
    /// compacted past what the peer needs (it must fetch a snapshot instead).
    pub fn missing_for(&self, clock: &VClock) -> Result<Vec<Envelope>, String> {
        let mut need = 0usize;
        for (r, s) in &self.applied {
            need += (*s - clock.get(r).copied().unwrap_or(0).min(*s)) as usize;
        }
        let out: Vec<Envelope> = self.log.iter().filter(|e| e.seq > clock.get(&e.replica).copied().unwrap_or(0)).cloned().collect();
        if out.len() < need {
            return Err("log was compacted past the peer's clock: fetch a snapshot".into());
        }
        Ok(out)
    }

    /// Snapshot + log compaction. `peers` are the clocks of every other replica that takes part.
    pub fn compact(&mut self, peers: &[VClock]) -> CompactStats {
        let mut st = CompactStats::default();
        let mut min = self.applied.clone();
        for p in peers {
            for (r, s) in min.iter_mut() {
                *s = (*s).min(p.get(r).copied().unwrap_or(0));
            }
        }
        // only a prefix of the log is dropped, so positions stay meaningful for a relay that numbers them
        let k = self.log.iter().take_while(|e| e.seq <= min.get(&e.replica).copied().unwrap_or(0)).count();
        self.log.drain(..k);
        st.log_dropped = k;
        let nz = |c: &VClock| -> VClock { c.iter().filter(|(_, v)| **v > 0).map(|(k, v)| (*k, *v)).collect() };
        let mine = nz(&self.applied);
        if self.pending.is_empty() && peers.iter().all(|p| nz(p) == mine) {
            st.purged = true;
            for c in self.doc.nodes.values_mut() {
                match c {
                    Cont::Arr(v) => {
                        let n = v.len();
                        v.retain(|e| e.tomb.is_none());
                        st.tombstones_purged += n - v.len();
                    }
                    Cont::Obj(m) => {
                        m.retain(|_, r| r.val != Val::Absent);
                    }
                }
            }
            let keep = self.doc.reachable();
            let n = self.doc.nodes.len();
            self.doc.nodes.retain(|id, _| keep.contains(id));
            st.nodes_collected = n - self.doc.nodes.len();
        }
        st
    }
}

// ---------------------------------------------------------------------------------------------
// presence (cursor, selection, name, colour) with expiry

const PALETTE: [&str; 8] = ["#e6194b", "#3cb44b", "#4363d8", "#f58231", "#911eb4", "#008080", "#9a6324", "#d2691e"];

pub fn colour_for(replica: ReplicaId) -> String {
    PALETTE[(replica as usize) % PALETTE.len()].to_string()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Presence {
    pub replica: ReplicaId,
    pub name: String,
    pub colour: String,
    /// Playhead of that user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<f32>,
    /// Pointer position in canvas units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<[f32; 2]>,
    /// Selected layer paths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selection: Vec<String>,
    /// Canvas rectangle `[x0, y0, x1, y1]` around the selected layers at the user's playhead; set by the
    /// server from the selection (never taken from the client), so the others can draw the selection box.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<[f32; 4]>,
    /// Sender's counter: an older update never overwrites a newer one.
    #[serde(default)]
    pub n: u64,
    /// Time of the last update in the caller's clock (milliseconds).
    #[serde(default)]
    pub seen_ms: u64,
}

fn valid_colour(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())
}

impl Presence {
    pub fn new(replica: ReplicaId, name: &str) -> Presence {
        Presence { replica, name: clean_name(name, replica), colour: colour_for(replica), frame: None, cursor: None, selection: vec![], bounds: None, n: 0, seen_ms: 0 }
    }
    /// Make an untrusted update safe: bounded sizes, finite numbers, a valid `#rrggbb` colour.
    pub fn sanitized(mut self) -> Presence {
        self.name = clean_name(&self.name, self.replica);
        if !valid_colour(&self.colour) {
            self.colour = colour_for(self.replica);
        }
        if let Some(f) = self.frame {
            if !f.is_finite() {
                self.frame = None;
            }
        }
        if let Some(c) = self.cursor {
            if !(c[0].is_finite() && c[1].is_finite()) {
                self.cursor = None;
            }
        }
        if let Some(b) = self.bounds {
            if !b.iter().all(|v| v.is_finite()) {
                self.bounds = None;
            }
        }
        self.selection.truncate(64);
        for s in self.selection.iter_mut() {
            *s = s.chars().filter(|c| !c.is_control()).take(200).collect();
        }
        self
    }
}

fn clean_name(n: &str, replica: ReplicaId) -> String {
    let s: String = n.chars().filter(|c| !c.is_control()).take(40).collect();
    let s = s.trim().to_string();
    if s.is_empty() {
        format!("user{replica}")
    } else {
        s
    }
}

/// Who is here. The caller supplies the time (milliseconds, any monotone clock), so this is deterministic.
#[derive(Clone, Debug)]
pub struct PresenceBoard {
    entries: BTreeMap<ReplicaId, Presence>,
    pub ttl_ms: u64,
}

impl PresenceBoard {
    pub fn new(ttl_ms: u64) -> PresenceBoard {
        PresenceBoard { entries: BTreeMap::new(), ttl_ms }
    }
    /// Returns true when the update was accepted (not older than what is stored).
    pub fn update(&mut self, p: Presence, now_ms: u64) -> bool {
        let mut p = p.sanitized();
        if let Some(old) = self.entries.get(&p.replica) {
            if p.n < old.n {
                return false;
            }
        }
        p.seen_ms = now_ms;
        self.entries.insert(p.replica, p);
        true
    }
    pub fn touch(&mut self, replica: ReplicaId, now_ms: u64) {
        if let Some(p) = self.entries.get_mut(&replica) {
            p.seen_ms = now_ms;
        }
    }
    pub fn remove(&mut self, replica: ReplicaId) -> bool {
        self.entries.remove(&replica).is_some()
    }
    /// Drop entries not refreshed within the TTL; returns who expired.
    pub fn expire(&mut self, now_ms: u64) -> Vec<ReplicaId> {
        let ttl = self.ttl_ms;
        let gone: Vec<ReplicaId> = self.entries.iter().filter(|(_, p)| now_ms.saturating_sub(p.seen_ms) > ttl).map(|(r, _)| *r).collect();
        for r in &gone {
            self.entries.remove(r);
        }
        gone
    }
    pub fn get(&self, r: ReplicaId) -> Option<&Presence> {
        self.entries.get(&r)
    }
    pub fn all(&self) -> Vec<Presence> {
        self.entries.values().cloned().collect()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ---------------------------------------------------------------------------------------------
// seeded simulation (used by the tests and by `armatura collab-sim`)

/// splitmix64: tiny deterministic generator, no dependency.
pub struct SimRng(pub u64);

impl SimRng {
    // Not `Iterator::next`: an endless generator returning `u64`, and the name is part of the stable API.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
    pub fn chance(&mut self, pct: usize) -> bool {
        self.below(100) < pct
    }
    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i + 1);
            v.swap(i, j);
        }
    }
}

fn collect_paths(v: &Value, path: &mut String, out: &mut Vec<String>, depth: usize) {
    out.push(path.clone());
    if depth > 12 || out.len() > 4000 {
        return;
    }
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                let n = path.len();
                path.push('/');
                path.push_str(&k.replace('~', "~0").replace('/', "~1"));
                collect_paths(x, path, out, depth + 1);
                path.truncate(n);
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                let n = path.len();
                path.push_str(&format!("/{i}"));
                collect_paths(x, path, out, depth + 1);
                path.truncate(n);
            }
        }
        _ => {}
    }
}

fn bump_leaf(v: &mut Value, rng: &mut SimRng) -> bool {
    match v {
        Value::Number(n) => {
            let x = n.as_f64().unwrap_or(0.0);
            let d = (rng.below(21) as f64 - 10.0) / 2.0;
            *v = serde_json::Number::from_f64(x + d).map(Value::Number).unwrap_or(Value::Null);
            true
        }
        Value::String(s) => {
            *s = format!("s{}", rng.below(1000));
            true
        }
        Value::Bool(b) => {
            *b = !*b;
            true
        }
        Value::Object(m) => {
            let keys: Vec<String> = m.keys().cloned().collect();
            if keys.is_empty() {
                return false;
            }
            let k = &keys[rng.below(keys.len())];
            bump_leaf(m.get_mut(k).unwrap(), rng)
        }
        Value::Array(a) => {
            if a.is_empty() {
                return false;
            }
            let i = rng.below(a.len());
            bump_leaf(&mut a[i], rng)
        }
        Value::Null => false,
    }
}

/// A random edit of `doc` (any JSON), or `None` when the dice picked something impossible.
pub fn random_op(doc: &Value, rng: &mut SimRng) -> Option<Op> {
    let mut paths = Vec::new();
    collect_paths(doc, &mut String::new(), &mut paths, 0);
    let path = paths[rng.below(paths.len())].clone();
    let at = if path.is_empty() { doc } else { doc.pointer(&path)? };
    let parent_ptr = path.rfind('/').map(|i| path[..i].to_string());
    let parent = parent_ptr.as_ref().and_then(|p| if p.is_empty() { Some(doc) } else { doc.pointer(p) });
    let in_array = parent.map(|p| p.is_array()).unwrap_or(false);
    let fresh = |rng: &mut SimRng| serde_json::json!({"name": format!("n{}", rng.below(50)), "v": rng.below(100), "tags": [rng.below(5), rng.below(5)]});
    match at {
        Value::Number(_) | Value::String(_) | Value::Bool(_) => {
            let mut nv = at.clone();
            bump_leaf(&mut nv, rng);
            if in_array && rng.chance(25) {
                return Some(Op::Remove { path });
            }
            if in_array && rng.chance(20) {
                let i = path.rsplit('/').next()?.to_string();
                let p = parent_ptr?;
                return Some(Op::Insert { path: format!("{p}/{i}"), value: nv });
            }
            Some(Op::Set { path, value: nv })
        }
        Value::Null => Some(Op::Set { path, value: Value::from(rng.below(10) as u64) }),
        Value::Array(a) => {
            let r = rng.below(10);
            if r < 6 || a.is_empty() {
                let i = rng.below(a.len() + 2);
                let ptr = if i > a.len() { format!("{path}/-") } else { format!("{path}/{i}") };
                let v = if !a.is_empty() && rng.chance(40) { a[rng.below(a.len())].clone() } else { fresh(rng) };
                Some(Op::Insert { path: ptr, value: v })
            } else if r < 8 {
                let i = rng.below(a.len());
                Some(Op::Remove { path: format!("{path}/{i}") })
            } else {
                let mut nv = at.clone();
                bump_leaf(&mut nv, rng);
                Some(Op::Set { path, value: nv })
            }
        }
        Value::Object(m) => {
            let r = rng.below(10);
            if r < 4 {
                let k = format!("k{}", rng.below(4));
                let v = if rng.chance(50) { Value::from(rng.below(100) as u64) } else { fresh(rng) };
                Some(Op::Set { path: format!("{path}/{k}"), value: v })
            } else if r < 6 && !m.is_empty() && !path.is_empty() {
                let keys: Vec<&String> = m.keys().collect();
                let k = keys[rng.below(keys.len())].replace('~', "~0").replace('/', "~1");
                Some(Op::Remove { path: format!("{path}/{k}") })
            } else if r < 9 {
                // whole-object replacement with one leaf changed: exercises the structural diff
                let mut nv = at.clone();
                if !bump_leaf(&mut nv, rng) {
                    return None;
                }
                Some(Op::Set { path, value: nv })
            } else if !path.is_empty() && in_array {
                Some(Op::Remove { path })
            } else {
                None
            }
        }
    }
}

/// A random edit that keeps a *project* document valid: numeric properties (bone rest pose, layer opacity / position,
/// key values of position/rotation channels), layer visibility, and layers the simulation itself created
/// (`sim_*`: inserted, moved by re-creating under a new name, removed). Referenced layers are never removed or
/// renamed, so every merge of such edits is still a project that compiles.
pub fn project_op(doc: &Value, rng: &mut SimRng) -> Option<Op> {
    let arr = |p: &str| doc.pointer(p).and_then(Value::as_array);
    let layers = arr("/layers")?;
    let delta = |rng: &mut SimRng, x: f64, span: usize| x + (rng.below(span * 2 + 1) as f64 - span as f64) / 2.0;
    let num = |v: &Value| v.as_f64();
    for _ in 0..8 {
        match rng.below(9) {
            0 => {
                let bones = arr("/bones")?;
                if bones.is_empty() {
                    continue;
                }
                let i = rng.below(bones.len());
                let x = num(&bones[i]["rest_angle"])?;
                return Some(Op::Set { path: format!("/bones/{i}/rest_angle"), value: Value::from(delta(rng, x, 10)) });
            }
            1 => {
                let bones = arr("/bones")?;
                if bones.is_empty() {
                    continue;
                }
                let (i, k) = (rng.below(bones.len()), rng.below(2));
                let x = num(&bones[i]["rest_offset"][k])?;
                return Some(Op::Set { path: format!("/bones/{i}/rest_offset/{k}"), value: Value::from(delta(rng, x, 6)) });
            }
            2 => {
                if layers.is_empty() {
                    continue;
                }
                let i = rng.below(layers.len());
                let v = (0.2 + rng.below(81) as f64 / 100.0).min(1.0);
                return Some(Op::Set { path: format!("/layers/{i}/opacity"), value: Value::from(v) });
            }
            3 => {
                if layers.is_empty() {
                    continue;
                }
                let (i, k) = (rng.below(layers.len()), rng.below(2));
                let x = num(&layers[i]["transform"]["position"][k]).unwrap_or(0.0);
                return Some(Op::Set { path: format!("/layers/{i}/transform/position/{k}"), value: Value::from(delta(rng, x, 20)) });
            }
            4 => {
                if layers.is_empty() {
                    continue;
                }
                let i = rng.below(layers.len());
                let v = layers[i]["visible"].as_bool().unwrap_or(true);
                return Some(Op::Set { path: format!("/layers/{i}/visible"), value: Value::Bool(!v) });
            }
            5 => {
                // a key value of a position / rotation channel
                let chans = arr("/channels")?;
                let ok = ["bone_angle", "bone_offset_x", "bone_offset_y", "layer_x", "layer_y", "layer_rotation"];
                let keyed: Vec<usize> = (0..chans.len())
                    .filter(|&c| chans[c]["source"] == "keys" && chans[c]["prop"].as_str().is_some_and(|p| ok.contains(&p)) && chans[c]["track"]["keys"].as_array().is_some_and(|k| !k.is_empty()))
                    .collect();
                if keyed.is_empty() {
                    continue;
                }
                let c = keyed[rng.below(keyed.len())];
                let keys = chans[c]["track"]["keys"].as_array()?;
                let k = rng.below(keys.len());
                let x = num(&keys[k]["value"])?;
                return Some(Op::Set { path: format!("/channels/{c}/track/keys/{k}/value"), value: Value::from(delta(rng, x, 10)) });
            }
            6 => {
                // insert a fresh layer of our own
                let name = format!("sim_{}", rng.below(1_000_000_000));
                let (x, y) = (rng.below(300) as f32, rng.below(300) as f32);
                let l = crate::project::Layer::vector(&name, vec![crate::path::VPath::rect("r", x, y, 20.0 + rng.below(60) as f32, 20.0 + rng.below(60) as f32).with_fill("#3366aa")]);
                let at = rng.below(layers.len() + 1);
                return Some(Op::Insert { path: format!("/layers/{at}"), value: serde_json::to_value(&l).ok()? });
            }
            7 => {
                let own: Vec<usize> = (0..layers.len()).filter(|&i| layers[i]["name"].as_str().is_some_and(|n| n.starts_with("sim_"))).collect();
                if own.is_empty() {
                    continue;
                }
                return Some(Op::Remove { path: format!("/layers/{}", own[rng.below(own.len())]) });
            }
            _ => {
                // move one of our layers: re-create it under a new name at another index (no duplicate names even when two
                // replicas move the same layer concurrently)
                let own: Vec<usize> = (0..layers.len()).filter(|&i| layers[i]["name"].as_str().is_some_and(|n| n.starts_with("sim_"))).collect();
                if own.is_empty() {
                    continue;
                }
                let i = own[rng.below(own.len())];
                let mut v = layers[i].clone();
                v["name"] = Value::from(format!("sim_{}", rng.below(1_000_000_000)));
                let at = rng.below(layers.len());
                return Some(Op::Batch { ops: vec![Op::Remove { path: format!("/layers/{i}") }, Op::Insert { path: format!("/layers/{}", at.min(layers.len() - 1)), value: v }] });
            }
        }
    }
    None
}

#[derive(Clone, Debug)]
pub struct SimConfig {
    pub clients: usize,
    pub ops: usize,
    pub seed: u64,
    /// Percent of steps that are an undo / redo of the acting client.
    pub undo_pct: usize,
    /// Percent of sent envelopes that are delivered twice.
    pub dup_pct: usize,
    /// Edit with [`project_op`] (valid project edits) instead of arbitrary JSON edits ([`random_op`]).
    pub project_aware: bool,
}

impl SimConfig {
    pub fn new(clients: usize, ops: usize, seed: u64) -> SimConfig {
        SimConfig { clients, ops, seed, undo_pct: 12, dup_pct: 8, project_aware: false }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SimReport {
    pub clients: usize,
    pub edits: usize,
    pub rejected_local: usize,
    pub noops: usize,
    pub undos: usize,
    pub envelopes: usize,
    pub deliveries: usize,
    pub duplicates: usize,
    pub buffered: usize,
    pub dropped_prims: u64,
    pub converged: bool,
    pub state_identical: bool,
    pub clocks_identical: bool,
    pub pending_left: usize,
    pub tombstones: usize,
    pub doc_bytes: usize,
    pub divergence: Option<String>,
    /// Every envelope produced, in creation order (for replaying in other orders).
    pub trace: Vec<Envelope>,
    pub final_doc: Value,
}

/// Run `cfg.clients` replicas that edit concurrently and exchange envelopes in random order, with
/// duplicates and reordering, then deliver everything and compare all replicas.
pub fn simulate(base: &Value, cfg: &SimConfig) -> Result<SimReport, String> {
    if cfg.clients == 0 || cfg.clients > 64 {
        return Err("clients must be 1..=64".into());
    }
    let mut rng = SimRng(cfg.seed);
    let mut reps: Vec<Replica> = (0..cfg.clients).map(|i| Replica::new(i as u32 + 1, base)).collect::<Result<_, _>>()?;
    let mut inbox: Vec<Vec<Envelope>> = vec![vec![]; cfg.clients];
    let mut rep = SimReport { clients: cfg.clients, ..Default::default() };
    let deliver_one = |reps: &mut Vec<Replica>, inbox: &mut Vec<Vec<Envelope>>, who: usize, rng: &mut SimRng, rep: &mut SimReport| {
        if inbox[who].is_empty() {
            return;
        }
        let k = rng.below(inbox[who].len());
        let e = inbox[who].swap_remove(k);
        rep.deliveries += 1;
        match reps[who].receive(e) {
            Ok(Delivery::Duplicate) => rep.duplicates += 1,
            Ok(Delivery::Buffered) => rep.buffered += 1,
            _ => {}
        }
    };
    let mut done = 0;
    let mut guard = 0;
    while done < cfg.ops && guard < cfg.ops * 20 {
        guard += 1;
        // random background delivery
        for _ in 0..rng.below(3) {
            let who = rng.below(cfg.clients);
            deliver_one(&mut reps, &mut inbox, who, &mut rng, &mut rep);
        }
        let c = rng.below(cfg.clients);
        let env = if rng.chance(cfg.undo_pct) {
            rep.undos += 1;
            if rng.chance(70) {
                reps[c].undo()?
            } else {
                reps[c].redo()?
            }
        } else {
            let cur = reps[c].json();
            let gen = |cur: &Value, rng: &mut SimRng| if cfg.project_aware { project_op(cur, rng) } else { random_op(cur, rng) };
            let Some(op) = gen(&cur, &mut rng) else { continue };
            let op = if rng.chance(15) {
                match gen(&cur, &mut rng) {
                    // indices of the second op were chosen against the same document: after a structural first op (insert /
                    // remove / move) they would hit other elements, so project mode batches plain value sets only
                    Some(o2) if !cfg.project_aware || (matches!(op, Op::Set { .. }) && matches!(o2, Op::Set { .. })) => Op::Batch { ops: vec![op, o2] },
                    _ => op,
                }
            } else {
                op
            };
            match reps[c].local_op(&op) {
                Ok(e) => e,
                Err(_) => {
                    rep.rejected_local += 1;
                    continue;
                }
            }
        };
        done += 1;
        rep.edits += 1;
        match env {
            None => rep.noops += 1,
            Some(e) => {
                rep.envelopes += 1;
                rep.trace.push(e.clone());
                for (j, q) in inbox.iter_mut().enumerate() {
                    if j != c {
                        q.push(e.clone());
                        if rng.chance(cfg.dup_pct) {
                            q.push(e.clone());
                        }
                    }
                }
            }
        }
    }
    for who in 0..cfg.clients {
        while !inbox[who].is_empty() {
            deliver_one(&mut reps, &mut inbox, who, &mut rng, &mut rep);
        }
    }
    let j0 = reps[0].json();
    rep.converged = reps.iter().all(|r| r.json() == j0);
    rep.state_identical = reps.iter().all(|r| r.doc == reps[0].doc);
    rep.clocks_identical = reps.iter().all(|r| r.applied == reps[0].applied);
    rep.pending_left = reps.iter().map(|r| r.pending.len()).sum();
    rep.dropped_prims = reps.iter().map(|r| r.dropped).sum();
    rep.tombstones = reps[0].tombstones();
    rep.doc_bytes = j0.to_string().len();
    if !rep.converged {
        let i = reps.iter().position(|r| r.json() != j0).unwrap_or(0);
        rep.divergence = Some(format!("replica {} differs from replica 1", i + 1));
    } else if !rep.state_identical {
        rep.divergence = Some("visible documents equal but internal state differs".into());
    }
    rep.final_doc = j0;
    Ok(rep)
}

// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base() -> Value {
        json!({
            "name": "t", "frames": 24,
            "layers": [{"name": "a", "opacity": 1.0, "visible": true}, {"name": "b", "opacity": 0.5, "visible": true}, {"name": "c", "opacity": 0.2, "visible": true}],
            "channels": [{"target": "a", "prop": "x", "track": {"keys": [{"frame": 0, "value": 0.0}, {"frame": 10, "value": 5.0}]}}],
            "meta": {"tags": ["x", "y"]}
        })
    }

    fn pair() -> (Replica, Replica) {
        (Replica::new(1, &base()).unwrap(), Replica::new(2, &base()).unwrap())
    }

    fn set(p: &str, v: Value) -> Op {
        Op::Set { path: p.into(), value: v }
    }
    fn ins(p: &str, v: Value) -> Op {
        Op::Insert { path: p.into(), value: v }
    }
    fn rem(p: &str) -> Op {
        Op::Remove { path: p.into() }
    }
    fn send(from: &mut Replica, op: &Op, to: &mut [&mut Replica]) -> Envelope {
        let e = from.local_op(op).unwrap().expect("envelope");
        for t in to {
            t.receive(e.clone()).unwrap();
        }
        e
    }

    #[test]
    fn local_ops_match_the_editor_semantics() {
        let mut r = Replica::new(1, &base()).unwrap();
        r.local_op(&set("/layers/1/opacity", json!(0.9))).unwrap();
        r.local_op(&ins("/layers/1", json!({"name": "new"}))).unwrap();
        r.local_op(&ins("/layers/-", json!({"name": "last"}))).unwrap();
        r.local_op(&rem("/layers/0")).unwrap();
        r.local_op(&set("/meta/extra", json!(7))).unwrap();
        r.local_op(&rem("/meta/tags")).unwrap();
        let mut want = base();
        // reference: the same ops on a plain document
        want["layers"][1]["opacity"] = json!(0.9);
        want["layers"].as_array_mut().unwrap().insert(1, json!({"name": "new"}));
        want["layers"].as_array_mut().unwrap().push(json!({"name": "last"}));
        want["layers"].as_array_mut().unwrap().remove(0);
        want["meta"]["extra"] = json!(7);
        want["meta"].as_object_mut().unwrap().remove("tags");
        assert_eq!(r.json(), want);
    }

    #[test]
    fn bad_local_ops_change_nothing_and_batches_are_atomic() {
        let mut r = Replica::new(1, &base()).unwrap();
        for op in [
            rem("/layers/9"),
            set("/nope/x", json!(1)),
            set("/layers/abc", json!(1)),
            ins("/layers/9", json!(1)),
            rem("/meta/missing"),
            ins("", json!(1)),
            set("layers", json!(1)),
            set("/frames/x", json!(1)),
        ] {
            assert!(r.local_op(&op).is_err(), "{op:?}");
            assert_eq!(r.json(), base());
        }
        let b = Op::Batch { ops: vec![set("/frames", json!(99)), rem("/layers/7")] };
        assert!(r.local_op(&b).is_err());
        assert_eq!(r.json(), base());
        assert_eq!(r.clock().get(&1), None, "a failed edit consumes no sequence number");
        assert!(r.local_op(&set("/frames", json!(24))).unwrap().is_none(), "writing the same value is a no-op");
    }

    #[test]
    fn concurrent_scalar_writes_use_lww_with_replica_tiebreak() {
        let (mut a, mut b) = pair();
        let ea = a.local_op(&set("/layers/0/opacity", json!(0.1))).unwrap().unwrap();
        let eb = b.local_op(&set("/layers/0/opacity", json!(0.9))).unwrap().unwrap();
        assert_eq!(ea.lamport, eb.lamport);
        a.receive(eb).unwrap();
        b.receive(ea).unwrap();
        assert_eq!(a.json(), b.json());
        assert_eq!(a.json()["layers"][0]["opacity"], json!(0.9), "equal lamport: the larger replica id wins");
        // a later write (larger lamport) beats a higher replica id
        let e = a.local_op(&set("/layers/0/opacity", json!(0.3))).unwrap().unwrap();
        b.receive(e).unwrap();
        assert_eq!(b.json()["layers"][0]["opacity"], json!(0.3));
    }

    #[test]
    fn stale_indices_are_transformed_by_identity() {
        let (mut a, mut b) = pair();
        // a inserts at the front; b (not having seen it) edits what it sees as layer 1 ("b")
        let ea = a.local_op(&ins("/layers/0", json!({"name": "front"}))).unwrap().unwrap();
        let eb = b.local_op(&set("/layers/1/opacity", json!(0.77))).unwrap().unwrap();
        a.receive(eb).unwrap();
        b.receive(ea).unwrap();
        assert_eq!(a.json(), b.json());
        let l = a.json()["layers"].clone();
        assert_eq!(l[0]["name"], "front");
        assert_eq!(l[2]["name"], "b");
        assert_eq!(l[2]["opacity"], json!(0.77), "the edit followed its element, not its index");
        assert_eq!(l[1]["opacity"], json!(1.0));
    }

    #[test]
    fn concurrent_inserts_at_the_same_place_keep_both_in_one_order() {
        let (mut a, mut b) = pair();
        let ea = a.local_op(&ins("/layers/1", json!({"name": "from_a"}))).unwrap().unwrap();
        let eb = b.local_op(&ins("/layers/1", json!({"name": "from_b"}))).unwrap().unwrap();
        a.receive(eb).unwrap();
        b.receive(ea).unwrap();
        assert_eq!(a.json(), b.json());
        let names: Vec<String> = a.json()["layers"].as_array().unwrap().iter().map(|l| l["name"].as_str().unwrap().to_string()).collect();
        assert_eq!(names.len(), 5);
        assert_eq!(names[0], "a");
        assert_eq!(names[3], "b");
        assert!(names.contains(&"from_a".to_string()) && names.contains(&"from_b".to_string()));
    }

    #[test]
    fn remove_wins_over_edit_inside_and_the_edit_is_dropped_deterministically() {
        let (mut a, mut b) = pair();
        let ea = a.local_op(&rem("/layers/1")).unwrap().unwrap();
        let eb = b.local_op(&set("/layers/1/opacity", json!(0.01))).unwrap().unwrap();
        a.receive(eb).unwrap();
        b.receive(ea).unwrap();
        assert_eq!(a.json(), b.json());
        assert_eq!(a.json()["layers"].as_array().unwrap().len(), 2);
        assert!(!a.json().to_string().contains("0.01"));
        assert_eq!(a.doc(), b.doc());
        assert_eq!(a.dropped, 0, "applied to the hidden element, not an error");
        // both remove the same element
        let (mut a, mut b) = pair();
        let ea = a.local_op(&rem("/layers/0")).unwrap().unwrap();
        let eb = b.local_op(&rem("/layers/0")).unwrap().unwrap();
        a.receive(eb).unwrap();
        b.receive(ea).unwrap();
        assert_eq!(a.doc(), b.doc());
        assert_eq!(a.json()["layers"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn whole_object_set_is_diffed_so_concurrent_key_edits_both_survive() {
        let (mut a, mut b) = pair();
        // a adds a key to the channel track by writing the whole channel (what Editor::set_key does)
        let mut ch = base()["channels"][0].clone();
        ch["track"]["keys"].as_array_mut().unwrap().push(json!({"frame": 20, "value": 1.5}));
        let ea = a.local_op(&set("/channels/0", ch)).unwrap().unwrap();
        // b meanwhile changes an existing key's value
        let eb = b.local_op(&set("/channels/0/track/keys/0/value", json!(-3.0))).unwrap().unwrap();
        assert_eq!(ea.ops.len(), 1, "only the new key travels: {:?}", ea.ops);
        a.receive(eb).unwrap();
        b.receive(ea).unwrap();
        assert_eq!(a.json(), b.json());
        let keys = a.json()["channels"][0]["track"]["keys"].clone();
        assert_eq!(keys.as_array().unwrap().len(), 3);
        assert_eq!(keys[0]["value"], json!(-3.0));
        assert_eq!(keys[2]["frame"], json!(20));
    }

    #[test]
    fn unrelated_objects_in_a_slot_are_replaced_not_merged() {
        let mut r = Replica::new(1, &base()).unwrap();
        let mut v = base();
        v["layers"] = json!([{"name": "z", "opacity": 3}]);
        let e = r.local_doc(&v).unwrap().unwrap();
        assert!(e.ops.iter().any(|p| matches!(p, Prim::DelElem { .. })) && e.ops.iter().any(|p| matches!(p, Prim::InsElem { .. })));
        assert_eq!(r.json(), v);
    }

    #[test]
    fn local_doc_diff_roundtrips_for_random_documents() {
        let mut rng = SimRng(5);
        for _ in 0..40 {
            let mut r = Replica::new(1, &base()).unwrap();
            let mut cur = base();
            for _ in 0..8 {
                if let Some(op) = random_op(&cur, &mut rng) {
                    // reference application on a second replica via pointer ops
                    let mut other = Replica::new(2, &base()).unwrap();
                    other.doc = r.doc.clone();
                    if other.local_op(&op).is_ok() {
                        let target = other.json();
                        r.local_doc(&target).unwrap();
                        assert_eq!(r.json(), target);
                        cur = target;
                    }
                }
            }
        }
    }

    #[test]
    fn redelivery_is_idempotent_and_the_causal_buffer_holds_back_early_envelopes() {
        let (mut a, mut b) = pair();
        let e1 = a.local_op(&ins("/layers/-", json!({"name": "n1"}))).unwrap().unwrap();
        let e2 = a.local_op(&set("/layers/3/opacity", json!(0.4))).unwrap().unwrap();
        // e2 first: buffered, nothing visible yet
        assert_eq!(b.receive(e2.clone()).unwrap(), Delivery::Buffered);
        assert_eq!(b.json(), base());
        assert_eq!(b.pending_len(), 1);
        assert_eq!(b.receive(e2.clone()).unwrap(), Delivery::Buffered, "same buffered envelope twice is harmless");
        match b.receive(e1.clone()).unwrap() {
            Delivery::Applied(v) => assert_eq!(v.len(), 2, "e1 releases e2"),
            d => panic!("{d:?}"),
        }
        assert_eq!(b.pending_len(), 0);
        assert_eq!(a.json(), b.json());
        let before = b.doc().clone();
        assert_eq!(b.receive(e1).unwrap(), Delivery::Duplicate);
        assert_eq!(b.receive(e2).unwrap(), Delivery::Duplicate);
        assert_eq!(*b.doc(), before);
    }

    #[test]
    fn cross_replica_dependency_waits_for_the_other_replica() {
        let mut a = Replica::new(1, &base()).unwrap();
        let mut b = Replica::new(2, &base()).unwrap();
        let mut c = Replica::new(3, &base()).unwrap();
        let ea = send(&mut a, &ins("/layers/-", json!({"name": "from_a"})), &mut [&mut b]);
        // b edits the element a created: its envelope depends on a's
        let eb = b.local_op(&set("/layers/3/opacity", json!(0.25))).unwrap().unwrap();
        assert_eq!(eb.deps.get(&1), Some(&1));
        assert_eq!(c.receive(eb).unwrap(), Delivery::Buffered);
        assert!(c.json()["layers"].as_array().unwrap().len() == 3);
        c.receive(ea).unwrap();
        assert_eq!(c.json(), b.json());
    }

    #[test]
    fn lamport_clock_follows_received_envelopes() {
        let (mut a, mut b) = pair();
        for i in 0..5 {
            a.local_op(&set("/frames", json!(100 + i))).unwrap();
        }
        let last = a.log.last().unwrap().clone();
        assert_eq!(last.lamport, 5);
        b.receive(last).unwrap_or(Delivery::Duplicate);
        for e in a.log.clone() {
            b.receive(e).unwrap();
        }
        assert_eq!(b.lamport(), 5);
        let e = b.local_op(&set("/frames", json!(7))).unwrap().unwrap();
        assert_eq!(e.lamport, 6, "happened-after => larger Lamport time");
        assert!(e.deps.get(&1) == Some(&5));
    }

    #[test]
    fn invalid_or_hostile_envelopes_never_panic() {
        let mut r = Replica::new(1, &base()).unwrap();
        let unk = Id { r: 9, s: 9, i: 9 };
        let prims = vec![
            Prim::SetKey { obj: unk, key: "k".into(), value: json!(1) },
            Prim::DelKey { obj: unk, key: "k".into() },
            Prim::SetElem { arr: unk, elem: unk, value: json!(1) },
            Prim::InsElem { arr: unk, after: Some(unk), value: json!(1) },
            Prim::DelElem { arr: unk, elem: unk },
            Prim::SetRoot { value: json!([1, 2, 3]) },
        ];
        // wrong kinds: array ops on an object id and vice versa
        let root_obj = match &r.doc.root.val {
            Val::Ref(id) => *id,
            _ => unreachable!(),
        };
        let mut ps = prims.clone();
        ps.push(Prim::InsElem { arr: root_obj, after: None, value: json!(1) });
        ps.push(Prim::SetKey { obj: root_obj, key: "ok".into(), value: json!(1) });
        let env = Envelope { replica: 2, seq: 1, lamport: 4, deps: VClock::new(), ops: ps };
        r.receive(env).unwrap();
        assert!(r.dropped >= 6);
        for bad in [
            Envelope { replica: 0, seq: 1, lamport: 1, deps: VClock::new(), ops: vec![] },
            Envelope { replica: 2, seq: 0, lamport: 1, deps: VClock::new(), ops: vec![] },
            Envelope { replica: 2, seq: 2, lamport: 0, deps: VClock::new(), ops: vec![] },
            Envelope { replica: 2, seq: 2, lamport: u64::MAX, deps: VClock::new(), ops: vec![] },
            Envelope { replica: 1, seq: 5, lamport: 1, deps: VClock::new(), ops: vec![] },
        ] {
            assert!(r.receive(bad).is_err());
        }
        // an envelope that depends on something that never arrives just waits
        let mut deps = VClock::new();
        deps.insert(7, 3);
        assert_eq!(r.receive(Envelope { replica: 2, seq: 2, lamport: 5, deps, ops: vec![] }).unwrap(), Delivery::Buffered);
    }

    #[test]
    fn hostile_envelopes_are_applied_identically_in_any_order() {
        let unk = Id { r: 9, s: 9, i: 9 };
        let mk = |rep: u32, n: u64| Envelope { replica: rep, seq: 1, lamport: n, deps: VClock::new(), ops: vec![Prim::DelElem { arr: unk, elem: unk }, Prim::SetRoot { value: json!({"w": n}) }] };
        let mut x = Replica::new(1, &base()).unwrap();
        let mut y = Replica::new(1, &base()).unwrap();
        for e in [mk(2, 3), mk(3, 3), mk(4, 9)] {
            x.receive(e).unwrap();
        }
        for e in [mk(4, 9), mk(3, 3), mk(2, 3)] {
            y.receive(e).unwrap();
        }
        assert_eq!(x.doc(), y.doc());
        assert_eq!(x.json(), json!({"w": 9}));
    }

    #[test]
    fn random_runs_converge_for_many_seeds_and_sizes() {
        for (clients, ops) in [(2, 60), (3, 120), (5, 200), (8, 150)] {
            for seed in 0..12u64 {
                let rep = simulate(&base(), &SimConfig::new(clients, ops, seed * 7919 + clients as u64)).unwrap();
                assert!(rep.converged, "clients {clients} seed {seed}: {:?}", rep.divergence);
                assert!(rep.state_identical, "internal state differs: clients {clients} seed {seed}");
                assert!(rep.clocks_identical && rep.pending_left == 0, "seed {seed}");
                assert_eq!(rep.dropped_prims, 0, "honest peers never produce dangling references (seed {seed})");
                assert!(rep.envelopes > ops / 3);
            }
        }
    }

    #[test]
    fn same_op_set_in_many_random_orders_gives_identical_projects() {
        let rep = simulate(&base(), &SimConfig::new(4, 150, 424242)).unwrap();
        assert!(rep.converged && rep.envelopes > 40);
        let reference = {
            let mut r = Replica::new(99, &base()).unwrap();
            for e in &rep.trace {
                r.receive(e.clone()).unwrap();
            }
            r
        };
        assert_eq!(reference.json(), rep.final_doc);
        for seed in 0..40u64 {
            let mut order = rep.trace.clone();
            SimRng(seed).shuffle(&mut order);
            // duplicates sprinkled in
            let extra: Vec<Envelope> = order.iter().step_by(5).cloned().collect();
            order.extend(extra);
            SimRng(seed + 1000).shuffle(&mut order);
            let mut r = Replica::new(100 + seed as u32, &base()).unwrap();
            for e in order {
                r.receive(e).unwrap();
            }
            assert_eq!(r.pending_len(), 0);
            assert_eq!(r.doc(), reference.doc(), "order seed {seed}");
        }
    }

    #[test]
    fn per_user_undo_touches_only_own_edits() {
        let (mut a, mut b) = pair();
        send(&mut a, &set("/layers/0/opacity", json!(0.1)), &mut [&mut b]);
        send(&mut b, &set("/layers/2/opacity", json!(0.9)), &mut [&mut a]);
        send(&mut a, &ins("/layers/-", json!({"name": "mine"})), &mut [&mut b]);
        send(&mut b, &set("/frames", json!(48)), &mut [&mut a]);
        // a undoes twice: the insert, then the opacity; b's edits stay
        for _ in 0..2 {
            let e = a.undo().unwrap().expect("something to undo");
            b.receive(e).unwrap();
        }
        assert_eq!(a.json(), b.json());
        let j = a.json();
        assert_eq!(j["layers"].as_array().unwrap().len(), 3);
        assert_eq!(j["layers"][0]["opacity"], json!(1.0));
        assert_eq!(j["layers"][2]["opacity"], json!(0.9));
        assert_eq!(j["frames"], json!(48));
        assert!(!a.can_undo());
        // redo brings a's edits back, again without touching b's
        let e = a.redo().unwrap().unwrap();
        b.receive(e).unwrap();
        let e = a.redo().unwrap().unwrap();
        b.receive(e).unwrap();
        assert_eq!(a.json(), b.json());
        assert_eq!(a.json()["layers"][0]["opacity"], json!(0.1));
        assert_eq!(a.json()["layers"][3]["name"], "mine");
        assert_eq!(a.json()["frames"], json!(48));
        assert!(a.redo().unwrap().is_none());
        // b can still undo its own, independently
        let e = b.undo().unwrap().unwrap();
        a.receive(e).unwrap();
        assert_eq!(a.json()["frames"], json!(24));
        assert_eq!(a.json()["layers"][0]["opacity"], json!(0.1));
    }

    #[test]
    fn undo_of_a_remove_reinserts_next_to_its_old_neighbour_after_remote_changes() {
        let (mut a, mut b) = pair();
        send(&mut a, &rem("/layers/1"), &mut [&mut b]); // removes "b"
        send(&mut b, &ins("/layers/0", json!({"name": "front"})), &mut [&mut a]);
        send(&mut b, &set("/layers/1/opacity", json!(0.6)), &mut [&mut a]); // edits "a"
        let e = a.undo().unwrap().unwrap();
        b.receive(e).unwrap();
        assert_eq!(a.json(), b.json());
        let names: Vec<String> = a.json()["layers"].as_array().unwrap().iter().map(|l| l["name"].as_str().unwrap().to_string()).collect();
        assert_eq!(names, vec!["front", "a", "b", "c"]);
        assert_eq!(a.json()["layers"][1]["opacity"], json!(0.6));
    }

    #[test]
    fn undo_skips_what_somebody_else_overwrote_later() {
        let (mut a, mut b) = pair();
        send(&mut a, &set("/layers/0/opacity", json!(0.1)), &mut [&mut b]);
        send(&mut b, &set("/layers/0/opacity", json!(0.8)), &mut [&mut a]);
        let before = a.json();
        let r = a.undo().unwrap();
        assert!(r.is_none(), "nothing of a's write is left to undo");
        assert_eq!(a.json(), before);
    }

    #[test]
    fn undo_of_a_multi_step_batch_restores_the_original() {
        let mut r = Replica::new(1, &base()).unwrap();
        r.local_op(&Op::Batch { ops: vec![set("/frames", json!(30)), set("/frames", json!(31)), ins("/layers/0", json!({"name": "q"})), rem("/layers/0"), rem("/layers/0")] }).unwrap();
        assert_ne!(r.json(), base());
        r.undo().unwrap();
        assert_eq!(r.json(), base());
        r.redo().unwrap();
        assert_eq!(r.json()["frames"], json!(31));
        assert_eq!(r.json()["layers"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn random_runs_with_heavy_undo_still_converge() {
        for seed in 0..10u64 {
            let mut cfg = SimConfig::new(4, 200, seed + 500);
            cfg.undo_pct = 45;
            let rep = simulate(&base(), &cfg).unwrap();
            assert!(rep.converged && rep.state_identical, "seed {seed}: {:?}", rep.divergence);
            assert!(rep.undos > 40);
        }
    }

    #[test]
    fn snapshot_roundtrips_through_json_and_a_joiner_continues_the_session() {
        let (mut a, mut b) = pair();
        send(&mut a, &ins("/layers/0", json!({"name": "x"})), &mut [&mut b]);
        send(&mut b, &rem("/layers/2"), &mut [&mut a]);
        let snap = a.snapshot();
        let text = serde_json::to_string(&snap).unwrap();
        let back: Snapshot = serde_json::from_str(&text).unwrap();
        let mut c = Replica::from_snapshot(3, back).unwrap();
        assert_eq!(c.json(), a.json());
        assert_eq!(c.clock(), a.clock());
        let e = c.local_op(&set("/layers/1/opacity", json!(0.42))).unwrap().unwrap();
        a.receive(e.clone()).unwrap();
        b.receive(e).unwrap();
        assert_eq!(a.doc(), b.doc());
        assert_eq!(a.json()["layers"][1]["opacity"], json!(0.42));
        // the snapshot's identities line up: an edit made by a before the snapshot-based join is understood by c
        let e = a.local_op(&set("/layers/0/name", json!("renamed"))).unwrap().unwrap();
        c.receive(e).unwrap();
        assert_eq!(a.json(), c.json());
        assert!(Replica::from_snapshot(0, a.snapshot()).is_err() && Replica::new(0, &base()).is_err());
    }

    #[test]
    fn envelopes_roundtrip_through_json() {
        let mut a = Replica::new(1, &base()).unwrap();
        let e = a.local_op(&Op::Batch { ops: vec![ins("/layers/1", json!({"name": "n", "k": [1, {"z": null}]})), rem("/layers/0"), set("/meta/new", json!("s"))] }).unwrap().unwrap();
        let text = serde_json::to_string(&e).unwrap();
        let back: Envelope = serde_json::from_str(&text).unwrap();
        assert_eq!(back, e);
        let mut b = Replica::new(2, &base()).unwrap();
        b.receive(back).unwrap();
        assert_eq!(a.doc(), b.doc());
        assert!(serde_json::from_str::<Envelope>("{\"replica\":1}").is_err());
        assert!(serde_json::from_str::<Prim>("{\"t\":\"del_elem\",\"arr\":\"bad\",\"elem\":\"1.1.1\"}").is_err());
    }

    #[test]
    fn compaction_drops_stable_log_and_purges_tombstones_only_when_quiescent() {
        let (mut a, mut b) = pair();
        send(&mut a, &rem("/layers/0"), &mut [&mut b]);
        send(&mut b, &rem("/layers/0"), &mut [&mut a]);
        send(&mut a, &ins("/layers/-", json!({"name": "k", "sub": {"x": [1, 2]}})), &mut [&mut b]);
        send(&mut a, &set("/meta", json!({"fresh": true})), &mut [&mut b]); // replaces the old subtree => garbage nodes
        assert_eq!(a.tombstones(), 2);
        let nodes_before = a.node_count();
        // peer b is behind a hypothetical third peer: not quiescent
        let behind = VClock::new();
        let st = a.compact(&[b.clock().clone(), behind]);
        assert!(!st.purged && st.tombstones_purged == 0 && a.tombstones() == 2);
        // everybody has everything
        let before = a.json();
        let st = a.compact(&[b.clock().clone()]);
        assert!(st.purged && st.tombstones_purged == 2 && st.log_dropped >= 3, "{st:?}");
        assert!(a.node_count() < nodes_before && st.nodes_collected > 0);
        assert_eq!(a.json(), before);
        b.compact(&[a.clock().clone()]);
        assert_eq!(a.doc(), b.doc(), "both compacted at the same cut stay identical");
        // and the session goes on
        let e = a.local_op(&ins("/layers/0", json!({"name": "after"}))).unwrap().unwrap();
        b.receive(e).unwrap();
        let e = b.local_op(&set("/layers/0/name", json!("after2"))).unwrap().unwrap();
        a.receive(e).unwrap();
        assert_eq!(a.doc(), b.doc());
        // a peer behind the compacted log is told to fetch a snapshot
        assert!(a.missing_for(&VClock::new()).is_err());
        assert!(a.missing_for(a.clock()).unwrap().is_empty());
    }

    #[test]
    fn missing_for_returns_exactly_what_a_lagging_peer_needs() {
        let (mut a, mut b) = pair();
        let e1 = a.local_op(&set("/frames", json!(1))).unwrap().unwrap();
        let _e2 = a.local_op(&set("/frames", json!(2))).unwrap().unwrap();
        b.receive(e1).unwrap();
        let m = a.missing_for(b.clock()).unwrap();
        assert_eq!(m.len(), 1);
        for e in m {
            b.receive(e).unwrap();
        }
        assert_eq!(a.doc(), b.doc());
    }

    #[test]
    fn fork_as_makes_an_independent_proxy_with_its_own_identity() {
        let (mut a, _) = pair();
        a.local_op(&set("/frames", json!(5))).unwrap();
        let mut p = a.fork_as(7).unwrap();
        let e = p.local_op(&set("/frames", json!(6))).unwrap().unwrap();
        assert_eq!(e.replica, 7);
        assert_eq!(e.seq, 1);
        a.receive(e).unwrap();
        assert_eq!(a.json()["frames"], json!(6));
    }

    #[test]
    fn presence_expires_sanitizes_and_ignores_stale_updates() {
        let mut b = PresenceBoard::new(1000);
        let mut p = Presence::new(3, "  Ana\u{7}  ");
        assert_eq!(p.name, "Ana");
        p.n = 5;
        p.frame = Some(12.0);
        p.selection = vec!["arm".into(); 100];
        p.colour = "red; evil".into();
        assert!(b.update(p.clone(), 100));
        let got = b.get(3).unwrap();
        assert_eq!(got.selection.len(), 64);
        assert_eq!(got.colour, colour_for(3));
        let mut old = p.clone();
        old.n = 4;
        old.frame = Some(99.0);
        assert!(!b.update(old, 200), "older update is ignored");
        assert_eq!(b.get(3).unwrap().frame, Some(12.0));
        let mut nan = p.clone();
        nan.n = 6;
        nan.frame = Some(f32::NAN);
        nan.cursor = Some([f32::INFINITY, 0.0]);
        nan.name = "x".repeat(500);
        assert!(b.update(nan, 300));
        assert_eq!(b.get(3).unwrap().frame, None);
        assert_eq!(b.get(3).unwrap().cursor, None);
        assert_eq!(b.get(3).unwrap().name.chars().count(), 40);
        assert!(b.update(Presence::new(4, ""), 900));
        assert_eq!(b.get(4).unwrap().name, "user4");
        assert!(b.expire(1300).is_empty(), "300 + 1000 not yet passed; 900 + 1000 neither");
        assert_eq!(b.expire(1301), vec![3]);
        b.touch(4, 1500);
        assert!(b.expire(2400).is_empty());
        assert_eq!(b.expire(2501), vec![4]);
        assert!(b.is_empty());
        assert!(!b.remove(4));
    }

    #[test]
    fn simulation_rejects_silly_sizes() {
        assert!(simulate(&base(), &SimConfig::new(0, 10, 1)).is_err());
        assert!(simulate(&base(), &SimConfig::new(65, 10, 1)).is_err());
        let one = simulate(&base(), &SimConfig::new(1, 30, 1)).unwrap();
        assert!(one.converged);
    }

    #[test]
    fn demo_project_converges_and_the_result_is_a_project_when_edits_are_numeric() {
        let p = crate::demo::project();
        let doc = serde_json::to_value(&p).unwrap();
        let rep = simulate(&doc, &SimConfig::new(3, 80, 77)).unwrap();
        assert!(rep.converged && rep.state_identical);
    }
    /// Project-aware simulation keeps the merged document a valid project (ok can be true on a real project).
    #[test]
    fn project_aware_edits_keep_the_demo_valid() {
        let base = serde_json::to_value(crate::demo::project()).unwrap();
        for seed in 1..=24u64 {
            let mut cfg = SimConfig::new(4, 250, seed);
            cfg.project_aware = true;
            let rep = simulate(&base, &cfg).unwrap();
            assert!(rep.converged && rep.state_identical && rep.pending_left == 0, "seed {seed}");
            let real = crate::demo::project().layers.len();
            assert_eq!(rep.final_doc["layers"].as_array().unwrap().iter().filter(|l| !l["name"].as_str().unwrap().starts_with("sim_")).count(), real, "seed {seed}: a real layer was lost");
            let pr: crate::project::Project = serde_json::from_value(rep.final_doc.clone()).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            if let Err(e) = crate::Engine::new(pr) {
                panic!("seed {seed}: merged project does not compile: {e:?}");
            }
        }
    }

}
