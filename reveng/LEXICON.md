# English dictionaries (`data-common\dict-eng`)

The lexicon files the English engine reads: two perfect-hashed dictionaries (the main pronunciation lexicon
`emb` and the TPP word-class dictionary), the hyphenated/foreign `exceptdict`, and nine `.txt2` tables. This
document gives the file formats, the key encoding, the hash, the payload decoding and where the front end calls
them ([FRONTEND_LING.md](FRONTEND_LING.md), [FRONTEND_TEXT.md](FRONTEND_TEXT.md)). Engine:
`C:\Program Files (x86)\NWSPaulNumber2\bin\vt_eng.dll` 3.11.7.1, image base `0x10000000`, VC6, cdecl; addresses
are VAs, functions renamed in Ghidra carry the `dict_` prefix. Rust: `src/lexicon.rs` (`Lexicon::open`,
`open_era`), `src/tree3.rs`, `src/tree2.rs`. Evidence tags: **[D]** decompile/disassembly, **[E]** verified
empirically, **[I]** inferred.

## 1. File inventory

| File | Bytes | Role | Loader |
|---|---:|---|---|
| `hashparams_emb` | 132,132 | perfect-hash parameters, main lexicon | `fe_LoadHashParams` 0x10011cb0 (arg 0) |
| `hashidx_emb` | 914,368 | `u32 maxrec` + `u32 offset[228,591]` into `engttsdict_emb` | `fe_LoadLexicons` 0x100114e0 |
| `hashcont_emb` | 457,182 | `{s8 keylen, u8 firstbyte}[228,591]` check table | same |
| `engttsdict_emb` | 2,157,937 | records `key\0payload\0` | same |
| `hashparams_eng_tpp` | 17,444 | parameters, TPP dictionary | `fe_LoadHashParams` (arg 1) |
| `hashidx_eng_tpp` | 126,204 | `u32 maxrec` + `u32 offset[31,550]` | `fe_LoadLexicons` (arg 1) |
| `hashcont_eng_tpp` | 63,100 | `{s8, u8}[31,550]` | same |
| `tppdict_eng` | 349,029 | records `key\0fields\0` | same |
| `exceptdict` | 2,891 | hyphenated / foreign multi-part pronunciations | `fe_LoadExceptDict` 0x10003660 |
| `*.txt2` (9 files) | | +10-shifted sorted text tables | `fe_LoadTxt2Table` 0x100033e0 |
| `engbi/sbd/poly/atmt.tree3` | | CARTs | [FRONTEND_TEXT.md](FRONTEND_TEXT.md), [PROSODY.md](PROSODY.md) |

The four hashed files are not loaded into RAM (only `hashparams_*` is read whole). Every lookup does three small
reads through `wav_ReadDbFileLocked` 0x10025440 (global critical section, seek + read): 2 bytes of `hashcont`,
4 bytes of `hashidx`, `maxrec` bytes of the dictionary.

## 2. Key encoding

### 2.1 Lexicon key (`fe_EncodeLexKey` 0x1000fd70) [D][E]

```
lower[i] = low_byte( u16 TOLOWER[(signed char) word[i]] )   // table 0x1007e388, stride 2; stop at the first 0
key      = pack_digraphs(lower)
```

`TOLOWER` is a 256-entry `u16` table indexed with a signed char, so bytes >= 0x80 read the 128 words before
`0x1007e388` (`0x1007e288..0x1007e386`). ASCII `A-Z` map to `a-z`; the rest of 0x20..0x7e maps to itself.

**Digraph packing** (greedy, left to right, `0x1000fde8..0x1000fea5`):

