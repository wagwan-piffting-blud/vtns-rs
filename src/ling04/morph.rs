//! Morphology of the 2004 engine (`FUN_100062e0`, `ws+0x429a2`): pronunciations for morphs the lookup left
//! without one, then the post-rules and the final "the" pass. The chain is the 2009 one (`ling::morph`)
//! in an older form: a short suffix list without "-ed", no British "-ise" retry, the clitic 'd always D,
//! no "Lexus IS" / no-spell lists and no vowel-count test in the spell check.

use super::lts::{letter_to_sound, lookup_or_lts, lts_hyphenated, spell};
use super::tables::CODEBOOK;
use super::{morph, mr, war, Wa04, MORPH_COUNT};
use crate::lexicon::{decode_payload_with, stricmp, strcmp, LexEntry, Lexicon};
use crate::ling::lts::{letter_vowel, lts_valid, phone_vowel};
use crate::ling::morph::{is_mixed_case, split_camel_part09};
use crate::ling::tables::{CHAR_CLASS, L_BE, L_CONJ, L_NUMWORD, L_OBJPRON, L_WH, TOLOWER_S};
use crate::ling::tagger::{in_list, modal_pair, strnicmp};
use crate::ling::{sc, Ws};

fn m(i: usize) -> usize {
    morph(i)
}

fn n_morphs(ws: &Ws) -> usize {
    ws.i16(MORPH_COUNT).max(0) as usize
}

fn sp(ws: &Ws, i: usize) -> Vec<u8> {
    ws.cstr(m(i) + mr::SPELLING).to_vec()
}

fn pron(ws: &Ws, i: usize) -> Vec<u8> {
    ws.cstr(m(i) + mr::PRON).to_vec()
}

fn set_pron(ws: &mut Ws, i: usize, p: &[u8]) {
    ws.strcpy(m(i) + mr::PRON, p);
}

fn flags_or(ws: &mut Ws, i: usize, bits: u8) {
    let o = m(i) + mr::FLAGS;
    ws.set_u8(o, ws.u8(o) | bits);
}

fn token(ws: &Ws, i: usize) -> i16 {
    ws.i16(m(i) + mr::TOKEN)
}

fn cls(c: u8) -> u8 {
    sc(&CHAR_CLASS, c)
}

fn low(c: u8) -> u8 {
    TOLOWER_S[c as usize] as u8
}

fn ci(a: &[u8], b: &[u8]) -> bool {
    stricmp(a, b) == 0
}

/// Phone code is a vowel (`DAT_1006596c`, codes 1..0x45).
fn vowel(c: i8) -> bool {
    (1..=0x45).contains(&c) && phone_vowel(c as u8) != 0
}

fn entry(lex: &Lexicon, w: &[u8]) -> LexEntry {
    let raw = lex.lookup_payload(w).unwrap_or(&[]);
    decode_payload_with(&raw[..raw.iter().position(|&c| c == 0).unwrap_or(raw.len())], &CODEBOOK)
}

fn spell_f(w: &[u8], fl: &mut u8) -> Vec<u8> {
    let s = spell(w);
    if !s.is_empty() {
        *fl |= 0x20;
    }
    s
}

fn lts_f(lex: &Lexicon, w: &[u8], fl: &mut u8) -> Option<Vec<u8>> {
    let (p, f) = letter_to_sound(lex, w)?;
    *fl |= f;
    Some(p)
}

/// Append Z / S / IH Z for a plural or possessive by the last phone.
fn s_ending(p: &mut Vec<u8>, last: u8) {
    match last {
        0x20 | 0x2a | 0x35 | 0x39 | 0x3a => p.push(0x37),
        0x14 | 0x29 | 0x37 | 0x38 | 0x44 | 0x45 => p.extend_from_slice(&[0x23, 0x44]),
        _ => p.push(0x44),
    }
}

