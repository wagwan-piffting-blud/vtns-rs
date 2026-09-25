//! Prosody parity against captures of the original engine:
//! `out/*.json` (main) and `out_prosody/*.json` (stage hooks).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use vtpaul::json::Json;
use vtpaul::prosody::*;

/// Capture root; override at build time with `VTPAUL_HARNESS=<dir>`.
const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};
const TREE3: &str = r"C:\Program Files (x86)\NWSPaulNumber2\data-paul\M16\ttsdata\tree3";

fn captures(sub: &str) -> Vec<PathBuf> {
    let dir = Path::new(HARNESS).join(sub);
    let mut v: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .filter(|p| !p.file_name().unwrap().to_string_lossy().starts_with('_'))
            .collect(),
        Err(_) => Vec::new(),
    };
    v.sort();
    v
}

fn sentence_from(js: &Json) -> Sentence {
    let mut s = Sentence::default();
    for w in js.get("words").arr() {
        let raw = w.get("raw").hex();
        let phones = w.get("phones").arr();
        let codes: Vec<u8> = phones.iter().map(|p| p.get("code").i64().unwrap() as u8).collect();
        let n = codes.len();
        let pos_code = (0..n).map(|k| raw[0x36a + k] as i8).collect();
        let syllables = w
            .get("syllables")
            .arr()
            .iter()
            .map(|y| Syllable {
                b0: y.get("b0").i64().unwrap() as i8,
                b1: y.get("b1").i64().unwrap() as i8,
                b2: y.get("b2").i64().unwrap() as i8,
                tone: 0,
                f0: [0; 12],
                first_phone: y.get("first_phone").i64().unwrap() as u8,
                n_phones: y.get("n_phones").i64().unwrap() as u8,
            })
            .collect();
        let brk = w.get("brk").str().unwrap().as_bytes()[0];
        s.words.push(Word { codes, pos_code, syllables, brk, dur: vec![0; n] });
    }
    s.phrase_words = js.get("phrase_words").arr().iter().map(|x| x.i64().unwrap() as usize).collect();
    s
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
        format!("{:<22} {:>7} / {:>7} match", what, self.total - self.bad, self.total)
    }
}

