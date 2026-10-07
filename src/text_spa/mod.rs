//! Text front end of vt_spa_violeta16.dll (Violeta): processed text -> per-sentence
//! normalization word arrays (`ling_spa::WaSpa`), as `text` does for vt_eng.dll.

pub mod abbrev;
pub mod abbrev_groups;
pub mod abbrev_tables;
pub mod emit;
pub mod initial;
pub mod norm_currency;
pub mod norm_date;
pub mod norm_fraction;
pub mod norm_math;
pub mod norm_number;
pub mod norm_numexpr;
pub mod norm_phone;
pub mod norm_phone09;
pub mod norm_roman;
pub mod norm_symbols;
pub mod norm_symbols2;
pub mod norm_time;
pub mod norm_units;
pub mod norm_url;
pub mod norm_vtml;
pub mod normalize;
pub mod numwords;
pub mod punct;
pub mod spell;
pub mod tables;
pub mod tables_oth;
pub mod tagmode;
pub mod tokenize;
pub mod userdict_spa;
pub mod word;

use std::io;
use std::path::Path;

use crate::era::Era;
use crate::ling_spa::lexicon::LexiconSpa;
use crate::ling_spa::WaSpa;
use userdict_spa::UserDictSpa;

/// Front-end resources (`data-common\dict-spa`).
pub struct FeSpa {
    pub lex: LexiconSpa,
    pub userdict: Option<UserDictSpa>,
    pub era: Era,
}

impl FeSpa {
    pub fn load(dict_spa: &Path) -> io::Result<FeSpa> {
        FeSpa::load_era(dict_spa, Era::V2014)
    }

    pub fn load_era(dict_spa: &Path, era: Era) -> io::Result<FeSpa> {
        let lex = LexiconSpa::open_era(dict_spa, era)?;
        // `data-common/userdict/userdict_spa.csv`, a sibling of `dict-spa`.
        let userdict = dict_spa
            .parent()
            .and_then(|common| std::fs::read(common.join("userdict").join("userdict_spa.csv")).ok())
            .and_then(|b| UserDictSpa::load(&b));
        Ok(FeSpa { lex, userdict, era })
    }
}

/// WA record fields written by the text front end (record-relative).
pub mod rec {
    pub const START: usize = 0x00;
    pub const END: usize = 0x04;
    pub const LEN: usize = 0x08;
    pub const PAUSE: usize = 0x0c;
    pub const ORIGIN: usize = 0x16;
    pub const CASE: usize = 0x17;
    pub const SRC: usize = 0x1c;
    /// Punctuation after the word: 1 newline, 2 sentence end, 3 question.
    pub const PUNCT: usize = 0x20;
    pub const TEXT: usize = 0x24;
    pub const PHON: usize = 0x42;
}

/// WA header: `+4` consumed, `+8` tag mode.
pub const CONSUMED: usize = 4;
pub const TAG_MODE: usize = 8;

/// `FUN_10035350` (`phones` None) / `FUN_10035400`: append a word record. False
/// when the WA is full (the engine's 0).
pub fn emit(wa: &mut WaSpa, text: &[u8], start: i32, end: i32, origin: u8, case: u8, src: u32, phones: Option<&[u8]>) -> bool {
    if text.is_empty() {
        return true;
    }
    let n = wa.i16(0);
    if n > 99 {
        return false;
    }
    let r = WaSpa::rec(n as usize);
    wa.set_i32(r + rec::START, start);
    wa.set_i32(r + rec::END, end);
    wa.set_i32(r + rec::LEN, text.len() as i32);
    wa.set_u8(r + rec::ORIGIN, origin);
    wa.set_u8(r + rec::CASE, case);
    wa.set_u32(r + rec::SRC, src & 0xff);
    wa.strcpy(r + rec::TEXT, text);
    if let Some(p) = phones {
        wa.strcpy(r + rec::PHON, p);
    }
    wa.set_i16(0, n + 1);
    wa.set_i32(CONSUMED, end);
    true
}

/// One sentence as `fe_NormalizeTokens` leaves it.
#[derive(Clone)]
pub struct Sentence {
    pub text_off: i32,
    pub wa: WaSpa,
}

/// `FUN_10031010`: clear the WA for the next sentence (the tag mode survives).
pub fn reset(wa: &mut WaSpa) {
    let tag = wa.i16(TAG_MODE);
    wa.b.iter_mut().for_each(|b| *b = 0);
    wa.set_i16(TAG_MODE, tag);
    for i in 0..100 {
        wa.set_i32(WaSpa::rec(i) + rec::PAUSE, -1);
    }
}

/// The `fe_NextSentence` loop over processed text.
pub fn sentences(fe: &FeSpa, processed: &[u8]) -> Vec<Sentence> {
    let mut t = processed.to_vec();
    t.extend_from_slice(&[0u8; 64]);
    let mut out = Vec::new();
    let mut cursor = 0usize;
    let mut wa = WaSpa::new();
    loop {
        reset(&mut wa);
        normalize::normalize_tokens(fe, &mut wa, &t, cursor);
        let c = wa.i32(CONSUMED);
        if c == 0 {
            break;
        }
        if wa.i16(0) > 0 {
            out.push(Sentence { text_off: cursor as i32, wa: wa.clone() });
        }
        cursor += c as usize;
    }
    out
}
