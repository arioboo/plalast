//! lint: checks that the repo is well formed.
//!
//! - `pla-lint`                            every area.
//! - `pla-lint [memory] [modules] [rust]`  only the given areas.
//!
//! `memory`: memory.json and params.yaml are consistent and fully documented in `primary.schema`.
//! `modules`: hand-written modules in `agents/` subfolders have the expected shape.
//! `rust`: `cargo fmt --check`, `cargo clippy -D warnings` and `cargo verus verify` pass.

use plalast::{
    arr, block_items, block_text, cargo, config, data, doc_dirs, entry_path, exit,
    external_modules, is_doc_path, lever, lookup, module_dir, open, paths, primary, run, str_of,
    visible_entries, Findings, CONFIG_KEYS, LOG_FIELDS, LOG_KINDS, MEMORY, MODULE_KINDS, PARAMS,
    PRIMARY_KEYS, SPECIAL_BUILDS,
};
use serde_json::Value;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::ExitCode;

const AREAS: [&str; 3] = ["memory", "modules", "rust"];
const ENTRY_TYPES: [&str; 5] = ["dir", "symlink", "file", "ignored", "planned"];
/// Findings about the brain are reported under this area.
const M: &str = "memory";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    exit((|| {
        if let Some(bad) = args.iter().find(|a| !AREAS.contains(&a.as_str())) {
            return Err(format!(
                "unknown area {bad}\nusage: pla-lint [memory] [modules] [rust]"
            ));
        }
        let want = |area: &str| args.is_empty() || args.iter().any(|a| a == area);
        let (root, cfg) = open()?;
        let mut f = Findings::default();
        if want("memory") {
            lint_memory(&root, &cfg, &mut f);
        }
        if want("modules") {
            lint_modules(&root, &cfg, &mut f);
        }
        if want("rust") {
            lint_rust(&root, &mut f);
        }
        Ok(f.finish("pla-lint"))
    })())
}

