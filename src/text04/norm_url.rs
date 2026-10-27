//! `norm_url` (0x1004e100): web and mail addresses read piece by piece (`FUN_10041e60`: letter runs as
//! words, digit runs digit by digit, symbols by name, "dot" for `.`): a scheme word (`http`, `ftp`)
//! followed by the rest of the address, a host after `@` (taking back the user part already emitted)
//! or a host-looking word (`www.`, a known top-level domain, a dotted IP address).

use super::alias::tokenize_alias;
use super::mem::*;
use super::normalize::{Cx, State};
use super::rx::{is_p, restore, save, Buf as A, Save};
use super::tokenize::Tok;
use super::wa::Wa;

const SCHEMES: Ptab = ptab(0x10071c50, 0x10084838);
const GTLD: Ptab = ptab(0x10071c74, 0x1008483c);
const CCTLD: Ptab = ptab(0x10071cac, 0x10084840);
/// Symbol names (`FUN_10041a00`'s attach list): key, name at +0x10.
const SYMBOLS: Rec30 = rec30(0x10072088, 0x10084844);

/// `FUN_1004eb70`: four dot-separated numbers of up to three digits, each below 256.
fn ip(t: &[u8]) -> bool {
    let t = cs(t);
    let n = t.len();
    let mut dots = 0;
    let mut buf: Vec<u8> = Vec::new();
    for (i, &c) in t.iter().enumerate() {
        if is_digit(c) {
            buf.push(c);
            continue;
        }
        if c != b'.' && i != n - 1 {
            return false;
        }
        dots += 1;
        if dots > 4 || buf.is_empty() || buf.len() > 2 || atoi(&buf) < 0 || atoi(&buf) > 0xff {
            return false;
        }
        buf.clear();
    }
    n > 0 && dots == 4
}

/// `FUN_1004ea80`: `www.`, `www`, a known top-level domain after the last `.`, or an IP address.
fn urlish(t: &[u8], len: i32) -> bool {
    if len > 4 && strnicmp(s(0x10085804), t, 4) == 0 {
        return true;
    }
    if stricmp(s(0x1008580c), t) == 0 {
        return true;
    }
    let t = cs(t);
    let Some(p) = t.iter().rposition(|&c| c == b'.') else { return false };
    let tld = &t[p + 1..];
    if is_alpha(at(tld, 0)) && (GTLD.find(tld, true) >= 0 || CCTLD.find(tld, true) >= 0) {
        return true;
    }
    ip(t)
}

/// `FUN_1004ee60`: a user-dictionary entry at `text[*k]`; 5 when none.
fn ud_piece(cx: &Cx, wa: &mut Wa, text: &[u8], k: &mut i32, s0: i32, e: i32, mode: u8) -> i32 {
    let Some(ud) = cx.fe.ud.as_ref() else { return 5 };
    let w0 = wa.n();
    let mut buf = cs(text).to_vec();
    buf.extend_from_slice(&[0u8; 64]);
    let base = *k as usize;
    let mut c = base;
    let mut m = Tok::default();
    let idx = ud.match_at(&buf, &mut c, base, &mut m, 1);
    if idx < 0 {
        return 5;
    }
    *k += c as i32 - base as i32 - 1;
    let undo = |wa: &mut Wa| {
        wa.set_n(w0);
        if w0 > 0 {
            let e = wa.ri32(w0 - 1, 4);
            wa.set_consumed(e);
        }
    };
    let (kind, target) = ud.info[idx as usize].clone();
    if kind != b'A' {
        let r = wa.emit_phones(&m.text, &target, s0, e, b'U', b'Y');
        if r != 1 {
            undo(wa);
        }
        return r;
    }
    let mut tb = target.clone();
    tb.extend_from_slice(&[0u8; 8]);
    let mut q = 0usize;
    let mut r = mode as i32;
    loop {
        let mut t = Tok::default();
        tokenize_alias(&mut t, &tb, &mut q, 0, 1);
        if t.ty == 9 {
            return r;
        }
        let (c18, c19);
        if t.text[0] == b'<' && t.len >= 1 && t.text[t.len as usize - 1] == b'>' {
            let inner: Vec<u8> = t.text[1..].to_vec();
            t.set_text(&inner);
            t.text[(t.len - 2).max(0) as usize] = 0;
            t.len -= 2;
            c18 = b'U';
            c19 = b'S';
        } else {
            c18 = if mode == b'I' { b'I' } else { b'M' };
            c19 = b'C';
        }
        r = wa.emit(&t.text, s0, e, c18, c19);
        if r != 1 {
            break;
        }
    }
    undo(wa);
    r
}

