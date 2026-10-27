//! `norm_mathsym` (0x1003d1d0): `= + * / -` and `×` between numbers, units and words ("equal",
//! "plus minus", "per hour", "slash", "minus", "dash", "to"), the fraction glyphs and fractions
//! written `3/4` ("three quarters", with a unit after them).

use super::mem::*;
use super::norm_units::{dbcs_symbol, is_sym, lookup, lookup_tok, op_x, unitish, ALL, ALL_LONG};
use super::normalize::{Cx, State};
use super::num;
use super::rx::*;
use super::stream::token_at;
use super::tokenize::Tok;
use super::wa::Wa;

/// Fraction glyphs (`0x10073440`): "one half", ... at +0x10.
const FRAC: Rec30 = rec30(0x10073440, 0x10084874);
/// Currency signs read after `-` (`-$5`), 0x60-byte unit records.
const CURRENCY: Rec60 = rec60(0x1006fe20, 0x1008482c);

fn opc(t: &Tok) -> bool {
    t.len == 1 && t.text[0] != 0 && b"-+*/=xX".contains(&t.text[0])
}

fn one_of(t: &Tok, set: &[u8]) -> bool {
    t.len == 1 && t.text[0] != 0 && set.contains(&t.text[0])
}

fn frac_glyph(t: &Tok) -> bool {
    dbcs_symbol(&t.text) && FRAC.find(&t.text, false) >= 0
}

/// `FUN_10050720`: the WA record ending at `end`, searching back.
fn wa_ending(wa: &Wa, end: i32) -> i32 {
    if end < 0 {
        return -1;
    }
    let mut k = wa.n() - 1;
    while k >= 0 {
        let e = wa.ri32(k, 4);
        if e < end {
            return -1;
        }
        if e == end {
            return k;
        }
        k -= 1;
    }
    -1
}

/// `FUN_10050760`: the WA record spanning exactly `t`.
pub fn wa_spanning(wa: &Wa, t: &Tok) -> i32 {
    for k in 0..wa.n() {
        if wa.ri32(k, 0) == t.start && wa.ri32(k, 4) == t.end {
            return k;
        }
        if t.start < wa.ri32(k, 0) {
            return -1;
        }
    }
    -1
}

/// The engine marks a joined `-` with a short 1 eight bytes before record `k` (the previous
/// record's +0x80, or the WA header for the first).
fn mark_joined(wa: &mut Wa, k: i32) {
    let o = (0xc + k * 0x88) as usize;
    wa.b[o..o + 2].copy_from_slice(&1i16.to_le_bytes());
}

struct M<'a, 'b> {
    cx: &'a Cx<'b>,
    st: &'a mut State,
    cur: &'a mut usize,
    sv: Save,
    start: i32,
    tok: Tok,
    word: Vec<u8>,
    done: bool,
    cls: i32,
    frac: Option<(Tok, Tok)>,
    and: bool,
    unit: Vec<u8>,
}

enum F {
    E5ac,
    E6fd,
    E4e7,
    E5f0,
    Done,
}

impl M<'_, '_> {
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

    fn fail(&mut self) -> i32 {
        restore(self.st, self.cur, self.sv, 4)
    }

    fn set(&mut self, w: &[u8], cls: i32) {
        self.word = w.to_vec();
        self.cls = cls;
        self.done = true;
    }

    fn back(&self, rel: i32) -> Option<Tok> {
        token_at(&self.st.done, &self.st.ahead, self.st.ahead.n - 1, rel)
    }

