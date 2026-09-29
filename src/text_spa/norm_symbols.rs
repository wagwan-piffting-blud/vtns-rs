//! `FUN_10042b20` (chain slot 11): symbols. Handles a lone `,` after a
//! number (marks the previous WA record and drops the comma), a lone
//! backtick/apostrophe/doublequote wrapping another symbol (`` `+` `` etc,
//! speaks the wrapped symbol's name and drops both quote marks), `(c)`/`(r)`,
//! a lone `<`/`>` (optionally merged with a following `=`) as a comparison
//! word, and a lone `:` between number-like context. Everything else
//! declines, letting `word`/`spell` speak (or, for symbols `math`/`norm_roman`
//! already tried and rejected, silently drop) whatever is left.
//!
//! Lookback uses `FUN_1004bd70` (`punct::prev_words`), `FUN_1004bf70`
//! (`last_token_0x12`, one word) and `FUN_1004bfe0` (`best_and_last_0x12`,
//! two words) -- all tokenizing with mode 0x12.

use super::emit::emit_split;
use super::punct::prev_words;
use super::tables::CLS_S;
use super::tables_oth::{symbol_name_index, SYMBOL_NAMES};
use super::tokenize::{tokenize, Tok};
use super::FeSpa;
use super::{emit, rec};
use crate::ling_spa::WaSpa;

fn digit(c: u8) -> bool {
    CLS_S[c as usize] & 0x10 != 0
}

fn is_all_digit(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&c| digit(c))
}

/// `FUN_1004b630`: digits, with '.' only between digits and ',' before a
/// digit -- the same check as `norm_math::numeric` (duplicated here rather
/// than made `pub` there, per the shared-file rule).
fn numeric(s: &[u8]) -> bool {
    let n = s.len();
    if n == 0 {
        return false;
    }
    for i in 0..n {
        let c = s[i];
        if digit(c) {
            continue;
        }
        if c == b'.' && i >= 1 && i + 1 < n && digit(s[i - 1]) && digit(s[i + 1]) {
            continue;
        }
        if c != b',' || i + 1 >= n {
            return false;
        }
        if i == 0 {
            if !digit(s[1]) {
                return false;
            }
        } else if !digit(s[i - 1]) || !digit(s[i + 1]) {
            return false;
        }
    }
    true
}

/// `FUN_1002d910`: a plausible year -- 4 digits (leading nonzero) or `'` +
/// exactly 2 digits. Only the validity (`>= 0`) matters to callers here.
fn parse_year_like(s: &[u8]) -> i32 {
    if s.first() == Some(&b'\'') {
        if s.len() == 3 && digit(s[1]) && digit(s[2]) {
            return 0;
        }
        return -1;
    }
    if s.len() == 4 && s[0] > b'0' && s[0] <= b'9' && digit(s[1]) && digit(s[2]) && digit(s[3]) {
        return 0;
    }
    -1
}

/// `DAT_1006fbe0` (39 entries, key + word, stride 0x2a): class-3 symbols'
/// spoken names, used by the backtick/quote-wrap quirk. `tables_oth::SYMBOL_NAMES`
/// is the exact same table (already extracted for `spell::expand`); reused
/// here rather than duplicated.
///
/// `DAT_10079718` (33 entries, same stride): class-6 raw-byte symbols'
/// spoken names, used by the same quirk when the wrapped token is class 6.
const HI_WORDS: [(u8, &[u8]); 33] = [
    (0xa1, b"exclamaci\xf3n abierta"),
    (0xa2, b"centavo"),
    (0xa3, b"libra esterlina"),
    (0xa4, b"monetario"),
    (0xa5, b"yen"),
    (0xa6, b"barra vertical partida"),
    (0xa7, b"secci\xf3n"),
    (0xa8, b"di\xe9resis"),
    (0xa9, b"derechos de copia"),
    (0xaa, b"indicador ordinal femenino"),
    (0xab, b"comillas francesas de apertura"),
    (0xac, b"negaci\xf3n"),
    (0xad, b"gui\xf3n separador de s\xedlabas"),
    (0xae, b"marca registrada"),
    (0xaf, b"macr\xf3n"),
    (0xb0, b"grado"),
    (0xb1, b"m\xe1s-menos"),
    (0xb2, b"cuadrado"),
    (0xb3, b"c\xfabico"),
    (0xb4, b"acento agudo"),
    (0xb5, b"micro"),
    (0xb6, b"calder\xf3n"),
    (0xb7, b"punto centrado"),
    (0xb8, b"cedilla"),
    (0xb9, b"super\xedndice 1"),
    (0xba, b"indicador ordinal masculino"),
    (0xbb, b"comillas francesas de cierre"),
    (0xbc, b"un cuarto"),
    (0xbd, b"un medio"),
    (0xbe, b"tres cuartos"),
    (0xbf, b"interrogaci\xf3n abierta"),
    (0xd7, b"multiplicaci\xf3n"),
    (0xf7, b"divisi\xf3n"),
];

