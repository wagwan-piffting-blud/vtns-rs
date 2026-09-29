//! `fe_BuildWordPhoneRecords` (0x1001d300, Violeta). English counterpart:
//! `0x10016c90` (`src\ling\words.rs`). Every offset below was read directly
//! off this function's decompile (Ghidra, `vt_spa_violeta16.dll`), not
//! inferred from English by analogy -- see the field-by-field notes in the
//! batch 3 handoff.
//!
//! Confirmed differences from English:
//!   * No 'c'-mark handling: the phone-string parser only recognizes 'd'
//!     (secondary-stress boundary, writes MARKS='1'); a bare 'c' byte does
//!     nothing at all (neither counts as a phone nor writes a mark), so
//!     Spanish's MARKS array never contains '2'. This is a real simplification,
//!     not a missed case: the `else if (cVar2 != 'c')` branch in the
//!     decompile has no corresponding `else` for `'c'`.
//!   * No M_FLAG ("is this code an 'M' phone") array -- Violeta doesn't
//!     write one at all.
//!   * Valid phone-code range is signed `1..=0x3f` ('?'), not `1..=0x45`
//!     ('E') -- Violeta's own (smaller) phone set.
//!   * wr::START/END are still populated from `fe_NextSentence`'s
//!     cursor+map, not this function -- same architecture as English, see
//!     `TextPos` below (front-end batch 1 confirmed Violeta's
//!     `fe_NextSentence`, 0x1000dce0, mirrors English's structurally).
//!   * SS_TYPE (ss+0x82dda) is never written anywhere in this function's
//!     decompile -- left untouched (open item, see final report).
//!   * The per-word classification byte (`wr2::CLASSCH`) is *computed* here
//!     from the word's first text character via two lookup tables
//!     (`CH_CLASS_LO`/`CH_CLASS_HI`, dumped from `DAT_10065364`/
//!     `DAT_100680d4`), not copied from the WA record like English's
//!     CLASS_CHAR.

use super::{war, wr, SsSpa, WaSpa, SYL_SIZE};
use crate::ling::ptr_wa;

/// Word-record fields written by this function that aren't in `ling_spa::wr`
/// yet (lead owns that module; report these as additions).
pub mod wr2 {
    pub const TEXT_PTR: usize = 0x400;
    pub const PH_PTR: usize = 0x404;
    pub const DOMAIN: usize = 0x3fc;
    pub const MARKS: usize = 0x46c;
    /// Unclear field (English's analogous unnamed "X3AE"); read back by
    /// `breaks::predict_break_index` as the previous word's feature idx13.
    pub const X3AE: usize = 0x534;
    /// Read by `fe_AssignPhraseBreaks` (`psVar5[-3]` relative to BRK) to
    /// pick the initial break byte from `breaks::BREAK_OF_CLASS`.
    pub const TOKEN_CLASS: usize = 0x536;
    pub const SRC: usize = 0x538;
    /// Computed classification of the word's first text character (0/1),
    /// not copied -- see `is_special_first_char`.
    pub const CLASSCH: usize = 0x539;
    /// Unclear field, read by `breaks::predict_break_index` (feature idx10).
    pub const UNK53A: usize = 0x53a;
}

/// WA-record source offsets for the `wr2` fields above (Violeta's WA record
/// is 0xa8 bytes vs English's 0x94, so these don't line up with English's
/// `war` constants).
pub mod war2 {
    pub const UNK14: usize = 0x14;
    pub const CLASS_SRC: usize = 0x20;
    pub const SRC_SRC: usize = 0x18;
    pub const UNK19: usize = 0x19;
    /// Phone-mark string (English: `war::PHONES` = 0x52); Violeta's WA
    /// record is smaller so this sits at 0x42.
    pub const PHONES: usize = 0x42;
    /// Per-word pause override, read by `breaks::assign_phrase_breaks`.
    /// Confirmed from `fe_AssignPhraseBreaks`'s decompile: `wa_ptr + 0x18`
    /// (= record 0 + 0x0c, since `WA_HEAD` = 0xc) stepped by `0x2a` ints
    /// (= 0xa8 bytes = `WA_REC`) -- same relative offset as English's
    /// `war::PAUSE`, so this should be merged into `ling_spa::mod::war`
    /// rather than kept here (see the batch 3 final report).
    pub const PAUSE: usize = 0x0c;
}

