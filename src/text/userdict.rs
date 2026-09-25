//! User dictionary (`data-common/userdict/userdict_eng.csv`): loader
//! `fe_LoadUserDictCsv` (0x1005e330), index builder `FUN_1005f040`, prefix
//! search `FUN_1005e010` (case-sensitive) / `FUN_1005e1a0` (case-insensitive)
//! and the word-boundary check `FUN_1005f840`.
//!
//! Entries are split into an 'S' list (compared with strncmp) and an 'I' list
//! (`[CI]` in the target field; compared through the tolower table). Each list
//! is sorted by key length, then key, and indexed by length.

use super::cstr::*;
use super::tokenize::{tokenize, Tok};

#[derive(Clone, Debug)]
pub struct Entry {
    pub kind: u8,
    pub key: Vec<u8>,
    pub target: Vec<u8>,
    pub is_alias: bool,
}

#[derive(Default, Clone, Debug)]
pub struct List {
    pub maxlen: i32,
    pub start: Vec<i32>,
    pub end: Vec<i32>,
    pub entries: Vec<Entry>,
}

#[derive(Default, Clone, Debug)]
pub struct UserDict {
    pub s: Option<List>,
    pub i: Option<List>,
}

/// CSV fields of one line: `"a","b","c"` with doubled quotes, as written by
/// the Voiceware user-dictionary editor.
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

/// `FUN_1005f2e0`: trimmed key, at most 0x31 bytes; None for invalid keys.
fn clean_key(src: &[u8]) -> Option<Vec<u8>> {
    let mut i = 0;
    while i < src.len() && is_ws(src[i]) {
        i += 1;
    }
    let mut k = Vec::new();
    while i < src.len() && src[i] != 0 {
        if i + 1 < src.len() && dbcs_pair(src[i], src[i + 1]) {
            return None;
        }
        if k.len() > 0x31 {
            return None;
        }
        k.push(src[i]);
        i += 1;
    }
    while k.last().is_some_and(|&c| is_ws(c)) {
        k.pop();
    }
    if k.is_empty() || k.len() > 0x31 {
        None
    } else {
        Some(k)
    }
}

impl List {
    fn build(mut entries: Vec<Entry>, insensitive: bool) -> List {
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
                // Exchange sort, not stable: equal keys put the higher kind
                // first ('S' before 'I').
                let cmp = |x: &Entry, y: &Entry| if insensitive { stricmp(&x.key, &y.key) } else { strcmp(&x.key, &y.key) };
                for a in s0..e0 - 1 {
                    for b in a..e0 {
                        let c = cmp(&entries[a], &entries[b]);
                        if c > 0 || (c == 0 && entries[a].kind < entries[b].kind) {
                            entries.swap(a, b);
                        }
                    }
                }
            }
        }
        List { maxlen, start, end, entries }
    }
}

/// Phone names at `0x10077c80`, sorted; a name's code is its index + 1.
fn phone_code(name: &[u8]) -> Option<u8> {
    crate::phones::NAMES[1..].binary_search_by(|n| n.as_bytes().cmp(name)).ok().map(|i| i as u8 + 1)
}

fn ws4(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r')
}

/// `FUN_1005f5c0` on a 'P' target: trimmed, every blank-separated token a
/// phone name (case-sensitive) or `#`, optionally ending in `[CI]` (any case),
/// which is cut off. Returns the list it belongs to and the cut target.
fn check_phones(t: &[u8]) -> Option<(u8, Vec<u8>)> {
    let mut a = 0;
    let mut b = t.len();
    while a < b && ws4(t[a]) {
        a += 1;
    }
    while b > a && ws4(t[b - 1]) {
        b -= 1;
    }
    let t = &t[a..b];
    if t.is_empty() {
        return None;
    }
    let valid = |tok: &[u8]| tok == b"#" || phone_code(tok).is_some();
    let mut ci = false;
    let mut cut = t.len();
    let mut tok: Vec<u8> = Vec::new();
    let mut count = 0usize;
    let mut i = 0;
    while i < t.len() {
        let c = t[i];
        if c == b' ' || c == b'\t' || c == b'[' {
            if !valid(&tok) {
                return None;
            }
            tok.clear();
            while i < t.len() && (t[i] == b' ' || t[i] == b'\t') {
                i += 1;
                count += 1;
            }
            if i < t.len() && t[i] == b'[' {
                if stricmp(&t[i..], b"[CI]") != 0 {
                    return None;
                }
                cut = i;
                ci = true;
                break;
            }
        } else {
            tok.push(c);
            i += 1;
            count += 1;
        }
    }
    if count > 0x103 {
        return None;
    }
    if !tok.is_empty() && !valid(&tok) {
        return None;
    }
    Some((if ci { b'I' } else { b'S' }, t[..cut].to_vec()))
}

