//! `FUN_1005c890` (dispatch order 4): URLs and e-mail addresses, with its
//! piece tokenizer `FUN_1005d9d0`, protocol reader `FUN_1005d600`, port
//! reader `FUN_1005d790`, validator `FUN_1005d530`/`FUN_1005d930`, emitter
//! `FUN_1005d290`, path tail `FUN_1005d3c0` and the tag-mode continuation
//! `FUN_1005dd20` (`tagmode_url`). class 0x10.
//!
//! An optional protocol ("HTTP" + "://") comes first. Then up to 9 slots,
//! each an alphanumeric piece (`-`/`_` may join) or one of `: @ . /`:
//! `D` digits (at most 4 groups), `W` "www", `N` a country code that ends
//! the address, `U` a generic TLD ("com", "gov", ...) or second-level label
//! ("co", "ac", ...), `H` any other host word, plus the separators. A `.`
//! only goes at an odd slot and after a D/A/u piece; `@` only after a
//! single host. The tags are matched longest first against `D.D.D.D`,
//! `W.H.H.N`, ..., `H.U`. When the word right before was a plain word that
//! ended just where this starts ("weather" then ".gov"), it becomes slot 0.
//!
//! Every piece is read by `norm::url_words` over the whole address's span;
//! `www`/`N`/`u` pieces are spelled when short. A `/` after the address
//! switches the WA into tag mode (`+0xc`), where `tagmode_url` keeps reading
//! the path piece by piece until whitespace.

use super::cstr::*;
use super::norm::url_words;
use super::numwords::{new_buf, number};
use super::tabs::{ptab, search, Ptab};
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::window::{back_words, last_two};
use super::Fe;

const PROTO: Ptab = ptab(0x100816e8, 0x1009b070);
const CC_TLD: Ptab = ptab(0x10081764, 0x1009b07c);
const TLD: Ptab = ptab(0x1008172c, 0x1009b074);
const SLD: Ptab = ptab(0x10081714, 0x1009b078);
const PATTERNS: Ptab = ptab(0x1009c480, 0x1009c4ac);

fn find(tab: Ptab, w: &[u8]) -> i32 {
    search(tab.count(), true, w, |i| tab.get(i))
}

fn all_digits(w: &[u8]) -> bool {
    let w = cs(w);
    !w.is_empty() && w.iter().all(|&c| cls(c) & 0x10 != 0)
}

fn is_end(c: u8) -> bool {
    matches!(c, 0 | b' ' | b'\t' | b'\n' | b'\r')
}

#[derive(Clone, Default)]
struct Slot {
    start: i32,
    end: i32,
    len: i32,
    b11: u8,
    mark: u8,
    text: Vec<u8>,
    exp: Vec<u8>,
}

impl Slot {
    fn of(x: &Tok, end: i32, mark: u8) -> Slot {
        Slot { start: x.start, end, len: x.len, b11: 0, mark, text: x.text().to_vec(), exp: Vec::new() }
    }
}

/// `FUN_1005d9d0`: the next URL piece. Whitespace is only skipped before
/// the first slot. `pub(super)`: also used by `norm_vtml`'s `Q` reader and
/// `initial`'s `sapi:email`/`sapi:web` converters, the tag's other readers
/// of this tokenizer.
pub(super) fn piece(t: &[u8], p: usize, off: i32, c: usize) -> (Tok, i32) {
    let mut x = Tok::default();
    let mut q = p;
    let mut used = 0i32;
    let pair_end = |q: usize| at(t, q as isize) == 0xa2 && at(t, q as isize + 1) == 0xfe;
    if c != 0 {
        if is_end(at(t, q as isize)) || pair_end(q) {
            return (x, 0);
        }
    } else {
        let mut nl = 0;
        loop {
            let ch = at(t, q as isize);
            match ch {
                b' ' | b'\t' | b'\r' => {}
                b'\n' => nl += 1,
                _ => {
                    if nl < 2 && ch != 0 {
                        break;
                    }
                    x.start = off + used;
                    x.class = if nl < 2 { 9 } else { 8 };
                    x.consumed = used;
                    x.end = off + used;
                    return (x, used);
                }
            }
            q += 1;
            used += 1;
            x.ws += 1;
        }
    }
    x.start = off + used;
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let ch = at(t, q as isize);
        let alnum = cls(ch) & 0xd0 != 0;
        let joiner = ch != 0 && strchr(s(0x1009c510), ch);
        if !alnum {
            if buf.is_empty() {
                buf.push(ch);
                used += 1;
                x.class = 3;
                break;
            }
            if !joiner {
                x.class = if all_digits(&buf) { 2 } else { 1 };
                break;
            }
        }
        if buf.len() + 1 > 0x3f || (joiner && at(t, q as isize + 1) != 0 && cls(at(t, q as isize + 1)) & 0xd0 == 0) {
            x.class = if all_digits(&buf) { 2 } else { 1 };
            break;
        }
        buf.push(ch);
        q += 1;
        used += 1;
    }
    let l = strlen(&buf).min(35);
    x.text[..l].copy_from_slice(&buf[..l]);
    x.len = l as i32;
    x.consumed = used;
    x.end = x.start - x.ws + used;
    (x, used)
}

