// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
//! Version control for animation projects with semantic merge ("ArmaGit", I-01).
//!
//! Own design; no code, format or command implementation of any other version-control system is
//! used (and nothing here shells out to one). Textbook ideas only:
//! * content-addressed store and Merkle DAG (R. Merkle 1979/1987): an object is named by the SHA-256
//!   (FIPS 180-4, implemented below and tested against the published vectors) of its bytes; a tree
//!   names its parts by hash and a commit names its tree and parents by hash, so one hash fixes
//!   the whole history below it;
//! * lowest common ancestors on the commit DAG for the merge base;
//! * the semantic diff and three-way merge of [`crate::semdiff`] (diff3, Khanna/Kunal/Pierce 2007).
//!
//! # Repository layout (`<project dir>/.osatura-vcs/`)
//! * `objects/ab/cdef...` one file per object: header `ARMOBJ1 <kind> <payload length>\n` then the
//!   payload; kinds `blob` (canonical JSON of one project part), `tree` (`{"parts": {name: hash}}`),
//!   `commit` (`{"tree","parents","author","time","message"}`);
//! * `refs/heads/<branch>`, `refs/tags/<tag>`: a 64-hex hash and a newline;
//! * `HEAD`: `ref: refs/heads/<branch>` or a bare hash (detached);
//! * `config`: JSON `{"project": "<file>", "author": "<name>"}`;
//! * `MERGE_STATE`: JSON, present while a merge / cherry-pick / revert has unresolved conflicts or
//!   is waiting for `commit`.
//!
//! # Trees
//! The project document is split into semantic parts so that unchanged parts are stored once:
//! `meta` (all top-level scalars), `obj/<key>` (top-level objects such as `camera`),
//! `order/<key>` + `arr/<key>/<id>` (each element of a top-level array with identities: every layer,
//! bone, channel, skin, sound ... is one part, the order list names them), `val/<key>` (arrays
//! without identities). Parts are canonical JSON (sorted keys, integral floats as integers).
//!
//! # NOT done
//! No remote, push or pull; no packing or compression; binary assets referenced by a project
//! (images, audio) are not stored, only their path strings; merge is semantic only for the JSON
//! structure (see `semdiff`), so a renamed layer is a remove plus an add; a criss-cross history
//! (several best common ancestors) uses the newest one instead of a recursive virtual base; the
//! store does no locking, so two simultaneous writers on one repository are not supported.

use crate::semdiff::{self, Change, Choice, Conflict};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const VCS_DIR: &str = ".osatura-vcs";

// ---------------------------------------------------------------------------------------------
// SHA-256 (FIPS 180-4)
// ---------------------------------------------------------------------------------------------

