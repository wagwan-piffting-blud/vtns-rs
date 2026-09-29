//! Shared emit helpers used by the user-dict pre-pass, tag mode and VTML:
//! `emit_split` (`FUN_100354d0`), `emit_bracketed` (`FUN_100355d0`) and
//! `spell::expand` (`FUN_10035880`, in `spell.rs`).

use super::emit;
use crate::ling_spa::WaSpa;

fn cut0(s: &[u8]) -> &[u8] {
    match s.iter().position(|&c| c == 0) {
        Some(i) => &s[..i],
        None => s,
    }
}

/// `FUN_100354d0`: emit `text`, one record per space/tab-separated part, all
/// sharing the same span and tags (a part may be empty; `emit` no-ops on
/// empty text, matching the engine calling it unconditionally at each
/// separator).
pub fn emit_split(wa: &mut WaSpa, text: &[u8], start: i32, end: i32, origin: u8, case: u8, src: u32) -> bool {
    let text = cut0(text);
    if !text.contains(&b' ') && !text.contains(&b'\t') {
        return emit(wa, text, start, end, origin, case, src, None);
    }
    let mut part = [0u8; 32];
    let mut k = 0usize;
    for i in 0..=text.len() {
        let c = if i < text.len() { text[i] } else { 0 };
        if c == b' ' || c == b'\t' || c == 0 {
            if !emit(wa, &part[..k], start, end, origin, case, src, None) {
                return false;
            }
            k = 0;
        } else if k < part.len() {
            part[k] = c;
            k += 1;
        }
    }
    true
}

/// `FUN_100355d0`: like `emit_split`, but a `<...>` run is emitted with case
/// forced to `'S'` and a `[...]` run is dropped silently (the markers
/// themselves are never emitted). Used only for user-dict targets.
pub fn emit_bracketed(wa: &mut WaSpa, text: &[u8], start: i32, end: i32, origin: u8, case: u8, src: u32) -> bool {
    let text = cut0(text);
    if !text.iter().any(|&c| matches!(c, b'<' | b'>' | b'[' | b']' | b' ' | b'\t')) {
        return emit(wa, text, start, end, origin, case, src, None);
    }
    let mut part = [0u8; 32];
    let mut k = 0usize;
    let mut i = 0usize;
    while i <= text.len() {
        let c = if i < text.len() { text[i] } else { 0 };
        if c == b' ' || c == b'\t' || c == 0 || (k > 0 && matches!(c, b'<' | b'>' | b'[' | b']')) {
            if c == b'>' {
                if !emit(wa, &part[..k], start, end, origin, b'S', src, None) {
                    return false;
                }
            } else if c != b']' {
                if !emit(wa, &part[..k], start, end, origin, case, src, None) {
                    return false;
                }
            }
            k = 0;
        } else if c != b'<' && c != b'[' && k < part.len() {
            part[k] = c;
            k += 1;
        }
        i += 1;
    }
    true
}
