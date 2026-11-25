//! audit: looks for things that must not be published or that put the repo at risk.
//!
//! - `pla-audit`                          every area.
//! - `pla-audit [secrets] [deps] [repo]`  only the given areas.
//!
//! `secrets`: keys, tokens, emails and home-folder paths in the files git would commit,
//!            and anything committed inside a `secret` folder.
//! `deps`:    known vulnerabilities in Cargo.lock (needs `cargo-audit`; skipped with a warning if absent).
//! `repo`:    broken symlinks, links leaving the repo, large files, uncommitted changes and
//!            branches out of sync (against the last fetch; it does not fetch).
//!
//! Exact strings listed in `primary.config.audit.allow` are never reported.

use plalast::{arr, cargo, data, exit, open, primary, repo_files, run, str_of, strings, Findings};
use regex::Regex;
use serde_json::Value;
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

const AREAS: [&str; 3] = ["secrets", "deps", "repo"];

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    exit((|| {
        if let Some(bad) = args.iter().find(|a| !AREAS.contains(&a.as_str())) {
            return Err(format!(
                "unknown area {bad}\nusage: pla-audit [secrets] [deps] [repo]"
            ));
        }
        let want = |area: &str| args.is_empty() || args.iter().any(|a| a == area);
        let (root, cfg) = open()?;
        let mut f = Findings::default();
        if want("secrets") {
            audit_secrets(&root, &cfg, &mut f)?;
        }
        if want("deps") {
            audit_deps(&root, &mut f);
        }
        if want("repo") {
            audit_repo(&root, &cfg, &mut f)?;
        }
        Ok(f.finish("pla-audit"))
    })())
}

// ---------- secrets ----------

fn patterns() -> Vec<(&'static str, Regex)> {
    [
        ("private key", r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
        ("AWS access key", r"\bAKIA[0-9A-Z]{16}\b"),
        (
            "GitHub token",
            r"\b(gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{22,})",
        ),
        ("Anthropic key", r"\bsk-ant-[A-Za-z0-9_-]{20,}"),
        ("API key (sk-)", r"\bsk-[A-Za-z0-9]{32,}\b"),
        ("Slack token", r"\bxox[abprs]-[A-Za-z0-9-]{10,}"),
        (
            "assigned secret",
            r#"(?i)\b(api[_-]?key|secret|token|passw(or)?d)\b\s*[:=]\s*["'][^"'\s]{8,}["']"#,
        ),
        (
            "email address",
            r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b",
        ),
        ("home folder path", r"(/Users/|/home/)[A-Za-z0-9._-]+/"),
    ]
    .into_iter()
    .map(|(name, re)| (name, Regex::new(re).expect("valid pattern")))
    .collect()
}

/// Shows enough of a match to find it, not enough to leak it.
fn mask(s: &str) -> String {
    let head: String = s.chars().take(6).collect();
    if s.chars().count() > 6 {
        format!("{head}…")
    } else {
        head
    }
}

/// `(line, pattern name, masked match)` for every hit in `text` that is not allowed.
fn scan(text: &str, pats: &[(&str, Regex)], allow: &[String]) -> Vec<(usize, String, String)> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        for (name, re) in pats {
            for m in re.find_iter(line) {
                if !allow.iter().any(|a| a == m.as_str()) {
                    out.push((n + 1, name.to_string(), mask(m.as_str())));
                }
            }
        }
    }
    out
}

fn audit_secrets(root: &Path, cfg: &Value, f: &mut Findings) -> Result<(), String> {
    let pats = patterns();
    let allow = strings(&primary(cfg)["config"]["audit"], "allow");
    for file in repo_files(root)? {
        let p = root.join(&file);
        if p.is_symlink() || !p.is_file() {
            continue;
        }
        // Binary files (images, ...) are not text: skip them.
        let Ok(text) = fs::read_to_string(&p) else {
            continue;
        };
        for (line, name, shown) in scan(&text, &pats, &allow) {
            f.error("secrets", format!("{file}:{line}: {name} `{shown}`"));
        }
    }
    // Nothing but .gitkeep may be committed inside a secret folder.
    for e in arr(data(cfg), "structure") {
        if e.get("secret").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let Some(dir) = str_of(e, "path") else {
            continue;
        };
        let (_, out) = run(root, "git", &["ls-files", "--cached", "--", dir])?;
        for tracked in out.lines().filter(|l| !l.ends_with("/.gitkeep")) {
            f.error(
                "secrets",
                format!("{tracked}: committed inside the secret folder {dir}"),
            );
        }
    }
    Ok(())
}

// ---------- deps ----------

fn audit_deps(root: &Path, f: &mut Findings) {
    let cargo = cargo();
    match run(root, &cargo, &["audit", "--version"]) {
        Ok((true, _)) => match run(root, &cargo, &["audit", "--quiet"]) {
            Ok((true, _)) => {}
            Ok((false, out)) => f.error("deps", format!("cargo audit found problems:\n{}", out.trim())),
            Err(e) => f.error("deps", e),
        },
        _ => f.warn(
            "deps",
            "cargo-audit is not installed, dependency audit skipped (install: cargo install cargo-audit --locked)",
        ),
    }
}

// ---------- repo ----------

