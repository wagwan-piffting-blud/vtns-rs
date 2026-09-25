//! `FUN_10031d40`: three sub-recognizers tried in order: `FUN_10059560`
//! (person names, "Dr. Smith"; see `norm_name.rs`), `FUN_10031db0` ("alpha
//! code + glued digits", NWS UGC zone codes `"MIZ009"`), and `FUN_10032180`
//! (honorific/address abbreviations and the two expansion tables).
//! class 0x16.

use super::cstr::*;
use super::norm_units::{dict_find, phrase_lookup, DICT_C, DICT_E, DICT_P};
use super::numwords::*;
use super::sbd;
use super::tabs::{rec30, Rec30};
use super::window::{back_words, last_three, prev_one};
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::Fe;

fn tk(fe: &Fe, t: &[u8], p: usize, off: i32, mode: i32) -> (Tok, i32) {
    let mut x = Tok::default();
    let r = tokenize(&mut x, t, p, off, mode, fe.ud.as_ref());
    (x, r)
}

/// `FUN_1000ffa0`: every character is a letter (upper or lower).
fn all_alpha(w: &[u8]) -> bool {
    let w = cs(w);
    !w.is_empty() && w.iter().all(|&c| cls(c) & 0xc0 != 0)
}

/// `FUN_1000ffd0`: no lowercase letter anywhere (all-caps, or no letters).
fn no_lowercase(w: &[u8]) -> bool {
    let w = cs(w);
    w.iter().all(|&c| cls(c) & 0xc0 == 0 || cls(c) & 0x40 == 0)
}

/// `FUN_100100d0`: count of a/e/i/o/u (case-folded).
fn vowel_count(w: &[u8]) -> i32 {
    cs(w).iter().filter(|&&c| matches!(tolower(c), b'a' | b'e' | b'i' | b'o' | b'u')).count() as i32
}

pub fn n15(fe: &Fe, wa: &mut Wa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let _ = tok;
    let r = super::norm_name::name(fe, wa, t, p, off);
    if r >= 0 {
        return r;
    }
    let r = ugc_code(fe, wa, t, p, off);
    if r >= 0 {
        return r;
    }
    abbr(fe, wa, t, p, off)
}

const ABBR_I: Rec30 = rec30(0x100996c8, 0x1009b104);
const ABBR_S: Rec30 = rec30(0x10099e78, 0x1009b108);

/// `FUN_10063120`: digits and commas, no comma at either end.
fn num_str(w: &[u8]) -> bool {
    let w = cs(w);
    for (i, &c) in w.iter().enumerate() {
        if c == b',' {
            if i == 0 || i == w.len() - 1 {
                return false;
            }
        } else if cls(c) & 0x10 == 0 {
            return false;
        }
    }
    true
}

fn is_period(x: &Tok) -> bool {
    x.ws == 0 && x.len < 3 && x.text[0] == b'.' && x.len == 1
}

fn bad(r: i32, x: &Tok) -> bool {
    r == 0 || x.class == 9 || x.udict != 0
}

/// "Mrs" -> "Misses", "Mr" -> "Mister", "Ms" -> "Miz".
fn honorific(x: &Tok) -> &'static [u8] {
    if x.len == 3 {
        s(0x1007ff94)
    } else if lower(x.text[1]) == 0x72 {
        s(0x1007ff8c)
    } else {
        s(0x1007ff88)
    }
}

fn is_mr(w: &[u8]) -> bool {
    stricmp(w, s(0x10080010)) == 0 || stricmp(w, s(0x1008000c)) == 0 || stricmp(w, s(0x10080008)) == 0
}