fn is_kebab(s: &str) -> bool {
    !s.is_empty()
        && s.split('-').all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

fn blank(v: Option<&str>) -> bool {
    v.is_none_or(|s| s.trim().is_empty())
}

fn keys(v: &Value) -> Vec<&str> {
    v.as_object()
        .map_or_default(|m| m.keys().map(String::as_str).collect())
}

// ---------- memory ----------

fn lint_memory(root: &Path, cfg: &Value, f: &mut Findings) {
    lint_schema_coverage(cfg, f);
    lint_registries(cfg, f);
    lint_structure(root, cfg, f);
    lint_docs(cfg, f);
    lint_modules_and_paths(root, cfg, f);
    lint_params(root, cfg, f);
    lint_plain_data(root, cfg, f);
}

/// Every key of params, params.config, primary and data is documented in primary.schema.keys;
/// a key with documented children has all of them documented.
fn lint_schema_coverage(cfg: &Value, f: &mut Findings) {
    let documented = &primary(cfg)["schema"]["keys"];
    for (top, obj) in [
        ("params", &cfg["params"]),
        ("params.config", config(cfg)),
        ("primary", primary(cfg)),
        ("data", data(cfg)),
    ] {
        for k in keys(obj) {
            let full = format!("{top}.{k}");
            if documented.get(&full).is_none() {
                f.error(
                    M,
                    format!("{full} is not documented in primary.schema.keys"),
                );
            }
            let prefix = format!("{full}.");
            let documents_children = keys(documented).iter().any(|d| d.starts_with(&prefix));
            if documents_children {
                for sub in keys(&obj[k]) {
                    if documented.get(format!("{prefix}{sub}")).is_none() {
                        f.error(
                            M,
                            format!("{prefix}{sub} is not documented in primary.schema.keys"),
                        );
                    }
                }
            }
        }
    }
}

/// primary and params.config hold exactly the keys the tools handle (lib registries), in order.
fn lint_registries(cfg: &Value, f: &mut Findings) {
    for (name, obj, registry, file) in [
        ("primary", primary(cfg), &PRIMARY_KEYS[..], MEMORY),
        ("params.config", config(cfg), &CONFIG_KEYS[..], PARAMS),
    ] {
        let known: Vec<&str> = registry.iter().map(|(k, _)| *k).collect();
        let present = keys(obj);
        for k in &present {
            if !known.contains(k) {
                f.error(M, format!("{name}.{k}: no tool handles it; add it to the registry in src/lib.rs and wire it"));
            }
        }
        for k in &known {
            if !present.contains(k) {
                f.error(M, format!("{name}.{k} is missing from {file}"));
            }
        }
    }
}

/// data.structure: resolvable unique paths, descriptions, known types, and per-type fields.
fn lint_structure(root: &Path, cfg: &Value, f: &mut Findings) {
    let mut seen: Vec<String> = Vec::new();
    for e in arr(data(cfg), "structure") {
        let Some(path) = entry_path(cfg, e) else {
            let r = str_of(e, "path_ref").unwrap_or("?");
            f.error(M, format!("data.structure: an entry has no `path`, or its path_ref `{r}` does not resolve"));
            continue;
        };
        // Messages show the path as written (path_ref entries show the resolved one).
        let ctx = format!("data.structure `{}`", str_of(e, "path").unwrap_or(&path));
        if seen.contains(&path) {
            f.error(M, format!("{ctx}: duplicate path"));
        }
        seen.push(path.clone());
        if blank(str_of(e, "description")) {
            f.error(M, format!("{ctx}: missing description"));
        }
        let ty = str_of(e, "type").unwrap_or("");
        if !ENTRY_TYPES.contains(&ty) {
            f.error(M, format!("{ctx}: unknown type `{ty}`"));
            continue;
        }
        // Folders written literally end with `/` (path_ref folders are normalized).
        let literal = str_of(e, "path").unwrap_or("");
        if ty == "dir" && !literal.is_empty() && !literal.ends_with('/')
            || matches!(ty, "file" | "symlink") && path.ends_with('/')
        {
            f.error(M, format!("{ctx}: only folders end with `/`"));
        }
        match ty {
            "symlink" => match (str_of(e, "target"), str_of(e, "target_ref")) {
                (None, None) => f.error(
                    M,
                    format!("{ctx}: symlink without `target` or `target_ref`"),
                ),
                (None, Some(r)) if lookup(cfg, r).and_then(Value::as_str).is_none() => {
                    f.error(
                        M,
                        format!("{ctx}: target_ref `{r}` is not a string in the brain"),
                    );
                }
                _ => {}
            },
            "file" => match str_of(e, "build") {
                Some(b) if SPECIAL_BUILDS.contains(&b) => {}
                Some(key) if !data(cfg)["content"][key].is_object() => {
                    f.error(M, format!("{ctx}: data.content.{key} does not exist"));
                }
                Some(_) => {}
                None => {
                    if !root.join(&path).exists() {
                        f.error(M, format!("{ctx}: hand-written file is missing on disk"));
                    }
                    if is_doc_path(cfg, &path) {
                        f.error(M, format!("{ctx}: it is a doc (an .md in params.config.paths.docs), so it must be generated (set `build`)"));
                    }
                }
            },
            _ => {}
        }
    }
}

/// data.content and params.levers: every doc is built, carries the agent note, has unique
/// kebab-case section ids with `title` and `when`, a valid lever each, and valid blocks.
fn lint_docs(cfg: &Value, f: &mut Findings) {
    let schema = &primary(cfg)["schema"];
    let built: Vec<&str> = arr(data(cfg), "structure")
        .iter()
        .filter_map(|e| str_of(e, "build"))
        .collect();
    let mut ids: Vec<String> = Vec::new();
    for (key, doc) in data(cfg)["content"].as_object().into_iter().flatten() {
        if !built.contains(&key.as_str()) {
            f.error(
                M,
                format!("data.content.{key}: no data.structure entry builds it"),
            );
        }
        check_blocks(
            cfg,
            arr(doc, "head"),
            &format!("data.content.{key}.head"),
            f,
        );
        let second = arr(doc, "head").get(1).and_then(|b| str_of(b, "type"));
        if second != Some("agent_note") {
            f.error(
                M,
                format!("data.content.{key}.head: the second block must be `agent_note`"),
            );
        }
        for s in arr(doc, "sections") {
            let id = str_of(s, "id").unwrap_or("");
            let ctx = format!("data.content.{key} section `{id}`");
            if !is_kebab(id) {
                f.error(M, format!("{ctx}: id must be kebab-case"));
            }
            if ids.iter().any(|x| x == id) {
                f.error(
                    M,
                    format!("{ctx}: id already used elsewhere (on_demand files would collide)"),
                );
            }
            ids.push(id.to_string());
            for k in ["title", "when"] {
                if blank(str_of(s, k)) {
                    f.error(M, format!("{ctx}: missing `{k}`"));
                }
            }
            match lever(cfg, key, id) {
                None => f.error(M, format!("{ctx}: no lever in params.levers.{key}")),
                Some(v) if schema["levers"].get(v).is_none() => {
                    f.error(
                        M,
                        format!("{ctx}: lever `{v}` is not in primary.schema.levers"),
                    );
                }
                Some(_) => {}
            }
            check_blocks(cfg, arr(s, "blocks"), &ctx, f);
        }
    }
    for (key, m) in cfg["params"]["levers"].as_object().into_iter().flatten() {
        for id in keys(m) {
            let exists = arr(&data(cfg)["content"][key], "sections")
                .iter()
                .any(|s| str_of(s, "id") == Some(id));
            if !exists {
                f.error(M, format!("params.levers.{key}.{id}: no such section"));
            }
        }
    }
}

/// Block types are documented, and every `ref` points at the right kind of value.
fn check_blocks(cfg: &Value, blocks: &[Value], ctx: &str, f: &mut Findings) {
    for b in blocks {
        let t = str_of(b, "type").unwrap_or("");
        if primary(cfg)["schema"]["blocks"].get(t).is_none() {
            f.error(
                M,
                format!("{ctx}: block type `{t}` is not documented in primary.schema.blocks"),
            );
        }
        if let Some(r) = str_of(b, "ref") {
            let resolves = match t {
                "list" | "ordered_list" => block_items(cfg, b).is_some(),
                "table" => lookup(cfg, r).is_some_and(Value::is_array),
                "image" => lookup(cfg, r).is_some_and(Value::is_string),
                _ => block_text(cfg, b).is_some(),
            };
            if !resolves {
                f.error(
                    M,
                    format!("{ctx}: {t} block ref `{r}` does not point at the right kind of value"),
                );
            }
        }
    }
}

/// Module kinds (definition in primary, folder and links in params) and the other paths.
fn lint_modules_and_paths(root: &Path, cfg: &Value, f: &mut Findings) {
    for kind in MODULE_KINDS {
        if blank(str_of(&primary(cfg)[kind], "definition")) {
            f.error(M, format!("primary.{kind}: missing `definition`"));
        }
        let ctx = format!("params.config.paths.modules.{kind}");
        match module_dir(cfg, kind) {
            Some(d) if root.join(&d).is_dir() => {}
            Some(d) => f.error(M, format!("{ctx}.dir: `{d}` is not a folder")),
            None => f.error(M, format!("{ctx}: missing `dir`")),
        }
        for (p, dest) in paths(cfg)["modules"][kind]["links"]
            .as_object()
            .into_iter()
            .flatten()
        {
            match dest.as_str() {
                Some(d) if root.join(d).is_dir() => {}
                Some(d) => f.error(M, format!("{ctx}.links.{p}: `{d}` is not a folder")),
                None => f.error(M, format!("{ctx}.links.{p}: not a path")),
            }
        }
    }
    // External modules: a definition, and a path that is relative (an absolute one leaks the home
    // folder) and, like any symlink target, read from the folder of the link that targets it.
    for (name, path, definition) in external_modules(cfg) {
        if blank(definition) {
            f.error(M, format!("primary.modules.{name}: missing `definition`"));
        }
        let ctx = format!("params.config.paths.modules.{name}");
        let link_dir = arr(data(cfg), "structure")
            .iter()
            .find(|e| str_of(e, "target_ref") == Some(ctx.as_str()))
            .and_then(|e| str_of(e, "path"))
            .and_then(|p| Path::new(p).parent())
            .map_or_else(|| root.to_path_buf(), |d| root.join(d));
        match path {
            None => f.error(M, format!("{ctx}: missing (the path of the module)")),
            Some(p) if Path::new(p).is_absolute() => {
                f.error(
                    M,
                    format!("{ctx}: `{p}` must be relative to the folder of its link"),
                );
            }
            Some(p) if !link_dir.join(p).is_dir() => {
                f.warn(M, format!("{ctx}: `{p}` is not a folder on this machine"));
            }
            Some(_) => {}
        }
    }
    if blank(str_of(paths(cfg), "on_demand")) {
        f.error(M, "params.config.paths: missing `on_demand`");
    }
    if doc_dirs(cfg).is_empty() {
        f.error(
            M,
            "params.config.paths.docs is empty: no folder holds the docs",
        );
    }
    for d in doc_dirs(cfg) {
        if !root.join(&d).is_dir() {
            f.error(
                M,
                format!("params.config.paths.docs: `{d}` is not a folder"),
            );
        }
    }
    if let Some(logo) = primary(cfg)["project"]["plalast_logo"].as_str() {
        if !root.join(logo).is_file() {
            f.error(
                M,
                format!("primary.project.plalast_logo: `{logo}` does not exist"),
            );
        }
    }
}

/// params.config settings: branches, `PLALAST_DIR`, lint budget, hooks and audit.
fn lint_params(root: &Path, cfg: &Value, f: &mut Findings) {
    let c = config(cfg);
    for b in ["integration", "stable"] {
        if blank(str_of(&c["branches"], b)) {
            f.error(M, format!("params.config.branches: missing `{b}`"));
        }
    }
    // PLALAST_DIR is `$PWD`: the tools resolve it to the repo root they find, on any machine
    // (a written path works on one machine only, and an expanded one leaks the user name).
    match str_of(c, "PLALAST_DIR") {
        Some("$PWD" | "${PWD}") => {}
        Some(d) => f.error(
            M,
            format!("params.config.PLALAST_DIR: `{d}` must be `$PWD`"),
        ),
        None => f.error(M, "params.config: missing `PLALAST_DIR`"),
    }
    // Context budget: the docs an agent loads every session stay under lint.max_lines.
    let lint_cfg = &c["lint"];
    match (
        lint_cfg["max_lines"].as_u64(),
        lint_cfg["always_on"].as_array(),
    ) {
        (Some(max), Some(files)) => {
            let mut total = 0;
            for file in files.iter().filter_map(Value::as_str) {
                match fs::read_to_string(root.join(file)) {
                    Ok(t) => total += t.lines().count() as u64,
                    Err(_) => f.error(
                        M,
                        format!("params.config.lint.always_on: `{file}` cannot be read"),
                    ),
                }
            }
            if total > max {
                f.error(M, format!("always-on docs have {total} lines, over params.config.lint.max_lines ({max}): move sections to on_demand"));
            }
        }
        _ => f.error(
            M,
            "params.config.lint needs `always_on` (list of docs) and `max_lines` (number)",
        ),
    }
    // Hooks: every script is executable, and every Claude Code hook command exists.
    let hooks = &c["hooks"];
    let hooks_dir = str_of(hooks, "dir")
        .unwrap_or_default()
        .trim_end_matches('/');
    if !root.join(hooks_dir).is_dir() {
        f.error(
            M,
            format!("params.config.hooks.dir: `{hooks_dir}` is not a folder"),
        );
    }
    for name in visible_entries(&root.join(hooks_dir)) {
        let mode =
            fs::metadata(root.join(hooks_dir).join(&name)).map_or(0, |m| m.permissions().mode());
        if mode & 0o111 == 0 {
            f.error(
                M,
                format!("{hooks_dir}/{name}: hook script is not executable (chmod +x)"),
            );
        }
    }
    let commands = hooks["claude"]
        .as_object()
        .into_iter()
        .flat_map(|events| events.values())
        .flat_map(|groups| groups.as_array().into_iter().flatten())
        .flat_map(|g| g["hooks"].as_array().into_iter().flatten())
        .filter_map(|h| str_of(h, "command"));
    for cmd in commands {
        let rel = cmd
            .replace("\"$CLAUDE_PROJECT_DIR\"/", "")
            .replace("${CLAUDE_PROJECT_DIR}/", "");
        if !root.join(&rel).is_file() {
            f.error(
                M,
                format!("params.config.hooks.claude: command `{cmd}` points at a missing script"),
            );
        }
    }
    let audit = &c["audit"];
    if audit.is_object() {
        if !audit["allow"]
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string))
        {
            f.error(M, "params.config.audit.allow must be a list of strings");
        }
        if !audit["max_file_kb"].is_u64() {
            f.error(M, "params.config.audit.max_file_kb must be a whole number");
        }
    } else {
        f.error(M, "params.config.audit is missing (settings of pla-audit)");
    }
}

