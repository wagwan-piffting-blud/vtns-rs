//! Unit selection (vt_eng.dll `us_SelectUnitsForPhrase` 0x10024980 and callees).
//!
//! Ported instruction-for-instruction where it changes results: the engine's
//! introsort decides tie order, candidate records live in a persistent pool
//! addressed through a permuted pointer array, and the x87 code runs at 53-bit
//! precision with float stores exactly where noted.

#[path = "classes.rs"]
pub mod classes;
#[path = "unitsel_tables.rs"]
pub mod tables;

use self::classes::{class_key, half_key, Classes};
use self::tables::{DATA, DATA_LO, LEFT_POS_LISTS, RDATA, RDATA_LO, RIGHT_POS_LISTS};
use crate::bank::Bank;

const MAX_CANDS: usize = 10000;
const BEAM: usize = 30;

fn tb(addr: u32, i: u8) -> u8 {
    DATA[(addr - DATA_LO) as usize + i as usize]
}

fn ti(addr: u32, i: usize) -> i32 {
    let o = (addr - DATA_LO) as usize + 4 * i;
    i32::from_le_bytes([DATA[o], DATA[o + 1], DATA[o + 2], DATA[o + 3]])
}

fn tf(addr: u32, i: usize) -> f32 {
    f32::from_bits(ti(addr, i) as u32)
}

fn rf(addr: u32) -> f32 {
    let o = (addr - RDATA_LO) as usize;
    f32::from_le_bytes([RDATA[o], RDATA[o + 1], RDATA[o + 2], RDATA[o + 3]])
}

// Phone-code tables (byte-indexed).
const T_COLLAPSE: u32 = 0x1007b6c0; // stress triplet -> one phone
const T_BROAD: u32 = 0x1007b97c; // 5-way broad class
const T_VOWEL: u32 = 0x1007b9e0;
const T_STRESS: u32 = 0x1007baa8;
const T_VOICED: u32 = 0x1007ba44;
const T_CTX_SUB: u32 = 0x1007c2c4;
const T_CENTER_SUB: u32 = 0x1007c324;
const T_EDGE_SUB: u32 = 0x1007c4a4;
const T_BB70: u32 = 0x1007bb70;
const T_C384: u32 = 0x1007c384;
const T_C3E4: u32 = 0x1007c3e4;
const T_C444: u32 = 0x1007c444;

/// Cepstral VQ distance matrix (`cepdist.tbl`), lower triangular.
pub struct CepDist {
    pub n: usize,
    tri: Vec<f32>,
}

impl CepDist {
    pub fn load(path: &std::path::Path) -> std::io::Result<CepDist> {
        let b = std::fs::read(path)?;
        let n = u16::from_le_bytes([b[0], b[1]]) as usize;
        let cnt = (n / 2) * (n + 1);
        let tri = (0..cnt)
            .map(|i| f32::from_le_bytes([b[2 + 4 * i], b[3 + 4 * i], b[4 + 4 * i], b[5 + 4 * i]]))
            .collect();
        Ok(CepDist { n, tri })
    }

    pub fn get(&self, a: u16, b: u16) -> f32 {
        let (hi, lo) = if a < b { (b as usize, a as usize) } else { (a as usize, b as usize) };
        self.tri[hi * (hi + 1) / 2 + lo]
    }
}

/// `us_MakeScalarDistTable` (0x1001ae70): `(a-b)^2 / ((a+b)/2)` over byte codes.
pub struct ScalarDist {
    tri: Vec<f32>,
}

impl ScalarDist {
    pub fn build() -> ScalarDist {
        let mut tri = Vec::with_capacity(256 * 257 / 2);
        let half = rf(0x1006d174);
        let zero = rf(0x1006d188);
        for i in 0..256 {
            for j in 0..=i {
                let (a, b) = (i as f32, j as f32);
                let m = ((a as f64 + b as f64) / half as f64) as f32;
                let v = if m == zero {
                    zero
                } else {
                    let d = (a as f64 - b as f64) as f32;
                    ((d as f64 * d as f64) / m as f64) as f32
                };
                tri.push(v);
            }
        }
        ScalarDist { tri }
    }

    pub fn get(&self, a: u8, b: u8) -> f32 {
        let (hi, lo) = if a < b { (b as usize, a as usize) } else { (a as usize, b as usize) };
        self.tri[hi * (hi + 1) / 2 + lo]
    }
}

/// The per-word fields unit selection reads out of the front end's 0x3c0-byte
/// word record (offsets relative to the record).
#[derive(Debug, Clone, Default)]
pub struct Word {
    /// `+0x10 + 2k`: predicted duration of phone k, 16 kHz samples.
    pub dur: Vec<i16>,
    /// `+0x96 + 7k`: `{LL, L, C, R, RR, posbits, flags}`.
    pub ctx: Vec<[u8; 7]>,
    /// `+0x25d + k`: two-digit left/right boundary code (tens = left).
    pub edge_code: Vec<u8>,
    /// `+0x29e + k`: blocks centre substitution in the back-off lattice.
    pub mflag: Vec<u8>,
    /// `+0x2df`: word domain (0x0c = EXP, 8 = NUM_SPLIT, 0 = general).
    pub domain: u8,
    /// Per syllable: phone count (`+0x1d`) and 12-point F0 in Hz (`+4`).
    pub syl_nph: Vec<u8>,
    pub syl_f0: Vec<[i16; 12]>,
}

impl Word {
    /// Build from a raw word record and its syllable records.
    pub fn from_raw(raw: &[u8], syls: &[Vec<u8>]) -> Word {
        let n_ph = raw[0x95] as usize;
        let mut w = Word { domain: raw[0x2df], ..Default::default() };
        for k in 0..n_ph {
            w.dur.push(i16::from_le_bytes([raw[0x10 + 2 * k], raw[0x11 + 2 * k]]));
            let mut c = [0u8; 7];
            c.copy_from_slice(&raw[0x96 + 7 * k..0x9d + 7 * k]);
            w.ctx.push(c);
            w.edge_code.push(raw[0x25d + k]);
            w.mflag.push(raw[0x29e + k]);
        }
        for s in syls {
            w.syl_nph.push(s[0x1d]);
            let mut f = [0i16; 12];
            for (i, v) in f.iter_mut().enumerate() {
                *v = i16::from_le_bytes([s[4 + 2 * i], s[5 + 2 * i]]);
            }
            w.syl_f0.push(f);
        }
        w
    }

    pub fn n_syl(&self) -> usize {
        self.syl_nph.len()
    }
}

