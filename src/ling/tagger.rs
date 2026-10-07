//! Homograph disambiguation for words with several lexicon pronunciations.
//!
//! `FUN_100068b0` first tries the hand-written rules (`FUN_100049b0`, with the
//! fallback `FUN_10004300` and the article rule `FUN_10003ff0`); when no rule
//! fires, every POS of every entry is scored by `poly.tree3` over a 15-feature
//! context of the neighbouring candidate spellings and the entry with the most
//! votes wins. All of it runs over the candidate array (0x554-byte records),
//! not the morph array.

use super::lookup::cand;
use super::tables::*;
use super::{sc, Ws};
use crate::lexicon::{stricmp, strcmp};
use crate::tree3::Tree;

/// `FUN_100560a0(list, key, n, 0x49)`: case-insensitive binary search.
pub fn bsearch_ci(list: &[&[u8]], key: &[u8]) -> i32 {
    if key.is_empty() {
        return -1;
    }
    let (mut lo, mut hi) = (0i32, list.len() as i32 - 1);
    let mut mid = hi;
    while lo <= hi {
        mid /= 2;
        let c = stricmp(list[mid as usize], key);
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

pub fn in_list(list: &[&[u8]], key: &[u8]) -> bool {
    bsearch_ci(list, key) >= 0
}

fn ci(a: &[u8], b: &[u8]) -> bool {
    stricmp(a, b) == 0
}

fn oci(a: Option<&[u8]>, b: &[u8]) -> bool {
    a.is_some_and(|a| ci(a, b))
}

fn cls(c: u8) -> u8 {
    sc(&CHAR_CLASS, c)
}

fn first(s: &[u8]) -> u8 {
    *s.first().unwrap_or(&0)
}

/// `FUN_1001c320`: case-insensitive strncmp.
pub fn strnicmp(a: &[u8], b: &[u8], n: usize) -> i32 {
    if n == 0 {
        return 0;
    }
    let at = |s: &[u8], i: usize| *s.get(i).unwrap_or(&0);
    let mut i = 0usize;
    loop {
        let d = TOLOWER_U[at(a, i) as usize] as i32 - TOLOWER_U[at(b, i) as usize] as i32;
        if d != 0 {
            return d;
        }
        if at(a, i) == 0 {
            return 0;
        }
        i += 1;
        if i == n {
            return 0;
        }
    }
}

/// `FUN_10063170`: every character is a lower-case letter.
pub fn all_lower(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&c| cls(c) & 0x40 != 0)
}

/// `FUN_1000ffa0`: every character is a letter.
pub fn all_letters(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&c| cls(c) & 0xc0 != 0)
}

/// `FUN_1000ffd0`: no character is a lower-case letter.
pub fn no_lower(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&c| !(cls(c) & 0xc0 != 0 && cls(c) & 0x40 != 0))
}

fn suffix(s: &[u8], k: usize) -> &[u8] {
    &s[s.len() - k..]
}

/// `FUN_10007160`: morphological class of a word from its ending.
pub fn suffix_class(w: Option<&[u8]>) -> i16 {
    let Some(p) = w else { return 0 };
    if p.is_empty() {
        return 0;
    }
    let n = p.len();
    let e = |k: usize, s: &[u8]| ci(suffix(p, k), s);
    if n > 8 && e(6, b"liness") {
        return 2;
    }
    if n > 6 && (e(4, b"ance") || e(4, b"tion")) {
        return 2;
    }
    if (n > 7 && e(5, b"tions")) || (n > 6 && e(4, b"sion")) || (n > 7 && e(5, b"sions")) || (n > 6 && e(4, b"ness"))
        || (n > 8 && e(6, b"nesses"))
    {
        return 2;
    }
    if n > 6 && (e(4, b"ment") || e(5, b"ments") || e(4, b"ship")) {
        return 2;
    }
    if n > 7 && e(5, b"ships") {
        return 2;
    }
    if n > 6 && (e(3, b"ity") || e(4, b"ties") || e(4, b"ings")) {
        return 2;
    }
    if all_letters(p) && no_lower(p) {
        return 2;
    }
    if n > 6 && (e(4, b"tial") || e(4, b"cial") || e(4, b"less")) {
        return 3;
    }
    if n > 5 && (e(3, b"ful") || e(3, b"cal") || e(3, b"fic") || e(3, b"ian") || e(3, b"ous")) {
        return 3;
    }
    if n > 6 && (e(4, b"tive") || e(4, b"sive")) {
        return 3;
    }
    if n > 8 && e(6, b"tional") {
        return 3;
    }
    if n > 4 && e(2, b"ly") {
        return 4;
    }
    if n > 5 {
        if e(3, b"ing") {
            return 5;
        }
        if e(3, b"ist") {
            return 6;
        }
    }
    if n < 7 || !e(4, b"ists") {
        if n > 4 {
            if e(2, b"ed") {
                return 7;
            }
            if e(1, b"s") {
                return 8;
            }
        }
        return 1;
    }
    6
}

/// `FUN_10006ca0`: 0 none, 1 not a preposition, else preposition index + 2.
fn prep_code(w: Option<&[u8]>) -> i16 {
    match w {
        Some(p) if !p.is_empty() => {
            let i = bsearch_ci(&L_PREP, p);
            if i < 0 {
                1
            } else {
                (i + 2) as i16
            }
        }
        _ => 0,
    }
}

const PUNCT: &[u8] = b"`~!@#$%^&*()-_+=|\\[]{};:'\"<>?,./";

/// `FUN_10006ce0(a, b, c)`: syntactic class of `b` given its neighbours.
pub fn word_class(a: Option<&[u8]>, b: Option<&[u8]>, c: Option<&[u8]>) -> i16 {
    word_class_e(false, a, b, c)
}

