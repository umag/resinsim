//! Classification of layer indices into print phases — Raft, BurnIn, Normal.
//!
//! Used by `FailurePredictor` via `LayerOverrides.is_raft` to downgrade
//! deflection failures on raft layers to Info severity. Previously this
//! classification was done inline in `app::simulation_runner::detect_raft_end`;
//! moved here per ADR-0001 DDD layering rule (domain vocabulary belongs in
//! `values/`, not `app/`).
//!
//! Phase classification is purely a function of per-layer area and the
//! printer/resin `Recipe`. A raft is detected by the same area-ratio heuristic
//! as before: large initial area followed by a >50% drop. Burn-in is the
//! first `recipe.bottom_layer_count()` non-raft layers. Everything else is
//! Normal.
//!
//! **Not involved in suction detection.** The new 3D `CavityDetector` (Step 6)
//! handles raft geometry natively — the raft→supports transition produces no
//! false-positive suction event because the inter-column void touches the
//! lateral bbox edge and is exterior-connected. This value object exists only
//! to preserve the `is_raft` severity-downgrade behaviour.

use crate::entities::Recipe;
use crate::values::CrossSectionArea;

/// Classification of a single layer within the print.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerPhase {
    /// Adhesion layer stack at the start of the print. Area stays nearly
    /// constant at a large value, then drops sharply as the model/supports
    /// begin. Recipe enforces bottom-exposure timing.
    Raft,
    /// First `recipe.bottom_layer_count()` non-raft layers. Receive
    /// bottom-exposure (longer cure) to establish adhesion into the model.
    BurnIn,
    /// Remainder of the print. Receive normal exposure.
    Normal,
}

impl LayerPhase {
    /// Classify every layer in an area sequence into a phase.
    ///
    /// Algorithm (lifted verbatim from `app::simulation_runner::detect_raft_end`):
    /// - If the sequence is fewer than 3 layers, no raft is detected;
    ///   everything is Normal/BurnIn per recipe.
    /// - If layer 0's area is < 10 mm², no raft is detected.
    /// - Otherwise, walk layers from index 1; the raft ends at the first
    ///   layer whose area drops below 50% of the first layer's area. If the
    ///   area grows past 120% of the first layer's area before such a drop
    ///   occurs, we're not looking at a raft pattern.
    /// - Layers before `raft_end` are `Raft`.
    /// - The next `recipe.bottom_layer_count()` layers (capped at the end
    ///   of the sequence) are `BurnIn`.
    /// - Remainder is `Normal`.
    pub fn classify_sequence(areas: &[CrossSectionArea], recipe: &Recipe) -> Vec<LayerPhase> {
        let raft_end = Self::detect_raft_end(areas) as usize;
        let burnin_end = raft_end + recipe.bottom_layer_count() as usize;
        let burnin_end = burnin_end.min(areas.len());

        areas
            .iter()
            .enumerate()
            .map(|(i, _)| {
                if i < raft_end {
                    LayerPhase::Raft
                } else if i < burnin_end {
                    LayerPhase::BurnIn
                } else {
                    LayerPhase::Normal
                }
            })
            .collect()
    }

