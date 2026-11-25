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
