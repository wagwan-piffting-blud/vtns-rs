//! `FUN_10034270` (chain slot 9): a lone operator/comparison symbol left
//! over after `norm_math.rs` -- `=` ("igual"), `+` ("m\xe1s", or "m\xe1s
//! menos" when glued to a following `-` -- `FUN_100258f0`/`append_word`
//! appends onto the already-nonempty "m\xe1s" buffer, it does not
//! overwrite it), and `-` (range/expression hyphens: "menos", "gui\xf3n",
//! or "a", depending on what surrounds it).
//!
//! `local_320`/`local_32c`/`local_324` in the decompile are `param_2`
//! (the driver's current token) re-read as `text[0]`/`len`/`class`; the
//! stack blocks named `tokN`/`tokN2`/`tokN3`/`tokN4`/`tokP`/`tokPP` below
//! are the engine's `local_400`/`local_3bc`/`local_140`/`local_180`/
//! `local_37c`/`local_2fc` token buffers.
//!
//! `/` (`FUN_10034270`'s third arm) reads a unit after a number ("5/hora")
//! as "por hora", and a slash after a unit word as "por", looking units up
//! in the three `FUN_100338b0` groups, then `FUN_10033990`.

use super::abbrev;
use super::emit::emit_split;
use super::numwords::{append_word, is_comma_grouped, is_period_grouped, strip_digits};
use super::tokenize::{tokenize, Tok};
use super::{rec, FeSpa};
use crate::ling_spa::lexicon::stricmp_spa;
use crate::ling_spa::WaSpa;

const SRC: u32 = 0x12;

const MENOS: &[u8] = b"menos";
/// `DAT_10067e4c`/`DAT_10067e50`.
const GUION: &[u8] = b"gui\xf3n";
/// `DAT_10069230`.
const MAS: &[u8] = b"m\xe1s";
const IGUAL: &[u8] = b"igual";
/// `s_barra_10069470`.
const GUION_BARRA: &[u8] = b"barra";
/// `_DAT_10064530`, spelled out via `CONCAT22` in the decompile.
const A_WORD: &[u8] = b"a";

/// `DAT_1006ac80` (5 entries, `FUN_10043860`): $, EUR, florin, GBP, yen.
const CURRENCY_KEYS: [u8; 5] = [0x24, 0x80, 0x83, 0xa3, 0xa5];
/// `DAT_10079698` (3 entries): 1/4, 1/2, 3/4 glyphs.
const FRACTION_KEYS: [u8; 3] = [0xbc, 0xbd, 0xbe];
/// `s______xX_10069460`.
const XX_SET: &[u8] = b"-+*/=xX";
/// `s_______10069254` (also `norm_math::FOLLOW_OPS`).
const FOLLOW_OPS: &[u8] = b"+-*/=";
/// `s______<>______10069450`.
const PUNCT_SET: &[u8] = b",:'()<>\".?!;";
/// `s________10069468`.
const SET_468: &[u8] = b",;%$#~";
/// `DAT_1006944c`: two raw high bytes, not printable Spanish text.
const SENTINEL_1006944C: &[u8] = &[0xa2, 0xfe];

fn c0(t: &Tok) -> u8 {
    t.text().first().copied().unwrap_or(0)
}

fn in_set(set: &[u8], c: u8) -> bool {
    c != 0 && set.contains(&c)
}

/// `FUN_1004c4b0 || FUN_1004c5d0`.
fn grouped(text: &[u8]) -> bool {
    is_period_grouped(text) || is_comma_grouped(text)
}

fn is_currency_key(c: u8) -> bool {
    c != 0 && CURRENCY_KEYS.contains(&c)
}

fn is_fraction_key(c: u8) -> bool {
    c != 0 && FRACTION_KEYS.contains(&c)
}

fn class_9_3_8(c: i32) -> bool {
    matches!(c, 9 | 3 | 8)
}

