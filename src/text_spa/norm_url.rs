//! `FUN_100486b0` (chain slot 1): URLs and e-mail addresses.
//!
//! Ported: the core piece-capture loop (`FUN_10049980`, here `dotpiece`),
//! classification against the three domain-suffix tables (all three, plus
//! the 8 shape templates and the phone-adjacent tables, transcribed with
//! `C:\tmp\vtwork\v4c_symbols\gen_rust_tables.py` via `pefile` -- never by
//! hand), shape matching with shrink-and-retry (`CC_TLD`/`GENERIC_TLD`/
//! `SECOND_TLD`/`URL_SHAPES`), the per-entry render (`FUN_100490a0`, spelling
//! each piece via `spell::expand`), and, from `FUN_100494c0`, the narrow "a
//! lone `WWW` immediately followed by `.`" prefix case (`www_prefix`):
//! confirmed by capture that the engine speaks `WWW` (literal, uppercase, its
//! own span) and the following `.` as two records on their own, then
//! re-enters the chain on what's left -- so this just speaks those two and
//! returns, letting the driver's own retry loop hit the general capture loop
//! above for the remaining domain.
//!
//! Also ported: `FUN_100494c0`'s `"://"`-prefixed scheme fast path
//! (`scheme_prefix`, restricted to the 4 protocols whose spoken CASE byte the
//! engine actually initializes -- FTP/HTTP/NNTP/WWW, indices 1/3/6/10 of
//! `PROTOCOLS` -- see below) with `FUN_100491d0`'s continuation speller
//! (`url_continuation`, reusing `spell::expand` exactly as the engine's own
//! `FUN_10035880` calls do); and the `FUN_100493f0` gate exposed by
//! `FUN_10049740` (`port_continuation`, detection only -- see below): the
//! `D.D.D.D` (bare IPv4) shape only ever renders when a `:port` immediately
//! follows -- ported as-is, a genuine engine quirk (or bug) confirmed by
//! capture, and the reason a bare "192.168.1.1" is read as a cardinal number
//! and the sentence splits at the first `.` (`punct.rs`'s `rules()` has no
//! "keep going" exception for digit-dot-digit, only alpha-dot-*).
//!
//! Declined (not attempted, falls through to a plain-word/symbol reading of
//! whatever's left): the other 7 `FUN_100494c0` protocols (FILE/GOPHER/
//! MAILTO/NEWS/PROSPERO/TELNET/WAIS) -- the engine leaves their spoken CASE
//! byte as uninitialized stack garbage (`local_4ef` is never assigned for
//! them), so there is no defined behaviour to reproduce; the two 1-2-word
//! lookback branches that fold a preceding "www"-like word already spoken by
//! an earlier call into the match (`FUN_1004bd70`+`FUN_1004bfe0`, gated on
//! the previous WA record's `rec::SRC == 7` and `rec::END` matching the
//! looked-back word's own end -- no code path in this port ever emits
//! `SRC == 7`, and guessing which one the engine's fuller `FUN_100494c0`/
//! render pipeline uses it for risks a wrong, unverifiable fold); what
//! `FUN_100491d0`'s bare-`/` continuation does after speaking "barra" and
//! setting `param_1[4] = 1` -- byte offset 8 of the same WA/FE pointer as
//! `TAG_MODE`, so a tempting guess, but wiring it to `tagmode::tag_mode`
//! (whose own doc comment already expected a URL normalizer to do exactly
//! this) does NOT reproduce the engine: a capture of
//! "http://example.com/index.html ..." shows the engine ending the SENTENCE
//! right after "barra" and starting a fresh one at "index.html" (matched
//! by `url()` itself again, as "index"/"punto"/"html"), whereas routing
//! through this port's existing `tag_mode()` instead speaks "index.html" as
//! one joined word and keeps the same sentence going -- so whatever
//! `param_1[4] = 1` actually drives here is NOT this codebase's `TAG_MODE`
//! (or `tag_mode()` itself has a pre-existing gap for a word.word path
//! piece); left unset, declining a path/query string after the first bare
//! `/` (matching everything up to and including that `/`, then falling
//! through) rather than guess further; `port_continuation`'s own RENDER (`FUN_100490a0`'s tail, speaking
//! `FUN_1004ca30`'s output over the port number's span) -- `FUN_1004ca30`
//! IS `numwords::cardinal_to_words` (re-derived from its own disassembly,
//! matching the existing port bit-for-bit: plain digits, no comma/period
//! grouping, go straight to `FUN_1004d080(...,'L',...)` = `group_words`) and
//! should give "ocho mil ochenta" for a captured "8080", but the engine's
//! OWN capture instead shows digit-by-digit ("ocho cero ocho cero") --
//! meaning `FUN_10049740` is either not the code path this input actually
//! takes, or something upstream of it changes what reaches `FUN_1004ca30`;
//! either way this is UNRESOLVED, so `port_continuation` is used only to
//! detect a port for the `D.D.D.D` gate above, never to render or consume
//! one. A bare "www.example.com", "example.com", "user@example.com",
//! "http://.../path" and "host.com:port" (port detected but left for the
//! chain to read on its own, exactly as before this round) are unaffected by
//! any of these -- confirmed against engine captures.

