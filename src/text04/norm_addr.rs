//! `norm_address` (0x1002a860): postal addresses. A state or province with its ZIP or postal code
//! (`FUN_1002aec0`), a post office box (`FUN_1002b9a0`), `No. 4` and apartment/floor units
//! (`FUN_1002dfd0`), street addresses (`FUN_1002c9d0`: preposition, house number, direction, name, street
//! type, trailing direction), cities of two to four words (`FUN_1002bc50`: `city2-4.txt2`, words
//! abbreviated per `citya.txt2`) and of one word (`FUN_1002c6a0`: `city1.txt2`). Words go to the pending
//! queue, flushed into the WA when the whole address is read.
//!
//! The engine's current-token pointer is always the buffer's last token, so it is read back from the
//! buffer here.

use super::mem::*;
use super::norm_abbr::{dict_b40, name_like};
use super::normalize::{Cx, State};
use super::num;
use super::rx::{is_p, restore, save, Buf as A, Queue};
use super::tokenize::Tok;

const STATES: Rec30 = rec30(0x10081c40, 0x100848b8);
const PROVINCES: Rec30 = rec30(0x100827b0, 0x100848bc);
const STATE_NAMES: Ptab = ptab(0x10082a20, 0x100848c0);
const PROVINCE_NAMES: Ptab = ptab(0x10082b14, 0x100848c4);
const DIRS: Rec30 = rec30(0x10082b48, 0x100848c8);
const DIR_NAMES: Ptab = ptab(0x10082d88, 0x100848cc);
const UNITS: Rec30 = rec30(0x10082da8, 0x100848d0);
const UNIT_NAMES: Ptab = ptab(0x10082f88, 0x100848d4);
const LEVELS: Rec30 = rec30(0x10082fc0, 0x100848d8);
const LEVEL_NAMES: Ptab = ptab(0x10083110, 0x100848dc);
/// `LEVEL_NAMES` searched with `LEVELS`' count, as `FUN_1002dfd0` does after a level word.
const LEVEL_NAMES_SHORT: Ptab = ptab(0x10083110, 0x100848d8);
const MOUNT: Rec30 = rec30(0x10083138, 0x100848e0);
/// Lower-case words a city name may go on with (`and`, `de`, `of`, `the`).
const CONNECT: Ptab = ptab(0x1006f148, 0x1006f174);

fn upper(c: u8) -> bool {
    cls(c) & 0x80 != 0
}

fn lower(c: u8) -> bool {
    cls(c) & 0x40 != 0
}

fn end_ws(c: u8) -> bool {
    matches!(c, 0 | b' ' | b'\t' | b'\n' | b'\r')
}

/// `FUN_1000cb40`.
fn all_alpha(t: &[u8]) -> bool {
    let t = cs(t);
    !t.is_empty() && t.iter().all(|&c| is_alpha(c))
}

/// `FUN_1004fa00`.
fn all_lower(t: &[u8]) -> bool {
    let t = cs(t);
    !t.is_empty() && t.iter().all(|&c| lower(c))
}

/// A `.` glued to the word before it.
fn dot_after(t: &Tok) -> bool {
    t.ws == 0 && t.len < 3 && t.text[0] == b'.'
}

fn comma(t: &Tok) -> bool {
    t.ws < 3 && is_p(t, b',')
}

pub fn address(cx: &Cx, st: &mut State, cur: &mut usize, _t: &Tok) -> i32 {
    let sv = save(st);
    let mut a = A { cx, st, cur };
    let n0 = a.n();
    let start0 = a.last().start;
    let f;
    'commit: {
        if state_zip(&mut a, b'I', 0) {
            f = 5;
            break 'commit;
        }
        if po_box(&mut a) {
            let e = a.last().end;
            a.st.q.add(s(0x1006f180), start0, e, b'A', b'D', false);
            f = 6;
            let Some(t) = a.push() else { return a.fail(sv) };
            if t.ws < 3 && t.ty == 2 && num::all_digits(&t.text) {
                a.st.q.add(&num::group(&t.text), t.start, t.end, b'D', b'D', false);
            } else {
                a.pop();
            }
            break 'commit;
        }
        if no_unit(&mut a, b'I') {
            f = 2;
            break 'commit;
        }
        if no_unit(&mut a, b'D') {
            let Some(t) = a.push() else { return a.fail(sv) };
            if comma(&t) {
                a.st.q.add(b",", t.start, t.end, b'S', b'0', false);
                if a.push().is_none() {
                    return a.fail(sv);
                }
            }
            if street(&mut a, 0) {
                f = 2;
                break 'commit;
            }
            a.pop();
        }
        if street(&mut a, 0) {
            f = 2;
            for _ in 0..2 {
                let Some(t) = a.push() else { return a.fail(sv) };
                if comma(&t) {
                    a.st.q.add(b",", t.start, t.end, b'S', b'0', false);
                    if a.push().is_none() {
                        return a.fail(sv);
                    }
                }
                if !no_unit(&mut a, b'D') {
                    a.pop();
                }
            }
            break 'commit;
        }
        if city_multi(&mut a) {
            let q0 = a.st.q.n();
            let t = a.last();
            a.st.q.add(b",", t.start, t.end, b'S', b'0', false);
            let Some(t) = a.push() else { return a.fail(sv) };
            let seen = comma(&t);
            if seen && a.push().is_none() {
                return a.fail(sv);
            }
            if state_zip(&mut a, b'D', 0) {
                f = 4;
                break 'commit;
            }
            if !seen {
                a.st.q.set_n(q0);
            }
            f = 3;
            a.pop();
            break 'commit;
        }
        if city_one(&mut a) {
            let Some(t) = a.push() else { return a.fail(sv) };
            if t.ws == 0 && is_p(&t, b',') {
                if a.push().is_none() {
                    return a.fail(sv);
                }
                if state_zip(&mut a, b'D', 0) {
                    f = 4;
                    break 'commit;
                }
                a.back(n0);
            }
        }
        if a.n() != n0 {
            a.back(n0);
        }
        return 5;
    }
    let mut r = a.st.done.append_all(&a.st.ahead, f);
    if r == 1 {
        r = a.st.q.flush(&mut a.st.wa);
        if r == 1 {
            return 1;
        }
    }
    restore(a.st, a.cur, sv, r)
}

