# Waveform generation

Everything between unit selection and the bytes the caller receives: unit and pause records, `.dat` decoding
(Shorten) and `.upm` pitch marks, the join smoother, overlap-add (unmodified) and PSOLA (pitch/rate modified),
output chunking, the emphasis post-filter, all output formats, the word/phone sync ring, the lip-sync log and
`waveOut` playback. Addresses are `vt_eng.dll` 3.11.7.1 VAs (image base 0x10000000); `wav_` names are Ghidra
renames (61 functions). Tags: **[D]** decompile/disassembly, **[E]** proven by the parity tests, **[I]** inferred.
Every stage is bit-exact against the engine on every capture [E].

Port: `src/wave.rs` (records, decode, smoother, OLA, PSOLA, chunking, sync), `src/formats.rs` (emphasis, G.711,
Dialogic ADPCM, WAVE/AU/raw writers), generated `src/wave_tables.rs`. Tests `tests/wave_parity.rs`. The 2009
engines are in [Builds](#builds) and [OLD_PAUL.md](OLD_PAUL.md).

## 1. Call graph

```
VT_TextToBuffer_ENG / VT_TextToFile_ENG / VT_TextToLipSyncLog_ENG / wav_PlayStart
 `- wav_NextChunkWithEmphasis 0x10026870          one chunk (<= 60000 bytes + slack), then emphasis
     `- wav_FillChunk 0x100267d0                 loops phrases/sentences until the chunk is full
         |- FUN_10026630                         next sentence: front end + prosody
         `- wav_SelectAndGeneratePhrase 0x1002c9b0
             |- us_SelectUnitsForPhrase 0x10024980   once per phrase
             `- wav_GeneratePhrase 0x1002bd90         resumable per record
                 |- wav_BuildUnitRecords 0x1002c220   (first call of a phrase)
                 |   |- wav_PauseBeforeWord 0x1002e890
                 |   `- wav_FillUnitRecord 0x1002c120
                 |- wav_BuildSyncEntries 0x1002c530   (first call of a phrase)
                 `- per record: silence -> zeros
                     unit -> wav_SmoothJoins 0x1002d230 (may replace the samples)
                               |- wav_DecodeUnit 0x1002c8b0 - wav_ReadDbFileLocked 0x10025440 - wav_ShortenDecode 0x10001b30
                               |- wav_ReadPitchMarks 0x1002bbd0
                               |- wav_LoadNeighbourUnit 0x1002d1e0 (x2)
                               `- wav_FadeOutPeriod 0x1002d010 / wav_FadeInPeriod 0x1002cdf0
                             wav_DecodeUnit (if not smoothed)
                             wav_ReadPitchMarks + wav_BuildEpochTable 0x1002bc60 (if modified)
                             wav_PsolaUnit 0x1002afb0 - wav_OlaUnitUnmodified 0x1002aac0 (pitch = dur = 100)
                                                      - wav_FindNearestEpoch 0x1002bd20
```

The chunk is converted by the API function (`wav_TextToBuffer{S16,Alaw,Ulaw,Dadpcm}`, `wav_TextToRaw*File`,
`wav_TextToDadpcmFile`, `wav_TextToWave*` via `wav_TextToWaveFile`, `wav_TextToAuFile`), by the lip-sync log
(`wav_FlushLipSyncLog`) or by `waveOut` (`wav_PlayStart` + `FUN_10026d00`).

## 2. Functions

| VA | Name | Role |
|---|---|---|
| 0x10026870 | `wav_NextChunkWithEmphasis` `(ch*, tib*, short *buf)` | `wav_FillChunk`, then the emphasis filter; returns chunk bytes (`tib+0x30`) |
| 0x100267d0 | `wav_FillChunk` | `tib+0x30 = 0`; repeats `wav_SelectAndGeneratePhrase` until `tib+0x40 == 1` (chunk full) or done. At phrase end (`tib+0x40 == 0`) advances `SS+0x47704`, clears `SS+0x47706` and `tib+0xfbd6c`, pulls the next sentence (`FUN_10026630`) when phrases are exhausted |
| 0x1002c9b0 | `wav_SelectAndGeneratePhrase` | stage limit `tib+0x20` (1 stop before selection, 2 no synthesis, 3 selection only); at record 0 runs unit selection, then `wav_GeneratePhrase` |
| 0x1002bd90 | `wav_GeneratePhrase` `(tib*, ch*)` | resumable record loop (`SS+0x47706` = next record), 6 |
| 0x1002c220 | `wav_BuildUnitRecords` `(recs*, tib*, u16 first_word, u16 n_words, ch*)` | 4 |
| 0x1002c120 | `wav_FillUnitRecord` `(rec*, dbentry*)` | 4.3 |
| 0x1002e890 | `wav_PauseBeforeWord` `(tib*, short w)` (= `pro_PauseSamples`) | 4.2 |
| 0x1002c530 | `wav_BuildSyncEntries` | 9 |
| 0x1002c8b0 | `wav_DecodeUnit` `(short *dst, rec*, tib*, ch*)` | 5.1 |
| 0x10001b30 | `wav_ShortenDecode` | 5.2 |
| 0x10001ea0 / 0x100020a0 / 0x10002170 | `wav_UvarGet` / `wav_SvarGet` / `wav_UlongGet` | Rice codes |
| 0x10001fb0 | `wav_BitRefillBE32` | 32-bit big-endian refill from a 1024-byte staging window |
| 0x100020d0 / 0x10002100 | `wav_FixBitshift` / `wav_EmitPcm16` | bitshift and output (top-only clamp at 0x7fff) |
| 0x10025440 | `wav_ReadDbFileLocked` `(h, pos, whence, buf, size, count)` | global critical section 0x100a8400; seeks (`wav_FileSeek`) only if `pos != h+0xc` or `whence != 0`, then `FUN_100254a0` (fread) |
| 0x1002bbd0 | `wav_ReadPitchMarks` | `n_pm = rec+0x20` bytes at `rec+0x1c` from the pool's `.upm`, each widened and doubled |
| 0x1002bc60 / 0x1002bd20 | `wav_BuildEpochTable` / `wav_FindNearestEpoch` | 7.2 |
| 0x1002d230 | `wav_SmoothJoins` `rec* (tib*, ch*, short i)` | 6.2; returns the record or NULL |
| 0x1002d1e0 | `wav_LoadNeighbourUnit` | `us_ReadPrimaryRecord` + `wav_FillUnitRecord` + decode + pitch marks |
| 0x1002d010 / 0x1002cdf0 | `wav_FadeOutPeriod` / `wav_FadeInPeriod` `(dst, src, p_old, p_new)` | 6.2.4 |
| 0x1002aac0 / 0x1002afb0 | `wav_OlaUnitUnmodified` / `wav_PsolaUnit` `(tib*, short i)` | 7.1 / 7.3 |
| 0x100200c0 / 0x10020460 / 0x10020930 / 0x10020e00 | `wav_TextToBuffer{S16,Alaw,Ulaw,Dadpcm}` | buffer API (<= 60000 / 30000 / 30000 / 15000 bytes per call, overflow carried) |
| 0x1001e290 / 0x1001e430 / 0x1001e630 / 0x1001ed40 | `wav_TextToRaw{S16,Alaw,Ulaw}File`, `wav_TextToDadpcmFile` | fmt 0/1/2/3 |
| 0x1001e930 | `wav_TextToWaveFile` `(text, path, tag, ...)` | RIFF writer; tag 1 PCM16, 0x99 PCM8, 6 A-law, 7 mu-law, 0x11 IMA (unreachable) |
| 0x1001e830 / 870 / 8b0 / 8f0 | `wav_TextToWave{S16,U8,Alaw,Ulaw}` | fmt 4/5/7/8 to tag 1/0x99/6/7 |
| 0x1001fc40 | `wav_TextToAuFile` | fmt 9 (broken header, 8.6) |
| 0x1001f1f0 / 0x1001f600 | `wav_InitWaveHeader` / `wav_WriteWaveHeader` | header at `tib+0xedd98..0xeddf0`, rewritten after every chunk |
| 0x10020080 | `wav_InitAuHeader` | `.snd`, 0x1c, 0, 1, rate, 1, 0 |
| 0x1002e780 / 0x1002e720 | `wav_Linear2Alaw` / `wav_Linear2Ulaw` | encoders behind the 64K tables |
| 0x1002e860 / 0x1002e830 | `wav_BuildAlawLut` / `wav_BuildUlawLut` | fill `common+0x1841c` / `common+0x841c` for -32768..32767 at load (`FUN_10012380`) |
| 0x1002e800 / 0x1002e700 | `wav_LoadUlawDecodeTable` / `wav_UlawDecodeEntry` | copy 0x100751c0 to `common+0x21c`; no synthesis path reads it |
| 0x1002e660 / e4c0 / e550 / e600 / e4a0 | `wav_Dadpcm{EncodeChunk,EncodeSample,Update,IndexAdjust,Reset}` | 8.4 |
| 0x10025500 / 0x100255a0 / 0x10025400 | `wav_FileWriteAt` / `wav_FileWriteAtSwapped` / `wav_FileSeek` | position cache, 8.6 |
| 0x1001df10 / 0x1001e0c0 / 0x10022200 | `wav_OpenLipSyncLog` / `wav_FlushLipSyncLog` / `wav_TextToLipSyncLog` | 9.2 |
| 0x10026b40 | `wav_PlayStart` | `waveOutOpen(16000 Hz, 1 ch, 16-bit, CALLBACK_WINDOW)`, two chunks queued |

## 3. Data structures

### 3.1 DB unit entry (0x18 bytes, from `us_ReadPrimaryRecord`; primary record byte offsets in parentheses) [D][E]

| Off | Type | Field |
|---|---|---|
| +0x00 | u32 | `pcm_pos`: byte offset in `merged-<pool>.dat` (0..3) |
| +0x04 | u32 | `pm_pos`: byte offset in `merged-<pool>.upm` (10..13) |
| +0x08/09/0a | 3*u8 | first / shared-middle / last pitch period in half-samples (16, 17, 18); equal `upm[pm_pos]`, `upm[pm_pos+pm1-1]`, `upm[pm_pos+pm1+pm2-2]` |
| +0x0c | s16 | `h1` = 2 * sum of `.upm` bytes of view 1 (4..5) |
| +0x0e | s16 | `h2` = 2 * sum of view 2 (6..7) |
| +0x10 | u16 | coded payload size (8..9) |
| +0x12 / +0x13 | u8 | `pm1` / `pm2`: pitch marks per half; halves share the boundary mark (14, 15) |
| +0x14 | u8 | pool / file index |

### 3.2 Unit record (0x34 bytes at `tib+0xeddf8 + i*0x34`; count `s16` at `tib+0xfbd68`, max 1100) [D][E]

| Off | Type | Field | Silence record |
|---|---|---|---|
| +0x00 | s32 | pitch % (`SS+0x47718[w]`) | channel pitch `ch+0x4cf4` |
| +0x04 | s32 | duration (speed) % (`SS+0x47710[w]`) | 100 |
| +0x08 | s32 | volume % (`SS+0x47714[w]`) | channel volume `ch+0x4cf8` |
| +0x0c | s32 | length in samples (overwritten by the smoother) | pause length |
| +0x10 | s16 | target index t | -1 |
| +0x12 | s16 | file (pool) index | |
| +0x14 | s32 | `pcm_pos` (+ sample offset for mode 2, 4.3) | |
| +0x18 | ptr | DB entry (`tib+0x11b24c + 0x18t`) | stale |
| +0x1c | u32 | `.upm` offset of the view | |
| +0x20 | u8 | pitch marks in the view (overwritten by the smoother) | |
| +0x22 / +0x24 | s16 | left / right overlap = 2 * first / last period of the view (overwritten by the smoother) | |
| +0x26 | u8 | mode: 0 whole, 1 first half, 2 second half (= target half-type) | |
| +0x27 | u8 | type: 1 silence, 2 unit | 1 |
| +0x28 | u16 | word index | phrase first word, or the previous record's word for the sentence-final pause |
| +0x2a | s16 | never written for units (stale) | -1 |
| +0x2c | u16 | phone index in the word | -1 |
| +0x2e / +0x30 | s16 | sync ring entry / phone slot (`wav_BuildSyncEntries`) | same |
| +0x32 | u8 | centre-phone byte of the 7-byte context (`W+0x96+7k+2`), for lip-sync codes | 0x5a |

### 3.3 TIB fields

| Offset | Meaning |
|---|---|
| +0x20 | stage limit (0 = full synthesis) |
| +0x2c | lip-sync log object (NULL unless `VT_TextToLipSyncLog_ENG`) |
| +0x30 / +0x34 | bytes in the current chunk / chunk buffer |
| +0x38 / +0x3c | output queue buffers (PCM / converted) |
| +0x40 | s16: 1 chunk full, 0 phrase finished |
| +0x44 / +0x4c | done / SS* |
| `SS+0x08` | pause before the current word (samples) |
| `SS+0x47704` / `+0x47706` / `+0x47708` | current phrase / next record / OLA tail length |
| `SS+0x4770a` / `+0x4770c` / `+0x47710/14/18` | sentence type / per-word pause ms / per-word speed, volume, pitch % |
| +0x47774 / +0x47778 | sync ring / lip-sync log print cursor |
| +0x4777c / +0x47780 | last word char span (sync) |
| +0x47784 | channel |
| +0xae0c4 / +0xae910 + 0xfc t | s16 path[t] / beam uid[30] |
| +0xec624 / +0xec628 + 6t | n targets / target `{word, syl, phone, idx, half, unvoiced}` |
| +0xedd98..0xeddf0 / +0xeddf4 | WAVE header image / ADPCM state `{pred, index}` |
| +0xeddf8 / +0xfbd68 | records / n records |
| +0xfbd6c | smoother result cached for the current record |
| +0xfbd70 / +0x100b90 / +0x1059b0 | left neighbour PCM / smoother's copy of the current unit / right neighbour PCM |
| +0x10a7d0 / +0x10aa28 / +0x10ac80 | left / current / right pitch periods |
| +0x10aed8 / +0x10b132 / +0x10b38c | smoother weights L / C / R (s16, <= 301) |
| +0x10b5e6 / +0x10b5e8 | n epochs / epoch table (5 ints each) |
| +0x10cd58 / +0x10d528 / +0x10d52c | OLA tail samples / tail length / period scratch |
| +0x10dcfc / +0x10e4cc | fade-in buffer B (1000 samples) / fade-out buffer A |
| +0x10ec9c | current pitch periods (smoothed or read) |
| +0x10eef4 | record source samples (10000) |
| +0x113d14 | Shorten decode window (10000 samples) |
| +0x118b34 | coded payload buffer |
| +0x1210d4 | emphasis state |
| +0x122440 / +0x122850 / +0x122851 | sentence-final pause ms / continuation flag / `.dtt` logging flag |

### 3.4 Channel (speaker) struct [D][E]

| Offset | Field | Default | Clamp |
|---|---|---|---|
| `ch+0x4cf0` | speed % | 100 | 50..400 |
| `ch+0x4cf4` | pitch % | 100 | 50..200 |
| `ch+0x4cf8` | volume % | 100 | 0..500 |
| `ch+0x4cfc` | emphasis | 0 | +-95 (`VT_SetEmphasisFactor_ENG` 0x10028310) |
| `ch+0x4d00` | sentence pause ms | 687 | <= 65535 |
| `ch+0x4d04` | comma pause ms | 200 | |

Also `ch+0x4d08` common block, `ch+0x64` global label array, `ch+0x6c` total units.

## 4. Unit and pause records (`wav_BuildUnitRecords`)

### 4.1 Algorithm [D][E]

```
records = []
for t in 0..n_targets:
    w = target[t].word
    if (t == 0 and w == first_word) or (t > 0 and target[t-1].word != w):
        pause = wav_PauseBeforeWord(w)                     # -> SS+8 (samples)
        if pause > 0:
            records += silence(len = (dfac(speed%[w]) * pause + 50) / 100,
                               word = first_word, pitch = ch.pitch, dur = 100, vol = ch.volume)
    r = unit(mode = target[t].half, unit = t, word = w, phone = target[t].phone,
             center = W(w).ctx[phone][2], pitch/dur/vol = per-word %)
    wav_FillUnitRecord(r, dbentry[t]); records += r
    if len(records) > 1099: n = 1100; return        # the sentence-final pause is lost too
if tib.final_pause_ms > 0 and SS.phrase == SS.n_phrases - 1:
    tib.final_pause_ms <<= 4                          # stays shifted in the TIB
    records += silence(len = (dfac(speed%[prev.word]) * final + 50) / 100, word = prev.word, ...)
```

`dfac(r) = ((r >> 1) + 10000) / r` = round(10000/r); pause records shrink as speed rises.

### 4.2 `wav_PauseBeforeWord` (0x1002e890) [D][E]

```
p = SS.word_pause_ms[w]                               # -1 unless VTML / the API set one
if p >= 0:
    if w == 0 and tib.cont_flag:
        if SS.word_pause_ms[0] > 0: tib.sync_span = (W0.start, W0.start)
        tib.cont_flag = 0
    return p << 4                                     # ms -> 16 kHz samples
if w == 0:
    if tib.cont_flag: tib.cont_flag = 0; return 0
    return 11000                                      # 687.5 ms
switch W(w-1).brk: 'Z', '^': return 11000;  '[': return ch.comma_ms << 4 (3200 with no channel);  default: return 0
```

The continuation-flag semantics are in [PROSODY.md](PROSODY.md) 6.7.

### 4.3 `wav_FillUnitRecord` (0x1002c120): the three `.upm` views [D][E]

| mode | len | ovl_l | ovl_r | upm | n_pm | pos14 |
|---|---|---|---|---|---|---|
| 0 whole | h1 + h2 - 2*per[1] | 2*per[0] | 2*per[2] | pm_pos | pm1 + pm2 - 1 | pcm_pos |
| 1 first half | h1 | 2*per[0] | 2*per[1] | pm_pos | pm1 | pcm_pos |
| 2 second half | h2 | 2*per[1] | 2*per[2] | pm_pos + pm1 - 1 | pm2 | pcm_pos + (s16)(h1 - 2*per[1]) |

Verified on all 12,463 captured speech records [E]: a record is exactly its pitch periods (`len == 2 * sum
upm[view]`); `ovl_l`/`ovl_r` are exactly one pitch period (first and last of the view); `pos14` mixes a byte offset
with a sample offset and does not reach the audio. An outside review reached the same view breakdown.

## 5. Decoding

### 5.1 `wav_DecodeUnit` (0x1002c8b0) [D][E]

```
read coded_size bytes at db.pcm_pos from .dat[rec.file] into tib+0x118b34
wav_ShortenDecode -> window tib+0x113d14              (the window persists across calls)
start = mode == 2 ? (s16)(db.h1 - rec.ovl_l) : 0      # first sample of the shared period
dst[0..len) = window[start .. start+len)
if vol != 100: dst[k] = sat16(dst[k] * vol / 100)     # C truncation
```

`sat16(v)`: `v >= 32767` gives 32766 and `v <= -32768` gives -32767 (same in smoother, volume and emphasis).
For well-formed units the span ends at the decoded length, so the window's stale tail is never read; the Rust
`DecodeWindow` models it anyway.

### 5.2 `.dat` codec: headerless mono Shorten [D][E]

Each unit payload is a raw Shorten v2+ bitstream: 1 channel, no `ajkg` header, no QLPC.

- **Bit reader:** MSB first, refilled with big-endian 32-bit words. `uvar(n)`: count 0-bits up to the first 1
  (hi), read n raw bits (lo), return `hi<<n | lo`. `svar(n)`: `u = uvar(n+1)`, then `u&1 ? ~(u>>1) : u>>1`.
  `ulong()` = `uvar(uvar(2))`.
- **Initial state:** blocksize 256, bitshift 0, nmean 4 (means 0), history `s[-3..-1] = 0`.
- **Command loop** (`cmd = uvar(2)` until QUIT):

| cmd | Name | Per sample of a block |
|---|---|---|
| 0 | DIFF0 | `e = uvar(3)`; `s = svar(e) + coffset` |
| 1 | DIFF1 | `s = svar(e) + s1` |
| 2 | DIFF2 | `s = svar(e) + 2*s1 - s2` |
| 3 | DIFF3 | `s = svar(e) + 3*(s1 - s2) + s3` |
| 8 | ZERO | no energy field; `s = 0` |
| 5 | BLOCKSIZE | `blocksize = ulong()` |
| 6 | BITSHIFT | `bitshift = uvar(2)` |
| 4 | QUIT | end of payload |
| 7 | QLPC | unsupported; decoder fails |

- **Running offset:** `coffset = trunc((2 + sum(means))/4)`; if `bitshift > 0`, `coffset = (coffset >> (bitshift-1)) >> 1`.
- **Block end:** `mean = trunc((trunc(bs/2) + sum(s))/bs) << bitshift` pushed into the 4-entry FIFO; history keeps
  the last 3 samples before the shift; output is `s << bitshift`, clamped only at the top (> 0x7fff gives 0x7fff),
  otherwise truncated to 16 bits.
- 471 MB of payload for 16 h of audio, about 4.8 bits/sample; lossless: the whole bank (all 580,474 units) decodes
  identically to the engine [E].

## 6. `wav_GeneratePhrase` (0x1002bd90)

### 6.1 Record loop and chunking [D][E]

```
if SS.next_record == 0 and tib.chunk_full_flag == 0:     # first entry for this phrase
    wav_BuildUnitRecords(...); if SS.next_record == 0: SS.tail_n = 0
    wav_BuildSyncEntries(...)
for i = SS.next_record .. n-1:
    before = tib.chunk_bytes
    if rec[i].type == SILENCE:
        if chunk_bytes + 2*len > 60000:                   # split exactly at 60000
            zero-fill to 60000; rec[i].len -= filled/2; credit filled/2 to sync; chunk full; return
        zero-fill 2*len bytes; SS.tail_n = 0
    else:
        if tib.smoothed == NULL: tib.smoothed = wav_SmoothJoins(i)       # cached across returns
        est = ((dfac(dur) * rec.len + 50) / 100) * 2                     # rec.len after smoothing
        if chunk_bytes + 2000 + est > 60000: chunk full; return          # resume at the same record
        if !smoothed: wav_DecodeUnit(tib+0x10eef4, rec)
        if pitch == 100 and dfac(dur) == 100: n_epochs = 0
        else: if !smoothed: wav_ReadPitchMarks(tib+0x10ec9c); n_epochs = wav_BuildEpochTable(...)
        wav_PsolaUnit(i); tib.smoothed = NULL
    sync[rec.entry].samples += (chunk_bytes - before)/2; sync[rec.entry].phone[rec.slot] += same
    SS.next_record++
if n == 0: done
tib.chunk_full_flag = 0                                   # phrase finished
```

Chunking never changes the audio (OLA tail, PSOLA state, smoother cache and emphasis state carry over); it shows
only in the DADPCM formats (8.4) and in buffer-API call granularity. `wave::chunk_ends` reproduces the chunk
boundaries on all 92 sweep captures (459 chunks) [E].

### 6.2 Join smoother `wav_SmoothJoins` (0x1002d230) [D][E]

Runs for every speech record before decoding. It blends the unit's first periods with the natural recorded
continuation of the previous selected unit and its last periods with the natural predecessor of the next one.

#### 6.2.1 Edge decisions (`L`/`R`: 0 off, 1 or 2 weight)

`lbl(u)` is the global 7-byte label `[L2,L1,C,R1,R2,posbits,flags]` (`ch+0x64 + 7u`); `cur`, `prev`, `next` are
the selected uids of targets t, t-1, t+1; `half` is the half-type of t.

```
L = 2
if t == 0: L = 0
elif lbl(cur).posbits & 0x80 and (lbl(cur).posbits & 0x38) >= 0x18: L = 0
elif half == 2 and prev == cur: L = 0
elif prev == n_units - 1: L = 0
elif half != 2:
    if prev + 1 == cur and lbl(prev).flags & 0x80: L = 0      # prev's flag (0x1002d395)
    elif not lbl(prev).flags & 0x80: L = 0
    elif not lbl(cur - 1).flags & 0x80: L = 1

R = 2
if t == n_targets - 1: R = 0
elif lbl(cur).posbits & 0x40 and (lbl(cur).posbits & 7) >= 3: R = 0
elif half == 1 and next == cur: R = 0
elif next == 0: R = 0
elif half != 1:
    if cur + 1 == next and lbl(cur).flags & 0x80: R = 0
    elif not lbl(next - 1).flags & 0x80: R = 0
    elif not lbl(cur).flags & 0x80: R = 1
if L == 0 and R == 0: return NULL
```

#### 6.2.2 Sources

- Current unit: decoded with a copy of the record (so `mode`/`vol` apply); periods P_c from its view.
- Left source (if L): unit `half == 2 ? prev : prev + 1`, via `wav_LoadNeighbourUnit` with the current record's
  mode and volume; periods P_l, `nL = n_pm`, `bL = min(nL, npm, 5)`.
- Right source (if R): unit `half == 1 ? next : next - 1`; P_r, `nR`, `bR = min(nR, npm, 5)`.

#### 6.2.3 Weights (16-bit) and new periods

```
cL = L ? bL*L : 1        sR = R ? bR*R : 1        W = (s16)(sR*cL)
for k in 0..=npm:  WL[k] = k < bL ? (bL-k)*sR : 0
                   WR[npm-k] = k < bR ? (bR-k)*cL : 0
                   WC[k] = W - WR[k] - WL[k]
for k in 0..npm:
    if WL[k] == 0 and WR[k+1] == 0: P'[k] = P_c[k]
    else:
        acc = (WC[k+1] + WC[k])*P_c[k]
        if WL[k] > 0:   acc += (WL[k] + WL[k+1])*P_l[k]
        if WR[k+1] > 0: acc += (WR[k] + WR[k+1])*P_r[nR - npm + k]
        P'[k] = (acc + W) / (2W)
```

#### 6.2.4 Synthesis

Output goes to `tib+0x10eef4`, zeroed to 10000 samples. Position counters `c`, `l`, `o` start at 0; `r` starts at
`sum P_r[0 .. nR-bR)`.

```
for k in 0..npm:
    if WL[k] == 0 and WR[k+1] == 0: copy P'[k] samples of cur[c..]; c += P_c[k]; o += P'[k]; continue
    out[o..o+P'[k]) += sat16(fadeout(cur+c, P_c[k], P'[k]) * WC[k] / W)
    if WL[k]   > 0: += sat16(fadeout(left+l,  P_l[k],        P'[k]) * WL[k]   / W)
    if WR[k]   > 0: += sat16(fadeout(right+r, P_r[nR-npm+k], P'[k]) * WR[k]   / W)
    out += sat16(fadein(cur+c, P_c[k], P'[k]) * WC[k+1] / W)
    if WL[k+1] > 0: += sat16(fadein(left+l,  P_l[k],        P'[k]) * WL[k+1] / W)
    if WR[k+1] > 0: += sat16(fadein(right+r, P_r[nR-npm+k], P'[k]) * WR[k+1] / W)
    if WL[k] > 0: l += P_l[k];   if WR[k+1] > 0: r += P_r[nR-npm+k]
    c += P_c[k]; o += P'[k]
rec.len = o; rec.n_pm = npm; rec.ovl_l = P'[0]; rec.ovl_r = P'[npm-1]
```

The `+=` accumulations are 16-bit wrapping adds. Windows: `fadeout(src, p_old, p_new)` = falling half-Hann over the
first `min(p_old, p_new)` samples (`idx = k*4096/len + 4096`), zero-padded to `p_new`; `fadein` = rising half over
the last `min(p_old, p_new)` samples of the period, right-aligned in `p_new`. Every window product is `wmul`
(7.0).

## 7. Overlap-add

### 7.0 Window product `wmul(w, s)` [D][E]

The window is `float hann[8192]` at 0x1006d1b0 (length word 0x1006d1ac); Hann to within 2e-7, so treat it as data.

```
FLD dword [0x1006d1b0 + 4*idx]; FILD s; FSTP qword; FMUL qword; FSTP qword   ; exact in f64
v >= 32767.0 -> 32766 ; v <= -32768.0 -> -32767 ; else __ftol(v) (truncation, 0x1006479c)
```

Bounds are the f64 constants at 0x100751b8 / 0x100751b0; result stored as s16. Rising half `idx = k*4096/L`; falling
half `idx = k*4096/L + 4096`.

### 7.1 `wav_OlaUnitUnmodified` (0x1002aac0; pitch = dur = 100) [D][E]

```
L = ovl_l; tn = SS.tail_n (tail = tib+0x10cd58)
if not (tn > L and L < 1):
    A = zeros(L)
    if 0 < tn <= L: A[k] = wmul(fall(k, tn), tail[k])  k < tn
    if tn > L:      A[k] = wmul(fall(k, L),  tail[k])  k < L
    out[k] = wmul(rise(k, L), src[k]) + A[k]  (16-bit)  k < L
mid = max(0, len - ovl_r - L); out += src[L .. L+mid)
SS.tail_n = len - mid - L                             # normally = ovl_r
if i < n-1 and rec[i+1].type != SILENCE: tail = src[len-tail_n .. len)     # carried
else: out += wmul(fall(k, tail_n), src[len-tail_n+k]); SS.tail_n = 0     # flushed
```

Each unit is cross-faded into its predecessor over its own first pitch period; its last period is held back as
the next unit's fade-out material.

### 7.2 Epochs [D][E]

`wav_BuildEpochTable`: for `k < n_pm - 1`, epoch `{pos = sum P[0..k), P_left = P[k], P_right = P[k+1], dfac(dur),
pitch}`; for `n_pm == 1` a single `{0, ovl_l - 1, 1, dfac, pitch}`. `wav_FindNearestEpoch(t)`: fail if `t >= last.pos
+ last.P_left + last.P_right`; otherwise the first k minimizing `|pos + P_left - t|` (strict `<`, starting at 20000).

### 7.3 `wav_PsolaUnit` (0x1002afb0; pitch or dur != 100) [D][E]

```
if pitch == 100 and dur == 100: return wav_OlaUnitUnmodified
op = 0; lim = epoch[0].P_left; prev_pr = 0
e = nearest(lim*100 / dfac)
while e:
    L = e.P_left*100 / e.pitch;  L = max(L, P_left/16); L = min(L, 31*P_left/16)   # signed /16
    if L - P_left + op > lim: L = P_left - op + lim
    A: op != 0        -> fade out the copied right period:   den = L >= prev_pr ? prev_pr : L
       op == 0, tail  -> fade out the carried tail:           den = L >= tn ? tn : L
       else zeros
    B: L >= P_left -> B[L-P_left+k] = wmul(rise(k, P_left), src[pos+k])  k < P_left (zeros before)
       else        -> B[k]          = wmul(rise(k, L), src[pos - L + P_left + k])  k < L
    dst[op+k] = A[k] + B[k] (16-bit)  k < L
    op += L; copy src[pos+P_left .. +P_right) to dst[op..]; prev_pr = P_right
    lim = op + P_right; e = nearest(lim*100 / e.dfac)
if op == 0: return                                     # nothing committed, tail untouched
SS.tail_n = prev_pr
if i < n-1 and next is not silence: tail = dst[op .. op+prev_pr); commit op samples
else: dst[op+k] = wmul(fall(k, prev_pr), dst[op+k]); commit lim samples      # tail_n not cleared
```

TD-PSOLA variant: each output period is the analysis period scaled by 100/pitch%, clamped to [P/16, 31P/16]; the
analysis cursor advances by the duration factor so periods repeat or drop; one Hann cross-fade per period over the
new length; the right-hand period is copied verbatim. Pitch alone changes total length (prose at 50-200%: 48,888
to 44,002 samples).

## 8. Output

### 8.1 Emphasis (`wav_NextChunkWithEmphasis`) [D][E]

`k = ch+0x4cfc` clamped to +-95; `k = 0` skips the filter. State `tib+0x1210d4` (starts 0) carries across chunks.
- k > 0 (IIR low shelf): `y[i] = sat16(x[i] + (y[i-1]*k + 50)/100)`, forward; state = last output.
- k < 0 (FIR pre-emphasis): `y[i] = sat16(x[i] + (x[i-1]*k + 50)/100)`, backward in place; state = last input.

Division is C truncation (`0x51eb851f` magic, `>>5`, sign fix).

### 8.2 G.711 [D][E]

Tables built at load for every s16: mu-law `common+0x841c` (`wav_Linear2Ulaw` 0x1002e720), A-law `common+0x1841c`
(`wav_Linear2Alaw` 0x1002e780).

- **A-law:** Sun g711.c `linear2alaw` on `s>>3`; segment table `{31,63,127,255,511,1023,2047,4095}` at 0x1007e0f8;
  mask 0xd5 (positive) / 0x55 (negative, `pcm = -1 - pcm`); segment >= 8 gives `0x7f ^ mask`.
- **mu-law:** classic `exp_lut` encoder: sign 0x80, negation in int, clip compared as s16 at 0x7f7b, bias 0x84,
  `exp = exp_lut[(v>>7)&0xff]` (256 ints at 0x1007dcf8), `mant = ((s16)v >> (exp+3)) & 15`, result
  `~(sign|exp<<4|mant)`.

The engine's int/s16 mixing makes -32768 encode to 0x7f (a standard encoder gives 0x00); reproduced.

### 8.3 U8 (tag 0x99) [D][E]

`byte = (s8)(s/256) + 0x80`, truncating division.

### 8.4 Dialogic/OKI ADPCM, "DADPCM" (fmt 3, buffer fmt 3) [D][E]

Input 12-bit samples (`s>>4`); 49 steps at 0x1007dc30 (16..1552); state `{pred, index}` at `tib+0xeddf4`.

```
encode:  d = s - pred (s16); sign 8 if d < 0 (d = -d)
         code |= 4 if d >= step (d -= step); |= 2 if d >= step/2 (d -= step/2); |= 1 if d >= step/4
update:  delta = step/8 [+ step/4 if 1] [+ step/2 if 2] [+ step if 4]; negate if 8
         pred = clamp((s16)(pred + delta), -2048, 2048)
         index = clamp(index + adj, 0, 48); adj = -1 for codes 0..3, 2/4/6/8 for 4..7
```

Packing (`wav_DadpcmEncodeChunk`), per engine chunk of n samples: n/2 bytes, high nibble first; the last sample
of an odd-length chunk is dropped and never encoded (the byte count is a signed `/4` of `2n`); the state resets
only before the first chunk of the request. File writer and buffer API (15000 bytes per call, overflow carried)
give the same stream. This is the one format where the 60000-byte chunking is visible.

### 8.5 WAVE (`wav_TextToWaveFile`, 0x1001e930) [D][E]

The header lives in the TIB and is rewritten at fixed offsets after every chunk; data is appended with `SEEK_END`.
Little-endian, 16000 Hz mono, avg bytes/s = 16000 * block.

| Tag | fmt size | wFormatTag | bits | block | extra | header bytes |
|---|---|---|---|---|---|---|
| 1 (fmt 4) | 16 | 1 | 16 | 2 | | 44 |
| 0x99 (fmt 5) | 16 | 1 | 8 | 1 | | 44 |
| 6 (fmt 7) | 18 | 6 (A-law) | 8 | 1 | cbSize 0 + `fact`(4) = samples | 58 |
| 7 (fmt 8) | 18 | 7 (mu-law) | 8 | 1 | same | 58 |

RIFF size = `data + 0x14 + fmt_size` (+12 with `fact`). Tag 0x11 (IMA ADPCM, `FUN_1001c8b0`) is coded but no
public fmt reaches it (fmt 6 falls into `VT_TextToFile_ENG`'s default case: error).

### 8.6 Raw and AU; the file-position cache [D][E]

Raw formats (fmt 0/1/2/3) write at an explicit running offset with no header. `wav_FileWriteAt(h, pos, whence)`
seeks only when `whence != 0` or `pos != h+0xc`; writes advance `h+0xc`.

**fmt 9 (`.au`) is broken by this.** `wav_TextToAuFile` writes the seven fields of `wav_InitAuHeader` (".snd", 28,
size, 1, 16000, 1, 0) all at "pos 0"; each write after the first seeks back to 0 and overwrites the previous one.
The file ends up as 4 zero bytes (the annotation field) followed by raw mu-law data. The port reproduces this byte
for byte; `formats::au_header` builds the intended header.

## 9. Word/phone sync

### 9.1 Ring (`wav_BuildSyncEntries`, 0x1002c530) [D][E]

600 entries of 0x24 bytes at `*(tib+0x47774)`; head at `ring+0xc`, wraps at 600.

| Off | Type | Field |
|---|---|---|
| +0 | s16 | n phone slots |
| +4 | ptr | phone slots `{s32 samples, u16 code}` |
| +8 | s32 | samples |
| +0xc / +0x10 | s32 | char span (inclusive) |
| +0x14 / +0x18 | s32 | sentence first / last char |
| +0x1c / +0x20 | s32 | n words / word index |

Per phrase record k: a new entry starts at k = 0 or when the word or type differs from record k-1. A unit entry
takes `W(word).start/end` and updates the last span (`tib+0x4777c/80`); a silence entry copies the last span (a
pause shows as the preceding word's text); `wav_PauseBeforeWord` can reset the span. A new phone slot starts when
the word, phone or type changes; its code is `LIPSYNC_CODE[rec.center]` (256 bytes at 0x1007daa8: phone code to base
index 0..38 with stress folded, 40 for silence 0x5a). `wav_GeneratePhrase` adds each record's samples to its entry
and slot. Word boundaries for `VT_TextToBufferEX_ENG`'s SYNCINFO and SAPI word events come from here.

### 9.2 Lip-sync log (`VT_TextToLipSyncLog_ENG(text, path, spk, pitch, speed, vol, pause, dict, type)`) [D][E]

Opened in text mode (CRLF). After each chunk `wav_FlushLipSyncLog` prints every ring entry created before the
previous flush. Per entry: `"%2d (%3s) : %d \n"` per phone slot (code, name from the 5-byte entries at 0x1007bbd4,
samples; silence prints `"%2d (sil) : %d \n"`), `"-------------------------------\n"`, `"%d - %d %s : %d \n"` (start,
end, `text[start..=end]`, samples), `"===============================\n\n"`. On close: `"Total  Word Length : %d\n"`
and `"Total Phone Length : %d\n"`. `wave::lipsync_log` reproduces the file byte for byte.

## 10. Playback (`VT_PLAYTTS_ENG` to `wav_PlayStart`, 0x10026b40) [D]

Prime the first sentence; render chunk 1; `waveOutOpen(&hwo, device = DAT_1007d6e0 (VT_SetSoundCardID_ENG), {PCM,
1 ch, 16000, 32000, 2, 16}, hidden window DAT_100a8418, CALLBACK_WINDOW)`; post the caller's `umsg` with the first
word's span; render chunk 2; `FUN_10026d00` prepares and writes both buffers. `WOM_DONE` on the hidden window
refills (double buffering). Pause/restart/stop map to `waveOutPause` / `waveOutRestart` / `waveOutReset`. Not
ported; the native engine hands PCM to its caller.

## 11. Tables (generated into `src/wave_tables.rs`)

| Address | Contents |
|---|---|
| 0x1006d1ac / 0x1006d1b0 | `HANN_LEN = 8192`, `float HANN[8192]` |
| 0x100751b0 / 0x100751b8 | `CLAMP_LO = -32768.0`, `CLAMP_HI = 32767.0` |
| 0x1007e0f8 | `ALAW_SEG_END[8]` |
| 0x1007dcf8 | `ULAW_EXP_LUT[256]` (int) |
| 0x1007dc30 | `ADPCM_STEPS[49]` = 16, 17, 19 ... 1552 |
| 0x100751c0 | `ULAW_DECODE[256]` (unused by synthesis) |
| 0x1007daa8 / 0x1007bbd4 | `LIPSYNC_CODE[256]` / `LIPSYNC_NAMES[41]` (AA ... ZH) |

## Builds

| Build | Differences |
|---|---|
| 2014 English, Violeta 2013 | this document (join smoother 6.2; Violeta in [VIOLETA.md](VIOLETA.md)) |
| Julie09 | **no join smoother**: `FUN_10029c20` decodes (`FUN_1002a800`) and goes straight to overlap-add; unit records are a struct of arrays; channel clamp 0..3 (pause-sample clamp to 3); everything else (decode, OLA, PSOLA, pitch marks, record fill, pause samples, sync, lip-sync text, Hann table) matches 2014 apart from offsets |
| Paul09 | as Julie09, channel clamp to 1 |
| Violeta09 | as Julie09, channel clamp 0 |
| Paul04 | no smoother, but the record builder, unmodified OLA and PSOLA are rewritten (section "2004 build", `src/wave04.rs`) |

Port: `wave::generate_phrase_traced_era` (2009: no `smooth_joins`); the engine calls it with the 2009 era and passes
centre index 1 of the 5-byte label to the record builder. All 13 formats and the lip-sync log are unchanged. See
[OLD_PAUL.md](OLD_PAUL.md).

## 2004 build (`Era::Paul04`, `src/wave04.rs`, `src/engine04.rs`)

`wav_GeneratePhrase` 0x100203c0 (`wav_SelectAndGeneratePhrase` 0x10020350, `wav_FillChunk` 0x1001da20,
`wav_NextChunkWithEmphasis` 0x1001dad0 with the same emphasis filter, factor at `spk+0x3830`, default 0). The unit
records are a struct of arrays at `tib+0xe54a8` (`wav_BuildUnitRecords` 0x10020640): pos14 `+0x17f0`, len `+0x27e4`,
unit-entry pointer `+0x37d8`, mode `+0x47cc`, upm `+0x4bd0`, n_pm `+0x5bc4`, ovl_l `+0x5fc2`, ovl_r `+0x67bc`, type
`+0x6fb6`, word `+0x73b4`, phone `+0x83a8`, centre `+0x9b96`, pitch `+0x9f94`, duration factor `+0xaf88`, volume
`+0xbf7c`, file `+0xff4`, count `+0xcf70` (at most 0x3fc; more makes the builder return 0 and leave the count). The
unit entry is 0x14 bytes in the word (`+0xe8`): `pcm_pos`, `pm_pos`, half1 (short), half2, coded size, pm1, pm2,
periods[3], file; the fill rules per mode are `Record::fill`'s. Targets are the 6-byte list at word `+0xec`
(word, syl, phone, index, half, unvoiced); the half is the mode.

- Pauses: `pro_PauseSamples` 0x100224f0 (`pause_samples04`): explicit `pause[w] << 4`, word 0 consumes the
  continuation flag `tib+0x1132b8`, otherwise 11000 after `Z` / `^`, 0xc80 after `[`, else 0. Each non-zero pause
  becomes **ten** silence records of `pause / 10` samples (word = the phrase's first word, centre `0x5a`), carrying
  the channel's pitch, duration factor and volume; a silence plays `(len * dur + 50) / 100` zeros. There is no
  trailing-pause record.
- Channel: `VT_SetPitchSpeedVolumePause_ENG` stores pitch 50..200 (`+0x3828`), the duration factor
  `10000 / speed` clamped 25..200 (`+0x3824`, speed itself at `+0x3820`), volume 0..500 (`+0x382c`), pause up to 20000
  (`+0x3834`, default 687); defaults 100 / 100 / 100. `Channel04::apply`.
- Decode: `wav_DecodeUnit` 0x10020de0 = `DecodeWindow::decode` (same persistent 10000-sample window, volume
  `x * vol / 100` saturated).
- Pitch and duration factor both 100: `wav_OlaUnitUnmodified` 0x10020f70 (`ola_unmodified04`). It copies
  `len - ovl_r` samples (`len` for the last speech record before a silence or the phrase end); the first `tail`
  samples are `f32(tail[j]*H[(t+j+1)(N-1)/(2t+1)] + src[j]*H[(j+1)(N-1)/(2t+1)])` clamped, with `t` the previous
  record's saved tail length and `N = 8192` the Hann table (same table as 2014). The last `ovl_r` samples are the next
  tail. `ovl_l` is not used.
- Otherwise `wav_PsolaUnit` 0x100211b0 (`psola04`) with the epochs of `wav_BuildEpochTable` 0x10021950
  (`[pos, left, right, dur factor, pitch]`; one mark gives `[0, m0 - 1, 1]`) and `wav_FindNearestEpoch` 0x10021a10
  (target time `lim * 100 / dur factor`). Each epoch windows `left + right` samples with the Hann table
  (`H[(i+1)*0x1fff/(2*left+1)]` rising, `H[(right-left+1+i)*0x1fff/(2*right+1)]` falling), places the grain by
  `l = clamp(left*100/pitch, left/16, left*31/16)` and overlap-adds it (cases `l < left` and `l >= left`, with and
  without the previous record's tail), and commits `op` samples (keeping the last `right` as the next tail) or
  `lim` samples for the last speech record. No fade-out at a flush: the grain already ends in the falling window.
- Chunks (`chunk_sizes04`): before each record, `(dur*len*2 + 50)/100 + 2000 + chunk_bytes > 60000` or
  `len*2 + decoded_since_entry*2 > 60000` ends the chunk and re-enters at that record.

Verification (2026-10-15): `wave_paul04_parity` PCM from captured records 420/420 (84 sweeps + 336 corpus x
parameter combinations, chunk sizes included), records from the captured selection 960/960 phrases (43,168 records),
selection to PCM 420/420, and the whole speak corpus (2909 texts) against `out_old/paul04`: 2909/2909 bit-exact.

## Verification

`tests/wave_parity.rs` (about 4 s). Captures (harness, not in the repository): 42-text corpus at default
parameters; 92 waveform captures (50 sweeps: speed 50/75/150/200/300/400, pitch 50/60/85/120/150/200, volume
0/30/250/500, emphasis +-50/+-95, pause/comma changes, combinations; record `records[].inp` and `chunks[]`); a
format host writing fmts 0/1/2/3/4/5/7/8/9 through `VT_TextToFile_ENG`, 0..3 through `VT_TextToBuffer_ENG` and the
lip-sync log for every text and parameter set (1,288 files).

| Test | Input to output | Result |
|---|---|---|
| `wave_parity_default` | captured records to PCM | 42/42 bit-exact |
| `wave_parity_params` | captured records to PCM (+ emphasis) | 92/92; chunk sizes 92/92 (459 chunks) |
| `records_parity` | selection + sentence to records | 218/218 phrases, 9,317/9,317 records (all fields but stale `+0x2a`) |
| `wave_parity_from_selection` | selection to records to PCM | 92/92 |
| `formats_parity` | PCM + chunks to 13 outputs | 1,196/1,196 byte-exact (files 0,1,2,3,4,5,7,8,9; buffers 0..3) |
| `lipsync_parity` | selection to records to sync to log | 92/92 byte-exact |

2009: `wave_{julie09,paul09,viol09}_parity.rs`. PCM from captured records Julie09 92/92 + 42/42, Paul09 92/92 +
42/42, Violeta09 1416/1416; records from selection differing: Julie09 0/9183, Paul09 0/10739, Violeta09
0/508661; formats and lip-sync 92/92 (`formats_parity_viol09`: 1416/1416). 2014 suites unchanged.

## Open items

- `wav_PlayStart` / `waveOut` callback details (`FUN_10026d00`, the window procedure, `umsg` payloads): call level only, not ported.
- Buffer-API SYNCINFO (`VT_AllocSyncInfo_New_ENG` ... `VT_TextToBufferEX_ENG`): the ring is ported, the exported struct layout is not decoded.
- IMA ADPCM writer `FUN_1001c8b0` (tag 0x11): unreachable through public fmt codes, not ported.
- Stage limits (`tib+0x20` = 1/2/3) and the `.dtt` path (`tib+0x122851`): the waveform side just skips synthesis; the dump format is in [UNITSEL.md](UNITSEL.md).
- Undefined behaviour not seen in captures: smoothed period counts > 300 overflow the weight arrays; a chunk check on an empty chunk with `est > 58000` loops forever.
- Spanish lip-sync log (Violeta phone names): library only.

## History

- 2026-09-23: OLA + smoother ported (1/42); fixed the `prev+1 == cur` test to use `lbl(prev).flags` (0x1002d395): 41/42.
- 2026-09-23: PSOLA ported: 42/42; 50-capture parameter sweep 50/50 on the first run, including emphasis.
- 2026-09-23: outside review's three-`.upm`-view correction applied (`pm1+pm2-1` marks): unchanged results.
- 2026-09-23: record builder 218/218; chunk model 92/92.
- 2026-09-23: formats: fmt 9 (AU position-cache bug, 4-byte file) and fmt 3 (odd sample dropped) fixed: 1,196/1,196.
- 2026-09-23: lip-sync log 0/92 until CRLF line endings, then 92/92.
- 2026-10-05: 2009 waveform for Julie09, Paul09, Violeta09 (no smoother) bit-exact.
- 2026-10-15: 2004 waveform (own record builder, OLA and PSOLA) bit-exact on the first run, 2909/2909 speak texts.
