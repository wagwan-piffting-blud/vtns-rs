# VoiceText Paul: English dictionaries (`data-common\dict-eng`)

Engine: `C:\Program Files (x86)\NWSPaulNumber2\bin\vt_eng.dll` 3.11.7.1. Image base `0x10000000`, VC6, cdecl throughout. All addresses below are VAs in that image; functions renamed in Ghidra this pass carry the `dict_` prefix.

**Status:** fully decoded and verified.
- Every key in both hashed dictionaries resolves through the reimplemented hash to its own slot: **228,591 / 228,591** (`emb`) and **31,550 / 31,550** (`tpp`).
- **400 / 400** randomly sampled lexicon pronunciations are byte-identical to what the engine's front end emits for the same word, checked with `VT_TextToPreprocessInfoFile_ENG` mode 7.
- Reference implementation: a standalone Python reader (lookup, decode, dump; not part of this repository) that mirrors the logic documented below. Full TSV dumps of the decoded tables were produced from it for verification.

Evidence tags: **[D]** read from decompilation/disassembly, **[E]** verified empirically on the files or the running engine, **[I]** inferred.

---

## 1. File inventory

| File | Bytes | Role | Loader |
|---|---:|---|---|
| `hashparams_emb` | 132,132 | Perfect-hash parameters for the main lexicon | `fe_LoadHashParams` `0x10011cb0` (arg 0) |
| `hashidx_emb` | 914,368 | `u32 maxrec` + `u32 offset[228,591]` into `engttsdict_emb` | `fe_LoadLexicons` `0x100114e0` |
| `hashcont_emb` | 457,182 | `{s8 keylen, u8 firstbyte}[228,591]` check table | same |
| `engttsdict_emb` | 2,157,937 | Records `key\0payload\0` | same |
| `hashparams_eng_tpp` | 17,444 | Perfect-hash parameters, TPP dictionary | `fe_LoadHashParams` (arg 1) |
| `hashidx_eng_tpp` | 126,204 | `u32 maxrec` + `u32 offset[31,550]` | `fe_LoadLexicons` (arg 1) |
| `hashcont_eng_tpp` | 63,100 | `{s8, u8}[31,550]` | same |
| `tppdict_eng` | 349,029 | Records `key\0fields\0` | same |
| `exceptdict` | 2,891 | Hyphenated / foreign multi-part pronunciations | `fe_LoadExceptDict` `0x10003660` |
| `*.txt2` (9 files) | — | +10-shifted sorted text tables | `fe_LoadTxt2Table` `0x100033e0` |
| `engbi/sbd/poly/atmt.tree3` | — | CARTs (see ENGINE.md, Prosody / Front end) | not covered here |

The four hashed files are **not** loaded into RAM. `fe_LoadLexicons` only opens them. Every lookup does 3 small reads through `wav_ReadDbFileLocked` `0x10025440`, a global-critical-section seek+read: 2 bytes of `hashcont`, 4 bytes of `hashidx`, and `maxrec` bytes of the dictionary. Only `hashparams_*` is read whole.

---

## 2. Key encoding

### 2.1 Lexicon key (`fe_EncodeLexKey` `0x1000fd70`) [D][E]

```
lower[i] = low_byte( u16 TOLOWER[(signed char) word[i]] )   // table 0x1007e388, stride 2
           (stop at the first 0 produced)
key      = pack_digraphs(lower)
```

`TOLOWER` is a 256-entry `u16` table. It is indexed with a **signed** char (`MOVSX`), so bytes ≥ 0x80 read the 128 words *before* `0x1007e388`, i.e. `0x1007e288..0x1007e386`. `vtdict_tables.json` holds the exact byte read for every index −128..127. ASCII `A–Z` map to `a–z`; everything else in 0x20..0x7e maps to itself.

**Digraph packing** (greedy, left to right, exactly as disassembled at `0x1000fde8..0x1000fea5`):

