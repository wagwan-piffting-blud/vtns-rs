//! The Spanish (Violeta) dictionaries in `data-common/dict-spa`: the
//! perfect-hashed main lexicon (`hashparams`/`hashcont`/`hashidx` + `spattsdict`,
//! no tag suffix -- unlike English's `_emb`), the TPP tagger dictionary
//! (`hashparams_spa_tpp`/`hashcont_spa_tpp`/`hashidx_spa_tpp` + `tppdict_spa`)
//! and `wab.txt2` (same "nlp"-magic format as English's, reused via
//! `crate::lexicon::Txt2`). Same perfect-hash algorithm as English
//! (`dict_JenkinsLookup2`/`dict_PerfectHashSlot`/`dict_LookupLexicon` in
//! Violeta are structurally identical to their English counterparts per the
//! funcmap and decompile diff), reimplemented here because the file-naming
//! convention differs (`crate::lexicon::HashDict::open` assumes an
//! `_{tag}`-suffixed pair of files that doesn't fit the untagged Spanish
//! lexicon files).
//!
//! Char classification/case tables (`CHAR_CLASS_SPA`, `TOLOWER_SPA`,
//! `HIBIT_SPA`) are raw dumps of Violeta's own `DAT_10065364`/`DAT_10065564`/
//! `DAT_100680d4` (256-entry tables), read via Ghidra MCP and transcribed
//! byte for byte -- used as opaque lookup tables (their bit layout wasn't
//! fully reverse engineered), not reimplemented from first principles.

use std::path::Path;

use crate::lexicon::lookup2;

include!("tables_spa_chars.rs");

/// `FUN_100259c0`: lowercase + accent-fold a word for lexicon key encoding
/// (ASCII via `CHAR_CLASS_SPA`/`TOLOWER_SPA`; high bytes >= 0x80 either
/// follow the same tables when `HIBIT_SPA[c] & 0x80 == 0`, or use a
/// generic Windows-1252 case shift: +0x10 for 0x8a/0x8e/0x9f, else +0x20).
pub fn encode_key_spa(word: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(word.len());
    for &c0 in word {
        let c = if c0 < 0x80 || HIBIT_SPA[c0 as usize] & 0x80 == 0 {
            if CHAR_CLASS_SPA[c0 as usize] & 0x80 != 0 {
                TOLOWER_SPA[c0 as usize]
            } else {
                c0
            }
        } else if matches!(c0, 0x8a | 0x8e | 0x9f) {
            c0.wrapping_add(0x10)
        } else {
            c0.wrapping_add(0x20)
        };
        out.push(c);
    }
    out
}

/// `FUN_10026080`: count of Spanish vowels (a/e/i/o/u plus accented
/// á/é/í/ó/ú and dieresis ü), case-insensitive via `TOLOWER_SPA`.
pub fn vowel_count_spa(word: &[u8]) -> i32 {
    let mut n = 0i32;
    for &c in word {
        let lo = TOLOWER_SPA[c as usize];
        if matches!(lo, b'a' | b'e' | b'i' | b'o' | b'u' | 0xe1 | 0xe9 | 0xed | 0xf3 | 0xfa | 0xfc) {
            n += 1;
        }
    }
    n
}

/// `FUN_10025a60`: every character of `word` is uppercase
/// (`CHAR_CLASS_SPA[c] & 0x80`); false for an empty string.
pub fn all_upper_spa(word: &[u8]) -> bool {
    !word.is_empty() && word.iter().all(|&c| CHAR_CLASS_SPA[c as usize] & 0x80 != 0)
}

/// `FUN_1001c320`-equivalent case-insensitive strcmp using `TOLOWER_SPA`.
pub fn stricmp_spa(a: &[u8], b: &[u8]) -> i32 {
    let at = |s: &[u8], i: usize| *s.get(i).unwrap_or(&0);
    let mut i = 0usize;
    loop {
        let ca = at(a, i);
        let cb = at(b, i);
        let d = TOLOWER_SPA[ca as usize] as i32 - TOLOWER_SPA[cb as usize] as i32;
        if d != 0 {
            return d;
        }
        if ca == 0 {
            return 0;
        }
        i += 1;
    }
}

