//! The normalizers of `fe_NormalizeTokens`, one function per engine routine.
//! Signature: `(fe, wa, tok, t, p, off)` where `t[p..]` is the current text
//! position and `off` its offset inside the sentence (`local_8`, `ppbVar8`).

use super::cstr::*;
use super::sbd;
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::Fe;

/// `FUN_10062f10`: the record's text ends in `.?!;`.
fn ends_sbd(wa: &Wa, i: i32) -> bool {
    let txt = wa.text(i);
    let len = wa.ri32(i, 8);
    if txt.is_empty() {
        return false;
    }
    let c = at(txt, len as isize - 1);
    c != 0 && strchr(s(0x100813bc), c)
}

/// Record the sentence-final punctuation code on the previous word:
/// 3 question, 4 exclamation, 2 other.
fn mark_final(wa: &mut Wa, i7: i32, c: u8) {
    let o = 0x2c;
    if wa.ri32(i7 - 1, o) != 0 {
        return;
    }
    let v = match c {
        b'?' => 3,
        b'!' => 4,
        _ => 2,
    };
    wa.set_ri32(i7 - 1, o, v);
}

/// `FUN_10051a00`: sentence punctuation, paragraph breaks and end of text.
pub fn punct_runs(fe: &Fe, wa: &mut Wa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let i7 = wa.n() as i32;
    tokenize(tok, t, p, off, 1, fe.ud.as_ref());
    if tok.class == 8 {
        if i7 > 0 && !ends_sbd(wa, i7 - 1) {
            mark_final(wa, i7, tok.text[0]);
            return tok.consumed;
        }
        if i7 == 0 {
            return -tok.consumed;
        }
        return tok.consumed;
    }
    let l = strlen(&tok.text);
    let single = l == 1 && tok.text[0] != 0 && strchr(s(0x100813bc), tok.text[0]);
    let dots = l == 3 && strcmp(&tok.text, s(0x100813b8)) == 0;
    if single || dots {
        let r = sbd::decide(fe, tok, t, p, off);
        if r == b'P' {
            if i7 > 0 {
                mark_final(wa, i7, tok.text[0]);
                return tok.consumed;
            }
        } else if i7 < 1 || wa.ri32(i7 - 1, 4) != off + tok.ws || wa.ri32(i7 - 1, 0x28) == 0 {
            if tok.ws != 0 || tok.len > 2 || tok.text[0] != b'.' {
                return 0;
            }
        }
        return -tok.consumed;
    }
    if tok.class != 9 {
        if tok.class == 3 && tok.len == 2 && tok.text[0] == 0xa2 && tok.text[1] == 0xfd {
            if i7 > 0 && !ends_sbd(wa, i7 - 1) && wa.ri32(i7 - 1, 0x2c) == 0 {
                wa.set_ri32(i7 - 1, 0x2c, 0);
            }
            return tok.consumed;
        }
        return 0;
    }
    if i7 > 0 && !ends_sbd(wa, i7 - 1) && wa.ri32(i7 - 1, 0x2c) == 0 {
        wa.set_ri32(i7 - 1, 0x2c, 0);
    }
    if tok.consumed == 0 {
        return 1;
    }
    tok.consumed
}

/// `FUN_1003aa50`: 1/2 for a hyphenated compound (the prefix is/isn't a
/// lexicon word), 0 otherwise.
fn hyphen_kind(fe: &Fe, w: &[u8]) -> i16 {
    let h = match strchr_at(w, b'-') {
        Some(h) => h,
        None => return 0,
    };
    if h == 0 || at(w, h as isize + 1) == 0 {
        return 0;
    }
    if stricmp(w, s(0x10080388)) == 0 {
        return 0;
    }
    fe.lex_has_prefix(&w[..h]) as i16 + 1
}

/// `FUN_1003a880`: a plain word.
pub fn word(fe: &Fe, wa: &mut Wa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    tokenize(tok, t, p, off, 8, fe.ud.as_ref());
    if tok.len == 0 {
        return -1;
    }
    if tok.class != 1 {
        return tok.consumed;
    }
    let kind = hyphen_kind(fe, &tok.text);
    if kind == 0 {
        let b = if tok.f18 < 1 { 0x44 } else { 0x58 };
        let txt = tok.text;
        if !wa.emit(&txt, tok.start, tok.end, 0x41, b, 0x18) {
            return 0;
        }
        return tok.consumed;
    }
    let txt = tok.text;
    let ok = match fe.emb.get(&crate::lexicon::encode_key(cs(&txt))) {
        Some(payload) => {
            let e = crate::lexicon::decode_payload(payload);
            match e.prons.first() {
                Some((_, phones)) => wa.emit_phones(&txt, phones, tok.start, tok.end, 0x41, 0x59, 0x18),
                None if e.b80 != 0 => wa.emit(&txt, tok.start, tok.end, 0x41, 0x41, 0x18),
                None => split_hyphen(wa, &txt, tok.start, tok.end, 0x41, 0x44, 0x18, kind),
            }
        }
        // julie09 FUN_10037ab0 has no '@' look-ahead.
        None if fe.era.is_2009() => split_hyphen(wa, &txt, tok.start, tok.end, 0x41, 0x44, 0x18, kind),
        None => {
            let mut x = Tok::default();
            let c = tok.consumed as usize;
            tokenize(&mut x, t, p + c, off + tok.consumed, 8, fe.ud.as_ref());
            if x.ws == 0 && x.len == 1 && x.text[0] == b'@' {
                url_words(fe, wa, &txt, tok.start, tok.end, 0x44) != 0
            } else {
                split_hyphen(wa, &txt, tok.start, tok.end, 0x41, 0x44, 0x18, kind)
            }
        }
    };
    if !ok {
        return 0;
    }
    tok.consumed
}

