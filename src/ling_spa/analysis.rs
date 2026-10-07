//! Violeta's lexicon lookup + TPP tagging pass: `FUN_10021bf0` (lexicon
//! lookup, Spanish plural/gender 's'-suffix stripping, letter-to-sound
//! dispatch) then `FUN_100252c0`/`FUN_100254b0`/`FUN_10025410` (TPP-based
//! POS tagging), called in sequence by `FUN_100256d0`:
//!
//! ```text
//! int FUN_100256d0(short *wa) {
//!     if (FUN_10021bf0(wa) < 0) return -1;
//!     return FUN_100252c0(wa) >= 0 ? 1 : -1;
//! }
//! ```
//!
//! which is itself the whole body of the `fe_LinguisticAnalysis` wrapper
//! (`0x10031700`). Unlike English (`ling::analysis`/`ling::morph`, five
//! separate sub-stages over a `Ws` candidate/morph array), Violeta does
//! lexicon lookup, morphology and POS tagging directly on the `WaSpa`
//! record array -- there is no separate candidate/morph array here, so
//! this module reads and writes `WaSpa` records in place, and there is no
//! `Ws`-equivalent to port.

use super::lexicon::{
    all_upper_spa, decode_payload_spa, encode_key_spa, mexican_ctx_spa, parse_tpp_num, spell_spa, stricmp_spa, strnicmp_spa, vowel_count_spa, w_foreign_ctx_spa, LexiconSpa,
};
use super::lts;
use super::WaSpa;

/// WA record fields this module reads/writes, beyond `ling_spa::war`
/// (`START`/`END`/`LEN`/`TEXT`). Derived from `fe_EmitWord` (`0x10035350`)
/// and `FUN_10021bf0`/`FUN_100252c0`/`FUN_10025410`'s own pointer
/// arithmetic -- every offset below was cross-checked against at least two
/// independent call sites (see this agent's final report for the byte-math
/// derivation, which needed a header-size correction partway through: a
/// direct `WA_base + i*0xa8 + K` computation is record-relative `K - 0xc`,
/// not `K`, since `i*0xa8` alone excludes the `0xc`-byte header `fe_EmitWord`
/// itself adds once via `+6` shorts).
pub mod war_spa {
    /// Morphology-chain outcome flags (`FUN_10021bf0`'s `pbVar1`, a single
    /// byte OR-target): 0x01 pron resolved (lexicon hit, 's'-strip retry
    /// hit, or already-preset `PHON`), 0x02 the LTS path was taken, 0x04 an
    /// extra flag from inside the stubbed LTS pipeline (`FUN_100250e0`,
    /// not reproduced -- see `LtsSpa`), 0x08 spelled, 0x10 "already had
    /// preset phones, from a class-7 `SRC` source".
    pub const FLAGS: usize = 0x10;
    /// Emission-time source marker (`fe_EmitWord`'s `param_5`; always 'A'
    /// for the plain "word" token handler, but `FUN_100254b0` tests it
    /// against both 'S' and 'A', so other Violeta token normalizers must
    /// emit different values here -- not enumerated by this agent).
    pub const ORIGIN: usize = 0x16;
    /// Dual-purpose byte: an input case-mode preset on entry
    /// (`fe_EmitWord`'s `param_6`, e.g. 'S'/'D'/0x55 from the word
    /// handler), which `FUN_10021bf0` overwrites with the decoded lexicon
    /// entry's class char ('A' bit 0x08, 'E' bit 0x04 takes precedence)
    /// once a payload is found. 'S' on entry skips lexicon lookup
    /// entirely (spelled directly); 'E' (from either source) always ends
    /// in spelling, discarding any pron the payload also carried.
    pub const CASE_CLASS: usize = 0x17;
    /// TPP-derived POS class byte, written by `FUN_10025410` (English's
    /// `war::TAG_A` counterpart, at a different offset: `0x18` vs `0x25`).
    pub const TAG_A: usize = 0x18;
    /// `wab.txt2` index + 1 (English's `war::TAG_B` counterpart).
    pub const TAG_B: usize = 0x19;
    /// u16, set to 1 by `FUN_10025410`'s multi-word-merge loop for every
    /// word in range except the last (English's WA `+0x20` "already
    /// tagged" marker). Only ever written by the multi-word TPP-phrase
    /// path, which this port stubs out (see `join_words_spa`), so this
    /// port never sets it -- kept here for the eventual real
    /// implementation.
    pub const TPP_MERGED: usize = 0x14;
    /// Source/token-class marker (`fe_EmitWord`'s `param_7 & 0xff`; stored
    /// as a 4-byte field but only its low byte is ever meaningful here).
    /// `FUN_10021bf0` gates the "preset phones" flag choice on `== 7`;
    /// `FUN_100252c0` gates whether TPP tagging runs at all on `== 0 || 1`.
    pub const SRC: usize = 0x1c;
    /// Phone-code output string: the field `FUN_10021bf0` checks for
    /// "already resolved" (first byte nonzero) on entry and ultimately
    /// fills for every word that needed resolving.
    pub const PHON: usize = 0x42;
}

