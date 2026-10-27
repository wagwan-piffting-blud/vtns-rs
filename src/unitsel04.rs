//! Unit selection of the 2004 Paul engine (`vt_eng.dll`, PE 2005-03-31), ported from its decompile
//! and disassembly. Older design than the 2009/2014 selectors: 5-byte unit labels with decimal-coded
//! position bytes, 6-byte class keys, one exact class per lookup variant (whole phones) or a prefix
//! back-off over a half-key ordering (half phones), a flat Viterbi with a mean-based beam prune.
//!
//! Function map (2004 VA -> role): `us_SelectUnitsForPhrase` 0x1001c100, `FUN_1001bcd0`
//! whole-phone lookup, `FUN_1001bdf0` half-phone lookups, `FUN_1001b950` context lattice,
//! `FUN_1001b8c0` / `FUN_1001b780` / `FUN_1001b670` class lookups and ranking, `FUN_1001afc0`
//! candidate generation, `FUN_1001ad10` contiguity, `FUN_1001a530` target cost, `FUN_1001aa20`
//! join penalty, `FUN_1001bfb0` beam prune, `FUN_100136a0` / `FUN_10013af0` introsorts,
//! `FUN_10012810` class build, `FUN_10021c70` half-key orderings.

use crate::bank::Bank;
use crate::unitsel::{CepDist, ScalarDist};

#[path = "unitsel_tables_paul04.rs"]
pub mod tab;

pub const BEAM: usize = 30;
const WB: usize = 4;
const POOL: usize = 10000;

const ZERO_POINT_ONE: f64 = f32::from_bits(0x3dcccccd) as f64;
const ZERO_POINT_TWO: f64 = f32::from_bits(0x3e4ccccd) as f64;

fn tb(addr: u32, i: u8) -> u8 {
    tab::DATA.get((addr - tab::DATA_LO) as usize + i as usize).copied().unwrap_or(0)
}

fn ti(addr: u32, i: usize) -> i32 {
    let o = (addr - tab::DATA_LO) as usize + 4 * i;
    match tab::DATA.get(o..o + 4) {
        Some(b) => i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        None => 0,
    }
}

fn tf(addr: u32, i: usize) -> f32 {
    f32::from_bits(ti(addr, i) as u32)
}

/// `FUN_10019060`: 6-byte class key of a 5-byte unit/target label.
pub fn key6(l: &[u8; 5]) -> [u8; 6] {
    let mut k = [tb(tab::T_C24, l[0]), tb(tab::T_BC4, l[1]), tb(tab::T_C84, l[2]), tb(tab::T_EC4, l[1]), l[3], l[4]];
    if l[3] % 10 == 0 {
        let c = tb(tab::T_B04, l[1]);
        if c == 0x14 || c == 0x1b || c == 0x1f || c == 9 {
            k[3] = (tb(tab::T_EC4, l[2]) != 0) as u8;
        }
    }
    k
}

/// `FUN_10019100`: 9-byte half-phone key of a class key; `mode` 1 = left half, 2 = right half.
pub fn half9(k: &[u8; 6], mode: u8) -> [u8; 9] {
    if mode == 1 {
        [0, k[1], tb(tab::T_CE4, k[0]), k[3], k[0], k[4] / 10, k[5] / 10, k[5] % 10, tb(tab::T_DA4, k[2])]
    } else {
        [1, k[1], tb(tab::T_D44, k[2]), k[3], k[2], k[4] % 10, k[5] % 10, k[5] / 10, tb(tab::T_DA4, k[0])]
    }
}

/// Introsort of `FUN_100136a0` / `FUN_10013af0` (same algorithm, `gt(a, b)` is "a sorts after b").
pub fn introsort<T: Copy>(arr: &mut [T], gt: &dyn Fn(&T, &T) -> bool) {
    let n = arr.len() as isize;
    if n < 2 {
        return;
    }
    let mut stack_lo: Vec<isize> = Vec::new();
    let mut stack_hi: Vec<isize> = Vec::new();
    let mut lo: isize = 0;
    let mut hi: isize = n - 1;
    loop {
        while hi - lo > 0x10 {
            let a = arr[lo as usize];
            let mid = (hi + lo) >> 1;
            if gt(&a, &arr[hi as usize]) {
                arr[lo as usize] = arr[hi as usize];
                arr[hi as usize] = a;
            }
            let b = arr[mid as usize];
            if gt(&arr[lo as usize], &b) {
                arr[mid as usize] = arr[lo as usize];
                arr[lo as usize] = b;
            }
            let c = arr[hi as usize];
            if gt(&arr[mid as usize], &c) {
                arr[hi as usize] = arr[mid as usize];
                arr[mid as usize] = c;
            }
            let pivot = arr[mid as usize];
            let mut i = lo;
            let mut j = hi;
            let jp;
            loop {
                loop {
                    i += 1;
                    if !gt(&pivot, &arr[i as usize]) {
                        break;
                    }
                }
                let mut last;
                loop {
                    last = j;
                    j -= 1;
                    if !gt(&arr[j as usize], &pivot) {
                        break;
                    }
                }
                if j <= i {
                    jp = last;
                    break;
                }
                arr.swap(i as usize, j as usize);
            }
            let right = hi - j;
            let left = (j - lo) + 1;
            if left < right {
                if left < (right >> 5) {
                    heapsort(arr, jp as usize, right as usize, gt);
                    hi = j;
                } else {
                    stack_lo.push(jp);
                    stack_hi.push(hi);
                    hi = j;
                }
            } else if right < (left >> 5) {
                heapsort(arr, lo as usize, left as usize, gt);
                lo = jp;
            } else {
                stack_lo.push(lo);
                stack_hi.push(j);
                lo = jp;
            }
        }
        let mut tmp = arr[lo as usize];
        let mut k = lo + 1;
        while k <= hi {
            let cur = arr[k as usize];
            let mut keep = cur;
            if gt(&tmp, &cur) {
                arr[k as usize] = tmp;
                let mut p = k;
                let mut idx = k;
                loop {
                    idx -= 1;
                    if !(lo < idx) {
                        break;
                    }
                    let prev = arr[(p - 2) as usize];
                    if !gt(&prev, &cur) {
                        break;
                    }
                    arr[(p - 1) as usize] = prev;
                    p -= 1;
                }
                arr[idx as usize] = cur;
                keep = tmp;
            }
            tmp = keep;
            k += 1;
        }
        match (stack_lo.pop(), stack_hi.pop()) {
            (Some(l), Some(h)) => {
                lo = l;
                hi = h;
            }
            _ => return,
        }
    }
}

