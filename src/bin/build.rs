//! build: renders every generated doc, folder, symlink, `.gitignore` and `.claude/settings.json`
//! from the brain (`memory.json` + `params.yaml`).
//!
//! - `pla-build`           applies the changes.
//! - `pla-build --check`   changes nothing; fails if the repo drifted from the brain.
//! - `pla-build --guard`   Claude Code `PreToolUse` hook: reads the payload on stdin and denies
//!   Edit/Write on any file this build generates (see agents/hooks/protect-generated.sh).
//!
//! Each document section is rendered according to its interruptor (memory lever,
//! `params.levers.<doc>.<section id>`): `enabled`, `on_demand` (the default) or `disabled`.
//! Each mode of `primary.mode` has one too (`params.levers.mode.<name>`): `enabled` puts its rules
//! in the `modes` block, `on_demand` writes them to their own file, `disabled` drops them.

use plalast::{
    arr, block_items, block_text, config, data, doc_dirs, docs, entry_path, exit, fill, lever,
    lookup, mode_path, modes, module_dir, module_links, open, paths, primary, rel, req, str_of,
    strings, visible_entries, Res, BRAIN_KEYS, MEMORY, SPECIAL_BUILDS,
};
use serde_json::{json, Value};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::io::Read;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "usage: pla-build [--check | --guard]";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    exit((|| {
        let (root, cfg) = open()?;
        match args.first().map(String::as_str) {
            None => build(&root, &cfg, false),
            Some("--check") => build(&root, &cfg, true),
            Some("--guard") => guard(&root, &cfg),
            Some(other) => Err(format!("unknown option {other}\n{USAGE}")),
        }
    })())
}

// ---------- blocks ----------

fn md_table(header: &[String], rows: &[Vec<String>]) -> String {
    let mut out = format!(
        "| {} |\n|{}",
        header.join(" | "),
        "---|".repeat(header.len())
    );
    for r in rows {
        let _ = write!(out, "\n| {} |", r.join(" | "));
    }
    out
}

/// The Structure table: one row per `data.structure` entry with a `table` text.
fn structure_table(cfg: &Value) -> String {
    let structure = arr(data(cfg), "structure");
    let planned: Vec<String> = structure
        .iter()
        .filter(|e| str_of(e, "type") == Some("planned"))
        .filter_map(|e| entry_path(cfg, e))
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
            let path = entry_path(cfg, e)?;
            let label = str_of(e, "label").map_or(path.clone(), |l| l.replace("{path}", &path));
            Some(vec![format!("`{label}`"), table.to_string()])
        })
        .collect();
    format!(
        "{intro}\n\n{}",
        md_table(&["Path".into(), "Content".into()], &rows)
    )
}

/// A `table` block: literal `rows`, or `ref` to a list of objects rendered with `row`
/// (one template per column), optionally `reverse`d.
fn table(cfg: &Value, b: &Value) -> Res<String> {
    let rows: Vec<Vec<String>> = match str_of(b, "ref") {
        None => arr(b, "rows")
            .iter()
            .map(|r| {
                r.as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect()
            })
            .collect(),
        Some(r) => {
            let list = lookup(cfg, r)
                .and_then(Value::as_array)
                .ok_or_else(|| format!("table block: ref `{r}` is not a list"))?;
            let templates = strings(b, "row");
            let mut rows: Vec<Vec<String>> = list
                .iter()
                .map(|obj| templates.iter().map(|t| fill(t, obj)).collect())
                .collect();
            if b.get("reverse").and_then(Value::as_bool) == Some(true) {
                rows.reverse();
            }
            rows
        }
    };
    Ok(md_table(&strings(b, "header"), &rows))
}

/// Table of `primary.schema.<part>`: one row per key, in file order.
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

