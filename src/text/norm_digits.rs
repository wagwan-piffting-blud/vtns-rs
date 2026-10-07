//! `FUN_10052e80` (dispatch order 9, `digits_a`) and `FUN_10053440` (order
//! 10, `digits_b`): a run of number tokens, each optionally glued to the
//! next by a single bare `-`, matched against a small table of known
//! "digit-count[-digit-count]*" shapes (e.g. `"4-4-4-4"` for a 16-digit
//! card-style number split into four groups of four, `"3-2-4"` for an
//! SSN-style `123-45-6789`). On a match, every group is read digit by digit
//! (not as a cardinal/year — `numwords::digits` with `plain=1`, which is the
//! part that makes porting this worth it: left unported, whatever
//! normalizer picks up a bare 4-digit group like `"1234"` next in the chain
//! misreads it as a year, `"twelve thirty-four"`, instead of `"one two
//! three four"` — confirmed live).
//! `digits_a`'s winning class is 0x1e, `digits_b`'s is 0xc (both confirmed
//! live — they differ from each other).
//!
//! Slot layout (scratch at `WA+0x39e4`, stride 0x140, `+0x1a`+`i*0x140` is
//! slot `i`'s tag): `+0x08` ws, `+0x0c` start, `+0x10` end, `+0x14` len,
//! `+0x18` class, `+0x1a` tag (`N` group / `-` glued dash), `+0x1c` (short)
//! a dash immediately follows this group with no space, `+0x24` the raw
//! text pointer into the sentence, `+0x28` the group's own copied text.
//! Slots and dashes are collected in one array (dashes get their own slot,
//! tag `-`); a group only continues the run when it's the first token, or
//! it follows a dash slot glued (`ws==0`), or it follows a group slot with
//! at most one space (`ws<=1`); a bare `-` (any class) continues the run
//! only when glued (`ws==0`) right after a group slot, and marks that
//! group's `+0x1c` flag.
//!
//! `digits_a` (`FUN_10052e80`) always re-tokenizes in its own mode `0x1d`
//! from the very first token (the shared token the driver passes is mode 1
//! and unused here — `param_2` never appears in the decompile). `digits_b`
//! (`FUN_10053440`) reuses the shared token for its first slot (`local_14 <
//! 1` copies the caller's token struct directly) and only re-tokenizes in
//! its own mode `0xa` from the second slot on.
//!
//! The pattern is matched by the count of collected slots (dashes count):
//! the first slot's length digit selects a leading char (`4`/`5` for
//! `digits_a`, `2`/`3`/`4`/`6` for `digits_b`), then a switch on that count
//! picks one table index (or a short range for the one case per leader with
//! two same-length shapes); a count with no valid shape at all is a hard
//! decline (no retry at a shorter count), one that just isn't a recognized
//! length for that leader retries the next-shorter count. The selected
//! table entry is compared against the built pattern string (one decimal
//! digit per group's length, `-` per dash) as a final check, since e.g.
//! `"4-4-4-4"` only matches a run of four length-4 groups, not a run
//! containing a length-5 group even though the token/dash count matches.
//!
//! `digits_a` only: `FUN_10053280` rejects a match outright (no retry at a
//! shorter count) when every one of its *own* group slots (skipping dash
//! slots — every other slot, since they strictly alternate) is pure
//! alphabetic (`FUN_1000ffa0`); how many groups it checks depends on the
//! matched table index (4 for index 0, 5 for 1 and 3, 6 for 2 — the only
//! indices `PATTERNS_A` has). A run like `"ABCD-EFGH-IJKL-MNOP"` (all 4
//! groups alphabetic) reads as plain text in the engine (class 0x18), not
//! digits_a — confirmed live.
//!
//! Emission (`FUN_100531c0` / `FUN_10053830`): every record uses the whole
//! match's fixed `(start, end)` span. A dash slot emits `DAT_100779a8`
//! ("dash"). A group slot in `digits_b` is always pure digits, read
//! directly via `numwords::digits`. A group slot in `digits_a` goes through
//! `FUN_10045680`: repeatedly, at the group's own current position (an
//! isolated copy of just the group's text — the userdict search must not
//! run past the group's own end), the user dictionary (`FUN_1005deb0`,
//! `norm::userdict`) is tried first; on a hit, advance by its own return
//! length. On a miss, the token `fe_Tokenize` already read at that position
//! must be all-alphabetic (spelled, `0x41`/`0x53`) or all-digit (`digits()`,
//! `0x44`/`0x44`) — anything else (a token that is neither, e.g. one mixing
//! letters, digits and no clean split) fails the *entire* call. After a
//! successful group emission, if the group's `+0x1c` flag is unset (no
//! dash follows), the last-emitted record's `+0x2c` field is set to 8.

use super::cstr::*;
use super::norm::userdict;
use super::numwords::{digits, new_buf};
use super::sbd::{all_alpha, all_digit};
use super::tabs::{ptab, Ptab};
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::Fe;

