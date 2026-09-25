//! `FUN_10058490`: clock times ("3 PM", "8 AM CDT", "10:45 AM", "6:00:30",
//! "3 hr"). Up to six mode-4 tokens are scanned; each is classified into a
//! role (hour, minute, second — chosen by how many `:` have been seen so
//! far — am/pm, zone name, or duration unit), then the resulting tag string
//! (colons plus one letter per role) is matched against the 18 patterns at
//! `0x1009c1e0`. class 5.
//!
//! **Scope**: every pattern except the bare `"N:N"` (`"10:45"` with no
//! am/pm/zone/seconds) is ported. That one pattern needs a phrase-context
//! gate (`FUN_100590f0`) this port doesn't run, and empirically — checked
//! against `synthetic.txt`'s standalone `"06:00"` and `"1:05 AKDT"` (whose
//! zone doesn't match `ZONE_TAB` either, leaving a bare `"N:N"`) — the real
//! engine declines a bare `HH:MM` with nothing else around it, so excluding
//! it here matches observed behavior rather than guessing. Two other
//! functions (`dispatch`, `n15`) were ruled out for a *different*, still
//! unresolved bug (`"220 PM EDT"` reading `'oh'` instead of the right word) —
//! that bug is NOT fixed by this port; `"220"` is a 3-digit token and this
//! normalizer's own hour validator rejects any 3+ digit leading numeral.

use super::cstr::*;
use super::numwords::*;
use super::tabs::{ptab, rec30, Ptab, Rec30};
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::Fe;

const UNIT_TAB: Ptab = ptab(0x1008a9e8, 0x1009b044);
const AMPM_TAB: Rec30 = rec30(0x1008a508, 0x1009b03c);
const ZONE_TAB: Rec30 = rec30(0x1008a5c8, 0x1009b040);
const PATTERNS: Ptab = ptab(0x1009c1e0, 0x1009c228);

#[derive(Clone, Copy, PartialEq)]
enum Role {
    Hour,
    Minute,
    Second,
    Ampm,
    Zone,
    Unit,
}

#[derive(Clone)]
struct Slot {
    start: i32,
    end: i32,
    role: Role,
    idx: i32,
    text: [u8; 40],
}

fn tk(fe: &Fe, t: &[u8], p: usize, off: i32, mode: i32) -> (Tok, i32) {
    let mut x = Tok::default();
    let r = tokenize(&mut x, t, p, off, mode, fe.ud.as_ref());
    (x, r)
}

pub fn time(fe: &Fe, wa: &mut Wa, _tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let mut sl: Vec<Slot> = Vec::with_capacity(6);
    let mut colons: u32 = 0;
    let (mut q, mut o) = (p, off);
    while sl.len() < 6 {
        let (x, r) = tk(fe, t, q, o, 4);
        if r == 0 || x.class == 9 {
            break;
        }
        if x.len == 1 && x.text[0] == b':' && x.ws == 0 {
            let ok = matches!(sl.last().map(|s| s.role), Some(Role::Hour) | Some(Role::Minute)) && colons < 2;
            if !ok {
                break;
            }
            colons += 1;
            q += r as usize;
            o += r;
            continue;
        }
        let mut slot = Slot { start: x.start, end: x.end, role: Role::Hour, idx: -1, text: [0; 40] };
        strcpy(&mut slot.text, &x.text);

        let mut hit = false;
        if x.class == 1 {
            if !sl.iter().any(|s| s.role == Role::Ampm) {
                let i = AMPM_TAB.find(&x.text, true);
                if i >= 0 {
                    slot.role = Role::Ampm;
                    slot.idx = i;
                    hit = true;
                }
            }
            if !hit && !sl.iter().any(|s| s.role == Role::Zone) {
                let i = ZONE_TAB.find(&x.text, false);
                if i >= 0 {
                    slot.role = Role::Zone;
                    slot.idx = i;
                    hit = true;
                }
            }
            if !hit && !sl.iter().any(|s| s.role == Role::Unit) {
                let i = UNIT_TAB.find(&x.text, true);
                if i >= 0 && x.len < 3 {
                    slot.role = Role::Unit;
                    slot.idx = i;
                    hit = true;
                }
            }
        } else if x.class == 2 {
            let role = match colons {
                0 => Role::Hour,
                1 => Role::Minute,
                _ => Role::Second,
            };
            // A second numeral at the same colon-depth (no ':' consumed in
            // between) is not a continuation of this time expression.
            if (x.len == 1 || x.len == 2) && super::sbd::all_digit(&x.text) && !sl.iter().any(|s| s.role == role) {
                slot.role = role;
                hit = true;
            }
        }
        if !hit {
            break;
        }
        sl.push(slot);
        q += r as usize;
        o += r;
    }
    finish(wa, off, &sl)
}

