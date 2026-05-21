---
issue: t2f4-thermal-diffusion
date: 2026-05-21
---

# UAT: voxel-mode CTB run emits the tier-2 thermal log lines

## Rationale

ADR-0020 §Decision-level observability contract. Auto-activation (per
ADR-0020 §Decision vii) replaces the originally-planned standalone
`--thermal-diffusion-mm` flag — `--voxel-cure-mm` is the only
activation surface. The stderr `tier-2 thermal: ...` info line and
`tier-2 thermal complete: ...` summary line carry the calibration
parameters and the run totals; they are the only observability into
a Tier-2 thermal run.

`thermal-field-arrhenius-per-voxel.md` UAT-1 mentions the lines but
asserts nothing about them. Without a dedicated test, a future
refactor could double-emit (per-layer instead of run-start), drop
silently, or change the prefix in a way that breaks downstream
operator tooling.

This UAT scenario pins the exact-once-each contract.

## UAT-1: Tier-2 activation emits exactly one info line + one summary line per run

```gherkin
Scenario: voxel-mode CTB run emits the tier-2 thermal log lines exactly once each
  Given a CTB input with per-layer masks
  And a Mars 5 Ultra printer profile (with field-sim thermal fields populated)
  And the Generic Standard resin
  When `resinsim sim --voxel-cure-mm 0.5 --file <CTB> --resin <resin> \
    --printer <printer> --initial-led-temp 27 --ambient 22 \
    --out model.sim.json` runs to completion
  Then exactly one line on stderr starts with `tier-2 thermal: voxel_size=`
  And that line carries `α=`, `k_resin=`, `h_top=`, `h_side(lumped)=`
      tokens for operator-side calibration debugging
  And exactly one line on stderr starts with `tier-2 thermal complete: total_substeps=`
  And the complete line carries `max_T=`, `volume_mean=`, `wall_clock=` tokens
  And neither line appears when `--voxel-cure-mm` is absent (Tier-1 path)
```

## See also

- ADR-0020 §Decision viii — observability contract.
- `crates/resinsim-core/src/app/simulation_runner.rs` —
  `apply_voxel_thermal_for_layer` emits the info line gated by
  `state.thermal_log_emitted`; `run_inner_full` emits the complete
  line after the per-layer loop.
- Sibling: `thermal-field-arrhenius-per-voxel.md` — documents the
  per-voxel Ec(T) contract but not the log lines.
