//! `fe_LinguisticAnalysis` (0x1003e240): merges the pronounced/tagged `Ws`
//! morph array back into the `Wa` word array.
//!
//! The real function's *first* step is a call to `fe_TagAndPronounce`, which
//! is lookup + copy + morphology + the wh-flag pass + function-word marking
//! all in one (`FUN_1000d190`, `dict_CopyMorphsToWordArray`, `FUN_10007520`,
//! `FUN_1000e990`, `FUN_1000dfc0` -- see its own decompile). That means most
//! of what a naive `wa_in`/`wa_out` capture-diff attributes to "the merge" is
//! actually `mark_function_words` (`war::TAG_A`/`war::TAG_B`, already ported
//! in `funcwords.rs`) and the wh-flag copy, not this function. This module
//! is only the merge loop itself: the caller must already have run
//! `analysis::tag_and_lookup`, `morph::morphology`, the (not yet ported)
//! wh-flag pass and `funcwords::mark_function_words` on `ws`/`wa` first.
//!
//! Ported from the disassembly, then proven against every `wa_in`/`wa_out`
//! pair in the parity corpus (`out_feling`, exercised by `merge_stage` in
//! `tests/feling_parity.rs`) rather than trusted from the pointer algebra
//! alone: the compiler folds the record cursor through a genuine
//! pointer-difference-then-re-add-base idiom (`_Str = record_ptr - 0x60`,
//! confirmed by canceling the `ESI` terms in `ECX = (0x4c-ESI)+EBX+EAX`),
//! so every `_Str+X` access was resolved to its own `war::*` field and
//! checked against the captures independently, not derived symbolically.

use super::{mr, war, Wa, Ws, WH_FLAG};

/// Does the group's first morph's pron get copied into `war::PHONES`
/// (overwriting it), or does the field's existing content (a VTML preset
/// pronunciation, most commonly) survive untouched? Confirmed from the asm
/// guard on `_Str+0x83=='U'` (`war::U_FLAG`) / `_Str+0x42!='Y'`
/// (`war::CASE_MODE`) gating the copy: skip the overwrite only when the word
/// is a `CASE_MODE=='Y'` preset-phones case that isn't itself a live 'U'
/// token. Neither field is written by this function, so it's safe to read
/// them straight off `wa` regardless of write order.
fn overwrite_phones(wa: &Wa, r: usize) -> bool {
    wa.u8(r + war::U_FLAG) == b'U' || wa.u8(r + war::CASE_MODE) != b'Y'
}

/// `fe_LinguisticAnalysis`'s merge loop, one morph at a time. A morph whose
/// `mr::TOKEN` differs from the previous one starts the next WA record:
///
/// * `war::FORCED_POS` = 0xff, `war::CLASS_CHAR` and `war::FLAGS` from the
///   morph, `war::PHONES` = its pron (if `overwrite_phones`).
/// * `war::NMORPH` is incremented from whatever the record already holds,
///   clearing `war::FORCED_POS+1+old` to 0xff.
///
/// A same-token morph whose spelling starts with the VTML marker `a2 fe`
/// cuts the record's `war::TEXT` at that spelling (`strstr`) and takes 2 off
/// `war::LEN`, once per record; every later same-token morph's spelling is
/// then appended to `war::TEXT`. The marker morph itself is not merged. Other
/// same-token morphs, while `war::NMORPH < 5`, append their pron to
/// `war::PHONES`, OR their flags into `war::FLAGS` and clear
/// `war::FORCED_POS+old` and `+old+1`. No text input seen so far puts the
/// marker inside a token (the normalizers split around it), so that branch
/// is transliterated from the asm without a capture behind it.
///
/// Returns the engine's own consistency check: `wa.n()` (the WA's word
/// count going in) must equal the number of WA records the merge started.
pub fn merge(ws: &Ws, wa: &mut Wa) -> bool {
    let nm = ws.n_morphs();
    let mut started = 0usize;
    let mut r = 0usize;
    let mut marker = false;
    for i in 0..nm {
        let m = Ws::morph(i);
        if i == 0 || ws.i16(Ws::morph(i - 1) + mr::TOKEN) != ws.i16(m + mr::TOKEN) {
            r = Wa::rec(started);
            wa.set_u8(r + war::FORCED_POS, 0xff);
            if overwrite_phones(wa, r) {
                let p = ws.cstr(m + mr::PRON).to_vec();
                wa.strcpy(r + war::PHONES, &p);
            }
            let k = wa.i32(r + war::NMORPH);
            wa.set_i32(r + war::NMORPH, k + 1);
            wa.set_u8(r + war::FORCED_POS + 1 + k as usize, 0xff);
            wa.set_u8(r + war::CLASS_CHAR, ws.u8(m + mr::CLASS_CHAR));
            wa.set_i16(r + war::FLAGS, ws.i16(m + mr::FLAGS));
            marker = false;
            started += 1;
            continue;
        }
        let sp = ws.cstr(m + mr::SPELLING).to_vec();
        if sp.starts_with(&[0xa2, 0xfe]) {
            if !marker {
                let text = wa.cstr(r + war::TEXT).to_vec();
                if let Some(p) = text.windows(sp.len()).position(|w| w == sp.as_slice()) {
                    wa.set_u8(r + war::TEXT + p, 0);
                }
                marker = true;
                let l = wa.i32(r + war::LEN);
                wa.set_i32(r + war::LEN, l - 2);
            }
            continue;
        }
        if marker {
            let mut text = wa.cstr(r + war::TEXT).to_vec();
            text.extend_from_slice(&sp);
            wa.strcpy(r + war::TEXT, &text);
        }
        let k = wa.i32(r + war::NMORPH);
        if k < 5 {
            let pron = ws.cstr(m + mr::PRON);
            if !pron.is_empty() {
                let mut ph = wa.cstr(r + war::PHONES).to_vec();
                ph.extend_from_slice(pron);
                wa.strcpy(r + war::PHONES, &ph);
            }
            wa.set_u8(r + war::FORCED_POS + k as usize, 0xff);
            wa.set_i32(r + war::NMORPH, k + 1);
            wa.set_u8(r + war::FORCED_POS + 1 + k as usize, 0xff);
            let f = wa.i16(r + war::FLAGS) as u16 | ws.i16(m + mr::FLAGS) as u16;
            wa.set_i16(r + war::FLAGS, f as i16);
        }
    }
    if nm > 0 {
        wa.set_wh(ws.i16(WH_FLAG));
    }
    wa.n() as usize == started
}
