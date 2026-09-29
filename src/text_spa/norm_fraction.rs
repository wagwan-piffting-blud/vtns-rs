//! `FUN_1002fd60` (chain slot 7): fractions ("1/3", "1/4 de milla", "3 ½",
//! "1 1/2", bare "½").
//!
//! All four shapes the scanner's own fixed shape table (`PTR_DAT_10069264`,
//! confirmed via `read_memory`/`pefile`, not guessed: 4 live entries "F",
//! "NF", "N/N", "NN/N", `DAT_10069274` = 4) recognizes are ported:
//! - "F": a bare vulgar-fraction glyph alone (0xbc/0xbd/0xbe -- the `¼`/`½`/
//!   `¾` cp1252 code points, confirmed via `pefile` against `DAT_10079698`'s
//!   own stride-0x2a table, `FUN_10043860`, count `DAT_10079e74` = 3: entry 0
//!   = 0xbc -> "cuarto", entry 1 = 0xbd -> "medio", entry 2 = 0xbe -> "tres
//!   cuartos"). `norm_numexpr.rs` already has this same table's *keys* as
//!   `FRACTION_KEYS`/`is_fraction_key` for its own, unrelated purpose; this
//!   file needs the keys' *words* too, so it keeps its own small copy
//!   (`GLYPH_WORDS`) rather than reaching into another family's file.
//! - "NF": a number directly followed by one of those glyphs ("3 ½"), the
//!   `FUN_10043860`/`DAT_10079698` lookup this task named.
//! - "N/N": bare numerator-slash-denominator (already ported, unchanged).
//! - "NN/N": a mixed number, number-space-number-slash-number ("1 1/2").
//!
//! Traced from `FUN_1002fd60`'s own per-token scratch-buffer field writes
//! (each scanned sub-token's fixed 0x140-byte slot, base `local_170`;
//! slot k's own text sits at `local_170+0x28+k*0x140`, its own tag byte at
//! `local_170+0x1a+k*0x140`, matching this task's own numbers), and
//! `FUN_100303b0`/`FUN_10030220` (the renderers, both re-decompiled fresh via
//! Ghidra MCP against the live `vt_spa_violeta16.dll` to cross-check the
//! static dump):
//! - "F"'s word is written directly into its own slot during the *scan*
//!   itself (a straight copy of `DAT_10079698`'s plain word, no numeral
//!   prefix, no gender) -- `FUN_100303b0`'s own `param_2==0` case is
//!   accordingly a no-op (falls through its `if`/`else if` chain to the
//!   default `goto LAB_100306bf` with nothing rendered), confirmed by
//!   reading it, not assumed.
//! - "NF" (`FUN_100303b0`'s `param_2==1`): the number renders via
//!   `FUN_1004ca30` (`numwords::cardinal_to_words`) with the lookahead
//!   gender passed through *unmodified*; requires the glyph's own
//!   whitespace count `<= 1` (`*(int*)(param_1+0x148)`, i.e. the *second*
//!   sub-token's ws field -- confirmed against the same struct-offset
//!   arithmetic as "N/N"'s own `param_1+0x148`/`param_1+0x288` ws checks,
//!   which are this same field one and two slots further on).
//! - "NN/N" (`FUN_100303b0`'s `param_2==3`): the whole-number part renders
//!   via `FUN_1004ca30` too, but with the lookahead gender zeroed first when
//!   it is exactly `G_M` (`if ((short)param_3==0x4d) uVar12=0;` -- a quirk
//!   `FUN_100303b0` applies *only* on this branch, not on "NF"'s; confirmed
//!   by re-reading both branches side by side). The numerator/denominator
//!   pair then renders through the exact same `FUN_100306d0` call "N/N"
//!   uses (see [`frac_words`]), just against sub-tokens 1 and 3 instead of
//!   0 and 2. Requires the numerator's own ws `<= 1`, the slash's and the
//!   denominator's own ws `== 0` (`param_1+0x148`/`param_1+0x288`/
//!   `param_1+0x3c8`).
//! - `FUN_10030220` (the final per-record emitter) inserts the joining word
//!   "y" (`DAT_100644d0`, confirmed via `pefile`) right after the
//!   whole-number/lone-number's own word whenever the matched shape is "NF"
//!   *or* "NN/N" (`(param_4==1||param_4==3) && iVar4==0`) -- via the plain,
//!   non-splitting `emit` (`FUN_10035350`, origin/case `'A'`/`'D'`, same
//!   span as the number it follows), not `emit_phrase`/`emit_split`
//!   (`FUN_100354d0`, used for every other word here, including "F"'s own
//!   possibly-multi-word "tres cuartos").
//! - The scanner's own `'S'` sub-shape (a superscript `²`/`³` glued directly
//!   onto a preceding number, via `DAT_10070248` -- the same table
//!   `norm_math.rs`'s `SUPERSCRIPT` already reads for its own operand
//!   suffixes) is declined here: it never changes the accepted shape length
//!   or signature (it modifies the *previous* slot's own trailing fields in
//!   place, `piVar9[-1]`/`*piVar9`/`piVar9+2` relative to the *next*,
//!   not-yet-committed slot -- read in full, but its target fields don't
//!   line up with anything `FUN_10030220`/`FUN_100303b0` read for any of the
//!   four shapes ported here, so its only possible effect is on a fifth
//!   shape family this task didn't ask for, e.g. "N²/N"). A bare number
//!   immediately followed by a superscript, with nothing else around it,
//!   already declines in the real engine too (an 'N'-lead shape of length 1
//!   has no table entry), so this omission is unobservable outside that
//!   undescribed compound shape.
//!
//! Every emitted word uses `src=0xe` (`FUN_10030220`'s own literal `0xe` at
//! every `FUN_100354d0`/`FUN_10035350` call site, for every shape).

