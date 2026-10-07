//! Linguistic front-end parity against the Frida captures in
//! `out_feling_julie09/*.json`. Each engine step is fed the captured state before it
//! and compared with the captured state after it.
//!
//!     cargo test --release --test feling_parity -- --nocapture

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use vtpaul::era::Era;
use vtpaul::json::Json;
use vtpaul::ling::lay::{Lay, L09};
use vtpaul::ling::{self, ptr_wa, war, Ss, Tib, Wa, WordStrings, SYL_SIZE};

/// Capture root; override at build time with `VTPAUL_HARNESS=<dir>`.
const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};

fn out() -> &'static str {
    Box::leak(format!("{}/out_feling_julie09", HARNESS).into_boxed_str())
}
/// Voice data root; override at build time with `VTPAUL_VOICES=<dir>`.
const VOICES: &str = match option_env!("VTPAUL_VOICES") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};
const ROOT: &str = "old_julie_violeta/Julie";
const ERA: Era = Era::Julie09;

fn dict() -> PathBuf {
    Path::new(VOICES).join(ROOT).join("data-common").join("dict-eng")
}
const LAY: &Lay = &L09;
const WORD_PTRS: [usize; 6] = [0x00, 0x04, 0x8c, 0x218, 0x21c, 0x2f0];

fn unhex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks(2)
        .map(|c| {
            let h = |x: u8| (x as char).to_digit(16).unwrap_or(0) as u8;
            h(c[0]) << 4 | h(c[1])
        })
        .collect()
}

fn load_wa(e: &Json) -> Wa {
    let mut wa = Wa::new();
    let head = unhex(e.get("head").str().unwrap());
    wa.b[..head.len()].copy_from_slice(&head);
    for (i, r) in e.get("recs").arr().iter().enumerate() {
        let b = unhex(r.str().unwrap());
        let o = Wa::rec(i);
        wa.b[o..o + b.len()].copy_from_slice(&b);
    }
    wa
}

fn load_ss(e: &Json, ss: &mut Ss) {
    let head = unhex(e.get("head").str().unwrap());
    ss.b[..head.len()].copy_from_slice(&head);
    for (i, w) in e.get("words").arr().iter().enumerate() {
        let b = unhex(w.get("raw").str().unwrap());
        let o = LAY.word(i);
        ss.b[o..o + b.len()].copy_from_slice(&b);
        ss.set_u32(o + LAY.text_ptr, ptr_wa(Wa::rec(i) + war::TEXT));
        ss.set_u32(o + LAY.ph_ptr, ptr_wa(Wa::rec(i) + war::PHONES));
    }
    ss.set_u8(LAY.ss_type, e.get("type").i64().unwrap_or(0) as u8);
}

fn load_tib(e: &Json) -> Tib {
    let mut t = Tib::new();
    let tib = e.get("tib");
    for (name, dst) in [("pause", &mut t.pause), ("brk_level", &mut t.brk_level)] {
        for (i, v) in tib.get(name).arr().iter().enumerate() {
            dst[i] = v.i64().unwrap_or(-1) as i32;
        }
    }
    t
}

fn kind7(e: &Json) -> Vec<i32> {
    e.get("tib").get("kind7").arr().iter().map(|v| v.i64().unwrap_or(-1) as i32).collect()
}

fn masked_word(ss: &Ss, i: usize) -> Vec<u8> {
    let o = LAY.word(i);
    let mut v = ss.b[o..o + LAY.stride].to_vec();
    for p in WORD_PTRS {
        v[p..p + 4].fill(0);
    }
    v
}

fn syllables(ss: &Ss, i: usize) -> Vec<Vec<u8>> {
    let o = LAY.word(i);
    let n = ss.u8(o + LAY.n_syl) as usize;
    match ss.deref(ss.u32(o + LAY.syl_ptr)) {
        Some(base) => (0..n).map(|s| ss.b[base + s * SYL_SIZE..base + (s + 1) * SYL_SIZE].to_vec()).collect(),
        None => Vec::new(),
    }
}

