//! `FUN_1002b410` (chain slot 2): dates.
//!
//! Ported this pass: the classify loop (`FUN_1002b410` itself, minus the
//! multi-token day+month/day+year span lookahead -- see below), the 20-shape
//! pattern match, the D/M/Y resolve switch (`FUN_1002c8e0`) for shapes 0-0x10,
//! the fully-ambiguous 3-number cascade (`FUN_1002d4c0`, simplified -- see
//! [`cascade_dmy`]'s doc), and the emit switch (`FUN_1002bf10`).
//!
//! ## Corrections to the previous pass's map (verified against the decompile
//! and, where dense, the disassembly/memory reads this pass)
//! - `'O'` is **not** a generic "unrecognized word" fallback: it is emitted
//!   *only* for "de"/"del" (`DAT_100686c0`/`DAT_10069168`). `'T'` is "el"
//!   (`DAT_100686bc`) -- the previous doc had these two swapped. A genuinely
//!   unrecognized word (anything not de/del/el/en/AC-DC/weekday/month) fails
//!   `FUN_1002d8b0`'s digit-or-ordinal-suffix gate and simply **stops** the
//!   classify loop (falls to the shape-match tail with whatever entries
//!   exist so far) -- it is not a role at all.
//! - `'S'` is the AC/DC-era-marker match (`DAT_1006915c`/`10069158`/
//!   `10069154`/`10069150` = "AC"/"A.C"/"DC"/"D.C"), not a season table --
//!   there is no season table. The two weekday tables are `0x1006a510`
//!   (14 entries, abbrev-or-full key -> full name value, stride 0x27,
//!   binary search; role `'w'` lowercase) and `PTR_s_Domingo_1006a734`
//!   (7 pointers to the 7 full names themselves, alphabetically sorted;
//!   role `'W'` uppercase). Both are emitted identically (verbatim literal).
//! - `FUN_100505b3` is a plain `atoi` on the whole string; its first
//!   ("this") argument is decorative everywhere it is called with one
//!   (confirmed from `FUN_1002da80`, where the day *value* -- not the
//!   *length* -- is range-checked). Consequently `FUN_1002bf10`'s "day
//!   ordinal when `FUN_100505b3(..)==1`" is a check on the day's **numeric
//!   value**, not its text length: only day 1 gets the ordinal "primero"
//!   (correct Spanish -- "el primero de enero" but "el dos de enero").
//! - `FUN_100259c0` (used for role `'F'`, a full month name matched
//!   literally) is a **lowercase** fold (ASCII + Latin-1 accented-uppercase
//!   pairs, three CP1252 exceptions shifted by `+0x10`, the rest by
//!   `+0x20`), not an uppercase fold -- verified from its own decompile
//!   (`DAT_10065564`-style ASCII fold, then the CP1252 accented-uppercase
//!   shift table). It normalizes "Septiembre"/"SEPTIEMBRE" to "septiembre".
//!
//! ## Declined / simplified, precisely
//! - `FUN_1002dc20`/`FUN_1002dd40` (the multi-token day-list/day-range span
//!   lookahead that would collapse e.g. "27, 28 y 29" or "27 al 30" into
//!   one entry) are **not ported this pass either** -- the classify loop
//!   never attempts them, so a digit run always falls through to plain
//!   per-token classification. This does not affect the numeric
//!   (`27/09/2026`, `27-09-2026`, `9/27/26`) or `de`-mediated
//!   (`27 de septiembre de 2026`) shapes -- confirmed by tracing both
//!   through the real per-token path -- only a bare day-list/day-range
//!   collapses differently (falls back to ordinary token-by-token
//!   classification instead of one merged span entry). This round read
//!   both functions in full (not just referenced) and their own
//!   *algorithms* are now understood precisely:
//!   - `FUN_1002dc20` (day-list, "27, 28 y 29"): given an entry that is
//!     role `'N'` (an unresolved digit run, i.e. the entry's original
//!     tokenize class was `2`) and a valid day (`FUN_1002da80`), requires
//!     the token right after it to be a lone `,` (len 1, `ws<2`); then
//!     loops alternating "expect a day" / "expect a separator (`,` or
//!     `y`)", where a `y`-separator switches the *next* required day into
//!     "this one ends the list" mode (returning the position right after
//!     it), any length/`ws>1`/wrong-character check failing the whole
//!     thing (`-1`), and running out of text before a `y`-terminated day
//!     also declining.
//!   - `FUN_1002dd40` (day range, "27 al 30" / "27-30" / "27/30"): given a
//!     valid-day entry, tries a literal connector token (`a`/`y`/`&`, or
//!     the word "al" via a full-string compare) then a second valid day;
//!     failing that, tries a symbol connector (`-`, `/`, or the CP1252
//!     en/em-dash bytes `0x96`/`0x97`) then a second valid day, with an
//!     extra check that a *third* occurrence of one of `-/.`
//!     (`DAT_1006916c`) right after doesn't also look like a second range
//!     (declining if so, to avoid mis-parsing "27-30-31" as one range).
//!   Neither function's *caller-side* integration was attempted this
//!   pass: splicing either into `classify`'s loop needs tracing
//!   `FUN_1002b410`'s own ~500-line control flow (not yet read in full)
//!   to find the exact insertion point and the corresponding "merged
//!   span" emit path (there is no shape in [`PATTERNS`] for a 3+ day
//!   list, so a real port needs its own dedicated emit, not
//!   [`emit_date`]) -- a documented decline on the integration, not the
//!   algorithms themselves.
//! - `FUN_1002d910` (apostrophe-year, `'97`) is **not** reachable from
//!   `classify`'s own per-token loop and so is not ported: `'` is itself one
//!   of `tokenize`'s `SYMBOLS` characters, so "'97" always tokenizes as two
//!   separate tokens ("'" then "97"), never the combined text
//!   `FUN_1002d910` expects. The bare `'` token then fails
//!   `FUN_1002d8b0`'s gate (it is not a digit and, at length 1, does not
//!   qualify for the gate's own apostrophe exemption, which requires length
//!   >= 2) and stops the loop -- confirmed against the decompile, not
//!   guessed. `FUN_1002d9b0` (ordinal-suffix value, "1ro"/"8vo") is left
//!   declining (`-1` always) -- it depends on `FUN_1004dd60`, not ported in
//!   `numwords.rs`. Per `FUN_1002d8b0`'s own gate, this decline only bites a
//!   digit run that actually has a non-digit suffix (i.e. the tokenizer's
//!   own ordinal-suffix join already fired) -- it correctly falls through
//!   for plain digits without ever calling it.
//! - `FUN_1002d2b0` (the weekday-adjacency lookahead that sets `weekdayFlag`
//!   for patterns 9/0xe/0x11/0x12/0x13) is **not ported**; `weekday_flag` is
//!   hardcoded `false`. This is a no-op for pattern 9 (the flag only gates a
//!   redundant fast-path -- see [`resolve`]'s case 9) and for pattern 0xe's
//!   two-number D/M-vs-M/D branch (only reachable when `weekday_flag!=0`).
//!   Patterns 0x11/0x12 (a bare month/weekday alone) still succeed
//!   unconditionally for a *full-name* match (`'F'`/`'W'`) -- the engine's
//!   own gate there checks the entry's role for `'A'`/`'w'` specifically
//!   (the abbreviation-capable tables), not the pattern's shape, so only
//!   those two narrower sub-cases need `weekday_flag` and decline here.
//!   Pattern 0x13 (a bare number alone) always needs it and always
//!   declines.
//! - The "period right after an abbreviated month/weekday, absorb into the
//!   previous entry" special case (`sept.` in "sept. 27") is ported for the
//!   branch this family's probe corpus exercises (peek token present, valid,
//!   with ws in {0,1}); the nested `LAB_1002b670` re-entry after the
//!   `else if` body's own sub-checks collapses to "not absorbed" rather than
//!   re-running the outer gate a second time -- flagged in [`classify`].
//! - The "weekday name followed immediately by a comma" shape continuation
//!   (b410's `0 < iVar14 && local_228[.]=='W' && ...`) is not ported; a bare
//!   comma after a weekday simply stops the loop like any other
//!   unrecognized token (graceful, not a hard decline).
//!
//! Sources: `C:\tmp\vtwork\v4\decomp\1002b410.c`, `1002bf10.c`, `1002c8e0.c`,
//! `1002c6e0.c` (unused -- span-only, see above), `1002d4c0.c`, `1002d8b0.c`,
//! `1002d910.c`, `1002d9b0.c`, `1002da80.c`, `1002db30.c`, `1002dad0.c`,
//! `1002e0c0.c`, plus memory reads off `vt_spa_violeta16.dll` for the month
//! abbreviation table (`0x10069f90`, 24 entries), full month name table
//! (`0x1006a338`, 12 entries), spoken month words (`PTR_DAT_10069f58`,
//! index 1-12), weekday tables (`0x1006a510` 14 entries / `0x1006a734` 7
//! pointers), the 20 `PTR_s_WNOMON_10069098` shape strings, and the small
//! filler/marker string constants cited inline below.

