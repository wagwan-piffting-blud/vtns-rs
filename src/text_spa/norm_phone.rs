//! `phone` `FUN_10044910` (chain slot 5): phone numbers.
//!
//! Ported: the core capture loop (plain `tokenize(.., 6)` per piece -- unlike
//! `norm_url.rs`, phone numbers don't need a custom piece-tokenizer), the
//! digit-group ('N') / literal-separator (one of `()/-.`) classification,
//! shape matching with shrink-and-retry (`PHONE_SHAPES`, transcribed via
//! `gen_rust_tables.py` + `pefile`, never by hand), the per-group digit
//! reading (`FUN_10045c90` -> `digit_group_words`, reusing `numwords`'s
//! `cardinal_to_words`/`digit_by_digit` exactly as `norm_symbols2.rs` does),
//! the render/commit (`FUN_10044f90`: 'N' entries share one span, each
//! non-final spoken record's `rec::PUNCT = 5`), the per-group length gate
//! (`FUN_10045190`'s loop over `FUN_10045d40`/`80`/`de0`/`e40`, INCLUDING the
//! value-threading mode nuance -- `validate_n_lengths` below), the
//! "(TEL)"/"(FAX)" word prefix and the bare teléfono/tel/fax/facsímil/
//! facsímile/telefonear trigger word (`FUN_10045440`, both branches --
//! `tel_fax_prefix`), and the one-word backward "is the previous word itself
//! a number" fold (`FUN_1004bd70`+`FUN_1004bf70`, `prev_word_is_number`) that
//! gates the bare `NNNN`/`NNN` shapes (`FUN_10045190`'s tail `switch`, cases
//! 8/0x10) via `param_5`.
//!
//! Also ported: the `FUN_10045190` shape->=0x11 pre-check that gates every
//! bare/minimal shape (`N/N`, `N-N`, `NN`, `N`) on a preceding trigger
//! (`validate_extension_shape`) -- a genuine engine quirk is ported as-is
//! here: `N/N` (0x11) and a lone `N` (0x14) share the SAME `entries[1]`
//! check as `N-N`'s separator (exactly one lead-in whitespace char and
//! exactly 4 digits of text), which is never true for a `/` (len 1) or a
//! nonexistent second entry (len 0) -- so those two shapes can never
//! actually match; only `N-N` (0x12) and `NN` (0x13) are reachable. Because
//! this pre-check's gate is unconditional, `FUN_10045190`'s tail-switch
//! `case 0x11`/`0x14` bodies (their own separate `entries[0]`/`entries[2]`
//! checks) are unreachable dead code and are not separately translated.
//!
//! Declined (not attempted): a leading `+` (`FUN_10045440`'s caller-side
//! "+CC" block in `FUN_10044910` -- a bare `+` isn't in the separator set and
//! isn't a digit, so capture stops at zero entries and this correctly
//! returns -1, falling through); the trailing extension/trigger-word
//! lookahead (`FUN_10045a70`, "ext. 123" and a trailing "tel"/"fax" style
//! trigger with NO preceding "(TEL)"/"teléfono" -- this also means shapes
//! `N-N`/`NN` (0x12/0x13) and the tail-switch's `param_6`/`local_540` check
//! for `NNNN`/`NNN` (which needs `FUN_10045a70`'s own forward-token capture)
//! only ever see the *preceding*-trigger path here, never a trailing one);
//! `FUN_100457a0` (a related forward-extension helper, only reachable from
//! `FUN_10045a70`). "(956) 504-1432", "1-800-555-1212", "(TEL) 555-1212",
//! "teléfono: 555-1212" and "555-1212 (no trigger, NNNN/NNN)" are unaffected
//! by any of these -- confirmed against engine captures.

use super::numwords::{atoi, cardinal_to_words, digit_by_digit, emit_phrase};
use super::punct::{last_token, prev_word};
use super::tables::CLS_S;
use super::tokenize::{tokenize, Tok};
use super::{emit, rec, FeSpa};
use crate::ling_spa::lexicon::stricmp_spa;
use crate::ling_spa::WaSpa;

