//! The unit database: `dblist.idx`, `mc_idx_tbl/unit-*.idx`, `dat/merged-*.{dat,upm}`
//! (vt_eng.dll `us_LoadUnitIndexes` 0x10019f40 and `us_ReadUnitColumns2013` 0x10019940).
//!
//! Like the engine's FILEIO build, the columns used by unit selection are held in
//! memory and the 19-byte primary records, audio and pitch marks are read from
//! the open files on demand.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use crate::shorten;

pub const PRIMARY_SIZE: usize = 19;

/// Domain byte derived from a dblist tag (`us_DomainFromDblistTag` 0x10019cb0).
pub fn domain_from_tag(tag: &str) -> u8 {
    match tag {
        "GEN" => 0,
        "DOMAIN" => 1,
        "ABC" => 2,
        "NUM" => 3,
        "SYL" => 7,
        "NUM_SPLIT" => 8,
        "EXP" => 0x0c,
        _ => 100,
    }
}

/// The 2009 engines' `us_DomainFromDblistTag` (julie09 `FUN_1001b3f0`): the 2014 tags plus `NAME` (0x0b).
pub fn domain_from_tag09(tag: &str) -> u8 {
    match tag {
        "NAME" => 0x0b,
        _ => domain_from_tag(tag),
    }
}

/// One boundary feature triple (unit start, middle or end).
#[derive(Debug, Clone, Copy, Default)]
pub struct Boundary {
    /// Cepstral VQ codeword, index into `cepdist.tbl` (spec & 0x3fff).
    pub cep: u16,
    /// Excitation class (spec >> 14), 0..=2.
    pub class: u8,
    /// Byte feature compared with lower join weight (likely energy).
    pub feat_b: u8,
    /// Quantized F0 at this boundary.
    pub feat_f: u8,
}

/// The on-demand part of a unit (primary record).
#[derive(Debug, Clone, Copy, Default)]
pub struct Primary {
    pub pcm_pos: u32,
    pub half1_len: u16,
    pub half2_len: u16,
    pub coded_size: u16,
    pub pm_pos: u32,
    pub pm1: u8,
    pub pm2: u8,
    /// First / middle / last pitch period in half-samples (x2 = samples).
    pub periods: [u8; 3],
}

impl Primary {
    fn parse(r: &[u8]) -> Primary {
        Primary {
            pcm_pos: u32::from_le_bytes([r[0], r[1], r[2], r[3]]),
            half1_len: u16::from_le_bytes([r[4], r[5]]),
            half2_len: u16::from_le_bytes([r[6], r[7]]),
            coded_size: u16::from_le_bytes([r[8], r[9]]),
            pm_pos: u32::from_le_bytes([r[10], r[11], r[12], r[13]]),
            pm1: r[14],
            pm2: r[15],
            periods: [r[16], r[17], r[18]],
        }
    }
}

struct RandomFile {
    f: File,
}

impl RandomFile {
    fn open(p: &Path) -> io::Result<Self> {
        Ok(RandomFile { f: File::open(p)? })
    }

    fn read_at(&self, buf: &mut [u8], off: u64) -> io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileExt;
            self.f.read_exact_at(buf, off)
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::FileExt;
            let mut done = 0;
            while done < buf.len() {
                let n = self.f.seek_read(&mut buf[done..], off + done as u64)?;
                if n == 0 {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
                done += n;
            }
            Ok(())
        }
    }
}

pub struct Pool {
    pub name: String,
    pub tag: String,
    pub domain: u8,
    /// Global index of this pool's first unit.
    pub base: u32,
    pub n: u32,
    idx: RandomFile,
    primary_off: u64,
    dat: RandomFile,
    upm: RandomFile,
}

/// Column layout of the unit indexes, from the `ver.YYYY` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdxLayout {
    /// `ver.2013`: file index, 7-byte label, duration code, boundaries (21 bytes per unit).
    V2013,
    /// `ver.2009` / `ver.2005` (the 2009 engines' `FUN_1001b050`): file index, flag, 5-byte label,
    /// duration code, boundaries (20 bytes per unit).
    V2009,
}

