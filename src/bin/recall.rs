//! recall: prints the repo map from the brain (`memory.json` + `params.yaml`), or searches it.
//!
//! - `pla-recall`          the map: docs, module kinds, plain data, settings, paths and levers.
//! - `pla-recall <term>`   everything matching `term`, naming its file, section and lever.
//! - `pla-recall --session`    the session start (Claude Code SessionStart hook): checks the brain
//!   at run time and prints the memory level and what can be recalled on demand.
//! - `pla-recall --show <id>`  prints one on_demand section, mode or doc, to bring it into context.

use plalast::{
    arr, block_items, block_text, config, doc_dirs, docs, entry_path, exit, external_modules, fill,
    lever_set, modes, module_dir, open, paths, primary, str_of, visible_entries, BRAIN_KEYS,
    DEFAULT_LEVER, DESCR_KEYS, LOG_FIELDS, MEMORY, MODE_GROUP, MODULE_KINDS,
};
use serde_json::Value;
use std::env;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    exit(
        open().map(|(root, cfg)| match args.first().map(String::as_str) {
            Some("--session") => session(&root, &cfg),
            Some("--show") => show(&root, &cfg, &args[1..].join(" ")),
            _ => recall(&root, &cfg, &args.join(" ")),
        }),
    )
}

/// What can be brought into context on demand: `(id, file, title, when)` for every `on_demand`
/// section and mode, and every generated doc that is not always in context.
fn on_demand(cfg: &Value) -> Vec<(String, String, String, String)> {
    let dir = str_of(paths(cfg), "on_demand")
        .unwrap_or("agents")
        .trim_end_matches('/');
    let always: Vec<String> = plalast::strings(&config(cfg)["lint"], "always_on");
    let mut out = Vec::new();
    for (path, key) in documents(cfg) {
        for s in arr(&plalast::content(cfg)[key], "sections") {
            let id = str_of(s, "id").unwrap_or("?");
            if plalast::lever(cfg, key, id) == "on_demand" {
                let title = str_of(s, "title").unwrap_or(id).to_string();
                let when = str_of(s, "when").unwrap_or("").to_string();
                out.push((id.to_string(), format!("{dir}/{id}.md"), title, when));
            }
        }
        if !always.contains(&path) && plalast::doc_lever(cfg, key) == "on_demand" {
            out.push((key.to_string(), path.clone(), path, String::new()));
        }
    }
    for (name, m, state) in modes(cfg) {
        if state == "on_demand" {
            let file = plalast::mode_path(cfg, name).unwrap_or_default();
            let title = str_of(m, "title").unwrap_or(name).to_string();
            let when = str_of(m, "when").unwrap_or("").to_string();
            out.push((name.to_string(), file, title, when));
        }
    }
    out
}

/// The session start: the brain checked at run time (`pla-build --check`), the memory level, what
/// is always in context and what comes only through `pla-recall --show <id>`. Never fails the
/// session: a drift is reported so the agent rebuilds before working.
fn session(root: &Path, cfg: &Value) -> bool {
    let build = env::current_exe()
        .map(|me| me.with_file_name("pla-build"))
        .unwrap_or_default();
    let check = std::process::Command::new(&build)
        .arg("--check")
        .current_dir(root)
        .output();
    match check {
        Ok(o) if o.status.success() => println!("brain: in sync with {MEMORY} and params.yaml"),
        Ok(o) => {
            println!(
                "brain: OUT OF SYNC; run `cargo run --release --bin pla-build` before working:"
            );
            print!("{}", String::from_utf8_lossy(&o.stdout));
        }
        Err(e) => println!("brain: not checked ({}: {e})", build.display()),
    }
    let level = str_of(config(cfg), "mem_level").unwrap_or("mid");
    println!("memory level: {level} (config.mem_level in params.yaml: low, mid or hard)");
    let subagents: Vec<String> =
        visible_entries(&root.join(module_dir(cfg, "agents").unwrap_or_default()))
            .iter()
            .filter_map(|n| n.strip_suffix(".md").map(String::from))
            .collect();
    match config(cfg)["master_arch"].as_bool() {
        Some(true) => println!(
            "master architecture: on (config.master_arch); orchestrate, delegating each task to its subagent: {}",
            subagents.join(", ")
        ),
        _ => println!("master architecture: off (config.master_arch); work alone"),
    }
    let mut always = plalast::strings(&config(cfg)["lint"], "always_on");
    always.extend(
        documents(cfg)
            .into_iter()
            .filter(|(_, key)| plalast::doc_lever(cfg, key) == "enabled")
            .map(|(path, _)| path),
    );
    println!("always in context: {}", always.join(", "));
    println!("on demand (bring one into context with `pla-recall --show <id>`, search with `pla-recall <term>`):");
    for (id, file, title, when) in on_demand(cfg) {
        let when = if when.is_empty() {
            String::new()
        } else {
            format!(": {when}")
        };
        println!("- {id} ({file}) {title}{when}");
    }
    true
}