/// 2009 (`FUN_100068e0`): after "'s" only the object-pronoun list is checked.
pub fn word_class_e(e09: bool, a: Option<&[u8]>, b: Option<&[u8]>, c: Option<&[u8]>) -> i16 {
    let b = match b {
        Some(x) if !x.is_empty() => x,
        _ => return 0,
    };
    if strcmp(b, b"I") == 0 {
        return 8;
    }
    let aux_to = |x: &[u8]| ci(x, b"ought") || ci(x, b"have") || ci(x, b"has") || ci(x, b"had") || ci(x, b"need");
    if let Some(c) = c {
        if (ci(b, b"wo") || ci(b, b"sha") || ci(b, b"ca")) && ci(c, b"n't") {
            return 2;
        }
        if aux_to(b) && ci(c, b"to") {
            return 2;
        }
    }
    if let Some(a) = a {
        if aux_to(a) && ci(b, b"to") {
            return 2;
        }
    }
    if ci(b, b"'s") {
        if let Some(a) = a {
            if ci(a, b"let") {
                return 8;
            }
            if in_list(&L_OBJPRON, a) || (!e09 && (in_list(&L_SUBJPRON, a) || in_list(&L_WH, a))) {
                return 3;
            }
        }
        return 7;
    }
    if in_list(&L_MODAL, b) {
        return 2;
    }
    if in_list(&L_BE, b) {
        return 3;
    }
    if in_list(&L_CONJ, b) {
        return 4;
    }
    if in_list(&L_DET, b) {
        return 0xc;
    }
    if in_list(&L_NEG, b) {
        return 6;
    }
    if in_list(&L_POSSPRON, b) {
        return 7;
    }
    if in_list(&L_OBJPRON, b) {
        return 8;
    }
    if in_list(&L_WH, b) {
        return 0xb;
    }
    if in_list(&L_NUMWORD, b) {
        return 9;
    }
    if (a.is_some_and(|a| ci(a, b"rather")) && ci(b, b"than")) || (c.is_some() && ci(b, b"rather") && oci(c, b"than")) {
        return 0xb;
    }
    if in_list(&L_PREP, b) {
        return 5;
    }
    if b.iter().any(|&ch| cls(ch) & 0x10 != 0) {
        return 9;
    }
    if b.iter().any(|&ch| PUNCT.contains(&ch)) {
        return 10;
    }
    1
}

/// `FUN_10008440(p1, p2)`: modal / auxiliary + infinitive marker.
pub(super) fn modal_pair(p1: &[u8], p2: Option<&[u8]>) -> bool {
    if p2.map_or(true, |s| s.is_empty()) && in_list(&L_MODAL, p1) {
        return true;
    }
    let Some(p2) = p2 else { return false };
    if (ci(p1, b"wo") || ci(p1, b"sha") || ci(p1, b"ca")) && ci(p2, b"n't") {
        return true;
    }
    if (ci(p1, b"ought") || ci(p1, b"have") || ci(p1, b"has") || ci(p1, b"had") || ci(p1, b"need")) && ci(p2, b"to") {
        return true;
    }
    false
}

/// `FUN_10010840`.
fn aux_context(p1: Option<&[u8]>, p2: Option<&[u8]>) -> bool {
    if let Some(a) = p1.filter(|s| !s.is_empty()) {
        if ci(a, b"let") || modal_pair(a, p2) || (modal_pair(a, Some(b"")) && oci(p2, b"not")) {
            return true;
        }
    }
    if let Some(b) = p2.filter(|s| !s.is_empty()) {
        if modal_pair(b, Some(b"")) || ci(b, b"do") {
            return true;
        }
    }
    false
}

/// A parsed POS list: entries split at tag 100, `)` folded into `%` and `(`
/// into `&` (`FUN_100105d0` / `FUN_10010550`), each with a vote count.
pub struct PosList {
    pub entries: Vec<(i32, Vec<u8>)>,
}

impl PosList {
    pub fn parse(raw: &[u8]) -> PosList {
        let mut entries = Vec::new();
        let mut cur: Vec<u8> = Vec::new();
        let mut i = 0usize;
        loop {
            while i < raw.len() && raw[i] != b'd' && raw[i] != 0xff {
                cur.push(raw[i]);
                i += 1;
            }
            let mut out: Vec<u8> = Vec::new();
            for &c in &cur {
                if c == b')' && out.contains(&b'%') {
                    continue;
                }
                if c == b'(' && out.contains(&b'&') {
                    continue;
                }
                out.push(match c {
                    b')' => b'%',
                    b'(' => b'&',
                    x => x,
                });
            }
            entries.push((0, out));
            cur.clear();
            if i >= raw.len() || raw[i] == 0xff {
                break;
            }
            i += 1;
        }
        PosList { entries }
    }

    /// `FUN_10010640`: first entry holding a tag of the same class as `tag`.
    pub fn find(&self, tag: u8) -> i32 {
        let want = POS_CLASS[(tag as i8 as isize + 0) as usize & 0x7f];
        for (k, (_, l)) in self.entries.iter().enumerate() {
            for &p in l {
                if POS_CLASS[p as usize & 0x7f] == want {
                    return k as i32;
                }
            }
        }
        -1
    }

    fn has_class(list: &[u8], class: i16) -> bool {
        list.iter().any(|&p| POS_CLASS[p as usize & 0x7f] == class)
    }
}

/// Candidate-array view used by the rules: spellings by relative index.
pub struct Cv<'a> {
    pub ws: &'a Ws,
    pub n: usize,
    /// julie09 / paul09 rule set (`FUN_10004940`, `FUN_10004300`, `FUN_100068e0`).
    pub e09: bool,
}

/// 2009 frequency-adverb list (`FUN_10010210`, julie09 0x10074630): 15 entries, 2014 has 20.
static ADVFREQ09: [&[u8]; 15] = [
    b"almost", b"also", b"always", b"even", b"hardly", b"just", b"never", b"often", b"only", b"quite", b"rarely", b"really",
    b"seldom", b"sometimes", b"usually",
];

