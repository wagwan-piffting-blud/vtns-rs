//! Sentence-boundary decision for `.?!;` (`FUN_10051cc0`): a 6-token window
//! (three tokens before the punctuation, the punctuation, two after), hand
//! rules (`FUN_10052000`) returning `P` end / `N` not end / `A` ambiguous, and
//! for `A` the `sbd.tree3` CART over 16 features (`FUN_10052520`).

use super::cstr::*;
use super::tokenize::{tokenize, Tok};
use super::window::{back_words, last_three};
use super::Fe;

/// One window slot as the engine copies it: ws, len, class and text only.
#[derive(Clone)]
struct Slot {
    ws: i32,
    len: i32,
    class: i32,
    text: [u8; 36],
}

impl Default for Slot {
    fn default() -> Self {
        Slot { ws: 0, len: 0, class: 0, text: [0; 36] }
    }
}

impl Slot {
    fn from(t: &Tok) -> Slot {
        Slot { ws: t.ws, len: t.len, class: t.class, text: t.text }
    }
    fn c0(&self) -> u8 {
        self.text[0]
    }
}

/// Class of the upper/lower-case mix (`FUN_10052710`).
fn case_class(s: &Slot) -> i32 {
    if s.len == 0 {
        return -1;
    }
    if s.class == 2 {
        return 4;
    }
    if s.class == 1 {
        let (mut up, mut low, mut first_up) = (false, false, false);
        for i in 0..s.len as usize {
            let k = cls(s.text[i]);
            if k & 0x80 == 0 {
                if k & 0x40 != 0 {
                    low = true;
                }
            } else {
                up = true;
                if i == 0 {
                    first_up = true;
                }
            }
        }
        if up && !low {
            return 1;
        }
        if low && !up {
            return 2;
        }
        if first_up && low {
            return 3;
        }
    }
    5
}

/// Punctuation class of a one-character token (`FUN_100527c0`).
fn punct_class(sl: &Slot) -> i32 {
    if sl.len == 1 && sl.c0() != 0 && strchr(s(0x100813cc), sl.c0()) {
        return match sl.c0() {
            0x21 => 3,
            0x22 | 0x27 | 0x60 => 6,
            0x2c | 0x3a => 5,
            0x2d => 7,
            0x2e => 1,
            0x3b => 4,
            0x3f => 2,
            _ => 8,
        };
    }
    -1
}

/// `FUN_100526b0`: bit 1 abbrh, bit 2 abbrt, bit 4 abbrc membership.
fn abbr_bits(fe: &Fe, w: &[u8]) -> u8 {
    let mut b = 0u8;
    if fe.abbrh.find_flagged(w) >= 0 {
        b |= 1;
    }
    if fe.abbrt.find_flagged(w) >= 0 {
        b |= 2;
    }
    if fe.abbrc.find_flagged(w) >= 0 {
        b |= 4;
    }
    b
}

/// `FUN_10052c90`: an interior '.' followed by something.
fn inner_dot(w: &[u8]) -> i16 {
    match strchr_at(w, b'.') {
        Some(i) if i > 0 && at(w, i as isize + 1) != 0 => 1,
        _ => 0,
    }
}

/// `FUN_1000ffa0`: all letters.
pub fn all_alpha(w: &[u8]) -> bool {
    let w = cs(w);
    !w.is_empty() && w.iter().all(|&c| cls(c) & 0xc0 != 0)
}

/// `FUN_10061940`: all digits.
pub fn all_digit(w: &[u8]) -> bool {
    let w = cs(w);
    !w.is_empty() && w.iter().all(|&c| cls(c) & 0x10 != 0)
}

