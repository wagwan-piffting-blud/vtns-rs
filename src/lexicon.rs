//! The English dictionaries in `data-common/dict-eng`:
//! the perfect-hashed main lexicon (`*_emb`), the TPP tagger dictionary
//! (`*_eng_tpp`), the hyphenated-word exception dictionary and the +10-shifted
//! `.txt2` tables. Lookups follow the engine byte for byte.

use std::path::Path;

use crate::era::Era;
use crate::ling::tables::{CODEBOOK, DIGRAPHS, TOLOWER_S, TOLOWER_U};

/// `FUN_1001c2c0`: case-insensitive strcmp through the u16 case map, indexed
/// with an unsigned byte.
pub fn stricmp(a: &[u8], b: &[u8]) -> i32 {
    let at = |s: &[u8], i: usize| *s.get(i).unwrap_or(&0);
    let mut i = 0;
    loop {
        let ca = at(a, i);
        let cb = at(b, i);
        let d = TOLOWER_U[ca as usize] as i32 - TOLOWER_U[cb as usize] as i32;
        if d != 0 {
            return d;
        }
        if ca == 0 {
            return 0;
        }
        i += 1;
    }
}

/// `strcmp` on C strings (unsigned bytes).
pub fn strcmp(a: &[u8], b: &[u8]) -> i32 {
    let at = |s: &[u8], i: usize| *s.get(i).unwrap_or(&0);
    let mut i = 0;
    loop {
        let ca = at(a, i);
        let cb = at(b, i);
        if ca != cb {
            return ca as i32 - cb as i32;
        }
        if ca == 0 {
            return 0;
        }
        i += 1;
    }
}

fn strncmp2(s: &[u8], i: usize, d: &[u8]) -> i32 {
    for k in 0..2 {
        let a = *s.get(i + k).unwrap_or(&0);
        let b = d[k];
        if a != b {
            return a as i32 - b as i32;
        }
        if a == 0 {
            return 0;
        }
    }
    0
}

/// Lower-case map of `fe_EncodeLexKey` (signed index, stops at a 0 result).
pub fn lower_map(word: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(word.len());
    for &c in word {
        let v = TOLOWER_S[c as usize] as u8;
        if v == 0 {
            break;
        }
        out.push(v);
    }
    out
}

