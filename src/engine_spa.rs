//! Violeta end to end: `vt_spa_violeta16.dll`'s pipeline from raw text to
//! PCM, chained from the per-stage Spanish ports (`text_spa`, `ling_spa`,
//! `prosody_spa`, `unitsel`/`wave` with `Lang::Spa`) in the order
//! `VT_TextToBuffer_SPA` runs them, as `engine.rs` does for English.

use std::io;
use std::path::Path;

use crate::bank::Bank;
use crate::engine::{apply_tag_spans_core, phrase_wave_targets, Channel, SpanOut};
use crate::era::Era;
use crate::lang::Lang;
use crate::ling::Tib;
use crate::ling_spa::words::{build_word_phone_records, deref_ss, sentence_from_ss, word_from_ss, TextPos, WordStringsSpa};
use crate::ling_spa::spa09::{self, w09, Ss09, TextPos09};
use crate::ling_spa::syllab::syl2;
use crate::ling_spa::{analysis, breaks, context, syl, wr, SsSpa, WaSpa, SYL_SIZE};
use crate::prosody;
use crate::prosody_spa::{self, TreesSpa};
use crate::text_spa::{self, initial::initial_spa, FeSpa};
use crate::tree3::Tree;
use crate::unitsel::classes::Classes;
use crate::unitsel::{CepDist, ScalarDist, Selector, Word};
use crate::wave::{self, DecodeWindow, PauseState, PhraseInput, WordInfo};

/// Word duration array (`s16` per phone), read by unit selection at the same offset.
const WR_DUR: usize = 0x10;
/// The same array in the 2009 word record (0x47c bytes).
const WR_DUR09: usize = 0x0e;

/// Violeta's pad/break bytes: English `Z`/`^`/`[` shifted by -0x14.
const PAD: u8 = 0x46;
const BRK_A: u8 = 0x46;
const BRK_B: u8 = 0x4a;
const BRK_COMMA: u8 = 0x47;

pub struct EngineSpa {
    pub bank: Bank,
    pub cls: Classes,
    pub cep: CepDist,
    pub sd: ScalarDist,
    pub trees: TreesSpa,
    pub spabi: Tree,
    pub fe: FeSpa,
    pub chan: Channel,
    pub era: Era,
}

/// One normalized sentence plus the per-word inputs the back half needs.
pub struct SentenceSpa {
    pub wa: WaSpa,
    pub tib: Tib,
    pub kind7: Vec<i32>,
    pub pitch: Vec<i32>,
    pub speed: Vec<i32>,
    pub volume: Vec<i32>,
    pub map_start: Vec<i32>,
    pub map_end: Vec<i32>,
    pub final_pause: i32,
}

impl EngineSpa {
    /// `data_root` holds `data-violeta\M16` and `data-common\dict-spa`.
    pub fn open(data_root: &Path) -> io::Result<EngineSpa> {
        let m16 = data_root.join("data-violeta").join("M16");
        let dict = data_root.join("data-common").join("dict-spa");
        let bank = Bank::open(&m16)?;
        let era = Era::detect(&bank, &m16, Lang::Spa)?;
        // The 2009 build reads `dist_tbl/cepdist.tbl` (English layout, no `.rd` remap).
        let (cls, cep) = if era.is_2009() {
            (Classes::build_era(&bank, era), CepDist::load(&m16.join("ttsdata").join("dist_tbl").join("cepdist.tbl"))?)
        } else {
            (Classes::build_lang(&bank, Lang::Spa), CepDist::load(&m16.join("ttsdata").join("dist_tbl").join("cepdist.tbl.rd"))?)
        };
        let sd = ScalarDist::build();
        let trees = TreesSpa::load(&m16.join("ttsdata").join("tree3"))?;
        let spabi = Tree::load(&dict.join("spabi.tree3"))?;
        let fe = FeSpa::load_era(&dict, era)?;
        Ok(EngineSpa { bank, cls, cep, sd, trees, spabi, fe, chan: Channel::default(), era })
    }

    pub fn set_params(&mut self, pitch: i32, speed: i32, volume: i32, pause: i32, comma: i32, emphasis: i32) {
        self.chan.apply(pitch, speed, volume, pause, comma, emphasis);
    }