fn digit(c: u8) -> bool {
    CLS_S[c as usize] & 0x10 != 0
}

fn is_all_digit(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&c| digit(c))
}

/// `PTR_DAT_1007a61c`, all 21 entries (transcribed via `gen_rust_tables.py`):
/// indices 0-16 are the always-tried digit-group shapes; 17-20 ("N/N",
/// "N-N", "NN", "N") are the bare/minimal shapes gated on a preceding
/// trigger by `FUN_10045190`'s shape->=0x11 pre-check (`validate_extension_shape`).
const PHONE_SHAPES: [&[u8]; 21] = [
    b"N-(N)-N-N",
    b"N(N)/N-N",
    b"N(N)N-N",
    b"N-N-N-N",
    b"N.N.N.N",
    b"N(N)NN",
    b"NN-N-N",
    b"NN.N.N",
    b"NNNN",
    b"(N)/N-N",
    b"(N)N-N",
    b"(N)NN",
    b"N/N-N",
    b"N-N-N",
    b"N.N.N",
    b"NN-N",
    b"NNN",
    b"N/N",
    b"N-N",
    b"NN",
    b"N",
];

/// `PTR_DAT_1006f708`, stride-0x27 (key, value) pairs, `DAT_10079e08`
/// entries: the parenthetical/bare trigger word's canonical spoken form.
const PAREN_WORD: [(&[u8], &[u8]); 2] = [(b"FAX", b"fax"), (b"TEL", b"tel\xe9fono")];

/// `DAT_1007a338`: a direct (non-table) trigger-word match.
const TEL_LIT: &[u8] = b"tel\xe9fono";
/// `DAT_1007a36c`: a direct (non-table) trigger-word match.
const FAX_LIT: &[u8] = b"facs\xedmil";

fn paren_word_index(text: &[u8]) -> Option<usize> {
    PAREN_WORD.iter().position(|&(k, _)| stricmp_spa(k, text) == 0)
}

fn eq_ci(a: &[u8], b: &[u8]) -> bool {
    stricmp_spa(a, b) == 0
}

#[derive(Clone, Default)]
struct Entry {
    start: i32,
    pos_after: i32,
    kind: u8,
    /// Leading whitespace/skip before this piece (`Tok::ws`); only used by
    /// `validate_extension_shape`.
    ws: i32,
    text: Vec<u8>,
}

/// `FUN_10044910`'s capture loop (mode-6 `tokenize`, no leading `+`): each
/// piece is either all-digit ('N') or a lone char from `()/-.`; anything
/// else stops capture.
fn capture(t: &[u8], p: usize, off: i32) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    let (mut cur_p, mut cur_off) = (p, off);
    loop {
        let (tok, r) = tokenize(t, cur_p, cur_off, 6);
        if r == 0 || tok.class == 10 {
            break;
        }
        let text = tok.text().to_vec();
        let c0 = text.first().copied().unwrap_or(0);
        let kind = if is_all_digit(&text) {
            b'N'
        } else if tok.len == 1 && c0 != 0 && b"()/-.".contains(&c0) {
            c0
        } else {
            break;
        };
        entries.push(Entry { start: tok.start, pos_after: cur_off + r, kind, ws: tok.ws, text });
        cur_p += r as usize;
        cur_off += r;
        if entries.len() >= 9 {
            break;
        }
    }
    entries
}

