//! julie09 math expressions (`FUN_10037c80`; the 2014 `FUN_1003aad0` is never reached in the
//! captures and stays a stub in `norm::math`). Reads up to four operands/operators
//! (`x >= 3`, `2 * 3`, `x^2`-style superscripts, brackets) into a small item table and reads
//! them back as words of class 0x1d. Helpers: `FUN_10038fc0` gate, `FUN_100390d0`,
//! `FUN_10039130`, `FUN_10038ef0`, `FUN_10038b40` validation, `FUN_100386d0` emission,
//! `FUN_10039190` variable emission.

use super::cstr::*;
use super::numwords::{new_buf, number};
use super::sbd::{all_alpha, all_digit};
use super::tabs::search;
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::Fe;

/// `0x10091200`: key (EUC-KR superscript) and text of the exponent words.
const EXPONENTS: [(&[u8], &[u8]); 7] = [
    (b"\xa9\xf6", b"to the first"),
    (b"\xa9\xf7", b"squared"),
    (b"\xa9\xf8", b"cubed"),
    (b"\xa9\xf9", b"to the fourth"),
    (b"\xa9\xfa", b"to the N TH"),
    (b"\xb2\x00", b"squared"),
    (b"\xb3\x00", b"cubed"),
];

fn exponent(w: &[u8]) -> i32 {
    search(7, true, w, |i| EXPONENTS[i as usize].0)
}

/// One 0x140-byte table slot (`local_10 + 8 + k * 0x140`).
#[derive(Clone, Default)]
struct Item {
    ws: i32,
    start: i32,
    end: i32,
    len: i32,
    class: u8,
    ty: u8,
    flags: u8,
    exp: i16,
    eq: i16,
    text: Vec<u8>,
    words: Vec<u8>,
}

impl Item {
    fn t0(&self) -> u8 {
        self.text.first().copied().unwrap_or(0)
    }
}

fn from_tok(t: &Tok) -> Item {
    Item { ws: t.ws, start: t.start, end: t.end, len: t.len, class: t.class as u8, text: cs(&t.text).to_vec(), ..Default::default() }
}

fn ws4(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r')
}

/// `FUN_1005ebf0` with `param_4 == 1`: the last blank-delimited word before `cur`
/// (at most 28 characters) and the number of bytes it and the blanks after it span.
fn prev_word(t: &[u8], start: usize, cur: usize) -> Option<(Vec<u8>, i32)> {
    if cur == start {
        return None;
    }
    let mut pt = cur as isize;
    let mut skipped = 0;
    loop {
        pt -= 1;
        if !ws4(t[pt as usize]) {
            break;
        }
        if pt <= start as isize {
            return None;
        }
        skipped += 1;
    }
    let mut count = 1isize;
    loop {
        if pt <= start as isize {
            break;
        }
        let c = t[pt as usize];
        if ws4(c) {
            pt += 1;
            count -= 1;
            break;
        }
        if 29 <= count {
            pt += 1;
            count -= 1;
            break;
        }
        count += 1;
        pt -= 1;
    }
    let w = t[pt as usize..pt as usize + count as usize].to_vec();
    Some((w, count as i32 + skipped))
}

/// `FUN_10038fc0`: the token before the expression. `Some((item, is_v))`; the item is filled
/// whenever that token has a length, `is_v` only when it is the end of a class-0x1d record
/// (the engine returns true only then).
fn gate(fe: &Fe, wa: &Wa, t: &[u8], p: usize, off: i32) -> Option<(Item, bool)> {
    let (word, cnt) = prev_word(t, p - off as usize, p)?;
    let mut buf = word.clone();
    buf.extend_from_slice(&[0u8; 64]);
    let mut tk = Tok::default();
    let (mut pos, mut o) = (0usize, off - cnt);
    while pos < word.len() {
        let c = tokenize(&mut tk, &buf, pos, o, 0x13, fe.ud.as_ref());
        if c <= 0 {
            break;
        }
        pos += c as usize;
        o += c;
    }
    if tk.len <= 0 {
        return None;
    }
    let n = wa.n() as i32;
    let mut it = from_tok(&tk);
    if n > 0 && wa.ri32(n - 1, 0x28) == 0x1d && wa.ri32(n - 1, 4) == tk.end && off - tk.end < 2 {
        it.len = -1;
        it.ty = b'V';
        return Some((it, true));
    }
    Some((it, false))
}

