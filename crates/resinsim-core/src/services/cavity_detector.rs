//! 3D cavity detection for MSLA resin prints.
//!
//! Consumes a stack of per-layer [`LayerMask`]s and emits one [`CavityEvent`]
//! per topologically-sealed void pocket at the layer where that pocket closes
//! from below (FEP direction). This is the 3D-correct replacement for the 2D
//! area-drop heuristic that previously produced false-positive suction risks
//! at raft→supports transitions.
//!
//! # Why 3D topology, not area drop
//!
//! The old area-drop heuristic flagged any layer whose solid area dropped
//! sharply — which matches both:
//!
//! - sealed cup geometry (solid base → ring walls with trapped fluid inside),
//! - support-column geometry (solid raft → discrete columns with fluid-permeable gaps).
//!
//! In 2D they are indistinguishable. In 3D they differ: the cup's interior
//! void is surrounded by wall material on all lateral sides, while the
//! inter-column void touches the bounding-box edge and drains freely to the
//! vat. This service distinguishes them by tracking void connectivity across
//! layers and to the lateral bbox exterior.
//!
//! # Ambient boundary policy
//!
//! - **Lateral bbox faces** (x=0, x=width-1, y=0, y=height-1 at any layer)
//!   = exterior. Voids touching these drain to the vat — no suction.
//! - **z=0** (build plate face) = barrier. The build plate is a rigid
//!   solid; voids sealed only on top by layer-0 solid ARE sealed.
//! - **z=N-1** (FEP face during peel) = barrier. Pockets extending to the
//!   last layer without closure are "open at FEP" during peel — classic
//!   ring-wall peel with no concentrated vacuum. No event emitted.
//!
//! # Algorithm (streaming layer-by-layer)
//!
//! ```text
//! pockets: HashMap<PocketId, PocketInfo>    // alive pockets tracked so far
//! prev_labels: Vec<Option<PocketId>>        // per-cell label at layer k-1
//!
//! // Layer 0 initialisation:
//! for each connected-component of void cells in mask[0]:
//!     assign fresh PocketId; record first_open_layer=0,
//!     touches_lateral_exterior (any cell on lateral edge)
//!
//! // Layer k, k ≥ 1:
//! new_labels = vec![None; w*h]
//! new_alive = HashSet<PocketId>
//! for each connected-component C of void cells in mask[k]:
//!     inherited_ids = { prev_labels[cell] for cell in C if Some }
//!     if inherited_ids empty:
//!         assign fresh PocketId
//!     else:
//!         merge inherited pocket infos into canonical id (lowest);
//!         merged info = min(first_open_layer), any(touches_lateral_exterior)
//!     update canonical's touches_lateral_exterior with C's edge flag
//!     write canonical id into new_labels[cell] for each cell in C
//!     mark canonical alive for this layer
//!
//! // Events: pockets that were alive at k-1 but not at k have closed.
//! for (id, info) in pockets where !new_alive.contains(id):
//!     if !info.touches_lateral_exterior:
//!         sealed_area = count(prev_labels == Some(id)) × voxel_size²
//!         emit CavityEvent { layer: k, sealed_area, ... }
//! pockets.retain(alive)
//! prev_labels = new_labels
//! ```
//!
//! # Hand trace: solid cube (all layers fully solid)
//!
//! No void cells anywhere → no pockets ever created → 0 events. ✓
//!
//! # Hand trace: raft + support columns (the lilith-torso repro)
//!
//! Layer 0 (raft): solid across the whole bbox.  0 void cells → 0 pockets.
//! Layer 23 (columns): void cells in inter-column gaps. They touch the
//! lateral bbox edge (columns are centred, gaps run to the edge) →
//! touches_lateral_exterior = true. Pocket alive but exterior.
//! Layer 23+: pocket continues, still exterior.
//! Final layer: pocket still alive, still exterior. No event (alive pockets
//! at end are not emitted anyway). ✓
//!
//! # Hand trace: closed cup (solid base, ring walls, solid top)
//!
//! Layer 0 (solid base): no void cells → no pocket.
//! Layer 1 (ring walls start): inside-of-ring cells are newly void, they
//! form a connected component entirely interior (no lateral-edge cells).
//! Fresh pocket P1 with first_open_layer=1, touches_lateral_exterior=false.
//! Layers 2..M (rings continue): component continues, P1 stays alive with
//! cross-section equal to ring interior.
//! Layer M+1 (solid top): no void cells above cavity. P1 has zero cells in
//! new_labels → P1 closed at M+1. Not exterior → emit CavityEvent at M+1
//! with sealed_area = ring interior area. ✓

