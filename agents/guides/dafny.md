# Dafny

Dafny is a programming language with verification built in. Methods carry `requires` (preconditions), `ensures` (postconditions) and loop `invariant`s, and the verifier proves them with an SMT solver (Z3) before the program runs. Verified code can then be compiled to C#, Java, JavaScript, Go or Python (C++ is experimental; release 4.11 has no Rust backend). In this repo it is the default language for LLM programming: a model writes code and specs, and the verifier tells it exactly what does not hold.

## Install (Linux, x86_64)

The releases of dafny-lang/dafny are self-contained: each zip carries its own .NET runtime and Z3, so nothing else is needed to verify. Download the Ubuntu zip of the latest release, unpack it into `~/.local/dafny` and link the binary:

```sh
gh release download v4.11.0 --repo dafny-lang/dafny --pattern '*-x64-ubuntu-22.04.zip'
python3 -m zipfile -e dafny-4.11.0-x64-ubuntu-22.04.zip .
mv dafny ~/.local/dafny
chmod +x ~/.local/dafny/{dafny,DafnyDriver,DafnyServer,createdump} ~/.local/dafny/z3/bin/*
ln -sf ~/.local/dafny/dafny ~/.local/bin/dafny
```

`python3 -m zipfile` does not keep the executable bit, hence the `chmod`; `unzip` keeps it. macOS and Windows have their own zips in the same release. Check the install with `dafny --version`.

Compiling to a target language needs that language's toolchain (the .NET SDK for C#, a JDK for Java, `go` for Go); verifying does not.

## Commands

```sh
dafny verify file.dfy            # prove every spec in the file
dafny run file.dfy               # verify, then run Main
dafny build --target py file.dfy # verify, then compile (cs, go, js, java, py; cpp experimental)
dafny format file.dfy            # format the file
```

A successful run ends with `Dafny program verifier finished with N verified, 0 errors`. A spec that does not hold points at the line and at the postcondition that could not be proved.

## Writing verified code

```dafny
method HeadingHashes(level: nat) returns (n: nat)
  ensures 1 <= n <= 6
  ensures 1 <= level <= 6 ==> n == level
{
  if level < 1 { n := 1; } else if level > 6 { n := 6; } else { n := level; }
}
```

This is the same contract as `heading_hashes` in `src/verified.rs`, which Verus proves in Rust. Drop the `level < 1` branch and Dafny reports `a postcondition could not be proved on this return path` and points at `1 <= n`.

Loops need an `invariant` that holds before and after every iteration, and a `decreases` clause when Dafny cannot guess why the loop ends. Functions (`function`) are pure and can be used inside specs; methods (`method`) can have effects.

## Sources

- Install: https://dafny.org/latest/Installation
- Releases: https://github.com/dafny-lang/dafny/releases
- Reference manual: https://dafny.org/latest/DafnyRef/DafnyRef
- Getting started: https://dafny.org/latest/OnlineTutorial/guide
