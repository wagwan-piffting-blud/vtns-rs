//! Number-to-Spanish-words core shared by the number-family normalizers
//! (`FUN_1004c6f0`, `FUN_1004c830`, `FUN_1004c4b0`, `FUN_1004c5d0`,
//! `FUN_1004c0b0`, `FUN_1004ca30`, `FUN_1004d080`, `FUN_1004d990`,
//! `FUN_1004dad0`, `FUN_1004db30`).
//!
//! Gender/agreement codes match the engine's own: `G_M` ('M', 0x4d) and
//! `G_F` ('F', 0x46) come from a following noun's lexicon class; `G_2` is a
//! second "feminine" code that (unlike `G_F`) also drives the 200-900
//! hundreds words ("doscientas" vs "doscientos"). `FUN_1004ca30` promotes
//! `G_F` to `G_2` itself once the value exceeds 199 -- see `cardinal_to_words`.
//! `exact` is the engine's apocope flag (`param_4`): true forces the
//! masculine apocope ("un", "veintiun") regardless of gender, used for a
//! count that is itself followed by "mil"/"millones" (`FUN_1004dad0`,
//! `FUN_1004db30` always pass `exact=true` for their own multiplier group).
//!
//! `FUN_1004dc30`/`FUN_1004dca0` (the "billones"/"mil millones" split for
//! 10-15 digit numbers) and `FUN_1004e400`'s 1-20 ordinal core (used by
//! `norm_roman.rs` via `FUN_1004e070`'s value/text dispatch) are ported below.
//!
//! Also ported: `FUN_1004dd60` (the ordinal-suffix classifier for tokens like
//! "1ro"/"2da"/"100\xba", [`ordinal_suffix_classify`]), `FUN_1004e070`'s
//! text/gender dispatch for that classifier's codes ([`ordinal_suffix_render`]),
//! and `FUN_1004e400`'s hundreds/thousands/millions ordinal sub-tree
//! (`FUN_1004ecb0`/`FUN_1004eef0`/`FUN_1004efa0`, values 100-999,999,999,
//! [`ordinal_word`]) -- see each function's own doc comment for exactly what
//! is and isn't covered (in particular: the 10-15 digit "H"-magnitude split
//! and `FUN_1004f050`/`FUN_1004f100`, its own even-higher magnitude scale,
//! are NOT ported; both are unreachable from realistic ordinal-suffixed
//! text).

use super::tokenize::tokenize;
use super::{emit, FeSpa};
use crate::ling_spa::lexicon::{encode_key_spa, stricmp_spa};
use crate::ling_spa::WaSpa;

pub const G_DEFAULT: i32 = 0;
pub const G_M: i32 = 0x4d;
pub const G_F: i32 = 0x46;
pub const G_2: i32 = 2;

fn is_digit(c: u8) -> bool {
    c.is_ascii_digit()
}

/// `FUN_100258f0`: append `word` to `out`, space-joined (no leading space
/// when `out` is still empty).
pub fn append_word(out: &mut Vec<u8>, word: &[u8]) {
    if !out.is_empty() {
        out.push(b' ');
    }
    out.extend_from_slice(word);
}

/// `atoi`-equivalent (`FUN_100505b3`) on a plain digit string: a native
/// 32-bit accumulator that wraps on overflow exactly like the compiled `int`
/// loop does (no saturation) -- load-bearing for the 10-15 digit path, where
/// the engine itself atoi's the *full* digit string (for the `>199` gender
/// promotion check) before ever splitting it into groups.
pub(super) fn atoi(s: &[u8]) -> i32 {
    let mut v: i32 = 0;
    for &c in s {
        if !is_digit(c) {
            break;
        }
        v = v.wrapping_mul(10).wrapping_add((c - b'0') as i32);
    }
    v
}

/// `FUN_1004c0b0`: copy only digit characters of `src` into a fresh buffer
/// (drops any `.`/`,`).
pub fn strip_digits(src: &[u8]) -> Vec<u8> {
    src.iter().copied().filter(|&c| is_digit(c)).collect()
}

/// `FUN_1004c290` / `FUN_1004c3a0`: split at `dec_sep` into (integer digits,
/// fraction).
fn split_decimal(src: &[u8], dec_sep: u8) -> (Vec<u8>, Vec<u8>) {
    match src.iter().position(|&c| c == dec_sep) {
        Some(k) => (strip_digits(&src[..k]), src[k + 1..].to_vec()),
        None => (strip_digits(src), Vec::new()),
    }
}

/// `FUN_1004c130` / `FUN_1004c1e0`.
fn plural_grouped(src: &[u8], group_sep: u8, dec_sep: u8) -> bool {
    if src.is_empty() || !grouped(src, group_sep, dec_sep) || src.len() >= 0x1e {
        return false;
    }
    if !src.contains(&dec_sep) {
        return atoi(&strip_digits(src)) != 1;
    }
    let (int, frac) = split_decimal(src, dec_sep);
    atoi(&int) != 1 || atoi(&frac) != 0
}

/// `FUN_1004c100`: the number is not exactly one. Callers store it as the
/// short at rec `+0x12` of the last record they emitted.
pub fn plural(src: &[u8]) -> bool {
    plural_grouped(src, b'.', b',') || plural_grouped(src, b',', b'.')
}

/// `FUN_1004c4b0`: `src` is digits grouped in 3s by `.`, with an optional
/// `,`-introduced decimal tail (digits only, non-empty). Rejects a leading
/// `.` and a multi-character integer part starting with `0`.
pub fn is_period_grouped(src: &[u8]) -> bool {
    grouped(src, b'.', b',')
}

/// `FUN_1004c5d0`: the mirror, `,` grouping 3s with a `.`-introduced tail.
pub fn is_comma_grouped(src: &[u8]) -> bool {
    grouped(src, b',', b'.')
}

