# Experiments

Parity runs, negative controls and root-caused fixes, by stage. Current pass rates are in
[README.md](README.md); function-level detail is in [DLL_ANALYSIS.md](DLL_ANALYSIS.md) and the stage docs.

Lessons that held across stages:

- Run the whole suite after every fix; a change that fixes the new capture can break passing ones.
- A passing easy corpus line proves little; add one hard combined-feature line per feature.
- Where the decompile is ambiguous (VC6 stack-slot reuse, Ghidra swapping `if`/`else` arms), read the
  disassembly or trace the live engine.
- Reproduce engine bugs; do not fix them.

## Linguistic front end ([FRONTEND_LING.md](FRONTEND_LING.md))

Corpus 730/730 sentences, 2026-09-23 to 2026-09-24.

| Change | Cause and fix |
|---|---|
| `fe_TagAndPronounce` bracketing | The call also runs morphology, wh-flag and function-word marking. Ported as `tag_and_lookup`, `morph::morphology`, `funcwords::wh_flag`, `funcwords::mark_function_words` in that order (`ling::analysis::tag_and_pronounce`) |
| `"ers"` suffix stem fallback | Swapping `stem(3)` for `stem(2)` fixed "farers" but broke 11 other words. `FUN_1000cb30` (live trace) tries `stem(1)`, `stem(2)`, `stem(3)` in sequence |
| camelCase split (`FUN_1000c3a0`/`c860`/`8cc0`) | Parts use the same lexicon context (`ws+0x48128`) as the whole-word lookup. "GoodNight", "JavaScript", "YouTube", "FedEx", "iPhone" do not split because they are whole lexicon entries; "PowerPoint", "LinkedIn" have an empty whole-word payload and do. Call site sets flags `0x18` |
| `clitic_inword` (0x1000c040) | Over-matched and dropped trailing text (corpus `tail_001`, "Fish'n'chips..."). Fix: the match must reach the end of the word |
| `lts::spell` double join and B80-hyphen `hy` condition | Found with `suffix_004` ("well-known", "J.R.R") after three easier lines passed. Joiner `0x64` is decided only by look-ahead; `should_spell` with a separator also sets `0x08` |
| `wr::START`/`wr::END` (word char offsets) | Written by `fe_NextSentence` (0x10022dc0), not `fe_BuildWordPhoneRecords`: walks the per-request `TIB+4` cursor after the linguistic stage, then remaps through the `TIB+0x47794`/`+0x47798` offset maps written by `fe_InitialTextProcess` (needed for VTML). Port: `TextPos` (`cursor` + optional `map_start`/`map_end`); checked with `vtml_pitch.json` |
| `merge.rs` `0xa2`/`0xfe` spelling-marker branch | Present in the asm, no capture reaches it; not ported |
| `u_token` non-`'Y'` modes | Static scan of all 246 `fe_EmitWord` call sites: `war::U_FLAG='U'` is written only by the `symbols` apostrophe continuation of a word of class `0x19`, which only `userdict` emission (dispatch 0, 0x1005deb0) produces. Reachable only with an `A` alias in a user dictionary; `userdict` is ported (`src/text/norm.rs::userdict`). Mode `'S'` is unidentified |

## Text front end ([FRONTEND_TEXT.md](FRONTEND_TEXT.md))

Coverage during the port (`cargo test --release --test fetext_parity`), 2026-09-23 to 2026-09-24:

| Step | Line % | Record % |
|---|---|---|
| tokenizer, driver, punct_runs + SBD, word | 21.3 | 32.3 |
| + symbols | 25.4 | 34.4 |
| + numword + number core | 41.9 | 54.2 |
| + date + units | 59.4 | 64.6 |
| + currency (single designator) | 63.7 | 67.2 |
| + time (all but bare `N:N`) | 66.2 | 68.2 |
| fix `numwords::digits` zero flag | 68.2 | 70.1 |
| + n15 UGC zone codes | 68.8 | 70.6 |
| + numexpr (5 operators) | 74.2 | 80.2 |
| + address PO box | 74.7 | 80.4 |
| + digits_a, digits_b | 74.5 | 81.2 |
| + compound (apostrophe suffix), address city/state/country | 74.6 | 82.5 |

Later sessions ported the rest (street, highway, `url`, `phone`, `fraction`, `n15` remainder,
`fe_InitialTextProcess`); the final text stage is bit-exact. `math` is dead code in 2014 (its entry gate
cannot be satisfied); `fraction`'s CP1252/EUC-KR glyph shapes remain open.

