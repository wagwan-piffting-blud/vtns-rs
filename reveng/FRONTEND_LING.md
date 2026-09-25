# vt_eng.dll linguistic front end: normalized words → SS word/phone records

## HANDOFF (2026-09-23, updated: 730/730 -- the linguistic front end is bit-exact on the whole corpus)

This section is for a fresh agent with none of the earlier context. Everything below it (sections 1+) is the start of the full reference; most of the detail currently lives in the Rust doc comments and in the project's own Ghidra decompile/disassembly dumps (not part of this repository).

**Every stage in the table below is 730/730 (or its own unit equivalent) as of this pass, `wr::START`/`wr::END` included.** Four rounds so far, all live-traced with a Frida-based deep-tracing harness (not part of this repository), not more asm reading:
1. `morph_028` "farers" and `camel_001` camelCase (§0.1).
2. `suffix_001`-`003`: `find_root`'s other 9 `FUN_1000a140` suffixes and `ise_retry`, corpus-verified with **zero code changes** (they'd been unfairly labelled "best-effort, likely buggy"). `suffix_004`, a deliberately-harder line mixing several features: 2 real, previously-uncaught bugs (`lts::spell`'s double-join on embedded punctuation, the B80-hyphen `hy` condition).
3. `wr::START`/`wr::END`: not written by `fe_BuildWordPhoneRecords` at all in the real engine -- `fe_NextSentence` writes them, a running request cursor plus a VTML/fullwidth offset-map remap, both now ported (`words::TextPos`). See the note below the table.
4. `tail_001`: `clitic_inword` was too permissive -- it matched a clitic pattern (`"'n"` etc.) anywhere in the word and silently discarded whatever came after it, instead of rejecting the match the way the real `FUN_1000c040` does when there's trailing text (`"Fish'n'chips"`, `"mac'n'cheese"`). `tail_002` corpus-verified `should_spell`'s class-`'A'` path with no changes needed.

Not exhaustive: `u_token`'s non-`'Y'` modes (mode `'S'`, the sub-token clitic list) are still genuinely unexercised -- every plain-text corpus line tried so far produces `war::U_FLAG != 'U'` for every morph, so the function bails before reaching the interesting code (see §0.1). 100% on the current corpus is a real milestone, not "done".

**`war::U_FLAG='U'` root cause, found 2026-09-24 (static capstone scan, no corpus line involved):** `fe_EmitWord` (`0x10044fd0`, text front end) writes its 5th argument to `war::U_FLAG` (`+0x23`) at emission time. A full-binary scan of every call to `fe_EmitWord` and its per-space/tab-part wrapper `FUN_100451e0` (246 call sites total, all of `.text`) found exactly **one** place that ever passes the literal `'U'` (`0x55`): two symmetric sites inside the `symbols` normalizer (`FUN_100544f0`, `0x10054ca4` and `0x10054de0`), both gated on `*(int*)(WA + prevWordIdx*0x94 + 0x28) == 0x19` -- i.e. the *immediately preceding* word's `war::CLASS` (producing-normalizer id, `+0x28`) already being `0x19`, which is an apostrophe/possessive continuation of a word the symbols normalizer itself just merged. Both matching sites also re-assign class `0x19` to the new word (self-perpetuating chain) and set the second `EmitWord` origin byte (`war::CASE_MODE`-adjacent `+0x24`) to `'Y'` -- confirms the `'Y'` in "`u_token`'s non-`'Y'` modes" is what this path always produces when it fires, and mode `'S'` is a different branch inside `u_token` gated on something else entirely (not yet identified).

The same exhaustive scan for who *first* assigns class `0x19` (246 call sites again, searching for a literal `push 0x19` as `fe_EmitWord`/`FUN_100451e0`'s 7th arg) found **only those same two self-referential sites** -- nothing else in `.text` ever emits a class-`0x19` word directly. Cross-checking `FRONTEND_TEXT.md`'s normalizer table (§4, "Emitted class (+0x28)" column) resolves the paradox: dispatch-order-0, **`userdict`** (`0x1005deb0`), is the row with `Emitted class = 0x19`, status **"lookup ported, emission not"** at the time this note was written. `userdict`'s own emission code (distinct from `fe_EmitWord`'s call sites scanned above, since its raw asm wasn't part of this scan) is the true seed. **Update:** `userdict`'s emission path is now ported — `src\text\norm.rs::userdict` (`FUN_1005deb0`) — so the reachability conclusion below is current, not blocked.

**Conclusion: `war::U_FLAG='U'` requires a user-dictionary alias (`'A'`-type) match, immediately followed by an apostrophe/possessive-looking suffix.** Per `README_TECHNICAL.md` §4, the *default-loaded* `userdict_eng.csv` has exactly one entry, a `'P'` (phones) type ("miles an hour"), and zero `'A'` (alias) entries -- but user dictionaries are user-supplied at runtime (`VT_LOAD_UserDict_ENG` loads any CSV in the loader format), and this install already ships two more, sitting right next to the default: `data-common\userdict\userdict_eng_fixed.csv` and `national.csv`, both with real `'a'`-type alias rows (e.g. `"adt","alaska daylight time","a"`). **So this is reachable, not unreachable — at the time, just blocked on `userdict`'s emission path (dispatch order 0, `0x1005deb0`) not being ported yet in the text front end; that emission path is now ported (`src\text\norm.rs::userdict`).** Load one of those alternate dictionaries (or add a temporary alias entry) and an apostrophe/possessive line right after the matched alias should reach it; corpus line + recapture is the right next step. Cross-team dependency, not a dead end -- see message to team-lead, 2026-09-24 (corrected same day after an initial "unreachable" framing).

**Headline correction from this pass, read this first:** the `wa_in`/`wa_out` Frida hooks bracket the *whole* `fe_LinguisticAnalysis` call, and `fe_LinguisticAnalysis`'s first action is to call `fe_TagAndPronounce` (`0x1000e2f0`) — which is **lookup + copy + morphology + the wh-flag pass + function-word marking**, not just lookup+copy as an earlier pass assumed. Its decompile (short, worth re-pulling if in doubt) is exactly:
```c
int tag_and_pronounce(Ws *ws, Wa *wa, int n, short mode, int lex) {
    if (n == 0) { ws->morph_count = 0; return 0; }
    ws->flags = 0;
    ws->morph_count = 0;
    lookup_result = token_lookup(ws, wa, n, mode);                   // FUN_1000d190
    if (lookup_result < 0) return -1;
    if (ws->morph_count > 0) dict_CopyMorphsToWordArray(ws, wa, n);  // copy
    if (mode != 0) morphology(ws, wa);                               // FUN_10007520
    wh_flag(&ws->morph_count);                                       // FUN_1000e990, scans the morphs from here
    funcwords_result = mark_function_words(wa, n, lex);              // FUN_1000dfc0
    if (funcwords_result < 0) return -1;
    return lookup_result;
}
```
So a `wa_in`→`wa_out` byte diff mixes together the effects of *five* functions, only the last of which (the merge loop proper) is `fe_LinguisticAnalysis`'s own code — this is why, e.g., `war::TAG_A`/`war::TAG_B` (set by function words) show up in a naive `wa_in`/`wa_out` diff even though `merge.rs` never touches them. `src\ling\analysis.rs::tag_and_pronounce` now composes the four already-ported pieces (`tag_and_lookup`, `morph::morphology`, `funcwords::wh_flag`, `funcwords::mark_function_words`) in this exact order, so it *is* a faithful port of `fe_TagAndPronounce`; call it, then `merge::merge`, to reproduce `wa_out` from `wa_in`. `tests\feling_parity.rs::chain_stage` does exactly that and drives words/breaks/contexts from the result.

**What's left, in order:**

1. **Coverage pass, continued.** `u_token`'s `war::U_FLAG=='U'` gate is now explained (see the root-cause note above, 2026-09-24): it requires a userdict alias match, and the installed `userdict_eng.csv` has none, so no plain text reaches it with the shipped default. It is reachable through user-supplied dictionaries, and the text front end's `userdict` emission that gates it is now ported (`src\text\norm.rs::userdict`, see the ruling above) — an apostrophe/possessive line right after a matched alias in a user-supplied dictionary is the next step to exercise it. Mode `'S'` inside `u_token` (vs. the always-produced `'Y'`) is a separate, still-unidentified branch condition, lower priority since the outer gate can't fire anyway. Worth another harder/combined corpus line pass generally, though: `suffix_004` and `tail_001` both found real bugs in code that looked fine on simpler lines (see the round-4 write-up above), so "passes on an easy case" isn't the same as "correct".
2. **The documentation merge** into this multi-doc layout (`reveng/README.md`, `README_TECHNICAL.md`, `DLL_ANALYSIS.md`, `EXPERIMENTS.md`) -- nobody owned this before; see the project's day-to-day working notes for the model docs and what goes where. Keep the stage docs (this one included) as the detailed source and link to them rather than duplicating; leave the text front end as a stub (`son-fetext` owns `FRONTEND_TEXT.md`).
3. The project's day-to-day working notes' step 3: `src/engine.rs`, `vtpaul speak`, WAV-for-WAV identity against the main capture set — already well underway by a separate agent, `son-engine`, who owns `src/engine.rs` and only calls this stage's public API (`analysis::tag_and_pronounce`, `merge::merge`, `words::build_word_phone_records`/`TextPos`, `breaks::assign_phrase_breaks`, `context::build_phone_contexts`) -- keep those signatures stable or message it first if one must change again.
4. `merge.rs` now carries the `0xa2 0xfe` marker branch too (transliterated 2026-09-24; see below). No text input found so far reaches it.

~~`dict_GetTppField` exhaustive equivalence~~ / ~~`FUN_1000e160` dash-joining~~ / ~~wh-question flag~~ / ~~`fe_LinguisticAnalysis` merge~~ / ~~end-to-end WA→SS chain~~ / ~~`morph_028` "farers"~~ / ~~`camel_001` camelCase split~~ — all **done**, see the state table.

### 0. State in one table

| Stage (engine function) | Rust | Parity vs captured ground truth |
|---|---|---|
| Token lookup + split `FUN_1000d190` / `FUN_1000d640` / FillMorph `0x1000d450` | `src\ling\lookup.rs` | **730/730** sentences (`lookup_out`) |
| `dict_CopyMorphsToWordArray` `0x1000ea20` + homograph tagger `FUN_100068b0` (rules `FUN_100049b0`, fallback `FUN_10004300`, article `FUN_10003ff0`, poly.tree3 vote) | `src\ling\analysis.rs`, `src\ling\tagger.rs` | **730/730** (`copy_out`) |
| LTS `fe_LetterToSound` `0x10002f10`, spell `FUN_10009cd0`, hyphen LTS `FUN_10003110` | `src\ling\lts.rs` | **3154/3154** lts calls, **866/866** spell calls |
| `fe_BuildWordPhoneRecords` `0x10016c90` | `src\ling\words.rs` | **730/730** (`words_out`), bit-exact -- see the note below the table on `wr::START`/`wr::END`: they're written by `fe_NextSentence` in the real engine, not this function, and needed a request-level cursor plus a VTML/fullwidth offset-map remap to get exactly right |
| `fe_AssignPhraseBreaks` `0x100130e0` (syllabifier, phrase table, engbi CART) | `src\ling\breaks.rs`, `src\ling\syllab.rs` | **730/730** (`breaks_out`) |
| `fe_BuildPhoneContexts` `0x10017510` (incl. allophones `FUN_10017100`) | `src\ling\context.rs` | **730/730** (`ctx_out`) |
| **Morphology `FUN_10007520`** and callees | **ported, bit-exact** (`src\ling\morph.rs`, test `morph_stage`) | **730/730** sentences (`morph_out`, flags included) — `camel_001`'s `camelcase_split` and `morph_028`'s `"ers"` recursion (`"farers"`) both closed this pass, see §0.1 |
| function words `FUN_1000dfc0` | **ported, bit-exact** (`src\ling\funcwords.rs`, tests `funcwords_stage`, `tpp_field_equivalence`) | **730/730** (`fw_in_wa`/`fw_out_wa`); TPP field extraction cross-checked 220,850/220,850 (31,550 keys × 7 letters) |
| wh flag `FUN_1000e990`/`FUN_1000e570` | **ported, bit-exact** (`src\ling\funcwords.rs::wh_flag`/`wh_scan`, test `wh_stage`) — see §0.2 | **730/730** (`wh_out`'s own `wh` field) |
| `fe_LinguisticAnalysis` `0x1003e240` merge morphs → WA | **ported, bit-exact** (`src\ling\merge.rs::merge`, test `merge_stage`) — see §0.3 | **2154/2154** sentences (full `wa_out` record bytes + the header wh flag) |
| **Chain: `wa_in` → `analysis::tag_and_pronounce` → `merge::merge` → words/breaks/contexts** | `tests\feling_parity.rs::chain_stage` | **730/730** words, **730/730** breaks, **730/730** contexts |

`chain_stage` builds `Wa`/`Ws` from nothing but `wa_in` and runs the real sequence end to end (`tag_and_pronounce` → `merge` → `words::build_word_phone_records` → `breaks::assign_phrase_breaks` → `context::build_phone_contexts`), diffed against `words_out`/`breaks_out`/`ctx_out`. `record_stages` (the older test, still present) feeds those three builder stages the *captured* `wa_out` directly, proving each stage in isolation; `chain_stage` additionally proves our own merge output is good enough to drive them, which is the thing that actually matters for `vtpaul speak`. None of the `feling_parity` tests `assert!` 100% yet even though they hit it -- deliberately, since more coverage-pass corpus lines are likely to open new (real, useful) gaps; see item 1 above.

**`wr::START`/`wr::END`, three layers deep, all closed this pass.** `son-engine`'s independent from-scratch `Ss` construction found layer 1: `words::build_word_phone_records` never copied the WA record's character offsets (`war::START`/`war::END`) into the SS word record at all. Both tests here call `load_ss` first, which preloads the *whole* captured word record (including those two fields), silently supplying the values the code should have computed -- so a totally missing write was invisible to every test in this file. `breaks::predict_break_index` reads them to detect a real character gap between adjacent words; left zeroed, that guard is always false and the `engbi.tree3` CART break predictor never fires.

Fixing that the obvious way (`wr::START = war::START`, `wr::END = war::END - 1`) is only right for a request's first sentence. Strengthening `record_stages`/`chain_stage` to zero `wr::START`/`wr::END` before rebuilding *and* check them in `compare_words` (the same way the other fields the stage claims to write already were -- they weren't before, which is the actual reason layer 1 stayed invisible even in principle) surfaced layer 2: `war::START`/`war::END` reset to 0 at the start of every sentence within a multi-sentence request (confirmed straight from the bytes), but the *captured* `wr::START`/`wr::END` keep accumulating across the whole request.

**Layer 3, and the real fix**: `wr::START`/`wr::END` turn out not to be written by `fe_BuildWordPhoneRecords` at all in the real engine -- `fe_NextSentence` (`0x10022dc0`) writes them, *after* the ling stage (`fe_AnalyzeSentence` -> ... -> `fe_LinguisticAnalysis`) has already run for that sentence, by walking its own `TIB+4` request cursor against the resulting WA/SS records. Decompile, in full:
```c
// wa is WA (TIB+0x1312bc); tib is the TIB; ss is TIB+0x4c = SS.
// wa_word walks WA records, ss_word walks SS word records, in lockstep.
war_start = wa_word.start;                    // war::START
cursor = tib.cursor;                          // TIB+4, the running request cursor
ss_word.start = cursor + war_start;           // wr::START = cursor + war::START
if (wa_word.start < wa_word.end)
    ss_word.end = wa_word.end - 1 + cursor;   // wr::END = cursor + war::END - 1
else
    ss_word.end = cursor + war_start;         // degenerate (empty word)
// ... later, once this sentence is fully handled:
tib.cursor += wa.consumed;                    // TIB+4 += WA+4 (Wa::consumed)
```
Verified exactly against all 5 sentences of a genuine 5-sentence-per-request capture: every predicted `wr::START`/`wr::END` matches when `cursor` is the sum of every earlier sentence's own `Wa::consumed()` (the new `Wa::consumed` accessor, `WA+4`) within the request, `0` for the first sentence.

**Layer 4: the VTML/fullwidth remap.** The `cursor`-only fix closed every non-VTML gap (`multi_01`, `nws_03`-`05`, `prose_06`, `suffix_004`) but opened new ones: every VTML capture (`vtml_pitch`, `vtml_speed`, `vtml_sayas`, `vtml_break`, ...) started failing, including single-sentence ones. `fe_NextSentence`'s decompile has a second step right after the cursor add, through "processed -> original position" tables `fe_InitialTextProcess` builds once per request (`FRONTEND_TEXT.md`'s pipeline diagram already had these as `TIB+0x47790/94/98`, "orig ↔ processed positions" -- this pass identified which two of the three are used here and confirmed the exact formula):
```c
if (fullwidth_and_vtml_flag == 1) {   // byte at DAT_100a0460 + 0x20424
    // untaken by every capture so far; a second, composed indirection
    // through the third table (TIB+0x47790) for text that's *both*
    // fullwidth-normalized *and* VTML-tag-stripped -- not implemented,
    // no capture exercises it
} else {
    // clamp into [0, TIB+0) first, then:
    wr::START = map_start[cursor + war::START];   // TIB+0x47794
    wr::END   = map_end[cursor + war::END - 1];    // TIB+0x47798
}
```
Live-traced directly (deep-tracing a new `nextSentence` entry point, reading `TIB+4`/`TIB+0x47790/94/98` and the SS words it had just written): a capture's sentence 2 has `cursor=18`, and its first word's raw `cursor + war::START` is `18` -- but the true value is `40`, exactly `map_start[18]`. For plain text these tables are the identity function, so applying them unconditionally is a harmless no-op (re-confirmed against `multi_01`'s 5 sentences with the maps included this time).

Captured properly (not just for deep tracing): a new, permanent `offset_maps` event, emitted once per request from a regular (always-on) `fe_InitialTextProcess` hook, holding `map_start`/`map_end` sized to the processed text's own length. It fires before any sentence's `wa_in`, so it doesn't land in `sentences()`'s per-sentence buckets -- read it off the top-level event list (`load_offset_maps` in `tests\feling_parity.rs`).

Folded into `build_word_phone_records` anyway (rather than porting `fe_NextSentence` as its own step) as a new `pos: &TextPos` parameter (a small struct: `cursor`, `map_start: Option<&[i32]>`, `map_end: Option<&[i32]>`; `TextPos::none()` for a standalone plain-text request), since this is the last point before SS words are consumed and every caller already has a natural per-sentence loop to track the running cursor in. **This was worth the escalation**: `word.START`/`END` aren't cosmetic -- the waveform sync ring (`FUN_1002c530`), the lip-sync log and `VT_TextToBufferEX`'s `SYNCINFO` word/mark events all report them, so a from-text pipeline needs the real value, not just internally-consistent gaps (which is all `breaks`/`context` ever needed, and why they stayed 730/730 through every layer of this). `words`/`chain-words` are now genuinely **730/730**, maps and all.

`son-engine` (whose `engine.rs` calls into this) doesn't yet track a request-level cursor or the map tables -- adapted its own call site to `TextPos { cursor, map_start: None, map_end: None }` (correct for the single-sentence, non-VTML dispatch it does today; revisit together if/when it needs true multi-sentence or VTML-position-accurate requests).

**Signature change, told to `son-engine`** (they call `build_word_phone_records`/`run_ling` from `engine.rs`, which does not yet track a request-level cursor): pass `0` for now (matches their existing workaround's implicit behaviour, so single-sentence dispatch is unaffected); thread a real running cursor once `engine.rs` handles genuine multi-sentence requests.

If you own a stage and its only test loads a full captured struct before running it, that test cannot prove the stage populates every field it claims to, only that the fields it *does* write are right when everything else is already correct -- it's worth periodically checking what a genuinely from-scratch build looks like, the way `chain_stage` already does one level up (`Wa`/`Ws` from nothing but `wa_in`), and the way `record_stages` now also does for `wr::START`/`wr::END` specifically. This one took four rounds of "that's not quite it either" to actually close -- each wrong-but-plausible fix was caught by re-running the *whole* corpus, not the one capture being chased at the time.

### 0.1 Morphology (`src\ling\morph.rs`), 730/730 — what's solid vs approximate

Reproduce: `cargo test --release --test feling_parity -- --nocapture morph_stage`. It chains `wa_in` -> `analysis::tag_and_lookup` -> `morph::morphology` and diffs every morph (spelling/pron/accent/B-flags **and the u16 flags word**) against the captured `morph_out`.

Asm-verified (fully read from the disassembly, not just the handoff prose, cross-checked branch by branch against the driver's own call sites):
- `morphology()` itself: the per-morph gate (`skip_rest`), the exceptdict-first dispatch and its `i += consumed` skip-post-rules jump, the `flags0` seed, the final "the"/"de" pass.
- The exceptdict chain: `match_hyphenated_except` (`dict_MatchHyphenatedExcept` 0x10008dc0), `count_hyphen_parts` (`dict_CountHyphenParts` 0x1000c9c0), `apply_except_pron` (`dict_ApplyExceptPron` 0x1000ca50), `dash_parts` (`FUN_1000ca30`).
- `cb30`/`cbe0` (`FUN_1000cb30`/`FUN_1000cbe0`, lexicon root lookups — pure, no flag side effect).
- `case_mode` (`FUN_10009030`), `verb_context` (`FUN_10008580`), `apostrophe_end` (`FUN_1000c710`), `is_numword` (`FUN_10008550`).
- `ly_root` and `s_suffix_root`, the `"ly"`/`"s"` branches of `suffix_strip` (`FUN_1000a140`) — read the full 1713-line `.s` for these two specifically (labels `L0a3xx`-`L0b45c`). `s_suffix_root` in particular replicates the exact branch tree (es/sibilant/ch/sh/consonant+o special cases pick different stripped stems; the plain "strip just the final s" path **rejects the match outright** — does not fall back to LTS or double-suffix — when the resulting root's own last phone is already a sibilant, e.g. `importss` -> strip one `s` -> lexicon hit on `imports` (ends in `S`) -> rejected, falls through to plain LTS instead of `imports` + another `S`). This was the single biggest source of failures at 668/722 and is now the reference pattern for how the rest of `FUN_1000a140` almost certainly behaves.
- `post_rules`: `"a"` (EY->AH0), `"use"`/`"close"` (S->Z via `verb_context`, **the morph-base-relative pron offset was previously off by 4** — fixed, worth double-checking any other `0x2Xu` literal lifted verbatim from an ESI-relative asm dump), number+`"minute"`, `"bow"` (`L_BOW` window 5), `"lead"` (`L_LEAD` window 10, only when the current pron is IY1), `"mouth"` (`L_DET` right window 2, only when pron[2] is DH and `i>1`), and `read_rule` (IY1->EH1: `L_BE`/`L_HAVE` in the previous 1-2 words or "not" 2 back, "he"/"she" one back, "I"/`L_OBJPRON` one back plus an `L_TIME` window hit, an `L_WH` word one back, or "get"/"got" one or two back **but specifically not "gets" one back**, and not immediately followed... preceded by "to"). `morph_list_window` (`FUN_1000cc60`) is the asm-derived window search shared by bow/lead/mouth/read (distinct from `tagger::phrase_in_window`: same binary search, but adds mode `'B'` = halve the window and search both directions into one combined array).

**`"ers"` (`morph_028`, "farers") -- closed this pass, live-traced, not more asm reading.** Deep-tracing `cb30`/`cbe0` for that one corpus line showed the real engine calling `cb30("farer")` (fails) then `cb30("fare")` (succeeds) for `"farers"` -- i.e. the second attempt strips only `"rs"` (`stem(2)`), not `"ers"` (`stem(3)`) as the code previously assumed. **Do not just swap `stem(3)` for `stem(2)`**: doing that outright broke 11 other, previously-passing `"ers"` words in the corpus whose `stem(2)` isn't a real word but whose `stem(3)` is (e.g. `"packthreads"`'s sibling forms -- this was caught immediately by running the *whole* suite, not just `morph_028`). The fix that closes both is trying `stem(2)` *before*, not instead of, `stem(3)`: `stem(1)` -> `stem(2)` -> `stem(3)`, first hit wins, all three using the same `+[0x1a, 0x44]` (ER Z) suffix as the `stem(3)` branch always did. `find_root`'s original "recurse through `suffix_strip` itself" hypothesis from the previous pass turned out not to be what the engine does here -- it's a second, shorter literal stem attempt, not recursion.

**`camelcase_split` (`FUN_1000c3a0`/`c860`/`8cc0`) -- ported this pass, live-traced.** A corpus line exercises it (`camel_001`: `JavaScript PowerPoint YouTube LinkedIn FedEx McDonald iPhone GoodNight.`). The earlier version of this doc flagged a real trap here and it's worth keeping the story, since the same shape (a plausible fix that's right for the wrong reason) will recur for the remaining suffixes:
  - `is_mixed_case` (`FUN_10008cc0`) and the case-transition splitter `split_camel_part` (`FUN_1000c860`) are fully decoded from the decompile alone (no `.s` needed, no VC6 slot-reuse ambiguity in either) and ported as-is.
  - The mystery was: ground truth shows `PowerPoint`->`Power`+`Point` and `LinkedIn`->`Linked`+`In` genuinely split (the `0x64` "d" joiner byte in the captured pron), but `JavaScript`/`YouTube`/`FedEx`/`iPhone`/`GoodNight` don't split at all -- including `GoodNight`, even though `Good`/`Night` are both ordinary words `FUN_1000c860` would happily split at. **Resolved by `--deep`-tracing `dict_LookupLexicon` for the whole sentence**: `"GoodNight"` (and `"JavaScript"`/`"YouTube"`/`"FedEx"`/`"iPhone"`) all get a **nonempty** payload from the very first, whole-word `dict_LookupLexicon` call inside `FUN_1000d190` (the main token lookup, which runs *before* morphology) -- they're simply already lexicon entries in their own right (proper nouns / compounds the dictionary happens to include), so morphology's own pre-existing "skip the whole suffix/clitic/camelCase chain when the morph already has a pron" gate short-circuits them before `camelcase_split` is ever reached. `PowerPoint`/`LinkedIn` get an *empty* whole-word payload, so they do reach it, and their parts (`"Power"`/`"Point"`/`"Linked"`/`"In"`) each get looked up via `dict_LookupLexicon(part, ..., 0, ws+0x48128)` -- **the exact same base pointer and mode `FUN_1000d190`'s own whole-word lookup uses** (confirmed: both compute the same lexicon-context pointer, `ws+0x48128`, from their own decompile-local base register, as the lexicon-context argument), i.e. there never was a separate "camelCase dictionary"; `ws+0x48128` is just where the loaded lexicon context lives, same as everywhere else. So the per-part resolution really is exactly `cb30`, as first prototyped -- it only needed to be reached at the right point (which the existing driver structure already handles), not a smarter lexicon check.
  - Originally ported as the "part found directly via `cb30`, `len>=2`" happy path only (`FUN_1000c3a0`'s "big buffer" branch). **Update:** the short-part/lexicon-miss fallback chain (B80 LTS, `should_spell`, `suffix_strip`, "X's", then plain LTS or spelling per part) is now also ported, in `src\ling\morph.rs::camelcase_split`.
  - One more `--deep` catch: `FUN_1000c3a0` sets its flags-out byte to `0x18` on success (traced directly: read `*flags` before/after the call), not just the generic chain's `0x08` -- `0x10` is presumably `c3a0`'s own "this word was split" marker, distinct from the generic "was modified by the fallback chain" bit every other chain function also sets.

**`find_root`'s doubled-consonant / silent-e / i->y fallbacks, and `ise_retry` -- corpus-tested this pass, no code changes needed.** Two new corpus lines (`suffix_001`: `Resemblance happiness amazement friendship hopeless typed stopped runner baker bigger running baking biggest widest happiest typist hopeful cried tried.`; `suffix_002`: `Admittance beginner shopper sadder planning gunner submission occurrence dimmer thinnest fittest sitting winning.`) deliberately hit `find_root`'s three fallbacks across all 9 of the other `FUN_1000a140` suffixes (`ance`/`ness`/`ment`/`ship`/`less`/`ed`/`er`/`ing`/`est`/`ist`/`ful`) -- doubled-consonant undoing (`admittance`->`admit`, `beginner`->`begin`, `planning`->`plan`, ...), silent-e restoration (`typed`->`type`, `baker`->`bake`, `amazement`->`amaze`, `typist`->`type`, `hopeful`->`hope`, ...) and i->y (`happiness`->`happy`, `happiest`->`happy`, `cried`/`tried`->`cry`/`try`). Both passed with **zero code changes**, and a third line (`suffix_003`: `Organise apologise realising civilisation specialised recognise emphasising.`) did the same for `ise_retry`. So the "best-effort, prose-derived, likely buggy" warning that used to sit on all of these no longer matches the evidence -- they're corpus-verified correct on every case tried, even though they're still not read from the `.s`. Not exhaustive (a coverage pass never is), but treat them as solid until a real failure says otherwise, not as an open item.

**Two real bugs found and fixed by a 4th, harder line (`suffix_004`: `J.R.R. Tolkien wrote well-known state-of-the-art e.g. books. XYZCORP announced ZZTOP results.`), both in code that predates this pass and had simply never been corpus-exercised:**
- `lts::spell` (`FUN_10009cd0`) double-inserted the `0x64` join separator around embedded punctuation (`"J.R.R"` came out as `jay`-`64`-`64`-`ar`-`64`-`64`-`ar` instead of `jay`-`64`-`ar`-`64`-`ar`): once from a letter's own look-ahead to the next separator-then-letter run, and again, redundantly, when the loop then *visited* that separator character itself. Fixed by only ever deciding "insert a joiner" from the look-ahead (a letter's own decision about what follows it), never from visiting the separator directly -- see the doc comment on `spell` for the corrected control flow. Also fixed the `should_spell` driver branch's flags: a separator-containing spelling gets `0x08` in addition to `0x20` (the same "did non-trivial multi-part work" marker every other successful chain step sets), which the code wasn't setting before.
- The B80-lexicon-hit branch's `hy` condition (which LTS variant to call: plain `letter_to_sound` or `lts_hyphenated`) was checking "does the *previous* morph belong to a hyphen-class token" -- true for the exceptdict multi-morph hyphenated words (`"vis-a-vis"` etc.) that consume the check today, but wrong for a *single*-morph word that has its own literal hyphen, like `"well-known"` (a genuine B80 lexicon entry with no direct pron). The handoff prose's original "or `FUN_10003110` if the word has '-'" meant *this word's own spelling*, not the previous morph's token class; fixed to check `sp.contains(&b'-')` directly. `"well-known"`'s pron was already byte-correct either way (via a different, coincidentally-right fallback path) -- this only fixed its `mr::FLAGS` byte, so it wouldn't have shown up outside `morph_stage`'s own flags-inclusive comparison.

Both were caught because a *harder* corpus line (mixing multiple features on real words, not one clean case per suffix) was deliberately chosen for the 4th round, after the first three rounds passed cleanly -- worth remembering for whoever continues the coverage pass: an easy corpus line that passes doesn't mean the surrounding code is bug-free, only that it's bug-free *for words like that*.

**`clitic_inword` (`FUN_1000c040`) -- fixed this pass, live-traced.** `tail_001` (`Fish'n'chips and mac'n'cheese are rock'n'roll classics.`) exercised it for the first time: `"rock'n'roll"` already passed (it's a direct lexicon entry, `flags=0x01`, never reaches this function at all), but `"Fish'n'chips"`/`"mac'n'cheese"` didn't. `--deep`-tracing `FUN_1000c040` itself (not just its callees) showed the *real* engine returns `0` (fail) for both. The port's bug was matching the clitic pattern (`"'n"` etc.) anywhere in the word and silently discarding whatever text followed it -- so it wrongly *succeeded* with a truncated pron (root + clitic, `"'chips"`/`"'cheese"` dropped). The real function requires the matched pattern to reach the end of the word; fixed by rejecting any match with trailing text, so these two words correctly fall through the rest of the chain to plain LTS on the whole spelling instead (which handles the embedded apostrophes fine on its own). One-line fix (`find` now filters for `p + pat.len() == lower.len()`), but only visible because the corpus line had a clitic *not* at the end of the word -- every earlier apostrophe-clitic corpus line (`contr_*`, `y'all`/`o'clock`/etc.) happens to have the clitic at or effectively at the end, so this never came up before.

**`should_spell`'s class-`'A'` path -- corpus-tested this pass, no bug found.** `tail_002` (`PEPSI and REEBOK are TOYOTA brands from ZANTAC labs.`, four unfamiliar all-caps words) passed cleanly.

Still genuinely unexercised: `u_token` (`FUN_100091b0`)'s non-`'Y'` modes -- mode `'S'` and the sub-token clitic list (`'s`/`'ve`/`'em`/`'re`/`'ll`/`'d`/`'m`/`'n`/`'ee`/`'er`/`'ers` after a `war::TEXT`/`+0x18` sub-token boundary). Every corpus line tried (including `tail_003`, `"The 1950's were golden and the 10's add up."`, chosen to hope a normalizer marks the possessive-decade pattern as a `'U'` token) produces `war::U_FLAG != 'U'` for every morph, so the function bails at its very first check every time -- **now explained, not just observed (2026-09-24, see the root-cause note at the top of this doc)**: `war::U_FLAG='U'` is only ever assigned by the `symbols` normalizer's apostrophe-continuation of a word whose `war::CLASS` (`+0x28`) is already `0x19`, and class `0x19` is only ever produced by `userdict`'s emission path (now ported, `src\text\norm.rs::userdict`) -- so `tail_003`'s possessive-decade guess was a reasonable try but numword-produced decades aren't class `0x19` and never will be; nothing plain-text can reach this. `clitic_same_token` is already well covered by the pre-existing `contr_*`/apostrophe corpus lines.

`camelcase_split`'s ported deep-trace method (the capture harness's opt-in per-function argument/return-value logging, not part of this repository) is the template for these too: add the callee(s) to the deep-trace list, capture a corpus line, read the actual args/return values across a call instead of re-deriving them from the decompile -- and, per the `spell`/`hy` bugs above, don't stop at the first corpus line that passes.

Files touched this pass: `src\ling\morph.rs` (new), `src\ling\mod.rs` (added `pub mod morph;`, `war::U_FLAG = 0x23`), `src\ling\tagger.rs` (`modal_pair` made `pub(super)`), `tests\feling_parity.rs` (new `morph_stage` test, reusing the already-present `compare_morphs`/`morph_key` helpers that were clearly staged for this).

### 0.2 Function words (`src\ling\funcwords.rs`) and the wh flag, both bit-exact at 730/730

Reproduce: `cargo test --release --test feling_parity -- --nocapture funcwords_stage`. It loads the `fw_in_wa` WA records directly (this stage runs on the WA, not the morph array — no chaining from `wa_in` needed), runs `funcwords::mark_function_words`, and diffs every record byte for byte against `fw_out_wa`.

**`FUN_1000dfc0` (`mark_function_words`) is ported and asm-verified** for the dominant single-word case:
- `get_tpp_field` = `dict_GetTppField` 0x1003a7b0, implemented directly in Rust against the already-loaded `lex.tpp` `HashDict` (the payload format from `LEXICON.md` §5: find the field starting with `letter` — either the payload's own first char or after a `" <letter>"` marker — cut at the next space) instead of porting the engine's string-scanning byte for byte; the two should agree for any real payload.
- `parse_tpp_num` = `FUN_10064645`/`FUN_100645ba`: leading decimal digits of the field, as a byte (the "G-class id", up to 124 per `LEXICON.md`, fits).
- `apply_class` = `FUN_1000e0c0`, ported in full including the span form (sets WA+0x20=1 word-sized on every index in `[i, end)`, then WA+0x25=class and WA+0x26=`wab.txt2` index+1 on every index in `[i, end)` *and* on `end` itself) even though the only caller here passes `end=0` (which the real function's own `0 < end` guard degenerates to "just apply to word `i`", matching the C exactly).

**`FUN_1000e160` (the dash-join span) is now also ported and asm-verified — 730/730, no known gaps.** Pulled the raw disassembly rather than trusting the decompile (which has the classic VC6 stack-slot-reuse problem: the 4th parameter `i`'s stack slot gets read as an int, then reused to hold a text pointer, then reused again to hold a WA-record pointer, which is what made the decompile's local variable for that pointer look uninitialized). Confirmed: it joins word `i` with up to 5 following class-`'A'` WA records (adjacent: `next.start - prev.end == 1`), trying `dict_GetTppField(joined, 'F', 'I')` after each append and keeping the *longest* successful match (same "keep growing, remember the last hit" pattern as the exceptdict matcher in `morph.rs`); it also transparently skips over literal `"dash"` placeholder tokens (class `'S'`, zero-width, adjacent — apparently the text front end emits these for some hyphens) without spending them against the 5-word cap. `war::U_FLAG` (`0x23`) turns out to be a dual-purpose byte: morphology's `u_token` compares it to `'U'`, this compares it to `'S'`/`'A'` — same field, different values checked by different callers.

`tpp_field_in_payload` (the field-extraction half of `dict_GetTppField`) is cross-checked against an independently-written second transcription over **every stored TPP payload** (`tests\feling_parity.rs::tpp_field_equivalence`, `lex.tpp.record(s)` for `s in 0..nkeys`) for all 7 documented field letters (G/A/B/C/D/E/F): 220,850/220,850 exact, 0 mismatches. This doesn't replace engine ground truth (there's no live oracle available, only captures), but it does rule out any off-by-one/edge-case bug that the captured words happen not to exercise.

**`FUN_1000e990` (the wh-question flag) is ported and bit-exact — `src\ling\funcwords.rs::wh_flag`/`wh_scan`, test `wh_stage`, 730/730.** Two of the six leaf-helper word lists turned out to already exist under different names: `FUN_1000e4e0`'s list (`0x10078038`) is `tables::L_NEG`, and `FUN_1000e3a0`'s list (`0x10078044`) is `tables::L_MODAL` — both already used by morphology's post-rules. The other four were new, added to the DLL-table generator (not part of this repository) and generated into `tables.rs`:

| List | Address (count at) | Content | Used by |
|---|---|---|---|
| `L_WHPREP` | `0x10078140` (`0x1007824c`) | 64 prepositions/subordinators (about, after, ... yet) | `FUN_1000e540`, gates whether `wh_scan` recurses before falling into the literal/wh-word checks |
| `L_WHCONJ` | `0x100780e0` (`0x10078244`) | and, both, but, either, neither, nor, or | `FUN_1000e4b0`, same gate |
| `L_WHWORD` | `0x100780fc` (`0x10078248`) | how, what(ever), when(ce/ever), where(by/ever), which(ever), who(ever), whom, whose(ever), why (17 entries; distinct from the existing `L_WH` used by morphology's `read_rule`, different address) | `FUN_1000e510`, the actual "is this word a wh-word" test |
| `L_WHBE` | `0x100780c8` (`0x10078240`) | 's, am, are, is, was, were | `FUN_1000e430`, position-gated (excludes a leading `"'s"`) |

`wh_scan` (`FUN_1000e570`) takes a morph index and returns a tri-state byte: `0x50` ('P', found — sets the flag), `0x4e` ('N', a definite veto — stops the scan without setting it) or `0x44` ('D', "not here, try the next candidate position"). The pointer-vs-index stack-slot reuse in the decompile (an index variable in the first half of the function becomes two pointer variables tracking the *same* position in the second half, both advancing in lockstep by one morph per loop iteration) was resolved by hand against the `+0x70`-per-morph stride, not trusted from the decompile's variable names or types. The genuinely useful simplification found while doing that: **once the current position is confirmed to be an `L_WHWORD`, every remaining return in the function is `0x50` unless a recursive sub-call explicitly returns `0x4e`** — the neighbour-word gates and the comma-scanning tail only ever shorten the scan or veto it, they never themselves assert "not a question". That collapsed what reads in the decompile as a dozen-odd distinct exit points into the shape `wh_scan`'s own doc comment describes. `FUN_1000e990` itself is a short outer loop: bail immediately unless the *last* morph's `mr::CAT==3` ('?'), then try `wh_scan` at position 0 and, for each later morph whose `mr::CAT!=0`, at `position+1`, stopping on the first `0x50` (flag=1) or `0x4e` (flag stays 0).

### 0.3 The `fe_LinguisticAnalysis` merge (`0x1003e240`) — ported, bit-exact, `src\ling\merge.rs`, 2154/2154

The full decompile/asm (not part of this repository) covers this function; `src\ling\merge.rs` has the Rust with a section-by-section doc comment. This section records how the last piece (the `EBX+X` puzzle for the "same-token, not a new record" path) actually got solved, since it's a good example of the capture-diff method working where pure asm algebra didn't.

**The key unlock: `fe_LinguisticAnalysis`'s *own* code (the merge loop, after its call to `fe_TagAndPronounce`) never touches `war::TAG_A`/`war::TAG_B` at all.** An earlier pass's two-capture diff found those fields changing inside the `wa_in`/`wa_out` window and reasonably assumed the merge set them. It doesn't — `fe_TagAndPronounce` (called first, see the top-of-file correction) runs `mark_function_words` internally, and *that's* what sets them. Once that was separated out (by chaining `tag_and_pronounce` — lookup+copy+morphology+wh+funcwords — before diffing what the merge loop itself does), the remaining `EBX+X` accesses resolved cleanly, the same "new record" way as before, just against a *different* effective base for the continuation-morph branch (`_Str = record_ptr - 0x60`, but `_Str` itself is reused: in the "new record" half it points at the record about to be started, in the "same token" half — reached without `_Str` having advanced yet — it points one record *ahead* of the one actually being filled, so continuation accesses land at `_Str - 0x94 + X`, i.e. `record - 0x34 + X`). Concretely, resolved against `war::*`:

- `war::CLASS_CHAR` (`0x22`) = **first morph's** class char only (`fail_first=0` across all 5,538 words in the corpus; continuation morphs never touch it).
- `war::FLAGS` (`0x1c`, a `short`) = the bitwise **OR** of every merged morph's own flags word, low byte and high byte both (`fail_or=0` across all 5,538 words; the first morph's flags word is stored as a plain assignment, but since the field starts at 0 that's equivalent to OR-ing into it).
- `war::PHONES` (`0x52`) = the first morph's pron, *unless* `war::U_FLAG=='U'` is false **and** `war::CASE_MODE=='Y'` is true (a VTML/preset-pronunciation word), in which case the field is left as whatever `wa_in` already had — then every later merged morph's own nonempty pron is appended (`strcat`, not `strcpy`, so it appends onto the preserved preset too, if there is one and a later morph has a pron). Proven with `fail=0` across the whole corpus, including the 3 `vtml_phoneme`/exceptdict-preset cases where the naive "concat everything" hypothesis first failed.
- `war::FORCED_POS` (`0x30`) is reset to the sentinel `0xff` once per WA record, and a further `char[]` run starting right after it, at `war::FORCED_POS+1 ..= war::FORCED_POS+n_merged`, is filled with `0xff` too — one byte per merged morph, capped at 5 (`if (*(int*)(_Str-0x12) < 5)` guards every continuation morph; a 6th+ same-token morph is silently dropped from the merge entirely, not even contributing its pron/flags — asm-confirmed, but no capture in the current corpus has 4+ morphs sharing a token, so the cap itself is unverified by test). No capture ever shows anything but `0xff` in this run, so its real purpose (if it's read anywhere downstream) is unknown.
- `war::NMORPH` (`0x10`) = the number of morphs merged into the record (matches the `char[]` run's length above).
- At the very end, if `ws.n_morphs()>0`: `WA[1]` (the WA header's second `short`, `Wa::wh()`) = `ws.wh` (`ws+0x429a4`, the flag `wh_flag` sets) — confirms the wh-flag pass really does run before the merge in the real pipeline (it's inside `fe_TagAndPronounce`, called first).
- Return value: `1` if the number of WA records the merge actually started equals `wa.n()` (the WA's own word count going in), else a failure code — a self-consistency check, not separately meaningful; `merge::merge` returns this as a `bool`.

**Marker branch (ported 2026-09-24, capture-unexercised):** after a record starts, the merge's cursor `_Str` points at that record's `war::TEXT` (+0x34), not PHONES. A same-token morph whose spelling starts with the VTML marker `a2 fe` cuts `war::TEXT` at its `strstr` hit and takes 2 off `war::LEN` (once per record); every later same-token morph's spelling is `strcat`ed onto `war::TEXT`; the marker morph is not merged. `war::NMORPH` counts up from the record's existing value. 34 probes with VTML tags inside words, numbers, URLs, times, contractions and abbreviations never put the marker inside a WA token: the normalizers split around it.

Proven with `merge_stage` (full `wa_out` record-byte + header-wh diff, chained from `wa_in` through `tag_and_pronounce`) at 2154/2154 sentences, and `chain_stage` (the same, then straight into words/breaks/contexts) at 730/730.

### 1. Reproduce the numbers

Build and test from PowerShell, from the repository root. The tests need the captured ground truth
(713 JSON files, produced by the capture harness described in the repository's top-level `README.md`;
not part of this repository — the test suite skips cleanly when they aren't present):

```
cargo build --release
cargo test --release --test feling_parity -- --nocapture
```

Expected output lines:
- `lookup 730/730`, `copy 730/730`, `words 730/730`, `breaks 730/730`, `contexts 730/730`, `funcwords 730/730`, `wh 730/730` (sentences exact)
- `lts 3148/3148`, `spell 865/865` (calls exact)
- `morph 730/730`, `merge 2154/2154`, `chain-words 730/730`, `chain-breaks 730/730`, `chain-ctx 730/730`
- `tpp_field_equivalence: 31550 keys, 220850 ... pairs checked, 0 mismatches`

Recapture the ground truth. It takes about 15 s in total and uses Frida plus a 32-bit engine host
in mode 7 (this tooling is not part of this repository).

### 2. Remaining work, in priority order

#### 2.1 Morphology `FUN_10007520(ws, wa_rec0)` — biggest item

About half of all corpus words go through this function (mode-7 source tags: G 2634, AT 1792, A 415, R 252, E 235, S 115, GR 60, RAT 28).

**Test to add:** load `wa_in` and the `copy_out` morphs into a `Ws`, run the port, then compare against `morph_out` **with flags** (`compare_morphs(..., true)` in `tests\feling_parity.rs`).

**Addressing.** `ws+0x429a2` is the s16 morph count. Morph `i` starts at `ws+0x429a6+i*0x70`, with these fields:

| Offset | Field |
|---|---|
| +0 | u16 flags |
| +2 | token |
| +4 | cat |
| +6 | class char |
| +7 | spelling |
| +0x25 | pron |
| +0x66 | accent 'X' |
| +0x68 | B40 |
| +0x6a | B10 |
| +0x6c | B20 |
| +0x6e | B80 |

`wa_rec0` = WA+0x14. Record fields (see `war::` in `src\ling\mod.rs`):

| Offset | Field |
|---|---|
| +0x14 / +0x18 | sub-token start/len |
| +0x23 | 'U' flag (add a constant) |
| +0x24 | case mode |
| +0x2c | token class (0xb = '-') |
| +0x30 | forced POS |
| +0x52 | preset phones |

**Driver, per morph i** (confirmed from the disassembly of `FUN_10007520`):

1. Set flags = pron nonempty ? 1 : 0.
2. Call `FUN_100091b0` (the 'U' token). **It may advance i.**
3. Continue only if (WA.forced_pos==0xff or pron empty) and (WA.U=='U' or WA.phones empty).
4. If `91b0` returned 0:
   1. `dict_MatchHyphenatedExcept 0x10008dc0`. On a match, i += consumed-1 and skip the post-rules.
   2. `FUN_10009030(morph, class, U, casemode)`. When that returns 0: if casemode=='A', try LTS.
   3. `FUN_100086c0(sp, ws+0x429a2, i, NULL, 0)`. Nonzero means spell.
   4. Otherwise, if pron is empty:
      - hy = (i≥1 and the previous morph's WA token class == 0xb).
      - If B80 is set: run LTS (or `FUN_10003110` if the word has '-'). On success, flags |= 0x40.
      - Otherwise run the chain `9dc0 → a140 → b4d0 → c040 → b800 → c3a0 → c710 → LTS`. If all of them fail: class = 'E' and spell.
5. Post-rules (`LAB_1000792a`): "a" (EY→AH0 / 0x1e), "use"/"close" (S→Z via `FUN_10008580`), "UP" (case-sensitive) before a/the, "read" (IY1→EH1 context rules: L_BE, L_HAVE, he/she, I/objpron + L_TIME window, L_WH, get/got/gets, to), number+"minute", "bow" (L_BOW window 5), "lead" (L_LEAD window 10), "mouth" (L_DET right window 2).
6. Final pass over all morphs: if the previous spelling is "the"/"de" and the current pron starts with a vowel code ≠ 'C', set the previous pron to `[p0, 0x26]` and flags |= 8.

**Callees.** All were dumped, for each function: raw decompile, a strlen/strcpy-folded cleaned form, a form with strings inlined, and compact asm (not part of this repository).

> **Trust the `.s` over the decompile.** VC6 reuses parameter stack slots (e.g. `[EBP+0x14]` becomes the MRU cache pointer and `[EBP+8]` becomes `local+len`). Ghidra's C for `FUN_1000a140` is wrong in several places.

| Function | What it does, with the confirmed details |
|---|---|
| `FUN_1000a140` | Suffix stripper, fully decoded from asm.<br>**Suffixes and minimum length:** liness >8; ance/ness/ment/ship/less >6; ed/er >4; ers/est/ing/ist/ful >5; ly/s >4.<br>**Root lookup:** `FUN_1000cb30` (lexicon first pron, else LTS when B80) and `FUN_1000cbe0` (pron for POS 0x25 = VB). Roots use doubled-consonant and i→y fixes.<br>**Phone suffixes by last phone:**<br>• ed: T / IH0 D / D (unvoiced set {2a,35,37,20,22,38,3a,14}; {39,15} gets 23 15).<br>• s: S / IH0 Z / Z (unvoiced {20,2a,35,39,3a}; sibilant {14,29,37,38,44,45}).<br>• ing: 23 2e. er: 1a. ers: 1a 44, or +44 when "…er" is found. est: 23 37 39, or 07 37 39 after i→y. ist: 23 37 39. ful: 20 07 2b.<br>• ly: 26→07, then +2b 26. ance: 07 2d 37. ness: 2d 23 37. ment: 2c 07 2d 39. ship: 38 25 35. less: 2b 23 37. liness: 2d 23 37 (root "…ly") or 2b 26 2d 23 37.<br>**On success:** flags \|= 8.<br>**Quirk:** the est-branch flag byte is the high byte of the spelling pointer when a root is found early. It is 0x02 for this WS (captured ws=0x02f62878), so it is ≠3 and gives IH0.<br>**Quirk:** on the b404 path, failure leaves the root pron in the output buffer. Keep the buffer semantics, because `FUN_1000c040`'s tail returns 1 whenever `out[0]≠0`. |
| `FUN_1000b4d0` | British "ise" → "ize" retry: isations/isation/isingly/isably/isable/ising/ised/ises/ise (min lengths 9/8/8/7/7/6/5/5/4). Looks up with `cb30`; flags \|= 9. Sets out[0]=0 at entry. |
| `FUN_1000c040` | Clitics 'n / 'ee / 'er / 'ers inside a word (strstr lower, then UPPER). The root comes from:<br>• the lexicon (flags \|= 1), or<br>• `86c0(…, &decoded, 1)` → spell, or<br>• `9dc0` / `a140` / LTS / spell.<br>Then it appends: 'n → N or AH0 N (N if the last phone is a vowel, or second-last vowel & last==0x36); 'ee → 26; 'er → 1a; 'ers → 1a 44.<br>**Tail:** out[0]≠0 → flags \|= 8, return 1. |
| `FUN_1000b800` | Clitic morph (same token as the previous morph).<br>First it looks up the concatenated spellings of the token; if found, the pron goes on the first morph, later morphs are emptied, and flags \|= 8.<br>Otherwise it appends to its own pron, based on the previous morph's last two phones:<br>• 's: Z / S / IH0 Z<br>• n't: N T or AH0 N T<br>• 've: V or AH0 V (0x41)<br>• 'em: AH0 M<br>• 're: 0x36 or 1a<br>• 'll: L or AH0 L<br>• 'd: T / IH0 D / D unless the previous word is in the subj-pron list 0x100786f0 or L_WH, then D or AH0 D<br>• 'm: M or AH0 M<br>If nothing matches, it writes pron="" (so reload the buffer after a failed call). |
| `FUN_1000c3a0` + `FUN_1000c860` + `FUN_10008cc0` | CamelCase split. `8cc0` is mixed-case (any case change; false for Mac/Mc prefixes). `c860` splits before Upper+lower after Upper, or before Upper after lower, and handles 's.<br>**Per part:** lexicon (len≥2), else B80 LTS, else `86c0` spell, else `a140`, else 's root, else LTS/spell. Parts are joined with 'd' (100) with a 0x40 cap. Returns 1 only if a part was found by lexicon/`a140`/'s.<br>**Quirk:** a single-part mixed-case word (e.g. "Hello") writes out[0]=0 and returns 0. |
| `FUN_1000c710` | Word ending in `'`. For "in'": try "ing"/"inG" and turn a final NG (0x2e) into N (0x2d), flags \|= 9. Else strip the apostrophe: `cb30` (9) / LTS (8) / spell (8). |
| `FUN_10009dc0` | Parts split at '-' / '.'.<br>**Single letter:** "a" → AH0 (7) when hyphen context applies; otherwise spell.<br>**Longer parts:** a known E/A class (`FUN_10003b10`, payload flag byte & 0x0c) with no lowercase is spelled. Otherwise `cb30` (flags \|= 1), else `a140` / `c710` / `c040` / `c3a0` / LTS / spell.<br>**"the"/"de" rule:** a part after one of them that starts with a vowel turns the previous part's last phone into IY (0x26).<br>**Joining:** parts are joined with 'd' with a 0x3f cap; flags \|= 8. |
| `FUN_100086c0` | "Should this all-caps word be spelled?" Decoded (decompile agrees with asm on the checked parts).<br>**Requires** `FUN_10010010` (all upper letters).<br>• class 'E' → 1. class 'A' → A-path.<br>• pron or B80 → 0.<br>• no vowels (`FUN_100100d0`) → 1.<br>• len>2 and `lts_valid` → 0.<br>• B40/B20 → 1.<br>Then it checks the neighbour words for no-lowercase (left/right scans of spellings that start with a letter), the L_NOSPELL list (case-sensitive, generated) and "Lexus"+"IS". Finally: B10 → 0; len<3 → 1; else !lts_valid.<br>**With the 5th argument set and a decoded entry:** len==1 or class E returns 1 unless the word is "Wi" with lowercase. |
| `FUN_100091b0` | 'U' tokens.<br>**Mode 'Y':** pron = WA preset phones, flags \|= 4; skip the token's later morphs.<br>**Mode 'S':** spell the part before the sub-token.<br>**Sub-token clitic** (WA+0x14/+0x18): 's, 've/'VE, 'em/'EM, 're/'RE, 'll/'LL, 'd, 'm, 'n, 'ee, 'er, 'ers as in b800. Anything else: `cb30`/LTS/spell of the text after the apostrophe, appended after a 'd'. flags \|= 8. |
| `FUN_10009030` | Case modes.<br>• 'S' → spell.<br>• 'C' with U 'I' / 'M' / 'E' → spell for class E/A, else keep pron (flags \|= 1), else B80 LTS, else ('I': LTS) spell. |
| `dict_MatchHyphenatedExcept` / `dict_ExceptDictFind` / `dict_ApplyExceptPron` / `dict_CountHyphenParts` / `FUN_1000ca30` | exceptdict. **Loader already ported:** `ExceptDict` in `src\lexicon.rs` (untested).<br>**Matching:** join morph spellings with '-', longest match wins. For accent 'X', retry with "h'". Pour the value into consecutive morphs at 'd' boundaries, with flags = 2 (u16). |
| `FUN_1000cc60` | Word-list window search over morph spellings (modes 'L'/'R'/'B'; 'B' halves the window). **Returns the list index.** Like `tagger::phrase_in_window` (which works over candidates and returns a position). |
| `FUN_10008580` | "Verb context" for use/close (`let`, modal pairs via `FUN_10008440` = `tagger::modal_pair`, `do`, `to`). **Quirk:** it also tests morph0/morph1 for a modal pair. |
| `FUN_10008550` | Number word (L_NUMWORD). |

**Helpers already in Rust:**
- `lts::{letter_to_sound, spell, lts_valid, lts_hyphenated, letter_vowel, phone_vowel, is_letter, d5e}`
- `lookup::pron_for_pos` (for `FUN_10003f10`)
- `tagger::{bsearch_ci, strnicmp, no_lower, modal_pair(private)}`
- `lexicon::{stricmp, strcmp, decode_payload}`
- `tables::L_*` (generated; add lists in the DLL-table generator, never by hand)

`FUN_100560a0` mode 0x53 is a case-sensitive bsearch. A negative count means a linear search.

#### 2.2 wh flag `FUN_1000e990` and function words `FUN_1000dfc0` — both done, see §0.2 for the full writeup

- `e990`: if the last morph's cat==3 ('?'), scan with `FUN_1000e570` (and `e3a0`/`e430`/`e4b0..e540`, dumped). Result 0x50 sets ws+0x429a4=1; 0x4e stops the scan.
- `dfc0` runs over the WA records:
  - `FUN_1000e160` joins up to 5 following 'A'/'S'("dash") tokens and looks them up with `dict_GetTppField(...,'F','I')`.
  - Otherwise it looks up the word with `dict_GetTppField(...,'G','I')` (TPP G class → WA+0x25, via `FUN_10064645`).
  - Then wab.txt2 index+1 → WA+0x26 (`FUN_100035d0` = `Txt2::find`). `FUN_1000e0c0` also sets WA+0x20=1 for the joined span.
- Ground truth: the `fw_in_wa` / `fw_out_wa` events (function words) and `wh_out` (wh flag).
- `dict_GetTppField 0x1003a7b0`'s field-scan half is independently cross-checked over all 31,550 TPP payloads (`tpp_field_equivalence`); this was the LEXICON open question "TPP G classes" and is now closed.

#### 2.3 Remaining steps — done, see §0.3 (merge) and the state table (chain); only the coverage pass (top-of-file HANDOFF item 1) is still open

1. ~~`fe_LinguisticAnalysis 0x1003e240` merge.~~ Done -- `src\ling\merge.rs`, §0.3.
2. ~~End-to-end chain: `wa_in` → … → `ctx_out`.~~ Done -- `chain_stage`.
3. **The rest of this document** to this project's own `README_TECHNICAL.md` depth (every function, struct field, table, confirmed/inferred, dated experiments) is still open, lower priority than the coverage pass.

### 3. Prosody-agent findings in my scope (credit: `PROSODY.md`) — ALL PORTED

| Finding | Where in Rust | Status |
|---|---|---|
| Syllable records are written by `FUN_10012c70` → `FUN_10013c00` → `FUN_10013f30` / `FUN_10013da0`: b0 nucleus stress, b1 shape (0 V, 1 CV, 2 VC, 3 CVC, 4 no nucleus), b2 position 1/2/3. The per-phone onset/nucleus/coda array is at W+0x36a. | `src\ling\syllab.rs` | Parity via `breaks_out` 730/730 |
| Legal-onset list typos (`S TR`, `S KR`, `SH `) reproduced, so no word-internal 3-consonant onset is legal. | `ONSETS` in `tables.rs` (generated from 0x10079a4c) | Reproduced |
| Allophone pass `FUN_10017100` (codes 0x46–0x53). | `context::allophones` | Parity via `ctx_out` 730/730 |

### 4. fedump "hang" (2026-09-23): harness bug, not an engine bug

The lead found 20 orphaned `fedump.exe 7 …` processes, one batch starting with "The overlook was on the table." Findings:

- **No engine hang.** Every text of both corpora finishes in <1 s, alone and in the capture's batches, with 0 timeouts (checked with a pair of Frida-free timeout-check scripts, not part of this repository).
- **Cause.** The Frida hook blocks in `ExitProcess` until Python posts `flush`. When the Python host died (tool timeout) or missed the exit message, children waited forever.
- **Fixes in the capture driver:**
  - Children join a kill-on-close job object.
  - A batch completes on its last `end` event.
  - Timeout is 180 s; `--jobs` is capped at 12.
- **Result:** the full recapture now takes 14 s.

### 5. File map

| Path | Content |
|---|---|
| `src\ling\mod.rs` | `Wa`/`Ss`/`Ws` byte structs with engine offsets (`war`, `wr`, `syl`, `mr` modules), tagged pointers, `Tib`, `WordStrings` |
| `src\ling\lookup.rs` | `FUN_1000d190` token lookup, `FUN_1000d640` split, FillMorph, `pron_for_pos` (`FUN_10003f10`), `casing` (`FUN_1000fd20`) |
| `src\ling\tagger.rs` | Homograph tagger (rules, fallback, poly.tree3 votes) + string helpers |
| `src\ling\analysis.rs` | `copy_morphs`, `tag_and_lookup` (lookup+copy), `tag_and_pronounce` (the full `fe_TagAndPronounce`: lookup+copy+morphology+wh-flag+funcwords) |
| `src\ling\morph.rs` | `FUN_10007520` morphology driver and callees (see §0.1/§2.1) |
| `src\ling\funcwords.rs` | `mark_function_words` (`FUN_1000dfc0`) + `wh_flag`/`wh_scan` (`FUN_1000e990`/`FUN_1000e570`) (see §0.2) |
| `src\ling\merge.rs` | `merge` (`fe_LinguisticAnalysis`'s own merge loop, called after `tag_and_pronounce`) (see §0.3) |
| `src\ling\lts.rs` | LTS, spell, cluster validity (chc_sort), hyphen LTS |
| `src\ling\words.rs` | `fe_BuildWordPhoneRecords` |
| `src\ling\syllab.rs` | Syllabifier (see §3) |
| `src\ling\breaks.rs` | Phrase table + engbi break CART + `fe_AssignPhraseBreaks` |
| `src\ling\context.rs` | 7-byte contexts, allophones, `fe_BuildPhoneContexts` |
| `src\ling\tables.rs` | **Generated** by a DLL-table extraction script (pefile reads of vt_eng.dll, not part of this repository). Do not edit. |
| `src\lexicon.rs` | Hash dicts (emb, tpp), key encoding, payload decode, txt2, ExceptDict (new, untested), `Lexicon::open` |
| `tests\feling_parity.rs` | Stage parity tests: `lookup_stage`, `copy_stage`, `lts_calls`, `morph_stage`, `funcwords_stage`, `wh_stage`, `tpp_field_equivalence`, `merge_stage`, `chain_stage`, `record_stages` |

None of the capture harness, corpus files, or table/documentation generators used to build and verify
this port are part of this repository. For reference, what they covered: Frida hooks emitting
`wa_in`/`wa_out`, `lookup_out`, `copy_out`, `morph_out`, `wh_out`, `fw_out`, `fw_in_wa`/`fw_out_wa`
(WA records + ws pointer), `tp_out`, `words_out`/`breaks_out`/`ctx_out` (SS + TIB arrays),
`lts`/`spell`/`lts_hy`, and a once-per-request `offset_maps` event (from `fe_InitialTextProcess`:
`map_b`/`map_c`, the `wr::START`/`wr::END` VTML/fullwidth remap tables -- see the `wr::START`/`wr::END`
note under §0); a Frida + fedump mode-7 batch driver, with an opt-in deep-trace mode that logs any
function's args (int32 + raw-bytes preview) and return value on every call -- the live-tracing method
that closed both the `"ers"` and camelCase gaps this pass, and is generally preferred over re-deriving
control flow from the decompile when the asm is ambiguous; a 42-text base corpus plus a 664-text/
4,740-word corpus generated from it; and DLL-table, decompile-dump, asm-format, and function-inventory
generator scripts.

### 6. Gotchas

- `FUN_1001c2c0` is stricmp through TOLOWER with an **unsigned** index. Inline 2-byte compare loops in the decompile are **case-sensitive** strcmp (e.g. "UP", `FUN_100091b0`'s 've/'VE pairs).
- `D5E` is shared:
  - `D5E[TOLOWER_S[c]]` = letter is a vowel.
  - `D94 = D5E+27` = phone is a vowel (valid for codes 1..0x45).
  - `F5A = D5E+254` = LTS code → phone.
- Many engine buffers are C strings modified in place. Model the **same buffer** across a chain of calls, because failed calls leave data that later calls test (see `a140`/`c040`).
- `copy_out` comparisons exclude flags; the `morph_out` comparison must include the u16 flags.
- Never kill python processes in bulk (the user owns other processes). Kill only PIDs you spawned.
- No heredocs. Write scripts with the Write tool and run them. Use 4-space indentation and K&R braces.

---

## 1. Reference

The full function/struct/table reference is still to be written from the Rust doc comments, the disassembly (not part of this repository), and the session notes above. The key layouts (WA 0x94 records, WS candidates 0x554 / morphs 0x70, SS word records 0x3c0 / syllables 0x1e) are defined with engine offsets in `src\ling\mod.rs` and `lookup.rs::cand`. The POS tag name table is at 0x100779b0 (7-byte stride, 48 tags, `tables::POS_NAMES`), which resolves the LEXICON question about the POS indices.
