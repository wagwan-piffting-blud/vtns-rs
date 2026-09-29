//! Waveform generation (vt_eng.dll `wav_GeneratePhrase` 0x1002bd90 and callees).
//!
//! One phrase at a time: the selected units become 0x34-byte unit records
//! (`build_records`), each speech record is optionally re-shaped by the join
//! smoother (`smooth_joins`), decoded, and overlap-added into the output with a
//! Hann cross-fade at every unit edge (`ola_unmodified`). When the record's pitch
//! or duration rate differs from 100 the PSOLA path (`psola`) runs instead.

#[path = "wave_tables.rs"]
mod tables;

#[path = "formats.rs"]
pub mod formats;

use std::io;

use crate::bank::{Bank, Primary};

pub use tables::{CLAMP_HI, CLAMP_LO, HANN_LEN};

const HALF: i32 = (HANN_LEN as i32) >> 1;

/// Scratch buffer for the smoothed unit: `TIB+0x10eef4` is cleared to 5000 dwords.
const SMOOTH_BUF: usize = 10_000;

#[inline]
fn hann(i: i32) -> f32 {
    f32::from_bits(tables::HANN_BITS[i as usize])
}

/// `(double)(float)w * (double)s`, saturated and truncated the way every
/// windowing loop in the engine does it (compare against 0x100751b8/b0, then
/// `__ftol` 0x1006479c). The product is exact in f64.
#[inline]
pub fn wmul(w: f32, s: i32) -> i16 {
    let v = w as f64 * s as f64;
    if !(v < CLAMP_HI) || v <= CLAMP_LO {
        if v < CLAMP_HI {
            -32767
        } else {
            32766
        }
    } else {
        v as i32 as i16
    }
}

#[inline]
fn rise(k: i32, len: i32) -> f32 {
    hann(k * HALF / len)
}

#[inline]
fn fall(k: i32, len: i32) -> f32 {
    hann(k * HALF / len + HALF)
}

/// The integer saturation used by volume scaling and the smoother:
/// `v > 0x7ffe || v < -0x7fff` maps to 32766 / -32767.
#[inline]
pub fn sat16(v: i32) -> i16 {
    if v > 0x7ffe {
        32766
    } else if v < -0x7fff {
        -32767
    } else {
        v as i16
    }
}

/// Duration factor as the engine derives it from a rate percentage.
#[inline]
pub fn dur_factor(rate: i32) -> i32 {
    ((rate >> 1) + 10000) / rate
}

/// 0x34-byte unit record (`TIB+0xeddf8 + i*0x34`).
#[derive(Debug, Clone, Default)]
pub struct Record {
    /// +0x00 pitch rate %.
    pub pitch: i32,
    /// +0x04 duration rate %.
    pub dur: i32,
    /// +0x08 volume %.
    pub vol: i32,
    /// +0x0c length in samples.
    pub len: i32,
    /// +0x10 target index in the phrase, -1 for silence.
    pub unit: i16,
    /// +0x12 .dat/.upm file index.
    pub file: i16,
    /// +0x14 `pcm_pos` plus the half-unit start offset in samples (bookkeeping only).
    pub pos14: i32,
    /// +0x18 the unit's DB entry.
    pub db: Primary,
    /// +0x1c byte offset of the first pitch mark used in `.upm`.
    pub upm: u32,
    /// +0x20 pitch marks used.
    pub n_pm: u8,
    /// +0x22 / +0x24 left / right overlap in samples.
    pub ovl_l: i16,
    pub ovl_r: i16,
    /// +0x26 0 whole, 1 first half, 2 second half.
    pub mode: u8,
    /// +0x27 1 silence, 2 unit.
    pub typ: u8,
    /// +0x28 word index (phrase index for a silence record).
    pub word: u16,
    /// +0x2a -1 for silence.
    pub w2a: i16,
    /// +0x2c phone index within the word, -1 for silence.
    pub phone: u16,
    /// +0x32 center-phone byte of the word's phone context, 0x5a for silence.
    pub center: u8,
}

pub const TYPE_SILENCE: u8 = 1;
pub const TYPE_UNIT: u8 = 2;

impl Record {
    /// `wav_FillUnitRecord` 0x1002c120: derive the per-mode fields from a DB entry.
    pub fn fill(&mut self, db: &Primary, file: u8) {
        self.file = file as i16;
        self.db = *db;
        let p1 = db.periods[1] as i16;
        match self.mode {
            1 => {
                self.ovl_l = (db.periods[0] as i16) << 1;
                self.ovl_r = p1 << 1;
                self.pos14 = db.pcm_pos as i32;
                self.len = db.half1_len as i16 as i32;
                self.upm = db.pm_pos;
                self.n_pm = db.pm1;
            }
            2 => {
                self.ovl_l = p1 << 1;
                self.ovl_r = (db.periods[2] as i16) << 1;
                self.len = db.half2_len as i16 as i32;
                self.pos14 = (db.half1_len as i16).wrapping_sub(p1 * 2) as i32 + db.pcm_pos as i32;
                self.upm = (db.pm1 as u32).wrapping_sub(1).wrapping_add(db.pm_pos);
                self.n_pm = db.pm2;
            }
            _ => {
                self.ovl_l = (db.periods[0] as i16) << 1;
                self.ovl_r = (db.periods[2] as i16) << 1;
                self.pos14 = db.pcm_pos as i32;
                self.len = db.half2_len as i16 as i32 + db.half1_len as i16 as i32 - (p1 * 2) as i32;
                self.upm = db.pm_pos;
                self.n_pm = db.pm2.wrapping_add(db.pm1).wrapping_sub(1);
            }
        }
    }

