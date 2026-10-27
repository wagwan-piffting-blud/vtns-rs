//! The symbol recognizers: `norm_symbols_cp` (0x10049420) for raw high bytes (`©`, `²`, `×`, and
//! `é`/`ñ` inside words), and `norm_symbols` (0x100484d0) for ASCII symbols and EUC-KR glyphs
//! (`@`, `&`, `#5`, `5:`, `1~3`, `< 5`, quotes, brackets, superscripts, circled letters). Most
//! symbols read as nothing; the rest go to the pending queue.

use super::mem::*;
use super::norm_math::wa_spanning;
use super::norm_units::{dbcs_symbol, is_sym, POWERS};
use super::normalize::{Cx, State};
use super::num;
use super::rx::*;
use super::stream::token_at;
use super::tokenize::{letter_phones, Tok};

const PUNCT_NAMES: Rec30 = rec30(0x10072088, 0x10084844);
const WH: Ptab = ptab(0x10065cd4, 0x10065e20);
const SUBS: Rec30 = rec30(0x100736e0, 0x1008487c);
const CIRCLED: Rec30 = rec30(0x100737a0, 0x10084880);
const SIGNS: Rec30 = rec30(0x10074100, 0x10084884);

fn one_of(t: &Tok, set: &[u8]) -> bool {
    t.len == 1 && t.text[0] != 0 && set.contains(&t.text[0])
}

fn blank(c: u8) -> bool {
    matches!(c, 0 | b' ' | b'\t' | b'\n' | b'\r')
}

/// The buffer index of the token `rel` before the last, or the consumed-list index (`Err`).
fn slot(st: &State, rel: i32) -> Option<Result<usize, usize>> {
    let k = st.ahead.n - 1 + rel;
    if k >= 0 {
        return Some(Ok(k as usize));
    }
    let j = st.done.n() + k;
    if j >= 0 {
        Some(Err(j as usize))
    } else {
        None
    }
}

/// `FUN_10049420`.
pub fn symbols_cp(cx: &Cx, st: &mut State, cur: &mut usize, _t: &Tok) -> i32 {
    let t = cur_tok(st);
    if t.ty != 6 {
        return 5;
    }
    let sv = save(st);
    let c = t.text[0];
    let r = match c {
        0xa4 => st.wa.emit(b"currency", t.start, t.end, b'S', b'D'),
        0xa9 => st.wa.emit(b"Copywrite", t.start, t.end, b'S', b'D'),
        0xb2 => st.wa.emit_words(b"superscript two", t.start, t.end, b'S', b'D'),
        0xb3 => st.wa.emit_words(b"superscript three", t.start, t.end, b'S', b'D'),
        0xb5 => st.wa.emit(b"micro", t.start, t.end, b'S', b'D'),
        0xb6 => st.wa.emit(b"Paragraph", t.start, t.end, b'S', b'D'),
        0xb9 => st.wa.emit_words(b"superscript one", t.start, t.end, b'S', b'D'),
        0xd7 => st.wa.emit(b"times", t.start, t.end, b'S', b'D'),
        0xc9 | 0xe9 | 0xd1 | 0xf1 => {
            let tail = |c: u8| -> &'static [u8] {
                if c == 0xc9 || c == 0xe9 {
                    b"ie"
                } else {
                    b"ny"
                }
            };
            let mut word;
            let start;
            let prev = token_at(&st.done, &st.ahead, st.ahead.n - 1, -1);
            let k = prev.map(|p| wa_spanning(&st.wa, &p)).unwrap_or(-1);
            match prev {
                Some(p) if t.ws == 0 && p.ty == 1 && p.f1 == 0 && k >= 0 && p.len + 3 <= 0x1d => {
                    word = [p.text(), tail(c)].concat();
                    start = p.start;
                    st.wa.set_n(k);
                    match slot(st, -1) {
                        Some(Ok(i)) => st.ahead.toks[i].f1 = 0x1b,
                        Some(Err(j)) => st.done.toks[j].f1 = 0x1b,
                        None => {}
                    }
                }
                _ => {
                    word = tail(c).to_vec();
                    start = t.start;
                }
            }
            let mut end = t.end;
            loop {
                if blank(at(cx.t, *cur as isize)) {
                    break;
                }
                let Some(n) = push(cx, st, cur) else { return restore(st, cur, sv, 4) };
                if n.ty == 1 {
                    word.extend_from_slice(n.text());
                } else if matches!(n.text[0], 0xc9 | 0xe9 | 0xd1 | 0xf1) {
                    word.extend_from_slice(tail(n.text[0]));
                } else {
                    pop(st, cur);
                    break;
                }
                end = n.end;
            }
            st.wa.emit(&word, start, end, b'A', b'A')
        }
        _ => 1,
    };
    if r == 1 {
        let a = st.done.append_all(&st.ahead, 0x1b);
        if a == 1 {
            return 1;
        }
    }
    restore(st, cur, sv, 5)
}

