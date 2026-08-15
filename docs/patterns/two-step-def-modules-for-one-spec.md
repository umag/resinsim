---
issue: uat-unskip-light-crosstalk-3d-gaussian-convolution
date: 2026-08-15
---

# Pattern: Two step-def modules for one spec via STEP_DEF_MODULE_RENAMES

## Context

A UAT spec covers both ungated scenarios (e.g. validation checks on
`PrinterProfile::validate`, reachable on default features) and field-sim-gated
scenarios (e.g. runtime behaviour through
`SimulationRunner::run_from_layer_inputs_with_voxel`, reachable only under
`#[cfg(feature = "field-sim")]`). One module cannot serve both: gating the
whole module loses the ungated scenarios under default features; leaving it
ungated means the field-sim imports fail to compile under default features.

## Pattern

Create TWO step-def modules for the same spec:

1. The primary module (name matches spec stem, ungated) — covers the
   scenarios whose production entry points are reachable on default features.
2. A secondary module (name = spec stem + `_runtime` or other disambiguator,
   `#[cfg(feature = "field-sim")]` gated) — covers the scenarios whose
   production entry points require `field-sim`.

Wire the secondary module via `STEP_DEF_MODULE_RENAMES` in `uat_gherkin.rs`
to map it back to the same spec name as the primary. The register entry uses
`per_config(default_skips, field_sim_skips)` to declare the config-asymmetric
shape. `assert_asymmetric_rows_have_a_gated_module` is satisfied by the gated
secondary module's presence in `mod.rs`.

## When to use

When a spec's scenarios span both sides of a `#[cfg]` boundary, and the
ungated scenarios are already stepped by an existing module.

## When NOT to use

If ALL scenarios share the same gating, use a single module with the
appropriate gate (or no gate). The two-module pattern adds complexity
(`STEP_DEF_MODULE_RENAMES`, a second `pub mod`, a second `use` entry) that is
only justified when the split is load-bearing.

## Exemplar

`spec/uat/light-crosstalk-3d-gaussian-convolution.md`:
- `light_crosstalk_3d_gaussian_convolution.rs` — UAT-5/6/7 (ungated, Always)
- `light_crosstalk_3d_gaussian_convolution_runtime.rs` — UAT-1/2/3/4/8/9
  (field-sim-gated, FieldSimOnly)

## See also

- `docs/patterns/band-membership-by-symbol.md` — the prerequisite: verify
  the gating boundary by grep before deciding which module goes where.
- `docs/patterns/per-spec-runtime-skip-attribution.md` — the two-column
  register design that carries the asymmetric counts.