fn grouped(src: &[u8], group_sep: u8, dec_sep: u8) -> bool {
    if src.is_empty() {
        return false;
    }
    let mut before = Vec::new();
    let mut after = Vec::new();
    let mut dec = false;
    for (i, &c) in src.iter().enumerate() {
        if !is_digit(c) && c != b'.' && c != b',' {
            return false;
        }
        if dec {
            if c == b'.' || c == b',' {
                return false;
            }
            after.push(c);
        } else if c == dec_sep {
            dec = true;
        } else {
            if i == 0 && src[0] == group_sep {
                return false;
            }
            before.push(c);
        }
    }
    if dec && after.is_empty() {
        return false;
    }
    if before.len() > 1 && before[0] == b'0' {
        return false;
    }
    // The walk indexes `src`, not the integer part, over the integer part's length.
    let mut run = 0i32;
    for k in (0..before.len()).rev() {
        if src[k] == group_sep {
            if run != 3 {
                return false;
            }
            run = 0;
        } else {
            run += 1;
        }
    }
    true
}

/// `FUN_1004c830`: read `text` (mixed digits/`.`/`,`) one character at a
/// time -- `cero`.."nueve", `punto`, `coma` -- with no gender agreement.
pub fn digit_by_digit(out: &mut Vec<u8>, text: &[u8]) {
    for &c in text {
        let word: &[u8] = match c {
            b'0' => b"cero",
            b'1' => b"uno",
            b'2' => b"dos",
            b'3' => b"tres",
            b'4' => b"cuatro",
            b'5' => b"cinco",
            b'6' => b"seis",
            b'7' => b"siete",
            b'8' => b"ocho",
            b'9' => b"nueve",
            b'.' => b"punto",
            b',' => b"coma",
            _ => continue,
        };
        append_word(out, word);
    }
}

fn leaf_word(out: &mut Vec<u8>, value: i32, exact: bool, gender: i32) {
    let uno = |exact: bool, gender: i32| -> &'static [u8] {
        if exact {
            b"un"
        } else if gender == G_F || gender == G_2 {
            b"una"
        } else if gender == G_M {
            b"un"
        } else {
            b"uno"
        }
    };
    let word: &[u8] = match value {
        1 => uno(exact, gender),
        2 => b"dos",
        3 => b"tres",
        4 => b"cuatro",
        5 => b"cinco",
        6 => b"seis",
        7 => b"siete",
        8 => b"ocho",
        9 => b"nueve",
        10 => b"diez",
        11 => b"once",
        12 => b"doce",
        13 => b"trece",
        14 => b"catorce",
        15 => b"quince",
        16 => b"diecis\xe9is",
        17 => b"diecisiete",
        18 => b"dieciocho",
        19 => b"diecinueve",
        20 => b"veinte",
        21 => {
            if exact {
                b"veintiun"
            } else if gender == G_F || gender == G_2 {
                b"veintiuna"
            } else if gender == G_M {
                b"veintiun"
            } else {
                b"veintiuno"
            }
        }
        22 => b"veintid\xf3s",
        23 => b"veintitr\xe9s",
        24 => b"veinticuatro",
        25 => b"veinticinco",
        26 => b"veintis\xe9is",
        27 => b"veintisiete",
        28 => b"veintiocho",
        29 => b"veintinueve",
        _ => return,
    };
    append_word(out, word);
}

/// `FUN_1004d080`'s 30-99 leaf: tens word plus an optional " y <units>"
/// suffix built as one compound token (matching the engine's own
/// concatenate-then-append-as-one-word order).
fn tens_word(out: &mut Vec<u8>, value: i32, exact: bool, gender: i32) {
    let tens: &[u8] = match value / 10 {
        3 => b"treinta",
        4 => b"cuarenta",
        5 => b"cincuenta",
        6 => b"sesenta",
        7 => b"setenta",
        8 => b"ochenta",
        9 => b"noventa",
        _ => return,
    };
    let units = value % 10;
    if units == 0 {
        append_word(out, tens);
        return;
    }
    let suffix: &[u8] = match units {
        1 => {
            if exact {
                b" y un"
            } else if gender == G_F || gender == G_2 {
                b" y una"
            } else if gender == G_M {
                b" y un"
            } else {
                b" y uno"
            }
        }
        2 => b" y dos",
        3 => b" y tres",
        4 => b" y cuatro",
        5 => b" y cinco",
        6 => b" y seis",
        7 => b" y siete",
        8 => b" y ocho",
        9 => b" y nueve",
        _ => return,
    };
    let mut word = Vec::with_capacity(tens.len() + suffix.len());
    word.extend_from_slice(tens);
    word.extend_from_slice(suffix);
    append_word(out, &word);
}

/// `FUN_1004d990`: hundreds digit (1-9) plus a 0-99 remainder. `cien` only
/// when the remainder is exactly 0, else `ciento`; 200-900 agree with
/// `gender == G_2` specifically (not `G_F`) -- see `cardinal_to_words`.
fn hundreds_group(out: &mut Vec<u8>, h: i32, rem: i32, exact: bool, gender: i32) {
    if h != 0 {
        let word: &[u8] = match h {
            1 => {
                if rem == 0 {
                    b"cien"
                } else {
                    b"ciento"
                }
            }
            2 => {
                if gender == G_2 {
                    b"doscientas"
                } else {
                    b"doscientos"
                }
            }
            3 => {
                if gender == G_2 {
                    b"trecientas"
                } else {
                    b"trecientos"
                }
            }
            4 => {
                if gender == G_2 {
                    b"cuatrocientas"
                } else {
                    b"cuatrocientos"
                }
            }
            5 => {
                if gender == G_2 {
                    b"quinientas"
                } else {
                    b"quinientos"
                }
            }
            6 => {
                if gender == G_2 {
                    b"seiscientas"
                } else {
                    b"seiscientos"
                }
            }
            7 => {
                if gender == G_2 {
                    b"setecientas"
                } else {
                    b"setecientos"
                }
            }
            8 => {
                if gender == G_2 {
                    b"ochocientas"
                } else {
                    b"ochocientos"
                }
            }
            9 => {
                if gender == G_2 {
                    b"novecientas"
                } else {
                    b"novecientos"
                }
            }
            _ => b"",
        };
        append_word(out, word);
    }
    if rem != 0 {
        group_words(out, rem, exact, gender);
    }
}

/// `FUN_1004dad0`: thousands count (0-999) plus a 0-999 remainder. The
/// count is always read with `exact = true` (apocope before "mil").
fn thousands_group(out: &mut Vec<u8>, th: i32, rem: i32, exact: bool, gender: i32) {
    if th > 1 {
        group_words(out, th, true, gender);
    }
    if th != 0 {
        append_word(out, b"mil");
    }
    if rem != 0 {
        group_words(out, rem, exact, gender);
    }
}

