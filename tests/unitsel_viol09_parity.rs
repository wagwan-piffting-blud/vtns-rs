//! Unit-selection parity for the 2009 Julie build (vt_eng_julie16.dll) against the Frida captures in
//! `out_initial_viol09` / `out_unitsel_viol09`: feeds each captured sentence's word records through
//! `unitsel::Selector` (`Era::Violeta09`) and compares targets, target features, candidate lists, the
//! final beam, the path and the chosen units field by field. Mirrors `unitsel_parity.rs`.

use std::path::{Path, PathBuf};

use vtpaul::bank::Bank;
use vtpaul::era::Era;
use vtpaul::json::Json;
use vtpaul::unitsel::classes::Classes;
use vtpaul::unitsel::{CepDist, PhraseResult, ScalarDist, Selector, Word};

const ERA: Era = Era::Violeta09;

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

fn m16() -> PathBuf {
    Path::new(VOICES).join("old_julie_violeta").join("Violeta").join("data-violeta").join("M16")
}

fn out_dirs() -> Vec<String> {
    vec![format!("{}/out_initial_viol09", HARNESS), format!("{}/out_unitsel_viol09", HARNESS)]
}

#[derive(Default, Debug)]
struct Tally {
    phrases: usize,
    targets: usize,
    target_rec: usize,
    feat: usize,
    cand_lists: usize,
    cand_fields: usize,
    beams: usize,
    paths: usize,
    uids: usize,
    uid_total: usize,
    primary: usize,
}

fn words_of(sent: &Json) -> Vec<Word> {
    sent.get("words")
        .arr()
        .iter()
        .map(|w| {
            let raw = w.get("raw").hex();
            let syls: Vec<Vec<u8>> = w.get("syllables").arr().iter().map(|s| s.get("raw").hex()).collect();
            Word::from_raw_era(&raw, &syls, ERA)
        })
        .collect()
}

fn f32_eq(j: &Json, v: f32) -> bool {
    match j.f64() {
        Some(x) => (x as f32).to_bits() == v.to_bits() || (x.is_nan() && v.is_nan()),
        None => false,
    }
}

/// The selected unit's primary record as the engine's `us_ReadPrimaryRecord` loaded it.
fn primary_matches(bank: &Bank, uid: u32, pj: &Json) -> bool {
    let Ok(p) = bank.primary(uid) else { return false };
    let per = pj.get("per").arr();
    pj.get("pcm_pos").i64() == Some(p.pcm_pos as i64)
        && pj.get("pm_pos").i64() == Some(p.pm_pos as i64)
        && pj.get("h1").i64() == Some(p.half1_len as i64)
        && pj.get("h2").i64() == Some(p.half2_len as i64)
        && pj.get("coded").i64() == Some(p.coded_size as i64)
        && pj.get("pm1").i64() == Some(p.pm1 as i64)
        && pj.get("pm2").i64() == Some(p.pm2 as i64)
        && (0..3).all(|k| per.get(k).and_then(|x| x.i64()) == Some(p.periods[k] as i64))
}