/// `FUN_1005f710`: blank-separated phone names (upper-cased) to codes, `#` to
/// the word break 100, "TD" to 0x4d; at most 0x41 named phones.
pub(crate) fn phones_to_codes(t: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    for tok in t.split(|&c| ws4(c)).filter(|x| !x.is_empty()) {
        let up: Vec<u8> = tok.iter().map(|&c| upper(c) as u8).collect();
        if up == b"#" {
            out.push(100);
            continue;
        }
        let code = match phone_code(&up) {
            Some(c) => c,
            None if up == b"TD" => 0x4d,
            None => return None,
        };
        if out.len() > 0x40 {
            return None;
        }
        out.push(code);
    }
    Some(out)
}

impl UserDict {
    /// Load `userdict_eng.csv`. Returns None when the file holds no usable entry
    /// (the engine then runs without a user dictionary). 'P' targets are stored
    /// as phone codes (`FUN_1005f5c0`/`FUN_1005f710`); 'A' (alias) targets are
    /// kept as text: the engine normalizes them at load time through
    /// `fe_AnalyzeSentence`, which is not reproduced (the installed dictionary
    /// has no alias entries).
    pub fn load(csv: &[u8]) -> Option<UserDict> {
        let mut all = Vec::new();
        for raw in csv.split(|&c| c == b'\n') {
            let line: Vec<u8> = raw.iter().copied().filter(|&c| c != b'\r').collect();
            let f = csv_fields(&line);
            if f.len() != 3 && f.len() != 4 {
                continue;
            }
            if f[2].len() != 1 {
                continue;
            }
            let ty = upper(f[2][0]) as u8;
            if ty != b'A' && ty != b'P' {
                continue;
            }
            let key = match clean_key(&f[0]) {
                Some(k) => k,
                None => continue,
            };
            let tgt = &f[1];
            let (kind, target) = if ty == b'P' {
                let Some((kind, cut)) = check_phones(tgt) else {
                    continue;
                };
                let Some(codes) = phones_to_codes(&cut) else {
                    continue;
                };
                (kind, codes)
            } else {
                let upper_tgt: Vec<u8> = tgt.iter().map(|&c| upper(c) as u8).collect();
                let ci = upper_tgt.windows(4).any(|w| w == b"[CI]");
                let target = match tgt.iter().position(|&c| c == b'[') {
                    Some(p) => tgt[..p].to_vec(),
                    None => tgt.clone(),
                };
                (if ci { b'I' } else { b'S' }, target)
            };
            all.push(Entry { kind, key, target, is_alias: ty == b'A' });
        }
        // Duplicates, judged on the combined list in its case-insensitive
        // order: the later of two adjacent entries goes when both are 'S' and
        // equal exactly, or otherwise equal ignoring case.
        let sorted = List::build(all, true).entries;
        let mut keep = vec![true; sorted.len()];
        for k in 1..sorted.len() {
            let (a, b) = (&sorted[k - 1], &sorted[k]);
            let dup = if a.kind == b'S' && b.kind == b'S' { strcmp(&a.key, &b.key) == 0 } else { stricmp(&a.key, &b.key) == 0 };
            if dup {
                keep[k] = false;
            }
        }
        let mut s_list = Vec::new();
        let mut i_list = Vec::new();
        for (e, k) in sorted.into_iter().zip(keep) {
            if !k {
                continue;
            }
            if e.kind == b'I' {
                i_list.push(e);
            } else {
                s_list.push(e);
            }
        }
        if s_list.is_empty() && i_list.is_empty() {
            return None;
        }
        Some(UserDict {
            s: if s_list.is_empty() { None } else { Some(List::build(s_list, false)) },
            i: if i_list.is_empty() { None } else { Some(List::build(i_list, true)) },
        })
    }

