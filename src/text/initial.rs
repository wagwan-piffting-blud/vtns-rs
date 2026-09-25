//! `fe_InitialTextProcess` (0x1001c990): raw request text -> processed text,
//! the processed -> original offset maps (`TIB+0x47794/98`), the 8 VTML span
//! channels (`TIB+0x1223c0`, 16 bytes each: count, cursor, pos[], val[]) and
//! the mark list (`TIB+0x122448`).
//!
//! Two filter passes (`FUN_1001d250`): `fe_MapFullwidthChars` then
//! `fe_ExtractVtmlTags`. The fullwidth pass maps the 73 EUC-KR pairs of
//! `0x1007caf0` to ASCII; every pair, mapped or not, gets lead/trail offsets
//! in `map_b`/`map_c` for all its output bytes. The tag pass tries
//! the 31-entry table `0x1007e888`
//! (longest case-insensitive prefix) at every byte that isn't whitespace;
//! each handler validates the tag body against one of the 25 Spencer regexes
//! compiled at load (`0x1007eb88`), writes a marker (`FD FE` open, `A2 FE`
//! close/point, `A2 FD` level-3 break, `A2 FC FD FE` emotion) and, for the
//! content tags, the content rewritten to `<\xa1\xdd` + letter + `>`...
//! `</\xa1\xdd>` for `norm_vtml`.
//!
//! Say-as (`<vtml_sayas interpret-as="...">`): `sayas_conv` looks the name up in
//! `FUN_10048d60`'s 22-entry table and passes content, format, detail and pausetime
//! to the handler, which checks its own options. Every handler is ported: characters
//! (`FUN_1004a1c0`, detail grouping, final-tag `\xff` pause), boolean, digits,
//! number, ordinal, cardinal (separator/decimal pair), duration, date (all field
//! orders, `SAPI` detail), vxml:date (2/4/6/8-digit and `?` shapes), phone/telephone
//! (letters, parens, `+cc`), currency, fraction (mixed numbers), vxml:time/phone/
//! currency, ssml:time (`FUN_100491c0`, compact and `.` shapes), sapi:time
//! (`FUN_10045bb0` preprocessor + `FUN_1004c260`), email/web and address.
//!
//! Phoneme alphabets: x-cmu (`FUN_10046320`, with its `#`-only word recursion),
//! x-sapi (`FUN_10046070`), ipa/"" (`FUN_100466d0`), x-worldbet/x-sampa
//! (`FUN_10046910`), x-ntsampa/x-tasampa (`FUN_10046b20`, second `<..T>` block past
//! 64 phones), via `sampa_ascii`/`sampa_group` (`FUN_10047510`/`FUN_10047e30`) and
//! `stress_fixup` (`FUN_100487a0`).
//!
//! Not ported: the demo nag.

use super::cstr::*;
use super::tabs::{ptab, ptabn, rec5a, rec66, Ptab, Rec5a, Rec66};
use super::tokenize::{tokenize, Tok};

// ---------------------------------------------------------------- regex --

enum Atom {
    Ch(u8),
    Set(Vec<(u8, u8)>),
    Any,
    Group(Vec<Vec<Piece>>),
    Bol,
    Eol,
}

struct Piece {
    a: Atom,
    min: usize,
    max: usize,
}

fn parse_alt(p: &[u8], i: &mut usize) -> Vec<Vec<Piece>> {
    let mut alts = vec![parse_seq(p, i)];
    while *i < p.len() && p[*i] == b'|' {
        *i += 1;
        alts.push(parse_seq(p, i));
    }
    alts
}

fn parse_seq(p: &[u8], i: &mut usize) -> Vec<Piece> {
    let mut seq = Vec::new();
    while *i < p.len() && p[*i] != b'|' && p[*i] != b')' {
        let c = p[*i];
        *i += 1;
        let a = match c {
            b'(' => {
                let g = parse_alt(p, i);
                *i += 1;
                Atom::Group(g)
            }
            b'[' => {
                let mut set = Vec::new();
                while p[*i] != b']' {
                    let lo = p[*i];
                    if p.get(*i + 1) == Some(&b'-') && p.get(*i + 2).is_some_and(|&h| h != b']') {
                        set.push((lo, p[*i + 2]));
                        *i += 3;
                    } else {
                        set.push((lo, lo));
                        *i += 1;
                    }
                }
                *i += 1;
                Atom::Set(set)
            }
            b'.' => Atom::Any,
            b'^' => Atom::Bol,
            b'$' => Atom::Eol,
            b'\\' => {
                *i += 1;
                Atom::Ch(p[*i - 1])
            }
            _ => Atom::Ch(c),
        };
        let (min, max) = match p.get(*i) {
            Some(b'*') => (0, usize::MAX),
            Some(b'+') => (1, usize::MAX),
            Some(b'?') => (0, 1),
            _ => (1, 1),
        };
        if max != 1 || min != 1 {
            *i += 1;
        }
        seq.push(Piece { a, min, max });
    }
    seq
}

fn m_atom(a: &Atom, s: &[u8], pos: usize, k: &dyn Fn(usize) -> bool) -> bool {
    match a {
        Atom::Ch(c) => s.get(pos) == Some(c) && k(pos + 1),
        Atom::Set(set) => s.get(pos).is_some_and(|&c| set.iter().any(|&(lo, hi)| lo <= c && c <= hi)) && k(pos + 1),
        Atom::Any => pos < s.len() && k(pos + 1),
        Atom::Group(alts) => alts.iter().any(|seq| m_seq(seq, s, pos, k)),
        Atom::Bol => pos == 0 && k(pos),
        Atom::Eol => pos == s.len() && k(pos),
    }
}

fn m_rep(p: &Piece, n: usize, s: &[u8], pos: usize, k: &dyn Fn(usize) -> bool) -> bool {
    if n < p.max && m_atom(&p.a, s, pos, &|q| q != pos && m_rep(p, n + 1, s, q, k)) {
        return true;
    }
    n >= p.min && k(pos)
}

fn m_seq(seq: &[Piece], s: &[u8], pos: usize, k: &dyn Fn(usize) -> bool) -> bool {
    let Some(p) = seq.first() else { return k(pos) };
    let rest = &seq[1..];
    if p.min == 1 && p.max == 1 {
        m_atom(&p.a, s, pos, &|q| m_seq(rest, s, q, k))
    } else {
        m_rep(p, 0, s, pos, &|q| m_seq(rest, s, q, k))
    }
}

/// `FUN_10016150`/`FUN_100161c0` (Spencer `regexec`): does regex `idx` of
/// the load-time array (`0x1007eb88`) match anywhere in `body`?
fn re(idx: usize, body: &[u8]) -> bool {
    let pat = ptabn(0x1007eb88, 25).get(idx as i32);
    let mut i = 0;
    let alts = parse_alt(pat, &mut i);
    let s = cs(body);
    (0..=s.len()).any(|st| alts.iter().any(|seq| m_seq(seq, s, st, &|_| true)))
}

// ------------------------------------------------------------ strtok etc --

const WS: &[u8] = b" \x0c\n\r\t\x0b";

/// `FUN_10024b50` (`strtok_r`) over an owned buffer: next token from `*at`.
fn strtok<'a>(buf: &'a [u8], at: &mut usize, delims: &[u8]) -> Option<&'a [u8]> {
    let n = cs(buf).len();
    let mut i = *at;
    while i < n && delims.contains(&buf[i]) {
        i += 1;
    }
    if i >= n {
        *at = n;
        return None;
    }
    let st = i;
    while i < n && !delims.contains(&buf[i]) {
        i += 1;
    }
    *at = if i < n { i + 1 } else { n };
    Some(&buf[st..i])
}

/// `FUN_1002ebf0`: the value of attribute `name` in a tag body
/// (`name` `=` `"value"`, any delimiter run between).
fn attr(body: &[u8], name: &[u8], delims: &[u8]) -> Option<Vec<u8>> {
    let mut at = 0;
    let start = loop {
        let before = at;
        let t = strtok(body, &mut at, delims)?;
        if stricmp(t, name) == 0 {
            break before + (t.as_ptr() as usize - body[before..].as_ptr() as usize);
        }
    };
    let mut at2 = start;
    let t = strtok(body, &mut at2, delims)?;
    if strnicmp(t, name, name.len()) != 0 {
        return None;
    }
    // `FUN_1002ece0`'s span stops on the delimiter list's last character (`"`).
    let n = cs(body).len();
    let skip = &delims[..delims.len().saturating_sub(1)];
    let mut i = at2;
    while i < n && skip.contains(&body[i]) {
        i += 1;
    }
    if i >= n {
        return None;
    }
    let mut at3 = i;
    strtok(body, &mut at3, b"\"").map(|v| v.to_vec())
}

fn trim(w: &[u8]) -> Vec<u8> {
    let ws = |c: &u8| matches!(c, b' ' | b'\t' | b'\n' | b'\r');
    let s = cs(w);
    let a = s.iter().position(|c| !ws(c)).unwrap_or(s.len());
    let b = s.iter().rposition(|c| !ws(c)).map(|i| i + 1).unwrap_or(a);
    s[a..b.max(a)].to_vec()
}

fn tagged(letter: u8, inner: &[u8]) -> Vec<u8> {
    let mut v = b"<\xa1\xdd".to_vec();
    v.push(letter);
    v.push(b'>');
    v.extend_from_slice(inner);
    v.extend_from_slice(b"</\xa1\xdd>");
    v
}

// --------------------------------------------------------- converters ----

/// `FUN_10046320`: x-cmu phones -> `<..T>word\xff codes </..>`.
fn cmu(ph: &[u8], word: &[u8]) -> Option<Vec<u8>> {
    cmu_lim(ph, word, 0x600)
}

/// `max` is `0x600` at top level, `0x200` for the `#`-only word recursion
/// (the engine calls `FUN_10046320` back into itself with an empty word and
/// the ph text from the triggering `#` onward, splicing the sub-block in).
fn cmu_lim(ph: &[u8], word: &[u8], max: usize) -> Option<Vec<u8>> {
    let ph = trim(ph);
    if ph.is_empty() || word.len() >= max - 1 {
        return None;
    }
    let mut out = b"<\xa1\xddT>".to_vec();
    out.extend_from_slice(word);
    out.push(0xff);
    let len = ph.len();
    let (mut k, mut count, mut last) = (0usize, 0, false);
    let mut buf = [0u8; 4];
    let mut i = 0;
    while i <= len {
        let c = at(&ph, i as isize);
        if c == 0 && !last {
            break;
        }
        if !((i == 0 && ph[0] == b' ') || (i == len && c == b' ')) {
            if c == b' ' || last {
                let mut tok = buf[..k].to_vec();
                if k == 2 && b"AEIOU".contains(&tok[0]) {
                    tok.push(b'0');
                }
                let code = if tok == b"#" {
                    0
                } else {
                    let list = ptabn(0x10077c80, 69);
                    let j = super::tabs::search(0x45, true, &tok, |x| list.get(x));
                    if j < 0 {
                        return None;
                    }
                    j + 1
                };
                let skip = code == 0 && !(count != 0 && out.last() != Some(&100) && i != len);
                if !skip {
                    if out.len() >= max - 6 || count > 0x3f {
                        return None;
                    }
                    if word.is_empty() && code == 0 {
                        let local_c = out.len();
                        out.extend_from_slice(b"</\xa1\xdd>");
                        return match cmu_lim(&ph[i..], b"", 0x200) {
                            None => None,
                            Some(sub) if sub.len() + 5 + local_c > 0x1ff => None,
                            Some(sub) if sub.len() < 0xc => Some(out),
                            Some(sub) => {
                                out.extend_from_slice(&sub);
                                Some(out)
                            }
                        };
                    }
                    out.push(if code != 0 { code as u8 } else { 100 });
                    count += 1;
                    if c != 0 {
                        while matches!(at(&ph, i as isize), b' ' | b'\t' | b'\n' | b'\r') {
                            i += 1;
                        }
                        i -= 1;
                    }
                }
                k = 0;
            } else {
                if k > 2 {
                    return None;
                }
                buf[k] = c;
                k += 1;
                if at(&ph, i as isize + 1) == 0 {
                    last = true;
                }
            }
        }
        i += 1;
    }
    out.extend_from_slice(b"</\xa1\xdd>");
    Some(out)
}

/// `FUN_10046070`: x-sapi phones; `1`/`2` add stress to the previous vowel.
fn sapi(ph: &[u8], word: &[u8]) -> Option<Vec<u8>> {
    let ph = trim(ph);
    if ph.is_empty() || word.len() >= 0x600 - 1 {
        return None;
    }
    let mut out = b"<\xa1\xddT>".to_vec();
    out.extend_from_slice(word);
    out.push(0xff);
    let list = ptab(0x100808b4, 0x10080978);
    let n = list.count();
    let codes = b(0x10080980, 196);
    let len = ph.len();
    let (mut k, mut count) = (0usize, 0);
    let mut buf = [0u8; 3];
    let mut i = 0;
    while i <= len {
        let c = at(&ph, i as isize);
        if matches!(c, b' ' | b'\t' | 0) {
            let j = super::tabs::search(n, true, &buf[..k], |x| list.get(x));
            let skip = j == 1 && !(count != 0 && out.last() != Some(&100) && i != len);
            if j < 0 {
                return None;
            }
            if !skip {
                if out.len() >= 0x600 - 6 || count > 0x3f {
                    return None;
                }
                let code = codes[4 * j as usize] as i32;
                let last = *out.last().unwrap();
                if code == 0x74 || code == 0x75 {
                    if crate::ling::lts::phone_vowel(last) != 0 {
                        *out.last_mut().unwrap() = last + (code - 0x73) as u8;
                    }
                } else {
                    out.push(code as u8);
                    count += 1;
                }
                if c != 0 {
                    while matches!(at(&ph, i as isize), b' ' | b'\t' | b'\n' | b'\r') {
                        i += 1;
                    }
                    i -= 1;
                }
            }
            k = 0;
        } else {
            if k > 1 {
                return None;
            }
            buf[k] = c;
            k += 1;
        }
        i += 1;
    }
    while out.last() == Some(&100) {
        out.pop();
    }
    out.extend_from_slice(b"</\xa1\xdd>");
    Some(out)
}

/// The 3 stress-suffixed forms (`1`/`2`/`0`) of one CMU vowel name, e.g.
/// `AA1`/`AA2`/`AA0` at `0x10079244`/`40`/`48`.
fn stem3(stress: u8, va1: u32, va2: u32, va0: u32) -> &'static [u8] {
    s(match stress {
        1 => va1,
        2 => va2,
        _ => va0,
    })
}

