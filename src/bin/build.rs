//! build: renders every generated `.md`, folder, symlink and `.gitignore` from `config.json`.
//!
//! - `pla-build`           applies the changes.
//! - `pla-build --check`   changes nothing; fails if the repo drifted from `config.json`.
//!
//! Each document section is rendered according to its memory lever
//! (`primary.docs.levers.<doc>.<section id>`): `enabled`, `on_demand` or `disabled`.

use plalast::{
    arr, data, doc_dirs, docs, exit, lever, lookup, module_dir, module_links, open, primary, req,
    str_of, strings, Res, CONFIG,
};
use serde_json::Value;
use std::env;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "usage: pla-build [--check]";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    exit((|| {
        let check = match args.first().map(String::as_str) {
            None => false,
            Some("--check") => true,
            Some(other) => return Err(format!("unknown option {other}\n{USAGE}")),
        };
        let (root, cfg) = open()?;
        build(&root, &cfg, check)
    })())
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

/// Table of `primary.schema.<part>`: one row per key, in config order.
fn schema_table(cfg: &Value, block: &Value) -> Res<String> {
    let part = req(block, "part", "schema block")?;
    let map = primary(cfg)["schema"][part]
        .as_object()
        .ok_or_else(|| format!("schema block: primary.schema.{part} is missing"))?;
    let rows: Vec<Vec<String>> = map
        .iter()
        .map(|(k, v)| vec![format!("`{k}`"), v.as_str().unwrap_or("").to_string()])
        .collect();
    Ok(md_table(&strings(block, "header"), &rows))
}

/// An image. Its path is `src`, or the config value named by `ref` (a dotted path such as
/// `primary.project.plalast_logo`), so a path kept in the config is written only once.
/// With `width`, it is an HTML `<img>` (markdown cannot size images).
fn image(cfg: &Value, block: &Value) -> Res<String> {
    let src = match (str_of(block, "src"), str_of(block, "ref")) {
        (Some(src), _) => src,
        (None, Some(r)) => lookup(cfg, r)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("image block: `{r}` is not a string in {CONFIG}"))?,
        (None, None) => return Err("image block: needs `src` or `ref`".to_string()),
    };
    let alt = str_of(block, "alt").unwrap_or("");
    Ok(match block.get("width").and_then(Value::as_u64) {
        Some(w) => format!("<img src=\"{src}\" alt=\"{alt}\" width=\"{w}\">"),
        None => format!("![{alt}]({src})"),
    })
}

