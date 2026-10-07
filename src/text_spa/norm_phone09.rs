//! viol09 phone numbers: `FUN_10040720` (2014 `FUN_10044910`). The 2009 build has 18 shapes, no
//! trailing-extension gating on the 2014 trigger rules, a simpler "(TEL)" prefix (`FUN_10040f60`), a
//! trigger-context check (`FUN_10041510`), an "ext. NNN" tail (`FUN_10041240`), its own validation
//! (`FUN_10040e10`), render (`FUN_10040c60`) and group reader (`FUN_10041730`, 4-digit groups are read as
//! two 2-digit pairs, `FUN_1004ab00`/`FUN_1004ae90`).

use super::norm_units::{best_and_last, phrase_find};
use super::numwords::{atoi, cardinal_to_words, digit_by_digit, emit_phrase};
use super::punct::prev_words;
use super::tables::CLS_S;
use super::tokenize::{tokenize, Tok};
use super::{emit, rec};
use crate::ling_spa::lexicon::stricmp_spa;
use crate::ling_spa::WaSpa;

/// `PTR_DAT_10075f8c`.
const SHAPES: [&[u8]; 18] = [
    b"N(N)/N-N",
    b"N(N)N-N",
    b"N-N-N-N",
    b"N.N.N.N",
    b"N(N)NN",
    b"NN-N-N",
    b"NN.N.N",
    b"NNNN",
    b"(N)/N-N",
    b"(N)N-N",
    b"(N)NN",
    b"N/N-N",
    b"N-N-N",
    b"N.N.N",
    b"NN-N",
    b"NNN",
    b"N-N",
    b"NN",
];

/// `0x1006b040` (stride 0x27): key, spoken word.
const PAREN_WORD: [(&[u8], &[u8]); 2] = [(b"FAX", b"fax"), (b"TEL", b"tel\xe9fono")];
const TEL_LIT: &[u8] = b"tel\xe9fono";
const FAX_LIT: &[u8] = b"facs\xedmil";
/// `PTR_DAT_1006b0a0`.
const TRIGGER_LEFT: [&[u8]; 7] = [b"facs\xedmil", b"facs\xedmile", b"fax", b"tel", b"telefonear", b"tel\xe9f", b"tel\xe9fono"];
/// `PTR_DAT_1006b090`.
const EXT_WORDS: [&[u8]; 4] = [b"ex", b"ext", b"x", b"xt"];
const MAS: &[u8] = b"m\xe1s";
const EXTENSION: &[u8] = b"extensi\xf3n";

fn digit(c: u8) -> bool {
    CLS_S[c as usize] & 0x10 != 0
}

/// `FUN_10046f50`.
fn all_digit(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&c| digit(c))
}

fn eq_ci(a: &[u8], b: &[u8]) -> bool {
    stricmp_spa(a, b) == 0
}

fn paren_word_index(text: &[u8]) -> Option<usize> {
    PAREN_WORD.iter().position(|&(k, _)| eq_ci(k, text))
}

#[derive(Clone, Default)]
struct Entry {
    start: i32,
    pos_after: i32,
    kind: u8,
    ws: i32,
    text: Vec<u8>,
}

struct Span {
    start: i32,
    end: i32,
    len: i32,
}

struct TelFax {
    span: Span,
    word: Vec<u8>,
}

#[derive(Default)]
struct ExtArea {
    word: Option<Span>,
    digits: Option<(Span, Vec<u8>)>,
}

fn ae90_word(s: &[u8]) -> Vec<u8> {
    let d = |c: u8| -> &'static [u8] {
        match c {
            b'0' => b"cero",
            b'1' => b"uno",
            b'2' => b"dos",
            b'3' => b"tres",
            b'4' => b"cuatro",
            b'5' => b"cinco",
            b'6' => b"seis",
            b'7' => b"siete",
            b'8' => b"ocho",
            b'9' => b"nueve",
            _ => b"",
        }
    };
    match s.len() {
        1 => d(s[0]).to_vec(),
        2 => {
            if atoi(s) > 9 {
                let mut w = Vec::new();
                cardinal_to_words(&mut w, s, 0);
                w
            } else {
                let mut w = b"cero ".to_vec();
                w.extend_from_slice(d(s[1]));
                w
            }
        }
        _ => Vec::new(),
    }
}

