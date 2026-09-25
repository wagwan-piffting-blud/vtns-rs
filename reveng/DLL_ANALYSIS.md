# vt_eng.dll — Function Inventory by Stage

Engine: `C:\Program Files (x86)\NWSPaulNumber2\bin\vt_eng.dll` 3.11.7.1 ("Paul M16 FILEIO"). Image
base `0x10000000`, 32-bit VC6, `__cdecl` throughout. All addresses below are VAs. Ghidra rename
prefixes: `fe_` front end (text + linguistic stages share it), `dict_` lexicon, `pro_` prosody,
`us_` unit selection, `wav_` waveform. A function still `FUN_<addr>` either hasn't been renamed yet
or is a small/private helper not judged worth a stage-prefix rename.

This is an index, not the authoritative source — every row links back to the stage doc with the
full decompile, asm notes, and (where applicable) the Rust port and its parity test. See
[`README.md`](README.md) for the pipeline diagram these stages compose into.

## Text front end — see [`FRONTEND_TEXT.md`](FRONTEND_TEXT.md)

Status: **bit-exact**, every normalizer, on the full test corpus and end to end.

| Address | Function | Role |
|---|---|---|
| `0x1001c990` | `fe_InitialTextProcess` | Once per request: full-width char mapping, VTML tag extraction (tags → `"<\xa1\xddN>"`/`"</\xa1\xdd>"` markers), orig↔processed offset maps |
| `0x1001ce10` | `fe_InjectDemoNag` | Demo nag / easter egg — dead code, `VT_CheckLicense_ENG` stub always returns 0 |
| `0x1001d250` | `fe_RunTextFilterPass` | Called ×2 by `fe_InitialTextProcess` |
| `0x1001d370` | `fe_MapFullwidthChars` | Full-width → ASCII char remap |
| `0x1001d5d0` | `fe_ExtractVtmlTags` | VTML tag extraction into the processed-text buffer |
| `0x10022dc0` | `fe_NextSentence` | Per-sentence driver loop; also writes `wr::START`/`wr::END` after the ling stage returns, walking its own `TIB+4` cursor — see [`FRONTEND_LING.md`](FRONTEND_LING.md) §0 and [`PROSODY.md`](PROSODY.md) §7.2 |
| `0x10022850` | `fe_ResolveSentencePSV` | Effective sentence pitch/speed/volume — see `PROSODY.md` §7.1 |
| `0x1003d350` | `fe_AnalyzeSentence` | Per-sentence: tokenize → normalize → ling analysis |
| `0x1005a350` | `fe_Tokenize` | Tokenizer — classifies + splits raw text into the pre-normalization token list |
| `0x10044fd0` | `fe_EmitWord` | Token → WA word record emission |
| `0x1003d3d0` | `fe_NormalizeTokens` | The ~20-entry normalizer chain (currency, date, time, units, digits, numword, numexpr, address, n15, compound, symbols, ...) — full per-normalizer table in `FRONTEND_TEXT.md` §4-5 |
| `0x1005e330` | `fe_LoadUserDictCsv` | User dictionary CSV loader |

Selected normalizer entry points (full list of ~20 with ported/not status in `FRONTEND_TEXT.md` §4):
`norm_symbols` `0x100544f0`, `norm_numword` `0x10040ef0`, `norm_date` `0x10036940`, `norm_units`
`0x1003e4a0`, `norm_currency` `0x100344e0`, `norm_time` `0x10058490`, `norm_n15` `0x10031d40`,
`norm_numexpr` `0x100420f0`, `norm_address` `0x10033760`, `norm_digits` `0x10052e80`/`0x10053440`,
`norm_compound` `0x10063780`, `phone` `0x100570c0`, `fraction` `0x1003c1a0`, `math` `0x1003aad0`,
`url` `0x1005c890`.

## Lexicon (`dict-eng` hash dictionaries) — see [`LEXICON.md`](LEXICON.md)

