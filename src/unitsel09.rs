//! Unit selection of the 2009 engines (julie09 `vt_eng_julie16.dll`), ported from its decompile
//! function by function. The 2009 builds select on 5-byte labels and 7-byte class keys, have no
//! preferred-candidate pass and no EXP-domain handling in the lookups, and use a different back-off
//! lattice, contiguity score, target cost and join cost than the 2014 engine in `unitsel.rs`.
//!
//! Function map (julie09 -> 2014 us_* in `unitsel.rs`):
//! `FUN_1001a760` us_SelectUnitsForPhrase, `FUN_10019ef0` us_BuildTargets, `FUN_10019a50`
//! us_LookupWholePhone, `FUN_10019bc0` us_LookupHalfPhones, `FUN_10019410` the context back-off
//! lattice, `FUN_10019380` / `FUN_100190b0` / `FUN_10019280` the class lookups, `FUN_10018fb0` /
//! `FUN_10019180` the class limiters, `FUN_10018ef0` us_ClassKeyDistance, `FUN_10018a20`
//! us_GetCandidates, `FUN_10018720` us_ScoreContiguity, `FUN_10018230` us_TargetCost, `FUN_1001a100`
//! the Viterbi join step, `FUN_10019d90` us_BeamPrune.

use super::classes::{class_key09, half_key09};
use super::{argmin, PhraseResult, Selector, Slot, Target, Word, BEAM};

const FOUR: f64 = 4.0;
const HALF: f64 = 0.5;
const FIFTY: f64 = 50.0;
const TWO: f64 = 2.0;
const FIVE: f32 = 5.0;
const TEN: f32 = 10.0;
const SIXTEEN: f64 = 16.0;
const TWOTHOUSAND: f64 = 2000.0;
const FIFTH: f32 = f32::from_bits(0x3e4ccccd);

impl<'a> Selector<'a> {
    fn t09(&self) -> super::classes::Tab09 {
        self.tab.expect("2009 selection tables")
    }

    /// `FUN_10018ef0`: weighted mismatch between two 7-byte class keys, plus the 9-byte half keys
    /// when `half` is 1 or 2.
    fn key_dist09(&self, a: &[u8; 7], b: &[u8; 7], half: u8) -> i32 {
        let t = self.t09();
        let mut d = 0;
        for i in 0..7 {
            if a[i] != b[i] {
                d += t.i(t.w_key, i);
            }
        }
        if half != 0 {
            let ha = half_key09(&t, a, half);
            let hb = half_key09(&t, b, half);
            for i in 0..t.hk_len() {
                if ha[i] != hb[i] {
                    d += t.i(t.w_half, i);
                }
            }
        }
        d
    }

    /// Ranks `src` classes by distance to `key` (`FUN_10018fb0` / `FUN_10019180` share this body) and
    /// returns the closest `maxc`.
    fn rank_classes09(&mut self, src: &[i32], maxc: usize, key: &[u8; 5], half: u8) -> Vec<i32> {
        let t = self.t09();
        let k = class_key09(&t, key);
        let mut n = 0usize;
        for &c in src {
            if n > 9999 {
                break;
            }
            let d = self.key_dist09(&k, &self.cls.key7[c as usize], half);
            let cm = self.cm(n);
            cm.id = c;
            cm.score = d as f32;
            n += 1;
        }
        self.sort_by_score(0, n);
        let n2 = n.min(maxc);
        (0..n2).map(|i| self.c(i).id).collect()
    }

    /// `FUN_10019180`: closest `maxc` classes of a half-key range.
    fn limit_half09(&mut self, maxc: usize, key: &[u8; 5], half: u8, range: &[i32]) -> Vec<i32> {
        if range.len() <= maxc {
            return range.to_vec();
        }
        self.rank_classes09(range, maxc, key, half)
    }

    /// `FUN_10018fb0`: closest 30 classes of the list buffer.
    fn limit_whole09(&mut self, key: &[u8; 5]) -> Vec<i32> {
        let src: Vec<i32> = self.lbuf[..self.lcount as usize].to_vec();
        if src.len() <= BEAM {
            return src;
        }
        self.rank_classes09(&src, BEAM, key, 0)
    }

    fn half_range09(&self, key: &[u8; 5], half: u8, plen: i32) -> Vec<i32> {
        let t = self.t09();
        let hk = half_key09(&t, &class_key09(&t, key), half);
        let (first, cnt) = self.cls.find_half_range(&hk, plen.max(0) as usize);
        let m = (hk[0] != 0) as usize;
        self.cls.half_order[m][first..first + cnt].iter().map(|&c| c as i32).collect()
    }

