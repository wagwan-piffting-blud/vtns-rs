//! Prosody: per-phone durations, per-syllable tone and 12-point F0 contours,
//! cross-syllable F0 smoothing, per-word pitch/speed/volume/pause/break arrays
//! from VTML spans, pause-gap lengths, and the 3-byte unit-selection target
//! feature. Mirrors vt_eng.dll `pro_*`.

use std::io;
use std::path::Path;

#[path = "prosody_tables.rs"]
pub mod tables;

use crate::tree3::Tree;
use tables::*;

/// Break codes (word record +0x3bd).
pub const BRK_NONE: u8 = b']';
pub const BRK_MINOR: u8 = b'\\';
pub const BRK_PHRASE: u8 = b'[';
pub const BRK_SENTENCE: u8 = b'Z';
pub const BRK_QUESTION: u8 = b'^';
pub const BRK_GRAVE: u8 = b'`';

/// Sentinel phone indices used as context features at word edges.
const EDGE_PAUSE: i16 = 40;
const EDGE_JUNCTURE: i16 = 42;

/// Unvoiced / vowel-less fallback durations when a phone has no class.
const DUR_VOWEL_DEFAULT: i16 = 1600;
const DUR_CONS_DEFAULT: i16 = 500;

/// Syllable record (0x1e bytes, array at word +0x08).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Syllable {
    /// +0, +1, +2: front-end syllable attributes, read as signed chars.
    pub b0: i8,
    pub b1: i8,
    pub b2: i8,
    /// +3: tone label from the `*t` tree.
    pub tone: i8,
    /// +4..+0x1b: F0 contour in Hz.
    pub f0: [i16; 12],
    /// +0x1c: index of the syllable's first phone within the word.
    pub first_phone: u8,
    /// +0x1d: phone count.
    pub n_phones: u8,
}

/// The parts of a word record (0x3c0 bytes) that prosody reads and writes.
#[derive(Debug, Clone, Default)]
pub struct Word {
    /// +0x2e8+k: phone codes (1..0x45 plus allophone codes up to 0x53).
    pub codes: Vec<u8>,
    /// +0x36a+k: per-phone syllable-position code (1 onset, 2 nucleus, 3 coda; inferred).
    pub pos_code: Vec<i8>,
    /// +0x08 -> syllables; count at +0x94.
    pub syllables: Vec<Syllable>,
    /// +0x3bd: break after this word.
    pub brk: u8,
    /// +0x10+2k: predicted duration per phone (16 kHz samples).
    pub dur: Vec<i16>,
}

/// One sentence: words plus phrase grouping (words per phrase, SS+0x0e+16g).
#[derive(Debug, Clone, Default)]
pub struct Sentence {
    pub words: Vec<Word>,
    pub phrase_words: Vec<usize>,
}

/// The 17 per-voice trees (speaker struct +0x494 + {0xb4..0x274}).
pub struct Trees {
    pub vshort: Tree,
    pub vlong: Tree,
    pub vdi: Tree,
    pub vsch: Tree,
    pub cstop: Tree,
    pub cfri: Tree,
    pub caff: Tree,
    pub cnas: Tree,
    pub capp: Tree,
    pub nbt: Tree,
    pub nbf: Tree,
    pub bt: Tree,
    pub bf: Tree,
    pub qbt: Tree,
    pub qbf: Tree,
    pub sbt: Tree,
    pub sbf: Tree,
}

impl Trees {
    /// `tree3_dir` is `.../M16/ttsdata/tree3`.
    pub fn load(tree3_dir: &Path) -> io::Result<Trees> {
        let d = |n: &str| Tree::load(&tree3_dir.join("duration").join(format!("{}.tree3", n)));
        let p = |n: &str| Tree::load(&tree3_dir.join("pitch").join(format!("{}.tree3", n)));
        Ok(Trees {
            vshort: d("vshort")?,
            vlong: d("vlong")?,
            vdi: d("vdi")?,
            vsch: d("vsch")?,
            cstop: d("cstop")?,
            cfri: d("cfri")?,
            caff: d("caff")?,
            cnas: d("cnas")?,
            capp: d("capp")?,
            nbt: p("nbt")?,
            nbf: p("nbf")?,
            bt: p("bt")?,
            bf: p("bf")?,
            qbt: p("qbt")?,
            qbf: p("qbf")?,
            sbt: p("sbt")?,
            sbf: p("sbf")?,
        })
    }
}

