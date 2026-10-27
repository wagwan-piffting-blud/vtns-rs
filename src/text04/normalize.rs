//! `fe_NormalizeTokens` (0x1004d720): one sentence of processed text into the WA.
//!
//! Each round tries the user dictionary, then reads one token into the cleared look-ahead buffer and
//! offers it to the recognizers in the engine's order. A recognizer returns 1 (consumed what it read),
//! 2/3/4/-1 (stop the sentence with that status) or anything else (not mine). `.?!;` and `...` first
//! go through the sentence-boundary decision.

use super::sbd;
use super::stream::{token_at, Ahead, Done};
use super::tokenize::Tok;
use super::wa::Wa;
use super::Fe;

#[derive(Clone, Default)]
pub struct State {
    pub wa: Wa,
    pub done: Done,
    pub ahead: Ahead,
    pub q: super::rx::Queue,
}

/// What a recognizer sees: the text, the cursor (after the current token) and the sentence base.
pub struct Cx<'a> {
    pub fe: &'a Fe,
    pub t: &'a [u8],
    pub base: usize,
}

pub type Norm = fn(&Cx, &mut State, &mut usize, &Tok) -> i32;

fn stop(r: i32) -> bool {
    matches!(r, 2 | 3 | 4 | -1)
}

/// The chain after the VTML dispatcher, in engine order.
fn chain() -> Vec<Norm> {
    use super::norm_num as n;
    vec![
        super::norm_sym::symbols_cp,
        super::norm_cur::currency,
        super::norm_url::url,
        super::norm_time::time,
        super::norm_date::date,
        super::norm_phone::phone,
        super::norm_digits::digits,
        super::norm_addr::address,
        n::ordinal,
        super::norm_math::mathsym,
        super::norm_units::units,
        n::numword,
        super::norm_sym::symbols,
        super::norm_abbr::abbrev,
        super::norm_alnum::alnum_code,
    ]
}

/// `FUN_1004ec30`: a user-dictionary entry at the cursor. 5 when none matches.
fn userdict(cx: &Cx, st: &mut State, cur: &mut usize) -> i32 {
    let Some(ud) = cx.fe.ud.as_ref() else { return 5 };
    let p0 = st.done.toks.len();
    let n0 = st.wa.n();
    let mut m = Tok::default();
    let idx = ud.match_at(cx.t, cur, cx.base, &mut m, 0);
    if idx < 0 {
        return 5;
    }
    let r = st.done.append(&m, 0x19);
    if r != 1 {
        return r;
    }
    let (kind, target) = ud.info[idx as usize].clone();
    let restore = |st: &mut State| {
        st.done.toks.truncate(p0);
        st.wa.set_n(n0);
        if n0 > 0 {
            let e = st.wa.ri32(n0 - 1, 4);
            st.wa.set_consumed(e);
        }
    };
    if kind != b'A' {
        let r = st.wa.emit_phones(&m.text, &target, m.start, m.end, b'U', b'Y');
        if r != 1 {
            restore(st);
        }
        return r;
    }
    let mut buf = target.clone();
    buf.extend_from_slice(&[0u8; 8]);
    let mut q = 0usize;
    let mut r = 1;
    loop {
        let mut at = Tok::default();
        super::alias::tokenize_alias(&mut at, &buf, &mut q, 0, 1);
        if at.ty == 9 {
            return r;
        }
        let mut c19 = b'D';
        if at.text[0] == b'<' && at.len >= 1 && at.text[at.len as usize - 1] == b'>' {
            let inner: Vec<u8> = at.text[1..].to_vec();
            at.set_text(&inner);
            at.text[(at.len - 2).max(0) as usize] = 0;
            at.len -= 2;
            c19 = b'S';
        }
        r = st.wa.emit(&at.text, m.start, m.end, b'U', c19);
        if r != 1 {
            restore(st);
            return r;
        }
    }
}