/// `FUN_100390d0`: an `x` followed by digits.
fn x_digits(w: &[u8]) -> bool {
    let w = cs(w);
    !w.is_empty() && upper(w[0]) == 0x58 && w[1..].iter().all(|&c| cls(c) & 0x10 != 0)
}

/// `FUN_10039130`: up to five characters out of `abcxyz` and digits.
fn var_name(w: &[u8]) -> bool {
    let w = cs(w);
    !w.is_empty() && w.len() < 6 && w.iter().all(|&c| (c != 0 && b"abcxyz".contains(&c)) || cls(c) & 0x10 != 0)
}

/// `FUN_1005d300`: an integer or decimal with well-formed thousands commas.
fn grouped(w: &[u8]) -> bool {
    let w = cs(w);
    if w.is_empty() {
        return false;
    }
    let (mut ip, mut fp) = (Vec::new(), Vec::new());
    let mut dot = false;
    for (i, &c) in w.iter().enumerate() {
        if cls(c) & 0x10 == 0 && c != b',' && c != b'.' {
            return false;
        }
        if dot {
            if c == b',' || c == b'.' {
                return false;
            }
            fp.push(c);
        } else if c == b'.' {
            dot = true;
        } else {
            if i == 0 && w[0] == b',' {
                return false;
            }
            ip.push(c);
        }
    }
    if dot && fp.is_empty() {
        return false;
    }
    if ip.len() > 1 && ip[0] == b'0' {
        return false;
    }
    let mut run = 0;
    for &c in ip.iter().rev() {
        if c == b',' {
            if run != 3 {
                return false;
            }
            run = 0;
        } else {
            run += 1;
        }
    }
    true
}

/// `FUN_10038ef0`: the next two tokens after the expression look like `+5` with nothing between.
fn followed_by_op(fe: &Fe, t: &[u8], p: usize, off: i32, used: i32) -> (bool, Tok) {
    let (mut a, mut b) = (Tok::default(), Tok::default());
    let (mut pos, mut o) = (p + used as usize, off + used);
    for i in 0..2 {
        let mut x = Tok::default();
        let c = tokenize(&mut x, t, pos, o, 0x13, fe.ud.as_ref());
        if c == 0 {
            break;
        }
        let cl = x.class;
        if i == 0 {
            a = x;
        } else {
            b = x;
        }
        if cl == 8 || cl == 9 {
            break;
        }
        pos += c as usize;
        o += c;
    }
    if b.len > 0 && b.class == 2 && b.ws == 0 && a.len == 1 {
        let c = a.text[0];
        let r = c != 0 && b"+*=/-".contains(&c) && a.ws == 0;
        return (r, a);
    }
    (false, a)
}

fn digits_item(i: &Item) -> bool {
    i.class == 2 && all_digit(&i.text)
}

