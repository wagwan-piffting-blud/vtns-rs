# Linguistic front end (WA words to SS word/phone records)

This stage takes the per-sentence WA produced by the text front end ([FRONTEND_TEXT.md](FRONTEND_TEXT.md)) and
produces the SS word records, phone strings, phrase breaks and phone contexts that prosody and unit selection
consume ([PROSODY.md](PROSODY.md), [UNITSEL.md](UNITSEL.md)). Entry point `fe_LinguisticAnalysis` (0x1003e240).
Order: `fe_TagAndPronounce` (lookup, copy, morphology, wh flag, function words), the merge loop,
`fe_BuildWordPhoneRecords`, `fe_AssignPhraseBreaks`, `fe_BuildPhoneContexts`. Lexicon and LTS formats are in
[LEXICON.md](LEXICON.md). Spanish has its own implementation ([VIOLETA.md](VIOLETA.md)). Code: `src/ling/`,
`src/lexicon.rs`; tests `tests/feling_parity.rs`.

## 1. State

Every stage is bit-exact on the captured corpus (730 sentences).

| Stage (engine function) | Rust | Parity |
|---|---|---|
| Token lookup + split `FUN_1000d190` / `FUN_1000d640`, FillMorph 0x1000d450 | `ling/lookup.rs` | 730/730 (`lookup_out`) |
| `dict_CopyMorphsToWordArray` 0x1000ea20 + homograph tagger `FUN_100068b0` (rules `FUN_100049b0`, fallback `FUN_10004300`, article `FUN_10003ff0`, `poly.tree3` vote) | `ling/analysis.rs`, `ling/tagger.rs` | 730/730 (`copy_out`) |
| LTS `fe_LetterToSound` 0x10002f10, spell `FUN_10009cd0`, hyphen LTS `FUN_10003110` | `ling/lts.rs` | 3154/3154 lts calls, 866/866 spell calls |
| Morphology `FUN_10007520` and callees | `ling/morph.rs` (`morph_stage`) | 730/730 (flags included) |
| Function words `FUN_1000dfc0` | `ling/funcwords.rs` (`funcwords_stage`) | 730/730; TPP field extraction 220,850/220,850 (31,550 keys x 7 letters, `tpp_field_equivalence`) |
| wh flag `FUN_1000e990` / `FUN_1000e570` | `funcwords::wh_flag`/`wh_scan` (`wh_stage`) | 730/730 |
| `fe_LinguisticAnalysis` merge | `ling/merge.rs` (`merge_stage`) | 2154/2154 sentences (full `wa_out` bytes + wh flag) |
| `fe_BuildWordPhoneRecords` 0x10016c90 | `ling/words.rs` | 730/730 (`words_out`) |
| `fe_AssignPhraseBreaks` 0x100130e0 (syllabifier, phrase table, engbi CART) | `ling/breaks.rs`, `ling/syllab.rs` | 730/730 (`breaks_out`) |
| `fe_BuildPhoneContexts` 0x10017510 (+ allophones `FUN_10017100`) | `ling/context.rs` | 730/730 (`ctx_out`) |
| Chain `wa_in` -> `tag_and_pronounce` -> `merge` -> words/breaks/contexts | `chain_stage` | 730/730 each |

Run: `cargo test --release --test feling_parity -- --nocapture`. `chain_stage` builds `Wa`/`Ws` from `wa_in`
alone; `record_stages` feeds the builders the captured `wa_out` to prove each in isolation. The tests need the
captured ground truth (713 JSON files, not in this repository) and skip when absent.

## 2. `fe_TagAndPronounce` (0x1000e2f0)

```c
int tag_and_pronounce(Ws *ws, Wa *wa, int n, short mode, int lex) {
    if (n == 0) { ws->morph_count = 0; return 0; }
    ws->flags = 0;  ws->morph_count = 0;
    lookup_result = token_lookup(ws, wa, n, mode);                   // FUN_1000d190
    if (lookup_result < 0) return -1;
    if (ws->morph_count > 0) dict_CopyMorphsToWordArray(ws, wa, n);
    if (mode != 0) morphology(ws, wa);                               // FUN_10007520
    wh_flag(&ws->morph_count);                                       // FUN_1000e990
    funcwords_result = mark_function_words(wa, n, lex);              // FUN_1000dfc0
    if (funcwords_result < 0) return -1;
    return lookup_result;
}
```