/// Shapes of primary's plain data: features, log (with real commits), index, commands.
fn lint_plain_data(root: &Path, cfg: &Value, f: &mut Findings) {
    for (k, v) in primary(cfg)["features"].as_object().into_iter().flatten() {
        let numbered = k
            .strip_prefix("feature_")
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        if !numbered {
            f.error(M, format!("primary.features.{k}: keys are feature_<n>"));
        }
        for field in ["descr", "status"] {
            if !v[field].is_string() {
                f.error(
                    M,
                    format!("primary.features.{k}: `{field}` must be a string"),
                );
            }
        }
    }
    match primary(cfg)["log"].as_array() {
        None => f.error(M, "primary.log must be a list of entries"),
        Some(entries) => {
            for (i, e) in entries.iter().enumerate() {
                for k in LOG_FIELDS {
                    if blank(str_of(e, k)) {
                        f.error(M, format!("primary.log[{i}]: missing `{k}`"));
                    }
                }
                if let Some(kind) = str_of(e, "kind") {
                    if !LOG_KINDS.contains(&kind) {
                        f.error(
                            M,
                            format!("primary.log[{i}]: kind `{kind}` must be one of {LOG_KINDS:?}"),
                        );
                    }
                }
                if let Some(h) = str_of(e, "commit") {
                    let obj = format!("{h}^{{commit}}");
                    if !matches!(run(root, "git", &["cat-file", "-e", &obj]), Ok((true, _))) {
                        f.error(M, format!("primary.log[{i}]: commit `{h}` is not in git"));
                    }
                }
            }
        }
    }
    if !primary(cfg)["index"].is_object() {
        f.error(M, "primary.index must be an object");
    }
    for (i, c) in arr(primary(cfg), "commands").iter().enumerate() {
        for k in ["cmd", "summary", "detail"] {
            if blank(str_of(c, k)) {
                f.error(M, format!("primary.commands[{i}]: missing `{k}`"));
            }
        }
    }
}