/// `FUN_10038b40`: reject expressions that read better as something else (dates, ranges, words).
/// `s` is the token before the expression, `flag` the engine's `(short)uVar5`, `next` the first
/// token after it.
fn check(it: &mut [Item; 5], s: &Item, pidx: i32, flag: bool, next: &Tok) -> bool {
    if pidx == 0 {
        if it[0].exp < 2 {
            return false;
        }
        if it[0].len > 0 && it[0].ty == b'N' {
            let mut buf = new_buf();
            number(&mut buf, &it[0].text, 0);
            it[0].words = cs(&buf).to_vec();
        }
        return true;
    }
    if pidx != 1 {
        return false;
    }
    let l0 = it[0].len;
    let (ws1, ws2) = (it[1].ws, it[2].ws);
    if (l0 == -1 && it[0].ws != ws1) || ws1 != ws2 {
        return false;
    }
    if l0 != -1 && it[0].exp == 0 && it[2].exp == 0 && it[1].len == 1 {
        if upper(it[1].t0()) == 0x58 {
            if 0 < l0 && (1 < ws1 || 1 < ws2) {
                return false;
            }
        } else if 0 < l0 && ws1 != 0 {
            return false;
        }
        if !flag {
            if s.len < 1 || s.class != 3 {
                return false;
            }
            let c = s.t0();
            if c == 0 || !b"+*=/-".contains(&c) || it[0].ws != it[1].ws {
                return false;
            }
        } else if 0 < l0 && ws2 != next.ws {
            return false;
        }
        if 0 < it[0].len && digits_item(&it[0]) && it[1].t0() != 0 && b"/-".contains(&it[1].t0()) && 0 < it[2].len && digits_item(&it[2]) {
            if !flag {
                if 0 < s.len && s.t0() == it[1].t0() {
                    return false;
                }
            } else if 0 < it[0].len && next.text[0] == it[1].t0() {
                return false;
            }
        }
        if it[0].class == 1
            && it[2].class == 1
            && ((all_alpha(&it[0].text) && s.len >= 0) || it[0].len > 3 || (all_alpha(&it[2].text) && s.len >= 0) || 3 < it[2].len)
        {
            return false;
        }
        if (it[0].class == 1 && all_alpha(&it[0].text) && s.len >= 0 && 3 < it[0].len) || (it[2].class == 1 && all_alpha(&it[2].text) && s.len >= 0 && 3 < it[2].len) {
            return false;
        }
        if 0 < it[0].len {
            if upper(it[1].t0()) == 0x58 && !(digits_item(&it[0]) && digits_item(&it[2])) {
                return false;
            }
            if 0 < it[0].len && it[1].t0() == b'-' && (!flag || !digits_item(&it[0]) || !digits_item(&it[2])) {
                return false;
            }
        }
    }
    if 0 < it[0].len && it[0].ty == b'N' {
        let mut buf = new_buf();
        number(&mut buf, &it[0].text, 0);
        it[0].words = cs(&buf).to_vec();
    }
    if 0 < it[2].len && it[2].ty == b'N' {
        let mut buf = new_buf();
        number(&mut buf, &it[2].text, 0);
        it[2].words = cs(&buf).to_vec();
    }
    true
}

/// `FUN_10039190`: a variable as typed: letters spelled, digits as numbers, superscripts by name.
fn emit_var(fe: &Fe, wa: &mut Wa, text: &[u8], start: i32, end: i32) -> i32 {
    let n = strlen(text) as i32;
    if n < 1 {
        return n;
    }
    let mut buf = cs(text).to_vec();
    buf.extend_from_slice(&[0u8; 64]);
    let (mut pos, mut o) = (0usize, start);
    loop {
        let mut x = Tok::default();
        let c = tokenize(&mut x, &buf, pos, o, 0, fe.ud.as_ref());
        let w = x.text();
        let ok = if x.class == 1 && all_alpha(w) {
            wa.emit(w, start, end, 0x41, 0x53, 0x1d)
        } else if x.class == 2 && super::numwords::numeric(w) {
            let mut b = new_buf();
            number(&mut b, w, 0);
            wa.emit_split(&b, start, end, 0x44, 0x44, 0x1d)
        } else {
            if x.class != 3 {
                return -1;
            }
            let i = exponent(w);
            if i < 0 {
                return -1;
            }
            wa.emit_split(EXPONENTS[i as usize].1, start, end, 0x44, 0x44, 0x1d)
        };
        if !ok {
            return 0;
        }
        if c <= 0 {
            return n;
        }
        pos += c as usize;
        o += c;
        if pos as i32 >= n {
            return n;
        }
    }
}

