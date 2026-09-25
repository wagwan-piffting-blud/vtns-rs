//! `FUN_100544f0`: symbols and punctuation read aloud or used as prosodic marks
//! (commas, quotes, brackets, comparison operators, `~ @ & # \\ © ®`, EUC-KR
//! symbol pairs). The tokenizer runs in mode 0x12 throughout.
//!
//! `©` (single-byte 0xa9, or the EUC-KR two-byte pair `0xa8 0xcf`) followed by
//! a bare year (`FUN_10039410`, ported as `numwords::year_value`) reads
//! "copyright <year>" (`FUN_10039d50`, `numwords::year`); otherwise it falls
//! through to the plain `©`/EUC-KR handling below.

use super::cstr::*;
use super::chars::letter_phones_sum;
use super::numwords::{new_buf, year, year_value};
use super::tabs::{ptab, rec30};
use super::tokenize::{tokenize, Tok};
use super::wa::Wa;
use super::window::{prev_one, prev_two};
use super::Fe;

const M: i32 = 0x12;

fn next_tok(fe: &Fe, t: &[u8], p: usize, off: i32, skip: i32, mode: i32) -> (Tok, i32) {
    let mut n = Tok::default();
    let r = tokenize(&mut n, t, p + skip as usize, off + skip, mode, fe.ud.as_ref());
    (n, r)
}

fn set_f2c(wa: &mut Wa, n8: i32, v: i32) {
    wa.set_ri32(n8 - 1, 0x2c, v);
}