/// Back to `n0` with the queue count at `q0`: the common failure exit of the helpers.
fn undo(a: &mut A, n0: i32, q0: i32) -> bool {
    a.back(n0);
    a.st.q.set_n(q0);
    false
}

/// `FUN_1002aec0`: a state or province (two-letter code or name) with an optional `.`/`,` and its ZIP
/// code (`68102`, `68102-1234`) or Canadian postal code (`K1A 0B1`). Mode `D` also takes it alone.
fn state_zip(a: &mut A, mode: u8, p7: u8) -> bool {
    let t = a.last();
    if t.ty != 1 {
        return false;
    }
    if mode == b'I' && a.st.q.n() != 0 {
        a.st.q.reset();
    }
    let q0 = a.st.q.n();
    let s0 = t.start;
    let n0 = a.n();
    let mut st_ab = Vec::new();
    let mut pr_ab = Vec::new();
    let mut name = false;
    if !code(&mut st_ab, &t, STATES, p7) && !code(&mut pr_ab, &t, PROVINCES, p7) {
        name = full_name(a, &mut st_ab, STATE_NAMES, mode) || full_name(a, &mut pr_ab, PROVINCE_NAMES, mode);
        if !name {
            if a.n() != n0 {
                undo(a, n0, q0);
            }
            return false;
        }
    }
    let mut l6c = a.n();
    let e0 = a.last().end;
    let mut dot = false;
    let mut cm = false;
    let Some(t) = a.push() else { return undo(a, n0, q0) };
    if dot_after(&t) {
        if name {
            if mode == b'D' {
                a.pop();
                a.st.q.add(&st_ab, s0, e0, b'A', b'D', false);
                return true;
            }
            a.back(n0);
            return false;
        }
        l6c = a.n();
        dot = true;
        if a.push().is_none() {
            return undo(a, n0, q0);
        }
    }
    let t = a.last();
    if comma(&t) {
        l6c = a.n();
        cm = true;
        let r = if pr_ab.is_empty() { a.push() } else { a.push_r() };
        if r.is_none() {
            return undo(a, n0, q0);
        }
    } else if !pr_ab.is_empty() {
        a.pop();
        if a.push_r().is_none() {
            return undo(a, n0, q0);
        }
    }
    'alone: {
        if st_ab.is_empty() {
            if pr_ab.is_empty() {
                break 'alone;
            }
            let t = a.last();
            if t.ws > 2 || !postal(&t.text, t.len) {
                break 'alone;
            }
            let s1 = t.start;
            let half = t.text().to_vec();
            let Some(t) = a.push_r() else { return undo(a, n0, q0) };
            if !(t.ws < 3 && postal(&t.text, t.len)) {
                break 'alone;
            }
            pr_ab.push(b',');
            a.st.q.add(&pr_ab, s0, e0, b'A', b'D', false);
            let both = [&half[..], b" ", t.text()].concat();
            spell_queue(&mut a.st.q, &both, s1, t.end);
            a.st.q.add(b",", t.start, t.end, b'S', b'D', false);
            return true;
        }
        let t = a.last();
        if t.ws > 2 || t.ty != 2 || !zip(&t.text, t.len) {
            break 'alone;
        }
        st_ab.push(b',');
        a.st.q.add(&st_ab, s0, e0, b'A', b'D', false);
        a.st.q.add(&num::group(&t.text), t.start, t.end, b'D', b'D', false);
        let Some(t) = a.push() else { return undo(a, n0, q0) };
        if !(t.ws <= 2 && (is_p(&t, b'-') || is_p(&t, b'~'))) {
            if t.ws < 3 && t.ty == 2 && zip(&t.text, t.len) {
                a.st.q.add(b",", t.start, t.end, b'S', b'0', false);
                a.st.q.add(&num::group(&t.text), t.start, t.end, b'D', b'D', false);
                return true;
            }
            let short = t.ty < 7;
            let t = a.pop();
            if short {
                a.st.q.add(b",", t.start, t.end, b'S', b'0', false);
            }
            return true;
        }
        let dash = t.text[0] == b'-';
        let Some(t) = a.push() else { return undo(a, n0, q0) };
        if t.ws < 3 && t.ty == 2 && zip(&t.text, t.len) {
            a.st.q.add(if dash { s(0x1006f190) } else { s(0x10065380) }, t.start, t.end, b'S', b'D', false);
            a.st.q.add(&num::group(&t.text), t.start, t.end, b'D', b'D', false);
            return true;
        }
        a.seek(-2);
        return true;
    }
    if mode == b'D' {
        a.back(l6c);
        if cm {
            a.pop();
        }
        if dot && !cm {
            a.pop();
        }
        let w = if st_ab.is_empty() { &pr_ab } else { &st_ab };
        a.st.q.add(w, s0, e0, b'A', b'D', false);
        return true;
    }
    if a.n() != n0 {
        undo(a, n0, q0);
    }
    false
}

/// `FUN_1002b750`: a two-letter code (capitalized unless `p7` is `B`) as its name.
fn code(out: &mut Vec<u8>, t: &Tok, tab: Rec30, p7: u8) -> bool {
    out.clear();
    if t.len != 2 || !all_alpha(&t.text) || !(p7 == b'B' || upper(t.text[0])) {
        return false;
    }
    let k = tab.find(&t.text, true);
    if k < 0 {
        return false;
    }
    *out = tab.field(k, 0x10).to_vec();
    true
}

/// `FUN_1002b7e0`: a state or province name, possibly of several words, in title case.
fn full_name(a: &mut A, out: &mut Vec<u8>, tab: Ptab, mode: u8) -> bool {
    out.clear();
    let t = a.last();
    if t.len < 2 || (mode == b'I' && !upper(t.text[0])) {
        return false;
    }
    let k = multi(a, tab);
    if k < 0 {
        return false;
    }
    let mut w = tab.get(k).to_vec();
    for i in 0..w.len() {
        if i == 0 {
            w[0] = toupper(w[0]);
        } else if w[i] == b' ' && i + 1 < w.len() {
            w[i + 1] = toupper(w[i + 1]);
        }
    }
    *out = w;
    true
}

