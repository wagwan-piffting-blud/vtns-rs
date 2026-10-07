//! Abbreviation and unit-name lookup: `FUN_100437a0` (simple table lookup),
//! `FUN_10033990` (the unit-name table, [`unit_lookup`], used by units,
//! numexpr's `/` and [`fallback_b`]),
//! `FUN_10033b70` (singular/plural rendering), `FUN_100286a0` (the first
//! gender-fallback cascade, [`fallback_a`]) and `FUN_10033c00` (the second,
//! [`fallback_b`]) -- both hooked into `numwords::lookahead_gender` where
//! its lexicon lookup misses. Tables are in `abbrev_tables.rs`, machine-read
//! by `dump_abbrev_tables.py`/`gen_abbrev_rs.py` (pefile against
//! `vt_spa_violeta16.dll`'s `.data` section), never transcribed by hand.
//!
//! `FUN_100440b0`/`FUN_10044320` (the stride-`0x27`/`0x46` multi-word table
//! matchers `fallback_a`/`fallback_b` need) are ported as
//! [`multiword_lookup`]/`unit_multiword`, confirmed against the
//! disassembly of `FUN_100440b0` (not just its decompile, whose brace
//! nesting around the `GROUP_0X10076AA8`-found branch is misleading -- see
//! [`fallback_a`]'s own doc).
//!
//! `FUN_100338b0`'s three table groups (`DAT_10082060`/`64`/`68`) are null
//! in the static image; `FUN_1002e160` points each at two static
//! sub-tables during engine load (confirmed live: two sub-tables each).
//! [`group_lookup`] ports `FUN_100338b0` with its `FUN_100438f0` ('I') and
//! `FUN_10043b20` ('S') searches over `abbrev_groups.rs`.

use super::abbrev_groups::{Entry, Sub, GROUP_A, GROUP_B, GROUP_C};
use super::abbrev_tables::{
    ANTES_DE_CRISTO, ANTIGUO_TESTAMENTO, CAPITAL_FEDERAL, GROUP_0X10076AA8, HERMANOS,
};
use super::tables::CLS_S;
use super::tables_oth::UNIT_WORDS;
use super::tokenize::tokenize;
use crate::ling_spa::lexicon::{stricmp_spa, strnicmp_spa};

fn strcmp_exact(a: &[u8], b: &[u8]) -> i32 {
    let n = a.len().min(b.len());
    for i in 0..n {
        if a[i] != b[i] {
            return a[i] as i32 - b[i] as i32;
        }
    }
    a.len() as i32 - b.len() as i32
}