    pub fn silence(len: i32, word: u16, pitch: i32, vol: i32) -> Record {
        Self::silence_pad(len, word, pitch, vol, 0x5a)
    }

    /// As `silence`, but with the pad/silence centre byte given explicitly:
    /// English's word-context pad code is `0x5a` ('Z'); Violeta's whole break
    /// alphabet is shifted `-0x14` (`0x5a -> 0x46`, confirmed by
    /// `wav_BuildUnitRecords` hardcoding `0x46` where the English build has
    /// `0x5a`, per `reveng\VIOLETA.md`).
    pub fn silence_pad(len: i32, word: u16, pitch: i32, vol: i32, pad: u8) -> Record {
        Record {
            pitch,
            dur: 100,
            vol,
            len,
            unit: -1,
            typ: TYPE_SILENCE,
            word,
            w2a: -1,
            phone: 0xffff,
            center: pad,
            ..Default::default()
        }
    }

    pub fn is_silence(&self) -> bool {
        self.typ == TYPE_SILENCE
    }
}

/// `TIB+0x113d14`: the engine decodes every payload into this 10000-sample
/// window and then copies `rec.len` samples out of it. A record can be a few
/// samples longer than its payload, so what is left from earlier decodes shows
/// through; the window therefore persists for the whole request.
pub struct DecodeWindow {
    buf: Vec<i16>,
}

impl Default for DecodeWindow {
    fn default() -> Self {
        DecodeWindow { buf: vec![0i16; DECODE_WINDOW] }
    }
}

const DECODE_WINDOW: usize = 0x4e20 / 2;

impl DecodeWindow {
    /// `wav_DecodeUnit` 0x1002c8b0: decode, cut the record's span, apply volume.
    pub fn decode(&mut self, bank: &Bank, uid: u32, rec: &Record) -> io::Result<Vec<i16>> {
        let full = bank.decode(uid, &rec.db)?;
        if full.len() > self.buf.len() {
            self.buf.resize(full.len(), 0);
        }
        self.buf[..full.len()].copy_from_slice(&full);
        let start = if rec.mode == 2 {
            (rec.db.half1_len as i16).wrapping_sub(rec.ovl_l) as isize
        } else {
            0
        };
        let len = rec.len.max(0) as usize;
        let mut out = vec![0i16; len];
        for (k, o) in out.iter_mut().enumerate() {
            let i = start + k as isize;
            if i >= 0 && (i as usize) < self.buf.len() {
                *o = self.buf[i as usize];
            }
        }
        if rec.vol != 100 {
            for s in out.iter_mut() {
                *s = sat16(*s as i32 * rec.vol / 100);
            }
        }
        Ok(out)
    }
}

/// `wav_ReadPitchMarks` 0x1002bbd0: `n_pm` bytes from `.upm` at `rec.upm`, doubled.
pub fn read_pitch_marks(bank: &Bank, uid: u32, rec: &Record) -> io::Result<Vec<i16>> {
    let all = bank.pitch_marks(uid, &rec.db)?;
    let off = rec.upm.wrapping_sub(rec.db.pm_pos) as usize;
    Ok((0..rec.n_pm as usize).map(|k| (all.get(off + k).copied().unwrap_or(0) as i16) << 1).collect())
}

/// What the smoother needs to know about the selected path around one record.
#[derive(Debug, Clone, Copy)]
pub struct PathInfo {
    /// Selected global unit of the previous / current / next target (None at the ends).
    pub prev: Option<u32>,
    pub cur: u32,
    pub next: Option<u32>,
    /// Half-type of the current target (0 whole, 1 left, 2 right).
    pub half: u8,
}

fn label(bank: &Bank, uid: u32) -> [u8; 7] {
    bank.label[uid as usize]
}

/// Left/right smoothing decision of `FUN_1002d230`. 0 = off, 1 or 2 = weight.
pub fn smooth_flags(bank: &Bank, pi: &PathInfo) -> (i16, i16) {
    let n_units = bank.len() as u32;
    let cur = pi.cur;
    let lc = label(bank, cur);
    let mut left: i16 = 2;
    match pi.prev {
        None => left = 0,
        Some(_) if lc[5] & 0x80 == 0x80 && (lc[5] & 0x38) > 0x17 => left = 0,
        Some(prev) => {
            let lp = label(bank, prev);
            if pi.half == 2 && prev == cur {
                left = 0;
            } else if prev == n_units - 1 {
                left = 0;
            } else if pi.half != 2 {
                if prev + 1 == cur && lp[6] & 0x80 != 0 {
                    left = 0;
                } else if lp[6] & 0x80 == 0 {
                    left = 0;
                } else if cur == 0 || label(bank, cur - 1)[6] & 0x80 == 0 {
                    left = 1;
                }
            }
        }
    }
    let mut right: i16 = 2;
    match pi.next {
        None => right = 0,
        Some(_) if lc[5] & 0x40 == 0x40 && (lc[5] & 7) > 2 => right = 0,
        Some(next) => {
            if pi.half == 1 && cur == next {
                right = 0;
            } else if next == 0 {
                right = 0;
            } else if pi.half != 1 {
                if cur + 1 == next && lc[6] & 0x80 != 0 {
                    right = 0;
                } else if label(bank, next - 1)[6] & 0x80 == 0 {
                    right = 0;
                } else if lc[6] & 0x80 == 0 {
                    right = 1;
                }
            }
        }
    }
    (left, right)
}