/// `FUN_100138f0` / `FUN_100134c0`: heapsort of `arr[off .. off + n]`.
fn heapsort<T: Copy>(arr: &mut [T], off: usize, n: usize, gt: &dyn Fn(&T, &T) -> bool) {
    if n <= 1 {
        return;
    }
    let at = |i: usize| off + i - 1;
    let half = n >> 1;
    if half > 0 {
        let mut p = off + half - 1;
        let mut i5 = half;
        loop {
            let mut i9 = 0usize;
            let mut i8 = i5;
            while i8 <= half {
                i8 *= 2;
                i9 += 1;
                if i8 < n && gt(&arr[off + i8], &arr[at(i8)]) {
                    i8 += 1;
                }
            }
            let v = arr[p];
            while i5 != i8 && gt(&v, &arr[at(i8)]) {
                i8 >>= 1;
                i9 -= 1;
            }
            let mut s = i9;
            while s > 0 {
                arr[at(i8 >> s)] = arr[at(i8 >> (s - 1))];
                s -= 1;
            }
            i5 -= 1;
            arr[at(i8)] = v;
            if i5 == 0 {
                break;
            }
            p -= 1;
        }
    }
    let mut m = n - 1;
    let mut pe = off + m;
    loop {
        let v = arr[pe];
        arr[pe] = arr[off];
        arr[off] = v;
        let mut i5 = 1usize;
        let mut i8 = 0usize;
        if (m >> 1) > 0 {
            loop {
                i5 *= 2;
                i8 += 1;
                if i5 < m && gt(&arr[off + i5], &arr[at(i5)]) {
                    i5 += 1;
                }
                if i5 > (m >> 1) {
                    break;
                }
            }
            while i5 != 1 && gt(&v, &arr[at(i5)]) {
                i5 >>= 1;
                i8 -= 1;
            }
            let mut s = i8;
            while s > 0 {
                arr[at(i5 >> s)] = arr[at(i5 >> (s - 1))];
                s -= 1;
            }
        }
        arr[at(i5)] = v;
        m -= 1;
        if m == 0 {
            break;
        }
        pe -= 1;
    }
}

/// Numerical Recipes `sort2` heapsort (`FUN_10021b30`) carrying `rb` along.
fn nr_heapsort2<F: Fn(usize, usize) -> i32>(ra: &mut [usize], rb: &mut [u32], cmp: F) {
    let n = ra.len();
    if n < 2 {
        return;
    }
    let mut l = (n >> 1) + 1;
    let mut ir = n;
    loop {
        let (rra, rrb);
        if l > 1 {
            l -= 1;
            rra = ra[l - 1];
            rrb = rb[l - 1];
        } else {
            rra = ra[ir - 1];
            rrb = rb[ir - 1];
            ra[ir - 1] = ra[0];
            rb[ir - 1] = rb[0];
            ir -= 1;
            if ir == 1 {
                ra[0] = rra;
                rb[0] = rrb;
                return;
            }
        }
        let mut i = l;
        let mut j = l + l;
        while j <= ir {
            if j < ir && cmp(ra[j - 1], ra[j]) < 0 {
                j += 1;
            }
            if cmp(rra, ra[j - 1]) < 0 {
                ra[i - 1] = ra[j - 1];
                rb[i - 1] = rb[j - 1];
                i = j;
                j <<= 1;
            } else {
                j = ir + 1;
            }
        }
        ra[i - 1] = rra;
        rb[i - 1] = rrb;
    }
}

/// Class index of the 2004 unit database (`FUN_10012810`, `FUN_10012ae0`, `FUN_10021c70`).
pub struct Cls04 {
    pub key: Vec<[u8; 6]>,
    /// Units of each class, ascending.
    pub units: Vec<Vec<u32>>,
    pub class_of: Vec<i32>,
    /// Class indices ordered by the left (0) / right (1) half-phone keys, and the keys in that order.
    pub half_order: [Vec<u32>; 2],
    pub half_key: [Vec<[u8; 9]>; 2],
}

impl Cls04 {
    pub fn build(bank: &Bank) -> Cls04 {
        let n = bank.len();
        let keys: Vec<[u8; 6]> = bank.label5.iter().map(key6).collect();
        let mut order: Vec<u32> = (0..n as u32).collect();
        order.sort_by(|&a, &b| keys[a as usize].cmp(&keys[b as usize]).then(a.cmp(&b)));
        let mut cls = Cls04 {
            key: Vec::new(),
            units: Vec::new(),
            class_of: vec![0; n],
            half_order: [Vec::new(), Vec::new()],
            half_key: [Vec::new(), Vec::new()],
        };
        for &u in &order {
            let k = keys[u as usize];
            if cls.key.last() != Some(&k) {
                cls.key.push(k);
                cls.units.push(Vec::new());
            }
            cls.units.last_mut().unwrap().push(u);
            cls.class_of[u as usize] = cls.key.len() as i32 - 1;
        }
        for m in 0..2 {
            let hk: Vec<[u8; 9]> = cls.key.iter().map(|k| half9(k, m as u8 + 1)).collect();
            let mut idx: Vec<u32> = (0..cls.key.len() as u32).collect();
            let mut ra: Vec<usize> = (0..hk.len()).collect();
            nr_heapsort2(&mut ra, &mut idx, |a, b| match hk[a].cmp(&hk[b]) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            });
            cls.half_key[m] = ra.iter().map(|&i| hk[i]).collect();
            cls.half_order[m] = idx;
        }
        cls
    }

    fn exact(&self, k: &[u8; 6]) -> i32 {
        match self.key.binary_search(k) {
            Ok(i) => i as i32,
            Err(_) => -1,
        }
    }
}

