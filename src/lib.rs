//! Shared helpers for the plalast tools: locate and read `config.json`, the brain of the repo.

use serde_json::Value;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub const CONFIG: &str = "config.json";

pub type Res<T> = Result<T, String>;

/// Finds the repo root and loads `config.json` from it.
pub fn open() -> Res<(PathBuf, Value)> {
    let root = find_root()?;
    let cfg = load(&root)?;
    check_top_level(&cfg)?;
    Ok((root, cfg))
}

/// `config.json` has exactly two top-level keys: `primary` (control) and `data` (what is built).
pub fn check_top_level(cfg: &Value) -> Res<()> {
    let obj = cfg
        .as_object()
        .ok_or_else(|| format!("{CONFIG}: the top level must be an object"))?;
    let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    keys.sort_unstable();
    if keys != ["data", "primary"] {
        return Err(format!(
            "{CONFIG}: top-level keys must be exactly `primary` and `data`, found {keys:?}"
        ));
    }
    Ok(())
}

/// Control panel: project, levers, on_demand, adapters, schema.
pub fn primary(cfg: &Value) -> &Value {
    &cfg["primary"]
}

/// What gets built: structure and content.
pub fn data(cfg: &Value) -> &Value {
    &cfg["data"]
}

/// Memory lever of one document section (`primary.levers.<doc>.<id>`), if set.
pub fn lever<'a>(cfg: &'a Value, doc: &str, id: &str) -> Option<&'a str> {
    primary(cfg)["levers"][doc][id].as_str()
}

/// Exit code shared by every tool: 0 in sync / found, 1 not, 2 error.
pub fn exit(result: Res<bool>) -> ExitCode {
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

/// Walks up from the current directory to the folder that holds `config.json`.
pub fn find_root() -> Res<PathBuf> {
    let mut dir = env::current_dir().map_err(|e| e.to_string())?;
    loop {
        if dir.join(CONFIG).is_file() {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(format!("no {CONFIG} found in this folder or any parent"));
        }
    }
}

pub fn load(root: &Path) -> Res<Value> {
    let text = fs::read_to_string(root.join(CONFIG)).map_err(|e| format!("{CONFIG}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("{CONFIG}: {e}"))
}

pub fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

pub fn arr<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

pub fn strings(v: &Value, key: &str) -> Vec<String> {
    arr(v, key)
        .iter()
        .filter_map(Value::as_str)
        .map(String::from)
        .collect()
}

pub fn req<'a>(v: &'a Value, key: &str, ctx: &str) -> Res<&'a str> {
    str_of(v, key).ok_or_else(|| format!("{ctx}: missing string field `{key}`"))
}

// ---------- checks shared by pla-lint and pla-audit ----------

/// How serious a finding is: errors fail the run, warnings are only reported.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Error,
    Warn,
}

/// Findings of one run, grouped by area (`config`, `secrets`, ...).
#[derive(Default)]
pub struct Findings {
    pub items: Vec<(Level, String, String)>,
}

impl Findings {
    pub fn error(&mut self, area: &str, msg: impl Into<String>) {
        self.items
            .push((Level::Error, area.to_string(), msg.into()));
    }

    pub fn warn(&mut self, area: &str, msg: impl Into<String>) {
        self.items.push((Level::Warn, area.to_string(), msg.into()));
    }

    pub fn errors(&self) -> usize {
        self.items
            .iter()
            .filter(|(l, _, _)| *l == Level::Error)
            .count()
    }

    /// Prints every finding and a summary. Returns true when there are no errors.
    pub fn finish(&self, tool: &str) -> bool {
        for (level, area, msg) in &self.items {
            let tag = match level {
                Level::Error => "error",
                Level::Warn => "warn ",
            };
            println!("{tag} [{area}] {msg}");
        }
        let errors = self.errors();
        let warns = self.items.len() - errors;
        println!("{tool}: {errors} error(s), {warns} warning(s)");
        errors == 0
    }
}

/// Runs `program args` in `root`. Returns (success, stdout + stderr), or Err if it cannot start.
pub fn run(root: &Path, program: &str, args: &[&str]) -> Res<(bool, String)> {
    let out = std::process::Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| format!("cannot run {program}: {e}"))?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok((out.status.success(), text))
}

/// The cargo to call: the one running us under `cargo run` (`$CARGO`), else `cargo` on PATH.
pub fn cargo() -> String {
    env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

/// Files git would commit: tracked plus untracked-but-not-ignored, relative to `root`.
pub fn repo_files(root: &Path) -> Res<Vec<String>> {
    let (ok, out) = run(
        root,
        "git",
        &["ls-files", "--cached", "--others", "--exclude-standard"],
    )?;
    if !ok {
        return Err(format!("git ls-files failed: {out}"));
    }
    let mut files: Vec<String> = out.lines().map(String::from).collect();
    files.sort();
    files.dedup();
    Ok(files)
}
