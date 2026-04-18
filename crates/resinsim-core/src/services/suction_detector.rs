use crate::values::CrossSectionArea;

/// Domain service: detect sealed cavities (suction cups) in layer geometry.
/// Stateless — all inputs via parameters.
///
/// A sealed cavity is a hollow region that traps air/resin against the FEP
/// film during peel, creating a vacuum (up to 101 kPa atmospheric pressure).
///
/// KB-114: F_suction = ΔP × A_sealed
/// KB-173: Sealed cups produce forces 7-26× higher than area-proportional peel.
///
/// Simplified Tier 1 approach: analyze consecutive layer cross-sections.
/// A sealed cavity exists when:
///   - Current layer has a solid region (wall)
///   - The interior of the wall has no drain holes to the exterior
///   - The layer below is also solid (sealing the bottom)
///
/// Full topology analysis (connected components on RLE masks) is Tier 2.
/// This Tier 1 version uses a heuristic: detect hollow geometry from
/// the difference between outer-boundary area and solid area.
pub struct SuctionDetector;

/// A detected suction risk.
#[derive(Debug, Clone)]
pub struct SuctionRisk {
    /// Layer where suction is detected.
    pub layer: u32,
    /// Estimated sealed area in mm².
    pub sealed_area_mm2: f64,
    /// Estimated suction force in Newtons (at 50 kPa partial vacuum).
    pub suction_force_n: f32,
}

impl SuctionDetector {
    /// Detect suction risks from per-layer areas.
    ///
    /// Heuristic: if cross-section area drops significantly between two
    /// consecutive layers while remaining non-zero, the geometry may have
    /// transitioned from solid to a ring/shell, creating a sealed cavity.
    ///
    /// Also checks for "cup closing" events: when area appears after a
    /// gap or when geometry transitions from ring to solid bottom.
    ///
    /// For proper detection, we need both outer_area and solid_area per layer.
    /// With only solid_area (our current slicer output), we use a simplified
    /// heuristic based on area patterns.
    pub fn detect_from_areas(
        solid_areas: &[CrossSectionArea],
        outer_areas: Option<&[CrossSectionArea]>,
    ) -> Vec<SuctionRisk> {
        match outer_areas {
            Some(outer) => Self::detect_with_outer_boundary(solid_areas, outer),
            None => Self::detect_heuristic(solid_areas),
        }
    }

    /// Detect suction using both solid and outer-boundary areas.
    /// Sealed area = outer_area - solid_area (the hollow interior).
    /// A cavity is sealed if the hollow area is bounded by solid walls
    /// on the current layer and solid floor on the layer below.
    fn detect_with_outer_boundary(
        solid_areas: &[CrossSectionArea],
        outer_areas: &[CrossSectionArea],
    ) -> Vec<SuctionRisk> {
        let mut risks = Vec::new();
        let n = solid_areas.len().min(outer_areas.len());

        for i in 1..n {
            let hollow = outer_areas[i].value() - solid_areas[i].value();
            if hollow < 1.0 {
                continue; // No significant hollow region
            }

            // Check if the layer below seals the cavity
            let prev_solid = solid_areas[i - 1].value();
            let prev_outer = outer_areas[i - 1].value();
            let prev_hollow = prev_outer - prev_solid;

            // Cavity is sealed if: current layer has a hollow,
            // and previous layer's solid covers the hollow area
            // (i.e., the previous layer was more solid in the cavity region)
            if prev_hollow < hollow * 0.5 {
                // Previous layer was mostly solid where current layer is hollow → sealed
                let sealed_area = hollow;
                let suction_force = 50.0 * sealed_area as f32 * 1e-3; // 50 kPa partial vacuum
                risks.push(SuctionRisk {
                    layer: i as u32,
                    sealed_area_mm2: sealed_area,
                    suction_force_n: suction_force,
                });
            }
        }

        risks
    }

