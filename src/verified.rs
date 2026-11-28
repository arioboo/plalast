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

/// Number shown for the item at `index` of an ordered list: the 0-based index plus one.
/// `requires` keeps the addition from overflowing; `ensures` states the whole contract.
#[must_use]
pub fn ordered_number(index: u64) -> (n: u64)
    requires
        index < u64::MAX,
    ensures
        n == index + 1,
        n >= 1,
{
    index + 1
}

} // verus!