/// Greedy digraph packing of `fe_EncodeLexKey` (0x1000fd70).
pub fn pack_digraphs(s: &[u8]) -> Vec<u8> {
    let n = s.len();
    let mut out = Vec::with_capacity(n);
    let mut i = 0usize;
    while i + 1 < n {
        let (mut lo, mut hi) = (0i32, 0x7ei32);
        let mut code: Option<i32> = None;
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            let c = strncmp2(s, i, DIGRAPHS[mid as usize]);
            if c == 0 {
                code = Some(mid);
                break;
            }
            if c < 0 {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        if code.is_none() {
            if strncmp2(s, i, DIGRAPHS[hi as usize]) == 0 {
                code = Some(hi);
            } else if strncmp2(s, i, DIGRAPHS[lo as usize]) == 0 {
                code = Some(lo);
            }
        }
        match code {
            Some(c) => {
                out.push(((c + 1) | 0x80) as u8);
                i += 2;
            }
            None => {
                out.push(s[i]);
                i += 1;
            }
        }
    }
    if i < n {
        out.push(s[i]);
    }
    out
}

pub fn encode_key(word: &[u8]) -> Vec<u8> {
    pack_digraphs(&lower_map(word))
}

fn mix(mut a: u32, mut b: u32, mut c: u32) -> (u32, u32, u32) {
    a = a.wrapping_sub(b).wrapping_sub(c);
    a ^= c >> 13;
    b = b.wrapping_sub(c).wrapping_sub(a);
    b ^= a << 8;
    c = c.wrapping_sub(a).wrapping_sub(b);
    c ^= b >> 13;
    a = a.wrapping_sub(b).wrapping_sub(c);
    a ^= c >> 12;
    b = b.wrapping_sub(c).wrapping_sub(a);
    b ^= a << 16;
    c = c.wrapping_sub(a).wrapping_sub(b);
    c ^= b >> 5;
    a = a.wrapping_sub(b).wrapping_sub(c);
    a ^= c >> 3;
    b = b.wrapping_sub(c).wrapping_sub(a);
    b ^= a << 10;
    c = c.wrapping_sub(a).wrapping_sub(b);
    c ^= b >> 15;
    (a, b, c)
}

/// Bob Jenkins lookup2 `hash()` (`dict_JenkinsLookup2` 0x10010a50).
pub fn lookup2(k: &[u8], initval: u32) -> u32 {
    let le = |p: usize| u32::from_le_bytes([k[p], k[p + 1], k[p + 2], k[p + 3]]);
    let mut a: u32 = 0x9e37_79b9;
    let mut b: u32 = 0x9e37_79b9;
    let mut c: u32 = initval;
    let mut p = 0usize;
    let mut n = k.len();
    while n >= 12 {
        a = a.wrapping_add(le(p));
        b = b.wrapping_add(le(p + 4));
        c = c.wrapping_add(le(p + 8));
        let r = mix(a, b, c);
        a = r.0;
        b = r.1;
        c = r.2;
        p += 12;
        n -= 12;
    }
    c = c.wrapping_add(k.len() as u32);
    let t = &k[p..];
    let g = |i: usize| t[i] as u32;
    if n >= 11 { c = c.wrapping_add(g(10) << 24); }
    if n >= 10 { c = c.wrapping_add(g(9) << 16); }
    if n >= 9 { c = c.wrapping_add(g(8) << 8); }
    if n >= 8 { b = b.wrapping_add(g(7) << 24); }
    if n >= 7 { b = b.wrapping_add(g(6) << 16); }
    if n >= 6 { b = b.wrapping_add(g(5) << 8); }
    if n >= 5 { b = b.wrapping_add(g(4)); }
    if n >= 4 { a = a.wrapping_add(g(3) << 24); }
    if n >= 3 { a = a.wrapping_add(g(2) << 16); }
    if n >= 2 { a = a.wrapping_add(g(1) << 8); }
    if n >= 1 { a = a.wrapping_add(g(0)); }
    mix(a, b, c).2
}

/// A perfect-hashed dictionary (`hashparams_*`, `hashcont_*`, `hashidx_*`, data).
pub struct HashDict {
    seed: u32,
    shift: u32,
    tabsize: u32,
    f4: u32,
    pub nkeys: u32,
    scramble: Vec<u32>,
    tab: Vec<u8>,
    cont: Vec<u8>,
    maxrec: usize,
    offs: Vec<u32>,
    data: Vec<u8>,
}

fn rd32(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes([b[p], b[p + 1], b[p + 2], b[p + 3]])
}

fn bad(msg: String) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, msg)
}

impl HashDict {
    pub fn open(dir: &Path, tag: &str, dictname: &str) -> std::io::Result<HashDict> {
        let hp = std::fs::read(dir.join(format!("hashparams_{}", tag)))?;
        let o = if hp.starts_with(b"NLP") { 3 } else { 0 };
        if hp[o] != 2 {
            return Err(bad(format!("hashparams_{}: type {} unsupported", tag, hp[o])));
        }
        let f = |i: usize| rd32(&hp, o + 1 + 4 * i);
        let (seed, shift, tabsize, f4, nkeys, nscr) = (f(0), f(2), f(3), f(4), f(5), f(6) as usize);
        let mut p = o + 0x1d;
        let scramble = (0..nscr).map(|i| rd32(&hp, p + 4 * i)).collect();
        p += 4 * nscr;
        let ntab = rd32(&hp, p) as usize;
        let tab = hp[p + 4..p + 4 + ntab].to_vec();
        let cont = std::fs::read(dir.join(format!("hashcont_{}", tag)))?;
        let idx = std::fs::read(dir.join(format!("hashidx_{}", tag)))?;
        let maxrec = rd32(&idx, 0) as usize;
        let offs = (0..(idx.len() - 4) / 4).map(|i| rd32(&idx, 4 + 4 * i)).collect();
        let data = std::fs::read(dir.join(dictname))?;
        Ok(HashDict { seed, shift, tabsize, f4, nkeys, scramble, tab, cont, maxrec, offs, data })
    }