/// Markdown for a list of blocks, separated by one blank line, without a trailing newline.
fn render_blocks(blocks: &[Value], cfg: &Value) -> Res<String> {
    let mut parts = Vec::new();
    for b in blocks {
        let kind = req(b, "type", "content block")?;
        let part = match kind {
            "heading" => {
                let level = b.get("level").and_then(Value::as_u64).unwrap_or(1);
                // Verified: always 1..=6 `#` (src/verified.rs).
                let hashes = plalast::verified::heading_hashes(level) as usize;
                format!("{} {}", "#".repeat(hashes), req(b, "text", "heading")?)
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
            "structure_table" => structure_table(arr(data(cfg), "structure")),
            "schema" => schema_table(cfg, b)?,
            "image" => image(cfg, b)?,
            "agent_note" => agent_note(cfg)?.to_string(),
            other => return Err(format!("unknown content block type `{other}`")),
        };
        // One blank line between blocks; the agent note sits right under the first line.
        let sep = if kind == "agent_note" { "\n" } else { "\n\n" };
        parts.push((sep, part));
    }
    let mut out = String::new();
    for (i, (sep, part)) in parts.iter().enumerate() {
        if i > 0 {
            out.push_str(sep);
        }
        out.push_str(part);
    }
    Ok(out.trim_end().to_string())
}

/// The note every generated doc carries on its second line (`primary.docs.agent_note`).
fn agent_note(cfg: &Value) -> Res<&str> {
    docs(cfg)["agent_note"]
        .as_str()
        .ok_or_else(|| "primary.docs.agent_note is missing".to_string())
}

/// Renders document `key`: its main text plus one `(path, text)` per `on_demand` section.
fn render_doc(key: &str, doc: &Value, cfg: &Value) -> Res<(String, Vec<(String, String)>)> {
    let sections = arr(doc, "sections");
    let allowed: Vec<&str> = primary(cfg)["schema"]["levers"]
        .as_object()
        .map(|m| m.keys().map(String::as_str).collect())
        .unwrap_or_default();
    if let Some(levers) = docs(cfg)["levers"][key].as_object() {
        for id in levers.keys() {
            if !sections.iter().any(|s| str_of(s, "id") == Some(id)) {
                return Err(format!(
                    "primary.docs.levers.{key}.{id}: no such section in data.content.{key}"
                ));
            }
        }
    }

    let mut parts = Vec::new();
    let head = render_blocks(arr(doc, "head"), cfg)?;
    if !head.is_empty() {
        parts.push(head);
    }
    let on_demand = &docs(cfg)["on_demand"];
    let mut pointers = Vec::new();
    let mut files = Vec::new();
    for s in sections {
        let id = req(s, "id", &format!("data.content.{key} section"))?;
        let title = req(s, "title", id)?;
        let state = lever(cfg, key, id).ok_or_else(|| {
            format!("data.content.{key}: section `{id}` has no lever in primary.docs.levers.{key}")
        })?;
        if !allowed.contains(&state) {
            return Err(format!(
                "primary.docs.levers.{key}.{id}: `{state}` is not one of {allowed:?}"
            ));
        }
        let body = render_blocks(arr(s, "blocks"), cfg)?;
        let with_body = |heading: String| {
            if body.is_empty() {
                heading
            } else {
                format!("{heading}\n\n{body}")
            }
        };
        match state {
            "enabled" => parts.push(with_body(format!("## {title}"))),
            "on_demand" => {
                let dir = req(on_demand, "dir", "primary.docs.on_demand")?.trim_end_matches('/');
                let path = format!("{dir}/{id}.md");
                let when = req(s, "when", id)?;
                pointers.push(
                    req(on_demand, "pointer", "primary.docs.on_demand")?
                        .replace("{path}", &path)
                        .replace("{title}", title)
                        .replace("{when}", when),
                );
                let heading = format!("# {title}\n{}", agent_note(cfg)?);
                files.push((path, format!("{}\n", with_body(heading))));
            }
            "disabled" => {}
            other => return Err(format!("lever `{other}` has no build rule")),
        }
    }
    if !pointers.is_empty() {
        let heading = req(on_demand, "heading", "primary.docs.on_demand")?;
        let list: Vec<String> = pointers.iter().map(|p| format!("- {p}")).collect();
        parts.push(format!("## {heading}\n\n{}", list.join("\n")));
    }
    // A document with nothing in it is an empty file, not a lone newline.
    let main = if parts.is_empty() {
        String::new()
    } else {
        format!("{}\n", parts.join("\n\n"))
    };
    Ok((main, files))
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
    let structure = arr(data(cfg), "structure");
    let mut out = Vec::new();
    let mut files: Vec<String> = Vec::new();
    let mut add_file = |out: &mut Vec<Want>, path: &str, text: String| -> Res<()> {
        if files.iter().any(|f| f == path) {
            return Err(format!(
                "{path} is generated twice; rename a section id or a path"
            ));
        }
        files.push(path.to_string());
        out.push(Want::File(root.join(path), text));
        Ok(())
    };
    for e in structure {
        let path = req(e, "path", "data.structure entry")?;
        let full = root.join(path.trim_end_matches('/'));
        match req(e, "type", path)? {
            "dir" => {
                out.push(Want::Dir(full.clone()));
                if e.get("gitkeep").and_then(Value::as_bool) == Some(true) {
                    out.push(Want::File(full.join(".gitkeep"), String::new()));
                }
            }
            "symlink" => {
                // `target`, or `target_ref`: a dotted config path holding it (one place for it).
                let target = match (str_of(e, "target"), str_of(e, "target_ref")) {
                    (Some(t), _) => t,
                    (None, Some(r)) => lookup(cfg, r).and_then(Value::as_str).ok_or_else(|| {
                        format!("{path}: target_ref `{r}` is not a string in {CONFIG}")
                    })?,
                    (None, None) => {
                        return Err(format!("{path}: symlink needs `target` or `target_ref`"))
                    }
                };
                out.push(Want::Link(full, target.to_string()))
            }
            "file" => match str_of(e, "build") {
                None => {}
                Some("gitignore") => add_file(&mut out, path, render_gitignore(structure))?,
                Some(key) => {
                    let doc = &data(cfg)["content"][key];
                    if !doc.is_object() {
                        return Err(format!("{path}: data.content.{key} is missing"));
                    }
                    let (main, extra) = render_doc(key, doc, cfg)?;
                    add_file(&mut out, path, main)?;
                    for (p, text) in extra {
                        add_file(&mut out, &p, text)?;
                    }
                }
            },
            "planned" | "ignored" => {}
            other => return Err(format!("{path}: unknown type `{other}`")),
        }
    }
    // Guides: with `render: "pdf"`, every hand-written .md in primary.guides.dir gets a .pdf
    // next to it (src/pdf.rs; deterministic, so --check compares it like any other file).
    if str_of(&primary(cfg)["guides"], "render") == Some("pdf") {
        if let Some(dir) = module_dir(cfg, "guides") {
            for name in modules(&root.join(&dir))? {
                let Some(stem) = name.strip_suffix(".md") else {
                    continue;
                };
                let md = fs::read_to_string(root.join(&dir).join(&name))
                    .map_err(|e| format!("{dir}/{name}: {e}"))?;
                add_file(
                    &mut out,
                    &format!("{dir}/{stem}.pdf"),
                    plalast::pdf::render(&md),
                )?;
            }
        }
    }
    // Module links (primary.<kind>.links): one symlink per module, never a whole folder.
    for (source, dest) in module_links(cfg) {
        let up = "../".repeat(Path::new(&dest).components().count());
        for name in modules(&root.join(&source))? {
            out.push(Want::Link(
                root.join(&dest).join(&name),
                format!("{up}{source}/{name}"),
            ));
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
    let wants = wanted(root, cfg)?;
    let generated: Vec<String> = wants
        .iter()
        .filter_map(|w| match w {
            Want::File(p, _) => Some(rel(root, p)),
            _ => None,
        })
        .collect();
    let mut drift = Vec::new();
    for w in wants {
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
    drift.extend(prune_module_links(root, cfg, check)?);
    let strays = stray_markdown(root, cfg, &generated);

    let verb = if check { "out of sync" } else { "updated" };
    for d in &drift {
        println!("{verb}: {d}");
    }
    for s in &strays {
        println!("not in {CONFIG}: {s} (it is a doc, an .md in primary.docs.dirs, so it must be generated; add it to {CONFIG} or delete it)");
    }
    if drift.is_empty() && strays.is_empty() {
        println!("in sync with {CONFIG}");
    }
    Ok(strays.is_empty() && (!check || drift.is_empty()))
}

/// Removes broken module links (left behind by deleted modules).
fn prune_module_links(root: &Path, cfg: &Value, check: bool) -> Res<Vec<String>> {
    let mut out = Vec::new();
    for (_source, dest) in module_links(cfg) {
        let dest = root.join(dest);
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
    Ok(out)
}

/// Docs (`.md` files in `primary.docs.dirs`) that this build does not generate.
fn stray_markdown(root: &Path, cfg: &Value, generated: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for dir in doc_dirs(cfg) {
        let Ok(read) = fs::read_dir(root.join(&dir)) else {
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
    out
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;
    use plalast::check_top_level;
    use serde_json::json;

    /// A config with one document `doc` holding sections `a` and `b` with the given levers.
    fn cfg(levers: Value) -> Value {
        json!({
            "primary": {
                "docs": {
                    "agent_note": "NOTE",
                    "levers": {"doc": levers},
                    "on_demand": {"dir": "agents", "heading": "On demand", "pointer": "`{path}`: {title}. Read it when {when}."}
                },
                "schema": {"levers": {"enabled": "", "on_demand": "", "disabled": ""}}
            },
            "data": {"structure": [], "content": {}}
        })
    }

    fn doc() -> Value {
        json!({
            "head": [{"type": "heading", "level": 1, "text": "Doc"}],
            "sections": [
                {"id": "a", "title": "A", "when": "doing a", "blocks": [{"type": "paragraph", "text": "alpha"}]},
                {"id": "b", "title": "B", "when": "doing b", "blocks": [{"type": "list", "items": ["x", "y"]}]}
            ]
        })
    }

    #[test]
    fn renders_blocks_with_one_blank_line_between() {
        let blocks = json!([
            {"type": "heading", "level": 2, "text": "Title"},
            {"type": "list", "items": ["a", "b"]},
            {"type": "ordered_list", "items": ["x", "y"]}
        ]);
        let out = render_blocks(blocks.as_array().unwrap(), &cfg(json!({}))).unwrap();
        assert_eq!(out, "## Title\n\n- a\n- b\n\n1. x\n2. y");
    }

    #[test]
    fn enabled_sections_render_inline() {
        let (main, files) =
            render_doc("doc", &doc(), &cfg(json!({"a": "enabled", "b": "enabled"}))).unwrap();
        assert_eq!(main, "# Doc\n\n## A\n\nalpha\n\n## B\n\n- x\n- y\n");
        assert!(files.is_empty());
    }

    #[test]
    fn on_demand_section_gets_its_own_file_and_a_pointer() {
        let (main, files) = render_doc(
            "doc",
            &doc(),
            &cfg(json!({"a": "on_demand", "b": "enabled"})),
        )
        .unwrap();
        assert_eq!(
            main,
            "# Doc\n\n## B\n\n- x\n- y\n\n## On demand\n\n- `agents/a.md`: A. Read it when doing a.\n"
        );
        assert_eq!(
            files,
            vec![(
                "agents/a.md".to_string(),
                "# A\nNOTE\n\nalpha\n".to_string()
            )]
        );
    }

    #[test]
    fn disabled_section_is_not_rendered() {
        let (main, files) = render_doc(
            "doc",
            &doc(),
            &cfg(json!({"a": "disabled", "b": "enabled"})),
        )
        .unwrap();
        assert_eq!(main, "# Doc\n\n## B\n\n- x\n- y\n");
        assert!(files.is_empty());
    }

    #[test]
    fn agent_note_sits_on_the_second_line() {
        let d = json!({
            "head": [{"type": "heading", "level": 1, "text": "Doc"}, {"type": "agent_note"},
                     {"type": "paragraph", "text": "intro"}],
            "sections": []
        });
        let (main, _) = render_doc("doc", &d, &cfg(json!({}))).unwrap();
        assert_eq!(main, "# Doc\nNOTE\n\nintro\n");
    }

    #[test]
    fn agent_note_without_its_text_is_an_error() {
        let c = json!({"primary": {"docs": {}}, "data": {}});
        let b = json!([{"type": "agent_note"}]);
        assert!(render_blocks(b.as_array().unwrap(), &c).is_err());
    }

    #[test]
    fn empty_document_is_an_empty_file() {
        let empty = json!({"head": [], "sections": []});
        assert_eq!(render_doc("doc", &empty, &cfg(json!({}))).unwrap().0, "");
    }

    #[test]
    fn missing_lever_is_an_error() {
        let err = render_doc("doc", &doc(), &cfg(json!({"a": "enabled"}))).unwrap_err();
        assert!(err.contains("`b` has no lever"), "{err}");
    }

    #[test]
    fn invalid_lever_value_is_an_error() {
        let err =
            render_doc("doc", &doc(), &cfg(json!({"a": "maybe", "b": "enabled"}))).unwrap_err();
        assert!(err.contains("`maybe` is not one of"), "{err}");
    }

    #[test]
    fn lever_for_unknown_section_is_an_error() {
        let err = render_doc(
            "doc",
            &doc(),
            &cfg(json!({"a": "enabled", "b": "enabled", "c": "enabled"})),
        )
        .unwrap_err();
        assert!(err.contains("primary.docs.levers.doc.c"), "{err}");
    }

    #[test]
    fn top_level_must_be_primary_and_data() {
        assert!(check_top_level(&json!({"primary": {}, "data": {}})).is_ok());
        assert!(check_top_level(&json!({"primary": {}, "data": {}, "extra": 1})).is_err());
        assert!(check_top_level(&json!({"data": {}})).is_err());
    }

    #[test]
    fn schema_block_renders_a_table_in_config_order() {
        let c = json!({"primary": {"schema": {"levers": {"enabled": "on", "disabled": "off"}}}, "data": {}});
        let b = json!([{"type": "schema", "part": "levers", "header": ["Value", "Effect"]}]);
        assert_eq!(
            render_blocks(b.as_array().unwrap(), &c).unwrap(),
            "| Value | Effect |\n|---|---|\n| `enabled` | on |\n| `disabled` | off |"
        );
    }

    #[test]
    fn structure_table_lists_rows_and_planned_paths() {
        let s = json!([
            {"path": "a/", "type": "dir", "label": "a/<x>.md", "table": "A"},
            {"path": "b", "type": "file"},
            {"path": "src/", "type": "planned", "table": "Code"}
        ]);
        assert_eq!(
            structure_table(s.as_array().unwrap()),
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
    fn image_block_resolves_its_path_from_the_config() {
        let c = json!({"primary": {"project": {"logo": "src/logo.png"}}, "data": {}});
        let b = json!([
            {"type": "image", "ref": "primary.project.logo", "alt": "Logo", "width": 200},
            {"type": "image", "src": "a.png", "alt": "A"}
        ]);
        assert_eq!(
            render_blocks(b.as_array().unwrap(), &c).unwrap(),
            "<img src=\"src/logo.png\" alt=\"Logo\" width=\"200\">\n\n![A](a.png)"
        );
        let bad = json!([{"type": "image", "ref": "primary.project.nope"}]);
        assert!(render_blocks(bad.as_array().unwrap(), &c).is_err());
    }

    #[test]
    fn doc_paths_come_from_primary_docs_dirs() {
        let c = json!({"primary": {"docs": {"dirs": [".", "agents/"]}}, "data": {}});
        assert!(plalast::is_doc_path(&c, "README.md"));
        assert!(plalast::is_doc_path(&c, "agents/BUILD.md"));
        assert!(!plalast::is_doc_path(&c, "agents/skills/x/SKILL.md"));
        assert!(!plalast::is_doc_path(&c, "src/notes.md"));
        assert!(!plalast::is_doc_path(&c, "agents/config.json"));
    }

    #[test]
    fn unknown_block_type_is_an_error() {
        let blocks = json!([{"type": "nope"}]);
        assert!(render_blocks(blocks.as_array().unwrap(), &cfg(json!({}))).is_err());
    }
}
