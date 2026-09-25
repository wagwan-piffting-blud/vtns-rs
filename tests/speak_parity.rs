//! From-text end-to-end parity tracker: `Engine::speak(text)` against the
//! real engine's own WAVs for every line of `corpus\*.tsv` and
//! `corpus_fetext\*.txt` (`out_speak\<name>.json`/`.wav`, captures of the
//! original engine). **Report-only** (no 100%-bit-exact assert): the text
//! front end is still being ported, so this tracks its progress rather than
//! gating the build.
//!
//! For every mismatch, labels whether the divergence starts at the text
//! stage or later: `Engine::text_to_sentences`' own WA, run through the
//! already-proven-exact `run_ling`/`run_prosody`, is checked against the
//! capture's own per-word/phone ground truth (`phones[].code/.ctx/.dur`,
//! `syllables[].tone/.f0`) the same way `engine_parity.rs`'s bisection does.
//! If that already disagrees, the mismatch is tagged `text:` (a wrong WA --
//! nothing later in the pipeline could still be exact from there). If it
//! agrees but the final PCM doesn't, the mismatch is tagged `later:` -- e.g.
//! VTML per-word rate spans aren't wired from `crate::text` into
//! `SentenceInput` yet, so a `speak()` of VTML-marked-up text can produce a
//! correct WA but still desync the PCM.
//!
//!     cargo test --release --test speak_parity -- --nocapture

use std::path::{Path, PathBuf};

use vtpaul::engine::{self, Engine};
use vtpaul::json::Json;
use vtpaul::ling::{self, wr, Ss};
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

/// `VTPAUL_SPEAK_SET=<name>` + `VT_ROOT=<root>`: the `out_speak_<name>`
/// captures recorded against an alternate data root.
fn out_dir() -> String {
    match std::env::var("VTPAUL_SPEAK_SET") {
        Ok(s) if !s.is_empty() => format!("out_speak_{}", s),
        _ => "out_speak".into(),
    }
}

fn data_root() -> String {
    match std::env::var("VT_ROOT") {
        Ok(s) if !s.is_empty() => s,
        _ => DATA_ROOT.into(),
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

/// Same field-by-field ling/prosody check as `engine_parity.rs`'s
/// `check_ling_prosody` (duplicated rather than shared: each `tests/*.rs`
/// file is its own compiled crate, so there's no direct way to reuse a
/// private test-file function across them).
fn check_ling_prosody(ss: &Ss, main_sent: &Json) -> Option<String> {
    for (i, mw) in main_sent.get("words").arr().iter().enumerate() {
        let w = Ss::word(i);
        let phones = mw.get("phones").arr();
        let n_got = ss.u8(w + wr::N_PHONES) as usize;
        if n_got != phones.len() {
            return Some(format!("word {} n_phones {} want {}", i, n_got, phones.len()));
        }
        for (k, p) in phones.iter().enumerate() {
            let code = p.get("code").i64().unwrap_or(-1);
            let got_code = ss.u8(w + wr::CODES + k) as i64;
            if got_code != code {
                return Some(format!("word {} phone {} code {} want {}", i, k, got_code, code));
            }
            let ctx: Vec<u8> = p.get("ctx").hex();
            let got_ctx: Vec<u8> = (0..7).map(|j| ss.u8(w + wr::CTX + 7 * k + j)).collect();
            if got_ctx != ctx {
                return Some(format!("word {} phone {} ctx {:02x?} want {:02x?}", i, k, got_ctx, ctx));
            }
        }
        let want_brk = mw.get("brk").str().unwrap_or("]").as_bytes()[0];
        if ss.u8(w + wr::BRK) != want_brk {
            return Some(format!("word {} brk {} want {}", i, ss.u8(w + wr::BRK) as char, want_brk as char));
        }
        for (k, p) in phones.iter().enumerate() {
            let dur = p.get("dur").i64().unwrap_or(-1);
            let got_dur = ss.i16(w + WR_DUR + 2 * k) as i64;
            if got_dur != dur {
                return Some(format!("word {} phone {} dur {} want {}", i, k, got_dur, dur));
            }
        }
        let syl_base = ss.deref(ss.u32(w + wr::SYL_PTR));
        for (k, y) in mw.get("syllables").arr().iter().enumerate() {
            let Some(base) = syl_base else {
                return Some(format!("word {} has no syllable pointer", i));
            };
            let o = base + k * ling::SYL_SIZE;
            let want_tone = y.get("tone").i64().unwrap_or(0);
            let got_tone = ss.u8(o + ling::syl::TONE) as i8 as i64;
            if got_tone != want_tone {
                return Some(format!("word {} syl {} tone {} want {}", i, k, got_tone, want_tone));
            }
            let want_f0: Vec<i64> = y.get("f0").arr().iter().map(|v| v.i64().unwrap_or(0)).collect();
            let got_f0: Vec<i64> = (0..12).map(|j| ss.i16(o + ling::syl::F0 + 2 * j) as i64).collect();
            if got_f0 != want_f0 {
                return Some(format!("word {} syl {} f0 {:?} want {:?}", i, k, got_f0, want_f0));
            }
        }
    }
    None
}

fn check_unitsel(sel: &mut Selector, ss: &Ss, main_doc: &Json, si: usize) -> Option<String> {
    let mut u_words = engine::ss_to_unitsel_words(ss);
    let np = ss.n_phrases();
    let sent_type = ss.u8(ling::SS_TYPE);
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
                return Some(format!("sentence {} phrase {} uids {:?} want {:?}", si, g, got_uids, want_uids));
            }
        }
        first += nwp;
    }
    None
}

