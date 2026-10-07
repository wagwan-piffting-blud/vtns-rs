//! `FUN_100570c0` (dispatch order 8): phone numbers. Transliterated from the
//! decompile with its label entry `FUN_10057b20`, extension reader
//! `FUN_10057e60`, validator `FUN_100578a0`, emitter `FUN_100576a0` and group
//! reader `FUN_100582a0`. class 0x1b.
//!
//! An optional label ("TEL", "FAX", "PH", "telephone", "facsimile", or one
//! in parentheses) comes first, then up to 9 slots in token mode 6: digit
//! groups `N` and the separators `( ) / - .`; a leading `+` is read "plus"
//! and must be glued to the first group. The slot tags are tried longest
//! first against 21 shapes, bucketed by how many groups they have. Groups
//! are checked right to left against phone-number lengths (subscriber 4,
//! exchange 3-4, area 1-3, country 1-3; tighter after country code 1). The
//! short or unpunctuated shapes (`NNNN NNN N/N N-N NN N`) additionally need
//! a label or a phone word nearby (`FUN_100580e0`). An extension ("x", "ext",
//! "ex", "xt" + up to 4 digits) may follow.
//!
//! Every group is read over the whole number's span, digit by digit with 0
//! as "zero", except `N00` and `NN00`/`N000` which read as hundreds/years.
//! Every group but the last ends with following-punctuation code 7.

use super::cstr::*;
use super::numwords::{digits, new_buf, number, year};
use super::phrases::phone_context;
use super::tabs::{ptab, rec30, search, Ptab, Rec30};
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::window::prev_one;
use super::Fe;

const LABELS: Rec30 = rec30(0x1008aa28, 0x1009b064);
const PATTERNS: Ptab = ptab(0x1009c0f0, 0x1009c144);
const EXT: Ptab = ptab(0x1008ab48, 0x1009b068);

fn tk(fe: &Fe, t: &[u8], p: usize, off: i32, mode: i32) -> (Tok, i32) {
    let mut x = Tok::default();
    let r = tokenize(&mut x, t, p, off, mode, fe.ud.as_ref());
    (x, r)
}

fn all_digits(w: &[u8]) -> bool {
    let w = cs(w);
    !w.is_empty() && w.iter().all(|&c| cls(c) & 0x10 != 0)
}

#[derive(Clone, Default)]
struct Slot {
    ws: i32,
    start: i32,
    end: i32,
    len: i32,
    tag: u8,
    text: Vec<u8>,
    exp: Vec<u8>,
}

impl Slot {
    fn of(x: &Tok, end: i32, tag: u8) -> Slot {
        Slot { ws: x.ws, start: x.start, end, len: x.len, tag, text: x.text().to_vec(), exp: Vec::new() }
    }
}

/// The label/plus/extension scratch slots (`local_644`: label, "+",
/// extension word, extension number).
#[derive(Default)]
struct Extra {
    label: Slot,
    plus: Slot,
    ext: Slot,
    extnum: Slot,
}

/// `FUN_100582a0`.
pub(super) fn group_words(w: &[u8]) -> Vec<u8> {
    let w = cs(w);
    let d = |c: u8| (b'1'..=b'9').contains(&c);
    let mut buf = new_buf();
    if w.len() == 3 && d(w[0]) && w[1] == b'0' && w[2] == b'0' {
        number(&mut buf, w, 1);
    } else if w.len() == 4 && d(w[0]) && ((d(w[1]) && w[2] == b'0' && w[3] == b'0') || (w[1] == b'0' && w[2] == b'0' && w[3] == b'0')) {
        year(&mut buf, w);
    } else {
        digits(&mut buf, w, 1, 1);
    }
    cs(&buf).to_vec()
}

fn is_label(w: &[u8]) -> Option<i32> {
    let i = LABELS.find(w, true);
    if i >= 0 || stricmp(s(0x1009b8b8), w) == 0 || stricmp(s(0x1009c1d4), w) == 0 {
        Some(i)
    } else {
        None
    }
}