/// Base phone index of a phone code (`(char)DAT_1007b6c0[(char)code]`).
pub fn phone_index(code: u8) -> i8 {
    PHONE_INDEX[(code & 0x7f) as usize]
}

pub fn phone_props(code: u8) -> [u8; 8] {
    let i = phone_index(code);
    if (0..40).contains(&i) {
        PHONE_PROPS[i as usize].1
    } else {
        [0; 8]
    }
}

fn is_final_break(b: u8) -> bool {
    matches!(b, BRK_PHRASE | BRK_SENTENCE | BRK_QUESTION | BRK_GRAVE)
}

/// Previous word ended in a phrase break, or this is the sentence's first word.
fn starts_after_pause(s: &Sentence, wi: usize) -> bool {
    wi == 0 || s.words[wi - 1].brk == BRK_PHRASE
}

/// pro_DurationFeatures (0x100135d0): 9-short feature vector for phone `p` of syllable `si` of word `wi`.
pub fn duration_features(s: &Sentence, wi: usize, si: usize, p: usize) -> [i16; 9] {
    let w = &s.words[wi];
    let syl = &w.syllables[si];
    let k = syl.first_phone as usize + p;
    let nsyl = w.syllables.len();
    let last_in_word = si + 1 == nsyl && p + 1 == syl.n_phones as usize;
    let mut fv = [0i16; 9];
    fv[0] = phone_index(w.codes[k]) as i16;
    fv[1] = if si == 0 && p == 0 {
        if starts_after_pause(s, wi) { EDGE_PAUSE } else { EDGE_JUNCTURE }
    } else {
        phone_index(w.codes[k - 1]) as u8 as i16
    };
    fv[2] = if last_in_word {
        if is_final_break(w.brk) { EDGE_PAUSE } else { EDGE_JUNCTURE }
    } else {
        phone_index(w.codes[k + 1]) as u8 as i16
    };
    fv[3] = PHONE_STRESS[(w.codes[k] & 0x7f) as usize] as i16;
    fv[4] = syl.b1 as i16;
    fv[5] = syl.b2 as i16;
    fv[6] = if si == 0 && starts_after_pause(s, wi) {
        1
    } else if si + 1 == nsyl && is_final_break(w.brk) {
        3
    } else {
        2
    };
    fv[7] = nsyl as i16;
    fv[8] = w.pos_code[k] as i16;
    fv
}

/// pro_PredictDurations (0x10013380): fills every word's `dur`.
pub fn predict_durations(trees: &Trees, s: &mut Sentence) {
    for wi in 0..s.words.len() {
        let mut out = Vec::with_capacity(s.words[wi].codes.len());
        for si in 0..s.words[wi].syllables.len() {
            for p in 0..s.words[wi].syllables[si].n_phones as usize {
                let fv = duration_features(s, wi, si, p);
                let k = s.words[wi].syllables[si].first_phone as usize + p;
                let props = phone_props(s.words[wi].codes[k]);
                let tree = if props[0] == 1 {
                    match props[1] {
                        1 => Some(&trees.vshort),
                        2 => Some(&trees.vlong),
                        3 => Some(&trees.vdi),
                        4 => Some(&trees.vsch),
                        _ => None,
                    }
                } else {
                    match props[5] {
                        1 => Some(&trees.cstop),
                        2 => Some(&trees.cfri),
                        3 => Some(&trees.caff),
                        4 | 5 => Some(&trees.cnas),
                        6 => Some(&trees.capp),
                        _ => None,
                    }
                };
                out.push(match tree {
                    Some(t) => t.predict_scalar(&fv),
                    None if props[0] == 1 => DUR_VOWEL_DEFAULT,
                    None => DUR_CONS_DEFAULT,
                });
            }
        }
        let w = &mut s.words[wi];
        // The engine writes a running index; phones outside every syllable keep their old value.
        if w.dur.len() < w.codes.len() {
            w.dur.resize(w.codes.len(), 0);
        }
        for (k, d) in out.into_iter().enumerate() {
            w.dur[k] = d;
        }
    }
}