/// Prints the file of one on_demand item (section id, mode name, doc key or doc path).
fn show(root: &Path, cfg: &Value, id: &str) -> bool {
    let id = id.trim();
    let Some((_, file, _, _)) = on_demand(cfg)
        .into_iter()
        .find(|(i, f, _, _)| i == id || f == id)
    else {
        println!("`{id}` is not on demand; `pla-recall --session` lists what is");
        return false;
    };
    match std::fs::read_to_string(root.join(&file)) {
        Ok(text) => {
            print!("{text}");
            true
        }
        Err(e) => {
            println!("{file}: {e} (run pla-build)");
            false
        }
    }
}

/// `(path, content key)` of every generated document, in structure order.
fn documents(cfg: &Value) -> Vec<(String, &str)> {
    plalast::structure(cfg)
        .iter()
        .filter_map(|e| Some((entry_path(cfg, e)?, str_of(e, "build")?)))
        .filter(|(_, key)| plalast::content(cfg)[*key].is_object())
        .collect()
}

/// Every string a block shows: `text`, list items and table cells (through `ref`s too).
fn block_texts(cfg: &Value, b: &Value) -> Vec<String> {
    let mut texts: Vec<String> = block_text(cfg, b).into_iter().collect();
    texts.extend(block_items(cfg, b).unwrap_or_default());
    texts.extend(
        arr(b, "rows")
            .iter()
            .flat_map(|r| r.as_array().into_iter().flatten())
            .filter_map(Value::as_str)
            .map(String::from),
    );
    texts
}

/// Number of entries of a list or a mapping.
fn count(v: &Value) -> usize {
    v.as_array()
        .map(Vec::len)
        .or_else(|| v.as_object().map(serde_json::Map::len))
        .unwrap_or(0)
}

/// One line per plain-data item (features, commands, log, index), as `(source, text)`.
fn plain_data(cfg: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (k, v) in primary(cfg)["features"].as_object().into_iter().flatten() {
        let status = v["status"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or("-");
        let descr = v["descr"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or("(no description)");
        out.push((
            "primary.features".to_string(),
            format!("{k} [{status}] {descr}"),
        ));
    }
    for (id, p) in plalast::problems(cfg) {
        let solved: Vec<&str> = arr(p, "solved_by")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        out.push((
            "primary.problems".to_string(),
            format!(
                "{id}: {} | {} | solved by: {}",
                str_of(p, "title").unwrap_or(id),
                str_of(p, "statement").unwrap_or(""),
                solved.join("; ")
            ),
        ));
    }
    for (id, p) in plalast::procedures(cfg) {
        let text: Vec<&str> = arr(p, "text").iter().filter_map(Value::as_str).collect();
        out.push((
            "primary.procedures".to_string(),
            format!(
                "{id}: {} | {}",
                str_of(p, "title").unwrap_or(id),
                text.join(" ")
            ),
        ));
    }
    for c in arr(primary(cfg), "commands") {
        out.push(("primary.commands".to_string(), fill("{cmd}: {summary}", c)));
    }
    for (i, e) in arr(primary(cfg), "changes").iter().enumerate() {
        let text = plalast::CHANGE_FIELDS
            .map(|k| str_of(e, k).unwrap_or(""))
            .join(" | ");
        out.push((format!("primary.changes[{i}]"), text));
    }
    for (i, e) in arr(primary(cfg), "log").iter().enumerate() {
        let text = LOG_FIELDS.map(|k| str_of(e, k).unwrap_or("")).join(" | ");
        out.push((format!("primary.log[{i}]"), text));
    }
    for (k, v) in primary(cfg)["index"].as_object().into_iter().flatten() {
        let text = v.as_str().map_or_else(|| v.to_string(), String::from);
        out.push((format!("primary.index.{k}"), text));
    }
    out
}

/// Every scalar under `v` as `(dotted path, text)`.
fn scalars(v: &Value, path: &str, out: &mut Vec<(String, String)>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                scalars(x, &format!("{path}.{k}"), out);
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                scalars(x, &format!("{path}.{i}"), out);
            }
        }
        Value::String(s) => out.push((path.to_string(), s.clone())),
        other => out.push((path.to_string(), other.to_string())),
    }
}

