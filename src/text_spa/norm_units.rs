//! `FUN_10031720` (chain slot 6): units ("60 NM", "30 kt", "10 mi", very
//! common in NWS marine text).
//!
//! Ported: a number immediately followed by a unit-dictionary word ("60
//! nudos", "21 millas"); a unit-dictionary word appearing by itself with no
//! leading number (e.g. a bare "\xe1rea" -- confirmed a real, observed
//! pattern: the word-unit dictionary at `DAT_10072ec8` (`tables_oth::
//! UNIT_WORDS`, `tables_oth::unit_word`, transcribed via `pefile`, not by
//! hand -- see its own doc comment, including the confirmed-dead
//! `FUN_100338b0`/`DAT_10082060`/`64`/`68` word-dictionary cascade the
//! engine tries first) recognizes ordinary Spanish words like "hora"/
//! "minuto"/"segundo"/"pie"/"punto" as units in their own right, so this
//! chain slot claims them *whenever they appear as a bare word*, not only
//! after a number -- verified against a fresh capture (`probe/out/
//! units.jsonl`, "millas por hora": "hora" itself lands at `src=0xa`, not
//! the plain-word `0x1`)); and now the **range shape** ("2-4 pies", "80-85
//! grados", "20 a 25 nudos", "1-2 pulgadas") -- see [`range_scan`]/
//! [`render_range`] and their own doc comments for exactly what's ported
//! (`FUN_10033050`'s simple range branch, `FUN_10033620`'s dash/word-
//! conjunction scanner, `FUN_100324d0`'s per-sub-token renderer) and what
//! isn't (`FUN_10033500`'s comma-list branch, `FUN_10033620`'s own
//! `/`-between-numbers third shape).
//!
//! Not ported: the scanner's other shape families (multi-token compounds
//! like "kil\xf3metros por hora", parenthesised amounts, currency-linked
//! units, squared/cubed suffixes via `DAT_10070248`, the dedicated
//! Celsius/Fahrenheit temperature-symbol special case in `FUN_10032660`
//! (confirmed dead code, not just unported -- see below), and the
//! symbol/abbreviation shapes like "NM"/"mph"/"km/h" that don't literally
//! spell out a `UNIT_WORDS` key (also investigated this round -- see
//! below)) -- those shapes decline here exactly as the round-1 placeholder
//! did.
//!
//! `FUN_100321c0` (the commit function, read in full): every word it emits
//! uses `src=0xa` (confirmed literally, `10` at each `FUN_10035350`/
//! `FUN_100354d0` call site, and against captures: "60 NM"/"5 nudos"/
//! "80-85 grados"/"2-4 pies" all carry `src=0xa` on the leading number in
//! the real engine). Its own plural-flag write (`param_1[wordcount*0x54-
//! 0x45]=1`, the same `+0x12`-on-the-last-record pattern used throughout
//! this front end) is gated on a flag read from the initial-classification
//! helper `FUN_10033050` (unread) rather than a direct `FUN_1004c100`
//! call; ported here as `numwords::plural` on the raw number text, which
//! is the same rule `FUN_1004c100` itself implements everywhere else it's
//! called directly. `FUN_100324d0` (an alternate per-token renderer for a
//! *different*, undeciphered shape family) and `FUN_10032660` (the shape
//! validator run per candidate shape before commit, with its own
//! Celsius/Fahrenheit special case gated on `FUN_10041dd0` -- already
//! ported as `punct::sentence_end`) were both read in full, but neither is
//! reachable from the two shapes ported here (both bypass the scanner's
//! own shape-matching/validation machinery entirely), so `sentence_end`
//! did not need to be made `pub(super)` for this file after all.
//!
//! This round: the range shape above is now ported and verified against
//! fresh captures (it was the single biggest class of full-corpus
//! mismatches, per the batch's own mismatch-dump classification -- units
//! numbers tagged `src=0x2` by the fallback chain instead of `src=0xa`).
//! Several other findings from investigating the *remaining* gaps are worth
//! recording so the next agent doesn't redo this work:
//! - **The Celsius/Fahrenheit special case in `FUN_10032660` is dead code,
//!   confirmed, not just unreachable from the two shapes ported here.**
//!   Every branch of it (`case 0`'s `cVar2=='P'||'A'||'C'` and `case 1`'s
//!   `cVar2=='E' && next=='C'` / `cVar2=='C' && next=='P'`) requires a `'C'`
//!   or `'P'` per-token tag byte (`local_524`/the committed slot's own
//!   `+0x1a`). Grepping every assignment to that tag in `FUN_10031720`
//!   shows `'C'`/`'P'` are written *only* inside the `FUN_100338b0`
//!   three-table cascade (`DAT_10082060`/`64`/`68`) -- and this session
//!   re-verified independently (fresh `pefile` read, not trusting the prior
//!   agent's own note) that all three headers are still `0` in this DLL's
//!   static image. With that cascade dead, `'C'`/`'P'` can never be
//!   produced, so `FUN_10032660`'s temperature branches can never fire
//!   either: porting them faithfully is equivalent to not porting them.
//!   `FUN_10041dd0`/`punct::sentence_end` accordingly still doesn't need to
//!   be `pub(super)`.
//! - **The abbreviation table at `0x100702a0`** (`FUN_100438f0`, tag `'W'`,
//!   gated on `local_4f4==2`) is real -- unlike the `FUN_100338b0` cascade,
//!   this is a fixed address in `.data`, and `pefile` confirms `DAT_10079e2c`
//!   (its count) is `2`, not `0`. But it is *not* a general abbreviation
//!   dictionary: reading the table directly gives exactly two entries,
//!   `"cu"` -> dash-field `"3"` and `"sq"` -> dash-field `"2"` (a prefix
//!   table for English-style "cu"/"sq" unit qualifiers, e.g. "cu ft"/"sq
//!   mi" -- the dash field is fed through `FUN_100505b3`, confirmed to be a
//!   thin wrapper over `FUN_10050528`, a locale-aware `atoi`, so the
//!   `"3"`/`"2"` strings become the integer exponents 3/2, landing in the
//!   same per-slot "exponent" field `FUN_10030220`'s superscript handling
//!   in `norm_fraction.rs` reads). This does **not** explain "NM"/"kt"/
//!   "mph": neither string appears in this table, nor in `UNIT_WORDS`, nor
//!   anywhere reachable through this function's own live tag-assignment
//!   code. Whatever makes the real engine tag "NM"/"kt"/"mph"-adjacent
//!   numbers with `src=0xa` (confirmed still happening in a fresh capture:
//!   "60 NM" -> "sesenta" carries `src=0xa` in the engine but `0x2` from
//!   this port, i.e. some *other* normalizer's tag) most likely happens at
//!   the lexicon level (a pronunciation entry for the abbreviation itself,
//!   outside `text_spa` entirely) rather than in `FUN_10031720` -- a
//!   genuinely different subsystem, out of this file's scope regardless of
//!   effort spent here.
//! - **Ranges are now ported.** They go through a *pre-scan*:
//!   `FUN_10033050` (the initial-classification helper, called once at
//!   `FUN_10031720`'s own entry) tries `FUN_10033500` (a comma/"y"-list
//!   scanner, not ported -- see [`range_scan`]'s own doc) then
//!   `FUN_10033620` (the dash/en-dash/"a"/"y"/"&"/"al"-conjunction range
//!   scanner, ported as [`range_scan`]). When it succeeds, the whole matched
//!   span is re-tokenized and rendered word-by-word by `FUN_100324d0`
//!   (ported as [`render_range`]) -- `FUN_100321c0`'s own `FUN_10006fe0`
//!   against `DAT_1006905c` ("NUMS", confirmed via `pefile`) picks this
//!   function over a `FUN_1004ca30`-on-the-whole-span fallback used by
//!   `FUN_10033050`'s *other*, prev-word-continuation branch (also not
//!   ported -- a rarer shape, and using it without reading that branch's own
//!   struct-field semantics would be guessing). `FUN_1004ca30` itself was
//!   already available as `numwords::cardinal_to_words`, fully ported and
//!   verified independently of this work.
//! - The trailing unit word after a range is always marked plural
//!   (`FUN_100321c0`'s own `*(short*)(param_3+0x18)!=0` check, unconditional
//!   whenever `FUN_10033050`'s simple range branch fired -- confirmed via
//!   decompile, this is *not* `numwords::plural` on any specific number's
//!   value), and so is the very last word the range itself renders.
//! - A small, separate 13-line mismatch class was also investigated this
//!   round ("Detalle de las olas: ... 3 pies a 5 segundos" -> the engine
//!   reads "cinco" under `norm_time.rs`'s own `src=4`, this crate under
//!   `src=0xa`): confirmed this is *not* caused by the range work above
//!   (`range_scan` correctly declines here -- "pies" isn't a conjunction
//!   right after "3", and there's no conjunction right after "5" either, so
//!   "5 segundos" reaches the plain bare-number-plus-unit-word shape, which
//!   was already matching it before this round). `norm_time.rs`'s own
//!   already-ported `lookback_las_flag` (requiring the word immediately
//!   before the number be exactly "las") also doesn't explain the real
//!   engine's behavior here ("a", not "las", precedes "5") -- so either a
//!   time shape not yet captured in that file's own port explains it, or
//!   there's a genuine chain-precedence subtlety in the real engine neither
//!   port currently replicates. Left for whoever owns `norm_time.rs`: fixing
//!   it there (making time correctly claim "5 segundos" first) is the right
//!   direction, not loosening this file's own unit-word match.

