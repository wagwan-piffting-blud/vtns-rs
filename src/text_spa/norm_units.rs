//! `FUN_10031720` (chain slot 6): units. A leading amount (`FUN_10033050`:
//! a grouped number, a number range or list, or the previous word when an
//! earlier normalizer left a number or currency amount there) followed by
//! up to five unit tokens, each tagged by where it was found:
//! - 'E' `FUN_100338b0` group A or the unit-name table `FUN_10033990`,
//! - 'P' group B, 'C' group C (`abbrev_groups.rs`),
//! - '/' a slash and 'O' an "x"/"\xd7"/"\xb7"/"\xf7" operator (both "por"),
//! - 'W' a "cu"/"sq" prefix that lends its exponent to the next unit,
//! - 's' a "\xb2"/"\xb3" superscript on the previous unit,
//! - '.' a period after a unit or prefix.
//!
//! The tag string is matched against `PTR_DAT_10069370` from the longest
//! prefix down, each candidate checked by `FUN_10032660`, and the first that
//! passes is written out by `FUN_100321c0` (src 0xa).

use super::abbrev::{find_i, group_lookup, unit_lookup};
use super::abbrev_groups::Entry;
use super::emit::emit_split;
use super::norm_math::numeric;
use super::numwords::{cardinal_to_words, is_comma_grouped, is_period_grouped, plural};
use super::punct::{prev_words, sentence_end_code};
use super::tokenize::{tokenize, Tok};
use super::{emit, rec, FeSpa};
use crate::ling_spa::lexicon::{stricmp_spa, strnicmp_spa};
use crate::ling_spa::WaSpa;

const SRC: u32 = 0xa;

/// `DAT_10069224`.
const POR: &[u8] = b"por";
/// `DAT_10069090`.
const AL: &[u8] = b"al";
/// `DAT_10064530` / `DAT_100644d0`.
const RANGE_A: &[u8] = b"a";
const RANGE_Y: &[u8] = b"y";
/// `DAT_1006905c`: the leading amount is a range or list, rendered per token.
const NUMS: &[u8] = b"NUMS";

/// `0x100702a0` (`DAT_10079e2c` = 2): prefix, name, exponent.
const CU_SQ: [Entry; 2] = [(b"cu", b"c\xfabico", b"3", 0), (b"sq", b"cuadrado", b"2", 0)];

/// `DAT_10070248` (`DAT_10079e28` = 2, stride 0x2a): superscript, name.
const SUP: [(u8, &[u8]); 2] = [(0xb2, b"cuadrado"), (0xb3, b"c\xfabico")];

/// `PTR_DAT_10069370` (`DAT_10069388` = 6).
const PATTERNS: [&[u8]; 6] = [b"$/A", b"A", b"AA", b"A/A", b"AOA", b"AOA/A"];

/// `FUN_10032c50`'s context tables.
const CTX_W: [&[u8]; 2] = [b"bulbo", b"bulbos"];
const CTX_M: [&[u8]; 5] = [b"cm", b"larga", b"largas", b"largo", b"largos"];
const CTX_N: [&[u8]; 4] = [b"kg", b"lb", b"poder", b"poders"];
const CTX_CFKR: [&[u8]; 6] = [b"CF", b"K", b"R", b"grado", b"grados", b"\xb0"];
const CTX_S: [&[u8]; 22] = [
    b"a.m",
    b"am",
    b"de la ma\xf1ana",
    b"de la noche",
    b"de la tarde",
    b"en la ma\xf1ana",
    b"en la noche",
    b"en la tarde",
    b"h",
    b"hora",
    b"horas",
    b"hrs",
    b"minuto",
    b"minutos",
    b"p.m",
    b"pm",
    b"por la ma\xf1ana",
    b"por la noche",
    b"por la tarde",
    b"segundo",
    b"segundos",
    b"tiempo",
];
const CTX_GT: [&[u8]; 6] = [b"gramo", b"gramos", b"kilogramo", b"kilogramos", b"peso", b"pesos"];

