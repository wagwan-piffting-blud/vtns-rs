//! Violeta text front-end parity against engine captures (`out_fetext_viol09/*.jsonl`).
//!
//!   cargo test --release --test fetext_spa_parity -- --nocapture
//!   FETEXT_CORPUS=synthetic FETEXT_SHOW=20 cargo test ...
//!
//! Stage 1 (normalization) feeds the engine's own processed text to
//! `text_spa::sentences` and compares every WA sentence: offset, word count,
//! consumed bytes and each 0xa8-byte record.

use std::path::{Path, PathBuf};

use vtpaul::json::Json;
use vtpaul::ling_spa::WaSpa;
use vtpaul::text_spa::{self, FeSpa};

/// Capture root; override at build time with `VTPAUL_HARNESS=<dir>`.
const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};
/// Voice data root; override at build time with `VTPAUL_VOICES=<dir>`.
const VOICES: &str = match option_env!("VTPAUL_VOICES") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};

fn hexb(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap_or(0)).collect()
}

fn show(b: &[u8]) -> String {
    b.iter().map(|&c| if (0x20..0x7f).contains(&c) { c as char } else { '\u{b7}' }).collect()
}

struct Engine {
    off: i32,
    n: i32,
    consumed: i32,
    recs: Vec<Vec<u8>>,
}

fn engine_sentences(rec: &Json) -> Vec<Engine> {
    let mut v = Vec::new();
    for s in rec.get("sentences").arr() {
        for w in s.get("wa").arr() {
            let n = w.get("n").i64().unwrap_or(0) as i32;
            let consumed = w.get("consumed").i64().unwrap_or(0) as i32;
            if n > 0 && consumed != 0 {
                v.push(Engine {
                    off: w.get("text_off").i64().unwrap_or(-1) as i32,
                    n,
                    consumed,
                    recs: w.get("recs").arr().iter().map(|r| hexb(r.get("raw").str().unwrap_or(""))).collect(),
                });
            }
        }
    }
    v
}

fn describe(r: &[u8]) -> String {
    let i = |o: usize| i32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]);
    let t = &r[0x24..];
    let t = &t[..t.iter().position(|&c| c == 0).unwrap_or(t.len())];
    format!(
        "[{},{}) '{}' {:02x}{:02x} src={:#x} p20={} f10={:02x} ph={}",
        i(0),
        i(4),
        show(t),
        r[0x16],
        r[0x17],
        i(0x1c),
        i(0x20),
        r[0x10],
        r[0x42..0x50].iter().map(|b| format!("{:02x}", b)).collect::<String>()
    )
}

