//! `FUN_100344e0`: currency amounts ("$5", "100 dollars", "$2.5 million",
//! "US$ 5", "£20", "$5, 10 and 15", ...). Transliterated from the decompile
//! of `FUN_100344e0` and its callees: the slot scan, the list/range numeral
//! extenders (`FUN_100364b0`/`FUN_100365f0`), the pattern groups over the 25
//! tag strings at `0x100800a4`, the validator (`FUN_10036160`) with its
//! next-token context (`FUN_10036370`), and the emitters (`FUN_10034f30`,
//! `FUN_10035950`, `FUN_10035030`).
//!
//! Up to six slots are scanned: the shared mode-1 token, then mode-5 tokens.
//! Each is tagged '$' (symbol), 'S' (country prefix), 'F'/'f' (spelled name),
//! 'C' (ISO code), 'U' (scale word) or 'N' (numeral, or "NUMS" for a list or
//! range), each at most once, or '('/')'. A '-' between a numeral and a scale
//! word and a '.' after a scale word are absorbed without a slot. The engine's
//! multi-word name continuation in `FUN_10056e50` is unreachable (keys like
//! "new lira" are compared over their full length against one token), so
//! names are single-token, as `Rec66::find_word` does.

use super::cstr::*;
use super::numwords::*;
use super::tabs::{ptab, rec30, rec5a, rec66, Ptab, Rec30, Rec5a, Rec66};
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::Fe;

/// Symbol records (rec5a): key @0, name @0x1e, plural @0x3c.
const SYMBOL_TAB: Rec5a = rec5a(0x10082350, 0x1009b050);
/// Country prefixes before a symbol ("A", "US", "HK", ...) -> qualifier @0x10.
const PREFIX_TAB: Rec30 = rec30(0x100827e8, 0x1009b054);
/// Spelled names (rec66): key @0, plural @0xf, subunit @0x32, its plural @0x41.
const NAME_TAB: Rec66 = rec66(0x10087150, 0x1009b05c);
/// Case-sensitive spelled names ("EMU six", "SDR", ...), same layout.
const NAME_TAB2: Rec66 = rec66(0x100899f8, 0x1009b060);
/// ISO codes (rec66): key @0, name @0xf, plural @0x32, qualifier @0x41.
const CODE_TAB: Rec66 = rec66(0x10082d28, 0x1009b058);
/// Scale words: "b"/"bn"/"billion" -> "billion" @0x10, ...
const SCALE_TAB: Rec30 = rec30(0x10082140, 0x1009b04c);
const PATTERNS: Ptab = ptab(0x100800a4, 0x10080108);

const AND: u32 = 0x10077400;
const TO: u32 = 0x10077380;

#[derive(Clone, Default)]
struct Slot {
    ws: i32,
    start: i32,
    end: i32,
    tag: u8,
    idx: i32,
    /// Text position the slot's bytes end at ("NUMS": where they start).
    tptr: usize,
    text: Vec<u8>,
    /// Prefix qualifier ('S') or scale word ('U').
    disp: Vec<u8>,
}

fn tk(fe: &Fe, x: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    tokenize(x, t, p, off, 5, fe.ud.as_ref())
}

fn first(x: &Tok) -> u8 {
    at(&x.text, 0)
}

/// `FUN_1000ffd0`: no lowercase letter.
fn no_lowercase(w: &[u8]) -> bool {
    cs(w).iter().all(|&c| cls(c) & 0xc0 == 0 || cls(c) & 0x40 == 0)
}

/// `FUN_10062530`: `key` + the "-suffix" plural, the irregular plural, or
/// `key` when the plural field is empty.
fn spell(key: &[u8], pl: &[u8]) -> Vec<u8> {
    let pl = cs(pl);
    if pl.is_empty() {
        return cs(key).to_vec();
    }
    if pl[0] == b'-' {
        let mut v = cs(key).to_vec();
        v.extend_from_slice(&pl[1..]);
        return v;
    }
    pl.to_vec()
}

