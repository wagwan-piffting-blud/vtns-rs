//! Waveform parity of the 2004 Paul build against `out_wave_paul04/*.json` + `.wav` (hooks_wave_paul04.js).
//!
//! Input per phrase: the unit-record arrays exactly as `FUN_10020640` left them, plus each speech record's
//! 0x14-byte unit entry. Output: PCM for the whole request, which must equal the engine's WAV sample for sample.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use vtpaul::bank::Bank;
use vtpaul::json::Json;
use vtpaul::engine04::{synth_request04, Sentence04};
use vtpaul::wave::{DecodeWindow, Record, TYPE_SILENCE, TYPE_UNIT};
use vtpaul::wave04::{build_records04, chunk_sizes04, generate_phrase04, primary_from_entry, Channel04, ChunkRec, PhraseInput04, Target04, Word04};

const VOICES: &str = match option_env!("VTPAUL_VOICES") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};
const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};
const OUT_WAVE: &str = "out_wave_paul04";

fn m16() -> PathBuf {
    Path::new(VOICES).join("2004_paul").join("data-paul").join("M16")
}

fn i(j: &Json) -> i64 {
    j.i64().unwrap_or(0)
}

fn read_wav(p: &Path) -> Vec<i16> {
    let b = std::fs::read(p).unwrap();
    b[44..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect()
}

/// (record, file index of the unit entry) from one captured record.
fn record_from_json(r: &Json) -> (Record, u8) {
    let typ = i(r.get("type")) as u8;
    let mut rec = Record {
        pitch: i(r.get("pitch")) as i32,
        dur: i(r.get("dur")) as i32,
        vol: i(r.get("vol")) as i32,
        len: i(r.get("len")) as i32,
        unit: -1,
        typ,
        word: i(r.get("word")) as u16,
        w2a: i(r.get("w2a")) as i16,
        phone: i(r.get("phone")) as u16,
        center: i(r.get("center")) as u8,
        ..Default::default()
    };
    let mut file = 0;
    if typ == TYPE_UNIT {
        let (db, f) = primary_from_entry(&r.get("db").hex());
        file = f;
        rec.db = db;
        rec.file = i(r.get("file")) as i16;
        rec.pos14 = i(r.get("pos14")) as i32;
        rec.upm = i(r.get("upm")) as u32;
        rec.n_pm = i(r.get("n_pm")) as u8;
        rec.ovl_l = i(r.get("ovl_l")) as i16;
        rec.ovl_r = i(r.get("ovl_r")) as i16;
        rec.mode = i(r.get("mode")) as u8;
    }
    (rec, file)
}

fn captures() -> Vec<PathBuf> {
    let d = Path::new(HARNESS).join(OUT_WAVE);
    let mut v: Vec<PathBuf> = match std::fs::read_dir(&d) {
        Ok(rd) => rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json") && p.with_extension("wav").exists())
            .collect(),
        Err(_) => Vec::new(),
    };
    v.sort();
    v
}

/// (file index, pcm_pos) -> global unit, over the whole bank.
fn unit_map(bank: &Bank) -> HashMap<(u8, u32), u32> {
    let mut m = HashMap::with_capacity(bank.len());
    for u in 0..bank.len() as u32 {
        let p = bank.primary(u).unwrap();
        m.entry((bank.file_idx[u as usize], p.pcm_pos)).or_insert(u);
    }
    m
}

struct Outcome {
    name: String,
    samples: usize,
    ref_samples: usize,
    mismatches: usize,
    first: Option<String>,
    fill_mismatch: usize,
    unmapped: usize,
    chunk_mismatch: Option<String>,
}

fn run_capture(bank: &Bank, units: &HashMap<(u8, u32), u32>, path: &Path) -> Outcome {
    let j = Json::load(path).unwrap();
    let wav = read_wav(&path.with_extension("wav"));
    let mut win = DecodeWindow::default();
    let mut out: Vec<i16> = Vec::new();
    let mut spans: Vec<(usize, String)> = Vec::new();
    let (mut fill_mismatch, mut unmapped) = (0, 0);
    let mut chunk_phrases: Vec<Vec<ChunkRec>> = Vec::new();
    for g in j.get("records").arr() {
        let mut recs = Vec::new();
        let mut uids = Vec::new();
        for r in g.get("records").arr() {
            let (rec, file) = record_from_json(r);
            let mut uid = 0;
            if rec.typ == TYPE_UNIT {
                let mut f = Record { mode: rec.mode, ..Default::default() };
                f.fill(&rec.db, file);
                if (f.len, f.ovl_l, f.ovl_r, f.upm, f.n_pm, f.pos14) != (rec.len, rec.ovl_l, rec.ovl_r, rec.upm, rec.n_pm, rec.pos14) {
                    fill_mismatch += 1;
                }
                match units.get(&(file, rec.db.pcm_pos)) {
                    Some(&u) => uid = u,
                    None => unmapped += 1,
                }
            }
            recs.push(rec);
            uids.push(uid);
        }
        let start = out.len();
        let input = PhraseInput04 { records: &recs, uids: &uids };
        let mut tr = Vec::new();
        generate_phrase04(&mut win, bank, &input, &mut out, Some(&mut tr)).unwrap();
        for (k, t) in tr.iter().enumerate() {
            let r = &recs[k];
            spans.push((
                t.start,
                format!("s{} w{} rec{} {} mode{} len{} dur{} pitch{} ovl{}/{}", i(g.get("sentence")), r.word, k,
                    if r.typ == TYPE_SILENCE { "SIL" } else { "unit" }, r.mode, r.len, r.dur, r.pitch, r.ovl_l, r.ovl_r),
            ));
        }
        let end = out.len();
        let crs: Vec<ChunkRec> = (0..recs.len())
            .map(|k| ChunkRec {
                len: recs[k].len,
                dur: recs[k].dur,
                produced: tr.get(k + 1).map(|t| t.start).unwrap_or(end) - tr[k].start,
                silence: recs[k].typ == TYPE_SILENCE,
            })
            .collect();
        chunk_phrases.push(crs);
        let _ = start;
    }
    let want: Vec<usize> = j.get("chunks").arr().iter().map(|v| i(v) as usize).filter(|&b| b > 0).collect();
    let got = chunk_sizes04(&chunk_phrases);
    let chunk_mismatch = if got != want { Some(format!("chunks ours {:?} want {:?}", got, want)) } else { None };
    let n = out.len().min(wav.len());
    let (mut mism, mut first) = (0, None);
    for k in 0..n {
        if out[k] != wav[k] {
            if first.is_none() {
                let sp = spans.iter().rev().find(|x| x.0 <= k).map(|x| x.1.clone()).unwrap_or_default();
                first = Some(format!("sample {} ({}) ours={} ref={}", k, sp, out[k], wav[k]));
            }
            mism += 1;
        }
    }
    Outcome {
        name: path.file_stem().unwrap().to_string_lossy().into_owned(),
        samples: out.len(),
        ref_samples: wav.len(),
        mismatches: mism + out.len().abs_diff(wav.len()),
        first,
        fill_mismatch,
        unmapped,
        chunk_mismatch,
    }
}

#[test]
fn wave_parity_paul04() {
    let paths = captures();
    if paths.is_empty() || !m16().exists() {
        eprintln!("skipping: no out_wave_paul04 captures or no 2004 voice");
        return;
    }
    let bank = Bank::open(&m16()).unwrap();
    let units = unit_map(&bank);
    let mut exact = 0;
    for p in &paths {
        let o = run_capture(&bank, &units, p);
        let ok = o.mismatches == 0 && o.fill_mismatch == 0 && o.unmapped == 0 && o.chunk_mismatch.is_none();
        exact += ok as usize;
        println!(
            "{:28} {} samples {:7}/{:7} mism {:7} fill {:2} unmapped {} {} {}",
            o.name,
            if ok { "OK " } else { "BAD" },
            o.samples,
            o.ref_samples,
            o.mismatches,
            o.fill_mismatch,
            o.unmapped,
            o.first.unwrap_or_default(),
            o.chunk_mismatch.unwrap_or_default()
        );
    }
    println!("{}: {}/{} bit-exact", OUT_WAVE, exact, paths.len());
    assert_eq!(exact, paths.len());
}

/// The sentence's words as the record builder sees them, from one phrase's captured `inp`: the rate arrays
/// cover the whole sentence, the targets only this phrase's words (previous word: break code only).
fn words_from_inp(g: &Json, units: Option<&HashMap<(u8, u32), u32>>) -> Vec<Word04> {
    let inp = g.get("inp");
    let ints = |k: &str| -> Vec<i32> { inp.get(k).arr().iter().map(|v| i(v) as i32).collect() };
    let (pause, speed, volume, pitch) = (ints("pause"), ints("speed"), ints("volume"), ints("pitch"));
    let first = i(g.get("first_word")) as usize;
    let mut words = vec![Word04::default(); pause.len()];
    for (w, wd) in words.iter_mut().enumerate() {
        wd.pause = pause[w];
        wd.dur = speed[w];
        wd.vol = volume[w];
        wd.pitch = pitch[w];
    }
    for wj in inp.get("words").arr() {
        let w = i(wj.get("w")) as usize;
        words[w].brk = i(wj.get("brk")) as u8;
        let entries = wj.get("entries").hex();
        for (k, lj) in wj.get("list").arr().iter().enumerate() {
            let l = lj.hex();
            let (db, file) = primary_from_entry(&entries[0x14 * k..0x14 * k + 0x14]);
            let uid = units.and_then(|m| m.get(&(file, db.pcm_pos)).copied()).unwrap_or(0);
            words[w].targets.push(Target04 { phone: l[2], half: l[4], db, file, center: i(wj.get("ctx").at(k)) as u8, uid });
        }
    }
    if first > 0 {
        words[first - 1].brk = i(inp.get("prev_brk")) as u8;
    }
    words
}

/// Rebuilds one phrase's records from the captured builder inputs (`inp`); `None` for captures without them.
fn rebuild(g: &Json) -> Option<(Vec<Record>, bool)> {
    let inp = g.get("inp");
    if inp.is_null() {
        return None;
    }
    let words = words_from_inp(g, None);
    let first = i(g.get("first_word")) as usize;
    let n = i(g.get("n_words")) as usize;
    let ch = Channel04 { pitch: i(inp.get("spk_pitch")) as i32, dur: i(inp.get("spk_dur")) as i32, vol: i(inp.get("spk_vol")) as i32, pause: 0 };
    let mut cont = i(inp.get("cont")) != 0;
    let recs = build_records04(&words, first, n, &ch, &mut cont)?;
    Some((recs, cont))
}

/// The whole path from selected units: `build_records04` + `generate_phrase04` + chunker through
/// `engine04::synth_request04`, against the engine's WAV and chunk sizes.
#[test]
fn wave_parity_from_selection04() {
    let paths = captures();
    if paths.is_empty() || !m16().exists() {
        eprintln!("skipping: no out_wave_paul04 captures or no 2004 voice");
        return;
    }
    let bank = Bank::open(&m16()).unwrap();
    let units = unit_map(&bank);
    let (mut exact, mut n) = (0, 0);
    for p in &paths {
        let j = Json::load(p).unwrap();
        if j.get("records").arr().iter().any(|g| g.get("inp").is_null()) {
            continue;
        }
        n += 1;
        let wav = read_wav(&p.with_extension("wav"));
        let (ok, msg) = synth_matches(&bank, &units, &j, &wav);
        exact += ok as usize;
        if !ok {
            println!("{}: BAD {}", p.file_stem().unwrap().to_string_lossy(), msg);
        }
    }
    println!("from selection: {}/{} bit-exact", exact, n);
    assert_eq!(exact, n);
}

/// Sentences (selected units, rates, phrase ranges) and the channel from one capture's phrase events.
fn sentences_from_capture(j: &Json, units: &HashMap<(u8, u32), u32>) -> (Vec<Sentence04>, Channel04) {
    let mut sentences: Vec<Sentence04> = Vec::new();
    let mut last_sentence = -1;
    for g in j.get("records").arr() {
        let s = i(g.get("sentence"));
        if s != last_sentence {
            sentences.push(Sentence04 { words: words_from_inp(g, Some(units)), phrases: Vec::new() });
            last_sentence = s;
        } else {
            // Later phrases of the sentence add their own words' targets.
            let w = words_from_inp(g, Some(units));
            let cur = sentences.last_mut().unwrap();
            for (k, wd) in w.into_iter().enumerate() {
                if !wd.targets.is_empty() {
                    cur.words[k].targets = wd.targets;
                }
                if wd.brk != 0 {
                    cur.words[k].brk = wd.brk;
                }
            }
        }
        sentences.last_mut().unwrap().phrases.push((i(g.get("first_word")) as usize, i(g.get("n_words")) as usize));
    }
    let mut ch = Channel04::default();
    if let Some(g) = j.get("records").arr().first() {
        let inp = g.get("inp");
        ch = Channel04 { pitch: i(inp.get("spk_pitch")) as i32, dur: i(inp.get("spk_dur")) as i32, vol: i(inp.get("spk_vol")) as i32, pause: 0 };
    }
    (sentences, ch)
}

fn synth_matches(bank: &Bank, units: &HashMap<(u8, u32), u32>, j: &Json, wav: &[i16]) -> (bool, String) {
    let (sentences, ch) = sentences_from_capture(j, units);
    let got = synth_request04(bank, &sentences, &ch).unwrap();
    let want: Vec<usize> = j.get("chunks").arr().iter().map(|v| i(v) as usize).filter(|&b| b > 0).collect();
    let ok = got.pcm == wav && got.chunks == want;
    let first = got.pcm.iter().zip(wav).position(|(a, b)| a != b);
    (ok, format!("samples {} vs {}, first diff {:?}, chunks {:?} vs {:?}", got.pcm.len(), wav.len(), first, got.chunks, want))
}

/// The same path over every speak-corpus text, against the reference WAVs (`out_old/paul04`, the fmt 4 file
/// writer, equal to the buffer API's samples) captured by `capture_wave_paul04.py --speak`.
#[test]
fn wave_parity_speak04() {
    let dir = Path::new(HARNESS).join("out_wave_paul04_speak");
    let refs = Path::new(HARNESS).join("out_old").join("paul04");
    if !dir.exists() || !refs.exists() || !m16().exists() {
        eprintln!("skipping: no speak captures, reference WAVs or 2004 voice");
        return;
    }
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    paths.sort();
    let bank = Bank::open(&m16()).unwrap();
    let units = unit_map(&bank);
    let (mut exact, mut n, mut shown, mut empty) = (0, 0, 0, 0);
    for p in &paths {
        let name = p.file_stem().unwrap().to_string_lossy().into_owned();
        let Ok(rw) = std::fs::read(refs.join(format!("{}.wav", name))) else { continue };
        let wav: Vec<i16> = rw[44..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        let j = Json::load(p).unwrap();
        if j.get("records").arr().is_empty() {
            empty += 1;
        }
        n += 1;
        let (ok, msg) = synth_matches(&bank, &units, &j, &wav);
        exact += ok as usize;
        if !ok && shown < 20 {
            shown += 1;
            println!("{}: BAD {}", name, msg);
        }
    }
    println!("speak corpus: {}/{} bit-exact ({} without speech)", exact, n, empty);
    assert_eq!(exact, n);
}

fn same_record(a: &Record, b: &Record) -> bool {
    let key = |r: &Record| (r.pitch, r.dur, r.vol, r.len, r.typ, r.word, r.phone, r.center);
    if key(a) != key(b) {
        return false;
    }
    if a.typ == TYPE_SILENCE {
        return a.w2a == b.w2a;
    }
    (a.mode, a.file, a.pos14, a.upm, a.n_pm, a.ovl_l, a.ovl_r) == (b.mode, b.file, b.pos14, b.upm, b.n_pm, b.ovl_l, b.ovl_r)
        && (a.db.pcm_pos, a.db.pm_pos, a.db.half1_len, a.db.half2_len, a.db.coded_size, a.db.pm1, a.db.pm2, a.db.periods)
            == (b.db.pcm_pos, b.db.pm_pos, b.db.half1_len, b.db.half2_len, b.db.coded_size, b.db.pm1, b.db.pm2, b.db.periods)
}

/// `FUN_10020640` + `FUN_100224f0` from the captured builder inputs against the captured record arrays.
#[test]
fn records_parity_paul04() {
    let paths = captures();
    if paths.is_empty() {
        eprintln!("skipping: no out_wave_paul04 captures");
        return;
    }
    let (mut phrases, mut exact, mut rec_total, mut rec_bad, mut cont_bad) = (0, 0, 0, 0, 0);
    for p in &paths {
        let j = Json::load(p).unwrap();
        for g in j.get("records").arr() {
            let Some((built, cont)) = rebuild(g) else { continue };
            let want: Vec<Record> = g.get("records").arr().iter().map(|r| record_from_json(r).0).collect();
            phrases += 1;
            rec_total += want.len();
            if cont != (i(g.get("inp").get("cont_out")) != 0) {
                cont_bad += 1;
            }
            let nbad = built.iter().zip(&want).filter(|(a, b)| !same_record(a, b)).count() + built.len().abs_diff(want.len());
            if nbad == 0 {
                exact += 1;
            } else {
                rec_bad += nbad;
                let first = built.iter().zip(&want).position(|(a, b)| !same_record(a, b));
                println!(
                    "{} s{} phrase@{}: built {} want {} first diff {:?}\n  ours {:?}\n  want {:?}",
                    p.file_stem().unwrap().to_string_lossy(),
                    i(g.get("sentence")),
                    i(g.get("first_word")),
                    built.len(),
                    want.len(),
                    first,
                    first.map(|k| &built[k]),
                    first.map(|k| &want[k])
                );
            }
        }
    }
    println!("records: {}/{} phrases exact, {} of {} records differ, {} continuation flags differ", exact, phrases, rec_bad, rec_total, cont_bad);
    assert!(phrases > 0);
    assert_eq!(exact, phrases);
    assert_eq!(cont_bad, 0);
}
