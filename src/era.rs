//! Engine generation. The 2014 builds (vt_eng.dll 3.11.7.1, vt_spa_violeta16.dll 2013) and the three 2009
//! builds read different voice data; each build loads only its own, so the data picks the build:
//!
//! | Era | DLL | index header | trees |
//! |---|---|---|---|
//! | `V2014` | vt_eng.dll / vt_spa_violeta16.dll (2013) | `ver.2013` only | tree3 |
//! | `Paul09` | vt_eng_paul16.dll (2009-09-15) | `ver.2009` only | `ttsdata\tree\*.tree2` |
//! | `Julie09` | vt_eng_julie16.dll (Dec 2009) | `ver.2009` only | tree3 |
//! | `Violeta09` | vt_spa_violeta16.dll (Dec 2009) | any `ver.` (ships `ver.2005`) | tree3 |

use std::io;
use std::path::Path;

use crate::bank::Bank;
use crate::lang::Lang;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Era {
    #[default]
    V2014,
    Paul09,
    Julie09,
    Violeta09,
    Paul04,
}

impl Era {
    /// The build that loads this bank (`m16` is its voice directory).
    pub fn detect(bank: &Bank, m16: &Path, lang: Lang) -> io::Result<Era> {
        if bank.version == "ver.2013" {
            return Ok(Era::V2014);
        }
        if bank.version == "ver.2004" && lang == Lang::Eng {
            return Ok(Era::Paul04);
        }
        match lang {
            Lang::Spa => Ok(Era::Violeta09),
            Lang::Eng if bank.version == "ver.2009" => {
                if m16.join("ttsdata").join("tree").is_dir() && !m16.join("ttsdata").join("tree3").is_dir() {
                    Ok(Era::Paul09)
                } else {
                    Ok(Era::Julie09)
                }
            }
            Lang::Eng => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}: {} English voice data is not supported (ver.2013, ver.2009 and ver.2004 are)", m16.display(), bank.version),
            )),
        }
    }

    pub fn is_2009(self) -> bool {
        !matches!(self, Era::V2014 | Era::Paul04)
    }

    pub fn is_2004(self) -> bool {
        self == Era::Paul04
    }
}
