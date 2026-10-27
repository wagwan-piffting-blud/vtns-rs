//! `norm_time` (0x1004a3e0): `H:MM[:SS]` and `MM:SS` with an optional AM/PM and time zone
//! ("five fifteen AM", "ten o'clock"). Modes `h`..`k` (0x68..0x6b) are the say-as time formats, which
//! start from a fresh look-ahead buffer.

use super::mem::*;
use super::norm_units::{context, is_sym};
use super::normalize::{Cx, State};
use super::num;
use super::rx::*;
use super::stream::token_at;
use super::tokenize::Tok;

/// Words that make a bare `N:NN` a time (`0x10073320`).
const TIME_WORDS: Ptab = ptab(0x10073320, 0x10084868);

/// `FUN_1004b4c0`: one digit, or two digits in range (hours below 25, minutes and seconds below 61;
/// below 100 in the say-as modes).
fn field(t: &Tok, what: u8, mode: i32) -> bool {
    let w = t.text();
    match w.len() {
        1 => is_digit(w[0]),
        2 if is_digit(w[0]) && is_digit(w[1]) => {
            let v = atoi(w);
            if mode != 0 {
                (0..100).contains(&v)
            } else if what == b'H' {
                (0..25).contains(&v)
            } else {
                (0..61).contains(&v)
            }
        }
        _ => false,
    }
}

/// `FUN_1004b840`: "at" right before, a paragraph start among the last five tokens, or a time word
/// around.
fn time_context(cx: &Cx, st: &mut State, cur: usize) -> bool {
    let mut k = 0;
    for t in st.done.toks.iter().rev() {
        if t.ty == 4 {
            continue;
        }
        if k == 0 && t.ty == 1 && stricmp(&t.text, b"at") == 0 {
            return true;
        }
        if k < 5 && t.f1 == 8 {
            return true;
        }
        k += 1;
    }
    if st.done.toks.is_empty() {
        return false;
    }
    context(cx, st, cur, TIME_WORDS)
}

struct T<'a, 'b> {
    cx: &'a Cx<'b>,
    st: &'a mut State,
    cur: &'a mut usize,
    sv: Save,
    mode: i32,
    tok: Tok,
}

impl T<'_, '_> {
    fn push(&mut self) -> Option<Tok> {
        let t = push(self.cx, self.st, self.cur)?;
        self.tok = t;
        Some(t)
    }

    fn pop(&mut self) {
        self.tok = pop(self.st, self.cur);
    }

    fn am_pm(&mut self) -> bool {
        let mut t = self.tok;
        let r = am_pm(self.cx, self.st, self.cur, &mut t, self.mode != 0);
        self.tok = t;
        r
    }

    fn zone(&mut self, m: u8) -> bool {
        let mut t = self.tok;
        let r = zone(self.cx, self.st, self.cur, &mut t, m, self.mode != 0);
        self.tok = t;
        r
    }
}

pub fn time(cx: &Cx, st: &mut State, cur: &mut usize, _t: &Tok) -> i32 {
    time_mode(cx, st, cur, 0)
}

pub fn time_mode(cx: &Cx, st: &mut State, cur: &mut usize, mode: i32) -> i32 {
    let first;
    if mode == 0 {
        first = cur_tok(st);
        if first.ty != 2 {
            return 5;
        }
    } else {
        if !(0x68..=0x6b).contains(&mode) {
            return 5;
        }
        st.ahead.reset();
        let Some(t) = push(cx, st, cur) else { return 5 };
        first = t;
    }
    let sv = save(st);
    let mut x = T { cx, st, cur, sv, mode, tok: first };
    match body(&mut x, first) {
        Some(r) => r,
        None => restore(x.st, x.cur, x.sv, 4),
    }
}

fn not_mine(x: &mut T) -> i32 {
    if x.st.ahead.n != x.sv.tn {
        x.st.ahead.n = x.sv.tn;
        if let Some(p) = x.st.ahead.pos[x.sv.tn as usize] {
            *x.cur = p;
        }
    }
    5
}

