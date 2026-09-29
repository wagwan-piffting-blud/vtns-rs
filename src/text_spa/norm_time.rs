//! `norm_time` `FUN_10045e70` (chain slot 3 / CHAIN index 2), with
//! `FUN_10046dc0` (hour/minute/second digit validator), `FUN_10046a60`
//! (role resolve -- folded directly into [`try_pattern`] since the H/M/S
//! entry positions are fixed per pattern, 0/2/4, so there is nothing to
//! "resolve") and `FUN_10046580` (emit).
//!
//! ## Entries and pattern table
//! Same per-entry array convention as `norm_symbols2.rs`/`norm_date.rs`
//! (`entry_base(i) = piVar3 + 2 + i*0x50` ints, `+0x20` text, `+0x3e`
//! secondary/output text, `+0x12` role byte). Tokens (mode 4) are classified
//! into a role-letter shape string exactly like the zone-header function:
//! `:` a lone `:` (`local_190==1` is **len==1**, not class -- confirmed by
//! the `Tok`-blit offsets, same correction as `norm_date.rs`'s note on
//! `local_50`/`local_48`); a `.` either substitutes for the first `:`
//! (`"3.15"` -> `"3:15"`, only right after an `H`/bare-`N` entry) or is
//! silently skipped once (right after an `m`/`D` entry, e.g. the trailing
//! dot of `"a.m."`/`"P.S.T."`); a `,` right after an `m` entry is silently
//! skipped once the same way; otherwise a token matching the hour/minute
//! unit-word table (`PTR_DAT_1006aabc`, 10 entries: h/hora/horas/hr/hrs/hs/
//! m/min/minuto/minutos) is role `U`; else the am/pm table (`0x1006a7d0`, 4
//! entries) is role `m`; else the timezone table (`0x1006a870`, 15 entries)
//! is role `D`; else an all-digit token whose value passes [`valid_hms`] is
//! role `N` (hour-range for the first entry, the wider 0-60 range
//! otherwise); anything else stops capture (up to 7 entries).
//!
//! All three tables are **not fuzzy matches** despite the decompile's
//! `bulk_fuzzy_match` annotations -- `FUN_100437a0`/`FUN_10043680` are a
//! plain case-insensitive (`FUN_100070b0`, confirmed byte-identical in
//! algorithm to `stricmp_spa`: same `TOLOWER`-style fold-and-subtract) binary
//! search over a *presorted* table, nothing more; read directly off their
//! decompiles plus the disassembly-adjacent data reads that recovered the
//! table contents (`0x1006aabc`, `0x1006a7d0`, `0x1006a870`).
//!
//! The classified shape string is matched, longest prefix first, against 18
//! known shapes keyed by (colon count, entry count) at `PTR_DAT_1007a708`
//! (fully read off the binary): see [`PATTERNS`], now all 18.
//!
//! ## The two gated shapes ("N:N" bare H:MM and "N" bare H alone)
//! `FUN_10046a60` gates both behind a flag pair (`local_1d8`/`local_1d4`)
//! computed by `FUN_10046b70`, called twice by the driver: once up front in
//! "L" (lookback) mode (`local_1d8`, unconditional), once after the token
//! loop in "R" (forward) mode (`local_1d4`, only when `local_1d8 != 2`).
//! `FUN_10046b70`'s "L" half is ported (`lookback_las_flag`): the word
//! immediately before the time expression is compared (case-insensitively)
//! against "las" (`DAT_100693f0`) -- `0` no match, `1` a match, and `2` when
//! the word before *that* is additionally "-" or "son" (`DAT_10064530`/
//! `DAT_100691c4`). The "R" half is now also ported (`forward_flag`,
//! [`template_match`]): does "de la tarde"/"de la mañana"/"en la
//! noche"/an am-pm marker/an hour-minute-second unit word/"tiempo"
//! *follow* the expression, against `PTR_DAT_1006aaec`'s 22-entry template
//! table (`DAT_10079de8`, read off the DLL, see [`FORWARD_TEMPLATES`]) --
//! so e.g. "3:15 de la tarde" now resolves with no preceding "las"/"a las"
//! needed. Confirmed against `FUN_10046a60`'s own decompile: shape "N:N"
//! needs `lookback != 0 || forward != 0`; shape "N" needs
//! `forward != 0 || lookback NOT IN {0,1}` (i.e. `lookback == 2` on its
//! own, or `forward` regardless of `lookback`).
//!
//! The `,`/`.` continuation skips (`local_1e8`/`local_1e4` in the engine,
//! `comma_used`/`period_used` here) are each one-shot, matching the engine's
//! own sentinel-then-set behavior.

