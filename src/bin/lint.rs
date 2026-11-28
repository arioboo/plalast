//! lint: checks that the repo is well formed.
//!
//! - `pla-lint`                            every area.
//! - `pla-lint [memory] [modules] [rust]`  only the given areas.
//!
//! `memory`: memory.json and params.yaml are consistent and fully documented in `primary.schema`.
//! `modules`: hand-written modules in `agents/` subfolders have the expected shape.
//! `rust`: `cargo fmt --check` and `cargo clippy -D warnings` pass (formal verification is in `pla-verify`).

use plalast::{
    arr, block_items, block_text, cargo, config, data, doc_dirs, entry_path, exit,
    external_modules, is_doc_path, lever_set, lookup, modes, module_dir, open, paths, primary, run,
    str_of, visible_entries, Findings, BRAIN_KEYS, CHANGE_FIELDS, CHANGE_TYPES, CONFIG_KEYS,
    DESCR_KEYS, INTERRUPTORS_MARK, LOG_FIELDS, LOG_LEVELS, LOG_STATUS, MEMORY, MODULE_KINDS,
    PARAMS, PRIMARY_KEYS, SPECIAL_BUILDS,
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
    lint_brain_keys(cfg, f);
    lint_migrate_up(cfg, f);
    lint_registries(cfg, f);
    lint_structure(root, cfg, f);
    lint_docs(cfg, f);
    lint_modes(cfg, f);
    lint_interruptors(root, cfg, f);
    lint_procedures(cfg, f);
    lint_problems(cfg, f);
    lint_fantastic(cfg, f);
    lint_texts(root, cfg, f);
    lint_modules_and_paths(root, cfg, f);
    lint_params(root, cfg, f);
    lint_plain_data(root, cfg, f);
}

