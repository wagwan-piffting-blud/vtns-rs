# Paul versions

Working log for the Paul builds other than the one the port targets. Paths are relative to the
repository root; the voice data itself is not part of the repository.

## 2026-09-28: inventory of `all_voices\old_paul_install`

### Builds

| Build | File | Arch | Link time (UTC) | Version resource | Notes |
|---|---|---|---|---|---|
| 2009 | `old_paul_install\Paul\lib\vt_pau.dll` | x86 | 2009-09-15 00:24 | none | byte-identical to `vt_eng_paul16.dll` in the same folder; 61 exports |
| 2014 (port target) | `vt_pau.dll` / NWS `vt_eng.dll` | x86 | 2014-01-14 05:50 | 3.11.7.1 | "vt_eng(PAUL-M16-FILEIO)"; 66 exports |
| 2014 x64 | `all_voices\bin\vt_pau.dll` | x64 | 2014-01-16 06:07 | 3.11.7.1 | the `--x64` sound |

- The 2009 install ships `vtengsapi50.dll` "VoiceText SAPI 5.1 English" 3.10.0.0 (2009-06-26).
  It also ships `UserDicEng.exe` with its CHMs, `dict_conv.dll` (2004), `VW_Pron.ttf` and the SAPI 5.1 SDK
  `TTSApp.exe` sample.
- The 2014 build adds 5 exports: `VT_CheckUserDict_SourceNorm`, `VT_GetPathKey`, `VT_SetDecimal0Pron`,
  `VT_SetPhone0Pron` and `VT_SpeakersInfo` (all `_ENG`). Every 2009 export still exists in 2014, so the
  harness drives the 2009 DLL unchanged. For example:
  `VT_DLL=...\old_paul_install\Paul\lib\vt_pau.dll VT_ROOT=.../old_paul_install/Paul/ harness\vtprobe.exe "<text>" out.wav`
  synthesizes normally, with no demo nag.

### Data (`old_paul_install\Paul` vs `all_voices`)

- **Voice database.** The 2009 database (`data-paul\M16`) is a different, smaller recording set:

  | Pool | 2009 units | 2014 units |
  |---|---:|---:|
  | gen | 272,026 | 440,124 |
  | num | 3,385 | 24,508 |
  | etc | 5,172 | 115,723 |
  | alp | 119 | 119 |

  `merged-gen.dat` is 210 MB in 2009 and 347 MB in 2014. `cepdist.tbl` and `dblist.idx` are identical.
- **Unit indexes.**
  - The headers read `ver.2009` (2014: `ver.2013`). The layout is the same up to the 19-byte primary records.
  - Per-unit columns take 20 bytes, not 21: the file index (1), a 7-byte field, and the 3 boundary sets
    (12). There is no duration-code byte.
  - The 7-byte field holds 0/1 values in 2009, where 2014 has the phone-context label (`Z`...).
  - The byte after the dat name is nonzero in 2009 (2, 3, 100, 0).
  - The 2014 DLL still carries the legacy readers `us_ReadUnitColumnsLegacy` (0x100197d0) and
    `us_ReadPrimaryRecordLegacy` (0x1001af50), so the 2009 column layout can be read from 2014 code in Ghidra.
- **Prosody and front-end trees.**
  - The 2009 files are `.tree2`: `ttsdata\tree\{duration,pitch}\*.tree2` and
    `dict-eng\{atmt,engbi,poly,sbd}.tree2`.
  - 2014 uses `.tree3`, and `pro_LoadTree3` rewrites the `.tree2` name to `.tree3`.
  - The formats differ. tree2 has no `u16 n_nodes / u8 dim / u32 n_set_total` header (`qbt.tree2` starts
    `01 44 00 00 ...`), and its sizes differ (`atmt` is 449 KB vs 292 KB, `engbi` is 1.6 KB vs 12 KB).
  - The tree2 parser is only in the 2009 DLL.
- **Lexicon.**
  - `engttsdict_emb` and `hashcont/hashidx/hashparams_emb` differ (2009 is slightly smaller), as do
    `exceptdict`, `chc_sort.txt2` and `streeta_sort.txt2`.
  - `tppdict_eng`, the `*_tpp` hashes and the other `*_sort.txt2` files are identical.
  - The 2009 `userdict_eng.csv` has 361 bytes of entries; the current one is empty.

### Cross-compatibility

- The 2014 DLL on the 2009 data fails: `VT_LOADTTS_ENG` returns 3, because there are no `.tree3` files.
- The port on the 2009 data panics in `bank.rs` `load_pool`. It only implements the ver.2013 column layout.

### Code diff (`tools\oldpaul_funcmap.py` -> `tools\oldpaul_funcmap.tsv`, same method as Violeta batch 0)

- 1015 functions in 2009 vs 1034 in 2014.
- Matches:

  | Kind | Count |
  |---|---:|
  | exports | 56 |
  | identical normalized code | 548 |
  | call graph | 304 |
  | fuzzy | 26 |
  | unmatched | 81 |

- Changed functions with known names:
  - unit selection: 27 `us_*`, including target building, beam pruning, class ranges and the unit-index readers;
  - prosody: `pro_PredictDurations`, `pro_PredictPitch`, `pro_PitchFeatures`, `pro_DurationFeatures`;
  - front end: `fe_Tokenize` (1.6 KB smaller in 2009), `fe_NormalizeTokens` and most normalizers
    (symbols, numexpr, numword, currency, date, units);
  - 23 `wav_*` functions.
- Largest unmatched 2009 functions: 0x1002b020 (2.9 KB), 0x100615af, 0x10044340, 0x10019680 and 0x10017080.
  The tree2 parser should be among them.

### What supporting the 2009 voice would take

This amounts to a second engine port, sized like Violeta batches 1-4 but reusing most of the code (about
half the functions are identical):

1. Open the 2009 `vt_pau.dll` in Ghidra.
2. Port the tree2 parser.
3. Add the ver.2009 index columns.
4. Diff the changed `us_*`, `pro_*` and `wav_*` functions against the port.
5. Build per-stage captures with the existing harness. Pointing `VT_DLL`/`VT_ROOT` at the old install
   already works; the Frida hook offsets must be remapped through `oldpaul_funcmap.tsv`.
