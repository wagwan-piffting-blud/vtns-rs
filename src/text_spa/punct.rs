//! Sentence punctuation (`FUN_10041b60`) and Violeta's rule-based sentence
//! boundary test (`FUN_10041dd0` / `FUN_10041f80`; English uses a CART tree).

use super::tables::{CLS_S, HI};
use super::tokenize::{tokenize, Tok};
use super::{rec, FeSpa};
use crate::ling_spa::WaSpa;
use crate::text::cstr::{at, cs};

/// `DAT_10069d40`.
const SENT_PUNCT: &[u8] = b".?!;";

fn in_set(set: &[u8], c: u8) -> bool {
    c != 0 && set.contains(&c)
}

fn ws(c: u8) -> bool {
    matches!(c, b' ' | 9 | 10 | 13)
}

/// `FUN_1004b6e0`: the text's last character is sentence punctuation.
fn ends_punct(text: &[u8], len: i32) -> bool {
    !text.is_empty() && len >= 1 && in_set(SENT_PUNCT, at(text, (len - 1) as isize))
}

/// `FUN_1004bd70(out, start, cur, words)`: the `words` words before `cur`
/// (whitespace before it skipped); returns the text and the bytes back to
/// its start.
pub(super) fn prev_words(t: &[u8], start: usize, cur: usize, words: i32) -> (Vec<u8>, usize) {
    if cur == start {
        return (Vec::new(), 0);
    }
    let lim = (words * 0x1e - 1) as usize;
    let mut p = cur;
    let mut skipped = 0;
    loop {
        p -= 1;
        if !ws(t[p]) {
            break;
        }
        if p <= start {
            return (Vec::new(), 0);
        }
        skipped += 1;
    }
    let mut count = 1usize;
    let mut word_no = 1i32;
    if p > start {
        loop {
            let c = t[p];
            if ws(c) {
                if count >= lim || words <= word_no {
                    p += 1;
                    count -= 1;
                    break;
                }
                word_no += 1;
            } else if count >= lim {
                p += 1;
                count -= 1;
                break;
            }
            count += 1;
            p -= 1;
            if p <= start {
                break;
            }
        }
    }
    (t[p..p + count].to_vec(), count + skipped)
}

/// `FUN_1004bd70(out, start, cur, 1)`: the word before `cur`.
pub(super) fn prev_word(t: &[u8], start: usize, cur: usize) -> (Vec<u8>, usize) {
    prev_words(t, start, cur, 1)
}

/// `FUN_1004bf70`: the last mode-`mode` token of `s` (`norm_phone.rs` also
/// calls this with mode 6 for its trigger-word fold).
pub(super) fn last_token(s: &[u8], mode: i32) -> Option<Tok> {
    if s.is_empty() {
        return None;
    }
    let mut b = s.to_vec();
    b.extend_from_slice(&[0u8; 8]);
    let mut tok = Tok::default();
    let mut used = 0usize;
    while used < s.len() {
        let (t, r) = tokenize(&b, used, 0, mode);
        tok = t;
        used += r.max(1) as usize;
    }
    Some(tok)
}

struct Slot {
    ws: i32,
    len: i32,
    class: i32,
    text: Vec<u8>,
}

impl Slot {
    fn of(t: &Tok) -> Slot {
        Slot { ws: t.ws, len: t.len, class: t.class, text: t.text().to_vec() }
    }

    fn c0(&self) -> u8 {
        self.text.first().copied().unwrap_or(0)
    }
}

fn upper(c: u8) -> bool {
    CLS_S[c as usize] & 0x80 != 0
}

fn lower(c: u8) -> bool {
    CLS_S[c as usize] & 0x40 != 0
}

fn all_alpha(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&c| CLS_S[c as usize] & 0xc0 != 0 || (c > 0x7f && HI[c as usize] & 0xc0 != 0))
}

fn all_digit(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&c| CLS_S[c as usize] & 0x10 != 0)
}

