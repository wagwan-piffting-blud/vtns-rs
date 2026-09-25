//! End-to-end synthesis pipeline: normalized word array (WA) -> linguistic
//! front end -> prosody -> unit selection -> waveform -> PCM, plus `speak`
//! (raw text -> PCM) through the text front end. Wires the bit-exact
//! per-stage modules (`ling`, `prosody`, `unitsel`, `wave`) together.
//!
//! The stages share the engine's own in-memory structures the way the real
//! DLL does: `run_ling` builds an `Ss` from a `Wa`, `run_prosody` writes
//! phone durations and syllable tone/F0 back into that same `Ss` (`+0x10+2k`,
//! syllable `+3`/`+4..+0x1b`), and unit selection / the waveform stage read
//! them back out of it (`unitsel::Word::from_raw`, the `center` closure in
//! `synth_sentence`) -- nothing is threaded through a side channel.

use std::io;
use std::path::Path;

use crate::bank::Bank;
use crate::lexicon::Lexicon;
use crate::ling::{self, wr, Ss, Tib, Wa, WordStrings, Ws};
use crate::prosody::{self, Trees};
use crate::text;
use crate::unitsel::classes::Classes;
use crate::unitsel::{self, CepDist, PhraseResult, ScalarDist, Selector};
use crate::wave::{self, DecodeWindow, PauseState, PhraseInput, WordInfo};

/// Word-record offset of the predicted phone duration (16 kHz samples),
/// `+0x10 + 2k`. Not in `ling::wr`: prosody, not the linguistic front end,
/// owns this field (see `unitsel::Word::from_raw`, which reads the same
/// offset back out).
const WR_DUR: usize = 0x10;

/// Per-request channel state (`VT_SetPitchSpeedVolumePause_ENG` /
/// `VT_SetCommaPause_ENG` / `VT_SetEmphasisFactor_ENG`, read back by
/// `wav_BuildUnitRecords` off the speaker struct at `+0x4cf0..+0x4d04`).
/// Defaults confirmed from the `out_wave` captures' `records[].inp` on the
/// default-parameter captures: speed/pitch/volume 100, sentence pause 687 ms,
/// comma pause 200 ms.
#[derive(Debug, Clone, Copy)]
pub struct Channel {
    pub speed: i32,
    pub pitch: i32,
    pub volume: i32,
    pub sent_pause_ms: i32,
    pub comma_ms: i32,
    pub emphasis: i32,
}

impl Default for Channel {
    fn default() -> Self {
        Channel { speed: 100, pitch: 100, volume: 100, sent_pause_ms: 687, comma_ms: 200, emphasis: 0 }
    }
}

impl Channel {
    /// Applies the original API's parameter convention: a negative value
    /// keeps the current one. Clamps as `VT_SetPitchSpeedVolumePause_ENG`
    /// and `VT_SetCommaPause_ENG` do.
    pub fn apply(&mut self, pitch: i32, speed: i32, volume: i32, pause: i32, comma: i32, emphasis: i32) {
        if pitch >= 0 {
            self.pitch = pitch.clamp(50, 200);
        }
        if speed >= 0 {
            self.speed = speed.clamp(50, 400);
        }
        if volume >= 0 {
            self.volume = volume.min(500);
        }
        if pause >= 0 {
            self.sent_pause_ms = pause.min(0xffff);
        }
        if comma >= 0 {
            self.comma_ms = comma.min(0xffff);
        }
        self.emphasis = wave::formats::clamp_emphasis(emphasis);
    }
}