// Dumped via Ghidra MCP (`vt_spa_violeta16.dll`, read-only memory reads).
// `CH_CLASS_LO` is `DAT_10065364` re-centered: index `i` here holds
// `DAT_10065364[i - 128]` (covers the `-128..127` signed-char window the
// engine actually indexes with). `CH_CLASS_HI` is `DAT_100680d4` read
// directly (indices 0..127 land on unrelated adjacent string data that the
// engine never indexes into, since that table is only consulted for
// `c >= 0x80`).
pub static CH_CLASS_LO: [u8; 256] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x02, 0x02, 0x02, 0x02, 0x02, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x04, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08,
    0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08,
    0x08, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
    0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x08, 0x08, 0x08, 0x08, 0x08,
    0x08, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40,
    0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x08, 0x08, 0x08, 0x08, 0x01,
];

pub static CH_CLASS_HI: [u8; 256] = [
    0x6f, 0x00, 0x70, 0x72, 0x00, 0x70, 0x73, 0x00, 0x71, 0x75, 0x00, 0x72, 0x61, 0x00, 0x72, 0x63,
    0x00, 0x72, 0x64, 0x00, 0x72, 0x65, 0x00, 0x72, 0x69, 0x00, 0x72, 0x6f, 0x00, 0x72, 0x72, 0x00,
    0x72, 0x73, 0x00, 0x72, 0x74, 0x00, 0x73, 0x61, 0x00, 0x73, 0x63, 0x00, 0x73, 0x65, 0x00, 0x73,
    0x69, 0x00, 0x73, 0x6d, 0x00, 0x73, 0x6f, 0x00, 0x73, 0x70, 0x00, 0x73, 0x73, 0x00, 0x73, 0x74,
    0x00, 0x74, 0x61, 0x00, 0x74, 0x65, 0x00, 0x74, 0x69, 0x00, 0x74, 0x6f, 0x00, 0x74, 0x72, 0x00,
    0x74, 0x75, 0x00, 0x75, 0x65, 0x00, 0x75, 0x6c, 0x00, 0x75, 0x72, 0x00, 0x75, 0x73, 0x00, 0x76,
    0x65, 0x00, 0x76, 0x69, 0x00, 0x00, 0x00, 0x00, 0xe9, 0x00, 0x00, 0xed, 0x00, 0x00, 0xf1, 0x00,
    0x00, 0xf3, 0x00, 0x00, 0xf3, 0x6e, 0x00, 0xfa, 0x00, 0x00, 0xfc, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x08, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x80, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x08, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x40, 0x80,
    0x00, 0x08, 0x08, 0x08, 0x00, 0x08, 0x00, 0x00, 0x00, 0x08, 0x08, 0x00, 0x00, 0x00, 0x08, 0x00,
    0x08, 0x00, 0x08, 0x08, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x08, 0x00, 0x08, 0x08, 0x08, 0x08,
    0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
    0x00, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x08, 0x00, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00, 0x00,
    0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x00, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40,
    0x00, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x08, 0x00, 0x40, 0x40, 0x40, 0x40, 0x40, 0x00, 0x40,
];

/// `(&DAT_10065364)[(char)c] & 0x80 == 0 && (c < 0x80 || DAT_100680d4[c] & 0x80 == 0)`
/// gives `uVar6 = 0`; anything else gives `uVar6 = 1`. `DAT_10065364` is
/// indexed with `c` reinterpreted as a *signed* char, i.e. `DAT_10065364[c]`
/// for `c < 0x80` and `DAT_10065364[c - 256]` (memory *before* the label)
/// for `c >= 0x80`; `CH_CLASS_LO` captures that whole window (`-128..127`)
/// so the lookup is just `CH_CLASS_LO[c ^ 0x80]`. `DAT_100680d4` is indexed
/// directly (unsigned) and only ever consulted for `c >= 0x80`.
fn is_special_first_char(c: u8) -> bool {
    let t1 = CH_CLASS_LO[(c ^ 0x80) as usize];
    !(t1 & 0x80 == 0 && (c < 0x80 || CH_CLASS_HI[c as usize] & 0x80 == 0))
}

