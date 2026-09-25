//! `FUN_10040ef0`: numbers in running text: ordinals ("21st"), decades
//! ("1990s", "'80s"), feet/inches (`6'`, `5"`), scores ("3-2"), codes read in
//! pairs, three-digit phone prefixes, Roman numerals I..XX, and the plain
//! cardinal fallback. Works on the shared token as `punct_runs` left it.

use super::cstr::*;
use super::numwords::*;
use super::phrases::{phone_context, score_context};
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::window::prev_two;
use super::Fe;

fn tok_at(fe: &Fe, t: &[u8], p: usize, off: i32, skip: i32, mode: i32) -> Tok {
    let mut x = Tok::default();
    tokenize(&mut x, t, p + skip as usize, off + skip, mode, fe.ud.as_ref());
    x
}

fn is_dec_suffix(w: &[u8], insensitive: bool) -> bool {
    let c = |a: &[u8], va: u32| if insensitive { stricmp(a, s(va)) } else { strcmp(a, s(va)) };
    c(w, 0x10077770) == 0 || c(w, 0x100778f0) == 0 || c(w, 0x100778ec) == 0
}

/// `M.class > 2 || ((M.class == 1 || M.class == 2) && M.ws > 0)`: the decade
/// suffix is not glued to more text.
fn decade_end(m: &Tok) -> bool {
    m.class > 2 || ((m.class == 1 || m.class == 2) && m.ws > 0)
}

/// Condition shared by feet/inches: what follows the mark ends the number.
/// The first test is the token's length (end of text), not its class: read
/// as `class == 0` it never fired, and `"6'"` at the end of a line lost its
/// "feet" (inferred, confirmed by 3 corpus lines with no regressions).
fn unit_end(m: &Tok) -> bool {
    let sep = |m: &Tok| m.text[0] != 0 && strchr(s(0x100802c0), m.text[0]);
    let punct_class = matches!(m.class, 8 | 3 | 7);
    m.len == 0 || (m.class > 0 && ((punct_class && sep(m) && m.ws == 0) || (m.class > 0 && (!punct_class || !sep(m)) && m.ws > 0)))
}

fn emit(wa: &mut Wa, buf: &[u8], st: i32, en: i32) -> bool {
    wa.emit_split(buf, st, en, 0x44, 0x44, 0x1c)
}

/// Decade readings: `'s` with a year reader, `'er`/`'ers` with a cardinal.
fn decade(buf: &mut [u8], num: &[u8], suffix_len: i32) {
    if suffix_len == 1 {
        if year_value(num) < 0 {
            number(buf, num, 1);
        } else {
            year(buf, num);
        }
        strcat(buf, s(0x100773a0));
    } else {
        number(buf, num, 1);
        strcat(buf, s(if suffix_len == 2 { 0x1007789c } else { 0x10077894 }));
    }
}