```
DG[0..126] = 127 two-letter strings at 0x10077b00, 3-byte stride ("ab\0", "ac\0", ... "wa\0"), sorted
i = 0; out = []
while i < len-1:
    lo = 0; hi = 0x7e
    while hi - lo > 1:
        mid = (lo + hi) / 2
        c = strncmp(&s[i], DG[mid], 2)           // CRT strncmp 0x10064650, unsigned bytes
        if c == 0: code = mid; goto hit
        if c < 0: hi = mid else lo = mid
    if strncmp(&s[i], DG[hi], 2) == 0: code = hi; goto hit     // hi checked before lo
    if strncmp(&s[i], DG[lo], 2) == 0: code = lo; goto hit
    out += s[i]; i += 1; continue
hit:
    out += (code + 1) | 0x80; i += 2
out += s[i]          // the last char, or the NUL if the final pair was consumed
```

Key bytes are a raw lower-case character (< 0x80) or `0x81..0xff` = digraph index + 1. Packing is greedy, not
optimal: `"hello"` -> `he|ll|o` = `ad c1 6f`. Every stored key re-encodes identically from its decoded spelling.

The 127 digraphs: `ab ac ad ag ai al am an ap ar as at ba be bi bl bo ca ce ch ci ck co cr ct da de di ea ec ed
ee el em en er es et fi ga ge gi gr ha he hi ho ia ic id ie ig il im in io ir is it iv ke la le li ll lo ly ma me
mi mo na nc nd ne ng ni no ns nt oc og ol om on oo op or os ot ou pa pe ph pi po pr ra re ri ro rs rt sa sc se sh
si so sp ss st su ta te th ti to tr ts ul un ur us ut ve wa`

### 2.2 TPP key (`dict_EncodeTppKey` 0x1003a610) [D]

Same algorithm with three differences: the digraph table is the byte-identical copy at `0x10081568`; the compare
is `dict_StrnCmpU` 0x1001c280 (unsigned, stops at NUL); mode `'S'` does not lower-case (case-sensitive key).
`dict_LookupTpp` (0x1003a570) encodes only for selector `0x45` (`'E'`); any other selector uses the raw string.

## 3. Perfect hash (Bob Jenkins `perfect.c`)

### 3.1 `hashparams_*` layout (`fe_LoadHashParams` 0x10011cb0) [D][E]

| Off | Size | Field | emb | tpp |
|---:|---:|---|---:|---:|
| 0 | 3 | magic `"NLP"` (optional, skipped if present) | present | present |
| 3 | 1 | hash type | 2 | 2 |
| 4 | 4 | `seed` (lookup2 initval) | `0xdbd20abf` | `0xcfc06585` |
| 8 | 4 | `f1` (type 1 only) | `0x8000` | `0x4000` |
| 12 | 4 | `shift` | 17 | 18 |
| 16 | 4 | `tabsize` (power of 2) | `0x20000` | `0x4000` |
| 20 | 4 | `f4` (guard: `< 2` gives slot 0) | `0x40000` | `0x8000` |
| 24 | 4 | `nkeys` | 228,591 | 31,550 |
| 28 | 4 | `nscramble` | 256 | 256 |
| 32 | 4*n | `u32 scramble[nscramble]` | | |
| ... | 4 | `ntab` | 131,072 | 16,384 |
| ... | ntab | `u8 tab[ntab]` | | |

File length equals the end of `tab` exactly. Integers are LE (byte-swapped by `pro_CopyMaybeByteswap` only when
`DAT_1007d6dc` is set, never on x86).

### 3.2 Slot (`dict_PerfectHashSlot` 0x10012030) [D][E]

```
slot(key, n):                       // n = strlen(encoded key), no terminator
    if type == 1:                   // not used by Paul's files
        h[0..7] = seed; dict_JenkinsChecksum(key, n, h)   // 0x10010c90
        return scramble[ tab[h[1] & (tabsize-1)] ] ^ (h[0] & (f1-1))
    val = dict_JenkinsLookup2(key, n, seed)               // 0x10010a50
    if f4 < 2: return 0
    if tabsize < 0x1000: return tab[val & (tabsize-1)] ^ (val >> (shift & 31))
    return scramble[ tab[val & (tabsize-1)] ] ^ (val >> (shift & 31))
```

