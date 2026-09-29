//! `FUN_100426b0` (chain slot 6 / CHAIN index 5): NWS UGC zone-header
//! hyphenated digit groups ("GMZ100-272100-", "FLZ168-271500-" -- the "GMZ100"
//! word prefix is consumed elsewhere; this handles what's left starting at
//! the first digit group).
//!
//! Captures up to 7 tokens (mode 10, `FUN_10047000`; the first token reuses
//! the already-tokenized `tok` argument rather than re-tokenizing): each is
//! either a lone "-" immediately adjacent to the previous token (no
//! whitespace), or an all-digit token (`FUN_1004b600`) whose *length* is one
//! of 2/3/4/6/7 -- not its `class`; the decompile names both fields
//! `local_50`/`local_48` similarly but the struct-copy loop pins `local_50`
//! to `Tok.len` and `local_48` to `Tok.class` (offsets +0x10/+0x18 into the
//! 0x40-byte `Tok` blitted from the caller's token), and the length filter
//! (`local_50 != 2 && != 3 && != 4 && != 6 && != 7`) is confirmed against the
//! disassembly: the JZ cascade at 0x100427be compares the same stack slot the
//! decompile calls `local_50`, and the vararg pushed for the `FUN_1004fd76`
//! sprintf call just below it (`PUSH ECX` at 0x10042808, loaded from that
//! same slot at 0x100427ba) is therefore the token's length, not its class --
//! Ghidra dropped this arg from the decompile entirely (it only shows the
//! format string), so it had to be read off the disassembly. Since only
//! lengths 2/3/4/6/7 ever pass the filter, `"%d"` on that vararg is always
//! exactly one digit, so `FUN_1004fd76` (confirmed byte-identical to
//! vt_eng.dll's own `vsprintf`, 0x10063ed6) is ported as a plain
//! `b'0' + len as u8` rather than a real sprintf.
//!
//! Consecutive entries must alternate correctly: after a "-" the next digit
//! entry needs zero whitespace before it; after a digit entry the next needs
//! zero or one whitespace byte (a real engine leniency, ported as-is). The
//! first entry skips this check (nothing precedes it) and *always* takes the
//! digit-group branch regardless of its own text (`local_84 < 1` short-
//! circuits the branch selector), so a bare leading "-" simply fails the
//! all-digit test and declines immediately.
//!
//! The per-entry "shape byte" the previous agent could not pin down (0x12 vs
//! 0x1a) is the same field measured from two different bases: `FUN_100426b0`
//! writes it at `entry_base + 0x12` (`piVar11 + 0x12`), while `FUN_10042a80`
//! reads it at `array_base + 0x1a` for entry 0 (`param_2 + 0x28 - 0xe`) --
//! `array_base + 8 == entry_base(0)`, so `0x1a == entry_base(0) + 0x12`; both
//! describe the identical byte, just relative to different pointers.
//!
//! Once the loop stops, the captured entries' shapes are matched: a length
//! digit ('2'/'3'/'4'/'6'/'7') per digit entry, a literal '-' per hyphen
//! entry, concatenated and matched against progressively shorter prefixes
//! (longest first) of 8 known shapes (`PTR_DAT_10069d4c` / `DAT_10069d9c`
//! read directly from the DLL): `27`, `2-7` (first digit length 2), `324`,
//! `3-2-4` (first length 3), `4444`, `4-4-4-4` (first length 4), `67`, `6-7`
//! (first length 6) -- first lengths other than 2/3/4/6 never match
//! (`DAT_10069d9c` == "2346"). On a match (`FUN_10042a80`), each digit entry
//! in the matched prefix is read digit-by-digit (`FUN_1004c830`, our
//! `digit_by_digit`) and emitted (origin/case 'D', src 9); "-" entries are
//! silent. After each digit entry's emission, `FUN_10042a80` also stamps the
//! *last* word just emitted with `rec::PUNCT = 6` (`(param_1 + n*0x54 -
//! 0x3e)` resolves to `WaSpa::rec(n - 1) + 0x20`, i.e. `rec::PUNCT`, via two
//! 16-bit stores of 6 and 0 -- one 32-bit write). Bytes consumed on a match
//! are the matched prefix's last entry's `end` minus the caller's `off`.

