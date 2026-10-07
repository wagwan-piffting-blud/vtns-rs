//! viol09 (2009 Violeta) linguistic front end after the lexicon pass: word records (0x47c bytes, 5-byte
//! decimal-coded contexts), syllabifier, phrase breaks and phone contexts. Layout and functions are from
//! the 2009 decompile (`FUN_10018e60` words, `FUN_1001a390` syllabifier, `FUN_1001a8f0` breaks with
//! `FUN_1001a670` / `FUN_1001a760`, `FUN_100191b0` contexts, word start/end from `FUN_10019b70`); the
//! 2014 equivalents live in `words.rs`, `syllab.rs`, `breaks.rs`, `context.rs`.

use super::syllab::{PH_STRESS, PH_VOWEL};
use super::words::{war2, CH_CLASS_HI, CH_CLASS_LO};
use super::{syl, war, WaSpa};
use crate::ling::{byte_struct, ptr_ss, ptr_wa, sc, Tib};
use crate::tree3::Tree;

pub const WORD_BASE: usize = 0x64c;
pub const WORD_SIZE: usize = 0x47c;
pub const SYL_BASE: usize = 0x1c6bc;
pub const SYL_SIZE: usize = 0x28;
pub const SS_SIZE: usize = 0x7e200;

/// Word-record fields (record-relative).
pub mod w09 {
    pub const SYL_PTR: usize = 0x008;
    pub const CTX: usize = 0x0d6;
    pub const EDGE: usize = 0x2ca;
    pub const TEXT_PTR: usize = 0x330;
    pub const PH_PTR: usize = 0x334;
    pub const CODES: usize = 0x338;
    pub const MARKS: usize = 0x39c;
    pub const POS: usize = 0x400;
    pub const N_SYL: usize = 0x464;
    pub const N_PHONES: usize = 0x465;
    pub const X3AE: usize = 0x466;
    pub const TOKEN_CLASS: usize = 0x468;
    pub const SRC: usize = 0x46a;
    pub const CLASSCH: usize = 0x46b;
    pub const UNK46C: usize = 0x46c;
    pub const START: usize = 0x470;
    pub const END: usize = 0x474;
    pub const MINOR: usize = 0x478;
    pub const BRK: usize = 0x479;
}

byte_struct!(Ss09, SS_SIZE);

impl Ss09 {
    pub fn n_words(&self) -> usize {
        self.i16(2).max(0) as usize
    }

    pub fn n_phrases(&self) -> usize {
        self.i16(0).max(0) as usize
    }

    pub fn word(i: usize) -> usize {
        WORD_BASE + i * WORD_SIZE
    }

    pub fn phrase(g: usize) -> usize {
        0x0c + g * 0x10
    }
}

const TAG_MASK: u32 = 0xf000_0000;

fn deref_ss(p: u32) -> usize {
    (p & !TAG_MASK) as usize
}

fn is_special_first_char(c: u8) -> bool {
    let t1 = CH_CLASS_LO[(c ^ 0x80) as usize];
    !(t1 & 0x80 == 0 && (c < 0x80 || CH_CLASS_HI[c as usize] & 0x80 == 0))
}

/// Per-request text position state of `fe_NextSentence` (`FUN_10019b70`): the cursor and the two offset maps
/// (`map_b` for word starts, `map_c` for ends) over a text of `n` bytes.
pub struct TextPos09<'a> {
    pub cursor: i32,
    pub n: i32,
    pub map_start: &'a [i32],
    pub map_end: &'a [i32],
}

impl TextPos09<'_> {
    fn map(&self, m: &[i32], v: i32) -> i32 {
        let mut v = v;
        if self.n <= v {
            v = self.n - 1;
        }
        if v < 0 {
            v = 0;
        }
        m.get(v as usize).copied().unwrap_or(v)
    }

    fn span(&self, start: i32, end: i32) -> (i32, i32) {
        let s = start + self.cursor;
        let e = if start < end { end - 1 + self.cursor } else { s };
        (self.map(self.map_start, s), self.map(self.map_end, e))
    }
}

