//! `FUN_10033760`: four sub-recognizers tried in order, same dispatcher shape
//! as `n15`/`numexpr`. The first (`FUN_10053920`, state/country/zip — see
//! `norm_address_csc.rs`), second (`FUN_100337f0`, "P.O. Box 123" and its
//! spelling variants — see `po_box` below) and third (`FUN_10033b60`, city
//! names gated by the TPP place-name dictionary — see
//! `norm_address_city.rs`) are ported. `FUN_10043810` (the ~600-line
//! street/highway recognizer with its own apartment/basement/mount/state
//! tables, and the `"I-80"`/`"US-6"` reading `numexpr` explicitly declines)
//! is not ported; reaching it declines rather than emitting unverified
//! output. class 0x12.
//!
//! `FUN_100337f0`'s decompile mis-renders one field: the check the pseudo-C
//! shows as `local_76 != 0` (read as "leading whitespace") is actually the
//! token's `udict` flag at struct offset `0x1a`, confirmed by disassembling
//! the function and matching the checked stack slot (`EBP-0x72`, i.e.
//! `struct_base+0x1a`) against the same offset used for the *second* token
//! buffer's equivalent check a few lines later (`EBP-0x2e` = `EBP-0x48+0x1a`)
//! — both are the udict flag, not whitespace.

use super::cstr::*;
use super::numwords::{new_buf, pairs};
use super::sbd::all_digit;
use super::tabs::{ptab, Ptab};
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::Fe;

/// "Post Office Box" / "Postoffice Box" — the only entries that take the
/// multi-word phrase path in `po_box`'s first attempt.
const POBOX_PHRASE: Ptab = ptab(0x10099070, 0x1009b0e4);

fn tk(fe: &Fe, t: &[u8], p: usize, off: i32, mode: i32) -> (Tok, i32) {
    let mut x = Tok::default();
    let r = tokenize(&mut x, t, p, off, mode, fe.ud.as_ref());
    (x, r)
}

pub fn address(fe: &Fe, wa: &mut Wa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let r = super::norm_address_csc::city_state_country(fe, wa, tok, t, p, off);
    if r >= 0 {
        return r;
    }
    let r = po_box(fe, wa, tok, t, p, off);
    if r >= 0 {
        return r;
    }
    let r = super::norm_address_city::street(fe, wa, tok, t, p, off);
    if r >= 0 {
        return r;
    }
    super::norm_address_street::street(fe, wa, tok, t, p, off)
}

/// A `"."`-only token by the engine's loose test: non-empty, one or two
/// characters, first one a period — not `len == 1`. Reproduced as found (a
/// real quirk, not a bug we get to fix): `".."` would also pass.
fn dotlike(x: &Tok) -> bool {
    x.len > 0 && x.len < 3 && x.text[0] == b'.'
}

