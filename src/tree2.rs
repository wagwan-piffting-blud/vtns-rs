//! `.tree2` CART files of the 2009 Paul engine (vt_eng_paul16.dll): scalar trees `FUN_10001850`
//! (node reader `FUN_10001650`), vector trees `FUN_10001d10` (`FUN_10001ac0`), the 27-tree
//! `atmt.tree2` `FUN_10001910`. Walkers `FUN_100015f0` / `FUN_10001a40`.
//!
//! A file is one pre-order tree. Every node carries its own value(s):
//!
//! - scalar: `feat u8, kind u8, thresh i16, value i16, nset u8, set[nset] i16, b1 u8, b2 u8`
//! - vector: `feat u8, kind u8, thresh i16, nval u8, vals[nval] i16, nset u8, set[nset] i16, b1 u8, b2 u8`
//!
//! `b2 != 0` means the first (yes) and second (no) subtrees follow; `b1` is read and dropped. The walk
//! is the tree3 one (`D`: set membership, else `thresh < x` goes to "no"), so each file converts into a
//! [`Tree`] with leaves as negative child references. Vector leaves copy the root's `nval` shorts from
//! the leaf's place in the engine's shared value buffer (vals and sets interleaved in pre-order).

use std::io;
use std::path::Path;

use crate::tree3::{Node, Tree};

fn rd_i16(b: &[u8], p: usize) -> Option<i16> {
    Some(i16::from_le_bytes([*b.get(p)?, *b.get(p + 1)?]))
}

struct Raw {
    feat: u8,
    kind: u8,
    thresh: i16,
    value: i16,
    /// Offset of this node's vals in the shared buffer (vector trees).
    val_off: usize,
    nval: usize,
    set: Vec<i16>,
    kids: Option<(usize, usize)>,
}

struct Parser<'a> {
    b: &'a [u8],
    p: usize,
    vector: bool,
    nodes: Vec<Raw>,
    buf: Vec<i16>,
}

impl Parser<'_> {
    fn node(&mut self) -> Option<usize> {
        let b = self.b;
        let feat = *b.get(self.p)?;
        let kind = *b.get(self.p + 1)?;
        let thresh = rd_i16(b, self.p + 2)?;
        self.p += 4;
        let (mut value, mut val_off, mut nval) = (0, self.buf.len(), 0);
        if self.vector {
            nval = *b.get(self.p)? as usize;
            self.p += 1;
            val_off = self.buf.len();
            for k in 0..nval {
                self.buf.push(rd_i16(b, self.p + 2 * k)?);
            }
            self.p += 2 * nval;
        } else {
            value = rd_i16(b, self.p)?;
            self.p += 2;
        }
        let nset = *b.get(self.p)? as usize;
        self.p += 1;
        let mut set = Vec::with_capacity(nset);
        for k in 0..nset {
            set.push(rd_i16(b, self.p + 2 * k)?);
        }
        self.buf.extend_from_slice(&set);
        self.p += 2 * nset;
        let more = *b.get(self.p + 1)? != 0;
        self.p += 2;
        let me = self.nodes.len();
        self.nodes.push(Raw { feat, kind, thresh, value, val_off, nval, set, kids: None });
        if more {
            let yes = self.node()?;
            let no = self.node()?;
            self.nodes[me].kids = Some((yes, no));
        }
        Some(me)
    }
}

/// Parse one tree at `b[0]`; returns it and the bytes consumed.
pub fn parse(b: &[u8], vector: bool) -> Option<(Tree, usize)> {
    let mut ps = Parser { b, p: 0, vector, nodes: Vec::new(), buf: Vec::new() };
    ps.node()?;
    let dim = if vector { ps.nodes[0].nval } else { 1 };
    let mut idx = vec![0i16; ps.nodes.len()];
    let (mut n_int, mut n_leaf) = (0usize, 0usize);
    for (i, r) in ps.nodes.iter().enumerate() {
        if r.kids.is_some() {
            idx[i] = i16::try_from(n_int).ok()?;
            n_int += 1;
        } else {
            idx[i] = i16::try_from(-(n_leaf as i32) - 1).ok()?;
            n_leaf += 1;
        }
    }
    let mut nodes = Vec::with_capacity(n_int);
    let mut values = Vec::with_capacity(n_leaf * dim);
    for r in &ps.nodes {
        match r.kids {
            Some((y, n)) => nodes.push(Node { feat: r.feat, kind: r.kind, thresh: r.thresh, set: r.set.clone(), yes: idx[y], no: idx[n] }),
            None if vector => {
                for k in 0..dim {
                    values.push(ps.buf.get(r.val_off + k).copied().unwrap_or(0));
                }
            }
            None => values.push(r.value),
        }
    }
    Some((Tree { dim, nodes, values }, ps.p))
}

fn bad(path: &Path) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("bad tree2: {}", path.display()))
}

/// A whole-file scalar (`vector == false`) or vector tree. The engine ignores trailing bytes.
pub fn load(path: &Path, vector: bool) -> io::Result<Tree> {
    let b = std::fs::read(path)?;
    parse(&b, vector).map(|(t, _)| t).ok_or_else(|| bad(path))
}

/// `atmt.tree2`: `count` records of `u32 length` + scalar tree.
pub fn load_multi(path: &Path, count: usize) -> io::Result<Vec<Tree>> {
    let b = std::fs::read(path)?;
    let mut p = 0;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let len = u32::from_le_bytes(b.get(p..p + 4).ok_or_else(|| bad(path))?.try_into().unwrap()) as usize;
        let rec = b.get(p + 4..p + 4 + len).ok_or_else(|| bad(path))?;
        out.push(parse(rec, false).ok_or_else(|| bad(path))?.0);
        p += 4 + len;
    }
    Ok(out)
}
