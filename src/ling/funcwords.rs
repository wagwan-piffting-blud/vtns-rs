//! Function-word marking (`FUN_1000dfc0`) and the wh-question flag
//! (`FUN_1000e990`). Function words run over the WA records; the wh-flag
//! scan runs over the morph array (`Ws`), reads only `mr::TOKEN`/`mr::CAT`/
//! `mr::SPELLING`, and writes the single flag at `WH_FLAG` (`ws+0x429a4`).
//! Both run inside the real `fe_TagAndPronounce`, after morphology.
//!
//! `mark_function_words` and its dash-join helper `join_dash_tokens`
//! (`FUN_1000e160`) are ported from the raw disassembly (pulled via Ghidra
//! MCP; the decompile has the classic VC6 stack-slot-reuse problem -- the
//! 4th parameter's stack slot is read as an int index, then reused to hold
//! first a text pointer and then a WA-record pointer, which is what made the
//! decompile's `iVar6` look uninitialized). `FUN_1000e0c0` is fully ported
//! as `apply_class`.
//!
//! The wh-flag scan (`wh_flag` = `FUN_1000e990`, `wh_scan` = `FUN_1000e570`,
//! plus 6 one-line list/literal-lookup leaves) is ported from the decompile
//! (the pointer-vs-index stack-slot reuse -- e.g. `FUN_1000e570` reuses
//! `pbVar6` as an index in the first half and `pbVar5`/`param_2` as pointers
//! that track the same position in the second -- was resolved by hand
//! against the asm's own `+0x70`-stride arithmetic, not trusted from the
//! decompile's variable names). Every return path in `wh_scan` once the
//! current position is confirmed a wh-word (`L_WHWORD`) turns out to end in
//! `P` unless a recursive sub-call vetoes with `N`, which collapses what
//! looks in the decompile like a dozen distinct exits into one shape; see
//! `wh_scan`'s doc comment. Proven against every `wh_out` capture in the
//! parity corpus (`out_feling`, exercised by `wh_stage` in `tests\feling_parity.rs`).

use super::tables::{L_MODAL, L_NEG, L_WHBE, L_WHCONJ, L_WHPREP, L_WHWORD};
use super::tagger::in_list;
use super::{mr, war, Wa, Ws, WH_FLAG};
use crate::lexicon::{encode_key, stricmp, Lexicon};

/// `dict_GetTppField` (0x1003a7b0) with case mode 'I' (selectors 'G' from
/// `FUN_1000dfc0`, 'F' from `FUN_1000e160`): the TPP payload's field starting
/// with `letter` (either the payload's own first char, or after a " <letter>"
/// marker), cut at the next space.
fn get_tpp_field(lex: &Lexicon, word: &[u8], letter: u8) -> Option<Vec<u8>> {
    if word.is_empty() {
        return None;
    }
    let payload = lex.tpp.get(&encode_key(word))?;
    tpp_field_in_payload(payload, letter).map(|f| f.to_vec())
}

/// The payload-parsing half of `dict_GetTppField`, split out so it can be
/// tested directly against every stored TPP payload (see
/// `tests\feling_parity.rs::tpp_field_equivalence`) without going through a
/// word lookup.
pub fn tpp_field_in_payload(payload: &[u8], letter: u8) -> Option<&[u8]> {
    let field = if payload.first() == Some(&letter) {
        &payload[1..]
    } else {
        let mut at = None;
        for i in 0..payload.len().saturating_sub(1) {
            if payload[i] == b' ' && payload[i + 1] == letter {
                at = Some(i + 2);
                break;
            }
        }
        &payload[at?..]
    };
    let end = field.iter().position(|&c| c == b' ').unwrap_or(field.len());
    Some(&field[..end])
}

/// `FUN_10064645` / `FUN_100645ba`: the TPP field's leading decimal digits
/// as a byte (the G-class id, or the digit part of an F/A/B/C/D/E field).
fn parse_tpp_num(s: &[u8]) -> u8 {
    let mut n: u32 = 0;
    for &c in s {
        if !c.is_ascii_digit() {
            break;
        }
        n = n.wrapping_mul(10).wrapping_add((c - b'0') as u32);
    }
    n as u8
}

/// `FUN_1000e0c0`: apply a G-class byte (and the wab.txt2 index) to word
/// `i`, and to every word in `[i, end)` when `end` is a real span (`end==0`
/// from the single-word caller degenerates to just word `i`).
fn apply_class(wa: &mut Wa, lex: &Lexicon, class: u8, i: usize, end: usize) {
    if end > 0 && i < end {
        for k in i..end {
            let r = Wa::rec(k);
            wa.set_i16(r + 0x20, 1);
            wa.set_u8(r + war::TAG_A, class);
            let text = wa.cstr(r + war::TEXT).to_vec();
            if let idx @ 0.. = lex.wab.find(&text) {
                wa.set_u8(r + war::TAG_B, (idx + 1) as u8);
            }
        }
    }
    let last = if end > 0 { end } else { i };
    let r = Wa::rec(last);
    wa.set_u8(r + war::TAG_A, class);
    let text = wa.cstr(r + war::TEXT).to_vec();
    if let idx @ 0.. = lex.wab.find(&text) {
        wa.set_u8(r + war::TAG_B, (idx + 1) as u8);
    }
}