use super::numwords::{cardinal_to_words, ordinal_word_1_20};
use super::tables::{CLS_S, HI};
use super::tokenize::{tokenize, Tok};
use super::{emit, emit::emit_split, FeSpa};
use crate::ling_spa::lexicon::stricmp_spa;
use crate::ling_spa::WaSpa;

fn digit(c: u8) -> bool {
    CLS_S[c as usize] & 0x10 != 0
}

fn all_digits(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&c| digit(c))
}

fn eqi(a: &[u8], b: &[u8]) -> bool {
    stricmp_spa(a, b) == 0
}

/// `atoi` (`FUN_100505b3`, decorative first arg dropped -- see module doc).
fn atoi(s: &[u8]) -> i32 {
    let mut v: i32 = 0;
    for &c in s {
        if !digit(c) {
            break;
        }
        v = v.wrapping_mul(10).wrapping_add((c - b'0') as i32);
    }
    v
}

/// `FUN_1002da80`: valid day -- all-digit, 1-2 chars, value 1..=31.
fn valid_day(s: &[u8]) -> i32 {
    if !all_digits(s) || s.is_empty() || s.len() > 2 {
        return -1;
    }
    let v = atoi(s);
    if v > 0 && v < 0x20 {
        v
    } else {
        -1
    }
}