/// The leading amount (`local_4c0`).
#[derive(Clone, Default)]
struct Pre {
    ws: i32,
    start: i32,
    end: i32,
    len: i32,
    class: i32,
    /// +0x18.
    plural: bool,
    /// +0x1c: where the amount's text starts.
    pos: usize,
    /// +0x20.
    text: Vec<u8>,
    /// +0x152: the previous word is a currency amount.
    dollar: bool,
}

/// One unit token (0x140 bytes each in the WA scratch area).
#[derive(Clone, Default)]
struct Slot {
    ws: i32,
    start: i32,
    end: i32,
    len: i32,
    class: i32,
    gender: u8,
    tag: u8,
    /// +0x1c: exponent + 2 index ("cuadrado"/"c\xfabico").
    exp: i16,
    /// +0x1e: a '.' followed.
    dot: i16,
    /// +0x20.
    plural: bool,
    /// +0x24: text position after the token.
    after: usize,
    /// +0x28.
    text: Vec<u8>,
    /// +0x46.
    render: Vec<u8>,
}

/// `local_100`: the render buffer. Only its first byte is ever cleared, and
/// `FUN_10032660` reads bytes 12..16 as an int (`param_2 + 0x3cc`), so the
/// residue of longer renders is kept. The stack garbage it starts with is
/// modelled as zeros.
struct Buf([u8; 256]);

impl Buf {
    fn set(&mut self, s: &[u8]) {
        let n = s.len().min(255);
        self.0[..n].copy_from_slice(&s[..n]);
        self.0[n] = 0;
    }

    fn get(&self) -> &[u8] {
        let n = self.0.iter().position(|&c| c == 0).unwrap_or(256);
        &self.0[..n]
    }

    fn residue(&self) -> i32 {
        i32::from_le_bytes([self.0[12], self.0[13], self.0[14], self.0[15]])
    }
}

fn c0(tok: &Tok) -> u8 {
    tok.text[0]
}

fn upper(c: u8) -> u8 {
    c.to_ascii_uppercase()
}

/// `FUN_10033500(tok, t+p, off)`: "N, N y N" lists; the bytes up to the
/// last number, or -1.
fn list_scan(tok: &Tok, t: &[u8], p: usize, off: i32) -> i32 {
    if tok.class != 2 || !numeric(tok.text()) {
        return -1;
    }
    let mut i = tok.consumed;
    let (nt, c) = tokenize(t, p + i as usize, off + i, 0xb);
    if c == 0 || c0(&nt) != b',' || nt.len != 1 || nt.ws >= 2 {
        return -1;
    }
    let mut state = 1u8;
    i += c;
    if t[p + i as usize] == 0 {
        return -1;
    }
    loop {
        let (nt, c) = tokenize(t, p + i as usize, off + i, 0xb);
        if c == 0 {
            return -1;
        }
        if state != 0 && numeric(nt.text()) && nt.ws <= 1 {
            if state == 2 {
                return i + c;
            }
            state = 0;
        } else {
            if state != 0 || (c0(&nt) != b',' && c0(&nt) != b'y') || nt.len != 1 || nt.ws > 1 {
                return -1;
            }
            state = if c0(&nt) != b',' { 2 } else { 1 };
        }
        i += c;
        if t[p + i as usize] == 0 {
            return -1;
        }
    }
}

/// The token after a range's second number must be a word, "\xb0" or "%".
fn range_tail(t: &[u8], p: usize, off: i32, i: i32) -> i32 {
    let (nt, _) = tokenize(t, p + i as usize, off + i, 0xb);
    match nt.class {
        6 if nt.len == 1 && c0(&nt) == 0xb0 => i,
        3 if nt.len == 1 && c0(&nt) == b'%' => i,
        1 => i,
        _ => -1,
    }
}

