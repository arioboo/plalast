//! lint: checks that the repo is well formed.
//!
//! - `pla-lint`                         every area.
//! - `pla-lint [config] [modules] [rust]`  only the given areas.
//!
//! `config`: config.json is consistent and fully documented in `primary.schema`.
//! `modules`: hand-written modules in `agents/` subfolders have the expected shape.
//! `rust`: `cargo fmt --check` and `cargo clippy -D warnings` pass.

use plalast::{
    arr, cargo, data, doc_dirs, docs, exit, is_doc_path, lever, lookup, module_dir, open, primary,
    run, str_of, Findings, CONFIG, MODULE_KINDS, PRIMARY_KEYS,
};
use serde_json::Value;
use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

const AREAS: [&str; 3] = ["config", "modules", "rust"];
const ENTRY_TYPES: [&str; 5] = ["dir", "symlink", "file", "ignored", "planned"];

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    exit((|| {
        if let Some(bad) = args.iter().find(|a| !AREAS.contains(&a.as_str())) {
            return Err(format!(
                "unknown area {bad}\nusage: pla-lint [config] [modules] [rust]"
            ));
        }
        let want = |area: &str| args.is_empty() || args.iter().any(|a| a == area);
        let (root, cfg) = open()?;
        let mut f = Findings::default();
        if want("config") {
            lint_config(&root, &cfg, &mut f);
        }
        if want("modules") {
            lint_modules(&root, &cfg, &mut f);
            lint_guides(&root, &cfg, &mut f);
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

// ---------- config ----------

fn lint_config(root: &Path, cfg: &Value, f: &mut Findings) {
    const A: &str = "config";
    let schema = &primary(cfg)["schema"];

    // Every key of primary and data is documented.
    for (top, obj) in [("primary", primary(cfg)), ("data", data(cfg))] {
        for k in obj.as_object().into_iter().flat_map(|m| m.keys()) {
            let full = format!("{top}.{k}");
            if schema["keys"].get(&full).is_none() {
                f.error(
                    A,
                    format!("{full} is not documented in primary.schema.keys"),
                );
            }
            // A key whose children are documented (such as primary.docs) needs all of them documented.
            let prefix = format!("{full}.");
            let documents_children = schema["keys"]
                .as_object()
                .is_some_and(|m| m.keys().any(|d| d.starts_with(&prefix)));
            if documents_children {
                for sub in obj[k].as_object().into_iter().flat_map(|m| m.keys()) {
                    if schema["keys"].get(format!("{prefix}{sub}")).is_none() {
                        f.error(
                            A,
                            format!("{prefix}{sub} is not documented in primary.schema.keys"),
                        );
                    }
                }
            }
        }
    }

    // Structure entries.
    let structure = arr(data(cfg), "structure");
    let mut seen: Vec<&str> = Vec::new();
    for e in structure {
        let Some(path) = str_of(e, "path") else {
            f.error(A, "data.structure: an entry has no `path`");
            continue;
        };
        let ctx = format!("data.structure `{path}`");
        if seen.contains(&path) {
            f.error(A, format!("{ctx}: duplicate path"));
        }
        seen.push(path);
        if blank(str_of(e, "description")) {
            f.error(A, format!("{ctx}: missing description"));
        }
        let ty = str_of(e, "type").unwrap_or("");
        if !ENTRY_TYPES.contains(&ty) {
            f.error(A, format!("{ctx}: unknown type `{ty}`"));
            continue;
        }
        if ty == "dir" && !path.ends_with('/')
            || matches!(ty, "file" | "symlink") && path.ends_with('/')
        {
            f.error(A, format!("{ctx}: only folders end with `/`"));
        }
        match ty {
            "symlink" => match (str_of(e, "target"), str_of(e, "target_ref")) {
                (Some(_), _) => {}
                (None, Some(r)) if lookup(cfg, r).and_then(Value::as_str).is_none() => f.error(
                    A,
                    format!("{ctx}: target_ref `{r}` is not a string in the config"),
                ),
                (None, Some(_)) => {}
                (None, None) => f.error(
                    A,
                    format!("{ctx}: symlink without `target` or `target_ref`"),
                ),
            },
            "file" => match str_of(e, "build") {
                Some("gitignore") => {}
                Some(key) if !data(cfg)["content"][key].is_object() => {
                    f.error(A, format!("{ctx}: data.content.{key} does not exist"))
                }
                Some(_) => {}
                None => {
                    if !root.join(path).exists() {
                        f.error(A, format!("{ctx}: hand-written file is missing on disk"));
                    }
                    if is_doc_path(cfg, path) {
                        f.error(A, format!("{ctx}: it is a doc (an .md in primary.docs.dirs), so it must be generated (set `build`)"));
                    }
                }
            },
            _ => {}
        }
    }

    // Documents, sections and levers.
    let built: Vec<&str> = structure
        .iter()
        .filter_map(|e| str_of(e, "build"))
        .collect();
    let mut ids: Vec<String> = Vec::new();
    for (key, doc) in data(cfg)["content"].as_object().into_iter().flatten() {
        if !built.contains(&key.as_str()) {
            f.error(
                A,
                format!("data.content.{key}: no data.structure entry builds it"),
            );
        }
        check_blocks(
            schema,
            arr(doc, "head"),
            &format!("data.content.{key}.head"),
            f,
        );
        for s in arr(doc, "sections") {
            let id = str_of(s, "id").unwrap_or("");
            let ctx = format!("data.content.{key} section `{id}`");
            if !is_kebab(id) {
                f.error(A, format!("{ctx}: id must be kebab-case"));
            }
            if ids.iter().any(|x| x == id) {
                f.error(
                    A,
                    format!("{ctx}: id already used elsewhere (on_demand files would collide)"),
                );
            }
            ids.push(id.to_string());
            for k in ["title", "when"] {
                if blank(str_of(s, k)) {
                    f.error(A, format!("{ctx}: missing `{k}`"));
                }
            }
            match lever(cfg, key, id) {
                None => f.error(A, format!("{ctx}: no lever in primary.docs.levers.{key}")),
                Some(v) if schema["levers"].get(v).is_none() => f.error(
                    A,
                    format!("{ctx}: lever `{v}` is not in primary.schema.levers"),
                ),
                Some(_) => {}
            }
            check_blocks(schema, arr(s, "blocks"), &ctx, f);
        }
    }
    for (key, m) in docs(cfg)["levers"].as_object().into_iter().flatten() {
        for id in m.as_object().into_iter().flat_map(|m| m.keys()) {
            let exists = arr(&data(cfg)["content"][key], "sections")
                .iter()
                .any(|s| str_of(s, "id") == Some(id));
            if !exists {
                f.error(
                    A,
                    format!("primary.docs.levers.{key}.{id}: no such section"),
                );
            }
        }
    }

    // Module kinds: a definition, a folder that exists, and link folders that exist.
    for kind in MODULE_KINDS {
        let m = &primary(cfg)[kind];
        if !m.is_object() {
            f.error(A, format!("primary.{kind} is missing"));
            continue;
        }
        if blank(str_of(m, "definition")) {
            f.error(A, format!("primary.{kind}: missing `definition`"));
        }
        match module_dir(cfg, kind) {
            Some(d) if root.join(&d).is_dir() => {}
            Some(d) => f.error(A, format!("primary.{kind}.dir: `{d}` is not a folder")),
            None => f.error(A, format!("primary.{kind}: missing `dir`")),
        }
        for (p, dest) in m["links"].as_object().into_iter().flatten() {
            match dest.as_str() {
                Some(d) if root.join(d).is_dir() => {}
                Some(d) => f.error(
                    A,
                    format!("primary.{kind}.links.{p}: `{d}` is not a folder"),
                ),
                None => f.error(A, format!("primary.{kind}.links.{p}: not a path")),
            }
        }
    }
    // The zettelkasten path is relative (an absolute one leaks the home folder).
    match str_of(&primary(cfg)["zettelkasten"], "path") {
        None => f.error(A, "primary.zettelkasten: missing `path`"),
        Some(p) if Path::new(p).is_absolute() => f.error(
            A,
            format!("primary.zettelkasten.path: `{p}` must be relative to the repo root"),
        ),
        Some(p) if !root.join(p).is_dir() => f.warn(
            A,
            format!("primary.zettelkasten.path: `{p}` is not a folder on this machine"),
        ),
        Some(_) => {}
    }
    // Features: feature_<n> -> {descr, status}, both strings.
    for (k, v) in primary(cfg)["features"].as_object().into_iter().flatten() {
        let numbered = k
            .strip_prefix("feature_")
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        if !numbered {
            f.error(A, format!("primary.features.{k}: keys are feature_<n>"));
        }
        for field in ["descr", "status"] {
            if !v[field].is_string() {
                f.error(
                    A,
                    format!("primary.features.{k}: `{field}` must be a string"),
                );
            }
        }
    }
    // Wiring: primary holds exactly the keys the tools handle (lib PRIMARY_KEYS), in that order.
    let known: Vec<&str> = PRIMARY_KEYS.iter().map(|(k, _)| *k).collect();
    let present: Vec<&str> = primary(cfg)
        .as_object()
        .map(|m| m.keys().map(String::as_str).collect())
        .unwrap_or_default();
    for k in &present {
        if !known.contains(k) {
            f.error(A, format!("primary.{k}: no tool handles it; add it to PRIMARY_KEYS in src/lib.rs and wire it"));
        }
    }
    for k in &known {
        if !present.contains(k) {
            f.error(A, format!("primary.{k} is missing from {CONFIG}"));
        }
    }
    // Shapes of the keys that hold plain data.
    if !primary(cfg)["log"].is_array() {
        f.error(A, "primary.log must be a list of entries");
    }
    if !primary(cfg)["index"].is_object() {
        f.error(A, "primary.index must be an object");
    }
    let audit = &primary(cfg)["config"]["audit"];
    if !primary(cfg)["config"].is_object() || !audit.is_object() {
        f.error(A, "primary.config.audit is missing (settings of pla-audit)");
    } else {
        let allow_ok = audit["allow"]
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string));
        if !allow_ok {
            f.error(A, "primary.config.audit.allow must be a list of strings");
        }
        if !audit["max_file_kb"].is_u64() {
            f.error(A, "primary.config.audit.max_file_kb must be a whole number");
        }
    }
    if doc_dirs(cfg).is_empty() {
        f.error(A, "primary.docs.dirs is empty: no folder holds the docs");
    }
    for d in doc_dirs(cfg) {
        if !root.join(&d).is_dir() {
            f.error(A, format!("primary.docs.dirs: `{d}` is not a folder"));
        }
    }
    if let Some(logo) = primary(cfg)["project"]["plalast_logo"].as_str() {
        if !root.join(logo).is_file() {
            f.error(
                A,
                format!("primary.project.plalast_logo: `{logo}` does not exist"),
            );
        }
    }
}

fn check_blocks(schema: &Value, blocks: &[Value], ctx: &str, f: &mut Findings) {
    for b in blocks {
        let t = str_of(b, "type").unwrap_or("");
        if schema["blocks"].get(t).is_none() {
            f.error(
                "config",
                format!("{ctx}: block type `{t}` is not documented in primary.schema.blocks"),
            );
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

/// Visible entries of a folder (hidden ones such as `.gitkeep` are skipped), sorted.
fn entries(dir: &Path) -> Vec<String> {
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

fn lint_modules(root: &Path, cfg: &Value, f: &mut Findings) {
    const A: &str = "modules";
    // Skills: agents/skills/<name>/SKILL.md, name in kebab-case and in the frontmatter.
    let skills_dir = module_dir(cfg, "skills").unwrap_or_default();
    let skills = root.join(&skills_dir);
    for name in entries(&skills) {
        let dir = skills.join(&name);
        let rel = format!("{skills_dir}/{name}");
        if !dir.is_dir() {
            f.error(A, format!("{rel}: a skill is a folder"));
            continue;
        }
        if !is_kebab(&name) {
            f.error(A, format!("{rel}: folder name must be kebab-case"));
        }
        // Exact case: the folder listing must contain `SKILL.md` itself.
        if !entries(&dir).iter().any(|n| n == "SKILL.md") {
            f.error(A, format!("{rel}: missing SKILL.md (exact uppercase name)"));
            continue;
        }
        match fs::read_to_string(dir.join("SKILL.md")) {
            Ok(text) => check_frontmatter(f, &format!("{rel}/SKILL.md"), &text, &name),
            Err(e) => f.error(A, format!("{rel}/SKILL.md: {e}")),
        }
    }
    // Subagents: agents/subagents/<name>.md with frontmatter name == file name.
    let agents_dir = module_dir(cfg, "agents").unwrap_or_default();
    let subagents = root.join(&agents_dir);
    for name in entries(&subagents) {
        let rel = format!("{agents_dir}/{name}");
        let Some(stem) = name.strip_suffix(".md") else {
            f.error(A, format!("{rel}: a subagent is one .md file"));
            continue;
        };
        if !is_kebab(stem) {
            f.error(A, format!("{rel}: file name must be kebab-case"));
        }
        match fs::read_to_string(subagents.join(&name)) {
            Ok(text) => check_frontmatter(f, &rel, &text, stem),
            Err(e) => f.error(A, format!("{rel}: {e}")),
        }
    }
    // Rules: agents/rules/<name>.md, not empty.
    let rules_dir = module_dir(cfg, "rules").unwrap_or_default();
    let rules = root.join(&rules_dir);
    for name in entries(&rules) {
        let rel = format!("{rules_dir}/{name}");
        if !name.ends_with(".md") {
            f.error(A, format!("{rel}: a rule is one .md file"));
            continue;
        }
        if fs::read_to_string(rules.join(&name)).map_or(true, |t| t.trim().is_empty()) {
            f.error(A, format!("{rel}: empty or unreadable"));
        }
    }
    // Connectors: one MCP server per json file in the neutral format.
    let mcp_dir = module_dir(cfg, "connectors").unwrap_or_default();
    for name in entries(&root.join(&mcp_dir)) {
        let rel = format!("{mcp_dir}/{name}");
        match fs::read_to_string(root.join(&mcp_dir).join(&name)) {
            Ok(text) => lint_connector(f, &rel, &name, &text),
            Err(e) => f.error(A, format!("{rel}: {e}")),
        }
    }
}

/// Guides: hand-written `.md` sources, each with the `.pdf` that pla-build renders from it.
fn lint_guides(root: &Path, cfg: &Value, f: &mut Findings) {
    const A: &str = "modules";
    let dir = module_dir(cfg, "guides").unwrap_or_default();
    let names = entries(&root.join(&dir));
    for name in &names {
        let rel = format!("{dir}/{name}");
        if let Some(stem) = name.strip_suffix(".pdf") {
            if !names.contains(&format!("{stem}.md")) {
                f.error(A, format!("{rel}: no {stem}.md to build it from (pla-build renders guides from their .md)"));
            }
        } else if name.ends_with(".md") {
            if fs::read_to_string(root.join(&dir).join(name)).map_or(true, |t| t.trim().is_empty())
            {
                f.error(A, format!("{rel}: empty or unreadable"));
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
            f.error(A, format!("{rel}: stdio transport needs `command`"))
        }
        Some("http") if str_of(&v, "url").is_none() => {
            f.error(A, format!("{rel}: http transport needs `url`"))
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
        let msgs: Vec<&str> = f.items.iter().map(|(_, _, m)| m.as_str()).collect();
        assert_eq!(
            msgs,
            vec![
                "x: name `other` must be `demo`",
                "y: frontmatter has no `description`"
            ]
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
        let all = f
            .items
            .iter()
            .map(|(_, _, m)| m.as_str())
            .collect::<Vec<_>>()
            .join("\n");
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
    fn config_lint_finds_each_kind_of_problem() {
        let cfg = json!({
            "primary": {
                "docs": {"levers": {"doc": {"Bad_Id": "maybe", "ghost": "enabled"}}, "extra": 1},
                "schema": {"keys": {"primary.docs": "", "primary.docs.levers": "", "primary.schema": "", "data.structure": ""},
                           "levers": {"enabled": ""}, "blocks": {"paragraph": ""}}
            },
            "data": {
                "structure": [
                    {"path": "a.md", "type": "file", "build": "doc", "description": "A"},
                    {"path": "a.md", "type": "file", "build": "doc"},
                    {"path": "dir", "type": "dir", "description": "D"}
                ],
                "content": {"doc": {"head": [{"type": "nope"}], "sections": [
                    {"id": "Bad_Id", "title": "T", "blocks": []}
                ]}}
            }
        });
        let mut f = Findings::default();
        lint_config(Path::new("/nonexistent"), &cfg, &mut f);
        let all = f
            .items
            .iter()
            .map(|(_, _, m)| m.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        for expected in [
            "data.content is not documented",
            "`a.md`: duplicate path",
            "`a.md`: missing description",
            "`dir`: only folders end with `/`",
            "block type `nope`",
            "id must be kebab-case",
            "missing `when`",
            "lever `maybe` is not in primary.schema.levers",
            "primary.docs.levers.doc.ghost: no such section",
            "primary.docs.extra is not documented",
            "primary.log must be a list",
            "primary.index must be an object",
            "primary.zettelkasten is missing",
            "primary.docs.dirs is empty",
        ] {
            assert!(all.contains(expected), "missing `{expected}` in:\n{all}");
        }
    }
}