A diff of the WA before and after `fe_LinguisticAnalysis` mixes five functions: `war::TAG_A`/`TAG_B` are set by
function words, not by the merge. `analysis::tag_and_pronounce` composes `tag_and_lookup`, `morph::morphology`,
`funcwords::wh_flag` and `funcwords::mark_function_words` in this order.

## 3. Structures

Engine offsets live in `src/ling/mod.rs` (`war`, `wr`, `syl`, `mr` modules) and `lookup.rs::cand`.

- WA record: 0x94 bytes ([FRONTEND_TEXT.md](FRONTEND_TEXT.md) section 2). Fields used here: +0x14/+0x18
  sub-token start/len, +0x1c flags (s16), +0x22 class char, +0x23 `U_FLAG` (`'U'` user-dictionary alias,
  `'Y'`/`'S'` other markers; a dual-purpose byte), +0x24 case mode, +0x25/+0x26 function-word class and
  `wab.txt2` index+1, +0x2c token class (0xb = `-`), +0x30 forced POS (0xff), +0x52 phones, `war::NMORPH` +0x10.
- WS: candidates 0x554 each; morph count s16 at `ws+0x429a2`, wh flag `ws+0x429a4`; morph `i` at
  `ws+0x429a6+i*0x70`; lexicon context `ws+0x48128`.
- Morph (0x70 bytes): +0 u16 flags, +2 token, +4 cat, +6 class char, +7 spelling, +0x25 pron, +0x66 accent `X`,
  +0x68 B40, +0x6a B10, +0x6c B20, +0x6e B80.
- SS word record 0x3c0 bytes, syllable record 0x1e bytes. POS tag names: 0x100779b0, 7-byte stride, 48 tags
  (`tables::POS_NAMES`).
- Syllable records are written by `FUN_10012c70` -> `FUN_10013c00` -> `FUN_10013f30`/`FUN_10013da0`: b0 nucleus
  stress, b1 shape (0 V, 1 CV, 2 VC, 3 CVC, 4 no nucleus), b2 position 1/2/3; the onset/nucleus/coda array is at
  W+0x36a. The legal-onset list (`ONSETS`, from 0x10079a4c) has typos (`S TR`, `S KR`, `SH `) that are
  reproduced: no word-internal 3-consonant onset is legal.

## 4. Morphology `FUN_10007520(ws, wa_rec0)`

About half of all corpus words pass through it (mode-7 source tags: G 2634, AT 1792, A 415, R 252, E 235, S 115,
GR 60, RAT 28). Per morph `i`:

1. flags = pron nonempty ? 1 : 0.
2. `FUN_100091b0` ('U' token); it may advance `i`.
3. Continue only if (WA forced POS == 0xff or pron empty) and (`U_FLAG == 'U'` or WA phones empty).
4. If `91b0` returned 0:
   1. `dict_MatchHyphenatedExcept` 0x10008dc0; on a match `i += consumed-1` and skip the post-rules.
   2. `FUN_10009030(morph, class, U, casemode)`; when it returns 0 and casemode is `'A'`, try LTS.
   3. `FUN_100086c0(sp, ws+0x429a2, i, NULL, 0)`: non-zero means spell.
   4. Otherwise, if pron is empty: `hy` = (`i >= 1` and the previous morph's WA token class is 0xb). If B80 is
      set: LTS (hyphen LTS `FUN_10003110` if the word's own spelling contains `-`), flags |= 0x40 on success.
      Else run the chain `9dc0 -> a140 -> b4d0 -> c040 -> b800 -> c3a0 -> c710 -> LTS`; if all fail, class = `E`
      and spell.
5. Post-rules (`LAB_1000792a`): "a" (EY -> AH0 / 0x1e; sentence-initial "A" before a word that is not
   `L_CONJ`/`L_BE`, or lowercase "a" before a capitalized word, reads 7; "a" + "'s" reads 0x1e); "use"/"close"
   (S -> Z via `verb_context`); "UP" (case-sensitive) before a/the; number + "minute"; "bow" (`L_BOW` window 5);
   "lead" (`L_LEAD` window 10, only when the pron is IY1); "mouth" (`L_DET` right window 2, only when pron[2] is
   DH and `i > 1`); "read" (IY1 -> EH1: `L_BE`/`L_HAVE` 1-2 words back or "not" 2 back, "he"/"she" one back,
   "I"/`L_OBJPRON` one back plus an `L_TIME` hit, an `L_WH` word one back, "get"/"got" one or two back but not
   "gets" one back, "to" before).
6. Final pass: if the previous spelling is "the"/"de" and the current pron starts with a vowel code other than
   `C`, the previous pron becomes `[p0, 0x26]` and flags |= 8.

Trust the disassembly over the decompile: VC6 reuses parameter stack slots (Ghidra's C for `FUN_1000a140` is
wrong in several places). Many engine buffers are C strings modified in place; model the same buffer across a
chain because failed calls leave data that later calls test (`a140`, `c040`).

| Function | Behaviour |
|---|---|
| `FUN_1000a140` suffix stripper (`suffix_strip`, `find_root`) | Minimum lengths: liness > 8; ance/ness/ment/ship/less > 6; ed/er > 4; ers/est/ing/ist/ful > 5; ly/s > 4. Root lookup: `FUN_1000cb30` (lexicon first pron, else LTS when B80) and `FUN_1000cbe0` (pron for POS 0x25 = VB); doubled-consonant, silent-e and i -> y fallbacks. Phone suffix by the root's last phone: ed T / IH0 D / D (unvoiced set {2a,35,37,20,22,38,3a,14}; {39,15} gets 23 15); s S / IH0 Z / Z (unvoiced {20,2a,35,39,3a}; sibilant {14,29,37,38,44,45}); ing 23 2e; er 1a; ers 1a 44 (or +44 when "...er" found); est 23 37 39 (07 37 39 after i -> y); ist 23 37 39; ful 20 07 2b; ly 26 -> 07 then +2b 26; ance 07 2d 37; ness 2d 23 37; ment 2c 07 2d 39; ship 38 25 35; less 2b 23 37; liness 2d 23 37 (root "...ly") or 2b 26 2d 23 37. Success: flags \|= 8. `s_suffix_root`: es/sibilant/ch/sh/consonant+o cases pick different stems; the plain strip-final-s path rejects the match when the root's last phone is already a sibilant ("importss" -> "imports" rejected, falls to LTS). `"ers"` tries `stem(1)`, `stem(2)`, `stem(3)` in that order, first lexicon hit wins, all with suffix ER Z ("farers" -> "fare" + rs; `stem(2)` alone breaks 11 other words). Quirks: the est branch flag byte is the high byte of the spelling pointer when a root is found early (0x02 for the captured WS, so != 3 and gives IH0); on the b404 path a failure leaves the root pron in the output buffer (`FUN_1000c040`'s tail returns 1 whenever `out[0] != 0`). |
| `FUN_1000b4d0` | British "ise" -> "ize" retry: isations/isation/isingly/isably/isable/ising/ised/ises/ise (min lengths 9/8/8/7/7/6/5/5/4); `cb30` lookup; flags \|= 9; sets out[0] = 0 at entry. |
| `FUN_1000c040` | Clitics 'n / 'ee / 'er / 'ers inside a word (strstr lower, then UPPER). The matched pattern must reach the end of the word, else the match fails ("Fish'n'chips" falls to plain LTS). Root from the lexicon (flags \|= 1), `86c0(..., &decoded, 1)` -> spell, or `9dc0`/`a140`/LTS/spell. Append: 'n N or AH0 N (N if the last phone is a vowel, or second-last vowel and last 0x36); 'ee 26; 'er 1a; 'ers 1a 44. Tail: `out[0] != 0` -> flags \|= 8, return 1. |
| `FUN_1000b800` | Clitic morph (same token as the previous). First looks up the concatenated spellings; if found the pron goes on the first morph, later morphs are emptied, flags \|= 8. Otherwise appends by the previous morph's last two phones: 's Z / S / IH0 Z; n't N T or AH0 N T; 've V or AH0 V (0x41); 'em AH0 M; 're 0x36 or 1a; 'll L or AH0 L; 'd T / IH0 D / D unless the previous word is in the subject-pronoun list 0x100786f0 or `L_WH`, then D or AH0 D; 'm M or AH0 M. If nothing matches it writes pron = "" (reload the buffer after a failed call). |
| `FUN_1000c3a0` + `FUN_1000c860` + `FUN_10008cc0` camelCase | `8cc0` mixed-case (any case change; false for Mac/Mc prefixes, letters-only check). `c860` splits before Upper+lower after Upper, or before Upper after lower, and handles 's. Per part: lexicon (len >= 2, `cb30`, same `ws+0x48128` context as the whole-word lookup), else B80 LTS, else `86c0` spell, else `a140`, else 's root, else LTS/spell. Parts joined with `d` (100), cap 0x40. Returns 1 only if a part was found by lexicon/`a140`/'s; success sets flags 0x18. A single-part mixed-case word writes out[0] = 0 and returns 0. Words that already have a whole-word lexicon payload ("GoodNight", "JavaScript", "YouTube", "FedEx", "iPhone") never reach it; "PowerPoint" and "LinkedIn" do. |
| `FUN_1000c710` | Word ending in `'`. For "in'": try "ing"/"inG" and turn a final NG (0x2e) into N (0x2d), flags \|= 9. Else strip the apostrophe: `cb30` (9) / LTS (8) / spell (8). |
| `FUN_10009dc0` (`hyphen_dot_split`) | Parts split at `-`/`.`. Single letter: "a" -> AH0 (7) only between dashes (or first before a dash after a 0xb token), else spelled. Longer parts: a known E/A class (`FUN_10003b10`, payload flag byte & 0x0c) with no lowercase is spelled; caps spell/abbrev lexicon entries are spelled; else `cb30` (flags \|= 1), `a140`/`c710`/`c040`/`c3a0`/LTS/spell. "the"/"de" before a vowel-initial part turns the previous part's last phone into IY (0x26). Parts joined with `d`, cap 0x3f; flags \|= 8. |
| `FUN_100086c0` (`should_spell`) | Requires all-upper letters (`FUN_10010010`). Class `E` -> 1; class `A` -> A-path; pron or B80 -> 0; no vowels (`FUN_100100d0`) -> 1; len > 2 and `lts_valid` -> 0; B40/B20 -> 1. Then neighbour words are checked for no-lowercase (left/right scans of spellings starting with a letter), the `L_NOSPELL` list (case-sensitive) and "Lexus" + "IS". Finally B10 -> 0; len < 3 -> 1; else `!lts_valid`. With the 5th argument set and a decoded entry (camelCase splitter): len 1 or class `E` returns 1 unless "Wi". |
| `FUN_100091b0` ('U' tokens) | Mode `Y`: pron = WA preset phones, flags \|= 4, skip the token's later morphs. Mode `S`: spell the part before the sub-token. Sub-token clitic (WA+0x14/+0x18): 's, 've/'VE, 'em/'EM, 're/'RE, 'll/'LL, 'd, 'm, 'n, 'ee, 'er, 'ers as in `b800`; anything else: `cb30`/LTS/spell of the text after the apostrophe appended after a `d`; flags \|= 8. |
| `FUN_10009030` (`case_mode`) | `S` -> spell. `C` with U `I`/`M`/`E`: spell for class E/A, else keep the pron (flags \|= 1), else B80 LTS, else (`I`: LTS) spell. |
| `dict_MatchHyphenatedExcept` 0x10008dc0 / `dict_ExceptDictFind` / `dict_ApplyExceptPron` 0x1000ca50 / `dict_CountHyphenParts` 0x1000c9c0 / `FUN_1000ca30` | exceptdict (`ExceptDict` in `src/lexicon.rs`). Join morph spellings with `-`, longest match wins; for accent `X` retry with "h'". The value is poured into consecutive morphs at `d` boundaries with flags = 2. |
| `FUN_1000cc60` (`morph_list_window`) | Word-list window search over morph spellings (modes `L`/`R`/`B`; `B` halves the window and searches both directions into one array). Returns the list index. Distinct from `tagger::phrase_in_window`. |
| `FUN_10008580` / `FUN_10008550` | `verb_context` for use/close (`let`, modal pairs via `FUN_10008440` = `tagger::modal_pair`, `do`, `to`; also tests morph0/morph1 for a modal pair) / number word (`L_NUMWORD`). |

`lts::spell` (`FUN_10009cd0`): the `0x64` join separator is decided only from a letter's look-ahead to the next
separator-then-letter run, never when the loop visits the separator ("J.R.R" -> jay 64 ar 64 ar); a
separator-containing spelling gets flags 0x08 in addition to 0x20. `cb30`/`cbe0` are pure (no flag side
effect). `u_token` mode `S` and the sub-token clitic list are
unexercised: `U_FLAG == 'U'` is only assigned by the `symbols` normalizer's apostrophe continuation of a word
whose class is already 0x19 (two sites, `FUN_100544f0` 0x10054ca4 and 0x10054de0, which also set the second
origin byte to `'Y'`), and class 0x19 is only produced by the `userdict` normalizer (`FUN_1005deb0`,
`src/text/norm.rs::userdict`). A user dictionary with an `a`-type alias followed by a possessive reaches it;
the shipped `userdict_eng.csv` has none (`userdict_eng_fixed.csv` and `national.csv` do).

Other tables and gotchas:
- `D5E` is shared: `D5E[TOLOWER_S[c]]` = letter is a vowel; `D94 = D5E+27` = phone is a vowel (codes 1..0x45);
  `F5A = D5E+254` = LTS code -> phone.
- `FUN_1001c2c0` is stricmp through TOLOWER with an unsigned index; inline 2-byte compare loops in the decompile
  are case-sensitive strcmp ("UP", the `'ve`/`'VE` pairs). `FUN_100560a0` mode 0x53 is a case-sensitive bsearch;
  a negative count means a linear search.
- A `0x2Xu` pron offset lifted from an ESI-relative asm dump may be off by 4 (the "use"/"close" rule was).

## 5. Function words and the wh flag

**`mark_function_words` (`FUN_1000dfc0`)** over the WA records:
- `get_tpp_field` = `dict_GetTppField` 0x1003a7b0: find the field starting with `letter` (payload first char or
  after a `" <letter>"` marker), cut at the next space ([LEXICON.md](LEXICON.md) section 5). Implemented directly
  on `lex.tpp`.
- `parse_tpp_num` = `FUN_10064645`/`FUN_100645ba`: leading decimal digits as a byte (G-class id, up to 124).
- Word lookup `dict_GetTppField(..., 'G', 'I')` -> WA+0x25; then `wab.txt2` index+1 (`Txt2::find`,
  `FUN_100035d0`) -> WA+0x26. `apply_class` (`FUN_1000e0c0`) sets WA+0x20 = 1 (word-sized) on `[i, end)`, then
  WA+0x25/+0x26 on `[i, end]`; the only caller passes `end = 0`, which its `0 < end` guard turns into "word `i`".
- `FUN_1000e160` (dash join): joins word `i` with up to 5 following class-`A` records (adjacent:
  `next.start - prev.end == 1`), trying `dict_GetTppField(joined, 'F', 'I')` after each append and keeping the
  longest hit; it skips zero-width `dash` placeholder tokens (class `S`) without counting them against the cap.

**`wh_flag` (`FUN_1000e990`)**: bail unless the last morph has `mr::CAT == 3` (`?`), then `wh_scan` at position
0 and, for each later morph with `CAT != 0`, at position+1; stop on the first `0x50` (flag = 1) or `0x4e` (flag
stays 0). `wh_scan` (`FUN_1000e570`) returns `0x50` found, `0x4e` veto, `0x44` try the next position; once the
current position is an `L_WHWORD` every remaining return is `0x50` unless a recursive call returns `0x4e`.
Stride is 0x70 per morph. Word lists:

| List | Address (count at) | Content | Used by |
|---|---|---|---|
| `L_NEG` | 0x10078038 | negations | `FUN_1000e4e0` |
| `L_MODAL` | 0x10078044 | modals | `FUN_1000e3a0` |
| `L_WHPREP` | 0x10078140 (0x1007824c) | 64 prepositions/subordinators | `FUN_1000e540`, gates recursion |
| `L_WHCONJ` | 0x100780e0 (0x10078244) | and, both, but, either, neither, nor, or | `FUN_1000e4b0` |
| `L_WHWORD` | 0x100780fc (0x10078248) | how, what(ever), when(ce/ever), where(by/ever), which(ever), who(ever), whom, whose(ever), why (17) | `FUN_1000e510`, the wh-word test |
| `L_WHBE` | 0x100780c8 (0x10078240) | 's, am, are, is, was, were | `FUN_1000e430`, excludes a leading `'s` |

## 6. Merge (`fe_LinguisticAnalysis` 0x1003e240, `ling/merge.rs`)

After `fe_TagAndPronounce`, each WA record absorbs the morphs of its token. The "same token" continuation path
addresses fields through a pointer one record ahead (`record - 0x34 + X`).
- `war::CLASS_CHAR` (0x22) = the first morph's class char only.
- `war::FLAGS` (0x1c, s16) = bitwise OR of every merged morph's flags word.
- `war::PHONES` (0x52) = the first morph's pron, unless `U_FLAG != 'U'` and `CASE_MODE == 'Y'` (VTML preset
  pronunciation): then the field keeps `wa_in`'s value. Every later merged morph's non-empty pron is `strcat`ed.
- `war::FORCED_POS` (0x30) is reset to 0xff once per record, followed by one 0xff byte per merged morph, capped at
  5 (a 6th same-token morph is dropped entirely; the cap is untested, no capture has 4+ morphs per token).
- `war::NMORPH` (0x10) = merged morph count.
- Marker branch (`0xa2 0xfe` VTML marker): a same-token morph whose spelling starts with the marker cuts
  `war::TEXT` at its `strstr` hit and subtracts 2 from `war::LEN` once per record; it is not merged; later
  same-token spellings are `strcat`ed onto `war::TEXT`. No capture reaches it (34 probes; the normalizers split
  around the marker).
- At the end, if `ws.n_morphs() > 0`: WA header short 1 (`Wa::wh()`) = `ws.wh`. Return 1 when the number of
  records started equals `wa.n()`, else a failure code.

## 7. Word records, breaks, contexts

- **`fe_BuildWordPhoneRecords`** (`words::build_word_phone_records(.., pos: &TextPos)`): copies WA words into SS
  word records. It does **not** write `wr::START`/`wr::END`; `fe_NextSentence` (0x10022dc0) does, after the ling
  stage, walking the TIB request cursor (`TIB+4`) over WA and SS records:
  `wr::START = cursor + war::START`; `wr::END = cursor + war::END - 1` (or `cursor + war::START` for an empty
  word); after the sentence `TIB+4 += WA+4` (`Wa::consumed`). `war::START/END` restart at 0 each sentence, the
  SS values accumulate over the request. Then they pass through the processed-to-original maps built by
  `fe_InitialTextProcess`: unless the fullwidth+VTML flag (byte at `DAT_100a0460 + 0x20424`, no capture takes
  it, not implemented) is set, `wr::START = map_start[cursor + war::START]` (`TIB+0x47794`),
  `wr::END = map_end[cursor + war::END - 1]` (`TIB+0x47798`), after clamping; identity for plain text.
  `TextPos { cursor, map_start, map_end }` carries this (`TextPos::none()` for a plain single sentence). The
  values matter: the waveform sync ring (`FUN_1002c530`), the lip-sync log and `SYNCINFO` events report them, and
  `breaks::predict_break_index` reads them to detect a character gap between adjacent words (zero START/END makes
  the engbi CART never fire). The `offset_maps` capture event (once per request, before any `wa_in`) feeds the test.
- **Breaks** (`breaks.rs`, `syllab.rs`): syllabifier, phrase table, `engbi.tree3` CART; section 3 for the syllable
  layout. **Contexts** (`context.rs`): 7-byte contexts per phone, allophone pass `FUN_10017100` (codes
  0x46-0x53).
- Tests that preload a whole captured struct cannot prove the stage fills every field it claims; `chain_stage`
  and the zeroing in `record_stages`/`compare_words` exist for that reason.

## 8. File map

| Path | Content |
|---|---|
| `src/ling/mod.rs` | `Wa`/`Ss`/`Ws` byte structs with engine offsets (`war`, `wr`, `syl`, `mr`), tagged pointers, `Tib`, `WordStrings` |
| `src/ling/lookup.rs` | `FUN_1000d190`, `FUN_1000d640`, FillMorph, `pron_for_pos` (`FUN_10003f10`), `casing` (`FUN_1000fd20`) |
| `src/ling/tagger.rs` | homograph tagger (rules, fallback, `poly.tree3` votes), string helpers |
| `src/ling/analysis.rs` | `copy_morphs`, `tag_and_lookup`, `tag_and_pronounce` |
| `src/ling/morph.rs`, `funcwords.rs`, `merge.rs` | sections 4, 5, 6 |
| `src/ling/lts.rs` | LTS, spell, cluster validity (`chc_sort`), hyphen LTS |
| `src/ling/words.rs`, `breaks.rs`, `syllab.rs`, `context.rs` | section 7 |
| `src/ling/tables.rs` | generated from `vt_eng.dll` by a table extractor; never edit |
| `src/ling/lay.rs`, `phtabs.rs`, `tables09.rs` | 2009 layout and tables (section 9) |
| `src/lexicon.rs` | hash dicts (emb, tpp), key encoding, payload decode, txt2, `ExceptDict`, `Lexicon::open` / `open_era` |
| `src/engine09.rs` | 2009 sentence builder |

## 9. Builds

| Build | Differences |
|---|---|
| 2014 English | everything above |
| Julie09 / Paul09 | `Lay::of(era)` selects the 2009 SS layout: word record 0x300 B with a 5-byte context label (2014 has 7 bytes); unit records are a struct of arrays; word offsets (Julie09/Paul09) n_phones +0x2e4, dur +0x0a, ctx +0x90, edge +0x1d5, flag +0x2fc, brk +0x2fd |
| Violeta09 / Violeta 2013 | [VIOLETA.md](VIOLETA.md) |

2009 English additions: break CART with 13 features, threshold 400 (Julie09) / 500 (Paul09)
(`0x100129d0`); words/breaks (`0x10016690`, `0x10012b30`); contexts `build_phone_contexts09` (`0x10016ed0`, a new
function); suffix stripper `suffix_strip09` (`0x10009da0`); 2009 tagger rules and morph post-rules; Paul09 uses
`.tree2` files for the lexicon trees (`poly`, `engbi`, `atmt` loaded by `Lexicon::open_era`, `sbd` by
`Fe::load_era`, all through `src/tree2.rs`; prosody trees in [PROSODY.md](PROSODY.md)). The engine runs it through `src/engine09.rs`. The 2009 text
front end is in [FRONTEND_TEXT.md](FRONTEND_TEXT.md).

## 10. Verification

| Test | Result |
|---|---|
| `feling_parity` (2014) | table in section 1 |
| `feling_julie09_parity`, `feling_paul09_parity` | all 10 stages 100%, asserted: words/breaks/contexts 1554/1554, morph 1554, merge 4542, lts 8807 (Paul09 8790), spell 2080 (Paul09 2085) |
| `engine09_parity` | ling words, durations, tones, F0 and unitsel contexts exact for both builds |

The 2014 `feling_parity` tests do not yet `assert!` 100%. Expected output lines: `lookup 730/730`, `copy`,
`words`, `breaks`, `contexts`, `funcwords`, `wh` 730/730; `morph 730/730`, `merge 2154/2154`, `chain-*` 730/730;
`tpp_field_equivalence: 31550 keys, 220850 pairs, 0 mismatches`. (An older note listed `lts 3148/3148`,
`spell 865/865`; the table above is the newer count.)

Method that closed the last gaps: a Frida deep-trace of named functions (arguments, return values, buffers)
beats re-reading the decompile; run the whole corpus after each fix, not only the line being chased. Harder
mixed corpus lines (`suffix_004`, `tail_001`) found real bugs in code that passed easy lines.

## 11. Open items

- `u_token` mode `S` and the sub-token clitic list are unexercised (needs a user dictionary alias, section 4).
- The fullwidth+VTML double indirection in the `wr::START/END` remap is not implemented.
- The 6-morph cap in the merge is unverified.

## 12. History

- 2026-09-23: morphology, `morph_028` ("farers") and `camel_001` closed; function words and wh flag bit-exact.
- 2026-09-23: merge ported (2154/2154); `chain_stage` 730/730 on words, breaks and contexts.
- 2026-09-23: suffix fallbacks corpus-verified without code changes; `spell` double-join and B80 `hy` bugs fixed.
- 2026-09-23: `clitic_inword` made strict (clitic must end the word).
- 2026-09-24: `wr::START/END` traced to `fe_NextSentence` (cursor plus offset maps); `U_FLAG 'U'` root cause found.
- 2026-10-05: Julie09/Paul09 ling 100% on all stages.