Engine bug, harmless here: in the TPP branch with `tabsize < 0x1000` the code indexes the emb `tab`
(`DAT_1009f980`). TPP's `tabsize` is `0x4000`, so it is never taken.

`dict_JenkinsLookup2` is Jenkins' 1996 `lookup2.c` `hash()` unmodified: golden ratio `0x9e3779b9` for `a` and `b`,
`c = initval`, 12-byte blocks with LE assembly, `c += length` before the tail switch, `mix()` shifts 13, 8, 13,
12, 16, 5, 3, 10, 15. The caller stores `strlen(key)+1` at `key_struct+200` and passes that minus 1.

### 3.3 Lookup (`fe_HashDictLookup` 0x10011820) [D]

```
s = slot(key, n)
if s >= nkeys: miss                                   // unsigned
read 2 bytes hashcont[2s .. 2s+1]
if (signed char)hashcont[2s] != n  or  hashcont[2s+1] != key[0]: miss
read u32 hashidx[4 + 4s]            -> off
read maxrec bytes of dict at off    (maxrec = u32 hashidx[0]: emb 37, tpp 27)
if strcmp(record, key) != 0: miss
result = the NUL-terminated string after the key's NUL
```

With a non-null cache pointer (always from the front end) the same logic runs through an MRU cache
(`dict_MruCacheFind` 0x10011a50, `dict_MruCacheLookupOrFill` 0x10011ac0): a circular doubly-linked list at
`TIB+0x13b0` of `{u32 slot, char* key, char* result, prev@+0xc, next@+0x10}`; a hit moves the node to the front,
a miss recycles the tail and stores `""`. Results equal the uncached path.

Worked example (emb) [E]:

| | `hello` | `record` |
|---|---|---|
| key (n) | `ad c1 6f` (3) | `e3 97 72 64` (4) = re.co.r.d |
| lookup2 | `0x03867e46` | `0x094aee99` |
| `tab[val & 0x1ffff]` / `scramble[t]` | `0x02` / `0x000080c4` | `0x06` / `0x00021d4f` |
| `val >> 17` | `0x1c3` | `0x4a5` |
| slot | 33031 | 137706 |
| hashcont / record | `03 ad` / `ad c1 6f 00 | 11 d9 ce 26 00` | `04 e3` / `e3 97 72 64 00 | 12 26 7c a9 08 cc ff 14 0f 7c a8 66 cc 00` |

## 4. Lexicon payload (`engttsdict_emb`)

`dict_DecodeLexPayload` 0x10003c50 decodes into a 0x554-byte morph candidate; `dict_FillMorphFromPayload`
0x1000d450 copies it into the 0x70-byte morph entries.

### 4.1 Byte 0: flags [D]

| Bit | Effect | Stored at (0x554 record) | Words | Meaning [I] |
|---|---|---|---:|---|
| `0x01` | single pronunciation follows | | 53,171 (+ combos) | |
| `0x02` | POS-conditioned pronunciations follow | | 414 | homographs |
| `0x04` | class char `E` | +4 | 21+ | letters-only acronym (att, tko, lpga) |
| `0x08` | class char `A` (overrides `E`) | +4 | 35+ | also valid as abbreviation (was, do, by, psi) |
| `0x40` | bool | +0x54c | | proper name / non-dictionary token (macwhyte) |
| `0x10` | bool | +0x54e | 66,671 alone | common vocabulary |
| `0x20` | bool | +0x550 | | place/brand (mongolians, u.s.a, bergamo) |
| `0x80` | bool | +0x552 | 56,084 alone | surname list (drummond) |

