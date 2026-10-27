//! The 2004 sentence structure (`TIB+0x48`, 0x52880 bytes) and the passes that fill it after the
//! linguistic analysis: `fe_BuildWordPhoneRecords` (0x1000ff80), `fe_AssignPhraseBreaks` (0x1000ed80, with
//! the syllabifier 0x1000eb60 and the phrase table 0x1000ec90) and `fe_BuildPhoneContexts` (0x100194f0,
//! with the allophone pass 0x100192e0). Word records are 0x2fc bytes from +0x64c, syllables 0x28 bytes from
//! +0x130bc, phrases 0x10 bytes from +0x0c. The phone tables are paul09's (`ling::tables09::P09`) for every
//! code the 2004 engine produces.

use super::{war, Wa04};
use crate::ling::byte_struct;
use crate::ling::phtabs::PhTabs;
use crate::ling::syllab::plan;
use crate::ling::tables09::P09;
use crate::ling::{ptr_ss, ptr_wa, sc};

pub const SS_SIZE: usize = 0x52880;
pub const WORD_BASE: usize = 0x64c;
pub const WORD_SIZE: usize = 0x2fc;
pub const SYL_BASE: usize = 0x130bc;
pub const SYL_SIZE: usize = 0x28;
pub const PHRASE_BASE: usize = 0x0c;
pub const PHRASE_SIZE: usize = 0x10;
/// Set when the sentence ends in '?' and the wh pass found no question word.
pub const Q_FLAG: usize = 0x5286c;

byte_struct!(Ss04, SS_SIZE);

/// Word record fields.
pub mod wd {
    pub const SYL_PTR: usize = 0x00;
    pub const TEXT_PTR: usize = 0x04;
    pub const PH_PTR: usize = 0x08;
    pub const CODES: usize = 0x0c;
    pub const MARKS: usize = 0x4d;
    pub const SYLPOS: usize = 0x8e;
    pub const N_SYL: usize = 0xd0;
    pub const N_PHONES: usize = 0xd2;
    pub const N_MORPH: usize = 0xd4;
    pub const POS_PTR: usize = 0xd8;
    pub const START: usize = 0xdc;
    pub const END: usize = 0xe0;
    pub const DUR: usize = 0xee;
    /// 5-byte phone labels `{L, C, R, flags, q}`.
    pub const CTX: usize = 0x170;
    pub const EDGE: usize = 0x2b5;
    pub const PUNCT: usize = 0x2f6;
    pub const U_FLAG: usize = 0x2f7;
    pub const MINOR: usize = 0x2f8;
    pub const BRK: usize = 0x2f9;
}

/// Syllable record fields.
pub mod sy {
    pub const STRESS: usize = 0x00;
    pub const SHAPE: usize = 0x01;
    pub const POSITION: usize = 0x02;
    pub const F0: usize = 0x04;
    pub const CODES_PTR: usize = 0x1c;
    pub const POS_PTR: usize = 0x20;
    pub const N_PHONES: usize = 0x24;
}

/// Phrase table entry fields.
pub mod ph {
    pub const FIRST: usize = 0x00;
    pub const N_WORDS: usize = 0x02;
    pub const N_SYL: usize = 0x04;
    pub const WORD_PTR: usize = 0x08;
    pub const SYL_PTR: usize = 0x0c;
}

impl Ss04 {
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
        PHRASE_BASE + g * PHRASE_SIZE
    }

    pub fn n_phones(&self, i: usize) -> usize {
        self.i16(Ss04::word(i) + wd::N_PHONES).max(0) as usize
    }

    fn brk(&self, i: isize) -> u8 {
        self.u8((Ss04::word(0) as isize + i * WORD_SIZE as isize + wd::BRK as isize) as usize)
    }
}

fn tabs() -> &'static PhTabs {
    &P09
}