use super::numwords::{
    atoi, cardinal_to_words, emit_phrase, is_comma_grouped, is_period_grouped, lookahead_gender,
    ordinal_gender_from_lookahead, ordinal_word, strip_digits, G_DEFAULT, G_F, G_M,
};
use super::tokenize::{tokenize, Tok};
use super::{emit, FeSpa};
use crate::ling_spa::WaSpa;

const SRC: u32 = 0xe;

/// `DAT_10079698` (3 entries, stride 0x2a, confirmed via `read_memory`/
/// `pefile`): the vulgar-fraction glyph -> plain word table. See the module
/// doc for why this duplicates `norm_numexpr.rs`'s `FRACTION_KEYS` (that
/// file only needs the keys, not the words).
const GLYPH_WORDS: [(u8, &[u8]); 3] = [(0xbc, b"cuarto"), (0xbd, b"medio"), (0xbe, b"tres cuartos")];

/// `FUN_10043860(&DAT_10079698, key, 3)`: the glyph's plain word, or `None`
/// if `text` isn't exactly one of the three glyph bytes.
fn glyph_word(text: &[u8]) -> Option<&'static [u8]> {
    if text.len() != 1 {
        return None;
    }
    GLYPH_WORDS.iter().find(|&&(k, _)| k == text[0]).map(|&(_, w)| w)
}

/// `FUN_100306d0`'s denominator word for 11-20 (`s_onceavo_100692f8` down to
/// `s_veinteavo_10069284`, confirmed via `read_memory`/`pefile`).
fn denom_11_20(value: i32) -> &'static [u8] {
    match value {
        11 => b"onceavo",
        12 => b"doceavo",
        13 => b"treceavo",
        14 => b"catorceavo",
        15 => b"quinceavo",
        16 => b"dieciseisavo",
        17 => b"diecisieteavo",
        18 => b"dieciochavo",
        19 => b"diecinueveavo",
        20 => b"veinteavo",
        _ => b"",
    }
}

