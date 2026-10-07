//! `fe_InitialTextProcess` (Violeta, 0x10007730) parity: raw request bytes ->
//! processed text and the processed -> original offset maps, against
//! `out_fetext_viol09/*.jsonl` (`initial`).
//!
//! NOTE ON SOURCE: the individual task brief named `capture_initial_spa.py`/
//! `out_initial_spa` for this, but that script's `hooks_spa.js` never hooks
//! `fe_InitialTextProcess` at all (it captures the *later* sentence/prosody/
//! selection/record stages under an unrelated name). The capture that
//! actually emits an `"initial"` object with `src`/`processed`/`map_b`/
//! `map_c` is `capture_fetext_spa.py` + `hooks_fetext_spa.js` -> `out_fetext_viol09`,
//! which is also what the shared batch brief's own rule 4 tells every agent
//! to run for their probe. This test reads that capture instead.
//!
//! `hooks_fetext_spa.js` itself documents (see its `install()`) that the
//! channel/mark-list TIB fields are not captured because the table-walk
//! callback has no Ghidra function boundary -- the same wall this port hit
//! independently (see `src/text_spa/initial.rs`'s module doc). So only
//! `processed`/`map_b`/`map_c` are checked here, not channels/marks.
//!
//!   cargo test --release --test initial_spa_parity -- --nocapture
//!
//! `INITIAL_SPA_EXTRA_DIR=<dir>` additionally checks every `*.jsonl` in
//! `<dir>` (the probe capture).

use std::path::Path;

use vtpaul::json::Json;
use vtpaul::text_spa::initial::initial_spa;

/// Capture root; override at build time with `VTPAUL_HARNESS=<dir>`.
const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};

fn hexb(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap_or(0)).collect()
}

fn ints(j: &Json) -> Vec<i32> {
    j.arr().iter().map(|x| x.i64().unwrap_or(0) as i32).collect()
}

fn check_dir(dir: &Path, show_n: usize, total: &mut usize, ok: &mut usize, shown: &mut usize) {
    if !dir.exists() {
        eprintln!("skipping {}: no captures", dir.display());
        return;
    }
    let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "jsonl")).collect();
    files.sort();
    let mut seen = std::collections::HashSet::new();
    for f in &files {
        for line in std::fs::read_to_string(f).unwrap().lines() {
            let Ok(j) = Json::parse(line) else { continue };
            let ini = j.get("initial");
            let (Some(src), Some(proc_hex)) = (ini.get("src").str(), ini.get("processed").str()) else { continue };
            if !seen.insert(src.to_string()) {
                continue;
            }
            *total += 1;
            let want = hexb(proc_hex);
            let got = initial_spa(&hexb(src));
            let n = want.len();
            let mut bad: Option<String> = None;
            if got.processed != want {
                bad = Some(format!("processed {:?}\n      want {:?}", String::from_utf8_lossy(&got.processed), String::from_utf8_lossy(&want)));
            } else {
                let wb = ints(ini.get("map_b"));
                let wc = ints(ini.get("map_c"));
                if got.map_b[..] != wb[..n.min(wb.len())] || got.map_c[..] != wc[..n.min(wc.len())] {
                    bad = Some(format!("maps b {:?} c {:?}\n      want b {:?} c {:?}", got.map_b, got.map_c, &wb[..n.min(wb.len())], &wc[..n.min(wc.len())]));
                }
            }
            match bad {
                None => *ok += 1,
                Some(b) => {
                    if *shown < show_n {
                        *shown += 1;
                        println!("MISMATCH {:?}\n      {}", j.get("text").str().unwrap_or(""), b);
                    }
                }
            }
        }
    }
}

#[test]
fn initial_spa_text_parity() {
    let show_n: usize = std::env::var("INITIAL_SPA_SHOW").ok().and_then(|s| s.parse().ok()).unwrap_or(15);
    let (mut total, mut ok, mut shown) = (0, 0, 0);
    check_dir(&Path::new(HARNESS).join("out_fetext_viol09"), show_n, &mut total, &mut ok, &mut shown);
    if let Ok(extra) = std::env::var("INITIAL_SPA_EXTRA_DIR") {
        check_dir(Path::new(&extra), show_n, &mut total, &mut ok, &mut shown);
    }
    println!("initial_viol09 text parity: {}/{} distinct inputs exact", ok, total);
    assert!(total > 0, "no capture lines read");
    assert_eq!(ok, total, "viol09 initial text parity below 100%");
}
