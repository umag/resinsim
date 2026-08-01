//! 2D voxel-field slice + summary statistics — `FieldSlice` / `FieldStats`
//! value objects. t2f6-field-inspector.
//!
//! `resinsim inspect field` walks one of the five Tier-2 voxel fields
//! (cure, photoinitiator, strain, stress, thermal) and extracts a single
//! 2D cross-section for display. `FieldSlicer` (`crate::services::
//! field_slicer`) is the domain service that produces a `FieldSlice`;
//! this module holds the pure data shape plus the statistics reduction.
//!
//! # Row-major layout
//!
//! `FieldSlice::values()` is row-major over `(nu, nv)`: flat index
//! `v * nu + u`, where `u`/`v` are the plane's two free axes in the
//! order fixed by [`SlicePlane`] (`Xy` → u=X, v=Y; `Xz` → u=X, v=Z;
//! `Yz` → u=Y, v=Z).
//!
//! # NaN policy
//!
//! Two-layer defence (`docs/patterns/nan-two-layer-defence.md`):
//! `FieldSlice::new` and `FieldStats::compute` both reject non-finite
//! input independently — `FieldStats::compute` is also unit-tested
//! directly against hand-built arrays that never pass through
//! `FieldSlice::new`, so it needs its own guard at the same trust
//! boundary.
//!
//! # Percentiles
//!
//! `p95`/`p99` use the nearest-rank method over a sorted copy:
//! `rank = ceil(p/100 * n)`, `index = rank - 1` (clamped into
//! `[0, n-1]`). This is a *total* function only for non-empty input —
//! an empty scope (e.g. `--cured-only` over an all-zero slice) returns
//! `FieldSliceError::EmptyScope` rather than NaN
//! (`docs/patterns/anti/magic-floor-vs-honest-filter.md`).
//!
//! # Dual-scope statistics
//!
//! `FieldStatsScope::All` computes over every voxel in the slice
//! (zeros included); `FieldStatsScope::Nonzero` (the `--cured-only`
//! mode) computes over only the nonzero voxels. `FieldStats` is ONE
//! type carrying a `scope` marker — never two structs — and both
//! `count` (the scoped population size) and `nonzero_count` (always
//! the raw nonzero cardinality, regardless of scope) are present on
//! every instance, so a JSON consumer never has to infer which count
//! means what from the scope alone.

#![cfg(feature = "field-sim")]

use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;

/// Which axis is held fixed to produce a 2D slice through a 3D voxel
/// field, and — inverted — which plane results from holding a given
/// axis fixed. `Xy` fixes Z (the common "layer view"); `Xz` fixes Y;
/// `Yz` fixes X.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlicePlane {
    Xy,
    Xz,
    Yz,
}

impl SlicePlane {
    /// Label for the plane's first (fastest-varying, "u") free axis.
    pub fn u_axis_label(&self) -> &'static str {
        match self {
            Self::Xy => "X",
            Self::Xz => "X",
            Self::Yz => "Y",
        }
    }

    /// Label for the plane's second ("v") free axis.
    pub fn v_axis_label(&self) -> &'static str {
        match self {
            Self::Xy => "Y",
            Self::Xz => "Z",
            Self::Yz => "Z",
        }
    }

    /// The axis this plane holds fixed (the slice's "depth" axis).
    pub fn fixed_axis(&self) -> SliceAxis {
        match self {
            Self::Xy => SliceAxis::Z,
            Self::Xz => SliceAxis::Y,
            Self::Yz => SliceAxis::X,
        }
    }
}

/// A single voxel-grid axis. Used by `FieldSlicer::resolve_index` (the
/// `--slice <AXIS>=<VALUE>mm` addressing input) and by [`SlicePlane`]
/// (the axis a plane holds fixed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SliceAxis {
    X,
    Y,
    Z,
}

