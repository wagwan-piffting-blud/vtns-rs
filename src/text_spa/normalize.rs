//! `FUN_10031060`: Violeta's `fe_NormalizeTokens`. Same contract as the English
//! driver (`text::normalize`): each normalizer returns `> 0` bytes consumed,
//! `0` stop the sentence, `< 0` not mine. Unlike English, a declined attempt
//! restores the word count but does not clear the record it may have written.

use super::norm_currency::currency;
use super::norm_date::date;
use super::norm_fraction::fraction;
use super::norm_math::math;
use super::norm_number::norm_number;
use super::norm_numexpr::numexpr;
use super::norm_phone::phone;
use super::norm_roman::norm_roman;
use super::norm_symbols::symbols;
use super::norm_symbols2::symbols2;
use super::norm_time::time;
use super::norm_units::units;
use super::norm_url::url;
use super::norm_vtml::vtml;
use super::punct::punct;
use super::tagmode::tag_mode;
use super::tokenize::{tokenize, Tok};
use super::userdict_spa::prepass;
use super::word::word;
use super::{rec, FeSpa, CONSUMED, TAG_MODE};
use crate::ling_spa::WaSpa;

pub type Norm = fn(&FeSpa, &mut WaSpa, &mut Tok, &[u8], usize, i32) -> i32;

/// The chain after VTML, in engine order: 0x100486b0 (url), 0x1002b410
/// (date), norm_time 0x10045e70 (time), 0x100289e0 (currency, declines --
/// see norm_currency.rs), phone 0x10044910, 0x100426b0 (symbols2),
/// 0x10031720 (units, number+word and bare-word shapes only -- see
/// norm_units.rs), 0x1002fd60 (fraction, "N/N" shape only -- see
/// norm_fraction.rs), 0x1002e6c0 (math), 0x10034270
/// (numexpr, declines -- see norm_numexpr.rs), 0x10033db0 (norm_roman),
/// 0x10042b20 (symbols), 0x10027e60 (norm_number), then the word handler
/// 0x10042180.
pub const CHAIN: [Norm; 13] = [url, date, time, currency, phone, symbols2, units, fraction, math, numexpr, norm_roman, symbols, norm_number];

fn restore(wa: &mut WaSpa, s1: i16) {
    wa.set_i16(0, s1);
    if s1 >= 1 {
        let e = wa.i32(WaSpa::rec(s1 as usize - 1) + rec::END);
        wa.set_i32(CONSUMED, e);
    }
}

fn finish(wa: &WaSpa) -> i32 {
    if wa.i16(0) < 1 {
        -1
    } else {
        1
    }
}

/// Stop with the sentence ending where the next word would start.
fn stop_at(wa: &mut WaSpa, mark: i16, s1: i16, off: i32) -> i32 {
    let st = wa.i32(WaSpa::rec(mark as usize) + rec::START);
    let d = st - off;
    wa.set_i16(0, s1);
    wa.set_i32(CONSUMED, off + if d < 1 { 0 } else { d });
    finish(wa)
}

/// `LAB_10031649`: stop here, the sentence ends at the last word.
fn stop(wa: &mut WaSpa, s1: i16) -> i32 {
    restore(wa, s1);
    finish(wa)
}

/// `LAB_10031665`: restore to `off` (not the marked record's start -- a
/// quirk distinct from `stop_at`, used only after punctuation's zero-width
/// token and after VTML's mark-crossing).
fn stop_here(wa: &mut WaSpa, s1: i16, off: i32) -> i32 {
    wa.set_i16(0, s1);
    wa.set_i32(CONSUMED, off);
    finish(wa)
}

/// Normalize the sentence at `t[p0..]`.
pub fn normalize_tokens(fe: &FeSpa, wa: &mut WaSpa, t: &[u8], p0: usize) -> i32 {
    crate::text::cstr::set_era(fe.era);
    let mut p = p0;
    let mut mark: i16 = 0;
    'outer: loop {
        let mut s1;
        let mut off;
        // Tag-mode consumption loop (`while (param_1[4] != 0)` at the top of
        // the driver's own `while(true)`).
        loop {
            s1 = wa.i16(0);
            off = (p - p0) as i32;
            wa.set_i32(CONSUMED, off);
            if wa.i16(TAG_MODE) == 0 {
                break;
            }
            let r = tag_mode(fe, wa, t, p, off);
            if r < 0 {
                restore(wa, s1);
                break;
            }
            if r == 0 {
                return stop(wa, s1);
            }
            if mark != 0 && mark < wa.i16(0) {
                // The next sentence picks the path up again in tag mode.
                if wa.i16(TAG_MODE) == 0 {
                    wa.set_i16(TAG_MODE, 1);
                }
                return stop_at(wa, mark, s1, off);
            }
            p += r as usize;
            if wa.i16(TAG_MODE) == 0 {
                continue;
            }
            if mark != 0 {
                return finish(wa);
            }
            mark = wa.i16(0);
            if mark < 1 {
                return finish(wa);
            }
        }

        let (mut tok, _) = tokenize(t, p, off, 0);
        if tok.nl == 1 && s1 > 0 {
            let r = WaSpa::rec(s1 as usize - 1) + rec::PUNCT;
            if wa.i32(r) == 0 {
                wa.set_i32(r, 1);
            }
        }

        // `FUN_10049e30` pre-pass: only for word/number/symbol tokens, and
        // only before punctuation.
        if matches!(tok.class, 1 | 2 | 3) {
            let r = prepass(fe, wa, t, p, off, 0);
            if r > 0 {
                if mark == 0 || mark >= wa.i16(0) {
                    p += r as usize;
                    continue 'outer;
                }
                return stop_at(wa, mark, s1, off);
            }
            if r == 0 {
                return stop(wa, s1);
            }
            restore(wa, s1);
        }

        let r = punct(fe, wa, &tok, t, p, off);
        if r > 0 {
            if mark == 0 {
                wa.set_i32(CONSUMED, off);
                if tok.consumed != 0 {
                    p += r as usize;
                    wa.set_i32(CONSUMED, off + r);
                }
                mark = wa.i16(0);
                if mark < 1 {
                    return finish(wa);
                }
                continue;
            }
            if tok.consumed == 0 {
                return stop_here(wa, s1, off);
            }
            p += r as usize;
            continue;
        }
        if r < 0 {
            p += (-r) as usize;
            continue;
        }

        restore(wa, s1);
        let rv = vtml(fe, wa, &mut tok, t, p, off);
        if rv >= 0 {
            if rv == 0 {
                return stop(wa, s1);
            }
            if mark != 0 && mark < wa.i16(0) {
                return stop_here(wa, s1, off);
            }
            p += rv as usize;
            continue;
        }

        restore(wa, s1);
        let mut r = -1;
        let mut slot = 0;
        for (i, f) in CHAIN.iter().chain(std::iter::once(&(word as Norm))).enumerate() {
            r = f(fe, wa, &mut tok, t, p, off);
            if r >= 0 {
                slot = i;
                break;
            }
            restore(wa, s1);
        }
        if r < 0 {
            continue;
        }
        if r == 0 {
            return stop(wa, s1);
        }
        if mark != 0 && mark < wa.i16(0) {
            return stop_at(wa, mark, s1, off);
        }
        p += r as usize;
        // Only the URL normalizer can leave tag mode on (a path follows).
        if slot == 0 && wa.i16(TAG_MODE) != 0 {
            if mark != 0 {
                return finish(wa);
            }
            mark = wa.i16(0);
            if mark < 1 {
                return finish(wa);
            }
        }
    }
}
