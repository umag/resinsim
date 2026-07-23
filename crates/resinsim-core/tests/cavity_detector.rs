//! Integration tests for `CavityDetector` — the 3D cavity-detection service
//! at the heart of the suction-detector-raft-false-positive fix. Specs in
//! plan v6 Step 1, bodies filled in across Steps 2-7 as the supporting types
//! landed.

use proptest::prelude::*;
use resinsim_core::entities::DEFAULT_VACUUM_PRESSURE_KPA;
use resinsim_core::services::{CavityDetector, CavityError};
use resinsim_core::values::LayerMask;

// ---------------------------------------------------------------------------
// Mask construction helpers (public-API-only, live in this test file so the
// primitives are visible to reviewers alongside the proptest).
// ---------------------------------------------------------------------------

fn solid_mask(w: u32, h: u32, voxel: f32) -> LayerMask {
    LayerMask::new_all_solid(w, h, voxel).expect("valid all-solid mask")
}

fn void_mask(w: u32, h: u32, voxel: f32) -> LayerMask {
    LayerMask::new(w, h, voxel).expect("valid all-void mask")
}

/// 4-connected ring: outer border solid, 1-cell-wide frame, interior void.
/// For n=7 → outer frame solid, (1..6)×(1..6) = 25-cell interior void.
fn ring_mask(n: u32, voxel: f32) -> LayerMask {
    let mut m = solid_mask(n, n, voxel);
    for x in 1..n - 1 {
        for y in 1..n - 1 {
            m.clear(x, y).expect("interior in bounds");
        }
    }
    m
}

// --- Primitive: sealed_cube(n) ---
//
// Solid mask at layers 0 and n-1; ring mask in between. Interior void is
// fully enclosed on all sides. Expected: 1 event at layer n-1.
fn sealed_cube(n: u32, voxel: f32) -> Vec<LayerMask> {
    let mut stack = Vec::with_capacity(n as usize);
    stack.push(solid_mask(n, n, voxel));
    for _ in 1..n - 1 {
        stack.push(ring_mask(n, voxel));
    }
    stack.push(solid_mask(n, n, voxel));
    stack
}

// --- Primitive: open_tube(n) ---
//
// Solid mask at layer 0; ring mask layers 1..n-1; no closure. Expected: 0
// events — pocket never closes from below.
fn open_tube(n: u32, voxel: f32) -> Vec<LayerMask> {
    let mut stack = Vec::with_capacity(n as usize);
    stack.push(solid_mask(n, n, voxel));
    for _ in 1..n {
        stack.push(ring_mask(n, voxel));
    }
    stack
}

// --- Primitive: stacked_cups(n, k) ---
//
// k sealed cubes (each n layers tall, n×n cells) stacked along z with solid
// separators. Expected: k events.
fn stacked_cups(n: u32, k: u32, voxel: f32) -> Vec<LayerMask> {
    let mut stack = Vec::new();
    for _ in 0..k {
        let cube = sealed_cube(n, voxel);
        stack.extend(cube);
        // No explicit separator needed — each sealed_cube already has a solid
        // cap at its last layer, and the next cube's first layer is solid too,
        // so the boundary is two solid layers. That keeps cavities disjoint.
    }
    stack
}

// ---------------------------------------------------------------------------
// Primitive internal-consistency tests (resolves v4 MEDIUM — primitives
// vetted before being used in the proptest).
// ---------------------------------------------------------------------------

#[test]
fn sealed_cube_primitive_is_internally_consistent() {
    let stack = sealed_cube(7, 1.0);
    assert_eq!(stack.len(), 7);
    // Layer 0 and 6 are fully solid
    assert_eq!(stack[0].solid_cell_count(), 49);
    assert_eq!(stack[6].solid_cell_count(), 49);
    // Layers 1..6 are rings (outer frame solid, 5×5 interior void)
    for (i, layer) in stack.iter().enumerate().take(6).skip(1) {
        assert_eq!(
            layer.solid_cell_count(),
            49 - 25,
            "layer {i} ring wall count"
        );
    }
    // Detector sees exactly one event at closure layer 6
    let events = CavityDetector::detect(&stack, DEFAULT_VACUUM_PRESSURE_KPA).expect("valid primitive");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].layer, 6);
    assert!((events[0].sealed_area_mm2 - 25.0).abs() < 1e-6);
}

