//! `FUN_1002e6c0` (chain slot 8): "operand operator operand" expressions.
//!
//! Operands and operators are collected into up to four entries (the engine's
//! 0x140-byte records behind WA `+0x41ac`), then only the shape `VOV` is
//! accepted (`PTR_DAT_1006917c`: "VS" is listed but `FUN_1002f580` rejects
//! every pattern index but 1). Validation is `FUN_1002f580`, emission
//! `FUN_1002f150`. The engine reads '(' as "cerrar paréntesis" and ')' as
//! "abrir paréntesis" (`DAT_10069234` / `DAT_100691b0`); kept.

use super::emit::emit_split;
use super::numwords::{cardinal_to_words, is_comma_grouped, is_period_grouped, lookahead_gender};
use super::punct::prev_word;
use super::tables::{CLS_S, HI};
use super::tokenize::{tokenize, Tok};
use super::{emit, rec, FeSpa};
use crate::ling_spa::WaSpa;

/// `DAT_10069198`, `DAT_10069194`, `DAT_10069190`, `s_______10069254`.
const ARITH: &[u8] = b"+-*/";
const TIMES: &[u8] = b"xX";
const COMPARE: &[u8] = b"<>=";
const FOLLOW_OPS: &[u8] = b"+-*/=";

/// `DAT_10070248` (key byte, then the word at +1; `DAT_10079e28` = 2).
const SUPERSCRIPT: [(u8, &[u8]); 2] = [(0xb2, b"cuadrado"), (0xb3, b"c\xfabico")];

const SRC: u32 = 0x11;

fn in_set(set: &[u8], c: u8) -> bool {
    c != 0 && set.contains(&c)
}

fn digit(c: u8) -> bool {
    CLS_S[c as usize] & 0x10 != 0
}

/// `DAT_10065864` is a toupper table; only 'x' and 'X' map to 0x58.
fn is_x(c: u8) -> bool {
    c == b'x' || c == b'X'
}

/// `FUN_10043860` on `DAT_10070248`.
fn superscript(c: u8) -> i32 {
    if c == 0 {
        return -1;
    }
    match SUPERSCRIPT.iter().position(|&(k, _)| k == c) {
        Some(i) => i as i32,
        None => -1,
    }
}

/// `FUN_1004b600`.
fn all_digits(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&c| digit(c))
}

/// `FUN_1004b5c0`.
fn all_alpha(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&c| CLS_S[c as usize] & 0xc0 != 0 || (c > 0x7f && HI[c as usize] & 0xc0 != 0))
}

/// `FUN_1004b630`: digits, with '.' only between digits and ',' before a digit.
/// `pub(super)`: also used by `norm_units.rs`'s range shape (`FUN_10033620`
/// calls this same function on its own second operand) -- verified against
/// a fresh decompile of `FUN_1004b630` before reuse, not assumed.
pub(super) fn numeric(s: &[u8]) -> bool {
    let n = s.len();
    if n == 0 {
        return false;
    }
    for i in 0..n {
        let c = s[i];
        if digit(c) {
            continue;
        }
        if c == b'.' && i >= 1 && i + 1 < n && digit(s[i - 1]) && digit(s[i + 1]) {
            continue;
        }
        if c != b',' || i + 1 >= n {
            return false;
        }
        if i == 0 {
            if !digit(s[1]) {
                return false;
            }
        } else if !digit(s[i - 1]) || !digit(s[i + 1]) {
            return false;
        }
    }
    true
}

/// `FUN_1002faf0`: 'x'/'X' followed only by digits.
fn x_digits(s: &[u8]) -> bool {
    if s.is_empty() || !is_x(s[0]) {
        return false;
    }
    s[1..].iter().all(|&c| digit(c))
}

/// `FUN_1002fb50`: under 6 characters, each a digit or one of "abcxyz".
fn variable(s: &[u8]) -> bool {
    !s.is_empty() && s.len() < 6 && s.iter().all(|&c| b"abcxyz".contains(&c) || digit(c))
}

#[derive(Clone, Default)]
struct Entry {
    ws: i32,
    start: i32,
    end: i32,
    len: i32,
    class: u8,
    role: u8,
    /// +0x14: superscript index + 2.
    sup: i16,
    /// +0x16: 1 '(' before, 2 ')' after, 4 '{' before, 8 '}' after.
    flags: u8,
    /// +0x18: a '=' was folded into this comparison operator.
    or_equal: i16,
    text: Vec<u8>,
    /// +0x3e: `FUN_1004ca30` output for 'N' operands.
    words: Vec<u8>,
}