/// `FUN_100086c0`: -liness, -ance, -ness, -ment, -ship, -less, -ing, -ist, -ful, -ly and plural -s off a
/// known root. A matching suffix whose root is unknown fails without trying the others.
fn suffix_strip(lex: &Lexicon, w: &[u8], fl: &mut u8) -> Option<Vec<u8>> {
    let n = w.len();
    let a050 = |root: &[u8]| lookup_or_lts(lex, root).unwrap_or_default();
    let ends = |k: usize, s: &[u8]| ci(&w[n - k..], s);
    let with = |root: &[u8], add: &[u8]| -> Option<Vec<u8>> {
        let mut p = a050(root);
        if p.is_empty() {
            return None;
        }
        p.extend_from_slice(add);
        Some(p)
    };
    let r = 'r: {
        if n >= 9 && ends(6, b"liness") {
            let mut root = w[..n - 5].to_vec();
            root.push(b'y');
            let mut p = a050(&root);
            if p.is_empty() {
                p = a050(&w[..n - 6]);
                if p.is_empty() {
                    return None;
                }
                p.extend_from_slice(&[0x2b, 0x26]);
            }
            p.extend_from_slice(&[0x2d, 0x23, 0x37]);
            break 'r p;
        }
        if n >= 7 {
            let root = &w[..n - 4];
            if ends(4, b"ance") {
                break 'r with(root, &[7, 0x2d, 0x37])?;
            }
            if ends(4, b"ness") {
                break 'r with(root, &[0x2d, 0x23, 0x37])?;
            }
            if ends(4, b"ment") {
                break 'r with(root, &[0x2c, 7, 0x2d, 0x39])?;
            }
            if ends(4, b"ship") {
                break 'r with(root, &[0x38, 0x25, 0x35])?;
            }
            if ends(4, b"less") {
                break 'r with(root, &[0x2b, 0x23, 0x37])?;
            }
        }
        if n >= 6 {
            let root = &w[..n - 3];
            if ends(3, b"ing") {
                break 'r with(root, &[0x23, 0x2e])?;
            }
            if ends(3, b"ist") {
                break 'r with(root, &[0x23, 0x37, 0x39])?;
            }
            if ends(3, b"ful") {
                break 'r with(root, &[0x20, 7, 0x2b])?;
            }
        }
        if n < 5 {
            return None;
        }
        if ends(2, b"ly") {
            let mut p = a050(&w[..n - 2]);
            if p.is_empty() {
                return None;
            }
            if p.last() != Some(&0x2b) {
                p.push(0x2b);
            }
            p.push(0x26);
            break 'r p;
        }
        if low(w[n - 1]) != b's' {
            return None;
        }
        let c3 = low(w[n - 3]);
        let es = low(w[n - 2]) == b'e'
            && (b"szx".contains(&c3)
                || strnicmp(&w[n - 4..], b"ch", 2) == 0
                || strnicmp(&w[n - 4..], b"sh", 2) == 0
                || (!letter_vowel(w[n - 4]) && c3 == b'o'));
        let p = if es {
            let p = a050(&w[..n - 2]);
            if p.is_empty() {
                a050(&w[..n - 1])
            } else {
                p
            }
        } else if !letter_vowel(w[n - 4]) && strnicmp(&w[n - 3..], b"ie", 2) == 0 {
            let mut root = w[..n - 3].to_vec();
            root.push(b'y');
            a050(&root)
        } else {
            let c2 = low(w[n - 2]);
            if b"szx".contains(&c2)
                || strnicmp(&w[n - 3..], b"ch", 2) == 0
                || strnicmp(&w[n - 3..], b"sh", 2) == 0
                || (!letter_vowel(w[n - 3]) && c2 == b'o')
            {
                return None;
            }
            a050(&w[..n - 1])
        };
        let Some(&last) = p.last() else { return None };
        let mut p = p;
        s_ending(&mut p, last);
        p
    };
    *fl |= 8;
    Some(r)
}

/// `FUN_10009c80`: a word ending in an apostrophe ("goin'" reads as "going" with N for NG).
fn apos_end(lex: &Lexicon, w: &[u8], fl: &mut u8) -> Option<Vec<u8>> {
    let n = w.len();
    if n == 0 || w[n - 1] != b'\'' {
        return None;
    }
    if n > 3 && ci(&w[n - 3..], b"in'") {
        let mut ing = w[..n - 1].to_vec();
        ing.push(if cls(w[n - 2]) & 0x80 != 0 { b'G' } else { b'g' });
        if let Some(mut r) = lookup_or_lts(lex, &ing) {
            if let Some(l) = r.last_mut() {
                if *l == 0x2e {
                    *l = 0x2d;
                }
            }
            *fl |= 9;
            return Some(r);
        }
    }
    let stem = &w[..n - 1];
    if let Some(r) = lookup_or_lts(lex, stem) {
        *fl |= 9;
        return Some(r);
    }
    if let Some(r) = lts_f(lex, w, fl) {
        *fl |= 8;
        return Some(r);
    }
    let s = spell_f(stem, fl);
    if s.is_empty() {
        return None;
    }
    *fl |= 8;
    Some(s)
}

/// The lexicon fields `FUN_10006f10` reads: from the morph (driver call) or from a decoded entry.
struct Src {
    class: u8,
    has_pron: bool,
    b80: bool,
    b40: bool,
    b20: bool,
    b10: bool,
}

impl Src {
    fn of(e: &LexEntry) -> Src {
        Src { class: e.class_char, has_pron: !e.prons.is_empty(), b80: e.b80 != 0, b40: e.b40 != 0, b20: e.b20 != 0, b10: e.b10 != 0 }
    }

    fn morph(ws: &Ws, i: usize) -> Src {
        let b = |o: usize| ws.i16(m(i) + o) != 0;
        Src {
            class: ws.u8(m(i) + mr::CLASS_CHAR),
            has_pron: ws.u8(m(i) + mr::PRON) != 0,
            b80: b(mr::B80),
            b40: b(mr::B40),
            b20: b(mr::B20),
            b10: b(mr::B10),
        }
    }
}

