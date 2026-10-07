# vt_eng.dll: function inventory by stage

Engine: `C:\Program Files (x86)\NWSPaulNumber2\bin\vt_eng.dll` 3.11.7.1 ("Paul M16 FILEIO"), image base
`0x10000000`, 32-bit VC6, `__cdecl`. All addresses are VAs. Rename prefixes: `fe_` front end, `dict_`
lexicon, `pro_` prosody, `us_` unit selection, `wav_` waveform; `FUN_<addr>` is unrenamed or a small
private helper.

An index, not the authority: each section links to the stage doc with the decompile notes and the Rust
port. Pipeline overview in [README.md](README.md). All stages are bit-exact.

## Text front end ([FRONTEND_TEXT.md](FRONTEND_TEXT.md))

| Address | Function | Role |
|---|---|---|
| 0x1001c990 | `fe_InitialTextProcess` | Once per request: fullwidth map, VTML tag extraction (tags become `"<\xa1\xddN>"` / `"</\xa1\xdd>"` markers), orig-to-processed offset maps |
| 0x1001ce10 | `fe_InjectDemoNag` | Dead code; `VT_CheckLicense_ENG` stub returns 0 |
| 0x1001d250 | `fe_RunTextFilterPass` | Called twice by `fe_InitialTextProcess` |
| 0x1001d370 | `fe_MapFullwidthChars` | Fullwidth to ASCII |
| 0x1001d5d0 | `fe_ExtractVtmlTags` | VTML extraction into the processed-text buffer |
| 0x10022dc0 | `fe_NextSentence` | Per-sentence driver; writes `wr::START`/`wr::END` after the linguistic stage, walking its own `TIB+4` cursor |
| 0x10022850 | `fe_ResolveSentencePSV` | Effective sentence pitch/speed/volume |
| 0x1003d350 | `fe_AnalyzeSentence` | Tokenize, normalize, linguistic analysis |
| 0x1005a350 | `fe_Tokenize` | Raw text to pre-normalization tokens |
| 0x10044fd0 | `fe_EmitWord` | Token to WA word record |
| 0x1003d3d0 | `fe_NormalizeTokens` | The ~20-entry normalizer chain |
| 0x1005e330 | `fe_LoadUserDictCsv` | User dictionary loader |

Normalizer entry points (full list in FRONTEND_TEXT.md): `norm_symbols` 0x100544f0, `norm_numword`
0x10040ef0, `norm_date` 0x10036940, `norm_units` 0x1003e4a0, `norm_currency` 0x100344e0, `norm_time`
0x10058490, `norm_n15` 0x10031d40, `norm_numexpr` 0x100420f0, `norm_address` 0x10033760, `norm_digits`
0x10052e80/0x10053440, `norm_compound` 0x10063780, `phone` 0x100570c0, `fraction` 0x1003c1a0, `math`
0x1003aad0 (dead code in 2014), `url` 0x1005c890.

## Lexicon ([LEXICON.md](LEXICON.md))

| Address | Function | Role |
|---|---|---|
| 0x10011cb0 | `fe_LoadHashParams` | Perfect-hash parameters (`hashparams_emb`) |
| 0x100114e0 | `fe_LoadLexicons` | Opens the four hashed dictionaries |
| 0x10003660 | `fe_LoadExceptDict` | `exceptdict` (2,891 entries) |
| 0x100033e0 | `fe_LoadTxt2Table` | `*.txt2` tables |
| 0x1000fd70 | `fe_EncodeLexKey` | Main-lexicon key |
| 0x1003a610 | `dict_EncodeTppKey` | TPP key |
| 0x1001c280 | `dict_StrnCmpU` | Unsigned `strncmp` |
| 0x1003a570 | `dict_LookupTpp` | TPP lookup; encodes only for selector `'E'` |
| 0x10012030 | `dict_PerfectHashSlot` | Slot from Jenkins checksum + lookup2 |
| 0x10010c90 / 0x10010a50 | `dict_JenkinsChecksum` / `dict_JenkinsLookup2` | `perfect.c` |
| 0x10011820 | `fe_HashDictLookup` | Top-level lookup, MRU-cached for non-null cache pointer |
| 0x10011a50 / 0x10011ac0 | `dict_MruCacheFind` / `dict_MruCacheLookupOrFill` | Cache at `TIB+0x13b0` |
| 0x10025440 | `wav_ReadDbFileLocked` | Shared file read, critical section 0x100a8400; also used by waveform |
| 0x10003a70 | `dict_LookupLexicon` | `fe_EncodeLexKey` then `fe_HashDictLookup` |
| 0x10003c50 | `dict_DecodeLexPayload` | Payload to 0x554-byte morph candidate |
| 0x1000d450 | `dict_FillMorphFromPayload` | Candidate to 0x70-byte morph entries |
| 0x1000ea20 | `dict_CopyMorphsToWordArray` | Morph entries + 4 flag bytes into the WA record |
| 0x1000ebc0 | `dict_InitPhoneCodebook` | Codebook at 0x100fe900 |
| 0x1003a7b0 | `dict_GetTppField` | TPP field by letter; cut at next space (`FUN_10024b50`, 0x1007b3cc) |
| 0x10003b70 | `dict_ExceptDictFind` | `exceptdict` lookup |
| 0x10008dc0 | `dict_MatchHyphenatedExcept` | Hyphenated/foreign matcher |
| 0x1000c9c0 | `dict_CountHyphenParts` | Letters, `'`, `-` only |
| 0x1000ca50 / 0x1000ca30 | `dict_ApplyExceptPron` / `FUN_1000ca30` | Pour `exceptdict` phones into morph entries / per-part syllable budget |

