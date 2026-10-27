//! 2004 Paul morphology chain steps on single words against an engine dump (`out_morph_paul04.tsv`: word,
//! then `ok:flags:phones` for the suffix stripper FUN_100086c0, the apostrophe ending FUN_10009c80, the
//! clitic suffix FUN_10009540, the camelCase split FUN_100098f0 and the hyphen / dot split FUN_100082f0
//! with dash off and on; the last two add `:pre`, the byte before the output, 0x55 when untouched).
//!
//!   cargo test --release --test morph_paul04_parity -- --nocapture

use std::path::Path;

use vtpaul::era::Era;
use vtpaul::lexicon::Lexicon;
use vtpaul::ling::Ws;
use vtpaul::ling04::morph::probe;

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

#[test]
fn morph_steps_parity() {
    let tsv = Path::new(HARNESS).join("out_morph_paul04.tsv");
    let dict = Path::new(VOICES).join("2004_paul").join("data-common").join("dict-eng");
    if !tsv.exists() || !dict.exists() {
        eprintln!("skipping: dump or voice data not present");
        return;
    }
    let show: usize = std::env::var("MORPH_SHOW").ok().and_then(|s| s.parse().ok()).unwrap_or(6);
    let lex = Lexicon::open_era(&dict, Era::Paul04).expect("open 2004 lexicon");
    let mut ws = Ws::new();
    let text = std::fs::read(&tsv).unwrap();
    let names = ["suffix", "apos", "clitic", "camel", "hyphen0", "hyphen1"];
    let mut ok = [0usize; 6];
    let mut shown = [0usize; 6];
    let mut n = 0;
    for line in text.split(|&c| c == b'\n').filter(|l| !l.is_empty()) {
        let cols: Vec<&[u8]> = line.split(|&c| c == b'\t').collect();
        let w = cols[0];
        n += 1;
        for k in 0..6 {
            let (r, fl, pre) = probe(&lex, &mut ws, k as u8, w);
            let mut ours = format!("{}:{}:{}", r.is_some() as u8, fl, r.as_deref().map(hex).unwrap_or_default());
            if k >= 4 {
                ours += if pre { ":26" } else { ":55" };
            }
            let want = String::from_utf8_lossy(cols[k + 1]);
            if ours == want {
                ok[k] += 1;
            } else if shown[k] < show {
                shown[k] += 1;
                eprintln!("MISMATCH {} {:?}: eng {} ours {}", names[k], String::from_utf8_lossy(w), want, ours);
            }
        }
    }
    eprintln!("morph paul04 steps over {} words:", n);
    for k in 0..6 {
        eprintln!("  {:8} {}/{}", names[k], ok[k], n);
    }
    assert!(ok.iter().all(|&o| o == n));
}