/// `FUN_1000cba0`: every character an uppercase letter.
fn caps_word(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&c| cls(c) & 0x80 != 0)
}

/// `FUN_1000cb70`: no lowercase letter.
fn no_lower(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&c| cls(c) & 0x40 == 0)
}

/// `FUN_10006f10(word, morphs, idx, entry, part)`: should an all-caps word be spelled? Parts of a split word
/// (`part`) that are single letters or class 'E' entries always are.
fn should_spell(lex: &Lexicon, ws: &Ws, idx: usize, w: &[u8], src: &Src, part: bool) -> bool {
    if part && (w.len() == 1 || src.class == b'E') {
        return true;
    }
    if !caps_word(w) {
        return false;
    }
    if src.class == b'E' {
        return true;
    }
    let n = n_morphs(ws);
    let wordish = |s: &[u8]| {
        let a = |c: u8| cls(c) & 0xc0 != 0;
        s.first().is_some_and(|&c| a(c)) || (s.len() > 1 && a(s[1]))
    };
    let prev = (0..idx).rev().map(|j| sp(ws, j)).find(|s| wordish(s));
    let next = (idx + 1..n).map(|j| sp(ws, j)).find(|s| wordish(s));
    let caps_ctx = match (&prev, &next) {
        (None, None) => false,
        (None, Some(b)) => no_lower(b),
        (Some(a), None) => no_lower(a),
        (Some(a), Some(b)) => no_lower(a) && no_lower(b),
    };
    let len = w.len();
    if src.class == b'A' {
        if !caps_ctx {
            return true;
        }
        if !src.has_pron && !src.b80 {
            return false;
        }
        if len > 3 {
            return false;
        }
        return !lts_valid(lex, w);
    }
    if src.has_pron || src.b80 {
        return false;
    }
    if len > 5 && lts_valid(lex, w) {
        return false;
    }
    if src.b40 || src.b20 || !caps_ctx {
        return true;
    }
    if src.b10 {
        return false;
    }
    len < 4 || !lts_valid(lex, w)
}

/// `FUN_100076f0`: the record's case mode ('S' spell, 'C' with U flag 'I' / 'M' / 'E').
fn case_mode(lex: &Lexicon, ws: &mut Ws, i: usize, u_flag: u8, mode: u8) -> bool {
    let class = ws.u8(m(i) + mr::CLASS_CHAR);
    let w = sp(ws, i);
    let spell_it = |ws: &mut Ws| {
        let mut fl = 0u8;
        let s = spell_f(&w, &mut fl);
        set_pron(ws, i, &s);
        flags_or(ws, i, fl);
        true
    };
    let lts_it = |ws: &mut Ws| -> bool {
        match letter_to_sound(lex, &w) {
            Some((p, f)) => {
                set_pron(ws, i, &p);
                flags_or(ws, i, f);
                true
            }
            None => false,
        }
    };
    let has_pron = ws.u8(m(i) + mr::PRON) != 0;
    let b80 = ws.i16(m(i) + mr::B80) != 0;
    if mode == b'S' {
        return spell_it(ws);
    }
    if mode != b'C' {
        return false;
    }
    match u_flag {
        b'I' => {
            if class == b'E' || class == b'A' {
                return spell_it(ws);
            }
            if has_pron {
                flags_or(ws, i, 1);
                return true;
            }
            if b80 && lts_it(ws) {
                return true;
            }
            if !lts_it(ws) {
                spell_it(ws);
            }
            true
        }
        b'M' | b'E' => {
            if class == b'E' || class == b'A' {
                return spell_it(ws);
            }
            if has_pron {
                flags_or(ws, i, 1);
                return true;
            }
            if b80 && lts_it(ws) {
                return true;
            }
            spell_it(ws)
        }
        _ => false,
    }
}