The four bools are copied per morph to `+0x66/+0x68/+0x6a/+0x6c` (B40/B10/B20/B80, see
[FRONTEND_LING.md](FRONTEND_LING.md)) by `dict_CopyMorphsToWordArray` 0x1000ea20. Meanings come from word
samples only. **175,006 of 228,591 keys (77%) carry flags only**: no stored pronunciation; morphology or LTS
pronounces them (mode-7 source `AT`/`A`/`R`/`S`, never `G`). Top flag values: `0x10` 66,671, `0x80` 56,084,
`0x01` 32,672, `0x90` 16,260, `0x60` 14,649, `0x11` 12,803, `0xe0` 8,480, `0x64` 7,112, `0x61` 5,055 (54 values
present).

### 4.2 Mode `0x01`: single pronunciation [D][E]

`payload = flags, cb[1], cb[2], ... cb[k], 0x00`; `phones = CODEBOOK[cb[1]] ++ CODEBOOK[cb[2]] ++ ...`. POS list
is empty (`0xff`), count 1.

### 4.3 Mode `0x02`: POS-conditioned entries [D][E]

```
payload = flags, seg1, 0xff, seg2, 0xff, ... segk, 0x00        (last 0xff optional)
seg     = pos1+1, pos2+1, ..., '|' (0x7c), cb, cb, ...
```

Each segment ends at `0xff` or NUL; the first `0x7c` splits it into a POS list (bytes minus 1; POS <= 0x2b so no
collision) and codebook bytes (a codebook byte may be `0x7c`, entry 124). Example `record`: flags `0x12`; entry 1
POS [37 VB] codebook `a9 08 cc` = `R IH0 K AO1 R D`; entry 2 POS [19 NN, 14 JJ] `a8 66 cc` = `R EH1 K ER0 D`.
413 words have 2 entries, 1 word (`a`) has 3.

`dict_FillMorphFromPayload`: a single entry gives POS list `0xff`, phones at `+0x37`; multiple entries
concatenate POS lists at `+0x23` separated by byte 100 and terminated by `0xff`, phones at `+0x37 + 0x41*k`. The
tagger (`FUN_100068b0`, called from `dict_CopyMorphsToWordArray` when count >= 2) picks the entry whose POS list
contains the predicted tag; a caller-forced POS (argument 8 != -1, e.g. `vtml_partofsp`) bypasses the lists.

### 4.4 Phone codebook (`dict_InitPhoneCodebook` 0x1000ebc0 to `0x100fe900`) [D][E]

256 x 5-byte BSS table filled at start-up by 727 `mov` stores.

| Index | Content |
|---|---|
| 0, 255 | empty |
| 1..183 | multi-phone clusters: 1 = `AH0 B AH0 L` (-able), 2 = `D AH0 N`, 3 = `N IH0 S`, 179 = `T ER0`, 183 = `T S` |
| 184..252 | single phone codes 1..0x45 (index - 183) |
| 253 | `100` = `'d'` |
| 254 | `99` = `'c'` |

### 4.5 Phone codes and marks [D][E]

Codes `1..0x45` are ARPAbet in alphabetical order with stress folded in:

| Code | Phone | Code | Phone | Code | Phone |
|---|---|---|---|---|---|
| 01-03 | AA0-2 | 20 | F | 2F-31 | OW0-2 |
| 04-06 | AE0-2 | 21 | G | 32-34 | OY0-2 |
| 07-09 | AH0-2 | 22 | HH | 35 | P |
| 0A-0C | AO0-2 | 23-25 | IH0-2 | 36 | R |
| 0D-0F | AW0-2 | 26-28 | IY0-2 | 37 | S |
| 10-12 | AY0-2 | 29 | JH | 38 | SH |
| 13 | B | 2A | K | 39 | T |
| 14 | CH | 2B | L | 3A | TH |
| 15 | D | 2C | M | 3B-3D | UH0-2 |
| 16 | DH | 2D | N | 3E-40 | UW0-2 |
| 17-19 | EH0-2 | 2E | NG | 41-45 | V W Y Z ZH |
| 1A-1C | ER0-2 | | | | |
| 1D-1F | EY0-2 | | | | |