/// `FUN_10049a00` (insensitive): bisection over a `char*` table whose keys may run over several
/// tokens, read and left in the buffer on a match.
fn multi(a: &mut A, tab: Ptab) -> i32 {
    if a.last().text[0] == 0 {
        return -1;
    }
    let (mut lo, mut hi) = (0, tab.count() - 1);
    let mut mid = hi;
    while lo <= hi {
        mid /= 2;
        let key = tab.get(mid);
        let t = a.last();
        let l = t.len.max(0) as usize;
        let mut c = strnicmp(key, &t.text, l);
        if c == 0 {
            match at(key, l as isize) {
                0 => return mid,
                b' ' => {
                    let mut rest: &[u8] = &key[l + 1..];
                    let mut k = 0;
                    c = 1;
                    while let Some(nt) = a.push() {
                        k += 1;
                        if nt.len < 1 {
                            break;
                        }
                        let nl = nt.len as usize;
                        let d = strnicmp(rest, &nt.text, nl);
                        if d != 0 {
                            c = d;
                            break;
                        }
                        match at(rest, nl as isize) {
                            0 => return mid,
                            b' ' => rest = &rest[nl + 1..],
                            _ => break,
                        }
                    }
                    a.seek(-k);
                }
                _ => c = 1,
            }
        }
        if c >= 0 {
            hi = mid - 1;
        } else {
            lo = mid + 1;
        }
        mid = hi + lo;
    }
    -1
}

/// `FUN_1002b8b0`.
fn zip(t: &[u8], len: i32) -> bool {
    3 < len && len < 6 && num::all_digits(t)
}

/// `FUN_1002b8e0`: one half of a Canadian postal code (`K1A`, `0B1`).
fn postal(t: &[u8], len: i32) -> bool {
    if len != 3 {
        return false;
    }
    let v;
    if is_alpha(t[0]) && is_digit(t[1]) && is_alpha(t[2]) {
        let u = toupper(t[0]);
        if u != 0 && s(0x1006f1a0).contains(&u) {
            return false;
        }
        v = toupper(t[2]);
    } else {
        if !is_digit(t[0]) || !is_alpha(t[1]) || !is_digit(t[2]) {
            return false;
        }
        v = toupper(t[1]);
    }
    !(v != 0 && s(0x1006f198).contains(&v))
}

/// `FUN_1002f280`: letters and digits as one word each (`FUN_1002eda0`; `0` is "oh").
fn spell_queue(q: &mut Queue, t: &[u8], s0: i32, e0: i32) -> bool {
    let mut out = Vec::new();
    for &c in cs(t) {
        if is_alpha(c) {
            out.push(c);
            out.push(b' ');
        } else if is_digit(c) {
            let w: &[u8] = match c {
                b'0' => s(0x1006f200),
                b'1' => s(0x1006655c),
                b'2' => s(0x10066344),
                b'3' => s(0x10066370),
                b'4' => s(0x10066488),
                b'5' => s(0x100664a8),
                b'6' => s(0x100663d4),
                b'7' => s(0x100663f8),
                b'8' => s(0x100664ec),
                b'9' => s(0x1006644c),
                _ => continue,
            };
            out.extend_from_slice(w);
            out.push(b' ');
        }
    }
    let mut w = Vec::new();
    for k in 0..=out.len() {
        let c = at(&out, k as isize);
        if c == b' ' || c == 0 {
            if w.is_empty() {
                return false;
            }
            if !q.add(&w, s0, e0, b'A', if w.len() == 1 { b'S' } else { b'D' }, false) {
                return false;
            }
            w.clear();
        } else {
            w.push(c);
        }
    }
    true
}

/// `FUN_1002b9a0`: `P.O.B`, `POB`, `P.O. Box`, `P.O.Box`, `Post Office Box`, `Postoffice Box`.
fn po_box(a: &mut A) -> bool {
    let t = a.last();
    if t.ty != 1 {
        return false;
    }
    let w = t.text;
    let box_after = |a: &mut A, t: &Tok| -> bool {
        if t.ws > 2 || stricmp(&t.text, s(0x1006f1d0)) != 0 {
            a.seek(-2);
            return false;
        }
        true
    };
    if stricmp(&w, s(0x1006f1dc)) == 0 || stricmp(&w, s(0x1006f1d8)) == 0 {
        let Some(t) = a.push() else { return false };
        if !(t.ws == 0 && is_p(&t, b'.')) {
            a.pop();
        }
        return true;
    }
    if stricmp(&w, s(0x1006f1d4)) == 0 {
        let Some(t) = a.push() else { return false };
        if !(t.ws == 0 && is_p(&t, b'.')) {
            if t.ws < 3 && stricmp(&t.text, s(0x1006f1d0)) == 0 {
                return true;
            }
            a.seek(-1);
            return false;
        }
        let Some(t) = a.push() else {
            a.seek(-1);
            return false;
        };
        return box_after(a, &t);
    }
    if stricmp(&w, s(0x1006f1c8)) == 0 {
        return true;
    }
    if stricmp(&w, s(0x1006f1c0)) != 0 {
        if stricmp(&w, s(0x1006f1ac)) != 0 {
            return false;
        }
        let Some(t) = a.push() else { return false };
        if t.ws < 3 && stricmp(&t.text, s(0x1006f1d0)) == 0 {
            return true;
        }
        a.pop();
        return false;
    }
    let Some(t) = a.push() else { return false };
    if t.ws > 2 || stricmp(&t.text, s(0x1006f1b8)) != 0 {
        a.seek(-1);
        return false;
    }
    let Some(t) = a.push() else {
        a.seek(-1);
        return false;
    };
    box_after(a, &t)
}