/// `FUN_1004ae90`: append the word of a 1- or 2-digit string, `"%s %s"` joined.
fn ae90(out: &mut Vec<u8>, s: &[u8]) {
    let w = ae90_word(s);
    if out.is_empty() {
        *out = w;
    } else {
        out.push(b' ');
        out.extend_from_slice(&w);
    }
}

/// `FUN_1004ab00` for 3- and 4-digit groups.
fn pairs(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    if s.len() == 3 {
        if s[1] == b'0' && s[2] == b'0' {
            ae90(&mut out, s);
        } else {
            ae90(&mut out, &s[..1]);
            ae90(&mut out, &s[1..]);
        }
    } else if s[0] < b'1' || s[1] != b'0' || s[2] != b'0' || s[3] != b'0' {
        ae90(&mut out, &s[..2]);
        ae90(&mut out, &s[2..]);
    } else {
        ae90(&mut out, &s[..1]);
        out.extend_from_slice(b" mil");
    }
    out
}

/// `FUN_10041730`.
fn digit_group_words(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    match text.len() {
        3 if text[0] > b'0' && text[0] <= b'9' && text[1] == b'0' && text[2] == b'0' => cardinal_to_words(&mut out, text, 0),
        3 | 4 => out = pairs(text),
        _ => digit_by_digit(&mut out, text),
    }
    out
}

/// `FUN_10040f60`: the "(TEL)" / "tel:" prefix. Returns the entry and the bytes it covers.
fn prefix(tok: &Tok, t: &[u8], p: usize, off: i32) -> Option<(TelFax, i32)> {
    let paren = tok.text().first() == Some(&b'(') && tok.len == 1;
    let (word_tok, base) = if paren {
        let after = tok.consumed;
        let (nx, r) = tokenize(t, p + after as usize, off + after, 6);
        if r == 0 || nx.class == 10 {
            return None;
        }
        (nx, after + r)
    } else {
        (tok.clone(), tok.consumed)
    };
    let found = paren_word_index(word_tok.text());
    if found.is_none() && !eq_ci(TEL_LIT, word_tok.text()) && !eq_ci(FAX_LIT, word_tok.text()) {
        return None;
    }
    if word_tok.class != 1 || word_tok.ws >= 3 {
        return None;
    }
    let word = match found {
        Some(i) => PAREN_WORD[i].1.to_vec(),
        None => word_tok.text().to_vec(),
    };
    let entry = TelFax { span: Span { start: word_tok.start, end: word_tok.end, len: word_tok.len }, word };
    let (cl, r2) = tokenize(t, p + base as usize, off + base, 6);
    let c = cl.text().first().copied().unwrap_or(0);
    let closes = if paren { c == b')' } else { c == b')' || c == b':' };
    if closes && cl.len == 1 && cl.ws < 3 {
        return Some((entry, base + r2));
    }
    Some((entry, base))
}

/// `FUN_10041510(t + p, off, 'B', consumed)`: a trigger word in the two words before or after.
fn trigger_context(t: &[u8], p: usize, off: i32, consumed: i32) -> bool {
    let mut left: Vec<Vec<u8>> = Vec::new();
    let (w, back) = prev_words(t, p - off as usize, p, 2);
    if back != 0 {
        let (best, last) = best_and_last(&w, off - back as i32, 6);
        if last.len > 0 {
            left.push(last.text().to_vec());
            if best.len > 0 {
                left.push(best.text().to_vec());
            }
        }
    }
    let mut right: Vec<Vec<u8>> = Vec::new();
    let mut q = p + consumed as usize;
    let mut o = off + consumed;
    while right.len() < 2 {
        let (tk, c) = tokenize(t, q, o, 6);
        if c == 0 || tk.class == 9 || tk.class == 10 {
            break;
        }
        q += c as usize;
        o += c;
        right.push(tk.text().to_vec());
    }
    if left.iter().any(|w| eq_ci(w, TEL_LIT)) {
        return true;
    }
    phrase_find(&TRIGGER_LEFT, &left, 2) || phrase_find(&EXT_WORDS, &right, 2)
}

