//! Phone classes of the unit database, rebuilt at load exactly as vt_eng.dll does
//! when `class.idx` / `classhp.idx` are absent (`us_MakePhoneClass` 0x1001a5a0,
//! `FUN_1002dea0` / `FUN_1002dce0` / `FUN_1002dd70`).
//!
//! A class is the set of units sharing a 5-byte key
//! `{mapL[L1], C, mapR[R1], posbits, flags & 0x20}` (`us_LabelToClassKey`).
//! Classes are numbered in ascending key order and list their units in
//! ascending unit order. Two further orderings of the classes, by the 10-byte
//! left-half and right-half keys, serve the half-phone lookups.

use super::tables::{DATA, DATA_LO};
use super::{tables_julie09, tables_paul09, tables_spa, tables_viol09};
use crate::bank::Bank;
use crate::era::Era;
use crate::lang::Lang;

/// Unit-selection tables of one 2009 build: the byte span copied from its DLL plus the address of
/// every table, read by address as the engine does (`tools/gen_unitsel_tables_09.py`).
#[derive(Clone, Copy)]
pub struct Tab09 {
    lo: u32,
    data: &'static [u8],
    pub collapse: u32,
    pub key_c: u32,
    pub key_l: u32,
    pub key_r: u32,
    pub hk1_cls: u32,
    pub hk2_cls: u32,
    pub vowel: u32,
    pub voiced: u32,
    pub stress: u32,
    pub edge_sub: u32,
    pub ctr_sub: u32,
    pub j_flag: u32,
    pub j_prev: u32,
    pub j_next: u32,
    pub w_key: u32,
    pub w_half: u32,
    pub pos_j: u32,
    pub pos_a: u32,
    pub pos_b: u32,
    pub st_a: u32,
    pub st_b: u32,
    pub tc_div: u32,
    pub join_w: u32,
    /// Violeta (`viol09`) only: class-key maps, and the half-key table of byte 7.
    pub l_nv: u32,
    pub r_nv: u32,
    pub l_v2: u32,
    pub hk_c: u32,
    /// The Violeta build: 8-byte half keys, its own class key, lookups, lattice, target and join cost.
    pub viol: bool,
}

macro_rules! tab09 {
    ($m:ident, $viol:expr) => {{
        use $m::*;
        Tab09 {
            viol: $viol,
            l_nv: A_L_NV,
            r_nv: A_R_NV,
            l_v2: A_L_V2,
            hk_c: A_HK_C,
            lo: DATA_LO,
            data: &DATA,
            collapse: A_COLLAPSE,
            key_c: A_KEY_C,
            key_l: A_KEY_L,
            key_r: A_KEY_R,
            hk1_cls: A_HK1_CLS,
            hk2_cls: A_HK2_CLS,
            vowel: A_VOWEL,
            voiced: A_VOICED,
            stress: A_STRESS,
            edge_sub: A_EDGE_SUB,
            ctr_sub: A_CTR_SUB,
            j_flag: A_J_FLAG,
            j_prev: A_J_PREV,
            j_next: A_J_NEXT,
            w_key: A_W_KEY,
            w_half: A_W_HALF,
            pos_j: A_POS_J,
            pos_a: A_POS_A,
            pos_b: A_POS_B,
            st_a: A_ST_A,
            st_b: A_ST_B,
            tc_div: A_TC_DIV,
            join_w: A_JOIN_W,
        }
    }};
}

impl Tab09 {
    pub fn julie09() -> Tab09 {
        tab09!(tables_julie09, false)
    }

    pub fn paul09() -> Tab09 {
        tab09!(tables_paul09, false)
    }

    pub fn viol09() -> Tab09 {
        tab09!(tables_viol09, true)
    }

    pub fn for_era(era: Era) -> Option<Tab09> {
        match era {
            Era::Julie09 => Some(Tab09::julie09()),
            Era::Paul09 => Some(Tab09::paul09()),
            Era::Violeta09 => Some(Tab09::viol09()),
            Era::V2014 | Era::Paul04 => None,
        }
    }

    /// Length of the half-phone keys.
    pub fn hk_len(&self) -> usize {
        if self.viol { 8 } else { 9 }
    }

    pub fn b(&self, addr: u32, i: u8) -> u8 {
        self.data.get((addr - self.lo) as usize + i as usize).copied().unwrap_or(0)
    }

