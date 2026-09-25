//! Number-to-words core shared by every number normalizer. Words are built in
//! a C string buffer the way the engine builds them: the first word is copied,
//! later ones are appended with `" %s"` (the WA emitter then splits on spaces).

use super::cstr::*;

pub type Buf = [u8; 256];

pub fn new_buf() -> Buf {
    [0u8; 256]
}

/// `sprintf(end, " %s", w)` when the buffer is non-empty, else copy.
pub fn append(buf: &mut [u8], w: &[u8]) {
    if buf[0] != 0 {
        let l = strlen(buf);
        buf[l] = b' ';
        put_str(buf, l + 1, w);
    } else {
        strcpy(buf, w);
    }
}

fn unit_word(v: i32) -> &'static [u8] {
    s(match v {
        1 => 0x10078a64,
        2 => 0x10078850,
        3 => 0x1007887c,
        4 => 0x10078994,
        5 => 0x100789b4,
        6 => 0x100788e0,
        7 => 0x10078904,
        8 => 0x100789f8,
        9 => 0x10078958,
        10 => 0x100788c4,
        11 => 0x100789dc,
        12 => 0x10078868,
        13 => 0x100788b0,
        14 => 0x10078988,
        15 => 0x100789d4,
        16 => 0x100788d8,
        17 => 0x100788f8,
        18 => 0x100789ec,
        19 => 0x1007894c,
        _ => 0x1009f948,
    })
}

fn tens_word(t: i32) -> &'static [u8] {
    s(match t {
        2 => 0x10078854,
        3 => 0x1007889c,
        4 => 0x1007899c,
        5 => 0x100789c4,
        6 => 0x100788c8,
        7 => 0x100788f0,
        8 => 0x100789e4,
        9 => 0x10078944,
        _ => 0x1009f948,
    })
}

/// Units with a leading space, used after a tens word (" one" .. " nine").
fn sp_unit(u: i32) -> &'static [u8] {
    s(match u {
        1 => 0x1009c56c,
        2 => 0x1009c564,
        3 => 0x1009c55c,
        4 => 0x1009c554,
        5 => 0x1009c54c,
        6 => 0x1009c544,
        7 => 0x1009c53c,
        8 => 0x1009c534,
        9 => 0x1009c52c,
        _ => 0x1009f948,
    })
}

/// `FUN_1005fa60`: append the words for `v`. Mode `b'L'` reads up to 999,999,999;
/// mode `b'H'` reads the billions/trillions part of a longer number.
pub fn cardinal(buf: &mut [u8], v: i32, mode: u8) {
    if mode != b'L' {
        if v < 1000 {
            // FUN_10060120: "N hundred M billion".
            let (h, r) = (v / 100, v % 100);
            scale_hundred(buf, h, r);
            if h != 0 || r != 0 {
                strcat(buf, s(0x1009c58c));
            }
            return;
        }
        if v > 999_999 {
            return;
        }
        // FUN_10060180
        let (th, r) = (v / 1000, v % 1000);
        cardinal(buf, th, b'L');
        if th != 0 {
            strcat(buf, s(0x1009c598));
        }
        cardinal(buf, r, b'H');
        return;
    }
    let mut w = [0u8; 32];
    if v > 0x13 {
        if v > 99 {
            if v < 1000 {
                scale_hundred(buf, v / 100, v % 100);
                return;
            }
            if v > 999_999 {
                if v > 999_999_999 {
                    return;
                }
                let (m, r) = (v / 1_000_000, v % 1_000_000);
                cardinal(buf, m, b'L');
                if m != 0 {
                    strcat(buf, s(0x1009c580));
                }
                cardinal(buf, r, b'L');
                return;
            }
            let (th, r) = (v / 1000, v % 1000);
            cardinal(buf, th, b'L');
            if th != 0 {
                strcat(buf, s(0x1009c574));
            }
            cardinal(buf, r, b'L');
            return;
        }
        strcpy(&mut w, tens_word(v / 10));
        if v % 10 != 0 {
            strcat(&mut w, sp_unit(v % 10));
        }
    } else {
        if v == 0 {
            return;
        }
        strcpy(&mut w, unit_word(v));
        if w[0] == 0 {
            return;
        }
    }
    append(buf, &w);
}

