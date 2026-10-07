//! `FUN_1004d990`: VTML content tags. `fe_InitialTextProcess` rewrites a
//! content tag as `<\xa1\xdd` + type letter + `>` ... `</\xa1\xdd>`; the
//! tokenizer hands the opening as a class-4 token whose one character is the
//! type letter. The handler gets the content between the opening and the
//! closing marker and returns the bytes it consumed; the dispatcher adds the
//! 5-byte close. An empty tag is skipped whole.
//!
//! Ported: `T` phoneme (`FUN_10051640`), `U` part of speech
//! (`FUN_100518b0`), `S` sub alias (`FUN_100514f0`), `B`/`M`/`N` time
//! (`FUN_1004df70`), `L` phone (`FUN_100500c0`), `J` currency
//! (`FUN_1004fbf0`), `D` characters (`FUN_1004eba0`), `A`/`H` date
//! (`FUN_1004dd20`), `C` phone keys (`FUN_1004e9a0`), `E`/`K` cardinal
//! (`FUN_1004f500`), `F` ordinal (`FUN_1004f710`), `G` boolean
//! (`FUN_1004f8b0`), `I` digits (`FUN_1004fa60`), `O` informal currency
//! (`FUN_100502c0`), `P` fraction (`FUN_10050890`), `V` duration
//! (`FUN_10051130`), `Q` net address (`FUN_10050b90`), `R` address
//! (`FUN_10050ec0`).
//!
//! These letters are produced by `fe_InitialTextProcess`'s say-as dispatch
//! (`FUN_10048d60`, a 22-entry `sapi:`/`ssml:`/`vtml:`/`vxml:` `interpret-as`
//! table over `<vtml_sayas interpret-as="...">`) which wraps validated
//! content in the type letter and hands it to *this* module's second-stage
//! reader (`FUN_1004d990`) — a different function per letter from the one
//! `FUN_10048d60` itself calls to validate/wrap (e.g. `M`'s wrapper is
//! `FUN_1004be10` but its reader is `FUN_1004df70`). The interpret-as ->
//! letter map (all 22 entries, confirmed via the table at `0x10080e04`
//! (strings) / `0x10080e60` (handler pointers) and the type-letter immediate
//! pushed before each handler's `FUN_10063ed6` sprintf call): `sapi:address`
//! -> R, `sapi:currency` -> O, `sapi:date` -> A, `sapi:email` -> Q,
//! `sapi:number` -> K (format empty/"cardinal"/"decimal"), I
//! (format="digit") or P (format="fraction"), `sapi:phone` -> C,
//! `sapi:time` -> N (time(), already ported), `sapi:web` -> Q,
//! `ssml:cardinal` -> E, `ssml:characters` -> D, `ssml:date` -> A,
//! `ssml:ordinal` -> F, `ssml:telephone` -> C, `ssml:time` -> B (time()),
//! `vtml:duration` -> V, `vxml:boolean` -> G, `vxml:currency` -> J,
//! `vxml:date` -> H, `vxml:digits` -> I, `vxml:number` -> K, `vxml:phone`
//! -> L, `vxml:time` -> M (time()).
//!
//! `Q`/`R`'s `initial.rs` first-stage converters are `sapi:email`/
//! `sapi:web`/`sapi:address`; see their functions below for engine quirks
//! specific to each (Q's digit-token rejection, R's `-`-with-no-label
//! no-op).

use super::cstr::*;
use super::norm::url_words;
use super::norm_address_csc::{mixed_code, postal};
use super::norm_date::month_name;
use super::norm_phone::group_words;
use super::norm_url::piece;
use super::numwords::{digits, digits_only, new_buf, number, numeric, ordinal_numeral_words, pairs, split_decimal};
use super::tabs::{ptab, search, rec5a, rec66, Ptab, Rec5a, Rec66};
use super::sbd::{all_alpha, all_digit};
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::Fe;

pub fn dispatch(fe: &Fe, wa: &mut Wa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    if tok.class != 4 || tok.len != 1 {
        return -1;
    }
    let close_pat = s(0x10081190);
    let Some(close) = t[p..].windows(close_pat.len()).position(|w| w == close_pat).map(|i| p + i) else {
        return -1;
    };
    let mut n = tok.consumed;
    let mut cp = p + n as usize;
    if cp == close {
        return n + 5;
    }
    while matches!(t[cp], b' ' | b'\t' | b'\n' | b'\r') {
        cp += 1;
        n += 1;
    }
    if cp == close {
        return n + 5;
    }
    let coff = off + n;
    let r = match tok.text[0] {
        b'T' => phoneme(wa, t, cp, coff, close),
        b'U' => partofsp(wa, t, cp, coff, close),
        b'S' => sub(fe, wa, t, cp, coff, close),
        b'B' | b'M' | b'N' => time(fe, wa, t, cp, coff, close),
        b'L' => phone(fe, wa, t, cp, coff, close),
        b'J' => currency(fe, wa, t, cp, coff, close),
        b'D' => characters(fe, wa, t, cp, coff, close),
        b'A' | b'H' => saydate(fe, wa, t, cp, coff, close),
        b'C' => phonekeys(fe, wa, t, cp, coff, close),
        b'E' | b'K' => numeral(fe, wa, t, cp, coff, close),
        b'F' => saydinal(fe, wa, t, cp, coff, close),
        b'G' => boolean(fe, wa, t, cp, coff, close),
        b'I' => digits_tag(fe, wa, t, cp, coff, close),
        b'P' => fraction_tag(fe, wa, t, cp, coff, close),
        b'V' => duration(fe, wa, t, cp, coff, close),
        b'O' => currency_informal(fe, wa, t, cp, coff, close),
        b'Q' => netaddr(fe, wa, t, cp, coff, close),
        b'R' => address(fe, wa, t, cp, coff, close),
        _ => -1,
    };
    if r < 0 {
        return -1;
    }
    if r == 0 {
        return 0;
    }
    n + r + 5
}

