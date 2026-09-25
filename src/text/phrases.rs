//! Multi-word context tables: `FUN_100564e0` finds a table phrase that the
//! word sequence starts, `FUN_100633d0` collects the words before a position,
//! `FUN_10041f30` / `FUN_100580e0` test the surrounding words against tables.

use super::cstr::*;
use super::tabs::Ptab;
use super::tokenize::{tokenize, Tok};
use super::window::{back_words, prev_two};
use super::Fe;

/// `FUN_100564e0`: index of the table phrase matching `words[k..]` for the
/// first `k` that matches, or -1. Phrases are space-separated; each word is
/// compared as a prefix of its phrase slot.
pub fn phrase_find(tab: Ptab, words: &[Option<Vec<u8>>], insensitive: bool) -> i32 {
    let cmp = |a: &[u8], b: &[u8], n: usize| if insensitive { strnicmp(a, b, n) } else { strncmp(a, b, n) };
    let n = tab.count();
    let nw = words.len();
    if words.first().map_or(true, |w| w.is_none()) {
        return -1;
    }
    // Continue matching words[k+1..] against the rest of phrase `ph` after `l`.
    // Returns Ok(true) full match, Ok(false) stop, Err(c) mismatch with sign c.
    let cont = |ph: &[u8], l: usize, k: usize| -> Result<bool, i32> {
        let mut rest: Vec<u8> = cs(&ph[l + 1..]).to_vec();
        rest.push(0);
        let mut pos = 0usize;
        let mut j = k + 1;
        while j < nw {
            let w = match &words[j] {
                Some(w) => w,
                None => return Ok(false),
            };
            let wl = strlen(w);
            let c = cmp(&rest[pos..], w, wl);
            if c != 0 {
                return Err(c);
            }
            let ch = at(&rest, (pos + wl) as isize);
            if ch == 0 {
                return Ok(true);
            }
            if ch != b' ' {
                return Ok(false);
            }
            j += 1;
            pos += wl + 1;
        }
        Ok(false)
    };
    if n < 0 {
        for k in 0..nw {
            let w = match &words[k] {
                Some(w) => w,
                None => return -1,
            };
            let l = strlen(w);
            if l == 0 {
                return -1;
            }
            for i in 0..-n {
                let ph = tab.get(i);
                if cmp(ph, w, l) == 0 {
                    let ch = at(ph, l as isize);
                    if ch == 0 {
                        return i;
                    }
                    if ch == b' ' && cont(ph, l, k) == Ok(true) {
                        return i;
                    }
                }
            }
        }
        return -1;
    }
    for k in 0..nw {
        let w = match &words[k] {
            Some(w) => w,
            None => return -1,
        };
        let l = strlen(w);
        if l == 0 {
            return -1;
        }
        let mut hi = n - 1;
        let mut lo = 0;
        let mut mid = hi;
        while hi >= 0 {
            mid /= 2;
            let ph = tab.get(mid);
            let c = cmp(ph, w, l);
            let mut go_low = true;
            if c == 0 {
                let ch = at(ph, l as isize);
                if ch == 0 {
                    return mid;
                }
                if ch == b' ' {
                    match cont(ph, l, k) {
                        Ok(true) => return mid,
                        Ok(false) => {}
                        Err(c2) => go_low = c2 >= 0,
                    }
                }
            } else {
                go_low = c >= 0;
            }
            if go_low {
                hi = mid - 1;
            } else {
                lo = mid + 1;
            }
            mid = hi + lo;
            if lo > hi {
                break;
            }
        }
    }
    -1
}

/// `FUN_100633d0`: up to five token texts before `t[p]`, nearest first.
pub fn words_before(t: &[u8], p: usize, off: i32, mode: i32) -> Vec<Vec<u8>> {
    let mut buf = [0u8; 160];
    let r = back_words(&mut buf, t, p as isize - off as isize, p as isize, 5);
    let mut arr: Vec<Vec<u8>> = vec![Vec::new(); 5];
    if r as i16 == 0 || buf[0] == 0 {
        return arr;
    }
    let n = strlen(&buf) as i32;
    let mut cnt = 0usize;
    let mut used = 0;
    let mut tok = Tok::default();
    while used < n {
        tokenize(&mut tok, &buf, used as usize, used, mode, None);
        if cnt > 0 && cnt < 5 {
            for k in (1..=cnt).rev() {
                arr[k] = arr[k - 1].clone();
            }
        }
        arr[0] = tok.text().to_vec();
        if cnt < 4 {
            cnt += 1;
        }
        used += tok.consumed;
        if tok.consumed == 0 {
            break;
        }
    }
    arr
}

/// `FUN_10041f30`: the two words before (`L`/`B`) or after (`R`/`B`) the
/// numeral, looked up as a phrase in the score/ratio table.
pub fn score_context(fe: &Fe, t: &[u8], p: usize, off: i32, mode: u8, skip: i32) -> bool {
    let mut before: Vec<Option<Vec<u8>>> = vec![None, None];
    if mode == b'L' || mode == b'B' {
        let (a, b) = prev_two(t, p, off, 0);
        if b.len > 0 {
            before[0] = Some(b.text().to_vec());
            if a.len > 0 {
                before[1] = Some(a.text().to_vec());
            }
        }
    }
    let mut after: Vec<Option<Vec<u8>>> = vec![None, None];
    if mode == b'R' || mode == b'B' {
        let mut q = skip;
        for slot in after.iter_mut() {
            let mut tok = Tok::default();
            let r = tokenize(&mut tok, t, p + q as usize, off + q, 0, fe.ud.as_ref());
            if r == 0 || tok.class == 8 || tok.class == 9 {
                break;
            }
            *slot = Some(tok.text().to_vec());
            q += r;
        }
    }
    let tab = super::tabs::ptab(0x1009adf4, 0x1009b128);
    phrase_find(tab, &before, true) >= 0 || phrase_find(tab, &after, true) >= 0
}

/// `FUN_100580e0`: telephone context: a phone word among the five words
/// before, or an extension word among the five tokens after. The first token
/// after is returned through `first`.
pub fn phone_context(fe: &Fe, t: &[u8], p: usize, off: i32, mode: u8, skip: i32, first: Option<&mut Tok>) -> bool {
    let mut before: Vec<Option<Vec<u8>>> = vec![None; 5];
    if mode == b'L' || mode == b'B' {
        let w = words_before(t, p, off, 6);
        for (i, x) in w.into_iter().enumerate().take(5) {
            if x.is_empty() {
                break;
            }
            before[i] = Some(x);
        }
    }
    let mut after: Vec<Option<Vec<u8>>> = vec![None; 5];
    let mut first_tok: Option<Tok> = None;
    if mode == b'R' || mode == b'B' {
        let mut q = skip;
        for k in 0..5 {
            let mut tok = Tok::default();
            let r = tokenize(&mut tok, t, p + q as usize, off + q, 6, fe.ud.as_ref());
            if r == 0 {
                break;
            }
            if k == 0 {
                first_tok = Some(tok.clone());
            }
            if tok.class == 8 || tok.class == 9 {
                break;
            }
            after[k] = Some(tok.text().to_vec());
            q += r;
        }
    }
    if let (Some(f), Some(t0)) = (first, first_tok) {
        *f = t0;
    }
    if let Some(Some(w0)) = before.first() {
        if stricmp(w0, s(0x10077694)) == 0 {
            return true;
        }
    }
    phrase_find(super::tabs::ptab(0x1008ab58, 0x1009b06c), &before, true) >= 0 || phrase_find(super::tabs::ptab(0x1008ab48, 0x1009b068), &after, true) >= 0
}