/// `FUN_1002dfd0`: `No. 4` ("number four"), an ordinal floor (`3rd Floor`); in mode `D` also `# 4`, a
/// unit with its number (`Apt 3B`, `Suite 200-A`) and a level word (`Bsmt`, `Rear`).
fn no_unit(a: &mut A, mode: u8) -> bool {
    let n0 = a.n();
    let q0 = a.st.q.n();
    if q0 != 0 && mode == b'I' {
        a.st.q.set_n(0);
    }
    let t = a.last();
    if t.ty == 2 && num::is_ordinal(&t.text) {
        let ord = t;
        let Some(t) = a.push() else { return undo(a, n0, q0) };
        if t.ws > 2 || t.ty != 1 {
            a.pop();
            return false;
        }
        let (mut un, mut lv, mut ln) = (-1, -1, -1);
        let u = UNITS.find(&t.text, true);
        if u < 0 {
            un = UNIT_NAMES.find(&t.text, true);
            if un < 0 {
                lv = LEVELS.find(&t.text, true);
                if lv < 0 {
                    ln = LEVEL_NAMES.find(&t.text, true);
                    if ln < 0 {
                        a.pop();
                        return false;
                    }
                }
            }
        }
        if a.st.q.n() >= 20 {
            return false;
        }
        a.st.q.add(&num::ordinal(&ord.text), ord.start, ord.end, b'D', b'D', false);
        let name = if u >= 0 {
            UNITS.field(u, 0x10)
        } else if lv >= 0 {
            LEVELS.field(lv, 0x10)
        } else if un < 0 {
            LEVEL_NAMES.get(ln)
        } else {
            UNIT_NAMES.get(un)
        };
        a.st.q.add(name, t.start, t.end, b'A', b'D', true);
        if u < 0 && lv < 0 {
            return true;
        }
        let Some(t) = a.push() else { return undo(a, n0, q0) };
        if !dot_after(&t) {
            a.pop();
        }
        return true;
    }
    let number_sign;
    if mode == b'D' && t.ty == 1 {
        let u = UNITS.find(&t.text, true);
        let name = if u >= 0 {
            Some(UNITS.field(u, 0x10))
        } else {
            let k = UNIT_NAMES.find(&t.text, true);
            (k >= 0).then(|| UNIT_NAMES.get(k))
        };
        if let Some(name) = name {
            return unit_number(a, n0, q0, u >= 0, name);
        }
        number_sign = false;
    } else {
        number_sign = mode == b'D' && is_p(&t, b'#');
    }
    if !number_sign && stricmp(&t.text, s(0x1006f1fc)) != 0 {
        let none = |a: &mut A| {
            if a.n() != n0 {
                undo(a, n0, q0);
            }
            false
        };
        if mode != b'D' || t.ty != 1 {
            return none(a);
        }
        let u = LEVELS.find(&t.text, true);
        let name = if u >= 0 {
            LEVELS.field(u, 0x10)
        } else {
            let k = LEVEL_NAMES_SHORT.find(&t.text, true);
            if k < 0 {
                return none(a);
            }
            LEVEL_NAMES.get(k)
        };
        a.st.q.add(name, t.start, t.end, b'A', b'D', false);
        if u < 0 {
            return true;
        }
        let Some(t) = a.push() else { return undo(a, n0, q0) };
        if !dot_after(&t) {
            a.pop();
        }
        return true;
    }
    let no = t;
    let Some(mut t) = a.push() else { return undo(a, n0, q0) };
    if t.text[0] != b'#' && dot_after(&t) {
        let Some(t2) = a.push() else { return undo(a, n0, q0) };
        t = t2;
    }
    if t.ws < 3 && t.ty == 2 && num::all_digits(&t.text) {
        a.st.q.add(s(0x100652d0), no.start, no.end, b'S', b'D', false);
        a.st.q.add(&num::group(&t.text), t.start, t.end, b'D', b'D', false);
        return true;
    }
    a.pop();
    false
}

/// The unit branch of `FUN_1002dfd0`: the unit word, an optional `.`, then the glued pieces of its
/// number (`3B`, `200-A`, `4~6`).
fn unit_number(a: &mut A, n0: i32, q0: i32, abbr: bool, name: &[u8]) -> bool {
    let t = a.last();
    a.st.q.add(name, t.start, t.end, b'A', b'D', false);
    let Some(mut t) = a.push() else { return undo(a, n0, q0) };
    if abbr && dot_after(&t) {
        let Some(t2) = a.push() else { return undo(a, n0, q0) };
        t = t2;
    }
    if t.ws > 2 || (t.ty != 2 && t.ty != 1) {
        a.pop();
        return true;
    }
    if !unit_piece(a, &t) {
        return false;
    }
    while !end_ws(a.nextc()) {
        let Some(t) = a.push() else { return undo(a, n0, q0) };
        let alnum = cls(a.nextc()) & 0xd0 != 0;
        if is_p(&t, b'-') && alnum {
            a.st.q.add(s(0x1006f190), t.start, t.end, b'S', b'D', false);
        } else if is_p(&t, b'~') && alnum {
            a.st.q.add(s(0x10065380), t.start, t.end, b'S', b'D', false);
        } else if t.ty == 1 || t.ty == 2 {
            if !unit_piece(a, &t) {
                return false;
            }
        } else {
            a.pop();
            return true;
        }
    }
    true
}

/// A word as itself, a number as an ordinal or in digit groups; false when the queue is full.
fn unit_piece(a: &mut A, t: &Tok) -> bool {
    if t.ty == 1 {
        a.st.q.add(&t.text, t.start, t.end, b'A', if t.len == 1 { b'S' } else { b'D' }, false);
        return true;
    }
    if a.st.q.n() >= 20 {
        return false;
    }
    let w = if num::is_ordinal(&t.text) { num::ordinal(&t.text) } else { num::group(&t.text) };
    a.st.q.add(&w, t.start, t.end, b'D', b'D', false);
    true
}

