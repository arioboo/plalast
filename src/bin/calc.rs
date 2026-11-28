//! calc: the pure calculator (`plalast::calc`) from the command line, to isolate a calculation.
//!
//! - `pla-calc <expression>`   prints the exact result and, for a fraction, its decimal digits.
//!
//! Exit codes: 0 with a result, 1 when the expression has no value (division by zero, overflow,
//! a syntax error), 2 without an expression.

use plalast::calc::eval;
use plalast::exit;
use std::env;
use std::process::ExitCode;

/// Decimal digits shown after the point for a fraction.
const PLACES: usize = 30;

fn main() -> ExitCode {
    let expression = env::args().skip(1).collect::<Vec<_>>().join(" ");
    exit((|| {
        if expression.trim().is_empty() {
            return Err("usage: pla-calc <expression>   (e.g. pla-calc '(1 + 2) / 7')".to_string());
        }
        match eval(&expression) {
            Ok(v) if v.denominator() == 1 => println!("{v}"),
            Ok(v) => {
                let (digits, cut) = v.decimal(PLACES);
                let mark = if cut { "…" } else { "" };
                println!("{v} = {digits}{mark}");
            }
            Err(e) => {
                println!("no value: {e}");
                return Ok(false);
            }
        }
        Ok(true)
    })())
}
