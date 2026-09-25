//! Character-level helpers used by the tokenizer and the normalizers.

use super::cstr::*;

/// `FUN_10062f50`: number of phones the letter `c` spells out as (A = 1: EY1,
/// B = 2: B IY1, ...). The tokenizer caps a word at 0x40 accumulated
/// `1 + phones(c)` so over-long spell-outs are split.
pub fn letter_phones(c: u8) -> i32 {
    if !latin_letter(c) || rd_i16(0x1009ae24 + (c as u32 - 0x80) * 2) == 0 {
        let k = cls_u(c);
        if k & 0xc0 != 0 {
            let i = if k & 0x80 == 0 { c as i32 - 0x61 } else { c as i32 - 0x41 };
            if (0..0x1a).contains(&i) {
                return rd_i16(0x10077e58 + i as u32 * 10) as i32;
            }
        }
        0
    } else {
        let i = c as i32 - 0x80;
        if (0..0x80).contains(&i) {
            rd_i16(0x1009af24 + i as u32 * 2) as i32
        } else {
            0
        }
    }
}

/// `FUN_10062fe0`: total spelled-out phones of a word.
pub fn letter_phones_sum(w: &[u8]) -> i32 {
    cs(w).iter().map(|&c| letter_phones(c)).sum()
}

/// Force-generate the per-letter tables used above.
#[allow(dead_code)]
fn _refs() {
    let _ = (b(0x1009ae24, 256), b(0x1009af24, 256), b(0x10077e58, 260));
}

/// `FUN_1005c660`: bytes a CP1252 high character expands to in the token
/// buffer (0 = dropped, 2 = digraph such as "ae"/"th"/EUC-KR symbol, 1 otherwise).
pub fn high_width(c: u8) -> i32 {
    match c {
        0x80 | 0x81 | 0x86 | 0x87 | 0x88 | 0x8d | 0x8e | 0x8f | 0x90 | 0x95 | 0x98 | 0x9d | 0x9e | 0xa1 | 0xa8 | 0xaa
        | 0xac | 0xad | 0xaf | 0xb4 | 0xb7 | 0xb8 | 0xba | 0xbf => 0,
        0x82 | 0x85 | 0x89 | 0x8a | 0x8c | 0x97 | 0x99 | 0x9a | 0x9c | 0xa2 | 0xa3 | 0xa5 | 0xa7 | 0xae | 0xb0 | 0xb1
        | 0xb2 | 0xb3 | 0xb9 | 0xbc | 0xbd | 0xbe | 0xc6 | 0xd0 | 0xd7 | 0xdf | 0xe6 | 0xf0 | 0xf7 => 2,
        _ => 1,
    }
}

/// `FUN_100621f0`: is `t[p..]` (starting at an apostrophe, after letter `prev`)
/// a contraction suffix? Writes it (with the apostrophe) to `out`.
pub fn contraction(out: &mut [u8], prev: u8, t: &[u8], p: isize) -> bool {
    out[0] = 0;
    if at(t, p) != b'\'' || prev == 0 {
        return false;
    }
    let q0 = at(t, p + 1);
    if lower(prev) == b's' as i16 && matches!(q0, b' ' | b'\t' | b'\n' | b'\r') {
        out[0] = b'\'';
        out[1] = 0;
        return true;
    }
    let first = lower(prev) == b'n' as i16 && lower(q0) == b't' as i16 && cls(at(t, p + 2)) & 0xc0 == 0;
    let s2 = lower(q0);
    if first || (matches!(s2, 0x73 | 100 | 0x6d) && cls(at(t, p + 2)) & 0xc0 == 0) {
        out[0] = b'\'';
        out[1] = q0;
        out[2] = 0;
        return true;
    }
    let c2 = lower(at(t, p + 2));
    if ((s2 == 0x76 && c2 == 0x65) || (s2 == 0x65 && c2 == 0x6d) || (s2 == 0x72 && c2 == 0x65) || (s2 == 0x6c && c2 == 0x6c))
        && cls(at(t, p + 3)) & 0xc0 == 0
    {
        out[0] = b'\'';
        out[1] = q0;
        out[2] = at(t, p + 2);
        out[3] = 0;
        return true;
    }
    false
}

/// `FUN_10061fc0`: EUC-KR full-width letter (A3 C1..DA / A3 E1..FA).
pub fn fw_letter(t: &[u8], p: isize) -> bool {
    has_n(t, p, 2) && at(t, p) == 0xa3 && {
        let c = at(t, p + 1);
        (0xc0 < c && c < 0xdb) || (0xe0 < c && c < 0xfb)
    }
}

/// `FUN_10062000`: EUC-KR full-width digit (A3 B0..B9).
pub fn fw_digit(t: &[u8], p: isize) -> bool {
    has_n(t, p, 2) && at(t, p) == 0xa3 && {
        let c = at(t, p + 1);
        0xaf < c && c < 0xba
    }
}

