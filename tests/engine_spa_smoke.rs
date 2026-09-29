//! Violeta end to end: stage-by-stage trace of one line (debug aid).

use std::path::Path;

use vtpaul::ling::Tib;
use vtpaul::ling_spa::words::{build_word_phone_records, TextPos};
use vtpaul::ling_spa::{analysis, breaks, SsSpa};

const VOICES: &str = match option_env!("VTPAUL_VOICES") {
    Some(v) => v,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/../all_voices"),
};

#[test]
fn violeta_stage_trace() {
    let root = Path::new(VOICES);
    if !root.join("data-violeta").exists() {
        eprintln!("skipping: no voice data");
        return;
    }
    let eng = vtpaul::engine_spa::EngineSpa::open(root).expect("open");
    let text = std::env::var("VTSPA_TEXT").unwrap_or_else(|_| "Hola, buenos d\u{ed}as.".into());
    let bytes: Vec<u8> = text.chars().map(|c| c as u32 as u8).collect();
    let sents = eng.text_to_sentences(&bytes);
    eprintln!("sentences: {}", sents.len());
    for (i, s) in sents.iter().enumerate() {
        let mut wa = s.wa.clone();
        eprintln!("#{} words {}", i, wa.n());
        let a = analysis::analyze(&mut wa, &eng.fe.lex);
        eprintln!("  analyze {}", a);
        let mut ss = SsSpa::new();
        ss.set_i16(2, wa.n() as i16);
        let w = build_word_phone_records(&mut ss, &wa, &s.kind7, &TextPos::none());
        eprintln!("  words {} n_words {}", w, ss.n_words());
        let mut tib = Tib::new();
        let b = breaks::assign_phrase_breaks(&mut ss, &mut tib, &wa, &eng.spabi);
        eprintln!("  breaks {} phrases {}", b, ss.n_phrases());
    }
    let pcm = eng.speak(&bytes).expect("speak");
    eprintln!("samples {}", pcm.len());
}
