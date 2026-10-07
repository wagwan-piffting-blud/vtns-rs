# Violeta (Spanish engine)

Reference for the Spanish VoiceText engine and its Rust port (`Lang::Spa`): `vt_spa_violeta16.dll` (32-bit,
June 2013, `Era::V2014`) and the 2009 build of the same DLL (`Era::Violeta09`). The pipeline has the same
stages as English; this document lists, per stage, how Spanish differs and points to the English stage docs for
everything shared: [FRONTEND_TEXT.md](FRONTEND_TEXT.md), [FRONTEND_LING.md](FRONTEND_LING.md),
[LEXICON.md](LEXICON.md), [PROSODY.md](PROSODY.md), [UNITSEL.md](UNITSEL.md), [WAVEFORM.md](WAVEFORM.md);
2009 formats in [OLD_PAUL.md](OLD_PAUL.md).

**State: bit-exact.** Raw text to WAV is identical to the engine on every captured corpus: Violeta 2013
`speak_spa_parity` 1327/1327 WAVs (NWS Spanish products), Violeta09 `speak_old_parity` 1366/1366. Code:
`src/engine_spa.rs` (`EngineSpa::open`, `speak`), `src/text_spa/`, `src/ling_spa/`, `src/prosody_spa.rs`,
`src/lang.rs`; CLI `vtpaul --voice violeta`.

## 1. Engine facts

- **API.** 64 exports, the English API with `_SPA` suffixes (no `VT_CheckUserDict_SourceNorm`/`TargetPhon`).
  Speaker slot 0 only: `VT_SetPitchSpeedVolumePause_SPA` clamps the slot to 0..1 and loading slot 1 fails with 8.
  `VT_TextToPreprocessInfoFile_SPA` with speaker index 1 returns -5 (null slot); use 0.
- **Channel fields** sit 4 bytes lower than English (uniform): speed `+0x4cec`, pitch `+0x4cf0`, volume `+0x4cf4`,
  sentence pause `+0x4cfc`, comma `+0x4d00`. Same clamps (pitch 50-200, speed 50-400, volume 0-500, pause
  <= 0xffff).
- **License.** `VT_CheckLicense_SPA` (0x10014b10) is a stub returning 0; the verification file is still read
  (`FUN_10013d40`) and a Spanish demo notice exists at 0x100643a0, but dates and phone numbers are expanded and
  no notice is spoken, so the build behaves licensed.
- **Code overlap.** Only 4.8% of Violeta's 32-byte `.text` windows appear verbatim in `vt_eng.dll`; the two
  builds share logic but differ in struct offsets, helpers and register allocation. Both load at `0x10000000`,
  so addresses are directly comparable. Function map (offline pefile + capstone): 898 functions in Violeta,
  1034 in English, 591 matched (181 exact on normalized instructions, 238 by aligned call-graph propagation,
  115 fuzzy mutual-best 4-gram Jaccard >= 0.55, 56 by export name, 1 manual); 110 carry an English name.
  Matching found 20 of the 41 documented unit-selection functions.
- **Phone set.** Violeta has its own phones, 63 names at 0x10067e58 (`ling_spa::lts::NAMES`); labels print with
  English names in the generic tools. Phone-class table at 0x10067114 (44 entries, codes 0..0x2c).

## 2. Structure differences (Violeta 2013 vs English 2014)

Hook addresses (Violeta / English):

