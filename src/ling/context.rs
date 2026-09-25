//! Phone contexts: allophone rewriting `FUN_10017100` (flaps, glottal and
//! unreleased stops, palatalisation before "you"), then
//! `fe_BuildPhoneContexts` (0x10017510): the 7-byte quinphone label
//! `{LL, L, C, R, RR, posbits, attr}` per phone, and the stress-distance
//! counters at word +0x25d.

use super::tables::{PALATAL_NEXT, PH_BE3C, PH_BB70, PH_RHOTIC, PH_STRESS, PH_VOWEL};
use super::{sc, wr, Ss, WordStrings};
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
fn stress_at(ss: &Ss, w: usize, k: i32) -> i16 {
    if k >= 0 {
        let c = ss.u8(w + wr::CODES + k as usize);
        if sc(&PH_VOWEL, c) == 1 {
            return sc(&PH_STRESS, c) as i16;
        }
        if c == R && k > 0 {
            let c2 = ss.u8(w + wr::CODES + k as usize - 1);
            if sc(&PH_VOWEL, c2) == 1 {
                return sc(&PH_STRESS, c2) as i16;
            }
        }
    }
    -1
}

/// `FUN_10017090`: vowel class of phone k (or of the vowel after a Y at k), -1 otherwise.
fn vowel_class_at(ss: &Ss, w: usize, k: i32) -> i16 {
    let n = ss.u8(w + wr::N_PHONES) as i32;
    if k < n {
        let c = ss.u8((w as isize + wr::CODES as isize + k as isize) as usize);
        if sc(&PH_VOWEL, c) == 1 {
            return sc(&PH_BB70, c) as i8 as i16;
        }
        if c == Y && k < n - 1 {
            let c2 = ss.u8(w + wr::CODES + k as usize + 1);
            if sc(&PH_VOWEL, c2) == 1 {
                return sc(&PH_BB70, c2) as i8 as i16;
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
    let nw = ss.n_words();
    for wi in 0..nw {
        let w = Ss::word(wi);
        let n = ss.u8(w + wr::N_PHONES) as i32;
        let code = |ss: &Ss, k: i32| ss.u8((w as i32 + wr::CODES as i32 + k) as usize);
        let mark = |ss: &Ss, k: i32| ss.u8((w as i32 + wr::MARKS as i32 + k) as usize);
        let mut k = 0;
        while k < n - 1 {
            let c = code(ss, k);
            if mark(ss, k) == b'0' && sc(&PH_RHOTIC, c) != 0 && code(ss, k + 1) == R {
                ss.set_u8(w + wr::CODES + k as usize, sc(&PH_RHOTIC, c));
            }
            k += 1;
        }
        let mut k = 1;
        while k < n - 1 {
            if mark(ss, k - 1) == b'0' {
                let prev_stress = stress_at(ss, w, k - 1);
                let next_v = vowel_class_at(ss, w, k + 1);
                let at = w + wr::CODES + k as usize;
                if mark(ss, k) == b'0' {
                    let c = ss.u8(at);
                    if c == T {
                        let pc = code(ss, k - 1);
                        if (pc == S || pc == SH) && next_v >= 0 {
                            ss.set_u8(at, 0x48);
                        } else if sc(&PH_BE3C, pc) == 1 && next_v == 0 {
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
                    if ss.u8(at) == T && sc(&PH_BE3C, code(ss, k - 1)) == 1 && next_v >= 0 {
                        ss.set_u8(at, 0x48);
                    }
                }
            }
            k += 1;
        }
        let mut k = 0;
        while k < n - 1 {
            if mark(ss, k) == b'0' && code(ss, k) == T {
                let v = vowel_class_at(ss, w, k + 1);
                if v == 1 || v == 2 {
                    ss.set_u8(w + wr::CODES + k as usize, 0x4c);
                }
            }
            k += 1;
        }
    }
    if nw < 2 {
        return;
    }
    for wi in 0..nw - 1 {
        let w = Ss::word(wi);
        let nx = Ss::word(wi + 1);
        if ss.u8(w + wr::BRK) != b']' {
            continue;
        }
        let n = ss.u8(w + wr::N_PHONES) as i32;
        let last = n - 1;
        let at = (w as i32 + wr::CODES as i32 + last) as usize;
        if vowel_class_at(ss, nx, 0) < 0 {
            continue;
        }
        if stress_at(ss, w, n - 2) >= 0 {
            if ss.u8(nx + wr::CODES) != Y {
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
        if last > 0 && sc(&PH_BE3C, ss.u8(at - 1)) == 1 {
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
        if (c == D || c == T) && in_sorted_ci(&PALATAL_NEXT, strs.text(ss, wi + 1)) {
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
            if bv != 0 && sc(&PH_VOWEL, codes[s + 1]) == 0 && s < n {
                loop {
                    q[s] = (q[s] & 7).wrapping_add(bv.wrapping_mul(8));
                    if sc(&PH_VOWEL, c(s)) != 0 {
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
            if bv & 7 != 0 && sc(&PH_VOWEL, c(i as usize)) == 0 {
                i -= 1;
                while i >= 0 {
                    let x = &mut q[i as usize];
                    if *x & 7 != 0 {
                        break;
                    }
                    *x = (*x & 0x38).wrapping_add(bv & 7);
                    if sc(&PH_VOWEL, c(i as usize)) != 0 {
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
                    if sc(&PH_VOWEL, c(idx)) != 0 {
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
                    if sc(&PH_VOWEL, c(idx)) != 0 {
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
