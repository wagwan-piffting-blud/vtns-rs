//! Letter-to-sound (`fe_LetterToSound` 0x10002f10): one `atmt.tree3` CART per
//! letter predicts an output code right to left from a 7-letter window and the
//! three codes already predicted; codes expand to phones (`FUN_100024f0`), then
//! `fe_PostProcessPhones` r-colours vowels, turns final IH/UH into IY/UW and
//! inserts schwa before syllabic L/M/N. Also the word validity check
//! (`FUN_10002c70`, consonant clusters against `chc_sort.txt2`), letter spelling
//! (`FUN_10009cd0`) and hyphenated LTS (`FUN_10003110`).

use super::sc;
use super::tables::{CHAR_CLASS, D5E, D94, D9E, F5A, SPELL, TOLOWER_S, TOUPPER_S};
use crate::lexicon::{stricmp, Lexicon};

fn cls(c: u8) -> u8 {
    sc(&CHAR_CLASS, c)
}

pub fn is_letter(c: u8) -> bool {
    cls(c) & 0xc0 != 0
}

/// Letter-is-vowel through the lower-case map (`DAT_10077d5e[TOLOWER[c]]`).
pub fn letter_vowel(c: u8) -> bool {
    let l = TOLOWER_S[c as usize] as isize;
    d5e(l) != 0
}

pub fn d5e(i: isize) -> i16 {
    if i < 0 || i as usize >= D5E.len() {
        0
    } else {
        D5E[i as usize]
    }
}

/// `DAT_10077d94[c]`: phone code is a vowel (1) or Y (1 too).
pub fn phone_vowel(c: u8) -> i16 {
    d5e(D94 as isize + c as i8 as isize)
}

pub fn upper(c: u8) -> u8 {
    if cls(c) & 0x40 != 0 {
        TOUPPER_S[c as usize] as u8
    } else {
        c
    }
}

/// `FUN_10002680`: cluster lookup with the required flag bits.
fn cluster_ok(lex: &Lexicon, s: &[u8], mode: u16) -> bool {
    let i = lex.chc.find(s);
    if i < 0 {
        return false;
    }
    let v = &lex.chc.vals[i as usize];
    let bit = |k: usize| v.get(k) == Some(&b'1');
    let mut f: u16 = 0;
    if bit(0) {
        f = 8;
    }
    if bit(1) {
        f |= 4;
    }
    if bit(2) {
        f |= 2;
    }
    if bit(3) {
        f |= 1;
    }
    f & mode == mode
}

/// `FUN_100026f0(cluster, len, word_len, mode)`: mode 8 whole word, 4 initial,
/// 2 medial (tries every split into coda + onset), 1 final.
fn cluster_valid(lex: &Lexicon, cl: &[u8], wlen: usize, mode: u16) -> bool {
    let len = cl.len();
    if cl.is_empty() || len > 8 {
        return false;
    }
    if cluster_ok(lex, cl, mode) {
        return true;
    }
    let split = |a: usize| cluster_ok(lex, &cl[..a], 1) && cluster_ok(lex, &cl[a..], 4);
    if len < 3 || mode != 1 {
        if mode == 2 {
            let order: &[usize] = match len {
                4 => &[2],
                5 => &[2, 3],
                6 => &[3, 2, 4],
                7 => &[2, 3, 4, 5],
                8 => &[2, 3, 4, 5, 6],
                _ => &[],
            };
            for &a in order {
                if split(a) {
                    return true;
                }
            }
        }
    } else if TOLOWER_S[cl[len - 1] as usize] == b's' as i16 && cluster_ok(lex, &cl[..len - 1], 1) {
        return true;
    }
    if wlen > 6 {
        if len < 5 {
            if mode != 8 {
                return true;
            }
        } else if mode == 2 {
            return true;
        }
    }
    false
}

/// `FUN_10002c70`: letters and apostrophes only, a vowel letter somewhere,
/// and every consonant cluster attested.
pub fn lts_valid(lex: &Lexicon, w: &[u8]) -> bool {
    let n = w.len();
    if n < 1 {
        return false;
    }
    let mut vowel = false;
    let mut cl: Vec<u8> = Vec::new();
    for i in 0..n {
        let c = w[i];
        if !is_letter(c) && c != b'\'' {
            return false;
        }
        if is_letter(c) {
            if !letter_vowel(c) {
                cl.push(c);
                if i == n - 1 && cl.len() > 1 {
                    let mode = if i + 1 != cl.len() { 1 } else { 8 };
                    if !cluster_valid(lex, &cl, n, mode) {
                        return false;
                    }
                    cl.clear();
                }
                continue;
            }
            vowel = true;
            if cl.len() >= 2 {
                let mode = if i != cl.len() { 2 } else { 4 };
                if !cluster_valid(lex, &cl, n, mode) {
                    return false;
                }
            }
        }
        cl.clear();
    }
    vowel
}