Marks: byte 99 `'c'` (15,555x) compound/prefix morpheme boundary (`thunder.storm`; the mode-7 dump prints `-`); byte
100 `'d'` (1,041x) hyphen-part boundary (`sight-read` = `S AY1 T d R IY1 D`), used by `dict_ApplyExceptPron`.

### 4.6 POS indices (Penn Treebank, alphabetical) [E by homograph fit]

| Idx | Tag | Evidence | Idx | Tag | Evidence |
|---:|---|---|---:|---|---|
| 10 | DT | `a` AH0 | 22 | NNS | `contests`, `uses` (noun) |
| 12 | FW | `de`, `joie` | 23 | ? | `alt` AO1 L T |
| 13 | IN | `re`, `inter` | 28 | RB | `overall`, `supply` |
| 14 | JJ | `deliberate`, `content` | 37 | VB | `record`, `use`, `permit` |
| 15 | JJR | `closer` | 38 | VBD | `read`/`lied` |
| 17 | LS | `a` EY1 | 39 | VBG | `uprising` |
| 18 | MD | `can` K AE0 N | 40 | VBN | `read`, `rebound` |
| 19 | NN | `record`, `permit` | 41 | VBP | `underlay` |
| 20 | NNP | `lead` L EH1 D, `job` | 42 | VBZ | `contests`, `uses` (verb) |

Consistent with CC=8, CD=9, DT=10, EX=11, FW=12, IN=13, JJ=14, JJR=15, JJS=16, LS=17, MD=18, NN=19, NNP=20,
NNPS=21, NNS=22, VB=37..VBZ=42; 23-36 hold PDT..UH plus 3 engine-specific tags. RB=28 is fixed by data (pure
alphabetical would give 27). No tag-name strings exist in the DLL; the name table is at 0x100779b0 (7-byte
stride, 48 tags, `tables::POS_NAMES`).

## 5. TPP dictionary (`tppdict_eng`)

Payload = space-separated fields `<letter><digits>` [E]:

| Letter | Count | Meaning [I] | Examples |
|---|---:|---|---|
| `G<n>` | 18,559 | tagger lexical class id (up to 124) | `the G23`, `a G22`, `of G61`, `running G120` |
| `A<d>` | 7,904 | 1-part proper name (place) | `upland A0 G95` |
| `B<dd>` | 3,981 | 2-part hyphen-joined name | `wilbur-springs B201` |
| `C<ddd>` | 310 | 3-part | `fond-du-lac C3000` |
| `D<dddd>` | 19 | 4-part | `cardiff-by-the-sea D40000` |
| `E<ddddd>` | 2 | 5-part | `saint-mary-of-the-woods E500000` |
| `F<n>` | 829 | hyphenated compound + G class | `counter-attacking F120` |

For `B`-`E` the first digit repeats the part count and the following digits carry one flag per part; keys join
parts with `-`. `dict_GetTppField` 0x1003a7b0(out, word, letter, mode, ...): look up the word (mode `'S'` keeps
case); if the payload's first char is `letter` take the rest, else `strstr` for `" <letter>"` (`" %c"` at
`0x10080384`) and take the text after it; cut at the next space (`FUN_10024b50`, `" "` at `0x1007b3cc`).
The field extraction is checked over all 31,550 payloads x 7 letters (220,850/220,850, `tpp_field_equivalence`).
The city-name normalizer uses `tppdict_eng` field `'A'+n-1` as its gate ([FRONTEND_TEXT.md](FRONTEND_TEXT.md)).

## 6. `exceptdict`

Loader `fe_LoadExceptDict` 0x10003660; lookup `dict_ExceptDictFind` 0x10003b70 driven by
`dict_MatchHyphenatedExcept` 0x10008dc0. Not obfuscated (keys are digraph-packed).

