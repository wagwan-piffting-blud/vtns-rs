//! `FUN_1003e4a0`: units of measure. `FUN_10040250` finds the number the
//! units belong to (at the position, or already emitted just before); then up
//! to five unit slots (`E` unit dictionary, `P`/`C` prefixed or context units,
//! `/` and `O` operators, `W` cubic/square prefixes, `s` superscript powers)
//! are collected in the WA+0x39e4 scratch, matched against the six patterns
//! at 0x10080508, validated by `FUN_1003f300` and emitted by `FUN_1003ef80`
//! (class 0xf).

use super::cstr::*;
use super::numwords::*;
use super::phrases::{phrase_find, words_before};
use super::sbd;
use super::tabs::{ptab, ptabn, rec30, rec5a, Rec5a};
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::window::{back_words, last_two};
use super::Fe;

const NUMS: u32 = 0x1008019c;

#[derive(Clone)]
struct Slot {
    ws: i32,
    start: i32,
    end: i32,
    len: i32,
    class: u8,
    tag: u8,
    /// +0x14: 10.. cubic/square prefix, 1..9 power suffix.
    pow: i16,
    /// +0x16: a dot followed the unit.
    dot: i16,
    /// +0x18: plural.
    plural: i16,
    tp: usize,
    text: [u8; 40],
    exp: [u8; 0x102],
}

impl Default for Slot {
    fn default() -> Self {
        Slot { ws: 0, start: 0, end: 0, len: 0, class: 0, tag: 0, pow: 0, dot: 0, plural: 0, tp: 0, text: [0; 40], exp: [0; 0x102] }
    }
}

/// `local_694`: slot 0 the number, slot 1 a currency symbol before it,
/// slot 3 a dash glued after it.
struct Nums {
    sl: [Slot; 4],
}

fn tk(fe: &Fe, t: &[u8], p: usize, off: i32, mode: i32) -> (Tok, i32) {
    let mut x = Tok::default();
    let r = tokenize(&mut x, t, p, off, mode, fe.ud.as_ref());
    (x, r)
}

fn from_tok(sl: &mut Slot, x: &Tok, tag: u8) {
    sl.ws = x.ws;
    sl.start = x.start;
    sl.end = x.end;
    sl.len = x.len;
    sl.class = x.class as u8;
    sl.tag = tag;
    strcpy(&mut sl.text, &x.text);
}

fn is_dash(c: u8) -> bool {
    matches!(c, b'-' | b'~' | 0x96 | 0x97)
}

/// `FUN_10040750`: "1, 2 and 3".
fn num_list(fe: &Fe, x: &Tok, t: &[u8], p: usize, off: i32) -> i32 {
    if x.class != 2 || !numeric(&x.text) {
        return -1;
    }
    let mut u = x.consumed;
    let (y, r) = tk(fe, t, p + u as usize, off + u, 0xb);
    if r == 0 || y.text[0] != b',' || y.len != 1 || y.ws >= 2 {
        return -1;
    }
    let mut st = 1;
    u += r;
    if at(t, (p + u as usize) as isize) == 0 {
        return -1;
    }
    loop {
        let (y, r) = tk(fe, t, p + u as usize, off + u, 0xb);
        if r == 0 {
            return -1;
        }
        if st != 0 && numeric(&y.text) && y.ws <= 1 {
            if st == 2 {
                return u + r;
            }
            st = 0;
        } else if ((y.text[0] == b'&' && y.len == 1) || stricmp(&y.text, s(0x10077400)) == 0) && y.ws < 2 {
            st = 2;
        } else {
            if st != 0 || y.text[0] != b',' {
                return -1;
            }
            st = 1;
            if y.len != 1 || y.ws > 1 {
                return -1;
            }
        }
        u += r;
        if at(t, (p + u as usize) as isize) == 0 {
            return -1;
        }
    }
}

/// `FUN_100408a0`: "5 or 6", "5-10" (a dash range must be followed by a word, `%` or `°`).
fn num_range(fe: &Fe, x: &Tok, t: &[u8], p: usize, off: i32) -> i32 {
    if x.class != 2 || !numeric(&x.text) {
        return -1;
    }
    let mut u = x.consumed;
    let (y1, r1) = tk(fe, t, p + u as usize, off + u, 0xb);
    if r1 == 0 {
        return -1;
    }
    let joiner = if stricmp(&y1.text, s(0x1007901c)) == 0 || stricmp(&y1.text, s(0x10077400)) == 0 {
        true
    } else if y1.text[0] == b'&' {
        if y1.len != 1 {
            return -1;
        }
        true
    } else {
        false
    };
    if joiner && y1.ws < 2 {
        u += r1;
        let (y2, r2) = tk(fe, t, p + u as usize, off + u, 0xb);
        if r2 == 0 || !numeric(&y2.text) || y2.ws > 1 {
            return -1;
        }
        return u + r2;
    }
    if is_dash(y1.text[0]) && y1.len == 1 && y1.ws >= 0 && y1.ws < 2 {
        u += r1;
        let (y2, r2) = tk(fe, t, p + u as usize, off + u, 0xb);
        if r2 == 0 || !numeric(&y2.text) || y2.ws < 0 || y2.ws > 1 || y1.ws != y2.ws {
            return -1;
        }
        u += r2;
        let (z, _) = tk(fe, t, p + u as usize, off + u, 0xb);
        let ok = match z.class {
            6 => {
                if z.len != 1 {
                    return -1;
                }
                z.text[0] == 0xb0
            }
            3 => {
                if z.len != 1 {
                    return -1;
                }
                z.text[0] == b'%'
            }
            1 => return u,
            _ => return -1,
        };
        if ok {
            return u;
        }
    }
    -1
}