/// `FUN_1005d600`: "HTTP://" and friends; returns the bytes taken.
fn protocol(fe: &Fe, pr: &mut [Slot; 2], tok: &Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let i = find(PROTO, tok.text());
    if i < 0 {
        return 0;
    }
    let mut x = Slot::of(tok, tok.end, b'P');
    if matches!(i, 1 | 3 | 6 | 10) {
        x.b11 = 0x53;
    }
    x.exp = PROTO.get(i).to_vec();
    let mut y = Tok::default();
    let r = tokenize(&mut y, t, p + tok.consumed as usize, off + tok.consumed, 9, fe.ud.as_ref());
    if strcmp(s(0x1008089c), y.text()) == 0 && y.len == 3 && y.ws == 0 {
        pr[0] = x;
        pr[1] = Slot { start: y.start, end: y.end, len: 3, b11: 0, mark: b'P', text: y.text().to_vec(), exp: Vec::new() };
        return r + tok.consumed;
    }
    0
}

/// `FUN_1005d790`: ":8080"; returns the bytes taken.
fn port(fe: &Fe, pt: &mut [Slot; 2], t: &[u8], p: usize, off: i32) -> i32 {
    let mut x = Tok::default();
    let r = tokenize(&mut x, t, p, off, 9, fe.ud.as_ref());
    if r == 0 || x.class != 3 || x.text[0] != b':' || x.len != 1 || x.ws != 0 {
        return 0;
    }
    pt[0] = Slot { start: x.start, end: x.end, len: 1, b11: 0, mark: b'S', text: x.text().to_vec(), exp: Vec::new() };
    let mut y = Tok::default();
    let r2 = tokenize(&mut y, t, p + r as usize, off + r, 9, fe.ud.as_ref());
    if y.ws == 0 && y.class == 2 && all_digits(y.text()) {
        let mut b = new_buf();
        number(&mut b, y.text(), 0);
        pt[1] = Slot::of(&y, y.end, b'D');
        pt[1].exp = cs(&b).to_vec();
        return r + r2;
    }
    pt[0] = Slot::default();
    0
}

/// `FUN_1005d930`: a dotted quad, each part 0..255.
fn ip(w: &[u8]) -> bool {
    let mut parts = 0;
    let mut cur: Vec<u8> = Vec::new();
    let w = cs(w);
    for i in 0..=w.len() {
        let c = at(w, i as isize);
        if cls(c) & 0x10 != 0 {
            cur.push(c);
            continue;
        }
        if c != b'.' && i != w.len() {
            return false;
        }
        parts += 1;
        if cur.is_empty() {
            return false;
        }
        let v = atoi(&cur);
        if cur.len() > 3 || parts > 4 || !(0..=255).contains(&v) {
            return false;
        }
        cur.clear();
    }
    parts == 4
}

/// `FUN_1005d290`.
fn emit(fe: &Fe, wa: &mut Wa, sl: &[Slot], pt: &[Slot; 2]) -> bool {
    let st = if sl[0].mark == b'a' { sl[1].start } else { sl[0].start };
    let en = sl[sl.len() - 1].end;
    for x in sl {
        if x.mark == b'a' {
            continue;
        }
        let b = if matches!(x.mark, b'W' | b'u' | b'N') { 0x53 } else { 0x44 };
        let w = if x.exp.is_empty() { &x.text } else { &x.exp };
        if url_words(fe, wa, w, st, en, b) == 0 {
            return false;
        }
    }
    if pt[0].len > 0 {
        if !wa.emit(&pt[0].exp, pt[0].start, pt[0].end, 0x53, 0x44, 0x10) {
            return false;
        }
        if !wa.emit_split(&pt[1].exp, pt[1].start, pt[1].end, 0x44, 0x44, 0x10) {
            return false;
        }
    }
    true
}