#[test]
fn open_tube_primitive_is_internally_consistent() {
    let stack = open_tube(7, 1.0);
    assert_eq!(stack.len(), 7);
    assert_eq!(stack[0].solid_cell_count(), 49); // solid base
    for (i, layer) in stack.iter().enumerate().take(7).skip(1) {
        assert_eq!(layer.solid_cell_count(), 24, "layer {i} ring");
    }
    // No closure → no events
    let events = CavityDetector::detect(&stack, DEFAULT_VACUUM_PRESSURE_KPA).expect("valid primitive");
    assert!(events.is_empty());
}

#[test]
fn stacked_cups_primitive_is_internally_consistent() {
    let k = 3;
    let stack = stacked_cups(5, k, 1.0);
    // k cubes × 5 layers each = 15 total
    assert_eq!(stack.len(), 15);
    let events = CavityDetector::detect(&stack, DEFAULT_VACUUM_PRESSURE_KPA).expect("valid primitive");
    assert_eq!(events.len(), k as usize, "got {events:?}");
}

// ---------------------------------------------------------------------------
// Scenarios (a)–(f) from plan v6 Step 1
// ---------------------------------------------------------------------------

#[test]
fn raft_plus_columns_no_suction() {
    // The lilith-torso reproduction scaled down: 11×11 bed, 1mm voxel.
    // Raft (fully solid) → 5 layers of discrete support columns with
    // inter-column gaps touching the bbox edges.
    let raft = solid_mask(11, 11, 1.0);
    let mut columns = void_mask(11, 11, 1.0);
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
    let events = CavityDetector::detect(&stack, DEFAULT_VACUUM_PRESSURE_KPA).expect("valid");
    assert!(
        events.is_empty(),
        "raft+columns false-positive reproduction must emit zero: {events:?}"
    );
}

#[test]
fn closed_cup_emits_one_event() {
    let events =
        CavityDetector::detect(&sealed_cube(7, 1.0), DEFAULT_VACUUM_PRESSURE_KPA).expect("valid");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].layer, 6);
    assert!((events[0].sealed_area_mm2 - 25.0).abs() < 1e-6);
    // 50 kPa × 25 mm² × 1e-3 = 1.25 N
    assert!((events[0].suction_force_n - 1.25).abs() < 1e-3);
}

#[test]
fn open_topped_cup_no_events() {
    let events =
        CavityDetector::detect(&open_tube(7, 1.0), DEFAULT_VACUUM_PRESSURE_KPA).expect("valid");
    assert!(events.is_empty());
}

#[test]
fn fully_sealed_interior_pocket() {
    // Same as sealed_cube — the "floor" at layer 0 acts as the FEP-side
    // seal from the detector's perspective since we emit at the layer that
    // closes the pocket from below.
    let events =
        CavityDetector::detect(&sealed_cube(5, 1.0), DEFAULT_VACUUM_PRESSURE_KPA).expect("valid");
    assert_eq!(events.len(), 1);
}

#[test]
fn two_disjoint_cups_two_events() {
    let events = CavityDetector::detect(&stacked_cups(4, 2, 1.0), DEFAULT_VACUUM_PRESSURE_KPA)
        .expect("valid");
    assert_eq!(events.len(), 2);
    // Events should be at successive closure layers (3 and 7 for 4-layer cubes)
    let layers: Vec<u32> = events.iter().map(|e| e.layer).collect();
    assert_eq!(layers, vec![3, 7]);
}

#[test]
fn lateral_touching_void_is_exterior() {
    // Cup-like shape but interior void reaches the bbox edge via a gap in the wall.
    let mut stack = Vec::new();
    stack.push(solid_mask(7, 7, 1.0)); // floor
    for _ in 0..3 {
        let mut m = ring_mask(7, 1.0);
        m.clear(3, 0).expect("cell on lateral edge"); // knock out a wall cell on the edge
        stack.push(m);
    }
    stack.push(solid_mask(7, 7, 1.0)); // cap
    let events = CavityDetector::detect(&stack, DEFAULT_VACUUM_PRESSURE_KPA).expect("valid");
    assert!(
        events.is_empty(),
        "lateral-edge-touching void must not emit: {events:?}"
    );
}