/// `FUN_1002dad0`: valid month-as-number -- all-digit, 1-2 chars, 1..=12.
fn valid_month_num(s: &[u8]) -> i32 {
    if !all_digits(s) || s.is_empty() || s.len() > 2 {
        return -1;
    }
    let v = atoi(s);
    if v > 0 && v < 13 {
        v
    } else {
        -1
    }
}

/// `FUN_1002db30(text, 0)`: valid year -- all-digit; 2 chars -> 0..99;
/// 3 chars -> 100..999; 4 chars -> 1000..9999; 5+ chars -> no leading `0`,
/// value >= 10000. (Modes 1/2 are never called from this subtree.)
fn valid_year(s: &[u8]) -> i32 {
    if !all_digits(s) {
        return -1;
    }
    let n = s.len();
    if n < 2 {
        return -1;
    }
    let v = atoi(s);
    if n == 2 {
        return if (0..100).contains(&v) { v } else { -1 };
    }
    if n == 3 {
        return if (100..1000).contains(&v) { v } else { -1 };
    }
    if n == 4 && v > 999 {
        return v;
    }
    if s[0] == b'0' || v < 10000 {
        return -1;
    }
    v
}

/// `FUN_1002e0c0`: year value-text to words. `0` -> "dos mil"; `1..=9` ->
/// "dos mil " + cardinal (avoids the ungrammatical "dos mil, cero, uno");
/// else plain `cardinal_to_words` (a bare 2-digit year like "26" therefore
/// reads as just "veintiséis", with no century prefix at all -- a genuine
/// engine asymmetry, kept as-is).
fn year_to_words(out: &mut Vec<u8>, text: &[u8]) {
    let v = atoi(text);
    if v < 0 {
        return;
    }
    if v == 0 {
        out.extend_from_slice(b"dos mil");
        return;
    }
    if v < 10 {
        out.extend_from_slice(b"dos mil");
        let mut word = Vec::new();
        cardinal_to_words(&mut word, text, 0);
        out.push(b' ');
        out.extend_from_slice(&word);
        return;
    }
    cardinal_to_words(out, text, 0);
}

/// `FUN_1002bf10`'s day reading: ordinal "primero" only for value 1 (see
/// module doc's `FUN_100505b3` correction), else cardinal.
fn day_words(out: &mut Vec<u8>, text: &[u8]) {
    if atoi(text) == 1 {
        ordinal_word_1_20(out, 1, 0);
    } else {
        cardinal_to_words(out, text, 0);
    }
}

/// `FUN_1002d9b0`: ordinal-suffix numeric-value reader ("1ro"->1, "8vo"->8).
/// **Declined** -- depends on `FUN_1004dd60`, not ported (see `numwords.rs`).
fn ordinal_suffix_value(_s: &[u8]) -> i32 {
    -1
}

/// `FUN_1002d8b0`: every character is a digit, except a token starting with
/// `'` (length >= 2) may have that leading apostrophe -- else the whole
/// token must be a valid ordinal-suffixed number (declined, see above).
fn ordinal_gate(s: &[u8]) -> bool {
    let n = s.len() as i32;
    for (i, &c) in s.iter().enumerate() {
        let exempt = i == 0 && n >= 2 && c == b'\'';
        if !digit(c) && !exempt {
            return ordinal_suffix_value(s) >= 0;
        }
    }
    true
}

/// `FUN_100259c0`: lowercase fold -- ASCII uppercase (`+0x20`), three CP1252
/// exceptions (`0x8a`/`0x8e`/`0x9f` -> `+0x10`, `0xff` fixed), else any other
/// Latin-1 accented-uppercase byte (`HI[..]&0x80`) also `+0x20`.
fn fold_lower(text: &[u8]) -> Vec<u8> {
    text.iter()
        .map(|&b| {
            if b < 0x80 || HI[b as usize] & 0x80 == 0 {
                if CLS_S[b as usize] & 0x80 != 0 {
                    b + 0x20
                } else {
                    b
                }
            } else if matches!(b, 0x8a | 0x8e | 0x9f) {
                if b == 0xff {
                    0xff
                } else {
                    b + 0x10
                }
            } else {
                b.wrapping_add(0x20)
            }
        })
        .collect()
}