/// `FUN_100567c0`: does the phrase table have an entry whose first word
/// (case-insensitively) is `w0`, and if that entry continues with more
/// words, do the following input tokens match them too (word by word, each
/// bounded by a literal space in both the candidate and the input — not by
/// `tok.ws`, matching the engine's own raw-byte peek). Returns the total
/// consumed byte count of the whole phrase on a full match.
fn pobox_phrase(fe: &Fe, w0: &[u8], t: &[u8], p: usize, off: i32, consumed0: i32) -> Option<i32> {
    let w0 = cs(w0);
    if w0.is_empty() {
        return None;
    }
    for i in 0..POBOX_PHRASE.count() {
        let cand = POBOX_PHRASE.get(i);
        if strnicmp(cand, w0, w0.len()) != 0 {
            continue;
        }
        match at(cand, w0.len() as isize) {
            0 => return Some(consumed0),
            b' ' => {
                let mut coff = w0.len() + 1;
                let mut consumed = consumed0;
                loop {
                    if at(t, p as isize + consumed as isize) != b' ' {
                        break;
                    }
                    let (w, r) = tk(fe, t, p + consumed as usize, off + consumed, 0);
                    if r == 0 || w.class == 9 || w.len == 0 {
                        break;
                    }
                    let rest = &cand[coff..];
                    let wl = w.len as usize;
                    if strnicmp(rest, &w.text, wl) != 0 {
                        break;
                    }
                    match at(rest, wl as isize) {
                        0 => return Some(consumed + r),
                        b' ' => {
                            coff += wl + 1;
                            consumed += r;
                        }
                        _ => break,
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// `FUN_100337f0`: `"Post Office Box"`/`"Postoffice Box"` (multi-word,
/// looked up first), else a single token spelled `"P.O.Box"`/`"P.O.B"`/
/// `"POB"` (an optional lone `"."` token right after is silently absorbed
/// into the consumed span but *not* into the emitted word's own span — the
/// engine reads it as "abbreviation terminator", not "part of the address"),
/// else `"P.O"` followed by `"Box"` with an optional `"."` between them.
/// Emits the canonical spelling (`"Postoffice Box"` for the phrase-table
/// match, `"P.O Box"` for every other spelling) then, if a following
/// all-digit token isn't too far away (`ws < 3`), the box number read in
/// digit pairs (`numwords::pairs`, e.g. `"123"` -> "one twenty three"-style
/// pairing, same reader `time` uses for HH:MM).
fn po_box(fe: &Fe, wa: &mut Wa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let _ = tok;
    let (w0, r0) = tk(fe, t, p, off, 0x15);
    if r0 == 0 || w0.class == 9 || w0.udict != 0 || w0.class != 1 {
        return -1;
    }

    let text: &[u8];
    let consumed: i32;
    let start = w0.start;

    if let Some(c) = pobox_phrase(fe, &w0.text, t, p, off, r0) {
        text = s(0x10080044); // "Postoffice Box"
        consumed = c;
        // the engine computes this span's end as `off + consumed` (an
        // absolute position), not a specific token's own `.end` field —
        // the two coincide since `consumed` is the exact byte count from
        // `off` through the last matched word.
        return finish(fe, wa, t, p, off, text, start, off + consumed, consumed);
    } else if stricmp(&w0.text, s(0x1008003c)) == 0 {
        // "P.O.Box" exact, optional trailing "."
        let (w1, r1) = tk(fe, t, p + r0 as usize, off + r0, 0x15);
        let extra = if w1.udict == 0 && w1.ws == 0 && dotlike(&w1) { r1 } else { 0 };
        text = s(0x10080020); // "P.O Box"
        consumed = r0 + extra;
        return finish(fe, wa, t, p, off, text, start, w0.end, consumed);
    } else if stricmp(&w0.text, s(0x10080034)) == 0 || stricmp(&w0.text, s(0x10080030)) == 0 {
        // "P.O.B" / "POB" exact, optional trailing "."
        let (w1, r1) = tk(fe, t, p + r0 as usize, off + r0, 0x15);
        let extra = if w1.udict == 0 && w1.ws == 0 && dotlike(&w1) { r1 } else { 0 };
        // paul09 FUN_10030ee0 speaks "P.O.B" as is.
        text = if fe.era == crate::era::Era::Paul09 { s(0x10080034) } else { s(0x10080020) };
        consumed = r0 + extra;
        return finish(fe, wa, t, p, off, text, start, w0.end, consumed);
    } else if stricmp(&w0.text, s(0x1008002c)) == 0 {
        // "P.O" [.] "Box"
        let (w1, r1) = tk(fe, t, p + r0 as usize, off + r0, 0x15);
        if r1 == 0 || w1.class == 9 || w1.udict != 0 || w1.len <= 0 {
            return -1;
        }
        let (boxw, period_r, box_r) = if w1.ws == 0 && dotlike(&w1) {
            let (w2, r2) = tk(fe, t, p + (r0 + r1) as usize, off + r0 + r1, 0x15);
            if r2 == 0 || w2.class == 9 || w2.udict != 0 {
                return -1;
            }
            (w2, r1, r2)
        } else {
            (w1, 0, r1)
        };
        if boxw.len <= 0 || stricmp(&boxw.text, s(0x10080028)) != 0 || boxw.ws >= 3 {
            return -1;
        }
        text = s(0x10080020); // "P.O Box"
        consumed = r0 + period_r + box_r;
        return finish(fe, wa, t, p, off, text, start, boxw.end, consumed);
    } else {
        return -1;
    }
}

fn finish(fe: &Fe, wa: &mut Wa, t: &[u8], p: usize, off: i32, text: &[u8], start: i32, end: i32, consumed: i32) -> i32 {
    // FUN_100451e0 is emit_split: "P.O Box"/"Postoffice Box" become two
    // records ("P.O"/"Box" or "Postoffice"/"Box") sharing this one span.
    if !wa.emit_split(text, start, end, 0x41, 0x44, 0x12) {
        return 0;
    }
    let (num, nr) = tk(fe, t, p + consumed as usize, off + consumed, 0x15);
    if num.len > 0 && num.udict == 0 && all_digit(&num.text) && num.ws < 3 {
        let mut buf = new_buf();
        pairs(&mut buf, &num.text);
        if !wa.emit_split(&buf, num.start, num.end, 0x44, 0x44, 0x12) {
            return 0;
        }
        return consumed + nr;
    }
    consumed
}
