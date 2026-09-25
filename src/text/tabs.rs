//! Engine lookup tables used by the normalizers: arrays of 0x30-byte records
//! (`FUN_10056150`, key at +0, output at +0x10) and `char*` tables
//! (`FUN_100560a0`). Both bisect when the count is non-negative and scan
//! linearly otherwise; `'I'` compares through the tolower table.
//!
//! Call sites write `rec30(TABLE, COUNT)` / `ptab(TABLE, COUNT)` with literal
//! VAs so the table and its strings can be extracted from the binary.

use super::cstr::*;

pub fn search(n: i32, insensitive: bool, word: &[u8], key: impl Fn(i32) -> &'static [u8]) -> i32 {
    if cs(word).is_empty() {
        return -1;
    }
    let cmp = |k: &[u8]| if insensitive { stricmp(k, word) } else { strcmp(k, word) };
    let mut hi = n - 1;
    let mut lo = 0;
    if n < 0 {
        let mut i = 0;
        while i <= hi {
            if cmp(key(i)) == 0 {
                return i;
            }
            i += 1;
        }
        return -1;
    }
    if hi < 0 {
        return -1;
    }
    let mut mid = hi;
    loop {
        mid /= 2;
        let c = cmp(key(mid));
        if c == 0 {
            return mid;
        }
        if c < 0 {
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
        mid = hi + lo;
        if lo > hi {
            return -1;
        }
    }
}

/// A table of 0x30-byte records whose count lives at `cva`.
#[derive(Clone, Copy)]
pub struct Rec30 {
    pub va: u32,
    pub cva: u32,
}

pub const fn rec30(va: u32, cva: u32) -> Rec30 {
    Rec30 { va, cva }
}

impl Rec30 {
    pub fn count(&self) -> i32 {
        rd_i32(self.cva)
    }

    /// `FUN_10056150`.
    pub fn find(&self, word: &[u8], insensitive: bool) -> i32 {
        let va = self.va;
        search(self.count(), insensitive, word, |i| s(va + i as u32 * 0x30))
    }

    /// NUL-terminated field at `off` of record `i`.
    pub fn field(&self, i: i32, off: u32) -> &'static [u8] {
        s(self.va + i as u32 * 0x30 + off)
    }
}

/// A `char*` table whose count lives at `cva`.
#[derive(Clone, Copy)]
pub struct Ptab {
    pub va: u32,
    pub cva: u32,
}

pub const fn ptab(va: u32, cva: u32) -> Ptab {
    Ptab { va, cva }
}

impl Ptab {
    pub fn count(&self) -> i32 {
        rd_i32(self.cva)
    }

    pub fn get(&self, i: i32) -> &'static [u8] {
        s(rd_i32(self.va + 4 * i as u32) as u32)
    }

    /// `FUN_100560a0`.
    pub fn find(&self, word: &[u8], insensitive: bool) -> i32 {
        search(self.count(), insensitive, word, |i| self.get(i))
    }
}

/// A table of 0x5a-byte unit records: key at +0, alternative spelling at
/// +0x1e, plural at +0x3c (`-s` = suffix to append).
#[derive(Clone, Copy)]
pub struct Rec5a {
    pub va: u32,
    pub cva: u32,
}

pub const fn rec5a(va: u32, cva: u32) -> Rec5a {
    Rec5a { va, cva }
}

impl Rec5a {
    pub fn count(&self) -> i32 {
        rd_i32(self.cva)
    }

