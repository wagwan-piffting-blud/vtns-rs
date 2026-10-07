//! `FUN_10043810`: address's street/highway sub-recognizer, the last of the
//! four `FUN_10033760` dispatches, with its context check `FUN_10044b30`,
//! highway-number reader `FUN_10044d00`, validator `FUN_100447a0`, emitter
//! `FUN_100444e0` and highway emitter `FUN_100445e0`. class 0x12.
//!
//! Up to 6 slots in token mode 0x15, each tagged with a digit: `1` house
//! number (read in pairs), `2` compass direction (after the first slot),
//! `3` "MT"/"ST" (Mount/Saint), `4` a street name (ordinal "5th", a
//! `streetf` word, or any capitalized word), `5` a street type (`streeta`,
//! "Ave" -> "Avenue"), `6` a unit designator ("Apt", "Suite", "Basement",
//! "#", "NO"), `7` the unit/highway number after a `6`. A `.` after an
//! abbreviation and a `,` after a direction/name/type are absorbed. Runs of
//! `4` collapse to one unless the address ends in one, and the tags are
//! matched against 30 shapes grouped by the first tag ("145267" = number,
//! name, type, direction, unit, number). A match is followed by the city
//! recognizer (`FUN_10033b60`, flag 1) for a trailing "City, ST ZIP".

use super::cstr::*;
use super::norm_address_city::city;
use crate::era::Era;
use super::numwords::{new_buf, ordinal_numeral, ordinal_numeral_words, pairs};
use super::tabs::{ptab, rec30, Ptab, Rec30};
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::window::{back_words, last_two};
use super::Fe;

const COMPASS_ABBR: Rec30 = rec30(0x10099078, 0x1009b0e8);
const COMPASS_FULL: Ptab = ptab(0x100992b8, 0x1009b0ec);
const MOUNT: Rec30 = rec30(0x10099668, 0x1009b100);
const APT_ABBR: Rec30 = rec30(0x100992d8, 0x1009b0f0);
const APT_FULL: Ptab = ptab(0x100994b8, 0x1009b0f4);
const BSMT_ABBR: Rec30 = rec30(0x100994f0, 0x1009b0f8);
const BSMT_FULL: Ptab = ptab(0x10099640, 0x1009b0fc);
const PATTERNS: Ptab = ptab(0x100806f4, 0x1008076c);

fn tk(fe: &Fe, t: &[u8], p: usize, off: i32, mode: i32) -> (Tok, i32) {
    let mut x = Tok::default();
    let r = tokenize(&mut x, t, p, off, mode, fe.ud.as_ref());
    (x, r)
}

fn all_digits(w: &[u8]) -> bool {
    let w = cs(w);
    !w.is_empty() && w.iter().all(|&c| cls(c) & 0x10 != 0)
}

fn up(c: u8) -> bool {
    cls(c) & 0x80 != 0
}

#[derive(Clone, Default)]
struct Slot {
    start: i32,
    end: i32,
    ws: i32,
    udict: bool,
    mark: u8,
    p: usize,
    text: Vec<u8>,
    exp: Vec<u8>,
    comma: i32,
}

/// `FUN_10044b30`: "at/on/onto the ..." or "at/on/onto" right before.
fn after_preposition(wa: &Wa, t: &[u8], p: usize, off: i32) -> bool {
    let mut buf = [0u8; 96];
    let bw = back_words(&mut buf, t, p as isize - off as isize, p as isize, 2);
    if bw as i16 == 0 {
        return false;
    }
    let (a, b) = last_two(&buf, off - bw, 0x15).unwrap_or_default();
    let n = wa.n() as i32;
    let close = |x: &Tok| n > 0 && wa.ri32(n - 1, 4) == x.end && off - x.end < 2;
    let prep = |w: &[u8]| [s(0x10077694), s(0x100776b0), s(0x10078ed8)].iter().any(|v| stricmp(w, v) == 0);
    let art = |w: &[u8]| [s(0x1007736c), s(0x10077718), s(0x10077370)].iter().any(|v| stricmp(w, v) == 0);
    if a.len > 0 {
        if b.len < 1 {
            return false;
        }
        if close(&b) && a.class == 1 && b.class == 1 && prep(a.text()) && art(b.text()) {
            return true;
        }
    }
    b.len > 0 && close(&b) && b.class == 1 && prep(b.text())
}

