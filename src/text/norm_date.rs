//! `FUN_10036940`: dates. Up to six slots (weekday, "the", day, month, year,
//! "of", BC/AD, numeric groups joined by `/ - .`) are collected into the
//! scratch at WA+0x39e4, the slot classes are matched longest-first against
//! the 26 patterns at 0x100801b0, `FUN_10037e90` validates and assigns
//! D/M/Y roles, and `FUN_100376d0` emits the words (class 4).

use super::cstr::*;
use super::numwords::*;
use super::phrases::phrase_find;
use super::sbd::all_digit;
use super::tabs::{ptab, ptabn, rec30};
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::window::prev_two;
use super::Fe;

const DAYS: u32 = 0x10080290;

/// One 0x140-byte scratch slot (offsets relative to scratch+8+k*0x140).
#[derive(Clone)]
struct Slot {
    ws: i32,
    start: i32,
    end: i32,
    len: i32,
    /// +0x10 token class.
    class: u8,
    /// +0x12 role: w W T O I s S A F M N D Y.
    tag: u8,
    /// +0x14 a comma follows (5).
    comma: i16,
    /// +0x16 an abbreviation dot follows.
    flag: i16,
    /// +0x18 separator that follows, sign-extended.
    sep: i16,
    /// +0x1c text position after the slot (at the slot for DAYS slots).
    tp: usize,
    text: [u8; 40],
    exp: [u8; 0x102],
}

impl Default for Slot {
    fn default() -> Self {
        Slot { ws: 0, start: 0, end: 0, len: 0, class: 0, tag: 0, comma: 0, flag: 0, sep: 0, tp: 0, text: [0; 40], exp: [0; 0x102] }
    }
}

struct Sc {
    n: i32,
    sl: Vec<Slot>,
}

fn tk(fe: &Fe, t: &[u8], p: usize, off: i32, mode: i32) -> (Tok, i32) {
    let mut x = Tok::default();
    let r = tokenize(&mut x, t, p, off, mode, fe.ud.as_ref());
    (x, r)
}

pub(super) fn month_name(i: i32) -> &'static [u8] {
    ptabn(0x10089bf8, 13).get(i)
}

fn is_days(w: &[u8]) -> bool {
    strcmp(w, s(DAYS)) == 0
}

/// `FUN_10039530`: 1..31 as one or two digits.
pub(super) fn day(w: &[u8]) -> i32 {
    if !all_digit(w) {
        return -1;
    }
    let l = strlen(w);
    let v = atoi(w);
    if l > 0 && l < 3 && v > 0 && v < 0x20 {
        return v;
    }
    -1
}

/// `FUN_10039580`: 1..12 as one or two digits.
pub(super) fn mon(w: &[u8]) -> i32 {
    if !all_digit(w) {
        return -1;
    }
    let l = strlen(w);
    let v = atoi(w);
    if cls(at(w, 0)) & 0x10 != 0 && l > 0 && l < 3 && v > 0 && v < 0xd {
        return v;
    }
    -1
}

/// `FUN_100395e0`: a year of 2..4 digits (mode 0 also 5+ digits not starting with 0).
pub(super) fn yr(w: &[u8], mode: i16) -> i32 {
    let l = strlen(w) as i32;
    let v = atoi(w);
    if !all_digit(w) {
        return -1;
    }
    let two = || if (0..=99).contains(&v) { v } else { -1 };
    match mode {
        1 => match l {
            _ if l < 2 => -1,
            2 => two(),
            3 => {
                if (100..=999).contains(&v) {
                    v
                } else {
                    -1
                }
            }
            4 => {
                if v < 0 {
                    -1
                } else {
                    v
                }
            }
            _ => -1,
        },
        2 => match l {
            _ if l < 2 => -1,
            2 => two(),
            4 => {
                if v < 0 {
                    -1
                } else {
                    v
                }
            }
            _ => -1,
        },
        _ => {
            if l < 2 {
                return -1;
            }
            if (l == 2 && (0..100).contains(&v)) || (l == 3 && (100..1000).contains(&v)) || (l == 4 && v > 999) {
                return v;
            }
            if at(w, 0) == b'0' || v < 10000 {
                return -1;
            }
            v
        }
    }
}

/// `FUN_100394c0`: "3rd"-style day ordinal -> 1..31.
fn day_ord(w: &[u8]) -> i32 {
    let l = strlen(w);
    if l > 0 && l < 5 && ordinal_numeral(w) {
        let mut buf = [0u8; 32];
        strncpy(&mut buf, w, l - 2);
        buf[l - 2] = 0;
        let v = atoi(&buf);
        if v > 0 && v < 0x20 {
            return v;
        }
    }
    -1
}

/// `FUN_100393b0`: digits (a leading `'` allowed) or a day ordinal.
pub(crate) fn numish(w: &[u8]) -> bool {
    let n = strlen(w);
    for i in 0..n {
        if cls(w[i]) & 0x10 == 0 && (i != 0 || n < 2 || w[0] != b'\'') {
            return day_ord(w) >= 0;
        }
    }
    true
}

fn is_dash(c: u8) -> bool {
    matches!(c, b'-' | b'~' | 0x96 | 0x97)
}

fn and_or(w: &[u8]) -> bool {
    stricmp(w, s(0x1007901c)) == 0 || stricmp(w, s(0x10077400)) == 0
}

