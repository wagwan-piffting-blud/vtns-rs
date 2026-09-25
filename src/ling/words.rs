//! `fe_BuildWordPhoneRecords` (0x10016c90): copy the analysed word array into
//! the SS word records, expand each word's phone string into codes and
//! boundary marks, set the per-word domain byte and the sentence type.
//!
//! `wr::START`/`wr::END` are *not* actually written by this function in the
//! real engine -- they're written by `fe_NextSentence` (`0x10022dc0`), which
//! calls `fe_AnalyzeSentence` (-> `fe_LinguisticAnalysis`, this whole ling
//! stage) then walks the resulting SS word records, before this function
//! ever runs. Confirmed from `fe_NextSentence`'s own decompile:
//! ```c
//! // psVar3 = WA (TIB+0x1312bc); piVar6 = TIB; iVar2 = TIB+0x4c = SS
//! // piVar7 walks WA records (war::START/war::END), piVar8 walks SS word
//! // records (wr::START/wr::END, stride 0x3c0), in lockstep
//! iVar9 = piVar7[-1];             // = war::START
//! iVar5 = piVar6[1];              // = TIB+4, the running request cursor
//! piVar8[-1] = iVar5 + iVar9;     // wr::START = cursor + war::START
//! if (piVar7[-1] < *piVar7)
//!     *piVar8 = *piVar7 - 1 + piVar6[1];   // wr::END = cursor + war::END - 1
//! else
//!     *piVar8 = iVar5 + iVar9;             // degenerate (empty word)
//! // ... later, after this sentence is fully handled:
//! piVar6[1] = piVar6[1] + iVar4;  // TIB+4 += WA+4 (`Wa::consumed`)
//! ```
//! i.e. `war::START`/`war::END` are per-sentence-relative (reset to 0 each
//! sentence -- confirmed from raw `wa_out` bytes), but the cursor `TIB+4`
//! accumulates every earlier sentence's `Wa::consumed` within the same
//! request, so `cursor + war::START/END` is request-relative. There's then
//! a *second* remap, through "processed -> original position" tables
//! `fe_InitialTextProcess` builds once per request at `TIB+0x47794`
//! (`map_start`) / `TIB+0x47798` (`map_end`), read straight off the
//! cursor-adjusted value:
//! ```c
//! if (*(char *)(DAT_100a0460 + 0x20424) == '\x01') {
//!     // untaken by every capture so far (single-map text); a second,
//!     // composed indirection through a third table at TIB+0x47790 for
//!     // text that's *both* fullwidth-normalized *and* VTML-tag-stripped
//!     // -- not implemented, no capture exercises it
//! } else {
//!     // clamp start/end into [0, TIB+0) first, then:
//!     start = map_start[cursor + war::START];
//!     end   = map_end[cursor + war::END - 1];
//! }
//! ```
//! For plain (non-VTML, non-fullwidth) text these tables are the identity
//! function, so applying them is a harmless no-op -- confirmed against the
//! `out_feling` capture `multi_01.json`'s 5 sentences (no VTML) with the maps
//! omitted entirely below. For VTML text they're not identity (tag text is
//! excised from the processed stream the WA/`war::START` positions are
//! relative to), confirmed against the `out_feling` capture `vtml_pitch.json`
//! (3 sentences, tags of different lengths between each): the raw
//! `cursor + war::START` for sentence 2's first word was `18`, but the true
//! original-input position is `40` -- exactly `map_start[18]`.
//!
//! Folded into this function anyway (rather than porting `fe_NextSentence`
//! as its own step) as a `pos: &TextPos` parameter, since this is the last
//! point before the SS word records are consumed and every caller already
//! has a natural per-sentence loop to track `cursor` in. `TextPos::none()`
//! (`cursor=0`, no maps) is correct for a standalone single-sentence
//! request with no VTML/fullwidth content, which covers most callers.
//!
//! `war::END` is exclusive (one past the word's last character); `wr::END`
//! is inclusive. Every existing test used to preload the whole captured
//! word record first (`load_ss`), which supplied `wr::START`/`wr::END`
//! directly, so a totally missing write here went unnoticed until
//! `engine.rs` built an `Ss` from scratch: without it,
//! `breaks::predict_break_index`'s "real character gap between adjacent
//! words" guard (`prev_end != end && prev_end+1 != start`) is always false,
//! so the engbi CART break predictor never fires. `breaks`/`context`
//! (and therefore prosody/synthesis) don't actually depend on `pos` being
//! right, only on START/END *gaps* between consecutive words being
//! internally consistent, which holds even with `TextPos::none()`
//! throughout -- but the raw values themselves are real output (the
//! waveform sync ring, the lip-sync log and `VT_TextToBufferEX`'s
//! `SYNCINFO` events all report them), so getting `pos` right matters for
//! anything consuming those.

