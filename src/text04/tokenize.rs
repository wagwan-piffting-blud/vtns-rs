//! `fe_Tokenize` (0x1004b8f0): cut the next token from the processed text.
//!
//! Token types: 1 word, 2 number, 3 symbol, 4 `<\xa1\xddX>` tag marker, 5 its close, 6 other high byte,
//! 7 sentence punctuation, 8 paragraph break (two newlines, or a run of `!?;.`), 9 end of text.

use super::alias::contraction;
use super::mem::*;
use super::userdict::UserDict;

/// Token record (`int[14]`, 0x38 bytes).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tok {
    /// [0] type.
    pub ty: i32,
    /// [1] set by the consumed-token list (`FUN_1004d180`'s flag).
    pub f1: i32,
    /// [2] blanks and skipped characters before the token.
    pub ws: i32,
    /// [3] start offset from the sentence base.
    pub start: i32,
    /// [4] `start + strlen(text)`.
    pub end: i32,
    /// [5] `strlen(text)`.
    pub len: i32,
    /// [6..] NUL-terminated text.
    pub text: [u8; 32],
}

impl Tok {
    pub fn text(&self) -> &[u8] {
        cs(&self.text)
    }

    pub fn set_text(&mut self, t: &[u8]) {
        let t = cs(t);
        let n = t.len().min(31);
        self.text = [0; 32];
        self.text[..n].copy_from_slice(&t[..n]);
    }

    /// Text set, then `len = strlen`, `end = start + len`.
    pub fn finish(&mut self, ty: i32, t: &[u8]) {
        self.ty = ty;
        self.set_text(t);
        self.len = self.text().len() as i32;
        self.end = self.start + self.len;
    }

    pub fn c0(&self) -> u8 {
        self.text[0]
    }
}

/// `FUN_10050960`: the phone count of a letter's spelling (`DAT_10065a30`, 10-byte records).
pub fn letter_phones(c: u8) -> i32 {
    let k = cls(c);
    if k & 0xc0 == 0 {
        return 0;
    }
    let i = if k & 0x80 != 0 { c as i8 as i32 - 0x41 } else { c as i8 as i32 - 0x61 };
    let m = b(0x10065a30, 260);
    i16::from_le_bytes([m[(i * 10) as usize], m[(i * 10) as usize + 1]]) as i32
}

/// `FUN_100500d0`: an EUC-KR symbol/fullwidth pair.
pub fn dbcs(t: &[u8], p: usize) -> bool {
    let (c0, c1) = (at(t, p as isize), at(t, p as isize + 1));
    if c0 == 0 || c1 == 0 {
        return false;
    }
    ((0xa0 < c0 && c0 < 0xae) && 0xa0 < c1 && c1 != 0xff) || (c0 == 0xae && 0xa0 < c1 && c1 < 0xc3)
}

/// `FUN_10050120`: a fullwidth letter.
fn fw_letter(t: &[u8], p: usize) -> bool {
    let c1 = at(t, p as isize + 1);
    at(t, p as isize) == 0xa3 && c1 != 0 && ((0xc0 < c1 && c1 < 0xdb) || (0xe0 < c1 && c1 < 0xfb))
}

/// `FUN_10050160`: a fullwidth digit.
fn fw_digit(t: &[u8], p: usize) -> bool {
    let c1 = at(t, p as isize + 1);
    at(t, p as isize) == 0xa3 && c1 != 0 && 0xaf < c1 && c1 < 0xba
}

/// The CP1252 high bytes `DAT_10084628` marks: a symbol token of the given replacement (`None`: the
/// byte itself, type 6).
fn high(c: u8) -> Option<&'static [u8]> {
    Some(match c {
        0x80 => s(0x100857b8),
        0x91 | 0x92 => s(0x10085790),
        0x99 => s(0x100857b0),
        0xa2 => s(0x100857c4),
        0xa3 => s(0x100857c0),
        0xa5 => s(0x100857bc),
        0xa7 => s(0x100857ac),
        0xad => s(0x1006f9d0),
        0xae => s(0x100857a8),
        0xb0 => s(0x1006f4ac),
        0xb1 => s(0x100857a4),
        0xbc => s(0x100857a0),
        0xbd => s(0x1008579c),
        0xbe => s(0x10085798),
        0xf7 => s(0x10085794),
        _ => return None,
    })
}

fn marked_high(c: u8) -> bool {
    let m = b(0x10084628, 512);
    c > 0x7f && i16::from_le_bytes([m[2 * c as usize], m[2 * c as usize + 1]]) != 0
}

/// Two bytes before a leading-dot number that make it a number (`DAT_100857b4..c8`).
fn before_dot_number(t: &[u8], p: usize) -> bool {
    let pre = [at(t, p as isize - 2), at(t, p as isize - 1)];
    [s(0x100857c8), s(0x100857c4), s(0x100857c0), s(0x100857bc), s(0x100857b8), s(0x100857b4)].iter().any(|x| strncmp(x, &pre, 2) == 0)
}

