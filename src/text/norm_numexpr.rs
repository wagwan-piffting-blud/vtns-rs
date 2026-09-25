//! `FUN_100420f0`: math/list operators on the shared token left by the
//! driver's mode-0 tokenize (`=`, `+`, `*`, `/`, `-`, and `×` or a lone
//! symbol `x`). class 0x17. Transliterated branch by branch.

use super::cstr::*;
use super::norm_units::{dict_find, phrase_lookup, DICT_C, DICT_E, DICT_P};
use super::numwords::*;
use super::tabs::{rec30, rec5a, Rec30, Rec5a};
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::window::{prev_one, prev_two};
use super::Fe;

fn tk(fe: &Fe, t: &[u8], p: usize, off: i32, mode: i32) -> (Tok, i32) {
    let mut x = Tok::default();
    let r = tokenize(&mut x, t, p, off, mode, fe.ud.as_ref());
    (x, r)
}

/// The four-way unit test the operators share: `FUN_10040ab0` over
/// `DAT_100a7a5c/60/64`, then `FUN_10040b60`. `x` was tokenized at `p`
/// and returned `r`.
fn unit(fe: &Fe, out: &mut [u8], x: &Tok, t: &[u8], p: usize, off: i32, r: i32) -> bool {
    let mut used = r;
    dict_find(out, &DICT_E, &x.text, false)
        || dict_find(out, &DICT_P, &x.text, false)
        || dict_find(out, &DICT_C, &x.text, false)
        || phrase_lookup(fe, out, &x.text, t, p, off, &mut used)
}

/// The last record is a unit reading (class 0xf) ending where `b` ends.
fn after_units(wa: &Wa, b: &Tok) -> bool {
    let n = wa.n() as i32;
    n > 0 && wa.ri32(n - 1, 0x28) == 0xf && wa.ri32(n - 1, 4) == b.end
}

pub fn numexpr(fe: &Fe, wa: &mut Wa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let r = tok.consumed;
    let op = |c: u8| tok.text[0] == c && tok.len == 1 && tok.class == 3;
    let (word, used): (Vec<u8>, i32) = if op(b'=') {
        (s(0x100803ec).to_vec(), r)
    } else if op(b'+') {
        plus(fe, t, p, off, r)
    } else if op(b'*') {
        (times(fe, wa, tok, t, p, off, r), r)
    } else if ((strcmp(&tok.text, s(0x100803b0)) == 0 && tok.len == 2) || (lower(tok.text[0]) == 0x78 && tok.len == 1)) && tok.class == 3 && (0..3).contains(&tok.ws) {
        match times_sign(fe, wa, tok, t, p, off, r) {
            Some(w) => (w, r),
            None => return -1,
        }
    } else if tok.text[0] == b'/' {
        if tok.len != 1 || tok.class != 3 {
            return -1;
        }
        match slash(fe, wa, tok, t, p, off, r) {
            Some(x) => x,
            None => return r,
        }
    } else if op(b'-') {
        match dash_word(fe, wa, tok, t, p, off, r) {
            Some(w) => (w.to_vec(), r),
            None => (Vec::new(), r),
        }
    } else {
        return -1;
    };
    if strlen(&word) == 0 {
        return used;
    }
    if !wa.emit_split(&word, tok.start, off + used, 0x53, 0x44, 0x17) {
        return 0;
    }
    used
}

/// `+`: "plus", or "plus minus" over a glued `-`.
fn plus(fe: &Fe, t: &[u8], p: usize, off: i32, r: i32) -> (Vec<u8>, i32) {
    let (nx, nr) = tk(fe, t, p + r as usize, off + r, 0);
    if nx.text[0] == b'-' && nx.len == 1 && nx.class == 3 && nx.ws == 0 {
        let mut w = s(0x10080460).to_vec();
        w.push(b' ');
        w.extend_from_slice(s(0x10080458));
        return (w, r + nr);
    }
    (s(0x10080460).to_vec(), r)
}