const PATTERNS_A: Ptab = ptab(0x10081404, 0x10081414);
const PATTERNS_B: Ptab = ptab(0x10081444, 0x10081464);

fn tk(fe: &Fe, t: &[u8], p: usize, off: i32, mode: i32) -> (Tok, i32) {
    let mut x = Tok::default();
    let r = tokenize(&mut x, t, p, off, mode, fe.ud.as_ref());
    (x, r)
}

/// `FUN_100533f0` plus `digits_a`'s own outer length test: 4 or 5 letters/digits.
fn group_a(x: &Tok) -> bool {
    (4..=5).contains(&x.len) && x.text().iter().all(|&c| cls(c) & 0xd0 != 0)
}

/// `digits_b`: all digits (`FUN_10061940`), 2, 3, 4, 6 or 7 of them.
fn group_b(x: &Tok) -> bool {
    matches!(x.len, 2 | 3 | 4 | 6 | 7) && all_digit(&x.text)
}

/// One collected slot: a group (`is_group`, tag `N`) or a bare glued dash
/// (tag `-`).
#[derive(Clone)]
struct Slot {
    start: i32,
    end: i32,
    text: Vec<u8>,
    is_group: bool,
    /// `+0x1c`: a dash immediately (no space) follows this group slot.
    dash_follows: bool,
}

fn build_pattern(slots: &[Slot]) -> Vec<u8> {
    let mut out = Vec::new();
    for s in slots {
        if s.is_group {
            out.extend(s.text.len().to_string().into_bytes());
        } else {
            out.push(b'-');
        }
    }
    out
}

/// Collect up to `max` slots in `mode`. `seed`, when `Some`, supplies the
/// first slot's already-tokenized `(Tok, consumed)` pair instead of
/// tokenizing (`digits_b` reuses the shared token; `digits_a`'s own mode
/// `0x1d` differs from the shared mode-1 token, so it always tokenizes
/// fresh, `seed = None`).
fn collect(fe: &Fe, t: &[u8], p: usize, off: i32, mode: i32, group: fn(&Tok) -> bool, max: usize, mut seed: Option<(Tok, i32)>) -> Vec<Slot> {
    let mut slots: Vec<Slot> = Vec::new();
    let (mut q, mut qo) = (p, off);
    loop {
        if slots.len() >= max {
            break;
        }
        let (x, r) = match seed.take() {
            Some(s) => s,
            None => tk(fe, t, q, qo, mode),
        };
        if r == 0 || x.class == 9 || (x.nl == 0 && x.ws > 2) {
            break;
        }
        let prev_is_group = slots.last().map(|s| s.is_group);
        let dash_continuation = prev_is_group == Some(true) && x.len == 1 && x.text[0] == b'-' && x.ws == 0;
        if dash_continuation {
            slots.last_mut().unwrap().dash_follows = true;
            slots.push(Slot { start: x.start, end: x.end, text: x.text().to_vec(), is_group: false, dash_follows: false });
        } else {
            let ok_spacing = match prev_is_group {
                None => true,
                Some(false) => x.ws == 0,
                Some(true) => x.ws <= 1,
            };
            if !group(&x) || !ok_spacing {
                break;
            }
            slots.push(Slot { start: x.start, end: x.end, text: x.text[..x.len as usize].to_vec(), is_group: true, dash_follows: false });
        }
        q += r as usize;
        qo += r;
    }
    slots
}

/// `FUN_10045680`: the group's text, word by word (mode-0 tokenize on an
/// isolated copy of just the group's own bytes — reads past its end come
/// back as 0, so the user-dictionary search can't run past the group), the
/// user dictionary tried first at each position.
fn emit_group(fe: &Fe, wa: &mut Wa, gtext: &[u8], start: i32, end: i32, class: u32) -> bool {
    let glen = gtext.len() as i32;
    let mut used = 0i32;
    while used < glen {
        let (x, r) = tk(fe, gtext, used as usize, 0, 0);
        let ru = userdict(fe, wa, gtext, used as usize, 0, end as i16);
        if ru >= 0 {
            used += ru;
            continue;
        }
        if r == 0 {
            return false;
        }
        let ok = if all_alpha(x.text()) {
            wa.emit(x.text(), start, end, 0x41, 0x53, class)
        } else if all_digit(&x.text) {
            let mut w = new_buf();
            digits(&mut w, &x.text, 1, 0);
            wa.emit_split(&w, start, end, 0x44, 0x44, class)
        } else {
            return false;
        };
        if !ok {
            return false;
        }
        used += r;
    }
    true
}

/// `FUN_100531c0`.
fn emit_a(fe: &Fe, wa: &mut Wa, slots: &[Slot]) -> bool {
    let (start, end) = (slots[0].start, slots[slots.len() - 1].end);
    for slot in slots {
        if slot.is_group {
            if !emit_group(fe, wa, &slot.text, start, end, 0x1e) {
                return false;
            }
            if !slot.dash_follows {
                wa.set_ri32(wa.n() as i32 - 1, 0x2c, 8);
            }
        } else if !wa.emit_split(s(0x100779a8), start, end, 0x44, 0x44, 0x1e) {
            return false;
        }
    }
    true
}