/// 2009 subject-pronoun list (julie09 0x100744c0): 7 entries, 2014 has 10.
static SUBJ09: [&[u8]; 7] = [b"he", b"i", b"it", b"she", b"they", b"we", b"you"];

impl<'a> Cv<'a> {
    fn advfreq(&self, p: &[u8]) -> bool {
        let l: &[&[u8]] = if self.e09 { &ADVFREQ09 } else { &L_ADVFREQ };
        in_list(l, p)
    }

    fn subj(&self, p: &[u8]) -> bool {
        let l: &[&[u8]] = if self.e09 { &SUBJ09 } else { &L_SUBJPRON };
        in_list(l, p)
    }
}

impl<'a> Cv<'a> {
    fn sp(&self, i: usize, k: isize) -> Option<&'a [u8]> {
        let j = i as isize + k;
        if j < 0 || j >= self.n as isize {
            return None;
        }
        Some(self.ws.cstr(Ws::cand(j as usize) + cand::SPELLING))
    }

    fn token(&self, j: usize) -> i16 {
        self.ws.i16(Ws::cand(j) + cand::TOKEN)
    }

    fn pron(&self, i: usize, k: i32) -> Vec<u8> {
        let o = (Ws::cand(i) + cand::PRON) as isize + k as isize * cand::PRON_STRIDE as isize;
        self.ws.cstr(o as usize).to_vec()
    }
}

/// `FUN_10003ff0`: pronunciation of "a" -- EY1 (0x1e) or AH0 (7).
fn article_a(c: &Cv, i: usize) -> u8 {
    let n = c.n;
    let w = c.sp(i, 0).unwrap_or(b"");
    let prev = c.sp(i, -1);
    let next = c.sp(i, 1);
    if i > 0 && (oci(prev, b"an") || oci(prev, b"the")) {
        return 0x1e;
    }
    if i + 1 < n {
        let nx = next.unwrap();
        if in_list(&L_CONJ, nx) {
            return 0x1e;
        }
        if (in_list(&L_BE, nx) || modal_pair(nx, Some(b"")) || ci(nx, b"of"))
            && ((!ci(nx, b"can") && !ci(nx, b"must")) || cls(first(w)) & 0x80 != 0)
        {
            return 0x1e;
        }
    }
    if i + 2 < n && modal_pair(next.unwrap(), c.sp(i, 2)) {
        return 0x1e;
    }
    let comma_path = i == 0 || first(prev.unwrap_or(b"")) == b',';
    if comma_path && i + 1 < n && first(next.unwrap()) == b',' {
        return 0x1e;
    }
    if i > 0 && i + 1 < n && cls(first(w)) & 0x80 != 0 && all_lower(prev.unwrap()) && all_lower(next.unwrap()) {
        return 0x1e;
    }
    if i + 1 == n || (i + 2 == n && b".?!,\0".contains(&first(next.unwrap()))) {
        return 0x1e;
    }
    if i > 0 {
        let p = prev.unwrap();
        if p.len() == 1 && cls(p[0]) & 0xc0 != 0 {
            return 0x1e;
        }
    }
    if i + 1 < n {
        let p = next.unwrap();
        if p.len() == 1 && cls(p[0]) & 0xc0 != 0 {
            return 0x1e;
        }
    }
    7
}

enum R {
    Tag(u8),
    Fallback,
}

/// `FUN_1000cf00(cands, i, n, mode, window, list)`: position in a window of
/// neighbouring spellings where an entry of `list` starts. Multi-word
/// entries only have their later words checked for length (engine bug).
pub fn phrase_in_window(c: &Cv, i: usize, mode: u8, window: usize, list: &[&[u8]]) -> i32 {
    let n = c.n as i32;
    let i = i as i32;
    let win = window.min(0x14) as i32;
    let after = n - 1 - i;
    let mut words: Vec<&[u8]> = Vec::new();
    if i >= 1 && mode == b'L' {
        let (from, cnt) = if i < win { (0, i) } else { (i - win, win) };
        if cnt < 1 {
            return -1;
        }
        for k in 0..cnt {
            words.push(c.sp(0, (from + k) as isize).unwrap_or(b""));
        }
    } else {
        if after < 1 || mode != b'R' {
            return -1;
        }
        let cnt = if after < win { after } else { win };
        if cnt < 1 {
            return -1;
        }
        for k in 0..cnt {
            words.push(c.sp(0, (i + 1 + k) as isize).unwrap_or(b""));
        }
    }
    for k in 0..words.len() {
        let s = words[k];
        let len = s.len();
        let (mut lo, mut hi) = (0i32, list.len() as i32 - 1);
        let mut mid = hi;
        while lo <= hi {
            mid /= 2;
            let e = list[mid as usize];
            let r = strnicmp(e, s, len);
            if r == 0 {
                let tail = *e.get(len).unwrap_or(&0);
                if tail == 0 {
                    return k as i32;
                }
                if tail == b' ' {
                    let mut rest: &[u8] = &e[len + 1..];
                    let mut j = k + 1;
                    while j < words.len() {
                        let l2 = words[j].len();
                        let t = *rest.get(l2).unwrap_or(&0);
                        if t == 0 {
                            return j as i32;
                        }
                        if t != b' ' {
                            break;
                        }
                        rest = &rest[(l2 + 1).min(rest.len())..];
                        j += 1;
                    }
                }
                hi = mid - 1;
            } else if r > 0 {
                hi = mid - 1;
            } else {
                lo = mid + 1;
            }
            mid = hi + lo;
        }
    }
    -1
}

