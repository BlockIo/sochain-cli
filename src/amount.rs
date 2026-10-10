//! Coin amounts. The API sends them as decimal strings with 8 decimal places;
//! they are held as whole units of the smallest denomination (satoshis,
//! litoshis, koinu, zatoshis: 100,000,000 to the coin) so no precision is lost
//! to floating point.

use std::fmt;

use serde::{Deserialize, Deserializer, de};

pub const UNITS_PER_COIN: i128 = 100_000_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Amount(pub i128);

impl Amount {
    /// Parses a decimal string such as `"12400.5"` or `"-0.00000001"`.
    pub fn parse(s: &str) -> Option<Amount> {
        let s = s.trim();
        let (negative, digits) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s.strip_prefix('+').unwrap_or(s)),
        };
        let (whole, frac) = digits.split_once('.').unwrap_or((digits, ""));
        if whole.is_empty() && frac.is_empty()
            || frac.len() > 8
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || !frac.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let whole: i128 = if whole.is_empty() {
            0
        } else {
            whole.parse().ok()?
        };
        let frac: i128 = format!("{frac:0<8}").parse().ok()?;
        let units = whole.checked_mul(UNITS_PER_COIN)?.checked_add(frac)?;
        Some(Amount(if negative { -units } else { units }))
    }

    pub fn is_negative(self) -> bool {
        self.0 < 0
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn abs(self) -> Amount {
        Amount(self.0.abs())
    }

    pub fn to_f64(self) -> f64 {
        self.0 as f64 / UNITS_PER_COIN as f64
    }

    /// At most `places` decimal places (rounded half away from zero), with
    /// thousands separators and trailing zeros removed: `1,234.5`.
    pub fn display(self, places: u32) -> String {
        let places = places.min(8);
        let step = 10i128.pow(8 - places);
        let rounded = (self.0.abs() + step / 2) / step * step;
        let whole = rounded / UNITS_PER_COIN;
        let frac = rounded % UNITS_PER_COIN;
        let mut out = String::new();
        if self.0 < 0 && rounded != 0 {
            out.push('-');
        }
        out.push_str(&group_thousands(whole));
        let frac = format!("{frac:08}");
        let frac = frac[..places as usize].trim_end_matches('0');
        if !frac.is_empty() {
            out.push('.');
            out.push_str(frac);
        }
        out
    }
}

/// Full precision, trailing zeros removed.
impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display(8))
    }
}

impl<'de> Deserialize<'de> for Amount {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Amount::parse(&s).ok_or_else(|| de::Error::custom(format!("invalid amount {s:?}")))
    }
}

pub fn group_thousands(n: i128) -> String {
    let digits = n.abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 {
        out.insert(0, '-');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_decimal_strings() {
        assert_eq!(
            Amount::parse("12400.00000000"),
            Some(Amount(12400 * UNITS_PER_COIN))
        );
        assert_eq!(Amount::parse("0.00000001"), Some(Amount(1)));
        assert_eq!(Amount::parse("-1.5"), Some(Amount(-150_000_000)));
        assert_eq!(Amount::parse(".5"), Some(Amount(50_000_000)));
        assert_eq!(Amount::parse("7"), Some(Amount(7 * UNITS_PER_COIN)));
    }

    #[test]
    fn rejects_malformed_amounts() {
        for bad in ["", "-", ".", "1.123456789", "1e5", "abc", "1.2.3", " - 1"] {
            assert_eq!(Amount::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn displays_with_grouping_and_rounding() {
        let a = Amount::parse("1234567.89000000").unwrap();
        assert_eq!(a.to_string(), "1,234,567.89");
        assert_eq!(a.display(0), "1,234,568");
        assert_eq!(
            Amount::parse("0.00000001").unwrap().to_string(),
            "0.00000001"
        );
        assert_eq!(Amount::parse("0.00000001").unwrap().display(2), "0");
        assert_eq!(Amount::parse("-2.5").unwrap().display(0), "-3");
        assert_eq!(Amount::parse("-0.001").unwrap().display(2), "0");
    }

    #[test]
    fn groups_thousands() {
        assert_eq!(group_thousands(0), "0");
        assert_eq!(group_thousands(999), "999");
        assert_eq!(group_thousands(1000), "1,000");
        assert_eq!(group_thousands(-1234567), "-1,234,567");
    }
}
