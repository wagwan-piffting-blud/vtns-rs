//! Token lookup: `FUN_1000d190` splits every normalized token into lexicon
//! pieces (`FUN_1000d640`: contractions, trailing punctuation, digit groups)
//! and fills one 0x554-byte candidate per piece (`dict_FillMorphFromPayload`).

use super::tables::CHAR_CLASS;
use super::{sc, war, Wa, Ws};
use crate::lexicon::Lexicon;

pub mod cand {
    pub const COUNT: usize = 0x00;
    pub const TOKEN: usize = 0x02;
    pub const CLASS_CHAR: usize = 0x04;
    pub const SPELLING: usize = 0x05;
    pub const POS: usize = 0x23;
    pub const PRON: usize = 0x37;
    pub const PRON_STRIDE: usize = 0x41;
    pub const B40: usize = 0x54c;
    pub const B10: usize = 0x54e;
    pub const B20: usize = 0x550;
    pub const B80: usize = 0x552;
}

fn endp(c: u8) -> bool {
    matches!(c, 0 | b';' | b'.' | b',' | b'?' | b'!')
}

/// Remaining token text, with the engine's leading-space quirk: when a piece
/// is split off, the rest is written after the skipped leading spaces.
pub struct TokenBuf {
    pub b: Vec<u8>,
}

impl TokenBuf {
    fn at(&self, i: usize) -> u8 {
        *self.b.get(i).unwrap_or(&0)
    }
}

/// `FUN_1000d640`: returns the next piece and leaves the rest in `buf`.
pub fn split_token(
    buf: &mut TokenBuf,
    is_last: bool,
    forced_pos: u8,
    preset_nonempty: bool,
    keep_case: bool,
    lex: &Lexicon,
) -> Vec<u8> {
    split_token_era(buf, is_last, forced_pos, preset_nonempty, keep_case, lex, false)
}

