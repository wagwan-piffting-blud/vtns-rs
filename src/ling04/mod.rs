//! Linguistic front end of the 2004 Paul engine (`vt_eng.dll`, PE 2005-03-31): `fe_LinguisticAnalysis`
//! (0x1004df40) over the 0x88-byte normalization records, then the word, phrase-break and phone-context
//! passes. The candidate array is the 2014 one (`ling::Ws`, `ling::lookup::cand`); morph entries are 0x6c
//! bytes from +0x429a6 and carry no category code.

pub mod analysis;
pub mod lookup;
pub mod lts;
pub mod merge;
pub mod morph;
pub mod sentence;
pub mod tables;
pub mod tagger;

use crate::ling::byte_struct;

pub const WA_HEAD: usize = 0x14;
pub const WA_REC: usize = 0x88;
pub const WA_MAX: usize = 100;
pub const WA_SIZE: usize = 0x3530;

pub const MORPH_COUNT: usize = 0x429a2;
pub const WH_FLAG: usize = 0x429a4;
pub const MORPH_BASE: usize = 0x429a6;
pub const MORPH_SIZE: usize = 0x6c;

byte_struct!(Wa04, WA_SIZE);

/// Normalization record fields.
pub mod war {
    pub const START: usize = 0x00;
    pub const END: usize = 0x04;
    pub const LEN: usize = 0x08;
    pub const PAUSE: usize = 0x0c;
    /// Offset and length of the sub-token (clitic) inside `TEXT`.
    pub const SUB_OFF: usize = 0x10;
    pub const SUB_LEN: usize = 0x14;
    pub const U_FLAG: usize = 0x18;
    pub const CASE_MODE: usize = 0x19;
    pub const FORCED_POS: usize = 0x1a;
    pub const TEXT: usize = 0x1e;
    pub const PHONES: usize = 0x3c;
}

/// Morph entry fields.
pub mod mr {
    pub const FLAGS: usize = 0x00;
    pub const TOKEN: usize = 0x02;
    pub const CLASS_CHAR: usize = 0x04;
    pub const SPELLING: usize = 0x05;
    pub const PRON: usize = 0x23;
    pub const B40: usize = 0x64;
    pub const B10: usize = 0x66;
    pub const B20: usize = 0x68;
    pub const B80: usize = 0x6a;
}

impl Wa04 {
    pub fn n(&self) -> usize {
        self.i32(4).max(0) as usize
    }

    pub fn rec(i: usize) -> usize {
        WA_HEAD + i * WA_REC
    }

    /// The short 8 bytes before record `i` (the linguistic functions address records from WA+0xc); the
    /// morphology passes it on as the hyphen splitter's "dash" argument (2014: token class 0xb).
    pub fn split_mark(&self, i: usize) -> i16 {
        self.i16(Wa04::rec(i) - 8)
    }
}

/// Offset of morph entry `i` in a `ling::Ws`.
pub fn morph(i: usize) -> usize {
    MORPH_BASE + i * MORPH_SIZE
}