/// `fe_BuildWordPhoneRecords`: phones and boundary marks from the merged records, the token's text,
/// phone and POS pointers, its morph count and U flag, the final punctuation per word, and the
/// question flag.
pub fn build_words(ss: &mut Ss04, wa: &Wa04) {
    let nw = ss.n_words();
    for i in 0..nw {
        ss.set_u32(Ss04::word(i) + wd::TEXT_PTR, ptr_wa(Wa04::rec(i) + war::TEXT));
    }
    for i in 0..nw {
        let r = Wa04::rec(i);
        let w = Ss04::word(i);
        ss.set_i16(w + wd::N_MORPH, wa.i16(r + 0x0c));
        ss.set_u32(w + wd::POS_PTR, ptr_wa(r + war::FORCED_POS));
        ss.set_u32(w + wd::PH_PTR, ptr_wa(r + war::PHONES));
        ss.set_u8(w + wd::U_FLAG, wa.u8(r + war::U_FLAG));
        let ph = wa.cstr(r + war::PHONES).to_vec();
        let mut n = 0usize;
        for &c in &ph[..ph.len().min(0x41)] {
            if c == b'd' {
                if n > 0 {
                    ss.set_u8(w + wd::MARKS + n - 1, b'1');
                }
                continue;
            }
            if (c as i8) < 1 || c > b'E' {
                break;
            }
            ss.set_u8(w + wd::CODES + n, c);
            ss.set_u8(w + wd::MARKS + n, b'0');
            n += 1;
        }
        ss.set_i16(w + wd::N_PHONES, n as i16);
    }
    for i in 0..nw {
        let w = Ss04::word(i);
        let t = Wa04::rec(i) + war::TEXT;
        let len = wa.cstr(t).len();
        let last = wa.u8(t + len - 1);
        ss.set_u8(
            w + wd::PUNCT,
            match last {
                b',' => 1,
                b'.' => 2,
                b'!' => 3,
                b'?' => 4,
                _ => 0,
            },
        );
    }
    let last_punct = ss.u8((Ss04::word(0) as isize + (nw as isize - 1) * WORD_SIZE as isize + wd::PUNCT as isize) as usize);
    ss.set_u8(Q_FLAG, (last_punct == 4 && wa.i16(8) == 0) as u8);
}

/// `fe_AssignPhraseBreaks`. `brk_level` is the TIB per-word break array (`TIB+0x111ed8`); word `k` reads
/// entry `k + 1`. False when a run of words without a break exceeds 1000 half-phones and cannot be split.
pub fn assign_breaks(ss: &mut Ss04, brk_level: &[i32]) -> bool {
    let nw = ss.n_words();
    let lvl = |k: usize| brk_level.get(k + 1).copied().unwrap_or(0);
    let set_brk = |ss: &mut Ss04, i: usize, b: u8| ss.set_u8(Ss04::word(i) + wd::BRK, b);
    for i in 0..nw.saturating_sub(1) {
        let b = if ss.u8(Ss04::word(i) + wd::PUNCT) != 1 { b'\\' } else { b'[' };
        set_brk(ss, i, b);
    }
    let last = if ss.u8(Q_FLAG) == 0 { b'Z' } else { b'^' };
    ss.set_u8((Ss04::word(0) as isize + (nw as isize - 1) * WORD_SIZE as isize + wd::BRK as isize) as usize, last);
    for k in 0..nw.saturating_sub(1) {
        if lvl(k) == 2 {
            set_brk(ss, k, b'[');
        }
    }
    let (mut total, mut i, mut start) = (0i32, 0usize, 0usize);
    while i < nw {
        total += ss.n_phones(i) as i32 * 2;
        if total > 1000 {
            if i == start {
                return false;
            }
            i -= 1;
            set_brk(ss, i, b'[');
        }
        let c = ss.brk(i as isize);
        if c == b'[' || c == b'Z' || c == b'^' {
            total = 0;
            start = i + 1;
        }
        i += 1;
    }
    syllabify(ss);
    phrase_table(ss);
    for i in 0..nw {
        ss.set_u8(Ss04::word(i) + wd::MINOR, 0);
    }
    for k in 0..nw.saturating_sub(1) {
        match lvl(k) {
            0 => {
                set_brk(ss, k, b']');
                ss.set_u8(Ss04::word(k) + wd::MINOR, 1);
            }
            1 => {
                set_brk(ss, k, b'\\');
                ss.set_u8(Ss04::word(k) + wd::MINOR, 1);
            }
            2 => set_brk(ss, k, b'['),
            _ => {}
        }
    }
    true
}