/// Prefixes that keep `X-a-`/`X-per-` hyphenation apart (`0x1006fc10`).
const HYPH_A: Rec30 = rec30(0x1006fc10, 0x10084828);

/// `fe_Tokenize`: the token at `*cur`; `base` is the sentence start the offsets count from.
pub fn tokenize(tok: &mut Tok, t: &[u8], cur: &mut usize, base: usize, ud: Option<&UserDict>) {
    *tok = Tok::default();
    let mut nl = 0;
    // LAB_1004b916: leading blanks.
    loop {
        let c = at(t, *cur as isize);
        match c {
            b'\n' => nl += 1,
            b' ' | b'\t' | b'\r' => {}
            _ => break,
        }
        *cur += 1;
        tok.start += 1;
        tok.ws += 1;
    }
    if nl > 1 {
        tok.start = (*cur - base) as i32;
        tok.finish(8, b"");
        return;
    }
    loop {
        tok.start = (*cur - base) as i32;
        let c = at(t, *cur as isize);
        if c == 0 {
            tok.finish(9, b"");
            return;
        }
        if matches!(c, b' ' | b'\t' | b'\n' | b'\r') {
            loop {
                let c = at(t, *cur as isize);
                match c {
                    b'\n' => nl += 1,
                    b' ' | b'\t' | b'\r' => {}
                    _ => break,
                }
                *cur += 1;
                tok.start += 1;
                tok.ws += 1;
            }
            if nl > 1 {
                tok.start = (*cur - base) as i32;
                tok.finish(8, b"");
                return;
            }
            if at(t, *cur as isize) == 0 {
                tok.finish(9, b"");
                return;
            }
            tok.start = (*cur - base) as i32;
        }
        let c = at(t, *cur as isize);
        let off = tok.start;
        if is_alpha(c) {
            word(tok, t, cur, base, ud);
            return;
        }
        if is_digit(c) {
            number(tok, t, cur, base);
            return;
        }
        if c == b'.' && is_digit(at(t, *cur as isize + 1)) {
            if off == 0 || (off > 0 && !is_alpha(at(t, *cur as isize - 1))) || (off > 1 && before_dot_number(t, *cur)) {
                dot_number(tok, t, cur, base);
                return;
            }
            *cur += 1;
            tok.finish(7, b".");
            return;
        }
        if matches!(c, b'!' | b'?' | b';') {
            let mut buf = vec![c];
            *cur += 1;
            while buf.len() <= 0x1c && matches!(at(t, *cur as isize), b'!' | b'?' | b';' | b'.') {
                buf.push(at(t, *cur as isize));
                *cur += 1;
            }
            tok.finish(if buf.len() > 1 { 8 } else { 7 }, &buf);
            return;
        }
        if c == b'.' {
            let mut buf = vec![c];
            *cur += 1;
            while buf.len() <= 0x1c && at(t, *cur as isize) == at(t, *cur as isize - 1) {
                buf.push(at(t, *cur as isize));
                *cur += 1;
            }
            tok.finish(if buf.len() > 1 { 3 } else { 7 }, &buf);
            return;
        }
        let rest = cs(&t[(*cur).min(t.len())..]);
        let mk = s(0x1006fa20);
        if rest.len() > 4 && c == b'<' && strncmp(&rest[1..], mk, 2) == 0 && (rest[3] as i8) > 0 && rest[3] < b'p' && rest[4] == b'>' {
            let x = rest[3];
            *cur += 5;
            tok.finish(4, &[x]);
            return;
        }
        if rest.len() > 4 && c == b'<' && rest[1] == b'/' && strncmp(&rest[2..], mk, 2) == 0 && rest[4] == b'>' {
            *cur += 5;
            tok.finish(5, b"");
            return;
        }
        if marked_high(c) {
            let (ty, txt) = match high(c) {
                Some(r) => (3, r.to_vec()),
                None => (6, vec![c]),
            };
            tok.ty = ty;
            tok.set_text(&txt);
            tok.len = tok.text().len() as i32;
            tok.end = tok.start + 1;
            *cur += 1;
            return;
        }
        if dbcs(t, *cur) {
            let (ty, step): (i32, fn(&[u8], usize) -> bool) = if fw_letter(t, *cur) {
                (1, fw_letter)
            } else if fw_digit(t, *cur) {
                (2, fw_digit)
            } else {
                let txt = [c, at(t, *cur as isize + 1)];
                *cur += 2;
                tok.finish(3, &txt);
                dbcs_symbol(tok);
                return;
            };
            let mut buf = vec![c, at(t, *cur as isize + 1)];
            *cur += 2;
            while buf.len() <= 0x1b && step(t, *cur) {
                buf.push(at(t, *cur as isize));
                buf.push(at(t, *cur as isize + 1));
                *cur += 2;
            }
            tok.finish(ty, &buf);
            dbcs_symbol(tok);
            return;
        }
        if s(0x100857d0).contains(&c) {
            let mut buf = vec![c];
            *cur += 1;
            while buf.len() <= 0x1c && at(t, *cur as isize) == at(t, *cur as isize - 1) {
                buf.push(at(t, *cur as isize));
                *cur += 1;
            }
            tok.finish(3, &buf);
            if tok.text() == s(0x100857cc) {
                tok.text = [0; 32];
                tok.text[0] = b'"';
            }
            return;
        }
        // Anything else is skipped (a high byte with its successor).
        if (c as i8) < 1 && at(t, *cur as isize + 1) != 0 {
            *cur += 2;
            tok.ws += 2;
        } else {
            *cur += 1;
            tok.ws += 1;
        }
    }
}

