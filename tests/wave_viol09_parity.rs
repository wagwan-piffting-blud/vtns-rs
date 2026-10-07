//! Waveform parity for the 2009 Violeta build (vt_spa_violeta16.dll, Dec 2009) against the Frida
//! captures in `out_wave_viol09` (`.json` + `.wav`) and `out_ref_viol09` / `out_old\viol09` (reference
//! `.wav` only). Mirrors `wave_parity.rs`. Input per phrase: the captured
//! 0x34-byte unit records (pre-smoothing, exactly as `wav_BuildUnitRecords`
//! left them, confirmed byte-identical to English's layout by
//! `harness\hooks_wave_spa.js`) and the selected path. Output: PCM, which
//! must equal both the `out_wave_viol09` and the reference (`out_ref_viol09`, `out_old\viol09`) WAV samples.

use std::path::{Path, PathBuf};

use vtpaul::bank::{Bank, Primary};
use vtpaul::json::Json;
use vtpaul::era::Era;
use vtpaul::wave::{self, DecodeWindow, PhraseInput, Record};

const ERA: Era = Era::Violeta09;
/// Centre phone byte of the 5-byte phone context `[L, C, R, pos, pos]`.
const CENTER_IDX: usize = 1;

/// Voice data root; override at build time with `VTPAUL_VOICES=<dir>`.
const VOICES: &str = match option_env!("VTPAUL_VOICES") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};

fn m16() -> PathBuf {
    Path::new(VOICES).join("old_julie_violeta").join("Violeta").join("data-violeta").join("M16")
}
/// Capture root; override at build time with `VTPAUL_HARNESS=<dir>`.
const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};

// Violeta's whole break/pad alphabet is English's shifted -0x14 (confirmed by
// reveng\VIOLETA.md and by us_TargetCost/wav_BuildUnitRecords
// decompile): pad/silence 0x5a -> 0x46, sentence-final-alt 0x5e -> 0x4a,
// comma 0x5b -> 0x47.
const PAD: u8 = 0x46;
const BRK_A: u8 = 0x46;
const BRK_B: u8 = 0x4a;
const BRK_COMMA: u8 = 0x47;

fn i(j: &Json) -> i64 {
    j.i64().unwrap_or(0)
}

fn record_from_json(r: &Json) -> Record {
    let db = r.get("db");
    let per = db.get("per");
    let prim = Primary {
        pcm_pos: i(db.get("pcm_pos")) as u32,
        half1_len: i(db.get("h1")) as u16,
        half2_len: i(db.get("h2")) as u16,
        coded_size: i(db.get("coded")) as u16,
        pm_pos: i(db.get("pm_pos")) as u32,
        pm1: i(db.get("pm1")) as u8,
        pm2: i(db.get("pm2")) as u8,
        periods: [i(per.at(0)) as u8, i(per.at(1)) as u8, i(per.at(2)) as u8],
    };
    Record {
        pitch: i(r.get("pitch")) as i32,
        dur: i(r.get("dur")) as i32,
        vol: i(r.get("vol")) as i32,
        len: i(r.get("len")) as i32,
        unit: i(r.get("unit")) as i16,
        file: i(r.get("file")) as i16,
        pos14: i(r.get("pos14")) as i32,
        db: prim,
        upm: i(r.get("upm")) as u32,
        n_pm: i(r.get("n_pm")) as u8,
        ovl_l: i(r.get("ovl_l")) as i16,
        ovl_r: i(r.get("ovl_r")) as i16,
        mode: i(r.get("mode")) as u8,
        typ: i(r.get("type")) as u8,
        word: i(r.get("word")) as u16,
        w2a: i(r.get("w2a")) as i16,
        phone: i(r.get("phone")) as u16,
        center: i(r.get("center")) as u8,
    }
}

fn read_wav(p: &Path) -> Vec<i16> {
    let b = std::fs::read(p).unwrap();
    let mut pos = 12;
    while pos + 8 <= b.len() {
        let id = &b[pos..pos + 4];
        let sz = u32::from_le_bytes(b[pos + 4..pos + 8].try_into().unwrap()) as usize;
        if id == b"data" {
            return b[pos + 8..(pos + 8 + sz).min(b.len())]
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                .collect();
        }
        pos += 8 + sz + (sz & 1);
    }
    panic!("no data chunk in {}", p.display());
}

