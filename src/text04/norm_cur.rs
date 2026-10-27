//! `norm_currency` (0x1002f350): money amounts: a currency sign, ISO code or country prefix with a
//! number, an optional multiplier and dash (`$5`, `US$ 2.5m`, `5 EUR`, `5 (USD)`, `C$10`), read through the
//! pending queue ("five dollars and fifty cents", "two point five million U.S. dollars"). Mode `D` is
//! the say-as currency format.

use super::mem::*;
use super::norm_units::is_sym;
use super::normalize::{Cx, State};
use super::num;
use super::rx::*;
use super::tokenize::Tok;

/// Currency signs: key, singular (+0x20), plural (+0x40).
const SIGNS: Rec60 = rec60(0x1006fe20, 0x1008482c);
/// Multipliers (`m`, `bn`, `thousand`), name at +0x10.
const MULT: Rec30 = rec30(0x1006fc10, 0x10084828);
/// Country prefixes (`US`, `C`, `HK`), name at +0x10.
const COUNTRY: Rec30 = rec30(0x10070120, 0x10084830);
/// ISO codes (`EUR`, `GBP`): key, singular (+0x20), plural (+0x40).
const CODES: Rec60 = rec60(0x100705d0, 0x10084834);

/// `FUN_1004f8d0`.
fn pluralize(sing: &[u8], plur: &[u8]) -> Vec<u8> {
    if !plur.is_empty() {
        if plur[0] == b'-' {
            return [sing, &plur[1..]].concat();
        }
        return plur.to_vec();
    }
    sing.to_vec()
}

struct Found {
    sign: i32,
    mult: i32,
    country: i32,
    code: i32,
    num: Option<Vec<u8>>,
    per: bool,
}

/// `FUN_10030810`: number, multiplier, country, code or sign name, in that order.
fn read_general(q: &mut Queue, s: i32, e: i32, f: &Found, mode: i32) {
    q.reset();
    let mut many = false;
    if let Some(n) = f.num.as_deref().filter(|n| !n.is_empty()) {
        if num::atof(n) == 0.0 {
            q.add(b"zero", s, e, b'D', b'D', false);
        } else {
            q.add(&num::cardinal(n, 1), s, e, b'D', b'D', false);
        }
        many = (f.mult >= 0 || num::plural(n)) && !f.per;
    }
    if f.mult >= 0 {
        q.add(MULT.field(f.mult, 0x10), s, e, b'A', b'D', false);
    }
    if f.country >= 0 {
        q.add(COUNTRY.field(f.country, 0x10), s, e, b'A', b'D', false);
    }
    if f.code >= 0 {
        let (sg, pl) = (CODES.field(f.code, 0x20), CODES.field(f.code, 0x40));
        q.add(&if many { pluralize(sg, pl) } else { sg.to_vec() }, s, e, b'A', b'D', false);
    }
    if f.sign >= 0 && f.code < 0 {
        let (sg, pl) = (SIGNS.field(f.sign, 0x20), SIGNS.field(f.sign, 0x40));
        q.add(&if many { pluralize(sg, pl) } else { sg.to_vec() }, s, e, b'S', b'D', false);
    }
    if mode == 0x44 && f.sign < 0 && f.code < 0 && f.country < 0 {
        q.add(if many { b"dollars" } else { b"dollar" }, s, e, b'D', b'D', false);
    }
}

/// `FUN_100303c0`: two-decimal amounts as units and cents, everything else through `read_general`.
fn read(q: &mut Queue, s: i32, e: i32, f: &Found, mode: i32) {
    let n = f.num.as_deref().unwrap_or(b"");
    if f.mult >= 0 || f.country >= 0 || f.code >= 0 || matches!(f.sign, 1 | 2 | 4 | 5 | 7) || n.is_empty() {
        return read_general(q, s, e, f, mode);
    }
    q.reset();
    let (whole, frac) = match n.iter().position(|&c| c == b'.') {
        None => (n.to_vec(), Vec::new()),
        Some(p) => (n[..p].to_vec(), n[p + 1..].to_vec()),
    };
    if frac.len() != 2 {
        return read_general(q, s, e, f, mode);
    }
    if !whole.is_empty() && atoi(&whole) != 0 {
        q.add(&num::cardinal(&whole, 1), s, e, b'D', b'D', false);
        if !num::plural(&whole) {
            if f.sign >= 0 {
                q.add(SIGNS.field(f.sign, 0x20), s, e, b'D', b'D', false);
            } else if mode == 0x44 {
                q.add(b"dollar", s, e, b'D', b'D', false);
            }
        } else if f.sign >= 0 {
            q.add(&pluralize(SIGNS.field(f.sign, 0x20), SIGNS.field(f.sign, 0x40)), s, e, b'D', b'D', false);
        } else if mode == 0x44 {
            q.add(b"dollars", s, e, b'D', b'D', false);
        }
    }
    if frac.is_empty() || atoi(&frac) == 0 {
        if q.n() == 0 {
            q.add(b"zero dollars", s, e, b'D', b'D', false);
        }
        return;
    }
    if q.n() > 0 {
        q.add(b"and", s, e, b'D', b'D', false);
    }
    q.add(&num::cardinal(&frac, 1), s, e, b'D', b'D', false);
    let penny = f.sign != 0 && f.sign != 6 && (f.sign >= 0 || f.code >= 0 || f.country >= 0 || mode != 0x44);
    let w: &[u8] = match (num::plural(&frac), penny) {
        (true, true) => b"pennies",
        (true, false) => b"cents",
        (false, true) => b"penny",
        (false, false) => b"cent",
    };
    q.add(w, s, e, b'D', b'D', false);
}

