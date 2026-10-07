# VoiceText reverse engineering: index

`vtpaul` is a zero-dependency Rust reimplementation of the VoiceText English and Spanish TTS engines
(Voiceware Co., Ltd.; sold in the US as NeoSpeech). It reproduces the engines' output bit for bit, raw
text to WAV, and runs on Windows, macOS (arm64, x86_64) and Linux (musl; amd64, arm64, armv7, armv6).

This directory is the stable reference. Stage docs hold the byte-level detail and algorithms; the
other docs link into them instead of repeating tables.

## Docs

| Doc | Content |
|---|---|
| [FRONTEND_TEXT.md](FRONTEND_TEXT.md) | Text front end: tokenizer, normalizer chain, VTML, fullwidth map, user dictionary |
| [FRONTEND_LING.md](FRONTEND_LING.md) | Linguistic front end: lexicon lookup, tagging, morphology, WA to SS merge, syllables, breaks, phone contexts |
| [LEXICON.md](LEXICON.md) | `dict-eng` hash dictionaries, payload formats, `exceptdict`, `*.txt2` |
| [PROSODY.md](PROSODY.md) | Duration/F0/tone CART trees, rate seeding, VTML spans, pause resolution, target features |
| [UNITSEL.md](UNITSEL.md) | Unit database as selection sees it, back-off lattice, target/join cost, Viterbi, beam pruning |
| [WAVEFORM.md](WAVEFORM.md) | Unit records, Shorten decode, join smoother, OLA/PSOLA, output formats, sync, lip-sync log |
| [VIOLETA.md](VIOLETA.md) | Spanish engine (Violeta 2013 and the 2009 build): per-stage differences from English |
| [OLD_PAUL.md](OLD_PAUL.md) | The 2009 engines (Paul09, Julie09, Violeta09): formats and per-stage differences from 2014 |
| [README_TECHNICAL.md](README_TECHNICAL.md) | Byte maps of every on-disk file format the 2014 English engine reads |
| [DLL_ANALYSIS.md](DLL_ANALYSIS.md) | Function inventory of `vt_eng.dll` by stage, with Rust counterparts |
| [EXPERIMENTS.md](EXPERIMENTS.md) | Parity runs, negative controls and fixes per stage |

## Conventions

- Addresses are VAs in the DLL under discussion. `vt_eng.dll` (3.11.7.1, "Paul M16 FILEIO") has image
  base `0x10000000`, 32-bit VC6, `__cdecl`. Default engine install:
  `C:\Program Files (x86)\NWSPaulNumber2` (voice data `data-paul\M16`, language data `data-common`).
- Ghidra rename prefixes: `fe_` front end (text and linguistic), `dict_` lexicon, `pro_` prosody,
  `us_` unit selection, `wav_` waveform. `FUN_<address>` is unrenamed.
- Evidence tags in stage-doc headers: `[D]` decompile/disassembly, `[E]`/`[C]`/`[P]` verified on
  captures or by the port, `[I]` inferred. Not fully uniform across docs; check each header.

## Pipeline

```
text
  | fe_InitialTextProcess (once per request): VTML tags, fullwidth map, offset maps
  v
Text front end  [FRONTEND_TEXT.md]      per sentence: tokenize, ~20-normalizer chain -> WA
  v
Linguistic front end  [FRONTEND_LING.md, LEXICON.md]
                                        lexicon/morphology/tagging, merge -> SS: word, phone, syllable records;
                                        syllabify, phrase breaks, 7-byte phone contexts
  v
Prosody  [PROSODY.md]                   rate seeding, VTML spans, CART duration/F0/tone, pauses
  v
Unit selection  [UNITSEL.md]            targets, back-off lattice, target+join cost, Viterbi, beam
  v
Waveform  [WAVEFORM.md]                 Shorten decode, join smoothing, OLA/PSOLA, emphasis, formats, sync
  v
PCM / file / buffer
```

Durations never reach the waveform directly; they only feed the target cost.

## Builds reproduced