/// `FUN_10043860`: linear scan is behaviourally equivalent to the engine's
/// binary search over the (key-sorted) table -- same precedent as
/// `norm_math::superscript`.
fn find_word(table: &[(u8, &[u8])], key: u8) -> i32 {
    if key == 0 {
        return -1;
    }
    match table.iter().position(|&(k, _)| k == key) {
        Some(i) => i as i32,
        None => -1,
    }
}

/// `FUN_1004bf70(out, text, off, 0x12)`: the last token of `text`.
fn last_token_0x12(text: &[u8], off: i32) -> Option<Tok> {
    if text.is_empty() {
        return None;
    }
    let mut b = text.to_vec();
    b.extend_from_slice(&[0u8; 64]);
    let mut tok = Tok::default();
    let mut used = 0usize;
    while used < text.len() {
        let (t, r) = tokenize(&b, used, off + used as i32, 0x12);
        tok = t;
        if r <= 0 {
            break;
        }
        used += r as usize;
    }
    Some(tok)
}

/// `FUN_1004bfe0(best, last, text, off, 0x12)`: tokenizes all of `text`,
/// returning the last token that did NOT reach its end (`best`, i.e. the
/// first of two words) and the very last token tokenized (`last`).
fn best_and_last_0x12(text: &[u8], off: i32) -> (Option<Tok>, Option<Tok>) {
    if text.is_empty() {
        return (None, None);
    }
    let mut b = text.to_vec();
    b.extend_from_slice(&[0u8; 64]);
    let total = text.len();
    let mut used = 0usize;
    let mut best = None;
    let mut last;
    loop {
        let (tok, r) = tokenize(&b, used, off + used as i32, 0x12);
        if r > 0 && used + (r as usize) < total && tok.len != 0 {
            best = Some(tok.clone());
        }
        last = tok;
        if r <= 0 {
            break;
        }
        used += r as usize;
        if used >= total {
            break;
        }
    }
    (best, Some(last))
}

fn mark_prev_quirk(wa: &mut WaSpa, n: i32) {
    if n > 0 {
        let r = WaSpa::rec((n - 1) as usize);
        wa.set_i32(r + rec::PUNCT, 4);
    }
}

/// The class-3 sub-branch for a lone backtick/apostrophe/doublequote (`q`):
/// tries "Q X Q" (X a single symbol whose spoken name is in `SYM_WORDS` or
/// `HI_WORDS`, tightly wrapped in the same `Q` on both sides) and speaks
/// only X's name, silently dropping both `Q`s and X. Otherwise silently
/// drops the lone `Q`.
fn quote_trick(wa: &mut WaSpa, tok: &Tok, t: &[u8], p: usize, off: i32, q: u8) -> i32 {
    let (b, rb) = tokenize(t, p + tok.consumed as usize, off + tok.consumed, 0x12);
    if rb == 0 || b.class == 10 {
        return tok.consumed;
    }
    if b.class == 3 && b.len == 1 {
        let idx = symbol_name_index(b.text()[0]);
        if idx >= 0 {
            let (c, rc) = tokenize(t, p + (tok.consumed + rb) as usize, off + tok.consumed + rb, 0x12);
            if rc != 0 && c.class != 10 && c.len == 1 && c.class == 3 && c.text().first() == Some(&q) && b.ws == c.ws {
                if !emit_split(wa, SYMBOL_NAMES[idx as usize].1, b.start, b.end, b'S', b'D', 0x10) {
                    return 0;
                }
                return tok.consumed + rb + rc;
            }
        }
    }
    if b.class == 6 && b.len == 1 {
        let idx = find_word(&HI_WORDS, b.text()[0]);
        if idx >= 0 {
            let (c, rc) = tokenize(t, p + (tok.consumed + rb) as usize, off + tok.consumed + rb, 0x12);
            if rc != 0 && c.class != 10 && c.len == 1 && c.class == 3 && c.text().first() == Some(&q) {
                if !emit_split(wa, HI_WORDS[idx as usize].1, b.start, b.end, b'S', b'D', 0x10) {
                    return 0;
                }
                return tok.consumed + rb + rc;
            }
        }
    }
    tok.consumed
}