/// `FUN_100437a0`: exact-match binary search over a stride-0x27 table
/// (`mode` 'I' = case-insensitive via `stricmp_spa`, else exact).
pub(super) fn simple_lookup(table: &[(&[u8], &[u8], u8)], word: &[u8], insensitive: bool) -> Option<usize> {
    if word.is_empty() {
        return None;
    }
    let mut lo: i32 = 0;
    let mut hi: i32 = table.len() as i32 - 1;
    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        let c = if insensitive { stricmp_spa(table[mid as usize].0, word) } else { strcmp_exact(table[mid as usize].0, word) };
        if c == 0 {
            return Some(mid as usize);
        } else if c < 0 {
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    None
}

/// `FUN_10027e60`'s abbreviation cascade: the expansion text (if any) and
/// its gender flag, for a single already-tokenized word (optionally
/// followed by a `.`, tried both with and without it, matching the
/// decompile's own retry). Multi-word entries (`CAPITAL_FEDERAL`'s
/// "Cap. Fed." -> "Capital Federal") only match when the whole thing
/// tokenizes as one word, same limitation as above.
pub fn abbrev_expand(word: &[u8]) -> (Option<&'static [u8]>, Option<u8>) {
    let try_all = |w: &[u8]| -> Option<(&'static [u8], u8)> {
        if let Some(i) = simple_lookup(&CAPITAL_FEDERAL, w, true) {
            let (_, ex, fl) = CAPITAL_FEDERAL[i];
            return Some((ex, fl));
        }
        if let Some(i) = simple_lookup(&ANTIGUO_TESTAMENTO, w, false) {
            let (_, ex, fl) = ANTIGUO_TESTAMENTO[i];
            return Some((ex, fl));
        }
        if let Some(i) = simple_lookup(&GROUP_0X10076AA8, w, true) {
            let (_, ex, fl) = GROUP_0X10076AA8[i];
            return Some((ex, fl));
        }
        if let Some(i) = simple_lookup(&ANTES_DE_CRISTO, w, false) {
            let (_, ex, fl) = ANTES_DE_CRISTO[i];
            return Some((ex, fl));
        }
        if let Some(i) = simple_lookup(&HERMANOS, w, true) {
            let (_, ex, fl) = HERMANOS[i];
            return Some((ex, fl));
        }
        None
    };
    if let Some((ex, fl)) = try_all(word) {
        return (Some(ex), if fl == b'F' { Some(fl) } else { None });
    }
    if word.last() == Some(&b'.') {
        if let Some((ex, fl)) = try_all(&word[..word.len() - 1]) {
            return (Some(ex), if fl == b'F' { Some(fl) } else { None });
        }
    }
    (None, None)
}

/// `dict_StrnCmpU`: raw (non-folding) byte compare of up to `n` bytes,
/// stopping early at `a`'s own NUL (matching the real function's own
/// `bVar1==*param_1; if(bVar1==0) return 0;` early exit -- it never checks
/// `b`'s NUL directly, only ever comparing byte-for-byte).
fn cmp_sensitive(a: &[u8], b: &[u8], n: usize) -> i32 {
    for i in 0..n {
        let ca = *a.get(i).unwrap_or(&0);
        let cb = *b.get(i).unwrap_or(&0);
        if ca != cb {
            return ca as i32 - cb as i32;
        }
        if ca == 0 {
            return 0;
        }
    }
    0
}

fn cmp_mode(insensitive: bool, a: &[u8], b: &[u8], n: usize) -> i32 {
    if insensitive {
        strnicmp_spa(a, b, n)
    } else {
        cmp_sensitive(a, b, n)
    }
}

/// `FUN_100440b0`: binary search over a stride-`0x27` `(key, expansion,
/// gender)` table for `word`, matching exactly `word.len()` bytes of each
/// candidate key (`insensitive` selects `FUN_10007110`/`strnicmp_spa` vs
/// the raw `dict_StrnCmpU`). An exact-length key match returns its index
/// immediately. A key that *continues* past `word` -- with a space, or (the
/// `word.len()==1 && word[0]==0xb0` special case) a bare degree sign
/// immediately glued to an alphabetic character in the text, no separating
/// space needed -- attempts the engine's own multi-word continuation:
/// re-tokenizing further text (mode `0xc`) at `pos+*cursor` and matching
/// each new token, word by word, against the key's remainder. A prefix
/// match that fails its continuation check is treated as "keep searching
/// the lower half" exactly as coded (`goto LAB_100442a4`), not as a
/// genuine less-than/greater-than outcome. `*cursor` is the position
/// (relative to `pos`) right after `word` on entry, and is advanced past
/// the match's last word on success; `off` is `pos`'s own sentence offset.
pub fn multiword_lookup(
    table: &[(&[u8], &[u8], u8)],
    word: &[u8],
    insensitive: bool,
    t: &[u8],
    pos: usize,
    off: i32,
    cursor: &mut i32,
) -> Option<usize> {
    if word.is_empty() {
        return None;
    }
    let n = word.len();
    let at = |rel: i32| -> u8 {
        let idx = pos as i64 + rel as i64;
        if idx < 0 { 0 } else { *t.get(idx as usize).unwrap_or(&0) }
    };

    let mut lo: i32 = 0;
    let mut hi: i32 = table.len() as i32 - 1;
    while lo <= hi {
        let mid = (lo + hi) / 2;
        let key = table[mid as usize].0;
        let c0 = cmp_mode(insensitive, key, word, n);
        if c0 != 0 {
            if c0 >= 0 { hi = mid - 1; } else { lo = mid + 1; }
            continue;
        }
        let next = key.get(n).copied().unwrap_or(0);
        if next == 0 {
            return Some(mid as usize);
        }
        let is_degree =
            n == 1 && word[0] == 0xb0 && key.get(1) == Some(&b' ') && CLS_S[at(*cursor) as usize] & 0xc0 != 0;
        if !is_degree && (next != b' ' || at(*cursor) != b' ') {
            hi = mid - 1;
            continue;
        }
        let mut rest: &[u8] = &key[n + 1..];
        let mut c = if is_degree { *cursor } else { *cursor + 1 };
        let mismatch: Option<i32> = 'cont: loop {
            if at(c) == 0 {
                break 'cont None;
            }
            let base = (pos as i64 + c as i64) as usize;
            let (tok, consumed) = tokenize(t, base, off + c, 0xc);
            if consumed == 0 || tok.class == 10 || tok.len == 0 {
                break 'cont None;
            }
            let tlen = tok.len as usize;
            let cw = cmp_mode(insensitive, rest, tok.text(), tlen);
            if cw != 0 {
                break 'cont Some(cw);
            }
            let after = rest.get(tlen).copied().unwrap_or(0);
            if after == 0 {
                *cursor = c + tok.consumed;
                return Some(mid as usize);
            }
            if after != b' ' || at(c + tok.consumed) != b' ' {
                break 'cont None;
            }
            c += tok.consumed + 1;
            rest = &rest[tlen + 1..];
        };
        match mismatch {
            None => hi = mid - 1,
            Some(cw) if cw >= 0 => hi = mid - 1,
            Some(_) => lo = mid + 1,
        }
    }
    None
}

