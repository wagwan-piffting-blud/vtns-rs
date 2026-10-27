//! VTNS: text box, voice controls and a VTML menu over the `vtpaul` library.
//! Synthesis runs in-process on a blocking thread; the webview only edits
//! text and plays the WAV bytes it gets back.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::ipc::Response;
use tauri::{Manager, State};
use vtpaul::engine::Engine;
use vtpaul::engine_spa::EngineSpa;
use vtpaul::era::Era;
use vtpaul::wave::formats;
use vtpaul::{cp1252, SAMPLE_RATE};

#[derive(Serialize, Clone)]
struct VoiceInfo {
    id: String,
    label: String,
    /// "2014", "2009" or "2004": which engine build the data selects.
    era: &'static str,
    lang: &'static str,
}

struct Found {
    info: VoiceInfo,
    root: PathBuf,
    /// The `data-<dir>` folder name the engine opens.
    dir: String,
}

enum Synth {
    Eng(Engine),
    Spa(EngineSpa),
}

struct App {
    /// Where the search may look besides a folder the user picked.
    roots: Vec<PathBuf>,
    voices: Mutex<Vec<Found>>,
    /// Only the voice in use stays loaded.
    loaded: Mutex<Option<(String, Synth)>>,
    last: Mutex<Notes>,
}

#[derive(Deserialize)]
struct Request {
    text: String,
    voice: String,
    pitch: i32,
    rate: i32,
    volume: i32,
    pause: i32,
    comma: i32,
    emphasis: i32,
    x64: bool,
}

#[derive(Serialize, Clone, Default)]
struct Notes {
    seconds: f32,
    /// Characters with no Windows-1252 code, read as "?".
    unencodable: Vec<String>,
}

#[derive(Serialize)]
struct VoiceList {
    voices: Vec<VoiceInfo>,
    /// The folder they were found in.
    root: Option<String>,
}

/// Where voices are looked for when the user has not picked a folder:
/// $VTPAUL_DATA, the folder holding the program, this platform's default
/// folders (`vtpaul::install`), then (in a debug build) the voices next to
/// the repo.
fn default_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(r) = std::env::var("VTPAUL_DATA") {
        if !r.is_empty() {
            roots.push(r.into());
        }
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        roots.push(dir);
    }
    roots.extend(vtpaul::install::default_roots());
    if cfg!(debug_assertions) {
        roots.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../all_voices"));
    }
    roots
}

/// The index version the bank's first pool carries ("ver.2013", ...).
fn bank_version(m16: &Path) -> Option<String> {
    let list = std::fs::read_to_string(m16.join("dblist.idx")).ok()?;
    let first = list.split_whitespace().nth(1)?;
    let mut head = [0u8; 9];
    let mut f = std::fs::File::open(m16.join("mc_idx_tbl").join(first)).ok()?;
    io::Read::read_exact(&mut f, &mut head).ok()?;
    Some(String::from_utf8_lossy(&head[1..]).into_owned())
}

fn voice_of(root: &Path, dir: &str) -> Option<Found> {
    let (name, lang, rank) = match dir.to_ascii_lowercase().as_str() {
        "paul" => ("Paul", "eng", 0),
        "kate" => ("Kate", "eng", 1),
        "juli" | "julie" => ("Julie", "eng", 2),
        "jame" => ("James", "eng", 3),
        "violeta" => ("Violeta", "spa", 4),
        _ => return None,
    };
    let m16 = root.join(format!("data-{dir}")).join("M16");
    let ver = bank_version(&m16)?;
    let era = match ver.as_str() {
        "ver.2013" => "2014",
        "ver.2004" if lang == "eng" => "2004",
        _ => "2009",
    };
    let label = if era == "2014" { name.to_string() } else { format!("{name} ({era})") };
    let id = format!("{rank}|{era}|{}", m16.display());
    Some(Found { info: VoiceInfo { id, label, era, lang }, root: root.to_path_buf(), dir: dir.to_string() })
}

