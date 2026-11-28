//! The 4 fantásticos: the categories of AGENTS.md (static rules, static premises, dynamic
//! premises, good practices, preferences; there are more than four, the name stays), handled
//! one by one.
//!
//! Each category is a [`Fantastic`]: its key, its path in the memory
//! (`primary.agents-md.<key>`), its kind (`obligatory` or `preferential`, from
//! `config.obligatory` / `config.preferential`) and its rules, each with a name and a text.
//!
//! [`CHECKS`] is the battery every category goes through: a check takes the brain and one
//! category and returns its failures. A check added to the table runs on every category, in the
//! lint (`pla-lint memory`) and in `tests/fantastic.rs`.

use crate::{arr, config, primary, problems, strings};
use serde_json::Value;

/// The categories of AGENTS.md, in their order, as keys of `primary.agents-md`.
pub const CATEGORIES: [&str; 5] = [
    "static_rules",
    "static_premises",
    "dynamic_premises",
    "good_practices",
    "preferences",
];

/// The kinds a category can have, as the params that list them (`config.<kind>`).
pub const KINDS: [&str; 2] = ["obligatory", "preferential"];

/// One rule of a category: its name (its text up to the first `:` or `. `) and its full text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rule<'a> {
    pub name: &'a str,
    pub text: &'a str,
}

/// One category of AGENTS.md with its rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fantastic<'a> {
    /// Key in `primary.agents-md`, such as `static_rules`.
    pub key: &'static str,
    /// Its path in the memory, such as `primary.agents-md.static_rules`.
    pub path: String,
    /// `obligatory` or `preferential`; None when no param lists it.
    pub kind: Option<&'static str>,
    pub rules: Vec<Rule<'a>>,
}

/// Name of a rule: its text up to the first `:` or `. ` (such as `Pareto rule`).
#[must_use]
pub fn rule_name(text: &str) -> &str {
    let end = [text.find(':'), text.find(". ")]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(text.len());
    text[..end].trim_end_matches('.').trim()
}

/// Path of a category in the memory.
#[must_use]
pub fn path_of(key: &str) -> String {
    format!("primary.agents-md.{key}")
}

/// The kind of the category at `path`: the first of [`KINDS`] whose param lists it.
#[must_use]
pub fn kind_of(cfg: &Value, path: &str) -> Option<&'static str> {
    KINDS
        .into_iter()
        .find(|k| strings(config(cfg), k).iter().any(|p| p == path))
}

/// One category, by key, with its rules.
#[must_use]
pub fn get<'a>(cfg: &'a Value, key: &'static str) -> Fantastic<'a> {
    let path = path_of(key);
    let rules = arr(&primary(cfg)["agents-md"][key], "items")
        .iter()
        .filter_map(Value::as_str)
        .map(|text| Rule {
            name: rule_name(text),
            text,
        })
        .collect();
    Fantastic {
        key,
        kind: kind_of(cfg, &path),
        path,
        rules,
    }
}

/// Every category, in order.
#[must_use]
pub fn all(cfg: &Value) -> Vec<Fantastic<'_>> {
    CATEGORIES.into_iter().map(|k| get(cfg, k)).collect()
}

/// Every rule of every category as `(category key, rule name)`, in order.
#[must_use]
pub fn rules(cfg: &Value) -> Vec<(&'static str, &str)> {
    all(cfg)
        .into_iter()
        .flat_map(|f| f.rules.into_iter().map(move |r| (f.key, r.name)))
        .collect()
}

/// ` (obligatory)` or ` (preferential)` for a doc section whose blocks point at a listed
/// category (a `ref` equal to its path or under it); empty otherwise. AGENTS.md titles use it.
#[must_use]
pub fn section_mark(cfg: &Value, section: &Value) -> String {
    for kind in KINDS {
        let listed = strings(config(cfg), kind);
        let hit = arr(section, "blocks")
            .iter()
            .filter_map(|b| b["ref"].as_str())
            .any(|r| {
                listed
                    .iter()
                    .any(|p| r == p || r.starts_with(&format!("{p}.")))
            });
        if hit {
            return format!(" ({kind})");
        }
    }
    String::new()
}

/// Paths in `config.obligatory` or `config.preferential` that are not a category.
#[must_use]
pub fn unknown_kinds(cfg: &Value) -> Vec<(&'static str, String)> {
    let known: Vec<String> = CATEGORIES.iter().map(|k| path_of(k)).collect();
    KINDS
        .into_iter()
        .flat_map(|kind| {
            strings(config(cfg), kind)
                .into_iter()
                .filter(|p| !known.contains(p))
                .map(move |p| (kind, p))
        })
        .collect()
}

// ---------- the battery ----------

/// A check of one category: its failures, as messages (empty when it passes).
pub type Check = fn(&Value, &Fantastic) -> Vec<String>;

/// The battery of checks every category goes through, by name. Add a check here and every
/// category runs it (in `pla-lint memory` and in `tests/fantastic.rs`).
pub const CHECKS: [(&str, Check); 4] = [
    ("one-kind", one_kind),
    ("unique-names", unique_names),
    ("solves-a-problem", solves_a_problem),
    ("rendered", rendered),
];

/// Runs every check on every category: `(category key, check name, message)` per failure.
#[must_use]
pub fn check(cfg: &Value) -> Vec<(&'static str, &'static str, String)> {
    let mut out = Vec::new();
    for f in all(cfg) {
        for (name, run) in CHECKS {
            out.extend(run(cfg, &f).into_iter().map(|m| (f.key, name, m)));
        }
    }
    out
}