fn rules(c: &Cv, i: usize) -> R {
    use R::*;
    let n = c.n;
    let w = c.sp(i, 0).unwrap_or(b"");
    let p1 = c.sp(i, -1);
    let p2 = c.sp(i, -2);
    let n1 = c.sp(i, 1);
    let n2 = c.sp(i, 2);
    let has = |k: usize| i + k < n;
    let q = |o: Option<&[u8]>, s: &[u8]| oci(o, s);
    let len_code = |s: &[u8]| -(s.len() as i32) - 2;

    if ci(w, b"anti") {
        if has(1) && in_list(&L_CONJ, n1.unwrap()) {
            return Tag(0x0e);
        }
        return Tag(0x13);
    }
    if ci(w, b"can") {
        if i < 1 || !q(p1, b"steel") {
            if has(1) && q(n1, b"be") {
                return Tag(0x12);
            }
            if i > 0 && (in_list(&L_DET, p1.unwrap()) || in_list(&L_PREP, p1.unwrap())) {
                return Tag(0x13);
            }
            if has(1) {
                if in_list(&L_PREP, n1.unwrap()) {
                    return Tag(0x13);
                }
                let x = n1.unwrap();
                if ci(x, b".") || ci(x, b",") || ci(x, b"!") || ci(x, b"?") {
                    return Tag(0x13);
                }
            }
            return if i + 1 == n { Tag(0x13) } else { Tag(0x12) };
        }
        return Tag(0x13);
    }
    if ci(w, b"close") {
        // LAB_10004c99 and friends.
        let after_c99 = |c: &Cv| -> R {
            if i < 2 || !q(p2, b"to") || !q(p1, b"a") {
                let l4d6e = || -> R {
                    if has(2) {
                        let x = n1.unwrap();
                        if (ci(x, b"at") && q(n2, b"hand")) || (ci(x, b"in") && q(n2, b"with")) {
                            return Tag(0x0e);
                        }
                    }
                    let l4e18 = || -> R {
                        if !has(2) {
                            return Fallback;
                        }
                        let x = n1.unwrap();
                        if !ci(x, b"it") || !q(n2, b"up") {
                            if !ci(x, b"down") {
                                return Fallback;
                            }
                            return if q(n2, b"on") { Tag(b'%') } else { Fallback };
                        }
                        Tag(b'%')
                    };
                    if has(1) {
                        let x = n1.unwrap();
                        if !ci(x, b"in") && !ci(x, b"up") {
                            return l4e18();
                        }
                        Tag(b'%')
                    } else {
                        l4e18()
                    }
                };
                if !has(1) {
                    return l4d6e();
                }
                let x = n1.unwrap();
                if !ci(x, b"by") && !ci(x, b"on") && !ci(x, b"to") && !ci(x, b"upon") && !ci(x, b"call") && !ci(x, b"calls")
                {
                    return l4d6e();
                }
                let _ = c;
                Tag(0x0e)
            } else {
                Tag(b'%')
            }
        };
        if i < 1 {
            return after_c99(c);
        }
        let p = p1.unwrap();
        if ci(p, b"I") || ci(p, b"you") || ci(p, b"we") || ci(p, b"they") {
            return Tag(b'%');
        }
        if !ci(p, b"feel") {
            if !c.e09 && (ci(p, b"fisherman") || ci(p, b"fishermans")) {
                return Tag(0x13);
            }
            return after_c99(c);
        }
        return Tag(0x0e);
    }
    if ci(w, b"closer") {
        let l4f97 = || -> R {
            if !has(2) {
                return Fallback;
            }
            let x = n1.unwrap();
            if (!ci(x, b"at") || !q(n2, b"hand")) && (!ci(x, b"in") || !q(n2, b"with")) {
                return Fallback;
            }
            Tag(0x0f)
        };
        if has(1) {
            let x = n1.unwrap();
            if !ci(x, b"than")
                && !ci(x, b"by")
                && !ci(x, b"on")
                && !ci(x, b"to")
                && !ci(x, b"upon")
                && !ci(x, b"call")
                && !ci(x, b"calls")
                && (c.e09 || (!ci(x, b"look") && !ci(x, b"looks")))
                && suffix_class(Some(x)) != 2
            {
                return l4f97();
            }
            return Tag(0x0f);
        }
        return l4f97();
    }
    if ci(w, b"conflicts") {
        if !has(1) || !q(n1, b"with") {
            return Fallback;
        }
        return Tag(b'*');
    }
    if !c.e09 && (ci(w, b"contest") || ci(w, b"contests")) {
        let lc = len_code(w);
        if !has(1) || !in_list(&L_BE, n1.unwrap()) {
            return Fallback;
        }
        return if lc != -9 { Tag(0x16) } else { Tag(0x13) };
    }
    if !c.e09 && ci(w, b"converts") {
        if !has(1) || !q(n1, b"from") {
            if i > 0 && (q(p1, b"the") || q(p1, b"a")) {
                return Tag(0x16);
            }
            return Tag(b'*');
        }
        return Tag(0x16);
    }
    if ci(w, b"de") {
        if (i < 1 || cls(first(p1.unwrap())) & 0x80 == 0) && (!has(1) || cls(first(n1.unwrap())) & 0x80 == 0) {
            return Fallback;
        }
        return Tag(0x0c);
    }
    if ci(w, b"does") {
        if (i < 1 || !q(p1, b"much")) && (!has(1) || !q(n1, b"not")) {
            return Fallback;
        }
        return Tag(b'*');
    }
    if ci(w, b"dogged") {
        let r = phrase_in_window(c, i, b'L', 3, &L_HAVE);
        if r < 0 {
            return Tag(0x0e);
        }
        if r > 0 && i > 0 {
            let p = p1.unwrap();
            if in_list(&L_DET, p) || ci(p, b"that") {
                return Tag(0x0e);
            }
        }
        return Tag(b'(');
    }
    if ci(w, b"dove") {
        if !has(1) {
            return Fallback;
        }
        return if q(n1, b"into") { Tag(b'&') } else { Fallback };
    }
    if !ci(w, b"elaborate") {
        if ci(w, b"house") {
            if (i < 1 || cls(first(p1.unwrap())) & 0x80 == 0) && (!has(1) || cls(first(n1.unwrap())) & 0x80 == 0) {
                if i > 0 && has(1) && first(w) == b'H' {
                    let p = p1.unwrap();
                    if (ci(p, b"the") || ci(p, b"a")) && cls(first(n1.unwrap())) & 0x40 != 0 {
                        return Tag(0x13);
                    }
                }
                if has(1) && q(n1, b"arrest") {
                    return Tag(0x13);
                }
                if !has(1) {
                    return Fallback;
                }
                let x = n1.unwrap();
                if c.e09 || (!ci(x, b"number") && !ci(x, b"numbers")) {
                    return Fallback;
                }
                return Tag(0x13);
            }
            return Tag(0x13);
        }
        if ci(w, b"import") {
            if (i > 0 && q(p1, b"export")) || (has(1) && q(n1, b"bank")) {
                return Tag(0x13);
            }
            if i < 1 {
                return Fallback;
            }
            let p = p1.unwrap();
            if ci(p, b"the") {
                return Tag(0x13);
            }
            return if ci(p, b"an") { Tag(0x13) } else { Fallback };
        }
        if ci(w, b"increase") || ci(w, b"increases") {
            if c.e09 {
                if !has(1) || !q(n1, b"in") {
                    return Fallback;
                }
                return Tag(0x13);
            }
            let lc = len_code(w);
            if i > 0 {
                let p = p1.unwrap();
                if ci(p, b"an") || ci(p, b"the") {
                    return if lc == -10 { Tag(0x13) } else { Tag(0x16) };
                }
            }
            if !has(1) || !in_list(&L_BE, n1.unwrap()) {
                return Fallback;
            }
            return if lc != -10 { Tag(0x16) } else { Tag(0x13) };
        }
        if ci(w, b"interstate") {
            if !has(1) || cls(first(w)) & 0x80 == 0 || cls(first(n1.unwrap())) & 0x80 == 0 {
                return Fallback;
            }
            return Tag(0x0e);
        }
        if ci(w, b"interstate") || ci(w, b"interstates") {
            if !has(1) || !q(n1, b"to") {
                return Fallback;
            }
            return if len_code(w) == -0xc { Tag(b'%') } else { Tag(b'*') };
        }
        if ci(w, b"invalid") {
            if i < 1 || !in_list(&L_BE, p1.unwrap()) {
                if i < 2 || !in_list(&L_BE, p2.unwrap()) {
                    return Fallback;
                }
                let p = p1.unwrap();
                if !ci(p, b"n't") && !ci(p, b"not") {
                    return Fallback;
                }
            }
            return Tag(0x0e);
        }
        if ci(w, b"job") {
            if !all_lower(w) {
                if !has(1) {
                    return Fallback;
                }
                return if q(n1, b"hunting") { Tag(0x13) } else { Fallback };
            }
            return Tag(0x13);
        }
        if !c.e09 && ci(w, b"laden") {
            if has(1) && q(n1, b"'s") {
                return Tag(0x14);
            }
            if i < 1 || !q(p1, b"bin") {
                if i != 0 || cls(first(c.sp(0, 0).unwrap_or(b""))) & 0x80 == 0 {
                    return Tag(0x0e);
                }
                return Tag(0x14);
            }
            return Tag(0x14);
        }
        if ci(w, b"learned") {
            if i + 1 != n {
                if !has(1) {
                    return Fallback;
                }
                let x = n1.unwrap();
                if !ci(x, b".") && !ci(x, b",") && !ci(x, b"!") {
                    return if ci(x, b"?") { Tag(b'&') } else { Fallback };
                }
            }
            return Tag(b'&');
        }
        if ci(w, b"lied") {
            if i < 1 || cls(first(p1.unwrap())) & 0x80 == 0 || first(w) != b'l' {
                if has(1) {
                    let x = n1.unwrap();
                    let list: [&[u8]; 20] = [
                        b"about", b"along", b"around", b"at", b"back", b"before", b"behind", b"by", b"close", b"down", b"in",
                        b"into", b"off", b"on", b"out", b"over", b"to", b"up", b"with", b"",
                    ];
                    if list[..19].iter().any(|s| ci(x, s)) {
                        return Tag(b'&');
                    }
                }
                if i < 1 {
                    return Fallback;
                }
                let p = p1.unwrap();
                if !c.advfreq(p) {
                    if !ci(p, b"have") && !ci(p, b"had") && !ci(p, b"been") {
                        return Fallback;
                    }
                    return Tag(b'(');
                }
                return Tag(b'&');
            }
            return Tag(b'&');
        }
        if ci(w, b"live") || ci(w, b"lives") {
            return live_rule(c, i, w);
        }
        if ci(w, b"minute") {
            if has(1) && q(n1, b"amount") {
                return Tag(0x0e);
            }
            if i > 0 {
                let p = p1.unwrap();
                if ci(p, b"the") || ci(p, b"a") {
                    return Tag(0x13);
                }
            }
            if has(2) && q(n1, b"by") && q(n2, b"minute") {
                return Tag(0x13);
            }
            if i < 2 || !q(p1, b"by") {
                return Fallback;
            }
            return if q(p2, b"minute") { Tag(0x13) } else { Fallback };
        }
        if ci(w, b"mouth") {
            if !has(2) || !q(n1, b"to") || !q(n2, b"mouth") {
                if i < 2 || !q(p1, b"to") {
                    return Fallback;
                }
                return if q(p2, b"mouth") { Tag(0x13) } else { Fallback };
            }
            return Tag(0x13);
        }
        if ci(w, b"number") {
            let l5b77 = || -> R {
                if !has(1) || !q(n1, b"with") {
                    return Tag(0x13);
                }
                Tag(0x0f)
            };
            if i < 1 {
                return l5b77();
            }
            let p = p1.unwrap();
            if !ci(p, b"less") && !ci(p, b"more") {
                return l5b77();
            }
            return Tag(0x0f);
        }
        if ci(w, b"perfect") {
            if !has(1) {
                return Fallback;
            }
            let x = n1.unwrap();
            if ci(x, b".") || ci(x, b",") || ci(x, b"!") {
                return Tag(0x0e);
            }
            return if ci(x, b"?") { Tag(0x0e) } else { Fallback };
        }
        if !c.e09 && ci(w, b"polish") {
            if !has(1) {
                return Fallback;
            }
            return if q(n1, b"jewish") { Tag(0x0e) } else { Fallback };
        }
        if ci(w, b"present") {
            if !has(3) {
                return Fallback;
            }
            return if q(n2, b"to") { Tag(b'%') } else { Fallback };
        }
        if ci(w, b"reading") {
            if i < 1 {
                return Fallback;
            }
            return if q(p1, b"brain") { Tag(0x13) } else { Fallback };
        }
        if ci(w, b"record") || ci(w, b"records") {
            let l62bf = || -> Option<R> {
                if !has(1) || (!q(n1, b"in") && !q(n1, b"with")) {
                    return Some(Fallback);
                }
                None
            };
            if i < 1 {
                if let Some(r) = l62bf() {
                    return r;
                }
            } else {
                if c.subj(p1.unwrap()) {
                    return Tag(b'%');
                }
                if !in_list(&L_NUMWORD, p1.unwrap()) {
                    if let Some(r) = l62bf() {
                        return r;
                    }
                }
            }
            return if len_code(w) != -8 { Tag(0x16) } else { Tag(0x13) };
        }
        if ci(w, b"refuse") || ci(w, b"refuses") {
            if !has(1) || !q(n1, b"to") {
                return Fallback;
            }
            return if len_code(w) != -8 { Tag(b'*') } else { Tag(b'%') };
        }
        if !c.e09 && (ci(w, b"resume") || ci(w, b"resumes")) {
            let lc = len_code(w);
            if i < 2 || !has(1) {
                return Fallback;
            }
            let x = n1.unwrap();
            if (!ci(x, b".") && !ci(x, b",") && !ci(x, b"!") && !ci(x, b"?"))
                || bsearch_ci(&L_BE, p1.unwrap()) != -1
                || bsearch_ci(&L_WH, p2.unwrap()) < 0
            {
                return Fallback;
            }
            return if lc == -8 { Tag(b'%') } else { Tag(b'*') };
        }
        if ci(w, b"separate") || ci(w, b"separates") {
            if !c.e09 && i > 0 && (q(p1, b"be") || word_class(None, p1, None) == 3) {
                return Tag(0x0e);
            }
            let nx_ok = has(1) && {
                let x = n1.unwrap();
                ci(x, b"from") || ci(x, b"into") || ci(x, b"out") || ci(x, b"up")
            };
            let n2_ok = has(2) && {
                let x = n2.unwrap();
                ci(x, b"from") || ci(x, b"into") || ci(x, b"and")
            };
            if !nx_ok && !n2_ok {
                return Fallback;
            }
            return if len_code(w) == -10 { Tag(b'%') } else { Tag(b'*') };
        }
        if !ci(w, b"subject") {
            if ci(w, b"supply") {
                if !has(1) || !in_list(&L_BE, n1.unwrap()) {
                    return Fallback;
                }
                return Tag(0x13);
            }
            if ci(w, b"transform") || ci(w, b"transforms") {
                if (!has(2) || !q(n2, b"into")) && (!has(3) || !q(c.sp(i, 3), b"into")) {
                    return Fallback;
                }
                return if len_code(w) == -0xb { Tag(b'%') } else { Tag(b'*') };
            }
            if ci(w, b"use") {
                if i < 1 {
                    return Fallback;
                }
                return if q(p1, b"still") { Tag(b'%') } else { Fallback };
            }
            if ci(w, b"wind") {
                if !has(1) {
                    return Fallback;
                }
                let x = n1.unwrap();
                if ci(x, b"up") || ci(x, b"down") {
                    return Tag(b'%');
                }
                return if ci(x, b"through") { Tag(b'%') } else { Fallback };
            }
            if !ci(w, b"wound") || !has(1) {
                return Fallback;
            }
            let x = n1.unwrap();
            if !ci(x, b"up") && !ci(x, b"down") && !ci(x, b"through") {
                return Fallback;
            }
            return Tag(b'&');
        }
        // subject
        if !has(1) || !q(n1, b"to") {
            return Fallback;
        }
        if i > 0 && in_list(&L_BE, p1.unwrap()) {
            return Tag(0x0e);
        }
        if i > 1 && in_list(&L_BE, p2.unwrap()) {
            let p = p1.unwrap();
            if ci(p, b"n't") || ci(p, b"not") {
                return Tag(0x0e);
            }
        }
        return Tag(b'%');
    }
    // elaborate
    if !has(1) {
        return Fallback;
    }
    if q(n1, b"on") {
        Tag(b'%')
    } else {
        Fallback
    }
}