fn subdirs(p: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(p) else { return Vec::new() };
    let mut v: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_dir()).collect();
    v.sort();
    v
}

/// Every voice under `root`: in it and in installs up to two folders down.
/// An install holds data-common and a data-<voice> per voice.
fn scan(root: &Path) -> Vec<Found> {
    let mut installs = Vec::new();
    let mut walk = vec![(root.to_path_buf(), 0)];
    while let Some((p, depth)) = walk.pop() {
        if p.join("data-common").is_dir() {
            installs.push(p.clone());
        }
        if depth < 2 {
            walk.extend(subdirs(&p).into_iter().map(|c| (c, depth + 1)));
        }
    }
    let mut found: Vec<Found> = Vec::new();
    for inst in installs {
        for d in subdirs(&inst) {
            let Some(dir) = d.file_name().and_then(|n| n.to_str()).and_then(|n| n.strip_prefix("data-")) else { continue };
            if let Some(v) = voice_of(&inst, dir) {
                found.push(v);
            }
        }
    }
    // the same voice and build from two installs: tell them apart by folder
    for i in 0..found.len() {
        let twin = found.iter().enumerate().any(|(j, f)| j != i && f.info.label == found[i].info.label);
        if twin {
            let folder = found[i].root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            found[i].info.label = format!("{} — {folder}", found[i].info.label);
        }
    }
    found.sort_by(|a, b| {
        let key = |f: &Found| (f.info.id.split('|').next().unwrap_or("").to_string(), std::cmp::Reverse(f.info.era));
        key(a).cmp(&key(b)).then(a.info.label.cmp(&b.info.label))
    });
    found
}

fn format_code(name: &str) -> Option<i32> {
    Some(match name {
        "wav" => formats::FMT_S16PCM_WAVE,
        "wav8" => formats::FMT_U08PCM_WAVE,
        "alaw-wav" => formats::FMT_ALAW_WAVE,
        "mulaw-wav" => formats::FMT_MULAW_WAVE,
        "au" => formats::FMT_MULAW_AU,
        "pcm" => formats::FMT_S16PCM,
        "alaw" => formats::FMT_ALAW,
        "mulaw" => formats::FMT_MULAW,
        "adpcm" => formats::FMT_DADPCM,
        _ => return None,
    })
}

fn encode(fmt: i32, pcm: &[i16], ends: &[usize]) -> Option<Vec<u8>> {
    let mut data = formats::encode_file(fmt, pcm, ends)?;
    if fmt == formats::FMT_MULAW_AU {
        // The engine's own .au writer leaves only a zeroed annotation where
        // the header belongs; write the header it meant to.
        let mut v = formats::au_header(pcm.len() as u32);
        v.extend_from_slice(&data[4..]);
        data = v;
    }
    Some(data)
}

fn unencodable(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in text.chars() {
        if cp1252::byte(c).is_none() && !out.iter().any(|s| s.starts_with(c)) {
            out.push(c.to_string());
        }
    }
    out
}