use super::numwords::{digit_by_digit, emit_phrase};
use super::tokenize::{tokenize, Tok};
use super::{rec, FeSpa};
use crate::ling_spa::WaSpa;

/// `FUN_1004b600`: every character of `s` (non-empty) has class bit 0x10
/// (digit).
fn all_digits(s: &[u8]) -> bool {
    use super::tables::CLS_S;
    !s.is_empty() && s.iter().all(|&c| CLS_S[c as usize] & 0x10 != 0)
}

struct Entry {
    tok: Tok,
    is_n: bool,
}

/// `PTR_DAT_10069d4c` + `DAT_10069d9c`/`DAT_10069d6c`: the 8 known shapes,
/// keyed by (first entry's length digit, total entry count).
fn pattern_for(first: u8, count: usize) -> Option<&'static [u8]> {
    match (first, count) {
        (b'2', 2) => Some(b"27"),
        (b'2', 3) => Some(b"2-7"),
        (b'3', 3) => Some(b"324"),
        (b'3', 5) => Some(b"3-2-4"),
        (b'4', 4) => Some(b"4444"),
        (b'4', 7) => Some(b"4-4-4-4"),
        (b'6', 2) => Some(b"67"),
        (b'6', 3) => Some(b"6-7"),
        _ => None,
    }
}

/// `FUN_100426b0(wa, tok, t + p, off)`.
pub fn symbols2(_fe: &FeSpa, wa: &mut WaSpa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let mut entries: Vec<Entry> = Vec::with_capacity(7);
    let mut cur_p = p;
    let mut cur_off = off;

    loop {
        let (cur, consumed) = if entries.is_empty() {
            (tok.clone(), tok.consumed)
        } else {
            let (tk, r) = tokenize(t, cur_p, cur_off, 10);
            if r == 0 || tk.class == 10 {
                break;
            }
            (tk, r)
        };

        let is_hyphen = !entries.is_empty() && cur.len == 1 && cur.ws == 0 && cur.text() == b"-";
        if is_hyphen {
            entries.push(Entry { tok: cur, is_n: false });
        } else {
            if !all_digits(cur.text()) || !matches!(cur.len, 2 | 3 | 4 | 6 | 7) {
                break;
            }
            if let Some(prev) = entries.last() {
                let p_ok = !prev.is_n && cur.ws == 0;
                let n_ok = prev.is_n && cur.ws <= 1;
                if !p_ok && !n_ok {
                    break;
                }
            }
            entries.push(Entry { tok: cur, is_n: true });
        }

        cur_p += consumed as usize;
        cur_off += consumed;
        if entries.len() >= 7 {
            break;
        }
    }

    if entries.is_empty() {
        return -1;
    }
    let code: Vec<u8> = entries.iter().map(|e| if e.is_n { b'0' + e.tok.len as u8 } else { b'-' }).collect();
    let first = code[0];
    if !matches!(first, b'2' | b'3' | b'4' | b'6') {
        return -1;
    }

    for count in (1..=entries.len()).rev() {
        let Some(pat) = pattern_for(first, count) else { continue };
        if pat != &code[..count] {
            continue;
        }
        for e in &entries[..count] {
            if !e.is_n {
                continue;
            }
            let mut words = Vec::new();
            digit_by_digit(&mut words, e.tok.text());
            if !emit_phrase(wa, &words, e.tok.start, e.tok.end, b'D', b'D', 9) {
                return 0;
            }
            let n = wa.n();
            wa.set_i32(WaSpa::rec(n - 1) + rec::PUNCT, 6);
        }
        return entries[count - 1].tok.end - off;
    }
    -1
}
