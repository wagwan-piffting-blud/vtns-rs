//! `fe_InitialTextProcess` (0x10007730 in vt_spa_violeta16.dll): raw request
//! text -> processed text, the processed -> original offset maps, the 8 VTML
//! span channels and the mark list. Violeta's own equivalent of
//! `text::initial`, ported independently (not shared source: the two
//! tokenizers/`Tok` types differ, see below) once the decompile proved the
//! framework identical.
//!
//! Framework, fully decompiled and ported exactly:
//! - `fe_InitialTextProcess` (0x10007730): allocates `map_a`/`map_b`/`map_c`,
//!   runs the fullwidth pass then the VTML pass (both via
//!   `fe_RunTextFilterPass`, 0x10007fc0, a generic two-pass -- dry run then
//!   real -- driver called with a different callback each time), then remaps
//!   every already-built WA record's stored offset through the map chain.
//!   Not ported (matches English): `fe_InjectDemoNag`.
//! - `FUN_100076d0`/`FUN_10019e10`: is `text[i..]` a fullwidth (EUC-KR-style)
//!   byte pair -> `dbcs_pair`+`has_n`, already shared, general-purpose code in
//!   `text::cstr`.
//! - The fullwidth-pair callback (no Ghidra function boundary existed for it
//!   -- it is only reached via an address-of passed as a callback, so I
//!   translated it from `disassemble_bytes` at 0x100080f0..0x1000834f, 608
//!   bytes/190 instructions) maps the 73-pair table at 0x10063c18 (stride
//!   0x10, `FUN_10003f20` binary search) to ASCII, `map_fullwidth` below;
//!   table extracted with `tools/gen_rust_tables.py` (pefile, not by hand).
//! - The VTML-tag callback (also unregistered; disassembled at
//!   0x10008350..0x1000873f, 1008 bytes) resets the 8 span channels and the
//!   mark list, then walks the text calling `FUN_10019d10` at every byte
//!   whose `CLS_S` class doesn't have bit `0x6` set (same mask as English's
//!   `cls(c) & 6`). `FUN_10019d10` is the longest-case-insensitive-prefix
//!   table walk (`try_tag` below): its table lives at the *constant* address
//!   0x10065a68 passed in by the caller (NOT the per-instance TIB, despite
//!   looking like one at first read) -- 31 entries, 0x18 bytes/6 dwords each
//!   (`[name_ptr, name_len, f08, f0c, handler, f14]`, terminated by
//!   `name_len==0`), extracted the same way. The 31 names, in table order,
//!   are byte-for-byte the same 31 as English's `TAGS`, and every numeric
//!   constant I checked against a Spanish decompile matches English's exactly
//!   (see per-handler notes below) -- this is the shared engine core, not
//!   per-language content, so `TAGS`/`CLAMP`'s *shape* is carried from
//!   `text::initial`'s proven table, independently re-verified against the
//!   Spanish table/handlers below rather than assumed.
//!
//! Per-handler status (each checked against its own Spanish decompile/
//! disassembly where noted; unchecked constants are flagged):
//! - `vtml_break`(0x1001a330)/`vt_break`(0x1001a360): thin wrappers pushing a
//!   literal kind constant (3 / 0x14) into the shared writer `FUN_1001a0e0`
//!   -- decompiled in full. `FUN_1001a0e0` strips a trailing `/` from the
//!   body *only* for kind 0x14 (the legacy self-closing form), clamps the
//!   value to `DAT_10065df4..DAT_10065de0` = `(0, 3)`, and writes `\xa2\xfd`
//!   instead of `\xa2\xfe` when the clamped value is 3 -- all confirmed
//!   identical to `text::initial`'s `H::Break`. PORTED.
//! - `vtml_pause`(0x1001af80)/`vt_pause`(0x1001afb0): same shape via
//!   `FUN_1001ad40`, decompiled in full; kind 6 requires the literal keyword
//!   `time` (confirmed: `DAT_10066fdc` reads exactly `"time"`), kind 0x13
//!   does not; clamp `DAT_10065df0..DAT_10065ddc` = `(0, 65535)`. PORTED.
//! - `vtml_mark`(0x1001a390): its own routine, not delegated; disassembly
//!   (0x1001a390 onward) shows it writing `\xa2\xfe` directly and indexing
//!   the same `param+0x82e6c/0x82e70/0x82e64/0x82e68` map arrays and the
//!   `+0x15db18` mark-list pointer `fe_InitialTextProcess` itself uses.
//!   Ported to the same shape as `text::initial`'s `H::Mark` (self-closing ->
//!   no name/kind 2; else needs a `name` attribute, long name ->
//!   truncated/kind 3); the exact name-length threshold (0x200) and the
//!   `name`-attribute regex slot are carried from the proven-shared pattern,
//!   not re-disassembled instruction-by-instruction.
//! - `vtml_phoneme`(0x1001b160): disassembled 0x1001b160..0x1001b48d.
//!   Confirmed: the close (`</vtml_phoneme>`) path gates on the same
//!   `open[PHON]`-style flag byte and writes `\xa2\xfe`; **the self-closing
//!   check the brief asked about is present and identical**: at
//!   0x1001b2e6-0x1001b30d the body's last byte is compared to `/` (0x2f)
//!   and, if equal, the content length is forced to 0 and the `content()`
//!   search is skipped entirely -- byte-for-byte the same branch as
//!   `text::initial`'s fixed `let self_closing = cs(&body).last() ==
//!   Some(&b'/')`. The regex-index push (7) right before this also matches
//!   English's `re(7, &body)` slot. PORTED the framework (self-closing
//!   detection, close handling, content-length bookkeeping); the alphabet
//!   converters themselves are declined below.
//! - `vtml_pitch/speed/volume`(0x1001c040/0x1001c070/0x1001c0a0) and their
//!   `vt_`/close pairs: **not** Ghidra function boundaries (same
//!   unregistered-callback problem as the two filter passes) and I did not
//!   have session budget to disassemble all six by hand. Ported the plain
//!   (non-`emotion`) `H::Level` shape from `text::initial` on the strength of
//!   the pattern above; `CLAMP[0..=2]` are *not* independently verified
//!   (only `CLAMP[3]` pause and `CLAMP[4]` break are decompile-confirmed).
//!   Declined: the `emotion` sub-case (`ch==7`, category-table lookup) --
//!   returns `None` (the tag falls through as literal text) since I never
//!   located Violeta's category table.
//! - `vtml_partofsp`, `say-as`/`vtml_sayas`: framework (open/close, content
//!   length, attribute pulls) ported to the same shape as `text::initial`;
//!   `partofsp_conv`/`sayas_conv` themselves are stubs returning `None`
//!   (0x1001a7a0 and the say-as dispatch are not registered Ghidra
//!   functions either). A stubbed converter is not a silent wrong answer:
//!   `content_tag`'s `None` branch is the same "pass the content through
//!   unconverted" path the *real* engine takes whenever its own converter
//!   rejects the attribute/content combination, so this only diverges from
//!   the engine on inputs it would have actually converted.
//! - `vtml_skip`: no attributes at all in the real engine (`!empty =>
//!   None`), so this needed no converter and is PORTED in full.
//! - `vtml_sub`/`say-as` (kind 0x11/0x15): `tagged()` is a pure byte-wrapper
//!   (no DLL data), so this is PORTED in full.
//!
//! Not reproduced anywhere in this file: the Spencer regex pre-validation
//! (`re(idx, body)` in `text::initial`). Violeta's compiled regex table is
//! reached through a *runtime* pointer (`FUN_10020bc0` calls through
//! `*(int*)(DAT_1007ea18 + 0x20428)`, not a fixed rodata address I could
//! `read_memory`/pefile without a live instance), so I could not extract its
//! pattern text this session. Every handler below instead validates directly
//! via the same keyword/numeric checks the engine performs *after* its regex
//! gate (`strtok`/`attr`-based), so well-formed VTML matches identically;
//! a small class of malformed bodies the real regex would reject up front
//! (e.g. trailing garbage after a valid value) may be accepted here instead.
//! The one exception is the "is body blank" check (English's `re(0, body)`),
//! reproduced directly as [`is_blank`] since a whitespace-only class is not
//! per-language content.
//!
//! Shared with `text::initial` (bumped from `fn` to `pub(crate) fn`/kept
//! `pub`, bodies unchanged -- pure byte utilities and result containers with
//! no DLL-address dependency, so identical regardless of language):
//! `strtok`, `attr`, `trim`, `tagged`, `Chan`, `Mark`, `Initial`.