## Linguistic front end ([FRONTEND_LING.md](FRONTEND_LING.md))

Rust: `src/ling/*`, `src/lexicon.rs`. Test: `tests/feling_parity.rs`.

| Address | Function | Role | Rust | Parity |
|---|---|---|---|---|
| 0x1000e2f0 | `fe_TagAndPronounce` | Lookup + copy, morphology, wh-flag, function-word marking, in that order | `ling::analysis::tag_and_pronounce` | composed of rows below |
| 0x1003e240 | `fe_LinguisticAnalysis` | `fe_TagAndPronounce` + merge loop (morphs back into WA records) | `ling::merge::merge` | 2154/2154 sentences |
| 0x1000d190 / 0x1000d640 | (lookup / token split) | Lexicon lookup, token splitting | `ling::lookup` | 730/730 |
| 0x100068b0 | (homograph tagger) | POS disambiguation when morph count >= 2 | `ling::tagger` | 730/730 |
| 0x100049b0 / 0x10004300 / 0x10003ff0 | (tagger rules / fallback / article rule) | Sub-rules + `poly.tree3` vote | `ling::tagger` | 730/730 |
| 0x10002f10 | `fe_LetterToSound` | LTS for unknown words | `ling::lts` | 3154/3154 |
| 0x10009cd0 | (spell) | Spelled-out reading | `ling::lts::spell` | 866/866 |
| 0x10003110 | (hyphen LTS) | LTS for hyphenated unknowns | `ling::lts` | in LTS totals |
| 0x1000b800 | (clitic morph) | `'s`/`n't`/`'ve`/`'em`/`'re`/`'ll`/`'d`/`'m`: concatenated-spelling lookup, then phone-table fallback | `ling::morph` | in morph totals |
| 0x1000c040 | `clitic_inword` | `'ers`/`'ee`/`'er`/`'n`; match must reach end of word | `ling::morph::clitic_inword` | 730/730 |
| 0x1000e160 | (dash-join span) | Word `i` plus up to 5 following class-`'A'` records across hyphens, longest TPP match wins | `ling::morph` | 730/730 |
| 0x1000cb30 | `cb30` | Stem-length candidates for suffix stripping | `ling::morph::suffix_strip` | in morph totals |
| 0x10008cc0 / 0x1000c3a0 / 0x1000c860 | `is_mixed_case` / `camelcase_split` / `split_camel_part` | camelCase detection and splitting | `ling::morph` | in morph totals |
| 0x100086c0 | `should_spell` | Spell-out decision; separator in spelling adds flag 0x08 | `ling::morph` | 730/730 |
| 0x100091b0 | `u_token` | Sub-token/clitic boundary; only mode `'Y'` seen | `ling::morph` | open: non-`'Y'` modes unreached |
| 0x1000e990 / 0x1000e570 | `wh_flag` / `wh_scan` | wh-question detection (recursive scan) | `ling::funcwords` | 730/730 |
| 0x1000e4e0 / 0x1000e3a0 | (leaf helpers) | Negation / modal lists (`tables::L_NEG`/`L_MODAL`) | `ling::funcwords` | covered |
| 0x1000e540 / 0x1000e4b0 / 0x1000e510 / 0x1000e430 | (leaf helpers) | Preposition / conjunction / wh-word / wh-be gates (`L_WHPREP`/`L_WHCONJ`/`L_WHWORD`/`L_WHBE`) | `ling::funcwords` | covered |
| 0x10016c90 | `fe_BuildWordPhoneRecords` | Phone codes, marks, M flags, domain byte, sentence type; does not write `wr::START`/`wr::END` | `ling::words::build_word_phone_records` | 730/730 |
| 0x100130e0 | `fe_AssignPhraseBreaks` | Syllabify, break codes, phrase table | `ling::breaks`, `ling::syllab` | 730/730 |
| 0x10012c70 / 0x10012df0 | (syllabify / phrase table) | | `ling::syllab` / `ling::breaks` | covered |
| 0x10017510 | `fe_BuildPhoneContexts` | 7-byte phone contexts | `ling::context` | 730/730 |
| 0x10017100 | (allophones) | Allophone resolution | `ling::context` | covered |