Status: fully decoded and verified — every payload format closed, including the "TPP G classes"
open question (closed by `FRONTEND_LING.md`'s `tpp_field_equivalence` cross-check over all 31,550
TPP payloads).

| Address | Function | Role |
|---|---|---|
| `0x10011cb0` | `fe_LoadHashParams` | Load perfect-hash parameters (`hashparams_emb`) |
| `0x100114e0` | `fe_LoadLexicons` | Open (not fully load) the 4 hashed dictionary files |
| `0x10003660` | `fe_LoadExceptDict` | Load `exceptdict` (2,891 hyphenated/foreign entries) |
| `0x100033e0` | `fe_LoadTxt2Table` | Load a `*.txt2` (+10-shifted sorted text table) |
| `0x1000fd70` | `fe_EncodeLexKey` | Encode a word into a main-lexicon hash key |
| `0x1003a610` | `dict_EncodeTppKey` | Encode a TPP key |
| `0x1001c280` | `dict_StrnCmpU` | Unsigned-byte strncmp, stops at NUL |
| `0x1003a570` | `dict_LookupTpp` | TPP lookup; encodes the key only when selector `== 'E'` |
| `0x10012030` | `dict_PerfectHashSlot` | Perfect-hash slot computation (Jenkins checksum + lookup2) |
| `0x10010c90` | `dict_JenkinsChecksum` | `perfect.c` `checksum()` |
| `0x10010a50` | `dict_JenkinsLookup2` | `perfect.c` `lookup2()` |
| `0x10011820` | `fe_HashDictLookup` | Top-level hashed lookup, MRU-cached when the caller passes a non-null cache pointer |
| `0x10011a50` / `0x10011ac0` | `dict_MruCacheFind` / `dict_MruCacheLookupOrFill` | Circular MRU cache at `TIB+0x13b0` |
| `0x10025440` | `wav_ReadDbFileLocked` | Shared file-read primitive (critical section `0x100a8400`) — every lexicon lookup goes through it; also used by the waveform stage |
| `0x10003a70` | `dict_LookupLexicon` | Main-lexicon lookup wrapper: `fe_EncodeLexKey` → `fe_HashDictLookup` |
| `0x10003c50` | `dict_DecodeLexPayload` | Decode raw payload → 0x554-byte morph candidate |
| `0x1000d450` | `dict_FillMorphFromPayload` | Copy decoded candidate into 0x70-byte morph entries |
| `0x1000ea20` | `dict_CopyMorphsToWordArray` | Copy morph entries + 4 flag bytes into the WA record |
| `0x1000ebc0` → `0x100fe900` | `dict_InitPhoneCodebook` | Phone codebook init |
| `0x1003a7b0` | `dict_GetTppField` | Scan a TPP payload for a field by letter |
| `0x10024b50` | (unnamed) | Cut at next space (`" "` at `0x1007b3cc`) |
| `0x10003b70` | `dict_ExceptDictFind` | exceptdict lookup |
| `0x10008dc0` | `dict_MatchHyphenatedExcept` | Hyphenated/foreign exceptdict matcher |
| `0x1000c9c0` | `dict_CountHyphenParts` | Count hyphen-separated parts (letters/`'`/`-` only) |
| `0x1000ca50` | `dict_ApplyExceptPron` | Pour exceptdict raw phone codes into morph entries |
| `0x1000ca30` | `FUN_1000ca30` | Per-part syllable budget for `dict_ApplyExceptPron` |

## Linguistic front end — see [`FRONTEND_LING.md`](FRONTEND_LING.md)

Status: **bit-exact**, every stage, full 730/730-sentence corpus.
Rust port: `src/ling/*`, `src/lexicon.rs`. Test: `tests/feling_parity.rs`.