/// `FUN_10045190`'s per-group length gate loop (`switch(iVar7)`, `iVar7`
/// counting down from the shape's total N-group count -- always 3 or 4 for
/// shapes 0-16 -- as each 'N' entry is hit; a leading `(` entry is simply
/// skipped, `iVar7` doesn't decrement for it, so a 3-group shape's FIRST
/// N-group is checked by `FUN_10045d80` -- not `FUN_10045d40`, which only
/// ever fires for a 4-group shape's first group). `FUN_10045d40` and
/// `FUN_10045d80`/`FUN_10045de0` in their "mode 0" branch all require the
/// same ranges as before, so this only changes behaviour for a 4-group
/// shape whose first group's value (`FUN_100505b3`, `atoi`) is exactly 1:
/// that value is threaded unchanged into BOTH the 2nd (`FUN_10045d80`) and
/// 3rd (`FUN_10045de0`) group's check, narrowing them from 1-3/3-4 digits to
/// 2-3/exactly-3.
fn validate_n_lengths(entries: &[Entry]) -> bool {
    let groups: Vec<&[u8]> = entries.iter().filter(|e| e.kind == b'N').map(|e| e.text.as_slice()).collect();
    let g = groups.len() as i32;
    let mut mode = 0i32;
    for (i, text) in groups.iter().enumerate() {
        let len = text.len();
        let ok = match g - i as i32 - 1 {
            3 => {
                if !(1..=3).contains(&len) {
                    false
                } else {
                    mode = atoi(text);
                    true
                }
            }
            2 => {
                if mode == 1 {
                    (2..=3).contains(&len)
                } else {
                    (1..=3).contains(&len)
                }
            }
            1 => {
                if mode == 1 {
                    len == 3
                } else {
                    (3..=4).contains(&len)
                }
            }
            0 => len == 4,
            _ => true,
        };
        if !ok {
            return false;
        }
    }
    true
}

/// `FUN_10045190`'s shape->=0x11 pre-check -- see the module doc comment for
/// why only 0x12 ("N-N") and 0x13 ("NN") are ever reachable.
fn validate_extension_shape(shape_idx: usize, entries: &[Entry]) -> bool {
    let e0_len = entries[0].text.len();
    let e1_ws = entries.get(1).map(|e| e.ws).unwrap_or(0);
    let e1_len = entries.get(1).map(|e| e.text.len()).unwrap_or(0);
    match shape_idx {
        0x12 => {
            let e2_len = entries.get(2).map(|e| e.text.len()).unwrap_or(0);
            (3..=4).contains(&e0_len) && e1_ws == 0 && e2_len == 4
        }
        0x11 | 0x13 | 0x14 => (3..=4).contains(&e0_len) && e1_ws == 1 && e1_len == 4,
        _ => false,
    }
}

/// `FUN_10045c90`: a digit group ending in one or two zeros after a nonzero
/// leading run (e.g. "500", "2300", "9000") reads as a cardinal number;
/// anything else reads digit by digit.
fn digit_group_words(text: &[u8]) -> Vec<u8> {
    let round = match text.len() {
        3 => text[0] > b'0' && text[0] <= b'9' && text[1] == b'0' && text[2] == b'0',
        4 => {
            text[0] > b'0'
                && text[0] <= b'9'
                && ((text[1] > b'0' && text[1] <= b'9' && text[2] == b'0' && text[3] == b'0') || (text[1] == b'0' && text[2] == b'0' && text[3] == b'0'))
        }
        _ => false,
    };
    let mut out = Vec::new();
    if round {
        cardinal_to_words(&mut out, text, 0);
    } else {
        digit_by_digit(&mut out, text);
    }
    out
}

/// A "(TEL)"/"(FAX)" or bare trigger-word match (`FUN_10045440`'s out
/// buffer, `local_500`): the span and canonical spoken word of the matched
/// word (its ORIGINAL text if matched via `TEL_LIT`/`FAX_LIT` rather than
/// `PAREN_WORD`).
struct TelFax {
    start: i32,
    end: i32,
    word: Vec<u8>,
}