// ---------- modules ----------

/// `key: value` pairs of a leading `---` frontmatter, or None if there is none.
fn frontmatter(text: &str) -> Option<Vec<(String, String)>> {
    let rest = text.strip_prefix("---\n")?;
    let end = rest.find("\n---")?;
    Some(
        rest[..end]
            .lines()
            .filter_map(|l| {
                let (k, v) = l.split_once(':')?;
                Some((k.trim().to_string(), v.trim().trim_matches('"').to_string()))
            })
            .collect(),
    )
}

/// A module file needs frontmatter whose `name` is `expected` and a non-empty `description`.
fn check_frontmatter(f: &mut Findings, file: &str, text: &str, expected: &str) {
    let Some(fm) = frontmatter(text) else {
        f.error(
            "modules",
            format!("{file}: missing frontmatter with `name` and `description`"),
        );
        return;
    };
    let get = |k: &str| fm.iter().find(|(x, _)| x == k).map(|(_, v)| v.as_str());
    match get("name") {
        Some(n) if n == expected => {}
        Some(n) => f.error(
            "modules",
            format!("{file}: name `{n}` must be `{expected}`"),
        ),
        None => f.error("modules", format!("{file}: frontmatter has no `name`")),
    }
    if blank(get("description")) {
        f.error(
            "modules",
            format!("{file}: frontmatter has no `description`"),
        );
    }
}

