//! `fe_InitialTextProcess` (0x10014880): request text -> processed text, the processed -> source offset
//! maps (`TIB+0x528d4`/`0x528d8`) and the five tag stacks (`TIB+0x112e78`, 12 bytes each: count, pos[],
//! val[]: pitch, speed, volume, pause, break).
//!
//! Two filter passes run through `FUN_10014ee0` (a counting run, then the writing run):
//! `fe_FilterPassA` maps the fullwidth pairs of `0x100690c0` to ASCII, `fe_FilterPassC` cuts the ten
//! `<vt_*>` tags of `0x1006bea8`. The SSML/SAPI pass `fe_FilterPassB` only runs when `TIB+0x111e9a` is set,
//! which no API of this build does by default.

use super::mem::*;

#[derive(Clone, Default, Debug, PartialEq)]
pub struct Stack {
    pub pos: Vec<i32>,
    pub val: Vec<i32>,
}

pub const PITCH: usize = 0;
pub const SPEED: usize = 1;
pub const VOLUME: usize = 2;
pub const PAUSE: usize = 3;
pub const BREAK: usize = 4;

#[derive(Clone, Default, Debug)]
pub struct Initial {
    pub processed: Vec<u8>,
    pub map_a: Vec<i32>,
    pub map_b: Vec<i32>,
    /// Source byte -> character index (a byte with the high bit set and a successor counts as one
    /// character with it).
    pub map_c: Vec<i32>,
    pub stacks: [Stack; 5],
}

/// `fe_FullwidthCmp` bisection over the 0x48 16-byte records of `0x100690c0`: u16 key `lead << 8 | trail`
/// at +0, replacement strings at +4/+8 (the flag byte at +0xc picks +8).
fn fullwidth(c0: u8, c1: u8) -> Option<&'static [u8]> {
    const TAB: Recp = recp(0x100690c0, 0x48, 0x10, &[4, 8]);
    let key = ((c0 as i32) << 8) | c1 as i32;
    let (mut lo, mut hi) = (0i32, TAB.n - 1);
    while lo <= hi {
        let mid = (lo + hi) / 2;
        let rec = TAB.rec(mid);
        let k = rd_i16(rec) as u16 as i32;
        if k == key {
            let flag = b(rec + 0xc, 1)[0];
            return Some(sp(rec + if flag != 0 { 8 } else { 4 }));
        }
        if key - k < 0 {
            hi = mid - 1;
        } else {
            lo = mid + 1;
        }
    }
    None
}

/// `fe_FilterPassA` (0x10015000): any high byte with a successor is a pair; a pair in the table becomes
/// its replacement, every output byte of a pair maps to the lead (`a`) and the trail (`b`).
fn pass_a(src: &[u8], a0: &[i32], b0: &[i32]) -> (Vec<u8>, Vec<i32>, Vec<i32>) {
    let n = src.len();
    let (mut out, mut a, mut b) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
    let mut i = 0;
    while i < n {
        if !has_n(src, i, 2) || src[i] & 0x80 == 0 {
            out.push(src[i]);
            a.push(a0[i]);
            b.push(b0[i]);
            i += 1;
            continue;
        }
        let rep: Vec<u8> = match fullwidth(src[i], src[i + 1]) {
            Some(r) => r.to_vec(),
            None => src[i..i + 2].to_vec(),
        };
        for &c in &rep {
            out.push(c);
            a.push(a0[i]);
            b.push(b0[i + 1]);
        }
        i += 2;
    }
    (out, a, b)
}

/// The ten tag entries of `0x1006bea8`: (name, handler).
#[derive(Clone, Copy, PartialEq)]
enum H {
    Level(usize),
    Voice,
    Pause,
    Break,
}

const TAGS: [(&[u8], H); 10] = [
    (b"<vt_break", H::Break),
    (b"<vt_pitch", H::Level(PITCH)),
    (b"</vt_pitch", H::Level(PITCH)),
    (b"<vt_speed", H::Level(SPEED)),
    (b"</vt_speed", H::Level(SPEED)),
    (b"<vt_volume", H::Level(VOLUME)),
    (b"</vt_volume", H::Level(VOLUME)),
    (b"<vt_voice", H::Voice),
    (b"</vt_voice", H::Voice),
    (b"<vt_pause", H::Pause),
];

