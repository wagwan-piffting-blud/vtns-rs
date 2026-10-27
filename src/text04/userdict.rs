//! 2004 user dictionary (`data-common/userdict/userdict_eng.csv`): loader `FUN_10036430`, index builder
//! `FUN_10037190`, the multi-word match `FUN_1004f090` and its boundary check `FUN_1004d490`.
//!
//! Every entry is in two lists sorted by key length (longest first): `ins` (then by `stricmp`, matched
//! with `strnicmp`) and `sens` (then by `strcmp`, matched with `strncmp`); `groups` holds the first index
//! of each length.

use super::mem::*;
use super::tokenize::{tokenize, Tok};

#[derive(Clone, Debug)]
pub struct Ent {
    pub len: i32,
    pub idx: i32,
    pub key: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct UserDict {
    pub groups: Vec<i32>,
    pub ins: Vec<Ent>,
    pub sens: Vec<Ent>,
    /// Per CSV entry: kind (`A` alias, `P` phones) and target (an alias, or phone codes with 100 for `#`).
    pub info: Vec<(u8, Vec<u8>)>,
}

const PHONES: Ptab = ptab(0x100656d8, 0x45);

/// `FUN_100511c0` with delimiter "," and at most 100 fields: `None` for a malformed line.
fn csv_fields(line: &[u8]) -> Option<Vec<Vec<u8>>> {
    let ws = |c: u8| matches!(c, 9 | 10 | 13 | 0x20);
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        while ws(at(line, i as isize)) {
            i += 1;
        }
        let mut f = Vec::new();
        let mut j;
        if at(line, i as isize) == b'"' {
            // FUN_10051300: unescape `""`, stop after the closing quote.
            j = i + 1;
            loop {
                let c = at(line, j as isize);
                if c == 0 {
                    break;
                }
                if c == b'"' {
                    if at(line, j as isize + 1) != b'"' {
                        j += 1;
                        break;
                    }
                    j += 1;
                }
                f.push(at(line, j as isize));
                j += 1;
                if at(line, j as isize) == 0 {
                    break;
                }
            }
        } else {
            j = i;
            while at(line, j as isize) != 0 && at(line, j as isize) != b',' {
                j += 1;
            }
            f.extend_from_slice(&line[i..j]);
        }
        while ws(at(line, j as isize)) {
            j += 1;
        }
        let c = at(line, j as isize);
        out.push(f);
        if c != 0 {
            j += 1;
        }
        if at(line, j as isize) == 0 {
            return Some(out);
        }
        if out.len() >= 100 {
            return None;
        }
        i = j;
    }
}

fn ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r')
}

/// `(&DAT_1006ad60)[(char)c * 2]`: the upper-casing table.
fn upper(c: u8) -> u8 {
    b(0x1006ac60, 768)[(256 + 2 * c as i8 as i32) as usize]
}

/// `FUN_1004f3a0`: trimmed key of 1..=0x32 bytes.
fn clean_key(src: &[u8]) -> Option<Vec<u8>> {
    let src = cs(src);
    let st = src.iter().position(|&c| !ws(c)).unwrap_or(src.len());
    let s = &src[st..];
    if s.len() > 100 {
        return None;
    }
    let mut v = s.to_vec();
    while v.last().is_some_and(|&c| ws(c)) {
        v.pop();
    }
    if v.is_empty() || v.len() > 0x32 {
        return None;
    }
    Some(v)
}