## Prosody ([PROSODY.md](PROSODY.md))

Rust: `src/prosody.rs`.

| Address | Function | Signature | Role |
|---|---|---|---|
| 0x10001050 | `pro_LoadProsodyTrees` | `int(void*, spkdb*)` | 17 CART trees into `speaker+0x494` |
| 0x10001900 | `pro_LoadTree3` | `short(tree*, char* path)` | Patches `.tree2` to `.tree3`, reads, parses |
| 0x10001af0 | `pro_LoadTree3Vec` | same | Vector-tree variant (`*bf`) |
| 0x100016e0 | `pro_ParseTree3` | | Node + value tables, set-size check |
| 0x100015f0 | `pro_TreeFindLeaf` | `uint(tree*, short* fv)` | CART walk |
| 0x100015c0 | `pro_ShortInSet` | `int(short, short*, int)` | Linear membership |
| 0x10001670 / 0x100016a0 | `pro_TreePredictScalar` / `pro_TreePredictVector` | | `values[leaf]` / copy `dim` shorts |
| 0x100014f0 | `pro_CopyMaybeByteswap` | | memcpy or byteswap for big-endian hosts |
| 0x10013380 | `pro_PredictDurations` | `int(spkdb*, tib*)` | Duration CART |
| 0x100135d0 | `pro_DurationFeatures` | `int(short[9], SS*, word, syl, phone)` | Duration features |
| 0x100138c0 | `pro_PredictPitch` | `int(spkdb*, tib*)` | F0 CART |
| 0x10013a20 | `pro_PitchFeatures` | `int(short[11], SS*, word, syl)` | F0 features |
| 0x100137c0 | `pro_SmoothF0AcrossSyllables` | `int(phrase, tib*)` | F0 smoothing |
| 0x10013790 | `pro_RoundedMean` | `short(short*, short n)` | `(n + 2Σ) / (2n)` |
| 0x10022970 | `pro_ApplyTagSpansToWords` | `int(tib*)` | VTML span application |
| 0x10026750 | `pro_ResolveSentencePause` | `int(spk*, tib*)` | Sentence pause |
| 0x1002e890 | `pro_PauseSamples` (= `wav_PauseBeforeWord`) | `void(tib*, short gap)`, writes `SS+8` | Pause in samples; shared with waveform |
| 0x10022fd0 | `pro_QuantizeDurationDiv50` | `u8(short)` | Duration quantization |

## Unit selection ([UNITSEL.md](UNITSEL.md))

Rust: `src/unitsel.rs`, `src/classes.rs`. 3269/3269 target-feature builds.

| Address | Function | Role |
|---|---|---|
| 0x10024980 | `us_SelectUnitsForPhrase` | Per-phrase driver, called from `wav_SelectAndGeneratePhrase` (0x1002c9b0) |
| 0x10019f40 | `us_LoadUnitIndexes` | Index load; sets EXP flag `\|= 0x20` on domain-0x0c units; resolves `.his` pool names |
| 0x1001b0d0 | `us_ReadPrimaryRecord2013` | Lazy 19-byte primary record read |
| 0x1001ae20 | `us_ScalarDist` | `m=(a+b)/2; m==0 ? 0 : (a-b)²/m` |
| 0x1002dd70 | `us_NRHeapSortHalfKeys` | Heapsort of half-phone keys |
| 0x10024680 | `us_BuildTargets` | Per-word targets (whole or half phone) |
| 0x10024060 / 0x100242a0 | `us_LookupWholePhone` / `us_LookupHalfPhones` | Target lookup (half: h=0,1) |
| 0x10018770 | `us_ContextBackoffLattice` | Mode 0/1/2 back-off lattice |
| 0x10023350 | `us_GetCandidates` | Target features + candidate gather |
| 0x100230a0 | `us_ScoreContiguity` | Label flag 0x80 = unit u+1 continues the recording |
| 0x100182e0 | `us_TargetCost` | Cost over `{L2,L1,C,R1,R2,pos,flags}` |
| 0x10018c80 | `us_ViterbiJoinStep` | Join-cost step |
| 0x10024510 | `us_BeamPrune` | Only when `word.domain==0 && n>10` |
| 0x1001b5f0 | `us_SortByScore` | Introsort |
| 0x100125d0 | `us_LoadSelectHistory` | `.his` load |
| 0x10022350 / 0x1002ca10 | `us_VTDTTS_MakeInfo_Impl` / `us_DumpDttRecords` | `.dtt` output |