| Function | Violeta | English |
|---|---|---|
| `fe_InitialTextProcess` | 0x10007730 | 0x1001c990 |
| `fe_NextSentence` | 0x1000dce0 | 0x10022dc0 |
| `fe_ResolveSentencePSV` | 0x1000d730 | 0x10022850 |
| `fe_AnalyzeSentence` | 0x10031010 | 0x1003d350 |
| `fe_NormalizeTokens` (driver `FUN_10031060`) | 0x10031060 | 0x1003d3d0 |
| `fe_Tokenize` | 0x10047000 | 0x1005a350 |
| word handler | 0x10042180 | 0x1003a880 |
| `fe_EmitWord` | 0x10035350 | 0x10044fd0 |
| `fe_LinguisticAnalysis` | 0x10031700 | 0x1003e240 |
| `fe_BuildWordPhoneRecords` | 0x1001d300 | 0x10016c90 |
| `fe_AssignPhraseBreaks` | 0x1001e750 | 0x100130e0 |
| `fe_BuildPhoneContexts` (0 args in Ghidra) | 0x1001d630 | 0x10017510 |
| `pro_PredictPitch` / `pro_SmoothF0AcrossSyllables` | 0x1001f040 / 0x1001ed90 | 0x100138c0 / 0x100137c0 |
| `us_SelectUnitsForPhrase` | 0x1000f9b0 | 0x1002c9b0 |
| `us_GetCandidates` / `us_TargetCost` / `us_BeamPrune` | 0x1000e290 / 0x1001f1a0 / 0x1000f520 | 0x10023350 / 0x100182e0 / 0x10024510 |
| `us_ViterbiJoinStep` | 0x1001f9d0 | |
| `wav_BuildUnitRecords` | 0x100173d0 | 0x1002c220 |
| `pro_ApplyTagSpansToWords` | 0x1000d840 | 0x10022970 |
| `pro_ResolveSentencePause` / `pro_PauseSamples` | 0x10011710 / 0x10019c10 | 0x10026750 / 0x1002e890 |
| `wav_NextChunkWithEmphasis` | 0x10011830 | 0x10026870 |

Structure offsets (TIB, WA, SS):

| Field | English | Violeta |
|---|---|---|
| orig/processed offset maps (map_a/b/c), TIB | +0x47790/94/98 | +0x82e60/64/68 |
| WA record stride / header / text | 0x94 / 0x14 / +0x34 | 0xa8 / 0x0c / +0x24 (start/end/len at +0/+4/+8 unchanged) |
| PSV override flags / values / effective / base / pause marker | +0x1210d7..d9 / +0x1210dc/e0/e4 / +0x1210e8/ec/f0 / +0x1210f4/f8/fc / +0x1312c4 | +0x15c7a7..a9 / +0x15c7ac/b0/b4 / +0x15c7b8/bc/c0 / +0x15c7c4/c8/cc / +0x16c994 |
| SS word base / stride | 0x64c / 0x3c0 | 0x64c / 0x540 |
| syllable stride | 0x1e | 0x28 |
| word n_syl / n_phones / ctx[] (7 B per phone) / brk | +0x94 / +0x95 / +0x96 / +0x3bd | +0xda / +0xdb / +0xdc / +0x53c |
| syllable n_phones; syllable `+0x1c/+0x20` | +0x1d; u8 first_phone | +0x24; pointers into per-word phone/pos arrays at word+0x408/+0x4d0 |
| cand_count (inferred) | +0x92 | +0xd8 |
| SS n_phrases/n_words/pause | 0/2/8 | 0/2/8 |
| SS phrase_index / word-pause cluster / type (type inferred) | 0x47704 / 0x4770c / 0x4770a | 0x82dcc / 0x82ddc / 0x82dda |
| TIB chan / final_pause / cont+dtt flags | 0x47784 / 0x122440 / 0x122850,1 | 0x82e54 / 0x15db10 / 0x15df20,1 |
| call_pause / tib_pause / wa_ptr | 0x1312c8 / 0x1312c4 / 0x1312bc | 0x16c998 / 0x16c994 / 0x16c98c |
| per-word pitch/speed/volume/pause/brk/k7 (stride 0x320) | 0x121100/1420/1740/1a60/1d80/220a0 | 0x15c7d0/caf0/ce10/d130/d450/d730 |
| span lists (8 x 16 B) | 0x1223c0 | 0x15da90 |
| target array / n | 0xec628 / 0xec624 | 0x127cf8 / 0x127cf4 |
| candidate pointer array | 0x477ac | 0x82e7c |
| beam block (n@+0xf4, stride 0xfc) / acc (0x78) / back (0x3c) / path (2) | 0xae894 / 0x8212c / 0x9f664 / 0xae0c4 | 0xe9f64 / 0xbd7fc / 0xdad34 / 0xe9794 |
| primary record array | 0x11b24c | 0x15691c |
| orchestrator per-sentence zero fields | +0x47704/06 | +0x82dcc/ce |