/// `FUN_100082f0`: a word with '-' or '.' inside, read part by part. A lone "a" is the article only
/// between dashes (or first, before a dash, when `dash`); other single letters are spelled. A part after
/// "the" that starts with a vowel turns the previous part's last phone into IY; the first multi-letter
/// part tests this too, against the byte before the output (returned as the second value).
fn hyphen_parts(lex: &Lexicon, ws: &Ws, idx: usize, w: &[u8], dash: bool, fl: &mut u8) -> (Option<Vec<u8>>, bool) {
    let n = w.len();
    let Some(sep) = w.iter().position(|&c| c == b'-').or_else(|| w.iter().position(|&c| c == b'.')) else {
        return (None, false);
    };
    if sep == 0 || sep + 1 >= n {
        return (None, false);
    }
    let mut out: Vec<u8> = Vec::new();
    let mut pre = false;
    let mut the_before = true;
    let mut start = 0usize;
    let mut k = 0usize;
    for pos in 0..=n {
        let c = w.get(pos).copied().unwrap_or(0);
        if c != b'-' && c != b'.' && c != 0 {
            continue;
        }
        let part = &w[start..pos];
        let ph: Vec<u8> = if part.len() == 1 {
            the_before = false;
            let dashed = if k == 0 { dash && c == b'-' } else { pos >= 2 && w[pos - 2] == b'-' && c == b'-' };
            if low(part[0]) == b'a' && dashed {
                vec![7]
            } else {
                spell_f(part, fl)
            }
        } else {
            let ph = if (entry(lex, part).class_char == b'E' || entry(lex, part).class_char == b'A') && no_lower(part) {
                spell_f(part, fl)
            } else if let Some(r) = lookup_or_lts(lex, part) {
                *fl |= 1;
                r
            } else {
                let d = if k == 0 { dash } else { start >= 1 && w[start - 1] == b'-' };
                if let Some(r) = suffix_strip(lex, part, fl) {
                    r
                } else if let Some(r) = apos_end(lex, part, fl) {
                    r
                } else if let Some(r) = clitic_suffix(lex, ws, idx, part, d, fl) {
                    r
                } else if let Some(r) = camel(lex, ws, idx, part, fl) {
                    r
                } else if let Some(r) = lts_f(lex, part, fl) {
                    r
                } else {
                    spell_f(part, fl)
                }
            };
            if the_before && ph.first().is_some_and(|&p| vowel(p as i8) && p != 0x43) {
                match out.last_mut() {
                    Some(l) => *l = 0x26,
                    None => pre = true,
                }
            }
            the_before = ci(part, b"the");
            ph
        };
        if !ph.is_empty() {
            if out.len() + ph.len() + 1 > 0x3f {
                break;
            }
            if !out.is_empty() {
                out.push(100);
            }
            out.extend_from_slice(&ph);
        }
        k += 1;
        start = pos + 1;
    }
    if out.is_empty() {
        return (None, pre);
    }
    *fl |= 8;
    (Some(out), pre)
}

/// `FUN_10009540`: a word ending in 'n / 'ee / 'er / 'ers (lower case or all caps, first occurrence).
fn clitic_suffix(lex: &Lexicon, ws: &Ws, idx: usize, w: &[u8], dash: bool, fl: &mut u8) -> Option<Vec<u8>> {
    let n = w.len();
    let strstr = |pat: &[u8]| w.windows(pat.len()).position(|x| x == pat);
    let at_end = |lo: &[u8], up: &[u8]| {
        let p = strstr(lo).or_else(|| strstr(up))?;
        if p + lo.len() == n {
            Some(p)
        } else {
            None
        }
    };
    let mut p = None;
    if n > 2 {
        p = at_end(b"'n", b"'N");
    }
    if p.is_none() && n > 3 {
        p = at_end(b"'ee", b"'EE").or_else(|| at_end(b"'er", b"'ER"));
    }
    if p.is_none() && n > 4 {
        p = at_end(b"'ers", b"'ERS");
    }
    let p = p?;
    let root = &w[..p];
    let e = entry(lex, root);
    let mut r = if matches!(e.class_char, b'E' | b'A') && should_spell(lex, ws, idx, root, &Src::of(&e), true) {
        spell_f(root, fl)
    } else if let Some((_, pr)) = e.prons.first() {
        *fl |= 1;
        pr.clone()
    } else if let (Some(r), _) = hyphen_parts(lex, ws, idx, root, dash, fl) {
        r
    } else if let Some(r) = suffix_strip(lex, root, fl) {
        r
    } else if let Some(r) = lts_f(lex, root, fl) {
        r
    } else {
        spell_f(root, fl)
    };
    let len = r.len();
    let last = if len >= 1 { r[len - 1] as i8 } else { -1 };
    let prev = if len >= 2 { r[len - 2] as i8 } else { -1 };
    let suf = &w[p..];
    if ci(suf, b"'n") {
        if vowel(last) || (vowel(prev) && last == 0x36) {
            r.push(0x2d);
        } else {
            r.extend_from_slice(&[7, 0x2d]);
        }
    } else if ci(suf, b"'ee") {
        r.push(0x26);
    } else if ci(suf, b"'er") {
        r.push(0x1a);
    } else if ci(suf, b"'ers") {
        r.extend_from_slice(&[0x1a, 0x44]);
    }
    if r.is_empty() {
        return None;
    }
    *fl |= 8;
    Some(r)
}