/// `FUN_10032180`: honorific and address abbreviations ("Mr."/"Mrs."/"Ms.",
/// "Vol./No./p./pp." + number, "Dr", "St", "Ste", "Co."/"Pty.", "Ctr"/"Pkwy",
/// "w/"), then the two expansion tables (`0x100996c8` case-insensitive,
/// `0x10099e78` case-sensitive). Transliterated from the disassembly.
fn abbr(fe: &Fe, wa: &mut Wa, t: &[u8], p: usize, off: i32) -> i32 {
    let (t1, r1) = tk(fe, t, p, off, 0);
    if r1 == 0 || t1.class == 9 || t1.udict != 0 || t1.class != 1 {
        return -1;
    }
    let w1 = t1.text();
    let upper1 = cls(w1[0]) & 0x80 != 0;
    let lead = off == 0 || (off > 0 && t1.ws > 0);
    let vol = [s(0x1008001c), s(0x10080018), s(0x10079178), s(0x10080014)].iter().any(|v| stricmp(w1, v) == 0);
    if lead && vol {
        let mut cons = r1;
        let (mut q, mut qo) = (p + r1 as usize, off + r1);
        let (mut t2, r2) = tk(fe, t, q, qo, 0);
        if bad(r2, &t2) {
            return -1;
        }
        let mut r2 = r2;
        let mut ok = true;
        if is_period(&t2) {
            cons += r2;
            q += r2 as usize;
            qo += r2;
            let x = tk(fe, t, q, qo, 0);
            t2 = x.0;
            r2 = x.1;
            if bad(r2, &t2) {
                return -1;
            }
        } else if t1.len != 2 || upper(w1[0]) != 0x50 {
            ok = false;
        }
        if ok && t2.class == 2 && t2.ws < 2 && num_str(&t2.text) {
            let word = match upper(w1[0]) {
                0x56 => s(0x1007ff80),
                0x4e => s(0x1007ff78),
                _ => s(0x1007ff70),
            };
            if !wa.emit(word, t1.start, t1.end, 0x41, 0x44, 0x16) {
                return 0;
            }
            let mut buf = new_buf();
            number(&mut buf, &t2.text, 1);
            if !wa.emit_split(&buf, t2.start, t2.end, 0x41, 0x44, 0x16) {
                return 0;
            }
            cons += r2;
            q += r2 as usize;
            qo += r2;
            let (t3, r3) = tk(fe, t, q, qo, 0);
            if t3.udict == 0 && t3.class == 3 && t3.len == 1 && t3.ws < 3 && matches!(t3.text[0], b'-' | b'~') {
                let (t4, r4) = tk(fe, t, q + r3 as usize, qo + r3, 0);
                if t4.udict == 0 && t4.class == 2 && t4.ws < 2 && num_str(&t4.text) {
                    if !wa.emit(s(0x10077380), t3.start, t3.end, 0x41, 0x44, 0x16) {
                        return 0;
                    }
                    number(&mut buf, &t4.text, 1);
                    if !wa.emit_split(&buf, t4.start, t4.end, 0x44, 0x44, 0x16) {
                        return 0;
                    }
                    return cons + t3.consumed + r4;
                }
            }
            return cons;
        }
        return table(fe, wa, t, p, off);
    }

    if is_mr(w1) {
        return honorifics(fe, wa, t, p, off, &t1, r1);
    }

    let mut lookback = [0u8; 96];
    if (stricmp(w1, s(0x10080004)) == 0 || stricmp(w1, s(0x10080000)) == 0) && upper1 {
        let bw = back_words(&mut lookback, t, p as isize - off as isize, p as isize, 3);
        let (a, b, c) = if bw as i16 != 0 { last_three(&lookback, off - bw, 0).unwrap_or_default() } else { Default::default() };
        let cap = |x: &Tok| x.len != 0 && x.class == 1 && cls(x.text[0]) & 0x80 != 0;
        // With no words before it, the engine skips last_three and tests its three
        // uninitialized token buffers anyway. In the 64-bit build (0x18003b790) the
        // middle one still holds this capitalized word from an earlier recognizer.
        let stale = bw as i16 == 0 && fe.x64;
        if stale || cap(&c) || cap(&b) || (cap(&a) && b.ws == 0 && b.len < 3 && b.text[0] == b'.') {
            let mut cons = r1;
            let (q, qo) = (p + r1 as usize, off + r1);
            let (t2, r2) = tk(fe, t, q, qo, 0);
            if t2.udict == 0 && is_period(&t2) && sbd::decide(fe, &t2, t, q, qo) != b'P' {
                cons += r2;
            }
            if w1[0] != b'C' {
                return if wa.emit(s(0x1007ffec), t1.start, t1.end, 0x41, 0x44, 0x16) { cons } else { 0 };
            }
            if (t2.class == 3 || t2.class == 7) && t2.len == 1 && t2.text[0] == b'-' && t2.ws == 0 {
                return -1;
            }
            return if wa.emit(s(0x1007fff8), t1.start, t1.end, 0x41, 0x44, 0x16) { cons } else { 0 };
        }
    } else if stricmp(w1, s(0x1007ffe8)) == 0 {
        let c = prev_one(t, p, off, 0);
        let mut cons = r1;
        let (mut q, mut qo) = (p + r1 as usize, off + r1);
        let (mut t2, r2) = tk(fe, t, q, qo, 0);
        if t2.udict == 0 && is_period(&t2) {
            cons += r2;
            q += r2 as usize;
            qo += r2;
            t2 = tk(fe, t, q, qo, 0).0;
        }
        if c.len > 0 && (c.class == 1 || c.class == 2) && t1.ws < 2 && !(t2.class == 1 && t2.ws < 2 && cls(t2.text[0]) & 0x40 == 0) {
            return if wa.emit(s(0x1007ffe0), t1.start, t1.end, 0x41, 0x44, 0x16) { t1.consumed } else { 0 };
        }
        if !wa.emit(s(0x1007ffd8), t1.start, t1.end, 0x41, 0x44, 0x16) {
            return 0;
        }
        if t2.class == 1 && cls(t2.text[0]) & 0x80 != 0 && t2.ws < 2 {
            return cons;
        }
        return t1.consumed;
    } else if stricmp(w1, s(0x1007ffd4)) == 0 {
        let mut cons = r1;
        let (mut q, mut qo) = (p + r1 as usize, off + r1);
        let (mut t2, r2) = tk(fe, t, q, qo, 0);
        if bad(r2, &t2) {
            return -1;
        }
        let mut t3 = Tok::default();
        if is_period(&t2) {
            cons += r2;
            q += r2 as usize;
            qo += r2;
            t3 = tk(fe, t, q, qo, 0).0;
            t2 = t3.clone();
        }
        if upper1 && t2.class == 1 && cls(t2.text[0]) & 0x80 != 0 && t2.ws < 2 {
            return if wa.emit(s(0x1007ffcc), t1.start, t1.end, 0x41, 0x44, 0x16) { cons } else { 0 };
        }
        if upper1 || (t3.len > 0 && t3.class == 1) {
            return if wa.emit(s(0x1007ffc4), t1.start, t1.end, 0x41, 0x44, 0x16) { t1.consumed } else { 0 };
        }
    } else if stricmp(w1, s(0x1007ffc0)) == 0 && upper1 {
        let mut cons = r1;
        let (mut q, mut qo) = (p + r1 as usize, off + r1);
        let (mut t2, r2) = tk(fe, t, q, qo, 0);
        if bad(r2, &t2) {
            return -1;
        }
        if is_period(&t2) {
            cons += r2;
            q += r2 as usize;
            qo += r2;
            t2 = tk(fe, t, q, qo, 0).0;
        }
        if t2.class == 1 && super::norm_address_city::cap_name(t2.text()) {
            return if wa.emit(s(0x1007ffb8), t1.start, t1.end, 0x41, 0x44, 0x16) { cons } else { 0 };
        }
    } else if stricmp(w1, s(0x1007ffb4)) == 0 || stricmp(w1, s(0x1007ffac)) == 0 {
        if t1.ws < 3 {
            let c = prev_one(t, p, off, 0);
            if c.len > 0 && c.class == 1 && cls(c.text[0]) & 0x80 != 0 {
                let w = if t1.len == 3 { s(0x1007ffa4) } else { s(0x1007ff9c) };
                return if wa.emit(w, t1.start, t1.end, 0x41, 0x44, 0x16) { r1 } else { 0 };
            }
        }
    } else if upper(w1[0]) == 0x57 && t1.ws < 3 && t1.len == 1 {
        let (q, qo) = (p + r1 as usize, off + r1);
        let (t2, r2) = tk(fe, t, q, qo, 0);
        if bad(r2, &t2) {
            return -1;
        }
        if t2.text[0] == b'/' && t2.ws == 0 && t2.len == 1 {
            let cons = r1 + r2;
            let (mut q, mut qo) = (q + r2 as usize, qo + r2);
            let (t3, r3) = tk(fe, t, q, qo, 0);
            if bad(r3, &t3) {
                return -1;
            }
            let (w, mut used) = if t3.class == 1 && t3.ws == 0 {
                (t3, r3)
            } else {
                q += r3 as usize;
                qo += r3;
                let (t4, r4) = tk(fe, t, q, qo, 0);
                if bad(r4, &t4) {
                    return -1;
                }
                (t4, r4)
            };
            let mut out = [0u8; 256];
            let unit = dict_find(&mut out, &DICT_E, &w.text, false)
                || dict_find(&mut out, &DICT_P, &w.text, false)
                || dict_find(&mut out, &DICT_C, &w.text, false)
                || phrase_lookup(fe, &mut out, &w.text, t, q, qo, &mut used);
            if !unit {
                return if wa.emit(s(0x10077680), t1.start, t2.end, 0x41, 0x44, 0x16) { cons } else { 0 };
            }
        }
    }
    table(fe, wa, t, p, off)
}