/// `FUN_10045440`'s first branch: the CURRENT dispatching token is a lone
/// `(`; tokenize the next piece (mode 6) and accept it as "FAX"/"TEL" (via
/// `PAREN_WORD`, case-insensitively) or a direct `TEL_LIT`/`FAX_LIT` match,
/// then optionally consume a clean, closely-following `)`.
fn tel_fax_paren(tok: &Tok, t: &[u8], p: usize, off: i32) -> Option<(TelFax, i32)> {
    let after_paren = tok.consumed;
    let (next, r1) = tokenize(t, p + after_paren as usize, off + after_paren, 6);
    if r1 == 0 || next.class == 10 {
        return None;
    }
    let found = paren_word_index(next.text());
    let not_found_anywhere = found.is_none() && !eq_ci(next.text(), TEL_LIT) && !eq_ci(next.text(), FAX_LIT);
    if not_found_anywhere || !matches!(next.class, 1 | 3) || next.ws > 2 {
        return None;
    }
    let word = match found {
        Some(i) => PAREN_WORD[i].1.to_vec(),
        None => next.text().to_vec(),
    };
    let entry = TelFax { start: next.start, end: next.end, word };
    let after_word = after_paren + r1;
    let (closer, r2) = tokenize(t, p + after_word as usize, off + after_word, 6);
    let clean_closer = closer.text().first() == Some(&b')') && closer.len == 1 && closer.ws <= 2;
    let consumed = if clean_closer { after_word + r2 } else { after_word };
    Some((entry, consumed))
}

/// `FUN_10045440`'s second branch: the CURRENT dispatching token IS the
/// trigger word itself (bare "teléfono"/"tel"/"fax"/"facsímil"/"facsímile"/
/// "telefonear", or one whose text matches `PAREN_WORD`'s key case-
/// insensitively), immediately followed by an optional clean `)`/`:`/`.`.
fn tel_fax_bare(tok: &Tok, t: &[u8], p: usize, off: i32, wa_words: i16) -> Option<(TelFax, i32)> {
    let found = paren_word_index(tok.text());
    let not_found_anywhere = found.is_none() && !eq_ci(tok.text(), TEL_LIT) && !eq_ci(tok.text(), FAX_LIT);
    if not_found_anywhere || !matches!(tok.class, 1 | 3) {
        return None;
    }
    if wa_words != 0 && tok.ws < 1 && tok.nl < 1 {
        return None;
    }
    let word = match found {
        Some(i) => PAREN_WORD[i].1.to_vec(),
        None => tok.text().to_vec(),
    };
    let entry = TelFax { start: tok.start, end: tok.end, word };
    let after_word = tok.consumed;
    let (next, r2) = tokenize(t, p + after_word as usize, off + after_word, 6);
    let clean_closer = matches!(next.text().first(), Some(b')') | Some(b':') | Some(b'.')) && next.len == 1 && next.ws <= 2;
    if clean_closer {
        return Some((entry, after_word + r2));
    }
    if found.is_some() && next.len > 0 {
        let dot_adjacent = next.text().first() == Some(&b'.') && next.ws == 0 && next.len == 1;
        if !dot_adjacent && next.class == 1 {
            return None;
        }
        if next.text().first() == Some(&b'.') {
            if next.ws == 0 {
                return if next.len == 1 { Some((entry, after_word)) } else { None };
            }
            // ws != 0: falls through to the unadvanced success below.
        } else if next.ws == 0 {
            return None;
        }
    }
    Some((entry, after_word))
}

/// `FUN_10045440`: dispatches to `tel_fax_paren` when the current token is a
/// lone `(`, else `tel_fax_bare`.
fn tel_fax_prefix(tok: &Tok, t: &[u8], p: usize, off: i32, wa_words: i16) -> Option<(TelFax, i32)> {
    if tok.text().len() == 1 && tok.text()[0] == b'(' {
        tel_fax_paren(tok, t, p, off)
    } else {
        tel_fax_bare(tok, t, p, off, wa_words)
    }
}

