//! Per-pane modules. Pass 1 implements `ForcesPane` end-to-end against
//! real sim data; the other 9 panes render an empty axis placeholder
//! and will be filled in during Pass 3.

pub mod area_delta;
pub mod cure_depth;
pub mod empty_slot;
pub mod forces;
pub mod layer_mask_2d;
pub mod safety;
pub mod vat_temp;
pub mod viscosity;
pub mod z_deflection;

pub use area_delta::AreaDeltaPane;
pub use cure_depth::CureDepthPane;
pub use empty_slot::EmptySlotPane;
pub use forces::ForcesPane;
pub use layer_mask_2d::LayerMask2dPane;
pub use safety::SafetyPane;
pub use vat_temp::VatTempPane;
pub use viscosity::ViscosityPane;
pub use z_deflection::ZDeflectionPane;
