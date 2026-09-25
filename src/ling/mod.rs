//! Linguistic front end of vt_eng.dll: normalized word array (WA) -> sentence
//! structure (SS) word/phone/syllable records handed to prosody and unit
//! selection.
//!
//! The engine works on two large fixed-layout structs, and this port keeps
//! them as raw byte buffers addressed with the engine's own offsets so every
//! record can be compared byte for byte with captures of the original engine:
//!
//! * `Wa`  -- the normalization word array (`TIB+0x1312bc`): `s16 n`, `s16 wh`,
//!   `i32 consumed`, ..., then 100 records of 0x94 bytes from +0x14.
//! * `Ss`  -- the sentence structure (`TIB+0x4c`): phrase table from +0x0c,
//!   word records (0x3c0) from +0x64c, syllable records (0x1e) from +0x17d4c.
//!
//! Pointers the engine stores inside these structs are kept as tagged offsets
//! (see `ptr_ss` / `ptr_wa`); parity tests mask them when comparing.

pub mod analysis;
pub mod breaks;
pub mod context;
pub mod funcwords;
pub mod lookup;
pub mod lts;
pub mod merge;
pub mod morph;
pub mod syllab;
pub mod tagger;
pub mod tables;
pub mod words;

pub const WA_HEAD: usize = 0x14;
pub const WA_REC: usize = 0x94;
pub const WA_MAX: usize = 100;
pub const WA_SIZE: usize = WA_HEAD + WA_MAX * WA_REC;

pub const WORD_BASE: usize = 0x64c;
pub const WORD_SIZE: usize = 0x3c0;
pub const SYL_BASE: usize = 0x17d4c;
pub const SYL_SIZE: usize = 0x1e;
pub const SS_TYPE: usize = 0x4770a;
pub const SS_SIZE: usize = 0x47720;

const TAG_SS: u32 = 0x1000_0000;
const TAG_WA: u32 = 0x2000_0000;
const TAG_MASK: u32 = 0xf000_0000;

pub fn ptr_ss(off: usize) -> u32 {
    TAG_SS | off as u32
}

pub fn ptr_wa(off: usize) -> u32 {
    TAG_WA | off as u32
}

/// Index into a table the engine reads with a signed `char` subscript.
#[inline]
pub fn sc(t: &[u8; 256], c: u8) -> u8 {
    t[c as usize]
}

