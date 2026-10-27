//! Linguistic front-end parity of the 2004 Paul engine against engine captures (`out_feling_paul04/*.json`,
//! `harness\capture_feling_paul04.py`). Each sentence starts from the captured `wa_in` (normalization records)
//! and every stage the port covers is compared with the event the engine recorded after it.
//!
//!   cargo test --release --test feling_paul04_parity -- --nocapture
//!   FELING_ONLY=prose_01 FELING_SHOW=20 cargo test ...

use std::collections::BTreeMap;
use std::path::Path;

use vtpaul::era::Era;
use vtpaul::json::Json;
use vtpaul::lexicon::Lexicon;
use vtpaul::ling::Ws;
use vtpaul::ling04::sentence::{self, ph, sy, wd, Ss04, Q_FLAG, SS_SIZE, SYL_BASE, SYL_SIZE};
use vtpaul::ling04::{analysis, lookup, merge, morph, Wa04, MORPH_COUNT, MORPH_SIZE};

const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};
const VOICES: &str = match option_env!("VTPAUL_VOICES") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};

fn hexb(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap_or(0)).collect()
}

fn wa_from(e: &Json) -> Wa04 {
    let mut wa = Wa04::new();
    let head = hexb(e.get("head").str().unwrap_or(""));
    wa.b[..head.len()].copy_from_slice(&head);
    for (i, r) in e.get("recs").arr().iter().enumerate() {
        let b = hexb(r.str().unwrap_or(""));
        let o = Wa04::rec(i);
        wa.b[o..o + b.len()].copy_from_slice(&b);
    }
    wa
}

/// Captured morphs are the 0x6c bytes from +2 of each entry, so the last two bytes are the next entry's
/// flags. The spelling (+3..0x21) and pronunciation (+0x21..0x62) keep stale bytes after their terminator.
fn mask_morph(m: &mut [u8], keep_next_flags: bool) {
    if !keep_next_flags {
        m[0x6a..0x6c].fill(0);
    }
    for (lo, hi) in [(3usize, 0x21usize), (0x21, 0x62)] {
        if let Some(z) = m[lo..hi].iter().position(|&c| c == 0) {
            m[lo + z..hi].fill(0);
        }
    }
}

/// Before morphology the flags are stale, after it every entry's flags are set.
fn morphs_of(e: &Json, flags: bool) -> Vec<Vec<u8>> {
    let a = e.get("morphs").arr();
    a.iter()
        .enumerate()
        .map(|(i, m)| {
            let mut b = hexb(m.str().unwrap_or(""));
            mask_morph(&mut b, flags && i + 1 < a.len());
            b
        })
        .collect()
}

fn ours_of(ws: &Ws, flags: bool) -> Vec<Vec<u8>> {
    let n = ws.i16(MORPH_COUNT).max(0) as usize;
    (0..n)
        .map(|i| {
            let o = morph(i) + 2;
            let mut b = ws.b[o..o + MORPH_SIZE].to_vec();
            mask_morph(&mut b, flags && i + 1 < n);
            b
        })
        .collect()
}

fn show_morph(m: &[u8]) -> String {
    let s = |lo: usize, hi: usize| {
        let t = &m[lo..hi];
        let t = &t[..t.iter().position(|&c| c == 0).unwrap_or(t.len())];
        t.iter().map(|&c| if (0x20..0x7f).contains(&c) { (c as char).to_string() } else { format!("\\x{:02x}", c) }).collect::<String>()
    };
    format!(
        "tok {} cls {:02x} '{}' pron '{}' tail {}",
        u16::from_le_bytes([m[0], m[1]]),
        m[2],
        s(3, 0x21),
        s(0x21, 0x62),
        m[0x62..].iter().map(|c| format!("{:02x}", c)).collect::<String>()
    )
}

/// The captured SS (head, word records, syllables) placed at their offsets, with the engine's SS and WA
/// base addresses.
fn ss_from(e: &Json) -> (Ss04, u32, u32) {
    let mut ss = Ss04::new();
    let head = hexb(e.get("head").str().unwrap_or(""));
    ss.b[..head.len()].copy_from_slice(&head);
    for (i, w) in e.get("words").arr().iter().enumerate() {
        let b = hexb(w.str().unwrap_or(""));
        let o = Ss04::word(i);
        ss.b[o..o + b.len()].copy_from_slice(&b);
    }
    let syls = hexb(e.get("syls").str().unwrap_or(""));
    ss.b[SYL_BASE..SYL_BASE + syls.len()].copy_from_slice(&syls);
    ss.b[Q_FLAG] = e.get("flag").i64().unwrap_or(0) as u8;
    let addr = |k: &str| u32::from_str_radix(e.get(k).str().unwrap_or("0x0").trim_start_matches("0x"), 16).unwrap_or(0);
    (ss, addr("ss_base"), addr("wa_base"))
}