/// Prints the repo map, or everything matching `query`. Returns false when a query matches nothing.
fn recall(root: &Path, cfg: &Value, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        print_map(root, cfg);
        true
    } else {
        let found = search(cfg, &q);
        if !found {
            println!("nothing in the brain ({MEMORY}, params.yaml) matches `{query}`");
        }
        found
    }
}

/// Interruptor value shown for a section or mode, marking the default when it is not set.
fn state_of(cfg: &Value, group: &str, id: &str) -> String {
    lever_set(cfg, group, id).map_or_else(|| format!("{DEFAULT_LEVER} (default)"), String::from)
}

/// Type shown for a structure entry: generated or manual files, else its type.
fn entry_kind(e: &Value) -> &str {
    match (str_of(e, "type").unwrap_or("?"), str_of(e, "build")) {
        ("file", Some(_)) => "generated",
        ("file", None) => "manual",
        (ty, _) => ty,
    }
}

/// The map: docs, module kinds, plain data, settings, structure and levers.
fn print_map(root: &Path, cfg: &Value) {
    if let Some(t) = str_of(&primary(cfg)["project"], "title") {
        println!("{t}\n");
    }
    println!("Memory ({MEMORY}): three primary keys");
    for key in BRAIN_KEYS {
        for (d, sign) in DESCR_KEYS.iter().zip(["+", "-"]) {
            println!(
                "  {key:<8} {sign} {}",
                str_of(&cfg[key], d).unwrap_or("(missing)")
            );
        }
    }
    println!();
    let definition = str_of(docs(cfg), "definition").unwrap_or("");
    println!(
        "Docs (primary.docs): {definition} Folders: {}\n",
        doc_dirs(cfg).join(", ")
    );
    for kind in MODULE_KINDS {
        let links: Vec<String> = paths(cfg)["modules"][kind]["links"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(p, d)| format!("{p} -> {}", d.as_str().unwrap_or("?")))
            .collect();
        let links = if links.is_empty() {
            "no links".to_string()
        } else {
            links.join(", ")
        };
        let dir = module_dir(cfg, kind).unwrap_or_default();
        let names = visible_entries(&root.join(&dir));
        let names = if names.is_empty() {
            "none yet".to_string()
        } else {
            names.join(", ")
        };
        println!("{dir:<30} {kind:<10} {links}; modules: {names}");
    }
    for (name, path, _) in external_modules(cfg) {
        println!(
            "{:<30} {:<10} {name} (primary.modules)",
            path.unwrap_or("?"),
            "external"
        );
    }
    println!("\nModes (primary.mode, interruptor primary.mode.<name> in params.yaml):");
    for (name, m, _) in modes(cfg) {
        println!(
            "  {name:<28} {:<20} {}",
            state_of(cfg, MODE_GROUP, name),
            str_of(m, "title").unwrap_or("")
        );
    }
    println!("\nFeatures (primary.features):");
    for (_, text) in plain_data(cfg)
        .iter()
        .filter(|(s, _)| s == "primary.features")
    {
        println!("  {text}");
    }
    println!(
        "Problems (primary.problems): {}, procedures (primary.procedures): {}, commands (primary.commands): {}, log (primary.log): {}, changes (primary.changes): {}, index (primary.index): {} entries",
        count(&primary(cfg)["problems"]["items"]),
        count(&primary(cfg)["procedures"]["items"]),
        count(&primary(cfg)["commands"]),
        count(&primary(cfg)["log"]),
        count(&primary(cfg)["changes"]),
        count(&primary(cfg)["index"])
    );
    let c = config(cfg);
    println!(
        "Config (config): PLALAST_DIR {} = {}, repository {}, branches {} -> {}, audit allows {} string(s) and reports files over {} KB\n",
        str_of(c, "PLALAST_DIR").unwrap_or("?"),
        root.display(),
        str_of(c, "repository").unwrap_or("?"),
        str_of(&c["branches"], "integration").unwrap_or("?"),
        str_of(&c["branches"], "stable").unwrap_or("?"),
        count(&c["audit"]["allow"]),
        c["audit"]["max_file_kb"].as_u64().unwrap_or(0)
    );
    for e in plalast::structure(cfg) {
        let path = entry_path(cfg, e).unwrap_or_default();
        println!(
            "{path:<30} {:<10} {}",
            entry_kind(e),
            str_of(e, "description").unwrap_or("")
        );
    }
    println!("\nInterruptors (params.yaml, after #---INTERRUPTORS---):");
    for (path, key) in documents(cfg) {
        for s in arr(&plalast::content(cfg)[key], "sections") {
            let id = str_of(s, "id").unwrap_or("?");
            let title = str_of(s, "title").unwrap_or(id);
            let state = state_of(cfg, key, id);
            println!("{:<40} {state:<10} {title}", format!("{path} › {id}"));
        }
    }
}

