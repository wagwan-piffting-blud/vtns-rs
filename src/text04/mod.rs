//! Text front end of the 2004 Paul engine (`vt_eng.dll`, PE 2005-03-31): request text -> processed text
//! (`initial`) -> per-sentence normalization records (`ling04::Wa04`) for `ling04::merge::analyze`.
//!
//! The 2004 normalizer is an older design than 2014's: `fe_NormalizeTokens` (0x1004d720) reads tokens
//! into a look-ahead buffer and offers them to a fixed chain of recognizers that consume any number of
//! them.

pub mod alias;
pub mod initial;
pub mod mem;
pub mod norm_abbr;
pub mod norm_addr;
pub mod norm_alnum;
pub mod norm_cur;
pub mod norm_date;
pub mod norm_digits;
pub mod norm_math;
pub mod norm_num;
pub mod norm_phone;
pub mod norm_sym;
pub mod norm_time;
pub mod norm_units;
pub mod norm_url;
pub mod normalize;
pub mod num;
pub mod rx;
pub mod sbd;
pub mod stream;
pub mod tables;
pub mod tokenize;
pub mod userdict;
pub mod wa;

use std::io;
use std::path::Path;

use crate::lexicon::Txt2;
use crate::tree3::Tree;

pub use wa::Wa;

/// Front-end resources (`FUN_10036eb0`): the user dictionary, `sbd.tree2` and the `.txt2` lists of
/// `data-common/dict-eng`.
pub struct Fe {
    pub ud: Option<userdict::UserDict>,
    pub sbd: Tree,
    pub abbrh: Txt2,
    pub abbrt: Txt2,
    pub abbrc: Txt2,
    pub sbdw: Txt2,
    pub streeta: Txt2,
    pub streetf: Txt2,
    pub city: [Txt2; 4],
    pub citya: Txt2,
    /// The main dictionary (`engttsdict_emb`): the abbreviation recognizer reads its entry flags.
    pub lex: crate::lexicon::Lexicon,
}

impl Fe {
    /// `common` is `data-common`.
    pub fn load(common: &Path) -> io::Result<Fe> {
        let d = common.join("dict-eng");
        let m = |name: &str| Txt2::open04(&d.join(name), true, true);
        Ok(Fe {
            ud: std::fs::read(common.join("userdict").join("userdict_eng.csv")).ok().and_then(|b| userdict::UserDict::load(&b)),
            sbd: crate::tree2::load(&d.join("sbd.tree2"), false)?,
            abbrh: m("abbrh.txt2")?,
            abbrt: m("abbrt.txt2")?,
            abbrc: m("abbrc.txt2")?,
            sbdw: m("sbdw.txt2")?,
            streeta: m("streeta.txt2")?,
            streetf: Txt2::open04(&d.join("streetf.txt2"), false, true)?,
            city: [m("city1.txt2")?, m("city2.txt2")?, m("city3.txt2")?, m("city4.txt2")?],
            citya: m("citya.txt2")?,
            lex: crate::lexicon::Lexicon::open_era(&d, crate::era::Era::Paul04)?,
        })
    }
}

/// One sentence as the normalizer leaves it.
#[derive(Clone)]
pub struct Sentence {
    /// Where the sentence begins in the processed text (before its leading blanks): record offsets
    /// count from here.
    pub cursor: i32,
    /// Offset of the normalizer's input (after the leading blanks) in the processed text.
    pub text_off: i32,
    pub ret: i32,
    pub wa: Wa,
}

/// Processed text padded with NULs so reads past the end see the engine's terminator.
pub fn padded(text: &[u8]) -> Vec<u8> {
    let mut v = text.to_vec();
    v.extend_from_slice(&[0u8; 64]);
    v
}

/// The `fe_NextSentence` loop: normalize at the cursor, skip sentences without words, stop when nothing
/// is consumed.
pub fn sentences(fe: &Fe, processed: &[u8]) -> Vec<Sentence> {
    let t = padded(processed);
    let mut out = Vec::new();
    let mut cursor = 0usize;
    let mut st = normalize::State::default();
    loop {
        let (off, ret) = normalize::analyze(fe, &mut st, &t, cursor);
        let c = st.wa.consumed();
        if c <= 0 {
            break;
        }
        if st.wa.n() > 0 {
            out.push(Sentence { cursor: cursor as i32, text_off: off as i32, ret, wa: st.wa.clone() });
        }
        cursor += c as usize;
    }
    out
}