/// `FUN_100528b0`: coarse word category of the following token.
fn word_cat(fe: &Fe, w: &[u8]) -> i16 {
    let w = cs(w);
    let l = w.len() as isize;
    let i = fe.sbdw.find(w);
    if i >= 0 {
        let v = atoi(&fe.sbdw.vals[i as usize]);
        let m = b(0x10081358, 0x80);
        return i16::from_le_bytes([m[v as usize * 2], m[v as usize * 2 + 1]]);
    }
    if w.iter().filter(|&&c| cls(c) & 0x10 != 0).count() > 0 {
        return 9;
    }
    if w.iter().filter(|&&c| cls(c) & 0x80 != 0).count() > 1 {
        return 3;
    }
    let c0 = at(w, 0);
    if c0 != 0 && strchr(s(0x100813bc), c0) {
        return 0x11;
    }
    if c0 == b',' {
        return 0xb;
    }
    if c0 != 0 && strchr(s(0x10081400), c0) {
        return 0xf;
    }
    if c0 != 0 && strchr(s(0x100813fc), c0) {
        return 0xc;
    }
    if c0 != 0 && strchr(s(0x100813f4), c0) {
        return 0xd;
    }
    if c0 != 0 && strchr(s(0x100813ec), c0) {
        return 0xe;
    }
    if !all_alpha(w) {
        let ap = match strchr_at(w, b'\'') {
            None => return if cls(c0) & 0xc0 != 0 { 0 } else { 0x10 },
            Some(a) => a,
        };
        let mut sub = [0u8; 32];
        if ap > 0 && stricmp(&w[ap - 1..], s(0x10077568)) == 0 {
            let n = ap - 1;
            sub[..n].copy_from_slice(&w[..n]);
            sub[n] = 0;
        } else {
            let tail = &w[ap + 1..];
            let hit = [0x10077770u32, 0x1007d38c, 0x1007d36c, 0x100813e0, 0x100813dc, 0x10078d74, 0x100813d8]
                .iter()
                .any(|&va| stricmp(tail, s(va)) == 0);
            if hit {
                sub[..ap].copy_from_slice(&w[..ap]);
                sub[ap] = 0;
            }
        }
        if sub[0] == 0 {
            return 0;
        }
        return word_cat(fe, &sub);
    }
    let tail = |k: isize| &w[(l - k) as usize..];
    if l > 6 && stricmp(tail(6), s(0x10077830)) == 0 {
        return 1;
    }
    if l > 4 {
        for va in [0x10077828u32, 0x10077820, 0x10077800, 0x100777f0, 0x10077810, 0x100777e0, 0x100813e4] {
            if stricmp(tail(4), s(va)) == 0 {
                return 1;
            }
        }
    }
    if l > 3 && stricmp(tail(3), s(0x10077780)) == 0 {
        return 1;
    }
    if l > 4 && stricmp(tail(4), s(0x100774d0)) == 0 {
        return 5;
    }
    if l > 3 && (stricmp(tail(3), s(0x10077378)) == 0 || stricmp(tail(3), s(0x100777b0)) == 0) {
        return 5;
    }
    if l < 3 {
        return 0;
    }
    if stricmp(tail(2), s(0x10077784)) != 0 {
        return 0;
    }
    5
}

/// `FUN_10052520`: the 16 CART features (shorts) around the punctuation.
fn features(fe: &Fe, w: &[Slot; 6]) -> [i16; 16] {
    let mut f = [-1i16; 16];
    let prev = &w[2];
    let next = &w[4];
    if prev.len == 0 {
        f[0] = 0;
    } else {
        f[0] = prev.len as i16;
        f[1] = case_class(prev) as i16;
        let bits = abbr_bits(fe, &prev.text);
        f[2] = (bits & 1 == 1) as i16;
        f[3] = (bits & 2 == 2) as i16;
        f[4] = (bits & 4 == 4 || f[3] != 0 || f[2] != 0) as i16;
        f[5] = inner_dot(&prev.text);
        f[6] = punct_class(prev) as i16;
    }
    let mut s3 = -1i16;
    if next.len == 0 {
        f[7] = 0;
    } else {
        f[7] = next.len as i16;
        f[8] = case_class(next) as i16;
        let bits = abbr_bits(fe, &next.text);
        f[9] = (bits & 1 == 1) as i16;
        f[10] = (bits & 2 == 2) as i16;
        f[11] = (bits & 4 == 4 || f[10] != 0 || f[9] != 0) as i16;
        f[12] = inner_dot(&next.text);
        f[13] = punct_class(next) as i16;
        s3 = word_cat(fe, &next.text);
    }
    f[14] = s3;
    f[15] = punct_class(&w[3]) as i16;
    f
}

