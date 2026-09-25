//! Morphology (`FUN_10007520`, `wa_rec0 = WA+0x14`) and its callees: the
//! per-morph pass that fills in a pronunciation for every morph that came out
//! of the lexicon lookup without one (flags-only / unknown words), then a
//! handful of post-rules and a final "the"/"de" pass over the whole morph
//! array.
//!
//! Confidence varies a lot between the pieces here:
//! - `driver`, the exceptdict chain (`match_hyphenated_except` and its
//!   helpers), `cb30`/`cbe0`, `case_mode` (`FUN_10009030`), `verb_context`
//!   (`FUN_10008580`), `is_numword` (`FUN_10008550`) and `apostrophe_end`
//!   (`FUN_1000c710`), `suffix_strip` (`FUN_1000a140`, full `.s`
//!   transliteration), `should_spell` (`FUN_100086c0`) and the camelCase
//!   splitter are ported from the actual disassembly (or a decompile
//!   cross-checked against the driver's own call sites).
//!   So are `ise_retry` (`FUN_1000b4d0`), `c040`, `c710` and `dc0`
//!   (`FUN_10009dc0`). Every chain step ORs into one flag byte, like the
//!   engine's `morph+4` pointer, including steps that go on to fail.
//!   `clitic_same_token` (`FUN_1000b800`) and `u_token` (`FUN_100091b0`,
//!   user-dictionary 'U' tokens) are full ports too.
//!
//! Stress corpora: `suffix.tsv`, `chain.tsv`, `clitic.tsv` and `userdict.tsv`
//! (the installed "miles an hour" entry).

use super::lts::{letter_to_sound, letter_vowel, lts_hyphenated, phone_vowel, spell};
use super::lookup::pron_for_pos;
use super::tables::{CHAR_CLASS, L_NOSPELL, TOLOWER_S};
use super::tagger::{modal_pair, strnicmp};
use super::{mr, sc, war, Wa, Ws};
use crate::lexicon::{decode_payload, stricmp, Lexicon};

fn m(i: usize) -> usize {
    Ws::morph(i)
}

fn m_spelling(ws: &Ws, i: usize) -> Vec<u8> {
    ws.cstr(m(i) + mr::SPELLING).to_vec()
}

fn m_pron(ws: &Ws, i: usize) -> Vec<u8> {
    ws.cstr(m(i) + mr::PRON).to_vec()
}

fn m_set_pron(ws: &mut Ws, i: usize, p: &[u8]) {
    ws.strcpy(m(i) + mr::PRON, p);
}

fn m_flags_or(ws: &mut Ws, i: usize, bits: u8) {
    let o = m(i) + mr::FLAGS;
    ws.set_u8(o, ws.u8(o) | bits);
}

fn m_class(ws: &Ws, i: usize) -> u8 {
    ws.u8(m(i) + mr::CLASS_CHAR)
}

fn m_b80(ws: &Ws, i: usize) -> bool {
    ws.i16(m(i) + mr::B80) != 0
}

/// `FUN_1000cb30`: lexicon lookup, first pronunciation; when the entry has no
/// pronunciation but B80 is set, fall back to LTS. Pure lookup, no flag
/// side effect (the engine passes a throwaway local as the LTS flags target).
fn cb30(lex: &Lexicon, word: &[u8]) -> Option<Vec<u8>> {
    if word.is_empty() {
        return None;
    }
    let e = lex.lookup_payload(word).map(decode_payload).unwrap_or_default();
    if !e.prons.is_empty() {
        return Some(e.prons[0].1.clone());
    }
    if e.b80 != 0 {
        if let Some((ph, _f)) = letter_to_sound(lex, word) {
            return Some(ph);
        }
    }
    None
}

/// `FUN_1000cbe0`: lexicon lookup, the pronunciation whose POS list holds
/// `pos` (`FUN_10003f10` / `lookup::pron_for_pos`).
fn cbe0(lex: &Lexicon, word: &[u8], pos: u8) -> Option<Vec<u8>> {
    let payload = lex.lookup_payload(word)?;
    if payload.is_empty() {
        return None;
    }
    let e = decode_payload(payload);
    let p = pron_for_pos(&e, pos);
    if p.is_empty() {
        None
    } else {
        Some(p)
    }
}

const VB: u8 = 0x25;

const ED_UNVOICED: [u8; 8] = [0x2a, 0x35, 0x37, 0x20, 0x22, 0x38, 0x3a, 0x14];
const ED_TD: [u8; 2] = [0x39, 0x15];

fn ed_suffix(last: u8) -> Vec<u8> {
    if ED_TD.contains(&last) {
        vec![0x23, 0x15]
    } else if ED_UNVOICED.contains(&last) {
        vec![0x39]
    } else {
        vec![0x15]
    }
}

const S_UNVOICED: [u8; 5] = [0x20, 0x2a, 0x35, 0x39, 0x3a];
const S_SIBILANT: [u8; 6] = [0x14, 0x29, 0x37, 0x38, 0x44, 0x45];

fn s_suffix(last: u8) -> Vec<u8> {
    if S_SIBILANT.contains(&last) {
        vec![0x23, 0x44]
    } else if S_UNVOICED.contains(&last) {
        vec![0x37]
    } else {
        vec![0x44]
    }
}