/// `*`: "times" between two numbers with matching spacing, or after a unit
/// reading when a number and a unit follow ("6 ft * 8 ft"). Otherwise the
/// `*` is consumed silently.
fn times(fe: &Fe, wa: &Wa, tok: &Tok, t: &[u8], p: usize, off: i32, r: i32) -> Vec<u8> {
    let (a, b) = prev_two(t, p, off, 0);
    if b.len <= 0 {
        return Vec::new();
    }
    if b.class == 2 && well_formed(&b.text) && (a.len == 0 || (a.len > 0 && b.ws > 0)) {
        let (x, _) = tk(fe, t, p + r as usize, off + r, 0);
        if x.class == 2 && well_formed(&x.text) && tok.ws < 2 && tok.ws == x.ws {
            return s(0x10080450).to_vec();
        }
        return Vec::new();
    }
    if !(after_units(wa, &b) && tok.ws < 2) {
        return Vec::new();
    }
    let (x1, r1) = tk(fe, t, p + r as usize, off + r, 0);
    if !(x1.len > 0 && x1.ws < 2 && well_formed(&x1.text)) {
        return Vec::new();
    }
    let (p2, o2) = (p + (r + r1) as usize, off + r + r1);
    let (x2, r2) = tk(fe, t, p2, o2, 0);
    let mut out = [0u8; 256];
    if x2.len > 0 && x2.ws < 2 && unit(fe, &mut out, &x2, t, p2, o2, r2) {
        return s(0x10080450).to_vec();
    }
    Vec::new()
}

/// `×` or a lone symbol `x`: "x", or "by" between a unit reading and a
/// number followed by a unit ("6 ft × 8 ft"). `None` when a lookahead
/// tokenize comes back empty.
fn times_sign(fe: &Fe, wa: &Wa, tok: &Tok, t: &[u8], p: usize, off: i32, r: i32) -> Option<Vec<u8>> {
    let b = prev_one(t, p, off, 0);
    if b.len > 0 && after_units(wa, &b) && tok.ws < 2 {
        let (x1, r1) = tk(fe, t, p + r as usize, off + r, 0);
        if r1 == 0 {
            return None;
        }
        if x1.len > 0 && x1.ws < 2 && well_formed(&x1.text) {
            let (p2, o2) = (p + (r + r1) as usize, off + r + r1);
            let (x2, r2) = tk(fe, t, p2, o2, 0);
            if r2 == 0 {
                return None;
            }
            let mut out = [0u8; 256];
            if x2.len > 0 && x2.ws < 2 && unit(fe, &mut out, &x2, t, p2, o2, r2) {
                return Some(s(0x100776b4).to_vec());
            }
        }
    }
    Some(s(0x1007d344).to_vec())
}

/// `/`: "per <unit>" after a number, "per" between a unit reading and a
/// number followed by a unit ("3 km/2 hr"), otherwise "slash". A glued
/// `word/word` is consumed with nothing emitted (`None`).
fn slash(fe: &Fe, wa: &Wa, tok: &Tok, t: &[u8], p: usize, off: i32, r: i32) -> Option<(Vec<u8>, i32)> {
    let b = prev_one(t, p, off, 0);
    let (pa, oa) = (p + r as usize, off + r);
    let (after, ar) = tk(fe, t, pa, oa, 0);
    if b.len > 0 {
        if b.class == 1 && tok.ws == 0 && after.class == 1 && after.ws == 0 {
            return None;
        }
        if b.class == 2 && tok.ws < 2 && after.class == 1 && after.ws < 2 && tok.ws == after.ws {
            let mut name = [0u8; 256];
            if unit(fe, &mut name, &after, t, pa, oa, ar) {
                let mut w = Vec::from(&b"per "[..]);
                w.extend_from_slice(cs(&name));
                return Some((w, r + ar));
            }
        } else if after_units(wa, &b) && off - b.end == tok.ws && tok.ws < 2 && after.len > 0 && after.ws == 0 && well_formed(&after.text) {
            let (p2, o2) = (pa + ar as usize, oa + ar);
            let (x2, r2) = tk(fe, t, p2, o2, 0);
            let mut out = [0u8; 256];
            if x2.len > 0 && x2.ws < 2 && unit(fe, &mut out, &x2, t, p2, o2, r2) {
                return Some((s(0x10078b04).to_vec(), r));
            }
        }
    }
    Some((s(0x100806e4).to_vec(), r))
}

const SYMBOL_TAB: Rec5a = rec5a(0x10082350, 0x1009b050);
const GLYPHS: Rec30 = rec30(0x10095a08, 0x1009b084);