    pub fn field(&self, i: i32, off: u32) -> &'static [u8] {
        s(self.va + i as u32 * 0x5a + off)
    }

    /// `FUN_10056210`: the key is a prefix of `word` and the rest is empty or
    /// the record's `-suffix` plural.
    pub fn find_prefix(&self, word: &[u8], insensitive: bool) -> i32 {
        if cs(word).is_empty() {
            return -1;
        }
        let cmp = |a: &[u8], b: &[u8], n: usize| if insensitive { strnicmp(a, b, n) } else { strncmp(a, b, n) };
        let hit = |i: i32| -> Option<bool> {
            let key = self.field(i, 0);
            let kl = key.len();
            if cmp(key, word, kl) != 0 {
                return None;
            }
            if at(word, kl as isize) == 0 {
                return Some(true);
            }
            let pl = self.field(i, 0x3c);
            Some(self.field(i, 0x1e).is_empty() && at(pl, 0) == b'-' && stricmp(&word[kl..], &pl[1..]) == 0)
        };
        let n = self.count();
        if n < 0 {
            for i in 0..-n {
                if hit(i) == Some(true) {
                    return i;
                }
            }
            return -1;
        }
        let mut hi = n - 1;
        let mut lo = 0;
        if hi < 0 {
            return -1;
        }
        let mut mid = hi;
        loop {
            mid /= 2;
            let key = self.field(mid, 0);
            let c = cmp(key, word, key.len());
            if c == 0 {
                if hit(mid) == Some(true) {
                    return mid;
                }
                lo = mid + 1;
            } else if c < 0 {
                lo = mid + 1;
            } else {
                hi = mid - 1;
            }
            mid = hi + lo;
            if hi < lo {
                return -1;
            }
        }
    }

    /// `FUN_10056430`: exact case-sensitive key.
    pub fn find_exact(&self, word: &[u8]) -> i32 {
        if cs(word).is_empty() {
            return -1;
        }
        let n = self.count();
        if n < 0 {
            for i in 0..-n {
                if strcmp(self.field(i, 0), word) == 0 {
                    return i;
                }
            }
            return -1;
        }
        let (mut lo, mut hi) = (0, n - 1);
        if hi < 0 {
            return -1;
        }
        let mut mid = hi;
        loop {
            mid /= 2;
            let c = strcmp(self.field(mid, 0), word);
            if c == 0 {
                return mid;
            }
            if c < 0 {
                lo = mid + 1;
            } else {
                hi = mid - 1;
            }
            mid = hi + lo;
            if lo > hi {
                return -1;
            }
        }
    }
}

/// A table of 0x66-byte records: key at +0, a plural field at +0xf (`-suffix`
/// to append, a full irregular spelling, or empty for "no distinct plural").
/// Used by the currency name tables (`FUN_10056370`/`FUN_10056e50`).
#[derive(Clone, Copy)]
pub struct Rec66 {
    pub va: u32,
    pub cva: u32,
}

pub const fn rec66(va: u32, cva: u32) -> Rec66 {
    Rec66 { va, cva }
}

impl Rec66 {
    pub fn count(&self) -> i32 {
        rd_i32(self.cva)
    }

    pub fn field(&self, i: i32, off: u32) -> &'static [u8] {
        s(self.va + i as u32 * 0x66 + off)
    }

    /// `FUN_10056370`: exact key match only (used for the ISO-code table).
    pub fn find_exact(&self, word: &[u8], insensitive: bool) -> i32 {
        search(self.count(), insensitive, word, |i| self.field(i, 0))
    }

    /// `FUN_10056e50`: the key matches `word`, or the key plus its `-suffix`
    /// plural does. Its multi-word continuation is unreachable: keys such as
    /// "new lira" are compared over their whole length against one token.
    pub fn find_word(&self, word: &[u8], insensitive: bool) -> i32 {
        if cs(word).is_empty() {
            return -1;
        }
        let cmp = |a: &[u8], b: &[u8], n: usize| if insensitive { strnicmp(a, b, n) } else { strncmp(a, b, n) };
        let hit = |i: i32| -> bool {
            let key = self.field(i, 0);
            let kl = key.len();
            if cmp(key, word, kl) != 0 {
                return false;
            }
            if at(word, kl as isize) == 0 {
                return true;
            }
            let pl = self.field(i, 0xf);
            at(pl, 0) == b'-' && stricmp(&word[kl..], &pl[1..]) == 0
        };
        let n = self.count();
        if n < 0 {
            for i in 0..-n {
                if hit(i) {
                    return i;
                }
            }
            return -1;
        }
        let mut hi = n - 1;
        let mut lo = 0;
        if hi < 0 {
            return -1;
        }
        let mut mid = hi;
        loop {
            mid /= 2;
            let key = self.field(mid, 0);
            let c = cmp(key, word, key.len());
            if c == 0 {
                if hit(mid) {
                    return mid;
                }
                lo = mid + 1;
            } else if c < 0 {
                lo = mid + 1;
            } else {
                hi = mid - 1;
            }
            mid = hi + lo;
            if hi < lo {
                return -1;
            }
        }
    }
}

/// A `char*` table the engine indexes directly (no count variable).
#[derive(Clone, Copy)]
pub struct PtabN {
    pub va: u32,
    pub n: i32,
}

pub const fn ptabn(va: u32, n: i32) -> PtabN {
    PtabN { va, n }
}

impl PtabN {
    /// Entry `i`; out-of-range indices and NULL entries read as "".
    pub fn get(&self, i: i32) -> &'static [u8] {
        if i < 0 || i >= self.n {
            return b"";
        }
        let p = rd_i32(self.va + 4 * i as u32) as u32;
        if p == 0 {
            b""
        } else {
            s(p)
        }
    }
}
