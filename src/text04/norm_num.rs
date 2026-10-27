//! `norm_numword` (0x10039b30): a number token: decimals, digit strings, `1990's`/`80s`, year ranges
//! (`1990 to 1995`, `1990-95`), house numbers after a word (`Route 66`, `221B`), `N:M` scores, dates in
//! a day list, plain cardinals.

use super::normalize::{Cx, State};
use super::num;
use super::rx::*;
use super::tokenize::Tok;
use super::mem::*;

fn plural_suffix(t: &Tok) -> bool {
    [&b"s"[..], b"ers", b"er"].iter().any(|x| stricmp(&t.text, x) == 0)
}

/// `1990's` / `1990s` / `80er`: the number's reading plus the suffix.
fn plural_reading(first: &Tok, suf: &Tok) -> (Vec<u8>, i32) {
    let (mut out, cls);
    if suf.len == 1 {
        let y = num::year_value(&first.text);
        if y < 0 {
            out = num::cardinal(&first.text, 1);
            cls = 0x10;
        } else {
            out = num::year(y);
            cls = 8;
        }
        out.extend_from_slice(b"'s");
    } else {
        out = num::cardinal(&first.text, 1);
        out.extend_from_slice(if suf.len == 2 { b"'er" } else { b"'ers" });
        cls = 0x10;
    }
    (out, cls)
}

pub fn numword(cx: &Cx, st: &mut State, cur: &mut usize, _t: &Tok) -> i32 {
    let first = cur_tok(st);
    if first.ty != 2 {
        return 5;
    }
    let sv = save(st);
    let start = first.start;
    let mut q_kept = true;
    let mut ok = false;
    let mut cls = 0;
    let mut out: Vec<u8> = Vec::new();
    let r;
    if num::is_decimal(&first.text) {
        let prev = super::stream::token_at(&st.done, &st.ahead, st.ahead.n - 1, -1);
        let prev2 = super::stream::token_at(&st.done, &st.ahead, st.ahead.n - 1, -2);
        if let Some(p) = prev {
            if first.ws == 0 && (p.ty == 1 || (is_p(&p, b'-') && p.ws == 0 && prev2.is_some_and(|q| q.ty == 1))) {
                out = num::house(&first.text);
                cls = 0x18;
            }
        }
        if out.is_empty() {
            out = num::cardinal(&first.text, 0);
            cls = 0xf;
        }
        r = st.wa.emit_words(&out, first.start, first.end, b'D', b'D');
        return finish(st, cur, sv, r, cls);
    }
    if !num::is_number(&first.text) {
        let mut o = Vec::new();
        num::spell(&mut o, &first.text, 0, 0);
        r = st.wa.emit_words(&o, first.start, first.end, b'D', b'D');
        return finish(st, cur, sv, r, 0x10);
    }
    let Some(mut nx) = push(cx, st, cur) else { return restore(st, cur, sv, 4) };
    'a016: {
        if nx.ws != 0 {
            break 'a016;
        }
        if !plural_suffix(&nx) {
            if nx.ws == 0 && is_p(&nx, b'\'') {
                let Some(n2) = push(cx, st, cur) else { return restore(st, cur, sv, 4) };
                if n2.ws == 0 && plural_suffix(&n2) {
                    let (o, c) = plural_reading(&first, &n2);
                    cls = c;
                    if st.wa.emit_words(&o, start, n2.end, b'D', b'D') == 1 {
                        return finish(st, cur, sv, 1, cls);
                    }
                    nx = n2;
                    break 'a016;
                }
                nx = pop(st, cur);
            }
            break 'a016;
        }
        let (o, c) = plural_reading(&first, &nx);
        cls = c;
        if st.wa.emit_words(&o, start, nx.end, b'D', b'D') == 1 {
            return finish(st, cur, sv, 1, cls);
        }
    }
    // LAB_1003a016
    let mut p9 = nx;
    if nx.ws < 2 && (stricmp(&nx.text, b"to") == 0 || is_p(&nx, b'-')) {
        let Some(n3) = push(cx, st, cur) else { return restore(st, cur, sv, 4) };
        p9 = n3;
        let mut range = false;
        if n3.ws < 2 && num::is_number(&n3.text) {
            st.q.reset();
            let a = num::year_value(&first.text);
            let b = if a >= 0 { num::year_value(&n3.text) } else { -1 };
            if a >= 0 && b >= 0 {
                st.q.add(&num::year(a), first.start, first.end, b'D', b'D', false);
                st.q.add(b"to", nx.start, nx.end, b'A', b'D', false);
                st.q.add(&num::year(b), n3.start, n3.end, b'D', b'D', false);
                cls = 8;
                if st.q.flush(&mut st.wa) == 1 {
                    return finish(st, cur, sv, 1, cls);
                }
                range = true;
            }
        }
        if !range {
            p9 = seek(st, cur, -2);
        }
        let o = num::cardinal(&p9.text, 0);
        cls = 0x10;
        if st.wa.emit_words(&o, p9.start, p9.end, b'D', b'D') == 1 {
            return finish(st, cur, sv, 1, cls);
        }
    }
    if p9.ws == 0 && p9.ty == 1 && cls_upper(p9.text[0]) {
        let f = pop(st, cur);
        cls = 0x18;
        let o = num::house(&f.text);
        if st.wa.emit_words(&o, start, f.end, b'D', b'D') == 1 {
            return finish(st, cur, sv, 1, cls);
        }
    }
    let mut last = pop(st, cur);
    let prev = super::stream::token_at(&st.done, &st.ahead, st.ahead.n - 1, -1);
    let prev2 = super::stream::token_at(&st.done, &st.ahead, st.ahead.n - 1, -2);
    let score = prev.is_some_and(|p| is_p(&p, b':') && p.ws == 0) && prev2.is_some_and(|q| q.ty == 2) && last.ws == 0;
    if !score {
        let house = last.ws == 0 && {
            let a58d = |prev: Option<Tok>| prev.is_some_and(|p| p.ty == 1);
            match (prev2, prev) {
                (None, p) => a58d(p),
                (Some(_), None) => false,
                (Some(q), Some(p)) => (q.ty == 1 && is_p(&p, b'-') && p.ws == 0) || a58d(Some(p)),
            }
        };
        if house {
            out = num::house(&last.text);
            cls = 0x18;
        } else {
            // LAB_1003a5be
            let list = prev2.is_some_and(|q| q.f1 == 8 || (q.ty == 3 && q.len == 1 && q.text[0] == b','))
                && prev.is_some_and(|p| is_p(&p, b',') || is_p(&p, b'&'))
                && last.ws < 3;
            if list && num::day_like(&last.text) {
                let prev3 = super::stream::token_at(&st.done, &st.ahead, st.ahead.n - 1, -3);
                let nx = push(cx, st, cur);
                if nx.is_none_or(|n| n.ws > 2 || !is_p(&n, b',')) {
                    if prev3.is_none_or(|p| p.ty != 2 || p.f1 != 8) {
                        out = num::cardinal(&first.text, 0);
                        cls = 0x10;
                    } else {
                        out = num::ordinal(&last.text);
                        cls = 8;
                    }
                    last = pop(st, cur);
                } else {
                    let n1 = push(cx, st, cur).unwrap_or_default();
                    let n2 = push(cx, st, cur);
                    let ok1 = n1.ws < 3 && n1.ty == 2 && num::day_like(&n1.text);
                    let ok2 = n1.ws < 3 && n1.ty == 3 && n1.len == 1 && n1.text[0] == b'&' && n2.is_some_and(|n| n.ws < 3 && n.ty == 2 && num::day_like(&n.text));
                    if ok1 || ok2 {
                        out = num::ordinal(&last.text);
                        cls = 8;
                        last = seek(st, cur, if n2.is_none() { -2 } else { -3 });
                    } else {
                        out = num::cardinal(&first.text, 0);
                        cls = 0x10;
                        last = seek(st, cur, -3);
                    }
                }
            } else {
                out = num::cardinal(&first.text, 0);
                cls = 0x10;
            }
        }
    } else {
        // `N:M`: "to M", or a time with AM/PM or a zone.
        let mut kept = false;
        let nx = push(cx, st, cur);
        match nx {
            None => {}
            Some(mut n) => {
                if n.ws > 2 {
                    pop(st, cur);
                } else if n.ty == 1 && am_pm(cx, st, cur, &mut n, false) {
                    kept = true;
                    q_kept = false;
                    if let Some(mut n2) = push(cx, st, cur) {
                        if n2.ws > 1 || n2.ty != 1 || !zone(cx, st, cur, &mut n2, b'D', false) {
                            pop(st, cur);
                        }
                    }
                } else if n.ws <= 2 && n.ty == 1 && zone(cx, st, cur, &mut n, b'I', false) {
                    kept = true;
                    q_kept = false;
                } else {
                    pop(st, cur);
                }
            }
        }
        out = if kept { Vec::new() } else { b"to".to_vec() };
        let c = num::cardinal(&first.text, 0);
        num::cat(&mut out, &c);
        let n = st.wa.n();
        if n > 0 {
            let l = st.wa.ri32(n - 1, 8);
            let o = super::wa::Wa::rec(n - 1) + 0x1e + l as usize - 1;
            if l > 0 && st.wa.b[o] == b',' {
                st.wa.b[o] = 0;
            }
        }
        cls = 0x10;
    }
    // LAB_1003a85d
    let r1 = st.wa.emit_words(&out, start, last.end, b'D', b'D');
    if r1 == 1 {
        ok = true;
    }
    if !q_kept && st.q.n() > 0 && st.q.flush(&mut st.wa) != 1 {
        let k = st.q.n();
        seek(st, cur, -k);
    }
    if ok {
        return finish(st, cur, sv, 1, cls);
    }
    restore(st, cur, sv, r1)
}

