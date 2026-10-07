//! Sentence builder of the 2009 English builds (julie09, paul09): the linguistic front end on one
//! normalized `Wa`, and the views of the finished `Ss` that prosody (`prosody::Sentence`, durations at
//! word +0x0a, syllables at +0x04) and unit selection (`unitsel::Word::from_raw_era`, 5-byte phone
//! context at +0x90 + 5k) read. `Engine::open_voice` does not call it yet; the lead wires it.

use crate::engine::SentenceInput;
use crate::era::Era;
use crate::lexicon::Lexicon;
use crate::ling::lay::Lay;
use crate::ling::{self, Ss, Tib, Wa, WordStrings, Ws};
use crate::prosody::{self, Trees};
use crate::unitsel;
use crate::wave::WordInfo;

/// Word-record offset of the predicted phone duration (16 kHz samples), `+0x0a + 2k`.
pub const WR_DUR09: usize = 0x0a;

/// `fe_LinguisticAnalysis` of julie09 / paul09 on one normalized sentence: lookup, copy, morphology,
/// function words, merge, then word records, phrase breaks (with syllabification) and phone contexts.
/// `lex` must have been opened with `Lexicon::open_era(dict, era)`. `cursor` is the sum of the earlier
/// sentences' `Wa::consumed()` of the request. `None` when the merge fails or the sentence cannot be
/// broken under the 1000-half-phone cap.
pub fn run_ling09(era: Era, lex: &Lexicon, input: &SentenceInput, cursor: i32) -> Option<(Wa, Ss, Tib)> {
    let lay = Lay::of(era);
    let mut wa = input.wa.clone();
    let n = wa.n();
    let mut ws = Ws::new();
    ling::analysis::tag_and_pronounce(&mut ws, &mut wa, n, lex, &lex.poly);
    if !ling::merge::merge(&ws, &mut wa) {
        return None;
    }
    let mut ss = Ss::new();
    ss.set_i16(2, n as i16);
    fn map(m: &[i32]) -> Option<&[i32]> {
        if m.is_empty() {
            None
        } else {
            Some(m)
        }
    }
    let pos = ling::words::TextPos { cursor, map_start: map(&input.map_start), map_end: map(&input.map_end) };
    ling::words::build_word_phone_records_lay(lay, &mut ss, &wa, &input.kind7, &pos);
    let mut tib = input.tib.clone();
    if !ling::breaks::assign_phrase_breaks_era(era, &mut ss, &mut tib, &wa, &lex.engbi) {
        return None;
    }
    let strs = WordStrings { wa: &wa };
    ling::context::build_phone_contexts09(era, &mut ss, &strs);
    Some((wa, ss, tib))
}

/// Sentence type of the 2009 engines: 4 declarative, 5 question, 6 other.
pub fn sentence_type09(era: Era, ss: &Ss) -> u8 {
    ss.u8(Lay::of(era).ss_type)
}

/// Prosody's view of a finished `Ss`; durations, tone and F0 start zeroed.
pub fn ss_to_prosody09(era: Era, ss: &Ss) -> prosody::Sentence {
    let lay = Lay::of(era);
    let mut s = prosody::Sentence { era, ..Default::default() };
    for i in 0..ss.n_words() {
        let w = lay.word(i);
        let n = ss.u8(w + lay.n_phones) as usize;
        let codes: Vec<u8> = (0..n).map(|k| ss.u8(w + lay.codes + k)).collect();
        let pos_code: Vec<i8> = (0..n).map(|k| ss.u8(w + lay.sylpos + k) as i8).collect();
        let nsyl = ss.u8(w + lay.n_syl) as usize;
        let syl_base = ss.deref(ss.u32(w + lay.syl_ptr));
        let syllables = (0..nsyl)
            .map(|k| {
                let o = syl_base.unwrap_or(0) + k * ling::SYL_SIZE;
                prosody::Syllable {
                    b0: ss.u8(o + ling::syl::STRESS) as i8,
                    b1: ss.u8(o + ling::syl::SHAPE) as i8,
                    b2: ss.u8(o + ling::syl::POSITION) as i8,
                    tone: 0,
                    f0: [0; 12],
                    first_phone: ss.u8(o + ling::syl::FIRST_PHONE),
                    n_phones: ss.u8(o + ling::syl::N_PHONES),
                }
            })
            .collect();
        s.words.push(prosody::Word { codes, pos_code, syllables, brk: ss.u8(w + lay.brk), dur: vec![0; n] });
    }
    s.phrase_words = (0..ss.n_phrases()).map(|g| ss.i16(Ss::phrase(g) + 2).max(0) as usize).collect();
    s
}