/// The "Mr"/"Mrs"/"Ms" half of `FUN_10032180` (from `0x10032ca7`).
fn honorifics(fe: &Fe, wa: &mut Wa, t: &[u8], p: usize, off: i32, t1: &Tok, r1: i32) -> i32 {
    let mut cons = r1;
    let (mut q, mut qo) = (p + r1 as usize, off + r1);
    let (mut t2, mut r2) = tk(fe, t, q, qo, 0);
    if bad(r2, &t2) {
        return -1;
    }
    let mut had_period = false;
    if is_period(&t2) {
        had_period = true;
        cons += r2;
        q += r2 as usize;
        qo += r2;
        let x = tk(fe, t, q, qo, 0);
        t2 = x.0;
        r2 = x.1;
    }
    if t2.len <= 0 {
        return table(fe, wa, t, p, off);
    }
    let w2 = t2.text();
    let conj = t2.udict == 0 && ((w2[0] == b'&' && t2.len == 1) || stricmp(w2, s(0x10077400)) == 0 || stricmp(w2, s(0x1007901c)) == 0) && t2.ws < 3;
    if conj {
        cons += r2;
        q += r2 as usize;
        qo += r2;
        let (t3, r3) = tk(fe, t, q, qo, 0);
        if bad(r3, &t3) {
            return -1;
        }
        if !is_mr(t3.text()) {
            return table(fe, wa, t, p, off);
        }
        if !wa.emit(honorific(t1), t1.start, t1.end, 0x41, 0x44, 0x16) {
            return 0;
        }
        let w = if upper(w2[0]) == 0x4f { s(0x1007901c) } else { s(0x10077400) };
        if !wa.emit(w, t2.start, t2.end, 0x41, 0x44, 0x16) {
            return 0;
        }
        if !wa.emit(honorific(&t3), t3.start, t3.end, 0x41, 0x44, 0x16) {
            return 0;
        }
        cons += r3;
        let (t4, r4) = tk(fe, t, q + r3 as usize, qo + r3, 0);
        if bad(r4, &t4) {
            return -1;
        }
        if is_period(&t4) {
            cons += r4;
        }
        return cons;
    }
    let cap = t2.class == 1 && cls(w2[0]) & 0x80 != 0 && t2.ws < 2;
    if !(had_period && cap) {
        let w1 = t1.text();
        let exact = strcmp(w1, s(0x10080010)) == 0 || strcmp(w1, s(0x1008000c)) == 0 || strcmp(w1, s(0x10080008)) == 0;
        if !exact || !cap {
            return table(fe, wa, t, p, off);
        }
    }
    if wa.emit(honorific(t1), t1.start, t1.end, 0x41, 0x44, 0x16) {
        cons
    } else {
        0
    }
}