impl Entry {
    fn c0(&self) -> u8 {
        self.text.first().copied().unwrap_or(0)
    }
}

/// `FUN_1004bf70(.., mode)`: the last token of `w`, offsets from `off`.
fn last_token(w: &[u8], off: i32, mode: i32) -> Option<Tok> {
    if w.is_empty() {
        return None;
    }
    let mut b = w.to_vec();
    b.extend_from_slice(&[0u8; 64]);
    let mut tok = Tok::default();
    let mut used = 0usize;
    while used < w.len() {
        let (t, _) = tokenize(&b, used, off + used as i32, mode);
        let c = t.consumed;
        tok = t;
        if c <= 0 {
            break;
        }
        used += c as usize;
    }
    Some(tok)
}

/// `FUN_1002f9f0`: the word just before, when this module emitted it
/// (SRC 0x11) and it ends at most one byte before `off`.
fn lookback(wa: &WaSpa, t: &[u8], p: usize, off: i32) -> Option<Entry> {
    let (w, back) = prev_word(t, p - off as usize, p);
    if back == 0 {
        return None;
    }
    let tk = last_token(&w, off - back as i32, 0x13)?;
    if tk.len <= 0 {
        return None;
    }
    let n = wa.i16(0);
    if n <= 0 {
        return None;
    }
    let r = WaSpa::rec(n as usize - 1);
    if wa.i32(r + rec::SRC) != SRC as i32 || wa.i32(r + rec::END) != tk.end || off - tk.end >= 2 {
        return None;
    }
    Some(Entry {
        ws: tk.ws,
        start: tk.start,
        end: tk.end,
        len: 0,
        class: tk.class as u8,
        role: b'V',
        text: tk.text().to_vec(),
        ..Entry::default()
    })
}

/// `FUN_1002f860(.., 'R', ret, ..)`: the next token is a lone operator glued
/// to the expression. Also returns the gender of the following word
/// (`FUN_1004c6f0`) unless that word is itself an operator.
fn follows_operator(fe: &FeSpa, t: &[u8], p: usize, off: i32, next: &mut [Tok; 2]) -> (bool, i32) {
    let mut toks: [Tok; 2] = [Tok::default(), Tok::default()];
    let mut gender = 0;
    let mut k = 0usize;
    let mut q = p;
    let mut o = off;
    while k < 2 {
        let (tk, r) = tokenize(t, q, o, 0x13);
        if r == 0 || tk.class == 9 || tk.class == 10 {
            break;
        }
        if k == 0 {
            let c = tk.text[0];
            if !(tk.len == 1 && in_set(FOLLOW_OPS, c)) {
                gender = lookahead_gender(fe, t, q, o, false);
            }
        }
        toks[k] = tk;
        q += r as usize;
        o += r;
        k += 1;
    }
    if k > 0 && toks[0].len == 1 && toks[0].ws == 0 && in_set(FOLLOW_OPS, toks[0].text[0]) {
        next[0] = toks[0].clone();
        if k > 1 {
            next[1] = toks[1].clone();
        }
        return (true, gender);
    }
    (false, gender)
}

/// `FUN_1002f580`.
fn validate(e: &mut [Entry], lb_len: i32, pat: i32, glued: bool, next: &[Tok; 2], gender: i32) -> bool {
    if pat != 1 {
        return false;
    }
    let op = e[1].c0();
    let len0 = e[0].len;
    if is_x(op) {
        if len0 > 0 && (e[1].ws < 0 || e[1].ws > 1 || e[2].ws < 0 || e[2].ws > 1 || e[1].ws != e[2].ws) {
            return false;
        }
    } else if len0 > 0 && (e[1].ws != 0 || e[2].ws != 0) {
        return false;
    }
    let num0 = e[0].class == 2 && all_digits(&e[0].text);
    let num2 = e[2].class == 2 && all_digits(&e[2].text);
    if len0 > 0
        && num0
        && op == b'/'
        && e[2].len > 0
        && num2
        && glued
        && next[0].len == 1
        && next[0].class == 3
        && next[0].text[0] == b'/'
        && next[1].len > 0
        && next[1].class == 2
        && all_digits(next[1].text())
    {
        return false;
    }
    if e[0].class == 1
        && e[2].class == 1
        && ((all_alpha(&e[0].text) && lb_len == 0) || len0 > 3 || (all_alpha(&e[2].text) && lb_len == 0) || e[2].len > 3)
    {
        return false;
    }
    if (e[0].class == 1 && all_alpha(&e[0].text) && lb_len == 0 && len0 > 3)
        || (e[2].class == 1 && all_alpha(&e[2].text) && lb_len == 0 && e[2].len > 3)
    {
        return false;
    }
    if len0 > 0 && is_x(op) && !(num0 && num2) {
        return false;
    }
    if op == b'-' && (!glued || !(num0 && num2)) {
        return false;
    }
    for i in [0usize, 2] {
        if e[i].len > 0 && e[i].role == b'N' {
            let text = e[i].text.clone();
            cardinal_to_words(&mut e[i].words, &text, gender);
        }
    }
    true
}