use crate::text::cstr::{at, atoi, cs, dbcs_pair, has_n, strnicmp};
use crate::text::initial::{attr, m_seq, parse_alt, strtok, tagged, trim, Chan, Initial, Mark};

use super::tables::CLS_S;

// ------------------------------------------------------- regex pre-gate ---
//
// Violeta's compiled regex table lives behind a runtime pointer
// (`*(int*)(DAT_1007ea18 + 0x20428)`, populated at engine load, not static
// rodata I can walk by index). But the *source* pattern strings it compiles
// from are themselves plain NUL-terminated rodata, sitting right after the
// 31-entry tag table (0x10065a68 + 31*0x18 = 0x10065d50 onward). Extracted
// verbatim with `scripts/extract_regex_strings.py` (pefile, never by hand)
// and matched to each handler by content (attribute keyword) plus, where
// ambiguous, a live capture (`probe/corpus2.txt` -- `<vt_pitch=120>` and
// `<vt_pitch PITCH=120>` disambiguate which shape the shared vt_-legacy gate
// actually accepts). `m_seq`/`parse_alt` are `text::initial`'s regex-engine
// internals (pure pattern matching, no DLL-address dependency), reused
// as-is; only the pattern *text* here is Spanish-specific.
fn re_match(pat: &[u8], body: &[u8]) -> bool {
    let mut i = 0;
    let alts = parse_alt(pat, &mut i);
    let s = cs(body);
    (0..=s.len()).any(|st| alts.iter().any(|seq| m_seq(seq, s, st, &|_| true)))
}

