# vt_eng.dll — On-Disk Format Reference

Byte-level reference for every file format the engine reads at `C:\Program Files
(x86)\NWSPaulNumber2\` (`data-paul\M16` voice data, `data-common` language data). Compiled from the
stage docs (`LEXICON.md`, `UNITSEL.md`, `WAVEFORM.md`, `PROSODY.md`, `FRONTEND_TEXT.md`); an
independent review verified the unit-index and Shorten-decoder claims against `vt_pau.dll` under
Wine, and every open point from that review is now resolved below and confirmed at 100% coverage
against all 580,474 units in all 4 Paul banks. Not a substitute for the stage docs — this is the
condensed byte-map for someone who needs "what's at offset N", not the derivation.

All multi-byte fields are little-endian unless stated otherwise. Evidence tags follow each doc's own
convention: `[D]` decompile/disassembly, `[E]`/`[C]` empirically confirmed against captures or a
whole-bank structural scan, `[P]` proven by a Rust port matching bit-exact.

## 1. Unit database

Located at `data-paul\M16\...`, one set of 4 files per pool (`gen`, `num`, `etc`, `alp`) plus a
shared index.

### 1.1 `dblist.idx`

Lists the pools; they are concatenated into one global unit array in file order. Paul:

| Pool | Unit range | Tag → domain |
|---|---|---|
| `gen` | `[0, 440124)` | GEN → 0 |
| `num` | `[440124, 464632)` | NUM → 3 |
| `etc` | `[464632, 580355)` | ETC → 100 (`'d'`) |
| `alp` | `[580355, 580474)` | ETC → 100 |

(Domain tags GEN=0, DOMAIN=1, ABC=2, NUM=3, SYL=7, NUM_SPLIT=8, EXP=0x0c also exist in the format;
Paul's `etc`/`alp` both use the generic ETC tag.) `[D][E]`

### 1.2 `unit-<pool>.idx`

Size = `45 + 40·N` bytes, `N` = unit count `[C]` (verified on all four Paul banks, and independently
by the outside review's own structural scanner).

**45-byte header:**

| Offset | Width | Field |
|---|---|---|
| 0 | 1 | `hdr_len` = 23 |
| 1 | 23 | `"ver.2013\0VoiceText-Eng\0"` |
| 24 | 2 | `n_files` = 1 |
| 26 | 2 | length of pool-name string |
| 28 | 10 | `"merged-<pool>"` |
| 38 | 1 | group byte |
| 39 | 4 | `N`, unit count |
| 43 | 2 | `S` = 19 (per-unit record size) |

**19-byte primary record, read by `us_ReadPrimaryRecord2013` (`0x1001b0d0`):**

| Offset | Width | Field | DB-entry offset | Meaning |
|---|---|---|---|---|
| 0 | 4 | `pcm_pos` | `+0` | `.dat` byte offset, spans tile `.dat` 0..EOF exactly |
| 4 | 2 | `h1` | `+0xc` | first-half length in **samples** = `2·Σ upm[base .. base+pm1)` |
| 6 | 2 | `h2` | `+0xe` | second-half length in **samples** = `2·Σ upm[base+pm1-1 .. base+pm1+pm2-1)` |
| 8 | 2 | `coded_len` | `+0x10` | `.dat` payload length, passed straight to the Shorten read |
| 10 | 4 | `upm_base` | `+4` | `.upm` byte offset |
| 14 | 1 | `pm1` | `+0x12` | first-half pitch-mark count |
| 15 | 1 | `pm2` | `+0x13` | second-half pitch-mark count |
| 16 | 1 | `per0` | `+8` | first pitch period of the record (half-samples; ×2 = samples) |
| 17 | 1 | `per_mid` | `+9` | shared/boundary pitch period |
| 18 | 1 | `per_last` | `+0xa` | last pitch period of the record |

Whole-unit decoded length = `h1 + h2 − 2·per_mid` samples; this equals `2·Σ` of the combined
pitch-period span `[base, base+pm1+pm2-1)`. **100% match on all 580,474 units, all 4 banks.**
`[D][E][C]`

**Then 21 bytes of feature columns per unit** (read as parallel column arrays, not per-record):
`file_idx`[1] + `label`[7] + `dur_code`[1] + 3 × (`u16 spec` + `u8 featB` + `u8 featF`). `[D][E]`

### 1.3 `.dat` (Shorten-coded PCM payload)

Headerless mono Shorten stream per unit: `blocksize` init 0x100, `nmean` fixed at **4** (not 1 —
see §1.5), no QLPC branch (`cmd` 7 unimplemented, ≥9 fails the unit). Command jump table at
`0x10001e4c`, `cmd` 0-8: 5=`ulong` blocksize, 6=`uvar(2)` bitshift, 4=quit. `[D][E]`

### 1.4 `.upm` (pitch marks)

One byte per pitch period, stored at **8 kHz resolution** (period length in half-samples; the
engine doubles every byte on read to get 16 kHz samples). Read via `wav_ReadPitchMarks`
(`0x1002bbd0`) → `wav_ReadDbFileLocked` (`0x10025440`, the one shared, critical-section-guarded file
reader used by every stage that touches disk — lexicon included). Three read views, selected by
mode in `wav_FillUnitRecord` (`0x1002c120`):

| Mode | Offset | Count | Use |
|---|---|---|---|
| 1 (left half) | `upm_base` | `pm1` | half-phone, left |
| 2 (right half) | `upm_base + pm1 − 1` | `pm2` | half-phone, right |
| 0 (whole) | `upm_base` | `pm1 + pm2 − 1` | whole unit |

**The two halves share one byte** — the boundary period — so spans tile every `.upm` file exactly
with `pm1+pm2−1`, not `pm1+pm2` (an earlier off-by-one in this project's own reader, fixed in
`src/bank.rs`, and in the matching C unit-bank exporter used during verification, after the outside
review's correction). Confirmed 100% on all four banks. `[D][E][C]`

The `per0`/`per_mid`/`per_last` primary-record bytes (§1.2) are exactly `upm[first]`,
`upm[boundary]`, `upm[last]` of this span, cached so the waveform stage doesn't need a `.upm` read
just to get the edge periods; they set the OLA/PSOLA cross-fade lengths (×2) at each unit edge. `[C]`

### 1.5 Shorten decoder detail — `nmean` and DIFF history

`wav_ShortenDecode` (`0x10001b30`) keeps **4** running block-mean dwords (`EBP-0x14..EBP-0x8`),
reset to 0 at the start of every unit (the history never carries across units — `wav_DecodeUnit`
calls the decoder fresh per unit). After each DIFF0-3/ZERO block:

```
acc = trunc(blocksize/2) + Σ(unshifted residual-domain samples in the block)
m0 ← m1 ← m2 ← m3           (3-entry FIFO shift)
m3 = trunc(acc / blocksize) << bitshift
coffset = trunc((2 + m0+m1+m2+m3) / 4)     // used by DIFF0 only, before the shift above
```

then `coffset >>= (bitshift-1); coffset >>= 1` if `bitshift > 0`. A last-mean-only (`nmean=1`)
model gives **wrong PCM on 19,272 of 580,474 Paul units (3.3%)** — confirmed by a dedicated
whole-bank A/B decode (not part of this repository); the earliest diverging `gen` unit is unit 14
(`pcm_pos` 13164, 568 bytes coded). The 4-mean model is bit-exact on all 580,474 units against a Frida capture of
`wav_ShortenDecode`'s own output buffer (`ctx+0x113d14`), pre-filled with a `0xCD` sentinel to catch
any length error. `[D][E]`

### 1.6 `cepdist.tbl`

`u16 n = 1024`, then an `f32` lower triangle: row `i` holds `d(i, 0..i)`. Lookup is
`table[max(a,b)][min(a,b)]` on `spec & 0x3fff`. Loader: `us_ReadCepDistTable` (`0x1001ad20`). `[D][E]`

### 1.7 `.his` (select history)

`us_LoadSelectHistory` (`0x100125d0`) reads `sprintf("%s%s.his", "<root>data-paul/M16/mc_idx_tbl/",
poolName)`, where `poolName` is the `dblist.idx` file name with `.idx` stripped — e.g.
`unit-gen.his`.

### 1.8 `.dtt` (debug target-trace dump)

`us_VTDTTS_MakeInfo_Impl` (`0x10022350`) opens `<name>.bin.dtt` (binary, mode `"wb"`) and
`<name>.asc.dtt` (ASCII, mode `"wt"`). `us_DumpDttRecords` (`0x1002ca10`) writes one 0x34-byte
binary record per selected unit. See `UNITSEL.md` §"Debug dump" for the full field layout with ASCII
labels.

## 2. `dict-eng` lexicon files

Loaded (opened, not slurped) by `fe_LoadLexicons` (`0x100114e0`); every lookup does 3 small reads
through `wav_ReadDbFileLocked`: 2 bytes of `hashcont`, 4 bytes of `hashidx`, `maxrec` bytes of the
dictionary payload. Only `hashparams_*` is read whole, at `fe_LoadHashParams` (`0x10011cb0`).

| File | Size | Content |
|---|---:|---|
| `hashparams_emb` | 132,132 B | Perfect-hash parameters for the main lexicon |
| `hashidx_emb` | 914,368 B | `u32 maxrec` + `u32 offset[228,591]` into `engttsdict_emb` |
| `engttsdict_emb` | — | Main lexicon payloads (morph candidates) |
| `hashparams_tpp`/`hashidx_tpp`/`tppdict_emb` | — | TPP (letter-to-sound-adjacent) dictionary, same perfect-hash scheme |
| `exceptdict` | 2,891 entries | Hyphenated/foreign multi-part pronunciations |
| `*.txt2` (9 files) | — | +10-shifted sorted `key\|value` text tables |

### 2.1 Perfect-hash slot computation — `dict_PerfectHashSlot` (`0x10012030`)

```
h[0..7] = seed
dict_JenkinsChecksum(key, n, h)     // 0x10010c90, perfect.c checksum()
val = dict_JenkinsLookup2(key, n, seed)   // 0x10010a50, perfect.c lookup2()
```
Then indexed into `hashparams_*`'s tables to get the final slot. `[D][E]`

### 2.2 Lookup key encoding

- Main lexicon: `fe_EncodeLexKey` (`0x1000fd70`).
- TPP: `dict_EncodeTppKey` (`0x1003a610`), applied only when the caller's selector byte is `0x45`
  (`'E'`) — otherwise the raw string is the key. Compare is `dict_StrnCmpU` (`0x1001c280`), an
  unsigned-byte strncmp stopping at NUL.

### 2.3 MRU cache

When `fe_HashDictLookup`'s (`0x10011820`) 3rd argument is non-null (always, from the front end), the
lookup routes through a circular doubly-linked MRU cache at `TIB+0x13b0`: `{u32 slot, char* key,
char* result, prev@+0xc, next@+0x10}`. A hit moves the node to front; a miss recycles the tail node
and caches `""`. Results are identical to the uncached path — this only saves file reads.

### 2.4 Lexicon payload decode

`dict_DecodeLexPayload` (`0x10003c50`) decodes the raw hashdict payload into a 0x554-byte morph
candidate. `dict_FillMorphFromPayload` (`0x1000d450`) copies that into 0x70-byte morph entries.
`dict_CopyMorphsToWordArray` (`0x1000ea20`) copies morph entries plus 4 flag bytes into the WA
record at `+0x66/+0x68/+0x6a/+0x6c`.

### 2.5 TPP payload

`dict_GetTppField` (`0x1003a7b0`) scans a TPP payload for a field starting with a given letter
(either the payload's own first char or after a `" <letter>"` marker), cutting at the next space
(`FUN_10024b50`, delimiter `" "` at `0x1007b3cc`). Field-scan behavior independently cross-checked
over **all 31,550 TPP payloads** (`tpp_field_equivalence` test) — no discrepancies. `[E][P]`

### 2.6 `exceptdict`

Loader `fe_LoadExceptDict` (`0x10003660`). Keys are digraph-packed (not obfuscated, just packed).
Lookup `dict_ExceptDictFind` (`0x10003b70`), driven by `dict_MatchHyphenatedExcept`
(`0x10008dc0`):
- `g` = hyphen-part count across the consumed tokens, via `dict_CountHyphenParts` (`0x1000c9c0`,
  counts letters/`'`/`-` only; any other char → 0, "not a candidate").
