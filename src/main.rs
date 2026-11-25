//! plalast: build and recall the repo from `config.json`, its single source of truth.
//!
//! - `build`           renders every generated `.md`, folder, symlink and `.gitignore`.
//! - `build --check`   changes nothing; fails if the repo drifted from `config.json`.
//! - `recall [term]`   prints the repo map, or searches it for `term`.

use serde_json::Value;
use std::env;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const CONFIG: &str = "config.json";
const USAGE: &str = "usage: plalast build [--check] | plalast recall [term]";

type Res<T> = Result<T, String>;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let run = || -> Res<bool> {
        let root = find_root()?;
        let cfg = load(&root)?;
        match args.first().map(String::as_str) {
            Some("build") => match args.get(1).map(String::as_str) {
                None => build(&root, &cfg, false),
                Some("--check") => build(&root, &cfg, true),
                Some(other) => Err(format!("unknown option {other}\n{USAGE}")),
            },
            Some("recall") => Ok(recall(&cfg, &args[1..].join(" "))),
            _ => Err(USAGE.to_string()),
        }
    };
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

// ---------- config ----------

/// Walks up from the current directory to the folder that holds `config.json`.
fn find_root() -> Res<PathBuf> {
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

fn load(root: &Path) -> Res<Value> {
    let text = fs::read_to_string(root.join(CONFIG)).map_err(|e| format!("{CONFIG}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("{CONFIG}: {e}"))
}

fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn arr<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn strings(v: &Value, key: &str) -> Vec<String> {
    arr(v, key)
        .iter()
        .filter_map(Value::as_str)
        .map(String::from)
        .collect()
}

fn req<'a>(v: &'a Value, key: &str, ctx: &str) -> Res<&'a str> {
    str_of(v, key).ok_or_else(|| format!("{ctx}: missing string field `{key}`"))
}

// ---------- render ----------

fn md_table(header: &[String], rows: &[Vec<String>]) -> String {
    let mut out = format!(
        "| {} |\n|{}",
        header.join(" | "),
        "---|".repeat(header.len())
    );
    for r in rows {
        out.push_str(&format!("\n| {} |", r.join(" | ")));
    }
    out
}

fn structure_table(structure: &[Value]) -> String {
    let planned: Vec<String> = structure
        .iter()
        .filter(|e| str_of(e, "type") == Some("planned"))
        .filter_map(|e| str_of(e, "path"))
        .map(|p| format!("`{p}`"))
        .collect();
    let intro = match planned.len() {
        0 => "Paths are added here as they are created:".to_string(),
        1 => format!(
            "Paths are added here as they are created ({} is planned, not created yet):",
            planned[0]
        ),
        _ => format!(
            "Paths are added here as they are created ({} are planned, not created yet):",
            planned.join(", ")
        ),
    };
    let rows: Vec<Vec<String>> = structure
        .iter()
        .filter_map(|e| {
            let table = str_of(e, "table")?;
            let label = str_of(e, "label").or_else(|| str_of(e, "path"))?;
            Some(vec![format!("`{label}`"), table.to_string()])
        })
        .collect();
    format!(
        "{intro}\n\n{}",
        md_table(&["Path".into(), "Content".into()], &rows)
    )
}

fn render_blocks(blocks: &[Value], structure: &[Value]) -> Res<String> {
    let mut parts = Vec::new();
    for b in blocks {
        let kind = req(b, "type", "content block")?;
        let part = match kind {
            "heading" => {
                let level = b.get("level").and_then(Value::as_u64).unwrap_or(1) as usize;
                format!("{} {}", "#".repeat(level), req(b, "text", "heading")?)
            }
            "paragraph" | "raw" => req(b, "text", kind)?.to_string(),
            "list" => strings(b, "items")
                .iter()
                .map(|i| format!("- {i}"))
                .collect::<Vec<_>>()
                .join("\n"),
            "ordered_list" => strings(b, "items")
                .iter()
                .enumerate()
                .map(|(n, i)| format!("{}. {i}", n + 1))
                .collect::<Vec<_>>()
                .join("\n"),
            "code" => format!(
                "```{}\n{}\n```",
                str_of(b, "lang").unwrap_or(""),
                req(b, "text", "code")?
            ),
            "table" => {
                let rows: Vec<Vec<String>> = arr(b, "rows")
                    .iter()
                    .map(|r| {
                        r.as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(String::from)
                            .collect()
                    })
                    .collect();
                md_table(&strings(b, "header"), &rows)
            }
            "structure_table" => structure_table(structure),
            other => return Err(format!("unknown content block type `{other}`")),
        };
        parts.push(part);
    }
    Ok(format!("{}\n", parts.join("\n\n").trim_end()))
}

fn render_gitignore(structure: &[Value]) -> String {
    let mut out = String::new();
    for e in structure {
        let Some(path) = str_of(e, "path") else {
            continue;
        };
        if e.get("secret").and_then(Value::as_bool) == Some(true) {
            out.push_str(&format!("{path}*\n!{path}.gitkeep\n"));
        } else if str_of(e, "type") == Some("ignored") {
            out.push_str(&format!("{path}\n"));
        }
    }
    out
}

// ---------- build ----------

enum Want {
    Dir(PathBuf),
    File(PathBuf, String),
    Link(PathBuf, String),
}

/// Everything `config.json` says should exist on disk, in order.
fn wanted(root: &Path, cfg: &Value) -> Res<Vec<Want>> {
    let structure = arr(cfg, "structure");
    let content = cfg.get("content").cloned().unwrap_or(Value::Null);
    let mut out = Vec::new();
    for e in structure {
        let path = req(e, "path", "structure entry")?;
        let full = root.join(path.trim_end_matches('/'));
        match req(e, "type", path)? {
            "dir" => {
                out.push(Want::Dir(full.clone()));
                if e.get("gitkeep").and_then(Value::as_bool) == Some(true) {
                    out.push(Want::File(full.join(".gitkeep"), String::new()));
                }
            }
            "symlink" => out.push(Want::Link(full, req(e, "target", path)?.to_string())),
            "file" => match str_of(e, "build") {
                None => {}
                Some("gitignore") => out.push(Want::File(full, render_gitignore(structure))),
                Some(key) => {
                    let blocks = content
                        .get(key)
                        .and_then(Value::as_array)
                        .ok_or_else(|| format!("{path}: content.{key} is missing or not a list"))?;
                    out.push(Want::File(full, render_blocks(blocks, structure)?));
                }
            },
            "planned" | "ignored" => {}
            other => return Err(format!("{path}: unknown type `{other}`")),
        }
    }
    // Adapters: one symlink per module, never a whole folder.
    for (_provider, maps) in cfg
        .get("adapters")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        for m in maps.as_array().into_iter().flatten() {
            let source = req(m, "source", "adapter")?.trim_end_matches('/');
            let dest = req(m, "dest", "adapter")?.trim_end_matches('/');
            let up = "../".repeat(Path::new(dest).components().count());
            for name in modules(&root.join(source))? {
                out.push(Want::Link(
                    root.join(dest).join(&name),
                    format!("{up}{source}/{name}"),
                ));
            }
        }
    }
    Ok(out)
}

