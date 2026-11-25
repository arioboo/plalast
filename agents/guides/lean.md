# Lean 4

Lean 4 is a programming language and a proof assistant. You state theorems as types and write proofs as programs (or with tactics such as `simp`, `omega` and `decide`); the Lean kernel checks every step, so a file that compiles is a proof that its theorems hold. In this repo it is the tool for formal verification and for an expandable base of checked facts.

## Install (Linux and macOS)

Lean is installed through `elan`, its toolchain manager (the Lean counterpart of `rustup`). The installer only downloads the `elan` binary from the releases of leanprover/elan; read it before running it. Pass `--no-modify-path` so it leaves the shell profile alone, and link the binaries into `~/.local/bin`, next to `verus` and `cargo`, so non-interactive shells find them:

```sh
curl -sSfL https://raw.githubusercontent.com/leanprover/elan/master/elan-init.sh -o elan-init.sh
sh elan-init.sh -y --no-modify-path --default-toolchain leanprover/lean4:v4.35.0
for b in elan lean lake; do ln -sf ~/.elan/bin/$b ~/.local/bin/$b; done
```

Check the install with `lean --version` and `lake --version`. A toolchain takes about 3 GB in `~/.elan`.

Pin a release, not `stable`: a project records its toolchain in a `lean-toolchain` file, and `elan` downloads that exact version on first use, so every machine checks the proofs with the same Lean.

## Commands

```sh
lean file.lean          # check one file: no output means every theorem holds
lake new <name> math    # new project with Mathlib (`lake new <name>` for a plain one)
lake build              # check every file of the project
lake update             # update the dependencies listed in lakefile.toml
elan toolchain list     # installed toolchains
```

A failed proof prints the file, line and column, and the goal Lean could not close.

## Writing proofs

```lean
theorem add_comm' (a b : Nat) : a + b = b + a := Nat.add_comm a b

theorem small : 2 + 2 = 4 := by decide

example (x : Nat) (h : x < 5) : x + 1 ≤ 5 := by omega
```

`decide` evaluates a decidable proposition, `omega` closes linear arithmetic over integers and naturals, and `simp` rewrites with known lemmas. A false statement such as `2 + 2 = 5 := by decide` fails with ``Tactic `decide` proved that the proposition 2 + 2 = 5 is false``.

Mathlib is the large community library of formalized mathematics; add it only to projects that need it, since it downloads several GB of prebuilt files.

## Sources

- Install: https://lean-lang.org/install/
- elan: https://github.com/leanprover/elan
- Releases: https://github.com/leanprover/lean4/releases
- Theorem Proving in Lean 4: https://lean-lang.org/theorem_proving_in_lean4/
- Mathlib: https://github.com/leanprover-community/mathlib4