/// `FUN_10060000`: "N hundred" then the remainder.
fn scale_hundred(buf: &mut [u8], h: i32, r: i32) {
    cardinal(buf, h, b'L');
    if h != 0 {
        strcat(buf, s(0x100802dc));
    }
    if r != 0 {
        cardinal(buf, r, b'L');
    }
}

/// `FUN_10061aa0`: keep only the digits.
pub fn digits_only(src: &[u8]) -> Vec<u8> {
    cs(src).iter().copied().filter(|&c| cls(c) & 0x10 != 0).collect()
}

/// `FUN_100611c0`: a number string (commas and a decimal point allowed).
/// `plain` = 0 lets four-digit numbers be read as years.
pub fn number(buf: &mut [u8], src: &[u8], plain: i16) {
    buf[0] = 0;
    let src = cs(src);
    if plain == 0 && super::sbd::all_digit(src) && src.first().is_some_and(|&c| c > b'0') && src.len() == 4 {
        year(buf, src);
        return;
    }
    if let Some(dot) = src.iter().position(|&c| c == b'.') {
        let ip = digits_only(&src[..dot]);
        let n = ip.len();
        if n < 0x10 {
            if n < 10 {
                cardinal(buf, atoi(&ip), b'L');
            } else {
                cardinal(buf, atoi(&ip[..n - 9]), b'H');
                cardinal(buf, atoi(&ip[n - 9..]), b'L');
            }
        } else {
            digits(buf, &ip, 1, 0);
        }
        if n > 0 && buf[0] == 0 {
            strcpy(buf, s(0x10078848));
        }
        digits(buf, &src[dot..], 1, 1);
        return;
    }
    let d = digits_only(src);
    let n = d.len();
    if n > 0xf {
        digits(buf, &d, 1, 0);
        return;
    }
    if n < 10 {
        if n < 1 {
            return;
        }
        cardinal(buf, atoi(&d), b'L');
    } else {
        cardinal(buf, atoi(&d[..n - 9]), b'H');
        cardinal(buf, atoi(&d[n - 9..]), b'L');
    }
    if buf[0] == 0 {
        strcpy(buf, s(0x10078848));
    }
}

/// `FUN_10060fc0`: read digit by digit ("point" for '.'). A `0` digit says
/// "oh" when `zero` is 0 (clear), "zero" when `zero` is nonzero (set) — the
/// disassembly's `JZ` at `0x10061052` jumps to the "oh" load
/// (`s(0x1009c740)`) on the zero-flag-clear path; the pseudo-C Ghidra shows
/// for this switch case visually swaps the two branches (an empty `{}` for
/// the `param_4==0` arm and the "zero" load in the `else`), which reads
/// backwards from what the jump actually does. Confirmed against a live
/// capture: `FUN_10060fc0(buf, "220", 0, 0)` (zero=0, i.e. clear) produces
/// "two two oh", not "zero".
pub fn digits(buf: &mut [u8], src: &[u8], plain: i16, zero: i16) {
    let src = cs(src);
    if plain == 0 && src.len() == 4 && super::sbd::all_digit(src) && src[0] > b'0' {
        year(buf, src);
        return;
    }
    let mut w = [0u8; 16];
    for &c in src {
        match c {
            b'.' => strcpy(&mut w, s(0x1009b17c)),
            b'0' => strcpy(&mut w, s(if zero != 0 { 0x10078848 } else { 0x1009c740 })),
            b'1'..=b'9' => strcpy(&mut w, unit_word((c - b'0') as i32)),
            _ => w[0] = 0,
        }
        append(buf, &w);
    }
}