/// A one-letter word right after, or a longer one with at most one blank before it.
fn tight(word: &Tok, next_ws: i32) -> bool {
    (word.len == 1 && next_ws == 0) || (word.len > 1 && next_ws <= 1)
}

struct C<'a, 'b> {
    cx: &'a Cx<'b>,
    st: &'a mut State,
    cur: &'a mut usize,
}

impl C<'_, '_> {
    fn push(&mut self) -> Option<Tok> {
        push(self.cx, self.st, self.cur)
    }

    fn pop(&mut self) -> Tok {
        pop(self.st, self.cur)
    }
}

enum At {
    F01fd,
    F01cf,
    F01e9,
    F02e5,
    Fail,
}

pub fn currency(cx: &Cx, st: &mut State, cur: &mut usize, _t: &Tok) -> i32 {
    currency_mode(cx, st, cur, 0)
}

pub fn currency_mode(cx: &Cx, st: &mut State, cur: &mut usize, mode: i32) -> i32 {
    let first;
    if mode == 0 {
        first = cur_tok(st);
    } else {
        if mode != 0x44 {
            return 5;
        }
        st.ahead.reset();
        let Some(t) = push(cx, st, cur) else { return 5 };
        first = t;
    }
    let sv = save(st);
    let start = first.start;
    let mut f = Found { sign: -1, mult: -1, country: -1, code: -1, num: None, per: false };
    let mut b14 = false;
    let mut b16 = false;
    let mut t9 = first;
    let mut c = C { cx, st, cur };
    let at = body(&mut c, mode, &mut f, &mut b14, &mut b16, &mut t9, first);
    let at = match at {
        At::Fail => return restore(c.st, c.cur, sv, 4),
        a => a,
    };
    let mut at = at;
    loop {
        match at {
            At::F01cf => {
                t9 = c.pop();
                at = At::F01fd;
            }
            At::F01e9 => {
                t9 = c.pop();
                at = At::F01fd;
            }
            At::F01fd => {
                b14 = true;
                if mode != 0 {
                    b14 = false;
                    let Some(mut x) = c.push() else { return restore(c.st, c.cur, sv, 4) };
                    if x.ty == 8 && x.text[0] == 0 {
                        let Some(y) = c.push() else { return restore(c.st, c.cur, sv, 4) };
                        x = y;
                    }
                    t9 = x;
                    if x.ty == 5 || x.ty == 9 {
                        b14 = true;
                    }
                }
                at = At::F02e5;
            }
            At::F02e5 => break,
            At::Fail => return restore(c.st, c.cur, sv, 4),
        }
    }
    if (f.sign != 1 || b16) && b14 {
        let mut r = c.st.done.append_all(&c.st.ahead, 7);
        if r == 1 {
            read(&mut c.st.q, start, t9.end, &f, mode);
            r = c.st.q.flush(&mut c.st.wa);
            if r == 1 {
                return r;
            }
        }
        restore(c.st, c.cur, sv, r);
    }
    if c.st.ahead.n != sv.tn {
        c.st.ahead.n = sv.tn;
        if let Some(p) = c.st.ahead.pos[sv.tn as usize] {
            *c.cur = p;
        }
    }
    5
}

/// A `-` right after (kept) or not (given back).
fn dash(c: &mut C) -> Option<bool> {
    let t = c.push()?;
    if t.ws < 2 && is_sym(&t, b'-') {
        Some(true)
    } else {
        c.pop();
        Some(false)
    }
}

