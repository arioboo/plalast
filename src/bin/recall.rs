//! recall: prints the repo map from `config.json`, or searches it.
//!
//! - `pla-recall`          every path with its type and description, then every memory lever.
//! - `pla-recall <term>`   matching paths, and document lines with their file, section and lever.

use plalast::{arr, data, exit, lever, open, primary, str_of, strings, CONFIG};
use serde_json::Value;
use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let query = env::args().skip(1).collect::<Vec<_>>().join(" ");
    exit(open().map(|(_root, cfg)| recall(&cfg, &query)))
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
fn recall(cfg: &Value, query: &str) -> bool {
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
        println!("\nMemory levers (primary.levers):");
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