    /// `dict_PerfectHashSlot` (type 2).
    pub fn slot(&self, key: &[u8]) -> u32 {
        let val = lookup2(key, self.seed);
        if self.f4 < 2 {
            return 0;
        }
        let t = self.tab[(val & (self.tabsize - 1)) as usize] as u32;
        let sh = val >> (self.shift & 31);
        if self.tabsize < 0x1000 {
            t ^ sh
        } else {
            self.scramble[t as usize] ^ sh
        }
    }

    /// Record `(key, payload)` of slot `s`, reading `maxrec` bytes like the engine.
    pub fn record(&self, s: u32) -> (&[u8], &[u8]) {
        let off = self.offs[s as usize] as usize;
        let end = (off + self.maxrec).min(self.data.len());
        let rec = &self.data[off..end];
        let kn = rec.iter().position(|&c| c == 0).unwrap_or(rec.len());
        let rest = &rec[(kn + 1).min(rec.len())..];
        let pn = rest.iter().position(|&c| c == 0).unwrap_or(rest.len());
        (&rec[..kn], &rest[..pn])
    }

    /// `fe_HashDictLookup`: payload of an encoded key, or None on a miss.
    pub fn get(&self, enc: &[u8]) -> Option<&[u8]> {
        if enc.is_empty() {
            return None;
        }
        let s = self.slot(enc);
        if s >= self.nkeys {
            return None;
        }
        let clen = self.cont[2 * s as usize] as i8 as i32;
        if clen != enc.len() as i32 || self.cont[2 * s as usize + 1] != enc[0] {
            return None;
        }
        let (k, p) = self.record(s);
        if k == enc {
            Some(p)
        } else {
            None
        }
    }
}

/// Decoded lexicon payload (`dict_DecodeLexPayload` 0x10003c50).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LexEntry {
    pub class_char: u8,
    pub b40: i16,
    pub b10: i16,
    pub b20: i16,
    pub b80: i16,
    /// `(pos list, phone codes)` per pronunciation; POS list empty for mode 0x01.
    pub prons: Vec<(Vec<u8>, Vec<u8>)>,
}

pub fn expand_codebook(cb: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for &b in cb {
        out.extend_from_slice(CODEBOOK[b as usize]);
    }
    out
}

pub fn decode_payload(p: &[u8]) -> LexEntry {
    let mut e = LexEntry::default();
    let Some(&f) = p.first() else {
        return e;
    };
    if f == 0 {
        return e;
    }
    if f & 4 != 0 {
        e.class_char = b'E';
    }
    if f & 8 != 0 {
        e.class_char = b'A';
    }
    e.b40 = (f & 0x40 != 0) as i16;
    e.b10 = (f & 0x10 != 0) as i16;
    e.b20 = (f & 0x20 != 0) as i16;
    e.b80 = (f & 0x80 != 0) as i16;
    if f & 1 != 0 {
        e.prons.push((Vec::new(), expand_codebook(&p[1..])));
    } else if f & 2 != 0 {
        for seg in p[1..].split(|&c| c == 0xff) {
            if seg.is_empty() {
                continue;
            }
            let bar = seg.iter().position(|&c| c == b'|').unwrap_or(seg.len());
            let pos: Vec<u8> = seg[..bar].iter().map(|&c| c.wrapping_sub(1)).collect();
            let cb = if bar < seg.len() { &seg[bar + 1..] } else { &[][..] };
            e.prons.push((pos, expand_codebook(cb)));
        }
    }
    e
}

/// A `.txt2` table (`fe_LoadTxt2Table` 0x100033e0 / reader `FUN_10010300`,
/// tokenizer `FUN_10003540`). `M` tables alternate key and value tokens, `L`
/// tables use the key as its own value.
pub struct Txt2 {
    pub ci: bool,
    pub keys: Vec<Vec<u8>>,
    pub vals: Vec<Vec<u8>>,
}

