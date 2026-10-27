//! `norm_date` (0x10030b30): dates around a month name, a weekday or digits with `/`, `-` or `.`
//! ("January fifth twenty oh four", "Wednesday", "the fifth of May", "nineteen ninety to ninety five",
//! "AD two thousand"). Modes `E`..`O` (0x45..0x4f) are the say-as date formats; they start from a fresh
//! look-ahead buffer.
//!
//! The engine's function is one large goto graph; `date_body` keeps its labels (`L101f` is
//! `LAB_1003101f`) so the port can be checked against the decompile block by block.

use super::mem::*;
use super::norm_units::is_sym;
use super::normalize::{Cx, State};
use super::num;
use super::rx::*;
use super::sbd;
use super::stream::{token_at, Ahead};
use super::tokenize::Tok;

const MONTHS: Rec30 = rec30(0x10072a50, 0x10084854);
const FULL_MONTHS: Rec30 = rec30(0x10072d80, 0x10084858);
const WEEKDAYS: Rec30 = rec30(0x10072fc0, 0x1008485c);
const WEEKDAY_NAMES: Ptab = ptab(0x100731a0, 0x10084860);
const MONTH_NAMES: Ptab = ptab(0x10072a18, 13);
/// Words that make a number nearby a date (`0x100731bc`, sorted, some of several words).
const DATE_WORDS: Ptab = ptab(0x100731bc, 0x10084864);

fn month_name(m: i32) -> &'static [u8] {
    if (0..13).contains(&m) {
        MONTH_NAMES.get(m)
    } else {
        b""
    }
}

/// `FUN_10034d30`: `1`..`12` or a month name.
fn month_of(t: &[u8]) -> i32 {
    let t = cs(t);
    let l = t.len();
    let v = atoi(t);
    if is_digit(at(t, 0)) && (1..=2).contains(&l) && (1..=12).contains(&v) {
        return v;
    }
    if is_alpha(at(t, 0)) {
        let k = MONTHS.find(t, true);
        if k >= 0 {
            return atoi(MONTHS.field(k, 0x10));
        }
        let k = FULL_MONTHS.find(t, true);
        if k >= 0 {
            return atoi(FULL_MONTHS.field(k, 0x10));
        }
    }
    -1
}

/// `FUN_10034c80`: a day `1`..`31` (any two digits but 0 in the say-as modes), also `5th`, `21st`.
fn day_of(t: &[u8], mode: i32) -> i32 {
    let t = cs(t);
    let l = t.len();
    let v = match l {
        1 | 2 => atoi(t),
        3 => {
            if !num::is_ordinal(t) {
                return -1;
            }
            return t[0] as i8 as i32 - 0x30;
        }
        4 => {
            if !num::is_ordinal(t) {
                return -1;
            }
            atoi(&t[..2])
        }
        _ => return -1,
    };
    if mode == 0 {
        if (1..=31).contains(&v) {
            v
        } else {
            -1
        }
    } else if (1..100).contains(&v) {
        v
    } else {
        -1
    }
}

/// `FUN_10034e90`: an ordinal day `1st`..`31st`.
fn day_ordinal(t: &[u8]) -> i32 {
    let t = cs(t);
    let l = t.len();
    if (1..=4).contains(&l) && num::is_ordinal(t) {
        let v = atoi(&t[..l - 2]);
        if (1..32).contains(&v) {
            return v;
        }
    }
    -1
}

/// `FUN_10034f00`: a year: `'NN`, digits of a plausible length (any digits in the say-as modes).
fn year_of(t: &[u8], mode: i32) -> i32 {
    let t = cs(t);
    let l = t.len();
    for (i, &c) in t.iter().enumerate() {
        if !is_digit(c) && (i != 0 || l < 2 || t[0] != b'\'') {
            return -1;
        }
    }
    if at(t, 0) == b'\'' {
        let v = atoi(&t[1..]);
        if l == 3 && v >= 0 {
            return if v > 99 { -1 } else { v };
        }
        return -1;
    }
    if 0x44 < mode && mode < 0x50 && num::all_digits(t) {
        return atoi(t);
    }
    let v = atoi(t);
    if l > 1 {
        let ok = match l {
            2 => (0..100).contains(&v),
            3 => (100..1000).contains(&v),
            4 => v > 999,
            _ => false,
        };
        if ok {
            return v;
        }
        if t[0] != b'0' && v > 9999 {
            return v;
        }
    }
    -1
}

/// `FUN_100353e0`: which orders three fields can be read in (1 M D Y, 2 D M Y, 4 Y M D).
fn orders(a: &[u8], b2: &[u8], c: &[u8]) -> u32 {
    let mut u = 0;
    if day_of(a, 0) >= 0 && month_of(b2) >= 0 && year_of(c, 0) >= 0 {
        u = 2;
    }
    if month_of(a) >= 0 && day_of(b2, 0) >= 0 && year_of(c, 0) >= 0 {
        u |= 1;
    }
    if year_of(a, 0) >= 0 && month_of(b2) >= 0 && day_of(c, 0) >= 0 {
        u |= 4;
    }
    u
}

/// `FUN_10035110`: three fields as (year, month, day).
fn three_fields(a: &[u8], b2: &[u8], c: &[u8], mode: i32) -> Option<(i32, i32, i32)> {
    let (d, m) = (day_ordinal(a), month_of(b2));
    if d >= 0 && m >= 0 {
        let y = year_of(c, mode);
        if y >= 0 {
            return Some((y, m, d));
        }
    }
    let d = day_ordinal(b2);
    if d >= 0 {
        let m = month_of(a);
        if m >= 0 {
            let y = year_of(c, mode);
            if y >= 0 {
                return Some((y, m, d));
            }
        }
    }
    let d = day_ordinal(c);
    if d >= 0 {
        let m = month_of(b2);
        if m >= 0 {
            let y = year_of(a, mode);
            if y >= 0 {
                return Some((y, m, d));
            }
        }
    }
    let y = num::year_value(a);
    if y >= 0 {
        let m = month_of(b2);
        if m >= 0 {
            let d = day_of(c, mode);
            if d >= 0 {
                return Some((y, m, d));
            }
        }
    }
    let y = num::year_value(c);
    if y >= 0 {
        let m = month_of(a);
        if m >= 0 {
            let d = day_of(b2, mode);
            if d >= 0 {
                return Some((y, m, d));
            }
        }
    }
    let y = num::year_value(c);
    if y >= 0 {
        let d = day_of(a, mode);
        if d >= 0 {
            let m = month_of(b2);
            if m >= 0 {
                return Some((y, m, d));
            }
        }
    }
    let u = orders(a, b2, c);
    let (b2s, b4s) = (u & 2 == 2, u & 4 == 4);
    if u & 1 != 1 && b2s && b4s {
        return Some((year_of(c, mode), month_of(b2), day_of(a, mode)));
    }
    if u != 1 {
        if u == 2 {
            return Some((year_of(c, mode), month_of(b2), day_of(a, mode)));
        }
        if u == 4 {
            return Some((year_of(a, mode), month_of(b2), day_of(c, mode)));
        }
        if u & 1 != 1 || !b2s {
            return None;
        }
    }
    Some((year_of(c, mode), month_of(a), day_of(b2, mode)))
}

/// `FUN_10035000`: three fields in the order the say-as mode names (`M` D/M/Y, `N` M/D/Y, `O` Y/M/D).
fn three_fields_fixed(a: &[u8], b2: &[u8], c: &[u8], mode: i32) -> Option<(i32, i32, i32)> {
    let (y, m, d) = match mode {
        0x4d => (year_of(c, mode), month_of(b2), day_of(a, mode)),
        0x4e => (year_of(c, mode), month_of(a), day_of(b2, mode)),
        0x4f => (year_of(a, mode), month_of(b2), day_of(c, mode)),
        _ => return None,
    };
    (y >= 0 && m >= 0 && d >= 0).then_some((y, m, d))
}

/// `FUN_10035940`: `a.b[.c]` into two or three fields (letters, or up to four digits with an ordinal
/// suffix), in the engine's 3 x 16-byte buffer.
fn split_dots(t: &[u8]) -> Option<(usize, [u8; 64])> {
    let t = cs(t);
    let mut out = [0u8; 64];
    let mut n = 0usize;
    let mut i8 = 0usize;
    let mut p = 0usize;
    let mut c = at(t, 0);
    while c != 0 {
        if n > 2 {
            return None;
        }
        if !is_alpha(c) {
            if !is_digit(c) {
                return None;
            }
            let mut last;
            loop {
                last = p;
                if i8 > 3 {
                    return None;
                }
                out[n * 16 + i8] = c;
                i8 += 1;
                p += 1;
                c = at(t, p as isize);
                if !is_digit(c) {
                    break;
                }
            }
            if c == 0 || c == b'.' {
                out[n * 16 + i8] = 0;
                n += 1;
                c = at(t, p as isize);
                i8 = 0;
                if c == b'.' {
                    p += 1;
                }
            } else {
                if i8 > 3 {
                    return None;
                }
                let d = out[n * 16 + i8 - 1];
                let suf = |s: &[u8]| strnicmp(&t[p..], s, 2) == 0;
                let ok = (d == b'1' && suf(b"st"))
                    || (d == b'2' && suf(b"nd"))
                    || (d == b'3' && suf(b"rd"))
                    || ((d == b'0' || (b'4'..=b'9').contains(&d)) && suf(b"th"));
                if !ok {
                    return None;
                }
                let a3 = at(t, last as isize + 3);
                if a3 == 0 || (a3 == b'.' && at(t, last as isize + 4) != 0) {
                    out[n * 16 + i8] = at(t, p as isize);
                    out[n * 16 + i8 + 1] = at(t, p as isize + 1);
                    out[n * 16 + i8 + 2] = 0;
                    n += 1;
                    i8 = 0;
                    p = if a3 == 0 { last + 3 } else { last + 4 };
                }
            }
        } else {
            loop {
                if i8 > 0xf {
                    return None;
                }
                out[n * 16 + i8] = c;
                i8 += 1;
                p += 1;
                c = at(t, p as isize);
                if !is_alpha(c) {
                    break;
                }
            }
            if c != 0 && c != b'.' {
                return None;
            }
            out[n * 16 + i8] = 0;
            n += 1;
            c = at(t, p as isize);
            i8 = 0;
            if c == b'.' {
                p += 1;
            }
        }
        c = at(t, p as isize);
    }
    if n != 1 {
        Some((n, out))
    } else {
        None
    }
}