/// Clamps `DAT_1006c128..` (min) / `DAT_1006c110..` (max) per stack.
fn clamp(k: usize, v: i32) -> i32 {
    let m = b(0x1006c110, 0x2c);
    let rd = |o: usize| i32::from_le_bytes([m[o], m[o + 1], m[o + 2], m[o + 3]]);
    let (lo, hi) = (rd(0x18 + 4 * k), rd(4 * k));
    let v = if v < lo { lo } else { v };
    if hi < v {
        hi
    } else {
        v
    }
}

/// Regex 0 of `0x1006bfb0`: `^[ \f\n\r\t\v]*$`.
fn blank(body: &[u8]) -> bool {
    body.iter().all(|c| b" \x0c\n\r\t\x0b".contains(c))
}

/// Regexes 0x2f `^=([0-9]+)$` and 0x30 `^=([0-2]+)$`.
fn eq_digits(body: &[u8], max: u8) -> bool {
    body.len() >= 2 && body[0] == b'=' && body[1..].iter().all(|&c| (b'0'..=max).contains(&c))
}

/// `FUN_1001c830` (`strtok` with `" \f\n\r\t\v="`) then `atoi`.
fn value(body: &[u8]) -> i32 {
    let delim = |c: &u8| b" \x0c\n\r\t\x0b=".contains(c);
    let st = body.iter().position(|c| !delim(c)).unwrap_or(body.len());
    let en = body[st..].iter().position(delim).map(|e| st + e).unwrap_or(body.len());
    atoi(&body[st..en])
}

struct St<'a> {
    src: &'a [u8],
    a0: &'a [i32],
    b0: &'a [i32],
    out: Vec<u8>,
    a: Vec<i32>,
    b: Vec<i32>,
    stacks: [Stack; 5],
}

impl St<'_> {
    fn put(&mut self, c: u8, i: usize) {
        self.out.push(c);
        self.a.push(self.a0[i]);
        self.b.push(self.b0[i]);
    }

    /// `FUN_10022810` with table 4: the longest case-insensitive name match, then its handler.
    fn tag(&mut self, pos: usize) -> Option<usize> {
        let rest = &self.src[pos..];
        let mut best: Option<usize> = None;
        let mut blen = 0;
        for (k, (name, _)) in TAGS.iter().enumerate() {
            if has_n(rest, 0, name.len()) && strnicmp(rest, name, name.len()) == 0 && blen < name.len() {
                best = Some(k);
                blen = name.len();
            }
        }
        let (name, h) = TAGS[best?];
        // FUN_10022940: the body runs to the first ">" (at most 0xff bytes).
        let end = rest.iter().position(|&c| c == 0).unwrap_or(rest.len());
        let gt = rest[..end].iter().position(|&c| c == b'>')?;
        let blen = gt as isize - name.len() as isize;
        if blen > 0xff {
            return None;
        }
        let body = &rest[name.len()..gt];
        let total = body.len() + 1 + name.len();
        match h {
            H::Voice => Some(total),
            H::Level(k) => {
                let v = if !blank(body) {
                    if !eq_digits(body, b'9') {
                        return None;
                    }
                    let v = clamp(k, value(body));
                    if k == SPEED {
                        10000 / v
                    } else {
                        v
                    }
                } else if strnicmp(rest, s(0x1006ebd4), 2) == 0 {
                    -1
                } else {
                    return None;
                };
                let p = self.out.len() as i32;
                self.stacks[k].pos.push(p);
                self.stacks[k].val.push(v);
                Some(total)
            }
            H::Pause => {
                if blank(body) || !eq_digits(body, b'9') {
                    return None;
                }
                let v = clamp(PAUSE, value(body));
                self.put(b',', pos);
                self.put(b' ', pos);
                let p = self.out.len() as i32 - 1;
                self.stacks[PAUSE].pos.push(p);
                self.stacks[PAUSE].val.push(v);
                Some(total)
            }
            H::Break => {
                if blank(body) || !eq_digits(body, b'2') {
                    return None;
                }
                let v = clamp(BREAK, value(body));
                self.put(b' ', pos);
                let p = self.out.len() as i32 - 1;
                self.stacks[BREAK].pos.push(p);
                self.stacks[BREAK].val.push(v);
                Some(total)
            }
        }
    }
}