/// `split_token`; the 2004 splitter (`FUN_1000a560`) has no final-`-` split and no VTML-marker split.
pub fn split_token_era(
    buf: &mut TokenBuf,
    is_last: bool,
    forced_pos: u8,
    preset_nonempty: bool,
    keep_case: bool,
    lex: &Lexicon,
    v2004: bool,
) -> Vec<u8> {
    let n = buf.b.len();
    let take_all = |buf: &mut TokenBuf| -> Vec<u8> {
        let v = std::mem::take(&mut buf.b);
        v
    };
    if forced_pos != 0xff {
        if n > 1 && sc(&CHAR_CLASS, buf.b[n - 1]) & 0xc0 == 0 {
            let piece = buf.b[..n - 1].to_vec();
            let last = buf.b[n - 1];
            buf.b = vec![last];
            return piece;
        }
        return take_all(buf);
    }
    if preset_nonempty || keep_case {
        return take_all(buf);
    }
    let lead = buf.b.iter().take_while(|&&c| c == b' ').count();
    let s: Vec<u8> = buf.b[lead..].to_vec();
    let n = s.len();
    let at = |i: usize| *s.get(i).unwrap_or(&0);
    let mut out: Vec<u8> = Vec::new();
    let mut dot = false;
    let mut i = 0usize;
    // Rest of the token goes back after the leading spaces.
    let split_rest = |buf: &mut TokenBuf, from: usize| {
        let mut v = buf.b[..lead].to_vec();
        v.extend_from_slice(&s[from.min(n)..]);
        buf.b = v;
    };
    if n == 0 {
        buf.b.clear();
        return out;
    }
    loop {
        if i >= n {
            break;
        }
        if i > 0 {
            let c = at(i);
            let (c1, c2, c3) = (at(i + 1), at(i + 2), at(i + 3));
            let contraction = (c == b'\'' && c1 == b's' && endp(c2))
                || (c == b'n' && c1 == b'\'' && c2 == b't' && endp(c3))
                || (c == b'\''
                    && ((c1 == b'v' && c2 == b'e' && endp(c3))
                        || (c1 == b'd' && endp(c2))
                        || (c1 == b'm' && endp(c2))
                        || (c1 == b'e' && c2 == b'm' && endp(c3))
                        || (c1 == b'r' && c2 == b'e' && endp(c3))
                        || (c1 == b'l' && c2 == b'l' && endp(c3))
                        || (c1 == b'S' && endp(c2))))
                || (c == b'N' && c1 == b'\'' && c2 == b'T' && endp(c3))
                || (c == b'\''
                    && ((c1 == b'V' && c2 == b'E' && endp(c3))
                        || (c1 == b'D' && endp(c2))
                        || (c1 == b'M' && endp(c2))
                        || (c1 == b'E' && c2 == b'M' && endp(c3))
                        || (c1 == b'R' && c2 == b'E' && endp(c3))
                        || (c1 == b'L' && c2 == b'L' && endp(c3))));
            if contraction {
                split_rest(buf, i);
                return out;
            }
        }
        if n != 1 && i == n - 1 {
            let c = at(i);
            if matches!(c, b';' | b'.' | b',' | b'?' | b'!') {
                if is_last {
                    split_rest(buf, i);
                    return out;
                }
                let mut with = out.clone();
                with.push(c);
                let found = lex.lookup_payload(&with).is_some_and(|p| !p.is_empty());
                if dot || found {
                    split_rest(buf, i + 1);
                    return with;
                }
                split_rest(buf, i);
                return out;
            }
            if c == b'-' && !v2004 {
                split_rest(buf, i);
                return out;
            }
        }
        let c = at(i);
        let (c1, c2, c3) = (at(i + 1), at(i + 2), at(i + 3));
        if i + 2 < n && endp(c3) {
            let three = (c == b'n' && c1 == b'\'' && c2 == b't')
                || (c == b'\'' && matches!((c1, c2), (b'v', b'e') | (b'e', b'm') | (b'r', b'e') | (b'l', b'l')))
                || (c == b'N' && c1 == b'\'' && c2 == b'T')
                || (c == b'\'' && matches!((c1, c2), (b'V', b'E') | (b'E', b'M') | (b'R', b'E') | (b'L', b'L')));
            if three {
                out.extend_from_slice(&[c, c1, c2]);
                i += 3;
                if i == n {
                    buf.b.clear();
                } else {
                    split_rest(buf, i);
                }
                return out;
            }
        }
        if i + 1 < n && endp(c2) && c == b'\'' && matches!(c1, b's' | b'd' | b'm' | b'S' | b'D' | b'M') {
            out.extend_from_slice(&[c, c1]);
            i += 2;
            if i == n {
                buf.b.clear();
            } else {
                split_rest(buf, i);
            }
            return out;
        }
        if c == b'\'' && c1 == b'\'' {
            out.push(b'\'');
            out.push(c1);
            i += 2;
        } else if sc(&CHAR_CLASS, c) & 0x10 != 0 && (c1 == b'.' || c1 == b',') && sc(&CHAR_CLASS, c2) & 0x10 != 0 {
            out.extend_from_slice(&[c, c1, c2]);
            i += 3;
        } else {
            if !v2004 && n != 2 && !out.is_empty() && c == 0xa2 && c1 == 0xfe {
                split_rest(buf, i);
                return out;
            }
            dot = c == b'.';
            out.push(c);
            i += 1;
        }
        if at(i) == 0 {
            break;
        }
    }
    if i == n {
        buf.b.clear();
    }
    out
}

/// Decoded payload in the engine's 0x554-byte candidate layout.
pub fn fill_candidate(
    ws: &mut Ws,
    idx: usize,
    spelling: &[u8],
    payload: &[u8],
    token: i16,
    prev_token: i32,
    forced_pos: u8,
    preset: &[u8],
    flag: bool,
) {
    fill_candidate_with(ws, idx, spelling, payload, token, prev_token, forced_pos, preset, flag, &super::tables::CODEBOOK);
}

/// `fill_candidate` with an explicit phone codebook (the 2004 engine has its own).
#[allow(clippy::too_many_arguments)]
pub fn fill_candidate_with(
    ws: &mut Ws,
    idx: usize,
    spelling: &[u8],
    payload: &[u8],
    token: i16,
    prev_token: i32,
    forced_pos: u8,
    preset: &[u8],
    flag: bool,
    book: &[&[u8]; 256],
) {
    let c = Ws::cand(idx);
    ws.set_i16(c + cand::TOKEN, token);
    ws.set_i16(c + cand::COUNT, 0);
    ws.strcpy(c + cand::SPELLING, spelling);
    let e = crate::lexicon::decode_payload_with(payload, book);
    ws.set_u8(c + cand::CLASS_CHAR, e.class_char);
    ws.set_i16(c + cand::B40, e.b40);
    ws.set_i16(c + cand::B20, e.b20);
    ws.set_i16(c + cand::B10, e.b10);
    ws.set_i16(c + cand::B80, e.b80);
    if forced_pos == 0xff || token as i32 == prev_token {
        let count = e.prons.len();
        let n_out;
        if count == 1 {
            ws.set_u8(c + cand::POS, 0xff);
            ws.strcpy(c + cand::PRON, &e.prons[0].1);
            n_out = 1;
        } else {
            let mut p = 0usize;
            for (k, (pos, ph)) in e.prons.iter().enumerate() {
                for &b in pos {
                    ws.set_u8(c + cand::POS + p, b);
                    p += 1;
                }
                if k + 1 < count {
                    ws.set_u8(c + cand::POS + p, 100);
                    p += 1;
                }
                ws.strcpy(c + cand::PRON + k * cand::PRON_STRIDE, ph);
            }
            ws.set_u8(c + cand::POS + p, 0xff);
            n_out = count;
        }
        ws.set_u8(c + cand::PRON + n_out * cand::PRON_STRIDE, 0);
        ws.set_i16(c + cand::COUNT, n_out as i16);
    } else {
        ws.set_u8(c + cand::POS, forced_pos);
        ws.set_u8(c + cand::POS + 1, 0xff);
        if flag && preset.is_empty() {
            pick_pron_for_pos(ws, c + cand::PRON, forced_pos, &e);
        }
    }
}