fn compare(bank: &Bank, name: &str, sel: &Json, r: &PhraseResult, tally: &mut Tally, verbose: bool) -> bool {
    let tj = sel.get("targets").arr();
    let mut ok = true;
    tally.phrases += 1;
    if tj.len() != r.targets.len() {
        if verbose {
            eprintln!(
                "{} s{} p{}: {} targets, engine {}",
                name,
                sel.get("sentence").i64().unwrap_or(-1),
                sel.get("phrase").i64().unwrap_or(-1),
                r.targets.len(),
                tj.len()
            );
        }
        return false;
    }
    for (t, j) in tj.iter().enumerate() {
        tally.targets += 1;
        let g = &r.targets[t];
        let want = [
            j.get("word").i64(),
            j.get("syl").i64(),
            j.get("phone").i64(),
            j.get("syl_phone").i64(),
            j.get("half").i64(),
            j.get("unvoiced").i64(),
        ];
        let got = [g.word, g.syl, g.phone, g.idx, g.half, g.unvoiced].map(|x| Some(x as i64));
        if want != got {
            tally.target_rec += 1;
            ok = false;
            if verbose {
                eprintln!("{} t{}: target {:?} vs engine {:?}", name, t, got, want);
            }
        }
        let f = j.get("feat");
        let wf = [f.get("dur_code").i64(), f.get("f0_a").i64(), f.get("f0_b").i64()];
        if wf != r.feats[t].map(|x| Some(x as i64)) {
            tally.feat += 1;
            ok = false;
            if verbose {
                eprintln!("{} t{}: feat {:?} vs engine {:?}", name, t, r.feats[t], wf);
            }
        }
        let cj = j.get("cands").arr();
        let cg = &r.cands[t];
        let mut cand_ok = cj.len() == cg.len();
        let mut first_bad = None;
        for (k, c) in cj.iter().enumerate().take(cg.len()) {
            let g = &cg[k];
            let kept = r.kept_after_get[t].get(k).copied();
            let good = f32_eq(c.get("tc"), g.tc)
                && f32_eq(c.get("score"), g.score)
                && c.get("uid").i64() == Some(g.id as i64)
                && c.get("fwd_run").i64() == Some(g.fwd_run as i64)
                && c.get("run").i64() == Some(g.run as i64)
                && c.get("wrun").i64() == Some(g.wrun as i64)
                && c.get("kept_uid").i64() == kept.map(|x| x as i64);
            if !good {
                tally.cand_fields += 1;
                cand_ok = false;
                if first_bad.is_none() {
                    first_bad = Some(k);
                }
            }
        }
        if !cand_ok {
            tally.cand_lists += 1;
            ok = false;
            if verbose {
                if let Some(k) = first_bad {
                    let c = &cj[k];
                    eprintln!(
                        "{} t{} cand{}: ours {:?} kept {:?} | engine tc={:?} score={:?} uid={:?} fwd={:?} run={:?} wrun={:?} kept={:?}",
                        name,
                        t,
                        k,
                        cg[k],
                        r.kept_after_get[t].get(k),
                        c.get("tc").f64(),
                        c.get("score").f64(),
                        c.get("uid").i64(),
                        c.get("fwd_run").i64(),
                        c.get("run").i64(),
                        c.get("wrun").i64(),
                        c.get("kept_uid").i64()
                    );
                } else {
                    eprintln!("{} t{}: {} cands, engine {}", name, t, cg.len(), cj.len());
                }
            }
        }
        let b = j.get("beam");
        let s = &r.slots[t];
        let mut beam_ok = b.get("n").i64() == Some(s.n as i64);
        for (k, u) in b.get("uid").arr().iter().enumerate() {
            beam_ok &= u.i64() == s.kept.get(k).map(|&x| x as i64);
        }
        for (k, a) in b.get("acc").arr().iter().enumerate() {
            beam_ok &= s.acc.get(k).is_some_and(|&x| f32_eq(a, x));
        }
        for (k, bk) in b.get("back").arr().iter().enumerate() {
            beam_ok &= bk.i64() == s.back.get(k).map(|&x| x as i64);
        }
        if !beam_ok {
            tally.beams += 1;
            ok = false;
            if verbose {
                eprintln!("{} t{}: beam n={} ours n={}", name, t, b.get("n").i64().unwrap_or(-1), s.n);
            }
        }
        if j.get("path").i64() != Some(r.path[t] as i64) {
            tally.paths += 1;
            ok = false;
        }
        tally.uid_total += 1;
        if j.get("uid").i64() != Some(r.uids[t] as i64) {
            tally.uids += 1;
            ok = false;
        }
        if !primary_matches(bank, r.uids[t], j.get("primary")) {
            tally.primary += 1;
            ok = false;
        }
    }
    ok
}


impl Tally {
    fn add(&mut self, o: &Tally) {
        self.phrases += o.phrases;
        self.targets += o.targets;
        self.target_rec += o.target_rec;
        self.feat += o.feat;
        self.cand_lists += o.cand_lists;
        self.cand_fields += o.cand_fields;
        self.beams += o.beams;
        self.paths += o.paths;
        self.uids += o.uids;
        self.uid_total += o.uid_total;
        self.primary += o.primary;
    }
}

