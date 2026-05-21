---
issue: t2f4-thermal-diffusion
date: 2026-05-21
---

# ADR-0020: Tier-2 spatial thermal diffusion as the canonical voxel-mode thermal path

## Status
Accepted (Phase 3 of issue `t2f4-thermal-diffusion`, 2026-05-21).

## Context

The Tier-1 lumped-capacitance thermal model (ADR-0007, KB-152) tracks
two coupled scalars: an LED-case temperature time-series fitted to
Mars 5 Ultra telemetry, and a vat temperature derived via a dimensionless
`led_to_vat_coupling` factor. ADR-0007 §"Alternatives considered (b)"
explicitly scoped full spatial diffusion to Tier-2 as a deliberate
deferral.

Phase 4 of `projects/000-global/research/resinsim-physics-simulation-plan.md`
schedules four Tier-2 voxel-field deliverables. As of 2026-05-20 the
prior three have shipped:

- ADR-0017 — `CureField` + `PhotoinitiatorField` (`t2f1`)
- ADR-0018 — `StrainField` + `StressField` (`t2f3`)
- ADR-0019 — RSFIELD sidecar binary format (`t2f3.5`)

This work (`t2f4-thermal-diffusion`) lands the fourth: a 3D temperature
field that supersedes the Tier-1 vat scalar for downstream consumers
(per-voxel `Ec(T)` Arrhenius cure correction, lazy per-voxel viscosity).
The paused issue `vat-temp-fan-curves` ("Tier-1 vat plateau under-
predicts reality") is its proximate motivator.

External literature surveyed during planning (Procedia CIRP 2022 on
in-situ thermography + FE simulation for VPP; Polymers 2020 / PMC7284352
on photo-DSC heat of polymerization ~260-570 J/g for acrylates; ADI
heat-equation literature including Peaceman & Rachford 1955) shaped the
scheme choice and the deferred follow-ons listed below.

## Decision

### (i) Explicit FTCS with CFL guard; ADI documented as CPU fallback

Forward-time centred-space (FTCS) finite-difference stencil:

```
T_new[i,j,k] = T_old[i,j,k] + dt · α · (
    (T[i+1,j,k] − 2 T[i,j,k] + T[i−1,j,k]) / dx²
  + (T[i,j+1,k] − 2 T[i,j,k] + T[i,j−1,k]) / dy²
  + (T[i,j,k+1] − 2 T[i,j,k] + T[i,j,k−1]) / dz²
)
```

CFL guard: `dt_max = 0.5 · min(dx², dy², dz²) / (3 · α)`. Substeps per
layer cap at 1000; exceeding raises `CflBudgetExceeded` with a hint
pointing to the ADI fallback documented under "Numerical scheme choice"
in §Alternatives below.

Rationale for FTCS over ADI:
- **Forward-compat with t2f5 GPU acceleration.** Explicit stencils are
  trivially GPU-portable (one kernel per substep, no thread
  dependencies). ADI's tridiagonal solves have sequential
  dependencies along each line — GPU-painful. Picking ADI now would
  create a rewrite at t2f5.
- **Simpler first landing.** No tridiagonal solver, no BC matrix
  assembly.
- **Performance budget fits.** Back-of-envelope for Mars 5 Ultra
  (16 M voxels, ~30 substeps/layer, 200 layers, ~7 FLOPS/voxel): ~28 s
  on CPU single-thread. Inside the "seconds-minutes" Tier-2 budget.

### (ii) Full vat envelope (NOT part bbox)

`ThermalField` spans the full vat volume, anchored to
`PrinterProfile.build_envelope_mm`. The other Tier-2 voxel fields
(`CureField`, `StrainField`, `StressField`) remain part-bbox anchored.
The aggregate invariant evolves from "all fields share one bbox" to
"each field has its own self-consistent bbox + `voxel_size_mm`; thermal
matches the printer envelope; the rest match the part bbox".

Rationale: the diffusion solve needs the wall + resin surface boundary
condition zones. Cropping to the part bbox removes the BC anchors,
leaving the solve with synthetic boundaries that mean nothing physical.

### (iii) Z axis is spatial mm (NOT layer index)

`ThermalField`'s Z axis represents vat-envelope height in mm divided
by `voxel_size_mm`. Field is mutated in-place every solver substep —
no `write_layer` write-once semantic like `StrainField`'s
`lock_strain_at`.

This intentionally departs from the
`docs/patterns/voxel-field-z-dimension-is-layer-count.md` pattern that
governs the other Tier-2 voxel fields. The new pattern
`docs/patterns/thermal-field-z-dim-is-spatial.md` documents the
deviation. Temperature is a continuous spatio-temporal field; layer-
count Z is the wrong abstraction.

### (iv) Single resin-domain α (vat walls handled as convective BC; cured polymer treated as resin)

The diffusion domain physically spans liquid resin (α ≈ 1.07e-7 m²/s),
cured polymer (slightly higher k; α ≈ 1.2-1.5× resin), FEP film at the
bottom (~9e-8 m²/s), and the Al vat wall (~8.4e-5 m²/s, 800× resin).
A faithful multi-region α would force `dt_max` through `α_max` to
~3 µs — millions of substeps per layer. Catastrophic.

v1 ships with a SINGLE scalar α = liquid resin's thermal diffusivity.
The vat wall is modelled exclusively as a convective boundary condition
with lumped resistance `1/h_eff = 1/h_air + wall_thickness/wall_k`.
Cured polymer inside the field is treated as resin — the approximation
introduces a ~30% local α error in the inner part region but the
cured-polymer region is shielded from the high-flux BC zones so the
downstream impact is small.

Multi-region α heterogeneity is filed as the `t2f4b` follow-on (see
§Follow-ons).

### (v) Single-threaded execution (no Rayon, no `par_*` iterators)

The solver is single-threaded. Same input → bit-identical
`thermal_field` bytes → stable sidecar sha256 (the existing
load-bearing invariant). Parallel reductions break this because f32
addition is non-associative.

Parallelisation is deferred to `t2f5-gpu-acceleration-wgpu` where the
cost-of-determinism trade is addressed by careful kernel design.

### (vi) Tier-1 stays as LED-case BC source, routed via LayerTimingCalculator

The Dirichlet bottom BC at the LCD/FEP interface is computed from
Tier-1's `ThermalCalculator::led_case_temperature_at(t_layer_end, …)`
where `t_layer_end = LayerTimingCalculator::cumulative_times_sec(recipe,
printer, n)`. The latter honours the ADR-0007 Linear-vs-Tilt
release-mechanism distinction — critical for Mars 5 Ultra (Tilt) whose
canonical per-layer release duration is `lift_cycle_sec`, not
`lift_distance/lift_speed`.

Pattern: `docs/patterns/tier1-as-bc-source-for-tier2.md`.

When Tier-2 is active, downstream consumers (cure, viscosity) bypass
Tier-1 Stage B (`vat_temperature_at_layer_v2`) and read per-voxel
temperature from `ThermalField`. Tier-1 Stage A survives as the LED-case
source only.

### (vii) No scalar/field dispatch fallback under field-sim

Under the `field-sim` Cargo feature, `ThermalField` is unconditional
on the aggregate (not `Option<ThermalField>`) and
`VoxelCureCalculator::ec_at_temp` is REPLACED by `ec_at_temp_field`
(not a sibling — the scalar wrapper at `voxel_cure_calculator.rs:284`
is deleted, callers migrate). The Tier-1 cure path
(`CureCalculator::ec_at_temp` taking a scalar `VatTemperature`) remains
for default-feature builds and as the underlying single-source Arrhenius
helper.

Auto-activation: setting `--voxel-cure-mm` (which engages the voxel
cure path) automatically engages Tier-2 thermal. The two are physically
coupled — per-voxel `Ec(T)` correction needs per-voxel `T`. No separate
`--thermal-diffusion-mm` CLI flag.

### (viii) Explicit scope cuts (filed as follow-ons)

v1 deliberately ships WITHOUT:

- **Exothermic source term Q ∝ Δcure × ΔH.** Heat-of-polymerization for
  acrylates is ~260-570 J/g (PMC7284352 photo-DSC), enough to raise a
  fully-curing voxel by ~250 K adiabatically. The cure → thermal
  dispatch ordering in `SimulationRunner::apply_voxel_*_for_layer`
  preserves Δdose availability for a follow-on. Filed as `t2f4c`.
- **Stored `ViscosityField` value object.** Viscosity is computed
  lazily from `ThermalField + ResinProfile` via the new
  `viscosity_at_temp_field` accessor. No stored VO in v1. Filed as
  `t2f4d`.
- **Multi-region α heterogeneity** (see §Decision iv). Filed as `t2f4b`.
- **ADI solver as CPU fallback.** Documented under §Alternatives but
  unimplemented. Filed alongside `CflBudgetExceeded`'s hint.
- **GPU parallelisation.** Filed as `t2f5-gpu-acceleration-wgpu`
  (already in the Phase-4 plan).
- **Vat-thermistor telemetry collection on Mars 5 Ultra.** No vat-side
  ground truth exists in `data/elegoo/`. BME280 + thermocouple proposal
  filed at harvest.
- **Sub-substep BC interpolation.** v1 holds LED-case BC constant across
  inner CFL substeps of a layer. Filed alongside the
  `tier1-as-bc-source-for-tier2` pattern.
- **Exposure-on/off thermal phases.** v1 evolves diffusion for the full
  `layer_time`; a finer model would split into exposure-on (LED
  active) and exposure-off (lift cycle). Filed at harvest.

### (ix) Single-PR landing accepted

This ships 2 new ADR/KB files + 3 new pattern docs + 1 new VO + 6 new
typed-boundary VOs + 1 new domain service + 1 sidecar variant + format
version bump + match-arm updates + 2 UAT scenarios + 1 calibration
integration test + 7 in-repo TOML migrations.

Per project memory `feedback_memory_tradeoffs.md` (simple-first over
splitting into coupled sub-PRs) and the ADR-0017 single-PR-landing
precedent, this is accepted as a single PR. Migration is mechanical
(no public-API rewrites like t2f1's `cure_depth_um → cure_depth_um_summary`
37-file sweep); thermal field is purely additive under `field-sim`.

### (x) RSFIELD_FORMAT_VERSION bumped 1 → 2 (clean break)

The sidecar `RSFIELD_FORMAT_VERSION` constant bumps from 1 to 2.
Decoders reject v1 sidecars with the existing typed `UnknownFormatVersion`
error path — no v1 read path retained, no compat shim. This is consistent
with the "don't care about legacy" lifecycle direction.

Safe because no `.fields.bin` files are checked into the repository
(verified by `find` audit). Existing tests (`sidecar_roundtrip_integration.rs`,
`sidecar_security_integration.rs`) generate fixtures at test-time; their
generators update to v2 in lockstep with the constant bump.

`MAX_FIELD_COUNT = 16` ceiling: `Thermal` is the 5th defined `FieldKind`;
11 slots remain.

## Alternatives considered

**(a) Alternating Direction Implicit (ADI), e.g. Peaceman-Rachford.**
Unconditionally stable for the heat equation; full layer-time `dt`
allowed; tridiagonal solves per dimension. The standard choice in
mature heat-equation literature. **Rejected for v1** because the
tridiagonal solves have sequential dependencies along each line, making
the t2f5 GPU port a rewrite rather than a port. Filed as the CPU-fallback
follow-on if FTCS substep counts blow the 1000 cap; the
`CflBudgetExceeded::hint` field points here.

**(b) Finite-element methods (FEM) as in commercial COMSOL VPP
workflows** (e.g. Procedia CIRP 2022 in-situ thermography paper). High
fidelity for geometric BCs but heavy implementation; no per-voxel
storage match with `CureField` / `StrainField`. **Rejected** for v1 —
massive scope, no clear consumer beyond a single calibration test.

**(c) Reaction-diffusion coupled solver with photoinitiator depletion + heat
source from cure** (newer VPP literature). Couples this work with t2f4c
exothermic Q + t2f1's existing photoinitiator depletion. **Rejected for
v1** as scope creep; preserved as the t2f4c follow-on. Cure→thermal
dispatch order in `SimulationRunner` preserves the option.

**(d) Multi-region α field from the outset.** See §Decision iv. The
α_max-driven CFL would push to microsecond substeps, blowing the budget.

**(e) Layer-count Z + per-layer snapshot history.** See §Decision iii.
Turns 3D × time into 4D storage; N× memory; obscures the natural domain.

## Consequences

- **`PrintSimulation.thermal_field: ThermalField`** added unconditionally
  under `field-sim` (gate at module level — the type doesn't exist in
  default builds). Default-feature builds untouched.
- **`PrinterProfile.build_envelope_mm` and the 7 new thermal material
  fields are required under `field-sim`** at `validate()` time. The 7
  in-repo profile TOMLs are migrated in lockstep with the code change.
  Implementation pattern: keep the fields as `Option<T>` on the struct
  + `#[serde(default)]` so cross-feature TOML interchange holds; reject
  `None` at `validate()` only when `cfg(feature = "field-sim")` is on.
- **Cargo feature matrix (4 configs)** per
  `agent-constraints/implementation-conventions.md` must stay clean
  before code review.
- **Sidecar v1 sidecars on disk become unreadable.** Acceptable per
  §Decision x and the "don't care about legacy" direction.
- **The paused `vat-temp-fan-curves` issue is superseded by this work**
  and closes cleanly post-merge (`swamp model method run vat-temp-fan-curves
  close --input close_reason="superseded-by-t2f4-thermal-diffusion"`).
  The "needs vat-thermistor data" caveat lives in the t2f4 harvest
  follow-on, not in the close note.

## Cold-start convention

`ThermalField` initialises to uniform `T_ambient` at layer 0. The first
hundreds of layers' diffusion wave from the LED-case BC IS the warm-up.
Visible in `resinsim-viz` heatmaps as a long cold tail in the early
print; documented here so a reviewer doesn't read the convention as a
bug. This matches the physical reality (a Mars 5 Ultra started from
ambient does take ~3 τ ≈ 3-4 h to reach steady state per KB-152's
`led_tau_sec = 4000 s`).

## References

- **KB-152** — `docs/kb/KB-152-led-vat-thermal-coupling.md` — Tier-1
  formulas, fitted Mars 5 Ultra coefficients, telemetry provenance,
  vat-side ground-truth gap.
- **ADR-0007** — Tier-1 two-stage model architectural decision. Now
  points to KB-152 for formulas (content lifted out of ADR-0007 §Decision
  during this lifecycle).
- **ADR-0017** — `CureField` / `PhotoinitiatorField` value objects
  (sibling Tier-2 work pattern).
- **ADR-0018** — `StrainField` / `StressField` (sibling Tier-2 work
  pattern; ADR-0018 numbering collision with the parallel
  light-crosstalk ADR-0018 is pre-existing — out of scope to renumber
  here).
- **ADR-0019** — RSFIELD sidecar binary format (extended in §Decision x).
- **`docs/patterns/cfl-guard-on-anisotropic-stencil.md`** — solver CFL
  + substep cap pattern.
- **`docs/patterns/tier1-as-bc-source-for-tier2.md`** — dispatch policy
  pattern.
- **`docs/patterns/thermal-field-z-dim-is-spatial.md`** — the spatial-Z
  departure from the layer-count voxel-field pattern.
- **`docs/patterns/voxel-field-z-dimension-is-layer-count.md`** — the
  layer-count Z pattern this work intentionally departs from.
- **`docs/patterns/typed-temperature-boundary.md`** — pattern for typed
  CLI/TOML scalar inputs; reused for the 6 new thermal material VOs.
- **`docs/patterns/single-source-arrhenius-helper.md`** — pattern that
  `ec_at_temp_field` continues to honour by delegating to
  `CureCalculator::ec_at_temp`.
- **`docs/patterns/anti/voxel-z-step-from-lateral-voxel-size.md`** —
  anti-pattern affecting layer-count Z fields; ThermalField is not
  affected because Z IS the lateral voxel pitch.
- **`docs/patterns/anti/clamp-onto-boundary-convolution.md`** — why
  `temperature_at_world` returns `Err(OutOfEnvelope)` instead of
  clamping at the vat boundary.
- **`data/elegoo/`** — raw thermistor telemetry (LED case + ambient
  only; no vat thermistor — see §Decision viii).
- **Procedia CIRP 2022** — in-situ thermal monitoring informed modeling
  for VPP additive manufacturing (external; shaped calibration strategy).
- **PMC7284352** — photo-DSC of acrylate photopolymers (external; ΔH
  ~260-570 J/g; informs the deferred exothermic source term).

## Follow-ons

| ID | Title | Trigger |
|---|---|---|
| `t2f4b` | Multi-region α heterogeneity (resin / cured / FEP / wall) | Vat-thermistor data shows homogeneous-α off by > convective-BC fit headroom |
| `t2f4c` | Exothermic heat-of-polymerization source term | When fast intra-layer transients matter (peel-force or viscosity transients) |
| `t2f4d` | Stored `ViscosityField` value object | When viz heatmaps want viscosity at full voxel resolution |
| `t2f4e` | Sub-substep BC interpolation | When fast LED-case transients dominate (raft → first part transitions) |
| `t2f4f` | Exposure-on/off thermal phases | When inner-layer temperature dynamics matter |
| `t2f5` | GPU acceleration via wgpu | Already in Phase-4 plan |
| `vat-thermistor-telemetry` | BME280 + thermocouple data collection on Mars 5 Ultra | Filed at t2f4 harvest |
| `adi-cpu-fallback` | ADI solver as the FTCS fallback | When `CflBudgetExceeded` fires for a real workload |
