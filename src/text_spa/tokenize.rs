//! `FUN_10047000`: Violeta's `fe_Tokenize`, cutting the next token from the
//! normalizer's input. A trimmed version of the English tokenizer
//! (`text::tokenize`): no CP1252 folding into ASCII, Spanish ordinal suffixes
//! ("1ro", "2da", "3ero", "5º") on numbers, '¿'/'¡' as their own class.
//!
//! `mode` (`param_4`) selects the caller-specific joining rules; the
//! normalizer chain calls it with 0.

use super::tables::{CLS_S, HI, UPPER_S, WIDTH};
use crate::ling_spa::lexicon::{stricmp_spa, strnicmp_spa};
use crate::text::chars::fold_dbcs;
use crate::text::cstr::{at, cs, dbcs_pair, strlen};

/// Token record (`int[0x10]`, 0x40 bytes).
#[derive(Clone, Debug)]
pub struct Tok {
    /// [0] whitespace and skipped characters before and inside the token.
    pub ws: i32,
    /// [1] exactly one newline preceded the token.
    pub nl: i32,
    /// [2] start offset (relative to the caller's `off`).
    pub start: i32,
    /// [3] end offset.
    pub end: i32,
    /// [4] strlen(text).
    pub len: i32,
    /// [5] bytes consumed (the return value).
    pub consumed: i32,
    /// [6] class: 0 word ended by joined whitespace, 1 word, 2 number, 3 symbol,
    /// 4 tag marker, 5 tag close, 6 raw high byte, 7 '¿'/'¡', 8 sentence
    /// punctuation, 9 paragraph break (or a run of sentence punctuation),
    /// 10 end of text.
    pub class: i32,
    /// [7..] NUL-terminated token text (36 bytes).
    pub text: [u8; 36],
}

impl Default for Tok {
    fn default() -> Self {
        Tok { ws: 0, nl: 0, start: 0, end: 0, len: 0, consumed: 0, class: 0, text: [0; 36] }
    }
}

impl Tok {
    pub fn text(&self) -> &[u8] {
        cs(&self.text)
    }
}

/// `DAT_10069d40`.
const SENT_PUNCT: &[u8] = b".?!;";
/// `s_________<>__1007a7dc`.
const DELIM_A: &[u8] = b",:{}[]()<>\"";
/// `s_________<>____1007a7cc`.
const DELIM_B: &[u8] = b",:{}[]()<>\";/";
/// `s_________________________<>___1007a7e8`.
const SYMBOLS: &[u8] = b"~`@#$%&^_|,:'+-*/={}[]()<>\\\"";

fn in_set(set: &[u8], c: u8) -> bool {
    c != 0 && set.contains(&c)
}

fn ws(c: u8) -> bool {
    matches!(c, b' ' | 9 | 10 | 13)
}

fn digit(c: u8) -> bool {
    CLS_S[c as usize] & 0x10 != 0
}

/// Letter test at the top of the token loop (0xfd excluded).
fn alpha(c: u8) -> bool {
    CLS_S[c as usize] & 0xc0 != 0 || (c != 0xfd && c > 0x7f && HI[c as usize] & 0xc0 != 0)
}

/// Letter test for the character after a joiner.
fn alpha2(c: u8) -> bool {
    CLS_S[c as usize] & 0xc0 != 0 || (c > 0x7f && HI[c as usize] & 0xc0 != 0)
}

/// `FUN_10025a90`: accent folding of token letters.
pub fn fold(c: u8) -> u8 {
    match c {
        0x8a | 0xc7 => 0x53,
        0x8e => 0x5a,
        0x9a | 0xe7 => 0x73,
        0x9e => 0x7a,
        0x9f | 0xdb | 0xdd => 0x59,
        0xc0 => 0xc1,
        0xc2..=0xc5 => 0x41,
        0xc8 => 0xc9,
        0xca | 0xcb => 0x45,
        0xcc => 0xcd,
        0xce | 0xcf => 0x49,
        0xd2 => 0xd3,
        0xd4..=0xd6 => 0x4f,
        0xd9 => 0xda,
        0xe0 => 0xe1,
        0xe2..=0xe5 => 0x61,
        0xe8 => 0xe9,
        0xea | 0xeb => 0x65,
        0xec => 0xed,
        0xee | 0xef => 0x69,
        0xf2 => 0xf3,
        0xf4..=0xf6 => 0x6f,
        0xf9 => 0xfa,
        0xfb | 0xfd | 0xff => 0x79,
        _ => c,
    }
}