/// `fe_NormalizeTokens` from `text` (after the leading blanks; `base` is where the sentence began).
pub fn normalize_tokens(fe: &Fe, st: &mut State, t: &[u8], text: usize) -> i32 {
    let base = text - st.wa.consumed() as usize;
    let cx = Cx { fe, t, base };
    let mut cur = text;
    let chain = chain();
    loop {
        let tok: Tok;
        'round: loop {
            loop {
                let r = userdict(&cx, st, &mut cur);
                if stop(r) {
                    return r;
                }
                if r != 1 {
                    break;
                }
            }
            st.ahead.reset();
            st.q.reset();
            let Some(k) = st.ahead.push(t, &mut cur, base, b'D', fe.ud.as_ref()) else {
                // The buffer was just cleared; a push cannot fail.
                return 4;
            };
            let tk = st.ahead.toks[k];
            if tk.ty == 8 {
                tok = tk;
                break 'round;
            }
            if tk.ty == 9 {
                let n = st.wa.n();
                if n > 0 && !st.wa.ends_punct(n - 1) {
                    st.wa.attach(n - 1, b'.', tk.end);
                }
                st.wa.set_consumed(tk.end);
                return 0;
            }
            let txt = tk.text();
            let punct = (txt.len() == 1 && b".?!;".contains(&txt[0])) || txt == b"...";
            if punct {
                let mut c2 = cur;
                let d = sbd::decide(fe, &st.done, &mut st.ahead, &tk, t, &mut c2, base);
                if d == b'P' {
                    let r = st.done.append(&tk, 0);
                    if r != 1 {
                        return r;
                    }
                    let n = st.wa.n();
                    if n > 0 {
                        let c = if tk.text[0] == b'.' || tk.text[0] == b'?' { tk.text[0] } else { b'.' };
                        st.wa.attach(n - 1, c, tk.end);
                        return 1;
                    }
                    continue 'round;
                }
                let prev = token_at(&st.done, &st.ahead, st.ahead.n - 1, -1);
                let to_chain = (tk.ws != 0 || tk.len > 2 || tk.text[0] != b'.') && prev.is_none_or(|p| p.f1 == 0);
                if !to_chain {
                    let r = st.done.append(&tk, 0);
                    if stop(r) {
                        return r;
                    }
                    continue 'round;
                }
            }
            // LAB_1004d9a9: the recognizers.
            for nf in &chain {
                let r = nf(&cx, st, &mut cur, &tk);
                if r == 1 {
                    continue 'round;
                }
                if stop(r) {
                    return r;
                }
            }
            if st.wa.n() > 0x62 {
                return 1;
            }
            st.done.append(&tk, 0);
            word(&mut st.wa, &tk);
        }
        let n = st.wa.n();
        if n > 0 && !st.wa.ends_punct(n - 1) {
            st.wa.attach(n - 1, if tok.text[0] == b'?' { b'?' } else { b'.' }, tok.end);
        }
        st.wa.set_consumed(tok.end);
        if st.wa.n() != 0 {
            return 1;
        }
    }
}

/// `FUN_10041840`: the fallback for a token no recognizer took. Symbols (type 3) and `.?!;`/`...`
/// attach to the previous word when they follow it closely; everything else is a record.
pub fn word(wa: &mut Wa, tok: &Tok) -> i32 {
    let txt = tok.text();
    if txt.is_empty() {
        wa.set_consumed(tok.end);
        return 1;
    }
    let attach = tok.ty == 3 || (txt.len() == 1 && b".?!;".contains(&txt[0])) || txt == b"...";
    if !attach {
        if wa.n() > 99 {
            return 4;
        }
        let c18 = match tok.ty {
            1 => b'A',
            2 => b'D',
            3 | 4 | 5 | 7 | 8 => b'S',
            _ => b'0',
        };
        let n = wa.n();
        let r = Wa::rec(n);
        wa.b[r + 0x18] = c18;
        wa.b[r + 0x19] = b'D';
        wa.set_ri32(n, 0, tok.start);
        wa.set_ri32(n, 4, tok.end);
        wa.set_ri32(n, 8, tok.len);
        wa.b[r + 0x1a] = 0xff;
        wa.b[r + 0x1e..r + 0x1e + txt.len()].copy_from_slice(txt);
        wa.b[r + 0x1e + txt.len()] = 0;
        wa.set_consumed(tok.end);
        wa.set_n(n + 1);
        return 1;
    }
    if tok.len > 1 || tok.start > wa.consumed() + 2 {
        return 1;
    }
    let c = if txt[0] == b';' || txt[0] == b'!' { b',' } else { txt[0] };
    let n = wa.n();
    wa.attach(n - 1, c, tok.end);
    1
}

/// `fe_AnalyzeSentence` up to the linguistic stage: reset (`FUN_1004d650`, the leading blanks count as
/// consumed), then normalize. Returns the normalizer's input offset and status.
pub fn analyze(fe: &Fe, st: &mut State, t: &[u8], cursor: usize) -> (usize, i32) {
    st.wa.reset();
    st.done = Done::default();
    st.ahead.reset();
    let mut p = cursor;
    while matches!(super::mem::at(t, p as isize), b' ' | b'\t' | b'\n' | b'\r') {
        p += 1;
        st.wa.set_consumed(st.wa.consumed() + 1);
    }
    let r = normalize_tokens(fe, st, t, p);
    (p, r)
}
