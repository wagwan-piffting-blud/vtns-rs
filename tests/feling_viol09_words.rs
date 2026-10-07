//! viol09 linguistic front end after the lexicon pass: word records (`FUN_10018e60`), syllabifier and phrase
//! breaks (`FUN_1001a8f0`) and phone contexts (`FUN_100191b0`) against `out_feling_viol09/*.json`
//! (`words_out`, `breaks_out`, `ctx_out` per sentence; the captures hold no syllable records or TIB arrays).
//!
//!     cargo test --release --test feling_viol09_words -- --nocapture

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use vtpaul::json::Json;
use vtpaul::ling::Tib;
use vtpaul::ling_spa::spa09::{assign_phrase_breaks, build_phone_contexts, build_word_phone_records, w09, Ss09, TextPos09, WORD_BASE, WORD_SIZE};
use vtpaul::ling_spa::WaSpa;
use vtpaul::tree3::Tree;

const HARNESS: &str = match option_env!("VTPAUL_HARNESS") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../harness"),
};

const VOICES: &str = match option_env!("VTPAUL_VOICES") {
    Some(p) => p,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};

/// Pointer fields masked before comparing raw word bytes.
const WORD_PTRS: [usize; 5] = [0x000, 0x004, w09::SYL_PTR, w09::TEXT_PTR, w09::PH_PTR];

fn unhex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks(2)
        .map(|c| {
            let h = |x: u8| (x as char).to_digit(16).unwrap_or(0) as u8;
            h(c[0]) << 4 | h(c[1])
        })
        .collect()
}

fn load_wa(e: &Json) -> WaSpa {
    let mut wa = WaSpa::new();
    let head = unhex(e.get("head").str().unwrap());
    wa.b[..head.len()].copy_from_slice(&head);
    for (i, r) in e.get("recs").arr().iter().enumerate() {
        let b = unhex(r.str().unwrap());
        let o = WaSpa::rec(i);
        wa.b[o..o + b.len()].copy_from_slice(&b);
    }
    wa
}

fn load_ss(e: &Json, ss: &mut Ss09) {
    let head = unhex(e.get("head").str().unwrap());
    ss.b[..head.len()].copy_from_slice(&head);
    for (i, w) in e.get("words").arr().iter().enumerate() {
        let b = unhex(w.get("raw").str().unwrap());
        let o = Ss09::word(i);
        ss.b[o..o + b.len()].copy_from_slice(&b);
        ss.set_u32(o + w09::TEXT_PTR, vtpaul::ling::ptr_wa(WaSpa::rec(i) + vtpaul::ling_spa::war::TEXT));
        ss.set_u32(o + w09::PH_PTR, vtpaul::ling::ptr_wa(WaSpa::rec(i) + vtpaul::ling_spa::words::war2::PHONES));
    }
}

fn masked_head(b: &[u8]) -> Vec<u8> {
    let mut v = b[..WORD_BASE].to_vec();
    for g in 0..100 {
        for p in [Ss09::phrase(g) + 8, Ss09::phrase(g) + 0xc] {
            if p + 4 <= v.len() {
                v[p..p + 4].fill(0);
            }
        }
    }
    v
}

#[derive(Default)]
struct Tally {
    ok: usize,
    total: usize,
    shown: usize,
}

impl Tally {
    fn report(&self, name: &str) {
        println!("{:10} {:5}/{:5} sentences exact", name, self.ok, self.total);
    }
}

fn compare_words(t: &mut Tally, tag: &str, ss: &Ss09, want: &Json) {
    t.total += 1;
    let mut diffs: Vec<String> = Vec::new();
    for (i, w) in want.get("words").arr().iter().enumerate() {
        let b = unhex(w.get("raw").str().unwrap());
        let o = Ss09::word(i);
        let n = b[w09::N_PHONES] as usize;
        let mut offs: Vec<usize> = vec![w09::N_PHONES, w09::SRC, w09::CLASSCH, w09::UNK46C, w09::TOKEN_CLASS, w09::TOKEN_CLASS + 1, w09::X3AE, w09::X3AE + 1];
        offs.extend((0..4).map(|k| w09::START + k));
        offs.extend((0..4).map(|k| w09::END + k));
        for k in 0..n {
            offs.extend([w09::CODES + k, w09::MARKS + k]);
        }
        for off in offs {
            if ss.u8(o + off) != b[off] {
                diffs.push(format!("w{}+{:x}: {:02x} want {:02x}", i, off, ss.u8(o + off), b[off]));
            }
        }
    }
    if diffs.is_empty() {
        t.ok += 1;
    } else if t.shown < 12 {
        t.shown += 1;
        println!("  [{} words] {} diffs: {}", tag, diffs.len(), diffs[..diffs.len().min(10)].join(" | "));
    }
}