/// `FUN_10047510`: SAMPA-ish ASCII phones -> space-separated CMU phone-name
/// text, used directly for x-worldbet/x-sampa and as stage 2 of x-ipa/""
/// (fed by `ipa_to_sampa`). A leading `'` marks primary stress on the next
/// vowel, `%`/`,` secondary; consumed whether or not a vowel follows.
fn sampa_ascii(text: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut phones = 0i32;
    let mut stress = 0u8;
    let mut i = 0isize;
    let mut push = |out: &mut Vec<u8>, name: &[u8]| -> bool {
        if !out.is_empty() {
            out.push(b' ');
        }
        out.extend_from_slice(name);
        phones += 1;
        out.len() <= 0x1ff && phones <= 0x40
    };
    loop {
        if at(text, i) == 0 {
            return Some(out);
        }
        if stress == 0 {
            if at(text, i) == b'\'' {
                stress = 1;
                i += 1;
            } else if matches!(at(text, i), b'%' | b',') {
                stress = 2;
                i += 1;
            }
        }
        let c = at(text, i);
        let n1 = at(text, i + 1);
        let ok = if c != 0 && b"AaIiUuEeOoVQ3{>@&=".contains(&c) {
            let mut adv = 1isize;
            let name = if c == b'A' {
                if n1 == b':' {
                    adv = 2;
                }
                stem3(stress, 0x10079244, 0x10079240, 0x10079248)
            } else if c == b'Q' || c == b'5' {
                stem3(stress, 0x10079244, 0x10079240, 0x10079248)
            } else if c == b'{' || (c == b'a' && n1 != b'I' && n1 != b'U') {
                stem3(stress, 0x10079238, 0x10079234, 0x1007923c)
            } else if c == b'V' || c == b'^' || (c == b'3' && n1 != b'r') || (c == b'&' && n1 != b'r') || (c == b'@' && n1 != b'U') {
                if (c == b'3' || c == b'@') && n1 == b':' {
                    adv = 2;
                }
                stem3(stress, 0x1007922c, 0x10079228, 0x10079230)
            } else if (c == b'>' && n1 != b'i' && n1 != b'I') || (c == b'O' && n1 == b':') {
                if c == b'O' {
                    adv = 2;
                }
                stem3(stress, 0x10079220, 0x1007921c, 0x10079224)
            } else if c == b'a' && n1 == b'U' {
                adv = 2;
                stem3(stress, 0x10079214, 0x10079210, 0x10079218)
            } else if c == b'a' && n1 == b'I' {
                adv = 2;
                stem3(stress, 0x10079208, 0x10079204, 0x1007920c)
            } else if c == b'E' {
                if n1 == b':' {
                    adv = 2;
                }
                stem3(stress, 0x100791ec, 0x100791e8, 0x100791f0)
            } else if (c == b'3' || c == b'=' || c == b'&') && n1 == b'r' {
                adv = 2;
                stem3(stress, 0x100791e0, 0x100791dc, 0x100791e4)
            } else if c == b'e' && matches!(n1, b'i' | b'I') {
                adv = 2;
                stem3(stress, 0x100791d4, 0x100791d0, 0x100791d8)
            } else if c == b'I' && n1 != b'@' {
                stem3(stress, 0x100791bc, 0x100791b8, 0x100791c0)
            } else if c == b'i' && n1 != b'&' {
                if n1 == b':' {
                    adv = 2;
                }
                stem3(stress, 0x100791b0, 0x100791ac, 0x100791b4)
            } else if (c == b'o' || c == b'@') && n1 == b'U' {
                adv = 2;
                stem3(stress, 0x1007918c, 0x10079188, 0x10079190)
            } else if (c == b'O' && n1 == b'I') || (c == b'>' && matches!(n1, b'i' | b'I')) {
                adv = 2;
                stem3(stress, 0x10079180, 0x1007917c, 0x10079184)
            } else if c == b'U' && n1 != b'@' {
                stem3(stress, 0x1007915c, 0x10079158, 0x10079160)
            } else if c == b'u' && n1 != b'&' {
                if n1 == b':' {
                    adv = 2;
                }
                stem3(stress, 0x10079150, 0x1007914c, 0x10079154)
            } else if (c == b'I' && n1 == b'@') || (c == b'i' && n1 == b'&') {
                adv = 2;
                stem3(stress, 0x10080cf4, 0x10080cec, 0x10080ce4)
            } else if (c == b'U' && n1 == b'@') || (c == b'u' && n1 == b'&') {
                adv = 2;
                stem3(stress, 0x10080cdc, 0x10080cd4, 0x10080ccc)
            } else {
                return None;
            };
            stress = 0;
            i += adv;
            push(&mut out, name)
        } else if c != 0 && at(text, i + 1) == b'u' && at(text, i + 2) == b':' && c == b'j' {
            i += 3;
            push(&mut out, stem3(stress, 0x10080ca8, 0x10080ca0, 0x10080c98))
        } else if c == b'd' && n1 == b'Z' {
            i += 2;
            push(&mut out, s(0x100791a8))
        } else if c == b't' && n1 == b'S' {
            i += 2;
            push(&mut out, s(0x100791fc))
        } else if c == b'D' {
            i += 1;
            push(&mut out, s(0x100791f4))
        } else if c == b'h' {
            i += 1;
            push(&mut out, s(0x100791c4))
        } else if c == b'N' {
            i += 1;
            push(&mut out, s(0x10079194))
        } else if c == b'S' {
            i += 1;
            push(&mut out, s(0x1007916c))
        } else if c == b'T' {
            i += 1;
            push(&mut out, s(0x10079164))
        } else if c == b'Z' {
            i += 1;
            push(&mut out, s(0x10079138))
        } else if c == b'b' {
            i += 1;
            push(&mut out, s(0x10079200))
        } else if c == b'd' {
            i += 1;
            push(&mut out, s(0x100791f8))
        } else if c == b'f' {
            i += 1;
            push(&mut out, s(0x100791cc))
        } else if c == b'g' {
            i += 1;
            push(&mut out, s(0x100791c8))
        } else if c == b'k' || c == b'x' {
            i += 1;
            push(&mut out, s(0x100791a4))
        } else if c == b'l' {
            i += 1;
            push(&mut out, s(0x100791a0))
        } else if c == b'm' {
            i += 1;
            push(&mut out, s(0x1007919c))
        } else if c == b'n' {
            i += 1;
            push(&mut out, s(0x10079198))
        } else if c == b'p' {
            i += 1;
            push(&mut out, s(0x10079178))
        } else if c == b'9' || c == b'r' {
            i += 1;
            push(&mut out, s(0x10079174))
        } else if c == b's' {
            i += 1;
            push(&mut out, s(0x10079170))
        } else if c == b't' {
            i += 1;
            push(&mut out, s(0x10079168))
        } else if c == b'v' {
            i += 1;
            push(&mut out, s(0x10079148))
        } else if c == b'w' || c == b'W' {
            i += 1;
            push(&mut out, s(0x10079144))
        } else if c == b'j' {
            i += 1;
            push(&mut out, s(0x10079140))
        } else if c == b'z' {
            i += 1;
            push(&mut out, s(0x1007913c))
        } else if c == b'#' {
            let n = at(text, i + 1);
            i += 1;
            if out.is_empty() || n == b'#' || n == 0 {
                true
            } else {
                push(&mut out, s(0x10079280))
            }
        } else if c == b'.' || c == b'|' {
            i += 1;
            true
        } else {
            return None;
        };
        if !ok {
            return None;
        }
    }
}

/// `FUN_100487a0`: collapses a vowel token immediately followed by a token
/// starting "ER" (0x10048a08-0x10048c86, disassembly-confirmed) to the vowel
/// token + a bare "R"; promotes a word-final bare `W`/`Y` glide token
/// (word-final = the last token, or immediately before a `#` token) to
/// `UW0`/`IY0`; then, if no token carries primary stress (`1`), promotes the
/// first `0` to `1`.
fn stress_fixup(text: &[u8]) -> Vec<u8> {
    let toks: Vec<&[u8]> = text.split(|&c| c == b' ').collect();
    // `FUN_100487a0` (0x10048a08-0x10048c86, disassembly-confirmed): a
    // 2-letter+digit vowel token immediately followed by a token starting
    // "ER" collapses to the vowel token unchanged + a bare "R" (dropping the
    // "ER" token's own leading "E" and stress digit entirely, uninspected).
    // Only these 6 vowels trigger it (0x10048a29-0x10048a95's letter
    // cascade: 'A'+{'A','E','O'}, 'E'+'H', 'I'+'H', 'U'+'H' -- i.e. exactly
    // AA/AE/AO/EH/IH/UH, not AH/AY/AW/EY/IY/OW/OY/UW).
    let mut toks: Vec<&[u8]> = {
        let mut out = Vec::with_capacity(toks.len());
        let mut i = 0;
        while i < toks.len() {
            let is_er_vowel = toks[i].len() == 3 && matches!(&toks[i][..2], b"AA" | b"AE" | b"AO" | b"EH" | b"IH" | b"UH");
            if is_er_vowel && i + 1 < toks.len() && toks[i + 1].starts_with(b"ER") {
                out.push(toks[i]);
                out.push(b"R".as_slice());
                i += 2;
            } else {
                out.push(toks[i]);
                i += 1;
            }
        }
        out
    };
    for i in 0..toks.len() {
        let last = i + 1 == toks.len() || toks[i + 1] == b"#";
        if toks[i] == b"W" && last {
            toks[i] = b"UW0";
        } else if toks[i] == b"Y" && last {
            toks[i] = b"IY0";
        }
    }
    let mut out = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        if i > 0 {
            out.push(b' ');
        }
        out.extend_from_slice(t);
    }
    if !out.contains(&b'1') {
        if let Some(p) = out.iter().position(|&c| c == b'0') {
            out[p] = b'1';
        }
    }
    out
}

