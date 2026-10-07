# Unit database and unit selection

The unit database as selection sees it (`dblist.idx`, `unit-*.idx` columns, class tables, `cepdist.tbl`, the scalar
distance table, `.his`) and the selection algorithm: target building, the context back-off lattice, candidate
generation, contiguity scoring, target cost, join cost, Viterbi, beam pruning, backtrack; plus the `.dtt` info
dump. Input: per-phrase word/syllable records with predicted durations and F0 ([PROSODY.md](PROSODY.md)). Output:
one chosen unit per target, as 0x18-byte DB entries that [WAVEFORM.md](WAVEFORM.md) turns into unit records.

Port: `src/unitsel.rs`, `src/classes.rs`, generated `src/unitsel_tables.rs`; bit-exact (see Verification). The 2009
engines are in [Builds](#builds) and [OLD_PAUL.md](OLD_PAUL.md). Addresses are `vt_eng.dll` 3.11.7.1 VAs (image
base 0x10000000); Ghidra renames carry the `us_` prefix. Tags: **[D]** decompile/disassembly, **[E]** verified on the
Paul data or captures, **[P]** proven by the port reproducing the engine bit for bit, **[I]** inferred.

## 1. Pipeline

Per phrase, `FUN_1002c9b0` calls `us_SelectUnitsForPhrase(phrase, TIB, voiceDB)` (0x10024980):

```
final_type = (phrase == last phrase of sentence) ? SS.sentence_type : 5     -> TIB+0x1312de
N = us_BuildTargets(phrase)                  one target per phone with dur > 0, or two half-phone targets
if N == 0: return
us_GetCandidates(0)                          expand classes -> units, contiguity, target cost, keep <= 30
acc[0][j] = f32(cand[j].tc / 2.0)
for t in 1..N:                               us_ViterbiForward(1, N-1)
    us_GetCandidates(t)
    us_ViterbiJoinStep(t)                    acc[t][j] = min_k(join + acc[t-1][k]) + tc/2
    us_BeamPrune(t, 1)
us_Backtrack(0, N-1)                         path from argmin of acc[N-1]
for t: us_ReadPrimaryRecord(kept[t][path[t]]) -> TIB+0x46c93*4 + 0x18t   (0x18-byte DB entries)
if .his mode: count candidates (post-prune) and chosen units
```

## 2. Functions

| VA | Name | Role |
|---|---|---|
| 10024980 | `us_SelectUnitsForPhrase` `(short phrase, TIB*, VDB*)` | top level |
| 10024680 | `us_BuildTargets` `(phrase, TIB*, VDB*) -> n` | one target per phone with duration > 0; whole phone first, else two halves |
| 10024060 | `us_LookupWholePhone` `(t, TIB*, VDB*) -> 0/1` | lattice, sort/unique, limit to 30 classes / 10000 units, merge B-list; succeeds if > 9 units (domain 8: > 0) |
| 100242a0 | `us_LookupHalfPhones` | builds targets t (left, half 1) and t+1 (right, half 2) |
| 10018770 | `us_ContextBackoffLattice` `(label7*, t, TIB*, VDB*, short mode) -> bool` | position back-off x edge substitution x context/centre substitution; collects classes in list A |
| 10023f90 | `us_LookupClasses` `(key7*, t, TIB*, VDB*, mode) -> count` | whole (exact 5-byte key) or half (10-byte prefix back-off) by `target.half` |
| 10023dc0 | `us_LookupWholeClassExact` | exact class-key range; caps list A at 300 |
| 10023e70 | `us_LookupHalfClassBackoff` | half-key prefix 10 down; <= 10 classes per call |
| 10019450 | `us_FindClassRange` `(classIdx*, key5*, n, int* first) -> count` | binary search on sorted 5-byte keys, then widen |
| 1002df50 | `us_FindHalfOrderRange` | binary search in a half-phone ordering, prefix length `range.len`, then widen |
| 10019570 | `us_SearchHalfKey` | label to class key to half key to search |
| 10016ea0 | `us_LabelToClassKey` | `{mapL[L1], C, mapR[R1], posbits, flags&0x20}` |
| 10016ef0 | `us_HalfPhoneKey` | 10-byte half key |
| 10023a70 | `us_ClassKeyDistance` `(a, b, half) -> int` | weighted byte mismatch (`bf14`), plus half key (`bf28`) |
| 10023af0 / 10023c70 | `us_LimitClasses30x10000` / `us_LimitHalfClasses` | class ranking and limit |
| 10024010 / 1001bea0 | `us_SortUniqueInts` / `us_IntroSortInts` | sort ascending + unique (only the sorted result matters) |
| 10023060 | `us_SumClassUnits` | |
| 10023350 | `us_GetCandidates` `(t, TIB*, VDB*)` | feature, expansion, contiguity, cutoff, preferred fill, target cost, keep 30, preferred substitution |
| 100230a0 | `us_ScoreContiguity` | run lengths through neighbouring beams / class lists; sort |
| 100182e0 | `us_TargetCost` `(uid, TIB*, target*, feat3*, float w, VDB*) -> float10` | 8 |
| 10018c80 | `us_ViterbiJoinStep` | join cost + recursion |
| 100248b0 | `us_ViterbiForward` | loop of GetCandidates / JoinStep / BeamPrune |
| 10024510 | `us_BeamPrune` `(t, TIB*, int k)` | keep `acc <= mean + k*(mean-min)/4`, or a forward run |
| 10024900 / 10023010 | `us_Backtrack` / `us_ArgMin` | first minimum |
| 1001b5f0 / 1001b400 | `us_SortByScore` / `us_HeapSortByScore` | introsort on `score` (+4) with a bottom-up heapsort fallback; decides tie order |
| 10019350 / 10019310 | `us_CmpBytesN` / `us_CmpClassKey5` | unsigned compares |
| 1001aca0 / 1001a5a0 / 1001a860 | `us_LoadOrMakePhoneClass` / `us_MakePhoneClass` / (`us_MapUnitsToClass`) | `class.idx` or rebuild; `class_of[u]` to VDB+0x30 |
| 1001ac60 / 1002dea0 | `us_LoadOrMakeHalfPhoneClass` / `us_MakeHalfPhoneOrders` | `classhp.idx` or rebuild; two orderings (left, right) |
| 1002dce0 / 1002dd70 | `us_BuildHalfKeys` / `us_NRHeapSortHalfKeys` | 10-byte keys per class; Numerical Recipes `sort2` heapsort; decides order inside equal half keys |
| 10017010 | `us_IsUnvoicedCenter` | `ba44[ctx[2]] == 0` |
| 1001ad20 | `us_ReadCepDistTable` | `cepdist.tbl` |
| 1001ae70 / 1001ae20 | `us_MakeScalarDistTable` / `us_ScalarDist` | 256x256 byte-feature distance |
| 1001b200 to 1001b0d0 | `us_ReadPrimaryRecord` / `...2013` | lazy 19-byte record read |
| 100125d0 / 10012660 / 1001b240 / 1001b350 | `us_LoadSelectHistory` / `us_SaveSelectHistory` / `us_ReadHisFile` / `us_WriteHisFile` | `.his` I/O |
| 10022350 / 1002ca10 | `us_VTDTTS_MakeInfo_Impl` / `us_DumpDttRecords` | `.dtt` (12) |

