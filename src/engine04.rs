//! Back half of the 2004 Paul pipeline: from selected units to PCM. Mirrors what `wav_FillChunk`
//! (0x1001da20) drives per phrase after `us_SelectUnitsForPhrase`: the unit-record builder
//! (`FUN_10020640`), `wav_GeneratePhrase` (0x100203c0) and the chunker. The linguistic front end and the
//! unit selector (other stages) hand over a `Sentence04` per sentence: words with their selected
//! `Target04`s and the per-word rates, plus the phrase ranges.

use std::io;

use crate::bank::Bank;
use crate::era::Era;
use crate::ling04::sentence::{ph, sy, wd, Ss04, SYL_SIZE};
use crate::prosody;
use crate::unitsel04::{Selector04, Word04 as SelWord04};
use crate::wave::{DecodeWindow, Record, TYPE_UNIT};
use crate::wave04::{
    build_records04, chunk_sizes04, generate_phrase04, primary_from_entry, Channel04, ChunkRec, PhraseInput04, Target04,
    Word04,
};

/// One sentence ready for waveform generation.
#[derive(Debug, Clone, Default)]
pub struct Sentence04 {
    pub words: Vec<Word04>,
    /// `(first word, word count)` per phrase (`SS+0x0c+0x10g`, `+0x0e+0x10g`).
    pub phrases: Vec<(usize, usize)>,
}

/// The PCM of a request and its chunk sizes in bytes (what `VT_TextToBuffer_ENG` hands back per call).
#[derive(Debug, Default)]
pub struct Synth04 {
    pub pcm: Vec<i16>,
    pub chunks: Vec<usize>,
}

/// Unit global ids of a phrase's records, in record order (0 for silence).
fn record_uids(words: &[Word04], recs: &[Record]) -> Vec<u32> {
    recs.iter()
        .map(|r| if r.typ == TYPE_UNIT { words[r.word as usize].targets[r.unit as usize].uid } else { 0 })
        .collect()
}

fn ss_off(v: u32) -> usize {
    (v & 0x0fff_ffff) as usize
}

/// Prosody's view of a finished 2004 SS (`fe_BuildPhoneContexts` done); durations, tone and F0 zeroed.
pub fn ss_to_prosody04(ss: &Ss04) -> prosody::Sentence {
    let mut s = prosody::Sentence { era: Era::Paul04, ..Default::default() };
    for i in 0..ss.n_words() {
        let w = Ss04::word(i);
        let n = ss.n_phones(i);
        let codes: Vec<u8> = ss.b[w + wd::CODES..w + wd::CODES + n].to_vec();
        let pos_code: Vec<i8> = (0..n).map(|k| ss.u8(w + wd::SYLPOS + k) as i8).collect();
        let base = ss_off(ss.u32(w + wd::SYL_PTR));
        let syllables = (0..ss.i16(w + wd::N_SYL).max(0) as usize)
            .map(|k| {
                let o = base + k * SYL_SIZE;
                prosody::Syllable {
                    b0: ss.u8(o + sy::STRESS) as i8,
                    b1: ss.u8(o + sy::SHAPE) as i8,
                    b2: ss.u8(o + sy::POSITION) as i8,
                    tone: 0,
                    f0: [0; 12],
                    first_phone: (ss_off(ss.u32(o + sy::CODES_PTR)) as isize - (w + wd::CODES) as isize) as u8,
                    n_phones: ss.i16(o + sy::N_PHONES) as u8,
                }
            })
            .collect();
        s.words.push(prosody::Word { codes, pos_code, syllables, brk: ss.u8(w + wd::BRK), dur: vec![0; n] });
    }
    s.phrase_words = (0..ss.n_phrases()).map(|g| ss.i16(Ss04::phrase(g) + ph::N_WORDS).max(0) as usize).collect();
    s
}

/// Durations (word +0xee), tone (syllable +3) and F0 (syllable +4) back into the SS.
pub fn write_prosody04(ss: &mut Ss04, s: &prosody::Sentence) {
    for (i, w) in s.words.iter().enumerate() {
        let wb = Ss04::word(i);
        for (k, &d) in w.dur.iter().enumerate() {
            ss.set_i16(wb + wd::DUR + 2 * k, d);
        }
        let base = ss_off(ss.u32(wb + wd::SYL_PTR));
        for (k, y) in w.syllables.iter().enumerate() {
            let o = base + k * SYL_SIZE;
            ss.set_u8(o + 3, y.tone as u8);
            for (j, &f) in y.f0.iter().enumerate() {
                ss.set_i16(o + sy::F0 + 2 * j, f);
            }
        }
    }
}

/// Per-word rates of a sentence: the TIB arrays `pause`, `speed`, `volume`, `pitch` (word `i` at index `i`).
#[derive(Debug, Clone, Default)]
pub struct Rates04 {
    pub pause: Vec<i32>,
    pub speed: Vec<i32>,
    pub volume: Vec<i32>,
    pub pitch: Vec<i32>,
}

/// One sentence of a request as `fe_NextSentence` (0x10019e70) leaves it: the normalization records and
/// the word arrays (`brk` = TIB break levels).
pub struct Front04 {
    pub wa: crate::ling04::Wa04,
    pub rates: Rates04,
    pub brk: Vec<i32>,
}