/// `FUN_10063740`: every character a letter or `-`.
fn alpha_or_hyphen(w: &[u8]) -> bool {
    let w = cs(w);
    !w.is_empty() && w.iter().all(|&c| cls(c) & 0xc0 != 0 || c == b'-')
}

/// `FUN_1000ffd0`: no lowercase letter.
fn no_lowercase(w: &[u8]) -> bool {
    cs(w).iter().all(|&c| cls(c) & 0xc0 == 0 || cls(c) & 0x40 == 0)
}

/// `FUN_100457b0`: one record per hyphen-separated part, all on the word's
/// span. A one-letter part other than "a" is spelled (`b = 'S'`); an all-caps
/// part ending in "ers"/"er"/"s" gets the apostrophe `compound` inserts
/// ("CDs" -> "CD's"). After a known prefix (`kind` 2) each part followed by
/// its hyphen gets following-punctuation code 0xb.
#[allow(clippy::too_many_arguments)]
fn split_hyphen(wa: &mut Wa, text: &[u8], start: i32, end: i32, a: u8, b: u8, class: u32, kind: i16) -> bool {
    let text = cs(text);
    let mut part: Vec<u8> = Vec::new();
    for i in 0..=text.len() {
        let c = at(text, i as isize);
        if c != b'-' && c != 0 {
            part.push(c);
            continue;
        }
        let n = part.len();
        let ok = if n == 1 {
            let bb = if part[0] != b'a' { 0x53 } else { b };
            wa.emit(&part, start, end, a, bb, class)
        } else {
            let mut out = part.clone();
            let ends = |suf: &[u8]| n >= suf.len() && &part[n - suf.len()..] == suf;
            let fix = |cut: usize, rep: u32| -> Option<Vec<u8>> {
                let pre = &part[..n - cut];
                if n < 0x1d && pre.len() > 1 && alpha_or_hyphen(pre) && no_lowercase(pre) {
                    let mut v = pre.to_vec();
                    v.extend_from_slice(s(rep));
                    Some(v)
                } else {
                    None
                }
            };
            if n >= 5 && ends(b"ers") {
                if let Some(v) = fix(3, 0x10077894) {
                    out = v;
                }
            } else if n > 3 && ends(b"er") {
                if let Some(v) = fix(2, 0x1007789c) {
                    out = v;
                }
            } else if n > 2 && ends(b"s") {
                let pre = &part[..n - 1];
                let caps = pre.iter().filter(|&&x| cls(x) & 0x80 != 0).count();
                if n < 0x1d && alpha_or_hyphen(pre) && caps > 1 && cls(part[n - 2]) & 0x80 != 0 {
                    out = pre.to_vec();
                    out.extend_from_slice(s(0x100773a0));
                }
            }
            wa.emit(&out, start, end, a, b, class)
        };
        if !ok {
            return false;
        }
        if kind == 2 && c == b'-' {
            let n = wa.n() as i32;
            wa.set_ri32(n - 1, 0x2c, 0xb);
        }
        part.clear();
    }
    true
}

/// `FUN_100453a0`: read a URL/e-mail-like string piece by piece (token mode
/// 9): letter runs as words (spelled when shorter than 3), digit runs digit
/// by digit, "://" and the URL punctuation by name. Every record carries the
/// whole string's span. class 0x10.
pub fn url_words(fe: &Fe, wa: &mut Wa, text: &[u8], start: i32, end: i32, b: u8) -> i32 {
    let n = strlen(text) as i32;
    if n < 1 {
        return n;
    }
    let mut buf = text[..n as usize].to_vec();
    buf.extend_from_slice(&[0u8; 64]);
    let sym = super::tabs::rec30(0x10081b40, 0x1009b080);
    let set = s(0x100808a0);
    let mut pos = 0i32;
    loop {
        let mut x = Tok::default();
        let r = tokenize(&mut x, &buf, pos as usize, start + pos, 9, fe.ud.as_ref());
        let w = x.text();
        let c0 = at(w, 0);
        let ok = if super::sbd::all_alpha(w) {
            let bb = if x.len < 3 { 0x53 } else { b };
            wa.emit(w, start, end, 0x41, bb, 0x10)
        } else if super::sbd::all_digit(w) {
            let mut d = super::numwords::new_buf();
            super::numwords::digits(&mut d, w, 1, 0);
            wa.emit_split(&d, start, end, 0x44, 0x44, 0x10)
        } else if x.len == 1 && c0 != 0 && strchr(set, c0) && sym.find(w, true) >= 0 {
            wa.emit_split(sym.field(sym.find(w, true), 0x10), start, end, 0x53, 0x44, 0x10)
        } else if strcmp(w, s(0x1008089c)) == 0 {
            wa.emit_split(s(0x10080888), start, end, 0x53, 0x44, 0x10)
        } else if c0 != 0 && strchr(set, c0) && x.len > 1 {
            let one = [c0];
            let i = sym.find(&one, true);
            if i >= 0 {
                wa.emit_split(sym.field(i, 0x10), start, end, 0x53, 0x44, 0x10)
            } else {
                true
            }
        } else {
            true
        };
        if !ok {
            return 0;
        }
        if r == 0 {
            return n;
        }
        pos += r;
        if pos >= n {
            return n;
        }
    }
}