/// `FUN_10051640`: `word \xff phones`. Phone codes 1..100 are copied; a final
/// `n`/`o`/`p`/`q` appends `.`/`?`/`!`/`,` to the word, `r` is ignored.
fn phoneme(wa: &mut Wa, t: &[u8], cp: usize, coff: i32, close: usize) -> i32 {
    let body = &t[cp..close];
    if body.len() > 0x1ff || body.is_empty() {
        return -1;
    }
    let Some(ff) = body.iter().rposition(|&c| c == 0xff) else { return -1 };
    let codes = &body[ff + 1..];
    if codes.len() > 0x41 || codes.is_empty() {
        return -1;
    }
    let mut word: Vec<u8> = if ff > 0 && ff < 0x1e {
        let mut w = body[..ff].to_vec();
        while w.first().is_some_and(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\r')) {
            w.remove(0);
        }
        while w.last().is_some_and(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\r')) {
            w.pop();
        }
        w
    } else {
        s(0x10081350).to_vec()
    };
    let last = codes.len() - 1;
    let mut phones = Vec::new();
    for (i, &c) in codes.iter().enumerate() {
        let sc = c as i8;
        if (1..=b'd' as i8).contains(&sc) {
            phones.push(c);
        } else if (b'n' as i8..=b'p' as i8).contains(&sc) {
            if i == last {
                word.extend_from_slice(match c {
                    b'n' => s(0x10077700),
                    b'o' => s(0x100776f4),
                    _ => s(0x100776f8),
                });
            }
        } else if c == b'q' {
            if i == last {
                word.extend_from_slice(s(0x100776fc));
            }
        } else if c != b'r' {
            return -1;
        }
    }
    let n = body.len() as i32;
    if wa.emit_phones(&word, &phones, coff, coff + n, 0x41, 0x59, 1) {
        n
    } else {
        0
    }
}

/// `FUN_100518b0`: `word \xff letter`; the letter becomes the record's
/// part-of-speech byte (+0x30).
fn partofsp(wa: &mut Wa, t: &[u8], cp: usize, coff: i32, close: usize) -> i32 {
    let body = &t[cp..close];
    if body.len() > 0x26 || body.is_empty() {
        return -1;
    }
    let Some(ff) = body.iter().position(|&c| c == 0xff) else { return -1 };
    if ff == 0 || ff > 0x1e || body.len() - ff != 2 {
        return -1;
    }
    let pos = match body[ff + 1] {
        b'I' => 0x24,
        b'M' => 0x0e,
        b'N' => 0x13,
        b'V' => 0x25,
        _ => 0xff,
    };
    let n = body.len() as i32;
    if wa.emit_pos(&body[..ff], pos, coff, coff + n, 0x41, 0x44, 1) {
        n
    } else {
        0
    }
}

/// `FUN_100514f0`: the alias text, word by word (mode 8), every record
/// spanning the whole content; numbers read as cardinals.
fn sub(fe: &Fe, wa: &mut Wa, t: &[u8], cp: usize, coff: i32, close: usize) -> i32 {
    let end = coff + (close - cp) as i32;
    let (mut p, mut o) = (cp, coff);
    let mut total = 0;
    while p < close {
        let mut x = Tok::default();
        let r = tokenize(&mut x, t, p, o, 8, fe.ud.as_ref());
        if r == 0 || x.class == 9 || x.len == 0 {
            return -1;
        }
        if x.class == 5 {
            return total;
        }
        let ok = if x.class == 1 && all_alpha(&x.text) {
            wa.emit(x.text(), coff, end, 0x41, 0x44, 0x18)
        } else {
            if x.class != 2 || !numeric(&x.text) {
                return -1;
            }
            let mut buf = new_buf();
            number(&mut buf, &x.text, 0);
            wa.emit_split(&buf, coff, end, 0x44, 0x44, 0x18)
        };
        if !ok {
            return 0;
        }
        total += r;
        p += r as usize;
        o += r;
    }
    total
}

/// `[|label|]` prefix shared by the time and phone handlers.
fn label_prefix(t: &[u8], cp: usize, close: usize) -> Option<(i32, i32)> {
    if t[cp] != b'|' {
        return Some((-1, 0));
    }
    let bar = t[cp + 1..close].iter().position(|&c| c == b'|')?;
    Some((atoi(&t[cp + 1..cp + 1 + bar]), bar as i32 + 2))
}

/// `FUN_100500c0` (`L`): digit groups read digit by digit (class 0x1b); `x`
/// says "extension"; `"` and `x` put a pause (or the label) on the record
/// before.
fn phone(fe: &Fe, wa: &mut Wa, t: &[u8], cp: usize, coff: i32, close: usize) -> i32 {
    let end = coff + (close - cp) as i32;
    let Some((label, mut total)) = label_prefix(t, cp, close) else { return -1 };
    let (mut p, mut o) = (cp + total as usize, coff + total);
    let mark = |wa: &mut Wa| {
        let n = wa.n() as i32;
        if n > 0 {
            if label < 0 {
                wa.set_ri32(n - 1, 0x2c, 1);
            } else {
                wa.set_ri32(n - 1, 0xc, label);
            }
        }
    };
    while p < close {
        let mut x = Tok::default();
        let r = tokenize(&mut x, t, p, o, 5, fe.ud.as_ref());
        if r == 0 || x.class == 9 || x.len == 0 {
            return -1;
        }
        if x.class == 5 {
            return total;
        }
        if x.class == 2 && all_digit(&x.text) {
            let mut buf = new_buf();
            digits(&mut buf, &x.text, 1, 1);
            if !wa.emit_split(&buf, coff, end, 0x44, 0x44, 0x1b) {
                return 0;
            }
        } else if x.text[0] == b'x' {
            if x.len != 1 {
                return -1;
            }
            mark(wa);
            if !wa.emit(s(0x10081330), coff, end, 0x41, 0x44, 0x1b) {
                return 0;
            }
        } else {
            if x.text[0] != b'"' || x.len != 1 {
                return -1;
            }
            mark(wa);
        }
        total += r;
        p += r as usize;
        o += r;
    }
    total
}

const CODE_TAB: Rec66 = rec66(0x10082d28, 0x1009b058);
const NAME_TAB: Rec66 = rec66(0x10087150, 0x1009b05c);
const NAME_TAB2: Rec66 = rec66(0x100899f8, 0x1009b060);

/// `FUN_10062530`: the plural of a table word (`-suffix`, an irregular
/// spelling, or empty for invariant).
fn plural_of(key: &[u8], pl: &[u8]) -> Vec<u8> {
    match pl.first() {
        None => key.to_vec(),
        Some(b'-') => [key, &pl[1..]].concat(),
        Some(_) => pl.to_vec(),
    }
}

