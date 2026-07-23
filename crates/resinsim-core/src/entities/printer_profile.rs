use serde::{Deserialize, Serialize};

use crate::values::{FloatRange, DEFAULT_VOXEL_SIZE_MM};

fn default_voxel_size_mm() -> f32 {
    DEFAULT_VOXEL_SIZE_MM
}

/// Conservative legacy-matching default for LED steady-state rise. Parallels
/// existing `delta_t_steady_c` legacy default. Unit: °C.
fn default_led_delta_t_steady_c() -> f32 {
    10.0
}

/// Conservative legacy-matching default for LED thermal time constant. Unit: seconds.
fn default_led_tau_sec() -> f32 {
    1200.0
}

/// Conservative midpoint default for LED → vat coupling factor. Dimensionless.
/// A legacy profile without measurements gets 0.5 (halfway between no coupling
/// and full coupling). Known-calibrated profiles override (Mars 5 Ultra = 0.71).
fn default_led_to_vat_coupling() -> f32 {
    0.5
}

/// Workspace-default voxel cure resolution (mm). ADR-0017 / t2f1.
/// Applied when `PrinterProfile.voxel_cure_resolution_mm` is `None` and the
/// CLI does not override. 0.2 mm is the Pareto point between thin-wall
/// resolution + memory budget — see ADR-0017 §"Variable voxel resolution".
pub const DEFAULT_VOXEL_CURE_RESOLUTION_MM: f32 = 0.2;

/// Upper-bound safety cap for `crosstalk_sigma_xy_um` and `crosstalk_sigma_z_um`
/// when validating `PrinterProfile`. ADR-0018 §1 / t2f2.
///
/// 5000 µm = 5 mm, ~100× the typical LCD pixel pitch (50 µm) and ~125× the
/// typical resin scatter mean-free-path (40 µm). Values above this would
/// produce kernels with radius spanning the entire build envelope — almost
/// certainly a misconfigured TOML, not a real calibration. Reject at
/// validate-time to prevent silent simulation corruption.
pub const MAX_SIGMA_UM: f32 = 5000.0;

/// Default partial-vacuum pressure ΔP assumed to form at a sealed cavity during
/// peel, used when a [`PrinterProfile`] does not specify a measured
/// `vacuum_pressure_kpa`. 50 kPa ≈ half atmospheric — strong enough to require
/// non-trivial peel force at realistic cavity sizes. INDICATIVE: the true ΔP is
/// a 50–101 kPa data gap (KB-184); E4 / KB-173 sealed-vs-drained cup calibration
/// is future work for resin/FEP-specific tuning. ADR-0022 Stage 2. (Migrated from
/// the former `cavity_detector::VACUUM_PRESSURE_KPA`.)
pub const DEFAULT_VACUUM_PRESSURE_KPA: f32 = 50.0;

/// Physical maximum for a sealed-cavity ΔP: one standard atmosphere. A vacuum
/// cannot pull the plate harder than atmospheric pressure pushes it down, so
/// `vacuum_pressure_kpa` is validated not to exceed this (KB-184). Unit: kPa.
pub const ATMOSPHERIC_PRESSURE_KPA: f32 = 101.325;

/// Hardware build envelope of a printer (ADR-0012, extends ADR-0005 Axis 1).
///
/// Optional on [`PrinterProfile`] — see ADR-0012. When present, all three
/// dimensions must be positive and finite. When absent, viz consumers must
/// fall back to alternative envelope sources (e.g. CTB header bed_size_mm
/// plus a sentinel max_z) and surface the missing-envelope state to the
/// user (typically via a one-shot warn).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BuildEnvelope {
    /// Build-volume X extent (LCD-pixel-grid axis). Unit: mm.
    pub width_mm: f32,
    /// Build-volume Y extent (LCD-pixel-grid axis). Unit: mm.
    pub depth_mm: f32,
    /// Build-volume Z extent (build axis). Unit: mm.
    pub max_z_mm: f32,
}

impl BuildEnvelope {
    /// Validate that all three extents are positive and finite. Called from
    /// [`PrinterProfile::validate`] when the envelope field is `Some`.
    pub fn validate(&self) -> Result<(), String> {
        let checks: &[(f32, &str)] = &[
            (self.width_mm, "width_mm"),
            (self.depth_mm, "depth_mm"),
            (self.max_z_mm, "max_z_mm"),
        ];
        for (val, field) in checks {
            if !val.is_finite() || *val <= 0.0 {
                return Err(format!(
                    "build_envelope_mm.{field} must be finite and > 0 (got {val})"
                ));
            }
        }
        Ok(())
    }
}

/// How the printer separates a cured layer from the FEP film (ADR-0007).
///
/// Distinct mechanical paradigms produce distinct per-layer time profiles, so
/// `LayerTimingCalculator` branches on this value. Two variants today; more
/// may be added (e.g. Anycubic peel-lift hybrids).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReleaseMechanism {
    /// Build plate (arm) lifts straight up, vat stationary. Athena II, Mars 4,
    /// classic MSLA. Recipe's `lift_distance_mm` + `lift_speed_mm_min` represent
    /// actual plate travel distance and speed.
    #[default]
    Linear,
    /// Vat hinges to peel layer off, plate stationary. Mars 5 Ultra, Saturn 4
    /// variants. Recipe's `lift_distance_mm` + `lift_speed_mm_min` are CTB-file
    /// METADATA ONLY for Tilt printers — `LayerTimingCalculator` does NOT use
    /// them; it uses `Recipe::lift_cycle_sec` as the canonical lumped release
    /// duration. Tilt-angular geometry refinement
    /// (`tilt_angle_deg` × `tilt_rate_deg_s`) is a follow-on issue.
    Tilt,
}