/// 6-byte target record at `TIB+0xec628 + 6t`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Target {
    pub word: u8,
    pub syl: u8,
    pub phone: u8,
    /// Index of this target among its word's targets.
    pub idx: u8,
    /// 0 whole phone, 1 left half, 2 right half.
    pub half: u8,
    pub unvoiced: u8,
}

/// Candidate record (0x14 bytes each, pool at `TIB+0x513ec`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Cand {
    pub tc: f32,
    pub score: f32,
    /// Unit id, or a class id while ranking classes.
    pub id: i32,
    pub fwd_run: i16,
    pub run: i16,
    pub wrun: i16,
    pub pref: u8,
}

/// Per-target slot (`TIB+0xae894 + 0xfc t`).
#[derive(Debug, Clone, Default)]
pub struct Slot {
    pub classes: Vec<i32>,
    pub n_pref_classes: usize,
    pub kept: Vec<i32>,
    /// `+0xf4`: expanded, then kept, then post-prune count.
    pub n: usize,
    pub acc: Vec<f32>,
    pub back: Vec<i16>,
}

/// What one phrase produced, including the intermediate state a capture of the
/// original engine records.
#[derive(Debug, Clone, Default)]
pub struct PhraseResult {
    pub targets: Vec<Target>,
    pub feats: Vec<[u8; 3]>,
    /// Candidate records as `us_GetCandidates` leaves them, `n` entries each.
    pub cands: Vec<Vec<Cand>>,
    pub kept_after_get: Vec<Vec<i32>>,
    pub slots: Vec<Slot>,
    pub path: Vec<i16>,
    pub uids: Vec<u32>,
}

pub struct Selector<'a> {
    pub bank: &'a Bank,
    pub cls: &'a Classes,
    pub cep: &'a CepDist,
    pub sd: &'a ScalarDist,
    /// Persistent candidate pool and its pointer array (reset per request).
    pool: Vec<Cand>,
    ptrs: Vec<usize>,
    /// History counters (`.his` mode), per unit: candidate / chosen.
    pub history: Option<(Vec<u32>, Vec<u32>)>,
    // per-phrase working state
    targets: Vec<Target>,
    slots: Vec<Slot>,
    list_a: Vec<i32>,
    list_b: Vec<i32>,
    final_type: u8,
    word_first_target: Vec<usize>,
    word_ntargets: Vec<i16>,
    scratch: Vec<f32>,
    /// Reproduce the 64-bit voice DLLs (all_voices\\bin\\vt_*.dll): their SSE build
    /// rounds every float operation of the target-cost tail and the join step to
    /// f32, in its own term order, where the 32-bit x87 build rounds once per store.
    pub x64: bool,
}