/// `LAB_100334e7`: the two expansion tables, token mode 0x1b.
fn table(fe: &Fe, wa: &mut Wa, t: &[u8], p: usize, off: i32) -> i32 {
    let (t1, r) = tk(fe, t, p, off, 0x1b);
    if bad(r, &t1) {
        return -1;
    }
    let w1 = t1.text();
    let i = ABBR_I.find(w1, true);
    if i >= 0 {
        let (t2, _) = tk(fe, t, p + r as usize, off + r, 0);
        let word = if stricmp(w1, s(0x1007ff68)) == 0 && t2.len > 0 && t2.udict == 0 && stricmp(t2.text(), s(0x1007ff64)) == 0 {
            s(0x1007ff5c)
        } else {
            ABBR_I.field(i, 0x10)
        };
        if !wa.emit_split(word, t1.start, t1.end, 0x41, 0x44, 0x16) {
            return 0;
        }
        if stricmp(w1, s(0x1007ff58)) == 0 {
            let n = wa.n() as i32;
            wa.set_u8(Wa::rec(n - 1) + 0x24, 0x53);
        }
        return r;
    }
    let j = ABBR_S.find(w1, false);
    if j >= 0 {
        let (t2, _) = tk(fe, t, p + r as usize, off + r, 0);
        let word = if stricmp(w1, s(0x1007ff54)) == 0 && t2.len > 0 && t2.udict == 0 && stricmp(t2.text(), s(0x1007ff48)) == 0 {
            s(0x1007ff38)
        } else {
            ABBR_S.field(j, 0x10)
        };
        return if wa.emit_split(word, t1.start, t1.end, 0x41, 0x44, 0x16) { r } else { 0 };
    }
    -1
}

