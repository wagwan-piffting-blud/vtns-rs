//! 2004 Paul text front end against engine captures (`out_fetext_paul04/*.jsonl`).
//!
//!   cargo test --release --test fetext_paul04_parity -- --nocapture
//!   FETEXT_CORPUS=synthetic FETEXT_SHOW=20 FETEXT_ONLY="text" cargo test ...

use std::path::{Path, PathBuf};

use vtpaul::json::Json;
use vtpaul::text04::initial::initial;
use vtpaul::text04::{sentences, Fe, Wa};

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

fn show(b: &[u8]) -> String {
    b.iter().map(|&c| if (0x20..0x7f).contains(&c) { c as char } else { '\u{b7}' }).collect()
}

fn ints(j: &Json) -> Vec<i32> {
    j.arr().iter().map(|v| v.i64().unwrap_or(-1) as i32).collect()
}

fn files() -> Vec<PathBuf> {
    let corpus = std::env::var("FETEXT_CORPUS").unwrap_or_else(|_| "all".into());
    let dir = Path::new(HARNESS).join("out_fetext_paul04");
    if !dir.exists() {
        return Vec::new();
    }
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .filter(|p| corpus == "all" || p.file_stem().unwrap().to_string_lossy() == corpus)
        .collect();
    v.sort();
    v
}

fn lines() -> Vec<Json> {
    let only = std::env::var("FETEXT_ONLY").ok();
    let mut v = Vec::new();
    for f in files() {
        for l in std::fs::read_to_string(&f).unwrap().lines() {
            let j = Json::parse(l).unwrap();
            if only.as_deref().is_some_and(|o| j.get("text").str() != Some(o)) {
                continue;
            }
            v.push(j);
        }
    }
    v
}

fn describe(r: &[u8]) -> String {
    let i = |o: usize| i32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]);
    let t = &r[0x1e..];
    let t = &t[..t.iter().position(|&c| c == 0).unwrap_or(t.len())];
    let p = &r[0x3c..];
    let p = &p[..p.iter().position(|&c| c == 0).unwrap_or(p.len())];
    format!("[{},{}) len={} '{}' {:02x}{:02x}{:02x} ph={:02x?}", i(0), i(4), i(8), show(t), r[0x18], r[0x19], r[0x1a], p)
}

struct EngWa {
    off: i32,
    n: i32,
    consumed: i32,
    recs: Vec<Vec<u8>>,
}

