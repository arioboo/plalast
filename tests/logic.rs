//! Logic tests: the contracts (`ensures`) of the Verus-verified functions in `src/verified.rs`,
//! checked at run time over whole small domains and the edges. Verus proves them for every input;
//! these keep them checked on machines without Verus. Precarious: they sample, they do not prove.

use plalast::calc::{eval, Rational};
use plalast::fantastic::rule_name;
use plalast::verified::{heading_hashes, ordered_number};

#[test]
fn heading_hashes_keeps_its_contract() {
    let edges = [u64::MAX - 1, u64::MAX];
    for level in (0..=1000).chain(edges) {
        let n = heading_hashes(level);
        assert!((1..=6).contains(&n), "level {level} -> {n}");
        if (1..=6).contains(&level) {
            assert_eq!(n, level);
        }
    }
}

/// The pure calculator obeys the laws of arithmetic on a grid of exact values: `a + b - b == a`,
/// commutativity, distributivity, and `a / b * b == a` for every `b` that is not zero.
#[test]
fn calc_obeys_the_laws_of_arithmetic() {
    let values: Vec<Rational> = (-6..=6)
        .flat_map(|n| (1..=4).map(move |d| Rational::new(n, d).unwrap()))
        .collect();
    for &a in &values {
        for &b in &values {
            assert_eq!(a.checked_add(b).unwrap().checked_sub(b).unwrap(), a);
            assert_eq!(a.checked_add(b).unwrap(), b.checked_add(a).unwrap());
            assert_eq!(a.checked_mul(b).unwrap(), b.checked_mul(a).unwrap());
            for &c in values.iter().step_by(7) {
                let left = a.checked_mul(b.checked_add(c).unwrap()).unwrap();
                assert_eq!(
                    left,
                    a.checked_mul(b)
                        .unwrap()
                        .checked_add(a.checked_mul(c).unwrap())
                        .unwrap()
                );
            }
            if b.numerator() != 0 {
                assert_eq!(a.checked_div(b).unwrap().checked_mul(b).unwrap(), a);
            }
        }
    }
    // The text form agrees with the value form.
    assert_eq!(eval("(3/4) * (8/3) - 1").unwrap().to_string(), "1");
}

#[test]
fn ordered_number_keeps_its_contract() {
    // `requires index < u64::MAX`: the inputs stay inside it.
    for index in (0..=1000).chain([u64::MAX - 2, u64::MAX - 1]) {
        let n = ordered_number(index);
        assert_eq!(n, index + 1);
        assert!(n >= 1);
    }
}

/// A rule's name is its text up to the first `:` or `. `, so problems can point at it.
#[test]
fn rule_names_stop_at_the_first_colon_or_full_stop() {
    assert_eq!(
        rule_name("Pareto rule: 20% of the effort..."),
        "Pareto rule"
    );
    assert_eq!(
        rule_name("No duplication. Only one way..."),
        "No duplication"
    );
    assert_eq!(
        rule_name("Zettelkasten markdown spirit."),
        "Zettelkasten markdown spirit"
    );
    assert_eq!(
        rule_name("Enforce, don't ask: rules that..."),
        "Enforce, don't ask"
    );
    assert_eq!(
        rule_name("Verification (precarious, growing): x"),
        "Verification (precarious, growing)"
    );
}
