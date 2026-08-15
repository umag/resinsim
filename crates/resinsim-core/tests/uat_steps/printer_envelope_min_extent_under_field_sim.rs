//! spec/uat/printer-envelope-min-extent-under-field-sim.md
//!
//! PRODUCTION-BLOCKED: the `PrinterProfile::validate()` min-extent check
//! this scenario needs does not exist yet in production code. The blocking
//! issue is `printer-envelope-min-extent-validation`. This stub module
//! exists to satisfy the layer 1/3 structural guard (mod.rs pub mod + use
//! list agreement) and pre-wires the field-sim gate so the step definitions
//! can be added when the production code lands.
//!
//! FIELD-SIM-GATED: the validate() check will be `#[cfg(feature =
//! "field-sim")]` (it guards against sub-minimum envelope extents that only
//! matter for the thermal field grid). Under default features this module
//! does not compile; the scenario skips at its first undefined Given in
//! BOTH configs. Register stays `both_configs(1)`.