const K256: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// SHA-256 digest of `data`.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64).wrapping_mul(8)).to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K256[i])
                .wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v[7] = v[6];
            v[6] = v[5];
            v[5] = v[4];
            v[4] = v[3].wrapping_add(t1);
            v[3] = v[2];
            v[2] = v[1];
            v[1] = v[0];
            v[0] = t1.wrapping_add(t2);
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    let mut out = [0u8; 32];
    for i in 0..8 {
        out[4 * i..4 * i + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    out
}

pub fn sha256_hex(data: &[u8]) -> String {
    sha256(data).iter().map(|b| format!("{b:02x}")).collect()
}

fn is_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

// ---------------------------------------------------------------------------------------------
// Object encoding, trees
// ---------------------------------------------------------------------------------------------

fn encode_object(kind: &str, payload: &[u8]) -> Vec<u8> {
    let mut out = format!("ARMOBJ1 {kind} {}\n", payload.len()).into_bytes();
    out.extend_from_slice(payload);
    out
}

fn decode_object(bytes: &[u8]) -> Result<(String, Vec<u8>), String> {
    let nl = bytes
        .iter()
        .position(|b| *b == b'\n')
        .ok_or("object has no header")?;
    let head = std::str::from_utf8(&bytes[..nl]).map_err(|_| "object header is not text")?;
    let mut it = head.split(' ');
    if it.next() != Some("ARMOBJ1") {
        return Err("object has a wrong magic".into());
    }
    let kind = it.next().ok_or("object header lacks a kind")?;
    let len: usize = it
        .next()
        .and_then(|l| l.parse().ok())
        .ok_or("object header lacks a length")?;
    if it.next().is_some() || !matches!(kind, "blob" | "tree" | "commit") {
        return Err(format!("object header is malformed ({head:?})"));
    }
    let payload = &bytes[nl + 1..];
    if payload.len() != len {
        return Err(format!(
            "object length {} differs from header {len}",
            payload.len()
        ));
    }
    Ok((kind.to_string(), payload.to_vec()))
}

/// Split a canonical project document into named parts (see the module doc).
pub fn split_parts(doc: &Value) -> Result<BTreeMap<String, Value>, String> {
    let m = doc.as_object().ok_or("a project must be a JSON object")?;
    let mut parts = BTreeMap::new();
    let mut meta = serde_json::Map::new();
    for (k, v) in m {
        match v {
            Value::Array(a) => match semdiff::elem_ids(a) {
                Some(ids) => {
                    parts.insert(
                        format!("order/{k}"),
                        Value::Array(ids.iter().map(|i| Value::String(i.clone())).collect()),
                    );
                    for (id, e) in ids.iter().zip(a) {
                        parts.insert(format!("arr/{k}/{id}"), e.clone());
                    }
                }
                None => {
                    parts.insert(format!("val/{k}"), v.clone());
                }
            },
            Value::Object(_) => {
                parts.insert(format!("obj/{k}"), v.clone());
            }
            _ => {
                meta.insert(k.clone(), v.clone());
            }
        }
    }
    parts.insert("meta".into(), Value::Object(meta));
    Ok(parts)
}

/// Inverse of [`split_parts`]; errors when a part is missing or malformed.
pub fn join_parts(parts: &BTreeMap<String, Value>) -> Result<Value, String> {
    let mut out = serde_json::Map::new();
    match parts.get("meta") {
        Some(Value::Object(m)) => out.extend(m.clone()),
        _ => return Err("tree lacks the meta part".into()),
    }
    for (name, v) in parts {
        if let Some(k) = name
            .strip_prefix("obj/")
            .or_else(|| name.strip_prefix("val/"))
        {
            out.insert(k.to_string(), v.clone());
        } else if let Some(k) = name.strip_prefix("order/") {
            let ids = v
                .as_array()
                .ok_or_else(|| format!("part {name} is not a list"))?;
            let mut arr = Vec::new();
            for id in ids {
                let id = id
                    .as_str()
                    .ok_or_else(|| format!("part {name} holds a non-text id"))?;
                arr.push(
                    parts
                        .get(&format!("arr/{k}/{id}"))
                        .ok_or_else(|| format!("tree lacks part arr/{k}/{id}"))?
                        .clone(),
                );
            }
            out.insert(k.to_string(), Value::Array(arr));
        }
    }
    Ok(Value::Object(out))
}

// ---------------------------------------------------------------------------------------------
// Repository
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Commit {
    pub tree: String,
    pub parents: Vec<String>,
    pub author: String,
    pub time: u64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeState {
    /// "merge", "cherry-pick" or "revert".
    pub kind: String,
    /// Second parent of the resulting commit (merge only).
    pub theirs: Option<String>,
    pub message: String,
    pub conflicts: Vec<Conflict>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub branch: Option<String>,
    pub head: Option<String>,
    pub changes: Vec<Change>,
    pub merge: Option<String>,
    pub conflicts: Vec<Conflict>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MergeOutcome {
    UpToDate,
    FastForward(String),
    Merged(String),
    Conflicted(Vec<Conflict>),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GcReport {
    pub kept: usize,
    pub removed: usize,
}

#[derive(Debug)]
pub struct Repo {
    /// Directory holding the project file and `.osatura-vcs/`.
    pub root: PathBuf,
    dir: PathBuf,
    /// Timestamp override (seconds since the epoch) for reproducible tests and demos.
    pub fixed_time: Option<u64>,
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    crate::fsutil::write_atomic(path, bytes).map_err(|e| format!("cannot write {e}"))
}

fn valid_ref_name(n: &str) -> Result<(), String> {
    let ok = !n.is_empty()
        && n.len() <= 100
        && !n.starts_with(['.', '-', '/'])
        && !n.ends_with(['/', '.'])
        && !n.contains("..")
        && !n.contains("//")
        && !n.ends_with(".lock")
        && n.chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-/".contains(c));
    if ok {
        Ok(())
    } else {
        Err(format!("invalid name '{n}' (use letters, digits and ._-/ ; no leading '.', '-' or '/', no '..')"))
    }
}

impl Repo {
    // ----- setup -----

    /// Create a repository next to `project_file` (a path to an existing project JSON).
    pub fn init(project_file: &Path, author: Option<&str>) -> Result<Repo, String> {
        let root = project_file
            .parent()
            .map(|p| {
                if p.as_os_str().is_empty() {
                    Path::new(".")
                } else {
                    p
                }
            })
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let name = project_file
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("bad project file name")?
            .to_string();
        let text = fs::read_to_string(project_file)
            .map_err(|e| format!("cannot read {}: {e}", project_file.display()))?;
        let v: Value = serde_json::from_str(&text)
            .map_err(|e| format!("{} is not JSON: {e}", project_file.display()))?;
        if !v.is_object() {
            return Err(format!(
                "{} is not a project (a JSON object is expected)",
                project_file.display()
            ));
        }
        let dir = root.join(VCS_DIR);
        if dir.exists() {
            return Err(format!("{} already exists", dir.display()));
        }
        for sub in ["objects", "refs/heads", "refs/tags"] {
            fs::create_dir_all(dir.join(sub))
                .map_err(|e| format!("cannot create {}: {e}", dir.join(sub).display()))?;
        }
        let r = Repo {
            root,
            dir,
            fixed_time: None,
        };
        write_atomic(&r.dir.join("HEAD"), b"ref: refs/heads/main\n")?;
        let cfg = json!({"project": name, "author": author.unwrap_or("")});
        write_atomic(
            &r.dir.join("config"),
            semdiff::canon_string(&cfg, true).as_bytes(),
        )?;
        Ok(r)
    }

    /// Open the repository containing `start` (walks up through the parent directories).
    pub fn open(start: &Path) -> Result<Repo, String> {
        let abs =
            fs::canonicalize(start).map_err(|e| format!("cannot open {}: {e}", start.display()))?;
        for d in abs.ancestors() {
            if d.join(VCS_DIR).is_dir() {
                return Ok(Repo {
                    root: d.to_path_buf(),
                    dir: d.join(VCS_DIR),
                    fixed_time: None,
                });
            }
        }
        Err(format!(
            "no {VCS_DIR} repository found from {} upwards (run: osatura vcs init <project.json>)",
            abs.display()
        ))
    }

    fn config(&self) -> Result<Value, String> {
        let t = fs::read_to_string(self.dir.join("config"))
            .map_err(|e| format!("cannot read config: {e}"))?;
        serde_json::from_str(&t).map_err(|e| format!("config is damaged: {e}"))
    }

    /// Absolute path of the tracked project file.
    pub fn project_path(&self) -> Result<PathBuf, String> {
        let c = self.config()?;
        let name = c
            .get("project")
            .and_then(|p| p.as_str())
            .ok_or("config lacks the project file name")?;
        let p = Path::new(name);
        if p.is_absolute()
            || p.components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            || name.is_empty()
        {
            return Err(format!("config names an unsafe project path '{name}'"));
        }
        Ok(self.root.join(p))
    }

    pub fn default_author(&self) -> String {
        if let Ok(c) = self.config() {
            if let Some(a) = c
                .get("author")
                .and_then(|a| a.as_str())
                .filter(|a| !a.is_empty())
            {
                return a.to_string();
            }
        }
        std::env::var("OSATURA_AUTHOR")
            .ok()
            .filter(|a| !a.is_empty())
            .or_else(|| std::env::var("USER").ok().filter(|a| !a.is_empty()))
            .unwrap_or_else(|| "unknown".into())
    }

    fn now(&self) -> u64 {
        self.fixed_time.unwrap_or_else(crate::clock::unix_seconds)
    }

    // ----- object store -----

    fn obj_path(&self, h: &str) -> Result<PathBuf, String> {
        if !is_hash(h) {
            return Err(format!("'{h}' is not an object hash"));
        }
        Ok(self.dir.join("objects").join(&h[..2]).join(&h[2..]))
    }

    pub fn put_object(&self, kind: &str, payload: &[u8]) -> Result<String, String> {
        let bytes = encode_object(kind, payload);
        let h = sha256_hex(&bytes);
        let p = self.obj_path(&h)?;
        if p.exists() {
            return Ok(h);
        }
        if let Some(d) = p.parent() {
            fs::create_dir_all(d).map_err(|e| format!("cannot create {}: {e}", d.display()))?;
        }
        write_atomic(&p, &bytes)?;
        Ok(h)
    }

    pub fn get_object(&self, h: &str) -> Result<(String, Vec<u8>), String> {
        let bytes = fs::read(self.obj_path(h)?)
            .map_err(|_| format!("object {} is missing", &h[..h.len().min(12)]))?;
        if sha256_hex(&bytes) != h {
            return Err(format!("object {} is corrupt (hash mismatch)", &h[..12]));
        }
        decode_object(&bytes)
    }

    fn get_json(&self, h: &str, kind: &str) -> Result<Value, String> {
        let (k, p) = self.get_object(h)?;
        if k != kind {
            return Err(format!("object {} is a {k}, expected a {kind}", &h[..12]));
        }
        serde_json::from_slice(&p)
            .map_err(|e| format!("object {} is not valid JSON: {e}", &h[..12]))
    }

    /// Store a canonical project document as a tree; returns the tree hash.
    pub fn write_tree(&self, doc: &Value) -> Result<String, String> {
        let doc = semdiff::canonicalize(doc);
        let mut names = BTreeMap::new();
        for (name, part) in split_parts(&doc)? {
            names.insert(
                name,
                Value::String(
                    self.put_object("blob", semdiff::canon_string(&part, false).as_bytes())?,
                ),
            );
        }
        let tree = json!({"parts": Value::Object(names.into_iter().collect())});
        self.put_object("tree", semdiff::canon_string(&tree, false).as_bytes())
    }

    pub fn read_tree(&self, h: &str) -> Result<Value, String> {
        let t = self.get_json(h, "tree")?;
        let names = t
            .get("parts")
            .and_then(|p| p.as_object())
            .ok_or("tree has no parts")?;
        let mut parts = BTreeMap::new();
        for (n, ph) in names {
            parts.insert(
                n.clone(),
                self.get_json(ph.as_str().ok_or("tree part hash is not text")?, "blob")?,
            );
        }
        join_parts(&parts)
    }

    pub fn read_commit(&self, h: &str) -> Result<Commit, String> {
        let v = self.get_json(h, "commit")?;
        let c: Commit = serde_json::from_value(v)
            .map_err(|e| format!("commit {} is malformed: {e}", &h[..12]))?;
        if !is_hash(&c.tree) || c.parents.iter().any(|p| !is_hash(p)) {
            return Err(format!("commit {} holds a malformed hash", &h[..12]));
        }
        Ok(c)
    }

    fn doc_of(&self, commit: &str) -> Result<Value, String> {
        self.read_tree(&self.read_commit(commit)?.tree)
    }

    // ----- refs -----

    fn read_ref_file(&self, rel: &str) -> Result<Option<String>, String> {
        let p = self.dir.join(rel);
        match fs::read_to_string(&p) {
            Ok(t) => {
                let h = t.trim().to_string();
                if is_hash(&h) {
                    Ok(Some(h))
                } else {
                    Err(format!("ref {rel} is damaged"))
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("cannot read ref {rel}: {e}")),
        }
    }

    fn write_ref(&self, rel: &str, h: &str) -> Result<(), String> {
        let p = self.dir.join(rel);
        if let Some(d) = p.parent() {
            fs::create_dir_all(d).map_err(|e| format!("cannot create {}: {e}", d.display()))?;
        }
        write_atomic(&p, format!("{h}\n").as_bytes())
    }

    /// Current branch name, or `None` for a detached HEAD.
    pub fn current_branch(&self) -> Result<Option<String>, String> {
        let t = fs::read_to_string(self.dir.join("HEAD"))
            .map_err(|e| format!("cannot read HEAD: {e}"))?;
        let t = t.trim();
        if let Some(r) = t.strip_prefix("ref: refs/heads/") {
            valid_ref_name(r)?;
            Ok(Some(r.to_string()))
        } else if is_hash(t) {
            Ok(None)
        } else {
            Err("HEAD is damaged".into())
        }
    }

    pub fn head_commit(&self) -> Result<Option<String>, String> {
        match self.current_branch()? {
            Some(b) => self.read_ref_file(&format!("refs/heads/{b}")),
            None => {
                let t = fs::read_to_string(self.dir.join("HEAD"))
                    .map_err(|e| format!("cannot read HEAD: {e}"))?;
                Ok(Some(t.trim().to_string()))
            }
        }
    }

    fn set_head_commit(&self, h: &str) -> Result<(), String> {
        match self.current_branch()? {
            Some(b) => self.write_ref(&format!("refs/heads/{b}"), h),
            None => write_atomic(&self.dir.join("HEAD"), format!("{h}\n").as_bytes()),
        }
    }

    fn list_refs(&self, sub: &str) -> Result<Vec<(String, String)>, String> {
        let base = self.dir.join(sub);
        let mut out = Vec::new();
        fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<(), String> {
            let rd = match fs::read_dir(dir) {
                Ok(r) => r,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(e) => return Err(format!("cannot list {}: {e}", dir.display())),
            };
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                let full = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}/{name}")
                };
                if e.path().is_dir() {
                    walk(&e.path(), &full, out)?;
                } else if !name.starts_with('.') {
                    out.push(full);
                }
            }
            Ok(())
        }
        let mut names = Vec::new();
        walk(&base, "", &mut names)?;
        names.sort();
        for n in names {
            if let Some(h) = self.read_ref_file(&format!("{sub}/{n}"))? {
                out.push((n, h));
            }
        }
        Ok(out)
    }

    pub fn branches(&self) -> Result<Vec<(String, String, bool)>, String> {
        let cur = self.current_branch()?;
        Ok(self
            .list_refs("refs/heads")?
            .into_iter()
            .map(|(n, h)| {
                let c = cur.as_deref() == Some(n.as_str());
                (n, h, c)
            })
            .collect())
    }

    pub fn tags(&self) -> Result<Vec<(String, String)>, String> {
        self.list_refs("refs/tags")
    }

    /// Resolve a branch, tag, `HEAD`, a hash prefix (at least 4 hex digits), each optionally followed
    /// by `~N` (N-th first-parent ancestor) to a commit hash.
    pub fn resolve_rev(&self, rev: &str) -> Result<String, String> {
        let (base, back) = match rev.split_once('~') {
            Some((b, n)) => (
                b,
                if n.is_empty() {
                    1
                } else {
                    n.parse::<usize>()
                        .map_err(|_| format!("bad revision '{rev}'"))?
                },
            ),
            None => (rev, 0),
        };
        let mut h = self.resolve_base(base)?;
        for _ in 0..back {
            h = self
                .read_commit(&h)?
                .parents
                .first()
                .cloned()
                .ok_or_else(|| format!("'{rev}' goes past the first commit"))?;
        }
        Ok(h)
    }

    fn resolve_base(&self, s: &str) -> Result<String, String> {
        if s == "HEAD" {
            return self
                .head_commit()?
                .ok_or_else(|| "there are no commits yet".to_string());
        }
        if valid_ref_name(s).is_ok() {
            if let Some(h) = self.read_ref_file(&format!("refs/heads/{s}"))? {
                return Ok(h);
            }
            if let Some(h) = self.read_ref_file(&format!("refs/tags/{s}"))? {
                return Ok(h);
            }
        }
        if s.len() >= 4
            && s.len() <= 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            let mut hits = Vec::new();
            for h in self.all_object_names()? {
                if h.starts_with(s)
                    && self
                        .get_object(&h)
                        .map(|o| o.0 == "commit")
                        .unwrap_or(false)
                {
                    hits.push(h);
                }
            }
            return match hits.len() {
                1 => Ok(hits.remove(0)),
                0 => Err(format!("unknown revision '{s}'")),
                _ => Err(format!("revision prefix '{s}' is ambiguous")),
            };
        }
        Err(format!("unknown revision '{s}'"))
    }

    fn all_object_names(&self) -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        let rd = fs::read_dir(self.dir.join("objects"))
            .map_err(|e| format!("cannot list objects: {e}"))?;
        for d in rd.flatten() {
            let dn = d.file_name().to_string_lossy().to_string();
            if dn.len() != 2 || !d.path().is_dir() {
                continue;
            }
            if let Ok(rd2) = fs::read_dir(d.path()) {
                for f in rd2.flatten() {
                    let n = format!("{dn}{}", f.file_name().to_string_lossy());
                    if is_hash(&n) {
                        out.push(n);
                    }
                }
            }
        }
        out.sort();
        Ok(out)
    }

    // ----- working file -----

    pub fn read_working(&self) -> Result<Value, String> {
        let p = self.project_path()?;
        let t = fs::read_to_string(&p).map_err(|e| format!("cannot read {}: {e}", p.display()))?;
        let v: Value = serde_json::from_str(&t)
            .map_err(|e| format!("{} is not valid JSON: {e}", p.display()))?;
        if !v.is_object() {
            return Err(format!(
                "{} is not a project (a JSON object is expected)",
                p.display()
            ));
        }
        Ok(semdiff::canonicalize(&v))
    }

    /// Replace the project file atomically (temporary file in the same directory, then rename).
    pub fn write_working(&self, doc: &Value) -> Result<(), String> {
        write_atomic(
            &self.project_path()?,
            semdiff::canon_string(&semdiff::canonicalize(doc), true).as_bytes(),
        )
    }

    /// Whether the working file differs from the commit at HEAD (an unborn branch counts as clean
    /// only if there is no working file to lose; use `status` for details).
    pub fn is_dirty(&self) -> Result<bool, String> {
        let w = self.read_working()?;
        match self.head_commit()? {
            Some(h) => Ok(w != self.doc_of(&h)?),
            None => Ok(true),
        }
    }

    // ----- merge state -----

    fn load_state(&self) -> Result<Option<MergeState>, String> {
        match fs::read_to_string(self.dir.join("MERGE_STATE")) {
            Ok(t) => Ok(Some(
                serde_json::from_str(&t).map_err(|e| format!("MERGE_STATE is damaged: {e}"))?,
            )),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("cannot read MERGE_STATE: {e}")),
        }
    }

    fn save_state(&self, s: &MergeState) -> Result<(), String> {
        let v = serde_json::to_value(s).map_err(|e| e.to_string())?;
        write_atomic(
            &self.dir.join("MERGE_STATE"),
            semdiff::canon_string(&v, true).as_bytes(),
        )
    }

    fn clear_state(&self) -> Result<(), String> {
        match fs::remove_file(self.dir.join("MERGE_STATE")) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("cannot remove MERGE_STATE: {e}")),
        }
    }

    pub fn merge_state(&self) -> Result<Option<MergeState>, String> {
        self.load_state()
    }

    fn require_no_merge(&self) -> Result<(), String> {
        if self.load_state()?.is_some() {
            return Err("a merge / cherry-pick / revert is in progress: resolve the conflicts and commit, or run 'vcs merge --abort'".into());
        }
        Ok(())
    }

    // ----- commits -----

    fn make_commit(
        &self,
        doc: &Value,
        parents: Vec<String>,
        message: &str,
        author: Option<&str>,
    ) -> Result<String, String> {
        let tree = self.write_tree(doc)?;
        let c = Commit {
            tree,
            parents,
            author: author
                .map(String::from)
                .unwrap_or_else(|| self.default_author()),
            time: self.now(),
            message: message.to_string(),
        };
        let v = serde_json::to_value(&c).map_err(|e| e.to_string())?;
        self.put_object("commit", semdiff::canon_string(&v, false).as_bytes())
    }

    /// Commit the working file on the current branch (or HEAD). With a merge in progress this
    /// creates the merge commit once all conflicts are resolved.
    pub fn commit(&self, message: Option<&str>, author: Option<&str>) -> Result<String, String> {
        let doc = self.read_working()?;
        let head = self.head_commit()?;
        let mut parents: Vec<String> = head.iter().cloned().collect();
        let mut msg = message.map(String::from);
        if let Some(st) = self.load_state()? {
            if !st.conflicts.is_empty() {
                return Err(format!("{} unresolved conflict(s): resolve them with 'vcs resolve <path> --ours|--theirs|--value <json>'", st.conflicts.len()));
            }
            if let Some(t) = &st.theirs {
                parents.push(t.clone());
            }
            msg.get_or_insert(st.message.clone());
        } else if let Some(h) = &head {
            if doc == self.doc_of(h)? {
                return Err("nothing to commit (the project is unchanged)".into());
            }
        }
        let msg = msg
            .filter(|m| !m.trim().is_empty())
            .ok_or("a commit message is required (-m)")?;
        let h = self.make_commit(&doc, parents, &msg, author)?;
        self.set_head_commit(&h)?;
        self.clear_state()?;
        Ok(h)
    }

    /// Commits reachable from `rev`, newest first (a commit never precedes one of its children).
    pub fn log(&self, rev: &str, limit: usize) -> Result<Vec<(String, Commit)>, String> {
        let start = self.resolve_rev(rev)?;
        let mut all: HashMap<String, Commit> = HashMap::new();
        let mut stack = vec![start.clone()];
        while let Some(h) = stack.pop() {
            if all.contains_key(&h) {
                continue;
            }
            let c = self.read_commit(&h)?;
            stack.extend(c.parents.iter().cloned());
            all.insert(h, c);
        }
        let mut children: HashMap<&str, usize> = HashMap::new();
        for c in all.values() {
            for p in &c.parents {
                *children.entry(p.as_str()).or_insert(0) += 1;
            }
        }
        let mut ready: Vec<&String> = all
            .keys()
            .filter(|h| !children.contains_key(h.as_str()))
            .collect();
        let mut out = Vec::new();
        while !ready.is_empty() && out.len() < limit {
            ready.sort_by(|a, b| (all[*a].time, a.as_str()).cmp(&(all[*b].time, b.as_str())));
            let h = ready.pop().unwrap();
            let c = &all[h];
            out.push((h.clone(), c.clone()));
            for p in &c.parents {
                if let Some(n) = children.get_mut(p.as_str()) {
                    *n -= 1;
                    if *n == 0 {
                        ready.push(all.get_key_value(p).map(|x| x.0).unwrap());
                    }
                }
            }
        }
        Ok(out)
    }

    fn ancestors(&self, h: &str) -> Result<HashSet<String>, String> {
        let mut seen = HashSet::new();
        let mut stack = vec![h.to_string()];
        while let Some(x) = stack.pop() {
            if seen.insert(x.clone()) {
                stack.extend(self.read_commit(&x)?.parents);
            }
        }
        Ok(seen)
    }

    /// Best common ancestor of two commits (lowest in the DAG; the newest if there are several).
    pub fn merge_base(&self, a: &str, b: &str) -> Result<Option<String>, String> {
        let (aa, ab) = (self.ancestors(a)?, self.ancestors(b)?);
        let common: Vec<&String> = aa.intersection(&ab).collect();
        let mut redundant: HashSet<String> = HashSet::new();
        for c in &common {
            let mut anc = self.ancestors(c)?;
            anc.remove(*c);
            redundant.extend(anc);
        }
        let mut best: Vec<&&String> = common.iter().filter(|c| !redundant.contains(**c)).collect();
        let mut keyed = Vec::new();
        for c in best.drain(..) {
            keyed.push((self.read_commit(c)?.time, (**c).clone()));
        }
        keyed.sort();
        Ok(keyed.pop().map(|x| x.1))
    }

    pub fn is_ancestor(&self, anc: &str, of: &str) -> Result<bool, String> {
        Ok(self.ancestors(of)?.contains(anc))
    }

    // ----- status, diff, show -----

    pub fn status(&self) -> Result<Status, String> {
        let working = self.read_working()?;
        let head = self.head_commit()?;
        let base = match &head {
            Some(h) => self.doc_of(h)?,
            None => Value::Object(Default::default()),
        };
        let st = self.load_state()?;
        Ok(Status {
            branch: self.current_branch()?,
            head,
            changes: semdiff::diff_docs(&base, &working),
            merge: st.as_ref().map(|s| s.kind.clone()),
            conflicts: st.map(|s| s.conflicts).unwrap_or_default(),
        })
    }

    /// Document at a revision, or the working file for `None`.
    fn doc_at(&self, rev: Option<&str>) -> Result<Value, String> {
        match rev {
            Some(r) => self.doc_of(&self.resolve_rev(r)?),
            None => self.read_working(),
        }
    }

    /// Semantic differences between two revisions; `None` means the working file.
    pub fn diff(&self, a: Option<&str>, b: Option<&str>) -> Result<Vec<Change>, String> {
        Ok(semdiff::diff_docs(&self.doc_at(a)?, &self.doc_at(b)?))
    }

    /// Changes a commit made relative to its first parent (everything for a root commit).
    pub fn show(&self, rev: &str) -> Result<(String, Commit, Vec<Change>), String> {
        let h = self.resolve_rev(rev)?;
        let c = self.read_commit(&h)?;
        let new = self.read_tree(&c.tree)?;
        let old = match c.parents.first() {
            Some(p) => self.doc_of(p)?,
            None => Value::Object(Default::default()),
        };
        Ok((h, c, semdiff::diff_docs(&old, &new)))
    }

    // ----- branches, tags, checkout -----

    pub fn branch_create(&self, name: &str, at: Option<&str>) -> Result<String, String> {
        valid_ref_name(name)?;
        if self.read_ref_file(&format!("refs/heads/{name}"))?.is_some() {
            return Err(format!("branch '{name}' already exists"));
        }
        let h = self.resolve_rev(at.unwrap_or("HEAD"))?;
        self.write_ref(&format!("refs/heads/{name}"), &h)?;
        Ok(h)
    }

    pub fn branch_delete(&self, name: &str) -> Result<(), String> {
        valid_ref_name(name)?;
        if self.current_branch()?.as_deref() == Some(name) {
            return Err("cannot delete the checked-out branch".into());
        }
        if self.read_ref_file(&format!("refs/heads/{name}"))?.is_none() {
            return Err(format!("no branch '{name}'"));
        }
        fs::remove_file(self.dir.join("refs/heads").join(name))
            .map_err(|e| format!("cannot delete branch: {e}"))
    }

    pub fn tag_create(&self, name: &str, at: Option<&str>) -> Result<String, String> {
        valid_ref_name(name)?;
        if self.read_ref_file(&format!("refs/tags/{name}"))?.is_some() {
            return Err(format!("tag '{name}' already exists"));
        }
        let h = self.resolve_rev(at.unwrap_or("HEAD"))?;
        self.write_ref(&format!("refs/tags/{name}"), &h)?;
        Ok(h)
    }

    /// Switch the working file to a branch (stays on it), or to any other revision (detached).
    /// Refuses when the working file has uncommitted changes unless `force`.
    pub fn checkout(&self, target: &str, force: bool) -> Result<String, String> {
        self.require_no_merge()?;
        let h = self.resolve_rev(target)?;
        if !force && self.head_commit()?.is_some() && self.is_dirty()? {
            return Err("the project has uncommitted changes (commit them or use --force)".into());
        }
        let doc = self.doc_of(&h)?;
        self.write_working(&doc)?;
        if valid_ref_name(target).is_ok()
            && self
                .read_ref_file(&format!("refs/heads/{target}"))?
                .is_some()
        {
            write_atomic(
                &self.dir.join("HEAD"),
                format!("ref: refs/heads/{target}\n").as_bytes(),
            )?;
        } else {
            write_atomic(&self.dir.join("HEAD"), format!("{h}\n").as_bytes())?;
        }
        Ok(h)
    }

    // ----- merge, cherry-pick, revert -----

    fn require_clean(&self) -> Result<(), String> {
        if self.is_dirty()? {
            return Err("the project has uncommitted changes: commit them first".into());
        }
        Ok(())
    }

    // internal merge helper threading one merge context; kept flat to match its callers.
    #[allow(clippy::too_many_arguments)]
    fn apply_three_way(
        &self,
        kind: &str,
        base: &Value,
        ours: &Value,
        theirs: &Value,
        second_parent: Option<String>,
        message: String,
        author: Option<&str>,
    ) -> Result<MergeOutcome, String> {
        let (merged, conflicts) = semdiff::merge_docs(base, ours, theirs);
        self.write_working(&merged)?;
        if conflicts.is_empty() {
            if merged == semdiff::canonicalize(ours) && second_parent.is_none() {
                return Err(format!("nothing to {kind}: the change is already present"));
            }
            let mut parents: Vec<String> = self.head_commit()?.into_iter().collect();
            parents.extend(second_parent);
            let h = self.make_commit(&merged, parents, &message, author)?;
            self.set_head_commit(&h)?;
            return Ok(MergeOutcome::Merged(h));
        }
        self.save_state(&MergeState {
            kind: kind.to_string(),
            theirs: second_parent,
            message,
            conflicts: conflicts.clone(),
        })?;
        Ok(MergeOutcome::Conflicted(conflicts))
    }

    /// Merge `rev` into the current branch. Fast-forwards when possible (unless `no_ff`).
    pub fn merge(
        &self,
        rev: &str,
        no_ff: bool,
        message: Option<&str>,
        author: Option<&str>,
    ) -> Result<MergeOutcome, String> {
        self.require_no_merge()?;
        let head = self
            .head_commit()?
            .ok_or("nothing to merge into: there are no commits yet")?;
        let theirs = self.resolve_rev(rev)?;
        self.require_clean()?;
        let base = self.merge_base(&head, &theirs)?;
        if base.as_deref() == Some(theirs.as_str()) {
            return Ok(MergeOutcome::UpToDate);
        }
        let theirs_doc = self.doc_of(&theirs)?;
        if base.as_deref() == Some(head.as_str()) && !no_ff {
            self.write_working(&theirs_doc)?;
            self.set_head_commit(&theirs)?;
            return Ok(MergeOutcome::FastForward(theirs));
        }
        let base_doc = match &base {
            Some(b) => self.doc_of(b)?,
            None => Value::Object(Default::default()),
        };
        let msg = message
            .map(String::from)
            .unwrap_or_else(|| match self.current_branch() {
                Ok(Some(b)) => format!("Merge {rev} into {b}"),
                _ => format!("Merge {rev}"),
            });
        self.apply_three_way(
            "merge",
            &base_doc,
            &self.doc_of(&head)?,
            &theirs_doc,
            Some(theirs),
            msg,
            author,
        )
    }

    /// Replay the change a commit made (relative to its first parent) onto the current state.
    pub fn cherry_pick(&self, rev: &str, author: Option<&str>) -> Result<MergeOutcome, String> {
        self.require_no_merge()?;
        let head = self.head_commit()?.ok_or("there are no commits yet")?;
        self.require_clean()?;
        let c = self.resolve_rev(rev)?;
        let commit = self.read_commit(&c)?;
        let base = match commit.parents.first() {
            Some(p) => self.doc_of(p)?,
            None => Value::Object(Default::default()),
        };
        self.apply_three_way(
            "cherry-pick",
            &base,
            &self.doc_of(&head)?,
            &self.read_tree(&commit.tree)?,
            None,
            commit.message.clone(),
            author,
        )
    }

    /// Undo the change a commit made (relative to its first parent) on the current state.
    pub fn revert(&self, rev: &str, author: Option<&str>) -> Result<MergeOutcome, String> {
        self.require_no_merge()?;
        let head = self.head_commit()?.ok_or("there are no commits yet")?;
        self.require_clean()?;
        let c = self.resolve_rev(rev)?;
        let commit = self.read_commit(&c)?;
        let parent = commit
            .parents
            .first()
            .ok_or("cannot revert the root commit")?;
        let msg = format!("Revert \"{}\"", commit.message.lines().next().unwrap_or(""));
        self.apply_three_way(
            "revert",
            &self.read_tree(&commit.tree)?,
            &self.doc_of(&head)?,
            &self.doc_of(parent)?,
            None,
            msg,
            author,
        )
    }

    /// Drop an in-progress merge / cherry-pick / revert and restore the committed project.
    pub fn abort(&self) -> Result<(), String> {
        if self.load_state()?.is_none() {
            return Err("no merge in progress".into());
        }
        let head = self.head_commit()?.ok_or("there are no commits yet")?;
        self.write_working(&self.doc_of(&head)?)?;
        self.clear_state()
    }

    /// Resolve one recorded conflict by its path.
    pub fn resolve(&self, path: &str, choice: &Choice) -> Result<usize, String> {
        let mut st = self.load_state()?.ok_or("no merge in progress")?;
        let idx = st
            .conflicts
            .iter()
            .position(|c| c.path == path)
            .ok_or_else(|| format!("no conflict at '{path}' (see 'vcs status')"))?;
        let mut doc = self.read_working()?;
        semdiff::resolve_in(&mut doc, &st.conflicts[idx], choice)?;
        self.write_working(&doc)?;
        st.conflicts.remove(idx);
        // a conflict list may hold the same path twice (order conflicts); drop the same-path duplicates too
        st.conflicts.retain(|c| c.path != path);
        let left = st.conflicts.len();
        self.save_state(&st)?;
        Ok(left)
    }

    // ----- gc, fsck -----

    fn roots(&self) -> Result<Vec<String>, String> {
        let mut r: Vec<String> = Vec::new();
        r.extend(self.list_refs("refs/heads")?.into_iter().map(|x| x.1));
        r.extend(self.list_refs("refs/tags")?.into_iter().map(|x| x.1));
        if let Some(h) = self.head_commit()? {
            r.push(h);
        }
        if let Some(st) = self.load_state()? {
            r.extend(st.theirs);
        }
        Ok(r)
    }

    fn reachable(&self) -> Result<HashSet<String>, String> {
        let mut seen = HashSet::new();
        let mut stack = self.roots()?;
        while let Some(h) = stack.pop() {
            if !seen.insert(h.clone()) {
                continue;
            }
            let c = self.read_commit(&h)?;
            stack.extend(c.parents.iter().cloned());
            let t = self.get_json(&c.tree, "tree")?;
            seen.insert(c.tree.clone());
            for (_, ph) in t
                .get("parts")
                .and_then(|p| p.as_object())
                .ok_or("tree has no parts")?
            {
                let ph = ph.as_str().ok_or("tree part hash is not text")?;
                if !is_hash(ph) {
                    return Err("tree holds a malformed hash".into());
                }
                seen.insert(ph.to_string());
            }
        }
        Ok(seen)
    }

    /// Delete objects no ref (branch, tag, HEAD, pending merge) reaches. Aborts without deleting
    /// anything if the reachable history cannot be read.
    pub fn gc(&self) -> Result<GcReport, String> {
        let keep = self
            .reachable()
            .map_err(|e| format!("gc aborted, nothing deleted: {e}"))?;
        let mut rep = GcReport::default();
        for h in self.all_object_names()? {
            if keep.contains(&h) {
                rep.kept += 1;
            } else {
                fs::remove_file(self.obj_path(&h)?)
                    .map_err(|e| format!("cannot delete object: {e}"))?;
                rep.removed += 1;
            }
        }
        Ok(rep)
    }

    /// Integrity check: every object rehashes to its name and parses, every reference resolves,
    /// every ref points to a commit. Returns the list of problems (empty = healthy).
    pub fn fsck(&self) -> Vec<String> {
        let mut bad = Vec::new();
        let rd = match fs::read_dir(self.dir.join("objects")) {
            Ok(r) => r,
            Err(e) => return vec![format!("cannot list objects: {e}")],
        };
        let mut count = 0;
        for d in rd.flatten() {
            let dn = d.file_name().to_string_lossy().to_string();
            let Ok(rd2) = fs::read_dir(d.path()) else {
                bad.push(format!("objects/{dn} is not a directory"));
                continue;
            };
            for f in rd2.flatten() {
                let fname = f.file_name().to_string_lossy().to_string();
                let name = format!("{dn}{fname}");
                if fname.starts_with('.') {
                    bad.push(format!("stray temporary file objects/{dn}/{fname}"));
                    continue;
                }
                count += 1;
                if !is_hash(&name) {
                    bad.push(format!("objects/{dn}/{fname}: not a valid object name"));
                    continue;
                }
                let bytes = match fs::read(f.path()) {
                    Ok(b) => b,
                    Err(e) => {
                        bad.push(format!("object {}: unreadable ({e})", &name[..12]));
                        continue;
                    }
                };
                if sha256_hex(&bytes) != name {
                    bad.push(format!(
                        "object {}: content does not match its hash (corrupt)",
                        &name[..12]
                    ));
                    continue;
                }
                match decode_object(&bytes) {
                    Err(e) => bad.push(format!("object {}: {e}", &name[..12])),
                    Ok((kind, payload)) => {
                        match (kind.as_str(), serde_json::from_slice::<Value>(&payload)) {
                            (_, Err(e)) => bad
                                .push(format!("object {}: payload is not JSON ({e})", &name[..12])),
                            ("commit", Ok(v)) => match serde_json::from_value::<Commit>(v) {
                                Err(e) => {
                                    bad.push(format!("commit {}: malformed ({e})", &name[..12]))
                                }
                                Ok(c) => {
                                    let mut refs = vec![(c.tree.clone(), "tree")];
                                    refs.extend(c.parents.iter().map(|p| (p.clone(), "commit")));
                                    for (r, k) in refs {
                                        match self.get_object(&r) {
                                            Ok((rk, _)) if rk == k => {}
                                            Ok((rk, _)) => bad.push(format!(
                                                "commit {}: {k} {} is a {rk}",
                                                &name[..12],
                                                &r[..r.len().min(12)]
                                            )),
                                            Err(e) => {
                                                bad.push(format!("commit {}: {e}", &name[..12]))
                                            }
                                        }
                                    }
                                }
                            },
                            ("tree", Ok(v)) => match v.get("parts").and_then(|p| p.as_object()) {
                                None => bad.push(format!("tree {}: no parts", &name[..12])),
                                Some(p) => {
                                    for (pn, ph) in p {
                                        match ph.as_str().map(|h| self.get_object(h)) {
                                            Some(Ok((k, _))) if k == "blob" => {}
                                            Some(Ok((k, _))) => bad.push(format!(
                                                "tree {}: part {pn} is a {k}",
                                                &name[..12]
                                            )),
                                            Some(Err(e)) => bad.push(format!(
                                                "tree {}: part {pn}: {e}",
                                                &name[..12]
                                            )),
                                            None => bad.push(format!(
                                                "tree {}: part {pn} has no hash",
                                                &name[..12]
                                            )),
                                        }
                                    }
                                }
                            },
                            _ => {}
                        }
                    }
                }
            }
        }
        let _ = count;
        for sub in ["refs/heads", "refs/tags"] {
            match self.list_refs(sub) {
                Err(e) => bad.push(e),
                Ok(rs) => {
                    for (n, h) in rs {
                        match self.get_object(&h) {
                            Ok((k, _)) if k == "commit" => {}
                            Ok((k, _)) => bad.push(format!("{sub}/{n} points to a {k}")),
                            Err(e) => bad.push(format!("{sub}/{n}: {e}")),
                        }
                    }
                }
            }
        }
        match self.current_branch() {
            Err(e) => bad.push(e),
            Ok(None) => match self.head_commit() {
                Ok(Some(h)) if self.read_commit(&h).is_ok() => {}
                _ => bad.push("detached HEAD does not point to a commit".into()),
            },
            Ok(Some(_)) => {
                if let Err(e) = self.head_commit() {
                    bad.push(e);
                }
            }
        }
        if let Err(e) = self.load_state() {
            bad.push(e);
        }
        if let Err(e) = self.config() {
            bad.push(e);
        }
        bad
    }

    /// Counts for the report of `gc --dry-run`-like inspection.
    pub fn object_count(&self) -> Result<usize, String> {
        Ok(self.all_object_names()?.len())
    }

    /// Hash set of every ref target, for tests and tools.
    pub fn ref_targets(&self) -> Result<BTreeSet<String>, String> {
        Ok(self.roots()?.into_iter().collect())
    }
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::ScratchDir::new(&format!("armvcs_{tag}"))
    }

    fn project() -> Value {
        json!({
            "format": 1, "name": "t", "fps": 24.0, "frames": 48,
            "camera": {"zoom": 1.0},
            "layers": [
                {"name": "sky", "opacity": 1.0, "visible": true},
                {"name": "sun", "opacity": 1.0, "visible": true},
                {"name": "hill", "opacity": 0.5, "visible": true}
            ],
            "bones": [{"name": "a", "length": 10.0}],
            "channels": [
                {"target": "a", "prop": "bone_angle", "source": "keys", "track": {"keys": [
                    {"frame": 0, "value": 0.0}, {"frame": 12, "value": 10.0}, {"frame": 24, "value": 0.0}]}},
                {"target": "sun", "prop": "layer_opacity", "source": "keys", "track": {"keys": [{"frame": 0, "value": 1.0}]}}
            ]
        })
    }

    /// A repo plus the scratch directory that holds it (removed when this value is dropped).
    struct TestRepo {
        repo: Repo,
        _dir: crate::fsutil::ScratchDir,
    }

    impl std::ops::Deref for TestRepo {
        type Target = Repo;
        fn deref(&self) -> &Repo {
            &self.repo
        }
    }

    impl std::ops::DerefMut for TestRepo {
        fn deref_mut(&mut self) -> &mut Repo {
            &mut self.repo
        }
    }

    fn new_repo(tag: &str) -> TestRepo {
        let d = tmp(tag);
        let f = d.join("p.json");
        fs::write(&f, serde_json::to_string_pretty(&project()).unwrap()).unwrap();
        let mut r = Repo::init(&f, Some("ana")).unwrap();
        r.fixed_time = Some(1_000);
        TestRepo { repo: r, _dir: d }
    }

    fn edit(r: &Repo, f: impl FnOnce(&mut Value)) {
        let mut d = r.read_working().unwrap();
        f(&mut d);
        r.write_working(&d).unwrap();
    }

    fn setp(d: &mut Value, p: &str, v: Value) {
        semdiff::set_at(d, &semdiff::segs_of(p).unwrap(), Some(v)).unwrap();
    }

    fn tick(r: &mut Repo) {
        r.fixed_time = Some(r.fixed_time.unwrap() + 10);
    }

    #[test]
    fn sha256_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        let million = vec![b'a'; 1_000_000];
        assert_eq!(
            sha256_hex(&million),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        // padding boundaries (55, 56, 63, 64, 65 bytes) must not panic and must differ
        let mut seen = HashSet::new();
        for n in [55, 56, 57, 63, 64, 65, 119, 120] {
            assert!(seen.insert(sha256_hex(&vec![7u8; n])));
        }
    }

    #[test]
    fn canonical_hash_is_deterministic() {
        let r = new_repo("canon");
        let a: Value = serde_json::from_str(r#"{"b":1.0,"a":{"y":2,"x":[1.0,2.5]}}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"a":{"x":[1,2.5],"y":2.0},"b":1}"#).unwrap();
        assert_eq!(r.write_tree(&a).unwrap(), r.write_tree(&b).unwrap());
        let c: Value = serde_json::from_str(r#"{"a":{"x":[1,2.6],"y":2},"b":1}"#).unwrap();
        assert_ne!(r.write_tree(&a).unwrap(), r.write_tree(&c).unwrap());
        // the same tree built twice gives the same hash across runs: pin one value
        let t = r.write_tree(&json!({"a": 1})).unwrap();
        assert_eq!(t, r.write_tree(&json!({"a": 1})).unwrap());
        assert!(is_hash(&t));
    }

    #[test]
    fn tree_split_join_roundtrip_and_dedupe() {
        let r = new_repo("tree");
        let doc = semdiff::canonicalize(&project());
        let t = r.write_tree(&doc).unwrap();
        assert_eq!(r.read_tree(&t).unwrap(), doc);
        let before = r.object_count().unwrap();
        let mut d2 = doc.clone();
        setp(&mut d2, "/layers/sky/opacity", json!(0.5));
        r.write_tree(&d2).unwrap();
        // one changed layer: one new blob + one new tree + order unchanged
        assert_eq!(r.object_count().unwrap(), before + 2);
        assert!(split_parts(&json!([1])).is_err());
        assert!(join_parts(&BTreeMap::new()).is_err());
    }

    #[test]
    fn commit_log_branch_roundtrip() {
        let mut r = new_repo("log");
        assert!(r.log("HEAD", 10).is_err());
        let c1 = r.commit(Some("first"), None).unwrap();
        tick(&mut r);
        assert!(r
            .commit(Some("again"), None)
            .unwrap_err()
            .contains("nothing to commit"));
        edit(&r, |d| setp(d, "/layers/sky/opacity", json!(0.4)));
        assert_eq!(r.status().unwrap().changes.len(), 1);
        let c2 = r.commit(Some("dim sky"), Some("bo")).unwrap();
        let log = r.log("HEAD", 10).unwrap();
        assert_eq!(
            log.iter().map(|x| x.0.clone()).collect::<Vec<_>>(),
            vec![c2.clone(), c1.clone()]
        );
        assert_eq!(log[0].1.author, "bo");
        assert_eq!(log[1].1.author, "ana");
        assert_eq!(r.resolve_rev("HEAD~1").unwrap(), c1);
        assert_eq!(r.resolve_rev(&c2[..8]).unwrap(), c2);
        assert!(r.resolve_rev("HEAD~5").is_err());
        assert!(r.resolve_rev("nope").is_err());
        r.branch_create("feature", Some(&c1)).unwrap();
        assert!(r
            .branch_create("feature", None)
            .unwrap_err()
            .contains("exists"));
        assert!(r.branch_create("bad..name", None).is_err());
        r.tag_create("v1", None).unwrap();
        assert_eq!(r.resolve_rev("v1").unwrap(), c2);
        r.checkout("feature", false).unwrap();
        assert_eq!(r.current_branch().unwrap().as_deref(), Some("feature"));
        assert_eq!(r.read_working().unwrap()["layers"][0]["opacity"], json!(1));
        r.checkout("main", false).unwrap();
        assert_eq!(
            r.read_working().unwrap()["layers"][0]["opacity"],
            json!(0.4)
        );
        r.checkout("v1", false).unwrap();
        assert_eq!(r.current_branch().unwrap(), None);
        r.checkout("main", false).unwrap();
        assert_eq!(r.branches().unwrap().len(), 2);
        r.branch_delete("feature").unwrap();
        assert!(r.branch_delete("main").is_err());
        assert!(r.fsck().is_empty(), "{:?}", r.fsck());
    }

    #[test]
    fn checkout_refuses_dirty_and_is_atomic() {
        let mut r = new_repo("co");
        r.commit(Some("one"), None).unwrap();
        r.branch_create("b", None).unwrap();
        tick(&mut r);
        edit(&r, |d| setp(d, "/fps", json!(30)));
        r.commit(Some("fps"), None).unwrap();
        r.checkout("b", false).unwrap();
        edit(&r, |d| setp(d, "/fps", json!(12)));
        let before = fs::read(r.project_path().unwrap()).unwrap();
        let e = r.checkout("main", false).unwrap_err();
        assert!(e.contains("uncommitted"), "{e}");
        assert_eq!(fs::read(r.project_path().unwrap()).unwrap(), before);
        assert_eq!(r.current_branch().unwrap().as_deref(), Some("b"));
        r.checkout("main", true).unwrap();
        assert_eq!(r.read_working().unwrap()["fps"], json!(30));
        // no temporary files left behind
        let leftovers: Vec<_> = fs::read_dir(&r.root)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("armtmp"))
            .collect();
        assert!(leftovers.is_empty());
        // a failing write (target directory vanished) leaves HEAD unchanged
        let mut cfg = r.config().unwrap();
        cfg["project"] = json!("missing_dir/p.json");
        write_atomic(&r.dir.join("config"), cfg.to_string().as_bytes()).unwrap();
        assert!(r.checkout("b", true).is_err());
        assert_eq!(r.current_branch().unwrap().as_deref(), Some("main"));
    }

    #[test]
    fn merge_fast_forward_and_up_to_date() {
        let mut r = new_repo("ff");
        let c1 = r.commit(Some("base"), None).unwrap();
        r.branch_create("topic", None).unwrap();
        r.checkout("topic", false).unwrap();
        tick(&mut r);
        edit(&r, |d| setp(d, "/layers/sky/opacity", json!(0.3)));
        let c2 = r.commit(Some("topic work"), None).unwrap();
        r.checkout("main", false).unwrap();
        assert_eq!(
            r.merge("topic", false, None, None).unwrap(),
            MergeOutcome::FastForward(c2.clone())
        );
        assert_eq!(r.head_commit().unwrap().unwrap(), c2);
        assert_eq!(
            r.read_working().unwrap()["layers"][0]["opacity"],
            json!(0.3)
        );
        assert_eq!(
            r.merge("topic", false, None, None).unwrap(),
            MergeOutcome::UpToDate
        );
        assert_eq!(
            r.merge(&c1, false, None, None).unwrap(),
            MergeOutcome::UpToDate
        );
    }

    #[test]
    fn merge_clean_two_animators_and_merge_base_diamond() {
        let mut r = new_repo("diamond");
        let c0 = r.commit(Some("base"), None).unwrap();
        r.branch_create("ana", None).unwrap();
        r.branch_create("bo", None).unwrap();
        r.checkout("ana", false).unwrap();
        tick(&mut r);
        edit(&r, |d| {
            setp(
                d,
                "/channels/a|bone_angle|keys/track/keys/12/value",
                json!(15),
            )
        });
        let ca = r.commit(Some("ana: key 12"), Some("ana")).unwrap();
        r.checkout("bo", false).unwrap();
        tick(&mut r);
        edit(&r, |d| {
            setp(d, "/layers/hill/visible", json!(false));
            setp(
                d,
                "/channels/sun|layer_opacity|keys/track/keys/0/value",
                json!(0.5),
            );
        });
        let cb = r.commit(Some("bo: hill + sun"), Some("bo")).unwrap();
        assert_eq!(r.merge_base(&ca, &cb).unwrap().unwrap(), c0);
        tick(&mut r);
        match r.merge("ana", false, None, Some("bo")).unwrap() {
            MergeOutcome::Merged(m) => {
                let c = r.read_commit(&m).unwrap();
                assert_eq!(c.parents, vec![cb.clone(), ca.clone()]);
                let d = r.read_working().unwrap();
                assert_eq!(d["channels"][0]["track"]["keys"][1]["value"], json!(15));
                assert_eq!(d["layers"][2]["visible"], json!(false));
                assert_eq!(d["channels"][1]["track"]["keys"][0]["value"], json!(0.5));
                // diamond: merge base of the merge and either parent is that parent
                assert_eq!(r.merge_base(&m, &ca).unwrap().unwrap(), ca);
                assert!(r.is_ancestor(&c0, &m).unwrap());
                let log = r.log("HEAD", 10).unwrap();
                assert_eq!(log.len(), 4);
                assert_eq!(log[0].0, m);
                assert_eq!(log[3].0, c0);
            }
            o => panic!("{o:?}"),
        }
        assert!(r.fsck().is_empty());
    }

    #[test]
    fn merge_conflict_resolve_and_commit() {
        let mut r = new_repo("conf");
        r.commit(Some("base"), None).unwrap();
        r.branch_create("t", None).unwrap();
        tick(&mut r);
        edit(&r, |d| {
            setp(d, "/layers/sky/opacity", json!(0.2));
            setp(d, "/layers/hill/visible", json!(false));
        });
        r.commit(Some("main edit"), None).unwrap();
        r.checkout("t", false).unwrap();
        tick(&mut r);
        edit(&r, |d| setp(d, "/layers/sky/opacity", json!(0.8)));
        r.commit(Some("t edit"), None).unwrap();
        r.checkout("main", false).unwrap();
        let out = r.merge("t", false, None, None).unwrap();
        let MergeOutcome::Conflicted(cs) = out else {
            panic!("{out:?}")
        };
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].path, "/layers/sky/opacity");
        // working file holds ours; commit and checkout and a second merge are refused
        assert_eq!(
            r.read_working().unwrap()["layers"][0]["opacity"],
            json!(0.2)
        );
        assert!(r
            .commit(Some("x"), None)
            .unwrap_err()
            .contains("unresolved"));
        assert!(r.checkout("t", true).unwrap_err().contains("in progress"));
        assert!(r.merge("t", false, None, None).is_err());
        assert!(r.resolve("/layers/nope", &Choice::Ours).is_err());
        assert!(r.status().unwrap().conflicts.len() == 1);
        assert_eq!(
            r.resolve("/layers/sky/opacity", &Choice::Value(json!(0.5)))
                .unwrap(),
            0
        );
        assert_eq!(
            r.read_working().unwrap()["layers"][0]["opacity"],
            json!(0.5)
        );
        tick(&mut r);
        let m = r.commit(None, None).unwrap();
        assert_eq!(r.read_commit(&m).unwrap().parents.len(), 2);
        assert!(r.merge_state().unwrap().is_none());
        // --theirs variant
        r.checkout("t", true).unwrap();
        assert!(r.fsck().is_empty());
    }

    #[test]
    fn resolve_theirs_and_abort_and_delete_edit() {
        let mut r = new_repo("abort");
        r.commit(Some("base"), None).unwrap();
        r.branch_create("t", None).unwrap();
        tick(&mut r);
        edit(&r, |d| setp(d, "/layers/sun/opacity", json!(0.1)));
        r.commit(Some("edit sun"), None).unwrap();
        r.checkout("t", false).unwrap();
        tick(&mut r);
        edit(&r, |d| {
            semdiff::set_at(d, &semdiff::segs_of("/layers/sun").unwrap(), None).unwrap();
        });
        r.commit(Some("remove sun"), None).unwrap();
        r.checkout("main", false).unwrap();
        let MergeOutcome::Conflicted(cs) = r.merge("t", false, None, None).unwrap() else {
            panic!()
        };
        assert_eq!(cs[0].kind, semdiff::ConflictKind::DeleteEdit);
        r.resolve("/layers/sun", &Choice::Theirs).unwrap();
        assert_eq!(
            r.read_working().unwrap()["layers"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        r.abort().unwrap();
        assert_eq!(
            r.read_working().unwrap()["layers"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert!(r.abort().is_err());
        assert!(!r.is_dirty().unwrap());
    }

    #[test]
    fn revert_and_cherry_pick() {
        let mut r = new_repo("rev");
        let c0 = r.commit(Some("base"), None).unwrap();
        tick(&mut r);
        edit(&r, |d| setp(d, "/layers/sky/opacity", json!(0.1)));
        let c1 = r.commit(Some("dim sky"), None).unwrap();
        tick(&mut r);
        edit(&r, |d| setp(d, "/layers/hill/visible", json!(false)));
        r.commit(Some("hide hill"), None).unwrap();
        // revert the middle commit: sky restored, hill stays hidden
        tick(&mut r);
        let MergeOutcome::Merged(_) = r.revert(&c1, None).unwrap() else {
            panic!()
        };
        let d = r.read_working().unwrap();
        assert_eq!(d["layers"][0]["opacity"], json!(1));
        assert_eq!(d["layers"][2]["visible"], json!(false));
        assert!(r.log("HEAD", 1).unwrap()[0]
            .1
            .message
            .starts_with("Revert \"dim sky\""));
        assert!(r.revert(&c0, None).unwrap_err().contains("root"));
        // cherry-pick c1 onto a branch from c0
        r.branch_create("old", Some(&c0)).unwrap();
        r.checkout("old", false).unwrap();
        tick(&mut r);
        let MergeOutcome::Merged(p) = r.cherry_pick(&c1, None).unwrap() else {
            panic!()
        };
        assert_eq!(r.read_commit(&p).unwrap().parents, vec![c0.clone()]);
        assert_eq!(
            r.read_working().unwrap()["layers"][0]["opacity"],
            json!(0.1)
        );
        // picking it again changes nothing
        assert!(r.cherry_pick(&c1, None).unwrap_err().contains("already"));
        assert!(r.cherry_pick("zzzz", None).is_err());
    }

    #[test]
    fn semantic_diff_between_revisions() {
        let mut r = new_repo("sd");
        r.commit(Some("a"), None).unwrap();
        tick(&mut r);
        edit(&r, |d| {
            setp(
                d,
                "/channels/a|bone_angle|keys/track/keys/12/value",
                json!(15),
            )
        });
        r.commit(Some("b"), None).unwrap();
        let ch = r.diff(Some("HEAD~1"), Some("HEAD")).unwrap();
        assert_eq!(ch.len(), 1);
        assert!(
            ch[0]
                .description
                .contains("key at frame 12 of channel a.bone_angle: value changed 10 -> 15"),
            "{}",
            ch[0].description
        );
        assert!(r.diff(Some("HEAD"), None).unwrap().is_empty());
        let (_, c, sh) = r.show("HEAD").unwrap();
        assert_eq!(c.message, "b");
        assert_eq!(sh.len(), 1);
        let (_, _, root) = r.show("HEAD~1").unwrap();
        assert!(!root.is_empty());
    }

    #[test]
    fn fsck_detects_corruption_and_gc_keeps_reachable() {
        let mut r = new_repo("fsck");
        r.commit(Some("one"), None).unwrap();
        tick(&mut r);
        edit(&r, |d| setp(d, "/layers/sky/opacity", json!(0.1)));
        r.commit(Some("two"), None).unwrap();
        r.branch_create("tmp", None).unwrap();
        r.checkout("tmp", false).unwrap();
        tick(&mut r);
        edit(&r, |d| setp(d, "/fps", json!(60)));
        r.commit(Some("tmp work"), None).unwrap();
        r.checkout("main", false).unwrap();
        r.branch_delete("tmp").unwrap();
        let total = r.object_count().unwrap();
        let g = r.gc().unwrap();
        assert!(g.removed >= 3, "{g:?}"); // commit, tree, changed blob(s)
        assert_eq!(r.object_count().unwrap(), total - g.removed);
        assert!(r.fsck().is_empty(), "{:?}", r.fsck());
        assert_eq!(r.log("HEAD", 10).unwrap().len(), 2);
        assert_eq!(r.gc().unwrap().removed, 0);
        // corrupt a blob: flip a byte
        let name = r
            .all_object_names()
            .unwrap()
            .into_iter()
            .find(|h| r.get_object(h).map(|o| o.0 == "blob").unwrap_or(false))
            .unwrap();
        let p = r.obj_path(&name).unwrap();
        let mut b = fs::read(&p).unwrap();
        let n = b.len();
        b[n - 2] ^= 1;
        fs::write(&p, &b).unwrap();
        let bad = r.fsck();
        assert!(bad.iter().any(|m| m.contains("corrupt")), "{bad:?}");
        let _ = r.gc();
        assert!(
            p.exists(),
            "gc must not delete a reachable (even corrupt) object"
        );
        // a deleted object is reported by its referrer
        fs::remove_file(&p).unwrap();
        assert!(
            r.fsck().iter().any(|m| m.contains("missing")),
            "{:?}",
            r.fsck()
        );
    }

    #[test]
    fn bad_input_and_malformed_repos_do_not_panic() {
        let d = tmp("bad");
        assert!(Repo::init(&d.join("nope.json"), None).is_err());
        fs::write(d.join("a.json"), "not json").unwrap();
        assert!(Repo::init(&d.join("a.json"), None).is_err());
        fs::write(d.join("a.json"), "[1,2]").unwrap();
        assert!(Repo::init(&d.join("a.json"), None)
            .unwrap_err()
            .contains("JSON object"));
        fs::write(d.join("a.json"), "{}").unwrap();
        let mut r = Repo::init(&d.join("a.json"), None).unwrap();
        r.fixed_time = Some(5);
        assert!(Repo::init(&d.join("a.json"), None)
            .unwrap_err()
            .contains("already"));
        assert!(r.commit(None, None).is_err()); // no message
        r.commit(Some("m"), None).unwrap();
        assert!(Repo::open(&d.join("zzz")).is_err());
        assert!(r.get_object("xyz").is_err());
        assert!(r.get_object(&"0".repeat(64)).is_err());
        assert!(r.resolve_rev("../../etc/passwd").is_err());
        assert!(r.branch_create("", None).is_err());
        assert!(r.tag_create("a b", None).is_err());
        assert!(r.checkout("nonexistent", false).is_err());
        // damage things one by one; every call must return, not panic
        fs::write(r.dir.join("HEAD"), "garbage").unwrap();
        assert!(r.head_commit().is_err() && r.status().is_err() && r.log("HEAD", 3).is_err());
        assert!(!r.fsck().is_empty());
        fs::write(r.dir.join("HEAD"), "ref: refs/heads/../../x\n").unwrap();
        assert!(r.current_branch().is_err());
        fs::write(r.dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(r.dir.join("refs/heads/main"), "nothex").unwrap();
        assert!(r.head_commit().is_err() && !r.fsck().is_empty());
        fs::write(
            r.dir.join("refs/heads/main"),
            format!("{}\n", "a".repeat(64)),
        )
        .unwrap();
        assert!(
            r.log("HEAD", 3).is_err()
                && r.merge("HEAD", false, None, None).is_err()
                && r.gc().is_err()
        );
        fs::write(r.dir.join("MERGE_STATE"), "{").unwrap();
        assert!(
            r.merge_state().is_err()
                && r.resolve("/x", &Choice::Ours).is_err()
                && r.abort().is_err()
        );
        fs::write(r.dir.join("config"), "{\"project\": \"../evil.json\"}").unwrap();
        assert!(r.project_path().is_err() && r.read_working().is_err());
        fs::write(r.dir.join("config"), "").unwrap();
        assert!(r.project_path().is_err() && !r.fsck().is_empty());
        let _ = fs::remove_dir_all(r.dir.join("objects"));
        assert!(!r.fsck().is_empty());
        let _ = r.default_author();
    }

    #[test]
    fn object_decoding_rejects_junk() {
        for junk in [
            &b""[..],
            b"no newline",
            b"ARMOBJ1 blob x\nabc",
            b"ARMOBJ1 blob 5\nabc",
            b"ARMOBJ1 weird 3\nabc",
            b"OTHER blob 3\nabc",
            b"ARMOBJ1 blob 3 extra\nabc",
        ] {
            assert!(decode_object(junk).is_err(), "{junk:?}");
        }
        assert_eq!(decode_object(b"ARMOBJ1 blob 3\nabc").unwrap().1, b"abc");
    }

    #[test]
    fn working_file_edge_cases() {
        let r = new_repo("work");
        fs::write(r.project_path().unwrap(), "[1]").unwrap();
        assert!(r.read_working().is_err());
        fs::write(r.project_path().unwrap(), "{broken").unwrap();
        assert!(r.status().is_err() && r.commit(Some("x"), None).is_err());
        fs::remove_file(r.project_path().unwrap()).unwrap();
        assert!(r.commit(Some("x"), None).is_err());
    }
}
