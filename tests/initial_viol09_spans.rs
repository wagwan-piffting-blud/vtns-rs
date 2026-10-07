//! viol09 VTML span lists built by `fe_InitialTextProcess` against the `spans` events of
//! `out_spans_viol09/*.json` (the 5 lists as `fe_NextSentence` hands them to `pro_ApplyTagSpansToWords`).
//!
//!     cargo test --release --test initial_viol09_spans -- --nocapture

use std::path::Path;

use vtpaul::json::Json;
use vtpaul::text_spa::initial::initial_spa;

const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};

#[test]
fn span_lists_match_viol09() {
    let dir = Path::new(HARNESS).join("out_spans_viol09");
    if !dir.exists() {
        eprintln!("skipping: no captures");
        return;
    }
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    let (mut ok, mut total) = (0, 0);
    for f in &files {
        let doc = Json::load(f).unwrap();
        let text = doc.get("text").str().unwrap().to_string();
        let ini = initial_spa(text.as_bytes());
        let Some(ev) = doc.get("events").arr().iter().find(|e| e.get("kind").str() == Some("spans")) else { continue };
        total += 1;
        let mut good = true;
        for (k, l) in ev.get("spans").arr().iter().enumerate() {
            let pos: Vec<i32> = l.get("pos").arr().iter().map(|v| v.i64().unwrap_or(0) as i32).collect();
            let val: Vec<i32> = l.get("val").arr().iter().map(|v| v.i64().unwrap_or(0) as i32).collect();
            let (gp, gv) = match ini.chans.get(k) {
                Some(c) => (c.pos.clone(), c.val.clone()),
                None => (Vec::new(), Vec::new()),
            };
            if gp != pos || gv != val {
                good = false;
                println!("  {} list {}: got pos {:?} val {:?} want pos {:?} val {:?}", f.file_stem().unwrap().to_string_lossy(), k, gp, gv, pos, val);
            }
        }
        if good {
            ok += 1;
        }
    }
    println!("span lists {}/{}", ok, total);
    assert!(total > 0);
    assert_eq!(ok, total);
}