/// `FUN_10060e50`: dotted digit groups ("1.2.3", version-like): each group
/// read as a number (or digit by digit when it starts with 0), "point" between.
pub fn dotted(buf: &mut [u8], src: &[u8], plain: i16, zero: i16) {
    buf[0] = 0;
    let src = cs(src);
    let mut part = [0u8; 32];
    let mut k = 0usize;
    let mut i = 0usize;
    loop {
        if i == src.len() || src[i] == b'.' {
            part[k] = 0;
            let mut t = new_buf();
            if part[0] == b'0' {
                digits(&mut t, &part, plain, zero);
            } else {
                number(&mut t, &part, plain);
            }
            if i < src.len() && src[i] == b'.' {
                strcat(&mut t, s(0x1009c738));
            }
            append(buf, &t);
            k = 0;
        } else {
            let c = src[i];
            if cls(c) & 0x10 == 0 {
                buf[0] = 0;
                digits(buf, src, plain, zero);
                return;
            }
            part[k] = c;
            k += 1;
        }
        i += 1;
        if i > src.len() {
            return;
        }
    }
}

fn ordinal_unit(v: i32) -> &'static [u8] {
    s(match v {
        1 => 0x100789bc,
        2 => 0x10078918,
        3 => 0x100788bc,
        4 => 0x1009c6f4,
        5 => 0x1009c6ec,
        6 => 0x1009c6e4,
        7 => 0x1009c6dc,
        8 => 0x1009c6d4,
        9 => 0x1009c6cc,
        10 => 0x1009c6c4,
        11 => 0x1009c6b8,
        12 => 0x1009c6b0,
        13 => 0x1009c6a4,
        14 => 0x1009c698,
        15 => 0x1009c68c,
        16 => 0x1009c680,
        17 => 0x1009c674,
        18 => 0x1009c668,
        19 => 0x1009c65c,
        _ => 0x1009f948,
    })
}

fn ordinal_tens(t: i32) -> &'static [u8] {
    s(match t {
        2 => 0x1009c650,
        3 => 0x1009c644,
        4 => 0x1009c638,
        5 => 0x1009c62c,
        6 => 0x1009c620,
        7 => 0x1009c614,
        8 => 0x1009c608,
        9 => 0x1009c5fc,
        10 => 0x1009c5f0,
        _ => 0x1009f948,
    })
}

/// Ordinal units with a leading space (" first" .. " ninth").
fn sp_ordinal(u: i32) -> &'static [u8] {
    s(match u {
        1 => 0x1009c5e8,
        2 => 0x1009c5e0,
        3 => 0x1009c5d8,
        4 => 0x1009c5d0,
        5 => 0x1009c5c8,
        6 => 0x1009c5c0,
        7 => 0x1009c5b4,
        8 => 0x1009c5ac,
        9 => 0x1009c5a4,
        _ => 0x1009f948,
    })
}

