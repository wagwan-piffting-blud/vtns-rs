//! `fe_TagAndPronounce` (0x1000e2f0) and `fe_LinguisticAnalysis` (0x1003e240):
//! candidates -> morph entries (`dict_CopyMorphsToWordArray`, with the homograph
//! tagger for multi-pronunciation words), morphology / LTS, the wh-question
//! flag, function-word marking, and the merge back into the word array.

use super::funcwords::{mark_function_words, wh_flag};
use super::lookup::{cand, lookup_tokens};
use super::morph::morphology;
use super::tagger::{choose_pron, Cv};
use super::{mr, war, Wa, Ws, MORPH_COUNT};
use crate::lexicon::Lexicon;
use crate::tree3::Tree;

/// `dict_CopyMorphsToWordArray`.
pub fn copy_morphs(ws: &mut Ws, wa: &Wa, n_tokens: usize, poly: &Tree, e09: bool) {
    ws.set_i16(MORPH_COUNT, 0);
    let nc = ws.i16(0).max(0) as usize;
    for i in 0..nc {
        let c = Ws::cand(i);
        let m = Ws::morph(i);
        let token = ws.i16(c + cand::TOKEN);
        ws.set_i16(m + mr::TOKEN, token);
        let sp = ws.cstr(c + cand::SPELLING).to_vec();
        ws.strcpy(m + mr::SPELLING, &sp);
        let pron = if ws.i16(c + cand::COUNT) < 2 {
            ws.cstr(c + cand::PRON).to_vec()
        } else {
            let cv = Cv { ws: &*ws, n: nc, e09 };
            choose_pron(&cv, i, n_tokens, poly)
        };
        ws.strcpy(m + mr::PRON, &pron);
        ws.set_u8(m + mr::CLASS_CHAR, ws.u8(c + cand::CLASS_CHAR));
        let r = Wa::rec(token as usize);
        ws.set_u8(m + mr::ACCENT, if wa.u8(r + war::CASE_MODE) == b'X' { b'X' } else { b'0' });
        ws.set_i16(m + mr::B40, ws.i16(c + cand::B40));
        ws.set_i16(m + mr::B10, ws.i16(c + cand::B10));
        ws.set_i16(m + mr::B20, ws.i16(c + cand::B20));
        ws.set_i16(m + mr::B80, ws.i16(c + cand::B80));
        let class = wa.i32(r + war::TOKEN_CLASS);
        ws.set_i16(m + mr::CAT, if class == 3 { 3 } else if class == 0 { 0 } else { 1 });
        ws.set_i16(MORPH_COUNT, ws.i16(MORPH_COUNT) + 1);
    }
}

/// First half of `fe_TagAndPronounce`: lookup + copy. Returns false on overflow.
pub fn tag_and_lookup(ws: &mut Ws, wa: &Wa, n: usize, lex: &Lexicon, poly: &Tree) -> bool {
    ws.set_i16(0, 0);
    ws.set_i16(MORPH_COUNT, 0);
    if n == 0 {
        return true;
    }
    if !lookup_tokens(ws, wa, n, true, lex) {
        return false;
    }
    if ws.i16(0) > 0 {
        copy_morphs(ws, wa, n, poly, lex.era.is_2009());
    }
    true
}

/// `fe_TagAndPronounce` (0x1000e2f0) in full: `tag_and_lookup` (lookup +
/// copy), then morphology, the wh-flag pass and function-word marking, in
/// that order -- confirmed from the decompile, which calls exactly these
/// five (`FUN_1000d190`, `dict_CopyMorphsToWordArray`, `FUN_10007520`,
/// `FUN_1000e990`, `FUN_1000dfc0`). This is the function
/// `fe_LinguisticAnalysis` (`merge::merge`'s caller in the real engine)
/// calls first, before the merge loop -- so most of what a naive
/// `wa_in`/`wa_out` diff attributes to "the merge" is actually this.
pub fn tag_and_pronounce(ws: &mut Ws, wa: &mut Wa, n: usize, lex: &Lexicon, poly: &Tree) -> bool {
    if !tag_and_lookup(ws, wa, n, lex, poly) {
        return false;
    }
    morphology(ws, wa, lex);
    wh_flag(ws);
    mark_function_words(wa, n, lex);
    true
}