/// A neighbour unit as `FUN_1002d1e0` loads it: same mode and volume as the
/// current record.
fn load_neighbour(win: &mut DecodeWindow, bank: &Bank, uid: u32, cur: &Record) -> io::Result<(Vec<i16>, Vec<i16>)> {
    let pr = bank.primary(uid)?;
    let mut r = Record { mode: cur.mode, vol: cur.vol, ..Default::default() };
    r.fill(&pr, bank.file_idx[uid as usize]);
    let pcm = win.decode(bank, uid, &r)?;
    let pm = read_pitch_marks(bank, uid, &r)?;
    Ok((pcm, pm))
}

/// `FUN_1002d010`: falling half-window over the first min(p_old, p_new) samples
/// of a period, zero-padded to `p_new`.
fn fade_out_period(tmp: &mut [i16], src: &[i16], p_old: i32, p_new: i32) {
    if p_old < p_new {
        for k in 0..p_old.max(0) {
            tmp[k as usize] = wmul(fall(k, p_old), src[k as usize] as i32);
        }
        for k in p_old.max(0)..p_new {
            tmp[k as usize] = 0;
        }
    } else {
        for k in 0..p_new.max(0) {
            tmp[k as usize] = wmul(fall(k, p_new), src[k as usize] as i32);
        }
    }
}

/// `FUN_1002cdf0`: rising half-window over the last min(p_old, p_new) samples of
/// a period, right-aligned in a `p_new`-sample output.
fn fade_in_period(tmp: &mut [i16], src: &[i16], p_old: i32, p_new: i32) {
    if p_old < p_new {
        for k in 0..(p_new - p_old) {
            tmp[k as usize] = 0;
        }
        for k in 0..p_old.max(0) {
            tmp[(p_new - p_old + k) as usize] = wmul(rise(k, p_old), src[k as usize] as i32);
        }
    } else {
        for k in 0..p_new.max(0) {
            tmp[k as usize] = wmul(rise(k, p_new), src[(k - p_new + p_old) as usize] as i32);
        }
    }
}

fn get(v: &[i16], i: i32) -> i16 {
    if i >= 0 {
        v.get(i as usize).copied().unwrap_or(0)
    } else {
        0
    }
}

fn slice_at(v: &[i16], at: i32, n: i32) -> Vec<i16> {
    (0..n.max(0)).map(|k| get(v, at + k)).collect()
}

/// Result of the join smoother: new samples and new pitch periods for the record.
pub struct Smoothed {
    pub pcm: Vec<i16>,
    pub periods: Vec<i16>,
}

