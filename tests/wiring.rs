//! End-to-end wiring: runs the four compiled tools against the real config.json of this repo.
//! If a memory key, a generated file or a tool drifts apart, one of these fails.

use plalast::{load, primary, CONFIG_KEYS, MODULE_KINDS, PRIMARY_KEYS};
use std::path::Path;
use std::process::{Command, Output};

fn run(bin: &str, args: &[&str]) -> Output {
    Command::new(bin)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("tool runs")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

const BUILD: &str = env!("CARGO_BIN_EXE_pla-build");
const RECALL: &str = env!("CARGO_BIN_EXE_pla-recall");
const LINT: &str = env!("CARGO_BIN_EXE_pla-lint");
const AUDIT: &str = env!("CARGO_BIN_EXE_pla-audit");

#[test]
fn repo_is_in_sync_with_the_memory() {
    let o = run(BUILD, &["--check"]);
    assert!(o.status.success(), "pla-build --check:\n{}", stdout(&o));
}

#[test]
fn every_generated_doc_has_the_agent_note_on_line_two() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cfg = load(root).unwrap();
    let note = cfg["primary"]["docs"]["agent_note"].as_str().unwrap();
    let mut checked = 0;
    for e in cfg["data"]["structure"].as_array().unwrap() {
        let (Some(path), Some(build)) = (e["path"].as_str(), e["build"].as_str()) else {
            continue;
        };
        if build == "gitignore" {
            continue;
        }
        let text = std::fs::read_to_string(root.join(path)).unwrap();
        assert_eq!(text.lines().nth(1), Some(note), "{path} line 2");
        checked += 1;
    }
    assert!(checked >= 7, "only {checked} docs checked");
}

#[test]
fn memory_keys_match_the_tools_registry() {
    let cfg = load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let present: Vec<&str> = primary(&cfg)
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let known: Vec<&str> = PRIMARY_KEYS.iter().map(|(k, _)| *k).collect();
    assert_eq!(
        present, known,
        "primary keys (config order) vs PRIMARY_KEYS in src/lib.rs"
    );
}

#[test]
fn config_keys_match_the_tools_registry() {
    let cfg = load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let present: Vec<&str> = cfg["config"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let known: Vec<&str> = CONFIG_KEYS.iter().map(|(k, _)| *k).collect();
    assert_eq!(present, known, "config keys vs CONFIG_KEYS in src/lib.rs");
}

#[test]
fn recall_shows_every_part_of_the_memory() {
    let o = run(RECALL, &[]);
    assert!(o.status.success());
    let out = stdout(&o);
    for part in [
        "primary.docs",
        "primary.features",
        "primary.log",
        "primary.index",
        "Config (config)",
        "zettelkasten",
        "primary.docs.levers",
    ] {
        assert!(out.contains(part), "recall map lacks {part}:\n{out}");
    }
    for kind in MODULE_KINDS {
        assert!(out.contains(kind), "recall map lacks module kind {kind}");
    }
}

#[test]
fn recall_fails_when_nothing_matches() {
    let o = run(RECALL, &["zz-nothing-matches-this-zz"]);
    assert_eq!(o.status.code(), Some(1));
}

#[test]
fn lint_passes_on_config_and_modules() {
    // The `rust` area runs cargo itself, so it is left to `pla-lint` runs outside tests.
    let o = run(LINT, &["config", "modules"]);
    assert!(o.status.success(), "pla-lint:\n{}", stdout(&o));
}

#[test]
fn audit_finds_no_secrets() {
    let o = run(AUDIT, &["secrets"]);
    assert!(o.status.success(), "pla-audit:\n{}", stdout(&o));
}

#[test]
fn bad_arguments_exit_with_2() {
    for (bin, arg) in [(BUILD, "--nope"), (LINT, "nope"), (AUDIT, "nope")] {
        assert_eq!(run(bin, &[arg]).status.code(), Some(2), "{bin} {arg}");
    }
}
