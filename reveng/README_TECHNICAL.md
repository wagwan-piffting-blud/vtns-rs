# On-disk formats (2014 English engine)

Byte maps of the files `vt_eng.dll` reads from `C:\Program Files (x86)\NWSPaulNumber2\`
(`data-paul\M16` voice data, `data-common` language data). This is the "what is at offset N" reference;
derivations are in the stage docs ([LEXICON.md](LEXICON.md), [UNITSEL.md](UNITSEL.md),
[WAVEFORM.md](WAVEFORM.md), [PROSODY.md](PROSODY.md), [FRONTEND_TEXT.md](FRONTEND_TEXT.md)). Everything
below is confirmed at 100% on all 580,474 units of the four Paul banks, or by a bit-exact port.

All multi-byte fields are little-endian. Tags: `[D]` decompile/disassembly, `[E]`/`[C]` captures or a
whole-bank scan, `[P]` proven by the port.

## 1. Unit database

One file set per pool (`gen`, `num`, `etc`, `alp`) plus a shared index. Pools concatenate into one
global unit array in `dblist.idx` order.

### 1.1 `dblist.idx`

| Pool | Unit range | Tag, domain |
|---|---|---|
| `gen` | `[0, 440124)` | GEN, 0 |
| `num` | `[440124, 464632)` | NUM, 3 |
| `etc` | `[464632, 580355)` | ETC, 100 (`'d'`) |
| `alp` | `[580355, 580474)` | ETC, 100 |

Other tags in the format: DOMAIN=1, ABC=2, SYL=7, NUM_SPLIT=8, EXP=0x0c. `[D][E]`

### 1.2 `unit-<pool>.idx`

Size `45 + 40·N` bytes (`N` = unit count). `[C]`

Header (45 bytes):

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | `hdr_len` = 23 |
| 1 | 23 | `"ver.2013\0VoiceText-Eng\0"` |
| 24 | 2 | `n_files` = 1 |
| 26 | 2 | length of pool-name string |
| 28 | 10 | `"merged-<pool>"` |
| 38 | 1 | group byte |
| 39 | 4 | `N` |
| 43 | 2 | `S` = 19 (primary record size) |

Primary record (19 bytes), `us_ReadPrimaryRecord2013` (0x1001b0d0):

| Offset | Size | Field | DB entry | Meaning |
|---|---|---|---|---|
| 0 | 4 | `pcm_pos` | +0 | `.dat` byte offset; tiles `.dat` 0..EOF |
| 4 | 2 | `h1` | +0xc | first-half length, samples = `2·Σ upm[base .. base+pm1)` |
| 6 | 2 | `h2` | +0xe | second-half length, samples = `2·Σ upm[base+pm1-1 .. base+pm1+pm2-1)` |
| 8 | 2 | `coded_len` | +0x10 | `.dat` payload length, passed to the Shorten read |
| 10 | 4 | `upm_base` | +4 | `.upm` byte offset |
| 14 | 1 | `pm1` | +0x12 | first-half pitch-mark count |
| 15 | 1 | `pm2` | +0x13 | second-half pitch-mark count |
| 16 | 1 | `per0` | +8 | first pitch period (half-samples) |
| 17 | 1 | `per_mid` | +9 | shared boundary period |
| 18 | 1 | `per_last` | +0xa | last period |

Whole-unit decoded length = `h1 + h2 − 2·per_mid` samples = `2·Σ` of the period span
`[base, base+pm1+pm2-1)`. `[D][E][C]`

Then 21 bytes of feature columns per unit, stored as parallel column arrays: `file_idx`[1] +
`label`[7] + `dur_code`[1] + 3 × (`u16 spec` + `u8 featB` + `u8 featF`). `[D][E]`

### 1.3 `.dat`

Headerless mono Shorten stream per unit: `blocksize` init 0x100, `nmean` 4, no QLPC (`cmd` 7
unimplemented, `cmd` ≥ 9 fails the unit). Command jump table 0x10001e4c, `cmd` 0-8: 4=quit,
5=`ulong` blocksize, 6=`uvar(2)` bitshift. `[D][E]`

### 1.4 `.upm`

One byte per pitch period at 8 kHz resolution (half-samples; the engine doubles on read). Read by
`wav_ReadPitchMarks` (0x1002bbd0) via `wav_ReadDbFileLocked` (0x10025440), the critical-section-guarded
reader shared by every stage that reads disk. Views chosen in `wav_FillUnitRecord` (0x1002c120):

| Mode | Offset | Count | Use |
|---|---|---|---|
| 1 (left) | `upm_base` | `pm1` | half-phone, left |
| 2 (right) | `upm_base + pm1 − 1` | `pm2` | half-phone, right |
| 0 (whole) | `upm_base` | `pm1 + pm2 − 1` | whole unit |

The halves share one byte (the boundary period), so spans tile each `.upm` with `pm1+pm2−1` bytes.
`per0`/`per_mid`/`per_last` are `upm[first]`, `upm[boundary]`, `upm[last]` of the span, cached so the
waveform stage skips a `.upm` read; ×2 they are the OLA/PSOLA cross-fade lengths at unit edges. `[D][E][C]`

### 1.5 Shorten decoder state

`wav_ShortenDecode` (0x10001b30) keeps 4 block-mean dwords (`EBP-0x14..EBP-0x8`), zeroed at the start of
every unit (`wav_DecodeUnit` calls it fresh per unit). After each DIFF0-3/ZERO block:

```
acc = trunc(blocksize/2) + sum(unshifted residual-domain samples in the block)
m0 <- m1 <- m2 <- m3                         (FIFO shift)
m3 = trunc(acc / blocksize) << bitshift
coffset = trunc((2 + m0+m1+m2+m3) / 4)       (DIFF0 only, computed before the shift)
if bitshift > 0: coffset >>= (bitshift-1); coffset >>= 1
```

A last-mean-only model (`nmean=1`) is wrong on 19,272 of 580,474 units (3.3%); the first is `gen` unit 14
(`pcm_pos` 13164, 568 coded bytes). The 4-mean model matches all 580,474 units against a capture of the
decoder's output buffer (`ctx+0x113d14`, pre-filled with `0xCD`). `[D][E]`

### 1.6 `cepdist.tbl`

`u16 n = 1024`, then an `f32` lower triangle (row `i` holds `d(i, 0..i)`). Lookup:
`table[max(a,b)][min(a,b)]` on `spec & 0x3fff`. Loader `us_ReadCepDistTable` (0x1001ad20). `[D][E]`

### 1.7 `.his`

`us_LoadSelectHistory` (0x100125d0) reads `<root>data-paul/M16/mc_idx_tbl/<pool>.his`, where `<pool>` is
the `dblist.idx` file name without `.idx` (e.g. `unit-gen.his`).

### 1.8 `.dtt`

`us_VTDTTS_MakeInfo_Impl` (0x10022350) opens `<name>.bin.dtt` (`"wb"`) and `<name>.asc.dtt` (`"wt"`).
`us_DumpDttRecords` (0x1002ca10) writes one 0x34-byte binary record per selected unit; layout in
[UNITSEL.md](UNITSEL.md).

## 2. `dict-eng` lexicon files

`fe_LoadLexicons` (0x100114e0) opens the files; each lookup does three small reads through
`wav_ReadDbFileLocked`: 2 bytes of `hashcont`, 4 bytes of `hashidx`, `maxrec` bytes of payload. Only
`hashparams_*` is read whole (`fe_LoadHashParams`, 0x10011cb0).

| File | Size | Content |
|---|---:|---|
| `hashparams_emb` | 132,132 | Perfect-hash parameters, main lexicon |
| `hashidx_emb` | 914,368 | `u32 maxrec` + `u32 offset[228,591]` into `engttsdict_emb` |
| `engttsdict_emb` | | Main lexicon payloads (morph candidates) |
| `hashparams_tpp`, `hashidx_tpp`, `tppdict_emb` | | TPP dictionary, same scheme |
| `exceptdict` | 2,891 entries | Hyphenated/foreign multi-part pronunciations |
| `*.txt2` (9 files) | | +10-shifted sorted `key\|value` tables |

**Slot** (`dict_PerfectHashSlot`, 0x10012030): `h[0..7] = seed`; `dict_JenkinsChecksum` (0x10010c90,
`perfect.c checksum()`); `dict_JenkinsLookup2` (0x10010a50, `lookup2()`); the result indexes the
`hashparams_*` tables. `[D][E]`

**Keys.** Main lexicon `fe_EncodeLexKey` (0x1000fd70). TPP `dict_EncodeTppKey` (0x1003a610), only when the
selector byte is `0x45` (`'E'`), else the raw string. Compare `dict_StrnCmpU` (0x1001c280), unsigned
`strncmp` stopping at NUL.

**MRU cache.** When the 3rd argument of `fe_HashDictLookup` (0x10011820) is non-null (always, from the front
end), lookups go through a circular doubly-linked cache at `TIB+0x13b0`: `{u32 slot, char* key, char*
result, prev@+0xc, next@+0x10}`. A hit moves the node to front; a miss recycles the tail and caches `""`.
Results equal the uncached path.

**Payload decode.** `dict_DecodeLexPayload` (0x10003c50) produces a 0x554-byte morph candidate;
`dict_FillMorphFromPayload` (0x1000d450) copies it into 0x70-byte morph entries;
`dict_CopyMorphsToWordArray` (0x1000ea20) copies entries plus 4 flag bytes into the WA record at
`+0x66/+0x68/+0x6a/+0x6c`.

**TPP field.** `dict_GetTppField` (0x1003a7b0) finds a field starting with a given letter (the payload's
first char, or after a `" <letter>"` marker) and cuts at the next space (`FUN_10024b50`, delimiter at
0x1007b3cc). Checked over all 31,550 TPP payloads (`tpp_field_equivalence`), no discrepancy. `[E][P]`

**`exceptdict`.** Loader `fe_LoadExceptDict` (0x10003660); keys are digraph-packed. Lookup
`dict_ExceptDictFind` (0x10003b70), driven by `dict_MatchHyphenatedExcept` (0x10008dc0):

- `g` = hyphen-part count over the consumed tokens, from `dict_CountHyphenParts` (0x1000c9c0; counts
  letters, `'` and `-` only; any other char gives 0, "not a candidate").
