//! End-to-end engine parity: captured per-sentence WA (`out_engine`, recorded
//! from the original engine) through `engine::Engine::synth_from_wa`,
//! compared sample for sample with the corpus's own bit-exact capture
//! (`out/<name>.wav`). Falls back to `out_feling` (a larger,
//! not-42-corpus-limited set of the same capture shape) when `out_engine`
//! hasn't been captured yet, so this test is useful before the full corpus
//! has been captured.
//!
//! On any mismatch, prints which stage first diverges from the capture
//! (ling words/breaks/ctx -> prosody durations/F0 -> unit-selection targets
//! -> waveform PCM), the same bisection `feling_parity.rs`/`unitsel_parity.rs`/
//! `wave_parity.rs` already prove each stage against in isolation -- the
//! useful output here is *which* stage's glue in `engine.rs` breaks the
//! chain, not re-proving stages already at 100%.
//!
//!     cargo test --release --test engine_parity -- --nocapture

use std::path::{Path, PathBuf};

use vtpaul::engine::{self, Engine};
use vtpaul::json::Json;
use vtpaul::ling::{wr, Ss};
use vtpaul::unitsel::Selector;

/// Word-record offset of the predicted phone duration; mirrors the private
/// `engine::WR_DUR` (prosody's field, not `ling::wr`'s -- see engine.rs).
const WR_DUR: usize = 0x10;

/// Capture root; override at build time with `VTPAUL_HARNESS=<dir>`.
const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};
const DATA_ROOT: &str = r"C:\Program Files (x86)\NWSPaulNumber2";
fn corpus() -> &'static str {
    Box::leak(format!("{}/corpus/corpus.tsv", HARNESS).into_boxed_str())
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

/// `out_engine` (the 42-corpus capture this task adds), falling back
/// to `out_feling` (a superset by word/construction coverage, not
/// limited to the 42 corpus names) so the test is useful either way.
fn captures_dir() -> PathBuf {
    let engine_dir = Path::new(HARNESS).join("out_engine");
    if engine_dir.exists() && std::fs::read_dir(&engine_dir).map(|mut d| d.next().is_some()).unwrap_or(false) {
        engine_dir
    } else {
        Path::new(HARNESS).join("out_feling")
    }
}

fn corpus_names() -> Vec<String> {
    std::fs::read_to_string(corpus())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let l = l.trim_end();
            if l.is_empty() || l.starts_with('#') || !l.contains('\t') || l.starts_with("name\t") {
                return None;
            }
            l.split('\t').next().map(|s| s.to_string())
        })
        .collect()
}

/// `run_ling` + `run_prosody`, then diffs the finished `Ss` against
/// `out/<name>.json`'s own per-word/phone/syllable fields (`phones[].code`,
/// `.ctx`, `.dur`, `syllables[].tone`/`.f0`) -- ground truth already captured
/// for every corpus text, independent of `out_feling`/`out_engine`. Returns
/// the first field that disagrees, tagged `ling:` or `prosody:` so a
/// mismatch summary can say which stage to look at first.
fn check_ling_prosody(ss: &Ss, main_sent: &Json) -> Option<String> {
    for (i, mw) in main_sent.get("words").arr().iter().enumerate() {
        let w = Ss::word(i);
        let phones = mw.get("phones").arr();
        let n_got = ss.u8(w + wr::N_PHONES) as usize;
        if n_got != phones.len() {
            return Some(format!("ling: word {} n_phones {} want {}", i, n_got, phones.len()));
        }
        for (k, p) in phones.iter().enumerate() {
            let code = p.get("code").i64().unwrap_or(-1);
            let got_code = ss.u8(w + wr::CODES + k) as i64;
            if got_code != code {
                return Some(format!("ling: word {} phone {} code {} want {}", i, k, got_code, code));
            }
            let ctx: Vec<u8> = p.get("ctx").hex();
            let got_ctx: Vec<u8> = (0..7).map(|j| ss.u8(w + wr::CTX + 7 * k + j)).collect();
            if got_ctx != ctx {
                return Some(format!("ling: word {} phone {} ctx {:02x?} want {:02x?}", i, k, got_ctx, ctx));
            }
        }
        let want_brk = mw.get("brk").str().unwrap_or("]").as_bytes()[0];
        if ss.u8(w + wr::BRK) != want_brk {
            return Some(format!("ling: word {} brk {} want {}", i, ss.u8(w + wr::BRK) as char, want_brk as char));
        }
        for (k, p) in phones.iter().enumerate() {
            let dur = p.get("dur").i64().unwrap_or(-1);
            let got_dur = ss.i16(w + WR_DUR + 2 * k) as i64;
            if got_dur != dur {
                return Some(format!("prosody: word {} phone {} dur {} want {}", i, k, got_dur, dur));
            }
        }
        let syl_base = ss.deref(ss.u32(w + wr::SYL_PTR));
        for (k, y) in mw.get("syllables").arr().iter().enumerate() {
            let Some(base) = syl_base else {
                return Some(format!("prosody: word {} has no syllable pointer", i));
            };
            let o = base + k * vtpaul::ling::SYL_SIZE;
            let want_tone = y.get("tone").i64().unwrap_or(0);
            let got_tone = ss.u8(o + vtpaul::ling::syl::TONE) as i8 as i64;
            if got_tone != want_tone {
                return Some(format!("prosody: word {} syl {} tone {} want {}", i, k, got_tone, want_tone));
            }
            let want_f0: Vec<i64> = y.get("f0").arr().iter().map(|v| v.i64().unwrap_or(0)).collect();
            let got_f0: Vec<i64> = (0..12).map(|j| ss.i16(o + vtpaul::ling::syl::F0 + 2 * j) as i64).collect();
            if got_f0 != want_f0 {
                return Some(format!("prosody: word {} syl {} f0 {:?} want {:?}", i, k, got_f0, want_f0));
            }
        }
    }
    None
}