/// `FUN_1005d3c0` (and, with `resume`, `FUN_1005dd20`'s loop): the path
/// after the address, piece by piece up to whitespace; a `/` ends it and
/// switches tag mode on. With `stop_early` only a following `/` is taken.
fn tail(fe: &Fe, wa: &mut Wa, t: &[u8], p: usize, off: i32, mut used: i32, stop_early: bool) -> i32 {
    let mut q = p;
    let mut o = off;
    loop {
        let c = at(t, q as isize);
        if c == 0 || c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' || used >= 0x823 {
            return used;
        }
        let mut x = Tok::default();
        let r = tokenize(&mut x, t, q, o, 9, fe.ud.as_ref());
        if r == 0 || x.ws >= 1 || x.class == 9 {
            return used;
        }
        if x.class == 3 && x.text[0] == b'/' {
            if url_words(fe, wa, x.text(), x.start, x.end, 0x44) == 0 {
                return 0;
            }
            wa.set_i16(0xc, 1);
            return used + r;
        }
        if stop_early {
            return used;
        }
        if x.class == 3 && (x.text[0] == 0 || !strchr(s(0x100808a0), x.text[0])) {
            return used;
        }
        if url_words(fe, wa, x.text(), x.start, x.end, 0x44) == 0 {
            return 0;
        }
        q += r as usize;
        o += r;
        used += r;
        if at(t, q as isize) == 0 {
            return used;
        }
    }
}