/// `FUN_10044320(UNIT_WORDS, word, n, 'I', t+pos, off, &cursor, &plural)`:
/// binary search where a key matching `word` over `word.len()` bytes either
/// ends there or continues with a space into the following text (tokenized
/// mode 0xb from `pos + cursor + 1`), word by word. A continuation token
/// whose stem matches the rest of the key and whose tail is the entry's
/// `-suffix` also matches and sets `plural`. A lone "\xb0" continues without
/// the space when a letter follows it directly. `cursor` (relative to `pos`)
/// is moved past the last matched token.
fn unit_multiword(word: &[u8], t: &[u8], pos: usize, off: i32, cursor: &mut i32, plural: &mut bool) -> Option<usize> {
    if word.is_empty() {
        return None;
    }
    let n = word.len();
    let at = |rel: i32| -> u8 {
        let idx = pos as i64 + rel as i64;
        if idx < 0 { 0 } else { *t.get(idx as usize).unwrap_or(&0) }
    };

    let mut lo: i32 = 0;
    let mut hi: i32 = UNIT_WORDS.len() as i32 - 1;
    while lo <= hi {
        let mid = (lo + hi) / 2;
        let (key, _sg, pl, _g) = UNIT_WORDS[mid as usize];
        let c0 = strnicmp_spa(key, word, n);
        if c0 != 0 {
            if c0 >= 0 { hi = mid - 1; } else { lo = mid + 1; }
            continue;
        }
        let next = key.get(n).copied().unwrap_or(0);
        if next == 0 {
            return Some(mid as usize);
        }
        let degree = n == 1 && word[0] == 0xb0 && key.get(1) == Some(&b' ') && CLS_S[at(*cursor) as usize] & 0xc0 != 0;
        if !degree && (next != b' ' || at(*cursor) != b' ') {
            hi = mid - 1;
            continue;
        }
        let suffix: &[u8] = if pl.first() == Some(&b'-') { &pl[1..] } else { b"" };
        let mut rest: &[u8] = &key[n + 1..];
        let mut c = if degree { *cursor } else { *cursor + 1 };
        let mismatch: Option<i32> = 'cont: loop {
            if at(c) == 0 {
                break 'cont None;
            }
            let base = (pos as i64 + c as i64) as usize;
            let (tok, consumed) = tokenize(t, base, off + c, 0xb);
            if consumed == 0 || tok.class == 10 || tok.len == 0 {
                break 'cont None;
            }
            let tlen = tok.len as usize;
            if !suffix.is_empty() && tlen >= suffix.len() {
                let stem = tlen - suffix.len();
                if strnicmp_spa(rest, tok.text(), stem) == 0 && stricmp_spa(&tok.text()[stem..], suffix) == 0 {
                    *cursor = c + consumed;
                    *plural = true;
                    return Some(mid as usize);
                }
            }
            let cw = strnicmp_spa(rest, tok.text(), tlen);
            if cw != 0 {
                break 'cont Some(cw);
            }
            let after = rest.get(tlen).copied().unwrap_or(0);
            if after == 0 {
                *cursor = c + consumed;
                return Some(mid as usize);
            }
            if after != b' ' || at(c + consumed) != b' ' {
                break 'cont None;
            }
            c += consumed + 1;
            rest = &rest[tlen + 1..];
        };
        match mismatch {
            Some(cw) if cw < 0 => lo = mid + 1,
            _ => hi = mid - 1,
        }
    }
    None
}