    /// `x` after a two-byte `×`.
    fn times(&mut self, prev: Option<Tok>, prev2: Option<Tok>) -> Option<()> {
        let Some(p) = prev else {
            self.cls = self.sv.wn;
            return Some(());
        };
        let nx = self.push()?;
        if nx.ws < 3 && num::is_number(&p.text) && num::is_number(&nx.text) {
            self.set(b"by", 0x15);
        } else if !unitish(&p) || lookup(ALL, &p.text, false, 0).is_none() {
            self.cls = self.sv.wn;
        } else if nx.ws < 3 && unitish(&nx) && lookup(ALL, &nx.text, false, 0).is_some() && p.len > 1 && nx.len > 1 {
            self.set(b"by", 0x15);
        } else if prev2.is_none_or(|q| !num::is_number(&q.text)) || p.ws > 2 || nx.ws > 2 || !num::is_number(&nx.text) {
            self.cls = self.sv.wn;
        } else {
            let n2 = self.push()?;
            if n2.ws < 3 && unitish(&n2) && lookup(ALL, &n2.text, false, 0).is_some() {
                self.set(b"by", 0x15);
            } else {
                self.cls = self.sv.wn;
            }
            self.pop();
        }
        self.pop();
        Some(())
    }

    /// `-` after a number, word, or nothing.
    fn dash(&mut self, d: Tok, prev: Option<Tok>, prev2: Option<Tok>) -> Option<()> {
        let nx = self.push()?;
        if nx.ws <= 2 && CURRENCY.find_prefix(&nx.text, b'S') >= 0 {
            let n2 = self.push()?;
            if n2.ws <= 2 && n2.ty == 2 {
                if nx.ws == 0 {
                    self.set(b"minus", 0x15);
                } else {
                    self.set(b"dash", 10);
                }
                self.pop();
                self.pop();
                return Some(());
            }
            self.pop();
        }
        // LAB_1003d9dd
        match prev {
            None => {
                if !num::is_number(&nx.text) {
                    self.cls = self.sv.wn;
                    self.done = true;
                } else if nx.ws == 0 {
                    self.set(b"minus", 0x15);
                } else {
                    self.set(b"dash", 10);
                }
            }
            Some(p) if p.ty == 2 => {
                if nx.ty == 3 && nx.ws < 3 && frac_glyph(&nx) {
                    self.set(b"minus", 0x15);
                } else if nx.ty != 2 {
                    self.cls = 0;
                    self.done = true;
                    if 0 < d.ws && d.ws < 3 && nx.ty == 1 && 0 < nx.ws && nx.ws < 3 {
                        self.set(b",", 0x15);
                    }
                    if d.ws == 0 && nx.ws == 0 {
                        let k = wa_ending(&self.st.wa, p.end);
                        if k >= 0 {
                            mark_joined(&mut self.st.wa, k);
                        }
                    }
                } else if num::is_number(&p.text) && num::is_number(&nx.text) {
                    self.numbers(d, p, nx, prev2)?;
                } else {
                    self.set(b"dash", 10);
                }
            }
            Some(p) => {
                if !(d.ws < 1 || nx.ws != 0 || !num::is_number(&nx.text)) {
                    self.set(b"minus", 0x15);
                } else if d.ws < 3 && nx.ws < 3 && num::is_number(&nx.text) && p.f1 == 9 {
                    self.set(b"dash", 10);
                } else if (p.ty == 1 || nx.ws != 0 || !num::is_number(&nx.text))
                    && (prev2.is_none_or(|q| q.ty != 2) || p.len != 1 || fold_s(p.text[0]) != b'e' as i32 || d.ws > 1 || nx.ws > 1)
                {
                    if p.ty == 1 && (nx.ty == 1 || nx.ty == 2) && 0 < d.ws && d.ws < 3 && 0 < nx.ws && nx.ws < 3 {
                        self.set(b",", 0x15);
                    } else if d.ws == 0 && nx.ws == 0 {
                        let k = wa_spanning(&self.st.wa, &p);
                        if k >= 0 {
                            mark_joined(&mut self.st.wa, k);
                        }
                        self.cls = self.sv.wn;
                        self.done = true;
                    } else if one_of(&p, b",;%$#~") && 0 < d.ws && d.ws < 3 && 0 < nx.ws && nx.ws < 3 {
                        self.set(b"dash", 0x15);
                    } else {
                        self.cls = self.sv.wn;
                        self.done = true;
                    }
                } else {
                    self.set(b"minus", 0x15);
                }
            }
        }
        self.pop();
        Some(())
    }