/// Reads `dir/name`, or reports it.
fn read(f: &mut Findings, dir: &Path, name: &str, rel: &str) -> Option<String> {
    fs::read_to_string(dir.join(name))
        .map_err(|e| f.error("modules", format!("{rel}: {e}")))
        .ok()
}

fn lint_modules(root: &Path, cfg: &Value, f: &mut Findings) {
    const A: &str = "modules";
    let dir_of = |kind: &str| module_dir(cfg, kind).unwrap_or_default();
    // Skills: <dir>/<name>/SKILL.md, name in kebab-case and in the frontmatter.
    let skills_dir = dir_of("skills");
    for name in visible_entries(&root.join(&skills_dir)) {
        let dir = root.join(&skills_dir).join(&name);
        let rel = format!("{skills_dir}/{name}");
        if !dir.is_dir() {
            f.error(A, format!("{rel}: a skill is a folder"));
            continue;
        }
        if !is_kebab(&name) {
            f.error(A, format!("{rel}: folder name must be kebab-case"));
        }
        // Exact case: the folder listing must contain `SKILL.md` itself.
        if !visible_entries(&dir).iter().any(|n| n == "SKILL.md") {
            f.error(A, format!("{rel}: missing SKILL.md (exact uppercase name)"));
            continue;
        }
        if let Some(text) = read(f, &dir, "SKILL.md", &rel) {
            check_frontmatter(f, &format!("{rel}/SKILL.md"), &text, &name);
        }
    }
    // Subagents: <dir>/<name>.md with frontmatter name == file name.
    let agents_dir = dir_of("agents");
    for name in visible_entries(&root.join(&agents_dir)) {
        let rel = format!("{agents_dir}/{name}");
        let Some(stem) = name.strip_suffix(".md") else {
            f.error(A, format!("{rel}: a subagent is one .md file"));
            continue;
        };
        if !is_kebab(stem) {
            f.error(A, format!("{rel}: file name must be kebab-case"));
        }
        if let Some(text) = read(f, &root.join(&agents_dir), &name, &rel) {
            check_frontmatter(f, &rel, &text, stem);
        }
    }
    // Rules: <dir>/<name>.md, not empty.
    let rules_dir = dir_of("rules");
    for name in visible_entries(&root.join(&rules_dir)) {
        let rel = format!("{rules_dir}/{name}");
        if !name.ends_with(".md") {
            f.error(A, format!("{rel}: a rule is one .md file"));
        } else if read(f, &root.join(&rules_dir), &name, &rel).is_some_and(|t| t.trim().is_empty())
        {
            f.error(A, format!("{rel}: empty"));
        }
    }
    // Connectors: one MCP server per json file in the neutral format.
    let mcp_dir = dir_of("connectors");
    for name in visible_entries(&root.join(&mcp_dir)) {
        let rel = format!("{mcp_dir}/{name}");
        if let Some(text) = read(f, &root.join(&mcp_dir), &name, &rel) {
            lint_connector(f, &rel, &name, &text);
        }
    }
    // Guides: .md sources, each with the .pdf that pla-build renders from it.
    let guides_dir = dir_of("guides");
    let names = visible_entries(&root.join(&guides_dir));
    for name in &names {
        let rel = format!("{guides_dir}/{name}");
        if let Some(stem) = name.strip_suffix(".pdf") {
            if !names.contains(&format!("{stem}.md")) {
                f.error(A, format!("{rel}: no {stem}.md to build it from (pla-build renders guides from their .md)"));
            }
        } else if name.ends_with(".md") {
            if read(f, &root.join(&guides_dir), name, &rel).is_some_and(|t| t.trim().is_empty()) {
                f.error(A, format!("{rel}: empty"));
            }
        } else {
            f.error(
                A,
                format!("{rel}: a guide is an .md source and its rendered .pdf"),
            );
        }
    }
}