/// `FUN_10033990(out, word, t+pos, off, &cursor, &gender)`: the unit-name
/// table (`DAT_10072ec8`). On a miss, `word` minus its last byte is retried
/// (words of 3+ bytes); that hit renders the plural when the entry's
/// `-suffix` starts with the stripped byte, else `word` itself. `out` is
/// cleared on entry either way; `gender` only ever gets `'F'`.
pub fn unit_lookup(
    mut out: Option<&mut Vec<u8>>,
    word: &[u8],
    t: &[u8],
    pos: usize,
    off: i32,
    cursor: &mut i32,
    gender: Option<&mut u8>,
) -> bool {
    if let Some(o) = out.as_deref_mut() {
        o.clear();
    }
    let mut plural = false;
    let mut hit = unit_multiword(word, t, pos, off, cursor, &mut plural);
    if let Some(i) = hit {
        if let Some(o) = out.as_deref_mut() {
            let (key, sg, pl, _) = UNIT_WORDS[i];
            *o = render_form(plural as i16, key, sg, pl);
        }
    } else if word.len() >= 3 {
        let count = word.len() - 1;
        hit = unit_multiword(&word[..count], t, pos, off, cursor, &mut plural);
        if let (Some(i), Some(o)) = (hit, out.as_deref_mut()) {
            let (key, sg, pl, _) = UNIT_WORDS[i];
            *o = if pl.first() == Some(&b'-') && pl.get(1) == Some(&word[count]) {
                render_form(1, key, sg, pl)
            } else {
                word.to_vec()
            };
        }
    }
    let Some(i) = hit else { return false };
    if UNIT_WORDS[i].3 {
        if let Some(g) = gender {
            *g = b'F';
        }
    }
    true
}

/// `DAT_10082060`/`64`/`68` by slot 0/1/2. Null in the static image;
/// `FUN_1002e160` points them at static tables during load
/// (`abbrev_groups.rs`).
pub fn group(slot: u8) -> &'static [Sub; 2] {
    match slot {
        0 => &GROUP_A,
        1 => &GROUP_B,
        _ => &GROUP_C,
    }
}