/// Colons are reconstructed from consecutive Hour/Minute/Second roles rather
/// than tracked separately — valid by construction, since a `Minute`/`Second`
/// role is only ever assigned after an actual `:` token was scanned.
fn tag_string(sl: &[Slot]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut prev_hms = false;
    for s in sl {
        let is_hms = matches!(s.role, Role::Hour | Role::Minute | Role::Second);
        if is_hms {
            if prev_hms {
                out.push(b':');
            }
            out.push(b'N');
        } else {
            out.push(match s.role {
                Role::Ampm => b'm',
                Role::Zone => b'D',
                Role::Unit => b'U',
                _ => unreachable!(),
            });
        }
        prev_hms = is_hms;
    }
    out
}

fn pattern_ok(tags: &[u8]) -> bool {
    if tags == b"N:N" {
        return false;
    }
    let n = PATTERNS.count();
    for i in 0..n {
        if PATTERNS.get(i) == tags {
            return true;
        }
    }
    false
}

fn finish(wa: &mut Wa, off: i32, sl: &[Slot]) -> i32 {
    let mut k = sl.len();
    while k >= 1 {
        if pattern_ok(&tag_string(&sl[..k])) {
            let end = sl[k - 1].end;
            return if emit_time(wa, &sl[..k]) { end - off } else { 0 };
        }
        k -= 1;
    }
    -1
}

/// `FUN_10058b60`. Hour and seconds are read as plain cardinals, minutes are
/// pair-read (`FUN_10062590`, e.g. "45" -> "forty five"), matching captures.
/// A minute of "00" is treated as absent (`FUN_10064645(...) > 0` gate in the
/// decompile), and "o'clock" is spoken only for the bare hour:00:seconds
/// case (confirmed against `"6:00:30"` -> "six o'clock and thirty seconds") —
/// every other pattern that lacks a minute (the colonless "3 PM" forms)
/// stays silent on it, matching `"3 PM"` -> `"three" "PM"` with no "o'clock".
fn emit_time(wa: &mut Wa, sl: &[Slot]) -> bool {
    let hour = sl.iter().find(|s| s.role == Role::Hour);
    let minute = sl.iter().find(|s| s.role == Role::Minute).filter(|s| atoi(&s.text) > 0);
    let second = sl.iter().find(|s| s.role == Role::Second).filter(|s| atoi(&s.text) > 0);
    let ampm = sl.iter().find(|s| s.role == Role::Ampm);
    let zone = sl.iter().find(|s| s.role == Role::Zone);
    let unit = sl.iter().find(|s| s.role == Role::Unit);
    // Hour/minute/second share one span (through the last H:M:S component);
    // am/pm, zone and the duration unit each keep their own token span.
    let start = hour.map(|h| h.start).unwrap_or(0);
    let end = sl
        .iter()
        .rev()
        .find(|s| matches!(s.role, Role::Hour | Role::Minute | Role::Second))
        .map(|s| s.end)
        .unwrap_or(start);

    if let Some(h) = hour {
        let mut buf = new_buf();
        number(&mut buf, &h.text, 0);
        if !wa.emit_split(&buf, start, end, 0x44, 0x44, 5) {
            return false;
        }
        if minute.is_none() && second.is_some() && ampm.is_none() && zone.is_none() && unit.is_none() {
            if !wa.emit(s(0x10081198), start, end, 0x41, 0x44, 5) {
                return false;
            }
        }
    }
    if let Some(m) = minute {
        let mut buf = new_buf();
        pairs(&mut buf, &m.text);
        if !wa.emit_split(&buf, start, end, 0x44, 0x44, 5) {
            return false;
        }
    }
    if let Some(sec) = second {
        if !wa.emit(s(0x10077400), start, end, 0x41, 0x44, 5) {
            return false;
        }
        let mut buf = new_buf();
        number(&mut buf, &sec.text, 0);
        if !wa.emit_split(&buf, start, end, 0x44, 0x44, 5) {
            return false;
        }
        let word = if atoi(&sec.text) < 2 { s(0x10078918) } else { s(0x100811a0) };
        if !wa.emit(word, start, end, 0x41, 0x44, 5) {
            return false;
        }
    }
    if let Some(u) = unit {
        let is_hour = to_lower(u.text[0]) == b'h';
        let count = hour.map(|h| atoi(&h.text)).unwrap_or(0);
        let word = if is_hour {
            if count < 2 { s(0x10081340) } else { s(0x10081348) }
        } else if count < 2 {
            s(0x100774e8)
        } else {
            s(0x100811a8)
        };
        if !wa.emit(word, u.start, u.end, 0x41, 0x44, 5) {
            return false;
        }
    }
    if let Some(a) = ampm {
        if !wa.emit(AMPM_TAB.field(a.idx, 0x10), a.start, a.end, 0x41, 0x53, 5) {
            return false;
        }
    }
    if let Some(z) = zone {
        if !wa.emit(ZONE_TAB.field(z.idx, 0x10), z.start, z.end, 0x41, 0x44, 5) {
            return false;
        }
    }
    true
}

fn to_lower(c: u8) -> u8 {
    if c.is_ascii_uppercase() {
        c + 32
    } else {
        c
    }
}