/// `FUN_10018e60` plus the word start/end writes of `FUN_10019b70`.
pub fn build_word_phone_records(ss: &mut Ss09, wa: &WaSpa, pos: &TextPos09) -> bool {
    for i in 0..ss.n_words() {
        let r = WaSpa::rec(i);
        let w = Ss09::word(i);
        let (s, e) = pos.span(wa.i32(r + war::START), wa.i32(r + war::END));
        ss.set_i32(w + w09::START, s);
        ss.set_i32(w + w09::END, e);
        ss.set_u32(w + w09::TEXT_PTR, ptr_wa(r + war::TEXT));
        ss.set_u32(w + w09::PH_PTR, ptr_wa(r + war2::PHONES));
        ss.set_i16(w + w09::TOKEN_CLASS, wa.i16(r + war2::CLASS_SRC));
        ss.set_i16(w + w09::X3AE, wa.i16(r + war2::UNK14));
        ss.set_u8(w + w09::SRC, wa.u8(r + war2::SRC_SRC));
        ss.set_u8(w + w09::UNK46C, wa.u8(r + war2::UNK19));
        ss.set_u8(w + w09::CLASSCH, is_special_first_char(wa.u8(r + war::TEXT)) as u8);

        let ph = wa.cstr(r + war2::PHONES);
        let len = ph.len().min(100);
        let mut n = 0usize;
        for &c in &ph[..len] {
            if c == b'd' {
                if n > 0 {
                    ss.set_u8(w + w09::MARKS + n - 1, b'1');
                }
            } else if c != b'c' {
                if (c as i8) < 1 || c > b'>' {
                    break;
                }
                ss.set_u8(w + w09::CODES + n, c);
                ss.set_u8(w + w09::MARKS + n, b'0');
                n += 1;
            }
        }
        ss.set_u8(w + w09::N_PHONES, n as u8);
    }
    true
}

/// `FUN_1001a390`.
pub fn syllabify(ss: &mut Ss09, wa: &WaSpa) {
    ss.set_i16(4, 0);
    let nw = ss.n_words();
    for i in 0..nw {
        let w = Ss09::word(i);
        let base = SYL_BASE + ss.i16(4) as usize * SYL_SIZE;
        ss.set_u32(w + w09::SYL_PTR, ptr_ss(base));
        ss.set_u8(w + w09::N_SYL, 0);
        ss.set_u32(base + syl::PHONES_PTR, ptr_ss(w + w09::CODES));
        ss.set_u32(base + syl::POS_PTR, ptr_ss(w + w09::POS));
        let ph = wa.cstr(deref_ss(ss.u32(w + w09::PH_PTR))).to_vec();
        let len = ph.len();
        let mut run: i32 = 0;
        let mut first_phone: u8 = 0;
        let mut n_syl: u8 = 0;
        for (k, &c) in ph.iter().enumerate() {
            if c == b'd' || c == b'c' {
                if run != 0 && k != len - 1 {
                    first_phone = first_phone.wrapping_add(run as u8);
                    ss.set_u8(base + n_syl as usize * SYL_SIZE + syl::N_PHONES, run as u8);
                    n_syl += 1;
                    let next = base + n_syl as usize * SYL_SIZE;
                    ss.set_u32(next + syl::PHONES_PTR, ptr_ss(w + w09::CODES + first_phone as usize));
                    ss.set_u32(next + syl::POS_PTR, ptr_ss(w + w09::POS + first_phone as usize));
                    run = 0;
                }
            } else {
                run += 1;
            }
        }
        ss.set_u8(base + n_syl as usize * SYL_SIZE + syl::N_PHONES, run as u8);
        n_syl += 1;
        ss.set_u8(w + w09::N_SYL, n_syl);
        ss.set_i16(4, ss.i16(4) + n_syl as i16);
    }
    for i in 0..nw {
        let w = Ss09::word(i);
        let n_syl = ss.u8(w + w09::N_SYL) as usize;
        let syl_base = deref_ss(ss.u32(w + w09::SYL_PTR));
        for g in 0..n_syl {
            let rec = syl_base + g * SYL_SIZE;
            let n = ss.u8(rec + syl::N_PHONES) as usize;
            ss.set_u8(rec, 0);
            let phones_off = deref_ss(ss.u32(rec + syl::PHONES_PTR));
            let pos_off = deref_ss(ss.u32(rec + syl::POS_PTR));
            let mut seen_nucleus = false;
            for k in 0..n {
                let code = ss.u8(phones_off + k);
                if sc(&PH_VOWEL, code) == 0 {
                    ss.set_u8(pos_off + k, if seen_nucleus { 3 } else { 1 });
                } else {
                    seen_nucleus = true;
                    ss.set_u8(pos_off + k, 2);
                    ss.set_u8(rec, sc(&PH_STRESS, code));
                }
            }
            let first_is_nuc = ss.u8(pos_off) == 2;
            let last_is_nuc = ss.u8(pos_off.wrapping_add(n.wrapping_sub(1))) == 2;
            let shape = match (first_is_nuc, last_is_nuc) {
                (true, true) => 0,
                (true, false) => 2,
                (false, true) => 1,
                (false, false) => 3,
            };
            ss.set_u8(rec + 1, shape);
            if g == 0 {
                ss.set_u8(syl_base + 2, 1);
            } else if g == n_syl - 1 {
                ss.set_u8(rec + 2, 3);
            } else {
                ss.set_u8(rec + 2, 2);
            }
        }
    }
}