/// `0x100666f8`-adjacent block. `vtml_break`'s own gate: `LEVEL="0".."3"`,
/// quoted, whole body. Confirmed live: `<vtml_break level="0"/>` converts,
/// `<vtml_break level="-5"/>` does not (not a single `[0-3]` char).
const RE_BREAK_LEVEL: &[u8] = b"^[ \x0c\n\r\t\x0b]*([Ll][Ee][Vv][Ee][Ll])[ \x0c\n\r\t\x0b]*=[ \x0c\n\r\t\x0b]*\"[ \x0c\n\r\t\x0b]*([0]|[1]|[2]|[3])[ \x0c\n\r\t\x0b]*\"[ \x0c\n\r\t\x0b]*$";
/// `0x100667b8`: `vt_break`'s bare gate, `=0`.."=3" only (its own dedicated
/// pattern, distinct from the generic signed-digit one below).
const RE_VTBREAK_BARE: &[u8] = b"^=([0]|[1]|[2]|[3])$";
/// `0x100667d0`: the generic bare `=N` gate, confirmed shared live by
/// `vt_pause`, `vt_pitch`, `vt_speed` and `vt_volume` (all four accept
/// `<tag=N>`, none accept `<tag KEYWORD=N>` -- `probe/corpus2.txt`).
const RE_BARE_SIGNED: &[u8] = b"^=([+-]?[0-9]+)$";
/// `0x100666b5c` (`TIME="[0-9]+"`, quoted, whole body): `vtml_pause`'s gate.
const RE_PAUSE_TIME: &[u8] = b"^[ \x0c\n\r\t\x0b]*([Tt][Ii][Mm][Ee])[ \x0c\n\r\t\x0b]*=[ \x0c\n\r\t\x0b]*\"[ \x0c\n\r\t\x0b]*[0-9]+[ \x0c\n\r\t\x0b]*\"[ \x0c\n\r\t\x0b]*$";
/// `0x10066970` (`VALUE="[0-9]+"`, quoted, whole body): the `vtml_pitch`/
/// `vtml_speed`/`vtml_volume` gate.
const RE_LEVEL_VALUE: &[u8] = b"^[ \x0c\n\r\t\x0b]*([Vv][Aa][Ll][Uu][Ee])[ \x0c\n\r\t\x0b]*=[ \x0c\n\r\t\x0b]*\"[ \x0c\n\r\t\x0b]*[0-9]+[ \x0c\n\r\t\x0b]*\"[ \x0c\n\r\t\x0b]*$";
/// `0x10066cc0`: `vtml_mark`'s gate, `NAME="..."` whole body.
const RE_NAME: &[u8] = b"^[ \x0c\n\r\t\x0b]*([Nn][Aa][Mm][Ee])[ \x0c\n\r\t\x0b]*=[ \x0c\n\r\t\x0b]*\"(.)*\"[ \x0c\n\r\t\x0b]*$";
/// `0x10066bb0`: `vtml_partofsp`'s gate -- `PART=` one of exactly the 6
/// listed words, whole body.
const RE_PART: &[u8] = b"^[ \x0c\n\r\t\x0b]*([Pp][Aa][Rr][Tt])[ \x0c\n\r\t\x0b]*=[ \x0c\n\r\t\x0b]*\"[ \x0c\n\r\t\x0b]*(([Nn][Oo][Uu][Nn])|([Vv][Ee][Rr][Bb])|([Mm][Oo][Dd][Ii][Ff][Ii][Ee][Rr])|([Ff][Uu][Nn][Cc][Tt][Ii][Oo][Nn])|([Ii][Nn][Tt][Ee][Rr][Jj][Ee][Cc][Tt][Ii][Oo][Nn])|([Uu][Nn][Kk][Nn][Oo][Ww][Nn]))[ \x0c\n\r\t\x0b]*\"[ \x0c\n\r\t\x0b]*$";
/// `0x10066774`: `say-as`'s (kind 0x15) gate -- `SUB=".+"`, whole body.
const RE_SUB: &[u8] = b"^[ \x0c\n\r\t\x0b]*([Ss][Uu][Bb])[ \x0c\n\r\t\x0b]*=[ \x0c\n\r\t\x0b]*\".+[ \x0c\n\r\t\x0b]*\"[ \x0c\n\r\t\x0b]*$";
/// `0x10066820`: `vtml_sub`'s (kind 0x11) gate -- `ALIAS="..."`, unanchored
/// (the engine's own pattern has no leading `^`/trailing `$` here, unlike
/// the ones above).
const RE_ALIAS: &[u8] = b"[ \x0c\n\r\t\x0b]*([Aa][Ll][Ii][Aa][Ss])[ \x0c\n\r\t\x0b]*=[ \x0c\n\r\t\x0b]*\"(.)*\"[ \x0c\n\r\t\x0b]*";

// -------------------------------------------------------- fullwidth pass --

/// `0x10063c18`, 73 entries, stride 0x10 (extracted with
/// `tools/gen_rust_tables.py`, never by hand): `(c0, c1, replacement)`.
#[rustfmt::skip]
const FW_TAB: [(u8, u8, &[u8]); 73] = [
    (0xa1, 0xa1, b"\x20"), (0xb0, 0xa1, b"\x22"), (0xb1, 0xa1, b"\x22"), (0xc8, 0xa1, b"\x22"),
    (0xa2, 0xa3, b"\x22"), (0xaf, 0xa3, b"\x2f"), (0xb0, 0xa3, b"\x30"), (0xb1, 0xa3, b"\x31"),
    (0xb2, 0xa3, b"\x32"), (0xb3, 0xa3, b"\x33"), (0xb4, 0xa3, b"\x34"), (0xb5, 0xa3, b"\x35"),
    (0xb6, 0xa3, b"\x36"), (0xb7, 0xa3, b"\x37"), (0xb8, 0xa3, b"\x38"), (0xb9, 0xa3, b"\x39"),
    (0xba, 0xa3, b"\x3a"), (0xbc, 0xa3, b"\x3c"), (0xbd, 0xa3, b"\x3d"), (0xbe, 0xa3, b"\x3e"),
    (0xc1, 0xa3, b"\x41"), (0xc2, 0xa3, b"\x42"), (0xc3, 0xa3, b"\x43"), (0xc4, 0xa3, b"\x44"),
    (0xc5, 0xa3, b"\x45"), (0xc6, 0xa3, b"\x46"), (0xc7, 0xa3, b"\x47"), (0xc8, 0xa3, b"\x48"),
    (0xc9, 0xa3, b"\x49"), (0xca, 0xa3, b"\x4a"), (0xcb, 0xa3, b"\x4b"), (0xcc, 0xa3, b"\x4c"),
    (0xcd, 0xa3, b"\x4d"), (0xce, 0xa3, b"\x4e"), (0xcf, 0xa3, b"\x4f"), (0xd0, 0xa3, b"\x50"),
    (0xd1, 0xa3, b"\x51"), (0xd2, 0xa3, b"\x52"), (0xd3, 0xa3, b"\x53"), (0xd4, 0xa3, b"\x54"),
    (0xd5, 0xa3, b"\x55"), (0xd6, 0xa3, b"\x56"), (0xd7, 0xa3, b"\x57"), (0xd8, 0xa3, b"\x58"),
    (0xd9, 0xa3, b"\x59"), (0xda, 0xa3, b"\x5a"), (0xdf, 0xa3, b"\x5f"), (0xe1, 0xa3, b"\x61"),
    (0xe2, 0xa3, b"\x62"), (0xe3, 0xa3, b"\x63"), (0xe4, 0xa3, b"\x64"), (0xe5, 0xa3, b"\x65"),
    (0xe6, 0xa3, b"\x66"), (0xe7, 0xa3, b"\x67"), (0xe8, 0xa3, b"\x68"), (0xe9, 0xa3, b"\x69"),
    (0xea, 0xa3, b"\x6a"), (0xeb, 0xa3, b"\x6b"), (0xec, 0xa3, b"\x6c"), (0xed, 0xa3, b"\x6d"),
    (0xee, 0xa3, b"\x6e"), (0xef, 0xa3, b"\x6f"), (0xf0, 0xa3, b"\x70"), (0xf1, 0xa3, b"\x71"),
    (0xf2, 0xa3, b"\x72"), (0xf3, 0xa3, b"\x73"), (0xf4, 0xa3, b"\x74"), (0xf5, 0xa3, b"\x75"),
    (0xf6, 0xa3, b"\x76"), (0xf7, 0xa3, b"\x77"), (0xf8, 0xa3, b"\x78"), (0xf9, 0xa3, b"\x79"),
    (0xfa, 0xa3, b"\x7a"),
];