Unchanged: target (6 B), candidate (0x14 B), primary (0x18 B), record (0x34 B), feature and phrase structs; the
TIB cursor, done flag, "more" flag and SS pointer (+0x4c). **Break bytes** shift -0x14: `F G H I J` (0x46..0x4a)
for English `Z [ \ ] ^`; the sentence-final word has brk 0x46, and `wav_BuildUnitRecords` hardcodes 0x46 where
English has 0x5a for pause-record centres. `us_ViterbiJoinStep` and `us_TargetCost` share a "vdb" context struct
(+0x24, +0x40, +0x6c, +0x78, +0x80).

## 3. Per-stage differences

### 3.1 Text front end (`src/text_spa/`)

Own implementation: `normalize.rs` is driver `FUN_10031060` (a 13-slot chain, then the word handler
`FUN_10042180`); `initial.rs` is `fe_InitialTextProcess`. Same tokenizer/WA concepts as
[FRONTEND_TEXT.md](FRONTEND_TEXT.md) with Spanish word forms. Ported normalizers:

| Normalizer | Function | Notes |
|---|---|---|
| math | `FUN_1002e6c0` | |
| numeric expressions | `FUN_10034270` | `-` after a unit word ("NM- 406") says "guion"; "5/hora" is "por hora" (unit consumed), a slash after a unit word is "por"; units looked up in the three groups then `FUN_10033990` |
| zone headers | `FUN_100426b0` | |
| symbols | `FUN_10042b20` | |
| date | `FUN_1002b410` | see below |
| fractions | `FUN_1002fd60` | all 4 shapes |
| cardinal/roman reader | `FUN_10033db0` | with ordinal suffixes |
| time | `FUN_10045e70` | |
| units | `FUN_10031720` | see below |
| gender fallbacks | `FUN_100286a0`, `FUN_10033c00` | |
| number glued to word ("FLC011") | `FUN_10028440` | leading zero reads digit by digit ("cero uno uno"), ordinal suffix as ordinal, else cardinal agreeing with the next word |
| abbreviations | `FUN_10027e60`, `FUN_10028200` | "C/" "con", "P/" "para", "T. N" "tomo" |
| URL | `FUN_100491d0`, tag mode `FUN_10049c60` | see below |

- **Word handler** consumes every non-word token silently. `(` is spoken "cerrar parentesis" and `)` "abrir
  parentesis" (engine data swap, kept). The timezone table maps "P.S.T." to "CST" (engine data bug, kept).
- **Units** (`FUN_10031720`): a leading amount from `FUN_10033050` (grouped number, list `FUN_10033500`, range
  `FUN_10033620`, or the previous word when an earlier normalizer left a number (src 0x11/0xe), "un"/"una" or a
  currency amount (src 5)); then up to five unit tokens tagged `E` (group A or the unit table), `P` (group B), `C`
  (group C), `/` and `O` ("por"), `W` ("cu"/"sq"), `s` (superscript), `.`. The tag string is matched from the
  longest prefix down against `PTR_DAT_10069370` ("$/A", "A", "AA", "A/A", "AOA", "AOA/A"); `FUN_10032660`
  validates, `FUN_100321c0` writes (src 0xa). Groups `DAT_10082060/64/68` are zero in the static image;
  `FUN_1002e160` builds them at load, so the `C`/`P` cases are live ("60 NM" -> "sesenta nanometros" via group B
  "nm"; "20 C" -> "grados celsius"). "70 por ciento" is a unit: "por ciento" is a two-word key in the unit table
  (`FUN_10044320` continues a key across tokens, mode 0xb). A range ("5 a 10 nudos") goes through
  `FUN_10033050` "NUMS" -> `FUN_100324d0`, and both the last number and the unit word get the plural flag.
  `FUN_10033990` strips one byte on its retry (words of 3+ bytes). `FUN_10032660` reads `param_2 + 0x3cc`, bytes
  12..15 of render buffer `local_100`, of which only the first byte is ever cleared: modelled with a zero start
  (the real start is stack garbage). `FUN_10032c50` confirms a one-letter group-C unit by context words among
  the five tokens before and after.
- **Dates** (`FUN_1002b410`): four-digit numbers are years at classify time (`FUN_1002d910`); ordinal-suffixed
  days are days (`FUN_1002d9b0`). `FUN_1002dc20` (day lists) and `FUN_1002dd40` (day and year ranges) collapse
  into one "DAYS" entry written token by token by `FUN_1002c6e0`; "272100- 238 AM CDT domingo" is a year range
  (272100 passes `FUN_1002db30`: 5+ digits, no leading zero). `FUN_1002d2b0` looks for `PTR_DAT_1006a750` words
  (weekdays, "fecha", "ano", "hoy", ...) among the two words before and three tokens after, unless the word
  before is "en"; it gates patterns 9, 0xe and 0x11 to 0x13. `FUN_1002bf10` never clears its day buffer between
  entries, so a second day appends.