/// `FUN_10008df0`: a morph sharing its token with the one before. The token's spellings joined are looked
/// up whole first; otherwise a known clitic reads after the previous morph's last phones.
fn clitic(lex: &Lexicon, ws: &mut Ws, idx: usize) -> bool {
    if idx == 0 || token(ws, idx - 1) != token(ws, idx) {
        return false;
    }
    let w = sp(ws, idx);
    let c0 = w.first().copied().unwrap_or(0);
    if (c0 == 0 || b",.!?".contains(&c0)) && w.get(1).copied().unwrap_or(0) == 0 {
        return false;
    }
    let mut start = idx - 1;
    while start > 0 && token(ws, start - 1) == token(ws, start) {
        start -= 1;
    }
    let mut joined: Vec<u8> = Vec::new();
    for k in start..=idx {
        joined.extend_from_slice(&sp(ws, k));
    }
    if let Some(r) = lookup_or_lts(lex, &joined) {
        set_pron(ws, start, &r);
        flags_or(ws, start, 8);
        for k in start + 1..=idx {
            set_pron(ws, k, b"");
            flags_or(ws, k, 8);
        }
        return true;
    }
    let pp = pron(ws, idx - 1);
    let n = pp.len();
    let last = if n >= 1 { pp[n - 1] as i8 } else { 0 };
    let prev = if n >= 2 { pp[n - 2] as i8 } else { 0 };
    let v_or_r = vowel(last) || (vowel(prev) && last == 0x36);
    let is = |lo: &[u8], up: &[u8]| w.as_slice() == lo || w.as_slice() == up;
    let p: Vec<u8> = if ci(&w, b"'s") {
        let mut v = Vec::new();
        s_ending(&mut v, last as u8);
        v
    } else if is(b"n't", b"N'T") {
        if v_or_r { vec![0x2d, 0x39] } else { vec![7, 0x2d, 0x39] }
    } else if is(b"'ve", b"'VE") {
        if vowel(last) { vec![0x41] } else { vec![7, 0x41] }
    } else if is(b"'em", b"'EM") {
        vec![7, 0x2c]
    } else if is(b"'re", b"'RE") {
        if vowel(last) { vec![0x36] } else { vec![0x1a] }
    } else if is(b"'ll", b"'LL") {
        if v_or_r { vec![0x2b] } else { vec![7, 0x2b] }
    } else if ci(&w, b"'d") {
        if v_or_r { vec![0x15] } else { vec![7, 0x15] }
    } else if ci(&w, b"'m") {
        if vowel(last) { vec![0x2c] } else { vec![7, 0x2c] }
    } else {
        Vec::new()
    };
    set_pron(ws, idx, &p);
    if p.is_empty() {
        return false;
    }
    flags_or(ws, idx, 8);
    true
}

/// `FUN_100098f0`: camelCase split (`FUN_10009dc0` parts); a part reads from the lexicon, LTS when the entry
/// asks for it, spelling, the suffix stripper or "X's"; LTS or spelling alone doesn't resolve the word.
fn camel(lex: &Lexicon, ws: &Ws, idx: usize, w: &[u8], fl: &mut u8) -> Option<Vec<u8>> {
    if !is_mixed_case(w) {
        return None;
    }
    let n = w.len();
    let mut pos = 0usize;
    let mut out: Vec<u8> = Vec::new();
    let mut k = 0usize;
    let mut resolved = false;
    loop {
        let part = split_camel_part09(w, &mut pos);
        if k == 0 && pos == n {
            break;
        }
        k += 1;
        let e = entry(lex, &part);
        let ph: Vec<u8> = if part.len() >= 2 && !e.prons.is_empty() {
            resolved = true;
            *fl |= 1;
            e.prons[0].1.clone()
        } else if let Some(r) = if e.b80 != 0 { lts_f(lex, &part, fl) } else { None } {
            resolved = true;
            r
        } else if should_spell(lex, ws, idx, &part, &Src::of(&e), true) {
            resolved = true;
            spell_f(&part, fl)
        } else if let Some(r) = suffix_strip(lex, &part, fl) {
            resolved = true;
            r
        } else if let Some(r) = possessive(lex, &part) {
            resolved = true;
            r
        } else if let Some(r) = lts_f(lex, &part, fl) {
            r
        } else {
            spell_f(&part, fl)
        };
        if out.len() + ph.len() + 1 > 0x3f {
            break;
        }
        if !out.is_empty() {
            out.push(100);
        }
        out.extend_from_slice(&ph);
        if pos >= n {
            break;
        }
    }
    if !out.is_empty() && resolved {
        *fl |= 8;
        Some(out)
    } else {
        None
    }
}

/// "X's" inside `FUN_100098f0` (case-sensitive "'s").
fn possessive(lex: &Lexicon, part: &[u8]) -> Option<Vec<u8>> {
    let l = part.len();
    if l < 2 || strcmp(&part[l - 2..], b"'s") != 0 {
        return None;
    }
    let mut p = lookup_or_lts(lex, &part[..l - 2])?;
    let last = *p.last()?;
    s_ending(&mut p, last);
    Some(p)
}