use super::norm_math::numeric;
use super::numwords::{cardinal_to_words, emit_phrase, is_comma_grouped, is_period_grouped, plural, G_F, G_M};
use super::tables_oth::unit_word;
use super::tokenize::{tokenize, Tok};
use super::{emit, FeSpa};
use crate::ling_spa::lexicon::stricmp_spa;
use crate::ling_spa::WaSpa;

const SRC: u32 = 0xa;

/// `DAT_10064530` ("a") and `DAT_100644d0` ("y"), confirmed via `pefile`
/// (see the module doc): the words a literal hyphen/en-dash/em-dash and a
/// literal `/`/`&` get read out as, inside a matched range span.
const RANGE_A: &[u8] = b"a";
const RANGE_Y: &[u8] = b"y";

/// `FUN_10033620`: the dash/en-dash/em-dash/"a"/"y"/"&"/"al"-conjunction
/// range scanner ("2-4", "80-85", "20 a 25", "5 al 10"). `tok` is the
/// already-tokenized leading number (the caller's own entry token, matching
/// `FUN_10033050`'s reuse of its own already-tokenized `param_3` rather than
/// re-tokenizing). Returns the total byte length from `off` through the end
/// of the second number, or `None` if this isn't a range.
///
/// `FUN_10033620`'s own `'/'`-between-numbers third shape (e.g. "5/10
/// grados") is not ported: with `norm_units.rs` running before
/// `norm_fraction.rs` in the chain, declining it here means a bare "N/N"
/// falls through to the already-verified fraction shape instead of being
/// treated as a range -- a real, documented divergence, not an oversight,
/// and a narrow one (NWS text uses "a"/"-" for ranges, not "/").
///
/// The scanner's own peek-only trailing degree/percent-symbol validation
/// (after the second number, gating class==6/'\xb0' or class==3/'%', but
/// never consuming either) is also not replicated: it only *validates* that
/// something reasonable follows, and this function's own caller already
/// requires a real `UNIT_WORDS` match at that exact position, which is a
/// strictly narrower, equivalent gate for every case that matters here.
fn range_scan(t: &[u8], p: usize, off: i32, tok: &Tok) -> Option<i32> {
    if tok.class != 2 || !numeric(tok.text()) {
        return None;
    }
    let mut len = tok.consumed;
    let p1 = p + len as usize;
    let off1 = off + len;
    let (conj, r1) = tokenize(t, p1, off1, 0xb);
    if r1 == 0 {
        return None;
    }
    let ctext = conj.text();
    let word_conj = (matches!(ctext, b"a" | b"y" | b"&") && conj.len == 1) || stricmp_spa(ctext, b"al") == 0;
    let dash_conj = matches!(ctext.first(), Some(&(b'-' | 0x96 | 0x97))) && conj.len == 1;

    if word_conj && conj.ws < 2 {
        len += r1;
        let p2 = p1 + r1 as usize;
        let off2 = off1 + r1;
        let (num2, r2) = tokenize(t, p2, off2, 0xb);
        if r2 == 0 || !numeric(num2.text()) || num2.ws > 1 {
            return None;
        }
        return Some(len + r2);
    }
    if dash_conj && conj.ws < 2 {
        len += r1;
        let p2 = p1 + r1 as usize;
        let off2 = off1 + r1;
        let (num2, r2) = tokenize(t, p2, off2, 0xb);
        if r2 == 0 || !numeric(num2.text()) || num2.ws > 1 {
            return None;
        }
        return Some(len + r2);
    }
    None
}