/// `FUN_100306d0`: the denominator word for `denom` (>= 2), given the
/// numerator's own value (`num`, to select singular/plural forms for the
/// dedicated 2/3/4 words) and the raw lookahead-gender value (`look`: `0`/
/// `0x4d` masculine default, `0x46` (`G_F`) feminine).
///
/// For denom 2/3/4, singular/plural is `num == 1` vs. not, straight from
/// the decompile's own outer `if (numerator == 1.0)` branch. For denom
/// 11-20 and the general (5-10, 21+) path, the decompile's *own* plural
/// check (`FUN_1004c100`/`plural`, gated as `if (local_108[0]!=0) {...}`
/// right before copying the denominator word out) reads `param_1` -- but by
/// that point `param_1` no longer holds the numerator's digit text: an
/// earlier, unconditional block in the same function (`FUN_1004ca30(&local_108,
/// param_1,...)`, run before any denom-specific branch) has already
/// overwritten it in place with the numerator's own *rendered word* ("un",
/// "una", or a cardinal like "dos"). `plural`/`FUN_1004c100` requires
/// digit-grouped input (`FUN_1004c130`/`FUN_1004c1e0`'s own `grouped()`
/// check), which a rendered word never satisfies, so the check is always
/// false and the plural suffix (`DAT_100644e8`, "s") never appends --
/// confirmed by re-decompiling `FUN_100306d0` fresh via Ghidra MCP and
/// tracing `param_1`'s writes, and by a probe capture ("2/7" -> "séptimo",
/// not "séptimos"; "11/20"/"13/20" -> "veinteavo", not "veinteavos"). The
/// original port here (round 3) had this backwards (`if num != 1 { push('s') }`);
/// fixed to never pluralize for these two branches.
fn denom_word(denom: i32, num: i32, look: i32) -> Vec<u8> {
    let fem = look == G_F;
    match denom {
        2 => {
            if num == 1 {
                if fem { b"media".to_vec() } else { b"medio".to_vec() }
            } else if fem {
                b"medias".to_vec()
            } else {
                b"medios".to_vec()
            }
        }
        // No feminine form exists for "tercio"/"tercios" -- a genuine
        // engine asymmetry, kept as read.
        3 => {
            if num == 1 {
                b"tercio".to_vec()
            } else {
                b"tercios".to_vec()
            }
        }
        4 => {
            if num == 1 {
                if fem { b"cuarta".to_vec() } else { b"cuarto".to_vec() }
            } else if fem {
                b"cuartas".to_vec()
            } else {
                b"cuartos".to_vec()
            }
        }
        11..=20 => {
            let mut word = denom_11_20(denom).to_vec();
            if fem {
                if let Some(last) = word.last_mut() {
                    if *last == b'o' {
                        *last = b'a';
                    }
                }
            }
            word
        }
        _ => {
            let mut word = Vec::new();
            ordinal_word(&mut word, denom, ordinal_gender_from_lookahead(look));
            word
        }
    }
}

/// `FUN_100306d0`, shared by "N/N" and "NN/N": the numerator word ("un",
/// bare, when its value is exactly 1; otherwise `cardinal_to_words` with the
/// *raw* lookahead-gender value passed straight through) paired with
/// [`denom_word`]'s denominator word.
fn frac_words(num_text: &[u8], num_val: i32, denom_val: i32, look: i32) -> (Vec<u8>, Vec<u8>) {
    let mut num_word = Vec::new();
    if num_val == 1 {
        num_word.extend_from_slice(b"un");
    } else {
        cardinal_to_words(&mut num_word, num_text, look);
    }
    (num_word, denom_word(denom_val, num_val, look))
}

/// The "y" joining word `FUN_10030220` inserts right after the whole-number/
/// lone-number's own word for the "NF" and "NN/N" shapes, via the plain
/// (non-splitting) `emit` (`FUN_10035350`), not `emit_phrase`.
fn emit_y(wa: &mut WaSpa, start: i32, end: i32) -> bool {
    emit(wa, b"y", start, end, b'A', b'D', SRC, None)
}

/// The "F" shape: a bare glyph, alone.
fn bare_glyph(wa: &mut WaSpa, tok: &Tok) -> i32 {
    let Some(word) = glyph_word(tok.text()) else {
        return -1;
    };
    if !emit_phrase(wa, word, tok.start, tok.end, b'D', b'D', SRC) {
        return 0;
    }
    tok.consumed
}

