//! Formally verified helpers. `cargo verus verify` proves their `ensures`;
//! a normal `cargo build` compiles them as plain Rust (specs are erased).

use vstd::prelude::*;

verus! {

/// Number of `#` for a markdown heading of `level`: always 1..=6, and equal to
/// `level` whenever `level` is already valid.
// Explicit comparisons, not `u64::clamp`: Verus proves the `ensures` from these branches.
#[allow(clippy::manual_clamp)]
#[must_use]
pub fn heading_hashes(level: u64) -> (n: u64)
    ensures
        1 <= n <= 6,
        1 <= level <= 6 ==> n == level,
{
    if level < 1 {
        1
    } else if level > 6 {
        6
    } else {
        level
    }
}

} // verus!