/// `FUN_10046fb0`: x-ipa/default phones (`;`-separated decimal Unicode
/// codepoints, e.g. `;107;230;116;` for "k{t") -> SAMPA-ish ASCII text fed to
/// `sampa_ascii`. Runs of separator codepoints (space/`#`/`.`) collapse to one
/// `#`; leading/trailing separators are dropped, and a stray one left after
/// that collapse (from a trailing run longer than 2) is trimmed off both ends
/// of the result. `None` if the result would be empty (`FUN_10046fb0` itself
/// returns false then) or a codepoint isn't recognized.
fn ipa_to_sampa(ph: &[u8]) -> Option<Vec<u8>> {
    let mut cps: Vec<i32> = Vec::new();
    let mut at_ = 0usize;
    while at_ < ph.len() {
        if ph[at_] == b';' {
            at_ += 1;
            continue;
        }
        let st = at_;
        while at_ < ph.len() && ph[at_] != b';' && at_ - st < 3 {
            at_ += 1;
        }
        let v = std::str::from_utf8(&ph[st..at_]).ok().and_then(|x| x.parse::<i32>().ok())?;
        if v == 0 || cps.len() > 0x40 {
            return None;
        }
        cps.push(v);
    }
    let is_sep = |c: i32| matches!(c, 0x20 | 0x23 | 0x2e);
    let mut out = Vec::new();
    for k in 0..cps.len() {
        let cp = cps[k];
        let nonsep = !is_sep(cp);
        let first_ok = k != 0 && !is_sep(cps[k - 1]);
        let last_ok = k + 1 != cps.len();
        if !((nonsep || first_ok) && (nonsep || last_ok)) {
            continue;
        }
        if is_sep(cp) {
            out.push(b'#');
            continue;
        }
        let next = cps.get(k + 1).copied().unwrap_or(0);
        let prev = if k == 0 { 0 } else { cps[k - 1] };
        match cp {
            0x61..=0x7a if matches!(cp, 0x61 | 0x62 | 0x64 | 0x66..=0x70 | 0x73 | 0x74 | 0x76..=0x78 | 0x7a) => {
                out.push(cp as u8);
            }
            0x65 => out.push(if matches!(next, 0x69 | 0x26a) { b'e' } else { b'E' }),
            0x75 => out.push(if next == 0x259 { b'U' } else { b'u' }),
            0x279 => out.push(b'9'),
            0x259 => {
                if next == 0x2d0 {
                    out.push(b'3');
                } else if k == 0 || prev != 0x69 {
                    out.push(b'@');
                } else {
                    out.push(b'&');
                }
            }
            0x251 => out.push(b'A'),
            0xe6 => out.push(b'{'),
            0xf0 => out.push(b'D'),
            0x14b => out.push(b'N'),
            0x252 => out.push(b'Q'),
            0x254 => out.push(if next == 0x2d0 { b'O' } else { b'>' }),
            0x25a => out.extend_from_slice(b"=r"),
            0x25b => out.push(b'E'),
            0x25c => out.push(b'3'),
            0x25d => out.extend_from_slice(b"3r"),
            0x26a => out.push(b'I'),
            0x283 => out.push(b'S'),
            0x28a => out.push(b'U'),
            0x28c => out.push(if next == 0x26a { b'a' } else { b'V' }),
            0x28d => out.push(b'W'),
            0x292 => out.push(b'Z'),
            0x2a4 => out.extend_from_slice(b"dZ"),
            0x2a7 => out.extend_from_slice(b"tS"),
            0x2c8 => out.push(b'\''),
            0x2cc => out.push(b'%'),
            0x2d0 => out.push(if k != 0 && prev == 0x25b { b'@' } else { b':' }),
            0x3b8 => out.push(b'T'),
            _ => return None,
        }
    }
    while out.first() == Some(&b'#') {
        out.remove(0);
    }
    while out.last() == Some(&b'#') {
        out.pop();
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Shared by `FUN_100466d0` (x-ipa/"") and `FUN_10046910` (x-worldbet/
/// x-sampa): `<..T>word\xff` + phone codes from `scan`, through
/// `stress_fixup` and `phones_to_codes`, + `</..>`.
fn phoneme_scanned(ph: &[u8], word: &[u8], scan: impl FnOnce(&[u8]) -> Option<Vec<u8>>) -> Option<Vec<u8>> {
    let ph = trim(ph);
    if ph.is_empty() || word.len() >= 0x600 - 1 {
        return None;
    }
    let mut out = b"<\xa1\xddT>".to_vec();
    out.extend_from_slice(word);
    out.push(0xff);
    if out.len() >= 0x600 - 6 {
        return None;
    }
    let text = scan(&ph)?;
    let fixed = stress_fixup(&text);
    let codes = super::userdict::phones_to_codes(&fixed)?;
    out.extend_from_slice(&codes);
    if out.len() >= 0x600 - 5 {
        return None;
    }
    out.extend_from_slice(b"</\xa1\xdd>");
    Some(out)
}

/// `FUN_100466d0`: x-ipa (and the default, unset-`alphabet`, phoneme
/// converter) via `ipa_to_sampa` then `sampa_ascii`.
fn ipa_conv(ph: &[u8], word: &[u8]) -> Option<Vec<u8>> {
    phoneme_scanned(ph, word, |p| sampa_ascii(&ipa_to_sampa(p)?))
}

/// `FUN_10046910`: x-worldbet/x-sampa via `sampa_ascii` directly.
fn worldbet_conv(ph: &[u8], word: &[u8]) -> Option<Vec<u8>> {
    phoneme_scanned(ph, word, sampa_ascii)
}

/// `FUN_10047e30`: x-ntsampa/x-tasampa phones -> `(consumed, phone-name
/// text)`. Unlike `sampa_ascii`: syllabic `m=`/`n=`/`l=` -> `AH0 M`/`AH0 N`/
/// `AH0 L`, `t_v` (flap) -> `TD`, r-coloring via a trailing `` ` `` (retroflex
/// mark) instead of a dedicated vowel+`r` spelling, `*` joins `%` as a
/// secondary-stress prefix, and `K` is a silent no-op letter (matched but
/// produces nothing). Past 64 phones: if a `#`/` ` boundary was already
/// crossed (the last one emitted), the text is truncated back to just before
/// it and `consumed` stops there too, letting `FUN_10046b20`'s caller open a
/// second `<..T>` block for the rest (the boundary itself, at the start of
/// that remainder, is then dropped as a leading separator, same as any other
/// leading `#`/` `); with no boundary crossed yet, the whole conversion fails
/// (`None`), same as a byte-length overflow (`out.len() > 0x1ff`), which is
/// never soft.
fn push_phone(out: &mut Vec<u8>, phones: &mut i32, name: &[u8]) -> bool {
    if !out.is_empty() {
        out.push(b' ');
    }
    out.extend_from_slice(name);
    *phones += 1;
    true
}

fn sampa_group(text: &[u8]) -> Option<(usize, Vec<u8>)> {
    let mut out = Vec::new();
    let mut phones = 0i32;
    let mut stress = 0u8;
    let mut i = 0isize;
    // (text position of the last `#`/` ` boundary, output length just before it).
    let mut checkpoint: Option<(usize, usize)> = None;
    loop {
        if at(text, i) == 0 {
            return Some((i as usize, out));
        }
        if stress == 0 {
            if at(text, i) == b'\'' {
                stress = 1;
                i += 1;
            } else if at(text, i) == b'%' || at(text, i) == b'*' {
                stress = 2;
                i += 1;
            }
        }
        let c = at(text, i);
        let n1 = at(text, i + 1);
        let n2 = at(text, i + 2);
        let mut adv = 1isize;
        let ok = if c != 0 && b"AaIiUuEeOoV{@3".contains(&c) {
            let name: Vec<u8> = if c == b'a' && n1 == b'U' && n2 == b'r' {
                adv = 3;
                stem3(stress, 0x10080dcc, 0x10080dc4, 0x10080dbc).to_vec()
            } else if c == b'a' && n1 == b'I' && n2 == b'r' {
                adv = 3;
                stem3(stress, 0x10080db4, 0x10080dac, 0x10080da4).to_vec()
            } else if c == b'O' && n1 == b'I' && n2 == b'r' {
                adv = 3;
                stem3(stress, 0x10080d9c, 0x10080d94, 0x10080d8c).to_vec()
            } else if c == b'a' && n1 == b'U' {
                adv = 2;
                stem3(stress, 0x10079214, 0x10079210, 0x10079218).to_vec()
            } else if c == b'a' && n1 == b'I' {
                adv = 2;
                stem3(stress, 0x10079208, 0x10079204, 0x1007920c).to_vec()
            } else if c == b'O' && n1 == b'I' {
                adv = 2;
                stem3(stress, 0x10079180, 0x1007917c, 0x10079184).to_vec()
            } else if c == b'i' && n1 == b'r' {
                // Disassembly-confirmed (0x100480c0/0x100480c3/0x100480c8):
                // the lookahead reads text[i+3], not text[i+2] as the decompile's
                // pointer arithmetic suggests, and on a match only advances one
                // extra byte (to i+2) -- the backtick itself (at i+3) is never
                // consumed. Since a bare `` ` `` isn't a member of any charset
                // this scanner recognizes (vowel, consonant, or `#`/` `/./|/$/K),
                // it always fails the very next position: this branch can only
                // ever be reached on a call that ultimately fails, so it makes
                // no observable difference to any successful conversion. Ported
                // as-is (proven, not assumed, via a live capture below).
                adv = if at(text, i + 3) == b'`' { 3 } else { 2 };
                stem3(stress, 0x10080d84, 0x10080d7c, 0x10080d74).to_vec()
            } else if c == b'e' && n1 == b'r' {
                adv = if at(text, i + 3) == b'`' { 3 } else { 2 };
                stem3(stress, 0x10080d6c, 0x10080d64, 0x10080d5c).to_vec()
            } else if c == b'u' && n1 == b'r' {
                adv = if at(text, i + 3) == b'`' { 3 } else { 2 };
                stem3(stress, 0x10080d54, 0x10080d4c, 0x10080d44).to_vec()
            } else if c == b'e' {
                if n1 == b'I' {
                    adv = 2;
                }
                stem3(stress, 0x100791d4, 0x100791d0, 0x100791d8).to_vec()
            } else if c == b'o' {
                if n1 == b'U' {
                    adv = 2;
                }
                stem3(stress, 0x1007918c, 0x10079188, 0x10079190).to_vec()
            } else if c == b'3' && n1 == b':' && n2 == b'`' {
                adv = 3;
                stem3(stress, 0x100791e0, 0x100791dc, 0x100791e4).to_vec()
            } else if c == b'@' && n1 == b'r' && n2 != b'`' {
                adv = 2;
                stem3(stress, 0x100791e0, 0x100791dc, 0x100791e4).to_vec()
            } else if c == b'A' {
                if n1 == b':' {
                    adv = 2;
                }
                stem3(stress, 0x10079244, 0x10079240, 0x10079248).to_vec()
            } else if c == b'{' {
                if n1 == b':' {
                    adv = 2;
                }
                stem3(stress, 0x10079238, 0x10079234, 0x1007923c).to_vec()
            } else if c == b'V' || c == b'@' {
                if n1 == b':' {
                    adv = 2;
                }
                stem3(stress, 0x1007922c, 0x10079228, 0x10079230).to_vec()
            } else if c == b'O' {
                if n1 == b':' {
                    adv = 2;
                }
                stem3(stress, 0x10079220, 0x1007921c, 0x10079224).to_vec()
            } else if c == b'E' {
                if n1 == b':' {
                    adv = 2;
                }
                stem3(stress, 0x100791ec, 0x100791e8, 0x100791f0).to_vec()
            } else if c == b'I' {
                if n1 == b':' {
                    adv = 2;
                }
                stem3(stress, 0x100791bc, 0x100791b8, 0x100791c0).to_vec()
            } else if c == b'i' {
                if n1 == b':' {
                    adv = 2;
                }
                stem3(stress, 0x100791b0, 0x100791ac, 0x100791b4).to_vec()
            } else if c == b'U' {
                if n1 == b':' {
                    adv = 2;
                }
                stem3(stress, 0x1007915c, 0x10079158, 0x10079160).to_vec()
            } else if c == b'u' {
                if n1 == b':' {
                    adv = 2;
                }
                stem3(stress, 0x10079150, 0x1007914c, 0x10079154).to_vec()
            } else {
                return None;
            };
            stress = 0;
            i += adv;
            push_phone(&mut out, &mut phones, &name)
        } else if c == b'd' && n1 == b'Z' {
            i += 2;
            push_phone(&mut out, &mut phones, s(0x100791a8))
        } else if c == b't' && n1 == b'S' {
            i += 2;
            push_phone(&mut out, &mut phones, s(0x100791fc))
        } else if c == b't' && n1 == b'_' && n2 == b'v' {
            i += 3;
            push_phone(&mut out, &mut phones, s(0x10080d28))
        } else if c == b'm' && n1 == b'=' {
            i += 2;
            push_phone(&mut out, &mut phones, s(0x10080d20))
        } else if c == b'n' && n1 == b'=' {
            i += 2;
            push_phone(&mut out, &mut phones, s(0x10080d18))
        } else if c == b'l' && n1 == b'=' {
            i += 2;
            push_phone(&mut out, &mut phones, s(0x10080d10))
        } else if c == b'D' {
            i += 1;
            push_phone(&mut out, &mut phones, s(0x100791f4))
        } else if c == b'h' {
            i += 1;
            push_phone(&mut out, &mut phones, s(0x100791c4))
        } else if c == b'N' {
            i += 1;
            push_phone(&mut out, &mut phones, s(0x10079194))
        } else if c == b'S' {
            i += 1;
            push_phone(&mut out, &mut phones, s(0x1007916c))
        } else if c == b'T' {
            i += 1;
            push_phone(&mut out, &mut phones, s(0x10079164))
        } else if c == b'Z' {
            i += 1;
            push_phone(&mut out, &mut phones, s(0x10079138))
        } else if c == b'b' {
            i += 1;
            push_phone(&mut out, &mut phones, s(0x10079200))
        } else if c == b'd' {
            i += 1;
            push_phone(&mut out, &mut phones, s(0x100791f8))
        } else if c == b'f' {
            i += 1;
            push_phone(&mut out, &mut phones, s(0x100791cc))
        } else if c == b'g' {
            i += 1;
            push_phone(&mut out, &mut phones, s(0x100791c8))
        } else if c == b'k' {
            i += 1;
            push_phone(&mut out, &mut phones, s(0x100791a4))
        } else if c == b'l' {
            i += 1;
            push_phone(&mut out, &mut phones, s(0x100791a0))
        } else if c == b'm' {
            i += 1;
            push_phone(&mut out, &mut phones, s(0x1007919c))
        } else if c == b'n' {
            i += 1;
            push_phone(&mut out, &mut phones, s(0x10079198))
        } else if c == b'p' {
            i += 1;
            push_phone(&mut out, &mut phones, s(0x10079178))
        } else if c == b'r' {
            i += if n1 == b'`' { 2 } else { 1 };
            push_phone(&mut out, &mut phones, s(0x10079174))
        } else if c == b's' {
            i += 1;
            push_phone(&mut out, &mut phones, s(0x10079170))
        } else if c == b't' {
            i += 1;
            push_phone(&mut out, &mut phones, s(0x10079168))
        } else if c == b'v' {
            i += 1;
            push_phone(&mut out, &mut phones, s(0x10079148))
        } else if c == b'w' {
            i += 1;
            push_phone(&mut out, &mut phones, s(0x10079144))
        } else if c == b'j' {
            i += 1;
            push_phone(&mut out, &mut phones, s(0x10079140))
        } else if c == b'z' {
            i += 1;
            push_phone(&mut out, &mut phones, s(0x1007913c))
        } else if c == b'#' || c == b' ' {
            let sep_pos = i as usize;
            let n = at(text, i + 1);
            i += 1;
            if out.is_empty() || n == c || n == 0 {
                true
            } else {
                let before = out.len();
                push_phone(&mut out, &mut phones, s(0x10079280));
                checkpoint = Some((sep_pos, before));
                true
            }
        } else if matches!(c, b'.' | b'|' | b'$') {
            i += 1;
            true
        } else if c == b'K' {
            i += 1;
            true
        } else {
            return None;
        };
        if !ok || out.len() > 0x1ff {
            return None;
        }
        if phones > 0x40 {
            return match checkpoint {
                Some((pos, len)) => {
                    out.truncate(len);
                    Some((pos, out))
                }
                None => None,
            };
        }
    }
}

/// `FUN_10046b20`: x-ntsampa/x-tasampa, `<..T>word\xff codes</..>` followed
/// by a `<..T>\xff codes</..>` block (empty word) per further `sampa_group`
/// call, for as long as `sampa_group` returns early (>64 phones past a `#`/`
/// ` boundary). Bounds checks (`0x600` throughout, confirmed by disassembly
/// at `0x10046b9c`/`0x10046c3f`/`0x10046d3a`): the first block needs
/// `codes.len()+4` bytes of headroom after the `<..T>word\xff` prefix already
/// written, a later block needs `codes.len()+10` (6-byte `<..T>\xff` header +
/// 5-byte closing tag, `-1` for the check being `<=` not `<`); either failing
/// fails the whole conversion (both engine labels just fall back to the
/// word verbatim, so both map to `None` here).
fn ntsampa_conv(ph: &[u8], word: &[u8]) -> Option<Vec<u8>> {
    let ph = trim(ph);
    if ph.is_empty() || word.len() >= 0x600 - 1 {
        return None;
    }
    let mut out = b"<\xa1\xddT>".to_vec();
    out.extend_from_slice(word);
    out.push(0xff);
    if out.len() >= 0x600 - 6 {
        return None;
    }
    let mut pos = 0usize;
    let mut first = true;
    loop {
        let (consumed, text) = sampa_group(&ph[pos..])?;
        if consumed == 0 {
            return None;
        }
        let fixed = stress_fixup(&text);
        let codes = super::userdict::phones_to_codes(&fixed)?;
        let headroom = if first { 4 } else { 10 };
        if codes.len() + headroom + out.len() >= 0x600 {
            return None;
        }
        if !first {
            out.extend_from_slice(b"<\xa1\xddT>\xff");
        }
        out.extend_from_slice(&codes);
        out.extend_from_slice(b"</\xa1\xdd>");
        first = false;
        pos += consumed;
        if pos >= ph.len() {
            return Some(out);
        }
    }
}

/// `FUN_10046df0`: part of speech -> `<..U>word\xff P </..>`.
fn partofsp_conv(pos: &[u8], word: &[u8]) -> Option<Vec<u8>> {
    let w = trim(word);
    let n = w.len();
    if n == 0 || n > 0x1c {
        return None;
    }
    let letter = |c: u8| cls(c) & 0xc0 != 0;
    for i in 0..n {
        let c = w[i];
        let inner = b"'_-".contains(&c) && i > 0 && i + 1 < n && letter(w[i - 1]) && letter(w[i + 1]);
        if !letter(c) && !inner && !b" \t".contains(&c) {
            return None;
        }
    }
    let p = [(b"Unknown".as_slice(), b'U'), (b"Noun", b'N'), (b"Verb", b'V'), (b"Modifier", b'M'), (b"Function", b'F'), (b"Interjection", b'I')]
        .iter()
        .find(|(k, _)| stricmp(pos, k) == 0)?
        .1;
    let mut inner = w.clone();
    inner.push(0xff);
    inner.push(p);
    Some(tagged(b'U', &inner))
}

/// `FUN_1004a1c0` (`ssml:characters`). `detail` is a space-separated list of
/// digit-only group sizes (`FUN_10064645` = `atoi` on each token); a `"` is
/// spliced into the `D` content after each group's count of "real" chars
/// (same class test as the main scan), and the groups must exactly consume
/// the content or the whole tag fails (engine: `group_count+sum-1` must
/// equal the total chars ever placed across all `D` chunks). `pausetime`
/// gates via the shared `pause_arg`; mid-loop length-flush chunks use the
/// shared `|N|` form (`tagged_p`, `FUN_10063ed6` format `1008103c`), but the
/// final chunk's C call (format `100810d8`, `"<%s%c>%c%d%c%s</%s>"`) passes
/// literal `-1` for both extra `%c` args instead of `'|'`, so the last tag
/// wraps its pause number in `\xff...\xff`, not `|...|` -- an engine bug,
/// ported as-is (verified in disassembly, not just the decompile, which
/// drops the vararg values here).
fn characters(content: &[u8], format: &[u8], detail: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    if !format.is_empty() && stricmp(format, b"characters") != 0 {
        return None;
    }
    let mut groups: Vec<i32> = Vec::new();
    if !detail.is_empty() {
        let d = trim(detail);
        if d.is_empty() {
            return None;
        }
        let mut tok = Vec::new();
        for &b in &d {
            if is_digit(b) {
                tok.push(b);
            } else {
                if tok.is_empty() || b != b' ' {
                    return None;
                }
                groups.push(atoi(&tok));
                tok.clear();
            }
        }
        if !tok.is_empty() {
            groups.push(atoi(&tok));
        }
    }
    let group_count = groups.len() as i32;
    let sum: i32 = groups.iter().sum();
    let pt = pause_arg(pausetime)?;

    let c = trim(content);
    if c.is_empty() {
        return None;
    }
    let punct = s(0x100810ec);
    let mut out = Vec::new();
    let mut chunk = Vec::new();
    let (mut weight, mut mode) = (0, 1);
    let mut since_boundary = 0i32;
    let mut group_idx = 0i32;
    let mut flushed = 0i32;
    let mut i = 0usize;
    while i < c.len() {
        if group_idx < group_count - 1 && groups[group_idx as usize] == since_boundary {
            chunk.push(b'"');
            group_idx += 1;
            since_boundary = 0;
            continue;
        }
        let b = c[i];
        if punct.contains(&b) {
            mode = 3;
        }
        if mode + weight > 0x62 {
            out.extend_from_slice(&tagged_p(b'D', pt, &chunk));
            flushed += chunk.len() as i32;
            chunk.clear();
            weight = 0;
        }
        if cls(b) & 0xd0 == 0 {
            if mode == 3 {
                chunk.push(b);
                weight += 3;
                mode = 1;
                since_boundary += 1;
            } else if !matches!(b, b' ' | b'\t' | b'\n' | b'\r') {
                return None;
            }
        } else {
            chunk.push(b);
            weight += 1;
            since_boundary += 1;
        }
        i += 1;
    }
    if group_count != 0 && group_count + sum - 1 != flushed + chunk.len() as i32 {
        return None;
    }
    if pt < 0 {
        out.extend_from_slice(&tagged(b'D', &chunk));
    } else {
        let mut v = vec![0xff];
        v.extend_from_slice(format!("{}", pt).as_bytes());
        v.push(0xff);
        v.extend_from_slice(&chunk);
        out.extend_from_slice(&tagged(b'D', &v));
    }
    Some(out)
}

fn tok(t: &[u8], off: i32, mode: i32) -> (Tok, i32) {
    let mut x = Tok::default();
    let mut v = t.to_vec();
    v.extend_from_slice(&[0; 64]);
    let r = tokenize(&mut x, &v, off as usize, off, mode, None);
    (x, r)
}

/// `FUN_1004be10` (`vxml:time`): one or more `"HHMM"` [`a`|`p`|`h`|`?`] groups
/// (quote-joined when whitespace-separated, `format`/`detail` empty,
/// `pausetime` as usual). `h` marks 24-hour mode for the hour bound (0..=23
/// instead of the 0..=12 default); `a`/`p` append `A`/`P`; `?` is accepted
/// but produces no text. Each group's H/M validated via `time_field` at
/// mode 0 (default) or 1 (`h` seen).
fn vxml_time(content: &[u8], format: &[u8], detail: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    if !format.is_empty() || !detail.is_empty() {
        return None;
    }
    let pt = pause_arg(pausetime)?;
    let c = trim(content);
    if c.is_empty() {
        return None;
    }
    let mut out: Vec<u8> = Vec::new();
    let mut pat: Vec<u8> = Vec::new();
    let (mut hour, mut minute) = (Vec::new(), Vec::new());
    let mut mode: i16 = 0;
    let mut off = 0i32;
    let flush = |hour: &[u8], minute: &[u8], mode: i16| {
        (hour.is_empty() || time_field(hour, b'H', mode)) && (minute.is_empty() || time_field(minute, b'M', mode))
    };
    while (off as usize) < c.len() {
        let (x, r) = tok(&c, off, 4);
        if r == 0 || x.class == 9 || x.len == 0 {
            return None;
        }
        let w = x.text();
        if pat.is_empty() {
            if x.class != 2 || !super::sbd::all_digit(w) || x.len != 4 {
                return None;
            }
            pat = b"N:N".to_vec();
            out.extend_from_slice(&w[..2]);
            out.push(b':');
            out.extend_from_slice(&w[2..]);
            hour = w[..2].to_vec();
            minute = w[2..].to_vec();
        } else {
            if pat.last() != Some(&b'N') || x.len != 1 || !s(0x10081140).contains(&w[0]) {
                return None;
            }
            pat.push(b'X');
            match w[0] {
                b'a' => out.extend_from_slice(s(0x1007d454)),
                b'h' => mode = 1,
                b'p' => out.extend_from_slice(s(0x10079178)),
                _ => {}
            }
        }
        off += r;
        if pat.last() == Some(&b'X') && (off as usize) < c.len() {
            if !flush(&hour, &minute, mode) {
                return None;
            }
            out.push(b'"');
            pat.clear();
            hour.clear();
            minute.clear();
            mode = 0;
        }
    }
    if (pat != b"N:NX" && pat != b"N:N") || !flush(&hour, &minute, mode) {
        return None;
    }
    Some(tagged_p(b'M', pt, &out))
}

/// `FUN_1004ba80` (`vxml:phone`): digit groups with `x` extensions.
fn vxml_phone(content: &[u8], format: &[u8], detail: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    if !format.is_empty() || !detail.is_empty() {
        return None;
    }
    let pt = pause_arg(pausetime)?;
    let c = trim(content);
    let mut out: Vec<u8> = Vec::new();
    let mut pat: Vec<u8> = Vec::new();
    let mut off = 0i32;
    let ok = |p: &[u8]| p == b"NXN" || p == b"XN" || p == b"N";
    while (off as usize) < c.len() {
        let (x, r) = tok(&c, off, 6);
        if r == 0 || x.class == 9 || x.len == 0 {
            return None;
        }
        let w = x.text();
        if w[0] == b'x' && x.len == 1 {
            pat.push(b'X');
        } else {
            if x.class != 2 || !super::sbd::all_digit(w) {
                return None;
            }
            if pat.last() == Some(&b'N') {
                if !ok(&pat) {
                    return None;
                }
                out.push(b'"');
                pat.clear();
            }
            pat.push(b'N');
        }
        out.extend_from_slice(w);
        off += r;
        if pat.len() >= 2 && pat[pat.len() - 2] == b'X' && pat[pat.len() - 1] == b'N' && (off as usize) < c.len() {
            if !(pat == b"NXN" || pat == b"XN") {
                return None;
            }
            out.push(b'"');
            pat.clear();
        }
    }
    if !ok(&pat) {
        return None;
    }
    Some(tagged_p(b'L', pt, &out))
}

/// `FUN_1004b510` (`vxml:currency`): ISO code + amount.
fn vxml_currency(content: &[u8], format: &[u8], detail: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    if !format.is_empty() || !detail.is_empty() {
        return None;
    }
    let pt = pause_arg(pausetime)?;
    let c = trim(content);
    let code_tab = super::tabs::rec66(0x10082d28, 0x1009b058);
    let mut out: Vec<u8> = Vec::new();
    let mut pat: Vec<u8> = Vec::new();
    let mut off = 0i32;
    while (off as usize) < c.len() {
        let (x, r) = tok(&c, off, 5);
        if r == 0 || x.class == 9 || x.len == 0 {
            return None;
        }
        let w = x.text();
        if pat.is_empty() {
            if x.class != 1 || !super::sbd::all_alpha(w) || x.len != 3 || code_tab.find_exact(w, true) < 0 {
                return None;
            }
            if !out.is_empty() {
                out.push(b'"');
            }
            pat.push(b'U');
        } else {
            if pat.last() != Some(&b'U') || !super::numwords::numeric(w) {
                return None;
            }
            pat.push(b'm');
        }
        out.extend_from_slice(w);
        off += r;
        if pat.len() == 2 && (off as usize) < c.len() {
            pat.clear();
        }
    }
    if pat != b"Um" {
        return None;
    }
    Some(tagged_p(b'J', pt, &out))
}

/// The `pausetime` gate the say-as handlers share: empty, or `atoi` in
/// 0..0x10000 (then written as `|%d|` after the opening tag).
fn pause_arg(pausetime: &[u8]) -> Option<i32> {
    if pausetime.is_empty() {
        return Some(-1);
    }
    let v = atoi(pausetime);
    if (0..0x10000).contains(&v) {
        Some(v)
    } else {
        None
    }
}

/// `<%s%c>%s</%s>` or `<%s%c>|%d|%s</%s>`.
fn tagged_p(letter: u8, pt: i32, inner: &[u8]) -> Vec<u8> {
    if pt < 0 {
        return tagged(letter, inner);
    }
    let mut v = format!("|{}|", pt).into_bytes();
    v.extend_from_slice(inner);
    tagged(letter, &v)
}

/// `FUN_1004ab90` (`vxml:boolean`): `TRUE`/`FALSE` tokens as typed,
/// `"`-joined.
fn vxml_boolean(content: &[u8], format: &[u8], detail: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    if !format.is_empty() || !detail.is_empty() {
        return None;
    }
    let pt = pause_arg(pausetime)?;
    let c = trim(content);
    if c.is_empty() {
        return None;
    }
    let mut out: Vec<u8> = Vec::new();
    let mut off = 0i32;
    while (off as usize) < c.len() {
        let (x, r) = tok(&c, off, 8);
        if r == 0 || x.class == 9 || x.len == 0 {
            return None;
        }
        let w = x.text();
        if (x.class != 1 || stricmp(w, s(0x10081120)) != 0) && stricmp(w, s(0x10081118)) != 0 {
            return None;
        }
        if off != 0 {
            out.extend_from_slice(s(0x1007d4f8));
        }
        out.extend_from_slice(w);
        off += r;
    }
    Some(tagged_p(b'G', pt, &out))
}

/// `FUN_1004a960` (`ssml:ordinal`) / `FUN_1004b2e0` (`vxml:digits`): one or
/// more numeric tokens, `"`-joined, passed through raw; `digits_only`
/// selects `vxml:digits`'s stricter all-digit check over `ssml:ordinal`'s
/// `numeric` (commas/decimal point allowed). Both require `format`/`detail`
/// empty and support `pausetime`.
fn numeric_pass(content: &[u8], format: &[u8], detail: &[u8], pausetime: &[u8], letter: u8, digits_only: bool) -> Option<Vec<u8>> {
    if !format.is_empty() || !detail.is_empty() {
        return None;
    }
    let pt = pause_arg(pausetime)?;
    let c = trim(content);
    if c.is_empty() {
        return None;
    }
    let mut out: Vec<u8> = Vec::new();
    let mut off = 0i32;
    while (off as usize) < c.len() {
        let (x, r) = tok(&c, off, 2);
        if r == 0 || x.class == 9 || x.len == 0 {
            return None;
        }
        if x.class != 2 {
            return None;
        }
        let w = x.text();
        let ok = if digits_only { super::sbd::all_digit(w) } else { super::numwords::numeric(w) };
        if !ok {
            return None;
        }
        if !out.is_empty() {
            out.push(b'"');
        }
        out.extend_from_slice(w);
        off += r;
    }
    Some(tagged_p(letter, pt, &out))
}

/// `FUN_1004b7e0` (`vxml:number`, and `sapi:number` format empty/"cardinal"/
/// "decimal"): a resetting one-or-two-slot pattern (`local_8`/`local_c` in
/// the engine): a leading `+`/`-` only counts when the pattern slot it lands
/// on is 0, and every plain number that isn't the very last token resets the
/// slot back to 0 for the next token, so a sign can reappear after any
/// number that isn't final; the *final* pattern (after the loop, or right
/// before such a reset) must be exactly `N` or `+N`. `format`/`detail` must
/// be empty; `pausetime` as usual.
fn vxml_number(content: &[u8], format: &[u8], detail: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    if !format.is_empty() || !detail.is_empty() {
        return None;
    }
    let pt = pause_arg(pausetime)?;
    let c = trim(content);
    if c.is_empty() {
        return None;
    }
    let mut out: Vec<u8> = Vec::new();
    let mut pat: [u8; 2] = [0, 0];
    let mut pos = 0usize;
    let mut off = 0i32;
    while (off as usize) < c.len() {
        let (x, r) = tok(&c, off, 2);
        if r == 0 || x.class == 9 || x.len == 0 {
            return None;
        }
        let w = x.text();
        let is_sign = pos == 0 && x.len == 1 && w[0] != 0 && strchr(s(0x100810c8), w[0]);
        if is_sign {
            pat[pos] = b'+';
        } else {
            if x.class != 2 || !super::numwords::numeric(w) {
                return None;
            }
            pat[pos] = b'N';
        }
        // The token glues directly onto `out` either way (fresh on the very
        // first token, appended after); the `"` only appears at a reset,
        // below, never between a sign and the number that follows it.
        out.extend_from_slice(w);
        off += r;
        if pat[pos] == b'N' && (off as usize) < c.len() {
            out.push(b'"');
            pos = 0;
        } else {
            pos += 1;
        }
    }
    if &pat[..pos] != b"N" && &pat[..pos] != b"+N" {
        return None;
    }
    Some(tagged_p(b'K', pt, &out))
}

/// `FUN_1004ca40` (`sapi:number` format="fraction"): a plain fraction `N/N`,
/// or a mixed number `N N/N` (whole and numerator glued by a literal `+`);
/// repeated complete groups are `"`-joined (`format`/`detail` are already
/// cleared to empty by the `sapi:number` dispatcher before this runs).
fn sapi_fraction(content: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    let pt = pause_arg(pausetime)?;
    let c = trim(content);
    if c.is_empty() {
        return None;
    }
    let mut out: Vec<u8> = Vec::new();
    let mut pat: Vec<u8> = Vec::new();
    let mut pos = 0usize;
    let mut off = 0i32;
    while (off as usize) < c.len() {
        let (x, r) = tok(&c, off, 0xd);
        if r == 0 || x.class == 9 || x.len == 0 {
            return None;
        }
        let w = x.text();
        if pat.len() <= pos {
            pat.resize(pos + 1, 0);
        }
        if pos >= 1 && pat[pos - 1] == b'N' && x.len == 1 && w.first() == Some(&b'/') {
            pat[pos] = b'/';
        } else {
            if x.class != 2 || !super::numwords::numeric(w) {
                return None;
            }
            if pos >= 1 && pat[pos - 1] == b'N' {
                out.push(b'+');
            }
            pat[pos] = b'N';
        }
        out.extend_from_slice(w);
        off += r;
        if pos >= 1 && pat[pos - 1] == b'/' && pat[pos] == b'N' && (off as usize) < c.len() {
            if &pat[..pos + 1] != b"N/N" && &pat[..pos + 1] != b"NN/N" {
                return None;
            }
            out.push(b'"');
            pos = 0;
            pat.clear();
        } else {
            pos += 1;
        }
    }
    if &pat[..pos] != b"N/N" && &pat[..pos] != b"NN/N" {
        return None;
    }
    Some(tagged_p(b'P', pt, &out))
}

/// `FUN_10045e90` (`sapi:number`): dispatches on `format` to the `digit`
/// (`I`), `cardinal`/`decimal`/empty (`K`) or `fraction` (`P`) reading,
/// clearing `format`/`detail` to empty for the sub-handler either way.
fn sapi_number(content: &[u8], format: &[u8], detail: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    if !detail.is_empty() {
        return None;
    }
    if format.is_empty() || stricmp(format, b"cardinal") == 0 || stricmp(format, b"decimal") == 0 {
        vxml_number(content, b"", b"", pausetime)
    } else if stricmp(format, b"digit") == 0 {
        numeric_pass(content, b"", b"", pausetime, b'I', true)
    } else if stricmp(format, b"fraction") == 0 {
        sapi_fraction(content, pausetime)
    } else {
        None
    }
}

/// `FUN_1004a690` (`ssml:cardinal`): `format` is a single group-separator
/// char, `detail` a single decimal-point char, each 1 char if given and
/// distinct from each other; when *both* are empty `format` defaults to
/// `.` (`DAT_10077700`). The separator may appear once, between two digits
/// (checked against the byte at the *output* cursor, not the input one - an
/// engine quirk that only shows once a decimal char has already been
/// dropped earlier in the same number), and is rewritten to a literal `.`;
/// the decimal char, once at least one digit has been seen, is dropped
/// entirely (its own digits glue onto the preceding ones); an optional
/// leading sign before any digit.
fn ssml_cardinal(content: &[u8], format: &[u8], detail: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    let pt = pause_arg(pausetime)?;
    let mut format = format.to_vec();
    if format.is_empty() && detail.is_empty() {
        format = s(0x10077700).to_vec();
    }
    let group = if !format.is_empty() {
        if format.len() != 1 {
            return None;
        }
        Some(format[0])
    } else {
        None
    };
    let dec = if !detail.is_empty() {
        if detail.len() != 1 {
            return None;
        }
        Some(detail[0])
    } else {
        None
    };
    if group.is_some() && group == dec {
        return None;
    }
    let c = trim(content);
    if c.is_empty() {
        return None;
    }
    let mut out: Vec<u8> = Vec::new();
    let (mut sign_seen, mut ndigits) = (false, 0i32);
    let n = c.len();
    for i in 0..n {
        let ch = c[i];
        if group == Some(ch) {
            if out.is_empty() {
                return None;
            }
            if !c.get(out.len() - 1).is_some_and(|b| b.is_ascii_digit()) {
                return None;
            }
            if !(i + 1 < n && c[i + 1].is_ascii_digit()) {
                return None;
            }
            if out.contains(&b'.') {
                return None;
            }
            out.push(b'.');
        } else if dec == Some(ch) {
            if ndigits == 0 {
                return None;
            }
            // dropped: its digits glue onto the preceding ones
        } else if ch.is_ascii_digit() {
            out.push(ch);
            ndigits += 1;
        } else {
            if sign_seen || ndigits != 0 || !matches!(ch, b'+' | b'-') {
                return None;
            }
            sign_seen = true;
            out.push(ch);
        }
    }
    if ndigits == 0 {
        return None;
    }
    Some(tagged_p(b'E', pt, &out))
}

/// `FUN_100593c0`: one H/M/S field. `mode` 3 (`vtml:duration`, all fields)
/// uses the shared 0..=99 / 0.0..=100.0 bound regardless of letter; other
/// modes (`ssml:time`/`sapi:time`) bound `M`/other at 0..=59/0..=60,
/// seconds at 0.0..=60.0, and `H` at 0..=12 (mode 0), 0..=24 (mode ==
/// the field's own length, i.e. 2), or 0..=23 (anything else, e.g. 1).
fn time_field(w: &[u8], f: u8, mode: i16) -> bool {
    let d = |c: u8| cls(c) & 0x10 != 0;
    if f == 0 || !strchr(s(0x10081188), f) {
        return false;
    }
    if w.len() == 1 && d(w[0]) {
        return true;
    }
    if w.len() == 2 && d(w[0]) && d(w[1]) {
        let v = atoi(w);
        if mode == 3 {
            return (0..=99).contains(&v);
        }
        return match f {
            b'H' if mode == 2 => (0..=24).contains(&v),
            b'H' if mode == 0 => (0..=12).contains(&v),
            b'H' => (0..=23).contains(&v),
            b'M' => (0..=59).contains(&v),
            _ => (0..=60).contains(&v),
        };
    }
    if f != b'S' || !super::numwords::numeric(w) {
        return false;
    }
    let n = w.iter().take_while(|&&c| c.is_ascii_digit() || c == b'.').count();
    let v = std::str::from_utf8(&w[..n]).ok().and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0) as f32;
    if mode == 3 {
        (0.0..=100.0).contains(&v)
    } else {
        (0.0..=60.0).contains(&v)
    }
}

/// `FUN_1004d540` (`vtml:duration`): numbers assigned to the `format` field
/// order (`H`, `HM`, `HMS` (default), `M`, `MS`, `S`) across at most two
/// `:`/`'`/`"` separators, written as `H:M:S` with `N` for a missing field.
/// More than six tokens stop the scan without failing.
fn vtml_duration(content: &[u8], format: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    let order: &[u8] = if format.is_empty() {
        s(0x10081188)
    } else {
        let list = [s(0x1007d428), s(0x1008118c), s(0x10081188), s(0x1007919c), s(0x100805bc), s(0x10079170)];
        *list.iter().find(|x| stricmp(x, format) == 0)?
    };
    let pt = pause_arg(pausetime)?;
    let c = trim(content);
    if c.is_empty() {
        return None;
    }
    let (mut h, mut m, mut sec): (Vec<u8>, Vec<u8>, Vec<u8>) = (Vec::new(), Vec::new(), Vec::new());
    let mut pat = [0u8; 8];
    let (mut colons, mut k, mut off) = (0usize, 0usize, 0i32);
    while (off as usize) < c.len() {
        if k > 5 {
            break;
        }
        let (x, r) = tok(&c, off, 0xf);
        if r == 0 || x.class == 9 || x.len == 0 {
            return None;
        }
        let w = x.text();
        if x.class == 2 && super::numwords::numeric(w) {
            let f = at(order, colons as isize);
            let slot = match f {
                b'H' => Some(&mut h),
                b'M' => Some(&mut m),
                b'S' => Some(&mut sec),
                _ => None,
            };
            if let Some(slot) = slot {
                if !slot.is_empty() || !time_field(w, f, 3) {
                    return None;
                }
                *slot = w.to_vec();
            }
            pat[k] = b'N';
        } else {
            let c0 = at(w, 0);
            if c0 == 0 || !strchr(s(0x10081184), c0) || x.len != 1 || k < 1 || pat[k - 1] != b'N' || colons > 1 {
                return None;
            }
            pat[k] = b':';
            colons += 1;
        }
        off += r;
        k += 1;
    }
    let mut out = Vec::new();
    for (i, f) in [h, m, sec].iter().enumerate() {
        if i > 0 {
            out.push(b':');
        }
        out.extend_from_slice(if f.is_empty() { s(0x10079198) } else { f });
    }
    Some(tagged_p(b'V', pt, &out))
}

/// `FUN_10048e10` (`ssml:date`, dispatched to directly; `sapi:date`'s entry
/// is the thin wrapper `FUN_10045b70`, which requires `detail` empty,
/// defaults an empty `format` to `"MDY"` and always calls in here with
/// `detail` fixed to the literal `"SAPI"`): `format` selects a field-order
/// string from the 10-candidate table `D`,`DM`,`DMY`,`M`,`MD`,`MDY`,`MY`,
/// `Y`,`YM`,`YMD` (case-insensitive; default `"MDY"` when empty), one
/// numeric token per letter validated by its role's own range
/// (`norm_date::mon`/`day`/`yr`), up to two separators from `./-`
/// (`DAT_10081010`) between fields (each checked against a digit on both
/// sides in the original text); content fields beyond the format's length
/// are consumed but ignored. `detail` is checked *case-sensitively*
/// (`FUN_1001c1f0` is a plain `strcmp`, not folded) and must be empty or
/// exactly `"SAPI"` (any other value, including e.g. `"julian"`, fails) -
/// `"SAPI"` selects `norm_date::yr`'s mode 2 (2- or 4-digit years only)
/// over the default mode 1 (also allows 3-digit). Output is always
/// `month/day/year`, `N` for a field the format didn't ask for.
fn sayas_date(content: &[u8], format: &[u8], detail: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    const ORDERS: [&[u8]; 10] = [b"D", b"DM", b"DMY", b"M", b"MD", b"MDY", b"MY", b"Y", b"YM", b"YMD"];
    let order: &[u8] = if format.is_empty() {
        b"MDY"
    } else {
        *ORDERS.iter().find(|o| stricmp(o, format) == 0)?
    };
    let mode: i16 = if detail.is_empty() {
        1
    } else if detail == b"SAPI" {
        2
    } else {
        return None;
    };
    let pt = pause_arg(pausetime)?;
    let c = trim(content);
    if c.is_empty() {
        return None;
    }
    let (mut m, mut d, mut y): (Vec<u8>, Vec<u8>, Vec<u8>) = (Vec::new(), Vec::new(), Vec::new());
    let mut pos = 0usize;
    let mut off = 0i32;
    let mut last_num = false;
    while (off as usize) < c.len() {
        let (x, r) = tok(&c, off, 0xe);
        if r == 0 || x.class == 9 || x.len == 0 {
            return None;
        }
        let w = x.text();
        if x.class == 2 && super::sbd::all_digit(w) {
            match order.get(pos) {
                Some(b'D') => {
                    if !d.is_empty() || super::norm_date::day(w) < 0 {
                        return None;
                    }
                    d = w.to_vec();
                }
                Some(b'M') => {
                    if !m.is_empty() || super::norm_date::mon(w) < 0 {
                        return None;
                    }
                    m = w.to_vec();
                }
                Some(b'Y') => {
                    if !y.is_empty() || super::norm_date::yr(w, mode) < 0 {
                        return None;
                    }
                    y = w.to_vec();
                }
                _ => {}
            }
            last_num = true;
        } else if x.len == 1 && w.first().is_some_and(|&c| s(0x10081010).contains(&c)) {
            if !last_num || pos >= 2 {
                return None;
            }
            pos += 1;
            last_num = false;
        } else {
            return None;
        }
        off += r;
    }
    let mut out: Vec<u8> = if m.is_empty() { s(0x10079198).to_vec() } else { m };
    out.push(b'/');
    out.extend_from_slice(if d.is_empty() { s(0x10079198) } else { &d });
    out.push(b'/');
    out.extend_from_slice(if y.is_empty() { s(0x10079198) } else { &y });
    Some(tagged_p(b'A', pt, &out))
}

/// `FUN_1004add0` (`vxml:date`): tokens of `2`/`4`/`6`/`8` all-digit chars
/// build up to three fields - month(2)/day(2) split from a 4-digit token
/// not in the year slot, year(4)+month(2) from a 6-digit token, or
/// year(4)+month(2)+day(2) from one 8-digit token, all unconditionally
/// (later tokens can overwrite earlier ones - transliterated as-is); a
/// leading 2-char `?`-run (only its first byte is checked) skips one field
/// without storing a value, a 6-char (or, via a fallthrough the engine
/// takes when a 4-char `?`-run isn't the very first field, *also* a 4-char)
/// `?`-run skips two, and (disassembly-confirmed: the copy at that branch
/// targets the pattern-tracking buffer, not the year slot) a 4-char
/// `?`-run *as* the first field also just skips one, leaving year unset.
/// `pausetime` as usual; year always validated at mode 1 (`norm_date::yr`);
/// output always `month/day/year`, `N` for an unset field.
fn vxml_date(content: &[u8], format: &[u8], detail: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    if !format.is_empty() || !detail.is_empty() {
        return None;
    }
    let pt = pause_arg(pausetime)?;
    let c = trim(content);
    if c.is_empty() {
        return None;
    }
    let (mut y, mut m, mut d): (Vec<u8>, Vec<u8>, Vec<u8>) = (Vec::new(), Vec::new(), Vec::new());
    let mut pos = 0usize;
    let mut off = 0i32;
    while (off as usize) < c.len() {
        let (x, r) = tok(&c, off, 3);
        if r == 0 || x.class == 9 || x.len == 0 {
            return None;
        }
        let w = x.text();
        if x.class == 2 && super::sbd::all_digit(w) && matches!(x.len, 2 | 4 | 6 | 8) {
            match x.len {
                2 => match pos {
                    1 => {
                        m = w.to_vec();
                        pos += 1;
                    }
                    2 => {
                        d = w.to_vec();
                        pos += 1;
                    }
                    _ => return None,
                },
                4 if pos == 0 => {
                    y = w.to_vec();
                    pos = 1;
                }
                4 => {
                    m = w[..2].to_vec();
                    d = w[2..].to_vec();
                    pos += 2;
                }
                6 => {
                    y = w[..4].to_vec();
                    m = w[4..].to_vec();
                    pos += 2;
                }
                _ => {
                    y = w[..4].to_vec();
                    m = w[4..6].to_vec();
                    d = w[6..].to_vec();
                    pos += 3;
                }
            }
        } else if w.first() == Some(&b'?') && matches!(x.len, 2 | 4 | 6) {
            match x.len {
                2 => pos += 1,
                4 if pos == 0 => {
                    pos = 1;
                }
                _ => pos += 2,
            }
        } else {
            return None;
        }
        off += r;
    }
    if !m.is_empty() && super::norm_date::mon(&m) < 0 {
        return None;
    }
    if !d.is_empty() && super::norm_date::day(&d) < 0 {
        return None;
    }
    if !y.is_empty() && super::norm_date::yr(&y, 1) < 0 {
        return None;
    }
    let mut out: Vec<u8> = if m.is_empty() { s(0x10079198).to_vec() } else { m };
    out.push(b'/');
    out.extend_from_slice(if d.is_empty() { s(0x10079198) } else { &d });
    out.push(b'/');
    out.extend_from_slice(if y.is_empty() { s(0x10079198) } else { &y });
    Some(tagged_p(b'H', pt, &out))
}

/// `FUN_10049b40` (`sapi:phone`/`ssml:telephone`), digit groups only (no
/// letters, parentheses or leading `+`). Digit-group tokens are joined by a
/// literal `-` when whitespace-separated from a preceding digit group;
/// A-Z letters (`Q`/`Z` rejected) map to their keypad digit and never get an
/// auto-hyphen; a leading `+` is dropped, and if immediately followed by a
/// <=3-digit group, that group is also dropped from the digits and instead
/// captured for a `+cc-rest` output (only used when fewer than 3 auto
/// hyphens ended up in `rest`); a `(` with a `)` within the next 6 raw
/// chars (`FUN_10062330`) opens a parenthetical (auto-hyphenated in front,
/// same as any other glue char from `0x100810d0`'s `()-./ ` set); a
/// trailing `*`/`#` is allowed once, hyphenated in front only if `rest`
/// already had a hyphen (that position is then remembered and no further
/// hyphen may follow it). `format`, if given, must be 1-3 digits
/// (`FUN_10058350`; validated, otherwise unused); `detail` must be empty.
fn sayas_phone(content: &[u8], format: &[u8], detail: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    if !format.is_empty() && (format.len() > 3 || !super::sbd::all_digit(format)) {
        return None;
    }
    if !detail.is_empty() {
        return None;
    }
    let pt = pause_arg(pausetime)?;
    let c = trim(content);
    if c.is_empty() {
        return None;
    }
    let mut out: Vec<u8> = Vec::new();
    let mut pat: Vec<u8> = Vec::new();
    let mut country: Vec<u8> = Vec::new();
    let mut hyphens = 0i32;
    let mut star_used = false;
    let mut star_tail: Option<usize> = None;
    let mut off = 0i32;
    while (off as usize) < c.len() {
        let (x, r) = tok(&c, off, 0x10);
        if r == 0 || x.class == 9 || x.len == 0 {
            return None;
        }
        let w = x.text();
        let pos = pat.len();
        if pos == 0 && w[0] == b'+' {
            pat.push(b'+');
            off += r;
            continue;
        }
        if x.class == 2 && super::sbd::all_digit(w) {
            if pos >= 1 && pat[pos - 1] == b'+' && x.len <= 3 {
                country = w.to_vec();
                pat.push(b'N');
                off += r;
                continue;
            }
            if pos != 0 {
                if pat[pos - 1] == b'N' && x.ws != 0 && !out.is_empty() {
                    out.push(b'-');
                    hyphens += 1;
                }
            }
            out.extend_from_slice(w);
            pat.push(b'N');
            off += r;
            continue;
        }
        if x.class == 1 && super::sbd::all_alpha(w) {
            for &ch in w {
                let up = ch.to_ascii_uppercase();
                if up == b'Q' || up == b'Z' {
                    return None;
                }
                let idx = if up < b'R' { up - b'A' } else { up - b'A' - 1 };
                out.push(b'2' + idx / 3);
            }
            pat.push(b'N');
            off += r;
            continue;
        }
        if x.len == 1 && w[0] != 0 && s(0x100810d0).contains(&w[0]) {
            let ch = w[0];
            let opens_paren = ch == b'('
                && c.get(off as usize + 1..(off as usize + 7).min(c.len())).is_some_and(|s| s.contains(&b')'));
            if opens_paren {
                if pos == 0 || (pos >= 1 && matches!(pat[pos - 1], b'N' | b'*' | b'#')) {
                    pat.push(b'(');
                } else if pos >= 1 && matches!(pat[pos - 1], b'+' | b'-') {
                    off += r;
                    continue;
                } else {
                    return None;
                }
                if !out.is_empty() {
                    out.push(b'-');
                    hyphens += 1;
                }
                off += r;
                continue;
            }
            if pos < 1 {
                return None;
            }
            if ch == b')' && pat.contains(&b'(') && pat[pos - 1] == b'N' {
                pat.push(b')');
            } else {
                if ch == b'(' {
                    return None;
                }
                if matches!(pat[pos - 1], b'+' | b'-' | b'(') {
                    return None;
                }
                if pat[pos - 1] == b')' {
                    off += r;
                    continue;
                }
                pat.push(b'-');
            }
            if !out.is_empty() {
                out.push(b'-');
                hyphens += 1;
            }
            off += r;
            continue;
        }
        if star_used || w.len() != 1 || !matches!(w[0], b'*' | b'#') {
            return None;
        }
        if out.contains(&b'-') {
            out.push(b'-');
            hyphens += 1;
            star_tail = Some(out.len());
        }
        pat.push(w[0]);
        out.push(w[0]);
        star_used = true;
        off += r;
    }
    if let Some(p) = star_tail {
        if out[p..].contains(&b'-') {
            return None;
        }
    }
    if country.is_empty() || hyphens >= 3 {
        Some(tagged_p(b'C', pt, &out))
    } else {
        let mut full = Vec::with_capacity(country.len() + out.len() + 2);
        full.push(b'+');
        full.extend_from_slice(&country);
        full.push(b'-');
        full.extend_from_slice(&out);
        Some(tagged_p(b'C', pt, &full))
    }
}

/// `FUN_10045b70` (`sapi:date`'s dispatch entry, wrapping `FUN_10048e10`):
/// `detail` (the user's own) must be empty; `format` defaults to `"MDY"`
/// (`sayas_date` already does this default itself); always calls in with
/// `detail` fixed to the literal `"SAPI"`.
fn sapi_date(content: &[u8], format: &[u8], detail: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    if !detail.is_empty() {
        return None;
    }
    sayas_date(content, format, b"SAPI", pausetime)
}

/// `FUN_10045e10` (`sapi:phone`'s dispatch entry, wrapping `FUN_10049b40`):
/// both `format` and `detail` (the user's own) must be empty; always calls
/// in with both fixed to empty (so `sayas_phone`'s own 1-3-digit `format`
/// reading is only reachable via `ssml:telephone`'s direct dispatch entry).
fn sapi_phone(content: &[u8], format: &[u8], detail: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    if !format.is_empty() || !detail.is_empty() {
        return None;
    }
    sayas_phone(content, b"", b"", pausetime)
}

/// `FUN_100491c0` (`ssml:time`): full transliteration. Unlike `vxml:time`/
/// `sapi:time`, the pattern-position counter (`iVar10`) is never reset -
/// there is no multi-group `"`-join here, and no final "pattern must be
/// exactly N/N:N/N:N:N" check either (confirmed by re-reading: nothing
/// past the loop compares the pattern; only whichever of H/M/S ended up
/// non-empty gets validated), so a dangling trailing `:` (e.g. content
/// `"12:"`) is accepted as-is - transliterated, not "fixed". Per token,
/// in the engine's own priority order:
/// 1. While fewer than 3 fields are filled: a plain 1-2 digit token is the
///    next field (`H`/`M`/`S` by position), gated on the *previous* pattern
///    byte being `:` (or this being the very first token).
/// 2. Once the pattern is exactly `N:N:` (`DAT_1008105c`), a longer
///    (possibly decimal) numeric token is accepted as `S` directly
///    (`time_field` handles the bound/decimal check).
/// 3. As the very first token only (position 0): a 3/4/5/6-char all-digit
///    run splits into H(1)+M(2) / H(2)+M(2) / H(1)+M(2)+S(2) / H(2)+M(2)+S(2)
///    respectively (`local_70<5` picks the 2-field shape); failing that, a
///    `numeric` token of length <=5 containing a single `.` splits into
///    H/M around the dot (which is rewritten to `:` in the emitted text).
/// 4. A literal `:` or `.` (single char) continues the group *if* the
///    previous pattern byte is `N` and fewer than 2 colon-equivalents have
///    been used yet (`local_14<2` - the compact/dot shapes above also count
///    against this budget, 1 for the 2-field shapes, 2 for the 3-field
///    ones).
/// 5. Otherwise (and only once, `local_3c==-1` sentinel), the token is
///    checked against the engine's real 8-word am/pm table (`A`/`A.M`/
///    `A.M.`/`AM` -> a literal `A`; `P`/`P.M`/`P.M.`/`PM` -> `P`, matching
///    what `norm_vtml::time` expects) - only when the pattern *so far* is
///    exactly `N`/`N:N`/`N:N:N`.
/// `format` empty/`hms12` -> `time_field` mode 0, `hms24` -> mode 1 (`M`/`S`
/// are mode-independent there); `detail` must be empty; `pausetime` as
/// usual.
fn ssml_time(content: &[u8], format: &[u8], detail: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    const AMPM: [&[u8]; 8] = [b"A", b"A.M", b"A.M.", b"AM", b"P", b"P.M", b"P.M.", b"PM"];
    let mode: i16 = if format.is_empty() || stricmp(format, b"hms12") == 0 {
        0
    } else if stricmp(format, b"hms24") == 0 {
        1
    } else {
        return None;
    };
    if !detail.is_empty() {
        return None;
    }
    let pt = pause_arg(pausetime)?;
    let c = trim(content);
    if c.is_empty() {
        return None;
    }
    let mut out: Vec<u8> = Vec::new();
    let mut pat: Vec<u8> = Vec::new();
    let (mut h, mut m, mut sec): (Vec<u8>, Vec<u8>, Vec<u8>) = (Vec::new(), Vec::new(), Vec::new());
    let mut fields = 0usize;
    let mut colons = 0i32;
    let mut marked = false;
    let mut off = 0i32;
    while (off as usize) < c.len() {
        let (x, r) = tok(&c, off, 0xf);
        if r == 0 || x.class == 9 || x.len == 0 {
            return None;
        }
        let w = x.text();
        let pos = pat.len();
        let mut done = false;

        if fields < 3 && x.class == 2 && super::sbd::all_digit(w) && x.len <= 2 {
            if pos != 0 && pat[pos - 1] != b':' {
                return None;
            }
            match fields {
                0 => h = w.to_vec(),
                1 => m = w.to_vec(),
                _ => sec = w.to_vec(),
            }
            pat.push(b'N');
            out.extend_from_slice(w);
            fields += 1;
            done = true;
        }
        if !done && x.class == 2 && pat == b"N:N:" && time_field(w, b'S', mode) {
            sec = w.to_vec();
            pat.push(b'N');
            out.extend_from_slice(w);
            fields += 1;
            done = true;
        }
        if !done && pos == 0 && x.class == 2 {
            if super::sbd::all_digit(w) && (3..=6).contains(&x.len) {
                let three_field = x.len == 5 || x.len == 6;
                let (hp, rest) = if x.len == 3 || x.len == 5 { w.split_at(1) } else { w.split_at(2) };
                let (mp, sp) = if three_field { rest.split_at(2) } else { (rest, [].as_slice()) };
                h = hp.to_vec();
                m = mp.to_vec();
                out.extend_from_slice(hp);
                out.push(b':');
                out.extend_from_slice(mp);
                pat = if three_field {
                    sec = sp.to_vec();
                    out.push(b':');
                    out.extend_from_slice(sp);
                    colons += 2;
                    b"N:N:N".to_vec()
                } else {
                    colons += 1;
                    b"N:N".to_vec()
                };
                fields = if three_field { 3 } else { 2 };
                done = true;
            } else if x.len <= 5 && super::numwords::numeric(w) {
                if let Some(dot) = w.iter().position(|&b| b == b'.') {
                    h = w[..dot].to_vec();
                    m = w[dot + 1..].to_vec();
                    out.extend_from_slice(&w[..dot]);
                    out.push(b':');
                    out.extend_from_slice(&w[dot + 1..]);
                    pat = b"N:N".to_vec();
                    fields = 2;
                    colons += 1;
                    done = true;
                }
            }
        }
        if !done && x.len == 1 && matches!(w[0], b':' | b'.') {
            if colons >= 2 || pos == 0 || pat[pos - 1] != b'N' {
                return None;
            }
            pat.push(b':');
            out.push(b':');
            colons += 1;
            done = true;
        }
        if !done && !marked {
            if let Some(idx) = AMPM.iter().position(|cand| stricmp(cand, w) == 0) {
                if pat != b"N" && pat != b"N:N" && pat != b"N:N:N" {
                    return None;
                }
                out.extend_from_slice(if idx < 4 { b"A" } else { b"P" });
                marked = true;
                pat.push(w[0]);
                done = true;
            }
        }
        if !done {
            return None;
        }
        off += r;
    }
    if pat.is_empty() {
        return None;
    }
    if (!h.is_empty() && !time_field(&h, b'H', mode)) || (!m.is_empty() && !time_field(&m, b'M', mode)) || (!sec.is_empty() && !time_field(&sec, b'S', mode)) {
        return None;
    }
    Some(tagged_p(b'B', pt, &out))
}

/// `FUN_1004c260` (`sapi:time`'s content parser; the real dispatch entry
/// `FUN_10045bb0` first rewrites `content` as a quoted/escaped format
/// picture before calling in here - that rewrite is NOT ported, so this
/// parses `content` directly): one or more colon-separated `H[:M[:S]]`
/// groups; two numbers back-to-back with no colon between them silently
/// closes the current group (`"`-joined) and starts a new one, mirroring
/// the engine's own reset. `H` is always `time_field` mode 2 (0..=24;
/// `FUN_1004c260` never consults `format`'s hms12/24 the way `ssml:time`
/// does), `M`/`S` mode 0. No am/pm marker. `format`/`detail` must be empty
/// (checked again here, redundantly with the wrapper); `pausetime` as
/// usual.
fn sapi_time(content: &[u8], format: &[u8], detail: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    if !format.is_empty() || !detail.is_empty() {
        return None;
    }
    let pre = sapi_time_preprocess(content)?;
    sapi_time_core(&pre, pausetime)
}

/// `FUN_10045bb0`: rewrites `content` into a quoted/escaped picture before
/// `FUN_1004c260` (`sapi_time_core`) ever sees it. Walked character by
/// character (after `trim`); for each position `i` (disassembly-confirmed,
/// the decompile badly mangled this function's variable reuse):
/// - `i > 0` only: `'` emits a literal `|`; `"` is a pure skip - it writes
///   nothing (disassembly: it sets the "next write position" to one less
///   than current, but the branch never writes, and the shared epilogue's
///   `+1` then nets to "unchanged" - so nothing is deleted from the output
///   either, despite `"` looking like a delete-previous-char op at a
///   glance).
/// - a digit: if there are at least 2 more characters after it and, within
///   this char plus the next two (`FUN_10062330`'s real window - it checks
///   offset 0 before its loop, then up to `maxlen=2` more, i.e. offsets
///   0..=2 total), there's no whitespace but there *is* a `'`, the digit
///   run up to that `'` is zero-padded to 2 chars and prefixed with a
///   literal `00|` (`DAT_10080bd4`="00|"/`DAT_1007d4dc`="0"): a lone digit
///   `N'` (apostrophe at offset 1) becomes `00|0N`, consuming just the one
///   digit (so the next iteration's `i>0` rule turns the `'` itself into
///   another `|`); two digits `NM'` (apostrophe at offset 2) become
///   `00|NM`, consuming both digits. Otherwise the digit passes through
///   unchanged.
/// - anything else: `' '`/`\t`/`\n`/`\r`/`:` pass through unchanged; any
///   other byte fails the whole conversion.
fn sapi_time_preprocess(content: &[u8]) -> Option<Vec<u8>> {
    let c = trim(content);
    if c.is_empty() {
        return None;
    }
    let n = c.len();
    let mut out: Vec<u8> = Vec::new();
    let mut i = 0usize;
    while i < n {
        if i > 0 {
            if c[i] == b'\'' {
                out.push(b'|');
                i += 1;
                continue;
            }
            if c[i] == b'"' {
                // A pure skip (disassembly: `iVar8 = j-1` then the shared
                // epilogue's `j = iVar8+1` nets to "j unchanged", and this
                // branch never writes to the output buffer) - not a pop.
                i += 1;
                continue;
            }
        }
        let ch = c[i];
        if cls(ch) & 0x10 == 0 {
            if !matches!(ch, b' ' | b'\t' | b'\n' | b'\r' | b':') {
                return None;
            }
            out.push(ch);
            i += 1;
            continue;
        }
        if i + 2 < n {
            let window = &c[i..i + 3];
            let has_ws = window.iter().any(|&b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'));
            if !has_ws {
                if let Some(d) = window.iter().position(|&b| b == b'\'') {
                    out.extend_from_slice(b"00|");
                    if d == 1 {
                        out.push(b'0');
                        out.push(ch);
                        i += 1;
                    } else {
                        out.push(c[i]);
                        out.push(c[i + 1]);
                        i += 2;
                    }
                    continue;
                }
            }
        }
        out.push(ch);
        i += 1;
    }
    Some(out)
}

/// `FUN_1004c260`: the value parser `sapi:time`'s preprocessed picture text
/// is fed into. One or more H/M/S groups; a plain 1-2 digit token is the
/// next field (gated on the previous pattern byte being `:`, `|`, or this
/// being the first token); two numbers back-to-back with no separator
/// closes the current group (`"`-joined, requiring the group-so-far be one
/// of the 3 exact complete shapes below) and starts a new one; `:` and `|`
/// are both valid separators (recorded distinctly in the pattern, at most 2
/// total, `local_18`) but may not mix within one group - the only complete
/// shapes are `N:N`, `N:N:N`, or `N|N|N` (`DAT_10081050`/`DAT_10081054`/
/// `s_N_N_N_10081148`; note the pipe form has no 2-field variant). No am/pm
/// marker. `H` is always `time_field` mode 2 (0..=24).
fn sapi_time_core(content: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    let pt = pause_arg(pausetime)?;
    let c = trim(content);
    if c.is_empty() {
        return None;
    }
    let mut out: Vec<u8> = Vec::new();
    let mut pat: Vec<u8> = Vec::new();
    let mut nfields = 0usize;
    let mut seps = 0i32;
    let (mut h, mut m, mut sec): (Vec<u8>, Vec<u8>, Vec<u8>) = (Vec::new(), Vec::new(), Vec::new());
    let mut off = 0i32;
    let flush = |h: &[u8], m: &[u8], sec: &[u8]| {
        (h.is_empty() || time_field(h, b'H', 2)) && (m.is_empty() || time_field(m, b'M', 0)) && (sec.is_empty() || time_field(sec, b'S', 0))
    };
    let complete = |p: &[u8]| p == b"N:N" || p == b"N:N:N" || p == b"N|N|N";
    while (off as usize) < c.len() {
        let (x, r) = tok(&c, off, 0xf);
        if r == 0 || x.class == 9 || x.len == 0 {
            return None;
        }
        let w = x.text();
        let pos = pat.len();
        if x.class == 2 && super::sbd::all_digit(w) && x.len < 3 {
            if pos != 0 && pat[pos - 1] == b'N' {
                if !complete(&pat) || !flush(&h, &m, &sec) {
                    return None;
                }
                out.push(b'"');
                h.clear();
                m.clear();
                sec.clear();
                pat.clear();
                nfields = 0;
                seps = 0;
            } else if pos != 0 && !matches!(pat[pos - 1], b':' | b'|') {
                return None;
            }
            match nfields {
                0 => h = w.to_vec(),
                1 => m = w.to_vec(),
                _ => sec = w.to_vec(),
            }
            pat.push(b'N');
            nfields += 1;
            out.extend_from_slice(w);
        } else if x.len == 1 && matches!(w[0], b':' | b'|') {
            if seps > 1 || pos == 0 || pat[pos - 1] != b'N' {
                return None;
            }
            pat.push(w[0]);
            out.push(w[0]);
            seps += 1;
        } else {
            return None;
        }
        off += r;
    }
    if !complete(&pat) || !flush(&h, &m, &sec) {
        return None;
    }
    Some(tagged_p(b'N', pt, &out))
}

const CODE_TAB: Rec66 = rec66(0x10082d28, 0x1009b058);
const SYMBOL_TAB: Rec5a = rec5a(0x10082350, 0x1009b050);

/// `FUN_1004c740` (`sapi:currency`): (ISO code or symbol, amount) groups,
/// code and amount glued, groups `"`-joined; the last pattern must be `Um`.
fn sapi_currency(content: &[u8], format: &[u8], detail: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    if !format.is_empty() || !detail.is_empty() {
        return None;
    }
    let pt = pause_arg(pausetime)?;
    let c = trim(content);
    if c.is_empty() {
        return None;
    }
    let mut out: Vec<u8> = Vec::new();
    let mut pat = [0u8; 4];
    let (mut k, mut off) = (0usize, 0i32);
    while (off as usize) < c.len() {
        let (x, r) = tok(&c, off, 5);
        if r == 0 || x.class == 9 || x.len == 0 {
            return None;
        }
        let w = x.text();
        if k == 0 {
            let code = x.class == 1 && super::sbd::all_alpha(w) && x.len == 3 && CODE_TAB.find_exact(w, true) >= 0;
            if !code && (x.class != 3 || x.len > 2 || SYMBOL_TAB.find_prefix(w, true) < 0) {
                return None;
            }
            pat[0] = b'U';
            if off != 0 {
                out.extend_from_slice(s(0x1007d4f8));
            }
        } else {
            if pat[k - 1] != b'U' || !super::numwords::numeric(w) {
                return None;
            }
            pat[k] = b'm';
        }
        out.extend_from_slice(w);
        off += r;
        k = if k == 1 && (off as usize) < c.len() { 0 } else { k + 1 };
    }
    pat[k.min(3)] = 0;
    if strcmp(&pat, s(0x10081128)) != 0 {
        return None;
    }
    Some(tagged_p(b'O', pt, &out))
}

const NET_CC_TLD: Ptab = ptab(0x10081764, 0x1009b07c);
const NET_TLD: Ptab = ptab(0x1008172c, 0x1009b074);
const NET_SLD: Ptab = ptab(0x10081714, 0x1009b078);

fn net_find(tab: Ptab, w: &[u8]) -> i32 {
    super::tabs::search(tab.count(), true, w, |i| tab.get(i))
}

/// `FUN_1004cd50` (`sapi:email`/`sapi:web`, shared by their thin wrappers
/// `FUN_10045fd0`/`FUN_10045f70`, which fix `format` to `"email"`/`"url"`
/// before calling in — `is_web` stands in for that fixed format here): the
/// same piece scan as `norm_vtml::netaddr`/`norm_url::url`'s tag classing,
/// capped at 6 pieces (`CMP EBX,0x6`/`JGE` at 0x1004ce1d, disassembly-
/// confirmed) and checked against only 5 of the shared `PATTERNS` table's
/// 11 shapes: `W.H.H.N`-style 7-tag shapes need one more piece than the cap
/// allows, and `D.D.D.D` needs a digit piece, which always fails the
/// per-piece classing here (the `H` fallback requires class 1) — both
/// proven unreachable, not just unhandled. The output content is rebuilt
/// piece-by-piece (`local_278`'s strcpy/strcat loop, disassembly-confirmed
/// at 0x1004cf7f-0x1004cfd8), not the trimmed input verbatim, so whitespace
/// between pieces (e.g. `"www . example . com"`) is dropped.
fn sayas_netaddr(content: &[u8], pausetime: &[u8], is_web: bool) -> Option<Vec<u8>> {
    let pt = pause_arg(pausetime)?;
    let c = trim(content);
    if c.len() <= 1 {
        return None;
    }
    let mut tags: Vec<u8> = Vec::new();
    let mut out: Vec<u8> = Vec::new();
    let (mut p, mut o) = (0usize, 0i32);
    while tags.len() <= 5 && p < c.len() {
        let (x, r) = super::norm_url::piece(&c, p, o, tags.len());
        if r == 0 || x.class == 9 || x.len == 0 {
            return None;
        }
        let w = x.text();
        let tag = if !tags.is_empty() && x.len == 1 && matches!(w[0], b'@' | b'.') {
            if w[0] == b'@' {
                if tags.len() != 1 || tags[0] != b'H' {
                    return None;
                }
            } else if tags.len() % 2 == 0 {
                return None;
            }
            w[0]
        } else if x.len == 3 && stricmp(s(0x10081150), w) == 0 {
            b'W'
        } else if net_find(NET_CC_TLD, w) >= 0 {
            b'N'
        } else if net_find(NET_TLD, w) >= 0 {
            b'U'
        } else if net_find(NET_SLD, w) >= 0 {
            b'u'
        } else if x.len >= 2 && x.class == 1 {
            b'H'
        } else {
            return None;
        };
        tags.push(tag);
        out.extend_from_slice(w);
        p += r as usize;
        o += r;
    }
    let ok = matches!(
        (is_web, tags.as_slice()),
        (true, b"W.H.U") | (true, b"H.H.U") | (true, b"H.U.N") | (true, b"H.U") | (false, b"H@H.U")
    );
    if !ok {
        return None;
    }
    Some(tagged_p(b'Q', pt, &out))
}

/// `FUN_10045fd0` (`sapi:email`): format and detail both empty.
fn sapi_email(content: &[u8], format: &[u8], detail: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    if !format.is_empty() || !detail.is_empty() {
        return None;
    }
    sayas_netaddr(content, pausetime, false)
}

/// `FUN_10045f70` (`sapi:web`): format empty or `"url"` (case-insensitive),
/// detail empty.
fn sapi_web(content: &[u8], format: &[u8], detail: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    if (!format.is_empty() && stricmp(format, b"url") != 0) || !detail.is_empty() {
        return None;
    }
    sayas_netaddr(content, pausetime, true)
}

/// `FUN_1004d1b0` (`sapi:address`, via the thin wrapper `FUN_10046010`,
/// which fixes `format` to `"postal"`): a `"`-joined run of 4-5 digit ZIPs
/// (raw digits) and Canadian FSA+LDU pairs (raw text, joined by a literal
/// `-`), each token classed by `norm_address_csc::postal`; an FSA not
/// immediately followed by its LDU (or a lone LDU) fails.
fn sapi_address(content: &[u8], format: &[u8], detail: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    if (!format.is_empty() && stricmp(format, b"postal") != 0) || !detail.is_empty() {
        return None;
    }
    let pt = pause_arg(pausetime)?;
    let c = trim(content);
    if c.len() == 1 {
        return None;
    }
    let mut out: Vec<u8> = Vec::new();
    let mut expect_ldu = false;
    let mut off = 0i32;
    while (off as usize) < c.len() {
        let (x, r) = tok(&c, off, 0x11);
        if r == 0 || x.class == 9 || x.len == 0 {
            return None;
        }
        let w = x.text();
        if !expect_ldu {
            if (4..6).contains(&x.len) && super::sbd::all_digit(w) {
                out.extend_from_slice(w);
            } else if super::norm_address_csc::postal(w) == 1 {
                out.extend_from_slice(w);
                expect_ldu = true;
            } else {
                return None;
            }
        } else {
            if super::norm_address_csc::postal(w) != 2 {
                return None;
            }
            out.push(b'-');
            out.extend_from_slice(w);
            expect_ldu = false;
        }
        off += r;
        if !expect_ldu && (off as usize) < c.len() {
            out.push(b'"');
        }
    }
    if expect_ldu {
        return None;
    }
    Some(tagged_p(b'R', pt, &out))
}

/// `FUN_10048d60`: looks up `interp` in the engine's 22-entry table
/// (`PTR_s_sapi_address_10080e04`/`PTR_FUN_10080e60`, count `DAT_10080e5c`,
/// `FUN_100560a0` case-insensitive semantics - matched here by name rather
/// than as a literal jump table, since the two are equivalent for a set of
/// distinct names) and calls the handler with `content`/`format`/`detail`/
/// `pausetime`; no gating here beyond the lookup itself; each handler does
/// its own option checks (this used to also gate several handlers on empty
/// `format`/`detail`/`pausetime` here, which the engine's handlers don't -
/// removed).
fn sayas_conv(content: &[u8], interp: &[u8], format: &[u8], detail: &[u8], pausetime: &[u8]) -> Option<Vec<u8>> {
    if stricmp(interp, b"ssml:characters") == 0 {
        characters(content, format, detail, pausetime)
    } else if stricmp(interp, b"vxml:time") == 0 {
        vxml_time(content, format, detail, pausetime)
    } else if stricmp(interp, b"vxml:phone") == 0 {
        vxml_phone(content, format, detail, pausetime)
    } else if stricmp(interp, b"vxml:currency") == 0 {
        vxml_currency(content, format, detail, pausetime)
    } else if stricmp(interp, b"vxml:boolean") == 0 {
        vxml_boolean(content, format, detail, pausetime)
    } else if stricmp(interp, b"vxml:digits") == 0 {
        numeric_pass(content, format, detail, pausetime, b'I', true)
    } else if stricmp(interp, b"vxml:number") == 0 {
        vxml_number(content, format, detail, pausetime)
    } else if stricmp(interp, b"ssml:ordinal") == 0 {
        numeric_pass(content, format, detail, pausetime, b'F', false)
    } else if stricmp(interp, b"ssml:cardinal") == 0 {
        ssml_cardinal(content, format, detail, pausetime)
    } else if stricmp(interp, b"vtml:duration") == 0 {
        vtml_duration(content, format, pausetime)
    } else if stricmp(interp, b"sapi:date") == 0 {
        sapi_date(content, format, detail, pausetime)
    } else if stricmp(interp, b"ssml:date") == 0 {
        sayas_date(content, format, detail, pausetime)
    } else if stricmp(interp, b"vxml:date") == 0 {
        vxml_date(content, format, detail, pausetime)
    } else if stricmp(interp, b"sapi:phone") == 0 {
        sapi_phone(content, format, detail, pausetime)
    } else if stricmp(interp, b"ssml:telephone") == 0 {
        sayas_phone(content, format, detail, pausetime)
    } else if stricmp(interp, b"sapi:currency") == 0 {
        sapi_currency(content, format, detail, pausetime)
    } else if stricmp(interp, b"sapi:number") == 0 {
        sapi_number(content, format, detail, pausetime)
    } else if stricmp(interp, b"sapi:time") == 0 {
        sapi_time(content, format, detail, pausetime)
    } else if stricmp(interp, b"ssml:time") == 0 {
        ssml_time(content, format, detail, pausetime)
    } else if stricmp(interp, b"sapi:email") == 0 {
        sapi_email(content, format, detail, pausetime)
    } else if stricmp(interp, b"sapi:web") == 0 {
        sapi_web(content, format, detail, pausetime)
    } else if stricmp(interp, b"sapi:address") == 0 {
        sapi_address(content, format, detail, pausetime)
    } else {
        None
    }
}

// -------------------------------------------------------- fullwidth map --

/// Fullwidth-map record: `u16` key, string pointers at `+4`/`+8`, selector
/// byte at `+0xc` (never set in the live table). Generated via `fwtab(...)`.
struct FwTab {
    va: u32,
    n: i32,
}

const fn fwtab(va: u32, n: i32) -> FwTab {
    FwTab { va, n }
}

const FW_TAB: FwTab = fwtab(0x1007caf0, 0x49);

impl FwTab {
    fn key(&self, i: i32) -> u16 {
        rd_i16(self.va + i as u32 * 0x10) as u16
    }

    fn value(&self, i: i32) -> &'static [u8] {
        let rec = self.va + i as u32 * 0x10;
        let flag = b(rec + 0xc, 1)[0];
        s(rd_i32(rec + if flag != 0 { 8 } else { 4 }) as u32)
    }

    /// `FUN_100192a0`/`FUN_1001d350`: bsearch by the `(lead << 8) | trail` key.
    fn lookup(&self, c0: u8, c1: u8) -> Option<&'static [u8]> {
        let key = ((c0 as u16) << 8) | c1 as u16;
        let (mut lo, mut hi) = (0i32, self.n - 1);
        while lo <= hi {
            let mid = (lo + hi) / 2;
            let k = self.key(mid);
            if k == key {
                return Some(self.value(mid));
            } else if k < key {
                lo = mid + 1;
            } else {
                hi = mid - 1;
            }
        }
        None
    }
}

/// `fe_MapFullwidthChars` (`0x1001d370`): a `dbcs_pair` in `FW_TAB` becomes
/// its ASCII form, any other pair passes through; every output byte of a pair
/// maps to the lead in `map_b` and the trail in `map_c`. The decompile's
/// past-the-old-map arms are dead: the old map is identity in the first pass.
fn map_fullwidth(src: &[u8]) -> (Vec<u8>, Vec<i32>, Vec<i32>) {
    let n = src.len();
    let mut out = Vec::with_capacity(n);
    let (mut b, mut c) = (Vec::with_capacity(n), Vec::with_capacity(n));
    let mut i = 0usize;
    while i < n {
        let c0 = src[i];
        let c1 = at(src, i as isize + 1);
        if has_n(src, i as isize, 2) && dbcs_pair(c0, c1) {
            let m = (i as i32, i as i32 + 1);
            match FW_TAB.lookup(c0, c1) {
                Some(v) => {
                    for &x in v {
                        out.push(x);
                        b.push(m.0);
                        c.push(m.1);
                    }
                }
                None => {
                    for x in [c0, c1] {
                        out.push(x);
                        b.push(m.0);
                        c.push(m.1);
                    }
                }
            }
            i += 2;
        } else {
            out.push(c0);
            b.push(i as i32);
            c.push(i as i32);
            i += 1;
        }
    }
    (out, b, c)
}

// ------------------------------------------------------------ the pass ----

#[derive(Clone, Default)]
pub struct Chan {
    pub pos: Vec<i32>,
    pub val: Vec<i32>,
}

pub struct Mark {
    pub pos: i32,
    pub name: Vec<u8>,
    pub kind: u8,
}

pub struct Initial {
    pub processed: Vec<u8>,
    pub map_b: Vec<i32>,
    pub map_c: Vec<i32>,
    pub chans: [Chan; 8],
    pub marks: Vec<Mark>,
    pub override_table: [i32; 11],
    pub pause_override: (i32, i32),
}

/// `TIB+0x1210d4`, 11 `i32`s (the PSV "eff"/"base" block at `+0x1210e8..fc`
/// plus 3 leading sentinels). `TIB+0x1312c4`/`+0x1312c8`: `pause_override`.
/// Neither is ever written by `fe_InitialTextProcess`/`fe_ExtractVtmlTags` or
/// any of its 31 tag handlers (checked branch-by-branch: `FUN_10030500`
/// pitch/speed/volume/emotion, `FUN_1002ed30` break, `FUN_1002f8e0` pause,
/// `FUN_1002efd0` mark, `FUN_1002f3a0` partofsp, `FUN_1002fcf0` phoneme,
/// `FUN_10030c90` sayas, `FUN_100313d0` skip, `FUN_10031650` sub/say-as --
/// none references `0x1210d4..0x121100` or `0x1312c4`/`0x1312c8`). Both are
/// only ever touched by `fe_ResolveSentencePSV` (a later, per-sentence stage
/// outside this file's scope), so every one of the 3487 existing `initial`
/// captures shows the same TIB-reset defaults regardless of which VTML/`vt_`
/// tags the line uses.
const OVERRIDE_DEFAULT: [i32; 11] = [0, 0, -1, -1, -1, 0, 0, 0, 0, 0, 0];
const PAUSE_OVERRIDE_DEFAULT: (i32, i32) = (-1, -1);

#[derive(Clone, Copy)]
enum H {
    Level(usize, usize),
    Break(usize),
    Pause(usize),
    Mark,
    PartOfSp,
    Phoneme,
    SayAs,
    Skip,
    Sub(usize),
}

/// `0x1007e888`: tag, terminator, handler, attribute delimiters.
const TAGS: [(&[u8], &[u8], H, &[u8]); 31] = [
    (b"<vtml_break", b"/>", H::Break(3), b" \x0c\n\r\t\x0b=\""),
    (b"<vtml_emotion", b">", H::Level(7, 0x17), b" \x0c\n\r\t\x0b=\""),
    (b"</vtml_emotion", b">", H::Level(7, 0x17), WS),
    (b"<vtml_mark", b"/>", H::Mark, b" \x0c\n\r\t\x0b=\""),
    (b"<vtml_partofsp", b">", H::PartOfSp, b" \x0c\n\r\t\x0b=\""),
    (b"</vtml_partofsp", b">", H::PartOfSp, WS),
    (b"<vtml_pause", b"/>", H::Pause(6), b" \x0c\n\r\t\x0b=\""),
    (b"<vtml_phoneme", b">", H::Phoneme, b" \x0c\n\r\t\x0b=\""),
    (b"</vtml_phoneme", b">", H::Phoneme, WS),
    (b"<vtml_pitch", b">", H::Level(0, 0xc), b" \x0c\n\r\t\x0b=\""),
    (b"</vtml_pitch", b">", H::Level(0, 0xc), WS),
    (b"<vtml_sayas", b">", H::SayAs, b" \x0c\n\r\t\x0b=\""),
    (b"</vtml_sayas", b">", H::SayAs, WS),
    (b"<vtml_skip", b">", H::Skip, b""),
    (b"</vtml_skip", b">", H::Skip, b""),
    (b"<vtml_speed", b">", H::Level(1, 0xc), b" \x0c\n\r\t\x0b=\""),
    (b"</vtml_speed", b">", H::Level(1, 0xc), WS),
    (b"<vtml_sub", b">", H::Sub(0x11), b" \x0c\n\r\t\x0b=\""),
    (b"</vtml_sub", b">", H::Sub(0x11), WS),
    (b"<vtml_volume", b">", H::Level(2, 0xc), b" \x0c\n\r\t\x0b=\""),
    (b"</vtml_volume", b">", H::Level(2, 0xc), WS),
    (b"<vt_pitch", b">", H::Level(0, 0x13), b" \x0c\n\r\t\x0b="),
    (b"</vt_pitch", b">", H::Level(0, 0x13), WS),
    (b"<vt_speed", b">", H::Level(1, 0x13), b" \x0c\n\r\t\x0b="),
    (b"</vt_speed", b">", H::Level(1, 0x13), WS),
    (b"<vt_volume", b">", H::Level(2, 0x13), b" \x0c\n\r\t\x0b="),
    (b"</vt_volume", b">", H::Level(2, 0x13), WS),
    (b"<vt_pause", b">", H::Pause(0x13), b" \x0c\n\r\t\x0b="),
    (b"<vt_break", b">", H::Break(0x14), b" \x0c\n\r\t\x0b="),
    (b"<say-as", b">", H::Sub(0x15), b" \x0c\n\r\t\x0b=\""),
    (b"</say-as", b">", H::Sub(0x15), WS),
];

/// Per-channel clamp (`0x1007ec04` min, `0x1007ebf0` max).
const CLAMP: [(i32, i32); 5] = [(50, 200), (50, 400), (0, 500), (0, 65535), (0, 3)];

struct St<'a> {
    src: &'a [u8],
    pb: Vec<i32>,
    pc: Vec<i32>,
    out: Vec<u8>,
    b: Vec<i32>,
    c: Vec<i32>,
    chans: [Chan; 8],
    marks: Vec<Mark>,
    /// `TIB+0x1312d5..dd`: emotion, partofsp, phoneme, sub/say-as, sayas, skip open.
    open: [bool; 6],
}