/// `FUN_100364b0`: "N, N and N" after the numeral token `x`. Returns the
/// bytes consumed from `q` through the last numeral, or -1.
fn list_ext(fe: &Fe, x: &Tok, t: &[u8], q: usize, o: i32) -> i32 {
    if x.class != 2 || !numeric(&x.text) {
        return -1;
    }
    let mut pos = x.consumed;
    let mut y = Tok::default();
    let r = tk(fe, &mut y, t, q + pos as usize, o + pos);
    if r == 0 || first(&y) != b',' || y.len != 1 || y.ws >= 2 {
        return -1;
    }
    pos += r;
    let mut state = 1;
    while at(t, (q + pos as usize) as isize) != 0 {
        let r = tk(fe, &mut y, t, q + pos as usize, o + pos);
        if r == 0 {
            break;
        }
        if state != 0 && numeric(&y.text) && y.ws < 2 {
            if state == 2 {
                return pos + r;
            }
            state = 0;
        } else {
            if state != 0 {
                return -1;
            }
            if first(&y) == b',' && y.len == 1 && y.ws < 2 {
                state = 1;
            } else {
                if stricmp(&y.text, s(AND)) != 0 || y.ws > 1 {
                    return -1;
                }
                state = 2;
            }
        }
        pos += r;
    }
    -1
}

/// `FUN_100365f0`: "N & N", "N to N" or "N-N", "N/N", "N–N" (not followed by
/// a lone symbol). Returns the bytes consumed from `q`, or -1.
fn range_ext(fe: &Fe, x: &Tok, t: &[u8], q: usize, o: i32) -> i32 {
    if x.class != 2 || !numeric(&x.text) {
        return -1;
    }
    let mut pos = x.consumed;
    let mut y = Tok::default();
    let r = tk(fe, &mut y, t, q + pos as usize, o + pos);
    if r == 0 {
        return -1;
    }
    if ((first(&y) == b'&' && y.len == 1) || stricmp(&y.text, s(TO)) == 0) && y.ws < 2 {
        pos += r;
        let r = tk(fe, &mut y, t, q + pos as usize, o + pos);
        if r == 0 || !numeric(&y.text) || y.ws > 1 {
            return -1;
        }
        return pos + r;
    }
    if matches!(first(&y), b'-' | b'/' | 0x96 | 0x97) && y.len == 1 {
        pos += r;
        let r = tk(fe, &mut y, t, q + pos as usize, o + pos);
        if r != 0 && numeric(&y.text) && y.ws < 2 {
            pos += r;
            tk(fe, &mut y, t, q + pos as usize, o + pos);
            if y.class != 3 || y.len != 1 {
                return pos;
            }
        }
    }
    -1
}

/// `FUN_10036780`: "BN"/"B"/"T" right after a numeral slot is a scale
/// abbreviation, not a prefix.
fn prefix_ok(text: &[u8], tags: &[u8]) -> bool {
    let scale = [b"BN".as_slice(), b"B", b"T"].iter().any(|k| stricmp(text, k) == 0);
    !(scale && tags.last() == Some(&b'N'))
}