- Tokens are appended `"-%s"`-joined until the part count reaches `min_parts`/`max_parts`; each
  count in range gets a lookup attempt. If the word is marked `'X'` (`WA+0x24`, accented input) and
  the plain key misses, retries with prefix `"h'%s"` — hence accented loanwords are stored as
  `h'expose`, `h'touche`, `h'resume`.
- `val` = raw phone codes (1..0x45) with marks `c`/`d`, **not** a codebook payload. Poured into
  morph entries by `dict_ApplyExceptPron` (`0x1000ca50`), advancing to the next part's entry at each
  `'d'` once that part's syllable budget (`FUN_1000ca30`) is used.

### 2.7 `*.txt2` tables

Loader `fe_LoadTxt2Table` (`0x100033e0`, via `FUN_10010300`). Every byte stored **+10 mod 256**.
Decoded form: `dbf` header + CRLF lines of `key|value` (mode M) or `key` (mode L) + NUL + count +
`dbf` trailer. Searched by bisection (`FUN_100035d0`). Used for: `sbdw_sort.txt2` (next-word
category), `abbrh/abbrt/abbrc_sort.txt2` (abbreviation classes, value 1=exact-case 2=any-case),
`streeta/streetf/citya_sort.txt2` (address normalizer tables).

### 2.8 Phone codebook