const EMO: usize = 0;
const POS: usize = 1;
const PHON: usize = 2;
const SUB: usize = 3;
const SAYAS: usize = 4;
const SKIP: usize = 5;

impl St<'_> {
    fn inherit(&self, i: usize) -> (i32, i32) {
        let n = self.pb.len();
        if i < n {
            (self.pb[i], self.pc[i])
        } else {
            let v = self.pb[n - 1] + 1;
            (v, v)
        }
    }

    /// Map pair for `k` output bytes standing for source `[from, from+len)`.
    fn span(&self, from: usize, len: usize) -> (i32, i32) {
        let n = self.pb.len();
        if from < n {
            (self.pb[from], self.pc[(from + len - 1).min(n - 1)])
        } else {
            let v = self.pb[n - 1] + 1;
            (v, v)
        }
    }

    fn put(&mut self, bytes: &[u8], m: (i32, i32)) {
        for &x in bytes {
            self.out.push(x);
            self.b.push(m.0);
            self.c.push(m.1);
        }
    }

    fn at(&self, i: usize) -> u8 {
        *self.src.get(i).unwrap_or(&0)
    }

    fn rest(&self, i: usize) -> &[u8] {
        &self.src[i.min(self.src.len())..]
    }

    fn find(&self, from: usize, pat: &[u8]) -> Option<usize> {
        self.rest(from).windows(pat.len()).position(|w| w == pat).map(|k| from + k)
    }

    /// `FUN_1002eab0`: the tag body and the whole tag's length.
    fn body(&self, pos: usize, tag: &[u8], term: &[u8]) -> Option<(Vec<u8>, usize)> {
        let e = self.find(pos, term)?;
        let st = pos + tag.len();
        if e < st || e - st > 0x5ff {
            return None;
        }
        let b = self.src[st..e].to_vec();
        let total = b.len() + term.len() + tag.len();
        Some((b, total))
    }

    /// The content up to the first `</name`/`</NAME`, and its length.
    fn content(&self, from: usize, close_lo: &[u8], close_up: &[u8]) -> Option<usize> {
        match (self.find(from, close_lo), self.find(from, close_up)) {
            (None, None) => None,
            (Some(a), None) | (None, Some(a)) => Some(a - from),
            (Some(a), Some(b)) => Some(a.min(b) - from),
        }
    }

    fn close(&mut self, pos: usize, total: usize, flag: usize) -> Option<usize> {
        if strnicmp(self.rest(pos), b"</", 2) != 0 || !self.open[flag] {
            return None;
        }
        let m = self.span(pos, total);
        self.put(b"\xa2\xfe", m);
        self.open[flag] = false;
        Some(total)
    }

    /// Open-tag marker + converted (or raw) content; returns bytes consumed.
    fn content_tag(&mut self, pos: usize, total: usize, clen: usize, conv: Option<Vec<u8>>, flag: usize) -> usize {
        let m = self.span(pos, total);
        self.put(b"\xfd\xfe", m);
        match conv {
            Some(v) => {
                // `FUN_1002fcf0` (the phoneme handler's own map writer,
                // disassembly/decompile-derived): for empty content
                // (`clen == 0`, e.g. an empty word recursion block), the
                // converted text's span isn't `(pb[from], pc[from])` like a
                // real single-source-position span would be -- it's
                // `(pb[from], pc[from-1])`, the trail offset of the position
                // *before* the (empty) gap, not of the position after it.
                // `span()` itself is untouched (its `len>=1` callers are
                // unaffected); this is inlined here since only this
                // (phoneme's) empty-content case exercises it in this batch.
                let from = pos + total;
                let m = if clen == 0 {
                    let b = self.span(from, 1).0;
                    let c = if from > 0 { self.span(from - 1, 1).1 } else { b };
                    (b, c)
                } else {
                    self.span(from, clen)
                };
                self.put(&v, m);
            }
            None => {
                for k in 0..clen {
                    let m = self.inherit(pos + total + k);
                    let x = self.at(pos + total + k);
                    self.put(&[x], m);
                }
            }
        }
        self.open[flag] = true;
        total + clen
    }

    fn handle(&mut self, pos: usize, e: &(&[u8], &[u8], H, &[u8])) -> Option<usize> {
        let (tag, term, h, delims) = *e;
        let (mut body, total) = self.body(pos, tag, term)?;
        let empty = re(0, &body);
        match h {
            H::Level(ch, rx) => {
                if empty {
                    if strnicmp(self.rest(pos), b"</", 2) != 0 || (ch == 7 && !self.open[EMO]) {
                        return None;
                    }
                    let m = self.span(pos, total);
                    self.put(b"\xa2\xfe", m);
                    let p = self.out.len() as i32;
                    self.chans[ch].pos.push(p);
                    self.chans[ch].val.push(-1);
                    if ch == 7 {
                        self.open[EMO] = false;
                    }
                    return Some(total);
                }
                if !re(rx, &body) {
                    return None;
                }
                if ch == 7 {
                    let cat = attr(&body, b"category", delims).unwrap_or_default();
                    let lvl = if re(0x18, &body) { attr(&body, b"level", delims).unwrap_or_default() } else { Vec::new() };
                    let cats = ptabn(0x1007ec18, 20);
                    let ci = (0..20).take_while(|&i| !cats.get(i).is_empty()).position(|i| stricmp(cats.get(i), &cat) == 0)?;
                    let l = atoi(&lvl);
                    let v = ci as i32 + if (1..=3).contains(&l) { l } else { 3 } * 100;
                    let m = self.span(pos, total);
                    self.put(b"\xa2\xfc\xfd\xfe", m);
                    let p = self.out.len() as i32;
                    self.chans[7].pos.push(p);
                    self.chans[7].val.push(v);
                    self.open[EMO] = true;
                    return Some(total);
                }
                let mut at = 0;
                let v = if rx == 0xc {
                    let k = strtok(&body, &mut at, delims)?;
                    if stricmp(k, b"value") != 0 {
                        return None;
                    }
                    strtok(&body, &mut at, delims)?.to_vec()
                } else {
                    strtok(&body, &mut at, delims)?.to_vec()
                };
                let v = atoi(&v).clamp(CLAMP[ch].0, CLAMP[ch].1);
                let m = self.span(pos, total);
                self.put(b"\xfd\xfe", m);
                let p = self.out.len() as i32;
                self.chans[ch].pos.push(p);
                self.chans[ch].val.push(v);
                Some(total)
            }
            H::Break(rx) | H::Pause(rx) => {
                if empty {
                    return None;
                }
                let n = cs(&body).len();
                if (rx == 0x14 || rx == 0x13) && n > 0 && body[n - 1] == b'/' {
                    body.truncate(n - 1);
                }
                if !re(rx, &body) {
                    return None;
                }
                let key: &[u8] = if matches!(h, H::Break(_)) { b"level" } else { b"time" };
                let mut at = 0;
                let t = strtok(&body, &mut at, delims)?;
                let v = if rx == 3 || rx == 6 {
                    if stricmp(t, key) != 0 {
                        return None;
                    }
                    strtok(&body, &mut at, delims)?.to_vec()
                } else {
                    t.to_vec()
                };
                let ch = if matches!(h, H::Break(_)) { 4 } else { 3 };
                let v = atoi(&v).clamp(CLAMP[ch].0, CLAMP[ch].1);
                let m = self.span(pos, total);
                self.put(if ch == 4 && v == 3 { b"\xa2\xfd" } else { b"\xa2\xfe" }, m);
                let p = self.out.len() as i32 - 1;
                self.chans[ch].pos.push(p);
                self.chans[ch].val.push(v);
                Some(total)
            }
            H::Mark => {
                let (name, kind) = if empty {
                    (Vec::new(), 2)
                } else {
                    if !re(4, &body) {
                        return None;
                    }
                    let nm = attr(&body, b"name", delims)?;
                    if nm.len() < 0x200 {
                        (nm, 1)
                    } else {
                        (nm[..0x1ff].to_vec(), 3)
                    }
                };
                let m = self.span(pos, total);
                self.put(b"\xa2\xfe", m);
                let p = self.out.len() as i32 - 1;
                self.marks.push(Mark { pos: p, name, kind });
                Some(total)
            }
            H::PartOfSp => {
                if empty {
                    return self.close(pos, total, POS);
                }
                if !re(5, &body) {
                    return None;
                }
                let clen = self.content(pos + total, b"</vtml_partofsp", b"</VTML_PARTOFSP")?;
                let part = attr(&body, b"part", delims)?;
                let word = self.src[pos + total..pos + total + clen.min(0x1ff)].to_vec();
                let conv = partofsp_conv(&part, &word);
                Some(self.content_tag(pos, total, clen, conv, POS))
            }
            H::Phoneme => {
                if empty {
                    return self.close(pos, total, PHON);
                }
                if !re(7, &body) {
                    return None;
                }
                let clen = self.content(pos + total, b"</vtml_phoneme", b"</VTML_PHONEME")?;
                let ph = attr(&body, b"ph", delims).unwrap_or_default();
                let alpha = if re(8, &body) { attr(&body, b"alphabet", delims).unwrap_or_default() } else { Vec::new() };
                let word = self.src[pos + total..pos + total + clen.min(0x1ff)].to_vec();
                let conv = if stricmp(&alpha, b"x-cmu") == 0 {
                    Some(cmu(&ph, &word).unwrap_or_else(|| word.clone()))
                } else if stricmp(&alpha, b"x-sapi") == 0 {
                    Some(sapi(&ph, &word).unwrap_or_else(|| word.clone()))
                } else if stricmp(&alpha, b"ipa") == 0 || alpha.is_empty() {
                    Some(ipa_conv(&ph, &word).unwrap_or_else(|| word.clone()))
                } else if stricmp(&alpha, b"x-worldbet") == 0 || stricmp(&alpha, b"x-sampa") == 0 {
                    Some(worldbet_conv(&ph, &word).unwrap_or_else(|| word.clone()))
                } else if stricmp(&alpha, b"x-ntsampa") == 0 || stricmp(&alpha, b"x-tasampa") == 0 {
                    Some(ntsampa_conv(&ph, &word).unwrap_or_else(|| word.clone()))
                } else {
                    None
                };
                Some(self.content_tag(pos, total, clen, conv, PHON))
            }
            H::SayAs => {
                if empty {
                    return self.close(pos, total, SAYAS);
                }
                if !re(0xd, &body) {
                    return None;
                }
                let clen = self.content(pos + total, b"</vtml_sayas", b"</VTML_SAYAS")?;
                let get = |rx: usize, name: &[u8]| if rx == 0 || re(rx, &body) { attr(&body, name, delims).unwrap_or_default() } else { Vec::new() };
                let interp = get(0, b"interpret-as");
                let format = get(0xe, b"format");
                let detail = get(0xf, b"detail");
                let mut pt = get(0x10, b"pausetime");
                if !pt.is_empty() {
                    pt = format!("{}", atoi(&pt).max(rd_i32(0x1007ec10)).min(rd_i32(0x1007ebfc))).into_bytes();
                }
                let content = self.src[pos + total..pos + total + clen.min(0x1ff)].to_vec();
                let conv = sayas_conv(&content, &interp, &format, &detail, &pt);
                Some(self.content_tag(pos, total, clen, conv, SAYAS))
            }
            H::Skip => {
                if !empty {
                    return None;
                }
                if strnicmp(self.rest(pos), b"</", 2) == 0 {
                    return self.close(pos, total, SKIP);
                }
                let clen = self.content(pos + total, b"</vtml_skip", b"</VTML_SKIP")?;
                let m = self.span(pos, total);
                self.put(b"\xfd\xfe", m);
                self.open[SKIP] = true;
                Some(total + clen)
            }
            H::Sub(rx) => {
                if empty {
                    return self.close(pos, total, SUB);
                }
                if !re(rx, &body) {
                    return None;
                }
                let clen = if rx == 0x11 {
                    self.content(pos + total, b"</vtml_sub", b"</VTML_SUB")?
                } else {
                    self.content(pos + total, b"</say-as", b"</say-as")?
                };
                let alias = attr(&body, if rx == 0x11 { b"alias".as_slice() } else { b"sub" }, delims).unwrap_or_default();
                let posv = if rx == 0x11 && re(0x12, &body) { attr(&body, b"pos", delims).unwrap_or_default() } else { Vec::new() };
                let conv = if posv.is_empty() { Some(tagged(b'S', &trim(&alias))) } else { None };
                Some(self.content_tag(pos, total, clen, conv, SUB))
            }
        }
    }
}