use super::emit;
use super::spell;
use super::tokenize::{tokenize, Tok};
use super::FeSpa;
use crate::ling_spa::lexicon::stricmp_spa;
use crate::ling_spa::WaSpa;
use crate::text::cstr::{at, atoi};

fn digit(c: u8) -> bool {
    use super::tables::CLS_S;
    CLS_S[c as usize] & 0x10 != 0
}

fn alnum(c: u8) -> bool {
    use super::tables::CLS_S;
    CLS_S[c as usize] & 0xd0 != 0
}

fn is_all_digit(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&c| digit(c))
}

const CC_TLD: [&[u8]; 247] = [
    b"ac", b"ad", b"ae", b"af", b"ag", b"ai", b"al", b"am", b"an", b"ao", b"aq", b"ar", b"as", b"at", b"au", b"aw", b"az", b"ba", b"bb", b"bd",
    b"be", b"bf", b"bg", b"bh", b"bi", b"bj", b"bm", b"bn", b"bo", b"br", b"bs", b"bt", b"bv", b"bw", b"by", b"bz", b"ca", b"cc", b"cd", b"cf",
    b"cg", b"ch", b"ci", b"ck", b"cl", b"cm", b"cn", b"co", b"cr", b"cs", b"cu", b"cv", b"cx", b"cy", b"cz", b"de", b"dj", b"dk", b"dm", b"do",
    b"dz", b"ec", b"ee", b"eg", b"eh", b"er", b"es", b"et", b"fi", b"fj", b"fk", b"fm", b"fo", b"fr", b"fx", b"ga", b"gd", b"ge", b"gf", b"gg",
    b"gh", b"gi", b"gl", b"gm", b"gn", b"gp", b"gq", b"gr", b"gs", b"gt", b"gu", b"gw", b"gy", b"hk", b"hm", b"hn", b"hr", b"ht", b"hu", b"id",
    b"ie", b"il", b"im", b"in", b"io", b"iq", b"ir", b"is", b"it", b"je", b"jm", b"jo", b"jp", b"ke", b"kg", b"kh", b"ki", b"km", b"kn", b"kp",
    b"kr", b"kw", b"ky", b"kz", b"la", b"lb", b"lc", b"li", b"lk", b"lr", b"ls", b"lt", b"lu", b"lv", b"ly", b"ma", b"mc", b"md", b"mg", b"mh",
    b"mk", b"ml", b"mm", b"mn", b"mo", b"mp", b"mq", b"mr", b"ms", b"mt", b"mu", b"mv", b"mw", b"mx", b"my", b"mz", b"na", b"nc", b"ne", b"nf",
    b"ng", b"ni", b"nl", b"no", b"np", b"nr", b"nt", b"nu", b"nz", b"om", b"pa", b"pe", b"pf", b"pg", b"ph", b"pk", b"pl", b"pm", b"pn", b"pr",
    b"pt", b"pw", b"py", b"qa", b"re", b"ro", b"ru", b"rw", b"sa", b"sb", b"sc", b"sd", b"se", b"sg", b"sh", b"si", b"sj", b"sk", b"sl", b"sm",
    b"sn", b"so", b"sr", b"st", b"su", b"sv", b"sy", b"sz", b"tc", b"td", b"tf", b"tg", b"th", b"tj", b"tk", b"tm", b"tn", b"to", b"tp", b"tr",
    b"tt", b"tv", b"tw", b"tz", b"ua", b"ug", b"uk", b"um", b"us", b"uy", b"uz", b"va", b"vc", b"ve", b"vg", b"vi", b"vn", b"vu", b"wf", b"ws",
    b"ye", b"yt", b"yu", b"za", b"zm", b"zr", b"zw",
];