fn primary_from_json(db: &Json) -> (Primary, u8) {
    let per = db.get("per");
    (
        Primary {
            pcm_pos: i(db.get("pcm_pos")) as u32,
            half1_len: i(db.get("h1")) as u16,
            half2_len: i(db.get("h2")) as u16,
            coded_size: i(db.get("coded")) as u16,
            pm_pos: i(db.get("pm_pos")) as u32,
            pm1: i(db.get("pm1")) as u8,
            pm2: i(db.get("pm2")) as u8,
            periods: [i(per.at(0)) as u8, i(per.at(1)) as u8, i(per.at(2)) as u8],
        },
        i(db.get("file")) as u8,
    )
}

/// Rebuild one phrase's records with `wave::build_records_lang` from the
/// selection and sentence data. `None` when the capture lacks `inp`
/// (`hooks_spa.js`-only captures, no `wav_BuildUnitRecords` input snapshot).
fn rebuild_records(j: &Json, rp: &Json, sel: &Json, state: &mut Option<(i64, wave::PauseState)>) -> Option<Vec<Record>> {
    let inp = rp.get("inp");
    if inp.is_null() {
        return None;
    }
    let s = i(rp.get("sentence"));
    let sent = &j.get("sentences").arr()[s as usize];
    let words: Vec<wave::WordInfo> = sent
        .get("words")
        .arr()
        .iter()
        .map(|w| wave::WordInfo {
            brk: i(w.get("brk")) as u8,
            pitch: i(w.get("pitch_pct")) as i32,
            speed: i(w.get("speed_pct")) as i32,
            volume: i(w.get("volume_pct")) as i32,
        })
        .collect();
    if state.as_ref().map(|x| x.0) != Some(s) {
        *state = Some((
            s,
            wave::PauseState {
                word_pauses: inp.get("word_pauses").arr().iter().map(|v| i(v) as i32).collect(),
                cont_flag: i(inp.get("cont_flag")) != 0,
                final_pause_ms: i(inp.get("final_pause")) as i32,
                ch_pitch: i(inp.get("ch_pitch")) as i32,
                ch_volume: i(inp.get("ch_volume")) as i32,
                ch_comma_ms: i(inp.get("ch_comma")) as i32,
                word0_start: 0, // no character-span capture for Violeta; unused by PCM generation
                span_reset: None,
            },
        ));
    }
    let st = &mut state.as_mut().unwrap().1;
    assert_eq!(st.cont_flag, i(inp.get("cont_flag")) != 0, "cont_flag carry");
    let targets: Vec<wave::Target> = sel
        .get("targets")
        .arr()
        .iter()
        .map(|t| {
            let (db, file) = primary_from_json(t.get("primary"));
            wave::Target { word: i(t.get("word")) as u8, phone: i(t.get("phone")) as u8, half: i(t.get("half")) as u8, db, file }
        })
        .collect();
    let sw = sent.get("words").arr();
    let center = |w: usize, k: usize| -> u8 {
        sw.get(w).map(|x| x.get("phones").at(k).get("ctx").hex().get(CENTER_IDX).copied().unwrap_or(0)).unwrap_or(0)
    };
    let last = i(inp.get("phrase_index")) == i(inp.get("n_phrases")) - 1;
    Some(wave::build_records_lang(
        &targets,
        &words,
        &center,
        i(rp.get("phrase")) as u16,
        last,
        st,
        PAD,
        BRK_A,
        BRK_B,
        BRK_COMMA,
    ))
}

struct Outcome {
    name: String,
    samples: usize,
    ref_samples: usize,
    mismatches: usize,
    first: Option<String>,
    fill_mismatch: usize,
    /// Set if the same-named reference WAV differs from what we produced,
    /// even though the co-located `out_wave_viol09` WAV matched (or vice versa).
    cross_check: Option<String>,
    /// None when the capture has no `chunks` or the modelled chunking matches.
    chunk_mismatch: Option<String>,
}

