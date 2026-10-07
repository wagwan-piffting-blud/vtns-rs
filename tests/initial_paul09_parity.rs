//! `fe_InitialTextProcess` parity: raw request bytes -> processed text,
//! processed -> original offset maps, the 8 VTML span channels and the mark
//! list, against every `out_fetext/*.jsonl` capture (`initial`).
//!
//!   cargo test --release --test initial_parity -- --nocapture

use std::path::Path;

use vtpaul::json::Json;
use vtpaul::era::Era;
use vtpaul::text::initial::initial_era;

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

#[test]
fn initial_text_parity() {
    let dir = Path::new(HARNESS).join("out_fetext_paul09");
    if !dir.exists() {
        eprintln!("skipping: no captures");
        return;
    }
    let show_n: usize = std::env::var("INITIAL_SHOW").ok().and_then(|s| s.parse().ok()).unwrap_or(15);
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "jsonl")).collect();
    files.sort();
    let (mut total, mut ok, mut shown) = (0, 0, 0);
    let mut seen = std::collections::HashSet::new();
    for f in &files {
        for line in std::fs::read_to_string(f).unwrap().lines() {
            let j = Json::parse(line).unwrap();
            let ini = j.get("initial");
            let (Some(src), Some(proc_hex)) = (ini.get("src").str(), ini.get("processed").str()) else { continue };
            if !seen.insert(src.to_string()) {
                continue;
            }
            total += 1;
            let want = hexb(proc_hex);
            let got = initial_era(&hexb(src), Era::Paul09);
            let n = want.len();
            let mut bad: Option<String> = None;
            if got.processed != want {
                bad = Some(format!("processed {:02x?}
      want      {:02x?}", got.processed, want));
            } else {
                let wb = ints(ini.get("map_b"));
                let wc = ints(ini.get("map_c"));
                if got.map_b[..] != wb[..n.min(wb.len())] || got.map_c[..] != wc[..n.min(wc.len())] {
                    bad = Some(format!("maps b {:?} c {:?}\n      want b {:?} c {:?}", got.map_b, got.map_c, &wb[..n.min(wb.len())], &wc[..n.min(wc.len())]));
                }
                for (k, t) in ini.get("tags").arr().iter().enumerate() {
                    let (wp, wv) = (ints(t.get("pos")), ints(t.get("val")));
                    if bad.is_none() && (got.chans[k].pos != wp || got.chans[k].val != wv) {
                        bad = Some(format!("chan {} {:?} {:?} want {:?} {:?}", k, got.chans[k].pos, got.chans[k].val, wp, wv));
                    }
                }
                let wm = ini.get("marks").get("marks").arr();
                let gm: Vec<(i32, Vec<u8>)> = got.marks.iter().map(|m| (m.pos, m.name.clone())).collect();
                let wm: Vec<(i32, Vec<u8>)> = wm
                    .iter()
                    .map(|m| {
                        let h = hexb(m.get("hdr").str().unwrap_or(""));
                        (i32::from_le_bytes([h[8], h[9], h[10], h[11]]), hexb(m.get("name").str().unwrap_or("")))
                    })
                    .collect();
                if bad.is_none() && gm != wm {
                    bad = Some(format!("marks {:?} want {:?}", gm, wm));
                }
                if bad.is_none() {
                    if let Some(ov_hex) = ini.get("override").str() {
                        let ov = hexb(ov_hex);
                        let want_ov: Vec<i32> = ov.chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
                        if got.override_table[..] != want_ov[..] {
                            bad = Some(format!("override {:?} want {:?}", got.override_table, want_ov));
                        }
                    }
                }
                if bad.is_none() {
                    let wpo = ints(ini.get("pause_override"));
                    if !wpo.is_empty() && [got.pause_override.0, got.pause_override.1] != wpo[..] {
                        bad = Some(format!("pause_override {:?} want {:?}", got.pause_override, wpo));
                    }
                }
            }
            match bad {
                None => ok += 1,
                Some(b) => {
                    if shown < show_n {
                        shown += 1;
                        println!("MISMATCH {:?}\n      {}", j.get("text").str().unwrap_or(""), b);
                    }
                }
            }
        }
    }
    println!("initial text parity: {}/{} distinct inputs exact", ok, total);
    assert_eq!(ok, total, "initial text parity (paul09)");
}
