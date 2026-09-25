//! Output post-filter and format conversion (`VT_TextToFile_ENG` /
//! `VT_TextToBuffer_ENG` fmt codes).
//!
//! Everything here works on the engine's output *chunks* (`wave::chunk_ends`),
//! because the Dialogic ADPCM writer is chunk-sensitive; the other formats are
//! per-sample and give the same bytes however the stream is cut.

use super::sat16;
use super::tables::{ADPCM_STEPS, ALAW_SEG_END, ULAW_DECODE, ULAW_EXP_LUT};

/// The engine's mu-law decode table (`common+0x21c`, from 0x100751c0). Loaded at
/// startup; no synthesis path reads it.
pub fn ulaw2linear(b: u8) -> i16 {
    ULAW_DECODE[b as usize]
}

/// `VT_SetEmphasisFactor_ENG` 0x10028310 clamp.
pub fn clamp_emphasis(k: i32) -> i32 {
    k.clamp(-95, 95)
}

/// `wav_NextChunkWithEmphasis` 0x10026870, applied to one output chunk.
///
/// k > 0: `y[i] = x[i] + (y[i-1]*k + 50)/100` (recursive, low-frequency boost),
/// state = last output. k < 0: `y[i] = x[i] + (x[i-1]*k + 50)/100` (FIR
/// pre-emphasis), state = last input. `state` lives at `TIB+0x1210d4` and
/// carries across chunks, so filtering the whole stream in one call is identical
/// to filtering it chunk by chunk.
pub fn emphasis(pcm: &mut [i16], k: i32, state: &mut i16) {
    if k == 0 || pcm.is_empty() {
        return;
    }
    let term = |v: i16| (v as i32 * k + 50) / 100;
    if k > 0 {
        let mut prev = *state;
        for s in pcm.iter_mut() {
            let y = sat16(*s as i32 + term(prev));
            *s = y;
            prev = y;
        }
        *state = prev;
    } else {
        let last = *pcm.last().unwrap();
        for i in (0..pcm.len()).rev() {
            let p = if i == 0 { *state } else { pcm[i - 1] };
            pcm[i] = sat16(pcm[i] as i32 + term(p));
        }
        *state = last;
    }
}

/// `FUN_1002e780`, the A-law encoder behind the table at `common+0x1841c`
/// (Sun g711.c `linear2alaw`, 16-bit input).
pub fn linear2alaw(s: i16) -> u8 {
    let mut pcm = s >> 3;
    let mask: u8 = if pcm < 0 {
        pcm = -1 - pcm;
        0x55
    } else {
        0xd5
    };
    let seg = match ALAW_SEG_END.iter().position(|&e| e >= pcm) {
        Some(seg) => seg,
        None => return 0x7f ^ mask,
    };
    let mut aval = (seg as u8) << 4;
    if seg < 2 {
        aval |= ((pcm as i8 >> 1) & 0xf) as u8;
    } else {
        aval |= ((pcm >> seg) & 0xf) as u8;
    }
    aval ^ mask
}

/// `FUN_1002e720`, the mu-law encoder behind the table at `common+0x841c`.
/// Faithful to the engine's int/short mix, which maps -32768 to 0x7f.
pub fn linear2ulaw(s: i16) -> u8 {
    let mut v = s as i32;
    let sign: u8 = if (v as i16) < 0 {
        v = -v;
        0x80
    } else {
        0
    };
    if (v as i16) > 0x7f7b {
        v = 0x7f7b;
    }
    let biased = v + 0x84;
    let exp = ULAW_EXP_LUT[((biased >> 7) & 0xff) as usize];
    let mant = (((biased as i16) >> ((exp + 3) & 0x1f)) & 0x0f) as u8;
    !(mant | ((exp as u8) << 4) | sign)
}

/// fmt 5 (`wav_TextToWaveFile` tag 0x99): signed 16 -> unsigned 8.
pub fn linear2u8(s: i16) -> u8 {
    ((s as i32 / 256) as i8 as u8).wrapping_add(0x80)
}

/// Dialogic/OKI ADPCM state at `TIB+0xeddf4` (`FUN_1002e4a0` resets it).
#[derive(Debug, Default, Clone, Copy)]
pub struct AdpcmState {
    pub pred: i16,
    pub index: i16,
}

fn adpcm_index_adjust(code: u8) -> i16 {
    match code & 7 {
        4 => 2,
        5 => 4,
        6 => 6,
        7 => 8,
        _ => -1,
    }
}

/// `FUN_1002e4c0` + `FUN_1002e550`: encode one 12-bit sample.
pub fn adpcm_encode_sample(s12: i16, st: &mut AdpcmState) -> u8 {
    let step = ADPCM_STEPS[st.index as usize] as i32;
    let mut diff = s12.wrapping_sub(st.pred);
    let mut code = 0u8;
    if diff < 0 {
        code = 8;
        diff = diff.wrapping_neg();
    }
    let mut d = diff as i32;
    if step <= d {
        code |= 4;
        d -= step;
    }
    if step / 2 <= d {
        code |= 2;
        d -= step / 2;
    }
    if step / 4 <= d {
        code |= 1;
    }
    let mut delta = step / 8;
    if code & 1 != 0 {
        delta += step / 4;
    }
    if code & 2 != 0 {
        delta += step / 2;
    }
    if code & 4 != 0 {
        delta += step;
    }
    if code & 8 != 0 {
        delta = -delta;
    }
    let mut p = st.pred.wrapping_add(delta as i16);
    if p > 0x800 {
        p = 0x800;
    } else if p < -0x800 {
        p = -0x800;
    }
    st.pred = p;
    st.index = (st.index + adpcm_index_adjust(code)).clamp(0, 48);
    code
}