/// `FUN_100438f0(table, word, n, 'I')`: binary search where a key matches
/// when it is a case-insensitive prefix of `word` and either ends `word` or
/// (singular empty, plural `-suffix`) the rest of `word` equals that suffix.
pub(super) fn find_i(table: &[Entry], word: &[u8]) -> Option<usize> {
    if word.is_empty() {
        return None;
    }
    let at = |i: usize| word.get(i).copied().unwrap_or(0);
    let (mut lo, mut hi) = (0i32, table.len() as i32 - 1);
    let mut mid_sum = hi;
    while hi >= 0 {
        let mid = mid_sum / 2;
        let (key, sg, pl, _) = table[mid as usize];
        let n = key.len();
        let c = strnicmp_spa(key, word, n);
        if c == 0 {
            if at(n) == 0 {
                return Some(mid as usize);
            }
            if sg.is_empty() && pl.first() == Some(&b'-') && stricmp_spa(&word[n.min(word.len())..], &pl[1..]) == 0 {
                return Some(mid as usize);
            }
            lo = mid + 1;
        } else if c < 0 {
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
        mid_sum = lo + hi;
        if hi < lo {
            return None;
        }
    }
    None
}

/// `FUN_10043b20`: exact, case-sensitive binary search on the key.
fn find_s(table: &[Entry], word: &[u8]) -> Option<usize> {
    if word.is_empty() {
        return None;
    }
    let (mut lo, mut hi) = (0i32, table.len() as i32 - 1);
    let mut mid_sum = hi;
    while hi >= 0 {
        let mid = mid_sum / 2;
        let c = strcmp_exact(table[mid as usize].0, word);
        if c == 0 {
            return Some(mid as usize);
        }
        if c < 0 {
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
        mid_sum = lo + hi;
        if hi < lo {
            return None;
        }
    }
    None
}

/// `FUN_10033b70(out, mode, key, sg, pl)`: the singular (mode 0) or plural
/// form of an entry; a `-suffix` plural is appended (`"%s%s"`) to the
/// singular, or to the key when the singular is empty.
pub fn render_form(mode: i16, key: &[u8], sg: &[u8], pl: &[u8]) -> Vec<u8> {
    let base: &[u8] = if sg.is_empty() { key } else { sg };
    if mode == 0 || pl.is_empty() {
        return base.to_vec();
    }
    if pl[0] == b'-' {
        let mut out = base.to_vec();
        out.extend_from_slice(&pl[1..]);
        return out;
    }
    pl.to_vec()
}

/// `FUN_100338b0(out, group, word, mode, gender)`: the first sub-table
/// (in order) that matches `word`. On a hit the rendered form goes to `out`
/// when given, and `'F'` to `gender` when the entry is feminine.
pub fn group_lookup(slot: u8, out: Option<&mut Vec<u8>>, word: &[u8], mode: i16, gender: Option<&mut u8>) -> bool {
    for &(kind, table) in group(slot).iter() {
        let hit = match kind {
            b'I' => find_i(table, word),
            b'S' => find_s(table, word),
            _ => None,
        };
        let Some(i) = hit else { continue };
        let (key, sg, pl, flag) = table[i];
        if let Some(o) = out {
            *o = render_form(mode, key, sg, pl);
        }
        if let Some(g) = gender {
            if flag == b'F' {
                *g = b'F';
            }
        }
        return true;
    }
    false
}

/// Shared tail for every [`fallback_a`] table hit (`JMP 0x100288ba` in the
/// disassembly): set `*gender='F'` only when the matched entry's own
/// gender flag is `'F'`; either way "found something" (`true`) is
/// returned regardless of the flag's value -- a hit on a non-feminine
/// entry still stops `lookahead_gender` from trying `fallback_b`.
fn gender_hit(entry_gender: u8, gender: &mut u8) -> bool {
    if entry_gender == b'F' {
        *gender = b'F';
    }
    true
}

/// `FUN_100286a0(t+pos, off, &gender)`: the first gender-fallback cascade,
/// tried by `numwords::lookahead_gender` when its lexicon lookup misses.
/// Tokenizes the word at `pos` (mode `0xc`) and peeks one more token (mode
/// `0`, only to gate the multi-word `CAPITAL_FEDERAL` attempt below), then
/// cascades through up to five tables:
/// 1. `CAPITAL_FEDERAL` (`0x10076a80`, insensitive, multi-word via
///    [`multiword_lookup`]) -- only attempted when the peeked token is a
///    genuine standalone following word (non-empty, class 1, `ws>=1`).
/// 2. `ANTIGUO_TESTAMENTO` (`0x10076bc0`, sensitive).
/// 3. `GROUP_0X10076AA8` (`0x10076aa8`, insensitive).
/// 4. `ANTES_DE_CRISTO` (`0x100792a0`, sensitive) -- skipped entirely when
///    the word ends with `.` (its keys never do, so a case-sensitive match
///    could never succeed there anyway).
/// 5. `HERMANOS` (`0x10079200`, insensitive) -- also skipped when the word
///    ends with `.`.
///
/// If the word does NOT end with `.`, exhausting 4-5 declines outright.
/// If it DOES end with `.` and 2-3 both missed, the trailing `.` is
/// stripped and the *full* 4-table cascade (`ANTES_DE_CRISTO` ->
/// `HERMANOS` -> `ANTIGUO_TESTAMENTO` -> `GROUP_0X10076AA8`) is retried
/// fresh against the trimmed word before declining.
///
/// This exact shape -- confirmed against the disassembly at `0x100286a0`,
/// not just the decompile -- corrects a round-3 mistake: the decompile's
/// own brace nesting makes the `GROUP_0X10076AA8`-first-try-found branch
/// look like it reads `DAT_10076aa6` (`CAPITAL_FEDERAL`'s own gender
/// field) instead of its own `DAT_10076ace`; the disassembly shows
/// `DAT_10076aa6` is used exactly once, only for `CAPITAL_FEDERAL`, and
/// `GROUP_0X10076AA8` reads its own field correctly both on the first try
/// and on the post-strip retry.
pub fn fallback_a(t: &[u8], pos: usize, off: i32, gender: &mut u8) -> bool {
    let (tok1, consumed1) = tokenize(t, pos, off, 0xc);
    if tok1.len == 0 {
        return false;
    }
    let (tok2, _) = tokenize(t, pos + consumed1 as usize, off + consumed1, 0);
    if tok2.len != 0 && tok2.class == 1 && tok2.ws == 0 {
        return false;
    }

    let word1 = tok1.text().to_vec();
    let ends_with_dot = word1.last() == Some(&b'.');

    if tok2.len >= 1 && tok2.class == 1 && tok2.ws >= 1 {
        let mut cursor = consumed1;
        if let Some(idx) = multiword_lookup(&CAPITAL_FEDERAL, &word1, true, t, pos, off, &mut cursor) {
            return gender_hit(CAPITAL_FEDERAL[idx].2, gender);
        }
    }
    if let Some(idx) = simple_lookup(&ANTIGUO_TESTAMENTO, &word1, false) {
        return gender_hit(ANTIGUO_TESTAMENTO[idx].2, gender);
    }
    if let Some(idx) = simple_lookup(&GROUP_0X10076AA8, &word1, true) {
        return gender_hit(GROUP_0X10076AA8[idx].2, gender);
    }

    if !ends_with_dot {
        if let Some(idx) = simple_lookup(&ANTES_DE_CRISTO, &word1, false) {
            return gender_hit(ANTES_DE_CRISTO[idx].2, gender);
        }
        if let Some(idx) = simple_lookup(&HERMANOS, &word1, true) {
            return gender_hit(HERMANOS[idx].2, gender);
        }
        return false;
    }

    let trimmed = &word1[..word1.len() - 1];
    if let Some(idx) = simple_lookup(&ANTES_DE_CRISTO, trimmed, false) {
        return gender_hit(ANTES_DE_CRISTO[idx].2, gender);
    }
    if let Some(idx) = simple_lookup(&HERMANOS, trimmed, true) {
        return gender_hit(HERMANOS[idx].2, gender);
    }
    if let Some(idx) = simple_lookup(&ANTIGUO_TESTAMENTO, trimmed, false) {
        return gender_hit(ANTIGUO_TESTAMENTO[idx].2, gender);
    }
    if let Some(idx) = simple_lookup(&GROUP_0X10076AA8, trimmed, true) {
        return gender_hit(GROUP_0X10076AA8[idx].2, gender);
    }
    false
}

/// `FUN_10033c00(t+pos, off, &gender)`: the second gender fallback, tried
/// only when [`fallback_a`] declines outright. Classifies the word at
/// `pos` (mode `0xb`); for a plain word or raw-high-byte token (class 1 or
/// 6), tries the three runtime abbreviation groups (see [`group_lookup`],
/// [`group`]) before falling back to the unit-name table
/// ([`unit_lookup`]).
pub fn fallback_b(t: &[u8], pos: usize, off: i32, gender: &mut u8) -> bool {
    let (tok, _consumed) = tokenize(t, pos, off, 0xb);
    if tok.len == 0 {
        return false;
    }
    if tok.class != 1 && tok.class != 6 {
        return false;
    }
    let word = tok.text().to_vec();
    for group_slot in 0..3u8 {
        if group_lookup(group_slot, None, &word, 0, Some(&mut *gender)) {
            return true;
        }
    }
    // `FUN_10033990`'s continuation cursor is seeded from the Tok struct's
    // `len` slot, not its `consumed` slot -- confirmed via disassembly at
    // 0x10033ca9 (`LEA ECX,[ESP+0x1c]`, the struct's `+0x10`/len offset,
    // not `+0x14`/consumed). A genuine engine quirk: when the word has
    // leading whitespace (`tok.ws>0`) this undershoots the real
    // continuation start by that many bytes. Ported as-is.
    let mut cursor = tok.len;
    unit_lookup(None, &word, t, pos, off, &mut cursor, Some(gender))
}