fn run_capture(bank: &Bank, json_path: &Path, rebuild: bool, violeta_dir: &Path) -> Outcome {
    let j = Json::load(json_path).unwrap();
    let wav = read_wav(&json_path.with_extension("wav"));
    let mut win = DecodeWindow::default();
    let mut out: Vec<i16> = Vec::new();
    let mut spans: Vec<(usize, usize, String)> = Vec::new();
    let mut fill_mismatch = 0;
    let mut pstate: Option<(i64, wave::PauseState)> = None;
    let mut all_trace: Vec<wave::RecTrace> = Vec::new();
    let sels = j.get("selections").arr();
    assert_eq!(sels.len(), j.get("records").arr().len(), "{}", json_path.display());
    for (rp, sel) in j.get("records").arr().iter().zip(sels) {
        let (s, p) = (i(rp.get("sentence")), i(sel.get("phrase")));
        assert_eq!(s, i(sel.get("sentence")));
        let targets = sel.get("targets").arr();
        let uids: Vec<u32> = targets.iter().map(|t| i(t.get("uid")) as u32).collect();
        let halves: Vec<u8> = targets.iter().map(|t| i(t.get("half")) as u8).collect();
        let recs: Vec<Record> = if rebuild {
            rebuild_records(&j, rp, sel, &mut pstate).expect("capture lacks inp")
        } else {
            rp.get("records").arr().iter().map(record_from_json).collect()
        };
        for r in &recs {
            if r.is_silence() {
                continue;
            }
            let mut f = Record { mode: r.mode, ..Default::default() };
            f.fill(&r.db, r.file as u8);
            if (f.len, f.ovl_l, f.ovl_r, f.upm, f.n_pm, f.pos14) != (r.len, r.ovl_l, r.ovl_r, r.upm, r.n_pm, r.pos14) {
                fill_mismatch += 1;
            }
        }
        let input = PhraseInput { records: &recs, uids: &uids, halves: &halves };
        let mut rt = Vec::new();
        wave::generate_phrase_traced_era(&mut win, bank, &input, &mut out, Some(&mut rt), ERA).unwrap();
        let mut tr: Vec<usize> = rt.iter().map(|r| r.start).collect();
        all_trace.extend(rt);
        tr.push(out.len());
        for k in 0..recs.len() {
            let r = &recs[k];
            spans.push((tr[k], tr[k + 1], format!("s{} p{} rec{} unit t{} mode{} len{}", s, p, k, r.unit, r.mode, r.len)));
        }
    }
    let mut chunk_mismatch = None;
    if !j.get("chunks").is_null() {
        let ends = wave::chunk_ends(&all_trace, out.len());
        let ours: Vec<i64> = ends
            .iter()
            .scan(0usize, |p, &e| {
                let b = ((e - *p) * 2) as i64;
                *p = e;
                Some(b)
            })
            .collect();
        let want: Vec<i64> = j.get("chunks").arr().iter().map(i).filter(|&b| b > 0).collect();
        if ours != want {
            chunk_mismatch = Some(format!("chunks ours {:?} want {:?}", ours, want));
        }
    }
    let k = wave::formats::clamp_emphasis(i(j.get("params").at(5)) as i32);
    let mut st = 0i16;
    wave::formats::emphasis(&mut out, k, &mut st);
    let n = out.len().min(wav.len());
    let mut mism = 0;
    let mut first = None;
    for k in 0..n {
        if out[k] != wav[k] {
            if first.is_none() {
                let sp = spans.iter().find(|x| x.0 <= k && k < x.1).map(|x| x.2.clone()).unwrap_or_default();
                first = Some(format!("sample {} ({}) ours={} ref={}", k, sp, out[k], wav[k]));
            }
            mism += 1;
        }
    }
    let name = json_path.file_stem().unwrap().to_string_lossy().into_owned();
    let mismatches = mism + out.len().abs_diff(wav.len());
    let mut cross_check = None;
    let vpath = [violeta_dir.join(format!("{}.wav", name)), Path::new(HARNESS).join("out_old").join("viol09").join(format!("{}.wav", name))]
        .into_iter()
        .find(|p| p.exists());
    if let Some(vpath) = vpath {
        let vwav = read_wav(&vpath);
        let nv = out.len().min(vwav.len());
        let bad = out.len() != vwav.len() || (0..nv).any(|k| out[k] != vwav[k]);
        if bad != (mismatches != 0) {
            cross_check = Some(format!("out_wave_viol09 wav {} but reference wav {}", if mismatches == 0 { "matches" } else { "differs" }, if bad { "differs" } else { "matches" }));
        }
    }
    Outcome { name, samples: out.len(), ref_samples: wav.len(), mismatches, first, fill_mismatch, cross_check, chunk_mismatch }
}