/// `FUN_1002d230`: pitch-synchronous blend of a unit's first/last few periods
/// with the natural recorded continuation of its neighbours. Updates `rec.len`,
/// `rec.ovl_l`, `rec.ovl_r` and returns the new samples; `None` when neither
/// edge qualifies.
pub fn smooth_joins(win: &mut DecodeWindow, bank: &Bank, pi: &PathInfo, rec: &mut Record) -> io::Result<Option<Smoothed>> {
    let (l6, l5) = smooth_flags(bank, pi);
    if l6 == 0 && l5 == 0 {
        return Ok(None);
    }
    let cur_pcm = win.decode(bank, pi.cur, rec)?;
    let cur_pm = read_pitch_marks(bank, pi.cur, rec)?;
    let npm = rec.n_pm as i32;

    let (mut left_pcm, mut left_pm, mut b_l, local_c) = (Vec::new(), Vec::new(), 0i32, 1i32);
    let mut local_c = local_c;
    if l6 != 0 {
        let prev = pi.prev.unwrap();
        let src = if pi.half != 2 { prev + 1 } else { prev };
        let (pcm, pm) = load_neighbour(win, bank, src, rec)?;
        let n_l = pm.len() as i32;
        b_l = n_l.min(npm).min(5);
        local_c = b_l * l6 as i32;
        left_pcm = pcm;
        left_pm = pm;
    }
    let (mut right_pcm, mut right_pm, mut b_r, mut s_r, mut n_r) = (Vec::new(), Vec::new(), 0i32, 1i32, 0i32);
    if l5 != 0 {
        let next = pi.next.unwrap();
        let src = if pi.half != 1 { next - 1 } else { next };
        let (pcm, pm) = load_neighbour(win, bank, src, rec)?;
        n_r = pm.len() as i32;
        b_r = n_r.min(npm).min(5);
        s_r = b_r * l5 as i32;
        right_pcm = pcm;
        right_pm = pm;
    }
    let w = (s_r * local_c) as i16 as i32;

    let n = npm.max(0) as usize;
    let mut wl = vec![0i32; n + 1];
    let mut wr = vec![0i32; n + 1];
    let mut wc = vec![0i32; n + 1];
    for k in 0..=n as i32 {
        wl[k as usize] = if k < b_l { ((b_l - k) * s_r) as i16 as i32 } else { 0 };
        wr[(npm - k) as usize] = if k < b_r { ((b_r - k) * local_c) as i16 as i32 } else { 0 };
    }
    for k in 0..=n {
        wc[k] = (w - wr[k] - wl[k]) as i16 as i32;
    }

    let mut new_p = vec![0i16; n];
    for k in 0..n {
        let pc = get(&cur_pm, k as i32) as i32;
        if wl[k] == 0 && wr[k + 1] == 0 {
            new_p[k] = pc as i16;
        } else {
            let mut acc = (wc[k + 1] + wc[k]) * pc;
            if wl[k] > 0 {
                acc += (wl[k] + wl[k + 1]) * get(&left_pm, k as i32) as i32;
            }
            if wr[k + 1] > 0 {
                acc += get(&right_pm, n_r - npm + k as i32) as i32 * (wr[k] + wr[k + 1]);
            }
            new_p[k] = ((acc + w) / (w * 2)) as i16;
        }
    }

    let mut out = vec![0i16; SMOOTH_BUF];
    let mut tmp = vec![0i16; 4096];
    let mut pos_r: i32 = 0;
    if l5 != 0 {
        for j in 0..(n_r - b_r).max(0) {
            pos_r += get(&right_pm, j) as i32;
        }
    }
    let (mut pos_c, mut pos_l, mut pos_o) = (0i32, 0i32, 0i32);
    let add = |out: &mut Vec<i16>, tmp: &[i16], at: i32, np: i32, weight: i32| {
        for j in 0..np.max(0) {
            let v = sat16(tmp[j as usize] as i32 * weight / w);
            let i = (at + j) as usize;
            if i < out.len() {
                out[i] = out[i].wrapping_add(v);
            }
        }
    };
    for k in 0..n {
        let ki = k as i32;
        let pc = get(&cur_pm, ki) as i32;
        let np = new_p[k] as i32;
        let r_idx = n_r - npm + ki;
        if wl[k] == 0 && wr[k + 1] == 0 {
            for j in 0..np.max(0) {
                let i = (pos_o + j) as usize;
                if i < out.len() {
                    out[i] = get(&cur_pcm, pos_c + j);
                }
            }
            pos_c += pc;
            pos_o += np;
            continue;
        }
        let cur_seg = slice_at(&cur_pcm, pos_c, pc.max(np) + 1);
        fade_out_period(&mut tmp, &cur_seg, pc, np);
        add(&mut out, &tmp, pos_o, np, wc[k]);
        if wl[k] > 0 {
            let pl = get(&left_pm, ki) as i32;
            let seg = slice_at(&left_pcm, pos_l, pl.max(np) + 1);
            fade_out_period(&mut tmp, &seg, pl, np);
            add(&mut out, &tmp, pos_o, np, wl[k]);
        }
        if wr[k] > 0 {
            let pr = get(&right_pm, r_idx) as i32;
            let seg = slice_at(&right_pcm, pos_r, pr.max(np) + 1);
            fade_out_period(&mut tmp, &seg, pr, np);
            add(&mut out, &tmp, pos_o, np, wr[k]);
        }
        fade_in_period(&mut tmp, &cur_seg, pc, np);
        add(&mut out, &tmp, pos_o, np, wc[k + 1]);
        if wl[k + 1] > 0 {
            let pl = get(&left_pm, ki) as i32;
            let seg = slice_at(&left_pcm, pos_l, pl.max(np) + 1);
            fade_in_period(&mut tmp, &seg, pl, np);
            add(&mut out, &tmp, pos_o, np, wl[k + 1]);
        }
        if wr[k + 1] > 0 {
            let pr = get(&right_pm, r_idx) as i32;
            let seg = slice_at(&right_pcm, pos_r, pr.max(np) + 1);
            fade_in_period(&mut tmp, &seg, pr, np);
            add(&mut out, &tmp, pos_o, np, wr[k + 1]);
        }
        if wl[k] > 0 {
            pos_l += get(&left_pm, ki) as i32;
        }
        if wr[k + 1] > 0 {
            pos_r += get(&right_pm, r_idx) as i32;
        }
        pos_c += pc;
        pos_o += np;
    }
    rec.len = pos_o;
    rec.ovl_l = new_p.first().copied().unwrap_or(0);
    rec.ovl_r = if n > 0 { new_p[n - 1] } else { 0 };
    Ok(Some(Smoothed { pcm: out, periods: new_p }))
}

/// Overlap-add state carried from one unit to the next inside a phrase
/// (`SS+0x47708` tail length, `TIB+0x10cd58` tail samples).
#[derive(Default)]
pub struct Tail {
    pub samples: Vec<i16>,
}