/// `DAT_100632bc`.
const BREAK_OF_CLASS: &[u8] = b"IGFJGGGI\x00";

/// `FUN_1001a670`.
pub fn build_phrase_table(ss: &mut Ss09) {
    let nw = ss.n_words();
    if nw == 0 {
        ss.set_i16(0, 0);
        return;
    }
    ss.set_u32(Ss09::phrase(0) + 8, ptr_ss(Ss09::word(0)));
    ss.set_i16(Ss09::phrase(0), 0);
    ss.set_i16(Ss09::phrase(0) + 2, 0);
    let mut g = 0usize;
    for i in 0..nw - 1 {
        let e = Ss09::phrase(g);
        ss.set_i16(e + 2, ss.i16(e + 2).wrapping_add(1));
        let b = ss.u8(Ss09::word(i) + w09::BRK);
        if b != b'H' && b != b'I' {
            let next = Ss09::phrase(g + 1);
            ss.set_u32(next + 8, ptr_ss(Ss09::word(i + 1)));
            ss.set_i16(next, (i + 1) as i16);
            ss.set_i16(next + 2, 0);
            g += 1;
        }
    }
    let e = Ss09::phrase(g);
    ss.set_i16(e + 2, ss.i16(e + 2).wrapping_add(1));
    ss.set_i16(0, (g + 1) as i16);
    let mut syls = 0usize;
    for g in 0..=g {
        let e = Ss09::phrase(g);
        ss.set_u32(e + 0xc, ptr_ss(SYL_BASE + syls * SYL_SIZE));
        ss.set_i16(e + 4, 0);
        let first = ss.i16(e) as usize;
        let n = ss.i16(e + 2).max(0) as usize;
        for k in 0..n {
            let v = ss.i16(e + 4).wrapping_add(ss.u8(Ss09::word(first + k) + w09::N_SYL) as i16);
            ss.set_i16(e + 4, v);
        }
        syls += ss.i16(e + 4) as usize;
    }
}

/// `FUN_1001a760`.
pub fn predict_break_index(ss: &mut Ss09, spabi: &Tree) {
    for g in 0..ss.n_phrases() {
        let e = Ss09::phrase(g);
        let first = ss.i16(e) as usize;
        let nwp = ss.i16(e + 2);
        let nsp = ss.i16(e + 4);
        let mut words_from_end: i16 = 0;
        let mut syl_from_end: i32 = 0;
        let mut syl_since: i32 = 0;
        let mut words_since: i32 = 0;
        let mut k = nwp as i32 - 1;
        while k > 0 {
            let wi = first + k as usize;
            let w = Ss09::word(wi);
            let p = Ss09::word(wi - 1);
            words_from_end += 1;
            words_since += 1;
            let ns = ss.u8(w + w09::N_SYL) as i32;
            syl_from_end += ns;
            syl_since += ns;
            let fv: [i16; 14] = [
                nwp.wrapping_sub(words_from_end),
                words_from_end,
                nwp,
                nsp.wrapping_sub(syl_from_end as i16),
                syl_from_end as i16,
                nsp,
                words_since as i16,
                syl_since as i16,
                ss.u8(p + w09::CLASSCH) as i8 as i16,
                ss.u8(w + w09::CLASSCH) as i8 as i16,
                ss.u8(w + w09::UNK46C) as i16,
                ss.u8(p + w09::SRC) as i16,
                ss.u8(w + w09::SRC) as i16,
                ss.i16(p + w09::X3AE),
            ];
            if spabi.predict_scalar(&fv) > 500 {
                ss.set_u8(p + w09::BRK, b'H');
                syl_since = 0;
                words_since = 0;
            }
            k -= 1;
        }
    }
}