/// Module names in a folder: every entry except hidden ones such as `.gitkeep`.
fn modules(dir: &Path) -> Res<Vec<String>> {
    let Ok(read) = fs::read_dir(dir) else {
        return Ok(Vec::new());
    };
    let mut names: Vec<String> = read
        .filter_map(Result::ok)
        .map(|d| d.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    Ok(names)
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string()
}

/// Applies (or, with `check`, only reports) every difference between disk and config.
/// Returns false when the repo is out of sync.
fn build(root: &Path, cfg: &Value, check: bool) -> Res<bool> {
    let mut drift = Vec::new();
    for w in wanted(root, cfg)? {
        match w {
            Want::Dir(p) => {
                if p.is_dir() {
                    continue;
                }
                if p.symlink_metadata().is_ok() {
                    return Err(format!(
                        "{} exists and is not a folder; not touching it",
                        rel(root, &p)
                    ));
                }
                drift.push(format!("missing folder {}", rel(root, &p)));
                if !check {
                    fs::create_dir_all(&p).map_err(|e| format!("{}: {e}", rel(root, &p)))?;
                }
            }
            Want::File(p, text) => {
                if fs::read_to_string(&p).ok().as_deref() == Some(text.as_str()) {
                    continue;
                }
                if p.is_symlink() || p.is_dir() {
                    return Err(format!(
                        "{} is not a regular file; not touching it",
                        rel(root, &p)
                    ));
                }
                drift.push(format!("stale file {}", rel(root, &p)));
                if !check {
                    if let Some(parent) = p.parent() {
                        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    fs::write(&p, text).map_err(|e| format!("{}: {e}", rel(root, &p)))?;
                }
            }
            Want::Link(p, target) => {
                let current = fs::read_link(&p).ok();
                if current.as_deref() == Some(Path::new(&target)) {
                    continue;
                }
                if current.is_none() && p.symlink_metadata().is_ok() {
                    return Err(format!(
                        "{} exists and is not a symlink; not touching it",
                        rel(root, &p)
                    ));
                }
                drift.push(format!("link {} -> {target}", rel(root, &p)));
                if !check {
                    if current.is_some() {
                        fs::remove_file(&p).map_err(|e| e.to_string())?;
                    }
                    symlink(&target, &p).map_err(|e| format!("{}: {e}", rel(root, &p)))?;
                }
            }
        }
    }
    drift.extend(prune_adapter_links(root, cfg, check)?);
    let strays = stray_markdown(root, cfg)?;

    let verb = if check { "out of sync" } else { "updated" };
    for d in &drift {
        println!("{verb}: {d}");
    }
    for s in &strays {
        println!("not in {CONFIG}: {s} (every .md in the root and agents/ must be generated; add it to {CONFIG} or delete it)");
    }
    if drift.is_empty() && strays.is_empty() {
        println!("in sync with {CONFIG}");
    }
    Ok(strays.is_empty() && (!check || drift.is_empty()))
}

/// Removes broken adapter links (left behind by deleted modules).
fn prune_adapter_links(root: &Path, cfg: &Value, check: bool) -> Res<Vec<String>> {
    let mut out = Vec::new();
    for (_provider, maps) in cfg
        .get("adapters")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        for m in maps.as_array().into_iter().flatten() {
            let dest = root.join(req(m, "dest", "adapter")?.trim_end_matches('/'));
            for name in modules(&dest)? {
                let p = dest.join(&name);
                if p.is_symlink() && !p.exists() {
                    out.push(format!("remove broken link {}", rel(root, &p)));
                    if !check {
                        fs::remove_file(&p).map_err(|e| e.to_string())?;
                    }
                }
            }
        }
    }
    Ok(out)
}

/// `.md` files in the root and in `agents/` that no structure entry generates.
fn stray_markdown(root: &Path, cfg: &Value) -> Res<Vec<String>> {
    let generated: Vec<String> = arr(cfg, "structure")
        .iter()
        .filter(|e| str_of(e, "type") == Some("file") && str_of(e, "build").is_some())
        .filter_map(|e| str_of(e, "path").map(String::from))
        .collect();
    let mut out = Vec::new();
    for dir in ["", "agents"] {
        let Ok(read) = fs::read_dir(root.join(dir)) else {
            continue;
        };
        for entry in read.filter_map(Result::ok) {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) != Some("md") || p.is_dir() {
                continue;
            }
            let r = rel(root, &p);
            if !generated.contains(&r) {
                out.push(r);
            }
        }
    }
    out.sort();
    Ok(out)
}