- Tokens are appended `"-%s"`-joined until `g` reaches `min_parts`/`max_parts`; each count in range gets
  a lookup. If the word is marked `'X'` (`WA+0x24`, accented input) and the plain key misses, it retries
  with prefix `"h'%s"` (hence `h'expose`, `h'touche`, `h'resume`).
- Value = raw phone codes (1..0x45) with marks `c`/`d`, not a codebook payload. `dict_ApplyExceptPron`
  (0x1000ca50) pours them into morph entries, moving to the next part's entry at each `'d'` once the part's
  syllable budget (`FUN_1000ca30`) is used.

**`*.txt2`.** Loader `fe_LoadTxt2Table` (0x100033e0, via `FUN_10010300`). Every byte stored +10 mod 256.
Decoded: `dbf` header, CRLF lines of `key|value` (mode M) or `key` (mode L), NUL, count, `dbf` trailer.
Bisection search `FUN_100035d0`. Users: `sbdw_sort.txt2` (next-word category);
`abbrh/abbrt/abbrc_sort.txt2` (abbreviation classes, value 1 = exact case, 2 = any case);
`streeta/streetf/citya_sort.txt2` (address normalizer).

**Phone codebook.** `dict_InitPhoneCodebook` (0x1000ebc0) fills the table at 0x100fe900.