/// `FUN_100505b3`/`FUN_10050528`: the `this` thiscall slot is dead (the
/// callee immediately overwrites it from a ctype table), so this is a
/// plain `atoi` -- verified safe to call on the all-digit output of
/// `strip_digits` (same conclusion `numwords.rs`'s own `atoi` doc comment
/// already draws for this address).
fn atoi_digits(s: &[u8]) -> i32 {
    let mut v: i32 = 0;
    for &c in s {
        if !c.is_ascii_digit() {
            break;
        }
        v = v.wrapping_mul(10).wrapping_add((c - b'0') as i32);
    }
    v
}

/// `FUN_1004bd70` generalized to `n` words (`punct::prev_word` is the same
/// engine routine specialized to `n == 1`). Returns the grabbed text and
/// how many bytes back its start is from `cur`.
fn prev_words(t: &[u8], start: usize, cur: usize, n: i32) -> (Vec<u8>, i32) {
    if cur == start {
        return (Vec::new(), 0);
    }
    let start_i = start as isize;
    let mut p = cur as isize;
    let mut skipped: i32 = 0;
    let word_start;
    loop {
        p -= 1;
        let c = t[p as usize];
        if !matches!(c, b' ' | 9 | 10 | 13) {
            if p <= start_i {
                return (t[p as usize..p as usize + 1].to_vec(), 1 + skipped);
            }
            word_start = p;
            break;
        }
        if p <= start_i {
            return (Vec::new(), 0);
        }
        skipped += 1;
    }
    p = word_start;
    let max_chars = n * 0x1e - 1;
    let mut count: i32 = 1;
    let mut word_count: i32 = 1;
    loop {
        let c = t[p as usize];
        let stop;
        if matches!(c, b' ' | 9 | 10 | 13) {
            if max_chars <= count || n <= word_count {
                p += 1;
                count -= 1;
                stop = true;
            } else {
                word_count += 1;
                stop = false;
            }
        } else if max_chars <= count {
            p += 1;
            count -= 1;
            stop = true;
        } else {
            stop = false;
        }
        if stop {
            break;
        }
        count += 1;
        p -= 1;
        if !(start_i < p) {
            break;
        }
    }
    (t[p as usize..p as usize + count as usize].to_vec(), count + skipped)
}

/// `FUN_1004bfe0` (mode 0): tokenizes all of `text`, returning the last
/// token and the one before it (skipping only empty tokens).
fn last_two_tokens(text: &[u8], off: i32) -> (Tok, Tok) {
    let mut prev = Tok::default();
    let mut last = Tok::default();
    if text.is_empty() || text[0] == 0 {
        return (prev, last);
    }
    let total = text.len() as i32;
    let mut b = text.to_vec();
    b.extend_from_slice(&[0u8; 64]);
    let mut used: i32 = 0;
    loop {
        let (tk, _) = tokenize(&b, used as usize, off + used, 0);
        if used + tk.consumed < total && tk.len != 0 {
            prev = tk.clone();
        }
        used += tk.consumed;
        last = tk;
        if used >= total {
            break;
        }
    }
    (prev, last)
}

/// The "menos"/"gui\xf3n" split shared by both `-`-and-a-following-number
/// shapes: tight (`gate_ws < 3`) is required at all, and no gap at all
/// (`menos`) upgrades it from "gui\xf3n" to "menos".
fn range_word(gate_ws: i32, menos: bool) -> Vec<u8> {
    if gate_ws < 3 {
        if menos { MENOS.to_vec() } else { GUION.to_vec() }
    } else {
        Vec::new()
    }
}

/// `LAB_10034c07`..`goto LAB_100352b3`: marks the previous WA record's
/// `PUNCT` field and never sets a word.
fn rec_check(wa: &mut WaSpa, cur: &Tok, prev: &Tok, tok_n: &Tok) {
    let n = wa.i16(0);
    if n < 1 {
        return;
    }
    let r = WaSpa::rec(n as usize - 1);
    let end_matches = wa.i32(r + rec::END) == prev.end;
    let clean = end_matches
        && cur.ws >= 1 && cur.ws <= 2
        && tok_n.class == 1
        && tok_n.ws >= 1 && tok_n.ws <= 2;
    if clean {
        wa.set_i32(r + rec::PUNCT, 1);
    } else if end_matches && cur.ws == 0 && tok_n.ws == 0 {
        wa.set_i32(r + rec::PUNCT, 7);
    }
}