/// `FUN_100367e0`: the subunit ("cent") of a currency name, pluralized when
/// asked; `None` when the currency has none.
fn subunit(name: &[u8], plural: bool) -> Option<Vec<u8>> {
    let i = NAME_TAB.find_exact(name, true);
    let (tab, j) = if i >= 0 {
        (NAME_TAB, i)
    } else {
        let j = NAME_TAB2.find_exact(name, false);
        if j < 0 {
            return None;
        }
        (NAME_TAB2, j)
    };
    let key = tab.field(j, 0x32);
    if key.is_empty() {
        return None;
    }
    Some(if plural { plural_of(key, tab.field(j, 0x41)) } else { key.to_vec() })
}

/// `FUN_1004fbf0` (`J`): an ISO code then an amount ("USD12.50"): "twelve
/// dollars and fifty cents", or without a two-digit subunit "N <country>
/// <name>". Class 3; every record spans the whole content.
fn currency(fe: &Fe, wa: &mut Wa, t: &[u8], cp: usize, coff: i32, close: usize) -> i32 {
    let end = coff + (close - cp) as i32;
    let Some((label, mut total)) = label_prefix(t, cp, close) else { return -1 };
    let (mut p, mut o) = (cp + total as usize, coff + total);
    let mut code = -1;
    let num = |wa: &mut Wa, w: &[u8]| {
        let mut buf = new_buf();
        number(&mut buf, w, 0);
        wa.emit_split(&buf, coff, end, 0x44, 0x44, 3)
    };
    while p < close {
        let mut x = Tok::default();
        let r = tokenize(&mut x, t, p, o, 5, fe.ud.as_ref());
        if r == 0 || x.class == 9 || x.len == 0 {
            return -1;
        }
        if x.class == 5 {
            return total;
        }
        let mut other = true;
        if x.class == 1 && code < 0 {
            if all_alpha(&x.text) && x.len == 3 {
                code = CODE_TAB.find_exact(&x.text, true);
                other = code < 0;
            }
        }
        if other && x.class == 2 && code >= 0 && numeric(&x.text) {
            other = false;
            let (ip, fp) = split_decimal(&x.text);
            let iv = atoi(&ip);
            let fv = atoi(&fp);
            let name_key = CODE_TAB.field(code, 0xf);
            let name_pl = CODE_TAB.field(code, 0x32);
            let sub = if fv != 0 && fp.len() == 2 { subunit(name_key, fv > 1) } else { None };
            match sub {
                None => {
                    if !num(wa, if fv == 0 { &ip } else { x.text() }) {
                        return 0;
                    }
                    let name = if iv != 1 || fv != 0 { plural_of(name_key, name_pl) } else { name_key.to_vec() };
                    if !wa.emit_split(CODE_TAB.field(code, 0x41), coff, end, 0x41, 0x44, 3) || !wa.emit_split(&name, coff, end, 0x41, 0x44, 3) {
                        return 0;
                    }
                }
                Some(sub) => {
                    if iv != 0 {
                        let name = if iv > 1 { plural_of(name_key, name_pl) } else { name_key.to_vec() };
                        if !num(wa, &ip) || !wa.emit_split(&name, coff, end, 0x41, 0x44, 3) || !wa.emit(s(0x10077400), coff, end, 0x41, 0x44, 3) {
                            return 0;
                        }
                    }
                    if !num(wa, &fp) || !wa.emit_split(&sub, coff, end, 0x41, 0x44, 3) {
                        return 0;
                    }
                }
            }
            code = -1;
        }
        if other {
            if x.text[0] != b'"' || x.len != 1 {
                return -1;
            }
            let n = wa.n() as i32;
            if n > 0 {
                if label < 0 {
                    wa.set_ri32(n - 1, 0x2c, 1);
                } else {
                    wa.set_ri32(n - 1, 0xc, label);
                }
            }
        }
        total += r;
        p += r as usize;
        o += r;
    }
    total
}

/// Informal-name symbol records (rec5a): key @0, name @0x1e, plural @0x3c.
const SYMBOL_TAB: Rec5a = rec5a(0x10082350, 0x1009b050);

/// `FUN_100502c0` (`O`, informal currency): a 3-letter ISO code (`CODE_TAB`)
/// or a spelled symbol/name (`SYMBOL_TAB`, matched by prefix), then an
/// amount; the same reading as `J`'s currency, but the symbol path speaks
/// the name directly (no country field) and the amount word list, when the
/// code index was still open on a `"` token, mirrors `J`'s labeling. Class
/// 3; every record spans the whole content.
fn currency_informal(fe: &Fe, wa: &mut Wa, t: &[u8], cp: usize, coff: i32, close: usize) -> i32 {
    let end = coff + (close - cp) as i32;
    let Some((label, mut total)) = label_prefix(t, cp, close) else { return -1 };
    let (mut p, mut o) = (cp + total as usize, coff + total);
    let (mut code, mut is_symbol) = (-1i32, false);
    let num = |wa: &mut Wa, w: &[u8]| {
        let mut buf = new_buf();
        number(&mut buf, w, 0);
        wa.emit_split(&buf, coff, end, 0x44, 0x44, 3)
    };
    while p < close {
        let mut x = Tok::default();
        let r = tokenize(&mut x, t, p, o, 5, fe.ud.as_ref());
        if r == 0 || x.class == 9 || x.len == 0 {
            return -1;
        }
        if x.class == 5 {
            return total;
        }
        let mut handled = false;
        if x.class == 1 && code < 0 {
            if all_alpha(&x.text) && x.len == 3 {
                let i = CODE_TAB.find_exact(&x.text, true);
                if i >= 0 {
                    code = i;
                    is_symbol = false;
                    handled = true;
                }
            }
        } else if x.class == 3 && code < 0 && x.len <= 2 {
            let i = SYMBOL_TAB.find_prefix(&x.text, true);
            if i >= 0 {
                code = i;
                is_symbol = true;
                handled = true;
            }
        }
        if !handled && x.class == 2 && code >= 0 && numeric(&x.text) {
            let (ip, fp) = split_decimal(&x.text);
            let iv = atoi(&ip);
            let fv = atoi(&fp);
            let (name, name_pl): (&[u8], &[u8]) =
                if is_symbol { (SYMBOL_TAB.field(code, 0x1e), SYMBOL_TAB.field(code, 0x3c)) } else { (CODE_TAB.field(code, 0xf), CODE_TAB.field(code, 0x32)) };
            let sub = if fv != 0 && fp.len() == 2 { subunit(name, fv > 1) } else { None };
            let ok = match sub {
                None => {
                    if !num(wa, if fv == 0 { &ip } else { x.text() }) {
                        false
                    } else {
                        let pname = if iv != 1 || fv != 0 { plural_of(name, name_pl) } else { name.to_vec() };
                        if !is_symbol {
                            wa.emit_split(CODE_TAB.field(code, 0x41), coff, end, 0x41, 0x44, 3) && wa.emit_split(&pname, coff, end, 0x41, 0x44, 3)
                        } else {
                            wa.emit(&pname, coff, end, 0x53, 0x44, 3)
                        }
                    }
                }
                Some(sub) => {
                    let whole_ok = if iv != 0 {
                        let pname = if iv > 1 { plural_of(name, name_pl) } else { name.to_vec() };
                        num(wa, &ip) && wa.emit_split(&pname, coff, end, 0x41, 0x44, 3) && wa.emit(s(0x10077400), coff, end, 0x41, 0x44, 3)
                    } else {
                        true
                    };
                    whole_ok && num(wa, &fp) && wa.emit_split(&sub, coff, end, 0x41, 0x44, 3)
                }
            };
            if !ok {
                return 0;
            }
            code = -1;
            handled = true;
        }
        if !handled {
            if x.text[0] != b'"' || x.len != 1 {
                return -1;
            }
            mark(wa, label, 1);
        }
        total += r;
        p += r as usize;
        o += r;
    }
    total
}