/// `FUN_1004db30`: millions count (0-999) plus a 0-999999 remainder.
/// `count == 1` is a genuine engine quirk: "un millón" (singular) only when
/// `out` is still empty at this point; if something already precedes it,
/// the plural "un millones" is used instead (raw concatenation, no
/// re-check). `rem == 0` appends "de" when `gender != 0` ("un millón de
/// personas").
fn millions_group(out: &mut Vec<u8>, mi: i32, rem: i32, exact: bool, gender: i32) {
    if mi < 2 {
        if mi == 1 {
            if out.is_empty() {
                append_word(out, b"un");
                out.push(b' ');
                out.extend_from_slice(b"mill\xf3n");
            } else {
                append_word(out, b"un");
                out.extend_from_slice(b" millones");
            }
        }
    } else {
        group_words(out, mi, true, gender);
        append_word(out, b"millones");
    }
    if rem == 0 {
        if gender != 0 {
            append_word(out, b"de");
        }
        return;
    }
    group_words(out, rem, exact, gender);
}

/// `FUN_1004d080` (the `code == 0x4c` / "L" path only -- see the module
/// doc for the unported `0x48`/"H" billones-split path): 0 through
/// 999,999,999.
pub fn group_words(out: &mut Vec<u8>, value: i32, exact: bool, gender: i32) {
    if value <= 0 {
        return;
    }
    if value < 30 {
        leaf_word(out, value, exact, gender);
    } else if value < 100 {
        tens_word(out, value, exact, gender);
    } else if value < 1000 {
        hundreds_group(out, value / 100, value % 100, exact, gender);
    } else if value < 1_000_000 {
        thousands_group(out, value / 1000, value % 1000, exact, gender);
    } else if value <= 999_999_999 {
        millions_group(out, value / 1_000_000, value % 1_000_000, exact, gender);
    }
}

/// `FUN_1004dc30`: hundreds-level group for the "H" (mil-millones, 10^9)
/// magnitude scale. `h` = H-value/100 (0-9), `rem` = H-value%100 (0-99).
/// `code` is the tri-state exact/suffix selector threaded down from the
/// top (`FUN_1004ca30`'s digit split): 0 or 1 both mean plain apocope-only
/// (nonzero forces "un"/"veintiún" etc, same as `exact`), while 2 is a
/// distinct case selecting the bare "mil" suffix instead of "mil millones"
/// (used when the lower-order remainder itself already contains a
/// "millones" word, to avoid saying "millones" twice). H-value == 1 exactly
/// (`h==0, rem==1`) is a genuine engine quirk: the guard on the call to
/// `hundreds_group` excludes it, so that case speaks bare "mil millones"
/// with no leading "un".
fn h_hundreds_group(out: &mut Vec<u8>, h: i32, rem: i32, code: i32, gender: i32) {
    if h != 0 || rem > 1 {
        hundreds_group(out, h, rem, code != 0, gender);
    }
    if h != 0 || rem != 0 {
        if code == 2 {
            append_word(out, b"mil");
        } else {
            append_word(out, b"mil millones");
        }
    }
}

/// `FUN_1004dca0`: the "billones" (10^12) split. `count` = number of
/// billones (0-999), `rem` = the mil-millones (10^9) remainder (0-999).
/// `count == 1` is "un billón" (two separate words, verified byte-exact via
/// the engine's own `sprintf(" %s", "billón")` append at `FUN_1004fd76`/
/// 0x1004dd0a-0x1004dd33); `count >= 2` reads the count via the plain
/// (`'L'`) chain -- forced to apocope (`exact=true`) whenever there's a
/// mil-millones remainder -- then appends "billones".
fn billones_group(out: &mut Vec<u8>, count: i32, rem: i32, code: i32, gender: i32) {
    if count >= 2 {
        group_words(out, count, rem != 0 || code != 0, gender);
        append_word(out, b"billones");
    } else if count == 1 {
        append_word(out, b"un");
        append_word(out, b"bill\xf3n");
    }
    if rem != 0 {
        h_group(out, rem, code, gender);
    }
}

/// `FUN_1004d080`'s `0x48`/"H" code path: the mil-millones magnitude,
/// 0-999999 (values above that -- i.e. more than 15 total significant
/// digits -- are silently dropped, matching the engine's own `if (param_2 <
/// 1000000)` guard on the recursive `FUN_1004dca0` call).
fn h_group(out: &mut Vec<u8>, value: i32, code: i32, gender: i32) {
    if value > 999999 {
        return;
    }
    if value > 999 {
        billones_group(out, value / 1000, value % 1000, code, gender);
    } else {
        h_hundreds_group(out, value / 100, value % 100, code, gender);
    }
}

