//! Engine data of the 2004 DLL by virtual address (`tables::CHUNKS`, generated from a live snapshot of
//! .rdata/.data) and the C-string primitives the 2004 front end is built on.

use super::tables::CHUNKS;

/// Bytes of engine data from `va` to the end of its generated region.
pub fn mem(va: u32) -> &'static [u8] {
    let i = match CHUNKS.binary_search_by(|&(a, _)| a.cmp(&va)) {
        Ok(i) => i,
        Err(0) => panic!("2004 engine VA {:#x} not generated", va),
        Err(i) => i - 1,
    };
    let (a, d) = CHUNKS[i];
    let off = (va - a) as usize;
    assert!(off < d.len(), "2004 engine VA {:#x} not generated", va);
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

pub fn rd_u32(va: u32) -> u32 {
    rd_i32(va) as u32
}

/// The string a `char*` at `va` points to.
pub fn sp(va: u32) -> &'static [u8] {
    s(rd_u32(va))
}

/// `(&DAT_1006a860)[(char)c]`: 0x80 upper, 0x40 lower, 0x10 digit, 0x08 punct, 0x04 space.
pub fn cls(c: u8) -> u8 {
    b(0x1006a7e0, 384)[(c as i8 as i32 + 128) as usize]
}

pub fn is_alpha(c: u8) -> bool {
    cls(c) & 0xc0 != 0
}

pub fn is_digit(c: u8) -> bool {
    cls(c) & 0x10 != 0
}

/// `*(short *)(&DAT_1006aa60 + c * 2)`: the case-folding key of the insensitive compares.
pub fn fold(c: u8) -> i32 {
    fold_at(256 + 2 * c as usize)
}

/// The same table indexed by a signed `char` (`*(short *)(&DAT_1006aa60 + (char)c * 2)`).
pub fn fold_s(c: u8) -> i32 {
    fold_at((256 + 2 * c as i8 as i32) as usize)
}

/// `(&DAT_1006ad60)[(char)c * 2]`: the upper-casing table.
pub fn toupper(c: u8) -> u8 {
    b(0x1006ac60, 768)[(256 + 2 * c as i8 as i32) as usize]
}

fn fold_at(o: usize) -> i32 {
    let m = b(0x1006a960, 768);
    i16::from_le_bytes([m[o], m[o + 1]]) as i32
}

/// NUL-terminated prefix of `t`.
pub fn cs(t: &[u8]) -> &[u8] {
    &t[..t.iter().position(|&c| c == 0).unwrap_or(t.len())]
}

/// `t[i]`, reading past the end as NUL the way the engine reads its padded buffers.
pub fn at(t: &[u8], i: isize) -> u8 {
    if i < 0 {
        0
    } else {
        t.get(i as usize).copied().unwrap_or(0)
    }
}

/// `FUN_10014380`: case-insensitive `strcmp`.
pub fn stricmp(a: &[u8], b: &[u8]) -> i32 {
    let mut i = 0;
    loop {
        let (x, y) = (at(a, i as isize), at(b, i as isize));
        let d = fold(x) - fold(y);
        if d != 0 || x == 0 {
            return d;
        }
        i += 1;
    }
}

/// `FUN_100143e0`: case-insensitive `strncmp`.
pub fn strnicmp(a: &[u8], b: &[u8], n: usize) -> i32 {
    if n == 0 {
        return 0;
    }
    let mut i = 0;
    loop {
        let (x, y) = (at(a, i as isize), at(b, i as isize));
        let d = fold(x) - fold(y);
        if d != 0 {
            return d;
        }
        i += 1;
        if x == 0 || i == n {
            return 0;
        }
    }
}

pub fn strcmp(a: &[u8], b: &[u8]) -> i32 {
    let mut i = 0;
    loop {
        let (x, y) = (at(a, i as isize), at(b, i as isize));
        if x != y {
            return x as i32 - y as i32;
        }
        if x == 0 {
            return 0;
        }
        i += 1;
    }
}

pub fn strncmp(a: &[u8], b: &[u8], n: usize) -> i32 {
    for i in 0..n {
        let (x, y) = (at(a, i as isize), at(b, i as isize));
        if x != y {
            return x as i32 - y as i32;
        }
        if x == 0 {
            return 0;
        }
    }
    0
}

/// `FUN_100227e0`: none of the next `n` bytes is NUL.
pub fn has_n(t: &[u8], i: usize, n: usize) -> bool {
    (0..n).all(|k| at(t, (i + k) as isize) != 0)
}

/// MSVC `atoi`: optional whitespace and sign, then digits.
pub fn atoi(t: &[u8]) -> i32 {
    let mut i = 0;
    while matches!(at(t, i), b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        i += 1;
    }
    let neg = at(t, i) == b'-';
    if matches!(at(t, i), b'-' | b'+') {
        i += 1;
    }
    let mut v: i32 = 0;
    while at(t, i).is_ascii_digit() {
        v = v.wrapping_mul(10).wrapping_add((at(t, i) - b'0') as i32);
        i += 1;
    }
    if neg {
        v.wrapping_neg()
    } else {
        v
    }
}

/// A table of 0x30-byte records keyed by the string at +0 (`FUN_10049b90`). `count` is a literal or the
/// VA of the engine's count variable.
#[derive(Clone, Copy)]
pub struct Rec30 {
    pub va: u32,
    pub n: i32,
}

pub const fn rec30(va: u32, count: u32) -> Rec30 {
    Rec30 { va, n: count as i32 }
}

