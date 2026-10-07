//! `FUN_10059560`: `n15`'s first sub-recognizer, a person-name grammar over
//! up to 5 tokens (mode 0xc). Each word gets a kind: `T` title (`"Dr"` ->
//! "doctor", or `"Ex"` before a dash), `N` capitalized name (or a
//! `"Chas"`-style abbreviated first name), `I` initial, `Q` qualifier
//! (`"Jr"`, `"Esq"`), `R` Roman numeral after the first word, `s` a
//! possessive right after a lone title. Periods after abbreviations, one comma
//! after a name, and the dash after `"Ex"` are absorbed without a slot.
//!
//! The kind string, shortened one slot at a time, must equal a pattern in
//! `PTR_s_TNINQ_1009c298` for its leading kind; `FUN_10059fc0` then vetoes
//! some ("MS"/"JUN" titles, "Co", title-less `NNNN`), and `FUN_10059ef0`
//! emits every slot with class 0x16.

use super::cstr::*;
use super::norm_address_city::cap_name;
use super::numwords::{new_buf, number, roman};
use super::tabs::{ptab, rec30, Ptab, Rec30};
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::Fe;
use crate::lexicon::encode_key;

const TITLES: Rec30 = rec30(0x1009a058, 0x1009b110);
const TITLES_P: Ptab = ptab(0x1009a838, 0x1009b114);
const QUALS: Rec30 = rec30(0x1009a8c0, 0x1009b118);
const QUALS_P: Ptab = ptab(0x1009a9e0, 0x1009b11c);
const NAMES: Rec30 = rec30(0x1009a9f0, 0x1009b10c);
const PATTERNS: Ptab = ptab(0x1009c298, 0x1009c304);

fn tk(fe: &Fe, t: &[u8], p: usize, off: i32) -> (Tok, i32) {
    let mut x = Tok::default();
    let r = tokenize(&mut x, t, p, off, 0xc, fe.ud.as_ref());
    (x, r)
}

#[derive(Default)]
struct Slot {
    start: i32,
    end: i32,
    len: i32,
    tag: u8,
    comma: i16,
    period: i16,
    text: Vec<u8>,
    exp: Vec<u8>,
}

/// `FUN_1005a300`: a `'` followed by whitespace, or `'s` not followed by a letter.
fn possessive(t: &[u8], p: usize) -> bool {
    if at(t, p as isize) != b'\'' {
        return false;
    }
    let c = at(t, p as isize + 1);
    if matches!(c, b' ' | b'\t' | b'\n' | b'\r') {
        return true;
    }
    lower(c) == 0x73 && cls(at(t, p as isize + 2)) & 0xc0 == 0
}

/// `FUN_1005a200`: an `I`/`V`/`X` initial must be followed by `.` and a
/// capitalized name.
fn initial_ok(fe: &Fe, x: &Tok, t: &[u8], p: usize, off: i32) -> bool {
    if !strchr(s(0x1009c3b0), x.text[0]) {
        return true;
    }
    let (d, r) = tk(fe, t, p, off);
    if r == 0 || d.ws != 0 || d.len > 2 || d.text[0] != b'.' {
        return false;
    }
    let (w, r2) = tk(fe, t, p + r as usize, off + r);
    if r2 == 0 || (w.nl == 0 && w.ws > 2) || (w.class == 1 && cls(w.text[0]) & 0x80 == 0) {
        return false;
    }
    cap_name(w.text())
}

/// `FUN_10003e90(w, 1, lexicon)`: a capitalized word that is not in the
/// lexicon, or whose entry has flag 0x40.
fn proper(fe: &Fe, w: &[u8]) -> bool {
    match fe.emb.get(&encode_key(w)) {
        Some(p) if !p.is_empty() && p[0] != 0 => p[0] & 0x40 != 0,
        _ => true,
    }
}