/// `FUN_1004ca30(out, text, gender)`: the top-level driver. `text` is the
/// raw number token (digits plus at most one `.`/`,` group-or-decimal
/// pattern). Ported for up to 15 significant digits (the engine's own
/// cutoff, `FUN_1004ca30`'s `if (n < 0x10)` / `if (0xf < n)` checks); 16+
/// digits fall back to `digit_by_digit` exactly as the engine does.
///
/// Two quirks specific to the *grouped-with-decimal* forms (`1.234,56` /
/// `1,234.56`, both branches below), verified from the disassembly rather
/// than approximated:
/// - The integer part's own gender is zeroed unless it is already `G_F`/
///   `G_2` (`uVar17` in `FUN_1004ca30`) -- a plain `G_M` lookahead gender is
///   discarded for the *grouped* forms only; the plain-digit form (no
///   separator) passes gender through unmodified.
/// - The fractional part's leading zeros are stripped one at a time via an
///   in-place byte shift that only ever overwrites the buffer's second byte
///   (`local_80._1_1_`); for a run of 2+ leading zeros this duplicates the
///   following digit into the text handed to the recursive call (e.g. "005"
///   recurses on "055", not "5"). Replicated verbatim in
///   [`shift_leading_zeros`]; a run of leading zeros reaching past the end
///   of a very short fractional string reads as an assumed zero pad (the
///   engine's own out-of-bounds stack read there is undefined and not
///   observable -- `strip_digits` filters the resulting NUL out downstream
///   either way).
pub fn cardinal_to_words(out: &mut Vec<u8>, text: &[u8], gender: i32) {
    let use_comma_as_decimal = is_period_grouped(text) && text.contains(&b',');
    let use_period_as_decimal =
        !is_period_grouped(text) && is_comma_grouped(text) && text.contains(&b'.');
    if use_comma_as_decimal || use_period_as_decimal {
        let sep = if use_comma_as_decimal { b',' } else { b'.' };
        let word_for_sep: &[u8] = if use_comma_as_decimal { b"coma" } else { b"punto" };
        let pos = text.iter().position(|&c| c == sep).unwrap();
        let int_part = &text[..pos];
        let frac_part = &text[pos + 1..];
        let digits = strip_digits(int_part);
        let n = digits.len();
        // `param_3 = 2` promotion happens in place in the engine and is
        // visible to the fractional recursion below -- unlike the plain
        // branch's own promotion, which stays local to this call.
        let mut gender = gender;
        if n > 0 {
            let value = atoi(&digits);
            if gender == G_F && value > 199 {
                gender = G_2;
            }
            let int_gender = if gender == G_F || gender == G_2 { gender } else { G_DEFAULT };
            if n <= 15 {
                if n > 9 {
                    let upper = atoi(&digits[..n - 9]);
                    let lower = atoi(&digits[n - 9..]);
                    let code = if lower < 1 { 0 } else if lower < 1_000_000 { 1 } else { 2 };
                    h_group(out, upper, code, G_DEFAULT);
                    group_words(out, lower, false, int_gender);
                } else {
                    group_words(out, value, false, int_gender);
                }
            } else {
                digit_by_digit(out, &digits);
            }
            // `if (0 < iVar7) && (*param_1=='\0')`: unconditional, applies to
            // both the grouped (n<=15) and digit-by-digit (n>15) sub-paths.
            if out.is_empty() {
                out.extend_from_slice(b"cero");
            }
        }
        if !frac_part.is_empty() {
            append_word(out, word_for_sep);
            let recurse_text = shift_leading_zeros(out, frac_part);
            cardinal_to_words(out, &recurse_text, gender);
        }
        return;
    }
    let digits = strip_digits(text);
    let n = digits.len();
    if n == 0 {
        return;
    }
    let value = atoi(&digits);
    let mut gender = gender;
    if gender == G_F && value > 199 {
        gender = G_2;
    }
    if n > 15 {
        digit_by_digit(out, &digits);
        return;
    }
    if n > 9 {
        let upper = atoi(&digits[..n - 9]);
        let lower = atoi(&digits[n - 9..]);
        let code = if lower < 1 { 0 } else if lower < 1_000_000 { 1 } else { 2 };
        h_group(out, upper, code, G_DEFAULT);
        group_words(out, lower, false, gender);
    } else {
        group_words(out, value, false, gender);
    }
    if out.is_empty() {
        out.extend_from_slice(b"cero");
    }
}

/// The fractional-part leading-zero walk documented on [`cardinal_to_words`]:
/// appends one "cero" per leading `'0'` digit to `out` and returns the (
/// possibly digit-duplicated, per the engine's own in-place shift bug) text
/// to recurse on.
fn shift_leading_zeros(out: &mut Vec<u8>, frac: &[u8]) -> Vec<u8> {
    if frac.first() != Some(&b'0') {
        return frac.to_vec();
    }
    append_word(out, b"cero");
    let mut buf = frac.to_vec();
    buf.push(0);
    if buf[1] == 0 {
        buf.pop();
        return buf;
    }
    let mut p: usize = 0;
    loop {
        if buf[1] != b'0' {
            break;
        }
        append_word(out, b"cero");
        let src = p + 2;
        buf[1] = if src < buf.len() { buf[src] } else { 0 };
        p += 1;
        if buf[1] == 0 {
            break;
        }
    }
    buf.pop();
    buf
}

/// `FUN_100354d0`: split `phrase` on spaces/tabs and `emit` each piece as
/// its own WA record, all sharing `start`/`end`/`origin`/`case`/`src`.
/// False (like `emit`) when the WA fills up mid-phrase.
pub fn emit_phrase(wa: &mut WaSpa, phrase: &[u8], start: i32, end: i32, origin: u8, case: u8, src: u32) -> bool {
    for word in phrase.split(|&c| c == b' ' || c == b'\t') {
        if word.is_empty() {
            continue;
        }
        if !emit(wa, word, start, end, origin, case, src, None) {
            return false;
        }
    }
    true
}

/// `FUN_1004e400`'s `'L'` code path, restricted to values 1-20 (all that
/// `norm_roman.rs`'s roman-numeral caller ever needs -- see the module doc
/// for why values >= 100 and the ordinal-suffix classifier are declined).
/// `gender`: `0x4d` forces the masculine apocope form ("primer"/"tercer");
/// any other nonzero value feminizes (replace the trailing "o" with "a" --
/// every one of these words ends in "o", matching the engine's own in-place
/// last-byte overwrite); `0` is the plain masculine default.
pub fn ordinal_word_1_20(out: &mut Vec<u8>, value: i32, gender: i32) {
    let apocope = gender == 0x4d;
    let feminine = gender != 0 && !apocope;
    let mut word: Vec<u8> = match value {
        1 if apocope => b"primer".to_vec(),
        1 => b"primero".to_vec(),
        2 => b"segundo".to_vec(),
        3 if apocope => b"tercer".to_vec(),
        3 => b"tercero".to_vec(),
        4 => b"cuarto".to_vec(),
        5 => b"quinto".to_vec(),
        6 => b"sexto".to_vec(),
        7 => b"s\xe9ptimo".to_vec(),
        8 => b"octavo".to_vec(),
        9 => b"noveno".to_vec(),
        10 => b"d\xe9cimo".to_vec(),
        11 => b"decimoprimero".to_vec(),
        12 => b"decimosegundo".to_vec(),
        13 => b"decimotercero".to_vec(),
        14 => b"decimocuarto".to_vec(),
        15 => b"decimoquinto".to_vec(),
        16 => b"decimosexto".to_vec(),
        17 => b"decimos\xe9ptimo".to_vec(),
        18 => b"decimoctavo".to_vec(),
        19 => b"decimonoveno".to_vec(),
        20 => b"vig\xe9simo".to_vec(),
        _ => return,
    };
    if feminine {
        if let Some(last) = word.last_mut() {
            if *last == b'o' {
                *last = b'a';
            }
        }
    }
    append_word(out, &word);
}