/// `FUN_1004f420`: an `A` target (blank runs collapsed; letters, digits, the characters of
/// `DAT_10085810`, `<` before and `>` after a letter) or a `P` target (blank-separated phone names).
fn clean_target(src: &[u8], kind: u8) -> Option<Vec<u8>> {
    let src = cs(src);
    let mut i = src.iter().position(|&c| !ws(c)).unwrap_or(src.len());
    let mut out = Vec::new();
    if kind == b'A' {
        while i < src.len() {
            let c = src[i];
            if out.len() > 99 {
                return None;
            }
            if ws(c) {
                out.push(b' ');
                i += 1;
                while i < src.len() && ws(src[i]) {
                    i += 1;
                }
                continue;
            }
            if cls(c) & 0xd0 == 0 && !s(0x10085810).contains(&c) {
                let lt = c == b'<' && is_alpha(at(src, i as isize + 1));
                let gt = i >= 1 && c == b'>' && is_alpha(src[i - 1]);
                if !lt && !gt {
                    return None;
                }
            }
            out.push(c);
            i += 1;
        }
    } else {
        while i < src.len() {
            while i < src.len() && ws(src[i]) {
                i += 1;
            }
            if i >= src.len() {
                break;
            }
            let mut name = Vec::new();
            while i < src.len() && !ws(src[i]) {
                name.push(upper(src[i]));
                i += 1;
            }
            if name == b"#" {
                out.push(100);
            } else {
                let k = (0..PHONES.count()).find(|&k| PHONES.get(k) == name.as_slice())?;
                if out.len() > 99 {
                    return None;
                }
                out.push(k as u8 + 1);
            }
        }
    }
    while out.last().is_some_and(|&c| ws(c)) {
        out.pop();
    }
    if out.len() > 0x32 {
        return None;
    }
    Some(out)
}

impl UserDict {
    pub fn load(csv: &[u8]) -> Option<UserDict> {
        let mut ud = UserDict::default();
        let mut ents = Vec::new();
        for line in csv.split(|&c| c == b'\n') {
            if line.first().is_none_or(|&c| c == 0) {
                break;
            }
            let Some(f) = csv_fields(line) else { continue };
            if f.len() != 3 && f.len() != 4 {
                continue;
            }
            if f[2].len() != 1 {
                continue;
            }
            let kind = upper(f[2][0]);
            if kind != b'A' && kind != b'P' {
                continue;
            }
            let Some(key) = clean_key(&f[0]) else { continue };
            let Some(target) = clean_target(&f[1], kind) else { continue };
            if target.is_empty() {
                continue;
            }
            ents.push(Ent { len: key.len() as i32, idx: ud.info.len() as i32, key });
            ud.info.push((kind, target));
        }
        if ents.is_empty() {
            return None;
        }
        let n = ents.len();
        for i in 0..n {
            for j in i + 1..n {
                if ents[i].len < ents[j].len {
                    ents.swap(i, j);
                }
            }
        }
        let mut sens = ents.clone();
        ud.groups.push(0);
        for k in 1..n {
            if ents[k - 1].len != ents[k].len {
                ud.groups.push(k as i32);
            }
        }
        for i in 0..n {
            for j in i + 1..n {
                if ents[i].len == ents[j].len && stricmp(&ents[i].key, &ents[j].key) > 0 {
                    ents.swap(i, j);
                }
            }
        }
        for i in 0..n {
            for j in i + 1..n {
                if sens[i].len == sens[j].len && strcmp(&sens[i].key, &sens[j].key) > 0 {
                    sens.swap(i, j);
                }
            }
        }
        ud.ins = ents;
        ud.sens = sens;
        Some(ud)
    }

    /// `FUN_1004f310`: bisection of `t[p..]` over entries `lo..=hi` of one list.
    fn search(&self, t: &[u8], p: usize, lo: i32, hi: i32, insensitive: bool) -> i32 {
        let list = if insensitive { &self.ins } else { &self.sens };
        let (mut lo, mut hi) = (lo, hi);
        let mut mid = lo + hi;
        while lo <= hi {
            mid /= 2;
            let e = &list[mid as usize];
            let c = if insensitive { strnicmp(&e.key, &t[p..], e.len as usize) } else { strncmp(&e.key, &t[p..], e.len as usize) };
            if c == 0 {
                return mid;
            }
            if c < 0 {
                lo = mid + 1;
            } else {
                hi = mid - 1;
            }
            mid = lo + hi;
        }
        -1
    }

