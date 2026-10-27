//! `norm_phone` (0x1003ab90): telephone numbers. An optional `TEL`/`FAX` label (`(TEL)`, `TEL:`) and
//! `+`, then one of six layouts tried in turn on the same tokens (`1-402-555-1234`, `(402) 555-1234`,
//! `555-1234` next to a phone word, `402.555.1234`, ...), an extension (`x 12`, `ext. 12`) or a range
//! end (`~5`). Digit groups are read one digit at a time (`FUN_10041670`) straight into the WA.
//! Mode `g` is the say-as telephone format, `B` emits without consuming.

use super::mem::*;
use super::normalize::{Cx, State};
use super::num;
use super::rx::{is_p, restore, save, Buf as A, Save};
use super::stream::{token_at, Ahead};
use super::tokenize::Tok;
use super::wa::Wa;

/// Labels: `TEL`, `FAX` and three DBCS signs; the name at +0x10.
const LABELS: Rec30 = rec30(0x10073340, 0x1008486c);
/// `call`, `fax`, `phone`, `telephone`.
const WORDS: Ptab = ptab(0x10073430, 0x10084870);

/// `FUN_10041730`: 1 to 3 digits.
fn d1_3(t: &[u8]) -> bool {
    let l = cs(t).len();
    0 < l && l < 4 && num::all_digits(t)
}

/// `FUN_10041760`: an area code for country code `cc` (2 or 3 digits after `1`).
fn area(t: &[u8], cc: i32) -> bool {
    let l = cs(t).len();
    let ok = if cc == 1 { 1 < l && l < 4 } else { 0 < l && l < 4 };
    ok && num::all_digits(t)
}

/// `FUN_100417c0`: an exchange (3 digits after `1`, else 3 or 4).
fn exchange(t: &[u8], cc: i32) -> bool {
    let l = cs(t).len();
    let ok = if cc == 1 { l == 3 } else { 2 < l && l < 5 };
    ok && num::all_digits(t)
}

/// `FUN_10041810`.
fn d4(t: &[u8]) -> bool {
    cs(t).len() == 4 && num::all_digits(t)
}

fn one_of(t: &Tok, set: &[u8]) -> bool {
    t.len == 1 && t.text[0] != 0 && set.contains(&t.text[0])
}

fn ext_word(t: &[u8]) -> bool {
    [s(0x10069908), s(0x1006f4cc), s(0x1006f4c8), s(0x1006f4c4)].iter().any(|w| stricmp(w, t) == 0)
}

/// The closing bracket for `(` or `[`.
fn closes(open: u8, t: &Tok) -> bool {
    (open == b'(' && is_p(t, b')')) || (open == b'[' && is_p(t, b']'))
}

/// `FUN_10041670`: a digit group digit by digit, except `500` ("five hundred") and `1500`/`2000`
/// (as years).
pub fn spell_group(t: &[u8]) -> Vec<u8> {
    let t = cs(t);
    let mut out = Vec::new();
    let dig = |c: u8| (b'1'..=b'9').contains(&c);
    if t.len() == 3 && dig(t[0]) && t[1] == b'0' && t[2] == b'0' {
        return num::cardinal(t, 1);
    }
    if t.len() == 4 && dig(t[0]) && ((dig(t[1]) && t[2] == b'0' && t[3] == b'0') || &t[1..] == b"000") {
        return num::year(atoi(t));
    }
    num::spell(&mut out, t, 1, 0);
    out
}