/// `FUN_1004e070`'s `param_3 == 0` gender mapping (the branch taken when the
/// "text" argument is already a plain decimal value, as from the
/// roman-numeral caller): the lookahead's own codes collapse to three --
/// `0x4d` (apocope-forced; the lexicon's ordinal-only class `1` code, NOT a
/// generic `G_M`, which does not force apocope here), `1` (feminine; any
/// nonzero-and-not-`0x4d` works since [`ordinal_word_1_20`] only tests
/// that), or `0` (masculine default, including a generic `G_M`).
pub fn ordinal_gender_from_lookahead(g: i32) -> i32 {
    if g == 1 {
        0x4d
    } else if g == G_F {
        1
    } else {
        0
    }
}

/// `FUN_1004c6f0`: gender/number class of the word at `t[pos..]` (tokenized
/// in mode 7, the hyphen-joining alpha mode), read from its lexicon
/// payload's numeric field (`FUN_100217f0`'s third field: present when
/// payload flags bit `0x02` is set, following the `pron` field when bit
/// `0x01` is also set; `decode_payload_spa` does not keep this field, see
/// its own doc comment) when the lexicon lookup hits. When it misses, the
/// two abbreviation-table fallbacks (`FUN_100286a0`/`abbrev::fallback_a`,
/// then `FUN_10033c00`/`abbrev::fallback_b`, tried in that order and only
/// the second if the first declines outright) are ported in full -- see
/// `abbrev.rs` for both. Either can only ever raise the result to `'F'`
/// (`local_198`'s own init value, `0`, is the only other outcome reachable
/// through them), never anything else, matching the real function's own
/// `return local_198;` tail. `exact` is `FUN_1004c6f0`'s own `param_4`
/// (nonzero for an ordinal context, as `norm_roman.rs`'s roman-numeral
/// ordinal caller and the ordinal-suffix caller
/// [`ordinal_suffix_render`]'s caller now pass); callers that don't
/// support ordinals still pass `false`.
pub fn lookahead_gender(fe: &FeSpa, t: &[u8], pos: usize, off: i32, exact: bool) -> i32 {
    let (tok, _) = tokenize(t, pos, off, 7);
    if tok.class != 1 || tok.len <= 1 {
        return G_DEFAULT;
    }
    let key = encode_key_spa(tok.text());
    let Some(payload) = fe.lex.lookup_encoded(&key) else {
        let mut gender_byte: u8 = 0;
        if !super::abbrev::fallback_a(t, pos, off, &mut gender_byte) {
            let _ = super::abbrev::fallback_b(t, pos, off, &mut gender_byte);
        }
        return gender_byte as i32;
    };
    let Some(&flags) = payload.first() else {
        return G_DEFAULT;
    };
    let rest = &payload[1..];
    let numeric_at: &[u8] = if flags & 1 != 0 {
        let end = rest.iter().position(|&c| c == 0 || c == 0xff).unwrap_or(rest.len());
        let after = &rest[end..];
        if after.first() == Some(&0xff) {
            &after[1..]
        } else {
            after
        }
    } else {
        rest
    };
    let class = if flags & 2 != 0 {
        let mut v: i32 = 0;
        for &c in numeric_at {
            if !c.is_ascii_digit() {
                break;
            }
            v = v * 10 + (c - b'0') as i32;
        }
        v
    } else {
        0
    };
    if (4..7).contains(&class) {
        G_F
    } else if exact && class == 1 {
        1
    } else if class != 0 {
        G_M
    } else {
        G_DEFAULT
    }
}

/// `FUN_1004e400`'s tens-ordinal word table (20-90 by tens digit), verified
/// against the engine's own string data (`read_memory` at
/// 0x1007ac34/.../0x1007abd4).
fn ordinal_tens_word(tens_digit: i32) -> &'static [u8] {
    match tens_digit {
        2 => b"vig\xe9simo",
        3 => b"trig\xe9simo",
        4 => b"cuadrag\xe9simo",
        5 => b"quincuag\xe9simo",
        6 => b"sexag\xe9simo",
        7 => b"septuag\xe9simo",
        8 => b"octog\xe9simo",
        9 => b"nonag\xe9simo",
        _ => b"",
    }
}

/// `FUN_1004ecb0`'s hundreds-ordinal word table (100-900 by hundreds digit),
/// verified against the engine's own string data (`read_memory` at
/// 0x1007ad18-0x1007ad9c). Note the engine's own "duocent\xe9simo" for
/// 200th (not the more standard "ducent\xe9simo") -- a genuine quirk, kept
/// as read.
fn hundreds_ordinal_word(h: i32) -> &'static [u8] {
    match h {
        1 => b"cent\xe9simo",
        2 => b"duocent\xe9simo",
        3 => b"tricent\xe9simo",
        4 => b"cuadringent\xe9simo",
        5 => b"quinquent\xe9simo",
        6 => b"sexcent\xe9simo",
        7 => b"septingent\xe9simo",
        8 => b"octingent\xe9simo",
        9 => b"noningent\xe9simo",
        _ => b"",
    }
}

/// `FUN_1004ecb0`: hundreds-ordinal word (`h` 1-9) plus a 0-99 ordinal
/// remainder (`rem`, via a recursive [`ordinal_word`] call -- always < 100).
/// Feminized (`gender != 0 && gender != 0x4d`, same rule as
/// [`ordinal_word_1_20`]'s leaf words) unlike the *tens* word's own quirkier
/// rule -- see [`ordinal_word`]'s doc.
fn hundreds_ordinal(out: &mut Vec<u8>, h: i32, rem: i32, gender: i32) {
    if h != 0 {
        let mut word = hundreds_ordinal_word(h).to_vec();
        if gender != 0 && gender != 0x4d {
            if let Some(last) = word.last_mut() {
                if *last == b'o' {
                    *last = b'a';
                }
            }
        }
        append_word(out, &word);
    }
    if rem != 0 {
        ordinal_word(out, rem, gender);
    }
}