/// pro_PitchFeatures (0x10013a20): 11 shorts; the caller appends the tone as fv[11].
pub fn pitch_features(s: &Sentence, wi: usize, si: usize) -> [i16; 12] {
    let w = &s.words[wi];
    let syl = &w.syllables[si];
    let nsyl = w.syllables.len();
    let first = syl.first_phone as usize;
    let mut fv = [0i16; 12];
    fv[0] = phone_props(w.codes[first])[5] as i8 as i16;
    let n = syl.n_phones as usize;
    let mut nuc = (0..n).find(|&j| w.pos_code[first + j] == 2).unwrap_or(n);
    if nuc == n {
        nuc = n.wrapping_sub(1);
    }
    fv[1] = phone_index(w.codes[first + nuc]) as u8 as i16;
    fv[2] = syl.b0 as i16;
    fv[5] = syl.b2 as i16;
    if si == 0 {
        fv[3] = 3;
        fv[6] = 0;
    } else {
        fv[3] = w.syllables[si - 1].b0 as i16;
        fv[6] = w.syllables[si - 1].b2 as i16;
    }
    if si + 1 == nsyl {
        fv[4] = 3;
        fv[7] = 4;
    } else {
        fv[4] = w.syllables[si + 1].b0 as i16;
        fv[7] = w.syllables[si + 1].b2 as i16;
    }
    fv[8] = if si == 0 && starts_after_pause(s, wi) {
        1
    } else if is_final_break(w.brk) && si + 1 == nsyl {
        3
    } else {
        2
    };
    fv[9] = syl.b1 as i16;
    fv[10] = nsyl as i16;
    fv
}

/// pro_PredictPitch (0x100138c0) without the final smoothing pass.
pub fn predict_pitch_raw(trees: &Trees, s: &mut Sentence) {
    for wi in 0..s.words.len() {
        for si in 0..s.words[wi].syllables.len() {
            let mut fv = pitch_features(s, wi, si);
            let last = si + 1 == s.words[wi].syllables.len();
            let (t, f) = match s.words[wi].brk {
                BRK_QUESTION if last => (&trees.qbt, &trees.qbf),
                BRK_SENTENCE if last => (&trees.sbt, &trees.sbf),
                BRK_PHRASE if last => (&trees.bt, &trees.bf),
                _ => (&trees.nbt, &trees.nbf),
            };
            let tone = t.predict_scalar(&fv) as i8;
            fv[11] = tone as i16;
            let contour = f.predict(&fv);
            let syl = &mut s.words[wi].syllables[si];
            syl.tone = tone;
            for (d, &v) in syl.f0.iter_mut().zip(contour.iter()) {
                *d = v;
            }
        }
    }
}

/// pro_RoundedMean (0x10013790).
fn rounded_mean(v: &[i16]) -> i16 {
    let n = v.len() as i32;
    let sum: i32 = v.iter().map(|&x| x as i32).sum();
    ((n + sum * 2) / (n * 2)) as i16
}

/// pro_SmoothF0AcrossSyllables (0x100137c0) for every phrase: the last three
/// points of each syllable and the first three of the next become the rounded
/// mean of the 7-point window centred on them, taken over a copy of the pair.
pub fn smooth_f0(s: &mut Sentence) {
    let mut w0 = 0;
    for &nw in &s.phrase_words {
        let mut refs: Vec<(usize, usize)> = Vec::new();
        for wi in w0..(w0 + nw).min(s.words.len()) {
            for si in 0..s.words[wi].syllables.len() {
                refs.push((wi, si));
            }
        }
        w0 += nw;
        if refs.len() < 2 {
            continue;
        }
        for j in 0..refs.len() - 1 {
            let (aw, as_) = refs[j];
            let (bw, bs) = refs[j + 1];
            let mut buf = [0i16; 24];
            buf[..12].copy_from_slice(&s.words[aw].syllables[as_].f0);
            buf[12..].copy_from_slice(&s.words[bw].syllables[bs].f0);
            for k in -3i32..3 {
                let start = (k + 9) as usize;
                let m = rounded_mean(&buf[start..start + 7]);
                if k < 0 {
                    s.words[aw].syllables[as_].f0[(12 + k) as usize] = m;
                } else {
                    s.words[bw].syllables[bs].f0[k as usize] = m;
                }
            }
        }
    }
}

/// Full pro_PredictPitch: tone + contour per syllable, then smoothing.
pub fn predict_pitch(trees: &Trees, s: &mut Sentence) {
    predict_pitch_raw(trees, s);
    smooth_f0(s);
}

/// pro_QuantizeDurationDiv50 (0x10022fd0). The argument is taken as a short.
pub fn quantize_duration(dur: i32) -> u8 {
    let d = dur as i16 as f32;
    // x87 at 53-bit precision: the quotient and sum round like f64.
    let v = ((d as f64) / (DUR_DIV as f64) + DUR_ROUND as f64).trunc() as i64 as i16;
    if v < 0 {
        0
    } else if v >= 0x100 {
        0xff
    } else {
        v as u8
    }
}