/// Hardware envelope of a printer (ADR-0005, Axis 1).
/// Identity: `name`. Loaded from TOML profiles in `data/printers/`.
///
/// # Hardware vs Recipe
///
/// This type carries only hardware-intrinsic fields (LED power, pixel pitch, Z-axis
/// stiffness, LCD uniformity, thermal, bed size) and the **range envelopes** that
/// bound which `Recipe` values a resin may request on this printer. Recipe fields
/// (exposure times, layer height, lift kinematics) live on `ResinProfile::recipe()`.
/// See ADR-0005.
///
/// # Validate-on-mutation contract
///
/// Fields are `pub(crate)` — external code cannot construct or mutate a
/// `PrinterProfile`. Construction is restricted to the factory methods on this type
/// (`generic_msla_4k`, `elegoo_mars5_ultra`) and to TOML deserialisation via
/// `PrinterProfileRepository`, both of which run `validate()` before returning.
/// After any field mutation by intra-crate code (typically tests), `validate()` MUST
/// be re-called before treating the profile as trusted. `simulation_runner` provides
/// defence-in-depth by calling `validate()` again at run entry. See
/// `docs/patterns/entity-validate-on-mutation.md`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterProfile {
    pub(crate) name: String,

    // Light source
    /// LED power density at LCD plane. Unit: mW/cm². KB-121.
    pub(crate) led_power_mw_cm2: f32,
    /// Physical pixel size. Unit: µm. KB-160.
    pub(crate) pixel_pitch_um: f32,

    // Hardware envelope (ADR-0005 Axis 1)
    /// Range of valid layer heights for this printer. Unit: µm.
    pub(crate) layer_height_range_um: FloatRange,
    /// Range of valid exposure times for this printer. Unit: seconds.
    pub(crate) exposure_range_sec: FloatRange,
    /// Range of valid lift speeds for this printer. Unit: mm/min.
    pub(crate) lift_speed_range_mm_min: FloatRange,
    /// Maximum supported bottom-layer count. Scalar ceiling, not a range —
    /// lower bound has no hardware meaning (see ADR-0005 §2).
    pub(crate) bottom_layer_count_max: u32,

    // Z-axis
    /// Z-axis stiffness. Unit: N/mm. KB-131, KB-182.
    pub(crate) z_stiffness_n_per_mm: f32,

    // Thermal
    /// Steady-state temperature rise above ambient. Unit: °C. KB-183.
    pub(crate) delta_t_steady_c: f32,
    /// Thermal time constant. Unit: seconds. KB-183.
    pub(crate) thermal_tau_sec: f32,

    // LCD uniformity — KB-120
    /// Peak-to-peak intensity variation as fraction (0.34 = 34%). 0.0 = ideal.
    pub(crate) lcd_uniformity_variation: f32,

    /// Voxel resolution in mm for slicer mask output (Step 4 of
    /// suction-detector-raft-false-positive). Controls memory budget of
    /// per-layer `LayerMask` stacks used by `CavityDetector`. Finer values
    /// catch thinner walls at the cost of memory; coarser values save memory
    /// but may over-report solid area around sub-mm features.
    ///
    /// Default 0.5 mm (`DEFAULT_VOXEL_SIZE_MM`). Existing TOML profiles
    /// without this field deserialise with the default via `#[serde(default)]`.
    #[serde(default = "default_voxel_size_mm")]
    pub(crate) voxel_size_mm: f32,

    /// How the printer separates cured layers from the FEP film. Selects the
    /// time-per-layer branch in `LayerTimingCalculator`. Legacy TOML profiles
    /// without this field deserialise as `Linear` via `#[serde(default)]`
    /// (matches classic MSLA arm-lift behaviour).
    #[serde(default)]
    pub(crate) release_mechanism: ReleaseMechanism,

    // --- LED thermal calibration (ADR-0007, KB-152) ---
    //
    // DELTA semantics — parallels existing `delta_t_steady_c` (the vat-level rise).
    // NOT an absolute temperature. `ThermalCalculator` stage A computes:
    //   led_temp(t) = initial_led_c + led_delta_t_steady_c × (1 - exp(-t/tau))
    //
    /// Steady-state LED-case temperature rise above `initial_led_c`. Unit: °C.
    /// For Mars 5 Ultra: 13.5 °C (plateau ~40.5 °C − idle ~27 °C from
    /// `data/elegoo/roden_uv_led_temp_dec_jan_hourly.csv`).
    #[serde(default = "default_led_delta_t_steady_c")]
    pub(crate) led_delta_t_steady_c: f32,

    /// Time constant of the LED heat-up curve. Unit: seconds. For Mars 5
    /// Ultra: 4000 s (3–4 h to 95 % of plateau ⇒ 3τ ≈ 3–4 h).
    #[serde(default = "default_led_tau_sec")]
    pub(crate) led_tau_sec: f32,

    /// LED → vat coupling factor. Dimensionless, [0, 1]. `ThermalCalculator`
    /// stage B computes: `vat_temp = ambient_c + coupling × (led_temp - ambient_c)`.
    /// For Mars 5 Ultra: 0.71 (user estimate — LED 40, vat ~35, ambient 23 ⇒
    /// ΔT_led = 17, ΔT_vat = 12, 12/17 ≈ 0.71). ESTIMATE only; the user has no
    /// vat sensor. Re-calibrate when one is added.
    #[serde(default = "default_led_to_vat_coupling")]
    pub(crate) led_to_vat_coupling: f32,

    /// Hardware build-envelope dimensions (ADR-0012). Optional so a profile
    /// can ship before its dimensions are confirmed (Athena II in v1).
    /// Consumed by `resinsim-viz` to size the build plate and frame the
    /// camera; `None` triggers a viz-side fallback to CTB header
    /// bed_size_mm + sentinel max_z + a one-shot warn.
    #[serde(default)]
    pub(crate) build_envelope_mm: Option<BuildEnvelope>,

    /// Voxel cure resolution (mm) for the t2f1 voxel cure path (ADR-0017).
    /// **Optional** + **forward-compat reservation only** in v1.
    ///
    /// The plan called for a CLI > profile > default 0.2 mm precedence
    /// chain, but the v1 implementation uses the slicer mask's voxel size
    /// for X/Y and `recipe.layer_height_um` for Z unconditionally. This
    /// field is parsed and validated (finite > 0 when Some) so existing
    /// profile TOMLs forward-compat; it is NOT read by `SimulationRunner`
    /// at runtime. t2f5 (GPU acceleration + resolution decoupling) will
    /// activate it.
    ///
    /// Profile authors can leave this `None` for v1; setting it documents
    /// intent for future calibration but has no current effect.
    #[serde(default)]
    pub(crate) voxel_cure_resolution_mm: Option<f32>,

    /// Lateral (XY) light crosstalk standard deviation (µm). ADR-0018 §2 / t2f2.
    ///
    /// When `Some(σ)`, the Tier-2 voxel cure path applies a separable 2D
    /// Gaussian convolution with this σ to the per-layer pixel intensity
    /// grid BEFORE the Beer-Lambert column march — captures LCD-source
    /// crosstalk (pixel pitch + collimation) plus the lateral component
    /// of volumetric resin scatter as an empirical lumped parameter.
    ///
    /// When `None`, no XY pre-convolution is applied (the per-pixel
    /// exposure path matches t2f1 behaviour at the XY level).
    ///
    /// Conversion to voxel-index σ: `σ_voxels = σ_xy_um / (mask.voxel_size_mm × 1000)`.
    ///
    /// Validation: `Some(s)` requires `s.is_finite() && s > 0.0 && s <= MAX_SIGMA_UM`.
    /// Existing TOML profiles without this field deserialise to `None` via
    /// `#[serde(default)]`.
    #[serde(default)]
    pub(crate) crosstalk_sigma_xy_um: Option<f32>,

    /// Axial (Z) light crosstalk standard deviation (µm). ADR-0018 §2 / t2f2.
    ///
    /// When `Some(σ)`, the Tier-2 voxel cure path applies a 1D Gaussian
    /// convolution with this σ along the Z (layer) axis to the per-column
    /// cure-dose AND PI-depletion deltas AFTER the Beer-Lambert column
    /// march — captures the axial component of volumetric resin scatter
    /// as an empirical lumped parameter. Co-scattered with the cure-dose
    /// delta to preserve consistency between deposited dose and resulting
    /// initiator depletion.
    ///
    /// When `None`, no Z post-convolution is applied (cure dose stays in
    /// its Beer-Lambert column).
    ///
    /// Conversion to layer-index σ: `σ_layers = σ_z_um / layer_height_um`.
    /// The Z-direction kernel is therefore in LAYER units, not physical
    /// µm — the per-axis kernels are anisotropic in INDEX space because
    /// the voxel-field storage is anisotropic
    /// (docs/patterns/voxel-field-z-dimension-is-layer-count.md).
    ///
    /// Validation: `Some(s)` requires `s.is_finite() && s > 0.0 && s <= MAX_SIGMA_UM`.
    /// Existing TOML profiles without this field deserialise to `None` via
    /// `#[serde(default)]`.
    #[serde(default)]
    pub(crate) crosstalk_sigma_z_um: Option<f32>,

    /// Convective heat-transfer coefficient at the vat outer walls. Unit:
    /// W/(m²·K). ADR-0020 / t2f4. Drives the Newton-cooling boundary
    /// condition at the four side faces of the thermal field, lumped
    /// through `1/h_eff = 1/h_air + wall_thickness/wall_k`. Still-air
    /// natural convection ~8 W/m²·K is the literature midpoint.
    /// **Optional on the struct** so cross-feature TOML interchange holds;
    /// **REQUIRED at validate() time under the `field-sim` feature** —
    /// None then is a typed validate-time error per ADR-0020 §Consequences.
    #[serde(default)]
    pub(crate) convective_wall_h_w_m2k: Option<f32>,

    /// Vat wall thickness. Unit: mm. ADR-0020. Lumped into the side-face
    /// convective BC via the series-resistance formula above. Typical
    /// MSLA vat walls are ~2 mm of Al alloy. Same Option-on-struct /
    /// required-under-field-sim semantics.
    #[serde(default)]
    pub(crate) vat_wall_thickness_mm: Option<f32>,

    /// Vat wall thermal conductivity. Unit: W/(m·K). ADR-0020. Used in
    /// the side-face convective BC's series-resistance formula. Al alloy
    /// ~200 W/m·K is the literature midpoint for Mars/Saturn-class
    /// printers. Same Option-on-struct / required-under-field-sim
    /// semantics.
    #[serde(default)]
    pub(crate) vat_wall_k_w_mk: Option<f32>,

    /// Partial-vacuum pressure ΔP assumed at a sealed cavity during peel. Unit:
    /// kPa. ADR-0022 Stage 2 / KB-184. **Optional**; `None` inherits
    /// [`DEFAULT_VACUUM_PRESSURE_KPA`] (50 kPa). The 50–101 kPa magnitude is a
    /// data gap (KB-184) pending E4 / KB-173 sealed-vs-drained calibration, so the
    /// default is INDICATIVE. When `Some`, validated finite, > 0, and <=
    /// [`ATMOSPHERIC_PRESSURE_KPA`]. Consumed by the `CavityDetector` suction
    /// pre-pass, which computes force via `PeelForceCalculator::suction_force`.
    /// Existing TOML profiles without this field deserialise to `None` via
    /// `#[serde(default)]`.
    #[serde(default)]
    pub(crate) vacuum_pressure_kpa: Option<f32>,
}