/// `FUN_10057b20`: a label before the number; returns the bytes it takes.
fn label_entry(fe: &Fe, wa: &Wa, ex: &mut Extra, tok: &Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let fill = |ex: &mut Extra, x: &Tok, i: i32| {
        ex.label = Slot::of(x, x.end, b'M');
        ex.label.exp = if i >= 0 { LABELS.field(i, 0x10).to_vec() } else { x.text().to_vec() };
    };
    if tok.text[0] == b'(' && tok.len == 1 {
        let mut base = tok.consumed;
        let (x, r) = tk(fe, t, p + base as usize, off + base, 6);
        if r == 0 || x.class == 9 {
            return 0;
        }
        let Some(i) = is_label(x.text()) else { return 0 };
        if (x.class != 1 && x.class != 3) || x.ws > 2 {
            return 0;
        }
        fill(ex, &x, i);
        base += r;
        let (y, r2) = tk(fe, t, p + base as usize, off + base, 6);
        if y.text[0] == b')' && y.len == 1 && y.ws <= 2 {
            return base + r2;
        }
        return base;
    }
    let Some(i) = is_label(tok.text()) else { return 0 };
    if tok.class != 1 && tok.class != 3 {
        return 0;
    }
    let n = wa.n();
    if n != 0 && (n < 1 || (tok.ws < 1 && tok.nl < 1)) {
        return 0;
    }
    fill(ex, tok, i);
    let base = tok.consumed;
    let (y, r) = tk(fe, t, p + base as usize, off + base, 6);
    if (matches!(y.text[0], b')' | b':') || (y.text[0] == b'.' && !fe.era.is_2009())) && y.len == 1 && y.ws <= 2 {
        return base + r;
    }
    if era() == crate::era::Era::Paul09 {
        // paul09 FUN_100522c0: a label is kept when whitespace or a short "." follows.
        return if y.ws != 0 || (y.len < 3 && y.text[0] == b'.') { base } else { 0 };
    }
    if i >= 0 && y.len > 0 {
        let not_period = y.len > 2 || y.text[0] != b'.';
        if (y.ws != 0 || not_period) && y.class == 1 {
            return 0;
        }
        if y.ws == 0 && not_period {
            return 0;
        }
    }
    base
}

/// `FUN_10057e60`: "x55", "ext. 123", ...; returns the bytes it takes.
fn ext_read(fe: &Fe, ex: &mut Extra, t: &[u8], p: usize, off: i32) -> i32 {
    let (x, r1) = tk(fe, t, p, off, 6);
    if r1 == 0 || x.class == 9 {
        return 0;
    }
    if search(EXT.count(), true, x.text(), |i| EXT.get(i)) < 0 || x.ws > 2 || x.class != 1 {
        return 0;
    }
    ex.ext = Slot::of(&x, x.end, b'M');
    ex.ext.exp = s(0x10081330).to_vec();
    let mut cons = r1;
    let (y, r2) = tk(fe, t, p + r1 as usize, off + r1, 6);
    let num = |ex: &mut Extra, z: &Tok| {
        ex.extnum = Slot::of(z, z.end, b'M');
        ex.extnum.exp = group_words(z.text());
    };
    if y.ws == 0 && y.len == 1 && y.text[0] == b'.' {
        cons += r2;
        let (z, r3) = tk(fe, t, p + cons as usize, off + cons, 6);
        if z.ws > 2 || z.class != 2 || z.len > 4 || !all_digits(z.text()) {
            return 0;
        }
        num(ex, &z);
        return cons + r3;
    }
    if y.ws < 3 && y.class == 2 && y.len < 5 && all_digits(y.text()) {
        num(ex, &y);
        return cons + r2;
    }
    ex.ext = Slot::default();
    0
}

/// `FUN_100583f0`, `FUN_10058390`, `FUN_10058350`, `FUN_10058450`.
fn exch_ok(w: &[u8], cc: i32) -> bool {
    let l = cs(w).len();
    all_digits(w) && if cc == 1 { l == 3 } else { (3..=4).contains(&l) }
}

fn area_ok(w: &[u8], cc: i32) -> bool {
    let l = cs(w).len();
    all_digits(w) && if cc == 1 { (2..=3).contains(&l) } else { (1..=3).contains(&l) }
}

fn short_ok(w: &[u8]) -> bool {
    (1..=3).contains(&cs(w).len()) && all_digits(w)
}

fn four_ok(w: &[u8]) -> bool {
    cs(w).len() == 4 && all_digits(w)
}

