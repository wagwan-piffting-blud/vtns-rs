//! Sentence-boundary decision for a `.?!;` token (`FUN_10048210`): hand rules (`FUN_10048270`) giving
//! `P` end / `N` not / `A` ask, and for `A` the `sbd.tree2` CART over the 16 features of `FUN_10048010`
//! (the token before, the token after, the punctuation).

use super::mem::*;
use super::stream::{token_at, Ahead, Done};
use super::tokenize::Tok;
use super::Fe;

/// `FUN_100509d0`.
fn case_class(t: &Tok) -> i16 {
    if t.len == 0 {
        return -1;
    }
    if t.ty == 2 {
        return 4;
    }
    if t.ty == 1 {
        let (mut up, mut low, mut first_up) = (false, false, false);
        for i in 0..t.len as usize {
            let k = cls(at(&t.text, i as isize));
            if k & 0x80 == 0 {
                if k & 0x40 != 0 {
                    low = true;
                }
            } else {
                up = true;
                if i == 0 {
                    first_up = true;
                }
            }
        }
        if up && !low {
            return 1;
        }
        if low && !up {
            return 2;
        }
        if first_up && low {
            return 3;
        }
    }
    5
}

/// `FUN_10050ac0`.
fn punct_class(t: &Tok) -> i16 {
    if t.ty != 3 && t.ty != 7 {
        return -1;
    }
    match t.text[0] {
        0x21 => 3,
        0x22 | 0x27 | 0x60 => 6,
        0x2c | 0x3a => 5,
        0x2d => 7,
        0x2e => 1,
        0x3b => 4,
        0x3f => 2,
        _ => 8,
    }
}

/// `FUN_10050b80`: like `find`, but among case-insensitively equal keys only one whose value starts with
/// `'2'`, or `'1'` with an exact case match, counts.
pub fn find_flagged(l: &crate::lexicon::Txt2, w: &[u8]) -> i32 {
    let w = cs(w);
    let i3 = l.find(w);
    if i3 < 0 {
        return -1;
    }
    let ok = |i: usize| match l.vals[i].first() {
        Some(b'2') => true,
        Some(b'1') => strcmp(&l.keys[i], w) == 0,
        _ => false,
    };
    if ok(i3 as usize) {
        return i3;
    }
    let mut u = i3 - 1;
    while u >= 0 {
        if stricmp(&l.keys[u as usize], w) != 0 {
            break;
        }
        if ok(u as usize) {
            return i3;
        }
        u -= 1;
    }
    let n = l.keys.len() as i32;
    let mut v = i3 + 1;
    while v < n {
        if stricmp(&l.keys[v as usize], w) != 0 {
            break;
        }
        if ok(v as usize) {
            return i3;
        }
        v += 1;
    }
    -1
}

/// `FUN_10050d00`: bit 1 abbrh, 2 abbrt, 4 abbrc.
fn abbr_bits(fe: &Fe, w: &[u8]) -> u8 {
    let mut b = 0;
    if find_flagged(&fe.abbrh, w) >= 0 {
        b |= 1;
    }
    if find_flagged(&fe.abbrt, w) >= 0 {
        b |= 2;
    }
    if find_flagged(&fe.abbrc, w) >= 0 {
        b |= 4;
    }
    b
}

/// `FUN_10050a80`: a '.' after the first character, with something after it.
fn inner_dot(w: &[u8]) -> i16 {
    let w = cs(w);
    match w.iter().position(|&c| c == b'.') {
        Some(i) if i > 0 && at(w, i as isize + 1) != 0 => 1,
        _ => 0,
    }
}

/// `FUN_1000cb40`.
pub fn all_alpha(w: &[u8]) -> bool {
    let w = cs(w);
    !w.is_empty() && w.iter().all(|&c| is_alpha(c))
}

/// `FUN_10050d60`: coarse category of the token after the punctuation.
fn word_cat(fe: &Fe, w: &[u8]) -> i16 {
    let w = cs(w);
    let i = fe.sbdw.find(w);
    if i >= 0 {
        let v = atoi(&fe.sbdw.vals[i as usize]);
        let m = b(0x1006fb04, 0x80);
        let o = (2 * v) as usize;
        return i16::from_le_bytes([m[o], m[o + 1]]);
    }
    if w.iter().any(|&c| is_digit(c)) {
        return 9;
    }
    if w.iter().filter(|&&c| cls(c) & 0x80 != 0).count() > 1 {
        return 3;
    }
    let c0 = at(w, 0);
    if c0 != 0 && b".?!;".contains(&c0) {
        return 0x11;
    }
    if c0 == b',' {
        return 0xb;
    }
    if c0 != 0 && s(0x100858b8).contains(&c0) {
        return 0xf;
    }
    if c0 != 0 && s(0x100858b4).contains(&c0) {
        return 0xc;
    }
    if c0 != 0 && b"([{<".contains(&c0) {
        return 0xd;
    }
    if c0 != 0 && b")]}>".contains(&c0) {
        return 0xe;
    }
    if !all_alpha(w) {
        let Some(ap) = w.iter().position(|&c| c == b'\'') else {
            return if is_alpha(c0) { 0 } else { 0x10 };
        };
        let mut sub: Vec<u8> = Vec::new();
        if ap > 0 && stricmp(&w[ap - 1..], b"n't") == 0 {
            sub = w[..ap - 1].to_vec();
        } else {
            let tail = &w[ap + 1..];
            let hit = [&b"s"[..], b"d", b"m", s(0x100858a8), s(0x100853c8), s(0x10085474), s(0x100858a4)]
                .iter()
                .any(|x| stricmp(tail, x) == 0);
            if hit {
                sub = w[..ap].to_vec();
            }
        }
        if sub.is_empty() {
            return 0;
        }
        return word_cat(fe, &sub);
    }
    let l = w.len();
    let tail = |k: usize| &w[l - k..];
    if l > 6 && stricmp(tail(6), b"liness") == 0 {
        return 1;
    }
    if l > 4 {
        for x in [&b"ance"[..], b"tion", b"ness", b"ment", b"sion", b"ship", s(0x100858ac)] {
            if stricmp(tail(4), x) == 0 {
                return 1;
            }
        }
    }
    if l > 3 && stricmp(tail(3), b"ist") == 0 {
        return 1;
    }
    if l > 4 && stricmp(tail(4), b"less") == 0 {
        return 5;
    }
    if l > 3 && (stricmp(tail(3), b"ing") == 0 || stricmp(tail(3), b"ful") == 0) {
        return 5;
    }
    if l < 3 {
        return 0;
    }
    if stricmp(tail(2), b"ly") != 0 {
        return 0;
    }
    5
}