fn compare(t: &mut Tally, tag: &str, ss: &Ss09, want: &Json) {
    t.total += 1;
    let mut diffs: Vec<String> = Vec::new();
    let head = masked_head(&unhex(want.get("head").str().unwrap()));
    let ours = masked_head(&ss.b);
    for (o, (a, b)) in ours.iter().zip(head.iter()).enumerate() {
        if a != b {
            diffs.push(format!("head+{:x}: {:02x} want {:02x}", o, a, b));
        }
    }
    for (i, w) in want.get("words").arr().iter().enumerate() {
        let mut b = unhex(w.get("raw").str().unwrap());
        for p in WORD_PTRS {
            b[p..p + 4].fill(0);
        }
        let o = Ss09::word(i);
        let mut a = ss.b[o..o + WORD_SIZE].to_vec();
        for p in WORD_PTRS {
            a[p..p + 4].fill(0);
        }
        for (off, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            if x != y {
                diffs.push(format!("w{}+{:x}: {:02x} want {:02x}", i, off, x, y));
            }
        }
    }
    if diffs.is_empty() {
        t.ok += 1;
    } else if t.shown < 12 {
        t.shown += 1;
        println!("  [{}] {} diffs: {}", tag, diffs.len(), diffs[..diffs.len().min(10)].join(" | "));
    }
}

fn captures(dir: &Path) -> Vec<PathBuf> {
    if !dir.exists() {
        return Vec::new();
    }
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    v.sort();
    v
}

fn sentences(doc: &Json) -> Vec<BTreeMap<String, Json>> {
    let mut out: Vec<BTreeMap<String, Json>> = Vec::new();
    for e in doc.get("events").arr() {
        let k = e.get("kind").str().unwrap_or("").to_string();
        if k == "wa_in" {
            out.push(BTreeMap::new());
        }
        if let Some(cur) = out.last_mut() {
            cur.insert(k, e.clone());
        }
    }
    out
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

#[test]
fn record_stages_viol09() {
    let dir = Path::new(HARNESS).join("out_feling_viol09");
    let tree = Path::new(VOICES).join("old_julie_violeta").join("Violeta").join("data-common").join("dict-spa").join("spabi.tree3");
    let caps = captures(&dir);
    if caps.is_empty() || !tree.exists() {
        eprintln!("skipping: no captures or voice data");
        return;
    }
    let spabi = Tree::load(&tree).unwrap();
    let mut t_words = Tally::default();
    let mut t_breaks = Tally::default();
    let mut t_ctx = Tally::default();
    for path in &caps {
        let doc = Json::load(path).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let mut cursor = 0i32;
        let (map_b, map_c) = load_offset_maps(&doc);
        for (si, s) in sentences(&doc).iter().enumerate() {
            let (Some(wa_out), Some(words), Some(brk), Some(ctx)) = (s.get("wa_out"), s.get("words_out"), s.get("breaks_out"), s.get("ctx_out")) else {
                continue;
            };
            let tag = format!("{}#{}", name, si);
            let wa = load_wa(wa_out);

            let mut ss = Ss09::new();
            load_ss(words, &mut ss);
            for i in 0..ss.n_words() {
                let w = Ss09::word(i);
                for o in [w09::N_PHONES, w09::SRC, w09::CLASSCH, w09::UNK46C, w09::TOKEN_CLASS, w09::X3AE] {
                    ss.set_u8(w + o, 0);
                }
                ss.set_i32(w + w09::START, 0);
                ss.set_i32(w + w09::END, 0);
                ss.b[w + w09::CODES..w + w09::CODES + 0x64].fill(0);
                ss.b[w + w09::MARKS..w + w09::MARKS + 0x64].fill(0);
            }
            let pos = TextPos09 { cursor, n: map_b.len() as i32, map_start: &map_b, map_end: &map_c };
            build_word_phone_records(&mut ss, &wa, &pos);
            cursor += wa.i32(4);
            compare_words(&mut t_words, &tag, &ss, words);

            let mut ss = Ss09::new();
            load_ss(words, &mut ss);
            let mut tib = Tib::new();
            assign_phrase_breaks(&mut ss, &mut tib, &wa, &spabi);
            compare(&mut t_breaks, &tag, &ss, brk);

            let mut ss = Ss09::new();
            load_ss(brk, &mut ss);
            build_phone_contexts(&mut ss);
            compare(&mut t_ctx, &tag, &ss, ctx);
        }
    }
    t_words.report("words");
    t_breaks.report("breaks");
    t_ctx.report("contexts");
    assert!(t_words.total > 0);
    assert_eq!(t_words.ok, t_words.total, "words mismatches");
    assert_eq!(t_breaks.ok, t_breaks.total, "breaks mismatches");
    assert_eq!(t_ctx.ok, t_ctx.total, "ctx mismatches");
}