pub fn numword(fe: &Fe, wa: &mut Wa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let mut buf = new_buf();
    if tok.len == 0 {
        return -1;
    }
    let (a, b) = prev_two(t, p, off, 0);

    if tok.class == 2 {
        if ordinal_numeral(&tok.text) {
            ordinal_numeral_words(&mut buf, &tok.text);
            if !emit(wa, &buf, tok.start, tok.end) {
                return 0;
            }
            return tok.consumed;
        }
        let tt = tok_at(fe, t, p, off, 0, 2);
        let mut u4 = tt.consumed;
        if !well_formed(&tt.text) {
            if count_char(&tt.text, b'.') < 2 {
                let v = atoi(&tt.text);
                digits(&mut buf, &tt.text, 0, (v == 0) as i16);
            } else {
                dotted(&mut buf, &tt.text, 0, 0);
            }
            if !emit(wa, &buf, tt.start, tt.end) {
                return 0;
            }
            if plural(&tt.text) {
                let n = wa.n() as i32;
                wa.set_i16(Wa::rec(n - 1) + 0x1e, 1);
            }
            return tt.consumed;
        }
        let nx = tok_at(fe, t, p, off, u4, 0);
        let i8_ = nx.consumed;

        if nx.ws == 0 && is_dec_suffix(&nx.text, false) {
            u4 += i8_;
            let m = tok_at(fe, t, p, off, u4, 0);
            if decade_end(&m) {
                decade(&mut buf, &tt.text, nx.len);
                if !emit(wa, &buf, tt.start, nx.end) {
                    return 0;
                }
                return u4;
            }
            return fallback(wa, &tt, &mut buf);
        }

        #[derive(Clone, Copy, PartialEq)]
        enum L {
            L152a,
            L1531,
            L1782,
            L179a,
            L158c,
            L1778,
            L317,
        }
        let mut st;
        if (b.class == 3 || b.class == 7) && b.len == 1 && b.text[0] == b'\'' {
            st = L::L152a;
        } else {
            if nx.text[0] == b'\'' && nx.len == 1 && nx.ws == 0 {
                u4 += i8_;
                let m = tok_at(fe, t, p, off, u4, 0);
                let i8b = m.consumed;
                if m.ws == 0 && is_dec_suffix(&m.text, true) {
                    u4 += i8b;
                    let x = tok_at(fe, t, p, off, u4, 0);
                    if x.class > 2 || ((x.class == 1 || x.class == 2) && x.ws > 0) {
                        decade(&mut buf, &tt.text, m.len);
                        if !emit(wa, &buf, tt.start, m.end) {
                            return 0;
                        }
                        return u4;
                    }
                } else if (b.len == 0 || tt.ws > 0) && unit_end(&m) {
                    number(&mut buf, &tt.text, 1);
                    if !emit(wa, &buf, tt.start, nx.end) {
                        return 0;
                    }
                    let w = s(if plural(&tt.text) { 0x10080664 } else { 0x1008065c });
                    if !wa.emit(w, tt.start, nx.end, 0x44, 0x44, 0x1c) {
                        return 0;
                    }
                    return u4;
                }
                return fallback(wa, &tt, &mut buf);
            }
            st = if b.len != 0 { if b.len > 0 { L::L152a } else { L::L1531 } } else { L::L158c };
        }
        loop {
            match st {
                L::L152a => {
                    st = if tt.ws > 0 { L::L158c } else { L::L1531 };
                }
                L::L1531 => {
                    if a.len < 1 || !well_formed(&a.text) || b.text[0] != b'\'' || b.len != 1 {
                        st = L::L1778;
                        continue;
                    }
                    st = if b.ws == 0 && tt.ws < 2 { L::L158c } else { L::L1782 };
                }
                L::L1782 => {
                    st = if b.class == 1 && tt.ws > 0 { L::L179a } else { L::L317 };
                }
                L::L179a => {
                    let colon_dash = (nx.class == 3 || nx.class == 7) && nx.len == 1 && (nx.text[0] == b':' || nx.text[0] == b'-');
                    if colon_dash && nx.ws == 0 {
                        let i8c = u4 + i8_;
                        let m = tok_at(fe, t, p, off, i8c, 0);
                        if m.class == 2 && super::sbd::all_digit(&m.text) && nx.ws == m.ws {
                            let u = i8c + m.consumed;
                            if score_context(fe, t, p, off, b'B', u) {
                                number(&mut buf, &tt.text, 1);
                                if !emit(wa, &buf, tt.start, m.end) {
                                    return 0;
                                }
                                if score_context(fe, t, p, off, b'B', u) && !wa.emit(s(0x10077380), tt.start, m.end, 0x44, 0x44, 0x1c) {
                                    return 0;
                                }
                                number(&mut buf, &m.text, 1);
                                if !emit(wa, &buf, tt.start, m.end) {
                                    return 0;
                                }
                                return u;
                            }
                        }
                        return fallback(wa, &tt, &mut buf);
                    }
                    st = L::L317;
                }
                L::L158c => {
                    if nx.text[0] == b'"' && nx.len == 1 && nx.ws == 0 && !quote_before(t, p, off) {
                        number(&mut buf, &tt.text, 1);
                        let m = tok_at(fe, t, p, off, u4, 0);
                        if unit_end(&m) {
                            u4 += i8_;
                            if !emit(wa, &buf, tt.start, nx.end) {
                                return 0;
                            }
                            let w = s(if plural(&tt.text) { 0x10080654 } else { 0x1008064c });
                            if !wa.emit(w, tt.start, nx.end, 0x44, 0x44, 0x1c) {
                                return 0;
                            }
                        } else if !emit(wa, &buf, tt.start, tt.end) {
                            return 0;
                        }
                        return u4;
                    }
                    st = L::L1778;
                }
                L::L1778 => {
                    st = if b.len == 0 {
                        L::L179a
                    } else if b.len > 0 {
                        L::L1782
                    } else {
                        L::L317
                    };
                }
                L::L317 => break,
            }
        }

        // line 317
        let b6 = |wa: &mut Wa, buf: &mut Buf| -> i32 {
            code(buf, &tt.text);
            if !emit(wa, buf, tt.start, tt.end) {
                return 0;
            }
            u4
        };
        if nx.class == 1 && cls(nx.text[0]) & 0x80 != 0 && nx.ws == 0 {
            return b6(wa, &mut buf);
        }
        if tt.ws == 0 {
            let a19b1 = |wa: &mut Wa, buf: &mut Buf| -> Option<i32> {
                if b.class == 1 {
                    return Some(b6(wa, buf));
                }
                None
            };
            if a.len < 1 {
                if b.len > 0 {
                    if let Some(r) = a19b1(wa, &mut buf) {
                        return r;
                    }
                }
            } else if b.len > 0 {
                if a.class != 1 || (b.class != 3 && b.class != 7) || b.len != 1 {
                    if let Some(r) = a19b1(wa, &mut buf) {
                        return r;
                    }
                } else if b.text[0] == b'-' && b.ws == 0 {
                    return b6(wa, &mut buf);
                } else if let Some(r) = a19b1(wa, &mut buf) {
                    return r;
                }
            }
        }
        if tt.len == 3 {
            if tt.ws > 1 || b.len < 1 || b.class != 1 {
                return fallback(wa, &tt, &mut buf);
            }
            let mut first = Tok::default();
            if phone_context(fe, t, p, off, b'B', u4, Some(&mut first)) {
                digits(&mut buf, &tt.text, 0, 0);
                if !emit(wa, &buf, tt.start, tt.end) {
                    return 0;
                }
                return u4;
            }
        }
        if tt.len == 5 && tt.ws < 2 && b.len > 0 && b.class == 1 {
            let n = wa.n() as i32;
            if n > 0 && wa.ri32(n - 1, 4) == b.end && wa.i16(Wa::rec(n - 1) + 0x1e) == 0x1f {
                pairs(&mut buf, &tt.text);
                if !emit(wa, &buf, tt.start, tt.end) {
                    return 0;
                }
                return u4;
            }
        }
        return fallback(wa, &tt, &mut buf);
    }

    if tok.class != 1 {
        return -1;
    }
    let v = roman(&tok.text);
    if v < 0 {
        return -1;
    }
    let ok_prev = |b: &Tok| {
        b.len >= 1
            && (stricmp(&b.text, s(0x10080638)) == 0
                || stricmp(&b.text, s(0x1008062c)) == 0
                || stricmp(&b.text, s(0x10080624)) == 0
                || stricmp(&b.text, s(0x10080618)) == 0)
    };
    if v == 1 || v == 5 || v == 10 {
        if a.len < 1 {
            if !ok_prev(&b) {
                return -1;
            }
        } else {
            if b.len < 1 {
                return -1;
            }
            if (stricmp(&a.text, s(0x10080644)) != 0 || stricmp(&b.text, s(0x10080640)) != 0) && !ok_prev(&b) {
                return -1;
            }
        }
    } else if v == 4 && b.len > 0 && stricmp(&b.text, s(0x1007736c)) == 0 {
        return -1;
    }
    number(&mut buf, format!("{}", v).as_bytes(), 0);
    if !emit(wa, &buf, tok.start, tok.end) {
        return 0;
    }
    tok.consumed
}

/// `LAB_10041b09`: plain cardinal (years allowed).
fn fallback(wa: &mut Wa, tt: &Tok, buf: &mut Buf) -> i32 {
    number(buf, &tt.text, 0);
    if !emit(wa, buf, tt.start, tt.end) {
        return 0;
    }
    if plural(&tt.text) {
        let n = wa.n() as i32;
        wa.set_i16(Wa::rec(n - 1) + 0x1e, 1);
    }
    tt.consumed
}