/// Named fields of the SS each stage writes; `ptr` maps a stored pointer to the port's tagged form.
fn ss_fields(ss: &Ss04, stage: u8, ptr: &dyn Fn(u32) -> u32) -> Vec<(String, Vec<u8>)> {
    let mut f: Vec<(String, Vec<u8>)> = Vec::new();
    let nw = ss.n_words();
    let p = |o: usize| ptr(ss.u32(o)).to_le_bytes().to_vec();
    f.push(("q_flag".into(), vec![ss.u8(Q_FLAG)]));
    for i in 0..nw {
        let w = Ss04::word(i);
        let n = ss.n_phones(i).min(0x41);
        f.push((format!("w{} text/ph/pos ptr", i), [p(w + wd::TEXT_PTR), p(w + wd::PH_PTR), p(w + wd::POS_PTR)].concat()));
        f.push((format!("w{} codes", i), ss.b[w + wd::CODES..w + wd::CODES + n].to_vec()));
        f.push((format!("w{} marks", i), ss.b[w + wd::MARKS..w + wd::MARKS + n].to_vec()));
        f.push((format!("w{} nph/nmorph", i), ss.b[w + wd::N_PHONES..w + wd::N_MORPH + 2].to_vec()));
        f.push((format!("w{} punct/u", i), ss.b[w + wd::PUNCT..w + wd::U_FLAG + 1].to_vec()));
        if stage >= 1 {
            f.push((format!("w{} syl ptr/n", i), [p(w + wd::SYL_PTR), ss.b[w + wd::N_SYL..w + wd::N_SYL + 2].to_vec()].concat()));
            f.push((format!("w{} sylpos", i), ss.b[w + wd::SYLPOS..w + wd::SYLPOS + n].to_vec()));
            f.push((format!("w{} minor/brk", i), ss.b[w + wd::MINOR..w + wd::BRK + 1].to_vec()));
        }
        if stage >= 2 {
            f.push((format!("w{} ctx", i), ss.b[w + wd::CTX..w + wd::CTX + 5 * n].to_vec()));
            f.push((format!("w{} edge", i), ss.b[w + wd::EDGE..w + wd::EDGE + n].to_vec()));
        }
    }
    if stage >= 1 {
        f.push(("n_phrases/n_syl".into(), [ss.b[0..2].to_vec(), ss.b[4..6].to_vec()].concat()));
        for g in 0..ss.n_phrases().min(0x40) {
            let e = Ss04::phrase(g);
            f.push((format!("phrase {}", g), [ss.b[e..e + 6].to_vec(), p(e + ph::WORD_PTR), p(e + ph::SYL_PTR)].concat()));
        }
        for s in 0..(ss.i16(4).max(0) as usize).min(2000) {
            let r = SYL_BASE + s * SYL_SIZE;
            f.push((format!("syl {}", s), [ss.b[r..r + 3].to_vec(), p(r + sy::CODES_PTR), p(r + sy::POS_PTR), ss.b[r + sy::N_PHONES..r + sy::N_PHONES + 2].to_vec()].concat()));
        }
    }
    f
}

#[derive(Default)]
struct Tally {
    n: usize,
    ok: usize,
}