macro_rules! byte_struct {
    ($name:ident, $size:expr) => {
        #[derive(Clone)]
        pub struct $name {
            pub b: Vec<u8>,
        }

        impl $name {
            pub fn new() -> Self {
                $name { b: vec![0u8; $size] }
            }

            #[inline]
            pub fn u8(&self, o: usize) -> u8 {
                self.b[o]
            }

            #[inline]
            pub fn set_u8(&mut self, o: usize, v: u8) {
                self.b[o] = v;
            }

            #[inline]
            pub fn i16(&self, o: usize) -> i16 {
                i16::from_le_bytes([self.b[o], self.b[o + 1]])
            }

            #[inline]
            pub fn set_i16(&mut self, o: usize, v: i16) {
                self.b[o..o + 2].copy_from_slice(&v.to_le_bytes());
            }

            #[inline]
            pub fn i32(&self, o: usize) -> i32 {
                i32::from_le_bytes([self.b[o], self.b[o + 1], self.b[o + 2], self.b[o + 3]])
            }

            #[inline]
            pub fn set_i32(&mut self, o: usize, v: i32) {
                self.b[o..o + 4].copy_from_slice(&v.to_le_bytes());
            }

            #[inline]
            pub fn u32(&self, o: usize) -> u32 {
                self.i32(o) as u32
            }

            #[inline]
            pub fn set_u32(&mut self, o: usize, v: u32) {
                self.set_i32(o, v as i32);
            }

            /// NUL-terminated string starting at `o` (bounded by the buffer).
            pub fn cstr(&self, o: usize) -> &[u8] {
                let s = &self.b[o..];
                let n = s.iter().position(|&c| c == 0).unwrap_or(s.len());
                &s[..n]
            }

            /// `strcpy(&buf[o], src)`: writes src and the terminator only.
            pub fn strcpy(&mut self, o: usize, src: &[u8]) {
                self.b[o..o + src.len()].copy_from_slice(src);
                self.b[o + src.len()] = 0;
            }

            /// `strcat(&buf[o], src)`.
            pub fn strcat(&mut self, o: usize, src: &[u8]) {
                let n = self.cstr(o).len();
                self.strcpy(o + n, src);
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

byte_struct!(Wa, WA_SIZE);
byte_struct!(Ss, SS_SIZE);

pub const CAND_SIZE: usize = 0x554;
pub const CAND_MAX: usize = 200;
pub const MORPH_COUNT: usize = 0x429a2;
pub const WH_FLAG: usize = 0x429a4;
pub const MORPH_BASE: usize = 0x429a6;
pub const MORPH_SIZE: usize = 0x70;
pub const WS_SIZE: usize = 0x48200;

byte_struct!(Ws, WS_SIZE);

impl Ws {
    /// Candidate `i` (0x554 bytes) from +2; the s16 count is at +0.
    pub fn cand(i: usize) -> usize {
        2 + i * CAND_SIZE
    }

    /// Morph entry `i` (0x70 bytes).
    pub fn morph(i: usize) -> usize {
        MORPH_BASE + i * MORPH_SIZE
    }

    pub fn n_morphs(&self) -> usize {
        self.i16(MORPH_COUNT).max(0) as usize
    }
}

/// Morph entry fields.
pub mod mr {
    pub const FLAGS: usize = 0x00;
    pub const TOKEN: usize = 0x02;
    pub const CAT: usize = 0x04;
    pub const CLASS_CHAR: usize = 0x06;
    pub const SPELLING: usize = 0x07;
    pub const PRON: usize = 0x25;
    pub const ACCENT: usize = 0x66;
    pub const B40: usize = 0x68;
    pub const B10: usize = 0x6a;
    pub const B20: usize = 0x6c;
    pub const B80: usize = 0x6e;
}

impl Wa {
    pub fn n(&self) -> usize {
        self.i16(0).max(0) as usize
    }

    /// Offset of word record `i`.
    pub fn rec(i: usize) -> usize {
        WA_HEAD + i * WA_REC
    }

    /// WA header +2: the wh-question flag (`FUN_1000e990` -> `ws+0x429a4`),
    /// copied in by `fe_LinguisticAnalysis` once the merge has run.
    pub fn wh(&self) -> i16 {
        self.i16(2)
    }

    pub fn set_wh(&mut self, v: i16) {
        self.set_i16(2, v);
    }

    /// WA header +4: `consumed`, the number of (processed-text) characters
    /// this sentence's analysis used up. `fe_NextSentence` (`0x10022dc0`)
    /// adds this to its running `TIB+4` cursor after each sentence; that
    /// cursor is what turns a WA word's own (per-sentence-relative)
    /// `war::START`/`war::END` into the SS word record's request-relative
    /// `wr::START`/`wr::END` -- see `words::build_word_phone_records`'s
    /// `cursor` parameter.
    pub fn consumed(&self) -> i32 {
        self.i32(4)
    }
}

/// Word-array record fields (offsets relative to the record).
pub mod war {
    pub const START: usize = 0x00;
    pub const END: usize = 0x04;
    pub const LEN: usize = 0x08;
    pub const PAUSE: usize = 0x0c;
    pub const NMORPH: usize = 0x10;
    /// Sub-token (clitic) offset into `TEXT` and its length, read by
    /// `FUN_100091b0`.
    pub const SUB_OFF: usize = 0x14;
    pub const SUB_LEN: usize = 0x18;
    pub const FLAGS: usize = 0x1c;
    pub const CLASS_CHAR: usize = 0x22;
    /// 'U' flag read by `FUN_100091b0` (morphology's U-token handler).
    pub const U_FLAG: usize = 0x23;
    pub const CASE_MODE: usize = 0x24;
    pub const TAG_A: usize = 0x25;
    pub const TAG_B: usize = 0x26;
    pub const SRC: usize = 0x28;
    pub const TOKEN_CLASS: usize = 0x2c;
    pub const FORCED_POS: usize = 0x30;
    pub const TEXT: usize = 0x34;
    pub const TEXT_MAX: usize = 0x1e;
    pub const PHONES: usize = 0x52;
    pub const PHONES_MAX: usize = 0x42;
}

/// Word record fields (offsets relative to the word base).
pub mod wr {
    pub const START: usize = 0x000;
    pub const END: usize = 0x004;
    pub const SYL_PTR: usize = 0x008;
    pub const N_SYL: usize = 0x094;
    pub const N_PHONES: usize = 0x095;
    pub const CTX: usize = 0x096;
    pub const DIST_A: usize = 0x25d;
    pub const DIST_B: usize = 0x25e;
    pub const M_FLAG: usize = 0x29e;
    pub const DOMAIN: usize = 0x2df;
    pub const TEXT_PTR: usize = 0x2e0;
    pub const PH_PTR: usize = 0x2e4;
    pub const CODES: usize = 0x2e8;
    pub const MARKS: usize = 0x329;
    pub const SYLPOS: usize = 0x36a;
    pub const TOKEN_CLASS: usize = 0x3ac;
    pub const X3AE: usize = 0x3ae;
    pub const SRC: usize = 0x3b0;
    pub const CLASS_CHAR: usize = 0x3b1;
    pub const TAG_A: usize = 0x3b2;
    pub const TAG_B: usize = 0x3b3;
    pub const X3B4: usize = 0x3b4;
    pub const WA_PTR: usize = 0x3b8;
    pub const MINOR: usize = 0x3bc;
    pub const BRK: usize = 0x3bd;
}

/// Syllable record fields.
pub mod syl {
    pub const STRESS: usize = 0x00;
    pub const SHAPE: usize = 0x01;
    pub const POSITION: usize = 0x02;
    pub const TONE: usize = 0x03;
    pub const F0: usize = 0x04;
    pub const FIRST_PHONE: usize = 0x1c;
    pub const N_PHONES: usize = 0x1d;
}

impl Ss {
    pub fn n_words(&self) -> usize {
        self.i16(2).max(0) as usize
    }

    pub fn word(i: usize) -> usize {
        WORD_BASE + i * WORD_SIZE
    }

    pub fn n_phrases(&self) -> usize {
        self.i16(0).max(0) as usize
    }

    /// Phrase table entry `g` (16 bytes): `+0 s16 first word, +2 s16 n_words,
    /// +4 s16 n_syl, +8 ptr first word, +0xc ptr first syllable`.
    pub fn phrase(g: usize) -> usize {
        0x0c + g * 0x10
    }

    /// Resolve a tagged pointer stored in the SS to an SS offset.
    pub fn deref(&self, p: u32) -> Option<usize> {
        if p & TAG_MASK == TAG_SS {
            Some((p & !TAG_MASK) as usize)
        } else {
            None
        }
    }
}

/// The per-request arrays the front end reads and writes outside the SS
/// (all in the TIB): per-word pause (`+0x121a60`) and forced break level
/// (`+0x121d80`), each `int[200]`, plus the WA itself.
#[derive(Clone)]
pub struct Tib {
    pub pause: Vec<i32>,
    pub brk_level: Vec<i32>,
}

impl Tib {
    pub fn new() -> Self {
        Tib { pause: vec![-1; 200], brk_level: vec![-1; 200] }
    }
}

impl Default for Tib {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolved `char*` fields of the SS words that point into the WA.
pub struct WordStrings<'a> {
    pub wa: &'a Wa,
}

impl<'a> WordStrings<'a> {
    fn resolve(&self, p: u32) -> &'a [u8] {
        if p & TAG_MASK == TAG_WA {
            self.wa.cstr((p & !TAG_MASK) as usize)
        } else {
            &[]
        }
    }

    pub fn text(&self, ss: &Ss, i: usize) -> &'a [u8] {
        self.resolve(ss.u32(Ss::word(i) + wr::TEXT_PTR))
    }

    pub fn phones(&self, ss: &Ss, i: usize) -> &'a [u8] {
        self.resolve(ss.u32(Ss::word(i) + wr::PH_PTR))
    }

    /// Raw byte `k` of the word's WA phone string (the engine indexes it
    /// directly, past the NUL if asked).
    pub fn phone_byte(&self, ss: &Ss, i: usize, k: usize) -> u8 {
        let p = ss.u32(Ss::word(i) + wr::PH_PTR);
        if p & TAG_MASK == TAG_WA {
            let o = (p & !TAG_MASK) as usize + k;
            *self.wa.b.get(o).unwrap_or(&0)
        } else {
            0
        }
    }
}