    /// `fe_InitialTextProcess` + `fe_NormalizeTokens` per sentence.
    pub fn text_to_sentences(&self, text: &[u8]) -> Vec<SentenceSpa> {
        let ini = initial_spa(text);
        // The 2009 word builder always maps positions through the initial pass's tables.
        let remap = ini.map_start_differs() || self.era.is_2009();
        let mut cursors = [0usize; 2];
        text_spa::sentences(&self.fe, &ini.processed)
            .into_iter()
            .map(|s| {
                let n = s.wa.n();
                let mut input = SentenceSpa {
                    wa: s.wa,
                    tib: Tib::new(),
                    kind7: vec![-1; n],
                    pitch: vec![self.chan.pitch; n],
                    speed: vec![self.chan.speed; n],
                    volume: vec![self.chan.volume; n],
                    map_start: if remap { ini.map_b.clone() } else { Vec::new() },
                    map_end: if remap { ini.map_c.clone() } else { Vec::new() },
                    final_pause: -1,
                };
                let se: Vec<(i32, i32)> =
                    (0..n).map(|i| (input.wa.i32(WaSpa::rec(i)), input.wa.i32(WaSpa::rec(i) + 4))).collect();
                let out = SpanOut {
                    pitch: &mut input.pitch,
                    speed: &mut input.speed,
                    volume: &mut input.volume,
                    kind7: &mut input.kind7,
                    tib: &mut input.tib,
                    final_pause: &mut input.final_pause,
                };
                apply_tag_spans_core(&se, out, &ini, s.text_off, input.wa.i32(4), &mut cursors);
                input
            })
            .collect()
    }

    /// `fe_LinguisticAnalysis` for one sentence: lexicon/LTS, word/phone
    /// records, phrase breaks (with syllabification) and phone contexts.
    fn run_ling(&self, input: &SentenceSpa, cursor: i32) -> Option<(WaSpa, SsSpa, Tib)> {
        let mut wa = input.wa.clone();
        if !analysis::analyze(&mut wa, &self.fe.lex) {
            return None;
        }
        let mut ss = SsSpa::new();
        ss.set_i16(2, wa.n() as i16);
        let map = |m: &[i32]| if m.is_empty() { None } else { Some(m.to_vec()) };
        let (ms, me) = (map(&input.map_start), map(&input.map_end));
        let pos = TextPos { cursor, map_start: ms.as_deref(), map_end: me.as_deref() };
        if !build_word_phone_records(&mut ss, &wa, &input.kind7, &pos) {
            return None;
        }
        let mut tib = input.tib.clone();
        if !breaks::assign_phrase_breaks(&mut ss, &mut tib, &wa, &self.spabi) {
            return None;
        }
        context::build_phone_contexts(&mut ss, &WordStringsSpa { wa: &wa });
        Some((wa, ss, tib))
    }

    /// Durations, tone and F0, written back into the word/syllable records.
    fn run_prosody(&self, ss: &mut SsSpa) {
        let mut s = sentence_from_ss(ss);
        let ctx: Vec<Vec<[u8; 7]>> = (0..ss.n_words())
            .map(|i| {
                let w = SsSpa::word(i);
                let n = ss.u8(w + wr::N_PHONES) as usize;
                (0..n)
                    .map(|k| {
                        let mut c = [0u8; 7];
                        c.copy_from_slice(&ss.b[w + wr::CTX + 7 * k..w + wr::CTX + 7 * k + 7]);
                        c
                    })
                    .collect()
            })
            .collect();
        prosody_spa::predict_durations_spa(&self.trees, &mut s, &ctx);
        prosody_spa::predict_pitch_spa(&self.trees, &mut s);
        for (i, w) in s.words.iter().enumerate() {
            let wb = SsSpa::word(i);
            for (k, &d) in w.dur.iter().enumerate() {
                ss.set_i16(wb + WR_DUR + 2 * k, d);
            }
            let base = deref_ss(ss.u32(wb + wr::SYL_PTR));
            for (k, sy) in w.syllables.iter().enumerate() {
                let o = base + k * SYL_SIZE;
                ss.set_u8(o + syl::TONE, sy.tone as u8);
                for (j, &f) in sy.f0.iter().enumerate() {
                    ss.set_i16(o + syl::F0 + 2 * j, f);
                }
            }
        }
    }