## Waveform ([WAVEFORM.md](WAVEFORM.md))

Rust: `src/wave.rs`, `src/formats.rs`. 61 functions renamed.

| Address | Function | Role |
|---|---|---|
| 0x10026870 | `wav_NextChunkWithEmphasis` | One chunk (<= 60000 bytes + slack), then emphasis filter |
| 0x100267d0 | `wav_FillChunk` | Repeats selection/generation until the chunk is full |
| 0x1002c9b0 | `wav_SelectAndGeneratePhrase` | Stage-limited driver (`tib+0x20`: 1 stop before selection, 2 no synth, 3 selection only) |
| 0x1002bd90 | `wav_GeneratePhrase` | Resumable per-record loop |
| 0x1002c220 | `wav_BuildUnitRecords` | Unit + pause records for a phrase |
| 0x1002c120 | `wav_FillUnitRecord` | Per-mode span/overlap/pitch-mark view |
| 0x1002e890 | `wav_PauseBeforeWord` | Pause before word w, samples |
| 0x1002c530 | `wav_BuildSyncEntries` | Word/phone sync ring |
| 0x1002c8b0 | `wav_DecodeUnit` | Shorten decode, span copy, volume |
| 0x10001b30 | `wav_ShortenDecode` | Shorten core |
| 0x10001ea0 / 0x100020a0 / 0x10002170 | `wav_UvarGet` / `wav_SvarGet` / `wav_UlongGet` | Rice codes |
| 0x10001fb0 | `wav_BitRefillBE32` | 32-bit BE bitstream refill |
| 0x100020d0 / 0x10002100 | `wav_FixBitshift` / `wav_EmitPcm16` | Bitshift + clamp |
| 0x10025440 | `wav_ReadDbFileLocked` | Locked file read |
| 0x1002bbd0 | `wav_ReadPitchMarks` | `.upm` read |
| 0x1002bc60 / 0x1002bd20 | `wav_BuildEpochTable` / `wav_FindNearestEpoch` | Epoch table, nearest epoch |
| 0x1002d230 | `wav_SmoothJoins` | Join smoother |
| 0x1002d1e0 | `wav_LoadNeighbourUnit` | Neighbour decode for smoothing |
| 0x1002d010 / 0x1002cdf0 | `wav_FadeOutPeriod` / `wav_FadeInPeriod` | Period crossfades |
| 0x1002aac0 | `wav_OlaUnitUnmodified` | OLA (pitch = dur = 100) |
| 0x1002afb0 | `wav_PsolaUnit` | PSOLA (pitch or dur != 100) |
| 0x100200c0 / 0x10020460 / 0x10020930 / 0x10020e00 | `wav_TextToBuffer{S16,Alaw,Ulaw,Dadpcm}` | Buffer output |
| 0x1001e290 / 0x1001e430 / 0x1001e630 / 0x1001ed40 | `wav_TextToRaw{S16,Alaw,Ulaw}File`, `wav_TextToDadpcmFile` | Raw file output |
| 0x1001e930 | `wav_TextToWaveFile` | RIFF writer (tags 1/0x99/6/7/0x11) |
| 0x1001e830 / 0x1001e870 / 0x1001e8b0 / 0x1001e8f0 | `wav_TextToWave{S16,U8,Alaw,Ulaw}` | WAVE entry points |
| 0x1001fc40 | `wav_TextToAuFile` | AU output (broken header, WAVEFORM.md section 8.6) |
| 0x1001f1f0 / 0x1001f600 / 0x10020080 | `wav_InitWaveHeader` / `wav_WriteWaveHeader` / `wav_InitAuHeader` | Headers |
| 0x1002e780 / 0x1002e720 | `wav_Linear2Alaw` / `wav_Linear2Ulaw` | Encoders |
| 0x1002e860 / 0x1002e830 | `wav_BuildAlawLut` / `wav_BuildUlawLut` | Encoder LUTs |
| 0x1002e800 / 0x1002e700 | `wav_LoadUlawDecodeTable` / `wav_UlawDecodeEntry` | µ-law decode (unused on the synth path) |
| 0x1002e660 / 0x1002e4c0 / 0x1002e550 / 0x1002e600 / 0x1002e4a0 | `wav_Dadpcm{EncodeChunk,EncodeSample,Update,IndexAdjust,Reset}` | Dialogic ADPCM |
| 0x10025500 / 0x100255a0 / 0x10025400 | `wav_FileWriteAt` / `wav_FileWriteAtSwapped` / `wav_FileSeek` | Positioned file I/O |
| 0x1001df10 / 0x1001e0c0 / 0x10022200 | `wav_OpenLipSyncLog` / `wav_FlushLipSyncLog` / `wav_TextToLipSyncLog` | Lip-sync log |
| 0x10026b40 | `wav_PlayStart` | `waveOutOpen` + double-buffered playback |