/// `FUN_10041040`: a dotted number split into its groups (`402.555.1234`, `1.402.555.1234`). All four
/// stay empty when the pieces do not fit.
fn split_dotted(t: &[u8], cc: i32) -> [Vec<u8>; 4] {
    let mut out: [Vec<u8>; 4] = Default::default();
    let t = cs(t);
    let mut pieces: Vec<&[u8]> = Vec::new();
    let mut rest = t;
    let mut done = false;
    loop {
        let p = match rest.iter().position(|&c| c == b'.') {
            Some(i) => {
                let p = &rest[..i];
                rest = &rest[i + 1..];
                p
            }
            None => {
                let p = rest;
                rest = &[];
                done = true;
                p
            }
        };
        if p.is_empty() {
            break;
        }
        pieces.push(p);
        if pieces.len() > 5 {
            break;
        }
        if done && rest.is_empty() {
            break;
        }
    }
    let n = pieces.len();
    if n < 2 || !rest.is_empty() || n > 4 {
        return out;
    }
    match n {
        2 => {
            if exchange(pieces[0], cc) && d4(pieces[1]) {
                out[2] = pieces[0].to_vec();
                out[3] = pieces[1].to_vec();
            }
        }
        3 => {
            if area(pieces[0], cc) && exchange(pieces[1], cc) && d4(pieces[2]) {
                out[1] = pieces[0].to_vec();
                out[2] = pieces[1].to_vec();
                out[3] = pieces[2].to_vec();
            }
        }
        _ => {
            if d1_3(pieces[0]) {
                let c = atoi(pieces[0]);
                if area(pieces[1], c) && exchange(pieces[2], c) && d4(pieces[3]) {
                    for (o, p) in out.iter_mut().zip(pieces.iter()) {
                        *o = p.to_vec();
                    }
                }
            }
        }
    }
    out
}

/// `FUN_100413d0` in mode `B`: a phone word or label among the five tokens before buffered token `idx`,
/// or within the next six tokens of the text (a phone word, or `x`/`ext` with a short number).
fn context(a: &A, idx: i32) -> bool {
    for rel in (-5..=-1).rev() {
        let Some(t) = token_at(&a.st.done, &a.st.ahead, idx, rel) else { break };
        if (t.ty == 3 || t.ty == 1) && LABELS.find(&t.text, true) >= 0 {
            return true;
        }
        if t.ty == 1 && WORDS.find(&t.text, true) >= 0 {
            return true;
        }
    }
    let mut ah = Ahead::default();
    let mut c = *a.cur;
    let base = *a.cur;
    loop {
        let Some(k) = ah.push(a.cx.t, &mut c, base, b'D', a.cx.fe.ud.as_ref()) else { return false };
        let ty = ah.toks[k].ty;
        if ty == 8 || ty == 9 || ah.n >= 6 {
            break;
        }
    }
    let n = ah.n as usize;
    let short_num = |t: &Tok| t.ws < 3 && t.ty == 2 && t.len < 5 && num::all_digits(&t.text);
    for i in 0..n {
        let t = &ah.toks[i];
        if t.ty == 1 && WORDS.find(&t.text, true) >= 0 {
            return true;
        }
        if t.ws < 3 && i + 1 < n && t.ty == 1 && ext_word(&t.text) {
            let t1 = &ah.toks[i + 1];
            if short_num(t1) {
                return true;
            }
            if t1.ws == 0 && is_p(t1, b'.') && i + 2 < n && short_num(&ah.toks[i + 2]) {
                return true;
            }
        }
    }
    false
}

#[derive(Default)]
struct Parts {
    a4: Option<Vec<u8>>,
    p98: Option<Vec<u8>>,
    p9c: Option<Vec<u8>>,
    a8: Option<Vec<u8>>,
    ext: Option<Vec<u8>>,
    to: Option<Vec<u8>>,
}

impl Parts {
    fn clear(&mut self) {
        self.a4 = None;
        self.p98 = None;
        self.p9c = None;
        self.a8 = None;
    }
}

fn txt(t: &Tok) -> Option<Vec<u8>> {
    Some(t.text().to_vec())
}

fn cc_of(p: &Parts) -> i32 {
    atoi(p.a4.as_deref().unwrap_or(b""))
}

/// `FUN_10040f30`: an exchange, `-`, four digits at the last token. `None` leaves the buffer as it was
/// (but the exchange recorded).
fn tail(a: &mut A, p: &mut Parts, cc: i32) -> Option<Tok> {
    let t = a.last();
    if t.ty != 2 || !exchange(&t.text, cc) {
        return None;
    }
    p.p9c = txt(&t);
    let t = a.push()?;
    if t.ws < 3 && is_p(&t, b'-') {
        let Some(t) = a.push() else {
            a.seek(-1);
            return None;
        };
        if t.ws < 3 && t.ty == 2 && d4(&t.text) {
            p.a8 = txt(&t);
            return Some(t);
        }
        a.seek(-2);
        return None;
    }
    a.pop();
    None
}