use std::collections::{HashMap, HashSet, VecDeque};

use thiserror::Error;

use crate::values::LayerMask;

/// Partial-vacuum pressure assumed to form at a sealed cavity during peel.
/// Consistent with the physics model used by the pre-existing area-based
/// detector (50 kPa ≈ half atmospheric; strong enough to require non-trivial
/// peel force at realistic cavity sizes). E4 Athena II calibration is future
/// work for resin/FEP-specific tuning.
pub const VACUUM_PRESSURE_KPA: f64 = 50.0;

/// Minimum sealed-cavity cross-section to emit as a [`CavityEvent`].
///
/// Cavities smaller than this produce suction force below the physical
/// noise floor of typical MSLA hardware (e.g., 1 mm² × 50 kPa = 0.05 N —
/// orders of magnitude below the build-plate motor's lift reserve and
/// dwarfed by peel adhesion). Downstream severity grading in
/// `FailurePredictor` already thresholds at 1 N; events below that can't
/// change the failure outcome. Dropping them at detection time keeps the
/// output focused on force magnitudes that matter to the user.
///
/// Physical rationale: aligns with the principle that a suction cup only
/// matters when it overpowers the lift mechanism. 1 mm² is a conservative
/// noise floor; voxel-edge artefacts (single-cell components from
/// downsampling rounding) are filtered out.
pub const MIN_SEALED_AREA_MM2: f64 = 1.0;

/// Errors returned by [`CavityDetector::detect`].
#[derive(Debug, Clone, PartialEq, Error)]
pub enum CavityError {
    /// Caller passed an empty mask slice.
    #[error("cannot detect cavities: no masks provided")]
    NoMasks,
    /// Masks in the input slice have different `voxel_size_mm` or
    /// `(width_cells, height_cells)`. Mixing resolutions silently would
    /// produce wrong results.
    #[error(
        "inconsistent masks at index {index}: expected {expected_w}×{expected_h} @ {expected_voxel_mm} mm, got {actual_w}×{actual_h} @ {actual_voxel_mm} mm"
    )]
    InconsistentMasks {
        index: usize,
        expected_w: u32,
        expected_h: u32,
        expected_voxel_mm: f32,
        actual_w: u32,
        actual_h: u32,
        actual_voxel_mm: f32,
    },
}

/// A topologically-sealed cavity detected during the 3D walk.
#[derive(Debug, Clone, PartialEq)]
pub struct CavityEvent {
    /// Layer at which the cavity closed from below (FEP direction). This is
    /// the peel event that generates concentrated vacuum. For a cavity with
    /// walls at layers `[a..b]` and a solid floor at `a-1` and a solid cap
    /// at `b+1`, the event is at layer `b+1`.
    pub layer: u32,
    /// Physical area of the sealing interface at peel time. Computed at
    /// `voxel_size_mm` resolution; may differ from native-pixel area by up
    /// to ~10% at 0.5 mm voxels for sub-mm features. See
    /// [`LayerMask::solid_area_mm2`] rustdoc.
    pub sealed_area_mm2: f64,
    /// Force estimate: `VACUUM_PRESSURE_KPA × sealed_area_mm2 × 1e-3`.
    pub suction_force_n: f32,
    /// Layer at which the cavity first became non-empty.
    pub first_open_layer: u32,
}

