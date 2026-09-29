//! `FUN_1003d710`: VTML markup (between punctuation and the normalizer
//! chain). Not yet ported; declines.
//!
//! Read the decompile (860 bytes) far enough to map its shape: it requires
//! class==4 (tag marker) and len==1, `strstr`s forward for the matching
//! `</V ` close (`DAT_10069b74`), skips leading whitespace inside the tag,
//! then dispatches on the tag body's *first letter* (`*(char*)(param_2+0x1c)`,
//! i.e. `tok.text()[0]`) to one of ~17 sub-handlers, each itself substantial:
//! `FUN_1003da70`(A/H, 572B), `FUN_1003dcb0`(B/M/N, 2946B!), `FUN_1003e840`(C,
//! 517B), `FUN_1003ea50`(D, 2888B!), `FUN_1003f5a0`(E/K, 536B), `FUN_1003f7c0`
//! (F, 442B), `FUN_1003f980`(G, 447B), `FUN_1003fb40`(I, 438B), `FUN_1003fd00`
//! (J, 1407B), `FUN_10040280`(L, 505B), `FUN_10040480`(O, 1689B),
//! `FUN_10040b20`(P, 592B), `FUN_10040d70`(Q, 924B), `FUN_10041110`(R, 653B),
//! `FUN_100413a0`(T, 655B), `FUN_10041630`(U, 973B), `FUN_10041a00`(S, 352B).
//! Each handler returns -1 (not this tag, keep scanning other letters is NOT
//! how it works -- only one letter matches per dispatch) or a byte count (0 =
//! WA full -> stop sentence). This is a whole separate subsystem (likely
//! `<A.../>` audio, `<Break/>`, `<Cardinal>`, `<Date>`, `<Emphasis>`,
//! `<Fraction>`, ... `<Say-as>`/`<Sub>`/`<Time>`/`<Unit>` -- guessing from
//! initials only, NOT verified) comparable in scope to the whole symbols
//! family; porting it "exactly" needs each of those ~17 functions read and
//! translated individually, which this session's budget did not allow.
//! Declining rather than guessing at any of them.

use super::tokenize::Tok;
use super::FeSpa;
use crate::ling_spa::WaSpa;

pub fn vtml(_fe: &FeSpa, _wa: &mut WaSpa, _tok: &mut Tok, _t: &[u8], _p: usize, _off: i32) -> i32 {
    -1
}