fn tail_ok(a: &mut A, p: &mut Parts, cc: i32) -> bool {
    tail(a, p, cc).is_some_and(|t| t.ws < 3)
}

struct Ph {
    mode: u8,
    plus: bool,
    i11: i32,
}

/// Each layout: `Some(true)` read, `Some(false)` not this one, `None` buffer full.
type Att = fn(&mut A, &mut Parts, &Ph) -> Option<bool>;

/// `1-402-555-1234`, `1 (402) 555-1234`, `44 20 7946 0958`.
fn country(a: &mut A, p: &mut Parts, _ph: &Ph) -> Option<bool> {
    #[derive(Clone, Copy)]
    enum L {
        Open,
        Fail,
        Dash,
        Tail,
        Close,
        Match,
    }
    let t = a.last();
    if !(t.ty == 2 && d1_3(&t.text)) {
        return Some(false);
    }
    p.a4 = txt(&t);
    let cc = cc_of(p);
    let t = a.push()?;
    let mut l;
    if t.ws > 2 {
        l = L::Open;
    } else if is_p(&t, b'-') {
        let t = a.push()?;
        l = L::Fail;
        if t.ws < 3 {
            if t.ty == 2 && area(&t.text, cc) {
                p.p98 = txt(&t);
                let t = a.push()?;
                l = if t.ws < 3 && is_p(&t, b'-') { L::Dash } else { L::Fail };
            } else if one_of(&t, s(0x1006f4d4)) {
                let open = t.text[0];
                let t = a.push()?;
                if t.ws < 3 && t.ty == 2 && area(&t.text, cc) {
                    p.p98 = txt(&t);
                    let t = a.push()?;
                    if t.ws < 3 && closes(open, &t) {
                        l = L::Close;
                    }
                }
            }
        }
    } else if t.ty == 2 {
        if !area(&t.text, cc) {
            l = L::Open;
        } else {
            p.p98 = txt(&t);
            let t = a.push()?;
            if t.ws > 2 {
                l = L::Fail;
            } else if one_of(&t, s(0x1006f4d0)) {
                l = L::Dash;
            } else {
                l = L::Fail;
                if t.ty == 2 && exchange(&t.text, cc) {
                    p.p9c = txt(&t);
                    let t = a.push()?;
                    if t.ws <= 2 && t.ty == 2 && d4(&t.text) {
                        p.a8 = txt(&t);
                        l = L::Match;
                    }
                }
            }
        }
    } else {
        l = L::Open;
    }
    loop {
        l = match l {
            L::Open => {
                let t = a.last();
                let mut nx = L::Fail;
                if t.ws < 3 && one_of(&t, s(0x1006f4d4)) {
                    let open = t.text[0];
                    let t = a.push()?;
                    if t.ws < 3 && t.ty == 2 && area(&t.text, cc) {
                        p.p98 = txt(&t);
                        let t = a.push()?;
                        if t.ws < 3 && closes(open, &t) {
                            let t = a.push()?;
                            if t.ws <= 2 && one_of(&t, s(0x1006f4d0)) {
                                a.push()?;
                                nx = L::Tail;
                            } else {
                                nx = if tail_ok(a, p, cc) { L::Match } else { L::Fail };
                            }
                        }
                    }
                }
                nx
            }
            L::Close => {
                let t = a.push()?;
                if t.ws < 3 && is_p(&t, b'-') {
                    a.push()?;
                    L::Tail
                } else {
                    L::Fail
                }
            }
            L::Dash => {
                a.push()?;
                L::Tail
            }
            L::Tail => {
                if tail_ok(a, p, cc) {
                    L::Match
                } else {
                    L::Fail
                }
            }
            L::Match => return Some(true),
            L::Fail => return Some(false),
        };
    }
}