const GENERIC_TLD: [&[u8]; 14] = [b"arts", b"com", b"edu", b"firm", b"gov", b"info", b"int", b"mil", b"net", b"nom", b"org", b"rec", b"shop", b"web"];

const SECOND_TLD: [&[u8]; 6] = [b"ac", b"co", b"go", b"ne", b"or", b"re"];

/// `PTR_DAT_1006f784`: protocol/scheme names (index 10 = "WWW", used by
/// `www_prefix`; the rest belong to the unported `://` fast path).
const PROTOCOLS: [&[u8]; 11] = [b"FILE", b"FTP", b"GOPHER", b"HTTP", b"MAILTO", b"NEWS", b"NNTP", b"PROSPERO", b"TELNET", b"WAIS", b"WWW"];

/// `PTR_DAT_1007a808`, `DAT_1007a828`(=8): the 8 recognized domain shapes.
const URL_SHAPES: [&[u8]; 8] = [b"D.D.D.D", b"H@H.H.N", b"H@H.U.N", b"H@H.U", b"H.H.U.N", b"H.H.U", b"H.U.N", b"H.U"];

fn table_find(table: &[&[u8]], key: &[u8]) -> Option<usize> {
    table.iter().position(|&w| stricmp_spa(w, key) == 0)
}

/// One captured dot/at-separated piece (`FUN_10049980`'s output struct).
#[derive(Clone, Default)]
struct Piece {
    start: i32,
    len: i32,
    consumed: i32,
    class: i32,
    text: Vec<u8>,
}

/// `FUN_10049980(out, t+p, off, first)`: the next piece -- a run of
/// alnum-or-embedded-hyphen chars (dropping a trailing dangling hyphen), or
/// (if not a run) the single non-alnum char at `p`. `first` additionally
/// skips leading whitespace and recognizes a paragraph break / end of text.
fn dotpiece(t: &[u8], p: usize, off: i32, first: bool) -> (Piece, i32) {
    let mut piece = Piece::default();
    let mut i: i32 = 0;
    if first {
        let mut nlc = 0;
        loop {
            let c = at(t, p as isize + i as isize);
            if c == b' ' || c == b'\t' {
            } else if c == b'\n' {
                nlc += 1;
            } else if c != b'\r' {
                break;
            }
            i += 1;
        }
        if nlc >= 2 {
            piece.class = 9;
            return (piece, 0);
        }
        if at(t, p as isize + i as isize) == 0 {
            piece.class = 10;
            return (piece, 0);
        }
    } else {
        let c = at(t, p as isize);
        let c1 = at(t, p as isize + 1);
        if c == 0 || c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' || (c == 0xa2 && c1 == 0xfe) {
            return (piece, 0);
        }
    }
    piece.start = off + i;
    let c0 = at(t, p as isize + i as isize);
    if !alnum(c0) {
        piece.text = vec![c0];
        piece.class = 3;
        piece.len = 1;
        piece.consumed = i + 1;
        return (piece, i + 1);
    }
    let mut buf = Vec::with_capacity(64);
    loop {
        let c = at(t, p as isize + i as isize);
        if !alnum(c) {
            if c != b'-' {
                break;
            }
        }
        let c1 = at(t, p as isize + i as isize + 1);
        if buf.len() + 1 > 0x3f || (c == b'-' && c1 != 0 && !alnum(c1)) {
            break;
        }
        buf.push(c);
        i += 1;
    }
    piece.class = if is_all_digit(&buf) { 2 } else { 1 };
    piece.len = buf.len() as i32;
    piece.consumed = i;
    piece.text = buf;
    (piece, i)
}