/// The date-word part of `FUN_10035490` for token `k` of `toks`: a month (case-sensitive) or an entry of
/// `DATE_WORDS` starting with it (the next tokens must spell the rest).
fn date_word_at(toks: &[Tok], k: usize) -> bool {
    let w = &toks[k];
    if w.ty == 1 && (MONTHS.find(&w.text, false) >= 0 || FULL_MONTHS.find(&w.text, false) >= 0) {
        return true;
    }
    for j in 0..DATE_WORDS.count() {
        let e = DATE_WORDS.get(j);
        let e0 = at(e, 0) as i8 as i32;
        let f = fold_s(w.text[0]);
        if f > e0 {
            continue;
        }
        if f < e0 {
            break;
        }
        let c = strnicmp(e, &w.text, w.len.max(0) as usize);
        if c == 0 {
            let rest_at = w.len.max(0) as usize;
            match at(e, rest_at as isize) {
                0 => return true,
                b' ' => {}
                _ => break,
            }
            let rest = &e[rest_at + 1..];
            let mut i = 0usize;
            for t2 in &toks[k + 1..] {
                if fold_s(t2.text[0]) != at(rest, i as isize) as i8 as i32 || strnicmp(&rest[i.min(rest.len())..], &t2.text, t2.len.max(0) as usize) != 0 {
                    break;
                }
                match at(rest, (i + t2.len.max(0) as usize) as isize) {
                    0 => return true,
                    b' ' => i += 1 + t2.len.max(0) as usize,
                    _ => break,
                }
            }
        } else if c > 0 {
            break;
        }
    }
    false
}

/// `FUN_10035490`: "on" or a date word among the last consumed tokens (`L`), and for `B` also among the
/// next three tokens.
fn date_context(cx: &Cx, st: &State, cur: usize, what: u8) -> bool {
    let p = &st.done.toks;
    let n = p.len();
    if n > 0 && (what == b'L' || what == b'B') {
        if stricmp(&p[n - 1].text, b"on") == 0 {
            return true;
        }
        let from = if n < 3 { 0 } else { n - 2 };
        for k in from..n {
            if date_word_at(p, k) {
                return true;
            }
        }
        if what == b'L' {
            return false;
        }
    }
    if what == b'R' || what == b'B' {
        let mut a = Ahead::default();
        let mut c = cur;
        loop {
            let Some(i) = a.push(cx.t, &mut c, cur, b'D', cx.fe.ud.as_ref()) else { return false };
            let ty = a.toks[i].ty;
            if ty == 8 || ty == 9 || a.n >= 3 {
                break;
            }
        }
        let toks: Vec<Tok> = a.toks[..a.n as usize].to_vec();
        for k in 0..toks.len() {
            if date_word_at(&toks, k) {
                return true;
            }
        }
    }
    false
}

/// `FUN_10042320`.
fn emit_month(st: &mut State, m: i32, s: i32, e: i32, c18: u8) -> i32 {
    st.wa.emit(month_name(m), s, e, c18, b'D')
}

/// `FUN_100422d0`.
fn emit_ordinal(st: &mut State, d: i32, s: i32, e: i32) -> i32 {
    let o = num::ordinal(d.to_string().as_bytes());
    st.wa.emit_words(&o, s, e, b'D', b'D')
}

/// `FUN_10042350`.
fn emit_year(st: &mut State, y: i32, s: i32, e: i32) -> i32 {
    st.wa.emit_words(&num::year(y), s, e, b'D', b'D')
}

/// `FUN_10042270`: month, day, year.
fn emit_mdy(st: &mut State, y: i32, m: i32, d: i32, s: i32, e: i32, c18: u8) -> i32 {
    let r = emit_month(st, m, s, e, c18);
    if r == 3 {
        return r;
    }
    let r = emit_ordinal(st, d, s, e);
    if r == 3 {
        return r;
    }
    emit_year(st, y, s, e)
}

/// `FUN_10042140`: an ordinal, a number, or the word itself.
fn emit_any(st: &mut State, t: &[u8], s: i32, e: i32) -> i32 {
    if num::is_ordinal(t) {
        let o = num::ordinal(t);
        return st.wa.emit_words(&o, s, e, b'D', b'D');
    }
    if num::numberish(t) {
        let c = num::cardinal(t, 0);
        return st.wa.emit_words(&c, s, e, b'D', b'D');
    }
    st.wa.emit_words(t, s, e, b'A', b'D')
}

struct D<'a, 'b> {
    cx: &'a Cx<'b>,
    st: &'a mut State,
    cur: &'a mut usize,
    sv: Save,
    mode: i32,
    tok: Tok,
    start: i32,
}

impl D<'_, '_> {
    fn push(&mut self) -> Option<Tok> {
        let t = push(self.cx, self.st, self.cur)?;
        self.tok = t;
        Some(t)
    }

    fn pop(&mut self) {
        self.tok = pop(self.st, self.cur);
    }

    fn seek(&mut self, d: i32) {
        self.tok = seek(self.st, self.cur, d);
    }

    /// `FUN_10035b40`: a number next to a month: year, day before or after.
    #[allow(clippy::too_many_arguments)]
    fn month_and(&mut self, t3: &Tok, month: i32, s: i32, e: i32, cur: usize, mode: i32, p9: bool, p10: bool, p11: bool) -> i32 {
        let mut year = -1;
        let mut day = -1;
        let mut b1 = false;
        enum B {
            C0b,
            D45,
            D4a,
            D97,
            Cc1,
            D07,
            D37,
            Cdc,
        }
        let mut at = B::C0b;
        let mut go = true;
        if p10 {
            if mode == 0x49 {
                year = year_of(&t3.text, 0x49);
                if year >= 0 {
                    at = B::D4a;
                    go = false;
                }
            }
            if go {
                if mode == 0x4a {
                    day = day_of(&t3.text, 0x4a);
                    if day >= 0 {
                        b1 = true;
                        at = B::D45;
                    }
                }
                go = false;
            }
        }
        if go {
            if mode == 0x4c {
                year = year_of(&t3.text, 0x4c);
                at = B::Cdc;
            } else if mode == 0x4b {
                day = day_of(&t3.text, 0x4b);
                if day < 0 {
                    return 5;
                }
                at = B::D97;
            }
        }
        loop {
            at = match at {
                B::Cdc => {
                    if year < 0 {
                        return 5;
                    }
                    B::D4a
                }
                B::C0b => {
                    if mode == 0x45 {
                        if p10 {
                            day = day_of(&t3.text, 0x45);
                            if day >= 0 {
                                B::Cc1
                            } else {
                                year = year_of(&t3.text, 0x45);
                                B::D07
                            }
                        } else {
                            year = year_of(&t3.text, 0x45);
                            if year >= 0 {
                                B::D4a
                            } else {
                                day = day_of(&t3.text, 0x45);
                                B::D37
                            }
                        }
                    } else {
                        if mode != 0 {
                            return 5;
                        }
                        let mut common = true;
                        let mut next = B::D45;
                        if !p9 && !date_context(self.cx, self.st, cur, b'B') {
                            if !p10 {
                                year = num::year_value(&t3.text);
                                next = B::Cdc;
                                common = false;
                            } else if !p11 {
                                day = day_of(&t3.text, 0);
                                if day < 0 {
                                    return 5;
                                }
                                next = B::Cc1;
                                common = false;
                            }
                        } else if !p10 {
                            year = num::year_value(&t3.text);
                            if year >= 0 {
                                next = B::D4a;
                            } else {
                                day = day_of(&t3.text, 0);
                                next = B::D37;
                            }
                            common = false;
                        }
                        if common {
                            day = day_of(&t3.text, 0);
                            if day >= 0 {
                                B::Cc1
                            } else {
                                year = num::year_value(&t3.text);
                                B::D07
                            }
                        } else {
                            next
                        }
                    }
                }
                B::D37 => {
                    if day < 0 {
                        return 5;
                    }
                    b1 = false;
                    B::D45
                }
                B::Cc1 => {
                    b1 = true;
                    B::D45
                }
                B::D07 => {
                    if year < 0 {
                        return 5;
                    }
                    B::D45
                }
                B::D45 => {
                    if year >= 0 {
                        B::D4a
                    } else if !b1 {
                        let r = emit_ordinal(self.st, day, s, e);
                        if r != 1 {
                            return r;
                        }
                        return emit_month(self.st, month, s, e, b'A');
                    } else {
                        B::D97
                    }
                }
                B::D97 => {
                    let r = emit_month(self.st, month, s, e, b'A');
                    if r != 1 {
                        return r;
                    }
                    return emit_ordinal(self.st, day, s, e);
                }
                B::D4a => {
                    let r = emit_month(self.st, month, s, e, b'A');
                    if r != 1 {
                        return r;
                    }
                    return emit_year(self.st, year, s, e);
                }
            };
        }
    }