/// While some normalizers are still stubs the driver must not spin on a token
/// only they would take.
pub const UNPORTED_GUARD: bool = true;

/// `FUN_1003aad0` (math) never claims a token. Its entry gate `FUN_1003be50`
/// needs the previous WA record to be class 0x1d and to end where the token
/// starts, and the only class-0x1d emits in the DLL are math's own
/// (`FUN_1003b550`, `FUN_1003c020`; no direct store of 0x1d to +0x28 either).
/// Its only other effect, re-zeroing the shared scratch at WA+0x39e4, is
/// redone by every user on entry. Confirmed live: 0 wins in 220 calls over
/// the parity corpus's `math*.txt`.
pub fn math(fe: &Fe, wa: &mut Wa, _tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    if fe.era.is_2009() {
        return super::norm_math::math09(fe, wa, t, p, off);
    }
    -1
}

/// `FUN_1003d290`: a raw high byte (class 6) that is `©` or `¶`, by name.
pub fn symbols_cp(fe: &Fe, wa: &mut Wa, _tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let mut x = Tok::default();
    let r = super::tokenize::tokenize(&mut x, t, p, off, 0x14, fe.ud.as_ref());
    if x.len == 0 || x.class != 6 {
        return -1;
    }
    let w = match x.text[0] {
        0xa9 => s(0x100804fc),
        0xb6 => s(0x100804f0),
        // paul09 FUN_1003a200 also names the micro sign.
        0xb5 if era() == crate::era::Era::Paul09 => b"micro",
        _ => return -1,
    };
    if !wa.emit(w, x.start, x.end, 0x53, 0x44, 2) {
        return 0;
    }
    r
}

pub use super::norm_vtml::dispatch;

pub use super::norm_url::{tagmode_url, url};

pub use super::norm_digits::{digits_a, digits_b};

pub use super::norm_fraction::fraction;

pub use super::norm_compound::compound;

pub use super::norm_phone::phone;

pub use super::norm_address::address;

pub use super::norm_currency::currency;

pub use super::norm_time::time;

pub use super::norm_n15::n15;

pub use super::norm_numexpr::numexpr;

pub use super::norm_date::date;

pub use super::norm_units::units;

pub use super::norm_numword::numword;

pub use super::norm_symbols::symbols;

/// `FUN_1005deb0`: user-dictionary entry at `t[p..]` (after blanks), the
/// case-sensitive list first. The entry's own key (or "source" for keys of
/// 30+ bytes) is emitted as 'U'/'Y' class 0x19 with the target in +0x52, one
/// record per `\`-separated part (`FUN_100452c0`); an empty target is
/// consumed silently. `flag`, when set, is the end offset to record.
pub fn userdict(fe: &Fe, wa: &mut Wa, t: &[u8], p: usize, off: i32, flag: i16) -> i32 {
    let Some(ud) = fe.ud.as_ref() else {
        return -1;
    };
    let mut p = p;
    let mut off = off;
    let mut ws = 0;
    while matches!(at(t, p as isize), b' ' | b'\t' | b'\n' | b'\r') {
        p += 1;
        off += 1;
        ws += 1;
    }
    let e = match ud.find_s(t, p, flag) {
        i if i >= 0 => &ud.s.as_ref().unwrap().entries[i as usize],
        _ => match ud.find_i(t, p, flag) {
            j if j >= 0 => &ud.i.as_ref().unwrap().entries[j as usize],
            _ => return -1,
        },
    };
    let klen = e.key.len() as i32;
    if e.target.len() > 0x2cf {
        return -1;
    }
    if !e.target.is_empty() {
        let text: &[u8] = if klen < 0x1e { &e.key } else { b"source" };
        let end = if flag == 0 { klen + off } else { flag as i32 };
        for part in e.target.split(|&c| c == b'\\') {
            if !wa.emit_phones(text, part, off, end, 0x55, 0x59, 0x19) {
                return 0;
            }
        }
    }
    klen + ws
}

