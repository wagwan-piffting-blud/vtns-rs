//! The last normalizer in the chain: plain words (`FUN_10042180`).

use super::tables::{CLS_S, HI};
use super::tokenize::{tokenize, Tok};
use super::{emit, FeSpa};
use crate::ling_spa::lexicon::{decode_payload_spa, encode_key_spa, stricmp_spa, vowel_count_spa};
use crate::ling_spa::WaSpa;

fn is_upper(c: u8) -> bool {
    CLS_S[c as usize] & 0x80 != 0 || (c > 0x7f && HI[c as usize] & 0x80 != 0)
}

fn is_lower(c: u8) -> bool {
    CLS_S[c as usize] & 0x40 != 0 || (c > 0x7f && HI[c as usize] & 0x40 != 0)
}

fn is_alpha(c: u8) -> bool {
    CLS_S[c as usize] & 0xc0 != 0 || (c > 0x7f && HI[c as usize] & 0xc0 != 0)
}

/// Accent restoration for a few unaccented spellings (`DAT_10079c88`, sorted).
const RESTORE: [(&[u8], &[u8]); 8] = [
    (b"aqui", b"aqu\xed"),
    (b"dieciseis", b"diecis\xe9is"),
    (b"gonzalez", b"gonz\xe1lez"),
    (b"ramirez", b"ram\xedrez"),
    (b"veintidos", b"veintid\xf3s"),
    (b"veintiseis", b"veintis\xe9is"),
    (b"veintitres", b"veintitr\xe9s"),
    (b"veintiun", b"veinti\xfan"),
];

/// `FUN_100437a0` on `RESTORE` (case-insensitive binary search).
fn restore_find(w: &[u8]) -> Option<usize> {
    if w.is_empty() {
        return None;
    }
    let (mut lo, mut hi) = (0i32, RESTORE.len() as i32 - 1);
    let mut mid = hi;
    while lo <= hi {
        mid /= 2;
        let c = stricmp_spa(RESTORE[mid as usize].0, w);
        if c == 0 {
            return Some(mid as usize);
        }
        if c < 0 {
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
        mid = hi + lo;
    }
    None
}

/// `FUN_10042390`: a hyphen that is neither first nor last.
fn inner_hyphen(w: &[u8]) -> bool {
    match w.iter().position(|&c| c == b'-') {
        Some(k) => k > 0 && k + 1 < w.len(),
        None => false,
    }
}

/// `FUN_100423c0`: mixed case inside the word (a case change after the first
/// letters), letters and 's-possessives only.
fn mixed_case(w: &[u8]) -> bool {
    let n = w.len();
    let mut state = 0i32;
    let mut change = None;
    for (i, &c) in w.iter().enumerate() {
        if is_upper(c) {
            if state == -1 {
                change = Some(i);
                break;
            }
            state = 1;
        } else if is_lower(c) {
            if state == 1 {
                change = Some(i);
                break;
            }
            state = -1;
        }
    }
    if change.is_none() {
        return false;
    }
    let at = |k: usize| w.get(k).copied().unwrap_or(0);
    let mut i = 0usize;
    while i < n {
        let c = at(i);
        if CLS_S[c as usize] & 0xc0 == 0 {
            if c != b'\'' {
                return false;
            }
            let s_upper = i + 2 < n && at(i + 1) == b's' && is_upper(at(i + 2));
            if !s_upper && !(i >= 1 && i + 1 < n && at(i - 1) == b's' && is_upper(at(i + 1))) {
                return false;
            }
        }
        i += 1;
    }
    true
}

/// `FUN_100424f0`: the next same-case run of `w` from `*k`.
fn case_run(w: &[u8], k: &mut usize) -> Vec<u8> {
    let n = w.len();
    let at = |j: usize| w.get(j).copied().unwrap_or(0);
    let mut out = vec![at(*k)];
    let mut upper_run = is_upper(at(*k));
    *k += 1;
    while *k <= n {
        let c = at(*k);
        if c == 0 {
            break;
        }
        if upper_run {
            upper_run = is_upper(c);
            out.push(c);
            *k += 1;
        } else if is_lower(c) {
            out.push(c);
            upper_run = false;
            *k += 1;
        } else {
            if c != b'\'' {
                break;
            }
            if *k + 2 < n && at(*k + 1) == b's' && is_upper(at(*k + 2)) {
                out.push(c);
                out.push(b's');
                *k += 2;
                return out;
            }
            if *k > 0 && *k + 1 < n && at(*k - 1) == b's' && is_upper(at(*k + 1)) {
                *k += 1;
                return out;
            }
            // The engine loops forever here; mixed_case() rules this out.
            break;
        }
    }
    out
}

/// `FUN_10042180(wa, tok, t + p, off)`.
pub fn word(fe: &FeSpa, wa: &mut WaSpa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let (tk, _) = tokenize(t, p, off, 8);
    *tok = tk;
    let w = tok.text().to_vec();
    let (s, e) = (tok.start, tok.end);
    if tok.len == 0 {
        return -1;
    }
    // Anything but a word is consumed silently.
    if tok.class != 1 {
        return tok.consumed;
    }
    let ok =if tok.len == 1 && w[0] != b'y' && w[0] != b'Y' {
        emit(wa, &w, s, e, b'A', b'S', 1, None)
    } else if tok.len > 1 && w.iter().all(|&c| is_alpha(c)) && vowel_count_spa(&w) == 0 {
        emit(wa, &w, s, e, b'A', b'S', 1, None)
    } else if !inner_hyphen(&w) {
        if mixed_case(&w) {
            let mut k = 0usize;
            while k < w.len() {
                let part = case_run(&w, &mut k);
                if !emit(wa, &part, s, e, b'A', b'D', 1, None) {
                    return 0;
                }
            }
            return tok.consumed;
        }
        match restore_find(&w) {
            Some(i) => emit(wa, RESTORE[i].1, s, e, b'A', b'D', 1, None),
            None => emit(wa, &w, s, e, b'A', b'D', 1, None),
        }
    } else {
        let key = encode_key_spa(&w);
        let pron = fe.lex.lookup_encoded(&key).map(|p| decode_payload_spa(p).pron).unwrap_or_default();
        if pron.is_empty() {
            let mut ok = true;
            for part in w.split(|&c| c == b'-') {
                if !emit(wa, part, s, e, b'A', b'D', 1, None) {
                    ok = false;
                    break;
                }
            }
            ok
        } else {
            emit(wa, &w, s, e, b'A', b'U', 1, Some(&pron))
        }
    };
    if !ok {
        return 0;
    }
    tok.consumed
}