/// `FUN_10007110`: case-insensitive compare of at most `n` characters.
pub fn strnicmp_spa(a: &[u8], b: &[u8], n: usize) -> i32 {
    let at = |s: &[u8], i: usize| *s.get(i).unwrap_or(&0);
    for i in 0..n {
        let (ca, cb) = (at(a, i), at(b, i));
        let d = TOLOWER_SPA[ca as usize] as i32 - TOLOWER_SPA[cb as usize] as i32;
        if d != 0 || ca == 0 {
            return d;
        }
    }
    0
}

fn eq_ci(a: &[u8], b: &[u8]) -> bool {
    stricmp_spa(a, b) == 0
}

fn starts_with_ci(word: &[u8], prefix: &[u8]) -> bool {
    word.len() >= prefix.len() && stricmp_spa(&word[..prefix.len()], prefix) == 0
}

/// The 10 "w-initial foreign word" exceptions checked verbatim in
/// `FUN_10021bf0` (`s_watercloset_10067d24` .. `s_wolfram_10067ccc`,
/// case-insensitive whole-word compare), used to set the `w_foreign_ctx`
/// flag passed to the LTS letter-to-sound stub.
pub const W_FOREIGN_WORDS: [&[u8]; 10] =
    [b"watercloset", b"wagneriano", b"walhalla", b"walkiria", b"watt", b"walquiria", b"wendo", b"wag\xf3n", b"wal\xf3n", b"wolfram"];

/// The 5 Mexican-place-name prefix/whole-word checks (`s_mexic_10067d50` ..
/// `s_oaxaq_10067d30`) that set the `mexican_ctx` flag. `M\xe9xico` is a
/// whole-word compare; the other four are 5-char prefix compares.
pub fn mexican_ctx_spa(word: &[u8]) -> bool {
    starts_with_ci(word, b"mexic") || eq_ci(word, b"m\xe9xico") || starts_with_ci(word, b"mexiq") || starts_with_ci(word, b"oaxac") || starts_with_ci(word, b"oaxaq")
}

pub fn w_foreign_ctx_spa(word: &[u8]) -> bool {
    W_FOREIGN_WORDS.iter().any(|w| eq_ci(word, w))
}

/// `PTR_DAT_10068410` (48 entries, sorted ascending case-insensitively):
/// every finite conjugated form of "haber", used by the TPP pass as a
/// fallback class-0x3d marker when a word isn't in the TPP dict directly.
/// Extracted from Violeta's own `.rdata` (Ghidra MCP memory read at
/// 0x10068410/0x10068974), not reconstructed from a grammar -- order
/// matches the binary's own (ascending, case-insensitive) so the binary
/// search below matches `FUN_10043680`'s exactly.
pub const HABER_FORMS: [&[u8]; 48] = [
    b"ha", b"habr\xe9mos", b"habr\xe1n", b"habr\xe1s", b"habr\xe1", b"habr\xe9is", b"habr\xe9", b"habr\xedais", b"habr\xedamos", b"habr\xedan",
    b"habr\xedas", b"habr\xeda", b"hab\xe9is", b"hab\xedais", b"hab\xeda", b"hab\xedamos", b"hab\xedan", b"hab\xedas", b"han", b"has",
    b"haya", b"hayamos", b"hayan", b"hayas", b"hay\xe1is", b"he", b"hemos", b"hube", b"hubiera", b"hubierais",
    b"hubieran", b"hubieras", b"hubiere", b"hubiereis", b"hubieren", b"hubieres", b"hubiese", b"hubieseis", b"hubiesen", b"hubieses",
    b"hubieron", b"hubimos", b"hubiste", b"hubisteis", b"hubi\xe9ramos", b"hubi\xe9remos", b"hubi\xe9semos", b"hubo",
];

fn haber_find(word: &[u8]) -> i32 {
    if word.is_empty() {
        return -1;
    }
    let (mut lo, mut hi) = (0i32, HABER_FORMS.len() as i32 - 1);
    let mut mid = hi;
    while lo <= hi {
        mid /= 2;
        let c = stricmp_spa(HABER_FORMS[mid as usize], word);
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

/// A perfect-hashed dictionary (untagged file trio + a data file), same
/// algorithm as `crate::lexicon::HashDict` (`dict_JenkinsLookup2` /
/// `dict_PerfectHashSlot` / `dict_LookupLexicon`) but constructed from
/// explicit file names since Violeta's main lexicon has no `_emb`-style tag.
pub struct HashDictSpa {
    seed: u32,
    shift: u32,
    tabsize: u32,
    f4: u32,
    pub nkeys: u32,
    scramble: Vec<u32>,
    tab: Vec<u8>,
    cont: Vec<u8>,
    maxrec: usize,
    offs: Vec<u32>,
    data: Vec<u8>,
}

fn rd32(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes([b[p], b[p + 1], b[p + 2], b[p + 3]])
}

fn bad(msg: String) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, msg)
}