/// Runs `Engine::text_to_sentences` -> `run_ling`/`run_prosody` (the same
/// "already proven exact" stages `engine_parity.rs` uses for its own
/// bisection) and checks the result against `main`'s own per-sentence
/// ground truth. `Ok(())` means the text stage produced a WA that's
/// indistinguishable from the real engine's, all the way through prosody
/// and unit selection.
fn check_upstream_of_wave(engine: &Engine, text: &[u8], main: &Json) -> Result<(), String> {
    let inputs = engine.text_to_sentences(text);
    let main_sents = main.get("sentences").arr();
    if inputs.len() != main_sents.len() {
        return Err(format!("text: sentence count {} want {}", inputs.len(), main_sents.len()));
    }
    let mut sel = Selector::new(&engine.bank, &engine.cls, &engine.cep, &engine.sd);
    sel.reset();
    let mut cursor = 0i32;
    for (si, input) in inputs.iter().enumerate() {
        let Some((wa, mut ss, _tib)) = engine::run_ling(&engine.lex, input, cursor) else {
            return Err(format!("text: sentence {} ling stage failed", si));
        };
        cursor += wa.consumed();
        engine::run_prosody(&engine.trees, &mut ss);
        if let Some(bad) = check_ling_prosody(&ss, &main_sents[si]) {
            return Err(format!("text: sentence {} {}", si, bad));
        }
        if let Some(bad) = check_unitsel(&mut sel, &ss, main, si) {
            return Err(format!("unitsel: sentence {} {}", si, bad));
        }
    }
    Ok(())
}

/// Windows-1252 bytes, as the capture tooling hands the text to the engine
/// (`str.encode("cp1252", "replace")`).
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

