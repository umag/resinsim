use std::fmt;

use serde::{Deserialize, Serialize};

/// A closed range `[min, max]` over positive finite `f32`.
/// Used by `PrinterProfile` hardware envelope fields (e.g. `layer_height_range_um`).
///
/// Constructed via `new(min, max)` which enforces:
/// - both bounds are finite (not NaN, not ±∞)
/// - both bounds are strictly positive (> 0)
/// - `min <= max` (equal bounds are allowed — a zero-width range pins the value)
///
/// Serde deserialisation bypasses `new()`. Consumers MUST call `validate()` after
/// deserialising from TOML/JSON — the owning entity's `validate()` should delegate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FloatRange {
    pub(crate) min: f32,
    pub(crate) max: f32,
}

impl FloatRange {
    pub fn new(min: f32, max: f32) -> Result<Self, String> {
        let r = Self { min, max };
        r.validate()?;
        Ok(r)
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.min.is_finite() {
            return Err(format!("range.min must be finite (got {})", self.min));
        }
        if !self.max.is_finite() {
            return Err(format!("range.max must be finite (got {})", self.max));
        }
        if self.min <= 0.0 {
            return Err(format!("range.min must be > 0 (got {})", self.min));
        }
        if self.max <= 0.0 {
            return Err(format!("range.max must be > 0 (got {})", self.max));
        }
        if self.min > self.max {
            return Err(format!(
                "range.min ({}) must be <= range.max ({})",
                self.min, self.max
            ));
        }
        Ok(())
    }

    pub fn min(&self) -> f32 {
        self.min
    }

    pub fn max(&self) -> f32 {
        self.max
    }

    /// Whether `value` lies within `[min, max]`. NaN returns false (IEEE 754 comparison
    /// semantics), which is the correct answer — a NaN recipe field should never be
    /// "contained" in any range, even if the range bounds are NaN themselves. The upstream
    /// Recipe::validate() rejects NaN recipe fields before pairing is evaluated
    /// (see ADR-0005 §5 pairing-validator trust contract).
    pub fn contains(&self, value: f32) -> bool {
        value >= self.min && value <= self.max
    }
}

impl fmt::Display for FloatRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}, {}]", self.min, self.max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_accepts_positive_range() {
        let r =
            FloatRange::new(20.0, 100.0).expect("test fixture: 20..100 is a valid positive range");
        assert_eq!(r.min(), 20.0);
        assert_eq!(r.max(), 100.0);
    }

    #[test]
    fn new_accepts_zero_width_range() {
        // Equal bounds pin the value — legitimate for fixed-parameter hardware.
        let r =
            FloatRange::new(50.0, 50.0).expect("test fixture: zero-width range at 50.0 is valid");
        assert!(r.contains(50.0));
        assert!(!r.contains(49.9));
        assert!(!r.contains(50.1));
    }

    #[test]
    fn new_rejects_min_greater_than_max() {
        let err = FloatRange::new(100.0, 20.0).expect_err("min > max must fail");
        assert!(
            err.contains("min") && err.contains("max"),
            "error names offending fields: {err}"
        );
    }

    #[test]
    fn new_rejects_nan_min() {
        assert!(FloatRange::new(f32::NAN, 100.0).is_err());
    }

    #[test]
    fn new_rejects_nan_max() {
        assert!(FloatRange::new(20.0, f32::NAN).is_err());
    }

    #[test]
    fn new_rejects_infinity_min() {
        assert!(FloatRange::new(f32::INFINITY, 100.0).is_err());
        assert!(FloatRange::new(f32::NEG_INFINITY, 100.0).is_err());
    }

    #[test]
    fn new_rejects_infinity_max() {
        assert!(FloatRange::new(20.0, f32::INFINITY).is_err());
        assert!(FloatRange::new(20.0, f32::NEG_INFINITY).is_err());
    }

    #[test]
    fn new_rejects_zero_min() {
        assert!(FloatRange::new(0.0, 100.0).is_err());
    }

    #[test]
    fn new_rejects_negative_min() {
        assert!(FloatRange::new(-1.0, 100.0).is_err());
    }

    #[test]
    fn new_rejects_zero_max() {
        assert!(FloatRange::new(20.0, 0.0).is_err());
    }

    #[test]
    fn new_rejects_negative_max() {
        assert!(FloatRange::new(20.0, -1.0).is_err());
    }

    #[test]
    fn contains_inclusive_at_min_and_max() {
        let r =
            FloatRange::new(20.0, 100.0).expect("test fixture: 20..100 is a valid positive range");
        assert!(r.contains(20.0), "min boundary is inclusive");
        assert!(r.contains(100.0), "max boundary is inclusive");
        assert!(r.contains(50.0));
    }

    #[test]
    fn contains_excludes_outside() {
        let r =
            FloatRange::new(20.0, 100.0).expect("test fixture: 20..100 is a valid positive range");
        assert!(!r.contains(19.9));
        assert!(!r.contains(100.1));
    }

    #[test]
    fn contains_rejects_nan_value() {
        // IEEE 754: NaN comparisons are always false, so contains(NaN) is false.
        // This is the correct answer but also fragile — the nan-two-layer-defence
        // pattern requires NaN to be caught at Recipe::validate() BEFORE pairing.
        // See ADR-0005 §5 trust contract.
        let r =
            FloatRange::new(20.0, 100.0).expect("test fixture: 20..100 is a valid positive range");
        assert!(!r.contains(f32::NAN));
    }

    // --- Parse-path (serde) tests: the anti-pattern gap in
    // docs/patterns/anti/rust-nan-positive-validation-gap.md says a positive-only check
    // accepts NaN. These tests lock the parse-then-validate() loop so a future repository
    // refactor that skips validate() surfaces as a test failure. ---

    #[test]
    fn parse_toml_then_validate_rejects_nan_min() {
        let toml_str = r#"
min = nan
max = 100.0
"#;
        let r: FloatRange =
            toml::from_str(toml_str).expect("TOML parse succeeds; validate() is the gate");
        assert!(
            r.validate().is_err(),
            "validate() must reject NaN min from deserialized path"
        );
    }

    #[test]
    fn parse_toml_then_validate_rejects_nan_max() {
        let toml_str = r#"
min = 20.0
max = nan
"#;
        let r: FloatRange =
            toml::from_str(toml_str).expect("TOML parse succeeds; validate() is the gate");
        assert!(
            r.validate().is_err(),
            "validate() must reject NaN max from deserialized path"
        );
    }

    #[test]
    fn parse_toml_then_validate_rejects_min_greater_than_max() {
        let toml_str = r#"
min = 100.0
max = 20.0
"#;
        let r: FloatRange =
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
min = 20.0
max = 100.0
"#;
        let r: FloatRange = toml::from_str(toml_str).expect("TOML parse succeeds for valid range");
        r.validate().expect("valid range must satisfy validate()");
        assert!(r.contains(50.0));
    }

    #[test]
    fn display_format() {
        let r =
            FloatRange::new(20.0, 100.0).expect("test fixture: 20..100 is a valid positive range");
        assert_eq!(format!("{r}"), "[20, 100]");
    }
}