impl PrinterProfile {
    /// Printer profile identity (used for display + matching by name).
    pub fn name(&self) -> &str {
        &self.name
    }

    // --- Public read-only accessors (pub(crate) fields per validate-on-mutation contract) ---

    pub fn z_stiffness_n_per_mm(&self) -> f32 {
        self.z_stiffness_n_per_mm
    }
    pub fn delta_t_steady_c(&self) -> f32 {
        self.delta_t_steady_c
    }
    pub fn thermal_tau_sec(&self) -> f32 {
        self.thermal_tau_sec
    }
    pub fn led_power_mw_cm2(&self) -> f32 {
        self.led_power_mw_cm2
    }
    pub fn pixel_pitch_um(&self) -> f32 {
        self.pixel_pitch_um
    }
    pub fn lcd_uniformity_variation(&self) -> f32 {
        self.lcd_uniformity_variation
    }
    pub fn layer_height_range_um(&self) -> FloatRange {
        self.layer_height_range_um
    }
    pub fn exposure_range_sec(&self) -> FloatRange {
        self.exposure_range_sec
    }
    pub fn lift_speed_range_mm_min(&self) -> FloatRange {
        self.lift_speed_range_mm_min
    }
    pub fn bottom_layer_count_max(&self) -> u32 {
        self.bottom_layer_count_max
    }
    pub fn voxel_size_mm(&self) -> f32 {
        self.voxel_size_mm
    }
    pub fn release_mechanism(&self) -> ReleaseMechanism {
        self.release_mechanism
    }
    pub fn led_delta_t_steady_c(&self) -> f32 {
        self.led_delta_t_steady_c
    }
    pub fn led_tau_sec(&self) -> f32 {
        self.led_tau_sec
    }
    pub fn led_to_vat_coupling(&self) -> f32 {
        self.led_to_vat_coupling
    }
    pub fn build_envelope_mm(&self) -> Option<BuildEnvelope> {
        self.build_envelope_mm
    }
    /// Per-printer voxel cure resolution override (mm). See struct doc.
    pub fn voxel_cure_resolution_mm(&self) -> Option<f32> {
        self.voxel_cure_resolution_mm
    }
    /// Convective heat-transfer coefficient at the vat outer walls. ADR-0020.
    pub fn convective_wall_h_w_m2k(&self) -> Option<f32> {
        self.convective_wall_h_w_m2k
    }
    /// Vat wall thickness (mm). ADR-0020.
    pub fn vat_wall_thickness_mm(&self) -> Option<f32> {
        self.vat_wall_thickness_mm
    }
    /// Vat wall thermal conductivity (W/m·K). ADR-0020.
    pub fn vat_wall_k_w_mk(&self) -> Option<f32> {
        self.vat_wall_k_w_mk
    }
    /// Effective voxel cure resolution after applying the precedence chain
    /// **without** the CLI level — i.e. the value the simulation uses when
    /// the CLI flag is absent. Returns the per-printer override if Some,
    /// otherwise [`DEFAULT_VOXEL_CURE_RESOLUTION_MM`]. The CLI layer
    /// (resinsim-inspect) applies the highest-priority override before
    /// consulting this method.
    pub fn effective_voxel_cure_resolution_mm(&self) -> f32 {
        self.voxel_cure_resolution_mm
            .unwrap_or(DEFAULT_VOXEL_CURE_RESOLUTION_MM)
    }

    /// Lateral light crosstalk σ in µm — see struct field doc. ADR-0018 / t2f2.
    pub fn crosstalk_sigma_xy_um(&self) -> Option<f32> {
        self.crosstalk_sigma_xy_um
    }

    /// Axial light crosstalk σ in µm — see struct field doc. ADR-0018 / t2f2.
    pub fn crosstalk_sigma_z_um(&self) -> Option<f32> {
        self.crosstalk_sigma_z_um
    }

    /// Suction ΔP for this printer in kPa, if explicitly set. See struct field
    /// doc. ADR-0022 Stage 2.
    pub fn vacuum_pressure_kpa(&self) -> Option<f32> {
        self.vacuum_pressure_kpa
    }

    /// Effective suction ΔP (kPa): the profile override if set, else
    /// [`DEFAULT_VACUUM_PRESSURE_KPA`]. This is the value the `CavityDetector`
    /// suction pre-pass multiplies by the sealed area. ADR-0022 Stage 2.
    pub fn effective_vacuum_pressure_kpa(&self) -> f32 {
        self.vacuum_pressure_kpa
            .unwrap_or(DEFAULT_VACUUM_PRESSURE_KPA)
    }