/// `402-555-1234`, `402/555-1234`, `555-1234` near a phone word.
fn area_first(a: &mut A, p: &mut Parts, ph: &Ph) -> Option<bool> {
    let t = a.last();
    if ph.plus || t.ty != 2 || !area(&t.text, 999) {
        return Some(false);
    }
    p.p98 = txt(&t);
    let t = a.push()?;
    if t.ws >= 3 {
        return Some(false);
    }
    if one_of(&t, s(0x1006f4d0)) {
        a.push()?;
        return Some(tail_ok(a, p, 999));
    }
    if t.ty != 2 || !exchange(&t.text, 999) {
        return Some(false);
    }
    p.p9c = txt(&t);
    four_after(a, p, ph, true)
}

/// After an exchange: four digits, or `-` and four digits, near a phone word unless in mode `g`.
/// `ctx_first` checks the context before reading past the `-`.
fn four_after(a: &mut A, p: &mut Parts, ph: &Ph, ctx_first: bool) -> Option<bool> {
    let ctx = |a: &A| ph.mode == b'g' || context(a, ph.i11 - 1);
    let t = a.push()?;
    if t.ws >= 3 {
        return Some(false);
    }
    if t.ty == 2 && d4(&t.text) && ctx(a) {
        p.a8 = txt(&t);
        return Some(true);
    }
    if is_p(&t, b'-') && (!ctx_first || ctx(a)) {
        let t = a.push()?;
        if t.ws < 3 && t.ty == 2 && d4(&t.text) && (ctx_first || ctx(a)) {
            p.a8 = txt(&t);
            return Some(true);
        }
    }
    Some(false)
}

/// `(402) 555-1234`, `[402] 555 1234`.
fn bracketed(a: &mut A, p: &mut Parts, ph: &Ph) -> Option<bool> {
    let t = a.last();
    if ph.plus || !one_of(&t, s(0x1006f4d4)) {
        return Some(false);
    }
    let open = t.text[0];
    let t = a.push()?;
    if !(t.ws < 3 && t.ty == 2 && area(&t.text, 999)) {
        return Some(false);
    }
    p.p98 = txt(&t);
    let t = a.push()?;
    if !(t.ws < 3 && closes(open, &t)) {
        return Some(false);
    }
    let t = a.push()?;
    if t.ws < 3 && one_of(&t, s(0x1006f4d0)) {
        a.push()?;
    }
    Some(tail_ok(a, p, 999))
}

/// `555-1234`, `555 1234` near a phone word.
fn local(a: &mut A, p: &mut Parts, ph: &Ph) -> Option<bool> {
    let t = a.last();
    if ph.plus || t.ty != 2 || !exchange(&t.text, 999) {
        return Some(false);
    }
    p.p9c = txt(&t);
    four_after(a, p, ph, false)
}

/// `1 402.555.1234`.
fn country_dotted(a: &mut A, p: &mut Parts, _ph: &Ph) -> Option<bool> {
    let t = a.last();
    if !(t.ws < 3 && t.ty == 2 && d1_3(&t.text)) {
        return Some(false);
    }
    p.a4 = txt(&t);
    let t = a.push()?;
    if t.ws < 3 && t.ty == 2 && t.text().contains(&b'.') {
        let [g0, g1, g2, g3] = split_dotted(&t.text, cc_of(p));
        if g0.is_empty() && !g1.is_empty() && !g2.is_empty() && !g3.is_empty() {
            p.p98 = Some(g1);
            p.p9c = Some(g2);
            p.a8 = Some(g3);
            return Some(true);
        }
    }
    p.a4 = None;
    Some(false)
}