```
DG[0..126] = 127 two-letter strings at 0x10077b00, 3-byte stride ("ab\0", "ac\0", ... "wa\0"), sorted
i = 0; out = []
while i < len-1:
    lo = 0; hi = 0x7e
    while hi - lo > 1:
        mid = (lo + hi) / 2                // signed div, but always ≥ 0
        c = strncmp(&s[i], DG[mid], 2)     // CRT strncmp 0x10064650, unsigned bytes
        if c == 0: code = mid; goto hit
        if c < 0: hi = mid else lo = mid
    if strncmp(&s[i], DG[hi], 2) == 0: code = hi; goto hit     // hi checked before lo
    if strncmp(&s[i], DG[lo], 2) == 0: code = lo; goto hit
    out += s[i]; i += 1; continue
hit:
    out += (code + 1) | 0x80; i += 2
out += s[i]          // the last char, or the NUL if the final pair was consumed
```

So key bytes are either a raw lower-case character (< 0x80) or `0x81..0xff` = digraph index + 1. Packing is **greedy**, not optimal: `"hello"` → `he|ll|o` = `ad c1 6f`. It is also deterministic, so every stored key re-encodes identically from its decoded spelling (228,591/228,591 and 31,550/31,550 [E]).

The 127 digraphs:
`ab ac ad ag ai al am an ap ar as at ba be bi bl bo ca ce ch ci ck co cr ct da de di ea ec ed ee el em en er es et fi ga ge gi gr ha he hi ho ia ic id ie ig il im in io ir is it iv ke la le li ll lo ly ma me mi mo na nc nd ne ng ni no ns nt oc og ol om on oo op or os ot ou pa pe ph pi po pr ra re ri ro rs rt sa sc se sh si so sp ss st su ta te th ti to tr ts ul un ur us ut ve wa`

### 2.2 TPP key (`dict_EncodeTppKey` `0x1003a610`) [D]

The algorithm is identical, except for three things:
- The digraph table is the copy at `0x10081568`. It is byte-identical to `0x10077b00` [E].
- The compare is `dict_StrnCmpU` `0x1001c280`, an unsigned-byte strncmp that stops at NUL.
- A mode byte: if `mode == 'S'`, the word is **not** lower-cased (case-sensitive key); otherwise it is lower-cased through the same `TOLOWER` table.

`dict_LookupTpp` (`0x1003a570`) encodes only when its selector is `0x45` (`'E'`). With any other selector the raw string is used as the key.

---

## 3. Perfect hash (Bob Jenkins `perfect.c` phash)

### 3.1 `hashparams_*` layout (`fe_LoadHashParams` `0x10011cb0`) [D][E]

| Off | Size | Field | Global (emb / tpp) | emb | tpp |
|---:|---:|---|---|---:|---:|
| 0 | 3 | magic `"NLP"` (optional; skipped if present) | — | present | present |
| 3 | 1 | hash type | `DAT_1009f978` / `…95c` | **2** | **2** |
| 4 | 4 | `seed` (lookup2 initval) | `…988` / `…98c` | `0xdbd20abf` | `0xcfc06585` |
| 8 | 4 | `f1` (only used by type 1) | `…9b8` / `…970` | `0x8000` | `0x4000` |
| 12 | 4 | `shift` | `…9b4` / `…9bc` | 17 | 18 |
| 16 | 4 | `tabsize` (power of 2) | `…964` / `…96c` | `0x20000` | `0x4000` |
| 20 | 4 | `f4` (guard: `< 2` ⇒ slot 0) | `…9c8` / `…958` | `0x40000` | `0x8000` |
| 24 | 4 | `nkeys` | `…990` / `…994` | 228,591 | 31,550 |
| 28 | 4 | `nscramble` | `…9b0` / `…99c` | 256 | 256 |
| 32 | 4·n | `u32 scramble[nscramble]` | `…974` / `…998` | | |
| … | 4 | `ntab` | `…9a4` / `…984` | 131,072 | 16,384 |
| … | ntab | `u8 tab[ntab]` | `…980` / `…97c` | | |

The file length equals the end of `tab` exactly for both files [E]. Integers are LE, byte-swapped by `pro_CopyMaybeByteswap` only when `DAT_1007d6dc` (host is big-endian) is set, which it never is on x86.

### 3.2 Slot computation (`dict_PerfectHashSlot` `0x10012030`) [D][E]

```
slot(key, n):                       // n = strlen(key) (see note)
    if type == 1:                   // not used by Paul's files
        h[0..7] = seed; dict_JenkinsChecksum(key, n, h)   // 0x10010c90 = perfect.c checksum()
        return scramble[ tab[h[1] & (tabsize-1)] ] ^ (h[0] & (f1-1))
    val = dict_JenkinsLookup2(key, n, seed)               // 0x10010a50
    if f4 < 2: return 0
    if tabsize < 0x1000: return tab[val & (tabsize-1)] ^ (val >> (shift & 31))
    return scramble[ tab[val & (tabsize-1)] ] ^ (val >> (shift & 31))
```