/// `FUN_1000e160`: try to join word `i` with up to 5 following class-`'A'`
/// words (skipping literal `"dash"` placeholder tokens -- class-`'S'` words
/// with zero-width span, adjacent to the previous word -- that the text
/// front end inserts for some hyphens, without counting them against the
/// 5-word cap or the running length) and look the growing hyphen-joined
/// string up with TPP field `'F'`. Returns `(field_value, last_joined_index)`
/// for the *longest* successful join (a later, longer join overwrites an
/// earlier, shorter one, same "longest match wins" pattern as the
/// exceptdict matcher in `morph.rs`).
fn join_dash_tokens(wa: &Wa, lex: &Lexicon, i: usize, n: usize) -> Option<(Vec<u8>, usize)> {
    let mut acc_len = wa.i32(Wa::rec(i) + war::LEN);
    let mut joined = wa.cstr(Wa::rec(i) + war::TEXT).to_vec();
    let mut count = 1i32;
    let mut best: Option<(Vec<u8>, usize)> = None;
    let mut k = i + 1;
    while k < n && count < 6 {
        let rk = Wa::rec(k);
        acc_len += wa.i32(rk + war::LEN) + 1;
        if acc_len >= 0x1e {
            break;
        }
        let u = wa.u8(rk + war::U_FLAG);
        let start = wa.i32(rk + war::START);
        let prev_end = wa.i32(Wa::rec(k - 1) + war::END);
        let adjacent = start - prev_end == 1;
        if u == b'S' && adjacent {
            let text = wa.cstr(rk + war::TEXT).to_vec();
            if stricmp(&text, b"dash") == 0 {
                k += 1;
                continue;
            }
        }
        if u != b'A' || start - prev_end >= 2 {
            break;
        }
        joined.push(b'-');
        joined.extend_from_slice(&wa.cstr(rk + war::TEXT));
        if let Some(f) = get_tpp_field(lex, &joined, b'F') {
            best = Some((f, k));
        }
        count += 1;
        k += 1;
    }
    best
}

/// `FUN_1000dfc0`.
pub fn mark_function_words(wa: &mut Wa, n: usize, lex: &Lexicon) {
    let mut i = 0usize;
    while i < n {
        if let Some((f, span_end)) = join_dash_tokens(wa, lex, i, n) {
            apply_class(wa, lex, parse_tpp_num(&f), i, span_end);
            i = span_end;
        } else {
            let text = wa.cstr(Wa::rec(i) + war::TEXT).to_vec();
            match get_tpp_field(lex, &text, b'G') {
                Some(g) => apply_class(wa, lex, parse_tpp_num(&g), i, 0),
                None => {
                    let idx = lex.wab.find(&text);
                    if idx >= 0 {
                        wa.set_u8(Wa::rec(i) + war::TAG_B, (idx + 1) as u8);
                    }
                }
            }
        }
        i += 1;
    }
}

// --- wh-question flag (`FUN_1000e990` / `FUN_1000e570`) ------------------

/// `wh_scan`'s tri-state return: found (set the flag and stop), not-found
/// (stop scanning without setting it) or the default "keep scanning".
const WH_P: u8 = 0x50;
const WH_N: u8 = 0x4e;
const WH_D: u8 = 0x44;

fn spelling(ws: &Ws, i: usize) -> Vec<u8> {
    ws.cstr(Ws::morph(i) + mr::SPELLING).to_vec()
}

fn cat(ws: &Ws, i: usize) -> i16 {
    ws.i16(Ws::morph(i) + mr::CAT)
}

fn ci(a: &[u8], b: &[u8]) -> bool {
    stricmp(a, b) == 0
}

/// `FUN_1000e3a0`: is `a` a modal/`"do"` (`L_MODAL`), or one of the
/// `"ca"`/`"sha"`/`"wo"` contraction stems with `b` == `"n't"`?
fn e3a0(a: &[u8], b: &[u8]) -> bool {
    in_list(&L_MODAL, a) || ((ci(a, b"ca") || ci(a, b"sha") || ci(a, b"wo")) && ci(b, b"n't"))
}

/// `FUN_1000e430`: is `a` (the morph at index `idx`) a finite/contracted
/// form of "be" (`L_WHBE`)? -- except when `idx==0` and `a` is exactly
/// `"'s"` (a leading possessive, not an "is" contraction), which is
/// excluded even though `"'s"` is itself in `L_WHBE`.
fn e430(a: &[u8], idx: usize) -> bool {
    if idx > 0 {
        return in_list(&L_WHBE, a);
    }
    !ci(a, b"'s") && in_list(&L_WHBE, a)
}