```
u32 min_parts (1)   u32 max_parts (4)
repeat for g = min..max:
    u32 g   u32 n
    n * { u32 klen, u32 vlen, u8 key[klen], u8 val[vlen] }
```

Groups hold 11 / 89 / 22 / 1 entries; the last byte ends exactly at EOF.
- **key** = `fe_EncodeLexKey(lower(parts joined by '-'))`, sorted by unsigned bytes within a group (binary search).
- **g** = total hyphen-separated parts across the consumed tokens; `dict_CountHyphenParts` 0x1000c9c0 counts
  letters, `'` and `-` only (any other char gives 0, "not a candidate").
- **Matching**: tokens are appended with `"-%s"` (`0x10077890`) until the part count reaches `min_parts` or
  exceeds `max_parts`; at each count in range a lookup is tried. If a word is marked `'X'` (`WA+0x24`, accented
  input) and the plain key misses, retry with the prefix `"h'%s"` (`0x10077888`): accented loanwords are stored
  as `h'expose`, `h'touche`, `h'resume`.
- **val** = raw phone codes (1..0x45) with marks `c`/`d`, not a codebook payload. `dict_ApplyExceptPron`
  0x1000ca50 pours it into consecutive morph entries, moving to the next part's entry at each `d` once the
  current part's syllable budget (`FUN_1000ca30`) is used.

Examples: `a-priori EY2 d P R AY0 AO1 R AY0`, `faux-pas F OW1 d P AA1`, `h'canon K AE1 N Y AH0 N`.

## 7. `*.txt2` tables

Loader `fe_LoadTxt2Table` 0x100033e0 (via `FUN_10010300`). Every byte is stored +10 mod 256. Decoded:

```
"dbf"  line1 "\r\n" ... linen "\r\n"  "\0"  count (ASCII, NUL-padded to 10)  "dbf"
line = key              (mode 'L' = 0x4c: key-only set)
     | key "|" value    (mode 'M' = 0x4d: map)
```

| File | Rows | Mode | Content |
|---|---:|---|---|
| `abbrc_sort` | 330 | M | abbreviation action (1 spell letters, 2 expand) |
| `abbrh_sort` | 55 | M | honorific abbreviations |
| `abbrt_sort` | 42 | M | title abbreviations |
| `chc_sort` | 2,581 | M | consonant cluster to 4-bit syllabification flags |
| `citya_sort` | 117 | M | city abbreviations |
| `sbdw_sort` | 1,067 | M | sentence-boundary word classes |
| `streeta_sort` | 347 | M | street-type abbreviations (`ALY|Alley`) |
| `streetf_sort` | 211 | L | street full names |
| `wab` | 113 | L | function words |

## 8. Front-end use (call graph)

```
fe_TagAndPronounce 0x1000e2f0
 FUN_1000d190 (per token; appends . ? ! , - to the token text for classes 2/3/4/5,0xc/0xb when < 29 chars)
   FUN_1000d640   split token into candidate spellings
   FUN_1000fd20   casing class: 0 none, 1 lower-only, 2/3 contains upper
   dict_LookupLexicon 0x10003a70 -> fe_EncodeLexKey -> fe_HashDictLookup(..., cache, emb)
   dict_FillMorphFromPayload 0x1000d450 -> dict_DecodeLexPayload 0x10003c50
 dict_CopyMorphsToWordArray 0x1000ea20 -> FUN_100068b0 (POS tagger) when count >= 2
 dict_MatchHyphenatedExcept 0x10008dc0 -> dict_ExceptDictFind -> dict_ApplyExceptPron
 FUN_10007520 (morphology) / fe_LetterToSound 0x10002f10 for flags-only and unknown words
```

Mode-7 dump source tags: `G` stored lexicon pronunciation; `AT`, `A`, `R` (from a root), `GR` morphology/LTS
variants; `S` special or spelled (printed by `FUN_10018020` from the WA source byte).