/// `0x10069f90`, stride 0x27, 24 entries (`DAT_10079dc4`): month
/// abbreviation -> month number (both the 2- and 3-letter forms).
const MONTH_ABBR: &[(&[u8], u8)] = &[
    (b"AB", 4), (b"ABR", 4), (b"AG", 8), (b"AGO", 8), (b"DI", 12), (b"DIC", 12), (b"ENE", 1), (b"FB", 2), (b"FEB", 2),
    (b"JL", 7), (b"JN", 6), (b"JUL", 7), (b"JUN", 6), (b"MAR", 3), (b"MAY", 5), (b"MR", 3), (b"MY", 5), (b"NO", 11),
    (b"NOV", 11), (b"OC", 10), (b"OCT", 10), (b"SEP", 9), (b"SEPT", 9), (b"SP", 9),
];

/// `0x1006a338`, stride 0x27, 12 entries (`DAT_10079dc8`): full month name
/// -> month number.
const MONTH_FULL: &[(&[u8], u8)] = &[
    (b"ABRIL", 4), (b"AGOSTO", 8), (b"DICIEMBRE", 12), (b"ENERO", 1), (b"FEBRERO", 2), (b"JULIO", 7), (b"JUNIO", 6),
    (b"MARZO", 3), (b"MAYO", 5), (b"NOVIEMBRE", 11), (b"OCTUBRE", 10), (b"SEPTIEMBRE", 9),
];

/// `PTR_DAT_10069f58`, index 1-12 (index 0 unused -- points at an empty
/// string): the spoken month word.
const MONTH_WORD: [&[u8]; 13] =
    [b"", b"Enero", b"Febrero", b"Marzo", b"Abril", b"Mayo", b"Junio", b"Julio", b"Agosto", b"Septiembre", b"Octubre", b"Noviembre", b"Diciembre"];

/// `0x1006a510`, stride 0x27, 14 entries (`DAT_10079dcc`): weekday key
/// (abbreviated or full, case-insensitive) -> full proper-cased name.
/// Role `'w'` (lowercase).
const WEEKDAY_FULL: &[(&[u8], &[u8])] = &[
    (b"Do", b"Domingo"), (b"Dom", b"Domingo"), (b"Ju", b"Jueves"), (b"Jue", b"Jueves"), (b"Lu", b"Lunes"), (b"Lun", b"Lunes"),
    (b"Ma", b"Martes"), (b"Mar", b"Martes"), (b"Mi", b"Mi\xe9rcoles"), (b"Mie", b"Mi\xe9rcoles"), (b"Sa", b"S\xe1bado"),
    (b"Sab", b"S\xe1bado"), (b"Vi", b"Viernes"), (b"Vie", b"Viernes"),
];

/// `PTR_s_Domingo_1006a734`, 7 pointers (`DAT_10079dd0`), alphabetically
/// sorted, each both the search key and the value. Role `'W'` (uppercase).
const WEEKDAY_SHORT: &[&[u8]] = &[b"Domingo", b"Jueves", b"Lunes", b"Martes", b"Mi\xe9rcoles", b"S\xe1bado", b"Viernes"];

/// `PTR_s_WNOMON_10069098`, the 20 pattern shapes, index = pattern index
/// used throughout `resolve`/`emit_date`/`cascade_dmy`.
const PATTERNS: [&[u8]; 20] = [
    b"WNOMON", b"WTNOM", b"NOMON", b"WNMN", b"WNOM", b"WMNN", b"NMN", b"NOM", b"MNN", b"NNN", b"WNM", b"NM", b"IN", b"MN", b"NN", b"SN", b"NS",
    b"M", b"W", b"N",
];

/// The pattern-index range to try for a given entry count (b410's tail
/// switch on `iVar14`; `DAT_100690e8` = 20 for the count-1 upper bound).
fn pattern_range(count: usize) -> Option<(usize, usize)> {
    match count {
        6 => Some((0, 1)),
        5 => Some((1, 3)),
        4 => Some((3, 6)),
        3 => Some((6, 11)),
        2 => Some((11, 17)),
        1 => Some((17, 20)),
        _ => None,
    }
}

#[derive(Clone, Default)]
struct DateEntry {
    ws: i32,
    start: i32,
    end: i32,
    role: u8,
    /// Raw token text (also the role-`'D'`/`'M'`/`'Y'` numeric value text
    /// once resolved -- entries never lose their original digit text).
    text: Vec<u8>,
    /// The literal spoken/verbatim text for roles `'w'`/`'W'`/`'S'`/`'A'`/`'F'`
    /// (unused for `'D'`/`'M'`/`'Y'`/`'I'`/`'O'`/`'T'`, which derive their
    /// output from `text` or their role alone at emit time).
    word: Vec<u8>,
}