/// The other `PUNCT`-only marker, inside the `tokP.class == 1` arm
/// (lines 434-452 of the decompile).
fn rec_check2(wa: &mut WaSpa, cur: &Tok, prev: &Tok, tok_n: &Tok) {
    let n = wa.i16(0);
    if n < 1 {
        return;
    }
    let r = WaSpa::rec(n as usize - 1);
    let end_matches = wa.i32(r + rec::END) == prev.end;
    if end_matches
        && prev.class == 1
        && cur.ws > 0 && cur.ws < 3
        && (tok_n.class == 1 || tok_n.class == 2)
        && tok_n.ws > 0 && tok_n.ws < 3
    {
        wa.set_i32(r + rec::PUNCT, 1);
    } else if end_matches && cur.ws == 0 && tok_n.ws == 0 {
        wa.set_i32(r + rec::PUNCT, 7);
    }
}

/// `tokP.class == 1` arm (lines 399-473): tokP is a word, not a number.
/// The nesting (verified against the decompile's own indentation, not just
/// its brace count) is `if COND_405 { if COND_410_417 { if COND_418 { if
/// COND_422 {"gui\xf3n"} else {rec_check2} } else {"menos"} } else
/// {"menos"} } else {"gui\xf3n"}`: a failing COND_405 (a unit word
/// right before) falls out to "gui\xf3n", the inner gates to "menos", and
/// COND_422 gives "gui\xf3n" or silence.
fn minus_word_num(wa: &mut WaSpa, cur: &Tok, prev_prev: &Tok, prev: &Tok, tok_n: &Tok) -> Vec<u8> {
    // Lines 400-403: an unconditional shortcut ahead of COND_405.
    if cur.ws > 0 && grouped(tok_n.text()) && tok_n.ws == 0 {
        return MENOS.to_vec();
    }

    // COND_405 (405-408).
    let n = wa.i16(0);
    let end_matches = n > 0 && wa.i32(WaSpa::rec(n as usize - 1) + rec::END) == prev.end;
    let src_is_10 = n > 0 && wa.i32(WaSpa::rec(n as usize - 1) + rec::SRC) == 10;
    let cond_405 =
        !(end_matches && src_is_10 && cur.ws <= 2 && grouped(tok_n.text()) && tok_n.ws <= 2);
    // A unit word (src 10) right before: falls out to `DAT_10067e4c`.
    if !cond_405 {
        return GUION.to_vec();
    }

    // COND_410_417 (410-417): the "1E-5" scientific-notation shape.
    let cond_410_417 = prev_prev.len < 1
        || !grouped(prev_prev.text())
        || prev.len != 1
        || !matches!(c0(prev), b'E' | b'e')
        || cur.ws > 1
        || !grouped(tok_n.text())
        || tok_n.ws > 1;
    if !cond_410_417 {
        return MENOS.to_vec();
    }

    // COND_418 (418-421).
    let cond_418 = prev.class == 1 || !grouped(tok_n.text()) || tok_n.ws != 0;
    if !cond_418 {
        return MENOS.to_vec();
    }

    // COND_422 (422-428): tight ",;%$#~" context.
    let cond_422 = prev.len == 1
        && c0(prev) != 0 && in_set(SET_468, c0(prev))
        && cur.ws > 0 && cur.ws < 3
        && grouped(tok_n.text())
        && tok_n.ws > 0 && tok_n.ws < 3;
    if cond_422 {
        GUION.to_vec()
    } else {
        rec_check2(wa, cur, prev, tok_n);
        Vec::new()
    }
}

struct MinusCtx<'a> {
    cur: &'a Tok,
    prev_prev: &'a Tok,
    prev: &'a Tok,
    tok_n: &'a Tok,
    tok2: &'a Tok,
}