/// `wav_OlaUnitUnmodified` 0x1002aac0: pitch and duration at 100%.
pub fn ola_unmodified(tail: &mut Tail, src: &[i16], rec: &Record, flush: bool, out: &mut Vec<i16>) {
    let len = rec.len;
    let l = rec.ovl_l as i32;
    let tn = tail.samples.len() as i32;
    let base = out.len();
    let s = |i: i32| get(src, i) as i32;
    if !(tn > l && l < 1) {
        let mut a = vec![0i16; l.max(0) as usize];
        if tn > 0 {
            let lim = tn.min(l);
            let den = if tn <= l { tn } else { l };
            for k in 0..lim.max(0) {
                a[k as usize] = wmul(fall(k, den), tail.samples[k as usize] as i32);
            }
        }
        for k in 0..l.max(0) {
            out.push(wmul(rise(k, l), s(k)).wrapping_add(a[k as usize]));
        }
    }
    let lw = l.max(0);
    let mid = (len - rec.ovl_r as i32 - l).max(0);
    for k in 0..mid {
        out.push(get(src, lw + k));
    }
    debug_assert_eq!(out.len(), base + (lw + mid) as usize);
    let new_tn = (len - mid - l) as i16 as i32;
    tail.samples.clear();
    if !flush {
        for k in 0..new_tn.max(0) {
            tail.samples.push(get(src, len - new_tn + k));
        }
        return;
    }
    for k in 0..new_tn.max(0) {
        out.push(wmul(fall(k, new_tn), s(len - new_tn + k)));
    }
}

/// One selected target as `wav_BuildUnitRecords` reads it (`TIB+0xec628+6t`
/// plus the primary record `TIB+0x11b24c+0x18t`).
#[derive(Debug, Clone, Copy)]
pub struct Target {
    pub word: u8,
    pub phone: u8,
    /// 0 whole, 1 first half, 2 second half.
    pub half: u8,
    pub db: Primary,
    pub file: u8,
}

/// Per-word data the record builder takes from the sentence structure.
#[derive(Debug, Clone, Copy)]
pub struct WordInfo {
    /// Break code after the word (`W+0x3bd`): `]` `\` `[` `Z` `^`.
    pub brk: u8,
    pub pitch: i32,
    pub speed: i32,
    pub volume: i32,
}

/// Sentence-level pause state (`FUN_1002e890` inputs).
#[derive(Debug, Clone)]
pub struct PauseState {
    /// `*(SS+0x4770c)`: explicit pause in ms before word w, or -1.
    pub word_pauses: Vec<i32>,
    /// `TIB+0x122850`: the sentence continues the previous one (no leading pause).
    pub cont_flag: bool,
    /// `TIB+0x122440`: sentence-final pause in ms (0 = none).
    pub final_pause_ms: i32,
    /// Channel values used for silence records.
    pub ch_pitch: i32,
    pub ch_volume: i32,
    pub ch_comma_ms: i32,
    /// Start offset of word 0 (`SS+0x64c`), for the sync-span reset below.
    pub word0_start: i32,
    /// Set by `pause_before` when an explicit pause precedes word 0 of a continued
    /// sentence: `FUN_1002e890` then resets the sync span (`TIB+0x4777c/0x47780`)
    /// to `(word0_start, word0_start)`. Consumed by the caller of `sync_phrase`.
    pub span_reset: Option<(i32, i32)>,
}

/// `FUN_1002e890`: pause in samples before word `w` (written to `SS+8`).
pub fn pause_before(st: &mut PauseState, words: &[WordInfo], w: usize) -> i32 {
    pause_before_brk(st, words, w, b'Z', b'^', b'[')
}

/// As `pause_before`, with the sentence-final/exclaim, sentence-final-alt and
/// comma break bytes given explicitly. Violeta's whole break alphabet is
/// English's shifted `-0x14` (`0x5a/0x5e/0x5b -> 0x46/0x4a/0x47`), confirmed
/// by `reveng\VIOLETA.md`.
pub fn pause_before_brk(st: &mut PauseState, words: &[WordInfo], w: usize, brk_a: u8, brk_b: u8, brk_comma: u8) -> i32 {
    let p = st.word_pauses.get(w).copied().unwrap_or(-1);
    if p >= 0 {
        if w == 0 {
            if st.cont_flag {
                if st.word_pauses[0] > 0 {
                    st.span_reset = Some((st.word0_start, st.word0_start));
                }
                st.cont_flag = false;
            }
            return st.word_pauses[0] << 4;
        }
        return p << 4;
    }
    if w == 0 {
        if st.cont_flag {
            st.cont_flag = false;
            return 0;
        }
        return 11000;
    }
    let brk = words[w - 1].brk;
    if brk == brk_a || brk == brk_b {
        11000
    } else if brk == brk_comma {
        st.ch_comma_ms << 4
    } else {
        0
    }
}

fn scaled_pause(samples: i32, speed: i32) -> i32 {
    (dur_factor(speed) * samples + 50) / 100
}

/// `wav_BuildUnitRecords` 0x1002c220 for one phrase. `first_word` is the phrase's
/// first word index (`SS+0xc+g*0x10`); `last_phrase` adds the sentence-final
/// pause. `center(word, phone)` is the C byte of that phone's 7-byte context.
pub fn build_records(
    targets: &[Target],
    words: &[WordInfo],
    center: &dyn Fn(usize, usize) -> u8,
    first_word: u16,
    last_phrase: bool,
    st: &mut PauseState,
) -> Vec<Record> {
    build_records_lang(targets, words, center, first_word, last_phrase, st, 0x5a, b'Z', b'^', b'[')
}