/// Stateless domain service. All inputs via parameters.
pub struct CavityDetector;

type PocketId = u32;

/// Per-cell pocket label map for one layer (None = solid, Some(id) = void in pocket id).
type PocketLabelMap = Vec<Option<PocketId>>;

/// List of connected void components within one layer; each component is its (x, y) cells.
type VoidComponents = Vec<Vec<(u32, u32)>>;

#[derive(Debug, Clone)]
struct PocketInfo {
    first_open_layer: u32,
    touches_lateral_exterior: bool,
}

impl CavityDetector {
    /// Walk a stack of layer masks and emit one [`CavityEvent`] per
    /// topologically-sealed cavity at the layer it closes. See module
    /// rustdoc for algorithm and ambient-boundary policy.
    ///
    /// # Errors
    ///
    /// - [`CavityError::NoMasks`] if `masks` is empty.
    /// - [`CavityError::InconsistentMasks`] if any mask has a different
    ///   `voxel_size_mm` or `(width_cells, height_cells)` than mask 0.
    pub fn detect(masks: &[LayerMask]) -> Result<Vec<CavityEvent>, CavityError> {
        if masks.is_empty() {
            return Err(CavityError::NoMasks);
        }
        let first = &masks[0];
        let width = first.width_cells();
        let height = first.height_cells();
        let voxel_size = first.voxel_size_mm();
        for (i, m) in masks.iter().enumerate().skip(1) {
            if m.width_cells() != width
                || m.height_cells() != height
                || m.voxel_size_mm() != voxel_size
            {
                return Err(CavityError::InconsistentMasks {
                    index: i,
                    expected_w: width,
                    expected_h: height,
                    expected_voxel_mm: voxel_size,
                    actual_w: m.width_cells(),
                    actual_h: m.height_cells(),
                    actual_voxel_mm: m.voxel_size_mm(),
                });
            }
        }

        let cell_count = (width as usize) * (height as usize);
        let voxel_area = (voxel_size as f64) * (voxel_size as f64);

        let mut pockets: HashMap<PocketId, PocketInfo> = HashMap::new();
        let mut next_id: PocketId = 0;
        let mut prev_labels: Vec<Option<PocketId>> = vec![None; cell_count];

        let mut events = Vec::new();

        // Layer 0: seed pockets from void connected-components.
        {
            let mask0 = &masks[0];
            let (labels, components) = connected_components_of_void(mask0, width, height);
            prev_labels = labels;
            for (id_local, comp) in components.iter().enumerate() {
                let canonical_id = next_id + id_local as u32;
                let touches = comp.iter().any(|&(x, y)| is_on_lateral_edge(x, y, width, height));
                pockets.insert(
                    canonical_id,
                    PocketInfo {
                        first_open_layer: 0,
                        touches_lateral_exterior: touches,
                    },
                );
            }
            // Rewrite labels from local (0..C) to global (next_id..next_id+C).
            for slot in prev_labels.iter_mut() {
                if let Some(local) = slot {
                    *slot = Some(next_id + *local);
                }
            }
            next_id += components.len() as u32;
        }

        // Layers 1..N
        for (k, mask_k) in masks.iter().enumerate().skip(1) {
            let (local_labels, components) =
                connected_components_of_void(mask_k, width, height);
            let mut new_labels: Vec<Option<PocketId>> = vec![None; cell_count];
            let mut alive_this_layer: HashSet<PocketId> = HashSet::new();

            for comp in &components {
                // Collect distinct inherited pocket ids from prev layer.
                let mut inherited: HashSet<PocketId> = HashSet::new();
                for &(x, y) in comp {
                    let idx = (y as usize) * (width as usize) + (x as usize);
                    if let Some(id) = prev_labels[idx] {
                        inherited.insert(id);
                    }
                }
                let touches_edge =
                    comp.iter().any(|&(x, y)| is_on_lateral_edge(x, y, width, height));

                let canonical = if inherited.is_empty() {
                    let id = next_id;
                    next_id += 1;
                    pockets.insert(
                        id,
                        PocketInfo {
                            first_open_layer: k as u32,
                            touches_lateral_exterior: touches_edge,
                        },
                    );
                    id
                } else {
                    let canonical = *inherited.iter().min().expect("inherited is non-empty");
                    // Merge all other inherited pockets into canonical.
                    for &other in inherited.iter() {
                        if other == canonical {
                            continue;
                        }
                        if let Some(other_info) = pockets.remove(&other)
                            && let Some(canon) = pockets.get_mut(&canonical)
                        {
                            if other_info.first_open_layer < canon.first_open_layer {
                                canon.first_open_layer = other_info.first_open_layer;
                            }
                            canon.touches_lateral_exterior |=
                                other_info.touches_lateral_exterior;
                        }
                        // Rewrite any prev_labels that pointed to `other` → `canonical`,
                        // so the pending sealed-area count lands on the right pocket.
                        for slot in prev_labels.iter_mut() {
                            if *slot == Some(other) {
                                *slot = Some(canonical);
                            }
                        }
                    }
                    // OR in the current component's edge flag.
                    if let Some(canon) = pockets.get_mut(&canonical) {
                        canon.touches_lateral_exterior |= touches_edge;
                    }
                    canonical
                };

                for &(x, y) in comp {
                    let idx = (y as usize) * (width as usize) + (x as usize);
                    new_labels[idx] = Some(canonical);
                }
                alive_this_layer.insert(canonical);
            }

            // Also handle the case where a previous-layer pocket had its cells
            // all appear in `local_labels` as solid at this layer (i.e., pocket
            // fully sealed from below). Those pockets are NOT in
            // alive_this_layer → close them.
            // (The removal happens inside the next loop.)

            // Identify pockets that closed at layer k.
            let closed: Vec<PocketId> = pockets
                .keys()
                .filter(|id| !alive_this_layer.contains(id))
                .copied()
                .collect();
            for id in closed {
                let info = pockets
                    .remove(&id)
                    .expect("id came from pockets.keys()");
                if info.touches_lateral_exterior {
                    continue;
                }
                // Count voxels in prev_labels with this id → sealing cross-section.
                let sealed_cells = prev_labels.iter().filter(|&&l| l == Some(id)).count();
                if sealed_cells == 0 {
                    continue;
                }
                let sealed_area = (sealed_cells as f64) * voxel_area;
                if sealed_area < MIN_SEALED_AREA_MM2 {
                    // Below physical noise floor — small cavities do not overpower
                    // the lift mechanism. See MIN_SEALED_AREA_MM2 rustdoc.
                    continue;
                }
                let suction_force = (VACUUM_PRESSURE_KPA * sealed_area * 1e-3) as f32;
                events.push(CavityEvent {
                    layer: k as u32,
                    sealed_area_mm2: sealed_area,
                    suction_force_n: suction_force,
                    first_open_layer: info.first_open_layer,
                });
            }

            let _ = local_labels; // silence unused; we didn't need the raw local labels
            prev_labels = new_labels;
        }

        Ok(events)
    }
}

