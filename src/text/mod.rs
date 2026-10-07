//! Text front end of vt_eng.dll: input text -> processed text (tags removed,
//! offset maps) -> per-sentence normalization word arrays (WA). The WA is the
//! hand-off to the linguistic front end (`ling`).

pub mod chars;
pub mod initial;
pub mod cstr;
pub mod norm;
pub mod norm_address;
pub mod norm_address_city;
pub mod norm_address_csc;
pub mod norm_address_street;
pub mod norm_compound;
pub mod norm_date;
pub mod norm_digits;
pub mod norm_fraction;
pub mod norm_math;
pub mod norm_numword;
pub mod norm_phone;
pub mod norm_symbols;
pub mod norm_currency;
pub mod norm_n15;
pub mod norm_name;
pub mod norm_numexpr;
pub mod norm_time;
pub mod norm_units;
pub mod norm_url;
pub mod norm_vtml;
pub mod normalize;
pub mod numwords;
pub mod phrases;
pub mod sbd;
pub mod tables;
pub mod tables_address;
pub mod tables_julie09;
pub mod tables_paul09;
pub mod tabs;
pub mod tokenize;
pub mod txt2;
pub mod userdict;
pub mod wa;
pub mod window;

use std::io;
use std::path::Path;

use crate::era::Era;
use crate::tree3::Tree;

pub use wa::Wa;

/// Front-end resources loaded once per engine (common DB, `data-common`).
pub struct Fe {
    pub ud: Option<userdict::UserDict>,
    pub sbd: Tree,
    pub abbrh: txt2::List,
    pub abbrt: txt2::List,
    pub abbrc: txt2::List,
    pub sbdw: txt2::List,
    pub streeta: txt2::List,
    pub streetf: txt2::List,
    pub citya: txt2::List,
    /// `tppdict_eng`: the place-name fields `A`..`E` gate `norm_address_city`.
    pub tpp: crate::lexicon::HashDict,
    /// `engttsdict_emb`: `word` looks hyphenated compounds up whole.
    pub emb: crate::lexicon::HashDict,
    /// Match the 64-bit voice DLLs where they read uninitialized stack
    /// differently (see `norm_n15::abbr`).
    pub x64: bool,
    pub era: Era,
}

impl Fe {
    /// `common` is `data-common` (holding `dict-eng` and `userdict`).
    pub fn load(common: &Path) -> io::Result<Fe> {
        Fe::load_era(common, Era::V2014)
    }

    /// Paul09 reads `sbd.tree2`; the other builds `sbd.tree3`.
    pub fn load_era(common: &Path, era: Era) -> io::Result<Fe> {
        let d = common.join("dict-eng");
        let ud = std::fs::read(common.join("userdict").join("userdict_eng.csv")).ok().and_then(|b| userdict::UserDict::load(&b));
        Ok(Fe {
            ud,
            sbd: if era == Era::Paul09 { crate::tree2::load(&d.join("sbd.tree2"), false)? } else { Tree::load(&d.join("sbd.tree3"))? },
            abbrh: txt2::List::load(&d.join("abbrh_sort.txt2"), b'M')?,
            abbrt: txt2::List::load(&d.join("abbrt_sort.txt2"), b'M')?,
            abbrc: txt2::List::load(&d.join("abbrc_sort.txt2"), b'M')?,
            sbdw: txt2::List::load(&d.join("sbdw_sort.txt2"), b'M')?,
            streeta: txt2::List::load(&d.join("streeta_sort.txt2"), b'M')?,
            streetf: txt2::List::load(&d.join("streetf_sort.txt2"), b'L')?,
            citya: txt2::List::load(&d.join("citya_sort.txt2"), b'M')?,
            tpp: crate::lexicon::HashDict::open(&d, "eng_tpp", "tppdict_eng")?,
            emb: crate::lexicon::HashDict::open(&d, "emb", "engttsdict_emb")?,
            x64: false,
            era,
        })
    }

    /// `FUN_10010810`: the part before a hyphen is one of the prefixes
    /// "de"/"inter"/"re" (`PTR_DAT_100782f4`, case-insensitive).
    pub fn lex_has_prefix(&self, w: &[u8]) -> bool {
        tabs::ptab(0x100782f4, 0x10078300).find(w, true) >= 0
    }
}

/// One sentence as the normalizer leaves it (`fe_NormalizeTokens` onLeave).
#[derive(Clone)]
pub struct Sentence {
    /// Offset of the sentence start in the processed text.
    pub text_off: i32,
    pub wa: Wa,
}

/// Processed text padded with NULs so the transliterated code can read past
/// the end the way the engine reads its heap buffer.
pub fn padded(text: &[u8]) -> Vec<u8> {
    let mut v = text.to_vec();
    v.extend_from_slice(&[0u8; 64]);
    v
}

/// The `fe_NextSentence` loop over processed text: normalize a sentence at
/// the cursor, skip sentences that produced no words, stop when nothing is consumed.
pub fn sentences(fe: &Fe, processed: &[u8]) -> Vec<Sentence> {
    let t = padded(processed);
    let mut out = Vec::new();
    let mut cursor: usize = 0;
    let mut wa = Wa::new();
    wa.set_i32(8, 1);
    loop {
        wa.reset();
        for i in 0..wa::MAX as i32 {
            wa.set_ri32(i, 0xc, -1);
        }
        normalize::normalize_tokens(fe, &mut wa, &t, cursor);
        let c = wa.consumed();
        if c == 0 {
            break;
        }
        if wa.n() > 0 {
            out.push(Sentence { text_off: cursor as i32, wa: wa.clone() });
        }
        cursor += c as usize;
    }
    out
}