    /// Validate physical invariants. Must be called after deserialization from
    /// untrusted sources to prevent NaN/inf propagation through motion and
    /// thermal calculations.
    ///
    /// **Contract:** intra-crate code that mutates any field of a previously
    /// validated `PrinterProfile` MUST re-call `validate()` before passing the
    /// profile to a downstream service. See struct-level doc comment.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("printer name must not be empty".into());
        }
        let positive_checks: &[(f32, &str)] = &[
            (self.led_power_mw_cm2, "led_power_mw_cm2"),
            (self.pixel_pitch_um, "pixel_pitch_um"),
            (self.z_stiffness_n_per_mm, "z_stiffness_n_per_mm"),
            (self.thermal_tau_sec, "thermal_tau_sec"),
        ];
        for (val, field) in positive_checks {
            if !val.is_finite() || *val <= 0.0 {
                return Err(format!("{field} must be finite and > 0 (got {val})"));
            }
        }
        self.layer_height_range_um
            .validate()
            .map_err(|e| format!("layer_height_range_um: {e}"))?;
        self.exposure_range_sec
            .validate()
            .map_err(|e| format!("exposure_range_sec: {e}"))?;
        self.lift_speed_range_mm_min
            .validate()
            .map_err(|e| format!("lift_speed_range_mm_min: {e}"))?;
        if self.bottom_layer_count_max == 0 {
            return Err("bottom_layer_count_max must be >= 1".into());
        }
        if !self.delta_t_steady_c.is_finite() {
            return Err(format!(
                "delta_t_steady_c must be finite (got {})",
                self.delta_t_steady_c
            ));
        }
        if !(0.0..=1.0).contains(&self.lcd_uniformity_variation)
            || !self.lcd_uniformity_variation.is_finite()
        {
            return Err(format!(
                "lcd_uniformity_variation must be in [0.0, 1.0] (got {})",
                self.lcd_uniformity_variation
            ));
        }
        if !self.voxel_size_mm.is_finite() || self.voxel_size_mm <= 0.0 {
            return Err(format!(
                "voxel_size_mm must be finite and > 0 (got {})",
                self.voxel_size_mm
            ));
        }
        if !self.led_delta_t_steady_c.is_finite() {
            return Err(format!(
                "led_delta_t_steady_c must be finite (got {})",
                self.led_delta_t_steady_c
            ));
        }
        if !self.led_tau_sec.is_finite() || self.led_tau_sec <= 0.0 {
            return Err(format!(
                "led_tau_sec must be finite and > 0 (got {})",
                self.led_tau_sec
            ));
        }
        if !self.led_to_vat_coupling.is_finite() || !(0.0..=1.0).contains(&self.led_to_vat_coupling)
        {
            return Err(format!(
                "led_to_vat_coupling must be in [0.0, 1.0] (got {})",
                self.led_to_vat_coupling
            ));
        }
        if let Some(env) = self.build_envelope_mm.as_ref() {
            env.validate()?;
        }
        // ADR-0017 / t2f1: voxel_cure_resolution_mm is Optional; when Some,
        // must be finite > 0. NaN-two-layer-defence: explicit is_finite()
        // check before the > 0.0 comparison to avoid the IEEE-754 NaN gap.
        if let Some(vmm) = self.voxel_cure_resolution_mm
            && (!vmm.is_finite() || vmm <= 0.0)
        {
            return Err(format!(
                "voxel_cure_resolution_mm, when present, must be finite and \
                 > 0.0 mm (got {vmm})"
            ));
        }
        // ADR-0018 / t2f2: crosstalk_sigma_{xy,z}_um are Optional; when Some,
        // must be finite, > 0, and <= MAX_SIGMA_UM. NaN-two-layer-defence:
        // explicit is_finite() check before numeric comparisons. The
        // upper-bound rejects misconfigured TOMLs (kernels spanning the
        // entire build envelope are almost certainly a calibration error,
        // not a real measurement).
        for (label, value) in [
            ("crosstalk_sigma_xy_um", self.crosstalk_sigma_xy_um),
            ("crosstalk_sigma_z_um", self.crosstalk_sigma_z_um),
        ] {
            if let Some(s) = value
                && (!s.is_finite() || s <= 0.0 || s > MAX_SIGMA_UM)
            {
                return Err(format!(
                    "{label}, when present, must be finite, > 0.0 µm, and \
                     <= {MAX_SIGMA_UM} µm (got {s})"
                ));
            }
        }
        // ADR-0020 / t2f4: thermal vat-wall + convective BC fields. Same
        // Option-on-struct shape as the rest of the optional KB-derived
        // fields; each present value passes through its typed-boundary VO
        // constructor. NaN-two-layer-defence (finite check first).
        if let Some(h) = self.convective_wall_h_w_m2k {
            crate::values::ConvectiveCoefficient::new(h)
                .map_err(|e| format!("convective_wall_h_w_m2k: {e}"))?;
        }
        if let Some(t) = self.vat_wall_thickness_mm {
            crate::values::VatWallThickness::new(t)
                .map_err(|e| format!("vat_wall_thickness_mm: {e}"))?;
        }
        if let Some(k) = self.vat_wall_k_w_mk {
            crate::values::ThermalConductivity::new(k)
                .map_err(|e| format!("vat_wall_k_w_mk: {e}"))?;
        }
        // ADR-0022 Stage 2: optional suction ΔP. When present, must be finite,
        // > 0, and not exceed atmospheric (a sealed-cavity vacuum cannot pull
        // harder than one atmosphere). NaN-two-layer-defence: finite check first.
        if let Some(p) = self.vacuum_pressure_kpa
            && (!p.is_finite() || p <= 0.0 || p > ATMOSPHERIC_PRESSURE_KPA)
        {
            return Err(format!(
                "vacuum_pressure_kpa, when present, must be finite, > 0.0 kPa, and \
                 <= {ATMOSPHERIC_PRESSURE_KPA} kPa (atmospheric) (got {p})"
            ));
        }
        #[cfg(feature = "field-sim")]
        {
            if self.build_envelope_mm.is_none() {
                return Err(
                    "build_envelope_mm is required under the field-sim feature \
                     (ADR-0020 §Decision ii — ThermalField is anchored to the \
                     vat envelope). Add `[build_envelope_mm]` block to the \
                     printer TOML with x_mm, y_mm, z_mm."
                        .into(),
                );
            }
            if self.convective_wall_h_w_m2k.is_none() {
                return Err(
                    "convective_wall_h_w_m2k is required under the field-sim \
                     feature (ADR-0020). Still-air natural convection ~8 \
                     W/m²·K is the literature midpoint — set it in the \
                     printer TOML."
                        .into(),
                );
            }
            if self.vat_wall_thickness_mm.is_none() {
                return Err(
                    "vat_wall_thickness_mm is required under the field-sim \
                     feature (ADR-0020). Typical MSLA vat walls are ~2.0 mm \
                     of Al alloy — set it in the printer TOML."
                        .into(),
                );
            }
            if self.vat_wall_k_w_mk.is_none() {
                return Err(
                    "vat_wall_k_w_mk is required under the field-sim feature \
                     (ADR-0020). Al alloy ~200 W/m·K is the literature \
                     midpoint — set it in the printer TOML."
                        .into(),
                );
            }
        }
        Ok(())
    }

    /// Elegoo Mars 5 Ultra — 12K mono LCD, ParaLED, triple linear rail.
    /// Sources: Elegoo published specs; ranges derived from published hardware envelope
    /// (KB-131 for Z stiffness, Elegoo spec sheet for layer/exposure/speed bounds).
    /// z_stiffness estimated from triple-rail geometry (calibrate with Athena II data).
    pub fn elegoo_mars5_ultra() -> Self {
        Self {
            name: "Elegoo Mars 5 Ultra".into(),
            led_power_mw_cm2: 20.0, // ParaLED matrix, 12K mono LCD
            pixel_pitch_um: 19.0,   // 218.88 mm / 11520 px
            layer_height_range_um: FloatRange::new(10.0, 150.0)
                .expect("Mars 5 Ultra layer-height range 10..150 µm is valid"),
            exposure_range_sec: FloatRange::new(0.5, 60.0)
                .expect("Mars 5 Ultra exposure range 0.5..60 sec is valid"),
            lift_speed_range_mm_min: FloatRange::new(10.0, 300.0)
                .expect("Mars 5 Ultra lift-speed range 10..300 mm/min is valid"),
            bottom_layer_count_max: 20,
            z_stiffness_n_per_mm: 900.0, // estimated — triple linear rail
            delta_t_steady_c: 8.0,
            thermal_tau_sec: 1200.0,
            lcd_uniformity_variation: 0.12, // ParaLED, better than Saturn-class
            voxel_size_mm: DEFAULT_VOXEL_SIZE_MM,
            release_mechanism: ReleaseMechanism::Tilt, // ADR-0007: Mars 5 Ultra uses tilting vat release
            // LED thermal (KB-152) — fitted from data/elegoo/ overnight session.
            led_delta_t_steady_c: 13.5, // plateau 40.5 − idle 27
            led_tau_sec: 4000.0,        // 3–4 h to 95 % of plateau (3τ ≈ 3–4 h)
            led_to_vat_coupling: 0.71,  // user estimate — no vat sensor; recalibrate when added
            // Build envelope (ADR-0012). Elegoo published spec.
            build_envelope_mm: Some(BuildEnvelope {
                width_mm: 153.36,
                depth_mm: 77.76,
                max_z_mm: 165.0,
            }),
            // ADR-0017 / t2f1: leave None to inherit the workspace default
            // (0.2 mm) for the voxel cure path. Mars 5 Ultra's 19 µm pixel
            // pitch could justify finer voxels, but until per-printer voxel
            // calibration data exists, the default is the right call.
            voxel_cure_resolution_mm: None,
            // ADR-0018 / t2f2: Mars 5 Ultra crosstalk ESTIMATE pending
            // Athena II beam-profile fit. σ_xy ≈ 8 µm cross-checked against
            // Wei et al. PMC11267290 (σ/pixel_pitch ≈ 0.36 → 19 µm pitch
            // gives σ ≈ 6.8 µm; 8 µm is conservative round) and KB-121 LED/LCD
            // geometry (pixel pitch × tan(5-10° collimation) ≈ 8 µm).
            // σ_z = 40 µm is the mid-range of typical photopolymer scatter
            // mean-free-path (20-80 µm).
            crosstalk_sigma_xy_um: Some(8.0),
            crosstalk_sigma_z_um: Some(40.0),
            // ADR-0020 / t2f4: vat-wall + convective BC properties.
            // Mars 5 Ultra has a typical Al-alloy vat frame.
            convective_wall_h_w_m2k: Some(8.0),  // still-air natural convection
            vat_wall_thickness_mm: Some(2.0),    // typical Al-alloy vat
            vat_wall_k_w_mk: Some(200.0),        // Al alloy thermal conductivity
            // ADR-0022 Stage 2: no measured suction ΔP yet — inherit the 50 kPa
            // indicative default (KB-184 data gap).
            vacuum_pressure_kpa: None,
        }
    }

    /// Generic MSLA 4K printer with conservative defaults.
    pub fn generic_msla_4k() -> Self {
        Self {
            name: "Generic MSLA 4K".into(),
            led_power_mw_cm2: 4.0, // KB-121: typical LCD printer
            pixel_pitch_um: 50.0,
            layer_height_range_um: FloatRange::new(20.0, 100.0)
                .expect("generic MSLA 4K layer-height range 20..100 µm is valid"),
            exposure_range_sec: FloatRange::new(1.0, 60.0)
                .expect("generic MSLA 4K exposure range 1..60 sec is valid"),
            lift_speed_range_mm_min: FloatRange::new(10.0, 200.0)
                .expect("generic MSLA 4K lift-speed range 10..200 mm/min is valid"),
            bottom_layer_count_max: 15,
            z_stiffness_n_per_mm: 460.0,    // KB-131: Elegoo Mars class
            delta_t_steady_c: 10.0,         // KB-150: estimate
            thermal_tau_sec: 1200.0,        // KB-183: estimate
            lcd_uniformity_variation: 0.22, // KB-120: Saturn 2 class
            voxel_size_mm: DEFAULT_VOXEL_SIZE_MM,
            release_mechanism: ReleaseMechanism::Linear, // classic MSLA arm-lift
            // LED thermal — conservative defaults until per-printer calibration exists.
            led_delta_t_steady_c: 10.0,
            led_tau_sec: 1200.0,
            led_to_vat_coupling: 0.5,
            // Build envelope (ADR-0012). Typical 8.9" / 4K monoLCD class.
            build_envelope_mm: Some(BuildEnvelope {
                width_mm: 192.0,
                depth_mm: 120.0,
                max_z_mm: 200.0,
            }),
            voxel_cure_resolution_mm: None, // ADR-0017: inherit default
            // ADR-0018 / t2f2: leave both fields unset so the generic 4K
            // profile runs the t2f1 no-crosstalk path. Per-printer
            // crosstalk calibration is required to activate Tier-2 light
            // crosstalk modelling.
            crosstalk_sigma_xy_um: None,
            crosstalk_sigma_z_um: None,
            // ADR-0020 / t2f4: vat-wall + convective BC properties.
            // Conservative literature midpoints — no per-printer calibration
            // for the generic 4K class.
            convective_wall_h_w_m2k: Some(8.0),
            vat_wall_thickness_mm: Some(2.0),
            vat_wall_k_w_mk: Some(200.0),
            // ADR-0022 Stage 2: inherit the 50 kPa indicative default (KB-184).
            vacuum_pressure_kpa: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_msla_4k_passes_validation() {
        PrinterProfile::generic_msla_4k()
            .validate()
            .expect("PrinterProfile::generic_msla_4k() factory must satisfy validate()");
    }

    #[test]
    fn elegoo_mars5_ultra_passes_validation() {
        PrinterProfile::elegoo_mars5_ultra()
            .validate()
            .expect("PrinterProfile::elegoo_mars5_ultra() factory must satisfy validate()");
    }

    #[test]
    fn uniformity_above_one_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.lcd_uniformity_variation = 1.5;
        assert!(p.validate().is_err());
    }

    #[test]
    fn infinite_z_stiffness_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.z_stiffness_n_per_mm = f32::INFINITY;
        assert!(p.validate().is_err());
    }

    #[test]
    fn zero_bottom_layer_count_max_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.bottom_layer_count_max = 0;
        assert!(p.validate().is_err());
    }

    // ADR-0018 / t2f2: crosstalk σ validation.

    #[test]
    fn crosstalk_sigma_xy_um_zero_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.crosstalk_sigma_xy_um = Some(0.0);
        let err = p.validate().expect_err("σ_xy = 0.0 must be rejected");
        assert!(err.contains("crosstalk_sigma_xy_um"));
    }

    #[test]
    fn crosstalk_sigma_z_um_zero_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.crosstalk_sigma_z_um = Some(0.0);
        let err = p.validate().expect_err("σ_z = 0.0 must be rejected");
        assert!(err.contains("crosstalk_sigma_z_um"));
    }

    #[test]
    fn crosstalk_sigma_xy_um_nan_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.crosstalk_sigma_xy_um = Some(f32::NAN);
        assert!(p.validate().is_err());
    }

    #[test]
    fn crosstalk_sigma_z_um_negative_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.crosstalk_sigma_z_um = Some(-1.0);
        assert!(p.validate().is_err());
    }

    #[test]
    fn crosstalk_sigma_xy_um_above_max_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.crosstalk_sigma_xy_um = Some(MAX_SIGMA_UM + 1.0);
        let err = p
            .validate()
            .expect_err("σ_xy above MAX_SIGMA_UM must be rejected");
        assert!(err.contains("crosstalk_sigma_xy_um"));
    }

    #[test]
    fn crosstalk_sigma_z_um_above_max_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.crosstalk_sigma_z_um = Some(MAX_SIGMA_UM + 1.0);
        let err = p
            .validate()
            .expect_err("σ_z above MAX_SIGMA_UM must be rejected");
        assert!(err.contains("crosstalk_sigma_z_um"));
    }

    #[test]
    fn crosstalk_sigmas_none_accepted() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.crosstalk_sigma_xy_um = None;
        p.crosstalk_sigma_z_um = None;
        p.validate()
            .expect("both σ fields None must satisfy validate (t2f1 path)");
    }

    #[test]
    fn crosstalk_sigmas_at_upper_bound_accepted() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.crosstalk_sigma_xy_um = Some(MAX_SIGMA_UM);
        p.crosstalk_sigma_z_um = Some(MAX_SIGMA_UM);
        p.validate()
            .expect("σ values equal to MAX_SIGMA_UM must satisfy validate (boundary inclusive)");
    }

    #[test]
    fn crosstalk_sigma_accessors_round_trip() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.crosstalk_sigma_xy_um = Some(8.0);
        p.crosstalk_sigma_z_um = Some(40.0);
        assert_eq!(p.crosstalk_sigma_xy_um(), Some(8.0));
        assert_eq!(p.crosstalk_sigma_z_um(), Some(40.0));
    }

    #[test]
    fn inverted_layer_height_range_rejected_via_validate() {
        let mut p = PrinterProfile::generic_msla_4k();
        // Directly mutate pub(crate) field inside the range to produce a min > max shape.
        // Bypasses FloatRange::new, reaches PrinterProfile::validate() delegating through.
        p.layer_height_range_um.min = 200.0;
        p.layer_height_range_um.max = 20.0;
        let err = p.validate().expect_err("inverted range must fail validate");
        assert!(
            err.contains("layer_height_range_um") && err.contains("min"),
            "error names the offending range: {err}"
        );
    }

    // Contract demonstration — see PrinterProfile struct doc comment.
    // Distinct from numeric-range tests because this exercises the "previously-VALID →
    // mutated → invalid" sequence on a string field, demonstrating the contract requires
    // re-running validate() after intra-crate mutation.
    #[test]
    fn validate_after_mutation_contract() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.validate().expect("baseline profile must be valid");
        p.name = "   ".into();
        assert!(
            p.validate().is_err(),
            "validate() must be re-called after intra-crate field mutation; \
             whitespace name should now be rejected"
        );
    }

    // --- Parse-path (serde) tests locking that NaN bounds are caught by validate(). ---

    fn valid_printer_toml() -> String {
        // SCALARS ONLY — no table headers — so callers can append additional
        // scalar fields OR a `[build_envelope_mm]` table without TOML
        // parse-scoping conflicts. ADR-0020 / t2f4: the new thermal
        // scalars are included so this TOML satisfies validate() under
        // field-sim once a `[build_envelope_mm]` block is also appended.
        r#"
name = "Test Printer"
led_power_mw_cm2 = 4.0
pixel_pitch_um = 50.0
layer_height_range_um = { min = 20.0, max = 100.0 }
exposure_range_sec = { min = 1.0, max = 60.0 }
lift_speed_range_mm_min = { min = 10.0, max = 200.0 }
bottom_layer_count_max = 15
z_stiffness_n_per_mm = 460.0
delta_t_steady_c = 10.0
thermal_tau_sec = 1200.0
lcd_uniformity_variation = 0.22
convective_wall_h_w_m2k = 8.0
vat_wall_thickness_mm = 2.0
vat_wall_k_w_mk = 200.0
"#
        .to_string()
    }

    /// `valid_printer_toml()` + an inline `[build_envelope_mm]` table.
    /// ADR-0020 / t2f4: a complete field-sim-valid fixture.
    fn valid_printer_toml_with_envelope() -> String {
        valid_printer_toml()
            + "[build_envelope_mm]\nwidth_mm = 192.0\ndepth_mm = 120.0\nmax_z_mm = 200.0\n"
    }

    #[test]
    fn parse_toml_then_validate_accepts_valid() {
        // ADR-0020 / t2f4: scalars-only fixture is valid under default-feature
        // builds; field-sim builds additionally require a build_envelope_mm
        // block. Cover both via the with_envelope helper.
        let p: PrinterProfile = toml::from_str(&valid_printer_toml_with_envelope())
            .expect("valid printer TOML must parse");
        p.validate().expect("valid TOML must satisfy validate()");
    }

    #[test]
    fn parse_toml_with_nan_range_bound_rejected() {
        let toml_str = valid_printer_toml().replace(
            "layer_height_range_um = { min = 20.0, max = 100.0 }",
            "layer_height_range_um = { min = nan, max = 100.0 }",
        );
        let p: PrinterProfile =
            toml::from_str(&toml_str).expect("TOML parse succeeds; validate() is the gate");
        let err = p
            .validate()
            .expect_err("NaN range bound must fail validate()");
        assert!(
            err.contains("layer_height_range_um"),
            "error names the range: {err}"
        );
    }

    // --- ReleaseMechanism (ADR-0007) tests ---

    #[test]
    fn mars5_ultra_factory_is_tilt() {
        assert_eq!(
            PrinterProfile::elegoo_mars5_ultra().release_mechanism(),
            ReleaseMechanism::Tilt,
        );
    }

    #[test]
    fn generic_msla_4k_factory_is_linear() {
        assert_eq!(
            PrinterProfile::generic_msla_4k().release_mechanism(),
            ReleaseMechanism::Linear,
        );
    }

    #[test]
    fn legacy_toml_without_release_mechanism_defaults_to_linear() {
        // Legacy TOML profiles (athena_ii, generic_msla_4k written before ADR-0007)
        // must deserialise with release_mechanism = Linear via #[serde(default)].
        let p: PrinterProfile = toml::from_str(&valid_printer_toml())
            .expect("valid printer TOML without release_mechanism must parse");
        assert_eq!(p.release_mechanism(), ReleaseMechanism::Linear);
    }

    #[test]
    fn toml_with_explicit_tilt_deserialises() {
        let toml_str = valid_printer_toml() + "release_mechanism = \"tilt\"\n";
        let p: PrinterProfile = toml::from_str(&toml_str).expect("explicit tilt must parse");
        assert_eq!(p.release_mechanism(), ReleaseMechanism::Tilt);
    }

    #[test]
    fn toml_with_explicit_linear_deserialises() {
        let toml_str = valid_printer_toml() + "release_mechanism = \"linear\"\n";
        let p: PrinterProfile = toml::from_str(&toml_str).expect("explicit linear must parse");
        assert_eq!(p.release_mechanism(), ReleaseMechanism::Linear);
    }

    // --- LED thermal fields (KB-152) tests ---

    #[test]
    fn mars5_ultra_led_thermal_fitted_values() {
        let p = PrinterProfile::elegoo_mars5_ultra();
        assert!((p.led_delta_t_steady_c() - 13.5).abs() < 1e-4);
        assert!((p.led_tau_sec() - 4000.0).abs() < 1e-4);
        assert!((p.led_to_vat_coupling() - 0.71).abs() < 1e-4);
    }

    #[test]
    fn legacy_toml_gets_conservative_led_defaults() {
        // Valid TOML without led_* fields → serde defaults kick in (10.0, 1200, 0.5).
        // Use the with_envelope variant so field-sim validate() also passes.
        let p: PrinterProfile = toml::from_str(&valid_printer_toml_with_envelope())
            .expect("legacy printer TOML without led_* fields must parse");
        assert_eq!(p.led_delta_t_steady_c(), 10.0);
        assert_eq!(p.led_tau_sec(), 1200.0);
        assert_eq!(p.led_to_vat_coupling(), 0.5);
        p.validate()
            .expect("legacy defaults must satisfy validate()");
    }

    #[test]
    fn led_tau_rejects_zero() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.led_tau_sec = 0.0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn led_tau_rejects_negative() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.led_tau_sec = -1.0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn led_tau_rejects_nan() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.led_tau_sec = f32::NAN;
        assert!(p.validate().is_err());
    }

    #[test]
    fn led_coupling_rejects_above_one() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.led_to_vat_coupling = 1.5;
        assert!(p.validate().is_err());
    }

    #[test]
    fn led_coupling_rejects_negative() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.led_to_vat_coupling = -0.1;
        assert!(p.validate().is_err());
    }

    #[test]
    fn led_coupling_boundary_zero_and_one_accepted() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.led_to_vat_coupling = 0.0;
        assert!(p.validate().is_ok());
        p.led_to_vat_coupling = 1.0;
        assert!(p.validate().is_ok());
    }

    #[test]
    fn led_delta_t_rejects_nan() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.led_delta_t_steady_c = f32::NAN;
        assert!(p.validate().is_err());
    }

    #[test]
    fn led_delta_t_accepts_zero() {
        // Zero rise is physically valid (perfectly dissipating printer).
        let mut p = PrinterProfile::generic_msla_4k();
        p.led_delta_t_steady_c = 0.0;
        assert!(p.validate().is_ok());
    }

    #[test]
    fn toml_with_unknown_release_mechanism_rejected_at_parse() {
        let toml_str = valid_printer_toml() + "release_mechanism = \"bogus\"\n";
        assert!(
            toml::from_str::<PrinterProfile>(&toml_str).is_err(),
            "unknown enum variant must fail parse (not validate)"
        );
    }

    // --- BuildEnvelope (ADR-0012) tests ---

    #[test]
    fn mars5_ultra_factory_populates_build_envelope() {
        let env = PrinterProfile::elegoo_mars5_ultra()
            .build_envelope_mm()
            .expect("Mars 5 Ultra factory must populate build_envelope_mm");
        assert!((env.width_mm - 153.36).abs() < 1e-4);
        assert!((env.depth_mm - 77.76).abs() < 1e-4);
        assert!((env.max_z_mm - 165.0).abs() < 1e-4);
    }

    #[test]
    fn generic_msla_4k_factory_populates_build_envelope() {
        let env = PrinterProfile::generic_msla_4k()
            .build_envelope_mm()
            .expect("generic_msla_4k factory must populate build_envelope_mm");
        assert!((env.width_mm - 192.0).abs() < 1e-4);
        assert!((env.depth_mm - 120.0).abs() < 1e-4);
        assert!((env.max_z_mm - 200.0).abs() < 1e-4);
    }

    #[test]
    fn legacy_toml_without_build_envelope_defaults_to_none() {
        // Legacy TOML profiles (athena_ii in v1) deserialise with
        // build_envelope_mm = None via #[serde(default)]. The field is
        // Optional per ADR-0012; ADR-0020 / t2f4 makes it REQUIRED under
        // the field-sim Cargo feature only — default-feature builds still
        // accept None.
        let p: PrinterProfile = toml::from_str(&valid_printer_toml())
            .expect("valid printer TOML without build_envelope_mm must parse");
        assert!(p.build_envelope_mm().is_none());
        #[cfg(not(feature = "field-sim"))]
        {
            p.validate()
                .expect("None build_envelope must satisfy validate() under default builds");
        }
        #[cfg(feature = "field-sim")]
        {
            let err = p.validate().expect_err(
                "field-sim build must reject None build_envelope per ADR-0020 §Decision ii",
            );
            assert!(
                err.contains("build_envelope_mm")
                    && err.contains("field-sim"),
                "error must name the field + the gating feature: {err}"
            );
        }
    }

    #[test]
    fn toml_with_explicit_build_envelope_round_trips() {
        let toml_str = valid_printer_toml()
            + "[build_envelope_mm]\nwidth_mm = 153.36\ndepth_mm = 77.76\nmax_z_mm = 165.0\n";
        let p: PrinterProfile =
            toml::from_str(&toml_str).expect("explicit build_envelope must parse");
        let env = p
            .build_envelope_mm()
            .expect("Some build_envelope after explicit TOML");
        assert!((env.width_mm - 153.36).abs() < 1e-4);
        assert!((env.depth_mm - 77.76).abs() < 1e-4);
        assert!((env.max_z_mm - 165.0).abs() < 1e-4);
    }

    #[test]
    fn build_envelope_with_zero_extent_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.build_envelope_mm = Some(BuildEnvelope {
            width_mm: 0.0,
            depth_mm: 120.0,
            max_z_mm: 200.0,
        });
        let err = p
            .validate()
            .expect_err("zero width_mm must fail validate()");
        assert!(
            err.contains("build_envelope_mm.width_mm"),
            "error names the offending field: {err}"
        );
    }

    #[test]
    fn build_envelope_with_negative_max_z_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.build_envelope_mm = Some(BuildEnvelope {
            width_mm: 192.0,
            depth_mm: 120.0,
            max_z_mm: -1.0,
        });
        assert!(p.validate().is_err());
    }

    #[test]
    fn build_envelope_with_nan_extent_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.build_envelope_mm = Some(BuildEnvelope {
            width_mm: f32::NAN,
            depth_mm: 120.0,
            max_z_mm: 200.0,
        });
        let err = p.validate().expect_err("NaN width_mm must fail validate()");
        assert!(
            err.contains("build_envelope_mm.width_mm"),
            "error names the offending field: {err}"
        );
    }

    #[test]
    fn build_envelope_with_infinite_extent_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.build_envelope_mm = Some(BuildEnvelope {
            width_mm: 192.0,
            depth_mm: f32::INFINITY,
            max_z_mm: 200.0,
        });
        assert!(p.validate().is_err());
    }

    #[test]
    fn parse_toml_with_min_greater_than_max_rejected() {
        let toml_str = valid_printer_toml().replace(
            "exposure_range_sec = { min = 1.0, max = 60.0 }",
            "exposure_range_sec = { min = 60.0, max = 1.0 }",
        );
        let p: PrinterProfile =
            toml::from_str(&toml_str).expect("TOML parse succeeds; validate() is the gate");
        let err = p
            .validate()
            .expect_err("inverted range bounds must fail validate()");
        assert!(
            err.contains("exposure_range_sec"),
            "error names the range: {err}"
        );
    }

    // --- ADR-0017 / t2f1 voxel_cure_resolution_mm tests ---

    #[test]
    fn voxel_cure_resolution_default_is_0_2_mm() {
        assert!((DEFAULT_VOXEL_CURE_RESOLUTION_MM - 0.2).abs() < 1e-6);
    }

    #[test]
    fn mars5_ultra_factory_inherits_voxel_cure_resolution_default() {
        let p = PrinterProfile::elegoo_mars5_ultra();
        assert_eq!(p.voxel_cure_resolution_mm(), None);
        // The effective value is the workspace default.
        assert!(
            (p.effective_voxel_cure_resolution_mm() - DEFAULT_VOXEL_CURE_RESOLUTION_MM).abs()
                < 1e-6
        );
    }

    #[test]
    fn generic_msla_4k_factory_inherits_voxel_cure_resolution_default() {
        let p = PrinterProfile::generic_msla_4k();
        assert_eq!(p.voxel_cure_resolution_mm(), None);
        assert!(
            (p.effective_voxel_cure_resolution_mm() - DEFAULT_VOXEL_CURE_RESOLUTION_MM).abs()
                < 1e-6
        );
    }

    #[test]
    fn voxel_cure_resolution_some_value_overrides_default() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.voxel_cure_resolution_mm = Some(0.05);
        assert!(p.validate().is_ok());
        assert_eq!(p.voxel_cure_resolution_mm(), Some(0.05));
        assert!((p.effective_voxel_cure_resolution_mm() - 0.05).abs() < 1e-6);
    }

    #[test]
    fn voxel_cure_resolution_zero_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.voxel_cure_resolution_mm = Some(0.0);
        assert!(p.validate().is_err());
    }

    #[test]
    fn voxel_cure_resolution_negative_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.voxel_cure_resolution_mm = Some(-0.1);
        assert!(p.validate().is_err());
    }

    #[test]
    fn voxel_cure_resolution_nan_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.voxel_cure_resolution_mm = Some(f32::NAN);
        assert!(p.validate().is_err());
    }

    #[test]
    fn voxel_cure_resolution_infinity_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.voxel_cure_resolution_mm = Some(f32::INFINITY);
        assert!(p.validate().is_err());
    }

    #[test]
    fn legacy_toml_without_voxel_cure_resolution_defaults_to_none() {
        let p: PrinterProfile = toml::from_str(&valid_printer_toml_with_envelope())
            .expect("legacy TOML without voxel_cure_resolution_mm must parse");
        assert_eq!(p.voxel_cure_resolution_mm(), None);
        p.validate()
            .expect("None voxel_cure_resolution_mm must satisfy validate()");
    }

    #[test]
    fn toml_with_explicit_voxel_cure_resolution_round_trips() {
        // voxel_cure_resolution_mm = 0.1 is a top-level scalar so it must
        // precede any [table] block. Append to scalars-only TOML then add
        // the build_envelope_mm table after.
        let toml_str = valid_printer_toml()
            + "voxel_cure_resolution_mm = 0.1\n"
            + "[build_envelope_mm]\nwidth_mm = 192.0\ndepth_mm = 120.0\nmax_z_mm = 200.0\n";
        let p: PrinterProfile =
            toml::from_str(&toml_str).expect("explicit voxel_cure_resolution_mm must parse");
        assert_eq!(p.voxel_cure_resolution_mm(), Some(0.1));
        p.validate()
            .expect("explicit 0.1 mm must satisfy validate()");
    }

    // --- ADR-0022 Stage 2: vacuum_pressure_kpa (suction ΔP) tests ---

    #[test]
    fn vacuum_pressure_default_is_50_kpa() {
        assert!((DEFAULT_VACUUM_PRESSURE_KPA - 50.0).abs() < 1e-6);
    }

    #[test]
    fn factories_inherit_vacuum_pressure_default() {
        for p in [
            PrinterProfile::generic_msla_4k(),
            PrinterProfile::elegoo_mars5_ultra(),
        ] {
            assert_eq!(p.vacuum_pressure_kpa(), None);
            assert!((p.effective_vacuum_pressure_kpa() - DEFAULT_VACUUM_PRESSURE_KPA).abs() < 1e-6);
        }
    }

    #[test]
    fn vacuum_pressure_some_overrides_default() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.vacuum_pressure_kpa = Some(80.0);
        assert!(p.validate().is_ok());
        assert_eq!(p.vacuum_pressure_kpa(), Some(80.0));
        assert!((p.effective_vacuum_pressure_kpa() - 80.0).abs() < 1e-6);
    }

    #[test]
    fn vacuum_pressure_zero_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.vacuum_pressure_kpa = Some(0.0);
        let err = p.validate().expect_err("ΔP = 0.0 must be rejected");
        assert!(err.contains("vacuum_pressure_kpa"), "{err}");
    }

    #[test]
    fn vacuum_pressure_negative_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.vacuum_pressure_kpa = Some(-1.0);
        assert!(p.validate().is_err());
    }

    #[test]
    fn vacuum_pressure_nan_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.vacuum_pressure_kpa = Some(f32::NAN);
        assert!(p.validate().is_err());
    }

    #[test]
    fn vacuum_pressure_above_atmospheric_rejected() {
        // A sealed-cavity vacuum cannot exceed one atmosphere pulling the plate.
        let mut p = PrinterProfile::generic_msla_4k();
        p.vacuum_pressure_kpa = Some(ATMOSPHERIC_PRESSURE_KPA + 1.0);
        let err = p
            .validate()
            .expect_err("ΔP above atmospheric must be rejected");
        assert!(err.contains("vacuum_pressure_kpa"), "{err}");
    }

    #[test]
    fn vacuum_pressure_at_atmospheric_accepted() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.vacuum_pressure_kpa = Some(ATMOSPHERIC_PRESSURE_KPA);
        p.validate()
            .expect("ΔP exactly atmospheric is the inclusive boundary");
    }

    #[test]
    fn legacy_toml_without_vacuum_pressure_defaults_to_none() {
        let p: PrinterProfile = toml::from_str(&valid_printer_toml_with_envelope())
            .expect("legacy TOML without vacuum_pressure_kpa must parse");
        assert_eq!(p.vacuum_pressure_kpa(), None);
        assert!((p.effective_vacuum_pressure_kpa() - DEFAULT_VACUUM_PRESSURE_KPA).abs() < 1e-6);
        p.validate()
            .expect("None vacuum_pressure_kpa must satisfy validate()");
    }

    #[test]
    fn toml_with_explicit_vacuum_pressure_round_trips() {
        let toml_str = valid_printer_toml()
            + "vacuum_pressure_kpa = 75.0\n"
            + "[build_envelope_mm]\nwidth_mm = 192.0\ndepth_mm = 120.0\nmax_z_mm = 200.0\n";
        let p: PrinterProfile =
            toml::from_str(&toml_str).expect("explicit vacuum_pressure_kpa must parse");
        assert_eq!(p.vacuum_pressure_kpa(), Some(75.0));
        p.validate().expect("explicit 75 kPa must satisfy validate()");
    }
}