## 3. Data structures

### 3.1 Target record (`TIB+0xec628 + 6t`) [D][P]

| Off | Field | Notes |
|---|---|---|
| +0 | word | absolute word index in the sentence structure |
| +1 | syllable | index in the word's syllable array (accumulating `syl.n_phones`, `+0x1d`) |
| +2 | phone | index within the word |
| +3 | idx | target number within the word (`word+0x92` before the increment; +1 for a right half); the harness calls it `syl_phone` |
| +4 | half | 0 whole, 1 left half, 2 right half |
| +5 | unvoiced | `ba44[ctx[2]] == 0` |

### 3.2 Per-target slot (`TIB+0xae894 + 0xfc t`) [D]

| Off | Type | Field |
|---|---|---|
| +0x00 | s16 | total classes in the list |
| +0x02 | s16 | size of the preferred B-list (first entries of the list) |
| +0x04 | s32[] | class list: B first, then A\B in ranked order (<= 60; entries >= 30 overlap the kept list, harmlessly) |
| +0x7c | s32[30] | kept unit ids (beam) |
| +0xf4 | s16 | n: expanded count, then kept count (<= 30), then post-prune count |

Beam arrays: `acc` f32 at `TIB+0x8212c + 0x78t`; backpointers s16 at `TIB+0x9f664 + 0x3ct`; join scratch f32[30] at
`TIB+0x9f5ec`; path s16 at `TIB+0xae0c4 + 2t`.

### 3.3 Candidate pool [D][P]

`TIB+0x477ac` is an array of 10,000 pointers to 10,000 consecutive 0x14-byte records at `TIB+0x513ec`, set up when
the TIB is created (`FUN_10025fc0`, once per request); records come from `malloc` (zero pages). Every access goes
through the pointer array and the sorts permute that array, so fields not rewritten by the current call keep
**stale values from earlier targets or phrases**. The port models pool and permutation exactly (the harness `pref`
field depends on it).

| Off | Type | Field |
|---|---|---|
| +0x00 | f32 | `tc` target cost |
| +0x04 | f32 | `score`: sort key (contiguity score, class distance, or domain-weighted TC) |
| +0x08 | s32 | unit id (a class id while ranking classes) |
| +0x0c / +0x0e / +0x10 | s16 | `fwd_run` / `run` / `wrun` (weighted run) |
| +0x12 | u8 | `pref`: unit's class is in the B-list |

### 3.4 Word record fields used (0x3c0-byte record) [D][P]

| Off | Field | Used by |
|---|---|---|
| +0x08 | ptr to syllables (0x1e bytes: F0 s16[12] at +4, n_phones at +0x1d) | BuildTargets, feature |
| +0x0c | ptr to the word's first target (written by BuildTargets) | GetCandidates |
| +0x10 + 2k | s16 predicted duration | feature, BuildTargets (`dur > 0`) |
| +0x92 / +0x94 / +0x95 | targets built / n_syl / n_phones | |
| +0x96 + 7k | 7-byte label `{L2, L1, C, R1, R2, posbits, flags}` | everywhere |
| +0x25d + k | two-digit edge code: tens gate the left back-off, ones the right | lattice |
| +0x29e + k | flag blocking centre substitution | lattice |
| +0x2df | domain: 0 general, 8 NUM_SPLIT, 0x0c EXP; **mutated**: reset to 0 by GetCandidates when no full-phrase run exists | lookup, TC, prune |

### 3.5 Voice-DB fields (`VDB` = speaker + 0) [D]