`dict_InitPhoneCodebook` (`0x1000ebc0`) → table at `0x100fe900`.

## 3. Prosody trees (`.tree3`, née `.tree2`)

The loader formats `"%sduration/capp.tree2"`-style paths and overwrites the second-to-last
character with `'3'` — the `.tree2` spelling in the format string is a historical leftover; the
files on disk are `.tree3`. Little-endian.

```
u16 n_nodes
u8  dim              values per leaf (1 = scalar, 12 = F0 contour)
u32 n_set_total       Σ node set sizes; loader rejects on mismatch
node[n_nodes]:        (16 bytes in memory: set*, thresh, yes, no, n_set, feat, kind)
    u8  feat          index into caller's s16 feature vector (signed char)
    u8  kind          'D' = set membership test; else numeric threshold
    s16 thresh
    u8  n_set
    s16 set[n_set]
    s16 yes           branch taken if feat ∈ set (kind='D') or feat <= thresh
    s16 no
s16 values[(n_nodes+1)·dim]
```

Walk starts at node 0 (a 0-node tree returns leaf 0 directly); `child >= 0` → descend to that node
index, `child < 0` → leaf `-child-1`. Scalar predict = `values[leaf]`; vector predict = `dim` shorts
at `values[leaf*dim ..]`. Every file is a full binary tree (`leaves = nodes+1`). `[D][C]`