fn side(fe: &Fe, t: &Tok, f: &mut [i16], at0: usize) {
    f[at0] = t.len as i16;
    f[at0 + 1] = case_class(t);
    let bits = abbr_bits(fe, &t.text);
    f[at0 + 2] = (bits & 1 == 1) as i16;
    f[at0 + 3] = (bits & 2 == 2) as i16;
    f[at0 + 4] = (bits & 4 == 4 || f[at0 + 3] != 0 || f[at0 + 2] != 0) as i16;
    f[at0 + 5] = inner_dot(&t.text);
    f[at0 + 6] = punct_class(t);
}

/// `FUN_10048010`.
fn features(fe: &Fe, done: &Done, a: &mut Ahead, t: &[u8], cur: &mut usize, base: usize) -> [i16; 16] {
    let mut f = [-1i16; 16];
    let prev = token_at(done, a, a.n - 1, -1);
    let k0 = a.n;
    let next = a.push(t, cur, base, b'D', fe.ud.as_ref());
    match prev {
        None => f[0] = 0,
        Some(p) => side(fe, &p, &mut f, 0),
    }
    let mut cat = -1;
    match next {
        None => f[7] = 0,
        Some(k) => {
            let nt = a.toks[k];
            side(fe, &nt, &mut f, 7);
            cat = word_cat(fe, &nt.text);
        }
    }
    f[14] = cat;
    f[15] = punct_class(&a.tok(k0 as isize - 1));
    a.pop(cur);
    f
}

fn upper(c: u8) -> bool {
    cls(c) & 0x80 != 0
}

/// `FUN_10048270`. `p` is the punctuation token (last in the buffer).
fn rules(fe: &Fe, done: &Done, a: &mut Ahead, p: &Tok, t: &[u8], cur: &mut usize, base: usize) -> u8 {
    if (p.ty == 3 || p.ty == 7) && p.len == 1 && p.text[0] == b'?' && p.ws == 0 {
        return b'P';
    }
    let Some(k) = a.push(t, cur, base, b'D', fe.ud.as_ref()) else { return b'P' };
    let nx = a.toks[k];
    if nx.ty == 9 {
        a.pop(cur);
        return b'P';
    }
    if nx.ws < 3 && nx.ty == 8 {
        a.pop(cur);
        return b'N';
    }
    let pi = a.pop(cur).unwrap_or(-1);
    let pt = a.tok(pi);
    let prev = token_at(done, a, a.n - 1, -1);
    let after = at(t, *cur as isize);
    let mut skip_mid = false;
    if let Some(pv) = prev {
        if pt.text[0] == b'.' && pt.ws == 0 && upper(at(&pv.text, pv.len as isize - 1)) && upper(after) {
            return b'N';
        }
        if pt.text[0] == b'.' {
            if pt.ws != 0 {
                skip_mid = true;
            } else if pv.ty == 1 && is_digit(after) {
                return b'N';
            }
        }
    }
    if !skip_mid && pt.ws == 0 && pt.len < 3 && pt.text[0] == b'.' && prev.is_some_and(|pv| pv.len == 1 && upper(pv.text[0])) {
        let Some(k) = a.push(t, cur, base, b'D', fe.ud.as_ref()) else { return b'P' };
        let nt = a.toks[k];
        if nt.ws < 2 && nt.len == 1 && upper(nt.text[0]) {
            a.pop(cur);
            return b'N';
        }
        a.pop(cur);
    }
    // LAB_10048416
    if pt.ws == 0 && pt.len < 3 && pt.text[0] == b'.' {
        let Some(k) = a.push(t, cur, base, b'D', fe.ud.as_ref()) else { return b'P' };
        let nt = a.toks[k];
        if nt.ws < 2 && nt.text() == b"and" {
            a.pop(cur);
            return b'N';
        }
        a.pop(cur);
    }
    b'A'
}

/// `FUN_10048210`: `P` the sentence ends at the punctuation token `p`, `N` it does not.
pub fn decide(fe: &Fe, done: &Done, a: &mut Ahead, p: &Tok, t: &[u8], cur: &mut usize, base: usize) -> u8 {
    let r = rules(fe, done, a, p, t, cur, base);
    if r != b'A' {
        return r;
    }
    let f = features(fe, done, a, t, cur, base);
    if (fe.sbd.predict_scalar(&f) as u8) != 0 {
        b'P'
    } else {
        b'N'
    }
}