- **URL**: both URL paths end in `FUN_100491d0`. A bare `/` is "barra" and sets `WA+8` (tag mode). The driver
  checks tag mode only after the URL normalizer: it marks the word count, and when the next tag-mode call adds
  words past the mark it ends the sentence there and re-arms tag mode. Every following path segment is its own
  sentence, spelled by `FUN_10049c60` through `FUN_10035880` (src 8). An IP address without a port fails the URL
  gate and is read as numbers.
- **VTML**: tag parsing including the regex gates, x-cmu phonemes and self-closing tags.

### 3.2 Linguistic front end (`src/ling_spa/`)

- **Lexicon + morphology is one function**, `FUN_10021bf0` (called from `FUN_100256d0`, which then calls the
  merge `FUN_100252c0`; the outer `fe_LinguisticAnalysis` wrapper `FUN_10031700` maps its sign to 1/-1). English
  has five sub-stages, so the `tp/lookup/copy/morph/wh/fw` record kinds do not exist for Spanish.
- `lexicon_pass` (`lexicon.rs`): the plural retry flag is per word (reset at 0x10021cbc); LTS runs on the
  casefolded, possibly `s`-stripped key. "-mente" adverbs run LTS on the raw stem and on "mente" and join them
  with a syllable break (code 99, `"%c%s"`); exception list of 67 words at 0x10068304. A stripped plural appends
  S (0x29, `"%c"`), twice on the -mente path. Any LTS or name-lookup failure truncates the WA word count and
  returns -1. Multi-word TPP join `FUN_100254b0`: joins with `-`, `A` origin, gap <= 1, "guion" `S` tokens
  stepped over, <= 7 words, <= 29 chars, longest match wins. Lexicon payload: flags, pron up to the first 0xff,
  then an optional number.
- **LTS** (`lts.rs`, translated from the engine, not rule-rewritten): `FUN_10022810` stage 1 (casefold, collapse
  mm/ss/tt/bb/dd/ff/gg/kk/pp, letter pattern T/D/S/W/w/B/1-3, syllable breaks, stressed syllable: accent, else
  penult/final rules; monosyllables stressed unless in the 10-word list at 0x1006866c); `FUN_10023930` stage 2
  (letters + pattern to codes); `FUN_100250e0` (degeminate, AA/OH/EH, UH0 -> AU/OU/EU, T+CH -> CH); for lexicon
  prons `FUN_10025700` + `FUN_10022430` + `FUN_10025820` (codes to names, C/V/B syllabification, names to codes).