/// `FUN_10041e60`: `len` bytes of `text` piece by piece. Mode `I` keeps `o'brien` together and marks the
/// words `I` (else `M`).
fn piece(cx: &Cx, wa: &mut Wa, text: &[u8], len: i32, s0: i32, e: i32, mode: u8) -> i32 {
    let mut pad = text.to_vec();
    pad.resize(text.len().max(len.max(0) as usize) + 8, 0);
    let text = &pad[..];
    let w0 = wa.n();
    let full = |wa: &mut Wa| {
        wa.set_n(w0);
        if w0 > 0 {
            let e = wa.ri32(w0 - 1, 4);
            wa.set_consumed(e);
        }
        3
    };
    let ch = |i: i32| at(text, i as isize);
    let mut k = 0i32;
    while k < len {
        let r = ud_piece(cx, wa, text, &mut k, s0, e, mode);
        if r != 1 {
            if r != 5 {
                return r;
            }
            let c = ch(k);
            if is_alpha(c) {
                let mut w = Vec::new();
                loop {
                    while is_alpha(ch(k)) {
                        w.push(ch(k));
                        k += 1;
                    }
                    if mode == b'I' && k > 0 && k + 1 < len && ch(k) == b'\'' && is_alpha(ch(k + 1)) {
                        w.push(b'\'');
                        k += 1;
                        continue;
                    }
                    break;
                }
                k -= 1;
                let c18 = if mode == b'I' { b'I' } else { b'M' };
                if wa.emit(&w, s0, e, c18, b'C') == 3 {
                    return full(wa);
                }
            } else if is_digit(c) {
                let mut d = Vec::new();
                while is_digit(ch(k)) {
                    d.push(ch(k));
                    k += 1;
                }
                k -= 1;
                let mut o = Vec::new();
                super::num::spell(&mut o, &d, 1, 0);
                if wa.emit_words(&o, s0, e, b'R', b'D') == 3 {
                    return full(wa);
                }
            } else if k > len - 3 || strncmp(&text[k as usize..], s(0x1006f7cc), 3) != 0 {
                let p = SYMBOLS.find(&[c], false);
                if p >= 0 && wa.emit_words(SYMBOLS.field(p, 0x10), s0, e, b'R', b'D') == 3 {
                    return full(wa);
                }
            } else {
                k += 2;
            }
        }
        k += 1;
    }
    1
}

/// The address goes on unless the next character ends it: neither alphanumeric nor a URL character,
/// or `.?!;,` before a blank, the end or another of `.?!;`.
fn ends(a: &A) -> bool {
    let c = a.nextc();
    let c2 = at(a.cx.t, *a.cur as isize + 1);
    let stop = s(0x1006f7c4);
    let not_url = cls(c) & 0xd0 == 0 && (c == 0 || !s(0x100857f0).contains(&c));
    let punct = ((c != 0 && stop.contains(&c)) || c == b',') && (matches!(c2, b' ' | b'\t' | b'\n' | b'\r' | 0) || stop.contains(&c2));
    not_url || punct
}

/// `FUN_10050620`: the first of the consumed tokens that run without blanks up to `start` (stopping at
/// brackets and quotes); -1 when there is none, 0 when the run reaches the sentence start.
fn run_back(a: &A, start: i32) -> i32 {
    let d = &a.st.done.toks;
    let n = d.len() as i32;
    let stopch = s(0x10085818);
    let after = |i: i32| if i >= n - 1 { -1 } else { i + 1 };
    let mut pos = start;
    let mut i = n - 1;
    loop {
        if i < 0 {
            return 0;
        }
        let t = &d[i as usize];
        if t.end != pos {
            return after(i);
        }
        let c = t.text[0];
        let stopped = c != 0 && stopch.contains(&c);
        if t.ws != 0 {
            return if stopped { after(i) } else { i };
        }
        if stopped {
            return after(i);
        }
        pos = t.start;
        i -= 1;
    }
}

/// `FUN_100506d0`: the first WA record starting at `start`.
fn wa_find(wa: &Wa, start: i32) -> i32 {
    if start < 0 {
        return -1;
    }
    let mut r = -1;
    for i in (0..wa.n()).rev() {
        let s = wa.ri32(i, 0);
        if s < start {
            return r;
        }
        if s == start {
            r = i;
        }
    }
    r
}

/// Records `from..` end where the address ends.
fn set_ends(wa: &mut Wa, from: i32, end: i32) {
    for i in from..wa.n() {
        wa.set_ri32(i, 4, end);
    }
}

pub fn url(cx: &Cx, st: &mut State, cur: &mut usize, _t: &Tok) -> i32 {
    let sv = save(st);
    let mut a = A { cx, st, cur };
    let t = a.last();
    if t.ty == 1 && SCHEMES.find(&t.text, true) >= 0 {
        return scheme(&mut a, sv, t);
    }
    if t.ws == 0 && is_p(&t, b'@') {
        return mail(&mut a, sv, t);
    }
    if !urlish(&t.text, t.len) {
        return not_mine(&mut a, sv.tn);
    }
    host(&mut a, sv)
}

fn not_mine(a: &mut A, n0: i32) -> i32 {
    if a.n() != n0 {
        a.back(n0);
    }
    5
}