/// `FUN_10040250`: the number the units attach to. >0: a number at the
/// position (bytes consumed); -1: the number (or "a"/"an", or a currency
/// symbol) is the word before; 0: none.
fn nums_parse(fe: &Fe, nm: &mut Nums, wa: &Wa, x: &Tok, t: &[u8], p: usize, off: i32) -> i32 {
    if x.class == 2 && well_formed(&x.text) {
        let mut r = num_list(fe, x, t, p, off);
        if r < 0 {
            r = num_range(fe, x, t, p, off);
        }
        if r >= 0 {
            let sl = &mut nm.sl[0];
            sl.start = off;
            sl.ws = x.ws;
            sl.end = off + r;
            sl.len = r - x.ws;
            sl.class = x.class as u8;
            sl.tag = b'N';
            strcpy(&mut sl.text, s(NUMS));
            sl.tp = p;
            sl.plural = 1;
            return r;
        }
        let c = x.consumed;
        let (y, r) = tk(fe, t, p + c as usize, off + c, 0);
        if r == 0 || y.class == 9 {
            return 0;
        }
        let mut ret = c;
        let mut dash = false;
        if (y.class == 3 || y.class == 7) && y.len == 1 && y.text[0] == b'-' && y.ws < 2 {
            let sl = &mut nm.sl[3];
            sl.ws = y.ws;
            sl.end = y.end;
            sl.len = 1;
            sl.class = y.class as u8;
            sl.tag = b'N';
            sl.start = y.start;
            strcpy(&mut sl.text, &y.text);
            dash = true;
            ret = c + r;
        }
        from_tok(&mut nm.sl[0], x, b'N');
        if !dash && plural(&x.text) {
            nm.sl[0].plural = 1;
        }
        return ret;
    }
    let mut buf = [0u8; 96];
    let r = back_words(&mut buf, t, p as isize - off as isize, p as isize, 2);
    if r as i16 == 0 {
        return 0;
    }
    let (a, b) = last_two(&buf, off - r, 0).unwrap_or_default();
    let n = wa.n() as i32;
    let prev = |o: usize| if n >= 1 { wa.i32(Wa::rec(n - 1) + o) } else { i32::MIN };
    let prev_plural = || n >= 1 && wa.i16(Wa::rec(n - 1) + 0x1e) != 0;
    let numeric_rec = |c: i32| c == 0x1d || c == 0xd;
    let mut try_b = a.len < 1;
    if !try_b {
        if b.len < 1 {
            return 0;
        }
        if n < 1 || !numeric_rec(prev(0x28)) || prev(4) != a.end || (b.class != 3 && b.class != 7) || b.len != 1 {
            try_b = true;
        } else if b.text[0] == b'-' && b.ws <= 1 && off - b.end <= 1 {
            from_tok(&mut nm.sl[0], &a, b'N');
            nm.sl[0].class = 1;
            if prev_plural() {
                nm.sl[0].plural = 1;
            }
            return -1;
        }
        if try_b {
            if b.len < 1 {
                return 0;
            }
        }
    } else if b.len < 1 {
        return 0;
    }
    if b.class == 1 && (stricmp(&b.text, s(0x10077718)) == 0 || stricmp(&b.text, s(0x10077370)) == 0) && prev(0) == b.start && prev(4) == b.end {
        from_tok(&mut nm.sl[0], &b, b'N');
        return -1;
    }
    if b.len < 1 || n < 1 {
        return 0;
    }
    if numeric_rec(prev(0x28)) && prev(4) == b.end && b.ws < 2 && off - b.end < 2 {
        from_tok(&mut nm.sl[0], &b, b'N');
        nm.sl[0].class = 1;
        if prev_plural() {
            nm.sl[0].plural = 1;
        }
        return -1;
    }
    if prev(0x28) != 3 || prev(4) != b.end || off - b.end > 2 {
        return 0;
    }
    from_tok(&mut nm.sl[1], &b, b'$');
    -1
}

/// `FUN_10040d60`: singular or plural spelling of a unit record.
fn spell(out: &mut [u8], plural: bool, key: &[u8], alt: &[u8], pl: &[u8]) {
    out[0] = 0;
    let base = if cs(alt).is_empty() { key } else { alt };
    if plural && at(pl, 0) != 0 {
        if pl[0] == b'-' {
            strcpy(out, base);
            strcat(out, &pl[1..]);
        } else {
            strcpy(out, pl);
        }
        return;
    }
    strcpy(out, base);
}

fn spell_rec(out: &mut [u8], plural: bool, tab: Rec5a, i: i32) {
    spell(out, plural, tab.field(i, 0), tab.field(i, 0x1e), tab.field(i, 0x3c));
}