/// `FUN_10033620(tok, t+p, off)`: "N a N", "N y N", "N & N", "N al N",
/// "N-N" and "N / N" ranges; the bytes up to the second number, or -1.
fn range_scan(tok: &Tok, t: &[u8], p: usize, off: i32) -> i32 {
    if tok.class != 2 || !numeric(tok.text()) {
        return -1;
    }
    let mut i = tok.consumed;
    let (nt, c) = tokenize(t, p + i as usize, off + i, 0xb);
    if c == 0 {
        return -1;
    }
    let k = c0(&nt);
    if ((matches!(k, b'a' | b'y' | b'&') && nt.len == 1) || stricmp_spa(nt.text(), AL) == 0) && nt.ws < 2 {
        i += c;
        let (n2, c2) = tokenize(t, p + i as usize, off + i, 0xb);
        if c2 == 0 || !numeric(n2.text()) || n2.ws > 1 {
            return -1;
        }
        return i + c2;
    }
    if matches!(k, b'-' | 0x96 | 0x97) && nt.len == 1 && nt.ws >= 0 && nt.ws < 2 {
        i += c;
        let (n2, c2) = tokenize(t, p + i as usize, off + i, 0xb);
        if c2 == 0 || !numeric(n2.text()) || n2.ws < 0 || n2.ws > 1 {
            return -1;
        }
        return range_tail(t, p, off, i + c2);
    }
    if k != b'/' || nt.len != 1 || nt.ws != 1 {
        return -1;
    }
    i += c;
    let (n2, c2) = tokenize(t, p + i as usize, off + i, 0xb);
    if c2 == 0 || !numeric(n2.text()) || n2.ws != 1 {
        return -1;
    }
    range_tail(t, p, off, i + c2)
}

/// `FUN_1004bfe0(best, last, s, off, 0)`: tokenize all of `s`; `best` is the
/// last non-empty token that stops short of the end, `last` the final one.
pub(super) fn best_and_last(s: &[u8], off: i32, mode: i32) -> (Tok, Tok) {
    let mut best = Tok::default();
    let mut last = Tok::default();
    if s.is_empty() {
        return (best, last);
    }
    let mut b = s.to_vec();
    b.extend_from_slice(&[0u8; 8]);
    let total = s.len() as i32;
    let mut acc = 0i32;
    while acc < total {
        let (tk, c) = tokenize(&b, acc as usize, off + acc, mode);
        if c + acc < total && tk.len != 0 {
            best = tk.clone();
        }
        last = tk;
        if c == 0 {
            break;
        }
        acc += c;
    }
    (best, last)
}

fn last_rec(wa: &WaSpa) -> Option<usize> {
    let n = wa.i16(0);
    if n >= 1 {
        Some(WaSpa::rec(n as usize - 1))
    } else {
        None
    }
}

/// `FUN_10033050(pre, wa, tok, t+p, off)`: the leading amount. Returns its
/// length, -1 when it is the previous word (already written), or 0.
fn pre_scan(wa: &WaSpa, tok: &Tok, t: &[u8], p: usize, off: i32) -> (Pre, i32) {
    let mut pre = Pre::default();
    let grouped = tok.class == 2 && (is_period_grouped(tok.text()) || is_comma_grouped(tok.text()));
    if grouped {
        let mut r = list_scan(tok, t, p, off);
        if r < 0 {
            r = range_scan(tok, t, p, off);
        }
        if r >= 0 {
            pre.ws = tok.ws;
            pre.start = off;
            pre.end = r + off;
            pre.len = r - tok.ws;
            pre.class = 2;
            pre.text = NUMS.to_vec();
            pre.pos = p;
            pre.plural = true;
            return (pre, r);
        }
        let mut r = tok.consumed;
        let (nt, c) = tokenize(t, p + r as usize, off + r, 0);
        if c == 0 || nt.class == 10 {
            return (pre, 0);
        }
        if matches!(nt.class, 3 | 8) && nt.len == 1 && c0(&nt) == b'-' && nt.ws < 2 {
            r += c;
        }
        pre.ws = tok.ws;
        pre.start = tok.start;
        pre.end = tok.end;
        pre.len = tok.len;
        pre.class = 2;
        pre.text = tok.text().to_vec();
        pre.plural = plural(tok.text());
        return (pre, r);
    }

    let (w, back) = prev_words(t, p - off as usize, p, 2);
    if back == 0 {
        return (pre, 0);
    }
    let (best, last) = best_and_last(&w, off - back as i32, 0);
    let from = |pre: &mut Pre, tk: &Tok, class: i32| {
        pre.ws = tk.ws;
        pre.start = tk.start;
        pre.end = tk.end;
        pre.len = tk.len;
        pre.class = class;
        pre.text = tk.text().to_vec();
    };
    let src_num = |r: usize| matches!(wa.i32(r + rec::SRC), 0x11 | 0xe);
    if best.len < 1 {
        if last.len < 1 {
            return (pre, 0);
        }
    } else {
        if last.len < 1 {
            return (pre, 0);
        }
        if let Some(r) = last_rec(wa) {
            if src_num(r)
                && wa.i32(r + rec::END) == best.end
                && matches!(last.class, 3 | 8)
                && last.len == 1
                && c0(&last) == b'-'
                && last.ws < 2
                && off - last.end < 2
            {
                from(&mut pre, &best, 1);
                return (pre, -1);
            }
        }
    }
    let lr = last_rec(wa);
    if last.class == 1 && (stricmp_spa(last.text(), b"un") == 0 || stricmp_spa(last.text(), b"una") == 0) {
        if let Some(r) = lr {
            if wa.i32(r + rec::START) == last.start && wa.i32(r + rec::END) == last.end {
                from(&mut pre, &last, last.class);
                return (pre, -1);
            }
        }
    }
    let Some(r) = lr else { return (pre, 0) };
    if last.len < 1 {
        return (pre, 0);
    }
    if src_num(r) && wa.i32(r + rec::END) == last.end && last.ws < 2 && off - last.end < 2 {
        from(&mut pre, &last, 1);
        return (pre, -1);
    }
    if wa.i32(r + rec::SRC) != 5 || wa.i32(r + rec::END) != last.end || off - last.end > 2 {
        return (pre, 0);
    }
    pre.dollar = true;
    (pre, -1)
}

