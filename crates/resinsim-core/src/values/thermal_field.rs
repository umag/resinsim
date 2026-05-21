//! 3D voxel-resolved temperature field — `ThermalField` value object.
//!
//! ADR-0020, t2f4. A dense `ndarray::Array3<f32>` of absolute temperatures
//! (°C) indexed by `[ix, iy, iz]`, anchored to the **full vat envelope**
//! (via `PrinterProfile.build_envelope_mm`) — NOT to a printed-part bbox
//! like `CureField` / `StrainField` / `StressField`.
//!
//! # Z axis is spatial mm, not layer count
//!
//! This field intentionally departs from the
//! `voxel-field-z-dimension-is-layer-count.md` pattern that governs the
//! other Tier-2 voxel fields. Temperature is a continuous spatio-temporal
//! state — every voxel updates every solver substep — and the spatial
//! domain has to span the LCD/FEP bottom + the resin top + the four vat
//! walls so the boundary conditions make physical sense. See
//! `docs/patterns/thermal-field-z-dim-is-spatial.md` for the contrasting
//! pattern.
//!
//! # Coordinates
//!
//! - `voxel_size_mm`: physical edge length of one voxel, identical on all
//!   three axes (homogeneous in v1; "anisotropic" framing remains in
//!   ADR-0020 for the future case where X-Y vs Z spacing diverges).
//! - `bbox_min_mm`: world-space corner `(x_min, y_min, z_min)` of voxel
//!   `[0, 0, 0]`. Voxel CENTRES sit at
//!   `bbox_min_mm + (ix + 0.5, iy + 0.5, iz + 0.5) * voxel_size_mm`.
//! - `temperature_at_world(x, y, z)` performs trilinear interpolation
//!   inside the envelope and returns `Err(OutOfEnvelope)` outside.
//!   Out-of-envelope coords are NOT clamped (per
//!   `docs/patterns/anti/clamp-onto-boundary-convolution.md`).
//!
//! # NaN policy
//!
//! Two-layer defence (`docs/patterns/nan-two-layer-defence.md`):
//! constructor checks `is_finite` AND value-range on `initial_c`. The
//! solver step (`ThermalDiffusionSolver::step`) is responsible for the
//! postcondition that no NaN exits a step (debug_assert! sweep in debug;
//! `volume_max_c().is_finite()` check in release with typed error).
//!
//! # Memory
//!
//! Dense f32. For the Mars 5 Ultra envelope (153×78×165 mm) at 0.5 mm
//! voxels: `306 × 156 × 330 ≈ 16 M voxels × 4 B = 64 MB`. Within
//! `DEFAULT_MAX_FIELD_ALLOCATION_BYTES = 4 GB`. The `total_bytes()` helper
//! exposes the cost so callers can pre-flight memory budgets before
//! allocation. The budget guard fires from the constructor.

#![cfg(feature = "field-sim")]

use ndarray::{Array3, ArrayView3, ArrayViewMut3};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::values::field_budget::{enforce_field_budget, FieldAllocationError};

/// Absolute-zero floor in °C — temperatures at or below this are unphysical.
const ABSOLUTE_ZERO_C: f32 = -273.15;

/// Errors from `ThermalField` construction and access.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ThermalFieldError {
    #[error("ThermalField dimensions must all be positive, got {nx}×{ny}×{nz}")]
    InvalidDimensions { nx: u32, ny: u32, nz: u32 },
    #[error("ThermalField voxel_size_mm must be finite and > 0, got {0}")]
    InvalidVoxelSize(f32),
    #[error("ThermalField bbox_min_mm must be finite on all axes, got ({x}, {y}, {z})")]
    InvalidBboxMin { x: f32, y: f32, z: f32 },
    /// Constructor rejects NaN/infinite/below-absolute-zero `initial_c`.
    /// Two-layer-defence finite check at the trust boundary.
    #[error("ThermalField initial_c must be finite and above absolute zero (-273.15 °C), got {0}")]
    InvalidInitialTemp(f32),
    #[error("ThermalField index ({ix}, {iy}, {iz}) out of bounds for {nx}×{ny}×{nz}")]
    OutOfBounds {
        ix: u32,
        iy: u32,
        iz: u32,
        nx: u32,
        ny: u32,
        nz: u32,
    },
    /// `temperature_at_world` returns this when the queried world-coord
    /// falls outside the vat envelope. Explicit error (NOT clamping) per
    /// `clamp-onto-boundary-convolution` anti-pattern from t2f2.
    #[error(
        "ThermalField world coord ({x}, {y}, {z}) mm is outside the envelope \
         [{x_min}, {x_max}] × [{y_min}, {y_max}] × [{z_min}, {z_max}]"
    )]
    OutOfEnvelope {
        x: f32,
        y: f32,
        z: f32,
        x_min: f32,
        x_max: f32,
        y_min: f32,
        y_max: f32,
        z_min: f32,
        z_max: f32,
    },
    /// Budget guard: requested allocation exceeds the configured per-field
    /// budget. See `crate::values::field_budget`.
    #[error("ThermalField allocation exceeds budget: {0}")]
    ExceedsBudget(FieldAllocationError),
}