/// `FUN_1004eef0`: thousands-ordinal count `th` (0-999, rendered as a plain
/// cardinal via [`group_words`] with apocope forced -- `FUN_1004d080`'s own
/// `param_4=1` -- skipped entirely when `th==1`, matching "mil\xe9simo"
/// alone for 1000th with no leading "un") plus the "mil\xe9simo" word
/// (feminized unless apocope) and a 0-999 ordinal remainder. `gender` is
/// passed straight into `group_words` as the *cardinal* gender argument even
/// though it's really one of `ordinal_word`'s own three codes -- a genuine
/// engine quirk (verified from the disassembly, `FUN_1004d080`'s own
/// `param_4` at this call site): since it's never `G_F`(0x46) or `G_2`(2),
/// the count's own hundreds digit (e.g. "doscientos" in "doscientos
/// milésimo") never feminizes regardless of the ordinal's real gender.
fn thousands_ordinal(out: &mut Vec<u8>, th: i32, rem: i32, gender: i32) {
    if th == 0 {
        ordinal_word(out, rem, gender);
        return;
    }
    if th > 1 {
        group_words(out, th, true, gender);
    }
    let mut word = b"mil\xe9simo".to_vec();
    if gender != 0 && gender != 0x4d {
        if let Some(last) = word.last_mut() {
            if *last == b'o' {
                *last = b'a';
            }
        }
    }
    append_word(out, &word);
    ordinal_word(out, rem, gender);
}

/// `FUN_1004efa0`: millions-ordinal count `mi` (0-999) plus the
/// "millon\xe9simo" word and a 0-999999 ordinal remainder. Same shape (and
/// same gender-passthrough quirk) as [`thousands_ordinal`].
fn millions_ordinal(out: &mut Vec<u8>, mi: i32, rem: i32, gender: i32) {
    if mi == 0 {
        ordinal_word(out, rem, gender);
        return;
    }
    if mi > 1 {
        group_words(out, mi, true, gender);
    }
    let mut word = b"millon\xe9simo".to_vec();
    if gender != 0 && gender != 0x4d {
        if let Some(last) = word.last_mut() {
            if *last == b'o' {
                *last = b'a';
            }
        }
    }
    append_word(out, &word);
    ordinal_word(out, rem, gender);
}

/// `FUN_1004e400`'s `'L'` code path, extended past `ordinal_word_1_20`'s 1-20
/// to the full 1-999,999,999 range needed by the ordinal-suffix classifier
/// (`FUN_1004dd60`/`FUN_1004e070`, [`ordinal_suffix_classify`]/
/// [`ordinal_suffix_render`], `norm_roman.rs`'s class-2 branch). Values 1-19
/// delegate to [`ordinal_word_1_20`] unchanged (verified identical against
/// this function's own `0x13<param_2` guard); 20-99 route through
/// [`ordinal_tens_word`] (`FUN_1004e400`'s own `param_2/10` switch), each
/// tens/units word appended as its own space-joined token (no " y" joiner,
/// unlike the cardinal `tens_word`). 100-999 ([`hundreds_ordinal`]),
/// 1000-999999 ([`thousands_ordinal`]), and 1000000-999999999
/// ([`millions_ordinal`]) are ported above. `gender` uses the same three
/// codes as `ordinal_word_1_20` (`0x4d`=masculine apocope,
/// nonzero-and-not-`0x4d`=feminine, `0`=masculine default) -- ported quirk:
/// the *tens* word alone (20/30/../90) feminizes for ANY nonzero gender
/// including `0x4d` (`FUN_1004e400`'s own `sVar3 != 0` check on the tens
/// word, vs `sVar3 != 0 && sVar3 != 0x4d` everywhere else, including the
/// units-remainder digit that follows it), so passing the apocope code for a
/// bare multiple of ten yields the feminine form (e.g. "vig\xe9sima") --
/// verified from the disassembly, not simplified away. Not ported: the
/// non-'L' ("H") magnitude code path (`FUN_1004f050`/`FUN_1004f100`, an even
/// higher scale than billones) -- never reached by [`ordinal_suffix_render`]
/// (which declines past 9 significant digits) or the roman-numeral caller
/// (max value 20).
pub fn ordinal_word(out: &mut Vec<u8>, value: i32, gender: i32) {
    if value <= 0 {
        return;
    }
    if value < 20 {
        ordinal_word_1_20(out, value, gender);
    } else if value < 100 {
        let tens = value / 10;
        let units = value % 10;
        let mut word = ordinal_tens_word(tens).to_vec();
        if gender != 0 {
            if let Some(last) = word.last_mut() {
                if *last == b'o' {
                    *last = b'a';
                }
            }
        }
        append_word(out, &word);
        if units != 0 {
            ordinal_word_1_20(out, units, gender);
        }
    } else if value < 1000 {
        hundreds_ordinal(out, value / 100, value % 100, gender);
    } else if value < 1_000_000 {
        thousands_ordinal(out, value / 1000, value % 1000, gender);
    } else if value <= 999_999_999 {
        millions_ordinal(out, value / 1_000_000, value % 1_000_000, gender);
    }
}

/// `FUN_1004b630`: `s` is non-empty and every embedded `.`/`,` has a digit on
/// both sides (a `,` is also allowed as the very first character, checked
/// only against what follows -- an asymmetry ported verbatim from the
/// disassembly) and isn't the last character. No grouping-size check, unlike
/// `is_period_grouped`/`is_comma_grouped`.
fn ordinal_number_ok(s: &[u8]) -> bool {
    if s.is_empty() {
        return false;
    }
    for i in 0..s.len() {
        let c = s[i];
        if is_digit(c) {
            continue;
        }
        if c == b'.' {
            if i > 0 && i + 1 < s.len() && is_digit(s[i - 1]) && is_digit(s[i + 1]) {
                continue;
            }
            return false;
        }
        if c == b',' {
            if i + 1 >= s.len() {
                return false;
            }
            if i > 0 && !is_digit(s[i - 1]) {
                return false;
            }
            if !is_digit(s[i + 1]) {
                return false;
            }
            continue;
        }
        return false;
    }
    true
}