/// The "NF" shape: a number directly followed by a glyph.
fn number_glyph(fe: &FeSpa, wa: &mut WaSpa, tok: &Tok, num_text: &[u8], t: &[u8], p1: usize, off1: i32) -> i32 {
    let (glyph_tok, r1) = tokenize(t, p1, off1, 0xd);
    if glyph_tok.class != 6 || glyph_tok.ws > 1 {
        return -1;
    }
    let Some(word) = glyph_word(glyph_tok.text()) else {
        return -1;
    };

    let after = p1 + r1 as usize;
    let off_after = off1 + r1;
    let look = lookahead_gender(fe, t, after, off_after, false);

    let mut num_word = Vec::new();
    cardinal_to_words(&mut num_word, num_text, look);
    if num_word.is_empty() {
        return -1;
    }

    if !emit_phrase(wa, &num_word, tok.start, tok.end, b'D', b'D', SRC) {
        return 0;
    }
    if !emit_y(wa, tok.start, tok.end) {
        return 0;
    }
    if !emit_phrase(wa, word, glyph_tok.start, glyph_tok.end, b'D', b'D', SRC) {
        return 0;
    }
    tok.consumed + r1
}

/// The "N/N" shape: a bare numerator-slash-denominator run.
fn plain_fraction(fe: &FeSpa, wa: &mut WaSpa, t: &[u8], p: usize, off: i32) -> i32 {
    let (num_tok, r0) = tokenize(t, p, off, 0xd);
    if num_tok.class != 2 {
        return -1;
    }
    let num_text = num_tok.text().to_vec();
    if !is_period_grouped(&num_text) && !is_comma_grouped(&num_text) {
        return -1;
    }

    let p1 = p + r0 as usize;
    let off1 = off + r0;
    let (slash_tok, r1) = tokenize(t, p1, off1, 0xd);
    if slash_tok.class != 3 || slash_tok.text() != b"/" || slash_tok.ws != 0 {
        return -1;
    }

    let p2 = p1 + r1 as usize;
    let off2 = off1 + r1;
    let (denom_tok, r2) = tokenize(t, p2, off2, 0xd);
    if denom_tok.class != 2 || denom_tok.ws != 0 {
        return -1;
    }
    let denom_text = denom_tok.text().to_vec();
    if !is_period_grouped(&denom_text) && !is_comma_grouped(&denom_text) {
        return -1;
    }

    let num_digits = strip_digits(&num_text);
    let denom_digits = strip_digits(&denom_text);
    if num_digits.is_empty() || denom_digits.is_empty() {
        return -1;
    }
    let num_val = atoi(&num_digits);
    let denom_val = atoi(&denom_digits);
    if num_val == 0 || denom_val == 0 {
        return -1;
    }

    let after = p2 + r2 as usize;
    let off_after = off2 + r2;
    let look = lookahead_gender(fe, t, after, off_after, false);
    let (num_word, denom_word) = frac_words(&num_text, num_val, denom_val, look);

    if !emit_phrase(wa, &num_word, num_tok.start, num_tok.end, b'D', b'D', SRC) {
        return 0;
    }
    if !emit_phrase(wa, &denom_word, denom_tok.start, denom_tok.end, b'D', b'D', SRC) {
        return 0;
    }
    // `FUN_10030220`'s own trailing plural-flag read (`param_2[count*0x50+8]`,
    // one slot past the last committed one) is never written anywhere in
    // `FUN_1002fd60`'s own scan for any of the four shapes here -- confirmed
    // dead, not shape-specific, so it's skipped for all of them, not just
    // this one.
    r0 + r1 + r2
}