/// Stores `predict_durations` / `predict_pitch` results into the `Ss` word and syllable records.
pub fn write_prosody09(era: Era, ss: &mut Ss, s: &prosody::Sentence) {
    let lay = Lay::of(era);
    for (i, w) in s.words.iter().enumerate() {
        let wb = lay.word(i);
        for (k, &d) in w.dur.iter().enumerate() {
            ss.set_i16(wb + WR_DUR09 + 2 * k, d);
        }
        let Some(syl_base) = ss.deref(ss.u32(wb + lay.syl_ptr)) else { continue };
        for (k, sy) in w.syllables.iter().enumerate() {
            let o = syl_base + k * ling::SYL_SIZE;
            ss.set_u8(o + ling::syl::TONE, sy.tone as u8);
            for (j, &f) in sy.f0.iter().enumerate() {
                ss.set_i16(o + ling::syl::F0 + 2 * j, f);
            }
        }
    }
}

/// Durations, tone and smoothed F0 of one sentence, left in `ss`.
pub fn run_prosody09(era: Era, trees: &Trees, ss: &mut Ss) {
    let mut s = ss_to_prosody09(era, ss);
    prosody::predict_durations(trees, &mut s);
    prosody::predict_pitch(trees, &mut s);
    write_prosody09(era, ss, &s);
}

/// Unit selection's view of every word of a prosody-filled `Ss` (`Word::from_raw_era`: 0x300-byte records).
pub fn ss_to_unitsel_words09(era: Era, ss: &Ss) -> Vec<unitsel::Word> {
    let lay = Lay::of(era);
    (0..ss.n_words())
        .map(|i| {
            let w = lay.word(i);
            let raw = ss.b[w..w + lay.stride].to_vec();
            let nsyl = ss.u8(w + lay.n_syl) as usize;
            let syl_base = ss.deref(ss.u32(w + lay.syl_ptr));
            let syls: Vec<Vec<u8>> = (0..nsyl)
                .map(|k| {
                    let o = syl_base.unwrap_or(0) + k * ling::SYL_SIZE;
                    ss.b[o..o + ling::SYL_SIZE].to_vec()
                })
                .collect();
            unitsel::Word::from_raw_era(&raw, &syls, era)
        })
        .collect()
}

/// Per-word waveform inputs: the break code and the caller's per-word rates.
pub fn ss_word_info09(era: Era, ss: &Ss, input: &SentenceInput) -> Vec<WordInfo> {
    let lay = Lay::of(era);
    (0..ss.n_words())
        .map(|i| WordInfo {
            brk: ss.u8(lay.word(i) + lay.brk),
            pitch: input.pitch.get(i).copied().unwrap_or(100),
            speed: input.speed.get(i).copied().unwrap_or(100),
            volume: input.volume.get(i).copied().unwrap_or(100),
        })
        .collect()
}

/// Centre phone of the 5-byte context `{L, C, R, f1, f2}` of phone `k` of word `w` (index 1), what the
/// record builder passes as the phone label.
pub fn center09(era: Era, ss: &Ss, w: usize, k: usize) -> u8 {
    let lay = Lay::of(era);
    ss.u8(lay.word(w) + lay.ctx + lay.ctx_stride * k + 1)
}
