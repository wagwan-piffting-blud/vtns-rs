//! Decoder for `merged-*.dat` unit payloads: headerless mono Shorten streams
//! (vt_eng.dll `wav_ShortenDecode` 0x10001b30). Bit-exact to the engine.

const FN_QUIT: u32 = 4;
const FN_BLOCKSIZE: u32 = 5;
const FN_BITSHIFT: u32 = 6;
const FN_ZERO: u32 = 8;

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    word: u32,
    nbits: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader { data, pos: 0, word: 0, nbits: 0 }
    }

    fn refill(&mut self) {
        // Past the payload the engine reads its zero-filled staging window.
        let mut w = [0u8; 4];
        for (k, b) in w.iter_mut().enumerate() {
            if let Some(&v) = self.data.get(self.pos + k) {
                *b = v;
            }
        }
        self.word = u32::from_be_bytes(w);
        self.pos += 4;
        self.nbits = 32;
    }

    fn overrun(&self) -> bool {
        self.pos > self.data.len() + 64
    }

    fn uvar(&mut self, mut n: u32) -> Option<u32> {
        if self.nbits == 0 {
            self.refill();
        }
        let mut v: u32 = 0;
        self.nbits -= 1;
        while (self.word >> self.nbits) & 1 == 0 {
            if self.nbits == 0 {
                self.refill();
                if self.overrun() {
                    return None;
                }
            }
            v = v.wrapping_add(1);
            self.nbits -= 1;
        }
        while n > 0 {
            if n <= self.nbits {
                self.nbits -= n;
                let lo = (self.word >> self.nbits) & mask(n);
                return Some(v.wrapping_shl(n) | lo);
            }
            let lo = self.word & mask(self.nbits);
            v = if self.nbits > 0 { v.wrapping_shl(self.nbits) } else { v } | lo;
            n -= self.nbits;
            self.refill();
        }
        Some(v)
    }

    fn svar(&mut self, n: u32) -> Option<i32> {
        let u = self.uvar(n + 1)?;
        Some(if u & 1 != 0 { !(u >> 1) as i32 } else { (u >> 1) as i32 })
    }
}

fn mask(n: u32) -> u32 {
    if n >= 32 {
        u32::MAX
    } else {
        (1u32 << n) - 1
    }
}

fn to_s16(v: i32) -> i16 {
    v.min(0x7fff) as u16 as i16
}

#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    Truncated,
    UnsupportedCommand(u32),
}

/// Decode one unit payload into 16-bit samples, appending to `out`.
pub fn decode_into(coded: &[u8], out: &mut Vec<i16>) -> Result<(), DecodeError> {
    let mut br = BitReader::new(coded);
    let mut blocksize: u32 = 256;
    let mut bitshift: u32 = 0;
    let mut means = [0i32; 4];
    let (mut h3, mut h2, mut h1) = (0i32, 0i32, 0i32);
    let mut blk: Vec<i32> = Vec::with_capacity(256);

    let mut cmd = br.uvar(2).ok_or(DecodeError::Truncated)?;
    while cmd != FN_QUIT {
        match cmd {
            0..=3 | FN_ZERO => {
                let energy = if cmd == FN_ZERO { 0 } else { br.uvar(3).ok_or(DecodeError::Truncated)? };
                let msum: i64 = means.iter().map(|&m| m as i64).sum();
                let mut coffset = ((2 + msum) / 4) as i32;
                if bitshift > 0 {
                    coffset = (coffset >> (bitshift - 1)) >> 1;
                }
                blk.clear();
                let mut sum: i64 = 0;
                for _ in 0..blocksize {
                    let p: i64 = match cmd {
                        0 => br.svar(energy).ok_or(DecodeError::Truncated)? as i64 + coffset as i64,
                        1 => br.svar(energy).ok_or(DecodeError::Truncated)? as i64 + h1 as i64,
                        2 => br.svar(energy).ok_or(DecodeError::Truncated)? as i64 + 2 * h1 as i64 - h2 as i64,
                        3 => {
                            br.svar(energy).ok_or(DecodeError::Truncated)? as i64
                                + 3 * (h1 as i64 - h2 as i64)
                                + h3 as i64
                        }
                        _ => 0,
                    };
                    let v = p as u32 as i32;
                    blk.push(v);
                    sum += v as i64;
                    if (1..=3).contains(&cmd) {
                        h3 = h2;
                        h2 = h1;
                        h1 = v;
                    }
                }
                if blocksize > 0 {
                    let acc = ((blocksize / 2) as i64 + sum) as u32 as i32;
                    means.rotate_left(1);
                    means[3] = ((acc / blocksize as i32) as u32).wrapping_shl(bitshift) as i32;
                    if cmd == 0 || cmd == FN_ZERO {
                        let n = blk.len();
                        match n {
                            1 => {
                                h3 = h2;
                                h2 = h1;
                                h1 = blk[0];
                            }
                            2 => {
                                h3 = h1;
                                h2 = blk[0];
                                h1 = blk[1];
                            }
                            _ => {
                                h3 = blk[n - 3];
                                h2 = blk[n - 2];
                                h1 = blk[n - 1];
                            }
                        }
                    }
                }
                out.extend(blk.iter().map(|&v| to_s16(if bitshift > 0 { (v as u32).wrapping_shl(bitshift) as i32 } else { v })));
            }
            FN_BLOCKSIZE => {
                let bits = br.uvar(2).ok_or(DecodeError::Truncated)?;
                blocksize = br.uvar(bits).ok_or(DecodeError::Truncated)?;
                if blocksize > 1 << 20 {
                    return Err(DecodeError::Truncated);
                }
            }
            FN_BITSHIFT => bitshift = br.uvar(2).ok_or(DecodeError::Truncated)?,
            other => return Err(DecodeError::UnsupportedCommand(other)),
        }
        if br.overrun() {
            return Err(DecodeError::Truncated);
        }
        cmd = br.uvar(2).ok_or(DecodeError::Truncated)?;
    }
    Ok(())
}

pub fn decode(coded: &[u8]) -> Result<Vec<i16>, DecodeError> {
    let mut out = Vec::new();
    decode_into(coded, &mut out)?;
    Ok(out)
}