/// Prints everything that contains `q` (lowercase), naming where it lives. True if anything did.
fn search(cfg: &Value, q: &str) -> bool {
    let hit = |s: &str| s.to_lowercase().contains(q);
    let mut found = false;
    let mut show = |line: String| {
        found = true;
        println!("{line}");
    };
    for key in BRAIN_KEYS {
        for d in DESCR_KEYS {
            let text = str_of(&cfg[key], d).unwrap_or("");
            if (hit(key) && d == "descr_+") || hit(text) {
                show(format!("{key}.{d}: {text}"));
            }
        }
    }
    let definition = str_of(docs(cfg), "definition").unwrap_or("");
    if hit(definition) {
        show(format!("primary.docs: {definition}"));
    }
    for kind in MODULE_KINDS {
        let d = str_of(&primary(cfg)[kind], "definition").unwrap_or("");
        if hit(d) {
            show(format!("primary.{kind}: {d}"));
        }
    }
    for (name, _, d) in external_modules(cfg) {
        let d = d.unwrap_or("");
        if hit(name) || hit(d) {
            show(format!("primary.modules.{name}: {d}"));
        }
    }
    for (name, m, _) in modes(cfg) {
        let state = state_of(cfg, MODE_GROUP, name);
        let mut texts: Vec<&str> = ["title", "when"]
            .iter()
            .filter_map(|k| str_of(m, k))
            .collect();
        texts.extend(arr(m, "rules").iter().filter_map(Value::as_str));
        if hit(name) {
            show(format!("primary.mode.{name} [{state}]"));
        }
        for t in texts.iter().filter(|t| hit(t)) {
            show(format!("primary.mode.{name} [{state}]: {t}"));
        }
    }
    for (source, text) in plain_data(cfg).iter().filter(|(_, t)| hit(t)) {
        show(format!("{source}: {text}"));
    }
    // Settings and levers: every scalar of params, with its dotted path.
    let mut params = Vec::new();
    scalars(&cfg["config"], "config", &mut params);
    scalars(&cfg["params"], "params", &mut params);
    for (path, value) in params.iter().filter(|(p, v)| hit(p) || hit(v)) {
        show(format!("{path}: {value}"));
    }
    for e in plalast::structure(cfg) {
        let path = entry_path(cfg, e).unwrap_or_default();
        let description = str_of(e, "description").unwrap_or("");
        let fields = [
            path.as_str(),
            str_of(e, "label").unwrap_or(""),
            description,
            str_of(e, "table").unwrap_or(""),
        ];
        if fields.iter().any(|f| hit(f)) {
            show(format!("{path:<30} {:<10} {description}", entry_kind(e)));
        }
    }
    for (path, key) in documents(cfg) {
        let doc = &plalast::content(cfg)[key];
        for b in arr(doc, "head") {
            for t in block_texts(cfg, b).iter().filter(|t| hit(t)) {
                show(format!("{path} › (head): {t}"));
            }
        }
        for s in arr(doc, "sections") {
            let id = str_of(s, "id").unwrap_or("?");
            let title = str_of(s, "title").unwrap_or(id);
            let state = state_of(cfg, key, id);
            let mut texts: Vec<String> = ["title", "when"]
                .iter()
                .filter_map(|k| str_of(s, k).map(String::from))
                .collect();
            texts.extend(arr(s, "blocks").iter().flat_map(|b| block_texts(cfg, b)));
            for t in texts.iter().filter(|t| hit(t)) {
                show(format!("{path} › {title} [{state}]: {t}"));
            }
        }
    }
    found
}
