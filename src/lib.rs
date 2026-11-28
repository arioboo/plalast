//! Shared helpers for the plalast tools. The brain of the repo is `memory.json` at its root, with
//! exactly three primary keys (`config`, `primary`, `data`); `params.yaml` holds the
//! interruptores (`levers`) and the agent placeholder (`agent_placeholder`). [`open`] joins them as one value, `{config, primary, data, params}`,
//! so a dotted path such as `config.paths.modules.agent_zettelkasten` or
//! `params.levers.agents_md.preferencias` works the same everywhere.

pub mod calc;
pub mod fantastic;
pub mod pdf;
pub mod verified;

use serde_json::Value;
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

/// The memory, the brain: what the tools need to run (`config`), what makes the docs (`primary`)
/// and what gets built (`data`).
pub const MEMORY: &str = "memory.json";
/// The parameters: the interruptores (`levers`) and the note on line 2 of every doc (`agent_placeholder`).
pub const PARAMS: &str = "params.yaml";

/// The primary keys of the memory, in file order; each one has a `descr_+` and a `descr_-`.
pub const BRAIN_KEYS: [&str; 3] = ["config", "primary", "data"];

/// What a primary key is (`descr_+`, the positive way) and is not (`descr_-`, the negative way).
pub const DESCR_KEYS: [&str; 2] = ["descr_+", "descr_-"];

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

/// The values an interruptor can take; a param with one of them is an interruptor.
pub const LEVER_VALUES: [&str; 3] = ["enabled", "on_demand", "disabled"];

/// The memory levels of `config.mem_level`, from the least to the most memory in context.
pub const MEM_LEVELS: [&str; 3] = ["low", "mid", "hard"];

/// The param that chooses the memory level.
pub const MEM_LEVEL: &str = "config.mem_level";

/// A param written as `{low: …, mid: …, hard: …}`: its value at `level` (None if it is not one).
fn by_level<'a>(v: &'a Value, level: &str) -> Option<&'a Value> {
    let m = v.as_object()?;
    if m.is_empty() || !m.keys().all(|k| MEM_LEVELS.contains(&k.as_str())) {
        return None;
    }
    Some(m.get(level).unwrap_or(&Value::Null))
}

/// The line of `params.yaml` after which only interruptors are written.
pub const INTERRUPTORS_MARK: &str = "#---INTERRUPTORS---";

/// Reads `memory.json` and `params.yaml` and joins them. Every key of `params.yaml` is the path of
/// a field of the memory (`config.…`, `primary.…`, `data.…`): a value `enabled`, `on_demand` or
/// `disabled` is the interruptor of that field (kept in `params.interruptors`, by path); any other
/// value sets the field, which must then not be written in `memory.json` too.
pub fn load(root: &Path) -> Res<Value> {
    let read = |name: &str| fs::read_to_string(root.join(name)).map_err(|e| format!("{name}: {e}"));
    let memory: Value =
        serde_json::from_str(&read(MEMORY)?).map_err(|e| format!("{MEMORY}: {e}"))?;
    let params: Value =
        serde_norway::from_str(&read(PARAMS)?).map_err(|e| format!("{PARAMS}: {e}"))?;
    check_keys(&memory, MEMORY, &["config", "data", "primary"])?;
    let params = params
        .as_object()
        .ok_or_else(|| format!("{PARAMS}: the top level must be a mapping"))?;
    let mut joined = memory;
    let mut interruptors = serde_json::Map::new();
    // `config.mem_level` (low, mid or hard) picks the value of every param written by level.
    let level = params
        .get(MEM_LEVEL)
        .and_then(Value::as_str)
        .unwrap_or("mid")
        .to_string();
    if !MEM_LEVELS.contains(&level.as_str()) {
        return Err(format!(
            "{PARAMS}: `{MEM_LEVEL}` must be one of {MEM_LEVELS:?}"
        ));
    }
    let mut levelled = serde_json::Map::new();
    for (path, raw) in params {
        let v = match by_level(raw, &level) {
            Some(Value::Null) => continue, // not set at this level: the default applies
            Some(v) => {
                levelled.insert(path.clone(), raw.clone());
                v
            }
            None => raw,
        };
        if !BRAIN_KEYS
            .iter()
            .any(|b| path.starts_with(&format!("{b}.")))
        {
            return Err(format!(
                "{PARAMS}: `{path}` is not the path of a field of {MEMORY} (config.…, primary.…, data.…)"
            ));
        }
        if v.as_str().is_some_and(|s| LEVER_VALUES.contains(&s)) {
            interruptors.insert(path.clone(), v.clone());
        } else {
            set_path(&mut joined, path, v.clone())?;
        }
    }
    joined["params"] = serde_json::json!({ "interruptors": interruptors, "by_level": levelled });
    // The derived view `structure`: each entry of `config.structure` (path, type, flags) with its
    // prose from `primary.structure.<path>` (description, table, label). Not a field of the memory.
    let mut view = Vec::new();
    for e in arr(&joined["config"], "structure") {
        let mut merged = e.clone();
        if let Some(path) = entry_path(&joined, e) {
            for (k, v) in joined["primary"]["structure"][path.as_str()]
                .as_object()
                .into_iter()
                .flatten()
            {
                merged[k.as_str()] = v.clone();
            }
        }
        view.push(merged);
    }
    joined["structure"] = Value::Array(view);
    Ok(joined)
}