use super::emit::emit_split;
use super::numwords::{cardinal_to_words, G_F, G_M};
use super::tokenize::{tokenize, Tok};
use super::{emit, FeSpa};
use crate::ling_spa::lexicon::{stricmp_spa, strnicmp_spa};
use crate::ling_spa::WaSpa;

/// `PTR_DAT_1006aabc` (count `DAT_10079de0` = 10): hour/minute unit-word
/// abbreviations, sorted case-insensitively for the engine's binary search.
const HM_UNITS: [&[u8]; 10] = [b"h", b"hora", b"horas", b"hr", b"hrs", b"hs", b"m", b"min", b"minuto", b"minutos"];

/// `0x1006a7d0` (count `DAT_10079dd8` = 4): match text -> spoken/tagged text,
/// stride 0x27, output at +0xb (`DAT_1006a7db`).
const AMPM: [(&[u8], &[u8]); 4] = [(b"a.m", b"AM"), (b"am", b"AM"), (b"p.m", b"PM"), (b"pm", b"PM")];

/// `0x1006a870` (count `DAT_10079ddc` = 15): timezone abbreviations, output
/// at +0xb (`DAT_1006a87b`). Entry 8 (`"P.S.T"` -> `"CST"`) is a genuine
/// engine data bug (every other P.S.T./PST spelling maps to "PST") --
/// ported byte-for-byte, not "fixed". Re-verified this pass with a
/// `read_memory`/pefile dump of the live 15 entries: the previous pass's
/// keys for entries 0-7 each had a spurious trailing `.` (e.g. `"C.S.T."`)
/// that isn't in the DLL at all (`"C.S.T"`, no trailing dot) -- fixed
/// here. That extra dot mattered in practice: mode-4 tokenize only joins a
/// trailing `.` into an alpha token when the *next* character is also
/// alpha, so an end-of-sentence "P.S.T." tokenizes as the word `"P.S.T"`
/// plus a separate `.` token; with the bogus `"P.S.T."` key this word
/// could never match entry 8 at all, always falling through to the
/// all-lowercase `"p.s.t"` entry ("PST") instead of the engine's own
/// "CST" quirk.
const TZ: [(&[u8], &[u8]); 15] = [
    (b"C.S.T", b"CST"),
    (b"CST", b"CST"),
    (b"E.S.T", b"EST"),
    (b"EST", b"EST"),
    (b"G.M.T", b"GMT"),
    (b"GMT", b"GMT"),
    (b"K.S.T", b"KST"),
    (b"KST", b"KST"),
    (b"P.S.T", b"CST"),
    (b"PST", b"PST"),
    (b"c.s.t", b"CST"),
    (b"e.s.t", b"EST"),
    (b"g.m.t", b"GMT"),
    (b"k.s.t", b"KST"),
    (b"p.s.t", b"PST"),
];

/// `FUN_10006fe0`: plain (non-folding) case-sensitive strcmp, used by
/// `FUN_100437a0` for a `'S'`-mode table (confirmed against `FUN_10045e70`'s
/// own call for the timezone table, `0x53`).
fn strcmp_sensitive(a: &[u8], b: &[u8]) -> i32 {
    let mut i = 0usize;
    loop {
        let ca = *a.get(i).unwrap_or(&0);
        let cb = *b.get(i).unwrap_or(&0);
        if ca != cb {
            return ca as i32 - cb as i32;
        }
        if ca == 0 {
            return 0;
        }
        i += 1;
    }
}

