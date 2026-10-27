//! `FUN_1000a110`: split every normalization record into lexicon pieces and fill one candidate per piece.

use super::{war, Wa04};
use crate::lexicon::Lexicon;
use super::tables::CODEBOOK;
use crate::ling::lookup::{cand, fill_candidate_with, split_token_era, TokenBuf};
use crate::ling::Ws;

/// Returns false when the candidate array overflows (200).
pub fn lookup_tokens(ws: &mut Ws, wa: &Wa04, n: usize, flag: bool, lex: &Lexicon) -> bool {
    ws.set_i16(0, 0);
    for t in 0..n {
        let r = Wa04::rec(t);
        let forced = wa.u8(r + war::FORCED_POS);
        let preset = wa.cstr(r + war::PHONES).to_vec();
        let keep_case = wa.u8(r + war::CASE_MODE) == b'S';
        let mut buf = TokenBuf { b: wa.cstr(r + war::TEXT).to_vec() };
        loop {
            let piece = split_token_era(&mut buf, t == n - 1, forced, !preset.is_empty(), keep_case, lex, true);
            // The payload is strcpy'd into a local buffer.
            let raw = lex.lookup_payload(&piece).unwrap_or(&[]);
            let payload = &raw[..raw.iter().position(|&c| c == 0).unwrap_or(raw.len())];
            let count = ws.i16(0);
            if count > 199 {
                return false;
            }
            let prev = if count < 1 { -1 } else { ws.i16(Ws::cand(count as usize - 1) + cand::TOKEN) as i32 };
            fill_candidate_with(ws, count as usize, &piece, payload, t as i16, prev, forced, &preset, flag, &CODEBOOK);
            ws.set_i16(0, count + 1);
            if buf.b.first().copied().unwrap_or(0) == 0 {
                break;
            }
        }
    }
    true
}
