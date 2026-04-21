//! Per-layer binary occupancy mask — a bit-packed 2D grid at a fixed physical
//! resolution. Anchored to a bbox by convention (position is implicit in the
//! caller's coordinate system; LayerMask only stores dimensions + cells).
//!
//! Used by [`CavityDetector`](crate::services::CavityDetector) to identify
//! topologically-sealed void pockets across layer stacks — the 3D replacement
//! for the area-drop heuristic that previously produced false-positive
//! suction risks at raft→supports transitions.
//!
//! # Area semantics
//!
//! `LayerMask::solid_area_mm2()` computes area from the downsampled grid at
//! `voxel_size_mm` resolution. This is NOT the same as
//! `LayerInput::cross_section_area_mm2`, which is computed from the native
//! pixel resolution of the slicer/CTB parser (typically sub-mm, much finer).
//!
//! Callers that need sub-mm area precision should use
//! `LayerInput::cross_section_area_mm2`. Callers doing topology analysis
//! (CavityDetector) use mask area. The two can differ by ~10% at 0.5 mm
//! voxels for sub-mm features.

use bitvec::vec::BitVec;
use thiserror::Error;

use crate::values::CrossSectionArea;

/// Default voxel size (physical mm per cell) for slicer output.
///
/// 0.5 mm is the v1 default: memory budget for a 4492-layer 153×78 mm print is
/// ~27 MB bit-packed; finer than slicer pixel pitch is wasteful; coarser misses
/// thin walls. Override per-printer via `PrinterProfile::voxel_size_mm` or
/// per-invocation via the CLI `--voxel-size-mm` flag.
pub const DEFAULT_VOXEL_SIZE_MM: f32 = 0.5;

/// Errors from LayerMask construction and access.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum MaskError {
    #[error("LayerMask dimensions must be positive, got {width}×{height}")]
    InvalidDimensions { width: u32, height: u32 },
    #[error("LayerMask voxel_size_mm must be positive and finite, got {0}")]
    InvalidVoxelSize(f32),
    #[error("LayerMask set({x},{y}) out of bounds for {width}×{height}")]
    OutOfBounds {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
}

/// 2D binary occupancy mask at a fixed physical resolution. Immutable after
/// construction except via `set`; bit-packed row-major.
///
/// Equality semantics: two LayerMasks compare equal iff their dimensions,
/// voxel size, and bit contents all match. Useful for test fixtures and
/// snapshot comparisons.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerMask {
    width_cells: u32,
    height_cells: u32,
    voxel_size_mm: f32,
    bits: BitVec,
}

impl LayerMask {
    /// Construct a new all-void (all-zero) mask.
    ///
    /// Returns `Err(MaskError::InvalidDimensions)` if width or height is 0.
    /// Returns `Err(MaskError::InvalidVoxelSize)` if voxel_size_mm is not
    /// positive finite.
    pub fn new(
        width_cells: u32,
        height_cells: u32,
        voxel_size_mm: f32,
    ) -> Result<Self, MaskError> {
        if width_cells == 0 || height_cells == 0 {
            return Err(MaskError::InvalidDimensions {
                width: width_cells,
                height: height_cells,
            });
        }
        if !voxel_size_mm.is_finite() || voxel_size_mm <= 0.0 {
            return Err(MaskError::InvalidVoxelSize(voxel_size_mm));
        }
        let total = (width_cells as usize) * (height_cells as usize);
        let mut bits = BitVec::with_capacity(total);
        bits.resize(total, false);
        Ok(Self {
            width_cells,
            height_cells,
            voxel_size_mm,
            bits,
        })
    }

    /// Construct a mask with every cell set to solid. Useful for representing
    /// a fully-occupied layer (e.g. the synthetic mask emitted by
    /// `SimulationRunner::run_from_areas`'s area→mask adapter).
    pub fn new_all_solid(
        width_cells: u32,
        height_cells: u32,
        voxel_size_mm: f32,
    ) -> Result<Self, MaskError> {
        let mut mask = Self::new(width_cells, height_cells, voxel_size_mm)?;
        mask.bits.fill(true);
        Ok(mask)
    }

    pub fn width_cells(&self) -> u32 {
        self.width_cells
    }

    pub fn height_cells(&self) -> u32 {
        self.height_cells
    }

    pub fn voxel_size_mm(&self) -> f32 {
        self.voxel_size_mm
    }

    pub fn width_mm(&self) -> f32 {
        self.width_cells as f32 * self.voxel_size_mm
    }

    pub fn height_mm(&self) -> f32 {
        self.height_cells as f32 * self.voxel_size_mm
    }

    /// Mark cell (x, y) as solid. Returns `Err(MaskError::OutOfBounds)` if
    /// the coordinates exceed the mask dimensions.
    pub fn set(&mut self, x: u32, y: u32) -> Result<(), MaskError> {
        self.check_bounds(x, y)?;
        let idx = self.index(x, y);
        self.bits.set(idx, true);
        Ok(())
    }