/// `FUN_1002fbb0`: speak a non-numeric operand token by token.
fn operand(wa: &mut WaSpa, text: &[u8], start: i32, end: i32) -> i32 {
    let n = text.len() as i32;
    if n < 1 {
        return n;
    }
    let mut b = text.to_vec();
    b.extend_from_slice(&[0u8; 64]);
    let mut used = 0i32;
    loop {
        let (tk, r) = tokenize(&b, used as usize, start + used, 0);
        let ok = if tk.class == 1 && all_alpha(tk.text()) {
            emit(wa, tk.text(), start, end, b'A', b'S', SRC, None)
        } else if tk.class == 2 && numeric(tk.text()) {
            let mut w = Vec::new();
            cardinal_to_words(&mut w, tk.text(), 0);
            emit_split(wa, &w, start, end, b'D', b'D', SRC)
        } else {
            let i = if tk.class == 6 { superscript(tk.text[0]) } else { -1 };
            if i < 0 {
                return -1;
            }
            emit_split(wa, SUPERSCRIPT[i as usize].1, start, end, b'D', b'D', SRC)
        };
        if !ok {
            return 0;
        }
        if r <= 0 {
            return -1;
        }
        used += r;
        if used >= n {
            return n;
        }
    }
}

/// `FUN_1002f150`.
fn speak(wa: &mut WaSpa, e: &[Entry]) -> bool {
    let s = |wa: &mut WaSpa, w: &[u8], a: i32, b: i32, origin: u8| emit_split(wa, w, a, b, origin, b'D', SRC);
    for x in e {
        if x.len <= 0 {
            continue;
        }
        if x.flags & 4 == 4 {
            let w: &[u8] = if x.flags & 1 == 1 {
                if !s(wa, b"abrir llave", x.start - 2, x.start - 1, b'S') {
                    return false;
                }
                b"cerrar par\xe9ntesis"
            } else {
                b"abrir llave"
            };
            if !s(wa, w, x.start - 1, x.start, b'S') {
                return false;
            }
        } else if x.flags & 1 == 1 && !s(wa, b"cerrar par\xe9ntesis", x.start - 1, x.start, b'S') {
            return false;
        }
        if x.role == b'O' {
            let w: &[u8] = match x.c0() {
                b'*' | b'X' | b'x' => b"por",
                b'+' => b"m\xe1s",
                b'-' => b"menos",
                b'/' => b"dividido por",
                b'<' if x.or_equal == 0 => b"menor que",
                b'<' => b"menor o igual que",
                b'=' if x.or_equal == 0 => b"son",
                b'=' => b"igual que",
                b'>' if x.or_equal == 0 => b"mayor que",
                b'>' => b"mayor o igual que",
                _ => b"",
            };
            if !s(wa, w, x.start, x.end, b'S') {
                return false;
            }
        } else {
            if x.sup > 0 && !s(wa, SUPERSCRIPT[(x.sup - 2) as usize].1, x.start, x.end, b'D') {
                return false;
            }
            if x.role == b'N' {
                if !s(wa, &x.words, x.start, x.end, b'D') {
                    return false;
                }
            } else if operand(wa, &x.text, x.start, x.end) == 0 {
                return false;
            }
        }
        if x.flags & 2 == 2 {
            if !s(wa, b"abrir par\xe9ntesis", x.end, x.end + 1, b'S') {
                return false;
            }
            if x.flags & 8 == 8 && !s(wa, b"cerrar llave", x.end + 1, x.end + 2, b'S') {
                return false;
            }
        } else if x.flags & 8 == 8 && !s(wa, b"cerrar llave", x.end, x.end + 1, b'S') {
            return false;
        }
    }
    true
}

enum L {
    A,
    B,
    C,
    D,
    G,
    H,
    I,
    J,
    K,
    E,
    F,
    Stop,
}

