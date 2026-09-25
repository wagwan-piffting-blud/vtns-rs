//! Internal phone codes: 1..=0x45 index the alphabetical CMU list with the
//! lexical stress digit folded in (vt_eng.dll 0x10079138). 0x5A pads an empty
//! context slot in the 7-byte unit label.

pub const PAD: u8 = 0x5a;

pub const NAMES: [&str; 0x46] = [
    "?", "AA0", "AA1", "AA2", "AE0", "AE1", "AE2", "AH0", "AH1", "AH2", "AO0", "AO1", "AO2", "AW0", "AW1", "AW2",
    "AY0", "AY1", "AY2", "B", "CH", "D", "DH", "EH0", "EH1", "EH2", "ER0", "ER1", "ER2", "EY0", "EY1", "EY2", "F",
    "G", "HH", "IH0", "IH1", "IH2", "IY0", "IY1", "IY2", "JH", "K", "L", "M", "N", "NG", "OW0", "OW1", "OW2", "OY0",
    "OY1", "OY2", "P", "R", "S", "SH", "T", "TH", "UH0", "UH1", "UH2", "UW0", "UW1", "UW2", "V", "W", "Y", "Z", "ZH",
];

pub fn name(code: u8) -> String {
    match code {
        1..=0x45 => NAMES[code as usize].to_string(),
        PAD => "-".to_string(),
        c => format!("x{:02X}", c),
    }
}

pub fn code(name: &str) -> Option<u8> {
    let up = name.to_ascii_uppercase();
    NAMES.iter().position(|&n| n == up).filter(|&i| i > 0).map(|i| i as u8)
}
