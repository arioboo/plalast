//! The pure calculator: evaluates an arithmetic expression exactly, with no input, output,
//! state or side effects, so a calculation is isolated from everything else (and from guessing).
//!
//! Numbers are exact rationals (`i128` numerator and denominator), so `0.1 + 0.2` is exactly
//! `3/10`. Supported: integers and decimals (`12`, `0.25`), `+ - * /`, `^` with an integer
//! exponent, unary minus and parentheses. Division by zero and overflow are errors, never a
//! wrong number.
//!
//! ```
//! use plalast::calc::{eval, Rational};
//! assert_eq!(eval("0.1 + 0.2").unwrap(), Rational::new(3, 10).unwrap());
//! assert_eq!(eval("(1 + 2) * 3 ^ 2").unwrap().to_string(), "27");
//! ```

use std::fmt;

/// An exact fraction `num / den`, always reduced, with `den > 0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rational {
    num: i128,
    den: i128,
}

fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.unsigned_abs(), b.unsigned_abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    // `a` divides both inputs, so it fits in i128 unless both were i128::MIN.
    i128::try_from(a).unwrap_or(1)
}

const OVERFLOW: &str = "overflow: the exact result does not fit in 128 bits";

impl Rational {
    /// `num / den`, reduced. Errors when `den` is zero or the reduced value does not fit.
    pub fn new(num: i128, den: i128) -> Result<Self, String> {
        if den == 0 {
            return Err("division by zero".to_string());
        }
        let g = gcd(num, den).max(1);
        let (mut num, mut den) = (num / g, den / g);
        if den < 0 {
            num = num.checked_neg().ok_or(OVERFLOW)?;
            den = den.checked_neg().ok_or(OVERFLOW)?;
        }
        Ok(Self { num, den })
    }

    #[must_use]
    pub fn int(n: i128) -> Self {
        Self { num: n, den: 1 }
    }

    #[must_use]
    pub fn numerator(&self) -> i128 {
        self.num
    }

    #[must_use]
    pub fn denominator(&self) -> i128 {
        self.den
    }

    pub fn checked_add(self, o: Self) -> Result<Self, String> {
        let num = self
            .num
            .checked_mul(o.den)
            .and_then(|a| o.num.checked_mul(self.den).and_then(|b| a.checked_add(b)))
            .ok_or(OVERFLOW)?;
        Self::new(num, self.den.checked_mul(o.den).ok_or(OVERFLOW)?)
    }

    pub fn checked_neg(self) -> Result<Self, String> {
        Ok(Self {
            num: self.num.checked_neg().ok_or(OVERFLOW)?,
            den: self.den,
        })
    }

    pub fn checked_sub(self, o: Self) -> Result<Self, String> {
        self.checked_add(o.checked_neg()?)
    }

    pub fn checked_mul(self, o: Self) -> Result<Self, String> {
        let num = self.num.checked_mul(o.num).ok_or(OVERFLOW)?;
        Self::new(num, self.den.checked_mul(o.den).ok_or(OVERFLOW)?)
    }

    pub fn checked_div(self, o: Self) -> Result<Self, String> {
        if o.num == 0 {
            return Err("division by zero".to_string());
        }
        let num = self.num.checked_mul(o.den).ok_or(OVERFLOW)?;
        Self::new(num, self.den.checked_mul(o.num).ok_or(OVERFLOW)?)
    }

    /// `self ^ exp` for an integer exponent (negative means the inverse).
    pub fn checked_pow(self, exp: Self) -> Result<Self, String> {
        if exp.den != 1 {
            return Err("the exponent must be an integer".to_string());
        }
        let e = u32::try_from(exp.num.unsigned_abs()).map_err(|_| OVERFLOW.to_string())?;
        let num = self.num.checked_pow(e).ok_or(OVERFLOW)?;
        let den = self.den.checked_pow(e).ok_or(OVERFLOW)?;
        if exp.num < 0 {
            Self::new(den, num)
        } else {
            Self::new(num, den)
        }
    }

    /// The decimal expansion with at most `places` digits after the point, truncated toward
    /// zero (exact digits, no rounding), and whether digits were cut.
    #[must_use]
    pub fn decimal(&self, places: usize) -> (String, bool) {
        let sign = if self.num < 0 { "-" } else { "" };
        let (n, d) = (self.num.unsigned_abs(), self.den.unsigned_abs());
        let mut out = format!("{sign}{}", n / d);
        let mut rem = n % d;
        if rem != 0 {
            out.push('.');
        }
        let mut digits = 0;
        while rem != 0 && digits < places {
            rem *= 10;
            out.push(char::from(b'0' + u8::try_from(rem / d).unwrap_or(0)));
            rem %= d;
            digits += 1;
        }
        (out, rem != 0)
    }
}

impl fmt::Display for Rational {
    /// `7` for an integer, `7/3` for a fraction.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 {
            write!(f, "{}", self.num)
        } else {
            write!(f, "{}/{}", self.num, self.den)
        }
    }
}