    pub fn i(&self, addr: u32, i: usize) -> i32 {
        let o = (addr - self.lo) as usize + 4 * i;
        i32::from_le_bytes([self.data[o], self.data[o + 1], self.data[o + 2], self.data[o + 3]])
    }

    pub fn f(&self, addr: u32, i: usize) -> f32 {
        f32::from_bits(self.i(addr, i) as u32)
    }
}

/// `us_LabelToClassKey` of the 2009 engines (julie09 `FUN_10016810`, viol09 `FUN_10018f80`): the
/// 7-byte class key of a 5-byte unit label `[L1, C, R1, a, b]`.
pub fn class_key09(t: &Tab09, l: &[u8; 5]) -> [u8; 7] {
    if t.viol {
        let voiced = t.b(t.voiced, l[1]) != 0;
        let k0 = if !voiced {
            t.b(t.l_nv, l[0])
        } else if l[3] / 10 == 2 {
            t.b(t.l_v2, l[0])
        } else {
            t.b(t.key_l, l[0])
        };
        let k2 = if l[3] % 10 == 2 {
            t.b(t.hk2_cls, l[2])
        } else if !voiced {
            t.b(t.r_nv, l[2])
        } else {
            t.b(t.key_r, l[2])
        };
        return [k0, t.b(t.key_c, l[1]), k2, l[3], l[4], t.b(t.key_c, l[2]), t.b(t.key_c, l[0])];
    }
    let hi = l[3] / 10;
    let lo = l[3] % 10;
    let k3 = if t.b(t.vowel, l[1]) == 0 || hi < 2 { hi } else { 2 };
    let mut k6 = l[4] % 10;
    if lo == 0 && l[4] % 10 == 2 {
        k6 = 1;
    }
    [t.b(t.key_l, l[0]), t.b(t.key_c, l[1]), t.b(t.key_r, l[2]), k3, lo, 0, k6]
}

/// The half-phone key of a 7-byte class key (julie09 `FUN_100168c0`: 9 bytes; viol09
/// `FUN_10019060`: 8 bytes); `mode` 1 = left half, 2 = right half.
pub fn half_key09(t: &Tab09, k: &[u8; 7], mode: u8) -> Vec<u8> {
    if t.viol {
        let b = k[3] % 10;
        let b_hi = if b < 3 { 0 } else { b };
        let c = k[4] / 10;
        let d = k[4] % 10;
        let d_hi = if d < 3 { 0 } else { d };
        return if mode == 1 {
            vec![
                0,
                k[1],
                t.b(t.hk1_cls, k[0]),
                (k[3] / 10).wrapping_mul(10).wrapping_add(b_hi),
                k[0],
                c.wrapping_mul(10).wrapping_add(d_hi),
                d,
                t.b(t.hk_c, k[2]),
            ]
        } else {
            vec![1, k[1], t.b(t.hk2_cls, k[2]), b, k[2], d, c, t.b(t.hk_c, k[0])]
        };
    }
    if mode == 1 {
        let mut b2 = if k[3] < 2 { 0 } else { k[3].wrapping_mul(10) };
        b2 = b2.wrapping_add(t.b(t.voiced, k[0]));
        vec![0, k[1], b2, t.b(t.hk1_cls, k[0]), k[0], k[3], k[5], k[4], k[6]]
    } else {
        let mut b2 = if k[4] < 2 { 0 } else { k[4].wrapping_mul(10) };
        b2 = b2.wrapping_add(t.b(t.voiced, k[2]));
        let b7 = (t.b(t.voiced, k[1]) == 0 && k[3] != 0) as u8;
        vec![1, k[1], b2, t.b(t.hk2_cls, k[2]), k[2], k[4], k[6], b7, k[5]]
    }
}

fn tb(addr: u32, i: u8) -> u8 {
    DATA[(addr - DATA_LO) as usize + i as usize]
}

fn tbs(addr: u32, i: u8) -> u8 {
    tables_spa::DATA[(addr - tables_spa::DATA_LO) as usize + i as usize]
}

/// `us_LabelToClassKey` (0x10016ea0).
pub fn class_key(label: &[u8; 7]) -> [u8; 5] {
    [tb(0x1007b7ec, label[1]), tb(0x1007b788, label[2]), tb(0x1007b850, label[3]), label[5], label[6] & 0x20]
}