/// `FUN_10061b00`: fold an EUC-KR string in `buf` to ASCII. Mode 1/2 map
/// full-width letters/digits (second byte - 0x80); otherwise the first symbol
/// pair is replaced by its ASCII punctuation. Returns false when `buf` does not
/// start with an EUC-KR pair (buffer untouched).
pub fn fold_dbcs(buf: &mut [u8], mode: i32) -> bool {
    let c0 = buf[0];
    let c1 = buf[1];
    if !dbcs_pair(c0, c1) {
        return false;
    }
    let n = strlen(buf) as i32;
    let mut out = [0u8; 64];
    if mode == 1 || mode == 2 {
        let mut j = 0usize;
        let mut i = 0i32;
        while i < n {
            out[j] = buf[(i + 1) as usize].wrapping_sub(0x80);
            j += 1;
            i += 2;
        }
        out[j] = 0;
    } else {
        let two = |va: u32| -> [u8; 3] {
            let m = b(va, 2);
            [m[0], m[1], 0]
        };
        let three = |va: u32, va2: u32| -> [u8; 3] {
            let m = b(va, 2);
            [m[0], m[1], b(va2, 1)[0]]
        };
        let r: Option<[u8; 3]> = match (c0, c1) {
            (0xa1, 0xa3) | (0xa3, 0xae) => Some(two(0x10077700)),
            (0xa1, 0xa8) | (0xa1, 0xb0) | (0xa1, 0xb1) | (0xa3, 0xa2) => Some(two(0x1007d4f8)),
            (0xa1, 0xa9) | (0xa1, 0xaa) | (0xa3, 0xad) => Some(two(0x1007739c)),
            (0xa1, 0xab) | (0xa1, 0xad) => Some(three(0x1009c7a0, 0x1009c7a2)),
            (0xa1, 0xac) | (0xa3, 0xdc) => Some(two(0x1007d944)),
            (0xa1, 0xae) | (0xa3, 0xe0) => Some(two(0x1009c79c)),
            (0xa1, 0xaf) | (0xa3, 0xa7) => Some(two(0x1009c3a4)),
            (0xa1, 0xb2) | (0xa1, 0xb8) | (0xa1, 0xba) | (0xa1, 0xbc) | (0xa3, 0xdb) => Some(two(0x1009c798)),
            (0xa1, 0xb3) | (0xa1, 0xb9) | (0xa1, 0xbb) | (0xa1, 0xbd) | (0xa3, 0xdd) => Some(two(0x1009c794)),
            (0xa1, 0xb4) | (0xa3, 0xbc) => Some(two(0x1007d480)),
            (0xa1, 0xb5) | (0xa3, 0xbe) => Some(two(0x1007d464)),
            (0xa1, 0xb6) => Some(three(0x1009c790, 0x1009c792)),
            (0xa1, 0xb7) => Some(three(0x1009c78c, 0x1009c78e)),
            (0xa3, 0xa1) => Some(two(0x100776f8)),
            (0xa3, 0xa3) => Some(two(0x10079280)),
            (0xa3, 0xa4) => Some(two(0x10080178)),
            (0xa3, 0xa5) => Some(two(0x1009c788)),
            (0xa3, 0xa6) => Some(two(0x10078bf4)),
            (0xa3, 0xa8) => Some(two(0x1009c784)),
            (0xa3, 0xa9) => Some(two(0x1009c780)),
            (0xa3, 0xaa) => Some(two(0x1007b47c)),
            (0xa3, 0xab) => Some(two(0x1007b480)),
            (0xa3, 0xac) => Some(two(0x100776fc)),
            (0xa3, 0xaf) => Some(two(0x1007d4e8)),
            (0xa3, 0xba) => Some(two(0x1007d48c)),
            (0xa3, 0xbb) => Some(two(0x1009c77c)),
            (0xa3, 0xbd) => Some(two(0x1007d470)),
            (0xa3, 0xbf) => Some(two(0x100776f4)),
            (0xa3, 0xc0) => Some(two(0x1009c778)),
            (0xa3, 0xde) => Some(two(0x1009c774)),
            (0xa3, 0xdf) => Some(two(0x1007d3a0)),
            (0xa3, 0xfb) => Some(two(0x1009c770)),
            (0xa3, 0xfc) => Some(two(0x1009c458)),
            (0xa3, 0xfd) => Some(two(0x1009c76c)),
            _ => None,
        };
        match r {
            Some(v) => {
                out[..3].copy_from_slice(&v);
            }
            None => {
                strcpy(&mut out, buf);
            }
        }
    }
    strcpy(buf, &out);
    true
}
