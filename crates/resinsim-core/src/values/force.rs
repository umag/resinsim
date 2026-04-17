use std::fmt;

use serde::{Deserialize, Serialize};

/// Peel force — force to separate cured layer from FEP film. Unit: Newtons.
/// Computed as σ_peel × A_layer × f(v_lift) + suction. KB-114.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct PeelForce(pub(crate) f32);

/// Support capacity — maximum force supports can resist. Unit: Newtons.
/// Computed as σ_tensile × π × r²_tip × N_supports. KB-114.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct SupportCapacity(pub(crate) f32);

/// Safety factor — ratio of support capacity to peel force. Dimensionless.
/// SF > 1.0 = safe, SF = 1.0 = marginal, SF < 1.0 = failure.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct SafetyFactor(pub(crate) f32);

impl PeelForce {
    pub fn new(newtons: f32) -> Result<Self, String> {
        if !newtons.is_finite() {
            return Err(format!("peel force must be finite, got {newtons}"));
        }
        if newtons < 0.0 {
            return Err(format!("peel force must be non-negative, got {newtons}"));
        }
        Ok(Self(newtons))
    }

    pub fn value(&self) -> f32 {
        self.0
    }
}

impl SupportCapacity {
    pub fn new(newtons: f32) -> Result<Self, String> {
        if !newtons.is_finite() {
            return Err(format!("support capacity must be finite, got {newtons}"));
        }
        if newtons < 0.0 {
            return Err(format!("support capacity must be non-negative, got {newtons}"));
        }
        Ok(Self(newtons))
    }

    pub fn value(&self) -> f32 {
        self.0
    }
}

impl SafetyFactor {
    /// Construct a safety factor from a known ratio.
    /// Allows `f32::INFINITY` because the `compute()` constructor returns that
    /// sentinel when peel force is zero; callers of `new()` must supply finite
    /// values only.
    pub fn new(ratio: f32) -> Result<Self, String> {
        if !ratio.is_finite() {
            return Err(format!("safety factor must be finite, got {ratio}"));
        }
        if ratio < 0.0 {
            return Err(format!("safety factor must be non-negative, got {ratio}"));
        }
        Ok(Self(ratio))
    }

    /// Compute safety factor from support capacity and peel force.
    /// Invariant: SF = capacity / force.
    /// Returns `f32::INFINITY` when the peel force is zero (no load).
    pub fn compute(capacity: SupportCapacity, force: PeelForce) -> Self {
        if force.0 <= 0.0 {
            return Self(f32::INFINITY);
        }
        Self(capacity.0 / force.0)
    }

    pub fn is_safe(&self) -> bool {
        self.0 > 1.0
    }

    pub fn is_marginal(&self) -> bool {
        self.0 > 0.8 && self.0 <= 1.5
    }

    pub fn value(&self) -> f32 {
        self.0
    }
}

impl fmt::Display for PeelForce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2} N", self.0)
    }
}

impl fmt::Display for SupportCapacity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2} N", self.0)
    }
}

impl fmt::Display for SafetyFactor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safety_factor_is_capacity_over_force() {
        // KB-114 invariant: SF = F_max / F_total
        let sf = SafetyFactor::compute(SupportCapacity(37.7), PeelForce(10.0));
        assert!((sf.value() - 3.77).abs() < 0.01);
    }

    #[test]
    fn safety_factor_one_at_equal() {
        let sf = SafetyFactor::compute(SupportCapacity(37.7), PeelForce(37.7));
        assert!((sf.value() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn safety_factor_below_one_is_failure() {
        // KB-114: F_total > F_max → FAIL
        let sf = SafetyFactor::compute(SupportCapacity(37.7), PeelForce(50.0));
        assert!(!sf.is_safe());
        assert!((sf.value() - 0.754).abs() < 0.001);
    }

    #[test]
    fn safety_factor_infinity_for_zero_force() {
        let sf = SafetyFactor::compute(SupportCapacity(37.7), PeelForce(0.0));
        assert!(sf.value().is_infinite());
    }

    #[test]
    fn peel_force_display() {
        assert_eq!(format!("{}", PeelForce(32.5)), "32.50 N");
    }

    #[test]
    fn peel_force_new_rejects_nan() {
        assert!(PeelForce::new(f32::NAN).is_err());
    }

    #[test]
    fn peel_force_new_rejects_infinity() {
        assert!(PeelForce::new(f32::INFINITY).is_err());
    }

    #[test]
    fn peel_force_new_rejects_negative() {
        assert!(PeelForce::new(-1.0).is_err());
    }

    #[test]
    fn peel_force_new_accepts_zero() {
        assert_eq!(PeelForce::new(0.0).unwrap().value(), 0.0);
    }

    #[test]
    fn support_capacity_new_rejects_nan() {
        assert!(SupportCapacity::new(f32::NAN).is_err());
    }

    #[test]
    fn support_capacity_new_rejects_negative() {
        assert!(SupportCapacity::new(-0.5).is_err());
    }

    #[test]
    fn support_capacity_new_accepts_zero() {
        assert_eq!(SupportCapacity::new(0.0).unwrap().value(), 0.0);
    }

    #[test]
    fn safety_factor_new_rejects_nan() {
        assert!(SafetyFactor::new(f32::NAN).is_err());
    }

    #[test]
    fn safety_factor_new_rejects_infinity() {
        // new() rejects infinity; compute() is the sentinel-producing path.
        assert!(SafetyFactor::new(f32::INFINITY).is_err());
    }

    #[test]
    fn safety_factor_new_rejects_negative() {
        assert!(SafetyFactor::new(-1.0).is_err());
    }
}
