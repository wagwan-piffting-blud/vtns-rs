//! `FUN_10063780` (dispatch order 19, receives the token `symbols` left,
//! mode 0x12): two unrelated features glued into one function.
//!
//! **Ported**: an all-caps letter/hyphen word directly followed by a
//! lowercase `s`/`er`/`ers` suffix (`"CDs"`, `"ATMs"`, `"RVers"`, ...) is
//! rewritten with an apostrophe inserted before the suffix (`"CD's"`,
//! `"RV'ers"`), so the linguistic stage spells the prefix letter by letter
//! and reads the suffix as its own possessive/agentive tail instead of
//! trying to pronounce the whole glued string as one word.
//!
//! **`single_letter`**: a lone letter flagged in `DAT_10077d5e` (a/e/i/o/u/y)
//! is spelled when bracketed or quoted (open set `<([{-'"` at `0x1009c82c`,
//! matching close after it), capitalized after a lowercase word or number,
//! capitalized between a capitalized and a lowercase word, or followed by
//! punctuation from `0x1009c818` / a number; "e-mail"/"e-commerce" and
//! "a-<word>" decline. Engine dead code, kept: the number-dash-letter
//! shortcut (the bracket gate, whose open set includes `-`, always wins
//! first), and letter-glued-digits ("a2"), which `n15`'s UGC recognizer
//! (order 16) always takes. Confirmed against the parity corpus's
//! `item13_compound.txt`.

use super::cstr::*;
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::window::{back_words, last_two};
use super::Fe;

/// `FUN_10063740`: every character is a letter or `-`.
fn alpha_or_hyphen(w: &[u8]) -> bool {
    let w = cs(w);
    !w.is_empty() && w.iter().all(|&c| cls(c) & 0xc0 != 0 || c == b'-')
}

/// `FUN_1000ffd0`: no lowercase letter anywhere (all-caps, or no letters).
fn no_lowercase(w: &[u8]) -> bool {
    let w = cs(w);
    w.iter().all(|&c| cls(c) & 0xc0 == 0 || cls(c) & 0x40 == 0)
}

const SUFFIXES: [(&[u8], u32); 3] = [(b"ers", 0x10077894), (b"er", 0x1007789c), (b"s", 0x100773a0)];

pub fn compound(fe: &Fe, wa: &mut Wa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let _ = tok;
    let mut x = Tok::default();
    let r = tokenize(&mut x, t, p, off, 0, fe.ud.as_ref());
    if r == 0 || x.class == 9 || x.class != 1 {
        return -1;
    }
    let w = cs(&x.text);
    for &(suf, repl) in &SUFFIXES {
        if w.len() <= suf.len() + 1 || !w.ends_with(suf) {
            continue;
        }
        let prefix = &w[..w.len() - suf.len()];
        if !alpha_or_hyphen(prefix) || !no_lowercase(prefix) {
            continue;
        }
        let mut buf = [0u8; 40];
        strcpy(&mut buf, prefix);
        strcat(&mut buf, s(repl));
        if !wa.emit(&buf, x.start, x.end, 0x41, 0x44, 0x18) {
            return 0;
        }
        return r;
    }
    if x.len == 1 && letter_flag(x.text[0]) {
        return single_letter(fe, wa, &x, r, t, p, off);
    }
    -1
}

/// `*(short *)(&DAT_10077d5e + tolower(c) * 2) != 0`: a/e/i/o/u/y (and some
/// non-letters) -- the letters that also read as words.
fn letter_flag(c: u8) -> bool {
    let v = lower(c) as i32;
    let m = b(0x10077c5e, 768);
    let o = ((v + 128) * 2) as usize;
    o + 1 < m.len() && i16::from_le_bytes([m[o], m[o + 1]]) != 0
}

/// `FUN_10063170`: every character a lowercase letter.
fn all_lower(w: &[u8]) -> bool {
    let w = cs(w);
    !w.is_empty() && w.iter().all(|&c| cls(c) & 0x40 != 0)
}