/// `FUN_10049bf0`: is the char at `t[p..]` a clean place to stop this piece
/// and try matching it as a final (country-code) element -- end of text,
/// whitespace, `/`, `:`, or a `.` immediately before end/whitespace.
fn clean_boundary(t: &[u8], p: isize) -> bool {
    let c = at(t, p);
    if c == 0 || c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' || (c == 0xa2 && at(t, p + 1) == 0xfe) {
        return true;
    }
    if c != b'.' {
        return c == b'/' || c == b':';
    }
    let c1 = at(t, p + 1);
    c1 == 0 || c1 == b' ' || c1 == b'\t' || c1 == b'\n' || c1 == b'\r' || (c1 == 0xa2 && at(t, p + 2) == 0xfe)
}

#[derive(Clone, Default)]
struct Entry {
    start: i32,
    pos_after: i32,
    kind: u8,
    /// `local_73c`'s transient 'u' marker: a `SECOND_TLD` match, which (unlike
    /// a `GENERIC_TLD`/`CC_TLD` match) may not be followed by another `.`.
    second_level: bool,
    text: Vec<u8>,
    matched: Vec<u8>,
}

/// `FUN_100486b0`'s capture loop: classify dot/at-separated pieces starting
/// at `t[p..]` into `Entry`s with a shape-code letter each ('D' digit, 'U'
/// TLD/second-level match, 'N' country-code match, 'H' plain host word, or
/// the literal `.`/`@` separator char), stopping on `/`/`:`, an invalid
/// separator, or an unrecognized piece.
fn capture(t: &[u8], p: usize, off: i32) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    let mut digit_groups = 0;
    let (mut cur_p, mut cur_off) = (p, off);
    loop {
        let (piece, consumed) = dotpiece(t, cur_p, cur_off, entries.is_empty());
        if consumed == 0 || piece.class >= 9 {
            break;
        }
        let idx = entries.len();
        let mut kind = 0u8;
        let mut matched = Vec::new();
        let mut second_level = false;
        if idx >= 1 && piece.len == 1 && piece.text[0] != 0 && b":@./".contains(&piece.text[0]) {
            let c = piece.text[0];
            if b"/:".contains(&c) {
                break;
            }
            if c == b'@' {
                if idx != 1 || entries[0].kind != b'H' {
                    break;
                }
            } else {
                // '.': reject two content-free entries in a row, a second
                // `.` after a second-level (SECOND_TLD) match, and a
                // trailing `.` (nothing meaningful follows) right after a
                // country-code (CC_TLD) match.
                let even = idx % 2 == 0;
                let prev = &entries[idx - 1];
                let prev_ok = matches!(prev.kind, b'D' | b'H') || (prev.kind == b'U' && !prev.second_level);
                if even || !prev_ok {
                    break;
                }
                if prev.kind == b'N' && clean_boundary(t, cur_p as isize) {
                    break;
                }
            }
            kind = c;
        } else if digit_groups < 4 && is_all_digit(&piece.text) {
            kind = b'D';
            digit_groups += 1;
        } else {
            let peek = clean_boundary(t, (cur_p as i32 + consumed) as isize);
            let mut found = false;
            if peek {
                if let Some(i) = table_find(&CC_TLD, &piece.text) {
                    kind = b'N';
                    matched = CC_TLD[i].to_vec();
                    found = true;
                }
            }
            if !found {
                if let Some(i) = table_find(&GENERIC_TLD, &piece.text) {
                    kind = b'U';
                    matched = GENERIC_TLD[i].to_vec();
                    found = true;
                } else if let Some(i) = table_find(&SECOND_TLD, &piece.text) {
                    kind = b'U';
                    second_level = true;
                    matched = SECOND_TLD[i].to_vec();
                    found = true;
                }
            }
            if !found {
                if piece.len >= 2 && piece.class == 1 {
                    kind = b'H';
                } else {
                    break;
                }
            }
        }
        entries.push(Entry { start: piece.start, pos_after: cur_off + consumed, kind, second_level, text: piece.text, matched });
        cur_p += consumed as usize;
        cur_off += consumed;
        if entries.len() >= 9 {
            break;
        }
    }
    entries
}

