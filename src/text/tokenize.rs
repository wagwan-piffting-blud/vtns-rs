//! `fe_Tokenize` (0x1005a350): cut the next token from the normalizer's input.
//!
//! Transliterated label for label: every `L_xxxx` state is the decompile's
//! `LAB_1005xxxx` (or the named goto target) so the control flow can be checked
//! against `decomp_s/1005a350.c` line by line. `mode` (`param_4`) selects the
//! caller-specific joining rules; the normalizer chain calls it with 0, the
//! recognizers with their own modes.

use super::chars::{contraction, fold_dbcs, fw_digit, fw_letter, high_width, letter_phones};
use super::cstr::*;
use super::userdict::UserDict;

/// Token record (`int[0x11]`, 0x44 bytes).
#[derive(Clone, Debug)]
pub struct Tok {
    /// [0] whitespace and skipped characters before the token.
    pub ws: i32,
    /// [1] exactly one newline preceded the token.
    pub nl: i32,
    /// [2] absolute start offset.
    pub start: i32,
    /// [3] absolute end offset.
    pub end: i32,
    /// [4] strlen(text).
    pub len: i32,
    /// [5] bytes consumed (the return value).
    pub consumed: i32,
    /// +0x18 (short): text length at the last Latin-1 replacement, mode 8 only.
    pub f18: i16,
    /// +0x1a (short): the token starts a user-dictionary entry.
    pub udict: i16,
    /// [7] class: 1 word, 2 number, 3 symbol/punct, 4 tag marker, 5 tag close,
    /// 6 raw high byte, 7 sentence punctuation, 8 paragraph break, 9 end of text.
    pub class: i32,
    /// [8..] NUL-terminated token text (36 bytes).
    pub text: [u8; 36],
}

impl Default for Tok {
    fn default() -> Self {
        Tok { ws: 0, nl: 0, start: 0, end: 0, len: 0, consumed: 0, f18: 0, udict: 0, class: 0, text: [0; 36] }
    }
}

impl Tok {
    pub fn text(&self) -> &[u8] {
        cs(&self.text)
    }
}

enum High {
    Skip,
    Raw,
    Ncpy1(u32),
    Ncpy2(u32),
    Cat1(u32),
    Cat2(u32),
}

/// The CP1252 high-byte switch at 0x1005a578.
fn high_action(t: &[u8], p: isize, buf: &[u8; 48], len: i32) -> High {
    use High::*;
    let c = at(t, p);
    match c {
        0x80 => Ncpy2(0x1009c47c),
        0x81 | 0x86 | 0x87 | 0x88 | 0x8d | 0x8e | 0x8f | 0x90 | 0x95 | 0x98 | 0x9d | 0x9e | 0xa1 | 0xa8 | 0xaa | 0xac
        | 0xad | 0xaf | 0xb4 | 0xb7 | 0xb8 | 0xba | 0xbf => Skip,
        0x82 | 0x8b | 0x91 | 0x9b => Ncpy1(0x1009c3a4),
        0x84 => Ncpy1(0x1007d4f8),
        0x85 => Ncpy2(0x1009c478),
        0x89 => Ncpy2(0x1009c474),
        0x8a | 0x9a => Cat2(0x100778dc),
        0x8c | 0x9c => Cat2(0x1009c470),
        0x92 => Ncpy1(0x1009c3a4),
        0x93 => Ncpy1(0x1007d4f8),
        0x94 | 0xab => Ncpy1(0x1007d4f8),
        0x96 => Ncpy1(0x1007739c),
        0x97 => Ncpy2(0x1009c46c),
        0x99 => Ncpy2(0x1009c468),
        0x9f | 0xdd | 0xde | 0xfd | 0xfe | 0xff => Cat1(0x1007d340),
        0xa0 => Ncpy1(0x1007b3cc),
        0xa2 => Ncpy2(0x1009c464),
        0xa3 => Ncpy2(0x1009c460),
        0xa5 => Ncpy2(0x1009c45c),
        0xa6 => Ncpy1(0x1009c458),
        0xa7 => Ncpy2(0x1009c454),
        0xae => Ncpy2(0x1009c450),
        0xb0 => Ncpy2(0x100805a4),
        0xb1 => Ncpy2(0x1009c44c),
        0xb2 => Ncpy2(0x1009c448),
        0xb3 => Ncpy2(0x1009c444),
        0xb5 if era() ==crate::era::Era::Paul09 => Raw,
        0xb5 => Ncpy2(0x10080590),
        0xb9 => Ncpy2(0x1009c440),
        0xbb => Ncpy1(0x1007d4f8),
        0xbc => Ncpy2(0x1009c43c),
        0xbd => Ncpy2(0x1009c438),
        0xbe => Ncpy2(0x1009c434),
        0xc0..=0xc4 => Cat1(0x10077718),
        0xc5 => {
            let prev_alpha = len >= 1 && cls(buf[(len - 1) as usize]) & 0xc0 != 0;
            if !prev_alpha {
                let b2 = at(t, p + 1);
                if cls(b2) & 0xc0 == 0 && (!latin_letter(b2) || ad24(b2) == 0) {
                    return Ncpy2(0x1009c430);
                }
            }
            Cat1(0x10077718)
        }
        0xc6 => Cat2(0x10080bc0),
        0xc7 => Cat1(0x1007d390),
        0xc8..=0xcb => Cat1(0x1007d388),
        0xcc..=0xcf => Cat1(0x10077854),
        0xd0 => Cat2(0x10080b84),
        0xd1 => Cat1(0x1007d368),
        0xd2..=0xd6 | 0xd8 | 0xf2..=0xf6 | 0xf8 => Cat1(0x1007d364),
        0xd7 => Ncpy2(0x100803b0),
        0xd9..=0xdc | 0xf9..=0xfc => Cat1(0x1007d350),
        0xdf => Cat2(0x1009c42c),
        0xe0..=0xe5 => Cat1(0x10077718),
        0xe6 => Cat2(0x10080bc0),
        0xe7 => Cat1(0x1007d390),
        0xe8..=0xeb => Cat1(0x1007d388),
        0xec..=0xef => Cat1(0x10077854),
        0xf0 => Cat2(0x10080b84),
        0xf1 => Cat1(0x1007d368),
        0xf7 => Ncpy2(0x100803ac),
        _ => Raw,
    }
}