pub fn symbols(fe: &Fe, wa: &mut Wa, tok: &mut Tok, t: &[u8], p: usize, off: i32, mark: i16) -> i32 {
    let n8 = wa.n() as i32;
    let mut out = [0u8; 64];
    tokenize(tok, t, p, off, M, fe.ud.as_ref());
    let class = tok.class;
    if class == 4 || class == 5 {
        return tok.consumed;
    }
    let len = tok.len;
    if len == 0 || (class != 3 && class != 6) {
        return -1;
    }
    let c0 = tok.text[0];

    if len != 1 {
        let emit_out = |wa: &mut Wa, o: &[u8], tok: &Tok| -> i32 {
            if !wa.emit_split(o, tok.start, tok.end, 0x53, 0x44, 0x13) {
                return 0;
            }
            tok.consumed
        };
        if len == 2 {
            let b1 = tok.text[1];
            let tabs = [rec30(0x10095c48, 0x1009b088), rec30(0x10095e58, 0x1009b090), rec30(0x10095f18, 0x1009b094), rec30(0x10096878, 0x1009b098)];
            for tb in tabs.iter() {
                let i = tb.find(&tok.text, false);
                if i >= 0 {
                    let o = tb.field(i, 0x10);
                    return emit_out(wa, o, tok);
                }
            }
            if (c0 == 0xa8 || c0 == 0xa9) && 0xcc < b1 && b1 < 0xe7 {
                if c0 == 0xa8 && b1 == 0xcf {
                    // Unlike the single-byte © below, "copyright" goes through FUN_100451e0 here.
                    let (nx, r) = next_tok(fe, t, p, off, tok.consumed, M);
                    if nx.class == 2 && super::sbd::all_digit(&nx.text) && nx.ws < 3 && year_value(&nx.text) >= 0 {
                        if !wa.emit_split(s(0x100804fc), tok.start, tok.end, 0x53, 0x44, 0x13) {
                            return 0;
                        }
                        let mut yb = new_buf();
                        year(&mut yb, &nx.text);
                        if !wa.emit_split(&yb, nx.start, nx.end, 0x53, 0x44, 0x13) {
                            return 0;
                        }
                        return tok.consumed + r;
                    }
                }
                // Any other pair in this row: the second byte + 0x74 as a one-letter word.
                out[0] = b1.wrapping_add(0x74);
                out[1] = 0;
                if !wa.emit(&out, tok.start, tok.end, 0x53, 0x53, 0x13) {
                    return 0;
                }
                return tok.consumed;
            }
            if c0 == b'+' {
                return emit_out(wa, s(0x10081520), tok);
            }
            if c0 != b'-' {
                return tok.consumed;
            }
            return prev_mark(wa, n8, tok);
        }
        if len < 3 {
            return -1;
        }
        if c0 != b'+' {
            if c0 == b'.' && n8 > 0 {
                set_f2c(wa, n8, 1);
                return tok.consumed;
            }
            return tok.consumed;
        }
        return emit_out(wa, s(0x10081510), tok);
    }

    // len == 1
    let quote = |c: u8| c != 0 && strchr(s(0x100813c4), c);
    let (mut a, mut b) = (Tok::default(), Tok::default());
    if class == 3 && quote(c0) {
        let ab = prev_two(t, p, off, M);
        a = ab.0;
        b = ab.1;
    }

    if class == 3 {
        if c0 == b',' {
            if n8 > 0 {
                let (nx, r) = next_tok(fe, t, p, off, tok.consumed, M);
                if r != 0 && nx.class != 9 {
                    if (stricmp(&nx.text, s(0x10078ea0)) != 0 && stricmp(&nx.text, s(0x1007902c)) != 0) || nx.ws > 1 {
                        set_f2c(wa, n8, 5);
                        return tok.consumed;
                    }
                    set_f2c(wa, n8, 0xc);
                }
            }
            return tok.consumed;
        }
        if quote(c0) && !(tok.ws < 1 && b.len != 0) {
            let mut used = tok.consumed;
            let (nx, mut lc) = next_tok(fe, t, p, off, used, M);
            if lc == 0 || nx.class == 9 {
                return tok.consumed;
            }
            let mut special: Option<&'static [u8]> = None;
            if nx.class == 3 {
                if nx.len == 1 {
                    let tb = rec30(0x10081b40, 0x1009b080);
                    let i = tb.find(&nx.text, true);
                    if i >= 0 {
                        used += lc;
                        let (nn, r2) = next_tok(fe, t, p, off, used, M);
                        lc = r2;
                        if lc == 0 || nn.class == 9 || nn.len != 1 || nn.class != 3 || c0 != nn.text[0] || nx.ws != nn.ws {
                            return tok.consumed;
                        }
                        special = Some(tb.field(i, 0x10));
                    }
                }
            }
            if special.is_none() && nx.class != 3 && nx.class == 6 && nx.len == 1 {
                let tb = rec30(0x1009ac30, 0x1009b120);
                let i = tb.find(&nx.text, true);
                if i >= 0 {
                    used += lc;
                    let (nn, r2) = next_tok(fe, t, p, off, used, M);
                    lc = r2;
                    if lc == 0 || nn.class == 9 || nn.len < 1 || nn.class != 3 || c0 != nn.text[0] || nx.len != 1 {
                        return tok.consumed;
                    }
                    special = Some(tb.field(i, 0x10));
                }
            }
            if let Some(o) = special {
                strcpy(&mut out, o);
                if !wa.emit_split(&out, nx.start, nx.end, 0x53, 0x44, 0x13) {
                    return 0;
                }
                return lc + used;
            }
            if nx.ws > 1 || ptab(0x100780fc, 0x10078248).find(&nx.text, true) < 0 {
                if c0 == b'"' && nx.ws < 2 && ptab(0x1007844c, 0x1007847c).find(&nx.text, true) >= 0 {
                    if n8 > 0 {
                        set_f2c(wa, n8, 1);
                        return tok.consumed;
                    }
                } else if c0 == b'\'' && nx.class == 1 && nx.ws == 0 && ptab(0x1009ade0, 0x1009b124).find(&nx.text, true) >= 0 {
                    out[0] = b'\'';
                    strcpy(&mut out[1..], &nx.text);
                    if !wa.emit_split(&out, tok.start, nx.end, 0x41, 0x44, 0x15) {
                        return 0;
                    }
                    return lc + used;
                }
                return tok.consumed;
            }
            return prev_mark(wa, n8, tok);
        }
    }

    // LAB_10054a2f
    if class == 3 && c0 == b'\'' && tok.ws == 0 {
        let apostrophe_merge = b.len > 0 && b.class == 1;
        let via_dot = !(a.len < 1 || n8 < 1 || wa.ri32(n8 - 1, 4) != a.end || (wa.ri32(n8 - 1, 0x28) != 0x16 && wa.ri32(n8 - 1, 0x28) != 0x18) || b.text[0] != b'.' || b.len != 1) && b.ws == 0;
        if apostrophe_merge || via_dot {
            return possessive(fe, wa, tok, t, p, off, n8, &b, mark);
        }
        let skip_e24 = !(a.len < 1 || n8 < 1 || wa.ri32(n8 - 1, 4) != a.end || (wa.ri32(n8 - 1, 0x28) != 0x16 && wa.ri32(n8 - 1, 0x28) != 0x18) || b.text[0] != b'.' || b.len != 1);
        if (skip_e24 || b.len > 0) && (b.class == 1 || b.class == 2) {
            return tok.consumed;
        }
        let (nx, r) = next_tok(fe, t, p, off, tok.consumed, M);
        if r != 0 && nx.class != 9 {
            if nx.class == 1 && nx.ws == 0 && ptab(0x1009ade0, 0x1009b124).find(&nx.text, true) >= 0 {
                out[0] = b'\'';
                strcpy(&mut out[1..], &nx.text);
                if !wa.emit_split(&out, tok.start, nx.end, 0x41, 0x44, 0x15) {
                    return 0;
                }
                return tok.consumed + r;
            }
            if nx.ws < 2 && ptab(0x100780fc, 0x10078248).find(&nx.text, true) >= 0 && n8 > 0 {
                set_f2c(wa, n8, 1);
                return tok.consumed;
            }
        }
        return tok.consumed;
    }

    if class == 3 && quote(c0) {
        let (nx, r) = next_tok(fe, t, p, off, tok.consumed, M);
        if r == 0 || nx.class == 9 {
            return tok.consumed;
        }
        if nx.ws > 1 || ptab(0x100780fc, 0x10078248).find(&nx.text, true) < 0 {
            if c0 == b'"' && nx.ws < 2 && ptab(0x1007844c, 0x1007847c).find(&nx.text, true) >= 0 && n8 > 0 {
                set_f2c(wa, n8, 1);
                return tok.consumed;
            }
            return tok.consumed;
        }
        return prev_mark(wa, n8, tok);
    }

    if class == 3 && c0 != 0 && strchr(s(0x10081554), c0) {
        return compare(fe, wa, tok, t, p, off, n8);
    }

    if class != 3 {
        return tok.consumed;
    }
    other_punct(fe, wa, tok, t, p, off, n8)
}