    /// Locate the first non-raft layer (returns 0 if no raft is detected).
    /// Used internally by `classify_sequence`; exposed for callers that only
    /// need the boundary.
    pub fn detect_raft_end(areas: &[CrossSectionArea]) -> u32 {
        if areas.len() < 3 {
            return 0;
        }
        let first_area = areas[0].value();
        if first_area < 10.0 {
            return 0;
        }
        for (i, a) in areas.iter().enumerate().skip(1) {
            let ratio = a.value() / first_area;
            if ratio < 0.5 {
                return i as u32;
            }
            if a.value() > first_area * 1.2 {
                return 0;
            }
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(mm2: f64) -> CrossSectionArea {
        CrossSectionArea::new(mm2)
            .expect("test fixture: finite non-negative mm² is in CrossSectionArea domain")
    }

    fn generic_recipe() -> Recipe {
        Recipe::generic_standard()
    }

    #[test]
    fn empty_sequence_yields_empty_phases() {
        let phases = LayerPhase::classify_sequence(&[], &generic_recipe());
        assert!(phases.is_empty());
    }

    #[test]
    fn too_short_sequence_has_no_raft() {
        let areas = vec![area(100.0), area(100.0)];
        let phases = LayerPhase::classify_sequence(&areas, &generic_recipe());
        assert_eq!(phases.len(), 2);
        // No raft → first `bottom_layer_count` are BurnIn, rest Normal
        assert_eq!(phases[0], LayerPhase::BurnIn);
        assert_eq!(phases[1], LayerPhase::BurnIn);
    }

    #[test]
    fn tiny_first_area_has_no_raft() {
        let areas = vec![area(5.0), area(5.0), area(5.0)];
        let phases = LayerPhase::classify_sequence(&areas, &generic_recipe());
        assert!(phases.iter().all(|&p| p == LayerPhase::BurnIn));
    }

    #[test]
    fn constant_area_has_no_raft_pattern() {
        // Constant area throughout → not a raft (no drop signal).
        let areas = vec![area(100.0); 10];
        let phases = LayerPhase::classify_sequence(&areas, &generic_recipe());
        let burnin_count = generic_recipe().bottom_layer_count() as usize;
        for (i, &p) in phases.iter().enumerate() {
            let expected = if i < burnin_count {
                LayerPhase::BurnIn
            } else {
                LayerPhase::Normal
            };
            assert_eq!(p, expected, "layer {i} constant-area classification");
        }
    }

    #[test]
    fn raft_then_model_classification() {
        // Raft (2085 mm²) layers 0-4, then thin (106 mm²) layers 5-19
        let mut areas = vec![area(2085.0); 5];
        areas.extend(vec![area(106.0); 15]);
        let phases = LayerPhase::classify_sequence(&areas, &generic_recipe());

        // Layers 0-4: Raft
        for (i, phase) in phases.iter().enumerate().take(5) {
            assert_eq!(*phase, LayerPhase::Raft, "layer {i}");
        }
        // Next bottom_layer_count (generic = 6) layers: BurnIn → layers 5-10
        let bottom = generic_recipe().bottom_layer_count() as usize;
        for (i, phase) in phases.iter().enumerate().take(5 + bottom).skip(5) {
            assert_eq!(*phase, LayerPhase::BurnIn, "layer {i}");
        }
        // Remainder: Normal → layers 11..
        for (i, phase) in phases.iter().enumerate().skip(5 + bottom) {
            assert_eq!(*phase, LayerPhase::Normal, "layer {i}");
        }
    }

    #[test]
    fn detect_raft_end_finds_transition() {
        let mut areas = vec![area(2085.0); 23];
        areas.extend(vec![area(106.0); 20]);
        assert_eq!(LayerPhase::detect_raft_end(&areas), 23);
    }

    #[test]
    fn detect_raft_end_no_drop_returns_zero() {
        let areas = vec![area(100.0); 50];
        assert_eq!(LayerPhase::detect_raft_end(&areas), 0);
    }

    #[test]
    fn detect_raft_end_area_grows_means_not_raft() {
        // Area grows sharply — this is a model expanding, not a raft pattern.
        let areas = vec![area(100.0), area(200.0), area(400.0)];
        assert_eq!(LayerPhase::detect_raft_end(&areas), 0);
    }

    #[test]
    fn burnin_capped_by_sequence_length() {
        // Very short non-raft tail — BurnIn should be capped, no Normal layers.
        let mut areas = vec![area(2085.0); 5];
        areas.extend(vec![area(106.0); 3]); // only 3 non-raft layers
        let phases = LayerPhase::classify_sequence(&areas, &generic_recipe());
        assert_eq!(phases.len(), 8);
        for phase in phases.iter().take(5) {
            assert_eq!(*phase, LayerPhase::Raft);
        }
        for phase in phases.iter().take(8).skip(5) {
            assert_eq!(*phase, LayerPhase::BurnIn);
        }
    }
}
