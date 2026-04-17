use crate::values::PeelForce;

/// Domain service: Z-axis elastic deflection and effective layer height.
/// Stateless — all inputs via parameters.
///
/// Core equations (KB-131):
///   Δz = F_peel / k_axis
///   h_effective = h_commanded - Δz
pub struct ZAxisCompensator;

/// Severity of Z deflection impact on the layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZDeflectionSeverity {
    /// Deflection negligible relative to layer height.
    Normal,
    /// Effective layer height < 50% of commanded — significant thickness variation.
    Warning,
    /// Deflection exceeds commanded height — layer compressed into previous.
    Catastrophic,
}

impl ZAxisCompensator {
    /// Elastic deflection under peel load.
    /// KB-131: Δz (µm) = F_peel (N) / k_axis (N/mm) × 1000.
    pub fn deflection_um(force: PeelForce, stiffness_n_per_mm: f32) -> f32 {
        if stiffness_n_per_mm <= 0.0 {
            return 0.0;
        }
        (force.value() / stiffness_n_per_mm) * 1000.0
    }

    /// Effective layer height after deflection.
    /// KB-131: h_eff = h_commanded - Δz. Can be negative (catastrophic).
    pub fn effective_layer_height_um(commanded_um: f32, deflection_um: f32) -> f32 {
        commanded_um - deflection_um
    }

    /// Classify deflection severity.
    /// Effective height of zero is catastrophic — no new resin cures at the
    /// previous layer's top surface.
    pub fn severity(commanded_um: f32, deflection_um: f32) -> ZDeflectionSeverity {
        let effective = Self::effective_layer_height_um(commanded_um, deflection_um);
        if effective <= 0.0 {
            ZDeflectionSeverity::Catastrophic
        } else if effective < commanded_um * 0.5 {
            ZDeflectionSeverity::Warning
        } else {
            ZDeflectionSeverity::Normal
        }
    }

    /// Derive stiffness from a known force-deflection pair.
    /// Useful for calibrating k_axis from Athena II data.
    /// k = F (N) / Δz (mm) = F / (Δz_um / 1000)
    pub fn derive_stiffness(force_n: f32, deflection_um: f32) -> f32 {
        if deflection_um <= 0.0 {
            return f32::INFINITY;
        }
        force_n / (deflection_um / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- KB-131 test vectors ---

    #[test]
    fn zero_force_zero_deflection() {
        let dz = ZAxisCompensator::deflection_um(PeelForce(0.0), 460.0);
        assert!((dz).abs() < 1e-6);
    }

    #[test]
    fn deflection_10n_at_460() {
        // KB-131: F=10, k=460 → Δz = 10/460 × 1000 = 21.7 µm
        let dz = ZAxisCompensator::deflection_um(PeelForce(10.0), 460.0);
        assert!((dz - 21.7).abs() < 0.1);
    }

    #[test]
    fn deflection_120n_matches_mrazek() {
        // KB-131: F=120, k=460 → Δz = 260.9 µm (Mrazek measured ~260)
        let dz = ZAxisCompensator::deflection_um(PeelForce(120.0), 460.0);
        assert!((dz - 260.9).abs() < 0.1);
    }

    #[test]
    fn deflection_200n_matches_mrazek() {
        // KB-131: F=200, k=460 → Δz = 434.8 µm (Mrazek measured ~340 + settling)
        let dz = ZAxisCompensator::deflection_um(PeelForce(200.0), 460.0);
        assert!((dz - 434.8).abs() < 0.1);
    }

    #[test]
    fn deflection_with_stiff_axis() {
        // KB-131: F=120, k=1500 → Δz = 80.0 µm
        let dz = ZAxisCompensator::deflection_um(PeelForce(120.0), 1500.0);
        assert!((dz - 80.0).abs() < 0.1);
    }

    #[test]
    fn light_load_stiff_axis() {
        // KB-131: F=10, k=1500 → Δz = 6.7 µm
        let dz = ZAxisCompensator::deflection_um(PeelForce(10.0), 1500.0);
        assert!((dz - 6.67).abs() < 0.1);
    }

    // --- Effective layer height ---

    #[test]
    fn effective_height_normal() {
        // KB-131: h=50, Δz=21.7 → h_eff=28.3
        let h = ZAxisCompensator::effective_layer_height_um(50.0, 21.7);
        assert!((h - 28.3).abs() < 0.1);
    }

    #[test]
    fn effective_height_negative_catastrophic() {
        // KB-131: h=50, Δz=108.7 → h_eff=-58.7
        let h = ZAxisCompensator::effective_layer_height_um(50.0, 108.7);
        assert!(h < 0.0);
        assert!((h - (-58.7)).abs() < 0.1);
    }

    #[test]
    fn thicker_layers_help() {
        // KB-131: h=100, Δz=108.7 → h_eff=-8.7 (still negative but closer)
        let h = ZAxisCompensator::effective_layer_height_um(100.0, 108.7);
        assert!((h - (-8.7)).abs() < 0.1);
    }

    // --- Severity classification ---

    #[test]
    fn severity_normal_for_small_deflection() {
        // Δz=21.7, h=50 → h_eff=28.3 (>50% of 50) → Normal
        assert_eq!(
            ZAxisCompensator::severity(50.0, 21.7),
            ZDeflectionSeverity::Normal,
        );
    }

    #[test]
    fn severity_warning_when_half_compressed() {
        // Δz=30, h=50 → h_eff=20 (40% of 50) → Warning
        assert_eq!(
            ZAxisCompensator::severity(50.0, 30.0),
            ZDeflectionSeverity::Warning,
        );
    }

    #[test]
    fn severity_catastrophic_when_exactly_zero() {
        // h=50, Δz=50 → h_eff=0 → zero-thickness layer, catastrophic
        assert_eq!(
            ZAxisCompensator::severity(50.0, 50.0),
            ZDeflectionSeverity::Catastrophic,
        );
    }

    #[test]
    fn severity_catastrophic_when_negative() {
        // Δz=108.7, h=50 → h_eff=-58.7 → Catastrophic
        assert_eq!(
            ZAxisCompensator::severity(50.0, 108.7),
            ZDeflectionSeverity::Catastrophic,
        );
    }

    // --- Stiffness derivation ---

    #[test]
    fn derive_stiffness_from_mrazek_data() {
        // KB-131: F=120N, Δz=260µm → k = 120 / 0.260 = 461.5 N/mm
        let k = ZAxisCompensator::derive_stiffness(120.0, 260.0);
        assert!((k - 461.5).abs() < 1.0);
    }

    #[test]
    fn derive_stiffness_zero_deflection_is_infinite() {
        let k = ZAxisCompensator::derive_stiffness(120.0, 0.0);
        assert!(k.is_infinite());
    }
}