/// `(char)param_2[7] == -0x57` (0xa9, `\xa9`): "derechos de autor" when
/// followed by a plausible year.
fn copyright(wa: &mut WaSpa, tok: &Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let (next, r) = tokenize(t, p + tok.consumed as usize, off + tok.consumed, 3);
    let ok = r != 0 && next.class != 10 && next.class == 2 && is_all_digit(next.text()) && parse_year_like(next.text()) >= 0;
    if ok && !emit_split(wa, b"derechos de autor", tok.start, tok.end, b'S', b'D', 0x10) {
        return 0;
    }
    tok.consumed
}

/// `(char)param_2[7] == -0x52` (0xae, `\xae`): "marca registrada".
fn trademark(wa: &mut WaSpa, tok: &Tok) -> i32 {
    if !emit_split(wa, b"marca registrada", tok.start, tok.end, b'S', b'D', 0x10) {
        return 0;
    }
    tok.consumed
}

fn speak_cmp(wa: &mut WaSpa, tok: &Tok, merged: bool, merge_end: i32, b_end: i32, q: u8) -> i32 {
    if merged {
        let word: &[u8] = if q == b'<' { b"menor o igual que" } else { b"mayor o igual que" };
        return if emit_split(wa, word, tok.start, b_end, b'S', b'D', 0x10) { merge_end } else { 0 };
    }
    let word: &[u8] = if q == b'<' { b"menor que" } else { b"mayor que" };
    if emit_split(wa, word, tok.start, tok.end, b'S', b'D', 0x10) {
        tok.consumed
    } else {
        0
    }
}

/// The class-3 `<`/`>` branch (len 1, `q`): a comparison against the
/// lookback word (`FUN_1004bd70(.., 1)` + `FUN_1004bf70`), optionally merged
/// with a glued `=` into "menor/mayor o igual que". Never declines to -1
/// once reached -- `LAB_10042bae` always returns the outer token's own
/// consumed count.
fn compare(wa: &mut WaSpa, tok: &Tok, t: &[u8], p: usize, off: i32, q: u8) -> i32 {
    let (buf, back) = prev_words(t, p - off as usize, p, 1);
    let f = if back != 0 { last_token_0x12(&buf, off - back as i32) } else { None }.unwrap_or_default();

    let (b, rb) = tokenize(t, p + tok.consumed as usize, off + tok.consumed, 0x12);
    if rb == 0 || b.class == 10 {
        return tok.consumed;
    }

    let mut merged = false;
    let mut merge_end = 0i32;
    let c;
    let mut rc;
    if b.class == 3 && b.len == 1 && b.text().first() == Some(&b'=') && b.ws == 0 {
        let base = tok.consumed + rb;
        let (c2, rc2) = tokenize(t, p + base as usize, off + base, 0x12);
        if rc2 == 0 || c2.class == 10 {
            return tok.consumed;
        }
        merged = true;
        merge_end = base;
        c = c2;
        rc = rc2;
    } else {
        c = b.clone();
        rc = rb;
    }
    let _ = &mut rc;

    // Is `c` a lone qualifying decorator ($, + or -) rather than a real
    // operand?
    let c_is_decorator = c.len == 1 && c.class == 3 && c.text().first().is_some_and(|&ch| ch != 0 && b"$+-".contains(&ch)) && c.ws <= 2;

    if !c_is_decorator {
        if f.len < 1 {
            return tok.consumed;
        }
        let wa_n = wa.i16(0) as i32;
        let end_ok = wa_n >= 1 && wa.i32(WaSpa::rec((wa_n - 1) as usize) + rec::END) == f.end;
        let overall2 = !end_ok || f.class != 2 || tok.ws > 1 || c.len < 1 || c.class != 2 || c.ws > 1;
        if overall2 {
            let cond3 = wa_n > 0 && end_ok && f.class == 1 && tok.ws < 2 && c.len > 0 && c.class == 1 && c.ws < 2;
            if cond3 {
                mark_prev_quirk(wa, wa_n);
            }
            return tok.consumed;
        }
        return speak_cmp(wa, tok, merged, merge_end, b.end, q);
    }

    // `c` is a decorator glued between the operator (or "<=") and a real
    // number `g`.
    let base2 = if merged { merge_end } else { tok.consumed } + rc;
    let (g, _) = tokenize(t, p + base2 as usize, off + base2, 0x12);
    let wa_n = wa.i16(0) as i32;
    let end_ok = wa_n >= 1 && wa.i32(WaSpa::rec((wa_n - 1) as usize) + rec::END) == f.end;
    let g_ok = g.len > 0 && f.len >= 1 && end_ok && (f.class == 2 || f.class == 1) && g.class == 2 && g.ws <= 2;
    if !g_ok {
        return tok.consumed;
    }
    speak_cmp(wa, tok, merged, merge_end, b.end, q)
}