    /// `N-M` between numbers: "minus" in arithmetic, "to" in a rising range, otherwise "dash".
    fn numbers(&mut self, d: Tok, p: Tok, nx: Tok, prev2: Option<Tok>) -> Option<()> {
        let n3 = self.push()?;
        if d.ws < 3 && nx.ws < 3 && n3.ws < 3 && opc(&n3) && p.f1 != 8 {
            self.set(b"minus", 0x15);
            self.pop();
            return Some(());
        }
        if n3.ws < 3 && frac_glyph(&n3) {
            self.set(b"minus", 0x15);
            self.pop();
            return Some(());
        }
        if d.ws < 3 && nx.ws < 3 && n3.ws < 3 && num::is_number(&n3.text) {
            let n4 = self.push()?;
            if n4.ws == 0 && is_sym(&n4, b'/') {
                let n5 = self.push()?;
                if n5.ws == 0 && num::is_number(&n5.text) {
                    self.set(b"minus", 0x15);
                    self.seek(-3);
                    return Some(());
                }
                self.seek(-3);
            } else {
                self.seek(-2);
            }
        } else {
            self.pop();
        }
        let chained = prev2.is_some_and(|q| opc(&q))
            && num::is_number(&p.text)
            && p.ws == d.ws
            && d.ws <= 2
            && d.ws == nx.ws
            && nx.ws <= 2;
        if chained {
            self.set(b"minus", 0x15);
            return Some(());
        }
        let x = push(self.cx, self.st, self.cur);
        if let Some(x) = x {
            self.tok = x;
        }
        match x {
            Some(x) if x.ty != 2 => {
                let to = strcmp(&x.text, b"points") == 0
                    || strcmp(&x.text, b"run") == 0
                    || !(atoi(&nx.text) <= atoi(&p.text) || p.ws < 1 || p.ws != nx.ws);
                if to {
                    self.set(b"to", 10);
                } else {
                    self.cls = self.sv.wn;
                }
            }
            _ => self.cls = self.sv.wn,
        }
        self.pop();
        if !self.done {
            self.set(b"dash", 10);
        }
        Some(())
    }

