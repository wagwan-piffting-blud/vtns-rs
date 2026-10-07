//! `engine09` (front end + prosody glue of the 2009 English builds) against the captures: the captured
//! per-sentence WA (`out_engine_<build>`) through `run_ling09` / `run_prosody09`, compared with the word
//! records the engine itself dumped after the context builder (`ctx_out`) and with the durations, tones and
//! smoothed F0 of the main capture (`out_<build>`).
//!
//!     cargo test --release --test engine09_parity -- --nocapture

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use vtpaul::engine::capture;
use vtpaul::engine09;
use vtpaul::era::Era;
use vtpaul::json::Json;
use vtpaul::ling::lay::Lay;
use vtpaul::ling::Ss;
use vtpaul::lexicon::Lexicon;
use vtpaul::prosody::Trees;
use vtpaul::unitsel::Word;

const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};
const VOICES: &str = match option_env!("VTPAUL_VOICES") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};

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

#[derive(Default)]
struct Tally {
    ok: usize,
    total: usize,
}

impl Tally {
    fn check(&mut self, ok: bool) -> bool {
        self.total += 1;
        self.ok += ok as usize;
        ok
    }

    fn assert_all(&self, name: &str) {
        println!("{:14} {:6}/{:6}", name, self.ok, self.total);
        assert!(self.total > 0 && self.ok == self.total, "{}: {} of {} differ", name, self.total - self.ok, self.total);
    }
}

fn load_offset_maps(doc: &Json) -> (Vec<i32>, Vec<i32>) {
    for e in doc.get("events").arr() {
        if e.get("kind").str() == Some("offset_maps") {
            let get = |name: &str| -> Vec<i32> { e.get(name).arr().iter().map(|v| v.i64().unwrap_or(0) as i32).collect() };
            return (get("map_b"), get("map_c"));
        }
    }
    (Vec::new(), Vec::new())
}

fn run(era: Era, root: &str, voice: &str, out_engine: &str, out_main: &str) {
    let base = Path::new(VOICES).join(root);
    let dict = base.join("data-common").join("dict-eng");
    let m16 = base.join(format!("data-{}", voice)).join("M16").join("ttsdata");
    let caps = captures(out_engine);
    if caps.is_empty() || !dict.exists() {
        eprintln!("skipping: captures or voice data missing");
        return;
    }
    let lex = Lexicon::open_era(&dict, era).unwrap();
    let trees = if era == Era::Paul09 { Trees::load_tree2(&m16.join("tree")) } else { Trees::load(&m16.join("tree3")) }.unwrap();
    let lay = Lay::of(era);
    let (mut ling, mut dur, mut tone, mut f0, mut uw) =
        (Tally::default(), Tally::default(), Tally::default(), Tally::default(), Tally::default());
    for path in &caps {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let doc = Json::load(path).unwrap();
        let main = Json::load(&Path::new(HARNESS).join(out_main).join(format!("{}.json", name))).ok();
        let (map_start, map_end) = load_offset_maps(&doc);
        let mut by_kind: Vec<BTreeMap<String, Json>> = Vec::new();
        for e in doc.get("events").arr() {
            let k = e.get("kind").str().unwrap_or("").to_string();
            if k == "wa_in" {
                by_kind.push(BTreeMap::new());
            }
            if let Some(cur) = by_kind.last_mut() {
                cur.insert(k, e.clone());
            }
        }
        let mut cursor = 0i32;
        let mut si = 0usize;
        for s in &by_kind {
            let (Some(wa_in), Some(words), Some(ctx)) = (s.get("wa_in"), s.get("words_out"), s.get("ctx_out")) else { continue };
            let mut input = capture::sentence_input(wa_in, words);
            input.map_start = map_start.clone();
            input.map_end = map_end.clone();
            let Some((wa, mut ss, _tib)) = engine09::run_ling09(era, &lex, &input, cursor) else { continue };
            cursor += wa.consumed();
            for (i, w) in ctx.get("words").arr().iter().enumerate() {
                let want = w.get("raw").hex();
                let o = lay.word(i);
                let mut ok = true;
                for (k, (a, b)) in ss.b[o..o + lay.stride].iter().zip(want.iter()).enumerate() {
                    let masked = [0usize, 4, 0x8c, 0x218, 0x21c, 0x2f0].iter().any(|&p| (p..p + 4).contains(&k));
                    // Stale per-phone data past the word's phone count is not defined.
                    let nph = want[0x2e4] as usize;
                    let stale = (0x0a..0x8a).contains(&k)
                        || [(0x90 + 5 * nph, 0x90 + 5 * 0x41), (0x1d5 + nph, 0x1d5 + 0x42), (0x220 + nph, 0x261), (0x261 + nph, 0x2a2), (0x2a2 + nph, 0x2e3)]
                            .iter()
                            .any(|&(a, b)| (a..b).contains(&k));
                    if !masked && a != b && !stale {
                        if ok {
                            eprintln!("{} w{}+{:x}: {:02x} want {:02x}", name, i, k, a, b);
                        }
                        ok = false;
                    }
                }
                ling.check(ok);
            }
            engine09::run_prosody09(era, &trees, &mut ss);
            let uwords = engine09::ss_to_unitsel_words09(era, &ss);
            let Some(sent) = main.as_ref().and_then(|m| m.get("sentences").arr().get(si)) else {
                si += 1;
                continue;
            };
            si += 1;
            for (wi, w) in sent.get("words").arr().iter().enumerate() {
                let base_w = lay.word(wi);
                for (k, p) in w.get("phones").arr().iter().enumerate() {
                    dur.check(ss.i16(base_w + engine09::WR_DUR09 + 2 * k) as i64 == p.get("dur").i64().unwrap());
                    let want: Vec<u8> = w.get("raw").hex()[0x90 + 5 * k..0x95 + 5 * k].to_vec();
                    uw.check(uwords[wi].ctx5[k][..] == want[..]);
                }
                let syl_base = ss.deref(ss.u32(base_w + lay.syl_ptr)).unwrap_or(0);
                for (yi, y) in w.get("syllables").arr().iter().enumerate() {
                    let o = syl_base + yi * vtpaul::ling::SYL_SIZE;
                    tone.check(ss.u8(o + vtpaul::ling::syl::TONE) as i8 as i64 == y.get("tone").i64().unwrap());
                    let want: Vec<i16> = y.get("f0").arr().iter().map(|v| v.i64().unwrap() as i16).collect();
                    let got: Vec<i16> = (0..12).map(|j| ss.i16(o + vtpaul::ling::syl::F0 + 2 * j)).collect();
                    f0.check(got == want);
                }
            }
        }
    }
    let _: Option<&Ss> = None;
    let _: Option<&Word> = None;
    ling.assert_all("ling words");
    dur.assert_all("durations");
    tone.assert_all("tones");
    f0.assert_all("f0");
    uw.assert_all("unitsel ctx5");
}

#[test]
fn engine09_julie09() {
    run(Era::Julie09, "old_julie_violeta/Julie", "julie", "out_engine_julie09", "out_julie09");
}

#[test]
fn engine09_paul09() {
    run(Era::Paul09, "old_paul_install/Paul", "paul", "out_engine_paul09", "out_paul09");
}