/// `FUN_10002d70`: letter feature code.
pub(crate) fn letter_code(c: u8) -> i16 {
    if c.is_ascii_uppercase() {
        return c as i16 - 0x40;
    }
    if c.is_ascii_lowercase() {
        return TOUPPER_S[c as usize] - 0x40;
    }
    if c == b'\'' {
        0x1c
    } else {
        0
    }
}

/// `FUN_100024f0`: expand LTS output codes to phone codes, then post-process.
fn expand(codes: &[u8]) -> Vec<u8> {
    let mut d: Vec<u8> = Vec::new();
    for &c in codes {
        match c {
            1 => {}
            9 => d.extend_from_slice(&[7, 0x2b]),
            10 => d.extend_from_slice(&[7, 0x2c]),
            11 => d.extend_from_slice(&[7, 0x2d]),
            12 => d.push(0x1a),
            b'\'' => d.extend_from_slice(&[0x21, 0x44]),
            b'(' => d.extend_from_slice(&[0x21, 0x45]),
            b'*' => d.extend_from_slice(&[0x22, 0x42]),
            b'3' => d.extend_from_slice(&[0x2a, 0x37]),
            b'4' => d.extend_from_slice(&[0x2a, 0x38]),
            b'E' => d.extend_from_slice(&[0x39, 0x37]),
            b'N' => d.extend_from_slice(&[0x42, 8]),
            b'O' => d.extend_from_slice(&[0x42, 9]),
            b'Q' => d.extend_from_slice(&[0x43, 7]),
            b'R' => d.extend_from_slice(&[0x43, 0x3b]),
            b'S' => d.extend_from_slice(&[0x43, 0x3c]),
            b'T' => d.extend_from_slice(&[0x43, 0x3d]),
            b'U' => d.extend_from_slice(&[0x43, 0x3e]),
            b'V' => d.extend_from_slice(&[0x43, 0x3f]),
            b'W' => d.extend_from_slice(&[0x43, 0x40]),
            _ => {
                if (c as i8) < 1 || c > b'Y' {
                    // Terminated here without post-processing; the prefix stays.
                    return d;
                }
                d.push(d5e(F5A as isize + c as isize) as u8);
            }
        }
    }
    if post_process(&mut d) {
        d
    } else {
        Vec::new()
    }
}

fn in_rng(c: u8, lo: u8, hi: u8) -> bool {
    (c as i8) >= lo as i8 && (c as i8) <= hi as i8
}

