//! Backward token context used by the recognizers: copy the words before the
//! current position (`FUN_10063300`) and re-tokenize them to get the last one,
//! two or three tokens (`FUN_10063510` / `FUN_10063590` / `FUN_10063660`).

use super::cstr::*;
use super::tokenize::{tokenize, Tok};

/// `FUN_10063300`: copy the last `nw` whitespace-separated words before
/// `t[p]` (not crossing `lo`) into `dst`. Returns bytes copied + trailing
/// whitespace skipped, or 0.
pub fn back_words(dst: &mut [u8], t: &[u8], lo: isize, p: isize, nw: i32) -> i32 {
    let mut trail = 0;
    let mut words = 1;
    let lim = nw * 0x1e - 1;
    if p == lo {
        return 0;
    }
    dst[0] = 0;
    let mut q = p;
    loop {
        q -= 1;
        if !is_ws(at(t, q)) {
            break;
        }
        if q <= lo {
            return 0;
        }
        trail += 1;
    }
    let mut count: i32 = 1;
    loop {
        if q <= lo {
            break;
        }
        let c = at(t, q);
        if is_ws(c) {
            if lim <= count || nw <= words {
                q += 1;
                count -= 1;
                break;
            }
            words += 1;
        } else if lim <= count {
            q += 1;
            count -= 1;
            break;
        }
        count += 1;
        q -= 1;
    }
    for k in 0..count as usize {
        dst[k] = at(t, q + k as isize);
    }
    dst[count as usize] = 0;
    count + trail
}

/// Walk the tokens of `buf`, calling `step(tok, is_last)` for each.
fn walk(buf: &[u8], off: i32, mode: i32, mut step: impl FnMut(&Tok, bool)) {
    let n = strlen(buf) as i32;
    let mut c = Tok::default();
    let mut used = 0;
    let mut q = 0usize;
    let mut o = off;
    while used < n {
        tokenize(&mut c, buf, q, o, mode, None);
        let r = c.consumed;
        step(&c, r + used >= n);
        q += r as usize;
        used += r;
        o += r;
        if r == 0 {
            break;
        }
    }
}

/// `FUN_10063660`: the last three tokens (oldest first).
pub fn last_three(buf: &[u8], off: i32, mode: i32) -> Option<(Tok, Tok, Tok)> {
    if strlen(buf) == 0 {
        return None;
    }
    let (mut a, mut b, mut c) = (Tok::default(), Tok::default(), Tok::default());
    walk(buf, off, mode, |t, last| {
        if !last {
            if b.len != 0 {
                a = b.clone();
            }
            if t.len != 0 {
                b = t.clone();
            }
        }
        c = t.clone();
    });
    Some((a, b, c))
}

/// `FUN_10063590`: (second-to-last qualifying token, last token). In mode 6
/// only words and numbers qualify as the earlier token.
pub fn last_two(buf: &[u8], off: i32, mode: i32) -> Option<(Tok, Tok)> {
    if strlen(buf) == 0 {
        return None;
    }
    let (mut a, mut b) = (Tok::default(), Tok::default());
    walk(buf, off, mode, |t, last| {
        if !last && t.len != 0 && (mode != 6 || t.class == 1 || t.class == 2) {
            a = t.clone();
        }
        b = t.clone();
    });
    Some((a, b))
}

/// `FUN_10063510`: the last token.
pub fn last_one(buf: &[u8], off: i32, mode: i32) -> Option<Tok> {
    if strlen(buf) == 0 {
        return None;
    }
    let mut b = Tok::default();
    walk(buf, off, mode, |t, _| b = t.clone());
    Some(b)
}

/// Previous-token context the way most recognizers fetch it: `nw` words
/// back from `t[p]` within the sentence starting `off` bytes earlier.
pub fn prev_one(t: &[u8], p: usize, off: i32, mode: i32) -> Tok {
    let mut buf = [0u8; 96];
    let r = back_words(&mut buf, t, p as isize - off as isize, p as isize, 1);
    if r as i16 != 0 {
        if let Some(b) = last_one(&buf, off - r, mode) {
            return b;
        }
    }
    Tok::default()
}

pub fn prev_two(t: &[u8], p: usize, off: i32, mode: i32) -> (Tok, Tok) {
    let mut buf = [0u8; 96];
    let r = back_words(&mut buf, t, p as isize - off as isize, p as isize, 2);
    if r as i16 != 0 {
        if let Some(ab) = last_two(&buf, off - r, mode) {
            return ab;
        }
    }
    (Tok::default(), Tok::default())
}
