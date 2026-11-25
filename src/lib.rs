//! Shared helpers for the plalast tools: locate and read `config.json`, the brain of the repo.

pub mod pdf;
pub mod verified;

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

/// Control panel: project, docs, adapters, audit, schema.
pub fn primary(cfg: &Value) -> &Value {
    &cfg["primary"]
}

/// Everything about the docs (`primary.docs`): definition, dirs, levers, on_demand.
pub fn docs(cfg: &Value) -> &Value {
    &primary(cfg)["docs"]
}

/// Every key of `primary`, in config order, and the tools that handle it. `pla-lint` fails on a
/// key missing from config.json or not listed here, so memory and binaries stay wired together.
pub const PRIMARY_KEYS: [(&str, &str); 13] = [
    ("project", "build (logo), recall (title), audit (branches)"),
    (
        "docs",
        "build (render, levers, on_demand, strays), recall, lint",
    ),
    ("features", "recall, lint"),
    ("log", "recall, lint"),
    ("index", "recall, lint"),
    ("skills", "build (links), recall, lint"),
    ("agents", "build (links), recall, lint"),
    ("rules", "build (links), recall, lint"),
    ("connectors", "build (links), recall, lint (format)"),
    ("guides", "build (links, pdf), recall, lint"),
    (
        "zettelkasten",
        "build (link target), recall, lint, audit (outside link)",
    ),
    ("config", "audit (config.audit), recall, lint"),
    (
        "schema",
        "build (schema blocks, lever values), lint (coverage)",
    ),
];

/// Kinds of hand-written module; each is `primary.<kind>` with `definition`, `dir` and `links`.
pub const MODULE_KINDS: [&str; 5] = ["skills", "agents", "rules", "connectors", "guides"];

/// Folder of a module kind (`primary.<kind>.dir`), without a trailing `/`.
pub fn module_dir(cfg: &Value, kind: &str) -> Option<String> {
    str_of(&primary(cfg)[kind], "dir").map(|d| d.trim_end_matches('/').to_string())
}

/// `(module folder, link folder)` for every provider link of every module kind.
pub fn module_links(cfg: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for kind in MODULE_KINDS {
        let Some(dir) = module_dir(cfg, kind) else {
            continue;
        };
        for (_provider, dest) in primary(cfg)[kind]["links"]
            .as_object()
            .into_iter()
            .flatten()
        {
            if let Some(d) = dest.as_str() {
                out.push((dir.clone(), d.trim_end_matches('/').to_string()));
            }
        }
    }
    out
}

/// The value at a dotted config path such as `primary.zettelkasten.path`.
pub fn lookup<'a>(cfg: &'a Value, dotted: &str) -> Option<&'a Value> {
    cfg.pointer(&format!("/{}", dotted.replace('.', "/")))
}

/// Folders whose `.md` files are docs (`primary.docs.dirs`), as relative paths; `.` is the root.
pub fn doc_dirs(cfg: &Value) -> Vec<String> {
    strings(docs(cfg), "dirs")
        .iter()
        .map(|d| d.trim_end_matches('/').to_string())
        .collect()
}

/// Whether the repo-relative `path` is a doc: an `.md` directly inside one of `doc_dirs`.
pub fn is_doc_path(cfg: &Value, path: &str) -> bool {
    if !path.ends_with(".md") {
        return false;
    }
    let parent = Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    doc_dirs(cfg)
        .iter()
        .any(|d| (d == "." && parent.is_empty()) || *d == parent)
}

/// What gets built: structure and content.
pub fn data(cfg: &Value) -> &Value {
    &cfg["data"]
}

/// Memory lever of one document section (`primary.docs.levers.<doc>.<id>`), if set.
pub fn lever<'a>(cfg: &'a Value, doc: &str, id: &str) -> Option<&'a str> {
    docs(cfg)["levers"][doc][id].as_str()
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