    /// `FUN_10035e20`: a month with two numbers (`t5` before the month or after it, `self.tok` last).
    #[allow(clippy::too_many_arguments)]
    fn month_two(&mut self, t4: &Tok, t5: &Tok, month: i32, s: i32, e: i32, mode: i32, p12: bool, p13: bool, p14: bool, p15: bool) -> i32 {
        let (mut start, mut end, mut mode) = (s, e, mode);
        let mut year = -1;
        let mut day = -1;
        let (mut b2, mut b3) = (false, false);
        let mut lc = 5;
        let mut s4 = 5;
        let mut s11 = 5;
        enum E {
            Ea8,
            E86,
            Fc1,
            Fdc,
            Six359,
            Six05c,
            J7f,
            Six306,
            Six30a,
            End,
        }
        let p5 = t5.text;
        let mut at = if !p13 {
            if mode != 0x4d {
                if mode != 0x4f {
                    E::Ea8
                } else {
                    year = year_of(&p5, 0x4f);
                    if year < 0 {
                        return 5;
                    }
                    s11 = 0x4f;
                    E::Six05c
                }
            } else {
                E::E86
            }
        } else if mode == 0x4e {
            E::E86
        } else {
            E::Ea8
        };
        loop {
            at = match at {
                E::Six05c => {
                    day = day_of(&self.tok.text, s11);
                    E::J7f
                }
                E::J7f => {
                    if day < 0 {
                        return 5;
                    }
                    E::Six306
                }
                E::E86 => {
                    day = day_of(&p5, mode);
                    if day < 0 {
                        return 5;
                    }
                    E::Fc1
                }
                E::Fc1 => {
                    year = year_of(&self.tok.text, mode);
                    if year < 0 {
                        return 5;
                    }
                    E::Fdc
                }
                E::Fdc => {
                    if day >= 0 {
                        let r = emit_mdy(self.st, year, month, day, start, end, b'A');
                        return if r != 1 { r } else { 1 };
                    }
                    if !b2 {
                        E::Six359
                    } else {
                        lc = emit_year(self.st, year, start, end);
                        if lc == 1 {
                            lc = emit_month(self.st, month, start, end, b'A');
                        }
                        return lc;
                    }
                }
                E::Six359 => {
                    let r = emit_month(self.st, month, start, end, b'A');
                    if r != 1 {
                        return r;
                    }
                    return emit_year(self.st, year, start, end);
                }
                E::Six306 => {
                    lc = s4;
                    b2 = false;
                    E::Six30a
                }
                E::Six30a => {
                    if year >= 0 {
                        E::Fdc
                    } else {
                        E::End
                    }
                }
                E::Ea8 => {
                    if mode == 0x45 {
                        if !p13 {
                            let y = num::year_value(&p5);
                            if y >= 0 {
                                year = y;
                                E::Six359
                            } else {
                                let d = day_ordinal(&self.tok.text);
                                let d = if d >= 0 { d } else { day_ordinal(&p5) };
                                day = d;
                                if d >= 0 {
                                    E::Six306
                                } else {
                                    year = num::year_value(&self.tok.text);
                                    if year >= 0 {
                                        E::Fdc
                                    } else {
                                        day = day_of(&p5, 0x45);
                                        if day >= 0 {
                                            year = year_of(&self.tok.text, 0x45);
                                        }
                                        if day >= 0 && year >= 0 {
                                            E::Fdc
                                        } else {
                                            year = year_of(&p5, 0x45);
                                            if year < 0 {
                                                return 5;
                                            }
                                            day = day_of(&self.tok.text, 0x45);
                                            E::J7f
                                        }
                                    }
                                }
                            }
                        } else {
                            day = day_of(&p5, 0x45);
                            if day < 0 {
                                return 5;
                            }
                            mode = 0x45;
                            E::Fc1
                        }
                    } else {
                        if mode != 0 {
                            return 5;
                        }
                        if p12 {
                            if !p13 {
                                day = day_of(&p5, 0);
                                if day >= 0 {
                                    year = year_of(&self.tok.text, 0);
                                }
                                if day < 0 || year < 0 {
                                    year = year_of(&p5, 0);
                                    if year < 0 {
                                        return 5;
                                    }
                                    s11 = 0;
                                    E::Six05c
                                } else {
                                    E::Fdc
                                }
                            } else {
                                day = day_of(&p5, 0);
                                if day < 0 {
                                    return 5;
                                }
                                mode = 0;
                                E::Fc1
                            }
                        } else if !p13 {
                            day = day_of(&p5, 0);
                            if day >= 0 {
                                year = num::year_value(&self.tok.text);
                            }
                            if day >= 0 && year >= 0 {
                                E::Fdc
                            } else {
                                year = num::year_value(&p5);
                                if year >= 0 {
                                    day = day_of(&self.tok.text, 0);
                                }
                                if year >= 0 && day >= 0 {
                                    E::Six306
                                } else {
                                    year = num::year_value(&p5);
                                    if year >= 0 {
                                        day = day_of(&self.tok.text, 0);
                                    }
                                    if year >= 0 && day < 0 {
                                        self.seek(if p14 { -2 } else { -1 });
                                        b2 = true;
                                        end = self.tok.end;
                                        lc = s11;
                                        E::Six30a
                                    } else {
                                        day = day_of(&p5, 0);
                                        if day < 0 {
                                            year = num::year_value(&self.tok.text);
                                        }
                                        if day >= 0 || year < 0 {
                                            let ctx = date_context(self.cx, self.st, *self.cur, b'B');
                                            let mut ok = ctx;
                                            if ok {
                                                day = day_of(&p5, 0);
                                                ok = day >= 0;
                                            }
                                            if ok {
                                                year = year_of(&self.tok.text, 0);
                                                ok = year >= 0;
                                            }
                                            if !ok {
                                                year = year_of(&p5, 0);
                                                if year < 0 {
                                                    return 5;
                                                }
                                                day = day_of(&self.tok.text, 0);
                                                E::J7f
                                            } else {
                                                E::Fdc
                                            }
                                        } else {
                                            let r = emit_any(self.st, &p5, t5.start, t5.end);
                                            if r != 1 {
                                                return r;
                                            }
                                            start = t4.start;
                                            s4 = 1;
                                            E::Six306
                                        }
                                    }
                                }
                            }
                        } else {
                            day = day_of(&p5, 0);
                            if day >= 0 {
                                year = num::year_value(&self.tok.text);
                            }
                            if day >= 0 && year >= 0 {
                                E::Fdc
                            } else {
                                self.pop();
                                end = self.tok.end;
                                let mut need = true;
                                if p15 {
                                    day = day_of(&p5, 0);
                                    if day < 0 {
                                        year = num::year_value(&p5);
                                        if year >= 0 {
                                            need = false;
                                        }
                                    } else {
                                        need = false;
                                    }
                                }
                                if need {
                                    day = day_ordinal(&p5);
                                    if day < 0 {
                                        if !date_context(self.cx, self.st, *self.cur, b'B') {
                                            return 5;
                                        }
                                        day = day_of(&p5, 0);
                                        if day < 0 {
                                            return 5;
                                        }
                                    }
                                }
                                b3 = true;
                                lc = s11;
                                if p15 {
                                    self.pop();
                                    end = self.tok.end;
                                }
                                E::Six30a
                            }
                        }
                    }
                }
                E::End => {
                    if year >= 0 || day < 0 {
                        return lc;
                    }
                    if b3 {
                        let r = emit_month(self.st, month, start, end, b'A');
                        if r != 1 {
                            return r;
                        }
                        return emit_ordinal(self.st, day, start, end);
                    }
                    lc = emit_ordinal(self.st, day, start, end);
                    if lc == 1 {
                        lc = emit_month(self.st, month, start, end, b'A');
                    }
                    return lc;
                }
            };
        }
    }
}

pub fn date(cx: &Cx, st: &mut State, cur: &mut usize, _t: &Tok) -> i32 {
    date_mode(cx, st, cur, 0)
}

