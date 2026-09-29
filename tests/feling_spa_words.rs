//! Batch 3 (Violeta): linguistic-front-end parity for the stages after
//! lexicon/LTS -- `fe_BuildWordPhoneRecords`, `fe_AssignPhraseBreaks`
//! (incl. the syllabifier) and `fe_BuildPhoneContexts` -- against Frida
//! captures. Mirrors `tests\feling_parity.rs`'s `record_stages`.
//!
//! Two capture sets are used:
//!   * `harness\out_feling_spa\*.json` (batch 1, 1366 files): `wa_out`,
//!     `words_out`, `breaks_out`, `ctx_out` per sentence, but no real
//!     syllable bytes (`hooks_feling_spa.js`'s `dumpSS` always emits
//!     `syl: []`) and no TIB pause/brk_level/kind7 arrays.
//!   * `harness\out_feling_spa_v3\*.json` (this batch, same
//!     1366 texts): a copy of `hooks_feling_spa.js` extended to also dump
//!     real syllable records (confirmed offsets word+0x008 pointer /
//!     word+0xda count, syllable STRESS/SHAPE/POSITION at +0/+1/+2) and the
//!     TIB per-word pause/brk_level/kind7 int[200] arrays -- see
//!     `harness\hooks_feling_spa_v3.js`. Used when
//!     present (preferred: it actually exercises the syllable-record and
//!     domain-byte comparisons); the harness set alone still exercises
//!     everything except syllables/kind7/tib.
//!
//!     cargo test --release --test feling_spa_words -- --nocapture

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use vtpaul::json::Json;
use vtpaul::ling::Tib;
use vtpaul::ling_spa::breaks::assign_phrase_breaks;
use vtpaul::ling_spa::context::build_phone_contexts;
use vtpaul::ling_spa::syllab::syl2;
use vtpaul::ling_spa::words::{build_word_phone_records, war2, wr2, TextPos, WordStringsSpa};
use vtpaul::ling_spa::{self, syl, wr, SsSpa, WaSpa, SYL_BASE, SYL_SIZE, WORD_SIZE};
use vtpaul::tree3::Tree;

/// Capture root; override at build time with `VTPAUL_HARNESS=<dir>`.
const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};
/// Voice data root; override at build time with `VTPAUL_VOICES=<dir>`.
const VOICES: &str = match option_env!("VTPAUL_VOICES") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};

fn v3_out() -> String {
    format!("{}/out_feling_spa_v3", HARNESS)
}

/// Word-record pointer fields to mask before comparing raw bytes (mirrors
/// `feling_parity.rs`'s `WORD_PTRS`): SYL_PTR, TARGET_LIST, TEXT_PTR,
/// PH_PTR.
const WORD_PTRS: [usize; 4] = [wr::SYL_PTR, wr::TARGET_LIST, wr2::TEXT_PTR, wr2::PH_PTR];

fn unhex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks(2)
        .map(|c| {
            let h = |x: u8| (x as char).to_digit(16).unwrap_or(0) as u8;
            h(c[0]) << 4 | h(c[1])
        })
        .collect()
}

fn load_wa(e: &Json) -> WaSpa {
    let mut wa = WaSpa::new();
    let head = unhex(e.get("head").str().unwrap());
    wa.b[..head.len()].copy_from_slice(&head);
    for (i, r) in e.get("recs").arr().iter().enumerate() {
        let b = unhex(r.str().unwrap());
        let o = WaSpa::rec(i);
        wa.b[o..o + b.len()].copy_from_slice(&b);
    }
    wa
}

/// Loads a captured SS snapshot's word records, re-tagging TEXT_PTR/PH_PTR
/// the way the real engine would have (same trick as English's `load_ss`):
/// the captured bytes hold the real engine's absolute pointers, which are
/// meaningless here, so they're overwritten with this port's own tagged
/// offsets into the loaded `WaSpa`.
fn load_ss(e: &Json, ss: &mut SsSpa) {
    let head = unhex(e.get("head").str().unwrap());
    ss.b[..head.len()].copy_from_slice(&head);
    for (i, w) in e.get("words").arr().iter().enumerate() {
        let b = unhex(w.get("raw").str().unwrap());
        let o = SsSpa::word(i);
        ss.b[o..o + b.len()].copy_from_slice(&b);
        ss.set_u32(o + wr2::TEXT_PTR, vtpaul::ling::ptr_wa(WaSpa::rec(i) + war2_text()));
        ss.set_u32(o + wr2::PH_PTR, vtpaul::ling::ptr_wa(WaSpa::rec(i) + war2::PHONES));
    }
}