/// Read on while the address continues, then consume the buffer (flag 12).
fn finish(a: &mut A, sv: Save, s0: i32, fill_from: i32) -> i32 {
    let mut end = -1;
    loop {
        if ends(a) {
            if end >= 0 {
                set_ends(&mut a.st.wa, fill_from, end);
            }
            let r = a.st.done.append_all(&a.st.ahead, 0xc);
            if r == 1 {
                return 1;
            }
            return restore(a.st, a.cur, sv, r);
        }
        let Some(t) = a.push_r() else { return restore(a.st, a.cur, sv, 4) };
        let r = piece(a.cx, &mut a.st.wa, &t.text, t.len, s0, t.end, b'R');
        if r != 1 {
            return restore(a.st, a.cur, sv, r);
        }
        end = t.end;
    }
}

/// `http://...`: the scheme in capitals, then the address.
fn scheme(a: &mut A, sv: Save, t: Tok) -> i32 {
    let s0 = t.start;
    let Some(t2) = a.push_r() else { return restore(a.st, a.cur, sv, 4) };
    if t2.ws != 0 || t2.len < 5 {
        a.pop();
        return not_mine(a, sv.tn);
    }
    let w12 = a.st.wa.n();
    let up: Vec<u8> = t.text().iter().map(|&c| if cls(c) & 0x40 != 0 { toupper(c) } else { c }).collect();
    let mut r = a.st.wa.emit(&up, s0, t2.end, b'R', b'D');
    if r == 1 {
        r = piece(a.cx, &mut a.st.wa, &t2.text, t2.len, s0, t2.end, b'R');
    }
    if r != 1 {
        return restore(a.st, a.cur, sv, r);
    }
    finish(a, sv, s0, w12)
}

/// `user@host`: the user part (already emitted) is taken back and read again in mode `I`.
fn mail(a: &mut A, sv: Save, t: Tok) -> i32 {
    let mut s1 = t.start;
    let Some(t2) = a.push_r() else { return restore(a.st, a.cur, sv, 4) };
    if t2.ws != 0 || !urlish(&t2.text, t2.len) || strnicmp(&t2.text, s(0x10085804), 4) == 0 {
        return not_mine(a, sv.tn);
    }
    if a.st.ahead.toks[0].ws == 0 {
        let k = run_back(a, t.start);
        if k >= 0 {
            let wi = wa_find(&a.st.wa, a.st.done.toks.get(k as usize).map_or(0, |d| d.start));
            if wi >= 0 {
                s1 = a.st.done.toks.get(k as usize).map_or(0, |d| d.start);
                a.st.wa.set_n(wi);
                let mut j = k as usize;
                while j < a.st.done.toks.len() {
                    a.st.done.toks[j].f1 = 0xd;
                    let d = &a.st.done.toks;
                    let mut text = d[j].text().to_vec();
                    let mut len = d[j].len;
                    while j + 2 < d.len() && d[j].ty == 1 && is_p(&d[j + 1], b'\'') && d[j + 2].ty == 1 {
                        text.push(b'\'');
                        text.extend_from_slice(d[j + 2].text());
                        len += 1 + d[j + 2].len;
                        j += 2;
                    }
                    let r = piece(a.cx, &mut a.st.wa, &text, len, s1, t2.end, b'I');
                    if r != 1 {
                        return restore(a.st, a.cur, sv, r);
                    }
                    j += 1;
                }
            }
        }
    }
    for i in 0..a.n() {
        let b = a.tok(i);
        let r = piece(a.cx, &mut a.st.wa, &b.text, b.len, s1, t2.end, b'R');
        if r != 1 {
            return restore(a.st, a.cur, sv, r);
        }
    }
    let r = a.st.done.append_all(&a.st.ahead, 0xd);
    if r == 1 {
        return 1;
    }
    restore(a.st, a.cur, sv, r)
}

/// A host (`www.example.com`, `noaa.gov`, `10.0.0.1`), read again with the alias tokenizer; consumed
/// pieces glued in front of it are taken back.
fn host(a: &mut A, sv: Save) -> i32 {
    a.pop();
    let Some(t2) = a.push_r() else { return restore(a.st, a.cur, sv, 4) };
    let mut s1 = t2.start;
    let mut l28 = -1;
    if t2.ws == 0 {
        let k = run_back(a, t2.start);
        if k >= 0 {
            l28 = wa_find(&a.st.wa, a.st.done.toks.get(k as usize).map_or(0, |d| d.start));
            if l28 >= 0 {
                s1 = a.st.done.toks.get(k as usize).map_or(0, |d| d.start);
                a.st.wa.set_n(l28);
                for j in k as usize..a.st.done.toks.len() {
                    a.st.done.toks[j].f1 = 0xc;
                    // The engine takes the length from the look-ahead slot with the same index.
                    let len = a.st.ahead.toks.get(j).map_or(0, |t| t.len);
                    let text = a.st.done.toks[j].text;
                    let r = piece(a.cx, &mut a.st.wa, &text, len, s1, t2.end, b'R');
                    if r != 1 {
                        return restore(a.st, a.cur, sv, r);
                    }
                }
            }
        }
    }
    for i in 0..a.n() {
        let b = a.tok(i);
        let r = piece(a.cx, &mut a.st.wa, &b.text, b.len, s1, t2.end, b'R');
        if r != 1 {
            return restore(a.st, a.cur, sv, r);
        }
    }
    finish(a, sv, s1, l28.max(0))
}