/// `FUN_1000e570`: recursive wh-clause scan starting at morph `idx`.
///
/// The first half either finds none of `L_WHPREP`/`L_NEG`/`L_WHCONJ` at
/// `idx`, runs out of room, or recurses to `idx+1` and gets back the
/// "keep scanning" sentinel `WH_D` -- any of which falls into the second
/// half; otherwise (the word *is* one of those three lists and the
/// recursion already resolved to `WH_P`/`WH_N`) that resolved value is
/// returned directly, unexamined.
///
/// The second half checks a handful of literal words ("why"/"how ..."/
/// "what ...") that immediately resolve to `WH_P`, then `L_WHWORD`
/// (the actual wh-words): once the current word is confirmed to be one,
/// *every* path through the rest of the function returns `WH_P` -- the
/// remaining logic (the two neighbour-word gates, then a scan for a comma
/// that triggers either a skip-to-next-comma for `"... if ..."` or a
/// recursive sub-scan that can veto with `WH_N`) only ever *shortens* the
/// scan or vetoes; it never itself asserts "not a question". Falling off
/// the end returns `WH_D` (not a wh-word here at all; the caller in
/// `wh_flag` tries the next candidate start).
fn wh_scan(ws: &Ws, idx: usize) -> u8 {
    let n = ws.n_morphs();
    let sp = spelling(ws, idx);

    let none_of_3 = !(in_list(&L_WHPREP, &sp) || in_list(&L_NEG, &sp) || in_list(&L_WHCONJ, &sp));
    let near_end = n <= idx + 2;
    if !none_of_3 && !near_end {
        let r = wh_scan(ws, idx + 1);
        if r == WH_P || r == WH_N {
            return r;
        }
    }

    if ci(&sp, b"why") {
        return WH_P;
    }
    if ci(&sp, b"how") && idx + 2 < n {
        let s1 = spelling(ws, idx + 1);
        if ci(&s1, b"about") || ci(&s1, b"come") || ci(&s1, b"much") || ci(&s1, b"say") {
            return WH_P;
        }
    }
    if ci(&sp, b"what") {
        if idx + 2 < n {
            let s1 = spelling(ws, idx + 1);
            if ci(&s1, b"about")
                || ci(&s1, b"if")
                || ci(&s1, b"in")
                || ci(&s1, b"thouth") // sic -- the engine's own literal, a typo of "though"
                || ci(&s1, b"then")
                || ci(&s1, b"time")
                || ci(&s1, b"else")
            {
                return WH_P;
            }
        }
        if idx + 3 < n {
            let s1 = spelling(ws, idx + 1);
            let s2 = spelling(ws, idx + 2);
            if ci(&s1, b"kind") && ci(&s2, b"of") {
                return WH_P;
            }
        }
    }

    if in_list(&L_WHWORD, &sp) {
        let j1 = idx + 1;
        if j1 + 1 < n {
            let s1 = spelling(ws, j1);
            if e430(&s1, j1) || e3a0(&s1, b"") || ci(&s1, b",") {
                return WH_P;
            }
        }
        if idx + 3 < n {
            let s1 = spelling(ws, j1);
            let s2 = spelling(ws, idx + 2);
            if e3a0(&s1, &s2) {
                return WH_P;
            }
        }
        if n <= j1 + 1 {
            return WH_P;
        }
        if (j1..n - 1).any(|p| ci(&spelling(ws, p), b",")) {
            let mut j2 = idx + 2;
            let mut p = j1;
            while p < n - 1 {
                if ci(&spelling(ws, p), b",") && j2 + 1 < n {
                    if ci(&spelling(ws, p + 1), b"if") {
                        if n <= j2 + 1 {
                            return WH_P;
                        }
                        let mut j3 = j2;
                        let mut q = p + 1;
                        while !ci(&spelling(ws, q), b",") {
                            j3 += 1;
                            q += 1;
                            if n <= j3 + 1 {
                                return WH_P;
                            }
                        }
                    } else if wh_scan(ws, j2) == WH_N {
                        return WH_N;
                    }
                }
                p += 1;
                j2 += 1;
            }
        }
        return WH_P;
    }

    WH_D
}

/// `FUN_1000e990`: sets `WH_FLAG` (`ws+0x429a4`) when the morph array ends
/// in a `'?'` morph (`mr::CAT==3`) and `wh_scan` finds a wh-word/clause
/// starting at 0 or right after any morph with a nonzero `mr::CAT`.
pub fn wh_flag(ws: &mut Ws) {
    let n = ws.n_morphs();
    ws.set_i16(WH_FLAG, 0);
    if n == 0 || n == 1 || cat(ws, n - 1) != 3 {
        return;
    }
    let mut i = 0usize;
    loop {
        let r = if i == 0 {
            Some(wh_scan(ws, 0))
        } else if cat(ws, i) != 0 {
            Some(wh_scan(ws, i + 1))
        } else {
            None
        };
        if let Some(r) = r {
            if r == WH_P {
                ws.set_i16(WH_FLAG, 1);
                return;
            }
            if r == WH_N {
                return;
            }
        }
        i += 1;
        if i >= n - 1 {
            return;
        }
    }
}
