//! `FUN_1002b410` (chain slot 2): dates. Up to six entries are classified
//! (mode-3 tokens), each with a shape letter (`local_228`) and a role
//! (`local_210`):
//! - 'O' "de"/"del", 'T' "el", 'I' a leading "en", 'S' "AC"/"DC" (shape = role),
//! - 'w' a weekday abbreviation and 'W' a full weekday (shape 'W'),
//! - 'A' a month abbreviation and 'F' a full month (shape 'M'),
//! - numbers (shape 'N'): role 'N', 'Y' (4-digit or "'97" year), 'D' (an
//!   ordinal-suffixed day), or a day/year list or range collapsed into one
//!   "DAYS" entry (`FUN_1002dc20`/`FUN_1002dd40`, role 'D' or 'Y').
//!
//! Separators "/-.," between entries are kept on the previous entry; a '.'
//! after an abbreviation is absorbed into it. The shape string is matched
//! against `PTR_s_WNOMON_10069098` from the longest prefix down; the first
//! candidate `FUN_1002c8e0` resolves is written by `FUN_1002bf10` (src 3).

use super::norm_units::{best_and_last, phrase_find};
use super::numwords::{cardinal_to_words, ordinal_suffix_classify, ordinal_word_1_20};
use super::punct::prev_words;
use super::tables::{CLS_S, HI};
use super::tokenize::{tokenize, Tok};
use super::{emit, emit::emit_split, FeSpa};
use crate::ling_spa::lexicon::stricmp_spa;
use crate::ling_spa::WaSpa;

const SRC: u32 = 3;

/// `DAT_10069148`: the text of a collapsed list/range entry.
const DAYS: &[u8] = b"DAYS";
/// `DAT_100686c0`.
const DE: &[u8] = b"de";
/// `DAT_100686bc`.
const EL: &[u8] = b"el";

/// `0x10069f90` (`DAT_10079dc4` = 24): month abbreviation, month number.
const MONTH_ABBR: [(&[u8], &[u8]); 24] = [
    (b"AB", b"4"),
    (b"ABR", b"4"),
    (b"AG", b"8"),
    (b"AGO", b"8"),
    (b"DI", b"12"),
    (b"DIC", b"12"),
    (b"ENE", b"1"),
    (b"FB", b"2"),
    (b"FEB", b"2"),
    (b"JL", b"7"),
    (b"JN", b"6"),
    (b"JUL", b"7"),
    (b"JUN", b"6"),
    (b"MAR", b"3"),
    (b"MAY", b"5"),
    (b"MR", b"3"),
    (b"MY", b"5"),
    (b"NO", b"11"),
    (b"NOV", b"11"),
    (b"OC", b"10"),
    (b"OCT", b"10"),
    (b"SEP", b"9"),
    (b"SEPT", b"9"),
    (b"SP", b"9"),
];

/// `0x1006a338` (`DAT_10079dc8` = 12): full month names.
const MONTH_FULL: [&[u8]; 12] = [
    b"ABRIL",
    b"AGOSTO",
    b"DICIEMBRE",
    b"ENERO",
    b"FEBRERO",
    b"JULIO",
    b"JUNIO",
    b"MARZO",
    b"MAYO",
    b"NOVIEMBRE",
    b"OCTUBRE",
    b"SEPTIEMBRE",
];

/// `PTR_DAT_10069f58`, by month number.
const MONTH_WORD: [&[u8]; 13] = [
    b"",
    b"Enero",
    b"Febrero",
    b"Marzo",
    b"Abril",
    b"Mayo",
    b"Junio",
    b"Julio",
    b"Agosto",
    b"Septiembre",
    b"Octubre",
    b"Noviembre",
    b"Diciembre",
];

/// `0x1006a510` (`DAT_10079dcc` = 14): weekday abbreviation, full name.
const WEEKDAY_ABBR: [(&[u8], &[u8]); 14] = [
    (b"Do", b"Domingo"),
    (b"Dom", b"Domingo"),
    (b"Ju", b"Jueves"),
    (b"Jue", b"Jueves"),
    (b"Lu", b"Lunes"),
    (b"Lun", b"Lunes"),
    (b"Ma", b"Martes"),
    (b"Mar", b"Martes"),
    (b"Mi", b"Mi\xe9rcoles"),
    (b"Mie", b"Mi\xe9rcoles"),
    (b"Sa", b"S\xe1bado"),
    (b"Sab", b"S\xe1bado"),
    (b"Vi", b"Viernes"),
    (b"Vie", b"Viernes"),
];

/// `PTR_s_Domingo_1006a734` (`DAT_10079dd0` = 7).
const WEEKDAY: [&[u8]; 7] = [b"Domingo", b"Jueves", b"Lunes", b"Martes", b"Mi\xe9rcoles", b"S\xe1bado", b"Viernes"];

