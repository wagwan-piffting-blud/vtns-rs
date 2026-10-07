//! Phone tables the syllabifier, the phrase-break pass and the context builder read, per build. The 2014
//! tables are the generated ones in `tables.rs`; the 2009 builds carry their own copies
//! (`tables09.rs`, from the .rdata/.data snapshots of the two DLLs).

use super::tables::{
    BREAK_OF_CLASS, NG_NAME, ONSETS, PALATAL_NEXT, PHONE_FEAT, PH_BASE, PH_BB70, PH_BE3C, PH_RHOTIC, PH_STRESS, PH_VOWEL,
};
use super::tables09::{J09, P09};
use crate::era::Era;

pub struct PhTabs {
    pub base: &'static [u8; 256],
    pub stress: &'static [u8; 256],
    pub vowel: &'static [u8; 256],
    pub rhotic: &'static [u8; 256],
    pub be3c: &'static [u8; 256],
    pub bb70: &'static [u8; 256],
    pub feat: &'static [u8],
    pub onsets: &'static [&'static [u8]],
    pub ng: &'static [u8],
    pub brk_of_class: &'static [u8],
    pub palatal: &'static [&'static [u8]],
}

pub static T14: PhTabs = PhTabs {
    base: &PH_BASE,
    stress: &PH_STRESS,
    vowel: &PH_VOWEL,
    rhotic: &PH_RHOTIC,
    be3c: &PH_BE3C,
    bb70: &PH_BB70,
    feat: &PHONE_FEAT,
    onsets: &ONSETS,
    ng: NG_NAME,
    brk_of_class: BREAK_OF_CLASS,
    palatal: &PALATAL_NEXT,
};

impl PhTabs {
    pub fn of(era: Era) -> &'static PhTabs {
        match era {
            Era::Julie09 => &J09,
            Era::Paul09 => &P09,
            _ => &T14,
        }
    }
}
