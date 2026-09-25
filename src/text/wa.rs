//! The normalization word array (WA, `TIB+0x1312bc`), kept as the engine's raw
//! byte image (same layout as `ling::Wa`) so normalizers can be transliterated
//! with their literal offsets and compared byte for byte with captures.
//!
//! Header: `+0 s16 n`, `+2 s16`, `+4 i32 consumed`, `+8 i32`, `+0xc s16 tag
//! mode`, `+0x10 ptr (common DB)`. Records of 0x94 bytes from `+0x14`:
//! `+0 start`, `+4 end`, `+8 len`, `+0xc pause (-1)`, `+0x1c/+0x1e flags`,
//! `+0x23 u8`, `+0x24 u8`, `+0x28 source class`, `+0x2c following punctuation`,
//! `+0x30 u8 (0xff)`, `+0x34 text`, `+0x52 phones`.

pub const HEAD: usize = 0x14;
pub const REC: usize = 0x94;
pub const MAX: usize = 100;
pub const SIZE: usize = HEAD + MAX * REC;

#[derive(Clone)]
pub struct Wa {
    pub b: Vec<u8>,
}

impl Default for Wa {
    fn default() -> Self {
        Wa::new()
    }
}

impl Wa {
    pub fn new() -> Self {
        Wa { b: vec![0u8; SIZE + 0x40] }
    }

    #[inline]
    pub fn rec(i: i32) -> usize {
        (HEAD as i32 + i * REC as i32) as usize
    }

    #[inline]
    pub fn u8(&self, o: usize) -> u8 {
        self.b[o]
    }

    #[inline]
    pub fn set_u8(&mut self, o: usize, v: u8) {
        self.b[o] = v;
    }

    #[inline]
    pub fn i16(&self, o: usize) -> i16 {
        i16::from_le_bytes([self.b[o], self.b[o + 1]])
    }

    #[inline]
    pub fn set_i16(&mut self, o: usize, v: i16) {
        self.b[o..o + 2].copy_from_slice(&v.to_le_bytes());
    }

    #[inline]
    pub fn i32(&self, o: usize) -> i32 {
        i32::from_le_bytes([self.b[o], self.b[o + 1], self.b[o + 2], self.b[o + 3]])
    }

    #[inline]
    pub fn set_i32(&mut self, o: usize, v: i32) {
        self.b[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }

    pub fn n(&self) -> i16 {
        self.i16(0)
    }

    pub fn set_n(&mut self, v: i16) {
        self.set_i16(0, v)
    }

    pub fn consumed(&self) -> i32 {
        self.i32(4)
    }

    pub fn set_consumed(&mut self, v: i32) {
        self.set_i32(4, v)
    }

    /// Record `i` field at `o`.
    pub fn ri32(&self, i: i32, o: usize) -> i32 {
        self.i32(Wa::rec(i) + o)
    }

    pub fn set_ri32(&mut self, i: i32, o: usize, v: i32) {
        self.set_i32(Wa::rec(i) + o, v)
    }

    pub fn text(&self, i: i32) -> &[u8] {
        let o = Wa::rec(i) + 0x34;
        let s = &self.b[o..];
        &s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())]
    }

    /// `FUN_1003e210`: reset for a new sentence (the tag-mode flag survives only when 1).
    pub fn reset(&mut self) {
        self.set_i32(4, 0);
        self.set_i16(0, 0);
        self.set_i16(2, 0);
        if self.i16(0xc) != 1 {
            self.set_i16(0xc, 0);
        }
        for x in &mut self.b[HEAD..HEAD + 0xe74 * 4] {
            *x = 0;
        }
    }

    /// Zero record `i` and set its pause to -1 (the chain's rollback).
    pub fn clear_rec(&mut self, i: i32) {
        if i < MAX as i32 {
            let o = Wa::rec(i);
            for x in &mut self.b[o..o + REC] {
                *x = 0;
            }
            self.set_i32(o + 0xc, -1);
        }
    }

    /// Restore n and consumed to their state before a failed normalizer.
    pub fn restore(&mut self, n: i16) {
        self.set_n(n);
        let c = if n < 1 { self.consumed() } else { self.ri32(n as i32 - 1, 4) };
        self.set_consumed(c);
    }

    /// `FUN_100451e0`: emit `text`, one record per space/tab-separated part,
    /// all sharing the same span and tags.
    pub fn emit_split(&mut self, text: &[u8], start: i32, end: i32, a: u8, b: u8, class: u32) -> bool {
        let l = text.iter().position(|&c| c == 0).unwrap_or(text.len());
        let text = &text[..l];
        if !text.contains(&b' ') && !text.contains(&b'\t') {
            return self.emit(text, start, end, a, b, class);
        }
        let mut part = [0u8; 64];
        let mut k = 0;
        for i in 0..=l {
            let c = if i < l { text[i] } else { 0 };
            if c == b' ' || c == b'\t' || c == 0 {
                part[k] = 0;
                if !self.emit(&part[..k], start, end, a, b, class) {
                    return false;
                }
                k = 0;
            } else {
                part[k] = c;
                k += 1;
            }
        }
        true
    }

    /// `FUN_10045070`: `emit` plus a lexicon pronunciation copied to +0x52.
    pub fn emit_phones(&mut self, text: &[u8], phones: &[u8], start: i32, end: i32, a: u8, b: u8, class: u32) -> bool {
        let l = text.iter().position(|&c| c == 0).unwrap_or(text.len());
        if l == 0 {
            return true;
        }
        if !self.emit(text, start, end, a, b, class) {
            return false;
        }
        let o = Wa::rec(self.n() as i32 - 1) + 0x52;
        let pl = phones.iter().position(|&c| c == 0).unwrap_or(phones.len());
        self.b[o..o + pl].copy_from_slice(&phones[..pl]);
        self.b[o + pl] = 0;
        true
    }

    /// `FUN_10045140`: `emit` with a part-of-speech byte at +0x30.
    pub fn emit_pos(&mut self, text: &[u8], pos: u8, start: i32, end: i32, a: u8, b: u8, class: u32) -> bool {
        let n = self.n();
        if !self.emit(text, start, end, a, b, class) {
            return false;
        }
        if self.n() > n {
            let o = Wa::rec(n as i32);
            self.b[o + 0x30] = pos;
            self.b[o + 0x31] = 0xff;
        }
        true
    }

    /// `fe_EmitWord` (0x10044fd0).
    pub fn emit(&mut self, text: &[u8], start: i32, end: i32, a: u8, b: u8, class: u32) -> bool {
        let l = text.iter().position(|&c| c == 0).unwrap_or(text.len());
        if l == 0 {
            return true;
        }
        let n = self.n();
        if n > 99 {
            return false;
        }
        let o = Wa::rec(n as i32);
        self.set_i32(o, start);
        self.set_i32(o + 4, end);
        self.set_i32(o + 8, l as i32);
        self.b[o + 0x23] = a;
        self.b[o + 0x24] = b;
        self.set_i32(o + 0x28, (class & 0xff) as i32);
        self.b[o + 0x30] = 0xff;
        self.b[o + 0x34..o + 0x34 + l].copy_from_slice(&text[..l]);
        self.b[o + 0x34 + l] = 0;
        self.set_n(n + 1);
        let e = self.i32(o + 4);
        self.set_consumed(e);
        true
    }
}