/// A word record as the selector reads it: the 0x300 bytes starting at `SS + 0x648 + w * 0x2fc`
/// (so the previous word's trailing flag byte is `raw[0]`), plus its syllable records.
#[derive(Clone)]
pub struct Word04 {
    raw: Vec<u8>,
    syl_nph: Vec<i16>,
    syl_f0: Vec<[u8; 12]>,
    pub ntargets: i16,
    first_target: usize,
}

impl Word04 {
    pub fn from_raw(raw: &[u8], syls: &[Vec<u8>]) -> Word04 {
        let syl_nph = syls.iter().map(|s| i16::from_le_bytes([s[0x24], s[0x25]])).collect();
        let syl_f0 = syls
            .iter()
            .map(|s| {
                let mut f = [0u8; 12];
                for (i, v) in f.iter_mut().enumerate() {
                    *v = s[4 + 2 * i];
                }
                f
            })
            .collect();
        Word04 { raw: raw.to_vec(), syl_nph, syl_f0, ntargets: 0, first_target: 0 }
    }

    fn s16(&self, o: usize) -> i16 {
        i16::from_le_bytes([self.raw[o], self.raw[o + 1]])
    }

    fn n_syl(&self) -> i16 {
        self.s16(WB + 0xd0)
    }

    fn n_phones(&self) -> i16 {
        self.s16(WB + 0xd2)
    }

    fn dur(&self, k: usize) -> i16 {
        self.s16(WB + 0xee + 2 * k)
    }

    fn label(&self, k: usize) -> [u8; 5] {
        let o = WB + 0x170 + 5 * k;
        [self.raw[o], self.raw[o + 1], self.raw[o + 2], self.raw[o + 3], self.raw[o + 4]]
    }

    fn edge(&self, k: usize) -> u8 {
        self.raw[WB + 0x2b5 + k]
    }

    fn flag_prev(&self) -> u8 {
        self.raw[0]
    }