/// One normalized sentence ready for the linguistic front end: the WA the
/// text stage produced, plus the other TIB arrays it fills in alongside it
/// (`ling::Tib` covers pause/break-level; `kind7`/`pitch`/`speed`/`volume`
/// are the remaining three `TIB+0x121100 + 0x320*a` arrays, as captured by
/// the original engine's `dumpSS` trace).
///
/// There is deliberately no `cont`/`final_pause_ms` field here: both are
/// cross-sentence *request* state, not per-sentence input, and `Engine`
/// threads them through `synth_from_wa` itself (see `resolve_word0_pause`
/// and its call site) -- an earlier version of this struct carried a
/// `cont`/`final_pause_ms` pair set per sentence by the caller, which got
/// the right total sample count for the whole 42-text corpus but for the
/// wrong reason (a `final_pause_ms` heuristic keyed on sentence position,
/// standing in for the real per-request `cont_flag` state machine reversed
/// afterwards).
pub struct SentenceInput {
    pub wa: Wa,
    pub tib: Tib,
    pub kind7: Vec<i32>,
    pub pitch: Vec<i32>,
    pub speed: Vec<i32>,
    pub volume: Vec<i32>,
    /// `fe_InitialTextProcess`' processed -> original start/end maps
    /// (`TIB+0x47794/98`); empty means identity.
    pub map_start: Vec<i32>,
    pub map_end: Vec<i32>,
    /// `TIB+0x122440`: VTML pause after the last word of the request, -1 if none.
    pub final_pause: i32,
}

impl SentenceInput {
    /// A sentence with no VTML overrides: channel-rate words, no forced
    /// pauses/breaks/EXP-domain words.
    pub fn plain(wa: Wa, chan: &Channel) -> SentenceInput {
        let n = wa.n();
        SentenceInput {
            wa,
            tib: Tib::new(),
            kind7: vec![-1; n],
            pitch: vec![chan.pitch; n],
            speed: vec![chan.speed; n],
            volume: vec![chan.volume; n],
            map_start: Vec::new(),
            map_end: Vec::new(),
            final_pause: -1,
        }
    }
}

/// `pro_ApplyTagSpansToWords` (0x10022970), run by `fe_NextSentence` on
/// processed-text positions before they are mapped back to the original.
/// Pitch/speed/volume/kind7 (channels 0/1/2/7): each word takes the value of
/// the span entry in force at its start (a close restores the value before
/// its open, which the channel pairing already stored). Pause (3): entries
/// between the previous word's end and this word's start add up on the word;
/// after the last word of the request they become the final pause. Break
/// (4): the last entry between a word's end and the next word's start. The
/// pause/break cursors persist across sentences. Clamps `0x1007d66c`/`d690`.
fn apply_tag_spans(input: &mut SentenceInput, ini: &text::initial::Initial, cursor: i32, sent_len: i32, cur: &mut [usize; 2]) {
    const MAX: [i32; 8] = [200, 400, 500, 65535, 3, 0, 9, 9];
    const MIN: [i32; 8] = [50, 50, 0, 0, 0, 0, 0, 0];
    let n = input.wa.n();
    let start: Vec<i32> = (0..n).map(|i| cursor + input.wa.i32(Wa::rec(i))).collect();
    let end: Vec<i32> = (0..n)
        .map(|i| {
            let (s, e) = (input.wa.i32(Wa::rec(i)), input.wa.i32(Wa::rec(i) + 4));
            if s < e {
                cursor + e - 1
            } else {
                cursor + s
            }
        })
        .collect();
    for ch in [0usize, 1, 2, 7] {
        let c = &ini.chans[ch];
        let arr = match ch {
            0 => &mut input.pitch,
            1 => &mut input.speed,
            2 => &mut input.volume,
            _ => &mut input.kind7,
        };
        let (mut w, mut filled) = (0usize, 0usize);
        for k in 0..c.pos.len() {
            while w < n && c.pos[k] > start[w] {
                w += 1;
            }
            if k == 0 {
                filled = filled.max(w);
            } else if filled < w {
                let v = c.val[k - 1];
                for x in &mut arr[filled..w] {
                    let mut y = if v < 0 { *x } else { v };
                    if !(ch == 7 && y == -1) {
                        y = y.max(MIN[ch]).min(MAX[ch]);
                    }
                    *x = y;
                }
                filled = w;
            }
            if k + 1 == c.pos.len() && filled < n {
                filled = n;
            }
        }
    }
    let scan = |c: &text::initial::Chan, from: usize, lo: i32, hi: i32| (from..c.pos.len()).find(|&i| lo <= c.pos[i] && c.pos[i] <= hi);
    for ch in [3usize, 4] {
        let c = &ini.chans[ch];
        if c.pos.is_empty() {
            continue;
        }
        for w in 0..n {
            let (lo, hi) = if ch == 3 {
                if w == 0 {
                    (0, start[0])
                } else {
                    (end[w - 1], start[w])
                }
            } else if w + 1 == n {
                (end[w], cursor + sent_len)
            } else {
                (end[w], start[w + 1])
            };
            let mut first = true;
            let mut idx = cur[ch - 3];
            while let Some(m) = scan(c, idx, lo, hi) {
                idx = m + 1;
                if ch == 3 {
                    let p = &mut input.tib.pause[w];
                    *p = if first { c.val[m] } else { *p + c.val[m] };
                    *p = (*p).max(MIN[3]).min(MAX[3]);
                } else {
                    input.tib.brk_level[w] = c.val[m];
                }
                first = false;
                cur[ch - 3] = idx;
            }
        }
    }
    input.final_pause = -1;
    let c = &ini.chans[3];
    if n > 0 && cursor + sent_len == ini.processed.len() as i32 {
        let mut first = true;
        while let Some(m) = scan(c, cur[0], end[n - 1], cursor + sent_len) {
            let p = if first { c.val[m] } else { input.final_pause + c.val[m] };
            input.final_pause = p.max(MIN[3]).min(MAX[3]);
            cur[0] = m + 1;
            first = false;
        }
    }
}