fn or_flag(wa: &mut WaSpa, r: usize, bit: u8) {
    let f = wa.u8(r + war_spa::FLAGS);
    wa.set_u8(r + war_spa::FLAGS, f | bit);
}

/// `FUN_10025f60` plus the flags-OR that follows every one of its call
/// sites in `FUN_10021bf0`.
fn spell_word(wa: &mut WaSpa, r: usize, text: &[u8]) {
    let s = spell_spa(text);
    wa.strcpy(r + war_spa::PHON, &s);
    or_flag(wa, r, 8);
}

fn apply_entry(wa: &mut WaSpa, r: usize, text: &[u8], payload: &[u8]) {
    let e = decode_payload_spa(payload);
    if e.class_char != 0 && all_upper_spa(text) {
        wa.set_u8(r + war_spa::CASE_CLASS, e.class_char);
    }
    if !e.pron.is_empty() {
        wa.strcpy(r + war_spa::PHON, &e.pron);
        or_flag(wa, r, 1);
    }
}

fn append(wa: &mut WaSpa, r: usize, s: &[u8]) {
    let o = r + war_spa::PHON + wa.cstr(r + war_spa::PHON).len();
    wa.strcpy(o, s);
}

/// `FUN_10022810` + `FUN_10023930` on one word; `None` aborts the pass.
fn lts_word(word: &[u8], len: i32, mexican: bool, w_foreign: bool, era09: bool) -> Option<Vec<u8>> {
    let mut w = lts::stage1(word, len, w_foreign, era09)?;
    lts::stage2(&mut w, mexican, w_foreign, era09)?;
    Some(w.phones())
}

/// `FUN_10021bf0`: lexicon lookup (with the plural 's' retry), letter-to-sound
/// for words without a pron, syllabification of lexicon prons. On an LTS or
/// name-lookup failure the engine truncates the WA word count to the failing
/// word and returns -1: here `false`.
pub fn lexicon_pass(wa: &mut WaSpa, lex: &LexiconSpa) -> bool {
    let mut i = 0usize;
    while i < wa.n() {
        let r = WaSpa::rec(i);
        if !lexicon_word(wa, lex, r) {
            wa.set_i16(0, i as i16);
            return false;
        }
        i += 1;
    }
    true
}