/// A connector: `name` = file name, `transport` stdio (with `command`) or http (with `url`),
/// and `env` values that are only `${VAR}` references, never secrets.
fn lint_connector(f: &mut Findings, rel: &str, file: &str, text: &str) {
    const A: &str = "modules";
    let Some(stem) = file.strip_suffix(".json") else {
        f.error(A, format!("{rel}: a connector is one .json file"));
        return;
    };
    let v: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            f.error(A, format!("{rel}: invalid json: {e}"));
            return;
        }
    };
    match str_of(&v, "name") {
        Some(n) if n == stem => {}
        Some(n) => f.error(A, format!("{rel}: name `{n}` must be `{stem}`")),
        None => f.error(A, format!("{rel}: missing `name`")),
    }
    match str_of(&v, "transport") {
        Some("stdio") if str_of(&v, "command").is_none() => {
            f.error(A, format!("{rel}: stdio transport needs `command`"));
        }
        Some("http") if str_of(&v, "url").is_none() => {
            f.error(A, format!("{rel}: http transport needs `url`"));
        }
        Some("stdio" | "http") => {}
        Some(t) => f.error(
            A,
            format!("{rel}: transport `{t}` must be `stdio` or `http`"),
        ),
        None => f.error(A, format!("{rel}: missing `transport`")),
    }
    for (k, val) in v["env"].as_object().into_iter().flatten() {
        let is_ref = val
            .as_str()
            .is_some_and(|s| s.starts_with("${") && s.ends_with('}') && s.len() > 3);
        if !is_ref {
            f.error(
                A,
                format!(
                    "{rel}: env `{k}` must be a `${{VAR}}` reference (values live in .secrets/)"
                ),
            );
        }
    }
}

