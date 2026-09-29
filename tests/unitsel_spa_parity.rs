//! Unit-selection parity for Violeta (vt_spa_violeta16.dll) against the
//! Frida captures in `out_initial_spa` / `out_unitsel_spa`: feeds each
//! captured sentence's word records through `unitsel::Selector` (`Lang::Spa`)
//! and compares targets, target features, candidate lists, the final beam,
//! the path and the chosen units field by field. Mirrors `unitsel_parity.rs`.

use std::path::{Path, PathBuf};

use vtpaul::bank::Bank;
use vtpaul::json::Json;
use vtpaul::lang::Lang;
use vtpaul::unitsel::classes::Classes;
use vtpaul::unitsel::{CepDist, PhraseResult, ScalarDist, Selector, Word};

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
    Path::new(VOICES).join("data-violeta").join("M16")
}

fn out_dirs() -> Vec<&'static str> {
    vec![
        Box::leak(format!("{}/out_initial_spa", HARNESS).into_boxed_str()),
        Box::leak(format!("{}/out_unitsel_spa", HARNESS).into_boxed_str()),
    ]
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
}

fn words_of(sent: &Json) -> Vec<Word> {
    sent.get("words")
        .arr()
        .iter()
        .map(|w| {
            let raw = w.get("raw").hex();
            let syls: Vec<Vec<u8>> = w.get("syllables").arr().iter().map(|s| s.get("raw").hex()).collect();
            Word::from_raw_lang(&raw, &syls, Lang::Spa)
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
                && c.get("pref").i64() == Some(g.pref as i64)
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
                        "{} t{} cand{}: ours {:?} kept {:?} | engine tc={:?} score={:?} uid={:?} fwd={:?} run={:?} wrun={:?} pref={:?} kept={:?}",
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
                        c.get("pref").i64(),
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
    }
    ok
}

#[test]
fn unitsel_matches_engine_spa() {
    let dirs = out_dirs();
    if !dirs.iter().any(|d| Path::new(d).exists()) || !m16().exists() {
        eprintln!("skipping: captures or Violeta voice data missing");
        return;
    }
    let verbose = std::env::var("VTPAUL_VERBOSE").is_ok();
    let only = std::env::var("VTPAUL_ONLY").ok();
    let bank = Bank::open(&m16()).unwrap();
    let cls = Classes::build_lang(&bank, Lang::Spa);
    let cep = CepDist::load(&m16().join("ttsdata").join("dist_tbl").join("cepdist.tbl.rd")).unwrap();
    let sd = ScalarDist::build();
    let mut tally = Tally::default();
    let mut files: Vec<_> = dirs
        .iter()
        .map(Path::new)
        .filter(|d| d.exists())
        .flat_map(|d| std::fs::read_dir(d).unwrap().map(|e| e.unwrap().path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json") && !p.file_name().unwrap().to_string_lossy().starts_with('_'))
        .collect();
    files.sort();
    let mut exact_files = 0;
    let mut n_files = 0;
    for p in files {
        let name = p.file_stem().unwrap().to_string_lossy().into_owned();
        if only.as_deref().is_some_and(|o| o != name) {
            continue;
        }
        n_files += 1;
        let cap = Json::load(&p).unwrap();
        let sents = cap.get("sentences").arr();
        let mut sel = Selector::new_lang(&bank, &cls, &cep, &sd, Lang::Spa);
        sel.reset();
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
            // `hooks_spa.js` reads Violeta's phrase-range field at ss+0xc, which
            // (cross-checked against `out_wave_spa`'s `phrase_ranges: [start,
            // count]` pairs, captured at ss+0xc/+0xe) is the phrase's *starting*
            // word index, not a word count as it is in English's capture (ss+0xe
            // there). Derive the count from consecutive starts instead of
            // summing, so this works without touching harness\.
            let starts: Vec<usize> = sj.get("phrase_words").arr().iter().map(|x| x.i64().unwrap() as usize).collect();
            let n_words_sj = sj.get("n_words").i64().unwrap() as usize;
            let first = starts[pi];
            let nw = if pi + 1 < starts.len() { starts[pi + 1] - first } else { n_words_sj - first };
            // Violeta's us_TargetCost never reads a final-type byte (confirmed
            // absent from the 0x1001f1a0 decompile), so the value here is unused.
            let r = sel.select_phrase(&mut words, first, nw, 5);
            file_ok &= compare(&name, s, &r, &mut tally, verbose);
        }
        if file_ok {
            exact_files += 1;
        } else {
            eprintln!("MISMATCH {}", name);
        }
    }
    eprintln!("unitsel parity (spa): {}/{} captures exact; {:?}", exact_files, n_files, tally);
    assert_eq!(exact_files, n_files);
}
