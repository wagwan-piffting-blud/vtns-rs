//! Prosody for the Violeta (Spanish) build: duration prediction, pitch prediction,
//! and the break-code constants that differ from vt_eng.dll. VTML span application,
//! sentence-pause resolution, pause-gap samples, silence length, duration
//! quantization and the unit-selection target feature are algorithmically identical
//! to English (confirmed by decompiling `vt_spa_violeta16.dll`'s counterparts of
//! pro_ApplyTagSpansToWords 0x1000d840, pro_ResolveSentencePause 0x10011710,
//! pro_PauseSamples 0x10019c10 and the head of us_GetCandidates 0x1000e290 against
//! `vt_eng.dll`'s: only struct offsets differ, already abstracted away by
//! `Sentence`/`Word`/`Syllable`/`SpanList`/`WordRates`) and are reused unchanged from
//! `prosody.rs`, as is `smooth_f0` (pro_SmoothF0AcrossSyllables FUN_1001ed90 is the
//! same algorithm at a different syllable stride). What differs and lives here:
//! duration/pitch feature vectors (different layout and phone tables) and the
//! duration tree set (8 trees instead of English's 9, one direct class table
//! instead of isVowel+vclass+cclass).

use std::io;
use std::path::Path;

use crate::prosody::{Sentence, WordRates};
use crate::prosody_tables_spa::*;
use crate::tree3::Tree;

/// Break codes (word record +0x53c), shifted -0x14 from the English bytes
/// (`Z [ \ ] ^` -> `F G H I J`; confirmed by decompiling fe_AssignPhraseBreaks
/// 0x1001e750, pro_PredictPitch 0x1001f040 and PredictDurations' break checks).
pub const BRK_NONE_SPA: u8 = b'I';
pub const BRK_MINOR_SPA: u8 = b'H';
pub const BRK_PHRASE_SPA: u8 = b'G';
pub const BRK_SENTENCE_SPA: u8 = b'F';
pub const BRK_QUESTION_SPA: u8 = b'J';

/// Sentinel phone indices at word edges (duration features only; pitch features use
/// no such sentinel). English uses 40/42 (just past its 39 valid indices); Violeta's
/// `PHONE_INDEX_SPA` reaches higher (up to 51), so the sentinels move to 50/51.
const EDGE_PAUSE_SPA: i16 = 50;
const EDGE_JUNCTURE_SPA: i16 = 51;

/// Base phone index of a phone code (`(char)DAT_10067114[(char)code]`).
pub fn phone_index_spa(code: u8) -> i8 {
    PHONE_INDEX_SPA[(code & 0x7f) as usize]
}

/// Violeta has no 4th "grave" break level in the prosody checks (English's
/// `` ` `` is never tested in the Violeta decompiles): PHRASE/SENTENCE/QUESTION only.
fn is_final_break_spa(b: u8) -> bool {
    matches!(b, BRK_PHRASE_SPA | BRK_SENTENCE_SPA | BRK_QUESTION_SPA)
}

/// Previous word ended in a phrase break, or this is the sentence's first word.
fn starts_after_pause_spa(s: &Sentence, wi: usize) -> bool {
    wi == 0 || s.words[wi - 1].brk == BRK_PHRASE_SPA
}

