//! Phrase breaks: `fe_AssignPhraseBreaks` (0x100130e0), the phrase table
//! builder `FUN_10012df0` and the break-index CART `fe_PredictBreakIndexCart`
//! (0x10012f00, `engbi.tree3`).

use super::tables::{BREAK_OF_CLASS, PH_VOWEL};
use super::{ptr_ss, sc, wr, Ss, Tib, Wa, WordStrings, SS_TYPE, SYL_BASE, SYL_SIZE};
use crate::tree3::Tree;

/// `FUN_10012df0`: rebuild the phrase table from the word break codes.
pub fn build_phrase_table(ss: &mut Ss) {
    let nw = ss.n_words();
    if nw == 0 {
        ss.set_i16(0, 0);
        return;
    }
    ss.set_u32(Ss::phrase(0) + 8, ptr_ss(Ss::word(0)));
    ss.set_i16(Ss::phrase(0), 0);
    ss.set_i16(Ss::phrase(0) + 2, 0);
    let mut g = 0usize;
    for i in 0..nw - 1 {
        let e = Ss::phrase(g);
        ss.set_i16(e + 2, ss.i16(e + 2).wrapping_add(1));
        let b = ss.u8(Ss::word(i) + wr::BRK);
        if b != b'\\' && b != b']' {
            let next = Ss::phrase(g + 1);
            ss.set_u32(next + 8, ptr_ss(Ss::word(i + 1)));
            ss.set_i16(next, (i + 1) as i16);
            ss.set_i16(next + 2, 0);
            g += 1;
        }
    }
    let e = Ss::phrase(g);
    ss.set_i16(e + 2, ss.i16(e + 2).wrapping_add(1));
    ss.set_i16(0, (g + 1) as i16);
    let mut syls = 0usize;
    for g in 0..=g {
        let e = Ss::phrase(g);
        ss.set_u32(e + 0xc, ptr_ss(SYL_BASE + syls * SYL_SIZE));
        ss.set_i16(e + 4, 0);
        let first = ss.i16(e) as usize;
        let n = ss.i16(e + 2).max(0) as usize;
        for k in 0..n {
            let v = ss.i16(e + 4).wrapping_add(ss.u8(Ss::word(first + k) + wr::N_SYL) as i16);
            ss.set_i16(e + 4, v);
        }
        syls += ss.i16(e + 4) as usize;
    }
}

/// `fe_PredictBreakIndexCart`: within each phrase, walk the words from the end
/// and insert a minor break `\` after a word when the CART scores > 500.
pub fn predict_break_index(ss: &mut Ss, strs: &WordStrings, engbi: &Tree) {
    let np = ss.n_phrases();
    for g in 0..np {
        let e = Ss::phrase(g);
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
            let w = Ss::word(wi);
            let p = Ss::word(wi - 1);
            words_from_end += 1;
            words_since += 1;
            let ns = ss.u8(w + wr::N_SYL) as i32;
            syl_from_end += ns;
            syl_since += ns;
            let prev_end = ss.i32(p + wr::END);
            if prev_end != ss.i32(w + wr::END) && prev_end + 1 != ss.i32(w + wr::START) {
                let nph_prev = ss.u8(p + wr::N_PHONES) as usize;
                let fv: [i16; 15] = [
                    nwp.wrapping_sub(words_from_end),
                    words_from_end,
                    nwp,
                    nsp.wrapping_sub(syl_from_end as i16),
                    syl_from_end as i16,
                    nsp,
                    words_since as i16,
                    syl_since as i16,
                    ss.u8(w + wr::TAG_B) as i16,
                    ss.u8(p + wr::TAG_A) as i16,
                    ss.u8(w + wr::TAG_A) as i16,
                    ss.i16(p + wr::X3AE),
                    (prev_end == ss.i32(w + wr::END)) as i16,
                    sc(&PH_VOWEL, strs.phone_byte(ss, wi, 0)) as i16,
                    sc(&PH_VOWEL, strs.phone_byte(ss, wi - 1, nph_prev.wrapping_sub(1))) as i16,
                ];
                if engbi.predict_scalar(&fv) > 500 {
                    ss.set_u8(p + wr::BRK, b'\\');
                    syl_since = 0;
                    words_since = 0;
                }
            }
            k -= 1;
        }
    }
}

/// `fe_AssignPhraseBreaks`. `wa` supplies the per-word pause overrides
/// (record +0x0c). Returns false when forced breaking cannot fit the
/// sentence under the 1000-half-phone limit (the engine returns 0).
pub fn assign_phrase_breaks(ss: &mut Ss, tib: &mut Tib, wa: &Wa, engbi: &Tree) -> bool {
    super::syllab::syllabify(ss);
    let nw = ss.n_words();
    for i in 0..nw {
        let w = Ss::word(i);
        let cls = ss.i16(w + wr::TOKEN_CLASS);
        let b = BREAK_OF_CLASS.get(cls as usize).copied().unwrap_or(0);
        ss.set_u8(w + wr::BRK, b);
    }
    let last = Ss::word(nw) - 0x3c0 + wr::BRK;
    if ss.u8(last) != b'^' {
        ss.set_u8(last, b'Z');
    }
    ss.set_u8(last, if ss.u8(SS_TYPE) == 6 { b'^' } else { b'Z' });
    build_phrase_table(ss);
    {
        let strs = WordStrings { wa };
        predict_break_index(ss, &strs, engbi);
    }
    for i in 0..nw {
        ss.set_u8(Ss::word(i) + wr::MINOR, 0);
    }
    if nw >= 1 {
        for i in 0..nw - 1 {
            let w = Ss::word(i);
            let b = ss.u8(w + wr::BRK);
            if ss.u8(w + wr::SRC) == 8 || ss.u8(Ss::word(i + 1) + wr::SRC) != 8 || (b != b']' && b != b'\\') {
                if ss.i16(w + wr::TOKEN_CLASS) == 0xc {
                    ss.set_u8(w + wr::MINOR, 1);
                }
            } else {
                ss.set_u8(w + wr::BRK, b'\\');
                ss.set_u8(w + wr::MINOR, 1);
            }
        }
        for i in 0..nw - 1 {
            let v = wa.i32(Wa::rec(i) + super::war::PAUSE);
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
            let w = Ss::word(i);
            match tib.brk_level[i] {
                0 => {
                    ss.set_u8(w + wr::BRK, b']');
                    ss.set_u8(w + wr::MINOR, 1);
                }
                1 => {
                    ss.set_u8(w + wr::BRK, b'\\');
                    ss.set_u8(w + wr::MINOR, 1);
                }
                2 => ss.set_u8(w + wr::BRK, b'['),
                3 => ss.set_u8(w + wr::BRK, b'Z'),
                _ => {}
            }
            i += 1;
        }
    }
    let v = tib.brk_level[i];
    if (0..3).contains(&v) {
        ss.set_u8(Ss::word(i) + wr::BRK, b'[');
    }
    let mut u: i32 = 0;
    let mut acc: i32 = 0;
    let mut last_break: i32 = 0;
    while u < nw as i32 {
        acc += ss.u8(Ss::word(u as usize) + wr::N_PHONES) as i32 * 2;
        if acc > 1000 {
            if u == last_break {
                return false;
            }
            u -= 1;
            ss.set_u8(Ss::word(u as usize) + wr::BRK, b'[');
        }
        let b = ss.u8(Ss::word(u as usize) + wr::BRK);
        if b == b'[' || b == b'Z' || b == b'^' || b == b'`' {
            last_break = u + 1;
            acc = 0;
        }
        u += 1;
    }
    build_phrase_table(ss);
    true
}
