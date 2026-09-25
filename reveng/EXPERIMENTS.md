# Experiment Log

Dated parity runs and negative results across every stage, pulled from the stage docs' own
experiment sections and day-to-day project notes. This is a narrative log — TRIED/FAILED, TRIED/WORKED,
NOT-YET-TRIED — not a status table; see [`README.md`](README.md)'s stage table for current pass
rates and [`DLL_ANALYSIS.md`](DLL_ANALYSIS.md) for the function-level detail behind each fix.

## Linguistic front end (`FRONTEND_LING.md`)

**2026-09-23 → 2026-09-24, full corpus 730/730 sentences.**

- **TRIED/WORKED** — `fe_TagAndPronounce` bracketing. An earlier pass assumed the `wa_in`/`wa_out`
  Frida hooks bracketed only lookup+copy; re-reading the decompile showed `fe_TagAndPronounce`
  (`0x1000e2f0`) also runs morphology, the wh-flag pass and function-word marking in one call. Fixed
  by re-decomposing into `tag_and_lookup` + `morph::morphology` + `funcwords::wh_flag` +
  `funcwords::mark_function_words`, composed in that exact order in `ling::analysis::tag_and_pronounce`.
- **TRIED/FAILED then FIXED** — the `"ers"` suffix stem-length fallback. Naively swapping
  `stem(3)`→`stem(2)` fixed "farers" but broke 11 other previously-passing "ers" words — caught
  immediately by running the *whole* suite, not just the new case. Live-traced `FUN_1000cb30`
  ("`cb30`") to find the real order: `stem(1)` → `stem(2)` → `stem(3)`, sequential fallback, not a
  single substitution. Lesson generalized in the handoff notes: "a plausible-looking fix that works
  on the ONE new capture can break others that were already passing."
- **TRIED/WORKED** — camelCase splitting (`FUN_1000c3a0`/`c860`/`8cc0`), previously a stub. The
  apparent puzzle ("why doesn't 'GoodNight' split?") turned out not to be a lexicon-selection
  mystery — `dict_LookupLexicon`'s "camelCase lexicon" is the *same* base as the main token lookup.
  "GoodNight" doesn't split because it's already a whole-word lexicon entry, short-circuited before
  camelCase logic ever runs. Found the extra `0x10` flag bit (call site sets `0x18`, not just `0x8`)
  by `--deep`-tracing `*flags` before/after the call directly, rather than guessing from the
  decompile.
- **TRIED/FAILED then FIXED** — `clitic_inword` (`0x1000c040`) over-matched: a clitic pattern
  (`'ers`/`'ee`/`'er`/`'n`) matching anywhere in the word silently discarded trailing text. Found via
  corpus line `tail_001` ("Fish'n'chips and mac'n'cheese are rock'n'roll classics.") and
  `--deep`-tracing the function itself showing real `ret=0` for both compound words. Fix: require the
  match to reach the end of the word.
- **TRIED/FAILED then FIXED** — `lts::spell`'s double-join bug and the B80-hyphen `hy` condition bug.
  Both found via one deliberately harder combined-feature corpus line (`suffix_004`, "well-known" +
  "J.R.R") *after* 3 easier lines had already passed with zero code changes. Methodology lesson
  written into the doc verbatim: "an easy corpus line that passes doesn't mean the surrounding code
  is bug-free, only that it's bug-free for words like that."
- **TRIED/FAILED (3 rounds) then FIXED** — `wr::START`/`wr::END` (word character offsets). Round 1:
  assumed a missing write in `fe_BuildWordPhoneRecords`, added one — fixed sentence 0 only. Round 2:
  discovered the true writer is `fe_NextSentence` (`0x10022dc0`), which walks a running
  per-request `TIB+4` cursor *after* the ling stage returns — required threading a real cursor
  through the test harness. Round 3: cursor-only fix worked for plain multi-sentence text but broke
  every VTML capture — discovered a second remap layer is needed (`TIB+0x47794`/`+0x47798`
  offset maps, written once per request by `fe_InitialTextProcess`). Final fix: new `TextPos` API
  (`cursor` + optional `map_start`/`map_end` slices) threaded into `build_word_phone_records`,
  confirmed via `--deep` trace of `0x10022dc0` and `0x10016c90` across a 3-sentence request plus a
  dedicated VTML capture (`vtml_pitch.json`).
- **NOT PORTED (deliberately)** — `merge.rs`'s `0xa2`/`0xfe` spelling-marker branch: asm-confirmed to
  exist but no capture in the corpus exercises it. Left unported rather than guessed, per the
  project's "prove on captures, not decompile alone" rule.