/// The class-3 `:` branch (len 1): speaks `DAT_10064530` -- not actually a
/// Spanish phrase, but the letter-spelling table's entry for 'a' (a
/// one-byte C string by coincidence of that table's layout); kept as-is,
/// see the brief's "engine bugs are ported as-is" rule -- for a `:` tightly
/// glued to a number, with two lookback words and guards against a colon on
/// either side of that number too. Never declines to -1.
fn colon(wa: &mut WaSpa, tok: &Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let (buf, back) = prev_words(t, p - off as usize, p, 2);
    let (e, _f) = if back != 0 { best_and_last_0x12(&buf, off - back as i32) } else { (None, None) };
    let e = e.unwrap_or_default();

    let (c, rc) = tokenize(t, p + tok.consumed as usize, off + tok.consumed, 0x12);
    if rc == 0 || c.class == 10 || c.class != 2 || !numeric(c.text()) || tok.ws != c.ws {
        return tok.consumed;
    }
    if tok.ws > 1 || (e.len == 1 && e.class == 3 && e.text().first() == Some(&b':')) {
        return tok.consumed;
    }
    let base = tok.consumed + rc;
    let (g, _) = tokenize(t, p + base as usize, off + base, 0x12);
    if g.len == 1 && g.class == 3 && g.text().first() == Some(&b':') {
        return tok.consumed;
    }
    if !emit(wa, b"a", tok.start, tok.end, b'S', b'D', 0x10, None) {
        return 0;
    }
    tok.consumed
}

/// `FUN_10042b20(wa, tok, t + p, off)`.
pub fn symbols(fe: &FeSpa, wa: &mut WaSpa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let _ = fe;
    if tok.class == 4 || tok.class == 5 {
        return tok.consumed;
    }
    if tok.len == 0 {
        return -1;
    }
    let c0 = tok.text().first().copied().unwrap_or(0);

    if tok.class == 3 {
        if c0 == b',' && tok.len == 1 {
            mark_prev_quirk(wa, wa.i16(0) as i32);
            return tok.consumed;
        }
        if matches!(c0, 0x60 | 0x27 | 0x22) && tok.len == 1 {
            return quote_trick(wa, tok, t, p, off, c0);
        }
    }

    if tok.class != 3 {
        if tok.class == 6 {
            if tok.len == 1 && c0 == 0xa9 {
                return copyright(wa, tok, t, p, off);
            }
            if tok.len == 1 && c0 == 0xae {
                return trademark(wa, tok);
            }
        }
        return -1;
    }

    if (c0 == b'<' || c0 == b'>') && tok.len == 1 {
        return compare(wa, tok, t, p, off, c0);
    }
    if c0 == b':' && tok.len == 1 {
        return colon(wa, tok, t, p, off);
    }
    -1
}
