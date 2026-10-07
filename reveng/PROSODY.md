# Prosody

Per sentence, after the front end has built words, phones, syllables and breaks, prosody predicts phone
durations and syllable F0 contours with CART trees, applies VTML rate/pause/break spans, and resolves pauses
and silences. Port: `src/prosody.rs` (+ generated `src/prosody_tables.rs`, tree reader `src/tree3.rs`); tests
`tests/prosody_parity.rs`. Addresses are `vt_eng.dll` 3.11.7.1 VAs (image base 0x10000000). Tags: **[D]**
decompile/disassembly, **[C]** confirmed by capture + parity test, **[I]** inferred. The 2009 engines are in
[Builds](#builds) and [OLD_PAUL.md](OLD_PAUL.md).

The predicted durations and F0 never reach the waveform [C]. They are compared with unit-DB bytes in the target
cost only; the waveform stage receives rate percentages (API + VTML) and applies them uniformly. The CART models
steer unit selection, not signal processing.

| Output | Location | Consumer |
|---|---|---|
| duration per phone (16 kHz samples) | `s16 W+0x10+2k` | target feature (dur_code) |
| tone per syllable (0-4) | `s8 syl+3` | F0 tree input only |
| 12-point F0 contour per syllable (Hz) | `s16 syl+4..+0x1b` | target feature (f0_a, f0_b) |
| per-word pitch/speed/volume % | TIB arrays (3.4) | unit records |
| per-word pause (ms), forced break level, kind-7 class | TIB arrays | pause gaps, breaks, target cost |
| silence length per gap | `SS+8` scratch, then unit record `+0xc` | waveform |
| target feature `{dur_code, f0_a, f0_b}` | stack, to `us_TargetCost` | unit selection |

Per-sentence driver `FUN_10026630` [D]:

```
fe_NextSentence 0x10022dc0          normalize words, seed per-word rate arrays, pro_ApplyTagSpansToWords
pro_ResolveSentencePause 0x10026750 (via FUN_10026750)
fe_BuildWordPhoneRecords 0x10016c90 phone codes, marks, M flags, domain byte, sentence type
fe_AssignPhraseBreaks 0x100130e0    FUN_10012c70 syllabify, break codes, FUN_10012df0 phrase table
fe_BuildPhoneContexts 0x10017510    FUN_10017100 allophones, 7-byte contexts
pro_PredictDurations 0x10013380
pro_PredictPitch 0x100138c0         -> pro_SmoothF0AcrossSyllables per phrase
per phrase: us_SelectUnitsForPhrase -> us_GetCandidates builds the target feature;
            wav_BuildUnitRecords -> pro_PauseSamples per word gap
```

## 1. Functions

| VA | Name | Role |
|---|---|---|
| 0x10001050 | `pro_LoadProsodyTrees` `(void*, spkdb*)` | loads 17 trees into the speaker +0x494 area (error 8) |
| 0x10001900 / 0x10001af0 | `pro_LoadTree3` / `pro_LoadTree3Vec` | patch `.tree2` to `.tree3`, read, parse (scalar / vector `*bf`) |
| 0x100016e0 | `pro_ParseTree3` | node + value tables, set-size check |
| 0x100015f0 | `pro_TreeFindLeaf` `(tree*, short* fv)` | walk (4.1) |
| 0x100015c0 | `pro_ShortInSet` | linear membership |
| 0x10001670 / 0x100016a0 | `pro_TreePredictScalar` / `pro_TreePredictVector` | `values[leaf]` / copy `dim` shorts at `values[leaf*dim]` |
| 0x100014f0 | `pro_CopyMaybeByteswap` | memcpy, or swap if `DAT_1007d6dc` (big-endian host) |
| 0x10013380 / 0x100135d0 | `pro_PredictDurations` / `pro_DurationFeatures` `(short fv[9], SS*, word, syl, phone)` | 5 |
| 0x100138c0 / 0x10013a20 | `pro_PredictPitch` / `pro_PitchFeatures` `(short fv[11], SS*, word, syl)` | 6 |
| 0x100137c0 | `pro_SmoothF0AcrossSyllables` `(phrase, tib*)` | 6.3 |
| 0x10013790 | `pro_RoundedMean` `(short*, short n)` | `(n + 2*sum) / (2n)` |
| 0x10022970 | `pro_ApplyTagSpansToWords` | 7.3 |
| 0x10026750 | `pro_ResolveSentencePause` | 7.4 |
| 0x1002e890 | `pro_PauseSamples` `(tib*, short gap)` to `SS+8` | 7.5 |
| 0x10022fd0 | `pro_QuantizeDurationDiv50` `u8(short)` | 8 |
| 0x10022850 | `fe_ResolveSentencePSV` | 7.1 |
| 0x10012c70 | syllabify words `(SS*)` | writes syllable records and `W+0x36a` (2.3) |
| 0x10013c00 / 0x10013f30 / 0x10013da0 | fill syllables of a chunk / nucleus-onset-coda split / legal-onset test | 2.3 |
| 0x10012df0 | build phrase table `(SS*)` | 2.1 |

## 2. Data structures

### 2.1 Sentence structure `SS = *(TIB+0x4c)`

| Offset | Type | Meaning |
|---|---|---|
| +0x00 / +0x02 / +0x04 | s16 | n_phrases / n_words / total syllables (`FUN_10012c70`) |
| +0x08 | s32 | scratch: pause length in samples, written by `pro_PauseSamples` (the harness `pause` field is this scratch, stale at sentence time) |
| +0x0e + 16g | s16 | words in phrase g |
| +0x10 + 16g | s16 | syllables in phrase g (sum of `W+0x94`) |
| +0x14 + 16g | ptr | first word record of phrase g |
| +0x18 + 16g | ptr | first syllable of phrase g (`== firstword+0x08`) |
| +0x64c + i*0x3c0 | | word record i (2.2) |
| +0x17d4c + j*0x1e | | syllable pool, contiguous over the sentence |
| +0x47704 | s16 | current phrase counter |
| +0x4770a | u8 | sentence type 5/6/7 |
| +0x4770c | ptr | per-word pause array (TIB+0x121a60) |
| +0x47710 / +0x47714 / +0x47718 | ptr | per-word speed / volume / pitch arrays |
| +0x4771c | ptr | per-word normalizer pause array |

### 2.2 Word record `W = SS + 0x64c + i*0x3c0` (prosody view)

| Offset | Type | Meaning | Writer |
|---|---|---|---|
| +0x00 / +0x04 | s32 | start / end char offset (tag-stripped text while spans apply, original input afterwards) | `fe_NextSentence` |
| +0x08 | ptr | first syllable (in the SS pool) | `FUN_10012c70` |
| +0x0c | ptr | word target list (6-byte entries, 8) | `us_BuildTargets` |
| +0x10 + 2k | s16 | predicted duration of phone k | `pro_PredictDurations` |
| +0x92 | s16 | word target count | `us_BuildTargets` |
| +0x94 / +0x95 | u8 | n_syllables / n_phones | `FUN_10012c70` / `fe_BuildWordPhoneRecords` |
| +0x96 + 7k | 7*u8 | phone context | `fe_BuildPhoneContexts` |
| +0x25d / +0x25e + k | u8 | stress-distance counters | `fe_BuildPhoneContexts` |
| +0x29e + k | u8 | `M` flag | `fe_BuildWordPhoneRecords` |
| +0x2df | u8 | 0x0c if the normalizer set a pause override | `fe_BuildWordPhoneRecords` |
| +0x2e0 / +0x2e4 | ptr | word text / lexical phone string (codes interleaved with `d`/`c` marks) | same |
| +0x2e8 + k | u8 | phone code | same, `FUN_10017100` (allophones) |
| +0x329 + k | char | mark `'0'`, `'1'` (lexical `d` after it), `'2'` (lexical `c`) | same |
| **+0x36a + k** | s8 | **syllable position: 1 onset, 2 nucleus, 3 coda** | `FUN_10013c00` |
| +0x3b8 | ptr | normalizer word record +0x20 | `fe_BuildWordPhoneRecords` |
| +0x3bc | u8 | minor-break flag | `fe_AssignPhraseBreaks` |
| +0x3bd | char | break after the word: `]` none, `\` minor, `[` phrase, `Z` sentence, `^` question, `` ` `` also final, `a` special | `fe_AssignPhraseBreaks` |

The previous word's break is `W-3` read at `+0x649`.

### 2.3 Syllable record (0x1e bytes)

| Off | Type | Meaning (`FUN_10013c00`) | Domain |
|---|---|---|---|
| +0 | s8 | b0: lexical stress of the nucleus (`DAT_1007baa8[code]`), 0 without nucleus | 0-2 |
| +1 | s8 | b1: shape: 0 V, 1 CV, 2 VC, 3 CVC, 4 no nucleus | 0-4 |
| +2 | s8 | b2: position inside its chunk: 1 first, 2 middle, 3 last (single-syllable chunk = 1) | 1-3 |
| +3 | s8 | tone (`pro_PredictPitch`) | 0-4 |
| +4..+0x1b | 12*s16 | F0 contour, Hz | 62-216 |
| +0x1c / +0x1d | u8 | first phone index in the word / phone count | |

**Syllabification** (`FUN_10012c70`, per word) [D]:

1. Split the phones into chunks: a chunk ends after phone j when `mark[j] != '0'` or j is last.
2. Per chunk, `FUN_10013c00(phone_idx[], stress[], n, sylrec, codes, poscodes)`:
   - `FUN_10013f30`: every phone whose base index is a vowel (`PHONE_PROPS.isVowel`) is a nucleus (max 0x41). For
     nucleus k the onset grows left from `nucleus-1` while `FUN_10013da0(from, nucleus-1)` accepts the cluster (and,
     for k >= 2, stays right of the previous nucleus). Unassigned phones after a nucleus up to the next assigned
     phone become its coda. The first syllable's start is forced to phone 1. (Dead code: if the last syllable's
     end != chunk length the engine writes the length into the first syllable's start, `[EDI+0x11c]`; the coda
     pass always extends the last syllable.)
   - `FUN_10013da0` legal onset: 1 phone = any consonant except `NG`; 2-3 phones = the names joined with `" "`
     must equal an entry of the 81-string list at 0x10079a4c: `B`, `B L`, `B R`, `B Y`, `CH`, `D`, `D R`, `D W`,
     `D Y`, `DH`, `F`, `F L`, `F R`, `F Y`, `G`, `G L`, `G R`, `G W`, `G Y`, `HH` (x3), `JH`, `K`, `K L`, `K R`,
     `K W`, `K Y`, `L`, `M`, `M Y`, `N`, `N Y`, `P`, `P L`, `P R`, `P Y`, `P W`, `R`, `S`, `S F`, `S FR`, `S K`,
     `S KL`, `S KR`, `S KW`, `S KY`, `S L`, `S M`, `S MY`, `S N`, `S P`, `S PL`, `S PR`, `S PY`, `S T`, `S TR`,
     `S TY`, `S V`, `S W`, `S Y`, `SH`, `SH ` (x5), `T`, `T R`, `T W`, `T Y`, `TH`, `TH ` (x3), `V`, `V Y`, `W`,
     `W Y`, `Y`, `Z`. **Engine data bug:** the three-consonant entries lack the inner space and `SH `/`TH `
     carry a stray space, so no word-internal three-consonant onset (nor `SH R`/`TH R`) is ever legal: `S T R`
     splits as `S | T R`. Word-initial clusters are unaffected (start forced to 1).
   - Per sub-syllable: pos codes onset 1, nucleus 2, coda 3 into `W+0x36a+k`. No nucleus: one syllable, all pos
     codes 1, `b0=0, b1=4, b2=1`.
3. `W+0x94 += count`; `SS+4 += W+0x94`.

**Allophone pass** `FUN_10017100` (front end; changes the codes the trees see) [D]: rewrites intervocalic,
post-`S` and word-final stops into codes 0x46-0x50 with base indices (`PHONE_INDEX`) 0x46 K, 0x47 P, 0x48-0x4d
T/D group (0x49 D, others T), 0x4e/0x4f D, 0x50 T; 0x51-0x53 are an AO stress triple via `DAT_1007be98`
(AO0/1/2 before R in some contexts, codes 81/82/83). Names are [I]: 0x4D/0x4E flap T/D, 0x46/0x47 unaspirated
K/P after S, 0x48 unreleased T, 0x4B T before a nasal, 0x4F/0x50 D/T in the function-word list at 0x1007beec.

### 2.4 TIB fields

| Offset | Type | Meaning |
|---|---|---|
| +0x00 / +0x04 | s32 | length of the tag-stripped text / offset of the current sentence in it |
| +0x4c | ptr | SS |
| +0x1210d7..d9 | u8 | per-call pitch/speed/volume "set" flags (`FUN_100286c0`) |
| +0x1210dc / e0 / e4 | s32 | per-call pitch / speed / volume |
| +0x1210e8 / ec / f0 | s32 | effective pitch / speed / volume (clamped) |
| +0x1210f4 / f8 / fc | s32 | sentence base speed / volume / pitch |
| +0x121100 + 4i | s32[200] | per-word pitch % |
| +0x121420 + 4i | s32[200] | per-word speed % |
| +0x121740 + 4i | s32[200] | per-word volume % |
| +0x121a60 + 4i | s32[200] | pause before word i, ms (i=0: sentence pause); -1 unset |
| +0x121d80 + 4i | s32[200] | forced break level after word i (0-3); -1 none |
| +0x1220a0 + 4i | s32[200] | kind-7 word class (0-9); -1 none |
| +0x1223c0 + 16k | `{s32 count; s32 cursor; s32* pos; s32* val}` | VTML span list k: 0 pitch, 1 speed, 2 volume, 3 pause, 4 break, 5, 6, 7 |
| +0x122440 | s32 | trailing pause (ms) after the last word of the text; -1 none |
| +0x122850 | u8 | continuation flag |
| +0x1312bc | ptr | normalizer word array (`+4` = sentence length in stripped text) |
| +0x1312c4 | s32 | TIB pause override (reset to -1 by `fe_ResolveSentencePSV`) |
| +0x1312c8 | s32 | per-call sentence pause (`FUN_100286c0`) |

Speaker/channel struct (0x4d18 B): `+0x4cf0` speed, `+0x4cf4` pitch, `+0x4cf8` volume, `+0x4cfc` emphasis,
`+0x4d00` sentence pause ms (687), `+0x4d04` comma pause ms (200).

## 3. Trees

### 3.1 File format (`.tree3`) [D][C]

The loader formats `"%sduration/capp.tree2"` and overwrites the second-to-last character with `'3'`. Little-endian.

```
u16 n_nodes
u8  dim                 values per leaf (1 scalar, 12 F0 contour)
u32 n_set_total         sum of node set sizes; the loader rejects a mismatch
node[n_nodes]:          (in memory 16 bytes: set*, thresh, yes, no, n_set, feat, kind)
    u8  feat            index into the caller's s16 feature vector (signed char)
    u8  kind            'D' = set membership; anything else = numeric
    s16 thresh
    u8  n_set
    s16 set[n_set]
    s16 yes             taken if feat in set / feat <= thresh
    s16 no
s16 values[(n_nodes+1)*dim]
```

Walk: start at node 0 (a 0-node tree returns leaf 0); child >= 0 is a node, child < 0 is leaf `-child-1`.
Scalar predict = `values[leaf]`; vector predict copies `dim` shorts at `values[leaf*dim]`. Every file is a full
binary tree (leaves = nodes + 1). The same format serves `engbi`, `sbd`, `poly`, Spanish `spabi`; `atmt.tree3` is 27
concatenated trees.

### 3.2 Tree slots (speaker DB struct, 0x1c-byte objects)

| Slot | File | Nodes | Dim | Features | Leaf range |
|---|---|---:|---:|---|---|
| +0xb4 | duration/vshort | 812 | 1 | 0-7 | 457-4044 |
| +0xd0 | duration/vlong | 369 | 1 | 0-7 | 477-5287 |
| +0xec | duration/vdi | 433 | 1 | 0-7 | 479-5887 |
| +0x108 | duration/vsch | 101 | 1 | 1-7 | 480-3361 |
| +0x124 | duration/cstop | 717 | 1 | 0-2, 4-8 | 446-2877 |
| +0x140 | duration/cfri | 248 | 1 | 0-2, 4-8 | 446-2851 |
| +0x15c | duration/caff | 48 | 1 | 0-2, 4-7 | 446-2819 |
| +0x178 | duration/cnas | 352 | 1 | 0-2, 4-8 | 477-4510 |
| +0x194 | duration/capp | 315 | 1 | 0-2, 4-8 | 477-3256 |
| +0x1b0 / +0x220 | pitch/nbt, nbf | 167, 263 | 1, 12 | 0-10, 0-11 | 0-1, 62-162 Hz |
| +0x1cc / +0x23c | pitch/bt, bf | 22, 18 | 1, 12 | | 0-4, 106-182 Hz |
| +0x204 / +0x274 | pitch/qbt, qbf | 7, 9 | 1, 12 | | 1-4, 84-149 Hz |
| +0x1e8 / +0x258 | pitch/sbt, sbf | 26, 42 | 1, 12 | | 0-4, 80-216 Hz |

## 4. Durations: `pro_PredictDurations` (0x10013380) [D][C]

For each word, syllable, phone (running index k in the word):

```
fv = pro_DurationFeatures(word, syl, phone)
props = PHONE_PROPS[PHONE_INDEX[code]]
if props.isVowel == 1: tree = {1: vshort, 2: vlong, 3: vdi, 4: vsch}[props.vclass]   else dur = 1600
else: tree = {1: cstop, 2: cfri, 3: caff, 4: cnas, 5: cnas, 6: capp}[props.cclass]    else dur = 500
W.dur[k] = tree.values[leaf(fv)]
```

| fv | Value (9 x s16) |
|---|---|
| 0 | `PHONE_INDEX[code]` (base phone 1-39) |
| 1 | previous phone's base index; at the word's first phone **40** if word 0 or the previous break is `[`, else **42** |
| 2 | next phone's base index; at the word's last phone **40** if this word's break is in `[ Z ^ \``, else **42** |
| 3 | `PHONE_STRESS[code]` (0/1/2) |
| 4 / 5 | syllable b1 / b2 |
| 6 | 1 first syllable after a pause (as fv1), 3 last syllable before a `[Z^\`` break, else 2 |
| 7 | syllables in the word |
| 8 | `W+0x36a+k` (1 onset, 2 nucleus, 3 coda) |

"Last phone" = last syllable and last phone of that syllable. Units are 16 kHz samples (1600 = 100 ms; unit DB
`dur_code * 50` tracks unit length, r = 0.999). ER has `vclass 4` and `cclass 6`; the vowel test runs first, so ER
uses `vsch`.

## 5. F0: `pro_PredictPitch` (0x100138c0) [D][C]

**Tree choice per syllable.** Only the last syllable of a word uses a boundary pair, by the word's break: `^`
qbt/qbf, `Z` sbt/sbf, `[` bt/bf. Every other syllable (and breaks `\`, `]`, `` ` ``) uses nbt/nbf. (q question,
s sentence-final, b phrase boundary, nb non-boundary; t tone tree, f F0 tree.)