/// `FUN_1000f9b0`: one boundary group of `n` phones into syllable records from `rec`; positions (1 onset,
/// 2 nucleus, 3 coda) into the word's position bytes. A group without a vowel is one syllable.
fn group(ss: &mut Ss04, idx: &[u8], stress: &[u8], n: usize, rec: usize, codes: usize, pos: usize) -> usize {
    let p = plan(tabs(), idx, stress, n as i32);
    let ns = (p.n.max(0) as usize).min(0x41);
    if ns == 0 {
        ss.set_u32(rec + sy::CODES_PTR, ptr_ss(codes));
        ss.set_i16(rec + sy::N_PHONES, n as i16);
        ss.set_u32(rec + sy::POS_PTR, ptr_ss(pos));
        for k in 0..n {
            ss.set_u8(pos + k, 1);
        }
        ss.set_u8(rec + sy::STRESS, 0);
        ss.set_u8(rec + sy::SHAPE, 4);
        ss.set_u8(rec + sy::POSITION, 1);
        return 1;
    }
    let mut off = 0usize;
    for k in 1..=ns {
        let e = p.syl[k - 1];
        let r = rec + (k - 1) * SYL_SIZE;
        ss.set_u32(r + sy::CODES_PTR, ptr_ss(codes + off));
        ss.set_u32(r + sy::POS_PTR, ptr_ss(pos + off));
        let mut cx = 0usize;
        let mut s = e[1] as i16;
        while (s as i32) < e[0] {
            ss.set_u8(pos + off + cx, 1);
            cx += 1;
            s = s.wrapping_add(1);
        }
        ss.set_u8(pos + off + cx, 2);
        cx += 1;
        let mut s = (e[0] as i16).wrapping_add(1);
        while s as i32 <= e[2] {
            ss.set_u8(pos + off + cx, 3);
            cx += 1;
            s = s.wrapping_add(1);
        }
        ss.set_u8(r + sy::STRESS, e[3] as u8);
        let first_nuc = ss.u8(pos + off) == 2;
        let last_nuc = ss.u8(pos + off + cx - 1) == 2;
        ss.set_u8(
            r + sy::SHAPE,
            match (first_nuc, last_nuc) {
                (true, true) => 0,
                (true, false) => 2,
                (false, true) => 1,
                (false, false) => 3,
            },
        );
        ss.set_i16(r + sy::N_PHONES, (cx & 0xff) as i16);
        off += cx;
        if k == ns {
            ss.set_u8(r + sy::POSITION, 3);
        } else if k == 1 {
            ss.set_u8(rec + sy::POSITION, 1);
        } else {
            ss.set_u8(r + sy::POSITION, 2);
        }
    }
    ns
}

