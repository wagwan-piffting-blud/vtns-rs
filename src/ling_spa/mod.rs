//! Linguistic front end of vt_spa_violeta16.dll (Violeta): WA -> SS, as `ling` does for
//! vt_eng.dll. Same raw-byte-buffer approach and tagged pointers (`ling::ptr_ss` /
//! `ling::ptr_wa`); only the layouts differ.
//!
//! * `WaSpa` -- 100 records of 0xa8 bytes from +0xc (`fe_EmitWord`, 0x10035350).
//! * `SsSpa` -- word records (0x540) from +0x64c, syllable records (0x28).
//!
//! The per-word pause / break-level arrays live at TIB+0x15d130 / +0x15d450 and reuse
//! `ling::Tib`.

pub mod analysis;
pub mod breaks;
pub mod context;
pub mod lexicon;
pub mod lts;
pub mod syllab;
pub mod words;

use crate::ling::byte_struct;

pub const WA_HEAD: usize = 0x0c;
pub const WA_REC: usize = 0xa8;
pub const WA_MAX: usize = 100;
pub const WA_SIZE: usize = WA_HEAD + WA_MAX * WA_REC;

pub const WORD_BASE: usize = 0x64c;
pub const WORD_SIZE: usize = 0x540;
/// WORD_BASE + 100 words (a literal in `FUN_1001e1e0`).
pub const SYL_BASE: usize = WORD_BASE + 100 * WORD_SIZE;
pub const SYL_SIZE: usize = 0x28;
pub const PHRASE_INDEX: usize = 0x82dcc;
pub const SS_TYPE: usize = 0x82dda;
pub const SS_SIZE: usize = 0x82df0;

byte_struct!(WaSpa, WA_SIZE);
byte_struct!(SsSpa, SS_SIZE);

impl WaSpa {
    pub fn n(&self) -> usize {
        self.i16(0).max(0) as usize
    }

    pub fn rec(i: usize) -> usize {
        WA_HEAD + i * WA_REC
    }
}

/// Word-array record fields known so far.
pub mod war {
    pub const START: usize = 0x00;
    pub const END: usize = 0x04;
    pub const LEN: usize = 0x08;
    pub const TEXT: usize = 0x24;
}

/// Word record fields known so far (batch 1/2 logs in VIOLETA_TODO.md).
pub mod wr {
    pub const START: usize = 0x000;
    pub const END: usize = 0x004;
    pub const SYL_PTR: usize = 0x008;
    pub const TARGET_LIST: usize = 0x00c;
    pub const N_SYL: usize = 0x0da;
    pub const N_PHONES: usize = 0x0db;
    pub const CTX: usize = 0x0dc;
    pub const PHONES: usize = 0x408;
    pub const POS: usize = 0x4d0;
    pub const BRK: usize = 0x53c;
}

/// Syllable record fields known so far.
pub mod syl {
    pub const TONE: usize = 0x03;
    pub const F0: usize = 0x04;
    pub const PHONES_PTR: usize = 0x1c;
    pub const POS_PTR: usize = 0x20;
    pub const N_PHONES: usize = 0x24;
}

impl SsSpa {
    pub fn n_words(&self) -> usize {
        self.i16(2).max(0) as usize
    }

    pub fn n_phrases(&self) -> usize {
        self.i16(0).max(0) as usize
    }

    pub fn word(i: usize) -> usize {
        WORD_BASE + i * WORD_SIZE
    }

    pub fn phrase(g: usize) -> usize {
        0x0c + g * 0x10
    }
}
