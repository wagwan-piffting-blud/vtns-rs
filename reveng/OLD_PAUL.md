# The 2009 and 2004 engines (Paul09, Julie09, Violeta09, Paul04)

Reference for the older VoiceText engine builds and their ports in `vtpaul-rs`: three 2009 builds and the 2004
Paul build ([The 2004 engine](#the-2004-engine-paul04) at the end). Each is bit-exact from raw text to PCM
against the real DLL. The port reads the voice data, decides which build it belongs to
(`Era`) and runs that build's variant of every stage. The 2014 engines (English `vt_eng.dll` 3.11.7.1,
Violeta 2013) are the baseline; this file lists what differs and where each difference lives. Stage details for
2014 are in [PROSODY.md](PROSODY.md), [UNITSEL.md](UNITSEL.md), [WAVEFORM.md](WAVEFORM.md),
[FRONTEND_TEXT.md](FRONTEND_TEXT.md), [FRONTEND_LING.md](FRONTEND_LING.md), [LEXICON.md](LEXICON.md) and
[VIOLETA.md](VIOLETA.md). Paths are relative to the repository or generic install paths; voice data is not in the
repository.

## Builds

| Build | DLL | Linked | Speaker | Index | Trees | License check |
|---|---|---|---:|---|---|---|
| Paul09 | `Paul\lib\vt_eng_paul16.dll` (same bytes as `vt_pau.dll`; 61 exports, no version resource) | 2009-09-15 | 1 | `ver.2009` | tree2 | stub in the shipped DLL |
| Julie09 | `Julie\lib\vt_eng_julie16.dll` | Dec 2009 | 3 | `ver.2009` | tree3 | `VT_CheckLicense_ENG` 0x10028d50, stubbed in the capture copy |
| Violeta09 | `Violeta\lib\vt_spa_violeta16.dll` | Dec 2009 | 0 (`VT_LANG=SPA`) | `ver.2005` | tree3 | `VT_CheckLicense_SPA` 0x100122c0, stubbed in the capture copy |

- The Paul09 install also ships `vtengsapi50.dll` ("VoiceText SAPI 5.1 English" 3.10.0.0), `UserDicEng.exe`,
  `dict_conv.dll`, `VW_Pron.ttf` and the SAPI 5.1 SDK `TTSApp.exe` sample.
- The 2009 English DLLs hold a 4-entry speaker table at 0x10078430 (Julie09): Kate 0, Paul 1, em001 2, Julie 3,
  6 dwords each (directory name, ..., display name, banner).
- 2014 `vt_eng.dll` has 5 exports more than Paul09 (`VT_CheckUserDict_SourceNorm`, `VT_GetPathKey`,
  `VT_SetDecimal0Pron`, `VT_SetPhone0Pron`, `VT_SpeakersInfo`, all `_ENG`). Every 2009 export exists in 2014, so
  `harness\vtprobe.exe` drives a 2009 DLL unchanged (`VT_DLL`, `VT_ROOT`, `VT_SPEAKER`, `VT_LANG`).
- The 2014 engine loads none of these banks: Paul09 fails with 3 (no `.tree3`), Julie09 with 7.
- Paul09 bank sizes against 2014 Paul: gen 272,026 vs 440,124 units, num 3,385 vs 24,508, etc 5,172 vs 115,723,
  alp 119 vs 119; `merged-gen.dat` 210 MB vs 347 MB. `cepdist.tbl` and `dblist.idx` are identical. Julie09 bank:
  293,925 units (2014 Julie 304,795). A third Julie bank (671,379 gen units, `(26183826 - 45) / 39`) is the one
  from vtns-rs issue #3; it has no reference output here.
- Paul09 lexicon: `engttsdict_emb`, `hashcont/hashidx/hashparams_emb`, `exceptdict`, `chc_sort.txt2`,
  `streeta_sort.txt2` differ from 2014 (slightly smaller); `tppdict_eng`, the `*_tpp` hashes and the other
  `*_sort.txt2` are identical. The 2009 `userdict_eng.csv` has 361 bytes of entries.

## How the port picks a build

`Era::{V2014, Paul09, Julie09, Violeta09}` in `src/era.rs`; `Era::detect(bank, m16, lang)`:

1. Index header `ver.2013` gives `V2014`.
2. Spanish gives `Violeta09`.
3. English with `ver.2009`: `ttsdata\tree` present and `ttsdata\tree3` absent gives `Paul09`, otherwise `Julie09`.
4. Any other English version is an error.

`Engine::open_voice` (English) and `EngineSpa::open` (Spanish) carry the era. The CLI (`--voice paul|julie|violeta
--data <install>`) falls back to `data-julie`, the 2009 Julie directory name. Per-era state:

| Piece | Mechanism |
|---|---|
| Unit selection | `unitsel09.rs`, `Selector::new_era`, `Word::from_raw_era`, `Classes::build_era`, `unitsel_tables_{julie09,paul09,viol09}.rs` |
| Prosody | `Sentence::era`, `*_era` functions in `prosody.rs` |
| Wave | `wave::generate_phrase_traced_era` (no join smoother) |
| English text front end | thread-local era in `text\cstr.rs` (`set_era`; `mem()` consults per-era table chunks keyed by the 2014 VA: `text\tables_julie09.rs`, `tables_paul09.rs`), `Fe::load_era`, `initial_era`, era branches in the normalizers |
| English linguistic front end | `ling\lay.rs` (`Lay::of(era)`), `ling\phtabs.rs`, `ling\tables09.rs`, `engine09.rs` (sentence builder), `Lexicon::open_era` |
| Spanish front end | `ling_spa\spa09.rs`, `text_spa\norm_phone09.rs`, `FeSpa::load_era`, `LexiconSpa::open_era` |

Table generators (not in the repository): `tools\gen_unitsel_tables_09.py` (regenerates byte-identical),
`gen_era_tables.py` with `era_spec_<build>.py` (from Frida `.rdata`/`.data` snapshots), `gen_ling09.py`.

## Unit index (`unit-*.idx`) and primary record

Header: same as ver.2013; the bytes after the dat name are nonzero in 2009 (2, 3, 100, 0).
`us_ReadIdxHeader` (Julie09 `FUN_1001b600`, Paul09 `FUN_1001b870`, Violeta09 `FUN_10004b60`, 2014 0x10019e80):
a file starting `ver.` sets pool+0x66 (2014: +0x62) and selects the columnar layout. Julie09/Paul09 then require
exactly `ver.2009` (`DAT_10078314` / `DAT_100762e4`), 2014 exactly `ver.2013`, Violeta09 accepts any `ver.`. Without
`ver.` the engines read interleaved legacy records (`FUN_1001aec0`); no shipped bank uses it. The 2014 engine
therefore rejects a 2009 bank by itself.

Per-unit columns, 20 bytes per unit (reader `FUN_1001b050`, identical in all three DLLs; pool struct +4 against
2014, stride 0x68 against 0x64):

| Column | Pool offset | Bytes | 2009 | 2014 |
|---|---|---:|---|---|
| file index | +0x48 | 1 | same | same |
| flag | +0x4c | 1 | 0/1, about 90% 1 in gen; **new** | none |
| label | +0x44 | 5 | `[L1, C, R1, posbits, flags]` | 7: adds L2 and R2 |
| duration code | +0x40 | 1 | about 129 values | same |
| three boundary sets | | 12 | same | same |

The 19-byte primary record parses identically (`us_ReadPrimaryRecord2013` only moved struct offsets). The
`ver.2009` bank file is `39 B/unit` in total (19 primary + 20 columns). Port: `bank.rs` `IdxLayout::{V2013, V2009}`,
`Bank::version`, `Bank::label5`, `Bank::flag` (2009 only; `label` stays empty); all three banks load with exact
size checks.

## tree2 (`src/tree2.rs`)

Paul09 reads every CART as `.tree2`: `ttsdata\tree\{duration,pitch}\*.tree2`, `dict-eng\{atmt,engbi,poly,sbd}.tree2`.
The 2014 `pro_LoadTree3` rewrites `.tree2` to `.tree3`; the formats differ (a tree2 file has no
`u16 n_nodes / u8 dim / u32 n_set_total` header; `qbt.tree2` starts `01 44 00 00`; `atmt` 449 KB against 292 KB,
`engbi` 1.6 KB against 12 KB). Engine readers: scalar `FUN_10001850` (node `FUN_10001650`), vector `FUN_10001d10`
(node `FUN_10001ac0`), atmt `FUN_10001910`, walkers `FUN_100015f0` / `FUN_10001a40`.

A file is one pre-order tree; every node carries its own value(s):

| Tree | Node layout |
|---|---|
| scalar | `feat u8, kind u8, thresh i16, value i16, nset u8, set[nset] i16, b1 u8, b2 u8` |
| vector | `feat u8, kind u8, thresh i16, nval u8, vals[nval] i16, nset u8, set[nset] i16, b1 u8, b2 u8` |

`b2 != 0`: the yes subtree and then the no subtree follow. `b1` is read and dropped. The walk equals tree3's
(`D`: set membership, otherwise `thresh < x` goes to "no"), so `tree2::parse` converts a file into a `tree3::Tree`
with leaves as negative child references. Vector leaves copy the root's `nval` shorts from the leaf's place in the
engine's shared value buffer (vals and sets interleaved in pre-order). The engine ignores trailing bytes.

- `atmt.tree2` is 27 records of `u32 length` + scalar tree (`load_multi`).
- poly, sbd, engbi and the duration and pitch `*t` trees are scalar; pitch `*f` trees are vector (dim 12).
- Julie09 and Violeta09 use tree3 as in 2014 (`atmt.tree3` = u32 count + 27 tree3). Violeta09 `spabi.tree3` loads
  with the 2014 loader. `cepdist.tbl` is plain (English layout) in Julie09, Paul09 and Violeta09, with no `.rd`
  remap.
- Loading: `Trees::load_tree2` (Paul09 prosody), `Lexicon::open_era` (Paul09 poly/engbi/atmt).

## Differences from 2014, per stage

### Text front end (English: Julie09, Paul09)

Julie09 and Paul09 differ from each other mostly here (`fe_Tokenize`, `fe_NormalizeTokens`, about 10 large
normalizers), so they are separate builds. `fe_Tokenize` is 1.6 KB smaller than 2014. Details in
[FRONTEND_TEXT.md](FRONTEND_TEXT.md); the era-specific rules found through the corpora:

- Normalizer chain: Julie09 swaps `digits_a` and `digits_b`; Paul09 has no `digits_a`. The dispatch table has 18
  entries. The math normalizer is live (`norm_math::math09`, Julie09 0x10037c80 + helpers); `*` reads "times"
  (Julie09 0x10050660).
- Julie09 `norm_digits::digits_a` returns -1 for a group that is neither spelled nor digits ("401K-2345-...");
  an empirical rule without decompile support.
- Date normalizer (Paul09 `FUN_10033b70`, Julie09 `FUN_10033d10`; validators `FUN_10034ff0`/`FUN_100351d0`;
  context `FUN_10035e30`/`FUN_10036050`): the context check runs only for patterns 7/0xc/0xd/0x11/0x15 and the
  hint is sticky 1; case 0xc has no length < 3 split, so "AP", "JA", "JE" read as months; case 0xd separators are
  `/-.` (0x100802a8); case 0x11 has an extra clause.
- Abbreviations `norm_n15` (Paul09 `FUN_1002fc40`, Julie09 `FUN_1002fa10`): honorific handling only for "Mr" and
  "Ms" ("mrs." falls to the tables); matched-trigger failures return -1; conjunctions only "&" and "and";
  `table09`.
- Paul09 `norm_symbols` (`FUN_1004f790`): only `(` is handled in that branch, not `[`.
- Year words (Paul09 `FUN_10036d10`, Julie09 `FUN_10036ec0`): values >= 10000 read digit-pair-wise.
- Time (Paul09 `FUN_10052b60`): own pattern table without `NmD` and a 15-entry zone table, so "5 PM EST" keeps
  `EST`; Julie09 adds a bare `N` pattern (declined like `N:N`). See FRONTEND_TEXT.md section 5.5.
- VTML (`handle09`, `TAGS09`): five tag stacks (no kind 7), open marker `a2 fe`, 5 channels, 29-tag table
  0x1007a4e8; `partofsp` and `phoneme` consume their content, `x-ntsampa` is verbatim. The three voice-variant
  steps only write TextToPreprocessInfoFile dumps (no audio) and are not ported.
- Also era-branched: street/city/PO box/currency/numword/symbols/names/phone label/tokenizer/digits/plural,
  a fullwidth map.
- Caveat: the table comparison tool counted a string as unchanged if it appeared anywhere in the 2009 image, so
  other string constants in 2014-addressed modules may differ on texts no corpus covers.

Violeta09 (`text_spa`): era branches in tokenize/numexpr/date/phone/lexicon/lts/analysis, `norm_phone09.rs`;
the 2009 lexicon/LTS pass `FUN_1001f3d0` is much shorter than 2014's (8 of 188 LTS words differ).

### Linguistic front end (English)

- Word record: smaller, with a 5-byte phone context `{L, C, R, f1, f2}` (Julie09/Paul09 offsets: n_phones
  +0x2e4, dur +0x0a, ctx +0x90, edge +0x1d5, flag +0x2fc, brk +0x2fd; Violeta09: record 0x47c, +0x465, +0x0e,
  +0xd6, +0x2ca, +0x478, +0x479, context byte 4 decimal-coded). Word record 0x300 B in Julie09.
- Words/breaks: 13-feature break CART (`0x100129d0`), threshold 400 Julie09 / 500 Paul09; words
  `0x10016690`/`0x10012b30`; contexts `build_phone_contexts09` (`0x10016ed0`, new function); suffix stripper
  `suffix_strip09` (`0x10009da0`); 2009 tagger rules and morph post-rules.
- 2009 emits `]` where 2014 emits `\` in some break positions (Paul09).
- Paul09 tree structs in `spk` are 0x1c bytes and the predictors take the root pointer.

### Prosody

See [PROSODY.md](PROSODY.md) "2009 builds". Real differences: BRK_GRAVE is not a final break
(`is_final_break(era, b)`); the target-feature duration code uses the whole phone's duration and the F0 list the
target's own half flag (`target_feature_era`); five tag stacks, rate kinds 0..2 (`apply_tag_spans_era`); pause
channel clamp (`clamp_channel`); Violeta09 own phone tables (`*_SPA09`).

### Unit selection

See [UNITSEL.md](UNITSEL.md) "2009 builds". 5-byte labels and 7-byte class keys, no preferred-candidate pass or
domain handling in lookups, different back-off thresholds, a 2009 lattice, a different contiguity score, f32
evaluation orders taken from the asm. Paul09 selection equals Julie09 with `tib+0x54`.

### Waveform

See [WAVEFORM.md](WAVEFORM.md) "2009 builds". No join smoother (decode, then straight to overlap-add); unit
records are a struct of arrays; channel clamp 0..3 / 0..3 / 0.

### Formats

All 13 file/buffer formats and the lip-sync log are unchanged in 2009 (see [WAVEFORM.md](WAVEFORM.md) section 8).
The Spanish lip-sync log uses Violeta's own phone names (`WE`, `OH`).

## Struct layout differences (hook offsets)

| Item | Paul09 | Julie09 | Violeta09 |
|---|---|---|---|
| SS pointer | TIB+0x54 | TIB+0x4c | single speaker slot |
| channel byte | TIB+0x42c94 | TIB+0x42c8c | (not recorded) |
| `pro_LoadProsodyTrees` argument | `param_1 + 0x498` (unverified for Paul09) | `param_1 + 0x498` | (not recorded); 2014: `+ 0x494` |
| tag stacks / span lists | 5 | 5 | 5 |
| sentence types | 4/5/6 | 4/5/6 | 4/5/6 |

Unit records are a struct of arrays in all three. Violeta09 has no domain-0xc branch or kind-7 pass.

## Code diff against 2014

Function maps (`tools\oldpaul_funcmap.py <old> <new> <out> [suffix]`) and decompile-level cross references
(callees renamed through the map, data addresses and casts masked):

| Pair | same | changed | unmatched |
|---|---:|---:|---:|
| Julie09 -> eng14 | 488 | 468 | 130 |
| Paul09 -> eng14 | 441 | 490 | 146 |
| Violeta09 -> Violeta14 | 471 | 341 | 85 |
| Julie09 -> Paul09 | 768 | 221 | 97 |

Paul09 -> 2014 by matching kind: 1015 functions in 2009 against 1034 in 2014; exports 56, identical normalized
code 548, call graph 304, fuzzy 26, unmatched 81. Changed: 27 `us_*`, `pro_PredictDurations`,
`pro_PredictPitch`, `pro_PitchFeatures`, `pro_DurationFeatures`, `fe_Tokenize`, `fe_NormalizeTokens` and most
normalizers, 23 `wav_*`. Diffs of up to 4 lines are mostly decompiler noise; struct offsets do move. Ghidra
programs of the 2014 and 2009 Violeta DLLs share a name, so MCP calls must pass the project path
(`/2009_vt_spa_violeta16.dll` against `/vt_spa_violeta16.dll`).

## Verification

Raw text to PCM against the real 2009 DLLs, `tests/speak_old_parity.rs` (`speak_old_match_engine`, asserted):

| Build | Texts | Result |
|---|---:|---|
| Paul09 | 2741 | 2741/2741 |
| Julie09 | 2741 | 2741/2741 |
| Violeta09 | 1366 | 1366/1366 |

References: `harness\capture_old.py` writes `harness\out_old\<build>\<name>.wav` (English: speak corpus; Spanish:
`corpus_spa`). Per-stage captures were hooked with Frida; hooked WAVs equal the unhooked engine for every
capture (Julie09 2971, Paul09 3170, Violeta09 5532, 0 Frida errors, deterministic with pointer slots masked).

| Stage | Tests | Result |
|---|---|---|
| English text front end | `fetext_{julie09,paul09}_parity`, `initial_{julie09,paul09}_parity` | fetext 2245/2245 and 2264/2264; initial 2197/2197 both |
| English linguistic | `feling_{julie09,paul09}_parity`, `engine09_parity` | all 10 stages 100%: words/breaks/contexts 1554, morph 1554, merge 4542, lts 8807 (Paul09 8790), spell 2080 (2085); ling words, durations, tones, F0 and unitsel ctx exact |
| Spanish front end | `fetext_viol09_parity`, `initial_viol09_parity`, `initial_viol09_spans`, `lts_viol09_parity`, `feling_viol09_words`, `feling_viol09_analysis` | fetext 1418/1418, records 89605/89605; initial 1418/1418; spans 6/6; lts 564/564; analysis 13766/13766 sentences; words/breaks/contexts 11034/11034 |
| Prosody | `prosody_{julie09,paul09,viol09}_parity` | 100% on all 11 record types; durations Julie09 100630, Paul09 100775 (tree2), Violeta09 455427 |
| tree2 | `tree2_parse` | 20 files and 27 atmt records parse to the exact byte; predictions exact via the Paul09 prosody suite |
| Unit selection | `unitsel_{julie09,paul09,viol09}_parity` | 0 mismatches: Julie09 6,460 + 119,613 targets, Paul09 19,031 + 135,518, Violeta09 977,798 |
| Waveform | `wave_{julie09,paul09,viol09}_parity` | PCM from captured records: Julie09 92/92 + 42/42, Paul09 92/92 + 42/42, Violeta09 1416/1416; records from selection 0 differing of 9183 / 10739 / 508661 |
| Formats | `wave_*09_parity::formats_parity`, `formats_parity_viol09` | 13 formats and lip-sync log 92/92 (Julie09, Paul09), 1416/1416 (Violeta09); 1600 runs, 0 failures |

Full `cargo test --release` after the merges: 39 binaries green, 0 warnings; the 2014 suites are unchanged.
Harness: `harness\vtfmt_old.c` (`build_vtfmt_old.bat`, env-driven DLL/root/speaker/lang) with
`run_vtfmt_old.py`.

## Open items

- Spanish lip-sync log (2014 and 2009): library only, the CLI does not expose it; the 2014 Violeta port never
  had a lip-sync or formats test either.
- `.his` selection history in 2009: no capture, untested.
- vtns-rs issue #3's own bank (671,379 units) is a third Julie build with no reference output here.
- `clamp_channel` is implemented per build but not yet called by the engine.
- String constants in 2014-addressed text modules that no corpus exercises may still differ from 2009.

## The 2004 engine (Paul04)

| Build | DLL | Linked | Speaker | Index | Trees | License check |
|---|---|---|---:|---|---|---|
| Paul04 | `2004_paul\lib\vt_eng.dll` (581,632 B, VC6, 36 exports) | 2005-03-31 | 1 | `ver.2004` | tree2 | none exported |

- Data: `ver.2004` has the `ver.2009` index layout byte for byte (19-byte primary record + 20 bytes of columns);
  `Era::detect` picks `Paul04` from the version string alone. Trees are `.tree2` as in Paul09. `dict-eng` lists
  have no `_sort` suffix; there is no tpp dictionary and no `wab.txt2`; `ENGBI_comp.tree2` ships but is never opened;
  `city1.txt2`..`city4.txt2` hold city names by word count. The `.txt2` container has no count field (lines are
  counted, `Txt2::open04`).
- API: same signatures as 2014 (`TextToBuffer` 13 arguments; speed stored as `10000 / speed`), but no
  `VT_SetCommaPause`, `VT_SetEmphasisFactor` or `VT_CheckLicense`: `--comma-pause` and `--emphasis` do nothing.
- VTML: ten `<vt_*>` tags (`<vt_pitch=N>..</vt_pitch>`, `<vt_speed=N>`, `<vt_volume=N>`, `<vt_pause=ms>`,
  `<vt_break=N>`, ...), stripped by the initial pass into five span lists; `<vtml_*>` tags are read aloud.
- Same pipeline as 2014 at every stage, with other struct layouts (text record 0x88, morph 0x6c, word record
  0x2fc, syllable 0x28) and three stages of a different design:
  - Text front end (`src/text04/`): `fe_NormalizeTokens` (0x1004d720) reads tokens into a look-ahead buffer and
    offers it to a fixed chain of recognizers (symbols, currency, URL, time, date, phone, digit codes, address,
    ordinal, math, units, number words, symbols, abbreviations, letter codes) that consume any number of tokens
    and queue their words; a failing recognizer rewinds the buffer. One module per recognizer
    (`norm_*.rs`), tables generated from a `.rdata`/`.data` snapshot (`text04/tables.rs`).
  - Unit selection (`src/unitsel04.rs`): an older design (`us_SelectUnitsForPhrase` 0x1001c100) with 30
    candidates per target, an exact 6-byte class key and a 9-byte half-phone back-off, join cost = cepstral
    distance + two class-distance terms weighted per boundary type + 0.2, target weight 0.1, mean-based beam pruning.
  - Waveform (`src/wave04.rs`, `src/engine04.rs`): its own unit-record builder, overlap-add and PSOLA.
- Linguistic front end (`src/ling04/`): own tagger, LTS (no short-word "E" rule), morphology and merge; breaks are
  rule-based. Prosody: the Paul09 algorithm with 2004 tables
  (`prosody_tables_paul04.rs`) and its own F0 smoothing (`smooth_f0_04`).
- Glue: `engine04::front04` is `fe_NextSentence` (0x10019e70): word positions, rate seeds
  (`pro_SeedWordRates` 0x1001a410) and tag spans (`pro_ApplyTagSpansToWords` 0x1001a130).
  `Engine::open_voice` loads the 2004 front end and classes (`engine::Paul04`) and `speak` runs it.

Verification (all asserted in the normal suite):

| Stage | Test | Result |
|---|---|---|
| Raw text to PCM | `e2e_text_paul04_parity` | 2909/2909 against `out_old\paul04` |
| Captured WA to PCM | `e2e_paul04_parity` | 2891/2891 (18 texts outside cp1252 skipped) |
| Text front end | `fetext_paul04_parity` | initial 5067/5067; normalization 5067/5067 lines, 53749/53749 records |
| Linguistic front end | `feling_paul04_parity`, `lts_paul04_parity`, `morph_paul04_parity` | lookup, copy, morph, analysis 5186/5186; tp and WA out 13871/13871; SS passes 5174/5174 |
| Prosody, rates, tree2 | `prosody_paul04_parity`, `rates_paul04_parity`, `tree2_paul04_parse` | 100% |
| Unit selection | `unitsel_paul04_parity` | 2909 captures, 164,160 targets, 0 mismatches |
| Waveform | `wave_paul04_parity` | 2909/2909 PCM from captured selections |

Open items: the say-as dispatcher `FUN_10043bb0` and its ten handlers are not ported; they only act on
`<\xa1\xddX>` marker tokens, which no capture contains. Synthesis from captured WAs (`synth-wa`) is not wired for
2004. The output formats other than 16-bit WAV are not verified against the 2004 DLL.

## History

- 2026-09-28: inventory of the Paul 2009 install; the 2014 DLL and the port reject its data (tree2, `ver.2009`).
- 2026-10-05: decision to port all three 2009 engines (vtns-rs issue #3: a 2009 Julie bank panicked in `bank.rs`); dumps and function maps of five DLLs.
- 2026-10-05: per-build Frida capture infrastructure; index header, 5-byte label columns and `Era` plumbing; tree2 parser. Correction: the 0/1 column is the flag and a duration byte exists.
- 2026-10-05 01:20-01:55: prosody, waveform and unit selection ported bit-exact for all three builds.
- 2026-10-05 02:40-03:38: English front ends (Julie09, Paul09) and Spanish front end (Violeta09) ported; engine wiring at 03:45.
- 2026-10-05 05:05: text front end complete; end to end Paul09 2735/2741, Julie09 2733/2741, Violeta09 1366/1366.
- 2026-10-05 05:55: 14 remaining texts fixed (date, abbreviations, `(`, year words); 100% end to end, asserted in the normal suite.
- 2026-10-09: time-pattern tables added to the Paul09/Julie09 table specs (a zone probe found Paul09 expanding
  "5 PM EST"); `corpus_fetext\timezones.txt`, 148/148 on all three English builds.
- 2026-10-15: the 2009 Paul/Julie user dictionaries hold nine `A` aliases, which the port had never resolved
  ("voicetext" was dropped). Aliases are now resolved at load; see FRONTEND_TEXT.md section 6.
- 2026-10-15: assessment of the 2004 Paul install (data compatible, code different); prosody, unit selection
  and waveform ported bit-exact from captures.
- 2026-10-16: 2004 linguistic front end, then the text front end (recognizer chain) ported; raw text to PCM
  2909/2909; CLI wired.