/// As `build_records`, with the silence/pad centre byte and the three break
/// bytes (sentence-final, sentence-final-alt, comma) given explicitly for
/// Violeta's shifted break alphabet.
#[allow(clippy::too_many_arguments)]
pub fn build_records_lang(
    targets: &[Target],
    words: &[WordInfo],
    center: &dyn Fn(usize, usize) -> u8,
    first_word: u16,
    last_phrase: bool,
    st: &mut PauseState,
    pad: u8,
    brk_a: u8,
    brk_b: u8,
    brk_comma: u8,
) -> Vec<Record> {
    let mut recs: Vec<Record> = Vec::new();
    for (t, tg) in targets.iter().enumerate() {
        let boundary = if t == 0 { tg.word as u16 == first_word } else { targets[t - 1].word != tg.word };
        if boundary {
            let pause = pause_before_brk(st, words, tg.word as usize, brk_a, brk_b, brk_comma);
            if pause > 0 {
                let len = scaled_pause(pause, words[tg.word as usize].speed);
                recs.push(Record::silence_pad(len, first_word, st.ch_pitch, st.ch_volume, pad));
            }
        }
        let w = &words[tg.word as usize];
        let mut r = Record {
            typ: TYPE_UNIT,
            mode: tg.half,
            unit: t as i16,
            word: tg.word as u16,
            phone: tg.phone as u16,
            center: center(tg.word as usize, tg.phone as usize),
            pitch: w.pitch,
            dur: w.speed,
            vol: w.volume,
            ..Default::default()
        };
        r.fill(&tg.db, tg.file);
        recs.push(r);
        if recs.len() > 1099 {
            recs.truncate(1100);
            return recs;
        }
    }
    if st.final_pause_ms > 0 && last_phrase {
        let prev_word = recs.last().map(|r| r.word).unwrap_or(0);
        let speed = words.get(prev_word as usize).map(|w| w.speed).unwrap_or(100);
        let len = scaled_pause(st.final_pause_ms << 4, speed);
        recs.push(Record::silence_pad(len, prev_word, st.ch_pitch, st.ch_volume, pad));
    }
    recs
}

/// Everything `generate_phrase` needs per record besides the bank.
pub struct PhraseInput<'a> {
    pub records: &'a [Record],
    /// Selected global unit per target index.
    pub uids: &'a [u32],
    /// Half-type per target index.
    pub halves: &'a [u8],
}

/// Generate the audio of one phrase (`wav_GeneratePhrase`), appending to `out`.
pub fn generate_phrase(win: &mut DecodeWindow, bank: &Bank, input: &PhraseInput, out: &mut Vec<i16>) -> io::Result<()> {
    generate_phrase_traced(win, bank, input, out, None)
}

/// What the chunker needs to know about one generated record.
#[derive(Debug, Clone, Copy)]
pub struct RecTrace {
    /// First output sample of this record.
    pub start: usize,
    pub silence: bool,
    /// Length after the join smoother (samples).
    pub len: i32,
    /// `wav_GeneratePhrase`'s size estimate `((dfac*len + 50)/100)*2` in bytes.
    pub est_bytes: i32,
}

/// As `generate_phrase`, recording every record in `trace`.
pub fn generate_phrase_traced(
    win: &mut DecodeWindow,
    bank: &Bank,
    input: &PhraseInput,
    out: &mut Vec<i16>,
    mut trace: Option<&mut Vec<RecTrace>>,
) -> io::Result<()> {
    let mut tail = Tail::default();
    let n = input.records.len();
    let nt = input.uids.len();
    for i in 0..n {
        let start = out.len();
        let mut rec = input.records[i].clone();
        if rec.is_silence() {
            out.extend(std::iter::repeat(0i16).take(rec.len.max(0) as usize));
            tail.samples.clear();
            if let Some(tr) = trace.as_deref_mut() {
                tr.push(RecTrace { start, silence: true, len: rec.len, est_bytes: rec.len * 2 });
            }
            continue;
        }
        let t = rec.unit as usize;
        let pi = PathInfo {
            prev: if t > 0 { Some(input.uids[t - 1]) } else { None },
            cur: input.uids[t],
            next: if t + 1 < nt { Some(input.uids[t + 1]) } else { None },
            half: input.halves[t],
        };
        let smoothed = smooth_joins(win, bank, &pi, &mut rec)?;
        let (src, periods) = match smoothed {
            Some(s) => (s.pcm, Some(s.periods)),
            None => (win.decode(bank, pi.cur, &rec)?, None),
        };
        if let Some(tr) = trace.as_deref_mut() {
            let est = (dur_factor(rec.dur) * rec.len + 50) / 100 * 2;
            tr.push(RecTrace { start, silence: false, len: rec.len, est_bytes: est });
        }
        let flush = i + 1 >= n || input.records[i + 1].is_silence();
        if rec.pitch == 100 && dur_factor(rec.dur) == 100 {
            ola_unmodified(&mut tail, &src, &rec, flush, out);
        } else {
            let pm = match periods {
                Some(p) => p,
                None => read_pitch_marks(bank, pi.cur, &rec)?,
            };
            let epochs = build_epochs(&pm, &rec);
            psola(&mut tail, &src, &rec, &epochs, flush, out);
        }
    }
    Ok(())
}