struct Cut<'a> {
    t: &'a [u8],
    base: usize,
    off: i32,
    tok: Tok,
    buf: [u8; 64],
    n: usize,
    i: usize,
}

impl Cut<'_> {
    fn c(&self, k: usize) -> u8 {
        at(self.t, (self.base + k) as isize)
    }

    /// `FUN_1004b590` / `FUN_1004fca0`: the next `k` characters are all non-NUL.
    fn nonnul(&self, k: usize) -> bool {
        (0..k).all(|j| self.c(self.i + j) != 0)
    }

    fn has(&self, ch: u8) -> bool {
        cs(&self.buf).contains(&ch)
    }

    fn rest(&self) -> &[u8] {
        self.t.get(self.base + self.i..).unwrap_or(&[])
    }

    /// `LAB_10048685`: copy the buffer out and set the lengths.
    fn fin(mut self, class: i32) -> (Tok, i32) {
        self.buf[self.n.min(63)] = 0;
        let s = cs(&self.buf);
        let k = s.len().min(35);
        self.tok.text[..k].copy_from_slice(&s[..k]);
        self.tok.text[k] = 0;
        self.tok.class = class;
        self.tok.len = k as i32;
        self.tok.end = self.tok.start + k as i32;
        self.tok.consumed = self.i as i32;
        (self.tok, self.i as i32)
    }

    /// Empty token at the cursor (paragraph break, end of text).
    fn empty(mut self, class: i32) -> (Tok, i32) {
        self.tok.start = self.i as i32 + self.off;
        self.n = 0;
        self.buf[0] = 0;
        self.fin(class)
    }

    /// Fixed-length markup tokens return past what they matched.
    fn fin_skip(self, class: i32, k: usize) -> (Tok, i32) {
        let (mut tok, _) = self.fin(class);
        tok.consumed += k as i32;
        let r = tok.consumed;
        (tok, r)
    }

    /// `joined_r0x10047c54` and friends: the character and every identical
    /// one after it.
    fn run(mut self, class: i32) -> (Tok, i32) {
        self.buf[self.n] = self.c(self.i);
        loop {
            self.n += 1;
            self.i += 1;
            if self.n > 0x1c || self.c(self.i) != self.c(self.i - 1) {
                break;
            }
            self.buf[self.n] = self.c(self.i);
        }
        self.fin(class)
    }

    /// Sentence punctuation / '¿' '¡': a run of the same character.
    fn punct_run(mut self, class: i32, repeat_class: impl Fn(u8) -> i32) -> (Tok, i32) {
        self.buf[self.n] = self.c(self.i);
        self.n += 1;
        self.i += 1;
        let mut cnt = 1;
        if self.n < 0x1d {
            while self.c(self.i - 1) == self.c(self.i) {
                self.buf[self.n] = self.c(self.i);
                self.n += 1;
                self.i += 1;
                cnt += 1;
                if self.n > 0x1c {
                    break;
                }
            }
        }
        let class = if cnt > 1 { repeat_class(self.c(self.i)) } else { class };
        self.fin(class)
    }

    fn dbcs(mut self) -> (Tok, i32) {
        let b720 = |s: &Cut, k: usize| {
            let (p0, p1) = (s.c(k), s.c(k + 1));
            p1 != 0 && p0 == 0xa3 && ((0xc0 < p1 && p1 < 0xdb) || (0xe0 < p1 && p1 < 0xfb))
        };
        let b760 = |s: &Cut, k: usize| {
            let (p0, p1) = (s.c(k), s.c(k + 1));
            p1 != 0 && p0 == 0xa3 && 0xaf < p1 && p1 < 0xba
        };
        self.tok.start = self.i as i32 + self.off;
        let first = b720(&self, self.i);
        if !first && !b760(&self, self.i) {
            self.buf[self.n] = self.c(self.i);
            self.buf[self.n + 1] = self.c(self.i + 1);
            self.i += 2;
            self.n += 2;
            self.buf[self.n] = 0;
            fold_dbcs(&mut self.buf, 3);
            self.n = strlen(&self.buf);
            return self.fin(3);
        }
        let more = |s: &Cut, k: usize| if first { b720(s, k) } else { b760(s, k) };
        loop {
            self.buf[self.n] = self.c(self.i);
            self.buf[self.n + 1] = self.c(self.i + 1);
            self.n += 2;
            self.i += 2;
            if self.n > 0x1b || !more(&self, self.i) {
                break;
            }
        }
        self.buf[self.n] = 0;
        fold_dbcs(&mut self.buf, if first { 1 } else { 2 });
        self.n = strlen(&self.buf);
        self.fin(1)
    }
}