/// `LAB_10035217` (lines 378-394): compare the two numbers' digit-string
/// magnitudes; `tokP < tokN` reads the hyphen as "a" (a range), otherwise
/// nothing overrides the caller's default.
fn minus_35217(ctx: &MinusCtx) -> Option<Vec<u8>> {
    if ctx.prev.text().contains(&b'.') || ctx.tok_n.text().contains(&b'.') {
        return None;
    }
    let a = atoi_digits(&strip_digits(ctx.prev.text()));
    let b = atoi_digits(&strip_digits(ctx.tok_n.text()));
    if a < b {
        Some(A_WORD.to_vec())
    } else {
        None
    }
}

/// `LAB_1003511b` (lines 356-377): the same shape as `minus_fd1`, tested
/// against `tok2` itself (its own `ws` field doubles as the "prev" ws).
fn minus_1011b(ctx: &MinusCtx) -> Option<Vec<u8>> {
    if ctx.tok2.len == 0 {
        return minus_35217(ctx);
    }
    if ctx.tok2.len < 1 || ctx.tok2.class == 2 || in_set(FOLLOW_OPS, c0(ctx.tok2)) {
        return None;
    }
    if ctx.tok2.class != 1 {
        let c = c0(ctx.tok2);
        let hit = in_set(PUNCT_SET, c);
        let eq = ctx.tok2.text() == SENTINEL_1006944C;
        let take = (class_9_3_8(ctx.tok2.class) && (hit || eq) && ctx.tok2.ws > -1)
            || ((!class_9_3_8(ctx.tok2.class) || (!hit && !eq)) && ctx.tok2.ws > 0);
        if !take {
            return None;
        }
        return minus_35217(ctx);
    }
    if ctx.tok2.ws < 1 {
        return None;
    }
    minus_35217(ctx)
}

/// `LAB_10034fd1` (lines 332-355): same shape, tested against `tokPP`.
fn minus_fd1(ctx: &MinusCtx) -> Option<Vec<u8>> {
    if ctx.cur.ws > 1 || ctx.tok_n.ws > 1 || ctx.cur.ws != ctx.tok_n.ws {
        return None;
    }
    if ctx.prev_prev.len != 0 {
        if ctx.prev_prev.len < 1
            || ctx.prev_prev.class == 2
            || in_set(FOLLOW_OPS, c0(ctx.prev_prev))
        {
            return None;
        }
        if ctx.prev_prev.class != 1 {
            let c = c0(ctx.prev_prev);
            let hit = in_set(PUNCT_SET, c);
            let eq = ctx.prev_prev.text() == SENTINEL_1006944C;
            let take = (class_9_3_8(ctx.prev_prev.class) && (hit || eq) && ctx.prev.ws > -1)
                || ((!class_9_3_8(ctx.prev_prev.class) || (!hit && !eq)) && ctx.prev.ws > 0);
            if !take {
                return None;
            }
            return minus_1011b(ctx);
        }
        if ctx.prev.ws < 1 {
            return None;
        }
    }
    minus_1011b(ctx)
}

/// Lines 316-331: `tokPP` is a lone x/X-ish glyph glued symmetrically
/// around `tokP`/cur/`tokN`.
fn minus_tail(ctx: &MinusCtx) -> Option<Vec<u8>> {
    let cond_xx = ctx.prev_prev.len == 1
        && c0(ctx.prev_prev) != 0
        && in_set(XX_SET, c0(ctx.prev_prev))
        && ctx.cur.ws == ctx.prev.ws
        && ctx.cur.ws == ctx.tok_n.ws;

    if cond_xx {
        if ctx.cur.ws >= 2 {
            return None;
        }
        let skip_to_fd1 = ctx.tok2.len != 0 && (ctx.tok2.len < 1 || ctx.tok2.class == 2);
        if !skip_to_fd1 {
            if c0(ctx.prev_prev) != b'-' || ctx.cur.ws != 0 {
                return Some(MENOS.to_vec());
            }
            return None;
        }
    }
    minus_fd1(ctx)
}

