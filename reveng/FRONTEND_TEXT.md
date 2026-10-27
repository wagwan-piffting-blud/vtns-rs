# Text front end (raw text to normalization word array)

This stage turns the request text into the per-sentence **WA** (normalization word array) that the linguistic
front end consumes ([FRONTEND_LING.md](FRONTEND_LING.md)). It has two parts: `fe_InitialTextProcess` (once per
request: filters, VTML tags, offset maps) and `fe_NormalizeTokens` (per sentence: tokenizer plus a chain of 21
normalizers that spell out numbers, dates, units, addresses and so on). Engine: `vt_eng.dll` 3.11.7.1 (Paul
M16), image base `0x10000000`, VC6, `__cdecl`. Addresses are VAs. Port: `src/text/`; parity test
`tests/fetext_parity.rs`. Spanish has its own implementation, see [VIOLETA.md](VIOLETA.md).

**State: 100%.** `fetext_parity`: 1534/1534 lines, 1807/1807 sentences, 14118/14118 WA records (full raw record
bytes). `initial_parity`: 1494/1494 distinct inputs. From raw text `speak_parity` is 1522/1524 WAVs; the 2 misses
are `"unclosed"` (first phone AH0 vs AH1, a morphology issue in [FRONTEND_LING.md](FRONTEND_LING.md), not text).

Rules that held for every stage: port from the decompile, not from behaviour; reproduce engine quirks; extract
tables with the generator (`src/text/tables.rs`, `tables_address.rs`, never hand-edit); one normalizer per file.
Evidence tags: **[D]** decompile/disassembly, **[E]** engine capture, **[I]** inferred.

## 1. Pipeline

```
VT_TextToBuffer_ENG  FUN_100200c0
  fe_InitialTextProcess 0x1001c990        once per request
    fe_InjectDemoNag 0x1001ce10           demo nag (disabled: VT_CheckLicense_ENG stub returns 0)
    char offset maps TIB+0x47790/94/98    original <-> processed positions
    fe_RunTextFilterPass 0x1001d250 x2
      fe_MapFullwidthChars 0x1001d370
      fe_ExtractVtmlTags 0x1001d5d0       tags become "<\xa1\xddN>" / "</\xa1\xdd>" markers
  per sentence: fe_NextSentence 0x10022dc0
    fe_ResolveSentencePSV 0x10022850
    fe_AnalyzeSentence 0x1003d350
      FUN_1003e210: WA reset (0xe74 dwords from WA+0x14; tag-mode flag survives only when 1)
      every record pause (+0xc) := -1
      fe_NormalizeTokens 0x1003d3d0       (FUN_1003e070 instead while the user dictionary loader resolves aliases)
      fe_LinguisticAnalysis 0x1003e240    see FRONTEND_LING.md
```

`fe_NextSentence` repeats `fe_AnalyzeSentence` from the cursor: `consumed == 0` ends the text (checked before the
words); `n == 0` advances the cursor and retries; otherwise the WA goes to the ling stage and the cursor advances
by `consumed`.

### 1.1 `fe_InitialTextProcess` (`src/text/initial.rs`)

Two filter passes; a 31-entry tag table at `0x1007e888` (longest case-insensitive prefix); a backtracking matcher
for the 25 Spencer regexes compiled at load (`0x1007eb88`, used only as yes/no); handlers for pitch/speed/volume/
`vt_*` (`FUN_10030500`), emotion, break/pause (`FUN_1002ed30`/`FUN_1002f8e0`), mark, partofsp (`FUN_10046df0`),
phoneme with x-cmu (`FUN_10046320`) and x-sapi (`FUN_10046070`), say-as (characters, `vxml:time`, phone,
currency converters), skip, sub (`FUN_10031650`); channel 0-2 open/close pairing. Output: processed text, both
offset maps, 8 span channels, marks.

**Spans to words** (`engine::apply_tag_spans`, `pro_ApplyTagSpansToWords` 0x10022970, called by
`fe_NextSentence` on processed positions before the maps apply): level channels 0/1/2/7 set per-word pitch/
speed/volume/kind7 from the entry in force at the word start; pause (3) accumulates entries between words and
entries after the last word become the final pause (`TIB+0x122440`, `SentenceInput::final_pause`); break (4)
takes entries after a word end. Pause and break cursors persist across sentences.

## 2. The WA (`TIB+0x1312bc`, 0x39e4 + tail bytes)

| Off | Type | Meaning |
|---|---|---|
| +0x00 | s16 | `n`: records in use (max 100) |
| +0x02 | s16 | cleared per sentence |
| +0x04 | i32 | `consumed`: bytes of processed text used; also the running "end of last word" |
| +0x08 | i32 | parenthesis length limit (`VT_SetParenthesisCharNumber_ENG`, 1 in captures); a `(`/`[` whose closer is at least this far away marks a prosodic break instead of being skipped [D][E] |
| +0x0c | s16 | tag-mode flag; non-zero routes each position through `FUN_1005dd20` first |
| +0x10 | ptr | common DB (lexicon probe uses `+0x48128` from it) |
| +0x14 + i*0x94 | rec | records 0..99 |
| +0x39e8 | ptr | user dictionary (`FUN_1005e330` result) or 0 |

**Record (0x94 bytes)**, offsets relative to the record; names match `ling::war`.

| Off | Type | Written by | Meaning |
|---|---|---|---|
| +0x00 / +0x04 / +0x08 | i32 | EmitWord | start, end (sentence-relative, processed text), text length |
| +0x0c | i32 | driver | pause ms, -1 = none |
| +0x14 / +0x18 | i32 | symbols (class 0x19) | sub-token (clitic) offset and length; read by morphology `FUN_100091b0` |
| +0x1e | s16 | numbers | 1 = plural number ("feet"/"inches" agreement); 0x1f = address component |
| +0x23 | u8 | EmitWord `a` | origin tag: `A` text, `D` digits, `S` symbol, `U` spelled |
| +0x24 | u8 | EmitWord `b` | `D` default, `X` Latin-1-folded, `S`, `Y` |
| +0x28 | u32 | EmitWord | class: producing normalizer (section 4) |
| +0x2c | i32 | punct/symbols | following punctuation: 1 soft break, 2 `.`, 3 `?`, 4 `!`, 5 `,`, 0xc `, too/either` |
| +0x30 | u8 | EmitWord | 0xff (POS byte for VTML partofsp) |
| +0x34 | char[] | EmitWord | word text (unbounded copy) |
| +0x52 | char[] | ling | phone string |

`fe_EmitWord` (0x10044fd0): empty text returns 1 and does nothing; `n > 99` returns 0; otherwise fills
start/end/len/+0x23/+0x24/+0x28, sets +0x30 = 0xff, copies the text, `n++`, `consumed = end`. `FUN_100451e0`
(`wa.emit_split`) applies it once per space/tab-separated part; all parts share one span. Use `emit_split` for
anything that may contain a space.