impl HashDictSpa {
    pub fn open(hashparams: &Path, hashcont: &Path, hashidx: &Path, data: &Path) -> std::io::Result<HashDictSpa> {
        let hp = std::fs::read(hashparams)?;
        let o = if hp.starts_with(b"NLP") { 3 } else { 0 };
        if hp[o] != 2 {
            return Err(bad(format!("{}: type {} unsupported", hashparams.display(), hp[o])));
        }
        let f = |i: usize| rd32(&hp, o + 1 + 4 * i);
        let (seed, shift, tabsize, f4, nkeys, nscr) = (f(0), f(2), f(3), f(4), f(5), f(6) as usize);
        let mut p = o + 0x1d;
        let scramble = (0..nscr).map(|i| rd32(&hp, p + 4 * i)).collect();
        p += 4 * nscr;
        let ntab = rd32(&hp, p) as usize;
        let tab = hp[p + 4..p + 4 + ntab].to_vec();
        let cont = std::fs::read(hashcont)?;
        let idx = std::fs::read(hashidx)?;
        let maxrec = rd32(&idx, 0) as usize;
        let offs = (0..(idx.len() - 4) / 4).map(|i| rd32(&idx, 4 + 4 * i)).collect();
        let data = std::fs::read(data)?;
        Ok(HashDictSpa { seed, shift, tabsize, f4, nkeys, scramble, tab, cont, maxrec, offs, data })
    }

    pub fn slot(&self, key: &[u8]) -> u32 {
        let val = lookup2(key, self.seed);
        if self.f4 < 2 {
            return 0;
        }
        let t = self.tab[(val & (self.tabsize - 1)) as usize] as u32;
        let sh = val >> (self.shift & 31);
        if self.tabsize < 0x1000 {
            t ^ sh
        } else {
            self.scramble[t as usize] ^ sh
        }
    }

    pub fn record(&self, s: u32) -> (&[u8], &[u8]) {
        let off = self.offs[s as usize] as usize;
        let end = (off + self.maxrec).min(self.data.len());
        let rec = &self.data[off..end];
        let kn = rec.iter().position(|&c| c == 0).unwrap_or(rec.len());
        let rest = &rec[(kn + 1).min(rec.len())..];
        let pn = rest.iter().position(|&c| c == 0).unwrap_or(rest.len());
        (&rec[..kn], &rest[..pn])
    }

    pub fn get(&self, enc: &[u8]) -> Option<&[u8]> {
        if enc.is_empty() {
            return None;
        }
        let s = self.slot(enc);
        if s >= self.nkeys {
            return None;
        }
        let clen = self.cont[2 * s as usize] as i8 as i32;
        if clen != enc.len() as i32 || self.cont[2 * s as usize + 1] != enc[0] {
            return None;
        }
        let (k, p) = self.record(s);
        if k == enc {
            Some(p)
        } else {
            None
        }
    }
}

/// `FUN_100217f0` (`dict_DecodeLexPayload`-equivalent): the two fields
/// `FUN_10021bf0` actually reads out of a decoded payload. A third field
/// (a numeric value from `FUN_100505b3` = `atoi`, when bit 0x02 is set) is
/// decoded by the real engine too but never consumed by `FUN_10021bf0`, so
/// it isn't kept here.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LexEntrySpa {
    /// 0 (unset), 'A' (bit 0x08) or 'E' (bit 0x04, overrides 'A' if both set).
    pub class_char: u8,
    /// Only populated when bit 0x01 is set; empty otherwise.
    pub pron: Vec<u8>,
}

pub fn decode_payload_spa(payload: &[u8]) -> LexEntrySpa {
    let mut e = LexEntrySpa::default();
    let Some(&flags) = payload.first() else {
        return e;
    };
    if flags & 8 != 0 {
        e.class_char = b'A';
    }
    if flags & 4 != 0 {
        e.class_char = b'E';
    }
    if flags & 1 != 0 {
        // flags, pron, optionally 0xff + a decimal number (unused here).
        let rest = &payload[1..];
        let end = rest.iter().position(|&c| c == 0 || c == 0xff).unwrap_or(rest.len());
        e.pron = rest[..end].to_vec();
    }
    e
}