/// `FUN_10007860`: preset 'U' tokens, on the token's first morph. Mode 'Y' takes the preset phones and moves
/// `*idx` to the token's last morph; mode 'S' spells the spelling cut at the sub-token. The sub-token is then
/// appended as a clitic, or read without its first character after a 100 break.
fn u_token(ws: &mut Ws, wa: &Wa04, lex: &Lexicon, idx: &mut usize) -> bool {
    let i0 = *idx;
    let tok = token(ws, i0);
    let r = Wa04::rec(tok as usize);
    if wa.u8(r + war::U_FLAG) != b'U' || (i0 != 0 && token(ws, i0 - 1) == tok) {
        return false;
    }
    let sub = |r: usize| -> Vec<u8> {
        let off = wa.i32(r + war::SUB_OFF);
        if off < 1 {
            return Vec::new();
        }
        let len = wa.i32(r + war::SUB_LEN).max(0) as usize;
        let base = r + war::TEXT + off as usize;
        (0..len).map(|k| wa.u8(base + k)).take_while(|&c| c != 0).collect()
    };
    let cl = match wa.u8(r + war::CASE_MODE) {
        b'Y' => {
            let preset = wa.cstr(r + war::PHONES).to_vec();
            set_pron(ws, i0, &preset);
            flags_or(ws, i0, 4);
            let n = n_morphs(ws);
            while *idx + 1 < n && token(ws, *idx) == token(ws, *idx + 1) {
                *idx += 1;
            }
            sub(Wa04::rec(token(ws, *idx) as usize))
        }
        b'S' => {
            let mut w = sp(ws, i0);
            let off = wa.i32(r + war::SUB_OFF);
            let c = if off < 1 {
                Vec::new()
            } else {
                w.truncate(off as usize);
                sub(r)
            };
            let mut fl = 0u8;
            let s = spell_f(&w, &mut fl);
            set_pron(ws, i0, &s);
            flags_or(ws, i0, fl);
            c
        }
        _ => return false,
    };
    if cl.is_empty() {
        return true;
    }
    let p = *idx;
    let mut pr = pron(ws, p);
    let n = pr.len();
    let last = if n >= 1 { pr[n - 1] as i8 } else { 0 };
    let prev = if n >= 2 { pr[n - 2] as i8 } else { 0 };
    let v_or_r = vowel(last) || (vowel(prev) && last == 0x36);
    let c = cl.as_slice();
    let is = |lo: &[u8], up: &[u8]| c == lo || c == up;
    let add: Vec<u8> = if ci(c, b"'s") {
        let mut v = Vec::new();
        s_ending(&mut v, last as u8);
        v
    } else if is(b"'ve", b"'VE") {
        if vowel(last) { vec![0x41] } else { vec![7, 0x41] }
    } else if is(b"'em", b"'EM") {
        vec![7, 0x2c]
    } else if is(b"'re", b"'RE") {
        if vowel(last) { vec![0x36] } else { vec![0x1a] }
    } else if is(b"'ll", b"'LL") {
        if v_or_r { vec![0x2b] } else { vec![7, 0x2b] }
    } else if ci(c, b"'d") {
        if v_or_r { vec![0x15] } else { vec![7, 0x15] }
    } else if ci(c, b"'m") {
        if vowel(last) { vec![0x2c] } else { vec![7, 0x2c] }
    } else if ci(c, b"'n") {
        if v_or_r { vec![0x2d] } else { vec![7, 0x2d] }
    } else if ci(c, b"'ee") {
        vec![0x26]
    } else if ci(c, b"'er") {
        vec![0x1a]
    } else if ci(c, b"'ers") {
        vec![0x1a, 0x44]
    } else {
        let w = &c[1..];
        let mut fl = 0u8;
        let r = match lookup_or_lts(lex, w) {
            Some(r) => r,
            None => match lts_f(lex, w, &mut fl) {
                Some(r) => r,
                None => spell_f(w, &mut fl),
            },
        };
        flags_or(ws, p, fl);
        if r.is_empty() {
            Vec::new()
        } else {
            let mut v = vec![100];
            v.extend_from_slice(&r);
            v
        }
    };
    pr.extend_from_slice(&add);
    if !pr.is_empty() {
        set_pron(ws, p, &pr);
        flags_or(ws, p, 8);
    }
    true
}

/// `FUN_10007530`: the longest exceptdict entry for the hyphen-joined spellings of the morphs from `i`;
/// its pronunciation is poured into the morphs it covers (`FUN_10009f80`). Returns how many it covered.
fn except(ws: &mut Ws, lex: &Lexicon, i: usize, n: usize) -> Option<usize> {
    let mut joined: Vec<u8> = Vec::new();
    let mut parts = 0i32;
    let mut best: Option<(Vec<u8>, usize)> = None;
    for k in i..n {
        let w = sp(ws, k);
        let mut own = 1i32;
        for &c in &w {
            if cls(c) & 0xc0 != 0 || c == b'\'' {
                continue;
            }
            if c != b'-' {
                own = 0;
                break;
            }
            own += 1;
        }
        if own == 0 {
            if k == i {
                return None;
            }
            break;
        }
        parts += own;
        if k == i {
            joined = w.clone();
        } else {
            joined.push(b'-');
            joined.extend_from_slice(&w);
        }
        if parts >= lex.exc.lo {
            if parts > lex.exc.hi {
                break;
            }
            if let Some(v) = lex.exc.find(&joined, parts) {
                best = Some((v.to_vec(), k - i + 1));
            }
        }
    }
    let (val, cnt) = best?;
    let dashes = |s: &[u8]| s.iter().filter(|&&c| c == b'-').count() + 1;
    let mut idx = i;
    let mut off = 0usize;
    let mut budget = dashes(&sp(ws, idx));
    let mut d = 0usize;
    for &c in &val {
        if c == 100 {
            d += 1;
            if d < budget {
                ws.set_u8(m(idx) + mr::PRON + off, 100);
                off += 1;
                continue;
            }
            ws.set_u8(m(idx) + mr::PRON + off, 0);
            ws.set_i16(m(idx) + mr::FLAGS, 2);
            idx += 1;
            off = 0;
            d = 0;
            budget = dashes(&sp(ws, idx));
        } else {
            ws.set_u8(m(idx) + mr::PRON + off, c);
            off += 1;
        }
    }
    ws.set_u8(m(idx) + mr::PRON + off, 0);
    ws.set_i16(m(idx) + mr::FLAGS, 2);
    Some(cnt)
}