/// `FUN_1002b410`'s classify loop (up to its shape-match tail, handled by
/// [`date`] itself). Builds up to 6 entries and their shape string; the loop
/// always terminates gracefully into the tail rather than hard-declining
/// (mirroring `goto LAB_1002bd87` everywhere in the original).
fn classify(t: &[u8], p0: usize, off0: i32) -> Vec<DateEntry> {
    let mut entries: Vec<DateEntry> = Vec::new();
    let mut sub_role: Vec<u8> = Vec::new();
    let mut pending_sep: i32 = -1;
    let mut sep_count = 0i32;
    let (mut p, mut off) = (p0, off0);

    'main: loop {
        if entries.len() >= 6 {
            break;
        }
        let (tok, consumed) = tokenize(t, p, off, 3);
        if consumed == 0 || tok.class == 10 {
            break;
        }
        let text = tok.text().to_vec();
        let cur_idx = sub_role.len() as i32;

        // Lone date-separator token ("/", "-", ".", ",").
        if pending_sep == -1 && tok.len == 1 && !text.is_empty() && b"/-.,".contains(&text[0]) {
            let mut absorb = false;
            if text[0] == b'.' && tok.ws == 0 && cur_idx > 0 && matches!(sub_role[cur_idx as usize - 1], b'A' | b'w') {
                let (peek, _) = tokenize(t, p + consumed as usize, off + consumed, 3);
                if peek.len > 0 {
                    if peek.ws < 1 {
                        absorb = peek.len == 1 && matches!(peek.text().first(), Some(b',') | Some(b'/'));
                    } else if peek.ws > 1 || !ordinal_gate(peek.text()) {
                        // `LAB_1002b670` re-entry after this branch's own
                        // checks is simplified to "not absorbed" -- see the
                        // module doc.
                        absorb = peek.ws > 0 && peek.len < 2 && eqi(peek.text(), b"el");
                    } else {
                        absorb = true;
                    }
                }
            }
            if absorb {
                if let Some(last) = entries.last_mut() {
                    last.end += 1;
                }
                sub_role.push(text[0]);
                p += consumed as usize;
                off += consumed;
                continue 'main;
            }
            if cur_idx == 0 || sep_count > 2 {
                break;
            }
            sub_role.push(text[0]);
            sep_count += 1;
            pending_sep = cur_idx;
            p += consumed as usize;
            off += consumed;
            continue 'main;
        }

        let start = tok.start;
        let end = off + consumed;
        let role;
        let mut word: Vec<u8> = Vec::new();

        if eqi(&text, b"de") || eqi(&text, b"del") {
            role = b'O';
        } else if eqi(&text, b"el") {
            role = b'T';
        } else if cur_idx == 0 && eqi(&text, b"en") {
            role = b'I';
        } else if eqi(&text, b"AC") || eqi(&text, b"A.C") || eqi(&text, b"DC") || eqi(&text, b"D.C") {
            role = b'S';
            // Literal-case check on the raw text (not the case-insensitive
            // match above) -- ported exactly, including the quirk that a
            // lowercase "ac"/"a.c" therefore gets the "DC" word instead.
            word = if text.first() == Some(&b'A') { b"AC".to_vec() } else { b"DC".to_vec() };
        } else if let Some(&(_, full)) = WEEKDAY_FULL.iter().find(|&&(k, _)| eqi(k, &text)) {
            role = b'w';
            word = full.to_vec();
        } else if let Some(&full) = WEEKDAY_SHORT.iter().find(|&&k| eqi(k, &text)) {
            role = b'W';
            word = full.to_vec();
        } else if let Some(&(_, num)) = MONTH_ABBR.iter().find(|&&(k, _)| eqi(k, &text)) {
            role = b'A';
            word = MONTH_WORD[num as usize].to_vec();
        } else if MONTH_FULL.iter().any(|&(k, _)| eqi(k, &text)) {
            role = b'F';
            word = fold_lower(&text);
        } else if ordinal_gate(&text) {
            // All-digit text trivially passes the gate (see `ordinal_gate`);
            // a token with a genuine ordinal suffix also reaches here and
            // is declined by the gate itself (`ordinal_suffix_value`) --
            // handled by the `else` arm below in that case.
            role = b'N';
        } else {
            // Unrecognized word, or a digit run with an ordinal suffix
            // (declined): stop the loop, matching every `goto LAB_1002bd87`
            // the real engine takes here.
            break;
        }

        if pending_sep != -1 {
            if pending_sep + 1 != cur_idx {
                break;
            }
            pending_sep = -1;
        }
        sub_role.push(role);
        entries.push(DateEntry { ws: tok.ws, start, end, role, text, word });
        p += consumed as usize;
        off += consumed;
    }
    entries
}

