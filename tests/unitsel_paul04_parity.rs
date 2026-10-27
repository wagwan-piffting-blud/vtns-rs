//! Unit-selection parity for the 2004 Paul build (vt_eng.dll, PE 2005-03-31) against the Frida captures
//! in `out_unitsel_paul04` (`hooks_unitsel_paul04.js`): feeds each captured sentence's word records
//! through `unitsel04::Selector04` phrase by phrase and compares targets, features, class lists,
//! candidate lists, the final beam, the path and the DB entries field by field.

use std::path::{Path, PathBuf};

use vtpaul::bank::Bank;
use vtpaul::json::Json;
use vtpaul::unitsel::{CepDist, ScalarDist};
use vtpaul::unitsel04::{Cls04, PhraseResult, Selector04, Word04};

const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};
const VOICES: &str = match option_env!("VTPAUL_VOICES") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};

fn m16() -> PathBuf {
    Path::new(VOICES).join("2004_paul").join("data-paul").join("M16")
}

#[derive(Default, Debug)]
struct Tally {
    phrases: usize,
    targets: usize,
    target_rec: usize,
    feat: usize,
    classes: usize,
    n_expanded: usize,
    expanded: usize,
    cand_lists: usize,
    cand_fields: usize,
    f_tc: usize,
    f_score: usize,
    f_uid: usize,
    f_runs: usize,
    f_kept: usize,
    beams: usize,
    paths: usize,
    uids: usize,
    db: usize,
}

impl Tally {
    fn add(&mut self, o: &Tally) {
        self.phrases += o.phrases;
        self.targets += o.targets;
        self.target_rec += o.target_rec;
        self.feat += o.feat;
        self.classes += o.classes;
        self.n_expanded += o.n_expanded;
        self.expanded += o.expanded;
        self.cand_lists += o.cand_lists;
        self.cand_fields += o.cand_fields;
        self.f_tc += o.f_tc;
        self.f_score += o.f_score;
        self.f_uid += o.f_uid;
        self.f_runs += o.f_runs;
        self.f_kept += o.f_kept;
        self.beams += o.beams;
        self.paths += o.paths;
        self.uids += o.uids;
        self.db += o.db;
    }
}

fn words_of(sent: &Json) -> Vec<Word04> {
    sent.get("words")
        .arr()
        .iter()
        .map(|w| {
            let raw = w.get("raw").hex();
            let syls: Vec<Vec<u8>> = w.get("syllables").arr().iter().map(|s| s.hex()).collect();
            Word04::from_raw(&raw, &syls)
        })
        .collect()
}

fn f32_eq(j: &Json, v: f32) -> bool {
    match j.f64() {
        Some(x) => (x as f32).to_bits() == v.to_bits() || (x.is_nan() && v.is_nan()),
        None => false,
    }
}

