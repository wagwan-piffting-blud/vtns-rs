//! `FUN_10033b60`: address's city-name sub-recognizer (dispatch order 3,
//! after PO box), with its validator `FUN_10034180` and emitter
//! `FUN_10034110`. Ported from the decompile, cross-checked against the
//! disassembly of `FUN_10034180` and `FUN_10062400`.
//!
//! Up to 4 word tokens (mode 0x16) become slots. Each slot is a compass
//! abbreviation (`'d'`, expanded through the record's +0x10 field), a
//! spelled-out compass word (`'D'`), a `citya` abbreviation (`'A'`), or any
//! capitalized all-letter word (`'F'`, hyphens rewritten to spaces in place
//! by `FUN_10034490`). A lone `.` right after a `'d'`/`'A'` slot is absorbed.
//! A `,` ends the scan and is remembered on the last slot.
//!
//! Longest first, the slots' expansions joined with `-` must be a TPP
//! dictionary entry with field `'A'+n-1` (the place-name fields, LEXICON.md
//! §5): that is the real gate, which is why ordinary capitalized words that
//! happen to be town names ("Hazard", "Max", "Mobile") read as addresses.
//! Exclusions: a following street-type word (`streeta`/`streetf`), "EADS",
//! a following Roman numeral, and "George"/"King"/"Martin"/"Luther" or
//! "George W" before a capitalized name.
//!
//! Every slot is emitted `emit_split(expansion, 0x41, 0x44, class 0x12)` with
//! the last record's +0x1e set to 0x1f. Unless it is a single slot with no
//! comma, `FUN_10053920(..., 1)` is tried on what follows; on success the
//! last city record's +0x2c becomes 9, on failure the comma is given back.

use super::cstr::*;
use super::norm_address_csc::trailing;
use super::tabs::{ptab, rec30, Ptab, Rec30};
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::Fe;
use crate::lexicon::encode_key;
use crate::ling::funcwords::tpp_field_in_payload;

fn tk(fe: &Fe, t: &[u8], p: usize, off: i32, mode: i32) -> (Tok, i32) {
    let mut x = Tok::default();
    let r = tokenize(&mut x, t, p, off, mode, fe.ud.as_ref());
    (x, r)
}

/// `FUN_10034490`: letters and `'` only, hyphens rewritten to spaces.
fn name_chars(w: &mut Vec<u8>) -> bool {
    if w.is_empty() {
        return false;
    }
    for c in w.iter_mut() {
        if cls(*c) & 0xc0 == 0 {
            if *c == b'-' {
                *c = b' ';
            } else if *c != b'\'' {
                return false;
            }
        }
    }
    true
}

const COMPASS_ABBR: Rec30 = rec30(0x10099078, 0x1009b0e8);
const COMPASS_FULL: Ptab = ptab(0x100992b8, 0x1009b0ec);

/// `FUN_10010120`: a capitalized name ("Smith", "SMITH", "McDonald",
/// "O'Brien", "J.R.").
pub fn cap_name(w: &[u8]) -> bool {
    let c0 = at(w, 0);
    let c1 = at(w, 1);
    if (c0 == b'M' && upper(c1) == 0x43) || (c0 == b'D' && upper(c1) == 0x45) {
        if cap_name(cs(&w[2.min(w.len())..])) {
            return true;
        }
    }
    let up0 = cls(c0) & 0x80 != 0;
    if up0 && c1 == b'\'' && cls(at(w, 2)) & 0x80 != 0 {
        return cap_name(cs(&w[2..]));
    }
    if !up0 {
        return false;
    }
    let set = s(0x10079284);
    let mut i = 1isize;
    if at(w, i) != 0 && strchr(set, at(w, i)) {
        loop {
            let c = at(w, i + 1);
            i += 1;
            if c == 0 || !strchr(set, c) {
                break;
            }
        }
        if at(w, i) == 0 {
            return false;
        }
    }
    if at(w, i) == 0 {
        return true;
    }
    let b = cls(at(w, i));
    let upper_run = if b & 0x80 != 0 {
        true
    } else if b & 0x40 != 0 {
        false
    } else {
        return false;
    };
    loop {
        let c = at(w, i + 1);
        if c == 0 {
            return true;
        }
        if !strchr(set, c) {
            let mask = if upper_run { 0x80 } else { 0x40 };
            if cls(c) & mask == 0 {
                return false;
            }
        }
        i += 1;
    }
}

/// `FUN_10041df0`: "I".."XX" (case-sensitive), or an EUC-KR roman numeral.
fn roman(w: &[u8]) -> i32 {
    let tab = [
        s(0x100776e8),
        s(0x100806bc),
        s(0x100806b8),
        s(0x100806b4),
        s(0x10079148),
        s(0x100806b0),
        s(0x100806ac),
        s(0x100806a4),
        s(0x100806a0),
        s(0x1007d3bc),
        s(0x1008069c),
        s(0x10080698),
        s(0x10080690),
        s(0x1008068c),
        s(0x10080688),
        s(0x10080684),
        s(0x1008067c),
        s(0x10080674),
        s(0x10080670),
        s(0x1008066c),
    ];
    for (i, r) in tab.iter().enumerate() {
        if strcmp(r, w) == 0 {
            return i as i32 + 1;
        }
    }
    let w = cs(w);
    if w.len() == 2 && dbcs_pair(w[0], w[1]) && w[0] == 0xa5 && w[1] > 0xaf && w[1] < 0xba {
        return w[1] as i32 - 0xaf;
    }
    -1
}

struct Slot {
    text: Vec<u8>,
    exp: Vec<u8>,
    src: u8,
    start: i32,
    end: i32,
    len: i32,
    comma: i32,
    p_after: usize,
}

