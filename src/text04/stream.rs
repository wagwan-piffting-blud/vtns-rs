//! The two token lists of `fe_NormalizeTokens`: the look-ahead buffer (`ctx[2]`: count, the cursor
//! before each token, up to 100 tokens) and the list of consumed tokens (`ctx[1]`, kept for the whole
//! sentence; the recognizers look back into it).

use super::mem::*;
use super::tokenize::{tokenize, Tok};
use super::userdict::UserDict;
use super::alias::tokenize_alias;

#[derive(Clone)]
pub struct Ahead {
    pub n: i32,
    /// `pos[k]`: cursor before token `k` (`pos[n]` after the last); `None` where the engine's slot is 0.
    pub pos: [Option<usize>; 102],
    pub toks: [Tok; 101],
}

impl Default for Ahead {
    fn default() -> Self {
        Ahead { n: 0, pos: [None; 102], toks: [Tok::default(); 101] }
    }
}

impl Ahead {
    pub fn reset(&mut self) {
        *self = Ahead::default();
    }

    /// `fe_TokenPush` (0x1004d380): tokenize the next token into the buffer (`'R'`: the alias
    /// tokenizer). Returns its index.
    pub fn push(&mut self, t: &[u8], cur: &mut usize, base: usize, mode: u8, ud: Option<&UserDict>) -> Option<usize> {
        if self.n > 99 {
            return None;
        }
        let k = self.n as usize;
        self.pos[k] = Some(*cur);
        let mut tok = Tok::default();
        if mode == b'R' {
            tokenize_alias(&mut tok, t, cur, base, 0);
        } else {
            tokenize(&mut tok, t, cur, base, ud);
        }
        self.toks[k] = tok;
        self.n += 1;
        self.pos[k + 1] = Some(*cur);
        Some(k)
    }

    /// `FUN_1004d410`: drop the last token, rewinding the cursor to its start. Returns the index of the
    /// new last token.
    pub fn pop(&mut self, cur: &mut usize) -> Option<isize> {
        if self.n < 1 {
            return None;
        }
        self.n -= 1;
        if let Some(p) = self.pos[self.n as usize] {
            *cur = p;
        }
        Some(self.n as isize - 1)
    }

    /// `FUN_1004d440`: move the count by `delta` (negative: give tokens back) when that slot was used.
    pub fn seek(&mut self, cur: &mut usize, delta: i32) -> isize {
        let k = self.n + delta;
        if k >= 0 && delta != 0 {
            if let Some(p) = self.pos[k as usize] {
                self.n = k;
                *cur = p;
            }
        }
        self.n as isize - 1
    }

    pub fn tok(&self, i: isize) -> Tok {
        if i < 0 {
            Tok::default()
        } else {
            self.toks[i as usize]
        }
    }
}

#[derive(Clone, Default)]
pub struct Done {
    pub toks: Vec<Tok>,
}

impl Done {
    pub fn n(&self) -> i32 {
        self.toks.len() as i32
    }

    /// `fe_TokenStreamAppend` (0x1004d180): 1, or 4 when full; empty tokens are not kept.
    pub fn append(&mut self, t: &Tok, flag: i32) -> i32 {
        if t.text[0] == 0 {
            return 1;
        }
        if self.toks.len() > 99 {
            return 4;
        }
        let mut c = *t;
        c.f1 = flag;
        self.toks.push(c);
        1
    }

    /// `FUN_1004d1e0`: every buffered token but tag markers.
    pub fn append_all(&mut self, a: &Ahead, flag: i32) -> i32 {
        for k in 0..a.n as usize {
            let t = &a.toks[k];
            if t.ty != 4 && t.ty != 5 && self.append(t, flag) == 4 {
                return 4;
            }
        }
        1
    }

    /// `FUN_1004d240`: buffered tokens `from..` up to `max(n, to)`.
    pub fn append_range(&mut self, a: &Ahead, from: i32, to: i32, flag: i32) -> i32 {
        let mut k = from;
        while k < a.n || k < to {
            let t = &a.toks[k as usize];
            if t.ty != 4 && t.ty != 5 && self.append(t, flag) == 4 {
                return 4;
            }
            k += 1;
        }
        1
    }

    /// `FUN_1004d2b0`: a word token made from `len` bytes of `text`.
    pub fn append_text(&mut self, text: &[u8], len: usize, start: i32, end: i32) -> i32 {
        if at(text, 0) == 0 {
            return 1;
        }
        if self.toks.len() > 99 {
            return 4;
        }
        let mut t = Tok::default();
        let n = cs(text).len().min(len);
        t.text[..n].copy_from_slice(&text[..n]);
        t.len = len as i32;
        t.ws = 0;
        t.start = start;
        t.end = end;
        t.f1 = 0x1a;
        t.ty = 1;
        self.toks.push(t);
        1
    }
}

/// `fe_TokenAt` (0x100507b0): the token `rel` (< 0) places before buffered token `idx`, looking back
/// into the consumed list past the buffer's start.
pub fn token_at(done: &Done, a: &Ahead, idx: i32, rel: i32) -> Option<Tok> {
    if rel >= 0 || idx < 0 {
        return None;
    }
    if !(idx < a.n && idx + rel < a.n) {
        return None;
    }
    let k = idx + rel;
    if k >= 0 {
        return Some(a.toks[k as usize]);
    }
    let j = done.n() + k;
    if j >= 0 && j <= done.n() - 1 {
        return Some(done.toks[j as usize]);
    }
    None
}