/// `pro_ResolveSentencePause` (0x10026750): resolves `word_pauses[0]`
/// (`tib.pause[0]`, `SS+0x4770c[0]`) for one sentence from the per-call
/// pause override (`TIB+0x1312c8`, always -1 here -- `VT_TextToBuffer_ENG`
/// is always called with -1 for it), the VTML
/// tib-pause override (`TIB+0x1312c4`, always -1 -- not modeled, no corpus
/// text needs it), the request's `cont` flag and the channel's default
/// sentence pause. The formula is bit-exact (`[C 73/73]`); this just calls
/// it with this request's live `cont` state instead of a capture's frozen
/// one.
///
/// Only resets `tib.pause[0]` to "unset" first when it's `<= 0`; a genuine
/// positive override (a VTML pause span landing in word 0's window, `[0,
/// start_0]` per section 7.3) is left untouched, matching the real function's
/// own `if p0 < 0` guard. This distinction matters because the `out_feling`/
/// `out_engine` captures (captured through `fedump.exe`'s mode-7 dump, not the
/// real `wav_FillChunk` synthesis driver) never actually run any audio
/// synthesis, so `wave::pause_before` -- the *only* thing that ever clears
/// `cont_flag` -- never fires there either: confirmed live via a debug trace,
/// `cont_flag` reads `1` for every sentence of a multi-sentence
/// `fedump`/`out_engine` capture, so its own `pro_ResolveSentencePause` call
/// can only ever produce `0`, never the real `ch_sent_pause` (687 ms) a
/// later sentence should get from a live `vtprobe.exe` run. A captured `0`
/// is therefore ambiguous (an unset value resolved by `fedump`'s always-true
/// `cont`, or a genuine `<vtml_pause time="0"/>`); resolving it fresh from
/// this request's real `cont` gets the ordinary case right at the cost of
/// that one rare tie. A captured *positive* value can't come from
/// `fedump`'s `resolve` call at all (which only ever yields 0 or fails the
/// `p0 < 0` guard) -- it can only be a genuine span override -- so it's kept.
/// See `tests/engine_parity.rs`'s `pause_boundary_edge_cases` (`pause_at_boundary`).
pub fn resolve_word0_pause(tib: &mut Tib, chan: &Channel, cont: bool) {
    let mut p0 = tib.pause[0];
    if p0 <= 0 {
        p0 = -1;
    }
    prosody::resolve_sentence_pause(&mut p0, -1, -1, cont, chan.sent_pause_ms);
    tib.pause[0] = p0;
}