pub fn currency(fe: &Fe, wa: &mut Wa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let mut sl: Vec<Slot> = Vec::new();
    let mut tags: Vec<u8> = Vec::new();
    let mut prev_tag = 0u8;
    let mut seen = 0u32;
    let mut dash: i32 = -1;
    let (mut q, mut o) = (p, off);
    let mut ntok = 0i32;
    let mut x = Tok::default();
    while sl.len() < 6 {
        let consumed = if ntok < 1 {
            x = tok.clone();
            tok.consumed
        } else {
            let r = tk(fe, &mut x, t, q, o);
            if r == 0 || x.class == 9 {
                break;
            }
            r
        };
        let c0 = first(&x);
        if c0 == b'-' && x.len == 1 && ntok > 0 {
            if dash != -1 {
                break;
            }
            q += consumed as usize;
            o += consumed;
            dash = ntok;
            prev_tag = b'-';
            ntok += 1;
            continue;
        }
        let is_dot = x.ws == 0 && x.len < 3 && c0 == b'.';
        if is_dot && ntok > 0 && prev_tag == b'U' {
            q += consumed as usize;
            o += consumed;
            prev_tag = b'.';
            ntok += 1;
            continue;
        }
        let mut slot = Slot { idx: -1, ..Default::default() };
        let tag: u8;
        if !is_dot && (c0 == b'(' || c0 == b')') && x.len == 1 {
            tag = c0;
        } else {
            let sym = if seen & 1 == 0 && (x.class == 3 || x.class == 6) && x.len < 3 {
                SYMBOL_TAB.find_prefix(&x.text, true)
            } else {
                -1
            };
            let pre = if sym < 0 && seen & 2 == 0 && x.class == 1 && prefix_ok(&x.text, &tags) {
                if x.len == 1 {
                    PREFIX_TAB.find(&x.text, false)
                } else if x.len > 1 {
                    PREFIX_TAB.find(&x.text, true)
                } else {
                    -1
                }
            } else {
                -1
            };
            if sym >= 0 {
                tag = b'$';
                slot.idx = sym;
                seen |= 1;
            } else if pre >= 0 {
                tag = b'S';
                slot.disp = PREFIX_TAB.field(pre, 0x10).to_vec();
                seen |= 2;
            } else {
                let mut hit: Option<(u8, i32)> = None;
                if seen & 4 == 0 && seen & 8 == 0 && x.class == 1 {
                    let i = NAME_TAB.find_word(&x.text, true);
                    if i >= 0 {
                        hit = Some((b'F', i));
                        seen |= 4;
                    }
                }
                if hit.is_none() {
                    let try_c = if seen & 4 == 0 {
                        if seen & 8 == 0 {
                            let i = if x.class == 1 { NAME_TAB2.find_word(&x.text, false) } else { -1 };
                            if i >= 0 {
                                hit = Some((b'f', i));
                                seen |= 4;
                                false
                            } else {
                                true
                            }
                        } else {
                            false
                        }
                    } else {
                        true
                    };
                    if try_c && seen & 8 == 0 && x.class == 1 {
                        let i = CODE_TAB.find_exact(&x.text, true);
                        if i >= 0 {
                            hit = Some((b'C', i));
                            seen |= 8;
                        }
                    }
                }
                if let Some((h, i)) = hit {
                    tag = h;
                    slot.idx = i;
                } else {
                    let sc = if seen & 0x10 == 0 && x.class == 1 { SCALE_TAB.find(&x.text, true) } else { -1 };
                    if sc >= 0 {
                        if dash >= 0 {
                            if seen & 0x20 == 0 || dash + 1 != ntok || x.ws != 0 {
                                break;
                            }
                            dash = -1;
                        }
                        tag = b'U';
                        slot.disp = SCALE_TAB.field(sc, 0x10).to_vec();
                        seen |= 0x10;
                    } else if seen & 0x20 == 0 && x.class == 2 && numeric(&x.text) {
                        let mut ext = list_ext(fe, &x, t, q, o);
                        if ext < 0 {
                            ext = range_ext(fe, &x, t, q, o);
                        }
                        if ext >= 0 {
                            sl.push(Slot {
                                ws: x.ws,
                                start: o,
                                end: o + ext,
                                tag: b'N',
                                idx: -1,
                                tptr: q,
                                text: b"NUMS".to_vec(),
                                disp: Vec::new(),
                            });
                            tags.push(b'N');
                            prev_tag = b'N';
                            q += ext as usize;
                            o += ext;
                            seen |= 0x20;
                            ntok += 1;
                            continue;
                        }
                        tag = b'N';
                        seen |= 0x20;
                    } else {
                        break;
                    }
                }
            }
        }
        if dash >= 0 {
            break;
        }
        slot.ws = x.ws;
        slot.start = x.start;
        slot.end = o + consumed;
        slot.tag = tag;
        slot.tptr = q + consumed as usize;
        slot.text = cs(&x.text).to_vec();
        sl.push(slot);
        tags.push(if tag == b'f' { b'F' } else { tag });
        prev_tag = tag;
        q += consumed as usize;
        o += consumed;
        ntok += 1;
    }

    let group = match tags.first() {
        Some(b'$') => 1,
        Some(b'S') => 2,
        Some(b'C') => 3,
        Some(b'N') => {
            if tags.get(1) == Some(&b'U') {
                4
            } else {
                5
            }
        }
        _ => return -1,
    };
    let mut count = sl.len();
    while count >= 1 {
        let range = match (group, count) {
            (1, 4) => 0..2,
            (1, 3) => 2..5,
            (1, 2) => 5..6,
            (1, 1) => 6..7,
            (2, 4) => 7..8,
            (2, 3) => 8..9,
            (3, 3) => 9..10,
            (3, 2) => 10..11,
            (4, 6) => 11..13,
            (4, 5) => 13..14,
            (4, 4) => 14..16,
            (4, 3) => 16..18,
            (5, 5) => 18..20,
            (5, 4) => 20..21,
            (5, 3) => 21..22,
            (5, 2) => 22..PATTERNS.count(),
            _ => 0..0,
        };
        for i in range {
            if PATTERNS.get(i) != &tags[..count] {
                continue;
            }
            let end = sl[count - 1].end;
            let mut nx = Tok::default();
            if matches!(i, 7 | 8 | 0x15 | 0x16 | 0x17) {
                let used = end - off;
                let mut y = Tok::default();
                if tk(fe, &mut y, t, p + used as usize, off + used) != 0 {
                    nx = y;
                }
            }
            if validate(&sl[..count], &tags[..count], i, &nx) {
                return if emit(fe, wa, t, &sl[..count]) { end - off } else { 0 };
            }
            break;
        }
        count -= 1;
    }
    -1
}