/// `FUN_100396d0`: a day list "5, 6 and 7" -> bytes consumed.
fn day_list(fe: &Fe, x: &Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let mut l8 = -1;
    if x.class != 2 {
        return -1;
    }
    if day(&x.text) < 0 {
        l8 = day_ord(&x.text);
        if l8 < 0 {
            return -1;
        }
    }
    let mut u = x.consumed;
    let (y, r) = tk(fe, t, p + u as usize, off + u, 3);
    if r == 0 || y.text[0] != b',' || y.len != 1 || y.ws >= 2 {
        return -1;
    }
    let mut st = 1;
    u += r;
    if at(t, (p + u as usize) as isize) == 0 {
        return -1;
    }
    loop {
        let (y, r) = tk(fe, t, p + u as usize, off + u, 3);
        if r == 0 {
            return -1;
        }
        let ok = st != 0 && {
            let bad = ((l8 != -1 || day(&y.text) < 0) && {
                l8 = day_ord(&y.text);
                l8 < 0
            }) || y.ws > 1;
            !bad
        };
        if ok {
            if st == 2 {
                return u + r;
            }
            st = 0;
        } else if ((y.text[0] == b'&' && y.len == 1) || stricmp(&y.text, s(0x10077400)) == 0) && y.ws < 2 {
            st = 2;
        } else {
            if st != 0 || y.text[0] != b',' {
                return -1;
            }
            st = 1;
            if y.len != 1 || y.ws > 1 {
                return -1;
            }
        }
        u += r;
        if at(t, (p + u as usize) as isize) == 0 {
            return -1;
        }
    }
}

/// `FUN_10039860`: "5 or 6", "5-7", "1990 and 1991", "1990-95" ->
/// bytes consumed. `yr_flag` marks a year range, `range` a dash range.
fn range(fe: &Fe, x: &Tok, t: &[u8], p: usize, off: i32, yr_flag: &mut i16, rng: &mut i16) -> i32 {
    let at_ = |u: i32| (p + u as usize, off + u);
    let mut lc = -1;
    if x.class == 2 && (day(&x.text) >= 0 || {
        lc = day_ord(&x.text);
        lc >= 0
    }) {
        'days: {
            let mut u = x.consumed;
            let (q, o) = at_(u);
            let (y1, r1) = tk(fe, t, q, o, 3);
            if r1 == 0 {
                return -1;
            }
            let joiner = if and_or(&y1.text) {
                true
            } else if y1.text[0] == b'&' {
                if y1.len != 1 {
                    return -1;
                }
                true
            } else {
                false
            };
            if joiner && y1.ws < 2 {
                u += r1;
                let (q, o) = at_(u);
                let (y2, r2) = tk(fe, t, q, o, 3);
                if r2 == 0 {
                    return -1;
                }
                if day(&y2.text) >= 0 && y2.ws < 2 {
                    return u + r2;
                }
                break 'days;
            }
            if !is_dash(y1.text[0]) || y1.len != 1 || y1.ws > 1 {
                return -1;
            }
            u += r1;
            let (q, o) = at_(u);
            let (y2, r2) = tk(fe, t, q, o, 3);
            if r2 == 0 {
                return -1;
            }
            let mut i3 = -1;
            let num = (lc == -1 && day(&y2.text) >= 0) || {
                i3 = day_ord(&y2.text);
                i3 >= 0
            };
            if !(num && y2.ws < 2 && y2.ws == y1.ws) {
                break 'days;
            }
            let rising = if lc == -1 { atoi(&x.text) < atoi(&y2.text) } else { lc >= 0 && lc < i3 };
            if !rising {
                break 'days;
            }
            u += r2;
            let (q, o) = at_(u);
            let (z, rz) = tk(fe, t, q, o, 3);
            if z.class == 3 && z.len == 1 {
                if z.text[0] == y1.text[0] {
                    return -1;
                }
                if z.text[0] != 0 && strchr(s(0x100802d0), z.text[0]) {
                    let (q, o) = at_(u + rz);
                    let (w, _) = tk(fe, t, q, o, 3);
                    if w.class == 2 && w.ws < 2 {
                        return -1;
                    }
                }
            }
            *rng = 1;
            return u;
        }
    }
    if x.class != 2 || yr(&x.text, 0) < 0 {
        return -1;
    }
    let mut u = x.consumed;
    let (q, o) = at_(u);
    let (y1, r1) = tk(fe, t, q, o, 3);
    if r1 == 0 {
        return -1;
    }
    let joiner = if and_or(&y1.text) {
        true
    } else if y1.text[0] == b'&' {
        if y1.len != 1 {
            return -1;
        }
        true
    } else {
        false
    };
    if joiner && y1.ws < 2 {
        u += r1;
        let (q, o) = at_(u);
        let (y2, r2) = tk(fe, t, q, o, 3);
        if r2 == 0 || yr(&y2.text, 0) < 0 || y2.ws > 1 {
            return -1;
        }
        *yr_flag = 1;
        return u + r2;
    }
    if is_dash(y1.text[0]) && y1.len == 1 && y1.ws < 2 {
        u += r1;
        let (q, o) = at_(u);
        let (y2, r2) = tk(fe, t, q, o, 3);
        if r2 != 0 && yr(&y2.text, 0) >= 0 && y2.ws < 2 && y2.ws == y1.ws && atoi(&x.text) < atoi(&y2.text) {
            u += r2;
            let (q, o) = at_(u);
            let (z, _) = tk(fe, t, q, o, 3);
            if z.class != 3 || z.len != 1 || z.text[0] != y1.text[0] {
                *yr_flag = 1;
                *rng = 1;
                return u;
            }
        }
    }
    -1
}