/// An image: `src`, or the path its `ref` points at. With `width`, an HTML `<img>`
/// (markdown cannot size images).
fn image(cfg: &Value, block: &Value) -> Res<String> {
    let src = match (str_of(block, "src"), str_of(block, "ref")) {
        (Some(src), _) => src,
        (None, Some(r)) => lookup(cfg, r)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("image block: `{r}` is not a string in the brain"))?,
        (None, None) => return Err("image block: needs `src` or `ref`".to_string()),
    };
    let alt = str_of(block, "alt").unwrap_or("");
    Ok(match block.get("width").and_then(Value::as_u64) {
        Some(w) => format!("<img src=\"{src}\" alt=\"{alt}\" width=\"{w}\">"),
        None => format!("![{alt}]({src})"),
    })
}

/// A block's text (`text` or `ref`), or an error naming what is missing.
fn text(cfg: &Value, b: &Value, kind: &str) -> Res<String> {
    block_text(cfg, b).ok_or_else(|| match str_of(b, "ref") {
        Some(r) => format!("{kind} block: ref `{r}` is not a string in the brain"),
        None => format!("{kind} block: needs `text` or `ref`"),
    })
}

/// A list block's items (`items` or `ref`, objects through `item`), or an error.
fn items(cfg: &Value, b: &Value, kind: &str) -> Res<Vec<String>> {
    block_items(cfg, b).ok_or_else(|| match str_of(b, "ref") {
        Some(r) => {
            format!("{kind} block: ref `{r}` is not a list of strings (or objects with `item`)")
        }
        None => format!("{kind} block: needs `items` or `ref`"),
    })
}

/// The note every generated doc carries on its second line (`primary.docs.agent_note`).
fn agent_note(cfg: &Value) -> Res<&str> {
    docs(cfg)["agent_note"]
        .as_str()
        .ok_or_else(|| "primary.docs.agent_note is missing".to_string())
}

/// The `brain_keys` block: one row per primary key of the memory with what it is (`descr_+`)
/// and what it is not (`descr_-`).
fn brain_keys(cfg: &Value) -> Res<String> {
    let header = ["Key", "Is (descr_+)", "Is not (descr_-)"].map(String::from);
    let mut rows = Vec::new();
    for key in BRAIN_KEYS {
        let is = req(&cfg[key], "descr_+", key)?;
        let is_not = req(&cfg[key], "descr_-", key)?;
        rows.push(vec![format!("`{key}`"), is.to_string(), is_not.to_string()]);
    }
    Ok(md_table(&header, &rows))
}

/// Rules of a mode (`primary.mode.<name>.rules`), or an error naming the mode.
fn mode_rules<'a>(name: &str, m: &'a Value) -> Res<Vec<&'a str>> {
    let rules: Vec<&str> = arr(m, "rules").iter().filter_map(Value::as_str).collect();
    if rules.is_empty() {
        return Err(format!(
            "primary.mode.{name}: needs a non-empty `rules` list"
        ));
    }
    Ok(rules)
}

/// The `modes` block: one line per mode that is not `disabled`: its rules when `enabled`,
/// a pointer to its own file when `on_demand`.
fn modes_block(cfg: &Value) -> Res<String> {
    let mut lines = Vec::new();
    for (name, m, state) in modes(cfg) {
        match state {
            "enabled" => lines.push(format!(
                "- Mode `{name}` (enabled): {}",
                mode_rules(name, m)?.join(" ")
            )),
            "on_demand" => {
                let path = mode_path(cfg, name)
                    .ok_or_else(|| "config.paths.on_demand is missing".to_string())?;
                let when = req(m, "when", &format!("primary.mode.{name}"))?;
                lines.push(format!(
                    "- Mode `{name}` (on_demand): read `{path}` when {when}."
                ));
            }
            "disabled" => {}
            other => {
                return Err(format!(
                    "params.levers.mode.{name}: `{other}` is not enabled, on_demand or disabled"
                ))
            }
        }
    }
    Ok(lines.join("\n"))
}

