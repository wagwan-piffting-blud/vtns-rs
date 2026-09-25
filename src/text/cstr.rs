//! C-string and engine-data primitives the front-end transliteration is built on.
//!
//! Text is handled the way the engine handles it: CP1252/EUC-KR bytes, NUL
//! terminated, read past the end as zeros. Engine tables are read by virtual
//! address from `tables::CHUNKS` (generated from the DLL's data sections).

use super::tables::CHUNKS;

/// Bytes of engine data from `va` to the end of its generated region.
pub fn mem(va: u32) -> &'static [u8] {
    let i = match CHUNKS.binary_search_by(|&(a, _)| a.cmp(&va)) {
        Ok(i) => i,
        Err(0) => panic!("engine VA {:#x} not generated", va),
        Err(i) => i - 1,
    };
    let (a, d) = CHUNKS[i];
    let off = (va - a) as usize;
    assert!(off < d.len(), "engine VA {:#x} not generated", va);
    &d[off..]
}

/// NUL-terminated engine string at `va`, without the NUL.
pub fn s(va: u32) -> &'static [u8] {
    let m = mem(va);
    &m[..m.iter().position(|&c| c == 0).unwrap_or(m.len())]
}

/// `n` bytes of engine data at `va`.
pub fn b(va: u32, n: usize) -> &'static [u8] {
    &mem(va)[..n]
}

pub fn rd_i16(va: u32) -> i16 {
    let m = mem(va);
    i16::from_le_bytes([m[0], m[1]])
}

pub fn rd_i32(va: u32) -> i32 {
    let m = mem(va);
    i32::from_le_bytes([m[0], m[1], m[2], m[3]])
}

// Character tables. The engine indexes some with a signed char and some with an
// unsigned byte, so each is generated over the -128..=255 index range.

fn class_tab() -> &'static [u8] {
    b(0x1007e108, 384)
}

/// `(&DAT_1007e188)[(char)c]`: 0x80 upper, 0x40 lower, 0x20 hex, 0x10 digit, 0x08 punct, 0x04 space.
pub fn cls(c: u8) -> u8 {
    class_tab()[(c as i8 as i32 + 128) as usize]
}

/// `(&DAT_1007e188)[(uint)c]`: the unsigned view also classifies Latin-1 letters.
pub fn cls_u(c: u8) -> u8 {
    class_tab()[c as usize + 128]
}

fn i16_at(m: &[u8], idx: i32) -> i16 {
    let o = ((idx + 128) * 2) as usize;
    i16::from_le_bytes([m[o], m[o + 1]])
}

/// `*(short *)(&DAT_1007e388 + (char)c * 2)`: signed-index tolower.
pub fn lower(c: u8) -> i16 {
    i16_at(b(0x1007e288, 768), c as i8 as i32)
}

/// `*(short *)(&DAT_1007e388 + (uint)c * 2)`.
pub fn lower_u(c: u8) -> i16 {
    i16_at(b(0x1007e288, 768), c as i32)
}

/// `*(short *)(&DAT_1007e688 + (char)c * 2)`: signed-index toupper.
pub fn upper(c: u8) -> i16 {
    i16_at(b(0x1007e588, 768), c as i8 as i32)
}

/// `*(short *)(&DAT_1009ad24 + (uint)c * 2)`; only its nonzero-ness is ever used.
pub fn ad24(c: u8) -> i16 {
    let m = b(0x1009ad24, 512);
    i16::from_le_bytes([m[c as usize * 2], m[c as usize * 2 + 1]])
}

pub fn is_alpha(c: u8) -> bool {
    cls(c) & 0xc0 != 0
}

pub fn is_digit(c: u8) -> bool {
    cls(c) & 0x10 != 0
}

pub fn is_upper(c: u8) -> bool {
    cls(c) & 0x80 != 0
}

pub fn is_lower(c: u8) -> bool {
    cls(c) & 0x40 != 0
}

pub fn is_ws(c: u8) -> bool {
    c == b' ' || c == b'\t' || c == b'\n' || c == b'\r'
}

/// The engine's "Latin-1 letter" test used before its accent folding:
/// `(c >= 0xc0 && c != 0xd7 && c != 0xf7) || c in {8a,8c,9a,9c,9f}`.
pub fn latin_letter(c: u8) -> bool {
    (c > 0xbf && c != 0xd7 && c != 0xf7) || matches!(c, 0x8a | 0x8c | 0x9a | 0x9c | 0x9f)
}

/// EUC-KR double-byte start: `0xa1..=0xad` + `0xa1..=0xfe`, `fd fe`, or `ae a1..=c2`.
pub fn dbcs_pair(c0: u8, c1: u8) -> bool {
    ((0xa0 < c0 && c0 < 0xae) && (0xa0 < c1 && c1 != 0xff)) || (c0 == 0xfd && c1 == 0xfe) || (c0 == 0xae && (0xa0 < c1 && c1 < 0xc3))
}