## 3. Tokenizer `fe_Tokenize` (0x1005a350)

`fe_Tokenize(tok, text, off, mode, userdict) -> consumed`. 2,119 decompiled lines, ported label for label in
`tokenize.rs`. Token record (0x44 bytes): `[0]` whitespace count, `[1]` one newline seen, `[2]` start, `[3]` end,
`[4]` length, `[5]` consumed, `+0x18` s16 length at last Latin-1 fold (mode 8), `+0x1a` s16 user-dictionary hit,
`[7]` class, `[8..]` text (36 bytes).

| Class | Meaning |
|---:|---|
| 1 | word |
| 2 | number |
| 3 | symbol/punct run |
| 4 / 5 | tag-open / tag-close marker |
| 6 | raw high byte |
| 7 | sentence punctuation `.?!;` (single) |
| 8 | paragraph break, or a run of `!!`/`??` |
| 9 | end of text |

**Character classes** (`DAT_1007e188`) [E]: `0x80` upper, `0x40` lower, `0x20` hex, `0x10` digit, `0x08` punct,
`0x04` space. Indexed with a signed char almost everywhere, so bytes >= 0x80 are class 0; `FUN_10062f50` indexes
unsigned and classifies Latin-1 letters. `DAT_1007e388` is tolower, `DAT_1007e688` toupper.

**CP1252 folding** (switch at 0x1005a578): Latin-1 letters fold to ASCII (`é`->`e`, `æ`->`ae`, `ß`->`ss`,
`Þ`->`th`, `Œ`->`oe`, `Š`->`sh`); typographic quotes to `'`/`"`; `–` to `-`, `—` to `--`. Symbols become
**EUC-KR** pairs (the engine is Korean-built): `°` `A1 C6`, `€` `A2 E6`, `±` `A1 BE`, `×` `A1 BF`, `½` `A8 F6`.
Quirk: single-character replacements use `strncpy(token, s, 1)`, which writes at index 0, not at the end.
Letters accumulate spelled-out phones (`FUN_10062f50`, table `0x10077e58`); a token is cut once
`sum(1+phones) > 0x40` or length exceeds 28.

**Mode-specific joining** after a letter run (mode = 4th argument):

| Mode | Joins |
|---|---|
| 0 / default | `X.Y` letters (`e.g`), `int'` |
| 8 (word normalizer) | hyphen compounds and `.`-joined letters unless the user dictionary claims the continuation; contractions (`don't`, `he'll`, `'s`) via `FUN_100621f0` |
| 0x17 (userdict boundary) | hyphen and contractions |
| 0x11 / 0x15 | `.` / `-` inside words, `A1B` codes, `United States` |
| 0x19 | `A1` |
| 0x16 | `-` and `'` |
| 0xc / 0x1a | `'` |
| 0x1b | `/` or `.` after short words or `Mrs`/`S.&P`/`Sec`/`ca`, `OK!`, `A.&B`, `A&B`, `A/B` |
| 0xf | `A.M.`/`P.M.` |
| 5 | `pa'anga` |
| 0x1d | letters followed by a digit |
| 9 | `://` joined |
| 0x13 / 2 | one letter / decimals |