**Engine bug (harmless here) [D]:** in the TPP branch with `tabsize < 0x1000`, the code indexes the **emb** `tab` (`DAT_1009f980`) instead of the TPP one. TPP's `tabsize` is `0x4000`, so the buggy branch is never taken. A port only needs the `≥ 0x1000` path, but should keep the quirk if it supports other voices' dictionaries.

**`dict_JenkinsLookup2`** is Bob Jenkins' 1996 `lookup2.c` `hash()`, unmodified: golden ratio `0x9e3779b9` for `a` and `b`, `c = initval`, 12-byte blocks with LE assembly of 4-byte groups, `c += length` before the tail switch, and the standard `mix()` shifts 13, 8, 13, 12, 16, 5, 3, 10, 15. See `vtdict.lookup2`.

**Key-length note [D]:** the caller stores `strlen(key)+1` at `key_struct+200` (the scas count including NUL) and passes `that−1`. So the hash length is `strlen(encoded key)` without the terminator.

### 3.3 Lookup (`fe_HashDictLookup` `0x10011820`) [D]

```
s = slot(key, n)
if s >= nkeys: miss                                   // unsigned compare
read 2 bytes hashcont[2s .. 2s+1]
if (signed char)hashcont[2s] != n  or  hashcont[2s+1] != key[0]: miss
read u32 hashidx[4 + 4s]            -> off
read maxrec bytes of dict at off    (maxrec = u32 hashidx[0]: emb 37, tpp 27)
if strcmp(record, key) != 0: miss
result = the NUL-terminated string that follows the key's NUL
```

When the lookup's cache-pointer argument is non-null (always, from the front end), it routes the same logic through an **MRU cache**: `dict_MruCacheFind` `0x10011a50` + `dict_MruCacheLookupOrFill` `0x10011ac0`. It is a circular doubly-linked list at `TIB+0x13b0` of `{u32 slot, char* key, char* result, prev@+0xc, next@+0x10}`. A hit moves the node to the front. A miss recycles the tail node and stores `""` as its result. Results are identical to the uncached path; it only saves file reads.

**Worked examples** (emb, from the standalone reference reader) [E]:

| | `hello` | `record` | `thunderstorm` |
|---|---|---|---|
| lower | `68 65 6c 6c 6f` | `72 65 63 6f 72 64` | `74 68 75 6e 64 65 72 73 74 6f 72 6d` |
| key (n) | `ad c1 6f` (3) = he·ll·o | `e3 97 72 64` (4) = re·co·r·d | `f4 fa 9b e6 f6 72 6d` (7) = th·un·de·rs·to·r·m |
| lookup2 | `0x03867e46` | `0x094aee99` | `0xf2fcd7b3` |
| `tab[val & 0x1ffff]` | `tab[0x07e46]=0x02` | `tab[0x0ee99]=0x06` | `tab[0x0d7b3]=0x01` |
| `scramble[t]` | `0x000080c4` | `0x00021d4f` | `0x000166c9` |
| `val >> 17` | `0x1c3` | `0x4a5` | `0x797e` |
| slot | **33031** | **137706** | **73655** |
| hashcont | `03 ad` | `04 e3` | `07 f4` |
| hashidx (file off) | `0x20420` → `0x8db42` | `0x867ac` → `0x10fc86` | `0x47ee0` → `0x159a2a` |
| record | `ad c1 6f 00 │ 11 d9 ce 26 00` | `e3 97 72 64 00 │ 12 26 7c a9 08 cc ff 14 0f 7c a8 66 cc 00` | `f4 fa 9b e6 f6 72 6d 00 │ 11 f1 bf 9f d1 fe b0 40 e3 00` |

---

## 4. Lexicon payload (`engttsdict_emb`)

Decoded by `dict_DecodeLexPayload` `0x10003c50` into a 0x554-byte morph candidate. `dict_FillMorphFromPayload` `0x1000d450` then copies it into the 0x70-byte morph entries.

### 4.1 Byte 0: flags [D]