/// `FUN_1000eb60`: syllabify every word, one group per boundary mark.
fn syllabify(ss: &mut Ss04) {
    let t = tabs();
    ss.set_i16(4, 0);
    for i in 0..ss.n_words() {
        let w = Ss04::word(i);
        let base = SYL_BASE + ss.i16(4) as usize * SYL_SIZE;
        ss.set_u32(w + wd::SYL_PTR, ptr_ss(base));
        let n = ss.i16(w + wd::N_PHONES).max(0) as usize;
        let mut idx = vec![0u8; n.max(1)];
        let mut st = vec![0u8; n.max(1)];
        for k in 0..n {
            let c = ss.u8(w + wd::CODES + k);
            idx[k] = sc(t.base, c);
            st[k] = sc(t.stress, c);
        }
        ss.set_i16(w + wd::N_SYL, 0);
        let mut gs = 0usize;
        for k in 0..n {
            if ss.u8(w + wd::MARKS + k) == b'1' || k == n - 1 {
                let nsyl = ss.i16(w + wd::N_SYL) as usize;
                let added = group(ss, &idx[gs..], &st[gs..], k - gs + 1, base + nsyl * SYL_SIZE, w + wd::CODES + gs, w + wd::SYLPOS + gs);
                ss.set_i16(w + wd::N_SYL, (nsyl + added) as i16);
                gs = k + 1;
            }
        }
        let tot = ss.i16(4) + ss.i16(w + wd::N_SYL);
        ss.set_i16(4, tot);
    }
}

/// `FUN_1000ec90`: phrases end at every break other than '\\' and ']'.
fn phrase_table(ss: &mut Ss04) {
    let nw = ss.n_words();
    if nw == 0 {
        ss.set_i16(0, 0);
        return;
    }
    ss.set_u32(Ss04::phrase(0) + ph::WORD_PTR, ptr_ss(Ss04::word(0)));
    ss.set_i16(Ss04::phrase(0) + ph::FIRST, 0);
    ss.set_i16(Ss04::phrase(0) + ph::N_WORDS, 0);
    let mut g = 0usize;
    for i in 0..nw - 1 {
        let e = Ss04::phrase(g) + ph::N_WORDS;
        ss.set_i16(e, ss.i16(e) + 1);
        let b = ss.u8(Ss04::word(i) + wd::BRK);
        if b != b'\\' && b != b']' {
            g += 1;
            let p = Ss04::phrase(g);
            ss.set_u32(p + ph::WORD_PTR, ptr_ss(Ss04::word(i + 1)));
            ss.set_i16(p + ph::FIRST, (i + 1) as i16);
            ss.set_i16(p + ph::N_WORDS, 0);
        }
    }
    let e = Ss04::phrase(g) + ph::N_WORDS;
    ss.set_i16(e, ss.i16(e) + 1);
    ss.set_i16(0, (g + 1) as i16);
    let mut syl = 0usize;
    for g in 0..=g {
        let p = Ss04::phrase(g);
        ss.set_u32(p + ph::SYL_PTR, ptr_ss(SYL_BASE + syl * SYL_SIZE));
        let first = ss.i16(p + ph::FIRST).max(0) as usize;
        let mut n = 0i16;
        for j in 0..ss.i16(p + ph::N_WORDS).max(0) as usize {
            n = n.wrapping_add(ss.i16(Ss04::word(first + j) + wd::N_SYL));
        }
        ss.set_i16(p + ph::N_SYL, n);
        syl += n as usize;
    }
}

const T: u8 = 0x39;
const D: u8 = 0x15;
const K: u8 = 0x2a;
const P: u8 = 0x35;
const S: u8 = 0x37;
const SH: u8 = 0x38;
const R: u8 = 0x36;
const Y: u8 = 0x43;

fn vowel(c: u8) -> bool {
    sc(tabs().vowel, c) == 1
}

/// `FUN_10019230`: stress of the vowel before phone `k` (or before an R there), -1 otherwise.
fn prev_stress(ss: &Ss04, w: usize, k: usize) -> i16 {
    if k > 0 {
        let c = ss.u8(w + wd::CODES + k - 1);
        if vowel(c) {
            return sc(tabs().stress, c) as i16;
        }
        if c == R && k > 1 {
            let c2 = ss.u8(w + wd::CODES + k - 2);
            if vowel(c2) {
                return sc(tabs().stress, c2) as i16;
            }
        }
    }
    -1
}

