//! Exact money amounts: integer counts of 1/[`Units::SCALE`] of a currency
//! unit, the same scale manual splits are stored at
//! (`transactions::VALUE_DENOM`). Sums of these are exact, so balances and
//! statement differences compare with `==`, never a tolerance. FX conversions
//! and market values are estimates: they go through `f64` and are rounded back
//! with [`Units::from_f64`].
//!
//! Crosses IPC as a branded `Units` number; `$lib/utils/units.ts` converts
//! and formats it on the frontend.

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};

use rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Units(i64);

/// Exports as `number & { readonly __brand: "Units" }`, so a plain number
/// can't be passed where units are expected. The payload is declared as `i32`
/// only because specta refuses `i64` inside a brand (the bindings' bigint cast
/// doesn't reach it); every value stays far below 2^53.
impl specta::Type for Units {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        let ty = types.define::<i32>();
        specta::datatype::DataType::Reference(specta::datatype::Reference::opaque(
            specta_typescript::Branded::new("Units", ty),
        ))
    }
}

/// Signed split value in [`Units`], for `SUM(...)` without floats. Integer
/// math when the stored denominator divides the scale (every split Spendo
/// writes and the usual 1/100 imports); any other denominator is rounded per
/// split before summing, so totals stay integers.
pub const VALUE_UNITS: &str = "(CASE WHEN 10000 % s.value_denom = 0 \
     THEN s.value_num * (10000 / s.value_denom) \
     ELSE CAST(ROUND(s.value_num * 10000.0 / s.value_denom) AS INTEGER) END)";

impl Units {
    /// Units per whole currency unit (mirrored as `UNIT_SCALE` in `units.ts`).
    pub const SCALE: i64 = 10_000;
    pub const ZERO: Units = Units(0);

    pub const fn new(units: i64) -> Self {
        Units(units)
    }

    /// Whole currency units, for estimates (FX, prices) and charts.
    pub fn to_f64(self) -> f64 {
        self.0 as f64 / Self::SCALE as f64
    }

    /// Round an estimated amount (FX conversion, market value) to the nearest
    /// unit. Non-finite inputs read as zero.
    pub fn from_f64(amount: f64) -> Self {
        if !amount.is_finite() {
            return Units(0);
        }
        Units((amount * Self::SCALE as f64).round() as i64)
    }

    pub fn abs(self) -> Self {
        Units(self.0.abs())
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }
}

impl Add for Units {
    type Output = Units;
    fn add(self, rhs: Units) -> Units {
        Units(self.0 + rhs.0)
    }
}

impl Sub for Units {
    type Output = Units;
    fn sub(self, rhs: Units) -> Units {
        Units(self.0 - rhs.0)
    }
}

impl Neg for Units {
    type Output = Units;
    fn neg(self) -> Units {
        Units(-self.0)
    }
}

impl AddAssign for Units {
    fn add_assign(&mut self, rhs: Units) {
        self.0 += rhs.0;
    }
}

impl SubAssign for Units {
    fn sub_assign(&mut self, rhs: Units) {
        self.0 -= rhs.0;
    }
}

impl Sum for Units {
    fn sum<I: Iterator<Item = Units>>(iter: I) -> Units {
        Units(iter.map(|u| u.0).sum())
    }
}

impl<'a> Sum<&'a Units> for Units {
    fn sum<I: Iterator<Item = &'a Units>>(iter: I) -> Units {
        iter.copied().sum()
    }
}

/// Plain decimal with at least two fraction digits (`-12.30`, `0.1234`), for
/// validation messages. The UI formats amounts itself.
impl fmt::Display for Units {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        let scale = Self::SCALE as u64;
        let whole = abs / scale;
        let mut frac = format!("{:04}", abs % scale);
        while frac.len() > 2 && frac.ends_with('0') {
            frac.pop();
        }
        write!(f, "{sign}{whole}.{frac}")
    }
}

impl FromSql for Units {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        i64::column_result(value).map(Units)
    }
}

impl ToSql for Units {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn scale_matches_stored_value_denominator() {
        assert_eq!(Units::SCALE, crate::transactions::VALUE_DENOM);
        assert!(VALUE_UNITS.contains(&Units::SCALE.to_string()));
    }

    #[test]
    fn display_keeps_two_decimals_and_trims_the_rest() {
        assert_eq!(Units::new(-123_000).to_string(), "-12.30");
        assert_eq!(Units::new(1_234).to_string(), "0.1234");
        assert_eq!(Units::new(50).to_string(), "0.005");
        assert_eq!(Units::ZERO.to_string(), "0.00");
    }

    #[test]
    fn from_f64_rounds_estimates() {
        assert_eq!(Units::from_f64(0.1 + 0.2), Units::new(3_000));
        assert_eq!(Units::from_f64(-1.23456), Units::new(-12_346));
        assert_eq!(Units::from_f64(f64::NAN), Units::ZERO);
    }

    #[test]
    fn value_units_sums_any_denominator_as_integers() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE splits (value_num INTEGER, value_denom INTEGER);
             INSERT INTO splits VALUES (10, 100), (20, 100), (-30, 100),
                                       (1, 3), (12345, 10000), (7, 1);",
        )
        .unwrap();
        let (total, kind): (Units, String) = conn
            .query_row(
                &format!("SELECT SUM({VALUE_UNITS}), typeof(SUM({VALUE_UNITS})) FROM splits s"),
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(kind, "integer");
        // 0.1 + 0.2 - 0.3 is exactly 0; 1/3 rounds to 3333; 1.2345; 7.
        assert_eq!(total, Units::new(3_333 + 12_345 + 70_000));
    }
}
