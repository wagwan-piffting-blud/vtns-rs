//! Raw text to PCM for the 2009 builds against the unhooked 2009 DLLs (`harness\capture_old.py`
//! -> `out_old\<build>\<name>.wav`): Paul09 and Julie09 through `Engine`, Violeta09 through `EngineSpa`.
//!
//!     cargo test --release --test speak_old_parity -- --nocapture

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use vtpaul::engine::Engine;
use vtpaul::engine_spa::EngineSpa;

const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};
const ROOT: &str = match option_env!("VTPAUL_VOICES") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};

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

/// (build, data root under all_voices, data directory voice name; "violeta" goes through `EngineSpa`)
const BUILDS: [(&str, &str, &str); 3] = [
    ("paul09", "old_paul_install/Paul", "paul"),
    ("julie09", "old_julie_violeta/Julie", "julie"),
    ("viol09", "old_julie_violeta/Violeta", "violeta"),
];

trait Speak {
    fn speak_pcm(&self, text: &[u8]) -> Vec<i16>;
}

impl Speak for Engine {
    fn speak_pcm(&self, text: &[u8]) -> Vec<i16> {
        self.speak(text).expect("speak")
    }
}

impl Speak for EngineSpa {
    fn speak_pcm(&self, text: &[u8]) -> Vec<i16> {
        self.speak(text).expect("speak")
    }
}

fn open(root: &Path, voice: &str) -> Box<dyn Speak> {
    if voice == "violeta" {
        Box::new(EngineSpa::open(root).expect("open violeta"))
    } else {
        Box::new(Engine::open_voice(root, voice).expect("open voice"))
    }
}

fn run_build(build: &str, root: &Path, voice: &str) -> Option<(usize, usize, Vec<String>)> {
    let dir: PathBuf = Path::new(HARNESS).join("out_old").join(build);
    let body = std::fs::read_to_string(dir.join("texts.tsv")).ok()?;
    if !root.exists() {
        return None;
    }
    let texts: Vec<(String, String)> = body
        .lines()
        .filter_map(|l| l.split_once('\t').map(|(a, b)| (a.to_string(), b.to_string())))
        .collect();
    let next = AtomicUsize::new(0);
    let ok = AtomicUsize::new(0);
    let bad = Mutex::new(Vec::new());
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(12);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                let engine = open(root, voice);
                loop {
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    if k >= texts.len() {
                        break;
                    }
                    let (name, text) = &texts[k];
                    let Some(want) = read_pcm(&dir.join(format!("{}.wav", name))) else { continue };
                    let pcm = engine.speak_pcm(&cp1252(text));
                    let got: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
                    if got == want {
                        ok.fetch_add(1, Ordering::Relaxed);
                    } else {
                        let first = got.iter().zip(&want).position(|(a, b)| a != b);
                        bad.lock().unwrap().push(format!("{} {}: {} bytes want {}, first diff {:?}", build, name, got.len(), want.len(), first));
                    }
                }
            });
        }
    });
    let mut bad = bad.into_inner().unwrap();
    bad.sort();
    Some((ok.into_inner(), texts.len(), bad))
}

#[test]
fn speak_old_match_engine() {
    let mut all = true;
    for (build, rel, voice) in BUILDS {
        let root = Path::new(ROOT).join(rel);
        let Some((ok, n, bad)) = run_build(build, &root, voice) else {
            eprintln!("skipping {}: no out_old captures or no voice data", build);
            continue;
        };
        for line in bad.iter().take(15) {
            println!("  MISMATCH {}", line);
        }
        println!("{}: {}/{} bit-exact", build, ok, n);
        all &= ok == n;
    }
    assert!(all);
}