/// Dialogic ADPCM for a chunked stream (`FUN_1002e660` per chunk, as fmt 3 of
/// both the file and the buffer API use it). Each chunk of n samples becomes n/2
/// bytes (high nibble first); the last sample of an odd-length chunk is dropped
/// and the encoder state carries on. State resets only before the first chunk.
pub fn dadpcm_chunked(pcm: &[i16], chunk_ends: &[usize]) -> Vec<u8> {
    let mut st = AdpcmState::default();
    let mut out = Vec::with_capacity(pcm.len() / 2);
    let mut start = 0usize;
    for &end in chunk_ends {
        for k in 0..(end - start) / 2 {
            let a = pcm[start + 2 * k] >> 4;
            let b = pcm[start + 2 * k + 1] >> 4;
            let hi = adpcm_encode_sample(a, &mut st) << 4;
            let lo = adpcm_encode_sample(b, &mut st);
            out.push(hi | lo);
        }
        start = end;
    }
    out
}

/// WAVE header as `FUN_1001f1f0` / `FUN_1001f600` write it. `tag` is the engine's
/// internal code: 1 PCM16, 0x99 PCM8, 6 A-law, 7 mu-law. `samples` is the total.
pub fn wave_header(tag: u16, samples: u32) -> Vec<u8> {
    let (format, bits, block) = match tag {
        1 => (1u16, 16u16, 2u16),
        0x99 => (1, 8, 1),
        6 => (6, 8, 1),
        7 => (7, 8, 1),
        _ => panic!("unsupported wave tag {:#x}", tag),
    };
    let data = match tag {
        1 => samples * 2,
        _ => samples,
    };
    let fmt_size: u32 = if format == 1 { 16 } else { 18 };
    let mut riff = data + 0x14 + fmt_size;
    if format != 1 {
        riff += 8 + 4;
    }
    let mut h = Vec::with_capacity(58);
    h.extend_from_slice(b"RIFF");
    h.extend_from_slice(&riff.to_le_bytes());
    h.extend_from_slice(b"WAVEfmt ");
    h.extend_from_slice(&fmt_size.to_le_bytes());
    h.extend_from_slice(&format.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes());
    h.extend_from_slice(&16000u32.to_le_bytes());
    h.extend_from_slice(&(16000u32 * block as u32).to_le_bytes());
    h.extend_from_slice(&block.to_le_bytes());
    h.extend_from_slice(&bits.to_le_bytes());
    if format != 1 {
        h.extend_from_slice(&0u16.to_le_bytes());
        h.extend_from_slice(b"fact");
        h.extend_from_slice(&4u32.to_le_bytes());
        h.extend_from_slice(&samples.to_le_bytes());
    }
    h.extend_from_slice(b"data");
    h.extend_from_slice(&data.to_le_bytes());
    h
}

/// Sun `.au` header as `FUN_10020080` builds it (big-endian, mu-law, 16 kHz).
pub fn au_header(samples: u32) -> Vec<u8> {
    let mut h = Vec::with_capacity(28);
    h.extend_from_slice(b".snd");
    for v in [0x1cu32, samples, 1, 16000, 1] {
        h.extend_from_slice(&v.to_be_bytes());
    }
    h.extend_from_slice(&[0, 0, 0, 0]);
    h
}

/// Public `VT_TextToFile_ENG` fmt codes (vt_eng.h `VT_FILE_API_FMT_*`).
pub const FMT_S16PCM: i32 = 0;
pub const FMT_ALAW: i32 = 1;
pub const FMT_MULAW: i32 = 2;
pub const FMT_DADPCM: i32 = 3;
pub const FMT_S16PCM_WAVE: i32 = 4;
pub const FMT_U08PCM_WAVE: i32 = 5;
pub const FMT_ALAW_WAVE: i32 = 7;
pub const FMT_MULAW_WAVE: i32 = 8;
pub const FMT_MULAW_AU: i32 = 9;

/// The file `VT_TextToFile_ENG(fmt, ...)` writes for this (post-emphasis) PCM.
/// `None` for fmt 6 and unknown codes, which the engine rejects.
pub fn encode_file(fmt: i32, pcm: &[i16], chunk_ends: &[usize]) -> Option<Vec<u8>> {
    let n = pcm.len() as u32;
    let bytes = |f: fn(i16) -> u8| pcm.iter().map(|&s| f(s)).collect::<Vec<u8>>();
    Some(match fmt {
        FMT_S16PCM => pcm.iter().flat_map(|s| s.to_le_bytes()).collect(),
        FMT_ALAW => bytes(linear2alaw),
        FMT_MULAW => bytes(linear2ulaw),
        FMT_DADPCM => dadpcm_chunked(pcm, chunk_ends),
        FMT_S16PCM_WAVE => {
            let mut v = wave_header(1, n);
            v.extend(pcm.iter().flat_map(|s| s.to_le_bytes()));
            v
        }
        FMT_U08PCM_WAVE => {
            let mut v = wave_header(0x99, n);
            v.extend(bytes(linear2u8));
            v
        }
        FMT_ALAW_WAVE => {
            let mut v = wave_header(6, n);
            v.extend(bytes(linear2alaw));
            v
        }
        FMT_MULAW_WAVE => {
            let mut v = wave_header(7, n);
            v.extend(bytes(linear2ulaw));
            v
        }
        FMT_MULAW_AU => {
            // FUN_1001fc40 writes all seven header fields at file offset 0 and the
            // writers advance the cached position, so each write seeks back to 0
            // and overwrites the previous one: only the zero annotation survives.
            let mut v = vec![0u8; 4];
            v.extend(bytes(linear2ulaw));
            v
        }
        _ => return None,
    })
}
