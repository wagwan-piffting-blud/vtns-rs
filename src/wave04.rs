//! Waveform generation of the 2004 Paul build (vt_eng.dll 2005-03-31): `wav_GeneratePhrase` 0x100203c0
//! and callees. Records are a struct of arrays (`Record`), the unit entry a 0x14-byte block, and there is
//! no join smoother. Speech records go through the unmodified overlap-add `FUN_10020f70` when pitch and
//! duration factor are both 100, else through the PSOLA pass `FUN_100211b0` (epochs `FUN_10021950`,
//! nearest epoch `FUN_10021a10`). The duration array holds the factor itself (10000 / speed percent).

use std::io;

use crate::bank::{Bank, Primary};
use crate::wave::{hann, wmul, DecodeWindow, Record, TYPE_SILENCE, TYPE_UNIT};

/// `DAT_1005b1ac` table length; every window index is scaled by `HANN_LEN - 1`.
const HANN_LAST: i32 = crate::wave::HANN_LEN as i32 - 1;

/// The 0x14-byte unit entry the record builder points at.
pub fn primary_from_entry(b: &[u8]) -> (Primary, u8) {
    let le16 = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
    let le32 = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    (
        Primary {
            pcm_pos: le32(0),
            pm_pos: le32(4),
            half1_len: le16(8),
            half2_len: le16(0xa),
            coded_size: le16(0xc),
            pm1: b[0xe],
            pm2: b[0xf],
            periods: [b[0x10], b[0x11], b[0x12]],
        },
        b[0x13],
    )
}

#[inline]
fn sat(v: i32) -> i16 {
    crate::wave::sat16(v)
}

/// Silence record length: `(len * dur + 50) / 100` samples.
pub fn silence_samples(rec: &Record) -> i32 {
    (rec.len * rec.dur + 50) / 100
}

/// Overlap state between consecutive speech records of a phrase (`TIB+0xf2fd4`, length at `SS+0x52860`).
#[derive(Default)]
pub struct Tail04 {
    pub samples: Vec<i16>,
    pub len: i32,
}

fn clamp_f32(v: f32) -> i16 {
    if v < 32767.0 && !(v <= -32768.0) {
        v as i32 as i16
    } else if v < 32767.0 {
        -32767
    } else {
        32766
    }
}

/// `FUN_10020f70`: pitch and duration factor at 100. The first `tail.len` samples cross-fade the
/// previous record's saved tail with this record's start; the last `ovl_r` samples become the next tail
/// unless this is the last speech record before a silence or the phrase end.
pub fn ola_unmodified04(tail: &mut Tail04, src: &[i16], rec: &Record, has_next: bool, region: &mut Vec<i16>) -> i32 {
    let len = rec.len;
    let tl = tail.len;
    let n = if has_next { len - rec.ovl_r as i32 } else { len };
    let g = |i: i32| -> i16 { if i >= 0 { src.get(i as usize).copied().unwrap_or(0) } else { 0 } };
    for j in 0..n.max(0) {
        let v = if j < tl {
            let den = 2 * tl + 1;
            let hf = hann((tl + j + 1) * HANN_LAST / den) as f64;
            let hr = hann((j + 1) * HANN_LAST / den) as f64;
            let t = tail.samples.get(j as usize).copied().unwrap_or(0) as f64;
            clamp_f32((t * hf + g(j) as f64 * hr) as f32)
        } else {
            g(j)
        };
        region.push(v);
    }
    if has_next {
        let k = rec.ovl_r as i32;
        tail.samples = (0..k.max(0)).map(|i| g(len - k + i)).collect();
        tail.len = k;
    } else {
        tail.len = 0;
    }
    n.max(0)
}

/// One analysis epoch (`FUN_10021950`, 5 ints).
#[derive(Debug, Clone, Copy)]
pub struct Epoch04 {
    pub pos: i32,
    pub pl: i32,
    pub pr: i32,
    /// Duration factor (the record's `dur` array value).
    pub speed: i32,
    pub pitch: i32,
}

/// `FUN_10021950`: `pm` are the record's pitch marks already doubled (`FUN_10021890`).
pub fn build_epochs04(pm: &[i16], rec: &Record) -> Vec<Epoch04> {
    let g = |i: usize| pm.get(i).copied().unwrap_or(0) as i32;
    match rec.n_pm {
        0 => Vec::new(),
        1 => vec![Epoch04 { pos: 0, pl: g(0) - 1, pr: 1, speed: rec.dur, pitch: rec.pitch }],
        n => {
            let mut v = Vec::new();
            let mut pos = 0;
            for k in 0..(n as usize - 1) {
                v.push(Epoch04 { pos, pl: g(k), pr: g(k + 1), speed: rec.dur, pitch: rec.pitch });
                pos += g(k);
            }
            v
        }
    }
}