/// `FUN_10038e50`: 2/3 when the word before is "in"/"on", 1 when the two
/// words before or after form a phrase in the era table, else 0. The first
/// token after the date is returned through `dc`.
#[allow(clippy::too_many_arguments)]
fn context(fe: &Fe, t: &[u8], p0: usize, off0: i32, mode: u8, mut u: i32, a: &Tok, b: &Tok, dc: &mut Tok) -> u8 {
    let mut lc: Option<Vec<u8>> = None;
    let mut l8: Option<Vec<u8>> = None;
    if (mode == b'L' || mode == b'B') && b.len > 0 {
        lc = Some(b.text().to_vec());
        if a.len > 0 {
            l8 = Some(a.text().to_vec());
        }
    }
    let mut after: Vec<Option<Vec<u8>>> = vec![None, None, None];
    if mode == b'R' || mode == b'B' {
        let mut i = 0;
        loop {
            let (x, r) = tk(fe, t, p0 + u as usize, off0 + u, 3);
            if r == 0 {
                break;
            }
            if i == 0 {
                *dc = x.clone();
            }
            if x.class == 8 || x.class == 9 {
                break;
            }
            after[i] = Some(x.text().to_vec());
            u += r;
            i += 1;
            if i > 2 {
                break;
            }
        }
    }
    if let Some(w) = &lc {
        if stricmp(w, s(0x10077688)) == 0 || stricmp(w, s(0x100776b0)) == 0 {
            return (upper(w[0]) != 0x49) as u8 + 2;
        }
    }
    let tab = ptab(0x1008a39c, 0x1009b038);
    if phrase_find(tab, &[lc, l8], true) >= 0 || phrase_find(tab, &after[..2], true) >= 0 {
        return 1;
    }
    0
}

fn punct_next(dc: &Tok) -> bool {
    matches!(dc.class, 8 | 3 | 7)
        && ((dc.text[0] != 0 && strchr(s(0x100802c0), dc.text[0])) || strcmp(s(0x100802bc), &dc.text) == 0 || (!era().is_2009() && strcmp(s(0x100802b8), &dc.text) == 0))
}

/// The token after the date must end it: glued punctuation, or a word,
/// number or symbol after whitespace.
fn next_ok(dc: &Tok) -> bool {
    if dc.len > 0 && !(punct_next(dc) && dc.ws == 0) {
        if !matches!(dc.class, 1 | 2 | 3) || dc.ws < 1 {
            return false;
        }
    }
    true
}

/// `FUN_10039000`: order of an all-numeric date: 1 M/D/Y, 2 D/M/Y, 3 Y/M/D, 0 none.
fn order(sc: &Sc, p2: &[u8], hint: i16, nn: i32) -> i16 {
    let (s0, s1, s2) = (&sc.sl[0].text[..], &sc.sl[1].text[..], &sc.sl[2].text[..]);
    if nn == 2 {
        if (p2[0] == b'M' && day(s1) >= 0 && yr(s2, 0) >= 0) || (mon(s0) >= 0 && p2[1] == b'D' && yr(s2, 0) >= 0) || (mon(s0) >= 0 && day(s1) >= 0 && p2[2] == b'Y') {
            return 1;
        }
        if (p2[0] == b'D' && mon(s1) >= 0 && yr(s2, 0) >= 0) || (day(s0) >= 0 && p2[1] == b'M' && yr(s2, 0) >= 0) || (day(s0) >= 0 && mon(s1) >= 0 && p2[2] == b'Y') {
            return 2;
        }
        if (p2[0] == b'Y' && mon(s1) >= 0 && day(s2) >= 0) || (yr(s0, 0) >= 0 && p2[1] == b'M' && day(s2) >= 0) {
            return 3;
        }
        if yr(s0, 0) >= 0 && mon(s1) >= 0 && p2[2] == b'D' {
            return 3;
        }
    } else if nn == 1 {
        if (p2[0] == b'M' && ((day(s1) >= 0 && p2[2] == b'Y') || (p2[1] == b'D' && yr(s2, 0) >= 0))) || (mon(s0) >= 0 && p2[1] == b'D' && p2[2] == b'Y') {
            return 1;
        }
        if (p2[0] == b'D' && ((mon(s1) >= 0 && p2[2] == b'Y') || (p2[1] == b'M' && yr(s2, 0) >= 0))) || (day(s0) >= 0 && p2[1] == b'M' && p2[2] == b'Y') {
            return 2;
        }
        if p2[0] == b'Y' && ((mon(s1) >= 0 && p2[2] == b'D') || (p2[1] == b'M' && day(s2) >= 0)) {
            return 3;
        }
        if yr(s0, 0) >= 0 && p2[1] == b'M' && p2[2] == b'D' {
            return 3;
        }
    }
    if hint != 0 && nn == 3 {
        if mon(s0) >= 0 && day(s1) >= 0 && yr(s2, 0) >= 0 {
            return 1;
        }
        if day(s0) >= 0 && mon(s1) >= 0 && yr(s2, 0) >= 0 {
            return 2;
        }
        if yr(s0, 0) >= 0 && mon(s1) >= 0 && day(s2) >= 0 {
            return 3;
        }
    }
    0
}

