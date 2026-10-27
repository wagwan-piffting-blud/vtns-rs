//! 2004 Paul letter-to-sound, lexicon-or-LTS lookup, spelling and hyphenated LTS against an engine dump
//! (`out_lts_paul04.tsv`: word, then `ret:flags:phones` for FUN_100031e0, FUN_1000a050, FUN_10008230 and
//! FUN_100033a0).
//!
//!   cargo test --release --test lts_paul04_parity -- --nocapture

use std::path::Path;

use vtpaul::era::Era;
use vtpaul::lexicon::Lexicon;
use vtpaul::ling04::lts;

const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};
const VOICES: &str = match option_env!("VTPAUL_VOICES") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};

fn hex(b: &[u8]) -> String {
    b.iter().map(|c| format!("{:02x}", c)).collect()
}

fn col(ret: i32, f: u8, p: &[u8]) -> String {
    format!("{}:{}:{}", ret, f, hex(p))
}

#[test]
fn lts_parity() {
    let tsv = Path::new(HARNESS).join("out_lts_paul04.tsv");
    let dict = Path::new(VOICES).join("2004_paul").join("data-common").join("dict-eng");
    if !tsv.exists() || !dict.exists() {
        eprintln!("skipping: dump or voice data not present");
        return;
    }
    let show: usize = std::env::var("LTS_SHOW").ok().and_then(|s| s.parse().ok()).unwrap_or(8);
    let lex = Lexicon::open_era(&dict, Era::Paul04).expect("open 2004 lexicon");
    let text = std::fs::read(&tsv).unwrap();
    let names = ["lts", "lookup", "spell", "hyphen"];
    let mut ok = [0usize; 4];
    let mut shown = [0usize; 4];
    let mut n = 0;
    for line in text.split(|&c| c == b'\n').filter(|l| !l.is_empty()) {
        let cols: Vec<&[u8]> = line.split(|&c| c == b'\t').collect();
        let w = cols[0];
        let ours = [
            match lts::letter_to_sound(&lex, w) {
                Some((p, f)) => col(1, f, &p),
                None => col(0, 0, &[]),
            },
            match lts::lookup_or_lts(&lex, w) {
                Some(p) => col(1, 0, &p),
                None => col(-1, 0, &[]),
            },
            {
                let s = lts::spell(w);
                col(0, if s.is_empty() { 0 } else { 0x20 }, &s)
            },
            match lts::lts_hyphenated(&lex, w) {
                Some((p, f)) => col(1, f, &p),
                None => col(0, 0, &[]),
            },
        ];
        n += 1;
        for k in 0..4 {
            let want = String::from_utf8_lossy(cols[k + 1]);
            if ours[k] == want {
                ok[k] += 1;
            } else if shown[k] < show {
                shown[k] += 1;
                eprintln!("MISMATCH {} {:?}: eng {} ours {}", names[k], String::from_utf8_lossy(w), want, ours[k]);
            }
        }
    }
    eprintln!("lts paul04 parity over {} words:", n);
    for k in 0..4 {
        eprintln!("  {:8} {}/{}", names[k], ok[k], n);
    }
    assert!(ok.iter().all(|&o| o == n));
}
