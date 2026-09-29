//! `FUN_10033db0` (chain slot 11): despite its "roman numerals" billing this
//! is also the engine's general context-aware cardinal-number reader for a
//! bare `class == 2` token -- a plain "60" reaches the spoken-word stage
//! here, not in `norm_number.rs`. For a `class == 1` token it additionally
//! recognizes the 20 numeral words "I".."XX" (`FUN_10034190`, case-sensitive)
//! and reads their value.
//!
//! Both branches consult the *previous* word: if it is "la"/"las"
//! (case-insensitive), the number defaults to feminine gender ('F') when
//! nothing else (the lookahead on the following word, `FUN_1004c6f0`)
//! supplies one. For the numeral-word branch, a previous word that is not a
//! capitalized alpha word makes bare "I"/"V"/"X" decline (too ambiguous
//! against real letters); other previous words there take an ordinal
//! reading via `FUN_1004e070` (`param_3 == 0`, the plain-value path) ->
//! `FUN_1004e400` (ported as `numwords::ordinal_word_1_20`, covering the
//! full 1-20 range this caller ever needs -- see numwords.rs's module doc),
//! so "Enrique VIII" now reads "... octavo" as the engine does.
//! `FUN_1004dd60`'s ordinal-suffix branch for the `class == 2` token ("1ro",
//! "8vo", "100\xba") is now ported too, via `numwords::ordinal_suffix_classify`
//! / `numwords::ordinal_suffix_render` -- see their own doc comments for
//! exactly what's covered. When the classifier declines (`None`, e.g. a
//! plain "60" or a digit/suffix mismatch like "2ro"), this falls back to the
//! original digit-by-digit/cardinal reading below, matching
//! `FUN_10033db0`'s own `(short)uVar2==0` fallback branch. After a cardinal
//! (the fallback path only -- the ordinal-suffix success path does NOT set
//! this, matching `FUN_10033db0`'s own control flow), `FUN_1004c100`
//! (`plural`) sets the last record's `+0x12` short.
//!
//! Verified against a fresh capture (`probe/out/units.jsonl`): "1er"/"3er"/
//! "21er" correctly decline (no "er" suffix exists in `FUN_1004dd60`'s own
//! tables -- only "ro"/"ero" do, for digit 3) and fall back to the same
//! digit-by-digit reading as the engine; "8vo"/"9no"/"100\xba"/"105to" all
//! render bit-exact via the new success path. One corpus line, "Terminó en
//! 2do lugar...", still mismatches -- but not on "2do" itself: the real
//! engine reads *both* "en" and "2do" (as bare "dos", not "segundo") under a
//! single unfamiliar `src=3`, meaning some other, still-declined normalizer
//! earlier in `normalize::CHAIN` (currency is the leading suspect --
//! `norm_units.rs`'s own doc notes an analogous "commit a bare number with
//! no unit matched" fallback) intercepts "en" + a following number before
//! `norm_roman.rs` ever sees it; every other ordinal-suffix line in the
//! probe corpus, none preceded by a bare "en" immediately before the digit,
//! reaches this function normally and matches. Not this function's bug --
//! left as a documented gap for whoever ports that earlier normalizer.

use super::numwords::{
    cardinal_to_words, digit_by_digit, emit_phrase, is_comma_grouped, is_period_grouped, lookahead_gender, ordinal_gender_from_lookahead,
    ordinal_suffix_classify, ordinal_suffix_render, ordinal_word_1_20, plural, G_DEFAULT, G_F,
};
use super::tokenize::{tokenize, Tok};
use super::FeSpa;
use crate::ling_spa::lexicon::stricmp_spa;
use crate::ling_spa::WaSpa;

fn ws(c: u8) -> bool {
    matches!(c, b' ' | 9 | 10 | 13)
}

/// `FUN_1004bd70(.., 1)` + `FUN_1004bf70(.., 0)`, simplified to what this
/// caller needs: the word immediately before `t[cur..]` (sentence start at
/// `start`), skipping whitespace, tokenized on its own (mode 1).
fn prev_word_tok(t: &[u8], start: usize, cur: usize) -> Option<Tok> {
    if cur <= start {
        return None;
    }
    let mut p = cur;
    loop {
        p -= 1;
        if !ws(t[p]) {
            break;
        }
        if p <= start {
            return None;
        }
    }
    let lim = 29usize;
    let mut count = 1usize;
    while p > start && count < lim {
        let c = t[p - 1];
        if ws(c) {
            break;
        }
        p -= 1;
        count += 1;
    }
    let mut buf = t[p..p + count].to_vec();
    buf.extend_from_slice(&[0u8; 8]);
    let mut tok = Tok::default();
    let mut used = 0usize;
    while used < count {
        let (tk, r) = tokenize(&buf, used, 0, 1);
        tok = tk;
        used += r.max(1) as usize;
    }
    Some(tok)
}