/// `LAB_10034de8` (lines 287-310): `tok2` is a grouped number glued
/// symmetrically; a following bare `/` plus a tight grouped number
/// upgrades "gui\xf3n" to "menos".
fn minus_de8(t: &[u8], cur_ws: i32, tok_n_ws: i32, tok2: &Tok, pos: usize, off: i32) -> Option<Vec<u8>> {
    if !grouped(tok2.text()) || cur_ws != tok_n_ws || tok2.ws >= 2 {
        return None;
    }
    let (tok_n3, r3) = tokenize(t, pos, off, 0);
    if tok_n3.len == 1 && c0(&tok_n3) == b'/' && tok_n3.ws == 0 {
        let (tok_n4, _) = tokenize(t, pos + r3 as usize, off + r3, 0);
        if tok_n4.len > 0 && grouped(tok_n4.text()) && tok_n4.ws == 0 {
            return Some(MENOS.to_vec());
        }
    }
    Some(GUION.to_vec())
}

/// `tokP.class == 2` arm's `tokN.class == 2` sub-case (lines 261-394):
/// `None` means "no override", so the caller applies the "gui\xf3n"
/// default.
fn minus_num_num_digits(
    cur: &Tok, prev_prev: &Tok, prev: &Tok, tok_n: &Tok,
    t: &[u8], pos_after_n: usize, off_after_n: i32,
) -> Option<Vec<u8>> {
    if !grouped(prev.text()) || !grouped(tok_n.text()) {
        return None;
    }
    let (tok2, r2) = tokenize(t, pos_after_n, off_after_n, 0);
    let pos2 = pos_after_n + r2 as usize;
    let off2 = off_after_n + r2;

    if tok2.len == 1 {
        if in_set(XX_SET, c0(&tok2)) && cur.ws == tok_n.ws && cur.ws == tok2.ws && tok2.ws < 2 {
            return Some(MENOS.to_vec());
        }
        if tok2.class == 6 && cur.ws == tok_n.ws && tok2.ws < 2 && is_fraction_key(c0(&tok2)) {
            return Some(MENOS.to_vec());
        }
        if let Some(w) = minus_de8(t, cur.ws, tok_n.ws, &tok2, pos2, off2) {
            return Some(w);
        }
    } else if tok2.len > 0 {
        if let Some(w) = minus_de8(t, cur.ws, tok_n.ws, &tok2, pos2, off2) {
            return Some(w);
        }
    }

    let ctx = MinusCtx { cur, prev_prev, prev, tok_n, tok2: &tok2 };
    minus_tail(&ctx)
}

/// `tokP.class == 2` arm (lines 225-398).
fn minus_num_num(
    wa: &mut WaSpa, cur: &Tok, prev_prev: &Tok, prev: &Tok, tok_n: &Tok,
    t: &[u8], pos_after_n: usize, off_after_n: i32,
) -> Vec<u8> {
    // A lone high-byte glyph after the hyphen forces the rec-check path.
    // The engine's own "menos" shortcut for it
    // (`FUN_10043860(&DAT_10079698, 0x2d, 3)`) is dead: verified in the
    // disassembly at 0x10034be4 the search key pushed is the literal byte
    // '-', never one of the table's 1/4-1/2-3/4 keys, so the search always
    // misses and this always falls through here.
    let force_rec_check = tok_n.len == 1 && tok_n.class == 6;
    if force_rec_check || tok_n.class != 2 {
        rec_check(wa, cur, prev, tok_n);
        return Vec::new();
    }
    minus_num_num_digits(cur, prev_prev, prev, tok_n, t, pos_after_n, off_after_n)
        .unwrap_or_else(|| GUION.to_vec())
}

