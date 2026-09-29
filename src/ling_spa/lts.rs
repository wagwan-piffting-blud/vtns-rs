//! Violeta letter-to-sound and lexicon-pron syllabification, translated from
//! vt_spa_violeta16.dll:
//!
//! * `stage1` = `FUN_10022810`: casefold, collapse doubled consonants, classify
//!   letters into a vowel/consonant pattern, insert syllable breaks, pick the
//!   stressed syllable.
//! * `stage2` = `FUN_10023930`: letters + pattern -> phone codes.
//! * `degeminate` = `FUN_100250e0`: merge doubled consonants and a few
//!   vowel+UH0 / T+CH sequences.
//! * `syllabify_pron` = `FUN_10025700` + `FUN_10022430` + `FUN_10025820`:
//!   re-inserts syllable breaks into a lexicon pron via its phone names.
//!
//! The working struct is kept as the engine's byte layout (`Work`), since
//! stage 2 reads the stress list past its end into the neighbouring fields.

use super::lexicon::{encode_key_spa, stricmp_spa};
use super::lexicon::TOLOWER_SPA;

/// Syllable break in a phone string.
pub const SYL: u8 = 99;

/// Phone names, code = index + 1 (`PTR_DAT_10067e58`).
pub const NAMES: [&[u8]; 0x3f] = [
    b"AA0", b"AA1", b"AO0", b"AO1", b"AU0", b"AU1", b"AY0", b"AY1", b"B", b"BH", b"CH", b"D", b"DH", b"EH0", b"EH1", b"EU0",
    b"EU1", b"EY0", b"EY1", b"F", b"G", b"HH", b"IH0", b"IH1", b"JH", b"K", b"L", b"M", b"N", b"NG", b"NY", b"OH0", b"OH1",
    b"OU0", b"OU1", b"OY0", b"OY1", b"P", b"R", b"RR", b"S", b"SH", b"T", b"UH0", b"UH1", b"V", b"WA0", b"WA1", b"WE0",
    b"WE1", b"WI0", b"WI1", b"WO0", b"WO1", b"YA0", b"YA1", b"YE0", b"YE1", b"YO0", b"YO1", b"YU0", b"YU1", b"Y",
];

/// Monosyllables left unstressed (`PTR_DAT_1006866c`, sorted).
pub const UNSTRESSED: [&[u8]; 10] = [b"aun", b"de", b"el", b"mas", b"mi", b"que", b"se", b"si", b"te", b"tu"];

/// Words ending in "mente" that are not adverbs (`PTR_s_abastimente_10068304`, sorted).
pub const NOT_MENTE: [&[u8]; 67] = [
    b"abastimente", b"abgmente", b"abmente", b"adormente", b"alimente", b"anutrimente", b"argumente", b"atormente",
    b"augmente", b"aumente", b"bastimente", b"cemente", b"chimente", b"cimente", b"clemente", b"comente", b"comente ",
    b"compartimente", b"complemente", b"condimente", b"cumplimente ", b"decremente", b"demente", b"desaumente",
    b"descaimente", b"descimente", b"desfragmente", b"deslimente", b"desreglamente", b"documente", b"electropulimente",
    b"elemente", b"emparamente", b"encemente", b"esbatimente", b"escarmente", b"excremente", b"experimente", b"fermente",
    b"fomente", b"fragmente", b"fundamente", b"implemente", b"incremente", b"instrumente", b"juramente", b"lamente",
    b"medicamente", b"mente", b"ornamente", b"paramente", b"parlamente", b"pavimente", b"pigmente", b"pulimente",
    b"regimente", b"reglamente", b"retroalimente", b"sacramente", b"salpimente", b"sarmente", b"sedimente", b"segmente",
    b"sobrealimente", b"subalimente", b"suplemente", b"tormente",
];