| Address | Function | Role | Rust port | Parity |
|---|---|---|---|---|
| `0x1000e2f0` | `fe_TagAndPronounce` | Lookup + copy + morphology + wh-flag + function-word marking, in that order | `ling::analysis::tag_and_pronounce` | (composed of the rows below) |
| `0x1003e240` | `fe_LinguisticAnalysis` | `fe_TagAndPronounce` + the merge loop (groups morphs by token back into WA records) | `ling::merge::merge` | 2154/2154 sentences |
| `0x1000d190` / `0x1000d640` | (lookup / token split) | Lexicon lookup + token splitting | `ling::lookup` | 730/730 |
| `0x1000ea20` | `dict_CopyMorphsToWordArray` | (see Lexicon table) | `ling::analysis` | 730/730 |
| `0x100068b0` | (homograph tagger) | POS disambiguation when morph count ≥ 2 | `ling::tagger` | 730/730 |
| `0x100049b0` / `0x10004300` / `0x10003ff0` | (tagger rules / fallback / article rule) | Homograph tagger sub-rules + poly.tree3 vote | `ling::tagger` | 730/730 |
| `0x10002f10` | `fe_LetterToSound` | LTS fallback for unknown words | `ling::lts` | 3154/3154 |
| `0x10009cd0` | (spell) | Spelled-out reading | `ling::lts::spell` | 866/866 |
| `0x10003110` | (hyphen LTS) | LTS for hyphenated unknowns | `ling::lts` | (covered by lts totals) |
| `0x1000b800` | (clitic morph) | 's/n't/'ve/'em/'re/'ll/'d/'m — concatenated-spelling lookup first, phone-table fallback | `ling::morph` | (covered by morph totals) |
| `0x1000c040` | (`clitic_inword`) | In-word clitic split (`'ers`/`'ee`/`'er`/`'n`); match must reach end of word | `ling::morph::clitic_inword` | 730/730 (fixed via `tail_001`, see `EXPERIMENTS.md`) |
| `0x1000e160` | (dash-join span) | Joins word `i` with up to 5 following class-`'A'` records across literal hyphens, longest-TPP-match-wins | `ling::morph` | 730/730, asm-verified |
| `0x1000cb30` | (`cb30`, suffix stem) | Stem-length candidate lookup used by suffix stripping | `ling::morph::suffix_strip` | (covered by morph totals) |
| `0x10008cc0` | (`is_mixed_case`) | camelCase detection | `ling::morph::is_mixed_case` | (covered by morph totals) |
| `0x1000c3a0` | (`camelcase_split`) | camelCase word splitting, direct per-part lexicon hits | `ling::morph::camelcase_split` | (covered by morph totals) |
| `0x1000c860` | (`split_camel_part`) | Single camelCase part extraction | `ling::morph::split_camel_part` | (covered by morph totals) |
| `0x100086c0` | (`should_spell`) | Decide whether a word should be spelled out; separator-in-spelling adds flag `0x08` | `ling::morph` | 730/730 |
| `0x100091b0` | (`u_token`) | Sub-token/clitic-boundary handling, mode `'Y'` only exercised | `ling::morph` | Open: non-`'Y'` modes unreached by any plain-text corpus line — needs text-front-end coordination |
| `0x1000e990` | (wh-question flag, `wh_flag`) | wh-question detection | `ling::funcwords::wh_flag` | 730/730 |
| `0x1000e570` | (`wh_scan`) | Recursive wh-scan | `ling::funcwords::wh_scan` | (covered above) |
| `0x1000e4e0` / `0x1000e3a0` | (leaf helpers) | Negation list / modal list (reused, not new — same tables as morphology's post-rules) | `ling::funcwords` (uses `tables::L_NEG`/`L_MODAL`) | (covered above) |
| `0x1000e540` / `0x1000e4b0` / `0x1000e510` / `0x1000e430` | (leaf helpers) | Preposition/conjunction/wh-word/wh-be list gates | `ling::funcwords` (`L_WHPREP`/`L_WHCONJ`/`L_WHWORD`/`L_WHBE`) | (covered above) |
| `0x10016c90` | `fe_BuildWordPhoneRecords` | Phone codes, marks, M flags, domain byte, sentence type — does **not** write `wr::START`/`wr::END` (that's `fe_NextSentence`, see §0 of `FRONTEND_LING.md`) | `ling::words::build_word_phone_records` | 730/730 |
| `0x100130e0` | `fe_AssignPhraseBreaks` | Syllabify + break codes + phrase table | `ling::breaks`, `ling::syllab` | 730/730 |
| `0x10012c70` | (syllabify) | Syllabification | `ling::syllab` | (covered above) |
| `0x10012df0` | (phrase table) | Phrase-break table application | `ling::breaks` | (covered above) |
| `0x10017510` | `fe_BuildPhoneContexts` | 7-byte phone contexts | `ling::context` | 730/730 |
| `0x10017100` | (allophones) | Allophone resolution for phone contexts | `ling::context` | (covered above) |

## Prosody — see [`PROSODY.md`](PROSODY.md)

Status: **bit-exact**. Rust port: `src/prosody.rs`.

| Address | Function | Signature | Role |
|---|---|---|---|
| `0x10001050` | `pro_LoadProsodyTrees` | `int(void*, spkdb*)` | Loads 17 CART trees into `speaker+0x494` |
| `0x10001900` | `pro_LoadTree3` | `short(tree*, char* path)` | Patches `.tree2`→`.tree3`, reads, parses |
| `0x10001af0` | `pro_LoadTree3Vec` | same | Vector-tree variant (`*bf`) |
| `0x100016e0` | `pro_ParseTree3` | | Node table + value table, set-size check |
| `0x100015f0` | `pro_TreeFindLeaf` | `uint(tree*, short* fv)` | CART walk |
| `0x100015c0` | `pro_ShortInSet` | `int(short, short*, int)` | Linear membership test |
| `0x10001670` | `pro_TreePredictScalar` | `short(tree*, short*)` | `values[leaf]` |
| `0x100016a0` | `pro_TreePredictVector` | `int(tree*, short*, void*)` | `memcpy dim` shorts from `values[leaf*dim]` |
| `0x100014f0` | `pro_CopyMaybeByteswap` | | memcpy or byteswap-on-read for big-endian host |
| `0x10013380` | `pro_PredictDurations` | `int(spkdb*, tib*)` | Duration CART prediction |
| `0x100135d0` | `pro_DurationFeatures` | `int(short[9], SS*, word, syl, phone)` | Duration feature vector |
| `0x100138c0` | `pro_PredictPitch` | `int(spkdb*, tib*)` | F0 CART prediction |
| `0x10013a20` | `pro_PitchFeatures` | `int(short[11], SS*, word, syl)` | F0 feature vector |
| `0x100137c0` | `pro_SmoothF0AcrossSyllables` | `int(phrase, tib*)` | Cross-syllable F0 smoothing |
| `0x10013790` | `pro_RoundedMean` | `short(short*, short n)` | `(n + 2Σ) / (2n)` |
| `0x10022970` | `pro_ApplyTagSpansToWords` | `int(tib*)` | VTML prosody-tag span application |
| `0x10026750` | `pro_ResolveSentencePause` | `int(spk*, tib*)` | Sentence-level pause resolution |
| `0x1002e890` | `pro_PauseSamples` (= `wav_PauseBeforeWord`) | `void(tib*, short gap) → SS+8` | Pause gap in samples; shared with the waveform stage |
| `0x10022fd0` | `pro_QuantizeDurationDiv50` | `u8(short)` | Duration quantization |
| `0x10022850` | `fe_ResolveSentencePSV` | `void(spk*, tib*)` | Effective sentence pitch/speed/volume (front-end name, shared with text stage) |

## Unit selection — see [`UNITSEL.md`](UNITSEL.md)

Status: **bit-exact**, 3269/3269 target-feature builds. Rust port: `src/unitsel.rs`,
`src/classes.rs`.

| Address | Function | Role |
|---|---|---|
| `0x10024980` | `us_SelectUnitsForPhrase` | Per-phrase driver, called from `wav_SelectAndGeneratePhrase` (`0x1002c9b0`) |
| `0x10019f40` | `us_LoadUnitIndexes` | Unit index load; sets EXP flag `\|= 0x20` on every domain-0x0c unit; also resolves `.his` pool names |
| `0x1001b0d0` | `us_ReadPrimaryRecord2013` | Lazy 19-byte primary record read for chosen units |
| `0x1001ae20` | `us_ScalarDist` | Scalar distance: `m=(a+b)/2; m==0?0:(a-b)²/m` |
| `0x1002dd70` | `us_NRHeapSortHalfKeys` | Numerical Recipes heapsort of half-phone keys |
| `0x10024680` | `us_BuildTargets` | Per-word target construction (whole-phone or half-phone) |
| `0x10024060` | `us_LookupWholePhone` | Whole-phone target lookup |
| `0x100242a0` | `us_LookupHalfPhones` | Half-phone target lookup (h=0,1) |
| `0x10018770` | `us_ContextBackoffLattice` | Mode 0/1/2 context back-off lattice |
| `0x10023350` | `us_GetCandidates` | Target-feature build + candidate gather |
| `0x100230a0` | `us_ScoreContiguity` | Contiguity scoring (label flag `0x80` = "unit u+1 continues the recording") |
| `0x100182e0` | `us_TargetCost` | Target cost over `{L2,L1,C,R1,R2,pos,flags}` |
| `0x10018c80` | `us_ViterbiJoinStep` | Viterbi join-cost step |
| `0x10024510` | `us_BeamPrune` | Beam pruning (only when `word.domain==0 && n>10`) |
| `0x1001b5f0` | `us_SortByScore` | Introsort |
| `0x100125d0` | `us_LoadSelectHistory` | `.his` load |
| `0x10022350` | `us_VTDTTS_MakeInfo_Impl` | `.dtt` (bin+asc) output setup |
| `0x1002ca10` | `us_DumpDttRecords` | Per-unit-record `.dtt` dump |

## Waveform — see [`WAVEFORM.md`](WAVEFORM.md)

Status: **bit-exact**, 61 functions renamed. Rust port: `src/wave.rs`, `src/formats.rs`.

| Address | Function | Role |
|---|---|---|
| `0x10026870` | `wav_NextChunkWithEmphasis` | One output chunk (≤60000 bytes + slack) then emphasis filter |
| `0x100267d0` | `wav_FillChunk` | Repeats phrase selection/generation until the chunk is full |
| `0x1002c9b0` | `wav_SelectAndGeneratePhrase` | Stage-limited driver (`tib+0x20`: 1 stop-before-selection, 2 no-synth, 3 selection-only) |
| `0x1002bd90` | `wav_GeneratePhrase` | Resumable per-record generation loop |
| `0x1002c220` | `wav_BuildUnitRecords` | Unit + pause records for one phrase |
| `0x1002c120` | `wav_FillUnitRecord` | Per-mode span/overlap/pitch-mark view |
| `0x1002e890` | `wav_PauseBeforeWord` | Pause before word w, in samples |
| `0x1002c530` | `wav_BuildSyncEntries` | Word/phone sync-ring entries |
| `0x1002c8b0` | `wav_DecodeUnit` | Shorten-decode + span copy + volume |
| `0x10001b30` | `wav_ShortenDecode` | Shorten codec core |
| `0x10001ea0` / `0x100020a0` / `0x10002170` | `wav_UvarGet` / `wav_SvarGet` / `wav_UlongGet` | Rice codes |
| `0x10001fb0` | `wav_BitRefillBE32` | 32-bit BE bitstream refill |
| `0x100020d0` / `0x10002100` | `wav_FixBitshift` / `wav_EmitPcm16` | Output bitshift + clamp |
| `0x10025440` | `wav_ReadDbFileLocked` | Shared locked file read (see Lexicon table) |
| `0x1002bbd0` | `wav_ReadPitchMarks` | `.upm` pitch-mark read |
| `0x1002bc60` | `wav_BuildEpochTable` | Epoch table build |
| `0x1002bd20` | `wav_FindNearestEpoch` | Nearest-epoch search |
| `0x1002d230` | `wav_SmoothJoins` | Join smoother |
| `0x1002d1e0` | `wav_LoadNeighbourUnit` | Neighbour-unit decode for smoothing |
| `0x1002d010` / `0x1002cdf0` | `wav_FadeOutPeriod` / `wav_FadeInPeriod` | Period-domain crossfades |
| `0x1002aac0` | `wav_OlaUnitUnmodified` | OLA path (pitch=dur=100) |
| `0x1002afb0` | `wav_PsolaUnit` | PSOLA path (pitch or dur ≠ 100) |
| `0x100200c0`/`0x10020460`/`0x10020930`/`0x10020e00` | `wav_TextToBuffer{S16,Alaw,Ulaw,Dadpcm}` | Buffer output API |
| `0x1001e290`/`0x1001e430`/`0x1001e630`/`0x1001ed40` | `wav_TextToRaw{S16,Alaw,Ulaw}File`, `wav_TextToDadpcmFile` | Raw file output |
| `0x1001e930` | `wav_TextToWaveFile` | RIFF writer (tags 1/0x99/6/7/0x11) |
| `0x1001e830`/`0x1001e870`/`0x1001e8b0`/`0x1001e8f0` | `wav_TextToWave{S16,U8,Alaw,Ulaw}` | WAVE-format entry points |
| `0x1001fc40` | `wav_TextToAuFile` | AU output (broken header — see `WAVEFORM.md` §8.6) |
| `0x1001f1f0`/`0x1001f600` | `wav_InitWaveHeader`/`wav_WriteWaveHeader` | WAVE header build/rewrite |
| `0x10020080` | `wav_InitAuHeader` | AU header |
| `0x1002e780`/`0x1002e720` | `wav_Linear2Alaw`/`wav_Linear2Ulaw` | Encoders |
| `0x1002e860`/`0x1002e830` | `wav_BuildAlawLut`/`wav_BuildUlawLut` | Encoder LUT build |
| `0x1002e800`/`0x1002e700` | `wav_LoadUlawDecodeTable`/`wav_UlawDecodeEntry` | µ-law decode table (unused on the synth path) |
| `0x1002e660`/`0x1002e4c0`/`0x1002e550`/`0x1002e600`/`0x1002e4a0` | `wav_Dadpcm{EncodeChunk,EncodeSample,Update,IndexAdjust,Reset}` | Dialogic ADPCM |
| `0x10025500`/`0x100255a0`/`0x10025400` | `wav_FileWriteAt`/`wav_FileWriteAtSwapped`/`wav_FileSeek` | Positioned file I/O |
| `0x1001df10`/`0x1001e0c0`/`0x10022200` | `wav_OpenLipSyncLog`/`wav_FlushLipSyncLog`/`wav_TextToLipSyncLog` | Lip-sync log |
| `0x10026b40` | `wav_PlayStart` | `waveOutOpen` + double-buffered playback |

## Cross-stage / shared infrastructure

| Address | Function | Used by |
|---|---|---|
| `0x10025440` | `wav_ReadDbFileLocked` | Lexicon reads, waveform DB reads — single global critical section (`0x100a8400`) around all file I/O |
| `0x1000e2f0` → `0x1003e240` | `fe_TagAndPronounce` → `fe_LinguisticAnalysis` | The full ling-stage entry point called from `fe_AnalyzeSentence` |
| `0x10022dc0` | `fe_NextSentence` | Drives the text stage per sentence AND is the true writer of ling-stage word char offsets — see `FRONTEND_LING.md` §0 |