/// The unit dictionaries built by `FUN_10039fe0` (`DAT_100a7a5c/60/64`):
/// (table, count variable, case-insensitive prefix match).
pub(crate) const DICT_E: [(Rec5a, bool); 3] = [(rec5a(0x1008ab88, 0x1009b09c), true), (rec5a(0x1008cc40, 0x1009b0a0), true), (rec5a(0x1008d2f0, 0x1009b0a4), false)];
pub(crate) const DICT_P: [(Rec5a, bool); 2] = [(rec5a(0x1008de30, 0x1009b0a8), true), (rec5a(0x1008f8e8, 0x1009b0ac), false)];
pub(crate) const DICT_C: [(Rec5a, bool); 2] = [(rec5a(0x1008fab0, 0x1009b0b0), true), (rec5a(0x100900b0, 0x1009b0b4), false)];

/// `FUN_10040ab0`.
pub(crate) fn dict_find(out: &mut [u8], dict: &[(Rec5a, bool)], w: &[u8], plural: bool) -> bool {
    out[0] = 0;
    for &(tab, ins) in dict.iter() {
        let i = if ins { tab.find_prefix(w, true) } else { tab.find_exact(w) };
        if i >= 0 {
            spell_rec(out, plural, tab, i);
            return true;
        }
    }
    false
}

/// `FUN_10063050`: capitalize the first letter and each word start, lowercase the rest.
fn title_case(w: &mut [u8]) {
    if w[0] == 0 {
        return;
    }
    let mut i = 0usize;
    loop {
        if i == 0 {
            w[0] = upper(w[0]) as u8;
        } else {
            let c = w[i];
            if c == b' ' && w[i + 1] != 0 && cls(w[i + 1]) & 0x40 != 0 {
                w[i + 1] = upper(w[i + 1]) as u8;
                i += 1;
            } else if cls(c) & 0x80 != 0 {
                w[i] = lower(c) as u8;
            }
        }
        i += 1;
        if w[i] == 0 {
            break;
        }
    }
}

