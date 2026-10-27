//! The abbreviation recognizer `FUN_10027500`: titles (`FUN_10027d20`), name suffixes, roman numerals
//! and given-name abbreviations (`FUN_10028200`), letters glued to a number (`FUN_10028e30`) and the
//! general list (`FUN_10029140`), each deciding from the words around it (`FUN_1002a2e0`). Words go to
//! the pending queue, which is flushed into the WA on success.

use super::mem::*;
use super::normalize::{Cx, State};
use super::norm_units::{self as units, lookup_tok};
use super::num;
use super::rx::*;
use super::stream::token_at;
use super::tokenize::Tok;

const TITLES: Rec30 = rec30(0x10083c48, 0x100848ec);
const TITLES2: Ptab = ptab(0x100843c8, 0x100848f0);
const SUFFIX: Rec30 = rec30(0x10084448, 0x100848f4);
const SUFFIX2: Ptab = ptab(0x10084568, 0x100848f8);
const GIVEN: Rec30 = rec30(0x10084578, 0x100848fc);
const ABBR: Rec30 = rec30(0x10083618, 0x100848e8);

fn upper(c: u8) -> bool {
    cls(c) & 0x80 != 0
}

fn up(c: u8) -> u8 {
    b(0x1006ac60, 768)[(256 + 2 * c as i8 as i32) as usize]
}