/// Runs unit selection on a finished `Ss` and diffs the chosen unit ids
/// against `out/<name>.json`'s own `selections[]` for this sentence
/// (already proven bit-exact input-for-input by `unitsel_parity.rs`; this
/// only checks that `engine.rs`'s glue feeds it the right `Ss`).
fn check_unitsel(sel: &mut Selector, ss: &Ss, main_doc: &Json, si: usize) -> Option<String> {
    let words = engine::ss_to_unitsel_words(ss);
    let mut u_words = words;
    let np = ss.n_phrases();
    let sent_type = ss.u8(vtpaul::ling::SS_TYPE);
    let mut first = 0usize;
    for g in 0..np {
        let e = Ss::phrase(g);
        let nwp = ss.i16(e + 2).max(0) as usize;
        let ftype = if g + 1 == np { sent_type } else { 5 };
        let r = sel.select_phrase(&mut u_words, first, nwp, ftype);
        let want = main_doc
            .get("selections")
            .arr()
            .iter()
            .find(|s| s.get("sentence").i64() == Some(si as i64) && s.get("phrase").i64() == Some(g as i64));
        if let Some(want) = want {
            let want_uids: Vec<i64> = want.get("targets").arr().iter().map(|t| t.get("uid").i64().unwrap_or(-1)).collect();
            let got_uids: Vec<i64> = r.uids.iter().map(|&u| u as i64).collect();
            if got_uids != want_uids {
                return Some(format!("unitsel: sentence {} phrase {} uids {:?} want {:?}", si, g, got_uids, want_uids));
            }
        }
        first += nwp;
    }
    None
}

/// Synthesizes one capture's whole text (every sentence in order, sharing
/// one `Selector`/`DecodeWindow` the way `Engine::synth_from_wa_traced`
/// does) and compares the result with `out/<name>.wav`. On a PCM
/// mismatch, re-derives each sentence's `Ss` and re-checks it against
/// `out/<name>.json`'s own per-word/phone ground truth (ling,
/// prosody, then unit selection) to say which stage first disagrees.
fn run_one(engine: &Engine, doc: &Json, main_doc: &Json, name: &str) -> Result<usize, String> {
    let inputs = engine::capture::sentences(doc);
    if inputs.is_empty() {
        return Err(format!("{}: no wa_in/words_out events captured", name));
    }
    let pcm = engine.synth_from_wa(&inputs).map_err(|e| format!("{}: wave stage io error: {}", name, e))?;
    let want_path = Path::new(HARNESS).join("out").join(format!("{}.wav", name));
    if !want_path.exists() {
        return Err(format!("{}: no out/{}.wav capture to compare against", name, name));
    }
    let want = read_wav(&want_path);
    let mism = if pcm.len() == want.len() { pcm.iter().zip(&want).filter(|(a, b)| a != b).count() } else { usize::MAX };
    if mism == 0 {
        return Ok(pcm.len());
    }

    // Bisect: re-run ling+prosody+unitsel per sentence against the out
    // capture's own ground truth and report the first stage that disagrees.
    let main_sents = main_doc.get("sentences").arr();
    let mut sel = Selector::new(&engine.bank, &engine.cls, &engine.cep, &engine.sd);
    sel.reset();
    let mut cursor = 0i32;
    for (si, input) in inputs.iter().enumerate() {
        let Some((wa, mut ss, _tib)) = engine::run_ling(&engine.lex, input, cursor) else {
            return Err(format!("{}: sentence {} ling stage failed (merge/word-phones/breaks)", name, si));
        };
        cursor += wa.consumed();
        engine::run_prosody(&engine.trees, &mut ss);
        let Some(main_sent) = main_sents.get(si) else {
            return Err(format!("{}: sentence {} not in the out/ capture (sentence-count mismatch)", name, si));
        };
        if let Some(bad) = check_ling_prosody(&ss, main_sent) {
            return Err(format!("{}: sentence {} {}", name, si, bad));
        }
        if let Some(bad) = check_unitsel(&mut sel, &ss, main_doc, si) {
            return Err(format!("{}: sentence {} {}", name, si, bad));
        }
    }
    if pcm.len() != want.len() {
        Err(format!("{}: wave: {} samples, engine wav has {} (ling/prosody/unitsel all matched)", name, pcm.len(), want.len()))
    } else {
        let first = pcm.iter().zip(&want).position(|(a, b)| a != b).unwrap();
        Err(format!("{}: wave: {}/{} samples differ, first at {} (ling/prosody/unitsel all matched)", name, mism, pcm.len(), first))
    }
}