/// `fe_PostProcessPhones` (0x10002200).
pub fn post_process(p: &mut Vec<u8>) -> bool {
    if p.is_empty() {
        return false;
    }
    let len = p.len();
    let mut tmp: Vec<u8> = Vec::with_capacity(len + 4);
    let mut rcol = false;
    for i in 0..len {
        if tmp.len() > 0x40 {
            break;
        }
        let c = p[i];
        if i < 1 || !in_rng(c, 0x1a, 0x1c) {
            tmp.push(c);
        } else {
            let pr = p[i - 1];
            let lax = in_rng(pr, 1, 3)
                || in_rng(pr, 4, 6)
                || in_rng(pr, 10, 12)
                || in_rng(pr, 0x17, 0x19)
                || in_rng(pr, 0x23, 0x25)
                || in_rng(pr, 0x3b, 0x3d);
            if lax {
                tmp.push(b'6');
                rcol = true;
            } else {
                tmp.push(c);
            }
        }
    }
    if rcol {
        let n = tmp.iter().position(|&c| c == 0).unwrap_or(tmp.len());
        tmp.truncate(n);
        *p = tmp;
    }
    let s: Vec<u8> = p.clone();
    let len = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    let mut out: Vec<u8> = Vec::with_capacity(len + 4);
    let at = |k: isize| -> u8 {
        if k < 0 {
            0
        } else {
            *s.get(k as usize).unwrap_or(&0)
        }
    };
    let no_vowel = !s[..len].iter().any(|&c| phone_vowel(c) != 0);
    let mut i = 0usize;
    while i < len {
        if out.len() > 0x40 {
            break;
        }
        let c = s[i];
        let prev = i as isize - 1;
        let ih_uh = in_rng(c, 0x23, 0x25) || in_rng(c, 0x3b, 0x3d);
        let tense = ih_uh && (i == len - 1 || (i < len - 1 && phone_vowel(s[i + 1]) == 1));
        if tense {
            out.push(match c {
                b'#' => b'&',
                b'$' => b'\'',
                b'%' => b'(',
                b';' => b'>',
                b'<' => b'?',
                _ => b'@',
            });
            i += 1;
            continue;
        }
        if (c == b'B' || c == b'C') && i == len - 1 {
            out.push(if c == b'B' { b'>' } else { b'&' });
            i += 1;
            continue;
        }
        if i + 1 >= len || !in_rng(c, 7, 9) {
            let prev_vowel = prev >= 0 && prev < 0x46 && phone_vowel(at(prev)) != 0;
            if prev < 0 || !in_rng(c, b'+', b'-') || prev_vowel {
                out.push(c);
            } else {
                let keep = (prev > 0 && (at(prev) == b'+' || at(prev) == b'6') && phone_vowel(at(prev - 1)) != 0)
                    || (i < len - 1 && !no_vowel);
                if keep {
                    out.push(c);
                } else {
                    out.push(7);
                    out.push(c);
                }
            }
            i += 1;
            continue;
        }
        let c1 = s[i + 1];
        if c1 == b'6' {
            match c {
                7 => out.push(0x1a),
                8 => out.push(0x1b),
                _ => out.push(0x1c),
            }
            i += 2;
        } else if in_rng(c1, 0x1a, 0x1c) {
            out.push(c1);
            i += 2;
        } else {
            out.push(c);
            i += 1;
        }
    }
    *p = out;
    true
}

/// `FUN_1000ff60`: the phone string holds a vowel.
pub fn has_vowel(p: &[u8]) -> bool {
    p.iter().any(|&c| (c as i8) > 0 && c < b'F' && phone_vowel(c) != 0)
}

/// `FUN_10009cd0`: spell the word letter by letter, 'd' between letters.
/// Returns the phone string; the caller ORs 0x20 into the flags when non-empty.
///
/// Non-letters (`._-'`) are never themselves visited for output -- only a
/// *letter*'s own look-ahead (`n1`) decides whether a joiner follows it, by
/// checking whether the run resumes at a letter directly or at one of those
/// separators followed by another letter. An earlier version of this port
/// also pushed a joiner when the loop *visited* a separator character
/// (redundant with the look-ahead, since the previous letter's check had
/// already decided a joiner belonged there) -- double-joining embedded
/// punctuation, e.g. spelling `"J.R.R"` as `jay-ar-ar` with a stray extra
/// gap. Corpus-verified against the `out_feling` capture `suffix_004.json`'s
/// `morph_out` for `"J.R.R"` (whole-word pron, produced by the
/// `should_spell` driver branch calling this on the full spelling in one
/// shot -- the captured per-letter `spell` *events* for this word are a
/// separate, lower-level trace of the real `FUN_10009cd0`'s own letter
/// loop, not evidence the driver calls it once per letter).
pub fn spell(w: &[u8]) -> Vec<u8> {
    let mut d: Vec<u8> = Vec::new();
    let at = |i: usize| *w.get(i).unwrap_or(&0);
    let seps = b"._-'";
    let mut i = 0usize;
    while i < w.len() {
        let c = w[i];
        if is_letter(c) {
            let k = (TOUPPER_S[c as usize] - b'A' as i16) as usize;
            let ph = SPELL.get(k).copied().unwrap_or(b"");
            let mut done = false;
            for &p in ph {
                if d.len() > 0x3f {
                    done = true;
                    break;
                }
                d.push(p);
            }
            if done {
                break;
            }
            let n1 = at(i + 1);
            if d.len() > 0x3f
                || n1 == 0
                || (!is_letter(n1) && (!seps.contains(&n1) || !is_letter(at(i + 2))))
                || d.len() > 0x3e
            {
                break;
            }
            d.push(100);
        }
        i += 1;
    }
    d
}