/// Letters (at most 0x40 phones and 0x1d bytes), then a `.letter` run, a hyphenated run or a
/// contraction.
fn word(tok: &mut Tok, t: &[u8], cur: &mut usize, base: usize, ud: Option<&UserDict>) {
    let mut buf: Vec<u8> = Vec::new();
    let mut ph = letter_phones(at(t, *cur as isize)) + 1;
    loop {
        if ph > 0x40 || buf.len() > 0x1c {
            tok.finish(1, &buf);
            return;
        }
        buf.push(at(t, *cur as isize));
        *cur += 1;
        let nc = at(t, *cur as isize);
        if is_alpha(nc) {
            ph += letter_phones(nc) + 1;
            continue;
        }
        if nc == b'.' && is_alpha(at(t, *cur as isize + 1)) {
            dotted(tok, t, cur, base, buf, ph + 1);
            return;
        }
        if nc == b'\'' {
            if let Some(suf) = contraction(t, *cur, at(t, *cur as isize - 1)) {
                let p2 = ph + suf.iter().map(|&c| letter_phones(c) + 1).sum::<i32>();
                if p2 < 0x41 && buf.len() + suf.len() < 0x1d {
                    buf.extend_from_slice(&suf);
                    *cur += suf.len();
                }
                tok.finish(1, &buf);
                return;
            }
        }
        if nc == b'-' && is_alpha(at(t, *cur as isize + 1)) {
            hyphen(tok, t, cur, base, buf, ph + 1, ud);
            return;
        }
        tok.finish(1, &buf);
        return;
    }
}

fn dotted(tok: &mut Tok, t: &[u8], cur: &mut usize, base: usize, mut buf: Vec<u8>, mut ph: i32) {
    loop {
        if ph > 0x40 || buf.len() > 0x1c {
            if buf.contains(&b'.') {
                while base < *cur && at(t, *cur as isize) != b'.' {
                    buf.pop();
                    *cur -= 1;
                }
            }
            tok.finish(1, &buf);
            return;
        }
        buf.push(at(t, *cur as isize));
        *cur += 1;
        let nc = at(t, *cur as isize);
        if !is_alpha(nc) && !(nc == b'.' && is_alpha(at(t, *cur as isize + 1))) {
            tok.finish(1, &buf);
            return;
        }
        ph += letter_phones(nc) + 1;
    }
}

fn hyphen(tok: &mut Tok, t: &[u8], cur: &mut usize, base: usize, mut buf: Vec<u8>, mut ph: i32, ud: Option<&UserDict>) {
    loop {
        if ph > 0x40 || buf.len() > 0x1c {
            if buf.contains(&b'-') {
                while base < *cur && at(t, *cur as isize) != b'-' {
                    buf.pop();
                    *cur -= 1;
                }
            }
            tok.finish(1, &buf);
            return;
        }
        let mut stop = false;
        if at(t, *cur as isize) == b'-' {
            let mut p2 = *cur + 1;
            let mut scratch = Tok::default();
            let hit = ud.map_or(-1, |u| u.match_at(t, &mut p2, base, &mut scratch, 0));
            if hit >= 0 {
                stop = true;
            } else {
                let a = at(t, *cur as isize + 1) == b'a' && at(t, *cur as isize + 2) == b'-';
                let per = strncmp(&t[*cur + 1..], b"per", 3) == 0 && at(t, *cur as isize + 4) == b'-';
                if (a || per) && HYPH_A.find(&buf, true) >= 0 {
                    stop = true;
                }
            }
        }
        if !stop {
            buf.push(at(t, *cur as isize));
            *cur += 1;
            let nc = at(t, *cur as isize);
            if is_alpha(nc) || (nc == b'-' && is_alpha(at(t, *cur as isize + 1))) {
                ph += letter_phones(nc) + 1;
                continue;
            }
        }
        // LAB_1004c24e
        if at(t, *cur as isize) == b'\'' {
            if let Some(suf) = contraction(t, *cur, at(t, *cur as isize - 1)) {
                if buf.len() + suf.len() < 0x1e {
                    buf.extend_from_slice(&suf);
                    *cur += suf.len();
                }
            }
        }
        tok.finish(1, &buf);
        return;
    }
}