/// `dict_GetTppField` (Spanish `field_char`s are 'A' for the multi-word
/// join, 'B' for the single-word class -- different letters than English's
/// 'F'/'G', same payload-field-scan algorithm as
/// `ling::funcwords::tpp_field_in_payload`).
pub fn tpp_field_in_payload(payload: &[u8], letter: u8) -> Option<&[u8]> {
    let field = if payload.first() == Some(&letter) {
        &payload[1..]
    } else {
        let mut at = None;
        for i in 0..payload.len().saturating_sub(1) {
            if payload[i] == b' ' && payload[i + 1] == letter {
                at = Some(i + 2);
                break;
            }
        }
        &payload[at?..]
    };
    let end = field.iter().position(|&c| c == b' ').unwrap_or(field.len());
    Some(&field[..end])
}

/// `FUN_10064645`-equivalent: leading decimal digits of a TPP field as a byte.
pub fn parse_tpp_num(s: &[u8]) -> u8 {
    let mut n: u32 = 0;
    for &c in s {
        if !c.is_ascii_digit() {
            break;
        }
        n = n.wrapping_mul(10).wrapping_add((c - b'0') as u32);
    }
    n as u8
}

/// `FUN_10025cb0`: letter (or digraph `ch`/`ll`/`rr`, when called with a
/// real second character -- `spell_spa` always passes 0, so digraphs never
/// combine there and each letter of e.g. "ch" is spelled separately) index
/// into `SPELL_TABLE`, 0..29. `0xf1` ('\xf1', tested in the engine as a
/// signed byte -0xf) resolves to 0x10 without needing a second character.
pub fn letter_index_spa(c0: u8, c1: u8) -> Option<i32> {
    if c1 == 0 {
        return match c0 {
            b'a' => Some(0),
            b'b' => Some(1),
            b'c' => Some(2),
            b'd' => Some(4),
            b'e' => Some(5),
            b'f' => Some(6),
            b'g' => Some(7),
            b'h' => Some(8),
            b'i' => Some(9),
            b'j' => Some(10),
            b'k' => Some(0xb),
            b'l' => Some(0xc),
            b'm' => Some(0xe),
            b'n' => Some(0xf),
            b'o' => Some(0x11),
            b'p' => Some(0x12),
            b'q' => Some(0x13),
            b'r' => Some(0x14),
            b's' => Some(0x16),
            b't' => Some(0x17),
            b'u' => Some(0x18),
            b'v' => Some(0x19),
            b'w' => Some(0x1a),
            b'x' => Some(0x1b),
            b'y' => Some(0x1c),
            b'z' => Some(0x1d),
            0xf1 => Some(0x10),
            _ => None,
        };
    }
    match (c0, c1) {
        (b'c', b'h') => Some(3),
        (b'l', b'l') => Some(0xd),
        (b'r', b'r') => Some(0x15),
        _ => None,
    }
}

/// `DAT_100681d8`/`DAT_100681da`: per-letter phone-code sequence spelled
/// out for letters 0..29 (`letter_index_spa`'s order: a b c ch d e f g h i
/// j k l ll m n \xf1 o p q r rr s t u v w x y z).
pub const SPELL_TABLE: [&[u8]; 30] = [
    &[2],
    &[9, 15],
    &[41, 15],
    &[11, 15],
    &[12, 15],
    &[15],
    &[15, 20, 14],
    &[22, 15],
    &[2, 11, 14],
    &[24],
    &[22, 33, 43, 1],
    &[26, 2],
    &[15, 27, 14],
    &[15, 27, 27, 57],
    &[15, 28, 14],
    &[15, 29, 14],
    &[15, 31, 14],
    &[33],
    &[38, 15],
    &[26, 45],
    &[15, 39, 14],
    &[15, 40, 14],
    &[15, 41, 14],
    &[43, 15],
    &[45],
    &[9, 15],
    &[12, 33, 10, 27, 14, 100, 9, 15],
    &[15, 26, 23, 41],
    &[24, 100, 21, 39, 58, 21, 1],
    &[41, 15, 43, 1],
];