- `numwords::digits` (0x10060fc0): Ghidra showed the `'0'` case with the flag-clear arm empty and `"zero"`
  in the `else`; the disassembly (`JZ` at 0x10061052) loads `"oh"` on the flag-clear path. `"220"` after
  `"at "` is "two two oh". This fix was worth +29 lines / +288 records.
- `"+1 402 555 1234"`: no `numexpr` fix needed; `phone` (dispatch 8) claims the string before `numexpr`
  (dispatch 15). `numexpr`'s `+` branch always emits "plus".
- Not ported (no corpus coverage): EUC-KR `×` as `*`; a few date/ordinal `-` sub-cases ("through" output).

## Lexicon ([LEXICON.md](LEXICON.md))

- `dict_GetTppField` checked over all 31,550 TPP payloads (`tpp_field_equivalence`): 0 discrepancies;
  closed the "TPP G classes" question.

## Prosody ([PROSODY.md](PROSODY.md))

2026-09-23, `cargo test --release --test prosody_parity`: all matched on the first run. The only float
operation (the duration quantizer) is exact in `f64` for every `s16` input.

| Check | Source | Result |
|---|---|---|
| phone durations | 42 captures | 2713/2713 |
| syllable tones | 42 | 1047/1047 |
| raw F0 contours | 42 | 1047/1047 |
| smoothed F0 contours | 42 | 1047/1047 |
| target features {dur_code, f0_a, f0_b} | 42 + word target lists | 3269/3269 |
| per-word rate seeding | 51 | 73/73 sentences |
| span application (6 arrays, trailing pause, 8 cursors) | 51 | 73/73 |
| sentence pause resolution | 51 | 73/73 |
| pause gaps (samples + continuation flag) | 51 | 735/735 |
| silence record lengths | 50 texts | 50/50 |
| trailing silence | `pause_start` | 1/1 |

Open: tone label 0-4 to accent-type mapping unknown (does not block parity); VTML kinds 5/6 clamped but
not applied here; kind 7 producer unconfirmed; allophone names 0x46-0x53 inferred from rule context.

## Unit selection ([UNITSEL.md](UNITSEL.md))

2026-09-23:

| Run | Corpus | Result |
|---|---|---|
| Baseline | 42 captures, 93 phrases, 3,269 targets | 42/42, 0 mismatching fields |
| + EXP-domain cases | 10 more captures (5 `vtml_emotion`); 108 phrases, 3,697 targets | 52/52 |
| Negative control | ETC penalty changed 10 to 11 | 0/52 (6,465 candidate fields, 2,252 beams, 19 paths differ) |

The negative control shows the harness detects a one-unit cost-table change.

## Waveform ([WAVEFORM.md](WAVEFORM.md))

2026-09-23, in order:

1. OLA fast path + smoother, no PSOLA: 1/42. The `prev+1==cur` branch tested `lbl(cur).flags` instead of
   `lbl(prev).flags` (0x1002d395). After the fix 41/42; `vtml_speed` needed PSOLA.
2. PSOLA ported from the disassembly: 42/42.
3. Parameter sweep (pitch/speed/volume/emphasis), 50 captures: 50/50.
4. `.upm` span fixed to `pm1+pm2-1` bytes (outside review): 42/42 and 92/92 unchanged (latent bug).
5. Record builder 218/218 phrases; chunk model 92/92.
6. Formats, first run 2 mismatches: AU (engine writes a 4-byte header because of a position-cache bug,
   reproduced, WAVEFORM.md section 8.6) and DADPCM (`n/2` bytes per chunk, odd sample dropped; the
   `(bytes + (bytes>>31 & 3)) >> 2` idiom is a signed divide by 4). Then 1,196/1,196.
7. Lip-sync log: 0/92 until CRLF line endings (text-mode `fopen`); then 92/92.

| Suite | Scope | Result |
|---|---|---|
| `wave_parity_default` | records to PCM | 42/42 |
| `wave_parity_params` | records to PCM, with emphasis | 92/92; chunk sizes 92/92 (459 chunks) |
| `records_parity` | selection + sentence to records | 218/218 phrases, 9,317/9,317 records (all fields but the stale `+0x2a`) |
| `wave_parity_from_selection` | selection to records to PCM | 92/92 |
| `formats_parity` | PCM + chunks to 13 outputs | 1,196/1,196 |
| `lipsync_parity` | selection to records to sync to log | 92/92 |