/// Run the linguistic front end on one already-normalized `Wa`: lookup,
/// copy, morphology, the wh-flag pass and function words (`tag_and_pronounce`),
/// the merge back into the `Wa` (`merge::merge`), then
/// `fe_BuildWordPhoneRecords` / `fe_AssignPhraseBreaks` (which syllabifies)
/// / `fe_BuildPhoneContexts` -- `fe_LinguisticAnalysis`'s own order, chained
/// exactly as `feling_parity.rs`'s `chain_stage` proves it end to end.
/// `cursor` is the sum of every earlier sentence's own `Wa::consumed()`
/// within the current request (`0` for the first sentence) -- see
/// `ling::words::build_word_phone_records`'s own doc for why
/// `wr::START`/`wr::END` need it (`fe_NextSentence`'s job in the real
/// engine, folded into this function instead of its own port). Passed
/// through as `ling::words::TextPos { cursor, map_start: None, map_end: None }`
/// -- the VTML/fullwidth remap tables aren't modeled here (no caller of
/// `run_ling` produces them), so `wr::START`/`wr::END` come out right for
/// plain text but not remapped for VTML-tag-stripped positions; see
/// `TextPos`'s own doc. Returns the
/// merged `Wa`, the finished `Ss`, and the post-breaks `Tib` (its `pause`
/// array is what `wav_BuildUnitRecords` reads at `SS+0x4770c`). `None` on
/// the engine's own failure paths (merge word-count mismatch, a word with
/// no phones, or a sentence too long to break under the 1000-half-phone cap).
pub fn run_ling(lex: &Lexicon, input: &SentenceInput, cursor: i32) -> Option<(Wa, Ss, Tib)> {
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
    if !ling::words::build_word_phone_records(&mut ss, &wa, &input.kind7, &pos) {
        return None;
    }
    let mut tib = input.tib.clone();
    if !ling::breaks::assign_phrase_breaks(&mut ss, &mut tib, &wa, &lex.engbi) {
        return None;
    }
    let strs = WordStrings { wa: &wa };
    ling::context::build_phone_contexts(&mut ss, &strs);
    Some((wa, ss, tib))
}

/// Prosody's view of a finished `Ss`: phone codes/syllable-position codes,
/// syllable attributes and per-word breaks, read straight off the raw
/// record bytes `run_ling` left. Durations/tone/F0 start zeroed; call
/// `predict_durations`/`predict_pitch` on the result, then `write_prosody`
/// to store them back into the `Ss`.
pub fn ss_to_prosody(ss: &Ss) -> prosody::Sentence {
    let mut s = prosody::Sentence::default();
    let nw = ss.n_words();
    for i in 0..nw {
        let w = Ss::word(i);
        let n = ss.u8(w + wr::N_PHONES) as usize;
        let codes: Vec<u8> = (0..n).map(|k| ss.u8(w + wr::CODES + k)).collect();
        let pos_code: Vec<i8> = (0..n).map(|k| ss.u8(w + wr::SYLPOS + k) as i8).collect();
        let nsyl = ss.u8(w + wr::N_SYL) as usize;
        let syl_base = ss.deref(ss.u32(w + wr::SYL_PTR));
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
        let brk = ss.u8(w + wr::BRK);
        s.words.push(prosody::Word { codes, pos_code, syllables, brk, dur: vec![0; n] });
    }
    let np = ss.n_phrases();
    s.phrase_words = (0..np).map(|g| ss.i16(Ss::phrase(g) + 2).max(0) as usize).collect();
    s
}

/// Writes `predict_durations`/`predict_pitch`'s results back into the `Ss`
/// word/syllable records at the offsets unit selection and the waveform
/// stage read them from -- the shared in-memory layout the real engine uses,
/// rather than a side channel.
pub fn write_prosody(ss: &mut Ss, s: &prosody::Sentence) {
    for (i, w) in s.words.iter().enumerate() {
        let wb = Ss::word(i);
        for (k, &d) in w.dur.iter().enumerate() {
            ss.set_i16(wb + WR_DUR + 2 * k, d);
        }
        let Some(syl_base) = ss.deref(ss.u32(wb + wr::SYL_PTR)) else { continue };
        for (k, sy) in w.syllables.iter().enumerate() {
            let o = syl_base + k * ling::SYL_SIZE;
            ss.set_u8(o + ling::syl::TONE, sy.tone as u8);
            for (j, &f) in sy.f0.iter().enumerate() {
                ss.set_i16(o + ling::syl::F0 + 2 * j, f);
            }
        }
    }
}