/// The `live` / `lives` block of `FUN_100049b0` (LAB_10006392 ... LAB_10006559).
fn live_rule(c: &Cv, i: usize, w: &[u8]) -> R {
    use R::*;
    let n = c.n;
    let has = |k: usize| i + k < n;
    let q = |o: Option<&[u8]>, s: &[u8]| oci(o, s);
    let p1 = c.sp(i, -1);
    let p2 = c.sp(i, -2);
    let n1 = c.sp(i, 1);
    let n2 = c.sp(i, 2);
    let lc = -(w.len() as i32) - 2;
    let l6392 = || Tag(b'*');
    let l65f2 = || if lc != -6 { Tag(b'*') } else { Tag(b'%') };
    if lc == -7 && has(1) && q(n1, b"on") {
        if i > 0 {
            let p = p1.unwrap();
            if ci(p, b"put") || ci(p, b"lay") {
                return Tag(0x16);
            }
        }
        return l6392();
    }
    if !c.e09 && has(2) && q(n1, b"on") && q(n2, b"TV") {
        return if lc == -6 { Tag(0x0e) } else { Tag(0x16) };
    }
    let go_6559 = if has(1) {
        let x = n1.unwrap();
        let words: [&[u8]; 18] = [
            b"at", b"by", b"in", b"for", b"with", b"near", b"off", b"on", b"out", b"through", b"to", b"together", b"up",
            b"upon", b"down", b"", b"", b"",
        ];
        let listed = words[..15].iter().any(|s| ci(x, s));
        if listed {
            return l65f2();
        }
        true
    } else {
        true
    };
    let _ = go_6559;
    // LAB_10006559
    if has(2) {
        let x = n1.unwrap();
        let y = n2.unwrap();
        if (ci(x, b"it") && ci(y, b"up")) || (ci(x, b"up") && ci(y, b"to")) || (ci(x, b"to") && self_suffix(y)) {
            return l65f2();
        }
    }
    if has(1) && suffix_class(n1) == 4 {
        return if lc == -6 { Tag(b'%') } else { Tag(b'*') };
    }
    if i < 2 || !q(p2, b"Larry") || !q(p1, b"King") {
        if i < 1 {
            return Fallback;
        }
        let p = p1.unwrap();
        if !ci(p, b"Window") && !ci(p, b"Windows") && (c.e09 || !ci(p, b"xbox")) {
            if c.e09 || (!ci(p, b"the") && !ci(p, b"a")) {
                return Fallback;
            }
            return if lc != -6 { Tag(0x16) } else { Tag(0x0e) };
        }
    }
    if lc != -6 {
        Tag(0x16)
    } else {
        Tag(0x0e)
    }
}

