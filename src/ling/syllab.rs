//! Syllabification: `FUN_10012c70` (per word), `FUN_10013c00` (one boundary
//! group), `FUN_10013f30` (nucleus / maximal-onset analysis) and
//! `FUN_10013da0` (onset legality against the cluster list at 0x10079a4c).

use super::tables::{NG_NAME, ONSETS, PHONE_FEAT, PHONE_FEAT_ROW, PH_BASE, PH_STRESS};
use super::{ptr_ss, sc, syl, wr, Ss, SYL_BASE, SYL_SIZE};

fn feat_row(idx: i32) -> usize {
    // The engine multiplies the low byte (sign-extended) by 11.
    (idx as u8 as i8 as isize * PHONE_FEAT_ROW as isize) as usize
}

fn is_vowel_idx(idx: i32) -> bool {
    PHONE_FEAT[feat_row(idx) + 3] == 1
}

fn phone_name(idx: i32) -> &'static [u8] {
    let r = feat_row(idx);
    let s = &PHONE_FEAT[r..r + 3];
    let n = s.iter().position(|&c| c == 0).unwrap_or(3);
    &s[..n]
}

/// `FUN_10013da0(arr, from, to)`: is phones[from..=to] (1-based) a legal onset.
fn legal_onset(arr: &[i32], from: i32, to: i32) -> bool {
    if from < 1 || to < 1 {
        return false;
    }
    let len = to - from + 1;
    if !(1..=3).contains(&len) {
        return false;
    }
    if len == 1 {
        let idx = arr[from as usize];
        return !is_vowel_idx(idx) && phone_name(idx) != NG_NAME;
    }
    let mut s: Vec<u8> = Vec::with_capacity(20);
    for k in from..=to {
        s.extend_from_slice(phone_name(arr[k as usize]));
        s.push(b' ');
    }
    s.pop();
    ONSETS.iter().any(|&o| o == s.as_slice())
}

/// Result of `FUN_10013f30` for one group: `(a, b, c, d)` per syllable =
/// nucleus, onset start, coda end (1-based phone positions) and stress.
struct SylPlan {
    n: i32,
    syl: Vec<[i32; 4]>,
}

fn plan(idx: &[u8], stress: &[u8], n: i32) -> SylPlan {
    if n > 0x41 {
        return SylPlan { n: 0, syl: Vec::new() };
    }
    let nu = n as usize;
    // 1-based arrays like the engine's locals; [0] holds n.
    let mut ph = vec![0i32; nu + 1];
    let mut st = vec![0i32; nu + 1];
    ph[0] = n;
    st[0] = n;
    for i in 0..nu {
        ph[i + 1] = idx[i] as i8 as i32;
        st[i + 1] = stress[i] as i8 as i32;
    }
    let mut owner = vec![0i32; nu + 2];
    let mut syl: Vec<[i32; 4]> = Vec::new();
    for i in 1..=nu {
        if is_vowel_idx(ph[i]) && syl.len() < 0x41 {
            syl.push([i as i32, 0, 0, st[i]]);
            owner[i] = syl.len() as i32;
        }
    }
    if syl.is_empty() {
        return SylPlan { n: 0, syl };
    }
    let ns = syl.len();
    for k in 0..ns {
        let last = syl[k][0] - 1;
        let mut j = last;
        let mut ok = legal_onset(&ph, j, last);
        while ok {
            j -= 1;
            if k + 1 >= 2 && j <= syl[k - 1][0] {
                break;
            }
            ok = legal_onset(&ph, j, last);
        }
        j += 1;
        if j <= last {
            syl[k][1] = j;
            for p in j..=last {
                owner[p as usize] = (k + 1) as i32;
            }
        } else {
            syl[k][1] = syl[k][0];
        }
    }
    for k in 0..ns {
        let from = syl[k][0] + 1;
        let mut j = from;
        while j <= n && owner[j as usize] == 0 {
            j += 1;
        }
        j -= 1;
        if from <= j {
            syl[k][2] = j;
            for p in from..=j {
                owner[p as usize] = (k + 1) as i32;
            }
        } else {
            syl[k][2] = syl[k][0];
        }
    }
    // Engine quirk: both fix-ups write the FIRST syllable's onset slot.
    if syl[0][1] != 1 {
        syl[0][1] = 1;
    }
    if syl[ns - 1][2] != n {
        syl[0][1] = n;
    }
    SylPlan { n: ns as i32, syl }
}