/// `FUN_100393b0`.
fn numeric_piece(w: &[u8]) -> bool {
    super::norm_date::numish(w)
}

/// `FUN_10044d00`: the number after a unit designator ("3B", "12-14");
/// returns the bytes it spans, or -1.
fn unit_number(fe: &Fe, t: &[u8], p: usize, off: i32, first: &mut Tok) -> i32 {
    let (x, r0) = tk(fe, t, p, off, 0x19);
    *first = x.clone();
    if r0 == 0 || (x.class == 1 && x.len > 1) || (x.class != 1 && x.class != 2) || x.ws > 1 {
        return -1;
    }
    let mut used = x.consumed;
    let (y, r2) = tk(fe, t, p + used as usize, off + used, 0x15);
    if r2 == 0 || y.class == 9 {
        return used;
    }
    let yw = y.text();
    let conj = stricmp(yw, s(0x1007901c)) == 0 || stricmp(yw, s(0x10077400)) == 0 || (yw.first() == Some(&b'&') && y.len == 1);
    if conj && y.ws < 2 {
        used += r2;
        let (z, r3) = tk(fe, t, p + used as usize, off + used, 0x15);
        if r3 == 0 || (z.class != 1 && z.class != 2) || z.ws != y.ws {
            return -1;
        }
        return used + r3;
    }
    if conj && (yw.first() != Some(&b'&')) {
        return used;
    }
    let c0 = at(yw, 0);
    if !matches!(c0, b'-' | b'~' | b'/' | 0x96 | 0x97) || y.len != 1 || y.ws >= 2 {
        return used;
    }
    used += r2;
    let (z, r3) = tk(fe, t, p + used as usize, off + used, 0x15);
    if r3 == 0 || (z.class != 1 && z.class != 2) || z.ws != y.ws {
        return -1;
    }
    used += r3;
    let (q, r4) = tk(fe, t, p + used as usize, off + used, 0x15);
    if q.class == 3 && q.len == 1 && at(q.text(), 0) == c0 && q.ws == z.ws {
        return -1;
    }
    let qc = at(q.text(), 0);
    if qc != 0 && strchr(s(0x10080884), qc) && q.class == 3 && q.len == 1 && q.ws == z.ws && numeric_piece(x.text()) && numeric_piece(z.text()) {
        let (w, _) = tk(fe, t, p + (used + r4) as usize, off + used + r4, 0x15);
        if numeric_piece(w.text()) && w.ws == z.ws {
            return -1;
        }
    }
    used
}

/// `FUN_100447a0`.
fn validate(sl: &[Slot], idx: i32, flag: bool) -> Option<Vec<Slot>> {
    let mut sl = sl.to_vec();
    let t = |i: usize| sl[i].text.clone();
    let eq = |a: &[u8], va: u32| stricmp(a, s(va)) == 0;
    let ok = match idx {
        0xc => sl[3].mark == b'S',
        0x10 => {
            if eq(&t(2), 0x1007ff34) {
                sl[2].exp = s(0x1007ffc4).to_vec();
            }
            true
        }
        0x11 => sl[1].mark == b'S',
        0x12 => {
            if eq(&t(0), 0x1007ff34) && stricmp(&sl[1].exp, s(0x1008082c)) == 0 {
                sl[0].exp = s(0x1008087c).to_vec();
            }
            true
        }
        0x17 => {
            flag || !((eq(&t(0), 0x10080870) && eq(&t(1), 0x10080864) && eq(&t(2), 0x10080860) && sl[2].mark == b's')
                || (eq(&t(0), 0x1007d454) && eq(&t(1), 0x10080858) && eq(&t(2), 0x10080854) && sl[2].mark == b's'))
        }
        0x18 => eq(&t(1), 0x1008082c),
        0x19 => sl[0].mark == b'O',
        0x1a => {
            let s1 = sl[1].mark == b's';
            if !flag && s1 && (eq(&t(0), 0x10080870) || eq(&t(0), 0x10080864) || eq(&t(0), 0x10080848)) {
                false
            } else if eq(&t(0), 0x10080840) && s1 {
                false
            } else if !flag
                && ((eq(&t(0), 0x10080858) && eq(&t(1), 0x10080854) && s1)
                    || [0x1008083c, 0x10080838, 0x10080834, 0x1007ffd4].iter().any(|&v| eq(&t(1), v) && !(v == 0x1008083c && era().is_2009())))
            {
                false
            } else {
                true
            }
        }
        0x1b => eq(&t(0), 0x1008082c) && sl[0].mark != b'A',
        0x1d => sl[0].mark != b'A',
        _ => true,
    };
    if ok {
        Some(sl)
    } else {
        None
    }
}

