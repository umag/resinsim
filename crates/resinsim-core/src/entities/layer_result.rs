use serde::{Deserialize, Serialize};

/// Simulation output for a single layer. Identity: layer index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerResult {
    pub index: u32,
    pub cure_depth_um: f32,
    pub peel_force_n: f32,
    pub suction_force_n: f32,
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
