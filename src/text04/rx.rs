//! Plumbing shared by the recognizers: the saved counts they roll back to, the pending-word queue
//! (`ctx[3]`: 20 entries of 0x10c bytes, flushed into the WA as a whole) and the small readers several
//! of them call.

use super::mem::*;
use super::normalize::{Cx, State};
use super::num;
use super::tokenize::Tok;

/// Counts at entry: look-ahead buffer, consumed list, WA.
#[derive(Clone, Copy)]
pub struct Save {
    pub tn: i32,
    pub pn: usize,
    pub wn: i32,
}

pub fn save(st: &State) -> Save {
    Save { tn: st.ahead.n, pn: st.done.toks.len(), wn: st.wa.n() }
}

/// The recognizers' common exit on failure: everything back to the entry counts.
pub fn restore(st: &mut State, cur: &mut usize, s: Save, r: i32) -> i32 {
    st.ahead.n = s.tn;
    if let Some(p) = st.ahead.pos[s.tn as usize] {
        *cur = p;
    }
    st.done.toks.truncate(s.pn);
    st.wa.set_n(s.wn);
    if s.wn > 0 {
        let e = st.wa.ri32(s.wn - 1, 4);
        st.wa.set_consumed(e);
    }
    r
}

/// The current token (last in the buffer).
pub fn cur_tok(st: &State) -> Tok {
    st.ahead.tok(st.ahead.n as isize - 1)
}

pub fn push(cx: &Cx, st: &mut State, cur: &mut usize) -> Option<Tok> {
    st.ahead.push(cx.t, cur, cx.base, b'D', cx.fe.ud.as_ref()).map(|k| st.ahead.toks[k])
}

/// `FUN_1004d410`: drop the last token; the new last one.
pub fn pop(st: &mut State, cur: &mut usize) -> Tok {
    let i = st.ahead.pop(cur).unwrap_or(-1);
    st.ahead.tok(i)
}

/// `FUN_1004d440`.
pub fn seek(st: &mut State, cur: &mut usize, d: i32) -> Tok {
    let i = st.ahead.seek(cur, d);
    st.ahead.tok(i)
}

pub fn is_p(t: &Tok, c: u8) -> bool {
    (t.ty == 3 || t.ty == 7) && t.len == 1 && t.text[0] == c
}

/// A recognizer's hold on the look-ahead buffer and the cursor. The engine's current-token pointer is
/// always the buffer's last token, so it is read back with `last`.
pub struct Buf<'a, 'b> {
    pub cx: &'a Cx<'b>,
    pub st: &'a mut State,
    pub cur: &'a mut usize,
}

impl Buf<'_, '_> {
    pub fn n(&self) -> i32 {
        self.st.ahead.n
    }

    pub fn last(&self) -> Tok {
        cur_tok(self.st)
    }

    pub fn tok(&self, i: i32) -> Tok {
        self.st.ahead.tok(i as isize)
    }

    pub fn push(&mut self) -> Option<Tok> {
        push(self.cx, self.st, self.cur)
    }

    /// `fe_TokenPush` with the alias tokenizer.
    pub fn push_r(&mut self) -> Option<Tok> {
        let k = self.st.ahead.push(self.cx.t, self.cur, self.cx.base, b'R', self.cx.fe.ud.as_ref())?;
        Some(self.st.ahead.toks[k])
    }

    pub fn pop(&mut self) -> Tok {
        pop(self.st, self.cur)
    }

    pub fn seek(&mut self, d: i32) -> Tok {
        seek(self.st, self.cur, d)
    }

    /// Buffer count back to `k`, the cursor to where token `k` began.
    pub fn back(&mut self, k: i32) {
        self.st.ahead.n = k;
        if let Some(p) = self.st.ahead.pos[k.max(0) as usize] {
            *self.cur = p;
        }
    }

    pub fn nextc(&self) -> u8 {
        at(self.cx.t, *self.cur as isize)
    }

    pub fn fail(&mut self, sv: Save) -> i32 {
        restore(self.st, self.cur, sv, 4)
    }

    /// Run `f` on a cleared queue, keeping the real one.
    pub fn scratch<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let q = std::mem::take(&mut self.st.q);
        let r = f(self);
        self.st.q = q;
        r
    }
}

#[derive(Clone, Debug, Default)]
pub struct QEnt {
    pub start: i32,
    pub end: i32,
    pub c18: u8,
    pub c19: u8,
    pub text: Vec<u8>,
}