/// `FUN_10019280`: stress of the vowel after phone `k` (or after a Y there), -1 otherwise.
fn next_stress(ss: &Ss04, w: usize, k: usize) -> i16 {
    let n = ss.i16(w + wd::N_PHONES) as i32;
    if (k as i32) < n - 1 {
        let c = ss.u8(w + wd::CODES + k + 1);
        if vowel(c) {
            return sc(tabs().stress, c) as i16;
        }
        if c == Y && (k as i32) < n - 2 {
            let c2 = ss.u8(w + wd::CODES + k + 2);
            if vowel(c2) {
                return sc(tabs().stress, c2) as i16;
            }
        }
    }
    -1
}

/// `FUN_100192e0`: word-internal allophones (glottal and unreleased T, flaps, unaspirated K / P).
fn allophones(ss: &mut Ss04) {
    for i in 0..ss.n_words() {
        let w = Ss04::word(i);
        let n = ss.i16(w + wd::N_PHONES) as i32;
        let code = |ss: &Ss04, k: i32| ss.u8((w as i32 + wd::CODES as i32 + k) as usize);
        let mark = |ss: &Ss04, k: i32| ss.u8((w as i32 + wd::MARKS as i32 + k) as usize);
        let mut k = 1;
        while k < n - 1 {
            if mark(ss, k - 1) != b'1' && mark(ss, k) != b'1' {
                let sv = prev_stress(ss, w, k as usize);
                let nv = next_stress(ss, w, k as usize);
                let at = w + wd::CODES + k as usize;
                let c = ss.u8(at);
                if c == T {
                    let pc = code(ss, k - 1);
                    if pc == S && nv >= 0 {
                        ss.set_u8(at, 0x48);
                    } else if matches!(pc, 0x20 | K | P | SH) && nv == 0 {
                        ss.set_u8(at, 0x48);
                    } else if sv < 1 || n - 2 <= k || mark(ss, k + 1) != b'0' || code(ss, k + 1) != 7 || code(ss, k + 2) != 0x2d {
                        if sv >= 0 {
                            if nv == 0 {
                                ss.set_u8(at, 0x4d);
                            } else if matches!(code(ss, k + 1), 0x2b | 0x2c | 0x2d) {
                                ss.set_u8(at, 0x4b);
                            }
                        }
                    } else {
                        ss.set_u8(at, 0x4b);
                    }
                } else if (c == K || c == P) && nv >= 0 && (code(ss, k - 1) == S || nv == 0) {
                    ss.set_u8(at, if c == K { 0x46 } else { 0x47 });
                } else if c == D && sv >= 0 && nv == 0 {
                    ss.set_u8(at, 0x4e);
                }
            }
            k += 1;
        }
        let mut k = 0;
        while k < n - 1 {
            if mark(ss, k) != b'1' && code(ss, k) == T && next_stress(ss, w, k as usize) > 0 {
                ss.set_u8(w + wd::CODES + k as usize, 0x4c);
            }
            k += 1;
        }
    }
}