impl Txt2 {
    pub fn open(path: &Path, map: bool, ci: bool) -> std::io::Result<Txt2> {
        let raw = std::fs::read(path)?;
        let n = raw.len();
        let (body, count): (Vec<u8>, usize) = if n >= 17 && raw[..3] == raw[n - 3..] {
            let shift = raw[n - 14];
            let body: Vec<u8> = raw[3..n - 14].iter().map(|&b| b.wrapping_sub(shift)).collect();
            let cnt: Vec<u8> = raw[n - 13..n - 3].iter().map(|&b| b.wrapping_sub(shift)).collect();
            let s: String = cnt.iter().take_while(|&&c| c.is_ascii_digit()).map(|&c| c as char).collect();
            (body, s.parse().unwrap_or(0))
        } else {
            (raw.clone(), 0)
        };
        let mut toks = Tokens { b: &body, p: 0 };
        let mut t = Txt2 { ci, keys: Vec::new(), vals: Vec::new() };
        for _ in 0..count {
            let Some(k) = toks.next() else { break };
            if map {
                let Some(v) = toks.next() else { break };
                t.keys.push(k);
                t.vals.push(v);
            } else {
                t.vals.push(k.clone());
                t.keys.push(k);
            }
        }
        Ok(t)
    }

    /// `FUN_100035d0`: binary search, case-insensitive for 'I' tables.
    pub fn find(&self, key: &[u8]) -> i32 {
        if self.keys.is_empty() {
            return -1;
        }
        let (mut lo, mut hi) = (0i32, self.keys.len() as i32 - 1);
        let mut mid = hi;
        while lo <= hi {
            mid /= 2;
            let c = if self.ci { stricmp(&self.keys[mid as usize], key) } else { strcmp(&self.keys[mid as usize], key) };
            if c == 0 {
                return mid;
            }
            if c < 0 {
                lo = mid + 1;
            } else {
                hi = mid - 1;
            }
            mid = hi + lo;
        }
        -1
    }
}

struct Tokens<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Tokens<'a> {
    fn next(&mut self) -> Option<Vec<u8>> {
        use crate::ling::tables::CHAR_CLASS;
        while self.p < self.b.len() {
            let c = self.b[self.p];
            if CHAR_CLASS[c as usize] & 6 == 0 && c != 0 {
                break;
            }
            self.p += 1;
        }
        if self.p >= self.b.len() {
            return None;
        }
        let st = self.p;
        while self.p < self.b.len() && !matches!(self.b[self.p], b'|' | 0 | b'\n' | b'\r') {
            self.p += 1;
        }
        let mut end = self.p;
        while end > st + 1 && matches!(self.b[end - 1], b'\t' | b' ') {
            end -= 1;
        }
        self.p += 1;
        Some(self.b[st..end].to_vec())
    }
}

/// `exceptdict` (`fe_LoadExceptDict` 0x10003660): multi-part words keyed by
/// the encoded spelling of the parts joined with '-', grouped by part count.
pub struct ExceptDict {
    pub lo: i32,
    pub hi: i32,
    /// `groups[g - lo]`: `(key, phone codes)` sorted by key bytes.
    pub groups: Vec<Vec<(Vec<u8>, Vec<u8>)>>,
}

impl ExceptDict {
    pub fn open(path: &Path) -> std::io::Result<ExceptDict> {
        let b = std::fs::read(path)?;
        let lo = rd32(&b, 0) as i32;
        let hi = rd32(&b, 4) as i32;
        let mut p = 8usize;
        let mut groups = Vec::new();
        for _ in lo..=hi {
            let n = rd32(&b, p + 4) as usize;
            p += 8;
            let mut g = Vec::with_capacity(n);
            for _ in 0..n {
                let kl = rd32(&b, p) as usize;
                let vl = rd32(&b, p + 4) as usize;
                p += 8;
                let k = cstr_of(&b[p..p + kl]);
                let v = cstr_of(&b[p + kl..p + kl + vl]);
                g.push((k, v));
                p += kl + vl;
            }
            groups.push(g);
        }
        Ok(ExceptDict { lo, hi, groups })
    }