`+0x0c` cepdist rows, `+0x18` scalar-distance rows, `+0x28` per-file domain, `+0x30` class of unit, `+0x3c/+0x40/+0x44`
spec[k] (u16), `+0x48..` featB[k], `+0x54..` featF[k], `+0x60` dur_code, `+0x64` 7-byte labels, `+0x68` file index
per unit, `+0x88` class count, `+0x8c` u16 units per class, `+0x90` 5-byte class keys, `+0x94` per-class pointer
into the sorted unit list `+0x98`, `+0x9c`/`+0xa8` half-phone orderings `{int* classes, int n, u8 keylen}`.

## 4. Unit database files

Byte layout of `unit-*.idx` / `.dat` / `.upm`: [README_TECHNICAL.md](README_TECHNICAL.md) (independently
verified by an outside review, see [EXPERIMENTS.md](EXPERIMENTS.md)). Unit selection relies on:

- **Pools.** `dblist.idx` lists pools merged into one global array in dblist order: gen `[0, 440124)`, num
  `[440124, 464632)`, etc `[464632, 580355)`, alp `[580355, 580474)` [E]. Tag to domain: GEN 0, DOMAIN 1, ABC 2, NUM
  3, SYL 7, NUM_SPLIT 8, EXP 0x0c, anything else 100 (`'d'`); Paul's etc and alp are `ETC` = 100 [D].
- **EXP flag.** At load every unit of a domain-0x0c file gets label flag `|= 0x20` (end of `us_LoadUnitIndexes`,
  0x10019f40) [D]. Paul has none.
- **Primary record.** 19 bytes, read lazily for chosen units only (`us_ReadPrimaryRecord2013`, 0x1001b0d0). DB
  entry: `+0 pcm_pos`, `+4 pm_pos`, `+8..0xa` first/shared/last period (half-samples), `+0xc h1`, `+0xe h2`, `+0x10
  coded`, `+0x12 pm1`, `+0x13 pm2`, `+0x14 file` [D][E]; matches all 3,269 runtime reads.
- **`cepdist.tbl`.** `u16 n = 1024`, then f32 lower triangle, row i = `d(i, 0..i)`; lookup `d[max][min]` on `spec &
  0x3fff` [D][E].
- **Scalar distance** (`us_ScalarDist` 0x1001ae20), for 0 <= b <= a <= 255: `m = f32((a+b)/2.0)`; `m == 0 ? 0 :
  f32(f32(a-b)^2 / m)` [D][P].

## 5. Class index

Rebuilt at load because `class.idx` / `classhp.idx` are not installed [D][P]:

1. `key5(u) = us_LabelToClassKey(label[u]) = {mapL[L1] (b7ec), C (b788, identity), mapR[R1] (b850), posbits, flags & 0x20}`.
2. Classes are the distinct keys in ascending unsigned byte order; each lists its units in ascending id order.
3. **Half-phone orderings** for mode 1 (left) and 2 (right): the 10-byte half keys of all classes are sorted with
   Numerical Recipes `sort2` heapsort (`us_NRHeapSortHalfKeys`, 0x1002dd70), unsigned 10-byte compare, carrying
   class indices. Classes with equal half keys stay in heapsort order, which later feeds class ranking.

Half key (`us_HalfPhoneKey`; `b = k[3]`, `lpos = b>>3 & 7`, `rpos = b & 7`, `lb = (b&0x80) && lpos >= 2`, `rb =
(b>>6&1) && rpos >= 2`):

| byte | mode 1 (left half) | mode 2 (right half) |
|---|---|---|
| 0 | 0 | 1 |
| 1 | k[4] (EXP bit) | k[4] |
| 2 | k[1] (centre) | k[1] |
| 3 | b97c[k[0]] | b97c[k[2]] |
| 4 | lb | rb |
| 5 | b8b4[k[0]] | b918[k[2]] |
| 6 | lpos*10 + rb | rpos*10 + lb |
| 7 | k[0] | k[2] |
| 8 | rpos | lpos |
| 9 | b97c[k[2]] | b97c[k[0]] |

## 6. Target building

`us_BuildTargets` (0x10024680) [D][P], per word of the phrase: set `word.ntargets = 0`, `word.first_target = n`. For
each phone k: fill the target record; if `dur[k] > 0`: `r = LookupWholePhone(n)`, and if `r < 1` `r =
LookupHalfPhones(n)`; `word.ntargets += r`, `n += r`. Phones with non-positive duration get no target.

**`us_LookupWholePhone`** (0x10024060):
1. `idx = word.ntargets`, `half = 0`, clear lists A and B.
2. Domain 0x0c word: run the lattice on a copy of the label with `flags &= 0xdf`, mode 1; if A is empty return 0;
   then run it on the real label, mode 2. Otherwise run it on the label, mode 1.
3. A = `sort_unique(A)`, then `LimitClasses(30, 10000, A, label)`; B = `sort_unique(B)`.
4. Class list = B ++ (A \ B) in A's order. Return 1 if `units(list) > 9` (`> 0` when `domain == 8`), else 0.

**`us_LookupHalfPhones`** (0x100242a0), for h = 0, 1: for h = 1 target t+1 is a copy of t; `half = h+1`, `idx =
word.ntargets + h`; run the lattice on the label (mode 1; for a domain 0x0c word first on a copy with `flags &=
0x80` in mode 1, then the real label in mode 2); A = `sort_unique`, then `LimitHalfClasses(30, 10000, A, label,
half)`; B = `sort_unique`; class list = B ++ (A \ B). Always returns 2.

## 7. Context back-off lattice

`us_ContextBackoffLattice(L, t, mode)` (0x10018770). Mode 1 is the primary pass (may seed the B-list), mode 2 is
always kept, mode 0 is for recursion and substitutions [D][P]:

```
saved = |A|
b = L[5]; lb = b>>7; rb = b>>6&1; lpos = b>>3&7; rpos = b&7
left  = LEFT_POS_LISTS [lb*5 + lpos]        (c4dc)
right = RIGHT_POS_LISTS[rpos + rb*8]        (c504)
code  = word.edge_code[phone]; tens = code/10; ones = code%10
found = false
for i, v in left:
    f3 = (v == 0) ? 0 : lb;   aok = v in {0, 1}
    l6 = (aok == (lpos <= 1))
    if !(tens == 0 || l6 || target.half == 2): continue
    for j, w in right:
        f5 = (w == 0) ? 0 : rb;   bok = w in {0, 1}
        l7 = (bok == (rpos <= 1))
        if !(ones == 0 || l7 || target.half == 1): continue
        key = L
        if i == 0 && j == 0:
            r = lookup(key, mode)
            if mode == 1 && saved == 0 && r > 0: B = A[0 .. min(|A|, 30)]
        else:
            if i > 0 && lb && v > 2: key[0] = key[1] = c4a4[v]
            if j > 0 && rb && w > 2: key[3] = key[4] = c4a4[w]
            key[5] = ((f3*2 + f5)*8 + v)*8 + w        (u8 arithmetic)
            r = lookup(key, 0)
        if r != 0 || lb || rb:
            if c2c4[L[2]] == 0:
                c1 = c2c4[L[1]]; c3 = c2c4[L[3]]
                if c1 == 0:      if c3: key[3] = c3; r += lookup(key, 0)
                elif c3 == 0:    key[1] = c1; r += lookup(key, 0)
                else:            key[1] = c1; r1 = lookup(key); key[3] = c3; r2 = lookup(key)
                                 key = L; key[3] = c3; r3 = lookup(key); r += r1+r2+r3
            elif (rb || L[2] != 0x39 || (i8)bb70[L[3]] < 1) && word.mflag[phone] == 0:
                key[2] = c2c4[L[2]]; r += lookup(key, 0)
            if !found && r > 0 && l6 && l7: found = true
if mode != 2 && !found: |A| = saved; return false        (B is left as is)
if mode != 0 && c324[L[2]]: key = L; key[2] = c324[L[2]]; lattice(key, 0)
return true
```

`lookup(key, mode)` = `us_LookupClasses`:
- Whole target (`half == 0`): `us_LookupWholeClassExact` does an exact 5-byte class-key range search and appends the
  range; list A is capped at 300 classes.
- Half target, `us_LookupHalfClassBackoff`: (1) if `|A| > 289` return 0; (2) search the half ordering with prefix
  length 10, shortening by 1 until something matches (non-primary mode: give up below 6); (3) rank and limit the
  range to <= 10 classes / 10000 units (`LimitHalfClasses`); (4) while the result has <= 9 units and the prefix is
  >= 1, shorten the prefix (again giving up below 6 when `mode != 1`) and redo 2-3; (5) append.

## 8. Class ranking and limiting

`us_LimitClasses30x10000` / `us_LimitHalfClasses` [D][P]: if `|src| <= maxc` and `units(src) <= maxu` keep `src`.
Otherwise load pool records `{id = class, score = (f32)dist(key5(label), key5(class), half)}`, `us_SortByScore`, take
classes in order at most `maxc`, stopping after the first class that pushes the unit total above `maxu` (that class is
kept). `dist` = sum of `bf14[i]` over mismatching class-key bytes (weights 40, 1, 40, 10, 1000), plus for half
targets the sum of `bf28[i]` over mismatching half-key bytes (50000, 20000, 10000, 5000, 1000, 2000, 500, 100, 50, 20).

## 9. Candidate generation

`us_GetCandidates(t)` (0x10023350) [D][P]:

1. **Feature** `{dur_code, f0_a, f0_b}`: see [PROSODY.md](PROSODY.md) section 7 (`dur_code = clamp(trunc(f32(dur)/50.0
   + 0.5), 0, 255)`, `dur` halved for half targets; F0 indices from the voiced targets on the same syllable).
2. **Expansion.** Every unit of every class in the list, in list order and ascending id within a class, up to
   10,000 (`n`).
3. `us_ScoreContiguity(t)` sorts by `-(run + 100*wrun)`.
4. **Cutoff `count`.** If `cand[0].run == N` (a run covering the whole phrase): count = leading candidates with
   `run >= N`. Otherwise: a word domain of 0x0c is reset to 0 (**mutation**); if `n < 31` count = n; else count
   extends to the end of the tie group straddling position 30: scanning i = 1..n-1, whenever `score[i-1] < score[i]`
   record `sort_from = i` while `i < 30`; at the first such i >= 30 stop with count = i.