    /// `FUN_1005e010`: longest case-sensitive entry matching at `t[p..]`.
    pub fn find_s(&self, t: &[u8], p: usize, flag: i16) -> i32 {
        find(self.s.as_ref(), t, p, flag, false)
    }

    /// `FUN_1005e1a0`: longest case-insensitive entry matching at `t[p..]`.
    pub fn find_i(&self, t: &[u8], p: usize, flag: i16) -> i32 {
        find(self.i.as_ref(), t, p, flag, true)
    }
}

fn find(list: Option<&List>, t: &[u8], p: usize, flag: i16, insensitive: bool) -> i32 {
    let list = match list {
        Some(l) => l,
        None => return -1,
    };
    let tail = &t[p.min(t.len())..];
    let cmp = |k: &[u8], l: usize| if insensitive { strnicmp(tail, k, l) } else { strncmp(tail, k, l) };
    let mut l = list.maxlen;
    loop {
        if l < 1 {
            return -1;
        }
        let (s0, e0) = (list.start[(l - 1) as usize], list.end[(l - 1) as usize]);
        if s0 >= 0 && e0 >= 0 && has_n(tail, 0, l as usize) {
            let (mut lo, mut hi) = (s0, e0);
            loop {
                if cmp(&list.entries[lo as usize].key, l as usize) == 0 && boundary(t, p, l, flag) {
                    return lo;
                }
                if cmp(&list.entries[hi as usize].key, l as usize) == 0 && boundary(t, p, l, flag) {
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
                if boundary(t, p, l, flag) {
                    return mid;
                }
                lo = mid;
            }
        }
        l -= 1;
    }
}

/// `FUN_1005f9b0`: `'s 'd 'm 've 'em 're 'll` at the end of a token.
fn apostrophe_suffix(x: &[u8]) -> bool {
    if at(x, 0) != b'\'' {
        return false;
    }
    let c1 = at(x, 1);
    let c2 = at(x, 2);
    if c2 == 0 && matches!(lower(c1), 0x73 | 100 | 0x6d) {
        return true;
    }
    if at(x, 3) == 0 {
        let a = lower(c1);
        let b2 = lower(c2);
        return (a == 0x76 && b2 == 0x65) || (a == 0x65 && b2 == 0x6d) || (a == 0x72 && b2 == 0x65) || (a == 0x6c && b2 == 0x6c);
    }
    false
}

/// `FUN_1005f840`: does a match of `len` bytes at `t[p..]` end on a token
/// boundary (mode-0x17 tokenization)?
pub fn boundary(t: &[u8], p: usize, len: i32, flag: i16) -> bool {
    let end = p as isize + len as isize;
    if is_ws(at(t, end)) {
        return true;
    }
    let mut tok = Tok::default();
    let mut used: i32 = 0;
    let mut q = p;
    loop {
        let r = tokenize(&mut tok, t, q, used, 0x17, None);
        if r == 0 || tok.class == 9 || tok.len == 0 {
            return true;
        }
        used += r;
        q += r as usize;
        if used >= len {
            break;
        }
    }
    if used == len {
        return true;
    }
    let txt = tok.text();
    let aligned = |pos: usize| q as isize - (txt.len() - pos) as isize == end;
    if flag != 0 {
        if let Some(d) = strchr_at(&tok.text, b'.') {
            if d > 0 && cls(at(txt, d as isize + 1)) & 0xc0 != 0 && aligned(d) {
                return true;
            }
        }
    }
    if let Some(d) = strchr_at(&tok.text, b'.') {
        if d > 0 && cls(at(txt, d as isize + 1)) & 0xc0 != 0 && aligned(d) {
            return true;
        }
    }
    if let Some(d) = strchr_at(&tok.text, b'\'') {
        if d > 0 && apostrophe_suffix(&txt[d..]) && aligned(d) {
            return true;
        }
    }
    if let Some(d) = strchr_at(&tok.text, b'-') {
        if d > 0 && cls(at(txt, d as isize + 1)) & 0xc0 != 0 && aligned(d) {
            return true;
        }
    }
    false
}