/// `FUN_10060380`: ordinal words for `v`.
pub fn ordinal(buf: &mut [u8], v: i32, mode: u8, big: i32) {
    if mode != b'L' {
        if v < 1000 {
            // FUN_10060d20
            let (h, r) = (v / 100, v % 100);
            scale_hundred(buf, h, r);
            if h == 0 && r == 0 {
                return;
            }
            strcat(buf, s(if big < 1 { 0x1009c720 } else { 0x1009c58c }));
            return;
        }
        if v > 999_999 {
            return;
        }
        // FUN_10060d90
        let (th, r) = (v / 1000, v % 1000);
        if th == 0 {
            ordinal(buf, r, b'H', big);
            return;
        }
        cardinal(buf, th, b'L');
        if big != 0 || r != 0 {
            strcat(buf, s(0x1009c598));
            ordinal(buf, r, b'H', big);
            return;
        }
        strcat(buf, s(0x1009c72c));
        return;
    }
    let mut w = [0u8; 32];
    if v > 0x13 {
        if v > 99 {
            let scaled = |buf: &mut [u8], hi: i32, lo: i32, word: u32, wordth: u32| {
                if hi == 0 {
                    ordinal(buf, lo, b'L', 0);
                    return;
                }
                cardinal(buf, hi, b'L');
                if lo != 0 {
                    strcat(buf, s(word));
                    ordinal(buf, lo, b'L', 0);
                    return;
                }
                strcat(buf, s(wordth));
            };
            if v < 1000 {
                scaled(buf, v / 100, v % 100, 0x100802dc, 0x1009c6fc);
                return;
            }
            if v > 999_999 {
                if v > 999_999_999 {
                    return;
                }
                scaled(buf, v / 1_000_000, v % 1_000_000, 0x1009c580, 0x1009c714);
                return;
            }
            scaled(buf, v / 1000, v % 1000, 0x1009c574, 0x1009c708);
            return;
        }
        if v % 10 == 0 {
            strcpy(&mut w, ordinal_tens(v / 10));
        } else {
            strcpy(&mut w, tens_word(v / 10));
            strcat(&mut w, sp_ordinal(v % 10));
        }
    } else {
        if v == 0 {
            return;
        }
        strcpy(&mut w, ordinal_unit(v));
        if w[0] == 0 {
            return;
        }
    }
    append(buf, &w);
}

fn fmt_d(v: i32) -> Vec<u8> {
    format!("{}", v).into_bytes()
}

/// `FUN_10039d50`: a year ("nineteen ninety", "nineteen oh five", "nineteen
/// hundred", "two thousand five").
pub fn year(buf: &mut [u8], src: &[u8]) {
    buf[0] = 0;
    let v = atoi(src);
    if v < 0 {
        return;
    }
    if v == 0 {
        strcpy(buf, s(0x100802ec));
        return;
    }
    if v < 10 {
        strcpy(buf, s(0x100802ec));
        let mut t = new_buf();
        number(&mut t, &fmt_d(v), 1);
        append_raw(buf, &t);
        return;
    }
    if v < 100 || v >= 10000 {
        number(buf, &fmt_d(v), 1);
        return;
    }
    let r = v % 100;
    let hi = v / 100;
    let c = hi % 10;
    if c == 0 && r <= 9 {
        let txt = if v < 1000 { fmt_d(v) } else { format!("0{}", v).into_bytes() };
        number(buf, &txt, 1);
        return;
    }
    if hi > 0 && c != 0 && r == 0 {
        number(buf, &fmt_d(hi), 1);
        strcat(buf, s(0x100802dc));
        return;
    }
    number(buf, &fmt_d(hi), 1);
    let mut t = new_buf();
    number(&mut t, &fmt_d(r), 1);
    if r > 9 || r < 1 {
        append_raw(buf, &t);
    } else {
        let l = strlen(buf);
        let n = put_str(buf, l, b" oh ");
        put_str(buf, l + n, &t);
    }
}

/// `sprintf(end, " %s", w)` regardless of whether the buffer is empty.
pub fn append_raw(buf: &mut [u8], w: &[u8]) {
    let l = strlen(buf);
    buf[l] = b' ';
    put_str(buf, l + 1, w);
}

/// `FUN_10039410`: a year token (`1995` or `'95`) -> its value, else -1.
pub fn year_value(w: &[u8]) -> i32 {
    let w = cs(w);
    if at(w, 0) == b'\'' {
        let t = &w[1..];
        if t.len() == 2 && cls(t[0]) & 0x10 != 0 && cls(at(w, 2)) & 0x10 != 0 {
            return atoi(t);
        }
        return -1;
    }
    if w.len() == 4 && w[0] > b'0' && (w[0] as i8) < b':' as i8 && cls(w[1]) & 0x10 != 0 && cls(w[2]) & 0x10 != 0 && cls(w[3]) & 0x10 != 0 {
        return atoi(w);
    }
    -1
}