/// `run_ling` + prosody (durations, tone, smoothed F0) on one sentence,
/// leaving the results in `ss`.
pub fn run_prosody(trees: &Trees, ss: &mut Ss) {
    let mut s = ss_to_prosody(ss);
    prosody::predict_durations(trees, &mut s);
    prosody::predict_pitch(trees, &mut s);
    write_prosody(ss, &s);
}

/// Unit selection's view of every word in a finished, prosody-filled `Ss`
/// (`unitsel::Word::from_raw` over the same raw record/syllable bytes
/// `write_prosody` just wrote).
pub fn ss_to_unitsel_words(ss: &Ss) -> Vec<unitsel::Word> {
    let nw = ss.n_words();
    (0..nw)
        .map(|i| {
            let w = Ss::word(i);
            let raw = ss.b[w..w + ling::WORD_SIZE].to_vec();
            let nsyl = ss.u8(w + wr::N_SYL) as usize;
            let syl_base = ss.deref(ss.u32(w + wr::SYL_PTR));
            let syls: Vec<Vec<u8>> = (0..nsyl)
                .map(|k| {
                    let o = syl_base.unwrap_or(0) + k * ling::SYL_SIZE;
                    ss.b[o..o + ling::SYL_SIZE].to_vec()
                })
                .collect();
            unitsel::Word::from_raw(&raw, &syls)
        })
        .collect()
}

/// Per-word waveform inputs (`wave::WordInfo`): the break code `assign_phrase_breaks`
/// resolved, and the per-word rates the caller supplied (channel defaults,
/// or VTML-resolved rates once the text front end produces them).
pub fn ss_word_info(ss: &Ss, input: &SentenceInput) -> Vec<WordInfo> {
    let nw = ss.n_words();
    (0..nw)
        .map(|i| WordInfo {
            brk: ss.u8(Ss::word(i) + wr::BRK),
            pitch: input.pitch.get(i).copied().unwrap_or(100),
            speed: input.speed.get(i).copied().unwrap_or(100),
            volume: input.volume.get(i).copied().unwrap_or(100),
        })
        .collect()
}

/// One phrase's unit-selection result turned into the waveform stage's
/// target list (`wave::Target`), resolving each chosen unit's DB entry and
/// `.dat`/`.upm` file index from the bank.
pub fn phrase_wave_targets(bank: &Bank, r: &PhraseResult) -> io::Result<Vec<wave::Target>> {
    r.targets
        .iter()
        .zip(&r.uids)
        .map(|(t, &uid)| {
            let db = bank.primary(uid)?;
            let file = bank.file_idx[uid as usize];
            Ok(wave::Target { word: t.word, phone: t.phone, half: t.half, db, file })
        })
        .collect()
}

pub struct Engine {
    pub bank: Bank,
    pub cls: Classes,
    pub cep: CepDist,
    pub sd: ScalarDist,
    pub trees: Trees,
    pub lex: Lexicon,
    pub fe: text::Fe,
    pub chan: Channel,
    /// Match the 64-bit voice DLLs instead of the 32-bit vt_eng.dll (see `Selector::x64`).
    pub x64: bool,
}

impl Engine {
    /// Loads the voice once. `data_root` is the engine install directory
    /// (e.g. `C:\Program Files (x86)\NWSPaulNumber2`, holding `data-paul\M16`
    /// and `data-common`); never hard-coded here so the same binary runs
    /// against any voice install directory on any target.
    pub fn open(data_root: &Path) -> io::Result<Engine> {
        Engine::open_voice(data_root, "paul")
    }

    /// `voice` is the data-directory name the engine DLL is built with:
    /// `paul`, `kate`, `juli` or `jame` (`data-<voice>\M16`). The English
    /// voice DLLs differ only in that string.
    pub fn open_voice(data_root: &Path, voice: &str) -> io::Result<Engine> {
        let m16 = data_root.join(format!("data-{}", voice)).join("M16");
        let common = data_root.join("data-common");
        let dict = common.join("dict-eng");
        let bank = Bank::open(&m16)?;
        let cls = Classes::build(&bank);
        let cep = CepDist::load(&m16.join("ttsdata").join("dist_tbl").join("cepdist.tbl"))?;
        let sd = ScalarDist::build();
        let trees = Trees::load(&m16.join("ttsdata").join("tree3"))?;
        let lex = Lexicon::open(&dict)?;
        let fe = text::Fe::load(&common)?;
        Ok(Engine { bank, cls, cep, sd, trees, lex, fe, chan: Channel::default(), x64: false })
    }

