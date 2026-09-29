# Violeta (Spanish) reverse engineering

Working log for `vt_spa_violeta16.dll` (32-bit, June 2013) and its data (`data-violeta\M16`,
`data-common\dict-spa`), ported as `Lang::Spa` in this crate. Dated HANDOFF-style entries, newest last;
the two batch 1 capture-harness reports follow as appendices.

## 2026-09-27 setup (done)

- **API.** 64 exports, the English API with `_SPA` suffixes (no `VT_CheckUserDict_SourceNorm`/`TargetPhon`).
  Speaker slot 0 only (`VT_SetPitchSpeedVolumePause_SPA` clamps the slot to 0..1; loading slot 1 fails with 8).
  Channel fields sit 4 bytes lower than English: pitch `+0x4cf0`, speed `+0x4cec`, volume `+0x4cf4`,
  pause `+0x4cfc`; same clamps (pitch 50-200, speed 50-400, volume 0-500, pause <= 0xffff).
- **License.** `VT_CheckLicense_SPA` (0x10014b10) is a stub returning 0. The verification file is still read
  (`FUN_10013d40`, callers `FUN_10013be0` and the license-info exports) and a Spanish demo notice exists at
  0x100643a0, but on this machine dates and phone numbers are expanded and no notice is spoken, so the build
  behaves licensed. Recheck if an output looks truncated or unexpanded.