// ---------- rust ----------

fn lint_rust(root: &Path, f: &mut Findings) {
    let cargo = cargo();
    let checks: [(&str, &[&str]); 2] = [
        ("cargo fmt --check (fix: cargo fmt)", &["fmt", "--check"]),
        (
            "cargo clippy -D warnings",
            &["clippy", "--all-targets", "--quiet", "--", "-D", "warnings"],
        ),
    ];
    for (label, args) in checks {
        match run(root, &cargo, args) {
            Ok((true, _)) => {}
            Ok((false, out)) => f.error("rust", format!("{label} failed:\n{}", out.trim())),
            Err(e) => f.error("rust", format!("{label}: {e}")),
        }
    }
    // Formal verification of the `verus!` code, when Verus is installed.
    match run(root, &cargo, &["verus", "--help"]) {
        Ok((true, _)) => match run(root, &cargo, &["verus", "verify"]) {
            Ok((true, _)) => {}
            Ok((false, out)) => {
                let lines: Vec<&str> = out.lines().collect();
                let tail = lines[lines.len().saturating_sub(30)..].join("\n");
                f.error("rust", format!("cargo verus verify failed:\n{tail}"));
            }
            Err(e) => f.error("rust", format!("cargo verus verify: {e}")),
        },
        _ => f.warn(
            "rust",
            "cargo-verus is not installed, formal verification skipped (see agents/guides/verus.md)",
        ),
    }
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn messages(f: &Findings) -> String {
        f.items
            .iter()
            .map(|(_, _, m)| m.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn kebab_case() {
        assert!(is_kebab("best-practices"));
        assert!(is_kebab("a1"));
        assert!(!is_kebab("Best"));
        assert!(!is_kebab("snake_case"));
        assert!(!is_kebab("double--dash"));
        assert!(!is_kebab(""));
    }

    #[test]
    fn frontmatter_is_parsed() {
        let fm = frontmatter("---\nname: demo\ndescription: \"Does X\"\n---\nbody").unwrap();
        assert_eq!(
            fm,
            vec![
                ("name".into(), "demo".into()),
                ("description".into(), "Does X".into())
            ]
        );
        assert!(frontmatter("no frontmatter").is_none());
    }

    #[test]
    fn module_frontmatter_must_match_its_name() {
        let mut f = Findings::default();
        check_frontmatter(
            &mut f,
            "x",
            "---\nname: other\ndescription: d\n---\n",
            "demo",
        );
        check_frontmatter(&mut f, "y", "---\nname: demo\n---\n", "demo");
        check_frontmatter(
            &mut f,
            "z",
            "---\nname: demo\ndescription: d\n---\n",
            "demo",
        );
        assert_eq!(
            messages(&f),
            "x: name `other` must be `demo`\ny: frontmatter has no `description`"
        );
    }

    #[test]
    fn connectors_need_the_neutral_format_and_no_literal_secrets() {
        let mut f = Findings::default();
        lint_connector(
            &mut f,
            "ok",
            "gh.json",
            r#"{"name":"gh","transport":"stdio","command":"gh-mcp","env":{"TOKEN":"${GH_TOKEN}"}}"#,
        );
        lint_connector(
            &mut f,
            "a",
            "web.json",
            r#"{"name":"other","transport":"http"}"#,
        );
        lint_connector(
            &mut f,
            "b",
            "x.json",
            r#"{"name":"x","transport":"ftp","env":{"KEY":"abc123"}}"#,
        );
        lint_connector(&mut f, "c", "y.json", "not json");
        let all = messages(&f);
        assert!(!all.contains("ok:"), "{all}");
        for expected in [
            "a: name `other` must be `web`",
            "a: http transport needs `url`",
            "b: transport `ftp` must be `stdio` or `http`",
            "b: env `KEY` must be a `${VAR}` reference",
            "c: invalid json",
        ] {
            assert!(all.contains(expected), "missing `{expected}` in:\n{all}");
        }
    }

    #[test]
    fn memory_lint_finds_each_kind_of_problem() {
        // Assembled at runtime so pla-audit never sees a home path in this file.
        let expanded = ["/", "Users", "/someone/plalast"].concat();
        let cfg = json!({
            "params": {
                "config": {"PLALAST_DIR": expanded},
                "levers": {"doc": {"Bad_Id": "maybe", "ghost": "enabled"}}
            },
            "primary": {
                "docs": {"extra": 1},
                "schema": {"keys": {"primary.docs": "", "primary.docs.definition": "", "primary.schema": "", "data.structure": "", "params.config": ""},
                           "levers": {"enabled": ""}, "blocks": {"paragraph": ""}}
            },
            "data": {
                "structure": [
                    {"path": "a.md", "type": "file", "build": "doc", "description": "A"},
                    {"path": "a.md", "type": "file", "build": "doc"},
                    {"path": "dir", "type": "dir", "description": "D"},
                    {"path_ref": "params.nowhere", "type": "dir", "description": "R"}
                ],
                "content": {"doc": {"head": [{"type": "nope"}, {"type": "list", "ref": "primary.nowhere"}], "sections": [
                    {"id": "Bad_Id", "title": "T", "blocks": []}
                ]}}
            }
        });
        let mut f = Findings::default();
        lint_memory(Path::new("/nonexistent"), &cfg, &mut f);
        let all = messages(&f);
        for expected in [
            "data.content is not documented",
            "params.levers is not documented",
            "`a.md`: duplicate path",
            "`a.md`: missing description",
            "`dir`: only folders end with `/`",
            "path_ref `params.nowhere` does not resolve",
            "block type `nope`",
            "id must be kebab-case",
            "missing `when`",
            "lever `maybe` is not in primary.schema.levers",
            "params.levers.doc.ghost: no such section",
            "primary.docs.extra is not documented",
            "primary.log must be a list",
            "primary.index must be an object",
            "primary.modules is missing",
            "params.config.repository is missing",
            "params.config.paths.docs is empty",
            "must be `$PWD`",
            "data.content.doc.head: the second block must be `agent_note`",
            "list block ref `primary.nowhere` does not point",
        ] {
            assert!(all.contains(expected), "missing `{expected}` in:\n{all}");
        }
    }
}