// ---------------------------------------------------------------------------
// CavityDetector preconditions (resolves v5 MEDIUM)
// ---------------------------------------------------------------------------

#[test]
fn detect_rejects_empty_input() {
    assert!(matches!(
        CavityDetector::detect(&[], DEFAULT_VACUUM_PRESSURE_KPA),
        Err(CavityError::NoMasks)
    ));
}

#[test]
fn detect_rejects_mixed_voxel_sizes() {
    let a = LayerMask::new(4, 4, 0.5).expect("valid");
    let b = LayerMask::new(4, 4, 1.0).expect("valid");
    assert!(matches!(
        CavityDetector::detect(&[a, b], DEFAULT_VACUUM_PRESSURE_KPA),
        Err(CavityError::InconsistentMasks { .. })
    ));
}

#[test]
fn detect_rejects_mismatched_dimensions() {
    let a = LayerMask::new(4, 4, 0.5).expect("valid");
    let b = LayerMask::new(5, 4, 0.5).expect("valid");
    assert!(matches!(
        CavityDetector::detect(&[a, b], DEFAULT_VACUUM_PRESSURE_KPA),
        Err(CavityError::InconsistentMasks { .. })
    ));
}

// ---------------------------------------------------------------------------
// Proptest: known-topology primitives (plan v6 Step 1 scenario (g))
// ---------------------------------------------------------------------------

proptest! {
    /// For each primitive, the event count matches the analytical expectation.
    /// Primitives are internally vetted by the dedicated consistency tests
    /// above — so a failure here is a genuine detector bug, not a fixture bug.
    #[test]
    fn proptest_known_topology_primitives(n in 5u32..12, k in 1u32..5) {
        // sealed_cube(n): expected 1 event
        let events = CavityDetector::detect(&sealed_cube(n, 1.0), DEFAULT_VACUUM_PRESSURE_KPA)
            .expect("sealed_cube(valid n) produces valid masks");
        prop_assert_eq!(events.len(), 1, "sealed_cube({}) expected 1 event", n);

        // open_tube(n): expected 0 events
        let events = CavityDetector::detect(&open_tube(n, 1.0), DEFAULT_VACUUM_PRESSURE_KPA)
            .expect("open_tube(valid n) produces valid masks");
        prop_assert_eq!(events.len(), 0, "open_tube({}) expected 0 events", n);

        // stacked_cups(n, k): expected k events
        let events = CavityDetector::detect(&stacked_cups(n, k, 1.0), DEFAULT_VACUUM_PRESSURE_KPA)
            .expect("stacked_cups(valid n, k) produces valid masks");
        prop_assert_eq!(
            events.len(),
            k as usize,
            "stacked_cups({}, {}) expected {} events",
            n,
            k,
            k
        );
    }
}

// ---------------------------------------------------------------------------
// Proptest: swiss-cheese cube — randomly place N sealed cavities inside a
// solid cube, assert the detector finds exactly N events.
//
// Construction (keeps each cavity sealed + disjoint):
// - Start with a solid cube (w×w cells × depth layers).
// - For each cavity: pick a random 3×3 XY footprint strictly inside the
//   cube (not touching lateral edge), and a random layer range [z..z+len]
//   strictly inside [1..depth-2] so layer 0 and layer depth-1 stay solid.
// - For each chosen (x, y, z) in the cavity footprint × layer range, clear
//   the cell in that layer's mask.
// - Reject generated candidates whose cavity footprints would touch or
//   overlap — keeps each cavity topologically distinct.
//
// Cavity sizes and placements are bounded so that for any valid sample:
// - Every cavity is wholly enclosed by surrounding solid (floor at z-1,
//   cap at z+len, walls on all 4 lateral sides within the same layer).
// - No cavity touches the lateral bbox edge.
// - No two cavities share cells.
//
// Expected: events.len() == N.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct CavitySpec {
    // XY position of the 3×3 cavity footprint, top-left corner.
    x0: u32,
    y0: u32,
    // Layer range [z0..z0 + depth) where cavity cells are void.
    z0: u32,
    depth: u32,
}