/// The category is listed exactly once, in `config.obligatory` or `config.preferential`.
fn one_kind(cfg: &Value, f: &Fantastic) -> Vec<String> {
    let times: usize = KINDS
        .iter()
        .map(|k| {
            strings(config(cfg), k)
                .iter()
                .filter(|p| **p == f.path)
                .count()
        })
        .sum();
    match times {
        1 => vec![],
        0 => vec![format!(
            "`{}` is neither in config.obligatory nor in config.preferential",
            f.path
        )],
        _ => vec![format!(
            "`{}` is listed {times} times in config.obligatory and config.preferential",
            f.path
        )],
    }
}

/// No rule name of the category is used by another rule, here or in another category (problems
/// point at rules by name).
fn unique_names(cfg: &Value, f: &Fantastic) -> Vec<String> {
    let everyone = rules(cfg);
    f.rules
        .iter()
        .filter(|r| everyone.iter().filter(|(_, n)| *n == r.name).count() > 1)
        .map(|r| format!("rule name `{}` is used twice in primary.agents-md; problems could not tell them apart", r.name))
        .collect()
}

/// Every rule of the category solves a problem of `primary.problems` (Lean memory).
fn solves_a_problem(cfg: &Value, f: &Fantastic) -> Vec<String> {
    let solved: Vec<&str> = problems(cfg)
        .into_iter()
        .flat_map(|(_, p)| arr(p, "solved_by").iter().filter_map(Value::as_str))
        .collect();
    f.rules
        .iter()
        .filter(|r| !solved.contains(&r.name))
        .map(|r| {
            format!(
                "`{}` solves no problem in primary.problems (Lean memory)",
                r.name
            )
        })
        .collect()
}

/// The category is shown in AGENTS.md: a section of `primary.docs.content.agents_md` points at
/// its rules.
fn rendered(cfg: &Value, f: &Fantastic) -> Vec<String> {
    let items = format!("{}.items", f.path);
    let shown = arr(&crate::content(cfg)["agents_md"], "sections")
        .iter()
        .flat_map(|s| arr(s, "blocks"))
        .any(|b| b["ref"].as_str() == Some(items.as_str()));
    if shown {
        vec![]
    } else {
        vec![format!("no section of AGENTS.md shows `{items}`")]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn brain() -> Value {
        json!({
            "config": {
                "obligatory": ["primary.agents-md.static_rules"],
                "preferential": ["primary.agents-md.preferences", "primary.agents-md.preferences"]
            },
            "primary": {
                "agents-md": {
                    "static_rules": {"items": ["Alpha: a.", "Beta. b"]},
                    "preferences": {"items": ["Alpha: again."]}
                },
                "problems": {"items": {"p": {"solved_by": ["Alpha"]}}},
                "docs": {"content": {"agents_md": {"sections": [
                    {"blocks": [{"type": "list", "ref": "primary.agents-md.static_rules.items"}]}
                ]}}}
            }
        })
    }

    #[test]
    fn categories_carry_their_path_kind_and_named_rules() {
        let c = brain();
        let f = get(&c, "static_rules");
        assert_eq!(f.path, "primary.agents-md.static_rules");
        assert_eq!(f.kind, Some("obligatory"));
        assert_eq!(
            f.rules.iter().map(|r| r.name).collect::<Vec<_>>(),
            ["Alpha", "Beta"]
        );
        assert_eq!(get(&c, "good_practices").kind, None);
        assert_eq!(all(&c).len(), CATEGORIES.len());
    }

    #[test]
    fn section_marks_follow_the_kind_of_the_category() {
        let c = brain();
        let s = |r: &str| json!({"blocks": [{"type": "list", "ref": r}]});
        assert_eq!(
            section_mark(&c, &s("primary.agents-md.static_rules.items")),
            " (obligatory)"
        );
        assert_eq!(
            section_mark(&c, &s("primary.agents-md.preferences.items")),
            " (preferential)"
        );
        assert_eq!(
            section_mark(&c, &s("primary.agents-md.static_rulesets")),
            ""
        );
    }

    #[test]
    fn the_battery_reports_each_failure_with_its_category_and_check() {
        let found = check(&brain());
        let has = |key: &str, chk: &str, part: &str| {
            found
                .iter()
                .any(|(k, c, m)| *k == key && *c == chk && m.contains(part))
        };
        assert!(has("good_practices", "one-kind", "neither"), "{found:?}");
        assert!(
            has("preferences", "one-kind", "listed 2 times"),
            "{found:?}"
        );
        assert!(has("static_rules", "unique-names", "`Alpha`"), "{found:?}");
        assert!(
            has("static_rules", "solves-a-problem", "`Beta`"),
            "{found:?}"
        );
        assert!(
            has("preferences", "rendered", "preferences.items"),
            "{found:?}"
        );
        assert!(!has("static_rules", "rendered", ""), "{found:?}");
    }

    #[test]
    fn unknown_kinds_are_paths_that_are_no_category() {
        let c = json!({"config": {"obligatory": ["primary.agents-md.static_rules", "primary.nowhere"]}});
        assert_eq!(
            unknown_kinds(&c),
            vec![("obligatory", "primary.nowhere".to_string())]
        );
    }
}