/// `FUN_10034180`.
fn validate(fe: &Fe, t: &[u8], sl: &[Slot]) -> bool {
    let n = sl.len();
    if n > 4 {
        return false;
    }
    let mut key: Vec<u8> = Vec::new();
    let mut acc = 0;
    for s_ in sl {
        acc += s_.len + 1;
        if acc >= 0x1e {
            return false;
        }
        if !key.is_empty() {
            key.push(b'-');
        }
        key.extend_from_slice(&s_.exp);
    }
    let Some(payload) = fe.tpp.get(&encode_key(&key)) else { return false };
    let Some(field) = tpp_field_in_payload(payload, b'A' + n as u8 - 1) else { return false };
    if n == 1 {
        if sl[0].src == b'd' || sl[0].src == b'A' {
            return false;
        }
    } else {
        for (i, s_) in sl.iter().enumerate() {
            if at(field, i as isize + 1) == b'0' && (s_.src == b'd' || s_.src == b'A') {
                return false;
            }
        }
    }
    let (x, _) = tk(fe, t, sl[n - 1].p_after, 0, 0x15);
    let word = x.len > 0 && x.ws < 2 && x.class == 1;
    let xt = x.text();
    if word && (fe.streeta.find(xt) >= 0 || fe.streetf.find(xt) >= 0) {
        return false;
    }
    let t0 = &sl[0].text;
    if n == 1 && strcmp(t0, s(0x1008009c)) == 0 {
        return false;
    }
    if n == 2 && stricmp(t0, s(0x10080094)) == 0 && sl[1].src == b'd' && stricmp(&sl[1].text, s(0x10079144)) == 0 && word && cap_name(xt) {
        return false;
    }
    if n == 1 && word && roman(xt) >= 0 {
        return false;
    }
    if n == 1 && (stricmp(t0, s(0x10080094)) == 0 || stricmp(t0, s(0x100773c4)) == 0) && word && cap_name(xt) {
        return false;
    }
    if n == 1 && (stricmp(t0, s(0x1008008c)) == 0 || stricmp(t0, s(0x10080084)) == 0) && word && cap_name(xt) {
        return false;
    }
    true
}

pub fn street(fe: &Fe, wa: &mut Wa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let _ = tok;
    city(fe, wa, t, p, off, false)
}

/// `FUN_10033b60(..., flag)`: no match is -1 with flag 0, 0 with flag 1
/// (the street recognizer's trailing-city call).
pub fn city(fe: &Fe, wa: &mut Wa, t: &[u8], p: usize, off: i32, flag: bool) -> i32 {
    let mut slots: Vec<Slot> = Vec::new();
    let (mut p2, mut off2) = (p, off);
    let mut ntok = 0;
    let mut prev_src = 0u8;
    while slots.len() < 4 {
        let (w, r) = tk(fe, t, p2, off2, 0x16);
        if r == 0 || w.class == 9 || (w.nl == 0 && w.ws > 2) {
            break;
        }
        if w.ws == 0 && w.len < 3 && w.text[0] == b'.' && ntok > 0 && (prev_src == b'A' || prev_src == b'd') {
            prev_src = b'P';
            p2 += r as usize;
            off2 += r;
            slots.last_mut().unwrap().p_after = p2;
        } else {
            if !slots.is_empty() && (w.class == 3 || w.class == 7) && w.len == 1 && w.text[0] == b',' {
                slots.last_mut().unwrap().comma = r;
                break;
            }
            if w.class != 1 {
                break;
            }
            let mut text = w.text().to_vec();
            let (src, exp) = {
                let i = COMPASS_ABBR.find(&text, true);
                if i >= 0 {
                    (b'd', COMPASS_ABBR.field(i, 0x10).to_vec())
                } else {
                    let j = COMPASS_FULL.find(&text, true);
                    if j >= 0 {
                        (b'D', COMPASS_FULL.get(j).to_vec())
                    } else {
                        let k = fe.citya.find(&text);
                        if k >= 0 {
                            (b'A', fe.citya.vals[k as usize].clone())
                        } else if is_upper(text[0]) && name_chars(&mut text) {
                            (b'F', text.clone())
                        } else {
                            break;
                        }
                    }
                }
            };
            prev_src = src;
            p2 += r as usize;
            off2 += r;
            slots.push(Slot { text, exp, src, start: w.start, end: off2, len: w.len, comma: 0, p_after: p2 });
        }
        ntok += 1;
    }

    for n in (1..=slots.len()).rev() {
        let sl = &slots[..n];
        if !validate(fe, t, sl) {
            continue;
        }
        let last = &sl[n - 1];
        let mut consumed = last.end - off;
        if last.comma != 0 {
            consumed += last.comma;
        }
        for s_ in sl {
            if !wa.emit_split(&s_.exp, s_.start, s_.end, 0x41, 0x44, 0x12) {
                return 0;
            }
            let r = wa.n() as i32 - 1;
            wa.set_i16(Wa::rec(r) + 0x1e, 0x1f);
        }
        if n == 1 && last.comma == 0 {
            return consumed;
        }
        let r = wa.n() as i32 - 1;
        if last.comma != 0 {
            wa.set_ri32(r, 0x2c, last.comma);
        }
        let tail = trailing(fe, wa, t, p + consumed as usize, off + consumed);
        if tail != 0 {
            wa.set_ri32(r, 0x2c, 9);
            return consumed + tail;
        }
        let f = wa.ri32(r, 0x2c);
        if f != 0 {
            consumed -= f;
        }
        wa.set_ri32(r, 0x2c, 0);
        return consumed;
    }
    if flag {
        0
    } else {
        -1
    }
}