/// `FUN_100102c0(s, "self", 4)`: ends in "self" (and is longer than it).
fn self_suffix(s: &[u8]) -> bool {
    s.len() > 4 && strnicmp(&s[s.len() - 4..], b"self", 4) == 0
}

/// `FUN_10010900`: indices of the first entries holding an adjective (JJ
/// class 2), adverb (8), verb (9/12) and noun (4/7) reading, and two flags.
struct TagFlags {
    part_noun: bool,
    vbd_other: bool,
    nn: i32,
    jj: i32,
    vb: i32,
    rb: i32,
}

fn tag_flags(l: &PosList) -> TagFlags {
    let mut f = TagFlags { part_noun: false, vbd_other: false, nn: -1, jj: -1, vb: -1, rb: -1 };
    for (k, (_, list)) in l.entries.iter().enumerate() {
        for &p in list {
            let cl = POS_CLASS[p as usize & 0x7f];
            if f.jj == -1 && cl == 2 {
                f.jj = k as i32;
            } else if f.rb == -1 && cl == 8 {
                f.rb = k as i32;
            } else if f.vb == -1 && (cl == 9 || cl == 0xc) {
                f.vb = k as i32;
            } else if f.nn == -1 && (cl == 4 || cl == 7) {
                f.nn = k as i32;
            }
        }
    }
    for (k, (_, list)) in l.entries.iter().enumerate() {
        if k as i32 == f.nn {
            continue;
        }
        for &p in list {
            let cl = POS_CLASS[p as usize & 0x7f];
            if f.nn < 0 || k as i32 == f.nn || (cl != 5 && cl != 6) {
                if f.vb >= 0 && k as i32 != f.vb && cl == 10 {
                    f.vbd_other = true;
                }
            } else {
                f.part_noun = true;
            }
        }
    }
    f
}