    /// `FUN_100190b0`: whole-phone class match, appended to the list buffer (cap 300). Returns the
    /// number of classes added.
    fn lookup_whole09(&mut self, key: &[u8; 5]) -> i16 {
        if self.lcount > 299 {
            return 0;
        }
        let t = self.t09();
        let k = class_key09(&t, key);
        if t.viol {
            // viol09 FUN_1001c130: matches on 7, 6 and 5 key bytes, each pass overwriting the list
            // from its start until a pass reaches 30 units.
            let base = self.lcount as usize;
            let mut plen = 7usize;
            let mut cnt: i32 = 0;
            let mut pos = base;
            loop {
                let (first, n) = self.cls.find_class_range7(&k, plen);
                let mut reset = pos;
                if n > 0 {
                    cnt = n as i32;
                    if self.lcount as i32 + cnt > 300 {
                        cnt = 300 - self.lcount as u16 as i32;
                    }
                    let sv = cnt as i16;
                    for i in 0..sv.max(0) as usize {
                        self.lbuf[pos] = first as i32 + i as i32;
                        pos += 1;
                        reset = base;
                    }
                    let got: Vec<i32> = self.lbuf[reset..reset + sv.max(0) as usize].to_vec();
                    if self.sum_units(&got) > 29 {
                        break;
                    }
                }
                plen -= 1;
                pos = reset;
                if plen <= 4 {
                    break;
                }
            }
            return cnt as i16;
        }
        let (first, n) = self.cls.find_class_range7(&k, 7);
        if n == 0 {
            return 0;
        }
        let mut cnt = n as i32;
        if self.lcount as i32 + cnt > 300 {
            cnt = 300 - self.lcount as u16 as i32;
        }
        let base = self.lcount as usize;
        for i in 0..cnt.max(0) as usize {
            self.lbuf[base + i] = first as i32 + i as i32;
        }
        cnt as i16
    }

    /// `FUN_10019280`: half-phone class match with key-prefix back-off, appended to the list buffer.
    /// Returns the number of classes added.
    fn lookup_half09(&mut self, key: &[u8; 5], half: u8, mode: i16) -> i16 {
        if self.t09().viol {
            return self.lookup_half_v09(key, half, mode);
        }
        let mut plen: i32 = 9;
        let mut range = self.half_range09(key, half, plen);
        let got;
        if mode != 0 {
            while range.is_empty() {
                plen -= 1;
                range = self.half_range09(key, half, plen);
            }
            let mut g = self.limit_half09(10, key, half, &range);
            let mut units = self.sum_units(&g);
            while units < 10 && plen > 3 {
                plen -= 1;
                range = self.half_range09(key, half, plen);
                g = self.limit_half09(10, key, half, &range);
                units = self.sum_units(&g);
            }
            got = g;
        } else {
            if range.is_empty() {
                return 0;
            }
            got = self.limit_half09(10, key, half, &range);
        }
        let base = self.lcount as usize;
        for (i, &c) in got.iter().enumerate() {
            self.lbuf[base + i] = c;
        }
        got.len() as i16
    }

    /// viol09 `FUN_1001c300`: the half lookup starts at 8 key bytes and stops widening at 3 units
    /// (or 4 bytes, or once a 5-digit right position has matched).
    fn lookup_half_v09(&mut self, key: &[u8; 5], half: u8, mode: i16) -> i16 {
        let mut plen: i32 = 8;
        let mut range = self.half_range09(key, half, plen);
        let got;
        if mode != 0 {
            while range.is_empty() {
                plen -= 1;
                range = self.half_range09(key, half, plen);
            }
            let mut g = self.limit_half09(10, key, half, &range);
            let mut units = self.sum_units(&g);
            while units < 3 && plen > 4 && (range.is_empty() || key[3] % 10 != 5) {
                plen -= 1;
                range = self.half_range09(key, half, plen);
                g = self.limit_half09(10, key, half, &range);
                units = self.sum_units(&g);
            }
            got = g;
        } else {
            if range.is_empty() {
                return 0;
            }
            got = self.limit_half09(10, key, half, &range);
        }
        let base = self.lcount as usize;
        for (i, &c) in got.iter().enumerate() {
            self.lbuf[base + i] = c;
        }
        got.len() as i16
    }

    /// `FUN_10019380`: one class lookup for target `t`; returns the unit count of the classes added.
    fn lookup09(&mut self, key: &[u8; 5], t: usize, mode: i16) -> i32 {
        let half = self.targets[t].half;
        let base = self.lcount as usize;
        let n = if half == 0 {
            self.lookup_whole09(key)
        } else {
            if self.lcount > 0x121 {
                return 0;
            }
            self.lookup_half09(key, half, mode)
        };
        self.lcount = self.lcount.wrapping_add(n);
        let added: Vec<i32> = self.lbuf[base..base + n.max(0) as usize].to_vec();
        self.sum_units(&added)
    }

