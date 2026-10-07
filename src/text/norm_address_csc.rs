//! `FUN_10053920`: address's state/country/zip sub-recognizer, the first of
//! the four `FUN_10033760` dispatches (flag 0) and the tail `FUN_10033b60`
//! recurses into after a city name (flag 1). Transliterated from the
//! decompile, emitter `FUN_10054050` checked against its disassembly.
//!
//! Up to 5 tokens (mode 0x15) become slots, each with a tag and a source:
//!
//! | tag | source | token |
//! |---|---|---|
//! | `S` | `A` | first token, 2-letter US state / Canadian province code |
//! | `S` | `a` | first token, informal abbreviation ("Calif") via its index field |
//! | `S` | `F` | first token, full state/province name phrase |
//! | `N` | `N` | "canada"/"USA"/"United States" right after an `S` |
//! | `Z` | `U`/`u` | 4-5 digit zip (after `S`/`N`, or after a `U`/`L` token) |
//! | `Z` | `C`/`c` | Canadian postal half (after `S`/`N`, or after a `C`/`L` token) |
//! | `L` | `L` | `-`/`~` after a `U`/`C` token |
//!
//! A `.` right after an `A`/`a`/`F` token and a `,` right after an
//! `A`/`N`/`a`/`F`/`P` token are absorbed without making a slot. The tag
//! string is tried longest first against `SNZLZ SZLZ SNZZ SZZ SNZ SZ SN S`
//! (`PTR_DAT_10081490`), each accepted match then checked by `FUN_100541a0`:
//! the connector must have the same leading whitespace as the zip after it,
//! "Mass" + zip needs its period, and a bare `S` is only taken with flag 1,
//! not from a full-name phrase, and not before a capitalized word.

use super::cstr::*;
use super::numwords::pairs;
use super::tabs::{ptab, rec30, search, Ptab, Rec30};
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::Fe;

const US_STATE: Rec30 = rec30(0x10097658, 0x1009b0d0);
const CA_PROV: Rec30 = rec30(0x10098cd8, 0x1009b0d4);
const US_INFORMAL: Rec30 = rec30(0x10098228, 0x1009b0d8);
const STATE_NAMES: Ptab = ptab(0x10098f48, 0x1009b0dc);
const PROV_NAMES: Ptab = ptab(0x1009903c, 0x1009b0e0);
const PATTERNS: Ptab = ptab(0x10081490, 0x100814b0);

fn tk(fe: &Fe, t: &[u8], p: usize, off: i32, mode: i32) -> (Tok, i32) {
    let mut x = Tok::default();
    let r = tokenize(&mut x, t, p, off, mode, fe.ud.as_ref());
    (x, r)
}

/// `FUN_1000ffa0`: every character a letter.
fn all_alpha(w: &[u8]) -> bool {
    !w.is_empty() && w.iter().all(|&c| cls(c) & 0xc0 != 0)
}

/// `FUN_10061940`: every character a digit.
fn all_digits(w: &[u8]) -> bool {
    !w.is_empty() && w.iter().all(|&c| cls(c) & 0x10 != 0)
}

/// `FUN_100542f0`: a 2-letter code in a state/province table.
fn code2(w: &[u8], tab: Rec30) -> i32 {
    if w.len() == 2 && all_alpha(w) && is_upper(w[0]) {
        let i = tab.find(w, true);
        if i >= 0 {
            return i;
        }
    }
    -1
}

