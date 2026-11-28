//! The 4 fantásticos of this repo's real brain, one by one, through the whole battery of
//! `plalast::fantastic::CHECKS`. A check added to the battery is run here on every category.

use plalast::fantastic::{all, CATEGORIES, CHECKS};
use plalast::load;
use std::path::Path;

#[test]
fn every_category_passes_every_check_of_the_battery() {
    let cfg = load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let categories = all(&cfg);
    assert_eq!(categories.len(), CATEGORIES.len());
    let mut failures = Vec::new();
    for f in &categories {
        assert!(!f.rules.is_empty(), "{} has no rules", f.key);
        for (name, run) in CHECKS {
            for msg in run(&cfg, f) {
                failures.push(format!("{} [{name}]: {msg}", f.key));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_category_has_a_kind() {
    let cfg = load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    for f in all(&cfg) {
        assert!(
            f.kind.is_some(),
            "{} is neither obligatory nor preferential",
            f.key
        );
    }
}
