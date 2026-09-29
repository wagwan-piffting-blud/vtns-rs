//! Phone contexts: `fe_BuildPhoneContexts` (0x1001d630). English
//! counterpart: allophone rewriting `FUN_10017100` + `fe_BuildPhoneContexts`
//! `0x10017510` (`src\ling\context.rs`).
//!
//! Confirmed structural differences from English:
//!   * No allophone-rewriting pass at all -- the decompile calls nothing
//!     before the phone-context loop except the MSVC stack-probe
//!     (`FUN_10050110`, `__chkstk`-style, not logic). Violeta's flap/
//!     glottal-stop/palatalisation rules either don't exist or are baked
//!     into the LTS output already.
//!   * No `MARKS[k]=='2'` distance-counter pass: Violeta's `words.rs` never
//!     writes a `'2'` mark (see that file's doc comment), so this whole
//!     English block is genuinely dead code here and is not ported.
//!   * The left/right phrase-boundary codes used to seed `pflag` recognize
//!     one extra byte, `'K'` (0x4b), alongside the standard -0x14-shifted
//!     set -- analogous to English's own extra `'a'` (0x61) case, but an
//!     independently-chosen byte, not `'a' - 0x14`.
//!   * `CTX`/`DIST_A` field offsets and the LL/L sentinel byte (`'F'`,
//!     0x46 = English's `'Z'` shifted) all confirmed directly from the
//!     decompile.

use super::breaks::MINOR;
use super::syllab::PH_VOWEL;
use super::words::{wr2, WordStringsSpa};
use super::{wr, SsSpa};
use crate::ling::sc;

/// Fields used only here.
mod wr3 {
    /// Combined "distance to nearest word-internal break" counter (English:
    /// `wr::DIST_A`), byte-per-phone. Zeroed per word at the top of
    /// `build_phone_contexts`, then gets `+1` per phone scanning backward
    /// from a MINOR word's own last phone, and (via the *next* word, same
    /// field) `+10` per phone scanning forward from that MINOR word's
    /// successor -- exactly English's DIST_A double-duty, just without a
    /// separate DIST_B (nothing here ever needs it, since Violeta's MARKS
    /// array never contains '2').
    pub const DIST_A: usize = 0x398;
    /// Forward-scan phone count (English's unnamed literal `w+0x455`);
    /// written by an earlier stage this batch didn't reverse -- read only.
    pub const FWD_N: usize = 0x61b;
}

fn left_code(prev_brk: Option<u8>) -> u8 {
    match prev_brk {
        None => 4,
        Some(b'K') => 3,
        Some(b) => (b != b'I') as u8 + 1,
    }
}

fn right_code(b: u8) -> u8 {
    match b {
        b'F' => 5,
        b'J' => 6,
        b'G' => 4,
        b'K' => 3,
        _ => (b != b'I') as u8 + 1,
    }
}

/// `fe_BuildPhoneContexts`.
pub fn build_phone_contexts(ss: &mut SsSpa, strs: &WordStringsSpa) {
    let nw = ss.n_words();
    for wi in 0..nw {
        let w = SsSpa::word(wi);
        let n = ss.u8(w + wr::N_PHONES) as usize;
        for k in 0..n {
            ss.set_u8(w + wr3::DIST_A + k, 0);
        }
    }
    let _ = strs; // Violeta's CODES/MARKS are already final; no allophone pass reads WA strings here.

    let np = ss.n_phrases();
    for g in 0..np {
        let e = SsSpa::phrase(g);
        let first = ss.i16(e) as usize;
        let nwp = ss.i16(e + 2).max(0) as usize;
        if nwp == 0 {
            continue;
        }
        let final_brk = ss.u8(SsSpa::word(first + nwp - 1) + wr::BRK);
        let mut pflag = vec![0u8; 1000];
        let mut wof = vec![0u8; 1000];
        let mut pof = vec![0u8; 1000];
        let mut codes = vec![0u8; 1004];
        let mut wstart = [0i16; 101];
        let mut np_: usize = 0;
        for j in 0..nwp {
            let w = SsSpa::word(first + j);
            wstart[j] = np_ as i16;
            let prev_brk = if j == 0 { None } else { Some(ss.u8(SsSpa::word(first + j - 1) + wr::BRK)) };
            pflag[np_] = left_code(prev_brk) << 3;
            let n = ss.u8(w + wr::N_PHONES) as usize;
            for k in 0..n {
                if k > 0 && ss.u8(w + wr2::MARKS + k - 1) != b'0' {
                    pflag[np_ + k] = pflag[np_ + k].wrapping_add(8);
                }
                if k + 1 < n && ss.u8(w + wr2::MARKS + k) != b'0' {
                    pflag[np_ + k] = pflag[np_ + k].wrapping_add(1);
                }
                wof[np_ + k] = j as u8;
                pof[np_ + k] = k as u8;
                codes[2 + np_ + k] = ss.u8(w + wr::PHONES + k);
            }
            let b = ss.u8(w + wr::BRK);
            np_ += n;
            pflag[np_ - 1] = pflag[np_ - 1].wrapping_add(right_code(b));
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
        // Forward flag propagation (spread a left-boundary code across a
        // trailing consonant cluster up to the next vowel). The entry test
        // checks the *flag-bearing* phone `s-1` for vowel-ness (decompile:
        // `DAT_10067328[codes[s-1]]`, one byte off from the inner loop's own
        // `codes[s]` check) -- not `s`, despite English's own
        // `context::build_phone_contexts` using `c(s)` there for the
        // analogous check.
        let mut s = 1usize;
        while s < n {
            let bv = (q[s - 1] >> 3) & 7;
            if bv != 0 && sc(&PH_VOWEL, c(s - 1)) == 0 && s < n {
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
        // Backward flag propagation (right-boundary code across a leading
        // consonant cluster back to the previous vowel).
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
            let w = SsSpa::word(first + j);
            if ss.u8(w + MINOR) != 1 {
                continue;
            }
            let nj = ss.u8(w + wr::N_PHONES) as i32;
            if nj > 0 {
                let st = wstart[j] as i32;
                let mut k = nj - 1;
                loop {
                    let idx = (st + k) as usize;
                    if q[idx] & 7 == 1 || q[idx] & 7 == 2 {
                        let o = w + wr3::DIST_A + k as usize;
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
            let fwd_n = ss.u8(w + wr3::FWD_N) as i32;
            if fwd_n != 0 {
                let st1 = wstart[j + 1] as i32;
                let next_w = SsSpa::word(first + j + 1);
                let mut k = 0;
                loop {
                    let idx = (st1 + k) as usize;
                    if (q[idx] >> 3) & 7 == 1 || (q[idx] >> 3) & 7 == 2 {
                        let o = next_w + wr3::DIST_A + k as usize;
                        ss.set_u8(o, ss.u8(o).wrapping_add(10));
                    }
                    if sc(&PH_VOWEL, c(idx)) != 0 {
                        break;
                    }
                    k += 1;
                    if k >= fwd_n {
                        break;
                    }
                }
            }
        }
        for s in 0..n {
            let wj = first + wof[s] as usize;
            let k = pof[s] as usize;
            let o = SsSpa::word(wj) + wr::CTX + 7 * k;
            match s {
                0 => {
                    ss.set_u8(o, 0x46);
                    ss.set_u8(o + 1, 0x46);
                }
                1 => {
                    ss.set_u8(o, 0x46);
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
            if ss.u8(SsSpa::word(wj) + wr2::DOMAIN) == 0x0c {
                ss.set_u8(o + 6, 0x20);
            }
        }
    }
}
