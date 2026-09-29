//! Violeta linguistic analysis (`FUN_100256d0`: lexicon, letter-to-sound, TPP
//! tagging) against the captures in `out_feling_spa/*.json`: `wa_in` ->
//! `ling_spa::analysis::analyze` -> every WA record compared byte for byte
//! with `wa_out` (WA records hold no pointers).
//!
//!     cargo test --release --test feling_spa_analysis -- --nocapture

use std::path::{Path, PathBuf};

use vtpaul::json::Json;
use vtpaul::ling_spa::analysis::{analyze, war_spa};
use vtpaul::ling_spa::lexicon::LexiconSpa;
use vtpaul::ling_spa::{war, WaSpa, WA_REC};

const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};

/// Voice data root (`data-common\dict-spa` below it); override at build time
/// with `VTPAUL_VOICES=<dir>`.
const VOICES: &str = match option_env!("VTPAUL_VOICES") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};
const DICT_SPA: &str = "data-common/dict-spa";

fn out() -> PathBuf {
    Path::new(HARNESS).join("out_feling_spa")
}

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

fn captures() -> Vec<PathBuf> {
    let dir = out();
    if !dir.exists() {
        return Vec::new();
    }
    let mut v: Vec<PathBuf> =
        std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    v.sort();
    v
}

/// (in, out) event pairs of the given kinds in capture order.
fn pairs_of(doc: &Json, kin: &str, kout: &str) -> Vec<(Json, Json)> {
    let mut v = Vec::new();
    let mut cur: Option<Json> = None;
    for e in doc.get("events").arr() {
        let k = e.get("kind").str().unwrap_or("");
        if k == kin {
            cur = Some(e.clone());
        } else if k == kout {
            if let Some(i) = cur.take() {
                v.push((i, e.clone()));
            }
        }
    }
    v
}

/// `FUN_10021bf0` alone on the letter-to-sound word list (`corpus_spa\lts_words*.txt`,
/// captured by `hooks_lts_spa.js` into `out_lts_spa`).
#[test]
fn lexicon_pass_matches_engine_spa() {
    let dir = Path::new(HARNESS).join("out_lts_spa");
    if !dir.exists() {
        eprintln!("skipping: no captures in {}", dir.display());
        return;
    }
    let lex = LexiconSpa::open(&Path::new(VOICES).join(DICT_SPA)).expect("open dict-spa");
    let mut files: Vec<PathBuf> =
        std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    let (mut ok, mut total) = (0usize, 0usize);
    for path in &files {
        let doc = Json::load(path).unwrap();
        for (wi, wo) in pairs_of(&doc, "word_pass_in", "word_pass_out") {
            let mut wa = load_wa(&wi);
            let want = load_wa(&wo);
            vtpaul::ling_spa::analysis::lexicon_pass(&mut wa, &lex);
            let n = want.n().max(wa.n());
            total += 1;
            let good = wa.b[..2] == want.b[..2] && (0..n).all(|i| {
                let o = WaSpa::rec(i);
                wa.b[o..o + WA_REC] == want.b[o..o + WA_REC]
            });
            if good {
                ok += 1;
            } else {
                for i in 0..n {
                    let o = WaSpa::rec(i);
                    if wa.b[o..o + WA_REC] != want.b[o..o + WA_REC] {
                        let off = (0..WA_REC).find(|&k| wa.b[o + k] != want.b[o + k]).unwrap();
                        println!(
                            "  {} w{} {:?} +{:#x}: got {} want {}",
                            path.file_stem().unwrap().to_string_lossy(),
                            i,
                            String::from_utf8_lossy(want.cstr(o + war::TEXT)),
                            off,
                            hex(wa.cstr(o + war_spa::PHON)),
                            hex(want.cstr(o + war_spa::PHON))
                        );
                    }
                }
            }
        }
    }
    println!("word passes {}/{}", ok, total);
    assert_eq!(ok, total);
}

/// (wa_in, wa_out) event pairs in capture order.
fn pairs(doc: &Json) -> Vec<(Json, Json)> {
    let mut v = Vec::new();
    let mut cur: Option<Json> = None;
    for e in doc.get("events").arr() {
        match e.get("kind").str().unwrap_or("") {
            "wa_in" => cur = Some(e.clone()),
            "wa_out" => {
                if let Some(i) = cur.take() {
                    v.push((i, e.clone()));
                }
            }
            _ => {}
        }
    }
    v
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|c| format!("{:02x}", c)).collect()
}

#[test]
fn analysis_matches_engine_spa() {
    let caps = captures();
    if caps.is_empty() {
        eprintln!("skipping: no captures in {}", out().display());
        return;
    }
    let lex = LexiconSpa::open(&Path::new(VOICES).join(DICT_SPA)).expect("open dict-spa");
    let (mut sent_ok, mut sent_n, mut word_ok, mut word_n, mut shown) = (0usize, 0usize, 0usize, 0usize, 0usize);
    for path in &caps {
        let doc = Json::load(path).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        for (si, (wi, wo)) in pairs(&doc).iter().enumerate() {
            let mut wa = load_wa(wi);
            let want = load_wa(wo);
            analyze(&mut wa, &lex);
            let n = want.n().max(wa.n());
            sent_n += 1;
            let mut ok = wa.b[..2] == want.b[..2];
            for i in 0..n {
                let o = WaSpa::rec(i);
                word_n += 1;
                if wa.b[o..o + WA_REC] == want.b[o..o + WA_REC] {
                    word_ok += 1;
                    continue;
                }
                ok = false;
                if shown < 40 {
                    shown += 1;
                    let off = (0..WA_REC).find(|&k| wa.b[o + k] != want.b[o + k]).unwrap();
                    println!(
                        "  {}#{} w{} {:?} +{:#x}: got {} want {}",
                        name,
                        si,
                        i,
                        String::from_utf8_lossy(want.cstr(o + war::TEXT)),
                        off,
                        hex(wa.cstr(o + war_spa::PHON)),
                        hex(want.cstr(o + war_spa::PHON))
                    );
                }
            }
            if ok {
                sent_ok += 1;
            }
        }
    }
    println!("sentences {}/{}  words {}/{}", sent_ok, sent_n, word_ok, word_n);
    assert_eq!(sent_ok, sent_n);
}
