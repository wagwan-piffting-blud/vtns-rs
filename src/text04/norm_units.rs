//! `norm_units` (0x100375b0): a unit word or symbol, usually after a number: `5 km`, `3 fl. oz`,
//! `9.8 m/s2`, `10 kWh`, `20 sq ft`, `-5 C` (by the words around it). Singular or plural follows the
//! number before it.
//!
//! The engine builds the reading in a 32-byte buffer that sits right below two flags and the unit
//! buffer on its stack; `Frame` keeps that layout so long readings spill the way they do there.

use super::mem::*;
use super::normalize::{Cx, State};
use super::num;
use super::rx::*;
use super::stream::token_at;
use super::tokenize::Tok;

const T_SYM: Rec60 = rec60(0x10074ee0, 0x10084888);
const T_ABBR: Rec60 = rec60(0x100771c0, 0x1008488c);
const T_CASE: Rec60 = rec60(0x100778e0, 0x10084890);
const T_LOW: Rec60 = rec60(0x10078540, 0x10084894);
const T_PA: Rec60 = rec60(0x1007a040, 0x10084898);
const T_ONE: Rec60 = rec60(0x1007a100, 0x1008489c);
const T_ONE_CS: Rec60 = rec60(0x1007a6a0, 0x100848a0);
const T_WORDS: Rec60 = rec60(0x1007a9a0, 0x100848a4);

/// The unit groups of `FUN_100367a0` (`ctx+0x48..0x54`) with their lookup modes.
pub type Group = &'static [(Rec60, u8)];
pub const ALL: Group = &[
    (T_SYM, b'I'),
    (T_ABBR, b'I'),
    (T_CASE, b'S'),
    (T_LOW, b'I'),
    (T_PA, b'S'),
    (T_ONE, b'I'),
    (T_ONE_CS, b'S'),
    (T_WORDS, b'M'),
];
/// `ALL` with the one-letter tables switched off (the fraction reader's unit check).
pub const ALL_LONG: Group = &[(T_SYM, b'I'), (T_ABBR, b'I'), (T_CASE, b'S'), (T_LOW, b'I'), (T_PA, b'S'), (T_WORDS, b'M')];
const COMMON: Group =&[(T_SYM, b'I'), (T_ABBR, b'I'), (T_CASE, b'S'), (T_WORDS, b'M')];
const LONG: Group = &[(T_LOW, b'I'), (T_PA, b'S'), (T_WORDS, b'M')];
const SHORT: Group = &[(T_ONE, b'I'), (T_ONE_CS, b'S'), (T_WORDS, b'M')];

/// Superscript powers (`0x100735f0`): "to the first", "squared", ... at +0x10.
pub const POWERS: Rec30 = rec30(0x100735f0, 0x10084878);

const TEMPERATURE: Ptab = ptab(0x10081c2c, 0x100848b4);
const FORCE: Ptab = ptab(0x10081c1c, 0x100848b0);
const WEIGHT: Ptab = ptab(0x10081c00, 0x100848a8);
const BULB: Ptab = ptab(0x10081c18, 0x100848ac);

fn op_by() -> &'static [u8] {
    s(0x1006f40c)
}

pub fn op_x() -> &'static [u8] {
    s(0x1006f408)
}

fn degree_sign() -> &'static [u8] {
    s(0x1006f4ac)
}

fn up_s(c: u8) -> i32 {
    let m = b(0x1006ac60, 768);
    let o = (256 + 2 * c as i8 as i32) as usize;
    i16::from_le_bytes([m[o], m[o + 1]]) as i32
}

pub fn is_sym(t: &Tok, c: u8) -> bool {
    (t.ty == 3 || t.ty == 7) && t.len == 1 && t.text[0] == c
}

/// `FUN_100500d0`: starts with a two-byte EUC-KR symbol.
pub fn dbcs_symbol(t: &[u8]) -> bool {
    let t = cs(t);
    if t.len() < 2 {
        return false;
    }
    let (a, b2) = (t[0], t[1]);
    (0xa0 < a && a < 0xae && 0xa0 < b2 && b2 != 0xff) || (a == 0xae && 0xa0 < b2 && b2 < 0xc3)
}

/// `FUN_10039240`.
pub fn unitish(t: &Tok) -> bool {
    match t.ty {
        1 => true,
        3 => (t.len == 1 && t.text[0] == b'%') || dbcs_symbol(&t.text),
        _ => false,
    }
}