impl Rec30 {
    pub fn count(&self) -> i32 {
        if (0x10000000..0x20000000).contains(&(self.n as u32)) {
            rd_i32(self.n as u32)
        } else {
            self.n
        }
    }

    pub fn key(&self, i: i32) -> &'static [u8] {
        s(self.va + i as u32 * 0x30)
    }

    pub fn field(&self, i: i32, off: u32) -> &'static [u8] {
        s(self.va + i as u32 * 0x30 + off)
    }

    /// `FUN_10049b90`: bisection (insensitive when `ins`), -1 when absent or `w` is empty.
    pub fn find(&self, w: &[u8], ins: bool) -> i32 {
        let w = cs(w);
        if w.is_empty() {
            return -1;
        }
        let cmp = |k: &[u8]| if ins { stricmp(k, w) } else { strcmp(k, w) };
        let mut lo = 0;
        let mut hi = self.count() - 1;
        let mut mid = hi;
        while lo <= hi {
            mid /= 2;
            let c = cmp(self.key(mid));
            if c == 0 {
                return mid;
            }
            if c < 0 {
                lo = mid + 1;
            } else {
                hi = mid - 1;
            }
            mid = hi + lo;
        }
        -1
    }
}

/// A unit table: 0x60-byte records of key (+0), singular (+0x20), plural or `-suffix` (+0x40).
#[derive(Clone, Copy)]
pub struct Rec60 {
    pub va: u32,
    pub n: u32,
}

pub const fn rec60(va: u32, count: u32) -> Rec60 {
    Rec60 { va, n: count }
}

impl Rec60 {
    pub fn count(&self) -> i32 {
        rd_i32(self.n)
    }

    pub fn field(&self, i: i32, off: u32) -> &'static [u8] {
        s(self.va + i as u32 * 0x60 + off)
    }

    /// `FUN_10049c10`: bisection on keys that prefix `w` (insensitive for `I`); the rest of `w` must be
    /// empty, or the `-suffix` of a record without a singular.
    pub fn find_prefix(&self, w: &[u8], mode: u8) -> i32 {
        let w = cs(w);
        if w.is_empty() {
            return -1;
        }
        let (mut lo, mut hi) = (0, self.count() - 1);
        let mut mid = hi;
        while lo <= hi {
            mid /= 2;
            let key = self.field(mid, 0);
            let l = key.len();
            let c = if mode == b'I' { strnicmp(key, w, l) } else { strncmp(key, w, l) };
            if c == 0 {
                if at(w, l as isize) == 0 {
                    return mid;
                }
                let p = self.field(mid, 0x40);
                if self.field(mid, 0x20).is_empty() && at(p, 0) == b'-' && stricmp(&w[l..], &p[1..]) == 0 {
                    return mid;
                }
                lo = mid + 1;
            } else if c < 0 {
                lo = mid + 1;
            } else {
                hi = mid - 1;
            }
            mid = hi + lo;
        }
        -1
    }

    /// `FUN_10049d10`: exact bisection; mode `W` falls back to an insensitive linear scan.
    pub fn find_exact(&self, w: &[u8], mode: u8) -> i32 {
        let w = cs(w);
        if w.is_empty() {
            return -1;
        }
        let (mut lo, mut hi) = (0, self.count() - 1);
        let mut mid = hi;
        while lo <= hi {
            mid /= 2;
            let c = strcmp(self.field(mid, 0), w);
            if c == 0 {
                return mid;
            }
            if c < 0 {
                lo = mid + 1;
            } else {
                hi = mid - 1;
            }
            mid = lo + hi;
        }
        if mode == b'W' {
            return (0..self.count()).find(|&i| stricmp(self.field(i, 0), w) == 0).unwrap_or(-1);
        }
        -1
    }
}

/// `n` records of `size` bytes holding string pointers at the listed offsets (the generator follows them).
#[derive(Clone, Copy)]
pub struct Recp {
    pub va: u32,
    pub n: i32,
    pub size: u32,
}

pub const fn recp(va: u32, n: i32, size: u32, _ptrs: &[u32]) -> Recp {
    Recp { va, n, size }
}

impl Recp {
    pub fn rec(&self, i: i32) -> u32 {
        self.va + i as u32 * self.size
    }
}

/// A `char*` table.
#[derive(Clone, Copy)]
pub struct Ptab {
    pub va: u32,
    pub n: i32,
}

pub const fn ptab(va: u32, count: u32) -> Ptab {
    Ptab { va, n: count as i32 }
}

impl Ptab {
    pub fn count(&self) -> i32 {
        if (0x10000000..0x20000000).contains(&(self.n as u32)) {
            rd_i32(self.n as u32)
        } else {
            self.n
        }
    }

    pub fn get(&self, i: i32) -> &'static [u8] {
        sp(self.va + 4 * i as u32)
    }

    /// `FUN_10049980`: bisection, insensitive for `ins`; -1 when absent or `w` is empty.
    pub fn find(&self, w: &[u8], ins: bool) -> i32 {
        let w = cs(w);
        if w.is_empty() {
            return -1;
        }
        let (mut lo, mut hi) = (0, self.count() - 1);
        let mut mid = hi;
        while lo <= hi {
            mid /= 2;
            let k = self.get(mid);
            let c = if ins { stricmp(k, w) } else { strcmp(k, w) };
            if c == 0 {
                return mid;
            }
            if c < 0 {
                lo = mid + 1;
            } else {
                hi = mid - 1;
            }
            mid = hi + lo;
        }
        -1
    }
}
