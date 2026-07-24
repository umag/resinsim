---
issue: peel-corrections-s3-perimeter-shape
date: 2026-07-24
---

# UAT: A/L peel shape factor scales force with aspect ratio (ADR-0022 Stage 3)

## Rationale

ADR-0022 Stage 3 adds a dimensionless area/perimeter (A/L) shape factor
(KB-185 Tier-1) that modulates the peel term so force reflects a layer's
*compactness*, not just its area (Pan Fig.9: at equal 314 mm², a compact
cylinder separates at 6.16 N vs a thin star at 4.9 N). The factor is
square-anchored (=1 for a square), reduction-only, opt-in per resin via
`peel_shape_factor_strength`, and applied to the peel term ONLY. These scenarios
guard that (1) the factor reduces force for thin shapes and is 1.0 for compact
ones, (2) it is behaviour-preserving when the strength is unset, (3) synthetic
placeholder masks never apply a spurious reduction, and (4) it modulates peel
without disturbing suction or base adhesion. Magnitude is indicative pending an
equal-area shape sweep (E2b); the shipped `generic_standard` value is 0.5.

## UAT-1: a thin cross-section peels at lower force than a compact one

```gherkin
Scenario: UAT-1 the A/L shape factor ranks thin below compact at equal area
  Given a resin whose peel_shape_factor_strength is 1.0
  And two equal-area layer masks — a compact square block and a thin 1×N line
  When the per-layer shape factors are computed from the masks
  Then the compact block's factor is 1.0 (square = the KB-181 baseline)
  And the thin line's factor is strictly between 0 and 1
  And the compact factor exceeds the thin factor
  # strength=0.5 reproduces the Pan Fig.9 cylinder→star ratio 4.9/6.16 = 0.795
```

## UAT-2: an unset strength is behaviour-preserving

```gherkin
Scenario: UAT-2 a resin without peel_shape_factor_strength applies no correction
  Given a resin whose peel_shape_factor_strength is unset
  When a job is simulated
  Then effective_peel_shape_factor_strength() returns 0.0
  And every layer's peel_shape_factor is None (omitted from sim.json)
  And every peel_force_n is byte-identical to the pre-Stage-3 output
```

## UAT-3: synthetic placeholder (fully-solid) masks never apply a reduction

```gherkin
Scenario: UAT-3 a fully-solid placeholder mask maps to factor 1.0
  Given a resin whose peel_shape_factor_strength is active (e.g. 0.5)
  And a run whose masks are fully-solid placeholders (run_from_areas 1×1,
    or the run_from_layer_inputs W×H all-solid fallback)
  When the per-layer shape factors are computed
  Then every fully-solid mask maps to factor 1.0 (no shape signal)
  And the peel force on those layers is unchanged
  # real geometry leaves void margins → not fully-solid → gets a real factor
```

## UAT-4: the shape factor modulates peel only, not suction or base

```gherkin
Scenario: UAT-4 a non-1.0 shape factor scales peel and leaves suction + base
  Given a layer with a non-zero peel, suction, and base-adhesion force
  When a peel_shape_factor of 0.5 is applied
  Then peel_force_n halves
  And suction_force_n and base_force_n are unchanged
  And total_force_n drops by exactly the peel reduction
```

## Evidence

- `crates/resinsim-core/src/values/layer_mask.rs::tests::{perimeter_single_cell_is_four_sides,perimeter_two_by_two_block,perimeter_rectangle_is_twice_w_plus_h,perimeter_ring_counts_interior_hole,perimeter_disconnected_cells_sum,perimeter_all_void_is_zero,is_fully_solid_*}`
  (perimeter exactness + the fully-solid discriminator).
- `crates/resinsim-core/src/services/peel_force_calculator.rs::tests::{shape_factor_square_is_one,shape_factor_thin_rectangle_below_one,shape_factor_monotonic_in_aspect_ratio,shape_factor_disk_clamps_at_one,shape_factor_strength_half_matches_pan_star_ratio,shape_factor_zero_*}`
  (UAT-1 — the factor formula + Pan ratio).
- `crates/resinsim-core/src/app/simulation_runner.rs::tests::{build_shape_factor_map_off_fully_solid_and_thin,run_from_areas_all_solid_masks_apply_factor_one}`
  (UAT-1/UAT-2/UAT-3 — runner threading + placeholder guard).
- `crates/resinsim-core/src/services/failure_predictor.rs::tests::peel_shape_factor_scales_peel_only`
  (UAT-4 — peel-only isolation).
- `crates/resinsim-core/src/entities/resin_profile.rs::tests::{effective_peel_shape_factor_strength_defaults_to_zero_when_unset,peel_shape_factor_strength_round_trips_through_toml,peel_shape_factor_strength_*_rejected}`
  (UAT-2 — default + validation + TOML round-trip of the shipped value).
- Qualitative: `inspect calibrate` on the 37 MB Athena reference print with
  `generic_standard` strength 0.5 — correlation 0.948→0.954, single-gain
  R² 0.562→0.771, peak layer still 0/0. Indicative (single geometry).
