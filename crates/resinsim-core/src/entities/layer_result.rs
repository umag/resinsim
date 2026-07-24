use serde::{Deserialize, Serialize};

#[cfg(feature = "field-sim")]
use crate::simulation::PrintSimulation;
#[cfg(feature = "field-sim")]
use crate::values::CureDepth;

/// Simulation output for a single layer. Identity: layer index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerResult {
    pub index: u32,
    pub cure_depth_um: f32,
    pub peel_force_n: f32,
    pub suction_force_n: f32,
    /// First-layer base-adhesion force (N), KB-116 oxygen-freshness
    /// σ-relaxation (ADR-0022 Stage 1). Peer of `suction_force_n`; a component
    /// of `total_force_n`. `0.0` unless the resin opts in via
    /// `base_adhesion_elevation_kpa`. `#[serde(default)]` so pre-Stage-1
    /// sim.json files deserialise unchanged.
    #[serde(default)]
    pub base_force_n: f32,
    /// Applied KB-185 A/L peel shape factor (ADR-0022 Stage 3), dimensionless
    /// in `(0, 1]`. `None` when the resin's `peel_shape_factor_strength` is
    /// unset (no correction — the factor is ≡ 1.0); `skip_serializing_if` then
    /// omits it so pre-Stage-3 sim.json files stay byte-identical. `Some(f)`
    /// records the factor the runner applied to `peel_force_n` this layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peel_shape_factor: Option<f32>,
    pub total_force_n: f32,
    pub support_capacity_n: f32,
    /// Safety factor (capacity / load). For zero-load layers
    /// (`total_force_n == 0`) this is `f32::INFINITY` per
    /// `failure_predictor` and UAT-1 of
    /// `spec/uat/safety-factor-zero-force.md`. Serde adapter
    /// [`f32_with_infinity`] round-trips INFINITY through JSON `null`
    /// since JSON has no Infinity literal.
    #[serde(with = "f32_with_infinity")]
    pub safety_factor: f32,
    pub cross_section_area_mm2: f64,
    pub area_delta_mm2: f64,
    pub vat_temperature_c: f32,
    pub viscosity_mpa_s: f32,
    pub z_deflection_um: f32,
    pub effective_layer_height_um: f32,
    /// Worst-case cure depth at plate edge/corner (LCD non-uniformity). KB-120.
    pub worst_cure_depth_um: f32,
    /// Per-layer max Frobenius-norm strain (ADR-0018 / t2f3). `None` on
    /// Tier-1 runs (no voxel mode) AND on voxel-mode runs where this
    /// layer's strain slab is entirely zero (uncured liquid). Skip-
    /// serialize-when-None keeps Tier-1 sim.json files unchanged.
    /// NOT feature-gated — the field exists in both build configs as a
    /// uniform Option<f32> so existing constructor literals across the
    /// workspace stay one shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strain_magnitude_max: Option<f32>,
    /// Per-layer max von Mises stress in MPa (ADR-0018 / t2f3). `None` in
    /// the same conditions as `strain_magnitude_max`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stress_von_mises_max_mpa: Option<f32>,
    /// Per-layer max strain gradient `|∇ε|` between adjacent voxels
    /// (ADR-0018 / t2f3). Boundary voxels skipped per KB-161.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strain_gradient_max_frac: Option<f32>,
    /// Per-layer voxel yield fraction (ADR-0018 / t2f3). The share of
    /// cured voxels in this layer's Z-slab whose von Mises stress
    /// exceeds `resin.tensile_strength_mpa()` — i.e. the share that
    /// have crossed the multi-axial yield surface. Range `[0, 1]`.
    /// `None` on Tier-1 paths. See KB-162 for the yield criterion
    /// derivation and the model-gap caveat (free shrinkage stress only,
    /// no cumulative residual stress).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voxel_yield_fraction: Option<f32>,
}