/// `LAB_100557ce`: mark a prosodic boundary after the previous word.
fn prev_mark(wa: &mut Wa, n8: i32, tok: &Tok) -> i32 {
    if n8 > 0 {
        set_f2c(wa, n8, 1);
    }
    tok.consumed
}

/// `LAB_10054ace`: `'` after a word: possessive `word'` / `word's` style
/// merges into the previous record.
#[allow(clippy::too_many_arguments)]
fn possessive(fe: &Fe, wa: &mut Wa, tok: &Tok, t: &[u8], p: usize, off: i32, n8: i32, b: &Tok, mark: i16) -> i32 {
    let mut out = [0u8; 96];
    let mut u5 = tok.consumed;
    let (nx, lc) = next_tok(fe, t, p, off, u5, 0x18);
    if n8 < 1 {
        return tok.consumed;
    }
    let prev = n8 - 1;
    let merge_next = nx.len >= 1 && nx.class == 1 && nx.ws == 0;
    if !merge_next {
        if b.len < 3 {
            return tok.consumed;
        }
        let in_tail = stricmp(&b.text[(b.len - 2) as usize..], s(0x10077688)) == 0;
        let hard_end = matches!(nx.class, 3 | 9 | 8 | 7) || nx.ws >= 1;
        if !in_tail || !hard_end {
            let soft_end = matches!(nx.class, 9 | 8 | 7) || nx.ws > 0;
            if b.len > 2 && b.class == 1 && lower(b.text[(b.len - 1) as usize]) == 0x73 && soft_end && wa.ri32(prev, 8) + 2 < 0x1d {
                let pt = wa.text(prev).to_vec();
                strcpy(&mut out, &pt);
                strcat(&mut out, b"'");
                wa.set_ri32(prev, 4, tok.end);
                wa.set_n(n8 as i16 - 1);
                return reemit(wa, prev, &out, u5, nx.len, false);
            }
            return tok.consumed;
        }
    }
    if wa.ri32(prev, 8) + 1 + nx.len < 0x1d && letter_phones_sum(&nx.text) + letter_phones_sum(wa.text(prev)) < 0x40 {
        if mark > 0 {
            let n = wa.n();
            wa.set_n(n + 1);
            return u5;
        }
        let pt = wa.text(prev).to_vec();
        strcpy(&mut out, &pt);
        if nx.class == 1 && nx.ws == 0 {
            strcat(&mut out, b"'");
            strcat(&mut out, &nx.text);
            wa.set_ri32(prev, 4, nx.end);
            u5 += lc;
        } else {
            strcat(&mut out, b"'");
            wa.set_ri32(prev, 4, tok.end);
        }
        wa.set_n(n8 as i16 - 1);
        return reemit(wa, prev, &out, u5, nx.len, true);
    }
    tok.consumed
}