/// `FUN_10021a10`.
pub fn nearest_epoch04(t: i32, ep: &[Epoch04]) -> Option<usize> {
    let last = ep.last()?;
    if t >= last.pos + last.pl + last.pr {
        return None;
    }
    let (mut best, mut bd) = (0usize, 20000i32);
    for (k, e) in ep.iter().enumerate() {
        let d = (e.pl - t + e.pos).abs();
        if d < bd {
            best = k;
            bd = d;
        }
    }
    Some(best)
}

struct Region {
    v: Vec<i16>,
}

impl Region {
    fn put(&mut self, i: i32, x: i16) {
        let i = i as usize;
        if self.v.len() <= i {
            self.v.resize(i + 1, 0);
        }
        self.v[i] = x;
    }

    fn at(&self, i: i32) -> i16 {
        if i >= 0 { self.v.get(i as usize).copied().unwrap_or(0) } else { 0 }
    }
}

/// `FUN_100211b0`: pitch-synchronous overlap-add of Hann-windowed grains of `pl + pr` samples. Returns the
/// number of samples committed to the output (`op` while a tail is kept for the next record, else `lim`).
pub fn psola04(tail: &mut Tail04, src: &[i16], ep: &[Epoch04], has_next: bool, region: &mut Vec<i16>) -> i32 {
    let tl0 = tail.len;
    let mut op: i32 = 0;
    let mut lim: i32 = match ep.first() {
        Some(e) => e.pl,
        None => return 0,
    };
    let mut out = Region { v: Vec::new() };
    let s = |i: i32| -> f64 { if i >= 0 { src.get(i as usize).copied().unwrap_or(0) as f64 } else { 0.0 } };
    let mut cur = nearest_epoch04(lim * 100 / ep[0].speed, ep);
    let mut last: Option<Epoch04> = None;
    while let Some(k) = cur {
        let e = ep[k];
        last = Some(e);
        let (pl, pr) = (e.pl, e.pr);
        let total = pl + pr;
        let mut l = pl * 100 / e.pitch;
        l = l.max(pl / 16);
        l = l.min(pl * 31 / 16);
        if lim < (l - pl) + op {
            l = (pl - op) + lim;
        }
        let mut grain: Vec<i16> = Vec::with_capacity(total.max(0) as usize);
        for i in 0..total.max(0) {
            let w = if i < pl { hann((i + 1) * HANN_LAST / (pl * 2 + 1)) } else { hann((pr - pl + 1 + i) * HANN_LAST / (pr * 2 + 1)) };
            let v = w as f64 * s(e.pos + i);
            grain.push(if 32767.0 <= v || v <= -32768.0 {
                if v < 32767.0 { -32767 } else { 32766 }
            } else {
                v as i32 as i16
            });
        }
        let start = (l - pl) + op;
        let d = l - pl;
        if d < 0 {
            if op != 0 {
                for t in 0..total {
                    let q = start + t;
                    if q >= 0 {
                        let g = grain[t as usize];
                        out.put(q, if q < lim { sat(out.at(q) as i32 + g as i32) } else { g });
                    }
                }
                op += l;
                lim = start + total;
            } else if tl0 == 0 {
                for t in 0..total {
                    out.put(t, grain[t as usize]);
                }
                op = pl;
                lim = total;
            } else {
                let n = d + total;
                for t in 0..n {
                    let g = grain[(t - d) as usize];
                    out.put(t, if t < tail.len { sat(tail.samples.get(t as usize).copied().unwrap_or(0) as i32 + g as i32) } else { g });
                }
                op = l;
                lim = n;
            }
        } else {
            if op == 0 {
                if tl0 == 0 {
                    for t in 0..d {
                        out.put(t, 0);
                    }
                    for t in 0..total {
                        out.put(d + t, grain[t as usize]);
                    }
                } else if tail.len < d {
                    for t in 0..tail.len {
                        out.put(t, tail.samples.get(t as usize).copied().unwrap_or(0));
                    }
                    for t in tail.len..d {
                        out.put(t, 0);
                    }
                    for t in 0..total {
                        out.put(d + t, grain[t as usize]);
                    }
                } else {
                    for t in 0..d {
                        out.put(t, tail.samples.get(t as usize).copied().unwrap_or(0));
                    }
                    for t in 0..total {
                        let g = grain[t as usize];
                        out.put(start + t, if t < tail.len - d { sat(tail.samples.get((d + t) as usize).copied().unwrap_or(0) as i32 + g as i32) } else { g });
                    }
                }
            } else {
                for t in 0..total {
                    let q = start + t;
                    let g = grain[t as usize];
                    out.put(q, if q < lim { sat(out.at(q) as i32 + g as i32) } else { g });
                }
            }
            op += l;
            lim = start + total;
        }
        cur = nearest_epoch04(lim * 100 / e.speed, ep);
    }
    if op == 0 {
        return 0;
    }
    let e = last.unwrap();
    tail.len = e.pr;
    if has_next {
        tail.samples = (0..e.pr.max(0)).map(|i| out.at(lim - e.pr + i)).collect();
        region.extend((0..op).map(|i| out.at(i)));
        op
    } else {
        region.extend((0..lim).map(|i| out.at(i)));
        lim
    }
}

