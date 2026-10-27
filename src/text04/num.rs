//! Number readers of the 2004 normalizers.

use super::mem::*;

/// `"%s %s"` onto a non-empty buffer, a copy onto an empty one (`FUN_1004f930` and the readers' own
/// appends).
pub fn cat(out: &mut Vec<u8>, w: &[u8]) {
    if out.is_empty() {
        out.extend_from_slice(cs(w));
    } else {
        out.push(b' ');
        out.extend_from_slice(cs(w));
    }
}

const ONES: [&[u8]; 20] = [
    b"", b"one", b"two", b"three", b"four", b"five", b"six", b"seven", b"eight", b"nine", b"ten", b"eleven", b"twelve", b"thirteen",
    b"fourteen", b"fifteen", b"sixteen", b"seventeen", b"eighteen", b"nineteen",
];
const TENS: [&[u8]; 10] = [b"", b"", b"twenty", b"thirty", b"forty", b"fifty", b"sixty", b"seventy", b"eighty", b"ninety"];

/// `FUN_1003ee70` mode `L`: below 10^9, appended.
pub fn low(out: &mut Vec<u8>, n: i32) {
    if n <= 0x13 {
        if n > 0 {
            cat(out, ONES[n as usize]);
        }
        return;
    }
    if n <= 99 {
        let mut w = TENS[(n / 10) as usize].to_vec();
        if n % 10 != 0 {
            w.push(b' ');
            w.extend_from_slice(ONES[(n % 10) as usize]);
        }
        cat(out, &w);
        return;
    }
    let (div, name): (i32, &[u8]) = if n < 1000 {
        (100, b" hundred")
    } else if n <= 999999 {
        (1000, b" thousand")
    } else if n <= 999999999 {
        (1000000, b" million")
    } else {
        return;
    };
    let (h, r) = (n / div, n % div);
    low(out, h);
    if h != 0 {
        out.extend_from_slice(name);
    }
    if div != 100 || r != 0 {
        low(out, r);
    }
}

/// `FUN_1003ee70` mode `H`: the digits above 10^9 (billions, trillions).
pub fn high(out: &mut Vec<u8>, n: i32) {
    if n < 1000 {
        billions(out, n / 100, n % 100);
        return;
    }
    if n > 999999 {
        return;
    }
    low(out, n / 1000);
    if n / 1000 != 0 {
        out.extend_from_slice(b" trillion");
    }
    high(out, n % 1000);
}

/// `FUN_1003f5a0`.
fn billions(out: &mut Vec<u8>, h: i32, r: i32) {
    low(out, h);
    if h != 0 {
        out.extend_from_slice(b" hundred");
    }
    if r != 0 {
        low(out, r);
    }
    if h != 0 || r != 0 {
        out.extend_from_slice(b" billion");
    }
}

/// `FUN_1004ff90`: the digits of `t`.
pub fn digits_of(t: &[u8]) -> Vec<u8> {
    cs(t).iter().copied().filter(|&c| is_digit(c)).collect()
}

/// `FUN_1004fb50`: non-empty, all digits.
pub fn all_digits(t: &[u8]) -> bool {
    let t = cs(t);
    !t.is_empty() && t.iter().all(|&c| is_digit(c))
}

fn whole(out: &mut Vec<u8>, d: &[u8]) {
    let l = d.len();
    if l > 15 {
        spell(out, d, 1, 0);
    } else if l < 10 {
        if l >= 1 {
            low(out, atoi(d));
        }
    } else {
        high(out, atoi(&d[..l - 9]));
        low(out, atoi(&d[l - 9..]));
    }
}

/// `FUN_1003eba0`: a cardinal number, with a decimal part read digit by digit. `mode` 0 reads a
/// four-digit number as a year.
pub fn cardinal(t: &[u8], mode: i16) -> Vec<u8> {
    let t = cs(t);
    let mut out = Vec::new();
    if mode == 0 && all_digits(t) && (t[0] as i8) > 0 && t.len() == 4 {
        return year(atoi(t));
    }
    if let Some(dot) = t.iter().position(|&c| c == b'.') {
        let d = digits_of(&t[..dot]);
        whole(&mut out, &d);
        if !d.is_empty() && out.is_empty() {
            out.extend_from_slice(b"zero");
        }
        spell(&mut out, &t[dot..], 1, 1);
        return out;
    }
    let d = digits_of(t);
    if d.is_empty() {
        return out;
    }
    whole(&mut out, &d);
    if out.is_empty() {
        out.extend_from_slice(b"zero");
    }
    out
}