#[test]
fn prosody_matches_engine() {
    let caps = captures("out");
    if caps.is_empty() || !Path::new(TREE3).exists() {
        eprintln!("skipping: captures or voice data missing");
        return;
    }
    let trees = Trees::load(Path::new(TREE3)).unwrap();
    let (mut dur, mut tone, mut raw, mut smooth, mut feat) =
        (Tally::default(), Tally::default(), Tally::default(), Tally::default(), Tally::default());
    let mut shown = 0;
    for path in &caps {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let cap = Json::load(path).unwrap();
        let pro = Json::load(&Path::new(HARNESS).join("out_prosody").join(format!("{}.json", name))).ok();
        let mut sents = Vec::new();
        for (si, js) in cap.get("sentences").arr().iter().enumerate() {
            let mut s = sentence_from(js);
            predict_durations(&trees, &mut s);
            for (wi, w) in js.get("words").arr().iter().enumerate() {
                for (k, p) in w.get("phones").arr().iter().enumerate() {
                    let want = p.get("dur").i64().unwrap() as i16;
                    if !dur.check(s.words[wi].dur[k] == want) && shown < 20 {
                        shown += 1;
                        eprintln!("{} s{} w{} p{}: dur {} want {}", name, si, wi, k, s.words[wi].dur[k], want);
                    }
                }
            }
            predict_pitch_raw(&trees, &mut s);
            let f0r = cap.get("f0_raw").arr().iter().find(|e| e.get("sentence").i64() == Some(si as i64));
            for (wi, w) in js.get("words").arr().iter().enumerate() {
                for (yi, y) in w.get("syllables").arr().iter().enumerate() {
                    tone.check(s.words[wi].syllables[yi].tone as i64 == y.get("tone").i64().unwrap());
                    if let Some(e) = f0r {
                        let want: Vec<i16> =
                            e.get("f0").at(wi).at(yi).arr().iter().map(|v| v.i64().unwrap() as i16).collect();
                        if !raw.check(s.words[wi].syllables[yi].f0[..] == want[..]) && shown < 20 {
                            shown += 1;
                            eprintln!("{} s{} w{} y{}: f0_raw {:?} want {:?}", name, si, wi, yi,
                                s.words[wi].syllables[yi].f0, want);
                        }
                    }
                }
            }
            smooth_f0(&mut s);
            for (wi, w) in js.get("words").arr().iter().enumerate() {
                for (yi, y) in w.get("syllables").arr().iter().enumerate() {
                    let want: Vec<i16> = y.get("f0").arr().iter().map(|v| v.i64().unwrap() as i16).collect();
                    if !smooth.check(s.words[wi].syllables[yi].f0[..] == want[..]) && shown < 20 {
                        shown += 1;
                        eprintln!("{} s{} w{} y{}: f0 {:?} want {:?}", name, si, wi, yi,
                            s.words[wi].syllables[yi].f0, want);
                    }
                }
            }
            sents.push(s);
        }
        // Target features, using the word target lists from the stage capture.
        let Some(pro) = pro else { continue };
        let mut lists: BTreeMap<(i64, i64), Vec<Vec<TargetRef>>> = BTreeMap::new();
        for e in pro.get("events").arr().iter().filter(|e| e.get("kind").str() == Some("wordtargets")) {
            let l = e
                .get("lists")
                .arr()
                .iter()
                .map(|w| {
                    w.arr()
                        .iter()
                        .map(|t| {
                            let b: Vec<u8> = t.arr().iter().map(|x| x.i64().unwrap() as u8).collect();
                            TargetRef { word: b[0], syl: b[1], phone: b[2], index: b[3], half: b[4], unvoiced: b[5] }
                        })
                        .collect()
                })
                .collect();
            lists.insert((e.get("sentence").i64().unwrap(), e.get("phrase").i64().unwrap()), l);
        }
        for sel in cap.get("selections").arr() {
            let si = sel.get("sentence").i64().unwrap();
            let key = (si, sel.get("phrase").i64().unwrap());
            let Some(wl) = lists.get(&key) else { continue };
            let s = &sents[si as usize];
            for tg in sel.get("targets").arr() {
                let t = TargetRef {
                    word: tg.get("word").i64().unwrap() as u8,
                    syl: tg.get("syl").i64().unwrap() as u8,
                    phone: tg.get("phone").i64().unwrap() as u8,
                    index: tg.get("syl_phone").i64().unwrap() as u8,
                    half: tg.get("half").i64().unwrap() as u8,
                    unvoiced: tg.get("unvoiced").i64().unwrap() as u8,
                };
                let f = tg.get("feat");
                if f.is_null() {
                    continue;
                }
                let want = [
                    f.get("dur_code").i64().unwrap() as u8,
                    f.get("f0_a").i64().unwrap() as u8,
                    f.get("f0_b").i64().unwrap() as u8,
                ];
                let got = target_feature(&s.words[t.word as usize], &wl[t.word as usize], &t);
                if !feat.check(got == want) && shown < 40 {
                    shown += 1;
                    eprintln!("{} s{} t{}: feat {:?} want {:?} ({:?})", name, si, tg.get("t").i64().unwrap(), got, want, t);
                }
            }
        }
    }
    println!("{} captures", caps.len());
    for (t, n) in [(&dur, "phone durations"), (&tone, "syllable tones"), (&raw, "raw F0 contours"),
                   (&smooth, "smoothed F0 contours"), (&feat, "target features")] {
        println!("{}", t.line(n));
    }
    assert_eq!(dur.bad + tone.bad + raw.bad + smooth.bad + feat.bad, 0);
}

