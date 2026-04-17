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