/// `FUN_100445e0`: a unit/highway number span, piece by piece.
fn emit_number(fe: &Fe, wa: &mut Wa, t: &[u8], x: &Slot) -> bool {
    let len = x.end - x.start;
    let mut q = x.p + x.ws as usize;
    let mut o = x.start;
    let mut used = 0;
    while used < len {
        let (y, r) = tk(fe, t, q, o, 0);
        let w = y.text();
        let ok = match y.class {
            2 => {
                let mut b = new_buf();
                if ordinal_numeral(w) {
                    ordinal_numeral_words(&mut b, w);
                } else {
                    pairs(&mut b, w);
                }
                wa.emit_split(&b, y.start, y.end, 0x44, 0x44, 0x12)
            }
            1 => wa.emit(w, y.start, y.end, 0x41, if y.len == 1 { 0x53 } else { 0x44 }, 0x12),
            3 if matches!(at(w, 0), b'-' | b'~') => wa.emit(s(0x10077380), y.start, y.end, 0x41, 0x44, 0x12),
            3 if matches!(at(w, 0), b'/' | b'&') => wa.emit(s(0x10077400), y.start, y.end, 0x41, 0x44, 0x12),
            6 if matches!(at(w, 0), 0x96 | 0x97) => wa.emit(s(0x10077380), y.start, y.end, 0x41, 0x44, 0x12),
            _ => true,
        };
        if !ok {
            return false;
        }
        if r == 0 {
            break;
        }
        q += r as usize;
        o += r;
        used += r;
    }
    true
}

/// `FUN_100444e0`.
fn emit(fe: &Fe, wa: &mut Wa, t: &[u8], sl: &[Slot]) -> bool {
    for x in sl {
        if !x.udict {
            let ok = match x.mark {
                b'1' => wa.emit_split(&x.exp, x.start, x.end, 0x44, 0x44, 0x12),
                b'4' | b'S' => wa.emit_split(&x.text, x.start, x.end, 0x41, 0x44, 0x12),
                b'7' => emit_number(fe, wa, t, x),
                _ => wa.emit_split(&x.exp, x.start, x.end, 0x41, 0x44, 0x12),
            };
            if !ok {
                return false;
            }
        }
        if x.comma != 0 {
            let n = wa.n() as i32;
            wa.set_ri32(n - 1, 0x2c, 5);
        }
    }
    true
}