/// `FUN_100386d0`: the items as words.
fn emit_items(fe: &Fe, wa: &mut Wa, it: &[Item; 5], count: usize) -> bool {
    let sp = |wa: &mut Wa, w: &[u8], a: i32, b: i32| wa.emit_split(w, a, b, 0x53, 0x44, 0x1d);
    for i in 0..count {
        let x = &it[i];
        if x.len <= 0 {
            continue;
        }
        let f = x.flags;
        if f & 4 != 0 {
            let mut at_ = x.start;
            if f & 1 != 0 {
                if !sp(wa, b"opening curly bracket", at_ - 2, at_ - 1) {
                    return false;
                }
                at_ = x.start;
                if !sp(wa, b"opening round bracket", at_ - 1, at_) {
                    return false;
                }
            } else if !sp(wa, b"opening curly bracket", at_ - 1, at_) {
                return false;
            }
        } else if f & 1 != 0 && !sp(wa, b"opening round bracket", x.start - 1, x.start) {
            return false;
        }
        if x.ty == b'O' {
            let eq = x.eq != 0;
            let w: &[u8] = match x.t0() {
                b'*' | b'X' | b'x' => b"times",
                b'+' => b"plus",
                b'-' => b"minus",
                b'/' => b"over",
                b'<' => if eq { b"less than or equal to" } else { b"less than" },
                b'=' => if eq { b"equal to" } else { b"equals" },
                b'>' => if eq { b"greater than or equal to" } else { b"greater than" },
                _ => return false,
            };
            if !sp(wa, w, x.start, x.end) {
                return false;
            }
        } else {
            if x.ty == b'N' {
                if !wa.emit_split(&x.words, x.start, x.end, 0x44, 0x44, 0x1d) {
                    return false;
                }
            } else if emit_var(fe, wa, &x.text, x.start, x.end) == 0 {
                return false;
            }
            if 1 < x.exp && !wa.emit_split(EXPONENTS[(x.exp - 2) as usize].1, x.start, x.end, 0x44, 0x44, 0x1d) {
                return false;
            }
        }
        let f = x.flags;
        if f & 2 != 0 {
            if !sp(wa, b"closing round bracket", x.end, x.end + 1) {
                return false;
            }
            if x.flags & 8 != 0 && !sp(wa, b"closing curly bracket", x.end + 1, x.end + 2) {
                return false;
            }
        } else if f & 8 != 0 && !sp(wa, b"closing curly bracket", x.end, x.end + 1) {
            return false;
        }
    }
    true
}

#[derive(Clone, Copy, PartialEq)]
enum L {
    Dd0,
    E1b,
    A380fa,
    A118,
    A178,
    A1b8,
    A1cb,
    A285,
    Operand,
    Store,
}

fn is_op(c: u8, set: &[u8]) -> bool {
    c != 0 && set.contains(&c)
}

