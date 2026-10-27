//! `fe_TokenizeAlias` (0x1004cb80): the tokenizer for user-dictionary targets and the `'R'` look-ahead
//! mode. A token is a run of letters, digits and the URL characters of `DAT_100857f0`; `mode` 1 also
//! keeps a `<TAG>` together and takes an apostrophe suffix. Tokens are type 3, a tag marker 4, its close
//! 5, the end 9.

use super::mem::*;
use super::tokenize::Tok;

fn tokchar(c: u8) -> bool {
    cls(c) & 0xd0 != 0 || (c != 0 && s(0x100857f0).contains(&c))
}

fn ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r')
}

/// `FUN_1004cfa0` (shared with the main tokenizer): the apostrophe suffix at `t[p]`.
pub fn contraction(t: &[u8], p: usize, prev: u8) -> Option<Vec<u8>> {
    if at(t, p as isize) != b'\'' {
        return None;
    }
    let c1 = at(t, p as isize + 1);
    let c2 = at(t, p as isize + 2);
    let c3 = at(t, p as isize + 3);
    if fold_s(prev) == 0x73 && matches!(c1, b' ' | b'\t' | b'\n' | b'\r') {
        return Some(b"'".to_vec());
    }
    let s1 = fold_s(c1);
    if ((fold_s(prev) == 0x6e && s1 == 0x74) || matches!(s1, 0x73 | 100 | 0x6d)) && !is_alpha(c2) {
        return Some(vec![b'\'', c1]);
    }
    let s2 = fold_s(c2);
    if matches!((s1, s2), (0x76, 0x65) | (0x65, 0x6d) | (0x72, 0x65) | (0x6c, 0x6c)) && !is_alpha(c3) {
        return Some(vec![b'\'', c1, c2]);
    }
    None
}

pub fn tokenize_alias(tok: &mut Tok, t: &[u8], cur: &mut usize, base: usize, mode: i16) {
    *tok = Tok::default();
    tok.start = (*cur - base) as i32;
    while ws(at(t, *cur as isize)) {
        *cur += 1;
        tok.start += 1;
        tok.ws += 1;
    }
    let mut buf: Vec<u8> = Vec::new();
    loop {
        tok.start = (*cur - base) as i32;
        let c = at(t, *cur as isize);
        if c == 0 {
            tok.finish(9, b"");
            return;
        }
        let rest = cs(&t[*cur..]);
        let mk = s(0x1006fa20);
        if rest.len() > 4 && c == b'<' && strncmp(&rest[1..], mk, 2) == 0 && (rest[3] as i8) > 0 && rest[3] < b'p' && rest[4] == b'>' {
            let x = rest[3];
            *cur += 5;
            tok.finish(4, &[x]);
            return;
        }
        if rest.len() > 4 && c == b'<' && rest[1] == b'/' && strncmp(&rest[2..], mk, 2) == 0 && rest[4] == b'>' {
            *cur += 5;
            tok.finish(5, b"");
            return;
        }
        if tokchar(c) || (mode != 0 && c == b'<' && is_alpha(at(t, *cur as isize + 1))) {
            break;
        }
        if (c as i8) < 0 && at(t, *cur as isize + 1) != 0 {
            *cur += 2;
            tok.ws += 2;
        } else {
            *cur += 1;
            tok.ws += 1;
        }
    }
    // LAB_1004cdb8
    loop {
        let c = at(t, *cur as isize);
        let n1 = at(t, *cur as isize + 1);
        if (c != 0 && b".?!;".contains(&c) || c == b',') && (ws(n1) || n1 == 0 || b".?!;".contains(&n1)) {
            break;
        }
        buf.push(c);
        *cur += 1;
        if buf.len() > 0x1c {
            tok.finish(3, &buf);
            return;
        }
        let nc = at(t, *cur as isize);
        if !tokchar(nc) {
            if mode == 0 {
                tok.finish(if buf.is_empty() { 0 } else { 3 }, &buf);
                return;
            }
            if !(*cur > base && nc == b'>' && is_alpha(at(t, *cur as isize - 1))) {
                break;
            }
        }
    }
    // LAB_1004ce90
    if mode != 0 && !buf.is_empty() {
        if let Some(suf) = contraction(t, *cur, at(t, *cur as isize - 1)) {
            if suf.len() + buf.len() < 0x1e {
                buf.extend_from_slice(&suf);
                *cur += suf.len();
            }
        }
    }
    tok.finish(if buf.is_empty() { 0 } else { 3 }, &buf);
}