impl<'a> Selector<'a> {
    pub fn new(bank: &'a Bank, cls: &'a Classes, cep: &'a CepDist, sd: &'a ScalarDist) -> Selector<'a> {
        Selector {
            bank,
            cls,
            cep,
            sd,
            pool: vec![Cand::default(); MAX_CANDS],
            ptrs: (0..MAX_CANDS).collect(),
            history: None,
            targets: Vec::new(),
            slots: Vec::new(),
            list_a: Vec::new(),
            list_b: Vec::new(),
            final_type: 5,
            word_first_target: Vec::new(),
            word_ntargets: Vec::new(),
            scratch: vec![0.0; BEAM],
            x64: false,
        }
    }

    /// Start of a new request (`FUN_10025fc0` resets the pointer array).
    pub fn reset(&mut self) {
        self.pool = vec![Cand::default(); MAX_CANDS];
        self.ptrs = (0..MAX_CANDS).collect();
    }

    fn c(&self, i: usize) -> &Cand {
        &self.pool[self.ptrs[i]]
    }

    fn cm(&mut self, i: usize) -> &mut Cand {
        let p = self.ptrs[i];
        &mut self.pool[p]
    }

    fn units_in(&self, c: i32) -> i32 {
        self.cls.count[c as usize] as i32
    }

    fn sum_units(&self, list: &[i32]) -> i32 {
        list.iter().map(|&c| self.units_in(c)).sum()
    }

    fn file_domain(&self, uid: i32) -> u8 {
        self.bank.pools[self.bank.file_idx[uid as usize] as usize].domain
    }

    // ------------------------------------------------------------------ sorts

    /// `us_SortByScore` (0x1001b5f0): introsort on `score` over `ptrs[lo..lo+n]`.
    fn sort_by_score(&mut self, lo0: usize, n: usize) {
        if n < 2 {
            return;
        }
        let key = |s: &Self, i: usize| s.pool[s.ptrs[i]].score;
        let mut stk_lo = [0usize; 32];
        let mut stk_hi = [0usize; 32];
        let mut depth = 0usize;
        let mut lo = lo0;
        let mut hi = lo0 + n - 1;
        loop {
            if hi as isize - lo as isize <= 16 {
                let mut prev = self.ptrs[lo];
                let mut i = lo + 1;
                while i <= hi {
                    let cur = self.ptrs[i];
                    if self.pool[prev].score > self.pool[cur].score {
                        self.ptrs[i] = prev;
                        let mut j = i - 1;
                        while j > lo {
                            let t = self.ptrs[j - 1];
                            if self.pool[t].score > self.pool[cur].score {
                                self.ptrs[j] = t;
                                j -= 1;
                            } else {
                                break;
                            }
                        }
                        self.ptrs[j] = cur;
                    } else {
                        prev = cur;
                    }
                    i += 1;
                }
                if depth == 0 {
                    return;
                }
                depth -= 1;
                lo = stk_lo[depth];
                hi = stk_hi[depth];
                continue;
            }
            let mid = (lo + hi) >> 1;
            if key(self, lo) > key(self, hi) {
                self.ptrs.swap(lo, hi);
            }
            if key(self, lo) > key(self, mid) {
                self.ptrs.swap(lo, mid);
            }
            if key(self, mid) > key(self, hi) {
                self.ptrs.swap(mid, hi);
            }
            let pivot = self.pool[self.ptrs[mid]].score;
            let mut i = lo;
            let mut j = hi;
            loop {
                loop {
                    i += 1;
                    if !(pivot > key(self, i)) {
                        break;
                    }
                }
                loop {
                    j -= 1;
                    if !(key(self, j) > pivot) {
                        break;
                    }
                }
                if j <= i {
                    break;
                }
                self.ptrs.swap(i, j);
            }
            let left = (j - lo + 1) as isize;
            let right = hi as isize - j as isize;
            if left >= right {
                if (left >> 5) > right {
                    self.heapsort(lo, left as usize);
                    lo = j + 1;
                } else {
                    stk_lo[depth] = lo;
                    stk_hi[depth] = j;
                    depth += 1;
                    lo = j + 1;
                }
            } else if (right >> 5) > left {
                self.heapsort(j + 1, right as usize);
                hi = j;
            } else {
                stk_lo[depth] = j + 1;
                stk_hi[depth] = hi;
                depth += 1;
                hi = j;
            }
        }
    }

    /// `FUN_1001b400`: bottom-up heapsort fallback of the introsort.
    fn heapsort(&mut self, off: usize, n: usize) {
        if n < 2 {
            return;
        }
        let e = |s: &Self, k: usize| s.pool[s.ptrs[off + k - 1]].score;
        let half = n >> 1;
        let mut s = half;
        while s >= 1 {
            let mut k = s;
            let mut depth = 0u32;
            loop {
                k *= 2;
                depth += 1;
                if k < n && e(self, k + 1) > e(self, k) {
                    k += 1;
                }
                if k > half {
                    break;
                }
            }
            let x = self.ptrs[off + s - 1];
            let xs = self.pool[x].score;
            while s != k {
                if !(xs > e(self, k)) {
                    break;
                }
                k >>= 1;
                depth -= 1;
            }
            while depth > 0 {
                self.ptrs[off + (k >> depth) - 1] = self.ptrs[off + (k >> (depth - 1)) - 1];
                depth -= 1;
            }
            self.ptrs[off + k - 1] = x;
            s -= 1;
        }
        let mut m = n - 1;
        loop {
            let x = self.ptrs[off + m];
            self.ptrs[off + m] = self.ptrs[off];
            self.ptrs[off] = x;
            let xs = self.pool[x].score;
            let mut k = 1usize;
            let mut depth = 0u32;
            let h = m >> 1;
            if h >= 1 {
                loop {
                    k *= 2;
                    depth += 1;
                    if k < m && e(self, k + 1) > e(self, k) {
                        k += 1;
                    }
                    if k > h {
                        break;
                    }
                }
                while k != 1 {
                    if !(xs > e(self, k)) {
                        break;
                    }
                    k >>= 1;
                    depth -= 1;
                }
                while depth > 0 {
                    self.ptrs[off + (k >> depth) - 1] = self.ptrs[off + (k >> (depth - 1)) - 1];
                    depth -= 1;
                }
            }
            self.ptrs[off + k - 1] = x;
            if m <= 1 {
                break;
            }
            m -= 1;
        }
    }

    // --------------------------------------------------------- class ranking

    /// `FUN_10023a70`: weighted mismatch between two class keys.
    fn key_dist(a: &[u8; 5], b: &[u8; 5], half: u8) -> i32 {
        let mut d = 0;
        for i in 0..5 {
            if a[i] != b[i] {
                d += ti(0x1007bf14, i);
            }
        }
        if half != 0 {
            let ha = half_key(a, half);
            let hb = half_key(b, half);
            for i in 0..10 {
                if ha[i] != hb[i] {
                    d += ti(0x1007bf28, i);
                }
            }
        }
        d
    }

    /// `us_LimitClasses30x10000` (half = 0) / `FUN_10023c70` (half = 1, 2):
    /// keep at most `maxc` classes, closest first, stopping once `maxu` units
    /// are exceeded (the class that crosses the limit is kept).
    fn limit_classes(&mut self, src: &[i32], maxc: usize, maxu: i32, label: &[u8; 7], half: u8) -> Vec<i32> {
        if src.len() <= maxc && self.sum_units(src) <= maxu {
            return src.to_vec();
        }
        let key = class_key(label);
        let mut n = 0usize;
        for &c in src {
            if half == 0 && n > 9999 {
                break;
            }
            let d = Self::key_dist(&key, &self.cls.key[c as usize], half);
            let cm = self.cm(n);
            cm.id = c;
            cm.score = d as f32;
            n += 1;
            if half != 0 && n > 9999 {
                break;
            }
        }
        self.sort_by_score(0, n);
        let n2 = n.min(maxc);
        let mut out = Vec::with_capacity(n2);
        let mut units = 0;
        for i in 0..n2 {
            let c = self.c(i).id;
            out.push(c);
            units += self.units_in(c);
            if units > maxu {
                break;
            }
        }
        out
    }

    // ------------------------------------------------------------- lookups

    /// `FUN_10023dc0`: exact whole-phone class match, appended to list A (cap 300).
    fn lookup_whole(&mut self, key7: &[u8; 7]) -> i32 {
        let k = class_key(key7);
        let (first, cnt) = self.cls.find_class_range(&k, 5);
        if cnt == 0 {
            return 0;
        }
        let mut add = cnt as i32;
        if self.list_a.len() as i32 + add > 300 {
            add = 300 - self.list_a.len() as i32;
        }
        for i in 0..add.max(0) {
            self.list_a.push(first as i32 + i);
        }
        add
    }

    fn half_range(&self, key7: &[u8; 7], half: u8, plen: usize) -> Vec<i32> {
        let hk = half_key(&class_key(key7), half);
        let (first, cnt) = self.cls.find_half_range(&hk, plen);
        let m = (hk[0] != 0) as usize;
        self.cls.half_order[m][first..first + cnt].iter().map(|&c| c as i32).collect()
    }

    /// `FUN_10023e70`: half-phone match with key-prefix back-off from 10 bytes.
    fn lookup_half(&mut self, key7: &[u8; 7], t: usize, mode: i16) -> i32 {
        if self.list_a.len() > 0x121 {
            return 0;
        }
        let half = self.targets[t].half;
        let mut plen: i32 = 10;
        let mut range = self.half_range(key7, half, plen as usize);
        while range.is_empty() {
            if mode != 1 && plen < 6 {
                return 0;
            }
            plen -= 1;
            range = self.half_range(key7, half, plen as usize);
        }
        let mut got = self.limit_classes(&range, 10, 10000, key7, half);
        let mut units = self.sum_units(&got);
        loop {
            if units > 9 || plen < 1 {
                break;
            }
            if mode != 1 && plen < 6 {
                return 0;
            }
            plen -= 1;
            range = self.half_range(key7, half, plen as usize);
            got = self.limit_classes(&range, 10, 10000, key7, half);
            units = self.sum_units(&got);
        }
        let n = got.len() as i32;
        self.list_a.extend(got);
        n
    }

    /// `FUN_10023f90`.
    fn lookup(&mut self, key7: &[u8; 7], t: usize, mode: i16) -> i32 {
        if self.targets[t].half == 0 {
            self.lookup_whole(key7)
        } else {
            self.lookup_half(key7, t, mode)
        }
    }

    /// `FUN_10018770`: context back-off lattice. Relaxes the boundary position
    /// codes (lists at 0x1007c4dc / 0x1007c504), substitutes edge contexts, and
    /// tries broader context / centre classes, collecting classes in list A.
    fn lattice(&mut self, l: &[u8; 7], t: usize, mode: i16, words: &[Word]) -> bool {
        let saved = self.list_a.len();
        let tg = self.targets[t];
        let b5 = l[5];
        let lb = b5 >> 7;
        let rb = (b5 >> 6) & 1;
        let rpos = b5 & 7;
        let lpos = (b5 >> 3) & 7;
        let lst = LEFT_POS_LISTS[(lb as usize) * 5 + lpos as usize];
        let rst = RIGHT_POS_LISTS[rpos as usize + (rb as usize) * 8];
        let word = &words[tg.word as usize];
        let code = word.edge_code[tg.phone as usize] as i32;
        let mflag = word.mflag[tg.phone as usize];
        let tens = code / 10;
        let ones = code % 10;
        let mut found = false;
        for (i, &v) in lst.iter().enumerate() {
            let (f3, a_ok) = if v == 0 { (0u8, true) } else { (lb, v == 1) };
            let local6 = a_ok == (lpos <= 1);
            if !(tens == 0 || local6 || tg.half == 2) {
                continue;
            }
            for (j, &w) in rst.iter().enumerate() {
                let (f5, b_ok) = if w == 0 { (0u8, true) } else { (rb, w == 1) };
                let local7 = b_ok == (rpos <= 1);
                if !(ones == 0 || local7 || tg.half == 1) {
                    continue;
                }
                let mut key = *l;
                let mut r: i32;
                if i == 0 && j == 0 {
                    r = self.lookup(&key, t, mode);
                    if mode == 1 && saved == 0 && r > 0 {
                        let nb = self.list_a.len().min(BEAM);
                        self.list_b = self.list_a[..nb].to_vec();
                    }
                } else {
                    if i > 0 && lb == 1 && v > 2 {
                        key[0] = tb(T_EDGE_SUB, v);
                        key[1] = key[0];
                    }
                    if j > 0 && rb == 1 && w > 2 {
                        key[3] = tb(T_EDGE_SUB, w);
                        key[4] = key[3];
                    }
                    key[5] = (((f3 * 2 + f5) * 8 + v) * 8).wrapping_add(w);
                    r = self.lookup(&key, t, 0);
                }
                if r != 0 || lb != 0 || rb != 0 {
                    if tb(T_CTX_SUB, l[2]) == 0 {
                        let c1 = tb(T_CTX_SUB, l[1]);
                        let c3 = tb(T_CTX_SUB, l[3]);
                        if c1 == 0 {
                            if c3 != 0 {
                                key[3] = c3;
                                r += self.lookup(&key, t, 0);
                            }
                        } else if c3 == 0 {
                            key[1] = c1;
                            r += self.lookup(&key, t, 0);
                        } else {
                            key[1] = c1;
                            let r1 = self.lookup(&key, t, 0);
                            key[3] = c3;
                            let r2 = self.lookup(&key, t, 0);
                            key = *l;
                            key[3] = c3;
                            let r3 = self.lookup(&key, t, 0);
                            r += r1 + r2 + r3;
                        }
                    } else if (rb != 0 || l[2] != 0x39 || (tb(T_BB70, l[3]) as i8) < 1) && mflag == 0 {
                        key[2] = tb(T_CTX_SUB, l[2]);
                        r += self.lookup(&key, t, 0);
                    }
                    if !found && r > 0 && local6 && local7 {
                        found = true;
                    }
                }
            }
        }
        if mode != 2 && !found {
            self.list_a.truncate(saved);
            return false;
        }
        if mode != 0 && tb(T_CENTER_SUB, l[2]) != 0 {
            let mut key = *l;
            key[2] = tb(T_CENTER_SUB, l[2]);
            self.lattice(&key, t, 0, words);
        }
        true
    }

    fn sort_unique(v: &mut Vec<i32>) {
        v.sort_unstable();
        v.dedup();
    }

    fn finish_class_list(&mut self, t: usize) {
        let mut list = self.list_b.clone();
        for &c in &self.list_a {
            if !self.list_b.contains(&c) {
                list.push(c);
            }
        }
        let s = &mut self.slots[t];
        s.n_pref_classes = self.list_b.len();
        s.classes = list;
    }

    /// `us_LookupWholePhone` (0x10024060). Returns 1 when the whole phone has
    /// more than 9 candidate units, else 0 (caller falls back to halves).
    fn lookup_whole_phone(&mut self, t: usize, words: &mut [Word]) -> i32 {
        let tg = self.targets[t];
        let wi = tg.word as usize;
        self.targets[t].idx = self.word_ntargets[wi] as u8;
        self.targets[t].half = 0;
        let dom = words[wi].domain;
        let l = words[wi].ctx[tg.phone as usize];
        self.list_a.clear();
        self.list_b.clear();
        let mode = if dom == 0x0c {
            let mut l2 = l;
            l2[6] &= 0xdf;
            self.lattice(&l2, t, 1, words);
            if self.list_a.is_empty() {
                return 0;
            }
            2
        } else {
            1
        };
        self.lattice(&l, t, mode, words);
        if self.list_a.is_empty() {
            return 0;
        }
        Self::sort_unique(&mut self.list_a);
        let a = self.list_a.clone();
        self.list_a = self.limit_classes(&a, BEAM, 10000, &l, 0);
        Self::sort_unique(&mut self.list_b);
        self.finish_class_list(t);
        let units = self.sum_units(&self.slots[t].classes);
        if dom == 8 && units > 0 {
            return 1;
        }
        (units > 9) as i32
    }

    /// `us_LookupHalfPhones` (0x100242a0): always yields two targets.
    fn lookup_half_phones(&mut self, t: usize, words: &mut [Word]) -> i32 {
        let base = self.targets[t];
        let wi = base.word as usize;
        let l = words[wi].ctx[base.phone as usize];
        for h in 0..2usize {
            if h == 0 {
                self.targets[t].half = 1;
            } else {
                let mut r = self.targets[t];
                r.half = 2;
                if self.targets.len() <= t + 1 {
                    self.targets.push(r);
                    self.slots.push(Slot::default());
                } else {
                    self.targets[t + 1] = r;
                }
            }
            self.targets[t + h].idx = (self.word_ntargets[wi] as u8).wrapping_add(h as u8);
            self.list_a.clear();
            self.list_b.clear();
            if words[wi].domain == 0x0c {
                let mut l2 = l;
                l2[6] &= 0x80;
                self.lattice(&l2, t + h, 1, words);
                self.lattice(&l, t + h, 2, words);
            } else {
                self.lattice(&l, t + h, 1, words);
            }
            Self::sort_unique(&mut self.list_a);
            let a = self.list_a.clone();
            let half = self.targets[t + h].half;
            self.list_a = self.limit_classes(&a, BEAM, 10000, &l, half);
            Self::sort_unique(&mut self.list_b);
            self.finish_class_list(t + h);
        }
        2
    }

    /// `us_BuildTargets` (0x10024680).
    fn build_targets(&mut self, words: &mut [Word], first: usize, nw: usize) -> usize {
        let mut n = 0usize;
        self.targets.clear();
        self.slots.clear();
        for wi in first..first + nw {
            self.word_ntargets[wi] = 0;
            self.word_first_target[wi] = n;
            for k in 0..words[wi].ctx.len() {
                if self.targets.len() <= n {
                    self.targets.push(Target::default());
                    self.slots.push(Slot::default());
                }
                let w = &words[wi];
                let mut s = 0usize;
                let mut acc = 0usize;
                while s < w.n_syl() {
                    let nph = w.syl_nph[s] as usize;
                    if acc <= k && k < acc + nph {
                        break;
                    }
                    acc += nph;
                    s += 1;
                }
                self.targets[n] = Target {
                    word: wi as u8,
                    syl: s as u8,
                    phone: k as u8,
                    idx: self.targets[n].idx,
                    half: self.targets[n].half,
                    unvoiced: (tb(T_VOICED, w.ctx[k][2]) == 0) as u8,
                };
                if w.dur[k] > 0 {
                    let mut r = self.lookup_whole_phone(n, words);
                    if r < 1 {
                        r = self.lookup_half_phones(n, words);
                    }
                    self.word_ntargets[wi] += r as i16;
                    n += r as usize;
                }
            }
        }
        self.targets.truncate(n);
        self.slots.truncate(n);
        n
    }

    // ---------------------------------------------------------- candidates

    fn quantize_dur(d: i16) -> u8 {
        let x = d as f32;
        let v = (x as f64 / rf(0x1006d1a0) as f64 + rf(0x1006d178) as f64).trunc() as i32 as i16;
        if v < 0 {
            0
        } else if v >= 0x100 {
            0xff
        } else {
            v as u8
        }
    }

    /// The 3-byte target feature `{dur_code, f0_a, f0_b}` (start of `us_GetCandidates`).
    fn target_feat(&self, t: usize, words: &[Word]) -> [u8; 3] {
        let tg = self.targets[t];
        let w = &words[tg.word as usize];
        let durk = |ph: u8, half: u8| -> i32 {
            let d = w.dur[ph as usize] as i32;
            if half != 0 { d / 2 } else { d }
        };
        let dc = Self::quantize_dur(durk(tg.phone, tg.half) as i16);
        let first = self.word_first_target[tg.word as usize];
        let nt = self.word_ntargets[tg.word as usize] as usize;
        let mut i = 0usize;
        while i < nt && self.targets[first + i].syl != tg.syl {
            i += 1;
        }
        let (mut before, mut upto, mut total) = (0i32, 0i32, 0i32);
        let (a, b);
        if i < nt {
            while i < nt {
                let r = self.targets[first + i];
                if r.syl != tg.syl {
                    break;
                }
                if r.unvoiced == 0 {
                    let d = durk(r.phone, r.half);
                    if (i as i32) < tg.idx as i32 {
                        before += d;
                    } else if i == tg.idx as usize {
                        upto = before + d;
                    }
                    total += d;
                }
                i += 1;
            }
            if total == 0 {
                a = 0;
                b = 0;
            } else {
                let mut aa = before * 12 / total;
                let mut bb = upto * 12 / total - 1;
                if aa > 11 {
                    aa = 11;
                }
                if bb < aa {
                    bb = aa;
                }
                a = aa;
                b = bb;
            }
        } else {
            a = 0;
            b = 0;
        }
        let f0 = &w.syl_f0[tg.syl as usize];
        [dc, f0[a as usize] as u8, f0[b as usize] as u8]
    }

    /// `us_ScoreContiguity` (0x100230a0).
    fn score_contiguity(&mut self, t: usize) {
        let n = self.slots[t].n;
        let nt = self.targets.len();
        let wts = [2i32, 1, 1];
        for c in 0..n {
            let u = self.c(c).id;
            let (mut back, mut wback) = (0i32, 0i32);
            let (mut fwd, mut wfwd) = (0i32, 0i32);
            let mut cur = u;
            let mut k = t as isize - 1;
            while k >= 0 {
                let kt = self.targets[k as usize];
                if kt.half != 1 {
                    cur -= 1;
                    if cur < 0 || self.bank.label[cur as usize][6] & 0x80 == 0 {
                        break;
                    }
                }
                let s = &self.slots[k as usize];
                if !s.kept[..s.n].contains(&cur) {
                    break;
                }
                back += 1;
                wback += wts[kt.half as usize];
                k -= 1;
            }
            let mut cur = u;
            let mut k = t + 1;
            while k < nt {
                let kt = self.targets[k];
                if kt.half != 2 {
                    if self.bank.label[cur as usize][6] & 0x80 == 0 {
                        break;
                    }
                    cur += 1;
                }
                let cl = self.cls.class_of[cur as usize] as i32;
                if !self.slots[k].classes.contains(&cl) {
                    break;
                }
                fwd += 1;
                wfwd += wts[kt.half as usize];
                k += 1;
            }
            let run = (fwd + 1 + back) as i16;
            let wb = (wback as i16) / 2;
            let wf = (wfwd as i16) / 2;
            let wrun = if run as usize == nt {
                wf + 1 + wb
            } else if back as i16 == t as i16 {
                wf
            } else if fwd != nt as i32 - t as i32 - 1 {
                wb.min(wf)
            } else {
                wb
            };
            let cm = self.cm(c);
            cm.run = run;
            cm.fwd_run = fwd as i16;
            cm.wrun = wrun;
            cm.score = -((run as i32 + wrun as i32 * 100) as f32);
        }
        self.sort_by_score(0, n);
    }

    /// `us_TargetCost` (0x100182e0).
    fn target_cost(&self, uid: i32, t: usize, feat: &[u8; 3], w: f32, words: &[Word]) -> f32 {
        let tg = self.targets[t];
        let word = &words[tg.word as usize];
        let tl = word.ctx[tg.phone as usize];
        let ul = self.bank.label[uid as usize];
        let mut pen: i32 = 0;
        if ul[2] != tl[2] {
            pen = if tl[2] == 0x4b || ul[2] == 0x4b {
                100
            } else if tb(T_COLLAPSE, ul[2]) != tb(T_COLLAPSE, tl[2]) {
                1000
            } else {
                5
            };
        }
        if ul[1] != tl[1] && tg.half != 2 {
            pen += if ul[5] & 0x80 == 0x80 && ul[5] & 0x38 >= 0x18 {
                1
            } else if tb(T_COLLAPSE, ul[1]) == tb(T_COLLAPSE, tl[1]) {
                5
            } else if tb(T_BROAD, ul[1]) == tb(T_BROAD, tl[1]) {
                10
            } else {
                500
            };
        }
        if ul[3] != tl[3] && tg.half != 1 {
            if self.final_type == 5 && tl[3] == 0x5a {
                pen += 100;
            } else if ul[5] & 0x40 == 0x40 && ul[5] & 7 >= 3 {
                pen += 1;
            } else if ul[3] == 0x4b {
                pen += 100;
            } else if tb(T_COLLAPSE, ul[3]) == tb(T_COLLAPSE, tl[3]) {
                if tl[3] != 0x5b {
                    pen += 5;
                }
            } else if tb(T_BROAD, ul[3]) == tb(T_BROAD, tl[3]) {
                pen += 10;
            } else {
                pen += 500;
            }
        }
        let urpos = ul[5] & 7;
        let ulpos = (ul[5] >> 3) & 7;
        pen += ti(0x1007c0d8, (((tl[5] >> 3) & 7) as usize) * 5 + ulpos as usize)
            + ti(0x1007c13c, urpos as usize + ((tl[5] & 7) as usize) * 8);
        let st = |c: u8| tb(T_STRESS, c) as usize;
        if st(tl[2]) == st(ul[2]) {
            if tb(T_VOWEL, tl[1]) != 0 && (tl[2] == 0x36 || tl[2] == 0x2b) {
                pen += ti(0x1007c23c, st(tl[1]) * 3 + st(ul[1]));
                if tl[1] != ul[1] {
                    pen += 100;
                }
            } else if urpos == 0 && tb(T_VOWEL, tl[3]) != 0 {
                pen += ti(0x1007c260, st(tl[3]) * 3 + st(ul[3]));
            }
        } else {
            pen += ti(0x1007c23c, st(tl[2]) * 3 + st(ul[2]));
        }
        let fdom = self.file_domain(uid);
        if fdom == 100 {
            pen += 10;
        }
        if fdom == 0x0c && word.domain != 0x0c {
            pen += 1000;
        }
        let dd = self.sd.get(feat[0], self.bank.dur_code[uid as usize]);
        let mut cont: f32;
        if tg.unvoiced == 0 {
            let (ka, kb) = match tg.half {
                1 => (0usize, 1usize),
                2 => (1, 2),
                _ => (0, 2),
            };
            let b = &self.bank.bounds[uid as usize];
            let fa = self.sd.get(feat[1], b[ka].feat_f);
            let fb = self.sd.get(feat[2], b[kb].feat_f);
            let mut s = (fb as f64 + fa as f64) as f32;
            let mut d = dd;
            if ulpos <= 1 && urpos <= 1 {
                s = (s as f64 / rf(0x1006d174) as f64) as f32;
            } else {
                s = (s as f64 / rf(0x1006d180) as f64) as f32;
                d = (d as f64 / rf(0x1006d180) as f64) as f32;
            }
            cont = (s as f64 + d as f64) as f32;
        } else {
            if ul[3] == tl[3] {
                pen /= 2;
            }
            cont = (dd as f64 / rf(0x1006d184) as f64) as f32;
        }
        if fdom == 0x0c && word.domain == 0x0c {
            cont = (cont as f64 / rf(0x1006d17c) as f64) as f32;
        }
        if self.x64 {
            // FUN_18001bc30 tail: mulss, then addss of the converted penalty.
            return cont * w + pen as f32;
        }
        (cont as f64 * w as f64 + pen as f32 as f64) as f32
    }

    /// `us_GetCandidates` (0x10023350).
    fn get_candidates(&mut self, t: usize, words: &mut [Word]) -> [u8; 3] {
        let feat = self.target_feat(t, words);
        let tg = self.targets[t];
        let wi = tg.word as usize;
        let mut n = 0usize;
        'outer: for ci in 0..self.slots[t].classes.len() {
            let c = self.slots[t].classes[ci];
            let cnt = self.cls.count[c as usize] as usize;
            let st = self.cls.start[c as usize] as usize;
            for j in 0..cnt {
                let u = self.cls.units[st + j] as i32;
                self.cm(n).id = u;
                n += 1;
                if n > 9999 {
                    break 'outer;
                }
            }
        }
        self.slots[t].n = n;
        self.score_contiguity(t);
        let nt = self.targets.len();
        let mut count: usize;
        let mut sort_from = 0usize;
        if self.c(0).run as i32 == nt as i32 {
            count = 0;
            while count < n && self.c(count).run as i32 >= nt as i32 {
                count += 1;
            }
        } else {
            if words[wi].domain == 0x0c {
                words[wi].domain = 0;
            }
            if n < 31 {
                count = n;
            } else {
                count = 1;
                while count < n {
                    if self.c(count - 1).score < self.c(count).score {
                        if count >= 30 {
                            break;
                        }
                        sort_from = count;
                    }
                    count += 1;
                }
            }
        }
        let mut n_pref = 0usize;
        if count > 30 {
            let nb = self.slots[t].n_pref_classes;
            let bl: Vec<i32> = self.slots[t].classes[..nb].to_vec();
            let units_b = self.sum_units(&bl);
            for i in 0..count {
                let u = self.c(i).id;
                let hit = bl.contains(&(self.cls.class_of[u as usize] as i32));
                self.cm(i).pref = hit as u8;
                n_pref += hit as usize;
            }
            if n_pref < 10 && units_b >= 10 {
                let mut thr = 0.0f32;
                let mut write = count;
                let mut idx = count;
                while idx < self.slots[t].n {
                    if n_pref >= 10 && self.c(idx).score != thr {
                        break;
                    }
                    let u = self.c(idx).id;
                    if bl.contains(&(self.cls.class_of[u as usize] as i32)) {
                        let src = *self.c(idx);
                        let d = self.cm(write);
                        d.id = src.id;
                        d.run = src.run;
                        d.fwd_run = src.fwd_run;
                        d.wrun = src.wrun;
                        d.pref = 1;
                        write += 1;
                        count += 1;
                        n_pref += 1;
                        if n_pref == 10 {
                            thr = src.score;
                        }
                    }
                    idx += 1;
                }
            }
        }
        let one = rf(0x1006d170);
        let two = rf(0x1006d174);
        for i in 0..count {
            let cd = *self.c(i);
            let mut r = ((cd.run as i32) as f32 as f64 / two as f64) as f32;
            if r <= one {
                r = one;
            }
            let w = (one as f64 / ((cd.wrun as i32) as f32 as f64 + r as f64)) as f32;
            let tc = self.target_cost(cd.id, t, &feat, w, words);
            self.cm(i).tc = tc;
        }
        let kept_n;
        if count < 31 {
            kept_n = count;
        } else {
            for i in sort_from..count {
                let cd = *self.c(i);
                let fd = self.file_domain(cd.id);
                let s = if fd == 100 {
                    (cd.tc as f64 * rf(0x1006d184) as f64) as f32
                } else if fd == 0x0c && words[wi].domain != 0x0c {
                    (cd.tc as f64 * rf(0x1006d1a4) as f64) as f32
                } else {
                    cd.tc
                };
                self.cm(i).score = s;
            }
            self.sort_by_score(sort_from, count - sort_from);
            kept_n = BEAM;
        }
        let mut kept: Vec<i32> = (0..kept_n).map(|i| self.c(i).id).collect();
        if kept_n < count {
            let mut p = (0..kept_n).filter(|&i| self.c(i).pref != 0).count();
            if p < 5 && p < n_pref {
                let mut e = kept_n as isize - 1;
                let mut i = kept_n;
                while i < count {
                    if self.c(i).pref != 0 {
                        while e > 0 && self.c(e as usize).pref == 1 {
                            e -= 1;
                        }
                        kept[e as usize] = self.c(i).id;
                        p += 1;
                        e -= 1;
                        if e < 1 || p >= 5 {
                            break;
                        }
                    }
                    i += 1;
                }
            }
        }
        self.slots[t].kept = kept;
        self.slots[t].n = kept_n;
        feat
    }

    // -------------------------------------------------------------- search

    /// `us_ViterbiJoinStep` (0x10018c80).
    fn join_step(&mut self, t: usize, words: &[Word]) {
        let cur = self.targets[t];
        let prev = self.targets[t - 1];
        let end_k: usize = if cur.half != 2 { 2 } else { 1 };
        let start_k: usize = (cur.half == 2) as usize;
        let jtype: usize = if cur.unvoiced == 0 && prev.unvoiced == 0 {
            if cur.word != prev.word { 4 } else { 0 }
        } else if cur.unvoiced == 0 {
            1
        } else if prev.unvoiced == 0 {
            2
        } else {
            3
        };
        let cur_c = words[cur.word as usize].ctx[cur.phone as usize][2];
        let prev_c = words[prev.word as usize].ctx[prev.phone as usize][2];
        let wc = tf(0x1007c288, jtype * 3);
        let wf = tf(0x1007c288, jtype * 3 + 1);
        let wb = tf(0x1007c288, jtype * 3 + 2);
        let two = rf(0x1006d174) as f64;
        let n_cur = self.slots[t].n;
        let n_prev = self.slots[t - 1].n;
        let mut acc = vec![0f32; n_cur];
        let mut back = vec![0i16; n_cur];
        let mut div: i32 = 1;
        for j in 0..n_cur {
            let u = self.slots[t].kept[j];
            let ul = self.bank.label[u as usize];
            let ub = self.bank.bounds[u as usize];
            let lpos = (ul[5] >> 3) & 7;
            let left_edge = ul[5] >> 7;
            let tcj = (self.c(j).tc as f64 / tf(0x1007c284, 0) as f64) as f32;
            for k in 0..n_prev {
                let p = self.slots[t - 1].kept[k];
                let pl = self.bank.label[p as usize];
                let pb = self.bank.bounds[p as usize];
                let right_edge = (pl[5] >> 6) & 1;
                let mut pen: i32 = 0;
                let join: f32;
                if (u == p && cur.half == 2) || (u - 1 == p && pl[6] & 0x80 != 0) {
                    join = 0.0;
                } else {
                    let cep = self.cep.get(ub[start_k].cep, pb[end_k].cep);
                    let db = self.sd.get(pb[end_k].feat_b, ub[start_k].feat_b);
                    let df = self.sd.get(pb[end_k].feat_f, ub[start_k].feat_f);
                    join = if self.x64 {
                        // FUN_18001c7a0: ((df*wf + cep*wc) + db*wb) + 2.0, all in f32.
                        df * wf + cep * wc + db * wb + two as f32
                    } else {
                        let x = df as f64 * wf as f64 + db as f64 * wb as f64;
                        let x = x + cep as f64 * wc as f64;
                        (x + two) as f32
                    };
                    if cur.half == 2 {
                        pen = if p == u { 0 } else { 5 };
                        if pl[2] != ul[2] {
                            pen += 1000;
                        }
                    } else {
                        pen = if right_edge != left_edge {
                            1000
                        } else if right_edge == 1 {
                            ti(0x1007bfd8, lpos as usize + (pl[5] & 7) as usize * 8)
                        } else {
                            0
                        };
                        let a = pl[2];
                        if a != ul[1] {
                            if tb(T_C384, a) != 0 || tb(T_C384, ul[1]) != 0 {
                                pen += 1000;
                            } else if tb(T_CTX_SUB, a) != 0 {
                                pen += 100;
                            }
                        }
                        let b = ul[2];
                        if pl[3] != b {
                            if tb(T_C384, pl[3]) != 0 || tb(T_C384, b) != 0 {
                                pen += 1000;
                            } else if tb(T_CTX_SUB, b) != 0 {
                                pen += 100;
                            }
                        }
                        if left_edge == 0
                            && ((tb(T_VOWEL, prev_c) != 0 && tb(T_C3E4, cur_c) != 0)
                                || (tb(T_C444, prev_c) != 0 && tb(T_VOWEL, cur_c) != 0))
                            && p != u - 1
                        {
                            pen += 10;
                        }
                    }
                    pen += ti(0x1007c544, pb[end_k].class as usize * 3 + ub[start_k].class as usize);
                    div = if cur.half == 2 || right_edge != 1 || left_edge != 1 {
                        1
                    } else if lpos < 3 {
                        (lpos == 2) as i32 + 1
                    } else {
                        8
                    };
                    let pd = self.file_domain(p) == 0x0c;
                    let ud = self.file_domain(u) == 0x0c;
                    if pd != ud {
                        pen += 2000;
                    }
                }
                if self.x64 {
                    self.scratch[k] = join / div as f32 + (tcj + self.slots[t - 1].acc[k]) + pen as f32;
                    continue;
                }
                let v = join as f64 / div as f32 as f64 + pen as f32 as f64;
                let v = v + self.slots[t - 1].acc[k] as f64;
                let v = v + tcj as f64;
                self.scratch[k] = v as f32;
            }
            let best = argmin(&self.scratch[..n_prev]);
            back[j] = best as i16;
            acc[j] = self.scratch[best];
        }
        self.slots[t].acc = acc;
        self.slots[t].back = back;
    }

    /// `us_BeamPrune` (0x10024510).
    fn beam_prune(&mut self, t: usize, words: &[Word]) {
        let tg = self.targets[t];
        let n = self.slots[t].n;
        if words[tg.word as usize].domain != 0 || n <= 10 {
            return;
        }
        let acc = self.slots[t].acc.clone();
        let mut sum = acc[0];
        let mut mn = acc[0];
        for &a in &acc[1..n] {
            sum = (sum as f64 + a as f64) as f32;
            if a < mn {
                mn = a;
            }
        }
        let mean = (sum as f64 / (n as i32) as f32 as f64) as f32;
        let spread = ((mean as f64 - mn as f64) / rf(0x1006d1a8) as f64) as f32;
        let thr = (1.0f32 as f64 * spread as f64 + mean as f64) as f32;
        let mut w = 0usize;
        for j in 0..n {
            if acc[j] <= thr || self.c(j).fwd_run != 0 {
                let s = &mut self.slots[t];
                s.acc[w] = acc[j];
                s.back[w] = s.back[j];
                s.kept[w] = s.kept[j];
                w += 1;
            }
        }
        let s = &mut self.slots[t];
        s.n = w;
        s.acc.truncate(w);
        s.back.truncate(w);
        s.kept.truncate(w);
    }

    /// `us_SelectUnitsForPhrase` (0x10024980). `words` is the whole sentence;
    /// the phrase covers `words[first..first+nw]`. `final_type` is the sentence
    /// type byte for the last phrase of a sentence and 5 otherwise.
    pub fn select_phrase(&mut self, words: &mut [Word], first: usize, nw: usize, final_type: u8) -> PhraseResult {
        self.final_type = final_type;
        if self.word_first_target.len() < words.len() {
            self.word_first_target.resize(words.len(), 0);
            self.word_ntargets.resize(words.len(), 0);
        }
        let mut res = PhraseResult::default();
        let n = self.build_targets(words, first, nw);
        if n == 0 {
            return res;
        }
        res.feats.push(self.get_candidates(0, words));
        res.cands.push((0..self.slots[0].n).map(|i| *self.c(i)).collect());
        res.kept_after_get.push(self.slots[0].kept.clone());
        let two = tf(0x1007c284, 0) as f64;
        let n0 = self.slots[0].n;
        self.slots[0].acc = (0..n0).map(|i| (self.c(i).tc as f64 / two) as f32).collect();
        self.slots[0].back = vec![0; n0];
        for t in 1..n {
            res.feats.push(self.get_candidates(t, words));
            res.cands.push((0..self.slots[t].n).map(|i| *self.c(i)).collect());
            res.kept_after_get.push(self.slots[t].kept.clone());
            self.join_step(t, words);
            self.beam_prune(t, words);
        }
        let mut path = vec![0i16; n];
        path[n - 1] = argmin(&self.slots[n - 1].acc) as i16;
        for t in (1..n).rev() {
            path[t - 1] = self.slots[t].back[path[t] as usize];
        }
        res.uids = (0..n).map(|t| self.slots[t].kept[path[t] as usize] as u32).collect();
        if let Some((cand_cnt, sel_cnt)) = self.history.as_mut() {
            for t in 0..n {
                for &u in &self.slots[t].kept {
                    cand_cnt[u as usize] += 1;
                }
                sel_cnt[res.uids[t] as usize] += 1;
            }
        }
        res.path = path;
        res.targets = self.targets.clone();
        res.slots = self.slots.clone();
        res
    }
}