fn compare(name: &str, sel: &Json, r: &PhraseResult, tally: &mut Tally, verbose: bool) -> bool {
    let tj = sel.get("targets").arr();
    let mut ok = true;
    tally.phrases += 1;
    let tag = format!("{} s{} p{}", name, sel.get("sentence").i64().unwrap_or(-1), sel.get("phrase").i64().unwrap_or(-1));
    if tj.len() != r.targets.len() {
        if verbose {
            eprintln!("{}: {} targets, engine {}", tag, r.targets.len(), tj.len());
        }
        tally.target_rec += 1;
        return false;
    }
    for (t, j) in tj.iter().enumerate() {
        tally.targets += 1;
        let g = &r.targets[t];
        let want = [
            j.get("word").i64(),
            j.get("syl").i64(),
            j.get("phone").i64(),
            j.get("idx").i64(),
            j.get("half").i64(),
            j.get("unvoiced").i64(),
        ];
        let got = [g.word, g.syl, g.phone, g.idx, g.half, g.unvoiced].map(|x| Some(x as i64));
        if want != got {
            tally.target_rec += 1;
            ok = false;
            if verbose {
                eprintln!("{} t{}: target {:?} vs engine {:?}", tag, t, got, want);
            }
        }
        let f = j.get("feat");
        let wf = [f.get("dur_code").i64(), f.get("f0_a").i64(), f.get("f0_b").i64()];
        if wf != r.feats[t].map(|x| Some(x as i64)) {
            tally.feat += 1;
            ok = false;
            if verbose {
                eprintln!("{} t{}: feat {:?} vs engine {:?}", tag, t, r.feats[t], wf);
            }
        }
        let cj: Vec<i64> = j.get("classes").arr().iter().map(|x| x.i64().unwrap()).collect();
        if cj.iter().map(|&x| x as i32).collect::<Vec<_>>() != r.classes[t] {
            tally.classes += 1;
            ok = false;
            if verbose {
                eprintln!("{} t{}: classes {:?} vs engine {:?}", tag, t, r.classes[t], cj);
            }
        }
        if j.get("n_expanded").i64() != Some(r.n_expanded[t] as i64) {
            tally.n_expanded += 1;
            ok = false;
            if verbose {
                eprintln!("{} t{}: n_expanded {} vs engine {:?}", tag, t, r.n_expanded[t], j.get("n_expanded").i64());
            }
        }
        let ex = j.get("expanded").arr();
        let mut ex_ok = ex.len() == r.expanded[t].len();
        for (k, e) in ex.iter().enumerate() {
            if let Some(g) = r.expanded[t].get(k) {
                let a = e.arr();
                ex_ok &= a[0].i64() == Some(g.0 as i64)
                    && a[1].i64() == Some(g.1 as i64)
                    && a[2].i64() == Some(g.2 as i64)
                    && a[3].i64() == Some(g.3 as i64);
            }
        }
        if !ex_ok {
            tally.expanded += 1;
            ok = false;
        }
        let cands = j.get("cands").arr();
        let cg = &r.cands[t];
        let mut cand_ok = cands.len() == cg.len();
        for (k, c) in cands.iter().enumerate().take(cg.len()) {
            let (g, kept) = &cg[k];
            let mut bad = false;
            if !f32_eq(c.get("tc"), g.tc) {
                tally.f_tc += 1;
                bad = true;
            }
            if !f32_eq(c.get("score"), g.score) {
                tally.f_score += 1;
                bad = true;
            }
            if c.get("uid").i64() != Some(g.id as i64) {
                tally.f_uid += 1;
                bad = true;
            }
            if c.get("back").i64() != Some(g.back as i64)
                || c.get("fwd").i64() != Some(g.fwd as i64)
                || c.get("run").i64() != Some(g.run as i64)
            {
                tally.f_runs += 1;
                bad = true;
            }
            if c.get("kept_uid").i64() != Some(*kept as i64) {
                tally.f_kept += 1;
                bad = true;
            }
            if bad {
                tally.cand_fields += 1;
                cand_ok = false;
                if verbose && tally.cand_fields < 20 {
                    eprintln!(
                        "{} t{} cand{}: ours {:?} | engine tc={:?} score={:?} uid={:?} b/f/r={:?}/{:?}/{:?}",
                        tag,
                        t,
                        k,
                        g,
                        c.get("tc").f64(),
                        c.get("score").f64(),
                        c.get("uid").i64(),
                        c.get("back").i64(),
                        c.get("fwd").i64(),
                        c.get("run").i64()
                    );
                }
            }
        }
        if !cand_ok {
            tally.cand_lists += 1;
            ok = false;
        }
        let b = j.get("beam");
        let mut beam_ok = b.get("n").i64() == Some(r.kept[t].len() as i64);
        for (k, u) in b.get("uid").arr().iter().enumerate() {
            beam_ok &= u.i64() == r.kept[t].get(k).map(|&x| x as i64);
        }
        for (k, a) in b.get("acc").arr().iter().enumerate() {
            beam_ok &= r.acc[t].get(k).is_some_and(|&x| f32_eq(a, x));
        }
        for (k, bk) in b.get("back").arr().iter().enumerate() {
            beam_ok &= bk.i64() == r.back[t].get(k).map(|&x| x as i64);
        }
        if !beam_ok {
            tally.beams += 1;
            ok = false;
            if verbose {
                eprintln!("{} t{}: beam n={} ours n={}", tag, t, b.get("n").i64().unwrap_or(-1), r.kept[t].len());
            }
        }
        if j.get("path").i64() != Some(r.path[t] as i64) {
            tally.paths += 1;
            ok = false;
        }
        if j.get("uid").i64() != Some(r.uids[t] as i64) {
            tally.uids += 1;
            ok = false;
        }
        let dbhex: String = r.db[t].iter().map(|b| format!("{:02x}", b)).collect();
        if j.get("db").str() != Some(dbhex.as_str()) {
            tally.db += 1;
            ok = false;
            if verbose {
                eprintln!("{} t{}: db {} vs engine {:?}", tag, t, dbhex, j.get("db").str());
            }
        }
    }
    ok
}

