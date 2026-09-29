//! User dictionary pre-pass (`FUN_10049e30`), search (`FUN_10049f50`
//! case-sensitive / `FUN_1004a0c0` case-insensitive) and the token-boundary
//! check (`FUN_1004b400`). Called by the driver before punctuation for token
//! classes 1..3 (word/number/symbol), and recursively from `spell::expand`
//! (`FUN_10035880`, inside VTML).
//!
//! Unlike English's `userdict_eng.csv` (key/target/kind[/alias]), the shipped
//! `userdict_spa.csv` has 2 fields: `"key","target[CI marker]"`. The loader
//! itself is not part of the front-end call tree (loaded at engine init, not
//! from `fe_NextSentence`), so it is reconstructed from the sample file and
//! the single `"[CI]"` string constant in the DLL, not from disassembly.

use super::emit::emit_bracketed;
use super::tokenize::tokenize;
use super::FeSpa;
use crate::ling_spa::lexicon::strnicmp_spa;
use crate::ling_spa::WaSpa;

#[derive(Clone, Debug)]
pub struct EntrySpa {
    pub key: Vec<u8>,
    pub target: Vec<u8>,
}

#[derive(Default, Clone, Debug)]
pub struct ListSpa {
    pub maxlen: i32,
    pub start: Vec<i32>,
    pub end: Vec<i32>,
    pub entries: Vec<EntrySpa>,
}

#[derive(Default, Clone, Debug)]
pub struct UserDictSpa {
    pub s: Option<ListSpa>,
    pub i: Option<ListSpa>,
}

fn ws(c: u8) -> bool {
    matches!(c, b' ' | 9 | 10 | 13)
}