/// `FUN_100324d0`: re-tokenize the raw span `t[p..p+span_len]` (a matched
/// range, see [`range_scan`]) and emit each sub-token as its own word,
/// `src=0xa` throughout, exactly as the engine's own commit dispatch does
/// (`FUN_100321c0`'s own `FUN_10006fe0` vs. `DAT_1006905c` ("NUMS", the
/// marker `FUN_10033050`'s simple range branch writes) check, confirmed via
/// `pefile`, picks this function over the `FUN_1004ca30`-on-the-whole-span
/// fallback used by the *other*, prev-word-continuation branch of
/// `FUN_10033050` -- not ported here, see the module doc): a number renders
/// via `cardinal_to_words` with `gender`; a plain word (the literal "a"/"y"/
/// "al" text) is emitted as itself; a `-` symbol reads as "a", a `/` or `&`
/// symbol reads as "y"; an en-dash/em-dash (raw high byte 0x96/0x97) reads
/// as "a"; anything else is silently skipped. Marks the very last word this
/// emits as plural (`FUN_100321c0`'s own `if (*(short*)(param_3+0x18)!=0) {
/// mark last record plural }` right after this call, unconditional on
/// whether a range was found at all -- the same `local_4c0[6]` flag
/// `FUN_10033050` always sets for its simple range branch that also makes
/// the trailing unit word plural, see the caller; confirmed via decompile,
/// not derived from "5 a 10 nudos" carrying the flag on "diez" but not on
/// "cinco").
fn render_range(wa: &mut WaSpa, t: &[u8], p: usize, off: i32, span_len: i32, gender: i32) -> bool {
    let mut pos = p;
    let mut o = off;
    let mut done = 0;
    while done < span_len {
        let (sub, r) = tokenize(t, pos, o, 0xb);
        if r == 0 {
            break;
        }
        let ok = match sub.class {
            2 => {
                let mut word = Vec::new();
                cardinal_to_words(&mut word, sub.text(), gender);
                emit_phrase(wa, &word, sub.start, sub.end, b'D', b'D', SRC)
            }
            1 => emit(wa, sub.text(), sub.start, sub.end, b'A', b'D', SRC, None),
            3 => match sub.text().first() {
                Some(b'-') => emit(wa, RANGE_A, sub.start, sub.end, b'A', b'D', SRC, None),
                Some(b'/' | b'&') => emit(wa, RANGE_Y, sub.start, sub.end, b'A', b'D', SRC, None),
                _ => true,
            },
            6 if matches!(sub.text().first(), Some(&(0x96 | 0x97))) => {
                emit(wa, RANGE_A, sub.start, sub.end, b'A', b'D', SRC, None)
            }
            _ => true,
        };
        if !ok {
            return false;
        }
        done += r;
        pos += r as usize;
        o += r;
    }
    let n = wa.i16(0) as usize;
    if n > 0 {
        wa.set_i16(WaSpa::rec(n - 1) + 0x12, 1);
    }
    true
}