fn is_upper(c: u8) -> bool {
    use super::tables::CLS_S;
    CLS_S[c as usize] & 0x80 != 0
}

/// Gender override from the previous word being "la"/"las" (see the module
/// doc): `G_F` if so, else `G_DEFAULT`.
fn la_gender(t: &[u8], start: usize, cur: usize) -> i32 {
    match prev_word_tok(t, start, cur) {
        Some(tok) if tok.class == 1 && (stricmp_spa(tok.text(), b"la") == 0 || stricmp_spa(tok.text(), b"las") == 0) => G_F,
        _ => G_DEFAULT,
    }
}

/// `FUN_10034190`: value 1-20 of an exact ("I".."XX", case-sensitive) roman
/// numeral word, else -1.
fn roman_value(w: &[u8]) -> i32 {
    const TABLE: [&[u8]; 20] = [
        b"I", b"II", b"III", b"IV", b"V", b"VI", b"VII", b"VIII", b"IX", b"X", b"XI", b"XII", b"XIII", b"XIV", b"XV", b"XVI", b"XVII",
        b"XVIII", b"XIX", b"XX",
    ];
    match TABLE.iter().position(|&s| s == w) {
        Some(i) => i as i32 + 1,
        None => -1,
    }
}

/// `FUN_10033db0(wa, tok, t + p, off)`.
pub fn norm_roman(fe: &FeSpa, wa: &mut WaSpa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    if tok.len == 0 {
        return -1;
    }
    let start = p - off as usize;
    let cap_gender = la_gender(t, start, p);

    if tok.class == 2 {
        // `FUN_1004dd60` ordinal-suffix branch, `(short)uVar2 != 0` success
        // path: classify the token's own suffix and render it directly, no
        // re-tokenize.
        if let Some((code, _digits)) = ordinal_suffix_classify(tok.text()) {
            let g = lookahead_gender(fe, t, p + tok.consumed as usize, off + tok.consumed, true);
            let mut words = Vec::new();
            ordinal_suffix_render(&mut words, tok.text(), code, g);
            if !emit_phrase(wa, &words, tok.start, tok.end, b'D', b'D', 2) {
                return 0;
            }
            return tok.consumed;
        }
        // `(short)uVar2 == 0` fallback: plain digit-by-digit/cardinal reading.
        let (tok3, _) = tokenize(t, p, off, 2);
        let text = tok3.text();
        let mut words = Vec::new();
        if !is_period_grouped(text) && !is_comma_grouped(text) {
            digit_by_digit(&mut words, text);
        } else {
            let g = lookahead_gender(fe, t, p + tok3.consumed as usize, off + tok3.consumed, false);
            let gender = if g != G_DEFAULT { g } else { cap_gender };
            cardinal_to_words(&mut words, text, gender);
        }
        if !emit_phrase(wa, &words, tok3.start, tok3.end, b'D', b'D', 2) {
            return 0;
        }
        if plural(text) {
            let n = wa.i16(0) as usize;
            wa.set_i16(WaSpa::rec(n - 1) + 0x12, 1);
        }
        return tok3.consumed;
    }

    if tok.class == 1 {
        let value = roman_value(tok.text());
        if value < 0 {
            return -1;
        }
        let prev = prev_word_tok(t, start, p);
        let prev_cap_word = prev.as_ref().is_some_and(|pt| pt.class == 1 && !pt.text().is_empty() && is_upper(pt.text()[0]));
        let digits = value.to_string().into_bytes();
        let mut words = Vec::new();
        if !prev_cap_word {
            if value == 1 || value == 5 || value == 10 {
                return -1;
            }
            cardinal_to_words(&mut words, &digits, G_DEFAULT);
        } else {
            // `FUN_1004e070(out, "%d"-of-value, param_3=0, gender)`: the
            // plain-value ordinal path (`exact=true`, matching
            // `FUN_1004c6f0`'s own `param_4=1` at this call site in
            // `FUN_10033db0`).
            let g = lookahead_gender(fe, t, p + tok.consumed as usize, off + tok.consumed, true);
            ordinal_word_1_20(&mut words, value, ordinal_gender_from_lookahead(g));
        }
        if !emit_phrase(wa, &words, tok.start, tok.end, b'D', b'D', 2) {
            return 0;
        }
        return tok.consumed;
    }

    -1
}