/// `FUN_10039280`: the singular (or the key), the plural, or the singular plus a `-suffix`.
fn pick(plural: bool, key: &[u8], sing: &[u8], plur: &[u8]) -> Vec<u8> {
    let base = if sing.is_empty() { key } else { sing };
    if plural && !plur.is_empty() {
        if plur[0] == b'-' {
            let mut v = base.to_vec();
            v.extend_from_slice(&plur[1..]);
            return v;
        }
        return plur.to_vec();
    }
    base.to_vec()
}

fn pick_rec(t: Rec60, i: i32, plural: bool) -> Vec<u8> {
    pick(plural, t.field(i, 0), t.field(i, 0x20), t.field(i, 0x40))
}

/// `FUN_10039310`: `w` in a group.
pub fn lookup(g: Group, w: &[u8], plural: bool, mode: u8) -> Option<Vec<u8>> {
    let mut pl = plural;
    for &(t, m) in g {
        let i = match m {
            b'I' => t.find_prefix(w, b'I'),
            b'S' => t.find_exact(w, mode),
            _ => {
                pl = false;
                let i = t.find_prefix(w, m);
                if i < 0 {
                    continue;
                }
                if stricmp(w, t.field(i, 0)) != 0 {
                    pl = true;
                }
                i
            }
        };
        if i >= 0 {
            return Some(pick_rec(t, i, pl));
        }
    }
    None
}