/// `FUN_10052000`: rules. Slots: 0,1,2 before, 3 the punctuation, 4,5 after.
fn rules(w: &[Slot; 6], after: &[u8]) -> u8 {
    let (s0, s1, s2, s3, s4, s5) = (&w[0], &w[1], &w[2], &w[3], &w[4], &w[5]);
    let c = s3.c0();
    if c != 0 && strchr(s(0x100813c8), c) && s3.ws == 0 && s3.len == 1 {
        return b'P';
    }
    if c == b'.' && s4.len == 0 && matches!(at(after, 0), b'\t' | b'\n' | b'\r') {
        return b'P';
    }
    if s4.len == 0 {
        return b'P';
    }
    let nws = s4.ws;
    if nws < 3 && s4.class == 8 {
        return b'N';
    }
    if nws > 1 {
        return b'P';
    }
    let plen = s2.len;
    if plen > 0 && cls(s2.c0()) & 0x80 != 0 && s3.ws == 0 && s3.len == 1 && c == b'.' && cls(s4.c0()) & 0x80 != 0 && nws == 0 {
        return b'N';
    }
    // LAB_100521ca
    let check_521ca = || s3.len == 1 && c == b'.' && cls(s4.c0()) & 0xc0 != 0 && nws == 0;
    if plen < 1 {
        if check_521ca() {
            return b'N';
        }
    } else {
        if cls(s2.c0()) & 0x80 != 0 && s3.ws == 0 && s3.len == 1 && c == b'.' && cls(s4.c0()) & 0x40 != 0 && nws == 0 {
            return b'N';
        }
        if cls(s2.c0()) & 0x40 == 0 || s3.ws != 0 {
            if check_521ca() {
                return b'N';
            }
        } else if s3.len == 1 {
            if c == b'.' && cls(s4.c0()) & 0x80 != 0 && nws == 0 {
                return b'P';
            }
            if check_521ca() {
                return b'N';
            }
        }
    }
    if plen > 0 && all_alpha(&s2.text) && s3.ws == 0 && s3.len == 1 && c == b'.' && all_digit(&s4.text) && s4.ws == 0 {
        return b'N';
    }
    if plen > 0 {
        if plen == 1 && cls(s2.c0()) & 0x80 != 0 && s3.ws == 0 && s3.len == 1 && c == b'.' && s4.ws < 2 && s4.len == 1 && cls(s4.c0()) & 0x80 != 0 {
            return b'N';
        }
        if plen == 1 && cls(s2.c0()) & 0x80 != 0 && s3.ws == 0 && s3.len == 1 && c == b'.' && s4.len == 0 {
            return b'P';
        }
        if s3.ws == 0 && s3.len < 3 && c == b'.' && s4.ws < 2 && strcmp(&s4.text, s(0x10077400)) == 0 {
            return b'N';
        }
    }
    if s1.len > 0 && s1.ws == 0 && s2.ws == 0 {
        if s3.ws != 0 {
            return tail_rule(s0, s1, s2, s3);
        }
        if s3.len == 1 && c == b'.' && s4.ws > 0 && s4.len > 0 && cls(s4.c0()) & 0x80 != 0 {
            return b'P';
        }
    }
    if s3.ws == 0 && s3.len == 1 && c == b'.' && s4.len == 1 && s4.ws > 0 && s4.class == 3 && s4.c0() != 0 && strchr(s(0x100813c4), s4.c0()) && s5.len > 0 && s5.ws == 0 && s5.class == 1 && cls(s5.c0()) & 0x80 != 0 {
        return b'P';
    }
    tail_rule(s0, s1, s2, s3)
}

/// `LAB_1005245b`: "neither/nor/so ... do/am I." patterns.
fn tail_rule(s0: &Slot, s1: &Slot, s2: &Slot, s3: &Slot) -> u8 {
    if s0.len > 0
        && (stricmp(&s0.text, s(0x10079024)) == 0 || stricmp(&s0.text, s(0x10079020)) == 0 || stricmp(&s0.text, s(0x10078ec8)) == 0)
        && s1.len > 0
        && (stricmp(&s1.text, s(0x1007786c)) == 0 || stricmp(&s1.text, s(0x10079134)) == 0)
        && s2.len > 0
        && stricmp(&s2.text, s(0x100776e8)) == 0
        && s3.ws == 0
        && s3.len < 3
        && s3.c0() == b'.'
    {
        return b'P';
    }
    b'A'
}

/// `FUN_10051cc0`: is the punctuation token `cur` at `t[p..]` (sentence
/// offset `off`) a sentence end? Returns `b'P'` or `b'N'`.
pub fn decide(fe: &Fe, cur: &Tok, t: &[u8], p: usize, off: i32) -> u8 {
    let mut w: [Slot; 6] = Default::default();
    let mut buf = [0u8; 96];
    let lo = p as isize - off as isize;
    let n = back_words(&mut buf, t, lo, p as isize, 3);
    if n as i16 != 0 {
        match last_three(&buf, off - n, 1) {
            None => return b'N',
            Some((a, b, c)) => {
                if a.len > 0 {
                    w[0] = Slot::from(&a);
                }
                if b.len > 0 {
                    w[1] = Slot::from(&b);
                }
                if c.len > 0 {
                    w[2] = Slot::from(&c);
                }
            }
        }
    }
    w[3] = Slot::from(cur);
    let q = p + cur.consumed as usize;
    let mut nx = Tok::default();
    tokenize(&mut nx, t, q, cur.consumed + off, 1, fe.ud.as_ref());
    if nx.len > 0 {
        w[4] = Slot::from(&nx);
        let mut nn = Tok::default();
        let q2 = q + nx.consumed as usize;
        tokenize(&mut nn, t, q2, nx.consumed + cur.consumed + off, 1, fe.ud.as_ref());
        if nn.len > 0 {
            w[5] = Slot::from(&nn);
        }
    }
    let r = rules(&w, &t[q.min(t.len())..]);
    if r == b'A' {
        let f = features(fe, &w);
        let v = fe.sbd.predict_scalar(&f);
        return if (v as u8) != 0 { b'P' } else { b'N' };
    }
    r
}