/// `FUN_1004dd60`: classify an ordinal-suffix token's tail ("1ro", "8vo",
/// "100\xba", "3ero", "1a"...). Tries, in order: a 3-letter suffix
/// ("ero"/"era" at `DAT_1007a7bc`/`DAT_1007a508`), then a 2-letter suffix
/// cascade (`DAT_1007a048`.."no"/"na" pairs, all read via `read_memory`),
/// then a bare last-character check (0xba/0xaa ordinal-indicator bytes, or
/// literal `'o'`/`'a'`) -- each falling back to the next when the longer
/// candidate doesn't match, exactly as the engine's own `goto` cascade does
/// (both use the *real* token length throughout, so a longer token that
/// fails the 3-letter check still gets checked against the 2-letter and
/// then 1-letter candidates using its own true last bytes). `DAT_1007a508`
/// ("era", confirmed by `read_memory`) is a genuine engine bug: it's
/// compared against only the last 2 bytes before the token's own NUL (one
/// off from the 3-letter check beside it), so it can never match -- "3era"
/// therefore always falls through to the 2-letter cascade and is
/// misclassified there (matches "ra", `FUN_1004dd60`'s own behaviour,
/// ported as-is, not fixed).
///
/// Returns `None` (the engine's `uVar3&0xffff0000`, a zero low word) when no
/// candidate matches, or when the extracted digit text fails
/// `is_period_grouped`/`is_comma_grouped`, or when a real suffix match
/// doesn't agree with the number's last digit (`FUN_1004dd60`'s own
/// digit/suffix validation table, e.g. "2ro" -- "ro" is only valid after
/// 1 or 3). On success, returns `(code, digits)` where `digits` is the raw
/// text before the suffix (still possibly containing `.`/`,`) and `code` is
/// the engine's own final return value 1-6: 1/2 = masc/fem bare indicator or
/// bare letter, 3/4 = masc/fem 2-letter abbreviation, 5/6 = masc/fem 3-letter
/// "ero"/"era" (6 is unreachable, see above). `code` is also
/// `FUN_1004e070`'s own suffix-byte-count selector (1-2 strip 1 byte, 3-4
/// strip 2, 5-6 strip 3) -- see [`ordinal_suffix_render`].
pub fn ordinal_suffix_classify(text: &[u8]) -> Option<(i32, Vec<u8>)> {
    let l = text.len();
    if l == 0 {
        return None;
    }

    let mut three_letter = false;
    if l >= 4 && stricmp_spa(&text[l - 3..], b"ero") == 0 {
        three_letter = true;
    }

    let mut two_letter: Option<(i32, bool)> = None;
    if !three_letter && l >= 3 {
        let tail2 = &text[l - 2..];
        two_letter = if stricmp_spa(tail2, b"no") == 0 {
            Some((7, false))
        } else if stricmp_spa(tail2, b"na") == 0 {
            Some((7, true))
        } else if stricmp_spa(tail2, b"vo") == 0 {
            Some((6, false))
        } else if stricmp_spa(tail2, b"va") == 0 {
            Some((6, true))
        } else if stricmp_spa(tail2, b"mo") == 0 {
            Some((5, false))
        } else if stricmp_spa(tail2, b"ma") == 0 {
            Some((5, true))
        } else if stricmp_spa(tail2, b"to") == 0 {
            Some((4, false))
        } else if stricmp_spa(tail2, b"ta") == 0 {
            Some((4, true))
        } else if stricmp_spa(tail2, b"do") == 0 {
            Some((3, false))
        } else if stricmp_spa(tail2, b"da") == 0 {
            Some((3, true))
        } else if stricmp_spa(tail2, b"ro") == 0 {
            Some((2, false))
        } else if stricmp_spa(tail2, b"ra") == 0 {
            Some((2, true))
        } else {
            None
        };
    }

    let (uvar7, digits_end): (i32, usize) = if three_letter {
        (8, l - 3)
    } else if let Some((code, _)) = two_letter {
        (code, l - 2)
    } else {
        if l < 2 {
            return None;
        }
        let last = text[l - 1];
        if last == b'a' || last == b'o' {
            (1, l - 1)
        } else if last == 0xaa || last == 0xba {
            (0, l - 1)
        } else {
            return None;
        }
    };

    let digits = &text[..digits_end];
    if !is_period_grouped(digits) && !is_comma_grouped(digits) {
        return None;
    }
    let last_digit = digits[digits.len() - 1];

    let big_validation = (last_digit == b'1' && uvar7 == 2)
        || (last_digit == b'2' && uvar7 == 3)
        || (last_digit == b'8' && uvar7 == 6)
        || (last_digit == b'9' && uvar7 == 7)
        || (last_digit == b'3' && (uvar7 == 2 || uvar7 == 8))
        || ((last_digit == b'7' || last_digit == b'0') && uvar7 == 5)
        || (matches!(last_digit, b'4' | b'5' | b'6') && uvar7 == 4)
        || uvar7 == 1;

    let bare_code = |last: u8| -> i32 {
        if last != 0xaa && last != b'a' {
            1
        } else {
            2
        }
    };

    let code = if uvar7 == 0 {
        // Never in the validation table above -- always falls through here.
        bare_code(text[l - 1])
    } else if !big_validation {
        return None;
    } else if uvar7 == 1 {
        bare_code(text[l - 1])
    } else if uvar7 < 8 {
        3 + two_letter.map_or(0, |(_, fem)| fem as i32)
    } else {
        5 + (text[l - 1] == b'a') as i32
    };
    Some((code, digits.to_vec()))
}