/// `FUN_10037e90`: validate pattern `idx` against the slots and fix the D/M/Y
/// roles of numeric slots. `p2` is the slot class string (updated in place).
#[allow(clippy::too_many_arguments)]
fn validate(sc: &mut Sc, p2: &mut [u8], idx: usize, hint: i16, a: &Tok, b: &Tok, dc: &Tok) -> bool {
    let mut nn = 0;
    for i in 0..sc.n as usize {
        if p2[i] == b'N' {
            if sc.sl[i].tag == b'N' {
                nn += 1;
            } else {
                p2[i] = sc.sl[i].tag;
            }
        }
    }
    // julie09 FUN_100351d0 has no next-token pre-check.
    if !era().is_2009() && !next_ok(dc) {
        return false;
    }
    let txt = |sc: &Sc, i: usize| sc.sl[i].text;
    macro_rules! tag {
        ($i:expr) => {
            sc.sl[$i].tag
        };
    }
    macro_rules! set {
        ($($i:expr => $v:expr),*) => {{
            $( sc.sl[$i].tag = $v; )*
            return true;
        }};
    }
    match idx {
        0 => {
            if nn == 2 {
                if day(&txt(sc, 2)) >= 0 {
                    if yr(&txt(sc, 5), 0) < 0 {
                        return false;
                    }
                    set!(2 => b'D', 5 => b'Y');
                }
            } else if nn == 1 {
                if (p2[2] == b'D' && yr(&txt(sc, 5), 0) >= 0) || (p2[5] == b'Y' && day(&txt(sc, 2)) >= 0) {
                    set!(2 => b'D', 5 => b'Y');
                }
            } else if nn < 1 && tag!(2) == b'D' && tag!(5) == b'Y' {
                return true;
            }
            false
        }
        1 | 5 => {
            if nn == 1 {
                if day(&txt(sc, 2)) < 0 {
                    return false;
                }
                set!(2 => b'D');
            }
            if nn > 0 {
                return false;
            }
            tag!(2) == b'D'
        }
        2 | 4 => {
            let d = if idx == 2 { 1 } else { 2 };
            if nn == 2 {
                if day(&txt(sc, d)) < 0 || yr(&txt(sc, 3), 0) < 0 {
                    return false;
                }
                set!(d => b'D', 3 => b'Y');
            }
            if nn == 1 {
                if p2[d] != b'D' || yr(&txt(sc, 3), 0) < 0 {
                    if p2[3] != b'Y' || day(&txt(sc, d)) < 0 {
                        return false;
                    }
                }
                set!(d => b'D', 3 => b'Y');
            }
            if nn > 0 {
                return false;
            }
            tag!(d) == b'D' && tag!(3) == b'Y'
        }
        3 | 6 | 8 | 9 => {
            if nn == 1 {
                if day(&txt(sc, 1)) < 0 {
                    return false;
                }
                set!(1 => b'D');
            }
            if nn > 0 {
                return false;
            }
            tag!(1) == b'D'
        }
        7 => {
            if tag!(0) != b'w' {
                return true;
            }
            // 2009 FUN_10034ff0 / FUN_100351d0 case 7: the context hint is only 0 or 1 there
            sc.sl[0].len > 2 && (hint == if era().is_2009() { 2 } else { 3 } || sc.sl[0].flag != 0)
        }
        10 => {
            if nn == 2 {
                if day(&txt(sc, 1)) < 0 || yr(&txt(sc, 2), 0) < 0 {
                    return false;
                }
                set!(1 => b'D', 2 => b'Y');
            }
            if nn == 1 {
                if p2[1] != b'D' || yr(&txt(sc, 2), 0) < 0 {
                    if p2[2] != b'Y' || day(&txt(sc, 1)) < 0 {
                        return false;
                    }
                }
                set!(1 => b'D', 2 => b'Y');
            }
            if nn > 0 {
                return false;
            }
            tag!(1) == b'D' && tag!(2) == b'Y'
        }
        11 => {
            if nn == 1 {
                if day(&txt(sc, 1)) >= 0 {
                    set!(1 => b'D');
                }
                if yr(&txt(sc, 1), 0) >= 0 {
                    set!(1 => b'Y');
                }
                return false;
            }
            if nn > 0 {
                return false;
            }
            tag!(1) == b'D' || tag!(1) == b'Y'
        }
        12 => {
            if tag!(0) == b'A' {
                if era().is_2009() {
                    return hint != 0 || sc.sl[0].flag != 0;
                }
                if sc.sl[0].len < 3 {
                    return hint != 0 && sc.sl[0].flag != 0;
                }
                return hint != 0 || sc.sl[0].flag != 0;
            }
            false
        }
        13 => {
            if nn == 0 {
                if tag!(0) == tag!(1) && tag!(1) == tag!(2) {
                    return false;
                }
            } else if nn == 1 && (tag!(0) == tag!(1) || tag!(1) == tag!(2) || tag!(2) == tag!(0)) {
                return false;
            }
            if sc.sl[1].ws != sc.sl[2].ws {
                return false;
            }
            if sc.sl[0].ws == 0 && b.len > 0 && b.text[0] != 0 && strchr(s(0x100802a8), b.text[0]) {
                return false;
            }
            if dc.len > 0 && !punct_next(dc) && dc.ws == 0 {
                return false;
            }
            let mut r = order(sc, p2, hint, nn);
            if r == 0 {
                r = order(sc, p2, 1, nn);
                if r == 0 {
                    return false;
                }
                if b.len != 0 && sc.sl[0].ws < 1 {
                    // julie09 compares against the `a2 fe` marker only.
                    if b.class != 3 || strcmp(s(0x100802bc), &b.text) != 0 || (!era().is_2009() && strcmp(s(0x100802b8), &b.text) != 0) || sc.sl[0].ws != 0 {
                        return false;
                    }
                }
                if sc.sl[0].len < 2 && (era() != crate::era::Era::Paul09 || sc.sl[1].len < 2) && sc.sl[2].len < 2 {
                    return false;
                }
                if sc.sl[1].ws != 0 || sc.sl[2].ws != 0 {
                    return false;
                }
                let seps = if era().is_2009() { 0x100802a8 } else { 0x1008028c };
                let dash_slash = |v: i16| v != 0 && strchr(s(seps), v as u8);
                if !dash_slash(sc.sl[0].sep) || !dash_slash(sc.sl[1].sep) {
                    return false;
                }
                if dc.len != 0 && !next_ok(dc) {
                    return false;
                }
                if dc.class == 1 && cls(dc.text[0]) & 0x80 != 0 {
                    return false;
                }
                if era() != crate::era::Era::Paul09 && sc.sl[0].ws < 2 && a.len > 0 && stricmp(&a.text, s(0x100802b4)) == 0 && b.ws == 0 && b.len < 3 && b.text[0] == b'.' {
                    return false;
                }
            }
            match r {
                1 => set!(0 => b'M', 1 => b'D', 2 => b'Y'),
                2 => set!(0 => b'D', 1 => b'M', 2 => b'Y'),
                3 => set!(0 => b'Y', 1 => b'M', 2 => b'D'),
                _ => true,
            }
        }
        14 => {
            let (s0, s2) = (txt(sc, 0), txt(sc, 2));
            if nn == 2 {
                if day(&s0) >= 0 && yr(&s2, 0) >= 0 {
                    set!(0 => b'D', 2 => b'Y');
                }
                if yr(&s0, 0) >= 0 {
                    if day(&s2) < 0 {
                        return false;
                    }
                    set!(0 => b'Y', 2 => b'D');
                }
            } else if nn == 1 {
                if (p2[0] == b'D' && yr(&s2, 0) >= 0) || (p2[2] == b'Y' && day(&s0) >= 0) {
                    set!(0 => b'D', 2 => b'Y');
                }
                if (p2[0] == b'Y' && day(&s2) >= 0) || (p2[2] == b'D' && yr(&s0, 0) >= 0) {
                    set!(0 => b'Y', 2 => b'D');
                }
            } else if nn < 1 && tag!(0) != tag!(2) {
                return true;
            }
            false
        }
        15 => {
            if nn == 1 {
                if day(&txt(sc, 0)) < 0 {
                    return false;
                }
                set!(0 => b'D');
            }
            if nn > 0 {
                return false;
            }
            tag!(0) == b'D'
        }
        16 | 19 | 20 => {
            if nn == 1 {
                if yr(&txt(sc, 0), 0) < 0 {
                    return false;
                }
                set!(0 => b'Y');
            }
            if nn > 0 {
                return false;
            }
            tag!(0) == b'Y'
        }
        17 => {
            let (s0, s1) = (txt(sc, 0), txt(sc, 1));
            if nn == 2 {
                if hint != 0 && sc.sl[0].sep == b'/' as i16 {
                    if mon(&s0) >= 0 && day(&s1) >= 0 {
                        set!(0 => b'M', 1 => b'D');
                    }
                    if day(&s0) >= 0 && mon(&s1) >= 0 {
                        set!(0 => b'D', 1 => b'M');
                    }
                    if yr(&s0, 0) >= 0 && mon(&s1) >= 0 {
                        set!(0 => b'Y', 1 => b'M');
                    }
                    if mon(&s0) >= 0 && yr(&s1, 0) >= 0 {
                        set!(0 => b'M', 1 => b'Y');
                    }
                }
                return false;
            }
            if nn == 1 {
                if p2[0] == b'M' && day(&s1) >= 0 {
                    set!(0 => b'M', 1 => b'D');
                }
                if mon(&s0) >= 0 && p2[1] == b'D' && !is_days(&s1) {
                    set!(0 => b'M', 1 => b'D');
                }
                if (era().is_2009() && p2[0] == b'D' && !is_days(&s0) && mon(&s1) >= 0) || (day(&s0) >= 0 && p2[1] == b'M') {
                    set!(0 => b'D', 1 => b'M');
                }
                if (p2[0] == b'Y' && !is_days(&s0) && mon(&s1) >= 0) || (yr(&s0, 0) >= 0 && p2[1] == b'M') {
                    if hint == 0 && (p2[0] != b'Y' || sc.sl[0].len == 4) && all_digit(&s0) && all_digit(&s1) {
                        return false;
                    }
                    set!(0 => b'Y', 1 => b'M');
                }
                if p2[0] != b'M' || yr(&s1, 0) < 0 {
                    if mon(&s0) < 0 || p2[1] != b'Y' || is_days(&s1) {
                        return false;
                    }
                }
                if hint == 0
                    && (p2[1] != b'Y' || sc.sl[1].len == 4 || (sc.sl[0].sep < 1 && sc.sl[1].ws < 1))
                    && all_digit(&s0)
                    && all_digit(&s1)
                {
                    return false;
                }
                set!(0 => b'M', 1 => b'Y');
            }
            nn < 1 && tag!(0) != tag!(1)
        }
        18 if era().is_2009() => {
            // julie09 FUN_100351d0 case 0x12
            if nn == 1 {
                let s0 = txt(sc, 0);
                if day(&s0) < 0 {
                    if yr(&s0, 0) < 0 {
                        return false;
                    }
                    sc.sl[0].tag = b'Y';
                } else {
                    sc.sl[0].tag = b'D';
                }
                return strcmp(&sc.sl[1].text, s(0x100790c8)) != 0 || hint != 0 || sc.sl[1].flag != 0;
            }
            if nn > 0 {
                return false;
            }
            tag!(0) == b'D' || tag!(0) == b'Y'
        }
        18 => {
            if nn == 1 {
                let s0 = txt(sc, 0);
                if day(&s0) < 0 {
                    if yr(&s0, 0) < 0 {
                        return false;
                    }
                    sc.sl[0].tag = b'Y';
                } else {
                    sc.sl[0].tag = b'D';
                }
            } else {
                if nn > 0 {
                    return false;
                }
                if tag!(0) == b'D' {
                    return true;
                }
                if tag!(0) != b'Y' {
                    return false;
                }
            }
            if strcmp(&sc.sl[1].text, s(0x100790c8)) == 0 && hint == 0 && sc.sl[1].flag == 0 {
                return false;
            }
            if tag!(1) != b'A' {
                return true;
            }
            if hint != 2 && hint != 3 {
                if hint != 0 {
                    return true;
                }
                return sc.sl[1].flag != 0;
            }
            tag!(0) != b'Y'
        }
        21 => hint != 0 && p2[0] == b'Y',
        22 | 23 => {
            if nn == 1 {
                if yr(&txt(sc, 1), 0) >= 0 {
                    set!(1 => b'Y');
                }
                return false;
            }
            if nn > 0 {
                return false;
            }
            tag!(1) == b'Y'
        }
        24 | 25 => tag!(1) == b'Y',
        _ => false,
    }
}

