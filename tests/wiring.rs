//! End-to-end wiring: runs the four compiled tools against this repo's real brain (memory.json, params.yaml).
//! If a memory key, a generated file or a tool drifts apart, one of these fails.

use plalast::{entry_path, load, primary, CONFIG_KEYS, MODULE_KINDS, PRIMARY_KEYS};
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

/// The brain premise: every generated file is rebuilt from memory.json + params.yaml byte by byte.
/// Copies the repo (what git would commit) to a temp folder, deletes everything pla-build
/// generates (every .md in the doc folders, `build` entries and guide PDFs), rebuilds there and
/// compares each file with the original.
#[test]
fn generated_files_rebuild_from_the_brain() {
    use std::fs;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ls = Command::new("git")
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .current_dir(root)
        .output()
        .expect("git runs");
    let files: Vec<String> = String::from_utf8_lossy(&ls.stdout)
        .lines()
        .filter(|f| root.join(f).symlink_metadata().is_ok())
        .map(String::from)
        .collect();
    let cfg = load(root).unwrap();
    let doc_dirs: Vec<&str> = cfg["config"]["paths"]["docs"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d.as_str())
        .collect();
    let built: Vec<String> = cfg["data"]["structure"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["build"].is_string())
        .filter_map(|e| entry_path(&cfg, e))
        .collect();
    let guides = cfg["config"]["paths"]["modules"]["guides"]["dir"]
        .as_str()
        .unwrap();
    let is_generated = |f: &str| {
        let p = Path::new(f);
        let dir = p.parent().and_then(Path::to_str).unwrap_or("");
        let dir = if dir.is_empty() { "." } else { dir };
        (f.ends_with(".md") && doc_dirs.contains(&dir) && !p.is_symlink())
            || built.iter().any(|b| b == f)
            || (dir == guides && f.ends_with(".pdf"))
    };

    let tmp = std::env::temp_dir().join(format!("plalast-rebuild-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    let mut generated = Vec::new();
    for f in &files {
        let (src, dst) = (root.join(f), tmp.join(f));
        fs::create_dir_all(dst.parent().unwrap()).unwrap();
        if is_generated(f) {
            generated.push(f.clone());
        } else if src.is_symlink() {
            std::os::unix::fs::symlink(fs::read_link(&src).unwrap(), &dst).unwrap();
        } else {
            fs::copy(&src, &dst).unwrap();
        }
    }
    assert!(
        generated.len() >= 10,
        "only {generated:?} counted as generated"
    );
    let o = Command::new(BUILD).current_dir(&tmp).output().unwrap();
    assert!(o.status.success(), "pla-build in the copy:\n{}", stdout(&o));
    for f in &generated {
        let rebuilt = fs::read(tmp.join(f)).unwrap_or_default();
        assert!(
            rebuilt == fs::read(root.join(f)).unwrap(),
            "{f} is not rebuilt byte by byte from the brain"
        );
    }
    fs::remove_dir_all(&tmp).unwrap();
}

#[test]
fn every_generated_doc_has_the_agent_note_on_line_two() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cfg = load(root).unwrap();
    let note = cfg["primary"]["docs"]["agent_note"].as_str().unwrap();
    let mut checked = 0;
    for e in cfg["data"]["structure"].as_array().unwrap() {
        let (Some(path), Some(key)) = (entry_path(&cfg, e), e["build"].as_str()) else {
            continue;
        };
        // Only markdown docs carry the note (.gitignore and .claude/settings.json do not),
        // and a doc can opt out with `"agent_note": false` (CLAUDE.md is only `@AGENTS.md`).
        let opted_out = cfg["data"]["content"][key]["agent_note"].as_bool() == Some(false);
        if !path.ends_with(".md") || opted_out {
            continue;
        }
        let text = std::fs::read_to_string(root.join(&path)).unwrap();
        assert_eq!(text.lines().nth(1), Some(note), "{path} line 2");
        checked += 1;
    }
    assert!(checked >= 6, "only {checked} docs checked");
}

#[test]
fn the_brain_is_memory_json_and_params_yaml() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(root.join(plalast::MEMORY).is_file() && root.join(plalast::PARAMS).is_file());
    assert!(
        !root.join("config.json").exists(),
        "config.json was replaced by memory.json + params.yaml"
    );
    let cfg = load(root).unwrap();
    let mut top: Vec<&str> = cfg
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    top.sort_unstable();
    assert_eq!(top, ["config", "data", "params", "primary"]);
}

#[test]
fn agents_md_shows_the_three_primary_keys_and_params_holds_only_levers() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let agents = std::fs::read_to_string(root.join("agents/AGENTS.md")).unwrap();
    assert!(agents.contains("| Key | Is (descr_+) | Is not (descr_-) |"));
    for key in plalast::BRAIN_KEYS {
        assert!(
            agents.contains(&format!("| `{key}` |")),
            "AGENTS.md lacks {key}"
        );
    }
    let memory: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join(plalast::MEMORY)).unwrap())
            .unwrap();
    let top: Vec<&str> = memory
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        top,
        plalast::BRAIN_KEYS,
        "memory.json primary keys, in order"
    );
    let cfg = load(root).unwrap();
    let params: Vec<&str> = cfg["params"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(params, ["levers"]);
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
        "(primary.modules)",
        "Modes (primary.mode",
        "params.levers",
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
    let o = run(LINT, &["memory", "modules"]);
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
