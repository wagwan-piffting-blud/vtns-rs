//! Phone contexts: allophone rewriting `FUN_10017100` (flaps, glottal and
//! unreleased stops, palatalisation before "you"), then
//! `fe_BuildPhoneContexts` (0x10017510): the 7-byte quinphone label
//! `{LL, L, C, R, RR, posbits, attr}` per phone, and the stress-distance
//! counters at word +0x25d.
//!
//! 2009: the allophone pass is julie09 `FUN_10016ac0` (same body, own tables and layout); the context builder
//! `FUN_10016ed0` is a different, older algorithm with a 5-byte label `{L, C, R, flags, q}` and decimal-coded
//! boundary flags, ported as `build_phone_contexts09`.

use super::lay::{Lay, L14};
use super::phtabs::{PhTabs, T14};
use super::{sc, wr, Ss, WordStrings};
use crate::era::Era;
use crate::lexicon::stricmp;

const T: u8 = 0x39;
const D: u8 = 0x15;
const K: u8 = 0x2a;
const P: u8 = 0x35;
const R: u8 = 0x36;
const S: u8 = 0x37;
const SH: u8 = 0x38;
const Y: u8 = 0x43;

/// `FUN_10017030`: stress of the vowel at phone k (or of the vowel before an
/// R at k), -1 otherwise.
fn stress_at(lay: &Lay, t: &PhTabs, ss: &Ss, w: usize, k: i32) -> i16 {
    if k >= 0 {
        let c = ss.u8(w + lay.codes + k as usize);
        if sc(t.vowel, c) == 1 {
            return sc(t.stress, c) as i16;
        }
        if c == R && k > 0 {
            let c2 = ss.u8(w + lay.codes + k as usize - 1);
            if sc(t.vowel, c2) == 1 {
                return sc(t.stress, c2) as i16;
            }
        }
    }
    -1
}

/// `FUN_10017090`: vowel class of phone k (or of the vowel after a Y at k), -1 otherwise.
fn vowel_class_at(lay: &Lay, t: &PhTabs, ss: &Ss, w: usize, k: i32) -> i16 {
    let n = ss.u8(w + lay.n_phones) as i32;
    if k < n {
        let c = ss.u8((w as isize + lay.codes as isize + k as isize) as usize);
        if sc(t.vowel, c) == 1 {
            return sc(t.bb70, c) as i8 as i16;
        }
        if c == Y && k < n - 1 {
            let c2 = ss.u8(w + lay.codes + k as usize + 1);
            if sc(t.vowel, c2) == 1 {
                return sc(t.bb70, c2) as i8 as i16;
            }
        }
    }
    -1
}