    /// Clear cell (x, y) (mark as void). Returns `Err(MaskError::OutOfBounds)`
    /// if out of range.
    pub fn clear(&mut self, x: u32, y: u32) -> Result<(), MaskError> {
        self.check_bounds(x, y)?;
        let idx = self.index(x, y);
        self.bits.set(idx, false);
        Ok(())
    }

    /// Returns `true` if cell (x, y) is solid. Returns `false` for
    /// out-of-bounds coordinates (caller's ergonomic convention — CavityDetector
    /// treats off-grid cells as void/exterior, which is the physically correct
    /// reading).
    pub fn is_solid(&self, x: u32, y: u32) -> bool {
        if x >= self.width_cells || y >= self.height_cells {
            return false;
        }
        let idx = self.index(x, y);
        self.bits[idx]
    }

    /// Count of solid cells.
    pub fn solid_cell_count(&self) -> usize {
        self.bits.count_ones()
    }

    /// Area of solid region in mm², computed from cell count × voxel_size².
    /// See module-level docs re: precision vs. native-pixel area.
    pub fn solid_area_mm2(&self) -> f64 {
        let area_per_cell = (self.voxel_size_mm as f64).powi(2);
        self.solid_cell_count() as f64 * area_per_cell
    }

    /// Iterate over (x, y) of all solid cells in row-major order.
    pub fn iter_solid(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        let w = self.width_cells;
        self.bits.iter_ones().map(move |idx| {
            let idx = idx as u32;
            (idx % w, idx / w)
        })
    }

    /// Returns `true` if any cell on the lateral bbox edge (x=0 or
    /// x=width-1 or y=0 or y=height-1) is void. Used by CavityDetector to
    /// determine whether a void pocket is exterior-reachable via the vat.
    pub fn has_void_on_lateral_edge(&self) -> bool {
        for x in 0..self.width_cells {
            if !self.is_solid(x, 0) {
                return true;
            }
            if !self.is_solid(x, self.height_cells - 1) {
                return true;
            }
        }
        for y in 0..self.height_cells {
            if !self.is_solid(0, y) {
                return true;
            }
            if !self.is_solid(self.width_cells - 1, y) {
                return true;
            }
        }
        false
    }

    fn check_bounds(&self, x: u32, y: u32) -> Result<(), MaskError> {
        if x >= self.width_cells || y >= self.height_cells {
            return Err(MaskError::OutOfBounds {
                x,
                y,
                width: self.width_cells,
                height: self.height_cells,
            });
        }
        Ok(())
    }

    fn index(&self, x: u32, y: u32) -> usize {
        (y as usize) * (self.width_cells as usize) + (x as usize)
    }
}

/// Per-layer slicer output combining scalar area (native-precision) with the
/// 2D occupancy mask (voxel-precision). Returned by `slice_layers` in Step 4.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerGeometry {
    pub area: CrossSectionArea,
    pub mask: LayerMask,
}