fn lexicon_word(wa: &mut WaSpa, lex: &LexiconSpa, r: usize) -> bool {
    let era09 = lex.era.is_2009();
    if wa.u8(r + war_spa::PHON) != 0 {
        or_flag(wa, r, if wa.i32(r + war_spa::SRC) == 7 { 0x10 } else { 1 });
        return true;
    }
    let text = wa.cstr(r + super::war::TEXT).to_vec();
    let len = wa.i32(r + super::war::LEN);
    let t0 = text.first().copied().unwrap_or(0);
    if wa.u8(r + war_spa::CASE_CLASS) == b'S' || !(vowel_count_spa(&text) != 0 || (len == 1 && matches!(t0, b'y' | b'Y'))) {
        spell_word(wa, r, &text);
        return true;
    }

    let mut key = encode_key_spa(&text);
    let mut klen = len;
    let mut plural = 0u8;
    match lex.lookup_encoded(&key) {
        Some(p) => {
            let p = p.to_vec();
            apply_entry(wa, r, &text, &p);
        }
        None => {
            let at = |k: i32| if k < 0 { 0 } else { key.get(k as usize).copied().unwrap_or(0) };
            let prev = at(len - 2);
            if !era09
                && len > 2
                && vowel_count_spa(&key) > 1
                && at(len - 1) == b's'
                && matches!(prev, b'b'..=b'd' | b'f'..=b'h' | b'j'..=b'n' | b'p'..=b't' | b'v'..=b'z' | 0xd1 | 0xf1)
            {
                key.truncate((len - 1) as usize);
                plural = if prev == b's' { 2 } else { 1 };
                klen = len - 1;
                if let Some(p) = lex.lookup_encoded(&key) {
                    let p = p.to_vec();
                    apply_entry(wa, r, &text, &p);
                }
            }
        }
    }
    if wa.u8(r + war_spa::CASE_CLASS) == b'E' {
        spell_word(wa, r, &text);
        return true;
    }

    if wa.u8(r + war_spa::PHON) == 0 {
        let mexican = mexican_ctx_spa(&text, era09);
        let w_foreign = !mexican && w_foreign_ctx_spa(&text);
        let tail = if len >= 5 { text.get((len - 5) as usize..).unwrap_or(&[]) } else { &[][..] };
        let mente = !(len < 6 || lts::bsearch_ci(&lts::NOT_MENTE, &text) != -1 || strnicmp_spa(tail, b"mente", 5) != 0);
        let mut stem: Vec<u8> = text.iter().copied().take((len - 5).max(0) as usize).collect();
        stem.resize((len - 5).max(0) as usize, 0);
        let stem = stem.split(|&c| c == 0).next().unwrap_or(&[]).to_vec();
        if mente && vowel_count_spa(&stem) >= 1 {
            let Some(mut ph) = lts_word(&stem, len - 5, mexican, w_foreign, era09) else { return false };
            if lts::degeminate(&mut ph) {
                or_flag(wa, r, 4);
            }
            wa.strcpy(r + war_spa::PHON, &ph);
            let Some(ph2) = lts_word(b"mente", 5, mexican, w_foreign, era09) else { return false };
            let mut s = vec![lts::SYL];
            s.extend_from_slice(&ph2);
            append(wa, r, &s);
            if plural == 1 {
                append(wa, r, &[0x29]);
            }
        } else {
            // With a vowelless stem the engine falls through with the stem's
            // length still in its length variable.
            let n = if era09 {
                len
            } else if mente {
                len - 5
            } else {
                klen
            };
            let Some(mut ph) = lts_word(&key, n, mexican, w_foreign, era09) else { return false };
            if lts::degeminate(&mut ph) {
                or_flag(wa, r, 4);
            }
            wa.strcpy(r + war_spa::PHON, &ph);
        }
        or_flag(wa, r, 2);
        if plural == 1 {
            append(wa, r, &[0x29]);
        }
        return true;
    }

    // viol09 `FUN_1001f3d0` uses a lexicon pron as stored.
    if era09 {
        return true;
    }
    let raw: Vec<u8> = wa.cstr(r + war_spa::PHON).iter().copied().filter(|&b| b != lts::SYL).collect();
    let Some(ph) = lts::syllabify_pron(&raw) else { return false };
    wa.strcpy(r + war_spa::PHON, &ph);
    if plural == 1 {
        append(wa, r, &[0x29]);
    }
    true
}

/// `FUN_10025410`: applies a TPP-derived class byte to word `start`
/// (single-word case, `end == start`, the loop below runs zero times) or
/// to every word in `[start, end]` inclusive (multi-word merge case,
/// `end > start`) -- `TAG_A` set to `class` on every word in range,
/// `TPP_MERGED` set to 1 on every word except the last, and `TAG_B` set to
/// `wab.find(text) + 1` per word (using THAT word's own text) whenever
/// found, on every word including the last.
fn apply_tpp_class(wa: &mut WaSpa, lex: &LexiconSpa, class: u8, start: usize, end: usize) {
    for k in start..end {
        let r = WaSpa::rec(k);
        wa.set_i16(r + war_spa::TPP_MERGED, 1);
        wa.set_u8(r + war_spa::TAG_A, class);
        let text = wa.cstr(r + super::war::TEXT).to_vec();
        let idx = lex.wab_find(&text);
        if idx >= 0 {
            wa.set_u8(r + war_spa::TAG_B, (idx + 1) as u8);
        }
    }
    let r = WaSpa::rec(end);
    wa.set_u8(r + war_spa::TAG_A, class);
    let text = wa.cstr(r + super::war::TEXT).to_vec();
    let idx = lex.wab_find(&text);
    if idx >= 0 {
        wa.set_u8(r + war_spa::TAG_B, (idx + 1) as u8);
    }
}