## 3. Prosody trees (`.tree3`)

The loader formats `"%sduration/capp.tree2"`-style paths and overwrites the second-to-last character
with `'3'`; files on disk are `.tree3`.

```
u16 n_nodes
u8  dim               values per leaf (1 scalar, 12 F0 contour)
u32 n_set_total       sum of node set sizes; the loader rejects a mismatch
node[n_nodes]:        (16 bytes in memory: set*, thresh, yes, no, n_set, feat, kind)
    u8  feat          index into the caller's s16 feature vector (signed char)
    u8  kind          'D' = set-membership test; else numeric threshold
    s16 thresh
    u8  n_set
    s16 set[n_set]
    s16 yes           taken if feat in set (kind 'D') or feat <= thresh
    s16 no
s16 values[(n_nodes+1)·dim]
```

The walk starts at node 0 (a 0-node tree returns leaf 0). `child >= 0` descends to that node; `child < 0`
is leaf `-child-1`. Scalar predict = `values[leaf]`; vector predict = `dim` shorts at
`values[leaf*dim ..]`. Every file is a full binary tree (`leaves = nodes+1`). `[D][C]`

The format is shared by every CART tree: front-end `engbi` (phrase break), `sbd`, `poly` (homographs),
Spanish `spabi`, and `atmt.tree3` (27 concatenated trees, one per allophone context class). Parser
`src/tree3.rs`. 17 trees load per speaker DB (`pro_LoadProsodyTrees`, 0x10001050, into `speaker+0x494`);
see [PROSODY.md](PROSODY.md). The 2009 Paul build uses a different on-disk `.tree2` that converts to the
same in-memory tree (`src/tree2.rs`, [OLD_PAUL.md](OLD_PAUL.md)).