fn ptab_find(t: Ptab, w: &[u8]) -> i32 {
    let w = cs(w);
    if w.is_empty() {
        return -1;
    }
    let (mut lo, mut hi) = (0, t.count() - 1);
    let mut mid = hi;
    while lo <= hi {
        mid /= 2;
        let c = stricmp(t.get(mid), w);
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

/// `FUN_1004fe90`: `I`..`X` (1..10), or a fullwidth roman numeral pair.
pub fn roman(t: &[u8]) -> i32 {
    let t = cs(t);
    let names: [&[u8]; 10] = [b"I", s(0x10085860), s(0x1008585c), s(0x10085858), b"V", s(0x10085854), s(0x10085850), s(0x10085848), s(0x10085844), b"X"];
    for (i, n) in names.iter().enumerate() {
        if strcmp(n, t) == 0 {
            return i as i32 + 1;
        }
    }
    if t.len() == 2 && super::tokenize::dbcs(t, 0) && t[0] == 0xa5 && 0xaf < t[1] && t[1] < 0xba {
        return t[1] as i32 - 0xaf;
    }
    -1
}

/// `FUN_1000cc90`: a capitalized name (`Mc`/`De` prefixes, `.-_&'` inside), all one case after the first
/// letter.
pub fn name_like(t: &[u8]) -> bool {
    let t = cs(t);
    let c0 = at(t, 0);
    let c1 = at(t, 1);
    if (c0 == b'M' && up(c1) == b'C') || (c0 == b'D' && up(c1) == b'E') {
        return name_like(&t[2.min(t.len())..]);
    }
    if !upper(c0) {
        return false;
    }
    let set = b".-_&'";
    let mut p = 1usize;
    if c1 != 0 && set.contains(&c1) {
        loop {
            let c = at(t, p as isize + 1);
            p += 1;
            if c == 0 || !set.contains(&c) {
                break;
            }
        }
        if at(t, p as isize) == 0 {
            return false;
        }
    }
    if at(t, p as isize) == 0 {
        return true;
    }
    let k = cls(t[p]);
    let upper_mode = if k & 0x80 != 0 {
        true
    } else if k & 0x40 != 0 {
        false
    } else {
        return false;
    };
    loop {
        let c = at(t, p as isize + 1);
        if c == 0 {
            return true;
        }
        if !set.contains(&c) {
            let m = if upper_mode { 0x80 } else { 0x40 };
            if cls(c) & m == 0 {
                return false;
            }
        }
        p += 1;
    }
}

/// `FUN_10003d90` + `FUN_10003f80`: the entry flags of a word in the main dictionary (0 if absent).
fn dict_flags(cx: &Cx, w: &[u8]) -> u8 {
    cx.fe.lex.lookup_payload(cs(w)).and_then(|p| p.first().copied()).unwrap_or(0)
}

/// `FUN_100041b0(w, 0)`: absent, or flagged 0x40.
pub fn dict_b40(cx: &Cx, w: &[u8]) -> bool {
    let f = dict_flags(cx, w);
    f == 0 || f & 0x40 == 0x40
}

/// What `FUN_1002a2e0` saw around the word: up to 10 codes on each side. `I` initial, `R` roman
/// numeral, `T` title, `A` given-name abbreviation, `Q` name suffix, `C` common word, `D` other word
/// (a name or unknown).
#[derive(Default)]
pub struct Ctx {
    pub left: Vec<u8>,
    pub right: Vec<u8>,
}

impl Ctx {
    fn l(&self, i: usize) -> u8 {
        self.left.get(i).copied().unwrap_or(0)
    }
    fn r(&self, i: usize) -> u8 {
        self.right.get(i).copied().unwrap_or(0)
    }
}

fn word_code(cx: &Cx, t: &Tok) -> Option<u8> {
    if roman(&t.text) >= 0 {
        return Some(b'R');
    }
    if TITLES.find(&t.text, true) >= 0 || ptab_find(TITLES2, &t.text) >= 0 {
        return Some(b'T');
    }
    if !name_like(&t.text) {
        return None;
    }
    Some(if dict_b40(cx, &t.text) { b'D' } else { b'C' })
}

/// `FUN_1002a2e0`: `left` scans the consumed list backwards, `right` reads ahead (and gives the tokens
/// back).
pub fn context(cx: &Cx, st: &mut State, cur: usize, left: bool, right: bool) -> Ctx {
    let mut c = Ctx::default();
    let max = 10;
    if left && !st.done.toks.is_empty() {
        let toks = st.done.toks.clone();
        let mut k = toks.len() as isize - 1;
        let mut prev_ws = 0;
        while k >= 0 {
            if c.left.len() >= max {
                break;
            }
            let t = toks[k as usize];
            let mut skip = false;
            if prev_ws == 0 {
                if (t.ty == 3 || t.ty == 7) && t.len == 1 && t.text[0] == b',' {
                    skip = true;
                }
            } else if prev_ws > 2 {
                break;
            }
            if !skip {
                if t.ty != 1 && t.ty != 7 {
                    break;
                }
                let mut tt = t;
                if k < 1 || t.ws != 0 || t.len > 2 || t.text[0] != b'.' {
                    if t.len == 1 && upper(t.text[0]) {
                        c.left.push(b'I');
                    } else {
                        match word_code(cx, &t) {
                            Some(x) => c.left.push(x),
                            None => break,
                        }
                    }
                } else {
                    k -= 1;
                    tt = toks[k as usize];
                    if tt.ty != 1 || prev_ws > 2 {
                        break;
                    }
                    if tt.len == 1 && upper(tt.text[0]) {
                        c.left.push(b'I');
                    } else if !upper(tt.text[0]) || GIVEN.find(&tt.text, true) < 0 {
                        if TITLES.find(&tt.text, true) < 0 {
                            break;
                        }
                        c.left.push(b'T');
                    } else {
                        c.left.push(b'A');
                    }
                }
                prev_ws = tt.ws;
            }
            k -= 1;
        }
    }
    if right {
        let n0 = st.ahead.n;
        let mut q = cur;
        while c.right.len() < max {
            let Some(t) = push(cx, st, &mut q) else { break };
            if t.ws == 0 && is_p(&t, b',') {
                continue;
            }
            if t.ws > 2 || t.ty != 1 {
                break;
            }
            if t.len == 1 && upper(t.text[0]) {
                c.right.push(b'I');
                let Some(t2) = push(cx, st, &mut q) else { break };
                if !(t2.ws == 0 && t2.len < 3 && t2.text[0] == b'.') {
                    pop(st, &mut q);
                }
                continue;
            }
            let s1 = SUFFIX.find(&t.text, true);
            if s1 >= 0 || ptab_find(SUFFIX2, &t.text) >= 0 {
                c.right.push(b'Q');
                if s1 >= 0 {
                    let Some(t2) = push(cx, st, &mut q) else { break };
                    if !(t2.ws == 0 && t2.len < 3 && t2.text[0] == b'.') {
                        pop(st, &mut q);
                    }
                }
                continue;
            }
            if roman(&t.text) >= 0 {
                c.right.push(b'R');
                continue;
            }
            if upper(t.text[0]) && GIVEN.find(&t.text, true) >= 0 {
                let Some(t2) = push(cx, st, &mut q) else { break };
                if t2.ws == 0 && t2.len < 3 && t2.text[0] == b'.' {
                    c.right.push(b'A');
                    continue;
                }
                let tb = pop(st, &mut q);
                if !name_like(&tb.text) {
                    break;
                }
                c.right.push(if dict_b40(cx, &tb.text) { b'D' } else { b'C' });
                continue;
            }
            if TITLES.find(&t.text, true) >= 0 || ptab_find(TITLES2, &t.text) >= 0 {
                c.right.push(b'T');
                continue;
            }
            if !name_like(&t.text) {
                break;
            }
            c.right.push(if dict_b40(cx, &t.text) { b'D' } else { b'C' });
        }
        st.ahead.n = n0;
    }
    c
}

fn tok_at(st: &State, i: isize) -> Tok {
    st.ahead.tok(i)
}

fn last(st: &State) -> isize {
    st.ahead.n as isize - 1
}

fn dot(t: &Tok) -> bool {
    t.ws == 0 && t.len < 3 && t.text[0] == b'.'
}

/// Give the buffer back to `n0` tokens (the cursor follows).
fn back_to(st: &mut State, cur: &mut usize, n0: i32, ti: &mut isize) {
    st.ahead.n = n0;
    if let Some(p) = st.ahead.pos[n0 as usize] {
        *cur = p;
    }
    *ti = last(st);
}

fn push_i(cx: &Cx, st: &mut State, cur: &mut usize, ti: &mut isize) -> Option<Tok> {
    let t = push(cx, st, cur)?;
    *ti = last(st);
    Some(t)
}

fn pop_i(st: &mut State, cur: &mut usize, ti: &mut isize) -> Tok {
    let t = pop(st, cur);
    *ti = last(st);
    t
}

fn seek_i(st: &mut State, cur: &mut usize, d: i32, ti: &mut isize) -> Tok {
    let t = seek(st, cur, d);
    *ti = last(st);
    t
}

/// `FUN_10027d20`: a title (`Dr`, `Capt`) before a name.
fn titles(cx: &Cx, st: &mut State, cur: &mut usize, ti: &mut isize) -> bool {
    let t0 = tok_at(st, *ti);
    if t0.ty != 1 {
        return false;
    }
    st.q.reset();
    let n0 = st.ahead.n;
    let fail = |st: &mut State, cur: &mut usize, ti: &mut isize| {
        if st.ahead.n != n0 {
            back_to(st, cur, n0, ti);
        }
        false
    };
    let k = TITLES.find(&t0.text, true);
    if k < 0 {
        let txt = t0.text();
        let ap = txt.windows(2).position(|w| w == b"'s").or_else(|| txt.windows(2).position(|w| w == b"'S"));
        if let Some(p) = ap {
            if at(txt, p as isize + 2) == 0 && TITLES.find(&txt[..p], true) >= 0 {
                let kk = TITLES.find(&txt[..p], true);
                let mut w = TITLES.field(kk, 0x10).to_vec();
                w.extend_from_slice(&txt[p..]);
                st.q.add(&w, t0.start, t0.end, b'A', b'D', upper(t0.text[0]));
                return true;
            }
        }
        return fail(st, cur, ti);
    }
    let val = TITLES.field(k, 0x10);
    let accept = |st: &mut State| {
        st.q.add(val, t0.start, t0.end, b'A', b'D', upper(t0.text[0]));
        true
    };
    let Some(nx) = push_i(cx, st, cur, ti) else { return fail(st, cur, ti) };
    if dot(&nx) {
        if push_i(cx, st, cur, ti).is_none() {
            return fail(st, cur, ti);
        }
    } else if stricmp(&t0.text, b"MS") == 0 {
        back_to(st, cur, n0, ti);
        return false;
    }
    let ct = tok_at(st, *ti);
    if is_p(&ct, b'\'') {
        let Some(s) = push_i(cx, st, cur, ti) else { return fail(st, cur, ti) };
        if s.len == 1 && up(s.text[0]) == b'S' {
            let mut w = val.to_vec();
            w.push(b'\'');
            w.extend_from_slice(s.text());
            st.q.add(&w, t0.start, s.end, b'A', b'D', upper(t0.text[0]));
            return true;
        }
    }
    pop_i(st, cur, ti);
    let c = context(cx, st, *cur, true, true);
    if !c.left.is_empty() && c.l(0) == b'T' {
        return accept(st);
    }
    if c.right.is_empty() {
        return fail(st, cur, ti);
    }
    let (r0, r1) = (c.r(0), c.r(1));
    if r0 != 0 && b"TADR".contains(&r0) {
        return accept(st);
    }
    if c.right.len() < 2 {
        if r0 != b'C' {
            return fail(st, cur, ti);
        }
    } else {
        if r0 != 0 && b"IC".contains(&r0) && r1 != 0 && b"ADQR".contains(&r1) {
            return accept(st);
        }
        if r0 != b'C' {
            if r0 == b'I' {
                if r1 == b'C' {
                    return accept(st);
                }
                return fail(st, cur, ti);
            }
            return fail(st, cur, ti);
        }
        if r1 != 0 {
            if b"IC".contains(&r1) {
                return accept(st);
            }
            // LAB_10027ff5 with r0 == 'C' falls to LAB_1002800d, which continues to the lexicon test.
        }
    }
    // The next word's dictionary entry decides: a name-ish flag (0x10/0x20/0x40) keeps the title.
    let Some(nx) = push_i(cx, st, cur, ti) else { return fail(st, cur, ti) };
    let f = dict_flags(cx, &nx.text);
    let named = f & 0x10 != 0 || f & 0x20 != 0 || f & 0x40 != 0;
    pop_i(st, cur, ti);
    if named {
        return accept(st);
    }
    fail(st, cur, ti)
}

/// `FUN_10028200`: name suffixes (`Jr`, `Esq`), roman numerals after a name, given-name abbreviations
/// (`Chas.`, `Wm.`) and initials.
fn suffixes(cx: &Cx, st: &mut State, cur: &mut usize, ti: &mut isize) -> bool {
    let t0i = *ti;
    let t0 = tok_at(st, t0i);
    if t0.ty != 1 {
        return false;
    }
    st.q.reset();
    let n0 = st.ahead.n;
    let restore_all = |st: &mut State, cur: &mut usize, ti: &mut isize| {
        back_to(st, cur, n0, ti);
        false
    };
    let sx = SUFFIX.find(&t0.text, true);
    let mut rn = -1;
    'a8708: {
        if sx < 0 {
            rn = roman(&t0.text);
            if rn < 0 {
                break 'a8708;
            }
        }
        let Some(nx) = push_i(cx, st, cur, ti) else { return restore_all(st, cur, ti) };
        if !dot(&nx) {
            pop_i(st, cur, ti);
        }
        let c = context(cx, st, *cur, true, true);
        if c.left.is_empty() {
            return false;
        }
        let (l0, l1) = (c.l(0), c.l(1));
        let ok = (l0 != 0 && b"AD".contains(&l0))
            || (c.left.len() >= 2 && ((l0 == b'I' && l1 != 0 && b"TADC".contains(&l1)) || (l0 == b'C' && l1 != 0 && b"TADIC".contains(&l1))));
        if ok {
            if sx >= 0 {
                st.q.add(SUFFIX.field(sx, 0x10), t0.start, t0.end, b'A', b'D', upper(t0.text[0]));
                return true;
            }
            let prev = token_at(&st.done, &st.ahead, st.ahead.n - 1, -1);
            let prev2 = token_at(&st.done, &st.ahead, st.ahead.n - 1, -2);
            let war = match (prev, prev2) {
                (Some(p), Some(q)) => stricmp(&q.text, b"World") == 0 && stricmp(&p.text, b"War") == 0 && p.ws <= 2,
                _ => false,
            };
            if war {
                return roman_reading(st, &t0, rn, false);
            }
            if t0.len == 1 && t0.text[0] != 0 && b"IVX".contains(&t0.text[0]) {
                let r0 = c.r(0);
                if c.right.is_empty() || (r0 != 0 && !b"ADICR".contains(&r0) && r0 != b'Q') || r0 == 0 {
                    st.q.add(&t0.text, t0.start, t0.end, b'A', b'D', false);
                    return true;
                }
                if b"ADICR".contains(&r0) {
                    st.q.add(&t0.text, t0.start, t0.end, b'A', b'S', false);
                    return true;
                }
            }
            return roman_reading(st, &t0, rn, true);
        }
        if rn >= 0 {
            let prev = token_at(&st.done, &st.ahead, st.ahead.n - 1, -1);
            let prev2 = token_at(&st.done, &st.ahead, st.ahead.n - 1, -2);
            if let Some(p) = prev {
                let war = prev2.is_some_and(|q| q.text().windows(5).any(|w| w == b"World")) && stricmp(&p.text, b"War") == 0 && p.ws < 3;
                let article = p.ty == 1 && stricmp(&p.text, b"A") != 0 && stricmp(&p.text, b"The") != 0 && c.l(0) == b'C' && rn != 1;
                if war || article {
                    return roman_reading(st, &t0, rn, false);
                }
            }
        }
    }
    // LAB_10028708
    let t0 = tok_at(st, *ti);
    if upper(t0.text[0]) {
        if t0.len == 1 {
            let Some(nx) = push_i(cx, st, cur, ti) else { return restore_all(st, cur, ti) };
            let with_dot = dot(&nx);
            if !with_dot {
                pop_i(st, cur, ti);
            }
            let c = context(cx, st, *cur, true, true);
            let (l0, l1) = (c.l(0), c.l(1));
            let (r0, r1, r2) = (c.r(0), c.r(1), c.r(2));
            let add_s = |st: &mut State| {
                st.q.add(&t0.text, t0.start, t0.end, b'A', b'S', false);
                true
            };
            if !with_dot && t0.text[0] == b'A' {
                if c.left.is_empty() {
                    if !c.right.is_empty() {
                        if r0 == b'R' {
                            return add_s(st);
                        }
                        if c.right.len() > 1 {
                            if r0 == b'D' && r1 != 0 && b"AQ".contains(&r1) {
                                return add_s(st);
                            }
                            if r0 == b'C' && r1 != 0 && b"AQR".contains(&r1) {
                                return add_s(st);
                            }
                        }
                    }
                } else {
                    if l0 != 0 && b"TA".contains(&l0) {
                        return add_s(st);
                    }
                    if c.left.len() > 1 && l0 == b'C' && l1 != 0 {
                        if b"TA".contains(&l1) {
                            return add_s(st);
                        }
                        return false;
                    }
                }
                return false;
            }
            if c.right.is_empty() {
                if with_dot {
                    pop_i(st, cur, ti);
                }
                return false;
            }
            if r0 != 0 && b"AQ".contains(&r0) {
                return add_s(st);
            }
            if r0 == b'D' {
                if !c.left.is_empty() && l0 != 0 && b"TADC".contains(&l0) {
                    return add_s(st);
                }
                if c.right.len() > 1 && r1 != 0 && b"AQDC".contains(&r1) {
                    return add_s(st);
                }
            } else {
                if r0 == b'R' {
                    if c.left.is_empty() || l0 == 0 {
                        return a89aa(st, cur, ti, n0);
                    }
                    if b"TADC".contains(&l0) {
                        return add_s(st);
                    }
                }
                if r0 == b'C' {
                    if with_dot {
                        let left_ok = (!c.left.is_empty() && l0 != 0 && b"TAD".contains(&l0)) || (c.left.len() > 1 && l0 == b'C' && l1 != 0 && b"TAD".contains(&l1));
                        if left_ok || (c.right.len() > 1 && r1 != 0 && b"AQDR".contains(&r1)) {
                            return add_s(st);
                        }
                        if c.right.len() > 2 && r1 == b'C' && r2 != 0 && b"AQDR".contains(&r2) {
                            return add_s(st);
                        }
                    } else {
                        if !c.left.is_empty() && l0 != 0 && b"TAD".contains(&l0) {
                            return add_s(st);
                        }
                        if c.right.len() > 1 && r1 != 0 && b"AQDR".contains(&r1) {
                            return add_s(st);
                        }
                    }
                }
            }
        } else {
            let g = GIVEN.find(&t0.text, true);
            if g >= 0 {
                let Some(nx) = push_i(cx, st, cur, ti) else { return restore_all(st, cur, ti) };
                if !dot(&nx) {
                    pop_i(st, cur, ti);
                    return false;
                }
                let c = context(cx, st, *cur, true, true);
                if c.right.is_empty() {
                    pop_i(st, cur, ti);
                    return false;
                }
                let (l0, l1) = (c.l(0), c.l(1));
                let (r0, r1, r2) = (c.r(0), c.r(1), c.r(2));
                let accept = (r0 != 0 && b"QDR".contains(&r0))
                    || (c.right.len() > 1
                        && r0 != 0
                        && b"IC".contains(&r0)
                        && ((r1 != 0 && b"QDR".contains(&r1)) || (c.right.len() > 2 && r0 != r1 && r1 != 0 && b"IC".contains(&r1) && r2 != 0 && b"QDR".contains(&r2))));
                let left_ok = !c.left.is_empty()
                    && ((r0 == b'I' && ((l0 != 0 && b"TD".contains(&l0)) || (c.left.len() > 1 && l0 == b'C' && l1 != 0 && b"TD".contains(&l1))))
                        || (r0 == b'C' && ((l0 != 0 && b"TD".contains(&l0)) || (c.left.len() > 1 && l0 != l1 && l0 != 0 && b"IC".contains(&l0) && l1 != 0 && b"TDIC".contains(&l1)))));
                if accept || left_ok {
                    st.q.add(GIVEN.field(g, 0x10), t0.start, t0.end, b'A', b'D', false);
                    return true;
                }
            }
        }
    }
    a89aa(st, cur, ti, n0)
}

/// A roman numeral after a name as an ordinal ("Henry VIII": "Eighth"), or after "World War" / an
/// article-less common word as a cardinal; the first letter capitalized.
fn roman_reading(st: &mut State, t0: &Tok, rn: i32, ordinal: bool) -> bool {
    if st.q.n() >= 20 {
        return ordinal;
    }
    let d = format!("{}", rn);
    let mut w = if ordinal { num::ordinal(d.as_bytes()) } else { num::cardinal(d.as_bytes(), 1) };
    if let Some(c0) = w.first_mut() {
        *c0 = up(*c0);
    }
    st.q.add(&w, t0.start, t0.end, b'A', b'D', false);
    true
}

fn a89aa(st: &mut State, cur: &mut usize, ti: &mut isize, n0: i32) -> bool {
    if st.ahead.n != n0 {
        back_to(st, cur, n0, ti);
    }
    false
}

/// `FUN_10028e30`: letters glued to a number (`A320`, `I95`, `B52s`).
fn alnum(cx: &Cx, st: &mut State, cur: &mut usize, ti: &mut isize) -> bool {
    let t0 = tok_at(st, *ti);
    if t0.ty != 1 {
        return false;
    }
    st.q.reset();
    if !super::sbd::all_alpha(&t0.text) {
        return false;
    }
    let Some(nx) = push_i(cx, st, cur, ti) else { return false };
    if !(nx.ws == 0 && nx.ty == 2) {
        pop_i(st, cur, ti);
        return false;
    }
    let caps = !t0.text().iter().any(|&c| cls(c) & 0x40 != 0);
    st.q.add(&t0.text, t0.start, t0.end, b'A', if caps { b'S' } else { b'D' }, false);
    if !num::is_ordinal(&nx.text) {
        let w = num::house(&nx.text);
        st.q.add(&w, nx.start, nx.end, b'D', b'D', false);
        return true;
    }
    let l = nx.len as usize;
    let suf = &nx.text[l - 2..l];
    if [&b"ST"[..], b"ND", b"RD", b"TH"].iter().all(|x| strcmp(suf, x) != 0) {
        let w = num::ordinal(&t0.text);
        st.q.add(&w, nx.start, nx.end, b'D', b'D', false);
        return true;
    }
    let w = num::house(&nx.text[..l - 2]);
    st.q.add(&w, t0.start, nx.end, b'D', b'D', false);
    st.q.add(suf, nx.end - 2, nx.end, b'D', b'S', false);
    true
}

/// `FUN_10029140`: `Vol./No./P./PP.` + number, `Mr`/`Mrs`/`Ms`, `Co`/`Pty`, `Dr` (doctor or drive),
/// `St` (saint or street), `Ste`, `Ctr`/`Pkwy`, `w/`, and the general list (`0x10083618`).
fn general(cx: &Cx, st: &mut State, cur: &mut usize, ti: &mut isize) -> bool {
    let t0 = tok_at(st, *ti);
    if t0.ty != 1 {
        return false;
    }
    let n0 = st.ahead.n;
    st.q.reset();
    let restore = |st: &mut State, cur: &mut usize, ti: &mut isize| {
        back_to(st, cur, n0, ti);
        false
    };
    let a29d = |st: &mut State, cur: &mut usize, ti: &mut isize| {
        if st.ahead.n == n0 {
            return false;
        }
        back_to(st, cur, n0, ti);
        false
    };
    let txt = t0.text().to_vec();
    if (t0.ws > 0 || st.done.toks.is_empty()) && [&b"Vol"[..], b"No", b"P", b"PP"].iter().any(|x| stricmp(&txt, x) == 0) {
        let Some(nx) = push_i(cx, st, cur, ti) else { return restore(st, cur, ti) };
        let num_t = if dot(&nx) {
            let Some(n2) = push_i(cx, st, cur, ti) else { return restore(st, cur, ti) };
            n2
        } else {
            nx
        };
        if num_t.ws > 1 || num_t.ty != 2 || !digits_commas(&num_t.text) {
            return a29d(st, cur, ti);
        }
        let w: &[u8] = match up(txt[0]) {
            b'V' => b"volume",
            b'N' => b"number",
            _ => b"pages",
        };
        st.q.add(w, t0.start, t0.end, b'A', b'D', false);
        st.q.add(&num::cardinal(&num_t.text, 1), num_t.start, num_t.end, b'D', b'D', false);
        let Some(dash) = push_i(cx, st, cur, ti) else { return restore(st, cur, ti) };
        if !((dash.ws <= 2 && is_p(&dash, b'-')) || is_p(&dash, b'~')) {
            seek_i(st, cur, -1, ti);
            return true;
        }
        if let Some(n3) = push_i(cx, st, cur, ti) {
            if n3.ws < 3 && n3.ty == 2 && digits_commas(&n3.text) {
                st.q.add(b"to", dash.start, dash.end, b'S', b'D', false);
                st.q.add(&num::cardinal(&n3.text, 1), n3.start, n3.end, b'D', b'D', false);
                return true;
            }
            seek_i(st, cur, -2, ti);
            return true;
        }
        return restore(st, cur, ti);
    }
    if [&b"Mr"[..], b"Mrs", b"Ms"].iter().any(|x| stricmp(&txt, x) == 0) {
        let mut had_dot = false;
        let Some(mut nx) = push_i(cx, st, cur, ti) else { return restore(st, cur, ti) };
        if dot(&nx) {
            had_dot = true;
            let Some(n2) = push_i(cx, st, cur, ti) else { return restore(st, cur, ti) };
            nx = n2;
        }
        let title = |t: &Tok| -> &'static [u8] {
            if t.len == 3 {
                b"Misses"
            } else if fold_s(t.text[1]) == 0x72 {
                b"Mister"
            } else {
                b"Miz"
            }
        };
        if nx.ws < 3 && (is_p(&nx, b'&') || stricmp(&nx.text, b"and") == 0) {
            let Some(n3) = push_i(cx, st, cur, ti) else { return restore(st, cur, ti) };
            if [&b"Mr"[..], b"Mrs", b"Ms"].iter().any(|x| stricmp(&n3.text, x) == 0) {
                let mut w = title(&t0).to_vec();
                w.extend_from_slice(b" and ");
                w.extend_from_slice(title(&n3));
                st.q.add(&w, t0.start, n3.end, b'A', b'D', false);
                return true;
            }
        } else if had_dot && nx.ws < 2 && nx.ty == 1 && upper(nx.text[0]) {
            let w: &[u8] = if t0.len == 2 { if fold_s(t0.text[1]) == 0x72 { b"Mister" } else { b"Miz" } } else { b"Misses" };
            st.q.add(w, t0.start, t0.end, b'A', b'D', false);
            pop_i(st, cur, ti);
            return true;
        }
        return a29d(st, cur, ti);
    }
    let co = upper(txt[0]) && (stricmp(&txt, b"Co") == 0 || stricmp(&txt, b"Pty") == 0);
    if co {
        let p1 = token_at(&st.done, &st.ahead, st.ahead.n - 1, -1);
        let p2 = token_at(&st.done, &st.ahead, st.ahead.n - 1, -2);
        let p3 = token_at(&st.done, &st.ahead, st.ahead.n - 1, -3);
        let cap = |t: Option<Tok>| t.is_some_and(|t| t.ty == 1 && upper(t.text[0]));
        let ok = cap(p1) || cap(p2) || (cap(p3) && p2.is_some_and(|q| dot(&q)));
        if !ok {
            return a29d(st, cur, ti);
        }
        let w: &[u8] = if up(txt[0]) == b'C' { b"Company" } else { b"Proprietary" };
        st.q.add(w, t0.start, t0.end, b'A', b'D', false);
        let Some(nx) = push_i(cx, st, cur, ti) else { return restore(st, cur, ti) };
        if !dot(&nx) {
            seek_i(st, cur, -1, ti);
            return true;
        }
        let Some(n2) = push_i(cx, st, cur, ti) else { return restore(st, cur, ti) };
        if n2.ws != 0 || (n2.ty != 7 && n2.ty != 3) {
            seek_i(st, cur, -2, ti);
            return true;
        }
        seek_i(st, cur, -1, ti);
        return true;
    }
    if stricmp(&txt, b"dr") == 0 {
        let cap = upper(txt[0]);
        let prev = token_at(&st.done, &st.ahead, st.ahead.n - 1, -1);
        let Some(nx) = push_i(cx, st, cur, ti) else { return restore(st, cur, ti) };
        let had_dot;
        let after;
        if dot(&nx) {
            had_dot = true;
            let Some(n2) = push_i(cx, st, cur, ti) else { return restore(st, cur, ti) };
            after = n2;
            pop_i(st, cur, ti);
        } else {
            had_dot = false;
            after = nx;
            pop_i(st, cur, ti);
        }
        let doctor = prev.is_none_or(|p| t0.ws > 1 || (p.ty != 1 && p.ty != 2));
        if doctor || (after.ws < 2 && after.ty == 1 && cls(after.text[0]) & 0x40 == 0) {
            st.q.add(b"doctor", t0.start, t0.end, b'A', b'D', cap);
            if after.ws < 2 && after.ty == 1 && upper(after.text[0]) {
                return true;
            }
        } else {
            st.q.add(b"drive", t0.start, t0.end, b'A', b'D', cap);
        }
        if had_dot {
            pop_i(st, cur, ti);
        }
        return true;
    }
    if stricmp(&txt, b"st") != 0 {
        if upper(txt[0]) && stricmp(&txt, b"Ste") == 0 {
            let Some(nx) = push_i(cx, st, cur, ti) else { return restore(st, cur, ti) };
            let mut had_dot = false;
            let mut w = nx;
            if dot(&nx) {
                had_dot = true;
                let Some(n2) = push_i(cx, st, cur, ti) else { return restore(st, cur, ti) };
                w = n2;
            }
            if w.ty == 1 && name_like(&w.text) {
                st.q.add(b"Sainte", t0.start, t0.end, b'A', b'D', false);
                st.q.add(&w.text, w.start, w.end, b'A', b'D', false);
                return true;
            }
            seek_i(st, cur, if had_dot { -2 } else { -1 }, ti);
            return false;
        }
        if t0.ws < 3 && (stricmp(&txt, b"Ctr") == 0 || stricmp(&txt, b"Pkwy") == 0) {
            let p = token_at(&st.done, &st.ahead, st.ahead.n - 1, -1);
            if p.is_some_and(|p| p.ty == 1 && upper(p.text[0])) {
                let w: &[u8] = if t0.len == 3 { b"Center" } else { b"Parkway" };
                st.q.add(w, t0.start, t0.end, b'A', b'D', upper(txt[0]));
                return true;
            }
            return a29d(st, cur, ti);
        }
        if t0.ws < 3 && t0.ty == 1 && stricmp(&txt, b"w") == 0 {
            if let Some(sl) = push(cx, st, cur) {
                if stricmp(&sl.text, b"/") == 0 && sl.ws < 2 {
                    // `w/` is "with" unless a unit (or a number and a unit) follows the slash.
                    let unit = |st: &mut State, cur: &mut usize, t: Option<Tok>| {
                        let Some(mut t) = t else { return false };
                        lookup_tok(cx, st, cur, units::ALL, &mut t, false, 0).is_some()
                    };
                    let n1 = push(cx, st, cur);
                    if n1.is_some_and(|t| t.ty == 1) {
                        if !unit(st, cur, n1) {
                            st.q.add(b"with", t0.start, sl.end, b'A', b'D', false);
                            pop_i(st, cur, ti);
                            return true;
                        }
                        seek_i(st, cur, -2, ti);
                        return true;
                    }
                    let n2 = push(cx, st, cur);
                    if unit(st, cur, n2) {
                        seek_i(st, cur, -3, ti);
                        return true;
                    }
                    st.q.add(b"with", t0.start, sl.end, b'A', b'D', false);
                    seek_i(st, cur, -2, ti);
                    return true;
                }
            }
            return a29d(st, cur, ti);
        }
        let k = ABBR.find(&txt, true);
        if t0.ty == 1 && k >= 0 {
            st.q.add(ABBR.field(k, 0x10), t0.start, t0.end, b'A', b'D', upper(txt[0]));
            return true;
        }
        return a29d(st, cur, ti);
    }
    // "st": street after a number, saint before a name.
    if !upper(txt[0]) {
        let Some(nx) = push_i(cx, st, cur, ti) else { return restore(st, cur, ti) };
        let after;
        let had_dot;
        if dot(&nx) {
            had_dot = true;
            let Some(n2) = push_i(cx, st, cur, ti) else { return restore(st, cur, ti) };
            after = n2;
            pop_i(st, cur, ti);
        } else {
            had_dot = false;
            after = nx;
            pop_i(st, cur, ti);
        }
        if after.ty != 1 || !had_dot {
            return false;
        }
        st.q.add(b"street", t0.start, t0.end, b'A', b'D', false);
        pop_i(st, cur, ti);
        return true;
    }
    let Some(nx) = push_i(cx, st, cur, ti) else { return restore(st, cur, ti) };
    let after;
    let had_dot;
    if dot(&nx) {
        had_dot = true;
        let Some(n2) = push_i(cx, st, cur, ti) else { return restore(st, cur, ti) };
        after = n2;
        pop_i(st, cur, ti);
    } else {
        had_dot = false;
        after = nx;
        pop_i(st, cur, ti);
    }
    if after.ty == 1 && upper(after.text[0]) {
        st.q.add(b"saint", t0.start, t0.end, b'A', b'D', true);
        return true;
    }
    st.q.add(b"street", t0.start, t0.end, b'A', b'D', true);
    if had_dot {
        pop_i(st, cur, ti);
    }
    true
}

