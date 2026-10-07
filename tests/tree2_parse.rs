//! Every `.tree2` file of the 2009 Paul install parses to its exact length (atmt: all 27 records).

use std::path::{Path, PathBuf};

use vtpaul::tree2;

const ROOT: &str = match option_env!("VTPAUL_VOICES") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};

fn paul09() -> PathBuf {
    Path::new(ROOT).join("old_paul_install").join("Paul")
}

#[test]
fn tree2_files_parse_exactly() {
    let root = paul09();
    if !root.join("data-paul").exists() {
        eprintln!("skipping: no 2009 Paul data under {}", root.display());
        return;
    }
    let tree = root.join("data-paul").join("M16").join("ttsdata").join("tree");
    let mut files: Vec<(PathBuf, bool)> = Vec::new();
    for sub in ["duration", "pitch"] {
        for e in std::fs::read_dir(tree.join(sub)).unwrap() {
            let p = e.unwrap().path();
            let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
            files.push((p, sub == "pitch" && stem.ends_with('f')));
        }
    }
    let dict = root.join("data-common").join("dict-eng");
    for t in ["engbi", "poly", "sbd"] {
        files.push((dict.join(format!("{}.tree2", t)), false));
    }
    for (p, vector) in &files {
        let b = std::fs::read(p).unwrap();
        let (t, used) = tree2::parse(&b, *vector).unwrap_or_else(|| panic!("{}: parse failed", p.display()));
        assert_eq!(used, b.len(), "{}: {} of {} bytes", p.display(), used, b.len());
        eprintln!("{}: {} internal nodes, dim {}, {} leaves", p.display(), t.nodes.len(), t.dim, t.values.len() / t.dim.max(1));
    }
    let b = std::fs::read(dict.join("atmt.tree2")).unwrap();
    let mut p = 0;
    for k in 0..27 {
        let len = u32::from_le_bytes(b[p..p + 4].try_into().unwrap()) as usize;
        let (_, used) = tree2::parse(&b[p + 4..p + 4 + len], false).unwrap_or_else(|| panic!("atmt record {} failed", k));
        assert_eq!(used, len, "atmt record {}", k);
        p += 4 + len;
    }
    assert_eq!(p, b.len(), "atmt trailing bytes");
}
