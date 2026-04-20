use std::fmt;

use serde::{Deserialize, Serialize};

/// Cross-section area of a cured layer. Unit: mm².
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct CrossSectionArea(f64);

/// Change in cross-section area between consecutive layers. Unit: mm².
/// Large positive delta = sudden area increase = force spike risk.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct AreaDelta(f64);

impl CrossSectionArea {
    pub fn new(mm2: f64) -> Result<Self, String> {
        if !mm2.is_finite() {
            return Err(format!("cross-section area must be finite, got {mm2}"));
        }
        if mm2 < 0.0 {
            return Err(format!(
                "cross-section area must be non-negative, got {mm2}"
            ));
        }
        Ok(Self(mm2))
    }

    /// Area of a circle with given diameter (mm).
    /// Returns Err if diameter is negative or non-finite.
    pub fn circle(diameter_mm: f64) -> Result<Self, String> {
        if !diameter_mm.is_finite() {
            return Err(format!("circle diameter must be finite, got {diameter_mm}"));
        }
        if diameter_mm < 0.0 {
            return Err(format!(
                "circle diameter must be non-negative, got {diameter_mm}"
            ));
        }
        Ok(Self(std::f64::consts::PI * (diameter_mm / 2.0).powi(2)))
    }

    pub fn value(&self) -> f64 {
        self.0
    }
}

impl AreaDelta {
    pub fn new(mm2: f64) -> Result<Self, String> {
        if !mm2.is_finite() {
            return Err(format!("area delta must be finite, got {mm2}"));
        }
        Ok(Self(mm2))
    }

    pub fn between(current: CrossSectionArea, previous: CrossSectionArea) -> Self {
        Self(current.0 - previous.0)
    }

    pub fn value(&self) -> f64 {
        self.0
    }
}

impl fmt::Display for CrossSectionArea {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.1} mm²", self.0)
    }
}

impl fmt::Display for AreaDelta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:+.1} mm²", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_area_10mm_diameter() {
        // KB-172: 10mm cylinder → 78.5 mm²
        let a = CrossSectionArea::circle(10.0)
            .expect("test fixture: 10.0 mm is in CrossSectionArea::circle domain");
        assert!((a.value() - 78.54).abs() < 0.01);
    }

    #[test]
    fn circle_area_30mm_diameter() {
        // KB-172: 30mm cylinder → 706.9 mm²
        let a = CrossSectionArea::circle(30.0)
            .expect("test fixture: 30.0 mm is in CrossSectionArea::circle domain");
        assert!((a.value() - 706.86).abs() < 0.01);
    }

    #[test]
    fn area_delta_positive_for_increase() {
        let d = AreaDelta::between(
            CrossSectionArea::new(100.0)
                .expect("test fixture: 100.0 mm² is in CrossSectionArea domain"),
            CrossSectionArea::new(50.0)
                .expect("test fixture: 50.0 mm² is in CrossSectionArea domain"),
        );
        assert!((d.value() - 50.0).abs() < 1e-6);
    }

    #[test]
    fn area_delta_negative_for_decrease() {
        let d = AreaDelta::between(
            CrossSectionArea::new(50.0)
                .expect("test fixture: 50.0 mm² is in CrossSectionArea domain"),
            CrossSectionArea::new(100.0)
                .expect("test fixture: 100.0 mm² is in CrossSectionArea domain"),
        );
        assert!((d.value() - (-50.0)).abs() < 1e-6);
    }

    #[test]
    fn area_display() {
        assert_eq!(
            format!(
                "{}",
                CrossSectionArea::new(78.5)
                    .expect("test fixture: 78.5 mm² is in CrossSectionArea domain")
            ),
            "78.5 mm²"
        );
    }

    #[test]
    fn delta_display_shows_sign() {
        assert_eq!(
            format!(
                "{}",
                AreaDelta::new(50.0).expect("test fixture: 50.0 mm² is in AreaDelta domain")
            ),
            "+50.0 mm²"
        );
        assert_eq!(
            format!(
                "{}",
                AreaDelta::new(-30.0).expect("test fixture: -30.0 mm² is in AreaDelta domain")
            ),
            "-30.0 mm²"
        );
    }

    #[test]
    fn cross_section_area_new_rejects_nan() {
        assert!(CrossSectionArea::new(f64::NAN).is_err());
    }

    #[test]
    fn cross_section_area_new_rejects_negative() {
        assert!(CrossSectionArea::new(-1.0).is_err());
    }

    #[test]
    fn cross_section_area_new_accepts_zero() {
        assert_eq!(
            CrossSectionArea::new(0.0)
                .expect("test fixture: 0.0 mm² is in CrossSectionArea domain")
                .value(),
            0.0
        );
    }

    #[test]
    fn cross_section_area_new_rejects_infinity() {
        assert!(CrossSectionArea::new(f64::INFINITY).is_err());
    }

    #[test]
    fn area_delta_new_rejects_nan() {
        assert!(AreaDelta::new(f64::NAN).is_err());
    }

    #[test]
    fn area_delta_new_accepts_negative() {
        // Negative delta is valid (shrinking layer).
        assert_eq!(
            AreaDelta::new(-5.0)
                .expect("test fixture: -5.0 mm² is in AreaDelta domain")
                .value(),
            -5.0
        );
    }

    #[test]
    fn area_delta_new_rejects_infinity() {
        assert!(AreaDelta::new(f64::INFINITY).is_err());
    }

    // --- Step 12: circle regression tests ---

    #[test]
    fn circle_valid_diameter() {
        let a = CrossSectionArea::circle(10.0)
            .expect("test fixture: 10.0 mm is in CrossSectionArea::circle domain");
        assert!((a.value() - 78.54).abs() < 0.01);
    }

    #[test]
    fn circle_rejects_negative_diameter() {
        assert!(CrossSectionArea::circle(-1.0).is_err());
    }

    #[test]
    fn circle_rejects_nan_diameter() {
        assert!(CrossSectionArea::circle(f64::NAN).is_err());
    }

    #[test]
    fn circle_zero_diameter_is_zero_area() {
        let a = CrossSectionArea::circle(0.0)
            .expect("test fixture: 0.0 mm is in CrossSectionArea::circle domain");
        assert!((a.value()).abs() < 1e-10);
    }
}