    fn flag_own(&self) -> u8 {
        self.raw[WB + 0x2f8]
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Tg {
    pub word: u8,
    pub syl: u8,
    pub phone: u8,
    pub idx: u8,
    pub half: u8,
    pub unvoiced: u8,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Rec {
    pub tc: f32,
    pub score: f32,
    pub id: i32,
    pub back: u8,
    pub fwd: u8,
    pub run: u8,
}

#[derive(Clone, Default)]
pub struct Slot {
    pub classes: Vec<i32>,
    pub kept: [i32; BEAM],
    pub n: i16,
}

#[derive(Default)]
pub struct PhraseResult {
    pub targets: Vec<Tg>,
    pub feats: Vec<[u8; 3]>,
    pub classes: Vec<Vec<i32>>,
    pub n_expanded: Vec<usize>,
    /// Pool records `0..n` right after candidate generation, with the kept unit id of each.
    pub cands: Vec<Vec<(Rec, i32)>>,
    pub expanded: Vec<Vec<(i32, u8, u8, u8)>>,
    /// Final beam per target: kept units, accumulated costs, back pointers.
    pub kept: Vec<Vec<i32>>,
    pub acc: Vec<Vec<f32>>,
    pub back: Vec<Vec<i16>>,
    pub path: Vec<i16>,
    pub uids: Vec<i32>,
    pub db: Vec<[u8; 20]>,
}

pub struct Selector04<'a> {
    bank: &'a Bank,
    cls: &'a Cls04,
    cep: &'a CepDist,
    sd: &'a ScalarDist,
    tg: Vec<Tg>,
    slots: Vec<Slot>,
    list: Vec<i32>,
    pool: Vec<Rec>,
    n_targets: usize,
    acc: Vec<[f32; BEAM]>,
    back: Vec<[i16; BEAM]>,
    expanded: Vec<Vec<(i32, u8, u8, u8)>>,
    n_expanded: Vec<usize>,
    cands: Vec<Vec<(Rec, i32)>>,
    feats: Vec<[u8; 3]>,
    classes_dbg: Vec<Vec<i32>>,
    domain_of_file: Vec<u8>,
}

impl<'a> Selector04<'a> {
    pub fn new(bank: &'a Bank, cls: &'a Cls04, cep: &'a CepDist, sd: &'a ScalarDist) -> Selector04<'a> {
        let mut domain_of_file = Vec::new();
        for p in &bank.pools {
            domain_of_file.push(p.domain);
        }
        Selector04 {
            bank,
            cls,
            cep,
            sd,
            tg: Vec::new(),
            slots: Vec::new(),
            list: Vec::new(),
            pool: vec![Rec::default(); POOL],
            n_targets: 0,
            acc: Vec::new(),
            back: Vec::new(),
            expanded: Vec::new(),
            n_expanded: Vec::new(),
            cands: Vec::new(),
            feats: Vec::new(),
            classes_dbg: Vec::new(),
            domain_of_file,
        }
    }

    fn file_of(&self, uid: i32) -> usize {
        let u = uid as usize;
        self.bank.file_col[u] as usize + self.bank.file_idx[u] as usize
    }

    fn domain(&self, uid: i32) -> u8 {
        self.domain_of_file.get(self.file_of(uid)).copied().unwrap_or(0)
    }

    fn ensure(&mut self, t: usize) {
        while self.tg.len() <= t {
            self.tg.push(Tg::default());
        }
        while self.slots.len() <= t {
            self.slots.push(Slot::default());
        }
    }

    // ---- class lookups -------------------------------------------------------------------------

    fn half_range(&self, l: &[u8; 5], half: u8, plen: i32) -> Option<(usize, usize)> {
        let m = (half == 2) as usize;
        let target = half9(&key6(l), half);
        let hk = &self.cls.half_key[m];
        let p = plen.max(0) as usize;
        let start = hk.partition_point(|k| k[..p] < target[..p]);
        let end = hk.partition_point(|k| k[..p] <= target[..p]);
        if end > start {
            Some((start, end - start))
        } else {
            None
        }
    }

    /// `FUN_1001b510`: distance between a label's class key and a class key, for half targets.
    fn key_dist(&self, k: &[u8; 6], ck: &[u8; 6], half: u8) -> i32 {
        let h1 = half9(k, half);
        let h2 = half9(ck, half);
        let mut i = 0;
        while i < 9 && h1[i] == h2[i] {
            i += 1;
        }
        let mut v = (9 - i as i32) * 2000;
        if half == 1 {
            if tb(tab::T_C84, k[2]) != tb(tab::T_C84, ck[2]) {
                v += 1000;
            }
        } else if half == 2 && tb(tab::T_C24, k[1]) != tb(tab::T_C24, ck[1]) {
            v += 1000;
        }
        v += ti(tab::T_0A0, (ck[5] % 10) as usize + (k[5] % 10) as usize * 6);
        v += ti(tab::T_060, (ck[5] / 10) as usize + (k[5] / 10) as usize * 4);
        for i in 0..6 {
            if k[i] != ck[i] {
                v += 1;
            }
        }
        v
    }

    /// `FUN_1001b670`: at most `maxc` classes of `classes`, closest first, else the list itself.
    fn rank_limit(&self, maxc: usize, l: &[u8; 5], half: u8, classes: &[i32]) -> Vec<i32> {
        if classes.len() <= maxc {
            return classes.to_vec();
        }
        let k = key6(l);
        let mut recs: Vec<(f32, i32)> = Vec::new();
        for &c in classes {
            let d = self.key_dist(&k, &self.cls.key[c as usize], half);
            recs.push((d as f32, c));
            if recs.len() > 9999 {
                break;
            }
        }
        introsort(&mut recs, &|a, b| b.0 < a.0);
        recs.truncate(maxc);
        recs.iter().map(|r| r.1).collect()
    }

    fn units_of(&self, classes: &[i32]) -> i32 {
        classes.iter().map(|&c| self.cls.units[c as usize].len() as i32).sum()
    }

    /// `FUN_1001b780`: classes appended for one half-phone lookup of label `l`.
    fn half_lookup(&self, l: &[u8; 5], half: u8, fl: bool) -> Vec<i32> {
        let order = &self.cls.half_order[(half == 2) as usize];
        let slice = |r: Option<(usize, usize)>| -> Vec<i32> {
            match r {
                Some((s, c)) => order[s..s + c].iter().map(|&x| x as i32).collect(),
                None => Vec::new(),
            }
        };
        let mut plen = 9;
        let mut found = self.half_range(l, half, plen);
        if !fl {
            return match found {
                None => Vec::new(),
                Some(_) => self.rank_limit(10, l, half, &slice(found)),
            };
        }
        while found.is_none() {
            plen -= 1;
            found = self.half_range(l, half, plen);
        }
        let mut out = self.rank_limit(10, l, half, &slice(found));
        let mut units = self.units_of(&out);
        while units < 3 && plen > 4 && (found.is_none() || l[3] % 10 != 5) {
            plen -= 1;
            found = self.half_range(l, half, plen);
            out = self.rank_limit(10, l, half, &slice(found));
            units = self.units_of(&out);
        }
        out
    }

    /// `FUN_1001b8c0`.
    fn add_lookup(&mut self, l: &[u8; 5], half: u8, fl: bool) -> usize {
        let cnt = self.list.len();
        if half != 0 {
            if cnt > 0xbd {
                return 0;
            }
            let v = self.half_lookup(l, half, fl);
            let n = v.len();
            self.list.extend(v);
            n
        } else {
            if cnt > 0x13 {
                return 0;
            }
            let i = self.cls.exact(&key6(l));
            if i >= 0 {
                self.list.push(i);
                1
            } else {
                0
            }
        }
    }

    /// `FUN_1001b950`: context lattice over position codes; appends class indices to `self.list`.
    fn lattice(&mut self, p: [u8; 5], edge: u8, half: u8, rec: bool) {
        let a3 = p[3] / 10;
        let b3 = p[3] % 10;
        let a4 = p[4] / 10;
        let b4 = p[4] % 10;
        let one_two = [1u8, 2u8];
        let ln = if edge / 10 == 0 && (a3 == 1 || a3 == 2) { 2 } else { 1 };
        let rn = if edge % 10 == 0 && (b3 == 1 || b3 == 2) { 2 } else { 1 };
        for li in 0..ln {
            let mut v8 = a4;
            if ln > 1 && (a4 == 1 || a4 == 2) {
                v8 = one_two[li];
            }
            let mut v4 = a3;
            if a3 != 0 && a3 == a4 {
                v4 = v8;
            }
            for ri in 0..rn {
                let mut v2 = b4;
                if rn > 1 && (b4 == 1 || b4 == 2) {
                    v2 = one_two[ri];
                }
                let mut v13 = b3;
                if b3 != 0 && b3 == b4 {
                    v13 = v2;
                }
                let mut tmp = p;
                tmp[4] = v8.wrapping_mul(10).wrapping_add(v2);
                tmp[3] = v4.wrapping_mul(10).wrapping_add(v13);
                let u = self.add_lookup(&tmp, half, tmp == p);
                if u != 0 || p[3] != 0 {
                    let x = tb(tab::T_B64, p[1]);
                    if x != 0 {
                        tmp[1] = x;
                        self.add_lookup(&tmp, half, false);
                    } else {
                        let y = tb(tab::T_B64, p[0]);
                        if y != 0 {
                            tmp[0] = y;
                            self.add_lookup(&tmp, half, false);
                        }
                    }
                }
            }
        }
        if !rec {
            if a4 == 3 && a3 == 0 {
                let mut t = p;
                t[4] = b4;
                self.lattice(t, edge, half, true);
            }
            if b4 == 3 {
                let mut t = p;
                if t[3] % 10 == 3 {
                    t[3] = (t[3] / 10).wrapping_mul(10).wrapping_add(4);
                }
                t[4] = (t[4] / 10).wrapping_mul(10).wrapping_add(4);
                self.lattice(t, edge, half, true);
            }
        }
    }

    /// `FUN_1001bcd0`.
    fn lookup_whole(&mut self, n: usize, words: &[Word04]) -> i16 {
        let g = self.tg[n];
        let w = &words[g.word as usize];
        let edge = w.edge(g.phone as usize);
        self.tg[n].idx = w.ntargets as u8;
        self.tg[n].half = 0;
        self.list.clear();
        self.lattice(w.label(g.phone as usize), edge, 0, false);
        let sum = self.units_of(&self.list);
        if sum > 2 {
            let mut c = self.list.clone();
            c.sort_unstable();
            self.slots[n].classes = c;
            return 1;
        }
        0
    }

    /// `FUN_1001bdf0`.
    fn lookup_halves(&mut self, n: usize, words: &[Word04]) -> i16 {
        let g0 = self.tg[n];
        let w = &words[g0.word as usize];
        let edge = w.edge(g0.phone as usize);
        let label = w.label(g0.phone as usize);
        for h in 0..2usize {
            if h == 0 {
                self.tg[n].half = 1;
            } else {
                self.tg[n + 1] = self.tg[n];
                self.tg[n + 1].half = 2;
            }
            self.tg[n + h].idx = (w.ntargets as u8).wrapping_add(h as u8);
            let half = self.tg[n + h].half;
            self.list.clear();
            self.lattice(label, edge, half, false);
            self.list.sort_unstable();
            self.list.dedup();
            let mut out = self.rank_limit(20, &label, half, &self.list.clone());
            out.sort_unstable();
            self.slots[n + h].classes = out;
        }
        2
    }

    fn build_targets(&mut self, words: &mut [Word04], first: usize, nw: usize) -> usize {
        let mut n = 0usize;
        for wi in 0..nw {
            let w = first + wi;
            words[w].ntargets = 0;
            words[w].first_target = n;
            let nph = words[w].n_phones();
            for k in 0..nph.max(0) as usize {
                self.ensure(n + 2);
                let label = words[w].label(k);
                let mut g = Tg { word: w as u8, phone: k as u8, ..Tg::default() };
                g.unvoiced = (tb(tab::T_E64, label[1]) == 0) as u8;
                let mut s = 0usize;
                let mut acc = 0i16;
                let nsyl = words[w].n_syl().max(0) as usize;
                if nsyl > 0 {
                    loop {
                        let np = words[w].syl_nph.get(s).copied().unwrap_or(0);
                        if acc <= k as i16 && (k as i16) < np + acc {
                            break;
                        }
                        acc += np;
                        s += 1;
                        if s >= nsyl {
                            break;
                        }
                    }
                }
                g.syl = s as u8;
                self.tg[n] = g;
                if words[w].dur(k) > 0 {
                    let mut r = self.lookup_whole(n, words);
                    if r < 1 {
                        r = self.lookup_halves(n, words);
                    }
                    words[w].ntargets += r;
                    n += r as usize;
                }
            }
        }
        n
    }

    // ---- candidates ----------------------------------------------------------------------------

    fn dur_code(dur: i16) -> u8 {
        let v = ((dur as f32) as f64 / 50.0 + 0.5) as i32 as i16;
        if v < 0 {
            0
        } else if v >= 0x100 {
            0xff
        } else {
            v as u8
        }
    }

    fn feature(&self, t: usize, words: &[Word04]) -> [u8; 3] {
        let g = self.tg[t];
        let w = &words[g.word as usize];
        let dur = w.dur(g.phone as usize);
        let dc = Self::dur_code(dur);
        let nt = w.ntargets.max(0) as usize;
        let ft = w.first_target;
        let mut a = 0usize;
        while a < nt {
            if g.syl == self.tg[ft + a].syl {
                break;
            }
            a += 1;
        }
        let (mut before, mut upto, mut total) = (0i32, 0i32, 0i32);
        let (mut i1, mut i2) = (0usize, 0usize);
        if a < nt {
            let v = if g.half == 0 { dur as i32 } else { dur as i32 / 2 };
            let mut k = a;
            while k < nt {
                let q = self.tg[ft + k];
                if q.syl != g.syl {
                    break;
                }
                if q.unvoiced == 0 {
                    if (k as i32) < g.idx as i32 {
                        before += v;
                    } else if k as i32 == g.idx as i32 {
                        upto = before + v;
                    }
                    total += v;
                }
                k += 1;
            }
            if total != 0 {
                let mut a1 = before * 12 / total;
                let mut a2 = upto * 12 / total - 1;
                if a1 >= 12 {
                    a1 = 11;
                }
                if a2 < a1 {
                    a2 = a1;
                }
                i1 = a1 as usize;
                i2 = a2 as usize;
            }
        }
        let f0 = &w.syl_f0[(g.syl as usize).min(w.syl_f0.len().saturating_sub(1))];
        [dc, f0[i1.min(11)], f0[i2.min(11)]]
    }

    /// `FUN_1001ad10`.
    fn contiguity(&mut self, t: usize, n: usize) {
        let flag = &self.bank.flag;
        let cof = &self.cls.class_of;
        for i in 0..n {
            let uid = self.pool[i].id as usize;
            let mut back = 0u8;
            let mut u = uid;
            let mut cl = cof[uid];
            let mut k = t as isize - 1;
            while k >= 0 {
                if self.tg[k as usize].half != 1 {
                    if u == 0 || flag[u - 1] == 0 {
                        break;
                    }
                    u -= 1;
                    cl = cof[u];
                }
                if cl < 0 || !self.slots[k as usize].classes.contains(&cl) {
                    break;
                }
                back += 1;
                k -= 1;
            }
            let mut fwd = 0u8;
            let mut u = uid;
            let mut cl = cof[uid];
            let mut k = t + 1;
            while k < self.n_targets {
                if self.tg[k].half != 2 {
                    if flag.get(u).copied().unwrap_or(0) == 0 {
                        break;
                    }
                    u += 1;
                    cl = cof.get(u).copied().unwrap_or(-1);
                }
                if cl < 0 || !self.slots[k].classes.contains(&cl) {
                    break;
                }
                fwd += 1;
                k += 1;
            }
            let r = &mut self.pool[i];
            r.back = back;
            r.fwd = fwd;
            r.run = back.wrapping_add(fwd).wrapping_add(1);
        }
    }

    fn run_score(r: &Rec) -> f32 {
        let run = r.run as i32;
        if r.back != 0 && r.fwd != 0 {
            (-(run * 2 + 1)) as f32
        } else {
            (run * -2) as f32
        }
    }

    /// `FUN_1001a530`.
    fn target_cost(&self, uid: i32, t: usize, feat: &[u8; 3], w: f32, words: &[Word04]) -> f32 {
        let g = self.tg[t];
        let word = &words[g.word as usize];
        let tl = word.label(g.phone as usize);
        let ul = self.bank.label5[uid as usize];
        let mut pen: i32 = 0;
        let (tc, uc) = (tl[1], ul[1]);
        if uc != tc {
            if tc == 0x4b || uc == 0x4b {
                pen = 100;
            } else {
                pen = (if tb(tab::T_B04, uc) != tb(tab::T_B04, tc) { 0x3e3 } else { 0 }) + 5;
            }
        }
        if ul[0] != tl[0] && g.half != 2 {
            if tb(tab::T_C24, ul[0]) == tb(tab::T_C24, tl[0]) {
                pen += 2;
            } else if tb(tab::T_B04, ul[0]) == tb(tab::T_B04, tl[0]) {
                pen += 5;
            } else {
                pen += 100;
            }
        }
        if ul[2] != tl[2] && g.half != 1 {
            if tl[2] == 0x5a || ul[2] == 0x4b {
                pen += 100;
            } else if tb(tab::T_C84, ul[2]) == tb(tab::T_C84, tl[2]) {
                pen += 2;
            } else if tb(tab::T_B04, ul[2]) == tb(tab::T_B04, tl[2]) {
                pen += 5;
            } else {
                pen += 100;
            }
        }
        let (t3h, t3l) = ((tl[3] / 10) as i32, (tl[3] % 10) as i32);
        let (u3h, u3l) = ((ul[3] / 10) as i32, (ul[3] % 10) as i32);
        if t3h < 4 && u3h < 4 && (g.word == 0 || word.flag_prev() != 1 || t3h == 0 || t3h == u3h) {
            pen += ti(tab::T_060, (u3h + t3h * 4) as usize);
        } else {
            pen += 1000;
        }
        if t3l < 6 && u3l < 6 && (word.flag_own() != 1 || t3l == 0 || t3l == u3l) {
            pen += ti(tab::T_0A0, (u3l + t3l * 6) as usize);
        } else {
            pen += 1000;
        }
        let (t4h, t4l) = ((tl[4] / 10) as i32, (tl[4] % 10) as i32);
        let (u4h, u4l) = ((ul[4] / 10) as i32, (ul[4] % 10) as i32);
        if t4h < 4 && u4h < 4 {
            pen += ti(tab::T_060, (u4h + t4h * 4) as usize);
        } else {
            pen += 1000;
        }
        if t4l < 6 && u4l < 6 {
            pen += ti(tab::T_0A0, (u4l + t4l * 6) as usize);
        } else {
            pen += 1000;
        }
        let st_t = tb(tab::T_EC4, tc) as usize;
        let st_u = tb(tab::T_EC4, uc) as usize;
        if st_t != st_u {
            pen += ti(tab::T_130, st_t * 3 + st_u);
        } else if tb(tab::T_E04, tl[0]) > 0 && tc == 0x36 {
            pen += ti(tab::T_130, tb(tab::T_EC4, tl[0]) as usize * 3 + tb(tab::T_EC4, ul[0]) as usize);
        } else if tb(tab::T_E04, tl[2]) > 0 {
            let a = tb(tab::T_EC4, tl[2]) as usize * 3 + tb(tab::T_EC4, ul[2]) as usize;
            if tc == 0x42 || tc == 0x43 {
                pen += ti(tab::T_130, a);
            } else {
                pen += ti(tab::T_154, a);
            }
        }
        if self.domain(uid) == 100 {
            pen += 10;
        }
        let u = uid as usize;
        let dd = self.sd.get(feat[0], self.bank.dur_code[u]);
        let cont: f32;
        if g.unvoiced == 0 {
            let (ka, kb) = if g.half == 1 {
                (0usize, 1usize)
            } else if g.half == 2 {
                (1, 2)
            } else {
                (0, 2)
            };
            let a = self.sd.get(feat[1], self.bank.bounds[u][ka].feat_f);
            let b = self.sd.get(feat[2], self.bank.bounds[u][kb].feat_f);
            let s = (b as f64 + a as f64) as f32;
            cont = (s as f64 + dd as f64) as f32;
        } else {
            if ul[2] == tl[2] {
                pen /= 2;
            }
            cont = (dd as f64 * ZERO_POINT_TWO) as f32;
        }
        ((cont as f64) * (w as f64) + (pen as f32) as f64) as f32
    }

    /// `FUN_1001afc0`.
    fn get_candidates(&mut self, t: usize, words: &[Word04]) {
        let feat = self.feature(t, words);
        self.feats[t] = feat;
        let mut n = 0usize;
        let cl = self.slots[t].classes.clone();
        'outer: for &c in &cl {
            let us = &self.cls.units[c as usize];
            for &u in us {
                self.pool[n].id = u as i32;
                n += 1;
                if n >= POOL {
                    break 'outer;
                }
            }
        }
        self.contiguity(t, n);
        self.n_expanded[t] = n;
        self.expanded[t] = self.pool[..n.min(48)].iter().map(|r| (r.id, r.back, r.fwd, r.run)).collect();
        for i in 0..n {
            self.pool[i].score = Self::run_score(&self.pool[i]);
        }
        introsort(&mut self.pool[..n], &|a, b| b.score < a.score);
        let nt = self.n_targets as i32;
        let mut count = n;
        let mut sort_from = 0usize;
        if n > 0 && self.pool[0].run as i32 == nt {
            let mut i = 0;
            while i < n {
                if (self.pool[i].run as i32) < nt {
                    break;
                }
                i += 1;
            }
            count = i;
        } else if n > 0x1e {
            let mut i = 1;
            while i < n {
                if self.pool[i - 1].score < self.pool[i].score {
                    if i >= 0x1e {
                        break;
                    }
                    sort_from = i;
                }
                i += 1;
            }
            count = i;
        }
        for i in 0..count {
            let r = self.pool[i];
            let m = r.back.min(r.fwd) as f64;
            let w = (1.0 / ((r.run as f32) as f64 / 2.0 + (m as f32) as f64)) as f32;
            let tc = self.target_cost(r.id, t, &feat, w, words);
            self.pool[i].tc = tc;
        }
        let mut keep = count;
        if count > 0x1e {
            for i in sort_from..count {
                let r = self.pool[i];
                self.pool[i].score = if self.domain(r.id) == 100 { (r.tc as f64 * 5.0) as f32 } else { r.tc };
            }
            introsort(&mut self.pool[sort_from..count], &|a, b| b.score < a.score);
            keep = 0x1e;
        }
        let slot = &mut self.slots[t];
        slot.n = keep as i16;
        for i in 0..keep {
            slot.kept[i] = self.pool[i].id;
        }
        self.cands[t] = (0..keep).map(|i| (self.pool[i], self.pool[i].id)).collect();
        self.classes_dbg[t] = self.slots[t].classes.clone();
    }