| Build | Engine DLL | Data | Era in code |
|---|---|---|---|
| 2014 English, Paul/Kate/Julie/James, x86 | `vt_eng.dll` 3.11.7.1 (voice name patched per voice) | `ver.2013` index, tree3 | `Era::V2014` |
| 2014 English, same four voices, x64 | `vt_pau/vt_kat/vt_jul/vt_jam.dll` | same banks | `Era::V2014`, `Engine::set_x64` / CLI `--x64` |
| Violeta 2013 (Spanish) | `vt_spa_violeta16.dll` | `ver.2013`, tree3 | `Era::V2014`, `Lang::Spa` |
| Paul09 | `vt_eng_paul16.dll` (2009-09-15) | `ver.2009`, `.tree2` | `Era::Paul09` |
| Julie09 | `vt_eng_julie16.dll` (Dec 2009) | `ver.2009`, tree3 | `Era::Julie09` |
| Violeta09 | `vt_spa_violeta16.dll` (Dec 2009) | `ver.2005` header, tree3 | `Era::Violeta09` |

`Era::detect` (`src/era.rs`) picks the build from the bank header and tree layout. x64 differs from x86
only in single-precision float evaluation in unit selection (see [UNITSEL.md](UNITSEL.md)). The 2009
builds differ in index layout (5-byte labels), word-record layouts, text and linguistic rules, selector
and wave details; each stage doc has a "2009 builds" section.

## Status

Every stage is bit-exact against the real engine on its capture corpus. End to end, raw text to WAV:

| Build | Result |
|---|---|
| 2014 English (`speak_parity`) | 2741/2741 texts; 1280 more under non-default pitch/rate/volume/pause |
| 2014 English, all four voices, x86 and x64 (`voices_parity`) | bit-exact |
| Violeta 2013 (`speak_spa_parity`) | 1327/1327 WAVs (2026-09-29) |
| Paul09, Julie09 (`speak_old_parity`) | 2741/2741 each (2026-10-05) |
| Violeta09 (`speak_old_parity`) | 1366/1366 (2026-10-05) |

Open, not blocking: Spanish lip-sync log; `.his` selection history on 2009 engines (no capture).

## Repository layout

| Path | Content |
|---|---|
| `src/text/`, `src/text_spa/` | Text front ends |
| `src/ling/`, `src/ling_spa/`, `src/lexicon.rs` | Linguistic front ends, lexicon |
| `src/prosody.rs`, `src/prosody_spa.rs`, `src/tree2.rs`, `src/tree3.rs` | Prosody, CART trees |
| `src/unitsel.rs`, `src/unitsel09.rs`, `src/classes.rs` | Unit selection |
| `src/wave.rs`, `src/formats.rs` | Waveform, output formats |
| `src/bank.rs`, `src/shorten.rs`, `src/era.rs` | Unit bank, Shorten, build detection |
| `src/engine.rs`, `src/engine09.rs`, `src/engine_spa.rs` | End-to-end wiring, `vtpaul speak` CLI |
| `tests/*_parity.rs` | Parity suites, one or more per stage and build |

The Frida capture harness, table generators, corpus generators and Ghidra dumps are not in this
repository. Parity suites skip when their captures are missing.

## Reproduce

```powershell
cargo build --release
cargo test --release
```

Captures come from a Frida harness that hooks the real DLL and records intermediate state and output
for a fixed corpus per stage. Narrow live traces settled questions the decompile could not
(VC6 stack-slot reuse, pointer-difference idioms).

Rules every stage followed:

- Port from the decompile, or the disassembly where VC6 hides the pointer arithmetic; prove on captures.
  Reproduce engine quirks and bugs.
- Read DLL tables with a `pefile` generator into `src/*_tables.rs`; never transcribe by hand.
- The engine's float math is x87: 53-bit precision, `FSTP dword/qword` store points, `__ftol` truncation.
  The port computes in `f64` and rounds to `f32` where the disassembly stores 4 bytes.
- Zero crate dependencies. 4-space indent, K&R braces.
- Frida spawns need a timeout and a kill-on-close job object; about 12 parallel spawns is the
  reliable limit.
- Ghidra MCP (port 8667) holds the analysis of `vt_eng.dll`; ask for it before continuing the reverse engineering.