/// Resolves `target` (a symlink's text) against the link's folder without touching disk.
fn lexical_target(link: &Path, target: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in link
        .parent()
        .unwrap_or(Path::new(""))
        .join(target)
        .components()
    {
        match c {
            // `..` cancels the last real folder; with none left (or only `..`), it stays.
            Component::ParentDir => {
                let last_is_real =
                    matches!(out.components().next_back(), Some(Component::Normal(_)));
                if last_is_real {
                    out.pop();
                } else {
                    out.push("..");
                }
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

fn audit_repo(root: &Path, cfg: &Value, f: &mut Findings) -> Result<(), String> {
    const A: &str = "repo";
    let allow = strings(&primary(cfg)["config"]["audit"], "allow");
    let max_kb = primary(cfg)["config"]["audit"]["max_file_kb"]
        .as_u64()
        .unwrap_or(1024);
    for file in repo_files(root)? {
        if allow.contains(&file) {
            continue;
        }
        let p = root.join(&file);
        if p.is_symlink() {
            let target = fs::read_link(&p).map_err(|e| e.to_string())?;
            let resolved = lexical_target(Path::new(&file), &target);
            let outside = resolved.starts_with("..") || resolved.is_absolute();
            match (p.exists(), outside) {
                (true, false) => {}
                (true, true) => f.warn(A, format!("{file} -> {}: points outside the repo, so it only resolves on machines with the same layout", target.display())),
                (false, true) => f.warn(A, format!("{file} -> {}: points outside the repo and is broken here", target.display())),
                (false, false) => f.error(A, format!("{file} -> {}: broken link", target.display())),
            }
            continue;
        }
        if let Ok(meta) = fs::metadata(&p) {
            if meta.len() > max_kb * 1024 {
                f.warn(
                    A,
                    format!(
                        "{file}: {} KB, larger than primary.config.audit.max_file_kb ({max_kb} KB)",
                        meta.len() / 1024
                    ),
                );
            }
        }
    }

    let (_, status) = run(root, "git", &["status", "--porcelain"])?;
    let changed = status.lines().count();
    if changed > 0 {
        f.warn(A, format!("{changed} uncommitted change(s)"));
    }
    let branches = primary(cfg)["project"]["branches"].clone();
    let integration = str_of(&branches, "integration").unwrap_or("develop");
    let stable = str_of(&branches, "stable").unwrap_or("main");
    let pairs = [
        (integration.to_string(), format!("origin/{integration}")),
        (stable.to_string(), format!("origin/{stable}")),
        (stable.to_string(), integration.to_string()),
    ];
    for (a, b) in pairs {
        let range = format!("{a}...{b}");
        match run(
            root,
            "git",
            &["rev-list", "--left-right", "--count", &range],
        ) {
            Ok((true, out)) => {
                let n: Vec<u64> = out
                    .split_whitespace()
                    .filter_map(|x| x.parse().ok())
                    .collect();
                if let [ahead, behind] = n[..] {
                    if ahead > 0 || behind > 0 {
                        f.warn(
                            A,
                            format!("{a} is {ahead} commit(s) ahead and {behind} behind {b}"),
                        );
                    }
                }
            }
            _ => f.warn(A, format!("cannot compare {a} with {b} (missing branch?)")),
        }
    }
    Ok(())
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    // Sample secrets are assembled at runtime so this file never matches its own patterns.
    fn sample(parts: &[&str]) -> String {
        parts.concat()
    }

    #[test]
    fn finds_tokens_emails_and_home_paths() {
        let text = format!(
            "aws = {}\nmail me at {}\nlog in {}\nnothing here",
            sample(&["AKIA", "ABCDEFGHIJKLMNOP"]),
            sample(&["someone", "@", "example.org"]),
            sample(&["/Use", "rs/alice/notes"]),
        );
        let hits = scan(&text, &patterns(), &[]);
        let names: Vec<(usize, &str)> = hits.iter().map(|(l, n, _)| (*l, n.as_str())).collect();
        assert_eq!(
            names,
            vec![
                (1, "AWS access key"),
                (2, "email address"),
                (3, "home folder path")
            ]
        );
        assert!(hits.iter().all(|(_, _, shown)| shown.chars().count() <= 7));
    }

    #[test]
    fn allowed_strings_are_not_reported() {
        let email = sample(&["team", "@", "example.org"]);
        assert!(scan(&email, &patterns(), std::slice::from_ref(&email)).is_empty());
    }

    #[test]
    fn assigned_secrets_are_found_but_short_values_are_not() {
        let hit = sample(&["api_key = \"", "abcd1234efgh", "\""]);
        let short = sample(&["password = \"", "x", "\""]);
        assert_eq!(scan(&hit, &patterns(), &[]).len(), 1);
        assert!(scan(&short, &patterns(), &[]).is_empty());
    }

    #[test]
    fn link_targets_resolve_lexically() {
        assert_eq!(
            lexical_target(
                Path::new(".claude/CLAUDE.md"),
                Path::new("../agents/CLAUDE.md")
            ),
            PathBuf::from("agents/CLAUDE.md")
        );
        assert!(lexical_target(Path::new("zettelkasten"), Path::new("../../x")).starts_with(".."));
    }
}