/// `FUN_1001a8f0`. False when forced breaking cannot fit the sentence under the 1000-half-phone limit.
pub fn assign_phrase_breaks(ss: &mut Ss09, tib: &mut Tib, wa: &WaSpa, spabi: &Tree) -> bool {
    syllabify(ss, wa);
    let nw = ss.n_words();
    for i in 0..nw {
        let w = Ss09::word(i);
        let cls = ss.i16(w + w09::TOKEN_CLASS);
        ss.set_u8(w + w09::BRK, BREAK_OF_CLASS.get(cls as usize).copied().unwrap_or(0));
    }
    if nw > 0 {
        let last = Ss09::word(nw - 1) + w09::BRK;
        if ss.u8(last) != b'J' {
            ss.set_u8(last, 0x46);
        }
    }
    build_phrase_table(ss);
    predict_break_index(ss, spabi);
    for i in 0..nw {
        ss.set_u8(Ss09::word(i) + w09::MINOR, 0);
    }
    if nw >= 1 {
        for i in 0..nw - 1 {
            let v = wa.i32(WaSpa::rec(i) + war2::PAUSE);
            if v == -2 {
                tib.pause[i + 1] = 100;
            } else if v >= 0 {
                tib.pause[i + 1] = v;
            }
        }
    }
    for i in 1..nw {
        if tib.pause[i] >= 0 {
            tib.brk_level[i - 1] = 2;
        }
    }
    let mut i = 0usize;
    if nw >= 1 {
        while i < nw - 1 {
            let w = Ss09::word(i);
            match tib.brk_level[i] {
                0 => {
                    ss.set_u8(w + w09::BRK, 0x49);
                    ss.set_u8(w + w09::MINOR, 1);
                }
                1 => {
                    ss.set_u8(w + w09::BRK, 0x48);
                    ss.set_u8(w + w09::MINOR, 1);
                }
                2 => ss.set_u8(w + w09::BRK, 0x47),
                3 => ss.set_u8(w + w09::BRK, 0x46),
                _ => {}
            }
            i += 1;
        }
    }
    let v = tib.brk_level[i];
    if (0..3).contains(&v) {
        ss.set_u8(Ss09::word(i) + w09::BRK, 0x47);
    }
    let mut u: i32 = 0;
    let mut acc: i32 = 0;
    let mut last_break: i32 = 0;
    while u < nw as i32 {
        acc += ss.u8(Ss09::word(u as usize) + w09::N_PHONES) as i32 * 2;
        if acc > 1000 {
            if u == last_break {
                return false;
            }
            u -= 1;
            ss.set_u8(Ss09::word(u as usize) + w09::BRK, b'G');
        }
        let b = ss.u8(Ss09::word(u as usize) + w09::BRK);
        if b == b'G' || b == b'F' || b == b'J' {
            last_break = u + 1;
            acc = 0;
        }
        u += 1;
    }
    build_phrase_table(ss);
    true
}

fn vowel(c: u8) -> bool {
    sc(&PH_VOWEL, c) != 0
}