use super::{ptr_wa, war, wr, Ss, Wa, SS_TYPE};

/// The bits of per-request text-position state `wr::START`/`wr::END` need:
/// the running "characters consumed by earlier sentences" cursor, and the
/// (optional) "processed -> original position" tables for VTML/fullwidth
/// text. See the module doc comment.
pub struct TextPos<'a> {
    pub cursor: i32,
    pub map_start: Option<&'a [i32]>,
    pub map_end: Option<&'a [i32]>,
}

impl<'a> TextPos<'a> {
    /// A standalone request with no accumulated cursor and no VTML/
    /// fullwidth remap -- correct for any single-sentence, plain-text
    /// request.
    pub fn none() -> Self {
        TextPos { cursor: 0, map_start: None, map_end: None }
    }

    fn remap(map: Option<&[i32]>, pos: i32) -> i32 {
        match map {
            Some(m) => m.get(pos.max(0) as usize).copied().unwrap_or(pos),
            None => pos,
        }
    }

    fn start(&self, war_start: i32) -> i32 {
        Self::remap(self.map_start, self.cursor + war_start)
    }

    fn end(&self, war_end_exclusive: i32) -> i32 {
        Self::remap(self.map_end, self.cursor + war_end_exclusive - 1)
    }
}

/// Returns false when a word has no phones (the engine returns 0 and the
/// sentence is dropped); the SS is left partially filled exactly as the
/// engine leaves it.
pub fn build_word_phone_records(ss: &mut Ss, wa: &Wa, kind7: &[i32], pos: &TextPos) -> bool {
    let nw = ss.n_words();
    for i in 0..nw {
        let r = Wa::rec(i);
        let w = Ss::word(i);
        ss.set_i32(w + wr::START, pos.start(wa.i32(r + war::START)));
        ss.set_i32(w + wr::END, pos.end(wa.i32(r + war::END)));
        ss.set_u32(w + wr::TEXT_PTR, ptr_wa(r + war::TEXT));
        ss.set_i16(w + wr::X3B4, wa.i16(r + war::NMORPH));
        ss.set_u32(w + wr::WA_PTR, ptr_wa(r + war::FORCED_POS));
        ss.set_u32(w + wr::PH_PTR, ptr_wa(r + war::PHONES));
        ss.set_u8(w + wr::CLASS_CHAR, wa.u8(r + 0x23));
        ss.set_i16(w + wr::TOKEN_CLASS, wa.i16(r + war::TOKEN_CLASS));
        ss.set_u8(w + wr::SRC, wa.u8(r + war::SRC));
        ss.set_u8(w + wr::TAG_A, wa.u8(r + war::TAG_A));
        ss.set_u8(w + wr::TAG_B, wa.u8(r + war::TAG_B));
        ss.set_i16(w + wr::X3AE, wa.i16(r + 0x20));

        let ph = wa.cstr(r + war::PHONES);
        let len = ph.len().min(0x41);
        let mut n = 0usize;
        for &c in &ph[..len] {
            match c {
                b'd' => {
                    if n > 0 {
                        ss.set_u8(w + wr::MARKS + n - 1, b'1');
                    }
                }
                b'c' => {
                    if n > 0 {
                        ss.set_u8(w + wr::MARKS + n - 1, b'2');
                    }
                }
                _ => {
                    ss.set_u8(w + wr::M_FLAG + n, 0);
                    if c == b'M' {
                        ss.set_u8(w + wr::M_FLAG + n, 1);
                    } else if (c as i8) < 1 || c > b'E' {
                        break;
                    }
                    ss.set_u8(w + wr::CODES + n, c);
                    ss.set_u8(w + wr::MARKS + n, b'0');
                    n += 1;
                }
            }
        }
        if n == 0 {
            return false;
        }
        ss.set_u8(w + wr::N_PHONES, n as u8);
    }
    for i in 0..nw {
        let v = kind7.get(i).copied().unwrap_or(-1);
        ss.set_u8(Ss::word(i) + wr::DOMAIN, if v == -1 { 0 } else { 0x0c });
    }
    // The engine indexes one record past the last word's base with offset
    // 0x638 == last word + 0x3ac, i.e. the last word's token class.
    let last = Ss::word(nw) - 0x3c0;
    let class = ss.i16(last + wr::TOKEN_CLASS);
    let t = if class == 3 {
        if wa.i16(2) == 0 { 6 } else { 7 }
    } else if class == 4 {
        7
    } else if ss.u8(last + wr::DOMAIN) == 0x0c {
        7
    } else {
        5
    };
    ss.set_u8(SS_TYPE, t);
    true
}