## 4. User dictionary (`data-common\userdict\userdict_eng.csv`)

Lines: `"source","target","type"[,"legacy"]`. `type` `A` = alias (text substitution; the target is
normalized through a temporary WA at load), `P` = phones (raw pronunciation). `[CI]` in the target puts
the entry on the case-insensitive list. Keys are trimmed and capped at 49 bytes (`FUN_1005f2e0`).
Loader `fe_LoadUserDictCsv` (0x1005e330).

Index `FUN_1005f040`: exchange-sorted by key length then key, with per-length first/last bounds. Search
`FUN_1005e010` (case-sensitive `strncmp`) / `FUN_1005e1a0` (`strnicmp`): longest length first, bisect
within it, and the match must end on a token boundary (`FUN_1005f840`, re-tokenizes in mode 0x17, accepts
internal `.`, `'`, `-`). If any `A` entry exists, `fe_AnalyzeSentence` switches to the simplified
tag-mode tokenizer (`FUN_1003e070`). The installed file has one `P` entry ("miles an hour" `[CI]`), so
the switch does not fire by default.

## 5. Builds

| Build | Differences from the above |
|---|---|
| 2014 x64 voice DLLs | Same files; only float evaluation in selection differs ([UNITSEL.md](UNITSEL.md)) |
| Violeta 2013 | Spanish files; `cepdist.tbl` has `n = 256` plus `m = 1024` remap entries ([VIOLETA.md](VIOLETA.md)) |
| Paul09, Julie09 | Header `ver.2009`; 39 B/unit = 19-byte primary record + 20 column bytes: file index, flag (0/1), 5-byte label `[L1, C, R1, posbits, flags]`, duration code, 3 boundary sets. Paul09 trees are `.tree2` under `ttsdata\tree` and `dict-eng\{atmt,engbi,poly,sbd}.tree2` ([OLD_PAUL.md](OLD_PAUL.md)) |
| Violeta09 | Header accepts any `ver.` (ships `ver.2005`); same 39 B/unit; plain `cepdist.tbl` without remap |

Index header readers: 2014 `0x10019e80` (requires exactly `ver.2013`); Julie09 `FUN_1001b600`, Paul09
`FUN_1001b870` (both require `ver.2009`); Violeta09 `FUN_10004b60`. A file without `ver.` selects the
interleaved legacy record layout (`FUN_1001aec0`); no shipped bank uses it. The 2009 column reader
`FUN_1001b050` is identical in all three 2009 DLLs.

## 6. Cross-references

| Topic | Doc |
|---|---|
| Lattice and target-cost math over the unit database | [UNITSEL.md](UNITSEL.md) |
| Shorten decode, OLA/PSOLA, output formats | [WAVEFORM.md](WAVEFORM.md) |
| Duration/F0 feature vectors that walk the trees | [PROSODY.md](PROSODY.md) |
| WA/SS layouts the lexicon payloads are copied into | [FRONTEND_LING.md](FRONTEND_LING.md) |
| Normalizer chain, `.txt2` consumers | [FRONTEND_TEXT.md](FRONTEND_TEXT.md) |
| Lexicon formats in depth | [LEXICON.md](LEXICON.md) |

## History

- 2026-09: unit index, `.upm` and Shorten decoder confirmed on all 580,474 units; an outside review's
  "halves share one byte" note fixed an off-by-one in the `.upm` reader (`pm1+pm2` to `pm1+pm2-1`).
- 2026-09: `nmean` = 4 found by disassembly (`0x10001c14`-`0x10001c43`) and proven by a whole-bank A/B decode.
- 2026-10-05: 2009 index layouts and `.tree2` format added.