/// `FUN_10025a90` then `FUN_10025be0`: fold an accented/CP1252 byte to its
/// base ASCII letter for spelling purposes (letters aren't spelled "with
/// an accent"), except \xd1/\xf1 (\xd1/\xf1) which are kept as themselves.
fn accent_fold_for_spell(c: u8) -> u8 {
    let step1 = match c {
        0x8a | 0xc7 => 0x53,
        0x8e => 0x5a,
        0x9a | 0xe7 => 0x73,
        0x9e => 0x7a,
        0x9f | 0xdb | 0xdd => 0x59,
        0xc0 => 0xc1,
        0xc2..=0xc5 => 0x41,
        0xc8 => 0xc9,
        0xca | 0xcb => 0x45,
        0xcc => 0xcd,
        0xce | 0xcf => 0x49,
        0xd2 => 0xd3,
        0xd4..=0xd6 => 0x4f,
        0xd9 => 0xda,
        0xe0 => 0xe1,
        0xe2..=0xe5 => 0x61,
        0xe8 => 0xe9,
        0xea | 0xeb => 0x65,
        0xec => 0xed,
        0xee | 0xef => 0x69,
        0xf2 => 0xf3,
        0xf4..=0xf6 => 0x6f,
        0xf9 => 0xfa,
        0xfb | 0xfd | 0xff => 0x79,
        other => other,
    };
    match step1 {
        0x8a => 0x53,
        0x8e => 0x5a,
        0x9a => 0x73,
        0x9e => 0x7a,
        0x9f | 0xdd => 0x59,
        0xc0..=0xc5 => 0x41,
        0xc7 => 0x53,
        0xc8..=0xcb => 0x45,
        0xcc..=0xcf => 0x49,
        0xd1 => 0xd1,
        0xd2..=0xd6 => 0x4f,
        0xd9..=0xdc => 0x55,
        0xe0..=0xe5 => 0x61,
        0xe7 => 0x73,
        0xe8..=0xeb => 0x65,
        0xec..=0xef => 0x69,
        0xf1 => 0xf1,
        0xf2..=0xf6 => 0x6f,
        0xf9..=0xfc => 0x75,
        0xfd | 0xff => 0x79,
        other if other < 0x80 => other,
        _ => 0,
    }
}

/// `FUN_10025ec0`: `encode_key_spa` plus, for high bytes the table flags
/// with `HIBIT_SPA[c] & 0x40`, the accent fold above -- the key `spell_spa`
/// walks letter by letter.
fn spell_key_spa(word: &[u8]) -> Vec<u8> {
    let base = encode_key_spa(word);
    base.iter()
        .map(|&c| if c >= 0x80 && HIBIT_SPA[c as usize] & 0x40 != 0 { accent_fold_for_spell(c) } else { c })
        .collect()
}

/// `FUN_10025f60`: spell a word letter by letter. Every byte is looked at
/// once (`FUN_10025cb0` is always called with a NUL second argument here --
/// unlike `letter_index_spa`'s digraph form used elsewhere, this function
/// never combines `ch`/`ll`/`rr`); non-letter bytes (punctuation) are simply
/// skipped without emitting anything. A separator (phone code 100) is
/// inserted after a letter's phones only when more spellable content
/// follows -- either the very next byte is itself a letter, or the next
/// byte is one of `. _ - '` and the byte after *that* is a letter; anything
/// else (including hitting the word's end) stops spelling immediately
/// (the trailing punctuation itself is dropped, not spelled).
pub fn spell_spa(word: &[u8]) -> Vec<u8> {
    let key = spell_key_spa(word);
    let n = key.len();
    let mut out: Vec<u8> = Vec::new();
    if n == 0 {
        return out;
    }
    let is_letter = |k: usize| k < n && CHAR_CLASS_SPA[key[k] as usize] & 0xc0 != 0;
    let mut i = 0usize;
    while i < n {
        if is_letter(i) {
            let Some(idx) = letter_index_spa(key[i], 0) else {
                i += 1;
                continue;
            };
            for &ph in SPELL_TABLE[idx as usize] {
                if out.len() > 0x62 {
                    return out;
                }
                out.push(ph);
            }
            let next_ok = is_letter(i + 1) || (i + 1 < n && b".-_'".contains(&key[i + 1]) && is_letter(i + 2));
            if out.len() > 0x62 || i + 1 >= n || !next_ok || out.len() > 0x61 {
                break;
            }
            out.push(100);
        }
        i += 1;
    }
    out
}