/// `FUN_10053830`.
fn emit_b(wa: &mut Wa, slots: &[Slot]) -> bool {
    let (start, end) = (slots[0].start, slots[slots.len() - 1].end);
    for slot in slots {
        if slot.is_group {
            let mut w = new_buf();
            digits(&mut w, &slot.text, 1, 0);
            if !wa.emit_split(&w, start, end, 0x44, 0x44, 0xc) {
                return false;
            }
            if !slot.dash_follows {
                wa.set_ri32(wa.n() as i32 - 1, 0x2c, 8);
            }
        } else if !wa.emit_split(s(0x100779a8), start, end, 0x44, 0x44, 0xc) {
            return false;
        }
    }
    true
}

/// `FUN_10053280`, `digits_a` only: reject when every one of the matched
/// index's own group slots is pure alphabetic. Indices beyond the 4 in
/// `PATTERNS_A` don't exist, so there is no "always pass" default case to
/// reach in practice, but it's kept for fidelity to the switch's `default`.
fn alpha_reject(idx: i32, groups: &[&Slot]) -> bool {
    let n = match idx {
        0 => 4,
        1 | 3 => 5,
        2 => 6,
        _ => return false,
    };
    groups.len() >= n && groups[..n].iter().all(|g| all_alpha(&g.text))
}

pub fn digits_a(fe: &Fe, wa: &mut Wa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let _ = tok;
    // julie09 FUN_1004f140 stops after 10 slots (2014: 11).
    let slots = collect(fe, t, p, off, 0x1d, group_a, if fe.era.is_2009() { 10 } else { 11 }, None);
    if slots.is_empty() {
        return -1;
    }
    let pattern = build_pattern(&slots);
    let lead = pattern[0];
    if lead != b'4' && lead != b'5' {
        return -1;
    }
    let mut nn = slots.len();
    while nn >= 1 {
        let range: Option<(i32, i32)> = match (lead, nn) {
            (b'4', 7) => Some((0, 1)),
            (b'4', 9) => Some((1, 2)),
            (b'4', 11) => Some((2, 3)),
            (b'4', 8) | (b'4', 10) => None,
            (b'4', _) if nn < 8 => return -1,
            (b'5', 9) => Some((3, 4)),
            (b'5', _) if nn < 10 => return -1,
            _ => None,
        };
        if let Some((lo, hi)) = range {
            for idx in lo..hi {
                if PATTERNS_A.get(idx) != &pattern[..nn] {
                    continue;
                }
                let consumed = slots[nn - 1].end - off;
                let groups: Vec<&Slot> = slots[..nn].iter().filter(|s| s.is_group).collect();
                if alpha_reject(idx, &groups) {
                    return -1;
                }
                if emit_a(fe, wa, &slots[..nn]) {
                    return consumed;
                }
                // julie09 (FUN_1004f140) declines a run whose group is neither spelled nor digits ("401K-2345-...")
                // where 2014 ends the sentence; observed in b2_item16_digits, the decompile shows no difference.
                return if era() == crate::era::Era::Julie09 { -1 } else { 0 };
            }
        }
        nn -= 1;
    }
    -1
}

pub fn digits_b(fe: &Fe, wa: &mut Wa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let slots = collect(fe, t, p, off, 0xa, group_b, 7, Some((tok.clone(), tok.consumed)));
    if slots.is_empty() {
        return -1;
    }
    let pattern = build_pattern(&slots);
    let lead = pattern[0];
    if !matches!(lead, b'2' | b'3' | b'4' | b'6') {
        return -1;
    }
    let mut nn = slots.len();
    while nn >= 1 {
        let range: Option<(i32, i32)> = match (lead, nn) {
            (b'2', 2) => Some((0, 1)),
            (b'2', 3) => Some((1, 2)),
            (b'2', _) if nn < 3 => return -1,
            (b'3', 3) => Some((2, 3)),
            (b'3', 5) => Some((3, 4)),
            (b'3', _) if nn < 4 => return -1,
            (b'4', 4) => Some((4, 5)),
            (b'4', 7) => Some((5, 6)),
            (b'4', _) if nn < 5 => return -1,
            (b'6', 2) => Some((6, 7)),
            (b'6', 3) => Some((7, PATTERNS_B.count())),
            (b'6', _) if nn < 3 => return -1,
            _ => None,
        };
        if let Some((lo, hi)) = range {
            for idx in lo..hi {
                if PATTERNS_B.get(idx) != &pattern[..nn] {
                    continue;
                }
                let consumed = slots[nn - 1].end - off;
                return if emit_b(wa, &slots[..nn]) { consumed } else { 0 };
            }
        }
        nn -= 1;
    }
    -1
}