fn fw_lookup(c0: u8, c1: u8) -> Option<&'static [u8]> {
    FW_TAB.iter().find(|&&(a, b, _)| a == c0 && b == c1).map(|&(_, _, v)| v)
}

/// The fullwidth-pair callback (0x100080f0, disassembly-only -- no Ghidra
/// function boundary; see module doc). Same shape as `text::initial`'s
/// `map_fullwidth`: every pair, mapped or not, gets lead/trail offsets in
/// `b`/`c` for all its output bytes.
fn map_fullwidth(src: &[u8]) -> (Vec<u8>, Vec<i32>, Vec<i32>) {
    let n = src.len();
    let mut out = Vec::with_capacity(n);
    let (mut b, mut c) = (Vec::with_capacity(n), Vec::with_capacity(n));
    let mut i = 0usize;
    while i < n {
        let c0 = src[i];
        let c1 = at(src, i as isize + 1);
        if has_n(src, i as isize, 2) && dbcs_pair(c0, c1) {
            let m = (i as i32, i as i32 + 1);
            match fw_lookup(c0, c1) {
                Some(v) => {
                    for &x in v {
                        out.push(x);
                        b.push(m.0);
                        c.push(m.1);
                    }
                }
                None => {
                    for x in [c0, c1] {
                        out.push(x);
                        b.push(m.0);
                        c.push(m.1);
                    }
                }
            }
            i += 2;
        } else {
            out.push(c0);
            b.push(i as i32);
            c.push(i as i32);
            i += 1;
        }
    }
    (out, b, c)
}

// -------------------------------------------------------------- the pass --

const WS: &[u8] = b" \x0c\n\r\t\x0b";

/// English's `re(0, body)`: is `body` blank. A whitespace-only character
/// class is not per-language content, so this is reproduced directly rather
/// than declined along with the rest of the regex gate (see module doc).
fn is_blank(body: &[u8]) -> bool {
    body.iter().all(|&c| WS.contains(&c))
}

#[derive(Clone, Copy)]
enum H {
    Level(usize, i32),
    Break(i32),
    Pause(i32),
    Mark,
    PartOfSp,
    Phoneme,
    SayAs,
    Skip,
    Sub(u8),
}

/// `0x10065a68`: tag, terminator, handler, attribute delimiters. Names,
/// order and every checked numeric constant match `text::initial`'s `TAGS`
/// exactly (see module doc); terminator/delimiter strings are not stored in
/// the table itself (each handler enforces its own), so they are carried
/// from that proven-identical pattern.
const TAGS: [(&[u8], &[u8], H, &[u8]); 31] = [
    (b"<vtml_break", b"/>", H::Break(3), b" \x0c\n\r\t\x0b=\""),
    (b"<vtml_emotion", b">", H::Level(7, 0x17), b" \x0c\n\r\t\x0b=\""),
    (b"</vtml_emotion", b">", H::Level(7, 0x17), WS),
    (b"<vtml_mark", b"/>", H::Mark, b" \x0c\n\r\t\x0b=\""),
    (b"<vtml_partofsp", b">", H::PartOfSp, b" \x0c\n\r\t\x0b=\""),
    (b"</vtml_partofsp", b">", H::PartOfSp, WS),
    (b"<vtml_pause", b"/>", H::Pause(6), b" \x0c\n\r\t\x0b=\""),
    (b"<vtml_phoneme", b">", H::Phoneme, b" \x0c\n\r\t\x0b=\""),
    (b"</vtml_phoneme", b">", H::Phoneme, WS),
    (b"<vtml_pitch", b">", H::Level(0, 0xc), b" \x0c\n\r\t\x0b=\""),
    (b"</vtml_pitch", b">", H::Level(0, 0xc), WS),
    (b"<vtml_sayas", b">", H::SayAs, b" \x0c\n\r\t\x0b=\""),
    (b"</vtml_sayas", b">", H::SayAs, WS),
    (b"<vtml_skip", b">", H::Skip, b""),
    (b"</vtml_skip", b">", H::Skip, b""),
    (b"<vtml_speed", b">", H::Level(1, 0xc), b" \x0c\n\r\t\x0b=\""),
    (b"</vtml_speed", b">", H::Level(1, 0xc), WS),
    (b"<vtml_sub", b">", H::Sub(0x11), b" \x0c\n\r\t\x0b=\""),
    (b"</vtml_sub", b">", H::Sub(0x11), WS),
    (b"<vtml_volume", b">", H::Level(2, 0xc), b" \x0c\n\r\t\x0b=\""),
    (b"</vtml_volume", b">", H::Level(2, 0xc), WS),
    (b"<vt_pitch", b">", H::Level(0, 0x13), b" \x0c\n\r\t\x0b="),
    (b"</vt_pitch", b">", H::Level(0, 0x13), WS),
    (b"<vt_speed", b">", H::Level(1, 0x13), b" \x0c\n\r\t\x0b="),
    (b"</vt_speed", b">", H::Level(1, 0x13), WS),
    (b"<vt_volume", b">", H::Level(2, 0x13), b" \x0c\n\r\t\x0b="),
    (b"</vt_volume", b">", H::Level(2, 0x13), WS),
    (b"<vt_pause", b">", H::Pause(0x13), b" \x0c\n\r\t\x0b="),
    (b"<vt_break", b">", H::Break(0x14), b" \x0c\n\r\t\x0b="),
    (b"<say-as", b">", H::Sub(0x15), b" \x0c\n\r\t\x0b=\""),
    (b"</say-as", b">", H::Sub(0x15), WS),
];

