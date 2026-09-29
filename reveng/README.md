# VoiceText Paul (vt_eng.dll): Reverse Engineering Summary

`vtpaul` is a native, zero-dependency Rust reimplementation of the VoiceText English TTS engine
(`vt_eng.dll` 3.11.7.1, the "Paul M16 FILEIO" build — Voiceware Co., Ltd.'s VoiceText, sold in the
US as NeoSpeech; the DLL's version resource reads "Copyright 2000-2013 Voiceware Co., Ltd.") that
reproduces the engine's output **bit for bit** and runs on Linux (musl, amd64/arm64/armv7/armv6) as
well as Windows.

Engine install: `C:\Program Files (x86)\NWSPaulNumber2` (voice data `data-paul\M16`, language data
`data-common`). All addresses in every doc here are VAs in `vt_eng.dll`, image base `0x10000000`,
32-bit VC6 `__cdecl`. Ghidra renames use a per-stage prefix: `fe_` front end (shared by the text and
linguistic stages), `dict_` lexicon, `pro_` prosody, `us_` unit selection, `wav_` waveform;
everything else is `FUN_<address>`.

This document is the index. The four stage docs below are the source of truth for byte-level detail
and asm-derived algorithms; this one and its three siblings (`README_TECHNICAL.md`,
`DLL_ANALYSIS.md`, `EXPERIMENTS.md`) link into them rather than duplicating their tables. This
`reveng/` directory is the stable reference; day-to-day project state (who owns what, what's
unfinished, in-flight agent coordination) is kept outside this repository.

## The pipeline

```
text
  │  VT_TextToBuffer_ENG / VT_TextToFile_ENG / VT_TextToPreprocessInfoFile_ENG
  ▼
┌─────────────────────┐   fe_InitialTextProcess (once per request): VTML tag extraction,
│  Text front end      │   fullwidth-char mapping, orig↔processed offset maps.
│  (per sentence)      │   fe_NextSentence loop: fe_AnalyzeSentence → fe_NormalizeTokens,
│  FRONTEND_TEXT.md    │   a ~20-normalizer chain (punctuation, currency, date, time, units,
└──────────┬───────────┘   digits, address, numword, compound, ...) building the WA
           │ WA (normalized word array, per sentence)
           ▼
┌─────────────────────┐   fe_LinguisticAnalysis: fe_TagAndPronounce (lexicon lookup, morphology,
│  Linguistic front end│   the wh-question flag, function-word tagging) → the merge back into the
│  FRONTEND_LING.md    │   WA (word-record character offsets, remapped to the original input by
└──────────┬───────────┘   fe_NextSentence) → fe_BuildWordPhoneRecords / fe_AssignPhraseBreaks
           │ SS (sentence structure: word/phone/syllable records)   (syllabify + CART breaks) / fe_BuildPhoneContexts
           ▼
┌─────────────────────┐   Per-word rate seeding + VTML span application, then CART-tree
│  Prosody              │   duration/F0/tone prediction (durations never reach the waveform --
│  PROSODY.md           │   they only feed target cost below), sentence/word pause resolution.
└──────────┬───────────┘
           │ predicted durations, F0 contours, target features, pause samples
           ▼
┌─────────────────────┐   Per phrase: build targets (whole-phone or half-phone), a context
│  Unit selection       │   back-off lattice over 580,474 units, target cost + join cost,
│  UNITSEL.md           │   Viterbi + beam pruning, backtrack.
└──────────┬───────────┘
           │ chosen unit ids per target
           ▼
┌─────────────────────┐   Unit + pause records, Shorten decoding, join smoothing, OLA/PSOLA,
│  Waveform              │   emphasis filter, output-format encoding (raw/WAVE/AU/G.711/DADPCM),
│  WAVEFORM.md           │   word/phone sync ring, lip-sync log.
└──────────┬───────────┘
           ▼
        PCM / file / buffer
```

Every stage above is **bit-exact** against the real engine on its own test corpus (see each stage
doc's own parity section, and `EXPERIMENTS.md` for the collected dated results). End to end,
raw text to WAV, the port matches the engine byte for byte on all 2741 test texts, on 1280 more
under non-default pitch/rate/volume/pause settings, and for all four English voices (Paul, Kate,
Julie, James) against both the 32-bit `vt_eng.dll` and the 64-bit voice DLLs.

## Repository layout

| Path | Content |
|---|---|
| `src/text/` | Text front end port |
| `src/ling/` | Linguistic front end port (`lookup.rs`, `tagger.rs`, `analysis.rs`, `morph.rs`, `funcwords.rs`, `merge.rs`, `words.rs`, `syllab.rs`, `breaks.rs`, `context.rs`, `mod.rs`, `tables.rs`) |
| `src/lexicon.rs` | Hash-dict lookup, key encoding, payload decode (see `LEXICON.md`) |
| `src/prosody.rs` | Prosody port |
| `src/unitsel.rs`, `src/classes.rs` | Unit selection port |
| `src/wave.rs`, `src/formats.rs` | Waveform + output-format port |
| `src/engine.rs` | End-to-end wiring (text → ling → prosody → unitsel → wave), `vtpaul speak` CLI |
| `src/bank.rs`, `src/shorten.rs` | Unit bank export/decode |
| `tests/*_parity.rs` | Per-stage parity test suites, one per stage doc below |

The Frida capture harness, the DLL-table generators, the corpus generators, and the Ghidra
decompile/disassembly dumps used while building these notes are not part of this repository.

## The stage docs

| Doc | Covers | Status |
|---|---|---|
| [`FRONTEND_TEXT.md`](FRONTEND_TEXT.md) | Tokenizer, the ~20-normalizer chain, VTML/fullwidth handling, user dictionary | **Bit-exact**, every normalizer, on the full test corpus |
| [`FRONTEND_LING.md`](FRONTEND_LING.md) | Lexicon lookup + homograph tagging, morphology, the wh-question flag, function-word tagging, the WA↔SS merge (`fe_LinguisticAnalysis`), syllabification, phrase breaks, phone contexts | **Bit-exact**, every stage, on the full test corpus |
| [`LEXICON.md`](LEXICON.md) | The `dict-eng` hash dictionaries, perfect-hash algorithm, lexicon/TPP payload formats, `exceptdict`, `*.txt2` tables | **Fully decoded and verified** |
| [`PROSODY.md`](PROSODY.md) | Duration/F0/tone CART trees, syllabification's prosody-relevant fields, rate seeding, VTML span application, pause resolution, target features | **Bit-exact** |
| [`UNITSEL.md`](UNITSEL.md) | Unit database as unit selection sees it, target building, context back-off lattice, target/join cost, Viterbi, beam pruning, `.his`/`.dtt` | **Bit-exact** |
| [`WAVEFORM.md`](WAVEFORM.md) | Unit/pause records, Shorten decode, join smoother, OLA/PSOLA, emphasis, all output formats, word/phone sync, lip-sync log | **Bit-exact** |
| [`VIOLETA.md`](VIOLETA.md) | Spanish engine (`vt_spa_violeta16.dll`): function map, struct layouts, letter-to-sound, lexicon/TPP, words/breaks/contexts, prosody, unit selection, wave | **Bit-exact** per stage from WA to PCM and for the initial text pass; normalizers 94.7% of records; end to end 1032/1327 WAVs identical (2026-09-29) |
| [`OLD_PAUL.md`](OLD_PAUL.md) | The 2009 Paul build and database vs the 2014 target: builds, data formats (ver.2009 unit index, `.tree2`), function-level diff | Inventory only; not ported |

Each stage doc states its own evidence tags in its header (typically `[D]` decompile/disassembly,
`[E]`/`[C]`/`[P]` verified against captures or the port itself, `[I]` inferred) — they aren't fully
standardized across docs written by different agents/sessions, so check the header of whichever one
you're reading.

## How to reproduce everything

Run from the repository root:

```powershell
cargo build --release
cargo test --release          # every *_parity.rs suite in one run
```

Each parity suite compares the port's output against captured ground truth recorded from the real
engine (per-stage JSON/WAV captures keyed by corpus name); a suite skips cleanly rather than failing
when its captures aren't present. The capture harness that produces them is a Frida-based tool that
hooks `vt_eng.dll` and records its intermediate state and output for a fixed corpus of test
sentences per stage — it is not part of this repository. Recapturing is deterministic and the
resulting audio matches the engine's own `VT_TextToFile_ENG` output bit for bit. Targeted live
tracing (hooking a narrower set of call sites) was used whenever a decompile alone was ambiguous —
VC6 stack-slot reuse and pointer-difference-cancellation idioms are common in this binary — every
stage doc has at least one example of live-tracing settling a question the decompile alone couldn't;
see `EXPERIMENTS.md`.

Ghidra MCP (port 8667) holds the project's own analysis of `vt_eng.dll` for anyone continuing the
reverse-engineering; ask the user to have it open.

## Rules every bit-exact stage followed

- Port from the decompile (or, where VC6 folds the pointer arithmetic past readability, the
  disassembly), then prove it on captures. Reproduce engine quirks and bugs; don't fix them.
- Extract DLL tables with a `pefile`-based generator script (not part of this repository) that reads
  them straight out of the DLL into `src/<stage>_tables.rs`. Never transcribe a table by hand.
- The engine's float math is x87 (VC6): 53-bit precision, `FSTP dword`/`FSTP qword` store points,
  `__ftol` truncation. Match it store by store — the Rust port computes in `f64` and rounds to `f32`
  exactly where the disassembly shows a 4-byte store.
- Zero crate dependencies. 4-space indent, K&R braces.
- Frida spawns need a timeout and a kill-on-close job object; cap parallel spawns around 12 — this
  was the empirically reliable limit found while building the capture harness used for the parity
  fixtures.
- No heredocs in any shell. Write scripts to files and run them.