/// Every path of the repo, in order: `config.structure` joined with its prose from
/// `primary.structure` (the derived view built by [`load`]).
#[must_use]
pub fn structure(cfg: &Value) -> &[Value] {
    arr(cfg, "structure")
}

/// The shape of every generated doc, by key: `primary.docs.content`.
#[must_use]
pub fn content(cfg: &Value) -> &Value {
    &cfg["primary"]["docs"]["content"]
}

/// Writes `v` at the dotted `path` of `root`, creating the objects on the way. A field that
/// already has a value is an error: a param never repeats what the memory holds.
fn set_path(root: &mut Value, path: &str, v: Value) -> Res<()> {
    let mut cur = root;
    let parts: Vec<&str> = path.split('.').collect();
    for (i, part) in parts.iter().enumerate() {
        let obj = cur.as_object_mut().ok_or_else(|| {
            format!("{PARAMS}: `{path}` goes through a value that is not an object")
        })?;
        if i == parts.len() - 1 {
            if obj.get(*part).is_some_and(|x| !x.is_null()) {
                return Err(format!(
                    "{PARAMS}: `{path}` is set in both {PARAMS} and {MEMORY}; keep it in one"
                ));
            }
            obj.insert((*part).to_string(), v);
            return Ok(());
        }
        cur = obj
            .entry((*part).to_string())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
    }
    Ok(())
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

/// `config`: what the tools need to run the repo (git, settings, paths).
#[must_use]
pub fn config(cfg: &Value) -> &Value {
    &cfg["config"]
}

/// `config.paths`: every path the tools use.
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

/// Value of an interruptor that is not set in `params.yaml`.
pub const DEFAULT_LEVER: &str = "on_demand";

/// Group name for the modes in [`lever_set`] (`primary.mode.<name>`), not a document.
pub const MODE_GROUP: &str = "mode";

/// The interruptor set in `params.yaml` for the field at `path`, if any.
#[must_use]
pub fn interruptor<'a>(cfg: &'a Value, path: &str) -> Option<&'a str> {
    cfg["params"]["interruptors"][path].as_str()
}

/// Path of the interruptor of one document section (`primary.docs.content.<doc>.sections.<id>`) or,
/// with `group` = [`MODE_GROUP`], of one mode (`primary.mode.<name>`).
#[must_use]
pub fn lever_path(group: &str, id: &str) -> String {
    if group == MODE_GROUP {
        format!("primary.mode.{id}")
    } else {
        format!("primary.docs.content.{group}.sections.{id}")
    }
}

/// Interruptor (memory lever) of one document section or one mode, only if it is set.
#[must_use]
pub fn lever_set<'a>(cfg: &'a Value, group: &str, id: &str) -> Option<&'a str> {
    interruptor(cfg, &lever_path(group, id))
}

/// Interruptor of a whole generated doc (`primary.docs.content.<key>`): its value, or the default.
#[must_use]
pub fn doc_lever<'a>(cfg: &'a Value, key: &str) -> &'a str {
    interruptor(cfg, &format!("primary.docs.content.{key}")).unwrap_or(DEFAULT_LEVER)
}

/// Interruptor of one document section or mode: its value, or [`DEFAULT_LEVER`] when unset.
#[must_use]
pub fn lever<'a>(cfg: &'a Value, group: &str, id: &str) -> &'a str {
    lever_set(cfg, group, id).unwrap_or(DEFAULT_LEVER)
}