    /// `VT_SetPitchSpeedVolumePause_ENG` / `VT_SetCommaPause_ENG` /
    /// `VT_SetEmphasisFactor_ENG` on the one channel this `Engine` holds;
    /// `-1` keeps the current value (the original API's convention).
    /// Reproduce the 64-bit voice DLLs instead of the 32-bit vt_eng.dll.
    pub fn set_x64(&mut self, on: bool) {
        self.x64 = on;
        self.fe.x64 = on;
    }

    pub fn set_params(&mut self, pitch: i32, speed: i32, volume: i32, pause: i32, comma: i32, emphasis: i32) {
        self.chan.apply(pitch, speed, volume, pause, comma, emphasis);
    }

    /// `speak`'s own front-end step: raw text -> `SentenceInput`s, using
    /// `crate::text`. No VTML span data reaches the WA/TIB through it yet, so
    /// pitch/speed/volume/kind7 stay at channel defaults and forced pause/break
    /// overrides are only as good as `war::PAUSE` in the WA the text stage
    /// produced; `speak` is only as exact as that stage.
    pub fn text_to_sentences(&self, text: &[u8]) -> Vec<SentenceInput> {
        let ini = text::initial::initial(text);
        let processed = text::padded(&ini.processed);
        let mut cursors = [0usize; 2];
        text::sentences(&self.fe, &processed)
            .into_iter()
            .map(|s| {
                let mut wa = Wa::new();
                wa.b.copy_from_slice(&s.wa.b[..ling::WA_SIZE]);
                let mut input = SentenceInput::plain(wa, &self.chan);
                if ini.map_start_differs() {
                    input.map_start = ini.map_b.clone();
                    input.map_end = ini.map_c.clone();
                }
                apply_tag_spans(&mut input, &ini, s.text_off, s.wa.consumed(), &mut cursors);
                input
            })
            .collect()
    }

    /// Synthesizes one sentence's phrases in order, appending PCM to `out`
    /// and advancing `win`'s cross-phrase join state and `sel`'s persistent
    /// candidate pool. `ss`/`tib` are `run_ling`'s (already prosody-filled
    /// by `run_prosody`, `tib.pause[0]` already resolved by
    /// `resolve_word0_pause`) output for this sentence. `cont` is the
    /// request-level continuation flag (see `resolve_word0_pause`): read for
    /// this sentence's `PauseState`, then written back with whatever
    /// `wave::pause_before` left it as (cleared the first time any sentence's
    /// word 0 actually gets a leading gap), for the next sentence to read.
    /// `final_pause_ms` is the request's trailing-pause override
    /// (`TIB+0x122440`, always -1 here -- see `SentenceInput`'s doc; a real
    /// VTML end-of-text pause would need to reach this from the text stage).
    fn synth_sentence(
        &self,
        sel: &mut Selector,
        ss: &Ss,
        tib: &Tib,
        input: &SentenceInput,
        cont: &mut bool,
        final_pause_ms: i32,
        out: &mut Vec<i16>,
        win: &mut DecodeWindow,
        trace: &mut Vec<wave::RecTrace>,
    ) -> io::Result<()> {
        let nw = ss.n_words();
        let mut u_words = ss_to_unitsel_words(ss);
        let words = ss_word_info(ss, input);
        let np = ss.n_phrases();
        let sent_type = ss.u8(ling::SS_TYPE);
        let mut pause_st = PauseState {
            word_pauses: tib.pause[..=nw.min(tib.pause.len() - 1)].to_vec(),
            cont_flag: *cont,
            final_pause_ms,
            ch_pitch: self.chan.pitch,
            ch_volume: self.chan.volume,
            ch_comma_ms: self.chan.comma_ms,
            word0_start: if nw > 0 { ss.i32(Ss::word(0) + wr::START) } else { 0 },
            span_reset: None,
        };
        let mut first = 0usize;
        for g in 0..np {
            let e = Ss::phrase(g);
            let nwp = ss.i16(e + 2).max(0) as usize;
            let ftype = if g + 1 == np { sent_type } else { 5 };
            let r = sel.select_phrase(&mut u_words, first, nwp, ftype);
            let targets = phrase_wave_targets(&self.bank, &r)?;
            let center = |w: usize, k: usize| -> u8 { ss.u8(Ss::word(w) + wr::CTX + 7 * k + 2) };
            let recs = wave::build_records(&targets, &words, &center, first as u16, g + 1 == np, &mut pause_st);
            let halves: Vec<u8> = r.targets.iter().map(|t| t.half).collect();
            let phrase_input = PhraseInput { records: &recs, uids: &r.uids, halves: &halves };
            wave::generate_phrase_traced(win, &self.bank, &phrase_input, out, Some(trace))?;
            first += nwp;
        }
        *cont = pause_st.cont_flag;
        Ok(())
    }