This format is shared by every CART tree in the engine, not just prosody: the front end's `engbi`
(phrase-break), `sbd` (sentence-boundary), `poly` (homograph disambiguation), Spanish `spabi`, and
`atmt.tree3` (27 concatenated trees, one per allophone context class). Parser: `src/tree3.rs`; a
standalone Python dump tool (not part of this repository) was used during verification.

17 trees load per speaker DB (`pro_LoadProsodyTrees`, `0x10001050`) into `speaker+0x494`; full
node/dim/feature table in `PROSODY.md` §4.2.

## 4. User dictionary (`data-common\userdict\userdict_eng.csv`)

CSV lines: `"source","target","type"[,"legacy"]`. `type` is `A` (alias, text substitution — the
target is normalized through a temporary WA at load time) or `P` (phones, raw pronunciation).
`[CI]` in the target puts the entry on the case-insensitive search list, otherwise case-sensitive.
Keys trimmed and capped at 49 bytes (`FUN_1005f2e0`). Loader `fe_LoadUserDictCsv` (`0x1005e330`).

Index `FUN_1005f040`: exchange-sorted by key length then key, with per-length first/last bounds.
Search `FUN_1005e010` (case-sensitive, `strncmp`) / `FUN_1005e1a0` (case-insensitive, `strnicmp`):
tries the longest length first, bisects within it, and requires the match to end on a token
boundary (`FUN_1005f840`, re-tokenizes in mode `0x17`, accepts internal `.`/`'`/`-`). Side effect:
if any alias (`A`) entries are present, `fe_AnalyzeSentence` switches to the simplified tag-mode
tokenizer (`FUN_1003e070`). The installed dictionary has one `P` entry ("miles an hour" `[CI]`), so
this switch does not currently fire.

## 5. Cross-references

| Topic | Detail lives in |
|---|---|
| Full unit-selection lattice/target-cost math over this unit database | [`UNITSEL.md`](UNITSEL.md) |
| Shorten decode → OLA/PSOLA → output-format pipeline consuming these files | [`WAVEFORM.md`](WAVEFORM.md) |
| Duration/F0 CART feature vectors that walk the `.tree3` files | [`PROSODY.md`](PROSODY.md) §5-6 |
| WA/SS struct layouts that the lexicon payloads get copied into | [`FRONTEND_LING.md`](FRONTEND_LING.md) |
| Normalizer chain, tokenizer, `.txt2` table consumers | [`FRONTEND_TEXT.md`](FRONTEND_TEXT.md) |