/// `last` digit plus the two letters at `p`: an ordinal suffix.
fn ordinal(t: &[u8], p: usize, buf: &[u8]) -> bool {
    let d = *buf.last().unwrap();
    let r = &t[p.min(t.len())..];
    (d == b'1' && strnicmp(r, b"st", 2) == 0)
        || (d == b'2' && strnicmp(r, b"nd", 2) == 0)
        || (d == b'3' && strnicmp(r, b"rd", 2) == 0)
        || ((d == b'0' || (b'4'..=b'9').contains(&d)) && strnicmp(r, b"th", 2) == 0)
        || (buf.len() >= 2 && strnicmp(r, b"th", 2) == 0)
}

/// Digits with `,ddd` groups, a decimal part (`decimal`) or an ordinal suffix; at most 0x1d bytes.
fn number(tok: &mut Tok, t: &[u8], cur: &mut usize, base: usize) {
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let p = *cur;
        if at(t, p as isize) == b',' && buf.first() == Some(&b'0') {
            break;
        }
        buf.push(at(t, p as isize));
        *cur = p + 1;
        let nc = at(t, *cur as isize);
        if !is_digit(nc) {
            let group = nc == b','
                && cs(&t[*cur..]).len() > 3
                && is_digit(at(t, p as isize + 2))
                && is_digit(at(t, p as isize + 3))
                && is_digit(at(t, p as isize + 4))
                && !is_digit(at(t, p as isize + 5));
            if !group {
                if nc == b'.' && is_digit(at(t, *cur as isize + 1)) {
                    decimal(tok, t, cur, base, buf);
                    return;
                }
                if buf.len() <= 0x1a && ordinal(t, *cur, &buf) {
                    buf.push(at(t, *cur as isize));
                    buf.push(at(t, *cur as isize + 1));
                    *cur += 2;
                }
                break;
            }
        }
        if buf.len() >= 0x1d {
            if buf.contains(&b',') {
                while base < *cur && at(t, *cur as isize) != b',' {
                    buf.pop();
                    *cur -= 1;
                }
            }
            break;
        }
    }
    tok.finish(2, &buf);
}

fn decimal(tok: &mut Tok, t: &[u8], cur: &mut usize, base: usize, mut buf: Vec<u8>) {
    loop {
        if buf.len() > 0x1c {
            if buf.contains(&b',') || buf.contains(&b'.') {
                while base < *cur && !matches!(at(t, *cur as isize), b',' | b'.') {
                    buf.pop();
                    *cur -= 1;
                }
            }
            if buf.is_empty() {
                // LAB_1004c461 with nothing left: the engine takes 0x1d bytes of stale buffer.
                *cur += 0x1d;
            }
            break;
        }
        buf.push(at(t, *cur as isize));
        *cur += 1;
        let nc = at(t, *cur as isize);
        if !is_digit(nc) && !(nc == b'.' && is_digit(at(t, *cur as isize + 1))) {
            break;
        }
    }
    tok.finish(2, &buf);
}

/// `.5`-style numbers.
fn dot_number(tok: &mut Tok, t: &[u8], cur: &mut usize, _base: usize) {
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.push(at(t, *cur as isize));
        *cur += 1;
        let nc = at(t, *cur as isize);
        if !is_digit(nc) && !(nc == b'.' && is_digit(at(t, *cur as isize + 1))) {
            break;
        }
        if buf.len() >= 0x1d {
            if buf.contains(&b'.') {
                while at(t, *cur as isize) != b'.' {
                    buf.pop();
                    *cur -= 1;
                }
            }
            if buf.is_empty() {
                *cur += 0x1d;
            }
            break;
        }
    }
    tok.finish(2, &buf);
}

/// `FUN_100501a0` on an EUC-KR token: fullwidth letters and digits to ASCII (`trail + 0x80`); symbols
/// keep their two bytes here (the symbol table is applied by the normalizers that see them).
fn dbcs_symbol(tok: &mut Tok) {
    let txt = tok.text().to_vec();
    if txt.len() < 2 || !dbcs(&txt, 0) {
        return;
    }
    if tok.ty == 1 || tok.ty == 2 {
        let v: Vec<u8> = txt.chunks(2).map(|p| at(p, 1).wrapping_add(0x80)).collect();
        tok.set_text(&v);
        tok.len = tok.text().len() as i32;
    }
}