    /// Full pipeline from already-normalized sentences to PCM: `run_ling` +
    /// `run_prosody` + unit selection + waveform generation per sentence,
    /// then the channel's emphasis filter over the whole request (matching
    /// `wave_parity.rs`'s `synth_capture`, which applies it once over the
    /// concatenated PCM of every sentence/phrase, not per phrase). Also
    /// returns `wave::chunk_ends` over the whole request, for callers
    /// encoding a chunk-sensitive format (Dialogic ADPCM).
    ///
    /// `cont` (`TIB+0x122850`) is seeded `true` here, exactly like
    /// `FUN_10025fc0` seeds it once per TIB allocation (i.e. once per
    /// request): true only until the first sentence's word 0 actually
    /// consumes a leading gap in the waveform stage, false for every
    /// sentence after that for the rest of the request -- see
    /// `resolve_word0_pause`'s doc for how this was reversed (live
    /// `vtprobe.exe` tracing, not just the decompile).
    pub fn synth_from_wa_traced(&self, sentences: &[SentenceInput]) -> io::Result<(Vec<i16>, Vec<usize>)> {
        let mut sel = Selector::new(&self.bank, &self.cls, &self.cep, &self.sd);
        sel.x64 = self.x64;
        sel.reset();
        let mut win = DecodeWindow::default();
        let mut out = Vec::new();
        let mut trace = Vec::new();
        let mut cont = true;
        let mut cursor = 0i32;
        for input in sentences {
            let Some((wa, mut ss, mut tib)) = run_ling(&self.lex, input, cursor) else { continue };
            cursor += wa.consumed();
            resolve_word0_pause(&mut tib, &self.chan, cont);
            run_prosody(&self.trees, &mut ss);
            self.synth_sentence(&mut sel, &ss, &tib, input, &mut cont, input.final_pause, &mut out, &mut win, &mut trace)?;
        }
        let ends = wave::chunk_ends(&trace, out.len());
        let mut st = 0i16;
        wave::formats::emphasis(&mut out, self.chan.emphasis, &mut st);
        Ok((out, ends))
    }

    /// `synth_from_wa_traced` without the chunk boundaries.
    pub fn synth_from_wa(&self, sentences: &[SentenceInput]) -> io::Result<Vec<i16>> {
        self.synth_from_wa_traced(sentences).map(|(pcm, _)| pcm)
    }

    /// Raw text -> PCM + chunk boundaries: the text front end
    /// (`text_to_sentences`) then `synth_from_wa_traced`. Only as exact as
    /// `crate::text` is.
    pub fn speak_traced(&self, text: &[u8]) -> io::Result<(Vec<i16>, Vec<usize>)> {
        let sentences = self.text_to_sentences(text);
        self.synth_from_wa_traced(&sentences)
    }

    /// `speak_traced` without the chunk boundaries.
    pub fn speak(&self, text: &[u8]) -> io::Result<Vec<i16>> {
        self.speak_traced(text).map(|(pcm, _)| pcm)
    }
}