- **RESOLVED (static analysis, 2026-09-24, no corpus line involved)** — `u_token`'s non-`'Y'` modes
  (mode `'S'`, sub-token clitic list): a full-binary capstone scan of every call to `fe_EmitWord`
  (246 call sites) found that `war::U_FLAG='U'` is written by exactly one place, the `symbols`
  normalizer's apostrophe-continuation of a word whose `war::CLASS` (`+0x28`) is already `0x19` —
  and class `0x19` is produced only by `userdict`'s emission path, which was unported at the time
  and, per the installed `userdict_eng.csv` (`README_TECHNICAL.md` §4), has zero alias entries to
  trigger it anyway. Conclusion: with the shipped default dictionary no plain text reaches it,
  **but it is a real engine path**. User dictionaries are user-supplied (`VT_LOAD_UserDict_ENG`
  loads any CSV, and this install already has `userdict_eng_fixed.csv` and `national.csv` beside the
  default), so an `A` alias entry followed by an apostrophe suffix triggers it. **Update:**
  `userdict`'s emission (text front end, dispatch order 0, `0x1005deb0`) is now ported
  (`src\text\norm.rs::userdict`); test it with a purpose-built CSV. Full derivation in
  `FRONTEND_LING.md`'s root-cause note. Mode `'S'` (vs. `'Y'`)
  is still unidentified; resolve it the same way, using the same kind of ad hoc scan scripts
  (not part of this repository).
- **Harness bug, not an engine bug (2026-09-23)**: a full-corpus recapture stalled indefinitely after
  a new hook was added. Root cause: 10 orphaned `fedump.exe` processes accumulated from prior runs,
  not a genuine hang. Fixed by killing the orphaned PIDs (correlated by spawn timestamp); the pending
  batch completed immediately afterward.

## Text front end (`FRONTEND_TEXT.md`)

**Running coverage log, 2026-09-23 → 2026-09-24** (percentage = decompiled-line coverage /
word-record coverage, `cargo test --release --test fetext_parity`):

| Date | Change | Line % | Record % |
|---|---|---|---|
| 2026-09-23 | tokenizer, driver, punct_runs + SBD, word | 21.3% | 32.3% |
| 2026-09-23 | + symbols | 25.4% | 34.4% |
| 2026-09-23 | + numword + number core | 41.9% | 54.2% |
| 2026-09-23 | + date + units | 59.4% | 64.6% |
| 2026-09-23 | + currency (single-designator patterns) | 63.7% | 67.2% |
| 2026-09-23 | + time (all but bare `N:N`) | 66.2% | 68.2% |
| 2026-09-23 | fix `numwords::digits` zero-flag inversion | 68.2% | 70.1% |
| 2026-09-23 | + n15 UGC zone codes | 68.8% | 70.6% |
| 2026-09-23 | + numexpr (all 5 operators) | 74.2% | 80.2% |
| 2026-09-23 | + address PO box | 74.7% | 80.4% |
| 2026-09-23 | + digits_a + digits_b | 74.5%* | 81.2% |
| 2026-09-24 | + compound (apostrophe-suffix insertion only) | 74.7% | 82.5% |
| 2026-09-24 | + address city/state/country | 74.6%* | 82.5% |

(*a small line-% dip on a record-% gain means the new code path is short relative to what it now
handles correctly — expected, not a regression.)

- **TRIED/FAILED then FIXED** — `numwords::digits` (`0x10060fc0`) zero-flag inversion. Ghidra's
  pseudo-C for the `'0'` switch case showed an empty `{}` body for the flag-clear arm and the
  `"zero"` load in the `else` — read at face value as "clear repeats the previous word", which a
  prior checkpoint's notes took as a genuine engine quirk and documented as such. **The raw
  disassembly said otherwise**: the `JZ` at `0x10061052` on the flag-clear path jumps to a load of
  `"oh"`, not to an empty fallthrough — Ghidra's CFG rendering had put the two case bodies in
  visually swapped `if`/`else` slots. Confirmed live: `"220"` after `"at "` produces `"two two oh"`,
  not the repeat-previous behavior. This single fix moved parity by **+29 lines / +288 records** —
  more than either of the two surrounding normalizer ports individually, because a clear zero flag
  is the common case.
- **TRIED/FAILED then RESOLVED (not a bug)** — `"+1 402 555 1234"` was suspected to need special
  phone-context handling in `numexpr`'s `+` branch. Re-reading `FUN_100420f0`'s actual `+` branch
  showed no phone-context logic at all (it unconditionally emits "plus"); confirmed live via a
  `--call` trace that `phone` (dispatch order 8, unported at the time) claims the string before
  `numexpr` (order 15) ever sees it. Conclusion: nothing to fix in `numexpr` — this resolved once
  `phone` was ported (it now is).