/// viol09 `FUN_100310a0`, `tokP.class == 2` arm. `tokN.class != 2` takes the
/// same rec-check as 2014; a non-grouped number is silent (2014: "gui\xf3n"),
/// the digit logic is shorter, and the "a" word only follows a literal "por".
fn minus_num_num09(
    wa: &mut WaSpa, cur: &Tok, prev_prev: &Tok, prev: &Tok, tok_n: &Tok,
    t: &[u8], pos_after_n: usize, off_after_n: i32,
) -> Vec<u8> {
    if tok_n.class != 2 {
        rec_check(wa, cur, prev, tok_n);
        return Vec::new();
    }
    if !grouped(prev.text()) || !grouped(tok_n.text()) {
        return Vec::new();
    }
    let (tok2, r2) = tokenize(t, pos_after_n, off_after_n, 0);
    let go_c24 = if tok2.len == 1 {
        if in_set(XX_SET, c0(&tok2)) && cur.ws == tok_n.ws && cur.ws == tok2.ws && tok2.ws < 2 {
            return MENOS.to_vec();
        }
        if tok2.class == 6 && cur.ws == tok_n.ws && tok2.ws < 2 && is_fraction_key(c0(&tok2)) {
            return MENOS.to_vec();
        }
        true
    } else {
        tok2.len > 0
    };
    if go_c24 && grouped(tok2.text()) && cur.ws == tok_n.ws && tok2.ws <= 1 {
        let pos2 = pos_after_n + r2 as usize;
        let off2 = off_after_n + r2;
        let (tok_n3, r3) = tokenize(t, pos2, off2, 0);
        if tok_n3.len == 1 && c0(&tok_n3) == b'/' && tok_n3.ws == 0 {
            let (tok_n4, _) = tokenize(t, pos2 + r3 as usize, off2 + r3, 0);
            if tok_n4.len > 0 && grouped(tok_n4.text()) && tok_n4.ws == 0 {
                return MENOS.to_vec();
            }
        }
        return GUION.to_vec();
    }
    if prev_prev.len == 1 {
        if c0(prev_prev) != 0
            && in_set(XX_SET, c0(prev_prev))
            && cur.ws == prev.ws
            && cur.ws == tok_n.ws
            && cur.ws < 2
            && tok2.len > 0
            && tok2.class != 2
        {
            return MENOS.to_vec();
        }
    } else if prev_prev.len < 1 {
        return GUION.to_vec();
    }
    if stricmp_spa(b"por", prev_prev.text()) != 0 {
        GUION.to_vec()
    } else {
        A_WORD.to_vec()
    }
}

/// The `/` dispatch (lines 84-153). `None` means the trivial early return
/// (line 128-131: a bare word/word split with nothing glued, silently
/// consumed); `Some` is always a non-empty word ("barra" folded in as the
/// shared default when neither abbreviation check fires).
fn slash(wa: &WaSpa, cur: &Tok, t: &[u8], p: usize, off: i32) -> Option<(Vec<u8>, i32)> {
    let (grabbed, back) = prev_words(t, p - off as usize, p, 1);
    let prev = if back != 0 { last_two_tokens(&grabbed, off - back).1 } else { Tok::default() };

    let pos_n = p + cur.consumed as usize;
    let off_n = off + cur.consumed;
    let (tok_n, r_n) = tokenize(t, pos_n, off_n, 0);
    let pos_after_n = pos_n + r_n as usize;
    let off_after_n = off_n + r_n;

    if prev.len >= 1 {
        if prev.class == 1 && cur.ws == 0 && tok_n.class == 1 && tok_n.ws == 0 {
            return None;
        }
        // "5/hora": "por hora", the unit consumed with the slash.
        if prev.class == 2 && cur.ws <= 1 && tok_n.class == 1 && tok_n.ws <= 1 {
            let mut name = Vec::new();
            let mut cursor = tok_n.consumed;
            if unit_name(&mut name, tok_n.text(), t, pos_n, off_n, &mut cursor) {
                let mut word = b"por ".to_vec();
                word.extend_from_slice(&name);
                return Some((word, r_n));
            }
            return Some((GUION_BARRA.to_vec(), 0));
        }
    }
    let n = wa.i16(0);
    let gate = n > 0
        && wa.i32(WaSpa::rec(n as usize - 1) + rec::END) == cur.start
        && wa.i32(WaSpa::rec(n as usize - 1) + rec::SRC) == 10
        && cur.ws == 0
        && tok_n.len > 0
        && tok_n.ws == 0
        && grouped(tok_n.text());
    if gate {
        let (tok2, _) = tokenize(t, pos_after_n, off_after_n, 0);
        let mut name = Vec::new();
        let mut cursor = tok2.consumed;
        if tok2.len > 0 && unit_name(&mut name, tok2.text(), t, pos_after_n, off_after_n, &mut cursor) {
            return Some((b"por".to_vec(), 0));
        }
    }
    Some((GUION_BARRA.to_vec(), 0))
}