    /// `LAB_1003e1e6` for a number: `3/4` (with what is around it) as a fraction.
    fn fraction(&mut self) -> Option<bool> {
        let numt = self.tok;
        let p6 = self.back(-1);
        let p7 = self.back(-2);
        let x = self.push()?;
        let (sl, den) = if is_sym(&x, b'/') {
            let d = self.push()?;
            if d.ty == 2 && num::is_number(&d.text) {
                (x, d)
            } else {
                self.seek(-2);
                return Some(false);
            }
        } else {
            self.pop();
            return Some(false);
        };
        let mut at = match (p7, p6) {
            (None, _) => F::E5ac,
            (_, None) => F::E6fd,
            (Some(q7), Some(q6)) => {
                if q7.ty == 2 && num::is_number(&q7.text) {
                    if q6.ws > 2 {
                        F::E5ac
                    } else if !opc(&q6) || numt.ws > 2 || sl.ws > 2 || den.ws == 0 {
                        F::E4e7
                    } else {
                        self.set(b"", 0x15);
                        F::Done
                    }
                } else {
                    F::E4e7
                }
            }
        };
        loop {
            at = match at {
                F::E4e7 => {
                    let (q6, q7) = (p6.unwrap(), p7.unwrap());
                    if q6.ws < 3 {
                        if one_of(&q6, b"-+") && numt.ws < 3 && sl.ws == 0 && den.ws == 0 {
                            self.set(b"", 0xe);
                            F::Done
                        } else if q6.ws > 2
                            || numt.ws < 1
                            || numt.ws > 2
                            || sl.ws != 0
                            || den.ws != 0
                            || !one_of(&q7, b"-+")
                            || !num::is_number(&q6.text)
                        {
                            F::E5ac
                        } else {
                            F::E5f0
                        }
                    } else {
                        F::E5ac
                    }
                }
                F::E5f0 => {
                    self.and = true;
                    self.set(b"", 0xe);
                    F::Done
                }
                F::E5ac => match p6 {
                    None => F::E6fd,
                    Some(q6) => {
                        if q6.ty == 2 && num::is_number(&q6.text) && 0 < numt.ws && numt.ws < 3 && sl.ws == 0 && den.ws == 0 {
                            F::E5f0
                        } else if !one_of(&q6, b"-+") {
                            F::E6fd
                        } else {
                            let y = self.push()?;
                            if y.ws == 1 && !opc(&y) && numt.ws < 3 && sl.ws == 0 && den.ws == 0 {
                                self.set(b"", 0xe);
                            }
                            self.pop();
                            if self.done {
                                F::Done
                            } else {
                                F::E6fd
                            }
                        }
                    }
                },
                F::E6fd => {
                    let y = self.push()?;
                    if sl.ws > 2 || den.ws > 2 || y.ws > 2 || !opc(&y) {
                        if sl.ws == 0 {
                            let pl = 1.0 <= num::atof(&numt.text) / num::atof(&den.text);
                            if den.ws == 0 && y.ws > 0 && y.ty == 1 {
                                self.set(b"", 0xe);
                                self.unit = self.lookup_unit(pl);
                                if self.unit.is_empty() {
                                    self.pop();
                                }
                                F::Done
                            } else if den.ws != 0 || y.ws > 2 {
                                self.pop();
                                self.seek(-2);
                                return Some(false);
                            } else {
                                if unitish(&y) {
                                    self.unit = self.lookup_unit(pl);
                                    if !self.unit.is_empty() {
                                        self.set(b"", 0xe);
                                    }
                                }
                                if self.done {
                                    F::Done
                                } else {
                                    self.seek(-2);
                                    return Some(false);
                                }
                            }
                        } else {
                            self.pop();
                            self.seek(-2);
                            return Some(false);
                        }
                    } else {
                        self.set(b"", 0x15);
                        self.pop();
                        F::Done
                    }
                }
                F::Done => break,
            };
        }
        self.frac = Some((numt, den));
        Some(true)
    }

    fn lookup_unit(&mut self, plural: bool) -> Vec<u8> {
        let mut t = self.tok;
        let r = lookup_tok(self.cx, self.st, self.cur, ALL_LONG, &mut t, plural, 0);
        self.tok = t;
        r.unwrap_or_default()
    }

    /// `LAB_1003e1e6` / `LAB_1003e2aa`.
    fn finish(&mut self) -> i32 {
        if !self.done {
            let t = self.tok;
            let k = if t.len == 2 && dbcs_symbol(&t.text) { FRAC.find(&t.text, false) } else { -1 };
            if k >= 0 {
                let p = self.back(-1);
                if p.is_some_and(|p| t.ws < 2 && num::is_number(&p.text)) {
                    self.word = b"and".to_vec();
                }
                num::cat(&mut self.word, FRAC.field(k, 0x10));
                self.cls = 0xe;
            } else {
                let ok = if t.ty == 2 && num::is_number(&t.text) {
                    match self.fraction() {
                        None => return self.fail(),
                        Some(ok) => ok,
                    }
                } else {
                    false
                };
                if !ok {
                    if self.st.ahead.n != self.sv.tn {
                        self.st.ahead.n = self.sv.tn;
                        if let Some(p) = self.st.ahead.pos[self.sv.tn as usize] {
                            *self.cur = p;
                        }
                    }
                    return 5;
                }
            }
        }
        let r = self.st.done.append_all(&self.st.ahead, self.cls);
        if r != 1 {
            self.st.done.toks.truncate(self.sv.pn);
            self.st.ahead.n = self.sv.tn;
            if let Some(p) = self.st.ahead.pos[self.sv.tn as usize] {
                *self.cur = p;
            }
            return r;
        }
        let (text, c18) = match self.frac {
            None => {
                if self.word.is_empty() {
                    return 1;
                }
                (self.word.clone(), b'S')
            }
            Some((n, d)) => {
                let mut s = Vec::new();
                if self.cls == 0xe {
                    let f = num::fraction(&n.text, &d.text);
                    s.extend_from_slice(if self.and { b" and " } else { b" " });
                    s.extend_from_slice(&f);
                    if !self.unit.is_empty() {
                        s.push(b' ');
                        s.extend_from_slice(&self.unit);
                    }
                } else {
                    s = num::cardinal(&n.text, 1);
                    s.extend_from_slice(b" slash ");
                    s.extend_from_slice(&num::cardinal(&d.text, 1));
                }
                (s, b'D')
            }
        };
        let r = self.st.wa.emit_words(&text, self.start, self.tok.end, c18, b'D');
        if r != 1 {
            return restore(self.st, self.cur, self.sv, r);
        }
        1
    }
}