impl LayerGeometry {
    pub fn new(area: CrossSectionArea, mask: LayerMask) -> Self {
        Self { area, mask }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_zero_width() {
        assert!(matches!(
            LayerMask::new(0, 10, 0.5),
            Err(MaskError::InvalidDimensions { width: 0, .. })
        ));
    }

    #[test]
    fn new_rejects_zero_height() {
        assert!(matches!(
            LayerMask::new(10, 0, 0.5),
            Err(MaskError::InvalidDimensions { height: 0, .. })
        ));
    }

    #[test]
    fn new_rejects_zero_voxel_size() {
        assert!(matches!(
            LayerMask::new(10, 10, 0.0),
            Err(MaskError::InvalidVoxelSize(_))
        ));
    }

    #[test]
    fn new_rejects_nan_voxel_size() {
        assert!(matches!(
            LayerMask::new(10, 10, f32::NAN),
            Err(MaskError::InvalidVoxelSize(_))
        ));
    }

    #[test]
    fn new_rejects_negative_voxel_size() {
        assert!(matches!(
            LayerMask::new(10, 10, -0.5),
            Err(MaskError::InvalidVoxelSize(_))
        ));
    }

    #[test]
    fn new_default_is_all_void() {
        let mask = LayerMask::new(10, 10, 0.5)
            .expect("valid 10×10 @ 0.5mm mask must construct");
        assert_eq!(mask.solid_cell_count(), 0);
        assert_eq!(mask.solid_area_mm2(), 0.0);
    }

    #[test]
    fn new_all_solid_fills_grid() {
        let mask = LayerMask::new_all_solid(4, 3, 1.0)
            .expect("valid 4×3 @ 1.0mm mask must construct");
        assert_eq!(mask.solid_cell_count(), 12);
        assert_eq!(mask.solid_area_mm2(), 12.0);
        for x in 0..4 {
            for y in 0..3 {
                assert!(mask.is_solid(x, y), "all cells should be solid");
            }
        }
    }

    #[test]
    fn set_and_is_solid_roundtrip() {
        let mut mask = LayerMask::new(5, 5, 0.5)
            .expect("valid 5×5 @ 0.5mm mask must construct");
        mask.set(2, 3)
            .expect("set(2,3) within 5×5 is in bounds");
        assert!(mask.is_solid(2, 3));
        assert!(!mask.is_solid(2, 4));
        assert!(!mask.is_solid(0, 0));
        assert_eq!(mask.solid_cell_count(), 1);
        assert_eq!(mask.solid_area_mm2(), 0.25);
    }

    #[test]
    fn set_out_of_bounds_returns_err() {
        let mut mask = LayerMask::new(5, 5, 0.5)
            .expect("valid 5×5 @ 0.5mm mask must construct");
        assert!(matches!(
            mask.set(10, 3),
            Err(MaskError::OutOfBounds { x: 10, .. })
        ));
        assert!(matches!(
            mask.set(3, 10),
            Err(MaskError::OutOfBounds { y: 10, .. })
        ));
    }

    #[test]
    fn is_solid_out_of_bounds_returns_false() {
        let mask = LayerMask::new(5, 5, 0.5)
            .expect("valid 5×5 @ 0.5mm mask must construct");
        assert!(!mask.is_solid(100, 100));
        assert!(!mask.is_solid(0, 100));
        assert!(!mask.is_solid(100, 0));
    }

    #[test]
    fn clear_flips_solid_back_to_void() {
        let mut mask = LayerMask::new_all_solid(3, 3, 0.5)
            .expect("valid 3×3 @ 0.5mm mask must construct");
        mask.clear(1, 1)
            .expect("clear(1,1) within 3×3 is in bounds");
        assert!(!mask.is_solid(1, 1));
        assert_eq!(mask.solid_cell_count(), 8);
    }

    #[test]
    fn iter_solid_returns_all_solid_cells() {
        let mut mask = LayerMask::new(4, 4, 1.0)
            .expect("valid 4×4 @ 1.0mm mask must construct");
        mask.set(0, 0).expect("in bounds");
        mask.set(1, 2).expect("in bounds");
        mask.set(3, 3).expect("in bounds");

        let cells: Vec<(u32, u32)> = mask.iter_solid().collect();
        assert_eq!(cells.len(), 3);
        assert!(cells.contains(&(0, 0)));
        assert!(cells.contains(&(1, 2)));
        assert!(cells.contains(&(3, 3)));
    }

    #[test]
    fn width_mm_height_mm_derive_from_cells_and_voxel_size() {
        let mask = LayerMask::new(300, 150, 0.5)
            .expect("valid 300×150 @ 0.5mm mask must construct");
        assert_eq!(mask.width_mm(), 150.0);
        assert_eq!(mask.height_mm(), 75.0);
    }

    #[test]
    fn has_void_on_lateral_edge_true_when_edge_unset() {
        let mask = LayerMask::new(5, 5, 0.5)
            .expect("valid 5×5 @ 0.5mm mask must construct");
        // Default all-void → every edge cell is void
        assert!(mask.has_void_on_lateral_edge());
    }

    #[test]
    fn has_void_on_lateral_edge_false_when_edges_solid() {
        let mask = LayerMask::new_all_solid(5, 5, 0.5)
            .expect("valid 5×5 @ 0.5mm mask must construct");
        // Fully solid → no void cells anywhere, including edges
        assert!(!mask.has_void_on_lateral_edge());
    }

    #[test]
    fn has_void_on_lateral_edge_detects_single_void_cell() {
        let mut mask = LayerMask::new_all_solid(5, 5, 0.5)
            .expect("valid 5×5 @ 0.5mm mask must construct");
        mask.clear(0, 2).expect("clear(0,2) in bounds");
        assert!(mask.has_void_on_lateral_edge());
    }

    #[test]
    fn has_void_on_lateral_edge_ignores_interior_voids() {
        let mut mask = LayerMask::new_all_solid(5, 5, 0.5)
            .expect("valid 5×5 @ 0.5mm mask must construct");
        // Void in the middle (2,2) — not on any lateral edge
        mask.clear(2, 2).expect("clear(2,2) in bounds");
        assert!(!mask.has_void_on_lateral_edge());
    }

    #[test]
    fn equality_requires_matching_dimensions_voxel_size_and_bits() {
        let a = LayerMask::new(3, 3, 0.5)
            .expect("valid 3×3 @ 0.5mm mask must construct");
        let b = LayerMask::new(3, 3, 0.5)
            .expect("valid 3×3 @ 0.5mm mask must construct");
        assert_eq!(a, b);

        let c = LayerMask::new(3, 3, 1.0)
            .expect("valid 3×3 @ 1.0mm mask must construct");
        assert_ne!(a, c);

        let d = LayerMask::new(4, 3, 0.5)
            .expect("valid 4×3 @ 0.5mm mask must construct");
        assert_ne!(a, d);
    }

    #[test]
    fn layer_geometry_pairs_area_and_mask() {
        let area = CrossSectionArea::new(10.0)
            .expect("area 10 mm² is in CrossSectionArea domain");
        let mask = LayerMask::new(5, 5, 1.0)
            .expect("valid 5×5 @ 1.0mm mask must construct");
        let geom = LayerGeometry::new(area, mask.clone());
        assert_eq!(geom.area, area);
        assert_eq!(geom.mask, mask);
    }
}
