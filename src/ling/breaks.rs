//! Phrase breaks: `fe_AssignPhraseBreaks` (0x100130e0), the phrase table
//! builder `FUN_10012df0` and the break-index CART `fe_PredictBreakIndexCart`
//! (0x10012f00, `engbi.tree3`).
//!
//! 2009 (julie09 `FUN_10012b30` / `FUN_100128d0` / `FUN_100129d0`; paul09 `FUN_10012ce0` / `FUN_10012a80` /
//! `FUN_10012b80`): word stride 0x300, '^' only for sentence type 5, no '`' break, a 13-feature CART call
//! without the character-gap guard and the two vowel features, threshold 400 (julie09) / 500 (paul09).

use super::lay::{Lay, L14};
use super::phtabs::PhTabs;
use super::tables::PH_VOWEL;
use super::{ptr_ss, sc, war, Ss, Tib, Wa, WordStrings};
use crate::era::Era;
use crate::tree3::Tree;

/// `FUN_10012df0`: rebuild the phrase table from the word break codes.
pub fn build_phrase_table(ss: &mut Ss) {
    build_phrase_table_lay(&L14, ss)
}

pub fn build_phrase_table_lay(lay: &Lay, ss: &mut Ss) {
    let nw = ss.n_words();
    if nw == 0 {
        ss.set_i16(0, 0);
        return;
    }
    ss.set_u32(Ss::phrase(0) + 8, ptr_ss(lay.word(0)));
    ss.set_i16(Ss::phrase(0), 0);
    ss.set_i16(Ss::phrase(0) + 2, 0);
    let mut g = 0usize;
    for i in 0..nw - 1 {
        let e = Ss::phrase(g);
        ss.set_i16(e + 2, ss.i16(e + 2).wrapping_add(1));
        let b = ss.u8(lay.word(i) + lay.brk);
        if b != b'\\' && b != b']' {
            let next = Ss::phrase(g + 1);
            ss.set_u32(next + 8, ptr_ss(lay.word(i + 1)));
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
        ss.set_u32(e + 0xc, ptr_ss(lay.syl_base + syls * super::SYL_SIZE));
        ss.set_i16(e + 4, 0);
        let first = ss.i16(e) as usize;
        let n = ss.i16(e + 2).max(0) as usize;
        for k in 0..n {
            let v = ss.i16(e + 4).wrapping_add(ss.u8(lay.word(first + k) + lay.n_syl) as i16);
            ss.set_i16(e + 4, v);
        }
        syls += ss.i16(e + 4) as usize;
    }
}

/// `fe_PredictBreakIndexCart`: within each phrase, walk the words from the end
/// and insert a minor break `\` after a word when the CART scores > 500.
pub fn predict_break_index(ss: &mut Ss, strs: &WordStrings, engbi: &Tree) {
    predict_break_index_lay(&L14, Era::V2014, ss, strs, engbi)
}

pub fn predict_break_index_lay(lay: &Lay, era: Era, ss: &mut Ss, strs: &WordStrings, engbi: &Tree) {
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
            let w = lay.word(wi);
            let p = lay.word(wi - 1);
            words_from_end += 1;
            words_since += 1;
            let ns = ss.u8(w + lay.n_syl) as i32;
            syl_from_end += ns;
            syl_since += ns;
            let prev_end = ss.i32(p + lay.end);
            if lay.is09 || (prev_end != ss.i32(w + lay.end) && prev_end + 1 != ss.i32(w + lay.start)) {
                let nph_prev = ss.u8(p + lay.n_phones) as usize;
                let mut fv: Vec<i16> = vec![
                    nwp.wrapping_sub(words_from_end),
                    words_from_end,
                    nwp,
                    nsp.wrapping_sub(syl_from_end as i16),
                    syl_from_end as i16,
                    nsp,
                    words_since as i16,
                    syl_since as i16,
                    ss.u8(w + lay.tag_b) as i16,
                    ss.u8(p + lay.tag_a) as i16,
                    ss.u8(w + lay.tag_a) as i16,
                    ss.i16(p + lay.x3ae),
                    (prev_end == ss.i32(w + lay.end)) as i16,
                ];
                if !lay.is09 {
                    fv.push(sc(&PH_VOWEL, strs.phone_byte_lay(lay, ss, wi, 0)) as i16);
                    fv.push(sc(&PH_VOWEL, strs.phone_byte_lay(lay, ss, wi - 1, nph_prev.wrapping_sub(1))) as i16);
                }
                let thresh = match era {
                    Era::Julie09 => 400,
                    _ => 500,
                };
                if engbi.predict_scalar(&fv) > thresh {
                    ss.set_u8(p + lay.brk, b'\\');
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
    assign_phrase_breaks_era(Era::V2014, ss, tib, wa, engbi)
}

pub fn assign_phrase_breaks_era(era: Era, ss: &mut Ss, tib: &mut Tib, wa: &Wa, engbi: &Tree) -> bool {
    let lay = Lay::of(era);
    let tabs = PhTabs::of(era);
    super::syllab::syllabify_lay(lay, tabs, ss);
    let nw = ss.n_words();
    for i in 0..nw {
        let w = lay.word(i);
        let cls = ss.i16(w + lay.token_class);
        let b = tabs.brk_of_class.get(cls as usize).copied().unwrap_or(0);
        ss.set_u8(w + lay.brk, b);
    }
    let last = lay.word(nw) - lay.stride + lay.brk;
    if ss.u8(last) != b'^' {
        ss.set_u8(last, b'Z');
    }
    let q_type = if lay.is09 { 5 } else { 6 };
    ss.set_u8(last, if ss.u8(lay.ss_type) == q_type { b'^' } else { b'Z' });
    build_phrase_table_lay(lay, ss);
    {
        let strs = WordStrings { wa };
        predict_break_index_lay(lay, era, ss, &strs, engbi);
    }
    for i in 0..nw {
        ss.set_u8(lay.word(i) + lay.minor, 0);
    }
    if nw >= 1 {
        for i in 0..nw - 1 {
            let w = lay.word(i);
            let b = ss.u8(w + lay.brk);
            if ss.u8(w + lay.src) == 8 || ss.u8(lay.word(i + 1) + lay.src) != 8 || (b != b']' && b != b'\\') {
                if ss.i16(w + lay.token_class) == 0xc {
                    ss.set_u8(w + lay.minor, 1);
                }
            } else {
                ss.set_u8(w + lay.brk, b'\\');
                ss.set_u8(w + lay.minor, 1);
            }
        }
        for i in 0..nw - 1 {
            let v = wa.i32(Wa::rec(i) + war::PAUSE);
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
            let w = lay.word(i);
            match tib.brk_level[i] {
                0 => {
                    ss.set_u8(w + lay.brk, b']');
                    ss.set_u8(w + lay.minor, 1);
                }
                1 => {
                    ss.set_u8(w + lay.brk, b'\\');
                    ss.set_u8(w + lay.minor, 1);
                }
                2 => ss.set_u8(w + lay.brk, b'['),
                3 => ss.set_u8(w + lay.brk, b'Z'),
                _ => {}
            }
            i += 1;
        }
    }
    let v = tib.brk_level[i];
    if (0..3).contains(&v) {
        ss.set_u8(lay.word(i) + lay.brk, b'[');
    }
    let mut u: i32 = 0;
    let mut acc: i32 = 0;
    let mut last_break: i32 = 0;
    while u < nw as i32 {
        acc += ss.u8(lay.word(u as usize) + lay.n_phones) as i32 * 2;
        if acc > 1000 {
            if u == last_break {
                return false;
            }
            u -= 1;
            ss.set_u8(lay.word(u as usize) + lay.brk, b'[');
        }
        let b = ss.u8(lay.word(u as usize) + lay.brk);
        if b == b'[' || b == b'Z' || b == b'^' || (!lay.is09 && b == b'`') {
            last_break = u + 1;
            acc = 0;
        }
        u += 1;
    }
    build_phrase_table_lay(lay, ss);
    true
}
