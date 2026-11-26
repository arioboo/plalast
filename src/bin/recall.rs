//! recall: prints the repo map from the brain (`memory.json` + `params.yaml`), or searches it.
//!
//! - `pla-recall`          the map: docs, module kinds, plain data, settings, paths and levers.
//! - `pla-recall <term>`   everything matching `term`, naming its file, section and lever.

use plalast::{
    arr, block_items, block_text, config, data, doc_dirs, docs, entry_path, exit, external_modules,
    fill, lever, module_dir, open, paths, primary, str_of, visible_entries, LOG_FIELDS, MEMORY,
    MODULE_KINDS,
};
use serde_json::Value;
use std::env;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let query = env::args().skip(1).collect::<Vec<_>>().join(" ");
    exit(open().map(|(root, cfg)| recall(&root, &cfg, &query)))
}

/// `(path, content key)` of every generated document, in structure order.
fn documents(cfg: &Value) -> Vec<(String, &str)> {
    arr(data(cfg), "structure")
        .iter()
        .filter_map(|e| Some((entry_path(cfg, e)?, str_of(e, "build")?)))
        .filter(|(_, key)| data(cfg)["content"][*key].is_object())
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
    for c in arr(primary(cfg), "commands") {
        out.push(("primary.commands".to_string(), fill("{cmd}: {summary}", c)));
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
    println!("\nFeatures (primary.features):");
    for (_, text) in plain_data(cfg)
        .iter()
        .filter(|(s, _)| s == "primary.features")
    {
        println!("  {text}");
    }
    println!(
        "Commands (primary.commands): {}, log (primary.log): {}, index (primary.index): {} entries",
        count(&primary(cfg)["commands"]),
        count(&primary(cfg)["log"]),
        count(&primary(cfg)["index"])
    );
    let c = config(cfg);
    println!(
        "Config (params.config): PLALAST_DIR {} = {}, repository {}, branches {} -> {}, audit allows {} string(s) and reports files over {} KB\n",
        str_of(c, "PLALAST_DIR").unwrap_or("?"),
        root.display(),
        str_of(c, "repository").unwrap_or("?"),
        str_of(&c["branches"], "integration").unwrap_or("?"),
        str_of(&c["branches"], "stable").unwrap_or("?"),
        count(&c["audit"]["allow"]),
        c["audit"]["max_file_kb"].as_u64().unwrap_or(0)
    );
    for e in arr(data(cfg), "structure") {
        let path = entry_path(cfg, e).unwrap_or_default();
        println!(
            "{path:<30} {:<10} {}",
            entry_kind(e),
            str_of(e, "description").unwrap_or("")
        );
    }
    println!("\nMemory levers (params.levers):");
    for (path, key) in documents(cfg) {
        for s in arr(&data(cfg)["content"][key], "sections") {
            let id = str_of(s, "id").unwrap_or("?");
            let title = str_of(s, "title").unwrap_or(id);
            let state = lever(cfg, key, id).unwrap_or("MISSING");
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
    for (source, text) in plain_data(cfg).iter().filter(|(_, t)| hit(t)) {
        show(format!("{source}: {text}"));
    }
    // Settings and levers: every scalar of params, with its dotted path.
    let mut params = Vec::new();
    scalars(&cfg["params"], "params", &mut params);
    for (path, value) in params.iter().filter(|(p, v)| hit(p) || hit(v)) {
        show(format!("{path}: {value}"));
    }
    for e in arr(data(cfg), "structure") {
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
        let doc = &data(cfg)["content"][key];
        for b in arr(doc, "head") {
            for t in block_texts(cfg, b).iter().filter(|t| hit(t)) {
                show(format!("{path} › (head): {t}"));
            }
        }
        for s in arr(doc, "sections") {
            let id = str_of(s, "id").unwrap_or("?");
            let title = str_of(s, "title").unwrap_or(id);
            let state = lever(cfg, key, id).unwrap_or("MISSING");
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