pub fn units(_fe: &FeSpa, wa: &mut WaSpa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    // Bare unit word, no leading number (e.g. a lone "\xe1rea"). Origin 'A'
    // (`FUN_100321c0`'s own default, non-slash/O/S per-token emit), not 'D'.
    if tok.class == 1 {
        let Some((word, _fem)) = unit_word(tok.text()) else {
            return -1;
        };
        if !emit_phrase(wa, &word, tok.start, tok.end, b'A', b'D', SRC) {
            return 0;
        }
        return tok.consumed;
    }

    if tok.class != 2 {
        return -1;
    }
    let num_text = tok.text().to_vec();
    if !is_period_grouped(&num_text) && !is_comma_grouped(&num_text) {
        return -1;
    }

    // Range shape ("2-4 pies", "80-85 grados", "20 a 25 nudos"): tried
    // before the plain number+word shape below, matching `FUN_10033050`'s
    // own precedence (it runs once at `FUN_10031720`'s entry, ahead of the
    // main per-token scan that the plain shape corresponds to).
    if let Some(span_len) = range_scan(t, p, off, tok) {
        let p_after = p + span_len as usize;
        let off_after = off + span_len;
        let (range_unit_tok, r1) = tokenize(t, p_after, off_after, 0xb);
        if range_unit_tok.class == 1 {
            if let Some((unit_rendered, fem)) = unit_word(range_unit_tok.text()) {
                let gender = if fem { G_F } else { G_M };
                if !render_range(wa, t, p, off, span_len, gender) {
                    return 0;
                }
                if !emit_phrase(wa, &unit_rendered, range_unit_tok.start, range_unit_tok.end, b'A', b'D', SRC) {
                    return 0;
                }
                // A range's trailing unit word is always marked plural
                // (`FUN_10033050`'s own simple-range branch writes its
                // `local_4c0[6]` plural marker unconditionally -- confirmed
                // via decompile -- which flows into the unit word's own
                // dword-8 plural-flag write in `FUN_10031720`).
                let n = wa.i16(0) as usize;
                if n > 0 {
                    wa.set_i16(WaSpa::rec(n - 1) + 0x12, 1);
                }
                return span_len + r1;
            }
        }
    }

    let p1 = p + tok.consumed as usize;
    let off1 = off + tok.consumed;
    let (unit_tok, r1) = tokenize(t, p1, off1, 0xb);
    if unit_tok.class != 1 {
        return -1;
    }
    let Some((unit_rendered, fem)) = unit_word(unit_tok.text()) else {
        return -1;
    };

    // Masculine units force the numeral's own apocope form ("veintiun",
    // not "veintiuno") the same way a following masculine noun would
    // (`G_M`), matching a fresh capture ("21 nudos" -> "veintiun"); `G_F`
    // for a feminine unit ("21 millas" -> "veintiuna") is unconditional.
    let gender = if fem { G_F } else { G_M };
    let mut num_word = Vec::new();
    cardinal_to_words(&mut num_word, &num_text, gender);

    // `FUN_100321c0`'s own plural-flag write happens inside its per-token
    // commit loop, right after committing *that* token's own record --
    // confirmed against captures: both the number's own record and the
    // unit word's own record get the flag when the count isn't 1.
    let mark_plural = |wa: &mut WaSpa| {
        if plural(&num_text) {
            let n = wa.i16(0) as usize;
            if n > 0 {
                wa.set_i16(WaSpa::rec(n - 1) + 0x12, 1);
            }
        }
    };
    if !emit_phrase(wa, &num_word, tok.start, tok.end, b'D', b'D', SRC) {
        return 0;
    }
    mark_plural(wa);
    if !emit_phrase(wa, &unit_rendered, unit_tok.start, unit_tok.end, b'A', b'D', SRC) {
        return 0;
    }
    mark_plural(wa);
    tok.consumed + r1
}
