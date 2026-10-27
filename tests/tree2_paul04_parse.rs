//! Every prosody `.tree2` of the 2004 Paul install parses to its exact length, and the bank is detected as Paul04.

use std::path::{Path, PathBuf};

use vtpaul::bank::Bank;
use vtpaul::era::Era;
use vtpaul::lang::Lang;
use vtpaul::tree2;

const ROOT: &str = match option_env!("VTPAUL_VOICES") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};

fn paul04() -> PathBuf {
    Path::new(ROOT).join("2004_paul")
}

#[test]
fn paul04_trees_parse_and_era_detects() {
    let root = paul04();
    let m16 = root.join("data-paul").join("M16");
    if !m16.exists() {
        eprintln!("skipping: no 2004 Paul data under {}", root.display());
        return;
    }
    let tree = m16.join("ttsdata").join("tree");
    let mut n = 0;
    for sub in ["duration", "pitch"] {
        for e in std::fs::read_dir(tree.join(sub)).unwrap() {
            let p = e.unwrap().path();
            let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
            let vector = sub == "pitch" && stem.ends_with('f');
            let b = std::fs::read(&p).unwrap();
            let (t, used) = tree2::parse(&b, vector).unwrap_or_else(|| panic!("{}: parse failed", p.display()));
            assert_eq!(used, b.len(), "{}: {} of {} bytes", p.display(), used, b.len());
            eprintln!("{}: {} internal nodes, dim {}", p.display(), t.nodes.len(), t.dim);
            n += 1;
        }
    }
    assert_eq!(n, 17);
    let bank = Bank::open(&m16).unwrap();
    assert_eq!(bank.version, "ver.2004");
    assert_eq!(Era::detect(&bank, &m16, Lang::Eng).unwrap(), Era::Paul04);
}