/// `FUN_100498e0`+`FUN_10050528`: each `.`-joined piece parses (C `atoi`) to
/// 0..=255.
fn valid_ipv4(entries: &[Entry]) -> bool {
    entries.iter().step_by(2).all(|e| {
        let v = atoi(&e.text);
        !e.text.is_empty() && (0..=255).contains(&v)
    })
}

/// `FUN_10049740`: is there a `:` immediately (no gap) followed by an
/// immediately (no gap) all-digit piece right here -- a port number. Used
/// ONLY as a detector for the `D.D.D.D` gate (see the module doc comment for
/// why rendering/consuming it is declined).
fn port_continuation(t: &[u8], p: usize, off: i32) -> bool {
    let (colon, r0) = tokenize(t, p, off, 9);
    if r0 == 0 || colon.ws != 0 || colon.class != 3 || colon.len != 1 || colon.text() != b":" {
        return false;
    }
    let (port, r1) = tokenize(t, p + r0 as usize, off + r0, 9);
    r1 != 0 && port.ws == 0 && port.class == 2 && is_all_digit(port.text())
}

/// `s_____________________1007a878`: URL-safe punctuation `FUN_100491d0`'s
/// continuation loop will speak; anything else stops it (unconsumed).
const CONT_SYMBOLS: &[u8] = b";/?:@&=+$,-_.!~*#\\%";

/// `FUN_100491d0`: after a matched `"scheme://"` prefix, keep speaking each
/// immediately-glued (`ws == 0`) mode-1 token -- a CC_TLD/SECOND_TLD word
/// spelled letter by letter (`'S'`), everything else (other words, numbers,
/// URL-safe symbols) with `'D'` -- until whitespace, EOF, a `.` immediately
/// before end/whitespace, a symbol outside `CONT_SYMBOLS`, or the 0x823-byte
/// safety cap. A lone `/` is spoken and ends the continuation right there,
/// setting `param_1[4] = 1` -- `TAG_MODE` (`WA+8`): `tagmode.rs`'s own doc
/// comment already expected this ("a URL path spelled piece by piece after
/// its own normalizer set the flag; ported with the URL normalizer") --
/// `tag_mode()` takes over from the driver loop on the next iteration and
/// speaks the rest of the path (e.g. "index.html") piece by piece.
fn url_continuation(fe: &FeSpa, wa: &mut WaSpa, t: &[u8], p: usize, off: i32, start_total: i32) -> i32 {
    let mut cur_p = p;
    let mut cur_off = off;
    let mut total = start_total;
    if at(t, cur_p as isize) == 0 {
        return total;
    }
    loop {
        let c = at(t, cur_p as isize);
        if matches!(c, b' ' | 9 | 10 | 13) || total >= 0x823 {
            break;
        }
        let (tok, r) = tokenize(t, cur_p, cur_off, 1);
        if r == 0 || tok.ws >= 1 || tok.class == 10 {
            break;
        }
        let text = tok.text();
        let c0 = text.first().copied().unwrap_or(0);
        let default_case: u8;
        if tok.class == 3 && c0 == b'/' {
            if tok.len == 1 {
                if spell::expand(fe, wa, text, tok.start, tok.end, b'D') == 0 {
                    return 0;
                }
                return total + r;
            }
            default_case = b'D';
        } else {
            if c0 == b'.' {
                let after = at(t, cur_p as isize + tok.consumed as isize);
                if matches!(after, 0 | b' ' | 9 | 10 | 13) {
                    break;
                }
            }
            if tok.class != 1 {
                if tok.class == 3 && (c0 == 0 || !CONT_SYMBOLS.contains(&c0)) {
                    break;
                }
                default_case = b'D';
            } else {
                let found = table_find(&CC_TLD, text).is_some() || table_find(&SECOND_TLD, text).is_some();
                default_case = if found { b'S' } else { b'D' };
            }
        }
        if spell::expand(fe, wa, text, tok.start, tok.end, default_case) == 0 {
            return 0;
        }
        cur_p += tok.consumed as usize;
        cur_off += tok.consumed;
        total += tok.consumed;
        if at(t, cur_p as isize) == 0 {
            return total;
        }
    }
    total
}