/// `(path, text)` of the file of every `on_demand` mode: its title, the agent note and its rules.
fn mode_files(cfg: &Value) -> Res<Vec<(String, String)>> {
    let mut out = Vec::new();
    for (name, m, state) in modes(cfg) {
        if state != "on_demand" {
            continue;
        }
        let path =
            mode_path(cfg, name).ok_or_else(|| "config.paths.on_demand is missing".to_string())?;
        let title = req(m, "title", &format!("primary.mode.{name}"))?;
        let rules: Vec<String> = mode_rules(name, m)?
            .iter()
            .map(|r| format!("- {r}"))
            .collect();
        out.push((
            path,
            format!("# {title}\n{}\n\n{}\n", agent_note(cfg)?, rules.join("\n")),
        ));
    }
    Ok(out)
}

/// Markdown for a list of blocks, separated by one blank line, without a trailing newline.
fn render_blocks(blocks: &[Value], cfg: &Value) -> Res<String> {
    let mut out = String::new();
    for (i, b) in blocks.iter().enumerate() {
        let kind = req(b, "type", "content block")?;
        let part = match kind {
            "heading" => {
                let level = b.get("level").and_then(Value::as_u64).unwrap_or(1);
                // Verified: always 1..=6 `#` (src/verified.rs).
                let hashes = usize::try_from(plalast::verified::heading_hashes(level)).unwrap_or(1);
                format!("{} {}", "#".repeat(hashes), text(cfg, b, kind)?)
            }
            "paragraph" => text(cfg, b, kind)?,
            "list" => items(cfg, b, kind)?
                .iter()
                .map(|i| format!("- {i}"))
                .collect::<Vec<_>>()
                .join("\n"),
            "ordered_list" => items(cfg, b, kind)?
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
            "table" => table(cfg, b)?,
            "structure_table" => structure_table(cfg),
            "schema" => schema_table(cfg, b)?,
            "image" => image(cfg, b)?,
            "agent_note" => agent_note(cfg)?.to_string(),
            "modes" => modes_block(cfg)?,
            "brain_keys" => brain_keys(cfg)?,
            other => return Err(format!("unknown content block type `{other}`")),
        };
        // A block with nothing to show (such as `modes` with every mode disabled) adds no lines.
        if part.is_empty() {
            continue;
        }
        // One blank line between blocks; the agent note sits right under the first line.
        if i > 0 && !out.is_empty() {
            out.push_str(if kind == "agent_note" { "\n" } else { "\n\n" });
        }
        out.push_str(&part);
    }
    Ok(out.trim_end().to_string())
}

// ---------- documents ----------

