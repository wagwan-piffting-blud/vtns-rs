//! Generated from vt_spa_violeta16.dll. Do not edit.
//! Violeta phone tables: unlike English, there is no PHONE_PROPS row table with
//! isVowel/vclass/cclass fields consulted by prosody. Duration and pitch prediction
//! instead consult one direct per-code class table (0-7, selecting among 8 duration
//! trees and doubling as a pitch feature) plus separate base-index and stress tables.

/// Phone code -> base phone index (0x10067114), read as signed char (`& 0x7f` masked
/// like the English port's `phone_index`, though every observed Violeta code is < 0x40).
pub const PHONE_INDEX_SPA: [i8; 128] = [
    0, 1, 1, 24, 24, 3, 3, 4, 4, 5, 6, 7, 8, 9, 10, 10,
    11, 11, 12, 12, 13, 14, 15, 16, 16, 17, 18, 19, 20, 21, 22, 23,
    24, 24, 25, 25, 26, 26, 27, 28, 29, 30, 31, 32, 33, 33, 6, 35,
    35, 36, 36, 37, 37, 38, 38, 39, 39, 40, 40, 41, 41, 42, 42, 43,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 32,
    33, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19,
    20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35,
    36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 10, 47, 48, 49, 50, 51,
];

/// Phone code -> stress flag (0x100673c0). Binary (0/1), unlike English's 0/1/2:
/// Spanish marks one stressed syllable per word, no primary/secondary distinction.
/// Raw bytes copied verbatim past the table's real domain (idx 76 = 0xff and beyond
/// spill into the next table, `PHONE_CLASS8_SPA`) since real phone codes never reach
/// there and copying exactly replicates what an out-of-range read in the DLL would see.
pub const PHONE_STRESS_SPA: [u8; 128] = [
    0, 0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 1,
    0, 1, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0,
    0, 1, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0,
    1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 0, 0, 0,
    0, 1, 1, 1, 1, 3, 3, 5, 3, 3, 0, 0, 1, 1, 1, 1,
    4, 3, 4, 0, 0, 5, 3, 6, 6, 6, 6, 6, 0, 0, 1, 1,
    1, 1, 3, 7, 7, 4, 4, 3, 0, 0, 4, 2, 2, 2, 2, 2,
];

/// Phone code -> duration-tree class (0x1006740c): 0=vmono 1=vdif 2=vdib 3=cstop
/// 4=cfri 5=caff 6=cnas 7=capp; 255 = invalid (never a real word-internal phone).
/// pro_PredictPitch also reads this table directly as fv[0] (in place of English's
/// consonant-class phone property).
pub const PHONE_CLASS8_SPA: [u8; 128] = [
    255, 0, 0, 0, 0, 1, 1, 1, 1, 3, 3, 5, 3, 3, 0, 0,
    1, 1, 1, 1, 4, 3, 4, 0, 0, 5, 3, 6, 6, 6, 6, 6,
    0, 0, 1, 1, 1, 1, 3, 7, 7, 4, 4, 3, 0, 0, 4, 2,
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 5,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0,
    0, 1, 1, 1, 1, 3, 3, 5, 3, 3, 1, 1, 1, 1, 1, 1,
    4, 3, 4, 1, 1, 4, 4, 2, 2, 2, 2, 2, 1, 1, 1, 1,
    1, 1, 4, 2, 2, 4, 4, 4, 1, 1, 3, 1, 1, 1, 1, 1,
];

/// VTML span clamps (0x1006485c / 0x10064880): identical to English's
/// `SPAN_MAX`/`SPAN_MIN` (`prosody_tables.rs`), confirmed by decompile.
pub const SPAN_MAX_SPA: [i32; 8] = [200, 400, 500, 65535, 3, 0, 9, 9];
pub const SPAN_MIN_SPA: [i32; 8] = [50, 50, 0, 0, 0, 0, 0, 0];
