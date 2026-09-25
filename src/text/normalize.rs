//! `fe_NormalizeTokens` (0x1003d3d0): turn one sentence of processed text into
//! the WA by offering each token to the normalizers in the engine's fixed order.
//!
//! Every normalizer has the engine's contract: `> 0` consumed that many bytes,
//! `0` stop the sentence here, `< 0` not mine (the driver rolls the WA back to
//! the count before the attempt and clears that record). The punctuation
//! normalizer additionally returns `-n` to skip `n` bytes.

use super::norm;
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::Fe;

type Norm = fn(&Fe, &mut Wa, &mut Tok, &[u8], usize, i32) -> i32;

/// The chain after the URL normalizer, in engine order; all share the
/// `joined_r0x1003d772` continuation.
const AFTER_URL: [Norm; 7] = [norm::currency, norm::date, norm::time, norm::phone, norm::digits_a, norm::digits_b, norm::units];

/// Normalize the sentence starting at `t[p0..]`. Returns 1 when words were
/// produced, -1 otherwise (the engine's `((n < 1) - 1 & 2) - 1`).
pub fn normalize_tokens(fe: &Fe, wa: &mut Wa, t: &[u8], p0: usize) -> i32 {
    let mut p = p0;
    let mut mark: i16 = 0;
    let mut tok = Tok::default();
    let finish = |wa: &Wa| if wa.n() < 1 { -1 } else { 1 };
    loop {
        let s1 = wa.n();
        let i7 = s1 as i32;
        let off = (p - p0) as i32;
        wa.set_consumed(off);

        if wa.i16(0xc) != 0 {
            let r = norm::tagmode_url(fe, wa, t, p, off);
            if r < 0 {
                wa.restore(s1);
                wa.clear_rec(i7);
            } else {
                if r == 0 {
                    wa.restore(s1);
                    wa.clear_rec(i7);
                    return finish(wa);
                }
                if mark != 0 && mark < wa.n() {
                    let st = wa.ri32(mark as i32, 0);
                    wa.restore(s1);
                    wa.clear_rec(i7);
                    let d = st - off;
                    wa.set_consumed(if d < 1 { 0 } else { d } + off);
                    if wa.i16(0xc) == 0 {
                        wa.set_i16(0xc, 1);
                    }
                    return finish(wa);
                }
                if wa.i16(0xc) == 0 {
                    p += r as usize;
                    continue;
                }
                p += r as usize;
                if mark != 0 {
                    return finish(wa);
                }
                mark = wa.n();
                if mark < 1 {
                    return finish(wa);
                }
                continue;
            }
        }

        // LAB_1003d477
        tokenize(&mut tok, t, p, off, 0, fe.ud.as_ref());
        if tok.nl == 1 {
            let n = wa.n();
            if n > 0 && wa.ri32(n as i32 - 1, 0x2c) == 0 {
                wa.set_ri32(n as i32 - 1, 0x2c, 1);
            }
        }
        if (1..=3).contains(&tok.class) {
            let r = norm::userdict(fe, wa, t, p, off, 0);
            if r >= 0 {
                match accept(wa, r, &mut p, mark, s1, i7, off) {
                    Step::Continue => continue,
                    Step::Return => return finish(wa),
                }
            }
            wa.restore(s1);
            wa.clear_rec(i7);
        }

        // LAB_1003d542
        let r = norm::punct_runs(fe, wa, &mut tok, t, p, off);
        if r > 0 {
            if mark == 0 {
                wa.set_consumed(off);
                if tok.consumed != 0 {
                    p += r as usize;
                    wa.set_consumed(off + r);
                }
                mark = wa.n();
                if mark < 1 {
                    return finish(wa);
                }
                continue;
            }
            if tok.consumed == 0 {
                wa.restore(s1);
                wa.clear_rec(i7);
                wa.set_consumed(off);
                return finish(wa);
            }
            p += r as usize;
            continue;
        }
        if r < 0 {
            p += (-r) as usize;
            continue;
        }
        wa.restore(s1);
        wa.clear_rec(i7);

        let r = norm::dispatch(fe, wa, &mut tok, t, p, off);
        if r >= 0 {
            if r == 0 {
                wa.restore(s1);
                wa.clear_rec(i7);
                return finish(wa);
            }
            if mark != 0 && mark < wa.n() {
                wa.restore(s1);
                wa.clear_rec(i7);
                wa.set_consumed(off);
                return finish(wa);
            }
            p += r as usize;
            continue;
        }
        wa.restore(s1);
        wa.clear_rec(i7);

        let r = norm::symbols_cp(fe, wa, &mut tok, t, p, off);
        if r >= 0 {
            match accept(wa, r, &mut p, mark, s1, i7, off) {
                Step::Continue => continue,
                Step::Return => return finish(wa),
            }
        }
        wa.restore(s1);
        wa.clear_rec(i7);

        let r = norm::url(fe, wa, &mut tok, t, p, off);
        if r >= 0 {
            if r == 0 {
                wa.restore(s1);
                wa.clear_rec(i7);
                return finish(wa);
            }
            if mark == 0 || wa.n() <= mark {
                if wa.i16(0xc) == 0 {
                    p += r as usize;
                    continue;
                }
                p += r as usize;
                if mark != 0 {
                    return finish(wa);
                }
                mark = wa.n();
                if mark < 1 {
                    return finish(wa);
                }
                continue;
            }
            stop_at_mark(wa, mark, s1, i7, off);
            return finish(wa);
        }
        wa.restore(s1);
        wa.clear_rec(i7);

        let mut handled = false;
        for f in AFTER_URL.iter() {
            let r = f(fe, wa, &mut tok, t, p, off);
            if r >= 0 {
                match accept(wa, r, &mut p, mark, s1, i7, off) {
                    Step::Continue => {
                        handled = true;
                        break;
                    }
                    Step::Return => return finish(wa),
                }
            }
            wa.restore(s1);
            wa.clear_rec(i7);
        }
        if handled {
            continue;
        }

        // address (0x10033760) has its own continuation rule.
        let r = norm::address(fe, wa, &mut tok, t, p, off);
        if r >= 0 {
            if r == 0 {
                wa.restore(s1);
                wa.clear_rec(i7);
                return finish(wa);
            }
            if mark == 0 || wa.n() <= mark {
                p += r as usize;
            } else {
                let i6 = mark as i32;
                let n = wa.n() as i32;
                let keep = wa.ri32(i6 - 1, 0x28) == 0x12
                    && wa.ri32(i6 - 1, 0x2c) == 2
                    && wa.ri32(n - 1, 0x28) == 0x12
                    && !(wa.i16(Wa::rec(i6 - 1) + 0x1e) == 0x1f && wa.i16(Wa::rec(n - 1) + 0x1e) == 0x1f);
                if !keep {
                    stop_at_mark(wa, mark, s1, i7, off);
                    return finish(wa);
                }
                wa.set_ri32(i6 - 1, 0x2c, 0);
                p += r as usize;
                mark = 0;
            }
            continue;
        }
        wa.restore(s1);
        wa.clear_rec(i7);

        // fraction .. compound share the LAB_1003dcea continuation.
        let mut done = false;
        for k in 0..7 {
            let r = match k {
                0 => norm::fraction(fe, wa, &mut tok, t, p, off),
                1 => norm::math(fe, wa, &mut tok, t, p, off),
                2 => norm::numexpr(fe, wa, &mut tok, t, p, off),
                3 => norm::n15(fe, wa, &mut tok, t, p, off),
                4 => norm::numword(fe, wa, &mut tok, t, p, off),
                5 => norm::symbols(fe, wa, &mut tok, t, p, off, mark),
                _ => norm::compound(fe, wa, &mut tok, t, p, off),
            };
            if r >= 0 {
                if r == 0 {
                    wa.restore(s1);
                    wa.clear_rec(i7);
                    return finish(wa);
                }
                if mark == 0 || wa.n() <= mark {
                    p += r as usize;
                    done = true;
                    break;
                }
                stop_at_mark(wa, mark, s1, i7, off);
                return finish(wa);
            }
            wa.restore(s1);
            wa.clear_rec(i7);
        }
        if done {
            continue;
        }

        let r = norm::word(fe, wa, &mut tok, t, p, off);
        if r < 0 {
            wa.restore(s1);
            wa.clear_rec(i7);
            // The engine loops here; it never gets here because an earlier
            // normalizer always takes such tokens. Guard against the ones
            // this port does not have yet.
            if norm::UNPORTED_GUARD {
                p += tok.consumed.max(1) as usize;
            }
            continue;
        }
        if r == 0 {
            wa.restore(s1);
            wa.clear_rec(i7);
            return finish(wa);
        }
        if mark == 0 || wa.n() <= mark {
            p += r as usize;
            continue;
        }
        stop_at_mark(wa, mark, s1, i7, off);
        return finish(wa);
    }
}

enum Step {
    Continue,
    Return,
}

/// `joined_r0x1003d772`: the common continuation after a normalizer accepted.
fn accept(wa: &mut Wa, r: i32, p: &mut usize, mark: i16, s1: i16, i7: i32, off: i32) -> Step {
    if r == 0 {
        wa.restore(s1);
        wa.clear_rec(i7);
        return Step::Return;
    }
    if mark == 0 {
        *p += r as usize;
        return Step::Continue;
    }
    if mark < wa.n() {
        stop_at_mark(wa, mark, s1, i7, off);
        return Step::Return;
    }
    *p += r as usize;
    Step::Continue
}

/// `LAB_1003dee7`: a token after the sentence mark produced words: end the
/// sentence at the mark, consuming up to the start of the first extra word.
fn stop_at_mark(wa: &mut Wa, mark: i16, s1: i16, i7: i32, off: i32) {
    let st = wa.ri32(mark as i32, 0);
    wa.restore(s1);
    let d = st - off;
    wa.clear_rec(i7);
    wa.set_consumed(if d < 1 { 0 } else { d } + off);
}