#[derive(Clone, Copy, Debug)]
enum L {
    A417,
    HighByte,
    CaseD81,
    Ab9d,
    Digits2Finish,
    Aba8,
    LettersDispatch,
    Acb6,
    Acbf,
    B136,
    A403,
    A408,
    A409,
    Ae57,
    Ae58,
    Af3d,
    Ae63,
    Bb77,
    Bb30,
    Bc15,
    C3c3,
    C3f4,
    C415,
    Digits,
    B5ee,
    B680,
    B75b,
    Bcf5,
    Bcff,
    Bed2,
    Bede,
    Bca7,
    NonAlnum,
    C2a7,
    C2bc,
    C2e9,
    B932,
    Ba14,
    Ba4e,
    C275,
    Baf7,
    Ba9e,
    Bf5e,
    Bf95,
    C03a,
    C071,
}

/// Tokenize one token from `t[p0..]`. `off` is the absolute offset of `p0`
/// (`param_3`). Returns the number of bytes consumed.
pub fn tokenize(tok: &mut Tok, t: &[u8], p0: usize, off: i32, mode: i32, ud: Option<&UserDict>) -> i32 {
    *tok = Tok::default();
    let mut p = p0 as isize;
    let mut n: i32 = 0;
    let mut len: i32 = 0;
    let mut ph: i32 = 0;
    let mut nls: i32 = 0;
    let mut cl: i32 = 0;
    let mut intok: i32 = 0;
    let mut f1c: i32 = 0;
    let mut buf = [0u8; 48];
    let mut c28 = [0u8; 16];
    let mut p8: isize;
    let mut i6: i32 = 0;
    let mut p9: i32 = 0;
    let mut p14: isize = 0;
    let mut dp: usize = 0;
    let ret: i32;

    // LAB_1005a384: leading whitespace.
    loop {
        let c = at(t, p);
        if c == b' ' || c == b'\t' || c == b'\r' || c == b'\n' {
            if c == b'\n' {
                nls += 1;
            }
            p += 1;
            n += 1;
            tok.ws += 1;
            continue;
        }
        break;
    }
    p8 = p;
    let mut st = if nls > 1 {
        tok.start = n + off;
        tok.text[0] = 0;
        tok.class = 8;
        L::Baf7
    } else {
        if nls == 1 {
            tok.nl = 1;
        }
        if at(t, p) == 0 {
            tok.start = n + off;
            tok.text[0] = 0;
            tok.class = 9;
            L::Baf7
        } else {
            L::A417
        }
    };

    loop {
        match st {
            L::A417 => {
                let c = at(t, p);
                if c == 0 {
                    if len < 1 {
                        tok.start = off + n;
                        tok.class = 9;
                        tok.text[0] = 0;
                        st = L::Ba9e;
                        continue;
                    }
                    st = L::C3c3;
                    continue;
                }
                if is_ws(c) {
                    if len > 0 {
                        st = L::C3c3;
                        continue;
                    }
                    let mut c = c;
                    while is_ws(c) {
                        if c == b'\n' {
                            nls += 1;
                        }
                        p += 1;
                        n += 1;
                        tok.ws += 1;
                        c = at(t, p);
                    }
                    p8 = p;
                    if nls > 1 {
                        tok.start = n + off;
                        tok.text[0] = 0;
                        tok.class = 8;
                        st = L::Baf7;
                        continue;
                    }
                    if c == 0 {
                        tok.start = n + off;
                        tok.text[0] = 0;
                        tok.class = 9;
                        st = L::Baf7;
                        continue;
                    }
                }
                let c = at(t, p);
                let c1 = at(t, p + 1);
                if c > 0x7f
                    && ad24(c) != 0
                    && (c < 0xa1 || c > 0xad || c1 < 0xa1 || c1 == 0xff)
                    && !(c == 0xfd && c1 == 0xfe && !era().is_2009())
                    && (c != 0xae || c1 < 0xa1 || c1 > 0xc2)
                {
                    st = L::HighByte;
                } else if cls(c) & 0xc0 != 0 {
                    st = L::LettersDispatch;
                } else if cls(c) & 0x10 == 0 {
                    st = L::NonAlnum;
                } else {
                    st = L::Digits;
                }
            }

            L::HighByte => {
                if intok as i16 == 0 {
                    ph = 0;
                    tok.start = n + off;
                }
                let c = at(t, p);
                if latin_letter(c) && ad24(c) != 0 {
                    i6 = letter_phones(c);
                    ph += 1 + i6;
                    i6 = len;
                    if ph > 0x40 || high_width(at(t, p)) + len > 0x1c {
                        st = L::Bc15;
                        continue;
                    }
                    buf[len as usize] = 0;
                }
                match high_action(t, p, &buf, len) {
                    High::Skip => {}
                    High::Raw => {
                        buf[len as usize] = at(t, p);
                        len += 1;
                        cl = 6;
                    }
                    High::Ncpy1(va) => {
                        strncpy(&mut buf, s(va), 1);
                        len += 1;
                        cl = 3;
                    }
                    High::Ncpy2(va) => {
                        strncpy(&mut buf, s(va), 2);
                        len += 2;
                        cl = 3;
                    }
                    High::Cat1(va) => {
                        cl = 1;
                        strcat(&mut buf, s(va));
                        len += 1;
                        p = p8;
                    }
                    High::Cat2(va) => {
                        cl = 1;
                        strcat(&mut buf, s(va));
                        len += 2;
                        p = p8;
                    }
                }
                st = L::CaseD81;
            }

            L::CaseD81 => {
                if mode == 8 && cl == 1 {
                    tok.f18 = len as i16;
                }
                let c = at(t, p);
                if latin_letter(c) && ad24(c) != 0 {
                    let c1 = at(t, p + 1);
                    if cls(c1) & 0xc0 != 0 || (latin_letter(c1) && ad24(c1) != 0) {
                        st = L::Ab9d;
                        continue;
                    }
                }
                n += 1;
                buf[len as usize] = 0;
                st = L::Bb30;
            }

            L::Ab9d => {
                let p13 = p + 1;
                p8 = p13;
                n += 1;
                p = p13;
                st = L::Aba8;
            }

            L::Aba8 => {
                intok = 1;
                st = L::A417;
            }

            L::LettersDispatch => {
                let mut i4 = ph;
                p9 = n;
                if intok as i16 == 0 {
                    ph = 0;
                    tok.start = n + off;
                    i4 = ph;
                }
                let mut bail = false;
                loop {
                    n = p9;
                    p14 = p;
                    i6 = len;
                    len = letter_phones(at(t, p14));
                    i4 += 1 + len;
                    ph = i4;
                    if i4 > 0x40 || i6 > 0x1c {
                        buf[i6 as usize] = 0;
                        if strchr_at(&buf, b'-').is_some() || strchr_at(&buf, b'.').is_some() {
                            while i6 > 0 && at(t, p14) != b'-' && at(t, p14) != b'.' {
                                p14 -= 1;
                                i6 -= 1;
                                n -= 1;
                            }
                        }
                        bail = true;
                        break;
                    }
                    buf[i6 as usize] = at(t, p14);
                    len = i6 + 1;
                    p = p14 + 1;
                    p9 = n + 1;
                    p8 = p;
                    if cls(at(t, p14 + 1)) & 0xc0 == 0 {
                        break;
                    }
                }
                if bail {
                    st = L::Bc15;
                    continue;
                }
                buf[(i6 + 1) as usize] = 0;
                if len > 0x1c {
                    st = L::Ae63;
                    continue;
                }
                if mode == 9 {
                    st = L::Acb6;
                    continue;
                }
                if mode != 8 {
                    if matches!(mode, 0xc | 0x1a | 0x12 | 0x18 | 0x1d | 0x1c) {
                        st = L::Acb6;
                        continue;
                    }
                    if at(t, p) == b'.' && cls(at(t, p14 + 2)) & 0xc0 != 0 {
                        buf[(i6 + 1) as usize] = b'.';
                        len = i6 + 2;
                        n = p9;
                    } else {
                        if len != 3 || stricmp(&buf, s(0x1009c060)) != 0 || at(t, p) != b'\'' || cls(at(t, p14 + 2)) & 0xc0 == 0 {
                            st = L::Acb6;
                            continue;
                        }
                        buf[3] = b'\'';
                        len = 4;
                        n = p9;
                    }
                    st = L::Ab9d;
                    continue;
                }
                st = L::Acbf;
            }

            L::Acb6 => {
                if mode == 8 {
                    st = L::Acbf;
                    continue;
                }
                if mode == 0x17 {
                    if at(t, p) == b'-' && cls(at(t, p14 + 2)) & 0xc0 != 0 {
                        buf[(i6 + 1) as usize] = b'-';
                        len = i6 + 2;
                        n = p9;
                        st = L::Ab9d;
                        continue;
                    }
                    if at(t, p) == b'\'' && len > 0 && strchr_at(&buf, b'\'').is_none() && contraction(&mut c28, at(t, p14), t, p) {
                        let bl = strlen(&buf);
                        put_str(&mut buf, bl, &c28);
                        let l = strlen(&c28) as i32;
                        len += l;
                        p8 = p + l as isize;
                        p = p8;
                        p9 += l;
                    } else {
                        if at(t, p) != b'.' || at(t, p14 + 2) != b'\'' || !contraction(&mut c28, b'.', t, p14 + 2) {
                            st = L::Ae63;
                            continue;
                        }
                        st = L::Af3d;
                        continue;
                    }
                    st = L::Ae63;
                    continue;
                }
                if mode != 0x15 && mode != 0x11 {
                    st = L::B136;
                    continue;
                }
                let c = at(t, p);
                if c == b'.' && cls(at(t, p14 + 2)) & 0xc0 != 0 {
                    buf[(i6 + 1) as usize] = b'.';
                    len = i6 + 2;
                    n = p9;
                    st = L::Ab9d;
                    continue;
                }
                if c == b'-' && cls(at(t, p14 + 2)) & 0xc0 != 0 {
                    buf[(i6 + 1) as usize] = b'-';
                    len = i6 + 2;
                    n = p9;
                    st = L::Ab9d;
                    continue;
                }
                if len == 1 && cls(buf[0]) & 0x80 != 0 && cls(c) & 0x10 != 0 && cls(at(t, p14 + 2)) & 0xc0 != 0 && cls(at(t, p14 + 2)) & 0x80 != 0 {
                    buf[1] = c;
                    len = 2;
                    n = p9;
                    st = L::Ab9d;
                    continue;
                }
                if c == b' ' && stricmp(&buf, s(0x1009c418)) == 0 {
                    let p13 = p14 + 2;
                    if !has_n(t, p13, 6) || (at(t, p14 + 8) != 0 && cls(at(t, p14 + 8)) & 0xc0 != 0) || strnicmp(&t[p13 as usize..], s(0x1009c410), 6) != 0 {
                        st = L::B136;
                        continue;
                    }
                    buf[(i6 + 1) as usize] = at(t, p);
                    len = i6 + 2;
                    n = p9;
                    // LAB_1005ab9e with pbVar13 = pbVar14 + 2.
                    p8 = p13;
                    n += 1;
                    p = p13;
                    st = L::Aba8;
                    continue;
                }
                st = L::B136;
            }

            L::B136 => {
                if mode == 0x19 {
                    if len != 1 || cls(buf[0]) & 0x80 == 0 || cls(at(t, p)) & 0x10 == 0 {
                        st = L::Ae63;
                        continue;
                    }
                    buf[1] = at(t, p);
                    len = 2;
                    st = L::A409;
                    continue;
                }
                if mode == 0x16 {
                    if at(t, p) == b'\'' && cls(at(t, p14 + 2)) & 0xc0 != 0 {
                        st = L::A403;
                        continue;
                    }
                    if at(t, p) != b'-' || cls(at(t, p14 + 2)) & 0xc0 == 0 {
                        st = L::Ae63;
                        continue;
                    }
                    buf[(i6 + 1) as usize] = b'-';
                    st = L::A408;
                    continue;
                }
                if (mode == 0xc || mode == 0x1a) && at(t, p) == b'\'' && cls(at(t, p14 + 2)) & 0xc0 != 0 {
                    st = L::A403;
                    continue;
                }
                if mode != 0x1b {
                    if mode == 0xf {
                        buf[(i6 + 1) as usize] = 0;
                        if at(t, p) == b'.' && (stricmp(&buf, s(0x10081090)) == 0 || stricmp(&buf, s(0x10081080)) == 0) {
                            buf[(i6 + 1) as usize] = at(t, p);
                            st = L::Ae57;
                            continue;
                        }
                    } else if mode == 5 {
                        buf[(i6 + 1) as usize] = 0;
                        if at(t, p) == b'\''
                            && stricmp(&buf, s(0x1009bdfc)) == 0
                            && lower(at(t, p14 + 2)) == 0x61
                            && lower(at(t, p14 + 3)) == 0x6e
                            && lower(at(t, p14 + 4)) == 0x67
                            && lower(at(t, p14 + 5)) == 0x61
                        {
                            buf[(i6 + 1) as usize] = at(t, p);
                            len = i6 + 2;
                            n = p9;
                            st = L::Ab9d;
                            continue;
                        }
                    } else if mode == 0x1d && cls(at(t, p)) & 0x10 != 0 {
                        buf[(i6 + 1) as usize] = at(t, p);
                        st = L::A408;
                        continue;
                    }
                    st = L::Ae63;
                    continue;
                }
                // mode 0x1b
                buf[(i6 + 1) as usize] = 0;
                let c = at(t, p);
                let c2 = at(t, p14 + 2);
                let slash_ok = c == b'/'
                    && len < 3
                    && (c2 == 0 || c2 == b' ' || c2 == b'\t' || c2 == b'\n' || c2 == b'\r' || strchr(s(0x100813bc), c2) || (c2 != 0 && strchr(s(0x1009c404), c2)));
                let dot_ok = c == b'.'
                    && (strcmp(&buf, s(0x1008000c)) == 0 || strcmp(&buf, s(0x1009c3fc)) == 0 || strcmp(&buf, s(0x1009c3f8)) == 0 || strcmp(&buf, s(0x10077764)) == 0)
                    && (c2 == 0
                        || cls(c2) & 0x10 != 0
                        || c2 == b' '
                        || c2 == b'\t'
                        || c2 == b'\n'
                        || c2 == b'\r'
                        || strchr(s(0x100813bc), c2)
                        || (c2 != 0 && strchr(s(0x1009c3e8), c2)));
                if slash_ok || dot_ok {
                    buf[(i6 + 1) as usize] = c;
                    st = L::Ae57;
                    continue;
                }
                if len != 1 {
                    if len != 2 || c != b'!' || strcmp(&buf, s(0x1009c3e4)) != 0 {
                        st = L::Ae63;
                        continue;
                    }
                    len = 3;
                    buf[2] = c;
                    st = L::Ae58;
                    continue;
                }
                if c == b'.' && cls(buf[0]) & 0x80 != 0 && c2 == b'&' && cls(at(t, p14 + 3)) & 0xc0 != 0 && cls(at(t, p14 + 3)) & 0x80 != 0 {
                    buf[1] = b'.';
                    p = p14 + 3;
                    n += 3;
                    buf[2] = c2;
                    len = 3;
                    p8 = p;
                    intok = 1;
                    st = L::A417;
                    continue;
                }
                if c == b'&' && cls(buf[0]) & 0x80 != 0 && cls(c2) & 0xc0 != 0 && cls(c2) & 0x80 != 0 {
                    buf[1] = c;
                } else {
                    if c != b'/' || cls(c2) & 0xc0 == 0 {
                        st = L::Ae63;
                        continue;
                    }
                    buf[1] = b'/';
                }
                p = p14 + 2;
                n += 2;
                len = 2;
                p8 = p;
                intok = 1;
                st = L::A417;
            }

            L::A403 => {
                buf[(i6 + 1) as usize] = b'\'';
                st = L::A408;
            }
            L::A408 => {
                len = i6 + 2;
                st = L::A409;
            }
            L::A409 => {
                p = p14 + 2;
                n += 2;
                p8 = p;
                intok = 1;
                st = L::A417;
            }

            L::Acbf => {
                // mode 8 joining rules.
                let c = at(t, p);
                if c == b'-' && cls(at(t, p14 + 2)) & 0xc0 != 0 {
                    let p13 = (p14 + 2) as usize;
                    let hit = match ud {
                        None => false,
                        Some(u) => u.find_s(t, p13, 0) >= 0 || u.find_i(t, p13, 0) >= 0,
                    };
                    if !hit {
                        buf[(i6 + 1) as usize] = c;
                        len = i6 + 2;
                        n = p9;
                        // LAB_1005ab9e with pbVar13 = pbVar14 + 2.
                        p8 = p13 as isize;
                        n += 1;
                        p = p13 as isize;
                        st = L::Aba8;
                        continue;
                    }
                    st = L::Bb77;
                    continue;
                }
                if c == b'.' && cls(at(t, p14 + 2)) & 0xc0 != 0 {
                    let p13 = p14 + 2;
                    if strchr_at(&buf, b'.').is_some() || cls(buf[0]) & 0x40 == 0 || cls(at(t, p13)) & 0x80 == 0 {
                        if let Some(u) = ud {
                            if u.find_s(t, p13 as usize, 0) > -1 || u.find_i(t, p13 as usize, 0) > -1 {
                                st = L::Bb77;
                                continue;
                            }
                        }
                        buf[(i6 + 1) as usize] = c;
                        len = i6 + 2;
                        n = p9;
                        p8 = p13;
                        n += 1;
                        p = p13;
                        st = L::Aba8;
                        continue;
                    }
                } else if c == b'\'' && len > 0 && strchr_at(&buf, b'\'').is_none() && contraction(&mut c28, at(t, p14), t, p) {
                    let bl = strlen(&buf);
                    put_str(&mut buf, bl, &c28);
                    let l = strlen(&c28) as i32;
                    p += l as isize;
                    len += l;
                    p9 += l;
                    p8 = p;
                    if at(t, p) == b'-' && cls(at(t, p + 1)) & 0xc0 != 0 {
                        buf[len as usize] = b'-';
                        len += 1;
                        n = p9;
                        st = L::Ab9d;
                        continue;
                    }
                } else if at(t, p) == b'\'' && stricmp(&buf, s(0x1009c428)) == 0 {
                    buf[(i6 + 1) as usize] = at(t, p);
                    st = L::Ae57;
                    continue;
                } else if at(t, p) == b'.' && at(t, p14 + 2) == b'\'' && contraction(&mut c28, b'.', t, p14 + 2) {
                    st = L::Af3d;
                    continue;
                }
                st = L::Ae63;
            }

            L::Af3d => {
                let bl = strlen(&buf);
                let mut tmp = [0u8; 20];
                tmp[0] = at(t, p);
                strcpy(&mut tmp[1..], &c28);
                put_str(&mut buf, bl, &tmp);
                let l = strlen(&c28) as i32 + 1;
                len += l;
                p8 = p + l as isize;
                p = p8;
                p9 += l;
                st = L::Ae63;
            }

            L::Ae57 => {
                len = i6 + 2;
                st = L::Ae58;
            }
            L::Ae58 => {
                p8 = p14 + 2;
                p = p8;
                p9 = n + 2;
                st = L::Ae63;
            }

            L::Ae63 => {
                n = p9;
                let c = at(t, p);
                let c1 = at(t, p + 1);
                i6 = len;
                if !latin_letter(c) || ad24(c) == 0 || ((0xa0 < c && c < 0xae) && (0xa0 < c1 && c1 != 0xff)) || (c == 0xfd && c1 == 0xfe && !era().is_2009()) {
                    st = L::Bc15;
                    continue;
                }
                if c != 0xae || c1 < 0xa1 {
                    st = L::Aba8;
                    continue;
                }
                if c1 < 0xc3 {
                    st = L::Bc15;
                    continue;
                }
                intok = 1;
                st = L::A417;
            }

            L::Bb77 => {
                buf[(i6 + 1) as usize] = 0;
                strcpy(&mut tok.text, &buf);
                tok.class = 1;
                tok.len = strlen(&tok.text) as i32;
                tok.consumed = p9;
                tok.end = p9 + (tok.start - tok.ws);
                ret = p9;
                break;
            }

            L::Bb30 => {
                strcpy(&mut tok.text, &buf);
                tok.class = cl;
                tok.len = strlen(&tok.text) as i32;
                tok.end = n + (tok.start - tok.ws);
                tok.consumed = n;
                st = L::C415;
            }

            L::Bc15 => {
                buf[i6 as usize] = 0;
                strcpy(&mut tok.text, &buf);
                tok.class = 1;
                st = L::C3f4;
            }

            L::C3c3 => {
                buf[len as usize] = 0;
                strcpy(&mut tok.text, &buf);
                tok.class = cl;
                st = L::C3f4;
            }

            L::C3f4 => {
                tok.len = strlen(&tok.text) as i32;
                tok.end = n + (tok.start - tok.ws);
                tok.consumed = n;
                st = L::C415;
            }

            L::C415 => {
                if let Some(u) = ud {
                    if u.find_s(&buf, 0, 0) > -1 || u.find_i(&buf, 0, 0) > -1 {
                        tok.udict = 1;
                    }
                }
                ret = n;
                break;
            }

            L::Digits => {
                p9 = n;
                if intok as i16 == 0 {
                    tok.start = n + off;
                }
                let mut bail = false;
                loop {
                    n = p9;
                    p14 = p;
                    i6 = len;
                    if i6 > 0x1c {
                        buf[i6 as usize] = 0;
                        len = i6;
                        p9 = n;
                        if strchr_at(&buf, b',').is_some() {
                            while len > 0 && at(t, p14) != b',' {
                                n -= 1;
                                len -= 1;
                                p14 -= 1;
                            }
                            p9 = n;
                        }
                        bail = true;
                        break;
                    }
                    buf[i6 as usize] = at(t, p14);
                    len = i6 + 1;
                    p = p14 + 1;
                    p9 = n + 1;
                    p8 = p;
                    if cls(at(t, p14 + 1)) & 0x10 == 0 {
                        break;
                    }
                }
                if bail {
                    st = L::Bede;
                    continue;
                }
                buf[(i6 + 1) as usize] = 0;
                if len >= 0x1d {
                    st = L::Bcf5;
                    continue;
                }
                if mode == 0x13 {
                    if f1c as i16 == 0 {
                        let c = at(t, p);
                        if cls(c) & 0xc0 != 0 && upper(c) != 0x58 && cls(at(t, p14 + 2)) & 0x10 == 0 {
                            buf[(i6 + 1) as usize] = c;
                            len = i6 + 2;
                            p = p14 + 2;
                            n += 2;
                            p8 = p;
                            f1c = 1;
                            if cls(at(t, p)) & 0xc0 != 0 {
                                intok = 1;
                            }
                            st = L::A417;
                            continue;
                        }
                    }
                    st = L::B5ee;
                    continue;
                }
                if mode != 0xe && mode != 0x10 && mode != 3 && mode != 6 && mode != 0x1c {
                    st = L::B5ee;
                    continue;
                }
                st = L::B680;
            }

            L::B5ee => {
                let c = at(t, p);
                if c == b','
                    && strchr_at(&buf, b'.').is_none()
                    && buf[0] != b'0'
                    && cls(at(t, p14 + 2)) & 0x10 != 0
                    && cls(at(t, p14 + 3)) & 0x10 != 0
                    && cls(at(t, p14 + 4)) & 0x10 != 0
                    && (at(t, p14 + 5) == 0 || cls(at(t, p14 + 5)) & 0x10 == 0)
                {
                    buf[(i6 + 1) as usize] = c;
                    len = i6 + 2;
                    n = p9;
                } else {
                    if strchr_at(&buf, b'.').is_some() || c != b'.' || cls(at(t, p14 + 2)) & 0x10 == 0 {
                        st = L::B680;
                        continue;
                    }
                    buf[(i6 + 1) as usize] = b'.';
                    len = i6 + 2;
                    n = p9;
                }
                st = L::Ab9d;
            }

            L::B680 => {
                if (mode == 0x15 || mode == 0x11) && len == 1 {
                    let c = at(t, p);
                    if cls(c) & 0xc0 != 0 && cls(c) & 0x80 != 0 && cls(at(t, p14 + 2)) & 0x10 != 0 {
                        buf[1] = c;
                        len = 2;
                        n = p9;
                        st = L::Ab9d;
                        continue;
                    }
                }
                if mode != 2 {
                    if mode != 0x1b && mode != 0 {
                        st = L::B75b;
                        continue;
                    }
                    if strcmp(&buf, s(0x1009c3e0)) == 0 && upper(at(t, p)) == 0x4b && cls(at(t, p14 + 2)) & 0xc0 == 0 {
                        n += 2;
                        buf[(i6 + 1) as usize] = at(t, p8);
                        buf[(i6 + 2) as usize] = 0;
                    } else {
                        if strcmp(&buf, s(0x1009c3e0)) != 0 || at(t, p) != b'(' || upper(at(t, p14 + 2)) != 0x4b || at(t, p14 + 3) != b')' {
                            st = L::B75b;
                            continue;
                        }
                        buf[(i6 + 1) as usize] = at(t, p8);
                        buf[(i6 + 2) as usize] = at(t, p8 + 1);
                        n += 4;
                        buf[(i6 + 3) as usize] = at(t, p8 + 2);
                        buf[(i6 + 4) as usize] = 0;
                    }
                    st = L::Bca7;
                    continue;
                }
                if at(t, p) == b'.' && cls(at(t, p14 + 2)) & 0x10 != 0 {
                    buf[(i6 + 1) as usize] = b'.';
                    len = i6 + 2;
                    n = p9;
                    st = L::Ab9d;
                    continue;
                }
                st = L::Bcff;
            }

            L::B75b => {
                if mode != 0x1d {
                    st = L::Bcf5;
                    continue;
                }
                if cls(at(t, p)) & 0xc0 != 0 {
                    buf[(i6 + 1) as usize] = at(t, p);
                    len = i6 + 2;
                    n = p9;
                    st = L::Ab9d;
                    continue;
                }
                st = L::Bcff;
            }

            L::Bcf5 => {
                if mode == 0x1c {
                    st = L::Bede;
                    continue;
                }
                st = L::Bcff;
            }

            L::Bcff => {
                // Ordinal suffixes: 1st 2nd 3rd/3Rd 4th..9th 0th, and 11th 12th 13th.
                let last = buf[i6 as usize];
                let ok2 = has_n(t, p, 2);
                let p3 = at(t, p14 + 3);
                let tail = &t[(p as usize).min(t.len())..];
                let std_suffix = strchr_at(&buf, b'.').is_none()
                    && ok2
                    && (p3 == 0 || cls(p3) & 0xc0 == 0)
                    && ((last == b'1' && strnicmp(tail, s(0x1007ffd4), 2) == 0)
                        || (last == b'2' && strnicmp(tail, s(0x1009c3bc), 2) == 0)
                        || (last == b'3' && strnicmp(tail, s(0x1009c3b8), 2) == 0 && strncmp(tail, s(0x1009c3b4), 2) != 0)
                        || ((last == b'0' || (b'4'..=b'9').contains(&last)) && strnicmp(tail, s(0x10080b84), 2) == 0));
                if !std_suffix {
                    let teen = len >= 2
                        && strchr_at(&buf, b'.').is_none()
                        && ok2
                        && (p3 == 0 || cls(p3) & 0xc0 == 0)
                        && buf[(i6 - 1) as usize] == b'1'
                        && (last == b'1' || last == b'2' || last == b'3')
                        && strnicmp(tail, s(0x10080b84), 2) == 0;
                    if !teen {
                        st = L::Bede;
                        continue;
                    }
                    buf[(i6 + 1) as usize] = at(t, p);
                    buf[(i6 + 2) as usize] = at(t, p14 + 2);
                    st = L::Bed2;
                    continue;
                }
                // 11st / 12nd / 13rd are not ordinals: stop before the suffix.
                let bad_teen = len >= 2
                    && buf[(i6 - 1) as usize] == b'1'
                    && ((last == b'1' && strnicmp(tail, s(0x1007ffd4), 2) == 0)
                        || (last == b'2' && strnicmp(tail, s(0x1009c3bc), 2) == 0)
                        || (last == b'3' && strnicmp(tail, s(0x1009c3b8), 2) == 0));
                if !bad_teen {
                    buf[(i6 + 1) as usize] = at(t, p);
                    buf[(i6 + 2) as usize] = at(t, p14 + 2);
                    st = L::Bed2;
                    continue;
                }
                buf[(i6 + 1) as usize] = 0;
                n = p9;
                st = L::Digits2Finish;
                continue;
            }

            L::Bed2 => {
                len = i6 + 3;
                p9 = n + 3;
                st = L::Bede;
            }

            L::Bede => {
                n = p9;
                buf[len as usize] = 0;
                st = L::Digits2Finish;
            }

            L::Digits2Finish => {
                strcpy(&mut tok.text, &buf);
                tok.class = 2;
                st = L::C3f4;
            }

            L::Bca7 => {
                strcpy(&mut tok.text, &buf);
                tok.class = 1;
                st = L::C2e9;
            }

            L::NonAlnum => {
                if len > 0 && intok as i16 != 0 {
                    st = L::C3c3;
                    continue;
                }
                let c = at(t, p);
                let c1 = at(t, p + 1);
                if has_n(t, p, 2) && dbcs_pair(c, c1) {
                    tok.start = n + off;
                    if fw_letter(t, p) {
                        buf[len as usize] = c;
                        buf[(len + 1) as usize] = c1;
                        i6 = len + 2;
                        n += 2;
                        if i6 > 0x1b {
                            st = L::Bf95;
                            continue;
                        }
                        dp = (len + 2) as usize;
                        st = L::Bf5e;
                        continue;
                    }
                    if !fw_digit(t, p) {
                        buf[len as usize] = at(t, p8);
                        buf[(len + 1) as usize] = at(t, p8 + 1);
                        n += 2;
                        buf[(len + 2) as usize] = 0;
                        fold_dbcs(&mut buf, 3);
                        strcpy(&mut tok.text, &buf);
                        tok.class = 3;
                        tok.len = strlen(&tok.text) as i32;
                        let e = n + (tok.start - tok.ws);
                        tok.end = e;
                        tok.consumed = n;
                        st = L::C415;
                        continue;
                    }
                    buf[len as usize] = c;
                    buf[(len + 1) as usize] = c1;
                    i6 = len + 2;
                    n += 2;
                    if i6 > 0x1b {
                        st = L::C071;
                        continue;
                    }
                    dp = (len + 2) as usize;
                    st = L::C03a;
                    continue;
                }
                if (c != 0 && strchr(s(0x1009c3c0), c)) || (c != 0 && strchr(s(0x100813bc), c)) {
                    tok.start = n + off;
                    if c == b'<' && has_n(t, p, 5) && strncmp(&t[(p + 1) as usize..], s(0x10080c24), 2) == 0 && (at(t, p + 3) as i8) > 0 && (at(t, p + 3) as i8) < b'X' as i8 && at(t, p + 4) == b'>' {
                        let pv = n + 5;
                        buf[len as usize] = at(t, p8 + 3);
                        buf[(len + 1) as usize] = 0;
                        strcpy(&mut tok.text, &buf);
                        tok.class = 4;
                        tok.len = strlen(&tok.text) as i32;
                        tok.consumed = pv;
                        tok.end = pv + (tok.start - tok.ws);
                        ret = pv;
                        break;
                    }
                    if strncmp(s(0x10081190), &t[p as usize..], 5) == 0 {
                        let pv = n + 5;
                        buf[len as usize] = 0;
                        strcpy(&mut tok.text, &buf);
                        tok.class = 5;
                        tok.len = strlen(&tok.text) as i32;
                        tok.consumed = pv;
                        tok.end = pv + (tok.start - tok.ws);
                        ret = pv;
                        break;
                    }
                    if mode == 9 && c == b':' && has_n(t, p, 2) && at(t, p + 1) == b'/' && at(t, p + 2) == b'/' {
                        buf[len as usize] = at(t, p8);
                        buf[(len + 1) as usize] = at(t, p8 + 1);
                        n += 3;
                        buf[(len + 2) as usize] = at(t, p8 + 2);
                        buf[(len + 3) as usize] = 0;
                        st = L::C2bc;
                        continue;
                    }
                    if c == b'\\' {
                        st = L::C2a7;
                        continue;
                    }
                    if mode == 3 {
                        if c == b'\'' && has_n(t, p, 1) && cls(at(t, p + 1)) & 0x10 != 0 {
                            buf[len as usize] = c;
                            len += 1;
                            st = L::Ab9d;
                            continue;
                        }
                        if era() != crate::era::Era::Paul09 && len == 0 && c == b'\'' && lower(at(t, p + 1)) == 0x73 && cls(at(t, p + 2)) & 0xc0 == 0 {
                            buf[0] = b'\'';
                            st = L::B932;
                            continue;
                        }
                    } else if mode == 0xc {
                        if c == b'\'' && len == 0 && off > 0 && strchr_at(&buf, b'\'').is_none() && contraction(&mut c28, at(t, p - 1), t, p) {
                            strcpy(&mut buf, &c28);
                            let l = strlen(&c28) as i32;
                            n += l;
                            buf[l as usize] = 0;
                            st = L::C275;
                            continue;
                        }
                    } else {
                        if matches!(mode, 5 | 2 | 1 | 0)
                            && len == 0
                            && c == b'.'
                            && has_n(t, p, 1)
                            && cls(at(t, p + 1)) & 0x10 != 0
                            && (off == 0 || (off > 0 && cls(at(t, p - 1)) & 0xc0 == 0))
                        {
                            buf[0] = c;
                            st = L::B932;
                            continue;
                        }
                        if mode == 0x1c {
                            st = L::C2a7;
                            continue;
                        }
                    }
                    if c == 0 || !strchr(s(0x1009c3c0), c) {
                        st = L::Ba14;
                        continue;
                    }
                    buf[len as usize] = c;
                    st = L::Ba4e;
                    continue;
                }
                p += 1;
                n += 1;
                p8 = p;
                tok.ws += 1;
                st = L::A417;
            }

            L::C2a7 => {
                n += 1;
                buf[len as usize] = at(t, p8);
                buf[(len + 1) as usize] = 0;
                st = L::C2bc;
            }

            L::C2bc => {
                strcpy(&mut tok.text, &buf);
                tok.class = 3;
                st = L::C2e9;
            }

            L::C2e9 => {
                tok.len = strlen(&tok.text) as i32;
                tok.consumed = n;
                tok.end = n + (tok.start - tok.ws);
                st = L::C415;
            }

            L::B932 => {
                p += 1;
                len = 1;
                n += 1;
                p8 = p;
                intok = 1;
                st = L::A417;
            }

            L::Ba14 => {
                let c = at(t, p);
                if c == 0 || !strchr(s(0x100813bc), c) {
                    st = L::A417;
                    continue;
                }
                cl = 7;
                buf[len as usize] = c;
                len += 1;
                p += 1;
                n += 1;
                let mut run = 1;
                if len < 0x1d {
                    loop {
                        if at(t, p - 1) != at(t, p) {
                            break;
                        }
                        buf[len as usize] = at(t, p);
                        len += 1;
                        p += 1;
                        n += 1;
                        run += 1;
                        if len >= 0x1d {
                            break;
                        }
                    }
                    if run > 1 {
                        cl = if at(t, p - 1) != b'.' { 8 } else { 3 };
                    }
                }
                if mode == 0x12 && run == 1 {
                    cl = 3;
                }
                buf[len as usize] = 0;
                strcpy(&mut tok.text, &buf);
                tok.class = cl;
                tok.len = strlen(&tok.text) as i32;
                tok.end = n + (tok.start - tok.ws);
                tok.consumed = n;
                st = L::C415;
            }

            L::Ba4e => {
                len += 1;
                n += 1;
                if len > 0x1c {
                    buf[len as usize] = 0;
                    st = L::C275;
                    continue;
                }
                let c1 = at(t, p + 1);
                if at(t, p) != c1 {
                    buf[len as usize] = 0;
                    st = L::C275;
                    continue;
                }
                buf[len as usize] = c1;
                p += 1;
            }

            L::C275 => {
                strcpy(&mut tok.text, &buf);
                tok.class = 3;
                st = L::C3f4;
            }

            L::Bf5e => {
                p += 2;
                if !fw_letter(t, p) {
                    st = L::Bf95;
                    continue;
                }
                buf[dp] = at(t, p);
                buf[dp + 1] = at(t, p + 1);
                i6 += 2;
                dp += 2;
                n += 2;
                if i6 > 0x1b {
                    st = L::Bf95;
                    continue;
                }
            }

            L::Bf95 => {
                buf[i6 as usize] = 0;
                fold_dbcs(&mut buf, 1);
                strcpy(&mut tok.text, &buf);
                tok.class = 1;
                tok.consumed = n;
                tok.len = strlen(&tok.text) as i32;
                tok.end = n + (tok.start - tok.ws);
                st = L::C415;
            }

            L::C03a => {
                p += 2;
                if !fw_digit(t, p) {
                    st = L::C071;
                    continue;
                }
                buf[dp] = at(t, p);
                buf[dp + 1] = at(t, p + 1);
                i6 += 2;
                dp += 2;
                n += 2;
                if i6 > 0x1b {
                    st = L::C071;
                    continue;
                }
            }

            L::C071 => {
                buf[i6 as usize] = 0;
                fold_dbcs(&mut buf, 2);
                strcpy(&mut tok.text, &buf);
                tok.class = 2;
                tok.consumed = n;
                tok.len = strlen(&tok.text) as i32;
                tok.end = n + (tok.start - tok.ws);
                st = L::C415;
            }

            L::Baf7 => {
                tok.len = strlen(&tok.text) as i32;
                tok.consumed = n;
                tok.end = n + (tok.start - tok.ws);
                ret = n;
                break;
            }

            L::Ba9e => {
                tok.consumed = n;
                tok.len = strlen(&tok.text) as i32;
                tok.end = n + ((off + n) - tok.ws);
                ret = n;
                break;
            }

        }
    }
    let _ = (p9, i6, p14);
    ret
}