```
fv = pro_PitchFeatures(word, syl)
tone = (s8) tTree.values[leaf(fv)]          stored at syl+3
fv[11] = tone
syl.f0[0..12] = fTree.values[leaf(fv)*12 ..]
```

| fv | Value (12 x s16) |
|---|---|
| 0 | consonant class (`props[5]`, signed) of the syllable's first phone; 0 if a vowel (ER gives 6) |
| 1 | base index of the nucleus (first phone with pos code 2, else the syllable's last phone) |
| 2 | this syllable's b0 |
| 3 / 4 | previous / next syllable's b0 in the word; 3 at word start / end |
| 5 | this b2 |
| 6 / 7 | previous / next b2; 0 at word start, 4 at word end |
| 8 | 1 first syllable after a pause (word 0 or previous break `[`), 3 last before `[Z^\``, else 2 |
| 9 | this b1 |
| 10 | syllables in the word |
| 11 | tone (f trees only) |

**Smoothing** `pro_SmoothF0AcrossSyllables` (0x100137c0). Per phrase g, over its contiguous syllables (count
`SS+0x10+16g`, pointer `SS+0x18+16g`, so across word boundaries), for each adjacent pair (j, j+1), left to
right, in place:

```
B[0..24] = syl[j].f0 ++ syl[j+1].f0
for k in -3..=2:
    m = RoundedMean(B[k+9 .. k+16])            7-point window centred on B[k+12]
    if k < 0: syl[j].f0[12+k] = m  else: syl[j+1].f0[k] = m
RoundedMean(v, n) = (short)((n + 2*sum(v)) / (2n))    C integer division
```

A one-syllable phrase is skipped.

## 6. Rates, pauses, silences

### 6.1 Effective sentence values: `fe_ResolveSentencePSV` (0x10022850)

`pitch = set_p ? TIB+0x1210dc : ch+0x4cf4`, likewise speed (`+0x1210e0` / `+0x4cf0`) and volume (`+0x1210e4` /
`+0x4cf8`). Clamp pitch 50-200, speed 50-400, volume 0-500; store at `+0x1210e8/ec/f0` and as sentence bases
`+0x1210fc/f4/f8`; `TIB+0x1312c4 = -1`.

### 6.2 Seeding: `fe_NextSentence` (0x10022dc0) [C 73/73]

Per word: pitch/speed/volume arrays = the sentence bases; pause, break, kind-7 = -1.

### 6.3 Span application: `pro_ApplyTagSpansToWords` (0x10022970) [C 73/73]

Each VTML kind k has a list of `(pos, val)`: `pos` is a char offset in the tag-stripped text, `val` the value in
force from there (`val < 0` = restore the base). Clamps (`SPAN_MIN`/`SPAN_MAX`, 0x1007d690 / 0x1007d66c): pitch
50-200, speed 50-400, volume 0-500, pause 0-65535, break 0-3, k5 0-0, k6 0-9, k7 0-9. Word offsets are the
stripped-text offsets (before `fe_NextSentence` remaps them).

Kinds 0, 1, 2, 7 (level spans), only if both arrays are non-null:

```
wi = 0; filled = 0
for j in 0..count:
    while wi < n and word[wi].start < pos[j]: wi++
    if j == 0: filled = max(filled, wi)
    else: for w in filled..wi: x = val[j-1] < 0 ? arr[w] : val[j-1]
                               if k != 7 or x != -1: x = clamp(x); arr[w] = x
          filled = wi
    if j == count-1 and filled < n: filled = n
```

Values are absolute: `<vtml_pitch value="150">` sets 150 regardless of the API pitch.

Kinds 3 (pause) and 4 (break) use gap spans with a resumable cursor (`list.cursor`). The window for word i is
`(prev.end, start_i]` for pauses (`[0, start_0]` for i=0) and `[end_i, next.start]` for breaks (`[end_last,
cursor+sent_len]` for the last word). Every span position inside the window, from the cursor on, is applied in
order (pause values accumulate, clamped after each add; break values overwrite) and the cursor advances past it.
After the words, if `cursor + sent_len == total_len` (last sentence of the text) the pause spans after the last
word's end accumulate into `TIB+0x122440` (else -1).

### 6.4 Sentence pause: `pro_ResolveSentencePause` (0x10026750) [C 73/73]

```
p0 = pause[0]
if p0 < 0:
    if call_pause >= 0: p0 = cont ? 0 : call_pause
    elif tib_pause >= 0: p0 = tib_pause
    elif cont: p0 = 0
    else: p0 = ch+0x4d00                   (687 ms)
if p0 > 0xfffe: p0 = 0xffff
```

### 6.5 Pause gaps: `pro_PauseSamples` (0x1002e890) [C 735/735]

Called by `wav_BuildUnitRecords` before the first target of each word w (0 = sentence start); result in samples
at `SS+8`:

| Case | Samples |
|---|---|
| w = 0, pause[0] >= 0 | `pause[0] << 4`; if the continuation flag was set it is cleared, and when pause[0] > 0 word 0's start is stored as the sync offset at TIB+0x4777c/+0x47780 |
| w = 0, unset, flag set | 0 (flag cleared) |
| w = 0, unset | 11000 (687.5 ms) |
| w >= 1, pause[w] >= 0 | `pause[w] << 4` |
| w >= 1, previous break `Z` or `^` | 11000 |
| w >= 1, previous break `[` | `ch+0x4d04 << 4` (3200 with no channel) |
| otherwise | 0 |

### 6.6 Silence records (`wav_BuildUnitRecords`, 0x1002c220) [C 50/50 texts]

If the gap is > 0 a type-1 record is inserted with

```
len = (((D >> 1) + 10000) / D * samples + 50) / 100        D = speed % of word w
```

(pitch = channel pitch, dur = 100, volume = channel volume, `+0x28` = phrase's first word). After the phrase, if
`TIB+0x122440 > 0` and this is the sentence's last phrase, the trailing pause becomes one more silence:
`samples = ms << 4`, D = speed of the word of the preceding record [C 1/1].

### 6.7 Cross-sentence continuation state, `TIB+0x122850` (`cont_flag`) [D][C live-traced]

`FUN_10025fc0` (TIB allocation, once per request) seeds `TIB+0x122850 = 1` and `TIB+0x122440 = -1`. The
per-sentence driver never touches `cont_flag`; only `pro_PauseSamples` clears it (both `w = 0` rows), i.e. once
audio generation reaches the first sentence's word 0. So within a request `cont_flag` is true only until the first
sentence is synthesized. By 6.4 (`call_pause` and the VTML `tib_pause` are -1 in every capture), sentence 0's
leading gap is 0 and every later sentence's leading gap is the channel sentence pause (`ch+0x4d00`, 687 ms = 11000
samples), scaled by `dur_factor` of that sentence's **first** word's speed.

- The per-stage capture tool (`fedump` mode 7: front end + prosody, no audio) never clears the flag, so every
  sentence there sees `cont_flag == 1` and `word_pauses[0] == 0`; this was found by a live Frida trace of
  `pro_ResolveSentencePause` and `wav_BuildUnitRecords` on a 3-sentence text (`cont_flag` 1,0,0; `word_pauses[0]`
  0,687,687; `;` is itself a sentence boundary).
- `TIB+0x122440` (trailing pause, 6.6) is a different mechanism and stays -1 at every internal boundary in the
  corpus. Using it as the inter-sentence gap gives the right length only when speed is equal across the boundary
  (it would scale by the preceding sentence's last word). The port implements the `cont_flag` machine
  (`src/engine.rs` `resolve_word0_pause`, `Engine::synth_from_wa_traced`).
- Not modelled: `TIB+0x1312c4` as a VTML pause landing in a later sentence's `[0, start_0]` window (no corpus text
  needs it).
- Test `tests/engine_parity.rs::pause_boundary_edge_cases`, 5/5: `questions_multi` (`?`/`?`/`!`),
  `semicolon_multi`, `pause_at_boundary` (explicit VTML pause on a boundary takes the genuine-override path),
  `trailing_partial` (no terminal punctuation), `speed_at_boundary` (sentence 2 at 200% speed: its leading gap
  scales by `dur_factor(200)`).

## 7. Target feature (`us_GetCandidates` 0x10023350, head) [C 3269/3269]

Target entry (6 bytes, at `TIB+0xec628+6t` and in the word list `*(W+0x0c)`): `word, syl, phone (in word), index
(in the word's list), half (0/1/2), unvoiced`.

```
d(e) = W.dur[e.phone] (/2, truncating, if e.half)
dur_code = pro_QuantizeDurationDiv50(d(t))
list = word t.word's target list; first = first entry with syl == t.syl
before = through = total = 0
for e in list[first..] while e.syl == t.syl:
    if !e.unvoiced:
        if e.index < t.index: before += d(e)
        elif e.index == t.index: through = before + d(e)
        total += d(e)
if total: a = min(before*12/total, 11); b = max(through*12/total - 1, a)   else a = b = 0
f0_a = (u8) syl[t.syl].f0[a];  f0_b = (u8) syl[t.syl].f0[b]              (low byte of smoothed Hz)
```

An unvoiced target keeps `through = 0`, so `b = a`. `pro_QuantizeDurationDiv50`: `FILD` the s16, `/ 50.0f`
(0x1006d1a0), `+ 0.5f` (0x1006d178), `__ftol`, take AX: `< 0` gives 0, `>= 256` gives 255, else AL. x87 at 53-bit
precision, so identical to f64 math.

## 8. Tables (generated into `src/prosody_tables.rs`)

| VA | Rust | Content |
|---|---|---|
| 0x10079878 | `PHONE_PROPS[40]` | `{name[3], isVowel, vclass(1 short 2 long 3 diph 4 rhotic), height, backness, rounding, cclass(1 stop 2 fric 3 affr 4 nasal 5 lateral 6 approx), place(1 lab 2 alv 3 pal 4 labdent 5 dent 6 vel 7 glot), voicing(1 -, 2 +)}`; row 0 empty, rows 1-39 AA..ZH |
| 0x1007b6c0 | `PHONE_INDEX[128]` (i8) | code to base index; 0x01-0x45 CMU with stress, 0x46-0x53 allophones (K,P,T,D,T,T,T,T,D,D,T,AO,AO,AO) |
| 0x1007baa8 | `PHONE_STRESS[128]` | code to stress digit (0x52/0x53 give 1/2) |
| 0x1007d66c / 0x1007d690 | `SPAN_MAX` / `SPAN_MIN` | `[200,400,500,65535,3,0,9,9]` / `[50,50,0,0,0,0,0,0]` |
| 0x1006d1a0 / 0x1006d178 | `DUR_DIV` / `DUR_ROUND` | 50.0f / 0.5f |
| 0x10079194, 0x1007b3cc, 0x10079a4c | (doc only) | `"NG"`, `" "`, 81 legal-onset strings |

Base phone rows (isVowel vclass height back round | cclass place voice): `AA 1 2 2 2 1`, `AE 1 1 2 1 1`,
`AH 1 1 2 2 1`, `AO 1 2 2 2 2`, `AW 1 3 2 2 1`, `AY 1 3 2 2 1`, `B | 1 1 2`, `CH | 3 3 1`, `D | 1 2 2`,
`DH | 2 5 2`, `EH 1 1 2 1 1`, `ER 1 4 2 2 1 | 6`, `EY 1 3 2 1 1`, `F | 2 4 1`, `G | 1 6 2`, `HH | 2 7 1`,
`IH 1 1 1 1 1`, `IY 1 2 1 1 1`, `JH | 3 3 2`, `K | 1 6 1`, `L | 5 2 2`, `M | 4 1 2`, `N | 4 2 2`, `NG | 4 6 2`,
`OW 1 3 2 2 2`, `OY 1 3 2 2 2`, `P | 1 1 1`, `R | 6 2 2`, `S | 2 2 1`, `SH | 2 3 1`, `T | 1 2 1`, `TH | 2 5 1`,
`UH 1 1 1 2 2`, `UW 1 2 1 2 2`, `V | 2 4 2`, `W | 6 1 2`, `Y | 6 3 2`, `Z | 2 2 2`, `ZH | 2 3 2`. The table is
followed by the POS-to-break string `"][Z^Z[][[][]\"` (0x10079a30).

## Rust API (`vtpaul::prosody`)

`Trees::load(tree3_dir)`, `predict_durations`, `predict_pitch` (= `predict_pitch_raw` + `smooth_f0`),
`duration_features`, `pitch_features`, `target_feature(word, word_targets, t)`, `quantize_duration`,
`WordRates::seed`, `apply_tag_spans`, `resolve_sentence_pause`, `pause_samples`, `silence_length`. Inputs are
plain structs (`Sentence`, `Word`, `Syllable`, `TargetRef`, `SpanList`).

## Builds

| Build | Differences |
|---|---|
| 2014 English (this document) | tree3; sentence types 5/6/7; 7-byte contexts; span kinds 0..7 incl. kind 7 |
| Violeta 2013 | see [VIOLETA.md](VIOLETA.md) |
| Julie09 | tree3; 5 tag stacks (no kind 7), rate kinds 0..2 (`apply_tag_spans_era`); BRK_GRAVE is not a final break (`is_final_break(era, b)`); target-feature duration code uses the whole phone's duration and the F0 list the target's own half flag (`target_feature_era`); pause-sample channel clamp 0..3 -> 3 (`clamp_channel`); 5-byte contexts |
| Paul09 | as Julie09 on `.tree2` trees (`Trees::load_tree2`, 0x1c-byte tree structs, predictors take the root pointer); channel clamp 0..3 -> 1; emits `]` where 2014 emits `\` in some break positions |
| Violeta09 | channel clamp 0 -> 0; context byte 4 decimal-coded; own phone tables (`*_SPA09`); no kind-7 pass |

`Sentence::era` defaults to `V2014`. `clamp_channel` is implemented but not yet called by the engine. The three
voice-variant steps of the 2009 driver only write TextToPreprocessInfoFile dumps and are not ported. See
[OLD_PAUL.md](OLD_PAUL.md).

## Verification

`cargo test --release --test prosody_parity` (2026-09-23), 42 corpus texts + 7 VTML/pause edge texts + 2
non-default-parameter runs (pitch 150 speed 250 volume 300 pause 400 comma 90; pitch 60 speed 55 volume 80 pause
1200 comma 500); everything matched on the first run:

| Check | Result |
|---|---|
| phone durations | 2713 / 2713 |
| syllable tones | 1047 / 1047 |
| raw F0 contours / smoothed F0 contours | 1047 / 1047 each |
| target features `{dur_code, f0_a, f0_b}` | 3269 / 3269 |
| per-word rate seeding | 73 / 73 sentences |
| span application (6 arrays + trailing pause + 8 cursors) | 73 / 73 |
| sentence pause resolution | 73 / 73 |
| pause gaps (samples + continuation flag) | 735 / 735 |
| silence record lengths | 50 / 50 texts |
| trailing silence | 1 / 1 |

2009: `prosody_{julie09,paul09,viol09}_parity.rs` at 100% on all 11 record types (durations Julie09 100630, Paul09
100775 on tree2, Violeta09 455427; tones, raw and smoothed F0, target features, span seeding, sentence pauses,
pause gaps, silences).

## Open items

- Tone labels 0-4: mapping to accent types unknown; they only feed the F0 trees.
- VTML kinds 5 and 6 are clamped but never applied here; kind 7 marks words with class 0x0c (+1000 target cost
  on mismatch): `<vtml_emotion>` sets it (see [UNITSEL.md](UNITSEL.md)).
- Allophone code names 0x46-0x53 are inferred from rule contexts.
- The normalizer pause path `W+0x2df == 0x0c` (e.g. `-2` = 100 ms) feeds `TIB+0x121a64`; covered by the gap
  parity, the normalizer side belongs to the front end.

## History

- 2026-09-23: prosody stage ported bit-exact against the 2014 captures (durations, F0, spans, pauses, target features).
- 2026-09-23: `cont_flag` sequencing found by live tracing while wiring the end-to-end engine.
- 2026-10-05: 2009 prosody for Julie09, Paul09 and Violeta09 ported at 100%; tree2 predictions exact.
