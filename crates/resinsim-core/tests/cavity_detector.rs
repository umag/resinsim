//! TDD red tests for `CavityDetector` and integration with `SuctionDetector`.
//!
//! Per plan v6 Step 1 (suction-detector-raft-false-positive issue lifecycle).
//!
//! **State: red.** Tests are gated with `#[ignore]` and bodies are `todo!()` until
//! the types they exercise exist. As each subsequent plan step lands, bodies
//! are filled in:
//!
//! - Step 2 introduces `LayerMask`, `LayerGeometry` — mask-construction helpers become available.
//! - Step 6 introduces `CavityDetector`, `CavityEvent`, `CavityError` — assertions become available.
//! - Step 7 removes the `#[ignore]` gates and verifies tests go green.
//!
//! Reference: `/Users/mag1/dev_tmp/ora/resinsim/docs/patterns/phase-boundaries-for-ddd-refactors.md`

// ---------------------------------------------------------------------------
// Scenario (a): raft_plus_columns_no_suction
// ---------------------------------------------------------------------------

/// Synthetic mask stack: raft (solid plate) layers 0-22, discrete support
/// columns layers 23+. `CavityDetector` must return zero events — the gaps
/// between columns touch the lateral bbox edge and drain to the vat.
///
/// This is the exact false-positive reproduction from triage: the raft→supports
/// transition that the old area-drop heuristic flagged with 99 N of fabricated
/// suction force. The 3D topology correctly identifies the inter-column void
/// as exterior-connected.
#[test]
#[ignore = "awaiting LayerMask (Step 2) + CavityDetector (Step 6)"]
fn raft_plus_columns_no_suction() {
    todo!(
        "Construct mask stack: layers 0-22 fully-solid plate (simulating raft); \
         layers 23-49 with 20 discrete 3x3 solid squares (support columns) spread \
         across the bbox with inter-column gaps touching the bbox lateral edges. \
         Invoke CavityDetector::detect(&masks) and assert the Ok(Vec<CavityEvent>) \
         returned contains zero events."
    );
}

// ---------------------------------------------------------------------------
// Scenario (b): closed_cup_emits_one_event
// ---------------------------------------------------------------------------

/// Solid base, ring walls, solid top. Exactly one event at the layer sealing
/// the cavity from below (FEP direction).
#[test]
#[ignore = "awaiting LayerMask (Step 2) + CavityDetector (Step 6)"]
fn closed_cup_emits_one_event() {
    todo!(
        "Construct mask stack: layer 0 solid square (base); layers 1-5 ring mask \
         (solid border, void interior); layer 6 solid square (top, seals cavity). \
         Assert exactly one CavityEvent at layer 6 with sealed_area_mm2 matching \
         the interior hole area within rounding tolerance."
    );
}

// ---------------------------------------------------------------------------
// Scenario (c): open_topped_cup_no_events
// ---------------------------------------------------------------------------

/// Solid base, ring walls, no closure. Zero events — wall peel is ring-peel
/// without concentrated suction.
#[test]
#[ignore = "awaiting LayerMask (Step 2) + CavityDetector (Step 6)"]
fn open_topped_cup_no_events() {
    todo!(
        "Construct mask stack: layer 0 solid (base); layers 1-10 ring mask; no \
         solid closure. Assert CavityDetector returns zero events (pocket remains \
         open at last layer → 'open at FEP' → no vacuum)."
    );
}

// ---------------------------------------------------------------------------
// Scenario (d): fully_sealed_interior_pocket
// ---------------------------------------------------------------------------

/// Solid blob with a hollow core entirely inside it. One event at the
/// bottom-sealing layer.
#[test]
#[ignore = "awaiting LayerMask (Step 2) + CavityDetector (Step 6)"]
fn fully_sealed_interior_pocket() {
    todo!(
        "Construct mask stack: layer 0 solid (roof); layers 1-5 ring mask \
         (walls); layer 6 solid (floor — seals from FEP). All outer bounds solid \
         so lateral exterior is not reached. Assert one CavityEvent at layer 6."
    );
}

// ---------------------------------------------------------------------------
// Scenario (e): two_disjoint_cups_two_events
// ---------------------------------------------------------------------------

/// Two closed cups side by side. Two separate events at respective closure layers.
#[test]
#[ignore = "awaiting LayerMask (Step 2) + CavityDetector (Step 6)"]
fn two_disjoint_cups_two_events() {
    todo!(
        "Construct mask stack containing two spatially-separated closed cups \
         (left cup and right cup, each with solid-base → ring-walls → solid-top). \
         Assert exactly two CavityEvents, one per cup, at their respective closure \
         layers."
    );
}

