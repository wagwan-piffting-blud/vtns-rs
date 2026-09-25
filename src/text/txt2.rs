//! `.txt2` word lists (`fe_LoadTxt2Table` 0x100033e0): every byte stored +10,
//! decoded as `dbf` + CRLF lines (`key` or `key|value`) + NUL + count + `dbf`.
//! Lookups are the engine's bisection `FUN_100035d0`.

use super::cstr::*;

#[derive(Clone, Debug, Default)]
pub struct List {
    /// `'I'`: compare through the tolower table, else strcmp.
    pub insensitive: bool,
    pub keys: Vec<Vec<u8>>,
    pub vals: Vec<Vec<u8>>,
}

impl List {
    /// `mode` is the loader's `param_3`: `b'L'` key-only, `b'M'` key|value.
    pub fn parse(raw: &[u8], mode: u8, insensitive: bool) -> Option<List> {
        let d: Vec<u8> = raw.iter().map(|&x| x.wrapping_sub(10)).collect();
        if d.len() < 6 || &d[..3] != b"dbf" {
            return None;
        }
        let end = d.windows(3).position(|w| w == b"\r\n\0").map(|i| i + 2).unwrap_or(d.len());
        let mut keys = Vec::new();
        let mut vals = Vec::new();
        for line in d[3..end].split(|&c| c == b'\n') {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if line.is_empty() {
                continue;
            }
            if mode == b'M' {
                let (k, v) = match line.iter().position(|&c| c == b'|') {
                    Some(i) => (&line[..i], &line[i + 1..]),
                    None => (line, &line[line.len()..]),
                };
                keys.push(k.to_vec());
                vals.push(v.to_vec());
            } else {
                keys.push(line.to_vec());
                vals.push(line.to_vec());
            }
        }
        Some(List { insensitive, keys, vals })
    }

    pub fn load(path: &std::path::Path, mode: u8) -> std::io::Result<List> {
        let raw = std::fs::read(path)?;
        List::parse(&raw, mode, true).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("bad txt2: {}", path.display())))
    }

    fn cmp(&self, a: &[u8], b: &[u8]) -> i32 {
        if self.insensitive {
            stricmp(a, b)
        } else {
            strcmp(a, b)
        }
    }

    /// `FUN_100035d0`: index of `word` or -1.
    pub fn find(&self, word: &[u8]) -> i32 {
        let n = self.keys.len() as i32;
        if n < 1 {
            return -1;
        }
        let mut hi = n - 1;
        let mut lo = 0;
        let mut mid = hi;
        loop {
            mid /= 2;
            let c = self.cmp(&self.keys[mid as usize], word);
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

    /// `FUN_10052d00`: like `find`, but among case-insensitively equal keys
    /// only accept one whose value starts with `'2'`, or `'1'` with an exact
    /// case match.
    pub fn find_flagged(&self, word: &[u8]) -> i32 {
        let i3 = self.find(word);
        if i3 < 0 {
            return -1;
        }
        let ok = |i: usize| -> bool {
            match self.vals[i].first() {
                Some(b'2') => true,
                Some(b'1') => strcmp(&self.keys[i], word) == 0,
                _ => false,
            }
        };
        if ok(i3 as usize) {
            return i3;
        }
        let mut u = i3 - 1;
        while u >= 0 {
            if stricmp(&self.keys[u as usize], word) != 0 {
                break;
            }
            if ok(u as usize) {
                return i3;
            }
            u -= 1;
        }
        let n = self.keys.len() as i32;
        let mut v = i3 + 1;
        if v < n {
            while stricmp(&self.keys[v as usize], word) == 0 {
                if ok(v as usize) {
                    return i3;
                }
                v += 1;
                if n <= v {
                    return -1;
                }
            }
        }
        -1
    }
}