/// Byte `i` of a NUL-terminated buffer; out of range reads as NUL.
#[inline]
pub fn at(t: &[u8], i: isize) -> u8 {
    if i < 0 {
        0
    } else {
        t.get(i as usize).copied().unwrap_or(0)
    }
}

pub fn strlen(t: &[u8]) -> usize {
    t.iter().position(|&c| c == 0).unwrap_or(t.len())
}

/// C string (up to the first NUL) as a slice.
pub fn cs(t: &[u8]) -> &[u8] {
    &t[..strlen(t)]
}

/// C `strchr(set, c) != NULL`; like C it also matches the terminating NUL.
pub fn strchr(set: &[u8], c: u8) -> bool {
    c == 0 || cs(set).contains(&c)
}

/// Index of the first `c` in the C string `t`.
pub fn strchr_at(t: &[u8], c: u8) -> Option<usize> {
    cs(t).iter().position(|&x| x == c)
}

/// `FUN_1001c1f0`: strcmp returning the unsigned byte difference.
pub fn strcmp(a: &[u8], b: &[u8]) -> i32 {
    let mut i = 0isize;
    loop {
        let (x, y) = (at(a, i), at(b, i));
        let d = x as i32 - y as i32;
        if d != 0 || x == 0 {
            return d;
        }
        i += 1;
    }
}

/// `FUN_1001c280`: strncmp.
pub fn strncmp(a: &[u8], b: &[u8], n: usize) -> i32 {
    for i in 0..n as isize {
        let (x, y) = (at(a, i), at(b, i));
        let d = x as i32 - y as i32;
        if d != 0 {
            return d;
        }
        if x == 0 {
            return 0;
        }
    }
    0
}

/// `FUN_1001c2c0`: case-insensitive compare through the unsigned tolower table.
pub fn stricmp(a: &[u8], b: &[u8]) -> i32 {
    let mut i = 0isize;
    loop {
        let d = lower_u(at(a, i)) as i32 - lower_u(at(b, i)) as i32;
        if d != 0 {
            return d;
        }
        if at(a, i) == 0 {
            return 0;
        }
        i += 1;
    }
}

/// `FUN_1001c320`: case-insensitive strncmp.
pub fn strnicmp(a: &[u8], b: &[u8], n: usize) -> i32 {
    if n == 0 {
        return 0;
    }
    let mut i = 0isize;
    let mut left = n;
    loop {
        let d = lower_u(at(a, i)) as i32 - lower_u(at(b, i)) as i32;
        if d != 0 {
            return d;
        }
        let c = at(a, i);
        i += 1;
        left -= 1;
        if c == 0 || left == 0 {
            return 0;
        }
    }
}

/// `FUN_10062370` / `FUN_10061f90`: the next `n` bytes are all non-NUL.
pub fn has_n(t: &[u8], i: isize, n: usize) -> bool {
    (0..n as isize).all(|k| at(t, i + k) != 0)
}

/// Copy the C string `src` into `dst`, NUL-terminated (C `strcpy`).
pub fn strcpy(dst: &mut [u8], src: &[u8]) {
    let n = strlen(src);
    dst[..n].copy_from_slice(&src[..n]);
    dst[n] = 0;
}

/// C `strcat`.
pub fn strcat(dst: &mut [u8], src: &[u8]) {
    let d = strlen(dst);
    let n = strlen(src);
    dst[d..d + n].copy_from_slice(&src[..n]);
    dst[d + n] = 0;
}

/// C `strncpy(dst, src, n)` (pads with NULs only while `src` is shorter than `n`).
pub fn strncpy(dst: &mut [u8], src: &[u8], n: usize) {
    let mut ended = false;
    for i in 0..n {
        let c = if ended { 0 } else { at(src, i as isize) };
        if c == 0 {
            ended = true;
        }
        dst[i] = c;
    }
}

/// `sprintf(dst, "%s", src)`-style append at `dst[at..]`.
pub fn put_str(dst: &mut [u8], at_: usize, src: &[u8]) -> usize {
    let n = strlen(src);
    dst[at_..at_ + n].copy_from_slice(&src[..n]);
    dst[at_ + n] = 0;
    n
}

/// C `atoi`.
pub fn atoi(t: &[u8]) -> i32 {
    let mut i = 0usize;
    while i < t.len() && matches!(t[i], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        i += 1;
    }
    let mut neg = false;
    if i < t.len() && (t[i] == b'-' || t[i] == b'+') {
        neg = t[i] == b'-';
        i += 1;
    }
    let mut v: i32 = 0;
    while i < t.len() && t[i].is_ascii_digit() {
        v = v.wrapping_mul(10).wrapping_add((t[i] - b'0') as i32);
        i += 1;
    }
    if neg {
        v.wrapping_neg()
    } else {
        v
    }
}