/// `us_LabelToClassKey` for Violeta (0x1001d470): same shape, its own maps at
/// 0x100671ac / 0x10067160 / 0x100671f8.
pub fn class_key_spa(label: &[u8; 7]) -> [u8; 5] {
    [tbs(0x100671ac, label[1]), tbs(0x10067160, label[2]), tbs(0x100671f8, label[3]), label[5], label[6] & 0x20]
}

pub fn class_key_lang(label: &[u8; 7], lang: Lang) -> [u8; 5] {
    match lang {
        Lang::Eng => class_key(label),
        Lang::Spa => class_key_spa(label),
    }
}

/// `FUN_10016ef0`: the 10-byte half-phone key of a 5-byte class key. `mode` 1 =
/// left half, 2 = right half. Any other mode leaves the key untouched in the
/// engine; callers never pass one.
pub fn half_key(k: &[u8; 5], mode: u8) -> [u8; 10] {
    let b = k[3];
    let rpos = b & 7;
    let lpos = (b >> 3) & 7;
    let lb = (b & 0x80 == 0x80 && lpos >= 2) as u8;
    let rb = ((b >> 6) & 1 == 1 && rpos >= 2) as u8;
    if mode == 1 {
        [
            0,
            k[4],
            k[1],
            tb(0x1007b97c, k[0]),
            lb,
            tb(0x1007b8b4, k[0]),
            lpos.wrapping_mul(10).wrapping_add(rb),
            k[0],
            rpos,
            tb(0x1007b97c, k[2]),
        ]
    } else {
        [
            1,
            k[4],
            k[1],
            tb(0x1007b97c, k[2]),
            rb,
            tb(0x1007b918, k[2]),
            rpos.wrapping_mul(10).wrapping_add(lb),
            k[2],
            lpos,
            tb(0x1007b97c, k[0]),
        ]
    }
}

/// `FUN_1001d4c0`: Violeta's half-phone key is 11 bytes, not 10 -- it inserts
/// `T_COLLAPSE[centre]` right after the EXP bit (byte 2), shifting the rest of
/// the English layout down by one and moving the raw centre byte to where
/// English keeps `hkL`/`hkR` (byte 5), with `T_HKL`/`T_HKR` (0x10067244 /
/// 0x10067290) taking byte 6.
pub fn half_key_spa(k: &[u8; 5], mode: u8) -> [u8; 11] {
    let b = k[3];
    let rpos = b & 7;
    let lpos = (b >> 3) & 7;
    let lb = (b & 0x80 == 0x80 && lpos >= 2) as u8;
    let rb = ((b >> 6) & 1 == 1 && rpos >= 2) as u8;
    if mode == 1 {
        [
            0,
            k[4],
            tbs(0x10067114, k[1]),
            tbs(0x10067458, k[0]),
            lb,
            k[1],
            tbs(0x10067244, k[0]),
            lpos.wrapping_mul(10).wrapping_add(rb),
            k[0],
            rpos,
            tbs(0x10067458, k[2]),
        ]
    } else {
        [
            1,
            k[4],
            tbs(0x10067114, k[1]),
            tbs(0x10067458, k[2]),
            rb,
            k[1],
            tbs(0x10067290, k[2]),
            rpos.wrapping_mul(10).wrapping_add(lb),
            k[2],
            lpos,
            tbs(0x10067458, k[0]),
        ]
    }
}

pub fn half_key_lang(k: &[u8; 5], mode: u8, lang: Lang) -> Vec<u8> {
    match lang {
        Lang::Eng => half_key(k, mode).to_vec(),
        Lang::Spa => half_key_spa(k, mode).to_vec(),
    }
}

pub fn half_key_len(lang: Lang) -> usize {
    match lang {
        Lang::Eng => 10,
        Lang::Spa => 11,
    }
}

/// `FUN_10019350`: unsigned compare of the first `n` bytes.
pub fn cmp_prefix(a: &[u8], b: &[u8], n: usize) -> i32 {
    for i in 0..n {
        if a[i] < b[i] {
            return -1;
        }
        if a[i] > b[i] {
            return 1;
        }
    }
    0
}

pub struct Classes {
    pub lang: Lang,
    pub key: Vec<[u8; 5]>,
    pub count: Vec<u16>,
    pub start: Vec<u32>,
    /// Units, grouped by class, ascending within a class.
    pub units: Vec<u32>,
    /// Class of each global unit.
    pub class_of: Vec<u32>,
    /// Class indices ordered by the left (0) and right (1) half-phone keys.
    pub half_order: [Vec<u32>; 2],
    /// 10 bytes/key for English, 11 for Violeta (`half_key_len`).
    pub half_keys: [Vec<Vec<u8>>; 2],
    /// 2009 builds only: the 7-byte class keys (`key` stays empty there).
    pub key7: Vec<[u8; 7]>,
}