/// `FUN_1002d4c0`, simplified: its only caller (`resolve`'s case 9) always
/// invokes it with all three entries still ambiguous (`param_2` = "NNN"),
/// which makes every one of the function's earlier `*param_2==role` gated
/// shortcuts unreachable dead code (they require a position already
/// resolved, which never happens before this call in this port -- see the
/// module doc for why the span lookahead that could pre-resolve one is not
/// ported). What remains is the final, unconditional priority cascade:
/// D-M-Y, then Y-M-D, then M-D-Y.
fn cascade_dmy(e: &mut [DateEntry; 3]) -> bool {
    if valid_day(&e[0].text) >= 0 && valid_month_num(&e[1].text) >= 0 && valid_year(&e[2].text) >= 0 {
        e[0].role = b'D';
        e[1].role = b'M';
        e[2].role = b'Y';
        return true;
    }
    if valid_year(&e[0].text) >= 0 && valid_month_num(&e[1].text) >= 0 && valid_day(&e[2].text) >= 0 {
        e[0].role = b'Y';
        e[1].role = b'M';
        e[2].role = b'D';
        return true;
    }
    if valid_month_num(&e[0].text) >= 0 && valid_day(&e[1].text) >= 0 && valid_year(&e[2].text) >= 0 {
        e[0].role = b'M';
        e[1].role = b'D';
        e[2].role = b'Y';
        return true;
    }
    false
}

/// `FUN_1002c8e0`: resolve the ambiguous `'N'` entries' roles for the
/// matched pattern. `weekday_flag` is always `false` in this port (see
/// module doc) -- cases 9 and 0xe degrade gracefully; 0x11/0x12 only
/// decline for their `'A'`/`'w'` sub-case; 0x13 always declines.
fn resolve(entries: &mut [DateEntry], pattern_idx: usize, weekday_flag: bool) -> bool {
    let n = entries.len();
    match pattern_idx {
        0 => {
            // "WNOMON": ambiguous at 1 and 5.
            if valid_day(&entries[1].text) >= 0 && valid_year(&entries[5].text) >= 0 {
                entries[1].role = b'D';
                entries[5].role = b'Y';
                return true;
            }
            false
        }
        1 => {
            // "WTNOM": ambiguous at 2 only -- role 'D'.
            if valid_day(&entries[2].text) >= 0 {
                entries[2].role = b'D';
                return true;
            }
            false
        }
        2 => {
            // "NOMON": ambiguous at 0 and 4.
            if valid_day(&entries[0].text) >= 0 && valid_year(&entries[4].text) >= 0 {
                entries[0].role = b'D';
                entries[4].role = b'Y';
                return true;
            }
            false
        }
        3 => {
            // "WNMN": ambiguous at 1 and 3.
            if valid_day(&entries[1].text) >= 0 && valid_year(&entries[3].text) >= 0 {
                entries[1].role = b'D';
                entries[3].role = b'Y';
                return true;
            }
            false
        }
        4 | 10 => {
            // "WNOM" / "WNM": ambiguous at 1 only -- role 'D'.
            if valid_day(&entries[1].text) >= 0 {
                entries[1].role = b'D';
                return true;
            }
            false
        }
        5 => {
            // "WMNN": ambiguous at 2 and 3.
            if valid_day(&entries[2].text) >= 0 && valid_year(&entries[3].text) >= 0 {
                entries[2].role = b'D';
                entries[3].role = b'Y';
                return true;
            }
            false
        }
        6 => {
            // "NMN": ambiguous at 0 and 2, either D..Y or Y..D order.
            if valid_day(&entries[0].text) >= 0 && valid_year(&entries[2].text) >= 0 {
                entries[0].role = b'D';
                entries[2].role = b'Y';
                return true;
            }
            if valid_year(&entries[0].text) >= 0 && valid_day(&entries[2].text) >= 0 {
                entries[0].role = b'Y';
                entries[2].role = b'D';
                return true;
            }
            false
        }
        7 | 11 => {
            // "NOM" / "NM": ambiguous at 0 only -- role 'D'.
            if valid_day(&entries[0].text) >= 0 {
                entries[0].role = b'D';
                return true;
            }
            false
        }
        8 => {
            // "MNN": ambiguous at 1 and 2.
            if valid_day(&entries[1].text) >= 0 && valid_year(&entries[2].text) >= 0 {
                entries[1].role = b'D';
                entries[2].role = b'Y';
                return true;
            }
            false
        }
        9 => {
            // "NNN": the fully-ambiguous cascade. The engine's own
            // `param_1[0x52]!=param_1[0xa2]` gate compares entries 1 and 2's
            // `ws` fields (whitespace-before counts); ported literally.
            if entries[1].ws != entries[2].ws {
                return false;
            }
            let mut e3: [DateEntry; 3] = [entries[0].clone(), entries[1].clone(), entries[2].clone()];
            if !cascade_dmy(&mut e3) {
                return false;
            }
            entries[0].role = e3[0].role;
            entries[1].role = e3[1].role;
            entries[2].role = e3[2].role;
            true
        }
        0xc => {
            // "IN": ambiguous at 1 only -- role 'Y' ("en <year>").
            if valid_year(&entries[1].text) >= 0 {
                entries[1].role = b'Y';
                return true;
            }
            false
        }
        0xd => {
            // "MN": try year first, else day.
            if valid_year(&entries[1].text) >= 0 {
                entries[1].role = b'Y';
                return true;
            }
            if valid_day(&entries[1].text) >= 0 {
                entries[1].role = b'D';
                return true;
            }
            false
        }
        0xe => {
            // "NN": both ambiguous, no separating filler. Only reachable
            // (per the engine) with weekday_flag != 0 -- always false here.
            let _ = n;
            if !weekday_flag {
                return false;
            }
            if valid_year(&entries[0].text) >= 0 && valid_month_num(&entries[1].text) >= 0 {
                entries[0].role = b'Y';
                entries[1].role = b'M';
                return true;
            }
            if valid_month_num(&entries[0].text) >= 0 && valid_year(&entries[1].text) >= 0 {
                entries[0].role = b'M';
                entries[1].role = b'Y';
                return true;
            }
            if valid_day(&entries[0].text) >= 0 && valid_month_num(&entries[1].text) >= 0 {
                entries[0].role = b'D';
                entries[1].role = b'M';
                return true;
            }
            if valid_month_num(&entries[0].text) >= 0 && valid_day(&entries[1].text) >= 0 {
                entries[0].role = b'M';
                entries[1].role = b'D';
                return true;
            }
            false
        }
        0xf => {
            // "SN": ambiguous at 1 only -- role 'Y'.
            if valid_year(&entries[1].text) >= 0 {
                entries[1].role = b'Y';
                return true;
            }
            false
        }
        0x10 => {
            // "NS": ambiguous at 0 only -- role 'Y'.
            if valid_year(&entries[0].text) >= 0 {
                entries[0].role = b'Y';
                return true;
            }
            false
        }
        0x11 => {
            // "M": a bare month alone. Only an *abbreviated* match
            // (`'A'`) needs `weekday_flag` (always declines here); a full
            // month name (`'F'`) succeeds unconditionally -- the engine's
            // own gate checks `role0=='A'` specifically, not the shape.
            entries[0].role != b'A'
        }
        0x12 => {
            // "W": a bare weekday alone; mirrors 0x11 with `role0=='w'`
            // (the lowercase, abbreviation-capable table) vs `'W'` (the
            // alphabetically-sorted full-name-only table, which always
            // succeeds).
            entries[0].role != b'w'
        }
        // 0x13 "N": a bare number alone always needs weekday_flag --
        // always declines in this port.
        _ => false,
    }
}