/// Modes of `primary.mode`, in file order, with their interruptor value.
#[must_use]
pub fn modes(cfg: &Value) -> Vec<(&str, &Value, &str)> {
    primary(cfg)["mode"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(name, m)| (name.as_str(), m, lever(cfg, MODE_GROUP, name)))
        .collect()
}

/// Procedures of `primary.procedures.items`, in file order: id and procedure (`title`, `text`).
#[must_use]
pub fn procedures(cfg: &Value) -> Vec<(&str, &Value)> {
    primary(cfg)["procedures"]["items"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(id, p)| (id.as_str(), p))
        .collect()
}

/// Problems of `primary.problems.items`, in file order: id and problem (`title`, `statement`,
/// `solved_by`).
#[must_use]
pub fn problems(cfg: &Value) -> Vec<(&str, &Value)> {
    primary(cfg)["problems"]["items"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(id, p)| (id.as_str(), p))
        .collect()
}

/// Path of the file an `on_demand` mode is rendered to: `<config.paths.on_demand>/mode-<name>.md`.
#[must_use]
pub fn mode_path(cfg: &Value, name: &str) -> Option<String> {
    let dir = str_of(paths(cfg), "on_demand")?.trim_end_matches('/');
    Some(format!("{dir}/mode-{name}.md"))
}

/// The value at a dotted path such as `config.paths.modules.agent_zettelkasten` (list items by index).
#[must_use]
pub fn lookup<'a>(cfg: &'a Value, dotted: &str) -> Option<&'a Value> {
    cfg.pointer(&format!("/{}", dotted.replace('.', "/")))
}

// ---------- registries: pla-lint fails when the brain and these drift apart ----------

/// Keys of `config`, in file order, and the tools that handle each.
pub const CONFIG_KEYS: [(&str, &str); 14] = [
    ("descr_+", "build (brain_keys), recall, lint"),
    ("descr_-", "build (brain_keys), recall, lint"),
    (
        "PLALAST_DIR",
        "recall (resolved to the repo root), lint (must be $PWD)",
    ),
    ("repository", "recall"),
    ("audit", "audit (allow, max_file_kb), recall, lint"),
    ("paths", "build, lint, recall, audit"),
    ("lint", "lint (always-on line budget)"),
    (
        "structure",
        "build (every path), lint (pairing with primary.structure), recall, audit",
    ),
    (
        "mem_level",
        "load (params by level), recall (--session), lint",
    ),
    (
        "master_arch",
        "recall (--session: alone or orchestrating the subagents)",
    ),
    ("branches", "audit (sync), recall"),
    (
        "hooks",
        "build (Claude Code settings, --guard), lint, audit",
    ),
    (
        "obligatory",
        "build (section titles), lint (covers the categories)",
    ),
    (
        "preferential",
        "build (section titles), lint (covers the categories)",
    ),
];

/// Keys of `primary`, in file order, and the tools that handle each.
pub const PRIMARY_KEYS: [(&str, &str); 21] = [
    ("descr_+", "build (brain_keys), recall, lint"),
    ("descr_-", "build (brain_keys), recall, lint"),
    ("project", "build (title, logo), recall"),
    ("docs", "build (render, on_demand, strays), recall, lint"),
    ("agents-md", "build (AGENTS.md text via refs), recall, lint"),
    ("commands", "build (AGENTS.md list, BUILD.md table), recall"),
    ("features", "recall, lint"),
    ("log", "build (LOG.md), recall, lint (levels, status)"),
    (
        "changes",
        "build (CHANGES.md), recall, lint (notation), audit (unrecorded commits)",
    ),
    ("index", "recall, lint"),
    ("skills", "build (links), recall, lint"),
    ("agents", "build (links), recall, lint"),
    ("rules", "build (links), recall, lint"),
    ("connectors", "build (links), recall, lint (format)"),
    ("guides", "build (links, pdf), recall, lint"),
    ("modules", "build (link target), recall, lint"),
    ("mode", "build (modes block, on_demand files), recall, lint"),
    ("procedures", "build (procedures block), recall, lint"),
    (
        "problems",
        "build (problems block), recall, lint (names, coverage)",
    ),
    (
        "schema",
        "build (schema blocks, lever values), lint (coverage)",
    ),
    (
        "structure",
        "build (structure table, via the derived view), recall, lint (pairing)",
    ),
];

/// `build` values rendered from the brain itself rather than from `primary.docs.content`:
/// `.gitignore` (from structure) and the Claude Code settings (from `config.hooks.claude`).
pub const SPECIAL_BUILDS: [&str; 2] = ["gitignore", "claude_settings"];