/// Mark matched open/close pairs: scanning from the end, an open entry pairs with the first later entry
/// that is a close not yet taken.
fn matching(val: &[i32]) -> (Vec<bool>, Vec<Option<usize>>) {
    let n = val.len();
    let mut keep = vec![false; n];
    let mut partner = vec![None; n];
    for i in (0..n).rev() {
        if val[i] < 0 {
            continue;
        }
        let mut j = i;
        while j < n && (val[j] >= 0 || keep[j]) {
            j += 1;
        }
        if j != n {
            keep[i] = true;
            keep[j] = true;
            partner[i] = Some(j);
            partner[j] = Some(i);
        }
    }
    (keep, partner)
}

/// `fe_FilterPassC`'s pairing of the pitch/speed/volume stacks. The removal of unmatched entries shifts
/// the arrays at the original index while the cursor keeps advancing (entries after a removal are
/// judged by the flags of their old neighbours), as the engine does. Values then nest by percent: an open
/// scales the running value, a close restores the value before its open (100 at the start).
fn pair(st: &mut Stack) {
    let n = st.val.len();
    let (keep, _) = matching(&st.val);
    let mut len = n;
    for c in 0..n {
        if !keep[c] {
            if c != n - 1 {
                let cnt = len as isize - 1 - c as isize;
                if cnt > 0 {
                    let cnt = cnt as usize;
                    st.pos.copy_within(c + 1..c + 1 + cnt, c);
                    st.val.copy_within(c + 1..c + 1 + cnt, c);
                }
            }
            len -= 1;
        }
    }
    st.pos.truncate(len);
    st.val.truncate(len);
    let (_, partner) = matching(&st.val);
    let mut run = 100;
    for i in 0..len {
        if st.val[i] < 0 {
            st.val[i] = match partner[i] {
                Some(0) | None => 100,
                Some(o) => st.val[o - 1],
            };
            run = st.val[i];
        } else {
            run = st.val[i] * run / 100;
            st.val[i] = run;
        }
    }
}

pub fn initial(src: &[u8]) -> Initial {
    let src = cs(src);
    let n = src.len();
    if n == 0 {
        return Initial::default();
    }
    let mut map_c = vec![0i32; n];
    let (mut i, mut ch) = (0usize, 0i32);
    while i < n {
        map_c[i] = ch;
        if src[i] & 0x80 == 0 || i == n - 1 {
            i += 1;
        } else {
            map_c[i + 1] = ch;
            i += 2;
        }
        ch += 1;
    }
    let ident: Vec<i32> = (0..n as i32).collect();
    let (fw, a0, b0) = pass_a(src, &ident, &ident);
    let mut st = St { src: &fw, a0: &a0, b0: &b0, out: Vec::new(), a: Vec::new(), b: Vec::new(), stacks: Default::default() };
    let mut i = 0;
    while i < fw.len() {
        if let Some(r) = st.tag(i) {
            i += r;
            continue;
        }
        if !has_n(&fw, i, 2) || fw[i] & 0x80 == 0 {
            st.put(fw[i], i);
            i += 1;
        } else {
            st.out.extend_from_slice(&fw[i..i + 2]);
            st.a.push(a0[i]);
            st.a.push(a0[i]);
            st.b.push(b0[i + 1]);
            st.b.push(b0[i + 1]);
            i += 2;
        }
    }
    for k in 0..3 {
        if !st.stacks[k].val.is_empty() {
            pair(&mut st.stacks[k]);
        }
    }
    Initial { processed: st.out, map_a: st.a, map_b: st.b, map_c, stacks: st.stacks }
}
