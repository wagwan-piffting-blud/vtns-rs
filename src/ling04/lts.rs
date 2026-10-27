//! Letter-to-sound, spelling and the dictionary-or-LTS lookup of the 2004 engine. The word check, the
//! letter features and the per-letter `atmt` trees are the 2014 ones; the 2004 LTS has no short-word
//! "E" rule, and its post-processing (`FUN_100023f0`) inserts no schwa before syllabic L/M/N.

use super::tables::CODEBOOK;
use crate::lexicon::{decode_payload_with, Lexicon};
use crate::ling::lts::{d5e, has_vowel, is_letter, letter_code, letter_vowel, lts_valid, phone_vowel, upper};
use crate::ling::tables::{F5A, TOUPPER_S};

/// `FUN_100031e0`: phones and the flag bits (0x10 LTS, 0x20 spelt), `None` when the word is not LTS-able.
pub fn letter_to_sound(lex: &Lexicon, w: &[u8]) -> Option<(Vec<u8>, u8)> {
    if !lts_valid(lex, w) {
        return None;
    }
    let n = w.len();
    if n == 1 && letter_vowel(w[0]) {
        return Some(spell_f(w));
    }
    let buf: Vec<u8> = if n > 2
        && TOUPPER_S[w[0] as usize] == b'M' as i16
        && TOUPPER_S[w[1] as usize] == b'C' as i16
        && w[2..].iter().any(|&c| is_letter(c) && letter_vowel(c))
    {
        b"MAC".iter().copied().chain(w[2..].iter().map(|&c| upper(c))).collect()
    } else {
        w.iter().map(|&c| upper(c)).collect()
    };
    let len = buf.len();
    let mut out: Vec<u8> = vec![0; len + 1];
    let mut k = 0usize;
    let bat = |j: isize| if j < 0 || j as usize >= len { 0 } else { buf[j as usize] };
    for i in (0..len).rev() {
        let c = buf[i];
        if !is_letter(c) && c != b'\'' {
            continue;
        }
        let (ii, li) = (i as isize, len as isize);
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
        out[k] = lex.atmt[t].predict_scalar(&fv) as u8;
        k += 1;
    }
    out.truncate(k);
    // FUN_1000cbd0 only reverses when the code string has no embedded 0.
    let sl = out.iter().position(|&c| c == 0).unwrap_or(out.len());
    if k > 0 && (k as i32 - 1) <= (sl as i8) as i32 {
        out.reverse();
    }
    let codes: Vec<u8> = out.iter().take_while(|&&c| c != 0).copied().collect();
    let ph = expand(&codes);
    if !has_vowel(&ph) {
        return Some(spell_f(w));
    }
    Some((ph, 0x10))
}

fn spell_f(w: &[u8]) -> (Vec<u8>, u8) {
    let s = spell(w);
    let f = if s.is_empty() { 0 } else { 0x20 };
    (s, f)
}

/// `FUN_10002610`: LTS output codes to phone codes. An unknown code ends the string unprocessed.
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
                    return d;
                }
                d.push(d5e(F5A as isize + c as isize) as u8);
            }
        }
    }
    post_process(&d)
}

fn in_rng(c: u8, lo: u8, hi: u8) -> bool {
    (c as i8) >= lo as i8 && (c as i8) <= hi as i8
}

/// `FUN_100023f0`: r-coloured vowels after a lax vowel, final / prevocalic IH and UH tensed, final B/C
/// (OW, AY) to their offglide forms, and the vowel + R pairs merged.
fn post_process(p: &[u8]) -> Vec<u8> {
    if p.is_empty() {
        return Vec::new();
    }
    let mut s: Vec<u8> = Vec::with_capacity(p.len());
    for i in 0..p.len() {
        let c = p[i];
        let lax = i > 0 && {
            let pr = p[i - 1];
            in_rng(pr, 1, 3)
                || in_rng(pr, 4, 6)
                || in_rng(pr, 10, 12)
                || in_rng(pr, 0x17, 0x19)
                || in_rng(pr, 0x23, 0x25)
                || in_rng(pr, 0x3b, 0x3d)
        };
        s.push(if in_rng(c, 0x1a, 0x1c) && lax { b'6' } else { c });
    }
    let len = s.len();
    let mut out: Vec<u8> = Vec::with_capacity(len);
    let mut i = 0usize;
    while i < len {
        let c = s[i];
        let ih_uh = in_rng(c, 0x23, 0x25) || in_rng(c, 0x3b, 0x3d);
        if ih_uh && (i == len - 1 || phone_vowel(s[i + 1]) != 0) {
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
            out.push(c);
            i += 1;
            continue;
        }
        let c1 = s[i + 1];
        if c1 == b'6' {
            out.push(match c {
                7 => 0x1a,
                8 => 0x1b,
                _ => 0x1c,
            });
            i += 2;
        } else if in_rng(c1, 0x1a, 0x1c) {
            out.push(c1);
            i += 2;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// `FUN_10008230`.
pub fn spell(w: &[u8]) -> Vec<u8> {
    crate::ling::lts::spell(w)
}

/// `FUN_100033a0`: LTS of every hyphen-separated part, joined by 100; `None` when nothing came out.
pub fn lts_hyphenated(lex: &Lexicon, w: &[u8]) -> Option<(Vec<u8>, u8)> {
    let mut d: Vec<u8> = Vec::new();
    let mut flags = 0u8;
    let mut part: Vec<u8> = Vec::new();
    for i in 0..=w.len() {
        let c = if i < w.len() { w[i] } else { 0 };
        if c != b'-' && c != 0 {
            part.push(c);
            continue;
        }
        let Some((ph, f)) = letter_to_sound(lex, &part) else { break };
        flags |= f;
        if ph.len() + 1 + d.len() > 0x3f {
            break;
        }
        if !d.is_empty() {
            d.push(100);
        }
        d.extend_from_slice(&ph);
        part.clear();
    }
    if d.is_empty() {
        None
    } else {
        Some((d, flags))
    }
}

/// `FUN_1000a050`: first lexicon pronunciation, else LTS when the entry carries B80.
pub fn lookup_or_lts(lex: &Lexicon, w: &[u8]) -> Option<Vec<u8>> {
    let raw = lex.lookup_payload(w).unwrap_or(&[]);
    let payload = &raw[..raw.iter().position(|&c| c == 0).unwrap_or(raw.len())];
    let e = decode_payload_with(payload, &CODEBOOK);
    if let Some((_, p)) = e.prons.first() {
        return Some(p.clone());
    }
    if e.b80 != 0 {
        return letter_to_sound(lex, w).map(|(p, _)| p);
    }
    None
}