/// `1.402.555.1234`, `402.555.1234`, `555.1234` near a phone word; in mode `g` up to four groups of
/// one to four digits.
fn dotted(a: &mut A, p: &mut Parts, ph: &Ph) -> Option<bool> {
    let t = a.last();
    if t.ws < 3 && t.ty == 2 && t.text().contains(&b'.') {
        let [g0, g1, g2, g3] = split_dotted(&t.text, 999);
        if !g0.is_empty() && !g1.is_empty() && !g2.is_empty() && !g3.is_empty() {
            p.a4 = Some(g0);
            p.p98 = Some(g1);
            p.p9c = Some(g2);
            p.a8 = Some(g3);
            return Some(true);
        }
        let partial = !ph.plus && g0.is_empty() && !g2.is_empty() && !g3.is_empty() && (ph.mode == b'g' || context(a, ph.i11 - 1));
        if partial {
            if !g1.is_empty() {
                p.p98 = Some(g1);
            }
            p.p9c = Some(g2);
            p.a8 = Some(g3);
            return Some(true);
        }
    }
    let t = a.last();
    if ph.mode == b'g' && t.ty == 2 && 0 < t.len && t.len < 5 && num::all_digits(&t.text) {
        let mut t = t;
        let mut k = 0;
        loop {
            let slot = match k {
                0 => &mut p.a4,
                1 => &mut p.p98,
                2 => &mut p.p9c,
                3 => &mut p.a8,
                _ => return Some(false),
            };
            *slot = txt(&t);
            k += 1;
            t = a.push()?;
            if t.ty != 2 || t.len < 1 || t.len > 4 || !num::all_digits(&t.text) {
                return Some(true);
            }
        }
    }
    Some(false)
}

pub fn phone(cx: &Cx, st: &mut State, cur: &mut usize, _t: &Tok) -> i32 {
    phone_mode(cx, st, cur, 0)
}

pub fn phone_mode(cx: &Cx, st: &mut State, cur: &mut usize, mode: u8) -> i32 {
    if mode == b'g' {
        st.ahead.reset();
        if super::rx::push(cx, st, cur).is_none() {
            return 5;
        }
    } else if mode != 0 && mode != b'B' {
        return 5;
    }
    let mut sv = save(st);
    if mode == b'g' {
        sv.tn = 0;
    }
    let mut a = A { cx, st, cur };
    match read(&mut a, mode, sv) {
        Some(r) => r,
        None => a.fail(sv),
    }
}

fn read(a: &mut A, mode: u8, sv: Save) -> Option<i32> {
    let bc = sv.tn;
    let not_mine = |a: &mut A| {
        if a.n() != bc {
            a.back(bc);
        }
        5
    };
    let first = a.last();
    let start = first.start;
    let mut label = -1;
    if is_p(&first, b'(') {
        let t = a.push()?;
        if t.ws > 2 || (t.ty != 3 && t.ty != 1) || {
            label = LABELS.find(&t.text, true);
            label < 0
        } {
            a.pop();
        } else {
            let t = a.push()?;
            if t.ws <= 2 && is_p(&t, b')') {
                a.push()?;
            }
        }
    } else if first.ty == 3 || first.ty == 1 {
        label = LABELS.find(&first.text, true);
        if label >= 0 {
            let t = a.push()?;
            if t.ws < 3 && (is_p(&t, b':') || is_p(&t, b'-')) {
                a.push()?;
            }
        }
    }
    let plus = is_p(&a.last(), b'+');
    if plus {
        a.push()?;
    }
    let ph = Ph { mode, plus, i11: a.n() };
    let mut p = Parts::default();
    let atts: [Att; 6] = [country, area_first, bracketed, local, country_dotted, dotted];
    let mut matched = false;
    for (k, att) in atts.iter().enumerate() {
        if k > 0 && a.n() != ph.i11 {
            p.clear();
            a.back(ph.i11);
        }
        matched = att(a, &mut p, &ph)?;
        if matched {
            break;
        }
    }
    if !matched {
        if a.n() != ph.i11 {
            a.back(ph.i11);
        }
        return Some(not_mine(a));
    }
    let t = a.push()?;
    'suffix: {
        if t.ws >= 3 {
            a.seek(-1);
            break 'suffix;
        }
        if t.ty == 1 && ext_word(&t.text) {
            let x = a.push()?;
            if x.ws == 0 && x.len < 3 && x.text[0] == b'.' {
                let x = a.push()?;
                if x.ws < 3 && x.ty == 2 && x.len < 5 && num::all_digits(&x.text) {
                    p.ext = txt(&x);
                } else {
                    a.seek(-3);
                }
                break 'suffix;
            }
            if x.ws > 2 || x.ty != 2 || x.len > 4 || !num::all_digits(&x.text) {
                a.seek(-2);
            } else {
                p.ext = txt(&x);
            }
            break 'suffix;
        }
        if t.ws < 3 && one_of(&t, s(0x1006f4c0)) {
            let x = a.push()?;
            let c = x.text[0];
            let last8 = p.a8.as_deref().and_then(|v| v.last().copied()).unwrap_or(0);
            if x.ws < 3 && x.ty == 2 && x.len == 1 && b'0' < c && c <= b'9' && (last8 as i8) < (c as i8) {
                p.to = txt(&x);
            } else {
                a.seek(-2);
            }
            break 'suffix;
        }
        a.seek(-1);
    }
    let ws = |c: u8| matches!(c, b' ' | b'\t' | b'\n' | b'\r');
    let append = |a: &mut A| a.st.done.append_all(&a.st.ahead, 0x13) == 1;
    let ok = if mode == 0 && p.ext.is_none() && p.to.is_none() && !ws(a.nextc()) {
        let x = a.push()?;
        if !((x.ty == 3 || x.ty == 7) && one_of(&x, s(0x1006f4b0))) {
            if x.ty != 9 && x.ty != 8 {
                a.pop();
                return Some(not_mine(a));
            }
            a.pop();
        }
        append(a) && emit(a, label, plus, &p, start)
    } else if mode == b'g' {
        let mut x = a.push()?;
        if x.ty == 8 && x.text[0] == 0 {
            x = a.push()?;
        }
        if x.ty != 5 && x.ty != 9 {
            return Some(not_mine(a));
        }
        append(a) && emit(a, label, plus, &p, start)
    } else if mode != b'B' {
        append(a) && emit(a, label, plus, &p, start)
    } else {
        emit(a, label, plus, &p, start)
    };
    if ok {
        return Some(1);
    }
    restore(a.st, a.cur, sv, 5);
    Some(not_mine(a))
}