/// `FUN_10054350` -> `FUN_10056960`: bisect the informal abbreviations with
/// `strnicmp(record, word, strlen(word))`; a record that continues with a
/// space (or, for a one-letter word from `DAT_1009c0ec`, with ". " matching
/// the text: "N. Dak") is matched on against the following tokens. `t` is
/// relative to the token's start; `used` is its return, advanced on a
/// multi-word match.
fn informal(fe: &Fe, w: &[u8], t: &[u8], p: usize, off: i32, used: &mut i32) -> i32 {
    if w.is_empty() || !is_upper(w[0]) {
        return -1;
    }
    let tp = |i: i32| at(t, p as isize + i as isize);
    let mut hi = US_INFORMAL.count() - 1;
    let mut lo = 0;
    if hi < 0 {
        return -1;
    }
    let mut mid = hi;
    loop {
        mid /= 2;
        let rec = US_INFORMAL.field(mid, 0);
        let mut n = w.len();
        let c = strnicmp(rec, w, n);
        let mut go_low = c >= 0;
        if c == 0 {
            go_low = true;
            if at(rec, n as isize) == 0 {
                return mid;
            }
            let u = *used;
            let dotted = n == 1 && strchr(s(0x1009c0ec), w[0]) && at(rec, 1) == tp(u) && at(rec, 1) == b'.' && at(rec, 2) == tp(u + 1) && at(rec, 2) == b' ';
            let spaced = at(rec, n as isize) == b' ' && tp(u) == b' ';
            if dotted {
                n = 2;
                *used = u + 1;
            }
            if dotted || spaced {
                let mut rest = &rec[n + 1..];
                let mut pos = if dotted { *used } else { *used + 1 };
                if tp(pos) != 0 {
                    loop {
                        let (x, r) = tk(fe, t, p + pos as usize, off + pos, 0x15);
                        if r == 0 || x.class == 9 || x.len == 0 {
                            break;
                        }
                        let l = x.len as usize;
                        let c2 = strnicmp(rest, &x.text, l);
                        if c2 != 0 {
                            go_low = c2 >= 0;
                            break;
                        }
                        if at(rest, l as isize) == 0 {
                            *used = pos + r;
                            return mid;
                        }
                        if at(rest, l as isize) != b' ' || tp(pos + r) != b' ' {
                            break;
                        }
                        rest = &rest[l + 1..];
                        pos += 1 + x.len;
                        if tp(pos) == 0 {
                            break;
                        }
                    }
                }
            }
        }
        if go_low {
            hi = mid - 1;
        } else {
            lo = mid + 1;
        }
        mid = hi + lo;
        if hi < lo {
            return -1;
        }
    }
}

/// `FUN_100567c0` as `FUN_100543a0` calls it: a (possibly multi-word) name
/// phrase from `tab`, matched case-insensitively word by word. Returns the
/// entry and the bytes consumed.
fn phrase(fe: &Fe, tab: Ptab, w0: &[u8], t: &[u8], p: usize, off: i32, consumed0: i32) -> Option<(i32, i32)> {
    let w0 = cs(w0);
    if w0.is_empty() {
        return None;
    }
    let idx = search(tab.count(), true, w0, |i| tab.get(i));
    if idx >= 0 {
        return Some((idx, consumed0));
    }
    for i in 0..tab.count() {
        let cand = tab.get(i);
        if strnicmp(cand, w0, w0.len()) != 0 || at(cand, w0.len() as isize) != b' ' {
            continue;
        }
        let mut coff = w0.len() + 1;
        let mut consumed = consumed0;
        loop {
            if at(t, p as isize + consumed as isize) != b' ' {
                break;
            }
            let (w, r) = tk(fe, t, p + consumed as usize, off + consumed, 0x15);
            if r == 0 || w.class == 9 || w.len == 0 {
                break;
            }
            let rest = &cand[coff..];
            let wl = w.len as usize;
            if strnicmp(rest, &w.text, wl) != 0 {
                break;
            }
            match at(rest, wl as isize) {
                0 => return Some((i, consumed + r)),
                b' ' => {
                    coff += wl + 1;
                    consumed += r;
                }
                _ => break,
            }
        }
    }
    None
}

/// `FUN_10054410`: Canadian postal FSA (letter digit letter, 1) or LDU
/// (digit letter digit, 2), with the letters Canada Post never uses there.
/// `pub(super)`: also used by `norm_vtml`'s `R` reader and `initial`'s
/// `sapi:address` converter.
pub(super) fn postal(w: &[u8]) -> i32 {
    if w.len() != 3 {
        return 0;
    }
    let (c0, c1, c2) = (w[0], w[1], w[2]);
    if cls(c0) & 0xc0 != 0 && cls(c1) & 0x10 != 0 && cls(c2) & 0xc0 != 0 {
        let u = upper(c0);
        if u != 0 && strchr(s(0x10081504), u as u8) {
            return 0;
        }
        let u = upper(c2);
        if u != 0 && strchr(s(0x100814fc), u as u8) {
            return 0;
        }
        return 1;
    }
    if cls(c0) & 0x10 != 0 && cls(c1) & 0xc0 != 0 && cls(c2) & 0x10 != 0 {
        let u = upper(c1);
        if u != 0 && strchr(s(0x100814fc), u as u8) {
            return 0;
        }
        return 2;
    }
    0
}