pub fn url(fe: &Fe, wa: &mut Wa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let mut pr: [Slot; 2] = Default::default();
    let mut pt: [Slot; 2] = Default::default();
    let mut sl: Vec<Slot> = Vec::new();
    let mut tags: Vec<u8> = Vec::new();
    let mut srcs: Vec<u8> = Vec::new();
    let pl = protocol(fe, &mut pr, tok, t, p, off);
    let (mut q, mut qo) = (p, off);
    if pl == 0 {
        let mut buf = [0u8; 96];
        let bw = back_words(&mut buf, t, p as isize - off as isize, p as isize, 2);
        if bw as i16 != 0 && tok.ws == 0 {
            let (a, b) = last_two(&buf, off - bw, 9).unwrap_or_default();
            let n = wa.n() as i32;
            let prev_ok = |end: i32| n >= 1 && matches!(wa.ri32(n - 1, 0x28), 0x19 | 0x18) && wa.ri32(n - 1, 4) == end;
            if a.len >= 1 && b.len == 1 && b.ws == 0 && b.text[0] == b'.' && (a.class == 1 || a.class == 2) && prev_ok(a.end) {
                sl.push(Slot::of(&a, a.end, b'a'));
                sl.push(Slot { start: b.start, end: b.end, len: 1, b11: 0, mark: b'S', text: b.text().to_vec(), exp: Vec::new() });
                tags.extend_from_slice(&[b'H', b'.']);
                srcs.extend_from_slice(&[b'a', b'S']);
            } else if b.len >= 1 && (b.class == 1 || b.class == 2) && prev_ok(b.end) {
                sl.push(Slot::of(&b, b.end, b'a'));
                tags.push(b'H');
                srcs.push(b'a');
            }
        }
    } else {
        q += pl as usize;
        qo += pl;
    }
    let mut dcount = 0;
    loop {
        let c = sl.len();
        let (x, r) = piece(t, q, qo, c);
        if r == 0 || x.class > 7 {
            break;
        }
        let w = x.text().to_vec();
        let c0 = at(&w, 0);
        let (tag, src, exp): (u8, u8, Vec<u8>) = if c >= 1 && x.len == 1 && c0 != 0 && strchr(s(0x1009c508), c0) {
            if strchr(s(0x1009c504), c0) {
                break;
            }
            if c0 == b'@' {
                if c != 1 || tags[0] != b'H' {
                    break;
                }
            } else if c0 == b'.' {
                if c % 2 == 0 {
                    break;
                }
                if let Some(&ps) = srcs.last() {
                    if !matches!(ps, b'D' | b'A' | b'u') {
                        break;
                    }
                }
                if tags[c - 1] == b'N' && tail_mark(t, q) {
                    break;
                }
            }
            (c0, b'S', Vec::new())
        } else if dcount < 4 && all_digits(&w) {
            dcount += 1;
            (b'D', b'D', Vec::new())
        } else if x.len == 3 && stricmp(s(0x10081150), &w) == 0 {
            (b'W', b'A', Vec::new())
        } else if tail_mark(t, q + r as usize) && find(CC_TLD, &w) >= 0 {
            (b'N', b'A', CC_TLD.get(find(CC_TLD, &w)).to_vec())
        } else if find(TLD, &w) >= 0 {
            (b'U', b'A', TLD.get(find(TLD, &w)).to_vec())
        } else if find(SLD, &w) >= 0 {
            (b'U', b'u', SLD.get(find(SLD, &w)).to_vec())
        } else {
            if x.len < 2 || x.class != 1 {
                break;
            }
            (b'H', b'A', w.clone())
        };
        let mark = if matches!(src, b'S' | b'u') { src } else { tag };
        let mut sx = Slot::of(&x, qo + r, mark);
        sx.exp = exp;
        sl.push(sx);
        tags.push(tag);
        srcs.push(src);
        q += r as usize;
        qo += r;
        if sl.len() > 8 {
            break;
        }
    }

    if pl != 0 {
        if !wa.emit(&pr[0].exp, pr[0].start, pr[0].end, 0x41, pr[0].b11, 0x10) {
            return 0;
        }
        if url_words(fe, wa, &pr[1].text, pr[1].start, pr[1].end, pr[1].b11) == 0 {
            return 0;
        }
    }
    let mode = match (tags.first(), tags.get(1)) {
        (Some(b'D'), _) => 1,
        (Some(b'W'), _) => 2,
        (Some(b'H'), Some(b'@')) => 3,
        (Some(b'H'), Some(b'.')) => 4,
        _ => 0,
    };
    let mut n = sl.len();
    while n >= 1 {
        let (lo, hi, next) = match (mode, n) {
            (1, 7) => (0, 1, true),
            (1, m) => (0, 0, m > 7),
            (2, 7) => (1, 3, true),
            (2, 5) => (3, 4, true),
            (2, m) => (0, 0, m > 5),
            (3, 7) => (4, 6, true),
            (3, 5) => (6, 7, true),
            (3, m) => (0, 0, m > 5),
            (4, 7) => (7, 8, true),
            (4, 5) => (8, 10, true),
            (4, 3) => (10, PATTERNS.count(), true),
            (4, m) => (0, 0, m > 3),
            _ => (0, 0, true),
        };
        if !next {
            break;
        }
        for idx in lo..hi {
            if strcmp(PATTERNS.get(idx), &tags[..n]) != 0 {
                continue;
            }
            let mut consumed = sl[n - 1].end - off;
            consumed += port(fe, &mut pt, t, p + consumed as usize, off + consumed) as i16 as i32;
            let flag = pl != 0 || pt[0].len > 0;
            let ok = idx != 0 || (flag && sl[..n].iter().all(|x| x.len <= 3) && {
                let mut s2: Vec<u8> = Vec::new();
                for x in &sl[..n] {
                    s2.extend_from_slice(&x.text);
                }
                ip(&s2)
            });
            if ok {
                if !emit(fe, wa, &sl[..n], &pt) {
                    return 0;
                }
                return tail(fe, wa, t, p + consumed as usize, off + consumed, consumed, true);
            }
            break;
        }
        n -= 1;
    }
    if pl > 0 {
        return tail(fe, wa, t, p + pl as usize, off + pl, pl, false);
    }
    -1
}

/// `FUN_1005dca0`: the address ends at `t[q]` (end of text, whitespace, a
/// final `.`, or `/`/`:`).
fn tail_mark(t: &[u8], q: usize) -> bool {
    let c = at(t, q as isize);
    if is_end(c) || (c == 0xa2 && at(t, q as isize + 1) == 0xfe) {
        return true;
    }
    if c != b'.' {
        return strchr(s(0x1009c504), c);
    }
    let d = at(t, q as isize + 1);
    is_end(d) || (d == 0xa2 && at(t, q as isize + 2) == 0xfe)
}

/// `FUN_1005dd20`: tag mode after a URL's `/`.
pub fn tagmode_url(fe: &Fe, wa: &mut Wa, t: &[u8], p: usize, off: i32) -> i32 {
    if wa.i16(0xc) == 0 {
        return -1;
    }
    wa.set_i16(0xc, 0);
    let r = tail(fe, wa, t, p, off, 0, false);
    if r != 0 {
        r
    } else {
        -1
    }
}