// ---------------------------------------------------------------------------
// Scenario (f): lateral_touching_void_is_exterior
// ---------------------------------------------------------------------------

/// A void that extends to the bbox edge never emits — it is exterior-connected
/// through the lateral vat.
#[test]
#[ignore = "awaiting LayerMask (Step 2) + CavityDetector (Step 6)"]
fn lateral_touching_void_is_exterior() {
    todo!(
        "Construct mask stack where the void region extends to at least one \
         lateral bbox face (x=0 OR x=max-1 OR y=0 OR y=max-1) across all layers. \
         Even if 'closed' by a top layer, the lateral edge makes it exterior. \
         Assert CavityDetector returns zero events."
    );
}

// ---------------------------------------------------------------------------
// Scenario (g): proptest_known_topology_primitives
// ---------------------------------------------------------------------------

/// Proptest: construct mask stacks from primitives where the sealed-component
/// count is analytically known from construction. Event count must equal the
/// constructed count.
///
/// Primitives (per plan v6 Step 1):
/// - `sealed_cube(n)`: solid mask at layers 0 and n-1; ring mask layers 1..n-2.
///   Expected 1 event at layer n-1.
/// - `open_tube(n)`: solid layer 0; ring layers 1..n-1; no closure. Expected 0.
/// - `nested_rings(n, k)`: k concentric sealed cubes. Expected k events.
/// - `stacked_cups(n, k)`: k sealed cubes along z with solid separators. Expected k events.
///
/// Each primitive has its own unit test asserting internal consistency
/// (volume/area/layer-span) BEFORE it is used in this proptest.
#[test]
#[ignore = "awaiting LayerMask (Step 2) + CavityDetector (Step 6) + primitives"]
fn proptest_known_topology_primitives() {
    todo!(
        "Use proptest::proptest! to generate arbitrary (kind, n, k) values, \
         construct the corresponding primitive mask stack, invoke \
         CavityDetector::detect, and assert event count equals the analytically \
         known count for that primitive."
    );
}

// ---------------------------------------------------------------------------
// Internal-consistency unit tests for each primitive
// ---------------------------------------------------------------------------
//
// These guard the primitives themselves against silent miscalibration.
// If a primitive is constructed wrong, these tests flag it — not the proptest.

#[test]
#[ignore = "awaiting LayerMask (Step 2) + primitives"]
fn sealed_cube_primitive_is_internally_consistent() {
    todo!(
        "Construct sealed_cube(n=10), assert: layer count = 10, layer 0 is fully \
         solid, layer 9 is fully solid, layers 1..8 are rings (non-zero wall \
         area, non-zero hole area), total void volume equals hole_area × 8 layers."
    );
}

#[test]
#[ignore = "awaiting LayerMask (Step 2) + primitives"]
fn open_tube_primitive_is_internally_consistent() {
    todo!(
        "Construct open_tube(n=10), assert: layer count = 10, layer 0 is fully \
         solid (base), layers 1..9 are rings, last layer is a ring (not closed)."
    );
}

#[test]
#[ignore = "awaiting LayerMask (Step 2) + primitives"]
fn nested_rings_primitive_is_internally_consistent() {
    todo!(
        "Construct nested_rings(n=10, k=3), assert: each ring has its own \
         concentric void, void count = 3, voids are disjoint."
    );
}

#[test]
#[ignore = "awaiting LayerMask (Step 2) + primitives"]
fn stacked_cups_primitive_is_internally_consistent() {
    todo!(
        "Construct stacked_cups(n=5, k=3), assert: total layer count ≈ 3*5 + 2 \
         separators, 3 disjoint sealed voids along z."
    );
}

// ---------------------------------------------------------------------------
// CavityDetector precondition test (resolves v5 MEDIUM)
// ---------------------------------------------------------------------------

/// `CavityDetector::detect` must reject mixed-resolution inputs with
/// `Err(CavityError::InconsistentMasks)`. Prevents silent wrong results.
#[test]
#[ignore = "awaiting LayerMask (Step 2) + CavityDetector (Step 6) + CavityError"]
fn detect_rejects_mixed_voxel_sizes() {
    todo!(
        "Construct two LayerMasks with different voxel_size_mm (e.g. 0.5 vs 1.0). \
         Invoke CavityDetector::detect(&[mask_a, mask_b]) and assert it returns \
         Err(CavityError::InconsistentMasks)."
    );
}

#[test]
#[ignore = "awaiting LayerMask (Step 2) + CavityDetector (Step 6) + CavityError"]
fn detect_rejects_mismatched_dimensions() {
    todo!(
        "Construct two LayerMasks with same voxel_size_mm but different \
         (width_cells, height_cells). Invoke CavityDetector::detect and assert \
         Err(CavityError::InconsistentMasks)."
    );
}
