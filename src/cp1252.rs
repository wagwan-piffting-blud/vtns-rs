//! Windows-1252, the only text encoding the engines read.

const HI: [char; 32] = [
    '\u{20ac}', '\u{81}', '\u{201a}', '\u{192}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{2c6}',
    '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8d}', '\u{17d}', '\u{8f}', '\u{90}', '\u{2018}',
    '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}', '\u{2dc}', '\u{2122}', '\u{161}',
    '\u{203a}', '\u{153}', '\u{9d}', '\u{17e}', '\u{178}',
];

/// The Windows-1252 byte for `c`, if it has one.
pub fn byte(c: char) -> Option<u8> {
    match c as u32 {
        u @ (0..=0x7f | 0xa0..=0xff) => Some(u as u8),
        _ => HI.iter().position(|&h| h == c).map(|p| 0x80 + p as u8),
    }
}

/// UTF-8 text as Windows-1252 bytes; characters with no Windows-1252 code
/// become `?` and a leading BOM is dropped. Input that is not valid UTF-8 is
/// assumed to be Windows-1252 already and passed through.
pub fn from_utf8(b: &[u8]) -> Vec<u8> {
    let Ok(s) = std::str::from_utf8(b) else {
        return b.to_vec();
    };
    s.strip_prefix('\u{feff}').unwrap_or(s).chars().map(|c| byte(c).unwrap_or(b'?')).collect()
}