/// `FUN_10004300`: second rule set. Returns the chosen entry.
fn fallback(c: &Cv, i: usize, l: &PosList) -> Option<i32> {
    let n = c.n;
    let has = |k: usize| i + k < n;
    let q = |o: Option<&[u8]>, s: &[u8]| oci(o, s);
    let w = c.sp(i, 0).unwrap_or(b"");
    let p1 = c.sp(i, -1);
    let n1 = c.sp(i, 1);
    let some = |x: i32| if x >= 0 { Some(x) } else { None };
    if has(1) && !ci(w, b"nice") && q(n1, b"of") {
        for t in [0x13u8, 0x16, 0x14, 0x15] {
            let u = l.find(t);
            if u >= 0 {
                return Some(u);
            }
        }
    }
    if !c.e09 && has(1) && q(n1, b"'s") {
        for t in [0x14u8, 0x15, 0x13, 0x16] {
            let u = l.find(t);
            if u >= 0 {
                return Some(u);
            }
        }
    }
    let fl = tag_flags(l);
    let tag = |t: u8| some(l.find(t));
    let same_token_prev = i > 0 && c.token(i - 1) == c.token(i);
    if !same_token_prev {
        // LAB_100044a4
        if in_list(&L_782F4, w) && has(1) && strcmp(n1.unwrap(), b"-") == 0 && c.token(i) == c.token(i + 1) {
            return tag(0x0d);
        }
    }
    // LAB_10004503
    if i >= 1 {
        let p = p1.unwrap();
        if in_list(&L_78278, p) {
            if fl.jj >= 0 {
                return Some(fl.jj);
            }
            return some(fl.rb);
        }
        if !c.advfreq(p) {
            let v = p_910690(p);
            if v {
                return l460e(&fl, l);
            }
        } else {
            if !c.e09 && i > 2 && word_class(c.sp(i, -3), c.sp(i, -2), None) == 3 {
                let u = l.find(b'(');
                if u >= 0 {
                    return Some(u);
                }
            }
            let mut u = l.find(b'%');
            if u < 0 {
                u = l.find(b'*');
                if u < 0 {
                    return tag(b'&');
                }
            }
            return Some(u);
        }
    }
    // LAB_100045d6
    if i > 1 && word_class_e(c.e09, c.sp(i, -2), p1, Some(w)) == 7 {
        return l460e(&fl, l);
    }
    if has(1) && p_910790(n1.unwrap()) {
        if fl.vbd_other {
            return None;
        }
        return some(fl.vb);
    }
    let l47ee = || -> Option<i32> {
        if i < 2 || !aux_context(c.sp(i, -2), p1) {
            let how_prev = if i < 1 {
                false
            } else {
                let p = p1.unwrap();
                if in_list(&L_BEGIN, p) && w.len() > 5 && ci(&w[w.len() - 3..], b"ing") {
                    return some(l.find(b'\''));
                }
                ci(p, b"how")
            };
            if !how_prev && (i < 2 || !q(c.sp(i, -2), b"how")) {
                return None;
            }
            let u = l.find(b'(');
            if (u < 0 && l.find(0x0e) < 0) || !has(1) || bsearch_ci(&L_OBJPRON, w) < 0 {
                return None;
            }
            return tag(0x0e);
        }
        tag(b'%')
    };
    if i >= 1 {
        let p = p1.unwrap();
        if ci(p, b"have") || ci(p, b"has") || ci(p, b"had") {
            let u = l.find(b'(');
            if u >= 0 {
                return Some(u);
            }
        }
        if ci(p, b"been") {
            let u = l.find(b'(');
            if u >= 0 {
                return Some(u);
            }
        }
        if ci(p, b"to") {
            let u = l.find(b'%');
            if u >= 0 {
                return Some(u);
            }
        }
        if c.subj(p) && !ci(p, b"it") {
            let mut u = l.find(b'%');
            if u < 0 {
                u = l.find(b'&');
            }
            if u >= 0 {
                return Some(u);
            }
        }
        if !aux_context(Some(p), Some(b"")) {
            return l47ee();
        }
        return tag(b'%');
    }
    l47ee()
}