fn is_on_lateral_edge(x: u32, y: u32, width: u32, height: u32) -> bool {
    x == 0 || y == 0 || x + 1 == width || y + 1 == height
}

/// 4-connected connected-component labelling of void cells within a layer.
///
/// Returns (per-cell label map, list of components). Each component is the
/// set of (x, y) cells in that component. Labels are local to this call
/// (0..n_components); callers remap to global pocket ids as needed.
fn connected_components_of_void(
    mask: &LayerMask,
    width: u32,
    height: u32,
) -> (PocketLabelMap, VoidComponents) {
    let total = (width as usize) * (height as usize);
    let mut labels: Vec<Option<PocketId>> = vec![None; total];
    let mut components: Vec<Vec<(u32, u32)>> = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let idx = (y as usize) * (width as usize) + (x as usize);
            if labels[idx].is_some() || mask.is_solid(x, y) {
                continue;
            }
            // BFS flood fill
            let label = components.len() as PocketId;
            let mut comp: Vec<(u32, u32)> = Vec::new();
            let mut queue: VecDeque<(u32, u32)> = VecDeque::new();
            queue.push_back((x, y));
            labels[idx] = Some(label);

            while let Some((cx, cy)) = queue.pop_front() {
                comp.push((cx, cy));
                // 4-neighbours
                let neigh: [(i64, i64); 4] = [
                    (cx as i64 - 1, cy as i64),
                    (cx as i64 + 1, cy as i64),
                    (cx as i64, cy as i64 - 1),
                    (cx as i64, cy as i64 + 1),
                ];
                for (nx, ny) in neigh {
                    if nx < 0 || ny < 0 || nx >= width as i64 || ny >= height as i64 {
                        continue;
                    }
                    let nx = nx as u32;
                    let ny = ny as u32;
                    let nidx = (ny as usize) * (width as usize) + (nx as usize);
                    if labels[nidx].is_some() {
                        continue;
                    }
                    if mask.is_solid(nx, ny) {
                        continue;
                    }
                    labels[nidx] = Some(label);
                    queue.push_back((nx, ny));
                }
            }
            components.push(comp);
        }
    }
    (labels, components)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask(w: u32, h: u32) -> LayerMask {
        LayerMask::new(w, h, 1.0).expect("test fixture: valid mask dimensions")
    }

    fn solid_mask(w: u32, h: u32) -> LayerMask {
        LayerMask::new_all_solid(w, h, 1.0).expect("test fixture: valid mask dimensions")
    }

    /// Build a 5×5 "ring wall" mask: solid border, void interior.
    fn ring_wall_mask() -> LayerMask {
        let mut m = mask(5, 5);
        for x in 0..5 {
            m.set(x, 0).expect("in bounds");
            m.set(x, 4).expect("in bounds");
        }
        for y in 0..5 {
            m.set(0, y).expect("in bounds");
            m.set(4, y).expect("in bounds");
        }
        m
    }

    #[test]
    fn detect_rejects_empty_input() {
        assert!(matches!(CavityDetector::detect(&[]), Err(CavityError::NoMasks)));
    }

    #[test]
    fn detect_rejects_mixed_voxel_sizes() {
        let a = LayerMask::new(4, 4, 0.5).expect("valid");
        let b = LayerMask::new(4, 4, 1.0).expect("valid");
        assert!(matches!(
            CavityDetector::detect(&[a, b]),
            Err(CavityError::InconsistentMasks { .. })
        ));
    }

    #[test]
    fn detect_rejects_mismatched_dimensions() {
        let a = LayerMask::new(4, 4, 0.5).expect("valid");
        let b = LayerMask::new(5, 4, 0.5).expect("valid");
        assert!(matches!(
            CavityDetector::detect(&[a, b]),
            Err(CavityError::InconsistentMasks { .. })
        ));
    }

    #[test]
    fn solid_cube_no_events() {
        let stack: Vec<LayerMask> = (0..5).map(|_| solid_mask(4, 4)).collect();
        let events = CavityDetector::detect(&stack).expect("valid input");
        assert!(events.is_empty());
    }

    #[test]
    fn all_void_no_events_lateral_exterior_everywhere() {
        let stack: Vec<LayerMask> = (0..5).map(|_| mask(4, 4)).collect();
        let events = CavityDetector::detect(&stack).expect("valid input");
        // All-void masks → the single pocket touches lateral edges → exterior → no events.
        assert!(events.is_empty());
    }

    #[test]
    fn closed_cup_emits_one_event_at_closure_layer() {
        // Layer 0: solid base (no void). Layers 1-3: ring wall (interior void).
        // Layer 4: solid cap. Expect exactly one event at layer 4.
        let mut stack = Vec::new();
        stack.push(solid_mask(5, 5)); // layer 0 floor
        for _ in 0..3 {
            stack.push(ring_wall_mask()); // layers 1-3 walls
        }
        stack.push(solid_mask(5, 5)); // layer 4 cap
        let events = CavityDetector::detect(&stack).expect("valid input");
        assert_eq!(events.len(), 1, "expected one event, got {events:?}");
        let e = &events[0];
        assert_eq!(e.layer, 4, "event should be at closure layer");
        assert_eq!(e.first_open_layer, 1);
        // Ring interior is 3×3 = 9 cells at 1mm voxel = 9 mm²
        assert!((e.sealed_area_mm2 - 9.0).abs() < 1e-6);
        // 50 kPa × 9 mm² × 1e-3 = 0.45 N
        assert!((e.suction_force_n - 0.45).abs() < 1e-3);
    }

    #[test]
    fn open_topped_cup_no_events() {
        // Layer 0 solid floor, layers 1-3 ring walls, no solid cap.
        let mut stack = Vec::new();
        stack.push(solid_mask(5, 5));
        for _ in 0..3 {
            stack.push(ring_wall_mask());
        }
        let events = CavityDetector::detect(&stack).expect("valid input");
        // Pocket still alive at end → "open at FEP" → no event emitted.
        assert!(events.is_empty());
    }

    #[test]
    fn fully_sealed_interior_pocket() {
        // Layer 0 solid (roof), layers 1-3 ring walls, layer 4 solid (floor).
        let mut stack = Vec::new();
        stack.push(solid_mask(5, 5));
        for _ in 0..3 {
            stack.push(ring_wall_mask());
        }
        stack.push(solid_mask(5, 5));
        let events = CavityDetector::detect(&stack).expect("valid input");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].layer, 4);
    }

    #[test]
    fn lateral_touching_void_is_exterior() {
        // Cup-like shape but one wall cell is missing → void touches lateral edge.
        let mut stack = Vec::new();
        stack.push(solid_mask(5, 5)); // floor
        for _ in 0..3 {
            let mut m = ring_wall_mask();
            // Knock out a wall cell on the edge — void now reaches bbox edge
            m.clear(2, 0).expect("in bounds");
            stack.push(m);
        }
        stack.push(solid_mask(5, 5)); // cap
        let events = CavityDetector::detect(&stack).expect("valid input");
        assert!(events.is_empty(), "lateral-edge-touching void must not emit");
    }

    #[test]
    fn two_disjoint_cups_two_events() {
        // Build a 9×5 workspace with two closed cups side by side (cup A at
        // x∈[0..5], cup B at x∈[5..9] + reusing column 4 shared wall).
        // For simplicity, use separated cups without shared walls.
        let wf = 9;
        let hf = 5;
        let floor = solid_mask(wf, hf);
        // Wall layer: two rings in the 9×5 span — cup A [0..5), cup B [5..9).
        let mut wall = solid_mask(wf, hf);
        // Cup A interior: cells (1,1), (1,2), (1,3), (2,1), (2,2), (2,3), (3,1), (3,2), (3,3)
        // but ring means only interior (1..4 × 1..4). Let me just do 3x3 interior.
        for x in 1..4 {
            for y in 1..4 {
                wall.clear(x, y).expect("in bounds");
            }
        }
        // Cup B interior: (5..8) × (1..4)
        for x in 5..8 {
            for y in 1..4 {
                wall.clear(x, y).expect("in bounds");
            }
        }
        let cap = solid_mask(wf, hf);
        let stack = vec![floor, wall.clone(), wall.clone(), wall.clone(), cap];

        let events = CavityDetector::detect(&stack).expect("valid input");
        assert_eq!(events.len(), 2, "got {events:?}");
        // Both cups close at layer 4
        for e in &events {
            assert_eq!(e.layer, 4);
            // Each cup interior is 3x3 = 9 mm²
            assert!((e.sealed_area_mm2 - 9.0).abs() < 1e-6);
        }
    }

    #[test]
    fn raft_plus_columns_no_suction() {
        // The lilith-torso reproduction scaled down: 11x11 workspace.
        // Layer 0 (raft): fully solid plate.
        // Layers 1..5 (columns): discrete 1x1 columns at (2,2), (2,8), (8,2), (8,8)
        //                        with inter-column gaps touching the bbox edge.
        let raft = solid_mask(11, 11);
        let mut columns = mask(11, 11);
        // 4 single-cell columns — gaps between them reach the bbox edge
        columns.set(2, 2).expect("in bounds");
        columns.set(2, 8).expect("in bounds");
        columns.set(8, 2).expect("in bounds");
        columns.set(8, 8).expect("in bounds");

        let stack = vec![
            raft,
            columns.clone(),
            columns.clone(),
            columns.clone(),
            columns.clone(),
            columns,
        ];
        let events = CavityDetector::detect(&stack).expect("valid input");
        // Inter-column void is a single large region touching the lateral bbox
        // edge → exterior → no events. Even when the raft "seals" the void
        // from above, the lateral edge keeps it exterior.
        assert!(events.is_empty(), "got unexpected events: {events:?}");
    }

    #[test]
    fn below_min_sealed_area_threshold_no_event() {
        // Construct a sealed cavity whose interior is 1 cell (1 mm² at 1 mm
        // voxel) — below MIN_SEALED_AREA_MM2 (1.0 mm²).
        // Actually at MIN = 1.0 and 1 cell = 1.0 mm², this is exactly at the
        // threshold. Make the voxel smaller so we're clearly below.
        let floor = LayerMask::new_all_solid(3, 3, 0.5).expect("valid");
        let mut wall = LayerMask::new_all_solid(3, 3, 0.5).expect("valid");
        wall.clear(1, 1).expect("in bounds"); // interior void = 1 cell × 0.25 mm² = 0.25 mm²
        let cap = LayerMask::new_all_solid(3, 3, 0.5).expect("valid");
        let _ = floor.solid_area_mm2();
        let stack = vec![floor, wall.clone(), wall, cap];
        let events = CavityDetector::detect(&stack).expect("valid");
        assert!(events.is_empty(), "sub-threshold cavity should not emit: {events:?}");
    }

    #[test]
    fn event_force_follows_area_linearly() {
        // Two different cup interior sizes → two different forces, linear ratio.
        let small = {
            let mut stack = Vec::new();
            stack.push(solid_mask(5, 5));
            for _ in 0..3 {
                stack.push(ring_wall_mask());
            }
            stack.push(solid_mask(5, 5));
            CavityDetector::detect(&stack).expect("valid")
        };
        let large = {
            // 7×7 ring → interior 5×5 = 25 mm² (vs small 3×3 = 9 mm²)
            let mut m = mask(7, 7);
            for x in 0..7 {
                m.set(x, 0).expect("in bounds");
                m.set(x, 6).expect("in bounds");
            }
            for y in 0..7 {
                m.set(0, y).expect("in bounds");
                m.set(6, y).expect("in bounds");
            }
            let mut stack = Vec::new();
            stack.push(solid_mask(7, 7));
            for _ in 0..3 {
                stack.push(m.clone());
            }
            stack.push(solid_mask(7, 7));
            CavityDetector::detect(&stack).expect("valid")
        };
        assert_eq!(small.len(), 1);
        assert_eq!(large.len(), 1);
        let ratio = large[0].suction_force_n / small[0].suction_force_n;
        // 25/9 ≈ 2.78
        assert!((ratio - 25.0 / 9.0).abs() < 0.01);
    }
}
