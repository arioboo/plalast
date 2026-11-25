//! lint: checks that the repo is well formed.
//!
//! - `pla-lint`                         every area.
//! - `pla-lint [config] [modules] [rust]`  only the given areas.
//!
//! `config`: config.json is consistent and fully documented in `primary.schema`.
//! `modules`: hand-written modules in `agents/` subfolders have the expected shape.
//! `rust`: `cargo fmt --check` and `cargo clippy -D warnings` pass.

use plalast::{arr, cargo, data, exit, lever, open, primary, run, str_of, Findings};
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
            lint_modules(&root, &mut f);
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
            "symlink" if str_of(e, "target").is_none() => {
                f.error(A, format!("{ctx}: symlink without `target`"))
            }
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
                    let top_md = path.ends_with(".md")
                        && (!path.contains('/')
                            || path.starts_with("agents/") && path.matches('/').count() == 1);
                    if top_md {
                        f.error(A, format!("{ctx}: an .md in the root or agents/ must be generated (set `build`)"));
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
                None => f.error(A, format!("{ctx}: no lever in primary.levers.{key}")),
                Some(v) if schema["levers"].get(v).is_none() => f.error(
                    A,
                    format!("{ctx}: lever `{v}` is not in primary.schema.levers"),
                ),
                Some(_) => {}
            }
            check_blocks(schema, arr(s, "blocks"), &ctx, f);
        }
    }
    for (key, m) in primary(cfg)["levers"].as_object().into_iter().flatten() {
        for id in m.as_object().into_iter().flat_map(|m| m.keys()) {
            let exists = arr(&data(cfg)["content"][key], "sections")
                .iter()
                .any(|s| str_of(s, "id") == Some(id));
            if !exists {
                f.error(A, format!("primary.levers.{key}.{id}: no such section"));
            }
        }
    }

    // Adapters and project paths point to real places.
    for (p, maps) in primary(cfg)["adapters"].as_object().into_iter().flatten() {
        for m in maps.as_array().into_iter().flatten() {
            for k in ["source", "dest"] {
                match str_of(m, k) {
                    Some(d) if root.join(d).is_dir() => {}
                    Some(d) => f.error(
                        A,
                        format!("primary.adapters.{p}: {k} `{d}` is not a folder"),
                    ),
                    None => f.error(A, format!("primary.adapters.{p}: missing `{k}`")),
                }
            }
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

fn lint_modules(root: &Path, f: &mut Findings) {
    const A: &str = "modules";
    // Skills: agents/skills/<name>/SKILL.md, name in kebab-case and in the frontmatter.
    let skills = root.join("agents/skills");
    for name in entries(&skills) {
        let dir = skills.join(&name);
        let rel = format!("agents/skills/{name}");
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
    let subagents = root.join("agents/subagents");
    for name in entries(&subagents) {
        let rel = format!("agents/subagents/{name}");
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
    let rules = root.join("agents/rules");
    for name in entries(&rules) {
        let rel = format!("agents/rules/{name}");
        if !name.ends_with(".md") {
            f.error(A, format!("{rel}: a rule is one .md file"));
            continue;
        }
        if fs::read_to_string(rules.join(&name)).map_or(true, |t| t.trim().is_empty()) {
            f.error(A, format!("{rel}: empty or unreadable"));
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
    fn config_lint_finds_each_kind_of_problem() {
        let cfg = json!({
            "primary": {
                "levers": {"doc": {"Bad_Id": "maybe", "ghost": "enabled"}},
                "schema": {"keys": {"primary.levers": "", "primary.schema": "", "data.structure": ""},
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
            "primary.levers.doc.ghost: no such section",
        ] {
            assert!(all.contains(expected), "missing `{expected}` in:\n{all}");
        }
    }
}