/// `FUN_10062cd0`: letters as typed, digits as words (`0` -> "oh").
/// `pub(super)`: also used by `norm_vtml`'s `R` reader and `initial`'s
/// `sapi:address` converter.
pub(super) fn mixed_code(src: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    for &c in cs(src) {
        if !buf.is_empty() {
            buf.push(b' ');
        }
        if is_alpha(c) {
            buf.push(c);
        } else {
            let w = match c {
                b'0' => s(0x1009c740),
                b'1' => s(0x10078a64),
                b'2' => s(0x10078850),
                b'3' => s(0x1007887c),
                b'4' => s(0x10078994),
                b'5' => s(0x100789b4),
                b'6' => s(0x100788e0),
                b'7' => s(0x10078904),
                b'8' => s(0x100789f8),
                b'9' => s(0x10078958),
                _ => b"",
            };
            buf.extend_from_slice(w);
        }
    }
    buf
}

struct Slot {
    tag: u8,
    src: u8,
    text: Vec<u8>,
    exp: Vec<u8>,
    ws: i32,
    start: i32,
    end: i32,
    period: bool,
    p_after: usize,
}

/// `FUN_100541a0`.
fn validate(fe: &Fe, t: &[u8], sl: &[Slot], pat: i32, flag: bool) -> bool {
    match pat {
        0 => sl[3].ws == sl[4].ws,
        1 => sl[2].ws == sl[3].ws,
        5 => !(stricmp(&sl[0].text, s(0x100814f4)) == 0 && !sl[0].period),
        7 => {
            if !flag || sl[0].src == b'F' {
                return false;
            }
            let (mut x, r) = tk(fe, t, sl[0].p_after, 0, 0);
            if sl[0].period {
                x = tk(fe, t, sl[0].p_after + r as usize, 0, 0).0;
            }
            x.len < 1 || x.class != 1 || cls(x.text[0]) & 0x80 == 0 || x.ws > 2
        }
        _ => true,
    }
}

/// `FUN_10054050`.
fn emit(wa: &mut Wa, sl: &[Slot]) -> bool {
    for s_ in sl {
        let u = upper(s_.src);
        if u != 0 && strchr(s(0x1007d3d4), u as u8) {
            if !wa.emit_split(&s_.exp, s_.start, s_.end, 0x44, 0x44, 0x12) {
                return false;
            }
        } else if s_.src == b'L' {
            let w = if s_.text[0] == b'-' { s(0x100779a8) } else { s(0x10077380) };
            if !wa.emit(w, s_.start, s_.end, 0x53, 0x44, 0x12) {
                return false;
            }
        } else {
            if !wa.emit_split(&s_.exp, s_.start, s_.end, 0x41, 0x44, 0x12) {
                return false;
            }
            if sl.len() > 1 {
                let r = wa.n() as i32 - 1;
                wa.set_ri32(r, 0x2c, 10);
            }
        }
    }
    true
}