/// `FUN_10043bf0(table, words, n, 'I', max)`: the first of `words` that
/// starts a key, the key's later words matched against the following ones.
pub(super) fn phrase_find(table: &[&[u8]], words: &[Vec<u8>], max: usize) -> bool {
    for i in 0..max {
        let Some(w) = words.get(i) else { return false };
        if w.is_empty() {
            return false;
        }
        let (mut lo, mut hi) = (0i32, table.len() as i32 - 1);
        let mut sum = hi;
        while lo <= hi {
            let mid = sum / 2;
            let key = table[mid as usize];
            let c = strnicmp_spa(key, w, w.len());
            if c == 0 {
                let nx = key.get(w.len()).copied().unwrap_or(0);
                if nx == 0 {
                    return true;
                }
                let mut cmp = 0;
                if nx == b' ' {
                    let mut rest = &key[w.len() + 1..];
                    for wj in words.iter().take(max).skip(i + 1) {
                        let cj = strnicmp_spa(rest, wj, wj.len());
                        if cj != 0 {
                            cmp = cj;
                            break;
                        }
                        let a = rest.get(wj.len()).copied().unwrap_or(0);
                        if a == 0 {
                            return true;
                        }
                        if a != b' ' {
                            break;
                        }
                        rest = &rest[wj.len() + 1..];
                    }
                }
                if cmp < 0 {
                    lo = mid + 1;
                } else {
                    hi = mid - 1;
                }
            } else if c < 0 {
                lo = mid + 1;
            } else {
                hi = mid - 1;
            }
            sum = lo + hi;
        }
    }
    false
}