/// `FUN_10049840`: a `<`/`>` that pairs up with the other one (an unread tag).
fn in_tag(st: &State) -> bool {
    let t0 = st.ahead.toks[0];
    let sym = |t: &Tok, c: u8| (t.ty == 3 || t.ty == 7) && t.len == 1 && t.text[0] == c;
    let mut depth = 0;
    if sym(&t0, b'<') {
        for k in 1..st.ahead.n.max(1) as usize {
            let t = &st.ahead.toks[k];
            if sym(t, b'<') {
                depth += 1;
            } else if sym(t, b'>') {
                depth -= 1;
            }
        }
        return depth == -1;
    }
    if !sym(&t0, b'>') {
        return false;
    }
    for t in &st.done.toks {
        if sym(t, b'<') {
            depth += 1;
        } else if sym(t, b'>') {
            depth -= 1;
        }
    }
    depth == 1
}

/// `FUN_100509a0`: the spelled-out phone count of a word's letters.
fn spelled_phones(t: &[u8]) -> i32 {
    cs(t).iter().map(|&c| letter_phones(c)).sum()
}

/// `FUN_1000cb40`.
fn all_alpha(t: &[u8]) -> bool {
    let t = cs(t);
    !t.is_empty() && t.iter().all(|&c| is_alpha(c))
}

enum Out {
    /// `LAB_10048d0c`: the buffer to the consumed list, the queue to the WA.
    Flush,
    /// `LAB_1004937b`: drop the last token, then flush.
    PopFlush,
    /// Not mine (`LAB_100493b0`).
    No,
    Ret(i32),
}

/// `FUN_100484d0`.
pub fn symbols(cx: &Cx, st: &mut State, cur: &mut usize, _t: &Tok) -> i32 {
    let t = cur_tok(st);
    if t.ty != 3 {
        return 5;
    }
    let sv = save(st);
    let p = token_at(&st.done, &st.ahead, st.ahead.n - 1, -1);
    let out = sym_body(cx, st, cur, t, p, sv);
    match out {
        Out::Ret(r) => r,
        Out::No => {
            if st.ahead.n != sv.tn {
                st.ahead.n = sv.tn;
                if let Some(q) = st.ahead.pos[sv.tn as usize] {
                    *cur = q;
                }
            }
            5
        }
        Out::Flush | Out::PopFlush => {
            if let Out::PopFlush = out {
                pop(st, cur);
            }
            let mut r = st.done.append_all(&st.ahead, 10);
            if r == 1 {
                r = st.q.flush(&mut st.wa);
                if r == 1 {
                    return 1;
                }
            }
            restore(st, cur, sv, r)
        }
    }
}