impl SliceAxis {
    /// Short display label — also what `{axis:?}` yields via `Debug`,
    /// kept as an explicit method so callers don't rely on Debug
    /// formatting for user-facing text.
    pub fn label(&self) -> &'static str {
        match self {
            Self::X => "X",
            Self::Y => "Y",
            Self::Z => "Z",
        }
    }

    /// The plane produced when this axis is held fixed. Inverse of
    /// [`SlicePlane::fixed_axis`].
    pub fn plane(&self) -> SlicePlane {
        match self {
            Self::X => SlicePlane::Yz,
            Self::Y => SlicePlane::Xz,
            Self::Z => SlicePlane::Xy,
        }
    }
}

/// Errors from `FieldSlice` / `FieldStats` construction.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum FieldSliceError {
    #[error(
        "FieldSlice dimension mismatch: nu={nu} × nv={nv} = {expected} values expected, got {actual}"
    )]
    DimensionMismatch {
        nu: u32,
        nv: u32,
        expected: u64,
        actual: usize,
    },
    #[error("FieldSlice value at flat index {index} is not finite, got {value}")]
    NonFiniteValue { index: usize, value: f32 },
    #[error(
        "FieldStats: no voxels in scope {scope:?} — cannot compute statistics over zero elements"
    )]
    EmptyScope { scope: FieldStatsScope },
}

/// Which voxel population a [`FieldStats`] was computed over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldStatsScope {
    /// Every voxel in the slice, zeros included.
    All,
    /// Only voxels with a nonzero value (`--cured-only`).
    Nonzero,
}

/// Summary statistics over a [`FieldSlice`]'s values (or any raw
/// `&[f32]` — see [`Self::compute`]). ONE type with a `scope` marker;
/// `--cured-only` selects `FieldStatsScope::Nonzero` rather than
/// forking a second struct.
///
/// `count` is the population size for `scope` (all voxels for `All`,
/// nonzero voxels for `Nonzero`). `nonzero_count` is ALWAYS the raw
/// nonzero cardinality of the underlying data, regardless of `scope` —
/// both counts are always present so a consumer never has to infer one
/// from the other.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FieldStats {
    pub scope: FieldStatsScope,
    pub count: u64,
    pub nonzero_count: u64,
    pub min: f32,
    pub max: f32,
    pub mean: f32,
    pub p95: f32,
    pub p99: f32,
}

impl FieldStats {
    /// Compute statistics over `values` for the given `scope`.
    ///
    /// Rejects non-finite entries (two-layer defence — this is a second,
    /// independent guard from `FieldSlice::new`'s, since this function is
    /// also called directly against hand-built test arrays). Returns
    /// `Err(EmptyScope)` when the scoped population is empty — e.g. an
    /// all-zero slice under `FieldStatsScope::Nonzero` — rather than
    /// producing NaN.
    pub fn compute(values: &[f32], scope: FieldStatsScope) -> Result<Self, FieldSliceError> {
        for (index, value) in values.iter().enumerate() {
            if !value.is_finite() {
                return Err(FieldSliceError::NonFiniteValue {
                    index,
                    value: *value,
                });
            }
        }
        let nonzero_count = values.iter().filter(|v| **v != 0.0).count() as u64;
        let mut population: Vec<f32> = match scope {
            FieldStatsScope::All => values.to_vec(),
            FieldStatsScope::Nonzero => values.iter().copied().filter(|v| *v != 0.0).collect(),
        };
        if population.is_empty() {
            return Err(FieldSliceError::EmptyScope { scope });
        }
        population.sort_by(|a, b| {
            a.partial_cmp(b).expect(
                "finiteness already validated above for every entry in `values`, and `population` is a subset of `values` — total ordering is always defined",
            )
        });
        let count = population.len() as u64;
        let min = population[0];
        let max = population[population.len() - 1];
        let sum: f64 = population.iter().map(|v| f64::from(*v)).sum();
        let mean = (sum / population.len() as f64) as f32;
        let p95 = nearest_rank_percentile(&population, 95.0);
        let p99 = nearest_rank_percentile(&population, 99.0);
        Ok(Self {
            scope,
            count,
            nonzero_count,
            min,
            max,
            mean,
            p95,
            p99,
        })
    }
}