/// `FUN_1000a140`: suffix stripper, transliterated from the disassembly.
/// Each suffix that matches owns the result: a failed lookup returns `None`
/// rather than trying the later suffixes. "Consonant" is the engine's
/// `DAT_10077d5e[tolower(c)] == 0` (non-letters count). The "est" path's
/// AH0-vs-IH choice keys off the stale top byte of the word pointer, which is
/// only ever 3 after the i->y retry.
fn suffix_strip(lex: &Lexicon, w: &[u8]) -> Option<Vec<u8>> {
    let n = w.len();
    let lo = |i: usize| TOLOWER_S[w[i] as usize];
    let cons = |i: usize| !letter_vowel(w[i]);
    let ends = |suf: &[u8]| stricmp(&w[n - suf.len()..], suf) == 0;
    let cut = |k: usize| &w[..k];
    let with_y = |k: usize| {
        let mut s = w[..k].to_vec();
        s.push(b'y');
        s
    };
    let look = |s: &[u8]| cb30(lex, s);
    let verb = |s: &[u8]| cbe0(lex, s, VB).or_else(|| cb30(lex, s));
    let add = |r: Option<Vec<u8>>, suf: &[u8]| {
        r.map(|mut r| {
            r.extend_from_slice(suf);
            r
        })
    };
    let doubled = |a: usize| !cons(a) && cons(a + 1) && w[a + 1] == w[a + 2];
    let szx = |v: i16| {
        let c = v as u8;
        c == 0 || b"szx".contains(&c)
    };

    if n > 8 && ends(b"liness") {
        if let Some(r) = look(&with_y(n - 5)) {
            return add(Some(r), &[0x2d, 0x23, 0x37]);
        }
        return add(look(cut(n - 6)), &[0x2b, 0x26, 0x2d, 0x23, 0x37]);
    }
    if n > 6 {
        if ends(b"ance") {
            return add(look(cut(n - 4)), &[0x07, 0x2d, 0x37]);
        }
        if ends(b"ness") {
            let mut r = look(cut(n - 4));
            if r.is_none() && cons(n - 6) && lo(n - 5) == 0x69 {
                r = look(&with_y(n - 5));
            }
            return add(r, &[0x2d, 0x23, 0x37]);
        }
        if ends(b"ment") {
            return add(look(cut(n - 4)), &[0x2c, 0x07, 0x2d, 0x39]);
        }
        if ends(b"ship") {
            return add(look(cut(n - 4)), &[0x38, 0x25, 0x35]);
        }
        if ends(b"less") {
            return add(look(cut(n - 4)), &[0x2b, 0x23, 0x37]);
        }
    }
    if n > 4 {
        if ends(b"ed") {
            let mut r = None;
            if lo(n - 3) != 0x65 {
                r = verb(cut(n - 1)).or_else(|| verb(cut(n - 2)));
            }
            if r.is_none() {
                r = if doubled(n - 5) {
                    verb(cut(n - 3))
                } else if cons(n - 4) && lo(n - 3) == 0x69 {
                    verb(&with_y(n - 3))
                } else {
                    return None;
                };
            }
            let mut r = r?;
            let last = *r.last().unwrap();
            r.extend_from_slice(&ed_suffix(last));
            return Some(r);
        }
        if ends(b"er") {
            let mut r = None;
            if lo(n - 3) != 0x65 {
                r = look(cut(n - 1)).or_else(|| look(cut(n - 2)));
            }
            if r.is_none() {
                r = if doubled(n - 5) {
                    look(cut(n - 3))
                } else if cons(n - 4) && lo(n - 3) == 0x69 {
                    look(&with_y(n - 3))
                } else {
                    return None;
                };
            }
            return add(r, &[0x1a]);
        }
    }
    if n > 5 {
        if ends(b"ers") {
            if let Some(r) = look(cut(n - 1)) {
                return add(Some(r), &[0x44]);
            }
            let mut r = None;
            if lo(n - 4) != 0x65 {
                r = look(cut(n - 2)).or_else(|| look(cut(n - 3)));
            }
            if r.is_none() {
                r = if doubled(n - 6) {
                    look(cut(n - 4))
                } else if cons(n - 5) && lo(n - 4) == 0x69 {
                    look(&with_y(n - 4))
                } else {
                    return None;
                };
            }
            return add(r, &[0x1a, 0x44]);
        }
        if ends(b"est") {
            let mut r = None;
            if lo(n - 4) != 0x65 {
                r = look(cut(n - 2)).or_else(|| look(cut(n - 3)));
            }
            if r.is_some() {
                return add(r, &[0x23, 0x37, 0x39]);
            }
            if doubled(n - 6) {
                return add(look(cut(n - 4)), &[0x23, 0x37, 0x39]);
            }
            if cons(n - 5) && lo(n - 4) == 0x69 {
                return add(look(&with_y(n - 4)), &[0x07, 0x37, 0x39]);
            }
            return None;
        }
        if ends(b"ing") {
            let r = if !cons(n - 5) && cons(n - 4) {
                let mut e = w[..n - 3].to_vec();
                e.push(b'e');
                verb(&e).or_else(|| verb(cut(n - 3)))
            } else if n > 6 && cons(n - 7) && doubled(n - 6) {
                verb(cut(n - 3)).or_else(|| verb(cut(n - 4)))
            } else {
                verb(cut(n - 3))
            };
            return add(r, &[0x23, 0x2e]);
        }
        if ends(b"ist") {
            return add(look(cut(n - 3)), &[0x23, 0x37, 0x39]);
        }
        if ends(b"ful") {
            return add(look(cut(n - 3)), &[0x20, 0x07, 0x2b]);
        }
    }
    if n <= 4 {
        return None;
    }
    if ends(b"ly") {
        let mut r = look(cut(n - 2));
        if r.is_none() && cons(n - 4) && lo(n - 3) == 0x69 {
            r = look(&with_y(n - 3));
        }
        let mut r = r?;
        let k = r.len() - 1;
        if r[k] == 0x26 {
            r[k] = 0x07;
        }
        if r[k] != 0x2b {
            r.push(0x2b);
        }
        r.push(0x26);
        return Some(r);
    }
    if lo(n - 1) != 0x73 {
        return None;
    }
    let e2 = lo(n - 2) == 0x65;
    let ch_sh = |at: usize| strnicmp(&w[at..], b"ch", 2) == 0 || strnicmp(&w[at..], b"sh", 2) == 0;
    let r = if e2 && (szx(lo(n - 3)) || ch_sh(n - 4) || (cons(n - 4) && lo(n - 3) == 0x6f)) {
        look(cut(n - 1)).or_else(|| look(cut(n - 2)))
    } else if cons(n - 4) && strnicmp(&w[n - 3..], b"ie", 2) == 0 {
        look(&with_y(n - 3))
    } else if !(szx(lo(n - 2)) || ch_sh(n - 3) || (cons(n - 3) && lo(n - 2) == 0x6f)) {
        let r = look(cut(n - 1));
        if r.is_none() && e2 {
            look(cut(n - 2))
        } else {
            r
        }
    } else if cons(n - 3) && lo(n - 2) == 0x6f {
        look(cut(n - 1))
    } else {
        let r = look(cut(n - 1))?;
        if S_SIBILANT.contains(r.last().unwrap()) {
            return None;
        }
        Some(r)
    };
    let mut r = r?;
    let last = *r.last().unwrap();
    r.extend_from_slice(&s_suffix(last));
    Some(r)
}

const ISE_SUF: [(&[u8], usize); 9] = [
    (b"isations", 9),
    (b"isation", 8),
    (b"isingly", 8),
    (b"isably", 7),
    (b"isable", 7),
    (b"ising", 6),
    (b"ised", 5),
    (b"ises", 5),
    (b"ise", 4),
];

/// `FUN_1000b4d0`: British "ise" -> "ize" retry.
fn ise_retry(lex: &Lexicon, word: &[u8]) -> Option<Vec<u8>> {
    let n = word.len();
    for (suf, min) in ISE_SUF {
        if n >= min && stricmp(&word[n - suf.len()..], suf) == 0 {
            let mut w = word[..n - suf.len()].to_vec();
            w.push(b'i');
            w.push(b'z');
            w.extend_from_slice(&suf[2..]);
            if let Some(r) = cb30(lex, &w) {
                return Some(r);
            }
        }
    }
    None
}

/// `FUN_10009cd0` with its flag side effect.
fn spell_f(w: &[u8], fl: &mut u8) -> Vec<u8> {
    let s = spell(w);
    if !s.is_empty() {
        *fl |= 0x20;
    }
    s
}

/// `fe_LetterToSound` with its flag side effect.
fn lts_f(lex: &Lexicon, w: &[u8], fl: &mut u8) -> Option<Vec<u8>> {
    let (ph, f) = letter_to_sound(lex, w)?;
    *fl |= f;
    Some(ph)
}

/// `FUN_1000a140` with its flag side effect.
fn a140(lex: &Lexicon, w: &[u8], fl: &mut u8) -> Option<Vec<u8>> {
    let r = suffix_strip(lex, w)?;
    *fl |= 8;
    Some(r)
}

/// `FUN_1000c040`: a word ending in 'n / 'ee / 'er / 'ers. Case-sensitive
/// `strstr` (lower case, then all upper), first occurrence only, which must
/// end the word. The root goes through the lexicon (spelled when an E/A entry
/// says so), then `FUN_10009dc0`, `FUN_1000a140`, LTS and spelling, each
/// ORing its own flag bits.
fn c040(lex: &Lexicon, ws: &Ws, idx: usize, w: &[u8], dash: bool, fl: &mut u8) -> Option<Vec<u8>> {
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
    let e = lex.lookup_payload(root).map(decode_payload).unwrap_or_default();
    let mut r = if matches!(e.class_char, b'E' | b'A') && should_spell_part(lex, ws, idx, root, &e) {
        spell_f(root, fl)
    } else if !e.prons.is_empty() {
        *fl |= 1;
        e.prons[0].1.clone()
    } else if let Some(r) = dc0(lex, ws, idx, root, dash, fl) {
        r
    } else if let Some(r) = a140(lex, root, fl) {
        r
    } else if let Some(r) = lts_f(lex, root, fl) {
        r
    } else {
        spell_f(root, fl)
    };
    let len = r.len();
    let last = if len >= 1 { r[len - 1] as i8 } else { -1 };
    let prev = if len >= 2 { r[len - 2] as i8 } else { -1 };
    let vowel = |c: i8| (1..=0x45).contains(&c) && phone_vowel(c as u8) != 0;
    let suf = &w[p..];
    if stricmp(suf, b"'n") == 0 {
        if vowel(last) || (vowel(prev) && last == 0x36) {
            r.push(0x2d);
        } else {
            r.extend_from_slice(&[7, 0x2d]);
        }
    } else if stricmp(suf, b"'ee") == 0 {
        r.push(0x26);
    } else if stricmp(suf, b"'er") == 0 {
        r.push(0x1a);
    } else if stricmp(suf, b"'ers") == 0 {
        r.extend_from_slice(&[0x1a, 0x44]);
    }
    if r.is_empty() {
        return None;
    }
    *fl |= 8;
    Some(r)
}

