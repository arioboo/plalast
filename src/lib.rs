//! Shared helpers for the plalast tools. The brain of the repo is two files at its root:
//! `memory.json` (`primary`, `data`) and `params.yaml` (`config`, `levers`). [`open`] joins them
//! as one value, `{params, primary, data}`, so a dotted path such as
//! `params.config.paths.zettelkasten` works the same everywhere.

pub mod pdf;
pub mod verified;

use serde_json::Value;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The memory: what makes the docs (`primary`) and what gets built (`data`).
pub const MEMORY: &str = "memory.json";
/// The parameters: what the tools need to run (`config`) and the memory levers (`levers`).
pub const PARAMS: &str = "params.yaml";

pub type Res<T> = Result<T, String>;

// ---------- loading ----------

/// Finds the repo root and loads the brain from it.
pub fn open() -> Res<(PathBuf, Value)> {
    let root = find_root()?;
    let cfg = load(&root)?;
    Ok((root, cfg))
}

/// Walks up from the current directory to the folder that holds `memory.json`.
pub fn find_root() -> Res<PathBuf> {
    let mut dir = env::current_dir().map_err(|e| e.to_string())?;
    loop {
        if dir.join(MEMORY).is_file() {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(format!("no {MEMORY} found in this folder or any parent"));
        }
    }
}

/// Reads `memory.json` and `params.yaml`, checks their top-level keys, and joins them.
pub fn load(root: &Path) -> Res<Value> {
    let read = |name: &str| fs::read_to_string(root.join(name)).map_err(|e| format!("{name}: {e}"));
    let memory: Value =
        serde_json::from_str(&read(MEMORY)?).map_err(|e| format!("{MEMORY}: {e}"))?;
    let params: Value =
        serde_norway::from_str(&read(PARAMS)?).map_err(|e| format!("{PARAMS}: {e}"))?;
    check_keys(&memory, MEMORY, &["data", "primary"])?;
    check_keys(&params, PARAMS, &["config", "levers"])?;
    let mut joined = memory;
    joined["params"] = params;
    Ok(joined)
}

/// `file` must be an object whose top-level keys are exactly `expected` (sorted).
pub fn check_keys(v: &Value, file: &str, expected: &[&str]) -> Res<()> {
    let obj = v
        .as_object()
        .ok_or_else(|| format!("{file}: the top level must be a mapping"))?;
    let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    keys.sort_unstable();
    if keys != expected {
        return Err(format!(
            "{file}: top-level keys must be exactly {expected:?}, found {keys:?}"
        ));
    }
    Ok(())
}

// ---------- sections of the brain ----------

/// `params.config`: what the tools need to run the repo (git, settings, paths).
#[must_use]
pub fn config(cfg: &Value) -> &Value {
    &cfg["params"]["config"]
}

/// `params.config.paths`: every path the tools use.
#[must_use]
pub fn paths(cfg: &Value) -> &Value {
    &config(cfg)["paths"]
}

/// `primary`: what makes the docs.
#[must_use]
pub fn primary(cfg: &Value) -> &Value {
    &cfg["primary"]
}

/// `primary.docs`: definition, agent note and `on_demand` settings of the docs.
#[must_use]
pub fn docs(cfg: &Value) -> &Value {
    &primary(cfg)["docs"]
}

/// `data`: what gets built (structure and content).
#[must_use]
pub fn data(cfg: &Value) -> &Value {
    &cfg["data"]
}

/// Memory lever of one document section (`params.levers.<doc>.<id>`), if set.
#[must_use]
pub fn lever<'a>(cfg: &'a Value, doc: &str, id: &str) -> Option<&'a str> {
    cfg["params"]["levers"][doc][id].as_str()
}

/// The value at a dotted path such as `params.config.paths.zettelkasten` (list items by index).
#[must_use]
pub fn lookup<'a>(cfg: &'a Value, dotted: &str) -> Option<&'a Value> {
    cfg.pointer(&format!("/{}", dotted.replace('.', "/")))
}

// ---------- registries: pla-lint fails when the brain and these drift apart ----------

/// Keys of `params.config`, in file order, and the tools that handle each.
pub const CONFIG_KEYS: [(&str, &str); 7] = [
    (
        "PLALAST_DIR",
        "recall (resolved to the repo root), lint (must be $PWD)",
    ),
    ("repository", "recall"),
    ("branches", "audit (sync), recall"),
    ("audit", "audit (allow, max_file_kb), recall, lint"),
    ("paths", "build, lint, recall, audit"),
    ("lint", "lint (always-on line budget)"),
    (
        "hooks",
        "build (.claude/settings.json, --guard), lint, audit",
    ),
];

