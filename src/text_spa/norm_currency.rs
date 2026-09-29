//! `FUN_100289e0` (chain slot 3): currency amounts and large grouped
//! numbers ("US$ 5.000.000,00", "$50", parenthesised amounts, etc).
//!
//! Still declined this round, but the round-3 doc below undersold the
//! scope: `FUN_100289e0` itself is 2890 lines (not 561 -- that number was
//! stale, from an earlier truncated read), and this round additionally
//! read `FUN_1002ac70` and `FUN_10029540` in full (both previously marked
//! "unread"), which corrects a real mischaracterization and confirms the
//! actual renderers are two *more*, still-unread functions bigger than
//! either of them:
//!
//! - `FUN_1002ac70` (430 lines) is **not** the renderer -- it's a per-shape
//!   *validation* gate, structurally the currency family's counterpart to
//!   `norm_units.rs`'s `FUN_10032660`: a `switch` on the matched shape's
//!   table index, rejecting on whitespace-count thresholds (`param_1+0x148`-
//!   style fields, the same per-token-slot layout convention as
//!   `norm_fraction.rs`'s `FUN_100303b0`/`FUN_10030220`) and a handful of
//!   parenthesis/field-set checks. It renders nothing.
//! - `FUN_10029540` (492 lines, read in full) is the **commit dispatcher**:
//!   it scans the committed per-token tag bytes (at each slot's own `+0x1a`,
//!   same convention again) for the first `'N'`, `'U'`/`'u'`, `'$'`, `'C'`,
//!   `'S'`, or `'F'`/`'f'` tag, uses whichever of `'U'`/`'C'`/`'F'` it finds
//!   to read a gender flag out of one of three currency-name tables (stride
//!   0x2a at `DAT_1006aca9` for a `'U'` hit, stride 0x42 at `DAT_1006b1e1`/
//!   `DAT_1006ddf9` for `'S'`/`'F'`), then dispatches to `FUN_10029730`
//!   (2772 lines) when no `'U'`/`'u'` tag was found and a `FUN_10006fe0`
//!   check against `DAT_1006905c` passes, else to `FUN_1002a210` (2649
//!   lines) -- confirmed by reading `FUN_10029540`'s own dispatch, not
//!   guessed. Those two functions are where the actual gender/plural
//!   agreement and "con" cents-joining words are decided, and both remain
//!   entirely unread.
//!
//! With the scanner (2890), the two renderers (2772 + 2649), the two
//! already-assigned commit/validate functions above (492 + 430), and the
//! remaining named helpers (`FUN_1002ae70`, `FUN_1002afd0`, `FUN_1002b0f0`,
//! `FUN_1002b260`, `FUN_10043860`, `FUN_10044630`, `FUN_10043f00`,
//! `FUN_10043a60`, `FUN_100437a0`) plus several currency-name string tables
//! still unread, this family totals north of 10,000 lines of decompile --
//! an order of magnitude past `norm_fraction.rs`'s full four-shape scope
//! this same round. Porting the signature-matching skeleton without the
//! renderers would still be guessing at the one part that actually produces
//! spoken text, so this declines rather than approximate it. Always
//! returns -1 ("not mine"), matching the round-1 placeholder's observable
//! behaviour. The full-corpus mismatch dump (see the batch's shared brief)
//! backs deprioritizing this further: the NWS marine/land forecast corpus
//! this voice actually serves is overwhelmingly units, ranges and
//! currency-free numbers -- bare dollar amounts essentially don't occur in
//! it, unlike `norm_fraction.rs`'s shapes, which do and were verified
//! against fresh captures this round.

use super::tokenize::Tok;
use super::FeSpa;
use crate::ling_spa::WaSpa;

pub fn currency(_fe: &FeSpa, _wa: &mut WaSpa, _tok: &mut Tok, _t: &[u8], _p: usize, _off: i32) -> i32 {
    -1
}