/// Renders document `key`: its main text plus one `(path, text)` per `on_demand` section.
fn render_doc(key: &str, doc: &Value, cfg: &Value) -> Res<(String, Vec<(String, String)>)> {
    let sections = arr(doc, "sections");
    let allowed: Vec<&str> = primary(cfg)["schema"]["levers"]
        .as_object()
        .map_or_default(|m| m.keys().map(String::as_str).collect());
    for id in cfg["params"]["levers"][key]
        .as_object()
        .into_iter()
        .flat_map(|m| m.keys())
    {
        if !sections.iter().any(|s| str_of(s, "id") == Some(id)) {
            return Err(format!(
                "params.levers.{key}.{id}: no such section in data.content.{key}"
            ));
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
        let state = lever(cfg, key, id);
        if !allowed.contains(&state) {
            return Err(format!(
                "params.levers.{key}.{id}: `{state}` is not one of {allowed:?}"
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
                let dir = req(paths(cfg), "on_demand", "config.paths")?.trim_end_matches('/');
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

/// `.claude/settings.json`: the Claude Code hooks of `config.hooks.claude`.
fn render_claude_settings(cfg: &Value) -> Res<String> {
    let hooks = &config(cfg)["hooks"]["claude"];
    if !hooks.is_object() {
        return Err("config.hooks.claude is missing".to_string());
    }
    let text =
        serde_json::to_string_pretty(&json!({ "hooks": hooks })).map_err(|e| e.to_string())?;
    Ok(format!("{text}\n"))
}

/// `.gitignore`: secret folders (all but their `.gitkeep`) and ignored paths.
fn render_gitignore(cfg: &Value) -> String {
    let mut out = String::new();
    for e in arr(data(cfg), "structure") {
        let Some(path) = entry_path(cfg, e) else {
            continue;
        };
        if e.get("secret").and_then(Value::as_bool) == Some(true) {
            let _ = write!(out, "{path}*\n!{path}.gitkeep\n");
        } else if str_of(e, "type") == Some("ignored")
            || e.get("local").and_then(Value::as_bool) == Some(true)
        {
            let _ = writeln!(out, "{path}");
        }
    }
    out
}

// ---------- what should exist ----------

enum Want {
    Dir(PathBuf),
    File(PathBuf, String),
    Link(PathBuf, String),
    /// A `local` link whose target is not on this machine: must not exist.
    NoLink(PathBuf),
}

/// Everything the brain says should exist on disk, in order.
fn wanted(root: &Path, cfg: &Value) -> Res<Vec<Want>> {
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
    for e in arr(data(cfg), "structure") {
        let path = entry_path(cfg, e)
            .ok_or_else(|| format!("data.structure entry without a resolvable path: {e}"))?;
        let full = root.join(path.trim_end_matches('/'));
        match req(e, "type", &path)? {
            "dir" => {
                out.push(Want::Dir(full.clone()));
                if e.get("gitkeep").and_then(Value::as_bool) == Some(true) {
                    out.push(Want::File(full.join(".gitkeep"), String::new()));
                }
            }
            "symlink" => {
                let target = match (str_of(e, "target"), str_of(e, "target_ref")) {
                    (Some(t), _) => t,
                    (None, Some(r)) => lookup(cfg, r).and_then(Value::as_str).ok_or_else(|| {
                        format!("{path}: target_ref `{r}` is not a string in the brain")
                    })?,
                    (None, None) => {
                        return Err(format!("{path}: symlink needs `target` or `target_ref`"))
                    }
                };
                // A `local` link depends on this machine's layout: it is git-ignored and only
                // built where its target exists (a relative target resolves from the link's folder).
                let local = e.get("local").and_then(Value::as_bool) == Some(true);
                let resolves = full.parent().is_some_and(|d| d.join(target).exists());
                if local && !resolves {
                    out.push(Want::NoLink(full));
                } else {
                    out.push(Want::Link(full, target.to_string()));
                }
            }
            "file" => match str_of(e, "build") {
                None => {}
                Some("gitignore") => add_file(&mut out, &path, render_gitignore(cfg))?,
                Some("claude_settings") => {
                    add_file(&mut out, &path, render_claude_settings(cfg)?)?;
                }
                Some(key) if SPECIAL_BUILDS.contains(&key) => {
                    return Err(format!("{path}: special build `{key}` has no renderer"))
                }
                Some(key) => {
                    let doc = &data(cfg)["content"][key];
                    if !doc.is_object() {
                        return Err(format!("{path}: data.content.{key} is missing"));
                    }
                    let (main, extra) = render_doc(key, doc, cfg)?;
                    add_file(&mut out, &path, main)?;
                    for (p, text) in extra {
                        add_file(&mut out, &p, text)?;
                    }
                }
            },
            "planned" | "ignored" => {}
            other => return Err(format!("{path}: unknown type `{other}`")),
        }
    }
    // Modes switched to `on_demand` get their own file.
    for (p, text) in mode_files(cfg)? {
        add_file(&mut out, &p, text)?;
    }
    // Guides: with `render: "pdf"`, every hand-written .md gets a .pdf next to it
    // (src/pdf.rs; deterministic, so --check compares it like any other file).
    if str_of(&primary(cfg)["guides"], "render") == Some("pdf") {
        if let Some(dir) = module_dir(cfg, "guides") {
            for name in visible_entries(&root.join(&dir)) {
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
    // Module links: one symlink per module, never a whole folder.
    for (source, dest) in module_links(cfg) {
        let up = "../".repeat(Path::new(&dest).components().count());
        for name in visible_entries(&root.join(&source)) {
            out.push(Want::Link(
                root.join(&dest).join(&name),
                format!("{up}{source}/{name}"),
            ));
        }
    }
    Ok(out)
}

/// Repo-relative paths of every file the build generates.
fn generated_files(root: &Path, wants: &[Want]) -> Vec<String> {
    wants
        .iter()
        .filter_map(|w| match w {
            Want::File(p, _) => Some(rel(root, p)),
            _ => None,
        })
        .collect()
}

// ---------- build ----------

/// Applies (or, with `check`, only reports) every difference between disk and the brain.
/// Returns false when the repo is out of sync.
fn build(root: &Path, cfg: &Value, check: bool) -> Res<bool> {
    let wants = wanted(root, cfg)?;
    let generated = generated_files(root, &wants);
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
            Want::NoLink(p) => {
                if !p.is_symlink() {
                    continue;
                }
                drift.push(format!(
                    "remove local link {} (its target is not on this machine)",
                    rel(root, &p)
                ));
                if !check {
                    fs::remove_file(&p).map_err(|e| format!("{}: {e}", rel(root, &p)))?;
                }
            }
        }
    }
    drift.extend(prune_module_links(root, cfg, check)?);
    let strays = stray_docs(root, cfg, &generated);

    let verb = if check { "out of sync" } else { "updated" };
    for d in &drift {
        println!("{verb}: {d}");
    }
    for s in &strays {
        println!("not in {MEMORY}: {s} (it is a doc, an .md in config.paths.docs, so it must be generated; add it to {MEMORY} or delete it)");
    }
    if drift.is_empty() && strays.is_empty() {
        println!("in sync with the brain ({MEMORY}, {})", plalast::PARAMS);
    }
    Ok(strays.is_empty() && (!check || drift.is_empty()))
}

/// Removes broken module links (left behind by deleted modules).
fn prune_module_links(root: &Path, cfg: &Value, check: bool) -> Res<Vec<String>> {
    let mut out = Vec::new();
    for (_source, dest) in module_links(cfg) {
        let dest = root.join(dest);
        for name in visible_entries(&dest) {
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

/// Docs (`.md` files directly in `config.paths.docs`) that this build does not generate.
fn stray_docs(root: &Path, cfg: &Value, generated: &[String]) -> Vec<String> {
    let mut out: Vec<String> = doc_dirs(cfg)
        .iter()
        .flat_map(|dir| {
            visible_entries(&root.join(dir))
                .into_iter()
                .map(move |name| {
                    if dir == "." {
                        name
                    } else {
                        format!("{dir}/{name}")
                    }
                })
        })
        .filter(|p| p.ends_with(".md") && root.join(p).is_file() && !generated.contains(p))
        .collect();
    out.sort();
    out
}

// ---------- guard ----------

/// The denial reason when `rel` is a generated file, or None to let the edit through.
fn guard_decision(rel: &str, generated: &[String]) -> Option<String> {
    generated.iter().any(|g| g == rel).then(|| {
        format!(
            "{rel} is generated by pla-build from the brain. Make the change in its section of \
             {MEMORY} (or params.yaml for settings and levers; see agents/BUILD.md), then run \
             `cargo run --release --bin pla-build`."
        )
    })
}

/// Claude Code `PreToolUse` hook: denies Edit/Write on generated files, silent otherwise.
fn guard(root: &Path, cfg: &Value) -> Res<bool> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| e.to_string())?;
    let payload: Value = serde_json::from_str(&input).map_err(|e| format!("hook payload: {e}"))?;
    let Some(file) = payload["tool_input"]["file_path"].as_str() else {
        return Ok(true);
    };
    // Resolve symlinks (such as .claude/AGENTS.md) so every route to a file is caught.
    let file = Path::new(file);
    let resolved = fs::canonicalize(file).unwrap_or_else(|_| {
        let parent = file.parent().and_then(|p| fs::canonicalize(p).ok());
        match (parent, file.file_name()) {
            (Some(p), Some(n)) => p.join(n),
            _ => file.to_path_buf(),
        }
    });
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    if !resolved.starts_with(&root) {
        return Ok(true);
    }
    let generated = generated_files(&root, &wanted(&root, cfg)?);
    if let Some(reason) = guard_decision(&rel(&root, &resolved), &generated) {
        let out = json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": reason
            }
        });
        println!("{out}");
    }
    Ok(true)
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    /// A brain with one document `doc` holding sections `a` and `b` with the given levers.
    fn cfg(levers: &Value) -> Value {
        json!({
            "config": {"paths": {"on_demand": "agents"}},
            "params": {"levers": {"doc": levers}},
            "primary": {
                "docs": {
                    "agent_note": "NOTE",
                    "on_demand": {"heading": "On demand", "pointer": "`{path}`: {title}. Read it when {when}."}
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

    fn render(levers: &Value) -> Res<(String, Vec<(String, String)>)> {
        render_doc("doc", &doc(), &cfg(levers))
    }

    #[test]
    fn renders_blocks_with_one_blank_line_between() {
        let blocks = json!([
            {"type": "heading", "level": 2, "text": "Title"},
            {"type": "list", "items": ["a", "b"]},
            {"type": "ordered_list", "items": ["x", "y"]}
        ]);
        let out = render_blocks(blocks.as_array().unwrap(), &cfg(&json!({}))).unwrap();
        assert_eq!(out, "## Title\n\n- a\n- b\n\n1. x\n2. y");
    }

    #[test]
    fn enabled_sections_render_inline() {
        let (main, files) = render(&json!({"a": "enabled", "b": "enabled"})).unwrap();
        assert_eq!(main, "# Doc\n\n## A\n\nalpha\n\n## B\n\n- x\n- y\n");
        assert!(files.is_empty());
    }

    #[test]
    fn on_demand_section_gets_its_own_file_and_a_pointer() {
        let (main, files) = render(&json!({"a": "on_demand", "b": "enabled"})).unwrap();
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
        let (main, files) = render(&json!({"a": "disabled", "b": "enabled"})).unwrap();
        assert_eq!(main, "# Doc\n\n## B\n\n- x\n- y\n");
        assert!(files.is_empty());
    }

    #[test]
    fn a_missing_lever_defaults_to_on_demand() {
        let (main, files) = render(&json!({"a": "enabled"})).unwrap();
        assert!(
            main.contains("## A") && main.contains("`agents/b.md`"),
            "{main}"
        );
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn brain_keys_is_a_table_of_the_three_primary_keys() {
        let mut c = json!({});
        for k in BRAIN_KEYS {
            c[k] = json!({"descr_+": format!("{k} is"), "descr_-": format!("{k} is not")});
        }
        let t = brain_keys(&c).unwrap();
        assert!(
            t.starts_with("| Key | Is (descr_+) | Is not (descr_-) |"),
            "{t}"
        );
        assert!(
            t.contains("| `primary` | primary is | primary is not |"),
            "{t}"
        );
        c["data"] = json!({"descr_+": "only half"});
        assert!(brain_keys(&c).unwrap_err().contains("descr_-"));
    }

    #[test]
    fn modes_follow_their_interruptor() {
        let brain = |state: &str| {
            let mut c = cfg(&json!({}));
            c["params"]["levers"]["mode"] = json!({"careful": state});
            c["primary"]["mode"] = json!({"careful": {"title": "Careful", "when": "in doubt", "rules": ["Ask.", "Wait."]}});
            c
        };
        let c = brain("enabled");
        assert_eq!(
            modes_block(&c).unwrap(),
            "- Mode `careful` (enabled): Ask. Wait."
        );
        assert!(mode_files(&c).unwrap().is_empty());
        let c = brain("on_demand");
        assert_eq!(
            modes_block(&c).unwrap(),
            "- Mode `careful` (on_demand): read `agents/mode-careful.md` when in doubt."
        );
        assert_eq!(
            mode_files(&c).unwrap(),
            vec![(
                "agents/mode-careful.md".to_string(),
                "# Careful\nNOTE\n\n- Ask.\n- Wait.\n".to_string()
            )]
        );
        let c = brain("disabled");
        assert_eq!(modes_block(&c).unwrap(), "");
        assert!(mode_files(&c).unwrap().is_empty());
        assert!(modes_block(&brain("maybe")).is_err());
    }

    #[test]
    fn lever_problems_are_errors() {
        let invalid = render(&json!({"a": "maybe", "b": "enabled"})).unwrap_err();
        assert!(invalid.contains("`maybe` is not one of"), "{invalid}");
        let unknown = render(&json!({"a": "enabled", "b": "enabled", "c": "enabled"})).unwrap_err();
        assert!(unknown.contains("params.levers.doc.c"), "{unknown}");
    }

    #[test]
    fn agent_note_sits_on_the_second_line() {
        let d = json!({
            "head": [{"type": "heading", "level": 1, "text": "Doc"}, {"type": "agent_note"},
                     {"type": "paragraph", "text": "intro"}],
            "sections": []
        });
        let (main, _) = render_doc("doc", &d, &cfg(&json!({}))).unwrap();
        assert_eq!(main, "# Doc\nNOTE\n\nintro\n");
        let no_note = json!({"primary": {"docs": {}}});
        assert!(render_blocks(
            json!([{"type": "agent_note"}]).as_array().unwrap(),
            &no_note
        )
        .is_err());
    }

    #[test]
    fn empty_document_is_an_empty_file() {
        let empty = json!({"head": [], "sections": []});
        assert_eq!(render_doc("doc", &empty, &cfg(&json!({}))).unwrap().0, "");
    }

    #[test]
    fn schema_block_renders_a_table_in_file_order() {
        let c = json!({"primary": {"schema": {"levers": {"enabled": "on", "disabled": "off"}}}});
        let b = json!([{"type": "schema", "part": "levers", "header": ["Value", "Effect"]}]);
        assert_eq!(
            render_blocks(b.as_array().unwrap(), &c).unwrap(),
            "| Value | Effect |\n|---|---|\n| `enabled` | on |\n| `disabled` | off |"
        );
    }

    #[test]
    fn table_rows_can_come_from_objects_newest_first() {
        let c = json!({"primary": {"log": [{"d": "1", "x": "first"}, {"d": "2", "x": "second"}]}});
        let b = json!([{"type": "table", "ref": "primary.log", "reverse": true,
                        "header": ["D", "X"], "row": ["{d}", "`{x}`"]}]);
        assert_eq!(
            render_blocks(b.as_array().unwrap(), &c).unwrap(),
            "| D | X |\n|---|---|\n| 2 | `second` |\n| 1 | `first` |"
        );
    }

    #[test]
    fn text_list_and_image_blocks_can_point_into_the_brain() {
        let c =
            json!({"primary": {"p": {"title": "T", "rules": ["a", "b"], "logo": "src/logo.png"}}});
        let b = json!([
            {"type": "heading", "level": 1, "ref": "primary.p.title"},
            {"type": "list", "ref": "primary.p.rules"},
            {"type": "image", "ref": "primary.p.logo", "alt": "Logo", "width": 200}
        ]);
        assert_eq!(
            render_blocks(b.as_array().unwrap(), &c).unwrap(),
            "# T\n\n- a\n- b\n\n<img src=\"src/logo.png\" alt=\"Logo\" width=\"200\">"
        );
        let bad = json!([{"type": "list", "ref": "primary.p.title"}]);
        assert!(render_blocks(bad.as_array().unwrap(), &c).is_err());
    }

    #[test]
    fn structure_table_resolves_paths_and_labels() {
        let c = json!({
            "config": {"paths": {"skills": "agents/skills"}},
            "data": {"structure": [
                {"path_ref": "config.paths.skills", "type": "dir", "label": "{path}<x>/SKILL.md", "table": "Skills"},
                {"path": "b", "type": "file"},
                {"path": "src/", "type": "planned", "table": "Code"}
            ]}
        });
        assert_eq!(
            structure_table(&c),
            "Paths are added here as they are created (`src/` is planned, not created yet):\n\n\
             | Path | Content |\n|---|---|\n| `agents/skills/<x>/SKILL.md` | Skills |\n| `src/` | Code |"
        );
    }

    #[test]
    fn gitignore_covers_secrets_and_ignored_paths() {
        let c = json!({"data": {"structure": [
            {"path": ".secrets/", "type": "dir", "secret": true},
            {"path": "target/", "type": "ignored"},
            {"path": "modules/vault", "type": "symlink", "target": "../../vault", "local": true}
        ]}});
        assert_eq!(
            render_gitignore(&c),
            ".secrets/*\n!.secrets/.gitkeep\ntarget/\nmodules/vault\n"
        );
    }

    #[test]
    fn local_links_are_built_only_where_their_target_exists() {
        let root = env::temp_dir().join(format!("pla-local-links-{}", std::process::id()));
        fs::create_dir_all(root.join("modules")).unwrap();
        fs::create_dir_all(root.join("here")).unwrap();
        let c = json!({"config": {}, "primary": {}, "data": {"structure": [
            {"path": "modules/found", "type": "symlink", "target": "../here", "local": true},
            {"path": "modules/missing", "type": "symlink", "target": "../nowhere", "local": true},
            {"path": "modules/shared", "type": "symlink", "target": "../nowhere"}
        ]}});
        let kinds: Vec<String> = wanted(&root, &c)
            .unwrap()
            .iter()
            .map(|w| match w {
                Want::Link(p, _) => format!("link {}", rel(&root, p)),
                Want::NoLink(p) => format!("nolink {}", rel(&root, p)),
                _ => "other".into(),
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "link modules/found",
                "nolink modules/missing",
                "link modules/shared"
            ]
        );
        // A stale local link is removed, and --check reports it until then.
        symlink("../nowhere", root.join("modules/missing")).unwrap();
        assert!(!build(&root, &c, true).unwrap());
        assert!(build(&root, &c, false).unwrap());
        assert!(root.join("modules/missing").symlink_metadata().is_err());
        assert!(build(&root, &c, true).unwrap());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn claude_settings_wrap_the_configured_hooks() {
        let c = json!({"config": {"hooks": {"claude": {"PreToolUse": []}}}});
        assert_eq!(
            render_claude_settings(&c).unwrap(),
            "{\n  \"hooks\": {\n    \"PreToolUse\": []\n  }\n}\n"
        );
        assert!(render_claude_settings(&json!({"params": {}})).is_err());
    }

    #[test]
    fn guard_denies_only_generated_files() {
        let generated = vec!["agents/AGENTS.md".to_string(), "README.md".to_string()];
        assert!(guard_decision("agents/AGENTS.md", &generated)
            .unwrap()
            .contains(MEMORY));
        assert!(guard_decision("src/lib.rs", &generated).is_none());
        assert!(guard_decision("memory.json", &generated).is_none());
    }

    #[test]
    fn doc_paths_come_from_params() {
        let c = json!({"config": {"paths": {"docs": [".", "agents/"]}}});
        assert!(plalast::is_doc_path(&c, "README.md"));
        assert!(plalast::is_doc_path(&c, "agents/BUILD.md"));
        assert!(!plalast::is_doc_path(&c, "agents/skills/x/SKILL.md"));
        assert!(!plalast::is_doc_path(&c, "agents/notes.json"));
    }

    #[test]
    fn unknown_block_type_is_an_error() {
        let blocks = json!([{"type": "raw", "text": "x"}]);
        assert!(render_blocks(blocks.as_array().unwrap(), &cfg(&json!({}))).is_err());
    }
}