| Bit | Engine effect | Stored at (0x554 record) | Words | Inferred meaning [I] |
|---|---|---|---:|---|
| `0x01` | **single pronunciation** follows | — | 53,171 (+ combos) | — |
| `0x02` | **POS-conditioned pronunciations** follow | — | 414 | homographs |
| `0x04` | class char `'E'` | `+4` (byte) | 21+ | letters-only acronym (att, tko, tgif, lpga) |
| `0x08` | class char `'A'` (overrides `'E'`) | `+4` | 35+ | word also valid as an abbreviation (was, do, by, psi, era) |
| `0x40` | bool | `+0x54c` | | proper-name / non-dictionary token (sbci, macwhyte, iubs) |
| `0x10` | bool | `+0x54e` | 66,671 alone | common-vocabulary word |
| `0x20` | bool | `+0x550` | | place/brand context (mongolians, u.s.a, bergamo) |
| `0x80` | bool | `+0x552` | 56,084 alone | surname / name list (schlachter, drummond) |

The four bools are copied per morph entry to `+0x66/+0x68/+0x6a/+0x6c` by `dict_CopyMorphsToWordArray` `0x1000ea20` (the `WA` flags the front-end doc lists at `+0x13/+0x15/+0x16/+0x18`). Their consumers were not traced; the "meaning" column comes from word samples only.

**175,006 of 228,591 keys (77%) carry flags only** (neither `0x01` nor `0x02`). They have no stored pronunciation. The engine pronounces them through morphology or LTS: the mode-7 dump reports source `AT`/`A`/`R`/`S` for 100/100 sampled flags-only words, never `G` [E]. The lexicon therefore doubles as a word list with class bits.

Flag histogram (all 54 values present), reproducible from the decoded lexicon. Top values: `0x10` 66,671 · `0x80` 56,084 · `0x01` 32,672 · `0x90` 16,260 · `0x60` 14,649 · `0x11` 12,803 · `0xe0` 8,480 · `0x64` 7,112 · `0x61` 5,055.

### 4.2 Mode `0x01`: single pronunciation [D][E]

```
payload = flags, cb[1], cb[2], ... cb[k], 0x00
phones  = CODEBOOK[cb[1]] ++ CODEBOOK[cb[2]] ++ ...
```

The result has POS list = empty (`0xff`) and count = 1.

### 4.3 Mode `0x02`: POS-conditioned entries [D][E]

```
payload = flags, seg₁, 0xff, seg₂, 0xff, ... segₖ, 0x00        (last 0xff optional)
seg     = pos₁+1, pos₂+1, ..., '|' (0x7c), cb, cb, ...
```

The parser finds each segment's end at `0xff` or NUL, then scans from the segment start for the first `0x7c`. Everything before it is a POS list (each byte −1); everything after it is codebook bytes. POS bytes are ≤ 0x2b, so they can never collide with `'|'`. A codebook byte may legally be `0x7c` (entry 124), because only the *first* bar splits.

Worked example, `record`: `12 │ 26 7c a9 08 cc │ ff │ 14 0f 7c a8 66 cc`
- flags `0x12` = multi + `0x10`
- entry 1: POS [0x26−1 = 37 VB], codebook `a9 08 cc` → `R IH0 · K AO1 R · D` = `R IH0 K AO1 R D`
- entry 2: POS [19 NN, 14 JJ], codebook `a8 66 cc` → `R EH1 · K ER0 · D` = `R EH1 K ER0 D`

Distribution: 413 words with 2 entries, 1 word (`a`) with 3.

In `dict_FillMorphFromPayload`:
- **single entry:** the POS list becomes `0xff` and the phone string goes to `+0x37`.
- **multiple entries:** POS lists are concatenated at `+0x23`, separated by byte `100` and terminated by `0xff`. Phone strings sit at `+0x37 + 0x41·k`.
- **selection:** the tagger (`FUN_100068b0`, called from `dict_CopyMorphsToWordArray` when count ≥ 2) chooses the entry whose POS list contains the predicted tag.
- **forced POS:** a caller-forced POS (argument 8 ≠ −1, e.g. from `vtml_partofsp`) bypasses the lists.

### 4.4 Phone codebook (`dict_InitPhoneCodebook` `0x1000ebc0` → `0x100fe900`) [D][E]