/// `FUN_10061890`: digits with optional thousands commas and decimal points.
pub fn numeric(w: &[u8]) -> bool {
    let w = cs(w);
    let n = w.len() as i32;
    if n == 0 {
        return false;
    }
    let d = |i: i32| cls(at(w, i as isize)) & 0x10 != 0;
    for i in 0..n {
        let c = w[i as usize];
        if cls(c) & 0x10 != 0 {
            continue;
        }
        if c == b',' && i >= 1 && i < n - 1 && d(i - 1) && d(i + 1) {
            continue;
        }
        if c != b'.' || i >= n - 1 {
            return false;
        }
        let ok = if i == 0 { d(1) } else { d(i - 1) && d(i + 1) };
        if !ok {
            return false;
        }
    }
    true
}

/// `FUN_100620e0`: split "1,234.5" into integer ("1234") and fraction ("5").
pub fn split_decimal(w: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let w = cs(w);
    match w.iter().position(|&c| c == b'.') {
        None => {
            let ip = if w.contains(&b',') { digits_only(w) } else { w.to_vec() };
            (ip, Vec::new())
        }
        Some(d) => {
            let ip = &w[..d];
            let ip = if ip.contains(&b',') { digits_only(ip) } else { ip.to_vec() };
            (ip, w[d + 1..].to_vec())
        }
    }
}

/// `FUN_10062040`: the number is not exactly 1 (plural units: "feet", "inches").
pub fn plural(w: &[u8]) -> bool {
    let w = cs(w);
    if w.is_empty() || !numeric(w) || w.len() >= 0x1e {
        return false;
    }
    if !w.contains(&b'.') {
        return atoi(&digits_only(w)) != 1;
    }
    let (ip, fp) = split_decimal(w);
    atoi(&ip) != 1 || atoi(&fp) != 0
}

/// `FUN_100601e0`: a numeral with a matching ordinal suffix ("21st", "12th", "1,000th").
pub fn ordinal_numeral(w: &[u8]) -> bool {
    let w = cs(w);
    let n = w.len();
    if n <= 2 {
        return false;
    }
    let suf = &w[n - 2..];
    let kind = if stricmp(suf, s(0x1007ffd4)) == 0 {
        1
    } else if stricmp(suf, s(0x1009c3bc)) == 0 {
        2
    } else if stricmp(suf, s(0x1009c3b8)) == 0 {
        3
    } else if stricmp(suf, s(0x10080b84)) == 0 {
        4
    } else {
        return false;
    };
    let num = &w[..n - 2];
    let m = num.len();
    for &c in num {
        if cls(c) & 0x10 == 0 && c != b',' && c != b'.' {
            return false;
        }
    }
    let last = at(num, m as isize - 1);
    let teen = m >= 2 && num[m - 2] == b'1';
    let matches = (last == b'1' && kind == 1)
        || (last == b'2' && kind == 2)
        || (last == b'3' && kind == 3)
        || ((last == b'0' || (b'4'..=b'9').contains(&last)) && kind == 4);
    if matches {
        let bad_teen = teen && ((last == b'1' && kind == 1) || (last == b'2' && kind == 2) || (last == b'3' && kind == 3));
        return !bad_teen;
    }
    teen && matches!(last, b'1' | b'2' | b'3') && kind == 4
}

/// `FUN_10061970`: a well-formed number: digits with correctly grouped
/// thousands commas, optional single decimal part, no leading zero.
pub fn well_formed(w: &[u8]) -> bool {
    let w = cs(w);
    if w.is_empty() {
        return false;
    }
    let first = w[0];
    let n = w.len();
    let mut ip: Vec<u8> = Vec::new();
    let mut fp: Vec<u8> = Vec::new();
    let mut in_frac = false;
    for (i, &c) in w.iter().enumerate() {
        if cls(c) & 0x10 == 0 && c != b',' && c != b'.' {
            return false;
        }
        if in_frac {
            if c == b',' || c == b'.' {
                return false;
            }
            fp.push(c);
        } else if c == b'.' {
            in_frac = true;
        } else {
            if i == 0 && first == b',' {
                return false;
            }
            ip.push(c);
        }
    }
    if in_frac && fp.is_empty() {
        return false;
    }
    let _ = n;
    if ip.len() > 1 && ip[0] == b'0' {
        return false;
    }
    let mut run = 0;
    let mut i = ip.len() as isize - 1;
    while i >= 0 {
        if w[i as usize] == b',' {
            if run != 3 {
                return false;
            }
            run = 0;
        } else {
            run += 1;
        }
        i -= 1;
    }
    true
}