/// `FUN_1002c6a0`: a one-word city of `city1.txt2` (glued pieces such as `O'Fallon`, `Wilkes-Barre`).
fn city_one(a: &mut A) -> bool {
    let fe = a.cx.fe;
    let t = a.last();
    if t.ty != 1 || lower(t.text[0]) {
        return false;
    }
    let s0 = t.start;
    let mut total = t.len;
    let n0 = a.n();
    let mut c = a.nextc();
    while !end_ws(c) {
        let Some(t) = a.push() else {
            a.back(n0);
            return false;
        };
        total += t.len;
        if total > 0x1d {
            a.pop();
            break;
        }
        if !is_alpha(t.text[0]) && (!(is_p(&t, b'\'') || is_p(&t, b'-')) || !is_alpha(a.nextc())) {
            a.pop();
            break;
        }
        c = a.nextc();
    }
    let mut name = Vec::new();
    for i in n0 - 1..a.n() {
        name.extend_from_slice(a.tok(i).text());
    }
    let k = fe.city[0].find(&name);
    if k >= 0 {
        let e = a.last().end;
        a.st.q.add(&fe.city[0].keys[k as usize], s0, e, b'A', b'D', false);
        return true;
    }
    if a.n() != n0 {
        a.back(n0);
    }
    false
}

/// `FUN_1002c8b0`: a word that may stand at position `k` of a city name.
fn city_word(t: &Tok, k: usize) -> bool {
    if t.ty != 1 {
        return false;
    }
    let c = t.text[0];
    if k > 0 && lower(c) {
        return CONNECT.find(&t.text, true) >= 0;
    }
    upper(c) && !t.text().contains(&b'.')
}

/// `FUN_1002c920`: the word before the last token is `O` (`O'`).
fn after_o(a: &A) -> bool {
    a.n() >= 2 && strcmp(&a.tok(a.n() - 2).text, s(0x100699b0)) == 0
}

/// `FUN_1002bc50`: a city of two to four words (`city2.txt2`..`city4.txt2`, longest first); a word may
/// be a `citya` abbreviation (`Bch`, `Spgs`) where the entry's flag for that word allows it.
fn city_multi(a: &mut A) -> bool {
    let fe = a.cx.fe;
    let n0 = a.n();
    if a.st.q.n() != 0 {
        a.st.q.reset();
    }
    let fail = |a: &mut A| {
        a.back(n0);
        false
    };
    let mut k = 0usize;
    let mut bounds = [-1i32; 4];
    let mut dot = [false; 4];
    let mut ab = [false; 4];
    let mut abbrs = 0;
    let mut names: [Vec<u8>; 4] = Default::default();
    let mut raw: [Vec<u8>; 4] = Default::default();
    let mut stop = false;
    while k < 4 {
        if !city_word(&a.last(), k) {
            if k != 0 {
                a.pop();
            }
            break;
        }
        let mut c = a.nextc();
        while !end_ws(c) {
            let Some(t) = a.push() else { return fail(a) };
            let ch = t.text[0];
            if is_alpha(ch) {
                c = a.nextc();
                continue;
            }
            let p1 = (t.ty == 3 || t.ty == 7) && t.len == 1;
            if p1 && ch == b'\'' {
                let c2 = a.nextc();
                if is_alpha(c2) || (end_ws(c2) && after_o(a)) {
                    c = a.nextc();
                    continue;
                }
                if k == 0 {
                    return fail(a);
                }
                a.pop();
                stop = true;
                break;
            }
            if p1 && ch == b'-' {
                if is_alpha(a.nextc()) {
                    c = a.nextc();
                    continue;
                }
                if k == 0 {
                    return fail(a);
                }
                a.pop();
                stop = true;
                break;
            }
            if !(p1 && ch == b'.') {
                a.pop();
                stop = true;
                break;
            }
            if is_digit(a.nextc()) {
                if k < 2 {
                    return fail(a);
                }
                a.back(bounds[k - 1]);
                stop = true;
                break;
            }
            dot[k] = true;
            break;
        }
        bounds[k] = a.n();
        k += 1;
        if stop {
            break;
        }
        if a.push().is_none() {
            return fail(a);
        }
    }
    if k < 2 {
        return fail(a);
    }
    if k == 4 {
        a.pop();
    }
    let mut i = k - 1;
    while i > 0 {
        let w = a.tok(bounds[k - 2]);
        if w.len < 2 || !lower(w.text[0]) {
            break;
        }
        i -= 1;
        k -= 1;
    }
    a.back(bounds[k - 1]);
    if k < 2 {
        return fail(a);
    }
    for seg in 0..k {
        let from = if seg == 0 { n0 - 1 } else { bounds[seg - 1] };
        let mut buf = Vec::new();
        let mut len = 0;
        for x in from..bounds[seg] {
            if dot[seg] && x == bounds[seg] - 1 {
                continue;
            }
            let w = a.tok(x);
            len += w.len;
            if len + 1 > 0x1d {
                return fail(a);
            }
            buf.extend_from_slice(w.text());
        }
        let j = fe.citya.find(&buf);
        if j < 0 {
            if seg == 0 && dot[0] {
                return fail(a);
            }
            names[seg] = buf.clone();
        } else {
            names[seg] = fe.citya.vals[j as usize].clone();
            ab[seg] = true;
            abbrs += 1;
        }
        raw[seg] = buf;
    }
    if k <= abbrs {
        return fail(a);
    }
    while k > 1 {
        let mut joined: Vec<u8> = Vec::new();
        let mut tot = 0;
        for name in names.iter().take(k) {
            tot += name.len();
            if tot + 1 > 0x1d {
                return fail(a);
            }
            if !joined.is_empty() {
                joined.push(b' ');
            }
            joined.extend_from_slice(name);
        }
        let tab = &fe.city[k - 1];
        let idx = tab.find(&joined);
        if idx >= 0 {
            let flags = &tab.vals[idx as usize];
            if (0..k).any(|i| ab[i] && at(flags, 1 + i as isize) != b'1') {
                return fail(a);
            }
            if ab[k - 1] && raw[k - 1].len() <= 1 {
                let pushed = dot[k - 1];
                if pushed && a.push().is_none() {
                    return fail(a);
                }
                let t = a.last();
                if t.ws <= 2 && t.ty == 1 && name_like(&t.text) {
                    return fail(a);
                }
                if pushed {
                    a.pop();
                }
            }
            let s0 = a.tok(n0 - 1).start;
            let e = a.last().end;
            if a.st.q.add(&tab.keys[idx as usize], s0, e, b'A', b'D', false) {
                if dot[k - 1] {
                    a.pop();
                }
                return true;
            }
            return fail(a);
        }
        k -= 1;
        if !ab[k - 1] {
            let j = fe.citya.find(&names[k - 1]);
            if j >= 0 {
                names[k - 1] = fe.citya.vals[j as usize].clone();
            }
        }
        a.back(bounds[k - 1]);
    }
    if a.n() != n0 {
        a.back(n0);
    }
    false
}