/// Dense 3D voxel field of absolute temperature (°C).
///
/// Dimensions, voxel size, and bbox anchor are fixed at construction. The
/// initial state is uniform `initial_c` (typically ambient at print start).
/// The solver mutates the field in-place via [`Self::as_array_mut`]. Pure
/// readers use [`Self::temperature_at`] (voxel-index) or
/// [`Self::temperature_at_world`] (world-coord, trilinear).
///
/// # Serialization
///
/// Serde derives via `ndarray`'s `serde` feature. Persistence is handled by
/// the RSFIELD sidecar (ADR-0019); the in-memory `Serialize` derive is
/// retained for compatibility with the sibling voxel fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalField {
    nx: u32,
    ny: u32,
    nz: u32,
    voxel_size_mm: f32,
    bbox_min_mm: [f32; 3],
    /// Per-voxel temperature in °C, indexed `[ix, iy, iz]`. Solver step's
    /// postcondition enforces finiteness (see `thermal_diffusion_solver.rs`).
    data: Array3<f32>,
}

impl ThermalField {
    /// Construct a new uniform `ThermalField` at `initial_c`.
    ///
    /// # Errors
    /// - `InvalidDimensions` if any axis is 0.
    /// - `InvalidVoxelSize` if `voxel_size_mm` is not finite > 0.
    /// - `InvalidBboxMin` if any axis of `bbox_min_mm` is not finite.
    /// - `InvalidInitialTemp` if `initial_c` is NaN/infinite/at or below
    ///   absolute zero (-273.15 °C).
    /// - `ExceedsBudget` if the requested allocation exceeds the per-field
    ///   budget (see `crate::values::field_budget`).
    pub fn new(
        nx: u32,
        ny: u32,
        nz: u32,
        voxel_size_mm: f32,
        bbox_min_mm: [f32; 3],
        initial_c: f32,
    ) -> Result<Self, ThermalFieldError> {
        if nx == 0 || ny == 0 || nz == 0 {
            return Err(ThermalFieldError::InvalidDimensions { nx, ny, nz });
        }
        if !voxel_size_mm.is_finite() || voxel_size_mm <= 0.0 {
            return Err(ThermalFieldError::InvalidVoxelSize(voxel_size_mm));
        }
        if !bbox_min_mm[0].is_finite()
            || !bbox_min_mm[1].is_finite()
            || !bbox_min_mm[2].is_finite()
        {
            return Err(ThermalFieldError::InvalidBboxMin {
                x: bbox_min_mm[0],
                y: bbox_min_mm[1],
                z: bbox_min_mm[2],
            });
        }
        if !initial_c.is_finite() || initial_c <= ABSOLUTE_ZERO_C {
            return Err(ThermalFieldError::InvalidInitialTemp(initial_c));
        }
        // Budget guard — fail BEFORE Array3::from_elem allocates.
        enforce_field_budget(
            "ThermalField",
            nx,
            ny,
            nz,
            std::mem::size_of::<f32>() as u64,
            voxel_size_mm,
        )
        .map_err(ThermalFieldError::ExceedsBudget)?;
        let data = Array3::<f32>::from_elem((nx as usize, ny as usize, nz as usize), initial_c);
        Ok(Self {
            nx,
            ny,
            nz,
            voxel_size_mm,
            bbox_min_mm,
            data,
        })
    }

