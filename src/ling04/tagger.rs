//! Homograph pick of the 2004 engine (`FUN_10005720`): the hand-written rules (`FUN_10004900`, with the
//! article rule `FUN_100042f0` and the second set `FUN_10004600`), else `poly` tree votes over the
//! candidate context as in 2014 (`ling::tagger::choose_pron`) but without its short-sentence shortcut.

use super::tables::POS_CLASS;
use crate::lexicon::{stricmp, Lexicon};
use crate::ling::lookup::cand;
use crate::ling::tables::{CHAR_CLASS, L_78250, L_78278, L_782E0, L_BE, L_BEGIN, L_CONJ, L_DET, L_POSSPRON};
use crate::ling::tagger::{all_lower, aux_context, in_list, modal_pair, prep_code, suffix_class, word_class_e, PosList};
use crate::ling::{sc, Ws};

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

fn upper(s: &[u8]) -> bool {
    cls(first(s)) & 0x80 != 0
}

fn class(p: u8) -> i16 {
    POS_CLASS[p as usize & 0x7f]
}

/// `FUN_1000cf20`: first entry holding a tag of the same class as `tag`.
fn find(l: &PosList, tag: u8) -> i32 {
    let want = class(tag);
    l.entries.iter().position(|(_, e)| e.iter().any(|&p| class(p) == want)).map_or(-1, |k| k as i32)
}

/// `FUN_1000cdd0`.
fn has_class(list: &[u8], c: i16) -> bool {
    list.iter().any(|&p| class(p) == c)
}

struct Ctx<'a> {
    ws: &'a Ws,
    n: usize,
    i: usize,
}

impl<'a> Ctx<'a> {
    fn sp(&self, k: isize) -> Option<&'a [u8]> {
        let j = self.i as isize + k;
        if j < 0 || j >= self.n as isize {
            return None;
        }
        Some(self.ws.cstr(Ws::cand(j as usize) + cand::SPELLING))
    }

    /// Unchecked read of a following candidate's spelling (the engine reads past the last one).
    fn raw(&self, k: usize) -> &'a [u8] {
        self.ws.cstr(Ws::cand(self.i + k) + cand::SPELLING)
    }

    fn w(&self) -> &'a [u8] {
        self.sp(0).unwrap_or(b"")
    }

    fn has(&self, k: usize) -> bool {
        self.i + k < self.n
    }

    fn pron(&self, k: i32) -> Vec<u8> {
        self.ws.cstr(Ws::cand(self.i) + cand::PRON + k as usize * cand::PRON_STRIDE).to_vec()
    }
}