pub fn street(fe: &Fe, wa: &mut Wa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let flag = after_preposition(wa, t, p, off);
    let mut sl: Vec<Slot> = Vec::new();
    let mut tags: Vec<u8> = Vec::new();
    let mut srcs: Vec<u8> = Vec::new();
    let mut named = false;
    let (mut q, mut qo) = (p, off);
    while sl.len() < 6 {
        let (x, r) = if srcs.is_empty() {
            (tok.clone(), tok.consumed)
        } else {
            let (x, r) = tk(fe, t, q, qo, 0x15);
            if r == 0 || x.class == 9 {
                break;
            }
            (x, r)
        };
        if x.nl == 0 && x.ws > 2 {
            break;
        }
        let w = x.text().to_vec();
        let c0 = at(&w, 0);
        let k = srcs.len();
        let c = sl.len();
        let prev_src = srcs.last().copied().unwrap_or(0);
        if x.ws == 0 && x.len < 3 && c0 == b'.' && k > 0 && matches!(prev_src, b'd' | b'3' | b's' | b'a') {
            srcs.push(b'P');
            q += r as usize;
            qo += r;
            continue;
        }
        if (x.class == 3 || x.class == 7) && x.len == 1 && c0 == b',' && k > 0 && (prev_src != b',' || fe.era.is_2009()) && c > 0 && matches!(tags[c - 1], b'5' | b'2' | b'4') {
            srcs.push(b',');
            sl[c - 1].comma = r;
            q += r as usize;
            qo += r;
            continue;
        }
        let number = |x: &Tok| x.class == 2 && all_digits(x.text());
        let compass = |x: &Tok| -> Option<(u8, Vec<u8>)> {
            if c < 1 || x.class != 1 || !up(at(x.text(), 0)) {
                return None;
            }
            let i = COMPASS_ABBR.find(x.text(), true);
            if i >= 0 {
                return Some((b'd', COMPASS_ABBR.field(i, 0x10).to_vec()));
            }
            let j = COMPASS_FULL.find(x.text(), true);
            if j >= 0 {
                return Some((b'D', COMPASS_FULL.get(j).to_vec()));
            }
            None
        };
        let mut got: Option<(u8, u8, Vec<u8>)> = None;
        // The decompile's label chain, in order: A19 (house number), A57
        // (compass), B61 (Mount), BF5 (ordinal), C46 (street type/name,
        // then unit designator), F21 (the rest).
        let a19 = k == 0 || (c > 0 && prev_src == b'S' && stricmp(&sl[c - 1].text, s(0x1008082c)) == 0);
        let mut a57 = !a19 && c > 0;
        let mut b61 = !a19 && c == 0;
        if a19 {
            if number(&x) {
                let mut b = new_buf();
                pairs(&mut b, &w);
                got = Some((b'1', b'1', cs(&b).to_vec()));
            } else {
                a57 = true;
            }
        }
        if got.is_none() && a57 {
            match compass(&x) {
                Some((src, exp)) => got = Some((b'2', src, exp)),
                None => b61 = true,
            }
        }
        let (mut bf5, mut c46, mut f21) = (false, false, false);
        if got.is_none() && b61 {
            if !named && x.class == 1 {
                if up(c0) {
                    let m = MOUNT.find(&w, true);
                    if m >= 0 {
                        got = Some((b'3', b'3', MOUNT.field(m, 0x10).to_vec()));
                    } else {
                        bf5 = true;
                    }
                } else {
                    c46 = true;
                }
            } else {
                bf5 = true;
            }
        }
        if got.is_none() && bf5 {
            if x.class == 2 && ordinal_numeral(&w) {
                let mut b = new_buf();
                ordinal_numeral_words(&mut b, &w);
                got = Some((b'4', b'O', cs(&b).to_vec()));
                named = true;
            } else if x.class == 1 {
                c46 = true;
            } else {
                f21 = true;
            }
        }
        if got.is_none() && c46 {
            let a = if up(c0) { fe.streeta.find(&w) } else { -1 };
            let f = if a < 0 { fe.streetf.find(&w) } else { -1 };
            if (a >= 0 || f >= 0) && (c == 0 || sl[c - 1].comma == 0) {
                if a >= 0 {
                    got = Some((b'5', b's', fe.streeta.vals[a as usize].clone()));
                } else {
                    named = true;
                    got = Some((b'4', b'S', fe.streetf.keys[f as usize].clone()));
                }
            } else {
                let i = APT_ABBR.find(&w, true);
                let j = if i < 0 { APT_FULL.find(&w, true) } else { -1 };
                let k2 = if i < 0 && j < 0 { BSMT_ABBR.find(&w, true) } else { -1 };
                let l = if i < 0 && j < 0 && k2 < 0 { BSMT_FULL.find(&w, true) } else { -1 };
                if i >= 0 {
                    got = Some((b'6', b'a', APT_ABBR.field(i, 0x10).to_vec()));
                } else if k2 >= 0 {
                    got = Some((b'6', b'a', BSMT_ABBR.field(k2, 0x10).to_vec()));
                } else if j >= 0 {
                    got = Some((b'6', b'A', APT_FULL.get(j).to_vec()));
                } else if l >= 0 {
                    got = Some((b'6', b'A', BSMT_FULL.get(l).to_vec()));
                } else {
                    f21 = true;
                }
            }
        }
        let _ = f21;
        if got.is_none() && c > 0 {
            if x.class == 3 && x.len == 1 && c0 == b'#' {
                got = Some((b'6', b'6', s(0x100775c4).to_vec()));
            } else if x.class == 1 && stricmp(&w, s(0x10080828)) == 0 {
                got = Some((b'6', b'a', s(0x100775c4).to_vec()));
            } else if tags[c - 1] == b'6' {
                let mut f = Tok::default();
                let n7 = unit_number(fe, t, q, qo, &mut f);
                if n7 >= 0 {
                    sl.push(Slot { start: f.start, end: qo + n7, ws: f.ws, udict: false, mark: b'7', p: q, text: s(0x1008019c).to_vec(), exp: Vec::new(), comma: 0 });
                    tags.push(b'7');
                    srcs.push(b'7');
                    break;
                }
            }
        }
        if got.is_none() {
            if x.len < 2 || x.class != 1 || !up(c0) {
                break;
            }
            named = true;
            got = Some((b'4', b'4', Vec::new()));
        }
        let (tag, src, exp) = got.unwrap();
        sl.push(Slot { start: x.start, end: qo + r, ws: x.ws, udict: x.udict != 0, mark: src, p: q, text: w, exp, comma: 0 });
        tags.push(tag);
        srcs.push(src);
        q += r as usize;
        qo += r;
    }

    let mode = match tags.first() {
        Some(b'1') => 1,
        Some(b'3') => 2,
        Some(b'4') => 3,
        Some(b'5') => 4,
        Some(b'6') => 5,
        _ => return -1,
    };
    let mut n = sl.len();
    while n >= 1 {
        let full: Vec<u8> = tags[..n].to_vec();
        let cmp: Vec<u8> = if mode != 3 {
            let mut v = Vec::new();
            let mut i = 0;
            while i < n {
                if i < n - 1 {
                    while full[n - 1] != b'4' && full[i] == b'4' {
                        if full[i + 1] != b'4' {
                            break;
                        }
                        i += 1;
                        if i >= n - 1 {
                            break;
                        }
                    }
                }
                v.push(full[i]);
                i += 1;
            }
            v
        } else {
            full.clone()
        };
        // julie09 FUN_10040620: 28 patterns; from index 10 on the validator cases are the 2014 ones minus 2.
        let e09 = fe.era.is_2009();
        let paul = fe.era == Era::Paul09;
        let (lo, hi) = if paul {
            // paul09 FUN_10040030: no "14" shape (27 patterns).
            match (mode, cmp.len()) {
                (1, 6) => (0, 3),
                (1, 5) => (3, 7),
                (1, 4) => (7, 11),
                (1, 3) => (11, 15),
                (2, 4) => (15, 16),
                (2, 3) => (16, 17),
                (2, 2) => (17, 18),
                (3, 6) => (18, 19),
                (3, 4) => (19, 20),
                (3, 3) => (20, 22),
                (3, 2) => (22, 25),
                (4, 3) => (25, 26),
                (5, 2) => (26, PATTERNS.count()),
                _ => (0, 0),
            }
        } else if e09 {
            match (mode, cmp.len()) {
                (1, 6) => (0, 3),
                (1, 5) => (3, 7),
                (1, 4) => (7, 11),
                (1, 3) => (11, 15),
                (1, 2) => (15, 16),
                (2, 4) => (16, 17),
                (2, 3) => (17, 18),
                (2, 2) => (18, 19),
                (3, 6) => (19, 20),
                (3, 4) => (20, 21),
                (3, 3) => (21, 23),
                (3, 2) => (23, 26),
                (4, 3) => (26, 27),
                (5, 2) => (27, PATTERNS.count()),
                _ => (0, 0),
            }
        } else {
        match (mode, cmp.len()) {
            (1, 6) => (0, 3),
            (1, 5) => (3, 7),
            (1, 4) => (7, 13),
            (1, 3) => (13, 17),
            (1, 2) => (17, 18),
            (2, 4) => (18, 19),
            (2, 3) => (19, 20),
            (2, 2) => (20, 21),
            (3, 6) => (21, 22),
            (3, 4) => (22, 23),
            (3, 3) => (23, 25),
            (3, 2) => (25, 28),
            (4, 3) => (28, 29),
            (5, 2) => (29, PATTERNS.count()),
            _ => (0, 0),
        }
        };
        for idx in lo..hi {
            if strcmp(PATTERNS.get(idx), &cmp) != 0 {
                continue;
            }
            let last = &sl[n - 1];
            let mut consumed = last.end - off;
            if last.comma != 0 {
                consumed += last.comma;
            }
            let vi = if paul && idx >= 15 { idx + 1 } else { idx };
            if let Some(v) = validate(&sl[..n], if e09 && vi >= 10 { vi + 2 } else { vi }, flag) {
                if !emit(fe, wa, t, &v) {
                    return 0;
                }
                return consumed + city(fe, wa, t, p + consumed as usize, off + consumed, true);
            }
            break;
        }
        n -= 1;
    }
    -1
}
