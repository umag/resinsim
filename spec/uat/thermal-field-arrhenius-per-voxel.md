---
issue: t2f4-thermal-diffusion
date: 2026-05-21
---

# UAT: Tier-2 thermal diffusion changes per-layer cure depth via Ec(T)

## Rationale

ADR-0020 / t2f4 §Decision vii pins the Tier-2 cure dispatch policy:
when the voxel cure path is active (`--voxel-cure-mm`), `Ec(T)` for
each layer is derived from `state.thermal.volume_mean_c()` (the
Tier-2 vat-volume-mean) rather than `ThermalCalculator::vat_temperature_at_layer_v2`
(the Tier-1 lumped scalar). The single-source-arrhenius-helper pattern
is preserved — only the temperature SOURCE changes.

This UAT documents the observable contract: as the diffusion field
warms from cold-start ambient toward the steady-state LED ceiling
over many layers, per-layer `cure_depth_um` MUST drift (since `Ec(T)`
drifts via Arrhenius). At layer 0 the thermal field is essentially
uniform ambient; by layer N for large N it has warmed measurably; the
two layers therefore see different `Ec(T)` and produce different
cure depths.

v1 simplification: ONE `Ec(T)` per layer derived from the vat-volume-
mean temperature. Full per-voxel `Ec(T)` inside the cure column
(true per-voxel Beer-Lambert inversion via `CureField::layer_summary`)
is filed as a t2f4 follow-on. The contract documented here is what
v1 ships.

The unit-level enforcement lives in
`crates/resinsim-core/tests/voxel_cure_thermal_field_integration.rs`
::`voxel_mode_thermal_field_drift_changes_cure_depth_across_layers`,
which runs a 60-layer Mars 5 Ultra voxel-mode simulation and asserts:
the thermal field's volume mean stays bounded between initial ambient
and the steady-state LED ceiling; per-run divergence histogram
(median + max of `|cure_depth_um[N] − cure_depth_um[0]|`) is logged
to stderr; at least one layer's cure depth differs from layer 0
(brittle thresholds avoided — the histogram is the observable).

See also:

- `docs/adr/0020-spatial-thermal-diffusion.md` §Decision vii — the
  dispatch policy.
- `docs/patterns/tier1-as-bc-source-for-tier2.md` — why Tier-1 Stage A
  still drives the bottom Dirichlet BC even though Tier-2 supersedes
  Stage B for downstream consumers.
- `docs/patterns/single-source-arrhenius-helper.md` — `Ec(T)`
  computation must delegate to `CureCalculator::ec_at_temp` (Tier-2
  changes the temperature input, NOT the formula).
- `KB-152-led-vat-thermal-coupling.md` — Tier-1 Stage A LED case
  formulas (still the BC source).
- `KB-153` (inline in ADR-0007) — Arrhenius `Ec(T)` correction.

## UAT-1: ThermalField drift changes per-layer cure depth

```gherkin
Scenario: UAT-1 voxel-mode cure_depth_um diverges across layers as the
          thermal field warms
  Given a Mars 5 Ultra printer profile with all field-sim thermal
        material properties populated
  And the Generic Standard resin (with thermal_conductivity_w_mk,
      specific_heat_j_kgk, convective_top_h_w_m2k set per ADR-0020)
  And a 60-layer 3×3 solid-cylinder CTB fixture
  When `resinsim sim --voxel-cure-mm 0.5 --initial-led-temp 27 \
    --ambient 22 ...` runs to completion
  Then `sim.thermal_field()` is `Some` with vat-envelope dimensions
  And the thermal field's `volume_mean_c()` is ≥ initial ambient (22 °C)
  And the thermal field's `volume_max_c()` is < the steady-state LED
      ceiling + a small slack (≈ 50 °C for Mars 5 Ultra @ 13.5 °C steady-state rise)
  And `sim.layers()[0].cure_depth_um != sim.layers()[N-1].cure_depth_um`
      (some layer differs from layer 0 — Tier-2 dispatch is observable)
  And stderr carries a `tier-2 thermal:` info line at run start AND a
      `tier-2 thermal complete:` summary line at run end
```

## UAT-2: Tier-1 path is unaffected when voxel cure is OFF

```gherkin
Scenario: UAT-2 absent --voxel-cure-mm leaves Tier-1 cure dispatch
          intact
  Given the same printer + resin profiles as UAT-1
  And a multi-layer CTB
  When `resinsim sim ...` runs WITHOUT `--voxel-cure-mm`
  Then `sim.thermal_field()` is `None`
  And `sim.cure_field()` / `sim.strain_field()` / etc. are `None`
  And per-layer `cure_depth_um` derives from the Tier-1 scalar
      `ThermalCalculator::vat_temperature_at_layer_v2` + `Ec(T)`
      Arrhenius compose, unchanged from pre-t2f4 behaviour
  And no `tier-2 thermal:` info line is emitted to stderr
```
