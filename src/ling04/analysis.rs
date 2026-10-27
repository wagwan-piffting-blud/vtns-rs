//! `fe_TagAndPronounce` (0x1000af20): lookup, candidates -> morph entries (`dict_CopyMorphsToWordArray`
//! 0x1000b640, with the homograph tagger for multi-pronunciation words), morphology and the wh pass.

use super::{morph, mr, Wa04, MORPH_COUNT, WH_FLAG};
use crate::lexicon::{stricmp, strcmp, Lexicon};
use crate::ling::lookup::cand;
use crate::ling::tables::{L_MODAL, L_NEG, L_WHBE, L_WHCONJ, L_WHPREP, L_WHWORD};
use crate::ling::tagger::in_list;
use crate::ling::Ws;

/// `fe_TagAndPronounce`: false when the candidate array overflows.
pub fn tag_and_pronounce(ws: &mut Ws, wa: &Wa04, n: usize, lex: &Lexicon) -> bool {
    if n == 0 {
        ws.set_i16(MORPH_COUNT, 0);
        return true;
    }
    ws.set_i16(MORPH_COUNT, 0);
    if !super::lookup::lookup_tokens(ws, wa, n, true, lex) {
        return false;
    }
    if ws.i16(0) > 0 {
        copy_morphs(ws, lex);
    }
    super::morph::morphology(ws, wa, lex);
    wh_flag(ws);
    true
}

/// `FUN_1000b640`.
pub fn copy_morphs(ws: &mut Ws, lex: &Lexicon) {
    ws.set_i16(MORPH_COUNT, 0);
    let nc = ws.i16(0).max(0) as usize;
    for i in 0..nc {
        let c = Ws::cand(i);
        let m = morph(i);
        ws.set_i16(m + mr::TOKEN, ws.i16(c + cand::TOKEN));
        let sp = ws.cstr(c + cand::SPELLING).to_vec();
        ws.strcpy(m + mr::SPELLING, &sp);
        if ws.i16(c + cand::COUNT) < 2 {
            let p = ws.cstr(c + cand::PRON).to_vec();
            ws.strcpy(m + mr::PRON, &p);
        } else {
            let p = super::tagger::choose_pron(ws, nc, i, lex);
            ws.strcpy(m + mr::PRON, &p);
        }
        ws.set_u8(m + mr::CLASS_CHAR, ws.u8(c + cand::CLASS_CHAR));
        ws.set_i16(m + mr::B40, ws.i16(c + cand::B40));
        ws.set_i16(m + mr::B10, ws.i16(c + cand::B10));
        ws.set_i16(m + mr::B20, ws.i16(c + cand::B20));
        ws.set_i16(m + mr::B80, ws.i16(c + cand::B80));
        ws.set_i16(MORPH_COUNT, ws.i16(MORPH_COUNT) + 1);
    }
}

const WH_P: u8 = 0x50;
const WH_N: u8 = 0x4e;
const WH_D: u8 = 0x44;

fn sp(ws: &Ws, i: usize) -> Vec<u8> {
    ws.cstr(morph(i) + mr::SPELLING).to_vec()
}

fn ci(a: &[u8], b: &[u8]) -> bool {
    stricmp(a, b) == 0
}

/// `FUN_1000afb0`: a modal, or "ca"/"sha"/"wo" before "n't".
fn modal(a: &[u8], b: &[u8]) -> bool {
    in_list(&L_MODAL, a) || ((ci(a, b"ca") || ci(a, b"sha") || ci(a, b"wo")) && ci(b, b"n't"))
}

/// `FUN_1000b030`: a form of "be"; a leading "'s" doesn't count.
fn be(a: &[u8], idx: usize) -> bool {
    in_list(&L_WHBE, a) && (idx > 0 || !ci(a, b"'s"))
}

/// `FUN_1000b160`: as 2014 `FUN_1000e570`, but a form of "be" or a modal that is not a wh-word answers
/// "not a question" and stops the scan.
fn wh_scan(ws: &Ws, idx: usize) -> u8 {
    let n = ws.i16(MORPH_COUNT).max(0) as usize;
    let w = sp(ws, idx);
    if (in_list(&L_WHPREP, &w) || in_list(&L_NEG, &w) || in_list(&L_WHCONJ, &w)) && idx + 2 < n {
        let r = wh_scan(ws, idx + 1);
        if r == WH_P || r == WH_N {
            return r;
        }
    }
    if ci(&w, b"why") {
        return WH_P;
    }
    if ci(&w, b"how") && idx + 2 < n {
        let s1 = sp(ws, idx + 1);
        if ci(&s1, b"about") || ci(&s1, b"come") || ci(&s1, b"much") || ci(&s1, b"say") {
            return WH_P;
        }
    }
    if ci(&w, b"what") {
        if idx + 2 < n {
            let s1 = sp(ws, idx + 1);
            // "thouth" is the engine's own spelling.
            let list: [&[u8]; 7] = [b"about", b"if", b"in", b"thouth", b"then", b"time", b"else"];
            if list.iter().any(|x| ci(&s1, x)) {
                return WH_P;
            }
        }
        if idx + 3 < n && ci(&sp(ws, idx + 1), b"kind") && ci(&sp(ws, idx + 2), b"of") {
            return WH_P;
        }
    }
    if in_list(&L_WHWORD, &w) {
        let j1 = idx + 1;
        if j1 + 1 < n {
            let s1 = sp(ws, j1);
            if be(&s1, j1) || modal(&s1, b"") || ci(&s1, b",") {
                return WH_P;
            }
        }
        if idx + 3 < n && modal(&sp(ws, j1), &sp(ws, idx + 2)) {
            return WH_P;
        }
        if n <= j1 + 1 {
            return WH_P;
        }
        if (j1..n - 1).any(|p| ci(&sp(ws, p), b",")) {
            let mut j2 = idx + 2;
            let mut p = j1;
            while p < n - 1 {
                if ci(&sp(ws, p), b",") && j2 + 1 < n {
                    if ci(&sp(ws, p + 1), b"if") {
                        if n <= j2 + 1 {
                            return WH_P;
                        }
                        let mut j3 = j2;
                        let mut q = p + 1;
                        while !ci(&sp(ws, q), b",") {
                            j3 += 1;
                            q += 1;
                            if n <= j3 + 1 {
                                return WH_P;
                            }
                        }
                    } else if wh_scan(ws, j2) == WH_N {
                        return WH_N;
                    }
                }
                p += 1;
                j2 += 1;
            }
        }
        return WH_P;
    }
    if (idx + 1 < n && (be(&w, idx) || modal(&w, b""))) || (idx + 2 < n && modal(&w, &sp(ws, idx + 1))) {
        return WH_N;
    }
    WH_D
}

/// `FUN_1000b540`: sets `WH_FLAG` when the last morph is "?" and a wh scan from the start, or from after a
/// comma, finds a question.
pub fn wh_flag(ws: &mut Ws) {
    let n = ws.i16(MORPH_COUNT).max(0) as usize;
    ws.set_i16(WH_FLAG, 0);
    if n < 2 || strcmp(&sp(ws, n - 1), b"?") != 0 {
        return;
    }
    for i in 0..n - 1 {
        let r = if i == 0 {
            wh_scan(ws, 0)
        } else if strcmp(&sp(ws, i), b",") == 0 {
            wh_scan(ws, i + 1)
        } else {
            WH_D
        };
        if r == WH_P {
            ws.set_i16(WH_FLAG, 1);
            return;
        }
        if r == WH_N {
            return;
        }
    }
}
