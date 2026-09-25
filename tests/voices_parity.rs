//! Every English voice against the real engine (the `out_voices` captures):
//! `x86_<voice>` is the 32-bit vt_eng.dll with its
//! voice name patched, `x64_<voice>` the voice's own 64-bit DLL. Both read the
//! data root `all_voices`. The port runs once per build (`Engine::x64` off and on),
//! and both must be bit-exact.
//!
//!     cargo test --release --test voices_parity -- --nocapture

use std::path::Path;

use vtpaul::engine::Engine;

const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};
/// The folder holding every voice's `data-<voice>` and `data-common`;
/// override at build time with `VTPAUL_VOICES=<dir>`.
const ROOT: &str = match option_env!("VTPAUL_VOICES") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};
const VOICES: [&str; 4] = ["jame", "juli", "kate", "paul"];

fn read_pcm(p: &Path) -> Option<Vec<u8>> {
    let b = std::fs::read(p).ok()?;
    Some(b.get(44..)?.to_vec())
}

fn cp1252(s: &str) -> Vec<u8> {
    const HI: [char; 32] = [
        '\u{20ac}', '\u{81}', '\u{201a}', '\u{192}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{2c6}',
        '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8d}', '\u{17d}', '\u{8f}', '\u{90}', '\u{2018}',
        '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}', '\u{2dc}', '\u{2122}', '\u{161}',
        '\u{203a}', '\u{153}', '\u{9d}', '\u{17e}', '\u{178}',
    ];
    s.chars()
        .map(|c| match c as u32 {
            u @ (0..=0x7f | 0xa0..=0xff) => u as u8,
            _ => HI.iter().position(|&h| h == c).map(|p| 0x80 + p as u8).unwrap_or(b'?'),
        })
        .collect()
}

struct Tally {
    ok: [usize; 2],
    missing: [usize; 2],
    x64_eq_x86: usize,
    both: usize,
    shown: Vec<String>,
}

fn run_voice(voice: &str, texts: &[(String, String)]) -> Tally {
    let mut engines = [0, 1].map(|k| {
        let mut e = Engine::open_voice(Path::new(ROOT), voice).expect("open voice");
        e.set_x64(k == 1);
        e
    });
    let _ = &mut engines;
    let dirs = ["x86", "x64"].map(|a| Path::new(HARNESS).join("out_voices").join(format!("{}_{}", a, voice)));
    let mut t = Tally { ok: [0; 2], missing: [0; 2], x64_eq_x86: 0, both: 0, shown: Vec::new() };
    for (name, text) in texts {
        let bytes = cp1252(text);
        let want: Vec<Option<Vec<u8>>> = dirs.iter().map(|d| read_pcm(&d.join(format!("{}.wav", name)))).collect();
        for k in 0..2 {
            let pcm = engines[k].speak(&bytes).expect("speak");
            let got: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
            match &want[k] {
                None => t.missing[k] += 1,
                Some(w) if *w == got => t.ok[k] += 1,
                Some(w) => {
                    if t.shown.len() < 10 {
                        let first = got.iter().zip(w).position(|(a, b)| a != b);
                        t.shown.push(format!(
                            "{} {} {}: {} bytes want {}, first diff {:?}",
                            ["x86", "x64"][k], voice, name, got.len(), w.len(), first
                        ));
                    }
                }
            }
        }
        if let (Some(a), Some(b)) = (&want[0], &want[1]) {
            t.both += 1;
            t.x64_eq_x86 += (a == b) as usize;
        }
    }
    t
}

#[test]
fn voices_match_engine() {
    let tsv = Path::new(HARNESS).join("out_voices").join("texts.tsv");
    let Ok(body) = std::fs::read_to_string(&tsv) else {
        eprintln!("skipping: no {} (need a local capture directory; set VTPAUL_HARNESS)", tsv.display());
        return;
    };
    let texts: Vec<(String, String)> = body
        .lines()
        .filter_map(|l| l.split_once('\t').map(|(a, b)| (a.to_string(), b.to_string())))
        .collect();
    let tallies: Vec<Tally> = std::thread::scope(|s| {
        let hs: Vec<_> = VOICES.iter().map(|v| s.spawn(|| run_voice(v, &texts))).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut bad = 0;
    for (v, t) in VOICES.iter().zip(&tallies) {
        for line in &t.shown {
            println!("  MISMATCH {}", line);
        }
        let n = texts.len();
        println!(
            "{}: x86 {}/{} bit-exact ({} missing), x64 {}/{} ({} missing); x64 == x86 on {}/{}",
            v, t.ok[0], n - t.missing[0], t.missing[0], t.ok[1], n - t.missing[1], t.missing[1], t.x64_eq_x86, t.both
        );
        bad += 2 * n - t.missing[0] - t.missing[1] - t.ok[0] - t.ok[1];
    }
    assert_eq!(bad, 0);
}