/// Keys of `primary`, in file order, and the tools that handle each.
pub const PRIMARY_KEYS: [(&str, &str); 14] = [
    ("project", "build (title, logo), recall"),
    ("docs", "build (render, on_demand, strays), recall, lint"),
    ("agents-md", "build (AGENTS.md text via refs), recall, lint"),
    ("commands", "build (AGENTS.md list, BUILD.md table), recall"),
    ("features", "recall, lint"),
    (
        "log",
        "build (LOG.md), recall, lint, audit (unlogged commits)",
    ),
    ("index", "recall, lint"),
    ("skills", "build (links), recall, lint"),
    ("agents", "build (links), recall, lint"),
    ("rules", "build (links), recall, lint"),
    ("connectors", "build (links), recall, lint (format)"),
    ("guides", "build (links, pdf), recall, lint"),
    ("zettelkasten", "build (link target), recall, lint"),
    (
        "schema",
        "build (schema blocks, lever values), lint (coverage)",
    ),
];

/// `build` values rendered from the brain itself rather than from `data.content`:
/// `.gitignore` (from structure) and `.claude/settings.json` (from `params.config.hooks.claude`).
pub const SPECIAL_BUILDS: [&str; 2] = ["gitignore", "claude_settings"];

/// Kinds of log entry in `primary.log`.
pub const LOG_KINDS: [&str; 3] = ["commit", "merge", "branch"];

/// Fields of every `primary.log` entry, in display order.
pub const LOG_FIELDS: [&str; 5] = ["date", "branch", "commit", "kind", "descr"];

/// Kinds of hand-written module: `primary.<kind>` holds the definition,
/// `params.config.paths.modules.<kind>` the folder and the provider links.
pub const MODULE_KINDS: [&str; 5] = ["skills", "agents", "rules", "connectors", "guides"];

// ---------- paths ----------

/// Folder of a module kind, without a trailing `/`.
#[must_use]
pub fn module_dir(cfg: &Value, kind: &str) -> Option<String> {
    str_of(&paths(cfg)["modules"][kind], "dir").map(|d| d.trim_end_matches('/').to_string())
}

/// `(module folder, link folder)` for every provider link of every module kind.
pub fn module_links(cfg: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for kind in MODULE_KINDS {
        let Some(dir) = module_dir(cfg, kind) else {
            continue;
        };
        for dest in paths(cfg)["modules"][kind]["links"]
            .as_object()
            .into_iter()
            .flat_map(|m| m.values())
            .filter_map(Value::as_str)
        {
            out.push((dir.clone(), dest.trim_end_matches('/').to_string()));
        }
    }
    out
}

/// Folders whose `.md` files are docs (`params.config.paths.docs`); `.` is the root.
#[must_use]
pub fn doc_dirs(cfg: &Value) -> Vec<String> {
    strings(paths(cfg), "docs")
        .iter()
        .map(|d| d.trim_end_matches('/').to_string())
        .collect()
}

/// Whether the repo-relative `path` is a doc: an `.md` directly inside one of `doc_dirs`.
#[must_use]
pub fn is_doc_path(cfg: &Value, path: &str) -> bool {
    if !path.ends_with(".md") {
        return false;
    }
    let parent = Path::new(path)
        .parent()
        .map_or_default(|p| p.to_string_lossy().into_owned());
    doc_dirs(cfg)
        .iter()
        .any(|d| (d == "." && parent.is_empty()) || *d == parent)
}

/// Path of a `data.structure` entry: its `path`, or the string its `path_ref` points at.
/// Folders always end with `/`.
#[must_use]
pub fn entry_path(cfg: &Value, entry: &Value) -> Option<String> {
    let path = match (str_of(entry, "path"), str_of(entry, "path_ref")) {
        (Some(p), _) => p.to_string(),
        (None, Some(r)) => lookup(cfg, r)?.as_str()?.to_string(),
        (None, None) => return None,
    };
    let is_dir = str_of(entry, "type") == Some("dir");
    Some(if is_dir && !path.ends_with('/') {
        format!("{path}/")
    } else {
        path
    })
}

/// Visible entries of a folder (hidden ones such as `.gitkeep` are skipped), sorted;
/// empty when the folder does not exist.
pub fn visible_entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|d| d.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    names
}

/// `p` relative to `root`, for messages.
#[must_use]
pub fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string()
}