/// `FUN_10041240`: the "ext. 123" tail. Writes into the persistent `area` as the engine does.
fn ext_scan(t: &[u8], p: usize, off: i32, area: &mut ExtArea) -> i32 {
    let (tok, u2) = tokenize(t, p, off, 6);
    if u2 == 0 || tok.class == 10 {
        return 0;
    }
    if !EXT_WORDS.iter().any(|k| eq_ci(k, tok.text())) || tok.ws > 2 || tok.class != 1 {
        return 0;
    }
    area.word = Some(Span { start: tok.start, end: tok.end, len: tok.len });
    let (n2, r2) = tokenize(t, p + u2 as usize, off + u2, 6);
    let fill = |area: &mut ExtArea, n: &Tok| {
        area.digits = Some((Span { start: n.start, end: n.end, len: n.len }, digit_group_words(n.text())));
    };
    if n2.ws == 0 {
        if n2.len == 1 && n2.text().first() == Some(&b'.') {
            let i4 = u2 + r2;
            let (n3, r3) = tokenize(t, p + i4 as usize, off + i4, 6);
            if n3.ws < 3 && n3.class == 2 && n3.len < 5 && all_digit(n3.text()) {
                fill(area, &n3);
                return r3 + i4;
            }
            return 0;
        }
    } else if n2.ws >= 3 {
        area.word = None;
        return 0;
    }
    if n2.class == 2 && n2.len < 5 && all_digit(n2.text()) {
        fill(area, &n2);
        return r2 + u2;
    }
    area.word = None;
    0
}

fn len_1_3(s: &[u8]) -> bool {
    (1..=3).contains(&s.len()) && all_digit(s)
}

/// `FUN_100417e0`.
fn len_e0(s: &[u8], mode: i32) -> bool {
    let ok = if mode == 1 { (2..=3).contains(&s.len()) } else { (1..=3).contains(&s.len()) };
    ok && all_digit(s)
}

/// `FUN_10041840`.
fn len_840(s: &[u8], mode: i32) -> bool {
    let ok = if mode == 1 { s.len() == 3 } else { (3..=4).contains(&s.len()) };
    ok && all_digit(s)
}

/// `FUN_100418a0`.
fn len_4(s: &[u8]) -> bool {
    s.len() == 4 && all_digit(s)
}

/// `FUN_10040e10`.
fn validate(entries: &[Entry], shape: usize, trigger: bool) -> bool {
    let mut left;
    let mut mode = trigger as i32;
    if shape < 8 {
        left = 4;
    } else if shape < 0x10 {
        left = 3;
    } else if !trigger {
        let e0 = &entries[0];
        return if shape == 0x10 {
            len_840(&e0.text, 0) && entries[1].ws == 0 && len_4(&entries[2].text)
        } else {
            len_840(&e0.text, 0) && entries[1].ws == 1 && len_4(&entries[1].text)
        };
    } else {
        left = 2;
    }
    for e in entries {
        if e.kind != b'N' {
            continue;
        }
        left -= 1;
        let ok = match left {
            0 => len_4(&e.text),
            1 => len_840(&e.text, mode),
            2 => len_e0(&e.text, mode),
            3 => {
                let ok = len_1_3(&e.text);
                if ok {
                    mode = atoi(&e.text);
                }
                ok
            }
            _ => true,
        };
        if !ok {
            return false;
        }
    }
    true
}

fn set_punct5(wa: &mut WaSpa) {
    let n = wa.i16(0);
    if n > 0 {
        wa.set_i32(WaSpa::rec(n as usize - 1) + rec::PUNCT, 5);
    }
}

/// `FUN_10040c60`.
fn render(wa: &mut WaSpa, tel: Option<&TelFax>, plus: Option<&Span>, entries: &[Entry], ext: &ExtArea) -> bool {
    if let Some(tf) = tel {
        if tf.span.len > 0 && !emit(wa, &tf.word, tf.span.start, tf.span.end, b'A', b'D', 6, None) {
            return false;
        }
    }
    if let Some(pl) = plus {
        if pl.len > 0 && !emit(wa, MAS, pl.start, pl.end, b'S', b'D', 6, None) {
            return false;
        }
    }
    let start = entries[0].start;
    let end = entries[entries.len() - 1].pos_after;
    for e in entries {
        if e.kind == b'N' {
            let words = digit_group_words(&e.text);
            if !emit_phrase(wa, &words, start, end, b'D', b'D', 6) {
                return false;
            }
            set_punct5(wa);
        }
    }
    if let (Some(w), Some((d, words))) = (&ext.word, &ext.digits) {
        if w.len > 0 && d.len > 0 {
            if !emit(wa, EXTENSION, w.start, w.end, b'A', b'D', 6, None) {
                return false;
            }
            if !emit_phrase(wa, words, d.start, d.end, b'D', b'D', 6) {
                return false;
            }
        }
    }
    true
}