fn capture_files(dir: &str) -> Vec<PathBuf> {
    if !Path::new(dir).exists() {
        return Vec::new();
    }
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files
}

#[test]
fn unitsel_paul04_matches_engine() {
    let m = m16();
    if !m.exists() {
        eprintln!("skipping: voice data missing");
        return;
    }
    let files = capture_files(&format!("{}/out_unitsel_paul04", HARNESS));
    if files.is_empty() {
        eprintln!("skipping: captures missing");
        return;
    }
    let only = std::env::var("VTPAUL_ONLY").ok();
    let limit: usize = std::env::var("VTPAUL_LIMIT").ok().and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
    let files: Vec<PathBuf> = files
        .into_iter()
        .filter(|p| only.as_deref().is_none_or(|o| o == p.file_stem().unwrap().to_string_lossy()))
        .take(limit)
        .collect();
    let verbose = std::env::var("VTPAUL_VERBOSE").is_ok();
    let bank = Bank::open(&m).unwrap();
    let cls = Cls04::build(&bank);
    let cep = CepDist::load(&m.join("ttsdata/dist_tbl/cepdist.tbl")).unwrap();
    let sd = ScalarDist::build();
    let nthreads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(files.len().max(1)).min(16);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results = std::sync::Mutex::new((0usize, Tally::default()));
    std::thread::scope(|sc| {
        for _ in 0..nthreads {
            sc.spawn(|| {
                let mut sel = Selector04::new(&bank, &cls, &cep, &sd);
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if i >= files.len() {
                        break;
                    }
                    let p = &files[i];
                    let name = p.file_stem().unwrap().to_string_lossy().into_owned();
                    let cap = Json::load(p).unwrap();
                    let sents = cap.get("sentences").arr();
                    let mut tally = Tally::default();
                    let mut file_ok = true;
                    let mut cur_sent = usize::MAX;
                    let mut words: Vec<Word04> = Vec::new();
                    for s in cap.get("selections").arr() {
                        let si = s.get("sentence").i64().unwrap() as usize;
                        let pi = s.get("phrase").i64().unwrap() as usize;
                        let sj = sents.iter().find(|x| x.get("sentence").i64() == Some(si as i64)).unwrap();
                        if si != cur_sent {
                            words = words_of(sj);
                            cur_sent = si;
                        }
                        let ph = &sj.get("phrases").arr()[pi];
                        let first = ph.get("first").i64().unwrap() as usize;
                        let n = ph.get("n").i64().unwrap() as usize;
                        let r = sel.select_phrase(&mut words, first, n);
                        file_ok &= compare(&name, s, &r, &mut tally, verbose);
                    }
                    if !file_ok {
                        eprintln!("MISMATCH {}", name);
                    }
                    let mut g = results.lock().unwrap();
                    g.0 += file_ok as usize;
                    g.1.add(&tally);
                }
            });
        }
    });
    let (exact, tally) = results.into_inner().unwrap();
    eprintln!("unitsel paul04 parity: {}/{} captures exact; {:?}", exact, files.len(), tally);
    assert_eq!(exact, files.len());
}
