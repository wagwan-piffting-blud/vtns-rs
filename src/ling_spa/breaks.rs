//! Phrase breaks: `fe_AssignPhraseBreaks` (0x1001e750), its phrase-table
//! builder `FUN_1001e4d0` and its break-index CART `FUN_1001e5c0`
//! (`spabi.tree3`). English counterparts: `0x100130e0`/(inlined)/
//! `fe_PredictBreakIndexCart` `0x10012f00` (`src\ling\breaks.rs`).
//!
//! Break-level byte codes are the English set shifted -0x14 (confirmed
//! from the decompile's literal switch cases, matching `VIOLETA_TODO.md`):
//! English `Z [ \ ] ^` (0x5a 0x5b 0x5c 0x5d 0x5e) -> Violeta `F G H I J`
//! (0x46 0x47 0x48 0x49 0x4a). Both languages also have one extra code
//! outside that shift for "already forced" breaks (English `'a'`=0x61,
//! Violeta `'K'`=0x4b -- *not* `'a'-0x14`, an independently-chosen byte).
//!
//! Confirmed structural differences from English:
//!   * `predict_break_index`'s CART feature vector is 14 values (English:
//!     15) and reads six *raw* per-word byte/word fields for the last six
//!     features instead of computing TAG_A/TAG_B/vowel-class/"real gap"
//!     values -- see the field list below.
//!   * No "real character gap" guard: every adjacent word pair in a phrase
//!     is scored, unconditionally.
//!   * `assign_phrase_breaks` has no equivalent of English's `SRC==8`
//!     minor-merge loop between the MINOR-zeroing pass and the pause-
//!     override pass -- confirmed absent from the decompile, not missed.
//!   * The final "did we run out of forced breaks" scan checks only three
//!     break bytes (`F`/`G`/`J`), not English's four (missing the
//!     shifted-`` ` `` reset case).
//!   * The last-word default break isn't SS_TYPE-dependent: it's simply
//!     "force `F` unless already `J`" (Violeta never reads its SS_TYPE
//!     byte here, consistent with `words.rs`'s finding that nothing writes
//!     it).

use super::syllab::syllabify;
use super::words::{war2, wr2};
use super::{wr, SsSpa, WaSpa, SYL_BASE, SYL_SIZE};
use crate::ling::{ptr_ss, Tib};
use crate::tree3::Tree;

/// Word-record field not yet in `ling_spa::wr` (lead owns that module):
/// one byte before `wr::BRK`, same relative position as English's
/// `MINOR`. Confirmed from `fe_AssignPhraseBreaks`'s decompile
/// (`psVar2+0xb87` = `WORD_BASE+0x53b`, one short-unit before the BRK
/// writes at `+0xb88`).
pub const MINOR: usize = 0x53b;

/// `DAT_10067744`: break byte by `wr2::TOKEN_CLASS`, out-of-range -> 0 (same
/// `.get().unwrap_or(0)` convention as English's `BREAK_OF_CLASS`). Only 8
/// classes are populated; what follows in memory is unrelated data (a `500`
/// and three `1000` constants), not more table.
pub static BREAK_OF_CLASS: &[u8] = b"IGFJGGGI\x00";

/// `fe_AssignPhraseBreaks`'s class -> break-byte step and the final-word
/// default both read `wr2::TOKEN_CLASS` (word+0x536, the byte 6 short-units
/// before BRK in the decompile: `psVar5[-3]`).
fn class_break(ss: &SsSpa, w: usize) -> u8 {
    let cls = ss.i16(w + wr2::TOKEN_CLASS);
    BREAK_OF_CLASS.get(cls as usize).copied().unwrap_or(0)
}