/// `FUN_10049df0`: bisection on keys whose first word is `tok`; a multiword key reads the following
/// tokens (left in the buffer when it matches). `tok` follows the buffer's last token.
fn find_multi(cx: &Cx, st: &mut State, cur: &mut usize, t: Rec60, tok: &mut Tok) -> i32 {
    let w = tok.text().to_vec();
    if w.is_empty() {
        return -1;
    }
    let l = w.len();
    let (mut lo, mut hi) = (0, t.count() - 1);
    let mut mid = hi;
    while lo <= hi {
        mid /= 2;
        let key = t.field(mid, 0);
        let mut c = strncmp(key, &w, l);
        if c == 0 {
            match at(key, l as isize) {
                0 => return mid,
                b' ' => {
                    let mut rest: &[u8] = &key[l + 1..];
                    let mut k = 0;
                    c = 1;
                    loop {
                        let Some(n) = push(cx, st, cur) else { break };
                        *tok = n;
                        k += 1;
                        if n.len < 1 {
                            break;
                        }
                        let d = strncmp(rest, &n.text, n.len as usize);
                        if d != 0 {
                            c = d;
                            break;
                        }
                        match at(rest, n.len as isize) {
                            0 => return mid,
                            b' ' => rest = &rest[n.len as usize + 1..],
                            _ => break,
                        }
                    }
                    *tok = seek(st, cur, -k);
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

/// `FUN_10039420`: the buffer's last token `tok` in a group (`M` tables may read on).
pub fn lookup_tok(cx: &Cx, st: &mut State, cur: &mut usize, g: Group, tok: &mut Tok, plural: bool, mode: u8) -> Option<Vec<u8>> {
    for &(t, m) in g {
        let i = match m {
            b'I' => t.find_prefix(&tok.text, b'I'),
            b'S' => t.find_exact(&tok.text, mode),
            _ => find_multi(cx, st, cur, t, tok),
        };
        if i >= 0 {
            return Some(pick_rec(t, i, plural));
        }
    }
    None
}

/// `FUN_10049f90` with mode `I`, `B`: one of the five words before the last token, or one of the five
/// after it, starts an entry of `tab` (the following words must match the rest).
pub fn context(cx: &Cx, st: &mut State, cur: usize, tab: Ptab) -> bool {
    let mut before: Vec<Vec<u8>> = Vec::new();
    let n = st.ahead.n;
    let mut k = n - 2;
    while k >= 0 && before.len() < 5 {
        before.push(st.ahead.toks[k as usize].text().to_vec());
        k -= 1;
    }
    let mut k = st.done.n() - 1;
    while k >= 0 && before.len() < 5 {
        before.push(st.done.toks[k as usize].text().to_vec());
        k -= 1;
    }
    before.reverse();
    if find_words(tab, &before, true) {
        return true;
    }
    let mut after = Vec::new();
    let mut c = cur;
    for _ in 0..5 {
        let Some(i) = st.ahead.push(cx.t, &mut c, cx.base, b'D', cx.fe.ud.as_ref()) else { break };
        let t = st.ahead.toks[i];
        if t.ty == 8 || t.ty == 9 {
            st.ahead.n -= 1;
            break;
        }
        after.push(t.text().to_vec());
    }
    st.ahead.n = n;
    find_words(tab, &after, false)
}

fn find_words(tab: Ptab, words: &[Vec<u8>], skip_empty: bool) -> bool {
    for (i, w) in words.iter().enumerate() {
        let l = w.len();
        if skip_empty && l == 0 {
            continue;
        }
        let (mut lo, mut hi) = (0, tab.count() - 1);
        let mut mid = hi;
        while lo <= hi {
            mid /= 2;
            let key = tab.get(mid);
            let c = strnicmp(key, w, l);
            if c == 0 {
                match at(key, l as isize) {
                    0 => return true,
                    b' ' => {
                        let mut rest: &[u8] = &key[l + 1..];
                        for w2 in &words[i + 1..] {
                            let l2 = w2.len();
                            if skip_empty && l2 == 0 {
                                continue;
                            }
                            // The engine returns on the end of the entry whatever the compare said.
                            match at(rest, l2 as isize) {
                                0 => return true,
                                b' ' => rest = &rest[(l2 + 1).min(rest.len())..],
                                _ => break,
                            }
                        }
                    }
                    _ => {}
                }
                hi = mid - 1;
            } else if c < 0 {
                lo = mid + 1;
            } else {
                hi = mid - 1;
            }
            mid = hi + lo;
        }
    }
    false
}

/// `FUN_10039520`: units decided by the words around a one-letter symbol: `C`/`F` after a degree,
/// `K`/`R` (temperature), `m` (length), `N` (force), `g`/`t` (weight), `W` (bulb). With `keep` the
/// unit read so far stands when none applies.
fn special(cx: &Cx, st: &mut State, cur: usize, unit: &[u8], plural: bool, keep: bool) -> Option<Vec<u8>> {
    let saved = if keep && !unit.is_empty() { Some(unit.to_vec()) } else { None };
    if st.ahead.n < 1 {
        return None;
    }
    let mut last = cur_tok(st);
    let prev = token_at(&st.done, &st.ahead, st.ahead.n - 1, -1);
    let named = |w: &[u8]| pick(plural, b"", w, b"-s");
    let c0 = last.text[0];
    if prev.is_some() && last.len == 1 && (fold_s(c0) == b'c' as i32 || fold_s(c0) == b'f' as i32) {
        let p = prev.unwrap();
        let after_degree = (last.ws == 0 && strcmp(&p.text, degree_sign()) == 0)
            || (last.ws < 2 && (stricmp(&p.text, b"degrees") == 0 || stricmp(&p.text, b"degree") == 0));
        if after_degree {
            return Some(named(if fold_s(c0) == b'c' as i32 { b"celsius" } else { b"fahrenheit" }));
        }
        if !keep {
            return None;
        }
    }
    if last.ws < 3 {
        let u = up_s(c0);
        if last.len == 1 && (u == b'K' as i32 || u == b'R' as i32) && context(cx, st, cur, TEMPERATURE) {
            return Some(named(if u == b'K' as i32 { b"kelvin" } else { b"reaumur" }));
        }
        if last.len == 1 && fold_s(c0) == b'm' as i32 {
            let length = st.done.toks.iter().any(|t| strcmp(&t.text, b"length") == 0);
            if length {
                return Some(named(b"meter"));
            }
            // Read with a private cursor that also serves as the offset base.
            let mut c = cur;
            let i = st.ahead.push(cx.t, &mut c, cur, b'D', cx.fe.ud.as_ref())?;
            if strcmp(&st.ahead.toks[i].text, b"long") == 0 {
                return Some(named(b"meter"));
            }
            st.ahead.pop(&mut c);
            last = cur_tok(st);
            if !keep {
                return None;
            }
        }
        if last.ws < 3 {
            let c0 = last.text[0];
            if last.len == 1 && up_s(c0) == b'N' as i32 && context(cx, st, cur, FORCE) {
                return Some(named(b"newton"));
            }
            let f = fold_s(c0);
            if last.len == 1 && (f == b'g' as i32 || f == b't' as i32) && context(cx, st, cur, WEIGHT) {
                return Some(named(if f == b'g' as i32 { b"gram" } else { b"ton" }));
            }
            if last.ws < 3 && last.len == 1 && f == b'w' as i32 && context(cx, st, cur, BULB) {
                return Some(named(b"watt"));
            }
        }
    }
    saved
}

/// The engine's stack from the reading buffer up: reading (32), `local_64`, `local_60`, unit (32).
struct Frame {
    b: [u8; 72],
}

impl Frame {
    fn reading(&self) -> Vec<u8> {
        cs(&self.b).to_vec()
    }

    fn set_reading(&mut self, v: &[u8]) {
        let n = v.len().min(71);
        self.b[..n].copy_from_slice(&v[..n]);
        self.b[n] = 0;
    }

    /// `FUN_1004f930`: append with a blank, or copy into an empty reading.
    fn add(&mut self, w: &[u8]) {
        let mut r = self.reading();
        if !r.is_empty() {
            r.push(b' ');
        }
        r.extend_from_slice(cs(w));
        self.set_reading(&r);
    }

    fn flag(&self, o: usize) -> bool {
        self.b[o] != 0 || self.b[o + 1] != 0
    }

    fn set_flag(&mut self, o: usize, v: bool) {
        self.b[o..o + 4].copy_from_slice(&(v as i32).to_le_bytes());
    }

    fn unit(&self) -> Vec<u8> {
        cs(&self.b[40..]).to_vec()
    }

    fn set_unit(&mut self, v: &[u8]) {
        let n = v.len().min(31);
        self.b[40..40 + n].copy_from_slice(&v[..n]);
        self.b[40 + n] = 0;
    }
}

const F64: usize = 32;
const F60: usize = 36;

struct U<'a, 'b> {
    cx: &'a Cx<'b>,
    st: &'a mut State,
    cur: &'a mut usize,
    sv: Save,
    mode: u8,
    start: i32,
    tok: Tok,
    prev: Option<Tok>,
    plural: bool,
    numb: bool,
    dash: bool,
    art: bool,
    from_queue: bool,
    f: Frame,
}

/// The power phrase for superscript `k` after `unit`.
fn power(k: i32, unit: &[u8]) -> Vec<u8> {
    match k {
        0 => unit.to_vec(),
        1 => [&b"square "[..], unit].concat(),
        2 => [&b"cubic "[..], unit].concat(),
        _ => [unit, b" ", POWERS.field(k, 0x10)].concat(),
    }
}

/// `reading + power phrase` after an operator word.
fn power_after(r: &[u8], k: i32, unit: &[u8], square: &[u8]) -> Vec<u8> {
    match k {
        1 => [r, b" ", square, b" ", unit].concat(),
        2 => [r, b" cubic ", unit].concat(),
        _ => [r, b" ", unit, b" ", POWERS.field(k, 0x10)].concat(),
    }
}

fn is_op(t: &Tok) -> bool {
    (t.len == 1 && t.text[0] != 0 && b"/*xX".contains(&t.text[0]))
        || (t.len == 2 && (strcmp(&t.text, op_by()) == 0 || strcmp(&t.text, op_x()) == 0))
}

impl U<'_, '_> {
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

    /// `LAB_10039195`.
    fn not_mine(&mut self) -> i32 {
        if self.st.ahead.n != self.sv.tn {
            self.st.ahead.n = self.sv.tn;
            if let Some(p) = self.st.ahead.pos[self.sv.tn as usize] {
                *self.cur = p;
            }
        }
        5
    }

    fn power_k(&self) -> i32 {
        if self.tok.ws == 0 {
            POWERS.find(&self.tok.text, false)
        } else {
            -1
        }
    }

    fn lookup_tok(&mut self, g: Group, plural: bool, mode: u8) -> Option<Vec<u8>> {
        let mut t = self.tok;
        let r = lookup_tok(self.cx, self.st, self.cur, g, &mut t, plural, mode);
        self.tok = t;
        r
    }

    /// `/2 h`: a number then a unit after `/` (looked up into the scratch buffer).
    fn per_number(&mut self, plural: bool) -> bool {
        let Some(mut t) = push(self.cx, self.st, self.cur) else { return false };
        lookup_tok(self.cx, self.st, self.cur, ALL, &mut t, plural, 0).is_some()
    }

    /// `LAB_10037f88`.
    fn emit(&mut self) -> i32 {
        let mut r = self.st.done.append_all(&self.st.ahead, 9);
        if r == 1 {
            r = if self.from_queue {
                self.st.q.flush(&mut self.st.wa)
            } else {
                let rd = self.f.reading();
                self.st.wa.emit_words(&rd, self.start, self.tok.end, b'A', b'D')
            };
            if r == 1 {
                return 1;
            }
        }
        restore(self.st, self.cur, self.sv, r)
    }

    /// `LAB_10037a61`: the unit lookups.
    fn lookups(&mut self, plural: bool) -> i32 {
        if let Some(r) = lookup(COMMON, &self.tok.text, plural, self.mode) {
            self.f.set_unit(&r);
            return self.common();
        }
        self.f.set_unit(b"");
        self.f.set_reading(b"");
        if let Some(r) = lookup(LONG, &self.tok.text, self.plural, self.mode) {
            self.f.set_unit(&r);
            return self.long();
        }
        self.f.set_unit(b"");
        self.short()
    }

    /// `LAB_10037a8e`: a unit of the common tables.
    fn common(&mut self) -> i32 {
        if self.push().is_none() {
            return self.fail();
        }
        let k = self.power_k();
        if k >= 0 {
            self.f.set_reading(&power(k, &self.f.unit()));
            if self.push().is_none() {
                return self.fail();
            }
        } else {
            let u = self.f.unit();
            self.f.set_reading(&u);
        }
        if self.tok.ws > 1 || !is_op(&self.tok) {
            self.pop();
            if self.st.q.n() != 0 {
                self.from_queue = true;
            }
            return self.emit();
        }
        let op = self.tok;
        if self.push().is_none() {
            return self.fail();
        }
        let found = if unitish(&self.tok) {
            let r = self.lookup_tok(ALL, false, self.mode);
            self.f.set_unit(r.as_deref().unwrap_or(b""));
            r.is_some()
        } else {
            false
        };
        if !found {
            if op.text[0] != b'/' || self.tok.ty != 2 || self.tok.ws != 0 {
                self.seek(-2);
                return self.emit();
            }
            if self.per_number(self.plural) {
                self.f.add(b"per");
                self.seek(-2);
                return self.emit();
            }
            self.seek(-3);
            return self.emit();
        }
        if self.push().is_none() {
            return self.fail();
        }
        let k = self.power_k();
        if k < 0 {
            self.pop();
            let w: &[u8] = if is_sym(&op, b'/') { b"per" } else { b"muliplied by" };
            self.f.add(w);
            let u = self.f.unit();
            self.f.add(&u);
            return self.emit();
        }
        let w: &[u8] = if op.text[0] == b'/' {
            b"per"
        } else if op.text[0] == b'*' || (op.len == 2 && strcmp(&self.tok.text, op_by()) == 0) {
            b"multiplied by"
        } else {
            b"by"
        };
        self.f.add(w);
        let u = self.f.unit();
        if k == 0 {
            self.f.add(&u);
        } else {
            let r = power_after(&self.f.reading(), k, &u, b"square");
            self.f.set_reading(&r);
        }
        self.emit()
    }

    /// The `0x10078540` group (`km`, `lb`, `hrs`, `Pa`, words).
    fn long(&mut self) -> i32 {
        if self.push().is_none() {
            return self.fail();
        }
        let k = self.power_k();
        if k >= 0 {
            self.f.set_reading(&power(k, &self.f.unit()));
            self.f.set_flag(F60, true);
            if self.push().is_none() {
                return self.fail();
            }
        } else if self.tok.ws < 2 && is_sym(&self.tok, b'=') {
            let r = [&self.f.unit()[..], b" equal"].concat();
            self.f.set_reading(&r);
            self.f.set_flag(F60, true);
            if self.push().is_none() {
                return self.fail();
            }
        } else {
            let u = self.f.unit();
            self.f.set_reading(&u);
            self.f.set_flag(F64, true);
        }
        if self.tok.ws > 1 || !is_op(&self.tok) {
            self.pop();
            return self.long_tail();
        }
        let op = self.tok;
        if self.push().is_none() {
            return self.fail();
        }
        self.f.set_flag(F64, true);
        if op.text[0] == b'/' {
            self.plural = false;
        }
        let pl = self.plural;
        let found = if unitish(&self.tok) {
            let r = self.lookup_tok(ALL, pl, self.mode);
            self.f.set_unit(r.as_deref().unwrap_or(b""));
            r.is_some()
        } else {
            false
        };
        if !found {
            if op.text[0] == b'/' && self.tok.ty == 2 && self.tok.ws == 0 {
                if self.per_number(pl) {
                    self.f.add(b"per");
                    self.f.set_flag(F60, true);
                    self.seek(-2);
                    return self.long_end();
                }
                self.seek(-3);
            } else {
                self.seek(-2);
            }
            return self.long_tail();
        }
        let w: &[u8] = if op.text[0] == b'/' {
            b"per"
        } else if op.text[0] == b'*' {
            b"multiplied by"
        } else if op.len == 2 && strcmp(&self.tok.text, op_by()) == 0 {
            b"multiplied by"
        } else if op.len == 2 && strcmp(&op.text, op_x()) == 0 {
            b"by"
        } else {
            b"x"
        };
        self.f.add(w);
        if self.push().is_none() {
            return self.fail();
        }
        let k = self.power_k();
        let u = self.f.unit();
        if k == 0 {
            self.f.add(&u);
        } else if k > 0 {
            let r = power_after(&self.f.reading(), k, &u, b"squre");
            self.f.set_reading(&r);
        } else {
            self.pop();
            self.f.add(&u);
        }
        self.f.set_flag(F60, true);
        self.long_end()
    }

    /// `LAB_100385d3`.
    fn long_tail(&mut self) -> i32 {
        if !self.f.flag(F60) && (self.numb || self.dash || self.art) {
            return self.emit();
        }
        self.long_end()
    }

    /// `LAB_10038606`.
    fn long_end(&mut self) -> i32 {
        if self.mode == b'W' || self.f.flag(F60) {
            return self.emit();
        }
        self.short()
    }

    /// `LAB_10038621`: one-letter units (`m`, `g`, `K`), decided with the words around.
    fn short(&mut self) -> i32 {
        self.f.set_unit(b"");
        self.f.set_reading(b"");
        let pl = self.plural;
        let Some(r) = lookup(SHORT, &self.tok.text, pl, self.mode) else {
            self.f.set_unit(b"");
            return self.square_cubic();
        };
        self.f.set_unit(&r);
        self.f.set_flag(F64, false);
        let u = self.f.unit();
        let sp = special(self.cx, self.st, *self.cur, &u, pl, true);
        self.f.set_unit(sp.as_deref().unwrap_or(b""));
        if sp.is_some() {
            let u = self.f.unit();
            self.f.set_reading(&u);
        }
        if self.push().is_none() {
            return self.fail();
        }
        let k = self.power_k();
        if k >= 0 {
            self.f.set_flag(F64, true);
            self.f.set_reading(&power(k, &self.f.unit()));
            self.f.set_flag(F60, true);
            if self.push().is_none() {
                return self.fail();
            }
        } else if self.tok.ws < 2 && is_sym(&self.tok, b'=') {
            let r = [&self.f.unit()[..], b" equal"].concat();
            self.f.set_reading(&r);
            self.f.set_flag(F60, true);
            if self.push().is_none() {
                return self.fail();
            }
        } else {
            let u = self.f.unit();
            self.f.set_reading(&u);
            self.art = true;
        }
        if self.tok.ws > 1 || !is_op(&self.tok) {
            self.pop();
            if self.mode == b'W' {
                return self.emit();
            }
            return self.short_tail();
        }
        let op = self.tok;
        if self.push().is_none() {
            return self.fail();
        }
        let found = if unitish(&self.tok) {
            let r = self.lookup_tok(ALL, false, self.mode);
            self.f.set_unit(r.as_deref().unwrap_or(b""));
            r.is_some()
        } else {
            false
        };
        if !found {
            if op.text[0] != b'/' || self.tok.ty != 2 || self.tok.ws != 0 {
                self.seek(-2);
                return self.short_tail();
            }
            if !self.per_number(self.plural) {
                self.seek(-3);
                return self.short_tail();
            }
            self.f.add(b"per");
            self.seek(-2);
            return self.emit();
        }
        let w: &[u8] = if op.text[0] == b'/' {
            b"per"
        } else if op.text[0] == b'*' || (op.len == 2 && strcmp(&self.tok.text, op_by()) == 0) {
            b"multiplied by"
        } else {
            b"by"
        };
        self.f.add(w);
        if self.push().is_none() {
            return self.fail();
        }
        let k = self.power_k();
        let u = self.f.unit();
        if k >= 0 {
            self.f.set_flag(F64, true);
            if k == 0 {
                self.f.add(&u);
            } else {
                let r = power_after(&self.f.reading(), k, &u, b"square");
                self.f.set_reading(&r);
            }
        } else {
            self.pop();
            self.f.add(&u);
        }
        if op.text[0] == 0 || !b"xX".contains(&op.text[0]) || self.f.flag(F64) {
            return self.emit();
        }
        self.short_tail()
    }

    /// `LAB_10038bd4`: an arithmetic operator after `5 m` keeps the letter a unit.
    fn short_tail(&mut self) -> i32 {
        if self.f.flag(F60) {
            return self.emit();
        }
        if !self.dash && !self.numb {
            return self.by_words();
        }
        if self.push().is_none() {
            return self.fail();
        }
        let t = self.tok;
        let op = t.ws <= 2 && t.len == 1 && t.text[0] != 0 && b"-+*/=xX".contains(&t.text[0]) && !is_sym(&t, b'-');
        if !op {
            self.pop();
            return self.after_number();
        }
        if self.push().is_none() {
            return self.fail();
        }
        let t = self.tok;
        if t.ws > 2 || t.ty != 2 || !num::is_number(&t.text) {
            self.seek(-2);
            return self.after_number();
        }
        self.seek(-2);
        self.emit()
    }

    /// `LAB_10038cfa`.
    fn after_number(&mut self) -> i32 {
        if !self.numb {
            return self.by_words();
        }
        let n = self.st.ahead.n - 1;
        let a = token_at(&self.st.done, &self.st.ahead, n, -1);
        let b2 = token_at(&self.st.done, &self.st.ahead, n, -2);
        let c = token_at(&self.st.done, &self.st.ahead, n, -3);
        self.art = b2.is_some();
        let listed = match (a, b2, c) {
            (Some(a), Some(b2), Some(c)) => c.f1 == 9 && b2.f1 == 0x15 && b2.ws <= 2 && a.ws <= 2,
            _ => false,
        };
        if !listed {
            return self.by_words();
        }
        let u = self.f.unit();
        self.f.set_reading(&u);
        self.emit()
    }

    /// `LAB_10038d7d`.
    fn by_words(&mut self) -> i32 {
        let u = self.f.unit();
        match special(self.cx, self.st, *self.cur, &u, self.plural, false) {
            Some(r) => {
                self.f.set_unit(&r);
                self.f.set_reading(&r);
                self.emit()
            }
            None => {
                self.f.set_unit(b"");
                self.square_cubic()
            }
        }
    }

    /// `LAB_10038dcf`: `5 sq ft`, `3 cu. in`, `2 square meters`.
    fn square_cubic(&mut self) -> i32 {
        self.f.set_unit(b"");
        self.f.set_reading(b"");
        self.art = false;
        if self.prev.is_none_or(|p| p.ty != 2) || self.tok.ty != 1 {
            return self.not_mine();
        }
        let t = self.tok;
        if stricmp(&t.text, b"cu") == 0 || stricmp(&t.text, b"sq") == 0 {
            let mut w = if fold_s(t.text[0]) == b'c' as i32 { b"cubic".to_vec() } else { b"square".to_vec() };
            if cls(t.text[0]) & 0x80 != 0 {
                w[0] = up_s(w[0]) as u8;
            }
            self.f.set_reading(&w);
            if self.push().is_none() {
                return self.fail();
            }
            if self.tok.ws == 0 && self.tok.len < 3 && self.tok.text[0] == b'.' {
                self.art = true;
                if self.push().is_none() {
                    return self.fail();
                }
            }
        } else if stricmp(&t.text, b"cubic") == 0 || stricmp(&t.text, b"square") == 0 {
            self.f.set_reading(t.text());
            if self.push().is_none() {
                return self.fail();
            }
        } else {
            return self.not_mine();
        }
        if self.tok.ty == 1 {
            let r = self.lookup_tok(ALL, self.plural, self.mode);
            self.f.set_unit(r.as_deref().unwrap_or(b""));
            if let Some(r) = r {
                let x = [&self.f.reading()[..], b" ", &r[..]].concat();
                self.f.set_reading(&x);
                return self.emit();
            }
        }
        let t = self.tok;
        let last = if t.len >= 1 { t.text[t.len as usize - 1] } else { 0 };
        if !self.plural || t.ty != 1 || (last != b's' && last != b'S') {
            self.seek(if self.art { -2 } else { -1 });
            return self.not_mine();
        }
        // The plural `s` comes off the token in the buffer for a second try.
        let i = (self.st.ahead.n - 1) as usize;
        let mut stem = t.text().to_vec();
        stem.pop();
        self.st.ahead.toks[i].set_text(&stem);
        self.tok.set_text(&stem);
        let r = self.lookup_tok(ALL, self.plural, self.mode);
        self.f.set_unit(r.as_deref().unwrap_or(b""));
        if let Some(r) = r {
            let x = [&self.f.reading()[..], b" ", &r[..]].concat();
            self.f.set_reading(&x);
            return self.emit();
        }
        let i = (self.st.ahead.n - 1) as usize;
        self.st.ahead.toks[i].set_text(b"");
        self.not_mine()
    }

    /// `fl oz` / `t oz` (with an optional `.`), queued as one word.
    fn ounces(&mut self) -> i32 {
        let t0 = self.tok;
        self.prev = Some(t0);
        self.f.set_unit(t0.text());
        let Some(nx) = self.push() else { return self.retry() };
        let found;
        if stricmp(&nx.text, b"oz") != 0 {
            if !(nx.ws == 0 && nx.len < 3 && nx.text[0] == b'.') {
                return self.retry();
            }
            let n2 = self.push().unwrap_or_default();
            if stricmp(&n2.text, b"oz") != 0 {
                return self.retry();
            }
            found = self.fl_oz();
        } else {
            found = self.fl_oz();
        }
        if !found {
            return self.retry();
        }
        let n3 = self.push().unwrap_or_default();
        if !(n3.ws == 0 && n3.len < 3 && n3.text[0] == b'.') {
            self.pop();
        }
        let u = self.f.unit();
        self.st.q.add(&u, t0.start, self.tok.end, b'A', b'D', false);
        self.common()
    }

    fn fl_oz(&mut self) -> bool {
        let w = [&self.f.unit()[..], b" oz"].concat();
        let r = lookup(COMMON, &w, false, 0);
        self.f.set_unit(r.as_deref().unwrap_or(b""));
        r.is_some()
    }

    /// `LAB_10037a4f`.
    fn retry(&mut self) -> i32 {
        self.pop();
        let pl = self.plural;
        self.lookups(pl)
    }
}

pub fn units(cx: &Cx, st: &mut State, cur: &mut usize, _t: &Tok) -> i32 {
    units_mode(cx, st, cur, 0)
}

/// `FUN_100375b0` with its mode (`W` from the number readers).
pub fn units_mode(cx: &Cx, st: &mut State, cur: &mut usize, mode: u8) -> i32 {
    let tok0 = cur_tok(st);
    if !unitish(&tok0) {
        return 5;
    }
    let sv = save(st);
    let mut u = U {
        cx,
        st,
        cur,
        sv,
        mode,
        start: tok0.start,
        tok: tok0,
        prev: None,
        plural: false,
        numb: false,
        dash: false,
        art: false,
        from_queue: false,
        f: Frame { b: [0; 72] },
    };
    if mode == 0 && stricmp(&tok0.text, b"in") == 0 {
        let Some(n) = u.push() else { return u.fail() };
        if n.ws < 3 && n.ty == 1 && stricmp(&n.text, b"the") == 0 {
            u.pop();
            return 5;
        }
        u.pop();
    }
    let n = u.st.ahead.n - 1;
    let mut prev = token_at(&u.st.done, &u.st.ahead, n, -1);
    if let Some(p) = prev {
        if p.ty == 2 && num::is_number(&p.text) {
            u.plural = num::plural(&p.text);
            u.numb = true;
        } else if p.ws < 3 && is_sym(&p, b'-') {
            prev = token_at(&u.st.done, &u.st.ahead, n, -2);
            if prev.is_some_and(|q| q.ty == 2 && num::is_number(&q.text)) {
                u.dash = true;
            }
        }
    }
    u.prev = prev;
    u.art = prev.is_some_and(|p| p.ty == 1 && (stricmp(&p.text, b"A") == 0 || stricmp(&p.text, b"An") == 0));
    if stricmp(&u.tok.text, b"fl") == 0 || stricmp(&u.tok.text, b"t") == 0 {
        return u.ounces();
    }
    let pl = u.plural;
    u.lookups(pl)
}