fn in_sorted_ci(list: &[&[u8]], key: &[u8]) -> bool {
    // FUN_100560a0 binary search, case-insensitive compare.
    if key.is_empty() {
        return false;
    }
    let (mut lo, mut hi) = (0i32, list.len() as i32 - 1);
    let mut mid = hi;
    while lo <= hi {
        mid /= 2;
        let c = stricmp(list[mid as usize], key);
        if c == 0 {
            return true;
        }
        if c < 0 {
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
        mid = hi + lo;
    }
    false
}

/// `FUN_10017100`.
pub fn allophones(ss: &mut Ss, strs: &WordStrings) {
    allophones_lay(&L14, &T14, ss, strs)
}

pub fn allophones_lay(lay: &Lay, t: &PhTabs, ss: &mut Ss, strs: &WordStrings) {
    let nw = ss.n_words();
    for wi in 0..nw {
        let w = lay.word(wi);
        let n = ss.u8(w + lay.n_phones) as i32;
        let code = |ss: &Ss, k: i32| ss.u8((w as i32 + lay.codes as i32 + k) as usize);
        let mark = |ss: &Ss, k: i32| ss.u8((w as i32 + lay.marks as i32 + k) as usize);
        let mut k = 0;
        while k < n - 1 {
            let c = code(ss, k);
            if mark(ss, k) == b'0' && sc(t.rhotic, c) != 0 && code(ss, k + 1) == R {
                ss.set_u8(w + lay.codes + k as usize, sc(t.rhotic, c));
            }
            k += 1;
        }
        let mut k = 1;
        while k < n - 1 {
            if mark(ss, k - 1) == b'0' {
                let prev_stress = stress_at(lay, t, ss, w, k - 1);
                let next_v = vowel_class_at(lay, t, ss, w, k + 1);
                let at = w + lay.codes + k as usize;
                if mark(ss, k) == b'0' {
                    let c = ss.u8(at);
                    if c == T {
                        let pc = code(ss, k - 1);
                        if (pc == S || pc == SH) && next_v >= 0 {
                            ss.set_u8(at, 0x48);
                        } else if sc(t.be3c, pc) == 1 && next_v == 0 {
                            ss.set_u8(at, 0x48);
                        } else if prev_stress < 1
                            || n - 2 <= k
                            || mark(ss, k + 1) != b'0'
                            || code(ss, k + 1) != 7
                            || code(ss, k + 2) != 0x2d
                        {
                            if prev_stress >= 0 {
                                if next_v == 0 {
                                    ss.set_u8(at, 0x4d);
                                } else {
                                    let nc = code(ss, k + 1);
                                    if nc == 0x2b || nc == 0x2c || nc == 0x2d {
                                        ss.set_u8(at, 0x4b);
                                    }
                                }
                            }
                        } else {
                            ss.set_u8(at, 0x4b);
                        }
                    } else if c == K || c == P {
                        if next_v >= 0 && (code(ss, k - 1) == S || next_v == 0) {
                            ss.set_u8(at, if c == K { 0x46 } else { 0x47 });
                        }
                    } else if c == D && prev_stress >= 0 && next_v == 0 {
                        ss.set_u8(at, 0x4e);
                    }
                } else {
                    if prev_stress >= 0 && next_v >= 0 {
                        if code(ss, k + 1) != Y {
                            let c = ss.u8(at);
                            if c == T {
                                ss.set_u8(at, 0x4d);
                            } else if c == D {
                                ss.set_u8(at, 0x4e);
                            }
                        }
                        let c = ss.u8(at);
                        if c == K {
                            ss.set_u8(at, 0x46);
                        } else if c == P {
                            ss.set_u8(at, 0x47);
                        }
                    }
                    if ss.u8(at) == T && sc(t.be3c, code(ss, k - 1)) == 1 && next_v >= 0 {
                        ss.set_u8(at, 0x48);
                    }
                }
            }
            k += 1;
        }
        let mut k = 0;
        while k < n - 1 {
            if mark(ss, k) == b'0' && code(ss, k) == T {
                let v = vowel_class_at(lay, t, ss, w, k + 1);
                if v == 1 || v == 2 {
                    ss.set_u8(w + lay.codes + k as usize, 0x4c);
                }
            }
            k += 1;
        }
    }
    if nw < 2 {
        return;
    }
    for wi in 0..nw - 1 {
        let w = lay.word(wi);
        let nx = lay.word(wi + 1);
        if ss.u8(w + lay.brk) != b']' {
            continue;
        }
        let n = ss.u8(w + lay.n_phones) as i32;
        let last = n - 1;
        let at = (w as i32 + lay.codes as i32 + last) as usize;
        if vowel_class_at(lay, t, ss, nx, 0) < 0 {
            continue;
        }
        if stress_at(lay, t, ss, w, n - 2) >= 0 {
            if ss.u8(nx + lay.codes) != Y {
                let c = ss.u8(at);
                if c == T {
                    ss.set_u8(at, 0x4d);
                } else if c == D {
                    ss.set_u8(at, 0x4e);
                }
            }
            let c = ss.u8(at);
            if c == K {
                ss.set_u8(at, 0x46);
            } else if c == P {
                ss.set_u8(at, 0x47);
            }
        }
        if last > 0 && sc(t.be3c, ss.u8(at - 1)) == 1 {
            let c = ss.u8(at);
            if c == T {
                ss.set_u8(at, 0x48);
            } else if c == K {
                ss.set_u8(at, 0x46);
            } else if c == P {
                ss.set_u8(at, 0x47);
            }
        }
        let c = ss.u8(at);
        if (c == D || c == T) && in_sorted_ci(t.palatal, strs.text_lay(lay, ss, wi + 1)) {
            ss.set_u8(at, if c == D { 0x4f } else { 0x50 });
        }
    }
}

/// `fe_BuildPhoneContexts`.
pub fn build_phone_contexts(ss: &mut Ss, strs: &WordStrings) {
    let nw = ss.n_words();
    for wi in 0..nw {
        let w = Ss::word(wi);
        let n = ss.u8(w + wr::N_PHONES) as usize;
        for k in 0..n {
            ss.set_u8(w + wr::DIST_A + k, 0);
        }
    }
    allophones(ss, strs);
    let np = ss.n_phrases();
    for g in 0..np {
        let e = Ss::phrase(g);
        let first = ss.i16(e) as usize;
        let nwp = ss.i16(e + 2).max(0) as usize;
        let final_brk = ss.u8(Ss::word(first + nwp) - 0x3c0 + wr::BRK);
        let mut pflag = vec![0u8; 1000];
        let mut wof = vec![0u8; 1000];
        let mut pof = vec![0u8; 1000];
        let mut codes = vec![0u8; 1004];
        let mut wstart = [0i16; 101];
        let mut np_: usize = 0;
        for j in 0..nwp {
            let w = Ss::word(first + j);
            wstart[j] = np_ as i16;
            let left = if j == 0 {
                4u8
            } else {
                let b = ss.u8(Ss::word(first + j - 1) + wr::BRK);
                if b == b'a' { 3 } else { (b != b']') as u8 + 1 }
            };
            pflag[np_] = left << 3;
            let n = ss.u8(w + wr::N_PHONES) as usize;
            for k in 0..n {
                if k > 0 && ss.u8(w + wr::MARKS + k - 1) != b'0' {
                    pflag[np_ + k] = pflag[np_ + k].wrapping_add(8);
                }
                if k + 1 < n && ss.u8(w + wr::MARKS + k) != b'0' {
                    pflag[np_ + k] = pflag[np_ + k].wrapping_add(1);
                }
                wof[np_ + k] = j as u8;
                pof[np_ + k] = k as u8;
            }
            for k in 0..n {
                codes[2 + np_ + k] = ss.u8(w + wr::CODES + k);
            }
            let b = ss.u8(w + wr::BRK);
            let right = match b {
                b'Z' => 5,
                b'^' => 6,
                b'[' => 4,
                b'a' => 3,
                _ => (b != b']') as u8 + 1,
            };
            np_ += n;
            pflag[np_ - 1] = pflag[np_ - 1].wrapping_add(right);
        }
        let n = np_;
        let c = |i: usize| codes[2 + i];
        let mut q: Vec<u8> = pflag[..n].to_vec();
        for i in 0..n {
            let b = q[i];
            pflag[i] = 0;
            if b & 0x38 != 0 {
                pflag[i] = 0x80;
            }
            if b & 7 != 0 {
                pflag[i] = pflag[i].wrapping_add(0x40);
            }
        }
        let mut s = 1usize;
        while s < n {
            let bv = (q[s - 1] >> 3) & 7;
            if bv != 0 && sc(&super::tables::PH_VOWEL, codes[s + 1]) == 0 && s < n {
                loop {
                    q[s] = (q[s] & 7).wrapping_add(bv.wrapping_mul(8));
                    if sc(&super::tables::PH_VOWEL, c(s)) != 0 {
                        break;
                    }
                    s += 1;
                    if s >= n {
                        break;
                    }
                }
            }
            s += 1;
        }
        let mut i = n as i32 - 1;
        while i > 0 {
            let bv = q[i as usize];
            if bv & 7 != 0 && sc(&super::tables::PH_VOWEL, c(i as usize)) == 0 {
                i -= 1;
                while i >= 0 {
                    let x = &mut q[i as usize];
                    if *x & 7 != 0 {
                        break;
                    }
                    *x = (*x & 0x38).wrapping_add(bv & 7);
                    if sc(&super::tables::PH_VOWEL, c(i as usize)) != 0 {
                        break;
                    }
                    i -= 1;
                }
            }
            i -= 1;
        }
        for j in 0..nwp.saturating_sub(1) {
            let w = Ss::word(first + j);
            if ss.u8(w + wr::MINOR) != 1 {
                continue;
            }
            let nj = ss.u8(w + wr::N_PHONES) as i32;
            let st = wstart[j] as i32;
            let mut k = nj - 1;
            if k >= 0 {
                loop {
                    let idx = (st + k) as usize;
                    let r = q[idx] & 7;
                    if r == 1 || r == 2 {
                        let o = w + wr::DIST_A + k as usize;
                        ss.set_u8(o, ss.u8(o).wrapping_add(1));
                    }
                    if sc(&super::tables::PH_VOWEL, c(idx)) != 0 {
                        break;
                    }
                    k -= 1;
                    if k < 0 {
                        break;
                    }
                }
            }
            let nn = ss.u8(w + 0x455) as i32;
            if nn != 0 {
                let st1 = wstart[j + 1] as i32;
                let mut k = 0;
                loop {
                    let idx = (st1 + k) as usize;
                    let l = (q[idx] >> 3) & 7;
                    if l == 1 || l == 2 {
                        let o = w + 0x61d + k as usize;
                        ss.set_u8(o, ss.u8(o).wrapping_add(10));
                    }
                    if sc(&super::tables::PH_VOWEL, c(idx)) != 0 {
                        break;
                    }
                    k += 1;
                    if k >= ss.u8(w + 0x455) as i32 {
                        break;
                    }
                }
            }
        }
        for j in 0..nwp {
            let w = Ss::word(first + j);
            let mut k = 0usize;
            while k < ss.u8(w + wr::N_PHONES) as usize {
                if ss.u8(w + wr::MARKS + k) == b'2' {
                    let nb = ss.u8(w + wr::N_PHONES) as usize;
                    let o = w + wr::DIST_B + k;
                    if k + 1 < nb && ss.u8(o) / 10 == 0 {
                        ss.set_u8(o, ss.u8(o).wrapping_add(10));
                    }
                    let o = w + wr::DIST_A + k;
                    if ss.u8(o) % 10 == 0 {
                        ss.set_u8(o, ss.u8(o).wrapping_add(1));
                    }
                }
                k += 1;
            }
        }
        for s in 0..n {
            let wj = first + wof[s] as usize;
            let k = pof[s] as usize;
            let o = Ss::word(wj) + wr::CTX + 7 * k;
            match s {
                0 => {
                    ss.set_u8(o, 0x5a);
                    ss.set_u8(o + 1, 0x5a);
                }
                1 => {
                    ss.set_u8(o, 0x5a);
                    ss.set_u8(o + 1, c(0));
                }
                _ => {
                    ss.set_u8(o, c(s - 2));
                    ss.set_u8(o + 1, c(s - 1));
                }
            }
            ss.set_u8(o + 2, c(s));
            if s == n - 1 {
                ss.set_u8(o + 3, final_brk);
                ss.set_u8(o + 4, final_brk);
            } else if s == n - 2 {
                ss.set_u8(o + 3, c(s + 1));
                ss.set_u8(o + 4, final_brk);
            } else {
                ss.set_u8(o + 3, c(s + 1));
                ss.set_u8(o + 4, c(s + 2));
            }
            ss.set_u8(o + 5, q[s].wrapping_add(pflag[s]));
            ss.set_u8(o + 6, 0);
            if ss.u8(Ss::word(wj) + wr::DOMAIN) == 0x0c {
                ss.set_u8(o + 6, 0x20);
            }
        }
    }
}

/// julie09 `FUN_10016ed0`. `pf` is the decimal-coded flag stack (`left * 10` on a word's first phone, `+10` /
/// `+1` for boundary marks, the right-break code on its last phone), `q` its copy that the two propagation
/// passes rewrite.
pub fn build_phone_contexts09(era: Era, ss: &mut Ss, strs: &WordStrings) {
    let lay = Lay::of(era);
    let t = PhTabs::of(era);
    let nw = ss.n_words();
    for wi in 0..nw {
        let w = lay.word(wi);
        for k in 0..ss.u8(w + lay.n_phones) as usize {
            ss.set_u8(w + lay.dist_a + k, 0);
        }
    }
    allophones_lay(lay, t, ss, strs);
    let np = ss.n_phrases();
    for g in 0..np {
        let e = Ss::phrase(g);
        let first = ss.i16(e) as usize;
        let nwp = ss.i16(e + 2).max(0) as usize;
        let final_brk = ss.u8(lay.word(first + nwp) - lay.stride + lay.brk);
        let mut pf = vec![0u8; 1000];
        let mut wof = vec![0usize; 1000];
        let mut pof = vec![0usize; 1000];
        let mut codes = vec![0u8; 1002];
        let mut wstart = [0i16; 101];
        let mut n: usize = 0;
        for j in 0..nwp {
            let w = lay.word(first + j);
            wstart[j] = n as i16;
            let left = if j == 0 {
                3u8
            } else {
                (ss.u8(lay.word(first + j - 1) + lay.brk) != b']') as u8 + 1
            };
            pf[n] = left.wrapping_mul(10);
            let nph = ss.u8(w + lay.n_phones) as usize;
            for k in 0..nph {
                if k > 0 && ss.u8(w + lay.marks + k - 1) != b'0' {
                    pf[n + k] = pf[n + k].wrapping_add(10);
                }
                if k + 1 < nph && ss.u8(w + lay.marks + k) != b'0' {
                    pf[n + k] = pf[n + k].wrapping_add(1);
                }
                pof[n + k] = k;
                wof[n + k] = first + j;
            }
            for k in 0..nph {
                codes[n + k] = ss.u8(w + lay.codes + k);
            }
            let right = match ss.u8(w + lay.brk) {
                b'Z' => 4,
                b'^' => 5,
                b'[' => 3,
                b => (b != b']') as u8 + 1,
            };
            n += nph;
            if n > 0 {
                pf[n - 1] = pf[n - 1].wrapping_add(right);
            }
        }
        let vowel = |i: usize| sc(t.vowel, codes[i]) != 0;
        // q[s] lives at index s + 1 in the engine's buffer; q[-1] reads the byte before it.
        let mut q: Vec<u8> = pf[..n].to_vec();
        let mut s = 1usize;
        while s < n {
            let cv = q[s - 1];
            if cv / 10 > 0 && sc(t.vowel, codes[s - 1]) == 0 {
                loop {
                    q[s] = q[s] % 10 + (cv / 10) * 10;
                    if vowel(s) {
                        break;
                    }
                    s += 1;
                    if s >= n {
                        break;
                    }
                }
            }
            s += 1;
        }
        let mut i = n as i32 - 1;
        while i > 0 {
            let cv = q[i as usize];
            if cv % 10 != 0 && !vowel(i as usize) {
                i -= 1;
                while i >= 0 {
                    let x = q[i as usize];
                    if x % 10 > 0 {
                        break;
                    }
                    q[i as usize] = (x / 10) * 10 + cv % 10;
                    if vowel(i as usize) {
                        break;
                    }
                    i -= 1;
                }
            }
            i -= 1;
        }
        for j in 0..nwp.saturating_sub(1) {
            let w = lay.word(first + j);
            if ss.u8(w + lay.minor) != 1 {
                continue;
            }
            let nj = ss.u8(w + lay.n_phones) as usize;
            let st = wstart[j] as usize;
            for k in (0..nj).rev() {
                let r = q[st + k] % 10;
                if r == 1 || r == 2 {
                    let o = w + lay.dist_a + k;
                    ss.set_u8(o, ss.u8(o).wrapping_add(1));
                }
            }
            let nn = ss.u8(w + lay.stride + lay.n_phones) as usize;
            let st1 = wstart[j + 1] as usize;
            for k in 0..nn {
                let l = q[st1 + k] / 10;
                if l == 1 || l == 2 {
                    let o = w + lay.stride + lay.dist_a + k;
                    ss.set_u8(o, ss.u8(o).wrapping_add(10));
                }
            }
        }
        if era == Era::Paul09 {
            // paul09 `FUN_10017080` only: the 2014 mark-'2' distance pass.
            for j in 0..nwp {
                let w = lay.word(first + j);
                let nb = ss.u8(w + lay.n_phones) as usize;
                for k in 0..nb {
                    if ss.u8(w + lay.marks + k) == b'2' {
                        let o = w + lay.dist_b + k;
                        if k + 1 < nb && ss.u8(o) / 10 == 0 {
                            ss.set_u8(o, ss.u8(o).wrapping_add(10));
                        }
                        let o = w + lay.dist_a + k;
                        if ss.u8(o) % 10 == 0 {
                            ss.set_u8(o, ss.u8(o).wrapping_add(1));
                        }
                    }
                }
            }
        }
        for s in 0..n {
            let o = lay.word(wof[s]) + lay.ctx + lay.ctx_stride * pof[s];
            ss.set_u8(o, if s == 0 { 0x5a } else { codes[s - 1] });
            ss.set_u8(o + 1, codes[s]);
            ss.set_u8(o + 2, if s == n - 1 { final_brk } else { codes[s + 1] });
            ss.set_u8(o + 3, pf[s]);
            ss.set_u8(o + 4, q[s]);
        }
    }
}