Open: `wav_PlayStart` callback details; buffer-API SYNCINFO layout; IMA ADPCM writer (`FUN_1001c8b0`, tag
`0x11`, unreachable through public format codes).

## Outside review

An independent review reverse-engineered the unit index against `vt_pau.dll` under Wine. None of its claims
was refuted.

- `.upm` spans: the review's "the two sides share one byte" showed this project's reader had `pm1+pm2`
  instead of `pm1+pm2-1`. Fixed in `src/bank.rs`.
- Unit index verified from 3,269 runtime primary-record reads, a scan of all 580,474 units in 4 banks, and
  12,463 captured unit records (8,159 whole, 2,152 per half). Bytes 16-18 are the first, shared and last
  pitch periods (cross-fade lengths), not generic half-period marks.
- Shorten `nmean`: disassembly (0x10001c14-0x10001c43) shows the mean of the last 4 block means, not 1.
  A whole-bank A/B decode confirmed it (wrong on 19,272/580,474 units with `nmean=1`); unit 0 alone cannot
  tell them apart since it never runs the DIFF0 branch that reads `coffset`.

## Voices and 64-bit builds

`tests/voices_parity.rs`: Paul, Kate, Julie, James, each against the 32-bit `vt_eng.dll` (voice name
patched) and the voice's own 64-bit DLL; `Engine::x64` off and on, both bit-exact. The x64 DLLs differ
only in single-precision `float` evaluation in two places in unit selection (see [UNITSEL.md](UNITSEL.md)).

## 2009 engines (2026-10-05)

Per-build capture hooks were validated first: hooked and unhooked engines produce identical WAVs
(Julie09 2971/2971, Paul09 3170/3170, Violeta09 5532/5532; 0 Frida errors).

| Stage | Result |
|---|---|
| Prosody | 100% on all 11 record types for all three builds (durations Julie09 100630, Paul09 100775, Violeta09 455427) |
| Waveform | PCM from captured records: Julie09 92/92 + 42/42, Paul09 92/92 + 42/42, Violeta09 1416/1416; records from selection 0 differing (Julie09 9183, Paul09 10739, Violeta09 508661); formats 13/13 on all three; Julie09/Paul09 lip-sync 92/92 |
| Unit selection | 0 mismatches on every target, candidate, beam, path and primary record (Julie09 6,460 + 119,613 targets, Paul09 19,031 + 135,518, Violeta09 977,798) |
| English linguistic | Julie09 and Paul09 100% on 10 stages (words/breaks/contexts 1554/1554, morph 1554, merge 4542, lts 8807 / 8790, spell 2080 / 2085) |
| English text | fetext Julie09 2245/2245, Paul09 2264/2264; initial 2197/2197 each |
| Spanish front end | Violeta09 fetext 1418/1418 (records 89605/89605), analysis 13766/13766, words/breaks/contexts 11034/11034, lts 564/564 |
| `.tree2` | `tree2_parse`: 20 files and 27 `atmt` records parse to the exact byte |
| End to end (`speak_old_parity`) | first 2735/2741 (Paul09) and 2733/2741 (Julie09); after the fixes below 2741/2741 both, Violeta09 1366/1366 |

Last 14 English misses, all in the text front end: date normalizer (context check only for patterns
7/0xc/0xd/0x11/0x15, sticky hint 1, no length<3 split so "AP"/"JA"/"JE" read as months); abbreviation
`norm_n15` (honorifics gated on "Mr"/"Ms", "mrs." falls to the tables, `&`/`and` only); Paul09
`norm_symbols` handles only `(` there; year words >= 10000 read in digit pairs. One rule lacks decompile
support: Julie09 `norm_digits::digits_a` returns -1 for a group that is neither spelled nor digits
("401K-2345-..."). Constants in 2014-addressed modules may still differ on texts no corpus covers.

Open: `.his` selection history (no capture); Spanish lip-sync log (2014 and 2009).

## Harness

A full recapture stalled after a hook was added: 10 orphaned `fedump.exe` processes from earlier runs.
Killing them (matched by spawn time) let the batch finish. Use timeouts and kill-on-close job objects.

## History

- 2026-09-23: prosody 100% on first run; unit selection negative control; waveform 42/42 to 1,196/1,196.
- 2026-09-24: linguistic corpus 730/730; text front end coverage log through compound.
- 2026-09-28: outside review exchange closed; `nmean` = 4 proven.
- 2026-09-29: Violeta 2013 end to end 1327/1327.
- 2026-10-05: 2009 engines bit-exact end to end (Paul09, Julie09 2741/2741; Violeta09 1366/1366).