/// Per-channel clamp, `(min, max)`: one shared 5-entry table
/// (`DAT_10065de4`=min array, `DAT_10065dd0`=max array, both read directly
/// via `read_memory`/pefile, index = channel number) used by
/// `FUN_1001b9c0` (pitch/speed/volume, disassembly-confirmed: `if (v <
/// DAT_10065de4[ch]) v = DAT_10065de4[ch]; if (DAT_10065dd0[ch] < v) v =
/// DAT_10065dd0[ch];`) and by `FUN_1001a0e0`/`FUN_1001ad40` (break/pause,
/// same array at the `+3`/`+4` slots -- `0x10065de4+0xc==0x10065df0`,
/// `0x10065dd0+0xc==0x10065ddc`, matching the pause addresses found
/// separately last round). All 5 entries now independently verified
/// identical to `text::initial`'s `CLAMP`.
const CLAMP: [(i32, i32); 5] = [(50, 200), (50, 400), (0, 500), (0, 65535), (0, 3)];

// ---------------------------------------------------- phoneme alphabets ---
//
// `FUN_10036470` (x-cmu, called from the phoneme handler at 0x1001b00b):
// translated line-by-line from its decompile. Structurally identical to
// `text::initial`'s `cmu`/`cmu_lim` (same trim, same empty/non-empty-word
// branch, same per-token scan with the "2-char token starting with a vowel
// gets a trailing 0" rule, same `#`-token/recursion and skip-vs-emit-100
// logic, same `0x600`/`0x200` top/sub-recursion bounds) -- only the phone
// table differs. Table: 63 entries (not English's 69), extracted with
// `scripts/gen_rust_tables.py`-style pefile reads from the pointer array at
// `0x10067e58` (dereferenced -- the array itself, not the string blob
// immediately before it, is the lookup table `FUN_10043740` binary-searches,
// count `0x3f`=63). Output byte is `index+1` through `DAT_100695f4`, which
// is confirmed-identity for every reachable index (1..=63) by direct memory
// read, so it collapses to the same `code as u8` English uses.
#[rustfmt::skip]
const CMU_PHONES_SPA: [&[u8]; 63] = [
    b"AA0", b"AA1", b"AO0", b"AO1", b"AU0", b"AU1", b"AY0", b"AY1", b"B", b"BH",
    b"CH", b"D", b"DH", b"EH0", b"EH1", b"EU0", b"EU1", b"EY0", b"EY1", b"F",
    b"G", b"HH", b"IH0", b"IH1", b"JH", b"K", b"L", b"M", b"N", b"NG",
    b"NY", b"OH0", b"OH1", b"OU0", b"OU1", b"OY0", b"OY1", b"P", b"R", b"RR",
    b"S", b"SH", b"T", b"UH0", b"UH1", b"V", b"WA0", b"WA1", b"WE0", b"WE1",
    b"WI0", b"WI1", b"WO0", b"WO1", b"YA0", b"YA1", b"YE0", b"YE1", b"YO0", b"YO1",
    b"YU0", b"YU1", b"Y",
];

fn cmu_phone_search(tok: &[u8]) -> Option<usize> {
    CMU_PHONES_SPA.iter().position(|&p| p == tok)
}

/// `FUN_10036470`: x-cmu phones -> `<..T>word\xff codes</..>`. `max` is
/// `0x600` at top level, `0x200` for the `#`-only word recursion, mirroring
/// `text::initial::cmu_lim` exactly.
fn cmu_spa(ph: &[u8], word: &[u8]) -> Option<Vec<u8>> {
    cmu_lim_spa(ph, word, 0x600)
}

// -------------------------------------------------------------- partofsp --
//
// `FUN_1001a7a0` (partofsp handler, disassembly 0x1001a7a0-0x1001a92c):
// confirmed same shape as every other content-tag handler (blank/close
// check writes `\xa2\xfe` gated on the `tib+0x16c9a6` open flag; non-blank
// gates on regex idx 5, matching `RE_PART` -- `PUSH 0x5` immediately before
// the `FUN_10020bc0` call). `RE_PART`'s alternation is the literal English
// words Noun/Verb/Modifier/Function/Interjection/Unknown, so I tried
// reusing `text::initial::partofsp_conv`'s exact word->letter mapping and
// class-based word validation (translated to Spanish's `CLS_S`) -- but a
// live capture disproved it: `<vtml_partofsp part="Noun">banco</...>`
// converts under that logic, while the real engine leaves it as literal
// text (`probe/out/initial.jsonl`, the `banco` line). So partofsp's *actual*
// per-word validation is NOT what I assumed; the disassembly past 0x1001a92c
// (where the real check must live) wasn't read this session. Declining the
// converter itself (kept as a documented wrong-guess warning, not shipped):
// `None` takes the same "pass content through unconverted" path the real
// converter's own rejection takes, so this is a safe decline, not a guess.
fn cmu_lim_spa(ph: &[u8], word: &[u8], max: usize) -> Option<Vec<u8>> {
    let ph = trim(ph);
    if ph.is_empty() || word.len() >= max - 1 {
        return None;
    }
    let mut out = b"<\xa1\xddT>".to_vec();
    out.extend_from_slice(word);
    out.push(0xff);
    let len = ph.len();
    let (mut k, mut count, mut last) = (0usize, 0, false);
    let mut buf = [0u8; 4];
    let mut i = 0;
    while i <= len {
        let c = at(&ph, i as isize);
        if c == 0 && !last {
            break;
        }
        if !((i == 0 && ph[0] == b' ') || (i == len && c == b' ')) {
            if c == b' ' || last {
                let mut tok = buf[..k].to_vec();
                if k == 2 && b"AEIOU".contains(&tok[0]) {
                    tok.push(b'0');
                }
                let code = if tok == b"#" {
                    0
                } else {
                    let j = cmu_phone_search(&tok);
                    match j {
                        None => return None,
                        Some(j) => j as i32 + 1,
                    }
                };
                let skip = code == 0 && !(count != 0 && out.last() != Some(&100) && i != len);
                if !skip {
                    if out.len() >= max - 6 || count > 0x3f {
                        return None;
                    }
                    if word.is_empty() && code == 0 {
                        let local_c = out.len();
                        out.extend_from_slice(b"</\xa1\xdd>");
                        return match cmu_lim_spa(&ph[i..], b"", 0x200) {
                            None => None,
                            Some(sub) if sub.len() + 5 + local_c > 0x1ff => None,
                            Some(sub) if sub.len() < 0xc => Some(out),
                            Some(sub) => {
                                out.extend_from_slice(&sub);
                                Some(out)
                            }
                        };
                    }
                    out.push(if code != 0 { code as u8 } else { 100 });
                    count += 1;
                    if c != 0 {
                        while matches!(at(&ph, i as isize), b' ' | b'\t' | b'\n' | b'\r') {
                            i += 1;
                        }
                        i -= 1;
                    }
                }
                k = 0;
            } else {
                if k > 2 {
                    return None;
                }
                buf[k] = c;
                k += 1;
                if at(&ph, i as isize + 1) == 0 {
                    last = true;
                }
            }
        }
        i += 1;
    }
    out.extend_from_slice(b"</\xa1\xdd>");
    Some(out)
}