- **Words/breaks/contexts** (`words.rs`, `syllab.rs`, `breaks.rs`, `context.rs`): `spabi.tree3` is the break
  tree (standard tree3, 37 nodes, 14 features; the DLL string says `spabi.tree2`, the shipped file and the port
  use tree3). Syllabifier `FUN_1001e1e0`; SYL_BASE 0x2134c; the word builder's kind7 is `*(ss+0x82dec)`
  (TIB+0x15d730 is prosody's span array). `fe_AssignPhraseBreaks` reads word 0 n_phones at ss+0x727 and brk at
  ss+0xb88. `wr::FWD_N` at word+0x61b lies past the 0x540 record (reads the next word): unchecked.

### 3.3 Prosody (`src/prosody_spa.rs`, `prosody_tables_spa.rs`)

`pro_PredictDurations` is `FUN_1001eb80` (features `FUN_1001e970`, 11 values, edge sentinels 50/51). One 8-way
class table (0x1006740c) picks among 8 duration trees (no `vshort`); loader `FUN_10001050`. Smooth F0, tag
spans, sentence pause and target feature are reused unchanged; pause samples need the Spanish break bytes.
Duration trees: `vdib`, `vdif`, `vmono` plus five consonant trees (English `vdi`, `vlong`, `vsch`, `vshort` +
consonants); pitch trees keep the English names.

### 3.4 Unit selection (`src/unitsel.rs`, `classes.rs`, `unitsel_tables_spa.rs`)

Differences from English: 2-level stress; no context/centre substitution in the lattice; acceptance units > 2
(English > 9); 11-byte half key (collapse class inserted); TargetCost without the TQ case and with an
unconditional pad bonus; join cost summed `cep*wc + db*wb + df*wf + 2`, no C384/vowel-adjacency terms, +20
partial penalty on the right half; pad/break bytes -0x14. **Cepdist remap** is applied in `us_ViterbiJoinStep`
(0x1001f9d0): vdb+0x18 remap table, vdb+0x1c flag; both units' `spec & 0x3fff` go through it before the n = 256
triangle (`CepDist::get`). `ttsdata\dist_tbl\cepdist.tbl.rd` (read by `FUN_10005a40` = English
`us_ReadCepDistTable`): u16 n = 256, f32 lower triangle, u16 m = 1024, m u16 remap entries (2 + 131584 + 2 + 2048
= 133636 bytes); `FUN_10005c10` builds the 256-entry scalar distance table (`ScalarDist::build`). Not used at
the other hooked selection sites.

### 3.5 Waveform

`wave.rs` is shared. Bank: `Bank` + Shorten decoder open `data-violeta\M16` unchanged: pools gen (65432) and
etc (2163), 67595 units, 108,729,602 samples (FNV-1a 6f84660e16fbb1c3). Pad/break bytes as above. The bank has
one speaker slot.

## 4. Data files

| File | Content |
|---|---|
| `data-violeta\M16` | unit bank (gen + etc pools) |
| `ttsdata\dist_tbl\cepdist.tbl.rd` | cepdist with remap (section 3.4) |
| `dict-spa`: `spattsdict` + `hashidx`/`hashcont`/`hashparams` | lexicon (format as [LEXICON.md](LEXICON.md)) |
| `dict-spa`: `tppdict_spa` + `_spa_tpp` hash files | TPP dictionary |
| `dict-spa\spabi.tree3` | break CART |
| `dict-spa\wab.txt2` | function words |

No abbreviation/street/city/sbd tables, no `exceptdict`, `atmt.tree3` or `poly.tree3`.

## 5. Builds

| Build | DLL | Header | Notes |
|---|---|---|---|
| Violeta 2013 | `vt_spa_violeta16.dll` (June 2013) | `ver.2013`, tree3 | everything above |
| Violeta09 | `Violeta\lib\vt_spa_violeta16.dll` (Dec 2009) | `ver.2005` (any `ver.` accepted), 39 B/unit, tree3 | speaker 0 (`VT_LANG=SPA`); `VT_CheckLicense_SPA` 0x100122c0 stubbed in the capture copy |

`Era::detect` selects the era from the index header and the tree file names. Violeta09 differences:
- **Layout:** word record 0x47c with a 5-byte decimal-coded context; unit records a struct of arrays; single
  speaker slot; 5 span lists; no domain-0xc branch or kind-7 pass; `dist_tbl/cepdist.tbl` without remap.
- **Text/ling** (`ling_spa/spa09.rs` for the 0x47c layout and post-lexicon stages, `text_spa/norm_phone09.rs`,
  era branches in tokenize/numexpr/date/phone/lexicon/lts/analysis): lexicon/LTS pass `FUN_1001f3d0` is much
  shorter than 2014's (8 of 188 LTS words differ). `FeSpa::load_era` must open the lexicon with `open_era`.
- **Prosody:** context byte 4 is decimal-coded; own phone tables (`*_SPA09`); pause-sample channel clamp 0 -> 0.
- **Selection** (`unitsel09.rs`, `unitsel_tables_viol09.rs`): 5-byte labels, 7-byte class keys (half keys 8 bytes),
  start accumulator `1.0 * tc`, no preferred pass or domain handling, 2009 lattice, contiguity
  `-(2*run [+1] + 100*wrun)`; word offsets +0x465 n_phones, +0x0e dur, +0xd6 ctx, +0x2ca edge, +0x478 flag,
  +0x479 brk. The `.his` history is untested (no capture).
- **Wave:** no join smoother (`generate_phrase_traced_era`), centre index 1 of the 5-byte label.

## 6. Verification

| Test | Result |
|---|---|
| `speak_spa_parity` (raw text, 1327 NWS WAVs) | 1327/1327 byte-identical |
| `fetext_spa_parity` | 1366/1366 lines, 89469/89469 records, 11035/11035 sentences |
| `initial_spa_parity` | 1366/1366 |
| `feling_spa_words` | 11035/11035 sentences per stage (words, breaks, contexts); glue to prosody/unitsel 11035 sentences / 89469 words |
| `feling_spa_analysis` | 13767/13767 sentences, 89469/89469 words (byte-exact WA); `lexicon_pass` alone on 184 LTS words: 564/564 passes |
| `prosody_spa_parity` | durations 453660/453660, tones and F0 190994/190994, target features 489167/489167, word rates and sentence pauses 11055/11055, pause gaps 89555/89555, spans 8/8 |
| `unitsel_spa_parity` | 2732/2732 captures, 978334 targets, 0 mismatches |
| `wave_spa_parity` | records 14030/14030 phrases (509019 records); PCM 1416/1416 from records and from selection (incl. 50 parameter sweeps) |
| `speak_old_parity` (Violeta09) | 1366/1366 |
| `fetext_viol09_parity`, `initial_viol09_parity` | fetext 1418/1418 lines, 89605/89605 records; initial 1418/1418, spans 6/6 (`initial_viol09_spans`) |
| `lts_viol09_parity`, `feling_viol09_words`, `feling_viol09_analysis` | lts 564/564; words/breaks/contexts 11034/11034; analysis 13766/13766 sentences |
| `prosody_viol09_parity` | all 11 record types 100% (durations 455427) |
| `unitsel_viol09_parity` | 977,798 targets, 0 mismatches |
| `wave_viol09_parity` | PCM 1416/1416; records from selection 0/508661 differing; `formats_parity_viol09` all 13 formats 1416/1416 |

Ground truth: Frida captures of the engine (front end `wa`/`next`/`initial`/`tok` events, `wa_in`/`wa_out`,
`words_out`/`breaks_out`/`ctx_out`, offset maps, prosody, selection and wave records) over the NWS Spanish
product translations (80 products from MFL, BRO, TBW; ZFP, CWF, SRF, HWO, SPS, NPW, FFA, FFW, FLS, FLW, SVR,
SVS, TOR, MWW, SMW, RFW; 1327 paragraphs, 715.6 min of audio) plus a 39-line synthetic probe corpus, VTML edge
cases and parameter sweeps (about 12.5 GB). All hooked WAVs are SHA1-identical to the unhooked engine. The
harness, corpora and captures are not part of the repository; tests skip when they are absent and take their
locations from `VTPAUL_HARNESS` and `VTPAUL_VOICES` (defaults `..\harness`, `..\all_voices`).

## 7. Open items

- Spanish lip-sync log (2013 and 2009): library only, no CLI; needs Violeta's own phone names (`WE`, `OH`); no
  phone-name table found; neither port has a lip-sync or formats test (except `formats_parity_viol09`).
- Text: currency renderers `FUN_10029730`/`FUN_1002a210`; say-as, part of speech and other phoneme alphabets;
  the VTML normalizer `FUN_1003d710`; URL www-lookback folds and `:port` render. No capture exercises them.
- `k7` per-word array is 0x40 short of the stride pattern (unused by the corpus); domain 8 and the heapsort
  fallback are unexercised (as in English); `.his` history on Violeta09.
- Word text/p2e4/pos/pos_b/domain and phone code/mark offsets, syllable pointers +0x1c/+0x20, VTML tag-stack
  offsets (blocked on two code labels `LAB_100080f0`/`LAB_10008350` that Ghidra has not made functions) and
  ss.type are not decoded; bytes are kept raw in captures.
- About 15 offsets remain hard-coded in `ling_spa` words/breaks/context.

## 8. History

- 2026-09-27: setup (API, license, bank decodes), function map, capture harness for front end and back half.
- 2026-09-27: prosody, unit selection and wave ported bit-exact (cepdist remap found in `us_ViterbiJoinStep`).
- 2026-09-27: words/breaks/contexts 11035/11035; LTS translated from the engine after a rule rewrite (94.4%) was dropped.
- 2026-09-27: analysis 13767/13767; `lexicon_pass` rewritten from the disassembly.
- 2026-09-29: text front end 100% (units, dates, URL path rewritten); `speak_spa_parity` 1327/1327.
- 2026-10-05: Violeta09 ported end to end (1366/1366), formats 1416/1416.
