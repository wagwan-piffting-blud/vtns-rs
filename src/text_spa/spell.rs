//! `FUN_10035880`: re-tokenize a short text buffer (mode 9) and emit each
//! piece -- letters, digit runs, `"://"`, or a single symbol/repeated-symbol
//! run named from `tables_oth::SYMBOL_NAMES` -- trying the user-dict
//! pre-pass on each piece first. Used by tag mode and (inside VTML) by
//! `FUN_10040d70`.

use super::emit::emit_split;
use super::tables::{CLS_S, HI};
use super::tables_oth::{symbol_name_index, SYMBOL_NAMES, WORDS_PUNCT};
use super::tokenize::tokenize;
use super::userdict_spa::prepass;
use super::{emit, FeSpa};
use crate::ling_spa::WaSpa;

fn cut0(s: &[u8]) -> &[u8] {
    match s.iter().position(|&c| c == 0) {
        Some(i) => &s[..i],
        None => s,
    }
}

fn is_all_alpha(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&c| CLS_S[c as usize] & 0xc0 != 0 || (c > 0x7f && HI[c as usize] & 0xc0 != 0))
}

fn is_all_digit(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&c| CLS_S[c as usize] & 0x10 != 0)
}

/// `FUN_10035880(wa, text, start, end, _origin[unused], default_case)`.
/// `start` is passed to the pre-pass unchanged every iteration (a faithfully
/// reproduced quirk: only the tokenizer sees the advancing offset). Returns
/// the original `strlen(text)` once fully consumed, or 0 (full WA / empty
/// input) -- callers only ever check `== 0`.
pub fn expand(fe: &FeSpa, wa: &mut WaSpa, text: &[u8], start: i32, end: i32, default_case: u8) -> i32 {
    let text = cut0(text);
    let total = text.len() as i32;
    if total < 1 {
        return total;
    }
    let mut cursor = start;
    let mut pos = 0usize;
    let mut done = 0i32;
    loop {
        let (tok, r) = tokenize(text, pos, cursor, 9);
        let pre = prepass(fe, wa, text, pos, start, end as i16);
        let consumed = if pre >= 0 {
            pre
        } else {
            let tt = tok.text();
            if is_all_alpha(tt) {
                let case = if tok.len < 3 { b'S' } else { default_case };
                if !emit(wa, tt, start, end, b'A', case, 8, None) {
                    return 0;
                }
            } else if is_all_digit(tt) {
                if !emit_split(wa, tt, start, end, b'D', b'D', 8) {
                    return 0;
                }
            } else {
                let c0 = tt.first().copied().unwrap_or(0);
                if tok.len == 1 && c0 != 0 && WORDS_PUNCT.contains(&c0) {
                    let idx = symbol_name_index(c0);
                    if idx >= 0 && !emit_split(wa, SYMBOL_NAMES[idx as usize].1, start, end, b'S', b'D', 8) {
                        return 0;
                    }
                } else if tt == b"://" {
                    if !emit_split(wa, b"dos puntos barra barra", start, end, b'S', b'D', 8) {
                        return 0;
                    }
                } else if c0 != 0 && WORDS_PUNCT.contains(&c0) && tok.len > 1 {
                    let idx = symbol_name_index(c0);
                    if idx >= 0 && !emit_split(wa, SYMBOL_NAMES[idx as usize].1, start, end, b'S', b'D', 8) {
                        return 0;
                    }
                }
                // Otherwise the piece is silently dropped (no emit call).
            }
            r
        };
        let step = consumed.max(1) as usize;
        cursor += consumed;
        pos += step;
        done += consumed;
        if total <= done || pos >= text.len() {
            return total;
        }
    }
}
