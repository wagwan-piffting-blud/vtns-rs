//! `FUN_10049c60`: tag-mode continuation. Called by the driver in a loop
//! while `WA+8` (`TAG_MODE`) is set, after a URL normalizer spoke a bare
//! `/`: the rest of the path is spelled (`FUN_10035880`) piece by piece up
//! to the next bare `/`, which sets the flag again.

use super::spell;
use super::tables_oth::TAGMODE_PUNCT;
use super::tokenize::tokenize;
use super::{FeSpa, TAG_MODE};
use crate::ling_spa::WaSpa;

fn ws(c: u8) -> bool {
    matches!(c, b' ' | 9 | 10 | 13)
}

fn in_set(set: &[u8], c: u8) -> bool {
    c != 0 && set.contains(&c)
}

/// `FUN_10049c60(wa, t + base, off)`.
pub fn tag_mode(fe: &FeSpa, wa: &mut WaSpa, t: &[u8], base: usize, off: i32) -> i32 {
    if wa.i16(TAG_MODE) == 0 {
        return -1;
    }
    wa.set_i16(TAG_MODE, 0);
    let at = |i: usize| *t.get(i).unwrap_or(&0);
    let mut c = at(base);
    let mut used = 0i32;
    while c != 0 && !ws(c) && used < 0x823 {
        let (tok, r) = tokenize(t, base + used as usize, off + used, 0);
        if r == 0 || tok.ws >= 1 || tok.class == 10 {
            break;
        }
        let text = tok.text().to_vec();
        let c0 = text.first().copied().unwrap_or(0);
        let spell_it = if tok.class == 3 && c0 == b'/' {
            if tok.len == 1 {
                if spell::expand(fe, wa, &text, tok.start, tok.end, b'D') == 0 {
                    return 0;
                }
                wa.set_i16(TAG_MODE, 1);
                return used + r;
            }
            in_set(TAGMODE_PUNCT, c0)
        } else {
            let after = at(base + (used + r) as usize);
            if c0 == b'.' && (after == 0 || ws(after)) {
                break;
            }
            match tok.class {
                1 | 2 => true,
                3 => in_set(TAGMODE_PUNCT, c0),
                _ => true,
            }
        };
        if !spell_it {
            break;
        }
        if spell::expand(fe, wa, &text, tok.start, tok.end, b'D') == 0 {
            return 0;
        }
        c = at(base + (used + r) as usize);
        used += r;
    }
    if used != 0 {
        used
    } else {
        -1
    }
}