/// `FUN_1002c9d0`'s word slots (`local_28[1..]`): 0 preposition, 1 article, 2 house number, 3 direction,
/// 4/5 name, 6 street type, 7 second street type, 8 trailing direction or number. Slots hold buffer
/// indices: the engine keeps pointers into the buffer.
struct Street {
    s: [Option<i32>; 12],
    np: usize,
    cnt: i32,
    /// Trailing direction (`param_6` of `FUN_1002d650`) and its capital flag.
    dir2: Option<&'static [u8]>,
    dir2_cap: bool,
    ty1: Option<Vec<u8>>,
    ty2: Option<Vec<u8>>,
    /// A `,` followed the street type. The engine keeps this flag in a reused pointer argument, so it is
    /// set before the first write.
    comma: bool,
}

impl Street {
    fn put(&mut self, a: &A) {
        self.s[self.np] = Some(a.n() - 1);
        self.np += 1;
    }

    fn get(&self, a: &A, i: usize) -> Option<Tok> {
        self.s.get(i).copied().flatten().map(|k| a.tok(k))
    }
}

/// `FUN_1002c9d0`: a street address: [`at`/`on`/`onto` [`the`/`a`/`an`]] [number] [direction] name
/// [name] type [type] [, | direction | number]; otherwise `St. Hwy`.
fn street(a: &mut A, mode: u8) -> bool {
    let fe = a.cx.fe;
    let n0 = a.n();
    let mut v = Street { s: [None; 12], np: 0, cnt: 0, dir2: None, dir2_cap: false, ty1: None, ty2: None, comma: true };
    let mut l48 = false;
    let mut cap = false;
    let mut dir_name: Option<&'static [u8]> = None;
    let mut mount: Option<Vec<u8>> = None;
    let t = a.last();
    let prep = [s(0x10065374), s(0x10065384), s(0x100667dc)];
    if t.ty != 1 || !prep.iter().any(|p| stricmp(&t.text, p) == 0) {
        v.np += 2;
    } else {
        v.cnt += 1;
        v.put(a);
        let Some(t) = a.push() else { return undo_n(a, n0) };
        let art = [s(0x10065208), s(0x1006539c), s(0x1006520c)];
        if t.ws < 3 && t.ty == 1 && art.iter().any(|p| stricmp(&t.text, p) == 0) {
            v.put(a);
            if a.push().is_none() {
                return undo_n(a, n0);
            }
        } else {
            v.np += 1;
        }
    }
    let t = a.last();
    if (a.n() == n0 || t.ws < 3) && t.ty == 2 && num::all_digits(&t.text) {
        v.cnt += 1;
        v.put(a);
        if a.push().is_none() {
            return undo_n(a, n0);
        }
    } else {
        v.np += 1;
    }
    let t = a.last();
    let (mut u, mut k) = (-1, -1);
    let is_dir = (a.n() == n0 || t.ws < 3) && t.ty == 1 && {
        u = DIRS.find(&t.text, true);
        u >= 0 || {
            k = DIR_NAMES.find(&t.text, true);
            k >= 0
        }
    };
    if is_dir {
        v.cnt += 1;
        v.put(a);
        dir_name = Some(if u < 0 { DIR_NAMES.get(k) } else { DIRS.field(u, 0x10) });
        cap = upper(t.text[0]);
        let Some(t) = a.push() else { return undo_n(a, n0) };
        if u >= 0 && dot_after(&t) && a.push().is_none() {
            return undo_n(a, n0);
        }
    } else {
        v.np += 1;
    }
    let t = a.last();
    if t.ws <= 2 && (t.ty == 1 || t.ty == 2) && num::is_ordinal(&t.text) {
        v.put(a);
        v.np += 1;
        if a.push().is_none() {
            return undo_n(a, n0);
        }
        if street_type(a, &mut v, mode) {
            l48 = true;
            v.cnt += 1;
        } else {
            a.pop();
            v.s[v.np - 2] = None;
            v.np -= 2;
        }
    }
    'd18e: {
        let t = a.last();
        if l48 || t.ws >= 3 || t.ty != 1 {
            break 'd18e;
        }
        let mut nl = name_like(&t.text);
        if !nl && !all_lower(&t.text) {
            break 'd18e;
        }
        v.put(a);
        if !nl && mode == b'B' && dict_b40(a.cx, &t.text) {
            nl = true;
        }
        mount = name_of(fe, &t);
        if a.push().is_none() {
            return undo_n(a, n0);
        }
        let cnt0 = v.cnt;
        if v.cnt > 1 || (nl && v.cnt > 0) {
            let l30 = v.np;
            v.np += 1;
            if street_type(a, &mut v, mode) {
                if nl || v.cnt > cnt0 {
                    l48 = true;
                    break 'd18e;
                }
                v.np = l30;
            } else {
                v.np -= 1;
            }
        }
        mount = None;
        let mut dotm = false;
        if let Some(p) = v.get(a, v.np - 1).filter(|p| p.ty == 1) {
            let m = MOUNT.find(&p.text, true);
            if m >= 0 {
                mount = Some(MOUNT.field(m, 0x10).to_vec());
                if dot_after(&a.last()) {
                    dotm = true;
                    if a.push().is_none() {
                        return undo_n(a, n0);
                    }
                }
            }
        }
        let t = a.last();
        let mut nl2 = false;
        let ok = t.ws <= 2 && t.ty == 1 && {
            nl2 = name_like(&t.text);
            nl2 || all_lower(&t.text)
        };
        if !ok {
            l48 = false;
            a.pop();
            if dotm {
                a.pop();
            }
            v.s[v.np - 1] = None;
            v.np -= 1;
            break 'd18e;
        }
        l48 = false;
        v.put(a);
        if !nl2 && mode == b'B' && dict_b40(a.cx, &t.text) {
            nl2 = true;
        }
        if nl && nl2 {
            v.cnt += 1;
        }
        if a.push().is_none() {
            return undo_n(a, n0);
        }
        if v.cnt == 0 || !street_type(a, &mut v, mode) {
            a.seek(-2);
            if dotm {
                a.pop();
            }
            v.s[v.np - 1] = None;
            v.s[v.np - 2] = None;
            v.np -= 2;
            if nl || nl2 {
                v.cnt -= 1;
            }
            break 'd18e;
        }
        if mount.is_none() {
            v.cnt += 1;
        }
        l48 = true;
        if v.cnt == 0 && mode == b'B' {
            v.cnt = 1;
            return emit_street(a, &v, dir_name, cap, mount.as_deref());
        }
        if v.cnt == 0 {
            return street_fallback(a, n0);
        }
    }
    let cnt0 = v.cnt;
    if v.cnt < 1 {
        if !l48 {
            return street_fallback(a, n0);
        }
    } else if !l48 {
        v.np += 2;
        let r = street_type(a, &mut v, mode);
        let (s6, s7, s8) = (v.get(a, 6), v.get(a, 7), v.get(a, 8));
        let lowers = match (s6, s7) {
            (Some(x), Some(y)) => s8.is_none() && !upper(x.text[0]) && !upper(y.text[0]),
            _ => false,
        };
        if !r || (cnt0 < 2 && (cnt0 != 1 || v.cnt < 2 || lowers)) {
            return street_fallback(a, n0);
        }
    }
    if v.cnt != 0 {
        return emit_street(a, &v, dir_name, cap, mount.as_deref());
    }
    street_fallback(a, n0)
}