fn body(c: &mut C, mode: i32, f: &mut Found, b14: &mut bool, b16: &mut bool, t9: &mut Tok, first: Tok) -> At {
    macro_rules! p {
        () => {
            match c.push() {
                Some(t) => t,
                None => return At::Fail,
            }
        };
    }
    let mut s4 = false;
    if first.ty == 3 && {
        f.sign = SIGNS.find_prefix(&first.text, b'S');
        f.sign >= 0
    } {
        let t7 = p!();
        if t7.ws < 2 && t7.ty == 2 {
            f.num = Some(t7.text().to_vec());
            let Some(d) = dash(c) else { return At::Fail };
            *b14 = d;
            let t7 = p!();
            let m = if t7.ws < 2 && t7.ty == 1 { MULT.find(&t7.text, true) } else { -1 };
            if m >= 0 {
                f.mult = m;
                *b16 = true;
                let t7 = p!();
                if !(t7.ws == 0 && t7.len <= 2 && t7.text[0] == b'.') {
                    if (t7.ty == 3 || t7.ty == 7) && t7.len == 1 && t7.text[0] == b'-' {
                        let x = c.push().unwrap_or_default();
                        let a = strncmp(&x.text, b"a", 1) == 0 || strncmp(&x.text, b"per", 3) == 0;
                        if a && (x.text[1] == b'-' || x.text[3] == b'-') && x.ws == 0 {
                            s4 = true;
                        }
                        c.pop();
                    }
                    c.pop();
                }
            } else {
                c.pop();
                if *b14 {
                    c.pop();
                }
            }
            let t = p!();
            *t9 = t;
            let k = if t.ty == 1 && ((t.len > 1 && t.ws < 2) || (t.len == 1 && t.ws == 0)) { COUNTRY.find(&t.text, true) } else { -1 };
            if k >= 0 {
                f.country = k;
                *b16 = true;
                f.per = s4;
                return At::F01fd;
            }
            f.per = s4;
            return At::F01cf;
        }
        if t7.ty == 1 && t7.ws < 2 {
            f.code = CODES.find_prefix(&t7.text, b'I');
            let mut ok = f.code >= 0;
            if !ok && (t7.len > 1 || (t7.len == 1 && t7.ws == 0)) {
                f.country = COUNTRY.find(&t7.text, true);
                ok = f.country >= 0;
            }
            if ok {
                let t7 = p!();
                if t7.ws > 1 || t7.ty != 2 {
                    *t9 = c.pop();
                    f.per = s4;
                    return At::F01fd;
                }
                f.num = Some(t7.text().to_vec());
                *b16 = true;
                let Some(d) = dash(c) else { return At::Fail };
                *b14 = d;
                let t = p!();
                let m = if t.ws <= 1 && t.ty == 1 { MULT.find(&t.text, true) } else { -1 };
                if m < 0 {
                    *t9 = c.pop();
                    f.per = s4;
                    if *b14 {
                        *t9 = c.pop();
                    }
                    return At::F01fd;
                }
                f.mult = m;
                let t = p!();
                *t9 = t;
                return dot_after(c, f, b14, t9, s4);
            }
        }
        *t9 = c.pop();
        f.per = s4;
        return At::F01fd;
    }
    if first.ty == 1 {
        f.country = COUNTRY.find(&first.text, true);
        if f.country < 0 {
            f.code = CODES.find_prefix(&first.text, b'I');
            if f.code < 0 {
                return digits_first(c, mode, f, b14, b16, t9, first);
            }
            let t7 = p!();
            if t7.ws > 1 || t7.ty != 2 {
                *t9 = t7;
                return if mode != 0 { At::F01e9 } else { At::F02e5 };
            }
            let Some(d) = dash(c) else { return At::Fail };
            *b14 = d;
            let t = p!();
            *t9 = t;
            return after_amount(c, f, b14, t9, t7, s4);
        }
        let t7 = p!();
        if t7.ty == 3 && tight(&first, t7.ws) {
            f.sign = SIGNS.find_prefix(&t7.text, b'S');
            if f.sign >= 0 {
                let t7 = p!();
                if t7.ws > 1 || t7.ty != 2 {
                    *t9 = c.pop();
                    f.per = s4;
                    return At::F01fd;
                }
                *b16 = true;
                let Some(d) = dash(c) else { return At::Fail };
                *b14 = d;
                let t = p!();
                *t9 = t;
                return after_amount(c, f, b14, t9, t7, s4);
            }
        }
        *t9 = t7;
        return if mode != 0 { At::F01e9 } else { At::F02e5 };
    }
    digits_first(c, mode, f, b14, b16, t9, first)
}