/// `SCHEME_SPOKEN_CASE`: indices into `PROTOCOLS` (FTP, HTTP, NNTP, WWW) for
/// which `FUN_100494c0` actually initializes the spoken protocol name's CASE
/// byte (`local_4ef = 'S'`); for the other 7 protocols it is left as
/// uninitialized stack garbage -- declined, see the module doc comment.
const SCHEME_SPOKEN_CASE: [usize; 4] = [1, 3, 6, 10];

/// `s________________1007a898`: punctuation `FUN_100494c0`'s own scan loop
/// keeps swallowing (glued, no gap) right after `"scheme://"`, before
/// `FUN_100491d0` takes over -- in practice almost always a no-op, since the
/// first real content piece of a URL is a word/number, not one of these.
const SCHEME_SCAN_PUNCT: &[u8] = b"{}[]();:.,!?'\"";

struct SchemeMatch {
    proto_start: i32,
    proto_end: i32,
    canon: Vec<u8>,
    /// The `"://"` separator's own span -- spoken separately (`FUN_100486b0`
    /// speaks slot0 `local_4c2`=protocol, THEN slot1's text via
    /// `spell::expand`, which is where "://" becomes "dos puntos barra
    /// barra"; found by capture, not derivable from the protocol's own span).
    sep_start: i32,
    sep_end: i32,
    consumed: i32,
}

/// `FUN_100494c0`'s `"://"` fast path: `PROTOCOLS[idx]` (case-insensitively,
/// restricted to `SCHEME_SPOKEN_CASE`) immediately followed by the literal
/// `"://"` (mode-9 `tokenize` recognizes it as one 3-byte token).
fn scheme_prefix(t: &[u8], p: usize, off: i32) -> Option<SchemeMatch> {
    let (proto, r0) = tokenize(t, p, off, 9);
    if proto.len == 0 {
        return None;
    }
    let idx = table_find(&PROTOCOLS, proto.text())?;
    if !SCHEME_SPOKEN_CASE.contains(&idx) {
        return None;
    }
    let (sep, r1) = tokenize(t, p + r0 as usize, off + r0, 9);
    if sep.text() != b"://" {
        return None;
    }
    let mut consumed = r0 + r1;
    loop {
        let (next, r) = tokenize(t, p + consumed as usize, off + consumed, 9);
        if r == 0 || next.len < 1 || next.ws != 0 {
            break;
        }
        let c0 = next.text().first().copied().unwrap_or(0);
        if c0 == 0 || !SCHEME_SCAN_PUNCT.contains(&c0) {
            break;
        }
        consumed += r;
    }
    Some(SchemeMatch { proto_start: proto.start, proto_end: proto.end, canon: PROTOCOLS[idx].to_vec(), sep_start: sep.start, sep_end: sep.end, consumed })
}