/// `FUN_1004eba0`'s stack table: the names of the characters in
/// `0x100810ec`, in the same order.
fn char_names() -> [&'static [u8]; 31] {
    [
        s(0x10081324), s(0x10081318), s(0x1008130c), s(0x10081304), s(0x100812f8), s(0x100812f0), s(0x100812dc), s(0x100812c8), s(0x100811bc), s(0x10080460), s(0x100812c0),
        s(0x100779a8), s(0x100812b8), s(0x100806e4), s(0x100812b0), s(0x100812a4), s(0x10081290), s(0x10081288), s(0x10081274), s(0x10081264), s(0x1008125c), s(0x10081248),
        s(0x1008123c), s(0x10081224), s(0x1008121c), s(0x10081210), s(0x10081208), s(0x100811f4), s(0x100811e4), s(0x100811d0), s(0x100811c8),
    ]
}

/// `FUN_1004eba0` (`D`, say-as characters): words as they are (`b='S'`),
/// digit strings digit by digit, punctuation by name. Class 0; every record
/// spans the whole content. The optional label is `\xff`-delimited here.
fn characters(fe: &Fe, wa: &mut Wa, t: &[u8], cp: usize, coff: i32, close: usize) -> i32 {
    let end = coff + (close - cp) as i32;
    let (mut label, mut total) = (-1, 0);
    if t[cp] == 0xff {
        let Some(bar) = t[cp + 1..close].iter().position(|&c| c == 0xff) else { return -1 };
        label = atoi(&t[cp + 1..cp + 1 + bar]);
        total = bar as i32 + 2;
    }
    let (mut p, mut o) = (cp + total as usize, coff + total);
    let syms = s(0x100810ec);
    while p < close {
        let mut x = Tok::default();
        let r = tokenize(&mut x, t, p, o, 0x1c, fe.ud.as_ref());
        if r == 0 || x.class == 9 || x.len == 0 {
            return -1;
        }
        if x.class == 5 {
            return total;
        }
        let c0 = x.text[0];
        let ok = if x.class == 1 && all_alpha(&x.text) {
            wa.emit(x.text(), coff, end, 0x41, 0x53, 0)
        } else if x.class == 2 && all_digit(&x.text) {
            let mut buf = new_buf();
            digits(&mut buf, &x.text, 1, 0);
            wa.emit_split(&buf, coff, end, 0x44, 0x44, 0)
        } else if c0 == b'"' && x.len == 1 {
            let n = wa.n() as i32;
            if n > 0 {
                if label < 0 {
                    wa.set_ri32(n - 1, 0x2c, 1);
                } else {
                    wa.set_ri32(n - 1, 0xc, label);
                }
            }
            true
        } else {
            let Some(k) = syms.iter().position(|&c| c == c0) else { return -1 };
            if x.len != 1 {
                return -1;
            }
            wa.emit_split(char_names()[k], coff, end, 0x53, 0x44, 0)
        };
        if !ok {
            return 0;
        }
        total += r;
        p += r as usize;
        o += r;
    }
    total
}

fn atof(w: &[u8]) -> f64 {
    let w = cs(w);
    let n = w.iter().take_while(|&&c| c.is_ascii_digit() || c == b'.').count();
    std::str::from_utf8(&w[..n]).ok().and_then(|s| s.parse().ok()).unwrap_or(0.0)
}

#[derive(Default)]
struct Clock {
    hour: Vec<u8>,
    min: Vec<u8>,
    sec: Vec<u8>,
    ampm: &'static [u8],
    field: i32,
    dur: bool,
}

struct Span {
    start: i32,
    end: i32,
    label: i32,
}

impl Span {
    fn number(&self, wa: &mut Wa, w: &[u8]) -> bool {
        let mut buf = new_buf();
        number(&mut buf, w, 0);
        wa.emit_split(&buf, self.start, self.end, 0x44, 0x44, 5)
    }

    fn word(&self, wa: &mut Wa, w: &[u8], a: u8) -> bool {
        wa.emit(w, self.start, self.end, a, 0x44, 5)
    }

    fn label(&self, wa: &mut Wa) {
        let n = wa.n() as i32;
        if self.label >= 0 && n > 0 {
            wa.set_ri32(n - 1, 0xc, self.label);
        }
    }