/// `FUN_10010690`.
fn p_910690(p: &[u8]) -> bool {
    ci(p, b"of") || suffix_class(Some(p)) == 3 || in_list(&L_DET, p) || in_list(&L_78250, p)
}

/// `FUN_10010790`.
fn p_910790(p: &[u8]) -> bool {
    ci(p, b"the") || in_list(&L_DET, p) || in_list(&L_POSSPRON, p) || in_list(&L_782E0, p)
}

fn l460e(fl: &TagFlags, l: &PosList) -> Option<i32> {
    if fl.part_noun {
        return None;
    }
    if fl.nn >= 0 {
        return Some(fl.nn);
    }
    let u = l.find(0x13);
    if u >= 0 {
        Some(u)
    } else {
        None
    }
}

/// `FUN_10006ae0`: poly.tree3 feature vector for class code `class`.
fn poly_features(c: &Cv, i: usize, class: i16) -> [i16; 15] {
    let s = |k: isize| c.sp(i, k);
    let w0 = s(0);
    [
        prep_code(s(-2)),
        prep_code(s(-1)),
        prep_code(s(1)),
        prep_code(s(2)),
        word_class_e(c.e09, s(-3), s(-2), s(-1)),
        word_class_e(c.e09, s(-2), s(-1), w0),
        word_class_e(c.e09, w0, s(1), s(2)),
        word_class_e(c.e09, s(1), s(2), s(3)),
        suffix_class(s(-2)),
        suffix_class(s(-1)),
        suffix_class(w0),
        suffix_class(s(1)),
        suffix_class(s(2)),
        class,
        (cls(first(w0.unwrap_or(b""))) & 0x80 != 0) as i16,
    ]
}

/// `FUN_100068b0`: choose the pronunciation of candidate `i` (which has at
/// least two). `n_tokens` is the WA word count. Returns the phone string.
pub fn choose_pron(c: &Cv, i: usize, n_tokens: usize, poly: &Tree) -> Vec<u8> {
    let w = c.sp(i, 0).unwrap_or(b"");
    if n_tokens < 2 && !ci(w, b"a") {
        return c.pron(i, 0);
    }
    if ci(w, b"a") {
        return vec![article_a(c, i)];
    }
    let pos_raw = {
        let o = Ws::cand(i) + cand::POS;
        let s = &c.ws.b[o..o + 0x14];
        let n = s.iter().position(|&b| b == 0xff).map(|p| p + 1).unwrap_or(s.len());
        s[..n].to_vec()
    };
    let mut l = PosList::parse(&pos_raw);
    match rules(c, i) {
        R::Tag(t) => {
            let k = l.find(t);
            if k >= 0 {
                return c.pron(i, k);
            }
            if let Some(k) = fallback(c, i, &l) {
                return c.pron(i, k);
            }
        }
        R::Fallback => {
            if let Some(k) = fallback(c, i, &l) {
                return c.pron(i, k);
            }
        }
    }
    let cnt = l.entries.len();
    for k in 0..cnt {
        let list = l.entries[k].1.clone();
        for p in list {
            let fv = poly_features(c, i, POS_CLASS[p as usize & 0x7f] as i8 as i16);
            let pred = poly.predict_scalar(&fv) as i8;
            for e in l.entries.iter_mut() {
                if PosList::has_class(&e.1, pred as i16) {
                    e.0 += 1;
                }
            }
        }
    }
    let (mut best, mut votes) = (0i32, 0i32);
    for (k, e) in l.entries.iter().enumerate() {
        if votes < e.0 {
            votes = e.0;
            best = k as i32;
        }
    }
    if ci(w, b"august") && votes == 0 && best == 0 {
        let t = if cls(first(w)) & 0x80 == 0 { 0x0e } else { 0x14 };
        best = l.find(t);
    }
    c.pron(i, best)
}
