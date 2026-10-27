//! 2004 Paul from raw text to PCM: every text of `out_old/paul04/texts.tsv`, encoded as the reference
//! harness fed it (cp1252, `?` for what it lacks), through the port's text front end, linguistic stage,
//! SS passes, prosody, unit selection and waveform generation, against the engine's WAV.
//!
//!   cargo test --release --test e2e_text_paul04_parity -- --nocapture      (E2E_ONLY=name, E2E_LIMIT=n)

use std::path::Path;

use vtpaul::bank::Bank;
use vtpaul::engine04::{front04, sentence04, synth_request04, Sentence04};
use vtpaul::ling::Ws;
use vtpaul::ling04::merge;
use vtpaul::ling04::sentence::{self, Ss04};
use vtpaul::prosody::Trees;
use vtpaul::text04::Fe;
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

/// Python's `str.encode("cp1252", "replace")`.
fn cp1252(t: &str) -> Vec<u8> {
    const HI: [(char, u8); 27] = [
        ('€', 0x80),
        ('‚', 0x82),
        ('ƒ', 0x83),
        ('„', 0x84),
        ('…', 0x85),
        ('†', 0x86),
        ('‡', 0x87),
        ('ˆ', 0x88),
        ('‰', 0x89),
        ('Š', 0x8a),
        ('‹', 0x8b),
        ('Œ', 0x8c),
        ('Ž', 0x8e),
        ('‘', 0x91),
        ('’', 0x92),
        ('“', 0x93),
        ('”', 0x94),
        ('•', 0x95),
        ('–', 0x96),
        ('—', 0x97),
        ('˜', 0x98),
        ('™', 0x99),
        ('š', 0x9a),
        ('›', 0x9b),
        ('œ', 0x9c),
        ('ž', 0x9e),
        ('Ÿ', 0x9f),
    ];
    t.chars()
        .map(|c| {
            let u = c as u32;
            if u < 0x80 || (0xa0..=0xff).contains(&u) {
                u as u8
            } else {
                HI.iter().find(|&&(h, _)| h == c).map_or(b'?', |&(_, b)| b)
            }
        })
        .collect()
}

#[test]
fn e2e_text_paul04() {
    let root = Path::new(VOICES).join("2004_paul");
    let m16 = root.join("data-paul").join("M16");
    let refs = Path::new(HARNESS).join("out_old").join("paul04");
    let Ok(tsv) = std::fs::read_to_string(refs.join("texts.tsv")) else {
        eprintln!("skipping: reference WAVs missing");
        return;
    };
    if !m16.exists() {
        eprintln!("skipping: voice data missing");
        return;
    }
    let only = std::env::var("E2E_ONLY").ok();
    let limit: usize = std::env::var("E2E_LIMIT").ok().and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
    let mut items: Vec<(String, String)> = tsv
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(n, t)| (n.to_string(), t.to_string()))
        .filter(|(n, _)| refs.join(format!("{}.wav", n)).exists())
        .filter(|(n, _)| only.as_deref().is_none_or(|o| o == n))
        .collect();
    items.sort();
    items.truncate(limit);
    let fe = Fe::load(&root.join("data-common")).unwrap();
    let trees = Trees::load_tree2(&m16.join("ttsdata").join("tree")).unwrap();
    let bank = Bank::open(&m16).unwrap();
    let cls = Cls04::build(&bank);
    let cep = CepDist::load(&m16.join("ttsdata/dist_tbl/cepdist.tbl")).unwrap();
    let sd = ScalarDist::build();
    let nthreads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(items.len().max(1)).min(20);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let res = std::sync::Mutex::new((0usize, Vec::<String>::new()));
    std::thread::scope(|sc| {
        for _ in 0..nthreads {
            sc.spawn(|| {
                let mut sel = Selector04::new(&bank, &cls, &cep, &sd);
                let mut ws = Ws::new();
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if i >= items.len() {
                        break;
                    }
                    let (name, text) = &items[i];
                    let ch = Channel04::default();
                    let mut sents: Vec<Sentence04> = Vec::new();
                    for f in front04(&fe, &cp1252(text), &ch) {
                        let mut wa = f.wa;
                        merge::analyze(&mut wa, &mut ws, &fe.lex, 0);
                        let mut ss = Ss04::new();
                        ss.set_i16(2, wa.n() as i16);
                        sentence::build_words(&mut ss, &wa);
                        if !sentence::assign_breaks(&mut ss, &f.brk) {
                            continue;
                        }
                        sentence::build_contexts(&mut ss);
                        sents.push(sentence04(&mut ss, &trees, &mut sel, &f.rates, &ch));
                    }
                    let want = read_wav(&refs.join(format!("{}.wav", name)));
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
    eprintln!("e2e text paul04: {}/{} bit-exact", ok, items.len());
    assert_eq!(ok, items.len());
}