/// `FUN_10036160`.
fn validate(sl: &[Slot], tags: &[u8], i: i32, nx: &Tok) -> bool {
    let find = |pat: &[u8]| tags.windows(2).position(|w| w == pat);
    if i != 6 {
        if let Some(pos) = find(b"$S").or_else(|| find(b"S$")) {
            if !tags.contains(&b'(') && sl[pos + 1].ws > 1 {
                return false;
            }
        }
    }
    let is_a = |k: usize| stricmp(&sl[k].text, s(0x1007d454)) == 0;
    let num_after = |ws1: i32| ws1 > 1 && nx.len > 0 && nx.class == 2 && nx.ws < 2;
    // paul09 FUN_10033650: only the "A" lead, lowercase and spacing checks.
    if era() == crate::era::Era::Paul09 {
        return match i {
            7 | 8 => !(is_a(0) && sl[1].ws > 0),
            10 => no_lowercase(&sl[0].text),
            0x17 => no_lowercase(&sl[1].text),
            _ => true,
        };
    }
    match i {
        0 | 0xe => !is_a(3),
        2 => !is_a(2),
        7 | 8 => !(is_a(0) && sl[1].ws > 0) && sl[1].ws <= 1,
        0x18 => sl[1].ws <= 1,
        10 => no_lowercase(&sl[0].text),
        0x15 | 0x16 => !(nx.len > 0 && nx.class != 3 && nx.ws == 0) && !num_after(sl[1].ws),
        0x17 => no_lowercase(&sl[1].text) && !num_after(sl[1].ws),
        _ => true,
    }
}

/// Last slot index per role, as `FUN_10034f30` collects them.
#[derive(Clone, Copy)]
struct Ids {
    n: i32,
    u: i32,
    d: i32,
    c: i32,
    s: i32,
    f: i32,
}