/// `FUN_10040770`: one word per character (`.` "point", `0` "oh" or with `zero` "zero", anything else an
/// empty word); `mode` 0 reads a four-digit number as a year.
pub fn spell(out: &mut Vec<u8>, t: &[u8], mode: i16, zero: i16) {
    let t = cs(t);
    if mode == 0 && t.len() == 4 && all_digits(t) && t[0] > b'0' {
        let y = year(atoi(t));
        out.clear();
        out.extend_from_slice(&y);
        return;
    }
    for &c in t {
        let w: &[u8] = match c {
            b'.' => b"point",
            b'0' => {
                if zero == 0 {
                    b"oh"
                } else {
                    b"zero"
                }
            }
            b'1'..=b'9' => ONES[(c - b'0') as usize],
            _ => b"",
        };
        cat(out, w);
    }
}

fn card1(n: i32) -> Vec<u8> {
    cardinal(format!("{}", n).as_bytes(), 1)
}

/// `FUN_10040980`: a year.
pub fn year(n: i32) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    if n < 0 {
        return out;
    }
    if n == 0 {
        return b"two thousand".to_vec();
    }
    if (1..=9).contains(&n) || (30..=99).contains(&n) {
        out.extend_from_slice(if n < 10 { b"two thousand" } else { b"nineteen" });
        cat(&mut out, &card1(n));
        return out;
    }
    if (10..30).contains(&n) {
        return card1(n);
    }
    if (100..10000).contains(&n) {
        let (h, r) = (n / 100, n % 100);
        if h % 10 == 0 && r < 10 {
            let s = if n < 1000 { format!("{}", n) } else { format!("0{}", n) };
            return cardinal(s.as_bytes(), 1);
        }
        if h < 11 || h % 10 == 0 || r != 0 {
            out = card1(h);
            let rr = card1(r);
            if r > 9 {
                cat(&mut out, &rr);
            } else {
                out.extend_from_slice(b" oh ");
                out.extend_from_slice(&rr);
            }
            return out;
        }
        out = card1(h);
        out.extend_from_slice(b" hundred");
        return out;
    }
    let s = format!("{}", n).into_bytes();
    let l = s.len();
    let mut i = 0;
    if l % 2 == 1 {
        out = cardinal(&s[..1], 1);
        i = 1;
    }
    while i < l {
        let w = cardinal(&s[i..(i + 2).min(l)], 1);
        if out.is_empty() && i == 0 {
            out = w;
        } else {
            out.push(b' ');
            out.extend_from_slice(&w);
        }
        i += 2;
    }
    out
}