    /// Heuristic detection with only solid areas (no outer boundary info).
    /// Detects "cup closing" events where solid area suddenly drops
    /// while the part is still present — suggests the geometry transitioned
    /// from a solid base to hollow walls.
    fn detect_heuristic(solid_areas: &[CrossSectionArea]) -> Vec<SuctionRisk> {
        let mut risks = Vec::new();

        for i in 1..solid_areas.len() {
            let curr = solid_areas[i].value();
            let prev = solid_areas[i - 1].value();

            if prev < 1.0 || curr < 1.0 {
                continue;
            }

            // Detect sharp area drop (>50%) while part still present
            // This suggests solid→ring transition (cup walls start)
            let ratio = curr / prev;
            if ratio < 0.5 && curr > 10.0 {
                // Estimated sealed area = what disappeared
                let sealed_area = prev - curr;
                let suction_force = 50.0 * sealed_area as f32 * 1e-3;
                risks.push(SuctionRisk {
                    layer: i as u32,
                    sealed_area_mm2: sealed_area,
                    suction_force_n: suction_force,
                });
            }
        }

        risks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(mm2: f64) -> CrossSectionArea {
        CrossSectionArea::new(mm2)
            .expect("test fixture: non-negative finite mm² is in CrossSectionArea domain")
    }

    #[test]
    fn solid_cube_no_suction() {
        // Constant area → no hollows → no suction
        let areas: Vec<CrossSectionArea> = vec![area(100.0); 50];
        let risks = SuctionDetector::detect_from_areas(&areas, None);
        assert!(risks.is_empty(), "solid cube should have no suction risks");
    }

    #[test]
    fn hollow_cup_detected_with_outer_boundary() {
        // KB-173: hollow cup — solid base transitions to ring walls
        // Layer 0-5: solid base (outer=314, solid=314)
        // Layer 6+: ring walls (outer=314, solid=59.7) → hollow=254.3 mm²
        let n = 20;
        let mut solid = vec![area(0.0); n];
        let mut outer = vec![area(0.0); n];

        // Solid base
        for i in 0..6 {
            solid[i] = area(314.0);
            outer[i] = area(314.0);
        }
        // Ring walls (20mm OD, 1mm wall → ID=18mm)
        for i in 6..n {
            solid[i] = area(59.7); // wall cross-section only
            outer[i] = area(314.0); // outer boundary still 20mm dia
        }

        let risks = SuctionDetector::detect_from_areas(&solid, Some(&outer));
        assert!(!risks.is_empty(), "hollow cup should be detected");
        assert_eq!(risks[0].layer, 6);
        assert!((risks[0].sealed_area_mm2 - 254.3).abs() < 1.0);
    }

    #[test]
    fn drained_cup_detects_only_small_residual() {
        // Cup with drain holes — outer boundary nearly matches solid area.
        // In a properly drained part, drain holes break the outer contour
        // so outer ≈ solid. Here outer slightly exceeds solid (5.3 mm²) to
        // model imperfect modelling; we assert that if a risk is flagged,
        // its sealed area is small (< 10 mm²) — far below a true sealed cup.
        let n = 20;
        let mut solid = vec![area(0.0); n];
        let mut outer = vec![area(0.0); n];
        for i in 0..6 {
            solid[i] = area(314.0);
            outer[i] = area(314.0);
        }
        for i in 6..n {
            solid[i] = area(59.7);
            outer[i] = area(65.0);
        }

        let risks = SuctionDetector::detect_from_areas(&solid, Some(&outer));
        for r in &risks {
            assert!(
                r.sealed_area_mm2 < 10.0,
                "drained cup residual should be small, got {} mm²",
                r.sealed_area_mm2
            );
        }
    }

    #[test]
    fn mismatched_array_lengths_do_not_panic() {
        // Defensive: outer_areas shorter than solid_areas must not oob-panic.
        let solid = vec![area(100.0); 10];
        let outer = vec![area(150.0); 3];
        let _ = SuctionDetector::detect_from_areas(&solid, Some(&outer));
    }

    #[test]
    fn heuristic_detects_area_drop() {
        // Solid base → thin walls: 314 mm² drops to 60 mm² (ratio 0.19)
        let mut areas: Vec<CrossSectionArea> = vec![area(314.0); 10];
        for a in areas.iter_mut().take(10).skip(5) {
            *a = area(60.0);
        }

        let risks = SuctionDetector::detect_from_areas(&areas, None);
        assert!(!risks.is_empty(), "50%+ area drop should be flagged");
        assert_eq!(risks[0].layer, 5);
    }

    #[test]
    fn gradual_area_change_no_suction() {
        // Sphere: area changes gradually — no sharp drops
        let areas: Vec<CrossSectionArea> = (0..100)
            .map(|i| {
                let h = (i as f64 + 0.5) / 100.0;
                let a = std::f64::consts::PI * 100.0 * (1.0 - (2.0 * h - 1.0).powi(2));
                CrossSectionArea::new(a.max(1.0)).expect("max(1.0) guarantees non-negative finite")
            })
            .collect();

        let risks = SuctionDetector::detect_from_areas(&areas, None);
        assert!(risks.is_empty(), "sphere should have no suction (gradual area change)");
    }

    #[test]
    fn suction_force_magnitude() {
        // KB-114: sealed 20mm cup → A=254 mm², ΔP=50 kPa → F=12.7 N
        let mut solid = vec![area(314.0); 10];
        let mut outer = vec![area(314.0); 10];
        for i in 5..10 {
            solid[i] = area(59.7);
            outer[i] = area(314.0);
        }

        let risks = SuctionDetector::detect_from_areas(&solid, Some(&outer));
        assert!(!risks.is_empty());
        // 50 kPa × 254.3 mm² × 1e-3 = 12.7 N
        assert!((risks[0].suction_force_n - 12.7).abs() < 0.5);
    }
}