/// `.his` file of a pool: `<M16>/mc_idx_tbl/unit-<pool>.his` (`"%s%s.his"`, 0x1007958c).
pub fn history_path(bank: &Bank, pool: usize) -> std::path::PathBuf {
    bank.root.join("mc_idx_tbl").join(format!("unit-{}.his", bank.pools[pool].name))
}

/// `us_LoadSelectHistory` / `FUN_1001b240`: `u32 N` then `N x {u32 candidate, u32 chosen}`.
/// A missing file yields zeros; a unit-count mismatch is an error, as in the engine.
pub fn load_history(bank: &Bank) -> std::io::Result<(Vec<u32>, Vec<u32>)> {
    let mut cand = vec![0u32; bank.len()];
    let mut chosen = vec![0u32; bank.len()];
    for (pi, p) in bank.pools.iter().enumerate() {
        let b = match std::fs::read(history_path(bank, pi)) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        let rd = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        if b.len() < 4 + 8 * p.n as usize || rd(0) != p.n {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "history unit count mismatch"));
        }
        for i in 0..p.n as usize {
            cand[p.base as usize + i] = rd(4 + 8 * i);
            chosen[p.base as usize + i] = rd(8 + 8 * i);
        }
    }
    Ok((cand, chosen))
}

/// `us_SaveSelectHistory` / `FUN_1001b350`.
pub fn save_history(bank: &Bank, hist: &(Vec<u32>, Vec<u32>)) -> std::io::Result<()> {
    for (pi, p) in bank.pools.iter().enumerate() {
        let mut b = Vec::with_capacity(4 + 8 * p.n as usize);
        b.extend_from_slice(&p.n.to_le_bytes());
        for i in 0..p.n as usize {
            b.extend_from_slice(&hist.0[p.base as usize + i].to_le_bytes());
            b.extend_from_slice(&hist.1[p.base as usize + i].to_le_bytes());
        }
        std::fs::write(history_path(bank, pi), b)?;
    }
    Ok(())
}

/// `FUN_10023010`: index of the first minimum.
fn argmin(v: &[f32]) -> usize {
    let mut best = 0usize;
    for i in 1..v.len() {
        if v[i] < v[best] {
            best = i;
        }
    }
    best
}