/// `FUN_10032c50(unit, t+p0, off0, 'B', consumed)`: a one-letter group-C unit
/// is confirmed by a context word among the five tokens before the amount or
/// the five after the units.
fn context(unit: &[u8], t: &[u8], p0: usize, off0: i32, consumed: i32) -> bool {
    let mut left: Vec<Vec<u8>> = Vec::new();
    let (w, back) = prev_words(t, p0 - off0 as usize, p0, 5);
    if back != 0 && !w.is_empty() {
        let mut b = w.clone();
        b.extend_from_slice(&[0u8; 8]);
        let mut bufs: [Vec<u8>; 5] = Default::default();
        let mut cnt = 0usize;
        let mut acc = 0usize;
        while acc < w.len() {
            let (tk, c) = tokenize(&b, acc, acc as i32, 0xb);
            if cnt > 0 {
                for j in (1..=cnt).rev() {
                    bufs[j] = bufs[j - 1].clone();
                }
            }
            bufs[0] = tk.text().to_vec();
            if cnt < 4 {
                cnt += 1;
            }
            if c == 0 {
                break;
            }
            acc += c as usize;
        }
        for b in bufs.iter() {
            if b.is_empty() {
                break;
            }
            left.push(b.clone());
        }
    }
    let mut right: Vec<Vec<u8>> = Vec::new();
    let mut q = p0 + consumed as usize;
    let mut o = off0 + consumed;
    while right.len() < 5 {
        let (tk, c) = tokenize(t, q, o, 0xb);
        if c == 0 || tk.class == 9 || tk.class == 10 {
            break;
        }
        q += c as usize;
        o += c;
        right.push(tk.text().to_vec());
    }
    if unit.len() != 1 {
        return false;
    }
    let u = upper(unit[0]);
    let hit = |tab: &[&[u8]]| phrase_find(tab, &left, 5) || phrase_find(tab, &right, 5);
    (u == b'W' && hit(&CTX_W))
        || (u == b'M' && hit(&CTX_M))
        || (u == b'N' && hit(&CTX_N))
        || (matches!(u, b'C' | b'F' | b'K' | b'R') && hit(&CTX_CFKR))
        || (u == b'S' && hit(&CTX_S))
        || (matches!(u, b'G' | b'T') && hit(&CTX_GT))
}

fn is_un(s: &[u8]) -> bool {
    stricmp_spa(s, b"un") == 0 || stricmp_spa(s, b"una") == 0
}

/// `FUN_10032660(slots, pre, pattern, ctx)`.
fn validate(s: &mut [Slot], pre: &Pre, k: usize, ctx: bool, residue: i32, t: &[u8]) -> bool {
    if k > 0 && pre.len > 0 {
        if pre.class == 1 {
            if s[0].ws > 2 {
                return false;
            }
        } else if s[0].ws > 1 {
            return false;
        }
    }
    match k {
        1 => {
            let tag = s[0].tag;
            if tag == b'P' || tag == b'A' {
                if s[0].exp < 1 {
                    if pre.len < 1 {
                        return false;
                    }
                    if is_un(&pre.text) && (stricmp_spa(&s[0].text, b"MS") == 0 || stricmp_spa(&s[0].text, b"ATM") == 0) {
                        return false;
                    }
                    if [&b"in"[..], b"dam", b"fur", b"ha"].iter().any(|w| stricmp_spa(&s[0].text, w) == 0) {
                        let (nt, _) = tokenize(t, s[0].after, 0, 0xb);
                        let dot = c0(&nt) == b'.' && nt.ws == 0 && nt.len == 1;
                        if !ctx && (!dot || sentence_end_code(&nt, t, s[0].after, 0) == b'P') && (residue < 1 || s[0].ws != 0) {
                            return false;
                        }
                    }
                }
            } else if tag == b'C' && s[0].exp < 1 && !ctx {
                return false;
            }
            true
        }
        2 => {
            if s[0].tag == b'E' && s[1].tag == b'C' {
                if s[1].dot != 0 {
                    s[1].len -= 1;
                }
                let u = upper(s[1].text.first().copied().unwrap_or(0));
                if s[1].len == 1 {
                    if u == b'C' {
                        s[1].render = b"celsius".to_vec();
                        return true;
                    }
                    if u == b'F' {
                        return true;
                    }
                } else if s[1].len != 2 {
                    return false;
                }
                u == b'R'
            } else if s[0].tag == b'C' && s[1].tag == b'P' {
                let fl = stricmp_spa(&s[0].text, b"fl") == 0 || stricmp_spa(&s[0].text, b"t") == 0;
                let oz = stricmp_spa(&s[1].text, b"oz") == 0 || stricmp_spa(&s[1].text, b"ozs") == 0;
                if !(fl && oz) {
                    return false;
                }
                s[0].render = if s[1].len == 2 { b"onza".to_vec() } else { b"onzas".to_vec() };
                let troy = s[0].len == 1 && upper(s[0].text[0]) == b'T';
                s[1].render = if troy { b"troy".to_vec() } else { b"l\xedquida".to_vec() };
                true
            } else {
                false
            }
        }
        3 | 4 => {
            if s[1].ws != s[2].ws {
                return false;
            }
            if s[0].tag == b'C' && s[2].tag == b'C' && s[0].exp < 1 && s[2].exp < 1 {
                if !ctx && pre.len == 0 {
                    return false;
                }
                if pre.len > 0 && is_un(&pre.text) {
                    return false;
                }
                if s[1].ws != 0 {
                    return false;
                }
                if k == 3 {
                    if s[0].len == 1 && s[2].len == 1 {
                        let (a, b) = (s[0].text[0], s[2].text[0]);
                        if (a == b'C' && b == b'C') || (a == b'M' && b == b'V') {
                            return false;
                        }
                    }
                } else if s[0].ws == 0 && pre.len > 0 && residue == 0 {
                    return false;
                }
            }
            true
        }
        5 => s[3].ws == s[4].ws,
        _ => true,
    }
}