// ---------- recall ----------

/// Prints the repo map, or every structure entry and document line matching `query`.
/// Returns false when a query matches nothing.
fn recall(cfg: &Value, query: &str) -> bool {
    let structure = arr(cfg, "structure");
    let q = query.trim().to_lowercase();
    let hit = |s: &str| q.is_empty() || s.to_lowercase().contains(&q);
    let mut found = false;

    if q.is_empty() {
        if let Some(t) = cfg.pointer("/project/title").and_then(Value::as_str) {
            println!("{t}\n");
        }
    }
    for e in structure {
        let fields = ["path", "label", "description", "table"].map(|k| str_of(e, k).unwrap_or(""));
        if fields.iter().any(|f| hit(f)) {
            found = true;
            let ty = str_of(e, "type").unwrap_or("?");
            let ty = match (ty, str_of(e, "build")) {
                ("file", Some(_)) => "generated",
                ("file", None) => "manual",
                _ => ty,
            };
            println!("{:<30} {:<10} {}", fields[0], ty, fields[2]);
        }
    }
    if q.is_empty() {
        return true;
    }
    // Document text: report the file and the section each match sits in.
    for e in structure {
        let (Some(path), Some(key)) = (str_of(e, "path"), str_of(e, "build")) else {
            continue;
        };
        let Some(blocks) = cfg
            .pointer(&format!("/content/{key}"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        let mut section = String::new();
        for b in blocks {
            let mut texts: Vec<String> = str_of(b, "text").map(String::from).into_iter().collect();
            texts.extend(strings(b, "items"));
            texts.extend(
                arr(b, "rows")
                    .iter()
                    .flat_map(|r| r.as_array().into_iter().flatten())
                    .filter_map(Value::as_str)
                    .map(String::from),
            );
            if str_of(b, "type") == Some("heading") {
                section = texts.first().cloned().unwrap_or_default();
            }
            for t in texts.iter().filter(|t| hit(t)) {
                found = true;
                println!("{path} › {section}: {t}");
            }
        }
    }
    if !found {
        println!("nothing in {CONFIG} matches `{query}`");
    }
    found
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_blocks_with_one_blank_line_between() {
        let blocks = json!([
            {"type": "heading", "level": 2, "text": "Title"},
            {"type": "list", "items": ["a", "b"]},
            {"type": "ordered_list", "items": ["x", "y"]}
        ]);
        let out = render_blocks(blocks.as_array().unwrap(), &[]).unwrap();
        assert_eq!(out, "## Title\n\n- a\n- b\n\n1. x\n2. y\n");
    }

    #[test]
    fn structure_table_lists_rows_and_planned_paths() {
        let s = json!([
            {"path": "a/", "type": "dir", "label": "a/<x>.md", "table": "A"},
            {"path": "b", "type": "file"},
            {"path": "src/", "type": "planned", "table": "Code"}
        ]);
        let out = structure_table(s.as_array().unwrap());
        assert_eq!(
            out,
            "Paths are added here as they are created (`src/` is planned, not created yet):\n\n\
             | Path | Content |\n|---|---|\n| `a/<x>.md` | A |\n| `src/` | Code |"
        );
    }

    #[test]
    fn gitignore_covers_secrets_and_ignored_paths() {
        let s = json!([
            {"path": "agents/.secrets/", "type": "dir", "secret": true},
            {"path": "target/", "type": "ignored"}
        ]);
        assert_eq!(
            render_gitignore(s.as_array().unwrap()),
            "agents/.secrets/*\n!agents/.secrets/.gitkeep\ntarget/\n"
        );
    }

    #[test]
    fn unknown_block_type_is_an_error() {
        let blocks = json!([{"type": "nope"}]);
        assert!(render_blocks(blocks.as_array().unwrap(), &[]).is_err());
    }
}