/// The `(n_groups, n_entries)` shape-index range of the engine's dispatch.
fn shape_range(groups: usize, n: usize) -> Option<(usize, usize)> {
    match (groups, n) {
        (2, 3) => Some((0x10, 0x11)),
        (2, 2) => Some((0x11, 18)),
        (3, 7) => Some((8, 9)),
        (3, 6) => Some((9, 10)),
        (3, 5) => Some((10, 0xe)),
        (3, 4) => Some((0xe, 0xf)),
        (3, 3) => Some((0xf, 0x10)),
        (4, 8) => Some((0, 1)),
        (4, 7) => Some((1, 4)),
        (4, 6) => Some((4, 7)),
        (4, 4) => Some((7, 8)),
        _ => None,
    }
}

/// `FUN_10040720(wa, tok, t + p, off)`.
pub fn phone09(wa: &mut WaSpa, tok: &Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let mut trigger = false;
    let mut tel: Option<TelFax> = None;
    let (mut cp, mut co) = (p, off);
    if let Some((entry, consumed)) = prefix(tok, t, p, off) {
        if consumed != 0 {
            trigger = true;
            cp += consumed as usize;
            co += consumed;
        }
        tel = Some(entry);
    }

    let mut entries: Vec<Entry> = Vec::new();
    let mut plus: Option<Span> = None;
    let mut plus_idx: i32 = -1;
    let mut piece = 0i32;
    let mut groups = 0usize;
    loop {
        let (tk, r) = tokenize(t, cp, co, 6);
        if r == 0 || tk.class == 10 {
            break;
        }
        let text = tk.text().to_vec();
        let c0 = text.first().copied().unwrap_or(0);
        if entries.is_empty() && c0 == b'+' && tk.len == 1 && plus_idx == -1 {
            plus = Some(Span { start: tk.start, end: tk.end, len: 1 });
            plus_idx = piece;
        } else {
            let sep = c0 != 0 && b"()/-.".contains(&c0) && tk.len == 1 && plus_idx == -1;
            let kind = if sep {
                c0
            } else {
                if !all_digit(&text) {
                    break;
                }
                if plus_idx > -1 {
                    if plus_idx + 1 != piece || tk.ws != 0 {
                        break;
                    }
                    plus_idx = -1;
                }
                groups += 1;
                b'N'
            };
            entries.push(Entry { start: tk.start, pos_after: co + r, kind, ws: tk.ws, text });
            if entries.len() > 7 {
                break;
            }
        }
        piece += 1;
        cp += r as usize;
        co += r;
    }
    if groups == 0 {
        return -1;
    }

    let mut ext = ExtArea::default();
    let mut n = entries.len();
    let mut groups = groups;
    while n > 0 {
        if n < entries.len() && entries[n].kind == b'N' {
            groups -= 1;
        }
        if let Some((lo, hi)) = shape_range(groups, n) {
            let code: Vec<u8> = entries[..n].iter().map(|e| e.kind).collect();
            for shape in lo..hi {
                if SHAPES[shape] != code.as_slice() {
                    continue;
                }
                let consumed = entries[n - 1].pos_after - off;
                if (shape == 0x10 || shape == 0x11) && !trigger && trigger_context(t, p, off, consumed) {
                    trigger = true;
                }
                let ext_bytes = ext_scan(t, p + consumed as usize, off + consumed, &mut ext) as i16 as i32;
                if validate(&entries[..n], shape, trigger) {
                    return if render(wa, tel.as_ref(), plus.as_ref(), &entries[..n], &ext) { consumed + ext_bytes } else { 0 };
                }
                break;
            }
        }
        n -= 1;
    }
    -1
}