## 9. Builds

| Build | Lexicon |
|---|---|
| 2014 English | everything above; trees are `.tree3` |
| Violeta 2013 | separate Spanish lexicon and LTS, [VIOLETA.md](VIOLETA.md) |
| Julie09 | same formats; `Lexicon`, `atmt.tree3` (u32 count + 27 tree3), `engbi/poly/sbd.tree3` load unchanged |
| Paul09 | every tree is `.tree2` (`dict-eng\{atmt,engbi,poly,sbd}.tree2`); `Lexicon::open_era` loads `atmt`, `engbi` and `poly` and `Fe::load_era` loads `sbd`, all through `src/tree2.rs`, which converts tree2 into `tree3::Tree` |
| Violeta09 | lexicon/LTS pass `FUN_1001f3d0` is much shorter than 2014's (8 of 188 LTS words differ); [VIOLETA.md](VIOLETA.md) |

tree2 (`src/tree2.rs`, from Paul09 `FUN_10001850`/`FUN_10001650` scalar, `FUN_10001d10`/`FUN_10001ac0` vector,
`FUN_10001910` atmt, walkers `FUN_100015f0`/`FUN_10001a40`): pre-order recursive nodes, each with its own
value(s). Scalar node `feat kind thresh value nset set[] b1 b2`; vector node `feat kind thresh nval vals[] nset
set[] b1 b2`; `b2 != 0` means yes/no subtrees follow. The walk equals tree3's. `atmt.tree2` = 27 x (u32 len +
scalar tree); poly/sbd/engbi are scalar; pitch `*f` trees vector (dim 12). `tests/tree2_parse.rs` parses all
20 files and 27 atmt records to the exact byte.

## 10. Verification

| Check | Result |
|---|---|
| every emb key: `slot()` is its own index, `hashcont` passes | 228,591 / 228,591 |
| every tpp key | 31,550 / 31,550 |
| decoded spelling re-packs to the stored key | 228,591 / 228,591, 31,550 / 31,550 |
| `hashparams_*` length = header + scramble + tab | exact |
| `exceptdict` consumes the file exactly, groups sorted | yes |
| `*.txt2` trailer counts equal line counts | 9 / 9 |
| engine mode-7 phones vs decoded lexicon, 400 random single-pron words | 400 / 400, all source `G` |
| engine on 100 random flags-only words | 0 `G`; `AT` 46, `A` 31, `R` 17, `S` 5, `GR` 1 |
| `tests/tree2_parse.rs` | 20 files, 27 atmt records |
| Lexicon use end to end | `feling_parity` ([FRONTEND_LING.md](FRONTEND_LING.md)) |

The reference reader, verification host and TSV dumps (decoded lexicon 228,591 rows, TPP 31,550, exceptdict
123, nine txt2 tables, codebook) are not in this repository.

## 11. Open items

- Flag bits 0x10/0x20/0x40/0x80: routing to `+0x66..+0x6c` is known, the meanings are sample-based.
- POS indices 23-27 and 29-36: only 10-22, 28 and 37-42 are pinned by homograph data.
- TPP `G<n>` class ids need the tagger's transition tables to name; the per-part digits of `A`-`E` are inferred.
- `FUN_1000d190` appends punctuation to the token for some classes, yet no stored key ends in punctuation;
  presumably `FUN_1000d640` strips it before lookup (not traced).
- Type-1 phash (`dict_JenkinsChecksum`) is decoded but unused, so untested.

## 12. History

- 2026-09: hash, key encoding, payload formats decoded; all keys verified against the engine's own lookup.
- 2026-09: `exceptdict`, `.txt2`, codebook and POS fit decoded; TPP field extraction cross-checked.
- 2026-10-05: Paul09 `.tree2` formats parsed (`tests/tree2_parse.rs`); Julie09 lexicon loads with the 2014 reader.
