//! `FUN_10027e60` (chain slot 13, last before the plain-word handler):
//! abbreviation lookups plus plain numbers. Only the "word glued to a
//! number" case (`FUN_10028440`) is ported; see the doc comments on
//! [`word_glued_number`] and below for what is declined and why.

use super::abbrev;
use super::emit::emit_split;
use super::numwords::{cardinal_to_words, emit_phrase, lookahead_gender};
use super::tokenize::{tokenize, Tok};
use super::{emit, FeSpa};
use crate::ling_spa::WaSpa;

/// `FUN_1004b5c0`: every character of `w` is a letter.
fn all_alpha(w: &[u8]) -> bool {
    use super::tables::{CLS_S, HI};
    !w.is_empty() && w.iter().all(|&c| CLS_S[c as usize] & 0xc0 != 0 || (c > 0x7f && HI[c as usize] & 0xc0 != 0))
}

/// `FUN_10025a60`: every character of `w` is uppercase (empty is false).
fn all_upper(w: &[u8]) -> bool {
    use super::tables::CLS_S;
    !w.is_empty() && w.iter().all(|&c| CLS_S[c as usize] & 0x80 != 0)
}

/// `FUN_10028440`: a plain alpha word immediately (no space) followed by a
/// number, e.g. "canal5". The word is emitted as-is (case 'S' if it's all
/// uppercase, else 'D'); the number is read as a cardinal via
/// [`cardinal_to_words`] using [`lookahead_gender`] on whatever follows the
/// number.
fn word_glued_number(fe: &FeSpa, wa: &mut WaSpa, t: &[u8], p: usize, off: i32) -> i32 {
    let (tok0, n0) = tokenize(t, p, off, 0);
    if tok0.class != 1 || !all_alpha(tok0.text()) {
        return -1;
    }
    let (tok1, n1) = tokenize(t, p + n0 as usize, off + n0, 0);
    if n1 == 0 || tok1.class != 2 || tok1.ws != 0 {
        return -1;
    }
    let (tok2, n2) = tokenize(t, p + (n0 + n1) as usize, off + n0 + n1, 0);
    if n2 != 0 && (tok2.len <= 0 || (tok2.class == 1 && tok2.ws <= 0)) {
        return -1;
    }
    let gender = lookahead_gender(fe, t, p + (n0 + n1) as usize, off + n0 + n1, false);
    let mut words = Vec::new();
    cardinal_to_words(&mut words, tok1.text(), gender);
    let case = if all_upper(tok0.text()) { b'S' } else { b'D' };
    if !emit(wa, tok0.text(), tok0.start, tok0.end, b'A', case, 0xc, None) {
        return 0;
    }
    if !emit_phrase(wa, &words, tok1.start, tok1.end, b'D', b'D', 0xc) {
        return 0;
    }
    n0 + n1
}

/// `FUN_10027e60(wa, tok(unused), t + p, off)`.
///
/// The abbreviation cascade (`FUN_100437a0` against the general-abbreviation
/// / "antes de Cristo" / "hermanos" tables) is ported via
/// [`abbrev::abbrev_expand`] for the single-word case. Declined (documented,
/// not a guess -- see `abbrev.rs` and this agent's report):
/// - `FUN_100440b0`'s "Capital Federal" multi-word match (its one table
///   entry, "Cap. Fed.", spans two tokens; `abbrev_expand` only tries a
///   single already-tokenized word, so this specific entry never matches
///   here even though the table data itself was recovered).
/// - `FUN_10028200` (a single letter immediately followed by "/" or "T."
///   plus a short number, e.g. address-style "C/25"): not exercised by the
///   corpus, still unread.
pub fn norm_number(fe: &FeSpa, wa: &mut WaSpa, _tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let (tok0, n0) = tokenize(t, p, off, 0xc);
    if tok0.len == 0 {
        return -1;
    }
    let (tok1, _n1) = tokenize(t, p + n0 as usize, off + n0, 0);
    let skip_cascade = tok1.len != 0 && tok1.class == 1 && tok1.ws < 1;
    if !skip_cascade {
        if let (Some(expansion), _gender) = abbrev::abbrev_expand(tok0.text()) {
            return if emit_split(wa, expansion, tok0.start, off + n0, b'A', b'D', 0xb) {
                n0
            } else {
                0
            };
        }
    }
    word_glued_number(fe, wa, t, p, off)
}