## Shared infrastructure

| Address | Function | Used by |
|---|---|---|
| 0x10025440 | `wav_ReadDbFileLocked` | Lexicon and waveform reads; one global critical section (0x100a8400) around all file I/O |
| 0x1000e2f0 to 0x1003e240 | `fe_TagAndPronounce` to `fe_LinguisticAnalysis` | Linguistic entry point called from `fe_AnalyzeSentence` |
| 0x10022dc0 | `fe_NextSentence` | Text-stage driver and the real writer of word character offsets |

## Other builds

| Build | Binary | Notes |
|---|---|---|
| 2014 x64 voices | `vt_pau.dll`, `vt_kat.dll`, `vt_jul.dll`, `vt_jam.dll` (x64, 2014-01) | Same pipeline; float math in selection is single precision (`FUN_18001bc30` target-cost tail, `FUN_18001c7a0` join step), see [UNITSEL.md](UNITSEL.md) |
| Violeta 2013 | `vt_spa_violeta16.dll` | 898 function starts from exports plus direct calls; own function map in [VIOLETA.md](VIOLETA.md) |
| Paul09 | `vt_eng_paul16.dll` (2009-09-15, byte-identical to its `vt_pau.dll`; 61 exports) | 2014 adds 5 exports (`VT_CheckUserDict_SourceNorm`, `VT_GetPathKey`, `VT_SetDecimal0Pron`, `VT_SetPhone0Pron`, `VT_SpeakersInfo`, all `_ENG`); [OLD_PAUL.md](OLD_PAUL.md) |
| Julie09 | `vt_eng_julie16.dll` (Dec 2009); `VT_CheckLicense_ENG` 0x10028d50 | 4-entry speaker table at 0x10078430 (Kate 0, Paul 1, em001 2, Julie 3; 6 dwords each) |
| Violeta09 | `vt_spa_violeta16.dll` (Dec 2009); `VT_CheckLicense_SPA` 0x100122c0 | Single speaker slot |

Decompile-level diff of the 2009 builds against their 2014 counterparts (callees renamed through the
function map; counts of matched functions):

| Pair | same | changed | unmatched |
|---|---:|---:|---:|
| Julie09 to 2014 English | 488 | 468 | 130 |
| Paul09 to 2014 English | 441 | 490 | 146 |
| Violeta09 to Violeta 2013 | 471 | 341 | 85 |
| Julie09 to Paul09 | 768 | 221 | 97 |

Diffs of 4 lines or fewer are mostly decompiler noise. Struct offsets move (`pro_LoadProsodyTrees` takes
`param_1 + 0x498` in 2009, `+ 0x494` in 2014; the SS pointer is at TIB+0x54 in Paul09, +0x4c in Julie09).
Julie09 and Paul09 differ mostly in the text front end (`fe_Tokenize`, `fe_NormalizeTokens`, about 10
large normalizers). The 2009 builds have no join smoother (`wav_SmoothJoins` absent). The 2014 engine
cannot load any 2009 bank (Paul09 fails with error 3 for the missing `.tree3`, Julie09 with 7).

## History

- 2026-09: stage renames and function inventory built up per stage (text, linguistic, prosody, unit selection, waveform).
- 2026-09-28: Paul09 inventory ([OLD_PAUL.md](OLD_PAUL.md)).
- 2026-10-05: 2009 function maps and cross-reference diffs for Paul09, Julie09, Violeta09.