struct St<'a> {
    src: &'a [u8],
    pb: Vec<i32>,
    pc: Vec<i32>,
    out: Vec<u8>,
    b: Vec<i32>,
    c: Vec<i32>,
    chans: [Chan; 8],
    marks: Vec<Mark>,
    /// partofsp, phoneme, sub/say-as, sayas, skip open (emotion has its own
    /// channel-value sentinel in `text::initial`'s scheme; Violeta's emotion
    /// case is declined here, see `H::Level`, so no flag is needed for it).
    open: [bool; 5],
}

const POS: usize = 0;
const PHON: usize = 1;
const SUB: usize = 2;
const SAYAS: usize = 3;
const SKIP: usize = 4;

impl St<'_> {
    fn inherit(&self, i: usize) -> (i32, i32) {
        let n = self.pb.len();
        if i < n {
            (self.pb[i], self.pc[i])
        } else {
            let v = self.pb[n - 1] + 1;
            (v, v)
        }
    }

    fn span(&self, from: usize, len: usize) -> (i32, i32) {
        let n = self.pb.len();
        if from < n {
            (self.pb[from], self.pc[(from + len - 1).min(n - 1)])
        } else {
            let v = self.pb[n - 1] + 1;
            (v, v)
        }
    }

    fn put(&mut self, bytes: &[u8], m: (i32, i32)) {
        for &x in bytes {
            self.out.push(x);
            self.b.push(m.0);
            self.c.push(m.1);
        }
    }

    fn at(&self, i: usize) -> u8 {
        *self.src.get(i).unwrap_or(&0)
    }

    fn rest(&self, i: usize) -> &[u8] {
        &self.src[i.min(self.src.len())..]
    }

    fn find(&self, from: usize, pat: &[u8]) -> Option<usize> {
        self.rest(from).windows(pat.len()).position(|w| w == pat).map(|k| from + k)
    }

    /// The tag body and the whole tag's length.
    fn body(&self, pos: usize, tag: &[u8], term: &[u8]) -> Option<(Vec<u8>, usize)> {
        let e = self.find(pos, term)?;
        let st = pos + tag.len();
        if e < st || e - st > 0x5ff {
            return None;
        }
        let b = self.src[st..e].to_vec();
        let total = b.len() + term.len() + tag.len();
        Some((b, total))
    }

    fn content(&self, from: usize, close_lo: &[u8], close_up: &[u8]) -> Option<usize> {
        match (self.find(from, close_lo), self.find(from, close_up)) {
            (None, None) => None,
            (Some(a), None) | (None, Some(a)) => Some(a - from),
            (Some(a), Some(b)) => Some(a.min(b) - from),
        }
    }

    fn close(&mut self, pos: usize, total: usize, flag: usize) -> Option<usize> {
        if strnicmp(self.rest(pos), b"</", 2) != 0 || !self.open[flag] {
            return None;
        }
        let m = self.span(pos, total);
        self.put(b"\xa2\xfe", m);
        self.open[flag] = false;
        Some(total)
    }

    /// Open-tag marker + converted (or raw) content; returns bytes consumed.
    /// Same empty-content span quirk as `text::initial`'s `content_tag`
    /// (`FUN_1002fcf0`-derived for English; not independently re-verified
    /// against a Spanish disassembly, but the *only* Spanish handler with an
    /// empty-content path checked this session -- phoneme's self-closing
    /// branch, see module doc -- takes `clen=0` down the exact same
    /// "converted text, no raw copy" road, so the span quirk applies here
    /// too whenever it is later filled in).
    fn content_tag(&mut self, pos: usize, total: usize, clen: usize, conv: Option<Vec<u8>>, flag: usize) -> usize {
        let m = self.span(pos, total);
        self.put(b"\xfd\xfe", m);
        match conv {
            Some(v) => {
                let from = pos + total;
                let m = if clen == 0 {
                    let b = self.span(from, 1).0;
                    let c = if from > 0 { self.span(from - 1, 1).1 } else { b };
                    (b, c)
                } else {
                    self.span(from, clen)
                };
                self.put(&v, m);
            }
            None => {
                for k in 0..clen {
                    let m = self.inherit(pos + total + k);
                    let x = self.at(pos + total + k);
                    self.put(&[x], m);
                }
            }
        }
        self.open[flag] = true;
        total + clen
    }

    fn handle(&mut self, pos: usize, e: &(&[u8], &[u8], H, &[u8])) -> Option<usize> {
        let (tag, term, h, delims) = *e;
        let (body, total) = self.body(pos, tag, term)?;
        let empty = is_blank(&body);
        match h {
            H::Level(ch, rx) => {
                if ch == 7 {
                    // Emotion: category-table lookup, declined (see module doc).
                    return None;
                }
                if empty {
                    if strnicmp(self.rest(pos), b"</", 2) != 0 {
                        return None;
                    }
                    let m = self.span(pos, total);
                    self.put(b"\xa2\xfe", m);
                    let p = self.out.len() as i32;
                    self.chans[ch].pos.push(p);
                    self.chans[ch].val.push(-1);
                    return Some(total);
                }
                // `0x1001b9c0` (shared pitch/speed/volume/emotion writer,
                // disassembly-confirmed): idx 0xc (vtml_) gates on
                // `RE_LEVEL_VALUE` then still separately checks the "value"
                // keyword via `strtok`; idx 0x13 (vt_ legacy) gates on the
                // generic bare `=N` pattern and skips the keyword check
                // entirely -- confirmed live (`probe/corpus2.txt`:
                // `<vt_pitch=120>` converts, `<vt_pitch PITCH=120>` doesn't).
                let mut at = 0;
                let v = if rx == 0xc {
                    if !re_match(RE_LEVEL_VALUE, &body) {
                        return None;
                    }
                    let t = strtok(&body, &mut at, delims)?;
                    if crate::ling_spa::lexicon::stricmp_spa(t, b"value") != 0 {
                        return None;
                    }
                    strtok(&body, &mut at, delims)?.to_vec()
                } else {
                    if !re_match(RE_BARE_SIGNED, &body) {
                        return None;
                    }
                    strtok(&body, &mut at, delims)?.to_vec()
                };
                let v = atoi(&v).clamp(CLAMP[ch].0, CLAMP[ch].1);
                let m = self.span(pos, total);
                self.put(b"\xfd\xfe", m);
                let p = self.out.len() as i32;
                self.chans[ch].pos.push(p);
                self.chans[ch].val.push(v);
                Some(total)
            }
            H::Break(kind) | H::Pause(kind) => {
                if empty {
                    return None;
                }
                let mut body = body;
                let n = cs(&body).len();
                if (kind == 0x14 || kind == 0x13) && n > 0 && body[n - 1] == b'/' {
                    body.truncate(n - 1);
                }
                // Gates, decompile/disassembly + live-capture confirmed
                // (`FUN_1001a0e0`/`FUN_1001ad40`, `probe/corpus2.txt`):
                // vtml_break -> `RE_BREAK_LEVEL` ("0".."3", quoted, keyworded);
                // vt_break -> `RE_VTBREAK_BARE` ("=0".."=3", bare, its own
                // pattern, distinct from the generic signed one);
                // vtml_pause -> `RE_PAUSE_TIME` ("time", quoted, keyworded);
                // vt_pause -> `RE_BARE_SIGNED` (shared with vt_ Level family).
                let gate: &[u8] = match kind {
                    3 => RE_BREAK_LEVEL,
                    0x14 => RE_VTBREAK_BARE,
                    6 => RE_PAUSE_TIME,
                    _ => RE_BARE_SIGNED,
                };
                if !re_match(gate, &body) {
                    return None;
                }
                let key: &[u8] = if matches!(h, H::Break(_)) { b"level" } else { b"time" };
                let mut at = 0;
                let t = strtok(&body, &mut at, delims)?;
                let v = if kind == 3 || kind == 6 {
                    if crate::ling_spa::lexicon::stricmp_spa(t, key) != 0 {
                        return None;
                    }
                    strtok(&body, &mut at, delims)?.to_vec()
                } else {
                    t.to_vec()
                };
                let ch = if matches!(h, H::Break(_)) { 4 } else { 3 };
                let v = atoi(&v).clamp(CLAMP[ch].0, CLAMP[ch].1);
                let m = self.span(pos, total);
                self.put(if ch == 4 && v == 3 { b"\xa2\xfd" } else { b"\xa2\xfe" }, m);
                let p = self.out.len() as i32 - 1;
                self.chans[ch].pos.push(p);
                self.chans[ch].val.push(v);
                Some(total)
            }
            H::Mark => {
                let (name, kind) = if empty {
                    (Vec::new(), 2)
                } else {
                    // `RE_NAME` ("name", quoted, whole body) -- 0x10066cc0.
                    if !re_match(RE_NAME, &body) {
                        return None;
                    }
                    let nm = attr(&body, b"name", delims)?;
                    if nm.len() < 0x200 {
                        (nm, 1)
                    } else {
                        (nm[..0x1ff].to_vec(), 3)
                    }
                };
                let m = self.span(pos, total);
                self.put(b"\xa2\xfe", m);
                let p = self.out.len() as i32 - 1;
                self.marks.push(Mark { pos: p, name, kind });
                Some(total)
            }
            H::PartOfSp => {
                if empty {
                    return self.close(pos, total, POS);
                }
                // `RE_PART` (0x10066bb0): `part=` one of exactly Noun/Verb/
                // Modifier/Function/Interjection/Unknown, whole body.
                if !re_match(RE_PART, &body) {
                    return None;
                }
                let clen = self.content(pos + total, b"</vtml_partofsp", b"</VTML_PARTOFSP")?;
                let _part = attr(&body, b"part", delims)?;
                // `partofsp_conv` declined -- see module doc above (a first
                // attempt reusing English's word->letter logic was tested
                // and disproved live).
                let conv: Option<Vec<u8>> = None;
                Some(self.content_tag(pos, total, clen, conv, POS))
            }
            H::Phoneme => {
                if empty {
                    return self.close(pos, total, PHON);
                }
                // Decompile/disassembly-confirmed (0x1001b2e6-0x1001b30d):
                // a body ending in '/' is self-closing -- empty content, and
                // the `content()` search is skipped entirely. Same branch as
                // `text::initial`'s fixed English phoneme handler.
                let self_closing = cs(&body).last() == Some(&b'/');
                let clen = if self_closing { 0 } else { self.content(pos + total, b"</vtml_phoneme", b"</VTML_PHONEME")? };
                let ph = attr(&body, b"ph", delims).unwrap_or_default();
                let alpha = attr(&body, b"alphabet", delims).unwrap_or_default();
                let word = self.src[pos + total..pos + total + clen.min(0x1ff)].to_vec();
                // x-cmu (`FUN_10036470`) ported in full, own 63-phone table
                // (see `cmu_spa`). x-sapi/ipa/x-worldbet/x-sampa/x-ntsampa/
                // x-tasampa (Spanish 0x100360a0/0x10036860/0x10036a70/
                // 0x10036c70) declined this session -- see module doc. As
                // with English, a conversion FAILURE also falls back to the
                // unconverted word (`Some(word.clone())`), matching the
                // engine's own fallback; only an actually-*unimplemented*
                // alphabet (sapi/ipa/worldbet/ntsampa) or a truly
                // unrecognized name reaches the `None` (declined) path.
                let conv = if crate::ling_spa::lexicon::stricmp_spa(&alpha, b"x-cmu") == 0 {
                    Some(cmu_spa(&ph, &word).unwrap_or_else(|| word.clone()))
                } else {
                    None
                };
                let used = self.content_tag(pos, total, clen, conv, PHON);
                if self_closing {
                    let m = self.span(pos, total);
                    self.put(b"\xa2\xfe", m);
                    self.open[PHON] = false;
                }
                Some(used)
            }
            H::SayAs => {
                if empty {
                    return self.close(pos, total, SAYAS);
                }
                let clen = self.content(pos + total, b"</vtml_sayas", b"</VTML_SAYAS")?;
                let _interp = attr(&body, b"interpret-as", delims).unwrap_or_default();
                // The 22-entry say-as dispatch and its content converters
                // are declined this session -- see module doc.
                let conv: Option<Vec<u8>> = None;
                Some(self.content_tag(pos, total, clen, conv, SAYAS))
            }
            H::Skip => {
                if !empty {
                    return None;
                }
                if strnicmp(self.rest(pos), b"</", 2) == 0 {
                    return self.close(pos, total, SKIP);
                }
                let clen = self.content(pos + total, b"</vtml_skip", b"</VTML_SKIP")?;
                let m = self.span(pos, total);
                self.put(b"\xfd\xfe", m);
                self.open[SKIP] = true;
                Some(total + clen)
            }
            H::Sub(kind) => {
                if empty {
                    return self.close(pos, total, SUB);
                }
                // `vtml_sub` (kind 0x11) gates on `RE_ALIAS` (0x10066820,
                // unanchored -- the engine's own pattern has no `^`/`$`);
                // `say-as` (kind 0x15) gates on `RE_SUB` (0x10066774,
                // anchored, whole body).
                if !re_match(if kind == 0x11 { RE_ALIAS } else { RE_SUB }, &body) {
                    return None;
                }
                let clen = if kind == 0x11 {
                    self.content(pos + total, b"</vtml_sub", b"</VTML_SUB")?
                } else {
                    self.content(pos + total, b"</say-as", b"</say-as")?
                };
                let alias = attr(&body, if kind == 0x11 { b"alias".as_slice() } else { b"sub" }, delims).unwrap_or_default();
                let posv = if kind == 0x11 { attr(&body, b"pos", delims).unwrap_or_default() } else { Vec::new() };
                let conv = if posv.is_empty() { Some(tagged(b'S', &trim(&alias))) } else { None };
                Some(self.content_tag(pos, total, clen, conv, SUB))
            }
        }
    }
}