/// `FUN_10034f30`.
fn emit(fe: &Fe, wa: &mut Wa, t: &[u8], sl: &[Slot]) -> bool {
    let mut ids = Ids { n: -1, u: -1, d: -1, c: -1, s: -1, f: -1 };
    for (k, x) in sl.iter().enumerate() {
        let k = k as i32;
        match x.tag {
            b'N' => ids.n = k,
            b'U' => ids.u = k,
            b'$' => ids.d = k,
            b'C' => ids.c = k,
            b'S' => ids.s = k,
            b'F' | b'f' => ids.f = k,
            _ => {}
        }
    }
    let end = sl[sl.len() - 1].end;
    // julie09 FUN_100323c0 reads a list only without a scale word.
    if ids.n >= 0 && sl[ids.n as usize].text == b"NUMS" && (!fe.era.is_2009() || ids.u == -1) {
        return emit_list(fe, wa, t, sl, ids, sl[0].start + sl[0].ws, end);
    }
    emit_amount(fe, wa, sl, ids, sl[0].start, end)
}

/// The subunit ("cent"/"cents") of `name` for a two-digit fraction `frac`.
fn subunit(name: &[u8], frac: i32) -> Vec<u8> {
    let mut i = NAME_TAB.find_exact(name, true);
    let tab = if i >= 0 {
        &NAME_TAB
    } else {
        i = NAME_TAB2.find_exact(name, false);
        if i < 0 {
            return Vec::new();
        }
        &NAME_TAB2
    };
    if frac < 2 {
        tab.field(i, 0x32).to_vec()
    } else {
        spell(tab.field(i, 0x32), tab.field(i, 0x41))
    }
}

/// Reading of one numeral: `(whole, frac, frac_len, plural)`.
struct Amount {
    whole: Vec<u8>,
    frac: Vec<u8>,
    w: i32,
    f: i32,
    fl: usize,
}

fn amount(text: &[u8]) -> Amount {
    let (whole, frac) = split_decimal(text);
    let w = atoi(&whole);
    let f = atoi(&frac);
    let fl = frac.len();
    Amount { whole, frac, w, f, fl }
}

fn say(src: &[u8]) -> Vec<u8> {
    let mut buf = new_buf();
    number(&mut buf, src, 1);
    cs(&buf).to_vec()
}

/// The names, qualifiers and subunit reading that follow the number in both
/// emitters (`LAB_10035d4b` onwards). `w` is the amount marker (0: none
/// spoken, 1: exactly one, 2: plural), `plural` the sticky plural flag.
#[allow(clippy::too_many_arguments)]
fn tail(wa: &mut Wa, sl: &[Slot], ids: Ids, st: i32, en: i32, w: i32, plural: bool, dollar_without_n: bool, a: Option<&Amount>, sub: &[u8]) -> bool {
    let sing = (w < 2 || era() == crate::era::Era::Paul09) && !plural;
    if ids.c >= 0 && ids.d == -1 && ids.s == -1 && ids.f == -1 && w != 0 {
        let i = sl[ids.c as usize].idx;
        let name = if sing { CODE_TAB.field(i, 0xf).to_vec() } else { spell(CODE_TAB.field(i, 0xf), CODE_TAB.field(i, 0x32)) };
        if !wa.emit_split(CODE_TAB.field(i, 0x41), st, en, 0x41, 0x44, 3) || !wa.emit_split(&name, st, en, 0x41, 0x44, 3) {
            return false;
        }
    }
    if w != 0 && ids.s >= 0 && ids.d >= 0 && ids.c == -1 && ids.f == -1 && !wa.emit_split(&sl[ids.s as usize].disp, st, en, 0x41, 0x44, 3) {
        return false;
    }
    if ids.d >= 0 && ids.c == -1 && ids.f == -1 && (w != 0 || dollar_without_n) {
        let i = sl[ids.d as usize].idx;
        let name = if sing { SYMBOL_TAB.field(i, 0x1e).to_vec() } else { spell(SYMBOL_TAB.field(i, 0x1e), SYMBOL_TAB.field(i, 0x3c)) };
        if !wa.emit(&name, st, en, 0x53, 0x44, 3) {
            return false;
        }
    }
    if ids.f >= 0 && ids.d == -1 && ids.c == -1 && ids.s == -1 && w != 0 && !wa.emit_split(&sl[ids.f as usize].text, st, en, 0x41, 0x44, 3) {
        return false;
    }
    if let Some(a) = a {
        if a.f != 0 && a.fl == 2 && !sub.is_empty() && ids.u == -1 {
            if w != 0 && !wa.emit(s(AND), st, en, 0x41, 0x44, 3) {
                return false;
            }
            if !wa.emit_split(&say(&a.frac), st, en, 0x44, 0x44, 3) || !wa.emit_split(sub, st, en, 0x41, 0x44, 3) {
                return false;
            }
        }
    }
    true
}