/// Number of ordinal-suffix bytes to join after a number ending in `last`.
fn ordinal(k: &Cut, last: u8) -> usize {
    let s = |j: usize| k.c(k.i + j);
    let m2 = |p: &[u8]| strnicmp_spa(k.rest(), p, 2) == 0;
    let nb = !k.has(b',');
    let two = nb && k.nonnul(2) && (s(2) == 0 || CLS_S[s(2) as usize] & 0xc0 == 0);
    let male = match last {
        b'0' | b'7' => m2(b"mo"),
        b'1' => m2(b"ro") || m2(b"mo"),
        b'2' => m2(b"do") || m2(b"mo"),
        b'3' => m2(b"ro"),
        b'4'..=b'6' => m2(b"to"),
        b'8' => m2(b"vo"),
        b'9' => m2(b"no"),
        _ => false,
    };
    if two && male {
        return 2;
    }
    let female = match last {
        b'0' | b'7' => m2(b"ma"),
        b'1' => m2(b"ra") || m2(b"ma"),
        b'2' => m2(b"da") || m2(b"ma"),
        b'3' => m2(b"ra"),
        b'4'..=b'6' => m2(b"ta"),
        b'8' => m2(b"va"),
        b'9' => m2(b"na"),
        _ => false,
    };
    if two && female {
        return 2;
    }
    if nb
        && k.nonnul(3)
        && (s(3) == 0 || CLS_S[s(3) as usize] & 0xc0 == 0)
        && last == b'3'
        && (strnicmp_spa(k.rest(), b"ero", 3) == 0 || strnicmp_spa(k.rest(), b"era", 3) == 0)
    {
        return 3;
    }
    if nb && (s(0) == 0xaa || s(0) == 0xba) {
        return 1;
    }
    let s1 = s(1);
    if !nb || (s(0) != b'o' && s(0) != b'a') || (s1 != 0 && !ws(s1) && !in_set(SENT_PUNCT, s1) && !in_set(DELIM_B, s1)) {
        return 0;
    }
    1
}