Numbers: digit runs join `,ddd` groups (not after a leading `0` or a `.`) and one decimal `.d`. Ordinal suffixes
join (`1st 2nd 3rd 4th..9th 0th 11th 12th 13th`; `3Rd` excluded as possible "Road", `11st` refused). `401K`/
`401(K)` special-cased in modes 0 and 0x1b. Punctuation: runs of one symbol form class 3; `.?!;` runs become
class 7 (single) or 8 (`!!`, `??`); `...` is class 3; `\` is its own token; tag markers become classes 4/5.
EUC-KR: full-width letters/digits are collected and folded (`FUN_10061b00` modes 1/2); other symbol pairs map
to ASCII punctuation (mode 3 table, `chars.rs`).

## 4. `fe_NormalizeTokens` (0x1003d3d0): the normalizer chain

The driver tokenizes each position in mode 0. Every normalizer gets `(WA, &tok, text, off)` and returns `> 0`
(bytes consumed), `0` (end the sentence here) or `< 0` (not mine: the driver restores `n`, sets `consumed` to the
end of record `n-1`, clears record `n` (zeroed, pause -1); records beyond `n` keep what a failed attempt wrote).
[E] Every normalizer from `dispatch` to `numword` receives the token `punct_runs` left (mode 1); `compound`
receives the one `symbols` left (mode 0x12).

| # | VA | Name | Cont. | Class (+0x28) | Notes |
|---:|---|---|---|---|---|
| 0 | 0x1005deb0 | userdict (token class 1-3) | d772 | 0x19, `U`/`Y` | `P` and `A` entries |
| 1 | 0x10051a00 | punct_runs | own | | writes mode 1; sentence boundaries (5.1) |
| 2 | 0x1004d990 | dispatch | own | | VTML content-tag dispatcher (token class 4 + `_strstr` delimiter), ~20 letter-keyed handlers; `norm_vtml.rs` |
| 3 | 0x1003d290 | symbols_cp | d772 | | (c) "copyright", paragraph sign "paragraph" (class 6); takes every lone `0xa9` |
| 4 | 0x1005c890 | url | own | | `norm_url.rs`; piece tokenizer `FUN_1005d9d0`, protocol, port, IP check, 11-pattern table, path tail |
| 5 | 0x100344e0 | currency | d772 | 3 | 5.4 |
| 6 | 0x10036940 | date | d772 | 4 (weekday) | + 8 helpers; `norm_date.rs` |
| 7 | 0x10058490 | time | d772 | 5 | 5.5 |
| 8 | 0x100570c0 | phone | d772 | 0x1b | 5.9 |
| 9 | 0x10052e80 | digits_a | d772 | 0x1e | 5.8 |
| 10 | 0x10053440 | digits_b | d772 | 0xc | 5.8 |
| 11 | 0x1003e4a0 | units | d772 | 0xf | 5.3 |
| 12 | 0x10033760 | address | own | 0x12, flag 0x1f | 5.7; sentence-mark merge rule for class 0x12 |
| 13 | 0x1003c1a0 | fraction | dcea | 0xd | 5.10 |
| 14 | 0x1003aad0 | math | dcea | 0x1d | dead in 2014 (see Builds) |
| 15 | 0x100420f0 | numexpr | dcea | 0x17 | 5.6 |
| 16 | 0x10031d40 | n15 | dcea | 0x16 | 5.7 |
| 17 | 0x10040ef0 | numword | dcea | 0x1c | 5.2 |
| 18 | 0x100544f0 | symbols (+ sentence mark) | dcea | 0x13 / 0x15 | writes 0x12 |
| 19 | 0x10063780 | compound | dcea | 0x18 | 5.11 |
| 20 | 0x1003a880 | word | own | 0x18 | writes mode 8; `< 0` retries (the engine would loop) |
| - | 0x1005dd20 | tagmode_url (tag mode only) | own | | `norm_url.rs` |

**Continuations.** `d772`: `r == 0` returns; with no sentence mark, or a mark at or after `n`, the driver
advances; otherwise it ends the sentence at the mark (consumed = start of the first word after the mark,
relative to `off`). `dcea`: the same logic, applied after fraction..compound.

**Sentence marking.** When `punct_runs` returns `> 0` (sentence-final `.`, paragraph break, end of text) and no
mark is set, the mark cursor becomes `n` and the driver looks one token further; if that token produces words,
the sentence ends at the mark (`LAB_1003dee7`).

**Wins on real NWS products** (`--cov`): word 3047, punct_runs 848, numword 466, numexpr 219, address 198,
symbols 189, units 122, date 84, n15 39, digits_b 15, compound 5, time 2, url 1.

## 5. Normalizers

### 5.1 `punct_runs` (0x10051a00), mode 1

- Class 8 (paragraph break, `!!`, `??`): sets the previous word's +0x2c to 3 (`?`), 4 (`!`) or 2 (other), only
  if it is still 0 and the word does not end in `.?!;`, and returns `consumed`; with no words yet returns
  `-consumed`.
- Single `.?!;` or `...` goes to `FUN_10051cc0` (decision P/N). P marks the previous word and returns `consumed`;
  N returns `-consumed` (0 for a bare `.` token at the very start).
- Class 9 returns `consumed`, or 1 when `consumed` is 0.

`FUN_10051cc0` builds six token slots `{ws, len, class, text}`: 0-2 the last three tokens before the punctuation
(`FUN_10063300` copies <= 3 words back, `FUN_10063660` re-tokenizes in mode 1), 3 the punctuation, 4-5 the next
two tokens. `FUN_10052000` applies hand rules in order, then the tree:

1. `?!;` glued to the previous word: P.  2. `.` followed by tab/newline/CR: P.  3. Nothing follows: P.
4. Next token is a paragraph break: N.  5. Two or more spaces before the next token: P.
6. Initials/abbreviations (case patterns around `.`): N or P.  7. `word.digits`: N.  8. `X. Y` single letters: N.
9. `. and`: N.  10. Quote followed by a capital: P.  11. `neither/nor/so ... do/am I.`: P.
12. Otherwise the `sbd.tree3` CART over 16 short features (`FUN_10052520`); a leaf with non-zero low byte means P.

| Features | Content |
|---|---|
| 0-6 | previous token: length, case class (`FUN_10052710`: 1 all-caps, 2 lower, 3 capitalized, 4 number, 5 other), abbreviation-list bits (`abbrh`/`abbrt`/`abbrc`, `FUN_10052d00`), inner `.`, punctuation class (`FUN_100527c0`) |
| 7-13 | the same for the next token |
| 14 | next-word category (`FUN_100528b0`: `sbdw` list value -> table `0x10081358`; digits 9, caps 3, sentence punctuation 0x11, comma 0xb, symbol classes 0xc-0xf, suffix rules for `-ly/-ing/-ed`) |
| 15 | the punctuation's own class |

### 5.2 `numword` (0x10040ef0) and the number core

Class 2 tokens: ordinals (`FUN_100601e0`/`FUN_10061450`); malformed numbers digit by digit (`FUN_10060fc0`) or as
dotted groups (`FUN_10060e50`); decades `1990s`/`'80s`/`80's` -> `nineteen ninety's`; `6'` feet, `5"` inches
(plural by `FUN_10062040`); scores `3-2`/`3:2` in a `game/odds/point/ratio/run/score` context (`FUN_10041f30`);
codes read in pairs after a capitalized word (`FUN_100630d0`/`FUN_10062590`); three-digit phone groups in a
`call/phone` context (`FUN_100580e0`); five-digit ZIPs after an address component; fallback cardinal
`FUN_100611c0` (four-digit numbers read as years). Unit end tests the unit token's `len == 0`, not `class == 0`
("6'" -> "six feet"). Class 1 tokens: Roman numerals I..XX (`FUN_10041df0`), except I/V/X unless after
`Chapter/Division/Part/Starcraft` or in `World War`, and `IV` after `the`.

`numwords.rs`:

| VA | Function | Behaviour |
|---|---|---|
| 0x1005fa60 | cardinal (`L` <= 999,999,999; `H` billions/trillions part) | words appended with `" %s"`; scale words strcat'ed with a leading space |
| 0x100611c0 | number string | commas stripped; a four-digit value > 999 reads as a year unless `plain`; 10-15 digits: high part `H` + low 9 digits; > 15 digits digit by digit; decimal part digit by digit with "point" |
| 0x10039d50 | year | `1905` nineteen oh five, `1900` nineteen hundred, `2005` two thousand five (formatted `0%d` to avoid recursion), `1990` nineteen ninety; `FUN_10039410` = `year_value` |
| 0x10060fc0 | digit reader | `0` says **"oh"** when the zero flag is clear, **"zero"** when set (Ghidra's pseudo-C swaps the two branches; the `JZ` at `0x10061052` jumps to the "oh" load `s(0x1009c740)`); non-digits append an empty word. Confirmed live: `FUN_10060fc0(buf, "220", 0, 0)` = "two two oh" |
| 0x10060e50 | dotted groups | `1.2.3` -> "one point two point three" |
| 0x10060380 | ordinals | `first`..`nineteenth`, `twentieth`.., `hundredth`..`trillionth` |
| 0x10062590 / 0x100629d0 | pair reader | `4500` forty five hundred, `4501` forty five oh one, `12000` twelve thousand, `05` oh five |
| 0x10061890 / 0x10061970 / 0x10062040 / 0x100601e0 | predicates | numeric / well-formed (reject leading zero unless len <= 2, comma groups of 3, one decimal point) / "not exactly one" / numeral + matching ordinal suffix |

### 5.3 `units` (0x1003e4a0 + 0x10040250), class 0xf

`norm_units.rs`. `nums_parse` (`FUN_10040250`) finds the number the unit belongs to: a number token at the
current position, or the previous WA record (so `units` fires on "60 mph" after `numword` emitted "60");
returns `-1` (use previous record; tag `$` for a leading currency-ish symbol) or the bytes it consumed.

**Slot scan.** Up to five slots, tokenized forward in mode 1 and tagged: `E` exact hit in `DICT_E`; `P` prefixed
unit `DICT_P` ("kilo-"); `C` context unit `DICT_C` (needs `context_unit` to confirm, guards "3 C"); `/`, `O`
operators ("per", "over"); `W` power prefix ("square", "cubic"); `s` suffix power ("^2"); phrase-table hits
(`phrase_lookup`/`phrase_unit`, e.g. "miles per hour"; distinct from the userdict alias of the same text).

**`finish`.** The accumulated tag string (`"E"`, `"WE"`, `"E/E"`) is matched against six patterns at
`0x10080508` (`ptabn`); `mode` (`$` vs `A`) picks the pattern range `(1,3)`/`(2,1..5)`. A match at index 1 or 3
re-runs `context_unit` to set the `f2c` flag; `validate` accepts or rejects; `emit_units` writes class `0xf`
records plus spelled unit words (singular/plural via `Rec5a`). Words that are also currencies ("pound", "yen",
"euro") never reach `units` because `currency` (order 5) claims them first; do not remove them from `DICT_E`.

### 5.4 `currency` (0x100344e0), class 3

`norm_currency.rs`, a full transliteration: validator `FUN_10036160` (every case; `CN` needs a code without
lowercase, "Try 2" is not Turkish lira), next-token context `FUN_10036370` (patterns 7/8/0x15-0x17), prefix
exclusion `FUN_10036780`, list/range extenders `FUN_100364b0`/`FUN_100365f0`, emitters `FUN_10034f30`/
`FUN_10035950`/`FUN_10035030`. All 25 patterns emit; 90-line dedicated corpus 90/90. `FUN_10056e50`'s multi-word
branch is unreachable.

**Slot scan.** Up to six tokens (mode 5), at most one tag each, tried in this order:

| Tag | Matches | Table |
|---|---|---|
| `$` | class 3/6 token | `SYMBOL_TAB` `0x10082350` (rec5a, 13): `$` dollar/-s, `£` pound/-s, `¥` yen, 2-byte ¥ yen |
| `S` | first slot only, class 1 | `PREFIX_TAB` `0x100827e8` (rec30, 28): `US` "U.S.", `NZ`, `C` "Canadian" |
| `F`/`f` | class 1, case-insensitive / fallback case-sensitive | `NAME_TAB` `0x10087150` (rec66, 102) / `NAME_TAB2` `0x100899f8` (5) |
| `C` | class 1, case-insensitive | `CODE_TAB` `0x10082d28` (rec66, 171): ISO 4217 |
| `U` | class 1, case-insensitive | `SCALE_TAB` `0x10082140` (rec30, 11): `b/bn/billion`, `m/mn/million`, `t/tn/trillion`, `hundred`, `thousand` (no `k`: "$10K" -> "ten dollars" + "K") |
| `N` | class-2 numeric | `numwords::numeric` |

**Patterns** (25, `0x100800a4`/`0x10080108`): `$NUS $SNU $NS $NU $SN $N $ S$NU S$N CNU CN NU($S) NU(S$) NU(C)
NU$S NUS$ NUC NUF N($S) N(S$) N(C) NS$ N$ NC NF`.

**Table layout.** `rec66` (stride 0x66) holds up to four NUL-terminated fields: key @0, plural suffix @0xf
("-s", "-es", a full irregular word, empty = invariant: `yen`, `colon` "-es", `koruna` "korun"), subunit name
@0x32 and subunit plural @0x41 (`dollar`: "cent"/"-s"), and on `CODE_TAB` only a qualifier @0x41 (`USD` "U.S.",
`EUR` "European", `AED` "United Arab Emirates").

**Emission** (`emit_currency`). The number is always spoken before the name, whatever the source order ("USD
100" -> "one hundred U.S. dollars").
- No `N` slot: qualifier then singular name ("dollar $ sign" -> "dollar").
- `U` slot: `numwords::number(plain=1)`, scale word, qualifier (if a `C` slot), name always plural.
- Otherwise: `numwords::split_decimal` (`FUN_100620e0`) splits whole/cents. Empty or zero whole part ("$.50",
  "$0.99"): no dollars word. Else whole cardinal, qualifier, name (plural iff whole != 1). Cents: "and" (only if
  a whole part was spoken), cents cardinal, subunit (`subunit()` looks the resolved name up in `NAME_TAB`,
  falling back to dollar's cent/-s).
- The name word's tag is `S` (0x53) when a `$` slot is present ("$5" -> "dollars" `SD`), `A` otherwise ("100
  dollars" `AD`). Qualifier comes only from a `C` slot: "$5" has none, "USD 100" has "U.S.".

### 5.5 `time` (0x10058490), mode 4, class 5

`norm_time.rs`; helpers `FUN_10058f80` (pattern validator), `FUN_10058b60` (accept check and emitter),
`FUN_10059310` (hour/minute range predicate; first numeral `FUN_10059310(text,1,0)` needs `0 < strlen < 3`, so
"220" never reaches `time`), `FUN_100590f0` (phrase-context gate, not called).

**Slot scan.** Up to six mode-4 tokens, each a role: `Hour`/`Minute`/`Second` (1-2 digit numeral; role by number
of `:` consumed so far, 0/1/2), `Ampm` (table `0x1008a508`, rec30, ci: `a.m am p.m pm`), `Zone` (table
`0x1008a5c8`, 22 entries, case-sensitive: `CST C.S.T CT`, `EST ET`, `GMT`, `KST`, `MST MT`, `PST PT`; no daylight
entries, so `EDT/CDT/PDT/HST/AST/AKDT` stay plain words), `Unit` (`DAT_1008a9e8` = `h hr hrs min`, only when
`len < 3`). A `:` is absorbed (up to 2) only after an Hour/Minute slot. A second bare numeral at the same colon
depth is rejected (else "1 2 3 4 5 ..." chains into a fake `N:N:N`).

**Patterns** (18, `&PTR_DAT_1009c1e0`): `N:N:NmD N:N:ND N:N:Nm N:N:N N:NUmD N:NUD N:NUm N:NmD N:NU N:ND N:Nm
N:N NUmD NUD NUm NmD NU Nm`. All accepted except bare `N:N` (index 11), which the engine gates through
`FUN_100590f0`; declining it matches captures ("06:00", "1:05 AKDT" decline).

**Emission** (`emit_time`): hour is a plain cardinal; minute is present only when non-zero ("6:00:30" has no
minute words), pair-read (`numwords::pairs`); seconds "and" + cardinal + "second(s)" (singular iff < 2); a
duration unit gives "hour(s)"/"minute(s)" by first letter, pluralized by the hour slot ("3 hr" -> "three
hours"); "o'clock" only for the `H:00:SS` shape. Hour/minute/second share one record span (hour start to last
component end); am/pm, zone and unit each get their own token span. The zone name is split at spaces into
one record per word ("Eastern" "Standard" "Time", all on the zone's span). Tags: am/pm word `AS`, other words
`AD`, numerals `DD`.

**2009 builds.** Pattern table Paul09 `0x100952b0` (18: no `NmD`, adds `N`), Julie09 `0x100975e4` (19: adds
`N`). Bare `N` goes through the same `FUN_100590f0` gate as `N:N` and is declined the same way. Paul09's zone
table (`0x10083b08`) has 15 entries: no `MST`/`M.S.T`/`m.s.t` and no two-letter zones. So in Paul09, "5 PM EST"
leaves `EST` as a plain word, while "5:45 PM EST" expands it.

### 5.6 `numexpr` (0x100420f0), class 0x17

`norm_numexpr.rs`, full transliteration. The shared token is a single punctuation character (`tok.len == 1 &&
tok.class == 3`); anything else declines. Each emit is one `FUN_100451e0` call ending at `off + consumed`
("plus minus" is one string split into two records). 39-line operator corpus 39/39.

- **`=`**: "equals" (`0x100803ec`).
- **`+`**: "plus" (`0x10080460`), extended to "plus" "minus" when a bare `-` is glued after. No phone logic:
  "+1 402 555 1234" is claimed earlier by `phone`.
- **`*`**: "times" (`0x10080450`) needs a number `b`, `a` absent or `b.ws > 0`, a number after with `tok.ws ==
  after.ws` (the operator's own spacing, not the neighbour's, 0 or 1). After a unit record (class 0xf ending at
  `b.end`), "times" when a number and unit follow ("6 ft * 8 ft"). An unmatched `*` is consumed silently.
- **`x`/`×`** and **`/`** read one previous word (`FUN_10063300(...,1)` + `FUN_10063510`). `x` is "x", or "by"
  between a unit record and a number + unit. The disassembly (0x1004266d..0x10042684) passes the correct start
  of the second token where Ghidra shows a bogus pointer.
- **`/`**: bare `word/word` glued both sides ("N/A") is consumed silently; number before + unit after with
  `tok.ws == after.ws < 2` -> "per <unit>" (consumes `/` and the unit; "60/hr" -> "per hour"); a unit-context
  case (`off - b.end == ws`, number glued after, unit after that -> "per", only `/` consumed: "3 km/2 hr");
  default "slash". Unit test `FUN_10040ab0` over `DAT_100a7a5c/60/64` (E, P, C) + `FUN_10040b60` (phrase table
  `0x10090490`, plural stem, title case: "5/miles" -> "per miles") + the `h/hr/hrs/min` table of `norm_time.rs`.
- **`-`** (`dash_word`/`number_dash`), in order:
  1. Next word not a number: decline silently if a number is glued before (`tok.ws < 2`; the second `-` in
     "MIZ009-240800- Gogebic"); also decline for word-dash-word ("Gogebic- Including", leading "- Small craft").
  2. Number before and after (`tok.ws < 2`): "to", "dash" or case 3. Both must pass `numwords::well_formed`; if
     any class-0x16 record (`n15` UGC code) exists **anywhere earlier in the sentence**, every later
     number-dash-number says "dash" ("INZ050-058-059-": the second dash too); a 3+-part chain ("3-4-5") says
     "dash", checked forward past `after` and backward (`chained_before`: the token before `b` is a `-` glued
     to `b`). Otherwise "to" for ascending integers ("555-1234" is "to"), or before "points"/"run"; "K-12
     school" -> "through".
  3. Nothing number-like before: "minus", except when another operator (`+ * / =`) is glued before ("+/- 5"
     declines; a run of dots still says "minus") or an all-caps word is glued before ("I-80", "US-6": left for
     `address`).
  Also handled: currency-symbol and glyph neighbours, units-record test, `e` exponent, `,;%$#~` gaps, spaced-word
  pause, `-+*/=xX` chains, `N-N/N`, the `+-*/=` and `,:'()<>".?!;` spacing tests. An empty result still consumes
  the dash; a number after it then says "dash".

### 5.7 `address` (0x10033760) and `n15` (0x10031d40)

`address` tries four sub-recognizers in order; they share compass-direction tables (`0x10099078`/`0x100992b8`).

| Function | File | Role |
|---|---|---|
| `FUN_10053920` | `norm_address_csc.rs` | state/country/zip, validator `FUN_100541a0`, emitter `FUN_10054050` |
| `FUN_100337f0` | `norm_address.rs::po_box` | PO box |
| `FUN_10033b60` | `norm_address_city.rs` | city, validator `FUN_10034180`, emitter `FUN_10034110` |
| `FUN_10043810` | `norm_address_street.rs` | street number/name/type, `FUN_10044b30`/`FUN_10044d00`/`FUN_100447a0`/`FUN_100444e0`/`FUN_100445e0` |

**State/country/zip** (`city_state_country`): up to 5 tokens (mode 0x15) tagged `S` (state/province/country:
2-letter code, informal abbreviation, country phrase), `N` ("canada"/"USA"/"United States" after an `S`), `Z` (4-5
digit zip read by `numwords::pairs`, or Canadian FSA/LDU via `mixed_code`, `FUN_10062cd0`), `L` (`-`/`~` between
zips: "dash"/"to"). Patterns (8, `PTR_DAT_10081490`): `S SN SZ SNZ SZZ SNZZ SZLZ SNZLZ`, longest first. Bare `S`
only with flag 1 (case 7 of the validator), and the top-level dispatcher passes 0: "Nebraska", "NE", "Canada"
typed alone stay for `word`. Also: `,`/`.` absorbed; not from a full-name phrase and not before a capitalized
word ("Chicago, Illinois" keeps its comma); "Mass" + zip needs its period. Informal abbreviations
(`FUN_10054350` -> `FUN_10056960`): bisect over 57 0x30-byte records at `0x10098228` with
`strnicmp(record, word, strlen(word))`; a record continuing with a space matches following mode-0x15 tokens
(next position `pos + 1 + len`); a one-letter word from `DAT_1009c0ec` may match a record's ". " and consume the
period. "N. Mex", "S. Dak", "W. Va" read as states, "N. Dak" does not (the bisect lands on another record).

**PO box**: shapes in order: phrase table `0x10099070` ("Post Office Box"/"Postoffice Box", `FUN_100567c0`
multi-word bisect); single token `P.O.Box`/`P.O.B`/`POB` (ci) with an optional lone `.` absorbed into the span but
not the word; `P.O` + `Box` with optional `.`. All but the phrase match emit "P.O Box"; the phrase match emits
"Postoffice Box". A following all-digit token with `ws < 3` is read via `numwords::pairs`. Decompile pitfall: the
guard that looks like `ws` is the token's `udict` flag (+0x1a).

**City** (`FUN_10033b60`): the gate is a TPP dictionary lookup (`tppdict_eng`, field `'A'+n-1`; slot expansions
joined with `-`), which is why "HAZARD", "MAX", "Mobile" read as addresses. Exclusions: following street word,
"EADS", Roman numeral, George/King/Martin/Luther + capitalized name, "George W". Every slot it emits gets
`+0x1e = 0x1f`; the last city record gets `+0x2c = 9` when the trailing state/zip recursion succeeds.

**Street** (`FUN_10043810`): follows the decompile's label chain (A19/A57/B61/BF5/C46/F21); slot tags collapse
runs of `4`; 30-pattern table `0x100806f4`; ends with `FUN_10033b60(..., 1)` for a trailing "City, ST ZIP".
Also the `I-80`/`US-6` highway reading. `FUN_100393b0` has the `FUN_100394c0` day-ordinal fallback shared with
`norm_date.rs`.

**`n15`** tries `norm_name::name` (`FUN_10059560`), `ugc_code` (`FUN_10031db0`), `abbr` (`FUN_10032180`) in
that order, each directly on `(fe, wa, t, p, off)`.
- *UGC code* (`norm_n15.rs::ugc_code`): letters-only word (`FUN_1000ffa0`) glued to a digit run ("MIZ009"). Letters
  tag `D` if they contain a lowercase char, or a vowel with length > 2 (but `S` when length is 1); else `S`.
  Digits via `numwords::code` ("009" -> "nine"), or `ordinal_numeral_words`. Words get their own token spans.
  First check is the udict flag, not `ws`. Apostrophe + s/er/ers stops; uppercase ST/ND/RD/TH suffix splits.
- *Names* (`norm_name.rs`, validator `FUN_10059fc0`, emitter `FUN_10059ef0`): kinds T/N/I/Q/R/s over <= 5 tokens
  (mode 0xc), periods/one comma/"Ex-" dash absorbed, 27-pattern table `0x1009c298` by leading kind, lexicon flag
  0x40 decides N vs W. "Dr. Smith" -> lowercase "doctor"; "WIND PRES REMARKS" is `NTN`.
- *Abbreviations* (`FUN_10032180`): Mr/Mrs/Ms (+ "&"/"and"/"or" pairs), Vol./No./p./pp. + number (+ range), Dr
  (Drive vs Doctor by context), St (Saint vs Street), Ste, Co./Pty., Ctr/Pkwy, "w/", `401(K)`, then the
  expansion tables `0x100996c8` (ci) / `0x10099e78` (cs).

### 5.8 `digits_a` (0x10052e80) / `digits_b` (0x10053440)

`norm_digits.rs`. Two independent chain entries (order 9, 10; `fe_Tokenize` mode `0x1d` / `0xa`) that collect
number tokens joined by a bare `-` (`ws == 0`), build a pattern string of digit counts ("4-4-4-4") and on an
exact match read every group digit by digit (`plain=1`, skipping the year rule), emitting "dash" where the input
had one. Without them "1234 5678 9012 3456" would read "twelve thirty four ...". `digits_a` re-tokenizes;
`digits_b` reuses the shared token for slot 1.

- Group tests: `digits_a` 4-5 letters/digits (`FUN_100533f0`); `digits_b` 2/3/4/6/7 digits only ("0.00 0.55" is not
  an ID). Pattern tables are unsorted: `digits_a` 4 entries; `digits_b` `27, 2-7, 324, 3-2-4, 4444, 4-4-4-4, 67,
  6-7`. The engine indexes by first count digit + total token count; a linear scan gives the same answer.
- Validator `FUN_10053280` (index 0 checks 4 group slots, 1/3 five, 2 six; all-alpha rejects with no retry);
  emitters `FUN_100531c0`/`FUN_10053830` (0x140-byte slots, tag at +0x1a, flag +0x1c decides the +0x2c write);
  group reader `FUN_10045680` (userdict on the group text first, then all-alpha spelled / all-digit read / else
  the call fails with 0). Token caps 11/7.
- `+0x2c = 8` is set on a group's last digit word only when no dash follows it ("123-45-6789": only on the last
  word; "1234 5678 9012 3456": on every group's last word).

### 5.9 `phone` (0x100570c0), class 0x1b

`norm_phone.rs`. A run of number tokens joined by `- . / ( )` matched against a 21-entry shape table
(`PTR_s_N__N__N_N_1009c0f0`, indexed directly: `N-(N)-N-N`, `N(N)N-N`, `N-N-N-N` .. bare `N`), optionally
preceded by a label (`Rec30` `0x1008aa28`: `FAX` -> "fax", `PH`/`TEL` -> "telephone", plus 3 Korean entries)
with an optional glued `:`/`.`. `.` tokenizes as class 7, so "555.123.4567" is three numbers and two `.` tokens.
Every group is read digit by digit (`plain=1`), punctuation silent ("555-1234" -> "five five five one two three
four"). `+0x2c = 7` on every record except the last (each group's last digit word and the label).

Helpers: label entry `FUN_10057b20`, extension `FUN_10057e60`, validator `FUN_100578a0` (right-to-left group
lengths), emitter `FUN_100576a0`, group reader `FUN_100582a0` (0 is "zero"; N00/NN00 as hundreds/years), "+"
prefix. Gates found by whole-suite testing (the first version passed its own corpus and dropped overall parity
from 82.5% to 74.7%):
1. Every unlabeled match needs `phrases::phone_context` (`FUN_100580e0`, 5 words back and 5 tokens forward, tables
   `0x1008ab58`/`0x1008ab48`). The decompile shows it on 6 of 21 patterns; the port applies it to all. A label
   skips it.
2. A bare separator-free number (`N`/`NN`, indices 20/19) is never accepted unlabeled ("ext. 123" is a cardinal).
3. No group exceeds 4 digits (else `digits_b`'s "12-3456789" is stolen after "Call").
4. `N/N` (index 17): `groups[0]` 1-3 digits and the last group's text length in 9..=12, checked before the
   context flag, regardless of label (`FUN_100578a0` case 0x11, `FUN_10058350`). Ordinary fractions are never
   phones ("calls for 1/2 inch").

Open: one phone number too far from its "No." cue (beyond the 5-word window) next to a URL fragment is still
claimed here where the engine declines (unisolated).

### 5.10 `fraction` (0x1003c1a0), class 0xd, tags `DD`

`norm_fraction.rs`. Patterns (`&PTR_DAT_100804a8`, 4): `F` (glyph token), `NF` (number + glyph), `N/N`, `NN/N`
(mixed). Glyph table `0x10095a08` (12: EUC-KR eighths/thirds, Latin-1 `¼ ½ ¾` = 0xbc..0xbe), powers table
`0x10095c48` (7: "squared", "cubed", "to the first", "to the fourth", "to the N TH"). Validator `FUN_1003c760`:
bare `N/N` only for numerator 1-3 and denominator 2-4 unless context `FUN_1003cfb0` says so ("5/10" belongs to
`numexpr`). The engine works on a 0x140-byte scratch record through a negatively biased pointer (offset -0x4b
ints); the port reconstructs behaviour in a `Slot` array instead.

**Scan.** Up to 6 tokens in mode `0xd`: `N` (class 2, `well_formed`) or `/` (bare glued slash, `ws == 0`, only
after a slot). Stops at the first failure, then tries the full tag string and drops the last slot until
`N/N` (3) or `N N/N` (4) matches.

**Words** (`frac_words`, `FUN_1003cbb0`): numerator is a cardinal (`plain=0`, so a 4-digit numerator reads as a
year). Denominator 2/4 is hard-coded "half"/"halves" (`0x10078bc4`/`0x100804dc`), "quarter"/"quarters"
(`0x100804d4`/`0x100804c8`), singular iff the numerator is 1; other denominators are
`numwords::ordinal(d, 'L', 0)` plus "s" (`0x10077770`) when the numerator > 1 ("two thirds", "five eighths").
The decompile's "4th argument > 0" branch never fires.

**Spans.** Bare `N/N`: numerator and denominator share the whole `N/N` span. Mixed: the whole number and "and"
(`0x10077400`) share the whole number's span; numerator/denominator share the `N/N` span. `+0x1e = 1` goes on
the denominator record of mixed numbers only, so a following unit pluralizes ("1 1/2 inches"; bare "3/4 inch"
stays singular).

### 5.11 `symbols` (0x100544f0, mode 0x12) and `compound` (0x10063780)

**`symbols`**
- Comma: next word `too`/`either` (<= 1 space) -> previous +0x2c = 0xc, else 5.
- Two-byte EUC-KR symbols: four sorted 0x30-byte tables (`0x10095c48`, `0x10095e58`, `0x10095f18`, `0x10096878`)
  -> words ("to the first", "sub one").
- Quotes/apostrophes: possessive merge `Jones'`/`word's` into the previous record; `'tis`-style words (table
  `0x1009ade0`); quoted `!`/`?` read as words; prosodic break before words in `0x100780fc`.
- `<` `>` with optional `=`: "less than", "greater than or equal to" between numbers, else a prosodic mark.
- Brackets: skipped or marked using WA+8 as the length limit.
- Other: `.NET` "dot net", `~` between numbers "to", `@` "at", `&` "and", `#N` "number", `\` "back slash", `®`
  "registered trademark".
- (c): `FUN_10039410` and `FUN_10039d50` are wired into both call sites. EUC-KR `a8 cf` + a bare year (class 2,
  digits, ws < 3) -> "copyright" (`FUN_100451e0`) + year, consuming both; other pairs in the `a8`/`a9` `cd..e6`
  row emit one letter (`b1 + 0x74`, tags `S`/`S`, class 0x13). Corpus 32/32.

**`compound`** (two features, order 19, on the token `symbols` left):
- Apostrophe suffix: a class-1 word ending in lowercase `ers`/`er`/`s` (tried longest first) whose prefix is
  >= 2 chars, letters-or-hyphens (`FUN_10063740`) and has no lowercase (`FUN_1000ffd0`) gets an apostrophe before
  the suffix: "CDs" -> "CD's", "RVers" -> "RV'ers". Class 0x18, `emit` (one word).
- Single letter (`single_letter`, `FUN_10063780`): a/e/i/o/u/y (table `0x10077d5e` via tolower) spelled
  (`b='S'`) by bracket/case/number/closing-symbol context; bracket gate open set `<([{-'"`, `FUN_10063300`/
  `FUN_10063590`/`FUN_10063170`, e-commerce check against "e-"/"commerce". Two branches are dead (number-dash-letter
  shortcut; letter-glued digits "a2", always claimed by `n15`). 27-line corpus 27/27.

### 5.12 `word` (0x1003a880), mode 8

Non-word classes are consumed silently. Words are emitted `A`/`D` (`A`/`X` after Latin-1 folding), class 0x18.
Hyphenated compounds (`FUN_1003aa50`, `norm.rs::word`/`hyphen_kind`/`split_hyphen`): the whole word is looked up
in `engttsdict_emb` first, else split per hyphen part ("summer-like", "Tri-State", "mid-week"). `lex_has_prefix`
(`FUN_10010810`) is the 3-entry prefix table de/inter/re, not the lexicon. URL words: `FUN_100453a0`
(`norm.rs::url_words`, shared by `word`, `url`, `tagmode_url`). Splitter `FUN_100457b0`, emit-with-phones
`FUN_10045070` (`wa.rs`).

## 6. User dictionary (`data-common/userdict/userdict_eng.csv`)

Loader `fe_LoadUserDictCsv` 0x1005e330: CSV lines `"source","target","type"[,"legacy"]`, type `A` (alias) or `P`
(phones); keys trimmed, <= 49 bytes (`FUN_1005f2e0`); `[CI]` in the target puts the entry on the case-insensitive
**I** list, else the **S** list. The installed 2014 dictionary has one `P` entry ("miles an hour" `[CI]`); the 2009
Paul and Julie installs ship nine `A` entries ("voicetext" -> "voice text [CI]", "ABSlider" -> "<AB> Slider", ...).

**Aliases** (`userdict.rs::check_alias`, `alias_words`, `text::alias_phones`). `FUN_1005f3b0` validates the
target: trimmed; `[SKIP]` alone; else at most 10 blank-separated words of letters (a `-` between letters counts as
a letter, a `<TAG>` of letters may be open once), <= 0x1e letters a word, <= 0x41 bytes, an optional trailing
`[CI]` (cut off; selects the I list). Anything else (digits, punctuation, DBCS) drops the entry. The loader then
runs `fe_AnalyzeSentence` on the target in a temporary WA with the user dictionary's flag (`+8`) at 0, which
selects `FUN_1003e070`: words emitted `A`/`D` class 0x15, the letters of a `<...>` group `A`/`S`, a `[...]` group
skipped, any other token ends the pass. `fe_LinguisticAnalysis` follows (lookup, morphology, merge), and the stored
target is each record's phones (`+0x52`) joined by `\`, so "voicetext" emits two records, `V OY S` and
`T EH K S T`. Normal text still goes through `fe_NormalizeTokens` after loading (verified with the 2009
dictionaries on 2014, Paul09 and Julie09).

Index `FUN_1005f040`: exchange-sorted by key length, then key; per-length first/last indexes. Search
`FUN_1005e010` (S, `strncmp`) / `FUN_1005e1a0` (I, `strnicmp`): longest length first, bisect within a length; a
match must end on a token boundary (`FUN_1005f840`, re-tokenizes in mode 0x17, accepts `.`/`'`/`-` splits). The
tokenizer calls it on every token end to set the token's `+0x1a` flag.

## 7. Data files

| File | Loader | Use |
|---|---|---|
| `dict-eng/sbd.tree3` | `pro_LoadTree3` into `DAT_100a7a68` | sentence-boundary CART |
| `dict-eng/sbdw_sort.txt2` | `DAT_100a7a4c` (M, I) | next-word category values |
| `dict-eng/abbrh/abbrt/abbrc_sort.txt2` | `DAT_100a7a50/54/58` (M, I) | abbreviation classes (1 = exact case, 2 = any case) |
| `dict-eng/streeta/streetf/citya_sort.txt2` | `DAT_100a7a40/44/48` | address normalizer |
| `userdict/userdict_eng.csv` | `fe_LoadUserDictCsv` | section 6 |

`.txt2`: every byte +10, then `dbf` + CRLF lines `key|value` (M) or `key` (L) + NUL + count + `dbf`; bisected
(`FUN_100035d0`, `txt2.rs`). Engine tables come from a live-process snapshot of `.rdata`/`.data` (identical to
the file apart from IAT and load globals); a generator emits the referenced regions (`s(VA)`, `b(VA,n)`,
`rec30(VA,count)`, `ptab(VA,count)`) into `src/text/tables.rs`.

## 8. Builds

| Build | Differences |
|---|---|
| 2014 English (`vt_eng.dll`) | everything above |
| Violeta 2013 | separate implementation, [VIOLETA.md](VIOLETA.md) |
| Julie09 / Paul09 | same architecture, other addresses; table values differ |
| Violeta09 | [VIOLETA.md](VIOLETA.md) |

**2009 builds** (Paul09 `vt_eng_paul16.dll`, Julie09 `vt_eng_julie16.dll`). Mechanism: a thread-local era
(`text/cstr.rs::set_era`); `mem()` consults per-era override chunks keyed by the 2014 VA
(`text/tables_julie09.rs`, `tables_paul09.rs`, generated from Frida `.rdata`/`.data` snapshots), plus era branches
in about 12 normalizers, the chain order per era and `Fe::load_era`. Known differences:
- Chain: Julie09 swaps `digits_a`/`digits_b`, dispatch table has 18 entries; Paul09 has no `digits_a`.
- VTML: handler set `TAGS09` / `handle09` (open marker `a2 fe`, 5 channels, 29-tag table `0x1007a4e8`;
  partofsp/phoneme consume content, x-ntsampa verbatim); fullwidth map.
- `math` is **live** in 2009 (`norm_math::math09`; Julie09 `0x10037c80` + helpers); `*` reads "times" in symbols
  (Julie09 `0x10050660`). In 2014 `math` is dead: its gate `FUN_1003be50` needs a previous class-0x1d record and
  only `math` emits 0x1d (0 wins in 220 live calls).
- Date (Paul09 `FUN_10033b70`, Julie09 `FUN_10033d10`; validators `FUN_10034ff0`/`FUN_100351d0`; context
  `FUN_10035e30`/`FUN_10036050`): the context check runs only for patterns 7/0xc/0xd/0x11/0x15 and the hint is
  sticky 1; case 0xc has no length < 3 split, so "AP", "JA", "JE" read as months; case 0xd separators `/-.`
  (`0x100802a8`); case 0x11 has an extra clause.
- Abbreviations `norm_n15` (Paul09 `FUN_1002fc40`, Julie09 `FUN_1002fa10`): honorifics gated on "Mr"/"Ms" only
  ("mrs." falls to the tables); a matched-trigger failure returns -1; conjunctions "&"/"and" only; `table09`.
- Paul09 `norm_symbols` (`FUN_1004f790`): only `(` handled in that branch. Year words (Paul09 `FUN_10036d10`,
  Julie09 `FUN_10036ec0`): values >= 10000 read digit-pair-wise. Also era branches in street, city, PO box,
  currency, numword, names, phone label, tokenizer, digits, plural.
- Julie09 `norm_digits::digits_a` returns -1 for a group neither spelled nor digits ("401K-2345-..."): empirical,
  no decompile support.
- Paul09 captures show `]` where 2014 emits `\` in some break positions (stage not identified, unverified).

## 9. Verification

Harness (not in this repository): a host runs one request per corpus line; a Frida script records the processed
text, offset maps, tag stacks, marks and every WA at `fe_NormalizeTokens` exit as JSONL; coverage mode counts
normalizer calls and wins; call tracing logs every normalizer's attempt and return, deep tracing logs arguments
and buffers of named functions (this found the `FUN_10060fc0` "oh" bug and the `n15` owner of UGC codes).
Corpora: `nws_real.txt` (170 paragraphs from api.weather.gov), `synthetic.txt` (1,230 lines), plus dedicated
corpora per normalizer.

| Test | Result |
|---|---|
| `fetext_parity` | 1534/1534 lines, 1807/1807 sentences, 14118/14118 records |
| `initial_parity` | 1494/1494 |
| `speak_parity` | 1522/1524 WAVs (the 2 misses are in morphology); inputs must be encoded CP1252 as the capture sent them |
| `fetext_julie09_parity`, `fetext_paul09_parity` | 2245/2245, 2264/2264 lines |
| `initial_julie09_parity`, `initial_paul09_parity` | 2197/2197 each |
| `speak_old_parity` | Paul09 2741/2741, Julie09 2741/2741 (end to end with the rest of the stack) |

Dedicated corpora: operators 39/39, currency 90/90, copyright 32/32, compound 27/27, informal abbreviations
22/22, PO box 11/11, phone 9/9. Every change was checked against the previous full-suite mismatch list.

## 10. Open items

- Julie09 / Paul09: table constants in 2014-addressed modules were compared by "string appears anywhere in the
  2009 image", so other `s()` constants may differ on texts no corpus covers.
- `phone`: the far-cue case in 5.9.
- Not exercised by any corpus: `numexpr` "through" tail checks, a few date/ordinal `-` sub-cases.

## 11. History

- 2026-09-23: tokenizer, driver, punct_runs, SBD, word, symbols, numword, date, units, currency, time (21% -> 66% lines).
- 2026-09-23: `numwords::digits` zero-flag fix ("oh" vs "zero"), +29 lines in one change.
- 2026-09-23/24: n15, numexpr, PO box, digits_a/b, compound, phone, fraction, city/state (75% lines, 83% records).
- 2026-09-24: city, street, state/zip, names, VTML, url, abbreviations, full re-ports of fraction/phone/numexpr: 100%.
- 2026-09-24: `fe_InitialTextProcess` ported (1494/1494); spans-to-words wired; `speak_parity` 1522/1524.
- 2026-10-05: Julie09/Paul09 text front end exact (fetext 2245/2245, 2264/2264); end to end 100% after a 14-text date/abbreviation fix round.
- 2026-10-09: zone names split into one record per word (`emit_split`); "5:45 pm EST" had been one record
  "Eastern Standard Time" (heard as "e a s t e r n" in a user report). No corpus line had a zone after a time. Probe
  `corpus_fetext\timezones.txt` (148 lines) on 2014, Paul09, Julie09.
- 2026-10-15: user-dictionary aliases ported (validator `FUN_1005f3b0`, load-time normalizer `FUN_1003e070`, phones
  per word). The 2009 installs ship aliases; "Welcome to voicetext" had dropped the word. Probe
  `corpus_fetext\aliases.txt` (20 lines) byte-identical WAVs on Paul09, Julie09 and 2014 with the 2009 dictionary.