/// `FUN_100254b0`: multi-word TPP phrase join. Appends following words to
/// word `i`'s text with '-' while they are adjacent 'A'-origin tokens (a
/// "guión" 'S' token is stepped over without counting), up to 7 words and 29
/// characters, and keeps the longest join with a TPP 'A' field. Returns that
/// field's class and the last joined word.
fn join_words_spa(wa: &WaSpa, lex: &LexiconSpa, i: usize, n: usize) -> Option<(u8, usize)> {
    let r0 = WaSpa::rec(i);
    let mut phrase = wa.cstr(r0 + super::war::TEXT).to_vec();
    let mut total = wa.i32(r0 + super::war::LEN);
    let mut count = 1;
    let mut best: Option<(u8, usize)> = None;
    let mut j = i + 1;
    loop {
        if j >= n {
            return best;
        }
        let r = WaSpa::rec(j);
        total += 1 + wa.i32(r + super::war::LEN);
        if total > 0x1d {
            return best;
        }
        let start = wa.i32(r + super::war::START);
        let gap = start - wa.i32(WaSpa::rec(j - 1) + super::war::END);
        let origin = wa.u8(r + war_spa::ORIGIN);
        let text = wa.cstr(r + super::war::TEXT);
        let hyphen = count > 0 && origin == b'S' && start - wa.i32(r + super::war::END) == 1 && stricmp_spa(text, b"gui\xf3n") == 0 && gap < 2;
        if !hyphen {
            if origin != b'A' || gap >= 2 {
                return best;
            }
            phrase.push(b'-');
            phrase.extend_from_slice(text);
            if let Some(f) = lex.tpp_field(&phrase, b'A') {
                best = Some((parse_tpp_num(&f), j));
            }
            count += 1;
        }
        j += 1;
        if count > 7 {
            return best;
        }
    }
}

/// `FUN_100252c0`: TPP-based POS tagging over every word whose `SRC`
/// field is 0 or 1. `join_words_spa` (stubbed, see above) is tried first;
/// on failure (always, currently), the single-word path looks up TPP
/// field 'B' for the word's own text, falling back to the 48-word
/// `haber`-conjugation list (`LexiconSpa::haber_find`, class 0x3d) when
/// that fails too, and always finishing with a `wab.txt2` lookup for
/// `TAG_B` regardless of which branch fired.
pub fn tpp_pass(wa: &mut WaSpa, lex: &LexiconSpa) {
    let n = wa.n();
    let mut i = 0usize;
    while i < n {
        let r = WaSpa::rec(i);
        let src = wa.i32(r + war_spa::SRC);
        if src == 0 || src == 1 {
            if let Some((class, end)) = join_words_spa(wa, lex, i, n) {
                apply_tpp_class(wa, lex, class, i, end);
                i = end;
            } else {
                let text = wa.cstr(r + super::war::TEXT).to_vec();
                match lex.tpp_field(&text, b'B') {
                    Some(f) => apply_tpp_class(wa, lex, parse_tpp_num(&f), i, i),
                    None => {
                        if lex.haber_find(&text) >= 0 {
                            wa.set_u8(r + war_spa::TAG_A, 0x3d);
                        }
                        let idx = lex.wab_find(&text);
                        if idx >= 0 {
                            wa.set_u8(r + war_spa::TAG_B, (idx + 1) as u8);
                        }
                    }
                }
            }
        }
        i += 1;
    }
}

/// `FUN_100256d0`: `lexicon_pass` then `tpp_pass` (this is the whole body
/// of the `fe_LinguisticAnalysis` wrapper, `0x10031700`, modulo its
/// -1/1 return-code mapping).
pub fn analyze(wa: &mut WaSpa, lex: &LexiconSpa) -> bool {
    if !lexicon_pass(wa, lex) {
        return false;
    }
    tpp_pass(wa, lex);
    true
}