/// `FUN_1002ef60`: one or two digits, `0X` as "oh X".
pub fn pair(out: &mut Vec<u8>, t: &[u8]) {
    let t = cs(t);
    let w: Vec<u8> = if t.len() == 1 {
        match t[0] {
            b'0' => b"oh".to_vec(),
            b'1'..=b'9' => ONES[(t[0] - b'0') as usize].to_vec(),
            _ => Vec::new(),
        }
    } else if t.len() == 2 {
        let v = atoi(t);
        if v > 9 {
            cardinal(t, 1)
        } else if v >= 0 {
            let mut w = b"oh ".to_vec();
            w.extend_from_slice(if v == 0 { b"oh" } else { ONES[v as usize] });
            w
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };
    cat(out, &w);
}

/// `FUN_1002e9d0`: a house or route number in digit groups.
pub fn group(t: &[u8]) -> Vec<u8> {
    let t = cs(t);
    let mut out = Vec::new();
    let l = t.len();
    if l < 3 {
        pair(&mut out, t);
        return out;
    }
    if l == 3 {
        pair(&mut out, &t[..1]);
        if t[1] == b'0' && t[2] == b'0' {
            cat(&mut out, b"hundred");
        } else {
            pair(&mut out, &t[1..]);
        }
        return out;
    }
    if l == 4 {
        let c0 = t[0];
        if c0 > b'0' {
            if &t[1..4] == b"000" {
                pair(&mut out, &t[..1]);
                out.extend_from_slice(b" thousand");
                return out;
            }
            if t[1] > b'0' && &t[2..4] == b"00" {
                pair(&mut out, &t[..2]);
                out.extend_from_slice(b" hundred");
                return out;
            }
        }
        pair(&mut out, &t[..2]);
        pair(&mut out, &t[2..]);
        return out;
    }
    if l == 5 && t[0] >= b'1' && t[1] >= b'0' && &t[2..5] == b"000" {
        pair(&mut out, &t[..2]);
        out.extend_from_slice(b" thousand");
        return out;
    }
    for &c in t {
        let w: &[u8] = match c {
            b'0' => b"oh",
            b'1'..=b'9' => ONES[(c - b'0') as usize],
            _ => b"",
        };
        cat(&mut out, w);
    }
    out
}

/// `FUN_10040ef0`: digits as a house number, anything else as a cardinal.
pub fn house(t: &[u8]) -> Vec<u8> {
    let t = cs(t);
    if t.is_empty() {
        return Vec::new();
    }
    if all_digits(t) {
        group(t)
    } else {
        cardinal(t, 1)
    }
}

/// `FUN_1004fb80`: a well-formed number (`1,234.5`; no leading zero, groups of three) or a member of
/// `0x10073440`.
pub fn is_number(t: &[u8]) -> bool {
    const SPECIAL: Rec30 = rec30(0x10073440, 0x10084874);
    let t = cs(t);
    if t.is_empty() {
        return false;
    }
    if SPECIAL.find(t, false) >= 0 {
        return true;
    }
    let mut int_part: Vec<u8> = Vec::new();
    let mut frac: Vec<u8> = Vec::new();
    let mut dot = false;
    for (i, &c) in t.iter().enumerate() {
        if !is_digit(c) && c != b',' && c != b'.' {
            return false;
        }
        if dot {
            if c == b',' || c == b'.' {
                return false;
            }
            frac.push(c);
        } else if c == b'.' {
            dot = true;
        } else {
            if i == 0 && t[0] == b',' {
                return false;
            }
            int_part.push(c);
        }
    }
    if dot && frac.is_empty() {
        return false;
    }
    if int_part.len() > 1 && int_part[0] == b'0' {
        return false;
    }
    let mut run = 0;
    for i in (0..int_part.len()).rev() {
        if t[i] == b',' {
            if run != 3 {
                return false;
            }
            run = 0;
        } else {
            run += 1;
        }
    }
    true
}

/// `FUN_1004fcc0`: a well-formed number with a decimal point.
pub fn is_decimal(t: &[u8]) -> bool {
    is_number(t) && cs(t).contains(&b'.')
}

/// `FUN_10034df0`: a four-digit year `1000..9999` or `'NN`; -1 otherwise.
pub fn year_value(t: &[u8]) -> i32 {
    let t = cs(t);
    if at(t, 0) == b'\'' {
        if t.len() == 3 && is_digit(t[1]) && is_digit(t[2]) {
            return atoi(&t[1..]);
        }
        return -1;
    }
    if t.len() == 4 && (b'1'..=b'9').contains(&t[0]) && is_digit(t[1]) && is_digit(t[2]) && is_digit(t[3]) {
        return atoi(t);
    }
    -1
}

/// `FUN_10034bb0`: digits (a leading `'` allowed), or digits ending in a matching ordinal suffix.
pub fn day_like(t: &[u8]) -> bool {
    let t = cs(t);
    let l = t.len();
    for i in 0..l {
        if is_digit(t[i]) || (i == 0 && l >= 2 && t[0] == b'\'') {
            continue;
        }
        if l > 2 && i == l - 2 {
            let p = t[i - 1];
            let suf = &t[i..];
            if (p == b'1' && stricmp(b"st", suf) == 0)
                || (p == b'2' && stricmp(b"nd", suf) == 0)
                || (p == b'3' && stricmp(b"rd", suf) == 0)
                || ((p == b'0' || (b'4'..=b'9').contains(&p)) && stricmp(b"th", suf) == 0)
            {
                return true;
            }
        }
        return false;
    }
    true
}

const ORD: [&[u8]; 20] = [
    b"", b"first", b"second", b"third", b"fourth", b"fifth", b"sixth", b"seventh", b"eighth", b"ninth", b"tenth", b"eleventh", b"twelfth",
    b"thirteenth", b"fourteenth", b"fifteenth", b"sixteenth", b"seventeenth", b"eighteenth", b"nineteenth",
];
/// The engine spells forty's ordinal "fourtieth".
const ORD_TENS: [&[u8]; 10] = [b"", b"", b"twentieth", b"thirtieth", b"fourtieth", b"fiftieth", b"sixtieth", b"seventieth", b"eightieth", b"ninetieth"];

/// `FUN_1003fc10` mode `L`: an ordinal below 10^9, appended.
pub fn ord_low(out: &mut Vec<u8>, n: i32) {
    if n <= 0x13 {
        if n > 0 {
            cat(out, ORD[n as usize]);
        }
        return;
    }
    if n <= 99 {
        let w = if n % 10 == 0 {
            ORD_TENS[(n / 10) as usize].to_vec()
        } else {
            let mut w = TENS[(n / 10) as usize].to_vec();
            w.push(b' ');
            w.extend_from_slice(ORD[(n % 10) as usize]);
            w
        };
        cat(out, &w);
        return;
    }
    let (div, name, nameth): (i32, &[u8], &[u8]) = if n < 1000 {
        (100, b" hundred", b" hundredth")
    } else if n <= 999999 {
        (1000, b" thousand", b" thousandth")
    } else if n <= 999999999 {
        (1000000, b" million", b" millionth")
    } else {
        return;
    };
    let (h, r) = (n / div, n % div);
    if h == 0 {
        ord_low(out, r);
        return;
    }
    low(out, h);
    if r != 0 {
        out.extend_from_slice(name);
        ord_low(out, r);
    } else {
        out.extend_from_slice(nameth);
    }
}

/// `FUN_1003fc10` mode `H`: the digits above 10^9; `lo` is the rest (0: this part takes the suffix).
pub fn ord_high(out: &mut Vec<u8>, n: i32, lo: i32) {
    if n < 1000 {
        let (h, r) = (n / 100, n % 100);
        low(out, h);
        if h != 0 {
            out.extend_from_slice(b" hundred");
        }
        if r != 0 {
            low(out, r);
        }
        if h != 0 || r != 0 {
            out.extend_from_slice(if lo < 1 { b" billionth" } else { b" billion" });
        }
        return;
    }
    if n > 999999 {
        return;
    }
    let (t, r) = (n / 1000, n % 1000);
    if t == 0 {
        ord_high(out, r, lo);
        return;
    }
    low(out, t);
    if lo != 0 || r != 0 {
        out.extend_from_slice(b" trillion");
        ord_high(out, r, lo);
    } else {
        out.extend_from_slice(b" trillionth");
    }
}

/// `FUN_1004fa50`: digits, with commas and points only between digits.
pub fn numberish(t: &[u8]) -> bool {
    let t = cs(t);
    let l = t.len();
    for i in 0..l {
        let c = t[i];
        if is_digit(c) {
            continue;
        }
        if c == b',' && i >= 1 && i + 1 < l && is_digit(t[i - 1]) && is_digit(t[i + 1]) {
            continue;
        }
        if c != b'.' || i + 1 >= l {
            return false;
        }
        let nb = if i == 0 {
            cls(at(t, 1))
        } else {
            if !is_digit(t[i - 1]) {
                return false;
            }
            cls(t[i + 1])
        };
        if nb & 0x10 == 0 {
            return false;
        }
    }
    true
}

/// `FUN_1003f790`: an ordinal (`21st`, `3rd`); a decimal reads as a cardinal.
pub fn ordinal(t: &[u8]) -> Vec<u8> {
    let t = cs(t);
    let mut out = Vec::new();
    let l = t.len();
    let stem: &[u8] = if l >= 3 && [&b"st"[..], b"nd", b"rd", b"th"].iter().any(|x| stricmp(&t[l - 2..], x) == 0) { &t[..l - 2] } else { t };
    if !numberish(stem) {
        return out;
    }
    if let Some(dot) = stem.iter().position(|&c| c == b'.') {
        let d = digits_of(&stem[..dot]);
        whole(&mut out, &d);
        if !d.is_empty() && out.is_empty() {
            out.extend_from_slice(b"zero");
        }
        spell(&mut out, &stem[dot..], 1, 1);
        return out;
    }
    let d = digits_of(t);
    let n = d.len();
    if n > 15 {
        spell(&mut out, &d, 1, 0);
        out.extend_from_slice(match d[n - 1] {
            b'1' => b" S T",
            b'2' => b" N D",
            b'3' => b" R D",
            _ => b" T H",
        });
        return out;
    }
    if n < 10 {
        if n > 0 {
            ord_low(&mut out, atoi(&d));
            if out.is_empty() {
                out.extend_from_slice(b"zeroth");
            }
        }
        return out;
    }
    let lo = atoi(&d[n - 9..]);
    ord_high(&mut out, atoi(&d[..n - 9]), lo);
    ord_low(&mut out, lo);
    if out.is_empty() {
        out.extend_from_slice(b"zeroth");
    }
    out
}

/// `FUN_1004fcf0`: a number that takes the plural (`1` and `0.5` do not).
pub fn plural(t: &[u8]) -> bool {
    let t = cs(t);
    if t.is_empty() || !numberish(t) {
        return false;
    }
    match t.iter().position(|&c| c == b'.') {
        None => atoi(&digits_of(t)) != 1,
        Some(p) => atoi(&t[..p]) > 0 && atoi(&t[p + 1..]) > 0,
    }
}

/// MSVC `atof`: the leading decimal number of `t`.
pub fn atof(t: &[u8]) -> f64 {
    let t = cs(t);
    let mut i = 0;
    while i < t.len() && matches!(t[i], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        i += 1;
    }
    let s = i;
    if i < t.len() && (t[i] == b'+' || t[i] == b'-') {
        i += 1;
    }
    let digits = |i: &mut usize| {
        let a = *i;
        while *i < t.len() && t[*i].is_ascii_digit() {
            *i += 1;
        }
        *i - a
    };
    let mut nd = digits(&mut i);
    if i < t.len() && t[i] == b'.' {
        i += 1;
        nd += digits(&mut i);
    }
    if nd == 0 {
        return 0.0;
    }
    let mut e = i;
    if e < t.len() && (t[e] == b'e' || t[e] == b'E') {
        e += 1;
        if e < t.len() && (t[e] == b'+' || t[e] == b'-') {
            e += 1;
        }
        if digits(&mut e) > 0 {
            i = e;
        }
    }
    std::str::from_utf8(&t[s..i]).ok().and_then(|x| x.parse().ok()).unwrap_or(0.0)
}

/// `FUN_10040d00`: `num/den` as a fraction ("one half", "three quarters", "five sixteenths").
pub fn fraction(num: &[u8], den: &[u8]) -> Vec<u8> {
    let (a, b) = (atof(num), atof(den));
    let named = |one: &[u8], many: &[u8]| {
        if a == 1.0 {
            return one.to_vec();
        }
        let mut out = cardinal(num, 1);
        if !out.is_empty() {
            out.extend_from_slice(many);
        }
        out
    };
    if b == 2.0 {
        return named(b"one half", b" halves");
    }
    if b == 4.0 {
        return named(b"one quarter", b" quarters");
    }
    let mut out = cardinal(num, 1);
    if out.is_empty() {
        return out;
    }
    let o = ordinal(den);
    if o.is_empty() {
        return Vec::new();
    }
    cat(&mut out, &o);
    if plural(num) {
        out.push(b's');
    }
    out
}

/// `FUN_1003f650`: digits with the ordinal suffix of their last digit (or `th` after two digits).
pub fn is_ordinal(t: &[u8]) -> bool {
    let t = cs(t);
    let l = t.len();
    if l < 3 {
        return false;
    }
    let suf = &t[l - 2..];
    let k = if stricmp(suf, b"st") == 0 {
        1
    } else if stricmp(suf, b"nd") == 0 {
        2
    } else if stricmp(suf, b"rd") == 0 {
        3
    } else if stricmp(suf, b"th") == 0 {
        4
    } else {
        return false;
    };
    let stem = &t[..l - 2];
    if !stem.iter().all(|&c| is_digit(c) || c == b',' || c == b'.') {
        return false;
    }
    let last = at(stem, stem.len() as isize - 1);
    if (last == b'1' && k == 1) || (last == b'2' && k == 2) || (last == b'3' && k == 3) || ((last == b'0' || (b'4'..=b'9').contains(&last)) && k == 4) {
        return true;
    }
    stem.len() > 1 && k == 4
}

/// `FUN_1004f860`: capitalize the first letter and every word, lower-case the other capitals.
pub fn title_case(t: &mut [u8]) {
    let up = |c: u8| b(0x1006ac60, 768)[(256 + 2 * c as i8 as i32) as usize];
    let n = cs(t).len();
    if n == 0 {
        return;
    }
    let mut i = 0;
    loop {
        if i == 0 {
            t[0] = up(t[0]);
        } else {
            let c = t[i];
            let c1 = at(t, i as isize + 1);
            if c == b' ' && c1 != 0 && cls(c1) & 0x40 != 0 {
                t[i + 1] = up(c1);
                i += 1;
            } else if cls(c) & 0x80 != 0 {
                t[i] = fold_s(c) as u8;
            }
        }
        i += 1;
        if at(t, i as isize) == 0 {
            break;
        }
    }
}