/// Re-emit the merged previous record, keeping its tags (class 0x19 records
/// also carry a sub-token span in +0x14/+0x18 that is extended).
fn reemit(wa: &mut Wa, prev: i32, out: &[u8], ret: i32, nlen: i32, second_form: bool) -> i32 {
    let (a, bb, cls);
    if wa.ri32(prev, 0x28) == 0x19 {
        if wa.ri32(prev, 0x14) == 0 {
            let l = wa.ri32(prev, 8);
            wa.set_ri32(prev, 0x14, l);
            wa.set_ri32(prev, 0x18, nlen + 1);
        } else {
            let v = wa.ri32(prev, 0x18) + nlen + 1;
            wa.set_ri32(prev, 0x18, v);
        }
        let n = wa.n() as i32;
        let mut i = n;
        while i > 0 {
            if wa.ri32(i - 1, 0) == wa.ri32(i, 0) {
                let e = wa.ri32(i, 4);
                wa.set_ri32(i - 1, 4, e);
            }
            i -= 1;
        }
        cls = 0x19;
        if second_form {
            a = 0x55;
            bb = 0x59;
        } else {
            a = 0x55;
            bb = 0x59;
        }
    } else {
        cls = wa.ri32(prev, 0x28) as u32 & 0xff;
        if second_form {
            a = 0x41;
            bb = 0x44;
        } else {
            a = 0x41;
            bb = wa.u8(Wa::rec(prev) + 0x24);
        }
    }
    let (st, en) = (wa.ri32(prev, 0), wa.ri32(prev, 4));
    if !wa.emit_split(out, st, en, a, bb, cls) {
        return 0;
    }
    ret
}

/// `<` / `>` (with `=`): "less than", "greater than or equal to", or
/// prosodic marks between numbers.
fn compare(fe: &Fe, wa: &mut Wa, tok: &Tok, t: &[u8], p: usize, off: i32, n8: i32) -> i32 {
    let prev = prev_one(t, p, off, M);
    let mut u5 = tok.consumed;
    let (alt, lc0) = next_tok(fe, t, p, off, u5, M);
    if lc0 == 0 || alt.class == 9 {
        return tok.consumed;
    }
    let mut local_54 = 0;
    let (nx, mut lc) = if alt.class == 3 && alt.text[0] == b'=' && alt.len == 1 && alt.ws == 0 {
        u5 += lc0;
        let (n2, r2) = next_tok(fe, t, p, off, u5, M);
        if r2 == 0 {
            return tok.consumed;
        }
        local_54 = u5;
        if n2.class == 9 {
            return tok.consumed;
        }
        (n2, r2)
    } else {
        (alt.clone(), lc0)
    };
    let c0 = tok.text[0];
    let sign_next = nx.len >= 1 && nx.class == 3 && nx.text[0] != 0 && strchr(s(0x10081550), nx.text[0]) && nx.len == 1 && nx.ws <= 2;
    let words: &[u8];
    let (st, en);
    if !sign_next {
        if prev.len < 1 {
            return tok.consumed;
        }
        let numeric_ctx = wa.n() >= 1 && wa.ri32(n8 - 1, 4) == prev.end && (prev.class == 2 || prev.class == 1) && tok.ws <= 1 && nx.len >= 1 && nx.class == 2 && nx.ws <= 1;
        if !numeric_ctx {
            if wa.n() > 0 && wa.ri32(n8 - 1, 4) == prev.end {
                if prev.class == 1 {
                    if prev.len != 1 {
                        return tok.consumed;
                    }
                } else if prev.class != 2 || !super::sbd::all_digit(&prev.text) {
                    return tok.consumed;
                }
                if tok.ws < 2 && nx.len > 0 && matches!(nx.class, 1 | 2 | 3) && nx.ws < 2 && n8 > 0 {
                    set_f2c(wa, n8, 1);
                    return tok.consumed;
                }
            }
            return tok.consumed;
        }
        if local_54 != 0 {
            words = if c0 == b'<' { s(0x10080438) } else { s(0x10080410) };
            st = tok.start;
            if !wa.emit_split(words, st, alt.end, 0x53, 0x44, 0x13) {
                return 0;
            }
            return local_54;
        }
        words = if c0 == b'<' { s(0x1008042c) } else { s(0x10080400) };
        st = tok.start;
        en = tok.end;
    } else {
        let (nn, r3) = next_tok(fe, t, p, off, u5 + lc, M);
        lc = r3;
        if r3 == 0 || nn.class == 9 || nn.len < 1 || nn.class != 2 || nn.ws > 2 || (prev.len != 0 && prev.class != 2 && prev.class != 1) {
            return tok.consumed;
        }
        if local_54 != 0 {
            words = if c0 == b'<' { s(0x10080438) } else { s(0x10080410) };
            if !wa.emit_split(words, tok.start, alt.end, 0x53, 0x44, 0x13) {
                return 0;
            }
            return local_54;
        }
        words = if c0 == b'<' { s(0x1008042c) } else { s(0x10080400) };
        st = tok.start;
        en = tok.end;
    }
    let _ = lc;
    if !wa.emit_split(words, st, en, 0x53, 0x44, 0x13) {
        return 0;
    }
    tok.consumed
}