/// julie09 `FUN_10053900` over the 18-entry pattern table.
fn validate09(sl: &[Slot], idx: i32, ex: &Extra, flag: bool, prev: &Tok, ctx: &Tok) -> bool {
    let mut k = if idx < 8 {
        4
    } else if idx < 0x10 {
        3
    } else {
        if !flag {
            return false;
        }
        2
    };
    let mut cc = 0;
    for x in sl {
        if x.tag != b'N' {
            continue;
        }
        k -= 1;
        let ok = match k {
            0 => four_ok(&x.text),
            1 => exch_ok(&x.text, cc),
            2 => area_ok(&x.text, cc),
            3 => {
                if short_ok(&x.text) {
                    cc = atoi(&x.text);
                    true
                } else {
                    false
                }
            }
            _ => true,
        };
        if !ok {
            return false;
        }
    }
    let quiet = |prev: &Tok, ctx: &Tok| !(prev.len > 0 && prev.class == 2) && !(ctx.len > 0 && ctx.class == 2);
    match idx {
        7 => flag || ex.label.len != 0 || ex.plus.len != 0 || ex.ext.len != 0 || quiet(prev, ctx),
        0xf => flag || ex.label.len != 0 || ex.ext.len != 0 || quiet(prev, ctx),
        0x10 | 0x11 => flag,
        _ => true,
    }
}

/// `FUN_100578a0`.
fn validate(sl: &[Slot], idx: i32, ex: &Extra, flag: bool, prev: &Tok, ctx: &Tok) -> bool {
    if era().is_2009() {
        return validate09(sl, idx, ex, flag, prev, ctx);
    }
    if idx < 0x11 {
        let mut k = if idx < 9 { 4 } else { 3 };
        let mut cc = 0;
        for x in sl {
            if x.tag != b'N' {
                continue;
            }
            k -= 1;
            let ok = match k {
                0 => four_ok(&x.text),
                1 => exch_ok(&x.text, cc),
                2 => area_ok(&x.text, cc),
                3 => {
                    if short_ok(&x.text) {
                        cc = atoi(&x.text);
                        true
                    } else {
                        false
                    }
                }
                _ => true,
            };
            if !ok {
                return false;
            }
        }
    } else {
        if !flag {
            return false;
        }
        if idx == 0x12 && !(exch_ok(&sl[0].text, 0) && sl[1].ws == 0 && four_ok(&sl[2].text)) {
            return false;
        }
        if idx == 0x13 && !(exch_ok(&sl[0].text, 0) && sl[1].ws == 1 && four_ok(&sl[1].text)) {
            return false;
        }
    }
    let quiet = |prev: &Tok, ctx: &Tok| !(prev.len > 0 && prev.class == 2) && !(ctx.len > 0 && ctx.class == 2);
    match idx {
        8 => flag || ex.label.len != 0 || ex.plus.len != 0 || ex.ext.len != 0 || quiet(prev, ctx),
        0x10 => flag || ex.label.len != 0 || ex.ext.len != 0 || quiet(prev, ctx),
        0x11 => short_ok(&sl[0].text) && (9..=12).contains(&sl[2].len) && flag,
        0x12 | 0x13 => flag,
        0x14 => (10..=14).contains(&sl[0].len) && flag,
        _ => true,
    }
}

/// `FUN_100576a0`.
fn emit(wa: &mut Wa, sl: &[Slot], ex: &Extra) -> bool {
    let set7 = |wa: &mut Wa| {
        let n = wa.n() as i32;
        wa.set_ri32(n - 1, 0x2c, 7);
    };
    if ex.label.len > 0 {
        // julie09 FUN_10053710 speaks the label as "the <label> number is" (s(0x100975c4)).
        let text = if era().is_2009() {
            let mut v = b"the ".to_vec();
            v.extend_from_slice(cs(&ex.label.exp));
            v.extend_from_slice(b" number is");
            v
        } else {
            ex.label.exp.clone()
        };
        if !wa.emit_split(&text, ex.label.start, ex.label.end, 0x41, 0x44, 0x1b) {
            return false;
        }
        set7(wa);
    }
    if ex.plus.len > 0 && !wa.emit(s(0x10080460), ex.plus.start, ex.plus.end, 0x53, 0x44, 0x1b) {
        return false;
    }
    let (st, en) = (sl[0].start, sl[sl.len() - 1].end);
    for (i, x) in sl.iter().enumerate() {
        if x.tag != b'N' {
            continue;
        }
        if !wa.emit_split(&group_words(&x.text), st, en, 0x44, 0x44, 0x1b) {
            return false;
        }
        if i + 1 < sl.len() {
            set7(wa);
        }
    }
    if ex.ext.len > 0 && ex.extnum.len > 0 {
        set7(wa);
        if !wa.emit(&ex.ext.exp, ex.ext.start, ex.ext.end, 0x41, 0x44, 0x1b) {
            return false;
        }
        if !wa.emit_split(&ex.extnum.exp, ex.extnum.start, ex.extnum.end, 0x44, 0x44, 0x1b) {
            return false;
        }
        set7(wa);
    }
    true
}