/// viol09 `PTR_DAT_10066088` (`DAT_10075734` = 31): the same list without "desde".
const DATE_CONTEXT_09: [&[u8]; 31] = [
    b"a.c",
    b"ac",
    b"antes de cristo",
    b"ayer",
    b"a\xf1o",
    b"cumplea\xf1os",
    b"d.c",
    b"dc",
    b"despues de cristo",
    b"domingo",
    b"duraci\xf3n",
    b"era",
    b"\xe9poca",
    b"fecha",
    b"feriada",
    b"feriado",
    b"fiesta",
    b"hoy",
    b"jornada",
    b"jueves",
    b"lunes",
    b"ma\xf1ana",
    b"martes",
    b"mi\xe9rcoles",
    b"periodo",
    b"per\xedodo",
    b"periodos",
    b"per\xedodos",
    b"s\xe1bado",
    b"siglo",
    b"viernes",
];

/// `PTR_DAT_1006a750` (`DAT_10079dd4` = 32): words that make a bare number
/// or weekday a date (`FUN_1002d2b0`).
const DATE_CONTEXT: [&[u8]; 32] = [
    b"a.c",
    b"ac",
    b"antes de cristo",
    b"ayer",
    b"a\xf1o",
    b"cumplea\xf1os",
    b"d.c",
    b"dc",
    b"desde",
    b"despues de cristo",
    b"domingo",
    b"duraci\xf3n",
    b"era",
    b"\xe9poca",
    b"fecha",
    b"feriada",
    b"feriado",
    b"fiesta",
    b"hoy",
    b"jornada",
    b"jueves",
    b"lunes",
    b"ma\xf1ana",
    b"martes",
    b"mi\xe9rcoles",
    b"periodo",
    b"per\xedodo",
    b"periodos",
    b"per\xedodos",
    b"s\xe1bado",
    b"siglo",
    b"viernes",
];

/// `PTR_s_WNOMON_10069098` (`DAT_100690e8` = 20).
const PATTERNS: [&[u8]; 20] = [
    b"WNOMON", b"WTNOM", b"NOMON", b"WNMN", b"WNOM", b"WMNN", b"NMN", b"NOM", b"MNN", b"NNN", b"WNM", b"NM", b"IN",
    b"MN", b"NN", b"SN", b"NS", b"M", b"W", b"N",
];

fn digit(c: u8) -> bool {
    CLS_S[c as usize] & 0x10 != 0
}

/// `FUN_1004b600`.
fn all_digits(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&c| digit(c))
}

fn eqi(a: &[u8], b: &[u8]) -> bool {
    stricmp_spa(a, b) == 0
}

/// `FUN_100505b3`: C `atoi`.
fn atoi(s: &[u8]) -> i32 {
    let mut i = 0;
    while i < s.len() && matches!(s[i], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        i += 1;
    }
    let neg = i < s.len() && s[i] == b'-';
    if i < s.len() && (s[i] == b'-' || s[i] == b'+') {
        i += 1;
    }
    let mut v: i32 = 0;
    while i < s.len() && s[i].is_ascii_digit() {
        v = v.wrapping_mul(10).wrapping_add((s[i] - b'0') as i32);
        i += 1;
    }
    if neg {
        v.wrapping_neg()
    } else {
        v
    }
}

/// `FUN_1002da80`: a day, 1..31 in one or two digits.
fn valid_day(s: &[u8]) -> i32 {
    if !all_digits(s) || s.len() > 2 {
        return -1;
    }
    let v = atoi(s);
    if v > 0 && v < 0x20 {
        v
    } else {
        -1
    }
}

/// `FUN_1002dad0`: a month number, 1..12 in one or two digits.
fn valid_month(s: &[u8]) -> i32 {
    if !all_digits(s) || s.len() > 2 {
        return -1;
    }
    let v = atoi(s);
    if v > 0 && v < 13 {
        v
    } else {
        -1
    }
}

/// `FUN_1002db30(s, 0)`: a year of 2, 3 or 4 digits, or 5+ digits from
/// 10000 without a leading zero.
fn valid_year(s: &[u8]) -> i32 {
    let n = s.len();
    let v = atoi(s);
    if !all_digits(s) || n < 2 {
        return -1;
    }
    match n {
        2 if (0..100).contains(&v) => return v,
        3 if (100..1000).contains(&v) => return v,
        4 if v > 999 => return v,
        _ => {}
    }
    if s[0] == b'0' || v < 10000 {
        -1
    } else {
        v
    }
}

/// `FUN_1002d9b0`: a day with an ordinal suffix ("1ro", "8vo").
fn ordinal_day(s: &[u8]) -> i32 {
    let n = s.len();
    if n == 0 || n >= 5 {
        return -1;
    }
    let Some((code, _)) = ordinal_suffix_classify(s) else { return -1 };
    let strip = match code {
        1 | 2 => 1,
        3 | 4 => 2,
        5 | 6 => 3,
        _ => return -1,
    };
    if n < strip {
        return -1;
    }
    let v = atoi(&s[..n - strip]);
    if v > 0 && v < 0x20 {
        v
    } else {
        -1
    }
}

/// `FUN_1002d8b0`: digits (a leading apostrophe allowed), or an ordinal day.
fn number_gate(s: &[u8]) -> bool {
    let n = s.len();
    for (i, &c) in s.iter().enumerate() {
        if !digit(c) && (i != 0 || n < 2 || s[0] != b'\'') {
            return ordinal_day(s) >= 0;
        }
    }
    true
}