/// `FUN_10047000(tok, t + base, off, mode)`: the token at `t[base..]`, whose
/// offset in the sentence is `off`. Returns the token and the bytes consumed.
pub fn tokenize(t: &[u8], base: usize, off: i32, mode: i32) -> (Tok, i32) {
    let mut k = Cut { t, base, off, tok: Tok::default(), buf: [0; 64], n: 0, i: 0 };
    let mut nlc = 0;
    let mut acc: i32 = 0;
    let mut cont = false;
    let mut f13 = false;
    loop {
        let c = k.c(k.i);
        if c == 10 {
            nlc += 1;
        } else if !(c == b' ' || c == 9 || c == 13) {
            break;
        }
        k.i += 1;
        k.tok.ws += 1;
    }
    if nlc >= 2 {
        return k.empty(9);
    }
    if nlc == 1 {
        k.tok.nl = 1;
    }
    if k.c(k.i) == 0 {
        return k.empty(10);
    }
    loop {
        let mut c = k.c(k.i);
        if c == 0 {
            return k.empty(10);
        }
        if ws(c) {
            if k.n > 0 {
                return k.fin(0);
            }
            while ws(c) {
                if c == 10 {
                    nlc += 1;
                }
                k.i += 1;
                k.tok.ws += 1;
                c = k.c(k.i);
            }
            if nlc > 1 {
                return k.empty(9);
            }
            if c == 0 {
                return k.empty(10);
            }
        }

        if alpha(c) {
            let mut w = acc;
            if !cont {
                w = 0;
                k.tok.start = k.i as i32 + off;
            }
            loop {
                let ch = k.c(k.i);
                w += 1 + WIDTH[ch as usize] as i32;
                if w > 99 || k.n > 0x1c {
                    return k.fin(1);
                }
                acc = w;
                k.buf[k.n] = fold(ch);
                k.n += 1;
                k.i += 1;
                if !alpha(k.c(k.i)) {
                    break;
                }
            }
            if k.n > 0x1c {
                return k.fin(1);
            }
            let (s0, s1) = (k.c(k.i), k.c(k.i + 1));
            let join = |k: &mut Cut, ch: u8| {
                k.buf[k.n] = ch;
                k.n += 1;
                k.i += 1;
            };
            if mode != 9 && mode != 8 && mode != 0x14 && s0 == b'.' && alpha2(s1) {
                join(&mut k, b'.');
                cont = true;
                continue;
            }
            match mode {
                7 | 8 => {
                    if s0 == b'-' && alpha2(s1) {
                        join(&mut k, b'-');
                        cont = true;
                        continue;
                    }
                }
                0xc => {
                    if s0 == b'/' && k.n < 3 && (s1 == 0 || ws(s1) || in_set(SENT_PUNCT, s1) || in_set(DELIM_A, s1)) {
                        join(&mut k, b'/');
                        return k.fin(1);
                    }
                    let other = s0 != b'.' || (s1 != 0 && !digit(s1) && !ws(s1) && !in_set(SENT_PUNCT, s1) && !in_set(DELIM_B, s1));
                    if !other {
                        join(&mut k, b'.');
                        return k.fin(1);
                    }
                    if (s0 == b'/' || s0 == b'.') && alpha2(s1) && k.n < 3 {
                        join(&mut k, s0);
                        cont = true;
                        continue;
                    }
                }
                0x13 => {
                    if !f13 && (digit(s0) || s0 == 0xb2 || s0 == 0xb3) {
                        join(&mut k, s0);
                        let c2 = k.c(k.i);
                        if CLS_S[c2 as usize] & 0xd0 != 0 || c2 == 0xb2 || c2 == 0xb3 {
                            f13 = true;
                            cont = true;
                            continue;
                        }
                    }
                }
                0xf => {
                    k.buf[k.n] = 0;
                    if s0 == b'.' && (stricmp_spa(cs(&k.buf), b"A.M") == 0 || stricmp_spa(cs(&k.buf), b"P.M") == 0) {
                        join(&mut k, b'.');
                    }
                }
                0x11 => {
                    if k.n == 1
                        && CLS_S[k.buf[0] as usize] & 0x80 != 0
                        && digit(s0)
                        && CLS_S[s1 as usize] & 0xc0 != 0
                        && CLS_S[s1 as usize] & 0x80 != 0
                    {
                        join(&mut k, s0);
                        cont = true;
                        continue;
                    }
                }
                _ => {}
            }
            return k.fin(1);
        }

        if !digit(c) {
            let c1 = k.c(k.i + 1);
            if c1 != 0 && dbcs_pair(c, c1) {
                return k.dbcs();
            }
            if !in_set(SYMBOLS, c) && !in_set(SENT_PUNCT, c) && (c < 0x80 || HI[c as usize] & 8 == 0) {
                k.i += 1;
                k.tok.ws += 1;
                continue;
            }
            k.tok.start = k.i as i32 + off;
            let s = |k: &Cut, j: usize| k.c(k.i + j);
            if c == b'<' && k.nonnul(5) && s(&k, 1) == 0xa1 && s(&k, 2) == 0xdd && (s(&k, 3) as i8) > 0 && (s(&k, 3) as i8) < b'W' as i8 && s(&k, 4) == b'>'
            {
                k.buf[k.n] = s(&k, 3);
                k.n += 1;
                return k.fin_skip(4, 5);
            }
            if (0..5).all(|j| s(&k, j) == b"</\xa1\xdd>"[j]) {
                return k.fin_skip(5, 5);
            }
            if mode == 9 {
                if c == b':' && k.nonnul(2) && s(&k, 1) == b'/' && s(&k, 2) == b'/' {
                    k.buf[k.n..k.n + 3].copy_from_slice(b"://");
                    k.n += 3;
                    return k.fin_skip(3, 3);
                }
            } else if mode == 0x14 {
                k.buf[k.n] = c;
                k.n += 1;
                return k.fin_skip(3, 1);
            }
            if in_set(SYMBOLS, c) {
                return k.run(3);
            }
            if in_set(SENT_PUNCT, c) {
                return k.punct_run(8, |next| if next != b'.' { 9 } else { 3 });
            }
            if c == 0xbf || c == 0xa1 {
                return k.punct_run(7, |_| 3);
            }
            if c > 0x7f && HI[c as usize] & 8 != 0 {
                if matches!(c, 0x80 | 0x83 | 0xa2 | 0xa3 | 0xa5 | 0xb2 | 0xb3 | 0xb9 | 0xb0 | 0xb7 | 0xd7 | 0xf7 | 0xbc | 0xbd | 0xbe | 0x96 | 0x97 | 0xa9 | 0xae)
                {
                    return k.run(6);
                }
                k.i += 1;
                k.tok.ws += 1;
            }
            continue;
        }

        if !cont {
            k.tok.start = k.i as i32 + off;
        }
        loop {
            if k.n >= 0x1d {
                k.buf[k.n] = 0;
                if k.has(b',') {
                    let mut p = k.i;
                    while k.n > 0 && k.c(p) != b',' {
                        k.n -= 1;
                        k.i -= 1;
                        p -= 1;
                    }
                }
                return k.fin(2);
            }
            k.buf[k.n] = k.c(k.i);
            k.n += 1;
            k.i += 1;
            if !digit(k.c(k.i)) {
                break;
            }
        }
        k.buf[k.n] = 0;
        let mut ordinals = true;
        if k.n < 0x1d {
            let decimal = mode == 0x13 || !matches!(mode, 0xe | 0x10 | 4 | 6 | 0x14);
            if mode == 0x13 && !f13 {
                let (s0, s1) = (k.c(k.i), k.c(k.i + 1));
                if (CLS_S[s0 as usize] & 0xc0 != 0 && UPPER_S[s0 as usize] != b'X' && !digit(s1)) || s0 == 0xb2 || s0 == 0xb3 {
                    k.buf[k.n] = s0;
                    k.n += 1;
                    k.i += 1;
                    f13 = true;
                    let c2 = k.c(k.i);
                    if CLS_S[c2 as usize] & 0xd0 != 0 || c2 == 0xb2 || c2 == 0xb3 {
                        cont = true;
                        continue;
                    }
                }
            }
            if decimal {
                let s = |j: usize| k.c(k.i + j);
                let three = digit(s(1)) && digit(s(2)) && digit(s(3)) && (s(4) == 0 || !digit(s(4)));
                let lead0 = k.buf[0] == b'0';
                let sep = if s(0) == b'.' && !k.has(b',') && !lead0 && three {
                    Some(b'.')
                } else if s(0) == b',' && !k.has(b'.') && !lead0 && three {
                    Some(b',')
                } else if ((!k.has(b',') && s(0) == b',') || (!k.has(b'.') && s(0) == b'.')) && digit(s(1)) {
                    Some(s(0))
                } else {
                    None
                };
                if let Some(ch) = sep {
                    k.buf[k.n] = ch;
                    k.n += 1;
                    k.i += 1;
                    cont = true;
                    continue;
                }
            }
            if mode == 0x11 {
                let (s0, s1) = (k.c(k.i), k.c(k.i + 1));
                if k.n == 1 && CLS_S[s0 as usize] & 0xc0 != 0 && CLS_S[s0 as usize] & 0x80 != 0 && digit(s1) {
                    k.buf[1] = s0;
                    k.i += 1;
                    k.n = 2;
                    cont = true;
                    continue;
                }
            } else if matches!(mode, 2 | 0xf | 0x14) {
                ordinals = false;
            }
        } else if matches!(mode, 2 | 0xf | 0x14) {
            ordinals = false;
        }
        if ordinals {
            let j = ordinal(&k, k.buf[k.n - 1]);
            for _ in 0..j {
                k.buf[k.n] = k.c(k.i);
                k.n += 1;
                k.i += 1;
            }
        }
        return k.fin(2);
    }
}