/// `FUN_1000b800`: a morph that shares its token with the morph before it.
/// The token's spellings joined are looked up whole first (pron on the first
/// morph, the rest emptied, all flagged 8). Otherwise the morph is a clitic
/// whose pron depends on the previous morph's last phones; "n't", "'ve",
/// "'em", "'re" and "'ll" match lower or all-upper case exactly, "'s", "'d"
/// and "'m" any case. Anything else leaves the morph's pron empty.
fn clitic_same_token(lex: &Lexicon, ws: &mut Ws, idx: usize) -> bool {
    if idx == 0 {
        return false;
    }
    let tok = ws.i16(m(idx) + mr::TOKEN);
    if ws.i16(m(idx - 1) + mr::TOKEN) != tok {
        return false;
    }
    let sp = m_spelling(ws, idx);
    let c0 = sp.first().copied().unwrap_or(0);
    if (c0 == 0 || b",.!?".contains(&c0)) && sp.get(1).copied().unwrap_or(0) == 0 {
        return false;
    }
    let mut start = idx - 1;
    while start > 0 && ws.i16(m(start - 1) + mr::TOKEN) == ws.i16(m(start) + mr::TOKEN) {
        start -= 1;
    }
    let mut joined: Vec<u8> = Vec::new();
    for k in start..=idx {
        joined.extend_from_slice(&m_spelling(ws, k));
    }
    if let Some(root) = cb30(lex, &joined) {
        m_set_pron(ws, start, &root);
        m_flags_or(ws, start, 8);
        for k in start + 1..=idx {
            m_set_pron(ws, k, b"");
            m_flags_or(ws, k, 8);
        }
        return true;
    }
    let prev_pron = m_pron(ws, idx - 1);
    let n = prev_pron.len();
    let last = if n >= 1 { prev_pron[n - 1] as i8 } else { 0 };
    let prev = if n >= 2 { prev_pron[n - 2] as i8 } else { 0 };
    let vowel = |c: i8| (1..=0x45).contains(&c) && phone_vowel(c as u8) != 0;
    let v_or_r = vowel(last) || (vowel(prev) && last == 0x36);
    let is = |lo: &[u8], up: &[u8]| sp.as_slice() == lo || sp.as_slice() == up;
    let pron: Vec<u8> = if stricmp(&sp, b"'s") == 0 {
        s_suffix(last as u8)
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
    } else if stricmp(&sp, b"'d") == 0 {
        let prev_sp = m_spelling(ws, idx - 1);
        let subj = super::tagger::in_list(super::tables::L_SUBJPRON.as_slice(), &prev_sp)
            || super::tagger::in_list(super::tables::L_WH.as_slice(), &prev_sp);
        if subj {
            if v_or_r { vec![0x15] } else { vec![7, 0x15] }
        } else {
            ed_suffix(last as u8)
        }
    } else if stricmp(&sp, b"'m") == 0 {
        if vowel(last) { vec![0x2c] } else { vec![7, 0x2c] }
    } else {
        Vec::new()
    };
    m_set_pron(ws, idx, &pron);
    !pron.is_empty()
}