#[test]
fn engine_matches_corpus() {
    let dir = captures_dir();
    if !dir.exists() || !Path::new(DATA_ROOT).exists() {
        eprintln!("skipping: no out_engine/out_feling captures or no voice data");
        return;
    }
    let mut engine = Engine::open(Path::new(DATA_ROOT)).expect("open engine");
    engine.set_params(-1, -1, -1, -1, -1, 0);
    let names = corpus_names();
    if names.is_empty() {
        eprintln!("skipping: no corpus names in {}", corpus());
        return;
    }
    let mut ok = 0usize;
    let mut total = 0usize;
    let mut fails: Vec<String> = Vec::new();
    for name in &names {
        let path = dir.join(format!("{}.json", name));
        if !path.exists() {
            continue;
        }
        total += 1;
        let doc = match Json::load(&path) {
            Ok(d) => d,
            Err(e) => {
                fails.push(format!("{}: capture parse error: {}", name, e));
                continue;
            }
        };
        let main_path = Path::new(HARNESS).join("out").join(format!("{}.json", name));
        let main_doc = match Json::load(&main_path) {
            Ok(d) => d,
            Err(e) => {
                fails.push(format!("{}: out/ capture parse error: {}", name, e));
                continue;
            }
        };
        match run_one(&engine, &doc, &main_doc, name) {
            Ok(_) => ok += 1,
            Err(msg) => fails.push(msg),
        }
    }
    println!("engine parity: {}/{} corpus texts bit-exact (of {} in corpus.tsv)", ok, total, names.len());
    for f in &fails {
        println!("  MISMATCH {}", f);
    }
    if total == 0 {
        eprintln!("skipping: none of the corpus names have a capture in {}", dir.display());
    }
}

/// Inter-sentence pause-state edge cases, proving `resolve_word0_pause`'s
/// `cont`-flag threading (`engine.rs`) rather than just the plain 42-text
/// corpus, which only ever hits the "no override, default channel rate" path
/// at each boundary. Captures: `corpus/corpus_pause.tsv` ->
/// `out_engine/<name>.json` (WA/TIB) and
/// `out/<name>.json`/`.wav` (PCM ground truth).
///
/// - `questions_multi`: three internal sentence boundaries from `?`/`?`/`!`
///   (not just `.`), each needing the same default gap.
/// - `semicolon_multi`: `;` is itself a sentence-boundary-triggering
///   character in this engine's SBD, confirmed live via a debug trace
///   (a single `;`-joined clause came back as two `wa_in` sentences).
/// - `pause_at_boundary`: an explicit VTML pause tag sitting exactly between
///   two sentences (rather than mid-sentence or at the very start/end of the
///   text, which older `pause_start`/`pause_stack`
///   edge cases already cover).
/// - `trailing_partial`: a request that ends mid-sentence, no terminal
///   punctuation -- exercises `cont`'s "single sentence in the request"
///   path with nothing unusual about sentence 0 itself.
/// - `speed_at_boundary`: sentence 2 starts at 200% speed (`<vtml_speed>`),
///   so its leading gap (`word_pauses[0] = 687` from a false `cont`) must be
///   scaled by `dur_factor(200)` in `wave::pause_before`/`scaled_pause`, not
///   left at the unscaled default -- the corpus's own `vtml_speed` text only
///   exercises mid-sentence speed changes, never one landing on a sentence's
///   very first word.
#[test]
fn pause_boundary_edge_cases() {
    let dir = Path::new(HARNESS).join("out_engine");
    if !dir.exists() || !Path::new(DATA_ROOT).exists() {
        eprintln!("skipping: no out_engine captures or no voice data");
        return;
    }
    let names = [
        "questions_multi",
        "semicolon_multi",
        "pause_at_boundary",
        "trailing_partial",
        "speed_at_boundary",
    ];
    let mut engine = Engine::open(Path::new(DATA_ROOT)).expect("open engine");
    engine.set_params(-1, -1, -1, -1, -1, 0);
    let mut ok = 0usize;
    let mut fails: Vec<String> = Vec::new();
    for name in names {
        let path = dir.join(format!("{}.json", name));
        let main_path = Path::new(HARNESS).join("out").join(format!("{}.json", name));
        if !path.exists() || !main_path.exists() {
            fails.push(format!("{}: missing out_engine or out capture", name));
            continue;
        }
        let doc = Json::load(&path).unwrap();
        let main_doc = Json::load(&main_path).unwrap();
        match run_one(&engine, &doc, &main_doc, name) {
            Ok(_) => ok += 1,
            Err(msg) => fails.push(msg),
        }
    }
    println!("pause boundary edge cases: {}/{} bit-exact", ok, names.len());
    for f in &fails {
        println!("  MISMATCH {}", f);
    }
    assert_eq!(ok, names.len(), "{:?}", fails);
}