fn undo_n(a: &mut A, n0: i32) -> bool {
    a.back(n0);
    false
}

/// The name word's replacement: `Mt` as "Mount", or a street-type word's full form.
fn name_of(fe: &super::Fe, t: &Tok) -> Option<Vec<u8>> {
    if t.ty != 1 {
        return None;
    }
    let m = MOUNT.find(&t.text, true);
    if m >= 0 {
        return Some(MOUNT.field(m, 0x10).to_vec());
    }
    let j = fe.streeta.find(t.text());
    (j >= 0).then(|| fe.streeta.vals[j as usize].clone())
}

/// `LAB_1002d5af`: the queue emptied, then `St. Hwy`.
fn street_fallback(a: &mut A, n0: i32) -> bool {
    if a.st.q.n() != 0 {
        a.st.q.set_n(0);
    }
    if a.n() != n0 {
        a.back(n0);
    }
    if street_highway(a) {
        return true;
    }
    if a.st.q.n() != 0 {
        a.st.q.set_n(0);
    }
    if a.n() != n0 {
        a.back(n0);
    }
    false
}

/// `LAB_1002d265`: the slots in order.
fn emit_street(a: &mut A, v: &Street, dir_name: Option<&[u8]>, cap: bool, mount: Option<&[u8]>) -> bool {
    let group = |a: &mut A, t: &Tok| {
        if a.st.q.n() < 20 {
            a.st.q.add(&num::group(&t.text), t.start, t.end, b'D', b'D', false);
        }
    };
    let mut i = 0;
    while i < v.np {
        let Some(t) = v.get(a, i) else {
            i += 1;
            continue;
        };
        let q = |a: &mut A, w: &[u8], tk: &Tok, c18: u8, title: bool| {
            a.st.q.add(w, tk.start, tk.end, c18, b'D', title);
        };
        match i {
            0 | 1 => q(a, &t.text, &t, b'A', false),
            2 => group(a, &t),
            3 => {
                if let Some(w) = dir_name {
                    q(a, w, &t, b'A', cap);
                }
            }
            4 => {
                let name: &[u8] = mount.unwrap_or(&t.text[..]);
                if let Some(s5) = v.get(a, 5) {
                    q(a, name, &t, b'A', false);
                    q(a, &s5.text, &s5, b'A', false);
                    i = 5;
                } else if !is_digit(t.text[0]) {
                    q(a, name, &t, b'A', false);
                    i = 5;
                } else if a.st.q.n() < 20 {
                    q(a, &num::ordinal(&t.text), &t, b'D', false);
                    i = 5;
                } else {
                    i = 6;
                }
            }
            6 => {
                q(a, v.ty1.as_deref().unwrap_or(b""), &t, b'A', false);
                if v.comma && v.get(a, 7).is_none() {
                    q(a, b",", &t, b'S', false);
                }
            }
            7 => {
                if let Some(w) = v.ty2.as_deref() {
                    q(a, w, &t, b'A', false);
                    if v.comma {
                        q(a, b",", &t, b'S', false);
                    }
                }
            }
            8 => {
                if is_digit(t.text[0]) {
                    group(a, &t);
                } else if let Some(w) = v.dir2 {
                    q(a, w, &t, b'A', v.dir2_cap);
                }
            }
            _ => {}
        }
        i += 1;
    }
    true
}

/// `FUN_10050930`: a line break right after the token before the last one.
fn nl_before(a: &A) -> bool {
    let Some(p) = a.st.ahead.pos[(a.n() - 1).max(0) as usize] else { return false };
    let t = a.cx.t;
    let base = a.cx.base as isize;
    let mut p = p as isize;
    while (base < p && at(t, p) == b' ') || at(t, p) == b'\t' {
        p -= 1;
    }
    matches!(at(t, p), b'\r' | b'\n')
}