/// `fe_LetterToSound`. `None` when the word is not LTS-able; otherwise the
/// phones and the flag bits to OR into the morph flags (0x10 LTS, 0x20 spelt).
pub fn letter_to_sound(lex: &Lexicon, w: &[u8]) -> Option<(Vec<u8>, u8)> {
    if !lts_valid(lex, w) {
        return None;
    }
    let n = w.len();
    let mut buf: Vec<u8>;
    if n == 1 {
        if letter_vowel(w[0]) {
            let s = spell(w);
            let f = if s.is_empty() { 0 } else { 0x20 };
            return Some((s, f));
        }
        buf = w.iter().map(|&c| upper(c)).collect();
    } else if n > 2
        && TOUPPER_S[w[0] as usize] == b'M' as i16
        && TOUPPER_S[w[1] as usize] == b'C' as i16
        && w[2..].iter().any(|&c| is_letter(c) && letter_vowel(c))
    {
        buf = b"MAC".to_vec();
        buf.extend(w[2..].iter().map(|&c| upper(c)));
    } else {
        buf = w.iter().map(|&c| upper(c)).collect();
    }
    let len = buf.len();
    let mut out: Vec<u8> = vec![0; len + 1];
    let mut k = 0usize;
    let bat = |j: isize| if j < 0 || j as usize >= len { 0 } else { buf[j as usize] };
    for i in (0..len).rev() {
        let c = buf[i];
        if !is_letter(c) && c != b'\'' {
            continue;
        }
        let ii = i as isize;
        let li = len as isize;
        let oat = |j: isize| out.get(j as usize).copied().unwrap_or(0) as i8 as i16;
        let fv: [i16; 10] = [
            if ii - 3 < 0 { 0 } else { letter_code(bat(ii - 3)) },
            if ii - 2 < 0 { 0 } else { letter_code(bat(ii - 2)) },
            if ii - 1 < 0 { 0 } else { letter_code(bat(ii - 1)) },
            letter_code(c),
            if ii + 1 < li { letter_code(bat(ii + 1)) } else { 0 },
            if ii + 2 < li { letter_code(bat(ii + 2)) } else { 0 },
            if ii + 3 < li { letter_code(bat(ii + 3)) } else { 0 },
            if ii + 1 < li { oat(li - ii - 2) } else { 0 },
            if ii + 2 < li { oat(li - ii - 3) } else { 0 },
            if ii + 3 < li { oat(li - ii - 4) } else { 0 },
        ];
        let t = if is_letter(c) { (c - b'A') as usize } else { 26 };
        let p = lex.atmt[t].predict_scalar(&fv) as u8;
        out[k] = p;
        k += 1;
        if c == b'E'
            && p == 1
            && len > 1
            && len < 4
            && i > 0
            && i + 1 < len
            && d5e(D9E as isize + bat(ii - 1) as i8 as isize) == 0
            && d5e(D9E as isize + bat(ii + 1) as i8 as isize) == 0
        {
            out[k - 1] = 0x1c;
        }
    }
    out.truncate(k);
    // FUN_10010050 only reverses when the code string has no embedded 0.
    let sl = out.iter().position(|&c| c == 0).unwrap_or(out.len());
    if k > 0 && (k as i32 - 1) <= (sl as i8) as i32 {
        out.reverse();
    }
    let codes: Vec<u8> = out.iter().take_while(|&&c| c != 0).copied().collect();
    let ph = expand(&codes);
    if !has_vowel(&ph) {
        let s = spell(w);
        let f = if s.is_empty() { 0 } else { 0x20 };
        return Some((s, f));
    }
    Some((ph, 0x10))
}

/// `FUN_10003110`: LTS each hyphen-separated part, joined by 'd'.
pub fn lts_hyphenated(lex: &Lexicon, w: &[u8]) -> (Vec<u8>, u8) {
    let mut d: Vec<u8> = Vec::new();
    let mut flags = 0u8;
    let mut part: Vec<u8> = Vec::new();
    let mut total = 0usize;
    for i in 0..=w.len() {
        let c = if i < w.len() { w[i] } else { 0 };
        if c == b'-' || c == 0 {
            let Some((ph, f)) = letter_to_sound(lex, &part) else { break };
            flags |= f;
            if ph.len() + 1 + total > 0x3f {
                break;
            }
            if total > 0 {
                d.push(100);
                total += 1;
            }
            total += ph.len();
            d.extend_from_slice(&ph);
            part.clear();
        } else {
            part.push(c);
        }
    }
    (d, flags)
}

pub fn is_word_char_letter(c: u8) -> bool {
    is_letter(c)
}

pub fn ci_eq(a: &[u8], b: &[u8]) -> bool {
    stricmp(a, b) == 0
}