/// `FUN_1004fb00`: digits with commas inside.
fn digits_commas(t: &[u8]) -> bool {
    let t = cs(t);
    let l = t.len();
    for i in 0..l {
        if t[i] == b',' {
            if i == 0 || i == l - 1 {
                return false;
            }
        } else if !is_digit(t[i]) {
            return false;
        }
    }
    true
}

/// `FUN_10027500`.
pub fn abbrev(cx: &Cx, st: &mut State, cur: &mut usize, _t: &Tok) -> i32 {
    let n0 = st.ahead.n;
    let mut ti = n0 as isize - 1;
    if tok_at(st, ti).ty != 1 {
        return 5;
    }
    let p0 = st.done.toks.len();
    let w0 = st.wa.n();
    let fail = |st: &mut State, cur: &mut usize, r: i32| {
        st.wa.set_n(w0);
        if w0 > 0 {
            let e = st.wa.ri32(w0 - 1, 4);
            st.wa.set_consumed(e);
        }
        st.ahead.n = n0;
        if let Some(p) = st.ahead.pos[n0 as usize] {
            *cur = p;
        }
        st.done.toks.truncate(p0);
        r
    };
    let cls = if titles(cx, st, cur, &mut ti) {
        0x16
    } else if suffixes(cx, st, cur, &mut ti) {
        0x17
    } else if alnum(cx, st, cur, &mut ti) {
        0x18
    } else if general(cx, st, cur, &mut ti) {
        0x16
    } else {
        if st.ahead.n != n0 {
            st.ahead.n = n0;
            if let Some(p) = st.ahead.pos[n0 as usize] {
                *cur = p;
            }
        }
        return 5;
    };
    let mut r = st.done.append_all(&st.ahead, cls);
    if r == 1 {
        r = st.q.flush(&mut st.wa);
        if r == 1 {
            return 1;
        }
    }
    fail(st, cur, r)
}