/// `None`: a push failed (roll back everything, 4).
fn body(x: &mut T, first: Tok) -> Option<i32> {
    let start = first.start;
    let mut mode = x.mode;
    let (mut hour, mut min, mut sec): (Option<Tok>, Option<Tok>, Option<Tok>) = (None, None, None);
    let (mut ok, mut suffix, mut after_at) = (false, false, false);
    let prev = token_at(&x.st.done, &x.st.ahead, x.st.ahead.n - 1, -1);
    let t = first;
    let mut as_hour = false;
    if let Some(p) = prev {
        if strcmp(&p.text, b"at") == 0 && t.ty == 2 && !field(&t, b'H', mode) {
            mode = 1;
            after_at = true;
            as_hour = true;
        }
    }
    if !as_hour {
        let h = field(&t, b'H', mode);
        if h && prev.is_none_or(|p| strcmp(&p.text, b":") != 0) {
            as_hour = true;
        } else if field(&t, b'M', mode) {
            // `MM:SS`
            min = Some(t);
            let c = x.push()?;
            if c.ws <= 2 && is_sym(&c, b':') {
                let s = x.push()?;
                if s.ws <= 2 && field(&s, b'S', mode) {
                    sec = Some(s);
                    let z = x.push()?;
                    if !(z.ws < 2 && is_sym(&z, b':')) {
                        ok = true;
                    }
                }
            }
        }
    }
    if as_hour {
        hour = Some(t);
        if !after_at && mode == 0x69 {
            ok = true;
            x.push()?;
        } else {
            if after_at {
                mode = 0;
            }
            let c = x.push()?;
            if c.ws > 2 || !is_sym(&c, b':') {
                ok = true;
            } else {
                let m = x.push()?;
                if m.ws < 3 && field(&m, b'M', mode) {
                    min = Some(m);
                    if mode == 0x6a {
                        ok = true;
                        x.push()?;
                    } else {
                        let z = x.push()?;
                        if z.ws < 3 && is_sym(&z, b':') {
                            let s = x.push()?;
                            if s.ws < 3 && field(&s, b'S', mode) {
                                sec = Some(s);
                                let v = x.push()?;
                                if !(v.ws < 2 && is_sym(&v, b':')) {
                                    ok = true;
                                    suffix = true;
                                }
                            }
                        } else if mode == 0 || mode == 0x68 {
                            ok = true;
                        }
                    }
                }
            }
        }
    }
    x.mode = mode;
    // LAB_1004ac19
    let mut tagged = false;
    if !ok {
        return Some(not_mine(x));
    }
    let t = x.tok;
    let mut done = false;
    if t.ws < 3 && t.ty == 1 && x.am_pm() {
        suffix = true;
        let z = x.push()?;
        if !(z.ws < 2 && z.ty == 1 && x.zone(b'D')) {
            x.pop();
        }
        done = true;
    } else if t.ws < 3 && t.ty == 1 && x.zone(b'I') {
        suffix = true;
        done = true;
    }
    if !done {
        // LAB_1004ad37
        let mut af88 = mode == 0;
        if mode != 0 {
            let mut five = t.ty == 5;
            if !five {
                if t.ty == 8 && t.text[0] == 0 {
                    let z = x.push()?;
                    if z.ty != 5 {
                        if !suffix {
                            return Some(not_mine(x));
                        }
                    } else {
                        five = true;
                    }
                } else {
                    af88 = true;
                }
            }
            if five {
                tagged = true;
                let z = x.push()?;
                let mut fin = false;
                if z.ws < 3 && z.ty == 1 && x.am_pm() {
                    suffix = true;
                    let w = x.push()?;
                    if !(w.ws < 2 && w.ty == 1 && x.zone(b'D')) {
                        x.pop();
                    }
                    fin = true;
                } else if z.ws < 3 && z.ty == 1 && x.zone(b'I') {
                    suffix = true;
                    fin = true;
                }
                if !fin {
                    x.pop();
                    suffix = true;
                }
            }
        }
        if af88 {
            x.pop();
            let piv = if hour.is_some() { min } else { sec };
            let no = mode == 0
                && ((hour.is_none() && min.is_none()) || piv.is_none() || !time_context(x.cx, x.st, *x.cur));
            if no {
                if !suffix {
                    return Some(not_mine(x));
                }
            } else {
                suffix = true;
            }
        }
    }
    // LAB_1004afc9
    if mode == 0 || tagged {
        if !suffix {
            return Some(not_mine(x));
        }
    } else {
        let mut z = x.push()?;
        if z.ty == 8 && z.text[0] == 0 {
            z = x.push()?;
        }
        if z.ty != 5 && z.ty != 9 {
            return Some(not_mine(x));
        }
    }
    let r = x.st.done.append_all(&x.st.ahead, 0xb);
    if r != 1 {
        x.st.ahead.n = x.sv.tn;
        if let Some(p) = x.st.ahead.pos[x.sv.tn as usize] {
            *x.cur = p;
        }
        x.st.done.toks.truncate(x.sv.pn);
        return Some(r);
    }
    let end = sec.or(min).or(hour).map_or(-1, |t| t.end);
    let emit = |x: &mut T, w: &[u8]| -> Option<i32> {
        let r = x.st.wa.emit_words(w, start, end, b'D', b'D');
        if r != 1 {
            return Some(restore(x.st, x.cur, x.sv, r));
        }
        None
    };
    if let Some(h) = hour {
        if let Some(r) = emit(x, &num::cardinal(&h.text, 1)) {
            return Some(r);
        }
    }
    let mut say_sec = true;
    if let Some(m) = min {
        let a = atoi(&m.text);
        let s0 = sec.map(|s| atoi(&s.text)).unwrap_or(0);
        if a > 0 || (a == 0 && sec.is_some() && s0 != 0) {
            if let Some(r) = emit(x, &num::cardinal(&m.text, 1)) {
                return Some(r);
            }
        } else if !((a != 0 || sec.is_some()) && (a != 0 || s0 != 0)) {
            if x.st.q.n() == 0 && !after_at {
                if let Some(r) = emit(x, b"o'clock") {
                    return Some(r);
                }
            }
            say_sec = false;
        }
    }
    if let (Some(s), true) = (sec, say_sec) {
        if let Some(r) = emit(x, &num::cardinal(&s.text, 1)) {
            return Some(r);
        }
    }
    let r = x.st.q.flush(&mut x.st.wa);
    if r == 1 {
        return Some(1);
    }
    Some(restore(x.st, x.cur, x.sv, r))
}