    /// `FUN_1001aa20`.
    fn join_pen(&self, prev: i32, cur: i32, tp: Tg, tc: Tg, words: &[Word04]) -> i32 {
        let pl = self.bank.label5[prev as usize];
        let cl = self.bank.label5[cur as usize];
        let pw = &words[tp.word as usize];
        let cw = &words[tc.word as usize];
        let p_lab = pw.label(tp.phone as usize);
        let c_lab = cw.label(tc.phone as usize);
        let p_ctr = p_lab[1];
        let c_ctr = c_lab[1];
        let b11 = pl[3] % 10;
        let c_hi = (cl[3] / 10) as usize;
        let p4lo = (pl[4] % 10) as usize;
        let mut pen: i32;
        if tp.word == tc.word && tp.phone == tc.phone {
            pen = 0;
            if b11 < 3 || b11 == cl[4] % 10 {
                if tb(tab::T_E04, p_ctr) != 0 && prev != cur {
                    pen = 2;
                }
            } else {
                pen = 10;
            }
            if pl[3] != cl[3] {
                pen += 2;
            }
            if pl[4] != cl[4] {
                pen += 2;
            }
            if pl[1] != cl[1] {
                pen += 100;
            }
        } else {
            pen = ti(tab::T_FD0, c_hi + b11 as usize * 6);
            if p_lab[4] % 10 != 0 && c_lab[3] / 10 == 0 {
                pen += ti(tab::T_FD0, (cl[4] % 10) as usize + p4lo * 6) / 2;
            }
            let (p1, c0) = (pl[1], cl[0]);
            if p1 != c0 {
                if p1 == 0x4a || c0 == 0x4a || p1 == 0x49 || c0 == 0x49 {
                    pen += 100;
                } else {
                    let c = tb(tab::T_B04, p1);
                    if c == 0x14 || c == 0x1b || c == 0x1f || c == 9 {
                        pen += 10;
                    } else {
                        pen += 1;
                    }
                }
            }
            let (p2, c1) = (pl[2], cl[1]);
            if p2 != c1 {
                if p2 == 0x4a || c1 == 0x4a || p2 == 0x49 || c1 == 0x49 {
                    pen += 10;
                } else {
                    pen += 1;
                }
            }
            if c_lab[3] / 10 == 0
                && tb(tab::T_E04, p_ctr) == 1
                && (c_ctr == 0x36 || c_ctr == 0x2b)
                && prev + 1 != cur
            {
                pen += 1;
            }
        }
        pen
    }