A 256 × 5-byte BSS table filled at start-up by 727 `mov` stores. It was extracted by emulating those stores with a small script (not part of this repository); the decoded values are summarized below.

| Index | Content |
|---|---|
| 0, 255 | empty |
| 1..183 | multi-phone clusters, e.g. 1 = `AH0 B AH0 L` (-able), 2 = `D AH0 N`, 3 = `N IH0 S`, 179 = `T ER0`, 183 = `T S` |
| 184..252 | single phone codes 1..0x45 (index − 183) |
| 253 | `100` = `'d'` |
| 254 | `99` = `'c'` |

### 4.5 Phone codes and marks [D][E]

Codes `1..0x45` are the CMU/ARPAbet set in alphabetical order, with stress folded in:

| Code | Phone | Code | Phone | Code | Phone | Code | Phone |
|---|---|---|---|---|---|---|---|
| 01–03 | AA0–2 | 17–19 | EH0–2 | 29 | JH | 35 | P |
| 04–06 | AE0–2 | 1A–1C | ER0–2 | 2A | K | 36 | R |
| 07–09 | AH0–2 | 1D–1F | EY0–2 | 2B | L | 37 | S |
| 0A–0C | AO0–2 | 20 | F | 2C | M | 38 | SH |
| 0D–0F | AW0–2 | 21 | G | 2D | N | 39 | T |
| 10–12 | AY0–2 | 22 | HH | 2E | NG | 3A | TH |
| 13 | B | 23–25 | IH0–2 | 2F–31 | OW0–2 | 3B–3D | UH0–2 |
| 14 | CH | 26–28 | IY0–2 | 32–34 | OY0–2 | 3E–40 | UW0–2 |
| 15 | D | | | | | 41–45 | V W Y Z ZH |
| 16 | DH | | | | | | |

Marks inside pronunciations:

| Byte | Char | Seen | Meaning |
|---|---|---:|---|
| 99 | `'c'` | 15,555 × | compound / prefix morpheme boundary (`thunder·storm`, `over·lay`, `re·use`); printed as `-` by the mode-7 dump [E] |
| 100 | `'d'` | 1,041 × | hyphen-part boundary (`sight-read` → `S AY1 T d R IY1 D`). Used by `dict_ApplyExceptPron` to split one pronunciation across several hyphen parts [D] |

### 4.6 POS indices (Penn Treebank, alphabetical) [E by homograph fit]

| Idx | Tag | Evidence | Idx | Tag | Evidence |
|---:|---|---|---:|---|---|
| 10 | DT | `a` → AH0 | 22 | NNS | `contests`, `uses` (noun) |
| 12 | FW | `de`, `joie` | 23 | ? | `alt` → AO1 L T |
| 13 | IN | `re`, `inter`, `de` | 28 | RB | `overall`, `supply` (= supplely), `loathly` |
| 14 | JJ | `deliberate`, `content` | 37 | VB | `record`, `use`, `permit` (verb) |
| 15 | JJR | `closer`, `number` (= more numb) | 38 | VBD | `read`/`lied` → R EH1 D / L AY1 D |
| 17 | LS | `a` → EY1 | 39 | VBG | `uprising`, `nestling` |
| 18 | MD | `can` → K AE0 N | 40 | VBN | `read`, `rebound` |
| 19 | NN | `record`, `permit` (noun) | 41 | VBP | `underlay` |
| 20 | NNP | `lead` → L EH1 D, `job` → JH OW1 B, `reading` → R EH1 D… | 42 | VBZ | `contests`, `uses` (verb) |

The fit is consistent with CC=8, CD=9, DT=10, EX=11, FW=12, IN=13, JJ=14, JJR=15, JJS=16, LS=17, MD=18, NN=19, NNP=20, NNPS=21, NNS=22, and VB=37…VBZ=42. Indices 23–36 hold PDT … UH plus 3 engine-specific tags. RB=28 is fixed by data, while pure alphabetical order would put RB at 27. **No tag-name strings exist in the DLL** [E], so names come only from this fit.

---

## 5. TPP dictionary (`tppdict_eng`)

Record payload = space-separated fields, each `<letter><digits>` [E]:

| Letter | Count | Meaning [I] | Examples |
|---|---:|---|---|
| `G<n>` | 18,559 | tagger lexical class id `n` (95, 120, 39, 54, 98, … up to 124) | `the G23`, `a G22`, `of G61`, `he G83`, `running G120` |
| `A<d>` | 7,904 | 1-part proper name (place), digit flags | `upland A0 G95`, `bisbee A0` |
| `B<dd>` | 3,981 | 2-part hyphen-joined name | `wilbur-springs B201` |
| `C<ddd>` | 310 | 3-part | `fond-du-lac C3000` |
| `D<dddd>` | 19 | 4-part | `cardiff-by-the-sea D40000` |
| `E<ddddd>` | 2 | 5-part | `saint-mary-of-the-woods E500000` |
| `F<n>` | 829 | hyphenated compound + G-class | `counter-attacking F120` |

For `B`–`E` the first digit repeats the part count and the following digits carry one flag per part. Keys join parts with `-`.

The engine reads a field with `dict_GetTppField` `0x1003a7b0(out, word, letter, mode, …)`:
1. It looks the word up (mode `'S'` keeps case).
2. If the payload's first char is `letter`, it takes the rest.
3. Otherwise it `strstr`s for `" <letter>"` (`" %c"` at `0x10080384`) and takes the text after it.
4. It then cuts at the next space (`FUN_10024b50` with `" "` at `0x1007b3cc`).

---

## 6. `exceptdict`: hyphenated and foreign multi-part words

Loader `fe_LoadExceptDict` `0x10003660`. Lookup `dict_ExceptDictFind` `0x10003b70`, driven by `dict_MatchHyphenatedExcept` `0x10008dc0`. It is **not obfuscated**; it looked scrambled only because keys are digraph-packed.

```
u32 min_parts (1)   u32 max_parts (4)
repeat for g = min..max:
    u32 g   u32 n
    n × { u32 klen, u32 vlen, u8 key[klen], u8 val[vlen] }
```

The groups hold 11 / 89 / 22 / 1 entries; the last byte ends exactly at EOF [E].

- **key** = `fe_EncodeLexKey(lower(parts joined by '-'))`, sorted by unsigned bytes within each group (binary search) [D][E].
- **g** = total number of hyphen-separated parts across the consumed tokens. `dict_CountHyphenParts` `0x1000c9c0` counts letters, `'` and `-` only; any other char gives 0 ("not a candidate").
- **Matching** (`dict_MatchHyphenatedExcept`): tokens are appended one at a time with `"-%s"` (`0x10077890`) until the part count reaches `min_parts` or exceeds `max_parts`. At each count in range it tries a lookup. If a word is marked `'X'` (`WA+0x24`; accented input) and the plain key misses, it retries with the prefix `"h'%s"` (`0x10077888`). That is why accented loanwords are stored as `h'expose`, `h'touche`, `h'resume`.
- **val** = raw phone codes (1..0x45) with marks `c`/`d`. It is **not** a codebook payload [D][E]. `dict_ApplyExceptPron` `0x1000ca50` pours it into consecutive morph entries, moving to the next part's entry at each `'d'` once the current part's syllable budget (`FUN_1000ca30`) is used.

Examples: `a-priori EY2 d P R AY0 AO1 R AY0`, `faux-pas F OW1 d P AA1`, `h'canon K AE1 N Y AH0 N`, `toysrus T OY1 Z c AA1 R c AH1 S`.

---

## 7. `*.txt2` tables

Loader `fe_LoadTxt2Table` `0x100033e0` (via `FUN_10010300`). Every byte is stored **+10 mod 256**. Decoded:

```
"dbf"  line₁ "\r\n"  line₂ "\r\n" … lineₙ "\r\n"  "\0"  count (ASCII, NUL-padded to 10)  "dbf"
line = key              (mode 'L' = 0x4c: key-only set)
     | key "|" value    (mode 'M' = 0x4d: map)
```

The trailer count matches the line count in all 9 files [E].

| File | Rows | Mode | Content |
|---|---:|---|---|
| `abbrc_sort` | 330 | M | abbreviation → action (1 spell letters, 2 expand) |
| `abbrh_sort` | 55 | M | honorific abbreviations |
| `abbrt_sort` | 42 | M | title abbreviations |
| `chc_sort` | 2,581 | M | consonant-cluster → 4-bit syllabification flags |
| `citya_sort` | 117 | M | city abbreviations |
| `sbdw_sort` | 1,067 | M | sentence-boundary word classes |
| `streeta_sort` | 347 | M | street-type abbreviations (`ALY|Alley`) |
| `streetf_sort` | 211 | L | street full names |
| `wab` | 113 | L | function words (`about`, `across`, …) |