/// `FUN_1002d910`: "'97" or a four-digit year starting 1-9.
fn year_form(s: &[u8]) -> i32 {
    if s.first() == Some(&b'\'') {
        if s.len() == 3 && digit(s[1]) && digit(s[2]) {
            return atoi(&s[1..]);
        }
        return -1;
    }
    if s.len() == 4 && (b'1'..=b'9').contains(&s[0]) && digit(s[1]) && digit(s[2]) && digit(s[3]) {
        return atoi(s);
    }
    -1
}

/// `FUN_100259c0`: lowercase fold (ASCII, and Latin-1 uppercase letters).
fn fold_lower(text: &[u8]) -> Vec<u8> {
    text.iter()
        .map(|&b| {
            if b < 0x80 || HI[b as usize] & 0x80 == 0 {
                if CLS_S[b as usize] & 0x80 != 0 {
                    b + 0x20
                } else {
                    b
                }
            } else if matches!(b, 0x8a | 0x8e | 0x9f) {
                b + 0x10
            } else {
                b.wrapping_add(0x20)
            }
        })
        .collect()
}

/// `FUN_100437a0` / `FUN_10043680` in mode 'I'.
fn search<T>(table: &[T], key: impl Fn(&T) -> &[u8], word: &[u8]) -> Option<usize> {
    if word.is_empty() {
        return None;
    }
    let (mut lo, mut hi) = (0i32, table.len() as i32 - 1);
    while lo <= hi {
        let mid = (lo + hi) / 2;
        let c = stricmp_spa(key(&table[mid as usize]), word);
        if c == 0 {
            return Some(mid as usize);
        }
        if c < 0 {
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    None
}

fn c0(tok: &Tok) -> u8 {
    tok.text[0]
}

/// `FUN_1002dc20(tok, t+p, off)`: "27, 28 y 29"; the bytes to the last day.
fn day_list(tok: &Tok, t: &[u8], p: usize, off: i32) -> i32 {
    if tok.class != 2 || valid_day(tok.text()) < 0 {
        return -1;
    }
    let mut i = tok.consumed;
    let (nt, c) = tokenize(t, p + i as usize, off + i, 3);
    if c == 0 || c0(&nt) != b',' || nt.len != 1 || nt.ws >= 2 {
        return -1;
    }
    let mut state = 1u8;
    i += c;
    if t[p + i as usize] == 0 {
        return -1;
    }
    loop {
        let (nt, c) = tokenize(t, p + i as usize, off + i, 3);
        if c == 0 {
            return -1;
        }
        if state != 0 && valid_day(nt.text()) >= 0 && nt.ws <= 1 {
            if state == 2 {
                return i + c;
            }
            state = 0;
        } else {
            if state != 0 || (c0(&nt) != b',' && c0(&nt) != b'y') || nt.len != 1 || nt.ws > 1 {
                return -1;
            }
            state = if c0(&nt) != b',' { 2 } else { 1 };
        }
        i += c;
        if t[p + i as usize] == 0 {
            return -1;
        }
    }
}

fn word_conj(nt: &Tok) -> bool {
    ((matches!(c0(nt), b'a' | b'y' | b'&') && nt.len == 1) || eqi(nt.text(), b"al")) && nt.ws < 2
}

fn dash(c: u8) -> bool {
    matches!(c, b'-' | b'/' | 0x96 | 0x97)
}

/// `FUN_1002dd40(tok, t+p, off, &year, &day)`: "27 al 30", "27-30",
/// "1990-1995"; the bytes to the second day or year.
fn day_range(tok: &Tok, t: &[u8], p: usize, off: i32, year: &mut bool, day: &mut bool) -> i32 {
    if tok.class != 2 {
        return -1;
    }
    let at = |i: i32| tokenize(t, p + i as usize, off + i, 3);
    let mut i = tok.consumed;
    if valid_day(tok.text()) >= 0 {
        let (nt, c) = at(i);
        if c == 0 {
            return -1;
        }
        if word_conj(&nt) {
            i += c;
            let (n2, c2) = at(i);
            if c2 == 0 || valid_day(n2.text()) < 0 || n2.ws > 1 {
                return -1;
            }
            return i + c2;
        }
        let d = c0(&nt);
        if dash(d) && nt.len == 1 {
            i += c;
            let (n2, c2) = at(i);
            if c2 != 0 && valid_day(n2.text()) >= 0 && n2.ws < 2 {
                i += c2;
                let (n3, c3) = at(i);
                if n3.class == 3 && n3.len == 1 {
                    if c0(&n3) == d {
                        return -1;
                    }
                    if c0(&n3) != 0 && b"-/.".contains(&c0(&n3)) {
                        let (n4, _) = at(i + c3);
                        if n4.class == 2 && n4.ws < 2 {
                            return -1;
                        }
                    }
                }
                *day = true;
                return i;
            }
        }
        return -1;
    }
    if valid_year(tok.text()) < 0 {
        return -1;
    }
    let (nt, c) = at(i);
    if c == 0 {
        return -1;
    }
    if word_conj(&nt) {
        i += c;
        let (n2, c2) = at(i);
        if c2 == 0 || valid_year(n2.text()) < 0 || n2.ws > 1 {
            return -1;
        }
        *year = true;
        return i + c2;
    }
    let d = c0(&nt);
    if !dash(d) || nt.len != 1 {
        return -1;
    }
    i += c;
    let (n2, c2) = at(i);
    if c2 == 0 || valid_year(n2.text()) < 0 || n2.ws > 1 {
        return -1;
    }
    i += c2;
    let (n3, _) = at(i);
    if n3.class == 3 && n3.len == 1 && c0(&n3) == d {
        return -1;
    }
    *year = true;
    i
}

/// `FUN_1002d2b0(t+p0, off0, 'B', consumed)`: a date context word among the
/// two words before or the three tokens after, unless the word right before
/// is "en".
fn date_context(t: &[u8], p0: usize, off0: i32, consumed: i32, era09: bool) -> bool {
    let mut left: Vec<Vec<u8>> = Vec::new();
    let (w, back) = prev_words(t, p0 - off0 as usize, p0, 2);
    if back != 0 {
        let (best, last) = best_and_last(&w, off0 - back as i32, 3);
        if last.len > 0 {
            left.push(last.text().to_vec());
            if best.len > 0 {
                left.push(best.text().to_vec());
            }
        }
    }
    let mut right: Vec<Vec<u8>> = Vec::new();
    let mut q = p0 + consumed as usize;
    let mut o = off0 + consumed;
    while right.len() < 3 {
        let (tk, c) = tokenize(t, q, o, 3);
        if c == 0 || tk.class == 9 || tk.class == 10 {
            break;
        }
        q += c as usize;
        o += c;
        right.push(tk.text().to_vec());
    }
    if left.first().is_some_and(|w| eqi(w, b"en")) {
        return false;
    }
    let table: &[&[u8]] = if era09 { &DATE_CONTEXT_09 } else { &DATE_CONTEXT };
    phrase_find(table, &left, 2) || phrase_find(table, &right, 3)
}

/// One entry (0x140 bytes each in the WA scratch area).
#[derive(Clone, Default)]
struct Ent {
    ws: i32,
    start: i32,
    end: i32,
    len: i32,
    class: i32,
    role: u8,
    /// +0x1e: a '.' was absorbed.
    dot: i16,
    /// +0x20: the separator that followed.
    sep: i16,
    /// +0x24: text position after the entry ("DAYS": its start).
    pos: usize,
    text: Vec<u8>,
    word: Vec<u8>,
}

/// `FUN_1002c8e0(entries, shape, pattern, context)`.
fn resolve(e: &mut [Ent], shape: &mut [u8], k: usize, ctx: bool) -> bool {
    let mut open = 0;
    for (i, s) in shape.iter_mut().enumerate().take(e.len()) {
        if *s == b'N' {
            if e[i].role == b'N' {
                open += 1;
            } else {
                *s = e[i].role;
            }
        }
    }
    let day = |e: &[Ent], i: usize| valid_day(&e[i].text) >= 0;
    let mon = |e: &[Ent], i: usize| valid_month(&e[i].text) >= 0;
    let year = |e: &[Ent], i: usize| valid_year(&e[i].text) >= 0;
    // `iVar2 == 2` day+year at (a, b), `iVar2 == 1` whichever is still open.
    let dy = |e: &mut [Ent], shape: &[u8], a: usize, b: usize| -> Option<bool> {
        if open == 2 {
            if day(e, a) {
                if !year(e, b) {
                    return Some(false);
                }
                e[a].role = b'D';
                e[b].role = b'Y';
                return Some(true);
            }
            None
        } else if open == 1 {
            if (shape[a] == b'D' && year(e, b)) || (shape[b] == b'Y' && day(e, a)) {
                e[a].role = b'D';
                e[b].role = b'Y';
                return Some(true);
            }
            None
        } else {
            Some(open < 1)
        }
    };
    match k {
        0 => {
            if open == 2 {
                if day(e, 1) && year(e, 5) {
                    e[1].role = b'D';
                    e[5].role = b'Y';
                    return true;
                }
                false
            } else if open == 1 {
                if (shape[1] == b'D' && year(e, 5)) || (shape[5] == b'Y' && day(e, 1)) {
                    e[1].role = b'D';
                    e[5].role = b'Y';
                    return true;
                }
                false
            } else {
                open < 1
            }
        }
        1 => {
            if open == 1 {
                if !day(e, 2) {
                    return false;
                }
                e[2].role = b'D';
                return true;
            }
            open < 1
        }
        2 => dy(e, shape, 0, 4).unwrap_or(false),
        3 => dy(e, shape, 1, 3).unwrap_or(false),
        5 => dy(e, shape, 2, 3).unwrap_or(false),
        6 => {
            if open == 2 {
                if day(e, 0) && year(e, 2) {
                    e[0].role = b'D';
                    e[2].role = b'Y';
                    return true;
                }
                if year(e, 0) {
                    if !day(e, 2) {
                        return false;
                    }
                    e[0].role = b'Y';
                    e[2].role = b'D';
                    return true;
                }
                false
            } else if open == 1 {
                if (shape[0] == b'D' && year(e, 2)) || (shape[2] == b'Y' && day(e, 0)) {
                    e[0].role = b'D';
                    e[2].role = b'Y';
                    return true;
                }
                if (shape[0] == b'Y' && day(e, 2)) || (shape[2] == b'D' && year(e, 0)) {
                    e[0].role = b'Y';
                    e[2].role = b'D';
                    return true;
                }
                false
            } else {
                open < 1
            }
        }
        7 | 0xb => {
            if open != 1 {
                return open < 1;
            }
            if day(e, 0) {
                e[0].role = b'D';
                return true;
            }
            false
        }
        8 => {
            if open == 2 {
                if !day(e, 1) {
                    if !year(e, 2) {
                        return false;
                    }
                    e[1].role = b'D';
                    e[2].role = b'Y';
                    return true;
                }
                false
            } else if open == 1 {
                if (shape[1] == b'D' && year(e, 2)) || (shape[2] == b'Y' && day(e, 1)) {
                    e[1].role = b'D';
                    e[2].role = b'Y';
                    return true;
                }
                false
            } else {
                open < 1
            }
        }
        9 => {
            if e[1].ws != e[2].ws {
                return false;
            }
            if !cascade(e, shape) {
                return false;
            }
            if ctx || !cascade(e, shape) {
                return true;
            }
            matches!(e[0].sep, 0x2f | 0x2d) || matches!(e[1].sep, 0x2f | 0x2d)
        }
        4 | 0xa => {
            if open != 1 {
                return open < 1;
            }
            if day(e, 1) {
                e[1].role = b'D';
                return true;
            }
            false
        }
        0xd => {
            if open != 1 {
                return open < 1;
            }
            if year(e, 1) {
                e[1].role = b'Y';
                return true;
            }
            if day(e, 1) {
                e[1].role = b'D';
                return true;
            }
            false
        }
        0xe => {
            if !ctx {
                return false;
            }
            let set = |e: &mut [Ent], a: u8, b: u8| {
                e[0].role = a;
                e[1].role = b;
                true
            };
            if open == 2 {
                if year(e, 0) && mon(e, 1) {
                    return set(e, b'Y', b'M');
                }
                if mon(e, 0) && year(e, 1) {
                    return set(e, b'M', b'Y');
                }
                if day(e, 0) && mon(e, 1) {
                    return set(e, b'D', b'M');
                }
                if mon(e, 0) && day(e, 1) {
                    return set(e, b'M', b'D');
                }
                false
            } else if open == 1 {
                let days = |e: &[Ent], i: usize| e[i].text.as_slice() == DAYS;
                if shape[0] == b'Y' && !days(e, 0) && mon(e, 1) {
                    return set(e, b'Y', b'M');
                }
                if year(e, 0) && shape[1] == b'M' {
                    return set(e, b'Y', b'M');
                }
                if (shape[0] == b'M' && year(e, 1)) || (mon(e, 0) && shape[1] == b'Y' && !days(e, 1)) {
                    return set(e, b'M', b'Y');
                }
                if (shape[0] == b'D' && !days(e, 0) && mon(e, 1)) || (day(e, 0) && shape[1] == b'M') {
                    return set(e, b'D', b'M');
                }
                if !(shape[0] == b'M' && day(e, 1)) && !(mon(e, 0) && shape[1] == b'D' && !days(e, 1)) {
                    return false;
                }
                set(e, b'M', b'D')
            } else {
                open < 1 && e[0].role != e[1].role
            }
        }
        0xc | 0xf => {
            if open == 1 {
                if !year(e, 1) {
                    return false;
                }
                e[1].role = b'Y';
                return true;
            }
            open < 1
        }
        0x10 => {
            if open != 1 {
                return open < 1;
            }
            if year(e, 0) {
                e[0].role = b'Y';
                return true;
            }
            false
        }
        0x11 | 0x12 => {
            let abbr = e[0].role == if k == 0x11 { b'A' } else { b'w' };
            !abbr || (e[0].len > 2 && ctx && e[0].dot != 0)
        }
        0x13 => ctx && shape[0] == b'Y',
        _ => false,
    }
}

/// `FUN_1002d4c0`: D/M/Y for three numbers, trying D-M-Y, Y-M-D, then M-D-Y.
fn cascade(e: &mut [Ent], s: &[u8]) -> bool {
    let (t0, t1, t2) = (e[0].text.clone(), e[1].text.clone(), e[2].text.clone());
    let set = |e: &mut [Ent], a: u8, b: u8, c: u8| {
        e[0].role = a;
        e[1].role = b;
        e[2].role = c;
        true
    };
    let (d0, m1, y2) = (valid_day(&t0) >= 0, valid_month(&t1) >= 0, valid_year(&t2) >= 0);
    if s[0] == b'D' && ((m1 && y2) || (m1 && s[2] == b'Y') || (s[1] == b'M' && y2)) {
        return set(e, b'D', b'M', b'Y');
    }
    if (d0 && s[1] == b'M' && s[2] == b'Y') || (d0 && s[1] == b'M' && y2) || (d0 && m1 && s[2] == b'Y') {
        return set(e, b'D', b'M', b'Y');
    }
    let (y0, d2) = (valid_year(&t0) >= 0, valid_day(&t2) >= 0);
    if s[0] == b'Y' && ((m1 && d2) || (m1 && s[2] == b'D') || (s[1] == b'M' && d2)) {
        return set(e, b'Y', b'M', b'D');
    }
    if (y0 && s[1] == b'M' && s[2] == b'D') || (y0 && s[1] == b'M' && d2) || (y0 && m1 && s[2] == b'D') {
        return set(e, b'Y', b'M', b'D');
    }
    let (d1, m0) = (valid_day(&t1) >= 0, valid_month(&t0) >= 0);
    if s[0] == b'M' && ((d1 && y2) || (d1 && s[2] == b'Y') || (s[1] == b'D' && y2)) {
        return set(e, b'M', b'D', b'Y');
    }
    if (m0 && s[1] == b'D' && s[2] == b'Y') || (m0 && s[1] == b'D' && y2) || (m0 && d1 && s[2] == b'Y') {
        return set(e, b'M', b'D', b'Y');
    }
    if d0 && m1 && y2 {
        return set(e, b'D', b'M', b'Y');
    }
    if y0 && m1 && d2 {
        return set(e, b'Y', b'M', b'D');
    }
    if m0 && d1 && y2 {
        return set(e, b'M', b'D', b'Y');
    }
    false
}

/// `FUN_1002e0c0`: a year, "dos mil" prefixed below 10.
fn year_words(out: &mut Vec<u8>, text: &[u8]) {
    let v = atoi(text);
    if v < 0 {
        return;
    }
    if v == 0 {
        out.extend_from_slice(b"dos mil");
        return;
    }
    if v < 10 {
        let mut w = Vec::new();
        cardinal_to_words(&mut w, text, 0);
        out.extend_from_slice(b"dos mil ");
        out.extend_from_slice(&w);
        return;
    }
    cardinal_to_words(out, text, 0);
}

/// `FUN_1002bf10`'s day: "primero" for 1, else the cardinal.
fn day_words(out: &mut Vec<u8>, text: &[u8]) {
    if atoi(text) == 1 {
        ordinal_word_1_20(out, 1, 0);
    } else {
        cardinal_to_words(out, text, 0);
    }
}

fn month_words(e: &Ent) -> Vec<u8> {
    if e.class == 1 {
        return e.word.clone();
    }
    let v = atoi(&e.text);
    MONTH_WORD.get(v as usize).copied().unwrap_or(b"").to_vec()
}

/// `FUN_1002c6e0(wa, entry, 0)`: a "DAYS" entry, one word per token.
fn emit_days(wa: &mut WaSpa, t: &[u8], e: &Ent) -> bool {
    let span = e.end - e.start;
    let mut pos = e.pos;
    let mut o = e.start;
    let mut done = 0;
    while done < span {
        let (sub, r) = tokenize(t, pos, o, 3);
        let ok = match sub.class {
            2 => {
                let mut w = Vec::new();
                if e.role == b'D' && atoi(sub.text()) == 1 {
                    ordinal_word_1_20(&mut w, 1, 0);
                } else {
                    cardinal_to_words(&mut w, sub.text(), 0);
                }
                emit_split(wa, &w, sub.start, sub.end, b'D', b'D', SRC)
            }
            1 => emit(wa, sub.text(), sub.start, sub.end, b'A', b'D', SRC, None),
            3 => match c0(&sub) {
                b'-' => emit(wa, b"a", sub.start, sub.end, b'A', b'D', SRC, None),
                b'/' | b'&' => emit(wa, b"y", sub.start, sub.end, b'A', b'D', SRC, None),
                _ => true,
            },
            6 if matches!(c0(&sub), 0x96 | 0x97) => emit(wa, b"a", sub.start, sub.end, b'A', b'D', SRC, None),
            _ => true,
        };
        if !ok {
            return false;
        }
        if r == 0 {
            break;
        }
        done += r;
        pos += r as usize;
        o += r;
    }
    true
}

/// `LAB_1002c2e6`: day, month and year over the whole span, joined by "de".
fn emit_joined(wa: &mut WaSpa, e: &mut [Ent]) -> i32 {
    let (mut dw, mut mw, mut yw) = (Vec::new(), Vec::new(), Vec::new());
    for x in e.iter_mut() {
        match x.role {
            b'D' => day_words(&mut dw, &x.text),
            b'M' | b'A' | b'F' => {
                x.role = b'M';
                mw = month_words(x);
            }
            b'Y' => {
                yw.clear();
                year_words(&mut yw, &x.text);
            }
            _ => return -1,
        }
    }
    let (start, end) = (e[0].start, e[e.len() - 1].end);
    for w in [&dw, &mw] {
        if !w.is_empty() && (!emit_split(wa, w, start, end, b'D', b'D', SRC) || !emit(wa, DE, start, end, b'A', b'D', SRC, None)) {
            return 0;
        }
    }
    if !yw.is_empty() && !emit_split(wa, &yw, start, end, b'D', b'D', SRC) {
        return 0;
    }
    1
}

/// `switchD_1002bf65_caseD_7` (`with_de` false) and `caseD_a` (true: "de"
/// after every entry but the last and weekdays).
fn emit_each(wa: &mut WaSpa, t: &[u8], e: &[Ent], with_de: bool) -> i32 {
    let n = e.len();
    // `local_200` is never cleared between entries: a second day appends.
    let mut dw = Vec::new();
    for (i, x) in e.iter().enumerate() {
        let ok = if x.text == DAYS && matches!(x.role, b'D' | b'Y') {
            emit_days(wa, t, x)
        } else {
            match x.role {
                b'D' => {
                    day_words(&mut dw, &x.text);
                    emit_split(wa, &dw, x.start, x.end, b'D', b'D', SRC)
                }
                b'M' | b'A' | b'F' => {
                    let origin = if x.class == 1 { b'A' } else { b'D' };
                    emit(wa, &month_words(x), x.start, x.end, origin, b'D', SRC, None)
                }
                b'Y' => {
                    let mut w = Vec::new();
                    year_words(&mut w, &x.text);
                    emit_split(wa, &w, x.start, x.end, b'D', b'D', SRC)
                }
                b'w' | b'W' if with_de => emit_split(wa, &x.word, x.start, x.end, b'D', b'D', SRC),
                b'w' | b'W' | b'S' if !with_de => emit(wa, &x.word, x.start, x.end, b'A', b'D', SRC, None),
                b'I' | b'O' | b'T' if !with_de => emit(wa, &x.text, x.start, x.end, b'A', b'D', SRC, None),
                _ => return -1,
            }
        };
        if !ok {
            return 0;
        }
        if with_de && i != n - 1 && !matches!(x.role, b'w' | b'W') && !emit(wa, DE, x.start, x.end, b'A', b'D', SRC, None) {
            return 0;
        }
    }
    1
}

/// `FUN_1002bf10(wa, entries, pattern)`.
fn emit_date(wa: &mut WaSpa, t: &[u8], e: &mut [Ent], k: usize) -> i32 {
    let plain = |e: &[Ent], idx: &[usize]| idx.iter().all(|&i| e[i].text != DAYS);
    let with_de = match k {
        6 => {
            if plain(e, &[0, 2]) {
                return emit_joined(wa, e);
            }
            e[0].role == b'D'
        }
        9 => {
            if plain(e, &[0, 1, 2]) {
                return emit_joined(wa, e);
            }
            e[0].role == b'D' && e[1].role == b'M' && e[2].role == b'Y'
        }
        0xa | 0xb => true,
        0xe => {
            if plain(e, &[0, 1]) {
                return emit_joined(wa, e);
            }
            (e[0].role == b'M' && e[1].role == b'Y') || (e[0].role == b'D' && e[1].role == b'M')
        }
        _ => false,
    };
    emit_each(wa, t, e, with_de)
}

/// `FUN_1002b410(wa, tok, t + p, off)`.
pub fn date(fe: &FeSpa, wa: &mut WaSpa, tok: &mut Tok, t: &[u8], p: usize, off: i32) -> i32 {
    let mut e: Vec<Ent> = vec![Ent::default(); 7];
    let mut shape = [0u8; 8];
    let mut roles = [0u8; 32];
    let (mut n, mut k) = (0usize, 0usize);
    let mut pending: i32 = -1;
    let mut seps = 0;
    let (mut year_flag, mut day_flag) = (false, false);
    let mut ctx = false;
    let (mut q, mut o) = (p, off);

    loop {
        let (cur, used) = if k < 1 {
            (tok.clone(), tok.consumed)
        } else {
            let (tk, c) = tokenize(t, q, o, 3);
            if c == 0 || tk.class == 10 {
                break;
            }
            (tk, c)
        };
        let text = cur.text().to_vec();
        let ch = cur.text[0];

        if pending == -1 && cur.len == 1 && ch != 0 && b"/-.,".contains(&ch) {
            q += used as usize;
            o += used;
            let (peek, _) = tokenize(t, q, o, 3);
            let mut absorb = false;
            if ch == b'.' && cur.ws == 0 && k > 0 && matches!(roles[k - 1], b'A' | b'w') && peek.len > 0 {
                let tail = |pk: &Tok| pk.ws >= 0 && pk.ws <= 1 && pk.len == 1 && matches!(pk.text[0], b',' | b'/');
                absorb = if peek.ws < 1 {
                    tail(&peek)
                } else if peek.ws > 1 || !number_gate(peek.text()) {
                    (peek.ws < 2 && peek.text() == EL) || tail(&peek)
                } else {
                    true
                };
            }
            if absorb {
                roles[k] = ch;
                e[n - 1].end += 1;
                e[n - 1].len += 1;
                e[n - 1].dot = 1;
            } else {
                if k == 0 || seps > 2 {
                    break;
                }
                roles[k] = ch;
                seps += 1;
                e[n - 1].sep = ch as i8 as i16;
                pending = k as i32;
            }
            k += 1;
            if n > 5 {
                break;
            }
            continue;
        }

        if n > 0
            && shape[0] == b'W'
            && pending == -1
            && ch == b','
            && cur.len == 1
            && cur.ws < 2
            && k > 0
            && matches!(roles[k - 1], b'.' | b'W' | b'w')
        {
            roles[k] = b',';
            q += used as usize;
            o += used;
            if roles[k - 1] == b'.' {
                e[n - 1].end += 1;
                e[n - 1].len += 1;
            }
            k += 1;
            if n > 5 {
                break;
            }
            continue;
        }

        let mut word: Vec<u8> = Vec::new();
        let mut text = text;
        let role: u8;
        let sh: u8;
        if eqi(DE, &text) || eqi(b"del", &text) {
            role = b'O';
            sh = b'O';
        } else if eqi(EL, &text) {
            role = b'T';
            sh = b'T';
        } else if k == 0 && eqi(b"en", &text) {
            role = b'I';
            sh = b'I';
        } else if [&b"AC"[..], b"A.C", b"DC", b"D.C"].iter().any(|w| eqi(w, &text)) {
            role = b'S';
            sh = b'S';
            word = if ch == b'A' { b"AC".to_vec() } else { b"DC".to_vec() };
        } else if let Some(i) = search(&WEEKDAY_ABBR, |x| x.0, &text) {
            role = b'w';
            sh = b'W';
            word = WEEKDAY_ABBR[i].1.to_vec();
        } else if let Some(i) = search(&WEEKDAY, |x| x, &text) {
            role = b'W';
            sh = b'W';
            word = WEEKDAY[i].to_vec();
        } else {
            let abbr = search(&MONTH_ABBR, |x| x.0, &text);
            let full = if abbr.is_none() { search(&MONTH_FULL, |x| x, &text) } else { None };
            if abbr.is_some() || full.is_some() {
                if pending >= 0 {
                    if pending + 1 != k as i32 {
                        break;
                    }
                    pending = -1;
                }
                sh = b'M';
                if let Some(i) = abbr {
                    role = b'A';
                    word = MONTH_WORD[atoi(MONTH_ABBR[i].1) as usize].to_vec();
                } else {
                    role = b'F';
                    word = fold_lower(&text);
                }
            } else {
                if !day_flag && cur.class == 2 && all_digits(&text) {
                    let mut r = day_list(&cur, t, q, o);
                    if r < 0 {
                        r = day_range(&cur, t, q, o, &mut year_flag, &mut day_flag);
                    }
                    if r >= 0 {
                        let after_range = day_flag && n >= 1 && shape[n - 1] == b'N' && matches!(e[n - 1].sep, 0x2d | 0x2f);
                        if !after_range {
                            let x = &mut e[n];
                            x.start = o;
                            x.ws = cur.ws;
                            x.end = o + r;
                            x.len = r - cur.ws;
                            x.class = 2;
                            x.text = DAYS.to_vec();
                            x.role = if year_flag { b'Y' } else { b'D' };
                            roles[k] = x.role;
                            x.pos = q;
                            shape[n] = b'N';
                            q += r as usize;
                            o += r;
                            n += 1;
                            k += 1;
                            year_flag = false;
                        }
                        if n > 5 {
                            break;
                        }
                        continue;
                    }
                }
                if !number_gate(&text) {
                    break;
                }
                if pending >= 0 {
                    if pending + 1 != k as i32 {
                        break;
                    }
                    pending = -1;
                }
                sh = b'N';
                let y = year_form(&text);
                if y >= 0 {
                    role = b'Y';
                    text = y.to_string().into_bytes();
                } else {
                    let d = ordinal_day(&text);
                    if d >= 0 {
                        role = b'D';
                        text = d.to_string().into_bytes();
                    } else {
                        role = b'N';
                    }
                }
            }
        }

        roles[k] = role;
        shape[n] = sh;
        let end = o + used;
        let x = &mut e[n];
        x.start = cur.start;
        x.ws = cur.ws;
        x.end = end;
        x.len = cur.len;
        x.class = cur.class;
        x.role = role;
        q += used as usize;
        o += used;
        x.pos = q;
        if day_flag {
            day_flag = false;
        }
        x.text = text;
        if !word.is_empty() {
            x.word = word;
        }
        n += 1;
        k += 1;
        if n > 5 {
            break;
        }
    }

    while n >= 1 {
        let range = match n {
            6 => 0..1,
            5 => 1..3,
            4 => 3..6,
            3 => 6..11,
            2 => 11..17,
            1 => 17..20,
            _ => 0..0,
        };
        if let Some(pat) = range.into_iter().find(|&i| PATTERNS[i] == &shape[..n]) {
            let consumed = e[n - 1].end - off;
            if matches!(pat, 9 | 0xe | 0x11 | 0x12 | 0x13) && date_context(t, p, off, consumed, fe.era.is_2009()) {
                ctx = true;
            }
            let (ents, sh) = (&mut e[..n], &mut shape[..n]);
            if resolve(ents, sh, pat, ctx) {
                let r = emit_date(wa, t, ents, pat);
                return if r < 1 { r } else { consumed };
            }
        }
        shape[n - 1] = 0;
        n -= 1;
    }
    -1
}