/// `FUN_10043680` with `FUN_100070b0` (case-insensitive) as the comparator.
pub fn bsearch_ci(list: &[&[u8]], key: &[u8]) -> i32 {
    if key.first().copied().unwrap_or(0) == 0 {
        return -1;
    }
    let (mut lo, mut hi) = (0i32, list.len() as i32 - 1);
    let mut mid = hi;
    while lo <= hi {
        mid /= 2;
        let c = stricmp_spa(list[mid as usize], key);
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

fn in_set(set: &[u8], c: u8) -> bool {
    c != 0 && set.contains(&c)
}

fn vowel_pat(c: u8) -> bool {
    in_set(b"TDSWw", c)
}

fn strong(c: u8) -> bool {
    matches!(c, b'a' | 0xe1 | b'e' | 0xe9 | b'o' | 0xf3)
}

fn weak(c: u8) -> bool {
    matches!(c, b'i' | b'u' | 0xfc)
}

fn accented(c: u8) -> bool {
    matches!(c, 0xc1 | 0xe1 | 0xc9 | 0xe9 | 0xd3 | 0xf3 | 0xda | 0xfa | 0xcd | 0xed)
}

fn vowel_class(c: u8) -> bool {
    matches!(c, b'a' | 0xe1 | b'e' | 0xe9 | b'o' | 0xf3 | b'i' | b'u' | 0xfc | 0xed | 0xfa)
}

fn consonant(c: u8) -> bool {
    matches!(c, b'b'..=b'd' | b'f'..=b'h' | b'j'..=b'n' | b'p'..=b't' | b'v'..=b'z' | 0xd1 | 0xf1)
}

fn front(c: u8) -> bool {
    matches!(c, b'e' | 0xe9 | b'i' | 0xed | b'y')
}

const W_NSYL: usize = 0x00;
const W_FLAG_Y: usize = 0x3e;
const W_LEN: usize = 0x40;
const W_NVG: usize = 0x44;
const W_LETTERS: usize = 0x48;
const W_PHONES: usize = 0x84;
const W_PATTERN: usize = 0xe8;

/// `FUN_10022810`'s output struct (caller's `local_6a0`).
pub struct Work {
    pub b: Vec<u8>,
}

impl Work {
    fn new() -> Work {
        Work { b: vec![0u8; 0x400] }
    }

    fn i16(&self, o: usize) -> i16 {
        i16::from_le_bytes([self.b[o], self.b[o + 1]])
    }

    fn set_i16(&mut self, o: usize, v: i16) {
        self.b[o..o + 2].copy_from_slice(&v.to_le_bytes());
    }

    fn i32(&self, o: usize) -> i32 {
        i32::from_le_bytes([self.b[o], self.b[o + 1], self.b[o + 2], self.b[o + 3]])
    }

    fn set_i32(&mut self, o: usize, v: i32) {
        self.b[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn cstr(&self, o: usize) -> &[u8] {
        let s = &self.b[o..];
        &s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())]
    }

    fn put(&mut self, o: usize, s: &[u8]) {
        self.b[o..o + s.len()].copy_from_slice(s);
        self.b[o + s.len()] = 0;
    }

    /// Stressed-syllable list entry `k` (1-based; slot 0 is the syllable count).
    fn stress(&self, k: usize) -> i16 {
        self.i16(2 * k)
    }

    fn letter(&self, i: i32) -> u8 {
        self.b[(W_LETTERS as i32 + i) as usize]
    }

    fn pat(&self, p: i32) -> u8 {
        self.b[(W_PATTERN as i32 + p) as usize]
    }

    /// Phone string written by `stage2` (`+0x84`).
    pub fn phones(&self) -> Vec<u8> {
        self.cstr(W_PHONES).to_vec()
    }
}

/// `FUN_10022810(work, word, len, w_foreign)`. `None` for an empty word (the
/// engine's -1).
pub fn stage1(word: &[u8], len: i32, w_foreign: bool) -> Option<Work> {
    let mut w = Work::new();
    w.set_i16(W_NSYL, 1);
    if word.first().copied().unwrap_or(0) == 0 {
        return None;
    }
    let mut w98 = encode_key_spa(word);
    let at = |s: &[u8], k: i32| -> u8 {
        if k < 0 {
            0
        } else {
            s.get(k as usize).copied().unwrap_or(0)
        }
    };

    let mut n = len;
    let mut w1: Vec<u8> = Vec::new();
    if n > 0 {
        let mut i = 0i32;
        loop {
            let b = at(&w98, i);
            if b == at(&w98, i + 1) && matches!(b, b'm' | b's' | b't' | b'b' | b'd' | b'f' | b'g' | b'k' | b'p') {
                i += 1;
            }
            w1.push(b);
            i += 1;
            if i >= n {
                break;
            }
        }
    }
    if n != w1.len() as i32 {
        n = w1.len() as i32;
        w98 = w1.clone();
    }

    let mut pat: Vec<u8> = Vec::new();
    let mut acc: Vec<i32> = Vec::new();
    let mut cons: u8 = 0;
    let mut nvg = 0i32;
    let mut flag_y = 0i16;
    let accent_after = |acc: &mut Vec<i32>, i: i32, c: u8| {
        if (acc.is_empty() || *acc.last().unwrap() != i) && accented(c) {
            acc.push(i);
        }
    };
    let mut i = 0i32;
    while i < n {
        let b0 = at(&w1, i);
        let b1 = at(&w1, i + 1);
        let b2 = at(&w1, i + 2);
        if accented(b0) {
            acc.push(i);
        }
        let mut next = i + 2;
        let w1_ = matches!(b0, b'i' | b'u' | 0xfc | b'w');
        let tri = (next == n && w1_ && strong(b1) && weak(b2))
            || (next < n && w1_ && strong(b1) && weak(b2) && !vowel_class(TOLOWER_SPA[at(&w1, i + 3) as usize]));
        if tri || (next == n && w1_ && strong(b1) && b2 == b'y') {
            pat.push(b'T');
            accent_after(&mut acc, i, b1);
            cons = 0;
            nvg += 1;
            next = i + 3;
        } else if i + 1 < n
            && ((strong(b0) && weak(b1))
                || (weak(b0) && strong(b1))
                || (b0 == b'i' && b1 == 0xfa)
                || (b0 == b'u' && b1 == 0xed)
                || (b0 == 0xfc && b1 == 0xed)
                || (b0 == b'i' && b1 == b'u')
                || (b0 == b'u' && b1 == b'i')
                || (b0 == 0xfc && b1 == b'i'))
        {
            pat.push(b'D');
            accent_after(&mut acc, i, b1);
            cons = 0;
            nvg += 1;
        } else if next == n && matches!(b0, b'a' | 0xe1 | b'e' | 0xe9 | b'o' | 0xf3 | b'u' | 0xfa | 0xfc) && b1 == b'y' {
            pat.push(b'D');
            flag_y = 1;
            nvg += 1;
            cons = 0;
        } else if next < n && matches!(b0, b'a' | 0xe1 | b'e' | 0xe9 | b'o' | 0xf3 | b'u' | 0xfa | 0xfc) && b1 == b'y' && consonant(b2) {
            pat.push(b'D');
            nvg += 1;
            cons = 0;
        } else if nvg == 0
            && i + 1 < n
            && b0 == b'y'
            && matches!(b1, b'a' | 0xe1 | b'e' | 0xe9 | b'o' | 0xf3 | b'u' | 0xfa)
            && !pat.is_empty()
            && !vowel_pat(*pat.last().unwrap())
        {
            pat.push(b'D');
            accent_after(&mut acc, i, b1);
            cons = 0;
            nvg += 1;
        } else if !w_foreign && i + 1 < n && b0 == b'w' && matches!(b1, b'a' | 0xe1 | b'e' | 0xe9 | b'o' | 0xf3 | b'i' | 0xed | b'y') {
            pat.push(b'D');
            accent_after(&mut acc, i, b1);
            cons = 0;
            nvg += 1;
        } else if vowel_class(TOLOWER_SPA[b0 as usize]) {
            pat.push(if strong(b0) {
                b'S'
            } else if b0 == 0xed || b0 == 0xfa {
                b'W'
            } else {
                b'w'
            });
            nvg += 1;
            cons = 0;
            next = i + 1;
        } else if i + 1 < n
            && ((matches!(b0, b'b' | b'c' | b'f' | b'g' | b'p') && matches!(b1, b'l' | b'r'))
                || (matches!(b0, b'd' | b't') && b1 == b'r')
                || matches!((b0, b1), (b'c', b'h') | (b'l', b'l') | (b'r', b'r') | (b'g', b'u') | (b'z', b'z') | (b't', b'x') | (b'q', b'u')))
        {
            if matches!(b0, b'g' | b'q') && b1 == b'u' && (next >= n || !matches!(b2, b'e' | 0xe9 | b'i' | 0xed)) {
                pat.push(cons.wrapping_add(b'1'));
                next = i + 1;
            } else {
                pat.push(b'B');
            }
            cons = cons.wrapping_add(1);
        } else {
            if !consonant(b0) {
                break;
            }
            pat.push(cons.wrapping_add(b'1'));
            cons = cons.wrapping_add(1);
            next = i + 1;
        }
        i = next;
    }

    let np = pat.len();
    let mut nsyl: i16 = 1;
    let mut pat2: Vec<u8> = Vec::new();
    for i in 0..np {
        let next = if i + 1 < np { pat[i + 1] } else { 0 };
        let more = i + 1 < np;
        let cur = pat[i];
        if i >= 1 {
            let p1 = pat[i - 1];
            if vowel_pat(p1) && vowel_pat(cur) {
                pat2.extend_from_slice(&[b'-', cur]);
                nsyl += 1;
                continue;
            }
            if more && (cur == b'1' || cur == b'B') && vowel_pat(p1) && vowel_pat(next) {
                pat2.extend_from_slice(&[b'-', cur]);
                nsyl += 1;
                continue;
            }
        }
        if i >= 2 && more {
            let (p2, p1) = (pat[i - 2], pat[i - 1]);
            if vowel_pat(p2) && (p1 == b'1' || p1 == b'B') && cur == b'2' && vowel_pat(next) {
                pat2.extend_from_slice(b"-2");
                nsyl += 1;
                continue;
            }
            if p1 == b'1' && cur == b'B' && vowel_pat(next) {
                pat2.extend_from_slice(b"-B");
                nsyl += 1;
                continue;
            }
        }
        if i > 2 && more && pat[i - 2] == b'1' && pat[i - 1] == b'2' {
            if cur == b'3' && vowel_pat(next) {
                pat2.extend_from_slice(b"-3");
                nsyl += 1;
                continue;
            }
            if cur == b'B' && vowel_pat(next) {
                pat2.extend_from_slice(b"-B");
                nsyl += 1;
                continue;
            }
        }
        if i >= 4 && more && pat[i - 3] == b'1' && pat[i - 2] == b'2' && pat[i - 1] == b'3' && cur == b'B' && vowel_pat(next) {
            pat2.extend_from_slice(b"-B");
            nsyl += 1;
        } else {
            pat2.push(cur);
        }
    }
    w.set_i16(W_NSYL, nsyl);
    w.set_i16(W_FLAG_Y, flag_y);
    w.set_i32(W_NVG, nvg);
    w.put(W_PATTERN, &pat2);

    let last_letter = at(&w98, n - 1);
    let m = pat2.len() as i32;
    let pat2_at = |k: i32| at(&pat2, k);
    if nsyl == 1 {
        if nvg < 2 {
            if bsearch_ci(&UNSTRESSED, &w98) == -1 {
                w.set_i16(2, 1);
            }
        } else {
            w.set_i16(W_NSYL, 0);
            let last = pat2_at(m - 1);
            let target = if (m < 1 || !in_set(b"TDSw", last))
                && (m < 2 || !in_set(b"TDSw", pat2_at(m - 2)) || last != b'1' || !matches!(last_letter, b's' | b'n'))
            {
                1
            } else {
                2
            };
            let mut cnt = 1;
            let mut k = m;
            while k - 1 >= 0 {
                let c = pat2_at(k - 1);
                k -= 1;
                if in_set(b"TDSw", c) {
                    if cnt == target {
                        w.set_i16(2, k as i16);
                        break;
                    }
                    cnt += 1;
                }
            }
        }
        w.set_i32(W_LEN, n);
        w.put(W_LETTERS, &w98);
        return Some(w);
    }

    w.set_i16(W_NSYL, 1);
    let mut letters: Vec<u8> = Vec::new();
    let mut slot = 1usize;
    let mut ai = 0usize;
    let mut li = 0i32;
    let mut pi = 0i32;
    let mut last_dash = 0i32;
    if n > 0 {
        loop {
            if pi == 0 || pat2_at(pi) != b'-' {
                let a = acc.get(ai).copied().unwrap_or(-1);
                if w.stress(slot) == 0 && a >= 0 && a <= li {
                    let s = w.i16(W_NSYL);
                    w.set_i16(2 * slot, s);
                    slot += 1;
                    ai += 1;
                }
                let k = match pat2_at(pi) {
                    b'T' => 3,
                    b'D' | b'B' => 2,
                    _ => 1,
                };
                for j in 0..k {
                    letters.push(at(&w98, li + j));
                }
                li += k;
            } else {
                letters.push(b'-');
                let s = w.i16(W_NSYL);
                w.set_i16(W_NSYL, s + 1);
                last_dash = pi;
            }
            pi += 1;
            if li >= n {
                break;
            }
        }
    }
    let nl = letters.len();
    w.b[W_LETTERS..W_LETTERS + nl].copy_from_slice(&letters);
    w.b[W_LETTERS + nl] = 0;
    w.set_i32(W_LEN, nl as i32);
    if w.stress(1) == 0 {
        let nsyl = w.i16(W_NSYL);
        let last = pat2_at(pi - 1);
        let penult = (pi > 0 && (in_set(b"TSw", last) || (last == b'D' && flag_y == 0)))
            || (pi > 1 && in_set(b"TDSw", pat2_at(pi - 2)) && last == b'1' && matches!(last_letter, b's' | b'n'))
            || (pi > 0 && !in_set(b"TDSw", last) && {
                let tail = &pat2[(last_dash as usize).min(pat2.len())..];
                tail.iter().any(|&c| matches!(c, b'T' | b'D' | b'S' | b'w')) && matches!(last_letter, b's' | b'n')
            });
        let s = if penult && nsyl >= 2 { nsyl - 1 } else { nsyl };
        w.set_i16(2, s);
    }
    Some(w)
}

/// `FUN_10023930(work, mnemonic, mexican, w_foreign)`: writes the phone string
/// into `work` (`Work::phones`). `None` on a letter/pattern it has no rule for.
pub fn stage2(w: &mut Work, mexican: bool, w_foreign: bool) -> Option<()> {
    let len = w.i32(W_LEN);
    let patlen = w.cstr(W_PATTERN).len() as i32;
    let nsyl0 = w.i16(W_NSYL);
    let mut out: Vec<u8> = Vec::new();
    let mut i = 0i32;
    let mut p = 0i32;
    let mut syl: i16 = 1;
    let mut skip = false;
    let mut si = 0usize;
    let l = |w: &Work, k: i32| if k < 0 { 0 } else { w.letter(k) };
    let consonant_pat = |w: &Work, k: i32| !vowel_pat(w.pat(k));
    if len > 0 {
        loop {
            if patlen <= p {
                break;
            }
            let stressed = |w: &Work, si: usize, p: i32, syl: i16| {
                let s = w.stress(si + 1);
                s == syl || (nsyl0 == 0 && s as i32 == p)
            };
            if p == 0 || w.pat(p) != b'-' {
                let pc = w.pat(p);
                if !vowel_pat(pc) {
                    let c = l(w, i);
                    let nx = l(w, i + 1);
                    let digraph = !skip
                        && pc == b'B'
                        && i + 1 < len
                        && matches!((c, nx), (b'c', b'h') | (b'l', b'l') | (b'r', b'r') | (b'g', b'u') | (b'z', b'z') | (b't', b'x') | (b'q', b'u'));
                    if digraph {
                        i += 1;
                        match (c, nx) {
                            (b'c', b'h') | (b't', b'x') => out.push(0x0b),
                            (b'g', b'u') => out.push(0x15),
                            (b'l', b'l') => {
                                if p + 1 < patlen && vowel_pat(w.pat(p + 1)) {
                                    out.push(0x3f);
                                } else {
                                    out.extend_from_slice(&[0x1b, 0x1b]);
                                }
                            }
                            (b'q', b'u') => out.push(0x1a),
                            (b'r', b'r') => out.push(0x28),
                            _ => out.extend_from_slice(&[0x2b, 0x29]),
                        }
                    } else {
                        let mnn = |x: u8| matches!(x, b'm' | b'n' | 0xf1);
                        let bv = |w: &Work| i == 0 || (i >= 1 && mnn(l(w, i - 1))) || (i >= 2 && mnn(l(w, i - 2)) && l(w, i - 1) == b'-');
                        let mut consume = false;
                        let ph: Option<Vec<u8>> = match c {
                            b'b' | b'v' => Some(vec![if bv(w) { 9 } else { 10 }]),
                            b'c' => Some(vec![if i + 1 < len && front(nx) { 0x29 } else { 0x1a }]),
                            b'd' => {
                                let mnl = |x: u8| matches!(x, b'm' | b'n' | 0xf1 | b'l');
                                let hard = i == 0 || (i > 0 && mnl(l(w, i - 1))) || (i > 1 && mnl(l(w, i - 2)) && l(w, i - 1) == b'-');
                                Some(vec![if hard { 0x0c } else { 0x0d }])
                            }
                            b'f' => Some(vec![0x14]),
                            b'g' => {
                                if i + 1 < len && front(nx) {
                                    Some(vec![0x16])
                                } else if i == 0 && l(w, 1) == b'n' {
                                    None
                                } else {
                                    Some(vec![0x15])
                                }
                            }
                            b'h' => None,
                            b'j' => Some(vec![0x16]),
                            b'k' | b'q' => Some(vec![0x1a]),
                            b'l' => Some(vec![0x1b]),
                            b'm' => {
                                if i == 0 && l(w, 1) == b'n' {
                                    None
                                } else {
                                    Some(vec![0x1c])
                                }
                            }
                            b'n' => {
                                let two_cons = p + 2 == patlen || (p + 2 < patlen && consonant_pat(w, p + 1) && consonant_pat(w, p + 2));
                                if i == 0 {
                                    Some(vec![0x1d])
                                } else if nx == b'g' && two_cons {
                                    consume = true;
                                    Some(vec![0x1e])
                                } else {
                                    let kgq = |x: u8| matches!(x, b'k' | b'g' | b'q');
                                    let x = nx == b'x' && two_cons;
                                    let g1 = len <= i + 1 || !kgq(nx);
                                    let g2 = i + 2 != len || nx != b'c';
                                    let g3 = len <= i + 2
                                        || ((nx != b'-' || !kgq(l(w, i + 2))) && (nx != b'c' || l(w, i + 2) == b'h' || front(l(w, i + 2))));
                                    let g4 = len <= i + 3 || nx != b'-' || l(w, i + 2) != b'c' || l(w, i + 3) == b'h' || front(l(w, i + 3));
                                    Some(vec![if !x && g1 && g2 && g3 && g4 { 0x1d } else { 0x1e }])
                                }
                            }
                            0xf1 => Some(vec![if len <= i + 1 || !vowel_class(TOLOWER_SPA[nx as usize]) { 0x1d } else { 0x1f }]),
                            b'p' => {
                                if i == 0 && l(w, 1) == b's' {
                                    None
                                } else {
                                    Some(vec![0x26])
                                }
                            }
                            b'r' => {
                                let lns = |x: u8| matches!(x, b'l' | b'n' | b's');
                                let trill = i == 0 || (i > 0 && lns(l(w, i - 1))) || (i > 1 && l(w, i - 1) == b'-' && lns(l(w, i - 2)));
                                Some(vec![if trill { 0x28 } else { 0x27 }])
                            }
                            b's' => {
                                if i + 2 <= len && nx == b'h' {
                                    consume = true;
                                    Some(vec![0x2a])
                                } else {
                                    Some(vec![0x29])
                                }
                            }
                            b't' => Some(vec![0x2b]),
                            b'w' => Some(vec![if w_foreign { 10 } else { 0x2c }]),
                            b'x' => {
                                if mexican {
                                    Some(vec![0x16])
                                } else if i == 0 {
                                    Some(vec![0x29])
                                } else {
                                    Some(vec![0x1a, 0x29])
                                }
                            }
                            b'y' => {
                                if len == 1 {
                                    Some(vec![0x18])
                                } else if i + 1 == len || patlen <= p + 1 || !vowel_pat(w.pat(p + 1)) {
                                    Some(vec![0x17])
                                } else {
                                    Some(vec![0x3f])
                                }
                            }
                            b'z' => Some(vec![0x29]),
                            _ => return None,
                        };
                        if let Some(ph) = ph {
                            out.extend_from_slice(&ph);
                            if consume {
                                i += 1;
                                p += 1;
                            }
                            skip = false;
                            let c2 = l(w, i);
                            let n2 = l(w, i + 1);
                            if w.pat(p) == b'B'
                                && i + 1 < len
                                && ((matches!(c2, b'b' | b'c' | b'f' | b'g' | b'p') && matches!(n2, b'l' | b'r')) || (matches!(c2, b'd' | b't') && n2 == b'r'))
                            {
                                skip = true;
                                p -= 1;
                            }
                        }
                    }
                } else {
                    let c = l(w, i);
                    let d = l(w, i + 1);
                    let st = stressed(w, si, p, syl);
                    let pick = |s: u8, u: u8| if st { s } else { u };
                    let a = |x: u8| matches!(x, b'a' | 0xe1);
                    let e = |x: u8| matches!(x, b'e' | 0xe9);
                    let o = |x: u8| matches!(x, b'o' | 0xf3);
                    let iy = |x: u8| matches!(x, b'i' | b'y');
                    let uu = |x: u8| matches!(x, b'u' | 0xfc);
                    match pc {
                        b'T' => {
                            let v = if c == b'i' {
                                if a(d) {
                                    pick(0x38, 0x37)
                                } else if e(d) {
                                    pick(0x3a, 0x39)
                                } else if o(d) {
                                    pick(0x3c, 0x3b)
                                } else {
                                    return None;
                                }
                            } else if matches!(c, b'u' | 0xfc | b'w') {
                                if a(d) {
                                    pick(0x30, 0x2f)
                                } else if e(d) {
                                    pick(0x32, 0x31)
                                } else if o(d) {
                                    pick(0x36, 0x35)
                                } else {
                                    return None;
                                }
                            } else {
                                return None;
                            };
                            if st {
                                si += 1;
                            }
                            let g = l(w, i + 2);
                            let glide = if iy(g) {
                                0x17
                            } else if uu(g) {
                                0x2c
                            } else {
                                return None;
                            };
                            out.extend_from_slice(&[v, glide]);
                            i += 2;
                        }
                        b'D' => {
                            let v = if a(c) {
                                if iy(d) {
                                    pick(8, 7)
                                } else if uu(d) {
                                    pick(6, 5)
                                } else {
                                    return None;
                                }
                            } else if e(c) {
                                if iy(d) {
                                    pick(0x13, 0x12)
                                } else if uu(d) {
                                    pick(0x11, 0x10)
                                } else {
                                    return None;
                                }
                            } else if o(c) {
                                if iy(d) {
                                    pick(0x25, 0x24)
                                } else if uu(d) {
                                    pick(0x23, 0x22)
                                } else {
                                    return None;
                                }
                            } else if iy(c) {
                                if a(d) {
                                    pick(0x38, 0x37)
                                } else if e(d) {
                                    pick(0x3a, 0x39)
                                } else if o(d) {
                                    pick(0x3c, 0x3b)
                                } else if matches!(d, b'u' | 0xfa | 0xfc) {
                                    pick(0x3e, 0x3d)
                                } else {
                                    return None;
                                }
                            } else if matches!(c, b'u' | 0xfc | b'w') {
                                if a(d) {
                                    pick(0x30, 0x2f)
                                } else if e(d) {
                                    pick(0x32, 0x31)
                                } else if o(d) {
                                    pick(0x36, 0x35)
                                } else if matches!(d, b'i' | 0xed | b'y') {
                                    pick(0x34, 0x33)
                                } else {
                                    return None;
                                }
                            } else {
                                return None;
                            };
                            if st {
                                si += 1;
                            }
                            out.push(v);
                            i += 1;
                        }
                        b'S' => {
                            let v = if a(c) {
                                pick(2, 1)
                            } else if e(c) {
                                pick(0x0f, 0x0e)
                            } else if o(c) {
                                pick(0x21, 0x20)
                            } else {
                                return None;
                            };
                            if st {
                                si += 1;
                            }
                            out.push(v);
                        }
                        b'W' => {
                            if !matches!(c, 0xed | 0xfa) || w.stress(si + 1) != syl {
                                return None;
                            }
                            si += 1;
                            out.push(if c == 0xed { 0x18 } else { 0x2d });
                        }
                        _ => {
                            if c == b'i' {
                                if st {
                                    si += 1;
                                    out.push(0x18);
                                } else {
                                    out.push(0x17);
                                }
                            } else if uu(c) {
                                if st {
                                    si += 1;
                                    out.push(0x2d);
                                } else {
                                    out.push(0x2c);
                                }
                            } else {
                                return None;
                            }
                        }
                    }
                }
            } else {
                out.push(SYL);
                syl += 1;
            }
            p += 1;
            i += 1;
            if i >= len {
                break;
            }
        }
    }
    let n = out.len();
    w.b[W_PHONES..W_PHONES + n].copy_from_slice(&out);
    w.b[W_PHONES + n] = 0;
    Some(())
}

fn gem(c: u8) -> bool {
    matches!(c, 9..=13 | 20..=22 | 25..=31 | 38..=43 | b'.' | b'?')
}

/// `FUN_100250e0`: rewrites `s` in place; true if anything merged.
pub fn degeminate(s: &mut Vec<u8>) -> bool {
    let n = s.len();
    let at = |k: usize| s.get(k).copied().unwrap_or(0);
    let mut out = Vec::with_capacity(n);
    let mut changed = false;
    let mut i = 0usize;
    while i < n {
        let c = at(i);
        let d = at(i + 1);
        if i + 1 < n && gem(c) && c == d {
            out.push(c);
            i += 1;
            changed = true;
        } else if i + 1 < n && c == b'+' && d == 0x0b {
            out.push(0x0b);
            i += 1;
            changed = true;
        } else if i + 2 < n && c == b'+' && d == SYL && at(i + 2) == 0x0b {
            out.push(0x0b);
            i += 2;
            changed = true;
        } else if i + 1 < n && d == b',' && matches!(c, 1 | 2 | b' ' | b'!' | 0x0e | 0x0f) {
            out.push(match c {
                1 => 5,
                2 => 6,
                b' ' => b'"',
                b'!' => b'#',
                0x0e => 0x10,
                _ => 0x11,
            });
            i += 1;
            changed = true;
        } else {
            out.push(c);
        }
        i += 1;
    }
    *s = out;
    changed
}

/// `FUN_10025700`: phone codes -> space-separated names (100 -> "#", 99 -> "-").
fn codes_to_names(codes: &[u8]) -> Vec<u8> {
    let mut s = Vec::new();
    for (k, &c) in codes.iter().enumerate() {
        if k > 0 {
            s.push(b' ');
        }
        match c {
            100 => s.push(b'#'),
            SYL => s.push(b'-'),
            _ => s.extend_from_slice(NAMES.get((c as usize).wrapping_sub(1)).copied().unwrap_or(b"")),
        }
    }
    s
}

/// `FUN_10022430`: re-inserts "-" tokens between syllables of a name string.
/// `None` for the engine's -1 (output left empty).
fn syllabify_names(s: &[u8]) -> Option<Vec<u8>> {
    let len = s.len();
    let at = |k: usize| s.get(k).copied().unwrap_or(0);
    let mut cls: Vec<u8> = Vec::new();
    let mut tok: Vec<u8> = Vec::new();
    let mut pos = 0usize;
    while pos <= len {
        let b = at(pos);
        if b == b' ' || pos == len {
            let c = if tok.contains(&b'0') || tok.contains(&b'1') {
                b'V'
            } else if tok.len() == 1 && in_set(b"BKFGP", tok[0]) && pos + 2 <= len && in_set(b"LR", at(pos + 1)) && (pos + 2 == len || at(pos + 2) == b' ') {
                pos += 2;
                b'B'
            } else if tok.len() == 1 && in_set(b"DT", tok[0]) && pos + 2 <= len && at(pos + 1) == b'R' && (pos + 2 == len || at(pos + 2) == b' ') {
                pos += 2;
                b'B'
            } else {
                b'C'
            };
            cls.push(c);
            tok.clear();
        } else {
            tok.push(b);
        }
        pos += 1;
    }
    let n = cls.len();
    if n == 0 {
        return None;
    }
    let mut p2: Vec<u8> = Vec::new();
    for i in 0..n {
        let cur = cls[i];
        let more = i + 1 < n;
        let next = if more { cls[i + 1] } else { 0 };
        if i >= 1 {
            let prev = cls[i - 1];
            if prev == b'V' && cur == b'V' {
                p2.extend_from_slice(b"-V");
                continue;
            }
            if more && prev == b'V' && (cur == b'C' || cur == b'B') && next == b'V' {
                p2.extend_from_slice(&[b'-', cur]);
                continue;
            }
        }
        if i > 1
            && more
            && cls[i - 2] == b'V'
            && matches!((cls[i - 1], cur), (b'C', b'C') | (b'B', b'C') | (b'C', b'B'))
            && next == b'V'
        {
            p2.extend_from_slice(&[b'-', cur]);
            continue;
        }
        if i > 2 && more && cls[i - 3] == b'V' && cls[i - 2] == b'C' && cls[i - 1] == b'C' && (cur == b'C' || cur == b'B') && next == b'V' {
            p2.extend_from_slice(&[b'-', cur]);
            continue;
        }
        p2.push(cur);
    }
    if p2.first() == Some(&b'-') {
        return None;
    }
    let mut toks = s.split(|&c| c == b' ').filter(|t| !t.is_empty());
    let mut out: Vec<u8> = Vec::new();
    let push = |out: &mut Vec<u8>, t: &[u8]| {
        if !out.is_empty() {
            out.push(b' ');
        }
        out.extend_from_slice(t);
    };
    for &c in &p2 {
        if c == b'-' {
            push(&mut out, b"-");
        } else {
            let t = toks.next().unwrap_or(b"");
            push(&mut out, t);
            if c == b'B' {
                let t = toks.next().unwrap_or(b"");
                push(&mut out, t);
            }
        }
    }
    Some(out)
}

/// `FUN_10025820`: names -> codes. `None` when a name is unknown (the caller
/// then aborts the whole analysis).
fn names_to_codes(s: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    for t in s.split(|&c| c == b' ').filter(|t| !t.is_empty()) {
        let up: Vec<u8> = t.iter().map(|c| c.to_ascii_uppercase()).collect();
        if up == b"-" {
            out.push(SYL);
            continue;
        }
        let k = NAMES.iter().position(|&n| n == &up[..])?;
        if out.len() > 99 {
            return None;
        }
        out.push(k as u8 + 1);
    }
    Some(out)
}

/// `FUN_10025700` + `FUN_10022430` + `FUN_10025820` on a lexicon pron with its
/// syllable marks already removed.
pub fn syllabify_pron(codes: &[u8]) -> Option<Vec<u8>> {
    let names = codes_to_names(codes);
    let syl = syllabify_names(&names).unwrap_or_default();
    names_to_codes(&syl)
}