fn mark_august(wa: &mut Wa, w: &[u8]) {
    if stricmp(w, s(0x100802ac)) == 0 {
        let r = Wa::rec(wa.n() as i32 - 1);
        wa.set_u8(r + 0x30, 0x14);
        wa.set_u8(r + 0x31, 0xff);
    }
}

/// `FUN_10037cc0`: a DAYS slot (list or range) read token by token.
fn read_days(fe: &Fe, wa: &mut Wa, t: &[u8], sl: &Slot) -> bool {
    let len = sl.end - sl.start;
    let mut used = 0;
    let mut o = sl.start;
    let mut q = sl.tp;
    let mut buf = new_buf();
    while used < len {
        let (x, r) = tk(fe, t, q, o, 3);
        let ok = match x.class {
            2 => {
                if sl.tag == b'D' {
                    ordinal_numeral_words(&mut buf, &x.text);
                } else {
                    year(&mut buf, &x.text);
                }
                let ok = wa.emit_split(&buf, x.start, x.end, 0x44, 0x44, 4);
                buf[0] = 0;
                ok
            }
            1 => wa.emit(x.text(), x.start, x.end, 0x41, 0x44, 4),
            3 => match x.text[0] {
                b'-' | b'~' => wa.emit(s(0x10077380), x.start, x.end, 0x41, 0x44, 4),
                b'/' | b'&' => wa.emit(s(0x10077400), x.start, x.end, 0x41, 0x44, 4),
                _ => true,
            },
            6 if x.text[0] == 0x96 || x.text[0] == 0x97 => wa.emit(s(0x10077380), x.start, x.end, 0x41, 0x44, 4),
            _ => true,
        };
        if !ok {
            return false;
        }
        if r == 0 {
            break;
        }
        used += r;
        o += r;
        q += r as usize;
    }
    true
}