/// `FUN_1000c710`: word ending in an apostrophe.
fn c710(lex: &Lexicon, w: &[u8], fl: &mut u8) -> Option<Vec<u8>> {
    let n = w.len();
    if n == 0 || w[n - 1] != b'\'' {
        return None;
    }
    if n > 3 && stricmp(&w[n - 3..], b"in'") == 0 {
        let mut ing = w[..n - 1].to_vec();
        ing.push(if sc(&CHAR_CLASS, w[n - 2]) & 0x80 != 0 { b'G' } else { b'g' });
        if let Some(mut r) = cb30(lex, &ing) {
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
    if let Some(r) = cb30(lex, stem) {
        *fl |= 9;
        return Some(r);
    }
    if let Some(r) = lts_f(lex, stem, fl) {
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

/// `FUN_10003b10`: the lexicon entry is a spell (4) or abbreviation (8) one.
fn lex_spell_entry(lex: &Lexicon, w: &[u8]) -> bool {
    lex.lookup_payload(w).and_then(|p| p.first().copied()).is_some_and(|f| f != 0 && (f & 4 != 0 || f & 8 != 0))
}

/// `FUN_10009dc0`: parts split at '-'/'.'. A lone "a" is the article (7)
/// only between dashes, or first before a dash after a 0xb token (`dash`);
/// other single letters and all-caps spell/abbreviation entries are spelled;
/// "the" (or a non-initial "de") before a vowel ends in IY. Other parts try
/// the lexicon, then `FUN_1000a140`, `FUN_1000c710`, `FUN_1000c040` (told
/// whether a dash precedes the part), `FUN_1000c3a0`, LTS and spelling.
fn dc0(lex: &Lexicon, ws: &Ws, idx: usize, w: &[u8], dash: bool, fl: &mut u8) -> Option<Vec<u8>> {
    let sep = w.iter().position(|&c| c == b'-').or_else(|| w.iter().position(|&c| c == b'.'))?;
    if sep == 0 || sep + 1 >= w.len() {
        return None;
    }
    let mut out: Vec<u8> = Vec::new();
    let mut the_before = false;
    let mut start = 0usize;
    let mut k = 0usize;
    for pos in 0..=w.len() {
        let c = w.get(pos).copied().unwrap_or(0);
        if c != b'-' && c != b'.' && c != 0 {
            continue;
        }
        let part = &w[start..pos];
        let ph: Vec<u8> = if part.len() == 1 {
            the_before = false;
            let dashed = if k == 0 { dash && c == b'-' } else { pos >= 2 && w[pos - 2] == b'-' && c == b'-' };
            if TOLOWER_S[part[0] as usize] == 0x61 && dashed {
                vec![7]
            } else {
                spell_f(part, fl)
            }
        } else {
            let ph = if lex_spell_entry(lex, part) && no_lower(part) {
                spell_f(part, fl)
            } else if let Some(r) = cb30(lex, part) {
                *fl |= 1;
                r
            } else {
                let d = if k == 0 { dash } else { w[start - 1] == b'-' };
                if let Some(r) = a140(lex, part, fl) {
                    r
                } else if let Some(r) = c710(lex, part, fl) {
                    r
                } else if let Some(r) = c040(lex, ws, idx, part, d, fl) {
                    r
                } else if let Some(r) = camelcase_split(lex, ws, idx, part, fl) {
                    r
                } else if let Some(r) = lts_f(lex, part, fl) {
                    r
                } else {
                    spell_f(part, fl)
                }
            };
            if the_before {
                if let (Some(&p0), Some(last)) = (ph.first(), out.last_mut()) {
                    if (1..0x46).contains(&p0) && phone_vowel(p0) != 0 && p0 != 0x43 {
                        *last = 0x26;
                    }
                }
            }
            the_before = stricmp(part, b"the") == 0 || (k > 0 && stricmp(part, b"de") == 0);
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
        return None;
    }
    *fl |= 8;
    Some(out)
}

/// `FUN_100091b0`: preset 'U' tokens, on the token's first morph only. Mode
/// 'Y' takes the token's preset phones (flag 4) and moves `*idx` to the
/// token's last morph, so the driver skips the rest. Mode 'S' spells the
/// spelling cut at the sub-token offset. Either way the sub-token (WA
/// `SUB_OFF`/`SUB_LEN` into the token text) is then appended as a clitic to
/// the pron of the morph at `*idx`; one that isn't a known clitic is read
/// without its first character (lexicon, LTS or spelling) after a 100 break.
fn u_token(ws: &mut Ws, wa: &Wa, lex: &Lexicon, idx: &mut usize) -> bool {
    let i0 = *idx;
    let tok = ws.i16(m(i0) + mr::TOKEN);
    let r = Wa::rec(tok as usize);
    if wa.u8(r + war::U_FLAG) != b'U' {
        return false;
    }
    if i0 != 0 && ws.i16(m(i0 - 1) + mr::TOKEN) == tok {
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
    let clitic = match wa.u8(r + war::CASE_MODE) {
        b'Y' => {
            let preset = wa.cstr(r + war::PHONES).to_vec();
            m_set_pron(ws, i0, &preset);
            m_flags_or(ws, i0, 4);
            let n = ws.n_morphs();
            while *idx + 1 < n && ws.i16(m(*idx) + mr::TOKEN) == ws.i16(m(*idx + 1) + mr::TOKEN) {
                *idx += 1;
            }
            sub(Wa::rec(ws.i16(m(*idx) + mr::TOKEN) as usize))
        }
        b'S' => {
            let mut sp = m_spelling(ws, i0);
            let off = wa.i32(r + war::SUB_OFF);
            let c = if off < 1 {
                Vec::new()
            } else {
                sp.truncate(off as usize);
                sub(r)
            };
            let mut fl = 0u8;
            let s = spell_f(&sp, &mut fl);
            m_set_pron(ws, i0, &s);
            m_flags_or(ws, i0, fl);
            c
        }
        _ => return false,
    };
    if clitic.is_empty() {
        return true;
    }
    let p = *idx;
    let mut pron = m_pron(ws, p);
    let n = pron.len();
    let last = if n >= 1 { pron[n - 1] as i8 } else { 0 };
    let prev = if n >= 2 { pron[n - 2] as i8 } else { 0 };
    let vowel = |c: i8| (1..=0x45).contains(&c) && phone_vowel(c as u8) != 0;
    let v_or_r = vowel(last) || (vowel(prev) && last == 0x36);
    let cl = clitic.as_slice();
    let is = |lo: &[u8], up: &[u8]| cl == lo || cl == up;
    let add: Vec<u8> = if stricmp(cl, b"'s") == 0 {
        s_suffix(last as u8)
    } else if is(b"'ve", b"'VE") {
        if vowel(last) { vec![0x41] } else { vec![7, 0x41] }
    } else if is(b"'em", b"'EM") {
        vec![7, 0x2c]
    } else if is(b"'re", b"'RE") {
        if vowel(last) { vec![0x36] } else { vec![0x1a] }
    } else if is(b"'ll", b"'LL") {
        if v_or_r { vec![0x2b] } else { vec![7, 0x2b] }
    } else if stricmp(cl, b"'d") == 0 {
        if v_or_r { vec![0x15] } else { vec![7, 0x15] }
    } else if stricmp(cl, b"'m") == 0 {
        if vowel(last) { vec![0x2c] } else { vec![7, 0x2c] }
    } else if stricmp(cl, b"'n") == 0 {
        if v_or_r { vec![0x2d] } else { vec![7, 0x2d] }
    } else if stricmp(cl, b"'ee") == 0 {
        vec![0x26]
    } else if stricmp(cl, b"'er") == 0 {
        vec![0x1a]
    } else if stricmp(cl, b"'ers") == 0 {
        vec![0x1a, 0x44]
    } else {
        let w = &cl[1..];
        let mut fl = 0u8;
        let r = match cb30(lex, w) {
            Some(r) => r,
            None => match lts_f(lex, w, &mut fl) {
                Some(r) => r,
                None => spell_f(w, &mut fl),
            },
        };
        m_flags_or(ws, p, fl);
        if r.is_empty() {
            Vec::new()
        } else {
            let mut v = vec![100];
            v.extend_from_slice(&r);
            v
        }
    };
    pron.extend_from_slice(&add);
    if !pron.is_empty() {
        m_set_pron(ws, p, &pron);
        m_flags_or(ws, p, 8);
    }
    true
}

/// `FUN_10010010`: every character an uppercase letter.
fn caps_word(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&c| sc(&CHAR_CLASS, c) & 0x80 != 0)
}

/// `FUN_1000ffd0`: no lowercase letter.
fn no_lower(s: &[u8]) -> bool {
    s.iter().all(|&c| sc(&CHAR_CLASS, c) & 0x40 == 0)
}

/// `FUN_100100d0`: a/e/i/o/u count (`y` doesn't count).
fn vowel_count(s: &[u8]) -> usize {
    s.iter().filter(|&&c| matches!(TOLOWER_S[c as usize], 0x61 | 0x65 | 0x69 | 0x6f | 0x75)).count()
}

/// The nearest morph before/after `idx` whose spelling starts with a letter
/// (or has one second), and whether that word is all caps too.
fn caps_neighbours(ws: &Ws, idx: usize) -> (Option<Vec<u8>>, bool, Option<Vec<u8>>, bool) {
    let n = ws.n_morphs();
    let wordish = |s: &[u8]| {
        let a = |c: u8| sc(&CHAR_CLASS, c) & 0xc0 != 0;
        s.first().is_some_and(|&c| a(c)) || (s.len() > 1 && a(s[1]))
    };
    let prev = (0..idx).rev().map(|j| m_spelling(ws, j)).find(|s| wordish(s));
    let next = (idx + 1..n).map(|j| m_spelling(ws, j)).find(|s| wordish(s));
    let pc = prev.as_deref().is_some_and(no_lower);
    let nc = next.as_deref().is_some_and(no_lower);
    (prev, pc, next, nc)
}

/// `L_NOSPELL` (`PTR_DAT_100783d0`, case-sensitive): all-caps words that
/// read as words even in all-caps text.
fn nospell(s: &[u8]) -> bool {
    L_NOSPELL.iter().any(|w| *w == s)
}

/// `FUN_100086c0` as the morphology driver calls it (no entry pointer, no
/// flag): should this all-caps word be spelled? An entry of class 'E' always
/// is. Otherwise a vowelless word is; a pronounceable one (`FUN_10002c70`)
/// isn't; and when the neighbouring words are all caps too (all-caps text),
/// only short words and unpronounceable ones are, except the handful in
/// `L_NOSPELL`. Class 'A' entries (lexicon abbreviations) are spelled unless
/// in all-caps context, "IS" after "Lexus" excepted.
fn should_spell(lex: &Lexicon, ws: &Ws, idx: usize) -> bool {
    let e = m(idx);
    let src = SpellSrc {
        class: m_class(ws, idx),
        has_pron: !m_pron(ws, idx).is_empty(),
        b80: m_b80(ws, idx),
        b40: ws.i16(e + mr::B40) != 0,
        b20: ws.i16(e + mr::B20) != 0,
        b10: ws.i16(e + mr::B10) != 0,
    };
    spell_test(lex, ws, idx, &m_spelling(ws, idx), &src)
}

/// The lexicon fields `FUN_100086c0` reads: from the morph (driver call)
/// or from a decoded entry (the camelCase splitter's call).
struct SpellSrc {
    class: u8,
    has_pron: bool,
    b80: bool,
    b40: bool,
    b20: bool,
    b10: bool,
}

impl SpellSrc {
    fn of(e: &crate::lexicon::LexEntry) -> SpellSrc {
        SpellSrc {
            class: e.class_char,
            has_pron: !e.prons.is_empty(),
            b80: e.b80 != 0,
            b40: e.b40 != 0,
            b20: e.b20 != 0,
            b10: e.b10 != 0,
        }
    }
}

/// `FUN_100086c0(sp, morphs, idx, entry, 1)`: a single letter or a class
/// 'E' entry is spelled unless it is "Wi" with a lowercase letter; anything
/// else takes the ordinary test.
fn should_spell_part(lex: &Lexicon, ws: &Ws, idx: usize, sp: &[u8], e: &crate::lexicon::LexEntry) -> bool {
    if (sp.len() == 1 || e.class_char == b'E') && (stricmp(b"Wi", sp) != 0 || no_lower(sp)) {
        return true;
    }
    spell_test(lex, ws, idx, sp, &SpellSrc::of(e))
}

fn spell_test(lex: &Lexicon, ws: &Ws, idx: usize, sp: &[u8], src: &SpellSrc) -> bool {
    if !caps_word(sp) {
        return false;
    }
    let class = src.class;
    if class == b'E' {
        return true;
    }
    let (b40, b20, b10) = (src.b40, src.b20, src.b10);
    let len = sp.len();
    let (prev, pc, next, nc) = caps_neighbours(ws, idx);
    if class == b'A' {
        let caps_ctx = match (&prev, &next) {
            (None, None) => false,
            (None, Some(_)) => nc,
            (Some(_), None) => pc,
            (Some(_), Some(_)) => pc && nc,
        };
        if !caps_ctx {
            if prev.as_deref().is_some_and(|p| stricmp(p, b"Lexus") == 0) && sp == b"IS" {
                return true;
            }
            if !nospell(sp) {
                return true;
            }
        }
        if !src.has_pron && !src.b80 {
            return false;
        }
        if len > 2 {
            return false;
        }
        return !super::lts::lts_valid(lex, sp);
    }
    if src.has_pron || src.b80 {
        return false;
    }
    if vowel_count(sp) == 0 {
        return true;
    }
    if len > 2 && super::lts::lts_valid(lex, sp) {
        return false;
    }
    if b40 || b20 {
        return true;
    }
    let caps_ctx = match (&prev, &next) {
        (None, None) => false,
        (None, Some(_)) => nc,
        (Some(_), None) => pc,
        (Some(_), Some(_)) => pc && nc,
    };
    if !caps_ctx && !nospell(sp) {
        return true;
    }
    if b10 {
        return false;
    }
    if len < 3 {
        return true;
    }
    !super::lts::lts_valid(lex, sp)
}

/// `FUN_10008cc0`: does `word` contain a genuine upper/lower case transition
/// (an Upper immediately followed by a lower, or a lower immediately
/// followed by an Upper -- non-letters don't reset the state, so a
/// transition can span a digit/punct run), ignoring the second read's own
/// result value (only used by callers as a `strlen`-position start, which
/// `camelcase_split` doesn't need) -- but always false for a `"Mac"`/`"Mc"`
/// prefix (case-insensitive, `strnicmp`).
fn is_mixed_case(word: &[u8]) -> bool {
    if strnicmp(word, b"mac", 3) == 0 || strnicmp(word, b"mc", 2) == 0 {
        return false;
    }
    let mut state = 0i8; // 0 none yet, 1 last-was-upper, -1 last-was-lower
    for &c in word {
        let cl = sc(&CHAR_CLASS, c);
        if cl & 0x80 != 0 {
            if state == -1 {
                return letters_only(word);
            }
            state = 1;
        } else if cl & 0x40 != 0 {
            if state == 1 {
                return letters_only(word);
            }
            state = -1;
        }
    }
    false
}

/// The rest of `FUN_10008cc0`: every character a letter, or an apostrophe
/// in `'s` + Upper / `s'` + Upper.
fn letters_only(w: &[u8]) -> bool {
    let n = w.len();
    let letter = |c: u8| sc(&CHAR_CLASS, c) & 0xc0 != 0;
    let upper = |c: u8| sc(&CHAR_CLASS, c) & 0x80 != 0;
    (0..n).all(|u| {
        letter(w[u]) || (w[u] == b'\'' && ((u + 2 < n && w[u + 1] == b's' && upper(w[u + 2])) || (u >= 1 && u + 1 < n && w[u - 1] == b's' && upper(w[u + 1]))))
    })
}

/// `FUN_1000c860`: the next camelCase "part" of `word` starting at `*pos`,
/// advancing `*pos` to where the next part should start. Splits before an
/// Upper that follows a lower (`fooBar` -> `foo`|`Bar`), and before the
/// second of two Uppers when the letter after *that* is lower (`FOOBar` ->
/// `FOO`|`Bar`, the acronym-then-word case) -- both read off the *previous
/// output byte*, not the previous source byte, so a run of non-letters
/// between two letters doesn't itself trigger a split. Also glues a
/// following `"'s"` onto the current part when it's followed by an Upper
/// letter, and ends the part (without consuming) at a bare `'` that follows
/// an `s` when the next letter is Upper (`Jones'Store` -> `Jones`|`Store`,
/// keeping the apostrophe out of both parts).
fn split_camel_part(word: &[u8], pos: &mut usize) -> Vec<u8> {
    let n = word.len();
    let mut out: Vec<u8> = Vec::new();
    let mut i = *pos;
    loop {
        if i >= n {
            *pos = i;
            return out;
        }
        let c = word[i];
        if let Some(&prev) = out.last() {
            let prev_cl = sc(&CHAR_CLASS, prev);
            let cur_cl = sc(&CHAR_CLASS, c);
            if i + 1 < n && prev_cl & 0x80 != 0 && cur_cl & 0x80 != 0 && sc(&CHAR_CLASS, word[i + 1]) & 0x40 != 0 {
                *pos = i;
                return out;
            }
            if prev_cl & 0x40 != 0 && cur_cl & 0x80 != 0 {
                *pos = i;
                return out;
            }
        }
        if c == b'\'' {
            if i + 2 < n && word[i + 1] == b's' && sc(&CHAR_CLASS, word[i + 2]) & 0x80 != 0 {
                out.push(word[i]);
                out.push(word[i + 1]);
                *pos = i + 2;
                return out;
            }
            if out.last() == Some(&b's') && i + 1 < n && sc(&CHAR_CLASS, word[i + 1]) & 0x80 != 0 {
                *pos = i + 1;
                return out;
            }
        }
        out.push(c);
        i += 1;
    }
}

/// `FUN_1000c3a0`: camelCase split. Each part (`FUN_1000c860`) takes the
/// lexicon pron (2+ letters), LTS when the entry asks for it, spelling when
/// `FUN_100086c0(.., entry, 1)` says so ("NWS" in "NWSOmaha"), the suffix
/// stripper (best-effort `suffix_strip`), "X's", then LTS or spelling. The
/// last two don't count as resolving the part: a word whose parts all fall
/// through to them is not split.
///
/// A word only reaches here once every other chain step above has already
/// failed AND its pron was still empty going in (see the driver's own
/// gate) -- which is why an ordinary capitalized word that happens to
/// *already be a lexicon entry itself* (e.g. `"GoodNight"`, live-traced:
/// `dict_LookupLexicon` finds it directly during the initial token lookup,
/// before morphology ever runs) never gets here at all, even though
/// `FUN_1000c860` would happily split it into two other real words.
fn camelcase_split(lex: &Lexicon, ws: &Ws, idx: usize, word: &[u8], fl: &mut u8) -> Option<Vec<u8>> {
    if !is_mixed_case(word) {
        return None;
    }
    let n = word.len();
    let mut pos = 0usize;
    let mut out: Vec<u8> = Vec::new();
    let mut parts = 0usize;
    let mut resolved = false;
    loop {
        let part = split_camel_part(word, &mut pos);
        if parts == 0 && pos == n {
            break;
        }
        parts += 1;
        let entry = lex.lookup_payload(&part).map(decode_payload).unwrap_or_default();
        let ph: Vec<u8> = if part.len() >= 2 && !entry.prons.is_empty() {
            resolved = true;
            *fl |= 1;
            entry.prons[0].1.clone()
        } else if let Some(r) = if entry.b80 != 0 { lts_f(lex, &part, fl) } else { None } {
            resolved = true;
            r
        } else if should_spell_part(lex, ws, idx, &part, &entry) {
            resolved = true;
            spell_f(&part, fl)
        } else if let Some(r) = a140(lex, &part, fl) {
            resolved = true;
            r
        } else if let Some(r) = possessive_part(lex, &part) {
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

/// `FUN_1000c3a0`'s "X's" part: the lexicon pron of X plus Z, IH Z or D
/// by its last phone.
fn possessive_part(lex: &Lexicon, part: &[u8]) -> Option<Vec<u8>> {
    let l = part.len();
    if l < 2 || &part[l - 2..] != b"'s" {
        return None;
    }
    let mut ph = cb30(lex, &part[..l - 2])?;
    let last = *ph.last()?;
    match last {
        0x20 | 0x2a | 0x35 | 0x39 | 0x3a => ph.push(0x37),
        0x14 | 0x29 | 0x37 | 0x38 | 0x44 | 0x45 => ph.extend_from_slice(&[0x23, 0x44]),
        _ => ph.push(0x44),
    }
    Some(ph)
}

const NUMWORD_MIN: [(&[u8], usize); 0] = [];
use super::tables::L_NUMWORD;
use super::tagger::bsearch_ci;

/// `FUN_10008550`.
fn is_numword(w: &[u8]) -> bool {
    let _ = NUMWORD_MIN;
    bsearch_ci(&L_NUMWORD, w) >= 0
}

/// `FUN_10008580`: verb context for "use"/"close" (let / modal pairs / do /
/// to, including the morph0/morph1 quirk).
fn verb_context(ws: &Ws, i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let sp = |k: usize| m_spelling(ws, k);
    if i > 1 {
        let a = sp(i - 2);
        let b = sp(i - 1);
        if stricmp(&a, b"let") == 0 {
            return true;
        }
        if modal_pair(&a, Some(&b)) {
            return true;
        }
        if modal_pair(&sp(0), Some(&sp(1))) {
            return true;
        }
        if modal_pair(&a, Some(b"")) && (stricmp(&b, b"n't") == 0 || stricmp(&b, b"not") == 0) {
            return true;
        }
    }
    let c = sp(i - 1);
    if stricmp(&c, b"do") == 0 || stricmp(&c, b"to") == 0 {
        return true;
    }
    modal_pair(&c, Some(b""))
}

/// `dict_CountHyphenParts` (0x1000c9c0, `allow_hyphen=1`): 0 on an invalid
/// character, else hyphen count + 1.
fn count_hyphen_parts(s: &[u8]) -> i32 {
    let mut parts = 0i32;
    for &c in s {
        let letter = sc(&CHAR_CLASS, c) & 0xc0 != 0;
        if !letter {
            if c == b'\'' {
                continue;
            }
            if c == b'-' {
                parts += 1;
                continue;
            }
            return 0;
        }
    }
    parts + 1
}

/// `FUN_1000ca30`: raw dash count + 1 (no validation).
fn dash_parts(s: &[u8]) -> i32 {
    s.iter().filter(|&&c| c == b'-').count() as i32 + 1
}

/// `dict_ApplyExceptPron` (0x1000ca50).
fn apply_except_pron(ws: &mut Ws, start_idx: usize, value: &[u8], count: i32) {
    if count <= 0 {
        return;
    }
    let mut idx = start_idx;
    let mut vi = 0usize;
    let mut off = 0usize;
    let mut remaining = count;
    loop {
        let mut budget = dash_parts(&m_spelling(ws, idx));
        let mut dcount = 0i32;
        loop {
            let c = *value.get(vi).unwrap_or(&0);
            if c == 0 {
                break;
            }
            vi += 1;
            if c == 100 {
                dcount += 1;
                if dcount < budget {
                    ws.set_u8(m(idx) + mr::PRON + off, 100);
                    off += 1;
                } else {
                    ws.set_u8(m(idx) + mr::PRON + off, 0);
                    ws.set_i16(m(idx) + mr::FLAGS, 2);
                    idx += 1;
                    off = 0;
                    dcount = 0;
                    budget = dash_parts(&m_spelling(ws, idx));
                }
            } else {
                ws.set_u8(m(idx) + mr::PRON + off, c);
                off += 1;
            }
        }
        ws.set_u8(m(idx) + mr::PRON + off, 0);
        ws.set_i16(m(idx) + mr::FLAGS, 2);
        remaining -= 1;
        if remaining == 0 {
            break;
        }
    }
}

/// `dict_MatchHyphenatedExcept` (0x10008dc0): grows a hyphen-joined spelling
/// across consecutive morphs starting at `i`, trying the longest exceptdict
/// match. On a match, pours the pronunciation into the consumed morphs and
/// returns how many morphs it consumed (the driver skips `post_rules` for
/// all of them and resumes at `i + consumed`).
fn match_hyphenated_except(ws: &mut Ws, lex: &Lexicon, i: usize, n: usize) -> Option<i32> {
    if i >= n {
        return None;
    }
    let mut joined: Vec<u8> = Vec::new();
    let mut total_parts = 0i32;
    let mut consumed = 0i32;
    let mut best: Option<(Vec<u8>, i32)> = None;
    let mut k = i;
    loop {
        let sp = m_spelling(ws, k);
        let own_parts = count_hyphen_parts(&sp);
        if own_parts == 0 {
            if k == i {
                return None;
            }
            break;
        }
        let parts = total_parts + own_parts;
        if k == i {
            joined = sp.clone();
        } else {
            joined.push(b'-');
            joined.extend_from_slice(&sp);
        }
        if parts < lex.exc.lo {
            consumed += 1;
        } else {
            if parts > lex.exc.hi {
                break;
            }
            consumed += 1;
            match lex.exc.find(&joined, parts) {
                Some(v) => best = Some((v.to_vec(), consumed)),
                None => {
                    let accent = ws.u8(m(k) + mr::ACCENT);
                    if accent == b'X' {
                        let mut hj = b"h'".to_vec();
                        hj.extend_from_slice(&joined);
                        if let Some(v) = lex.exc.find(&hj, parts) {
                            best = Some((v.to_vec(), consumed));
                        }
                    }
                }
            }
        }
        k += 1;
        total_parts = parts;
        if k >= n {
            break;
        }
    }
    match best {
        Some((val, cnt)) => {
            apply_except_pron(ws, i, &val, cnt);
            Some(cnt)
        }
        None => None,
    }
}

/// `FUN_10007520`: morphology driver.
pub fn morphology(ws: &mut Ws, wa: &Wa, lex: &Lexicon) {
    let n = ws.n_morphs();
    let mut i = 0usize;
    while i < n {
        let flags0: u8 = if !m_pron(ws, i).is_empty() { 1 } else { 0 };
        ws.set_u8(m(i) + mr::FLAGS, flags0);

        let u_handled = u_token(ws, wa, lex, &mut i);

        let tok = ws.i16(m(i) + mr::TOKEN) as usize;
        let r = Wa::rec(tok);
        let skip_rest = if wa.u8(r + war::FORCED_POS) != 0xff && !m_pron(ws, i).is_empty() {
            true
        } else if wa.u8(r + war::U_FLAG) != b'U' && !wa.cstr(r + war::PHONES).is_empty() {
            true
        } else {
            false
        };

        if !skip_rest && !u_handled {
            if let Some(consumed) = match_hyphenated_except(ws, lex, i, n) {
                // The driver skips post_rules for every morph the exceptdict
                // match consumed and resumes right after them.
                i += (consumed - 1).max(0) as usize;
                i += 1;
                continue;
            }
            let mut handled = false;
            {
                let class = m_class(ws, i);
                let u_flag = wa.u8(r + war::U_FLAG);
                let casemode = wa.u8(r + war::CASE_MODE);
                if case_mode(ws, lex, i, class, u_flag, casemode) {
                    handled = true;
                } else if should_spell(lex, ws, i) {
                    let sp = m_spelling(ws, i);
                    let s = spell(&sp);
                    m_set_pron(ws, i, &s);
                    if !s.is_empty() {
                        // Corpus-verified (`suffix_004`, "J.R.R"): a
                        // separator-containing spelling (assembled from
                        // multiple letter/segment groups, not spelt as one
                        // flat run) also gets bit 0x08 -- the same "this
                        // chain step did non-trivial multi-part work"
                        // marker `hyphen_dot_split`/`suffix_strip`/etc. all
                        // set on success, just reached from inside `spell`
                        // itself here instead of a separate function.
                        let multi = sp.iter().any(|&c| c == b'.' || c == b'-' || c == b'\'');
                        m_flags_or(ws, i, 0x20 | if multi { 8 } else { 0 });
                    }
                    handled = true;
                } else if m_pron(ws, i).is_empty() {
                    let sp = m_spelling(ws, i);
                    // Corpus-verified (`suffix_004`, "well-known" -- B80
                    // lexicon hit, no direct pron): "or FUN_10003110 if the
                    // word has '-'" means this word's
                    // *own* spelling, not (as previously coded) whether the
                    // *previous* morph belongs to a hyphen-class token.
                    let hy = sp.contains(&b'-');
                    if m_b80(ws, i) {
                        let res = if hy { Some(lts_hyphenated(lex, &sp)) } else { letter_to_sound(lex, &sp) };
                        if let Some((ph, f)) = res {
                            m_set_pron(ws, i, &ph);
                            m_flags_or(ws, i, f | 0x40);
                            handled = true;
                        }
                    }
                    if !handled {
                        // Every step ORs into the morph's own flag byte
                        // (`morph+4`), so bits set by steps that go on to
                        // fail stay set.
                        let after_b = i >= 1 && wa.i32(Wa::rec(ws.i16(m(i - 1) + mr::TOKEN) as usize) + war::TOKEN_CLASS) == 0xb;
                        let mut fl = 0u8;
                        let mut ph = dc0(lex, ws, i, &sp, after_b, &mut fl);
                        if ph.is_none() {
                            ph = a140(lex, &sp, &mut fl);
                        }
                        if ph.is_none() {
                            ph = ise_retry(lex, &sp);
                            if ph.is_some() {
                                fl |= 9;
                            }
                        }
                        if ph.is_none() {
                            ph = c040(lex, ws, i, &sp, after_b, &mut fl);
                        }
                        let mut done = ph.is_some();
                        if !done && clitic_same_token(lex, ws, i) {
                            fl |= 8;
                            done = true;
                        }
                        if !done {
                            ph = camelcase_split(lex, ws, i, &sp, &mut fl);
                            if ph.is_none() {
                                ph = c710(lex, &sp, &mut fl);
                            }
                            if ph.is_none() {
                                ph = lts_f(lex, &sp, &mut fl);
                            }
                            if ph.is_none() {
                                ws.set_u8(m(i) + mr::CLASS_CHAR, b'E');
                                ph = Some(spell_f(&sp, &mut fl));
                            }
                        }
                        if let Some(p) = ph {
                            m_set_pron(ws, i, &p);
                        }
                        m_flags_or(ws, i, fl);
                        handled = true;
                    }
                }
            }
            let _ = handled;
        }

        // The driver's skip exits jump straight to the loop increment.
        if !skip_rest {
            post_rules(ws, wa, i);
        }
        i += 1;
    }

    // Final pass: "the"/"de" before a pron starting with a vowel other than Y
    // (0x43) becomes [p0, IY].
    let n = ws.n_morphs();
    for i in 1..n {
        let p0 = ws.u8(m(i) + mr::PRON);
        if !(1..=0x45).contains(&p0) || super::lts::phone_vowel(p0) == 0 {
            continue;
        }
        let sp_prev = m_spelling(ws, i - 1);
        if (stricmp(&sp_prev, b"the") == 0 || stricmp(&sp_prev, b"de") == 0) && p0 != 0x43 {
            ws.set_u8(m(i - 1) + mr::PRON + 1, 0x26);
            ws.set_u8(m(i - 1) + mr::PRON + 2, 0);
            m_flags_or(ws, i - 1, 8);
        }
    }
}

/// `FUN_10009030`: case-mode pass.
fn case_mode(ws: &mut Ws, lex: &Lexicon, idx: usize, class: u8, u_flag: u8, casemode: u8) -> bool {
    let spell_it = |ws: &mut Ws| {
        let w = m_spelling(ws, idx);
        let s = spell(&w);
        m_set_pron(ws, idx, &s);
        if !s.is_empty() {
            m_flags_or(ws, idx, 0x20);
        }
    };
    let try_lts = |ws: &mut Ws| -> bool {
        let w = m_spelling(ws, idx);
        if let Some((ph, f)) = letter_to_sound(lex, &w) {
            m_set_pron(ws, idx, &ph);
            m_flags_or(ws, idx, f);
            true
        } else {
            false
        }
    };
    if casemode != b'S' {
        if casemode != b'C' {
            return false;
        }
        if u_flag != b'I' {
            if u_flag == b'M' {
                if class == b'E' || class == b'A' {
                    spell_it(ws);
                    return true;
                }
                if !m_pron(ws, idx).is_empty() {
                    m_flags_or(ws, idx, 1);
                    return true;
                }
                if m_b80(ws, idx) && try_lts(ws) {
                    return true;
                }
            } else if u_flag == b'E' {
                if class == b'E' || class == b'A' {
                    spell_it(ws);
                    return true;
                }
                if !m_pron(ws, idx).is_empty() {
                    m_flags_or(ws, idx, 1);
                    return true;
                }
                if m_b80(ws, idx) && try_lts(ws) {
                    return true;
                }
            } else {
                return false;
            }
            spell_it(ws);
            return true;
        }
        if class != b'E' && class != b'A' {
            if !m_pron(ws, idx).is_empty() {
                m_flags_or(ws, idx, 1);
                return true;
            }
            if m_b80(ws, idx) && try_lts(ws) {
                return true;
            }
            if !try_lts(ws) {
                spell_it(ws);
            }
            return true;
        }
        spell_it(ws);
        return true;
    }
    spell_it(ws);
    true
}

/// `FUN_1000cc60`: window search over morph spellings, like
/// `tagger::phrase_in_window` but addressing morphs instead of candidates,
/// with an extra 'B' mode that halves the window and searches both
/// directions into one combined, position-ordered list.
fn morph_list_window(ws: &Ws, i: usize, n: usize, mode: u8, window: usize, list: &[&[u8]]) -> i32 {
    let mut win = window.min(0x14) as i64;
    if mode == b'B' {
        win = (win + (win & 1)) / 2;
    }
    let i = i as i64;
    let n = n as i64;
    let mut words: Vec<Vec<u8>> = Vec::new();
    if i > 0 && (mode == b'L' || mode == b'B') {
        let cnt = win.min(i);
        if cnt > 0 {
            let from = i - cnt;
            for k in 0..cnt {
                words.push(m_spelling(ws, (from + k) as usize));
            }
        }
    }
    let after = n - i - 1;
    if after > 0 && (mode == b'R' || mode == b'B') {
        let cnt = win.min(after);
        if cnt > 0 {
            for k in 0..cnt {
                words.push(m_spelling(ws, (i + 1 + k) as usize));
            }
        }
    }
    for (k, s) in words.iter().enumerate() {
        let len = s.len();
        let (mut lo, mut hi) = (0i32, list.len() as i32 - 1);
        let mut mid = hi;
        while lo <= hi {
            mid /= 2;
            let e = list[mid as usize];
            let r = super::tagger::strnicmp(e, s, len);
            if r == 0 {
                let tail = *e.get(len).unwrap_or(&0);
                if tail == 0 || tail == b' ' {
                    return k as i32;
                }
                hi = mid - 1;
            } else if r > 0 {
                hi = mid - 1;
            } else {
                lo = mid + 1;
            }
            mid = hi + lo;
        }
    }
    -1
}

/// `read` in a past-participle context (L_BE/L_HAVE before it, "he"/"she"
/// before it, "I"/an object pronoun before it within a time window, an
/// L_WH word before it, or "get"/"got" before it not immediately followed by
/// "to") re-colours the second phone from IY1 to EH1.
fn read_rule(ws: &mut Ws, i: usize) {
    use super::tables::{L_BE, L_HAVE, L_OBJPRON, L_TIME, L_WH};
    if stricmp(&m_spelling(ws, i), b"read") != 0 {
        return;
    }
    if m_pron(ws, i).get(1) != Some(&0x27) {
        return;
    }
    if i == 0 {
        return;
    }
    let sp = |k: usize| m_spelling(ws, k);
    let back_list = |list: &[&[u8]]| -> bool {
        if bsearch_ci(list, &sp(i - 1)) >= 0 {
            return true;
        }
        i > 1 && (bsearch_ci(list, &sp(i - 2)) >= 0 || stricmp(&sp(i - 2), b"not") == 0)
    };
    let mut apply = back_list(&L_BE) || back_list(&L_HAVE);
    if !apply {
        if stricmp(&sp(i - 1), b"he") == 0 || stricmp(&sp(i - 1), b"she") == 0 {
            apply = true;
        } else if stricmp(&sp(i - 1), b"i") == 0 || bsearch_ci(&L_OBJPRON, &sp(i - 1)) >= 0 {
            let n = ws.n_morphs();
            if morph_list_window(ws, i, n, b'B', 0x14, &L_TIME) >= 0 {
                apply = true;
            }
        }
    }
    if !apply && bsearch_ci(&L_WH, &sp(i - 1)) >= 0 {
        apply = true;
    }
    if !apply {
        let mut got_get = i > 2 && matches!(sp(i - 2).as_slice(), b"get" | b"got" | b"gets");
        // "gets" one word back deliberately does not count (engine quirk).
        if !got_get && i > 1 && matches!(sp(i - 1).as_slice(), b"get" | b"got") {
            got_get = true;
        }
        if got_get && stricmp(&sp(i - 1), b"to") != 0 {
            apply = true;
        }
    }
    if apply {
        let o = m(i) + mr::PRON + 1;
        ws.set_u8(o, 0x18); // EH1
        m_flags_or(ws, i, 8);
    }
}

/// `LAB_1000792a`'s "a": the article (7) or the letter name (0x1e).
fn a_rule(ws: &mut Ws, i: usize, sp: &[u8]) {
    use super::tables::{L_BE, L_CONJ};
    let n = ws.n_morphs();
    let p0 = ws.u8(m(i) + mr::PRON);
    let letter = |c: u8| sc(&CHAR_CLASS, c) & 0xc0 != 0;
    let upper = |c: u8| sc(&CHAR_CLASS, c) & 0x80 != 0;
    let next = if i + 1 < n { Some(m_spelling(ws, i + 1)) } else { None };
    let article = if i == 0 && upper(sp[0]) && (0x1d..=0x1f).contains(&p0) {
        match &next {
            None => false,
            Some(nx) => {
                n >= 2
                    && bsearch_ci(&L_CONJ, &m_spelling(ws, 1)) < 0
                    && bsearch_ci(&L_BE, nx) < 0
                    && nx.first().is_some_and(|&c| letter(c))
            }
        }
    } else {
        sc(&CHAR_CLASS, sp[0]) & 0x40 != 0 && (0x1d..=0x1f).contains(&p0) && next.as_ref().and_then(|nx| nx.first().copied()).is_some_and(upper)
    };
    if article {
        ws.set_u8(m(i) + mr::PRON, 7);
    } else {
        let same_tok = i + 1 < n && ws.i16(m(i) + mr::TOKEN) == ws.i16(m(i + 1) + mr::TOKEN);
        if !(same_tok && next.as_deref().is_some_and(|nx| stricmp(nx, b"'s") == 0)) {
            return;
        }
        ws.set_u8(m(i) + mr::PRON, 0x1e);
    }
    m_flags_or(ws, i, 8);
}

/// `LAB_1000792a` onward: post-rules run on every morph after the main
/// dispatch, regardless of how the pron was produced.
fn post_rules(ws: &mut Ws, wa: &Wa, i: usize) {
    use super::tables::{L_BOW, L_LEAD};
    let sp = m_spelling(ws, i);
    let tok = ws.i16(m(i) + mr::TOKEN) as usize;
    if stricmp(&sp, b"a") == 0 && wa.u8(Wa::rec(tok) + war::CASE_MODE) != b'S' {
        a_rule(ws, i, &sp);
    }
    read_rule(ws, i);
    if stricmp(&sp, b"use") == 0 || stricmp(&sp, b"close") == 0 {
        let off = if stricmp(&sp, b"use") == 0 { mr::PRON + 2 } else { mr::PRON + 3 };
        if ws.u8(m(i) + off) == 0x37 && verb_context(ws, i) {
            ws.set_u8(m(i) + off, 0x44);
            m_flags_or(ws, i, 8);
        }
    }
    if i > 0 && is_numword(&m_spelling(ws, i - 1)) && stricmp(&sp, b"minute") == 0 {
        let mut p = [0u8; 5];
        p.copy_from_slice(&[0x2c, 0x24, 0x2d, 0x23, 0x39]);
        m_set_pron(ws, i, &p);
        m_flags_or(ws, i, 8);
    }
    if stricmp(&sp, b"bow") == 0 {
        let n = ws.n_morphs();
        let found = morph_list_window(ws, i, n, b'B', 5, &L_BOW) >= 0;
        ws.set_u8(m(i) + mr::PRON + 1, if found { 0x30 } else { 0x0e });
        m_flags_or(ws, i, 8);
    }
    if stricmp(&sp, b"lead") == 0 && m_pron(ws, i).get(1) == Some(&0x27) {
        let n = ws.n_morphs();
        if morph_list_window(ws, i, n, b'B', 0xa, &L_LEAD) >= 0 {
            ws.set_u8(m(i) + mr::PRON + 1, 0x18);
            m_flags_or(ws, i, 8);
        }
    }
    if stricmp(&sp, b"mouth") == 0 && m_pron(ws, i).get(2) == Some(&0x16) && i > 1 {
        let n = ws.n_morphs();
        if morph_list_window(ws, i, n, b'R', 2, &super::tables::L_DET) >= 0 {
            ws.set_u8(m(i) + mr::PRON + 2, 0x3a);
            m_flags_or(ws, i, 8);
        }
    }
}

