//! VTML-rate stage of the 2004 build against `out_vtml_paul04/*.json` (hooks_vtml_paul04.js): word-rate
//! seeding (`FUN_1001a410`), span application (`FUN_1001a130`) and the sentence-pause resolution.

use std::path::{Path, PathBuf};

use vtpaul::json::Json;
use vtpaul::prosody::{apply_tag_spans04, resolve_sentence_pause04, seed_rates04, SpanList, WordRates};

const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};
const OUT: &str = "out_vtml_paul04";

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
        format!("{:<26} {:>7} / {:>7} match", what, self.total - self.bad, self.total)
    }
}

fn ints(j: &Json) -> Vec<i32> {
    j.arr().iter().map(|v| v.i64().unwrap() as i32).collect()
}

fn captures() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = match std::fs::read_dir(Path::new(HARNESS).join(OUT)) {
        Ok(rd) => rd.map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect(),
        Err(_) => Vec::new(),
    };
    v.sort();
    v
}

fn rates_from(j: &Json) -> WordRates {
    WordRates {
        pitch: ints(j.get("pitch")),
        speed: ints(j.get("speed")),
        volume: ints(j.get("volume")),
        pause: ints(j.get("pause")),
        brk: ints(j.get("brk")),
        k7: Vec::new(),
    }
}

#[test]
fn rates_and_pauses_match_engine() {
    let caps = captures();
    if caps.is_empty() {
        eprintln!("skipping: no out_vtml_paul04 captures");
        return;
    }
    let (mut seed, mut seeded, mut spans, mut sp) = (Tally::default(), Tally::default(), Tally::default(), Tally::default());
    let (mut with_lists, mut shown) = (0usize, 0);
    for path in &caps {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let cap = Json::load(path).unwrap();
        let ev = cap.get("events").arr();
        let mut last_seed: Option<[i32; 3]> = None;
        let mut last_np: Option<&Json> = None;
        for e in ev {
            match e.get("kind").str().unwrap_or("") {
                "seed" => {
                    let flags = ints(e.get("flags"));
                    let (vtml, chan) = (ints(e.get("vtml")), ints(e.get("chan")));
                    let got = seed_rates04([flags[0] == 1, flags[1] == 1, flags[2] == 1], [vtml[0], vtml[1], vtml[2]], [chan[0], chan[1], chan[2]]);
                    let (out, out2) = (ints(e.get("out")), ints(e.get("out2")));
                    let ok = got[..] == out[..] && got[..] == out2[..];
                    if !seed.check(ok) && shown < 20 {
                        shown += 1;
                        eprintln!("{} seed: got {:?} out {:?} {:?} ev {:?}", name, got, out, out2, e);
                    }
                    last_seed = Some(got);
                }
                "spans" => {
                    let before = rates_from(e.get("before"));
                    let n = before.pitch.len();
                    if let Some(s) = last_seed {
                        let want = WordRates { pitch: vec![s[0]; n], speed: vec![s[1]; n], volume: vec![s[2]; n], pause: vec![-1; n], brk: vec![-1; n], k7: Vec::new() };
                        if !seeded.check(before == want) && shown < 20 {
                            shown += 1;
                            eprintln!("{} seeded arrays: {:?} want {:?}", name, before, want);
                        }
                    }
                    let words: Vec<(i32, i32)> = e.get("words").arr().iter().map(|w| (w.at(0).i64().unwrap() as i32, w.at(1).i64().unwrap() as i32)).collect();
                    let mut lists: [SpanList; 5] = Default::default();
                    for (k, l) in e.get("lists").arr().iter().enumerate() {
                        lists[k] = SpanList { pos: ints(l.get("pos")), val: ints(l.get("val")), cursor: 0 };
                    }
                    if lists.iter().any(|l| !l.pos.is_empty()) {
                        with_lists += 1;
                    }
                    let mut r = before.clone();
                    apply_tag_spans04(&words, e.get("cursor").i64().unwrap() as i32, &lists, &mut r);
                    let want = rates_from(e.get("after"));
                    if !spans.check(r == want) && shown < 20 {
                        shown += 1;
                        eprintln!("{} spans: got {:?}\n  want {:?}\n  lists {:?} words {:?}", name, r, want, e.get("lists"), words);
                    }
                }
                "np_in" => last_np = Some(e),
                "np_out" => {
                    let i = last_np.expect("np_in before np_out");
                    let got = resolve_sentence_pause04(
                        i.get("p0_in").i64().unwrap() as i32,
                        i.get("call_pause").i64().unwrap() as i32,
                        i.get("tib_pause").i64().unwrap() as i32,
                        i.get("ch_pause").i64().unwrap() as i32,
                    );
                    if !sp.check(got == e.get("p0_out").i64().unwrap() as i32) && shown < 20 {
                        shown += 1;
                        eprintln!("{} sentpause: got {} in {:?} out {:?}", name, got, i, e);
                    }
                }
                _ => {}
            }
        }
    }
    println!("{} captures, {} span events with lists", caps.len(), with_lists);
    for (t, n) in [(&seed, "rate seeds"), (&seeded, "seeded word arrays"), (&spans, "span application"), (&sp, "sentence pauses")] {
        println!("{}", t.line(n));
    }
    assert_eq!(seed.bad + seeded.bad + spans.bad + sp.bad, 0);
}
