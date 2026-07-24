---
issue: peel-crack-propagation-tier1
date: 2026-07-24
---

# UAT: interlayer crack-front knockdown reduces holding CAPACITY with perimeter (KB-198)

## Rationale

`peel-crack-propagation-tier1` (supersedes the closed `peel-crack-propagation`)
makes the resin↔resin interlayer bond fracture-limited (KB-198 / KB-188): its
holding CAPACITY is knocked down by `min(1, 4√A/P)` for NORMAL layers, so thin /
high-perimeter geometry holds less than the pure-area model assumed and its
over-estimated safety factor is corrected. A `Delamination` warning fires when the
crack-reduced interlayer bond alone can no longer hold the shaped peel load. These
scenarios guard that (1) the knockdown scales the interlayer CAPACITY with real
perimeter and lowers the safety factor, (2) it is CAPACITY-ONLY — the peel/total
LOAD is never touched, (3) bottom-layer plate adhesion and placeholder masks are
never knocked down (behaviour-preserving), and (4) `Delamination` fires when — and
only when — the crack-reduced interlayer bond drops below the peel load. Extends the
S3 perimeter / `is_fully_solid` scenarios and `safety-factor-zero-force`. Magnitude
is indicative pending E2b (equal-area shape sweep + interlayer-fracture measurement).

## UAT-1: a thin normal layer holds less and has a lower safety factor

```gherkin
Scenario: UAT-1 the interlayer knockdown scales capacity with perimeter
  Given a NORMAL layer with a real per-layer perimeter
  And a compact (square) reference and a thin (high-perimeter) variant at equal area
  When each layer is assessed
  Then the compact layer's effective bonded fraction is 1.0 (square = 4√A/P)
  And the thin layer's effective bonded fraction is strictly between 0 and 1
  And the thin layer's interlayer capacity and safety factor are strictly lower
  And the thin layer records a crack_front_fraction Some(>0); the compact records None
```

## UAT-2: the knockdown is CAPACITY-ONLY — the peel/total LOAD is unchanged

```gherkin
Scenario: UAT-2 the crack never touches the separation load
  Given a NORMAL layer simulated with and without a real perimeter
  When the crack knockdown is applied
  Then peel_force_n and total_force_n are byte-identical between the two runs
  And only the safety factor (and any Delamination) changes
  # sim_golden byte-identical; Athena calibrate FORCE metrics unchanged
```

## UAT-3: bottom layers and placeholder masks are never knocked down

```gherkin
Scenario: UAT-3 behaviour-preserving for plate adhesion and placeholders
  Given a bottom layer (below the plate bottom_layer_count) with any perimeter
  And a run whose masks are fully-solid placeholders (run_from_areas 1×1, or the
    run_from_layer_inputs W×H all-solid fallback)
  When the layers are assessed
  Then the bottom-layer plate adhesion is unchanged (no crack)
  And every placeholder-mask layer records crack_front_fraction None (no knockdown)
  And no Delamination is emitted on those layers
```

## UAT-4: Delamination fires iff the crack-reduced interlayer bond < peel load

```gherkin
Scenario: UAT-4 the Delamination gate is the reduced-capacity-vs-load comparison
  Given a NORMAL layer with a crack front present (crack_front_fraction > 0)
  When the crack-reduced interlayer capacity is below the shaped peel load
  Then a Delamination warning is emitted (co-firing with SupportOverload if capacity is short)
  When instead the crack-reduced interlayer capacity still exceeds the peel load
  Then the crack is still recorded but NO Delamination is emitted
```

## Evidence

- `crates/resinsim-core/src/services/crack_propagator.rs::tests::{fraction_square_is_one,fraction_thin_rectangle_below_one_exact,fraction_thin_wall_matches_4_sqrt_a_over_p,fraction_disk_clamps_to_one,fraction_zero_*_is_neutral,fraction_non_finite_input_is_neutral,fraction_monotonic_in_aspect_ratio,fraction_never_exceeds_one,crack_*,bonded_area_*}`
  (UAT-1 — the `min(1,4√A/P)` knockdown formula, degenerate/non-finite guards, monotonicity).
- `crates/resinsim-core/src/values/crack.rs::tests::*` (the CrackFront value object contract).
- `crates/resinsim-core/src/services/support_analyzer.rs::tests::{assess_normal_layer_crack_reduces_interlayer_capacity,assess_normal_layer_crack_lowers_safety_factor,assess_bottom_layer_crack_does_not_reduce_plate_adhesion,assess_support_capacity_invariant_under_crack}`
  (UAT-1/UAT-3 — capacity knockdown, bottom-layer + support-capacity invariance).
- `crates/resinsim-core/src/services/failure_predictor.rs::tests::{thin_normal_layer_records_crack_and_lowers_safety_factor,crack_knockdown_is_capacity_only_peel_and_total_force_unchanged,compact_normal_layer_records_no_crack,bottom_layer_never_records_crack,delamination_fires_when_crack_reduced_interlayer_below_peel,mildly_thin_layer_records_crack_but_does_not_delaminate,compact_normal_layer_does_not_delaminate,delamination_co_fires_with_support_overload,crack_applies_at_first_normal_layer_not_last_bottom_layer}`
  (UAT-1/UAT-2/UAT-4 — capacity-only, the Delamination gate, bottom/normal boundary).
- `crates/resinsim-core/tests/crack_propagation_runner.rs::{thin_wall_normal_layers_record_a_crack_front,thin_wall_bottom_layers_never_record_a_crack,thin_wall_emits_delamination_end_to_end,fully_solid_masks_apply_no_crack_knockdown,run_from_areas_never_records_a_crack}`
  (UAT-2/UAT-3/UAT-4 — end-to-end runner threading, placeholder guard, per-layer independence).
- Regression: the full `sim_golden` / `kb_golden` / `force_comparator_golden` suites stay
  byte-identical (capacity-only), confirming the peel LOAD is untouched.
