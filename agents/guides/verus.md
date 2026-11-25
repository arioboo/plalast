# Verus with Rust

Verus checks that Rust code meets its specification. You write `requires` (what callers must guarantee) and `ensures` (what the function promises), and Verus proves them with an SMT solver (Z3) at build time. Specs and proofs are ghost code: they are erased from the compiled program, so verified code runs like plain Rust.

## Install (macOS, Apple Silicon)

Prebuilt releases exist for macOS, Linux and Windows on arm64 and x86. Download the one for your machine from the latest release of verus-lang/verus, unpack it, clear the macOS quarantine flag and put it on your PATH:

```sh
gh release download --repo verus-lang/verus --pattern '*-arm64-macos.zip'
unzip verus-*-arm64-macos.zip
bash verus-arm64-macos/macos_allow_gatekeeper.sh
mv verus-arm64-macos ~/.local/verus
ln -s ~/.local/verus/verus ~/.local/bin/verus
ln -s ~/.local/verus/cargo-verus ~/.local/bin/cargo-verus
```

The gatekeeper script only removes the `com.apple.quarantine` attribute from the Verus binaries; read it before running it. Each release needs one exact Rust toolchain. Run `verus --version`: if the toolchain is missing, Verus prints the command to install it, for example:

```sh
rustup install 1.100.0-aarch64-apple-darwin
```

Check the install with `verus --version` (version, platform and toolchain) and `cargo verus --help`.

## How this repo uses it

- `Cargo.toml` depends on `vstd`, the Verus standard library, pinned to the version that matches the installed release.
- `[package.metadata.verus] verify = true` tells `cargo verus` to verify this crate.
- `[lints.rust]` declares `cfg(verus_only)`, a flag Verus turns on while verifying.
- `src/verified.rs` holds verified code. `heading_hashes` proves that a markdown heading always gets 1 to 6 `#`; `pla-build` and the PDF renderer use it.
- `pla-lint` runs `cargo verus verify` in its `rust` area when `cargo-verus` is installed, and warns when it is not.

## Commands

```sh
cargo verus verify   # verify the crate (runs cargo build under Verus)
cargo verus focus    # verify only this crate, not its dependencies
cargo verus build    # verify, then build
verus file.rs        # verify one standalone file
cargo build          # normal build: specs are erased, nothing is verified
```

A successful run prints lines such as `verification results:: 1 verified, 0 errors`. A spec that does not hold fails the build with `postcondition not satisfied` and points at the failing `ensures`.

## Writing verified code

Verified code lives inside the `verus!` macro, with the `vstd` prelude in scope. Name the return value to talk about it in `ensures`:

```rust
use vstd::prelude::*;

verus! {

fn octuple(x1: i8) -> (x8: i8)
    requires
        -16 <= x1 < 16,
    ensures
        x8 == 8 * x1,
{
    let x2 = x1 + x1;
    let x4 = x2 + x2;
    x4 + x4
}

} // verus!
```

The `requires` clause is what makes the proof possible: without it, `x1 + x1` could overflow an `i8`, and Verus would reject the function. Callers must prove the precondition too, so `octuple(100)` does not verify.

Keep verified code small and pure at first: integer arithmetic, bounds and simple data. Code outside `verus!` is not checked, so the rest of the crate keeps compiling as usual.

## New projects and upgrades

- `cargo verus new --lib <name>` (or `--bin <name>`) creates a project with `vstd`, the metadata and the lint already set up.
- To upgrade, install the new release as above, then pin `vstd` in `Cargo.toml` to the version that `cargo verus new` writes for that release.

## Sources

- Install: https://github.com/verus-lang/verus/blob/main/INSTALL.md
- Releases: https://github.com/verus-lang/verus/releases
- Guide: https://verus-lang.github.io/verus/guide/
- requires and ensures: https://verus-lang.github.io/verus/guide/requires_ensures.html