impl Classes {
    pub fn build(bank: &Bank) -> Classes {
        Self::build_lang(bank, Lang::Eng)
    }

    /// Classes of a 2009 bank, as `us_MakePhoneClass` + `us_MakeHalfPhoneOrders` (julie09
    /// `FUN_1001bcf0` / `FUN_1002c190`) build them when no class index file exists: classes in
    /// ascending 7-byte key order, units ascending within a class, the two half-key orders (9-byte
    /// keys) by the engine's heap sort.
    pub fn build_era(bank: &Bank, era: Era) -> Classes {
        let t = Tab09::for_era(era).expect("2009 selection tables for this build");
        let n = bank.len();
        let keys: Vec<[u8; 7]> = bank.label5.iter().map(|l| class_key09(&t, l)).collect();
        let mut order: Vec<u32> = (0..n as u32).collect();
        order.sort_by(|&a, &b| keys[a as usize].cmp(&keys[b as usize]).then(a.cmp(&b)));
        let mut cls = Classes {
            lang: Lang::Eng,
            key: Vec::new(),
            count: Vec::new(),
            start: Vec::new(),
            units: order.clone(),
            class_of: vec![0; n],
            half_order: [Vec::new(), Vec::new()],
            half_keys: [Vec::new(), Vec::new()],
            key7: Vec::new(),
        };
        for (i, &u) in order.iter().enumerate() {
            let k = keys[u as usize];
            if cls.key7.last() != Some(&k) {
                cls.key7.push(k);
                cls.count.push(0);
                cls.start.push(i as u32);
            }
            let c = cls.key7.len() - 1;
            *cls.count.last_mut().unwrap() += 1;
            cls.class_of[u as usize] = c as u32;
        }
        for m in 0..2 {
            let hk: Vec<Vec<u8>> = cls.key7.iter().map(|k| half_key09(&t, k, m as u8 + 1)).collect();
            let mut idx: Vec<u32> = (0..cls.key7.len() as u32).collect();
            let mut ra: Vec<usize> = (0..hk.len()).collect();
            nr_heapsort2(&mut ra, &mut idx, |a, b| cmp_prefix(&hk[a], &hk[b], t.hk_len()));
            cls.half_keys[m] = ra.iter().map(|&i| hk[i].clone()).collect();
            cls.half_order[m] = idx;
        }
        cls
    }

    /// `FUN_1001ab30` (julie09): the contiguous run of classes whose 7-byte key matches `k` in its
    /// first `n` bytes. Returns `(first, count)`.
    pub fn find_class_range7(&self, k: &[u8; 7], n: usize) -> (u32, u32) {
        let len = self.key7.len() as i32;
        let (mut lo, mut hi) = (0i32, len - 1);
        if hi < 0 {
            return (0, 0);
        }
        let mid = loop {
            let mid = (lo + hi) / 2;
            let c = cmp_prefix(k, &self.key7[mid as usize], n);
            if c == 0 {
                break mid;
            }
            if c < 0 {
                hi = mid - 1;
            } else {
                lo = mid + 1;
            }
            if hi < lo {
                return (0, 0);
            }
        };
        let mut first = mid;
        while first > 0 && cmp_prefix(k, &self.key7[first as usize - 1], n) == 0 {
            first -= 1;
        }
        let mut last = mid;
        while last < len - 1 && cmp_prefix(k, &self.key7[last as usize + 1], n) == 0 {
            last += 1;
        }
        (first as u32, (last - first + 1) as u32)
    }

