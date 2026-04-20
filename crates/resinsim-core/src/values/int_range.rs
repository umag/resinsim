use std::fmt;

use serde::{Deserialize, Serialize};

/// A closed range `[min, max]` over `u32`. Sibling to `FloatRange` for integer-valued
/// hardware envelopes (e.g. a hypothetical `bottom_layer_count_range`).
///
/// **Status:** no consumer today — ADR-0005 §2 decided `bottom_layer_count_max` is a
/// scalar `u32`, not a range. `IntRange` is built alongside `FloatRange` per the plan
/// for completeness; if no consumer emerges in a follow-up, revisit removing it.
///
/// Constructed via `new(min, max)` which enforces `min <= max`. `u32` cannot be negative
/// so there is no NaN/finite check; `min == max` is allowed (zero-width range).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntRange {
    pub(crate) min: u32,
    pub(crate) max: u32,
}

impl IntRange {
    pub fn new(min: u32, max: u32) -> Result<Self, String> {
        let r = Self { min, max };
        r.validate()?;
        Ok(r)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.min > self.max {
            return Err(format!(
                "range.min ({}) must be <= range.max ({})",
                self.min, self.max
            ));
        }
        Ok(())
    }

    pub fn min(&self) -> u32 {
        self.min
    }

    pub fn max(&self) -> u32 {
        self.max
    }

    pub fn contains(&self, value: u32) -> bool {
        value >= self.min && value <= self.max
    }
}

impl fmt::Display for IntRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}, {}]", self.min, self.max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_accepts_ordered_range() {
        let r = IntRange::new(1, 10).expect("test fixture: 1..10 is a valid range");
        assert_eq!(r.min(), 1);
        assert_eq!(r.max(), 10);
    }

    #[test]
    fn new_accepts_zero_min() {
        // Unlike FloatRange, zero is valid for u32 — e.g. a range starting at 0 bottom layers.
        let r = IntRange::new(0, 6).expect("test fixture: 0..6 is a valid range");
        assert!(r.contains(0));
        assert!(r.contains(6));
    }

    #[test]
    fn new_accepts_zero_width_range() {
        let r = IntRange::new(5, 5).expect("test fixture: zero-width at 5 is valid");
        assert!(r.contains(5));
        assert!(!r.contains(4));
        assert!(!r.contains(6));
    }

    #[test]
    fn new_rejects_min_greater_than_max() {
        let err = IntRange::new(10, 1).expect_err("min > max must fail");
        assert!(
            err.contains("min") && err.contains("max"),
            "error names fields: {err}"
        );
    }

    #[test]
    fn contains_inclusive_at_both_bounds() {
        let r = IntRange::new(1, 10).expect("test fixture: 1..10 is a valid range");
        assert!(r.contains(1));
        assert!(r.contains(10));
        assert!(r.contains(5));
        assert!(!r.contains(0));
        assert!(!r.contains(11));
    }

    #[test]
    fn parse_toml_then_validate_rejects_min_greater_than_max() {
        let toml_str = r#"
min = 10
max = 1
"#;
        let r: IntRange =
            toml::from_str(toml_str).expect("TOML parse succeeds; validate() is the gate");
        let err = r
            .validate()
            .expect_err("inverted bounds must fail validate()");
        assert!(
            err.contains("min") && err.contains("max"),
            "error names fields: {err}"
        );
    }

    #[test]
    fn parse_toml_then_validate_accepts_valid() {
        let toml_str = r#"
min = 1
max = 10
"#;
        let r: IntRange = toml::from_str(toml_str).expect("TOML parse succeeds for valid range");
        r.validate().expect("valid range must satisfy validate()");
        assert!(r.contains(5));
    }

    #[test]
    fn display_format() {
        let r = IntRange::new(1, 10).expect("test fixture: 1..10 is a valid range");
        assert_eq!(format!("{r}"), "[1, 10]");
    }
}