#[test]
fn normalization_parity() {
    let corpus = std::env::var("FETEXT_CORPUS").unwrap_or_else(|_| "all".into());
    let show_n: usize = std::env::var("FETEXT_SHOW").ok().and_then(|s| s.parse().ok()).unwrap_or(10);
    let dir = Path::new(HARNESS).join("out_fetext_viol09");
    let dict = Path::new(VOICES).join("old_julie_violeta").join("Violeta").join("data-common").join("dict-spa");
    if !dir.exists() || !dict.exists() {
        eprintln!("skipping: captures or voice data not present");
        return;
    }
    let fe = FeSpa::load_era(&dict, vtpaul::era::Era::Violeta09).expect("load front-end data");
    let list_jsonl = |dir: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
                    .filter(|p| {
                        let n = p.file_stem().unwrap().to_string_lossy().to_string();
                        !n.starts_with("cov_") && (corpus == "all" || n == corpus)
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut files: Vec<PathBuf> = list_jsonl(&dir);
    // Extra capture directory (e.g. a probe corpus), outside harness\out_fetext_viol09.
    if let Ok(extra) = std::env::var("FETEXT_EXTRA_DIR") {
        files.extend(list_jsonl(Path::new(&extra)));
    }
    files.sort();
    let (mut lines, mut lines_ok, mut sent, mut sent_ok, mut recs, mut recs_ok) = (0, 0, 0, 0, 0, 0);
    let mut shown = 0;
    for f in &files {
        for line in std::fs::read_to_string(f).unwrap().lines() {
            let j = Json::parse(line).unwrap();
            let processed = match j.get("initial").get("processed").str() {
                Some(h) => hexb(h),
                None => continue,
            };
            lines += 1;
            let eng = engine_sentences(&j);
            let ours = text_spa::sentences(&fe, &processed);
            if let Ok(only) = std::env::var("FETEXT_ONLY") {
                if j.get("text").str() == Some(only.as_str()) {
                    for (k, e) in eng.iter().enumerate() {
                        eprintln!("eng  sent {} off={} n={} consumed={}", k, e.off, e.n, e.consumed);
                        for r in &e.recs {
                            eprintln!("      {}", describe(r));
                        }
                    }
                    for (k, o) in ours.iter().enumerate() {
                        eprintln!("ours sent {} off={} n={} consumed={}", k, o.text_off, o.wa.i16(0), o.wa.i32(4));
                        for i in 0..o.wa.i16(0) as usize {
                            let b = WaSpa::rec(i);
                            eprintln!("      {}", describe(&o.wa.b[b..b + 0xa8]));
                        }
                    }
                }
            }
            let mut line_ok = eng.len() == ours.len();
            let mut first_bad: Option<String> = None;
            for (k, e) in eng.iter().enumerate() {
                sent += 1;
                let o = ours.get(k);
                let mut s_ok = o.is_some_and(|o| o.text_off == e.off && o.wa.i16(0) as i32 == e.n && o.wa.i32(4) == e.consumed);
                for (i, er) in e.recs.iter().enumerate() {
                    recs += 1;
                    let ob = o.map(|o| {
                        let b = WaSpa::rec(i);
                        o.wa.b[b..b + er.len()].to_vec()
                    });
                    if ob.as_deref() == Some(er.as_slice()) {
                        recs_ok += 1;
                    } else {
                        s_ok = false;
                        if first_bad.is_none() {
                            let diff: Vec<String> = ob
                                .as_deref()
                                .map(|b| (0..er.len()).filter(|&x| b[x] != er[x]).map(|x| format!("+{:#x}:{:02x}/{:02x}", x, er[x], b[x])).take(8).collect())
                                .unwrap_or_default();
                            first_bad = Some(format!(
                                "    sent {} rec {}:\n      eng  {}\n      ours {}\n      bytes eng/ours {}",
                                k,
                                i,
                                describe(er),
                                ob.as_deref().map(describe).unwrap_or_else(|| "-".into()),
                                diff.join(" ")
                            ));
                        }
                    }
                }
                if s_ok {
                    sent_ok += 1;
                } else {
                    line_ok = false;
                    if first_bad.is_none() {
                        first_bad = Some(format!(
                            "    sent {}: eng off={} n={} consumed={} | ours {}",
                            k,
                            e.off,
                            e.n,
                            e.consumed,
                            o.map(|o| format!("off={} n={} consumed={}", o.text_off, o.wa.i16(0), o.wa.i32(4))).unwrap_or_else(|| "-".into())
                        ));
                    }
                }
            }
            if line_ok {
                lines_ok += 1;
            } else if shown < show_n {
                shown += 1;
                eprintln!("MISMATCH {:?}", j.get("text").str().unwrap_or(""));
                if eng.len() != ours.len() {
                    eprintln!("    sentences: eng {} ours {}", eng.len(), ours.len());
                }
                if let Some(b) = first_bad {
                    eprintln!("{}", b);
                }
            }
        }
    }
    eprintln!(
        "fetext_spa normalization parity: lines {}/{} ({:.1}%), sentences {}/{}, records {}/{} ({:.1}%)",
        lines_ok,
        lines,
        100.0 * lines_ok as f64 / lines.max(1) as f64,
        sent_ok,
        sent,
        recs_ok,
        recs,
        100.0 * recs_ok as f64 / recs.max(1) as f64
    );
    assert!(lines > 0, "no capture lines read");
    assert_eq!((lines_ok, sent_ok, recs_ok), (lines, sent, recs), "viol09 fetext parity below 100%");
}