    /// "and" N second(s); `by_value` picks the unit from the float value
    /// (the `h:m:s` form) rather than from `atoi` (the duration form), and
    /// only when the value is above zero.
    fn seconds(&self, wa: &mut Wa, sec: &[u8], by_value: bool) -> bool {
        let f = atof(sec);
        if by_value && f <= 0.0 {
            return true;
        }
        self.label(wa);
        if !self.word(wa, s(0x10077400), 0x41) {
            return false;
        }
        self.label(wa);
        if !self.number(wa, sec) {
            return false;
        }
        let one = if by_value { f <= 1.0 } else { atoi(cs(sec)) < 2 };
        wa.emit(if one { s(0x10078918) } else { s(0x100811a0) }, self.start, self.end, 0x44, 0x44, 5)
    }

    fn minutes(&self, wa: &mut Wa, min: &[u8]) -> bool {
        if !self.number(wa, min) {
            return false;
        }
        let w = if atoi(cs(min)) < 2 { s(0x100774e8) } else { s(0x100811a8) };
        self.word(wa, w, 0x41)
    }

    /// One clock reading. `last` is the reading at the end of the tag, whose
    /// minutes are read in pairs; a reading closed by `"` reads them as a
    /// cardinal.
    fn flush(&self, wa: &mut Wa, c: &Clock, last: bool) -> bool {
        if c.dur {
            if !c.min.is_empty() && !self.minutes(wa, &c.min) {
                return false;
            }
            return c.sec.is_empty() || self.seconds(wa, &c.sec, false);
        }
        if !c.hour.is_empty() && !self.number(wa, &c.hour) {
            return false;
        }
        if c.min.is_empty() {
            if c.sec.is_empty() && c.ampm.is_empty() && !wa.emit_split(s(0x10081198), self.start, self.end, 0x41, 0x44, 5) {
                return false;
            }
        } else if atoi(cs(&c.min)) < 1 {
            if c.ampm.is_empty() && !wa.emit_split(s(0x10081198), self.start, self.end, 0x41, 0x44, 5) {
                return false;
            }
        } else {
            self.label(wa);
            let mut buf = new_buf();
            if last {
                pairs(&mut buf, &c.min);
            } else {
                number(&mut buf, &c.min, 0);
            }
            if !wa.emit_split(&buf, self.start, self.end, 0x44, 0x44, 5) {
                return false;
            }
        }
        if !c.sec.is_empty() && !self.seconds(wa, &c.sec, true) {
            return false;
        }
        c.ampm.is_empty() || wa.emit_split(c.ampm, self.start, self.end, 0x41, 0x53, 5)
    }
}

/// `FUN_1004df70` (`B`/`M`/`N`): `[|label|] h[:m[:s]] [A|P]`, readings
/// separated by `"`; `|` instead of `:` makes it a minutes/seconds duration.
/// Every record spans the whole content.
fn time(fe: &Fe, wa: &mut Wa, t: &[u8], cp: usize, coff: i32, close: usize) -> i32 {
    let Some((label, mut total)) = label_prefix(t, cp, close) else { return -1 };
    let sp = Span { start: coff, end: coff + (close - cp) as i32, label };
    let (mut p, mut o) = (cp + total as usize, coff + total);
    let mut c = Clock::default();
    while p < close {
        let mut x = Tok::default();
        let r = tokenize(&mut x, t, p, o, 0xf, fe.ud.as_ref());
        if r == 0 || x.class == 9 || x.len == 0 {
            return -1;
        }
        if x.class == 5 {
            break;
        }
        let c0 = x.text[0];
        if c0 != 0 && strchr(s(0x100811b0), c0) && x.len == 1 {
            c.ampm = if c0 == b'A' { s(0x10081084) } else { s(0x10081074) };
        } else if x.class == 2 && numeric(&x.text) {
            let v = x.text().to_vec();
            match c.field {
                0 => c.hour = v,
                1 => c.min = v,
                2 => c.sec = v,
                _ => return -1,
            }
        } else if x.len != 1 {
            return -1;
        } else if c0 == b':' {
            c.field += 1;
        } else if c0 == b'|' {
            c.dur = true;
            c.field += 1;
        } else if c0 == b'"' {
            if !sp.flush(wa, &c, false) {
                return 0;
            }
            sp.label(wa);
            c = Clock::default();
        } else {
            return -1;
        }
        total += r;
        p += r as usize;
        o += r;
    }
    if !sp.flush(wa, &c, true) {
        return 0;
    }
    total
}

/// `"` after a record marks it with a pause (`code`, matching each caller's
/// engine literal) or, with a `|label|` prefix, the label.
fn mark(wa: &mut Wa, label: i32, code: i32) {
    let n = wa.n() as i32;
    if n > 0 {
        if label < 0 {
            wa.set_ri32(n - 1, 0x2c, code);
        } else {
            wa.set_ri32(n - 1, 0xc, label);
        }
    }
}

/// `FUN_1004dd20` (`A`/`H`, date): up to 3 numeric fields in fixed
/// month/day/year order (a literal `N` token silently skips a field, an
/// engine quirk), `/` optionally marks the label; every record spans the
/// whole content.
fn saydate(fe: &Fe, wa: &mut Wa, t: &[u8], cp: usize, coff: i32, close: usize) -> i32 {
    let end = coff + (close - cp) as i32;
    let Some((label, mut total)) = label_prefix(t, cp, close) else { return -1 };
    let (mut p, mut o) = (cp + total as usize, coff + total);
    let mut field = 3;
    while p < close {
        let mut x = Tok::default();
        let r = tokenize(&mut x, t, p, o, 3, fe.ud.as_ref());
        if r == 0 || x.class == 9 || x.len == 0 {
            return -1;
        }
        if x.class == 5 {
            return total;
        }
        let ok = if x.len == 1 && x.text[0] == b'N' {
            field -= 1;
            true
        } else if x.class == 2 {
            let mut buf = new_buf();
            match field {
                3 => strcpy(&mut buf, month_name(atoi(&x.text))),
                2 => ordinal_numeral_words(&mut buf, &x.text),
                1 => {
                    let v = atoi(&x.text);
                    if v < 1000 && (x.len > 2 || v > 99) {
                        number(&mut buf, &x.text, 1);
                    } else {
                        super::numwords::year(&mut buf, &x.text);
                    }
                }
                _ => return -1,
            }
            field -= 1;
            wa.emit_split(&buf, coff, end, 0x44, 0x44, 4)
        } else if x.len == 1 && x.text[0] == b'/' {
            if label >= 0 {
                mark(wa, label, 0);
            }
            true
        } else {
            return -1;
        };
        if !ok {
            return 0;
        }
        total += r;
        p += r as usize;
        o += r;
    }
    total
}