fn mark_plural(wa: &mut WaSpa) {
    if let Some(r) = last_rec(wa) {
        wa.set_i16(r + 0x12, 1);
    }
}

/// `FUN_100324d0`: a range or list amount, one word per token.
fn render_nums(wa: &mut WaSpa, t: &[u8], pre: &Pre, gender: u8) -> bool {
    let span = pre.end - pre.start;
    let mut pos = pre.pos;
    let mut o = pre.start;
    let mut done = 0;
    while done < span {
        let (sub, r) = tokenize(t, pos, o, 0xb);
        let ok = match sub.class {
            2 => {
                let mut w = Vec::new();
                cardinal_to_words(&mut w, sub.text(), gender as i32);
                emit_split(wa, &w, sub.start, sub.end, b'D', b'D', SRC)
            }
            1 => emit(wa, sub.text(), sub.start, sub.end, b'A', b'D', SRC, None),
            3 => match c0(&sub) {
                b'-' => emit(wa, RANGE_A, sub.start, sub.end, b'A', b'D', SRC, None),
                b'/' | b'&' => emit(wa, RANGE_Y, sub.start, sub.end, b'A', b'D', SRC, None),
                _ => true,
            },
            6 if matches!(c0(&sub), 0x96 | 0x97) => emit(wa, RANGE_A, sub.start, sub.end, b'A', b'D', SRC, None),
            _ => true,
        };
        if !ok {
            return false;
        }
        if r == 0 {
            break;
        }
        done += r;
        pos += r as usize;
        o += r;
    }
    true
}

/// `FUN_100321c0`: write the amount and the matched units.
fn commit(wa: &mut WaSpa, s: &[Slot], n: usize, pre: &Pre, t: &[u8]) -> bool {
    let amount = pre.len > 0 && pre.class == 2;
    if amount {
        if pre.text == NUMS {
            if !render_nums(wa, t, pre, s[0].gender) {
                return false;
            }
        } else {
            let mut w = Vec::new();
            cardinal_to_words(&mut w, &pre.text, s[0].gender as i32);
            if !emit_split(wa, &w, pre.start, pre.end, b'D', b'D', SRC) {
                return false;
            }
        }
        if pre.plural {
            mark_plural(wa);
        }
    }
    for (i, u) in s.iter().enumerate().take(n) {
        if u.len <= 0 {
            continue;
        }
        if i == 0 && amount {
            if let Some(r) = last_rec(wa) {
                let last = wa.cstr(r + rec::TEXT).to_vec();
                if stricmp_spa(&last, b"uno") == 0 || stricmp_spa(&last, b"un") == 0 || stricmp_spa(&last, b"una") == 0 {
                    if u.gender == b'M' {
                        wa.set_i32(r + rec::LEN, 2);
                        wa.strcpy(r + rec::TEXT, b"un");
                    } else {
                        wa.strcpy(r + rec::TEXT, b"una");
                    }
                }
            }
        }
        match u.tag {
            b'/' | b'O' | b'S' => {
                if !emit_split(wa, &u.render, u.start, u.end, b'S', b'D', SRC) {
                    return false;
                }
            }
            b'a' => {
                if !emit(wa, &u.text, u.start, u.end, b'A', b'D', SRC, None) {
                    return false;
                }
            }
            _ => {
                if !emit_split(wa, &u.render, u.start, u.end, b'A', b'D', SRC) {
                    return false;
                }
                if u.plural {
                    mark_plural(wa);
                }
            }
        }
        if u.exp > 0 {
            let name = SUP[(u.exp - 2).clamp(0, 1) as usize].1;
            if !emit(wa, AL, u.start, u.end, b'A', b'D', SRC, None) || !emit_split(wa, name, u.start, u.end, b'D', b'D', SRC) {
                return false;
            }
        }
    }
    true
}