/// `FUN_100100a0`: occurrences of `c`.
pub fn count_char(w: &[u8], c: u8) -> i32 {
    cs(w).iter().filter(|&&x| x == c).count() as i32
}

/// `FUN_100629d0`: one digit, or two digits read as a number / "oh N".
fn pair(buf: &mut [u8], src: &[u8]) {
    let src = cs(src);
    let mut w = [0u8; 32];
    if src.len() == 1 {
        match src[0] {
            b'1'..=b'9' => strcpy(&mut w, unit_word((src[0] - b'0') as i32)),
            _ => w[0] = 0,
        }
    } else if src.len() == 2 {
        let v = atoi(src);
        if v > 9 {
            let mut t = new_buf();
            number(&mut t, src, 1);
            strcpy(&mut w, &t[..strlen(&t).min(31)]);
        } else {
            let va = [0x1009c7fc, 0x1009c7f4, 0x1009c7ec, 0x1009c7e0, 0x1009c7d8, 0x1009c7d0, 0x1009c7c8, 0x1009c7bc, 0x1009c7b0, 0x1009c7a8];
            if (0..10).contains(&v) {
                strcpy(&mut w, s(va[v as usize]));
            }
        }
    }
    append(buf, &w);
}

/// `FUN_10062590`: digits read in pairs ("4500" forty five hundred, "4501"
/// forty five oh one, "12000" twelve thousand), longer strings digit by digit.
pub fn pairs(buf: &mut [u8], src: &[u8]) {
    buf[0] = 0;
    let src = cs(src);
    let n = src.len();
    let d = |i: usize| at(src, i as isize);
    if n < 3 {
        pair(buf, src);
        return;
    }
    if n == 3 {
        pair(buf, &src[..1]);
        if d(1) == b'0' && d(2) == b'0' {
            append(buf, s(0x10078980));
            return;
        }
        pair(buf, &src[1..]);
        return;
    }
    if n == 4 {
        let c0 = d(0);
        if (c0 as i8) > b'0' as i8 {
            if d(1) == b'0' && d(2) == b'0' && d(3) == b'0' {
                pair(buf, &src[..1]);
                strcat(buf, s(0x1009c574));
                return;
            }
            if (d(1) as i8) > b'0' as i8 && d(2) == b'0' && d(3) == b'0' {
                pair(buf, &src[..2]);
                strcat(buf, s(0x100802dc));
                return;
            }
            if d(1) == b'0' && d(2) == b'0' && (d(3) as i8) > b'0' as i8 {
                pair(buf, &src[..1]);
                strcat(buf, s(0x1009c574));
                pair(buf, &src[3..]);
                return;
            }
        }
        pair(buf, &src[..2]);
        pair(buf, &src[2..]);
        return;
    }
    if n == 5 && (d(0) as i8) >= b'1' as i8 && (d(1) as i8) >= b'0' as i8 && d(2) == b'0' && d(3) == b'0' && d(4) == b'0' {
        pair(buf, &src[..2]);
        strcat(buf, s(0x1009c574));
        return;
    }
    let mut w = [0u8; 16];
    for &c in src {
        match c {
            b'0' => strcpy(&mut w, s(0x1009c740)),
            b'1'..=b'9' => strcpy(&mut w, unit_word((c - b'0') as i32)),
            _ => {}
        }
        append(buf, &w);
    }
}