/// `FUN_100423a0`: the label ("the telephone number is"), "plus", the groups with `,` between them,
/// "extension N" and "to N", all spanning the whole number.
fn emit(a: &mut A, label: i32, plus: bool, p: &Parts, s0: i32) -> bool {
    let e = a.last().end;
    emit_wa(&mut a.st.wa, label, plus, p, s0, e) == 1
}

fn emit_wa(wa: &mut Wa, label: i32, plus: bool, p: &Parts, s0: i32, e: i32) -> i32 {
    let mut r = 1;
    if label >= 0 {
        let f = s(0x1006f7dc);
        let k = f.windows(2).position(|w| w == b"%s").unwrap_or(f.len());
        let text = [&f[..k], LABELS.field(label, 0x10), &f[(k + 2).min(f.len())..]].concat();
        r = wa.emit_words(&text, s0, e, b'S', b'D');
        if r != 1 {
            return r;
        }
    }
    if plus {
        r = wa.emit(s(0x1006f52c), s0, e, b'S', b'D');
        if r != 1 {
            return r;
        }
    }
    let groups = [&p.a4, &p.p98, &p.p9c, &p.a8];
    for (i, g) in groups.iter().enumerate() {
        let Some(g) = g else { continue };
        if i > 0 && groups[i - 1].is_some() {
            r = wa.emit(s(0x10066b98), s0, e, b'S', b'0');
            if r != 1 {
                return r;
            }
        }
        r = wa.emit_words(&spell_group(g), s0, e, b'D', b'D');
        if r != 1 {
            return r;
        }
    }
    if let Some(x) = &p.ext {
        if p.a8.is_some() {
            r = wa.emit(s(0x10066b98), s0, e, b'S', b'0');
            if r != 1 {
                return r;
            }
        }
        r = wa.emit(s(0x1006f7d0), s0, e, b'A', b'D');
        if r == 1 {
            r = wa.emit_words(&spell_group(x), s0, e, b'D', b'D');
            if r != 1 {
                return r;
            }
        }
    }
    if let Some(x) = &p.to {
        r = wa.emit(s(0x10065380), s0, e, b'S', b'D');
        if r == 1 {
            r = wa.emit_words(&spell_group(x), s0, e, b'D', b'D');
        }
    }
    r
}