impl Initial {
    /// Whether the maps are anything but the identity (the engine always
    /// applies them; identity maps are equivalent to none).
    pub fn map_start_differs(&self) -> bool {
        self.map_b.iter().enumerate().any(|(i, &v)| v != i as i32) || self.map_c.iter().enumerate().any(|(i, &v)| v != i as i32)
    }
}

/// `FUN_1002e990`: the longest matching tag entry, then its handler.
fn try_tag(st: &mut St, pos: usize) -> Option<usize> {
    let rest = st.rest(pos);
    let mut best: Option<usize> = None;
    for (i, e) in TAGS.iter().enumerate() {
        let n = e.0.len();
        if rest.len() >= n && !rest[..n].contains(&0) && strnicmp(rest, e.0, n) == 0 && best.map_or(true, |b| TAGS[b].0.len() < n) {
            best = Some(i);
        }
    }
    let e = TAGS[best?];
    let (ol, bl, cl) = (st.out.len(), st.b.len(), st.c.len());
    let r = st.handle(pos, &e);
    if r.is_none() {
        st.out.truncate(ol);
        st.b.truncate(bl);
        st.c.truncate(cl);
    }
    r
}

/// `fe_ExtractVtmlTags`' pairing of open/close entries on channels 0..2:
/// unmatched entries are dropped and each close takes the value the channel
/// had before its open.
fn pair(ch: &mut Chan) {
    for round in 0..2 {
        let n = ch.pos.len();
        let mut keep = vec![false; n];
        let mut partner = vec![0usize; n];
        for i in (0..n).rev() {
            if ch.val[i] < 0 {
                continue;
            }
            let mut j = i;
            while j < n && (ch.val[j] >= 0 || keep[j]) {
                j += 1;
            }
            if j != n {
                keep[i] = true;
                keep[j] = true;
                partner[i] = j;
                partner[j] = i;
            }
        }
        if round == 0 {
            let (pos, val): (Vec<i32>, Vec<i32>) = (0..n).filter(|&i| keep[i]).map(|i| (ch.pos[i], ch.val[i])).unzip();
            ch.pos = pos;
            ch.val = val;
        } else {
            for i in 0..n {
                if ch.val[i] < 0 {
                    let o = partner[i];
                    ch.val[i] = if o == 0 { -1 } else { ch.val[o - 1] };
                }
            }
        }
    }
}