/// `FUN_10061450`: an ordinal numeral ("21st" twenty first, "1,000th"
/// one thousandth); longer than 15 digits it is read digit by digit plus the
/// spelled suffix (" S T").
pub fn ordinal_numeral_words(buf: &mut [u8], src: &[u8]) {
    buf[0] = 0;
    let src = cs(src);
    let n = src.len();
    let has_suffix = n >= 3 && {
        let suf = &src[n - 2..];
        [0x1007ffd4u32, 0x1009c3bc, 0x1009c3b8, 0x10080b84].iter().any(|&va| stricmp(suf, s(va)) == 0)
    };
    let num: &[u8] = if has_suffix { &src[..n - 2] } else { src };
    if !numeric(num) {
        return;
    }
    if let Some(dot) = num.iter().position(|&c| c == b'.') {
        let ip = digits_only(&num[..dot]);
        let m = ip.len();
        if m < 0x10 {
            if m < 10 {
                cardinal(buf, atoi(&ip), b'L');
            } else {
                cardinal(buf, atoi(&ip[..m - 9]), b'H');
                cardinal(buf, atoi(&ip[m - 9..]), b'L');
            }
        } else {
            digits(buf, &ip, 1, 0);
        }
        if m > 0 && buf[0] == 0 {
            strcpy(buf, s(0x10078848));
        }
        digits(buf, &num[dot..], 1, 1);
        return;
    }
    let d = digits_only(src);
    let m = d.len();
    if m > 0xf {
        digits(buf, &d, 1, 0);
        let va = match d[m - 1] {
            b'1' => 0x1009c74c,
            b'2' => 0x1009c754,
            b'3' => 0x1009c75c,
            _ => 0x1009c764,
        };
        strcat(buf, s(va));
        return;
    }
    if m < 10 {
        if m > 0 {
            ordinal(buf, atoi(&d), b'L', 0);
            if buf[0] == 0 {
                strcpy(buf, s(0x1009c744));
            }
        }
        return;
    }
    let lo = atoi(&d[m - 9..]);
    ordinal(buf, atoi(&d[..m - 9]), b'H', lo);
    ordinal(buf, lo, b'L', lo);
    if buf[0] == 0 {
        strcpy(buf, s(0x1009c744));
    }
}

/// `FUN_100630d0`: digits read in pairs unless they start with 0 or contain
/// non-digits (then as a plain number).
pub fn code(buf: &mut [u8], src: &[u8]) {
    buf[0] = 0;
    let src = cs(src);
    if src.is_empty() {
        return;
    }
    if src[0] != b'0' && super::sbd::all_digit(src) {
        pairs(buf, src);
        return;
    }
    number(buf, src, 1);
}

/// `FUN_10041df0`: Roman numerals I..XX (and EUC-KR roman digits) -> value, else -1.
pub fn roman(w: &[u8]) -> i32 {
    let tab = [
        0x100776e8u32, 0x100806bc, 0x100806b8, 0x100806b4, 0x10079148, 0x100806b0, 0x100806ac, 0x100806a4, 0x100806a0, 0x1007d3bc, 0x1008069c, 0x10080698,
        0x10080690, 0x1008068c, 0x10080688, 0x10080684, 0x1008067c, 0x10080674, 0x10080670, 0x1008066c,
    ];
    for (i, &va) in tab.iter().enumerate() {
        if strcmp(s(va), w) == 0 {
            return i as i32 + 1;
        }
    }
    let w = cs(w);
    if w.len() == 2 && dbcs_pair(w[0], w[1]) && w[0] == 0xa5 && 0xaf < w[1] && w[1] < 0xba {
        return w[1] as i32 - 0xaf;
    }
    -1
}

/// `FUN_10062ee0`: a '"' within the `n` bytes before `t[p]`.
pub fn quote_before(t: &[u8], p: usize, n: i32) -> bool {
    if n == 0 || n < 0 {
        return false;
    }
    let mut i = 0;
    while i <= n {
        if at(t, p as isize - i as isize) == b'"' {
            return true;
        }
        i += 1;
    }
    false
}