pub fn phone(fe: &Fe, wa: &mut Wa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let mut ex = Extra::default();
    let lab = label_entry(fe, wa, &mut ex, tok, t, p, off);
    let mut flag = false;
    let mut prev = Tok::default();
    let (mut q, mut qo) = (p, off);
    if lab == 0 {
        prev = prev_one(t, p, off, 6);
    } else {
        q += lab as usize;
        qo += lab;
        flag = true;
    }
    let mut sl: Vec<Slot> = Vec::new();
    let mut k = 0i32;
    let mut plus_at = -1i32;
    loop {
        let (x, r) = tk(fe, t, q, qo, 6);
        if r == 0 || x.class == 9 || !(x.nl != 0 || x.ws < 3) {
            break;
        }
        let c0 = x.text[0];
        if sl.is_empty() && c0 == b'+' && x.len == 1 && plus_at == -1 {
            ex.plus = Slot::of(&x, x.end, b'+');
            ex.plus.len = 1;
            plus_at = k;
            k += 1;
        } else {
            let punct = c0 != 0 && strchr(s(0x1009c1cc), c0) && x.len == 1 && plus_at == -1;
            if punct {
                sl.push(Slot::of(&x, qo + r, c0));
            } else {
                if !all_digits(x.text()) {
                    break;
                }
                if plus_at >= 0 {
                    if plus_at + 1 != k || x.ws != 0 {
                        break;
                    }
                    plus_at = -1;
                }
                sl.push(Slot::of(&x, qo + r, b'N'));
            }
            k += 1;
            if sl.len() > if fe.era.is_2009() { 7 } else { 8 } {
                break;
            }
        }
        q += r as usize;
        qo += r;
    }

    let mut n = sl.len();
    let mut groups = sl.iter().filter(|x| x.tag == b'N').count() as i32;
    let mut first = true;
    loop {
        if n < 1 {
            if lab != 0 {
                return if wa.emit(&ex.label.exp, ex.label.start, ex.label.end, 0x41, 0x44, 0x1b) { lab } else { 0 };
            }
            return -1;
        }
        if !first && sl[n].tag == b'N' {
            groups -= 1;
        }
        first = false;
        let range = if era().is_2009() {
            // julie09 FUN_10053190: 18 patterns, no single-group entry.
            match (groups, n) {
                (2, 3) => Some((0x10, 0x11)),
                (2, 2) => Some((0x11, PATTERNS.count())),
                (3, 7) => Some((8, 9)),
                (3, 6) => Some((9, 10)),
                (3, 5) => Some((10, 0xe)),
                (3, 4) => Some((0xe, 0xf)),
                (3, 3) => Some((0xf, 0x10)),
                (4, 8) => Some((0, 1)),
                (4, 7) => Some((1, 4)),
                (4, 6) => Some((4, 7)),
                (4, 4) => Some((7, 8)),
                _ => None,
            }
        } else {
            match (groups, n) {
            (1, 1) => Some((20, PATTERNS.count())),
            (2, 3) => Some((0x11, 0x13)),
            (2, 2) => Some((0x13, 0x14)),
            (3, 7) => Some((9, 10)),
            (3, 6) => Some((10, 11)),
            (3, 5) => Some((11, 15)),
            (3, 4) => Some((15, 16)),
            (3, 3) => Some((16, 17)),
            (4, 9) => Some((0, 1)),
            (4, 8) => Some((1, 2)),
            (4, 7) => Some((2, 5)),
            (4, 6) => Some((5, 8)),
            (4, 4) => Some((8, 9)),
            _ => None,
            }
        };
        if let Some((lo, hi)) = range {
            let tags: Vec<u8> = sl[..n].iter().map(|x| x.tag).collect();
            for idx in lo..hi {
                if strcmp(PATTERNS.get(idx), &tags) != 0 {
                    continue;
                }
                let consumed = sl[n - 1].end - off;
                let mut ctx = Tok::default();
                let ctx_idx = if fe.era.is_2009() { matches!(idx, 7 | 0xf | 0x10 | 0x11) } else { matches!(idx, 8 | 0x10 | 0x11 | 0x12 | 0x13 | 0x14) };
                if ctx_idx && !flag && phone_context(fe, t, p, off, b'B', consumed, Some(&mut ctx)) {
                    flag = true;
                }
                let e = ext_read(fe, &mut ex, t, p + consumed as usize, off + consumed);
                if validate(&sl[..n], idx, &ex, flag, &prev, &ctx) {
                    return if emit(wa, &sl[..n], &ex) { consumed + e as i16 as i32 } else { 0 };
                }
                break;
            }
        }
        n -= 1;
    }
}