fn masked_head(b: &[u8]) -> Vec<u8> {
    let mut v = b[..ling::WORD_BASE].to_vec();
    for g in 0..100 {
        for p in [0x14 + 16 * g, 0x18 + 16 * g] {
            if p + 4 <= v.len() {
                v[p..p + 4].fill(0);
            }
        }
    }
    v
}

#[derive(Default)]
struct Tally {
    ok: usize,
    total: usize,
    shown: usize,
}

impl Tally {
    fn report(&self, name: &str) {
        println!("{:12} {:5}/{:5} sentences exact", name, self.ok, self.total);
    }

    fn assert_all(&self, name: &str) {
        self.report(name);
        assert!(self.ok == self.total, "{}: {} of {} sentences differ", name, self.total - self.ok, self.total);
    }
}

/// Syllable bytes written by syllabification (the rest is prosody output,
/// stale from the previous sentence until pro_PredictPitch runs).
fn syl_written(v: &[Vec<u8>]) -> Vec<Vec<u8>> {
    v.iter().map(|s| vec![s[0], s[1], s[2], s[0x1c], s[0x1d]]).collect()
}

/// Fields fe_BuildWordPhoneRecords writes (everything else in the record is
/// stale or belongs to earlier steps).
fn compare_words(t: &mut Tally, tag: &str, ss: &Ss, want: &Json) {
    t.total += 1;
    let mut diffs: Vec<String> = Vec::new();
    for (i, w) in want.get("words").arr().iter().enumerate() {
        let b = unhex(w.get("raw").str().unwrap());
        let o = LAY.word(i);
        let n = b[LAY.n_phones] as usize;
        let mut offs: Vec<usize> = vec![LAY.n_phones, LAY.class_char, LAY.src, LAY.tag_a, LAY.tag_b];
        offs.extend([LAY.token_class, LAY.token_class + 1, LAY.x3ae, LAY.x3ae + 1, LAY.x3b4, LAY.x3b4 + 1]);
        offs.extend((0..4).map(|k| LAY.start + k));
        offs.extend((0..4).map(|k| LAY.end + k));
        for k in 0..n {
            offs.extend([LAY.codes + k, LAY.marks + k]);
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

/// Compare Rust SS state against the captured stage `want`; print the first
/// differences.
fn compare(t: &mut Tally, tag: &str, ss: &Ss, want: &Json) -> bool {
    t.total += 1;
    let mut diffs: Vec<String> = Vec::new();
    let head = unhex(want.get("head").str().unwrap());
    let mh = masked_head(&head);
    let ours = masked_head(&ss.b);
    for (o, (a, b)) in ours.iter().zip(mh.iter()).enumerate() {
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
        let ws: Vec<Vec<u8>> = w.get("syl").arr().iter().map(|s| unhex(s.str().unwrap())).collect();
        let (ws, hs) = (syl_written(&ws), syl_written(&syllables(ss, i)));
        if ws != hs {
            diffs.push(format!("w{} syl {:02x?} want {:02x?}", i, hs, ws));
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

fn captures() -> Vec<PathBuf> {
    let dir = Path::new(out());
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

/// Group a capture's events into sentences keyed by kind.
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

fn cstr(b: &[u8]) -> &[u8] {
    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    &b[..n]
}

/// `offset_maps` fires once per request, from `fe_InitialTextProcess`,
/// before the first sentence's `wa_in` -- so it never lands in any of
/// `sentences()`'s per-sentence buckets; read it straight off the top-level
/// event list. `(map_start, map_end)`, both empty when the capture predates
/// this event (recapture) or the hook found nothing.
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

/// The defined part of a 0x554 candidate: count, token, class, spelling, POS
/// list up to 0xff, pronunciations, the four bools.
fn cand_key(b: &[u8]) -> Vec<u8> {
    use ling::lookup::cand;
    let mut v = Vec::new();
    v.extend_from_slice(&b[..5]);
    v.extend_from_slice(cstr(&b[cand::SPELLING..cand::SPELLING + 0x1e]));
    v.push(0xee);
    let pos = &b[cand::POS..cand::PRON];
    let pn = pos.iter().position(|&c| c == 0xff).map(|p| p + 1).unwrap_or(pos.len());
    v.extend_from_slice(&pos[..pn]);
    let count = i16::from_le_bytes([b[0], b[1]]).clamp(1, 16) as usize;
    for k in 0..count {
        let o = cand::PRON + k * cand::PRON_STRIDE;
        v.extend_from_slice(cstr(&b[o..o + cand::PRON_STRIDE]));
        v.push(0xee);
    }
    v.extend_from_slice(&b[cand::B40..cand::B40 + 8]);
    v
}

fn lexicon() -> vtpaul::lexicon::Lexicon {
    vtpaul::lexicon::Lexicon::open_era(&dict(), ERA).unwrap()
}

#[test]
fn lookup_stage() {
    let caps = captures();
    if caps.is_empty() {
        return;
    }
    let lex = lexicon();
    let mut t = Tally::default();
    for path in &caps {
        let doc = Json::load(path).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        for (si, s) in sentences(&doc).iter().enumerate() {
            let (Some(wa_in), Some(lk)) = (s.get("wa_in"), s.get("lookup_out")) else {
                continue;
            };
            t.total += 1;
            let wa = load_wa(wa_in);
            let mut ws = ling::Ws::new();
            ling::lookup::lookup_tokens(&mut ws, &wa, wa.n(), true, &lex);
            let want: Vec<Vec<u8>> = lk.get("cands").arr().iter().map(|c| unhex(c.str().unwrap())).collect();
            let got_n = ws.i16(0) as usize;
            let mut diffs: Vec<String> = Vec::new();
            if got_n != want.len() {
                diffs.push(format!("count {} want {}", got_n, want.len()));
            }
            for (i, w) in want.iter().enumerate().take(got_n) {
                let o = ling::Ws::cand(i);
                let g = &ws.b[o..o + ling::CAND_SIZE];
                if cand_key(g) != cand_key(w) {
                    diffs.push(format!(
                        "c{} '{}' got {:02x?} want {:02x?}",
                        i,
                        String::from_utf8_lossy(cstr(&w[5..0x23])),
                        cand_key(g),
                        cand_key(w)
                    ));
                }
            }
            if diffs.is_empty() {
                t.ok += 1;
            } else if t.shown < 15 {
                t.shown += 1;
                println!("  [{}#{}] {}", name, si, diffs[..diffs.len().min(4)].join(" | "));
            }
        }
    }
    t.assert_all("lookup");
}

/// Defined part of a 0x70 morph entry (spelling / pron as C strings).
fn morph_key(b: &[u8], with_flags: bool) -> Vec<u8> {
    use ling::mr;
    let mut v = Vec::new();
    if with_flags {
        v.extend_from_slice(&b[0..2]);
    }
    v.extend_from_slice(&b[mr::TOKEN..mr::SPELLING]);
    v.extend_from_slice(cstr(&b[mr::SPELLING..mr::PRON]));
    v.push(0xee);
    v.extend_from_slice(cstr(&b[mr::PRON..mr::ACCENT]));
    v.push(0xee);
    v.push(b[mr::ACCENT]);
    v.extend_from_slice(&b[mr::B40..mr::B40 + 8]);
    v
}

fn compare_morphs(t: &mut Tally, tag: &str, ws: &ling::Ws, want: &Json, with_flags: bool) {
    t.total += 1;
    let wm: Vec<Vec<u8>> = want.get("morphs").arr().iter().map(|m| unhex(m.str().unwrap())).collect();
    let n = ws.n_morphs();
    let mut diffs: Vec<String> = Vec::new();
    if n != wm.len() {
        diffs.push(format!("n {} want {}", n, wm.len()));
    }
    for (i, w) in wm.iter().enumerate().take(n) {
        let o = ling::Ws::morph(i);
        let g = &ws.b[o..o + ling::MORPH_SIZE];
        let (a, b) = (morph_key(g, with_flags), morph_key(w, with_flags));
        if a != b {
            diffs.push(format!(
                "m{} '{}' got {:02x?} want {:02x?}",
                i,
                String::from_utf8_lossy(cstr(&w[7..0x25])),
                a,
                b
            ));
        }
    }
    if diffs.is_empty() {
        t.ok += 1;
    } else if t.shown < 20 {
        t.shown += 1;
        println!("  [{}] {}", tag, diffs[..diffs.len().min(3)].join(" | "));
    }
}

#[test]
fn copy_stage() {
    let caps = captures();
    if caps.is_empty() {
        return;
    }
    let lex = lexicon();
    let poly = lexicon().poly;
    let mut t = Tally::default();
    for path in &caps {
        let doc = Json::load(path).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        for (si, s) in sentences(&doc).iter().enumerate() {
            let (Some(wa_in), Some(cp)) = (s.get("wa_in"), s.get("copy_out")) else {
                continue;
            };
            let wa = load_wa(wa_in);
            let mut ws = ling::Ws::new();
            ling::analysis::tag_and_lookup(&mut ws, &wa, wa.n(), &lex, &poly);
            compare_morphs(&mut t, &format!("{}#{}", name, si), &ws, cp, false);
        }
    }
    t.assert_all("copy");
}

/// fe_LetterToSound / FUN_10009cd0 / FUN_10003110 called in isolation on the
/// captured words.
#[test]
fn lts_calls() {
    let caps = captures();
    if caps.is_empty() {
        return;
    }
    let lex = lexicon();
    let mut tallies: BTreeMap<String, Tally> = BTreeMap::new();
    for path in &caps {
        let doc = Json::load(path).unwrap();
        for e in doc.get("events").arr() {
            let kind = e.get("kind").str().unwrap_or("");
            if !matches!(kind, "lts" | "spell" | "lts_hy") {
                continue;
            }
            let word = unhex(e.get("word").str().unwrap());
            let want_out = unhex(e.get("out").str().unwrap());
            let f0 = e.get("f0").i64().unwrap_or(0) as u8;
            let f1 = e.get("f1").i64().unwrap_or(0) as u8;
            let ret = e.get("ret").i64().unwrap_or(0);
            let (ok_ret, out, flags) = match kind {
                "lts" => match ling::lts::letter_to_sound(&lex, &word) {
                    Some((ph, f)) => (1, ph, f),
                    None => (0, want_out.clone(), 0),
                },
                "spell" => {
                    let s = ling::lts::spell(&word);
                    let f = if s.is_empty() { 0 } else { 0x20 };
                    (ret, s, f)
                }
                _ => {
                    let (ph, f) = ling::lts::lts_hyphenated(&lex, &word);
                    ((!ph.is_empty()) as i64, ph, f)
                }
            };
            let t = tallies.entry(kind.to_string()).or_default();
            t.total += 1;
            let want_flags = f1 & !f0;
            if ok_ret == ret && out == want_out && (flags | f0) & !f0 == want_flags {
                t.ok += 1;
            } else if t.shown < 12 {
                t.shown += 1;
                println!(
                    "  [{}] '{}' ret {} want {} out {:02x?} want {:02x?} flags {:02x} want {:02x}",
                    kind,
                    String::from_utf8_lossy(&word),
                    ok_ret,
                    ret,
                    out,
                    want_out,
                    flags,
                    want_flags
                );
            }
        }
    }
    for (k, t) in &tallies {
        println!("{:12} {:5}/{:5} calls exact", k, t.ok, t.total);
        assert!(t.ok == t.total, "{}: {} of {} calls differ", k, t.total - t.ok, t.total);
    }
}

#[test]
fn morph_stage() {
    let caps = captures();
    if caps.is_empty() {
        return;
    }
    let lex = lexicon();
    let poly = lexicon().poly;
    let mut t = Tally::default();
    for path in &caps {
        let doc = Json::load(path).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        for (si, s) in sentences(&doc).iter().enumerate() {
            let (Some(wa_in), Some(mo)) = (s.get("wa_in"), s.get("morph_out")) else {
                continue;
            };
            let wa = load_wa(wa_in);
            let mut ws = ling::Ws::new();
            ling::analysis::tag_and_lookup(&mut ws, &wa, wa.n(), &lex, &poly);
            ling::morph::morphology(&mut ws, &wa, &lex);
            compare_morphs(&mut t, &format!("{}#{}", name, si), &ws, mo, true);
        }
    }
    t.assert_all("morph");
}

/// Independent transcription of `dict_GetTppField`'s field-scan (strstr for
/// `" <letter>"` rather than the byte-loop `tpp_field_in_payload` uses), run
/// over every one of the 31,550 stored TPP payloads for every field letter
/// `LEXICON.md` documents (G/A/B/C/D/E/F), to prove `tpp_field_in_payload`
/// agrees with the engine's own algorithm exhaustively rather than just on
/// the words the feling captures happen to exercise.
fn tpp_field_literal(payload: &[u8], letter: u8) -> Option<Vec<u8>> {
    let body: &[u8] = if payload.first() == Some(&letter) {
        &payload[1..]
    } else {
        let pat = [b' ', letter];
        let pos = payload.windows(2).position(|w| w == pat)?;
        &payload[pos + 2..]
    };
    let end = body.iter().position(|&c| c == b' ').unwrap_or(body.len());
    Some(body[..end].to_vec())
}

#[test]
fn tpp_field_equivalence() {
    let lex = lexicon();
    let mut checked = 0u64;
    let mut mismatches: Vec<String> = Vec::new();
    for s in 0..lex.tpp.nkeys {
        let (key, payload) = lex.tpp.record(s);
        if key.is_empty() {
            continue;
        }
        for &letter in b"GABCDEF" {
            checked += 1;
            let got = ling::funcwords::tpp_field_in_payload(payload, letter);
            let want = tpp_field_literal(payload, letter);
            if got != want.as_deref() {
                if mismatches.len() < 20 {
                    mismatches.push(format!(
                        "key '{}' payload '{}' letter {}: got {:?} want {:?}",
                        String::from_utf8_lossy(key),
                        String::from_utf8_lossy(payload),
                        letter as char,
                        got.map(String::from_utf8_lossy),
                        want.as_deref().map(String::from_utf8_lossy)
                    ));
                }
            }
        }
    }
    for m in &mismatches {
        println!("  {}", m);
    }
    println!("tpp_field_equivalence: {} keys, {} (key,letter) pairs checked, {} mismatches", lex.tpp.nkeys, checked, mismatches.len());
    assert!(mismatches.is_empty(), "{} mismatches (of {}) between tpp_field_in_payload and the literal transcription", mismatches.len(), checked);
}

#[test]
fn funcwords_stage() {
    let caps = captures();
    if caps.is_empty() {
        return;
    }
    let lex = lexicon();
    let mut t = Tally::default();
    for path in &caps {
        let doc = Json::load(path).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        for (si, s) in sentences(&doc).iter().enumerate() {
            let (Some(fw_in), Some(fw_out)) = (s.get("fw_in_wa"), s.get("fw_out_wa")) else {
                continue;
            };
            let n = fw_in.get("n").i64().unwrap_or(0) as usize;
            let mut wa = ling::Wa::new();
            wa.set_i16(0, n as i16);
            for (i, r) in fw_in.get("recs").arr().iter().enumerate() {
                let b = unhex(r.str().unwrap());
                let o = ling::Wa::rec(i);
                wa.b[o..o + b.len()].copy_from_slice(&b);
            }
            ling::funcwords::mark_function_words(&mut wa, n, &lex);
            t.total += 1;
            let mut diffs: Vec<String> = Vec::new();
            for (i, r) in fw_out.get("recs").arr().iter().enumerate() {
                let want = unhex(r.str().unwrap());
                let o = ling::Wa::rec(i);
                let got = &wa.b[o..o + want.len()];
                if got != want.as_slice() {
                    for (off, (a, b)) in got.iter().zip(want.iter()).enumerate() {
                        if a != b {
                            diffs.push(format!("w{}+{:x}: {:02x} want {:02x}", i, off, a, b));
                        }
                    }
                }
            }
            if diffs.is_empty() {
                t.ok += 1;
            } else if t.shown < 20 {
                t.shown += 1;
                println!("  [{}#{}] {}", name, si, diffs[..diffs.len().min(6)].join(" | "));
            }
        }
    }
    t.assert_all("funcwords");
}

/// `fe_LinguisticAnalysis` (0x1003e240) chained end to end:
/// `ling::analysis::tag_and_pronounce` (lookup + copy + morphology + the
/// wh-flag pass + function words) then `ling::merge::merge`, diffed against
/// the captured `wa_out` record bytes and the WA header's wh flag.
#[test]
fn merge_stage() {
    let caps = captures();
    if caps.is_empty() {
        return;
    }
    let lex = lexicon();
    let poly = lexicon().poly;
    let mut t = Tally::default();
    for path in &caps {
        let doc = Json::load(path).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        for (si, s) in sentences(&doc).iter().enumerate() {
            let (Some(wa_in), Some(wa_out)) = (s.get("wa_in"), s.get("wa_out")) else {
                continue;
            };
            let mut wa = load_wa(wa_in);
            let n = wa.n();
            let mut ws = ling::Ws::new();
            ling::analysis::tag_and_pronounce(&mut ws, &mut wa, n, &lex, &poly);
            ling::merge::merge(&ws, &mut wa);

            t.total += 1;
            let mut diffs: Vec<String> = Vec::new();
            let want = load_wa(wa_out);
            if wa.wh() != want.wh() {
                diffs.push(format!("wh: {} want {}", wa.wh(), want.wh()));
            }
            for i in 0..n {
                let o = ling::Wa::rec(i);
                let got = &wa.b[o..o + ling::WA_REC];
                let exp = &want.b[o..o + ling::WA_REC];
                for (off, (a, b)) in got.iter().zip(exp.iter()).enumerate() {
                    if a != b {
                        diffs.push(format!("w{}+{:x}: {:02x} want {:02x}", i, off, a, b));
                    }
                }
            }
            if diffs.is_empty() {
                t.ok += 1;
            } else if t.shown < 20 {
                t.shown += 1;
                println!("  [{}#{}] {}", name, si, diffs[..diffs.len().min(8)].join(" | "));
            }
        }
    }
    t.assert_all("merge");
}

/// The real end-to-end chain: `wa_in` -> lookup + copy + morphology +
/// function words + merge (everything `merge_stage` proves) -> our own `Wa`,
/// then straight into `fe_BuildWordPhoneRecords` / `fe_AssignPhraseBreaks` /
/// `fe_BuildPhoneContexts`, diffed against `words_out`/`breaks_out`/
/// `ctx_out` -- the same comparison `record_stages` makes, except `record_stages`
/// feeds those three stages the *captured* `wa_out` and only proves each
/// stage in isolation. This proves our merge's `Wa` is itself good enough to
/// drive them. `k7` (the mode-7 source-tag list) and the phrase-break `Tib`
/// stay captured inputs, same as `record_stages` -- they come from the text
/// front end, not this stage.
#[test]
fn chain_stage() {
    let caps = captures();
    if caps.is_empty() {
        return;
    }
    let lex = lexicon();
    let poly = lexicon().poly;
    let engbi = lexicon().engbi;
    let mut t_words = Tally::default();
    let mut t_breaks = Tally::default();
    let mut t_ctx = Tally::default();
    for path in &caps {
        let doc = Json::load(path).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        // `fe_NextSentence`'s own running `TIB+4` cursor: the sum of every
        // earlier sentence's `Wa::consumed()` within this request (one
        // capture file == one request), plus the "processed -> original
        // position" maps `fe_InitialTextProcess` builds once per request.
        // See `words::build_word_phone_records`.
        let mut cursor = 0i32;
        let (map_start, map_end) = load_offset_maps(&doc);
        for (si, s) in sentences(&doc).iter().enumerate() {
            let (Some(wa_in), Some(words), Some(brk), Some(ctx)) =
                (s.get("wa_in"), s.get("words_out"), s.get("breaks_out"), s.get("ctx_out"))
            else {
                continue;
            };
            let tag = format!("{}#{}", name, si);

            let mut wa = load_wa(wa_in);
            let n = wa.n();
            let mut ws = ling::Ws::new();
            ling::analysis::tag_and_pronounce(&mut ws, &mut wa, n, &lex, &poly);
            ling::merge::merge(&ws, &mut wa);

            let mut ss = Ss::new();
            load_ss(words, &mut ss);
            let expect_type = ss.u8(LAY.ss_type);
            for i in 0..ss.n_words() {
                let w = LAY.word(i);
                for o in [LAY.n_phones, LAY.class_char, LAY.src, LAY.tag_a, LAY.tag_b] {
                    ss.set_u8(w + o, 0);
                }
                // Zeroed too so the rebuild is genuinely from scratch, not
                // silently supplied by `load_ss`'s preload: a real bug
                // (words.rs never wrote these) went unnoticed here until
                // an independent from-scratch `Ss` build elsewhere caught
                // it -- see the note on `words::build_word_phone_records`.
                ss.set_i32(w + LAY.start, 0);
                ss.set_i32(w + LAY.end, 0);
                ss.b[w + LAY.codes..w + LAY.codes + 0x41].fill(0);
                ss.b[w + LAY.marks..w + LAY.marks + 0x41].fill(0);
            }
            ss.set_u8(LAY.ss_type, 0);
            let k7 = kind7(words);
            let pos = ling::words::TextPos {
                cursor,
                map_start: if map_start.is_empty() { None } else { Some(&map_start) },
                map_end: if map_end.is_empty() { None } else { Some(&map_end) },
            };
            ling::words::build_word_phone_records_lay(LAY, &mut ss, &wa, &k7, &pos);
            cursor += wa.consumed();
            if ss.u8(LAY.ss_type) != expect_type {
                println!("  [{}] type {} want {}", tag, ss.u8(LAY.ss_type), expect_type);
                t_words.total += 1;
            } else {
                compare_words(&mut t_words, &tag, &ss, words);
            }

            let mut ss = Ss::new();
            load_ss(words, &mut ss);
            let mut tib = load_tib(words);
            ling::breaks::assign_phrase_breaks_era(ERA, &mut ss, &mut tib, &wa, &engbi);
            compare(&mut t_breaks, &tag, &ss, brk);

            let mut ss = Ss::new();
            load_ss(brk, &mut ss);
            let mut base = LAY.syl_base;
            for i in 0..ss.n_words() {
                let w = LAY.word(i);
                ss.set_u32(w + LAY.syl_ptr, ling::ptr_ss(base));
                for (k, sy) in brk.get("words").at(i).get("syl").arr().iter().enumerate() {
                    let b = unhex(sy.str().unwrap());
                    ss.b[base + k * SYL_SIZE..base + (k + 1) * SYL_SIZE].copy_from_slice(&b);
                }
                base += ss.u8(w + LAY.n_syl) as usize * SYL_SIZE;
            }
            let strs = WordStrings { wa: &wa };
            ling::context::build_phone_contexts09(ERA, &mut ss, &strs);
            compare(&mut t_ctx, &tag, &ss, ctx);
        }
    }
    t_words.assert_all("chain-words");
    t_breaks.assert_all("chain-breaks");
    t_ctx.assert_all("chain-ctx");
}

#[test]
fn wh_stage() {
    let caps = captures();
    if caps.is_empty() {
        return;
    }
    let mut t = Tally::default();
    for path in &caps {
        let doc = Json::load(path).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        for (si, s) in sentences(&doc).iter().enumerate() {
            let Some(wh) = s.get("wh_out") else {
                continue;
            };
            let want = wh.get("wh").i64().unwrap_or(0) as i16;
            let morphs = wh.get("morphs").arr();
            let mut ws = ling::Ws::new();
            ws.set_i16(ling::MORPH_COUNT, morphs.len() as i16);
            for (i, m) in morphs.iter().enumerate() {
                let b = unhex(m.str().unwrap());
                let o = ling::Ws::morph(i);
                ws.b[o..o + b.len()].copy_from_slice(&b);
            }
            ling::funcwords::wh_flag(&mut ws);
            let got = ws.i16(ling::WH_FLAG);
            t.total += 1;
            if got == want {
                t.ok += 1;
            } else if t.shown < 20 {
                t.shown += 1;
                println!("  [{}#{}] got {} want {}", name, si, got, want);
            }
        }
    }
    t.assert_all("wh");
}

#[test]
fn record_stages() {
    let caps = captures();
    if caps.is_empty() {
        eprintln!("skipping: no captures in {}", out());
        return;
    }
    let engbi = lexicon().engbi;
    let mut t_words = Tally::default();
    let mut t_breaks = Tally::default();
    let mut t_ctx = Tally::default();
    for path in &caps {
        let doc = Json::load(path).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        // See `chain_stage`'s comment: `fe_NextSentence`'s running cursor
        // plus the request's "processed -> original position" maps.
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

            // fe_BuildWordPhoneRecords: start from the captured result with the
            // fields it writes cleared, rebuild them.
            let mut ss = Ss::new();
            load_ss(words, &mut ss);
            let expect_type = ss.u8(LAY.ss_type);
            for i in 0..ss.n_words() {
                let w = LAY.word(i);
                for o in [LAY.n_phones, LAY.class_char, LAY.src, LAY.tag_a, LAY.tag_b] {
                    ss.set_u8(w + o, 0);
                }
                // Zeroed too so the rebuild is genuinely from scratch, not
                // silently supplied by `load_ss`'s preload: a real bug
                // (words.rs never wrote these) went unnoticed here until
                // an independent from-scratch `Ss` build elsewhere caught
                // it -- see the note on `words::build_word_phone_records`.
                ss.set_i32(w + LAY.start, 0);
                ss.set_i32(w + LAY.end, 0);
                ss.b[w + LAY.codes..w + LAY.codes + 0x41].fill(0);
                ss.b[w + LAY.marks..w + LAY.marks + 0x41].fill(0);
            }
            ss.set_u8(LAY.ss_type, 0);
            let k7 = kind7(words);
            let pos = ling::words::TextPos {
                cursor,
                map_start: if map_start.is_empty() { None } else { Some(&map_start) },
                map_end: if map_end.is_empty() { None } else { Some(&map_end) },
            };
            ling::words::build_word_phone_records_lay(LAY, &mut ss, &wa, &k7, &pos);
            cursor += wa.consumed();
            if ss.u8(LAY.ss_type) != expect_type {
                println!("  [{}] type {} want {}", tag, ss.u8(LAY.ss_type), expect_type);
                t_words.total += 1;
            } else {
                compare_words(&mut t_words, &tag, &ss, words);
            }

            let mut ss = Ss::new();
            load_ss(words, &mut ss);
            let mut tib = load_tib(words);
            ling::breaks::assign_phrase_breaks_era(ERA, &mut ss, &mut tib, &wa, &engbi);
            compare(&mut t_breaks, &tag, &ss, brk);

            let mut ss = Ss::new();
            load_ss(brk, &mut ss);
            // Re-point syllables the way syllabify laid them out.
            let mut base = LAY.syl_base;
            for i in 0..ss.n_words() {
                let w = LAY.word(i);
                ss.set_u32(w + LAY.syl_ptr, ling::ptr_ss(base));
                for (k, sy) in brk.get("words").at(i).get("syl").arr().iter().enumerate() {
                    let b = unhex(sy.str().unwrap());
                    ss.b[base + k * SYL_SIZE..base + (k + 1) * SYL_SIZE].copy_from_slice(&b);
                }
                base += ss.u8(w + LAY.n_syl) as usize * SYL_SIZE;
            }
            let strs = WordStrings { wa: &wa };
            ling::context::build_phone_contexts09(ERA, &mut ss, &strs);
            compare(&mut t_ctx, &tag, &ss, ctx);
        }
    }
    t_words.assert_all("words");
    t_breaks.assert_all("breaks");
    t_ctx.assert_all("contexts");
}