/// `FUN_100376d0`: emit the matched date.
fn emit_date(fe: &Fe, wa: &mut Wa, t: &[u8], sc: &mut Sc, idx: usize) -> bool {
    let n = sc.n as usize;
    let d = |sc: &Sc, i: usize| is_days(&sc.sl[i].text);
    let reorder = match idx {
        0xd => !d(sc, 0) && !d(sc, 1) && !d(sc, 2),
        0xe => !d(sc, 0) && !d(sc, 2),
        0x11 => !d(sc, 0) && !d(sc, 1) && sc.sl[0].tag == b'D' && sc.sl[1].tag == b'M',
        0x12 => !d(sc, 0) && sc.sl[0].tag == b'D',
        _ => false,
    };
    let mut dayb = new_buf();
    let mut yrb = new_buf();
    let mut monb = [0u8; 64];
    if reorder {
        for i in 0..n {
            let c = sc.sl[i].tag;
            if c == b'D' {
                ordinal_numeral_words(&mut dayb, &sc.sl[i].text);
            } else if c == b'M' || c == b'A' || c == b'F' {
                sc.sl[i].tag = b'M';
                if sc.sl[i].class == 1 {
                    strcpy(&mut monb, &sc.sl[i].exp);
                } else {
                    strcpy(&mut monb, month_name(atoi(&sc.sl[i].text)));
                }
            } else if c == b'Y' {
                year(&mut yrb, &sc.sl[i].text);
            }
        }
        let st = sc.sl[0].start;
        let en = sc.sl[n - 1].end;
        if monb[0] != 0 {
            if !wa.emit_split(&monb, st, en, 0x44, 0x44, 4) {
                return false;
            }
            mark_august(wa, &monb);
        }
        if dayb[0] != 0 && !wa.emit_split(&dayb, st, en, 0x44, 0x44, 4) {
            return false;
        }
        if yrb[0] != 0 && !wa.emit_split(&yrb, st, en, 0x44, 0x44, 4) {
            return false;
        }
        return true;
    }
    let mut i = 0;
    while i < n {
        let sl = sc.sl[i].clone();
        let c = sl.tag;
        let ok = if is_days(&sl.text) && (c == b'D' || c == b'Y') {
            read_days(fe, wa, t, &sl)
        } else if c == b'D' {
            ordinal_numeral_words(&mut dayb, &sl.text);
            wa.emit_split(&dayb, sl.start, sl.end, 0x44, 0x44, 4)
        } else if c == b'M' || c == b'A' || c == b'F' {
            let (w, a) = if sl.class == 1 { (cs(&sl.exp), 0x41) } else { (month_name(atoi(&sl.text)), 0x44) };
            let mut mb = [0u8; 64];
            strcpy(&mut mb, w);
            if !wa.emit(&mb, sl.start, sl.end, a, 0x44, 4) {
                return false;
            }
            mark_august(wa, &mb);
            true
        } else if c == b'Y' {
            year(&mut yrb, &sl.text);
            if !matches!(idx, 0x10 | 0x14 | 0x16 | 0x18) {
                wa.emit_split(&yrb, sl.start, sl.end, 0x44, 0x44, 4)
            } else {
                strcat(&mut yrb, s(0x100773a0));
                let en = sc.sl[i + 1].end;
                i += 1;
                wa.emit_split(&yrb, sl.start, en, 0x44, 0x44, 4)
            }
        } else if c == b'w' || c == b'W' || c == b'S' {
            wa.emit(cs(&sl.exp), sl.start, sl.end, 0x41, 0x44, 4)
        } else if c == b'I' || c == b'O' || c == b'T' {
            wa.emit(cs(&sl.text), sl.start, sl.end, 0x41, 0x44, 4)
        } else {
            true
        };
        if !ok {
            return false;
        }
        i += 1;
    }
    true
}