fn captures(dir: &str) -> Vec<PathBuf> {
    let d = Path::new(HARNESS).join(dir);
    let mut v: Vec<PathBuf> = match std::fs::read_dir(&d) {
        Ok(rd) => rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.extension().is_some_and(|x| x == "json")
                    && !p.file_name().unwrap().to_string_lossy().starts_with('_')
                    && p.with_extension("wav").exists()
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    v.sort();
    v
}

fn report(dir: &str, rebuild: bool) -> (usize, usize) {
    let paths = captures(dir);
    let violeta_dir = Path::new(HARNESS).join("out_ref_viol09");
    if paths.is_empty() || !m16().exists() {
        eprintln!("skipping {}: no captures or no Violeta voice", dir);
        return (0, 0);
    }
    let bank = Bank::open(&m16()).unwrap();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results = std::sync::Mutex::new(Vec::new());
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| loop {
                let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if k >= paths.len() {
                    break;
                }
                let o = run_capture(&bank, &paths[k], rebuild, &violeta_dir);
                results.lock().unwrap().push((k, o));
            });
        }
    });
    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|r| r.0);
    let mut exact = 0;
    for (_, o) in results {
        let ok = o.mismatches == 0 && o.cross_check.is_none() && o.chunk_mismatch.is_none();
        exact += ok as usize;
        if !ok {
            println!(
                "{:28} BAD samples {:7}/{:7} mism {:7} fill {:2} {} {} {}",
                o.name,
                o.samples,
                o.ref_samples,
                o.mismatches,
                o.fill_mismatch,
                o.first.unwrap_or_default(),
                o.cross_check.unwrap_or_default(),
                o.chunk_mismatch.unwrap_or_default()
            );
        }
    }
    println!("{}: {}/{} bit-exact", dir, exact, paths.len());
    (exact, paths.len())
}

/// As `wave_parity.rs`'s `same_record`, but for silence (`typ == 1`) records
/// also ignoring `mode` along with the DB-derived fields. Violeta's captures
/// show a silence record's `mode` carrying whatever was left in that record
/// slot by an earlier unit record (observed values of 0 or 1 for otherwise
/// byte-identical pauses re-used verbatim across many sentences in the same
/// capture) -- `generate_phrase`/`ola_unmodified` never read `mode` for a
/// silence record (only `is_silence()` and `len` matter), so this is the same
/// "don't-care" leftover-buffer pattern `wave.rs` already documents for
/// silence's DB fields, just extended to one more field for this build.
fn same_record(a: &Record, b: &Record) -> bool {
    if a.is_silence() != b.is_silence() {
        return false;
    }
    if a.is_silence() {
        let key = |r: &Record| (r.pitch, r.dur, r.vol, r.len, r.unit, r.typ, r.word, r.phone, r.center);
        return key(a) == key(b);
    }
    let key = |r: &Record| (r.pitch, r.dur, r.vol, r.len, r.unit, r.typ, r.word, r.phone, r.center, r.mode);
    key(a) == key(b) && (a.file, a.pos14, a.upm, a.n_pm, a.ovl_l, a.ovl_r) == (b.file, b.pos14, b.upm, b.n_pm, b.ovl_l, b.ovl_r)
}

/// PCM (post-emphasis) and record trace for a capture, from its captured records.
fn synth_capture(bank: &Bank, j: &Json) -> (Vec<i16>, Vec<wave::RecTrace>) {
    let mut win = DecodeWindow::default();
    let mut out = Vec::new();
    let mut trace = Vec::new();
    for (rp, sel) in j.get("records").arr().iter().zip(j.get("selections").arr()) {
        let targets = sel.get("targets").arr();
        let uids: Vec<u32> = targets.iter().map(|t| i(t.get("uid")) as u32).collect();
        let halves: Vec<u8> = targets.iter().map(|t| i(t.get("half")) as u8).collect();
        let recs: Vec<Record> = rp.get("records").arr().iter().map(record_from_json).collect();
        let input = PhraseInput { records: &recs, uids: &uids, halves: &halves };
        wave::generate_phrase_traced_era(&mut win, bank, &input, &mut out, Some(&mut trace), ERA).unwrap();
    }
    let k = wave::formats::clamp_emphasis(i(j.get("params").at(5)) as i32);
    let mut st = 0i16;
    wave::formats::emphasis(&mut out, k, &mut st);
    (out, trace)
}