/// `FUN_100437a0`: binary search, `insensitive` selecting `FUN_100070b0`/
/// `stricmp_spa` (the am-pm table's own mode, `0x49`) vs `FUN_10006fe0`/
/// [`strcmp_sensitive`] (the timezone table's mode, `0x53` -- confirmed
/// against `FUN_10045e70`'s own two calls, `DAT_10079dd8`/`DAT_10079ddc`;
/// the previous pass had both tables case-insensitive, which happened to
/// still find every *other* timezone spelling since none of them collide
/// case-insensitively except "P.S.T"/"p.s.t", but silently skipped over
/// the engine's own "P.S.T" -> "CST" data-bug entry in favor of the
/// later, wrong, all-lowercase "p.s.t" -> "PST" entry).
fn lookup<'a, T>(table: &'a [(&[u8], T)], text: &[u8], insensitive: bool) -> Option<&'a T> {
    let mut lo = 0i32;
    let mut hi = table.len() as i32 - 1;
    while lo <= hi {
        let mid = (lo + hi) / 2;
        let cmp = if insensitive { stricmp_spa(table[mid as usize].0, text) } else { strcmp_sensitive(table[mid as usize].0, text) };
        if cmp == 0 {
            return Some(&table[mid as usize].1);
        } else if cmp < 0 {
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    None
}

fn lookup_plain(table: &[&[u8]], text: &[u8]) -> bool {
    let mut lo = 0i32;
    let mut hi = table.len() as i32 - 1;
    while lo <= hi {
        let mid = (lo + hi) / 2;
        let cmp = stricmp_spa(table[mid as usize], text);
        if cmp == 0 {
            return true;
        } else if cmp < 0 {
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    false
}

fn is_digit(c: u8) -> bool {
    c.is_ascii_digit()
}

fn all_digits(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&c| is_digit(c))
}

fn atoi(s: &[u8]) -> i32 {
    let mut v: i32 = 0;
    for &c in s {
        if !is_digit(c) {
            break;
        }
        v = v.saturating_mul(10).saturating_add((c - b'0') as i32);
    }
    v
}

/// `FUN_10046dc0(text, hour_mode, wide)`: 1 digit -> any value 0-9; 2 digits
/// -> 0-24 when `hour_mode`, else 0-60 (or 0-99 when `wide`).
fn valid_hms(text: &[u8], hour_mode: bool, wide: bool) -> bool {
    if !all_digits(text) {
        return false;
    }
    match text.len() {
        1 => true,
        2 => {
            let v = atoi(text);
            if wide {
                v < 100
            } else if hour_mode {
                v < 0x19
            } else {
                v < 0x3d
            }
        }
        _ => false,
    }
}

fn fold_lower(c: u8) -> u8 {
    if c.is_ascii_uppercase() {
        c + 32
    } else {
        c
    }
}

/// `FUN_1004bd70`(1 word) + `FUN_1004bf70`: does the word immediately before
/// `t[cur]` (skipping trailing whitespace, sentence starts at `start`) begin
/// with `:`? Used only by the bare-hour lookback check.
fn prev_word_starts_with_colon(t: &[u8], start: usize, cur: usize) -> bool {
    if cur <= start {
        return false;
    }
    let mut p = cur;
    loop {
        if p <= start {
            return false;
        }
        p -= 1;
        if !matches!(t[p], b' ' | 9 | 10 | 13) {
            break;
        }
    }
    let end = p + 1;
    while p > start && !matches!(t[p - 1], b' ' | 9 | 10 | 13) {
        p -= 1;
    }
    end > p && t[p] == b':'
}

/// The word immediately before `cur` (skipping trailing whitespace,
/// capped at 29 chars like `FUN_1004bd70`'s own per-word limit) and, if
/// there's room before `start`, the word before *that*.
fn two_prev_words(t: &[u8], start: usize, cur: usize) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    fn one_word_back(t: &[u8], start: usize, cur: usize) -> Option<(Vec<u8>, usize)> {
        if cur <= start {
            return None;
        }
        let mut p = cur;
        loop {
            if p <= start {
                return None;
            }
            p -= 1;
            if !matches!(t[p], b' ' | 9 | 10 | 13) {
                break;
            }
        }
        let end = p + 1;
        let lim = 29usize;
        let mut count = 1usize;
        while p > start && count < lim && !matches!(t[p - 1], b' ' | 9 | 10 | 13) {
            p -= 1;
            count += 1;
        }
        Some((t[p..end].to_vec(), p))
    }
    let Some((w1, w1_start)) = one_word_back(t, start, cur) else {
        return (None, None);
    };
    let w2 = one_word_back(t, start, w1_start).map(|(w, _)| w);
    (Some(w1), w2)
}

/// `FUN_10046b70(.., 'L', 0, &flag)`: the lookback half. `0` no match; `1`
/// the word right before the time expression is "las"; `2` additionally
/// the word before *that* is "-" or "son" (a plain case-insensitive
/// compare on the extracted word suffices here -- `FUN_1004bfe0`'s own
/// tokenize-classification step doesn't change the outcome for these
/// single-symbol/short-word targets).
fn lookback_las_flag(t: &[u8], start: usize, cur: usize) -> i32 {
    let (w1, w2) = two_prev_words(t, start, cur);
    let Some(w1) = w1 else { return 0 };
    if stricmp_spa(&w1, b"las") != 0 {
        return 0;
    }
    match w2 {
        Some(w2) if stricmp_spa(&w2, b"-") == 0 || stricmp_spa(&w2, b"son") == 0 => 2,
        _ => 1,
    }
}

/// `PTR_DAT_1006aaec` (count `DAT_10079de8` = 22), alphabetically sorted
/// case-insensitively for `FUN_10043bf0`'s binary search: am/pm markers,
/// the 9 "de/en/por la mañana/noche/tarde" phrases, and the hour/minute/
/// second unit words (bare or as a trailing "... horas" etc. after the
/// time), plus the bare word "tiempo".
const FORWARD_TEMPLATES: [&[u8]; 22] = [
    b"a.m", b"am", b"de la ma\xf1ana", b"de la noche", b"de la tarde", b"en la ma\xf1ana", b"en la noche",
    b"en la tarde", b"h", b"hora", b"horas", b"hrs", b"minuto", b"minutos", b"p.m", b"pm", b"por la ma\xf1ana",
    b"por la noche", b"por la tarde", b"segundo", b"segundos", b"tiempo",
];

/// `FUN_10043bf0`, the binary-search branch only (`param_3>=0`, always true
/// at both of this codebase's call sites -- the count is a real, positive
/// table size read off the data section; the linear-scan branch,
/// `param_3<0`, is unreachable and not ported). Tries matching a
/// contiguous run of `words` against `table`'s templates, word by word,
/// starting from every offset into `words` in turn (index 0 first, then
/// 1, ...) until one succeeds or all are exhausted. A prefix match whose
/// continuation fails is treated as "keep searching the lower half"
/// exactly as coded (`goto LAB_10043e9f`), the same quirk as
/// `abbrev::multiword_lookup`. `words` is a fixed 0-3 word window (an
/// empty slice, like the real function's `NULL` sentinel, stops both the
/// outer starting-offset loop and the inner continuation walk).
fn template_match(table: &[&[u8]], words: &[&[u8]]) -> bool {
    for start in 0..words.len() {
        let w0 = words[start];
        if w0.is_empty() {
            return false;
        }
        let n = w0.len();
        let mut lo: i32 = 0;
        let mut hi: i32 = table.len() as i32 - 1;
        while lo <= hi {
            let mid = (lo + hi) / 2;
            let key = table[mid as usize];
            let c0 = strnicmp_spa(key, w0, n);
            if c0 != 0 {
                if c0 >= 0 { hi = mid - 1; } else { lo = mid + 1; }
                continue;
            }
            let next = key.get(n).copied().unwrap_or(0);
            if next == 0 {
                return true;
            }
            if next != b' ' {
                hi = mid - 1;
                continue;
            }
            let mut rest: &[u8] = &key[n + 1..];
            let mut ok = false;
            let mut mismatch: Option<i32> = None;
            for &w in &words[start + 1..] {
                if w.is_empty() {
                    break;
                }
                let wl = w.len();
                let cw = strnicmp_spa(rest, w, wl);
                if cw != 0 {
                    mismatch = Some(cw);
                    break;
                }
                let after = rest.get(wl).copied().unwrap_or(0);
                if after == 0 {
                    ok = true;
                    break;
                }
                if after != b' ' {
                    break;
                }
                rest = &rest[wl + 1..];
            }
            if ok {
                return true;
            }
            match mismatch {
                None => hi = mid - 1,
                Some(cw) if cw >= 0 => hi = mid - 1,
                Some(_) => lo = mid + 1,
            }
        }
    }
    false
}

/// The `param_3 in {'R','B'}` forward-word collection loop shared by
/// `FUN_1002d2b0`/`FUN_10046b70`: up to 3 further tokens (mode `4`), each
/// non-empty and not class 9/10.
fn collect_forward_words(t: &[u8], pos: usize, off: i32) -> Vec<Vec<u8>> {
    let mut words = Vec::new();
    let (mut p, mut o) = (pos, off);
    while words.len() < 3 {
        let (tok, consumed) = tokenize(t, p, o, 4);
        if consumed == 0 || tok.class == 9 || tok.class == 10 {
            break;
        }
        words.push(tok.text().to_vec());
        p += consumed as usize;
        o += consumed;
    }
    words
}

/// `FUN_10046b70(.., 'R', consumed_so_far, &flag)`: the forward half,
/// called once after the token loop (only when the lookback flag isn't
/// already `2`, per [`time`]) -- does "de la tarde"/"en la noche"/"por la
/// mañana"/an am-pm marker/an hour-minute-second unit word/the bare word
/// "tiempo" follow the time expression at `pos`. `true`/`false` (the real
/// function's own `-1 < pcVar2` check on `FUN_10043bf0`'s return).
fn forward_flag(t: &[u8], pos: usize, off: i32) -> bool {
    let words = collect_forward_words(t, pos, off);
    let refs: Vec<&[u8]> = words.iter().map(|w| w.as_slice()).collect();
    template_match(&FORWARD_TEMPLATES, &refs)
}

#[derive(Clone)]
struct Entry {
    start: i32,
    end: i32,
    role: u8,
    text: Vec<u8>,
    /// Only set for `m`/`D` roles: the pre-resolved output text.
    out: Vec<u8>,
}

/// `FUN_10045e70(wa, tok(unused), t + p, off)`.
pub fn time(_fe: &FeSpa, wa: &mut WaSpa, _tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let sentence_start = p - off as usize;
    // `FUN_10046b70((char*)param_3, param_4, 'L', 0, &local_1d8)`, called
    // once up front exactly like the real driver.
    let las_flag = lookback_las_flag(t, sentence_start, p);
    let mut entries: Vec<Entry> = Vec::with_capacity(7);
    let mut colons = 0i32;
    let mut period_used = false;
    let mut comma_used = false;
    let mut cur_p = p;
    let mut cur_off = off;

    loop {
        let (tok, consumed) = tokenize(t, cur_p, cur_off, 4);
        if consumed == 0 || tok.class == 10 {
            break;
        }
        let text = tok.text();
        let prev_role = entries.last().map(|e| e.role);

        if tok.len == 1 && tok.ws == 0 && text == b":" {
            let ok = !entries.is_empty() && colons <= 2 && prev_role == Some(b'N');
            if !ok {
                break;
            }
            colons += 1;
            entries.push(Entry { start: tok.start, end: tok.end, role: b':', text: Vec::new(), out: Vec::new() });
            cur_p += consumed as usize;
            cur_off += consumed;
            if entries.len() >= 7 {
                break;
            }
            continue;
        }

        if tok.len == 1 && tok.ws == 0 && text == b"." {
            if colons == 0 && !period_used && !entries.is_empty() && prev_role == Some(b'N') {
                period_used = true;
                colons = 1;
                entries.push(Entry { start: tok.start, end: tok.end, role: b':', text: Vec::new(), out: Vec::new() });
                cur_p += consumed as usize;
                cur_off += consumed;
                if entries.len() >= 7 {
                    break;
                }
                continue;
            }
            if !period_used && !entries.is_empty() && matches!(prev_role, Some(b'm') | Some(b'D')) {
                period_used = true;
                cur_p += consumed as usize;
                cur_off += consumed;
                continue;
            }
            break;
        }

        if tok.len == 1 && tok.ws <= 2 && text == b"," {
            if !comma_used && !entries.is_empty() && prev_role == Some(b'm') {
                comma_used = true;
                cur_p += consumed as usize;
                cur_off += consumed;
                continue;
            }
            break;
        }

        if tok.ws < 3 {
            if lookup_plain(&HM_UNITS, text) {
                entries.push(Entry { start: tok.start, end: tok.end, role: b'U', text: text.to_vec(), out: Vec::new() });
                cur_p += consumed as usize;
                cur_off += consumed;
                if entries.len() >= 7 {
                    break;
                }
                continue;
            }
        }
        if tok.ws <= 2 {
            if let Some(&out) = lookup(&AMPM, text, true) {
                entries.push(Entry { start: tok.start, end: tok.end, role: b'm', text: text.to_vec(), out: out.to_vec() });
                cur_p += consumed as usize;
                cur_off += consumed;
                if entries.len() >= 7 {
                    break;
                }
                continue;
            }
            if let Some(&out) = lookup(&TZ, text, false) {
                entries.push(Entry { start: tok.start, end: tok.end, role: b'D', text: text.to_vec(), out: out.to_vec() });
                cur_p += consumed as usize;
                cur_off += consumed;
                if entries.len() >= 7 {
                    break;
                }
                continue;
            }
        }

        // Bare number (`FUN_10046dc0`): hour range for entry 0, wider 0-60
        // range otherwise; entry 0 also declines if the word right before
        // this call starts with `:`.
        let is_first = entries.is_empty();
        let hour_ok = if is_first {
            valid_hms(text, true, false) && !prev_word_starts_with_colon(t, sentence_start, cur_p)
        } else {
            valid_hms(text, false, false)
        };
        if !hour_ok {
            break;
        }
        entries.push(Entry { start: tok.start, end: tok.end, role: b'N', text: text.to_vec(), out: Vec::new() });
        cur_p += consumed as usize;
        cur_off += consumed;
        if entries.len() >= 7 {
            break;
        }
    }

    if entries.is_empty() {
        return -1;
    }
    let shape: Vec<u8> = entries.iter().map(|e| e.role).collect();
    // `FUN_10046b70((char*)param_3, cur_off, 'R', cur_p - p, &local_1d4)`,
    // called once after the token loop, only when the lookback flag isn't
    // already `2` (its value can't change either gate's outcome then).
    let forward = if las_flag != 2 { forward_flag(t, cur_p, cur_off) } else { false };

    for len in (1..=entries.len()).rev() {
        if let Some(&pat) = PATTERNS.iter().find(|s| s.len() == len && s.as_bytes() == &shape[..len]) {
            // `FUN_10046a60`'s gate on the two `FUN_10046b70`-dependent
            // shapes (see the module doc): "N:N" needs the lookback/forward
            // flag pair not both zero; bare "N" needs `forward!=0 ||
            // lookback NOT IN {0,1}` (i.e. `lookback==2` on its own, or
            // `forward` regardless of `lookback`).
            let gate_ok = match pat {
                "N:N" => las_flag != 0 || forward,
                "N" => forward || las_flag == 2,
                _ => true,
            };
            if !gate_ok {
                return -1;
            }
            if let Some(r) = try_pattern(wa, &entries[..len], pat, off) {
                return r;
            }
            // A shape match whose length/value checks fail is a hard
            // decline in the engine too (`FUN_10046a60` returning 0 there
            // never falls back to a shorter prefix) -- ported the same way.
            return -1;
        }
    }
    -1
}

/// All 18 `PTR_DAT_1007a708` shapes, including the two gated ones ("N:N"
/// bare H:MM and "N" bare H), now ported -- see `lookback_las_flag` and its
/// gate in [`time`].
const PATTERNS: [&str; 18] = [
    "N:N:NmD", "N:N:ND", "N:N:Nm", "N:N:N", "N:NUmD", "N:NUD", "N:NUm", "N:NmD", "N:NU", "N:ND", "N:Nm", "N:N", "NUmD", "NUD", "NUm", "NU", "Nm", "N",
];

/// `FUN_10046a60` (role validation, folded in since H/M/S positions are
/// fixed per shape) + `FUN_10046580` (emit). `entries` is exactly the
/// matched prefix; returns bytes consumed (`entries.last().end - off`) or
/// `None` on a length/value check failure.
fn try_pattern(wa: &mut WaSpa, entries: &[Entry], shape: &str, off: i32) -> Option<i32> {
    let has_colon2 = shape.starts_with("N:N:N");
    let h = &entries[0];
    let (m, s): (Option<&Entry>, Option<&Entry>) = if has_colon2 {
        (Some(&entries[2]), Some(&entries[4]))
    } else if shape.starts_with("N:N") {
        (Some(&entries[2]), None)
    } else {
        (None, None)
    };
    // Trailing U/m/D roles: whatever isn't consumed by H(:M(:S)) above.
    let tail_start = if has_colon2 { 5 } else if m.is_some() { 3 } else { 1 };
    let (mut u, mut mk, mut dd): (Option<&Entry>, Option<&Entry>, Option<&Entry>) = (None, None, None);
    for e in &entries[tail_start..] {
        match e.role {
            b'U' => u = Some(e),
            b'm' => mk = Some(e),
            b'D' => dd = Some(e),
            _ => {}
        }
    }

    if !(h.text.len() == 1 || h.text.len() == 2) {
        return None;
    }
    if let Some(m) = m {
        if m.text.len() != 2 {
            return None;
        }
    }
    if let Some(s) = s {
        if s.text.len() != 2 {
            return None;
        }
    }

    let span_end = if let Some(s) = s {
        s.end
    } else if let Some(m) = m {
        m.end
    } else {
        h.end
    };
    let span_start = h.start;
    // The bytes consumed overall (the function's own return value) must
    // reach past any trailing U/m/D entry too, not just stop at the H:M:S
    // digits (`span_end`) -- entries are already in text order, so the
    // matched prefix's own last entry is exactly that position. Fixed:
    // the previous pass returned `span_end - off` unconditionally, which
    // under-consumed any pattern with a trailing unit word/meridiem/
    // timezone (e.g. "21 horas", "6:30 p.m."), leaving that trailing text
    // for the chain to reprocess from scratch and duplicate.
    let full_end = entries.last().map(|e| e.end).unwrap_or(span_end);

    let mut words = Vec::new();
    cardinal_to_words(&mut words, &h.text, G_F);
    if !emit_split(wa, &words, span_start, span_end, b'D', b'D', 4) {
        return Some(0);
    }
    if s.is_some() {
        let word: &[u8] = if atoi(&h.text) < 2 { b"hora" } else { b"horas" };
        if !emit(wa, word, span_start, span_end, b'A', b'D', 4, None) {
            return Some(0);
        }
    }
    // `FUN_10046580`: appends "en punto" ("exactly") in place of speaking
    // the minutes, whenever the shape has no unit word/meridiem/timezone
    // at all (`bVar2==false`, set for exactly the three fully-bare shapes
    // "N:N:N"/"N:N"/"N") AND there is no *nonzero* minute value --
    // `local_11c[1]` (the minute-entry marker this gate reads) is only
    // ever set when `FUN_100505b3(minute_text) > 0`, so a bare "N" (no
    // minute entry at all) and a "N:N"/"N:N:N" whose minutes are "00"
    // both count as "no minute" here, e.g. "4:00" -> "cuatro en punto".
    let bare = u.is_none() && mk.is_none() && dd.is_none();
    let real_minute = m.filter(|m| atoi(&m.text) != 0);
    if bare && real_minute.is_none() {
        if !emit_split(wa, b"en punto", span_start, span_end, b'A', b'D', 4) {
            return Some(0);
        }
    } else if let Some(m) = real_minute {
        let gender = if s.is_some() { G_M } else if u.is_some() { G_F } else { 0 };
        let mut words = Vec::new();
        cardinal_to_words(&mut words, &m.text, gender);
        if !emit_split(wa, &words, span_start, span_end, b'D', b'D', 4) {
            return Some(0);
        }
        if s.is_some() {
            let word: &[u8] = if atoi(&m.text) < 2 { b"minuto" } else { b"minutos" };
            if !emit(wa, word, span_start, span_end, b'A', b'D', 4, None) {
                return Some(0);
            }
        }
    }
    if let Some(s) = s {
        if !emit(wa, b"y", span_start, span_end, b'A', b'D', 4, None) {
            return Some(0);
        }
        let mut words = Vec::new();
        cardinal_to_words(&mut words, &s.text, G_M);
        if !emit_split(wa, &words, span_start, span_end, b'D', b'D', 4) {
            return Some(0);
        }
        let word: &[u8] = if atoi(&s.text) < 2 { b"segundo" } else { b"segundos" };
        if !emit(wa, word, span_start, span_end, b'A', b'D', 4, None) {
            return Some(0);
        }
    }
    if let Some(u) = u {
        // Both branches check the HOUR entry's value (`local_11c[0]`), even
        // when an `M` entry is also present -- confirmed against the
        // decompile twice (`param_2 + local_11c[0]*0x50 + 10` in both the
        // `'h'` and the `else` arm); a genuine engine quirk, ported as-is.
        let hv = atoi(&h.text);
        let word: &[u8] = if fold_lower(u.text[0]) == b'h' {
            if hv < 2 {
                b"hora"
            } else {
                b"horas"
            }
        } else if hv < 2 {
            b"minuto"
        } else {
            b"minutos"
        };
        if !emit(wa, word, u.start, u.end, b'A', b'D', 4, None) {
            return Some(0);
        }
    }
    if let Some(mk) = mk {
        if !emit(wa, &mk.out, mk.start, mk.end, b'A', b'S', 4, None) {
            return Some(0);
        }
    }
    if let Some(dd) = dd {
        if !emit(wa, &dd.out, dd.start, dd.end, b'A', b'S', 4, None) {
            return Some(0);
        }
    }
    Some(full_end - off)
}