/// One generated record, for the chunker.
#[derive(Debug, Clone, Copy)]
pub struct RecTrace04 {
    pub start: usize,
    pub silence: bool,
}

pub struct PhraseInput04<'a> {
    pub records: &'a [Record],
    /// Global unit per record (ignored for silence).
    pub uids: &'a [u32],
}

/// `wav_GeneratePhrase` for one phrase: appends the PCM of every record to `out`. `win` is the request's
/// persistent decode window.
pub fn generate_phrase04(
    win: &mut DecodeWindow,
    bank: &Bank,
    input: &PhraseInput04,
    out: &mut Vec<i16>,
    mut trace: Option<&mut Vec<RecTrace04>>,
) -> io::Result<()> {
    let mut tail = Tail04::default();
    let n = input.records.len();
    for i in 0..n {
        let rec = &input.records[i];
        if let Some(tr) = trace.as_deref_mut() {
            tr.push(RecTrace04 { start: out.len(), silence: rec.typ == TYPE_SILENCE });
        }
        if rec.typ == TYPE_SILENCE {
            out.extend(std::iter::repeat(0i16).take(silence_samples(rec).max(0) as usize));
            tail.len = 0;
            continue;
        }
        debug_assert_eq!(rec.typ, TYPE_UNIT);
        let uid = input.uids[i];
        let src = win.decode(bank, uid, rec)?;
        let has_next = i + 1 < n && input.records[i + 1].typ != TYPE_SILENCE;
        let mut region = Vec::new();
        if rec.pitch == 100 && rec.dur == 100 {
            ola_unmodified04(&mut tail, &src, rec, has_next, &mut region);
        } else {
            let pm: Vec<i16> = crate::wave::read_pitch_marks(bank, uid, rec)?;
            let ep = build_epochs04(&pm, rec);
            psola04(&mut tail, &src, &ep, has_next, &mut region);
        }
        out.extend(region);
    }
    Ok(())
}

/// One selected unit of a word (6-byte target list entry at word `+0xec`, 0x14-byte unit entry at `+0xe8`).
#[derive(Debug, Clone, Copy)]
pub struct Target04 {
    pub phone: u8,
    /// 0 whole, 1 first half, 2 second half.
    pub half: u8,
    pub db: Primary,
    pub file: u8,
    /// Centre byte of the phone's 5-byte context (`word + 0x175 + 5 * phone`).
    pub center: u8,
    /// Global unit the selector chose (what `DecodeWindow::decode` needs; the engine holds a file handle).
    pub uid: u32,
}

/// What the record builder reads of one word: break code, selected units, and the per-word rate arrays
/// (`SS+0x52870` pauses, `+0x52874` duration factor, `+0x52878` volume, `+0x5287c` pitch).
#[derive(Debug, Clone, Default)]
pub struct Word04 {
    pub brk: u8,
    pub targets: Vec<Target04>,
    pub pause: i32,
    pub dur: i32,
    pub vol: i32,
    pub pitch: i32,
}

/// Speaker values (`spk + 0x3828 pitch / 0x3824 duration factor / 0x382c volume / 0x3834 pause`); the
/// silence records carry pitch, duration factor and volume.
#[derive(Debug, Clone, Copy)]
pub struct Channel04 {
    pub pitch: i32,
    pub dur: i32,
    pub vol: i32,
    pub pause: i32,
}

impl Default for Channel04 {
    fn default() -> Self {
        Channel04 { pitch: 100, dur: 100, vol: 100, pause: 687 }
    }
}

