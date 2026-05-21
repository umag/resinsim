pub mod area;
pub mod cure_depth;
#[cfg(feature = "field-sim")]
pub mod cure_field;
#[cfg(feature = "field-sim")]
pub mod field_budget;
pub mod float_range;
pub mod force;
pub mod int_range;
pub mod layer_height_provenance;
pub mod layer_height_seq;
pub mod layer_input;
pub mod layer_mask;
pub mod layer_phase;
#[cfg(feature = "field-sim")]
pub mod photoinitiator_field;
#[cfg(feature = "field-sim")]
pub mod strain_field;
#[cfg(feature = "field-sim")]
pub mod strain_tensor;
#[cfg(feature = "field-sim")]
pub mod stress_field;
#[cfg(feature = "field-sim")]
pub mod stress_tensor;
pub mod thermal;
#[cfg(feature = "field-sim")]
pub mod thermal_field;

pub use area::{AreaDelta, CrossSectionArea};
pub use cure_depth::{CureDepth, Energy, PenetrationDepth};
#[cfg(feature = "field-sim")]
pub use cure_field::{CureField, CureFieldError, LayerSummary};
#[cfg(feature = "field-sim")]
pub use field_budget::{
    active_budget_bytes, enforce_field_budget, FieldAllocationError,
    DEFAULT_MAX_FIELD_ALLOCATION_BYTES, FIELD_BUDGET_ENV_VAR,
};
pub use float_range::FloatRange;
pub use force::{PeelForce, SafetyFactor, SupportCapacity};
pub use int_range::IntRange;
pub use layer_height_provenance::{LayerHeightProvenance, MismatchDetail, MismatchKind};
pub use layer_height_seq::LayerHeightSeq;
pub use layer_input::LayerInput;
pub use layer_mask::{LayerGeometry, LayerMask, MaskError, DEFAULT_VOXEL_SIZE_MM};
pub use layer_phase::LayerPhase;
#[cfg(feature = "field-sim")]
pub use photoinitiator_field::{PhotoinitiatorField, PhotoinitiatorFieldError};
#[cfg(feature = "field-sim")]
pub use strain_field::{StrainField, StrainFieldError};
#[cfg(feature = "field-sim")]
pub use strain_tensor::{StrainTensor, StrainTensorError};
#[cfg(feature = "field-sim")]
pub use stress_field::{StressField, StressFieldError};
#[cfg(feature = "field-sim")]
pub use stress_tensor::{StressTensor, StressTensorError};
pub use thermal::{
    AmbientTemperature, ConvectiveCoefficient, Density, InitialLedTemperature,
    ScreenHeatFlux, SpecificHeatCapacity, ThermalConductivity, ThermalTimeConstant,
    VatTemperature, VatWallThickness,
};
#[cfg(feature = "field-sim")]
pub use thermal_field::{ThermalField, ThermalFieldError};