/// `FUN_10056ba0`: multi-word unit phrase starting with `w` (the rest of
/// the phrase is matched against the following tokens). Updates `*used`
/// (bytes consumed from `p`) and `plural`.
#[allow(clippy::too_many_arguments)]
fn phrase_unit(fe: &Fe, tab: Rec5a, w: &[u8], t: &[u8], p: usize, off: i32, used: &mut i32, plural: &mut bool) -> i32 {
    let w = cs(w);
    if w.is_empty() {
        return -1;
    }
    let wl = w.len();
    let mut lo = 0;
    let mut hi = tab.count() - 1;
    if hi < 0 {
        return -1;
    }
    let mut mid = hi;
    loop {
        mid /= 2;
        let key = tab.field(mid, 0);
        let c = strnicmp(key, w, wl);
        let mut go_low = c >= 0;
        if c == 0 {
            let nc = at(key, wl as isize);
            if nc == 0 {
                return mid;
            }
            let glued = wl == 1 && w[0] == 0xb0 && at(key, 1) == b' ' && cls(at(t, (p + *used as usize) as isize)) & 0xc0 != 0;
            let spaced = nc == b' ' && matches!(at(t, (p + *used as usize) as isize), b' ' | b'-');
            go_low = true;
            if glued || spaced {
                let rest_all = &key[wl + 1..];
                let mut rest = 0usize;
                let mut q = *used + if glued { 0 } else { 1 };
                let pl = tab.field(mid, 0x3c);
                let pls = if at(pl, 0) == b'-' { strlen(&pl[1..]) as i32 } else { 0 };
                if at(t, (p + q as usize) as isize) != 0 {
                    loop {
                        let (x, r) = tk(fe, t, p + q as usize, off + q, 0);
                        if r == 0 || x.class == 9 || x.len == 0 {
                            break;
                        }
                        let rs = &rest_all[rest..];
                        if pls > 0 && x.len - pls > 0 && strnicmp(rs, &x.text, (x.len - pls) as usize) == 0 && stricmp(&x.text[(x.len - pls) as usize..], &pl[1..]) == 0 {
                            *used = q + r;
                            *plural = true;
                            return mid;
                        }
                        let c2 = strnicmp(rs, &x.text, x.len as usize);
                        if c2 != 0 {
                            go_low = c2 >= 0;
                            break;
                        }
                        let nc2 = at(rs, x.len as isize);
                        if nc2 == 0 {
                            *used = q + r;
                            return mid;
                        }
                        if nc2 != b' ' || at(t, (p + (q + r) as usize) as isize) != b' ' {
                            break;
                        }
                        q += r + 1;
                        rest += x.len as usize + 1;
                        if at(t, (p + q as usize) as isize) == 0 {
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

/// paul09 `FUN_1003d680`: the word, then up to three more words joined with `-` (a glued `-` is skipped),
/// looked up whole in the unit-name table; the result is the typed phrase with its hyphens read as blanks.
/// A miss on the first word is retried without its last letter when that is the record's plural suffix.
#[allow(clippy::too_many_arguments)]
fn phrase_lookup_paul(fe: &Fe, out: &mut [u8], tab: Rec5a, w: &[u8], t: &[u8], p: usize, off: i32, used: &mut i32) -> bool {
    let mut key: Vec<u8> = Vec::new();
    let mut res: Vec<u8> = Vec::new();
    let mut found = false;
    let mut total = strlen(w) as i32;
    let mut count = 0;
    let mut c6 = 0i32;
    while count < 4 {
        let adv;
        if c6 < 1 {
            key = cs(w).to_vec();
            adv = *used;
        } else {
            let (x, r) = tk(fe, t, p + c6 as usize, off + c6, 0);
            if r == 0 || x.class == 9 {
                break;
            }
            total += 1 + x.len;
            if total > 0x1e {
                break;
            }
            if (x.class == 3 || x.class == 7) && x.len == 1 && x.text[0] == b'-' && x.ws == 0 {
                c6 += r;
                continue;
            }
            if x.class == 1 && x.ws < 2 {
                if !key.is_empty() {
                    key.push(b'-');
                }
                key.extend_from_slice(x.text());
                adv = r;
            } else {
                break;
            }
        }
        c6 += adv;
        let i = tab.find_prefix(&key, true);
        if i < 0 {
            if !found && key.len() > 2 {
                let stem = &key[..key.len() - 1];
                let j = tab.find_prefix(stem, true);
                if j >= 0 && at(tab.field(j, 0x3c), 0) == b'-' && at(tab.field(j, 0x3c), 1) == key[key.len() - 1] {
                    for c in key.iter_mut() {
                        if *c == b'-' {
                            *c = b' ';
                        }
                    }
                    res = key.clone();
                    if count > 0 {
                        *used = c6;
                    }
                    found = true;
                }
            }
        } else {
            for c in key.iter_mut() {
                if *c == b'-' {
                    *c = b' ';
                }
            }
            res = key.clone();
            if count > 0 {
                *used = c6;
            }
            found = true;
        }
        count += 1;
    }
    if !found {
        return false;
    }
    strcpy(out, &res);
    true
}

/// `FUN_10040b60`: the multi-word unit table (also `word` minus its last
/// letter when that letter is the record's plural suffix).
#[allow(clippy::too_many_arguments)]
pub(crate) fn phrase_lookup(fe: &Fe, out: &mut [u8], w: &[u8], t: &[u8], p: usize, off: i32, used: &mut i32) -> bool {
    out[0] = 0;
    let tab = rec5a(0x10090490, 0x1009b0b8);
    if era() == crate::era::Era::Paul09 {
        return phrase_lookup_paul(fe, out, tab, w, t, p, off, used);
    }
    let mut pl = false;
    let i = phrase_unit(fe, tab, w, t, p, off, used, &mut pl);
    if i >= 0 {
        spell_rec(out, pl, tab, i);
        if cls(w[0]) & 0x80 != 0 {
            title_case(out);
        }
        return true;
    }
    let l = strlen(w);
    if l < 3 {
        return false;
    }
    let cnt = l - 1;
    let mut stem = [0u8; 32];
    strncpy(&mut stem, w, cnt);
    stem[cnt] = 0;
    let j = phrase_unit(fe, tab, &stem, t, p, off, used, &mut pl);
    if j < 0 {
        return false;
    }
    let plr = tab.field(j, 0x3c);
    if at(plr, 0) == b'-' && at(plr, 1) == w[cnt] {
        spell_rec(out, true, tab, j);
    } else {
        strcpy(out, w);
    }
    if cls(w[0]) & 0x80 != 0 {
        title_case(out);
    }
    true
}

fn opt_words(v: Vec<Vec<u8>>) -> Vec<Option<Vec<u8>>> {
    let mut out: Vec<Option<Vec<u8>>> = vec![None; 5];
    for (i, w) in v.into_iter().take(5).enumerate() {
        if w.is_empty() {
            break;
        }
        out[i] = Some(w);
    }
    out
}

/// `FUN_1003fbf0`: context check for one- and two-letter unit symbols
/// ("W" near "bulb", "M" near "length", "C" near "temperature", ...), with
/// the spelling written to `out` when given.
#[allow(clippy::too_many_arguments)]
fn context_unit(fe: &Fe, out: Option<&mut [u8]>, plural: bool, w: &[u8], t: &[u8], p0: usize, off0: i32, u: i32) -> bool {
    let wl = strlen(w);
    let before = opt_words(words_before(t, p0, off0, 0xb));
    let mut after: Vec<Option<Vec<u8>>> = vec![None; 5];
    let mut dc = Tok::default();
    let mut q = u;
    for i in 0..5 {
        let (x, r) = tk(fe, t, p0 + q as usize, off0 + q, 0xb);
        if r == 0 {
            break;
        }
        if i == 0 {
            dc = x.clone();
        }
        if x.class == 8 || x.class == 9 {
            break;
        }
        after[i] = Some(x.text().to_vec());
        q += r;
    }
    let near = |tab: super::tabs::Ptab| {
        phrase_find(tab, &before, true) >= 0 || phrase_find(tab, &after, true) >= 0
    };
    let tail = |dc: &Tok| dc.len == 1 && dc.ws < 2 && dc.text[0] != 0 && strchr(s(0x1007d470), dc.text[0]);
    if wl != 1 {
        if wl == 2 {
            if stricmp(w, s(0x10077688)) == 0 && near(ptab(0x100959a8, 0x1009b0c0)) {
                return true;
            }
            if (strcmp(w, s(0x100805d0)) == 0 || strcmp(w, s(0x10080558)) == 0) && near(ptab(0x100959c8, 0x1009b0c8)) {
                return true;
            }
        }
        return tail(&dc);
    }
    let c = upper(w[0]);
    let word: Option<u32> = if c == 0x57 && near(ptab(0x100959a4, 0x1009b0bc)) {
        Some(0x1008060c)
    } else if c == 0x4d && near(ptab(0x100959a8, 0x1009b0c0)) {
        Some(0x10080604)
    } else if c == 0x4e && near(ptab(0x100959b8, 0x1009b0c4)) {
        Some(0x100805fc)
    } else if matches!(c, 0x43 | 0x46 | 0x4b | 0x52) && near(ptab(0x100959c8, 0x1009b0c8)) {
        Some(match c {
            0x4b => 0x100805f4,
            0x52 => 0x100805ec,
            0x43 => 0x10080564,
            _ => 0x100805e0,
        })
    } else if c == 0x53 && near(ptab(0x1008a9f8, 0x1009b048)) {
        Some(0x10078918)
    } else if (c == 0x47 || c == 0x54) && near(ptab(0x100959ec, 0x1009b0cc)) {
        Some(if c == 0x47 { 0x100805d8 } else { 0x100805d4 })
    } else {
        None
    };
    match word {
        Some(va) => {
            if let Some(o) = out {
                spell(o, plural, s(0x1009f948), s(va), s(0x10080614));
            }
            true
        }
        None => tail(&dc),
    }
}

fn is_a_an(w: &[u8]) -> bool {
    stricmp(w, s(0x10077718)) == 0 || stricmp(w, s(0x10077370)) == 0
}

/// `FUN_1003f300`: validate pattern `idx` ("$/A", "A", "AA", "A/A", "AOA", "AOA/A").
fn validate(fe: &Fe, t: &[u8], sl: &mut [Slot], nm: &Nums, idx: usize, f2c: bool) -> bool {
    let n0 = &nm.sl[0];
    if idx > 0 && n0.len > 0 {
        let v = sl[0].ws;
        if n0.class == 1 {
            if v > 2 {
                return false;
            }
        } else {
            if v > 1 {
                return false;
            }
            if nm.sl[3].len > 0 && nm.sl[3].ws != v {
                return false;
            }
        }
    }
    let dot_tok = |sl: &Slot| tk(fe, t, sl.tp, 0, 0xb).0;
    let not_abbr_end = |x: &Tok| x.ws != 0 || x.len > 2 || x.text[0] != b'.' || sbd::decide(fe, x, t, sl[0].tp, 0) == b'P';
    let short_word = |w: &[u8]| stricmp(w, s(0x10077688)) == 0 || stricmp(w, s(0x100805c8)) == 0 || stricmp(w, s(0x100805c4)) == 0 || stricmp(w, s(0x100805c0)) == 0;
    match idx {
        1 => {
            let c = sl[0].tag;
            if c == b'P' || c == b'A' {
                let s3 = sl[0].pow;
                if s3 < 1 {
                    if n0.len < 1 {
                        return false;
                    }
                    let w = sl[0].text;
                    if is_a_an(&n0.text) {
                        if stricmp(&w, s(0x100805bc)) == 0
                            || stricmp(&w, s(0x100805b8)) == 0
                            || strcmp(&w, s(0x100805b4)) == 0
                            || strcmp(&w, s(0x100805b0)) == 0
                            || strcmp(&w, s(0x100805ac)) == 0
                            || (stricmp(&w, s(0x100805a8)) == 0 && at(t, sl[0].tp as isize) == b'3')
                        {
                            return false;
                        }
                    }
                    if short_word(&w) {
                        let x = dot_tok(&sl[0]);
                        if !f2c
                            && not_abbr_end(&x)
                            && (nm.sl[3].len < 1 || sl[0].ws != 0)
                            && (sl[0].dot == 0 || x.ws != 0 || x.len > 2 || x.text[0] != b'.' || x.len < 2)
                        {
                            return false;
                        }
                    }
                } else if s3 == 0xb || s3 == 0xd {
                    if short_word(&sl[0].text) {
                        let x = dot_tok(&sl[0]);
                        if !f2c && not_abbr_end(&x) && n0.len < 1 {
                            if x.ws != 0 || x.len > 2 || x.text[0] != b'.' {
                                return false;
                            }
                            if x.len < 2 {
                                return false;
                            }
                        }
                    }
                }
            } else if c == b'C' && !f2c && sl[0].pow < 1 {
                if n0.len < 1 || stricmp(&sl[0].text, s(0x1007d380)) != 0 {
                    return false;
                }
                if strcmp(&n0.text, s(0x100805cc)) == 0 {
                    return false;
                }
            }
            true
        }
        2 => {
            let (c0, c1) = (sl[0].tag, sl[1].tag);
            if c0 == b'E' && c1 == b'C' {
                let w = sl[0].text;
                if stricmp(&w, s(0x100805a4)) == 0 || stricmp(&w, s(0x1008059c)) == 0 || stricmp(&w, s(0x10080594)) == 0 {
                    let v = sl[1].len;
                    if v == 1 {
                        match upper(sl[1].text[0]) {
                            0x43 => {
                                strcpy(&mut sl[1].exp, s(0x10080564));
                                return true;
                            }
                            0x46 => return true,
                            0x52 => {
                                strcpy(&mut sl[1].exp, s(0x1008055c));
                                return true;
                            }
                            _ => {}
                        }
                    }
                    return v == 2 && stricmp(&sl[1].text, s(0x10080558)) == 0;
                }
                if stricmp(&w, s(0x10080590)) == 0 && sl[1].len == 1 {
                    let u = upper(sl[1].text[0]);
                    if u != 0 && strchr(s(0x10080584), u as u8) {
                        strcpy(&mut sl[0].exp, s(0x1008057c));
                        let c = sl[1].text[0];
                        if c != b'a' && c != b'f' {
                            if c == b'A' {
                                strcpy(&mut sl[1].exp, s(0x10080574));
                            } else if c == b'F' {
                                strcpy(&mut sl[1].exp, s(0x1008056c));
                            }
                            if sl[0].plural != 0 && sl[1].plural == 0 {
                                strcat(&mut sl[1].exp, s(0x10077770));
                            }
                            return true;
                        }
                    }
                }
                return false;
            }
            if c0 == b'C' && c1 == b'P' {
                let w = sl[0].text;
                if stricmp(&w, s(0x10080554)) == 0 || stricmp(&w, s(0x1007d354)) == 0 {
                    if stricmp(&sl[1].text, s(0x10080550)) == 0 || stricmp(&sl[1].text, s(0x1008054c)) == 0 {
                        if sl[0].len == 1 && upper(w[0]) == 0x54 {
                            strcpy(&mut sl[0].exp, s(0x10080544));
                        }
                        return true;
                    }
                }
            }
            false
        }
        3 | 4 => {
            if sl[1].ws != sl[2].ws {
                return false;
            }
            if sl[0].tag == b'C' && sl[2].tag == b'C' && sl[0].pow < 1 && sl[2].pow < 1 {
                if !f2c && n0.len == 0 {
                    return false;
                }
                if n0.len > 0 && is_a_an(&n0.text) {
                    return false;
                }
                if sl[1].ws != 0 {
                    return false;
                }
                if idx == 3 {
                    if sl[0].len == 1 && sl[2].len == 1 && ((sl[0].text[0] == b'C' && sl[2].text[0] == b'C') || (sl[0].text[0] == b'M' && sl[2].text[0] == b'V')) {
                        return false;
                    }
                } else if sl[0].ws == 0 && n0.len > 0 && nm.sl[3].len == 0 {
                    return false;
                }
            }
            true
        }
        5 => sl[3].ws == sl[4].ws,
        _ => true,
    }
}

/// `FUN_1003f180`: a number list/range slot read token by token.
fn read_nums(fe: &Fe, wa: &mut Wa, t: &[u8], sl: &Slot) -> bool {
    let len = sl.end - sl.start;
    let mut used = 0;
    let mut o = sl.start;
    let mut q = sl.tp;
    let mut buf = new_buf();
    while used < len {
        let (x, r) = tk(fe, t, q, o, 0xb);
        let ok = match x.class {
            2 => {
                number(&mut buf, &x.text, 0);
                let ok = wa.emit_split(&buf, x.start, x.end, 0x44, 0x44, 0xf);
                buf[0] = 0;
                ok
            }
            1 => wa.emit(x.text(), x.start, x.end, 0x41, 0x44, 0xf),
            3 => {
                if x.text[0] != 0 && strchr(s(0x10080540), x.text[0]) {
                    wa.emit(s(0x10077380), x.start, x.end, 0x41, 0x44, 0xf)
                } else if x.text[0] == b'&' {
                    wa.emit(s(0x10077400), x.start, x.end, 0x41, 0x44, 0xf)
                } else {
                    true
                }
            }
            6 if x.text[0] == 0x96 || x.text[0] == 0x97 => wa.emit(s(0x10077380), x.start, x.end, 0x41, 0x44, 0xf),
            _ => true,
        };
        if !ok {
            return false;
        }
        if r == 0 {
            break;
        }
        used += r;
        o += r;
        q += r as usize;
    }
    true
}

fn mark_plural(wa: &mut Wa) {
    let n = wa.n() as i32;
    wa.set_i16(Wa::rec(n - 1) + 0x1e, 1);
}

/// `FUN_1003ef80`.
fn emit_units(fe: &Fe, wa: &mut Wa, t: &[u8], sl: &[Slot], n: usize, nm: &Nums) -> bool {
    let n0 = &nm.sl[0];
    if n0.len > 0 && n0.class == 2 {
        if strcmp(&n0.text, s(NUMS)) == 0 {
            if !read_nums(fe, wa, t, n0) {
                return false;
            }
        } else {
            let mut buf = new_buf();
            number(&mut buf, &n0.text, 0);
            if !wa.emit_split(&buf, n0.start, n0.end, 0x44, 0x44, 0xf) {
                return false;
            }
        }
        if n0.plural != 0 {
            mark_plural(wa);
        }
    }
    let prefix = rec30(0x10095d98, 0x1009b08c);
    let power = rec30(0x10095c48, 0x1009b088);
    for x in sl.iter().take(n) {
        if x.len <= 0 {
            continue;
        }
        let v = x.pow;
        if v > 9 && !wa.emit_split(prefix.field((v - 10) as i32, 0x10), x.start, x.end, 0x44, 0x44, 0xf) {
            return false;
        }
        match x.tag {
            b'/' | b'O' | b'S' => {
                if !wa.emit_split(&x.exp, x.start, x.end, 0x53, 0x44, 0xf) {
                    return false;
                }
            }
            b'a' => {
                if !wa.emit(cs(&x.text), x.start, x.end, 0x41, 0x44, 0xf) {
                    return false;
                }
            }
            _ => {
                if !wa.emit_split(&x.exp, x.start, x.end, 0x41, 0x44, 0xf) {
                    return false;
                }
                if x.plural != 0 {
                    mark_plural(wa);
                }
            }
        }
        if v > 0 && v < 10 && !wa.emit_split(power.field((v - 1) as i32, 0x10), x.start, x.end, 0x44, 0x44, 0xf) {
            return false;
        }
    }
    true
}

/// Pending "cubic"/"square" prefix (`local_414`), merged into the next unit slot.
#[derive(Default)]
struct Pending {
    ws: i32,
    start: i32,
    len: i32,
    idx: i16,
    dot: bool,
}

pub fn units(fe: &Fe, wa: &mut Wa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let mut sl: Vec<Slot> = vec![Slot::default(); 20];
    let mut nm = Nums { sl: Default::default() };
    let mut cl = [0u8; 8];
    let mut tags = [0u8; 40];
    let mut ti: usize = 0;
    let mut k: usize = 0;
    let mut exp = [0u8; 0x100];
    let mut pend = Pending::default();
    let mut plural = false;
    let mut f2c = false;

    let r40 = nums_parse(fe, &mut nm, wa, tok, t, p, off);
    let mut used: i32 = 0;
    let (mut q, mut o) = (p, off);
    if r40 == -1 {
        if nm.sl[1].tag == b'$' {
            tags[0] = b'$';
            cl[0] = b'$';
            ti = 1;
            k = 1;
        }
    } else if r40 > 0 {
        used = r40;
        q += r40 as usize;
        o += r40;
    }
    if nm.sl[0].plural != 0 {
        plural = true;
    }

    while k < 5 {
        let (x, c): (Tok, i32);
        let mut c_mut;
        if used < 1 {
            x = tok.clone();
            c = tok.consumed;
        } else {
            let (xx, cc) = tk(fe, t, q, o, 0xb);
            if cc == 0 || xx.class == 9 {
                break;
            }
            x = xx;
            c = cc;
        }
        c_mut = c;
        if x.nl == 0 && x.ws > 2 {
            break;
        }
        let t0 = x.text[0];
        // Tag for the slot stored at LAB_1003ec05, or None when the token
        // only extended the tag string.
        let mut store: Option<u8> = None;
        'body: {
            if (x.class == 3 || x.class == 7) && x.len == 1 && t0 == b'/' {
                cl[k] = b'/';
                strcpy(&mut exp, s(0x10078b04));
                store = Some(b'/');
                break 'body;
            }
            let mut to_717 = x.ws != 0 || x.len > 2;
            if !to_717 {
                if t0 == b'.' && k > 0 && cl[k - 1] == b'A' && ti > 0 && tags[ti - 1] != 0 && strchr(s(0x1008053c), tags[ti - 1]) {
                    tags[ti] = b'.';
                    q += c as usize;
                    o += c;
                    sl[k - 1].dot = 1;
                    ti += 1;
                    break 'body;
                }
                if x.len < 3 && t0 == b'.' {
                    if ti > 0 {
                        if tags[ti - 1] == b'W' {
                            tags[ti] = b'.';
                            o += c;
                            q += c as usize;
                            pend.len += 1;
                            used += c;
                            pend.dot = true;
                            ti += 1;
                            break 'body;
                        }
                        to_717 = true;
                    }
                } else {
                    to_717 = true;
                }
            }
            if to_717 && ti > 0 {
                let times = (t0 == b'*' && x.len == 1)
                    || (t0 == b'x' && x.len == 1)
                    || (strcmp(&x.text, s(0x100803b0)) == 0 && x.len == 2)
                    || ((t0 == 0xb7 || t0 == 0xd7) && x.len == 1);
                if times && x.ws < 2 {
                    cl[k] = b'O';
                    if lower(t0) == 0x78 {
                        strcpy(&mut exp, s(0x1007d344));
                    }
                    store = Some(b'O');
                    break 'body;
                }
                if ((strcmp(&x.text, s(0x100803ac)) == 0 && x.len == 2) || (t0 == 0xf7 && x.len == 1)) && x.ws < 2 {
                    cl[k] = b'O';
                    strcpy(&mut exp, s(0x10078b04));
                    store = Some(b'O');
                    break 'body;
                }
            }
            if k >= 1 && cl[k - 1] == b'A' && (x.len == 1 || x.len == 2) && x.class == 3 && x.ws == 0 {
                let i = rec30(0x10095c48, 0x1009b088).find(&x.text, true);
                if i >= 0 {
                    q += c as usize;
                    used += c;
                    o += c;
                    tags[ti] = b's';
                    sl[k - 1].end += c;
                    sl[k - 1].len += c;
                    sl[k - 1].pow = i as i16 + 1;
                    ti += 1;
                    break 'body;
                }
            }
            let prev_w = |tags: &[u8], ti: usize, pend: &Pending| ti > 0 && (tags[ti - 1] == b'W' || (tags[ti - 1] == b'.' && pend.dot));
            let mut take_pending = false;
            let mut try_e = x.class == 1 || x.class == 3 || x.class == 6;
            if pend.len == 0 && x.class == 1 {
                let i = rec30(0x10095d98, 0x1009b08c).find(&x.text, true);
                if i >= 0 {
                    pend.start = x.start;
                    pend.ws = x.ws;
                    pend.len = x.len;
                    o += c;
                    tags[ti] = b'W';
                    pend.idx = i as i16 + 10;
                    used += c;
                    q += c as usize;
                    ti += 1;
                    break 'body;
                }
            }
            if pend.len == 0 && x.class != 1 {
                try_e = x.class == 3 || x.class == 6;
            }
            if try_e && dict_find(&mut exp, &DICT_E, &x.text, plural) {
                cl[k] = b'A';
                tags[ti] = b'E';
                if plural {
                    plural = false;
                    sl[k].plural = 1;
                }
                if pend.len > 0 && prev_w(&tags, ti, &pend) {
                    take_pending = true;
                }
            } else if x.class == 1 && dict_find(&mut exp, &DICT_P, &x.text, plural) {
                cl[k] = b'A';
                tags[ti] = b'P';
                if plural {
                    plural = false;
                    sl[k].plural = 1;
                }
                if pend.len > 0 && prev_w(&tags, ti, &pend) {
                    take_pending = true;
                }
            } else if x.class == 1 && dict_find(&mut exp, &DICT_C, &x.text, plural) {
                tags[ti] = b'C';
                cl[k] = b'A';
                let pl = plural;
                let mut e2 = [0u8; 0x100];
                strcpy(&mut e2, &exp);
                context_unit(fe, Some(&mut e2), pl, &x.text, t, p, off, used + c);
                strcpy(&mut exp, &e2);
                if pl {
                    plural = false;
                    sl[k].plural = 1;
                }
                if pend.len > 0 && prev_w(&tags, ti, &pend) {
                    take_pending = true;
                }
            } else {
                if !phrase_lookup(fe, &mut exp, &x.text, t, q, used, &mut c_mut) {
                    if k != 2 || cl[0] != b'$' || cl[1] != b'/' || x.class != 1 || sl[1].ws != 0 {
                        return finish(fe, wa, t, p, off, &mut sl, &mut cl, k, &nm, &mut f2c);
                    }
                    cl[2] = b'A';
                    tags[ti] = b'a';
                } else {
                    cl[k] = b'A';
                    tags[ti] = b'E';
                    if plural {
                        plural = false;
                        sl[k].plural = 1;
                    }
                    if pend.len > 0 && prev_w(&tags, ti, &pend) {
                        take_pending = true;
                    }
                }
            }
            if take_pending {
                sl[k].start = pend.start;
                sl[k].ws = pend.ws;
                sl[k].pow = pend.idx;
                pend = Pending::default();
            }
            store = Some(tags[ti]);
        }
        if let Some(tag) = store {
            if tag == b'/' || tag == b'O' {
                tags[ti] = tag;
            }
            let c = c_mut;
            let s_ = &mut sl[k];
            if s_.start == 0 {
                s_.start = x.start;
                s_.ws = x.ws;
            }
            s_.end = o + c;
            s_.len = x.len;
            s_.class = x.class as u8;
            s_.tag = tags[ti];
            s_.tp = q + c as usize;
            strcpy(&mut s_.text, &x.text);
            if exp[0] != 0 {
                strcpy(&mut s_.exp, &exp);
                exp[0] = 0;
            }
            used += c;
            o += c;
            q += c as usize;
            k += 1;
            ti += 1;
        }
    }
    finish(fe, wa, t, p, off, &mut sl, &mut cl, k, &nm, &mut f2c)
}

/// Pattern matching and emission after the slot scan (from LAB after the loop).
#[allow(clippy::too_many_arguments)]
fn finish(fe: &Fe, wa: &mut Wa, t: &[u8], p: usize, off: i32, sl: &mut [Slot], cl: &mut [u8], k: usize, nm: &Nums, f2c: &mut bool) -> i32 {
    cl[k] = 0;
    let mode = match cl[0] {
        b'$' => 1,
        b'A' => 2,
        _ => return -1,
    };
    let pats = ptabn(0x10080508, 6);
    let mut kk = k;
    while kk >= 1 {
        cl[kk] = 0;
        let range = match (mode, kk) {
            (1, 3) => Some((0, 1)),
            (2, 1) => Some((1, 2)),
            (2, 2) => Some((2, 3)),
            (2, 3) => Some((3, 5)),
            (2, 5) => Some((5, rd_i32(0x10080520) as usize)),
            _ => None,
        };
        if let Some((lo, hi)) = range {
            for idx in lo..hi {
                if strcmp(pats.get(idx as i32), cl) != 0 {
                    continue;
                }
                let u = sl[kk - 1].end - off;
                let n0len = nm.sl[0].len;
                if idx == 1 {
                    if n0len > 0 {
                        if sl[0].tag == b'C' && context_unit(fe, None, false, &sl[0].text, t, p, off, u) {
                            *f2c = true;
                        } else if n0len > 0 && stricmp(&sl[0].text, s(0x10077688)) == 0 && context_unit(fe, None, false, &sl[0].text, t, p, off, u) {
                            *f2c = true;
                        }
                    }
                } else if idx == 3 {
                    if sl[0].tag == b'C' && context_unit(fe, None, false, &sl[0].text, t, p, off, u) {
                        *f2c = true;
                    }
                    if sl[2].tag == b'C' && context_unit(fe, None, false, &sl[2].text, t, p, off, u) {
                        *f2c = true;
                    }
                }
                if validate(fe, t, sl, nm, idx, *f2c) {
                    return if emit_units(fe, wa, t, sl, kk, nm) { u } else { 0 };
                }
                break;
            }
        }
        kk -= 1;
    }
    -1
}
