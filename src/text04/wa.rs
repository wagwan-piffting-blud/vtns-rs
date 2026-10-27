//! The 2004 normalization word array as the engine's byte image: `+0 consumed`, `+4 n`, then 0x88-byte
//! records from `+0x14` (`+0 start`, `+4 end`, `+8 len`, `+0x18` class letter, `+0x19` source letter,
//! `+0x1a` 0xff, `+0x1e` text, `+0x3c` phone codes). `ling04::Wa04` is the same image.

use super::mem::*;
use crate::ling04::{Wa04, WA_SIZE};

pub const HEAD: usize = 0x14;
pub const REC: usize = 0x88;
pub const MAX: i32 = 100;

#[derive(Clone)]
pub struct Wa {
    pub b: Vec<u8>,
}

impl Default for Wa {
    fn default() -> Self {
        Wa::new()
    }
}

/// Symbols `FUN_10041a00` attaches to the previous word instead of emitting (`0x10072088`).
const PUNCT_NAMES: Rec30 = rec30(0x10072088, 0x10084844);

impl Wa {
    pub fn new() -> Self {
        Wa { b: vec![0u8; HEAD + MAX as usize * REC + 0x40] }
    }

    pub fn rec(i: i32) -> usize {
        (HEAD as i32 + i * REC as i32) as usize
    }

    pub fn i32(&self, o: usize) -> i32 {
        i32::from_le_bytes([self.b[o], self.b[o + 1], self.b[o + 2], self.b[o + 3]])
    }

    pub fn set_i32(&mut self, o: usize, v: i32) {
        self.b[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }

    pub fn consumed(&self) -> i32 {
        self.i32(0)
    }

    pub fn set_consumed(&mut self, v: i32) {
        self.set_i32(0, v)
    }

    pub fn n(&self) -> i32 {
        self.i32(4)
    }

    pub fn set_n(&mut self, v: i32) {
        self.set_i32(4, v)
    }

    pub fn ri32(&self, i: i32, o: usize) -> i32 {
        self.i32(Wa::rec(i) + o)
    }

    pub fn set_ri32(&mut self, i: i32, o: usize, v: i32) {
        self.set_i32(Wa::rec(i) + o, v)
    }

    pub fn text(&self, i: i32) -> &[u8] {
        cs(&self.b[Wa::rec(i) + 0x1e..])
    }

    /// `FUN_1004d650`'s reset: header and records cleared from `+0xc` (the `+8` word survives).
    pub fn reset(&mut self) {
        self.set_i32(0, 0);
        self.set_i32(4, 0);
        let end = (0xc + 0xd48 * 4).min(self.b.len());
        self.b[0xc..end].fill(0);
    }

    pub fn to_wa04(&self) -> Wa04 {
        let mut w = Wa04::new();
        w.b[..WA_SIZE].copy_from_slice(&self.b[..WA_SIZE]);
        w
    }

    /// `FUN_100508f0`: record `i`'s text ends in `.?!;`.
    pub fn ends_punct(&self, i: i32) -> bool {
        let t = &self.b[Wa::rec(i) + 0x1e..];
        let l = self.ri32(i, 8);
        if t[0] == 0 || l < 1 {
            return false;
        }
        let c = t[(l - 1) as usize];
        c != 0 && b".?!;".contains(&c)
    }

    /// `fe_AttachPunct` (0x10050860): append `c` to record `i` unless it already ends in `.?!;` or `,`.
    pub fn attach(&mut self, i: i32, c: u8, end: i32) {
        if end > 0 {
            self.set_consumed(end);
        }
        let n = self.n();
        if !(0 < n && n < 100) {
            return;
        }
        let l = self.ri32(i, 8);
        if l + 1 >= 0x1e {
            return;
        }
        let o = Wa::rec(i) + 0x1e;
        let last = if l >= 1 { self.b[o + l as usize - 1] } else { at(&self.b, o as isize - 1) };
        if last != 0 && b".?!;".contains(&last) {
            return;
        }
        if last != b',' {
            self.b[o + l as usize] = c;
            self.set_ri32(i, 8, l + 1);
            self.b[o + l as usize + 1] = 0;
        }
    }

    fn put(&mut self, text: &[u8], start: i32, end: i32, c18: u8, c19: u8) {
        let n = self.n();
        let r = Wa::rec(n);
        self.b[r + 0x18] = c18;
        self.b[r + 0x19] = c19;
        self.set_ri32(n, 0, start);
        self.set_ri32(n, 4, end);
        let t = cs(text);
        self.set_ri32(n, 8, t.len() as i32);
        self.b[r + 0x1a] = 0xff;
        self.b[r + 0x1e..r + 0x1e + t.len()].copy_from_slice(t);
        self.b[r + 0x1e + t.len()] = 0;
    }

    /// `fe_EmitWord` (0x10041a00): one record, or a known symbol attached to the previous word.
    pub fn emit(&mut self, text: &[u8], start: i32, end: i32, c18: u8, c19: u8) -> i32 {
        let text = cs(text);
        if text.is_empty() {
            self.set_consumed(end);
            return 1;
        }
        if PUNCT_NAMES.find(text, false) >= 0 {
            if start <= self.consumed() + 2 {
                let n = self.n();
                self.attach(n - 1, text[0], end);
            }
            return 1;
        }
        if self.n() > 99 {
            return 3;
        }
        self.put(text, start, end, c18, c19);
        self.set_consumed(end);
        self.set_n(self.n() + 1);
        1
    }

    /// `fe_EmitWords` (0x10041d60): blank-separated words, rolled back as a whole when the WA fills.
    pub fn emit_words(&mut self, text: &[u8], start: i32, end: i32, c18: u8, c19: u8) -> i32 {
        let text = cs(text);
        if !text.contains(&b' ') && !text.contains(&b'\t') {
            return self.emit(text, start, end, c18, c19);
        }
        let n0 = self.n();
        let mut w: Vec<u8> = Vec::new();
        for k in 0..=text.len() {
            let c = at(text, k as isize);
            if c == b' ' || c == b'\t' || c == 0 {
                if self.emit(&w, start, end, c18, c19) == 3 {
                    self.set_n(n0);
                    if n0 >= 1 {
                        let e = self.ri32(n0 - 1, 4);
                        self.set_consumed(e);
                    }
                    return 3;
                }
                w.clear();
            } else {
                w.push(c);
            }
        }
        1
    }

    /// `fe_EmitWordAlias` (0x10041b30): a record carrying phone codes.
    pub fn emit_phones(&mut self, text: &[u8], phones: &[u8], start: i32, end: i32, c18: u8, c19: u8) -> i32 {
        if cs(text).is_empty() || cs(phones).is_empty() {
            self.set_consumed(end);
            return 1;
        }
        if self.n() > 99 {
            return 3;
        }
        self.put(text, start, end, c18, c19);
        let r = Wa::rec(self.n()) + 0x3c;
        let p = cs(phones);
        self.b[r..r + p.len()].copy_from_slice(p);
        self.b[r + p.len()] = 0;
        self.set_consumed(end);
        self.set_n(self.n() + 1);
        1
    }
}