/// Nearest-rank percentile over an ascending-sorted, non-empty slice.
/// `rank = ceil(p/100 * n)`; flat index `rank - 1`, clamped into
/// `[0, n-1]`.
fn nearest_rank_percentile(sorted: &[f32], p: f64) -> f32 {
    let n = sorted.len();
    let rank = (p / 100.0 * n as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(n - 1);
    sorted[idx]
}

/// A 2D cross-section through one of the five Tier-2 voxel fields, plus
/// the metadata needed to render it: which plane, at which fixed
/// index, the free-axis grid shape, the physical unit of the values,
/// and the world-space origin/spacing of the two free axes.
///
/// # `world_origin_mm` / `spacing_mm` caveat for layer-stacked Z
///
/// For the four layer-stacked fields (cure / photoinitiator / strain /
/// stress), the Z axis's real per-layer thickness is generally
/// non-uniform (`docs/patterns/anti/voxel-z-step-from-lateral-voxel-size.md`).
/// When Z is one of the slice's free axes (`Xz` or `Yz` planes),
/// `spacing_mm`'s Z component is the field's *lateral* `voxel_size_mm`
/// used as a nominal display pitch — it is NOT the true per-layer
/// physical spacing. Callers that need exact per-layer Z positions
/// must consult `LayerHeightProvenance` directly; `FieldSlice` does not
/// carry the per-row breakdown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldSlice {
    plane: SlicePlane,
    index: u32,
    nu: u32,
    nv: u32,
    values: Vec<f32>,
    unit_label: String,
    world_origin_mm: [f32; 2],
    spacing_mm: [f32; 2],
}