/// A two-byte fraction/superscript glyph the tokenizer produced (EUC-KR
/// `A1..AD xx`, `AE A1..C2`, or the `FD FE` marker) that is in `GLYPHS`.
fn glyph_pair(x: &Tok) -> Option<bool> {
    let (c0, c1) = (x.text[0], x.text[1]);
    let shape = ((0xa1..=0xad).contains(&c0) && c1 >= 0xa1 && c1 != 0xff) || (c0 == 0xfd && c1 == 0xfe) || (c0 == 0xae && c1 > 0xa0 && c1 < 0xc3);
    if !shape {
        return None;
    }
    Some(GLYPHS.find(&x.text, true) >= 0)
}

/// The punctuation-context test the dash applies to the token two before
/// and the token two after: punctuation from `0x100802c0` (or the `A2 FE`
/// break marker) may touch, anything else needs a space.
fn spaced_ok(x: &Tok) -> bool {
    let c0 = x.text[0];
    let is_p = matches!(x.class, 8 | 3 | 7);
    let hit = (c0 != 0 && strchr(s(0x100802c0), c0)) || strcmp(s(0x100802bc), &x.text) == 0;
    if is_p && hit {
        x.ws >= 0
    } else {
        x.ws > 0
    }
}

/// A neighbour that rules out a range: an operator-ish first character, or
/// a number.
fn blocks_range(x: &Tok) -> bool {
    x.class == 2 || (x.text[0] != 0 && strchr(s(0x100806cc), x.text[0]))
}

/// `FUN_100420f0`'s `-` branch: "minus", "dash", "to", "through", or `None`
/// (consumed silently, sometimes leaving a pause on the previous record).
fn dash_word(fe: &Fe, wa: &mut Wa, tok: &Tok, t: &[u8], p: usize, off: i32, r: i32) -> Option<&'static [u8]> {
    let minus = s(0x10080458);
    let dash = s(0x100779a8);
    let (a, b) = prev_two(t, p, off, 0);
    let (nx, nr) = tk(fe, t, p + r as usize, off + r, 0);
    let p2 = p + (r + nr) as usize;
    let o2 = off + r + nr;
    let n = wa.n() as i32;
    let last_is_b = n > 0 && wa.ri32(n - 1, 4) == b.end;

    if nx.len == 1 && nx.ws < 3 && SYMBOL_TAB.find_prefix(&nx.text, true) >= 0 {
        let (n2, _) = tk(fe, t, p2, o2, 0);
        if n2.len > 0 && n2.class == 2 && n2.ws < 3 {
            return Some(if nx.ws == 0 { minus } else { dash });
        }
        return None;
    }
    if b.len == 0 {
        if nx.len > 0 && nx.class == 2 && well_formed(&nx.text) && nx.ws < 3 {
            return Some(if nx.ws == 0 { minus } else { dash });
        }
        return None;
    }
    if b.class != 2 {
        if tok.ws > 0 && well_formed(&nx.text) && nx.ws == 0 {
            return Some(minus);
        }
        let units = last_is_b && wa.ri32(n - 1, 0x28) == 0xf && tok.ws <= 2 && well_formed(&nx.text) && nx.ws <= 2;
        if units {
            return Some(dash);
        }
        let e_exp = a.len > 0 && well_formed(&a.text) && b.len == 1 && lower(b.text[0]) == 0x65 && tok.ws < 2 && well_formed(&nx.text) && nx.ws < 2;
        if e_exp || (b.class != 1 && well_formed(&nx.text) && nx.ws == 0) {
            return Some(minus);
        }
        let sym_gap = b.len == 1 && b.text[0] != 0 && strchr(s(0x100806dc), b.text[0]) && (1..=2).contains(&tok.ws) && well_formed(&nx.text) && (1..=2).contains(&nx.ws);
        if sym_gap {
            return Some(dash);
        }
        let spaced = last_is_b && b.class == 1 && (1..=2).contains(&tok.ws) && (nx.class == 1 || nx.class == 2) && (1..=2).contains(&nx.ws);
        if spaced {
            wa.set_ri32(n - 1, 0x2c, 1);
            return None;
        }
        if last_is_b && b.class == 1 && b.len == 1 && upper(b.text[0]) == 0x4b && (0..=2).contains(&tok.ws) && nx.class == 2 && tok.ws == nx.ws {
            let (n2, _) = tk(fe, t, p2, o2, 0);
            if n2.len > 0 && n2.ws < 3 && stricmp(&n2.text, s(0x100806d4)) == 0 {
                return Some(s(0x1007740c));
            }
        }
        return None;
    }

    // `b` is a number.
    let word = if (nx.len == 1 || nx.len == 2) && nx.class == 3 && glyph_pair(&nx).is_some() {
        match glyph_pair(&nx) {
            Some(true) => Some(if (tok.ws < 2 && tok.ws == nx.ws) || nx.ws == 0 { minus } else { dash }),
            _ => number_dash(fe, wa, tok, &a, &b, &nx, t, p2, o2, last_is_b),
        }
    } else {
        number_dash(fe, wa, tok, &a, &b, &nx, t, p2, o2, last_is_b)
    };
    if word.is_none() && nx.class == 2 {
        return Some(dash);
    }
    word
}