/// `wab.txt2`: a sorted (case-insensitively ascending), CRLF-separated list
/// of Spanish function words, in a `"nlp"`-magic-bracketed format that
/// **differs from English's `wab.txt2`** (`crate::lexicon::Txt2`'s own
/// format, which this agent tried first and which parses to zero entries
/// here): the trailer here is just a single shift byte immediately before
/// the closing `"nlp"` magic (`raw[n-4]`), no separate ASCII-digit count
/// field -- `raw[3..n-4]`, each byte `wrapping_sub(raw[n-4])`, split on
/// `"\r\n"`, is the complete word list (117 entries; verified against
/// captured `TAG_B` values in `out_feling_spa`: "de" -> index 23 -> 24,
/// "la" -> 51 -> 52, "las" -> 52 -> 53, "para" -> 72 -> 73, all exact).
/// Kept as its own type (not reusing `crate::lexicon::Txt2`, whose parser
/// assumes the wrong trailer layout for this file).
pub struct WabList {
    pub words: Vec<Vec<u8>>,
}

impl WabList {
    pub fn open(path: &Path) -> std::io::Result<WabList> {
        let raw = std::fs::read(path)?;
        let n = raw.len();
        if n < 8 || raw[..3] != raw[n - 3..] {
            return Err(bad(format!("{}: not an nlp-magic file", path.display())));
        }
        let shift = raw[n - 4];
        let body: Vec<u8> = raw[3..n - 4].iter().map(|&b| b.wrapping_sub(shift)).collect();
        let words = body.split(|&c| c == b'\n').map(|w| w.iter().copied().filter(|&c| c != b'\r').collect::<Vec<u8>>()).filter(|w| !w.is_empty()).collect();
        Ok(WabList { words })
    }

    /// Case-insensitive (Spanish folding) binary search; the list is
    /// already sorted ascending the same way, matching `FUN_10021b70`'s
    /// own binary search over this list.
    pub fn find(&self, word: &[u8]) -> i32 {
        if word.is_empty() {
            return -1;
        }
        let (mut lo, mut hi) = (0i32, self.words.len() as i32 - 1);
        let mut mid = hi;
        while lo <= hi {
            mid /= 2;
            let c = stricmp_spa(&self.words[mid as usize], word);
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
}

/// All Spanish dictionaries the linguistic analysis stage reads.
pub struct LexiconSpa {
    pub emb: HashDictSpa,
    pub tpp: HashDictSpa,
    pub wab: WabList,
}

impl LexiconSpa {
    /// `dict_spa` = `all_voices/data-common/dict-spa`.
    pub fn open(dict_spa: &Path) -> std::io::Result<LexiconSpa> {
        Ok(LexiconSpa {
            emb: HashDictSpa::open(&dict_spa.join("hashparams"), &dict_spa.join("hashcont"), &dict_spa.join("hashidx"), &dict_spa.join("spattsdict"))?,
            tpp: HashDictSpa::open(
                &dict_spa.join("hashparams_spa_tpp"),
                &dict_spa.join("hashcont_spa_tpp"),
                &dict_spa.join("hashidx_spa_tpp"),
                &dict_spa.join("tppdict_spa"),
            )?,
            wab: WabList::open(&dict_spa.join("wab.txt2"))?,
        })
    }

    /// `dict_LookupLexicon(key, .., mode=1, ..)`: raw payload of an
    /// already-encoded key (mode 1 skips the dict's own encode step, which
    /// is why every caller in `FUN_10021bf0` runs `encode_key_spa` first).
    pub fn lookup_encoded(&self, enc: &[u8]) -> Option<&[u8]> {
        self.emb.get(enc)
    }

    pub fn lookup_payload(&self, word: &[u8]) -> Option<&[u8]> {
        self.emb.get(&encode_key_spa(word))
    }

    /// `FUN_10021b70(DAT_1008207c, word)`.
    pub fn wab_find(&self, word: &[u8]) -> i32 {
        self.wab.find(word)
    }

    pub fn haber_find(&self, word: &[u8]) -> i32 {
        haber_find(word)
    }

    /// `dict_GetTppField` field `letter` for `word` (`'A'` multi-word join,
    /// `'B'` single-word class).
    pub fn tpp_field(&self, word: &[u8], letter: u8) -> Option<Vec<u8>> {
        if word.is_empty() {
            return None;
        }
        let payload = self.tpp.get(&encode_key_spa(word))?;
        tpp_field_in_payload(payload, letter).map(|f| f.to_vec())
    }
}