/// One entry of a word's target list (word +0x0c, count at +0x92), same 6-byte
/// layout as the phrase target list at TIB+0xec628.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetRef {
    pub word: u8,
    pub syl: u8,
    /// Phone index within the word.
    pub phone: u8,
    /// Index of this entry within the word's target list.
    pub index: u8,
    /// 0 whole phone, 1 left half, 2 right half.
    pub half: u8,
    pub unvoiced: u8,
}

/// The 3-byte target feature built at the top of us_GetCandidates (0x10023350):
/// quantized duration, and the syllable's smoothed F0 sampled at the voiced
/// start and end of this target (low byte of the Hz value).
pub fn target_feature(word: &Word, word_targets: &[TargetRef], t: &TargetRef) -> [u8; 3] {
    let half_dur = |phone: u8, half: u8| {
        let d = word.dur[phone as usize] as i32;
        if half != 0 { d / 2 } else { d }
    };
    let dur_code = quantize_duration(half_dur(t.phone, t.half));
    let first = word_targets.iter().position(|e| e.syl == t.syl);
    let (mut a, mut b) = (0i32, 0i32);
    if let Some(first) = first {
        let (mut before, mut through, mut total) = (0i32, 0i32, 0i32);
        for (i, e) in word_targets.iter().enumerate().skip(first) {
            if e.syl != t.syl {
                break;
            }
            if e.unvoiced == 0 {
                let d = half_dur(e.phone, e.half);
                if (i as i32) < t.index as i32 {
                    before += d;
                } else if i as i32 == t.index as i32 {
                    through = before + d;
                }
                total += d;
            }
        }
        if total != 0 {
            a = (before * 12 / total).min(11);
            b = (through * 12 / total - 1).max(a);
        }
    }
    let f0 = &word.syllables[t.syl as usize].f0;
    [dur_code, f0[a as usize] as u8, f0[b as usize] as u8]
}

/// Span list for one VTML kind (TIB+0x1223c0 + 16k): character positions in the
/// tag-stripped text and the value that applies from each position on.
#[derive(Debug, Clone, Default)]
pub struct SpanList {
    pub pos: Vec<i32>,
    pub val: Vec<i32>,
    /// Resume cursor (TIB+0x1223c4 + 16k), carried across sentences.
    pub cursor: i32,
}

/// Per-word arrays seeded by fe_NextSentence and overlaid by
/// pro_ApplyTagSpansToWords (TIB+0x121100 + 0x320*k).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WordRates {
    pub pitch: Vec<i32>,
    pub speed: Vec<i32>,
    pub volume: Vec<i32>,
    /// Pause before word i in ms (index 0 = sentence pause), -1 = unset.
    pub pause: Vec<i32>,
    /// Forced break level after word i, -1 = none.
    pub brk: Vec<i32>,
    /// Kind-7 word class, -1 = none.
    pub k7: Vec<i32>,
}

impl WordRates {
    /// The seeding done in fe_NextSentence (0x10022dc0).
    pub fn seed(n: usize, pitch: i32, speed: i32, volume: i32) -> WordRates {
        WordRates {
            pitch: vec![pitch; n],
            speed: vec![speed; n],
            volume: vec![volume; n],
            pause: vec![-1; n],
            brk: vec![-1; n],
            k7: vec![-1; n],
        }
    }
}

/// Inputs to pro_ApplyTagSpansToWords that live outside the word array.
pub struct SpanContext<'a> {
    /// Word (start, end) in tag-stripped text coordinates, as held at call time.
    pub words: &'a [(i32, i32)],
    /// TIB+0: length of the whole tag-stripped text.
    pub total_len: i32,
    /// TIB+4: offset of this sentence in it.
    pub cursor: i32,
    /// Sentence length (normalization word array +4).
    pub sent_len: i32,
}