pub fn initial(src: &[u8]) -> Initial {
    let src = cs(src);
    if src.is_empty() {
        return Initial {
            processed: Vec::new(),
            map_b: Vec::new(),
            map_c: Vec::new(),
            chans: Default::default(),
            marks: Vec::new(),
            override_table: OVERRIDE_DEFAULT,
            pause_override: PAUSE_OVERRIDE_DEFAULT,
        };
    }
    let (fw, pb, pc) = map_fullwidth(src);
    let n = fw.len();
    let mut st = St { src: &fw, pb, pc, out: Vec::new(), b: Vec::new(), c: Vec::new(), chans: Default::default(), marks: Vec::new(), open: [false; 6] };
    let mut i = 0;
    while i < n {
        if cls(fw[i]) & 6 == 0 {
            if let Some(r) = try_tag(&mut st, i) {
                i += r;
                continue;
            }
        }
        let m = st.inherit(i);
        st.put(&[fw[i]], m);
        i += 1;
    }
    for ch in 0..3 {
        if !st.chans[ch].pos.is_empty() {
            pair(&mut st.chans[ch]);
        }
    }
    let marks = st
        .marks
        .into_iter()
        .map(|mut m| {
            m.pos = *st.b.get(m.pos as usize).unwrap_or(&m.pos);
            m
        })
        .collect();
    Initial {
        processed: st.out,
        map_b: st.b,
        map_c: st.c,
        chans: st.chans,
        marks,
        override_table: OVERRIDE_DEFAULT,
        pause_override: PAUSE_OVERRIDE_DEFAULT,
    }
}