/// The currency name the subunit is looked up by.
fn base_name(sl: &[Slot], ids: Ids, with_f: bool) -> Vec<u8> {
    if ids.d >= 0 {
        SYMBOL_TAB.field(sl[ids.d as usize].idx, 0x1e).to_vec()
    } else if ids.c >= 0 {
        CODE_TAB.field(sl[ids.c as usize].idx, 0xf).to_vec()
    } else if with_f && ids.f >= 0 {
        let x = &sl[ids.f as usize];
        if x.tag == b'F' {
            NAME_TAB.field(x.idx, 0).to_vec()
        } else {
            x.text.clone()
        }
    } else {
        Vec::new()
    }
}

/// `FUN_10035950`: one amount.
fn emit_amount(fe: &Fe, wa: &mut Wa, sl: &[Slot], ids: Ids, st: i32, en: i32) -> bool {
    let mut w = 0;
    let mut plural = false;
    let mut am: Option<Amount> = None;
    let mut sub: Vec<u8> = Vec::new();
    if ids.n >= 0 {
        let text = sl[ids.n as usize].text.clone();
        let words: Option<Vec<u8>>;
        let a_tag: u8;
        if ids.u == -1 {
            let a = amount(&text);
            w = a.w;
            // paul09 FUN_10032e80: plural above one, or exactly one with a fraction; the full text is read unless cents follow.
            let p09 = fe.era == crate::era::Era::Paul09;
            if p09 {
                if a.w > 1 || (a.w == 1 && a.f != 0) {
                    plural = true;
                }
            } else if a.w != 1 && a.f != 0 {
                plural = true;
            }
            // julie09 FUN_10032da0 takes the subunit only from a symbol or ISO code.
            let name = base_name(sl, ids, !fe.era.is_2009());
            if a.f != 0 && a.fl == 2 && !name.is_empty() {
                sub = subunit(&name, a.f);
            }
            let cents = a.fl == 2 && !sub.is_empty();
            words = if a.w == 0 {
                if cents {
                    None
                } else {
                    w = if p09 { 1 } else { 2 };
                    Some(say(if a.f == 0 && !p09 { &a.whole } else { &text }))
                }
            } else {
                Some(say(if cents || (a.f == 0 && !p09) { &a.whole } else { &text }))
            };
            a_tag = 0x44;
            am = Some(a);
        } else {
            let p09 = fe.era == crate::era::Era::Paul09;
            w = if p09 { 1 } else { 2 };
            if !wa.emit_split(&say(&text), st, en, 0x44, 0x44, 3) {
                return false;
            }
            plural = true;
            words = Some(sl[ids.u as usize].disp.clone());
            a_tag = 0x41;
        }
        if let Some(words) = words {
            if !wa.emit_split(&words, st, en, a_tag, 0x44, 3) {
                return false;
            }
        }
    }
    tail(wa, sl, ids, st, en, w, plural, ids.n == -1, am.as_ref(), &sub)
}