    /// `FUN_10019410`: context back-off lattice over the boundary position digits of the label.
    fn lattice09(&mut self, key: &[u8; 5], t: usize, mode: i16, words: &[Word]) -> i32 {
        let tab = self.t09();
        if tab.viol {
            return self.lattice_v09(key, t, mode, words);
        }
        let saved = self.lcount;
        let tg = self.targets[t];
        let ec = words[tg.word as usize].edge_code[tg.phone as usize];
        let a = key[3] / 10;
        let b = key[3] % 10;
        let c = key[4] / 10;
        let d = key[4] % 10;
        let le = [2u8, 1, 1, 2];
        let nl: usize = if ec / 10 == 0 { ((c == 1 || c == 2) as usize) + 1 } else { 1 };
        let nr: usize = if ec % 10 == 0 { ((d == 1 || d == 2) as usize) + 1 } else { 1 };
        let mut total: i32 = 0;
        let mut found = false;
        for i in 0..nl {
            let mut cc = c;
            if nl > 1 {
                if c == 1 {
                    cc = le[i + 2];
                } else if c == 2 {
                    cc = le[i];
                }
            }
            let a2 = if a != 0 && a == c { cc } else { a };
            for j in 0..nr {
                let mut dd = d;
                if nr > 1 {
                    if d == 1 {
                        dd = le[j + 2];
                    } else if d == 2 {
                        dd = le[j];
                    }
                }
                let l6 = if b == 0 || b != d { b } else { dd };
                let mut k = *key;
                k[3] = a2.wrapping_mul(10).wrapping_add(l6);
                k[4] = cc.wrapping_mul(10).wrapping_add(dd);
                let r = self.lookup09(&k, t, mode) as i16 as i32;
                total += r;
                if r != 0 || key[3] != 0 {
                    let sc = tab.b(tab.edge_sub, key[1]);
                    if sc == 0 {
                        let sl = tab.b(tab.edge_sub, key[0]);
                        let sr = tab.b(tab.edge_sub, key[2]);
                        if sl == 0 {
                            if sr != 0 {
                                k[2] = sr;
                                total += self.lookup09(&k, t, 0) as i16 as i32;
                            }
                        } else if sr == 0 {
                            k[0] = sl;
                            total += self.lookup09(&k, t, 0) as i16 as i32;
                        } else {
                            k[0] = sl;
                            total += self.lookup09(&k, t, 0) as i16 as i32;
                            k[2] = sr;
                            total += self.lookup09(&k, t, 0) as i16 as i32;
                            k = *key;
                            k[2] = sr;
                            total += self.lookup09(&k, t, 0) as i16 as i32;
                        }
                    } else {
                        k[1] = sc;
                        total += self.lookup09(&k, t, 0) as i16 as i32;
                    }
                    if !found && i == 0 && j == 0 && total > 2 {
                        found = true;
                    }
                }
            }
        }
        if mode == 0 {
            return if found { total } else { 0 };
        }
        let mut t2 = total;
        let cs = tab.b(tab.ctr_sub, key[1]);
        if cs != 0 {
            let mut k = *key;
            k[1] = cs;
            t2 = self.lattice09(&k, t, 0, words) + total;
        }
        if t2 < 0x1e && c == 3 && a == 0 {
            let mut k = *key;
            k[4] = d;
            t2 += self.lattice09(&k, t, 0, words);
        }
        if d == 3 {
            let mut k = *key;
            if k[3] % 10 == 3 {
                k[3] = (k[3] / 10) * 10 + 4;
            }
            k[4] = (k[4] / 10) * 10 + 4;
            if k[2] == 0x5b {
                k[2] = 0x5a;
            }
            t2 += self.lattice09(&k, t, 0, words);
            if c == 3 && a == 0 {
                k[4] %= 10;
                t2 += self.lattice09(&k, t, 0, words);
            }
        }
        if self.final_type == 6 && d == 4 {
            let mut k = *key;
            if k[3] % 10 == 4 {
                k[3] = (k[3] / 10) * 10 + 6;
            }
            k[4] = (k[4] / 10) * 10 + 6;
            if k[2] == 0x5a {
                k[2] = 0x60;
            }
            t2 += self.lattice09(&k, t, 0, words);
            if c == 3 && a == 0 {
                k[4] %= 10;
                t2 += self.lattice09(&k, t, 0, words);
            }
        }
        if !found && t2 == total {
            self.lcount = saved;
            return 0;
        }
        t2
    }

    /// viol09 `FUN_1001c4c0`: the Violeta lattice relaxes the two position digit pairs only; no edge
    /// or centre substitutions.
    fn lattice_v09(&mut self, key: &[u8; 5], t: usize, mode: i16, words: &[Word]) -> i32 {
        let saved = self.lcount;
        let tg = self.targets[t];
        let ec = words[tg.word as usize].edge_code[tg.phone as usize];
        let a = key[3] / 10;
        let b = key[3] % 10;
        let c = key[4] / 10;
        let d = key[4] % 10;
        let le = [1u8, 2];
        let nl: usize = if ec / 10 == 0 { ((a == 1 || a == 2) as usize) + 1 } else { 1 };
        let nr: usize = if ec % 10 == 0 { ((b == 1 || b == 2) as usize) + 1 } else { 1 };
        let mut total: i32 = 0;
        for i in 0..nl {
            let mut cc = c;
            if nl > 1 && (c == 1 || c == 2) {
                cc = le[i];
            }
            let a2 = if a != 0 && a == c { cc } else { a };
            for j in 0..nr {
                let mut dd = d;
                if nr > 1 && (d == 1 || d == 2) {
                    dd = le[j];
                }
                let b2 = if b != 0 && b == d { dd } else { b };
                let mut k = *key;
                k[3] = a2.wrapping_mul(10).wrapping_add(b2);
                k[4] = cc.wrapping_mul(10).wrapping_add(dd);
                total += self.lookup09(&k, t, mode) as i16 as i32;
            }
        }
        let found = total > 0;
        if mode == 0 {
            return if found { total } else { 0 };
        }
        let mut t2 = total;
        if total < 0x1e && c == 3 && a == 0 {
            let mut k = *key;
            k[4] = d;
            t2 = self.lattice_v09(&k, t, 0, words) + total;
        }
        if d == 3 {
            let mut k = *key;
            if k[3] % 10 == 3 {
                k[3] = (k[3] / 10) * 10 + 4;
            }
            k[4] = (k[4] / 10) * 10 + 4;
            t2 += self.lattice_v09(&k, t, 0, words);
            if c == 3 && a == 0 {
                k[4] %= 10;
                t2 += self.lattice_v09(&k, t, 0, words);
            }
        }
        if !found && t2 == total {
            self.lcount = saved;
            return 0;
        }
        t2
    }