/// The engine keeps a count over 20 fixed slots: lowering the count leaves the slots' contents behind,
/// so raising it again brings old entries back.
#[derive(Clone)]
pub struct Queue {
    pub ents: Vec<QEnt>,
    pub n: usize,
}

impl Default for Queue {
    fn default() -> Self {
        Queue { ents: vec![QEnt::default(); 20], n: 0 }
    }
}

impl Queue {
    /// Count and slots cleared.
    pub fn reset(&mut self) {
        *self = Queue::default();
    }

    pub fn n(&self) -> i32 {
        self.n as i32
    }

    /// `*q = k`: the count alone.
    pub fn set_n(&mut self, k: i32) {
        self.n = k.clamp(0, 20) as usize;
    }

    /// `FUN_10042640`; `title` capitalizes the words (`FUN_1004f860`).
    pub fn add(&mut self, text: &[u8], start: i32, end: i32, c18: u8, c19: u8, title: bool) -> bool {
        if self.n >= 20 {
            return false;
        }
        let mut t = cs(text).to_vec();
        if title {
            t.push(0);
            num::title_case(&mut t);
            t.pop();
        }
        self.ents[self.n] = QEnt { start, end, c18, c19, text: t };
        self.n += 1;
        true
    }

    /// `FUN_10042220`: every entry through `fe_EmitWords`.
    pub fn flush(&self, wa: &mut super::wa::Wa) -> i32 {
        for e in &self.ents[..self.n] {
            let r = wa.emit_words(&e.text, e.start, e.end, e.c18, e.c19);
            if r != 1 {
                return r;
            }
        }
        1
    }
}

/// `FUN_1004b560`: `a.m`/`am`/`p.m`/`pm` (`0x10072688`) as "AM"/"PM", then a `.` and a `,`. `tok` is
/// the caller's current token, advanced over what was read.
pub fn am_pm(cx: &Cx, st: &mut State, cur: &mut usize, tok: &mut Tok, keep_dot: bool) -> bool {
    const TAB: Rec30 = rec30(0x10072688, 0x10084848);
    st.q.reset();
    let k = TAB.find(&tok.text, true);
    if k < 0 {
        return false;
    }
    st.q.add(TAB.field(k, 0x10), tok.start, tok.end, b'A', b'S', false);
    let Some(t2) = push(cx, st, cur) else { return false };
    *tok = t2;
    let (s, e);
    if t2.ws == 0 && t2.len < 3 && t2.text[0] == b'.' {
        let Some(t3) = push(cx, st, cur) else {
            *tok = seek(st, cur, -1);
            return false;
        };
        *tok = t3;
        if !(t3.ws <= 2 && is_p(&t3, b',')) {
            *tok = seek(st, cur, if keep_dot { -1 } else { -2 });
            return true;
        }
        s = t3.start;
        e = t3.end;
    } else {
        if !(t2.ws <= 2 && is_p(&t2, b',')) {
            *tok = pop(st, cur);
            return true;
        }
        s = t2.start;
        e = t2.end;
    }
    st.q.add(b",", s, e, b'S', b'0', false);
    true
}

/// `FUN_1004b6e0`: a time zone abbreviation (`0x10072748`, case-sensitive) as its name, then a `.` and
/// a `,`. `mode` `I` clears the queue first.
pub fn zone(cx: &Cx, st: &mut State, cur: &mut usize, tok: &mut Tok, mode: u8, keep_dot: bool) -> bool {
    const TAB: Rec30 = rec30(0x10072748, 0x1008484c);
    if mode == b'I' {
        st.q.reset();
    }
    let k = TAB.find(&tok.text, false);
    if k < 0 {
        return false;
    }
    st.q.add(TAB.field(k, 0x10), tok.start, tok.end, b'A', b'D', false);
    let Some(t2) = push(cx, st, cur) else { return false };
    *tok = t2;
    if t2.ws != 0 || t2.len > 2 || t2.text[0] != b'.' {
        *tok = pop(st, cur);
        return true;
    }
    let Some(t3) = push(cx, st, cur) else {
        *tok = seek(st, cur, -1);
        return false;
    };
    *tok = t3;
    if t3.ws < 3 && is_p(&t3, b',') {
        st.q.add(b",", t3.start, t3.end, b'S', b'0', false);
        return true;
    }
    *tok = seek(st, cur, if keep_dot { -1 } else { -2 });
    true
}
