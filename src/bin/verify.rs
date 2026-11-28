//! verify: checks a change at several levels before it becomes part of the repo
//! (the static premise "Only small diff steps").
//!
//! - `pla-verify`                                 every level.
//! - `pla-verify [consistency] [logic] [formal]`  only the given levels.
//!
//! `consistency`: the repo agrees with what is already known: `pla-build --check` and
//!                `pla-lint memory modules`.
//! `logic`:       the Rust code is well formed (`pla-lint rust`: fmt, clippy) and its logic tests
//!                pass (`cargo test`: unit tests, `tests/logic.rs` and `tests/wiring.rs`).
//! `formal`:      formal verification of the `verus!` code: `cargo verus verify`; skipped with a
//!                warning where Verus is not installed (see agents/guides/verus.md).
//!
//! The last level is human: it lists the changed files for a person to review; it never fails.
//! Still precarious: few proofs, and the formal level is often skipped.

use plalast::{cargo, exit, open, run, Findings};
use std::env;
use std::path::Path;
use std::process::ExitCode;

const LEVELS: [&str; 3] = ["consistency", "logic", "formal"];

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    exit((|| {
        if let Some(bad) = args.iter().find(|a| !LEVELS.contains(&a.as_str())) {
            return Err(format!(
                "unknown level {bad}\nusage: pla-verify [consistency] [logic] [formal]"
            ));
        }
        let want = |level: &str| args.is_empty() || args.iter().any(|a| a == level);
        let (root, _cfg) = open()?;
        let mut f = Findings::default();
        if want("consistency") {
            consistency(&root, &mut f)?;
        }
        if want("logic") {
            logic(&root, &mut f)?;
        }
        if want("formal") {
            formal(&root, &mut f);
        }
        human(&root);
        Ok(f.finish("pla-verify"))
    })())
}

/// The last 30 lines of a tool's output: enough to see why it failed.
fn tail(out: &str) -> String {
    let lines: Vec<&str> = out.lines().collect();
    lines[lines.len().saturating_sub(30)..].join("\n")
}

/// Runs one check of a level and records an error when it fails.
fn step(root: &Path, f: &mut Findings, level: &str, label: &str, program: &str, args: &[&str]) {
    match run(root, program, args) {
        Ok((true, _)) => println!("ok    [{level}] {label}"),
        Ok((false, out)) => f.error(level, format!("{label} failed:\n{}", tail(&out))),
        Err(e) => f.error(level, format!("{label}: {e}")),
    }
}

/// pla-build and pla-lint, from the same folder as this binary (built together by cargo).
fn consistency(root: &Path, f: &mut Findings) -> Result<(), String> {
    let me = env::current_exe().map_err(|e| e.to_string())?;
    let dir = me.parent().ok_or("pla-verify has no folder")?;
    let tool = |name: &str| dir.join(name).to_string_lossy().into_owned();
    step(
        root,
        f,
        "consistency",
        "pla-build --check",
        &tool("pla-build"),
        &["--check"],
    );
    step(
        root,
        f,
        "consistency",
        "pla-lint memory modules",
        &tool("pla-lint"),
        &["memory", "modules"],
    );
    Ok(())
}

/// The Rust code is well formed (`pla-lint rust`: fmt and clippy) and its logic tests pass.
fn logic(root: &Path, f: &mut Findings) -> Result<(), String> {
    let me = env::current_exe().map_err(|e| e.to_string())?;
    let lint = me.with_file_name("pla-lint").to_string_lossy().into_owned();
    step(
        root,
        f,
        "logic",
        "pla-lint rust (fmt, clippy)",
        &lint,
        &["rust"],
    );
    step(
        root,
        f,
        "logic",
        "cargo test",
        &cargo(),
        &["test", "--release", "--quiet"],
    );
    Ok(())
}

/// `cargo verus verify`, or a warning that the formal level was skipped.
fn formal(root: &Path, f: &mut Findings) {
    let cargo = cargo();
    match run(root, &cargo, &["verus", "--help"]) {
        Ok((true, _)) => step(root, f, "formal", "cargo verus verify", &cargo, &["verus", "verify"]),
        _ => f.warn(
            "formal",
            "cargo-verus is not installed, formal verification skipped (see agents/guides/verus.md)",
        ),
    }
}

/// The human level: what a person should review before the change is kept.
fn human(root: &Path) {
    let changed = run(root, "git", &["status", "--porcelain"])
        .map(|(_, out)| out.lines().count())
        .unwrap_or(0);
    println!(
        "human [review] {changed} changed file(s): read the diff (git diff) before committing"
    );
}