/// `FUN_10059fc0`.
fn validate(sl: &mut [Slot], idx: i32) -> bool {
    let ms = |s0: &Slot| stricmp(&s0.text, s(0x100805bc)) == 0 && s0.comma == 0;
    let jun = |s0: &Slot| stricmp(&s0.text, s(0x1009c3ac)) == 0 && s0.comma == 0;
    match idx {
        4 | 0xc => sl[0].tag == b't' && !ms(&sl[0]) && !jun(&sl[0]),
        8 => sl[0].tag == b't' && !ms(&sl[0]) && !jun(&sl[0]) && stricmp(&sl[2].text, s(0x1009c3a8)) != 0,
        0xd => {
            if sl[0].len == 3 && strnicmp(&sl[0].text, s(0x100805bc), 2) == 0 && sl[0].comma == 0 {
                return false;
            }
            if sl[0].len == 5 && strnicmp(&sl[0].text, s(0x1009c3ac), 3) == 0 && sl[0].comma == 0 {
                return false;
            }
            sl[1].start = sl[0].start;
            sl[0].end = sl[1].end;
            true
        }
        0x12 => sl[..4].iter().any(|x| x.tag == b'A'),
        0x13 => !(sl[0].tag == b'W' && sl[2].tag == b'W'),
        0x14 => sl[2].tag == b'q',
        0x15 => !(sl[0].tag == b'W' || sl[1].tag == b'W'),
        0x16 => sl[1].tag == b't',
        _ => true,
    }
}

/// `FUN_10059ef0`.
fn emit(wa: &mut Wa, sl: &[Slot]) -> bool {
    // paul09 FUN_10054400: no 'E' text case and no comma pause.
    let paul = era() == crate::era::Era::Paul09;
    for x in sl {
        let ok = match x.tag {
            b's' => true,
            b'I' => wa.emit_split(&x.text, x.start, x.end, 0x41, 0x53, 0x16),
            b'N' | b'W' => wa.emit_split(&x.text, x.start, x.end, 0x41, 0x44, 0x16),
            b'E' if !paul => wa.emit_split(&x.text, x.start, x.end, 0x41, 0x44, 0x16),
            _ => wa.emit_split(&x.exp, x.start, x.end, 0x41, 0x44, 0x16),
        };
        if !ok {
            return false;
        }
        if x.comma == 1 && !paul {
            let r = wa.n() as i32 - 1;
            wa.set_ri32(r, 0x2c, 5);
        }
    }
    true
}