/// `FUN_10041f80`: 'P' sentence end, 'N' not, 'A' accept (also an end).
fn rules(prev: &Slot, cur: &Slot, next: &Slot, after: u8) -> u8 {
    let c = cur.c0();
    if in_set(b"?!;", c) && cur.ws == 0 && cur.len == 1 {
        return b'P';
    }
    if c == b'.' && next.len == 0 && after != 0 && matches!(after, 9 | 10 | 13) {
        return b'P';
    }
    if next.len == 0 {
        return b'P';
    }
    if next.ws < 3 && next.class == 9 {
        return b'N';
    }
    if next.ws > 1 {
        return b'P';
    }
    let dot = cur.ws == 0 && cur.len == 1 && c == b'.';
    if prev.len > 0 {
        if upper(prev.c0()) && dot && upper(next.c0()) && next.ws == 0 {
            return b'N';
        }
        if lower(prev.c0()) && dot && lower(next.c0()) {
            return b'N';
        }
        if all_alpha(&prev.text) && dot && all_digit(&next.text) && next.ws == 0 {
            return b'N';
        }
    }
    if prev.len == 1 && upper(prev.c0()) && dot && next.ws <= 1 && next.len == 1 && upper(next.c0()) {
        return b'N';
    }
    b'A'
}

/// `FUN_10041dd0(tok, t + p, off)`: is `tok` (at `t[p..]`, sentence start at
/// `p - off`) a sentence end.
fn sentence_end(tok: &Tok, t: &[u8], p: usize, off: i32) -> bool {
    let empty = || Slot { ws: 0, len: 0, class: 0, text: Vec::new() };
    let mut prev = empty();
    let (w, back) = prev_word(t, p - off as usize, p);
    if back != 0 {
        match last_token(&w, 1) {
            Some(pt) => prev = Slot::of(&pt),
            None => return false,
        }
    }
    let cur = Slot::of(tok);
    let q = p + tok.consumed as usize;
    let (nt, _) = tokenize(t, q, tok.consumed + off, 1);
    let next = if nt.len > 0 { Slot::of(&nt) } else { empty() };
    matches!(rules(&prev, &cur, &next, at(t, q as isize)), b'P' | b'A')
}

fn prev_rec(wa: &WaSpa) -> Option<usize> {
    let n = wa.i16(0);
    if n > 0 {
        Some(WaSpa::rec(n as usize - 1))
    } else {
        None
    }
}

fn mark_prev(wa: &mut WaSpa, r: usize, v: i32) {
    if wa.i32(r + rec::PUNCT) == 0 {
        wa.set_i32(r + rec::PUNCT, v);
    }
}

/// `FUN_10041b60(wa, tok, t + p, off)`.
pub fn punct(_fe: &FeSpa, wa: &mut WaSpa, tok: &Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let prev = prev_rec(wa);
    let prev_open = |wa: &WaSpa, r: usize| !ends_punct(wa.cstr(r + rec::TEXT), wa.i32(r + rec::LEN));
    let q = tok.text().first().copied().unwrap_or(0) == b'?';
    match tok.class {
        9 => {
            if let Some(r) = prev {
                if prev_open(wa, r) {
                    mark_prev(wa, r, if q { 3 } else { 2 });
                    return tok.consumed;
                }
            } else {
                return -tok.consumed;
            }
            tok.consumed
        }
        8 if (tok.text().len() == 1 && in_set(SENT_PUNCT, tok.text()[0])) || cs(&tok.text) == b"..." => {
            if !sentence_end(tok, t, p, off) {
                let joined = prev.is_some_and(|r| wa.i32(r + rec::END) == tok.ws + off && wa.i32(r + rec::SRC) != 0);
                if !joined && (tok.ws != 0 || tok.len > 2 || tok.text()[0] != b'.') {
                    return 0;
                }
                return -tok.consumed;
            }
            let Some(r) = prev else { return -tok.consumed };
            mark_prev(wa, r, if q { 3 } else { 2 });
            tok.consumed
        }
        10 => {
            if let Some(r) = prev {
                if prev_open(wa, r) {
                    mark_prev(wa, r, 0);
                }
            }
            if tok.consumed == 0 {
                1
            } else {
                tok.consumed
            }
        }
        _ => 0,
    }
}