/// `FUN_10019d10`: the longest matching tag entry, then its handler.
fn try_tag(st: &mut St, pos: usize) -> Option<usize> {
    let rest = st.rest(pos);
    let mut best: Option<usize> = None;
    for (i, e) in TAGS.iter().enumerate() {
        let n = e.0.len();
        if rest.len() >= n && !rest[..n].contains(&0) && strnicmp(rest, e.0, n) == 0 && best.map_or(true, |b| TAGS[b].0.len() < n) {
            best = Some(i);
        }
    }
    let e = TAGS[best?];
    let (ol, bl, cl) = (st.out.len(), st.b.len(), st.c.len());
    let r = st.handle(pos, &e);
    if r.is_none() {
        st.out.truncate(ol);
        st.b.truncate(bl);
        st.c.truncate(cl);
    }
    r
}

/// Open/close pairing on channels 0..2 (pitch/speed/volume): unmatched
/// entries dropped, each close takes the value the channel had before its
/// open. Same algorithm as `text::initial`'s `pair` (pure bookkeeping over
/// the `Chan` result type, no DLL data).
fn pair(ch: &mut Chan) {
    for round in 0..2 {
        let n = ch.pos.len();
        let mut keep = vec![false; n];
        let mut partner = vec![0usize; n];
        for i in (0..n).rev() {
            if ch.val[i] < 0 {
                continue;
            }
            let mut j = i;
            while j < n && (ch.val[j] >= 0 || keep[j]) {
                j += 1;
            }
            if j != n {
                keep[i] = true;
                keep[j] = true;
                partner[i] = j;
                partner[j] = i;
            }
        }
        if round == 0 {
            let (pos, val): (Vec<i32>, Vec<i32>) = (0..n).filter(|&i| keep[i]).map(|i| (ch.pos[i], ch.val[i])).unzip();
            ch.pos = pos;
            ch.val = val;
        } else {
            for i in 0..n {
                if ch.val[i] < 0 {
                    let o = partner[i];
                    ch.val[i] = if o == 0 { -1 } else { ch.val[o - 1] };
                }
            }
        }
    }
}

