---
issue: uat-unskip-cli-sim-rejects-tampered-sidecar
date: 2026-08-15
---

# Anti-pattern: STL input does not produce voxel fields or sidecars

## What goes wrong

`resinsim sim --stl test_cube.stl --voxel-cure-mm 0.05 --out model.sim.json`
accepts the `--voxel-cure-mm` flag, runs successfully (exit 0), and
produces a valid `model.sim.json` — but NO `model.fields.bin` sidecar.

The STL slicer produces `CrossSectionArea` values per layer, not
per-layer pixel masks. The voxel cure path
(`SimulationRunner::run_from_layer_inputs`, simulation_runner.rs)
requires masks to compute `CureField` / `PhotoinitiatorField` —
without masks, `voxel_state` stays `None` and `encode_paired_sidecar`
returns `Ok(None)`.

The failure is silent: no error, no warning, just a missing file. A
test fixture that asserts `model.fields.bin.is_file()` after a
successful sim run catches it; one that doesn't will pass the Given
step and fail at the When/Then with a confusing error about a missing
file that the run "should have" produced.

## Why it survives

`--voxel-cure-mm` is a flag on the `Sim` subcommand, not on the
slicer. Clap validates the flag's value (finite, positive) at parse
time — there is no gate that says "this flag requires `--file`, not
`--stl`". The sim runner silently skips voxel computation when masks
are absent, which is the correct behavior for the production use case
(a CTB file provides masks; an STL file doesn't).

## Detection and repair

**Guard:** assert `fields.bin.is_file()` immediately after any fixture
run that claims to produce a sidecar. The assertion catches the
missing sidecar at fixture time, not at assertion time.

**Repair:** use in-process `save_with_provenance` with manually-
constructed `CureField` / `PhotoinitiatorField` (same approach as
`sidecar_security_integration.rs`), or use a committed CTB fixture
via `--file` instead of `--stl`.

## See also

- `crates/resinsim-core/tests/sidecar_security_integration.rs` — the
  in-process fixture approach
- `crates/resinsim-core/tests/uat_steps/cli_sim_rejects_tampered_sidecar.rs`
  — the step-def module that uses this approach
- `docs/patterns/dual-binary-cli-uat-testing.md` — the dual-binary
  pattern this fixture approach composes with