/// `FUN_10013c00`: syllabify one group of `n` phones into syllable records at
/// `rec` (first one already has its first-phone byte set). Writes position
/// codes (1 onset, 2 nucleus, 3 coda) to `pos`. Returns syllables added.
fn group(ss: &mut Ss, idx: &[u8], stress: &[u8], n: i32, rec: usize, pos: usize) -> i32 {
    let p = plan(idx, stress, n);
    let mut ns = p.n;
    if ns > 0x41 {
        ns = 0x41;
    }
    if ns == 0 {
        let cnt = n as u8;
        ss.set_u8(rec + syl::N_PHONES, cnt);
        for i in 0..cnt as usize {
            ss.set_u8(pos + i, 1);
        }
        ss.set_u8(rec + syl::STRESS, 0);
        ss.set_u8(rec + syl::SHAPE, 4);
        ss.set_u8(rec + syl::POSITION, 1);
        return 1;
    }
    let first = ss.u8(rec + syl::FIRST_PHONE);
    let mut off: i32 = 0;
    for k in 1..=ns {
        let e = p.syl[(k - 1) as usize];
        let r = rec + (k as usize - 1) * SYL_SIZE;
        ss.set_u8(r + syl::FIRST_PHONE, first.wrapping_add(off as u8));
        let mut cx: i32 = 0;
        let b = e[1] as i16;
        while ((b as i32 + cx) as i16 as i32) < e[0] {
            ss.set_u8(pos + (off + cx) as usize, 1);
            cx += 1;
        }
        ss.set_u8(pos + (off + cx) as usize, 2);
        cx += 1;
        let mut si = (e[0] as i16).wrapping_add(1);
        while si as i32 <= e[2] {
            ss.set_u8(pos + (off + cx) as usize, 3);
            cx += 1;
            si = si.wrapping_add(1);
        }
        ss.set_u8(r + syl::STRESS, e[3] as u8);
        let first_is_nuc = ss.u8(pos + off as usize) == 2;
        let last_is_nuc = ss.u8(pos + (off + cx - 1) as usize) == 2;
        let shape = match (first_is_nuc, last_is_nuc) {
            (true, true) => 0,
            (true, false) => 2,
            (false, true) => 1,
            (false, false) => 3,
        };
        ss.set_u8(r + syl::SHAPE, shape);
        off += cx;
        ss.set_u8(r + syl::N_PHONES, cx as u8);
        if k == 1 {
            ss.set_u8(rec + syl::POSITION, 1);
        } else if k == ns {
            ss.set_u8(r + syl::POSITION, 3);
        } else {
            ss.set_u8(r + syl::POSITION, 2);
        }
    }
    ns
}

/// `FUN_10012c70`: syllabify every word of the sentence.
pub fn syllabify(ss: &mut Ss) {
    ss.set_i16(4, 0);
    let nw = ss.n_words();
    for i in 0..nw {
        let w = Ss::word(i);
        let n = ss.u8(w + wr::N_PHONES) as usize;
        let base = SYL_BASE + ss.i16(4) as usize * SYL_SIZE;
        ss.set_u32(w + wr::SYL_PTR, ptr_ss(base));
        let mut idx = [0u8; 68];
        let mut st = [0u8; 68];
        for k in 0..n.min(68) {
            let c = ss.u8(w + wr::CODES + k);
            idx[k] = sc(&PH_BASE, c);
            st[k] = sc(&PH_STRESS, c);
        }
        ss.set_u8(w + wr::N_SYL, 0);
        let mut gs: i16 = 0;
        let mut k: i16 = 0;
        while (k as i32) < n as i32 {
            if ss.u8(w + wr::MARKS + k as usize) != b'0' || k as usize == n - 1 {
                let nsyl = ss.u8(w + wr::N_SYL) as usize;
                let rec = base + nsyl * SYL_SIZE;
                ss.set_u8(rec + syl::FIRST_PHONE, gs as u8);
                let g = gs as usize;
                let cnt = (k - gs + 1) as i32;
                let added = group(ss, &idx[g.min(68)..], &st[g.min(68)..], cnt, rec, w + wr::SYLPOS + g);
                let v = ss.u8(w + wr::N_SYL).wrapping_add(added as u8);
                ss.set_u8(w + wr::N_SYL, v);
                gs = k + 1;
            }
            k += 1;
        }
        let t = ss.i16(4).wrapping_add(ss.u8(w + wr::N_SYL) as i16);
        ss.set_i16(4, t);
    }
}