pub fn mathsym(cx: &Cx, st: &mut State, cur: &mut usize, _t: &Tok) -> i32 {
    let t0 = cur_tok(st);
    if !opc(&t0) && !dbcs_symbol(&t0.text) && t0.ty != 2 {
        return 5;
    }
    let sv = save(st);
    let mut m = M {
        cx,
        st,
        cur,
        sv,
        start: t0.start,
        tok: t0,
        word: Vec::new(),
        done: false,
        cls: 0,
        frac: None,
        and: false,
        unit: Vec::new(),
    };
    let prev = m.back(-1);
    let prev2 = m.back(-2);
    let ok = if is_sym(&t0, b'=') {
        m.set(b"equal", 0x15);
        Some(())
    } else if is_sym(&t0, b'+') {
        m.word = b"plus".to_vec();
        match m.push() {
            None => None,
            Some(nx) => {
                if nx.ws == 0 && is_sym(&nx, b'-') {
                    num::cat(&mut m.word, b"minus");
                } else {
                    m.pop();
                }
                m.done = true;
                m.cls = 0x15;
                Some(())
            }
        }
    } else if is_sym(&t0, b'*') {
        if prev.is_none_or(|p| p.ty != 2) || t0.ws > 2 {
            m.set(b"", 0x15);
            Some(())
        } else {
            match m.push() {
                None => None,
                Some(nx) => {
                    if nx.ty == 2 && nx.ws < 3 {
                        m.word = b"asterisk".to_vec();
                    }
                    m.pop();
                    m.done = true;
                    m.cls = 0x15;
                    Some(())
                }
            }
        }
    } else if t0.ws < 3 && t0.len == 2 && strcmp(&t0.text, op_x()) == 0 {
        m.times(prev, prev2)
    } else if is_sym(&t0, b'/') {
        match m.push() {
            None => None,
            Some(nx) => {
                let mut per = false;
                if let Some(p) = prev {
                    if p.ty == 1 && nx.ty == 1 && t0.ws == 0 && nx.ws == 0 {
                        m.set(b"", 0x15);
                        m.pop();
                        per = true;
                    } else if p.ty == 2 && t0.ws < 2 && nx.ws < 2 && nx.ty == 1 {
                        if let Some(u) = lookup(ALL, &nx.text, num::plural(&p.text), 0) {
                            m.set(&[&b"per "[..], &u[..]].concat(), 9);
                            per = true;
                        }
                    }
                }
                if !per {
                    m.set(b"slash", 0x15);
                    m.pop();
                }
                Some(())
            }
        }
    } else if is_sym(&t0, b'-') {
        m.dash(t0, prev, prev2)
    } else {
        m.cls = m.sv.wn;
        Some(())
    };
    if ok.is_none() {
        return m.fail();
    }
    m.finish()
}
