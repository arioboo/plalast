//! recall: prints the repo map from `config.json`, or searches it.
//!
//! - `pla-recall`          every path with its type and description, then every memory lever.
//! - `pla-recall <term>`   matching paths, and document lines with their file, section and lever.

use plalast::{
    arr, data, doc_dirs, docs, exit, lever, module_dir, open, primary, str_of, strings, CONFIG,
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

/// Visible entries of a module folder (hidden ones such as `.gitkeep` are skipped), sorted.
fn module_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|d| d.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    names
}

/// `(path, content key)` of every generated document, in structure order.
fn documents(cfg: &Value) -> Vec<(&str, &str)> {
    arr(data(cfg), "structure")
        .iter()
        .filter_map(|e| Some((str_of(e, "path")?, str_of(e, "build")?)))
        .filter(|(_, key)| data(cfg)["content"][*key].is_object())
        .collect()
}

/// Every string a block shows: `text`, list items and table cells.
fn block_texts(b: &Value) -> Vec<String> {
    let mut texts: Vec<String> = str_of(b, "text").map(String::from).into_iter().collect();
    texts.extend(strings(b, "items"));
    texts.extend(
        arr(b, "rows")
            .iter()
            .flat_map(|r| r.as_array().into_iter().flatten())
            .filter_map(Value::as_str)
            .map(String::from),
    );
    texts
}

/// Prints the repo map, or every structure entry and document line matching `query`.
/// Returns false when a query matches nothing.
fn recall(root: &Path, cfg: &Value, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    let hit = |s: &str| q.is_empty() || s.to_lowercase().contains(&q);
    let mut found = false;

    if q.is_empty() {
        if let Some(t) = primary(cfg)
            .pointer("/project/title")
            .and_then(Value::as_str)
        {
            println!("{t}\n");
        }
    }
    // What the docs are, and where they live (primary.docs).
    let definition = str_of(docs(cfg), "definition").unwrap_or("");
    if q.is_empty() || hit(definition) {
        found = true;
        println!(
            "Docs (primary.docs): {definition} Folders: {}\n",
            doc_dirs(cfg).join(", ")
        );
    }
    if q.is_empty() {
        // Module kinds and the zettelkasten (primary.<kind>, primary.zettelkasten).
        for kind in MODULE_KINDS {
            let links: Vec<String> = primary(cfg)[kind]["links"]
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
            let names = module_names(&root.join(&dir));
            let names = if names.is_empty() {
                "none yet".to_string()
            } else {
                names.join(", ")
            };
            println!("{dir:<30} {kind:<10} {links}; modules: {names}");
        }
        if let Some(z) = str_of(&primary(cfg)["zettelkasten"], "path") {
            println!("{z:<30} {:<10} the vault the agents use", "zettelkasten");
        }
        // Plain data: features, log, index, and the tools' settings.
        println!("\nFeatures (primary.features):");
        for (k, v) in primary(cfg)["features"].as_object().into_iter().flatten() {
            let status = v["status"]
                .as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or("-");
            let descr = v["descr"]
                .as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or("(no description)");
            println!("{k:<30} {status:<10} {descr}");
        }
        let count = |v: &Value| {
            v.as_array()
                .map(Vec::len)
                .or_else(|| v.as_object().map(|m| m.len()))
                .unwrap_or(0)
        };
        println!("Log (primary.log): {} entries", count(&primary(cfg)["log"]));
        println!(
            "Index (primary.index): {} entries",
            count(&primary(cfg)["index"])
        );
        let audit = &primary(cfg)["config"]["audit"];
        println!(
            "Config (primary.config): audit allows {} string(s), reports files over {} KB",
            count(&audit["allow"]),
            audit["max_file_kb"].as_u64().unwrap_or(0)
        );
        println!();
    }
    // Definitions of module kinds and the zettelkasten, and the plain data, also answer queries.
    if !q.is_empty() {
        for (k, v) in primary(cfg)["features"].as_object().into_iter().flatten() {
            let text = format!(
                "{k} [{}] {}",
                v["status"].as_str().unwrap_or(""),
                v["descr"].as_str().unwrap_or("")
            );
            if hit(&text) {
                found = true;
                println!("primary.features: {text}");
            }
        }
        for (i, entry) in arr(primary(cfg), "log").iter().enumerate() {
            let text = entry
                .as_str()
                .map(String::from)
                .unwrap_or_else(|| entry.to_string());
            if hit(&text) {
                found = true;
                println!("primary.log[{i}]: {text}");
            }
        }
        for (k, v) in primary(cfg)["index"].as_object().into_iter().flatten() {
            let text = format!(
                "{k}: {}",
                v.as_str()
                    .map(String::from)
                    .unwrap_or_else(|| v.to_string())
            );
            if hit(&text) {
                found = true;
                println!("primary.index.{text}");
            }
        }
        for kind in MODULE_KINDS.iter().copied().chain(["zettelkasten"]) {
            let d = str_of(&primary(cfg)[kind], "definition").unwrap_or("");
            if hit(d) {
                found = true;
                println!("primary.{kind}: {d}");
            }
        }
    }
    for e in arr(data(cfg), "structure") {
        let fields = ["path", "label", "description", "table"].map(|k| str_of(e, k).unwrap_or(""));
        if fields.iter().any(|f| hit(f)) {
            found = true;
            let ty = match (str_of(e, "type").unwrap_or("?"), str_of(e, "build")) {
                ("file", Some(_)) => "generated",
                ("file", None) => "manual",
                (ty, _) => ty,
            };
            println!("{:<30} {:<10} {}", fields[0], ty, fields[2]);
        }
    }

    if q.is_empty() {
        println!("\nMemory levers (primary.docs.levers):");
        for (path, key) in documents(cfg) {
            for s in arr(&data(cfg)["content"][key], "sections") {
                let id = str_of(s, "id").unwrap_or("?");
                let title = str_of(s, "title").unwrap_or(id);
                let state = lever(cfg, key, id).unwrap_or("MISSING");
                println!("{:<40} {:<10} {title}", format!("{path} › {id}"), state);
            }
        }
        return true;
    }

    // Document text: report the file, the section and its lever for each match.
    for (path, key) in documents(cfg) {
        let doc = &data(cfg)["content"][key];
        for b in arr(doc, "head") {
            for t in block_texts(b).iter().filter(|t| hit(t)) {
                found = true;
                println!("{path} › (head): {t}");
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
            texts.extend(arr(s, "blocks").iter().flat_map(block_texts));
            for t in texts.iter().filter(|t| hit(t)) {
                found = true;
                println!("{path} › {title} [{state}]: {t}");
            }
        }
    }
    if !found {
        println!("nothing in {CONFIG} matches `{query}`");
    }
    found
}