pub fn name(fe: &Fe, wa: &mut Wa, t: &[u8], p: usize, off: i32) -> i32 {
    let mut sl: Vec<Slot> = Vec::new();
    let mut kinds: Vec<u8> = Vec::new();
    let mut tags: Vec<u8> = Vec::new();
    let (mut p, mut o) = (p, off);
    let mut comma_seen = false;
    let mut ex = false;
    while sl.len() < 5 {
        let (x, r) = tk(fe, t, p, o);
        let n = sl.len();
        if r == 0 || x.class == 9 || (x.nl == 0 && x.ws > 2) || (x.class == 1 && cls(x.text[0]) & 0x80 == 0) || (n > 0 && x.udict != 0) {
            break;
        }
        let punct = (x.class == 3 || x.class == 7) && x.len == 1;
        let prev = tags.last().copied().unwrap_or(0);
        if x.ws == 0 && x.len < 3 && x.text[0] == b'.' && !tags.is_empty() && matches!(prev, b'A' | b'I' | b'q' | b't') {
            tags.push(b'P');
            sl[n - 1].period = 1;
            p += r as usize;
            o += r;
            continue;
        }
        let tag;
        let mut exp: Vec<u8> = Vec::new();
        if x.class == 1 && x.len == 2 && stricmp(s(0x1009b908), &x.text) == 0 {
            kinds.push(b'T');
            tag = b'E';
            ex = true;
        } else if ex && x.ws == 0 && punct && x.text[0] == b'-' && prev == b'E' {
            tags.push(b'-');
            sl[n - 1].comma = 2;
            ex = false;
            p += r as usize;
            o += r;
            continue;
        } else if !comma_seen && punct && x.text[0] == b',' && n > 0 && kinds[n - 1] == b'N' {
            tags.push(b',');
            comma_seen = true;
            sl[n - 1].comma = 1;
            p += r as usize;
            o += r;
            continue;
        } else if n == 1 && kinds[0] == b'T' && (stricmp(&x.text, s(0x100773a0)) == 0 || stricmp(&x.text, s(0x1009c3a4)) == 0) && possessive(t, p) {
            let s0 = &mut sl[0];
            s0.end += r;
            s0.len += r + (s0.period != 0) as i32;
            s0.text.extend_from_slice(x.text());
            s0.exp.extend_from_slice(x.text());
            kinds.push(b's');
            sl.push(Slot { start: x.start, end: o + r, len: x.len, tag: b's', text: x.text().to_vec(), ..Default::default() });
            break;
        } else {
            let mut classified = None;
            if x.class == 1 {
                let i = NAMES.find(&x.text, true);
                if i >= 0 {
                    classified = Some((b'N', b'A', NAMES.field(i, 0x10).to_vec()));
                } else if x.len == 1 && !(n > 1 && matches!(kinds[0], b'N' | b'T') && kinds[1] == b'N' && !initial_ok(fe, &x, t, p + r as usize, o + r)) {
                    classified = Some((b'I', b'I', Vec::new()));
                }
            }
            let (k, tg, e) = match classified {
                Some(c) => c,
                None => {
                    if x.class != 1 {
                        break;
                    }
                    let mut res = None;
                    if n >= 1 {
                        let qi = QUALS.find(&x.text, true);
                        if qi >= 0 {
                            res = Some((b'Q', b'q', QUALS.field(qi, 0x10).to_vec()));
                        } else {
                            let qp = QUALS_P.find(&x.text, true);
                            if qp >= 0 {
                                res = Some((b'Q', b'Q', QUALS_P.get(qp).to_vec()));
                            } else {
                                let rv = roman(&x.text);
                                if rv >= 0 {
                                    let mut buf = new_buf();
                                    number(&mut buf, format!("{}", rv).as_bytes(), 1);
                                    res = Some((b'R', b'R', cs(&buf).to_vec()));
                                }
                            }
                        }
                    }
                    match res {
                        Some(c) => c,
                        None => {
                            let ti = TITLES.find(&x.text, true);
                            if ti >= 0 {
                                (b'T', b't', TITLES.field(ti, 0x10).to_vec())
                            } else {
                                let tp = TITLES_P.find(&x.text, true);
                                if tp >= 0 {
                                    (b'T', b'T', TITLES_P.get(tp).to_vec())
                                } else if cap_name(x.text()) {
                                    (b'N', if proper(fe, x.text()) { b'N' } else { b'W' }, Vec::new())
                                } else {
                                    break;
                                }
                            }
                        }
                    }
                }
            };
            if k == b'T' && stricmp(s(0x1009b908), &e) == 0 {
                ex = true;
            }
            kinds.push(k);
            tag = tg;
            exp = e;
        }
        tags.push(tag);
        sl.push(Slot { start: x.start, end: o + r, len: x.len, tag, text: x.text().to_vec(), exp, ..Default::default() });
        p += r as usize;
        o += r;
    }

    let mode = match kinds.first() {
        Some(b'T') => 1,
        Some(b'N') => 2,
        Some(b'I') => 3,
        _ => return -1,
    };
    let mut n = sl.len();
    while n >= 1 {
        // paul09 FUN_10053be0: 26 shapes, "TNR" missing; its validator cases from 9 on are the 2014 ones minus 1.
        let paul = era() == crate::era::Era::Paul09;
        let range = if paul {
            match (mode, n) {
                (1, 5) => Some((0, 3)),
                (1, 4) => Some((3, 7)),
                (1, 3) => Some((7, 0xa)),
                (1, 2) => Some((0xa, 0xd)),
                (2, 5) => Some((0xd, 0xe)),
                (2, 4) => Some((0xe, 0x12)),
                (2, 3) => Some((0x12, 0x16)),
                (2, 2) => Some((0x16, 0x17)),
                (3, 4) => Some((0x17, 0x19)),
                (3, 3) => Some((0x19, PATTERNS.count())),
                _ => None,
            }
        } else {
        match (mode, n) {
            (1, 5) => Some((0, 3)),
            (1, 4) => Some((3, 7)),
            (1, 3) => Some((7, 0xb)),
            (1, 2) => Some((0xb, 0xe)),
            (2, 5) => Some((0xe, 0xf)),
            (2, 4) => Some((0xf, 0x13)),
            (2, 3) => Some((0x13, 0x17)),
            (2, 2) => Some((0x17, 0x18)),
            (3, 4) => Some((0x18, 0x1a)),
            (3, 3) => Some((0x1a, PATTERNS.count())),
            _ => None,
        }
        };
        if let Some((a, b)) = range {
            let ks = &kinds[..n];
            for i in a..b {
                if PATTERNS.get(i) == ks {
                    let ret = sl[n - 1].end - off;
                    if validate(&mut sl, if paul && i >= 9 { i + 1 } else { i }) {
                        return if emit(wa, &sl[..n]) { ret } else { 0 };
                    }
                    break;
                }
            }
        }
        n -= 1;
    }
    -1
}
