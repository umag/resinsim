//! Thin adapter around [`CavityDetector`]. Emits [`SuctionRisk`] per
//! topologically-sealed cavity, preserving the existing `SuctionRisk` output
//! shape consumed by `FailurePredictor` via `LayerOverrides.suction_force_n`.
//!
//! The old area-drop heuristic (`detect_heuristic`, `detect_from_areas`) was
//! removed in Phase B of the suction-detector-raft-false-positive lifecycle.
//! It produced false-positive suction events at raft→supports transitions
//! because it could not distinguish fluid-permeable inter-column gaps from
//! sealed ring-wall cavities — both present as a 2D area drop. The 3D
//! [`CavityDetector`] handles both correctly via topology.

use crate::services::{CavityDetector, CavityError};
use crate::values::LayerMask;

/// A detected suction risk. One risk per topologically-sealed cavity, at the
/// layer where the cavity closes from below (FEP direction during peel).
#[derive(Debug, Clone, PartialEq)]
pub struct SuctionRisk {
    /// Layer where suction event occurs.
    pub layer: u32,
    /// Estimated sealed area in mm² (voxel-resolution precision — see
    /// [`LayerMask::solid_area_mm2`] and
    /// [`CavityEvent::sealed_area_mm2`](crate::services::CavityEvent) rustdoc).
    pub sealed_area_mm2: f64,
    /// Estimated suction force in Newtons at [`VACUUM_PRESSURE_KPA`](crate::services::cavity_detector::VACUUM_PRESSURE_KPA).
    pub suction_force_n: f32,
}

/// Stateless domain service. Delegates topology detection to
/// [`CavityDetector`] and maps each `CavityEvent` to a `SuctionRisk`.
pub struct SuctionDetector;

impl SuctionDetector {
    /// Detect suction risks from a stack of per-layer occupancy masks.
    ///
    /// Returns one `SuctionRisk` per topologically-sealed cavity (exterior-
    /// connected voids produce none). See [`CavityDetector::detect`] for
    /// algorithm details and ambient-boundary policy.
    ///
    /// # Errors
    ///
    /// Propagates [`CavityError`] from the underlying detector:
    /// - [`CavityError::NoMasks`] for empty input.
    /// - [`CavityError::InconsistentMasks`] for mixed voxel sizes or
    ///   dimensions across the stack.
    pub fn detect_from_masks(masks: &[LayerMask]) -> Result<Vec<SuctionRisk>, CavityError> {
        let events = CavityDetector::detect(masks)?;
        Ok(events
            .into_iter()
            .map(|e| SuctionRisk {
                layer: e.layer,
                sealed_area_mm2: e.sealed_area_mm2,
                suction_force_n: e.suction_force_n,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_mask(w: u32, h: u32) -> LayerMask {
        LayerMask::new_all_solid(w, h, 1.0).expect("test fixture: valid dimensions")
    }

    fn ring_mask(w: u32, h: u32) -> LayerMask {
        let mut m = LayerMask::new_all_solid(w, h, 1.0).expect("test fixture: valid dimensions");
        // Clear interior
        for x in 1..w - 1 {
            for y in 1..h - 1 {
                m.clear(x, y).expect("in bounds");
            }
        }
        m
    }

    #[test]
    fn detect_from_masks_empty_returns_err() {
        assert!(SuctionDetector::detect_from_masks(&[]).is_err());
    }

    #[test]
    fn detect_from_masks_solid_stack_no_risks() {
        let stack: Vec<LayerMask> = (0..5).map(|_| solid_mask(4, 4)).collect();
        let risks = SuctionDetector::detect_from_masks(&stack).expect("valid");
        assert!(risks.is_empty());
    }

    #[test]
    fn detect_from_masks_closed_cup_emits_one_risk() {
        let mut stack = Vec::new();
        stack.push(solid_mask(5, 5)); // floor
        for _ in 0..3 {
            stack.push(ring_mask(5, 5));
        }
        stack.push(solid_mask(5, 5)); // cap
        let risks = SuctionDetector::detect_from_masks(&stack).expect("valid");
        assert_eq!(risks.len(), 1);
        assert_eq!(risks[0].layer, 4);
        // 3×3 interior at 1mm voxel = 9 mm²
        assert!((risks[0].sealed_area_mm2 - 9.0).abs() < 1e-6);
        // 50 kPa × 9 × 1e-3 = 0.45 N
        assert!((risks[0].suction_force_n - 0.45).abs() < 1e-3);
    }

    #[test]
    fn detect_from_masks_raft_plus_columns_no_false_positive() {
        // The lilith-torso reproduction scaled down.
        let raft = solid_mask(11, 11);
        let mut columns = LayerMask::new(11, 11, 1.0).expect("valid");
        columns.set(2, 2).expect("in bounds");
        columns.set(2, 8).expect("in bounds");
        columns.set(8, 2).expect("in bounds");
        columns.set(8, 8).expect("in bounds");
        let stack = vec![
            raft,
            columns.clone(),
            columns.clone(),
            columns.clone(),
            columns,
        ];
        let risks = SuctionDetector::detect_from_masks(&stack).expect("valid");
        assert!(risks.is_empty(), "no false-positive on raft+columns: {risks:?}");
    }
}
