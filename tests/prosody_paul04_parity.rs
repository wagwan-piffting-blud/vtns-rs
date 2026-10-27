//! Prosody parity of the 2004 Paul build against `out_prosody_paul04/*.json` (hooks_prosody_paul04.js):
//! phone durations, syllable tones, raw and smoothed F0 contours of every captured sentence.

use std::path::{Path, PathBuf};

use vtpaul::era::Era;
use vtpaul::json::Json;
use vtpaul::prosody::*;

const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};
const VOICES: &str = match option_env!("VTPAUL_VOICES") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};
const OUT_PRO: &str = "out_prosody_paul04";

fn tree2_dir() -> PathBuf {
    Path::new(VOICES).join("2004_paul").join("data-paul").join("M16").join("ttsdata").join("tree")
}

fn captures() -> Vec<PathBuf> {
    let dir = Path::new(HARNESS).join(OUT_PRO);
    let mut v: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect(),
        Err(_) => Vec::new(),
    };
    v.sort();
    v
}

#[derive(Default)]
struct Tally {
    total: usize,
    bad: usize,
}

impl Tally {
    fn check(&mut self, ok: bool) -> bool {
        self.total += 1;
        if !ok {
            self.bad += 1;
        }
        ok
    }

    fn line(&self, what: &str) -> String {
        format!("{:<22} {:>8} / {:>8} match", what, self.total - self.bad, self.total)
    }
}

fn shorts(j: &Json) -> Vec<i16> {
    j.arr().iter().map(|v| v.i64().unwrap() as i16).collect()
}

/// The sentence as the duration step sees it: per-syllable code/position arrays joined into the word's
/// phone list (checked contiguous by the capture), phrases from the SS phrase table (word count at +0x0e).
fn sentence_from(ev: &Json, contig: &mut Tally, phr: &mut Tally) -> Sentence {
    let mut s = Sentence { era: Era::Paul04, ..Default::default() };
    for w in ev.get("words").arr() {
        let (mut codes, mut pos_code, mut syllables) = (Vec::new(), Vec::new(), Vec::new());
        for y in w.get("syllables").arr() {
            contig.check(y.get("contig") == &Json::Bool(true));
            let first = codes.len();
            let c: Vec<u8> = y.get("codes").arr().iter().map(|v| v.i64().unwrap() as u8).collect();
            let p: Vec<i8> = y.get("pos").arr().iter().map(|v| v.i64().unwrap() as i8).collect();
            let n = c.len();
            codes.extend(c);
            pos_code.extend(p);
            syllables.push(Syllable {
                b0: y.get("b0").i64().unwrap() as i8,
                b1: y.get("b1").i64().unwrap() as i8,
                b2: y.get("b2").i64().unwrap() as i8,
                tone: 0,
                f0: [0; 12],
                first_phone: first as u8,
                n_phones: n as u8,
            });
        }
        let n = codes.len();
        s.words.push(Word { codes, pos_code, syllables, brk: w.get("brk").i64().unwrap() as u8, dur: vec![0; n] });
    }
    for p in ev.get("phrases").arr() {
        let raw = p.get("raw").hex();
        s.phrase_words.push(u16::from_le_bytes([raw[2], raw[3]]) as usize);
        let syls = p.get("syls").i64().unwrap() as usize;
        let _ = syls;
    }
    let total: usize = s.phrase_words.iter().sum();
    phr.check(total == s.words.len());
    s
}

#[test]
fn prosody_matches_engine() {
    let caps = captures();
    if caps.is_empty() || !tree2_dir().exists() {
        eprintln!("skipping: captures or voice data missing");
        return;
    }
    let trees = Trees::load_tree2(&tree2_dir()).unwrap();
    let (mut contig, mut phr) = (Tally::default(), Tally::default());
    let (mut dur, mut tone, mut raw, mut smooth) = (Tally::default(), Tally::default(), Tally::default(), Tally::default());
    let mut shown = 0;
    let mut sentences = 0;
    for path in &caps {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let cap = Json::load(path).unwrap();
        let ev = cap.get("events").arr();
        let of = |kind: &str, si: i64| ev.iter().find(|e| e.get("kind").str() == Some(kind) && e.get("sentence").i64() == Some(si));
        for e in ev.iter().filter(|e| e.get("kind").str() == Some("dur_in")) {
            let si = e.get("sentence").i64().unwrap();
            let (Some(d_out), Some(p_raw), Some(p_out)) = (of("dur_out", si), of("pitch_raw", si), of("pitch_out", si)) else { continue };
            sentences += 1;
            let mut s = sentence_from(e, &mut contig, &mut phr);
            predict_durations(&trees, &mut s);
            for (wi, want) in d_out.get("dur").arr().iter().enumerate() {
                let want = shorts(want);
                for (k, &d) in want.iter().enumerate() {
                    if !dur.check(s.words[wi].dur[k] == d) && shown < 20 {
                        shown += 1;
                        eprintln!("{} s{} w{} p{}: dur {} want {}", name, si, wi, k, s.words[wi].dur[k], d);
                    }
                }
            }
            predict_pitch_raw(&trees, &mut s);
            for (wi, w) in p_raw.get("words").arr().iter().enumerate() {
                for (yi, y) in w.get("syllables").arr().iter().enumerate() {
                    tone.check(s.words[wi].syllables[yi].tone as i64 == y.get("tone").i64().unwrap());
                    let want = shorts(y.get("f0"));
                    if !raw.check(s.words[wi].syllables[yi].f0[..] == want[..]) && shown < 20 {
                        shown += 1;
                        eprintln!("{} s{} w{} y{}: f0_raw {:?} want {:?}", name, si, wi, yi, s.words[wi].syllables[yi].f0, want);
                    }
                }
            }
            smooth_f0_04(&mut s);
            for (wi, w) in p_out.get("words").arr().iter().enumerate() {
                for (yi, y) in w.get("syllables").arr().iter().enumerate() {
                    let want = shorts(y.get("f0"));
                    if !smooth.check(s.words[wi].syllables[yi].f0[..] == want[..]) && shown < 20 {
                        shown += 1;
                        eprintln!("{} s{} w{} y{}: f0 {:?} want {:?}", name, si, wi, yi, s.words[wi].syllables[yi].f0, want);
                    }
                }
            }
        }
    }
    println!("{} captures, {} sentences", caps.len(), sentences);
    for (t, n) in [(&contig, "contiguous syllables"), (&phr, "phrase word totals"), (&dur, "phone durations"),
                   (&tone, "syllable tones"), (&raw, "raw F0 contours"), (&smooth, "smoothed F0 contours")] {
        println!("{}", t.line(n));
    }
    assert_eq!(contig.bad + phr.bad + dur.bad + tone.bad + raw.bad + smooth.bad, 0);
}