/// `FUN_100042f0`: pronunciation of "a" -- EY1 (0x1e) or AH0 (7).
fn article(c: &Ctx) -> u8 {
    let (i, n) = (c.i, c.n);
    let w = c.w();
    let prev = c.sp(-1);
    let next = c.sp(1);
    if i > 0 && (oci(prev, b"an") || oci(prev, b"the")) {
        return 0x1e;
    }
    if i + 1 < n {
        let nx = next.unwrap();
        if in_list(&L_CONJ, nx) {
            return 0x1e;
        }
        if (in_list(&L_BE, nx) || modal_pair(nx, Some(b"")) || ci(nx, b"of")) && (!ci(nx, b"can") || upper(w)) {
            return 0x1e;
        }
    }
    if i + 2 < n && modal_pair(next.unwrap(), c.sp(2)) {
        return 0x1e;
    }
    if (i == 0 || first(prev.unwrap_or(b"")) == b',') && i + 1 < n && first(next.unwrap()) == b',' {
        return 0x1e;
    }
    if i > 0 && i + 1 < n && upper(w) && all_lower(prev.unwrap()) && all_lower(next.unwrap()) {
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

/// The word rules of `FUN_10004900`: the tag to look for, or `None` for the second rule set.
fn word_rule(c: &Ctx) -> Option<u8> {
    let i = c.i;
    let w = c.w();
    let p1 = c.sp(-1);
    let p2 = c.sp(-2);
    let n1 = c.sp(1);
    let n2 = c.sp(2);
    let q = |o: Option<&[u8]>, s: &[u8]| oci(o, s);
    let any = |o: Option<&[u8]>, l: &[&[u8]]| o.is_some_and(|x| l.iter().any(|s| ci(x, s)));
    let star_pct = |len: usize| if w.len() == len { b'%' } else { b'*' };

    if ci(w, b"anti") {
        return Some(if c.has(1) && in_list(&L_CONJ, n1.unwrap()) { 0x0e } else { 0x13 });
    }
    if ci(w, b"close") {
        if c.has(1) && any(n1, &[b"by", b"on", b"to", b"upon"]) {
            return Some(0x0e);
        }
        if c.has(2) && ((q(n1, b"at") && q(n2, b"hand")) || (q(n1, b"in") && q(n2, b"with"))) {
            return Some(0x0e);
        }
        if c.has(1) && any(n1, &[b"in", b"up"]) {
            return Some(b'%');
        }
        if !c.has(2) {
            return None;
        }
        if (q(n1, b"it") && q(n2, b"up")) || (q(n1, b"down") && q(n2, b"on")) {
            return Some(b'%');
        }
        return None;
    }
    if ci(w, b"closer") {
        if c.has(1) && (any(n1, &[b"than", b"by", b"on", b"to", b"upon"]) || suffix_class(n1) == 2) {
            return Some(0x0f);
        }
        if !c.has(2) {
            return None;
        }
        if (q(n1, b"at") && q(n2, b"hand")) || (q(n1, b"in") && q(n2, b"with")) {
            return Some(0x0f);
        }
        return None;
    }
    if ci(w, b"conflicts") {
        return if c.has(1) && q(n1, b"with") { Some(b'*') } else { None };
    }
    if ci(w, b"de") {
        if (i < 1 || !upper(p1.unwrap())) && (!c.has(1) || !upper(n1.unwrap())) {
            return None;
        }
        return Some(0x0c);
    }
    if ci(w, b"dove") {
        return if c.has(1) && q(n1, b"into") { Some(b'&') } else { None };
    }
    if ci(w, b"elaborate") {
        return if c.has(1) && q(n1, b"on") { Some(b'%') } else { None };
    }
    if ci(w, b"house") {
        if (i > 0 && upper(p1.unwrap())) || (c.has(1) && upper(n1.unwrap())) {
            return Some(0x13);
        }
        if i < 1 || !c.has(1) || first(w) != b'H' {
            return None;
        }
        if (!q(p1, b"the") && !q(p1, b"a")) || cls(first(n1.unwrap())) & 0x40 == 0 {
            return None;
        }
        return Some(0x13);
    }
    if ci(w, b"increase") {
        return if c.has(1) && q(n1, b"in") { Some(0x13) } else { None };
    }
    if ci(w, b"interstate") {
        if !c.has(1) || !upper(w) || !upper(n1.unwrap()) {
            return None;
        }
        return Some(0x0e);
    }
    if ci(w, b"interstates") {
        return if c.has(1) && q(n1, b"to") { Some(star_pct(10)) } else { None };
    }
    if ci(w, b"live") || ci(w, b"lives") {
        let first_list: [&[u8]; 9] = [b"at", b"by", b"down", b"for", b"in", b"off", b"on", b"out", b"through"];
        if !(c.has(1) && any(n1, &first_list)) {
            let x = c.raw(1);
            if ![&b"to"[..], b"together", b"up", b"upon", b"with"].iter().any(|s| ci(x, s)) {
                if !c.has(2) {
                    return None;
                }
                let y = n2.unwrap();
                if !(ci(x, b"it") && ci(y, b"up")) && !(ci(x, b"up") && ci(y, b"to")) {
                    if !ci(x, b"to") || y.len() < 5 || !ci(&y[y.len() - 4..], b"self") {
                        return None;
                    }
                }
            }
        }
        return Some(star_pct(4));
    }
    if ci(w, b"number") {
        if i >= 1 && (q(p1, b"less") || q(p1, b"more")) {
            return Some(0x0f);
        }
        return Some(if c.has(1) && q(n1, b"with") { 0x0f } else { 0x13 });
    }
    if ci(w, b"present") {
        return if c.has(3) && q(n2, b"to") { Some(b'%') } else { None };
    }
    if ci(w, b"refuse") || ci(w, b"refuses") {
        return if c.has(1) && q(n1, b"to") { Some(star_pct(6)) } else { None };
    }
    if ci(w, b"record") || ci(w, b"records") {
        if i < 1 || !in_list(&crate::ling::tables::L_NUMWORD, p1.unwrap()) {
            if !c.has(1) || !any(n1, &[b"in", b"with"]) {
                return None;
            }
        }
        return Some(if w.len() == 6 { 0x13 } else { 0x16 });
    }
    if ci(w, b"separate") || ci(w, b"separates") {
        if !(c.has(1) && any(n1, &[b"from", b"into", b"out", b"up"])) {
            if !c.has(2) || !any(n2, &[b"from", b"into", b"and"]) {
                return None;
            }
        }
        return Some(star_pct(8));
    }
    if ci(w, b"subject") {
        if !c.has(1) || !q(n1, b"to") {
            return None;
        }
        if i > 0 && in_list(&L_BE, p1.unwrap()) {
            return Some(0x0e);
        }
        if i > 1 && in_list(&L_BE, p2.unwrap()) && (q(p1, b"n't") || q(p1, b"not")) {
            return Some(0x0e);
        }
        return Some(b'%');
    }
    if ci(w, b"transform") || ci(w, b"transforms") {
        if (!c.has(2) || !q(n2, b"into")) && (!c.has(3) || !q(c.sp(3), b"into")) {
            return None;
        }
        return Some(star_pct(9));
    }
    if ci(w, b"wind") {
        return if c.has(1) && any(n1, &[b"up", b"down", b"through"]) { Some(b'%') } else { None };
    }
    if ci(w, b"wound") && c.has(1) && any(n1, &[b"up", b"down", b"through"]) {
        return Some(b'&');
    }
    None
}

/// `FUN_1000d180`: as 2014 `FUN_10010900`, but an adjective reading also marks `part_noun`.
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
            let cl = class(p);
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
            let cl = class(p);
            if f.nn < 0 || (cl != 5 && cl != 6 && cl != 2) {
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

/// `FUN_10004600`: second rule set. Returns the chosen entry.
fn second_rules(c: &Ctx, l: &PosList) -> Option<i32> {
    let i = c.i;
    let w = c.w();
    let p1 = c.sp(-1);
    let n1 = c.sp(1);
    let some = |x: i32| if x >= 0 { Some(x) } else { None };
    if c.has(1) && oci(n1, b"of") {
        for t in [0x13u8, 0x16, 0x14, 0x15] {
            let u = find(l, t);
            if u >= 0 {
                return Some(u);
            }
        }
    }
    let fl = tag_flags(l);
    if i >= 1 {
        let p = p1.unwrap();
        if in_list(&L_78278, p) {
            return if fl.jj >= 0 { Some(fl.jj) } else { some(fl.rb) };
        }
        // FUN_1000cf80
        if ci(p, b"of") || suffix_class(Some(p)) == 3 || in_list(&L_DET, p) || in_list(&L_78250, p) {
            if fl.part_noun {
                return None;
            }
            return some(fl.nn);
        }
    }
    // FUN_1000d050
    if c.has(1) {
        let x = n1.unwrap();
        if ci(x, b"the") || in_list(&L_DET, x) || in_list(&L_POSSPRON, x) || in_list(&L_782E0, x) {
            return if !fl.vbd_other && fl.vb >= 0 { Some(fl.vb) } else { None };
        }
    }
    let aux = (i >= 1 && aux_context(p1, Some(b""))) || (i >= 2 && aux_context(c.sp(-2), p1));
    if aux {
        return some(find(l, b'%'));
    }
    if i < 1 || !in_list(&L_BEGIN, p1.unwrap()) || w.len() < 6 || !ci(&w[w.len() - 3..], b"ing") {
        return None;
    }
    some(find(l, b'\''))
}

/// `FUN_10005910`: poly tree feature vector for class code `class`.
fn poly_features(c: &Ctx, class: i16) -> [i16; 15] {
    let s = |k: isize| c.sp(k);
    let w0 = s(0);
    [
        prep_code(s(-2)),
        prep_code(s(-1)),
        prep_code(s(1)),
        prep_code(s(2)),
        word_class_e(true, s(-3), s(-2), s(-1)),
        word_class_e(true, s(-2), s(-1), w0),
        word_class_e(true, w0, s(1), s(2)),
        word_class_e(true, s(1), s(2), s(3)),
        suffix_class(s(-2)),
        suffix_class(s(-1)),
        suffix_class(w0),
        suffix_class(s(1)),
        suffix_class(s(2)),
        class,
        upper(w0.unwrap_or(b"")) as i16,
    ]
}

/// `FUN_10005720`: pronunciation of candidate `i` (which has at least two) among `n` candidates.
pub fn choose_pron(ws: &Ws, n: usize, i: usize, lex: &Lexicon) -> Vec<u8> {
    let c = Ctx { ws, n, i };
    let w = c.w();
    let pos_raw = {
        let o = Ws::cand(i) + cand::POS;
        let s = &ws.b[o..o + 0x14];
        let k = s.iter().position(|&b| b == 0xff).map(|p| p + 1).unwrap_or(s.len());
        s[..k].to_vec()
    };
    let mut l = PosList::parse(&pos_raw);
    if ci(w, b"a") {
        return vec![article(&c)];
    }
    if let Some(t) = word_rule(&c) {
        let k = find(&l, t);
        if k >= 0 {
            return c.pron(k);
        }
    }
    if let Some(k) = second_rules(&c, &l) {
        return c.pron(k);
    }
    for k in 0..l.entries.len() {
        let list = l.entries[k].1.clone();
        for p in list {
            let fv = poly_features(&c, class(p) as u8 as i8 as i16);
            let pred = lex.poly.predict_scalar(&fv) as i8 as i16;
            for e in l.entries.iter_mut() {
                if has_class(&e.1, pred) {
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
        best = find(&l, if upper(w) { 0x14 } else { 0x0e });
    }
    c.pron(best)
}