/// `]`, `(`/`[`, `)`/`]`, `.NET`, `:`, `~`, `@`, `\\`, `&`, `#`, ©, ®.
fn other_punct(fe: &Fe, wa: &mut Wa, tok: &Tok, t: &[u8], p: usize, off: i32, n8: i32) -> i32 {
    let c0 = tok.text[0];
    let len = tok.len;
    let emit_word = |wa: &mut Wa, w: &[u8]| -> i32 {
        if !wa.emit(w, tok.start, tok.end, 0x53, 0x44, 0x13) {
            return 0;
        }
        tok.consumed
    };
    let mark_after = |wa: &mut Wa, nx: &Tok, tok: &Tok| -> i32 {
        if nx.class == 1 || nx.class == 2 || nx.class == 3 {
            return prev_mark(wa, n8, tok);
        }
        tok.consumed
    };
    if c0 == b']' && len == 1 {
        let prev = prev_one(t, p, off, M);
        if prev.len < 1 || ((prev.class != 2 || !super::sbd::all_digit(&prev.text)) && (prev.class != 1 || prev.len != 1)) {
            return tok.consumed;
        }
        let (nx, r) = next_tok(fe, t, p, off, tok.consumed, M);
        if r == 0 || nx.class == 9 || nx.ws > 1 {
            return tok.consumed;
        }
        return mark_after(wa, &nx, tok);
    }
    if (c0 == b'(' || c0 == b'[') && len == 1 {
        let u5 = tok.consumed;
        let d = close_bracket(&t[(p + u5 as usize).min(t.len())..]);
        if d == 0 {
            return u5;
        }
        // WA +8 is the parenthesis length limit (VT_SetParenthesisCharNumber_ENG).
        let limit = wa.i32(8);
        if limit != 0 && limit <= d {
            if n8 < 1 {
                return u5;
            }
            set_f2c(wa, n8, 1);
            return u5;
        }
        return u5 + d;
    }
    if (c0 == b')' || c0 == b']') && len == 1 {
        if tok.ws < 2 {
            let prev = prev_one(t, p, off, M);
            if prev.len < 1 || ((prev.class != 2 || !super::sbd::all_digit(&prev.text)) && prev.class != 1) {
                return tok.consumed;
            }
        }
        let (nx, r) = next_tok(fe, t, p, off, tok.consumed, M);
        if r == 0 || nx.class == 9 {
            return tok.consumed;
        }
        return mark_after(wa, &nx, tok);
    }
    if c0 == b'.' && len == 1 {
        let (nx, r) = next_tok(fe, t, p, off, tok.consumed, M);
        if r == 0 || nx.class == 9 || nx.ws != 0 || stricmp(&nx.text, s(0x1008154c)) != 0 {
            return tok.consumed;
        }
        if !wa.emit_split(s(0x10081544), tok.start, nx.end, 0x53, 0x44, 0x13) {
            return 0;
        }
        return tok.consumed + r;
    }
    if c0 == b':' && len == 1 && tok.ws < 2 {
        let (a, b) = super::window::prev_two(t, p, off, M);
        let a_colon = a.len >= 1 && a.class == 3 && a.len == 1 && a.text[0] == b':';
        if !a_colon && b.len > 0 {
            if b.class == 2 && super::sbd::all_digit(&b.text) {
                let (nx, lc) = next_tok(fe, t, p, off, tok.consumed, M);
                if lc != 0 && nx.class != 9 {
                    if nx.class == 2 && super::sbd::all_digit(&nx.text) && tok.ws == nx.ws {
                        let (nn, _) = next_tok(fe, t, p, off, tok.consumed + lc, M);
                        if nn.len > 0 && nn.class == 3 && nn.len == 1 {
                            return tok.consumed;
                        }
                    } else if n8 > 0 {
                        set_f2c(wa, n8, 1);
                        return tok.consumed;
                    }
                }
            } else if b.len > 0 && b.class == 1 && n8 > 0 {
                set_f2c(wa, n8, 1);
                return tok.consumed;
            }
        }
        return tok.consumed;
    }
    if c0 == b'~' && len == 1 && tok.ws < 2 {
        let prev = prev_one(t, p, off, M);
        if prev.len == 0 && prev.class == 0 {
            return tok.consumed;
        }
        let (nx, r) = next_tok(fe, t, p, off, tok.consumed, M);
        if r == 0 || nx.class == 9 || prev.len < 1 {
            return tok.consumed;
        }
        if prev.class == 1 {
            if prev.len != 1 {
                return tok.consumed;
            }
        } else if prev.class != 2 {
            return tok.consumed;
        }
        if nx.len < 1 {
            return tok.consumed;
        }
        if nx.class == 1 {
            if nx.len != 1 {
                return tok.consumed;
            }
        } else if nx.class != 2 {
            return tok.consumed;
        }
        if tok.ws != nx.ws {
            return tok.consumed;
        }
        return emit_word(wa, s(0x10077380));
    }
    if c0 == b'@' && len == 1 {
        return emit_word(wa, s(0x10077694));
    }
    if c0 == b'\\' && len == 1 {
        if !wa.emit_split(s(0x1008123c), tok.start, tok.end, 0x53, 0x44, 0x13) {
            return 0;
        }
        return tok.consumed;
    }
    if c0 == b'&' && len == 1 {
        return emit_word(wa, s(0x10077400));
    }
    if c0 == b'#' && len == 1 {
        let (nx, r) = next_tok(fe, t, p, off, tok.consumed, M);
        if r == 0 || nx.class == 9 || nx.class != 2 || !super::sbd::all_digit(&nx.text) || nx.ws > 2 {
            return tok.consumed;
        }
        return emit_word(wa, s(0x100775c4));
    }
    if len == 1 && c0 == 0xa9 {
        let (nx, r) = next_tok(fe, t, p, off, tok.consumed, 3);
        if r == 0 || nx.class == 9 || nx.class != 2 || !super::sbd::all_digit(&nx.text) || nx.ws > 2 || year_value(&nx.text) < 0 {
            return tok.consumed;
        }
        // Unreachable live: symbols_cp (position 3) always takes a lone 0xa9 first.
        if !wa.emit(s(0x100804fc), tok.start, tok.end, 0x53, 0x44, 0x13) {
            return 0;
        }
        let mut yb = new_buf();
        year(&mut yb, &nx.text);
        if !wa.emit_split(&yb, nx.start, nx.end, 0x53, 0x44, 0x13) {
            return 0;
        }
        return tok.consumed + r;
    }
    if len != 1 || c0 != 0xae {
        return tok.consumed;
    }
    if !wa.emit_split(s(0x1008152c), tok.start, tok.end, 0x53, 0x44, 0x13) {
        return 0;
    }
    tok.consumed
}

/// `FUN_10062ea0`: bytes to the matching `)`/`]` (0 if none).
fn close_bracket(x: &[u8]) -> i32 {
    let mut i = 0usize;
    let mut sp = 0;
    loop {
        let c = at(x, i as isize);
        if c == 0 || sp > 2999 {
            return 0;
        }
        if c == b')' || c == b']' {
            return i as i32;
        }
        if at(x, 0) == b' ' {
            sp += 1;
        }
        i += 1;
    }
}