/// `FUN_1002bf10`'s "joined" fast path (`LAB_1002c2e6`), used for patterns
/// 6/9/0xe once every entry is D/M/A/F/Y (always true here -- see module
/// doc): reads day/month/year into three word buffers and emits them in
/// entry order with an inserted literal "de" after the day and month parts,
/// regardless of what separators the original text used.
fn emit_joined(wa: &mut WaSpa, entries: &[DateEntry]) -> i32 {
    let mut day_w = Vec::new();
    let mut month_w = Vec::new();
    let mut year_w = Vec::new();
    for e in entries {
        match e.role {
            b'D' => day_words(&mut day_w, &e.text),
            b'M' => {
                let v = atoi(&e.text) as usize;
                if v < MONTH_WORD.len() {
                    month_w.extend_from_slice(MONTH_WORD[v]);
                }
            }
            b'A' | b'F' => month_w.extend_from_slice(&e.word),
            b'Y' => year_to_words(&mut year_w, &e.text),
            _ => return -1,
        }
    }
    let (start, end) = (entries[0].start, entries[entries.len() - 1].end);
    if !day_w.is_empty() {
        if !emit_split(wa, &day_w, start, end, b'D', b'D', 3) {
            return 0;
        }
        if !emit(wa, b"de", start, end, b'A', b'D', 3, None) {
            return 0;
        }
    }
    if !month_w.is_empty() {
        if !emit_split(wa, &month_w, start, end, b'D', b'D', 3) {
            return 0;
        }
        if !emit(wa, b"de", start, end, b'A', b'D', 3, None) {
            return 0;
        }
    }
    if !year_w.is_empty() && !emit_split(wa, &year_w, start, end, b'D', b'D', 3) {
        return 0;
    }
    1
}

