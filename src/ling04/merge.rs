//! `fe_LinguisticAnalysis` (0x1004df40): tag and pronounce, then fold the morphs back into the
//! normalization records: each token's phones (concatenated over its morphs), morph count, one 0xff POS
//! slot per morph, the OR of the morph flags and the first morph's class.

use super::{morph, mr, war, Wa04, MORPH_COUNT, WH_FLAG};
use crate::lexicon::Lexicon;
use crate::ling::Ws;

/// Morphs in the token, from `rec+0xc`.
const NMORPH: usize = 0x0c;
/// One POS slot per morph from `rec+0x1a`.
const POS: usize = war::FORCED_POS;
/// Flags and class of the token, in the 8 bytes before the record.
const FLAGS: isize = -6;
const CLASS: isize = -4;

fn at(r: usize, d: isize) -> usize {
    (r as isize + d) as usize
}

/// Returns the engine's status: 0, -1 when the tokens don't add up, or the `mode` mapping (1 / -1).
pub fn analyze(wa: &mut Wa04, ws: &mut Ws, lex: &Lexicon, mode: i16) -> i16 {
    let n = wa.n();
    if !super::analysis::tag_and_pronounce(ws, wa, n, lex) {
        return -1;
    }
    let nm = ws.i16(MORPH_COUNT).max(0) as usize;
    let mut tokens = 0usize;
    let mut r = Wa04::rec(0);
    for c in 0..nm {
        let mo = morph(c);
        let p = ws.cstr(mo + mr::PRON).to_vec();
        if c == 0 || ws.i16(morph(c - 1) + mr::TOKEN) != ws.i16(mo + mr::TOKEN) {
            r = Wa04::rec(tokens);
            wa.set_u8(r + POS, 0xff);
            if wa.u8(r + war::U_FLAG) == b'U' || wa.u8(r + war::CASE_MODE) != b'Y' {
                wa.strcpy(r + war::PHONES, &p);
            }
            let k = wa.i32(r + NMORPH);
            wa.set_i32(r + NMORPH, k + 1);
            wa.set_u8(at(r + POS + 1, k as isize), 0xff);
            wa.set_u8(at(r, CLASS), ws.u8(mo + mr::CLASS_CHAR));
            wa.set_i16(at(r, FLAGS), ws.i16(mo + mr::FLAGS));
            tokens += 1;
        } else if wa.i32(r + super::WA_REC + NMORPH) < 5 {
            // The engine tests the next record's count here.
            if !p.is_empty() {
                let end = r + war::PHONES + wa.cstr(r + war::PHONES).len();
                wa.strcpy(end, &p);
            }
            let k = wa.i32(r + NMORPH);
            wa.set_u8(at(r + POS, k as isize), 0xff);
            wa.set_i32(r + NMORPH, k + 1);
            wa.set_u8(at(r + POS + 1, k as isize), 0xff);
            let f = wa.i16(at(r, FLAGS)) as u16 | ws.i16(mo + mr::FLAGS) as u16;
            wa.set_i16(at(r, FLAGS), f as i16);
        }
    }
    if nm > 0 {
        wa.set_i16(8, ws.i16(WH_FLAG));
    }
    if n != tokens {
        return -1;
    }
    if mode != 0 {
        return if mode != -1 { 1 } else { -1 };
    }
    0
}