fn run_one(engine: &Engine, main: &Json, name: &str) -> Result<usize, String> {
    let text = &cp1252(main.get("text").str().unwrap_or(""))[..];
    let pcm = engine.speak(text).map_err(|e| format!("{}: speak io error: {}", name, e))?;
    let want_path = Path::new(HARNESS).join(out_dir()).join(format!("{}.wav", name));
    let want = read_wav(&want_path);
    if pcm == want {
        return Ok(pcm.len());
    }
    if let Err(bad) = check_upstream_of_wave(engine, text, main) {
        return Err(format!("{}: {}", name, bad));
    }
    if pcm.len() != want.len() {
        Err(format!("{}: later: {} samples, engine wav has {} (text/ling/prosody/unitsel all matched)", name, pcm.len(), want.len()))
    } else {
        let first = pcm.iter().zip(&want).position(|(a, b)| a != b).unwrap();
        Err(format!("{}: later: samples differ, first at {} (text/ling/prosody/unitsel all matched)", name, first))
    }
}

#[test]
fn speak_matches_engine() {
    let dir = Path::new(HARNESS).join(out_dir());
    let root = data_root();
    if !dir.exists() || !Path::new(&root).exists() {
        eprintln!("skipping: no out_speak captures or no voice data (need a local capture directory; set VTPAUL_HARNESS)");
        return;
    }
    let engine = Engine::open(Path::new(&root)).expect("open engine");
    let mut names: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    names.sort();
    if names.is_empty() {
        eprintln!("skipping: no captures in {}", dir.display());
        return;
    }
    let (mut ok, mut text_fail, mut later_fail, mut other_fail) = (0usize, 0usize, 0usize, 0usize);
    let mut shown = 0;
    for path in &names {
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let main = match Json::load(path) {
            Ok(d) => d,
            Err(e) => {
                other_fail += 1;
                if shown < 40 {
                    shown += 1;
                    println!("  PARSE-ERROR {}: {}", name, e);
                }
                continue;
            }
        };
        match run_one(&engine, &main, &name) {
            Ok(_) => ok += 1,
            Err(msg) => {
                if msg.contains(": text:") || msg.contains(": unitsel:") {
                    text_fail += 1;
                } else if msg.contains(": later:") {
                    later_fail += 1;
                } else {
                    other_fail += 1;
                }
                if shown < 40 {
                    shown += 1;
                    println!("  MISMATCH {}", msg);
                }
            }
        }
    }
    let total = names.len();
    println!(
        "speak parity: {}/{} bit-exact ({} text/WA-stage mismatches, {} later-stage, {} other; {} shown above)",
        ok, total, text_fail, later_fail, other_fail, shown
    );
}

/// `out_speak_params`: the same texts under non-default
/// pitch/speed/volume/pause/comma/emphasis, including values the engine's
/// setters clamp.
#[test]
fn speak_params_matches_engine() {
    let dir = Path::new(HARNESS).join("out_speak_params");
    if !dir.exists() || !Path::new(DATA_ROOT).exists() {
        eprintln!("skipping: no out_speak_params captures (need a local capture directory; set VTPAUL_HARNESS)");
        return;
    }
    let mut engine = Engine::open(Path::new(DATA_ROOT)).expect("open engine");
    let mut names: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    names.sort();
    let (mut ok, mut shown) = (0usize, 0usize);
    for path in &names {
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let doc = Json::load(path).unwrap();
        let p: Vec<i32> = doc.get("params").arr().iter().map(|v| v.i64().unwrap_or(-1) as i32).collect();
        engine.chan = engine::Channel::default();
        engine.set_params(p[0], p[1], p[2], p[3], p[4], p[5]);
        let text = cp1252(doc.get("text").str().unwrap_or(""));
        let pcm = engine.speak(&text).unwrap();
        let want = read_wav(&dir.join(format!("{}.wav", name)));
        if pcm == want {
            ok += 1;
        } else if shown < 40 {
            shown += 1;
            let first = pcm.iter().zip(&want).position(|(a, b)| a != b);
            println!("  MISMATCH {} {:?}: {} samples want {}, first diff {:?}", name, p, pcm.len(), want.len(), first);
        }
    }
    println!("speak params parity: {}/{} bit-exact", ok, names.len());
    assert_eq!(ok, names.len());
}