/// `fe_NextSentence` over a request's raw text: the initial pass, the sentences, and per sentence the
/// rate seeds (`pro_SeedWordRates` 0x1001a410; the leading-tag flags it reads are never set, so the
/// channel's values) and the tag spans (`pro_ApplyTagSpansToWords` 0x1001a130) over the words' positions
/// in the processed text (`end - 1`, or the start for an empty record).
pub fn front04(fe: &crate::text04::Fe, src: &[u8], ch: &Channel04) -> Vec<Front04> {
    let ini = crate::text04::initial::initial(src);
    let lists: [prosody::SpanList; 5] =
        std::array::from_fn(|k| prosody::SpanList { pos: ini.stacks[k].pos.clone(), val: ini.stacks[k].val.clone(), cursor: 0 });
    let seed = prosody::seed_rates04([false; 3], [-1; 3], [ch.pitch, ch.dur, ch.vol]);
    crate::text04::sentences(fe, &ini.processed)
        .into_iter()
        .map(|s| {
            let n = s.wa.n().max(0);
            let words: Vec<(i32, i32)> = (0..n)
                .map(|i| {
                    let (st, e) = (s.wa.ri32(i, 0), s.wa.ri32(i, 4));
                    (s.cursor + st, if st < e { s.cursor + e - 1 } else { s.cursor + st })
                })
                .collect();
            let n = n as usize;
            let mut r = prosody::WordRates {
                pitch: vec![seed[0]; n],
                speed: vec![seed[1]; n],
                volume: vec![seed[2]; n],
                pause: vec![-1; n],
                brk: vec![-1; n],
                k7: Vec::new(),
            };
            if n > 0 {
                prosody::apply_tag_spans04(&words, s.cursor, &lists, &mut r);
            }
            Front04 { wa: s.wa.to_wa04(), rates: Rates04 { pause: r.pause, speed: r.speed, volume: r.volume, pitch: r.pitch }, brk: r.brk }
        })
        .collect()
}

/// Prosody, unit selection and the record-builder view of one sentence. A negative first pause takes the
/// channel's (`fe_ProcessNextSentence`), capped at 20000.
pub fn sentence04(ss: &mut Ss04, trees: &prosody::Trees, sel: &mut Selector04, rates: &Rates04, ch: &Channel04) -> Sentence04 {
    let mut ps = ss_to_prosody04(ss);
    prosody::predict_durations(trees, &mut ps);
    prosody::predict_pitch(trees, &mut ps);
    write_prosody04(ss, &ps);
    let nw = ss.n_words();
    let mut sw: Vec<SelWord04> = (0..nw)
        .map(|i| {
            let w = Ss04::word(i);
            let base = ss_off(ss.u32(w + wd::SYL_PTR));
            let syls: Vec<Vec<u8>> =
                (0..ss.i16(w + wd::N_SYL).max(0) as usize).map(|k| ss.b[base + k * SYL_SIZE..base + (k + 1) * SYL_SIZE].to_vec()).collect();
            SelWord04::from_raw(&ss.b[w - 4..w - 4 + 0x300], &syls)
        })
        .collect();
    let at = |v: &[i32], i: usize| v.get(i).copied().unwrap_or(-1);
    let mut words: Vec<Word04> = (0..nw)
        .map(|i| Word04 {
            brk: ss.u8(Ss04::word(i) + wd::BRK),
            targets: Vec::new(),
            pause: at(&rates.pause, i),
            dur: at(&rates.speed, i),
            vol: at(&rates.volume, i),
            pitch: at(&rates.pitch, i),
        })
        .collect();
    if let Some(w0) = words.first_mut() {
        if w0.pause < 0 {
            w0.pause = ch.pause;
        }
        w0.pause = w0.pause.min(20000);
    }
    let mut phrases = Vec::new();
    for g in 0..ss.n_phrases() {
        let e = Ss04::phrase(g);
        let (first, n) = (ss.i16(e + ph::FIRST).max(0) as usize, ss.i16(e + ph::N_WORDS).max(0) as usize);
        phrases.push((first, n));
        let r = sel.select_phrase(&mut sw, first, n);
        for (k, t) in r.targets.iter().enumerate() {
            let (db, file) = primary_from_entry(&r.db[k]);
            let wi = t.word as usize;
            let center = ss.u8(Ss04::word(wi) + wd::CTX + 5 * t.phone as usize + 1);
            words[wi].targets.push(Target04 { phone: t.phone, half: t.half, db, file, center, uid: r.uids[k] as u32 });
        }
    }
    Sentence04 { words, phrases }
}

/// Every phrase of every sentence in order. `cont` is `TIB+0x1132b8`, seeded true once per request and
/// cleared by the first word 0 that consumes it (`pause_samples04`). A phrase the builder rejects (more
/// than 0x3fc records) produces nothing here; the engine re-plays the previous phrase's stale records.
pub fn synth_request04(bank: &Bank, sentences: &[Sentence04], ch: &Channel04) -> io::Result<Synth04> {
    let mut win = DecodeWindow::default();
    let mut out = Vec::new();
    let mut cont = true;
    let mut chunk_in: Vec<Vec<ChunkRec>> = Vec::new();
    for s in sentences {
        for &(first, n) in &s.phrases {
            let Some(recs) = build_records04(&s.words, first, n, ch, &mut cont) else { continue };
            let uids = record_uids(&s.words, &recs);
            let mut trace = Vec::new();
            let start = out.len();
            generate_phrase04(&mut win, bank, &PhraseInput04 { records: &recs, uids: &uids }, &mut out, Some(&mut trace))?;
            let end = out.len();
            let _ = start;
            chunk_in.push(
                recs.iter()
                    .enumerate()
                    .map(|(k, r)| ChunkRec {
                        len: r.len,
                        dur: r.dur,
                        produced: trace.get(k + 1).map(|t| t.start).unwrap_or(end) - trace[k].start,
                        silence: r.typ != TYPE_UNIT,
                    })
                    .collect(),
            );
        }
    }
    Ok(Synth04 { pcm: out, chunks: chunk_sizes04(&chunk_in) })
}