5. **Preferred fill** (only if count > 30): mark `pref` for the first `count` candidates whose class is in B and
   count them (`n_pref`). If `n_pref < 10` and B has >= 10 units: scan idx = count..n-1, stopping once `n_pref >= 10`
   and `score[idx]` differs from the score of the 10th preferred found; copy each B-list candidate's `{id, run,
   fwd_run, wrun}` into record `count` (`pref = 1`), incrementing count and `n_pref`. The copy does not move `tc` or
   `score`.
6. **Target cost** for i < count: `r = f32(f32(run)/2.0)`, raised to 1.0 if `r <= 1.0`; `w = f32(1.0 / (wrun + r))`;
   `tc = us_TargetCost(uid, t, feat, w)`.
7. **Keep.** count <= 30: keep = count. Otherwise for i in [sort_from, count) set `score = tc*5` (domain 100),
   `tc*10` (EXP unit, non-EXP word), or `tc`; sort that range, keep 30.
8. `kept[i] = cand[i].id`.
9. **Preferred substitution** (only if count > kept): p = preferred among kept; if `p < 5` and `p < n_pref`, e =
   kept - 1. For each preferred candidate beyond the kept ones: step e down past kept entries already preferred
   (`while e > 0 && cand[e].pref == 1`), set `kept[e] = uid`, p++, e--; stop when `e < 1` or `p >= 5`. The `tc`
   later used for slot e is still `cand[e].tc`, not the substituted unit's (engine quirk, reproduced).

## 10. Contiguity scoring

`us_ScoreContiguity(t)` (0x100230a0). Label flag 0x80 on unit u means "unit u+1 continues the recording"
[D][E][P]. Weights by half: `{2, 1, 1}`.

- **Backward**, k = t-1 down to 0: unless target k is a left half (same unit), move to u-1. Stop if u-1 < 0 or
  `label[u-1].flags & 0x80 == 0`, or if the unit is not in target k's kept (post-prune) beam. Otherwise `back++`,
  `wback += w[half_k]`.
- **Forward**, k = t+1 up: unless target k is a right half, require `label[u].flags & 0x80` and move to u+1. Stop if
  `class_of[u]` is not in target k's class list. Otherwise `fwd++`, `wfwd += w[half_k]`.
- `run = fwd + 1 + back`, `fwd_run = fwd`. With `wb = (i16)wback/2`, `wf = (i16)wfwd/2`: `wrun` = `wf+1+wb` if `run
  == N`; else `wf` if `back == t`; else `min(wb, wf)` if `fwd != N-t-1`; else `wb`.
- `score = -(f32)(run + 100*wrun)`, then `us_SortByScore`.

## 11. Target cost, join cost, beam

### 11.1 Target cost `us_TargetCost(uid, t, feat, w)` (0x100182e0) [D][P]

T is the target label, U the unit label, both `{L2, L1, C, R1, R2, pos, flags}`. The penalty is an integer:

| Term | Rule |
|---|---|
| Centre | `U.C != T.C`: 100 if either is 0x4B (TQ), else 1000 if `b6c0` differs, else 5 |
| Left context (not right halves) | `U.L1 != T.L1`: 1 if U's left edge is flagged (`pos&0x80` and `pos&0x38 >= 0x18`), else 5 same `b6c0`, else 10 same `b97c`, else 500 |
| Right context (not left halves) | `U.R1 != T.R1`: 100 if final_type = 5 and T.R1 = pad, else 1 if U's right edge is flagged (`pos&0x40` and `pos&7 >= 3`), else 100 if U.R1 = TQ, else 5 same `b6c0` (0 if T.R1 = 0x5B), else 10 same `b97c`, else 500 |
| Position | `c0d8[T.lpos*5 + U.lpos] + c13c[U.rpos + T.rpos*8]` |
| Stress, equal centre stress | if T.L1 is a vowel and T.C in {R (0x36), L (0x2B)}: `c23c[st(T.L1)*3 + st(U.L1)]`, +100 if T.L1 != U.L1; else if U.rpos = 0 and T.R1 is a vowel: `c260[st(T.R1)*3 + st(U.R1)]` |
| Stress, unequal centre stress | `c23c[st(T.C)*3 + st(U.C)]` |
| Domain | +10 if the unit's file domain is 100; +1000 if EXP unit and non-EXP word |

Continuous term with `sd(a, b)` = scalar distance and `dd = sd(feat.dur, U.dur_code)`:
- Voiced target: boundaries `(ka, kb)` = (0,1) left half, (1,2) right half, (0,2) otherwise; `s = f32(sd(f0_b, fF[kb]) +
  sd(f0_a, fF[ka]))`; if U.lpos <= 1 and U.rpos <= 1 `s = f32(s/2)`, else `s = f32(s/8)` and `dd = f32(dd/8)`; `cont =
  f32(s + dd)`.
- Unvoiced target: `pen /= 2` (C truncation) if U.R1 = T.R1; `cont = f32(dd/5)`.
- EXP unit on an EXP word: `cont = f32(cont/16)`.

Result `f32((f64)cont*w + (f32)pen)`, stored by the caller.

### 11.2 Join cost and Viterbi step `us_ViterbiJoinStep` (0x10018c80) [D][P]

Join type from unvoiced flags (cur / prev): voiced/voiced 0 (same word) or 4 (different words); voiced/unvoiced 1;
unvoiced/voiced 2; unvoiced/unvoiced 3. Boundaries compared: prev end (k = 2, or 1 if the current target is a right
half) against current start (k = 0, or 1 for a right half).

For each kept unit j of t, `tcj = f32(cand[j].tc / 2)`, and each kept unit p of t-1:
- **Zero join** if (u = p and cur is a right half) or (p = u-1 and `label[p].flags & 0x80`).
- **Otherwise** `join = f32(sd(fF_p, fF_u)*wF + sd(fB_p, fB_u)*wB + cep[p][u]*wC + 2.0)`, weights from `c288[type]`,
  plus a penalty:
  - Right-half target: 5 (0 if the same unit), +1000 if the centres differ.
  - Other targets: edge flags: prev right-edge != cur left-edge gives 1000; both set gives `bfd8[cur.lpos +
    prev.rpos*8]`; otherwise 0. `prev.C != cur.L1`: +1000 if `c384` marks either code, else +100 if `c2c4[prev.C]`.
    `prev.R1 != cur.C`: +1000 if `c384` marks either code, else +100 if `c2c4[cur.C]`. +10 if cur left-edge = 0,
    the targets are a vowel-to-`c3e4` or `c444`-to-vowel pair, and p != u-1.
  - Always add `c544[class_p*3 + class_u]`, plus 2000 if exactly one of the two units is EXP.
  - Divisor: 1 if cur is a right half or the edges are not both set; `(lpos == 2) + 1` if lpos < 3; 8 otherwise (kept
    across zero joins, harmlessly).
- `scratch[p] = f32(join/f32(div) + f32(pen) + acc[t-1][p] + tcj)`.

Finally `back[t][j] = argmin(scratch)` (first minimum), `acc[t][j] = scratch[back]`.

### 11.3 Beam pruning `us_BeamPrune(t, 1)` (0x10024510) [D][P]

Applied only if `word.domain == 0` and n > 10: sum `acc` with an f32 store after each add, take the strict minimum;
`mean = f32(sum / f32(n))`, `spread = f32((mean - min)/4)`, `thr = f32(1.0*spread + mean)`; keep j if `acc[j] <= thr`
or `cand[j].fwd_run != 0` (`cand[j]` is pool slot j, not necessarily the kept unit after a substitution); compact
acc, back and kept in place. The first target is never pruned; EXP words are never pruned.

**`us_Backtrack`**: `path[N-1] = argmin(acc[N-1])`, then `path[t-1] = back[t][path[t]]`.

## 12. Sorting, floating point

**`us_SortByScore`** (0x1001b5f0) introsort [D][P]: ranges of `hi - lo <= 16` use a stable insertion sort with strict
`>`. Larger ranges: median-of-three on lo/mid/hi (swap when `>`, pivot `a[mid]`); partition `do i++ while a[i] <
pivot`, `do j-- while a[j] > pivot`, swap while `j > i`; recurse on the smaller side through an explicit stack
(lo/hi arrays of 32); if one side exceeds 32x the other, heap-sort the larger side with `us_HeapSortByScore` (bottom-up
heap: descend to a leaf, then climb back). Comparisons are `FCOMP` on f32, "not greater" for `<=`.
`us_NRHeapSortHalfKeys` is exactly Numerical Recipes `sort2`. Integer sorts do not depend on stability.

**Floating point.** The 2014 DLL is VC6 x87 at 53-bit precision control: every intermediate is an IEEE double
operation, rounded to f32 where the compiler stores a `float` (`FSTP dword`). The port computes in f64 and rounds
with `as f32` at those points. `__ftol` truncates toward zero. The 64-bit voice DLLs (`vt_pau/vt_kat/vt_jul/
vt_jam.dll`, MSVC x64 SSE) evaluate in single precision; only two places change results: the target-cost tail
`FUN_18001bc30` = `f32(f32(cont*w) + pen)`, and the join step `FUN_18001c7a0`: `join = ((df*wf + cep*wc) + db*wb) +
2`, `scratch = (join/div + (tcj + acc)) + pen`, all f32 in that order. The port does both behind `Selector::x64`.

## 13. Domains, EXP words and `vtml_emotion`

Word domain `word+0x2df` is set by VTML span list 7. `<vtml_emotion ...>` marks every word inside it 0x0c [E]
(captures `dom_emotion*`); `vtml_partofsp`, `vtml_sayas` digits/characters, `vtml_sub` and spaced digits produced no
non-zero domain; domain 8 was not observed. An EXP word: the whole-phone lookup runs a first lattice pass without
the EXP bit and then a mode-2 pass; half-phone lookups keep only the contiguity bit; pruning is disabled;
`GetCandidates` resets the word to domain 0 as soon as one target lacks a full-phrase run (later targets see 0).
Paul has no EXP units, so the 1000/2000/x10 penalties never fire, but the lookup and prune differences do.

## 14. Unit-select history (`.his`)

`VT_SetUnitSelectHistoryMode_ENG(1)` sets `DAT_100a045d` before the first load. `us_LoadSelectHistory` (0x100125d0)
reads `sprintf("%s%s.his", speaker+0x694 = "<root>data-paul/M16/mc_idx_tbl/", poolName)`; the pool name is the dblist
file name with `.idx` stripped (0x10019f40, compare `".idx"` at 0x1007c5b0), e.g. `unit-gen.his`. Format: `u32 N`, then
`N * {u32 times_candidate, u32 times_chosen}`; a missing file gives zeros, a count mismatch fails. After every
phrase each unit in every target's post-prune beam gets +1 candidate and each chosen unit +1 chosen;
`us_SaveSelectHistory` writes the same layout on unload. No effect on selection. Port: `load_history` /
`save_history` / `Selector::history`.

## 15. VTDTTS info dump (`.dtt`)

`VTDTTS_MakeInfo_ENG` to `us_VTDTTS_MakeInfo_Impl` (0x10022350) opens `<name>.bin.dtt` (binary) and `<name>.asc.dtt`
(`"wt"`) at TIB+0x122854 / +0x122858. Binary header: `"VTDTTS BINARY\0"` (14 bytes), version u8 = 3, number of
`.dat` files u8, each `.dat` name NUL-terminated; the ASCII file mirrors it as `%s\n` / `%d\n` lines plus a blank
line. The engine sets TIB+0x122851 = 1 and synthesizes; `wav_BuildUnitRecords` calls `us_DumpDttRecords`
(0x1002ca10) for every unit record (0x34 bytes at TIB+0xeddf8) with length != 0. Fields (ASCII label in brackets):

- every record: `type` u8 [`TypeFlag : %d`].
- silence (type 1): `len` u32 [`Size : %d`], blank line.
- unit (type 2): phone name from the 5-byte table at 0x1007bc98 indexed by record +0x32, NUL-terminated
  [`PhoneString : %s`]; `file` u8 [`File Index`]; `pcm_pos` u32 [`PCM Pos`]; `len` u16 [`PCM Size`]; `coded` u16 [`PCM
  Coded Size`]; `mode` u8 [`Phone Mode`]; mode 2 only `h1 - ovl_l` u16 [`Shift Size`]; `ovl_l` u16 [`Pitch_first`];
  `ovl_r` u16 [`Pitch_last`]; `pm_pos` u32 [`PM Pos`]; `n_pm` u16 [`PM Size`]; `pitch%` u16 [`Pitch_rate`];
  `(dur%/2 + 10000)/dur%` u16 [`Duration_rate`]; `vol%` u16 [`Volume_rate`]; blank line.

This is a record-level script of the synthesis (a "pre-selected DTTS"), not a selection trace.

## 16. Tables (generated into `src/unitsel_tables.rs`)

**Phone-code tables** (byte-indexed, codes 0x00-0x5F; one row per code in the generated file, columns by address):
collapse `b6c0`, broad class `b97c`, vowel `b9e0`, stress `baa8`, voiced `ba44`, mapL `b7ec`, mapC `b788`, mapR
`b850`, hkL `b8b4`, hkR `b918`, `bb70` (i8; -1 for non-vowels, 0-3 vowels), ctxSub `c2c4`, ctrSub `c324`, `c384`,
`c3e4`, `c444`. Codes 0x01-0x45 are the CMU phones with stress digits (e.g. 0x01-0x03 = AA0-AA2); 0x5A is the pad;
0x5B-0x5F appear only in the class-map tables (origin unknown). Row examples: code 01 AA0: collapse 1, broad 1,
vowel 1, stress 0, voiced 1, mapL/C/R/hkL/hkR 01, bb70 3; code 15 D: collapse 9, broad 3, ctxSub 4E (DV); code 5B:
broad 5, mapL/mapR/hk 5A, mapC 5B. The remaining rows are in `src/unitsel_tables.rs`.

`c4a4` edge substitution (index = lattice list value): v 0..5 gives codes `00 00 00 61 5B 5A`.

`c0d8` (0x1007c0d8, 8x5 int32; row = target lpos, col = unit lpos):

| | 0 | 1 | 2 | 3 | 4 |
|---|---|---|---|---|---|
| 0 | 0 | 20 | 20 | 50 | 50 |
| 1 | 20 | 0 | 2 | 20 | 50 |
| 2 | 10 | 2 | 0 | 5 | 50 |
| 3 | 10 | 10 | 2 | 0 | 5 |
| 4 | 10 | 10 | 10 | 2 | 0 |
| 5 | 0 | 20 | 20 | 100 | 500 |
| 6 | 500 | 1000 | 500 | 20 | 0 |
| 7 | 5 | 100 | 500 | 500 | 1000 |

`c13c` (0x1007c13c, 8x8 int32; row = target rpos, col = unit rpos):

| | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
|---|---|---|---|---|---|---|---|---|
| 0 | 0 | 20 | 20 | 100 | 500 | 500 | 1000 | 500 |
| 1 | 20 | 0 | 5 | 100 | 500 | 500 | 1000 | 500 |
| 2 | 20 | 5 | 0 | 5 | 500 | 500 | 1000 | 500 |
| 3 | 100 | 100 | 100 | 0 | 20 | 50 | 1000 | 500 |
| 4 | 100 | 100 | 100 | 5 | 0 | 20 | 1000 | 500 |
| 5 | 100 | 100 | 100 | 50 | 20 | 0 | 1000 | 500 |
| 6 | 100 | 100 | 100 | 100 | 100 | 500 | 0 | 100 |
| 7 | 100 | 100 | 100 | 100 | 100 | 20 | 1000 | 0 |

`bfd8` join edge position (0x1007bfd8, 8x8; row = prev rpos, col = cur lpos):

| | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
|---|---|---|---|---|---|---|---|---|
| 0 | 0 | 20 | 500 | 1000 | 1000 | 1000 | 5000 | 1000 |
| 1 | 20 | 0 | 500 | 1000 | 1000 | 1000 | 5000 | 1000 |
| 2 | 500 | 500 | 0 | 50 | 500 | 1000 | 5000 | 1000 |
| 3 | 1000 | 1000 | 50 | 0 | 10 | 100 | 5000 | 1000 |
| 4 | 1000 | 1000 | 500 | 10 | 0 | 100 | 5000 | 1000 |
| 5 | 1000 | 1000 | 1000 | 500 | 100 | 0 | 5000 | 1000 |
| 6 | 5000 | 5000 | 5000 | 5000 | 5000 | 5000 | 0 | 1000 |
| 7 | 1000 | 1000 | 1000 | 1000 | 1000 | 100 | 1000 | 0 |

Stress `c23c` (0x1007c23c, 3x3; row = target stress, col = unit stress): `[0,100,50 / 100,0,20 / 50,20,0]`.
Right-context stress `c260` (0x1007c260, 3x3): `[0,10,5 / 10,0,1 / 5,1,0]`. Excitation class `c544` (0x1007c544, 3x3;
row = prev `spec>>14`, col = cur): `[0,100,0 / 100,0,0 / 0,0,0]`. Class-key weights `bf14` = [40, 1, 40, 10, 1000];
half-key weights `bf28` = [50000, 20000, 10000, 5000, 1000, 2000, 500, 100, 50, 20].

Join weights `c288` (float, per join type; w_cep w_F0 w_B): 0 voiced-voiced same word 10 10 5; 1 unvoiced-prev
voiced-cur 10 5 2; 2 voiced-prev unvoiced-cur 10 5 2; 3 unvoiced-unvoiced 10 2 2; 4 voiced-voiced across words 10 5 2.

Lattice position lists, `c4dc` left (index `lb*5 + lpos`): 0 `[0,1]`, 1 `[1,0,2]`, 2 `[2,1,3]`, 3 `[3,4,2,1]`, 4
`[4,3,2,1]`, 5 `[0]`, 6 `[1,0,2]`, 7 `[2,1,3]`, 8 `[3,4]`, 9 `[4,3]`. `c504` right (index `rpos + rb*8`): 0 `[0]`, 1
`[1,0,2]`, 2 `[2,1,3]`, 3 `[3,4,5]`, 4 `[4,3,5]`, 5 `[5]`, 6 `[6]`, 7 `[7,5]`, then 8-15 repeat 0-7.

Float constants: 0x1006d170 1.0 (weight numerator, run floor), 0x1006d174 2.0 (run/2, F0 sum/2, scalar-dist mean,
+2 join), 0x1006d178 0.5, 0x1006d17c 16.0 (EXP/EXP TC divisor), 0x1006d180 8.0 (edge TC divisor), 0x1006d184 5.0
(unvoiced dur/5, ETC score x5), 0x1006d188 0.0, 0x1006d1a0 50.0, 0x1006d1a4 10.0 (EXP score x10), 0x1006d1a8 4.0
(beam spread divisor), 0x1007c284 2.0 (TC to accumulated-cost divisor).

## Builds

| Build | Differences |
|---|---|
| 2014 English (this document) | 7-byte labels (`unit-*.idx` ver.2013), 5-byte class keys, preferred-candidate pass, domain handling in lookups |
| Violeta 2013 | see [VIOLETA.md](VIOLETA.md) |
| Julie09 | `src/unitsel09.rs`; 5-byte labels `[L1, C, R1, posbits, flags]` and 7-byte class keys (half keys 9 bytes); no preferred pass or domain handling in lookups; different back-off thresholds; stale half-list buffer reads (modelled by `lbuf`); 2009 lattice; contiguity `-(2*run [+1] + 100*wrun)`; f32 evaluation orders from the asm; `domain_from_tag09` (NAME tag 0x0b); `final_type` = sentence type (4/5/6) for the last phrase, 4 otherwise (mid-sentence phrases select with 4); word offsets n_phones +0x2e4, dur +0x0a, ctx +0x90, edge +0x1d5, flag +0x2fc, brk +0x2fd |
| Paul09 | equals Julie09 with `tib+0x54` |
| Violeta09 | half keys 8 bytes; start `acc = 1.0 * tc`; single speaker slot; no domain-0xc branch or kind-7 pass; word offsets +0x465, +0x0e, +0xd6, +0x2ca, +0x478, +0x479 |

Class tables per build are generated by `tools\gen_unitsel_tables_09.py` into `unitsel_tables_{julie09,paul09,viol09}.rs`;
`classes.rs` has `Tab09`, `class_key09`, `half_key09`, `Classes::build_era`; `unitsel.rs` has `Selector::new_era`
and `Word::from_raw_era`. 2009 index columns: [OLD_PAUL.md](OLD_PAUL.md).

## Verification

`cargo test --release --test unitsel_parity -- --nocapture` feeds each captured sentence's raw word and syllable
records through `Selector::select_phrase` phrase by phrase (one persistent pool per capture) and compares target
records and 3-byte features, every kept candidate's `tc`, `score` (f32 bits), `uid`, `fwd_run`, `run`, `wrun`, `pref`,
`kept_uid`, the final beam (`n`, uids, `acc` bits, backpointers), the path and the chosen uids.

| Date | Corpus | Result |
|---|---|---|
| 2026-09-23 | main set: 42 captures, 93 phrases, 3,269 targets | 42/42 exact |
| 2026-09-23 | + unit-selection set: 10 captures incl. 5 `vtml_emotion` EXP cases; 108 phrases, 3,697 targets total | 52/52 exact |
| 2026-09-23 | negative control: ETC penalty 10 to 11 | 0/52; 6,465 candidate fields, 2,252 beams, 19 paths differ |

Path coverage: half-phone targets 1,112; preferred substitution 555 targets; full-phrase-run branch 79 targets;
pruned beams 1,968; kept lists at 30: 2,857.

2009: `unitsel_{julie09,paul09,viol09}_parity.rs` (`*_matches_engine`, `*_speak_matches_engine`): 0 mismatches on
every target, candidate, beam, path and primary record; Julie09 6,460 + 119,613 targets, Paul09 19,031 + 135,518,
Violeta09 977,798.

## Open items

- Domain 8 (NUM_SPLIT): no Paul text produced it; its `units > 0` acceptance is ported but untested.
- The heapsort fallback of `us_SortByScore` (needs a 32:1 imbalance): ported from the disassembly, not seen firing.
- Physical scale of `featF` (probably Hz) and meaning of `featB` (probably energy) are inferred.
- Codes 0x5B-0x5F appear only in class-map tables; their front-end origin is unknown.
- 2009 `.his` history: no capture, untested.

## History

- 2026-09-23: 2014 selector ported bit-exact (42/42, then 52/52 with EXP-domain cases).
- x64 voice DLL evaluation order (`Selector::x64`) added.
- 2026-10-05: 2009 selector (`unitsel09.rs`) for Julie09, Paul09 and Violeta09: 0 mismatches.