impl Channel04 {
    /// `VT_SetPitchSpeedVolumePause_ENG` 0x1001ede0: a negative value (speed: non-positive) keeps the
    /// current one. Speed is stored as the factor `10000 / speed`, clamped to 25..=200.
    pub fn apply(&mut self, pitch: i32, speed: i32, volume: i32, pause: i32) {
        if pitch >= 0 {
            self.pitch = pitch.clamp(50, 200);
        }
        if speed > 0 {
            self.dur = (10000 / speed).clamp(25, 200);
        }
        if volume >= 0 {
            self.vol = volume.clamp(0, 500);
        }
        if pause >= 0 {
            self.pause = pause.min(20000);
        }
    }
}

/// `FUN_100224f0`: samples of silence before word `w` (`SS+8`). `cont` is `TIB+0x1132b8`: set until the
/// first word 0 consumes it. The comma pause is fixed at 0xc80.
pub fn pause_samples04(words: &[Word04], w: usize, cont: &mut bool) -> i32 {
    let p = words[w].pause;
    let v = if p >= 0 {
        if w != 0 {
            p << 4
        } else if *cont {
            *cont = false;
            0
        } else {
            words[0].pause << 4
        }
    } else if w == 0 {
        if *cont {
            *cont = false;
            0
        } else {
            11000
        }
    } else {
        match words[w - 1].brk {
            b'Z' | b'^' => 11000,
            b'[' => 0xc80,
            _ => 0,
        }
    };
    v * 16000 / 16000
}

/// `FUN_10020640`: the unit-record arrays of words `first .. first + n`. Every pause becomes ten silence
/// records of a tenth of its length; speech records take mode, unit entry and centre from the target and
/// the per-word rates from `words`. `None` when the phrase needs more than 0x3fc records (the engine
/// bails out and leaves the previous phrase's count in place).
pub fn build_records04(words: &[Word04], first: usize, n: usize, ch: &Channel04, cont: &mut bool) -> Option<Vec<Record>> {
    let mut recs: Vec<Record> = Vec::new();
    for wi in first..first + n {
        let pause = pause_samples04(words, wi, cont);
        if pause > 0 {
            for _ in 0..10 {
                recs.push(Record {
                    pitch: ch.pitch,
                    dur: ch.dur,
                    vol: ch.vol,
                    len: pause / 10,
                    unit: -1,
                    typ: TYPE_SILENCE,
                    word: first as u16,
                    w2a: -1,
                    phone: 0xffff,
                    center: 0x5a,
                    ..Default::default()
                });
            }
        }
        let w = &words[wi];
        for (j, t) in w.targets.iter().enumerate() {
            let mut r = Record {
                pitch: w.pitch,
                dur: w.dur,
                vol: w.vol,
                unit: j as i16,
                typ: TYPE_UNIT,
                mode: t.half,
                word: wi as u16,
                phone: t.phone as u16,
                center: t.center,
                ..Default::default()
            };
            r.fill(&t.db, t.file);
            recs.push(r);
            if recs.len() > 0x3fc {
                return None;
            }
        }
    }
    Some(recs)
}

/// Byte limit of one output chunk.
pub const CHUNK_BYTES04: i32 = 60_000;

/// What the chunker reads of one record: its raw `len` / `dur` and the samples it added to the output.
#[derive(Debug, Clone, Copy)]
pub struct ChunkRec {
    pub len: i32,
    pub dur: i32,
    pub produced: usize,
    pub silence: bool,
}

/// Chunk sizes in bytes (`wav_GeneratePhrase` 0x100203c0): before each record the estimate
/// `(dur * len * 2 + 50) / 100 + 2000 + chunk_bytes` and `len * 2 + decoded_since_entry * 2` are tested
/// against 60000; when either exceeds it the chunk ends and the phrase is re-entered at that record.
pub fn chunk_sizes04(phrases: &[Vec<ChunkRec>]) -> Vec<usize> {
    let mut chunks = Vec::new();
    let mut bytes: i32 = 0;
    for ph in phrases {
        let mut decoded: i32 = 0;
        for r in ph {
            let est = (r.dur * r.len * 2 + 0x32) / 100;
            if CHUNK_BYTES04 < est + 2000 + bytes || CHUNK_BYTES04 < r.len * 2 + decoded * 2 {
                if bytes > 0 {
                    chunks.push(bytes as usize);
                }
                bytes = 0;
                decoded = 0;
            }
            if !r.silence {
                decoded += r.len;
            }
            bytes += (r.produced * 2) as i32;
        }
    }
    if bytes > 0 {
        chunks.push(bytes as usize);
    }
    chunks
}

#[allow(dead_code)]
fn _unused() {
    let _ = wmul;
}