/// Parses `out_feling`/`out_engine`-shaped capture JSON (`wa_in` +
/// `words_out` events grouped per sentence, as captured by the original
/// engine's `dumpSS` trace) into `SentenceInput`s: a captured-WA entry point
/// for callers/tests that can't or don't want to go through the (still WIP)
/// text front end -- `tests/engine_parity.rs` and `vtpaul`'s `synth-wa` CLI
/// subcommand both use this, instead of each re-parsing the capture schema
/// themselves.
///
/// The captured per-word pitch/speed/volume/kind7 arrays are used as-is
/// (real VTML-resolved rates: `fedump.exe` mode 7 does run prosody's span
/// application). `tib.pause[0]` is passed through as captured, *not*
/// filtered here -- `resolve_word0_pause`'s doc explains why it needs to see
/// the raw captured value to tell a genuine VTML override apart from a
/// `fedump`-only artifact.
pub mod capture {
    use super::{SentenceInput, Wa};
    use crate::json::Json;
    use crate::ling::Tib;
    use std::collections::BTreeMap;

    fn unhex(s: &str) -> Vec<u8> {
        s.as_bytes()
            .chunks(2)
            .map(|c| {
                let h = |x: u8| (x as char).to_digit(16).unwrap_or(0) as u8;
                h(c[0]) << 4 | h(c[1])
            })
            .collect()
    }

    fn load_wa(e: &Json) -> Wa {
        let mut wa = Wa::new();
        let head = unhex(e.get("head").str().unwrap_or(""));
        wa.b[..head.len()].copy_from_slice(&head);
        for (i, r) in e.get("recs").arr().iter().enumerate() {
            let b = unhex(r.str().unwrap_or(""));
            let o = Wa::rec(i);
            wa.b[o..o + b.len()].copy_from_slice(&b);
        }
        wa
    }

    /// Events grouped by their enclosing sentence (a new group starts at
    /// every `wa_in`), each a map from event kind to the event.
    fn group_sentences(doc: &Json) -> Vec<BTreeMap<String, Json>> {
        let mut out: Vec<BTreeMap<String, Json>> = Vec::new();
        for e in doc.get("events").arr() {
            let k = e.get("kind").str().unwrap_or("").to_string();
            if k == "wa_in" {
                out.push(BTreeMap::new());
            }
            if let Some(cur) = out.last_mut() {
                cur.insert(k, e.clone());
            }
        }
        out
    }

    fn tib_array(words: &Json, name: &str) -> Vec<i32> {
        words.get("tib").get(name).arr().iter().map(|v| v.i64().unwrap_or(-1) as i32).collect()
    }

    /// One sentence's `SentenceInput` from its `wa_in`/`words_out` events
    /// (the state right after `fe_BuildWordPhoneRecords`, before the breaks
    /// stage mutates the pause/break-level arrays further -- `run_ling`
    /// redoes that mutation itself via `assign_phrase_breaks`).
    pub fn sentence_input(wa_in: &Json, words: &Json) -> SentenceInput {
        let wa = load_wa(wa_in);
        let n = wa.n();
        let mut tib = Tib::new();
        let pause = tib_array(words, "pause");
        let brk_level = tib_array(words, "brk_level");
        for i in 0..200.min(pause.len()) {
            tib.pause[i] = pause[i];
        }
        for i in 0..200.min(brk_level.len()) {
            tib.brk_level[i] = brk_level[i];
        }
        let pad = |v: Vec<i32>, fill: i32| -> Vec<i32> {
            let mut o = v[..n.min(v.len())].to_vec();
            o.resize(n, fill);
            o
        };
        SentenceInput {
            wa,
            tib,
            kind7: pad(tib_array(words, "kind7"), -1),
            pitch: pad(tib_array(words, "pitch"), 100),
            speed: pad(tib_array(words, "speed"), 100),
            volume: pad(tib_array(words, "volume"), 100),
            map_start: Vec::new(),
            map_end: Vec::new(),
            final_pause: -1,
        }
    }

    /// Every sentence of one capture document that has both a `wa_in` and a
    /// `words_out` event (the normal case; a sentence left with neither, e.g.
    /// a capture-side failure, is skipped).
    pub fn sentences(doc: &Json) -> Vec<SentenceInput> {
        group_sentences(doc)
            .iter()
            .filter_map(|s| {
                let (wa_in, words) = (s.get("wa_in")?, s.get("words_out")?);
                Some(sentence_input(wa_in, words))
            })
            .collect()
    }
}