fn sym_body(cx: &Cx, st: &mut State, cur: &mut usize, t: Tok, p: Option<Tok>, sv: Save) -> Out {
    let fail = |st: &mut State, cur: &mut usize| Out::Ret(restore(st, cur, sv, 4));
    let c0 = t.text[0];
    if one_of(&t, b"`'\"") && (t.ws > 0 || sv.pn == 0) {
        let Some(x) = push(cx, st, cur) else { return fail(st, cur) };
        let k = if x.ws == 0 { PUNCT_NAMES.find(&x.text, false) } else { -1 };
        if k >= 0 {
            let Some(y) = push(cx, st, cur) else { return fail(st, cur) };
            if y.ws == 0 && (y.ty == 3 || y.ty == 7) && y.len == 1 && y.text[0] == c0 {
                st.q.add(PUNCT_NAMES.field(k, 0x10), x.start, x.end, b'S', b'D', false);
            } else {
                seek(st, cur, -2);
            }
            return Out::Flush;
        }
        let n = st.wa.n();
        if n > 0 && x.ws < 2 && find_ptab(WH, &x.text) >= 0 {
            let l = st.wa.ri32(n - 1, 8);
            let last = st.wa.b[super::wa::Wa::rec(n - 1) + 0x1e + (l - 1).max(0) as usize];
            if is_alpha(last) {
                st.wa.attach(n - 1, b',', -1);
                pop(st, cur);
                return Out::Flush;
            }
        }
        return Out::PopFlush;
    }
    let pty = |k: &[i32]| p.is_some_and(|p| k.contains(&p.ty));
    if is_sym(&t, b'@') {
        st.q.add(b"at", t.start, t.end, b'S', b'D', false);
        return Out::Flush;
    }
    if is_sym(&t, b'&') {
        st.q.add(b"and", t.start, t.end, b'S', b'D', false);
        return Out::Flush;
    }
    if is_sym(&t, b'#') {
        let Some(y) = push(cx, st, cur) else { return fail(st, cur) };
        if y.ws > 2 || y.ty != 2 {
            return Out::PopFlush;
        }
        st.q.add(b"number", t.start, t.end, b'S', b'D', false);
        return Out::PopFlush;
    }
    if is_sym(&t, b':') {
        if pty(&[1, 2]) {
            st.q.add(b",", t.start, t.end, b'S', b'0', false);
            return Out::Flush;
        }
        return Out::No;
    }
    if t.ws < 3 && t.len > 1 && c0 == b'-' {
        if pty(&[1, 2]) {
            let Some(y) = push(cx, st, cur) else { return fail(st, cur) };
            let comma = y.ws < 3 && (y.ty == 1 || y.ty == 2);
            if comma {
                st.q.add(b",", t.start, t.end, b'S', b'0', false);
            }
            pop(st, cur);
            if comma {
                return Out::Flush;
            }
        }
        return Out::No;
    }
    if t.ws < 2 && t.len == 1 && c0 == b'~' {
        let Some(y) = push(cx, st, cur) else { return fail(st, cur) };
        let small = |q: &Tok| q.ty == 2 || (q.ty == 1 && q.len == 1);
        if p.is_some_and(|p| small(&p)) && small(&y) {
            st.q.add(b"to", t.start, t.end, b'S', b'D', false);
            pop(st, cur);
            return Out::Flush;
        }
        return Out::PopFlush;
    }
    if !(t.ws < 2 && t.len != 1) && one_of(&t, b"<>") {
        let Some(mut y) = push(cx, st, cur) else { return fail(st, cur) };
        let mut sign = false;
        let mut said = false;
        if y.ws < 3 {
            if one_of(&y, b"$+-") {
                sign = true;
                let Some(y2) = push(cx, st, cur) else { return fail(st, cur) };
                y = y2;
            }
            if !(y.ws > 2 || y.ty != 2 || in_tag(st) || p.is_some_and(|p| p.ty != 2 && p.ty != 1)) {
                st.q.add(if c0 == b'>' { b"more than" } else { b"less than" }, t.start, t.end, b'S', b'D', false);
                said = true;
            }
        }
        if !said
            && !sign
            && t.ws < 2
            && y.ws < 2
            && matches!(y.ty, 1 | 2 | 3)
            && p.is_some_and(|p| (p.ty == 1 && p.len == 1) || (p.ty == 2 && num::all_digits(&p.text)))
        {
            st.q.add(b",", t.start, t.end, b'S', b'0', false);
        }
        pop(st, cur);
        if sign {
            pop(st, cur);
        }
        return Out::Flush;
    }
    if t.len == 2 && dbcs_symbol(&t.text) {
        let (b0, b1) = (t.text[0], t.text[1]);
        let tables = [(POWERS, 0x10u32), (SUBS, 0x10), (CIRCLED, 0x10)];
        for (tab, off) in tables {
            let k = tab.find(&t.text, false);
            if k >= 0 {
                st.q.add(tab.field(k, off), t.start, t.end, b'S', b'D', false);
                return Out::Flush;
            }
        }
        if (b0 == 0xa8 || b0 == 0xa9) && 0xcc < b1 && b1 < 0xe7 {
            if b0 == 0xa8 && b1 == 0xcf {
                let Some(y) = push(cx, st, cur) else { return fail(st, cur) };
                let yr = if y.ws < 3 && y.ty == 2 { num::year_value(&y.text) } else { -1 };
                if yr >= 0 {
                    st.q.add(b"Copyright", t.start, t.end, b'S', b'D', false);
                    st.q.add(&num::year(yr), y.start, y.end, b'D', b'D', false);
                } else {
                    let tt = pop(st, cur);
                    st.q.add(b"C", tt.start, tt.end, b'S', b'S', false);
                }
                return Out::Flush;
            }
            let letter = [b1.wrapping_add(0x74)];
            st.q.add(&letter, t.start, t.end, b'S', b'S', false);
            return Out::Flush;
        }
        let k = SIGNS.find(&t.text, false);
        if k >= 0 {
            st.q.add(SIGNS.field(k, 0x10), t.start, t.end, b'S', b'D', false);
        }
        return Out::Flush;
    }
    if t.len != 1 {
        return Out::No;
    }
    if c0 == b'\'' && t.ws == 0 && p.is_some_and(|p| p.ty == 1) {
        let p = p.unwrap();
        let goin = p.len >= 3 && stricmp(&p.text[p.len as usize - 2..], b"in") == 0 && blank(at(cx.t, *cur as isize)) && all_alpha(&p.text);
        let y = if goin {
            t
        } else {
            let Some(y) = push(cx, st, cur) else { return fail(st, cur) };
            if y.ws != 0 || y.ty != 1 {
                return Out::PopFlush;
            }
            y
        };
        let k = wa_ending(st, p.end);
        if k >= 0 && st.wa.ri32(k, 8) + 1 + y.len < 0x1d {
            let base = st.wa.text(k).to_vec();
            if spelled_phones(&base) + spelled_phones(&y.text) < 0x40 {
                let mut w = base.clone();
                w.push(b'\'');
                if y.ty == 1 {
                    w.extend_from_slice(y.text());
                }
                st.wa.set_n(k);
                let mut r = st.done.append_all(&st.ahead, 0);
                if r == 1 {
                    let (s, c18, c19) = (st.wa.ri32(k, 0), st.wa.b[super::wa::Wa::rec(k) + 0x18], st.wa.b[super::wa::Wa::rec(k) + 0x19]);
                    if p.f1 == 0x19 {
                        if st.wa.ri32(k, 0x10) == 0 {
                            let l = st.wa.ri32(k, 8);
                            st.wa.set_ri32(k, 0x10, l);
                            st.wa.set_ri32(k, 0x14, y.len + 1);
                        } else {
                            let v = st.wa.ri32(k, 0x14) + y.len + 1;
                            st.wa.set_ri32(k, 0x14, v);
                        }
                        r = st.wa.emit(&w, s, y.end, c18, c19);
                    } else {
                        r = st.wa.emit(&w, s, y.end, b'A', b'D');
                    }
                    if r == 1 {
                        return Out::Ret(1);
                    }
                }
                return Out::Ret(restore(st, cur, sv, r));
            }
        }
        return Out::PopFlush;
    }
    if one_of(&t, b")]")
        && t.ws < 2
        && p.is_some_and(|p| (p.ty == 2 && num::all_digits(&p.text)) || (p.ty == 1 && p.len == 1))
    {
        let Some(y) = push(cx, st, cur) else { return fail(st, cur) };
        if y.ws > 1 || !matches!(y.ty, 1 | 2 | 3) {
            return Out::PopFlush;
        }
        st.q.add(b",", t.start, t.end, b'S', b'0', false);
        return Out::PopFlush;
    }
    if one_of(&t, b"`^()_|\\[]{}'\"") {
        return Out::Flush;
    }
    Out::No
}

/// `FUN_10050720`.
fn wa_ending(st: &State, end: i32) -> i32 {
    if end < 0 {
        return -1;
    }
    let mut k = st.wa.n() - 1;
    while k >= 0 {
        let e = st.wa.ri32(k, 4);
        if e < end {
            return -1;
        }
        if e == end {
            return k;
        }
        k -= 1;
    }
    -1
}

/// `FUN_10049980` (insensitive).
fn find_ptab(tab: Ptab, w: &[u8]) -> i32 {
    let w = cs(w);
    if w.is_empty() {
        return -1;
    }
    let (mut lo, mut hi) = (0, tab.count() - 1);
    let mut mid = hi;
    while lo <= hi {
        mid /= 2;
        let c = stricmp(tab.get(mid), w);
        if c == 0 {
            return mid;
        }
        if c < 0 {
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
        mid = hi + lo;
    }
    -1
}