// war::TEXT lives in the lead's `ling_spa::war` module.
fn war2_text() -> usize {
    ling_spa::war::TEXT
}

fn load_tib(e: &Json) -> Tib {
    let mut t = Tib::new();
    let tib = e.get("tib");
    if !tib.is_null() {
        for (name, dst) in [("pause", &mut t.pause), ("brk_level", &mut t.brk_level)] {
            for (i, v) in tib.get(name).arr().iter().enumerate() {
                dst[i] = v.i64().unwrap_or(-1) as i32;
            }
        }
    }
    t
}

fn kind7(e: &Json) -> Vec<i32> {
    let arr = e.get("tib").get("kind7").arr();
    arr.iter().map(|v| v.i64().unwrap_or(-1) as i32).collect()
}

fn masked_word(ss: &SsSpa, i: usize) -> Vec<u8> {
    let o = SsSpa::word(i);
    let mut v = ss.b[o..o + WORD_SIZE].to_vec();
    for p in WORD_PTRS {
        v[p..p + 4].fill(0);
    }
    v
}

fn syllables(ss: &SsSpa, i: usize) -> Vec<Vec<u8>> {
    let o = SsSpa::word(i);
    let n = ss.u8(o + wr::N_SYL) as usize;
    let p = ss.u32(o + wr::SYL_PTR);
    if p & 0xf000_0000 != 0x1000_0000 {
        return Vec::new();
    }
    let base = (p & !0xf000_0000) as usize;
    (0..n).map(|s| ss.b[base + s * SYL_SIZE..base + (s + 1) * SYL_SIZE].to_vec()).collect()
}

/// Only the syllable bytes `syllab::syllabify` itself writes (STRESS/SHAPE/
/// POSITION/N_PHONES); PHONES_PTR/POS_PTR are pointers into this port's own
/// word buffer and aren't expected to equal the real engine's raw pointer
/// bytes, and TONE/F0 belong to prosody (batch 2), not this stage.
fn syl_written(v: &[Vec<u8>]) -> Vec<Vec<u8>> {
    v.iter().map(|s| vec![s[syl2::STRESS], s[syl2::SHAPE], s[syl2::POSITION], s[syl::N_PHONES]]).collect()
}

#[derive(Default)]
struct Tally {
    ok: usize,
    total: usize,
    shown: usize,
}

impl Tally {
    fn report(&self, name: &str) {
        println!("{:14} {:5}/{:5} sentences exact", name, self.ok, self.total);
    }
}

/// Fields `build_word_phone_records` writes (everything else in the record
/// is stale or belongs to a later step).
fn compare_words(t: &mut Tally, tag: &str, ss: &SsSpa, want: &Json) {
    t.total += 1;
    let mut diffs: Vec<String> = Vec::new();
    for (i, w) in want.get("words").arr().iter().enumerate() {
        let b = unhex(w.get("raw").str().unwrap());
        let o = SsSpa::word(i);
        let n = b[wr::N_PHONES] as usize;
        let mut offs: Vec<usize> = vec![
            wr::N_PHONES,
            wr2::DOMAIN,
            wr2::SRC,
            wr2::CLASSCH,
            wr2::UNK53A,
            wr2::TOKEN_CLASS,
            wr2::TOKEN_CLASS + 1,
            wr2::X3AE,
            wr2::X3AE + 1,
        ];
        offs.extend((0..4).map(|k| wr::START + k));
        offs.extend((0..4).map(|k| wr::END + k));
        for k in 0..n {
            offs.extend([wr::PHONES + k, wr2::MARKS + k]);
        }
        for off in offs {
            if ss.u8(o + off) != b[off] {
                diffs.push(format!("w{}+{:x}: {:02x} want {:02x}", i, off, ss.u8(o + off), b[off]));
            }
        }
    }
    if diffs.is_empty() {
        t.ok += 1;
    } else if t.shown < 12 {
        t.shown += 1;
        println!("  [{} words] {} diffs: {}", tag, diffs.len(), diffs[..diffs.len().min(10)].join(" | "));
    }
}

