//! Waveform parity against the Frida captures (`out*/*.json` + `.wav`).
//!
//! Input per phrase: the captured 0x34-byte unit records (pre-smoothing, exactly
//! as `wav_BuildUnitRecords` left them) and the selected path. Output: PCM, which
//! must equal the engine's WAV sample for sample.

use std::path::{Path, PathBuf};

use vtpaul::bank::{Bank, Primary};
use vtpaul::json::Json;
use vtpaul::era::Era;
use vtpaul::wave::{self, DecodeWindow, PhraseInput, Record};

const ERA: Era = Era::Paul09;
/// Centre phone byte of the 5-byte phone context `[L1, C, R1, a, b]`.
const CENTER_IDX: usize = 1;

const VOICES: &str = match option_env!("VTPAUL_VOICES") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};

fn m16() -> PathBuf {
    Path::new(VOICES).join("old_paul_install").join("Paul").join("data-paul").join("M16")
}
/// Capture root; override at build time with `VTPAUL_HARNESS=<dir>`.
const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};

fn i(j: &Json) -> i64 {
    j.i64().unwrap_or(0)
}

pub fn record_from_json(r: &Json) -> Record {
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

pub fn read_wav(p: &Path) -> Vec<i16> {
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

pub struct Outcome {
    pub name: String,
    pub samples: usize,
    pub ref_samples: usize,
    pub mismatches: usize,
    pub first: Option<String>,
    pub fill_mismatch: usize,
    /// None when the capture has no `chunks` or the modelled chunking matches.
    pub chunk_mismatch: Option<String>,
}

pub fn run_capture(bank: &Bank, json_path: &Path, rebuild: bool) -> Outcome {
    let j = Json::load(json_path).unwrap();
    let wav = read_wav(&json_path.with_extension("wav"));
    let mut win = DecodeWindow::default();
    let mut out: Vec<i16> = Vec::new();
    let mut spans: Vec<(usize, usize, String)> = Vec::new();
    let mut fill_mismatch = 0;
    let mut pstate: Option<(i64, wave::PauseState)> = None;
    let mut all_trace: Vec<wave::RecTrace> = Vec::new();
    // records[].phrase is wav_BuildUnitRecords arg2, the phrase's first word
    // index; records and selections are paired by order instead.
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
            let desc = format!(
                "s{} p{} rec{} {} t{} mode{} len{} ovl{}/{}",
                s,
                p,
                k,
                if r.is_silence() { "SIL" } else { "unit" },
                r.unit,
                r.mode,
                r.len,
                r.ovl_l,
                r.ovl_r
            );
            spans.push((tr[k], tr[k + 1], desc));
        }
    }
    let mut chunk_mismatch = None;
    if !j.get("chunks").is_null() {
        let ends = wave::chunk_ends(&all_trace, out.len());
        let ours: Vec<i64> = ends.iter().scan(0usize, |p, &e| {
            let b = ((e - *p) * 2) as i64;
            *p = e;
            Some(b)
        }).collect();
        let want: Vec<i64> = j.get("chunks").arr().iter().map(i).filter(|&b| b > 0).collect();
        if ours != want {
            chunk_mismatch = Some(format!("chunks ours {:?} want {:?}", ours, want));
        }
    }
    let k = wave::formats::clamp_emphasis(i(j.get("params").at(5)) as i32);
    let mut st = 0i16;
    wave::formats::emphasis(&mut out, k, &mut st);
    if let Ok(dir) = std::env::var("VTP_DUMP") {
        let mut b = Vec::new();
        for s in &out {
            b.extend_from_slice(&s.to_le_bytes());
        }
        let name = json_path.file_stem().unwrap().to_string_lossy().into_owned();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(Path::new(&dir).join(format!("{}.pcm", name)), b).unwrap();
        let spans_txt: String = spans.iter().map(|s| format!("{}\t{}\t{}\n", s.0, s.1, s.2)).collect();
        std::fs::write(Path::new(&dir).join(format!("{}.spans", name)), spans_txt).unwrap();
    }
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
    Outcome {
        name: json_path.file_stem().unwrap().to_string_lossy().into_owned(),
        samples: out.len(),
        ref_samples: wav.len(),
        mismatches: mism + out.len().abs_diff(wav.len()),
        first,
        fill_mismatch,
        chunk_mismatch,
    }
}