/// julie09 `FUN_10037c80`: `-1` not mine, `0` stop the sentence, else the bytes consumed.
pub fn math09(fe: &Fe, wa: &mut Wa, t: &[u8], p0: usize, off0: i32) -> i32 {
    let tk = |p: usize, off: i32, mode: i32| -> (Tok, i32) {
        let mut x = Tok::default();
        let c = tokenize(&mut x, t, p, off, mode, fe.ud.as_ref());
        (x, c)
    };
    let mut items: [Item; 5] = Default::default();
    let mut ty = vec![0u8; 64];
    let mut patt = vec![0u8; 8];
    let (mut p, mut off) = (p0, off0);
    let (mut k, mut n) = (0usize, 0usize);
    let (mut open_p, mut open_c) = (-1i32, -1i32);
    let (mut depth_p, mut depth_c) = (0i32, 0i32);
    let mut flag98 = false;
    let mut gate_v = false;
    let mut s = Item::default();
    if let Some((g, v)) = gate(fe, wa, t, p, off) {
        s = g;
        if v {
            gate_v = true;
            k = 1;
            ty[1] = b'V';
            patt[0] = b'V';
            items[0] = s.clone();
            n = 1;
        }
    }
    'outer: loop {
        let (mut x, mut c) = tk(p, off, if flag98 { 0 } else { 0x13 });
        if c == 0 || x.class == 9 {
            break;
        }
        flag98 = false;
        let mut lbl = if !gate_v || k != 1 {
            if k > 0 {
                if ty[k] == b'N' {
                    L::Dd0
                } else {
                    L::E1b
                }
            } else {
                L::A380fa
            }
        } else {
            L::Dd0
        };
        loop {
            let (cl, t0, len, ws) = (x.class, x.text[0], x.len, x.ws);
            let ck = ty[k];
            let nvbp = matches!(ck, b'N' | b'V' | b'{' | b'(');
            match lbl {
                L::Dd0 => {
                    if x_digits(&x.text) {
                        let (x2, c2) = tk(p, off, 0);
                        if c2 == 0 || x2.class == 9 {
                            break 'outer;
                        }
                        x = x2;
                        c = c2;
                        flag98 = true;
                    }
                    lbl = L::E1b;
                }
                L::E1b => {
                    if k < 1 {
                        lbl = L::A380fa;
                        continue;
                    }
                    let plain_op = nvbp && cl == 3 && is_op(t0, b"+-*/") && len == 1 && ws < 2;
                    if plain_op {
                        ty[k + 1] = b'O';
                        patt[n] = b'O';
                        lbl = L::Store;
                        continue;
                    }
                    if nvbp && cl == 3 {
                        let m = strcmp(&x.text, b"\xa1\xbf") == 0;
                        if ((m && len == 2) || ((t0 == 0xb7 || t0 == 0xd7) && len == 1)) && ws < 2 {
                            ty[k + 1] = b'O';
                            x.text[0] = b'*';
                            x.text[1] = 0;
                            patt[n] = b'O';
                            lbl = L::Store;
                            continue;
                        }
                    }
                    if nvbp && cl == 3 {
                        let m = strcmp(&x.text, b"\xa1\xc0") == 0;
                        if ((m && len == 2) || (t0 == 0xf7 && len == 1)) && ws < 2 {
                            ty[k + 1] = b'O';
                            x.text[0] = b'/';
                            x.text[1] = 0;
                            patt[n] = b'O';
                            lbl = L::Store;
                            continue;
                        }
                    }
                    let t0 = x.text[0];
                    if (nvbp && cl == 1 && is_op(t0, b"xX") && len == 1 && ws < 2) || ((ck == b'N' || ck == b'V') && cl == 3 && is_op(t0, b"<>=") && len == 1 && ws < 2) {
                        ty[k + 1] = b'O';
                        patt[n] = b'O';
                        lbl = L::Store;
                        continue;
                    }
                    let pt0 = if n > 0 { items[n - 1].t0() } else { 0 };
                    let combine = ck == b'O' && is_op(pt0, b"<>=") && n > 0 && items[n - 1].eq == 0 && cl == 3 && t0 == b'=' && len == 1 && ws == 0;
                    if combine {
                        p += c as usize;
                        off += c;
                        items[n - 1].end += 1;
                        items[n - 1].len += 1;
                        ty[k + 1] = b'O';
                        items[n - 1].eq = 1;
                        break;
                    }
                    if nvbp && cl == 3 && t0 == b'=' && len == 1 && ws < 2 {
                        ty[k + 1] = b'O';
                        patt[n] = b'O';
                        lbl = L::Store;
                    } else {
                        lbl = L::A380fa;
                    }
                }
                L::A380fa => {
                    if k > 0 {
                        lbl = if ck == b'O' || ck == b'{' { L::A118 } else { L::A178 };
                    } else {
                        lbl = L::A118;
                    }
                }
                L::A118 => {
                    if cl != 3 || t0 != b'(' || len != 1 || ws > 1 {
                        lbl = L::A178;
                    } else {
                        p += c as usize;
                        off += c;
                        ty[k + 1] = b'(';
                        items[n].flags |= 1;
                        open_p = k as i32;
                        depth_p += 1;
                        break;
                    }
                }
                L::A178 => {
                    if k > 0 && (ck == b'N' || ck == b'V') && cl == 3 && t0 == b')' && len == 1 && ws == 0 {
                        let d = if s.len == 0 { depth_p } else { s.len };
                        if 0 < d {
                            p += c as usize;
                            off += c;
                            depth_p -= 1;
                            ty[k + 1] = b'(';
                            items[n - 1].flags |= 2;
                            break;
                        }
                    }
                    lbl = L::A1b8;
                }
                L::A1b8 => {
                    if k == 0 {
                        lbl = L::A1cb;
                    } else if ck == b'O' {
                        lbl = L::A1cb;
                    } else {
                        lbl = L::A285;
                    }
                }
                L::A1cb => {
                    if cl == 3 && t0 == b'{' && len == 1 && ws < 2 && depth_c == 0 {
                        p += c as usize;
                        off += c;
                        items[n].flags |= 4;
                        ty[k + 1] = b'{';
                        open_c = k as i32;
                        depth_c = 1;
                        break;
                    }
                    lbl = L::A285;
                }
                L::A285 => {
                    if 0 < k {
                        if (ck == b'N' || ck == b'V' || ck == b'(') && cl == 3 && t0 == b'}' && len == 1 && ws == 0 {
                            let d = if s.len == 0 { depth_c } else { s.len };
                            if 0 < d {
                                p += c as usize;
                                off += c;
                                ty[k + 1] = b'{';
                                items[n - 1].flags |= 8;
                                depth_c -= 1;
                                break;
                            }
                        }
                        if (ck == b'N' || ck == b'V') && cl == 3 {
                            if 2 < len || ws != 0 {
                                break 'outer;
                            }
                            let i = exponent(&x.text);
                            if i >= 0 {
                                p += c as usize;
                                off += c;
                                ty[k + 1] = b'S';
                                items[n - 1].end = x.end;
                                items[n - 1].len += x.len;
                                items[n - 1].exp = i as i16 + 2;
                                break;
                            }
                        }
                    }
                    lbl = L::Operand;
                }
                L::Operand => {
                    if cl == 2 && grouped(&x.text) && ws <= 1 {
                        if open_p >= 0 || open_c >= 0 {
                            if ck != b'(' {
                                if ck == b'{' {
                                    open_c = -1;
                                } else {
                                    break 'outer;
                                }
                            } else {
                                open_p = -1;
                                if open_c >= 0 {
                                    open_c = -1;
                                }
                            }
                        }
                        ty[k + 1] = b'N';
                    } else if cl == 2 && !all_digit(&x.text) || cl == 1 {
                        if !var_name(&x.text) || ws > 1 {
                            break 'outer;
                        }
                        if open_p >= 0 || open_c >= 0 {
                            if ck == b'(' {
                                open_p = -1;
                                if open_c >= 0 {
                                    open_c = -1;
                                }
                            } else if ck != b'{' {
                                break 'outer;
                            } else {
                                open_c = -1;
                            }
                        }
                        ty[k + 1] = b'V';
                    } else {
                        break 'outer;
                    }
                    patt[n] = b'V';
                    lbl = L::Store;
                }
                L::Store => {
                    if open_p >= 0 || open_c >= 0 {
                        break 'outer;
                    }
                    let it = &mut items[n];
                    it.ws = x.ws;
                    it.start = x.start;
                    off += c;
                    it.end = off;
                    it.len = x.len;
                    it.class = x.class as u8;
                    it.ty = ty[k + 1];
                    it.text = cs(&x.text).to_vec();
                    p += c as usize;
                    n += 1;
                    break;
                }
            }
        }
        k += 1;
        if n >= 4 {
            break;
        }
    }
    if patt[0] != b'V' || (n == 1 && items[0].exp <= 1) {
        return -1;
    }
    let mut m = n;
    while m > 0 {
        let pat = &patt[..m];
        let pidx = if m == 1 && pat == b"V" {
            Some(0)
        } else if m == 3 && pat == b"VOV" {
            Some(1)
        } else {
            None
        };
        if let Some(pidx) = pidx {
            let last = items[m - 1].flags;
            let mut used = items[m - 1].end - off0;
            if last & 2 != 0 {
                used += 1;
            }
            if last & 8 != 0 {
                used += 1;
            }
            let mut flag = gate_v;
            let mut next = Tok::default();
            if pidx == 1 && !gate_v {
                let (r, a) = followed_by_op(fe, t, p0, off0, used);
                flag = r;
                next = a;
            }
            if !check(&mut items, &s, pidx, flag, &next) {
                return -1;
            }
            return if emit_items(fe, wa, &items, m) { used } else { 0 };
        }
        m -= 1;
    }
    -1
}