/// The "NN/N" shape: a mixed number, number-space-number-slash-number.
fn mixed_number(
    fe: &FeSpa, wa: &mut WaSpa, tok: &Tok, whole_text: &[u8], t: &[u8], p1: usize, off1: i32,
) -> i32 {
    let (num_tok, r1) = tokenize(t, p1, off1, 0xd);
    if num_tok.class != 2 || num_tok.ws > 1 {
        return -1;
    }
    let num_text = num_tok.text().to_vec();
    if !is_period_grouped(&num_text) && !is_comma_grouped(&num_text) {
        return -1;
    }

    let p2 = p1 + r1 as usize;
    let off2 = off1 + r1;
    let (slash_tok, r2) = tokenize(t, p2, off2, 0xd);
    if slash_tok.class != 3 || slash_tok.text() != b"/" || slash_tok.ws != 0 {
        return -1;
    }

    let p3 = p2 + r2 as usize;
    let off3 = off2 + r2;
    let (denom_tok, r3) = tokenize(t, p3, off3, 0xd);
    if denom_tok.class != 2 || denom_tok.ws != 0 {
        return -1;
    }
    let denom_text = denom_tok.text().to_vec();
    if !is_period_grouped(&denom_text) && !is_comma_grouped(&denom_text) {
        return -1;
    }

    let num_digits = strip_digits(&num_text);
    let denom_digits = strip_digits(&denom_text);
    if num_digits.is_empty() || denom_digits.is_empty() {
        return -1;
    }
    let num_val = atoi(&num_digits);
    let denom_val = atoi(&denom_digits);
    if num_val == 0 || denom_val == 0 {
        return -1;
    }

    let after = p3 + r3 as usize;
    let off_after = off3 + r3;
    let look = lookahead_gender(fe, t, after, off_after, false);

    // `FUN_100303b0`'s own gender-zeroing quirk for the whole-number part
    // only (`if ((short)param_3==0x4d) uVar12=0;`) -- "NF"'s own number
    // does *not* get this, confirmed by reading both branches.
    let whole_gender = if look == G_M { G_DEFAULT } else { look };
    let mut whole_word = Vec::new();
    cardinal_to_words(&mut whole_word, whole_text, whole_gender);
    if whole_word.is_empty() {
        return -1;
    }
    let (num_word, denom_word) = frac_words(&num_text, num_val, denom_val, look);

    if !emit_phrase(wa, &whole_word, tok.start, tok.end, b'D', b'D', SRC) {
        return 0;
    }
    if !emit_y(wa, tok.start, tok.end) {
        return 0;
    }
    if !emit_phrase(wa, &num_word, num_tok.start, num_tok.end, b'D', b'D', SRC) {
        return 0;
    }
    if !emit_phrase(wa, &denom_word, denom_tok.start, denom_tok.end, b'D', b'D', SRC) {
        return 0;
    }
    tok.consumed + r1 + r2 + r3
}

/// `FUN_1002fd60(wa, tok, t + p, off)`: dispatch to whichever of the four
/// live shapes ("F", "NF", "N/N", "NN/N" -- see the module doc) the token at
/// `t[p..]` matches. The three number-led shapes are mutually exclusive by
/// construction (the token right after the leading number is either a
/// glyph, another number, or the bare `/`), so peeking its class once is
/// enough to route to the one shape it could possibly be; a structural
/// mismatch there declines outright, matching the engine's own behaviour of
/// never retrying a shorter shape once the scan has moved past the leading
/// number (`FUN_1002fd60`'s own backtrack only ever shortens the *scanned*
/// signature, and every one of these three shapes has a distinct, mutually
/// exclusive second token).
pub fn fraction(fe: &FeSpa, wa: &mut WaSpa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    if tok.class == 6 {
        return bare_glyph(wa, tok);
    }
    if tok.class != 2 {
        return -1;
    }
    let num_text = tok.text().to_vec();
    if !is_period_grouped(&num_text) && !is_comma_grouped(&num_text) {
        return -1;
    }

    let p1 = p + tok.consumed as usize;
    let off1 = off + tok.consumed;
    let (peek, _) = tokenize(t, p1, off1, 0xd);
    match peek.class {
        6 => number_glyph(fe, wa, tok, &num_text, t, p1, off1),
        2 => mixed_number(fe, wa, tok, &num_text, t, p1, off1),
        3 | 8 if peek.text() == b"/" => plain_fraction(fe, wa, t, p, off),
        _ => -1,
    }
}