pub fn captures(dir: &str) -> Vec<PathBuf> {
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
    if paths.is_empty() || !m16().as_path().exists() {
        eprintln!("skipping {}: no captures or no voice", dir);
        return (0, 0);
    }
    let bank = Bank::open(m16().as_path()).unwrap();
    let mut exact = 0;
    for p in &paths {
        let o = run_capture(&bank, p, rebuild);
        let ok = o.mismatches == 0 && o.chunk_mismatch.is_none();
        exact += ok as usize;
        println!(
            "{:28} {} samples {:7}/{:7} mism {:7} fill {:2} {} {}",
            o.name,
            if ok { "OK " } else { "BAD" },
            o.samples,
            o.ref_samples,
            o.mismatches,
            o.fill_mismatch,
            o.first.unwrap_or_default(),
            o.chunk_mismatch.unwrap_or_default()
        );
    }
    println!("{}: {}/{} bit-exact", dir, exact, paths.len());
    (exact, paths.len())
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

fn same_record(a: &Record, b: &Record) -> bool {
    // The struct-of-arrays block is never cleared: a silence record keeps the stale mode of
    // whatever the slot held before (paul09 `FUN_1002a190` writes no mode for it).
    let key = |r: &Record| (r.pitch, r.dur, r.vol, r.len, r.unit, r.typ, r.word, r.phone, r.center);
    if key(a) != key(b) {
        return false;
    }
    if a.is_silence() {
        return true;
    }
    if a.mode != b.mode {
        return false;
    }
    (a.file, a.pos14, a.upm, a.n_pm, a.ovl_l, a.ovl_r) == (b.file, b.pos14, b.upm, b.n_pm, b.ovl_l, b.ovl_r)
}

/// Rebuild one phrase's records with `wave::build_records` from the selection and
/// sentence data. `None` when the capture predates hooks_wave.js (no `inp`).
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
            brk: w.get("brk").str().unwrap_or("]").bytes().next().unwrap_or(b']'),
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
                word0_start: i(sent.get("words").at(0).get("start")) as i32,
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
    Some(wave::build_records(&targets, &words, &center, i(rp.get("phrase")) as u16, last, st))
}

