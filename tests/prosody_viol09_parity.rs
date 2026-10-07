//! Violeta (Spanish) prosody parity against captures of the original engine:
//! `out_initial_viol09/*.json` (main) and `out_prosody_viol09/*.json` (stage hooks).
//! Mirrors `tests/prosody_parity.rs`'s structure and checks.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use vtpaul::era::Era;
use vtpaul::json::Json;
use vtpaul::prosody::{
    apply_tag_spans_era, resolve_sentence_pause, silence_length, smooth_f0, target_feature_era,
    Sentence, SpanContext, SpanList, Syllable, TargetRef, Word,
};
use vtpaul::prosody_spa::*;

/// Capture root; override at build time with `VTPAUL_HARNESS=<dir>`.
const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};
/// Voice data root; override at build time with `VTPAUL_VOICES=<dir>`.
const VOICES: &str = match option_env!("VTPAUL_VOICES") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};

fn tree3_spa() -> PathBuf {
    Path::new(VOICES).join("old_julie_violeta").join("Violeta").join("data-violeta").join("M16").join("ttsdata").join("tree3")
}

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

/// Builds a `Sentence` (reused from the English port: `codes`/`pos_code`/`syllables`/
/// `brk`/`dur` mean the same thing in both languages) plus the per-word context
/// array `duration_features_spa` needs, from one `out_initial_viol09` sentence record.
/// `codes`/`pos_code` come from `word.raw` at the fixed offsets confirmed by
/// decompiling the Violeta syllabifier (FUN_1001e1e0): phone codes at word+0x408,
/// pos-codes (onset/nucleus/coda) at word+0x4d0, both flat across the whole word.
/// `first_phone` isn't captured (Violeta stores it as a per-syllable pointer, not
/// an index -- see VIOLETA_TODO.md's batch 1 notes) so it's rebuilt here as a
/// running sum of `n_phones`, exactly mirroring what the pointers encode.
fn sentence_from_spa(js: &Json) -> (Sentence, Vec<Vec<[u8; 7]>>) {
    let mut s = Sentence { era: Era::Violeta09, ..Default::default() };
    let mut ctxs = Vec::new();
    for w in js.get("words").arr() {
        let raw = w.get("raw").hex();
        let phones = w.get("phones").arr();
        let n = phones.len();
        let codes: Vec<u8> = raw[0x338..0x338 + n].to_vec();
        let pos_code: Vec<i8> = raw[0x400..0x400 + n].iter().map(|&b| b as i8).collect();
        let ctx: Vec<[u8; 7]> = phones
            .iter()
            .map(|p| {
                let h = p.get("ctx").hex();
                [h[0], h[1], h[2], h[3], h[4], 0, 0]
            })
            .collect();
        let mut first_phone = 0u8;
        let syllables = w
            .get("syllables")
            .arr()
            .iter()
            .map(|y| {
                let np = y.get("n_phones").i64().unwrap() as u8;
                let syl = Syllable {
                    b0: y.get("b0").i64().unwrap() as i8,
                    b1: y.get("b1").i64().unwrap() as i8,
                    b2: y.get("b2").i64().unwrap() as i8,
                    tone: 0,
                    f0: [0; 12],
                    first_phone,
                    n_phones: np,
                };
                first_phone += np;
                syl
            })
            .collect();
        let brk = w.get("brk").i64().unwrap() as u8;
        s.words.push(Word { codes, pos_code, syllables, brk, dur: vec![0; n] });
        ctxs.push(ctx);
    }
    // `hooks_spa.js`'s `phrase_words` field reads ss+0xc+16g, which for Violeta
    // holds each phrase's *starting word index* (confirmed against `records[]`,
    // which enumerates every word touched per phrase), not a word count like
    // English's ss+0xe+16g "words in phrase g" the field name suggests. Convert
    // to counts here (a read-only capture bug, not something to recapture for).
    let starts: Vec<usize> = js.get("phrase_words").arr().iter().map(|x| x.i64().unwrap() as usize).collect();
    let n_words = s.words.len();
    s.phrase_words = starts
        .iter()
        .enumerate()
        .map(|(g, &st)| starts.get(g + 1).copied().unwrap_or(n_words) - st)
        .collect();
    (s, ctxs)
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
fn prosody_matches_engine_spa() {
    let caps = captures("out_initial_viol09");
    if caps.is_empty() || !tree3_spa().exists() {
        eprintln!("skipping: captures or Violeta voice data missing");
        return;
    }
    let trees = TreesSpa::load(&tree3_spa()).unwrap();
    let (mut dur, mut tone, mut raw, mut smooth, mut feat) =
        (Tally::default(), Tally::default(), Tally::default(), Tally::default(), Tally::default());
    let mut shown = 0;
    for path in &caps {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let cap = Json::load(path).unwrap();
        let pro = Json::load(&Path::new(HARNESS).join("out_prosody_viol09").join(format!("{}.json", name))).ok();
        let mut sents = Vec::new();
        for (si, js) in cap.get("sentences").arr().iter().enumerate() {
            let (mut s, ctx) = sentence_from_spa(js);
            predict_durations_spa(&trees, &mut s, &ctx);
            for (wi, w) in js.get("words").arr().iter().enumerate() {
                for (k, p) in w.get("phones").arr().iter().enumerate() {
                    let want = p.get("dur").i64().unwrap() as i16;
                    if !dur.check(s.words[wi].dur[k] == want) && shown < 20 {
                        shown += 1;
                        eprintln!("{} s{} w{} p{}: dur {} want {}", name, si, wi, k, s.words[wi].dur[k], want);
                    }
                }
            }
            predict_pitch_raw_spa(&trees, &mut s);
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
                let got = target_feature_era(Era::Violeta09, &s.words[t.word as usize], &wl[t.word as usize], &t);
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

fn rates_from(j: &Json) -> vtpaul::prosody::WordRates {
    vtpaul::prosody::WordRates {
        pitch: ints(j.get("pitch")),
        speed: ints(j.get("speed")),
        volume: ints(j.get("volume")),
        pause: ints(j.get("pause")),
        brk: ints(j.get("brk")),
        k7: ints(j.get("k7")),
    }
}

#[test]
fn rates_and_pauses_match_engine_spa() {
    let caps = captures("out_prosody_viol09");
    if caps.is_empty() {
        eprintln!("skipping: out_prosody_viol09 captures missing");
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
        // `hooks_prosody_spa.js`'s "spans" event only captures each word's `brk`
        // (its own comment: word start/end "are omitted here" for Violeta); a
        // follow-up capture (`harness\hooks_spans_spa.js` /
        // `capture_spans_spa.py`, output `out_spans_viol09\`) re-ran the 6 texts
        // whose span lists are actually non-empty with start/end added, WAV
        // SHA1-verified unchanged against `out_prosody_viol09`. Matched by position
        // among that file's own "spans" events (each of the 6 has exactly one).
        let extra = Json::load(&Path::new(HARNESS).join("out_spans_viol09").join(format!("{}.json", name))).ok();
        let extra_spans: Vec<&Json> = extra
            .as_ref()
            .map(|e| e.get("events").arr().iter().filter(|e| e.get("kind").str() == Some("spans")).collect())
            .unwrap_or_default();
        for (ei, e) in ev.iter().filter(|e| e.get("kind").str() == Some("spans")).enumerate() {
            let before = rates_from(e.get("before"));
            let n = before.pitch.len();
            // out_prosody_viol09 has no separate base_pitch/base_speed/base_volume
            // fields (unlike English's out_prosody): every word's seeded rate is
            // uniform, so the base is `before.{pitch,speed,volume}[0]`.
            let seeded = seed_word_rates_spa(
                n,
                *before.pitch.first().unwrap_or(&100),
                *before.speed.first().unwrap_or(&100),
                *before.volume.first().unwrap_or(&100),
            );
            // k7 excluded: `hooks_prosody_spa.js` reads it from a narrower buffer
            // than the other five per-word arrays (see `seed_word_rates_spa`'s
            // doc), so captures only show a reliable 0 within roughly the first
            // 16 words of a sentence and -1 (stale/out-of-buffer) beyond that.
            let seed_ok = seeded.pitch == before.pitch
                && seeded.speed == before.speed
                && seeded.volume == before.volume
                && seeded.pause == before.pause
                && seeded.brk == before.brk;
            seed.check(seed_ok);
            // Use the follow-up capture's word start/end when the main one lacks
            // them; skip only if neither has it (shouldn't happen for any capture
            // that actually needs bucketing -- see the doc comment above).
            let words_src = if e.get("words").arr().iter().any(|w| w.get("start").is_null()) {
                match extra_spans.get(ei) {
                    Some(&x) => x.get("words"),
                    None => continue,
                }
            } else {
                e.get("words")
            };
            let words: Vec<(i32, i32)> = words_src
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
            apply_tag_spans_era(Era::Violeta09, &ctx, &mut lists, &mut r, &mut tr);
            let want = rates_from(e.get("after"));
            let want_curs = ints(e.get("cursors_out"));
            let curs: Vec<i32> = lists.iter().take(want_curs.len()).map(|l| l.cursor).collect();
            let ok = r == want && tr == e.get("trail_out").i64().unwrap() as i32 && curs == want_curs;
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
        let main = Json::load(&Path::new(HARNESS).join("out_initial_viol09").join(format!("{}.json", name))).ok();
        let mut expect_sil = Vec::new();
        for e in ev.iter().filter(|e| e.get("kind").str() == Some("gap")) {
            let w = e.get("w").i64().unwrap() as usize;
            let mut pause = vec![-1i32; w + 1];
            pause[w] = e.get("pw").i64().unwrap() as i32;
            let mut cont = e.get("cont").i64().unwrap() != 0;
            let prev = e.get("prev_brk").i64().unwrap_or(0) as u8;
            let comma = pro.get("params").at(4).i64().filter(|&c| c >= 0).map(|c| c as i32).unwrap_or(200);
            let got = pause_samples_spa(&pause, w, prev, &mut cont, Some(comma));
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