/// `FUN_1004e9a0` (`C`, phone keys): digit groups via the shared phone group
/// reader; `*`/`#` spelled "asterisk"/"sharp"; `-` marks a pause (code 7) or
/// the label, `+` is accepted but silent.
fn phonekeys(fe: &Fe, wa: &mut Wa, t: &[u8], cp: usize, coff: i32, close: usize) -> i32 {
    let end = coff + (close - cp) as i32;
    let Some((label, mut total)) = label_prefix(t, cp, close) else { return -1 };
    let (mut p, mut o) = (cp + total as usize, coff + total);
    while p < close {
        let mut x = Tok::default();
        let r = tokenize(&mut x, t, p, o, 6, fe.ud.as_ref());
        if r == 0 || x.class == 9 || x.len == 0 {
            return -1;
        }
        if x.class == 5 {
            return total;
        }
        let ok = if x.class == 2 && all_digit(&x.text) {
            let w = group_words(&x.text);
            wa.emit_split(&w, coff, end, 0x44, 0x44, 0x1b)
        } else if x.len == 1 && matches!(x.text[0], b'*' | b'#') {
            let w = if x.text[0] == b'*' { s(0x100811bc) } else { s(0x100811b4) };
            wa.emit(w, coff, end, 0x53, 0x44, 0x1b)
        } else if x.len == 1 && x.text[0] == b'-' {
            mark(wa, label, 7);
            true
        } else if x.len == 1 && x.text[0] == b'+' {
            true
        } else {
            return -1;
        };
        if !ok {
            return 0;
        }
        total += r;
        p += r as usize;
        o += r;
    }
    total
}

/// `FUN_1004f500` (`E`/`K`, cardinal): one number; `+`/`-` spelled; `"` a
/// pause or the label.
fn numeral(fe: &Fe, wa: &mut Wa, t: &[u8], cp: usize, coff: i32, close: usize) -> i32 {
    let end = coff + (close - cp) as i32;
    let Some((label, mut total)) = label_prefix(t, cp, close) else { return -1 };
    let (mut p, mut o) = (cp + total as usize, coff + total);
    while p < close {
        let mut x = Tok::default();
        let r = tokenize(&mut x, t, p, o, 2, fe.ud.as_ref());
        if r == 0 || x.class == 9 || x.len == 0 {
            return -1;
        }
        if x.class == 5 {
            return total;
        }
        let ok = if x.class == 2 && numeric(&x.text) {
            let mut buf = new_buf();
            number(&mut buf, &x.text, 1);
            wa.emit_split(&buf, coff, end, 0x44, 0x44, 0x1c)
        } else if x.len == 1 && x.text[0] != 0 && strchr(s(0x100810c8), x.text[0]) {
            let w = if x.text[0] == b'+' { s(0x10080460) } else { s(0x10080458) };
            wa.emit(w, coff, end, 0x53, 0x44, 0x1c)
        } else if x.len == 1 && x.text[0] == b'"' {
            mark(wa, label, 1);
            true
        } else {
            return -1;
        };
        if !ok {
            return 0;
        }
        total += r;
        p += r as usize;
        o += r;
    }
    total
}

/// `FUN_1004f710` (`F`, ordinal): one number read as an ordinal; `"` a
/// pause or the label.
fn saydinal(fe: &Fe, wa: &mut Wa, t: &[u8], cp: usize, coff: i32, close: usize) -> i32 {
    let end = coff + (close - cp) as i32;
    let Some((label, mut total)) = label_prefix(t, cp, close) else { return -1 };
    let (mut p, mut o) = (cp + total as usize, coff + total);
    while p < close {
        let mut x = Tok::default();
        let r = tokenize(&mut x, t, p, o, 2, fe.ud.as_ref());
        if r == 0 || x.class == 9 || x.len == 0 {
            return -1;
        }
        if x.class == 5 {
            return total;
        }
        let ok = if x.class == 2 && numeric(&x.text) {
            let mut buf = new_buf();
            ordinal_numeral_words(&mut buf, &x.text);
            wa.emit_split(&buf, coff, end, 0x44, 0x44, 0x1c)
        } else if x.len == 1 && x.text[0] == b'"' {
            mark(wa, label, 1);
            true
        } else {
            return -1;
        };
        if !ok {
            return 0;
        }
        total += r;
        p += r as usize;
        o += r;
    }
    total
}

/// `FUN_1004f8b0` (`G`, boolean): the literal `TRUE`/`FALSE` word; `"` a
/// pause or the label.
fn boolean(fe: &Fe, wa: &mut Wa, t: &[u8], cp: usize, coff: i32, close: usize) -> i32 {
    let end = coff + (close - cp) as i32;
    let Some((label, mut total)) = label_prefix(t, cp, close) else { return -1 };
    let (mut p, mut o) = (cp + total as usize, coff + total);
    while p < close {
        let mut x = Tok::default();
        let r = tokenize(&mut x, t, p, o, 8, fe.ud.as_ref());
        if r == 0 || x.class == 9 || x.len == 0 {
            return -1;
        }
        if x.class == 5 {
            return total;
        }
        let ok = if x.class == 1 && stricmp(&x.text, s(0x10081120)) == 0 {
            wa.emit(s(0x10081120), coff, end, 0x41, 0x44, 0x18)
        } else if x.class == 1 && stricmp(&x.text, s(0x10081118)) == 0 {
            wa.emit(s(0x10081118), coff, end, 0x41, 0x44, 0x18)
        } else if x.len == 1 && x.text[0] == b'"' {
            mark(wa, label, 1);
            true
        } else {
            return -1;
        };
        if !ok {
            return 0;
        }
        total += r;
        p += r as usize;
        o += r;
    }
    total
}