/// `wav_BuildUnitRecords` from the selected path + sentence data, compared with
/// the captured records (only captures made with hooks_wave.js carry `inp`).
#[test]
fn records_parity() {
    let paths = captures("out_wave_paul09");
    if paths.is_empty() {
        eprintln!("skipping: no out_wave captures");
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
                let nbad = built.iter().zip(&want).filter(|(a, b)| !same_record(a, b)).count()
                    + built.len().abs_diff(want.len());
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
    println!("records: {}/{} phrases exact, {} of {} records differ", exact, phrases, rec_bad, rec_total);
    assert_eq!(exact, phrases);
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

/// Every VT_TextToFile_ENG / VT_TextToBuffer_ENG format, against files the engine
/// wrote for the same text and parameters (captured to `out_fmt/`).
#[test]
fn formats_parity() {
    let fmt_dir = Path::new(HARNESS).join("out_fmt_paul09");
    let paths = captures("out_wave_paul09");
    if paths.is_empty() || !fmt_dir.exists() || !m16().as_path().exists() {
        eprintln!("skipping: need out_wave captures, out_fmt files and the voice");
        return;
    }
    let bank = Bank::open(m16().as_path()).unwrap();
    let file_fmts = [0, 1, 2, 3, 4, 5, 7, 8, 9];
    let mut ok = std::collections::BTreeMap::<String, (usize, usize)>::new();
    for p in &paths {
        let j = Json::load(p).unwrap();
        let name = j.get("name").str().unwrap().to_string();
        let (pcm, trace) = synth_capture(&bank, &j);
        let ends = wave::chunk_ends(&trace, pcm.len());
        let mut check = |label: String, ours: Vec<u8>, path: PathBuf| {
            let want = std::fs::read(&path).unwrap_or_default();
            let e = ok.entry(label.clone()).or_insert((0, 0));
            e.1 += 1;
            if ours == want {
                e.0 += 1;
            } else {
                let first = ours.iter().zip(&want).position(|(a, b)| a != b);
                println!("{} {}: len {} vs {}, first diff at {:?}", name, label, ours.len(), want.len(), first);
            }
        };
        for &f in &file_fmts {
            let ours = wave::formats::encode_file(f, &pcm, &ends).unwrap();
            check(format!("file{}", f), ours, fmt_dir.join(format!("{}.file{}", name, f)));
        }
        for f in 0..=3 {
            let ours = wave::formats::encode_file(f, &pcm, &ends).unwrap();
            check(format!("buf{}", f), ours, fmt_dir.join(format!("{}.buf{}", name, f)));
        }
    }
    let mut all = true;
    for (label, (good, n)) in &ok {
        println!("{:6} {}/{} byte-exact", label, good, n);
        all &= good == n;
    }
    assert!(all);
}

/// Windows-1252 bytes, as the capture tooling hands the text to the engine.
fn cp1252(s: &str) -> Vec<u8> {
    const HI: [char; 32] = [
        '\u{20ac}', '\u{81}', '\u{201a}', '\u{192}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{2c6}',
        '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8d}', '\u{17d}', '\u{8f}', '\u{90}', '\u{2018}',
        '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}', '\u{2dc}', '\u{2122}', '\u{161}',
        '\u{203a}', '\u{153}', '\u{9d}', '\u{17e}', '\u{178}',
    ];
    s.chars()
        .map(|c| {
            if (c as u32) < 0x80 || ((c as u32) >= 0xa0 && (c as u32) <= 0xff) {
                c as u32 as u8
            } else {
                HI.iter().position(|&h| h == c).map(|p| 0x80 + p as u8).unwrap_or(b'?')
            }
        })
        .collect()
}

/// VT_TextToLipSyncLog_ENG output, rebuilt from the selection (`out_fmt/*.sync`).
#[test]
fn lipsync_parity() {
    let fmt_dir = Path::new(HARNESS).join("out_fmt_paul09");
    let paths = captures("out_wave_paul09");
    if paths.is_empty() || !fmt_dir.exists() || !m16().as_path().exists() {
        eprintln!("skipping: need out_wave captures, out_fmt logs and the voice");
        return;
    }
    let bank = Bank::open(m16().as_path()).unwrap();
    let (mut good, mut n) = (0, 0);
    for p in &paths {
        let j = Json::load(p).unwrap();
        let name = j.get("name").str().unwrap().to_string();
        let want = match std::fs::read(fmt_dir.join(format!("{}.sync", name))) {
            Ok(w) => w,
            Err(_) => continue,
        };
        let mut win = DecodeWindow::default();
        let mut out = Vec::new();
        let mut pstate: Option<(i64, wave::PauseState)> = None;
        let mut sync = wave::SyncState::default();
        let mut words: Vec<wave::SyncWord> = Vec::new();
        for (rp, sel) in j.get("records").arr().iter().zip(j.get("selections").arr()) {
            let recs = rebuild_records(&j, rp, sel, &mut pstate).expect("inp");
            if let Some(s) = pstate.as_mut().unwrap().1.span_reset.take() {
                sync.last_span = s;
            }
            let targets = sel.get("targets").arr();
            let uids: Vec<u32> = targets.iter().map(|t| i(t.get("uid")) as u32).collect();
            let halves: Vec<u8> = targets.iter().map(|t| i(t.get("half")) as u8).collect();
            let input = PhraseInput { records: &recs, uids: &uids, halves: &halves };
            let mut rt = Vec::new();
            wave::generate_phrase_traced_era(&mut win, &bank, &input, &mut out, Some(&mut rt), ERA).unwrap();
            let produced: Vec<i32> = (0..rt.len())
                .map(|k| (rt.get(k + 1).map(|x| x.start).unwrap_or(out.len()) - rt[k].start) as i32)
                .collect();
            let sent = &j.get("sentences").arr()[i(rp.get("sentence")) as usize];
            let spans: Vec<(i32, i32)> = sent
                .get("words")
                .arr()
                .iter()
                .map(|w| (i(w.get("start")) as i32, i(w.get("end")) as i32))
                .collect();
            wave::sync_phrase(&recs, &spans, &produced, &mut sync, &mut words);
        }
        let ours = wave::lipsync_log(&words, &cp1252(j.get("text").str().unwrap()));
        n += 1;
        if ours == want {
            good += 1;
        } else {
            let first = ours.iter().zip(&want).position(|(a, b)| a != b).unwrap_or(ours.len().min(want.len()));
            let ctx = |v: &[u8]| String::from_utf8_lossy(&v[first.saturating_sub(80)..(first + 60).min(v.len())]).into_owned();
            println!("{}: differs at byte {}\n--- ours\n{}\n--- engine\n{}", name, first, ctx(&ours), ctx(&want));
        }
    }
    println!("lipsync: {}/{} byte-exact", good, n);
    assert_eq!(good, n);
}

#[test]
fn wave_parity_default() {
    let (exact, n) = report("out_paul09", false);
    assert_eq!(exact, n);
}

#[test]
fn wave_parity_params() {
    let (exact, n) = report("out_wave_paul09", false);
    assert_eq!(exact, n);
}

/// Selection output -> build_records -> generate -> emphasis, with no captured records.
#[test]
fn wave_parity_from_selection() {
    let (exact, n) = report("out_wave_paul09", true);
    assert_eq!(exact, n);
}