- **Harness.** `harness\vtprobe.exe` takes `VT_LANG=SPA` and `VT_SPEAKER=0` (plus `VT_DLL`, `VT_ROOT`).
  `harness\capture_violeta.py` captures every `harness\corpus_spa\*.tsv` line into `harness\out_violeta\`
  (WAV only, no Frida yet).
- **Corpus.** `harness\fetch_nws_spanish.py` pulls the NWS Spanish product translations
  (weather.gov/translate data at `/source/translate/data/<office>/es/<PIL>/translations.json` + files; serial,
  1 s gap, capped) into `harness\nws_es\` (80 products: MFL, BRO, TBW; ZFP, CWF, SRF, HWO, SPS, NPW, FFA, FFW,
  FLS, FLW, SVR, SVS, TOR, MWW, SMW, RFW). `harness\build_corpus_spa.py` makes `corpus_spa\nws_es.tsv`:
  1327 paragraphs (U+202F -> space, BOM dropped). Captured: 1327/1327 WAVs, 715.6 min.
- **Unit bank.** The port's `Bank` + Shorten decoder open `data-violeta\M16` unchanged: pools gen (65432,
  GEN) and etc (2163, ETC), all 67595 units decode (108,729,602 samples, FNV-1a 6f84660e16fbb1c3). Not yet
  checked sample-exact against the engine. Phone labels print with English names: Violeta has its own phone set.
- **Data differences found so far.**
  - `ttsdata\dist_tbl\cepdist.tbl.rd` (the DLL names this file): u16 n = 256 (English 1024), then the f32
    lower triangle as in English, then 2050 extra bytes (looks like u16 + 512 f32). Tail meaning unknown.
  - Duration trees `vdib`, `vdif`, `vmono` + the five consonant trees (English: `vdi`, `vlong`, `vsch`,
    `vshort` + consonants). Pitch trees have the English names.
  - `dict-spa`: `spattsdict` + `hashidx/hashcont/hashparams` (lexicon), `tppdict_spa` + `_spa_tpp` hash
    files, `spabi.tree3` (the DLL string says `spabi.tree2`: check which it opens), `wab.txt2`. No
    abbreviation/street/city/sbd tables and no `exceptdict`, `atmt.tree3` or `poly.tree3`.
- **Code overlap.** Only 4.8% of Violeta's 32-byte `.text` windows appear verbatim in `vt_eng.dll` (embedded
  addresses break byte matching), so functions must be mapped in Ghidra.

## 2026-09-27 batch 0: function map (done, renames pending)

- `tools\violeta_funcmap.py` -> `tools\violeta_funcmap.tsv` (spa, eng, eng_name, kind, score, sizes).
  Offline (pefile + capstone): Ghidra's `bulk_fuzzy_match` returned nothing and inline scripts are disabled
  (`GHIDRA_MCP_ALLOW_SCRIPTS` unset). Function starts = exports + direct call targets: 898 in Violeta, 1034 in
  English. **591/898 matched**: 1 manual, 56 by export name, 181 exact (normalized instructions), 238 by aligned
  call-graph propagation, 115 fuzzy (mutual-best mnemonic 4-gram Jaccard >= 0.55). 110 carry an English name
  (names harvested from reveng notes + port comments; many more English names exist only in the Ghidra DB).
- Exact matching alone found none of the 41 documented unit-selection functions: the two builds share logic but
  differ in struct offsets, helpers and register allocation. Mnemonic 4-grams + call graph recover 20/41; the
  half-phone ordering helpers (`us_MakeHalfPhoneOrders`, `us_BuildHalfKeys`, `us_NRHeapSortHalfKeys`) and the
  sorts are among the misses, and may not exist in this older engine in the same form.
- `cepdist.tbl.rd` decoded from `FUN_10005a40` (= English `us_ReadCepDistTable`, manual anchor): English layout
  (u16 n, f32 lower triangle) + optional u16 m + m u16 remap entries. Violeta: n = 256, m = 1024, so the 1024
  cepstral codewords map onto 256 distance rows (2 + 131584 + 2 + 2048 = 133636 bytes). `FUN_10005c10` builds
  the 256-entry scalar-distance table (the port's `ScalarDist::build`).
- **Renames applied** (2026-09-27, after a Ghidra restart picked up `GHIDRA_MCP_ALLOW_SCRIPTS=1`): the
  274 `vt_eng.dll` Ghidra names are exported to `tools\vt_eng_ghidra_names.tsv` and used by the matcher (176 of
  the 591 matches now carry an English name). One inline script renamed 100 Violeta `FUN_*` functions (20 already
  had names and were kept) and put a plate comment on 586 matched functions: English address, name, match kind,
  score. 5 matched starts have no Ghidra function. Programs saved.

## 2026-09-27 batch 1: capture harness (done)

Two Sonnet agents; full logs in `C:\tmp\vtwork\v1_front\REPORT.md` and `C:\tmp\vtwork\v1_back\REPORT.md`.
English scripts and exes untouched; everything new is `*_spa`.
- **Front end.** `hooks_fetext_spa.js` + `capture_fetext_spa.py` + `vtfe_multi_spa.c/.exe`
  (`build_fetext_spa.bat`); `hooks_feling_spa.js` + `capture_feling_spa.py` + `tools\fedump\fedump_spa.c/.exe`
  (`build_spa.bat`, speaker slot 0). `corpus_spa\probe_front.txt/.tsv`: 39 synthetic lines. Outputs
  `out_fetext_spa` (1327 + 39 lines) and `out_feling_spa` (1366 files), 0 Frida errors at `--jobs 12` (24 hit
  attach timeouts).
- **Back half.** `hooks_spa.js`, `hooks_prosody_spa.js`, `hooks_wave_spa.js` + `capture_initial_spa.py`,
  `capture_unitsel_spa.py`, `capture_prosody_spa.py`, `capture_wave_spa.py` (stock `vtprobe.exe`). Full corpus in
  `out_initial_spa`, `out_unitsel_spa` (1366), `out_prosody_spa` (1378), `out_wave_spa` (1416, incl. 50 param
  cases). About 12.5 GB. All hooked WAVs SHA1-identical to `out_violeta`.
- **Lead fix.** Agent A had `WORD_BASE = 0xa4c` in `hooks_feling_spa.js`; agent B `0x64c`. `fe_AssignPhraseBreaks`
  (0x1001e750) reads word 0 n_phones at ss+0x727 and brk at ss+0xb88, so 0x64c (same as English) is right. Fixed
  and recaptured `out_feling_spa` (old copy in `C:\tmp\vtwork\v1_front\out_feling_spa_base0xa4c`): 33105
  sentences, n_phones plausible for every word; last-word brk 0x46 in 23628.
- **Layout.** WA record stride 0xa8, header 0xc, text +0x24 (English 0x94/0x14/0x34). Word 0x540 bytes (English
  0x3c0), syllable 0x28 (0x1e); n_syl/n_phones/ctx/brk at +0xda/+0xdb/+0xdc/+0x53c. Target, candidate, primary,
  record, feature and phrase structs identical to English, only container bases moved (table in v1_back report).
  Channel fields confirmed by code: uniform -4 shift.
- **Break bytes** shifted -0x14 from English: `F G H I J` (0x46..0x4a) for English `Z [ \ ] ^`.
- **Lexicon/morphology** is one function in Violeta (`FUN_10021bf0`) where English has five sub-stages, so
  tp/lookup/copy/morph/wh/fw records are not captured.
- **Not captured yet:** word text/pos/domain and phone code/mark (bytes in `word.raw`), syllable pointers
  +0x1c/+0x20, VTML tag-stack offsets, syllable sub-array in the front end, LTS/spell addresses, spabi tree2 vs
  tree3. No phone-name table found; phone-class table at 0x10067114 has 44 entries. The cepdist remap is not used
  at any hooked selection site.

## 2026-09-27 batch 2: back half port (done)

Two Sonnet agents on plain copies (`C:\tmp\vtwork\v2_pro`, `v2_sel`), merged by the lead; `src\lang.rs` (`Lang::{Eng, Spa}`)
added first. Full `cargo test --release` green after the merge, no warnings, all English suites unchanged.
- **Prosody** (`src\prosody_spa.rs`, `prosody_tables_spa.rs`, `tests\prosody_spa_parity.rs`), 1366 + 1378 captures:
  durations 453660/453660, tones/raw F0/smoothed F0 190994/190994, target features 489167/489167, word rates and
  sentence pauses 11055/11055, pause gaps 89555/89555, spans 8/8. `pro_PredictDurations` is `FUN_1001eb80`
  (features `FUN_1001e970`, 11 values, edge sentinels 50/51); one 8-way class table (0x1006740c) picks among 8
  duration trees (no vshort); tree loader `FUN_10001050`. Syllabifier `FUN_1001e1e0`: syllable +0x1c/+0x20 point
  into per-word phone/pos arrays at word+0x408/+0x4d0. Smooth F0, tag spans, sentence pause, target feature reused
  unchanged; pause samples needs the Spanish break bytes.
- **Unit selection + wave** (`src\unitsel.rs`, `classes.rs`, `wave.rs` extended; new `unitsel_tables_spa.rs`,
  `tests\unitsel_spa_parity.rs`, `tests\wave_spa_parity.rs`): 2732/2732 captures, 978334 targets, 0 mismatches;
  records 14030/14030 phrases (509019 records); PCM 1416/1416 bit-exact both from captured records and from
  selection (incl. 50 param sweeps). Differences: 2-level stress; no context/centre substitution in the lattice;
  acceptance units > 2 (English > 9); 11-byte half key (collapse class inserted); TargetCost without the TQ case and
  with an unconditional pad bonus; join cost summed `cep*wc + db*wb + df*wf + 2`, no C384/vowel-adjacency terms,
  +20 partial penalty on the right half; pad/break bytes -0x14.
- **Cepdist remap** is applied in `us_ViterbiJoinStep` (0x1001f9d0): vdb+0x18 remap table, vdb+0x1c flag; both
  units' `spec & 0x3fff` go through it before the n=256 triangle. Ported inside `CepDist::get`.
- **Open.** Spanish lip-sync (no phone-name table); `k7` per-word array is 0x40 short of the stride pattern
  (unused by this corpus); `hooks_spa.js` `phrase_words` holds each phrase's start word, not a count (tests work
  around it); domain 8 and the heapsort fallback unexercised (as in English); engine glue waits for batch 3/5.

## 2026-09-27 batch 3: linguistic front end (done)

Three Sonnet agents on plain copies (`C:\tmp\vtwork\v3_lex`, `v3_lts`, `v3_words`), then the lead redid letter-to-sound
alone (user: no more sub-agents for now). Lead scaffold `src\ling_spa\mod.rs` (WaSpa/SsSpa) + `pub(crate) use byte_struct`
in `ling\mod.rs`. Full `cargo test --release` green, no warnings.
- **Words/breaks/contexts (agent WORDS).** `src\ling_spa\{words,syllab,breaks,context}.rs`, `tests\feling_spa_words.rs`:
  11035/11035 sentences per stage, glue to prosody/unitsel 11035 sentences / 89469 words. `spabi.tree3` is the file used
  (standard tree3, 37 nodes, 14 features). SYL_BASE 0x2134c confirmed. The word builder's kind7 is `*(ss+0x82dec)`, not
  TIB+0x15d730 (that one is prosody's span array). Extended capture moved to `harness\out_feling_spa_v3` (+
  `hooks_feling_spa_v3.js`, `capture_feling_spa_v3.py`). Still to fold: ~15 offsets kept in words/breaks/context.rs;
  `wr::FWD_N` at word+0x61b lies past the 0x540 record (reads the next word): check.
- **Letter-to-sound (lead, translated).** The agent's rule rewrite (174/180, 94.4% of corpus LTS words) was dropped.
  `src\ling_spa\lts.rs` translates `FUN_10022810` (stage 1: casefold, collapse mm/ss/tt/bb/dd/ff/gg/kk/pp, letter
  pattern T/D/S/W/w/B/1-3, syllable breaks, stressed syllable: accent, else penult/final rules, monosyllables stressed
  unless in the 10-word list at 0x1006866c), `FUN_10023930` (stage 2: letters + pattern -> codes), `FUN_100250e0`
  (degeminate + AA/OH/EH + UH0 -> AU/OU/EU, T+CH -> CH) and, for lexicon prons, `FUN_10025700` + `FUN_10022430` +
  `FUN_10025820` (codes -> names, C/V/B syllabification, names -> codes). Phone names = the 63-entry table at
  0x10067e58 (`lts::NAMES`).
- **Analysis (agent LEX + lead).** `src\ling_spa\{analysis,lexicon,tables_spa_chars}.rs`. Lead rewrote `lexicon_pass`
  from the disassembly: the plural retry flag is per word (reset at 0x10021cbc); LTS runs on the casefolded (possibly
  's'-stripped) key; "-mente" adverbs run LTS on the raw stem and on "mente" and join them with a syllable break (99,
  `"%c%s"`), with the 67-word exception list at 0x10068304; a stripped plural appends S (0x29, `"%c"`), twice on the
  -mente path; any LTS or name-lookup failure truncates the WA word count and returns -1. Lead ported the multi-word
  TPP join `FUN_100254b0` (joins with '-', 'A' origin, gap <= 1, "guión" 'S' tokens stepped over, <= 7 words, <= 29
  chars, longest match wins). Lexicon payload fixed: flags, pron up to the first 0xff, then an optional number (LEX had
  it inverted; broke "marketing").
- **Parity.** `tests\feling_spa_analysis.rs`: 13767/13767 sentences, 89469/89469 words (WA records byte-exact);
  `lexicon_pass` alone on the 184-word LTS list (`harness\out_lts_spa`, `hooks_lts_spa.js`, `corpus_spa\lts_words*.txt`):
  564/564 passes, including every word the agent's rewrite missed.
- **Paths.** Spanish tests now use `VTPAUL_HARNESS` / `VTPAUL_VOICES` (default `..\harness`, `..\all_voices`); captures
  from agent workspaces moved into `harness\` (`out_lts_spa`, `out_feling_spa_v3`, `out_spans_spa` + their hooks).

## 2026-09-27 to 2026-09-29 batch 4: text front end (in progress)

- **Module.** `src\text_spa\`:
  - the driver `FUN_10031060` (`normalize.rs`) is a 13-slot chain, then the word handler `FUN_10042180`;
  - `fe_InitialTextProcess` (0x10007730) is `initial.rs`.
- **Parity.** `tests\fetext_spa_parity.rs` (NWS corpus):
  - 2026-09-29: records 84752/89469 (94.7%), lines 845/1366;
  - `tests\initial_spa_parity.rs`: 1366/1366.
- **Ported exactly:**
  - normalizers: math `FUN_1002e6c0`, numeric expressions `FUN_10034270`, zone headers `FUN_100426b0`,
    symbols `FUN_10042b20`, date `FUN_1002b410`, fractions `FUN_1002fd60` (all 4 shapes), the cardinal/roman
    reader `FUN_10033db0` with the ordinal suffixes;
  - partly: time `FUN_10045e70`, units `FUN_10031720`, the phone gate, URL;
  - the gender fallbacks `FUN_100286a0`/`FUN_10033c00`;
  - VTML tag parsing, including the regex gates, the x-cmu phonemes and self-closing tags.
- **Findings:**
  - The word handler consumes every non-word token silently.
  - '(' is spoken "cerrar paréntesis" and ')' "abrir paréntesis" (engine data swap, kept).
  - The timezone table maps "P.S.T." to "CST" (engine data bug, kept).
  - A number range ("5 a 10 nudos") goes through the units range path (`FUN_10033050` "NUMS" ->
    `FUN_100324d0`), and both the last number and the unit word get the plural flag.
  - `FUN_100338b0`'s tables `DAT_10082060/64/68` are zero in the static image.
  - An IP address without a port fails the URL gate and is read as numbers.
- **Open:**
  - currency;
  - say-as, part of speech and the other phoneme alphabets;
  - the Spanish VTML normalizer `FUN_1003d710`;
  - the remaining unit, date and URL shapes;
  - VTML spans applied to per-word rates.
- **End to end (2026-09-29).** `src\engine_spa.rs`, `vtpaul --voice violeta`.
  - `tests\speak_spa_parity.rs`: 1032/1327 NWS WAVs byte-identical from raw text.
  - The remaining mismatches trace to the open normalizer items above.

## Appendix A: batch 1 front-end capture log


Working log, agent A, batch 1 of the Violeta (Spanish) port. Goal: Frida
capture hooks for `vt_spa_violeta16.dll`'s front end (text normalizer in/out
+ linguistic records), mirroring `hooks_fetext.js`/`hooks_feling.js`, so a
later Rust port can be checked against captures. Ghidra MCP used read-only
throughout (no renames/comments/saves -- another agent had both programs
open concurrently for the back-half batch).

### 2026-09-27

#### Setup

- Workspace: `C:\tmp\vtwork\v1_front` (this report, smoke-test scripts,
  scratch corpora).
- Read `VIOLETA_TODO.md`, `tools\violeta_funcmap.tsv`, the English
  `hooks_fetext.js` / `capture_fetext.py` / `vtfe_multi.c` /
  `build_fetext.bat`, `hooks_feling.js` / `capture_feling.py` /
  `tools\fedump\fedump.c`, `vtprobe.c` (already SPA-aware), and
  `capture_violeta.py`.
- Confirmed via Ghidra MCP (`list_open_programs`) both `vt_eng.dll` and
  `vt_spa_violeta16.dll` load at the same image base (`0x10000000`), so
  English RVAs and Violeta RVAs are directly comparable as absolute
  addresses without any base-relocation arithmetic.

#### Address map: how each Violeta hook point was found

`tools\violeta_funcmap.tsv` (batch 0's offline pefile/capstone matcher) only
covers functions reachable as **exports or direct call targets** in each
binary's own scan; several of the functions `hooks_fetext.js`/
`hooks_feling.js` hook are reached only through data/callback tables and
weren't in that scan on the English side either, so they have no funcmap
row at all. Where the funcmap had a row, I decompiled both sides and diffed
structurally to confirm it (not just trusted the score). Where it didn't, I
walked the call graph by hand: decompile a confirmed anchor, read off the
named/addressed callees Ghidra shows in the pseudocode, and match those
against the same call site in English. This is the same technique
`violeta_funcmap.py`'s "callgraph" tier automates, just done manually for
the handful of addresses it missed.

| Concept | Violeta addr | English addr | How confirmed |
|---|---|---|---|
| `fe_InitialTextProcess` | `0x10007730` | `0x1001c990` | funcmap (callgraph 0.682) + full decompile diff: identical structure, only TIB offsets differ (map_a/b/c, see below). |
| `fe_NextSentence` | `0x1000dce0` | `0x10022dc0` | Not in funcmap. Found as the function calling the confirmed `fe_ResolveSentencePSV` address; decompile matches English's `fe_NextSentence` line-for-line except the offset-map/PSV-struct offsets. |
| `fe_ResolveSentencePSV` | `0x1000d730` | `0x10022850` | funcmap (fuzzy 0.791) + full decompile diff: byte-identical logic. Confirmed the pitch/speed/volume TIB offsets (`+0x4cf0/+0x4cec/+0x4cf4`) match `VIOLETA_TODO.md`'s existing batch-0 note exactly. |
| `fe_AnalyzeSentence` (called from `fe_NextSentence`) | `0x10031010` | `0x1003d350` | Read off `fe_NextSentence`'s decompile (calls it in place of the named English callee); its own decompile matches structurally (init WA header, call normalize, call linguistic-analysis-wrapper). |
| `fe_NormalizeTokens` | `0x10031060` | `0x1003d3d0` | Read off `fe_AnalyzeSentence`'s decompile. Signature matches (`short* WA, uint* text`); its huge try-chain calls two already-Ghidra-named functions, `phone` and `norm_time`, directly by name -- strong independent confirmation this is the token-dispatch function. |
| `fe_Tokenize` | `0x10047000` | `0x1005a350` | Found via the "word" normalizer handler (see next row): both call it with matching mode arguments (`8`, `0xc`) at the matching call sites; 4-arg signature (`tok, text, off, flag`) matches English's first 4 params. English's 5th arg (`sayas`) was not observed at any Violeta call site -- may not exist here. Medium-high confidence (structural + call-site match, not byte-identical decompile). |
| "word" normalizer handler | `0x10042180` | `0x1003a880` | Found inside `fe_NormalizeTokens`'s try-chain by matching the `fe_Tokenize(..., 8, ...)` / `dict_LookupLexicon` call pattern against English's `FUN_1003a880`. |
| `fe_EmitWord` (writes WA records) | `0x10035350` | `0x10044fd0` | Found inside the "word" handler's callees. **Byte-identical logic** to English's `fe_EmitWord`, confirming this is the record writer and giving the WA record layout (see below). |
| `fe_LinguisticAnalysis` (outer wrapper) | `0x10031700` | `0x1003e240` | Read off `fe_AnalyzeSentence`'s decompile (2nd call). Violeta's wrapper is a 2-line shell that just calls `FUN_100256d0` and maps its sign to 1/-1; English's equivalent inlines the tag+merge logic directly. Structural role confirmed (called at the same point, same 1-arg WA-pointer signature), internal decomposition genuinely differs (see below). |
| `FUN_100256d0` (calls tag-pass then merge-pass) | `0x100256d0` | n/a (inlined in English) | Decompiled; calls `FUN_10021bf0` then `FUN_100252c0`, matching English's own two phases (`fe_TagAndPronounce` call, then inline WA/morph merge loop) but as two separate Violeta functions instead of one. |
| `fe_BuildWordPhoneRecords` | `0x1001d300` | `0x10016c90` | Not in funcmap (English address itself isn't reached as an export/call-target in the offline scan). Found the shared orchestrator: `VT_TextToBuffer_ENG` -> `wav_TextToBufferS16` -> `FUN_10026ab0` -> `FUN_10026630`, which **is** in the funcmap (`eng 0x10026630` -> `spa 0x100115f0`, callgraph 0.735). Decompiling the Violeta side of that match gave the calls to `fe_BuildWordPhoneRecords`/`fe_AssignPhraseBreaks`/`fe_BuildPhoneContexts` directly, byte-identical control flow to English's `FUN_10026630` (down to `pro_ResolveSentencePause` and `pro_PredictPitch` already sharing English names in Ghidra's DB). High confidence. |
| `fe_AssignPhraseBreaks` | `0x1001e750` | `0x100130e0` | funcmap (callgraph 0.5) + full decompile diff: identical structure. Also independently reachable as the 2nd call in the orchestrator above. |
| `fe_BuildPhoneContexts` | `0x1001d630` | `0x10017510` | Same orchestrator call graph as above. Ghidra infers a **0-argument** signature for both this function and its only call site (`FUN_1001d630()` with no args, vs English's explicit `fe_BuildPhoneContexts((int)param_2)`); the hook therefore does not read `args[0]` for the TIB and instead reuses the TIB last seen entering `fe_NextSentence` (see "deferred" notes below). |
| `VT_TextToPreprocessInfoFile_SPA` | `0x10008b30` | `0x1001dd60` | Export listing (`list_exports`). Thin wrapper (10-arg passthrough to `FUN_10009cf0`), byte-identical shell to English's wrapper around `FUN_1001ef20`. |
| `VT_TextToBuffer_SPA` | `0x100089f0` | `0x1001dc40` | Export listing; used by `vtfe_multi_spa.c` (same 13-arg call shape as `vtprobe.c` already assumes and as `vtfe_multi.c` uses for English). |
| `VT_LOADTTS_SPA` / `VT_UNLOADTTS_SPA` / `VT_SetPitchSpeedVolumePause_SPA` / `VT_SetCommaPause_SPA` | `0x10012e10` / `0x10012f20` / `0x10012f70` / `0x10013140` | -- | Export listing; already proven working by `vtprobe.c`/`capture_violeta.py`. |

#### Struct offsets that differ from English

All of these were derived by decompiling the Violeta function and comparing
field-by-field against the confirmed English counterpart, not guessed.

| Field | English | Violeta | Notes |
|---|---|---|---|
| `fe_InitialTextProcess`'s orig<->processed offset maps (map_a/map_b/map_c), off TIB | `+0x47790` / `+0x47794` / `+0x47798` | `+0x82e60` / `+0x82e64` / `+0x82e68` | Same 3 contiguous int-array-pointer fields, just placed later in Violeta's (larger) TIB. |
| WA record: stride / header size / text field offset | `0x94` / `0x14` / `+0x34` | `0xa8` / `0x0c` / `+0x24` | From `fe_EmitWord`'s byte-identical logic at different literal offsets. **This was wrong on the first pass** -- I initially assumed the English constants held and got back all-zero/garbage `start`/`end`/`len`/`text` fields; decompiling Violeta's own `fe_EmitWord` (`0x10035350`) and finding the real stride/base/text-offset fixed it. Re-ran the smoke test afterward: WA record text now reads `Hoy`, `es`, `Domingo`, `veintisiete`, `de`, `septiembre`, ... which matches `fedump_spa.exe`'s own mode-7 phone dump word list exactly (see Sanity below). start/end/len fields (offsets `+0`/`+4`/`+8` within the record) are unchanged from English. |
| PSV override/effective/base struct, off TIB (read by `fe_ResolveSentencePSV`) | override flags `+0x1210d7..d9`, override values `+0x1210dc/e0/e4`, effective `+0x1210e8/ec/f0`, base `+0x1210f4/f8/fc`, pause-override marker `+0x1312c4` | override flags `+0x15c7a7..a9`, override values `+0x15c7ac/b0/b4`, effective `+0x15c7b8/bc/c0`, base `+0x15c7c4/c8/cc`, pause-override marker `+0x16c994` | Same relative layout (effective->base offsets are `+0xc/+0x10/+0x14` in both), base address shifted. Pitch/speed/volume default-value TIB offsets (`+0x4cf0/+0x4cec/+0x4cf4`) match `VIOLETA_TODO.md`'s existing note. |
| SS per-word record: base offset / stride, off SS pointer (itself at TIB`+0x4c`, unchanged) | `WORD_BASE=0x64c`, `WORD_SIZE=0x3c0` | `WORD_BASE=0xa4c`, `WORD_SIZE=0x540` | Derived from `fe_BuildWordPhoneRecords` (`0x1001d300`)'s per-word int-pointer-array loop (stride `0x150` ints `= 0x540` bytes, base `0xa4c`), independently cross-checked against `fe_AssignPhraseBreaks` (`0x1001e750`), which walks the same SS array in `0x2a0`-*short*-unit (`= 0x540`-byte) steps -- two unrelated functions agreeing on the stride. Record contents look sane at these offsets (see Sanity below): first 8 bytes of word 0/1's records are plausible in-buffer pointers, consistent with `fe_BuildWordPhoneRecords`'s own writes (`*piVar11 = iVar12 + -0x1e; piVar11[1] = iVar12;`). |
| Phrase-break level byte codes | `Z`/`[`/`\`/`]` (`0x5a`-`0x5d`) | `F`/`G`/`H`/`I` (`0x46`-`0x49`) | From `fe_AssignPhraseBreaks`'s `switch` cases; otherwise identical control flow. |
| Orchestrator per-sentence TIB zero fields | `+0x47704`/`+0x47706` | `+0x82dcc`/`+0x82dce` | From `FUN_100115f0` (Violeta's `FUN_10026630` counterpart). |

#### What did NOT get ported, and why (rather than guessed)

- **VTML tag stacks / marks / PSV-override raw dump** (English's
  `readTagStacks`, `readMarks`, `tib+0x1210d4` override bytes in the
  `initial` event). The callback table `fe_InitialTextProcess` passes to
  `fe_RunTextFilterPass` for VTML extraction (English's
  `fe_ExtractVtmlTags`/`fe_MapFullwidthChars`) resolves in Violeta to two
  bare code labels (`LAB_100080f0`, `LAB_10008350`, right after
  `fe_RunTextFilterPass`'s own body) that Ghidra has **not** turned into
  Function objects, so `get_function_by_address`/`decompile_function`
  can't read them without creating a function first -- which would mutate
  the shared, concurrently-open Ghidra DB, against the read-only
  instruction for this batch. Left out of `hooks_fetext_spa.js` rather than
  guessing TIB offsets for a VTML struct I couldn't inspect. NWS text
  essentially never carries VTML tags, so this shouldn't affect the
  captured corpus; it's the main open item for the linguistic-front-end
  batch (batch 3) or a follow-up with exclusive Ghidra access.
- **`tp_out`/`lookup_out`/`copy_out`/`morph_out`/`wh_out`/`fw_out`/
  `fw_in_wa`/`fw_out_wa`** (English's `fe_TagAndPronounce` sub-stage dumps).
  English decomposes lexicon lookup into 5 separate functions
  (`FUN_1000d190`, `dict_CopyMorphsToWordArray`, `FUN_10007520`,
  `FUN_1000e990`, `FUN_1000dfc0`), each touching a distinct morph/candidate
  array (`MORPH_OFF`/`MORPH_COUNT`/`CAND_SIZE` in `hooks_feling.js`).
  Violeta's equivalent (`FUN_10021bf0`, called from `FUN_100256d0`, called
  from the `fe_LinguisticAnalysis` wrapper `FUN_10031700`) does lexicon
  lookup, Spanish plural/gender suffix stripping (`-s`/-consonant checks)
  and LTS fallback **in one monolithic ~2KB function** with no separate
  morph array found in its decompile. This is a genuine architectural
  difference, not a mapping I failed to find -- the concept these six
  record kinds capture doesn't exist at that granularity in Violeta.
  `hooks_feling_spa.js` captures `wa_in`/`wa_out` around the outer
  `fe_LinguisticAnalysis` wrapper instead, which is where the WA array
  concept still applies. Reversing `FUN_10021bf0`/`FUN_100252c0`'s internal
  layout in detail is linguistic-front-end (batch 3) work.
- **`fe_LetterToSound` / spell / `lts_hy` per-word hooks**. Not located;
  given the above (no separate morph/LTS-dispatch layer was found at the
  `fe_TagAndPronounce` granularity), the equivalent call sites are probably
  inside `FUN_10021bf0` itself rather than sibling functions, and weren't
  chased further in this batch's time budget.
- **Syllable sub-array in the SS word record** (English's `nsyl`/`sylp`/
  `SYL_SIZE` at word`+0x94`/`+0x08`/`0x1e`). `fe_BuildPhoneContexts`'s
  Violeta decompile (`FUN_1001d630`) is a single function full of Ghidra
  `in_stack_*`/`extraout_*` synthetic variables (a sign the decompiler lost
  track of the real stack frame, likely from an unusual calling-convention
  or heavy inlining) and was not reliable enough to pull a byte offset from
  with confidence. `dumpSS`'s per-word records are therefore raw
  `WORD_SIZE`-byte dumps with `syl: []` always empty, rather than a guessed
  sub-array.
- **`COV_FUNCS`/`DEEP_FUNCS`/`--cov`/`--call`/`--deep`** (English's
  per-normalizer coverage counters and the numword/address/fraction/phone/
  math deep-dive traces). These were built for specific historical English
  bugs (the "220 PM", "son-address", fraction/phone and math-vs-"N/N"
  investigations noted in `hooks_fetext.js`'s comments) and have no
  Spanish equivalent investigation driving them; reversing all ~20 of
  Violeta's number/date/time/currency/etc. normalizer dispatch-chain
  functions to build an equivalent list is Spanish-normalizer work (batch
  4, "Text front end"), not capture-harness work. Dropped from
  `hooks_fetext_spa.js` and `capture_fetext_spa.py` (no `--cov`/`--call`/
  `--deep` flags); `--tok` was kept since `fe_Tokenize`'s address was found
  anyway.
- **TIB per-word prosody arrays** (English's `pitch`/`speed`/`volume`/
  `pause`/`brk_level`/`kind7` int[200] arrays at `tib+0x121100 + a*0x320`
  in `dumpSS`). Not chased -- these feed prosody assignment, which is
  batch 2's territory, and `dumpSS`'s raw per-word record bytes already
  cover the front-end-relevant "what does the linguistic stage know about
  this word" question for batch 1's purposes.

#### One correctness bug found and fixed along the way

`fedump_spa.c`'s first draft copied `fedump.c`'s exact call
(`info(argv[i], out, (char)mode, 1, -1, -1, -1, -1, 0, 0)`), including the
literal `1` in the 4th argument. Decompiling `VT_TextToPreprocessInfoFile_SPA`'s
target (`FUN_10009cf0`) showed that argument indexes a small
per-speaker-slot array (`(&DAT_1007ea1c)[iVar7]`, clamped to `[0,1]` but
**not** clamped further to slot 0 for a single-speaker build); since
Violeta only has speaker slot 0 populated, passing `1` looked up a null
slot and every call returned `0xfffb` (-5) with no dump file written.
Changed the literal to `0` (matching `VIOLETA_TODO.md`'s "speaker slot 0
only" note and `vtprobe`'s `VT_SPEAKER=0`); confirmed working immediately
after (`fedump_spa.exe 7 ... "Hoy es domingo..."` now returns `1` and
writes a full mode-7 phone dump).

#### Data files opened (from decompile observation during this batch)

Not the focus of this batch, but noted in passing while confirming
`fe_InitialTextProcess`/`FUN_10009cf0`: no VTML/tag-related file opens were
observed for the plain-text NWS corpus (consistent with the VTML machinery
never firing on this input). No new dictionary/tree file names beyond
those already logged in `VIOLETA_TODO.md`'s setup section were seen.
Confirming which of `spabi.tree2`/`.tree3` is actually opened at runtime
was not re-derived here (already answered in the setup log: the DLL string
says `spabi.tree2` but the shipped file is `spabi.tree3` -- worth a
follow-up file-monitor pass if it matters for batch 3).

### Deliverables

#### New files

- `harness\hooks_fetext_spa.js` -- `initial`/`next`/`wa`/`tok` events,
  same shape as `hooks_fetext.js` minus `cov`/`call`/`deep` (see above).
- `harness\capture_fetext_spa.py` -- same CLI shape as `capture_fetext.py`;
  accepts both plain one-line-per-text corpora (like `probe_front.txt`) and
  `name\ttext` TSVs (like `corpus_spa\nws_es.tsv`) so the existing corpus
  didn't need reformatting. Adds a `name` field to each JSON record (`null`
  for plain corpora).
- `harness\vtfe_multi_spa.c` + `harness\build_fetext_spa.bat` -- Violeta
  build of the `vtfe_multi` driver (speaker 0, `_SPA` exports, `VT_DLL`/
  `VT_ROOT` env overrides default to `all_voices`). Built successfully with
  MSVC x86; `vtfe_multi_spa.exe` present in `harness\`.
- `harness\hooks_feling_spa.js` -- `wa_in`/`wa_out`/`words_out`/
  `breaks_out`/`ctx_out`/`offset_maps`/`begin`/`end` events. Field names
  kept identical to `hooks_feling.js` wherever the concept survives; the
  six English-specific tag/morph-decomposition event kinds are simply not
  emitted (see above) rather than emitted with wrong data.
- `harness\capture_feling_spa.py` -- same CLI shape, job-object-cleanup
  pattern and per-text output naming (`out_feling_spa\<name>.json`) as
  `capture_feling.py`. Drives `tools\fedump\fedump_spa.exe`.
- `tools\fedump\fedump_spa.c` + `tools\fedump\build_spa.bat` -- Violeta
  build of `fedump` (speaker 0 fix noted above; `_SPA` export). Placed
  alongside `fedump.c`/`fedump.exe` (existing file, unmodified) rather than
  in `harness\`, mirroring where the English driver already lives; flag
  this placement if a different convention was intended.
- `harness\corpus_spa\probe_front.txt` (39 lines) + `probe_front.tsv`
  (same 39 rows, `name\ttext`, built by a throwaway helper in this
  workspace) -- synthetic probe corpus covering numbers, dates, times,
  ordinals, ni~o/accented vowels, inverted-punctuation, abbreviations,
  English loanwords and NWS zone/UGC headers (`FLZ168-...`,
  `TXZ214-215-...`, `GMZ435-450-...`, `WWUS74 KBRO ...`).

#### Nothing existing was overwritten

`harness\hooks_fetext.js`, `capture_fetext.py`, `vtfe_multi.c`/`.exe`,
`build_fetext.bat`, `hooks_feling.js`, `capture_feling.py`,
`tools\fedump\fedump.c`/`.exe`/`build.bat` are all untouched (verified by
re-reading them at the start of this session and only ever using `Write`
on new `_spa`-suffixed paths).

#### Captures produced

| Output | Count | Notes |
|---|---|---|
| `harness\out_fetext_spa\nws_es.jsonl` | 1327/1327 lines | 0 Frida errors. ~9s wall time at `--jobs 24`. |
| `harness\out_fetext_spa\probe_front.jsonl` | 39/39 lines | 0 Frida errors. |
| `harness\out_feling_spa\*.json` | 1327 (nws_es) + 39 (probe_front) = 1366 files | 0 Frida errors on the successful run. First attempt at `--jobs 24` hit `frida.TransportError: timeout` around text 1008/1327 (spawn/attach contention from too many concurrent job-object-wrapped `fedump_spa.exe` processes each doing real synthesis-adjacent work, unlike the audio-discarding `vtfe_multi_spa`); re-ran clean at `--jobs 12` (34.7s, matching `capture_feling.py`'s own `MAX_JOBS=12` cap) with 0 errors. |

`out_fetext_spa` is ~49MB, `out_feling_spa` is ~944MB (raw per-word
`WORD_SIZE`-byte SS dumps at 3 pipeline stages per sentence adds up on the
longer NWS forecast paragraphs; same tradeoff English's `hooks_feling.js`
makes).

### Sanity checks (deliverable 5)

**WA-to-linguistic-record chaining**, probe text "Hoy es domingo 27 de
septiembre de 2026.":

```
wa_in  n=10        (fe_LinguisticAnalysis entry)
wa_out n=10 ret=1   (fe_LinguisticAnalysis exit)
words_out n_words=10 ret=1   (fe_BuildWordPhoneRecords)
breaks_out n_words=10 ret=1  (fe_AssignPhraseBreaks)
ctx_out   n_words=10 ret=1   (fe_BuildPhoneContexts)
```

...and the same text's mode-7 engine dump lists exactly 10 words (`Hoy`,
`es`, `Domingo`, `veintisiete`, `de`, `septiembre`, `de`, `dos`, `mil`,
`veintiseis` -- the last three are the engine's own multi-reading
expansion of "2026"). The WA record count from `fe_NormalizeTokens`
(captured by `hooks_fetext_spa.js`) is *also* 10 for this line, and its
per-record `text` field spells out the identical word list
(`Hoy`/`es`/`Domingo`/`veintisiete`/`de`/`septiembre`/`de`/`dos`/`mil`/
`veintiseis`) -- confirming the text-normalizer-out records
(`hooks_fetext_spa.js`'s `wa` event) chain correctly into the
linguistic-analysis records (`hooks_feling_spa.js`'s `wa_in`/`words_out`/
`breaks_out`/`ctx_out`) for the same line, and that the WA record layout
fix (see "Struct offsets" above) is correct end to end.

**WAV unchanged under hooks** (`C:\tmp\vtwork\v1_front\sanity_wav_check.py`):
ran `vtprobe.exe` under Frida with `hooks_feling_spa.js` attached for 5
`nws_es` lines and SHA-256-compared the resulting WAV against the existing
unhooked `harness\out_violeta\*.wav`:

```
nws_BRO_CWF_0_001: MATCH  d91eee156c8a...
nws_BRO_CWF_0_002: MATCH  9a4346f60adb...
nws_BRO_CWF_0_003: MATCH  16fd8599ff33...
nws_BRO_CWF_0_004: MATCH  26cdd1c70634...
nws_BRO_CWF_0_005: MATCH  1400f65bab4d...
```

5/5 byte-identical. `hooks_fetext_spa.js` could not be attached the same
way -- it requires the `vtfe_marker` export that only `vtfe_multi_spa.exe`
provides (by design, matching English's `hooks_fetext.js`/`vtfe_multi.exe`
pairing), and `vtfe_multi_spa` itself never writes a WAV (audio discarded,
same as English's tool). Both hook scripts use the identical
`Interceptor.attach`-with-`onEnter`/`onLeave`-reads-only style (no argument
or return-value writes anywhere in either file), so `hooks_feling_spa.js`'s
5/5 exact-match result is taken as evidence for `hooks_fetext_spa.js`'s
non-invasiveness too, rather than re-proving it a different way.

### Counts / odd findings summary

- 8 Violeta addresses hooked directly across the two scripts
  (`fe_InitialTextProcess`, `fe_NextSentence`, `fe_NormalizeTokens`,
  `fe_Tokenize`, `fe_LinguisticAnalysis`, `fe_BuildWordPhoneRecords`,
  `fe_AssignPhraseBreaks`, `fe_BuildPhoneContexts`), plus the
  `VT_TextToPreprocessInfoFile_SPA` export -- all listed with English
  counterparts in the table above.
- 1327/1327 NWS Spanish corpus lines captured on both scripts, 0 Frida
  errors, plus a 39-line synthetic probe corpus, 0 Frida errors.
- One real bug found and fixed: `fedump_spa`'s hardcoded speaker index `1`
  (copied from `fedump.c`) needed to be `0` for Violeta's single-speaker
  build -- otherwise every `VT_TextToPreprocessInfoFile_SPA` call failed
  with `-5`.
- One real bug found and fixed in my own hook: `hooks_fetext_spa.js`'s
  first draft assumed the English WA record layout (`0x94`/`0x14`/`+0x34`)
  unchanged; Violeta actually uses `0xa8`/`0x0c`/`+0x24`, found by
  decompiling Violeta's own `fe_EmitWord`. First-pass symptom was
  all-zero/garbage `start`/`end`/`len`/`text` fields; fixed and confirmed
  against the mode-7 dump.
- WA-record byte layout, SS per-word record base/stride, and phrase-break
  level byte codes all differ from English (table above); low-level TIB
  header fields (cursor, done flag, "more" flag, SS pointer at `+0x4c`)
  are unchanged.
- Genuine architectural difference: Violeta's lexicon/morphology tagging
  pass is one monolithic function (`FUN_10021bf0`) where English uses 5
  separate sub-stage functions with distinct morph/candidate arrays; no
  Violeta analogue exists at that granularity, so 6 of English's record
  kinds have no Spanish counterpart and are not emitted (not guessed).
- Deferred/open items for follow-up (batch 2/3, or a session with
  exclusive Ghidra access): VTML tag-stack/mark offsets (blocked on two
  un-analyzed code labels), SS syllable sub-array offset (decompile too
  unreliable), `fe_LetterToSound`/spell/`lts_hy` addresses (not located),
  per-word TIB prosody arrays (out of front-end scope), and independently
  confirming whether `spabi.tree2` or `.tree3` is opened at runtime.

## Appendix B: batch 1 back-half capture log


Saved by the lead from agent B's hand-back (the agent could not write .md files).

### Deliverables (harness\, new files only)
- `hooks_spa.js`: prosody + selection + wave-record hooks (mirrors `hooks.js`).
- `hooks_wave_spa.js`: `hooks_spa.js` + `wav_BuildUnitRecords` input snapshot + `wav_NextChunkWithEmphasis` chunk hook.
- `hooks_prosody_spa.js`: VTML span application, sentence pause, pause gaps, pre-selection word-target lists.
- `capture_initial_spa.py`, `capture_unitsel_spa.py` (thin wrapper), `capture_prosody_spa.py`, `capture_wave_spa.py`:
  drive `vtprobe.exe` with `VT_DLL`/`VT_ROOT`/`VT_LANG=SPA`/`VT_SPEAKER=0`; job-object kill-on-close as in
  `capture_speak.py`.
- Outputs, full `nws_es.tsv` (1327) in all four: `out_initial_spa` 1366, `out_unitsel_spa` 1366 (+39
  `probe_front.tsv`, picked up by the `corpus_spa\*.tsv` glob), `out_prosody_spa` 1378 (+10 VTML edge cases, +2
  param sweeps), `out_wave_spa` 1416 (+40 speed/pitch/vol/emphasis sweeps, +10 combo/pause). 2-6 min per full pass
  at 24 jobs. About 12.5 GB total.
- Workspace tools: `xref.py` (direct-call caller/callee finder over `tools\violeta_funcmap.py`'s `Bin`),
  `verify.py` (sanity checks).

### Sanity checks
- All 1327 WAVs in `out_initial_spa`/`out_unitsel_spa`/`out_wave_spa` with a matching `out_violeta\*.wav` are
  SHA1-identical to it.
- `nws_BRO_CWF_0_001/_002`, `nws_TBW_ZFP_0_010` across the four sets: n_targets equal (initial vs unitsel);
  n_records/pcm_samples/record_samples equal (initial vs wave); wave `chunks[]` sum to `pcm_samples*2`; prosody
  wordtargets count = n_targets.
- `ch_pause` (hooks_prosody_spa, spk+0x4cfc) and `ch_sent_pause` (hooks_wave_spa, same offset, different function)
  agree (687).

### Hooked addresses
| Violeta | English | Name | Confirmed by |
|---|---|---|---|
|0x1001f040|0x100138c0|pro_PredictPitch|funcmap callgraph 0.516|
|0x1001ed90|0x100137c0|pro_SmoothF0AcrossSyllables|last call of PredictPitch, same `(i,tib)` shape|
|0x1000f9b0|0x1002c9b0|us_SelectUnitsForPhrase|same call site in wav_SelectAndGeneratePhrase (0.833)|
|0x1000e290|0x10023350|us_GetCandidates|funcmap fuzzy 0.589|
|0x1001f1a0|0x100182e0|us_TargetCost|funcmap callgraph 0.489|
|0x1000f520|0x10024510|us_BeamPrune|same position in us_ViterbiForward (0x1000f8e0)|
|0x100173d0|0x1002c220|wav_BuildUnitRecords|funcmap callgraph 0.452|
|0x1000d840|0x10022970|pro_ApplyTagSpansToWords|called from fe_NextSentence (0x1000dce0); same case 0/1/2/3/4/7 switch|
|0x10011710|0x10026750|pro_ResolveSentencePause|funcmap callgraph 0.78|
|0x10019c10|0x1002e890|pro_PauseSamples|same spot in wav_BuildUnitRecords|
|0x10011830|0x10026870|wav_NextChunkWithEmphasis|funcmap callgraph 0.748|

Each candidate was decompiled in Ghidra and its pointer arithmetic matched against the English original.

### Struct offsets (Violeta vs English)
Confirmed unless marked inferred.
- WORD_BASE 0x64c (same); WORD_SIZE 0x3c0 -> 0x540; SYL_SIZE 0x1e -> 0x28.
- word: syl_ptr +0x08, target_list_ptr +0x0c (same); cand_count +0x92 -> +0xd8 (inferred); phones[].dur +0x10
  stride 2 (inferred); n_syl +0x94 -> +0xda; n_phones +0x95 -> +0xdb; ctx[] (7 B/phone) +0x96 -> +0xdc (3
  functions); brk +0x3bd -> +0x53c. text/p2e4/pos/pos_b/domain and phones[].code/mark not located.
- syllable: tone +3, f0[12] +4 stride 2 (same); b0/b1/b2 0/1/2 (inferred); English u8 first_phone at +0x1c is a
  pointer in Violeta (+0x1c and +0x20 are pointers, not decoded); n_phones +0x1d -> +0x24.
- target struct (6 B) identical; array 0xec628 -> 0x127cf8, n 0xec624 -> 0x127cf4.
- candidate struct (0x14 B) identical; pointer array 0x477ac -> 0x82e7c.
- beam block (n@+0xf4, stride 0xfc) 0xae894 -> 0xe9f64; acc (stride 0x78) 0x8212c -> 0xbd7fc; back (stride 0x3c)
  0x9f664 -> 0xdad34; path (stride 2) 0xae0c4 -> 0xe9794.
- primary record (0x18 B) identical; array 0x11b24c -> 0x15691c. Record (0x34 B), feature and phrase structs
  identical.
- ss: n_phrases/n_words/pause 0/2/8 (same); phrase_index 0x47704 -> 0x82dcc; word_pauses/speed/vol/pitch cluster
  0x4770c -> 0x82ddc; type 0x4770a -> 0x82dda (inferred).
- tib: chan 0x47784 -> 0x82e54; final_pause 0x122440 -> 0x15db10; cont/dtt flags 0x122850/1 -> 0x15df20/1;
  call_pause/tib_pause 0x1312c8/c4 -> 0x16c998/94; wa_ptr 0x1312bc -> 0x16c98c; per-word pitch/speed/volume/pause/
  brk/k7 (stride 0x320) 0x121100/1420/1740/1a60/1d80/220a0 -> 0x15c7d0/caf0/ce10/d130/d450/d730; base
  pitch/speed/volume (inferred); span lists (8 x 16 B) 0x1223c0 -> 0x15da90.
- channel, uniform -4: speed 0x4cf0 -> 0x4cec, pitch 0x4cf4 -> 0x4cf0, volume 0x4cf8 -> 0x4cf4, sent_pause
  0x4d00 -> 0x4cfc, comma 0x4d04 -> 0x4d00.

### Cepdist remap
Not used at any hooked site (TargetCost/GetCandidates/ViterbiJoinStep use phone-context tables). `FUN_10005c10`
runs at load; no second consumer of the 1024-codeword space found in the selection path. Recheck when porting
the join cost.

### Phone table
No standalone phone-name table found. Phone-class lookup at 0x10067114, populated for codes 0..0x2c (44 entries).
Per-phone features are captured as `phones[].ctx`.

### Schema vs English
- Same names/meaning: n_syl, n_phones, pitch/speed/volume_pct, brk, phones[].ctx, target/candidate/beam/primary/
  record structs, channel, span lists, pauses.
- Dropped from word: text, p2e4, pos, pos_b, domain (offsets unknown; bytes kept in `word.raw`, pointers masked).
- Dropped from phones[]: code, mark. `dur` kept, offset inferred.
- `syllable.first_phone` not emitted (pointer in Violeta); `syllable.raw` masks +0x1c/+0x20.

### Odd things
- Break bytes shifted -0x14: English Z/[/^ (0x5a/0x5b/0x5e), Violeta F/G/J (0x46/0x47/0x4a). Sentence-final
  word has brk 0x46; `wav_BuildUnitRecords` hardcodes 0x46 where English has 0x5a for pause records' center.
- us_ViterbiJoinStep (FUN_1001f9d0) and us_TargetCost share a "vdb" context struct (+0x24, +0x40, +0x6c, +0x78,
  +0x80 common).
- wav_FillUnitRecord and us_Backtrack are "exact" funcmap matches (normalized), not byte-identical: embedded
  offsets differ.

### Left for later
- word text/p2e4/pos/pos_b/domain, phones code/mark (recoverable from raw bytes).
- syllable pointers +0x1c/+0x20.
- ss.type and tib base pitch/speed/volume (inferred only).
- Spanish phone-name table (batch 2).