- **NOT PORTED (low/zero corpus coverage, documented rather than guessed)**: the EUC-KR `×`
  multiplication symbol (2 bytes) as a `*`-equivalent; several decompiled date/ordinal `-` sub-cases
  (the `"through"` output, plus a couple of context-word checks in the `-` branch's tail) that never
  fired on either corpus.
- **OPEN, large scope, at the time this was written**: `address`'s other 3 sub-recognizers (street-number, street/highway — each
  `numexpr`-sized, 350-600 decompiled lines, mutually recursive, sharing lookup tables); `compound`'s
  bracket/e-commerce feature; `url`, `phone`, `fraction` (bespoke non-WA-stride scratch record,
  unanalyzed `FUN_1003cbb0` sub-helper), `math`; `n15`'s other 2 sub-recognizers;
  `fe_InitialTextProcess` (offset maps, VTML tag mode, full-width map). **Update (2026-09-24, later
  session):** all of the above landed except `fraction`'s CP1252/EUC-KR glyph shapes (still open)
  and `math` (confirmed dead code — its entry gate can never be satisfied, not a missing port); see
  `FRONTEND_TEXT.md`'s own HANDOFF and per-normalizer sections for detail.

## Lexicon (`LEXICON.md`)

- **TRIED/WORKED** — `dict_GetTppField`'s field-scan logic was independently cross-checked over
  **all 31,550 TPP payloads** (`tpp_field_equivalence` test) rather than a corpus sample, closing
  what had been an open question ("TPP G classes") in earlier notes. 0 discrepancies.
- **RESOLVED via outside review correspondence** — see the "Outside review" section below; the
  lexicon format itself needed no correction, only the unit-index/`.upm` reader downstream.

## Prosody (`PROSODY.md`)

**2026-09-23, `cargo test --release --test prosody_parity`: everything matched on the first run.**
No x87 corner cases were hit — the only float operation (the duration quantizer) is exact in `f64`
for every `s16` input, so there was no store-by-store rounding hunt needed for this stage.

| Check | Source | Result |
|---|---|---|
| phone durations | 42 main captures | 2713/2713 |
| syllable tones | 42 | 1047/1047 |
| raw F0 contours (pre-smoothing) | 42 | 1047/1047 |
| smoothed F0 contours | 42 | 1047/1047 |
| target features {dur_code, f0_a, f0_b} | 42 + word target lists | 3269/3269 |
| per-word rate seeding | 51 stage captures | 73/73 sentences |
| span application (6 arrays + trailing pause + 8 cursors) | 51 | 73/73 |
| sentence pause resolution | 51 | 73/73 |
| pause gaps (samples + continuation flag) | 51 | 735/735 |
| silence record lengths | 50 texts with records | 50/50 |
| trailing silence | `pause_start` | 1/1 |

Open (documented, not experiments): tone-label 0-4 → accent-type mapping is unknown (only feeds the
F0 trees, doesn't block parity); VTML kinds 5/6 are clamped but never applied in this stage, kind 7's
producer (front-end side) unconfirmed; allophone code names 0x46-0x53 are inferred from rule context,
not directly observed; the `W+0x2df==0x0c` normalizer-pause override's interplay with span pauses is
parity-covered but not independently isolated.

## Unit selection (`UNITSEL.md`)

**2026-09-23:**

| Run | Corpus | Result |
|---|---|---|
| Baseline | main capture set (42 captures, 93 phrases, 3,269 targets) | 42/42 exact, 0 mismatching fields |
| + EXP-domain cases | unit-selection capture set (10 captures incl. 5 `vtml_emotion`; 108 phrases, 3,697 targets total) | 52/52 exact |
| **Negative control** | ETC penalty deliberately changed 10 → 11 | **0/52** — 6,465 candidate fields, 2,252 beams, 19 paths differ |

The negative control is the load-bearing check here: it proves the parity harness actually detects a
real algorithmic change (a 1-unit cost-table perturbation) rather than passing vacuously.

## Waveform (`WAVEFORM.md`)

**2026-09-23, experiment log (numbered as in the source doc):**

1. First run, OLA fast path + smoother without PSOLA: **1/42 exact**. Every capture's first
   mismatch sat inside a record whose smoother decision was `(2,0)`, with ±1-3 sample errors spread
   over the whole unit. Cause: in the `prev+1==cur` branch, the port tested `lbl(cur).flags` instead
   of `lbl(prev).flags` (`0x1002d395`). After the fix: **41/42** — the remaining capture
   (`vtml_speed`) needed PSOLA, not yet ported at this point.
2. PSOLA ported from the disassembly (§7.3): **42/42**.
3. Parameter sweep (50 captures, pitch/speed/volume/emphasis variation): **50/50** on the first
   attempt.
4. Outside review's "three `.upm` views" correction applied (`Bank::pitch_marks` switched to
   `pm1+pm2-1` bytes, fixing an off-by-one): still **42/42 and 92/92** — the fix corrected an
   internal representation without changing observable output, meaning the bug hadn't yet been
   exercised by any capture in a way that broke parity, only latent.