/// The single-letter half of `FUN_10063780` (from `0x10063a74`, ported from
/// the disassembly): a lone a/e/i/o/u/y is spelled (`b = 'S'`) when its
/// context says it's a letter rather than a word -- bracketed ("(a)"),
/// capitalized after a lowercase word ("and I"), before a number or a
/// closing symbol, and so on.
fn single_letter(fe: &Fe, wa: &mut Wa, x: &Tok, r1: i32, t: &[u8], p: usize, off: i32) -> i32 {
    let mut buf = [0u8; 96];
    let bw = back_words(&mut buf, t, p as isize - off as isize, p as isize, 2);
    let (a, bt) = if bw as i16 != 0 { last_two(&buf, off - bw, 0).unwrap_or_default() } else { Default::default() };
    let c0 = x.text[0];
    let up = cls(c0) & 0x80 != 0;
    let tk = |q: usize, o: i32| {
        let mut y = Tok::default();
        tokenize(&mut y, t, q, o, 0, fe.ud.as_ref());
        y
    };
    let emit = |wa: &mut Wa, n: i32| -> i32 {
        if n == 0 {
            return -1;
        }
        if wa.emit(&x.text, x.start, x.end, 0x41, 0x53, 0x18) {
            n
        } else {
            0
        }
    };
    let b0 = bt.text[0];
    if bt.len == 1 && b0 != 0 && strchr(s(0x1009c82c), b0) && x.ws == 0 {
        let close = match b0 {
            b'<' => b'>',
            b'(' => b')',
            b'[' => b']',
            b'{' => b'}',
            o => o,
        };
        let t2 = tk(p + r1 as usize, off + r1);
        if (t2.class != 3 && t2.class != 7) || t2.len != 1 || t2.text[0] != close || t2.ws != 0 {
            return -1;
        }
        if close == b'-' && a.len > 0 {
            let n = wa.n() as i32;
            if n > 0 {
                let cl = wa.ri32(n - 1, 0x28);
                if (cl == 3 || cl == 0xf) && wa.ri32(n - 1, 4) == a.end && bt.ws == 0 {
                    return -1;
                }
            }
            if a.class == 2 {
                return -1;
            }
        }
        return emit(wa, r1);
    }
    if up && x.ws < 2 && bt.len > 0 && (bt.class == 2 || all_lower(&bt.text)) {
        return emit(wa, r1);
    }
    if a.len > 0 && a.class == 2 && bt.len > 0 && (bt.class == 3 || bt.class == 7) && bt.len == 1 && b0 == b'-' && bt.ws == 0 && x.ws == 0 {
        return emit(wa, r1);
    }
    let (q, qo) = (p + r1 as usize, off + r1);
    let mut t2 = Tok::default();
    let r2 = tokenize(&mut t2, t, q, qo, 0, fe.ud.as_ref());
    if t2.class == 2 && t2.ws == 0 {
        return emit(wa, x.consumed);
    }
    if up && bt.len > 0 && bt.class == 1 && cls(b0) & 0x80 != 0 && x.ws < 2 && t2.len > 0 && t2.class == 1 && all_lower(&t2.text) && t2.ws < 2 {
        return emit(wa, x.consumed);
    }
    if (t2.class != 3 && t2.class != 7) || t2.ws != 0 {
        return -1;
    }
    let t3 = tk(q + r2 as usize, qo + r2);
    let c2 = t2.text[0];
    if t2.len == 1 && c2 != 0 && strchr(s(0x1009c818), c2) {
        let u = upper(c0);
        if u == 0x45 && c2 == b'-' && t2.ws == 0 && (stricmp(t3.text(), s(0x1009c810)) == 0 || stricmp(t3.text(), s(0x1009c804)) == 0) && t3.ws == 0 {
            return -1;
        }
        if u == 0x41 && c2 == b'-' && t2.ws == 0 && t3.len > 1 && t3.ws == 0 {
            return -1;
        }
        return emit(wa, x.consumed);
    }
    if t3.class != 2 || t3.ws != 0 {
        return -1;
    }
    emit(wa, x.consumed)
}