/// `FUN_1004bd70(...,1)` + `FUN_1004bf70(...,6)`: the word immediately
/// before `p` tokenizes (mode 6) as a number -- gates the bare `NNNN`/`NNN`
/// shapes (`FUN_10045190`'s tail `switch`, cases 8/0x10, `param_5`) when no
/// "(TEL)"/trigger prefix was found.
fn prev_word_is_number(t: &[u8], p: usize, off: i32) -> bool {
    let (w, back) = prev_word(t, (p as i32 - off) as usize, p);
    if back == 0 {
        return false;
    }
    match last_token(&w, 6) {
        Some(tok) => tok.len > 0 && tok.class == 2,
        None => false,
    }
}

/// `FUN_10044f90`: speak the matched "(TEL)"/"(FAX)"/trigger word first (if
/// any, its own span, `rec::PUNCT = 5`), then the 'N' entries, all sharing
/// one span, each non-final spoken record's `rec::PUNCT = 5`.
fn render(wa: &mut WaSpa, tel_fax: Option<&TelFax>, entries: &[Entry]) -> bool {
    if let Some(tf) = tel_fax {
        if !emit(wa, &tf.word, tf.start, tf.end, b'A', b'D', 6, None) {
            return false;
        }
        let n = wa.i16(0);
        if n > 0 {
            wa.set_i32(WaSpa::rec(n as usize - 1) + rec::PUNCT, 5);
        }
    }
    let start = entries[0].start;
    let end = entries[entries.len() - 1].pos_after;
    let ngroups = entries.iter().filter(|e| e.kind == b'N').count();
    let mut seen = 0;
    for e in entries {
        if e.kind != b'N' {
            continue;
        }
        let words = digit_group_words(&e.text);
        if !emit_phrase(wa, &words, start, end, b'D', b'D', 6) {
            return false;
        }
        seen += 1;
        if seen < ngroups {
            let n = wa.i16(0);
            if n > 0 {
                wa.set_i32(WaSpa::rec(n as usize - 1) + rec::PUNCT, 5);
            }
        }
    }
    true
}

pub fn phone(_fe: &FeSpa, wa: &mut WaSpa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let wa_words = wa.i16(0);
    let mut cap_p = p;
    let mut cap_off = off;
    let mut trigger = false;
    let mut tel_fax: Option<TelFax> = None;
    let mut prev_is_number = false;
    if let Some((entry, consumed)) = tel_fax_prefix(tok, t, p, off, wa_words) {
        cap_p += consumed as usize;
        cap_off += consumed;
        trigger = true;
        tel_fax = Some(entry);
    } else {
        prev_is_number = prev_word_is_number(t, p, off);
    }

    let entries = capture(t, cap_p, cap_off);
    if !entries.is_empty() {
        let code: Vec<u8> = entries.iter().map(|e| e.kind).collect();
        for count in (1..=entries.len()).rev() {
            let want = &code[..count];
            for (shape_idx, shape) in PHONE_SHAPES.iter().enumerate() {
                if shape.len() != count || *shape != want {
                    continue;
                }
                if shape_idx < 17 {
                    if !validate_n_lengths(&entries[..count]) {
                        continue;
                    }
                    if (shape_idx == 8 || shape_idx == 16) && !trigger && prev_is_number {
                        continue;
                    }
                } else {
                    if !trigger || !validate_extension_shape(shape_idx, &entries[..count]) {
                        continue;
                    }
                }
                if !render(wa, tel_fax.as_ref(), &entries[..count]) {
                    return 0;
                }
                return entries[count - 1].pos_after - off;
            }
        }
    }
    // `FUN_10044910`'s `if (iVar12 < 1) { ... }` tail: no digit-group shape
    // ever matched (or none were captured at all). If a "(TEL)"/trigger
    // prefix WAS consumed, it is still spoken alone (no `rec::PUNCT` pause
    // marker -- that's only set by the full `render` path above) and the
    // match succeeds with just the prefix's byte count; otherwise decline.
    match &tel_fax {
        Some(tf) => {
            if emit(wa, &tf.word, tf.start, tf.end, b'A', b'D', 6, None) {
                cap_off - off
            } else {
                0
            }
        }
        None => -1,
    }
}