/// `FUN_100338b0` over the three groups (singular), then `FUN_10033990`.
fn unit_name(out: &mut Vec<u8>, word: &[u8], t: &[u8], pos: usize, off: i32, cursor: &mut i32) -> bool {
    for g in 0..3 {
        if abbrev::group_lookup(g, Some(&mut *out), word, 0, None) {
            return true;
        }
    }
    abbrev::unit_lookup(Some(out), word, t, pos, off, cursor, None)
}

/// The `-` dispatch (lines 154-480 minus the leading class check).
fn minus(wa: &mut WaSpa, cur: &Tok, t: &[u8], p: usize, off: i32, era09: bool) -> Option<Vec<u8>> {
    let (grabbed, back) = prev_words(t, p - off as usize, p, 2);
    let (prev_prev, prev) = if back != 0 {
        last_two_tokens(&grabbed, off - back)
    } else {
        (Tok::default(), Tok::default())
    };

    let (tok_n, r_n) = tokenize(t, p + cur.consumed as usize, off + cur.consumed, 0);
    if r_n == 0 {
        return None;
    }
    let pos_after_n = p + cur.consumed as usize + r_n as usize;
    let off_after_n = off + cur.consumed + r_n;

    let word = if tok_n.len == 1 && tok_n.ws < 3 && is_currency_key(c0(&tok_n)) {
        let (tok_n2, _) = tokenize(t, pos_after_n, off_after_n, 0);
        if tok_n2.len > 0 && tok_n2.class == 2 {
            range_word(tok_n2.ws, tok_n.ws == 0)
        } else {
            Vec::new()
        }
    } else if prev.len < 1 {
        if tok_n.len > 0 && tok_n.class == 2 && grouped(tok_n.text()) {
            range_word(tok_n.ws, tok_n.ws == 0)
        } else {
            Vec::new()
        }
    } else if prev.class == 2 && era09 {
        minus_num_num09(wa, cur, &prev_prev, &prev, &tok_n, t, pos_after_n, off_after_n)
    } else if prev.class == 2 {
        minus_num_num(wa, cur, &prev_prev, &prev, &tok_n, t, pos_after_n, off_after_n)
    } else {
        minus_word_num(wa, cur, &prev_prev, &prev, &tok_n)
    };

    Some(word)
}

pub fn numexpr(fe: &FeSpa, wa: &mut WaSpa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let cur = tok.clone();
    let is_op = cur.len == 1 && cur.class == 3;
    let ch = c0(&cur);
    let mut consumed = cur.consumed;

    let word: Vec<u8> = if is_op && ch == b'=' {
        IGUAL.to_vec()
    } else if is_op && ch == b'+' {
        let mut w = MAS.to_vec();
        let (next, r) = tokenize(t, p + consumed as usize, off + consumed, 0);
        if next.len == 1 && next.class == 3 && c0(&next) == b'-' && next.ws == 0 {
            append_word(&mut w, MENOS);
            consumed += r;
        }
        w
    } else if is_op && ch == b'/' {
        match slash(wa, &cur, t, p, off) {
            Some((w, extra)) => {
                consumed += extra;
                w
            }
            None => return consumed,
        }
    } else if is_op && ch == b'-' {
        match minus(wa, &cur, t, p, off, fe.era.is_2009()) {
            Some(w) => w,
            None => return -1,
        }
    } else {
        return -1;
    };

    if word.is_empty() {
        return consumed;
    }
    if emit_split(wa, &word, cur.start, off + consumed, b'S', b'D', SRC) {
        consumed
    } else {
        0
    }
}