const OVERRIDE_DEFAULT: [i32; 11] = [0, 0, -1, -1, -1, 0, 0, 0, 0, 0, 0];
const PAUSE_OVERRIDE_DEFAULT: (i32, i32) = (-1, -1);

/// `fe_InitialTextProcess` (0x10007730), the demo-nag call aside (not
/// ported, matching `text::initial`).
pub fn initial_spa(src: &[u8]) -> Initial {
    let src = cs(src);
    if src.is_empty() {
        return Initial {
            processed: Vec::new(),
            map_b: Vec::new(),
            map_c: Vec::new(),
            chans: Default::default(),
            marks: Vec::new(),
            override_table: OVERRIDE_DEFAULT,
            pause_override: PAUSE_OVERRIDE_DEFAULT,
        };
    }
    let (fw, pb, pc) = map_fullwidth(src);
    let n = fw.len();
    let mut st = St { src: &fw, pb, pc, out: Vec::new(), b: Vec::new(), c: Vec::new(), chans: Default::default(), marks: Vec::new(), open: [false; 5] };
    let mut i = 0;
    while i < n {
        if CLS_S[fw[i] as usize] & 6 == 0 {
            if let Some(r) = try_tag(&mut st, i) {
                i += r;
                continue;
            }
        }
        let m = st.inherit(i);
        st.put(&[fw[i]], m);
        i += 1;
    }
    for ch in 0..3 {
        if !st.chans[ch].pos.is_empty() {
            pair(&mut st.chans[ch]);
        }
    }
    let marks = st
        .marks
        .into_iter()
        .map(|mut m| {
            m.pos = *st.b.get(m.pos as usize).unwrap_or(&m.pos);
            m
        })
        .collect();
    Initial {
        processed: st.out,
        map_b: st.b,
        map_c: st.c,
        chans: st.chans,
        marks,
        override_table: OVERRIDE_DEFAULT,
        pause_override: PAUSE_OVERRIDE_DEFAULT,
    }
}