/// `FUN_10035030`: a list or range ("NUMS" slot), re-tokenized and read
/// token by token. Names follow a numeral only once an unhandled symbol has
/// been seen, or when it has a two-digit subunit fraction; the plural flag
/// and the subunit word carry over between numerals.
fn emit_list(fe: &Fe, wa: &mut Wa, t: &[u8], sl: &[Slot], ids: Ids, st: i32, en: i32) -> bool {
    let ns = &sl[ids.n as usize];
    let len = ns.end - ns.start;
    let (mut q, mut o) = (ns.tptr, ns.start);
    let mut done = 0;
    // julie09 FUN_100324d0 names every numeral (2014 only after an unhandled token).
    let mut other = fe.era.is_2009();
    let mut plural = false;
    let mut sub: Vec<u8> = Vec::new();
    let mut x = Tok::default();
    while done < len {
        let r = tk(fe, &mut x, t, q, o);
        if r == 0 {
            return false;
        }
        match x.class {
            2 => {
                if ids.u == -1 {
                    let a = amount(&x.text);
                    let mut w = a.w;
                    // paul09 FUN_100325d0: as FUN_10032e80, the whole text is read unless cents follow.
                    let p09 = fe.era == crate::era::Era::Paul09;
                    if p09 {
                        if a.w > 1 || (a.w == 1 && a.f != 0) {
                            plural = true;
                        }
                    } else if a.w != 1 && a.f != 0 {
                        plural = true;
                    }
                    let whole_only = a.f == 0 && !p09;
                    let name = base_name(sl, ids, false);
                    if a.f != 0 && a.fl == 2 && !name.is_empty() {
                        let sb = subunit(&name, a.f);
                        if !sb.is_empty() {
                            sub = sb;
                        }
                    }
                    let cents = a.fl == 2 && !sub.is_empty();
                    let names = if a.w == 0 {
                        if !cents {
                            let src = if whole_only { a.whole.clone() } else { cs(&x.text).to_vec() };
                            w = if p09 { 1 } else { 2 };
                            if !wa.emit_split(&say(&src), st, en, 0x44, 0x44, 3) {
                                return false;
                            }
                        }
                        other
                    } else if !cents {
                        let src = if whole_only { a.whole.clone() } else { cs(&x.text).to_vec() };
                        if !wa.emit_split(&say(&src), st, en, 0x44, 0x44, 3) {
                            return false;
                        }
                        other
                    } else {
                        if !wa.emit_split(&say(&a.whole), st, en, 0x44, 0x44, 3) {
                            return false;
                        }
                        true
                    };
                    if names && !tail(wa, sl, ids, st, en, w, plural, false, Some(&a), &sub) {
                        return false;
                    }
                } else {
                    if super::numwords::plural(&x.text) {
                        plural = true;
                    }
                    if !wa.emit_split(&say(&x.text), st, en, 0x44, 0x44, 3) {
                        return false;
                    }
                    if other {
                        plural = true;
                        if !wa.emit_split(&sl[ids.u as usize].disp, st, en, 0x41, 0x44, 3) {
                            return false;
                        }
                        if !tail(wa, sl, ids, st, en, if fe.era == crate::era::Era::Paul09 { 1 } else { 2 }, plural, false, None, &sub) {
                            return false;
                        }
                    }
                }
            }
            1 | 3 | 6 => {
                let word: Option<u32> = match (x.class, first(&x)) {
                    (1, _) => None,
                    (3, b'-') | (6, 0x96) | (6, 0x97) => Some(TO),
                    (3, b'/') | (3, b'&') => Some(AND),
                    _ => Some(0),
                };
                let ok = match word {
                    None => wa.emit(&x.text, st, en, 0x41, 0x44, 3),
                    Some(0) => true,
                    Some(va) => wa.emit(s(va), st, en, 0x41, 0x44, 3),
                };
                if !ok {
                    return false;
                }
                // Every word or symbol token, read or not, lets later
                // numerals take their names (`LAB_1003590a`).
                other = true;
            }
            _ => {}
        }
        done += r;
        o += r;
        q += r as usize;
    }
    true
}
