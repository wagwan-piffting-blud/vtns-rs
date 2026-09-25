//! Debug aid, not a parity test: prints our text front end's WA records for
//! `WA_TEXT` (data root from `VT_ROOT`, default the installed engine).
//!     WA_TEXT="The NWS's forecast." cargo test --release --test wa_dump -- --nocapture

use std::path::Path;

use vtpaul::text::{self, Fe};

#[test]
fn wa_dump() {
    let Ok(t) = std::env::var("WA_TEXT") else {
        return;
    };
    let root = std::env::var("VT_ROOT").unwrap_or_else(|_| r"C:\Program Files (x86)\NWSPaulNumber2".into());
    let fe = Fe::load(&Path::new(&root).join("data-common")).expect("load front-end data");
    let ini = text::initial::initial(t.as_bytes());
    for (k, s) in text::sentences(&fe, &ini.processed).iter().enumerate() {
        println!("sent {} off={} n={} consumed={}", k, s.text_off, s.wa.n(), s.wa.consumed());
        for i in 0..s.wa.n() as i32 {
            let b = text::wa::Wa::rec(i);
            let r = &s.wa.b[b..b + 0x94];
            let i32at = |o: usize| i32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]);
            let cs = |o: usize, n: usize| r[o..o + n].iter().take_while(|&&c| c != 0).copied().collect::<Vec<u8>>();
            println!(
                "  {:?} {}{} cls={:#x} {:?} ph={}",
                String::from_utf8_lossy(&cs(0x34, 0x1e)),
                r[0x23] as char,
                r[0x24] as char,
                i32at(0x28),
                [0, 4, 8, 0x10, 0x14, 0x18, 0x1c].map(i32at),
                cs(0x52, 0x42).iter().map(|c| format!("{:02x}", c)).collect::<String>()
            );
        }
    }
}