fn capture_files(dirs: &[String]) -> Vec<PathBuf> {
    let mut files: Vec<_> = dirs
        .iter()
        .filter(|d| Path::new(d.as_str()).exists())
        .flat_map(|d| std::fs::read_dir(d).unwrap().map(|e| e.unwrap().path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json") && !p.file_name().unwrap().to_string_lossy().starts_with('_'))
        .collect();
    files.sort();
    files
}

/// Runs every capture through a fresh selector; returns `(exact files, files, tally)`.
fn run(bank: &Bank, cls: &Classes, cep: &CepDist, sd: &ScalarDist, files: Vec<PathBuf>, verbose: bool) -> (usize, usize, Tally) {
    let only = std::env::var("VTPAUL_ONLY").ok();
    let files: Vec<PathBuf> = files
        .into_iter()
        .filter(|p| only.as_deref().is_none_or(|o| o == p.file_stem().unwrap().to_string_lossy()))
        .collect();
    let nthreads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(files.len().max(1));
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results = std::sync::Mutex::new((0usize, Tally::default()));
    std::thread::scope(|sc| {
        for _ in 0..nthreads {
            sc.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if i >= files.len() {
                    break;
                }
                let p = &files[i];
                let name = p.file_stem().unwrap().to_string_lossy().into_owned();
                let cap = Json::load(p).unwrap();
                let sents = cap.get("sentences").arr();
                let mut sel = Selector::new_era(bank, cls, cep, sd, ERA);
                sel.reset();
                let mut tally = Tally::default();
                let mut file_ok = true;
                let mut cur_sent = usize::MAX;
                let mut words: Vec<Word> = Vec::new();
                for s in cap.get("selections").arr() {
                    let si = s.get("sentence").i64().unwrap() as usize;
                    let pi = s.get("phrase").i64().unwrap() as usize;
                    let sj = &sents[si];
                    if si != cur_sent {
                        words = words_of(sj);
                        cur_sent = si;
                    }
                    // The Violeta capture's `phrase_words` holds each phrase's first word index (as in
                    // `unitsel_spa_parity.rs`); the count comes from consecutive starts. The build never
                    // reads the sentence type byte.
                    let starts: Vec<usize> = sj.get("phrase_words").arr().iter().map(|x| x.i64().unwrap() as usize).collect();
                    let n_words_sj = sj.get("n_words").i64().unwrap() as usize;
                    let first = starts[pi];
                    let nw = if pi + 1 < starts.len() { starts[pi + 1] - first } else { n_words_sj - first };
                    let r = sel.select_phrase(&mut words, first, nw, 4);
                    file_ok &= compare(bank, &name, s, &r, &mut tally, verbose);
                }
                if !file_ok {
                    eprintln!("MISMATCH {}", name);
                }
                let mut g = results.lock().unwrap();
                g.0 += file_ok as usize;
                g.1.add(&tally);
            });
        }
    });
    let (exact, tally) = results.into_inner().unwrap();
    (exact, files.len(), tally)
}

fn setup() -> Option<(Bank, Classes, CepDist, ScalarDist)> {
    let m = m16();
    if !m.exists() {
        eprintln!("skipping: voice data missing");
        return None;
    }
    let bank = Bank::open(&m).unwrap();
    let cls = Classes::build_era(&bank, ERA);
    let cep = CepDist::load(&m.join("ttsdata/dist_tbl/cepdist.tbl")).unwrap();
    Some((bank, cls, cep, ScalarDist::build()))
}

#[test]
fn unitsel_viol09_matches_engine() {
    let files = capture_files(&out_dirs());
    let Some((bank, cls, cep, sd)) = setup() else { return };
    if files.is_empty() {
        eprintln!("skipping: captures missing");
        return;
    }
    let verbose = std::env::var("VTPAUL_VERBOSE").is_ok();
    let (exact, n, tally) = run(&bank, &cls, &cep, &sd, files, verbose);
    eprintln!("unitsel viol09 parity: {}/{} captures exact; {:?}", exact, n, tally);
    assert_eq!(exact, n);
}