#[test]
fn linguistic_parity() {
    let dir = Path::new(HARNESS).join("out_feling_paul04");
    let dict = Path::new(VOICES).join("2004_paul").join("data-common").join("dict-eng");
    if !dir.exists() || !dict.exists() {
        eprintln!("skipping: captures or voice data not present");
        return;
    }
    let only = std::env::var("FELING_ONLY").ok();
    let show_n: usize = std::env::var("FELING_SHOW").ok().and_then(|s| s.parse().ok()).unwrap_or(10);
    let lex = Lexicon::open_era(&dict, Era::Paul04).expect("open 2004 lexicon");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let mut t: BTreeMap<&str, Tally> = BTreeMap::new();
    let mut shown = 0;
    let mut ws = Ws::new();
    for f in &files {
        let name = f.file_stem().unwrap().to_string_lossy().to_string();
        if only.as_deref().is_some_and(|o| o != name) {
            continue;
        }
        let j = Json::parse(&std::fs::read_to_string(f).unwrap()).unwrap();
        let mut cur: Option<Wa04> = None;
        let mut n_cands = 0i16;
        let mut snaps: BTreeMap<&str, Vec<Vec<u8>>> = BTreeMap::new();
        let mut wh: Option<i16> = None;
        let mut wa_out: Option<(i16, Vec<u8>)> = None;
        let mut merged: Option<Wa04> = None;
        let mut ss_cur: Option<Ss04> = None;
        let mut ss_ok = true;
        for e in j.get("events").arr() {
            let kind = e.get("kind").str().unwrap_or("");
            match kind {
                "wa_in" => {
                    let wa = wa_from(e);
                    let n = wa.n();
                    snaps.clear();
                    wh = None;
                    ws.set_i16(0, 0);
                    ws.set_i16(MORPH_COUNT, 0);
                    if n > 0 && lookup::lookup_tokens(&mut ws, &wa, n, true, &lex) {
                        n_cands = ws.i16(0);
                        if n_cands > 0 {
                            analysis::copy_morphs(&mut ws, &lex);
                        }
                        snaps.insert("copy_out", ours_of(&ws, false));
                        vtpaul::ling04::morph::morphology(&mut ws, &wa, &lex);
                        snaps.insert("morph_out", ours_of(&ws, true));
                        analysis::wh_flag(&mut ws);
                        snaps.insert("wh_out", ours_of(&ws, true));
                        snaps.insert("tp_out", ours_of(&ws, true));
                        wh = Some(ws.i16(vtpaul::ling04::WH_FLAG));
                    }
                    let mut wa2 = wa_from(e);
                    let ret = merge::analyze(&mut wa2, &mut ws, &lex, 0);
                    let mut img = wa2.b[..vtpaul::ling04::WA_HEAD].to_vec();
                    for i in 0..n {
                        img.extend_from_slice(&wa2.b[Wa04::rec(i)..Wa04::rec(i) + vtpaul::ling04::WA_REC]);
                    }
                    wa_out = Some((ret, img));
                    merged = Some(wa2);
                    cur = Some(wa);
                }
                "words_out" | "breaks_out" | "ctx_out" => {
                    let stage = match kind {
                        "words_out" => 0u8,
                        "breaks_out" => 1,
                        _ => 2,
                    };
                    let Some(m) = merged.as_ref() else { continue };
                    // Captures from before the hook dumped words from +0x64c carry no base addresses.
                    if e.get("ss_base").is_null() {
                        continue;
                    }
                    if stage == 0 {
                        let mut ss = Ss04::new();
                        ss.set_i16(2, m.n() as i16);
                        sentence::build_words(&mut ss, m);
                        ss_cur = Some(ss);
                    }
                    let Some(ss) = ss_cur.as_mut() else { continue };
                    if stage == 1 {
                        let lv: Vec<i32> = e.get("tib").get("brk_level").arr().iter().map(|v| v.i64().unwrap_or(0) as i32).collect();
                        ss_ok = sentence::assign_breaks(ss, &lv);
                    }
                    if stage == 2 && ss_ok {
                        sentence::build_contexts(ss);
                    }
                    let (cap, ssb, wab) = ss_from(e);
                    let norm = |v: u32| {
                        if v >= ssb && v < ssb + SS_SIZE as u32 {
                            vtpaul::ling::ptr_ss((v - ssb) as usize)
                        } else if v >= wab && v < wab + vtpaul::ling04::WA_SIZE as u32 {
                            vtpaul::ling::ptr_wa((v - wab) as usize)
                        } else {
                            v
                        }
                    };
                    let want = ss_fields(&cap, stage, &norm);
                    let ours = ss_fields(ss, stage, &|v| v);
                    let tl = t.entry(["words_out", "breaks_out", "ctx_out"][stage as usize]).or_default();
                    tl.n += 1;
                    if want == ours {
                        tl.ok += 1;
                    } else if shown < show_n {
                        shown += 1;
                        let k = (0..want.len()).find(|&k| ours.get(k) != Some(&want[k])).unwrap_or(want.len());
                        eprintln!(
                            "MISMATCH {} {} {}: eng {:02x?} ours {:02x?}",
                            name,
                            kind,
                            want.get(k).map(|x| x.0.as_str()).unwrap_or("?"),
                            want.get(k).map(|x| &x.1),
                            ours.get(k).map(|x| &x.1)
                        );
                    }
                }
                "wa_out" => {
                    let tl = t.entry("wa_out").or_default();
                    tl.n += 1;
                    let mut want = hexb(e.get("head").str().unwrap_or(""));
                    for r in e.get("recs").arr() {
                        want.extend(hexb(r.str().unwrap_or("")));
                    }
                    let want_ret = e.get("ret").i64().unwrap_or(0) as i16;
                    let (ret, ours) = wa_out.clone().unwrap_or((0, Vec::new()));
                    // The status also folds in fe_NormalizeTokens' result, which the capture doesn't hold: only a
                    // token-count failure (-1 with mode 0) is checked.
                    if ours == want && (ret != -1 || want_ret == -1) {
                        tl.ok += 1;
                    } else if shown < show_n {
                        shown += 1;
                        let k = (0..want.len()).find(|&k| ours.get(k) != Some(&want[k])).unwrap_or(want.len());
                        let (rec, off) = if k < 0x14 { (-1, k) } else { (((k - 0x14) / 0x88) as i64, (k - 0x14) % 0x88) };
                        eprintln!(
                            "MISMATCH {} wa_out ret eng {} ours {}; first diff byte {} (rec {} +{:#x}): eng {:02x?} ours {:02x?}",
                            name,
                            want_ret,
                            ret,
                            k,
                            rec,
                            off,
                            &want[k.min(want.len())..(k + 8).min(want.len())],
                            &ours[k.min(ours.len())..(k + 8).min(ours.len())]
                        );
                    }
                }
                "copy_out" | "morph_out" | "wh_out" | "tp_out" => {
                    let flags = kind != "copy_out";
                    let tl = t.entry(match kind {
                        "copy_out" => "copy_out",
                        "morph_out" => "morph_out",
                        "wh_out" => "wh_out",
                        _ => "tp_out",
                    })
                    .or_default();
                    tl.n += 1;
                    let want = morphs_of(e, flags);
                    let ours = snaps.get(kind).cloned().unwrap_or_default();
                    let wh_ok = kind == "copy_out" || kind == "morph_out" || wh.map_or(true, |w| w as i64 == e.get("wh").i64().unwrap_or(0));
                    if !wh_ok && shown < show_n {
                        shown += 1;
                        eprintln!("MISMATCH {} {} wh: eng {} ours {:?}", name, kind, e.get("wh").i64().unwrap_or(0), wh);
                    }
                    if ours == want && wh_ok {
                        tl.ok += 1;
                    } else if shown < show_n {
                        shown += 1;
                        let k = (0..want.len()).find(|&k| ours.get(k) != Some(&want[k])).unwrap_or(want.len());
                        eprintln!("MISMATCH {} {} morph {} of {} (ours {})", name, kind, k, want.len(), ours.len());
                        if let Some(w) = want.get(k) {
                            eprintln!("    eng  {}", show_morph(w));
                        }
                        if let Some(o) = ours.get(k) {
                            eprintln!("    ours {}", show_morph(o));
                        }
                    }
                }
                "lookup_out" => {
                    let want = e.get("n").i64().unwrap_or(0) as i16;
                    let tl = t.entry("lookup_out").or_default();
                    tl.n += 1;
                    if n_cands == want {
                        tl.ok += 1;
                    } else if shown < show_n {
                        shown += 1;
                        let texts: Vec<String> = cur
                            .as_ref()
                            .map(|w| (0..w.n()).map(|i| String::from_utf8_lossy(w.cstr(Wa04::rec(i) + 0x1e)).into_owned()).collect())
                            .unwrap_or_default();
                        eprintln!("MISMATCH {} lookup_out: eng {} ours {} {:?}", name, want, n_cands, texts);
                    }
                }
                _ => {}
            }
        }
    }
    eprintln!("feling paul04 parity:");
    for (k, v) in &t {
        eprintln!("  {:12} {}/{}", k, v.ok, v.n);
    }
}