fn csv_fields(line: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i <= line.len() {
        let mut f = Vec::new();
        if i < line.len() && line[i] == b'"' {
            i += 1;
            while i < line.len() {
                if line[i] == b'"' {
                    if i + 1 < line.len() && line[i + 1] == b'"' {
                        f.push(b'"');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                f.push(line[i]);
                i += 1;
            }
            while i < line.len() && line[i] != b',' {
                i += 1;
            }
        } else {
            while i < line.len() && line[i] != b',' {
                f.push(line[i]);
                i += 1;
            }
        }
        out.push(f);
        i += 1;
    }
    out
}

fn clean_key(src: &[u8]) -> Option<Vec<u8>> {
    let mut k: Vec<u8> = src.to_vec();
    while k.first().is_some_and(|&c| ws(c)) {
        k.remove(0);
    }
    while k.last().is_some_and(|&c| ws(c)) {
        k.pop();
    }
    if k.is_empty() {
        None
    } else {
        Some(k)
    }
}

fn strcmp_spa(a: &[u8], b: &[u8]) -> i32 {
    let at = |s: &[u8], i: usize| *s.get(i).unwrap_or(&0);
    let mut i = 0usize;
    loop {
        let (ca, cb) = (at(a, i), at(b, i));
        let d = ca as i32 - cb as i32;
        if d != 0 || ca == 0 {
            return d;
        }
        i += 1;
    }
}

fn strncmp_spa(a: &[u8], b: &[u8], n: usize) -> i32 {
    let at = |s: &[u8], i: usize| *s.get(i).unwrap_or(&0);
    for i in 0..n {
        let (ca, cb) = (at(a, i), at(b, i));
        let d = ca as i32 - cb as i32;
        if d != 0 || ca == 0 {
            return d;
        }
    }
    0
}

impl ListSpa {
    fn build(mut entries: Vec<EntrySpa>, insensitive: bool) -> ListSpa {
        let n = entries.len();
        for a in 0..n.saturating_sub(1) {
            for b in a..n {
                if entries[b].key.len() < entries[a].key.len() {
                    entries.swap(a, b);
                }
            }
        }
        let maxlen = entries.iter().map(|e| e.key.len()).max().unwrap_or(0) as i32;
        let mut start = vec![-1i32; maxlen as usize];
        let mut end = vec![-1i32; maxlen as usize];
        for l in 1..=maxlen as usize {
            if let Some(s0) = entries.iter().position(|e| e.key.len() == l) {
                let mut e0 = s0;
                while e0 < n && entries[e0].key.len() == l {
                    e0 += 1;
                }
                start[l - 1] = s0 as i32;
                end[l - 1] = e0 as i32 - 1;
                let cmp = |x: &EntrySpa, y: &EntrySpa| if insensitive { strnicmp_spa(&x.key, &y.key, x.key.len().max(y.key.len()) + 1) } else { strcmp_spa(&x.key, &y.key) };
                for a in s0..e0.saturating_sub(1) {
                    for b in a..e0 {
                        if cmp(&entries[a], &entries[b]) > 0 {
                            entries.swap(a, b);
                        }
                    }
                }
            }
        }
        ListSpa { maxlen, start, end, entries }
    }
}

impl UserDictSpa {
    /// Not from disassembly (the loader is outside the text front-end call
    /// tree): `"key","target"`, an optional trailing `[CI]` (any case) on the
    /// target selects the case-insensitive list and is cut off.
    pub fn load(csv: &[u8]) -> Option<UserDictSpa> {
        let mut s_list = Vec::new();
        let mut i_list = Vec::new();
        for raw in csv.split(|&c| c == b'\n') {
            let line: Vec<u8> = raw.iter().copied().filter(|&c| c != b'\r').collect();
            if line.is_empty() {
                continue;
            }
            let f = csv_fields(&line);
            if f.len() < 2 {
                continue;
            }
            let Some(key) = clean_key(&f[0]) else { continue };
            let tgt = &f[1];
            let up: Vec<u8> = tgt.iter().map(|&c| if c.is_ascii_lowercase() { c - 32 } else { c }).collect();
            let ci = up.windows(4).any(|w| w == b"[CI]");
            let target = match tgt.iter().position(|&c| c == b'[') {
                Some(p) => tgt[..p].to_vec(),
                None => tgt.clone(),
            };
            let e = EntrySpa { key, target };
            if ci {
                i_list.push(e);
            } else {
                s_list.push(e);
            }
        }
        if s_list.is_empty() && i_list.is_empty() {
            return None;
        }
        Some(UserDictSpa {
            s: if s_list.is_empty() { None } else { Some(ListSpa::build(s_list, false)) },
            i: if i_list.is_empty() { None } else { Some(ListSpa::build(i_list, true)) },
        })
    }
}

/// `FUN_1004b590`: the next `n` bytes from `t[p..]` are all non-NUL.
fn has_n(t: &[u8], p: usize, n: usize) -> bool {
    (0..n).all(|k| *t.get(p + k).unwrap_or(&0) != 0)
}

/// `FUN_1004b400`: does a match of `len` bytes at `t[p..]` end on a token
/// boundary? The decompile runs the `.`-followed-by-letter check twice (once
/// gated by `flag != 0`, once unconditionally); since both checks are
/// identical this is folded into one.
fn boundary(t: &[u8], p: usize, len: i32) -> bool {
    let end = p + len as usize;
    if ws(*t.get(end).unwrap_or(&0)) {
        return true;
    }
    let mut used = 0i32;
    let mut q = p;
    let last;
    loop {
        let (tok, r) = tokenize(t, q, used, 0);
        if r == 0 || tok.class == 10 || tok.len == 0 {
            return true;
        }
        used += r;
        q += r as usize;
        if used >= len {
            last = tok;
            break;
        }
    }
    if used == len {
        return true;
    }
    let txt = last.text();
    let aligned = |pos: usize| q as isize - (txt.len() - pos) as isize == end as isize;
    let alpha = |c: u8| super::tables::CLS_S[c as usize] & 0xc0 != 0 || (c > 0x7f && super::tables::HI[c as usize] & 0xc0 != 0);
    if let Some(d) = txt.iter().position(|&c| c == b'.') {
        if d > 0 && alpha(*txt.get(d + 1).unwrap_or(&0)) && aligned(d) {
            return true;
        }
    }
    if let Some(d) = txt.iter().position(|&c| c == b'-') {
        if d > 0 && alpha(*txt.get(d + 1).unwrap_or(&0)) && aligned(d) {
            return true;
        }
    }
    false
}

/// `FUN_10049f50` (case-sensitive) / `FUN_1004a0c0` (case-insensitive):
/// longest entry of `list` matching `t[p..]` on a token boundary.
fn find(list: &ListSpa, t: &[u8], p: usize, flag: i16, insensitive: bool) -> i32 {
    let cmp = |k: &[u8], l: usize| if insensitive { strnicmp_spa(&t[p.min(t.len())..], k, l) } else { strncmp_spa(&t[p.min(t.len())..], k, l) };
    let mut l = list.maxlen;
    loop {
        if l < 1 {
            return -1;
        }
        let li = l as usize - 1;
        if li < list.start.len() {
            let (s0, e0) = (list.start[li], list.end[li]);
            if s0 >= 0 && e0 >= 0 && has_n(t, p, l as usize) {
                let (mut lo, mut hi) = (s0, e0);
                loop {
                    if cmp(&list.entries[lo as usize].key, l as usize) == 0 && boundary_flag(t, p, l, flag) {
                        return lo;
                    }
                    if cmp(&list.entries[hi as usize].key, l as usize) == 0 && boundary_flag(t, p, l, flag) {
                        return hi;
                    }
                    if hi - lo < 2 {
                        break;
                    }
                    let mid = (hi - lo) / 2 + lo;
                    let c = cmp(&list.entries[mid as usize].key, l as usize);
                    if c != 0 {
                        if c > -1 {
                            lo = mid;
                        } else {
                            hi = mid;
                        }
                        continue;
                    }
                    if boundary_flag(t, p, l, flag) {
                        return mid;
                    }
                    lo = mid;
                }
            }
        }
        l -= 1;
    }
}

fn boundary_flag(t: &[u8], p: usize, len: i32, _flag: i16) -> bool {
    boundary(t, p, len)
}

fn find_s(ud: &UserDictSpa, t: &[u8], p: usize, flag: i16) -> i32 {
    match &ud.s {
        Some(l) => find(l, t, p, flag, false),
        None => -1,
    }
}

fn find_i(ud: &UserDictSpa, t: &[u8], p: usize, flag: i16) -> i32 {
    match &ud.i {
        Some(l) => find(l, t, p, flag, true),
        None => -1,
    }
}

/// `FUN_10049e30(wa, t + p, start, end_override)`: `start` is the offset
/// attributed to the emitted word and stays fixed across `spell::expand`'s
/// loop even as `p` advances (a faithfully-reproduced quirk: only the
/// tokenize calls there use the advancing offset).
pub fn prepass(fe: &FeSpa, wa: &mut WaSpa, t: &[u8], p: usize, start: i32, end_override: i16) -> i32 {
    let Some(ud) = fe.userdict.as_ref() else {
        return -1;
    };
    if ud.s.is_none() && ud.i.is_none() {
        return -1;
    }
    let mut p = p;
    let mut start = start;
    let mut ws_skip = 0i32;
    while ws(*t.get(p).unwrap_or(&0)) {
        p += 1;
        start += 1;
        ws_skip += 1;
    }
    let si = find_s(ud, t, p, end_override);
    let (list, idx) = if si >= 0 {
        (ud.s.as_ref().unwrap(), si)
    } else {
        let ii = find_i(ud, t, p, end_override);
        if ii < 0 {
            return -1;
        }
        (ud.i.as_ref().unwrap(), ii)
    };
    let e = &list.entries[idx as usize];
    let klen = e.key.len() as i32;
    let tlen = e.target.len() as i32;
    if tlen >= 0x32 {
        return -1;
    }
    if tlen != 0 {
        let end = if end_override == 0 { klen + start } else { end_override as i32 };
        if !emit_bracketed(wa, &e.target, start, end, b'U', b'U', 7) {
            return 0;
        }
    }
    klen + ws_skip
}