/// `FUN_10003f10`: the pronunciation whose POS list holds `pos` (')' also
/// matches '%'), else the first one listing tag 0x17, else the first.
fn pick_pron_for_pos(ws: &mut Ws, dst: usize, pos: u8, e: &crate::lexicon::LexEntry) {
    let p = pron_for_pos(e, pos);
    ws.strcpy(dst, &p);
}

pub fn pron_for_pos(e: &crate::lexicon::LexEntry, pos: u8) -> Vec<u8> {
    if e.prons.is_empty() {
        return Vec::new();
    }
    let mut fallback: Option<usize> = None;
    for (k, (list, _)) in e.prons.iter().enumerate() {
        // Mode-0x01 entries carry an empty list ({0xff} in the engine).
        for &b in list {
            if b == pos || (pos == b')' && b == b'%') {
                return e.prons[k].1.clone();
            }
            if b == 0x17 && fallback.is_none() {
                fallback = Some(k);
            }
        }
    }
    e.prons[fallback.unwrap_or(0)].1.clone()
}

/// `FUN_1000fd20`: 0 no letters, 1 lower-case, 2 one leading capital, 3 more capitals.
pub fn casing(s: &[u8]) -> u8 {
    let mut seen: i16 = 0;
    let mut r: u8 = 0;
    for &c in s {
        let cl = sc(&CHAR_CLASS, c);
        if cl & 0xc0 != 0 {
            if cl & 0x80 == 0 {
                if r == 0 {
                    seen = 1;
                    r = 1;
                }
            } else {
                let b = (seen != 0) as u8;
                seen = b as i16 + 2;
                r = b + 2;
            }
        }
    }
    r
}

const SUFFIX_OF_CLASS: [(i32, &[u8]); 6] = [(2, b"."), (3, b"?"), (4, b"!"), (5, b","), (0xc, b","), (0xb, b"-")];

/// `FUN_1000d190`. Returns false when the candidate array overflows (200).
pub fn lookup_tokens(ws: &mut Ws, wa: &Wa, n: usize, flag: bool, lex: &Lexicon) -> bool {
    ws.set_i16(0, 0);
    for t in 0..n {
        let r = Wa::rec(t);
        let mut text = wa.cstr(r + war::TEXT).to_vec();
        let is_last = t == n - 1;
        let class = wa.i32(r + war::TOKEN_CLASS);
        if wa.i32(r + war::LEN) < 0x1d && class != 0 {
            if let Some((_, s)) = SUFFIX_OF_CLASS.iter().find(|(c, _)| *c == class) {
                text.extend_from_slice(s);
            }
        }
        let forced = wa.u8(r + war::FORCED_POS);
        let preset = wa.cstr(r + war::PHONES).to_vec();
        let keep_case = wa.u8(r + war::CASE_MODE) == b'S';
        let mut buf = TokenBuf { b: text };
        loop {
            let piece = split_token(&mut buf, is_last, forced, !preset.is_empty(), keep_case, lex);
            let payload = lex.lookup_payload(&piece).unwrap_or(&[]).to_vec();
            let count = ws.i16(0);
            if count > 199 {
                return false;
            }
            let prev = if count < 1 { -1 } else { ws.i16(Ws::cand(count as usize - 1) + cand::TOKEN) as i32 };
            fill_candidate(ws, count as usize, &piece, &payload, t as i16, prev, forced, &preset, flag);
            ws.set_i16(0, count + 1);
            if buf.at(0) == 0 {
                break;
            }
        }
    }
    true
}