/// pro_ApplyTagSpansToWords (0x10022970). Returns the trailing pause (TIB+0x122440,
/// ms) that applies after the last word of the text, or -1.
pub fn apply_tag_spans(ctx: &SpanContext, spans: &mut [SpanList; 8], r: &mut WordRates, trail: &mut i32) {
    let n = ctx.words.len();
    for k in [0usize, 1, 2, 7] {
        let l = &spans[k];
        if l.pos.is_empty() {
            continue;
        }
        let arr = match k {
            0 => &mut r.pitch,
            1 => &mut r.speed,
            2 => &mut r.volume,
            _ => &mut r.k7,
        };
        let (mx, mn) = (SPAN_MAX[k], SPAN_MIN[k]);
        let mut wi = 0usize;
        let mut filled = 0usize;
        for j in 0..l.pos.len() {
            while wi < n && ctx.words[wi].0 < l.pos[j] {
                wi += 1;
            }
            if j == 0 {
                filled = filled.max(wi);
            } else {
                let v = l.val[j - 1];
                while filled < wi {
                    let mut x = if v < 0 { arr[filled] } else { v };
                    if k != 7 || x != -1 {
                        x = x.max(mn).min(mx);
                    }
                    arr[filled] = x;
                    filled += 1;
                }
            }
            if j + 1 == l.pos.len() && filled < n {
                filled = n;
            }
        }
    }
    for k in [3usize, 4] {
        if spans[k].pos.is_empty() {
            continue;
        }
        let count = spans[k].pos.len();
        for i in 0..n {
            let (lo, hi) = if k == 3 {
                if i == 0 { (0, ctx.words[0].0) } else { (ctx.words[i - 1].1, ctx.words[i].0) }
            } else if i + 1 == n {
                (ctx.words[i].1, ctx.cursor + ctx.sent_len)
            } else {
                (ctx.words[i].1, ctx.words[i + 1].0)
            };
            let arr = if k == 3 { &mut r.pause } else { &mut r.brk };
            let mut acc = false;
            let mut c = spans[k].cursor;
            loop {
                let mut j = c;
                while (j as usize) < count {
                    let p = spans[k].pos[j as usize];
                    if lo <= p && p <= hi {
                        break;
                    }
                    j += 1;
                }
                if j as usize == count {
                    break;
                }
                let v = spans[k].val[j as usize];
                if k == 3 {
                    arr[i] = if acc { arr[i] + v } else { v };
                    arr[i] = arr[i].max(SPAN_MIN[3]);
                    if arr[i] > SPAN_MAX[3] {
                        arr[i] = SPAN_MAX[3];
                    }
                } else {
                    arr[i] = v;
                }
                c = j + 1;
                acc = true;
                spans[k].cursor = c;
            }
        }
        if k == 3 {
            *trail = -1;
            let end = ctx.cursor + ctx.sent_len;
            if end == ctx.total_len {
                let lo = ctx.words[n - 1].1;
                let mut acc = false;
                let mut c = spans[3].cursor;
                loop {
                    let mut j = c;
                    while (j as usize) < count {
                        let p = spans[3].pos[j as usize];
                        if lo <= p && p <= end {
                            break;
                        }
                        j += 1;
                    }
                    if j as usize == count {
                        break;
                    }
                    let v = spans[3].val[j as usize];
                    *trail = if acc { *trail + v } else { v };
                    *trail = (*trail).max(SPAN_MIN[3]).min(SPAN_MAX[3]);
                    c = j + 1;
                    spans[3].cursor = c;
                    acc = true;
                }
            }
        }
    }
}

/// pro_ResolveSentencePause (0x10026750): resolves pause[0] in place.
/// `call_pause` = TIB+0x1312c8, `tib_pause` = TIB+0x1312c4, `cont` = TIB+0x122850.
pub fn resolve_sentence_pause(p0: &mut i32, call_pause: i32, tib_pause: i32, cont: bool, ch_pause: i32) {
    if *p0 < 0 {
        *p0 = if call_pause >= 0 {
            if cont { 0 } else { call_pause }
        } else if tib_pause >= 0 {
            tib_pause
        } else if cont {
            0
        } else {
            ch_pause
        };
    }
    if *p0 > 0xfffe {
        *p0 = 0xffff;
    }
}

/// pro_PauseSamples (0x1002e890): silence before word `w` (0 = sentence start),
/// in 16 kHz samples. Clears the continuation flag at w == 0.
pub fn pause_samples(pause: &[i32], w: usize, prev_brk: u8, cont: &mut bool, comma_ms: Option<i32>) -> i32 {
    let v = pause[w];
    if w == 0 {
        if v >= 0 {
            *cont = false;
            return pause[0] << 4;
        }
        if *cont {
            *cont = false;
            return 0;
        }
        return 11000;
    }
    if v >= 0 {
        return v << 4;
    }
    match prev_brk {
        BRK_SENTENCE | BRK_QUESTION => 11000,
        BRK_PHRASE => comma_ms.map_or(3200, |ms| ms << 4),
        _ => 0,
    }
}

/// Length of a silence record for `pause_samples` at duration rate `speed` (%).
pub fn silence_length(pause_samples: i32, speed: i32) -> i32 {
    (((speed >> 1) + 10000) / speed * pause_samples + 50) / 100
}