fn disjoint_cavity_specs(
    bed_w: u32,
    bed_h: u32,
    stack_depth: u32,
    proposed: &[CavitySpec],
) -> Vec<CavitySpec> {
    // Filter: (1) all cavity cells must stay strictly inside the bbox
    //         (floor layer 0 solid, cap layer stack_depth-1 solid, no XY
    //         edge). (2) topological separation — two cavities share a
    //         pocket iff they are connected via any 6-neighbour void path
    //         across the 3D grid. To keep them distinct we require EITHER
    //         a z-gap of ≥1 solid layer between them, OR (for layers
    //         within 1 of each other) a 1-cell XY gap.
    let mut accepted: Vec<CavitySpec> = Vec::new();
    for c in proposed {
        // Spatial bounds check
        if c.x0 == 0 || c.x0 + 3 >= bed_w {
            continue;
        }
        if c.y0 == 0 || c.y0 + 3 >= bed_h {
            continue;
        }
        if c.z0 == 0 || c.z0 + c.depth >= stack_depth {
            continue;
        }
        // Topological separation: treat z-ranges as inflated by 1 on each
        // side so that z-adjacent cavities (no solid separator) are still
        // considered "overlapping" for the purposes of requiring XY clearance.
        let overlaps = accepted.iter().any(|a| {
            let z_adjacent_or_overlap = c.z0 < a.z0 + a.depth + 1 && a.z0 < c.z0 + c.depth + 1;
            if !z_adjacent_or_overlap {
                return false;
            }
            let x_clear = c.x0 + 3 < a.x0 || a.x0 + 3 < c.x0;
            let y_clear = c.y0 + 3 < a.y0 || a.y0 + 3 < c.y0;
            !(x_clear || y_clear)
        });
        if overlaps {
            continue;
        }
        accepted.push(c.clone());
    }
    accepted
}

fn build_swiss_cheese(bed: u32, depth: u32, voxel: f32, specs: &[CavitySpec]) -> Vec<LayerMask> {
    let mut stack: Vec<LayerMask> = (0..depth).map(|_| solid_mask(bed, bed, voxel)).collect();
    for spec in specs {
        for dz in 0..spec.depth {
            let layer_idx = (spec.z0 + dz) as usize;
            let m = &mut stack[layer_idx];
            for dx in 0..3 {
                for dy in 0..3 {
                    let _ = m.clear(spec.x0 + dx, spec.y0 + dy);
                }
            }
        }
    }
    stack
}

proptest! {
    // Small-scale proptest — fast inner loop for rapid iteration + shrinking.
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    /// Randomly drop up to 4 disjoint 3×3 sealed cavities inside a 12×12×10
    /// solid cube. For any subset that survives the disjointness/bounds
    /// filter, the detector must find exactly that many events.
    #[test]
    fn proptest_swiss_cheese_small_cube(
        raw in prop::collection::vec(
            (1u32..9, 1u32..9, 1u32..8, 1u32..4),  // (x0, y0, z0, depth)
            0..4usize,
        )
    ) {
        let bed = 12u32;
        let depth = 10u32;
        let voxel = 1.0_f32;
        let proposed: Vec<CavitySpec> = raw
            .iter()
            .map(|&(x0, y0, z0, d)| CavitySpec {
                x0,
                y0,
                z0,
                depth: d,
            })
            .collect();
        let accepted = disjoint_cavity_specs(bed, bed, depth, &proposed);

        let stack = build_swiss_cheese(bed, depth, voxel, &accepted);
        let events = CavityDetector::detect(&stack, DEFAULT_VACUUM_PRESSURE_KPA).expect("valid swiss-cheese cube");

        prop_assert_eq!(
            events.len(),
            accepted.len(),
            "expected {} events for {} disjoint cavities, got {:?}",
            accepted.len(),
            accepted.len(),
            events
        );

        for e in &events {
            prop_assert!((e.sealed_area_mm2 - 9.0).abs() < 1e-6);
        }
    }
}