/// Zeroes the per-phrase word/syllable pointer fields (`SsSpa::phrase(g)+8`
/// and `+0xc`, same relative layout as English) before comparing: both are
/// tagged offsets into *this* buffer, not the real engine's absolute
/// pointers.
fn masked_head(b: &[u8]) -> Vec<u8> {
    let mut v = b[..ling_spa::WORD_BASE].to_vec();
    for g in 0..100 {
        for p in [SsSpa::phrase(g) + 8, SsSpa::phrase(g) + 0xc] {
            if p + 4 <= v.len() {
                v[p..p + 4].fill(0);
            }
        }
    }
    v
}

/// Compare Rust SS state against the captured stage `want`; print the first
/// differences. `check_syl` is only meaningful against the v3 captures
/// (batch-1 captures never have real syllable bytes).
fn compare(t: &mut Tally, tag: &str, ss: &SsSpa, want: &Json, check_syl: bool) -> bool {
    t.total += 1;
    let mut diffs: Vec<String> = Vec::new();
    let head = masked_head(&unhex(want.get("head").str().unwrap()));
    let ours = masked_head(&ss.b);
    for (o, (a, b)) in ours.iter().zip(head.iter()).enumerate() {
        if a != b {
            diffs.push(format!("head+{:x}: {:02x} want {:02x}", o, a, b));
        }
    }
    for (i, w) in want.get("words").arr().iter().enumerate() {
        let mut b = unhex(w.get("raw").str().unwrap());
        for p in WORD_PTRS {
            b[p..p + 4].fill(0);
        }
        let a = masked_word(ss, i);
        for (o, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            if x != y {
                diffs.push(format!("w{}+{:x}: {:02x} want {:02x}", i, o, x, y));
            }
        }
        if check_syl {
            let ws: Vec<Vec<u8>> = w.get("syl").arr().iter().map(|s| unhex(s.str().unwrap())).collect();
            if !ws.is_empty() {
                let (ws, hs) = (syl_written(&ws), syl_written(&syllables(ss, i)));
                if ws != hs {
                    diffs.push(format!("w{} syl {:02x?} want {:02x?}", i, hs, ws));
                }
            }
        }
    }
    if diffs.is_empty() {
        t.ok += 1;
        return true;
    }
    if t.shown < 12 {
        t.shown += 1;
        println!("  [{}] {} diffs: {}", tag, diffs.len(), diffs[..diffs.len().min(10)].join(" | "));
    }
    false
}

fn captures(dir: &str) -> Vec<PathBuf> {
    let dir = Path::new(dir);
    if !dir.exists() {
        return Vec::new();
    }
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    v.sort();
    v
}

/// Group a capture's events into sentences keyed by kind (same convention
/// as `feling_parity.rs`'s `sentences()`: a new bucket starts at each
/// `wa_in`; a bucket that never gets a `words_out` -- because the engine's
/// per-sentence loop skipped an empty WA -- is simply missing that key and
/// gets skipped by the `let Some(...) = ... else { continue }` pattern
/// below).
fn sentences(doc: &Json) -> Vec<BTreeMap<String, Json>> {
    let mut out: Vec<BTreeMap<String, Json>> = Vec::new();
    for e in doc.get("events").arr() {
        let k = e.get("kind").str().unwrap_or("").to_string();
        if k == "wa_in" {
            out.push(BTreeMap::new());
        }
        if let Some(cur) = out.last_mut() {
            cur.insert(k, e.clone());
        }
    }
    out
}