impl FieldSlice {
    /// Construct a new `FieldSlice`. Validates `nu * nv == values.len()`
    /// and that every value is finite.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        plane: SlicePlane,
        index: u32,
        nu: u32,
        nv: u32,
        values: Vec<f32>,
        unit_label: impl Into<String>,
        world_origin_mm: [f32; 2],
        spacing_mm: [f32; 2],
    ) -> Result<Self, FieldSliceError> {
        let expected = u64::from(nu) * u64::from(nv);
        if expected != values.len() as u64 {
            return Err(FieldSliceError::DimensionMismatch {
                nu,
                nv,
                expected,
                actual: values.len(),
            });
        }
        for (index, value) in values.iter().enumerate() {
            if !value.is_finite() {
                return Err(FieldSliceError::NonFiniteValue {
                    index,
                    value: *value,
                });
            }
        }
        Ok(Self {
            plane,
            index,
            nu,
            nv,
            values,
            unit_label: unit_label.into(),
            world_origin_mm,
            spacing_mm,
        })
    }

    pub fn plane(&self) -> SlicePlane {
        self.plane
    }

    /// The fixed-axis voxel index this slice was taken at.
    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn nu(&self) -> u32 {
        self.nu
    }

    pub fn nv(&self) -> u32 {
        self.nv
    }

    /// Row-major values: flat index `v * nu + u`.
    pub fn values(&self) -> &[f32] {
        &self.values
    }

    pub fn unit_label(&self) -> &str {
        &self.unit_label
    }

    pub fn world_origin_mm(&self) -> [f32; 2] {
        self.world_origin_mm
    }

    pub fn spacing_mm(&self) -> [f32; 2] {
        self.spacing_mm
    }

    pub fn u_axis_label(&self) -> &'static str {
        self.plane.u_axis_label()
    }

    pub fn v_axis_label(&self) -> &'static str {
        self.plane.v_axis_label()
    }

    /// Value at free-axis coordinate `(u, v)`. `None` if either
    /// coordinate is out of range.
    pub fn value_at(&self, u: u32, v: u32) -> Option<f32> {
        if u >= self.nu || v >= self.nv {
            return None;
        }
        self.values.get((v * self.nu + u) as usize).copied()
    }

    /// Compute statistics over this slice's values for `scope`.
    /// Delegates to [`FieldStats::compute`].
    pub fn stats(&self, scope: FieldStatsScope) -> Result<FieldStats, FieldSliceError> {
        FieldStats::compute(&self.values, scope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_MSG: &str =
        "test fixture: literal inputs satisfy FieldSlice/FieldStats constructor preconditions";

    // ---- SlicePlane / SliceAxis mapping ----

    #[test]
    fn slice_plane_fixed_axis_and_axis_labels() {
        assert_eq!(SlicePlane::Xy.fixed_axis(), SliceAxis::Z);
        assert_eq!(SlicePlane::Xy.u_axis_label(), "X");
        assert_eq!(SlicePlane::Xy.v_axis_label(), "Y");

        assert_eq!(SlicePlane::Xz.fixed_axis(), SliceAxis::Y);
        assert_eq!(SlicePlane::Xz.u_axis_label(), "X");
        assert_eq!(SlicePlane::Xz.v_axis_label(), "Z");

        assert_eq!(SlicePlane::Yz.fixed_axis(), SliceAxis::X);
        assert_eq!(SlicePlane::Yz.u_axis_label(), "Y");
        assert_eq!(SlicePlane::Yz.v_axis_label(), "Z");
    }

    #[test]
    fn slice_axis_plane_is_inverse_of_fixed_axis() {
        for axis in [SliceAxis::X, SliceAxis::Y, SliceAxis::Z] {
            assert_eq!(axis.plane().fixed_axis(), axis);
        }
    }

    // ---- FieldSlice::new validation ----

    #[test]
    fn new_rejects_dimension_mismatch() {
        let err = FieldSlice::new(SlicePlane::Xy, 0, 2, 2, vec![1.0, 2.0, 3.0], "mJ/cm²", [0.0; 2], [0.5; 2])
            .expect_err("test fixture: 2×2=4 declared but only 3 values supplied, so Err is the expected outcome");
        assert!(
            matches!(
                err,
                FieldSliceError::DimensionMismatch {
                    nu: 2,
                    nv: 2,
                    expected: 4,
                    actual: 3
                }
            ),
            "expected DimensionMismatch {{nu:2,nv:2,expected:4,actual:3}}, got {err:?}"
        );
    }

    #[test]
    fn new_rejects_nan_value() {
        let err = FieldSlice::new(
            SlicePlane::Xy,
            0,
            2,
            1,
            vec![1.0, f32::NAN],
            "mJ/cm²",
            [0.0; 2],
            [0.5; 2],
        )
        .expect_err("test fixture: NaN deliberately injected, so Err is the expected outcome");
        assert!(
            matches!(err, FieldSliceError::NonFiniteValue { index: 1, value } if value.is_nan()),
            "expected NonFiniteValue {{index:1, NaN}}, got {err:?}"
        );
    }

    #[test]
    fn new_rejects_infinite_value() {
        let err = FieldSlice::new(
            SlicePlane::Xy,
            0,
            1,
            1,
            vec![f32::INFINITY],
            "mJ/cm²",
            [0.0; 2],
            [0.5; 2],
        )
        .expect_err("test fixture: ±∞ deliberately injected, so Err is the expected outcome");
        assert!(
            matches!(err, FieldSliceError::NonFiniteValue { index: 0, value } if value.is_infinite()),
            "expected NonFiniteValue {{index:0, ±∞}}, got {err:?}"
        );
    }

    #[test]
    fn new_accepts_valid_slice_and_getters_round_trip() {
        let s = FieldSlice::new(
            SlicePlane::Xz,
            3,
            2,
            3,
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            "MPa",
            [10.0, 20.0],
            [0.5, 0.04],
        )
        .expect(FIXTURE_MSG);
        assert_eq!(s.plane(), SlicePlane::Xz);
        assert_eq!(s.index(), 3);
        assert_eq!(s.nu(), 2);
        assert_eq!(s.nv(), 3);
        assert_eq!(s.values(), &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(s.unit_label(), "MPa");
        assert_eq!(s.world_origin_mm(), [10.0, 20.0]);
        assert_eq!(s.spacing_mm(), [0.5, 0.04]);
        assert_eq!(s.u_axis_label(), "X");
        assert_eq!(s.v_axis_label(), "Z");
    }

    #[test]
    fn value_at_reads_row_major() {
        // nu=3, nv=2: row v=0 is [1,2,3], row v=1 is [4,5,6].
        let s = FieldSlice::new(
            SlicePlane::Xy,
            0,
            3,
            2,
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            "",
            [0.0; 2],
            [0.5; 2],
        )
        .expect(FIXTURE_MSG);
        assert_eq!(s.value_at(0, 0), Some(1.0));
        assert_eq!(s.value_at(2, 0), Some(3.0));
        assert_eq!(s.value_at(0, 1), Some(4.0));
        assert_eq!(s.value_at(2, 1), Some(6.0));
        assert_eq!(s.value_at(3, 0), None);
        assert_eq!(s.value_at(0, 2), None);
    }

    #[test]
    fn stats_delegates_to_field_stats_compute() {
        let s = FieldSlice::new(
            SlicePlane::Xy,
            0,
            2,
            2,
            vec![0.0, 5.0, 0.0, 7.0],
            "mJ/cm²",
            [0.0; 2],
            [0.5; 2],
        )
        .expect(FIXTURE_MSG);
        let all = s.stats(FieldStatsScope::All).expect(FIXTURE_MSG);
        assert_eq!(all.count, 4);
        assert_eq!(all.nonzero_count, 2);
        assert_eq!(all.min, 0.0);
        assert_eq!(all.max, 7.0);
        let nz = s.stats(FieldStatsScope::Nonzero).expect(FIXTURE_MSG);
        assert_eq!(nz.count, 2);
        assert_eq!(nz.nonzero_count, 2);
        assert_eq!(nz.min, 5.0);
        assert_eq!(nz.max, 7.0);
    }

    // ---- FieldStats::compute — percentile boundaries (n = 1, 19, 20, 21, 100) ----
    //
    // Fixtures are `1.0..=n as f32` (already sorted ascending), so the
    // nearest-rank formula's expected output is a clean integer,
    // independently hand-computed in the comment above each case.

    #[test]
    fn percentile_boundary_n1() {
        let values = vec![10.0_f32];
        let s = FieldStats::compute(&values, FieldStatsScope::All).expect(FIXTURE_MSG);
        // rank = ceil(0.95*1) = 1 -> idx 0 -> 10.0 (only element) for both p95/p99.
        assert_eq!(s.p95, 10.0);
        assert_eq!(s.p99, 10.0);
        assert_eq!(s.min, 10.0);
        assert_eq!(s.max, 10.0);
        assert_eq!(s.mean, 10.0);
        assert_eq!(s.count, 1);
    }

    #[test]
    fn percentile_boundary_n19() {
        let values: Vec<f32> = (1..=19).map(|v| v as f32).collect();
        let s = FieldStats::compute(&values, FieldStatsScope::All).expect(FIXTURE_MSG);
        // rank_p95 = ceil(0.95*19) = ceil(18.05) = 19 -> idx 18 -> 19.0 (max).
        // rank_p99 = ceil(0.99*19) = ceil(18.81) = 19 -> idx 18 -> 19.0 (max).
        assert_eq!(s.p95, 19.0);
        assert_eq!(s.p99, 19.0);
    }

    #[test]
    fn percentile_boundary_n20() {
        let values: Vec<f32> = (1..=20).map(|v| v as f32).collect();
        let s = FieldStats::compute(&values, FieldStatsScope::All).expect(FIXTURE_MSG);
        // rank_p95 = ceil(0.95*20) = ceil(19.0) = 19 -> idx 18 -> 19.0.
        // rank_p99 = ceil(0.99*20) = ceil(19.8) = 20 -> idx 19 -> 20.0 (max).
        assert_eq!(s.p95, 19.0);
        assert_eq!(s.p99, 20.0);
    }

    #[test]
    fn percentile_boundary_n21() {
        let values: Vec<f32> = (1..=21).map(|v| v as f32).collect();
        let s = FieldStats::compute(&values, FieldStatsScope::All).expect(FIXTURE_MSG);
        // rank_p95 = ceil(0.95*21) = ceil(19.95) = 20 -> idx 19 -> 20.0.
        // rank_p99 = ceil(0.99*21) = ceil(20.79) = 21 -> idx 20 -> 21.0 (max).
        assert_eq!(s.p95, 20.0);
        assert_eq!(s.p99, 21.0);
    }

    #[test]
    fn percentile_boundary_n100() {
        let values: Vec<f32> = (1..=100).map(|v| v as f32).collect();
        let s = FieldStats::compute(&values, FieldStatsScope::All).expect(FIXTURE_MSG);
        // rank_p95 = ceil(95.0) = 95 -> idx 94 -> 95.0.
        // rank_p99 = ceil(99.0) = 99 -> idx 98 -> 99.0.
        assert_eq!(s.p95, 95.0);
        assert_eq!(s.p99, 99.0);
    }

    // ---- FieldStats::compute — all-equal, zeros, scopes ----

    #[test]
    fn all_equal_array_stats_collapse_to_the_shared_value() {
        let values = vec![4.0_f32; 12];
        let s = FieldStats::compute(&values, FieldStatsScope::All).expect(FIXTURE_MSG);
        assert_eq!(s.min, 4.0);
        assert_eq!(s.max, 4.0);
        assert_eq!(s.mean, 4.0);
        assert_eq!(s.p95, 4.0);
        assert_eq!(s.p99, 4.0);
        assert_eq!(s.count, 12);
        // Nonzero — every value equals 4.0, so nonzero_count == count.
        assert_eq!(s.nonzero_count, 12);
    }

    #[test]
    fn many_zeros_nonzero_count_is_correct_under_all_scope() {
        let values = vec![0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 8.0, 0.0, 0.0];
        let s = FieldStats::compute(&values, FieldStatsScope::All).expect(FIXTURE_MSG);
        assert_eq!(s.count, 10);
        assert_eq!(s.nonzero_count, 2);
        // All-scope stats include the zeros.
        assert_eq!(s.min, 0.0);
        assert_eq!(s.max, 8.0);
        assert!((s.mean - 1.1).abs() < 1e-5);
    }

    #[test]
    fn nonzero_scope_excludes_exactly_the_zeros_and_both_counts_present() {
        let values = vec![0.0, 5.0, 0.0, 7.0, 0.0, 0.0];
        let s = FieldStats::compute(&values, FieldStatsScope::Nonzero).expect(FIXTURE_MSG);
        assert_eq!(s.scope, FieldStatsScope::Nonzero);
        assert_eq!(
            s.count, 2,
            "population size under Nonzero scope excludes zeros"
        );
        assert_eq!(
            s.nonzero_count, 2,
            "nonzero_count always present regardless of scope"
        );
        assert_eq!(s.min, 5.0);
        assert_eq!(s.max, 7.0);
        assert_eq!(s.mean, 6.0);
    }

    #[test]
    fn all_zero_slice_under_nonzero_scope_returns_typed_empty_error_never_nan() {
        let values = vec![0.0_f32; 8];
        let err = FieldStats::compute(&values, FieldStatsScope::Nonzero)
            .expect_err("test fixture: all-zero slice deliberately has an empty Nonzero scope, so Err is the expected outcome");
        assert!(
            matches!(
                err,
                FieldSliceError::EmptyScope {
                    scope: FieldStatsScope::Nonzero
                }
            ),
            "expected EmptyScope {{scope: Nonzero}}, got {err:?}"
        );
    }

    #[test]
    fn empty_input_under_all_scope_returns_typed_empty_error() {
        let values: Vec<f32> = vec![];
        let err = FieldStats::compute(&values, FieldStatsScope::All).expect_err(
            "test fixture: empty input deliberately supplied, so Err is the expected outcome",
        );
        assert!(
            matches!(
                err,
                FieldSliceError::EmptyScope {
                    scope: FieldStatsScope::All
                }
            ),
            "expected EmptyScope {{scope: All}}, got {err:?}"
        );
    }

    #[test]
    fn compute_rejects_nan_before_scoping() {
        let values = vec![1.0, f32::NAN, 3.0];
        let err = FieldStats::compute(&values, FieldStatsScope::All)
            .expect_err("test fixture: NaN deliberately injected, so Err is the expected outcome");
        assert!(
            matches!(err, FieldSliceError::NonFiniteValue { index: 1, value } if value.is_nan()),
            "expected NonFiniteValue {{index:1, NaN}}, got {err:?}"
        );
    }
}
