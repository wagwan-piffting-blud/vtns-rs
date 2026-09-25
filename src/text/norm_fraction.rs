//! `FUN_1003c1a0` (dispatch order 13, "fraction"), with its context check
//! `FUN_1003cfb0`, validator `FUN_1003c760`, word builder `FUN_1003cbb0` and
//! emitter `FUN_1003c610`. class 0xd.
//!
//! Up to 6 slots, the first from the shared (mode 1) token and the rest in
//! token mode 0xd: `N` a well-formed number, `/` a glued slash, `F` a
//! fraction glyph (`0x10095a08`: EUC-KR pairs and CP1252 `¼ ½ ¾`). A power
//! glyph glued to a number (`0x10095c48`: ², ³, ...) folds into that number's
//! slot. The tag string is tried longest first against `F NF N/N NN/N`.
//!
//! The validator is what keeps ordinary slashes out: a bare `N/N` is only a
//! fraction when both sides are single digits (numerator 1-3, denominator
//! 2-4) unless the context check finds "about"/`-`/`+` before it or an
//! operator, "of", a unit or a cooking measure after it -- otherwise
//! `numexpr` reads "5/10" as "five slash ten". Any shape directly followed
//! by another `/` is rejected.

use super::cstr::*;
use super::norm_units::{dict_find, phrase_lookup, DICT_C, DICT_E, DICT_P};
use super::numwords::*;
use super::tabs::{ptab, rec30, search, Ptab, Rec30};
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::window::{back_words, last_two};
use super::Fe;

const CLASS: u32 = 0xd;
const GLYPHS: Rec30 = rec30(0x10095a08, 0x1009b084);
const POWERS: Rec30 = rec30(0x10095c48, 0x1009b088);
const PATTERNS: Ptab = ptab(0x100804a8, 0x100804b8);
const MEASURES: Ptab = ptab(0x1009ae0c, 0x1009b12c);

#[derive(Clone, Default)]
struct Slot {
    ws: i32,
    start: i32,
    end: i32,
    len: i32,
    tag: u8,
    text: Vec<u8>,
    exp: Vec<u8>,
    /// +0x14: power glyph index + 1.
    pow: i32,
    /// +0x16: the slash was absorbed into the fraction words.
    silent: bool,
    /// +0x18: the value is at least 1 (plural).
    plural: bool,
}

fn tk(fe: &Fe, t: &[u8], p: usize, off: i32, mode: i32) -> (Tok, i32) {
    let mut x = Tok::default();
    let r = tokenize(&mut x, t, p, off, mode, fe.ud.as_ref());
    (x, r)
}

fn all_digits(w: &[u8]) -> bool {
    let w = cs(w);
    !w.is_empty() && w.iter().all(|&c| cls(c) & 0x10 != 0)
}

fn atof_digits(w: &[u8]) -> f64 {
    let d = digits_only(w);
    let mut v = 0f64;
    for &c in &d {
        v = v * 10.0 + (c - b'0') as f64;
    }
    v
}

fn cstring(buf: &[u8]) -> Vec<u8> {
    cs(buf).to_vec()
}

pub fn fraction(fe: &Fe, wa: &mut Wa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let mut sl: Vec<Slot> = Vec::new();
    let mut prev_tag = 0u8;
    let (mut q, mut qo) = (p, off);
    let mut k = 0;
    while sl.len() < 6 {
        let (x, r) = if k == 0 {
            (tok.clone(), tok.consumed)
        } else {
            let (x, r) = tk(fe, t, q, qo, 0xd);
            if r == 0 || x.class == 9 {
                break;
            }
            (x, r)
        };
        let w = x.text();
        let c0 = at(w, 0);
        let slot = |tag: u8, exp: Vec<u8>, sl: &mut Vec<Slot>| {
            sl.push(Slot { ws: x.ws, start: x.start, end: qo + r, len: x.len, tag, text: w.to_vec(), exp, ..Default::default() });
        };
        if (x.class == 3 || x.class == 7) && x.len == 1 && c0 == b'/' && k > 0 && x.ws == 0 {
            slot(b'/', Vec::new(), &mut sl);
            prev_tag = b'/';
        } else if x.class == 3 && (x.len == 1 || x.len == 2) && GLYPHS.find(w, true) >= 0 {
            let i = GLYPHS.find(w, true);
            slot(b'F', GLYPHS.field(i, 0x10).to_vec(), &mut sl);
            prev_tag = b'F';
        } else if k > 0 && prev_tag == b'N' && x.class == 3 && (x.len == 1 || x.len == 2) && x.ws == 0 && POWERS.find(w, true) >= 0 {
            let i = POWERS.find(w, true);
            let last = sl.last_mut().unwrap();
            last.end += 1;
            last.len += 1;
            last.pow = i + 1;
            prev_tag = b'S';
        } else if well_formed(w) {
            slot(b'N', Vec::new(), &mut sl);
            prev_tag = b'N';
        } else {
            break;
        }
        q += r as usize;
        qo += r;
        k += 1;
    }

    let tags: Vec<u8> = sl.iter().map(|x| x.tag).collect();
    let mode = match tags.first() {
        Some(b'F') => 1,
        Some(b'N') => 2,
        _ => return -1,
    };
    let mut ctx = false;
    let mut n = sl.len();
    while n >= 1 {
        let (lo, hi) = match (mode, n) {
            (1, 1) => (0, 1),
            (2, 2) => (1, 2),
            (2, 3) => (2, 3),
            (2, 4) => (3, PATTERNS.count()),
            _ => (0, 0),
        };
        for idx in lo..hi {
            if strcmp(PATTERNS.get(idx), &tags[..n]) != 0 {
                continue;
            }
            let consumed = sl[n - 1].end - off;
            let mut next = Tok::default();
            if idx != 0 && context(fe, t, p, off, consumed, &mut next) {
                ctx = true;
            }
            let mut s2 = sl[..n].to_vec();
            if !validate(fe, &mut s2, idx, ctx, &next, t, p, off) {
                return -1;
            }
            return if emit(wa, &s2, idx) { consumed } else { 0 };
        }
        n -= 1;
    }
    -1
}

