//! `norm_digits` (0x1003ca50): fixed digit-group codes, read group by group with "dash" (or `,`)
//! between them: `123-45-6789` (3-2-4), `123456-1234567` (6-7, digit by digit) and `12-3456789` (2-7).

use super::mem::*;
use super::norm_phone::spell_group;
use super::normalize::{Cx, State};
use super::num;
use super::rx::{is_p, restore, save, Buf as A};
use super::tokenize::Tok;

fn group(t: &Tok, len: i32) -> bool {
    t.ws < 3 && t.len == len && t.ty == 2 && num::all_digits(&t.text)
}

pub fn digits(cx: &Cx, st: &mut State, cur: &mut usize, _t: &Tok) -> i32 {
    let sv = save(st);
    let mut a = A { cx, st, cur };
    match read(&mut a) {
        Some(r) => r,
        None => a.fail(sv),
    }
}

fn read(a: &mut A) -> Option<i32> {
    let sv = save(a.st);
    let n0 = a.n();
    let t0 = a.last();
    if t0.ty != 2 {
        return Some(5);
    }
    let start = t0.start;
    let not_mine = |a: &mut A| {
        if a.n() != n0 {
            a.back(n0);
        }
        5
    };
    let first = |a: &mut A| -> Option<(Tok, bool)> {
        let t = a.push()?;
        if t.ws < 3 && is_p(&t, b'-') {
            return Some((a.push()?, true));
        }
        Some((t, false))
    };
    let all = num::all_digits(&t0.text);
    let (groups, dash, digitwise): (Vec<Vec<u8>>, bool, bool);
    if t0.len == 3 && all {
        let (t, d) = first(a)?;
        if !group(&t, 2) {
            return Some(not_mine(a));
        }
        let mut t2 = a.push()?;
        if d {
            if !(t2.ws <= 2 && is_p(&t2, b'-')) {
                return Some(not_mine(a));
            }
            t2 = a.push()?;
        }
        if !group(&t2, 4) {
            return Some(not_mine(a));
        }
        groups = vec![t0.text().to_vec(), t.text().to_vec(), t2.text().to_vec()];
        dash = d;
        digitwise = false;
    } else if (t0.len == 6 || t0.len == 2) && all {
        let (t, d) = first(a)?;
        if !group(&t, 7) {
            return Some(not_mine(a));
        }
        groups = vec![t0.text().to_vec(), t.text().to_vec()];
        dash = d;
        digitwise = t0.len == 6;
    } else {
        return Some(not_mine(a));
    }
    let r = a.st.done.append_all(&a.st.ahead, 0x14);
    if r != 1 {
        a.back(n0);
        a.st.done.toks.truncate(sv.pn);
        return Some(r);
    }
    let e = a.last().end;
    let sep: &[u8] = if dash { s(0x1006f190) } else { s(0x10066b98) };
    for (i, g) in groups.iter().enumerate() {
        if i != 0 {
            let r = a.st.wa.emit(sep, start, e, b'S', b'D');
            if r != 1 {
                return Some(restore(a.st, a.cur, sv, r));
            }
        }
        let text = if digitwise {
            let mut o = Vec::new();
            num::spell(&mut o, g, 1, 0);
            o
        } else {
            spell_group(g)
        };
        let r = a.st.wa.emit_words(&text, start, e, b'A', b'D');
        if r != 1 {
            return Some(restore(a.st, a.cur, sv, r));
        }
    }
    Some(1)
}
