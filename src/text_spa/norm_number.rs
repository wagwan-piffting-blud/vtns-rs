//! `FUN_10027e60` (chain slot 13, last before the plain-word handler): the
//! abbreviation tables, then `FUN_10028200` ("C/", "P/", "T. N") and
//! `FUN_10028440` (a word glued to a number, "FLC011").

use super::abbrev::{multiword_lookup, simple_lookup};
use super::abbrev_tables::{ANTES_DE_CRISTO, ANTIGUO_TESTAMENTO, CAPITAL_FEDERAL, GROUP_0X10076AA8, HERMANOS};
use super::emit::emit_split;
use super::norm_math::numeric;
use super::numwords::{cardinal_to_words, digit_by_digit, lookahead_gender, ordinal_suffix_classify, ordinal_suffix_render};
use super::tokenize::{tokenize, Tok};
use super::{emit, FeSpa};
use crate::ling_spa::WaSpa;

type Table = [(&'static [u8], &'static [u8], u8)];

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

/// `FUN_10028200`: a lone "C"/"P" before "/" ("con"/"para"), or "T." before
/// a number ("tomo").
fn letter_abbrev(wa: &mut WaSpa, t: &[u8], p: usize, off: i32) -> i32 {
    let (tok0, n0) = tokenize(t, p, off, 0);
    if tok0.len != 1 {
        return -1;
    }
    let u = tok0.text[0].to_ascii_uppercase();
    let p1 = p + n0 as usize;
    if u == b'C' || u == b'P' {
        let (tok1, n1) = tokenize(t, p1, off + n0, 0);
        if n1 == 0 || tok1.len != 1 || tok1.text[0] != b'/' {
            return -1;
        }
        let (tok2, n2) = tokenize(t, p1 + n1 as usize, off + n0 + n1, 0);
        if n2 != 0 && tok2.ws < 1 && (tok2.ws != 0 || tok2.class == 1) {
            return -1;
        }
        let word: &[u8] = if u == b'C' { b"con" } else { b"para" };
        return if emit(wa, word, tok0.start, tok1.end, b'A', b'D', 0xd, None) { n0 + n1 } else { 0 };
    }
    if u == b'T' {
        let (tok1, n1) = tokenize(t, p1, off + n0, 0);
        if n1 != 0 && tok1.len == 1 && tok1.text[0] == b'.' && tok1.ws == 0 {
            let (tok2, n2) = tokenize(t, p1 + n1 as usize, off + n0 + n1, 0);
            if n2 != 0 && tok2.class == 2 && tok2.ws < 2 && numeric(tok2.text()) {
                return if emit(wa, b"tomo", tok0.start, tok1.end, b'A', b'D', 0xd, None) { n0 + n1 } else { 0 };
            }
        }
    }
    -1
}

/// `FUN_10028440`: a word immediately followed by a number. A number with a
/// leading zero (no ',' or '.') is read digit by digit, an ordinal suffix
/// ("1ro") as an ordinal, anything else as a cardinal agreeing with the next
/// word.
fn word_glued_number(fe: &FeSpa, wa: &mut WaSpa, t: &[u8], p: usize, off: i32) -> i32 {
    let (tok0, n0) = tokenize(t, p, off, 0);
    if tok0.class != 1 || !all_alpha(tok0.text()) {
        return -1;
    }
    let (tok1, n1) = tokenize(t, p + n0 as usize, off + n0, 0);
    if n1 == 0 || tok1.class != 2 || tok1.ws != 0 {
        return -1;
    }
    let p2 = p + (n0 + n1) as usize;
    let off2 = off + n0 + n1;
    let (tok2, n2) = tokenize(t, p2, off2, 0);
    if n2 != 0 && !(tok2.len > 0 && (tok2.class != 1 || tok2.ws > 0)) {
        return -1;
    }
    let num = tok1.text();
    let mut words = Vec::new();
    match ordinal_suffix_classify(num) {
        None => {
            if (num[0] as i8) < b'1' as i8 && !num.contains(&b',') && !num.contains(&b'.') {
                digit_by_digit(&mut words, num);
            } else {
                let g = lookahead_gender(fe, t, p2, off2, false);
                cardinal_to_words(&mut words, num, g);
            }
        }
        Some((code, _)) => {
            let g = lookahead_gender(fe, t, p2, off2, true);
            ordinal_suffix_render(&mut words, num, code, g);
        }
    }
    let case = if all_upper(tok0.text()) { b'S' } else { b'D' };
    if !emit(wa, tok0.text(), tok0.start, tok0.end, b'A', case, 0xc, None) {
        return 0;
    }
    if !emit_split(wa, &words, tok1.start, tok1.end, b'D', b'D', 0xc) {
        return 0;
    }
    n0 + n1
}

fn find(table: &Table, word: &[u8], insensitive: bool) -> Option<&'static [u8]> {
    simple_lookup(table, word, insensitive).map(|i| table[i].1)
}

/// `FUN_10027e60(wa, tok(unused), t + p, off)`.
pub fn norm_number(fe: &FeSpa, wa: &mut WaSpa, _tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let (tok0, n0) = tokenize(t, p, off, 0xc);
    if tok0.len == 0 {
        return -1;
    }
    let (tok1, _) = tokenize(t, p + n0 as usize, off + n0, 0);
    let fallback = |fe: &FeSpa, wa: &mut WaSpa| -> i32 {
        if tok1.len > 0 {
            let r = letter_abbrev(wa, t, p, off);
            if r >= 0 {
                return r;
            }
            let r = word_glued_number(fe, wa, t, p, off);
            if r >= 0 {
                return r;
            }
        }
        -1
    };
    if tok1.len != 0 {
        if tok1.len < 1 {
            return -1;
        }
        if tok1.class == 1 && tok1.ws < 1 {
            return fallback(fe, wa);
        }
    }
    let word = tok0.text().to_vec();
    let mut end = tok0.end;
    let mut consumed = n0;
    let mut hit: Option<&[u8]> = None;
    if tok1.len > 0 && tok1.class == 1 && tok1.ws > 0 {
        let mut cursor = n0;
        if let Some(i) = multiword_lookup(&CAPITAL_FEDERAL, &word, true, t, p, off, &mut cursor) {
            hit = Some(CAPITAL_FEDERAL[i].1);
            consumed = cursor;
            end = off + cursor;
        }
    }
    let dot = word.last() == Some(&b'.');
    if hit.is_none() {
        hit = find(&ANTIGUO_TESTAMENTO, &word, false).or_else(|| find(&GROUP_0X10076AA8, &word, true));
        if hit.is_none() && !dot {
            hit = find(&ANTES_DE_CRISTO, &word, false).or_else(|| find(&HERMANOS, &word, true));
        }
    }
    if hit.is_none() && dot {
        let w = &word[..word.len() - 1];
        hit = find(&ANTES_DE_CRISTO, w, false)
            .or_else(|| find(&HERMANOS, w, true))
            .or_else(|| find(&ANTIGUO_TESTAMENTO, w, false))
            .or_else(|| find(&GROUP_0X10076AA8, w, true));
    }
    match hit {
        Some(ex) if !ex.is_empty() => {
            if emit_split(wa, ex, tok0.start, end, b'A', b'D', 0xb) {
                consumed
            } else {
                0
            }
        }
        _ => fallback(fe, wa),
    }
}
