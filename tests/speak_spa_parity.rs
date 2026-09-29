//! Violeta end to end: raw text -> PCM through `EngineSpa`, against the real
//! engine's WAVs (`harness\out_violeta\<name>.wav`, texts in `texts.tsv`,
//! written by `capture_violeta.py` with default parameters).
//!
//! `VTSPA_SHOW=n` prints the first n mismatching names.

use std::path::Path;

use vtpaul::engine_spa::EngineSpa;

const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(v) => v,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};
const VOICES: &str = match option_env!("VTPAUL_VOICES") {
    Some(v) => v,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};

fn wav_pcm(b: &[u8]) -> Option<Vec<i16>> {
    let mut p = 12;
    while p + 8 <= b.len() {
        let id = &b[p..p + 4];
        let n = u32::from_le_bytes(b[p + 4..p + 8].try_into().ok()?) as usize;
        if id == b"data" {
            let d = &b[p + 8..(p + 8 + n).min(b.len())];
            return Some(d.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect());
        }
        p += 8 + n + (n & 1);
    }
    None
}

fn cp1252(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| {
            let u = c as u32;
            if u < 0x100 {
                return u as u8;
            }
            match c {
                '\u{20ac}' => 0x80,
                '\u{2018}' => 0x91,
                '\u{2019}' => 0x92,
                '\u{201c}' => 0x93,
                '\u{201d}' => 0x94,
                '\u{2013}' => 0x96,
                '\u{2014}' => 0x97,
                '\u{2026}' => 0x85,
                _ => b'?',
            }
        })
        .collect()
}

#[test]
fn speak_parity_spa() {
    let dir = Path::new(HARNESS).join("out_violeta");
    let tsv = dir.join("texts.tsv");
    let root = Path::new(VOICES);
    if !tsv.exists() || !root.join("data-violeta").exists() {
        eprintln!("skipping: no out_violeta captures or no voice data");
        return;
    }
    let show: usize = std::env::var("VTSPA_SHOW").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    let eng = EngineSpa::open(root).expect("open");
    let (mut total, mut ok, mut shown) = (0usize, 0usize, 0usize);
    for line in std::fs::read_to_string(&tsv).unwrap().lines() {
        let Some((name, text)) = line.split_once('\t') else { continue };
        let Ok(wav) = std::fs::read(dir.join(format!("{}.wav", name))) else { continue };
        let Some(want) = wav_pcm(&wav) else { continue };
        total += 1;
        let got = eng.speak(&cp1252(text)).unwrap_or_default();
        if got == want {
            ok += 1;
        } else if shown < show {
            shown += 1;
            let first = got.iter().zip(&want).position(|(a, b)| a != b).unwrap_or(got.len().min(want.len()));
            eprintln!("MISMATCH {} samples ours {} eng {} first diff at {}: {:?}", name, got.len(), want.len(), first, text);
        }
    }
    eprintln!("speak_spa parity: {}/{} WAVs identical", ok, total);
}