/// Pattern-index range per first slot class and slot count, as in the
/// switch at the end of `FUN_10036940`. `Err(())` rejects the date.
fn pat_range(first: u8, k: usize) -> Result<Option<(usize, usize)>, ()> {
    let r = match (first, k) {
        (b'W', 6) => (0, 1),
        (b'W', 5) => (1, 2),
        (b'W', 4) => (2, 6),
        (b'W', 3) => (6, 7),
        (b'W', 1) => (7, 8),
        (b'T', 4) => (8, 9),
        (b'T', 3) => (9, 10),
        (b'T', _) => return if k < 3 { Err(()) } else { Ok(None) },
        (b'M', 3) => (10, 11),
        (b'M', 2) => (11, 12),
        (b'M', 1) => (12, 13),
        (b'N', 3) => (13, 17),
        (b'N', 2) => (17, 21),
        (b'N', 1) => (21, 22),
        (b'S', 3) => (22, 23),
        (b'S', 2) => (23, 24),
        (b'I', 3) => (24, 25),
        (b'I', 2) => (25, rd_i32(0x10080218) as usize),
        (b'S', _) | (b'I', _) => return if k < 2 { Err(()) } else { Ok(None) },
        _ => return Ok(None),
    };
    Ok(Some(r))
}

pub fn date(fe: &Fe, wa: &mut Wa, _tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let mut sc = Sc { n: 0, sl: vec![Slot::default(); 20] };
    let (a, b) = prev_two(t, p, off, 3);
    let mut pat = [0u8; 40];
    let mut cl = [0u8; 8];
    let mut exp = [0u8; 256];
    let mut pi: usize = 0;
    let mut k: usize = 0;
    let mut sep_at: i32 = -1;
    let mut seps = 0;
    let mut f28: i16 = 0;
    let mut f24: i16 = 0;
    let mut q: i32 = 0;

    'scan: loop {
        let (mut x, c) = tk(fe, t, p + q as usize, off + q, 3);
        if c == 0 || x.class == 9 || (x.nl == 0 && x.ws > 2) {
            break;
        }
        sc.n = k as i32;
        let t0 = x.text[0];
        if sep_at == -1 && x.len == 1 && t0 != 0 && strchr(s(0x100802a8), t0) {
            q += c;
            let (dc, _) = tk(fe, t, p + q as usize, off + q, 3);
            let x_ok = dc.ws == 1 && (numish(&dc.text) || strcmp(&dc.text, s(0x1007736c)) == 0 || (dc.class == 1 && cls(dc.text[0]) & 0x40 != 0));
            let y_ok = dc.ws >= 0 && dc.ws < 2 && dc.len == 1 && (dc.text[0] == b',' || dc.text[0] == b'/');
            if t0 == b'.' && x.ws == 0 && pi > 0 && (pat[pi - 1] == b'A' || pat[pi - 1] == b'w') && dc.len > 0 && (x_ok || y_ok) {
                pat[pi] = b'.';
                pi += 1;
                sc.sl[k - 1].flag = 1;
            } else {
                if pi == 0 || seps > 2 {
                    break;
                }
                seps += 1;
                pat[pi] = t0;
                sep_at = pi as i32;
                pi += 1;
                sc.sl[k - 1].sep = t0 as i8 as i16;
            }
            if k > 5 {
                break;
            }
            continue;
        }
        let comma = sep_at == -1 && t0 == b',' && x.len == 1 && x.ws < 2 && pi > 0;
        if comma && k > 0 && cl[0] == b'W' && matches!(pat[pi - 1], b'.' | b'W' | b'w') {
            q += c;
            pat[pi] = b',';
            pi += 1;
            sc.sl[k - 1].comma = 5;
            if k > 5 {
                break;
            }
            continue;
        }
        if comma && k > 1 && cl[0] == b'M' && matches!(pat[pi - 1], b'D' | b'N') {
            q += c;
            pat[pi] = b',';
            pi += 1;
            sc.sl[k - 1].comma = 5;
            if k > 5 {
                break;
            }
            continue;
        }

        macro_rules! sep_check {
            () => {
                if sep_at >= 0 {
                    if sep_at + 1 != pi as i32 {
                        break 'scan;
                    }
                    sep_at = -1;
                }
            };
        }
        let tag: u8;
        if stricmp(s(0x10077368), &x.text) == 0 {
            tag = b'O';
            cl[k] = b'O';
        } else if stricmp(s(0x1007736c), &x.text) == 0 {
            tag = b'T';
            cl[k] = b'T';
        } else if pi == 0 && stricmp(s(0x10077688), &x.text) == 0 {
            tag = b'I';
            cl[k] = b'I';
        } else if ((t0 == b's' && x.len == 1) || (x.len == 2 && era() != crate::era::Era::Paul09 && stricmp(s(0x100773a0), &x.text) == 0)) && x.ws == 0 && k > 0 && pat[pi - 1] != b'D' && cl[k - 1] == b'N' {
            tag = b's';
            cl[k] = b's';
        } else if stricmp(s(0x100802a4), &x.text) == 0 || stricmp(s(0x100802a0), &x.text) == 0 || stricmp(s(0x1008029c), &x.text) == 0 || stricmp(s(0x10080298), &x.text) == 0 {
            tag = b'S';
            cl[k] = b'S';
            strcpy(&mut exp, s(if upper(t0) == 0x42 { 0x100802a4 } else { 0x1008029c }));
        } else {
            let wd = rec30(0x1008a1a0, 0x1009b030);
            let i = wd.find(&x.text, true);
            if i >= 0 {
                tag = b'w';
                strcpy(&mut exp, wd.field(i, 0x10));
                cl[k] = b'W';
            } else {
                let full = ptab(0x1008a380, 0x1009b034);
                let j = full.find(&x.text, true);
                if j >= 0 {
                    tag = b'W';
                    strcpy(&mut exp, full.get(j));
                    cl[k] = b'W';
                } else {
                    let ma = rec30(0x10089c30, 0x1009b028);
                    let m = ma.find(&x.text, true);
                    if m >= 0 {
                        sep_check!();
                        tag = b'A';
                        cl[k] = b'M';
                        strcpy(&mut exp, month_name(atoi(ma.field(m, 0x10))));
                    } else if rec30(0x10089f60, 0x1009b02c).find(&x.text, true) >= 0 {
                        sep_check!();
                        cl[k] = b'M';
                        tag = b'F';
                        strcpy(&mut exp, &x.text);
                    } else {
                        let r = if f24 == 0 && x.class == 2 && numish(&x.text) {
                            let r1 = day_list(fe, &x, t, p + q as usize, off + q);
                            if r1 >= 0 {
                                r1
                            } else {
                                range(fe, &x, t, p + q as usize, off + q, &mut f28, &mut f24)
                            }
                        } else {
                            -1
                        };
                        if r >= 0 {
                            if f24 < 1 || k < 1 || cl[k - 1] != b'N' || (sc.sl[k - 1].sep != b'-' as i16 && sc.sl[k - 1].sep != b'/' as i16) {
                                let sl = &mut sc.sl[k];
                                sl.start = off + q;
                                sl.ws = x.ws;
                                sl.end = off + q + r;
                                sl.len = r - x.ws;
                                sl.class = 2;
                                strcpy(&mut sl.text, s(DAYS));
                                sl.tag = if f28 == 0 { b'D' } else { b'Y' };
                                pat[pi] = sl.tag;
                                sl.tp = p + q as usize;
                                cl[k] = b'N';
                                q += r;
                                k += 1;
                                pi += 1;
                                f28 = 0;
                            }
                            if k > 5 {
                                break;
                            }
                            continue;
                        }
                        if !numish(&x.text) {
                            break;
                        }
                        sep_check!();
                        let yv = year_value(&x.text);
                        if yv >= 0 {
                            tag = b'Y';
                            strcpy(&mut x.text, format!("{}", yv).as_bytes());
                        } else {
                            let dv = day_ord(&x.text);
                            if dv >= 0 {
                                tag = b'D';
                                strcpy(&mut x.text, format!("{}", dv).as_bytes());
                            } else {
                                tag = b'N';
                            }
                        }
                        cl[k] = b'N';
                    }
                }
            }
        }
        pat[pi] = tag;
        let sl = &mut sc.sl[k];
        sl.start = x.start;
        sl.ws = x.ws;
        sl.end = off + q + c;
        sl.len = x.len;
        sl.tag = tag;
        sl.class = x.class as u8;
        sl.tp = p + (q + c) as usize;
        q += c;
        if f24 > 0 {
            f24 = 0;
        }
        strcpy(&mut sl.text, &x.text);
        if exp[0] != 0 {
            strcpy(&mut sl.exp, &exp);
            exp[0] = 0;
        }
        k += 1;
        pi += 1;
        if k > 5 {
            break;
        }
    }

    cl[k] = 0;
    if b.len >= 1 && b.class == 3 && b.text[0] != 0 && strchr(s(0x1008028c), b.text[0]) && sc.sl[0].ws == 0 && b.text[0] as i8 as i16 == sc.sl[0].sep {
        return -1;
    }
    let first = cl[0];
    if !matches!(first, b'W' | b'T' | b'M' | b'N' | b'S' | b'I') {
        return -1;
    }
    let pats = ptabn(0x100801b0, 26);
    let mut hint: i16 = 0;
    let mut kk = k;
    while kk > 0 {
        cl[kk] = 0;
        let (lo, hi) = match pat_range(first, kk) {
            Err(()) => return -1,
            Ok(None) => {
                kk -= 1;
                continue;
            }
            Ok(Some(r)) => r,
        };
        let ls = kk - 1;
        let sp = sc.sl[ls].sep;
        if (sp == b'/' as i16 || sp == b'-' as i16) && !matches!(at(t, sc.sl[ls].tp as isize), b' ' | b'\t' | b'\n' | b'\r') {
            return -1;
        }
        for idx in lo..hi {
            if strcmp(pats.get(idx as i32), &cl) != 0 {
                continue;
            }
            let u = sc.sl[ls].end - off;
            let mut dc = Tok::default();
            if era().is_2009() {
                // 2009 FUN_10033b70 / FUN_10033d10: no hint for 0x12, and a nonzero context only sets it to 1
                if matches!(idx, 7 | 0xc | 0xd | 0x11 | 0x15) && context(fe, t, p, off, b'B', u, &a, &b, &mut dc) != 0 {
                    hint = 1;
                }
            } else if matches!(idx, 7 | 0xc | 0xd | 0x11 | 0x12 | 0x15) {
                hint = context(fe, t, p, off, b'B', u, &a, &b, &mut dc) as i16;
            }
            sc.n = kk as i32;
            if dc.len == 0 && !era().is_2009() {
                tokenize(&mut dc, t, sc.sl[ls].tp, 0, 3, fe.ud.as_ref());
            }
            let vr = validate(&mut sc, &mut cl, idx, hint, &a, &b, &dc);
            if vr {
                return if emit_date(fe, wa, t, &mut sc, idx) { u } else { 0 };
            }
            break;
        }
        kk -= 1;
    }
    -1
}