/// `FUN_100191b0`: per-phone `{L, C, R, pos, pos}` contexts (the position byte is decimal-coded) and the
/// edge counters of minor-break words.
pub fn build_phone_contexts(ss: &mut Ss09) {
    let nw = ss.n_words();
    let nph = |ss: &Ss09, k: usize| ss.u8(Ss09::word(k) + w09::N_PHONES) as usize;
    let brk = |ss: &Ss09, k: usize| ss.u8(Ss09::word(k) + w09::BRK);
    let ctx = |k: usize, j: usize, b: usize| Ss09::word(k) + w09::CTX + 5 * j + b;
    for cur in 0..nw {
        let (left_code, left_pos);
        let mut m = cur as i32 - 1;
        let mut found = false;
        let (mut lc, mut lp) = (0x46u8, 3u8);
        while m >= 0 {
            let mk = m as usize;
            if brk(ss, mk) == b'G' || nph(ss, mk) != 0 {
                let b = brk(ss, mk);
                if b == b'F' || b == b'G' {
                    lc = 0x46;
                    lp = 3;
                } else {
                    lc = ss.u8(Ss09::word(mk) + w09::CODES + nph(ss, mk).wrapping_sub(1));
                    lp = (b != b'I') as u8 + 1;
                }
                found = true;
                break;
            }
            m -= 1;
        }
        if !found {
            lc = 0x46;
            lp = 3;
        }
        left_code = lc;
        left_pos = lp;

        let mut s = cur + 1;
        loop {
            let b = brk(ss, s - 1);
            if b == b'F' || b == b'J' || b == b'G' || nph(ss, s) != 0 {
                break;
            }
            s += 1;
        }
        let b = brk(ss, s - 1);
        let (right_code, right_pos) = match b {
            b'F' => (0x46u8, 4u8),
            b'J' => (0x4a, 5),
            b'G' => (0x47, 3),
            _ => (ss.u8(Ss09::word(s) + w09::CODES), (b != b'I') as u8 + 1),
        };

        let n = nph(ss, cur);
        let mut seq: Vec<u8> = Vec::with_capacity(n + 2);
        seq.push(left_code);
        for j in 0..n {
            seq.push(ss.u8(Ss09::word(cur) + w09::CODES + j));
        }
        seq.push(right_code);
        let marks = |ss: &Ss09, j: usize| ss.u8(Ss09::word(cur) + w09::MARKS + j);
        for j in 0..n {
            for b in 0..5 {
                ss.set_u8(ctx(cur, j, b), 0);
            }
            for b in 0..3 {
                ss.set_u8(ctx(cur, j, b), seq[j + b]);
            }
            let mut c = 0u8;
            if j == 0 {
                c = left_pos.wrapping_mul(10);
            }
            if j == n - 1 {
                c = c.wrapping_add(right_pos);
            }
            if j > 0 && marks(ss, j - 1) == b'1' {
                c = c.wrapping_add(10);
            }
            if j + 1 < n && marks(ss, j) == b'1' {
                c = c.wrapping_add(1);
            }
            ss.set_u8(ctx(cur, j, 3), c);
            ss.set_u8(ctx(cur, j, 4), c);
        }
        let head = ss.u8(ctx(cur, 0, 3)) / 10;
        if head > 2 && n > 1 && !vowel(ss.u8(ctx(cur, 0, 1))) {
            let mut j = 1usize;
            while j < n {
                let v = ss.u8(ctx(cur, j, 4)) % 10 + head * 10;
                ss.set_u8(ctx(cur, j, 4), v);
                if vowel(ss.u8(ctx(cur, j, 1))) {
                    break;
                }
                j += 1;
            }
        }
        if n > 1 {
            let tail = ss.u8(ctx(cur, n - 1, 3)) % 10;
            if tail > 2 && !vowel(ss.u8(ctx(cur, n - 1, 1))) {
                let mut j = n as i32 - 2;
                while j >= 0 {
                    let o = ctx(cur, j as usize, 4);
                    let v = (ss.u8(o) / 10).wrapping_mul(10).wrapping_add(tail);
                    ss.set_u8(o, v);
                    if vowel(ss.u8(ctx(cur, j as usize, 1))) {
                        break;
                    }
                    j -= 1;
                }
            }
        }
    }
    for k in 0..nw {
        for j in 0..nph(ss, k) {
            ss.set_u8(Ss09::word(k) + w09::EDGE + j, 0);
        }
    }
    if nw != 1 {
        for k in 0..nw.saturating_sub(1) {
            let w = Ss09::word(k);
            if ss.u8(w + w09::MINOR) != 1 {
                continue;
            }
            for j in (0..nph(ss, k)).rev() {
                let m = ss.u8(ctx(k, j, 4)) % 10;
                if m == 1 || m == 2 {
                    let o = w + w09::EDGE + j;
                    ss.set_u8(o, ss.u8(o).wrapping_add(1));
                }
            }
            let nn = nph(ss, k + 1);
            for j in 0..nn {
                let t = ss.u8(ctx(k + 1, j, 4)) / 10;
                if t == 1 || t == 2 {
                    let o = Ss09::word(k + 1) + w09::EDGE + j;
                    ss.set_u8(o, ss.u8(o).wrapping_add(10));
                }
            }
        }
    }
}