/// All pools merged into one global unit array, in dblist order.
pub struct Bank {
    pub root: PathBuf,
    pub pools: Vec<Pool>,
    pub layout: IdxLayout,
    /// The `ver.YYYY` string of the first index, e.g. `ver.2013`.
    pub version: String,
    /// Per global unit: which pool (== .dat/.upm file index).
    pub file_idx: Vec<u8>,
    /// Raw first column of every index (the unit's dat-file number within its pool).
    pub file_col: Vec<u8>,
    /// ver.2013 only, per global unit: `[L2, L1, C, R1, R2, posbits, flags]`.
    pub label: Vec<[u8; 7]>,
    /// ver.2009 only, per global unit: `[L1, C, R1, posbits, flags]`.
    pub label5: Vec<[u8; 5]>,
    /// ver.2009 only, per global unit: the 0/1 column at pool +0x4c.
    pub flag: Vec<u8>,
    pub dur_code: Vec<u8>,
    /// Per global unit: start / middle / end boundary features.
    pub bounds: Vec<[Boundary; 3]>,
}

fn bad(msg: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

impl Bank {
    /// `m16` is the voice directory, e.g. `.../data-paul/M16`.
    pub fn open(m16: &Path) -> io::Result<Bank> {
        let list = std::fs::read_to_string(m16.join("dblist.idx"))?;
        let mut tok = list.split_whitespace();
        let count: usize = tok.next().and_then(|s| s.parse().ok()).ok_or_else(|| bad("dblist count".into()))?;
        let mut bank = Bank {
            root: m16.to_path_buf(),
            pools: Vec::new(),
            layout: IdxLayout::V2013,
            version: String::new(),
            file_idx: Vec::new(),
            file_col: Vec::new(),
            label: Vec::new(),
            label5: Vec::new(),
            flag: Vec::new(),
            dur_code: Vec::new(),
            bounds: Vec::new(),
        };
        for _ in 0..count {
            let file = tok.next().ok_or_else(|| bad("dblist truncated".into()))?;
            let tag = tok.next().ok_or_else(|| bad("dblist truncated".into()))?;
            bank.load_pool(file, tag)?;
        }
        Ok(bank)
    }

    fn load_pool(&mut self, file: &str, tag: &str) -> io::Result<()> {
        let path = self.root.join("mc_idx_tbl").join(file);
        let raw = std::fs::read(&path)?;
        let short = || bad(format!("{}: truncated index", path.display()));
        if raw.len() < 9 {
            return Err(short());
        }
        // Headerless indexes use the interleaved legacy records (`us_ReadUnitColumnsLegacy`); no
        // known bank ships them.
        if !raw[1..].starts_with(b"ver.") {
            return Err(bad(format!("{}: headerless (pre-2005) index, not supported", path.display())));
        }
        let version = String::from_utf8_lossy(&raw[1..9]).into_owned();
        let layout = if version == "ver.2013" { IdxLayout::V2013 } else { IdxLayout::V2009 };
        if self.pools.is_empty() {
            self.layout = layout;
            self.version = version;
        } else if layout != self.layout {
            return Err(bad(format!("{}: {} mixed with {} in one bank", path.display(), version, self.version)));
        }
        let rd16 = |p: usize| raw.get(p..p + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize).ok_or_else(short);
        let mut p = 1 + raw[0] as usize;
        let nfiles = rd16(p)?;
        p += 2;
        if nfiles != 1 {
            return Err(bad(format!("{}: {} dat files per pool", path.display(), nfiles)));
        }
        let ln = rd16(p)?;
        let dat_name = String::from_utf8_lossy(raw.get(p + 2..p + 2 + ln).ok_or_else(short)?).into_owned();
        p += 2 + ln + 1;
        let n = u32::from_le_bytes(raw.get(p..p + 4).ok_or_else(short)?.try_into().unwrap());
        p += 4;
        let s = rd16(p)?;
        p += 2;
        if s != PRIMARY_SIZE {
            return Err(bad(format!("{}: primary size {}", path.display(), s)));
        }
        let nn = n as usize;
        let cols = match layout {
            IdxLayout::V2013 => 1 + 7 + 1 + 12,
            IdxLayout::V2009 => 1 + 1 + 5 + 1 + 12,
        };
        let need = p + (PRIMARY_SIZE + cols) * nn;
        if need != raw.len() {
            return Err(bad(format!("{}: {} bytes, expected {} for {} units", path.display(), raw.len(), need, nn)));
        }
        let primary_off = p as u64;
        p += s * nn;
        let fidx = self.pools.len() as u8;
        self.file_idx.extend(std::iter::repeat(fidx).take(nn));
        self.file_col.extend_from_slice(&raw[p..p + nn]);
        p += nn;
        match layout {
            IdxLayout::V2013 => {
                for i in 0..nn {
                    let mut l = [0u8; 7];
                    l.copy_from_slice(&raw[p + 7 * i..p + 7 * i + 7]);
                    self.label.push(l);
                }
                p += 7 * nn;
            }
            IdxLayout::V2009 => {
                self.flag.extend_from_slice(&raw[p..p + nn]);
                p += nn;
                for i in 0..nn {
                    let mut l = [0u8; 5];
                    l.copy_from_slice(&raw[p + 5 * i..p + 5 * i + 5]);
                    self.label5.push(l);
                }
                p += 5 * nn;
            }
        }
        self.dur_code.extend_from_slice(&raw[p..p + nn]);
        p += nn;
        let start = self.bounds.len();
        self.bounds.resize(start + nn, [Boundary::default(); 3]);
        for k in 0..3 {
            for i in 0..nn {
                let spec = u16::from_le_bytes([raw[p + 2 * i], raw[p + 2 * i + 1]]);
                let b = &mut self.bounds[start + i][k];
                b.cep = spec & 0x3fff;
                b.class = (spec >> 14) as u8;
                b.feat_b = raw[p + 2 * nn + i];
                b.feat_f = raw[p + 3 * nn + i];
            }
            p += 4 * nn;
        }
        let name = file.trim_start_matches("unit-").trim_end_matches(".idx").to_string();
        self.pools.push(Pool {
            name,
            tag: tag.to_string(),
            domain: if layout == IdxLayout::V2009 { domain_from_tag09(tag) } else { domain_from_tag(tag) },
            base: start as u32,
            n,
            idx: RandomFile::open(&path)?,
            primary_off,
            dat: RandomFile::open(&self.root.join("dat").join(format!("{}.dat", dat_name)))?,
            upm: RandomFile::open(&self.root.join("dat").join(format!("{}.upm", dat_name)))?,
        });
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.dur_code.len()
    }

    pub fn is_empty(&self) -> bool {
        self.dur_code.is_empty()
    }

    /// (pool, local index) of a global unit.
    pub fn locate(&self, unit: u32) -> (&Pool, u32) {
        let pool = &self.pools[self.file_idx[unit as usize] as usize];
        (pool, unit - pool.base)
    }

    pub fn primary(&self, unit: u32) -> io::Result<Primary> {
        let (pool, local) = self.locate(unit);
        let mut r = [0u8; PRIMARY_SIZE];
        pool.idx.read_at(&mut r, pool.primary_off + local as u64 * PRIMARY_SIZE as u64)?;
        Ok(Primary::parse(&r))
    }

    /// Raw pitch-mark bytes (half-sample periods) for the whole unit. The two
    /// halves share their boundary period, so the span is `pm1 + pm2 - 1` bytes;
    /// these spans tile each `.upm` file exactly. `periods` holds the first,
    /// shared and last bytes, and `half1_len`/`half2_len`/decoded length are
    /// twice the sums of side 1, side 2 and the whole span.
    pub fn pitch_marks(&self, unit: u32, pr: &Primary) -> io::Result<Vec<u8>> {
        let (pool, _) = self.locate(unit);
        let n = (pr.pm1 as usize + pr.pm2 as usize).saturating_sub(1);
        let mut v = vec![0u8; n];
        pool.upm.read_at(&mut v, pr.pm_pos as u64)?;
        Ok(v)
    }

    /// Decoded, unmodified samples of one unit.
    pub fn decode(&self, unit: u32, pr: &Primary) -> io::Result<Vec<i16>> {
        let (pool, _) = self.locate(unit);
        let mut coded = vec![0u8; pr.coded_size as usize];
        pool.dat.read_at(&mut coded, pr.pcm_pos as u64)?;
        shorten::decode(&coded).map_err(|e| bad(format!("unit {}: {:?}", unit, e)))
    }
}