/// `fe_BuildPhoneContexts`: allophones, then per word the 5-byte labels `{L, C, R, flags, q}` with the
/// neighbouring words' edge phones or break codes, the boundary flags pushed through the consonants up
/// to the next vowel, and the edge counters before minor breaks.
pub fn build_contexts(ss: &mut Ss04) {
    allophones(ss);
    let nw = ss.n_words();
    for i in 0..nw {
        let (lc, left) = {
            let mut found = None;
            let mut j = i as isize - 1;
            while j >= 0 {
                let ju = j as usize;
                if ss.brk(j) == b'[' || ss.i16(Ss04::word(ju) + wd::N_PHONES) > 0 {
                    found = Some(ju);
                    break;
                }
                j -= 1;
            }
            match found {
                None => (0x5a, 3u8),
                Some(j) => {
                    let b = ss.brk(j as isize);
                    if b == b'[' {
                        (0x5a, 3)
                    } else {
                        let n = ss.n_phones(j);
                        (ss.u8(Ss04::word(j) + wd::CODES + n - 1), (b != b']') as u8 + 1)
                    }
                }
            }
        };
        let mut j = i + 1;
        loop {
            let b = ss.brk(j as isize - 1);
            if b == b'Z' || b == b'^' || b == b'[' || ss.i16(Ss04::word(j) + wd::N_PHONES) >= 1 {
                break;
            }
            j += 1;
        }
        let (rc, right) = match ss.brk(j as isize - 1) {
            b'Z' => (0x5a, 4u8),
            b'^' => (0x5e, 5),
            b'[' => (0x5b, 3),
            b => (ss.u8(Ss04::word(j) + wd::CODES), (b != b']') as u8 + 1),
        };
        let w = Ss04::word(i);
        let n = ss.n_phones(i);
        let mut win = vec![lc];
        for k in 0..n {
            win.push(ss.u8(w + wd::CODES + k));
        }
        win.push(rc);
        for k in 0..n {
            let o = w + wd::CTX + 5 * k;
            ss.set_u8(o, win[k]);
            ss.set_u8(o + 1, win[k + 1]);
            ss.set_u8(o + 2, win[k + 2]);
            let mut f: u8 = if k == 0 { left.wrapping_mul(10) } else { 0 };
            if k + 1 == n {
                f = f.wrapping_add(right);
            }
            if k > 0 && ss.u8(w + wd::MARKS + k - 1) == b'1' {
                f = f.wrapping_add(10);
            }
            if k + 1 < n && ss.u8(w + wd::MARKS + k) == b'1' {
                f = f.wrapping_add(1);
            }
            ss.set_u8(o + 3, f);
            ss.set_u8(o + 4, f);
        }
        let lab = |k: usize| w + wd::CTX + 5 * k;
        let b = ss.u8(lab(0) + 3) / 10;
        if b > 2 && n > 1 && !vowel(ss.u8(lab(0) + 1)) {
            for k in 1..n {
                let q = ss.u8(lab(k) + 4) % 10 + b.wrapping_mul(10);
                ss.set_u8(lab(k) + 4, q);
                if vowel(ss.u8(lab(k) + 1)) {
                    break;
                }
            }
        }
        if n >= 1 {
            let last = n - 1;
            let b = ss.u8(lab(last) + 3) % 10;
            if last > 0 && b > 2 && !vowel(ss.u8(lab(last) + 1)) {
                let mut k = last as isize - 1;
                while k >= 0 {
                    let o = lab(k as usize);
                    ss.set_u8(o + 4, (ss.u8(o + 4) / 10).wrapping_mul(10).wrapping_add(b));
                    if vowel(ss.u8(o + 1)) {
                        break;
                    }
                    k -= 1;
                }
            }
        }
    }
    for i in 0..nw {
        let w = Ss04::word(i);
        for k in 0..ss.n_phones(i) {
            ss.set_u8(w + wd::EDGE + k, 0);
        }
    }
    for i in 0..nw.saturating_sub(1) {
        let w = Ss04::word(i);
        if ss.u8(w + wd::MINOR) != 1 {
            continue;
        }
        for k in (0..ss.n_phones(i)).rev() {
            let r = ss.u8(w + wd::CTX + 5 * k + 4) % 10;
            if r == 1 || r == 2 {
                let o = w + wd::EDGE + k;
                ss.set_u8(o, ss.u8(o).wrapping_add(1));
            }
        }
        let nx = Ss04::word(i + 1);
        for k in 0..ss.n_phones(i + 1) {
            let l = ss.u8(nx + wd::CTX + 5 * k + 4) / 10;
            if l == 1 || l == 2 {
                let o = nx + wd::EDGE + k;
                ss.set_u8(o, ss.u8(o).wrapping_add(10));
            }
        }
    }
}