/// `FUN_1004e070`'s ordinal-suffix render path: `code` is one of
/// [`ordinal_suffix_classify`]'s return codes 1-6 (selecting how many
/// trailing bytes of `raw_token` -- the ORIGINAL text, suffix still
/// attached -- to strip: 1/2/3 for codes 1-2/3-4/5-6); `lookahead` is the
/// raw `lookahead_gender(..., exact=true)` result. Masculine suffixes
/// (`code` odd) apocope (`0x4d`) only when the lookahead itself returned the
/// ordinal-class code `1`, else plain masculine (`0`); feminine suffixes
/// (`code` even) are unconditionally gender `1`, regardless of the lookahead
/// -- both verified from the disassembly's own `uVar9 & 1` odd/even split.
/// A `,`-decimal grouped token (e.g. "1,5to") skips the ordinal machinery
/// entirely and reads as a plain cardinal via `cardinal_to_words`, with the
/// engine's own `lookahead==1 -> 0x4d` gender rewrite kept as-is. Ported for
/// up to 9 significant digits (`FUN_1004e400`'s own 'L' magnitude,
/// 1-999,999,999, via [`ordinal_word`]); the engine's own 10-15 digit split
/// into an "H"+"L" pair (mirroring the cardinal billones split, but for
/// ordinals -- `FUN_1004f050`/`FUN_1004f100`, never read) is NOT ported, so
/// a 10+ digit ordinal-suffixed number is silently dropped (produces no
/// words but still consumes the token) exactly like the engine's own
/// `FUN_1004b630`-failure early-out -- realistically unreachable text.
pub fn ordinal_suffix_render(out: &mut Vec<u8>, raw_token: &[u8], code: i32, lookahead: i32) {
    let strip = match code {
        1 | 2 => 1,
        3 | 4 => 2,
        5 | 6 => 3,
        _ => return,
    };
    if raw_token.len() < strip {
        return;
    }
    let local_80 = &raw_token[..raw_token.len() - strip];
    if !ordinal_number_ok(local_80) {
        return;
    }
    if local_80.contains(&b',') {
        let gender = if lookahead == 1 { 0x4d } else { lookahead };
        cardinal_to_words(out, local_80, gender);
        return;
    }
    let gender = if code % 2 == 1 {
        if lookahead == 1 {
            0x4d
        } else {
            0
        }
    } else {
        1
    };
    let digits = strip_digits(raw_token);
    if digits.is_empty() || digits.len() > 9 {
        return;
    }
    let value = atoi(&digits);
    ordinal_word(out, value, gender);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cw(text: &[u8], gender: i32) -> String {
        let mut out = Vec::new();
        cardinal_to_words(&mut out, text, gender);
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn billones_split() {
        // 10 digits, exactly 1e9 -> "mil millones" with no leading "un".
        assert_eq!(cw(b"1000000000", G_DEFAULT), "mil millones");
        // 2 billones exactly.
        assert_eq!(cw(b"2000000000000", G_DEFAULT), "dos billones");
        // upper=1 (H-hundreds, rem==1 quirk: bare "mil", no leading "un")
        // with a lower remainder >= 1e6 (code==2, so "mil" not "mil
        // millones") -- 1,001,000,005.
        assert_eq!(cw(b"1001000005", G_DEFAULT), "mil un millones cinco");
        // upper=5 (H-hundreds, rem=5) with a lower remainder >= 1e6 --
        // 5,001,500,000.
        assert_eq!(cw(b"5001500000", G_DEFAULT), "cinco mil un millones quinientos mil");
    }

    #[test]
    fn ordinal_1_20() {
        let mut out = Vec::new();
        ordinal_word_1_20(&mut out, 8, 0);
        assert_eq!(out, b"octavo");
        let mut out = Vec::new();
        ordinal_word_1_20(&mut out, 1, 0x4d);
        assert_eq!(out, b"primer");
        let mut out = Vec::new();
        ordinal_word_1_20(&mut out, 3, 1);
        assert_eq!(out, b"tercera");
    }

    #[test]
    fn fraction_leading_zeros() {
        // Single leading zero: clean split, no duplication.
        assert_eq!(cw(b"3.05", G_DEFAULT), "tres punto cero cinco");
        // Two leading zeros: the engine's own byte-shift duplicates the
        // following digit (see shift_leading_zeros's doc).
        // "3.005" itself is '.'-grouped thousands (`FUN_1004c4b0` checks every
        // character), so the two-zero case needs a 4-digit fraction.
        assert_eq!(cw(b"3.005", G_DEFAULT), "tres mil cinco");
        assert_eq!(cw(b"3.0055", G_DEFAULT), "tres punto cero cero quinientos cincuenta y cinco");
    }

    fn ow(value: i32, gender: i32) -> Vec<u8> {
        let mut out = Vec::new();
        ordinal_word(&mut out, value, gender);
        out
    }

    #[test]
    fn ordinal_100_and_up() {
        assert_eq!(ow(100, 0), b"cent\xe9simo");
        assert_eq!(ow(105, 0), b"cent\xe9simo quinto");
        assert_eq!(ow(200, 0), b"duocent\xe9simo");
        assert_eq!(ow(21, 0), b"vig\xe9simo primero");
        assert_eq!(ow(20, 0x4d), b"vig\xe9sima");
        assert_eq!(ow(1000, 0), b"mil\xe9simo");
        assert_eq!(ow(2000, 0), b"dos mil\xe9simo");
        assert_eq!(ow(1_000_000, 0), b"millon\xe9simo");
    }

    fn osr(raw: &[u8], lookahead: i32) -> Option<Vec<u8>> {
        let (code, _) = ordinal_suffix_classify(raw)?;
        let mut out = Vec::new();
        ordinal_suffix_render(&mut out, raw, code, lookahead);
        Some(out)
    }

    #[test]
    fn ordinal_suffix_tokens() {
        assert_eq!(osr(b"8vo", 0).as_deref(), Some(&b"octavo"[..]));
        assert_eq!(osr(b"9no", 0).as_deref(), Some(&b"noveno"[..]));
        assert_eq!(osr(b"100\xba", 0).as_deref(), Some(&b"cent\xe9simo"[..]));
        assert_eq!(osr(b"105to", 0).as_deref(), Some(&b"cent\xe9simo quinto"[..]));
        assert_eq!(ordinal_suffix_classify(b"1er"), None);
        assert_eq!(ordinal_suffix_classify(b"3er"), None);
        assert_eq!(ordinal_suffix_classify(b"2ro"), None);
        assert_eq!(osr(b"1ro", 1).as_deref(), Some(&b"primer"[..]));
        assert_eq!(osr(b"1ro", 0).as_deref(), Some(&b"primero"[..]));
        assert_eq!(osr(b"1ra", 0).as_deref(), Some(&b"primera"[..]));
    }
}