enum Step {
    Commit,
    Skip,
    Stop,
}

pub fn units(_fe: &FeSpa, wa: &mut WaSpa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let mut s: Vec<Slot> = vec![Slot::default(); 6];
    let mut t524 = [0u8; 32];
    let mut t53c = [0u8; 8];
    let (mut u10, mut u11) = (0usize, 0usize);
    let mut gender = b'M';
    let mut buf = Buf([0u8; 256]);
    // Pending "cu"/"sq" prefix (`local_240`).
    let (mut w_ws, mut w_start, mut w_len, mut w_exp, mut w_dot) = (0i32, 0i32, 0i32, 0i16, 0i16);

    let (pre, r) = pre_scan(wa, tok, t, p, off);
    let (mut p3, mut p4) = (p, off);
    let mut l540 = r;
    let mut plural544 = 0i16;
    if r == -1 {
        if pre.dollar {
            t524[0] = b'$';
            t53c[0] = b'$';
            u10 = 1;
            u11 = 1;
        }
        l540 = 0;
    } else if r > 0 {
        p3 += r as usize;
        p4 += r;
    }
    if r != 0 && pre.plural {
        plural544 = 1;
    }

    let mut u7 = l540;
    while u11 < 5 {
        let cur: Tok;
        let mut l534: i32;
        if u7 < 1 {
            cur = tok.clone();
            l534 = tok.consumed;
            u7 = l540;
        } else {
            let (tk, c) = tokenize(t, p3, p4, 0xb);
            if c == 0 || tk.class == 10 {
                break;
            }
            cur = tk;
            l534 = c;
        }
        let ch = c0(&cur);
        let step: Step = 'body: {
            if matches!(cur.class, 3 | 8) && cur.len == 1 && ch == b'/' {
                t524[u10] = b'/';
                t53c[u11] = b'/';
                buf.set(POR);
                break 'body Step::Commit;
            }
            let dot = ch == b'.' && cur.ws == 0 && cur.len == 1;
            let mut x_check = !dot;
            if dot {
                let after_unit = u11 >= 1 && t53c[u11 - 1] == b'A' && t524[u10 - 1] != 0;
                if after_unit && b"EPC".contains(&t524[u10 - 1]) {
                    t524[u10] = b'.';
                    p3 += l534 as usize;
                    p4 += l534;
                    s[u11 - 1].dot = 1;
                    break 'body Step::Skip;
                }
                if u10 >= 1 {
                    if t524[u10 - 1] == b'W' {
                        p3 += l534 as usize;
                        u7 += l534;
                        p4 += l534;
                        t524[u10] = b'.';
                        w_len += 1;
                        w_dot = 1;
                        l540 = u7;
                        break 'body Step::Skip;
                    }
                    x_check = true;
                }
            }
            if x_check && u10 >= 1 {
                let x = (upper(ch) == b'X' || ch == 0xb7 || ch == 0xd7 || ch == 0xf7) && cur.len == 1 && cur.ws < 2;
                if x {
                    t524[u10] = b'O';
                    t53c[u11] = b'O';
                    buf.set(POR);
                    break 'body Step::Commit;
                }
            }
            // LAB_10031a43
            if u11 > 0 && t53c[u11 - 1] == b'A' && cur.class == 6 && cur.len == 1 && cur.ws == 0 {
                if let Some(i) = SUP.iter().position(|&(c, _)| c == ch) {
                    u7 += l534;
                    p4 += l534;
                    p3 += l534 as usize;
                    t524[u10] = b's';
                    s[u11 - 1].end += 1;
                    s[u11 - 1].len += 1;
                    s[u11 - 1].exp = i as i16 + 2;
                    l540 = u7;
                    break 'body Step::Skip;
                }
            }
            let text = cur.text().to_vec();
            if w_len == 0 && cur.class == 1 && cur.len == 2 {
                if let Some(i) = find_i(&CU_SQ, &text) {
                    w_ws = cur.ws;
                    w_start = cur.start;
                    w_len = cur.len;
                    t524[u10] = b'W';
                    w_exp = CU_SQ[i].2[0] as i16 - b'0' as i16;
                    u7 += l534;
                    p4 += l534;
                    p3 += l534 as usize;
                    l540 = u7;
                    break 'body Step::Skip;
                }
            }
            let mode = plural544;
            let mut out = Vec::new();
            let mut tag = 0u8;
            if matches!(cur.class, 1 | 3 | 6) && group_lookup(0, Some(&mut out), &text, mode, Some(&mut gender)) {
                tag = b'E';
            } else if cur.class == 1 {
                if group_lookup(1, Some(&mut out), &text, mode, Some(&mut gender)) {
                    tag = b'P';
                } else if group_lookup(2, Some(&mut out), &text, mode, Some(&mut gender)) {
                    tag = b'C';
                }
            }
            if tag != 0 {
                buf.set(&out);
            } else {
                buf.0[0] = 0;
                if unit_lookup(Some(&mut out), &text, t, p3, u7, &mut l534, Some(&mut gender)) {
                    buf.set(&out);
                    tag = b'E';
                } else {
                    if u11 == 2 && t53c[0] == b'$' && t53c[1] == b'/' && cur.class == 1 && s[1].ws == 0 {
                        t524[u10] = b'a';
                        t53c[2] = b'A';
                        break 'body Step::Commit;
                    }
                    break 'body Step::Stop;
                }
            }
            t524[u10] = tag;
            t53c[u11] = b'A';
            if mode != 0 {
                plural544 = 0;
                s[u11].plural = true;
            }
            if w_len > 0 && (t524[u10 - 1] == b'W' || (t524[u10 - 1] == b'.' && w_dot != 0)) {
                s[u11].ws = w_ws;
                s[u11].start = w_start;
                s[u11].exp = w_exp;
                w_ws = 0;
                w_start = 0;
                w_len = 0;
                w_exp = 0;
                w_dot = 0;
            }
            s[u11].gender = gender;
            if gender == b'F' {
                gender = b'M';
            }
            Step::Commit
        };
        match step {
            Step::Stop => break,
            Step::Skip => {}
            Step::Commit => {
                let u = &mut s[u11];
                if u.start == 0 {
                    u.start = cur.start;
                    u.ws = cur.ws;
                }
                u.end = p4 + l534;
                u.len = cur.len;
                u.class = cur.class;
                u.tag = t524[u10];
                u.after = p3 + l534 as usize;
                u.text = cur.text().to_vec();
                if buf.0[0] != 0 {
                    u.render = buf.get().to_vec();
                    buf.0[0] = 0;
                }
                u7 = l540 + l534;
                l540 = u7;
                p4 += l534;
                p3 += l534 as usize;
                u11 += 1;
            }
        }
        u10 += 1;
    }

    let mode = match t53c[0] {
        b'$' => 1,
        b'A' => 2,
        _ => return -1,
    };
    let mut ctx = false;
    let mut n = u11;
    while n >= 1 {
        let range = match (mode, n) {
            (1, 3) => Some(0..1),
            (2, 1) => Some(1..2),
            (2, 2) => Some(2..3),
            (2, 3) => Some(3..5),
            (2, 5) => Some(5..6),
            _ => None,
        };
        if let Some(range) = range {
            if let Some(k) = range.into_iter().find(|&k| PATTERNS[k] == &t53c[..n]) {
                let consumed = s[n - 1].end - off;
                if k == 1 {
                    if pre.len > 0 && s[0].tag == b'C' && context(&s[0].text, t, p, off, consumed) {
                        ctx = true;
                    }
                } else if k == 3 {
                    if s[0].tag == b'C' && context(&s[0].text, t, p, off, consumed) {
                        ctx = true;
                    }
                    if s[2].tag == b'C' && context(&s[2].text, t, p, off, consumed) {
                        ctx = true;
                    }
                }
                if validate(&mut s, &pre, k, ctx, buf.residue(), t) {
                    return if commit(wa, &s, n, &pre, t) { consumed } else { 0 };
                }
            }
        }
        n -= 1;
    }
    -1
}
