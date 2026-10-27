//! 2004 Paul from normalization records to PCM: every sentence's captured `wa_in` (`out_feling_paul04`)
//! through the port's linguistic stage, SS passes, prosody, unit selection and waveform generation, against
//! the engine's WAV of the same text (`out_old/paul04`). The per-word TIB arrays (break levels, pauses,
//! rates) come from the capture until the text front end is ported.
//!
//!   cargo test --release --test e2e_paul04_parity -- --nocapture      (E2E_ONLY=name, E2E_LIMIT=n)

use std::path::{Path, PathBuf};

use vtpaul::bank::Bank;
use vtpaul::engine04::{sentence04, synth_request04, Rates04, Sentence04};
use vtpaul::era::Era;
use vtpaul::json::Json;
use vtpaul::lexicon::Lexicon;
use vtpaul::ling::Ws;
use vtpaul::ling04::sentence::{self, Ss04};
use vtpaul::ling04::{merge, Wa04};
use vtpaul::prosody::Trees;
use vtpaul::unitsel::{CepDist, ScalarDist};
use vtpaul::unitsel04::{Cls04, Selector04};
use vtpaul::wave04::Channel04;

const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};
const VOICES: &str = match option_env!("VTPAUL_VOICES") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};

fn hexb(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap_or(0)).collect()
}

fn read_wav(p: &Path) -> Vec<i16> {
    let b = std::fs::read(p).unwrap();
    let mut pos = 12;
    while pos + 8 <= b.len() {
        let id = &b[pos..pos + 4];
        let sz = u32::from_le_bytes(b[pos + 4..pos + 8].try_into().unwrap()) as usize;
        if id == b"data" {
            return b[pos + 8..(pos + 8 + sz).min(b.len())].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        }
        pos += 8 + sz + (sz & 1);
    }
    Vec::new()
}

fn wa_from(e: &Json) -> Wa04 {
    let mut wa = Wa04::new();
    let head = hexb(e.get("head").str().unwrap_or(""));
    wa.b[..head.len()].copy_from_slice(&head);
    for (i, r) in e.get("recs").arr().iter().enumerate() {
        let b = hexb(r.str().unwrap_or(""));
        let o = Wa04::rec(i);
        wa.b[o..o + b.len()].copy_from_slice(&b);
    }
    wa
}

/// Texts the two harnesses feed differently: `capture_old.py` writes cp1252 with '?' for what cp1252 lacks,
/// the linguistic capture passes argv, which Windows best-fits ("ＡＢＣ" -> "ABC").
fn cp1252_safe(t: &str) -> bool {
    const HI: &str = "€‚ƒ„…†‡ˆ‰Š‹ŒŽ‘’“”•–—˜™š›œžŸ";
    t.chars().all(|c| (c as u32) < 0x80 || (0xa0..=0xff).contains(&(c as u32)) || HI.contains(c))
}

fn ints(j: &Json) -> Vec<i32> {
    j.arr().iter().map(|v| v.i64().unwrap_or(-1) as i32).collect()
}

#[test]
fn e2e_paul04() {
    let root = Path::new(VOICES).join("2004_paul");
    let m16 = root.join("data-paul").join("M16");
    let caps = Path::new(HARNESS).join("out_feling_paul04");
    let refs = Path::new(HARNESS).join("out_old").join("paul04");
    if !m16.exists() || !caps.exists() || !refs.exists() {
        eprintln!("skipping: voice data or captures missing");
        return;
    }
    let only = std::env::var("E2E_ONLY").ok();
    let limit: usize = std::env::var("E2E_LIMIT").ok().and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&refs)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "wav"))
        .filter(|p| caps.join(format!("{}.json", p.file_stem().unwrap().to_string_lossy())).exists())
        .filter(|p| only.as_deref().is_none_or(|o| o == p.file_stem().unwrap().to_string_lossy()))
        .collect();
    let corpus = std::fs::read_to_string(Path::new(HARNESS).join("corpus_feling").join("speak_paul04.tsv")).unwrap_or_default();
    let unsafe_names: Vec<&str> =
        corpus.lines().filter_map(|l| l.split_once('\t')).filter(|(_, t)| !cp1252_safe(t)).map(|(n, _)| n).collect();
    let before = files.len();
    files.retain(|p| !unsafe_names.contains(&p.file_stem().unwrap().to_string_lossy().as_ref()));
    eprintln!("skipping {} texts with characters outside cp1252", before - files.len());
    files.sort();
    files.truncate(limit);
    let lex = Lexicon::open_era(&root.join("data-common").join("dict-eng"), Era::Paul04).unwrap();
    let trees = Trees::load_tree2(&m16.join("ttsdata").join("tree")).unwrap();
    let bank = Bank::open(&m16).unwrap();
    let cls = Cls04::build(&bank);
    let cep = CepDist::load(&m16.join("ttsdata/dist_tbl/cepdist.tbl")).unwrap();
    let sd = ScalarDist::build();
    let nthreads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(files.len().max(1)).min(20);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let res = std::sync::Mutex::new((0usize, Vec::<String>::new()));
    std::thread::scope(|sc| {
        for _ in 0..nthreads {
            sc.spawn(|| {
                let mut sel = Selector04::new(&bank, &cls, &cep, &sd);
                let mut ws = Ws::new();
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if i >= files.len() {
                        break;
                    }
                    let wav = &files[i];
                    let name = wav.file_stem().unwrap().to_string_lossy().into_owned();
                    let cap = Json::load(&caps.join(format!("{}.json", name))).unwrap();
                    let ch = Channel04::default();
                    let mut sents: Vec<Sentence04> = Vec::new();
                    let mut merged: Option<Wa04> = None;
                    for e in cap.get("events").arr() {
                        match e.get("kind").str().unwrap_or("") {
                            "wa_in" => {
                                let mut wa = wa_from(e);
                                merge::analyze(&mut wa, &mut ws, &lex, 0);
                                merged = Some(wa);
                            }
                            "ctx_out" => {
                                let Some(m) = merged.as_ref() else { continue };
                                let tib = e.get("tib");
                                let mut ss = Ss04::new();
                                ss.set_i16(2, m.n() as i16);
                                sentence::build_words(&mut ss, m);
                                if !sentence::assign_breaks(&mut ss, &ints(tib.get("brk_level"))) {
                                    continue;
                                }
                                sentence::build_contexts(&mut ss);
                                let rates = Rates04 {
                                    pause: ints(tib.get("pause")),
                                    speed: ints(tib.get("speed")),
                                    volume: ints(tib.get("volume")),
                                    pitch: ints(tib.get("pitch")),
                                };
                                sents.push(sentence04(&mut ss, &trees, &mut sel, &rates, &ch));
                            }
                            _ => {}
                        }
                    }
                    let want = read_wav(wav);
                    let got = synth_request04(&bank, &sents, &ch).map(|s| s.pcm).unwrap_or_default();
                    let mut g = res.lock().unwrap();
                    if got == want {
                        g.0 += 1;
                    } else {
                        let k = (0..want.len().min(got.len())).find(|&k| got[k] != want[k]).unwrap_or(want.len().min(got.len()));
                        g.1.push(format!("{} (len {} want {}, first diff {})", name, got.len(), want.len(), k));
                    }
                }
            });
        }
    });
    let (ok, mut bad) = res.into_inner().unwrap();
    bad.sort();
    for b in bad.iter().take(60) {
        eprintln!("MISMATCH {}", b);
    }
    eprintln!("e2e paul04: {}/{} bit-exact", ok, files.len());
    assert_eq!(ok, files.len());
}
