//! `FUN_10049c60`: tag-mode continuation. Called by the driver in a loop
//! while `WA+8` (`TAG_MODE`) is set (a URL path spelled piece by piece after
//! its own normalizer set the flag; ported with the URL normalizer).

use super::tables_oth::TAGMODE_PUNCT;
use super::tokenize::tokenize;
use super::{emit, FeSpa, TAG_MODE};
use crate::ling_spa::WaSpa;

fn ws(c: u8) -> bool {
    matches!(c, b' ' | 9 | 10 | 13)
}

fn in_set(set: &[u8], c: u8) -> bool {
    c != 0 && set.contains(&c)
}

/// `FUN_10049c60(wa, t + base, off)`.
pub fn tag_mode(_fe: &FeSpa, wa: &mut WaSpa, t: &[u8], base: usize, off: i32) -> i32 {
    if wa.i16(TAG_MODE) == 0 {
        return -1;
    }
    wa.set_i16(TAG_MODE, 0);
    if *t.get(base).unwrap_or(&0) == 0 {
        return -1;
    }
    let mut used = 0i32;
    loop {
        let c = *t.get(base + used as usize).unwrap_or(&0);
        if ws(c) || used >= 0x823 {
            break;
        }
        let (tok, r) = tokenize(t, base + used as usize, off + used, 0);
        if r == 0 || tok.ws >= 1 || tok.class == 10 {
            break;
        }
        let text = tok.text().to_vec();
        let c0 = text.first().copied().unwrap_or(0);
        let mut stop = false;
        if tok.class == 3 && c0 == b'/' {
            if tok.len == 1 {
                if !emit(wa, &text, tok.start, tok.end, b'S', b'D', 1, None) {
                    return 0;
                }
                wa.set_i16(TAG_MODE, 1);
                return used + r;
            }
            if !in_set(TAGMODE_PUNCT, c0) {
                stop = true;
            } else if !emit(wa, &text, tok.start, tok.end, b'S', b'D', 1, None) {
                return 0;
            }
        } else {
            let after = *t.get(base + used as usize + r as usize).unwrap_or(&0);
            if c0 == b'.' && (after == 0 || ws(after)) {
                stop = true;
            } else {
                let origin = match tok.class {
                    1 => Some(b'A'),
                    2 => Some(b'D'),
                    3 => {
                        if c0 == 0 {
                            stop = true;
                            None
                        } else if !in_set(TAGMODE_PUNCT, c0) {
                            stop = true;
                            None
                        } else {
                            Some(b'S')
                        }
                    }
                    _ => Some(b'S'),
                };
                if let Some(origin) = origin {
                    if !emit(wa, &text, tok.start, tok.end, origin, b'D', 1, None) {
                        return 0;
                    }
                }
            }
        }
        if stop {
            break;
        }
        let next = *t.get(base + used as usize + r as usize).unwrap_or(&0);
        used += r;
        if next == 0 {
            break;
        }
    }
    if used != 0 {
        used
    } else {
        -1
    }
}