    /// `dict_ExceptDictFind` (0x10003b70).
    pub fn find(&self, word: &[u8], parts: i32) -> Option<&[u8]> {
        if parts < self.lo || parts > self.hi {
            return None;
        }
        let g = &self.groups[(parts - self.lo) as usize];
        if g.is_empty() {
            return None;
        }
        let key = encode_key(word);
        let (mut lo, mut hi) = (0i32, g.len() as i32 - 1);
        while lo <= hi {
            let mid = (lo + hi) / 2;
            let c = strcmp(&g[mid as usize].0, &key);
            if c == 0 {
                return Some(&g[mid as usize].1);
            }
            if c < 1 {
                lo = mid + 1;
            } else {
                hi = mid - 1;
            }
        }
        None
    }
}

fn cstr_of(b: &[u8]) -> Vec<u8> {
    b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())].to_vec()
}

/// All dictionaries the linguistic front end reads.
pub struct Lexicon {
    pub emb: HashDict,
    pub tpp: HashDict,
    /// `chc_sort.txt2`: consonant clusters -> 4 flag digits (DAT_100fee04).
    pub chc: Txt2,
    /// `wab.txt2`: function words (DAT_100fee08).
    pub wab: Txt2,
    /// `atmt.tree3`: 27 letter-to-sound trees, A..Z and apostrophe.
    pub atmt: Vec<crate::tree3::Tree>,
    /// `poly.tree3`: homograph POS class tree (DAT_100ff100).
    pub poly: crate::tree3::Tree,
    /// `engbi.tree3`: break-index tree.
    pub engbi: crate::tree3::Tree,
    pub exc: ExceptDict,
    pub era: Era,
}

impl Lexicon {
    /// `dict_eng` = `data-common/dict-eng`.
    pub fn open(dict_eng: &Path) -> std::io::Result<Lexicon> {
        Lexicon::open_era(dict_eng, Era::V2014)
    }

    /// The Paul09 trees are `.tree2` (`tree2::load`); every other build reads `.tree3`.
    pub fn open_era(dict_eng: &Path, era: Era) -> std::io::Result<Lexicon> {
        let tree = |name: &str| -> std::io::Result<crate::tree3::Tree> {
            if era == Era::Paul09 {
                crate::tree2::load(&dict_eng.join(format!("{}.tree2", name)), false)
            } else {
                crate::tree3::Tree::load(&dict_eng.join(format!("{}.tree3", name)))
            }
        };
        let atmt = if era == Era::Paul09 {
            crate::tree2::load_multi(&dict_eng.join("atmt.tree2"), 27)?
        } else {
            let atmt_raw = std::fs::read(dict_eng.join("atmt.tree3"))?;
            let n = u32::from_le_bytes(atmt_raw[..4].try_into().unwrap()) as usize;
            let mut atmt = Vec::with_capacity(n);
            let mut p = 4usize;
            for _ in 0..n {
                let (t, used) = crate::tree3::Tree::parse(&atmt_raw[p..]).ok_or_else(|| bad("atmt.tree3".into()))?;
                atmt.push(t);
                p += used;
            }
            atmt
        };
        Ok(Lexicon {
            emb: HashDict::open(dict_eng, "emb", "engttsdict_emb")?,
            tpp: HashDict::open(dict_eng, "eng_tpp", "tppdict_eng")?,
            chc: Txt2::open(&dict_eng.join("chc_sort.txt2"), true, true)?,
            wab: Txt2::open(&dict_eng.join("wab.txt2"), false, true)?,
            atmt,
            poly: tree("poly")?,
            engbi: tree("engbi")?,
            exc: ExceptDict::open(&dict_eng.join("exceptdict"))?,
            era,
        })
    }

    /// `dict_LookupLexicon` (mode 0): raw payload bytes of a spelling.
    pub fn lookup_payload(&self, word: &[u8]) -> Option<&[u8]> {
        self.emb.get(&encode_key(word))
    }
}