---

## 8. Front-end use of the lexicon (call graph)

```
fe_TagAndPronounce 0x1000e2f0
 └─ FUN_1000d190 (per token; appends '.', '?', '!', ',', '-' to the token text for token
    │             classes 2/3/4/5,0xc/0xb when the word is < 29 chars)
    ├─ FUN_1000d640   split token into candidate spellings (the candidate-spelling list)
    ├─ FUN_1000fd20   casing class of the spelling: 0 none, 1 lower-only, 2/3 contains upper
    ├─ dict_LookupLexicon 0x10003a70  → fe_EncodeLexKey → fe_HashDictLookup(…, cache, emb)
    └─ dict_FillMorphFromPayload 0x1000d450 → dict_DecodeLexPayload 0x10003c50
 dict_CopyMorphsToWordArray 0x1000ea20 → FUN_100068b0 (POS tagger) when count ≥ 2
 dict_MatchHyphenatedExcept 0x10008dc0 → dict_ExceptDictFind → dict_ApplyExceptPron
 FUN_10007520 (morphology) / fe_LetterToSound 0x10002f10 for flags-only and unknown words
```

Mode-7 dump source tags observed [E]:
- **`G`**: stored lexicon pronunciation.
- **`AT`, `A`, `R`, `GR`**: morphology / LTS variants (`R` = derived from a root).
- **`S`**: special or spelled.

The two-letter tag is printed by `FUN_10018020` from the WA source byte.

---

## 9. Verification

| Check | Result |
|---|---|
| Every emb key → `slot()` == its own index, `hashcont` passes | **228,591 / 228,591** |
| Every tpp key → own slot, `hashcont` passes | **31,550 / 31,550** |
| Decoded spelling re-packs to the stored key bytes | 228,591 / 228,591, 31,550 / 31,550 |
| `hashparams_*` length = header + scramble + tab | exact (both) |
| `exceptdict` parse consumes the file exactly; groups sorted | yes |
| `*.txt2` trailer counts = line counts | 9 / 9 |
| Engine mode-7 phones vs `vtdict` for 400 random single-pron words (seed 1) | **400 / 400 identical, all source `G`** |
| Engine on 100 random flags-only words | 0 × `G`; `AT` 46, `A` 31, `R` 17, `S` 5, `GR` 1 |

Verification harness: a small 32-bit host (not part of this repository, built with VS 2022 x86) that drives `VT_TextToPreprocessInfoFile_ENG(text, outfile, mode=7, spk=1, -1, -1, -1, -1, 0, 0)` directly and cross-checks the output against the decoded lexicon.

---

## 10. Verification tooling

A standalone Python reference implementation (lookup, decode, `verify`/`dump` CLI, plus separate
exceptdict and `*.txt2` readers and the codebook/digraph/`TOLOWER`/char-class extraction script) and
a small 32-bit engine cross-check host were used to produce and verify the tables above. Full TSV
dumps were generated from them: the decoded lexicon (228,591 rows: slot, word, flags, class, n,
entries (`[POS] phones`), payload hex), the TPP dictionary (31,550 rows), `exceptdict` (123 rows),
the nine `*.txt2` tables, and the 256-entry phone codebook. None of this tooling or its output is
part of this repository.

---

## 11. Open questions

1. **Flag bits 0x10/0x20/0x40/0x80.** The routing to morph `+0x66..+0x6c` is known [D]; the consumers are not traced, so the meanings in §4.1 are sample-based [I].
2. **POS indices 23–27, 29–36.** Only 10–22, 28 and 37–42 are pinned by homograph data; the Penn-alphabetical fit for the rest is [I].
3. **TPP letters.** `G<n>` class ids need the tagger's transition tables to name. The digit semantics of `A`–`E` (per-part flags) are [I].
4. **Punctuation-suffixed lookups.** `FUN_1000d190` appends `. ? ! , -` for some token classes, yet no stored key ends in punctuation. Presumably `FUN_1000d640` strips or re-splits them before lookup; not traced.
5. **Type-1 phash** (`dict_JenkinsChecksum`) is decoded [D] but unused by these files, so it is untested.
