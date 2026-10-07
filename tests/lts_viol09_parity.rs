//! viol09 lexicon / letter-to-sound word pass (`FUN_1001f3d0`, stages `FUN_1001f930` / `FUN_100209b0` /
//! `FUN_100220d0`) against `out_lts_viol09/*.json` (`word_pass_in` -> `word_pass_out`).
//!
//!     cargo test --release --test lts_viol09_parity -- --nocapture

use std::path::{Path, PathBuf};

use vtpaul::era::Era;
use vtpaul::json::Json;
use vtpaul::ling_spa::analysis::{lexicon_pass, war_spa};
use vtpaul::ling_spa::lexicon::LexiconSpa;
use vtpaul::ling_spa::{war, WaSpa, WA_REC};

const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};

const VOICES: &str = match option_env!("VTPAUL_VOICES") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};

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

fn pairs_of(doc: &Json) -> Vec<(Json, Json)> {
    let mut v = Vec::new();
    let mut cur: Option<Json> = None;
    for e in doc.get("events").arr() {
        match e.get("kind").str().unwrap_or("") {
            "word_pass_in" => cur = Some(e.clone()),
            "word_pass_out" => {
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
fn lexicon_pass_matches_viol09() {
    let dir = Path::new(HARNESS).join("out_lts_viol09");
    let dict = Path::new(VOICES).join("old_julie_violeta").join("Violeta").join("data-common").join("dict-spa");
    if !dir.exists() || !dict.exists() {
        eprintln!("skipping: no captures or voice data");
        return;
    }
    let lex = LexiconSpa::open_era(&dict, Era::Violeta09).expect("open dict-spa");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    let (mut ok, mut total) = (0usize, 0usize);
    for path in &files {
        let doc = Json::load(path).unwrap();
        for (wi, wo) in pairs_of(&doc) {
            let mut wa = load_wa(&wi);
            let want = load_wa(&wo);
            lexicon_pass(&mut wa, &lex);
            let n = want.n().max(wa.n());
            total += 1;
            let good = wa.b[..2] == want.b[..2]
                && (0..n).all(|i| {
                    let o = WaSpa::rec(i);
                    wa.b[o..o + WA_REC] == want.b[o..o + WA_REC]
                });
            if good {
                ok += 1;
            } else {
                for i in 0..n {
                    let o = WaSpa::rec(i);
                    if wa.b[o..o + WA_REC] != want.b[o..o + WA_REC] {
                        println!(
                            "  {} w{} {:?}: got {} want {}",
                            path.file_stem().unwrap().to_string_lossy(),
                            i,
                            String::from_utf8_lossy(want.cstr(o + war::TEXT)),
                            hex(wa.cstr(o + war_spa::PHON)),
                            hex(want.cstr(o + war_spa::PHON))
                        );
                    }
                }
            }
        }
    }
    println!("viol09 word passes {}/{}", ok, total);
    assert!(total > 0);
    assert_eq!(ok, total);
}