/// `FUN_1002d650`: a street type at slot `np` (`St.`, `Avenue`), maybe a second one, then a `,`, a
/// trailing direction or a number, unless a city or another street follows.
fn street_type(a: &mut A, v: &mut Street, mode: u8) -> bool {
    let fe = a.cx.fe;
    let n1 = a.n();
    v.dir2 = None;
    v.ty2 = None;
    v.ty1 = None;
    let t = a.last();
    if t.ws > 2 || t.ty != 1 {
        return false;
    }
    let u = fe.streeta.find(t.text());
    let f = if u < 0 { fe.streetf.find(t.text()) } else { -1 };
    if u < 0 && f < 0 {
        return false;
    }
    if v.get(a, 4).is_some_and(|s4| s4.ty == 1 && s4.len == 1) {
        return false;
    }
    v.put(a);
    v.ty1 = Some(if u < 0 { fe.streetf.keys[f as usize].clone() } else { fe.streeta.vals[u as usize].clone() });
    let Some(t) = a.push() else { return undo_n(a, n1) };
    if u >= 0 && dot_after(&t) && a.push().is_none() {
        return undo_n(a, n1);
    }
    let mut n2 = a.n();
    let mut second = false;
    let mut flags = 0u8;
    let t = a.last();
    let (mut u2, mut f2) = (-1, -1);
    let is2 = t.ws < 3 && t.ty == 1 && {
        u2 = fe.streeta.find(t.text());
        u2 >= 0 || {
            f2 = fe.streetf.find(t.text());
            f2 >= 0
        }
    };
    if is2 {
        v.put(a);
        v.ty2 = Some(if u2 < 0 { fe.streetf.keys[f2 as usize].clone() } else { fe.streeta.vals[u2 as usize].clone() });
        flags = 0x10;
        second = true;
        let Some(t) = a.push() else { return undo_n(a, n1) };
        if u2 >= 0 && dot_after(&t) && a.push().is_none() {
            return undo_n(a, n1);
        }
    } else {
        v.np += 1;
    }
    v.comma = false;
    enum P {
        After,
        Drop,
        Kept,
        City,
        DropOne,
        Done,
    }
    let t = a.last();
    let mut p = if t.ws <= 2 && is_p(&t, b',') {
        v.comma = true;
        let nl = nl_before(a);
        if a.push().is_none() {
            return undo_n(a, n1);
        }
        if nl {
            P::Drop
        } else {
            P::After
        }
    } else {
        P::After
    };
    let mut dirflag = false;
    loop {
        p = match p {
            P::After => {
                if nl_before(a) {
                    P::Drop
                } else {
                    let n3 = a.n();
                    let mut r = a.scratch(city_multi);
                    a.back(n3);
                    if !r {
                        r = a.scratch(|a| street(a, mode));
                    }
                    a.back(n3);
                    if r {
                        P::Drop
                    } else {
                        let t = a.last();
                        let word = t.ws <= 2 && t.ty == 1;
                        let d = if word { DIRS.find(&t.text, true) } else { -1 };
                        let f3 = if word && d < 0 { DIR_NAMES.find(&t.text, true) } else { -1 };
                        if t.ws < 3 && t.ty == 2 && num::all_digits(&t.text) {
                            v.put(a);
                            P::Done
                        } else if d < 0 && f3 < 0 {
                            a.pop();
                            if v.comma {
                                a.pop();
                            }
                            P::Kept
                        } else {
                            dirflag = true;
                            v.put(a);
                            flags |= 1;
                            v.dir2 = Some(if d < 0 { DIR_NAMES.get(f3) } else { DIRS.field(d, 0x10) });
                            v.dir2_cap = upper(t.text[0]);
                            let Some(t) = a.push() else { return undo_n(a, n1) };
                            n2 = n3;
                            if !dot_after(&t) {
                                a.pop();
                            }
                            P::City
                        }
                    }
                }
            }
            P::Drop => {
                a.pop();
                if v.comma {
                    a.pop();
                }
                P::Kept
            }
            P::Kept => {
                dirflag = false;
                if !second || v.comma {
                    P::Done
                } else {
                    P::City
                }
            }
            P::City => {
                let n4 = a.n();
                let Some(t) = a.push() else { return undo_n(a, n1) };
                if t.ws < 3 && t.ty == 1 {
                    a.back(n2);
                    if a.tok(n2 - 1).ty == 1 && a.scratch(city_multi) {
                        a.back(n2 - 1);
                        if dirflag {
                            v.dir2 = None;
                            flags &= 0x10;
                        } else {
                            flags &= 1;
                            v.ty2 = None;
                        }
                        v.np -= 1;
                        v.s[v.np] = None;
                        if v.comma {
                            P::DropOne
                        } else {
                            P::Done
                        }
                    } else {
                        a.back(n4);
                        P::Done
                    }
                } else {
                    P::DropOne
                }
            }
            P::DropOne => {
                a.pop();
                P::Done
            }
            P::Done => {
                if flags != 0 {
                    v.cnt += 1;
                }
                return true;
            }
        };
    }
}

/// `FUN_1002de40`: `St. Hwy` as "Street Highway".
fn street_highway(a: &mut A) -> bool {
    let n0 = a.n();
    let t = a.last();
    let undo = |a: &mut A| {
        a.back(n0);
        a.st.q.set_n(0);
        false
    };
    if t.ty == 1 && stricmp(&t.text, s(0x1006f1f8)) == 0 {
        let s0 = t.start;
        let Some(t) = a.push() else { return undo(a) };
        if dot_after(&t) && a.push().is_none() {
            return undo(a);
        }
        let t = a.last();
        if t.ws < 3 && t.ty == 1 && stricmp(&t.text, s(0x1006f1f4)) == 0 {
            let e = t.end;
            let Some(t) = a.push() else { return undo(a) };
            if !dot_after(&t) {
                a.pop();
            }
            a.st.q.add(s(0x1006f1e4), s0, e, b'A', b'D', false);
            return true;
        }
    }
    if a.n() != n0 {
        a.back(n0);
    }
    a.st.q.set_n(0);
    false
}