fn engine_sentences(rec: &Json) -> Vec<EngWa> {
    let mut v = Vec::new();
    for s in rec.get("sentences").arr() {
        for w in s.get("wa").arr() {
            let n = w.get("n").i64().unwrap_or(0) as i32;
            let consumed = w.get("consumed").i64().unwrap_or(0) as i32;
            if n > 0 && consumed != 0 {
                v.push(EngWa {
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

#[test]
fn normalization_parity() {
    let show_n: usize = std::env::var("FETEXT_SHOW").ok().and_then(|s| s.parse().ok()).unwrap_or(10);
    let common = Path::new(VOICES).join("2004_paul").join("data-common");
    let all = lines();
    if all.is_empty() || !common.exists() {
        eprintln!("skipping: captures or voice data not present");
        return;
    }
    let fe = Fe::load(&common).expect("load 2004 front-end data");
    let verbose = std::env::var("FETEXT_ONLY").is_ok();
    let (mut nl, mut lok, mut ns, mut sok, mut nr, mut rok, mut shown) = (0, 0, 0, 0, 0, 0, 0);
    for j in &all {
        let Some(p) = j.get("initial").get("processed").str() else { continue };
        nl += 1;
        let eng = engine_sentences(j);
        let ours = sentences(&fe, &hexb(p));
        if verbose {
            for (k, e) in eng.iter().enumerate() {
                eprintln!("eng  sent {} off={} n={} consumed={}", k, e.off, e.n, e.consumed);
                for r in &e.recs {
                    eprintln!("      {}", describe(r));
                }
            }
            for (k, o) in ours.iter().enumerate() {
                eprintln!("ours sent {} off={} n={} consumed={} ret={}", k, o.text_off, o.wa.n(), o.wa.consumed(), o.ret);
                for i in 0..o.wa.n() {
                    eprintln!("      {}", describe(&o.wa.b[Wa::rec(i)..Wa::rec(i) + 0x88]));
                }
            }
        }
        let mut line_ok = eng.len() == ours.len();
        let mut first_bad: Option<String> = None;
        for (k, e) in eng.iter().enumerate() {
            ns += 1;
            let o = ours.get(k);
            let mut s_ok = o.is_some_and(|o| o.text_off == e.off && o.wa.n() == e.n && o.wa.consumed() == e.consumed);
            if !s_ok && first_bad.is_none() {
                first_bad = Some(format!(
                    "    sent {}: eng off={} n={} consumed={} | ours {}",
                    k,
                    e.off,
                    e.n,
                    e.consumed,
                    o.map(|o| format!("off={} n={} consumed={}", o.text_off, o.wa.n(), o.wa.consumed())).unwrap_or_else(|| "-".into())
                ));
            }
            for (i, er) in e.recs.iter().enumerate() {
                nr += 1;
                let ob = o.map(|o| o.wa.b[Wa::rec(i as i32)..Wa::rec(i as i32) + er.len()].to_vec());
                if ob.as_deref() == Some(er.as_slice()) {
                    rok += 1;
                } else {
                    s_ok = false;
                    if first_bad.is_none() {
                        first_bad = Some(format!(
                            "    sent {} rec {}:\n      eng  {}\n      ours {}",
                            k,
                            i,
                            describe(er),
                            ob.map(|b| describe(&b)).unwrap_or_else(|| "-".into())
                        ));
                    }
                }
            }
            if s_ok {
                sok += 1;
            } else {
                line_ok = false;
            }
        }
        if line_ok {
            lok += 1;
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
    eprintln!(
        "fetext paul04: lines {}/{} ({:.1}%), sentences {}/{}, records {}/{} ({:.1}%)",
        lok,
        nl,
        100.0 * lok as f64 / nl.max(1) as f64,
        sok,
        ns,
        rok,
        nr,
        100.0 * rok as f64 / nr.max(1) as f64
    );
    // The recognizer chain is still being ported: hold the line count, demand everything with FETEXT_STRICT=1.
    if std::env::var("FETEXT_STRICT").is_ok() || std::env::var("FETEXT_CORPUS").is_ok() || std::env::var("FETEXT_ONLY").is_ok() {
        if std::env::var("FETEXT_STRICT").is_ok() {
            assert_eq!(lok, nl);
        }
    } else {
        assert!(lok >= MIN_LINES, "fetext paul04 regressed: {} < {}", lok, MIN_LINES);
    }
}

/// Lines bit-exact at the last checkpoint of the port (all corpora).
const MIN_LINES: usize = 5067;

#[test]
fn initial_parity() {
    let show_n: usize = std::env::var("FETEXT_SHOW").ok().and_then(|s| s.parse().ok()).unwrap_or(10);
    let all = lines();
    if all.is_empty() {
        eprintln!("skipping: captures not present");
        return;
    }
    let (mut n, mut ok, mut shown) = (0, 0, 0);
    for j in &all {
        let ini = j.get("initial");
        let Some(src) = ini.get("src").str() else { continue };
        let Some(want) = ini.get("processed").str() else { continue };
        n += 1;
        let got = initial(&hexb(src));
        let mut bad = Vec::new();
        if got.processed != hexb(want) {
            bad.push(format!("processed\n      eng  {}\n      ours {}", show(&hexb(want)), show(&got.processed)));
        }
        for (name, ours) in [("map_a", &got.map_a), ("map_b", &got.map_b), ("map_c", &got.map_c)] {
            let w = ints(ini.get(name));
            if &w != ours {
                bad.push(format!("{} eng {:?}\n      ours {:?}", name, w, ours));
            }
        }
        for (k, t) in ini.get("tags").arr().iter().enumerate() {
            let (pos, val) = (ints(t.get("pos")), ints(t.get("val")));
            if pos != got.stacks[k].pos || val != got.stacks[k].val {
                bad.push(format!("tags[{}] eng {:?} {:?}\n      ours {:?} {:?}", k, pos, val, got.stacks[k].pos, got.stacks[k].val));
            }
        }
        if bad.is_empty() {
            ok += 1;
        } else if shown < show_n {
            shown += 1;
            eprintln!("MISMATCH {:?}", j.get("text").str().unwrap_or(""));
            for b in bad.iter().take(2) {
                eprintln!("    {}", b);
            }
        }
    }
    eprintln!("initial paul04: {}/{}", ok, n);
    assert_eq!(ok, n);
}
