//! `norm_alnum_code` (0x100276b0), the last recognizer before the plain word: capitals with a plural
//! (`CDs` as "CD's", `MPers`), and a single letter read as a letter (`I`, `a)`, `x 5`).

use super::mem::*;
use super::normalize::{Cx, State};
use super::rx::*;
use super::stream::token_at;
use super::tokenize::Tok;

/// Letters that read as words on their own (`DAT_10065936`, by folded letter).
fn letter_word(c: u8) -> bool {
    let f = fold_s(c);
    let m = b(0x10065836, 768);
    let o = (256 + 2 * f) as usize;
    o + 1 < m.len() && i16::from_le_bytes([m[o], m[o + 1]]) != 0
}

/// `FUN_1000cb70`: no lower-case letter.
fn no_lower(t: &[u8]) -> bool {
    !t.is_empty() && t.iter().all(|&c| !(cls(c) & 0xc0 != 0 && cls(c) & 0x40 != 0))
}

/// `FUN_1004fa00`: all lower-case letters.
fn all_lower(t: &[u8]) -> bool {
    let t = cs(t);
    !t.is_empty() && t.iter().all(|&c| cls(c) & 0x40 != 0)
}

fn uppers(t: &[u8]) -> usize {
    t.iter().filter(|&&c| cls(c) & 0x80 != 0).count()
}

pub fn alnum_code(cx: &Cx, st: &mut State, cur: &mut usize, _t: &Tok) -> i32 {
    let t = cur_tok(st);
    if t.ty != 1 {
        return 5;
    }
    let sv = save(st);
    let w = t.text();
    let l = w.len();
    let done = |st: &mut State, cur: &mut usize| {
        let r = st.done.append_all(&st.ahead, 0);
        if r == 1 {
            return 1;
        }
        restore(st, cur, sv, r)
    };
    for (suf, min, mark) in [(&b"ers"[..], 4usize, &b"'ers"[..]), (b"er", 3, b"'er")] {
        if l > min && &w[l - suf.len()..] == suf {
            let stem = &w[..l - suf.len()];
            if stem.len() > 1 && no_lower(stem) {
                let x = [stem, mark].concat();
                if st.wa.emit(&x, t.start, t.end, b'A', b'D') == 1 {
                    return done(st, cur);
                }
            }
        }
    }
    if l > 2 && w[l - 1] == b's' {
        let stem = &w[..l - 1];
        if uppers(stem) > 1 && cls(stem[stem.len() - 1]) & 0x80 != 0 {
            let x = [stem, b"'s"].concat();
            if st.wa.emit(&x, t.start, t.end, b'A', b'D') == 1 {
                return done(st, cur);
            }
        }
    }
    let not_mine = |st: &mut State, cur: &mut usize| {
        if st.ahead.n != sv.tn {
            st.ahead.n = sv.tn;
            if let Some(p) = st.ahead.pos[sv.tn as usize] {
                *cur = p;
            }
        }
        5
    };
    if !(l == 1 && letter_word(t.text[0])) {
        return not_mine(st, cur);
    }
    let n = st.ahead.n - 1;
    let p11 = token_at(&st.done, &st.ahead, n, -1);
    let p12 = token_at(&st.done, &st.ahead, n, -2);
    let sym = |x: &Tok, c: u8| (x.ty == 3 || x.ty == 7) && x.len == 1 && x.text[0] == c;
    let opening = p11.filter(|p| p.len == 1 && t.ws == 0 && p.text[0] != 0 && s(0x1006effc).contains(&p.text[0]));
    if let Some(p) = opening {
        let close = match p.text[0] {
            b'<' => b'>',
            b'(' => b')',
            b'[' => b']',
            b'{' => b'}',
            c => c,
        };
        let Some(x) = push(cx, st, cur) else { return restore(st, cur, sv, 4) };
        if !(x.ws == 0 && sym(&x, close)) {
            pop(st, cur);
            return not_mine(st, cur);
        }
    } else {
        let after_word = cls(t.text[0]) & 0x80 != 0
            && p11.is_some_and(|p| t.ws <= 1 && (p.ty == 2 || all_lower(&p.text)));
        let after_dash = p12.is_some_and(|q| q.ty == 2) && p11.is_some_and(|p| p.ws == 0 && sym(&p, b'-')) && t.ws == 0;
        if !after_word && !after_dash {
            let Some(x) = push(cx, st, cur) else { return restore(st, cur, sv, 4) };
            let mut ok = false;
            if x.ws == 0 {
                if x.ty == 2 {
                    ok = true;
                } else if x.ty == 3 || x.ty == 7 {
                    let Some(y) = push(cx, st, cur) else { return restore(st, cur, sv, 4) };
                    ok = y.ws == 0 && y.ty == 2;
                    let x2 = pop(st, cur);
                    if x2.len == 1 && x2.text[0] != 0 && b"-+*$&~#_=|\\;:.".contains(&x2.text[0]) {
                        ok = true;
                    }
                }
            }
            pop(st, cur);
            if !ok {
                return not_mine(st, cur);
            }
        }
    }
    if st.wa.emit(t.text(), t.start, t.end, b'A', b'S') == 1 {
        return done(st, cur);
    }
    not_mine(st, cur)
}