/// One word entry of the 600-slot sync ring (`*(TIB+0x47774)`, 0x24 bytes each),
/// as `FUN_1002c530` creates them and `wav_GeneratePhrase` fills in the sample
/// counts. Silence records get their own entry carrying the previous word's span.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncWord {
    /// +0x0c / +0x10 character span in the original input (inclusive end).
    pub start: i32,
    pub end: i32,
    /// +0x08 output samples.
    pub samples: i32,
    /// +0x00 / +0x04 phone slots: (lip-sync code, samples). Code 40 is silence.
    pub phones: Vec<(u8, i32)>,
    pub silence: bool,
}

/// State `FUN_1002c530` keeps across phrases: the last word span (`TIB+0x4777c/0x47780`).
#[derive(Debug, Clone, Default)]
pub struct SyncState {
    pub last_span: (i32, i32),
}

/// Word/phone sync entries for one phrase. `spans[w]` is word w's character span;
/// `produced[k]` the samples record k added to the output (from the trace).
pub fn sync_phrase(
    recs: &[Record],
    spans: &[(i32, i32)],
    produced: &[i32],
    st: &mut SyncState,
    out: &mut Vec<SyncWord>,
) {
    let base = out.len();
    let mut entry_of = vec![0usize; recs.len()];
    for (k, r) in recs.iter().enumerate() {
        let new = k == 0 || recs[k - 1].word != r.word || recs[k - 1].typ != r.typ;
        if new {
            let (start, end) = if r.is_silence() {
                st.last_span
            } else {
                let s = spans.get(r.word as usize).copied().unwrap_or((0, 0));
                st.last_span = s;
                s
            };
            out.push(SyncWord { start, end, silence: r.is_silence(), ..Default::default() });
        }
        entry_of[k] = out.len() - 1;
    }
    for (k, r) in recs.iter().enumerate() {
        let e = &mut out[entry_of[k]];
        let new = k == 0 || recs[k - 1].word != r.word || recs[k - 1].phone != r.phone || recs[k - 1].typ != r.typ;
        if new {
            e.phones.push((tables::LIPSYNC_CODE[r.center as usize], 0));
        }
        let n = produced[k];
        e.samples += n;
        e.phones.last_mut().unwrap().1 += n;
    }
    debug_assert!(out.len() >= base);
}

/// The text `VT_TextToLipSyncLog_ENG` writes (`FUN_1001e0c0` per entry, totals
/// from the close routine). `text` is the original input as the engine got it.
pub fn lipsync_log(words: &[SyncWord], text: &[u8]) -> Vec<u8> {
    let mut s: Vec<u8> = Vec::new();
    let (mut words_total, mut phones_total) = (0i64, 0i64);
    for w in words {
        for &(code, n) in &w.phones {
            if code == 40 {
                s.extend_from_slice(format!("{:2} (sil) : {} \n", code, n).as_bytes());
            } else {
                let name = tables::LIPSYNC_NAMES.get(code as usize).copied().unwrap_or("");
                s.extend_from_slice(format!("{:2} ({:>3}) : {} \n", code, name, n).as_bytes());
            }
            phones_total += n as i64;
        }
        s.extend_from_slice(b"-------------------------------\n");
        let a = (w.start.max(0) as usize).min(text.len());
        let b = ((w.end + 1).max(0) as usize).min(text.len()).max(a);
        s.extend_from_slice(format!("{} - {} ", w.start, w.end).as_bytes());
        s.extend_from_slice(&text[a..b]);
        s.extend_from_slice(format!(" : {} \n", w.samples).as_bytes());
        s.extend_from_slice(b"===============================\n\n");
        words_total += w.samples as i64;
    }
    s.extend_from_slice(format!("Total  Word Length : {}\n", words_total).as_bytes());
    s.extend_from_slice(format!("Total Phone Length : {}\n", phones_total).as_bytes());
    // The log is opened in text mode ("w"), so the CRT turns every \n into \r\n.
    let mut crlf = Vec::with_capacity(s.len() + s.len() / 16);
    for b in s {
        if b == b'\n' {
            crlf.push(b'\r');
        }
        crlf.push(b);
    }
    crlf
}

/// Byte limit of one engine output chunk (`wav_GeneratePhrase`, `FUN_100200c0`).
pub const CHUNK_BYTES: i32 = 60_000;

/// Chunk boundaries (sample offsets where each chunk ends) the engine would
/// produce for records traced across a whole request, in order. A speech record
/// starts a new chunk when `chunk + 2000 + est_bytes > 60000`; a silence record
/// is split exactly at 60000 bytes. Phrase and sentence ends do not end a chunk.
pub fn chunk_ends(trace: &[RecTrace], total: usize) -> Vec<usize> {
    let mut ends = Vec::new();
    let mut chunk_start = 0usize;
    for (k, r) in trace.iter().enumerate() {
        let end = trace.get(k + 1).map(|n| n.start).unwrap_or(total);
        let chunk = ((r.start - chunk_start) * 2) as i32;
        if r.silence {
            let mut pos = r.start;
            let mut bytes = chunk;
            let mut left = ((end - r.start) * 2) as i32;
            while bytes + left > CHUNK_BYTES {
                let fill = CHUNK_BYTES - bytes;
                pos += (fill / 2) as usize;
                left -= fill;
                ends.push(pos);
                chunk_start = pos;
                bytes = 0;
            }
        } else if chunk + 2000 + r.est_bytes > CHUNK_BYTES && r.start > chunk_start {
            ends.push(r.start);
            chunk_start = r.start;
        }
    }
    if total > chunk_start {
        ends.push(total);
    }
    ends
}