/// `norm_ordinal` (0x1003a9a0): `21st`, `3rd`, `11th`.
pub fn ordinal(_cx: &Cx, st: &mut State, cur: &mut usize, _t: &Tok) -> i32 {
    let t = cur_tok(st);
    if t.ty != 2 {
        return 5;
    }
    let sv = save(st);
    if t.len >= 3 && num::is_ordinal(&t.text) {
        let stem = &t.text()[..t.len as usize - 2];
        let out = num::ordinal(stem);
        if !out.is_empty() && st.wa.emit_words(&out, t.start, t.end, b'D', b'D') == 1 {
            let r = st.done.append_all(&st.ahead, 0x11);
            if r != 1 {
                return restore(st, cur, sv, r);
            }
            return r;
        }
    }
    restore(st, cur, sv, 5)
}

fn cls_upper(c: u8) -> bool {
    cls(c) & 0x80 != 0
}

/// `LAB_1003a8d0` / `LAB_1003a948`: on success hand every buffered token to the consumed list with
/// `cls`; otherwise roll back.
fn finish(st: &mut State, cur: &mut usize, sv: Save, r: i32, cls: i32) -> i32 {
    if r == 1 {
        let a = st.done.append_all(&st.ahead, cls);
        if a == 1 {
            return 1;
        }
        return restore(st, cur, sv, a);
    }
    restore(st, cur, sv, r)
}