/// Types of the `type(scope)` notation of a `primary.changes` entry.
pub const CHANGE_TYPES: [&str; 8] = [
    "feat", "fix", "docs", "refactor", "test", "chore", "merge", "branch",
];

/// Fields of every `primary.changes` entry, in display order.
pub const CHANGE_FIELDS: [&str; 5] = ["date", "branch", "commit", "change", "descr"];

/// Levels of a `primary.log` entry, from the mildest to the one that needs work.
pub const LOG_LEVELS: [&str; 3] = ["warning", "error", "issue"];

/// Status of a `primary.log` entry.
pub const LOG_STATUS: [&str; 2] = ["open", "closed"];

/// Fields of every `primary.log` entry, in display order.
pub const LOG_FIELDS: [&str; 5] = ["date", "level", "where", "descr", "status"];

/// Kinds of hand-written module: `primary.<kind>` holds the definition,
/// `config.paths.modules.<kind>` the folder and the provider links.
pub const MODULE_KINDS: [&str; 5] = ["skills", "agents", "rules", "connectors", "guides"];

/// External modules (`primary.modules.<name>`), in file order: name, path
/// (`config.paths.modules.<name>`, relative to the folder of its link) and definition.
#[must_use]
pub fn external_modules(cfg: &Value) -> Vec<(&str, Option<&str>, Option<&str>)> {
    primary(cfg)["modules"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(name, m)| {
            (
                name.as_str(),
                str_of(&paths(cfg)["modules"], name),
                str_of(m, "definition"),
            )
        })
        .collect()
}

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

/// Folders whose `.md` files are docs (`config.paths.docs`); `.` is the root.
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

/// Path of a `config.structure` entry: its `path`, or the string its `path_ref` points at.
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

/// Resolves `target` (a symlink's text) against the link's folder without touching disk.
#[must_use]
pub fn lexical_target(link: &Path, target: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in link
        .parent()
        .unwrap_or(Path::new(""))
        .join(target)
        .components()
    {
        match c {
            // `..` cancels the last real folder; with none left (or only `..`), it stays.
            Component::ParentDir => {
                let last_is_real =
                    matches!(out.components().next_back(), Some(Component::Normal(_)));
                if last_is_real {
                    out.pop();
                } else {
                    out.push("..");
                }
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
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
    fn params_by_level_take_the_value_of_the_current_level() {
        let v = json!({"low": "disabled", "hard": "enabled"});
        assert_eq!(by_level(&v, "hard"), Some(&json!("enabled")));
        assert_eq!(by_level(&v, "mid"), Some(&Value::Null));
        assert_eq!(by_level(&json!({"PreToolUse": []}), "mid"), None);
        assert_eq!(by_level(&json!("enabled"), "mid"), None);
    }

    #[test]
    fn params_set_fields_by_path_and_never_twice() {
        let mut m = json!({"config": {"audit": {"max_file_kb": 1}}});
        set_path(&mut m, "config.audit.allow", json!([])).unwrap();
        set_path(&mut m, "config.hooks.dir", json!("h")).unwrap();
        assert_eq!(
            m,
            json!({"config": {"audit": {"max_file_kb": 1, "allow": []}, "hooks": {"dir": "h"}}})
        );
        assert!(set_path(&mut m, "config.audit.max_file_kb", json!(2))
            .unwrap_err()
            .contains("set in both"));
        assert!(set_path(&mut m, "config.hooks.dir.x", json!(1)).is_err());
    }

    #[test]
    fn interruptors_are_found_by_the_path_of_their_field() {
        let cfg = json!({"params": {"interruptors": {
            "primary.docs.content.agents_md.sections.a": "enabled",
            "primary.mode.careful": "disabled",
            "primary.docs.content.log_md": "disabled"
        }}});
        assert_eq!(lever_set(&cfg, "agents_md", "a"), Some("enabled"));
        assert_eq!(lever(&cfg, MODE_GROUP, "careful"), "disabled");
        assert_eq!(lever(&cfg, "agents_md", "b"), DEFAULT_LEVER);
        assert_eq!(doc_lever(&cfg, "log_md"), "disabled");
        assert_eq!(doc_lever(&cfg, "index_md"), DEFAULT_LEVER);
    }

    #[test]
    fn structure_paths_can_point_into_the_brain() {
        let cfg = json!({"config": {"paths": {"x": "agents/skills"}}});
        let e = json!({"path_ref": "config.paths.x", "type": "dir"});
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