/// `FUN_10006de0`: verb context for "use" / "close" (let, modal pairs incl. the morph 0/1 quirk, do, to).
fn verb_context(ws: &Ws, i: usize) -> bool {
    if i == 0 {
        return true;
    }
    if i > 1 {
        let a = sp(ws, i - 2);
        let b = sp(ws, i - 1);
        if ci(&a, b"let") || modal_pair(&a, Some(&b)) || modal_pair(&sp(ws, 0), Some(&sp(ws, 1))) {
            return true;
        }
        if modal_pair(&a, Some(b"")) && (ci(&b, b"n't") || ci(&b, b"not")) {
            return true;
        }
    }
    let c = sp(ws, i - 1);
    ci(&c, b"do") || ci(&c, b"to") || modal_pair(&c, Some(b""))
}

/// Test hook: one chain step on `w` as morph 0 of a one-morph array in `ws` (0 suffix, 1 apostrophe,
/// 2 clitic suffix, 3 camelCase, 4 / 5 hyphen parts with dash off / on). Returns the result, the flag bits
/// and whether the hyphen pass marked the byte before its output.
#[doc(hidden)]
pub fn probe(lex: &Lexicon, ws: &mut Ws, step: u8, w: &[u8]) -> (Option<Vec<u8>>, u8, bool) {
    ws.b[MORPH_COUNT..m(1)].fill(0);
    ws.set_i16(MORPH_COUNT, 1);
    ws.strcpy(m(0) + mr::SPELLING, w);
    let mut fl = 0u8;
    let mut pre = false;
    let r = match step {
        0 => suffix_strip(lex, w, &mut fl),
        1 => apos_end(lex, w, &mut fl),
        2 => clitic_suffix(lex, ws, 0, w, false, &mut fl),
        3 => camel(lex, ws, 0, w, &mut fl),
        _ => {
            let (r, p) = hyphen_parts(lex, ws, 0, w, step == 5, &mut fl);
            pre = p;
            r
        }
    };
    (r, fl, pre)
}

/// The chain for a morph without a pronunciation (`LAB_10006524`).
fn chain(lex: &Lexicon, ws: &mut Ws, i: usize, dash: bool) {
    let w = sp(ws, i);
    let mut fl = 0u8;
    let (h, pre) = hyphen_parts(lex, ws, i, &w, dash, &mut fl);
    if pre {
        ws.set_u8(m(i) + mr::PRON - 1, 0x26);
    }
    let mut r = h;
    if r.is_none() {
        r = suffix_strip(lex, &w, &mut fl);
    }
    if r.is_none() {
        r = clitic_suffix(lex, ws, i, &w, dash, &mut fl);
    }
    if r.is_none() {
        flags_or(ws, i, fl);
        fl = 0;
        if clitic(lex, ws, i) {
            return;
        }
        r = camel(lex, ws, i, &w, &mut fl);
    }
    if r.is_none() {
        r = apos_end(lex, &w, &mut fl);
    }
    if r.is_none() {
        r = lts_f(lex, &w, &mut fl);
    }
    let p = match r {
        Some(p) => p,
        None => {
            ws.set_u8(m(i) + mr::CLASS_CHAR, b'E');
            spell_f(&w, &mut fl)
        }
    };
    set_pron(ws, i, &p);
    flags_or(ws, i, fl);
}