/// Same role as `ling::words::TextPos`: the per-request "characters consumed
/// so far" cursor plus the optional VTML/fullwidth position-remap tables
/// `fe_InitialTextProcess` builds (`offset_maps`'s `map_b`/`map_c` in the
/// captures -- same field names as English since `hooks_feling_spa.js`
/// reuses them for the analogous TIB+0x82e64/+0x82e68 tables).
pub struct TextPos<'a> {
    pub cursor: i32,
    pub map_start: Option<&'a [i32]>,
    pub map_end: Option<&'a [i32]>,
}

impl<'a> TextPos<'a> {
    pub fn none() -> Self {
        TextPos { cursor: 0, map_start: None, map_end: None }
    }

    fn remap(map: Option<&[i32]>, pos: i32) -> i32 {
        match map {
            Some(m) => m.get(pos.max(0) as usize).copied().unwrap_or(pos),
            None => pos,
        }
    }

    fn start(&self, war_start: i32) -> i32 {
        Self::remap(self.map_start, self.cursor + war_start)
    }

    fn end(&self, war_end_exclusive: i32) -> i32 {
        Self::remap(self.map_end, self.cursor + war_end_exclusive - 1)
    }
}

/// Resolved `char*` fields of the SS words that point into the WA (mirrors
/// `ling::WordStrings`; Violeta's tag scheme is the same `ptr_ss`/`ptr_wa`
/// used throughout `ling`, reused here rather than redefined).
pub struct WordStringsSpa<'a> {
    pub wa: &'a WaSpa,
}

const TAG_MASK: u32 = 0xf000_0000;
const TAG_WA: u32 = 0x2000_0000;

impl<'a> WordStringsSpa<'a> {
    fn resolve(&self, p: u32) -> &'a [u8] {
        if p & TAG_MASK == TAG_WA {
            self.wa.cstr((p & !TAG_MASK) as usize)
        } else {
            &[]
        }
    }

    pub fn text(&self, ss: &SsSpa, i: usize) -> &'a [u8] {
        self.resolve(ss.u32(SsSpa::word(i) + wr2::TEXT_PTR))
    }

    pub fn phones(&self, ss: &SsSpa, i: usize) -> &'a [u8] {
        self.resolve(ss.u32(SsSpa::word(i) + wr2::PH_PTR))
    }
}

/// Returns false when a word has no phones (the engine returns 0 and the
/// sentence is dropped), same as English.
pub fn build_word_phone_records(ss: &mut SsSpa, wa: &WaSpa, kind7: &[i32], pos: &TextPos) -> bool {
    let nw = ss.n_words();
    for i in 0..nw {
        let r = WaSpa::rec(i);
        let w = SsSpa::word(i);
        ss.set_i32(w + wr::START, pos.start(wa.i32(r + war::START)));
        ss.set_i32(w + wr::END, pos.end(wa.i32(r + war::END)));
        ss.set_u32(w + wr2::TEXT_PTR, ptr_wa(r + war::TEXT));
        ss.set_u32(w + wr2::PH_PTR, ptr_wa(r + war2::PHONES));
        ss.set_i16(w + wr2::X3AE, wa.i16(r + war2::UNK14));
        ss.set_i16(w + wr2::TOKEN_CLASS, wa.i16(r + war2::CLASS_SRC));
        ss.set_u8(w + wr2::SRC, wa.u8(r + war2::SRC_SRC));
        ss.set_u8(w + wr2::UNK53A, wa.u8(r + war2::UNK19));

        let text0 = wa.u8(r + war::TEXT);
        ss.set_u8(w + wr2::CLASSCH, is_special_first_char(text0) as u8);

        let ph = wa.cstr(r + war2::PHONES);
        let len = ph.len();
        let mut n = 0usize;
        for k in 0..len {
            let c = ph[k];
            if c == b'd' {
                if n > 0 {
                    ss.set_u8(w + wr2::MARKS + n - 1, b'1');
                }
            } else if c != b'c' {
                if (c as i8) < 1 || c > b'?' {
                    break;
                }
                ss.set_u8(w + wr::PHONES + n, c);
                ss.set_u8(w + wr2::MARKS + n, b'0');
                n += 1;
            }
        }
        if n == 0 {
            return false;
        }
        ss.set_u8(w + wr::N_PHONES, n as u8);
    }
    for i in 0..nw {
        let v = kind7.get(i).copied().unwrap_or(-1);
        ss.set_u8(SsSpa::word(i) + wr2::DOMAIN, if v == -1 { 0 } else { 0x0c });
    }
    true
}

