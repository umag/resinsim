use std::fmt;

use serde::{Deserialize, Serialize};

/// Crack-front fraction — the share of a layer's resin↔resin interlayer bond
/// that has released as the peel crack advances across it. Dimensionless,
/// clamped to `[0, 1]`: `0.0` = fully bonded (no crack), `1.0` = fully
/// delaminated.
///
/// This is the CAPACITY-side companion to [`PeelForce`](crate::values::PeelForce)
/// (a LOAD). It reduces the interlayer-bond holding capacity via
/// [`effective_fraction`](Self::effective_fraction); it never touches the peel
/// load. KB-188 (Kendall crack-front-width), KB-191 (periphery→centre CZM),
/// KB-116 (weak FEP LOAD vs strong crosslinked interlayer CAPACITY).
///
/// Unlike the other value objects (`area.rs`, `force.rs`) whose `new` returns a
/// `Result`, [`CrackFront::new`] CLAMPS rather than errors: a crack fraction is
/// a DERIVED geometric quantity (`1 − Kendall knockdown`), always producible
/// from a valid area/perimeter, so an out-of-range input is saturated (mirroring
/// the `min(1, ·)` clamp in [`CrackPropagator`](crate::services::CrackPropagator))
/// rather than rejected.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct CrackFront(f32);

impl CrackFront {
    /// Construct from a raw crack fraction, clamping into `[0, 1]`. `NaN` maps
    /// to `0.0` (no crack — the safe, behaviour-preserving reading).
    pub fn new(fraction: f32) -> Self {
        if fraction.is_nan() {
            return Self(0.0);
        }
        Self(fraction.clamp(0.0, 1.0))
    }

    /// A fully-bonded front (fraction `0.0`) — no crack. The behaviour-preserving
    /// default passed by callers (bottom layers, placeholder masks) that have no
    /// crack geometry.
    pub fn no_crack() -> Self {
        Self(0.0)
    }

    /// A fully-delaminated front (fraction `1.0`).
    pub fn full() -> Self {
        Self(1.0)
    }

    /// The crack fraction in `[0, 1]`.
    pub fn value(&self) -> f32 {
        self.0
    }

    /// The remaining bonded fraction, `1 − crack_fraction`, in `[0, 1]`. This is
    /// the multiplier applied to the interlayer-bond holding capacity for a
    /// normal layer.
    pub fn effective_fraction(&self) -> f32 {
        1.0 - self.0
    }

    /// Whether the crack front has effectively fully released (`>= 0.99`).
    pub fn is_delaminated(&self) -> bool {
        self.0 >= 0.99
    }
}

impl fmt::Display for CrackFront {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_stores_in_range_fraction() {
        assert_eq!(CrackFront::new(0.5).value(), 0.5);
        assert_eq!(CrackFront::new(0.0).value(), 0.0);
        assert_eq!(CrackFront::new(1.0).value(), 1.0);
    }

    #[test]
    fn new_clamps_below_zero_to_zero() {
        assert_eq!(CrackFront::new(-0.5).value(), 0.0);
    }

    #[test]
    fn new_clamps_above_one_to_one() {
        assert_eq!(CrackFront::new(1.5).value(), 1.0);
    }

    #[test]
    fn new_maps_nan_to_no_crack() {
        assert_eq!(CrackFront::new(f32::NAN).value(), 0.0);
    }

    #[test]
    fn no_crack_is_zero() {
        assert_eq!(CrackFront::no_crack().value(), 0.0);
    }

    #[test]
    fn full_is_one() {
        assert_eq!(CrackFront::full().value(), 1.0);
    }

    #[test]
    fn effective_fraction_is_one_minus_crack() {
        assert!((CrackFront::new(0.0).effective_fraction() - 1.0).abs() < 1e-6);
        assert!((CrackFront::new(0.6).effective_fraction() - 0.4).abs() < 1e-6);
        assert!((CrackFront::new(1.0).effective_fraction() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn no_crack_effective_fraction_is_one_behaviour_preserving() {
        // The default passed at capacity composition — full bonded area retained.
        assert_eq!(CrackFront::no_crack().effective_fraction(), 1.0);
    }

    #[test]
    fn is_delaminated_true_at_threshold() {
        assert!(CrackFront::new(0.99).is_delaminated());
        assert!(CrackFront::new(1.0).is_delaminated());
        assert!(CrackFront::full().is_delaminated());
    }

    #[test]
    fn is_delaminated_false_below_threshold() {
        assert!(!CrackFront::new(0.0).is_delaminated());
        assert!(!CrackFront::new(0.98).is_delaminated());
        assert!(!CrackFront::no_crack().is_delaminated());
    }

    #[test]
    fn display_two_decimals() {
        assert_eq!(format!("{}", CrackFront::new(0.5)), "0.50");
        assert_eq!(format!("{}", CrackFront::no_crack()), "0.00");
        assert_eq!(format!("{}", CrackFront::full()), "1.00");
    }

    #[test]
    fn ord_by_fraction() {
        assert!(CrackFront::new(0.2) < CrackFront::new(0.8));
        assert!(CrackFront::no_crack() < CrackFront::full());
    }
}