/// `LAB_1002fb31`: the number `t7` and the token after the optional dash (`t9`): a multiplier, then an
/// optional `.`.
fn after_amount(c: &mut C, f: &mut Found, b14: &mut bool, t9: &mut Tok, t7: Tok, s4: bool) -> At {
    f.num = Some(t7.text().to_vec());
    let t = *t9;
    let m = if t.ws <= 1 && t.ty == 1 { MULT.find(&t.text, true) } else { -1 };
    if m < 0 {
        *t9 = c.pop();
        *b14 = !*b14;
        f.per = s4;
        if !*b14 {
            *t9 = c.pop();
        }
        return At::F01fd;
    }
    f.mult = m;
    let Some(t) = c.push() else { return At::Fail };
    *t9 = t;
    dot_after(c, f, b14, t9, s4)
}

/// `LAB_1002f8cb`: a `.` after the multiplier stays.
fn dot_after(c: &mut C, f: &mut Found, b14: &mut bool, t9: &mut Tok, s4: bool) -> At {
    let t = *t9;
    f.per = s4;
    if t.ws == 0 && t.len < 3 {
        *b14 = t.text[0] == b'.';
        if !*b14 {
            *t9 = c.pop();
        }
        return At::F01fd;
    }
    At::F01cf
}

/// `LAB_1002fceb`: a number first, then a multiplier, a bracket and a code, sign or country.
fn digits_first(c: &mut C, mode: i32, f: &mut Found, b14: &mut bool, b16: &mut bool, t9: &mut Tok, first: Tok) -> At {
    let _ = b14;
    macro_rules! p {
        () => {
            match c.push() {
                Some(t) => t,
                None => return At::Fail,
            }
        };
    }
    if first.ty != 2 {
        return At::F02e5;
    }
    f.num = Some(first.text().to_vec());
    let Some(b15) = dash(c) else { return At::Fail };
    let t7 = p!();
    let m = if t7.ws < 2 && t7.ty == 1 { MULT.find(&t7.text, true) } else { -1 };
    if m >= 0 {
        f.mult = m;
        let t7 = p!();
        if !(t7.ws == 0 && t7.len <= 2 && t7.text[0] == b'.') {
            c.pop();
        }
    } else {
        c.pop();
        if b15 {
            c.pop();
        }
    }
    let t7 = p!();
    let bracket = t7.ws < 2 && t7.ty == 3 && t7.text[1] == 0 && t7.text[0] != 0 && b"([{<".contains(&t7.text[0]);
    if !bracket {
        c.pop();
    }
    let t = p!();
    *t9 = t;
    if t.ws >= 2 {
        return if mode == 0 { At::F02e5 } else { At::F01e9 };
    }
    let close = |c: &mut C, t9: &mut Tok| -> At {
        let Some(x) = c.push() else { return At::Fail };
        *t9 = x;
        if x.ws > 1 || x.ty != 3 || x.text[1] != 0 {
            return At::F01cf;
        }
        if x.text[0] == 0 || !b")]}>".contains(&x.text[0]) {
            return At::F01cf;
        }
        At::F01fd
    };
    if t.ty == 1 {
        f.code = CODES.find_prefix(&t.text, b'I');
        if f.code >= 0 {
            if !bracket {
                return At::F01fd;
            }
            return close(c, t9);
        }
    }
    let sign = if t.ty == 3 { SIGNS.find_prefix(&t.text, b'S') } else { -1 };
    if sign >= 0 {
        f.sign = sign;
    }
    let country = if sign < 0 && t.ty == 1 { COUNTRY.find(&t.text, true) } else { -1 };
    if sign < 0 {
        if country < 0 {
            return if mode == 0 { At::F02e5 } else { At::F01e9 };
        }
        f.country = country;
    }
    let t7 = p!();
    let mut t7k = t7;
    if f.sign < 0 {
        // A country prefix, then a sign right after it.
        let mut ok = false;
        if tight(&t, t7.ws) && t7.ty == 3 {
            f.sign = SIGNS.find_prefix(&t7.text, b'S');
            ok = f.sign >= 0;
        }
        if !ok {
            *t9 = c.pop();
            return At::F02e5;
        }
    } else if t7.ty == 1 && tight(&t7, t7.ws) && {
        f.country = COUNTRY.find(&t7.text, true);
        f.country >= 0
    } {
    } else {
        t7k = c.pop();
    }
    *b16 = true;
    *t9 = t7k;
    if !bracket {
        return At::F01fd;
    }
    close(c, t9)
}