/// serde adapter for an `f32` that may legitimately be `f32::INFINITY`
/// (the zero-force `safety_factor` case). JSON has no Infinity/NaN
/// literal — `serde_json` writes non-finite values as `null` but then
/// fails to deserialize them back into `f32`. This adapter makes the
/// round-trip lossless:
///
/// - **Serialize**: finite → JSON number; non-finite → JSON `null`.
/// - **Deserialize**: number → f32; `null` → `f32::INFINITY`.
///
/// `f32::INFINITY` is the only legitimate non-finite value for
/// `safety_factor` (set when load is zero). NaN should never reach this
/// field; if it somehow does, it round-trips as INFINITY, which is
/// strictly safer than crashing on null→f32 deserialise. Downstream
/// consumers that need to distinguish "no load" (safety_factor = INF)
/// from "very large finite SF" should branch on `value.is_infinite()`.
mod f32_with_infinity {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &f32, s: S) -> Result<S::Ok, S::Error> {
        if value.is_finite() {
            s.serialize_f32(*value)
        } else {
            s.serialize_none()
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
        let opt: Option<f32> = Option::deserialize(d)?;
        Ok(opt.unwrap_or(f32::INFINITY))
    }
}

#[cfg(feature = "field-sim")]
impl LayerResult {
    /// Per-layer scalar cure depth (µm), dispatching on whether the
    /// containing simulation carries a voxel cure field.
    ///
    /// - When `sim.cure_field()` is `Some`, returns the mean cure depth
    ///   across the voxel field's Z-slab at this layer's index, using
    ///   the supplied resin Dp/Ec to map dose → depth.
    /// - When `sim.cure_field()` is `None` (Tier-1 mode), returns the
    ///   stored `cure_depth_um` cache (which SimulationRunner populated
    ///   from `CureCalculator::cure_depth_at_temp`).
    ///
    /// `dp_um` and `ec_mj_cm2` are read from the resin profile by the
    /// caller; they're only consulted in voxel mode for the dose-to-
    /// depth mapping. In Tier-1 mode they're ignored — the cached scalar
    /// already encodes them.
    pub fn cure_depth_um_summary(
        &self,
        sim: &PrintSimulation,
        dp_um: f32,
        ec_mj_cm2: f32,
    ) -> CureDepth {
        // Voxel mode: dispatch through the field's layer summary so the
        // answer reflects the actual per-voxel dose distribution rather
        // than a pre-computed scalar that may drift on long prints
        // (KB-160 photoinitiator depletion). Fall through to the cached
        // scalar if the layer index is out-of-bounds on the field
        // (defensive — should not happen for a well-constructed aggregate).
        if let Some(cf) = sim.cure_field()
            && let Ok(summary) = cf.layer_summary(self.index, dp_um, ec_mj_cm2)
        {
            return CureDepth::new(summary.mean).unwrap_or_else(|_| {
                CureDepth::new(self.cure_depth_um)
                    .expect("LayerResult.cure_depth_um is validated finite on construction")
            });
        }
        CureDepth::new(self.cure_depth_um)
            .expect("LayerResult.cure_depth_um is validated finite on construction")
    }

    /// Worst-case (most-undercured) cure depth for this layer.
    ///
    /// Voxel-mode: returns `LayerSummary.min` (the minimum cure depth
    /// across all voxels in the layer's Z-slab — the most-undercured
    /// pixel per LCD non-uniformity + photoinitiator depletion).
    /// Tier-1: returns the cached `worst_cure_depth_um` scalar.
    pub fn worst_cure_depth_um_summary(
        &self,
        sim: &PrintSimulation,
        dp_um: f32,
        ec_mj_cm2: f32,
    ) -> CureDepth {
        if let Some(cf) = sim.cure_field()
            && let Ok(summary) = cf.layer_summary(self.index, dp_um, ec_mj_cm2)
        {
            return CureDepth::new(summary.min).unwrap_or_else(|_| {
                CureDepth::new(self.worst_cure_depth_um).expect(
                    "LayerResult.worst_cure_depth_um is validated finite on construction",
                )
            });
        }
        CureDepth::new(self.worst_cure_depth_um)
            .expect("LayerResult.worst_cure_depth_um is validated finite on construction")
    }

    /// Per-voxel cure depth at LCD pixel `(x, y)` of this layer.
    ///
    /// Returns `Some` when `sim` carries a voxel cure field AND `(x, y)`
    /// is within the field's bbox at this layer's `iz`. Returns `None`
    /// in Tier-1 mode (no field) or when `(x, y)` is out-of-bbox — the
    /// caller decides whether to fall back to the layer summary or skip
    /// the read entirely.
    pub fn cure_depth_um_at_voxel(
        &self,
        sim: &PrintSimulation,
        x: u32,
        y: u32,
        dp_um: f32,
        ec_mj_cm2: f32,
    ) -> Option<CureDepth> {
        let cf = sim.cure_field()?;
        cf.cure_depth_at(x, y, self.index, dp_um, ec_mj_cm2).ok()
    }
}