/// One entry's spoken text and origin for the default/`case_a` per-entry
/// walks' `'D'`/`'M'`(numeric)/`'A'`/`'F'`/`'Y'` dispatch: origin is `'D'`
/// for a day/numeric-month/year reading, but `'A'` for an alpha-matched
/// month name (`'A'`/`'F'`, which reach `emit` via `LAB_1002c20e` in the
/// real `FUN_1002bf10`, not the inline numeric-month call) -- the one
/// origin byte this pass's earlier version got wrong (see the probe/capture
/// diff this was fixed against).
fn entry_words(e: &DateEntry) -> Option<(Vec<u8>, u8)> {
    let mut w = Vec::new();
    let origin = match e.role {
        b'D' => {
            day_words(&mut w, &e.text);
            b'D'
        }
        b'M' => {
            let v = atoi(&e.text) as usize;
            if v < MONTH_WORD.len() {
                w.extend_from_slice(MONTH_WORD[v]);
            }
            b'D'
        }
        b'A' | b'F' => {
            w.extend_from_slice(&e.word);
            b'A'
        }
        b'Y' => {
            year_to_words(&mut w, &e.text);
            b'D'
        }
        _ => return None,
    };
    Some((w, origin))
}

/// `switchD_1002bf65_caseD_a` (patterns 10/11, "WNM"/"NM"): like the default
/// walk, but a literal "de" is inserted after every non-final, non-weekday
/// entry (these two shapes have no filler token of their own).
fn emit_case_a(wa: &mut WaSpa, entries: &[DateEntry]) -> i32 {
    let n = entries.len();
    for (i, e) in entries.iter().enumerate() {
        let ok = match e.role {
            b'D' | b'M' | b'A' | b'F' | b'Y' => {
                let Some((w, origin)) = entry_words(e) else { return -1 };
                emit_split(wa, &w, e.start, e.end, origin, b'D', 3)
            }
            b'w' | b'W' => emit_split(wa, &e.word, e.start, e.end, b'A', b'D', 3),
            _ => return -1,
        };
        if !ok {
            return 0;
        }
        if i != n - 1 && !matches!(e.role, b'w' | b'W') && !emit(wa, b"de", e.start, e.end, b'A', b'D', 3, None) {
            return 0;
        }
    }
    1
}

/// `switchD_1002bf65_caseD_7`: the general per-entry walk for every pattern
/// not covered by [`emit_joined`]/[`emit_case_a`]. Each entry is emitted
/// using its own literal separator/filler text (`'I'`/`'O'`/`'T'`) or
/// resolved word (`'D'`/`'M'`/`'A'`/`'F'`/`'Y'`/`'w'`/`'W'`/`'S'`).
fn emit_default(wa: &mut WaSpa, entries: &[DateEntry]) -> i32 {
    for e in entries {
        let ok = match e.role {
            b'D' | b'M' | b'A' | b'F' | b'Y' => {
                let Some((w, origin)) = entry_words(e) else { return -1 };
                emit_split(wa, &w, e.start, e.end, origin, b'D', 3)
            }
            b'w' | b'W' | b'S' => emit_split(wa, &e.word, e.start, e.end, b'A', b'D', 3),
            b'I' | b'O' | b'T' => emit_split(wa, &e.text, e.start, e.end, b'A', b'D', 3),
            _ => return -1,
        };
        if !ok {
            return 0;
        }
    }
    1
}

/// `FUN_1002bf10(wa, entries, pattern_idx)`.
fn emit_date(wa: &mut WaSpa, entries: &[DateEntry], pattern_idx: usize) -> i32 {
    match pattern_idx {
        6 | 9 | 14 => emit_joined(wa, entries),
        10 | 11 => emit_case_a(wa, entries),
        _ => emit_default(wa, entries),
    }
}

/// The pattern-match shape letter for an entry's role: `local_228` (the
/// shape string) shows `'M'` for *both* an abbreviated (`'A'`) and a full
/// (`'F'`) month-name match, and `'W'` for *both* the lowercase (`'w'`) and
/// uppercase (`'W'`) weekday-table match -- only the *entry's own* role
/// (`local_210`, kept verbatim in `DateEntry::role`) distinguishes them, for
/// the period-absorb check and (for A/F) which word source `emit` uses.
fn shape_char(role: u8) -> u8 {
    match role {
        b'A' | b'F' => b'M',
        b'w' => b'W',
        r => r,
    }
}

/// `FUN_1002b410(wa, tok, t + p, off)`.
pub fn date(_fe: &FeSpa, wa: &mut WaSpa, _tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let entries = classify(t, p, off);
    let mut count = entries.len();
    while count >= 1 {
        let Some((lo, hi)) = pattern_range(count) else {
            count -= 1;
            continue;
        };
        let shape: Vec<u8> = entries[..count].iter().map(|e| shape_char(e.role)).collect();
        for pattern_idx in lo..hi {
            if PATTERNS[pattern_idx] != shape.as_slice() {
                continue;
            }
            let mut resolved = entries[..count].to_vec();
            if !resolve(&mut resolved, pattern_idx, false) {
                break;
            }
            let r = emit_date(wa, &resolved, pattern_idx);
            if r < 1 {
                return r;
            }
            return resolved[count - 1].end - off;
        }
        count -= 1;
    }
    -1
}