/// The 8 duration trees + 8 pitch trees (speaker struct, uniformly -4 from English's
/// slot offsets; confirmed by decompiling the Violeta loader FUN_10001050 against
/// vt_eng.dll's pro_LoadProsodyTrees). Violeta has no `vshort`-slot equivalent: only
/// 3 "vowel" duration trees (`vmono`, `vdif`, `vdib`) load into the slots English's
/// `vlong`/`vdi`/`vsch` occupy; the 5 consonant trees keep English's names.
pub struct TreesSpa {
    pub vmono: Tree,
    pub vdif: Tree,
    pub vdib: Tree,
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

impl TreesSpa {
    /// `tree3_dir` is `.../M16/ttsdata/tree3`.
    pub fn load(tree3_dir: &Path) -> io::Result<TreesSpa> {
        let d = |n: &str| Tree::load(&tree3_dir.join("duration").join(format!("{}.tree3", n)));
        let p = |n: &str| Tree::load(&tree3_dir.join("pitch").join(format!("{}.tree3", n)));
        Ok(TreesSpa {
            vmono: d("vmono")?,
            vdif: d("vdif")?,
            vdib: d("vdib")?,
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

/// pro_DurationFeatures counterpart (FUN_1001e970, called from FUN_1001eb80):
/// an 11-short vector, not English's 9-short one. Violeta stores phone codes and
/// pos-codes per-syllable via pointers (syl+0x1c / syl+0x20) into the same flat
/// per-word arrays (word+0x408 / word+0x4d0) `Word.codes`/`Word.pos_code` already
/// hold; `ctx` is the word's per-phone 7-byte context array (word+0xdc, stride 7 --
/// same layout fe_BuildPhoneContexts writes in English, captured directly from
/// `phones[].ctx` rather than reimplemented, since that stage belongs to the front
/// end). fv[4]/fv[5] read ctx byte 5's nibbles (clamped to 3) -- a genuinely new
/// pair of features with no English counterpart.
pub fn duration_features_spa(s: &Sentence, wi: usize, si: usize, p: usize, ctx: &[[u8; 7]]) -> [i16; 11] {
    let w = &s.words[wi];
    let syl = &w.syllables[si];
    let k = syl.first_phone as usize + p;
    let nsyl = w.syllables.len();
    let last_in_word = si + 1 == nsyl && p + 1 == syl.n_phones as usize;
    let mut fv = [0i16; 11];
    fv[0] = phone_index_spa(w.codes[k]) as i16;
    fv[1] = if si == 0 && p == 0 {
        if starts_after_pause_spa(s, wi) { EDGE_PAUSE_SPA } else { EDGE_JUNCTURE_SPA }
    } else {
        phone_index_spa(w.codes[k - 1]) as u8 as i16
    };
    fv[2] = if last_in_word {
        if is_final_break_spa(w.brk) { EDGE_PAUSE_SPA } else { EDGE_JUNCTURE_SPA }
    } else {
        phone_index_spa(w.codes[k + 1]) as u8 as i16
    };
    fv[3] = PHONE_STRESS_SPA[(w.codes[k] & 0x7f) as usize] as i16;
    let c5 = ctx[k][5];
    fv[4] = ((c5 >> 3) & 7).min(3) as i16;
    fv[5] = (c5 & 7).min(3) as i16;
    fv[6] = syl.b1 as i16;
    fv[7] = syl.b2 as i16;
    fv[8] = if si == 0 && starts_after_pause_spa(s, wi) {
        1
    } else if si + 1 == nsyl && is_final_break_spa(w.brk) {
        3
    } else {
        2
    };
    fv[9] = nsyl as i16;
    fv[10] = w.pos_code[k] as i16;
    fv
}

/// pro_PredictDurations counterpart (FUN_1001eb80). `ctx[wi]` is word `wi`'s
/// per-phone context array, same order as `Word.codes`.
pub fn predict_durations_spa(trees: &TreesSpa, s: &mut Sentence, ctx: &[Vec<[u8; 7]>]) {
    for wi in 0..s.words.len() {
        let mut out = Vec::with_capacity(s.words[wi].codes.len());
        for si in 0..s.words[wi].syllables.len() {
            for p in 0..s.words[wi].syllables[si].n_phones as usize {
                let fv = duration_features_spa(s, wi, si, p, &ctx[wi]);
                let k = s.words[wi].syllables[si].first_phone as usize + p;
                let code = s.words[wi].codes[k];
                let class = PHONE_CLASS8_SPA[(code & 0x7f) as usize];
                let tree = match class {
                    0 => Some(&trees.vmono),
                    1 => Some(&trees.vdif),
                    2 => Some(&trees.vdib),
                    3 => Some(&trees.cstop),
                    4 => Some(&trees.cfri),
                    5 => Some(&trees.caff),
                    6 => Some(&trees.cnas),
                    7 => Some(&trees.capp),
                    _ => None,
                };
                // The engine leaves the destination untouched (reuses whatever the
                // CPU register held) for a class outside 0-7; never observed for a
                // real word-internal phone (only code 0 maps outside 0-7, and 0
                // never appears inside `Word.codes`), so this never fires in
                // practice. 500 mirrors English's unclassified-consonant default.
                out.push(match tree {
                    Some(t) => t.predict_scalar(&fv),
                    None => 500,
                });
            }
        }
        let w = &mut s.words[wi];
        if w.dur.len() < w.codes.len() {
            w.dur.resize(w.codes.len(), 0);
        }
        for (k, d) in out.into_iter().enumerate() {
            w.dur[k] = d;
        }
    }
}

/// pro_PitchFeatures counterpart (FUN_1001ee90): the same 12-element layout as
/// English's `pitch_features` (index for index), except fv[0] reads the shared
/// duration/pitch class table instead of a consonant-class phone property, and the
/// nucleus/phone-index lookups use `PHONE_INDEX_SPA`.
pub fn pitch_features_spa(s: &Sentence, wi: usize, si: usize) -> [i16; 12] {
    let w = &s.words[wi];
    let syl = &w.syllables[si];
    let nsyl = w.syllables.len();
    let first = syl.first_phone as usize;
    let mut fv = [0i16; 12];
    fv[0] = PHONE_CLASS8_SPA[(w.codes[first] & 0x7f) as usize] as i8 as i16;
    let n = syl.n_phones as usize;
    let mut nuc = (0..n).find(|&j| w.pos_code[first + j] == 2).unwrap_or(n);
    if nuc == n {
        nuc = n.wrapping_sub(1);
    }
    fv[1] = phone_index_spa(w.codes[first + nuc]) as u8 as i16;
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
    fv[8] = if si == 0 && starts_after_pause_spa(s, wi) {
        1
    } else if is_final_break_spa(w.brk) && si + 1 == nsyl {
        3
    } else {
        2
    };
    fv[9] = syl.b1 as i16;
    fv[10] = nsyl as i16;
    fv
}

/// pro_PredictPitch counterpart (0x1001f040) without the final smoothing pass.
pub fn predict_pitch_raw_spa(trees: &TreesSpa, s: &mut Sentence) {
    for wi in 0..s.words.len() {
        for si in 0..s.words[wi].syllables.len() {
            let mut fv = pitch_features_spa(s, wi, si);
            let last = si + 1 == s.words[wi].syllables.len();
            let (t, f) = match s.words[wi].brk {
                BRK_QUESTION_SPA if last => (&trees.qbt, &trees.qbf),
                BRK_SENTENCE_SPA if last => (&trees.sbt, &trees.sbf),
                BRK_PHRASE_SPA if last => (&trees.bt, &trees.bf),
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

/// Full pro_PredictPitch: tone + contour per syllable, then smoothing. The
/// smoothing pass (pro_SmoothF0AcrossSyllables FUN_1001ed90) is the same algorithm
/// as English at a different syllable stride, already generic in `Sentence`
/// terms, so `prosody::smooth_f0` is reused unchanged.
pub fn predict_pitch_spa(trees: &TreesSpa, s: &mut Sentence) {
    predict_pitch_raw_spa(trees, s);
    crate::prosody::smooth_f0(s);
}

/// fe_NextSentence counterpart's per-word rate seeding (FUN_1000dce0): pitch/speed/
/// volume seed to the sentence bases and pause/brk seed to -1, same as English, but
/// k7 seeds to 0, not -1 (confirmed against `out_prosody_spa`'s `spans.before.k7`,
/// e.g. `nws_BRO_CWF_0_000.json`: every word's k7 reads 0 with no VTML k7 tag in
/// force, where English's `WordRates::seed` would leave -1). `hooks_prosody_spa.js`
/// reads the k7 array from tib+0x15d730 (its own comment/`ARRAYS` table), 0x40
/// bytes before where the pitch/speed/volume/pause/brk uniform 0x320 stride would
/// put it (tib+0x15d770, confirmed by decompiling FUN_1000dce0's seeding loop) --
/// k7 is a narrower buffer than the other five 200-word arrays, so captures only
/// show a reliable 0 for a word's k7 within roughly the first 16 words of a
/// sentence; beyond that the capture reads past its real extent (seen as -1 in
/// `out_prosody_spa`). Harmless for every capture here: `apply_tag_spans` only
/// reads/writes k7 when VTML kind 7 has span data, which never happens in this
/// corpus (open item; see the final report).
pub fn seed_word_rates_spa(n: usize, pitch: i32, speed: i32, volume: i32) -> WordRates {
    WordRates {
        pitch: vec![pitch; n],
        speed: vec![speed; n],
        volume: vec![volume; n],
        pause: vec![-1; n],
        brk: vec![-1; n],
        k7: vec![0; n],
    }
}

/// pro_PauseSamples counterpart (FUN_10019c10): identical arithmetic to English's
/// `prosody::pause_samples`, but that function's `match prev_brk` hardcodes the
/// English break bytes (`Z`/`[`/`^`), so it can never match Violeta's `F`/`G`/`J`
/// and always falls through to the `_ => 0` arm. This copy swaps in the Spanish
/// break constants; everything else -- including the w==0 continuation-flag
/// handling and the `<< 4` samples-per-ms scale -- is unchanged.
pub fn pause_samples_spa(pause: &[i32], w: usize, prev_brk: u8, cont: &mut bool, comma_ms: Option<i32>) -> i32 {
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
        BRK_SENTENCE_SPA | BRK_QUESTION_SPA => 11000,
        BRK_PHRASE_SPA => comma_ms.map_or(3200, |ms| ms << 4),
        _ => 0,
    }
}

// `apply_tag_spans`, `resolve_sentence_pause`, `silence_length`, `quantize_duration`
// and `target_feature` are reused unchanged from `prosody`: decompiling
// pro_ApplyTagSpansToWords (0x1000d840), pro_ResolveSentencePause (0x10011710) and
// the target-feature head of us_GetCandidates (0x1000e290) against their
// vt_eng.dll counterparts showed the same algorithm at different struct offsets in
// every case (SPAN_MAX/SPAN_MIN, DUR_DIV/DUR_ROUND and the target-feature formula
// are byte-identical constants, and neither function branches on a break-code
// literal the way pro_PauseSamples does), so no Spanish override of those
// functions exists in this file; callers use `crate::prosody::{apply_tag_spans,
// resolve_sentence_pause, silence_length, quantize_duration, target_feature,
// Sentence, Word, Syllable, SpanList, SpanContext, TargetRef, WordRates}` directly
// alongside this module.
