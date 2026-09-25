use std::path::Path;

use vtpaul::json::Json;

/// Capture root; override at build time with `VTPAUL_HARNESS=<dir>`.
const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};

fn out() -> &'static str {
    Box::leak(format!("{}/out", HARNESS).into_boxed_str())
}

#[test]
fn every_capture_parses() {
    let dir = Path::new(out());
    if !dir.exists() {
        eprintln!("skipping: {} not present", out());
        return;
    }
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "json") && !p.file_name().unwrap().to_string_lossy().starts_with('_') {
            let j = Json::load(&p).unwrap_or_else(|e| panic!("{}: {}", p.display(), e));
            assert!(j.get("sentences").arr().len() > 0, "{}", p.display());
            n += 1;
        }
    }
    assert!(n >= 40, "only {} captures", n);
}
