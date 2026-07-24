use serde::{Deserialize, Serialize};

/// Type of predicted print failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureType {
    /// Peel force exceeds support capacity.
    SupportOverload,
    /// Cure depth insufficient for layer height.
    InsufficientCure,
    /// Z deflection exceeds commanded layer height.
    ZAxisCatastrophic,
    /// Vat temperature exceeds degradation threshold.
    ThermalDegradation,
    /// Rapid cross-section area increase.
    RapidAreaIncrease,
    /// Sealed cavity creating suction cup effect against FEP.
    SuctionCup,
    /// LCD non-uniformity causes insufficient cure at plate edges.
    NonUniformCure,
    /// High accumulated tensile von-Mises stress at part boundaries —
    /// the cured part is expected to warp on release. ADR-0018 / t2f3.
    /// Producer: `FailurePredictor::predict_strain_failures` (only
    /// active under `#[cfg(feature = "field-sim")]`).
    WarpingRisk,
    /// High strain gradient `|∇ε|` between adjacent voxels — micro-
    /// crack risk along the gradient direction (cohesive failure).
    /// ADR-0018 / t2f3. Producer: `predict_strain_failures`.
    CohesiveFailure,
    /// The resin↔resin interlayer bond, reduced by the Kendall crack-front
    /// knockdown, can no longer hold this NORMAL layer against the peel load —
    /// the layer is predicted to delaminate from the one below. Emitted at
    /// `Severity::Warning`; co-fires with `SupportOverload` when supports also
    /// cannot make up the shortfall. KB-188 / KB-116. Producer:
    /// `FailurePredictor::predict_layer` (normal layers with a real crack front).
    Delamination,
}

/// Severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

/// A predicted failure at a specific layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureEvent {
    pub layer: u32,
    pub failure_type: FailureType,
    pub severity: Severity,
    pub message: String,
}
