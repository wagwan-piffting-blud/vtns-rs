//! SS word-record layout per build. 2014 (`wr`): stride 0x3c0, syllables from 0x17d4c, sentence type at
//! 0x4770a. The 2009 English builds: stride 0x300, syllables from 0x1324c, type at 0x42c14, a 5-byte phone
//! context label and no domain / M-flag bytes.

use super::{wr, SS_TYPE, SYL_BASE, WORD_BASE, WORD_SIZE};
use crate::era::Era;

pub struct Lay {
    pub stride: usize,
    pub syl_base: usize,
    pub ss_type: usize,
    pub start: usize,
    pub end: usize,
    pub syl_ptr: usize,
    pub n_syl: usize,
    pub n_phones: usize,
    pub ctx: usize,
    pub ctx_stride: usize,
    pub dist_a: usize,
    pub dist_b: usize,
    pub m_flag: usize,
    pub domain: usize,
    pub text_ptr: usize,
    pub ph_ptr: usize,
    pub codes: usize,
    pub marks: usize,
    pub sylpos: usize,
    pub token_class: usize,
    pub x3ae: usize,
    pub src: usize,
    pub class_char: usize,
    pub tag_a: usize,
    pub tag_b: usize,
    pub x3b4: usize,
    pub wa_ptr: usize,
    pub minor: usize,
    pub brk: usize,
    pub is09: bool,
}

pub static L14: Lay = Lay {
    stride: WORD_SIZE,
    syl_base: SYL_BASE,
    ss_type: SS_TYPE,
    start: wr::START,
    end: wr::END,
    syl_ptr: wr::SYL_PTR,
    n_syl: wr::N_SYL,
    n_phones: wr::N_PHONES,
    ctx: wr::CTX,
    ctx_stride: 7,
    dist_a: wr::DIST_A,
    dist_b: wr::DIST_B,
    m_flag: wr::M_FLAG,
    domain: wr::DOMAIN,
    text_ptr: wr::TEXT_PTR,
    ph_ptr: wr::PH_PTR,
    codes: wr::CODES,
    marks: wr::MARKS,
    sylpos: wr::SYLPOS,
    token_class: wr::TOKEN_CLASS,
    x3ae: wr::X3AE,
    src: wr::SRC,
    class_char: wr::CLASS_CHAR,
    tag_a: wr::TAG_A,
    tag_b: wr::TAG_B,
    x3b4: wr::X3B4,
    wa_ptr: wr::WA_PTR,
    minor: wr::MINOR,
    brk: wr::BRK,
    is09: false,
};

/// julie09 and paul09 (identical record layout).
pub static L09: Lay = Lay {
    stride: 0x300,
    syl_base: 0x1324c,
    ss_type: 0x42c14,
    start: 0x2f4,
    end: 0x2f8,
    syl_ptr: 0x04,
    n_syl: 0x2e3,
    n_phones: 0x2e4,
    ctx: 0x90,
    ctx_stride: 5,
    dist_a: 0x1d5,
    dist_b: 0x1d6,
    m_flag: usize::MAX,
    domain: usize::MAX,
    text_ptr: 0x218,
    ph_ptr: 0x21c,
    codes: 0x220,
    marks: 0x261,
    sylpos: 0x2a2,
    token_class: 0x2e6,
    x3ae: 0x2e8,
    src: 0x2ea,
    class_char: 0x2eb,
    tag_a: 0x2ec,
    tag_b: 0x2ed,
    x3b4: 0x2ee,
    wa_ptr: 0x2f0,
    minor: 0x2fc,
    brk: 0x2fd,
    is09: true,
};

impl Lay {
    pub fn of(era: Era) -> &'static Lay {
        if era.is_2009() {
            &L09
        } else {
            &L14
        }
    }

    /// Offset of word record `i`.
    #[inline]
    pub fn word(&self, i: usize) -> usize {
        WORD_BASE + i * self.stride
    }
}