/// The number-before, not-a-glyph-after part of the `-` branch
/// (`LAB_1004307a` onward).
#[allow(clippy::too_many_arguments)]
fn number_dash(fe: &Fe, wa: &mut Wa, tok: &Tok, a: &Tok, b: &Tok, nx: &Tok, t: &[u8], p2: usize, o2: i32, last_is_b: bool) -> Option<&'static [u8]> {
    let minus = s(0x10080458);
    let dash = s(0x100779a8);
    let to = s(0x10077380);
    if nx.class != 2 {
        let n = wa.n() as i32;
        if last_is_b && tok.ws > 0 && nx.class == 1 && nx.ws > 0 {
            wa.set_ri32(n - 1, 0x2c, 1);
        }
        return None;
    }
    if !well_formed(&b.text) || !well_formed(&nx.text) {
        return None;
    }
    let (n2, r2) = tk(fe, t, p2, o2, 0);
    let c2 = n2.text[0];
    if n2.len == 1 && c2 != 0 {
        if strchr(s(0x100804e4), c2) && tok.ws == nx.ws && tok.ws == n2.ws && n2.ws < 2 {
            return if c2 != b'-' || tok.ws != 0 { Some(minus) } else { None };
        }
        if strchr(s(0x1007d4e8), c2) && tok.ws == nx.ws && n2.ws < 2 {
            let (n3, _) = tk(fe, t, p2 + r2 as usize, o2 + r2, 0);
            if n3.len > 0 && well_formed(&n3.text) && n2.ws == n3.ws {
                return Some(minus);
            }
            return Some(dash);
        }
    }
    if (n2.len == 1 || n2.len == 2) && n2.class == 3 && glyph_pair(&n2).is_some() {
        if tok.ws == nx.ws && n2.ws <= 1 && glyph_pair(&n2) == Some(true) {
            return Some(minus);
        }
    }
    if n2.len > 0 && well_formed(&n2.text) && tok.ws == nx.ws && n2.ws < 2 {
        let (n3, r3) = tk(fe, t, p2 + r2 as usize, o2 + r2, 0);
        if n3.len == 1 && n3.text[0] == b'/' && n3.ws == 0 {
            let (n4, _) = tk(fe, t, p2 + (r2 + r3) as usize, o2 + r2 + r3, 0);
            if n4.len > 0 && well_formed(&n4.text) && n4.ws == 0 {
                return Some(minus);
            }
        }
        return Some(dash);
    }
    if a.len == 1 && a.text[0] != 0 && strchr(s(0x100804e4), a.text[0]) && tok.ws == b.ws && tok.ws == nx.ws {
        if tok.ws >= 2 {
            return None;
        }
        if !(n2.len != 0 && (n2.len < 1 || n2.class == 2)) {
            return if a.text[0] != b'-' || tok.ws != 0 { Some(minus) } else { None };
        }
    }
    if tok.ws > 1 || nx.ws > 1 || tok.ws != nx.ws {
        return None;
    }
    if a.len != 0 {
        if blocks_range(a) {
            return None;
        }
        let ok = if a.class == 1 { b.ws >= 1 } else { spaced_ok(&Tok { ws: b.ws, ..*a }) };
        if !ok {
            return None;
        }
    }
    if n2.len != 0 {
        if blocks_range(&n2) {
            return None;
        }
        let ok = if n2.class == 1 { n2.ws >= 1 } else { spaced_ok(&n2) };
        if !ok {
            return None;
        }
    }
    if !b.text().contains(&b'.') && !nx.text().contains(&b'.') {
        let bv = atoi(&digits_only(&b.text));
        let nv = atoi(&digits_only(&nx.text));
        if bv < nv {
            return Some(to);
        }
    }
    if n2.len > 0 && (stricmp(s(0x100806c4), &n2.text) == 0 || stricmp(s(0x100806c0), &n2.text) == 0) {
        return Some(to);
    }
    None
}