proptest! {
    // Large-scale proptest — exercises the detector at build-plate
    // dimensions (Elegoo Mars 5 Ultra: 218.88×122.88mm). At 1mm voxel:
    // 219×123 cells per layer × up to 120 layers ≈ 3.2M voxels. Capped at
    // 32 cases to keep total wall-clock under ~30s on developer hardware.
    #![proptest_config(ProptestConfig {
        cases: 32,
        max_shrink_iters: 64,
        ..ProptestConfig::default()
    })]

    /// Randomly drop up to 10 disjoint sealed cavities inside a cube sized
    /// to Mars 5 Ultra's build volume at 1mm voxel resolution. Cavities are
    /// 5×5 footprints with 2-8 layer depth. Exercises the detector's
    /// scaling properties and the union-find correctness under many
    /// simultaneous pockets.
    #[test]
    fn proptest_swiss_cheese_buildplate_scale(
        raw in prop::collection::vec(
            (1u32..215, 1u32..119, 1u32..115, 2u32..9),  // (x0, y0, z0, depth)
            0..10usize,
        )
    ) {
        let bed_w = 219u32;  // Mars 5 Ultra ≈ 218.88mm
        let bed_h = 123u32;  // Mars 5 Ultra ≈ 122.88mm
        let depth = 120u32;  // 60mm tall at 0.5mm layer height
        let voxel = 1.0_f32;

        let proposed: Vec<CavitySpec> = raw
            .iter()
            .map(|&(x0, y0, z0, d)| CavitySpec {
                x0,
                y0,
                z0,
                depth: d,
            })
            .collect();
        let accepted = disjoint_cavity_specs_of_size(bed_w, bed_h, depth, &proposed, 5);

        let stack = build_swiss_cheese_of_size(bed_w, bed_h, depth, voxel, &accepted, 5);
        let events = CavityDetector::detect(&stack, DEFAULT_VACUUM_PRESSURE_KPA).expect("valid build-plate stack");

        prop_assert_eq!(
            events.len(),
            accepted.len(),
            "expected {} events on build-plate-scale cube, got {:?}",
            accepted.len(),
            events
        );

        // Each 5×5 cavity at 1mm voxel → 25 mm² sealed area.
        for e in &events {
            prop_assert!(
                (e.sealed_area_mm2 - 25.0).abs() < 1e-6,
                "sealed area {} mm² should be 25 mm²",
                e.sealed_area_mm2
            );
        }
    }
}

/// Generalised version of `disjoint_cavity_specs` with configurable cavity
/// footprint side (in cells). Used by the large-scale proptest.
fn disjoint_cavity_specs_of_size(
    bed_w: u32,
    bed_h: u32,
    stack_depth: u32,
    proposed: &[CavitySpec],
    footprint: u32,
) -> Vec<CavitySpec> {
    let mut accepted: Vec<CavitySpec> = Vec::new();
    for c in proposed {
        if c.x0 == 0 || c.x0 + footprint >= bed_w {
            continue;
        }
        if c.y0 == 0 || c.y0 + footprint >= bed_h {
            continue;
        }
        if c.z0 == 0 || c.z0 + c.depth >= stack_depth {
            continue;
        }
        let overlaps = accepted.iter().any(|a| {
            let z_adjacent_or_overlap = c.z0 < a.z0 + a.depth + 1 && a.z0 < c.z0 + c.depth + 1;
            if !z_adjacent_or_overlap {
                return false;
            }
            let x_clear = c.x0 + footprint < a.x0 || a.x0 + footprint < c.x0;
            let y_clear = c.y0 + footprint < a.y0 || a.y0 + footprint < c.y0;
            !(x_clear || y_clear)
        });
        if overlaps {
            continue;
        }
        accepted.push(c.clone());
    }
    accepted
}

/// Generalised version of `build_swiss_cheese` with configurable cavity
/// footprint side.
fn build_swiss_cheese_of_size(
    bed_w: u32,
    bed_h: u32,
    depth: u32,
    voxel: f32,
    specs: &[CavitySpec],
    footprint: u32,
) -> Vec<LayerMask> {
    let mut stack: Vec<LayerMask> = (0..depth)
        .map(|_| solid_mask(bed_w, bed_h, voxel))
        .collect();
    for spec in specs {
        for dz in 0..spec.depth {
            let layer_idx = (spec.z0 + dz) as usize;
            let m = &mut stack[layer_idx];
            for dx in 0..footprint {
                for dy in 0..footprint {
                    let _ = m.clear(spec.x0 + dx, spec.y0 + dy);
                }
            }
        }
    }
    stack
}