    /// Voxel-grid dimensions `(nx, ny, nz)`.
    pub fn dimensions(&self) -> (u32, u32, u32) {
        (self.nx, self.ny, self.nz)
    }

    /// Edge length of one voxel in millimetres (homogeneous on all axes).
    pub fn voxel_size_mm(&self) -> f32 {
        self.voxel_size_mm
    }

    /// World-space corner `(x_min, y_min, z_min)` of voxel `[0, 0, 0]`.
    pub fn bbox_min_mm(&self) -> [f32; 3] {
        self.bbox_min_mm
    }

    /// Total voxel count `nx × ny × nz`.
    pub fn voxel_count(&self) -> u64 {
        u64::from(self.nx) * u64::from(self.ny) * u64::from(self.nz)
    }

    /// Total allocated bytes for the dense f32 storage.
    pub fn total_bytes(&self) -> u64 {
        self.voxel_count() * std::mem::size_of::<f32>() as u64
    }

    /// Borrow the raw dense storage immutably.
    pub fn as_array_view(&self) -> ArrayView3<'_, f32> {
        self.data.view()
    }

    /// Borrow the raw dense storage mutably (solver entry point).
    /// Postcondition: the solver step is responsible for emitting only
    /// finite values back into the field; the consumer guard runs in
    /// `ThermalDiffusionSolver::step`.
    pub fn as_array_mut(&mut self) -> ArrayViewMut3<'_, f32> {
        self.data.view_mut()
    }

    /// Read temperature at voxel `(ix, iy, iz)`.
    ///
    /// # Errors
    /// `OutOfBounds` if any index exceeds the field dimensions.
    pub fn temperature_at(&self, ix: u32, iy: u32, iz: u32) -> Result<f32, ThermalFieldError> {
        if ix >= self.nx || iy >= self.ny || iz >= self.nz {
            return Err(ThermalFieldError::OutOfBounds {
                ix,
                iy,
                iz,
                nx: self.nx,
                ny: self.ny,
                nz: self.nz,
            });
        }
        Ok(self.data[(ix as usize, iy as usize, iz as usize)])
    }

    /// Read temperature at world-coord `(x_mm, y_mm, z_mm)` via trilinear
    /// interpolation inside the envelope.
    ///
    /// # Coordinate conventions
    ///
    /// Voxel CENTRES sit at `bbox_min_mm + (i + 0.5) · voxel_size_mm`.
    /// Trilinear interpolation uses the 8 surrounding voxel centres; when
    /// the world coord coincides with a voxel CENTRE, the result equals
    /// `temperature_at` for that voxel byte-identically. When the query
    /// coord lies between voxel centres but inside the envelope (i.e.
    /// `[bbox_min, bbox_max]`), 8-neighbour trilinear weights apply.
    ///
    /// # Boundary policy
    ///
    /// Out-of-envelope coords (outside `[bbox_min, bbox_max]`) return
    /// `Err(OutOfEnvelope)` — NOT a clamped value (per the
    /// `clamp-onto-boundary-convolution` anti-pattern). Coords at the
    /// extreme face of the envelope (where one of the trilinear
    /// neighbour indices would exceed `n - 1`) fall back to the nearest
    /// voxel value rather than fabricating a phantom neighbour.
    pub fn temperature_at_world(
        &self,
        x_mm: f32,
        y_mm: f32,
        z_mm: f32,
    ) -> Result<f32, ThermalFieldError> {
        let x_min = self.bbox_min_mm[0];
        let y_min = self.bbox_min_mm[1];
        let z_min = self.bbox_min_mm[2];
        let x_max = x_min + self.nx as f32 * self.voxel_size_mm;
        let y_max = y_min + self.ny as f32 * self.voxel_size_mm;
        let z_max = z_min + self.nz as f32 * self.voxel_size_mm;
        if !x_mm.is_finite() || !y_mm.is_finite() || !z_mm.is_finite()
            || x_mm < x_min
            || x_mm > x_max
            || y_mm < y_min
            || y_mm > y_max
            || z_mm < z_min
            || z_mm > z_max
        {
            return Err(ThermalFieldError::OutOfEnvelope {
                x: x_mm,
                y: y_mm,
                z: z_mm,
                x_min,
                x_max,
                y_min,
                y_max,
                z_min,
                z_max,
            });
        }
        // Voxel-centre-aligned indexing. Subtract 0.5 because voxel
        // centres sit at +0.5 inside their integer index. Clamp the
        // trilinear neighbour indices so the extreme-face case falls
        // back to nearest rather than fabricating a phantom voxel.
        let fx = (x_mm - x_min) / self.voxel_size_mm - 0.5;
        let fy = (y_mm - y_min) / self.voxel_size_mm - 0.5;
        let fz = (z_mm - z_min) / self.voxel_size_mm - 0.5;
        let nx_max = self.nx.saturating_sub(1);
        let ny_max = self.ny.saturating_sub(1);
        let nz_max = self.nz.saturating_sub(1);
        let i0 = fx.floor().clamp(0.0, nx_max as f32) as u32;
        let j0 = fy.floor().clamp(0.0, ny_max as f32) as u32;
        let k0 = fz.floor().clamp(0.0, nz_max as f32) as u32;
        let i1 = (i0 + 1).min(nx_max);
        let j1 = (j0 + 1).min(ny_max);
        let k1 = (k0 + 1).min(nz_max);
        let tx = (fx - i0 as f32).clamp(0.0, 1.0);
        let ty = (fy - j0 as f32).clamp(0.0, 1.0);
        let tz = (fz - k0 as f32).clamp(0.0, 1.0);
        let v = |i: u32, j: u32, k: u32| self.data[(i as usize, j as usize, k as usize)];
        let c000 = v(i0, j0, k0);
        let c100 = v(i1, j0, k0);
        let c010 = v(i0, j1, k0);
        let c110 = v(i1, j1, k0);
        let c001 = v(i0, j0, k1);
        let c101 = v(i1, j0, k1);
        let c011 = v(i0, j1, k1);
        let c111 = v(i1, j1, k1);
        let c00 = c000 * (1.0 - tx) + c100 * tx;
        let c10 = c010 * (1.0 - tx) + c110 * tx;
        let c01 = c001 * (1.0 - tx) + c101 * tx;
        let c11 = c011 * (1.0 - tx) + c111 * tx;
        let c0 = c00 * (1.0 - ty) + c10 * ty;
        let c1 = c01 * (1.0 - ty) + c11 * ty;
        Ok(c0 * (1.0 - tz) + c1 * tz)
    }

    /// Volume-mean temperature (°C). Pure reduction; no side effects.
    /// Used by the SimulationRunner run-end summary log line.
    pub fn volume_mean_c(&self) -> f32 {
        let sum: f64 = self.data.iter().map(|&v| v as f64).sum();
        (sum / self.voxel_count() as f64) as f32
    }

    /// Volume-max temperature (°C). Pure reduction.
    /// Used by the solver-step postcondition's `is_finite` check and the
    /// run-end summary line.
    pub fn volume_max_c(&self) -> f32 {
        self.data.iter().copied().fold(f32::NEG_INFINITY, f32::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_field() -> ThermalField {
        ThermalField::new(4, 3, 2, 0.5, [0.0, 0.0, 0.0], 25.0)
            .expect("test fixture: 4×3×2 ThermalField at 25 °C is in-domain")
    }

    // --- construction ---

    #[test]
    fn construction_with_valid_inputs_succeeds() {
        let f = small_field();
        assert_eq!(f.dimensions(), (4, 3, 2));
        assert_eq!(f.voxel_size_mm(), 0.5);
        assert_eq!(f.bbox_min_mm(), [0.0, 0.0, 0.0]);
        assert_eq!(f.voxel_count(), 24);
        assert_eq!(f.total_bytes(), 24 * 4);
    }

    #[test]
    fn zero_dimension_rejected() {
        let err = ThermalField::new(0, 3, 2, 0.5, [0.0, 0.0, 0.0], 25.0)
            .expect_err("zero nx must reject");
        match err {
            ThermalFieldError::InvalidDimensions { nx, ny, nz } => {
                assert_eq!((nx, ny, nz), (0, 3, 2));
            }
            other => panic!("expected InvalidDimensions, got {other:?}"),
        }
    }

    #[test]
    fn non_finite_voxel_size_rejected() {
        for bad in [f32::NAN, f32::INFINITY, -0.5, 0.0] {
            assert!(matches!(
                ThermalField::new(2, 2, 2, bad, [0.0, 0.0, 0.0], 25.0),
                Err(ThermalFieldError::InvalidVoxelSize(_))
            ));
        }
    }

    #[test]
    fn non_finite_bbox_min_rejected() {
        for axis in 0..3 {
            let mut bbox = [0.0, 0.0, 0.0];
            bbox[axis] = f32::NAN;
            assert!(matches!(
                ThermalField::new(2, 2, 2, 0.5, bbox, 25.0),
                Err(ThermalFieldError::InvalidBboxMin { .. })
            ));
        }
    }

    #[test]
    fn nan_initial_temp_rejected() {
        assert!(matches!(
            ThermalField::new(2, 2, 2, 0.5, [0.0, 0.0, 0.0], f32::NAN),
            Err(ThermalFieldError::InvalidInitialTemp(_))
        ));
    }

    #[test]
    fn below_absolute_zero_initial_temp_rejected() {
        for bad in [-273.15, -300.0, f32::NEG_INFINITY] {
            assert!(matches!(
                ThermalField::new(2, 2, 2, 0.5, [0.0, 0.0, 0.0], bad),
                Err(ThermalFieldError::InvalidInitialTemp(_))
            ));
        }
    }

    // --- temperature_at ---

    #[test]
    fn temperature_at_returns_initial_uniform_value() {
        let f = small_field();
        for ix in 0..4 {
            for iy in 0..3 {
                for iz in 0..2 {
                    assert_eq!(
                        f.temperature_at(ix, iy, iz)
                            .expect("in-bounds index must read"),
                        25.0
                    );
                }
            }
        }
    }

    #[test]
    fn temperature_at_out_of_bounds_returns_err() {
        let f = small_field();
        assert!(matches!(
            f.temperature_at(4, 0, 0),
            Err(ThermalFieldError::OutOfBounds { .. })
        ));
        assert!(matches!(
            f.temperature_at(0, 3, 0),
            Err(ThermalFieldError::OutOfBounds { .. })
        ));
        assert!(matches!(
            f.temperature_at(0, 0, 2),
            Err(ThermalFieldError::OutOfBounds { .. })
        ));
    }

    // --- as_array_mut writability ---

    #[test]
    fn as_array_mut_writes_visible_via_temperature_at() {
        let mut f = small_field();
        f.as_array_mut()[(1, 2, 0)] = 42.5;
        assert_eq!(
            f.temperature_at(1, 2, 0).expect("in-bounds"),
            42.5
        );
    }

    // --- volume reductions ---

    #[test]
    fn volume_mean_uniform_field_equals_initial() {
        let f = small_field();
        assert!((f.volume_mean_c() - 25.0).abs() < 1e-6);
    }

    #[test]
    fn volume_max_uniform_field_equals_initial() {
        let f = small_field();
        assert_eq!(f.volume_max_c(), 25.0);
    }

    #[test]
    fn volume_max_after_hot_spot_write() {
        let mut f = small_field();
        f.as_array_mut()[(0, 0, 0)] = 100.0;
        assert_eq!(f.volume_max_c(), 100.0);
        // mean = (23 * 25 + 1 * 100) / 24 = (575 + 100) / 24
        let expected_mean = (23.0 * 25.0 + 100.0) / 24.0;
        assert!((f.volume_mean_c() - expected_mean).abs() < 1e-4);
    }

    // --- temperature_at_world translation invariant ---

    #[test]
    fn world_coord_at_voxel_center_equals_temperature_at() {
        // Set distinct values on every voxel via as_array_mut.
        let mut f = ThermalField::new(3, 3, 3, 0.5, [10.0, 20.0, 30.0], 25.0)
            .expect("3×3×3 ThermalField in-domain");
        for ix in 0..3u32 {
            for iy in 0..3u32 {
                for iz in 0..3u32 {
                    f.as_array_mut()[(ix as usize, iy as usize, iz as usize)] =
                        ix as f32 * 100.0 + iy as f32 * 10.0 + iz as f32;
                }
            }
        }
        // Voxel centre for (ix, iy, iz) sits at bbox_min + (idx + 0.5) * voxel_size.
        for ix in 0..3u32 {
            for iy in 0..3u32 {
                for iz in 0..3u32 {
                    let x = 10.0 + (ix as f32 + 0.5) * 0.5;
                    let y = 20.0 + (iy as f32 + 0.5) * 0.5;
                    let z = 30.0 + (iz as f32 + 0.5) * 0.5;
                    let world_t = f
                        .temperature_at_world(x, y, z)
                        .expect("voxel-centre coord in-envelope");
                    let voxel_t = f
                        .temperature_at(ix, iy, iz)
                        .expect("in-bounds voxel index");
                    assert!(
                        (world_t - voxel_t).abs() < 1e-4,
                        "voxel ({ix}, {iy}, {iz}) centre world-coord ({x}, {y}, {z}) \
                         must equal temperature_at: world={world_t}, voxel={voxel_t}"
                    );
                }
            }
        }
    }

    #[test]
    fn world_coord_sub_voxel_interpolates_linearly() {
        // 2x1x1 field with values 0.0 and 10.0 — midpoint between centres
        // should interpolate to 5.0.
        let mut f = ThermalField::new(2, 1, 1, 1.0, [0.0, 0.0, 0.0], 25.0)
            .expect("2×1×1 ThermalField in-domain");
        f.as_array_mut()[(0, 0, 0)] = 0.0;
        f.as_array_mut()[(1, 0, 0)] = 10.0;
        // Centre of voxel 0: x = 0.5. Centre of voxel 1: x = 1.5.
        // Midpoint between centres: x = 1.0. Trilinear should give 5.0.
        let mid = f
            .temperature_at_world(1.0, 0.5, 0.5)
            .expect("in-envelope midpoint");
        assert!((mid - 5.0).abs() < 1e-4, "midpoint interpolation got {mid}");
    }

    #[test]
    fn world_coord_outside_envelope_returns_err() {
        let f = small_field();
        // Field spans (0..2, 0..1.5, 0..1) mm.
        for (x, y, z) in [
            (-0.01, 0.5, 0.5),
            (2.01, 0.5, 0.5),
            (0.5, -0.01, 0.5),
            (0.5, 1.51, 0.5),
            (0.5, 0.5, -0.01),
            (0.5, 0.5, 1.01),
            (f32::NAN, 0.5, 0.5),
        ] {
            assert!(
                matches!(
                    f.temperature_at_world(x, y, z),
                    Err(ThermalFieldError::OutOfEnvelope { .. })
                ),
                "out-of-envelope ({x}, {y}, {z}) must err"
            );
        }
    }

    #[test]
    fn world_coord_at_extreme_face_returns_nearest_voxel_value() {
        // At bbox_min (0,0,0), the nearest voxel is (0,0,0). At
        // bbox_max — also a valid envelope coordinate — should land
        // on the nearest face voxel without panicking on the
        // out-of-range neighbour index.
        let mut f = ThermalField::new(2, 2, 2, 1.0, [0.0, 0.0, 0.0], 25.0)
            .expect("2×2×2 ThermalField in-domain");
        f.as_array_mut()[(0, 0, 0)] = 1.0;
        f.as_array_mut()[(1, 1, 1)] = 100.0;
        let at_min = f
            .temperature_at_world(0.0, 0.0, 0.0)
            .expect("bbox_min is in-envelope");
        assert!((at_min - 1.0).abs() < 1e-4, "bbox_min → nearest voxel: got {at_min}");
        let at_max = f
            .temperature_at_world(2.0, 2.0, 2.0)
            .expect("bbox_max is in-envelope");
        assert!(
            (at_max - 100.0).abs() < 1e-4,
            "bbox_max → nearest far-voxel: got {at_max}"
        );
    }

    // --- budget guard ---

    #[test]
    fn budget_guard_rejects_oversized_allocation() {
        // The field_budget default is 4 GB. 2 GB voxels × 4 B = 8 GB.
        // u32 dims won't allow 2 G directly, but 1290×1290×1290 ≈ 2.1 G
        // voxels × 4 B = 8.6 GB exceeds budget.
        let err = ThermalField::new(1290, 1290, 1290, 0.5, [0.0, 0.0, 0.0], 25.0)
            .expect_err("over-budget allocation must reject");
        assert!(matches!(err, ThermalFieldError::ExceedsBudget(_)));
    }
}