/// `FUN_1003cfb0` (mode 'B'): "about"/`-`/`+` right before, or an operator,
/// "of", a unit or a cooking measure right after. `next` receives the first
/// token after the shape.
fn context(fe: &Fe, t: &[u8], p: usize, off: i32, consumed: i32, next: &mut Tok) -> bool {
    let mut buf = [0u8; 96];
    let bw = back_words(&mut buf, t, p as isize - off as isize, p as isize, 2);
    let (_, b) = if bw as i16 != 0 { last_two(&buf, off - bw, 0).unwrap_or_default() } else { Default::default() };
    let mut fwd: Vec<Tok> = Vec::new();
    let mut used = consumed;
    loop {
        let (x, r) = tk(fe, t, p + used as usize, off + used, 0);
        if r == 0 {
            break;
        }
        if fwd.is_empty() {
            *next = x.clone();
        }
        if x.class == 8 || x.class == 9 {
            break;
        }
        used += r;
        fwd.push(x);
        if fwd.len() > 1 {
            break;
        }
    }
    let b0 = b.text[0];
    if b.len == 1 && b0 != 0 && strchr(s(0x100804ec), b0) {
        return true;
    }
    if b.len >= 1 && b.class == 1 && stricmp(b.text(), s(0x10077538)) == 0 {
        return true;
    }
    let f = fwd.first().cloned().unwrap_or_default();
    let fw = f.text();
    if f.ws < 2 {
        if (f.len == 1 && fw[0..].first().is_some_and(|&c| c != 0 && strchr(s(0x100804e4), c))) || stricmp(fw, s(0x10077368)) == 0 {
            return true;
        }
        let unitish = f.class == 1
            || (f.class == 3 && f.len == 1 && at(fw, 0) == b'%')
            || dbcs_pair(at(fw, 0), at(fw, 1));
        if unitish {
            let mut out = [0u8; 256];
            let mut u = f.consumed;
            if dict_find(&mut out, &DICT_E, fw, false)
                || dict_find(&mut out, &DICT_P, fw, false)
                || dict_find(&mut out, &DICT_C, fw, false)
                || phrase_lookup(fe, &mut out, fw, t, p, off, &mut u)
            {
                return true;
            }
        }
    }
    search(MEASURES.count(), true, fw, |i| MEASURES.get(i)) >= 0
}

/// `FUN_1003cbb0`: numerator/denominator words, or `None` when the pair
/// isn't a plain positive fraction. Sets `plural` (value >= 1) and `silent`
/// (the slash is read into the denominator word).
fn frac_words(num: &[u8], npow: i32, den: &[u8], dpow: i32, plural: &mut bool, silent: &mut bool) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut a = atof_digits(num);
    let mut b = atof_digits(den);
    if a == 0.0 || b == 0.0 || !all_digits(num) || !all_digits(den) {
        return None;
    }
    for _ in 1..npow.max(1) {
        a *= a;
    }
    for _ in 1..dpow.max(1) {
        b *= b;
    }
    *plural = a / b >= 1.0;
    let mut buf = new_buf();
    let num_words = |buf: &mut [u8]| -> Option<Vec<u8>> {
        number(buf, num, 0);
        if buf[0] == 0 {
            None
        } else {
            Some(cstring(buf))
        }
    };
    if b == 2.0 || b == 4.0 {
        let (small, one, many) = if b == 2.0 {
            (s(0x10078850), s(0x10078bc4), s(0x100804dc))
        } else {
            (s(0x10078994), s(0x100804d4), s(0x100804c8))
        };
        let nw = if a == 1.0 { s(0x10078a64).to_vec() } else { num_words(&mut buf)? };
        if dpow > 0 {
            return Some((nw, small.to_vec()));
        }
        *silent = true;
        return Some((nw, if a == 1.0 { one } else { many }.to_vec()));
    }
    let nw = num_words(&mut buf)?;
    let mut d = new_buf();
    if dpow < 1 {
        ordinal_numeral_words(&mut d, den);
        if d[0] == 0 {
            return None;
        }
        if a > 1.0 {
            strcat(&mut d, s(0x10077770));
        }
        *silent = true;
    } else {
        number(&mut d, den, 0);
        if d[0] == 0 {
            return None;
        }
    }
    Some((nw, cstring(&d)))
}