/// Every VT_TextToFile_SPA / VT_TextToBuffer_SPA format, against the files the engine wrote for the
/// same text and parameters (`harness\run_vtfmt_old.py` -> `out_fmt_viol09`).
#[test]
fn formats_parity_viol09() {
    let fmt_dir = Path::new(HARNESS).join("out_fmt_viol09");
    let paths = captures("out_wave_viol09");
    if paths.is_empty() || !fmt_dir.exists() || !m16().exists() {
        eprintln!("skipping: need out_wave_viol09 captures, out_fmt_viol09 files and the voice");
        return;
    }
    let bank = Bank::open(&m16()).unwrap();
    let file_fmts = [0, 1, 2, 3, 4, 5, 7, 8, 9];
    let ok = std::sync::Mutex::new(std::collections::BTreeMap::<String, (usize, usize)>::new());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| loop {
                let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if k >= paths.len() {
                    break;
                }
                let j = Json::load(&paths[k]).unwrap();
                let name = j.get("name").str().unwrap().to_string();
                let (pcm, trace) = synth_capture(&bank, &j);
                let ends = wave::chunk_ends(&trace, pcm.len());
                let mut res = Vec::new();
                for &f in &file_fmts {
                    let ours = wave::formats::encode_file(f, &pcm, &ends).unwrap();
                    let want = std::fs::read(fmt_dir.join(format!("{}.file{}", name, f))).unwrap_or_default();
                    res.push((format!("file{}", f), ours == want));
                }
                for f in 0..=3 {
                    let ours = wave::formats::encode_file(f, &pcm, &ends).unwrap();
                    let want = std::fs::read(fmt_dir.join(format!("{}.buf{}", name, f))).unwrap_or_default();
                    res.push((format!("buf{}", f), ours == want));
                }
                let mut ok = ok.lock().unwrap();
                for (label, good) in res {
                    if !good {
                        println!("{} {}: differs", name, label);
                    }
                    let e = ok.entry(label).or_insert((0, 0));
                    e.0 += good as usize;
                    e.1 += 1;
                }
            });
        }
    });
    let mut all = true;
    for (label, (good, n)) in ok.into_inner().unwrap() {
        println!("{:6} {}/{} byte-exact", label, good, n);
        all &= good == n;
    }
    assert!(all);
}

/// `wav_BuildUnitRecords` from the selected path + sentence data, compared
/// with the captured records (`out_wave_viol09`, which carries `inp`).
#[test]
fn records_parity_viol09() {
    let paths = captures("out_wave_viol09");
    if paths.is_empty() {
        eprintln!("skipping: no out_wave_viol09 captures");
        return;
    }
    let (mut phrases, mut exact, mut rec_total, mut rec_bad) = (0, 0, 0, 0);
    for p in &paths {
        let j = Json::load(p).unwrap();
        let sels = j.get("selections").arr();
        let mut state: Option<(i64, wave::PauseState)> = None;
        for (rp, sel) in j.get("records").arr().iter().zip(sels) {
            let s = i(rp.get("sentence"));
            let built = match rebuild_records(&j, rp, sel, &mut state) {
                Some(b) => b,
                None => continue,
            };
            let want: Vec<Record> = rp.get("records").arr().iter().map(record_from_json).collect();
            phrases += 1;
            rec_total += want.len();
            let bad = built.len() != want.len() || built.iter().zip(&want).any(|(a, b)| !same_record(a, b));
            if bad {
                let nbad = built.iter().zip(&want).filter(|(a, b)| !same_record(a, b)).count() + built.len().abs_diff(want.len());
                rec_bad += nbad;
                let first = built.iter().zip(&want).position(|(a, b)| !same_record(a, b));
                println!(
                    "{} s{} phrase@{}: built {} want {} first diff {:?}\n  ours {:?}\n  want {:?}",
                    p.file_stem().unwrap().to_string_lossy(),
                    s,
                    i(rp.get("phrase")),
                    built.len(),
                    want.len(),
                    first,
                    first.map(|k| &built[k]),
                    first.map(|k| &want[k])
                );
            } else {
                exact += 1;
            }
        }
    }
    println!("records (viol09): {}/{} phrases exact, {} of {} records differ", exact, phrases, rec_bad, rec_total);
    assert_eq!(exact, phrases);
}

/// Final PCM against the captured records, for every `out_wave_viol09` capture
/// (the base corpus plus the 50 speed/pitch/volume/emphasis/pause sweeps),
/// cross-checked against `out_violeta`'s reference WAVs where present.
#[test]
fn wave_parity_viol09_params() {
    let (exact, n) = report("out_wave_viol09", false);
    assert_eq!(exact, n);
}

/// Selection output -> build_records_lang -> generate -> emphasis, with no
/// captured records (rebuilds `wav_BuildUnitRecords`'s input from scratch).
#[test]
fn wave_parity_viol09_from_selection() {
    let (exact, n) = report("out_wave_viol09", true);
    assert_eq!(exact, n);
}