fn ints(j: &Json) -> Vec<i32> {
    j.arr().iter().map(|v| v.i64().unwrap() as i32).collect()
}

fn rates_from(j: &Json) -> WordRates {
    WordRates {
        pitch: ints(j.get("pitch")),
        speed: ints(j.get("speed")),
        volume: ints(j.get("volume")),
        pause: ints(j.get("pause")),
        brk: ints(j.get("brk")),
        k7: ints(j.get("k7")),
    }
}

#[test]
fn rates_and_pauses_match_engine() {
    let caps = captures("out_prosody");
    if caps.is_empty() {
        eprintln!("skipping: out_prosody captures missing");
        return;
    }
    let (mut seed, mut spans, mut sp, mut gaps, mut sil, mut trail) = (
        Tally::default(), Tally::default(), Tally::default(), Tally::default(), Tally::default(), Tally::default(),
    );
    let mut shown = 0;
    for path in &caps {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let pro = Json::load(path).unwrap();
        let ev = pro.get("events").arr();
        for e in ev.iter().filter(|e| e.get("kind").str() == Some("spans")) {
            let before = rates_from(e.get("before"));
            let n = before.pitch.len();
            let seeded = WordRates::seed(
                n,
                e.get("base_pitch").i64().unwrap() as i32,
                e.get("base_speed").i64().unwrap() as i32,
                e.get("base_volume").i64().unwrap() as i32,
            );
            seed.check(seeded == before);
            let words: Vec<(i32, i32)> = e
                .get("words")
                .arr()
                .iter()
                .map(|w| (w.get("start").i64().unwrap() as i32, w.get("end").i64().unwrap() as i32))
                .collect();
            let ctx = SpanContext {
                words: &words,
                total_len: e.get("total_len").i64().unwrap() as i32,
                cursor: e.get("cursor").i64().unwrap() as i32,
                sent_len: e.get("sent_len").i64().unwrap() as i32,
            };
            let mut lists: [SpanList; 8] = Default::default();
            for (k, l) in e.get("spans").arr().iter().enumerate() {
                if l.get("has_pos").f64() == Some(1.0) && l.get("has_val").f64() == Some(1.0) {
                    lists[k] = SpanList { pos: ints(l.get("pos")), val: ints(l.get("val")), cursor: l.get("cursor").i64().unwrap() as i32 };
                } else {
                    lists[k].cursor = l.get("cursor").i64().unwrap() as i32;
                }
            }
            let mut r = before.clone();
            let mut tr = e.get("trail_in").i64().unwrap() as i32;
            apply_tag_spans(&ctx, &mut lists, &mut r, &mut tr);
            let want = rates_from(e.get("after"));
            let curs: Vec<i32> = lists.iter().map(|l| l.cursor).collect();
            let ok = r == want && tr == e.get("trail_out").i64().unwrap() as i32 && curs == ints(e.get("cursors_out"));
            if !spans.check(ok) && shown < 20 {
                shown += 1;
                eprintln!("{} spans: got {:?} trail {} curs {:?}\n  want {:?} trail {} curs {:?}", name, r, tr, curs,
                    want, e.get("trail_out").i64().unwrap(), ints(e.get("cursors_out")));
            }
        }
        for e in ev.iter().filter(|e| e.get("kind").str() == Some("sentpause")) {
            let mut p0 = e.get("p0_in").i64().unwrap() as i32;
            resolve_sentence_pause(
                &mut p0,
                e.get("call_pause").i64().unwrap() as i32,
                e.get("tib_pause").i64().unwrap() as i32,
                e.get("cont").i64().unwrap() != 0,
                e.get("ch_pause").i64().unwrap() as i32,
            );
            if !sp.check(p0 == e.get("p0_out").i64().unwrap() as i32) && shown < 20 {
                shown += 1;
                eprintln!("{} sentpause: got {} ev {:?}", name, p0, e);
            }
        }
        // Main capture for the same text, if the corpus holds it: silence records.
        let main = Json::load(&Path::new(HARNESS).join("out").join(format!("{}.json", name))).ok();
        let mut expect_sil = Vec::new();
        for e in ev.iter().filter(|e| e.get("kind").str() == Some("gap")) {
            let w = e.get("w").i64().unwrap() as usize;
            let mut pause = vec![-1i32; w + 1];
            pause[w] = e.get("pw").i64().unwrap() as i32;
            let mut cont = e.get("cont").i64().unwrap() != 0;
            let prev = e.get("prev_brk").i64().unwrap_or(0) as u8;
            let comma = pro.get("params").at(4).i64().filter(|&c| c >= 0).map(|c| c as i32).unwrap_or(200);
            let got = pause_samples(&pause, w, prev, &mut cont, Some(comma));
            let ok = got == e.get("samples").i64().unwrap() as i32 && cont == (e.get("cont_out").i64().unwrap() != 0);
            if !gaps.check(ok) && shown < 20 {
                shown += 1;
                eprintln!("{} gap: got {} cont {} ev {:?}", name, got, cont, e);
            }
            if got > 0 {
                expect_sil.push(silence_length(got, e.get("speed").i64().unwrap() as i32));
            }
        }
        // Silence records: the stage capture's own records when present (it
        // covers the extra texts), else the main capture's.
        let stage_recs: Vec<&Json> = ev.iter().filter(|e| e.get("kind").str() == Some("records")).collect();
        let main_recs: Vec<&Json> = main.as_ref().map(|m| m.get("records").arr().iter().collect()).unwrap_or_default();
        let groups = if stage_recs.is_empty() { main_recs } else { stage_recs };
        if groups.is_empty() {
            continue;
        }
        let spans_ev: Vec<&Json> = ev.iter().filter(|e| e.get("kind").str() == Some("spans")).collect();
        let mut got_sil = Vec::new();
        let mut trailing = Vec::new();
        for g in groups {
            let recs = g.get("records").arr();
            for (i, r) in recs.iter().enumerate() {
                if r.get("type").i64() == Some(1) {
                    if i + 1 == recs.len() {
                        trailing.push((g.get("sentence").i64().unwrap(), r.get("len").i64().unwrap() as i32,
                            recs.get(i.wrapping_sub(1)).map(|p| p.get("word").i64().unwrap())));
                    } else {
                        got_sil.push(r.get("len").i64().unwrap() as i32);
                    }
                }
            }
        }
        if !sil.check(got_sil == expect_sil) && shown < 20 {
            shown += 1;
            eprintln!("{} silences: model {:?} engine {:?}", name, expect_sil, got_sil);
        }
        for (si, len, prev_word) in trailing {
            let sp_ev = spans_ev.iter().filter(|e| e.get("sentence").i64() == Some(si)).last();
            let tr = sp_ev.map(|e| e.get("trail_out").i64().unwrap() as i32).unwrap_or(-1);
            let speed = prev_word
                .and_then(|w| sp_ev.and_then(|e| e.get("after").get("speed").at(w as usize).i64()))
                .unwrap_or(100) as i32;
            let ok = tr > 0 && silence_length(tr << 4, speed) == len;
            if !trail.check(ok) && shown < 20 {
                shown += 1;
                eprintln!("{} trailing silence: engine {} trail_ms {} speed {}", name, len, tr, speed);
            }
        }
    }
    println!("{} stage captures", caps.len());
    for (t, n) in [(&seed, "word-rate seeding"), (&spans, "span application"), (&sp, "sentence pauses"),
                   (&gaps, "pause gaps"), (&sil, "silence lengths/texts"), (&trail, "trailing silences")] {
        println!("{}", t.line(n));
    }
    assert_eq!(seed.bad + spans.bad + sp.bad + gaps.bad + sil.bad + trail.bad, 0);
}