/// `FUN_10031db0`: a letters-only word directly followed (no space) by a
/// digit run — the letters as one word (tag `S` spelled-letter-by-letter or
/// `D` word-like, by `no_lowercase`/`vowel_count`), the digits via
/// `numwords::code` (which already handles a leading `0` by falling back to
/// plain cardinal reading — confirmed live: `"009"` -> `"nine"`, not
/// `"oh oh nine"`).
fn ugc_code(fe: &Fe, wa: &mut Wa, t: &[u8], p: usize, off: i32) -> i32 {
    let (w1, r1) = tk(fe, t, p, off, 0x1a);
    if r1 == 0 || w1.class == 9 || w1.class != 1 || w1.udict != 0 {
        return -1;
    }
    if !all_alpha(&w1.text) {
        return -1;
    }
    let (w2, r2) = tk(fe, t, p + r1 as usize, off + r1, 0x1a);
    if r2 == 0 || w2.class == 9 || w2.udict != 0 || w2.class != 2 || w2.ws != 0 {
        return -1;
    }

    let b = if no_lowercase(&w1.text) {
        if vowel_count(&w1.text) != 0 && w1.len > 2 {
            if w1.len == 1 {
                0x53
            } else {
                0x44
            }
        } else {
            0x53
        }
    } else if w1.len == 1 {
        0x53
    } else {
        0x44
    };
    if !wa.emit(&w1.text, w1.start, w1.end, 0x41, b, 0x16) {
        return 0;
    }

    let mut buf = new_buf();
    let d = w2.text();
    if !ordinal_numeral(d) {
        let q = p + (r1 + r2) as usize;
        let (x3, r3) = tk(fe, t, q, off + r1 + r2, 0x1a);
        if x3.class == 3 && x3.ws == 0 && x3.text[0] == b'\'' && x3.len == 1 {
            let (x4, _) = tk(fe, t, q + r3 as usize, off + r1 + r2 + r3, 0xc);
            let w4 = x4.text();
            if x4.class == 1 && x4.ws == 0 && (stricmp(w4, s(0x10077770)) == 0 || stricmp(w4, s(0x100778f0)) == 0 || stricmp(w4, s(0x100778ec)) == 0) {
                return r1 + r2 - w2.consumed;
            }
        }
        code(&mut buf, d);
        return if wa.emit_split(&buf, w2.start, w2.end, 0x44, 0x44, 0x16) { r1 + r2 } else { 0 };
    }
    let sfx = &d[d.len() - 2..];
    if [s(0x1007ff34), s(0x1007ff30), s(0x1007ff2c), s(0x10079164)].iter().any(|x| strcmp(sfx, x) == 0) {
        code(&mut buf, &d[..d.len() - 2]);
        if !wa.emit_split(&buf, w2.start, w2.end - 2, 0x44, 0x44, 0x16) {
            return 0;
        }
        if !wa.emit(sfx, w2.end - 2, w2.end, 0x41, 0x53, 0x16) {
            return 0;
        }
    } else {
        ordinal_numeral_words(&mut buf, d);
        if !wa.emit_split(&buf, w2.start, w2.end, 0x44, 0x44, 0x16) {
            return 0;
        }
    }
    r1 + r2
}

fn tolower(c: u8) -> u8 {
    if c.is_ascii_uppercase() {
        c + 32
    } else {
        c
    }
}