5. Record builder ported: **218/218** phrases. Chunk model: **92/92**.
6. Formats, first run: **2 mismatches**, both root-caused and fixed:
   - fmt 9 (AU): engine file had 4 header bytes where the port wrote 28 — root cause is a documented
     AU position-cache bug in the engine itself (§8.6), reproduced rather than "fixed".
   - fmt 3 (DADPCM): port emitted `(2n+3)>>2` bytes/chunk; correct is `n/2` bytes with the odd sample
     dropped — `(bytes + (bytes>>31 & 3)) >> 2` is a signed divide-by-4, not round-up.
   - After both fixes: **1,196/1,196**.
7. Lip-sync log, first run: **0/92** — engine writes CRLF line endings (text-mode `fopen`). After
   matching that: **92/92**.

Final parity table (all 2026-09-23):

| Suite | Scope | Result |
|---|---|---|
| `wave_parity_default` | captured records → PCM | 42/42 bit-exact |
| `wave_parity_params` | captured records → PCM (+emphasis) | 92/92 bit-exact; chunk sizes 92/92 (459 chunks) |
| `records_parity` | selection + sentence → records | 218/218 phrases, 9,317/9,317 records (all fields but the stale `+0x2a`) |
| `wave_parity_from_selection` | selection → records → PCM | 92/92 bit-exact |
| `formats_parity` | PCM + chunks → 13 outputs | 1,196/1,196 byte-exact |
| `lipsync_parity` | selection → records → sync → log | 92/92 byte-exact |

Open (not experiments, documented gaps): `wav_PlayStart`/`waveOut` callback details not ported
(call-level only); buffer-API SYNCINFO struct layout not decoded (ring itself is ported); IMA ADPCM
writer (`FUN_1001c8b0`, tag `0x11`) unreachable through public fmt codes, not ported; `.dtt` logging
path documented but the dump format belongs to unit selection, not waveform.

## Outside review correspondence

An independent outside review reverse-engineered the same unit-index format against `vt_pau.dll`
under Wine and shared its findings. This project's response, point by point:

- 0 of the outside review's claims were refuted; several were extended with meaning the outside
  note had left as "unnamed". Key correction going *the other direction*: the outside note read
  `.upm` spans as `first_count + second_count` bytes; this project's own reader had the same
  off-by-one (`pm1+pm2` instead of `pm1+pm2-1`). The outside note's "the two sides share one byte"
  framing was the thing that caught it — fixed in `src/bank.rs`, and in the matching C unit-bank
  exporter used during verification (not part of this repository).
- The unit-index claims were verified independently from this project's own primary-record captures
  (3,269 runtime reads) and a full structural scan of all 580,474 units in all 4 banks. Additionally
  resolved the outside note's "unnamed bytes 16-18" and the `pos14`-for-mode-2 question the outside
  note hadn't reached.
- The same claims were cross-checked against 12,463 captured unit records (8,159 whole, 2,152 each
  half). Corrected the outside note's implicit assumption that bytes 16-18 were generic
  "half-period marks" (a mischaracterization also present in this project's own earlier
  `WAVEFORM.md` draft) — they are specifically the first/shared/last pitch periods, used as
  cross-fade overlap lengths.
- A follow-up question from the outside review about the Shorten decoder's `nmean` parameter was
  resolved by disassembly (`0x10001c14`-`0x10001c43`): the engine averages the **last 4** block
  means, not 1. A last-mean-only model gives wrong PCM on 19,272/580,474 units (3.3%) — proven with
  a dedicated whole-bank A/B probe (not part of this repository) rather than argued from the
  disassembly alone, since the disassembly plus a naive corpus sample (`gen` unit 0) could not by
  itself distinguish the two models (unit 0 never executes the DIFF0 branch that reads `coffset`).

Net effect on this project's own code from the exchange: one real off-by-one fixed
(`pm1+pm2` → `pm1+pm2-1`), one mischaracterization corrected (bytes 16-18's role), zero regressions
in existing parity (42/42, 92/92 held throughout).