/// `FUN_100062e0`.
pub fn morphology(ws: &mut Ws, wa: &Wa04, lex: &Lexicon) {
    let mut i = 0usize;
    while i < n_morphs(ws) {
        let f0 = if ws.u8(m(i) + mr::PRON) != 0 { 1 } else { 0 };
        ws.set_i16(m(i) + mr::FLAGS, f0);
        let u = u_token(ws, wa, lex, &mut i);
        let r = Wa04::rec(token(ws, i) as usize);
        let go = (wa.u8(r + war::FORCED_POS) == 0xff || ws.u8(m(i) + mr::PRON) == 0)
            && (wa.u8(r + war::U_FLAG) == b'U' || wa.u8(r + war::PHONES) == 0);
        if !go {
            i += 1;
            continue;
        }
        if !u {
            let n = n_morphs(ws);
            if let Some(c) = except(ws, lex, i, n) {
                i += c;
                continue;
            }
            let (u_flag, mode) = (wa.u8(r + war::U_FLAG), wa.u8(r + war::CASE_MODE));
            if !case_mode(lex, ws, i, u_flag, mode) {
                let w = sp(ws, i);
                let mut done = false;
                if mode == b'A' {
                    if let Some((p, f)) = letter_to_sound(lex, &w) {
                        set_pron(ws, i, &p);
                        flags_or(ws, i, f);
                        done = true;
                    }
                }
                if !done {
                    if should_spell(lex, ws, i, &w, &Src::morph(ws, i), false) {
                        let mut fl = 0u8;
                        let s = spell_f(&w, &mut fl);
                        set_pron(ws, i, &s);
                        flags_or(ws, i, fl);
                    } else if ws.u8(m(i) + mr::PRON) == 0 {
                        let dash = i >= 1 && wa.split_mark(token(ws, i - 1) as usize) != 0;
                        if ws.i16(m(i) + mr::B80) == 0 {
                            chain(lex, ws, i, dash);
                        } else {
                            let r = if w.contains(&b'-') { lts_hyphenated(lex, &w) } else { letter_to_sound(lex, &w) };
                            match r {
                                Some((p, f)) => {
                                    set_pron(ws, i, &p);
                                    flags_or(ws, i, f | 0x40);
                                }
                                None => chain(lex, ws, i, dash),
                            }
                        }
                    }
                }
            }
        }
        post_rules(ws, wa, i);
        i += 1;
    }
    let n = n_morphs(ws);
    for i in 1..n {
        let c = ws.u8(m(i) + mr::PRON);
        if vowel(c as i8) && ci(&sp(ws, i - 1), b"the") && c != 0x43 {
            set_pron(ws, i - 1, &[0x16, 0x26]);
            flags_or(ws, i - 1, 8);
        }
    }
}

/// `LAB_10006768` and the rules around it: "a", "used / unused to", "use", "UP a / the", "read", "minute"
/// and "close".
fn post_rules(ws: &mut Ws, wa: &Wa04, i: usize) {
    let n = n_morphs(ws);
    let w = sp(ws, i);
    let next = sp(ws, i + 1);
    let tok = token(ws, i);
    if ci(&w, b"a") && wa.u8(Wa04::rec(tok as usize) + war::CASE_MODE) != b'S' {
        let p0 = ws.u8(m(i) + mr::PRON);
        let letter = |c: u8| cls(c) & 0xc0 != 0;
        if i == 0
            && cls(w[0]) & 0x80 != 0
            && (0x1d..0x20).contains(&p0)
            && n > 1
            && !in_list(&L_CONJ, &next)
            && next.first().is_some_and(|&c| letter(c))
        {
            ws.set_u8(m(i) + mr::PRON, 7);
            flags_or(ws, i, 8);
        } else if i + 1 < n && tok != 0 && token(ws, i + 1) != 0 && ci(&next, b"'s") {
            ws.set_u8(m(i) + mr::PRON, 0x1e);
            flags_or(ws, i, 8);
        }
    }
    if i + 1 < n && ci(&next, b"to") {
        if ci(&w, b"used") {
            set_pron(ws, i, &[0x43, 0x3f, 0x37, 0x39]);
            flags_or(ws, i, 8);
        } else if ci(&w, b"unused") {
            set_pron(ws, i, &[9, 0x2d, 0x43, 0x3f, 0x37, 0x39]);
            flags_or(ws, i, 8);
        }
    }
    if ci(&w, b"use") && ws.u8(m(i) + mr::PRON + 2) == 0x37 && verb_context(ws, i) {
        ws.set_u8(m(i) + mr::PRON + 2, 0x44);
        flags_or(ws, i, 8);
    }
    if strcmp(&w, b"UP") == 0 && (ci(&next, b"a") || ci(&next, b"the")) {
        set_pron(ws, i, &[8, 0x35]);
        flags_or(ws, i, 8);
    }
    if i == 0 {
        return;
    }
    let prev = sp(ws, i - 1);
    if ci(&w, b"read") && ws.u8(m(i) + mr::PRON + 1) == 0x27 {
        if in_list(&L_OBJPRON, &prev) || strcmp(&prev, b"I") == 0 || in_list(&L_BE, &prev) || in_list(&L_WH, &prev) {
            ws.set_u8(m(i) + mr::PRON + 1, 0x18);
        }
    }
    if in_list(&L_NUMWORD, &prev) && ci(&w, b"minute") {
        set_pron(ws, i, &[0x2c, 0x24, 0x2d, 0x23, 0x39]);
        flags_or(ws, i, 8);
    }
    if ci(&w, b"close") && ws.u8(m(i) + mr::PRON + 3) == 0x37 && verb_context(ws, i) {
        ws.set_u8(m(i) + mr::PRON + 3, 0x44);
    }
}