    /// viol09 `fe_LinguisticAnalysis`: the 2009 lexicon pass, then `spa09`'s word records, phrase breaks
    /// (with syllabification) and 5-byte phone contexts on the 0x47c-byte word layout.
    fn run_ling09(&self, input: &SentenceSpa, cursor: i32) -> Option<(WaSpa, Ss09, Tib)> {
        let mut wa = input.wa.clone();
        if !analysis::analyze(&mut wa, &self.fe.lex) {
            return None;
        }
        let mut ss = Ss09::new();
        ss.set_i16(2, wa.n() as i16);
        let pos = TextPos09 { cursor, n: input.map_start.len() as i32, map_start: &input.map_start, map_end: &input.map_end };
        if !spa09::build_word_phone_records(&mut ss, &wa, &pos) {
            return None;
        }
        let mut tib = input.tib.clone();
        if !spa09::assign_phrase_breaks(&mut ss, &mut tib, &wa, &self.spabi) {
            return None;
        }
        spa09::build_phone_contexts(&mut ss);
        Some((wa, ss, tib))
    }

    /// viol09 durations, tone and F0 (`FUN_1001ad00`, `FUN_1001b1c0`), written back at word +0x0e and into
    /// the syllable records.
    fn run_prosody09(&self, ss: &mut Ss09) {
        let mut s = prosody::Sentence { era: self.era, ..Default::default() };
        let mut ctx: Vec<Vec<[u8; 7]>> = Vec::new();
        for i in 0..ss.n_words() {
            let w = Ss09::word(i);
            let n = ss.u8(w + w09::N_PHONES) as usize;
            let codes: Vec<u8> = (0..n).map(|k| ss.u8(w + w09::CODES + k)).collect();
            let pos_code: Vec<i8> = (0..n).map(|k| ss.u8(w + w09::POS + k) as i8).collect();
            ctx.push(
                (0..n)
                    .map(|k| {
                        let c = w + w09::CTX + 5 * k;
                        [ss.u8(c), ss.u8(c + 1), ss.u8(c + 2), ss.u8(c + 3), ss.u8(c + 4), 0, 0]
                    })
                    .collect(),
            );
            let base = deref_ss(ss.u32(w + w09::SYL_PTR));
            let mut first_phone = 0u8;
            let syllables = (0..ss.u8(w + w09::N_SYL) as usize)
                .map(|g| {
                    let rec = base + g * SYL_SIZE;
                    let np = ss.u8(rec + syl::N_PHONES);
                    let y = prosody::Syllable {
                        b0: ss.u8(rec + syl2::STRESS) as i8,
                        b1: ss.u8(rec + syl2::SHAPE) as i8,
                        b2: ss.u8(rec + syl2::POSITION) as i8,
                        tone: 0,
                        f0: [0; 12],
                        first_phone,
                        n_phones: np,
                    };
                    first_phone += np;
                    y
                })
                .collect();
            s.words.push(prosody::Word { codes, pos_code, syllables, brk: ss.u8(w + w09::BRK), dur: vec![0; n] });
        }
        s.phrase_words = (0..ss.n_phrases()).map(|g| ss.i16(Ss09::phrase(g) + 2).max(0) as usize).collect();
        prosody_spa::predict_durations_spa(&self.trees, &mut s, &ctx);
        prosody_spa::predict_pitch_spa(&self.trees, &mut s);
        for (i, w) in s.words.iter().enumerate() {
            let wb = Ss09::word(i);
            for (k, &d) in w.dur.iter().enumerate() {
                ss.set_i16(wb + WR_DUR09 + 2 * k, d);
            }
            let base = deref_ss(ss.u32(wb + w09::SYL_PTR));
            for (k, sy) in w.syllables.iter().enumerate() {
                let o = base + k * SYL_SIZE;
                ss.set_u8(o + syl::TONE, sy.tone as u8);
                for (j, &f) in sy.f0.iter().enumerate() {
                    ss.set_i16(o + syl::F0 + 2 * j, f);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn synth_sentence09(
        &self,
        sel: &mut Selector,
        ss: &Ss09,
        tib: &Tib,
        input: &SentenceSpa,
        cont: &mut bool,
        out: &mut Vec<i16>,
        win: &mut DecodeWindow,
        trace: &mut Vec<wave::RecTrace>,
    ) -> io::Result<()> {
        let nw = ss.n_words();
        let mut u_words: Vec<_> = (0..nw)
            .map(|i| {
                let w = Ss09::word(i);
                let base = deref_ss(ss.u32(w + w09::SYL_PTR));
                let syls: Vec<Vec<u8>> =
                    (0..ss.u8(w + w09::N_SYL) as usize).map(|g| ss.b[base + g * SYL_SIZE..base + (g + 1) * SYL_SIZE].to_vec()).collect();
                Word::from_raw_era(&ss.b[w..w + spa09::WORD_SIZE], &syls, self.era)
            })
            .collect();
        let words: Vec<WordInfo> = (0..nw)
            .map(|i| WordInfo {
                brk: ss.u8(Ss09::word(i) + w09::BRK),
                pitch: input.pitch.get(i).copied().unwrap_or(100),
                speed: input.speed.get(i).copied().unwrap_or(100),
                volume: input.volume.get(i).copied().unwrap_or(100),
            })
            .collect();
        let np = ss.n_phrases();
        let mut pause_st = PauseState {
            word_pauses: tib.pause[..=nw.min(tib.pause.len() - 1)].to_vec(),
            cont_flag: *cont,
            final_pause_ms: input.final_pause,
            ch_pitch: self.chan.pitch,
            ch_volume: self.chan.volume,
            ch_comma_ms: self.chan.comma_ms,
            word0_start: if nw > 0 { ss.i32(Ss09::word(0) + w09::START) } else { 0 },
            span_reset: None,
        };
        for g in 0..np {
            let e = Ss09::phrase(g);
            let first = ss.i16(e).max(0) as usize;
            let nwp = ss.i16(e + 2).max(0) as usize;
            let r = sel.select_phrase(&mut u_words, first, nwp, 5);
            let targets = phrase_wave_targets(&self.bank, &r)?;
            let center = |w: usize, k: usize| -> u8 { ss.u8(Ss09::word(w) + w09::CTX + 5 * k + 1) };
            let recs = wave::build_records_lang(&targets, &words, &center, first as u16, g + 1 == np, &mut pause_st, PAD, BRK_A, BRK_B, BRK_COMMA);
            let halves: Vec<u8> = r.targets.iter().map(|t| t.half).collect();
            let phrase_input = PhraseInput { records: &recs, uids: &r.uids, halves: &halves };
            wave::generate_phrase_traced_era(win, &self.bank, &phrase_input, out, Some(trace), self.era)?;
        }
        *cont = pause_st.cont_flag;
        Ok(())
    }

    pub fn synth_sentences_traced(&self, sentences: &[SentenceSpa]) -> io::Result<(Vec<i16>, Vec<usize>)> {
        if self.era.is_2009() {
            return self.synth_sentences09_traced(sentences);
        }
        let mut sel = Selector::new_lang(&self.bank, &self.cls, &self.cep, &self.sd, Lang::Spa);
        sel.reset();
        let mut win = DecodeWindow::default();
        let mut out = Vec::new();
        let mut trace = Vec::new();
        let mut cont = true;
        let mut cursor = 0i32;
        for input in sentences {
            let Some((wa, mut ss, mut tib)) = self.run_ling(input, cursor) else { continue };
            cursor += wa.i32(4);
            let mut p0 = tib.pause[0];
            prosody::resolve_sentence_pause(&mut p0, -1, -1, cont, self.chan.sent_pause_ms);
            tib.pause[0] = p0;
            self.run_prosody(&mut ss);
            self.synth_sentence(&mut sel, &ss, &tib, input, &mut cont, &mut out, &mut win, &mut trace)?;
        }
        let ends = wave::chunk_ends(&trace, out.len());
        let mut st = 0i16;
        wave::formats::emphasis(&mut out, self.chan.emphasis, &mut st);
        Ok((out, ends))
    }

    #[allow(clippy::too_many_arguments)]
    fn synth_sentence(
        &self,
        sel: &mut Selector,
        ss: &SsSpa,
        tib: &Tib,
        input: &SentenceSpa,
        cont: &mut bool,
        out: &mut Vec<i16>,
        win: &mut DecodeWindow,
        trace: &mut Vec<wave::RecTrace>,
    ) -> io::Result<()> {
        let nw = ss.n_words();
        let mut u_words: Vec<_> = (0..nw).map(|i| word_from_ss(ss, i)).collect();
        let words: Vec<WordInfo> = (0..nw)
            .map(|i| WordInfo {
                brk: ss.u8(SsSpa::word(i) + wr::BRK),
                pitch: input.pitch.get(i).copied().unwrap_or(100),
                speed: input.speed.get(i).copied().unwrap_or(100),
                volume: input.volume.get(i).copied().unwrap_or(100),
            })
            .collect();
        let np = ss.n_phrases();
        let mut pause_st = PauseState {
            word_pauses: tib.pause[..=nw.min(tib.pause.len() - 1)].to_vec(),
            cont_flag: *cont,
            final_pause_ms: input.final_pause,
            ch_pitch: self.chan.pitch,
            ch_volume: self.chan.volume,
            ch_comma_ms: self.chan.comma_ms,
            word0_start: if nw > 0 { ss.i32(SsSpa::word(0) + wr::START) } else { 0 },
            span_reset: None,
        };
        for g in 0..np {
            let e = SsSpa::phrase(g);
            let first = ss.i16(e).max(0) as usize;
            let nwp = ss.i16(e + 2).max(0) as usize;
            // Violeta's target cost never reads the final-type byte.
            let r = sel.select_phrase(&mut u_words, first, nwp, 5);
            let targets = phrase_wave_targets(&self.bank, &r)?;
            let center = |w: usize, k: usize| -> u8 { ss.u8(SsSpa::word(w) + wr::CTX + 7 * k + 2) };
            let recs = wave::build_records_lang(&targets, &words, &center, first as u16, g + 1 == np, &mut pause_st, PAD, BRK_A, BRK_B, BRK_COMMA);
            let halves: Vec<u8> = r.targets.iter().map(|t| t.half).collect();
            let phrase_input = PhraseInput { records: &recs, uids: &r.uids, halves: &halves };
            wave::generate_phrase_traced(win, &self.bank, &phrase_input, out, Some(trace))?;
        }
        *cont = pause_st.cont_flag;
        Ok(())
    }

    fn synth_sentences09_traced(&self, sentences: &[SentenceSpa]) -> io::Result<(Vec<i16>, Vec<usize>)> {
        let mut sel = Selector::new_era(&self.bank, &self.cls, &self.cep, &self.sd, self.era);
        sel.reset();
        let mut win = DecodeWindow::default();
        let mut out = Vec::new();
        let mut trace = Vec::new();
        let mut cont = true;
        let mut cursor = 0i32;
        for input in sentences {
            let Some((wa, mut ss, mut tib)) = self.run_ling09(input, cursor) else { continue };
            cursor += wa.i32(4);
            let mut p0 = tib.pause[0];
            prosody::resolve_sentence_pause(&mut p0, -1, -1, cont, self.chan.sent_pause_ms);
            tib.pause[0] = p0;
            self.run_prosody09(&mut ss);
            self.synth_sentence09(&mut sel, &ss, &tib, input, &mut cont, &mut out, &mut win, &mut trace)?;
        }
        let ends = wave::chunk_ends(&trace, out.len());
        let mut st = 0i16;
        wave::formats::emphasis(&mut out, self.chan.emphasis, &mut st);
        Ok((out, ends))
    }

    pub fn speak_traced(&self, text: &[u8]) -> io::Result<(Vec<i16>, Vec<usize>)> {
        let sentences = self.text_to_sentences(text);
        self.synth_sentences_traced(&sentences)
    }

    pub fn speak(&self, text: &[u8]) -> io::Result<Vec<i16>> {
        self.speak_traced(text).map(|(pcm, _)| pcm)
    }
}