    /// Sorts and de-duplicates the list buffer in place exactly as the two lookups do (a count of 0
    /// yields a count of 1 over whatever the buffer held).
    fn sort_dedupe09(&mut self) -> i16 {
        let cnt = self.lcount as usize;
        if cnt > 0 {
            self.lbuf[..cnt].sort_unstable();
        }
        let mut m = 1usize;
        let mut i = 1usize;
        while (i as i16) < self.lcount {
            if self.lbuf[i] != self.lbuf[i - 1] {
                if m != i {
                    self.lbuf[m] = self.lbuf[i];
                }
                m += 1;
            }
            i += 1;
        }
        self.lcount = m as i16;
        m as i16
    }

    /// `FUN_10019a50` (us_LookupWholePhone): 1 when the whole phone has more than 9 candidate units.
    fn lookup_whole_phone09(&mut self, t: usize, words: &[Word]) -> i32 {
        let tg = self.targets[t];
        let wi = tg.word as usize;
        self.targets[t].idx = self.word_ntargets[wi] as u8;
        self.targets[t].half = 0;
        let key = words[wi].ctx5[tg.phone as usize];
        self.lcount = 0;
        self.lattice09(&key, t, 1, words);
        if self.lcount == 0 {
            return 0;
        }
        let m = self.sort_dedupe09();
        let classes: Vec<i32> = if m < 0x1f {
            self.lbuf[..m as usize].to_vec()
        } else {
            let mut g = self.limit_whole09(&key);
            g.sort_unstable();
            g
        };
        let units = self.sum_units(&classes);
        self.slots[t].classes = classes;
        self.slots[t].n_pref_classes = 0;
        (units >= if self.t09().viol { 3 } else { 10 }) as i32
    }