/// Every key of params, config, primary and data is documented in primary.schema.keys;
/// a key with documented children has all of them documented.
fn lint_schema_coverage(cfg: &Value, f: &mut Findings) {
    let documented = &primary(cfg)["schema"]["keys"];
    // Every field says what it is (`descr_+`) and is not (`descr_-`), Pareto-short.
    for (k, v) in documented.as_object().into_iter().flatten() {
        for d in DESCR_KEYS {
            if blank(str_of(v, d)) {
                f.error(M, format!("primary.schema.keys.{k}: missing `{d}`; every field says what it is (descr_+) and is not (descr_-)"));
            }
        }
    }
    for (top, obj) in [
        ("params", &cfg["params"]),
        ("config", config(cfg)),
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

/// Every primary key of the memory says what it is (`descr_+`) and is not (`descr_-`).
fn lint_brain_keys(cfg: &Value, f: &mut Findings) {
    for key in BRAIN_KEYS {
        for d in DESCR_KEYS {
            if blank(str_of(&cfg[key], d)) {
                f.error(M, format!("{key}.{d}: missing; every primary key says what it is (descr_+) and is not (descr_-)"));
            }
        }
    }
}

/// "Only migrate UP!": `data` is external memory, nearly empty. It holds only its `descr_+` and
/// `descr_-` and fields the schema marks with `migrates_to` (`config` or `primary`). The structure
/// keeps tool settings in `config.structure` and its prose in `primary.structure`, path by path.
fn lint_migrate_up(cfg: &Value, f: &mut Findings) {
    let schema = &primary(cfg)["schema"]["keys"];
    for k in keys(data(cfg)) {
        if DESCR_KEYS.contains(&k) {
            continue;
        }
        let target = str_of(&schema[format!("data.{k}")], "migrates_to");
        if !matches!(target, Some("config" | "primary")) {
            f.error(M, format!("data.{k}: data is external memory; a field there needs `migrates_to` (config or primary) in primary.schema.keys, or it moves up now (Only migrate UP!)"));
        }
    }
    let prose = &primary(cfg)["structure"];
    let mut paths = Vec::new();
    for e in arr(config(cfg), "structure") {
        let path = entry_path(cfg, e).unwrap_or_default();
        for k in ["description", "table", "label"] {
            if e.get(k).is_some() {
                f.error(
                    M,
                    format!(
                        "config.structure `{path}`: `{k}` is prose, it goes in primary.structure"
                    ),
                );
            }
        }
        if prose.get(&path).is_none() {
            f.error(
                M,
                format!("config.structure `{path}`: no primary.structure entry describes it"),
            );
        }
        paths.push(path);
    }
    for p in keys(prose) {
        if !paths.iter().any(|x| x == p) {
            f.error(
                M,
                format!("primary.structure `{p}`: no config.structure entry has this path"),
            );
        }
    }
}

/// primary and config hold exactly the keys the tools handle (lib registries), in order.
fn lint_registries(cfg: &Value, f: &mut Findings) {
    for (name, obj, registry, file) in [
        ("primary", primary(cfg), &PRIMARY_KEYS[..], MEMORY),
        ("config", config(cfg), &CONFIG_KEYS[..], MEMORY),
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

/// A folder with `links_only: true` (the Claude Code adapter) holds nothing but symlinks into
/// `agents/` (static premise "One folder for every assistant").
fn lint_links_only(cfg: &Value, f: &mut Findings) {
    let entries = plalast::structure(cfg);
    for dir in entries
        .iter()
        .filter(|e| e.get("links_only").and_then(Value::as_bool) == Some(true))
    {
        let Some(dir) = entry_path(cfg, dir) else {
            continue;
        };
        let prefix = format!("{}/", dir.trim_end_matches('/'));
        for e in entries {
            let Some(path) = entry_path(cfg, e) else {
                continue;
            };
            if !path.starts_with(&prefix) || path == prefix {
                continue;
            }
            let target = str_of(e, "target").unwrap_or("");
            let resolved = plalast::lexical_target(Path::new(&path), Path::new(target));
            if str_of(e, "type") != Some("symlink") || !resolved.starts_with("agents") {
                f.error(M, format!("config.structure `{path}`: {prefix} holds only symlinks into agents/ (One folder for every assistant)"));
            }
        }
    }
}

/// config.structure: resolvable unique paths, descriptions, known types, and per-type fields.
fn lint_structure(root: &Path, cfg: &Value, f: &mut Findings) {
    lint_links_only(cfg, f);
    let mut seen: Vec<String> = Vec::new();
    for e in plalast::structure(cfg) {
        let Some(path) = entry_path(cfg, e) else {
            let r = str_of(e, "path_ref").unwrap_or("?");
            f.error(M, format!("config.structure: an entry has no `path`, or its path_ref `{r}` does not resolve"));
            continue;
        };
        // Messages show the path as written (path_ref entries show the resolved one).
        let ctx = format!("config.structure `{}`", str_of(e, "path").unwrap_or(&path));
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
                Some(key) if !plalast::content(cfg)[key].is_object() => {
                    f.error(
                        M,
                        format!("{ctx}: primary.docs.content.{key} does not exist"),
                    );
                }
                Some(_) => {}
                None => {
                    if !root.join(&path).exists() {
                        f.error(M, format!("{ctx}: hand-written file is missing on disk"));
                    }
                    if is_doc_path(cfg, &path) {
                        f.error(M, format!("{ctx}: it is a doc (an .md in config.paths.docs), so it must be generated (set `build`)"));
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
    let built: Vec<&str> = plalast::structure(cfg)
        .iter()
        .filter_map(|e| str_of(e, "build"))
        .collect();
    let mut ids: Vec<String> = Vec::new();
    for (key, doc) in plalast::content(cfg).as_object().into_iter().flatten() {
        if !built.contains(&key.as_str()) {
            f.error(
                M,
                format!("primary.docs.content.{key}: no config.structure entry builds it"),
            );
        }
        check_blocks(
            cfg,
            arr(doc, "head"),
            &format!("primary.docs.content.{key}.head"),
            f,
        );
        // `"agent_note": false` opts a doc out of the note (adapter files such as CLAUDE.md).
        let wants_note = doc.get("agent_note").and_then(Value::as_bool) != Some(false);
        let second = arr(doc, "head").get(1).and_then(|b| str_of(b, "type"));
        if wants_note && second != Some("agent_note") {
            f.error(
                M,
                format!("primary.docs.content.{key}.head: the second block must be `agent_note`"),
            );
        }
        for s in arr(doc, "sections") {
            let id = str_of(s, "id").unwrap_or("");
            let ctx = format!("primary.docs.content.{key} section `{id}`");
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
            // An unset interruptor is `on_demand` (DEFAULT_LEVER); only a wrong value is an error.
            match lever_set(cfg, key, id) {
                None => {}
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
}

/// True when `path` names something an interruptor switches: a doc section by its id
/// (`primary.docs.content.<doc>.sections.<id>`), a whole doc (`primary.docs.content.<doc>`) or a mode.
fn switchable(cfg: &Value, path: &str) -> bool {
    let content = plalast::content(cfg);
    match path.split('.').collect::<Vec<_>>()[..] {
        ["primary", "docs", "content", doc, "sections", id] => arr(&content[doc], "sections")
            .iter()
            .any(|s| str_of(s, "id") == Some(id)),
        ["primary", "docs", "content", doc] => content[doc].is_object(),
        ["primary", "mode", name] => primary(cfg)["mode"].get(name).is_some(),
        _ => false,
    }
}

/// Interruptors of `params.yaml`: each one switches a field that exists (a doc section by its id,
/// a whole doc, or a mode) with a known value, and every one of them is written after the
/// `#---INTERRUPTORS---` line, every other param before it.
fn lint_interruptors(root: &Path, cfg: &Value, f: &mut Findings) {
    let schema = &primary(cfg)["schema"];
    let interruptors = cfg["params"]["interruptors"].as_object();
    for (path, v) in interruptors.into_iter().flatten() {
        let ctx = format!("{PARAMS} `{path}`");
        if v.as_str().is_none_or(|v| schema["levers"].get(v).is_none()) {
            f.error(M, format!("{ctx}: `{v}` is not in primary.schema.levers"));
        }
        if !switchable(cfg, path) {
            f.error(M, format!("{ctx}: not a doc section (primary.docs.content.<doc>.sections.<id>), a doc (primary.docs.content.<doc>) or a mode (primary.mode.<name>) of the memory"));
        }
    }
    let Ok(text) = fs::read_to_string(root.join(PARAMS)) else {
        return;
    };
    let mark = text.lines().position(|l| l.trim() == INTERRUPTORS_MARK);
    if mark.is_none() {
        f.error(
            M,
            format!("{PARAMS}: missing the `{INTERRUPTORS_MARK}` line before the interruptors"),
        );
    }
    let key_line = regex::Regex::new(r"^([A-Za-z][^:\s]*):").expect("valid pattern");
    for (n, line) in text.lines().enumerate() {
        let Some(cap) = key_line.captures(line) else {
            continue;
        };
        // A param by level counts as an interruptor when every level holds an interruptor value.
        let by_level = cfg["params"]["by_level"][&cap[1]].as_object();
        let levelled = by_level.is_some_and(|m| {
            m.values().all(|v| {
                v.as_str()
                    .is_some_and(|s| plalast::LEVER_VALUES.contains(&s))
            })
        });
        let is_interruptor = interruptors.is_some_and(|m| m.contains_key(&cap[1])) || levelled;
        let after = mark.is_some_and(|m| n > m);
        if is_interruptor != after {
            let place = if is_interruptor { "after" } else { "before" };
            f.error(
                M,
                format!(
                    "{PARAMS}:{}: `{}` goes {place} the `{INTERRUPTORS_MARK}` line",
                    n + 1,
                    &cap[1]
                ),
            );
        }
    }
}

/// Modes: every mode has a `title`, a `when` and a non-empty `rules` list of strings (its
/// interruptor is checked with the others, in `lint_interruptors`).
fn lint_modes(cfg: &Value, f: &mut Findings) {
    for (name, m, _) in modes(cfg) {
        let ctx = format!("primary.mode.{name}");
        for k in ["title", "when"] {
            if blank(str_of(m, k)) {
                f.error(M, format!("{ctx}: missing `{k}`"));
            }
        }
        let rules = arr(m, "rules");
        if rules.is_empty() || !rules.iter().all(Value::is_string) {
            f.error(
                M,
                format!("{ctx}: `rules` must be a non-empty list of strings"),
            );
        }
    }
}

/// Procedures: a definition, and per procedure a title and paragraphs in human language, with
/// no references to code or to the repo (no backticks, no file names).
fn lint_procedures(cfg: &Value, f: &mut Findings) {
    let root = &primary(cfg)["procedures"];
    if blank(str_of(root, "definition")) {
        f.error(M, "primary.procedures: missing `definition`");
    }
    let file_name =
        regex::Regex::new(r"\b[\w-]+\.(md|json|yaml|yml|rs|toml|sh|py)\b").expect("valid pattern");
    for (id, p) in plalast::procedures(cfg) {
        let ctx = format!("primary.procedures.items.{id}");
        if !is_kebab(id) {
            f.error(M, format!("{ctx}: id must be kebab-case"));
        }
        if blank(str_of(p, "title")) {
            f.error(M, format!("{ctx}: missing `title`"));
        }
        let text = arr(p, "text");
        if text.is_empty() || !text.iter().all(Value::is_string) {
            f.error(
                M,
                format!("{ctx}: `text` must be a non-empty list of paragraphs"),
            );
        }
        for t in text.iter().filter_map(Value::as_str) {
            if t.contains('`') || file_name.is_match(t) {
                f.error(M, format!("{ctx}: a procedure is human language, with no references to code or to the repo: {t}"));
            }
        }
    }
}

/// Problems: each has a title, a statement in human language and `solved_by` names that resolve
/// to exactly one rule of AGENTS.md; and every rule solves at least one problem (the
/// deterministic form of "Lean memory": every rule solves a problem).
fn lint_problems(cfg: &Value, f: &mut Findings) {
    if blank(str_of(&primary(cfg)["problems"], "definition")) {
        f.error(M, "primary.problems: missing `definition`");
    }
    let rules = plalast::fantastic::rules(cfg);
    let file_name =
        regex::Regex::new(r"\b[\w-]+\.(md|json|yaml|yml|rs|toml|sh|py)\b").expect("valid pattern");
    for (id, p) in plalast::problems(cfg) {
        let ctx = format!("primary.problems.items.{id}");
        if !is_kebab(id) {
            f.error(M, format!("{ctx}: id must be kebab-case"));
        }
        for k in ["title", "statement"] {
            if blank(str_of(p, k)) {
                f.error(M, format!("{ctx}: missing `{k}`"));
            }
        }
        let statement = str_of(p, "statement").unwrap_or("");
        if statement.contains('`') || file_name.is_match(statement) {
            f.error(M, format!("{ctx}: a problem is said in human language, with no references to code or to the repo"));
        }
        let names = arr(p, "solved_by");
        if names.is_empty() {
            f.error(M, format!("{ctx}: `solved_by` must name at least one rule"));
        }
        for n in names.iter().filter_map(Value::as_str) {
            if !rules.iter().any(|(_, r)| *r == n) {
                f.error(
                    M,
                    format!("{ctx}: `{n}` is not the name of a rule in primary.agents-md"),
                );
            }
        }
    }
}

/// The 4 fantásticos, one by one, through the battery of `plalast::fantastic::CHECKS`; and the
/// params that list a category that does not exist.
fn lint_fantastic(cfg: &Value, f: &mut Findings) {
    for (kind, p) in plalast::fantastic::unknown_kinds(cfg) {
        f.error(M, format!("config.{kind}: `{p}` is not a category of AGENTS.md (primary.agents-md.<category>)"));
    }
    for (key, check, msg) in plalast::fantastic::check(cfg) {
        f.error(M, format!("primary.agents-md.{key} [{check}]: {msg}"));
    }
}

/// Every string under `v` with its dotted path (list items by index).
fn texts<'a>(v: &'a Value, path: &str, out: &mut Vec<(String, &'a str)>) {
    match v {
        Value::Object(m) => m
            .iter()
            .for_each(|(k, x)| texts(x, &format!("{path}.{k}"), out)),
        Value::Array(a) => a
            .iter()
            .enumerate()
            .for_each(|(i, x)| texts(x, &format!("{path}.{i}"), out)),
        Value::String(s) => out.push((path.to_string(), s)),
        _ => {}
    }
}

/// A sentence reduced to lowercase words, so that punctuation and case do not hide a repeat.
fn normalize(sentence: &str) -> String {
    sentence
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Sentences of a text: split after `.`, `;` or `:` followed by a space.
fn sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    for (i, c) in text.char_indices() {
        if matches!(c, '.' | ';' | ':') && bytes.get(i + 1).is_some_and(u8::is_ascii_whitespace) {
            out.push(&text[start..=i]);
            start = i + 1;
        }
    }
    out.push(&text[start..]);
    out
}

/// The texts of the memory say each thing once and point at things that exist (deterministic
/// checks of "No duplication" and of stale references; the log is history, so it is skipped):
/// - a sentence of 40+ characters repeated in two places (outside the schema, whose rows share
///   their phrasing on purpose);
/// - a backticked path whose first folder exists in the repo but the path does not;
/// - a backticked dotted key (`config.…`, `primary.…`, `data.…`, `params.…`) that does not resolve;
/// - a `--bin <name>` that is not a binary of Cargo.toml.
fn lint_texts(root: &Path, cfg: &Value, f: &mut Findings) {
    let mut all = Vec::new();
    for key in BRAIN_KEYS {
        texts(&cfg[key], key, &mut all);
    }
    let all: Vec<(String, &str)> = all
        .into_iter()
        .filter(|(p, _)| !p.starts_with("primary.log."))
        .collect();

    let mut seen: Vec<(String, String)> = Vec::new();
    // The schema shares its phrasing on purpose, and `solved_by` names repeat rule names by design.
    let prose = |p: &str| !p.starts_with("primary.schema.") && !p.contains(".solved_by.");
    for (path, text) in all.iter().filter(|(p, _)| prose(p)) {
        for s in sentences(text) {
            let n = normalize(s);
            if n.len() < 40 {
                continue;
            }
            match seen.iter().find(|(m, _)| *m == n) {
                Some((_, first)) if first != path => f.error(
                    M,
                    format!(
                        "{path}: repeats a sentence of {first} (No duplication): {}",
                        s.trim()
                    ),
                ),
                Some(_) => {}
                None => seen.push((n, path.clone())),
            }
        }
    }

    let tick = regex::Regex::new(r"`([^`\s]+)`").expect("valid pattern");
    let dotted = regex::Regex::new(r"^(config|primary|data|params)\.[A-Za-z0-9_+.-]+$")
        .expect("valid pattern");
    let file_ext =
        regex::Regex::new(r"\.(md|json|yaml|yml|rs|toml|sh|conf|pdf|png)$").expect("valid pattern");
    let bins = fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();
    let bin_re = regex::Regex::new(r"--bin (pla-[a-z]+)").expect("valid pattern");
    for (path, text) in &all {
        for cap in tick.captures_iter(text) {
            let tok = &cap[1];
            // `../x` and `./x` are link targets relative to a link, not repo paths.
            if tok.contains(['<', '>', '*', '{', '}', '$', '~'])
                || tok.contains("://")
                || tok.starts_with("../")
                || tok.starts_with("./")
            {
                continue;
            }
            let first = tok.split('/').next().unwrap_or("");
            let in_repo = tok.contains('/')
                && !first.is_empty()
                && root.join(first).symlink_metadata().is_ok();
            if in_repo
                && root
                    .join(tok.trim_end_matches('/'))
                    .symlink_metadata()
                    .is_err()
            {
                f.error(M, format!("{path}: `{tok}` does not exist in the repo"));
            }
            if dotted.is_match(tok)
                && !file_ext.is_match(tok)
                && lookup(cfg, tok).is_none()
                && !switchable(cfg, tok)
            {
                f.error(M, format!("{path}: `{tok}` does not resolve in the brain"));
            }
        }
        for cap in bin_re.captures_iter(text) {
            if !bins.contains(&format!("name = \"{}\"", &cap[1])) {
                f.error(
                    M,
                    format!("{path}: `--bin {}` is not a binary in Cargo.toml", &cap[1]),
                );
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
                "table" => lookup(cfg, r).is_some_and(|v| v.is_array() || v.is_object()),
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
        let ctx = format!("config.paths.modules.{kind}");
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
        let ctx = format!("config.paths.modules.{name}");
        let link = plalast::structure(cfg)
            .iter()
            .find(|e| str_of(e, "target_ref") == Some(ctx.as_str()));
        // A `local` link is only built where the module exists, so its absence is expected.
        let local = link.and_then(|e| e.get("local")).and_then(Value::as_bool) == Some(true);
        let link_dir = link
            .and_then(|e| str_of(e, "path"))
            .and_then(|p| Path::new(p).parent())
            // With no link, the path is relative to `modules/`, as its schema key says.
            .map_or_else(|| root.join("modules"), |d| root.join(d));
        match path {
            None => f.error(M, format!("{ctx}: missing (the path of the module)")),
            Some(p) if Path::new(p).is_absolute() => {
                f.error(
                    M,
                    format!("{ctx}: `{p}` must be relative to the folder of its link"),
                );
            }
            Some(p) if !local && !link_dir.join(p).is_dir() => {
                f.warn(M, format!("{ctx}: `{p}` is not a folder on this machine"));
            }
            Some(_) => {}
        }
    }
    if blank(str_of(paths(cfg), "on_demand")) {
        f.error(M, "config.paths: missing `on_demand`");
    }
    if doc_dirs(cfg).is_empty() {
        f.error(M, "config.paths.docs is empty: no folder holds the docs");
    }
    for d in doc_dirs(cfg) {
        if !root.join(&d).is_dir() {
            f.error(M, format!("config.paths.docs: `{d}` is not a folder"));
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

/// config settings: branches, `PLALAST_DIR`, lint budget, hooks and audit.
fn lint_params(root: &Path, cfg: &Value, f: &mut Findings) {
    let c = config(cfg);
    for b in ["integration", "stable"] {
        if blank(str_of(&c["branches"], b)) {
            f.error(M, format!("config.branches: missing `{b}`"));
        }
    }
    // PLALAST_DIR is `$PWD`: the tools resolve it to the repo root they find, on any machine
    // (a written path works on one machine only, and an expanded one leaks the user name).
    match str_of(c, "PLALAST_DIR") {
        Some("$PWD" | "${PWD}") => {}
        Some(d) => f.error(M, format!("config.PLALAST_DIR: `{d}` must be `$PWD`")),
        None => f.error(M, "config: missing `PLALAST_DIR`"),
    }
    // Context budget: the docs an agent loads every session stay under lint.max_lines.
    let lint_cfg = &c["lint"];
    match (
        lint_cfg["max_lines"].as_u64(),
        lint_cfg["always_on"].as_array(),
    ) {
        (Some(max), Some(files)) => {
            let mut total = 0;
            // Docs whose interruptor is `enabled` are read at the start of every session too.
            let enabled: Vec<String> = plalast::structure(cfg)
                .iter()
                .filter_map(|e| Some((entry_path(cfg, e)?, str_of(e, "build")?)))
                .filter(|(_, key)| plalast::doc_lever(cfg, key) == "enabled")
                .map(|(path, _)| path)
                .collect();
            let listed = files.iter().filter_map(Value::as_str).map(String::from);
            for file in listed.chain(enabled) {
                let file = file.as_str();
                match fs::read_to_string(root.join(file)) {
                    Ok(t) => total += t.lines().count() as u64,
                    Err(_) => f.error(M, format!("config.lint.always_on: `{file}` cannot be read")),
                }
            }
            if total > max {
                f.error(M, format!("always-on docs have {total} lines, over config.lint.max_lines ({max}): move sections to on_demand"));
            }
        }
        _ => f.error(
            M,
            "config.lint needs `always_on` (list of docs) and `max_lines` (number)",
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
            format!("config.hooks.dir: `{hooks_dir}` is not a folder"),
        );
    }
    // Hand-written scripts must run; files pla-build generates there (the Claude Code settings) are data.
    let generated: Vec<String> = plalast::structure(cfg)
        .iter()
        .filter(|e| e["build"].is_string())
        .filter_map(|e| entry_path(cfg, e))
        .collect();
    for name in visible_entries(&root.join(hooks_dir)) {
        if generated.contains(&format!("{}/{name}", hooks_dir.trim_end_matches('/'))) {
            continue;
        }
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
                format!("config.hooks.claude: command `{cmd}` points at a missing script"),
            );
        }
    }
    let audit = &c["audit"];
    if audit.is_object() {
        if !audit["allow"]
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string))
        {
            f.error(M, "config.audit.allow must be a list of strings");
        }
        if !audit["max_file_kb"].is_u64() {
            f.error(M, "config.audit.max_file_kb must be a whole number");
        }
    } else {
        f.error(M, "config.audit is missing (settings of pla-audit)");
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
    // primary.changes: one entry per commit, merge or branch, with its `type(scope)` notation.
    let notation = regex::Regex::new(r"^([a-z]+)\(([a-z0-9-]+)\)$").expect("valid pattern");
    match primary(cfg)["changes"].as_array() {
        None => f.error(M, "primary.changes must be a list of entries"),
        Some(entries) => {
            for (i, e) in entries.iter().enumerate() {
                for k in CHANGE_FIELDS {
                    if blank(str_of(e, k)) {
                        f.error(M, format!("primary.changes[{i}]: missing `{k}`"));
                    }
                }
                if let Some(ch) = str_of(e, "change") {
                    let ok = notation
                        .captures(ch)
                        .is_some_and(|c| CHANGE_TYPES.contains(&&c[1]));
                    if !ok {
                        f.error(M, format!("primary.changes[{i}]: change `{ch}` must be `type(scope)` with type one of {CHANGE_TYPES:?}"));
                    }
                }
                if let Some(h) = str_of(e, "commit") {
                    let obj = format!("{h}^{{commit}}");
                    if !matches!(run(root, "git", &["cat-file", "-e", &obj]), Ok((true, _))) {
                        f.error(
                            M,
                            format!("primary.changes[{i}]: commit `{h}` is not in git"),
                        );
                    }
                }
            }
        }
    }
    // primary.log: warnings, errors and issues, each with a level and a status.
    match primary(cfg)["log"].as_array() {
        None => f.error(M, "primary.log must be a list of entries"),
        Some(entries) => {
            for (i, e) in entries.iter().enumerate() {
                for k in LOG_FIELDS {
                    if blank(str_of(e, k)) {
                        f.error(M, format!("primary.log[{i}]: missing `{k}`"));
                    }
                }
                for (k, allowed) in [("level", &LOG_LEVELS[..]), ("status", &LOG_STATUS[..])] {
                    if let Some(v) = str_of(e, k) {
                        if !allowed.contains(&v) {
                            f.error(
                                M,
                                format!("primary.log[{i}]: {k} `{v}` must be one of {allowed:?}"),
                            );
                        }
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
    // Formal verification (`cargo verus verify`) is a level of pla-verify, not of the lint.
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A fixture written with `data.structure` / `data.content`, as `load` joins it: the derived
    /// `structure` view and `primary.docs.content`.
    fn view(mut v: Value) -> Value {
        if let Some(s) = v
            .get_mut("data")
            .and_then(|d| d.as_object_mut())
            .and_then(|d| d.remove("structure"))
        {
            v["structure"] = s;
        }
        if let Some(c) = v
            .get_mut("data")
            .and_then(|d| d.as_object_mut())
            .and_then(|d| d.remove("content"))
        {
            v["primary"]["docs"]["content"] = c;
        }
        v
    }

    #[test]
    fn data_stays_nearly_empty_and_structure_pairs_by_path() {
        let cfg = view(json!({
            "config": {"structure": [
                {"path": "a/", "type": "dir"},
                {"path": "b.md", "type": "file", "description": "prose in config"}
            ]},
            "primary": {
                "structure": {"a/": {"description": "A"}, "ghost/": {"description": "G"}},
                "schema": {"keys": {"data.staged": {"migrates_to": "primary"}}}
            },
            "data": {"descr_+": "x", "descr_-": "y", "staged": 1, "loose": 2}
        }));
        let mut f = Findings::default();
        lint_migrate_up(&cfg, &mut f);
        let all = messages(&f);
        for expected in [
            "data.loose: data is external memory",
            "config.structure `b.md`: `description` is prose",
            "config.structure `b.md`: no primary.structure entry",
            "primary.structure `ghost/`: no config.structure entry",
        ] {
            assert!(all.contains(expected), "missing `{expected}` in:\n{all}");
        }
        assert!(
            !all.contains("data.staged") && !all.contains("`a/`"),
            "{all}"
        );
    }

    #[test]
    fn links_only_folders_hold_only_links_into_agents() {
        let cfg = view(json!({"config": {}, "primary": {}, "data": {"structure": [
            {"path": ".claude/", "type": "dir", "links_only": true},
            {"path": ".claude/CLAUDE.md", "type": "symlink", "target": "../agents/CLAUDE.md"},
            {"path": ".claude/settings.json", "type": "file", "build": "claude_settings"},
            {"path": ".claude/x", "type": "symlink", "target": "../src/lib.rs"}
        ]}}));
        let mut f = Findings::default();
        lint_links_only(&cfg, &mut f);
        let all = messages(&f);
        assert!(all.contains("`.claude/settings.json`"), "{all}");
        assert!(all.contains("`.claude/x`"), "{all}");
        assert!(!all.contains("CLAUDE.md"), "{all}");
    }

    #[test]
    fn texts_lint_finds_repeats_stale_paths_keys_and_bins() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let long = "This sentence is long enough to count as a repeat of another one.";
        let cfg = json!({
            "config": {"a": format!("{long} Then more."), "lint": {}},
            "primary": {
                "x": format!("Other text. {}", long.to_uppercase()),
                "y": "See `src/nowhere.rs`, `src/lib.rs`, `config.lint` and `config.missing`.",
                "z": "Run `cargo run --bin pla-ghost` or `--bin pla-build`.",
                "log": [{"descr": "Old `src/gone.rs`."}],
                "schema": {"keys": {"k": long}}
            },
            "data": {}
        });
        let mut f = Findings::default();
        lint_texts(root, &cfg, &mut f);
        let all = messages(&f);
        for expected in [
            "primary.x: repeats a sentence of config.a",
            "`src/nowhere.rs` does not exist",
            "`config.missing` does not resolve",
            "`--bin pla-ghost` is not a binary",
        ] {
            assert!(all.contains(expected), "missing `{expected}` in:\n{all}");
        }
        for unexpected in [
            "src/lib.rs",
            "config.lint`",
            "gone.rs",
            "pla-build`",
            "primary.schema",
        ] {
            assert!(
                !all.contains(unexpected),
                "unexpected `{unexpected}` in:\n{all}"
            );
        }
    }

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
        let cfg = view(json!({
            "config": {"PLALAST_DIR": expanded},
            "params": {
                "interruptors": {
                    "primary.docs.content.doc.sections.Bad_Id": "maybe",
                    "primary.docs.content.doc.sections.ghost": "enabled"
                }
            },
            "primary": {
                "docs": {"extra": 1},
                "schema": {"keys": {"primary.docs": "", "primary.docs.definition": "", "primary.schema": "", "config.structure": "", "config": ""},
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
        }));
        let mut f = Findings::default();
        lint_memory(Path::new("/nonexistent"), &cfg, &mut f);
        let all = messages(&f);
        for expected in [
            "primary.docs.content is not documented",
            "params.interruptors is not documented",
            "config.descr_+: missing",
            "data.descr_-: missing",
            "`a.md`: duplicate path",
            "`a.md`: missing description",
            "`dir`: only folders end with `/`",
            "path_ref `params.nowhere` does not resolve",
            "block type `nope`",
            "id must be kebab-case",
            "missing `when`",
            "lever `maybe` is not in primary.schema.levers",
            "`primary.docs.content.doc.sections.ghost`: not a doc section",
            "primary.docs.extra is not documented",
            "primary.log must be a list",
            "primary.index must be an object",
            "primary.modules is missing",
            "config.repository is missing",
            "config.paths.docs is empty",
            "must be `$PWD`",
            "primary.docs.content.doc.head: the second block must be `agent_note`",
            "list block ref `primary.nowhere` does not point",
        ] {
            assert!(all.contains(expected), "missing `{expected}` in:\n{all}");
        }
    }
}