    pub fn build_lang(bank: &Bank, lang: Lang) -> Classes {
        let n = bank.len();
        let keys: Vec<[u8; 5]> = bank.label.iter().map(|l| class_key_lang(l, lang)).collect();
        let mut order: Vec<u32> = (0..n as u32).collect();
        order.sort_by(|&a, &b| keys[a as usize].cmp(&keys[b as usize]).then(a.cmp(&b)));
        let mut cls = Classes {
            lang,
            key: Vec::new(),
            count: Vec::new(),
            start: Vec::new(),
            units: order.clone(),
            class_of: vec![0; n],
            half_order: [Vec::new(), Vec::new()],
            half_keys: [Vec::new(), Vec::new()],
            key7: Vec::new(),
        };
        for (i, &u) in order.iter().enumerate() {
            let k = keys[u as usize];
            if cls.key.last() != Some(&k) {
                cls.key.push(k);
                cls.count.push(0);
                cls.start.push(i as u32);
            }
            let c = cls.key.len() - 1;
            // The engine counts in a u16; no Paul class comes close to wrapping.
            *cls.count.last_mut().unwrap() += 1;
            cls.class_of[u as usize] = c as u32;
        }
        let hklen = half_key_len(lang);
        for m in 0..2 {
            let hk: Vec<Vec<u8>> = cls.key.iter().map(|k| half_key_lang(k, m as u8 + 1, lang)).collect();
            let mut idx: Vec<u32> = (0..cls.key.len() as u32).collect();
            let mut ra: Vec<usize> = (0..hk.len()).collect();
            nr_heapsort2(&mut ra, &mut idx, |a, b| cmp_prefix(&hk[a], &hk[b], hklen));
            cls.half_keys[m] = ra.iter().map(|&i| hk[i].clone()).collect();
            cls.half_order[m] = idx;
        }
        cls
    }

    pub fn n_classes(&self) -> usize {
        self.key.len().max(self.key7.len())
    }

    pub fn units_of(&self, c: u32) -> &[u32] {
        let s = self.start[c as usize] as usize;
        &self.units[s..s + self.count[c as usize] as usize]
    }

    /// `FUN_10019450`: the contiguous run of classes whose key matches `k` in
    /// its first `n` bytes. Returns `(first, count)`.
    pub fn find_class_range(&self, k: &[u8; 5], n: usize) -> (u32, u32) {
        let len = self.key.len() as i32;
        let (mut lo, mut hi) = (0i32, len - 1);
        if hi < 0 {
            return (0, 0);
        }
        let mid = loop {
            let mid = (lo + hi) / 2;
            let c = cmp_prefix(k, &self.key[mid as usize], n);
            if c == 0 {
                break mid;
            }
            if c < 0 {
                hi = mid - 1;
            } else {
                lo = mid + 1;
            }
            if hi < lo {
                return (0, 0);
            }
        };
        let mut first = mid;
        while first > 0 && cmp_prefix(k, &self.key[first as usize - 1], n) == 0 {
            first -= 1;
        }
        let mut last = mid;
        while last < len - 1 && cmp_prefix(k, &self.key[last as usize + 1], n) == 0 {
            last += 1;
        }
        (first as u32, (last - first + 1) as u32)
    }

    /// `FUN_1002df50`: range of the half-phone ordering whose keys match `hk` in
    /// the first `plen` bytes. The ordering is chosen by `hk[0]`. Returns
    /// `(first position, count)` into `half_order[hk[0]]`; count 0 = not found.
    pub fn find_half_range(&self, hk: &[u8], plen: usize) -> (usize, usize) {
        let m = (hk[0] != 0) as usize;
        let keys = &self.half_keys[m];
        let n = keys.len() as i32;
        let mut lo = 0i32;
        let mut hi = n - 1;
        let found = loop {
            if cmp_prefix(hk, &keys[lo as usize], plen) == 0 {
                break lo;
            }
            if cmp_prefix(hk, &keys[hi as usize], plen) == 0 {
                break hi;
            }
            if hi - lo < 2 {
                return (0, 0);
            }
            let mid = (hi - lo) / 2 + lo;
            let c = cmp_prefix(hk, &keys[mid as usize], plen);
            if c == 0 {
                break mid;
            }
            if c < 0 {
                hi = mid;
            } else {
                lo = mid;
            }
        };
        let mut first = found;
        while first >= 1 && cmp_prefix(hk, &keys[first as usize - 1], plen) == 0 {
            first -= 1;
        }
        let mut last = found;
        while last < n - 1 && cmp_prefix(hk, &keys[last as usize + 1], plen) == 0 {
            last += 1;
        }
        (first as usize, (last - first + 1) as usize)
    }
}

/// Numerical Recipes `sort2` heapsort, as `FUN_1002dd70` implements it: sorts
/// `ra` by `cmp` carrying `rb` along. Equal keys land in heapsort order, which
/// decides the order of classes inside a half-phone range.
fn nr_heapsort2<F: Fn(usize, usize) -> i32>(ra: &mut [usize], rb: &mut [u32], cmp: F) {
    let n = ra.len();
    if n < 2 {
        return;
    }
    // 1-based views.
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