/// `FUN_1001e4d0`: rebuild the phrase table from the word break codes.
/// Byte-for-byte the same algorithm as English's `breaks::build_phrase_table`
/// (`Ss::phrase`/`Ss::word` are unchanged formulas for Violeta too -- only
/// the break-byte constants differ: `H`/`I` here where English has `\`/`]`).
pub fn build_phrase_table(ss: &mut SsSpa) {
    let nw = ss.n_words();
    if nw == 0 {
        ss.set_i16(0, 0);
        return;
    }
    ss.set_u32(SsSpa::phrase(0) + 8, ptr_ss(SsSpa::word(0)));
    ss.set_i16(SsSpa::phrase(0), 0);
    ss.set_i16(SsSpa::phrase(0) + 2, 0);
    let mut g = 0usize;
    for i in 0..nw - 1 {
        let e = SsSpa::phrase(g);
        ss.set_i16(e + 2, ss.i16(e + 2).wrapping_add(1));
        let b = ss.u8(SsSpa::word(i) + wr::BRK);
        if b != b'H' && b != b'I' {
            let next = SsSpa::phrase(g + 1);
            ss.set_u32(next + 8, ptr_ss(SsSpa::word(i + 1)));
            ss.set_i16(next, (i + 1) as i16);
            ss.set_i16(next + 2, 0);
            g += 1;
        }
    }
    let e = SsSpa::phrase(g);
    ss.set_i16(e + 2, ss.i16(e + 2).wrapping_add(1));
    ss.set_i16(0, (g + 1) as i16);
    let mut syls = 0usize;
    for g in 0..=g {
        let e = SsSpa::phrase(g);
        ss.set_u32(e + 0xc, ptr_ss(SYL_BASE + syls * SYL_SIZE));
        ss.set_i16(e + 4, 0);
        let first = ss.i16(e) as usize;
        let n = ss.i16(e + 2).max(0) as usize;
        for k in 0..n {
            let v = ss.i16(e + 4).wrapping_add(ss.u8(SsSpa::word(first + k) + wr::N_SYL) as i16);
            ss.set_i16(e + 4, v);
        }
        syls += ss.i16(e + 4) as usize;
    }
}

/// `FUN_1001e5c0`: within each phrase, walk the words from the end and
/// insert a minor break `H` after the earlier word when the CART scores
/// > 500. No "real gap" guard (unlike English) -- every adjacent pair is
/// scored.
pub fn predict_break_index(ss: &mut SsSpa, spabi: &Tree) {
    let np = ss.n_phrases();
    for g in 0..np {
        let e = SsSpa::phrase(g);
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
            let w = SsSpa::word(wi);
            let p = SsSpa::word(wi - 1);
            words_from_end += 1;
            words_since += 1;
            let ns = ss.u8(w + wr::N_SYL) as i32;
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
                ss.u8(p + wr2::CLASSCH) as i8 as i16,
                ss.u8(w + wr2::CLASSCH) as i8 as i16,
                ss.u8(w + wr2::UNK53A) as i16,
                ss.u8(p + wr2::SRC) as i16,
                ss.u8(w + wr2::SRC) as i16,
                ss.i16(p + wr2::X3AE),
            ];
            if spabi.predict_scalar(&fv) > 500 {
                ss.set_u8(p + wr::BRK, b'H');
                syl_since = 0;
                words_since = 0;
            }
            k -= 1;
        }
    }
}

/// `fe_AssignPhraseBreaks`. `wa` supplies the per-word pause overrides
/// (record `war::PAUSE`, same offset as English). Returns false when
/// forced breaking cannot fit the sentence under the 1000-half-phone limit.
pub fn assign_phrase_breaks(ss: &mut SsSpa, tib: &mut Tib, wa: &WaSpa, spabi: &Tree) -> bool {
    syllabify(ss, wa);
    let nw = ss.n_words();
    for i in 0..nw {
        let w = SsSpa::word(i);
        let b = class_break(ss, w);
        ss.set_u8(w + wr::BRK, b);
    }
    if nw > 0 {
        let last = SsSpa::word(nw - 1) + wr::BRK;
        if ss.u8(last) != b'J' {
            ss.set_u8(last, b'F');
        }
    }
    build_phrase_table(ss);
    predict_break_index(ss, spabi);
    for i in 0..nw {
        ss.set_u8(SsSpa::word(i) + MINOR, 0);
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
            let w = SsSpa::word(i);
            match tib.brk_level[i] {
                0 => {
                    ss.set_u8(w + wr::BRK, b'I');
                    ss.set_u8(w + MINOR, 1);
                }
                1 => {
                    ss.set_u8(w + wr::BRK, b'H');
                    ss.set_u8(w + MINOR, 1);
                }
                2 => ss.set_u8(w + wr::BRK, b'G'),
                3 => ss.set_u8(w + wr::BRK, b'F'),
                _ => {}
            }
            i += 1;
        }
    }
    let v = tib.brk_level[i];
    if (0..3).contains(&v) {
        ss.set_u8(SsSpa::word(i) + wr::BRK, b'G');
    }
    let mut u: i32 = 0;
    let mut acc: i32 = 0;
    let mut last_break: i32 = 0;
    while u < nw as i32 {
        acc += ss.u8(SsSpa::word(u as usize) + wr::N_PHONES) as i32 * 2;
        if acc > 1000 {
            if u == last_break {
                return false;
            }
            u -= 1;
            ss.set_u8(SsSpa::word(u as usize) + wr::BRK, b'G');
        }
        let b = ss.u8(SsSpa::word(u as usize) + wr::BRK);
        if b == b'G' || b == b'F' || b == b'J' {
            last_break = u + 1;
            acc = 0;
        }
        u += 1;
    }
    build_phrase_table(ss);
    true
}