pub fn date_mode(cx: &Cx, st: &mut State, cur: &mut usize, mode: i32) -> i32 {
    let first;
    if mode == 0 {
        first = cur_tok(st);
    } else {
        if !(0x45..=0x4f).contains(&mode) {
            return 5;
        }
        st.ahead.reset();
        let Some(t) = push(cx, st, cur) else { return 5 };
        first = t;
    }
    let sv = save(st);
    let mut d = D { cx, st, cur, sv, mode, tok: first, start: first.start };
    date_body(&mut d)
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum L {
    C93,
    D64,
    J196a,
    J27f0,
    E2ae1,
    L1013,
    L2af0,
    L326e,
    L3504,
    L3452,
    L34ac,
    L34d1,
    L34e2,
    L3472,
    L359e,
    L3209,
    L356d,
    L101f,
    L101fTail,
    L3901,
    L3f98,
    L37a3,
    L3e69,
    L3eed,
    L264f,
    L42c5,
    L126e,
    L1266,
    L127d,
    L4ab8,
    L4b00,
    L4b5e,
    E83,
    L19ca,
    L1c5e,
    L1cf7,
    L1226,
    L1251,
    L1503,
    L15db,
    L15f6,
    L15e5,
    L170e,
    L134b,
    L277d,
    L2786,
    L1920,
    L192b,
    L22c8,
    L2421,
    L251f,
    L27e1,
    /// Return the status in `s3` as it is.
    Ret,
    /// `Jan.` closed the sentence (2).
    Ret2,
}

/// The variables of `FUN_10030b30` that cross labels.
struct V {
    s3: i32,
    s11: i32,
    s12: i32,
    l160: i32,
    month: i32,
    day: i32,
    year: i32,
    i13c: i32,
    t13c: Tok,
    t144: Tok,
    t154: Tok,
    f154: i32,
    sep: u8,
    i5: i32,
    pb6: Tok,
    i16: usize,
    c18: u8,
    q144: i32,
    pb6i: i32,
}

fn date_body(d: &mut D) -> i32 {
    let mode = d.mode;
    let mut v = V {
        s3: 5,
        s11: 5,
        s12: 5,
        l160: 0,
        month: -1,
        day: -1,
        year: -1,
        i13c: -1,
        t13c: Tok::default(),
        t144: Tok::default(),
        t154: Tok::default(),
        f154: 0,
        sep: 0,
        i5: 0,
        pb6: Tok::default(),
        i16: 0,
        c18: b'A',
        q144: 0,
        pb6i: 0,
    };
    let prev = token_at(&d.st.done, &d.st.ahead, d.st.ahead.n - 1, -1);
    let t = d.tok;
    let sepsym = |t: &Tok| (t.ty == 3 || t.ty == 7) && t.len == 1 && (t.text[0] == b'/' || t.text[0] == b'-');
    let mut at = match prev {
        None => {
            if t.ty == 2 && num::day_like(&t.text) {
                L::D64
            } else {
                L::C93
            }
        }
        Some(p) => {
            if t.ty == 2 && num::day_like(&t.text) && t.ws >= 1 {
                L::D64
            } else if p.ty == 3 && !(p.len == 1 && (p.text[0] == b'/' || p.text[0] == b'-')) && t.ty == 2 && num::day_like(&t.text) {
                L::D64
            } else {
                L::C93
            }
        }
    };
    macro_rules! push_or {
        ($l:expr) => {
            match d.push() {
                Some(t) => t,
                None => {
                    at = $l;
                    continue;
                }
            }
        };
    }
    loop {
        at = match at {
            L::C93 => {
                if d.tok.ty != 1 {
                    L::L359e
                } else {
                    let k = FULL_MONTHS.find(&d.tok.text, true);
                    if k >= 0 {
                        v.month = atoi(FULL_MONTHS.field(k, 0x10));
                        v.t13c = d.tok;
                        let t = push_or!(L::E83);
                        if t.ty == 2 && num::day_like(&t.text) {
                            v.t144 = t;
                            let t = push_or!(L::E83);
                            if t.ws < 3 {
                                if !is_sym(&t, b',') {
                                    if !(t.len == 1 && t.text[0] != 0 && b"-~".contains(&t.text[0])) {
                                        L::L2af0
                                    } else {
                                        push_or!(L::L4ab8);
                                        match range(d, &mut v) {
                                            Some(l) => l,
                                            None => {
                                                if mode != 0 {
                                                    L::L101f
                                                } else {
                                                    d.seek(-2);
                                                    let t144 = v.t144;
                                                    v.s3 = d.month_and(&t144, v.month, d.start, d.tok.end, *d.cur, 0, false, true, true);
                                                    L::E2ae1
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    let t = push_or!(L::L3e69);
                                    if t.ty == 2 && num::day_like(&t.text) {
                                        v.s12 = 1;
                                        v.s11 = 1;
                                        v.i5 = t.end;
                                        v.s3 = 0;
                                        L::L3472
                                    } else if mode != 0 {
                                        L::L101f
                                    } else {
                                        d.seek(-2);
                                        v.s12 = 1;
                                        v.i5 = d.tok.end;
                                        v.s3 = 0;
                                        L::L34d1
                                    }
                                }
                            } else {
                                L::L2af0
                            }
                        } else if t.ws < 3 && sepsym(&t) {
                            v.sep = t.text[0];
                            let t = push_or!(L::E83);
                            if t.ws > 2 || !num::day_like(&t.text) {
                                L::L101f
                            } else {
                                v.t144 = t;
                                let t = push_or!(L::E83);
                                if t.ws > 2 || !(t.ty == 3 || t.ty == 7) || t.len != 1 || t.text[0] != v.sep {
                                    L::L3504
                                } else {
                                    let t = push_or!(L::L4ab8);
                                    if t.ws < 3 {
                                        L::L3452
                                    } else {
                                        L::L34ac
                                    }
                                }
                            }
                        } else if mode == 0x47 || mode == 0x45 {
                            d.pop();
                            v.s3 = emit_month(d.st, v.month, d.start, d.tok.end, b'A');
                            L::J196a
                        } else {
                            L::L101f
                        }
                    } else {
                        let k = MONTHS.find(&d.tok.text, true);
                        if k < 0 {
                            L::L359e
                        } else {
                            abbreviated(d, &mut v, k)
                        }
                    }
                }
            }
            L::L2af0 => {
                if d.tok.ty == 2 && num::day_like(&d.tok.text) {
                    let (t13c, t144) = (v.t13c, v.t144);
                    let e = d.tok.end;
                    v.s3 = d.month_two(&t13c, &t144, v.month, d.start, e, mode, false, true, false, false);
                    L::J196a
                } else {
                    L::L326e
                }
            }
            L::L326e => {
                d.pop();
                let t = d.tok;
                v.s3 = d.month_and(&t, v.month, d.start, t.end, *d.cur, mode, false, true, false);
                L::J196a
            }
            L::L3504 => {
                d.pop();
                let t = d.tok;
                v.s3 = d.month_and(&t, v.month, d.start, t.end, *d.cur, mode, true, true, false);
                L::J196a
            }
            L::L3452 => {
                if num::day_like(&d.tok.text) {
                    v.s12 = 0;
                    v.s11 = 1;
                    v.i5 = d.tok.end;
                    v.s3 = 1;
                    L::L3472
                } else {
                    L::L34ac
                }
            }
            L::L34ac => {
                if mode == 0 {
                    d.seek(-2);
                    v.s12 = 0;
                    v.i5 = d.tok.end;
                    v.s3 = 1;
                    L::L34d1
                } else {
                    L::L101f
                }
            }
            L::L34d1 => {
                v.s11 = 1;
                v.i16 = *d.cur;
                v.pb6 = d.tok;
                L::L34e2
            }
            L::L34e2 => {
                let pb6 = v.pb6;
                v.s3 = d.month_and(&pb6, v.month, d.start, v.i5, v.i16, 0, v.s3 != 0, v.s11 != 0, v.s12 != 0);
                L::J27f0
            }
            L::L3472 => {
                let (t13c, t144) = (v.t13c, v.t144);
                v.s3 = d.month_two(&t13c, &t144, v.month, d.start, v.i5, mode, v.s3 != 0, v.s11 != 0, false, v.s12 != 0);
                L::J196a
            }
            L::L3209 => {
                if d.tok.ws < 3 && d.tok.ty == 2 && num::day_like(&d.tok.text) {
                    let (t13c, t144) = (v.t13c, v.t144);
                    let e = d.tok.end;
                    v.s3 = d.month_two(&t13c, &t144, v.month, d.start, e, mode, false, true, false, false);
                    L::J196a
                } else {
                    L::L326e
                }
            }
            L::L356d => {
                d.pop();
                v.s3 = emit_month(d.st, v.month, d.start, v.t13c.end, b'A');
                L::J196a
            }
            L::J196a => {
                if v.s3 == 1 {
                    L::L1266
                } else {
                    L::L101f
                }
            }
            L::J27f0 => {
                if v.s3 == 1 {
                    L::L4b00
                } else {
                    L::L101f
                }
            }
            L::E2ae1 => {
                if v.s3 == 1 {
                    v.l160 = 1;
                }
                d.seek(1);
                L::L1013
            }
            L::L1013 => {
                if v.l160 as i16 == 0 {
                    L::L101f
                } else {
                    L::L126e
                }
            }
            L::L101f => digits_with_dots(d, &mut v),
            L::L101fTail => {
                d.pop();
                if push(d.cx, d.st, d.cur).is_some() {
                    L::L4b5e
                } else {
                    restore_tp(d);
                    L::E83
                }
            }
            L::L359e => {
                if mode == 0 {
                    if d.tok.ty == 1 && 1 < d.tok.len && d.tok.len < 4 {
                        if [&b"BC"[..], b"B.C", b"AD", b"A.D"].iter().all(|x| strcmp(&d.tok.text, x) != 0) {
                            L::L3901
                        } else {
                            L::L37a3
                        }
                    } else {
                        L::L3901
                    }
                } else {
                    let era = (mode == 0x45 || mode == 0x48)
                        && d.tok.ty == 1
                        && (2..=3).contains(&d.tok.len)
                        && [&b"BC"[..], b"B.C", b"AD", b"A.D"].iter().any(|x| stricmp(x, &d.tok.text) == 0);
                    if era {
                        L::L37a3
                    } else if mode == 0x45 {
                        L::L3901
                    } else {
                        L::L3f98
                    }
                }
            }
            L::L3901 => {
                if cls(d.tok.text[0]) & 0x80 == 0 {
                    L::L3f98
                } else {
                    weekday(d, &mut v)
                }
            }
            L::L3f98 => the_nth(d, &mut v),
            L::L37a3 => {
                v.t13c = d.tok;
                let t = push_or!(L::E83);
                let mut ok = true;
                if t.ws == 0 && t.len <= 2 && t.text[0] == b'.' {
                    ok = d.push().is_some();
                }
                if !ok {
                    L::L3e69
                } else {
                    let t = d.tok;
                    let y = if t.ty == 2 { year_of(&t.text, mode) } else { -1 };
                    if y >= 0 {
                        v.year = y;
                        v.s3 = d.st.wa.emit(v.t13c.text(), d.start, t.end, b'A', b'S');
                        if v.s3 == 1 {
                            let text = if y < 100 { num::cardinal(&t.text, 1) } else { num::year(y) };
                            v.s3 = d.st.wa.emit_words(&text, d.start, t.end, b'D', b'D');
                            L::J196a
                        } else {
                            L::L101f
                        }
                    } else {
                        L::L101f
                    }
                }
            }
            L::L3e69 => {
                restore_tp(d);
                L::E83
            }
            L::L4ab8 => {
                restore_tp(d);
                L::E83
            }
            L::E83 => {
                return restore(d.st, d.cur, d.sv, 4);
            }
            L::L3eed => {
                if v.i13c >= 0 {
                    d.st.q.set_n(v.q144);
                    d.st.ahead.n = v.l160;
                    if let Some(p) = d.st.ahead.pos[v.l160.max(0) as usize] {
                        *d.cur = p;
                    }
                    d.tok = d.st.ahead.tok(d.st.ahead.n as isize - 1);
                    let t = d.tok;
                    if cls(t.text[0]) & 0x80 != 0 && ((v.i13c != 3 && v.i13c != 9) || date_context(d.cx, d.st, *d.cur, b'B')) {
                        v.s3 = d.st.wa.emit(WEEKDAYS.field(v.i13c, 0x10), t.start, t.end, b'A', b'D');
                        L::J196a
                    } else {
                        L::L101f
                    }
                } else {
                    L::L101f
                }
            }
            L::L264f => {
                if d.tok.ws == 0 && d.tok.len < 3 && d.tok.text[0] == b'.' {
                    L::L126e
                } else {
                    L::L42c5
                }
            }
            L::L42c5 => {
                d.pop();
                L::L126e
            }
            L::L1266 => {
                v.l160 = 1;
                L::L126e
            }
            L::L126e => {
                if mode == 0 {
                    if v.l160 as i16 != 0 {
                        L::L4b00
                    } else {
                        L::L4b5e
                    }
                } else {
                    L::L127d
                }
            }
            L::L127d => {
                match d.push() {
                    None => {
                        restore_tp(d);
                        L::E83
                    }
                    Some(t) => {
                        let mut gone = false;
                        if t.ty == 8 && t.text[0] == 0 && d.push().is_none() {
                            gone = true;
                        }
                        if gone {
                            L::L4ab8
                        } else if d.tok.ty == 5 || d.tok.ty == 9 {
                            L::L4b00
                        } else {
                            L::L4b5e
                        }
                    }
                }
            }
            L::L4b00 => {
                let r = d.st.done.append_all(&d.st.ahead, 8);
                if r == 1 {
                    return 1;
                }
                return restore(d.st, d.cur, d.sv, r);
            }
            L::L4b5e => {
                let sv = d.sv;
                d.st.wa.set_n(sv.wn);
                if sv.wn > 0 {
                    let e = d.st.wa.ri32(sv.wn - 1, 4);
                    d.st.wa.set_consumed(e);
                }
                if d.st.ahead.n != sv.tn {
                    d.st.ahead.n = sv.tn;
                    if let Some(p) = d.st.ahead.pos[sv.tn as usize] {
                        *d.cur = p;
                    }
                }
                return v.s3;
            }
            L::D64 => day_first(d, &mut v),
            L::L19ca => {
                let k = if d.tok.ty == 1 { MONTHS.find(&d.tok.text, true) } else { -1 };
                if k >= 0 {
                    v.month = atoi(MONTHS.field(k, 0x10));
                    v.t13c = d.tok;
                    let t = push_or!(L::E83);
                    if !(t.ws == 0 && t.len <= 2 && t.text[0] == b'.') {
                        if t.ws < 3 && (t.ty == 3 || t.ty == 7) && t.len == 1 && t.text[0] == v.sep {
                            let t = push_or!(L::L4ab8);
                            if t.ty == 2 && num::day_like(&t.text) {
                                L::L1920
                            } else {
                                L::L1c5e
                            }
                        } else {
                            d.pop();
                            let t144 = v.t144;
                            v.s3 = d.month_and(&t144, v.month, d.start, d.tok.end, *d.cur, mode, true, false, false);
                            L::J196a
                        }
                    } else {
                        let t = push_or!(L::L3e69);
                        if t.ws > 2 || !(t.ty == 3 || t.ty == 7) || t.len != 1 || t.text[0] != v.sep {
                            d.seek(if mode == 0 { -2 } else { -1 });
                            let t144 = v.t144;
                            v.s3 = d.month_and(&t144, v.month, d.start, d.tok.end, *d.cur, mode, true, false, false);
                            L::J196a
                        } else {
                            let t = push_or!(L::L3e69);
                            if t.ty != 2 || !num::day_like(&t.text) {
                                if mode == 0 {
                                    d.seek(-2);
                                    let t144 = v.t144;
                                    v.s3 = d.month_and(&t144, v.month, d.start, d.tok.end, *d.cur, 0, true, false, false);
                                    if v.s3 == 1 {
                                        d.seek(1);
                                        L::L4b00
                                    } else {
                                        L::L101f
                                    }
                                } else {
                                    L::L101f
                                }
                            } else {
                                v.s3 = 1;
                                v.i5 = t.end;
                                L::L192b
                            }
                        }
                    }
                } else {
                    L::L1cf7
                }
            }
            L::L1c5e => {
                if mode != 0 {
                    L::L101f
                } else {
                    d.seek(-2);
                    let t144 = v.t144;
                    v.s3 = d.month_and(&t144, v.month, d.start, d.tok.end, *d.cur, 0, true, false, false);
                    L::J27f0
                }
            }
            L::L1cf7 => {
                let t = d.tok;
                if t.ws > 2 || !is_sym(&t, b'\'') {
                    L::L101f
                } else {
                    let t = push_or!(L::E83);
                    let y = if t.ws == 0 && t.len == 2 && t.ty == 2 { year_of(&t.text, mode) } else { -1 };
                    let m = if y >= 0 && (mode == 0 || mode == 0x49 || mode == 0x45) { month_of(&v.t144.text) } else { -1 };
                    if y < 0 || !(mode == 0 || mode == 0x49 || mode == 0x45) || m < 0 {
                        L::L101f
                    } else {
                        v.year = y;
                        v.month = m;
                        v.c18 = b'A';
                        v.l160 = t.end;
                        L::L1226
                    }
                }
            }
            L::L1226 => {
                v.s3 = emit_month(d.st, v.month, d.start, v.l160, v.c18);
                if v.s3 != 1 {
                    L::L101f
                } else {
                    L::L1251
                }
            }
            L::L1251 => {
                v.s3 = emit_year(d.st, v.year, d.start, v.l160);
                L::J196a
            }
            L::L1920 => {
                v.s3 = 0;
                v.i5 = d.tok.end;
                L::L192b
            }
            L::L192b => {
                let (t13c, t144) = (v.t13c, v.t144);
                v.s3 = d.month_two(&t13c, &t144, v.month, d.start, v.i5, mode, true, false, v.s3 != 0, false);
                L::J196a
            }
            L::L1503 => {
                let t144 = v.t144;
                let t = d.tok;
                if v.sep == b'-' && 2 < t144.len && t144.len < 5 && t.len == 4 {
                    v.year = year_of(&t144.text, mode);
                    v.i13c = if v.year >= 0 { year_of(&t.text, mode) } else { -1 };
                    if v.year < 0 || v.i13c < 0 || v.i13c <= v.year {
                        L::L15f6
                    } else {
                        v.s3 = emit_year(d.st, v.year, d.start, v.l160);
                        if v.s3 == 1 {
                            v.s3 = d.st.wa.emit(b"to", d.start, v.l160, b'S', b'D');
                            v.pb6i = v.i13c;
                            if v.s3 != 1 {
                                L::L170e
                            } else {
                                L::L15db
                            }
                        } else {
                            L::L170e
                        }
                    }
                } else {
                    L::L15f6
                }
            }
            L::L15db => {
                v.s3 = emit_year(d.st, v.pb6i, d.start, v.l160);
                L::L15e5
            }
            L::L15e5 => {
                if v.s3 == 1 {
                    L::L1266
                } else {
                    L::L170e
                }
            }
            L::L15f6 => {
                let t144 = v.t144;
                let t = d.tok;
                v.month = month_of(&t144.text);
                if v.month < 0 {
                    v.year = num::year_value(&t144.text);
                    v.s3 = v.s11;
                    if v.year >= 0 {
                        v.month = month_of(&t.text);
                    }
                    if v.year >= 0 && v.month >= 0 {
                        v.s3 = emit_year(d.st, v.year, d.start, v.l160);
                        if v.s3 == 1 {
                            v.s3 = emit_month(d.st, v.month, d.start, v.l160, b'D');
                            L::L15e5
                        } else {
                            L::L170e
                        }
                    } else {
                        L::L170e
                    }
                } else {
                    v.day = day_ordinal(&t.text);
                    let mut ok = v.day >= 0;
                    if !ok {
                        v.year = num::year_value(&t.text);
                        v.s3 = v.s11;
                        ok = v.year >= 0;
                    }
                    if ok {
                        v.s3 = emit_month(d.st, v.month, d.start, v.l160, b'D');
                        if v.s3 == 1 {
                            v.pb6i = v.year;
                            if v.day < 0 {
                                L::L15db
                            } else {
                                v.s3 = emit_ordinal(d.st, v.day, d.start, v.l160);
                                L::L15e5
                            }
                        } else {
                            L::L170e
                        }
                    } else {
                        L::L170e
                    }
                }
            }
            L::L170e => {
                if mode != 0x45 {
                    L::L101f
                } else {
                    let t144 = v.t144;
                    let t = d.tok;
                    v.month = month_of(&t144.text);
                    if v.month < 0 {
                        v.year = year_of(&t144.text, 0x45);
                        if v.year >= 0 {
                            v.month = month_of(&t.text);
                        }
                        if v.year >= 0 && v.month >= 0 {
                            v.s3 = emit_year(d.st, v.year, d.start, v.l160);
                            if v.s3 == 1 {
                                v.s3 = emit_month(d.st, v.month, d.start, v.l160, b'D');
                                L::L2786
                            } else {
                                L::L101f
                            }
                        } else {
                            L::L101f
                        }
                    } else {
                        v.day = day_of(&t.text, 0x45);
                        if v.day < 0 {
                            v.year = year_of(&t.text, 0x45);
                        }
                        if v.day < 0 && v.year < 0 {
                            L::L101f
                        } else {
                            v.s3 = emit_month(d.st, v.month, d.start, v.l160, b'D');
                            if v.s3 != 1 {
                                L::L101f
                            } else {
                                v.pb6i = v.day;
                                if v.day >= 0 {
                                    L::L277d
                                } else {
                                    v.s3 = emit_year(d.st, v.year, d.start, v.l160);
                                    L::L2786
                                }
                            }
                        }
                    }
                }
            }
            L::L134b => {
                v.s3 = emit_month(d.st, v.month, d.start, v.l160, b'D');
                if v.s3 != 1 {
                    L::L101f
                } else {
                    v.s3 = emit_ordinal(d.st, v.day, d.start, v.l160);
                    L::J196a
                }
            }
            L::L277d => {
                v.s3 = emit_ordinal(d.st, v.pb6i, d.start, v.l160);
                L::L2786
            }
            L::L2786 => {
                if v.s3 == 1 {
                    L::L127d
                } else {
                    L::L101f
                }
            }
            L::L22c8 => {
                let t = d.tok;
                if mode != 0 {
                    let era = (mode == 0x45 || mode == 0x48)
                        && t.ty == 1
                        && (2..=3).contains(&t.len)
                        && [&b"BC"[..], b"B.C", b"AD", b"A.D"].iter().any(|x| stricmp(x, &t.text) == 0);
                    if era {
                        L::L251f
                    } else {
                        L::L2421
                    }
                } else if t.ty == 1 && 1 < t.len && t.len < 4 {
                    if [&b"BC"[..], b"B.C", b"AD", b"A.D"].iter().all(|x| strcmp(x, &t.text) != 0) {
                        L::L2421
                    } else {
                        L::L251f
                    }
                } else {
                    L::L2421
                }
            }
            L::L251f => {
                let t144 = v.t144;
                v.year = year_of(&t144.text, mode);
                if v.year < 0 {
                    L::L101f
                } else {
                    let text = if v.year < 100 { num::cardinal(&t144.text, 1) } else { num::year(v.year) };
                    let e = d.tok.end;
                    v.s3 = d.st.wa.emit_words(&text, d.start, e, b'D', b'D');
                    if v.s3 != 1 {
                        L::L101f
                    } else {
                        let t = d.tok;
                        v.s3 = d.st.wa.emit(t.text(), d.start, e, b'A', b'S');
                        if v.s3 != 1 {
                            L::L101f
                        } else {
                            v.l160 = 1;
                            if mode == 0 {
                                L::L4b00
                            } else if t.len != 3 {
                                L::L126e
                            } else {
                                push_or!(L::E83);
                                L::L264f
                            }
                        }
                    }
                }
            }
            L::L2421 => {
                d.pop();
                v.l160 = d.tok.end;
                let t144 = v.t144;
                if mode == 0 {
                    v.year = num::year_value(&t144.text);
                    if v.year < 0 || !date_context(d.cx, d.st, *d.cur, b'B') {
                        L::L101f
                    } else {
                        L::L27e1
                    }
                } else if mode != 0x45 {
                    let t = d.tok;
                    match mode {
                        0x48 => {
                            v.year = year_of(&t.text, 0x48);
                            if v.year >= 0 {
                                v.s3 = emit_year(d.st, v.year, d.start, v.l160);
                                L::L2786
                            } else {
                                L::L101f
                            }
                        }
                        0x47 => {
                            v.month = month_of(&t.text);
                            if v.month >= 0 {
                                v.s3 = emit_month(d.st, v.month, d.start, v.l160, b'D');
                                L::L2786
                            } else {
                                L::L101f
                            }
                        }
                        0x46 => {
                            v.pb6i = day_of(&t.text, 0x46);
                            v.day = v.pb6i;
                            if v.pb6i >= 0 {
                                L::L277d
                            } else {
                                L::L101f
                            }
                        }
                        _ => L::L101f,
                    }
                } else {
                    v.year = num::year_value(&t144.text);
                    if v.year < 0 {
                        v.month = month_of(&t144.text);
                        if v.month < 0 {
                            v.pb6i = day_of(&t144.text, 0x45);
                            v.day = v.pb6i;
                            if v.pb6i >= 0 {
                                L::L277d
                            } else {
                                L::L101f
                            }
                        } else {
                            v.s3 = emit_month(d.st, v.month, d.start, v.l160, b'D');
                            if v.s3 >= 0 {
                                L::L127d
                            } else {
                                L::L101f
                            }
                        }
                    } else {
                        v.s3 = emit_year(d.st, v.year, d.start, v.l160);
                        if v.s3 != 1 {
                            L::L101f
                        } else {
                            L::L127d
                        }
                    }
                }
            }
            L::L27e1 => {
                v.s3 = emit_year(d.st, v.year, d.start, v.l160);
                L::J27f0
            }
            L::Ret => return v.s3,
            L::Ret2 => return 2,
        };
    }
}

fn restore_tp(d: &mut D) {
    let sv = d.sv;
    d.st.done.toks.truncate(sv.pn);
    d.st.ahead.n = sv.tn;
    if let Some(p) = d.st.ahead.pos[sv.tn as usize] {
        *d.cur = p;
    }
}

/// `Jan 5-7` / `January 5~7`: month, two days with "to". `Some(L1266)` on success.
fn range(d: &mut D, v: &mut V) -> Option<L> {
    let mode = d.mode;
    let t = d.tok;
    v.s3 = v.s11;
    if t.ws < 3 && num::day_like(&t.text) && (mode == 0 || mode == 0x45 || mode == 0x4a) {
        v.day = day_of(&v.t144.text, mode);
        if v.day >= 0 {
            v.i13c = day_of(&t.text, mode);
            if v.i13c >= 0 {
                let (s, e) = (d.start, t.end);
                v.s3 = emit_month(d.st, v.month, s, e, b'A');
                if v.s3 == 1 {
                    v.s3 = emit_ordinal(d.st, v.day, s, e);
                    if v.s3 == 1 {
                        v.s3 = d.st.wa.emit(b"to", s, e, b'S', b'D');
                        if v.s3 == 1 {
                            v.s3 = emit_ordinal(d.st, v.i13c, s, e);
                            if v.s3 == 1 {
                                return Some(L::L1266);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// An abbreviated month (`Jan`, `SEPT`), the `0x10072a50` index `k`.
fn abbreviated(d: &mut D, v: &mut V, k: i32) -> L {
    let mode = d.mode;
    v.month = atoi(MONTHS.field(k, 0x10));
    v.t13c = d.tok;
    v.f154 = date_context(d.cx, d.st, *d.cur, b'L') as i32;
    let Some(mut t) = d.push() else { return L::L3e69 };
    if t.ws == 0 && t.len < 3 && t.text[0] == b'.' && mode == 0 {
        let mut c = *d.cur;
        let dec = sbd::decide(d.cx.fe, &d.st.done, &mut d.st.ahead, &t, d.cx.t, &mut c, d.cx.base);
        if dec == b'P' {
            let t13c = v.t13c;
            let mut r = if v.f154 == 0 {
                d.st.wa.emit(t13c.text(), d.start, t13c.end, b'A', b'D')
            } else {
                emit_month(d.st, v.month, d.start, t13c.end, b'A')
            };
            if r == 1 {
                let n = d.st.wa.n();
                d.st.wa.attach(n - 1, b'.', t.end);
                r = d.st.done.append_all(&d.st.ahead, 8);
                if r == 1 {
                    return L::Ret2;
                }
            }
            v.s3 = r;
            return L::Ret;
        }
        match d.push() {
            Some(x) => t = x,
            None => return L::L3e69,
        }
    }
    let sepsym = |t: &Tok| (t.ty == 3 || t.ty == 7) && t.len == 1 && (t.text[0] == b'/' || t.text[0] == b'-');
    if t.ws < 3 {
        if t.ty == 2 && num::day_like(&t.text) {
            v.t144 = t;
            let Some(t) = d.push() else { return L::L3e69 };
            if t.ws > 2 {
                return L::L3209;
            }
            if is_sym(&t, b',') {
                let Some(t) = d.push() else { return L::L4ab8 };
                if t.ws < 3 && t.ty == 2 && num::day_like(&t.text) {
                    let (t13c, t144) = (v.t13c, v.t144);
                    v.s3 = d.month_two(&t13c, &t144, v.month, d.start, t.end, mode, false, true, false, true);
                    return L::J196a;
                }
                if mode == 0 {
                    d.seek(-2);
                    let tt = d.tok;
                    v.s3 = d.month_and(&tt, v.month, d.start, tt.end, *d.cur, 0, false, true, true);
                    return L::J27f0;
                }
                return L::L101f;
            }
            if t.ws > 2 || !(t.len == 1 && t.text[0] != 0 && b"-~".contains(&t.text[0])) {
                return L::L3209;
            }
            if d.push().is_none() {
                return L::L4ab8;
            }
            if let Some(l) = range(d, v) {
                return l;
            }
            if mode == 0 {
                d.seek(-2);
                let t144 = v.t144;
                v.s3 = d.month_and(&t144, v.month, d.start, d.tok.end, *d.cur, 0, false, true, true);
                return L::E2ae1;
            }
            return L::L101f;
        }
        if t.ws < 3 && sepsym(&t) {
            v.sep = t.text[0];
            let Some(t) = d.push() else { return L::E83 };
            if t.ws > 2 || t.ty != 2 || !num::day_like(&t.text) {
                return L::L101f;
            }
            v.t144 = t;
            let Some(t) = d.push() else { return L::E83 };
            if t.ws > 2 || !(t.ty == 3 || t.ty == 7) || t.len != 1 || t.text[0] != v.sep {
                return L::L3504;
            }
            let Some(t) = d.push() else { return L::L4ab8 };
            if t.ws < 3 && t.ty == 2 {
                return L::L3452;
            }
            return L::L34ac;
        }
    }
    if mode == 0 {
        if v.f154 == 0 {
            return L::L101f;
        }
        return L::L356d;
    }
    if mode == 0x47 || mode == 0x45 {
        return L::L356d;
    }
    L::L101f
}

fn upper(c: u8) -> bool {
    cls(c) & 0x80 != 0
}

/// A month name for the queue, from the abbreviation or the full name (`None`: neither).
fn queued_month(t: &Tok, f154: &mut i32) -> Option<&'static [u8]> {
    *f154 = MONTHS.find(&t.text, true);
    if *f154 >= 0 {
        return Some(month_name(atoi(MONTHS.field(*f154, 0x10))));
    }
    let k = FULL_MONTHS.find(&t.text, true);
    if k < 0 {
        return None;
    }
    Some(month_name(atoi(FULL_MONTHS.field(k, 0x10))))
}

/// `the 5th [of] May` after "the" (queued); `L3eed` / `L101f` when it is not one.
fn the_nth_of(d: &mut D, v: &mut V, fail: L) -> L {
    let mode = d.mode;
    let t = d.tok;
    d.st.q.add(t.text(), t.start, t.end, b'A', b'D', false);
    let Some(t) = d.push() else { return L::E83 };
    if t.ws > 2 || t.ty != 2 || !num::is_ordinal(&t.text) {
        return fail;
    }
    let o = num::ordinal(&t.text);
    d.st.q.add(&o, t.start, t.end, b'D', b'D', false);
    let Some(mut t) = d.push() else { return L::L4ab8 };
    if t.ws < 3 {
        if t.len == 2 && stricmp(&t.text, b"of") == 0 {
            d.st.q.add(t.text(), t.start, t.end, b'A', b'D', false);
            match d.push() {
                Some(x) => t = x,
                None => return L::L3e69,
            }
        }
        if t.ws < 3 && upper(t.text[0]) {
            let Some(name) = queued_month(&t, &mut v.f154) else { return fail };
            d.st.q.add(name, t.start, t.end, b'A', b'D', false);
            v.s3 = d.st.q.flush(&mut d.st.wa);
            if v.s3 == 1 {
                v.l160 = 1;
                if mode == 0 {
                    return L::L4b00;
                }
                if v.f154 < 0 {
                    return L::L126e;
                }
                if d.push().is_none() {
                    return L::L3e69;
                }
                return L::L264f;
            }
        }
    }
    fail
}

/// `LAB_10033f98`: "the 5th of May", `'90s`, "in May" / "in 1990".
fn the_nth(d: &mut D, v: &mut V) -> L {
    let mode = d.mode;
    let t = d.tok;
    if (mode == 0 || mode == 0x45) && t.len == 3 && t.ty == 1 && stricmp(b"the", &t.text) == 0 {
        return the_nth_of(d, v, L::L101f);
    }
    if (t.ws > 0 || (d.st.done.n() == 0 && d.st.ahead.n == 1)) && is_sym(&t, b'\'') {
        let Some(t) = d.push() else { return L::L3e69 };
        let y = if (mode == 0 || mode == 0x45 || mode == 0x48) && t.ws == 0 && t.ty == 2 && t.len == 2 { year_of(&t.text, mode) } else { -1 };
        if y < 0 {
            return L::L101f;
        }
        v.year = y;
        let Some(t) = d.push() else { return L::L3e69 };
        if t.ws == 0 && t.len == 1 && fold_s(t.text[0]) == b's' as i32 {
            let mut w = num::year(y);
            w.extend_from_slice(b"'s");
            v.s3 = d.st.wa.emit_words(&w, d.start, t.end, b'D', b'D');
        } else {
            d.pop();
            v.s3 = emit_year(d.st, y, d.start, d.tok.end);
        }
        return L::J196a;
    }
    if stricmp(&t.text, b"in") != 0 {
        return L::L101f;
    }
    v.t154 = t;
    let Some(t) = d.push() else {
        restore_tp(d);
        return L::E83;
    };
    let w = v.t154;
    if t.ws < 3 {
        let k = if t.ty == 1 && upper(t.text[0]) { MONTHS.find(&t.text, true) } else { -1 };
        if k >= 0 {
            v.month = atoi(MONTHS.field(k, 0x10));
            v.s3 = d.st.wa.emit(w.text(), w.start, w.end, b'A', b'D');
            if v.s3 == 1 {
                v.s3 = emit_month(d.st, v.month, t.start, t.end, b'A');
                return L::J196a;
            }
        } else if t.ty == 2 {
            let y = num::year_value(&t.text);
            if y >= 0 {
                v.year = y;
                v.s3 = d.st.wa.emit(w.text(), w.start, w.end, b'A', b'D');
                if v.s3 == 1 {
                    v.s3 = emit_year(d.st, y, t.start, t.end);
                    return L::J196a;
                }
            }
        }
    }
    L::L101f
}

/// The capitalized-word branch of `LAB_10033901`: a weekday (`Wed`, `Thurs.`, `Friday`), queued, then
/// optionally "the 5th of May".
fn weekday(d: &mut D, v: &mut V) -> L {
    let t = d.tok;
    let k = WEEKDAYS.find(&t.text, true);
    let name: &[u8] = if k >= 0 {
        v.l160 = d.st.ahead.n;
        v.q144 = d.st.q.n();
        WEEKDAYS.field(k, 0x10)
    } else {
        let k2 = find_ptab(WEEKDAY_NAMES, &t.text);
        if k2 < 0 {
            return L::L3f98;
        }
        WEEKDAY_NAMES.get(k2)
    };
    v.i13c = k;
    d.st.q.add(name, d.start, t.end, b'A', b'D', false);
    let Some(mut t) = d.push() else { return L::L4ab8 };
    if t.ws == 0 && t.len < 3 && t.text[0] == b'.' {
        match d.push() {
            Some(x) => t = x,
            None => return L::L4ab8,
        }
        if t.ws < 3 && is_sym(&t, b',') {
            d.st.q.add(b",", t.start, t.end, b'S', b'0', false);
            match d.push() {
                Some(x) => t = x,
                None => return L::L4ab8,
            }
        }
    } else if t.ws < 3 && t.ty == 3 && t.len < 4 && t.text[0] != 0 && b".,".contains(&t.text[0]) {
        d.st.q.add(b",", t.start, t.end, b'S', b'0', false);
        match d.push() {
            Some(x) => t = x,
            None => return L::E83,
        }
    }
    v.s3 = v.s11;
    if t.ws < 3 && t.len == 3 && stricmp(&t.text, b"the") == 0 {
        return the_nth_of(d, v, L::L3eed);
    }
    L::L3eed
}

/// `FUN_10049980` (insensitive bisection over a `char*` table).
fn find_ptab(tab: Ptab, w: &[u8]) -> i32 {
    let w = cs(w);
    if w.is_empty() {
        return -1;
    }
    let (mut lo, mut hi) = (0, tab.count() - 1);
    let mut mid = hi;
    while lo <= hi {
        mid /= 2;
        let c = stricmp(tab.get(mid), w);
        if c == 0 {
            return mid;
        }
        if c < 0 {
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
        mid = hi + lo;
    }
    -1
}

/// `LAB_1003101f`: the number tokens again with the alias tokenizer, for dotted dates (`12.25.2003`,
/// `25.Dec`).
fn digits_with_dots(d: &mut D, v: &mut V) -> L {
    let mode = d.mode;
    let sv = d.sv;
    if d.st.ahead.n != sv.tn {
        d.st.ahead.n = sv.tn;
        if let Some(p) = d.st.ahead.pos[sv.tn as usize] {
            *d.cur = p;
        }
    }
    if d.st.ahead.n > 0 {
        d.pop();
    }
    if !is_digit(at(d.cx.t, *d.cur as isize)) {
        return L::L4b5e;
    }
    let Some(i) = d.st.ahead.push(d.cx.t, d.cur, d.cx.base, b'R', d.cx.fe.ud.as_ref()) else {
        restore_tp(d);
        return L::E83;
    };
    d.tok = d.st.ahead.toks[i];
    let t = d.tok;
    if !t.text().contains(&b'.') {
        return L::L101fTail;
    }
    let Some((n, parts)) = split_dots(&t.text) else { return L::L101fTail };
    let p0 = cs(&parts[0..]).to_vec();
    let p1 = cs(&parts[16..]).to_vec();
    let p2 = cs(&parts[32..]).to_vec();
    d.start = t.start;
    v.l160 = t.end;
    let (s, e) = (d.start, v.l160);
    if mode == 0 || mode == 0x45 {
        if n == 3 {
            if let Some((y, m, dd)) = three_fields(&p0, &p1, &p2, mode) {
                v.s3 = emit_mdy(d.st, y, m, dd, s, e, b'D');
                if v.s3 == 1 {
                    return L::L1266;
                }
            }
        }
        if mode == 0x45 && n == 2 {
            v.month = month_of(&p0);
            let mut ok1 = false;
            if v.month >= 0 {
                v.day = day_of(&p1, 0x45);
                ok1 = v.day >= 0;
            }
            let mut ok2 = false;
            if !ok1 {
                v.month = month_of(&p0);
                if v.month >= 0 {
                    v.year = year_of(&p0, 0x45);
                    ok2 = v.year >= 0;
                }
            }
            if !ok1 && !ok2 {
                v.year = year_of(&p0, 0x45);
                if v.year >= 0 {
                    v.month = month_of(&p1);
                    if v.month >= 0 {
                        v.s3 = emit_month(d.st, v.month, s, e, b'D');
                        if v.s3 == 1 {
                            v.s3 = emit_year(d.st, v.year, s, e);
                            if v.s3 == 1 {
                                return L::L127d;
                            }
                        }
                    }
                }
            } else {
                v.s3 = emit_month(d.st, v.month, s, e, b'D');
                if v.s3 == 1 {
                    v.s3 = if v.day < 0 { emit_year(d.st, v.year, s, e) } else { emit_ordinal(d.st, v.day, s, e) };
                    if v.s3 == 1 {
                        return L::L127d;
                    }
                }
            }
        }
    } else if n == 3 {
        if mode == 0x4d || mode == 0x4f || mode == 0x4e {
            if let Some((y, m, dd)) = three_fields_fixed(&p0, &p1, &p2, mode) {
                v.s3 = emit_mdy(d.st, y, m, dd, s, e, b'D');
                if v.s3 == 1 {
                    return L::L1266;
                }
            }
        }
    } else if n == 2 {
        let mut md = None;
        let mut my = None;
        match mode {
            0x4a => {
                v.month = month_of(&p0);
                if v.month >= 0 {
                    v.day = day_of(&p1, 0x4a);
                    if v.day >= 0 {
                        md = Some(());
                    }
                }
            }
            0x4b => {
                v.day = day_of(&p0, 0x4b);
                if v.day >= 0 {
                    v.month = month_of(&p1);
                    if v.month >= 0 {
                        md = Some(());
                    }
                }
            }
            0x4c => {
                v.year = year_of(&p0, 0x4c);
                if v.year >= 0 {
                    v.month = month_of(&p1);
                    if v.month >= 0 {
                        my = Some(());
                    }
                }
            }
            0x49 => {
                v.month = month_of(&p0);
                if v.month >= 0 {
                    v.year = year_of(&p1, 0x49);
                    if v.year >= 0 {
                        my = Some(());
                    }
                }
            }
            _ => {}
        }
        if md.is_some() {
            v.s3 = emit_month(d.st, v.month, s, e, b'D');
            if v.s3 == 1 {
                v.s3 = emit_ordinal(d.st, v.day, s, e);
                if v.s3 == 1 {
                    return L::L1266;
                }
            }
        } else if my.is_some() {
            v.s3 = emit_month(d.st, v.month, s, e, b'D');
            if v.s3 == 1 {
                v.s3 = emit_year(d.st, v.year, s, e);
                if v.s3 == 1 {
                    return L::L1266;
                }
            }
        }
    }
    L::L101fTail
}

/// `LAB_10030d64`: a day-like number first (`5/12/2004`, `25-Dec`, `5 January`, `1990 AD`).
fn day_first(d: &mut D, v: &mut V) -> L {
    let mode = d.mode;
    v.t144 = d.tok;
    let Some(t) = d.push() else { return L::L3e69 };
    let sepsym = (t.ty == 3 || t.ty == 7) && t.len == 1 && (t.text[0] == b'/' || t.text[0] == b'-');
    if t.ws < 3 && sepsym {
        v.sep = t.text[0];
        let Some(t) = d.push() else { return L::L4ab8 };
        if t.ws > 2 {
            return L::L19ca;
        }
        let same_sep = |t: &Tok, sep: u8| t.ws <= 2 && (t.ty == 3 || t.ty == 7) && t.len == 1 && t.text[0] == sep;
        if t.ty == 2 && num::day_like(&t.text) {
            v.t154 = t;
            let Some(t) = d.push() else { return L::L3e69 };
            if !same_sep(&t, v.sep) {
                d.pop();
                v.l160 = d.tok.end;
                if mode == 0 {
                    if !date_context(d.cx, d.st, *d.cur, b'B') {
                        return L::L101f;
                    }
                    return L::L1503;
                }
                if mode == 0x45 {
                    return L::L1503;
                }
                let (a, b2) = (v.t144.text, d.tok.text);
                match mode {
                    0x49 => {
                        v.month = month_of(&a);
                        if v.month >= 0 {
                            v.year = year_of(&b2, 0x49);
                            if v.year >= 0 {
                                v.c18 = b'D';
                                return L::L1226;
                            }
                        }
                    }
                    0x4c => {
                        v.year = year_of(&a, 0x4c);
                        if v.year >= 0 {
                            v.month = month_of(&b2);
                            if v.month >= 0 {
                                v.c18 = b'D';
                                return L::L1226;
                            }
                        }
                    }
                    0x4a => {
                        v.month = month_of(&a);
                        if v.month >= 0 {
                            v.day = day_of(&b2, 0x4a);
                            if v.day >= 0 {
                                return L::L134b;
                            }
                        }
                    }
                    0x4b => {
                        v.day = day_of(&a, 0x4b);
                        if v.day >= 0 {
                            v.month = month_of(&b2);
                            if v.month >= 0 {
                                return L::L134b;
                            }
                        }
                    }
                    _ => {}
                }
                if v.sep != b'-' {
                    return L::L101f;
                }
                let (s, e) = (d.start, v.l160);
                match mode {
                    0x48 => {
                        v.year = year_of(&a, 0x48);
                        v.i13c = if v.year >= 0 { year_of(&b2, 0x48) } else { -1 };
                        if v.year < 0 || v.i13c < 0 {
                            return L::L101f;
                        }
                        v.s3 = emit_year(d.st, v.year, s, e);
                        if v.s3 != 1 {
                            return L::L101f;
                        }
                        v.s3 = d.st.wa.emit(b"to", s, e, b'S', b'D');
                        if v.s3 != 1 {
                            return L::L101f;
                        }
                        v.s3 = emit_year(d.st, v.i13c, s, e);
                        L::L2786
                    }
                    0x47 => {
                        v.month = month_of(&a);
                        if v.month >= 0 {
                            v.i13c = month_of(&b2);
                            if v.i13c >= 0 {
                                v.s3 = emit_month(d.st, v.month, s, e, b'D');
                                if v.s3 == 1 {
                                    v.s3 = d.st.wa.emit(b"to", s, e, b'S', b'D');
                                    if v.s3 == 1 {
                                        v.s3 = emit_month(d.st, v.i13c, s, e, b'D');
                                        return L::L2786;
                                    }
                                }
                            }
                        }
                        L::L101f
                    }
                    0x46 => {
                        // Both days come from the first number in the engine.
                        v.day = day_of(&a, 0x46);
                        v.i13c = if v.day >= 0 { day_of(&a, 0x46) } else { -1 };
                        if v.day < 0 || v.i13c < 0 {
                            return L::L101f;
                        }
                        v.s3 = emit_ordinal(d.st, v.day, s, e);
                        if v.s3 != 1 {
                            return L::L101f;
                        }
                        v.s3 = d.st.wa.emit(b"to", s, e, b'S', b'D');
                        v.pb6i = v.i13c;
                        if v.s3 != 1 {
                            return L::L101f;
                        }
                        L::L277d
                    }
                    _ => L::L101f,
                }
            } else {
                let Some(t) = d.push() else { return L::L3e69 };
                if t.ws > 2 || t.ty != 2 || !num::day_like(&t.text) {
                    return L::L101f;
                }
                let p8 = push(d.cx, d.st, d.cur);
                v.s3 = v.s12;
                let (a, b2, c) = (v.t144.text, v.t154.text, t.text);
                let found = if mode == 0 || mode == 0x45 {
                    let open = match p8 {
                        None => true,
                        Some(x) => {
                            let sep_sym = (x.ty == 3 || x.ty == 7) && x.len == 1 && x.text[0] == v.sep;
                            (!sep_sym && x.ty > 2) || x.ws > 0
                        }
                    };
                    if open {
                        three_fields(&a, &b2, &c, mode)
                    } else {
                        None
                    }
                } else {
                    three_fields_fixed(&a, &b2, &c, mode)
                };
                if let Some((y, m, dd)) = found {
                    v.s3 = emit_mdy(d.st, y, m, dd, d.start, t.end, b'D');
                    if v.s3 == 1 {
                        v.l160 = 1;
                    }
                }
                seek(d.st, d.cur, -1);
                L::L1013
            }
        } else if t.ty != 1 {
            L::L1cf7
        } else {
            let k = FULL_MONTHS.find(&t.text, true);
            if k < 0 {
                return L::L19ca;
            }
            v.month = atoi(FULL_MONTHS.field(k, 0x10));
            v.t13c = t;
            let Some(t) = d.push() else { return L::L4ab8 };
            if t.ws < 3 && (t.ty == 3 || t.ty == 7) && t.len == 1 && t.text[0] == v.sep {
                let Some(t) = d.push() else { return L::L3e69 };
                if t.ty != 2 || !num::day_like(&t.text) {
                    return L::L1c5e;
                }
                return L::L1920;
            }
            d.pop();
            let t144 = v.t144;
            v.s3 = d.month_and(&t144, v.month, d.start, d.tok.end, *d.cur, mode, true, false, false);
            if v.s3 != 1 {
                return L::L101f;
            }
            L::L1266
        }
    } else {
        after_day(d, v, t)
    }
}

/// The `else` of `LAB_10030d64`: a month name or an era after the number.
fn after_day(d: &mut D, v: &mut V, t: Tok) -> L {
    let mode = d.mode;
    if t.ty != 1 {
        return L::L22c8;
    }
    let k = FULL_MONTHS.find(&t.text, true);
    if k < 0 {
        let k2 = MONTHS.find(&t.text, true);
        if k2 < 0 {
            return L::L22c8;
        }
        v.month = atoi(MONTHS.field(k2, 0x10));
        let pb6 = t;
        let Some(t) = d.push() else { return L::L3e69 };
        let t144 = v.t144;
        v.s3 = if t.ws == 0 && t.len < 3 && t.text[0] == b'.' {
            let Some(t) = d.push() else { return L::L4ab8 };
            if t.ty == 2 && num::day_like(&t.text) {
                d.month_two(&pb6, &t144, v.month, d.start, t.end, mode, false, false, true, false)
            } else {
                d.seek(if mode == 0 { -2 } else { -1 });
                d.month_and(&t144, v.month, d.start, d.tok.end, *d.cur, mode, false, false, false)
            }
        } else if t.ty == 2 && num::day_like(&t.text) {
            d.month_two(&pb6, &t144, v.month, d.start, t.end, mode, false, false, false, false)
        } else {
            d.pop();
            d.month_and(&t144, v.month, d.start, d.tok.end, *d.cur, mode, false, false, false)
        };
        return L::J196a;
    }
    v.month = atoi(FULL_MONTHS.field(k, 0x10));
    v.t13c = t;
    let Some(t) = d.push() else { return L::E83 };
    if t.ty == 2 && num::day_like(&t.text) {
        v.i5 = t.end;
        v.s12 = 0;
        v.s11 = 0;
        v.s3 = 0;
        return L::L3472;
    }
    let t144 = v.t144;
    if t.ws > 2 || !is_sym(&t, b'\'') {
        d.pop();
        v.s3 = d.month_and(&t144, v.month, d.start, d.tok.end, *d.cur, mode, false, false, false);
        return L::J196a;
    }
    let Some(t) = d.push() else { return L::L4ab8 };
    let y = if t.ws == 0 && t.len == 2 { year_of(&t.text, mode) } else { -1 };
    if y >= 0 {
        v.year = y;
        v.l160 = t.end;
        if mode == 0 || mode == 0x4d || mode == 0x45 {
            v.day = day_of(&t144.text, mode);
            if v.day >= 0 {
                v.s3 = emit_mdy(d.st, y, v.month, v.day, d.start, v.l160, b'A');
                return L::J196a;
            }
        }
        if mode != 0 {
            return L::L101f;
        }
        v.s3 = emit_any(d.st, &t144.text, d.start, v.l160);
        if v.s3 != 1 {
            return L::L101f;
        }
        v.s3 = emit_month(d.st, v.month, d.start, v.l160, b'A');
        if v.s3 != 1 {
            return L::L101f;
        }
        return L::L27e1;
    }
    if mode != 0 {
        return L::L101f;
    }
    d.seek(-2);
    v.i16 = *d.cur;
    v.s12 = 0;
    v.s11 = 0;
    v.i5 = d.tok.end;
    v.s3 = 0;
    v.pb6 = t144;
    L::L34e2
}