/// `FUN_1004fa60` (`I`, digits): one number read digit by digit ("zero" for
/// `0`); `"` a pause or the label.
fn digits_tag(fe: &Fe, wa: &mut Wa, t: &[u8], cp: usize, coff: i32, close: usize) -> i32 {
    let end = coff + (close - cp) as i32;
    let Some((label, mut total)) = label_prefix(t, cp, close) else { return -1 };
    let (mut p, mut o) = (cp + total as usize, coff + total);
    while p < close {
        let mut x = Tok::default();
        let r = tokenize(&mut x, t, p, o, 2, fe.ud.as_ref());
        if r == 0 || x.class == 9 || x.len == 0 {
            return -1;
        }
        if x.class == 5 {
            return total;
        }
        let ok = if x.class == 2 && all_digit(&x.text) {
            let mut buf = new_buf();
            digits(&mut buf, &x.text, 1, 1);
            wa.emit_split(&buf, coff, end, 0x44, 0x44, 0x1c)
        } else if x.len == 1 && x.text[0] == b'"' {
            mark(wa, label, 1);
            true
        } else {
            return -1;
        };
        if !ok {
            return 0;
        }
        total += r;
        p += r as usize;
        o += r;
    }
    total
}

/// `FUN_10050890` (`P`, fraction): `N/N`, a cardinal numerator then an
/// ordinal denominator (pluralized with a literal `s`); `+` glues a
/// following fraction (a mixed number), `"` a pause or the label. The
/// plural flag is sticky, not reassigned per numerator: it's set true when
/// *any* numerator seen since the last denominator was consumed (or the
/// start) has value > 1, and reset to false only once actually used by a
/// denominator - so for a mixed number like `5+1/2` the whole-part
/// numerator "5" (>1) still makes the denominator plural ("seconds"), even
/// though the immediately-preceding numerator token was "1".
fn fraction_tag(fe: &Fe, wa: &mut Wa, t: &[u8], cp: usize, coff: i32, close: usize) -> i32 {
    let end = coff + (close - cp) as i32;
    let Some((label, mut total)) = label_prefix(t, cp, close) else { return -1 };
    let (mut p, mut o) = (cp + total as usize, coff + total);
    let (mut denom, mut plural) = (false, false);
    while p < close {
        let mut x = Tok::default();
        let r = tokenize(&mut x, t, p, o, 0xd, fe.ud.as_ref());
        if r == 0 || x.class == 9 || x.len == 0 {
            return -1;
        }
        if x.class == 5 {
            return total;
        }
        let ok = if !denom && x.class == 2 && numeric(&x.text) {
            let ip = match x.text.iter().position(|&c| c == b'.') {
                Some(d) => digits_only(&x.text[..d]),
                None => digits_only(&x.text),
            };
            if atoi(&ip) > 1 {
                plural = true;
            }
            let mut buf = new_buf();
            number(&mut buf, &x.text, 1);
            wa.emit_split(&buf, coff, end, 0x44, 0x44, 0xd)
        } else if denom && x.class == 2 && numeric(&x.text) {
            let mut buf = new_buf();
            ordinal_numeral_words(&mut buf, &x.text);
            if plural && !fe.era.is_2009() {
                strcat(&mut buf, s(0x10077770));
                plural = false;
            }
            denom = false;
            wa.emit_split(&buf, coff, end, 0x44, 0x44, 0xd)
        } else if x.len == 1 && x.text[0] == b'/' {
            denom = true;
            true
        } else if x.len == 1 && x.text[0] == b'+' {
            true
        } else if x.len == 1 && x.text[0] == b'"' {
            mark(wa, label, 1);
            true
        } else {
            return -1;
        };
        if !ok {
            return 0;
        }
        total += r;
        p += r as usize;
        o += r;
    }
    total
}

/// `FUN_10051130` (`V`, duration): up to 3 numeric fields in fixed
/// hours/minutes/seconds order (a literal `N` token silently skips a
/// field), each followed by its own unit word ("hour(s)"/"minute(s)", "and"
/// + "second(s)" for the last); `:` optionally marks the label. Every
/// record spans the whole content.
fn duration(fe: &Fe, wa: &mut Wa, t: &[u8], cp: usize, coff: i32, close: usize) -> i32 {
    let end = coff + (close - cp) as i32;
    let Some((label, mut total)) = label_prefix(t, cp, close) else { return -1 };
    let (mut p, mut o) = (cp + total as usize, coff + total);
    let mut field = 3;
    let mut started = false;
    while p < close {
        let mut x = Tok::default();
        let r = tokenize(&mut x, t, p, o, 0xf, fe.ud.as_ref());
        if r == 0 || x.class == 9 || x.len == 0 {
            return -1;
        }
        if x.class == 5 {
            return total;
        }
        let ok = if x.len == 1 && x.text[0] == b'N' {
            field -= 1;
            true
        } else if x.class == 2 && numeric(&x.text) {
            match field {
                3 => {
                    let mut buf = new_buf();
                    number(&mut buf, &x.text, 0);
                    if !wa.emit_split(&buf, coff, end, 0x44, 0x44, 5) {
                        false
                    } else {
                        let w = if atoi(&x.text) < 2 { s(0x10081340) } else { s(0x10081348) };
                        started = true;
                        field -= 1;
                        wa.emit(w, coff, end, 0x41, 0x44, 5)
                    }
                }
                2 => {
                    let mut buf = new_buf();
                    number(&mut buf, &x.text, 0);
                    if !wa.emit_split(&buf, coff, end, 0x44, 0x44, 5) {
                        false
                    } else {
                        let w = if atoi(&x.text) > 1 { s(0x100811a8) } else { s(0x100774e8) };
                        field -= 1;
                        wa.emit(w, coff, end, 0x41, 0x44, 5)
                    }
                }
                1 => {
                    if started && !wa.emit(s(0x10077400), coff, end, 0x41, 0x44, 5) {
                        false
                    } else {
                        let mut buf = new_buf();
                        number(&mut buf, &x.text, 0);
                        if !wa.emit_split(&buf, coff, end, 0x44, 0x44, 5) {
                            false
                        } else {
                            let f = atof(&x.text);
                            let w = if f <= 1.0 { s(0x10078918) } else { s(0x100811a0) };
                            field -= 1;
                            wa.emit(w, coff, end, 0x44, 0x44, 5)
                        }
                    }
                }
                _ => return -1,
            }
        } else if x.len == 1 && x.text[0] == b':' {
            if label >= 0 {
                mark(wa, label, 0);
            }
            true
        } else {
            return -1;
        };
        if !ok {
            return 0;
        }
        total += r;
        p += r as usize;
        o += r;
    }
    total
}