/// One analysis epoch (`wav_BuildEpochTable` 0x1002bc60, 5 ints).
#[derive(Debug, Clone, Copy)]
pub struct Epoch {
    pub pos: i32,
    pub p_left: i32,
    pub p_right: i32,
    pub dfac: i32,
    pub pitch: i32,
}

pub fn build_epochs(pm: &[i16], rec: &Record) -> Vec<Epoch> {
    let dfac = dur_factor(rec.dur);
    if rec.n_pm == 1 {
        return vec![Epoch { pos: 0, p_left: rec.ovl_l as i32 - 1, p_right: 1, dfac, pitch: rec.pitch }];
    }
    let mut v = Vec::new();
    let mut pos = 0;
    for k in 0..(rec.n_pm as usize).saturating_sub(1) {
        let pl = get(pm, k as i32) as i32;
        v.push(Epoch { pos, p_left: pl, p_right: get(pm, k as i32 + 1) as i32, dfac, pitch: rec.pitch });
        pos += pl;
    }
    v
}

/// `wav_FindNearestEpoch` 0x1002bd20.
pub fn nearest_epoch(t: i32, ep: &[Epoch]) -> Option<usize> {
    let last = ep.last()?;
    if t >= last.pos + last.p_left + last.p_right {
        return None;
    }
    let (mut best, mut bd) = (0usize, 20000i32);
    for (k, e) in ep.iter().enumerate() {
        let d = (e.p_left - t + e.pos).abs();
        if d < bd {
            best = k;
            bd = d;
        }
    }
    Some(best)
}

/// `wav_PsolaUnit` 0x1002afb0 (modified path). See WAVEFORM.md.
///
/// `op` is the committed output position inside this unit; after every
/// cross-fade the analysis period to the right of the epoch is copied verbatim
/// behind `op` and becomes the fade-out material of the next cross-fade.
pub fn psola(tail: &mut Tail, src: &[i16], rec: &Record, ep: &[Epoch], flush: bool, out: &mut Vec<i16>) {
    if rec.pitch == 100 && rec.dur == 100 {
        ola_unmodified(tail, src, rec, flush, out);
        return;
    }
    let base = out.len();
    let mut dst: Vec<i16> = Vec::new();
    let put = |dst: &mut Vec<i16>, i: i32, v: i16| {
        let i = i as usize;
        if dst.len() <= i {
            dst.resize(i + 1, 0);
        }
        dst[i] = v;
    };
    let at = |dst: &Vec<i16>, i: i32| get(dst, i);
    let s = |i: i32| get(src, i) as i32;
    let tail_n = tail.samples.len() as i32;
    let mut op: i32 = 0;
    let mut prev_pr: i32 = 0;
    let mut lim: i32 = ep.first().map(|e| e.p_left).unwrap_or(0);
    let mut dfac = dur_factor(rec.dur);
    let mut cur = nearest_epoch(lim * 100 / dfac, ep);
    while let Some(k) = cur {
        let e = ep[k];
        dfac = e.dfac;
        let pl = e.p_left;
        let mut l = pl * 100 / e.pitch;
        l = l.max(pl / 16);
        l = l.min(pl * 31 / 16);
        if l - pl + op > lim {
            l = pl - op + lim;
        }
        let mut a = vec![0i16; l.max(0) as usize];
        if op != 0 {
            let n = prev_pr.min(l);
            let den = if l >= prev_pr { prev_pr } else { l };
            for j in 0..n.max(0) {
                a[j as usize] = wmul(fall(j, den), at(&dst, op + j) as i32);
            }
        } else if tail_n != 0 {
            let n = tail_n.min(l);
            let den = if l >= tail_n { tail_n } else { l };
            for j in 0..n.max(0) {
                a[j as usize] = wmul(fall(j, den), tail.samples[j as usize] as i32);
            }
        }
        let mut b = vec![0i16; l.max(0) as usize];
        if l >= pl {
            for j in 0..pl.max(0) {
                b[(l - pl + j) as usize] = wmul(rise(j, pl), s(e.pos + j));
            }
        } else {
            for j in 0..l.max(0) {
                b[j as usize] = wmul(rise(j, l), s(e.pos - l + pl + j));
            }
        }
        for j in 0..l.max(0) {
            put(&mut dst, op + j, b[j as usize].wrapping_add(a[j as usize]));
        }
        let op1 = op + l;
        for j in 0..e.p_right.max(0) {
            put(&mut dst, op1 + j, get(src, e.pos + pl + j));
        }
        op = op1;
        prev_pr = e.p_right;
        lim = op1 + e.p_right;
        cur = nearest_epoch(lim * 100 / dfac, ep);
    }
    if op == 0 {
        return;
    }
    tail.samples.clear();
    if !flush {
        for j in 0..prev_pr.max(0) {
            tail.samples.push(at(&dst, op + j));
        }
        out.extend((0..op).map(|j| at(&dst, j)));
        return;
    }
    for j in 0..prev_pr.max(0) {
        let v = wmul(fall(j, prev_pr), at(&dst, op + j) as i32);
        put(&mut dst, op + j, v);
    }
    out.extend((0..lim).map(|j| at(&dst, j)));
    debug_assert!(out.len() == base + lim.max(0) as usize);
}