// ---------- content blocks ----------

/// Text of a content block: its `text`, or the string its `ref` points at.
pub fn block_text(cfg: &Value, block: &Value) -> Option<String> {
    match (str_of(block, "text"), str_of(block, "ref")) {
        (Some(t), _) => Some(t.to_string()),
        (None, Some(r)) => lookup(cfg, r).and_then(Value::as_str).map(String::from),
        (None, None) => None,
    }
}

/// Items of a list block: its `items`, or what its `ref` points at. A list of objects is
/// rendered through the block's `item` template (such as `{cmd}: {summary}`).
#[must_use]
pub fn block_items(cfg: &Value, block: &Value) -> Option<Vec<String>> {
    let list = match (block.get("items"), str_of(block, "ref")) {
        (Some(items), _) => items,
        (None, Some(r)) => lookup(cfg, r)?,
        (None, None) => return None,
    };
    list.as_array()?
        .iter()
        .map(|v| match (v.as_str(), str_of(block, "item")) {
            (Some(s), _) => Some(s.to_string()),
            (None, Some(t)) if v.is_object() => Some(fill(t, v)),
            _ => None,
        })
        .collect()
}

/// Replaces every `{key}` in `template` with the string field `key` of `obj`.
#[must_use]
pub fn fill(template: &str, obj: &Value) -> String {
    let mut out = template.to_string();
    for (k, v) in obj.as_object().into_iter().flatten() {
        if let Some(s) = v.as_str() {
            out = out.replace(&format!("{{{k}}}"), s);
        }
    }
    out
}

// ---------- small accessors ----------

pub fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

pub fn arr<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
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

// ---------- running tools and reporting ----------

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

    /// Prints every finding and a summary. Returns true when there are no errors.
    #[must_use]
    pub fn finish(&self, tool: &str) -> bool {
        for (level, area, msg) in &self.items {
            let tag = match level {
                Level::Error => "error",
                Level::Warn => "warn ",
            };
            println!("{tag} [{area}] {msg}");
        }
        let errors = self
            .items
            .iter()
            .filter(|(l, _, _)| *l == Level::Error)
            .count();
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
#[must_use]
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn top_level_keys_must_match_exactly() {
        assert!(check_keys(&json!({"data": 1, "primary": 2}), "m", &["data", "primary"]).is_ok());
        assert!(check_keys(&json!({"primary": 2}), "m", &["data", "primary"]).is_err());
        assert!(check_keys(
            &json!({"data": 1, "primary": 2, "x": 3}),
            "m",
            &["data", "primary"]
        )
        .is_err());
        assert!(check_keys(&json!([1]), "m", &["data", "primary"]).is_err());
    }

    #[test]
    fn params_yaml_parses_into_json_values() {
        let v: Value = serde_norway::from_str(
            "config:\n  paths:\n    docs: [\".\", agents]\n  audit: {allow: [], max_file_kb: 1024}\nlevers:\n  doc: {a: on_demand}\n",
        )
        .unwrap();
        assert_eq!(v["config"]["paths"]["docs"], json!([".", "agents"]));
        assert_eq!(v["config"]["audit"]["max_file_kb"], json!(1024));
        assert_eq!(v["levers"]["doc"]["a"], json!("on_demand"));
    }

    #[test]
    fn structure_paths_can_point_into_the_brain() {
        let cfg = json!({"params": {"config": {"paths": {"x": "agents/skills"}}}});
        let e = json!({"path_ref": "params.config.paths.x", "type": "dir"});
        assert_eq!(entry_path(&cfg, &e).as_deref(), Some("agents/skills/"));
        assert_eq!(
            entry_path(&cfg, &json!({"path": "a.md", "type": "file"})).as_deref(),
            Some("a.md")
        );
        assert_eq!(entry_path(&cfg, &json!({"path_ref": "params.nope"})), None);
    }

    #[test]
    fn list_items_can_come_from_objects_through_a_template() {
        let cfg = json!({"primary": {"commands": [{"cmd": "a", "summary": "does a"}, {"cmd": "b", "summary": "does b"}]}});
        let b = json!({"type": "list", "ref": "primary.commands", "item": "{cmd}: {summary}"});
        assert_eq!(
            block_items(&cfg, &b).unwrap(),
            vec!["a: does a", "b: does b"]
        );
        // Objects without a template are not text.
        assert!(block_items(&cfg, &json!({"type": "list", "ref": "primary.commands"})).is_none());
    }
}