// --- Glue for later batches: build `prosody_spa`'s `Sentence` and
// `unitsel`'s `Word` straight from a fully-built `SsSpa` (after
// `context::build_phone_contexts`), the same structs
// `tests\prosody_spa_parity.rs`'s `sentence_from_spa` and
// `tests\unitsel_spa_parity.rs`'s `words_of` currently build from JSON
// captures. See `tests\feling_spa_words.rs`'s `glue_matches_captures` for
// the parity check against those same captures. ---

const TAG_SS: u32 = 0x1000_0000;

pub(crate) fn deref_ss(p: u32) -> usize {
    debug_assert_eq!(p & TAG_MASK, TAG_SS);
    (p & !TAG_MASK) as usize
}

/// Builds a `prosody::Sentence` from a `SsSpa` that has already been through
/// `breaks::assign_phrase_breaks` (for syllables/BRK) and, ideally,
/// `context::build_phone_contexts` (codes/pos_code are unaffected by that
/// stage, so `breaks_out`-only state works too). `dur`/`tone`/`f0` are left
/// zeroed -- they're `prosody_spa`'s own output, not this stage's.
pub fn sentence_from_ss(ss: &SsSpa) -> crate::prosody::Sentence {
    use super::syllab::syl2;
    use super::syl;
    use crate::prosody::{Sentence, Syllable, Word as ProWord};

    let mut s = Sentence::default();
    let nw = ss.n_words();
    for i in 0..nw {
        let w = SsSpa::word(i);
        let n = ss.u8(w + wr::N_PHONES) as usize;
        let codes: Vec<u8> = (0..n).map(|k| ss.u8(w + wr::PHONES + k)).collect();
        let pos_code: Vec<i8> = (0..n).map(|k| ss.u8(w + wr::POS + k) as i8).collect();
        let n_syl = ss.u8(w + wr::N_SYL) as usize;
        let syl_base = deref_ss(ss.u32(w + wr::SYL_PTR));
        let mut first_phone = 0u8;
        let syllables: Vec<Syllable> = (0..n_syl)
            .map(|g| {
                let rec = syl_base + g * SYL_SIZE;
                let np = ss.u8(rec + syl::N_PHONES);
                let mut f0 = [0i16; 12];
                for (k, v) in f0.iter_mut().enumerate() {
                    *v = ss.i16(rec + syl::F0 + 2 * k);
                }
                let y = Syllable {
                    b0: ss.u8(rec + syl2::STRESS) as i8,
                    b1: ss.u8(rec + syl2::SHAPE) as i8,
                    b2: ss.u8(rec + syl2::POSITION) as i8,
                    tone: ss.u8(rec + syl::TONE) as i8,
                    f0,
                    first_phone,
                    n_phones: np,
                };
                first_phone += np;
                y
            })
            .collect();
        let brk = ss.u8(w + wr::BRK);
        s.words.push(ProWord { codes, pos_code, syllables, brk, dur: vec![0; n] });
    }
    let np = ss.n_phrases();
    s.phrase_words = (0..np).map(|g| ss.i16(SsSpa::phrase(g) + 2).max(0) as usize).collect();
    s
}

/// Builds a `unitsel::Word` for word `i` of a `SsSpa` (post
/// `breaks::assign_phrase_breaks`, same syllable-pointer requirement as
/// `sentence_from_ss`): just the raw word/syllable bytes `Word::from_raw_lang`
/// already knows how to parse for `Lang::Spa`.
pub fn word_from_ss(ss: &SsSpa, i: usize) -> crate::unitsel::Word {
    let w = SsSpa::word(i);
    let raw = ss.b[w..w + super::WORD_SIZE].to_vec();
    let n_syl = ss.u8(w + wr::N_SYL) as usize;
    let syl_base = deref_ss(ss.u32(w + wr::SYL_PTR));
    let syls: Vec<Vec<u8>> =
        (0..n_syl).map(|g| ss.b[syl_base + g * SYL_SIZE..syl_base + (g + 1) * SYL_SIZE].to_vec()).collect();
    crate::unitsel::Word::from_raw_lang(&raw, &syls, crate::lang::Lang::Spa)
}