/// `offset_maps` fires once per request, before the first sentence's
/// `wa_in`; `hooks_feling_spa.js` names the fields `map_b`/`map_c` (the
/// same TIB+0x82e64/+0x82e68 tables `fe_NextSentence` reads as
/// map_start/map_end -- see `words.rs`'s `TextPos`).
fn load_offset_maps(doc: &Json) -> (Vec<i32>, Vec<i32>) {
    for e in doc.get("events").arr() {
        if e.get("kind").str() == Some("offset_maps") {
            let get = |name: &str| -> Vec<i32> {
                e.get(name).arr().iter().map(|v| v.i64().unwrap_or(0) as i32).collect()
            };
            return (get("map_b"), get("map_c"));
        }
    }
    (Vec::new(), Vec::new())
}

fn run_record_stages(dir: &str, has_syl_tib: bool) {
    let caps = captures(dir);
    if caps.is_empty() {
        eprintln!("skipping {}: no captures", dir);
        return;
    }
    let spabi = Tree::load(&Path::new(VOICES).join("data-common").join("dict-spa").join("spabi.tree3")).unwrap();
    let mut t_words = Tally::default();
    let mut t_breaks = Tally::default();
    let mut t_ctx = Tally::default();
    for path in &caps {
        let doc = Json::load(path).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let mut cursor = 0i32;
        let (map_start, map_end) = load_offset_maps(&doc);
        for (si, s) in sentences(&doc).iter().enumerate() {
            let (Some(wa_out), Some(words), Some(brk), Some(ctx)) =
                (s.get("wa_out"), s.get("words_out"), s.get("breaks_out"), s.get("ctx_out"))
            else {
                continue;
            };
            let tag = format!("{}#{}", name, si);
            let wa = load_wa(wa_out);

            // fe_BuildWordPhoneRecords: start from the captured result with
            // the fields it writes cleared, rebuild them from scratch.
            let mut ss = SsSpa::new();
            load_ss(words, &mut ss);
            for i in 0..ss.n_words() {
                let w = SsSpa::word(i);
                for o in [wr::N_PHONES, wr2::DOMAIN, wr2::SRC, wr2::CLASSCH, wr2::UNK53A, wr2::TOKEN_CLASS, wr2::X3AE] {
                    ss.set_u8(w + o, 0);
                }
                ss.set_i32(w + wr::START, 0);
                ss.set_i32(w + wr::END, 0);
                ss.b[w + wr::PHONES..w + wr::PHONES + 0x41].fill(0);
                ss.b[w + wr2::MARKS..w + wr2::MARKS + 0x41].fill(0);
            }
            let k7 = kind7(words);
            let pos = TextPos {
                cursor,
                map_start: if map_start.is_empty() { None } else { Some(&map_start) },
                map_end: if map_end.is_empty() { None } else { Some(&map_end) },
            };
            build_word_phone_records(&mut ss, &wa, &k7, &pos);
            // WaSpa's header is only 0xc bytes (English's Wa::consumed() is
            // at header+4 of a 0x14-byte header); tentatively read the same
            // relative offset -- if wrong, the words-stage START/END diffs
            // below will show it directly.
            cursor += wa.i32(4);
            compare_words(&mut t_words, &tag, &ss, words);

            let mut ss = SsSpa::new();
            load_ss(words, &mut ss);
            let mut tib = load_tib(words);
            assign_phrase_breaks(&mut ss, &mut tib, &wa, &spabi);
            compare(&mut t_breaks, &tag, &ss, brk, has_syl_tib);

            let mut ss = SsSpa::new();
            load_ss(brk, &mut ss);
            // Re-point syllables the way syllabify laid them out: a running
            // sum of each captured syllable's own n_phones gives the same
            // PHONES_PTR/POS_PTR offsets `syllab::syllabify` would compute
            // (see `prosody_spa_parity.rs`'s `sentence_from_spa`, same
            // trick, for why the raw pointer bytes themselves aren't used).
            let mut base = SYL_BASE;
            for i in 0..ss.n_words() {
                let w = SsSpa::word(i);
                let n_syl = ss.u8(w + wr::N_SYL) as usize;
                ss.set_u32(w + wr::SYL_PTR, vtpaul::ling::ptr_ss(base));
                let mut first_phone = 0usize;
                for (k, sy) in brk.get("words").at(i).get("syl").arr().iter().enumerate().take(n_syl) {
                    let b = unhex(sy.str().unwrap());
                    let rec = base + k * SYL_SIZE;
                    ss.b[rec..rec + SYL_SIZE].copy_from_slice(&b);
                    ss.set_u32(rec + syl::PHONES_PTR, vtpaul::ling::ptr_ss(w + wr::PHONES + first_phone));
                    ss.set_u32(rec + syl::POS_PTR, vtpaul::ling::ptr_ss(w + wr::POS + first_phone));
                    first_phone += b[syl::N_PHONES] as usize;
                }
                base += n_syl * SYL_SIZE;
            }
            let strs = WordStringsSpa { wa: &wa };
            build_phone_contexts(&mut ss, &strs);
            compare(&mut t_ctx, &tag, &ss, ctx, false);
        }
    }
    t_words.report("words");
    t_breaks.report("breaks");
    t_ctx.report("contexts");
    assert_eq!(t_words.ok, t_words.total, "words mismatches in {}", dir);
    assert_eq!(t_breaks.ok, t_breaks.total, "breaks mismatches in {}", dir);
    assert_eq!(t_ctx.ok, t_ctx.total, "ctx mismatches in {}", dir);
}