/// Returns the bytes consumed; on no match -1 with flag 0, 0 with flag 1.
pub fn state_zip(fe: &Fe, wa: &mut Wa, t: &[u8], p: usize, off: i32, flag: bool) -> i32 {
    let mut sl: Vec<Slot> = Vec::new();
    let (mut p, mut o) = (p, off);
    let mut prev_src = 0u8;
    let mut k = 0;
    let e09 = fe.era.is_2009();
    // julie09 FUN_1004fbe0: at most 4 slots and no country slot.
    while sl.len() <= if e09 { 3 } else { 4 } {
        let (w, r0) = tk(fe, t, p, o, 0x15);
        if r0 == 0 || w.class == 9 {
            break;
        }
        let mut r = r0;
        let text = w.text().to_vec();
        let c0 = at(&text, 0);
        if w.ws == 0 && w.len < 3 && c0 == b'.' && k > 0 && matches!(prev_src, b'A' | b'a' | b'F') {
            prev_src = b'P';
            p += r as usize;
            o += r;
            k += 1;
            sl.last_mut().unwrap().period = true;
            continue;
        }
        if (w.class == 3 || w.class == 7) && w.len == 1 && c0 == b',' && w.ws < 3 && k > 0 && matches!(prev_src, b'A' | b'N' | b'a' | b'F' | b'P') {
            prev_src = b',';
            p += r as usize;
            o += r;
            k += 1;
            continue;
        }
        let prev_tag = sl.last().map(|x| x.tag).unwrap_or(0);
        let country = stricmp(&text, s(0x100814ec)) == 0 || stricmp(&text, s(0x100814e8)) == 0 || stricmp(&text, s(0x100814d8)) == 0;
        let (tag, src, exp): (u8, u8, Vec<u8>) = if !e09 && prev_tag == b'S' && country && w.ws <= 2 {
            (b'N', b'N', text.clone())
        } else if k >= 1 && matches!(prev_src, b'U' | b'C') && w.class == 3 && w.len == 1 && c0 != 0 && strchr(s(0x10080540), c0) && w.ws <= 2 {
            (b'L', b'L', Vec::new())
        } else {
            let mut first: Option<(u8, u8, Vec<u8>)> = None;
            if k == 0 && w.class == 1 {
                let i = code2(&text, US_STATE);
                let j = if i < 0 { code2(&text, CA_PROV) } else { -1 };
                if i >= 0 {
                    first = Some((b'S', b'A', US_STATE.field(i, 0x10).to_vec()));
                } else if j >= 0 {
                    first = Some((b'S', b'A', CA_PROV.field(j, 0x10).to_vec()));
                } else {
                    let a = informal(fe, &text, t, p, o, &mut r);
                    if a >= 0 {
                        let idx = atoi(US_INFORMAL.field(a, 0x10));
                        first = Some((b'S', b'a', US_STATE.field(idx, 0x10).to_vec()));
                    } else if let Some((i, c)) = phrase(fe, STATE_NAMES, &text, t, p, o, r) {
                        r = c;
                        first = Some((b'S', b'F', STATE_NAMES.get(i).to_vec()));
                    } else if let Some((i, c)) = phrase(fe, PROV_NAMES, &text, t, p, o, r) {
                        r = c;
                        first = Some((b'S', b'F', PROV_NAMES.get(i).to_vec()));
                    }
                }
            }
            match first {
                Some(x) => x,
                None => {
                    if sl.is_empty() {
                        break;
                    }
                    let sn = prev_tag == b'S' || prev_tag == b'N';
                    if (sn || prev_src == b'U' || prev_src == b'L') && w.class == 2 && w.ws < 3 && (4..6).contains(&w.len) && all_digits(&text) {
                        let mut b2 = [0u8; 256];
                        pairs(&mut b2, &text);
                        (b'Z', if sn { b'U' } else { b'u' }, cs(&b2).to_vec())
                    } else if (sn || prev_src == b'C' || prev_src == b'L') && (w.class == 2 || w.class == 1) && w.ws <= 2 && postal(&text) != 0 {
                        (b'Z', if sn { b'C' } else { b'c' }, mixed_code(&text))
                    } else {
                        break;
                    }
                }
            }
        };
        prev_src = src;
        p += r as usize;
        o += r;
        sl.push(Slot { tag, src, text, exp, ws: w.ws, start: w.start, end: o, period: false, p_after: p });
        k += 1;
    }

    let tags: Vec<u8> = sl.iter().map(|x| x.tag).collect();
    let mut n = sl.len();
    while n >= 1 {
        let (lo, hi) = if e09 {
            // The 4-entry table (SZLZ SZZ SZ S) uses the 2014 validator cases 1, 3, 5, 7.
            match n {
                4 => (0, 1),
                3 => (1, 2),
                2 => (2, 3),
                _ => (3, PATTERNS.count()),
            }
        } else {
            match n {
                5 => (0, 1),
                4 => (1, 3),
                3 => (3, 5),
                2 => (5, 7),
                _ => (7, PATTERNS.count()),
            }
        };
        for i in lo..hi {
            if strcmp(PATTERNS.get(i), &tags[..n]) == 0 {
                let case = if e09 { 2 * i + 1 } else { i };
                if validate(fe, t, &sl[..n], case, flag) {
                    let consumed = sl[n - 1].end - off;
                    return if emit(wa, &sl[..n]) { consumed } else { 0 };
                }
                break;
            }
        }
        n -= 1;
    }
    if flag {
        0
    } else {
        -1
    }
}

pub fn city_state_country(fe: &Fe, wa: &mut Wa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let _ = tok;
    state_zip(fe, wa, t, p, off, false)
}

/// The flag-1 entry `FUN_10033b60` recurses into after a city name.
pub fn trailing(fe: &Fe, wa: &mut Wa, t: &[u8], p: usize, off: i32) -> i32 {
    state_zip(fe, wa, t, p, off, true)
}