    /// `FUN_10019bc0` (us_LookupHalfPhones): always yields two targets.
    fn lookup_half_phones09(&mut self, t: usize, words: &[Word]) -> i32 {
        let base = self.targets[t];
        let wi = base.word as usize;
        let key = words[wi].ctx5[base.phone as usize];
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
            self.lcount = 0;
            self.lattice09(&key, t + h, 1, words);
            let m = self.sort_dedupe09();
            let classes: Vec<i32> = if m < 0x1f {
                self.lbuf[..m as usize].to_vec()
            } else {
                let src: Vec<i32> = self.lbuf[..m as usize].to_vec();
                let half = self.targets[t + h].half;
                let mut g = self.limit_half09(BEAM, &key, half, &src);
                g.sort_unstable();
                g
            };
            self.slots[t + h].classes = classes;
            self.slots[t + h].n_pref_classes = 0;
        }
        2
    }

    /// `FUN_10019ef0` (us_BuildTargets).
    fn build_targets09(&mut self, words: &[Word], first: usize, nw: usize) -> usize {
        let t = self.t09();
        let mut n = 0usize;
        self.targets.clear();
        self.slots.clear();
        for wi in first..first + nw {
            self.word_ntargets[wi] = 0;
            self.word_first_target[wi] = n;
            for k in 0..words[wi].ctx5.len() {
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
                    unvoiced: (t.b(t.voiced, w.ctx5[k][1]) == 0) as u8,
                };
                if w.dur[k] > 0 {
                    let mut r = self.lookup_whole_phone09(n, words);
                    if r < 1 {
                        r = self.lookup_half_phones09(n, words);
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

    /// `FUN_100189e0`: duration quantised to a byte (`dur / 50 + 0.5`, truncated, clamped).
    fn quantize_dur09(d: i16) -> u8 {
        let v = ((d as f32) as f64 / FIFTY + HALF).trunc() as i32 as i16;
        if v < 0 {
            0
        } else if v >= 0x100 {
            0xff
        } else {
            v as u8
        }
    }

    /// The 3-byte target feature `{dur_code, f0_a, f0_b}` (start of `FUN_10018a20`). The duration
    /// code uses the whole phone's duration; the contour position halves the duration of every
    /// target of the syllable when the current target is a half.
    fn target_feat09(&self, t: usize, words: &[Word]) -> [u8; 3] {
        let tg = self.targets[t];
        let w = &words[tg.word as usize];
        let dc = Self::quantize_dur09(w.dur[tg.phone as usize]);
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
                    let mut d = w.dur[r.phone as usize] as i32;
                    if tg.half != 0 {
                        d /= 2;
                    }
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

    fn flag_at(&self, u: i64) -> u8 {
        if u < 0 { 0 } else { self.bank.flag.get(u as usize).copied().unwrap_or(0) }
    }

    /// `FUN_10018720` (us_ScoreContiguity): runs of consecutive units through neighbouring targets'
    /// kept units / class lists; score `-(2 run [+1] + 100 wrun)`, then sort ascending.
    fn score_contiguity09(&mut self, t: usize) {
        let n = self.slots[t].n;
        let nt = self.targets.len();
        let wts = [2i32, 1, 1];
        for c in 0..n {
            let u = self.c(c).id as i64;
            let (mut back, mut wback) = (0i32, 0i32);
            let (mut fwd, mut wfwd) = (0i32, 0i32);
            let mut cur = u;
            let mut k = t as isize - 1;
            while k >= 0 {
                let kt = self.targets[k as usize];
                if kt.half != 1 {
                    if cur == 0 || self.flag_at(cur - 1) == 0 {
                        break;
                    }
                    cur -= 1;
                }
                let s = &self.slots[k as usize];
                if !s.kept[..s.n].contains(&(cur as i32)) {
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
                    if self.flag_at(cur) == 0 {
                        break;
                    }
                    cur += 1;
                }
                let cl = match self.cls.class_of.get(cur as usize) {
                    Some(&c) => c as i32,
                    None => break,
                };
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
            let mut s2 = run as i32 * 2;
            let h = self.targets[t].half;
            if (h == 1 && fwd > 0) || (h == 2 && back > 0) {
                s2 += 1;
            }
            let cm = self.cm(c);
            cm.run = run;
            cm.fwd_run = fwd as i16;
            cm.wrun = wrun;
            cm.score = -((s2 + wrun as i32 * 100) as f32);
        }
        self.sort_by_score(0, n);
    }

    /// `FUN_10018230` (us_TargetCost).
    fn target_cost09(&self, uid: i32, t: usize, feat: &[u8; 3], w: f32, words: &[Word]) -> f32 {
        let tab = self.t09();
        if tab.viol {
            return self.target_cost_v09(uid, t, feat, w, words);
        }
        let tg = self.targets[t];
        let word = &words[tg.word as usize];
        let tl = word.ctx5[tg.phone as usize];
        let ul = self.bank.label5[uid as usize];
        let cl = |c: u8| tab.b(tab.collapse, c);
        let mut pen: i32 = 0;
        if ul[1] != tl[1] {
            pen = if tl[1] == 0x4b || ul[1] == 0x4b {
                100
            } else if cl(ul[1]) != cl(tl[1]) {
                1000
            } else {
                5
            };
        }
        if ul[0] != tl[0] && tg.half != 2 {
            pen += if cl(ul[0]) == cl(tl[0]) {
                5
            } else if tab.b(tab.key_l, ul[0]) == tab.b(tab.key_l, tl[0]) {
                20
            } else {
                50
            };
        }
        if ul[2] != tl[2] && tg.half != 1 {
            if self.final_type == 4 && tl[2] == 0x5a {
                pen += 100;
            } else if ul[2] == 0x4b {
                pen += 100;
            } else if cl(ul[2]) == cl(tl[2]) {
                if tl[2] != 0x5b {
                    pen += 5;
                }
            } else if tab.b(tab.key_r, ul[2]) == tab.b(tab.key_r, tl[2]) {
                pen += 20;
            } else {
                pen += 50;
            }
        }
        let t_hi = (tl[3] / 10) as i16;
        let t_lo = (tl[3] % 10) as i16;
        let u_hi = (ul[3] / 10) as i16;
        let mut u_lo = (ul[3] % 10) as i16;
        if u_lo == 6 {
            u_lo = 4;
        }
        let prev_flag = if tg.word > 0 { words[tg.word as usize - 1].flag } else { 0 };
        let mut p = if t_hi < 4
            && u_hi < 4
            && (tg.word == 0 || prev_flag != 1 || t_hi == 0 || t_hi == u_hi)
        {
            tab.i(tab.pos_a, (u_hi + t_hi * 4) as usize)
        } else {
            1000
        };
        if t_lo > 5 || u_lo > 5 || (word.flag == 1 && t_lo != 0 && t_lo != u_lo) {
            p += 1000;
        }
        let t_d = (tl[4] % 10) as i16;
        let mut u_d = (ul[4] % 10) as i16;
        if u_d == 6 {
            u_d = 4;
        }
        if t_d < 6 && u_d < 6 {
            pen += p + tab.i(tab.pos_b, (u_d + t_d * 6) as usize);
        } else {
            pen += p + 1000;
        }
        let st = |c: u8| tab.b(tab.stress, c) as usize;
        if st(tl[1]) == st(ul[1]) {
            if tab.b(tab.vowel, tl[0]) == 0 || (tl[1] != 0x36 && tl[1] != 0x2b) {
                if t_lo == 0 && tab.b(tab.vowel, tl[2]) != 0 {
                    pen += tab.i(tab.st_b, st(tl[2]) * 3 + st(ul[2]));
                }
            } else {
                pen += tab.i(tab.st_a, st(tl[0]) * 3 + st(ul[0]));
            }
        } else {
            pen += tab.i(tab.st_a, st(tl[1]) * 3 + st(ul[1]));
        }
        let fdom = self.file_domain(uid);
        if fdom == 100 {
            pen += 20;
        }
        if fdom == 0x0c {
            if self.final_type == 4 {
                pen += 100;
            }
        } else if self.final_type != 4 {
            pen += 10;
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
            let s = (fb as f64 + fa as f64) as f32;
            cont = (s as f64 + dd as f64) as f32;
        } else {
            if ul[2] == tl[2] {
                pen /= 2;
            }
            cont = (dd as f64 * FIFTH as f64) as f32;
        }
        if fdom == 0x0c && self.final_type == 6 {
            cont = (cont as f64 / SIXTEEN) as f32;
        }
        (cont as f64 * w as f64 + pen as f32 as f64) as f32
    }

    /// viol09 `FUN_1001b320` (us_TargetCost): no 0x4b / sentence-type / EXP-domain terms, a second
    /// position penalty over the label's `b` digits, and the stress terms of the Violeta tables.
    fn target_cost_v09(&self, uid: i32, t: usize, feat: &[u8; 3], w: f32, words: &[Word]) -> f32 {
        let tab = self.t09();
        let tg = self.targets[t];
        let word = &words[tg.word as usize];
        let tl = word.ctx5[tg.phone as usize];
        let ul = self.bank.label5[uid as usize];
        let cl = |c: u8| tab.b(tab.collapse, c);
        let mut pen: i32 = 0;
        if ul[1] != tl[1] {
            pen = if cl(ul[1]) != cl(tl[1]) { 1000 } else { 5 };
        }
        if ul[0] != tl[0] && tg.half != 2 {
            pen += if tab.b(tab.key_l, ul[0]) == tab.b(tab.key_l, tl[0]) {
                2
            } else if cl(ul[0]) == cl(tl[0]) {
                5
            } else {
                10
            };
        }
        if ul[2] != tl[2] && tg.half != 1 {
            pen += if tl[2] == 0x46 {
                100
            } else if tab.b(tab.key_r, ul[2]) == tab.b(tab.key_r, tl[2]) {
                2
            } else if cl(ul[2]) == cl(tl[2]) {
                5
            } else {
                10
            };
        }
        let prev_flag = if tg.word > 0 { words[tg.word as usize - 1].flag } else { 0 };
        let pos_a = |t_hi: i16, u_hi: i16| -> i32 {
            if t_hi <= 3 && u_hi <= 3 { tab.i(tab.pos_a, (u_hi + t_hi * 4) as usize) } else { 1000 }
        };
        let pos_b = |t_lo: i16, u_lo: i16| -> i32 {
            if t_lo <= 5 && u_lo <= 5 { tab.i(tab.pos_b, (u_lo + t_lo * 6) as usize) } else { 1000 }
        };
        let (t_hi, t_lo) = ((tl[3] / 10) as i16, (tl[3] % 10) as i16);
        let (u_hi, u_lo) = ((ul[3] / 10) as i16, (ul[3] % 10) as i16);
        if t_hi <= 3 && u_hi <= 3 && (tg.word == 0 || prev_flag != 1 || t_hi == 0 || t_hi == u_hi) {
            pen += tab.i(tab.pos_a, (u_hi + t_hi * 4) as usize);
        } else {
            pen += 1000;
        }
        if t_lo <= 5 && u_lo <= 5 && (word.flag != 1 || t_lo == 0 || t_lo == u_lo) {
            pen += tab.i(tab.pos_b, (u_lo + t_lo * 6) as usize);
        } else {
            pen += 1000;
        }
        pen += pos_a((tl[4] / 10) as i16, (ul[4] / 10) as i16);
        pen += pos_b((tl[4] % 10) as i16, (ul[4] % 10) as i16);
        let st = |c: u8| tab.b(tab.stress, c) as usize;
        if st(tl[1]) != st(ul[1]) {
            pen += tab.i(tab.st_a, st(tl[1]) * 3 + st(ul[1]));
        } else if tab.b(tab.vowel, tl[2]) != 0 {
            pen += tab.i(tab.st_b, st(tl[2]) * 3 + st(ul[2]));
        }
        if self.file_domain(uid) == 100 {
            pen += 10;
        }
        let dd = self.sd.get(feat[0], self.bank.dur_code[uid as usize]);
        let cont: f32;
        if tg.unvoiced == 0 {
            let (ka, kb) = match tg.half {
                1 => (0usize, 1usize),
                2 => (1, 2),
                _ => (0, 2),
            };
            let b = &self.bank.bounds[uid as usize];
            let fa = self.sd.get(feat[1], b[ka].feat_f);
            let fb = self.sd.get(feat[2], b[kb].feat_f);
            let s = (fb as f64 + fa as f64) as f32;
            cont = (s as f64 + dd as f64) as f32;
        } else {
            if ul[2] == tl[2] {
                pen /= 2;
            }
            cont = (dd as f64 * FIFTH as f64) as f32;
        }
        (cont as f64 * w as f64 + pen as f32 as f64) as f32
    }

    /// viol09 `FUN_1001cf90`: the join step without the EXP-domain and class-pair terms.
    fn join_step_v09(&mut self, t: usize, words: &[Word]) {
        let tab = self.t09();
        let cur = self.targets[t];
        let prev = self.targets[t - 1];
        let end_k: usize = if cur.half != 2 { 2 } else { 1 };
        let start_k: usize = (cur.half == 2) as usize;
        let jtype: usize = if cur.unvoiced == 0 {
            if prev.unvoiced == 0 {
                if cur.word == prev.word || words[prev.word as usize].brk != 0x48 { 0 } else { 4 }
            } else {
                1
            }
        } else if prev.unvoiced == 0 {
            2
        } else {
            3
        };
        let cur_c = words[cur.word as usize].ctx5[cur.phone as usize][1];
        let prev_c = words[prev.word as usize].ctx5[prev.phone as usize][1];
        let wc = tab.f(tab.join_w, jtype * 3);
        let wf = tab.f(tab.join_w, jtype * 3 + 1);
        let wb = tab.f(tab.join_w, jtype * 3 + 2);
        let tc_mul = tab.f(tab.tc_div, 0) as f64;
        let n_cur = self.slots[t].n;
        let n_prev = self.slots[t - 1].n;
        let mut acc = vec![0f32; n_cur];
        let mut back = vec![0i16; n_cur];
        for j in 0..n_cur {
            let u = self.slots[t].kept[j];
            let ul = self.bank.label5[u as usize];
            let ub = self.bank.bounds[u as usize];
            let tc2 = (tc_mul * self.c(j).tc as f64) as f32;
            for k in 0..n_prev {
                let p = self.slots[t - 1].kept[k];
                let pl = self.bank.label5[p as usize];
                let pb = self.bank.bounds[p as usize];
                let join: f32;
                if (u == p && cur.half == 2) || (u - 1 == p && self.bank.flag[p as usize] == 1) {
                    join = 0.0;
                } else {
                    let cep = self.cep.get(ub[start_k].cep, pb[end_k].cep);
                    let db = self.sd.get(pb[end_k].feat_b, ub[start_k].feat_b);
                    let df = self.sd.get(pb[end_k].feat_f, ub[start_k].feat_f);
                    let x = db as f64 * wb as f64 + df as f64 * wf as f64;
                    let x = x + cep as f64 * wc as f64;
                    join = (x + TWO) as f32;
                }
                let mut pen: i32 = 0;
                if cur.half == 2 {
                    if tab.b(tab.vowel, prev_c) != 0 && p != u {
                        pen = 10;
                    }
                    if pl[1] != ul[1] {
                        pen += 1000;
                    }
                } else {
                    pen = tab.i(tab.pos_j, (ul[3] / 10) as usize + (pl[3] % 10) as usize * 6);
                    if ul[3] / 10 == 0 && tab.b(tab.vowel, prev_c) != 0 && cur_c == 0x1b && p != u - 1 {
                        pen += 10;
                    }
                }
                let j2 = (pen as f32 as f64 + join as f64) as f32;
                let v = j2 as f64 + tc2 as f64;
                let v = v + self.slots[t - 1].acc[k] as f64;
                self.scratch[k] = v as f32;
            }
            let best = argmin(&self.scratch[..n_prev]);
            back[j] = best as i16;
            acc[j] = self.scratch[best];
        }
        self.slots[t].acc = acc;
        self.slots[t].back = back;
    }

    /// `FUN_10018a20` (us_GetCandidates).
    fn get_candidates09(&mut self, t: usize, words: &[Word]) -> [u8; 3] {
        let feat = self.target_feat09(t, words);
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
        self.score_contiguity09(t);
        let nt = self.targets.len();
        let mut count: usize;
        let mut sort_from = 0usize;
        let c0 = *self.c(0);
        let viol = self.t09().viol;
        if c0.run as i32 == nt as i32 && (viol || self.file_domain(c0.id) == 0x0c || self.final_type == 4) {
            count = 0;
            while count < n && self.c(count).run as i32 >= nt as i32 {
                count += 1;
            }
        } else if n < 31 {
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
        let one = 1.0f64;
        for i in 0..count {
            let cd = *self.c(i);
            let mut r = ((cd.run as i32) as f32 as f64 / TWO) as f32;
            if r <= 1.0 {
                r = 1.0;
            }
            let w = (one / ((cd.wrun as i32) as f32 as f64 + r as f64)) as f32;
            let tc = self.target_cost09(cd.id, t, &feat, w, words);
            self.cm(i).tc = tc;
        }
        let kept_n;
        if count > 30 {
            for i in sort_from..count {
                let cd = *self.c(i);
                let fd = self.file_domain(cd.id);
                let s = if fd == 100 {
                    (cd.tc as f64 * FIVE as f64) as f32
                } else if !viol && fd == 0x0c && self.final_type == 4 {
                    (cd.tc as f64 * TEN as f64) as f32
                } else {
                    cd.tc
                };
                self.cm(i).score = s;
            }
            self.sort_by_score(sort_from, count - sort_from);
            kept_n = BEAM;
        } else {
            kept_n = count;
        }
        self.slots[t].kept = (0..kept_n).map(|i| self.c(i).id).collect();
        self.slots[t].n = kept_n;
        feat
    }

    /// `FUN_1001a100` (Viterbi join step): best predecessor and accumulated cost of every kept
    /// candidate of target `t`.
    fn join_step09(&mut self, t: usize, words: &[Word]) {
        let tab = self.t09();
        if tab.viol {
            return self.join_step_v09(t, words);
        }
        let cur = self.targets[t];
        let prev = self.targets[t - 1];
        let end_k: usize = if cur.half != 2 { 2 } else { 1 };
        let start_k: usize = (cur.half == 2) as usize;
        let jtype: usize = if cur.unvoiced == 0 {
            if prev.unvoiced == 0 {
                if cur.word == prev.word || words[prev.word as usize].brk != 0x5c { 0 } else { 4 }
            } else {
                1
            }
        } else if prev.unvoiced == 0 {
            2
        } else {
            3
        };
        let cur_c = words[cur.word as usize].ctx5[cur.phone as usize][1];
        let prev_c = words[prev.word as usize].ctx5[prev.phone as usize][1];
        let wc = tab.f(tab.join_w, jtype * 3);
        let wf = tab.f(tab.join_w, jtype * 3 + 1);
        let wb = tab.f(tab.join_w, jtype * 3 + 2);
        let tc_div = tab.f(tab.tc_div, 0) as f64;
        let n_cur = self.slots[t].n;
        let n_prev = self.slots[t - 1].n;
        let mut acc = vec![0f32; n_cur];
        let mut back = vec![0i16; n_cur];
        for j in 0..n_cur {
            let u = self.slots[t].kept[j];
            let ul = self.bank.label5[u as usize];
            let ub = self.bank.bounds[u as usize];
            let tc2 = (self.c(j).tc as f64 / tc_div) as f32;
            for k in 0..n_prev {
                let p = self.slots[t - 1].kept[k];
                let pl = self.bank.label5[p as usize];
                let pb = self.bank.bounds[p as usize];
                let join: f32;
                if (u == p && cur.half == 2) || (u - 1 == p && self.bank.flag[p as usize] == 1) {
                    join = 0.0;
                } else {
                    let cep = self.cep.get(ub[start_k].cep, pb[end_k].cep);
                    let db = self.sd.get(pb[end_k].feat_b, ub[start_k].feat_b);
                    let df = self.sd.get(pb[end_k].feat_f, ub[start_k].feat_f);
                    let x = db as f64 * wb as f64 + df as f64 * wf as f64;
                    let x = x + cep as f64 * wc as f64;
                    join = (x + TWO) as f32;
                }
                let mut pen: i32 = 0;
                if cur.half == 2 {
                    if tab.b(tab.vowel, prev_c) != 0 && p != u {
                        pen = 20;
                    }
                    if pl[3] != ul[3] {
                        pen += 10;
                    }
                    if pl[1] != ul[1] {
                        pen += 1000;
                    }
                } else {
                    pen = tab.i(tab.pos_j, (ul[3] / 10) as usize + (pl[3] % 10) as usize * 6);
                    if pl[1] != ul[0] {
                        if tab.b(tab.j_flag, pl[1]) == 0 && tab.b(tab.j_flag, ul[0]) == 0 {
                            if tab.b(tab.edge_sub, pl[1]) != 0 {
                                pen += 100;
                            }
                        } else {
                            pen += 1000;
                        }
                    }
                    if pl[2] != ul[1] {
                        if tab.b(tab.j_flag, pl[2]) == 0 && tab.b(tab.j_flag, ul[1]) == 0 {
                            if tab.b(tab.edge_sub, ul[1]) != 0 {
                                pen += 100;
                            }
                        } else {
                            pen += 1000;
                        }
                    }
                    if ul[3] / 10 == 0
                        && ((tab.b(tab.vowel, prev_c) != 0 && tab.b(tab.j_prev, cur_c) != 0)
                            || (tab.b(tab.j_next, prev_c) != 0 && tab.b(tab.vowel, cur_c) != 0))
                        && p != u - 1
                    {
                        pen += 10;
                    }
                }
                let mut j2 = (pen as f32 as f64 + join as f64) as f32;
                if self.file_domain(p) == 0x0c && self.file_domain(u) != 0x0c {
                    j2 = (j2 as f64 + TWOTHOUSAND) as f32;
                }
                let v = j2 as f64 + tc2 as f64;
                let v = v + self.slots[t - 1].acc[k] as f64;
                self.scratch[k] = v as f32;
            }
            let best = argmin(&self.scratch[..n_prev]);
            back[j] = best as i16;
            acc[j] = self.scratch[best];
        }
        self.slots[t].acc = acc;
        self.slots[t].back = back;
    }

    /// `FUN_10019d90` (us_BeamPrune): drops candidates above `mean + (mean - min) / 4` unless they
    /// continue a unit run.
    fn beam_prune09(&mut self, t: usize) {
        let n = self.slots[t].n;
        if n <= 10 {
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
        let spread = ((mean as f64 - mn as f64) / FOUR) as f32;
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

    /// `FUN_1001a760` (us_SelectUnitsForPhrase). `final_type` is the sentence type byte (4 / 5 / 6)
    /// for the last phrase of a sentence and 4 otherwise.
    pub(super) fn select_phrase09(&mut self, words: &mut [Word], first: usize, nw: usize, final_type: u8) -> PhraseResult {
        self.final_type = final_type;
        if self.word_first_target.len() < words.len() {
            self.word_first_target.resize(words.len(), 0);
            self.word_ntargets.resize(words.len(), 0);
        }
        let tc_div = self.t09().f(self.t09().tc_div, 0) as f64;
        let mut res = PhraseResult::default();
        let n = self.build_targets09(words, first, nw);
        if n == 0 {
            return res;
        }
        res.feats.push(self.get_candidates09(0, words));
        res.cands.push((0..self.slots[0].n).map(|i| *self.c(i)).collect());
        res.kept_after_get.push(self.slots[0].kept.clone());
        let n0 = self.slots[0].n;
        let viol = self.t09().viol;
        self.slots[0].acc = (0..n0).map(|i| if viol { (tc_div * self.c(i).tc as f64) as f32 } else { (self.c(i).tc as f64 / tc_div) as f32 }).collect();
        self.slots[0].back = vec![0; n0];
        for t in 1..n {
            res.feats.push(self.get_candidates09(t, words));
            res.cands.push((0..self.slots[t].n).map(|i| *self.c(i)).collect());
            res.kept_after_get.push(self.slots[t].kept.clone());
            self.join_step09(t, words);
            self.beam_prune09(t);
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
