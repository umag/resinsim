---
issue: t2f1-voxelized-cure-distribution
date: 2026-05-19
---

# UAT: Voxel cure field and photoinitiator depletion

**ADR-0017 / KB-160 note.** These scenarios verify the t2f1 voxel-mode
end-to-end behaviour: presence of `--voxel-cure-mm` switches the
simulation from Tier-1 scalar Beer-Lambert to a 3D voxel-resolved
`CureField` + `PhotoinitiatorField` pair, with KB-160 standard-radical
depletion kinetics. Tier-1 numerics are unchanged when the flag is absent.

## UAT-1: --voxel-cure-mm populates voxel fields on the aggregate

**Rationale.** The voxel mode only matters if its output reaches
downstream consumers. The simulation aggregate must carry the populated
`CureField` and `PhotoinitiatorField` so that viz heatmaps, per-voxel
inspectors, and report generators can read them.

```gherkin
Scenario: --voxel-cure-mm populates voxel fields on the aggregate
  Given a CTB input with per-layer masks
  And a resin and printer profile validated against the recipe
  When the simulation runs with the --voxel-cure-mm flag set to a positive finite value
  Then the produced sim.json's aggregate carries a populated cure_field
  And the aggregate carries a populated photoinitiator_field
  And both fields share identical (nx, ny, nz) dimensions
  And nz equals the layer count of the input
```

## UAT-2: Absence of --voxel-cure-mm preserves Tier-1 numerics

**Rationale.** Backward compatibility for the existing default path. A
run WITHOUT the flag must produce byte-equivalent simulation outputs
relative to the Tier-1 path (modulo new optional fields that serialise as
absent when `None`).

```gherkin
Scenario: Tier-1 mode does not install voxel fields
  Given a CTB input with per-layer masks
  And a resin and printer profile validated against the recipe
  When the simulation runs without the --voxel-cure-mm flag
  Then the produced sim.json's aggregate cure_field is absent
  And the aggregate photoinitiator_field is absent
  And each layer's cure_depth_um value matches the Tier-1 CureCalculator::cure_depth_at_temp scalar
```

## UAT-3: Photoinitiator depletes monotonically along a column

**Rationale.** KB-160's load-bearing physics claim: cumulative
absorbed dose drives concentration downward, never upward. Recombination
chemistry is not modelled. The simulator must honour this across a multi-
layer print.

```gherkin
Scenario: Repeated exposures of the same voxel column drive photoinitiator down monotonically
  Given a CTB with N consecutive layers each marking the same pixel column as solid
  When the simulation runs with --voxel-cure-mm set
  Then the deepest voxel's photoinitiator concentration is less than or equal to the topmost voxel's
  And no voxel's concentration is below zero
  And no voxel's concentration is above the resin's photoinitiator_concentration_initial
```

## UAT-4: --voxel-cure-mm 0 or negative is rejected at parse time

**Rationale.** The CLI value_parser must reject pathological inputs
before reaching profile validation, so users see the error attached to
the flag name (mirroring `--ambient` / `--initial-led-temp` precedent).

```gherkin
Scenario: Zero or negative --voxel-cure-mm rejected at parse
  Given a resinsim binary built with the field-sim Cargo feature
  When the user invokes "resinsim sim --voxel-cure-mm 0 ..."
  Then the CLI errors with a message referencing --voxel-cure-mm by name
  And the message describes the constraint "must be finite and positive"
  And the simulation does not begin
```

## UAT-5: Layer cache reflects voxel field summary

**Rationale.** Legacy callers reading `layer.cure_depth_um` directly
(many cross-crate readers in resinsim-viz + tests) must transparently
see voxel-derived values when the simulation was run in voxel mode.
The SimulationRunner promotes `LayerSummary.mean` and `LayerSummary.min`
into the cache fields so direct-access paths stay correct without per-
site dispatch-method rewrites.

```gherkin
Scenario: Voxel-mode cache reflects per-layer voxel summary
  Given a sim.json produced with --voxel-cure-mm
  When a downstream consumer reads layer.cure_depth_um directly
  Then the value equals the LayerSummary.mean of the cure_field's Z-slab at that layer
  And the value of layer.worst_cure_depth_um equals the LayerSummary.min
  And the LayerResult::cure_depth_um_summary dispatch method returns the same value as the cache
```

## UAT-6: apply_column_exposure ↔ compute_column_exposure + manual deposit parity

**Rationale.** ADR-0018 / t2f2 refactored `VoxelCureCalculator` to
extract `compute_column_exposure` as a pure functional sibling of
`apply_column_exposure`. The in-place form became a thin wrapper that
reads the PI column via `column_at`, calls the pure compute, and
applies the resulting dose column via `add_dose` + `deplete`. The
two forms MUST remain bit-exact equivalent — this is the load-bearing
invariant for the t2f2 crosstalk path which uses the pure compute form
to post-process dose columns through a 1D Z convolution before deposit.

Coverage: `parity_apply_vs_compute_proptest` (50 randomised cases,
fixture cap 8×8×10) asserts byte-identical CureField + PhotoinitiatorField
output between the two forms. Promoting to UAT here documents the
invariant at a level that survives module reorganisation.

```gherkin
Scenario: VoxelCureCalculator apply_column_exposure equals compute_column_exposure + manual deposit
  Given any valid (pi_field, cure_field, ix, iy, iz_top, intensity, exposure_sec, dp, k_d, layer_height_um)
  When apply_column_exposure is invoked in-place on a cloned (cure, pi)
  And compute_column_exposure is invoked on a snapshot of pi, producing a dose column
  And the dose column is applied manually via cure.add_dose + pi.deplete for each in-bounds iz
  Then both result fields are bit-exact f32 equal at every voxel
```

## See also

- `docs/adr/0017-voxel-cure-field-and-photoinitiator-depletion.md` —
  design decisions captured during planning.
- `docs/adr/0018-light-crosstalk-3d-gaussian-convolution.md` —
  the t2f2 ADR that motivated the `compute_column_exposure` refactor.
- `docs/patterns/bit-exact-parity-proptest-for-pure-wrapper-refactors.md`
  — the general refactor-gating pattern.
- `docs/kb/KB-160-photoinitiator-depletion-model.md` — depletion physics +
  ±50 % uncertainty band for the default decay constant.
- `crates/resinsim-core/tests/voxel_cure_integration.rs` — 5 end-to-end
  tests covering UAT-1 / UAT-2 / UAT-3 / UAT-5. UAT-4 covered by
  `resinsim-inspect`'s `parse_voxel_cure_mm` unit tests. UAT-6
  covered by `parity_apply_vs_compute_proptest` in
  `crates/resinsim-core/src/services/voxel_cure_calculator.rs::tests`.