/// `FUN_1003c760`.
#[allow(clippy::too_many_arguments)]
fn validate(fe: &Fe, sl: &mut [Slot], idx: i32, ctx: bool, next: &Tok, t: &[u8], p: usize, off: i32) -> bool {
    let _ = (fe, t, p, off);
    if next.len > 0 && next.ws < 2 && next.class == 3 && next.text[0] == b'/' {
        return false;
    }
    let num = |w: &[u8]| -> Option<Vec<u8>> {
        let mut b = new_buf();
        number(&mut b, w, 0);
        if b[0] == 0 {
            None
        } else {
            Some(cstring(&b))
        }
    };
    match idx {
        1 => {
            if sl[1].ws > 1 || (next.class == 2 && (sl[1].ws > 0 || next.ws == 0)) {
                return false;
            }
            match num(&sl[0].text) {
                Some(v) => sl[0].exp = v,
                None => return false,
            }
            true
        }
        2 => {
            if !ctx {
                let t0 = at(&sl[0].text, 0);
                let t2 = at(&sl[2].text, 0);
                if sl[0].len != 1 || !matches!(t0, b'1' | b'2' | b'3') || sl[2].len != 1 || !matches!(t2, b'2' | b'3' | b'4') {
                    return false;
                }
            }
            if sl[1].ws >= 1 || sl[2].ws >= 1 {
                return false;
            }
            if (next.class == 2 || next.class == 1) && next.ws == 0 {
                return false;
            }
            if sl[0].text.is_empty() || sl[2].text.is_empty() {
                return false;
            }
            let (mut plural, mut silent) = (false, false);
            let Some((nw, dw)) = frac_words(&sl[0].text, sl[0].pow, &sl[2].text, sl[2].pow, &mut plural, &mut silent) else { return false };
            sl[2].plural = plural;
            sl[1].silent = silent;
            sl[0].exp = nw;
            sl[2].exp = dw;
            let (st, en) = (sl[0].start, sl[2].end);
            for x in sl.iter_mut().take(3) {
                x.start = st;
                x.end = en;
            }
            true
        }
        3 => {
            if sl[1].ws >= 2 || sl[2].ws >= 1 || sl[3].ws >= 1 {
                return false;
            }
            if next.class == 2 && next.ws < 2 {
                return false;
            }
            match num(&sl[0].text) {
                Some(v) => sl[0].exp = v,
                None => return false,
            }
            if sl[1].text.is_empty() || sl[3].text.is_empty() {
                return false;
            }
            let (mut plural, mut silent) = (false, false);
            let Some((nw, dw)) = frac_words(&sl[1].text, sl[1].pow, &sl[3].text, sl[3].pow, &mut plural, &mut silent) else { return false };
            sl[3].plural = true;
            sl[2].silent = silent;
            sl[1].exp = nw;
            sl[3].exp = dw;
            let (st, en) = (sl[1].start, sl[3].end);
            for x in sl.iter_mut().skip(1).take(3) {
                x.start = st;
                x.end = en;
            }
            true
        }
        _ => true,
    }
}

/// `FUN_1003c610`.
fn emit(wa: &mut Wa, sl: &[Slot], idx: i32) -> bool {
    for (k, x) in sl.iter().enumerate() {
        if x.pow > 0 && !wa.emit(POWERS.field(x.pow - 1, 0x10), x.start, x.end, 0x53, 0x44, CLASS) {
            return false;
        }
        if x.tag == b'/' {
            let prev_pow = k > 0 && sl[k - 1].pow > 0;
            let next_pow = sl.get(k + 1).is_some_and(|y| y.pow > 0);
            if (!x.silent || prev_pow || next_pow) && !wa.emit_split(s(0x10077500), x.start, x.end, 0x53, 0x44, CLASS) {
                return false;
            }
        } else {
            if !wa.emit_split(&x.exp, x.start, x.end, 0x44, 0x44, CLASS) {
                return false;
            }
            if (idx == 1 || idx == 3) && k == 0 && !wa.emit(s(0x10077400), sl[0].start, sl[0].end, 0x41, 0x44, CLASS) {
                return false;
            }
        }
    }
    if sl.last().is_some_and(|x| x.plural) {
        let n = wa.n() as i32;
        wa.set_i16(Wa::rec(n - 1) + 0x1e, 1);
    }
    true
}
