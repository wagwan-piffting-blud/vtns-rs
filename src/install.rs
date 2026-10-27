//! Where voice data is looked for when no folder is given.

use std::path::PathBuf;

/// This platform's default voice-data folders, most specific first. Each is
/// a data root (it holds `data-common` and a `data-<voice>` per voice) or,
/// for the GUI's search, a folder of them. Windows has the setup programs'
/// install folder; Linux and macOS have no installer, so these are the usual
/// places to put shared data by hand.
pub fn default_roots() -> Vec<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let mut roots: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        // 32-bit Windows has no ProgramFiles(x86)
        for k in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(p) = env(k) {
                roots.push(p.join("NWSPaulNumber2"));
            }
        }
    } else if cfg!(target_os = "macos") {
        if let Some(h) = env("HOME") {
            roots.push(h.join("Library/Application Support/vtpaul"));
        }
        roots.push("/Library/Application Support/vtpaul".into());
    } else {
        match (env("XDG_DATA_HOME"), env("HOME")) {
            (Some(x), _) => roots.push(x.join("vtpaul")),
            (None, Some(h)) => roots.push(h.join(".local/share/vtpaul")),
            (None, None) => {}
        }
        roots.push("/usr/local/share/vtpaul".into());
        roots.push("/usr/share/vtpaul".into());
    }
    roots.dedup();
    roots
}