// ---------- parser: expr = term (('+'|'-') term)*, term = factor (('*'|'/') factor)*,
// factor = unary ('^' factor)?, unary = '-' unary | atom, atom = number | '(' expr ')' ----------

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn skip(&mut self) {
        while self.s.get(self.i).is_some_and(u8::is_ascii_whitespace) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) -> bool {
        self.skip();
        if self.s.get(self.i) == Some(&c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn expr(&mut self) -> Result<Rational, String> {
        let mut v = self.term()?;
        loop {
            if self.eat(b'+') {
                v = v.checked_add(self.term()?)?;
            } else if self.eat(b'-') {
                v = v.checked_sub(self.term()?)?;
            } else {
                return Ok(v);
            }
        }
    }

    fn term(&mut self) -> Result<Rational, String> {
        let mut v = self.factor()?;
        loop {
            if self.eat(b'*') {
                v = v.checked_mul(self.factor()?)?;
            } else if self.eat(b'/') {
                v = v.checked_div(self.factor()?)?;
            } else {
                return Ok(v);
            }
        }
    }

    /// `^` is right-associative: `2 ^ 3 ^ 2` is `2 ^ 9`.
    fn factor(&mut self) -> Result<Rational, String> {
        let base = self.unary()?;
        if self.eat(b'^') {
            base.checked_pow(self.factor()?)
        } else {
            Ok(base)
        }
    }

    fn unary(&mut self) -> Result<Rational, String> {
        if self.eat(b'-') {
            self.unary()?.checked_neg()
        } else {
            self.atom()
        }
    }

    fn atom(&mut self) -> Result<Rational, String> {
        if self.eat(b'(') {
            let v = self.expr()?;
            if !self.eat(b')') {
                return Err(format!("expected `)` at position {}", self.i));
            }
            return Ok(v);
        }
        self.skip();
        let start = self.i;
        let (mut num, mut den, mut point) = (0i128, 1i128, false);
        while let Some(&c) = self.s.get(self.i) {
            if c.is_ascii_digit() {
                num = num
                    .checked_mul(10)
                    .and_then(|n| n.checked_add(i128::from(c - b'0')))
                    .ok_or(OVERFLOW)?;
                if point {
                    den = den.checked_mul(10).ok_or(OVERFLOW)?;
                }
            } else if c == b'.' && !point {
                point = true;
            } else {
                break;
            }
            self.i += 1;
        }
        if self.i == start || (point && self.i == start + 1) {
            return Err(format!("expected a number at position {start}"));
        }
        Rational::new(num, den)
    }
}

/// Evaluates `expression` exactly. Errors name what is wrong and where.
pub fn eval(expression: &str) -> Result<Rational, String> {
    let mut p = Parser {
        s: expression.as_bytes(),
        i: 0,
    };
    let v = p.expr()?;
    p.skip();
    if p.i != p.s.len() {
        return Err(format!(
            "unexpected `{}` at position {}",
            char::from(p.s[p.i]),
            p.i
        ));
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(n: i128, d: i128) -> Rational {
        Rational::new(n, d).unwrap()
    }

    #[test]
    fn evaluates_exactly() {
        assert_eq!(eval("0.1 + 0.2").unwrap(), r(3, 10));
        assert_eq!(eval("1/3 + 1/6").unwrap(), r(1, 2));
        assert_eq!(eval("2 + 3 * 4").unwrap(), r(14, 1));
        assert_eq!(eval("(2 + 3) * 4").unwrap(), r(20, 1));
        assert_eq!(eval("2 ^ 3 ^ 2").unwrap(), r(512, 1));
        assert_eq!(eval("2 ^ -2").unwrap(), r(1, 4));
        assert_eq!(eval("-3 - -3").unwrap(), r(0, 1));
        assert_eq!(eval("10 / 4").unwrap().to_string(), "5/2");
    }

    #[test]
    fn errors_instead_of_wrong_numbers() {
        assert!(eval("1 / 0").unwrap_err().contains("division by zero"));
        assert!(eval("1 / (2 - 2)")
            .unwrap_err()
            .contains("division by zero"));
        assert!(eval("10 ^ 40").unwrap_err().contains("overflow"));
        assert!(eval("2 ^ 0.5").unwrap_err().contains("integer"));
        assert!(eval("2 +").unwrap_err().contains("expected a number"));
        assert!(eval("(1 + 2").unwrap_err().contains("expected `)`"));
        assert!(eval("1 2").unwrap_err().contains("unexpected `2`"));
        assert!(eval("").is_err());
    }

    #[test]
    fn decimal_digits_are_exact_and_say_when_cut() {
        assert_eq!(r(7, 3).decimal(5), ("2.33333".to_string(), true));
        assert_eq!(r(-1, 8).decimal(5), ("-0.125".to_string(), false));
        assert_eq!(r(4, 1).decimal(5), ("4".to_string(), false));
    }
}