/// `FUN_100490a0`: speak every matched entry (content and separator alike)
/// with the SAME span (`entries[0].start .. entries[count-1].pos_after`) --
/// a faithfully-kept quirk, not a per-entry span.
fn render(fe: &FeSpa, wa: &mut WaSpa, entries: &[Entry]) -> bool {
    let start = entries[0].start;
    let end = entries[entries.len() - 1].pos_after;
    for e in entries {
        let case = if e.kind == b'N' { b'S' } else { b'D' };
        let text: &[u8] = if !e.matched.is_empty() { &e.matched } else { &e.text };
        if spell::expand(fe, wa, text, start, end, case) == 0 {
            return false;
        }
    }
    true
}

/// `FUN_100494c0`, narrowed to the "WWW." case: the current word is exactly
/// (case-insensitively) `PROTOCOLS[10]` ("WWW") and immediately followed by
/// a lone `.`.
fn www_prefix(fe: &FeSpa, wa: &mut WaSpa, t: &[u8], p: usize, off: i32) -> Option<i32> {
    let (proto, pr) = tokenize(t, p, off, 9);
    if pr == 0 || proto.len == 0 || table_find(&PROTOCOLS, proto.text()) != Some(10) {
        return None;
    }
    let (next, nr) = tokenize(t, p + pr as usize, off + pr, 9);
    if nr == 0 || next.len != 1 || next.text() != b"." {
        return None;
    }
    if spell::expand(fe, wa, b"WWW", proto.start, proto.end, b'S') == 0 {
        return Some(0);
    }
    if spell::expand(fe, wa, b".", next.start, next.end, b'D') == 0 {
        return Some(0);
    }
    Some(pr + nr)
}

pub fn url(fe: &FeSpa, wa: &mut WaSpa, _tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    if let Some(sm) = scheme_prefix(t, p, off) {
        if !emit(wa, &sm.canon, sm.proto_start, sm.proto_end, b'A', b'S', 8, None) {
            return 0;
        }
        // `spell::expand`'s own `tt == b"://"` case turns this into "dos
        // puntos barra barra"; the engine speaks it as its own record(s),
        // separately from the protocol word.
        if spell::expand(fe, wa, b"://", sm.sep_start, sm.sep_end, b'S') == 0 {
            return 0;
        }
        return url_continuation(fe, wa, t, p + sm.consumed as usize, off + sm.consumed, sm.consumed);
    }
    if let Some(r) = www_prefix(fe, wa, t, p, off) {
        return r;
    }
    let entries = capture(t, p, off);
    if entries.is_empty() || !matches!(entries[0].kind, b'D' | b'H') {
        return -1;
    }
    let mut code = Vec::with_capacity(entries.len());
    for e in &entries {
        code.push(e.kind);
    }
    for count in (1..=entries.len()).rev() {
        let want = &code[..count];
        for (shape_idx, shape) in URL_SHAPES.iter().enumerate() {
            if shape.len() != count || *shape != want {
                continue;
            }
            if *shape == *b"D.D.D.D" && !valid_ipv4(&entries[..count]) {
                continue;
            }
            // `FUN_100493f0`'s gate: a `:port` is tried after EVERY shape,
            // but only the "D.D.D.D" IP shape (index 0) requires finding one
            // to render at all -- ported as-is, see the module doc comment.
            // The port itself is detected only, not consumed/rendered.
            if shape_idx == 0 {
                let match_end = entries[count - 1].pos_after;
                let match_p = (p as i32 - off + match_end) as usize;
                if !port_continuation(t, match_p, match_end) {
                    continue;
                }
            }
            if !render(fe, wa, &entries[..count]) {
                return 0;
            }
            return entries[count - 1].pos_after - off;
        }
    }
    -1
}