    /// `FUN_1001bfb0` with k = 1.
    fn beam_prune(&mut self, t: usize) {
        let n = self.slots[t].n as i32;
        if n <= 5 {
            return;
        }
        let acc = &mut self.acc[t];
        let mut min = acc[0];
        let mut sum = acc[0];
        for i in 1..n as usize {
            sum = (sum as f64 + acc[i] as f64) as f32;
            if acc[i] < min {
                min = acc[i];
            }
        }
        let mean = (sum as f64 / (n as f32) as f64) as f32;
        let spread = ((mean as f64 - min as f64) / 4.0) as f32;
        let thr = ((1.0f32 as f64) * spread as f64 + mean as f64) as f32;
        let mut w = 0usize;
        for i in 0..n as usize {
            if acc[i] <= thr {
                acc[w] = acc[i];
                self.back[t][w] = self.back[t][i];
                self.slots[t].kept[w] = self.slots[t].kept[i];
                w += 1;
            }
        }
        self.slots[t].n = if w > 5 { w as i16 } else { 5 };
    }

    /// Port of `us_SelectUnitsForPhrase`; `first`/`nw` are the phrase's first word and word count.
    pub fn select_phrase(&mut self, words: &mut [Word04], first: usize, nw: usize) -> PhraseResult {
        self.tg.clear();
        self.slots.clear();
        let n = self.build_targets(words, first, nw);
        let mut res = PhraseResult::default();
        self.n_targets = n;
        if n == 0 {
            return res;
        }
        self.tg.truncate(n);
        self.slots.truncate(n);
        self.acc = vec![[0.0; BEAM]; n];
        self.back = vec![[0; BEAM]; n];
        self.expanded = vec![Vec::new(); n];
        self.n_expanded = vec![0; n];
        self.cands = vec![Vec::new(); n];
        self.feats = vec![[0; 3]; n];
        self.classes_dbg = vec![Vec::new(); n];
        self.get_candidates(0, words);
        for j in 0..self.slots[0].n as usize {
            self.acc[0][j] = (self.pool[j].tc as f64 * ZERO_POINT_ONE) as f32;
        }
        for t in 1..n {
            self.get_candidates(t, words);
            let cur = self.tg[t];
            let prv = self.tg[t - 1];
            let ty = if cur.unvoiced == 0 && prv.unvoiced == 0 { 0 } else { 1 };
            let (pi, ci) = if cur.half == 2 { (1usize, 1usize) } else { (2, 0) };
            let nj = self.slots[t].n as usize;
            let np = self.slots[t - 1].n as usize;
            for j in 0..nj {
                let cu = self.slots[t].kept[j];
                let tcj = (self.pool[j].tc as f64 * ZERO_POINT_ONE) as f32;
                let mut scratch = [0f32; BEAM];
                for p in 0..np {
                    let pu = self.slots[t - 1].kept[p];
                    let join: f32 = if (cu == pu && cur.half == 2) || (cu == pu + 1 && self.bank.flag[pu as usize] == 1) {
                        0.0
                    } else {
                        let (pb, cb) = (&self.bank.bounds[pu as usize][pi], &self.bank.bounds[cu as usize][ci]);
                        let cep = self.cep.get(pb.cep, cb.cep);
                        let mut ff = self.sd.get(pb.feat_f, cb.feat_f);
                        let fb = self.sd.get(pb.feat_b, cb.feat_b);
                        let both8 = self.domain(cu) == 8 && self.domain(pu) == 8;
                        if both8 {
                            ff = (ff as f64 / 2.0) as f32;
                        }
                        let wf = tf(tab::T_178, ty * 2);
                        let wb = tf(tab::T_178, ty * 2 + 1);
                        let s = fb as f64 * wb as f64 + ff as f64 * wf as f64;
                        ((s + cep as f64) + ZERO_POINT_TWO) as f32
                    };
                    let pen = self.join_pen(pu, cu, prv, cur, words);
                    let x = (pen as f64 + join as f64) as f32;
                    scratch[p] = ((x as f64 + self.acc[t - 1][p] as f64) + tcj as f64) as f32;
                }
                let mut best = 0usize;
                for p in 1..np {
                    if scratch[p] < scratch[best] {
                        best = p;
                    }
                }
                self.back[t][j] = best as i16;
                self.acc[t][j] = scratch[best];
            }
            self.beam_prune(t);
        }
        let last = n - 1;
        let mut path = vec![0i16; n];
        let nl = self.slots[last].n as usize;
        let mut best = 0usize;
        for p in 1..nl {
            if self.acc[last][p] < self.acc[last][best] {
                best = p;
            }
        }
        path[last] = best as i16;
        for t in (1..n).rev() {
            path[t - 1] = self.back[t][path[t] as usize];
        }
        res.targets = self.tg.clone();
        res.feats = self.feats.clone();
        res.classes = self.classes_dbg.clone();
        res.n_expanded = self.n_expanded.clone();
        res.cands = self.cands.clone();
        res.expanded = self.expanded.clone();
        for t in 0..n {
            let s = &self.slots[t];
            res.kept.push(s.kept[..s.n as usize].to_vec());
            res.acc.push(self.acc[t][..s.n as usize].to_vec());
            res.back.push(self.back[t][..s.n as usize].to_vec());
            let uid = s.kept[path[t] as usize];
            res.uids.push(uid);
            let mut db = [0u8; 20];
            if let Ok(p) = self.bank.primary(uid as u32) {
                db[0..4].copy_from_slice(&p.pcm_pos.to_le_bytes());
                db[4..8].copy_from_slice(&p.pm_pos.to_le_bytes());
                db[8..10].copy_from_slice(&p.half1_len.to_le_bytes());
                db[10..12].copy_from_slice(&p.half2_len.to_le_bytes());
                db[12..14].copy_from_slice(&p.coded_size.to_le_bytes());
                db[14] = p.pm1;
                db[15] = p.pm2;
                db[16..19].copy_from_slice(&p.periods);
            }
            db[19] = self.file_of(uid) as u8;
            res.db.push(db);
        }
        res.path = path;
        res
    }
}