impl App {
    /// Renders `r` with its voice, loading that voice first if another one
    /// is loaded.
    fn render(&self, r: &Request) -> Result<(Vec<i16>, Vec<usize>), String> {
        let (root, dir, era) = {
            let voices = self.voices.lock().unwrap();
            let f = voices.iter().find(|f| f.info.id == r.voice).ok_or_else(|| "pick a voice first".to_string())?;
            (f.root.clone(), f.dir.clone(), f.info.era)
        };
        let mut loaded = self.loaded.lock().unwrap();
        if loaded.as_ref().is_none_or(|(id, _)| *id != r.voice) {
            // drop the old one before reading the new
            *loaded = None;
            let open_err = |e: io::Error| format!("cannot open {} in {}: {e}", dir, root.display());
            let s = if dir == "violeta" {
                Synth::Spa(EngineSpa::open(&root).map_err(open_err)?)
            } else {
                Synth::Eng(Engine::open_voice(&root, &dir).map_err(open_err)?)
            };
            *loaded = Some((r.voice.clone(), s));
        }
        let text = cp1252::from_utf8(r.text.as_bytes());
        let spoken = match &mut loaded.as_mut().unwrap().1 {
            Synth::Eng(e) => {
                e.set_x64(r.x64 && e.era == Era::V2014 && era == "2014");
                e.set_params(r.pitch, r.rate, r.volume, r.pause, r.comma, r.emphasis);
                e.speak_traced(&text)
            }
            Synth::Spa(e) => {
                e.set_params(r.pitch, r.rate, r.volume, r.pause, r.comma, r.emphasis);
                e.speak_traced(&text)
            }
        };
        spoken.map_err(|e| e.to_string())
    }

    fn remember(&self, r: &Request, samples: usize) -> Notes {
        let n = Notes { seconds: samples as f32 / SAMPLE_RATE as f32, unencodable: unencodable(&r.text) };
        *self.last.lock().unwrap() = n.clone();
        n
    }
}

/// Find the voices: under `root` if the user picked one (and it holds any),
/// else under the first default place that does.
#[tauri::command]
fn voices(app: State<'_, Arc<App>>, root: Option<String>) -> VoiceList {
    let picked = root.map(PathBuf::from);
    let (found, from) = picked.iter().chain(app.roots.iter())
        .map(|r| (scan(r), r))
        .find(|(v, _)| !v.is_empty())
        .map_or((Vec::new(), None), |(v, r)| (v, Some(r.display().to_string())));
    let list = found.iter().map(|f| f.info.clone()).collect();
    *app.voices.lock().unwrap() = found;
    VoiceList { voices: list, root: from }
}

/// The WAV for `req`, as raw bytes. `notes` has what it could not read.
#[tauri::command]
async fn speak(app: State<'_, Arc<App>>, req: Request) -> Result<Response, String> {
    let app = app.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (pcm, ends) = app.render(&req)?;
        app.remember(&req, pcm.len());
        let wav = encode(formats::FMT_S16PCM_WAVE, &pcm, &ends).ok_or("cannot encode WAV")?;
        Ok(Response::new(wav))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Render `req` into `path` in `format` (the CLI's --format names).
#[tauri::command]
async fn save(app: State<'_, Arc<App>>, req: Request, path: String, format: String) -> Result<Notes, String> {
    let app = app.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let fmt = format_code(&format).ok_or_else(|| format!("unknown format {format}"))?;
        let (pcm, ends) = app.render(&req)?;
        let data = encode(fmt, &pcm, &ends).ok_or_else(|| format!("cannot encode {format}"))?;
        std::fs::write(&path, data).map_err(|e| format!("cannot write {path}: {e}"))?;
        Ok(app.remember(&req, pcm.len()))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn notes(app: State<'_, Arc<App>>) -> Notes {
    app.last.lock().unwrap().clone()
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(Arc::new(App {
                roots: default_roots(),
                voices: Mutex::new(Vec::new()),
                loaded: Mutex::new(None),
                last: Mutex::new(Notes::default()),
            }));

            #[allow(unused_mut)]
            let mut window = tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
                .title("VTNS")
                .inner_size(780.0, 700.0)
                .min_inner_size(580.0, 560.0);
            // Tauri's own WebView2 arguments replace
            // WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS, so a DevTools port for
            // tools/cdp.py has to come through here. The first three are
            // Tauri's defaults.
            #[cfg(windows)]
            if let Ok(port) = std::env::var("VTNS_DEVTOOLS_PORT") {
                window = window.additional_browser_args(&format!(
                    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection \
                     --remote-debugging-port={port}"));
            }
            window.build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![voices, speak, save, notes])
        .run(tauri::generate_context!())
        .expect("error while running VTNS");
}