pub fn math(fe: &FeSpa, wa: &mut WaSpa, _tok: &mut Tok, t: &[u8], p0: usize, off0: i32) -> i32 {
    let mut e: Vec<Entry> = vec![Entry::default(); 5];
    let mut roles = [0u8; 256];
    let mut pat = [0u8; 8];
    let mut n = 0usize;
    let mut u = 0usize;
    let mut lb = false;
    let mut retok = false;
    let mut paren = 0i32;
    let mut brace = 0i32;
    let mut pmark = -1i32;
    let mut bmark = -1i32;
    let (mut p, mut off) = (p0, off0);

    let back = lookback(wa, t, p0, off0);
    let lb_len = back.as_ref().map_or(0, |b| b.len);
    if let Some(b) = back {
        roles[0] = b'V';
        pat[0] = b'V';
        lb = true;
        e[0] = b;
        n = 1;
        u = 1;
    }

    'outer: loop {
        let mode = if retok { 0 } else { 0x13 };
        retok = false;
        let (mut tk, mut r) = tokenize(t, p, off, mode);
        if r == 0 || tk.class == 10 {
            break;
        }
        let mut st = if !lb || u != 1 {
            if u > 0 {
                if roles[u - 1] == b'N' {
                    L::A
                } else {
                    L::B
                }
            } else {
                L::C
            }
        } else {
            L::A
        };
        loop {
            let c0 = tk.text[0];
            let prev = if u > 0 { roles[u - 1] } else { 0 };
            let operand_or_open = matches!(prev, b'N' | b'V' | b'{' | b'(');
            st = match st {
                L::A => {
                    if x_digits(tk.text()) {
                        let (t2, r2) = tokenize(t, p, off, 0);
                        tk = t2;
                        r = r2;
                        if r == 0 || tk.class == 10 {
                            break 'outer;
                        }
                        retok = true;
                    }
                    L::B
                }
                L::B => {
                    if u < 1 {
                        L::C
                    } else if operand_or_open && tk.class == 3 && in_set(ARITH, c0) && tk.len == 1 && tk.ws < 2 {
                        roles[u] = b'O';
                        pat[n] = b'O';
                        L::E
                    } else if operand_or_open && tk.class == 1 && in_set(TIMES, c0) && tk.len == 1 && tk.ws < 2 {
                        roles[u] = b'O';
                        pat[n] = b'O';
                        L::E
                    } else if matches!(prev, b'N' | b'V') && tk.class == 3 && in_set(COMPARE, c0) && tk.len == 1 && tk.ws < 2 {
                        roles[u] = b'O';
                        pat[n] = b'O';
                        L::E
                    } else if prev == b'O'
                        && in_set(COMPARE, e[n - 1].c0())
                        && e[n - 1].or_equal == 0
                        && tk.class == 3
                        && c0 == b'='
                        && tk.len == 1
                        && tk.ws == 0
                    {
                        p += r as usize;
                        off += r;
                        roles[u] = b'O';
                        if n > 0 {
                            e[n - 1].end += 1;
                            e[n - 1].len += 1;
                            e[n - 1].or_equal = 1;
                        }
                        L::F
                    } else if operand_or_open && tk.class == 3 && c0 == b'=' && tk.len == 1 && tk.ws < 2 {
                        roles[u] = b'O';
                        pat[n] = b'O';
                        L::E
                    } else {
                        L::C
                    }
                }
                L::C => {
                    if u == 0 || matches!(prev, b'O' | b'{') {
                        L::D
                    } else {
                        L::G
                    }
                }
                L::D => {
                    if tk.class == 3 && c0 == b'(' && tk.len == 1 && tk.ws < 2 {
                        p += r as usize;
                        off += r;
                        roles[u] = b'(';
                        e[n].flags |= 1;
                        pmark = u as i32;
                        paren += 1;
                        L::F
                    } else {
                        L::G
                    }
                }
                L::G => {
                    if u > 0 && matches!(prev, b'N' | b'V') && tk.class == 3 && c0 == b')' && tk.len == 1 && tk.ws == 0 {
                        let k = if lb_len == 0 { paren } else { lb_len };
                        if k > 0 {
                            p += r as usize;
                            off += r;
                            roles[u] = b'(';
                            paren -= 1;
                            if n > 0 {
                                e[n - 1].flags |= 2;
                            }
                            L::F
                        } else {
                            L::H
                        }
                    } else {
                        L::H
                    }
                }
                L::H => {
                    if u == 0 || prev == b'O' {
                        L::I
                    } else {
                        L::J
                    }
                }
                L::I => {
                    if tk.class == 3 && c0 == b'{' && tk.len == 1 && tk.ws < 2 && brace == 0 {
                        p += r as usize;
                        off += r;
                        roles[u] = b'{';
                        e[n].flags |= 4;
                        brace = 1;
                        bmark = u as i32;
                        L::F
                    } else {
                        L::J
                    }
                }
                L::J => {
                    let mut next = L::K;
                    if u > 0 {
                        if matches!(prev, b'N' | b'V' | b'(') && tk.class == 3 && c0 == b'}' && tk.len == 1 && tk.ws == 0 {
                            let k = if lb_len == 0 { brace } else { lb_len };
                            if k > 0 {
                                p += r as usize;
                                off += r;
                                brace -= 1;
                                roles[u] = b'{';
                                if n > 0 {
                                    e[n - 1].flags |= 8;
                                }
                                next = L::F;
                            }
                        }
                        if !matches!(next, L::F) && prev == b'N' && tk.class == 6 {
                            if tk.len != 1 || tk.ws != 0 {
                                break 'outer;
                            }
                            let i = superscript(c0);
                            if i >= 0 {
                                p += r as usize;
                                off += r;
                                roles[u] = b'S';
                                if n > 0 {
                                    e[n - 1].end += 1;
                                    e[n - 1].len += 1;
                                    e[n - 1].sup = i as i16 + 2;
                                }
                                next = L::F;
                            }
                        }
                    }
                    next
                }
                L::K => {
                    let marked = pmark >= 0 || bmark >= 0;
                    let mut as_variable = tk.class != 2;
                    if tk.class == 2 {
                        let text = tk.text();
                        if (!is_period_grouped(text) && !is_comma_grouped(text)) || tk.ws > 1 {
                            if all_digits(text) {
                                // LAB_1002ee5f: class 2 is not 1.
                                break 'outer;
                            }
                            as_variable = true;
                        } else {
                            if marked {
                                if prev != b'(' {
                                    if prev != b'{' {
                                        break 'outer;
                                    }
                                    bmark = -1;
                                } else {
                                    pmark = -1;
                                    if bmark >= 0 {
                                        bmark = -1;
                                    }
                                }
                            }
                            roles[u] = b'N';
                        }
                    } else if tk.class != 1 {
                        break 'outer;
                    }
                    if as_variable {
                        if !variable(tk.text()) || tk.ws > 1 {
                            break 'outer;
                        }
                        if marked {
                            if prev == b'(' {
                                pmark = -1;
                                if bmark >= 0 {
                                    bmark = -1;
                                }
                            } else if prev != b'{' {
                                break 'outer;
                            } else {
                                bmark = -1;
                            }
                        }
                        roles[u] = b'V';
                    }
                    pat[n] = b'V';
                    L::E
                }
                L::E => {
                    if pmark >= 0 || bmark >= 0 {
                        break 'outer;
                    }
                    off += r;
                    p += r as usize;
                    let x = &mut e[n];
                    x.start = tk.start;
                    x.ws = tk.ws;
                    x.end = off;
                    x.len = tk.len;
                    x.class = tk.class as u8;
                    x.role = roles[u];
                    x.text = tk.text().to_vec();
                    n += 1;
                    L::F
                }
                L::F => {
                    u += 1;
                    if n >= 4 || u >= roles.len() - 1 {
                        break 'outer;
                    }
                    L::Stop
                }
                L::Stop => unreachable!(),
            };
            if matches!(st, L::Stop) {
                break;
            }
        }
    }
    pat[n] = 0;

    while n >= 1 {
        pat[n] = 0;
        let range = match n {
            2 => 0..1,
            3 => 1..2,
            _ => 0..0,
        };
        for idx in range {
            let want: &[u8] = if idx == 0 { b"VS" } else { b"VOV" };
            if &pat[..n] != want {
                continue;
            }
            let last = &e[n - 1];
            let mut ret = last.end - off0;
            if last.flags & 2 == 2 {
                ret += 1;
            }
            if last.flags & 8 == 8 {
                ret += 1;
            }
            let mut next: [Tok; 2] = [Tok::default(), Tok::default()];
            let mut glued = lb;
            let mut gender = 0;
            if idx == 1 && !lb {
                let (g, gd) = follows_operator(fe, t, p0 + ret as usize, off0 + ret, &mut next);
                gender = gd;
                if g {
                    glued = true;
                }
            }
            if !validate(&mut e[..3], lb_len, idx, glued, &next, gender) {
                return -1;
            }
            return if speak(wa, &e[..n]) { ret } else { 0 };
        }
        n -= 1;
    }
    -1
}