    /// `FUN_1004f090`: the longest entry at `*cur` (after blanks; two newlines stop it) that ends on a
    /// token boundary, case-sensitive list first. Fills `tok` as a word and returns the entry index.
    pub fn match_at(&self, t: &[u8], cur: &mut usize, base: usize, tok: &mut Tok, flag: i16) -> i32 {
        let start = *cur;
        let mut nl = 0;
        let mut skipped = 0;
        loop {
            let c = at(t, *cur as isize);
            if c == b'\n' {
                nl += 1;
            } else if !matches!(c, b' ' | b'\t' | b'\r') {
                break;
            }
            skipped += 1;
            *cur += 1;
        }
        if nl > 1 {
            *cur = start;
            return -1;
        }
        let n = self.ins.len() as i32;
        for insensitive in [false, true] {
            for g in 0..self.groups.len() {
                let lo = self.groups[g];
                let hi = self.groups.get(g + 1).copied().unwrap_or(n) - 1;
                let k = self.search(t, *cur, lo, hi, insensitive);
                if k < 0 {
                    continue;
                }
                let e = if insensitive { &self.ins[k as usize] } else { &self.sens[k as usize] };
                if !boundary(self, t, base, *cur, e.len as usize, flag) {
                    continue;
                }
                *tok = Tok::default();
                tok.start = (*cur - base) as i32;
                let l = e.len as usize;
                tok.set_text(&t[*cur..*cur + l]);
                tok.end = l as i32 + tok.start;
                tok.ws = skipped;
                tok.len = l as i32;
                tok.ty = 1;
                *cur += l;
                return e.idx;
            }
        }
        *cur -= skipped as usize;
        -1
    }
}

/// `FUN_1004d0d0`: `'s 'd 'm 've 'em 're 'll` exactly.
pub fn apostrophe_suffix(x: &[u8]) -> bool {
    let x = cs(x);
    if x.first() != Some(&b'\'') {
        return false;
    }
    let f = |c: u8| fold_s(c);
    if x.len() == 2 && matches!(f(x[1]), 0x73 | 100 | 0x6d) {
        return true;
    }
    x.len() == 3 && matches!((f(x[1]), f(x[2])), (0x76, 0x65) | (0x65, 0x6d) | (0x72, 0x65) | (0x6c, 0x6c))
}

/// `FUN_1004d490` in mode 'D': does a match of `len` bytes at `t[p..]` end where tokenization from it
/// ends? With `flag`, a token running on past it by a `.letter`, an apostrophe suffix or a `-letter`
/// tail still counts.
fn boundary(ud: &UserDict, t: &[u8], base: usize, p: usize, len: usize, flag: i16) -> bool {
    let mut q = p;
    while base < q && matches!(at(t, (q + len) as isize - 1), b' ' | b'\t' | b'\n' | b'\r') {
        q -= 1;
    }
    let end = p + len;
    let mut tok = Tok::default();
    loop {
        tokenize(&mut tok, t, &mut q, base, Some(ud));
        if q >= end {
            break;
        }
    }
    if q == end {
        return true;
    }
    let text = tok.text().to_vec();
    let tail = |c: u8| text.iter().position(|&x| x == c).map(|i| text[i..].to_vec());
    if flag != 0 {
        if let Some(d) = tail(b'.') {
            if d.len() < text.len() && is_alpha(at(&d, 1)) && q - d.len() == end {
                return true;
            }
        }
    }
    if let Some(a) = tail(b'\'') {
        if a.len() < text.len() && apostrophe_suffix(&a) && q - a.len() == end {
            return true;
        }
    }
    if let Some(h) = tail(b'-') {
        if h.len() < text.len() && is_alpha(at(&h, 1)) && q - h.len() == end {
            return true;
        }
    }
    false
}