#[test]
fn record_stages_v1() {
    run_record_stages(&format!("{}/out_feling_spa", HARNESS), false);
}

#[test]
fn record_stages_v3() {
    run_record_stages(&v3_out(), true);
}

/// The glue a later batch needs: `words::sentence_from_ss` /
/// `words::word_from_ss` build `prosody_spa::Sentence` / `unitsel::Word`
/// straight from a `SsSpa`. Proves they reproduce the same fields an
/// independent, from-scratch JSON parse of the same `breaks_out`/`ctx_out`
/// capture gets (mirroring `tests\prosody_spa_parity.rs`'s
/// `sentence_from_spa` and `tests\unitsel_spa_parity.rs`'s `words_of`,
/// without depending on those private test-binary functions).
#[test]
fn glue_matches_captures() {
    use vtpaul::ling_spa::words::{sentence_from_ss, word_from_ss};
    use vtpaul::unitsel::Word as UWord;

    let caps = captures(&v3_out());
    if caps.is_empty() {
        eprintln!("skipping: no captures in {}", v3_out());
        return;
    }
    let mut t_sent = Tally::default();
    let mut t_word = Tally::default();
    for path in &caps {
        let doc = Json::load(path).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        for (si, s) in sentences(&doc).iter().enumerate() {
            let Some(ctx) = s.get("ctx_out") else { continue };
            let tag = format!("{}#{}", name, si);

            // Load ctx_out's word bytes straight into a SsSpa (syllable
            // pointers re-based the same way `run_record_stages` does).
            let mut ss = SsSpa::new();
            let head = unhex(ctx.get("head").str().unwrap());
            ss.b[..head.len()].copy_from_slice(&head);
            let nw = ctx.get("n_words").i64().unwrap_or(0) as usize;
            let mut base = SYL_BASE;
            for (i, w) in ctx.get("words").arr().iter().enumerate().take(nw) {
                let b = unhex(w.get("raw").str().unwrap());
                let o = SsSpa::word(i);
                ss.b[o..o + b.len()].copy_from_slice(&b);
                let n_syl = ss.u8(o + wr::N_SYL) as usize;
                let mut first_phone = 0usize;
                for (k, sy) in w.get("syl").arr().iter().enumerate().take(n_syl) {
                    let sb = unhex(sy.str().unwrap());
                    let rec = base + k * SYL_SIZE;
                    ss.b[rec..rec + SYL_SIZE].copy_from_slice(&sb);
                    ss.set_u32(rec + syl::PHONES_PTR, vtpaul::ling::ptr_ss(o + wr::PHONES + first_phone));
                    ss.set_u32(rec + syl::POS_PTR, vtpaul::ling::ptr_ss(o + wr::POS + first_phone));
                    first_phone += sb[syl::N_PHONES] as usize;
                }
                ss.set_u32(o + wr::SYL_PTR, vtpaul::ling::ptr_ss(base));
                base += n_syl * SYL_SIZE;
            }
            // Phrase table: rebuild it from the BRK bytes just loaded (same
            // algorithm `breaks::build_phrase_table` uses; ctx_out's own
            // phrase-pointer bytes are real absolute pointers, not usable
            // here) so `sentence_from_ss`'s `phrase_words` has something to
            // read.
            vtpaul::ling_spa::breaks::build_phrase_table(&mut ss);

            // Independent JSON-only reference, mirroring
            // `prosody_spa_parity.rs`'s `sentence_from_spa` exactly (same
            // fixed offsets, same running first_phone reconstruction).
            let mut ref_words: Vec<(Vec<u8>, Vec<i8>, u8, Vec<(i8, i8, i8, u8)>)> = Vec::new();
            for w in ctx.get("words").arr() {
                let raw = w.get("raw").hex();
                let n = raw[wr::N_PHONES] as usize;
                let codes = raw[wr::PHONES..wr::PHONES + n].to_vec();
                let pos_code: Vec<i8> = raw[wr::POS..wr::POS + n].iter().map(|&b| b as i8).collect();
                let brk = raw[wr::BRK];
                let syls: Vec<(i8, i8, i8, u8)> = w
                    .get("syl")
                    .arr()
                    .iter()
                    .map(|s| {
                        let b = unhex(s.str().unwrap());
                        (b[syl2::STRESS] as i8, b[syl2::SHAPE] as i8, b[syl2::POSITION] as i8, b[syl::N_PHONES])
                    })
                    .collect();
                ref_words.push((codes, pos_code, brk, syls));
            }

            t_sent.total += 1;
            let sent = sentence_from_ss(&ss);
            let mut ok = sent.words.len() == ref_words.len();
            for (gw, (rc, rp, rb, rsyl)) in sent.words.iter().zip(ref_words.iter()) {
                ok &= &gw.codes == rc && &gw.pos_code == rp && gw.brk == *rb;
                ok &= gw.syllables.len() == rsyl.len();
                for (gy, (b0, b1, b2, np)) in gw.syllables.iter().zip(rsyl.iter()) {
                    ok &= gy.b0 == *b0 && gy.b1 == *b1 && gy.b2 == *b2 && gy.n_phones == *np;
                }
            }
            if ok {
                t_sent.ok += 1;
            } else if t_sent.shown < 10 {
                t_sent.shown += 1;
                println!("  [{} sentence_from_ss] mismatch", tag);
            }

            // unitsel::Word: glue-built vs. `Word::from_raw_lang` called
            // directly on the same raw/syllable byte slices `word_from_ss`
            // extracts (the parser itself is batch 2's; this only proves
            // the glue extracts the right slices).
            for i in 0..ss.n_words() {
                t_word.total += 1;
                let got = word_from_ss(&ss, i);
                let o = SsSpa::word(i);
                let raw = ss.b[o..o + vtpaul::ling_spa::WORD_SIZE].to_vec();
                let n_syl = ss.u8(o + wr::N_SYL) as usize;
                let syl_base = (ss.u32(o + wr::SYL_PTR) & !0xf000_0000) as usize;
                let syls: Vec<Vec<u8>> = (0..n_syl)
                    .map(|g| ss.b[syl_base + g * SYL_SIZE..syl_base + (g + 1) * SYL_SIZE].to_vec())
                    .collect();
                let want = UWord::from_raw_lang(&raw, &syls, vtpaul::lang::Lang::Spa);
                if format!("{:?}", got) == format!("{:?}", want) {
                    t_word.ok += 1;
                } else if t_word.shown < 10 {
                    t_word.shown += 1;
                    println!("  [{} word{}] glue Word mismatch", tag, i);
                }
            }
        }
    }
    t_sent.report("glue-sentence");
    t_word.report("glue-word");
    assert_eq!(t_sent.ok, t_sent.total);
    assert_eq!(t_word.ok, t_word.total);
}