/// `FUN_1005d9d0`'s three lookup tables, the same `norm_url::piece` scans.
const CC_TLD: Ptab = ptab(0x10081764, 0x1009b07c);
const TLD: Ptab = ptab(0x1008172c, 0x1009b074);
const SLD: Ptab = ptab(0x10081714, 0x1009b078);

fn find(tab: Ptab, w: &[u8]) -> i32 {
    search(tab.count(), true, w, |i| tab.get(i))
}

/// `FUN_10050ec0` (`R`, `sapi:address`): a 4-5 digit all-digit token
/// (a ZIP).
fn is_zip45(text: &[u8], len: i32) -> bool {
    (4..6).contains(&len) && all_digit(text)
}

/// `FUN_10050b90` (`Q`, `sapi:email`/`sapi:web`): the bespoke URL/e-mail
/// tokenizer (`FUN_1005d9d0`, `norm_url::piece`) over `www` (spelled),
/// `@`/`.` (spelled "at"/"dot"), the CC_TLD/TLD/SLD tables (raw text, not
/// the table's own casing — the copied expansion string is computed then
/// discarded, confirmed in the disassembly at 0x10050d74 where the emitted
/// word is `local_60`, not the copy at `local_180`) and any other alpha
/// word of 2+ chars via `url_words`. A digit piece always fails (`local_64
/// != 1` at the `LAB_10050e39` fallback), so no IP-address-shaped content
/// reaches here, unlike `norm_url::url`. Class 0x10; `[|label|]` prefix,
/// applied after every record (Q has no `"` separator of its own).
fn netaddr(fe: &Fe, wa: &mut Wa, t: &[u8], cp: usize, coff: i32, close: usize) -> i32 {
    let end = coff + (close - cp) as i32;
    let Some((label, mut total)) = label_prefix(t, cp, close) else { return -1 };
    let (mut p, mut o) = (cp + total as usize, coff + total);
    let mut first = true;
    while p < close {
        let (x, r) = piece(t, p, o, if first { 0 } else { 1 });
        if r == 0 || x.class == 9 || x.len == 0 {
            return -1;
        }
        let w = x.text();
        let ok = if x.len == 1 && matches!(w[0], b'@' | b'.') {
            let word = if w[0] == b'@' { s(0x10077694) } else { s(0x1008133c) };
            wa.emit(word, coff, end, 0x53, 0x44, 0x10)
        } else if x.len == 3 && stricmp(s(0x10081150), w) == 0 {
            wa.emit(w, coff, end, 0x41, 0x53, 0x10)
        } else if find(CC_TLD, w) >= 0 {
            wa.emit(w, coff, end, 0x41, 0x53, 0x10)
        } else if find(TLD, w) >= 0 {
            wa.emit(w, coff, end, 0x41, 0x44, 0x10)
        } else if find(SLD, w) >= 0 {
            wa.emit(w, coff, end, 0x41, 0x53, 0x10)
        } else if x.len >= 2 && x.class == 1 {
            url_words(fe, wa, w, coff, end, 0x44) != 0
        } else {
            return -1;
        };
        if !ok {
            return 0;
        }
        if label >= 0 {
            let n = wa.n() as i32;
            if n > 0 {
                wa.set_ri32(n - 1, 0xc, label);
            }
        }
        first = false;
        total += r;
        p += r as usize;
        o += r;
    }
    total
}

/// `FUN_10045ac0`: `emit_split` where a 1-char chunk is spelled (`b=S`) and
/// any other chunk is read normally (`b=N`), independent of the caller's
/// own `b`.
fn emit_letters(wa: &mut Wa, text: &[u8], start: i32, end: i32, a: u8, class: u32) -> bool {
    for part in cs(text).split(|&c| c == b' ' || c == b'\t') {
        let b = if part.len() == 1 { 0x53 } else { 0x44 };
        if !wa.emit(part, start, end, a, b, class) {
            return false;
        }
    }
    true
}

/// `FUN_10050ec0` (`R`, `sapi:address`): 4-5 digit ZIP groups
/// (`is_zip45`/`FUN_100543e0`) read in pairs (`numwords::pairs`); Canadian
/// postal FSA/LDU halves (`norm_address_csc::postal`) read letter-as-typed,
/// digit-as-word (`mixed_code`, each single-char chunk spelled via
/// `emit_letters`); `-` marks the label only, silently doing nothing
/// without one (unlike every other letter's `"`, which falls back to a
/// pause); `"` a pause (code 1) or the label. Class 0x12; `[|label|]`
/// prefix.
fn address(fe: &Fe, wa: &mut Wa, t: &[u8], cp: usize, coff: i32, close: usize) -> i32 {
    let end = coff + (close - cp) as i32;
    let Some((label, mut total)) = label_prefix(t, cp, close) else { return -1 };
    let (mut p, mut o) = (cp + total as usize, coff + total);
    while p < close {
        let mut x = Tok::default();
        let r = tokenize(&mut x, t, p, o, 0x11, fe.ud.as_ref());
        if r == 0 || x.class == 9 || x.len == 0 {
            return -1;
        }
        if x.class == 5 {
            return total;
        }
        let ok = if x.class == 2 && is_zip45(x.text(), x.len) {
            let mut buf = new_buf();
            pairs(&mut buf, x.text());
            wa.emit_split(&buf, coff, end, 0x44, 0x44, 0x12)
        } else if x.class == 1 && postal(x.text()) == 1 {
            emit_letters(wa, &mixed_code(x.text()), coff, end, 0x41, 0x12)
        } else if x.class == 2 && postal(x.text()) == 2 {
            emit_letters(wa, &mixed_code(x.text()), coff, end, 0x44, 0x12)
        } else if x.len == 1 && x.text[0] == b'-' {
            if label >= 0 {
                let n = wa.n() as i32;
                if n > 0 {
                    wa.set_ri32(n - 1, 0xc, label);
                }
            }
            true
        } else if x.len == 1 && x.text[0] == b'"' {
            mark(wa, label, 1);
            true
        } else {
            return -1;
        };
        if !ok {
            return 0;
        }
        total += r;
        p += r as usize;
        o += r;
    }
    total
}
