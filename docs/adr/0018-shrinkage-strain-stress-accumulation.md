---
issue: t2f3-shrinkage-strain-stress
date: 2026-05-20
---

# ADR-0018: Per-voxel shrinkage strain and cumulative residual stress

## Status

Accepted (Phase 4 of issue `t2f3-shrinkage-strain-stress`, 2026-05-20).

## Context

The Tier-2 voxel cure path landed in t2f1 (ADR-0017) gives every voxel a
cumulative absorbed dose. Phase 4 step 3 of
`projects/000-global/research/resinsim-physics-simulation-plan.md` calls
for the next field in the pipeline: **shrinkage strain** (the dimensional
change that cured photopolymer undergoes as it polymerises, KB-142
linear-shrinkage range 0.9–2.4 % for common formulations) and the
**residual stress field** that accumulates when adjacent voxels' strains
mismatch.

Why this matters in MSLA practice:

1. **Warping prediction.** Resin shrinks 1–3 % on cure. Layer-by-layer
   shrinkage creates differential strain between cured and uncured
   regions; on release from the build plate the part relaxes into a
   warped configuration. Predicting where high tensile stress
   accumulates lets the user re-orient or add supports before the print.
2. **Cohesive failure prediction.** Sharp strain gradients between
   adjacent voxels (e.g. thick-thin wall transitions) cause internal
   micro-cracking — visible in some prints as a "cleaved" layer at the
   gradient interface.
3. **Calibration anchor.** `ResinProfile.linear_shrinkage_pct` (KB-142)
   already exists. The strain/stress pipeline closes the loop: ε from
   cure-extent, σ from linear elasticity, threshold-cross from
   tensile_strength_mpa.

## Decision

**Nine interlocking design decisions.** Each addresses one round-1/2
plan-review finding or a user-supplied constraint from the 2026-05-20
planning session. As with ADR-0017, the decisions are interdependent and
should be evaluated together.

### 1. Full 6-component symmetric tensors (user decision)

The strain and stress tensors are stored in **Voigt notation**
`[ε_xx, ε_yy, ε_zz, ε_yz, ε_xz, ε_xy]` (six f32 each).

User decision 2026-05-20: scalar isotropic strain was offered as the
simpler v1 alternative; the explicit choice was full tensor. Per the
`feedback_memory_tradeoffs.md` "simple-first peak-RAM-hungry" policy
the cost (24 bytes/voxel × two fields = +48 bytes/voxel above t2f1's
single-f32 cure field, ~768 MB additional for a typical 16 M voxel part)
is accepted within the documented 18 GB peak budget.

Rejected: scalar isotropic ε. Cheaper but loses any directional
information that downstream models (anisotropic shrinkage, fibre-filled
resin variants) need.

### 2. Linear elasticity with closed-form 6×6 isotropic stiffness (user decision)

`StressTensor::from_strain_linear_elastic(ε, E, ν)` applies the
closed-form isotropic Voigt stiffness:

```text
D_ii = E·(1 − ν) / ((1 + ν)(1 − 2ν))    for normal-normal diagonal
D_ij = E·ν / ((1 + ν)(1 − 2ν))          for normal-normal off-diagonal
D_ss = E / (2·(1 + ν))                  for shear-shear (= G)
```

User decision 2026-05-20: strain-only tracking (no stress derivation,
ε's gradient alone drives failure detection) was offered; the explicit
choice was linear elasticity through to stress.

Small-strain assumption (ε ≲ 5 %) holds within the photopolymer
shrinkage range; KB-162 cites the literature boundary. A non-linear
follow-on is a deliberate t3 ticket.

### 3. ResinProfile gains `youngs_modulus_mpa` + `poissons_ratio`

Both `Option<f32>`, both `#[serde(default)]`. Mirrors the
`cure_kinetics_ea_kj_mol` / `photoinitiator_decay_constant_k_d`
**Option-with-loud-warn** precedent: legacy resin TOMLs without the
fields parse unchanged; runtime falls back to the KB-163 literature
midpoints (`DEFAULT_YOUNGS_MODULUS_MPA = 2000.0`,
`DEFAULT_POISSONS_RATIO = 0.35`) and the producer surfaces an
uncalibrated-moduli caveat in any emitted `FailureEvent.message`.

Validator: `E > 0` if `Some`, `ν` strictly in `(-1.0, 0.5)` if `Some`.
The upper bound is strict because `ν = 0.5` is the incompressible limit
and makes the stiffness denominator `(1 − 2ν)` go to zero (singular).

Data TOMLs:
- `data/resins/generic_standard.toml` — explicit KB-163 midpoints.
- `data/resins/generic_abs_like.toml` — explicit tougher-resin values
  (E = 2300 MPa, ν = 0.38).
- `data/resins/elegoo_ceramic_grey_v2.toml` — INTENTIONALLY uncalibrated
  (ceramic-filled resin diverges significantly from photopolymer
  literature midpoint; calibrate via Athena II).
- `data/resins/liqcreate_premium_black.toml` — uncalibrated (no
  vendor-published values).

### 4. New `FailureType` variants `WarpingRisk` + `CohesiveFailure`

Both unconditional on the enum (no `#[cfg]` gate); only their producer
(`predict_strain_failures`) is feature-gated. Anti-pattern
`non-exhaustive-on-single-variant-enum` was specifically watched for —
the only exhaustive `match FailureType` in the workspace lives at
`resinsim-viz/src/ui/v2/failures_rail.rs:139`; the companion
hand-rolled variant-coverage test at lines 252-272 is updated in
lockstep.

### 5. `predict_strain_failures` is a sibling, NOT an extension of `predict_layer`

Round-1 plan-review HIGH-severity finding rejected the "extend
`predict_layer` with `Option<&StrainField>` args" model because the t2f1
precedent (`SimulationRunner::apply_voxel_cure_for_layer`) is itself a
sibling static method on the runner, not a `predict_layer` extension.
The corrected shape: new `FailurePredictor::predict_strain_failures(
layer, &strain, &stress, resin) -> Vec<FailureEvent>` co-exists with
the unchanged 11-arg `predict_layer`. The SimulationRunner orchestrator
calls both per layer and merges the failure vectors before
`sim.add_layer(...)`.

### 6. `SimulationRunner` owns per-layer iteration; services own per-voxel math

Two new private static methods on `SimulationRunner`:

- `apply_voxel_shrinkage_for_layer(state, layer, layer_height_um, thermal, recipe, printer)`
  iterates the layer's Z-slab, reads `state.cure.dose_at(...)`, computes
  cure-extent via Beer-Lambert + Arrhenius Ec(T), delegates to
  `ShrinkageCalculator::free_shrinkage_strain_at_voxel(...)` per voxel,
  and locks the resulting `StrainTensor` via
  `StrainField::lock_strain_at`.
- `accumulate_layer_stress(state, layer)` iterates the same slab, reads
  `state.strain.strain_at(...)`, delegates to
  `StressAccumulator::strain_to_stress(...)` per voxel, writes via
  `StressField::accumulate_at`.

This mirrors `voxel_cure_calculator.rs` / `apply_voxel_cure_for_layer`:
calculators own column physics, runner owns layer iteration.

### 7. `VoxelState` is dimension-locked across all four fields

The full Tier-2 path now allocates `cure`, `pi`, `strain`, `stress`
together. There is no `--no-strain-stress` flag — voxel-cure-on implies
full Tier-2. The four-field allocation happens atomically in the
existing `let mut voxel_state` block of `run_inner_full`.

User decision 2026-05-20: this coupling is explicit. If a future
workload needs cure-only mode (e.g. KB-160 photoinitiator analysis
without mechanical predictions), a `--no-strain-stress` flag becomes
that ticket's concern.

### 8. `MAX_FIELD_ALLOCATION_BYTES` budget guard with typed error

Round-1 HIGH-risk finding: the strain field at full envelope + 0.05 mm
voxels reaches 24 GB, exceeding the accepted 18 GB peak. Allocating
unconditionally hits kernel OOM with no actionable error.

Mitigation: new `crates/resinsim-core/src/values/field_budget.rs`
introduces `enforce_field_budget(...)` returning typed
`FieldAllocationError::ExceedsBudget { requested_bytes, budget_bytes,
suggested_voxel_size_mm, env_var }` BEFORE any `Array3::zeros` runs.
Default budget: 4 GB per field; override via
`RESINSIM_MAX_FIELD_BYTES`. **The guard is retrofitted to
`CureField` + `PhotoinitiatorField` as well** so all four fields
enforce uniformly — without that, the four-field dimension-lock could
produce partial state on out-of-budget configurations.

Consequences: existing tests / fixtures that constructed CureField or
PhotoinitiatorField above 4 GB now fail with `ExceedsBudget`. CI /
nextest environments that intentionally exercise large allocations must
override `RESINSIM_MAX_FIELD_BYTES`. The 4-config matrix in
`agent-constraints/implementation-conventions.md` is the ship gate.

### 9. Per-voxel yield-fraction WarpingRisk criterion

WarpingRisk fires on the per-voxel **yield fraction** — the share of
cured voxels in a layer's Z-slab whose von Mises stress σ_vm exceeds
`resin.tensile_strength_mpa()`. Two severity thresholds, named as
`pub const` in `services::failure_predictor`:

- `YIELD_FRACTION_WARN_THRESHOLD = 0.001` (0.1 %, Warning) — see
  `crates/resinsim-core/src/services/failure_predictor.rs:430`.
- `YIELD_FRACTION_CRIT_THRESHOLD = 0.05` (5 %, Critical) — see
  `failure_predictor.rs:437`.

CohesiveFailure retains its strain-gradient threshold:
`|∇ε| > GRADIENT_THRESHOLD_FRAC = 0.005` (Warning).

**Why yield-fraction against `tensile_strength_mpa`, not against
`0.5 × tensile_strength_mpa`.** `tensile_strength_mpa` IS the
physically-correct yield threshold — von Mises generalises uniaxial
tensile yield to a multi-axial stress state via the invariant
`σ_vm > σ_y`. Halving it would inject an arbitrary safety factor
unmoored from physics. Per-voxel fraction (rather than layer-max σ_vm)
makes the signal robust to single-voxel outliers from upstream
numerical artefacts while remaining sensitive to true progressive
yielding.

**Model-gap caveat (KB-162).** The σ_vm value used here reflects
**free-shrinkage** stress only — it does NOT include the cumulative
residual stress that accumulates as later layers cure against
already-cured layers below. Real MSLA prints warp because of the
latter. Lilith torso empirical validation on t2f3 v1 confirms this:
σ_vm peaked at 5.71 MPa per layer against Elegoo Ceramic Grey's
tensile_strength of 38 MPa — `voxel_yield_fraction` read exactly 0.0
on every layer. The B3 integration test in t2f3.1
(`honest_zero_yield_fraction_on_generic_standard_solid`) locks this
"honest-zero on the calibrated profile" behaviour; the companion
`nonzero_strain_magnitude_on_generic_standard_solid` test catches
the dual magnitude-collapse direction. The yield-fraction signal will
become a useful early-warning at this same threshold once the Tier-3
cumulative residual stress model lands — no threshold recalibration
needed because `tensile_strength_mpa` is already the physical yield
boundary.

Real-world calibration of E, ν, and z_ratio via Athena II
measurements is a separately filed follow-on (NOT in t2f3 scope; see
KB-163 + KB-164). The `FailureEvent.message` discloses the
uncalibrated-moduli caveat when `resin.has_calibrated_moduli() ==
false` (any of E, ν, z_ratio is `None`) so users can distinguish
calibration-artefact emissions from real physics. t2f3.1 widened the
predicate from the original 2-of-2 (E + ν) to 3-of-3 (E + ν +
z_ratio); see Decision 10 below.

### 10. Anisotropic free shrinkage (Z amplification)

`StrainTensor::from_free_shrinkage` takes a `z_anisotropy_ratio: f32`
parameter (was a scalar isotropic strain in pre-fix v1). `ratio > 1.0`
amplifies `ε_zz` relative to `ε_xx = ε_yy` while preserving the
volumetric trace:

```text
ε_xx = ε_yy = a            (in-plane)
ε_zz = ratio · a           (out-of-plane)
trace = (2 + ratio) · a = 3 · ε_iso       ⟹    a = 3 · ε_iso / (2 + ratio)
```

For `ratio = 1.5` (the v1 default): `a ≈ 0.857 · ε_iso`,
`ε_zz ≈ 1.286 · ε_iso`, `trace = 3 · ε_iso` ✓. The volumetric trace
identity preserves `linear_shrinkage_pct`'s vendor-data-sheet meaning
regardless of the chosen ratio — vendors publish total linear
shrinkage, and as long as the trace is conserved that headline number
is honoured.

**Why it matters.** The pre-fix v1 produced a *hydrostatic-symmetric*
strain field. The deviatoric stress of a hydrostatic strain (under
linear elasticity with constant E + ν across all axes) is identically
zero, and von Mises is invariant under hydrostatic stress — so
σ_vm ≡ 0 at every voxel. Hydrostatic strain is a silent-zero
warpage detector
(`docs/patterns/anti/hydrostatic-strain-dead-warpage-detector.md`):
the per-voxel yield criterion produces zero signal regardless of the
strain magnitude. Breaking the XY-vs-Z symmetry is what makes any
yield criterion produce a non-trivial signal at all.

**Default value.** `ratio = 1.5 ± 0.3` (KB-164). Anchored to the
PMC5344561 modulus-anisotropy measurements (`E_z / E_xy ∈ [1.27,
1.39]` for untreated DLP photopolymers — strain anisotropy correlates
because the stiffer axis resists shrinkage less under a given driving
chemistry) plus the engineering mechanism: in MSLA the XY plane is
constrained by adhesion to the cured layer below, while Z is free to
amplify. The ±0.3 band acknowledges that strain anisotropy is not
identical to modulus anisotropy — calibration via DIC capture on a
free-shrinkage column geometry per resin is the follow-on.

**Disclosure.** When `shrinkage_anisotropy_z_ratio` is `None`, the
uncalibrated-moduli caveat fires (per A1 + A2 in t2f3.1). The caveat
cites BOTH KB-163 (E + ν defaults) AND KB-164 (z_ratio anisotropy
±0.3 band), closing the disclosure trail.

**References for Decision 10.** KB-163 (moduli defaults disclosure
contract), KB-164 (Z/XY anisotropy ratio mechanism + literature
anchor + volume-conserving mapping), PMC5344561 (modulus-anisotropy
data), `docs/patterns/anti/hydrostatic-strain-dead-warpage-detector.md`
(the silent-zero anti-pattern this decision breaks), and the A1 + A2
predicate/caveat changes in t2f3.1.

## Alternatives considered

**(a) Scalar isotropic ε for v1.** Cheaper memory + simpler math; loses
directional information. User explicitly rejected on 2026-05-20.

**(b) Strain-only failure detection (no stress tensor).** Cheaper still;
loses the natural threshold via tensile strength (which is a stress).
User explicitly rejected on 2026-05-20.

**(c) Extend `predict_layer` with `Option<&StrainField>` args.** Plan
v1; rejected in round-1 review because t2f1 didn't actually use this
shape and the precedent claim was wrong. Plan v2 uses a sibling method.

**(d) Allocate strain/stress lazily on first write.** Saves memory on
runs that never accumulate strain (e.g. very thin parts). Rejected for
v1 because the four-field dimension-lock is simpler to reason about;
lazy allocation can be a follow-on if profiling shows it matters.

## Consequences

- `ResinProfile` gains two new mechanical-axis fields; legacy TOMLs
  continue to parse but emit a loud warn-on-use via the producer.
- `FailureType` gains two new variants; downstream `match`/`==` sites
  audited and updated.
- `PrintSimulation` gains `strain_field` + `stress_field` accessors and
  a new `set_strain_stress_fields(strain, stress)` setter parallel to
  `set_voxel_fields(cure, pi)`. New aggregate invariant:
  `strain_field.is_some()` requires `cure_field.is_some()` with matching
  dimensions.
- `LayerResult` gains three Option<f32> caches
  (`strain_magnitude_max`, `stress_von_mises_max_mpa`,
  `strain_gradient_max_frac`) — unconditional, not feature-gated, so
  the struct shape stays uniform across feature configs. Populated when
  the voxel pass runs; `None` on Tier-1 paths.
- `CureFieldError` + `PhotoinitiatorFieldError` gain an `ExceedsBudget`
  variant — a non-`#[non_exhaustive]` enum extension that is
  technically API-breaking but acceptable inside this internal crate.
  Behaviour change: existing constructors that previously allocated
  unconditionally now fail when the requested allocation exceeds
  `MAX_FIELD_ALLOCATION_BYTES`. CI must override the env var for
  intentionally-large allocations.
- `sim.json` schema is NOT bumped to v2 in this issue. The new
  Option-typed fields use `skip_serializing_if = Option::is_none`, so
  v1 producer output stays a valid v1-shape file when the new fields
  are `None`. A v2 bump is deferred to a follow-on when t2f4 / t2f5
  also add fields, to amortise the fixture-update cost.
- `agent-constraints/implementation-conventions.md` 4-config matrix
  applies unchanged.
- New domain services: `ShrinkageCalculator`, `StressAccumulator`.
  Both are pure-function helpers; orchestration owns iteration.
- Inspector hooks (formerly plan step 12) DROPPED — deferred entirely
  to t2f6-field-inspector. Tests in the integration suite read fields
  via the aggregate API directly.

## References

- ADR-0005 — three-axis printer/resin/recipe (the resin chemistry-axis
  fields added here).
- ADR-0007 — two-stage LED/vat thermal coupling (used by Ec(T) inside
  `apply_voxel_shrinkage_for_layer`).
- ADR-0015 — sim.json canonical interchange (no schema bump in this
  issue; deferred).
- ADR-0017 — voxel cure field + photoinitiator depletion. Parent ADR;
  t2f3 mirrors its architecture (dense Array3, bbox-anchored,
  Z = num_layers, two-layer NaN defence, field-sim Cargo feature gate,
  4-config matrix).
- **KB-161** (this issue) — Cure-extent → free-shrinkage strain.
- **KB-162** (this issue) — Linear-elasticity 6×6 Voigt stiffness +
  per-voxel yield criterion (the basis of the §9 threshold scheme).
- **KB-163** (this issue) — Photopolymer E + ν literature ranges +
  uncalibrated-moduli disclosure contract.
- **KB-164** (this issue) — Z/XY shrinkage anisotropy ratio
  mechanism + PMC5344561 modulus-anisotropy anchor + volume-
  conserving mapping. Basis of Decision 10.
- `docs/patterns/voxel-field-z-dimension-is-layer-count.md` — Z
  convention shared with t2f1.
- `docs/patterns/nan-two-layer-defence.md` — applied uniformly to
  StrainTensor + StressTensor + StrainField + StressField.
- `docs/patterns/single-source-arrhenius-helper.md` — Ec(T) Arrhenius
  goes through `CureCalculator::ec_at_temp` in
  `apply_voxel_shrinkage_for_layer`.
- `docs/patterns/anti/rust-nan-positive-validation-gap.md` — strain
  validators use `is_finite()` first (signed components, positive
  check alone misses NaN).
- `docs/patterns/anti/voxel-z-step-from-lateral-voxel-size.md` — Z
  dimension of strain/stress fields comes from layer count, NOT
  voxel_size_mm.
- `feedback_memory_tradeoffs.md` — accepted 18 GB peak budget.
- `feedback_no_ora_commits.md` — research-plan markdown at the ora
  root is NOT updated by this PR.

## Folded plan-review findings (round 2, non-blocking)

These were surfaced during planning and accepted as documented rather
than addressed in v1 code:

- MED: CureField/PhotoinitiatorField constructor behaviour change
  (consequences section above). CI must override env var.
- MED: Full Tier-2 mandatory mode (no `--no-strain-stress` flag).
  Decision 7 above.
- MED: Uncalibrated-moduli disclosure in FailureEvent.message.
  Implemented via `resin.has_calibrated_moduli()` (Decisions 9 and
  10 above). t2f3.1 widened the predicate to require z_ratio, and
  the caveat now cites KB-163 + KB-164.
- LOW: `lock_strain_at` naming (set-once semantic explicit in the
  StrainField API surface).
- LOW: v2 sim.json roundtrip test — deferred until v2 bump.
  **RESOLVED by t2f3.5 / ADR-0019 (2026-05-20)**: schema bumped to v2
  with strain + stress fields persisted via the paired binary sidecar
  (`<stem>.fields.bin`). Roundtrip tests live in
  `crates/resinsim-core/tests/sidecar_roundtrip_integration.rs`.

## Folded post-implementation findings (t2f3.1)

After t2f3 shipped, the Phase 5 code review surfaced 5 MED + 3 LOW
findings folded as non-blocking. Six of those became the t2f3.1
follow-up implementation; two LOW (typed `Pressure` newtype +
sentinel-explicitness) were accepted as v1 trade-offs. The t2f3.1
implementation:

- Widened `has_calibrated_moduli()` from a 2-of-2 predicate (E + ν)
  to a 3-of-3 predicate (E + ν + z_ratio). The disclosure contract
  was silently incomplete for any partially-calibrated profile.
- Extended the `FailureEvent.message` caveat to cite both KB-163 and
  KB-164.
- Added six direct unit tests for `StressField::yield_fraction`,
  two error-path tests for
  `PrintSimulation::set_strain_stress_fields`, and an integration-
  level honest-zero regression guard (`B3`) + companion
  strain-magnitude guard (catches the dual magnitude-collapse
  direction).
- Rewrote §9 from the original `vm > 0.5 × tensile`-style threshold
  set to the per-voxel yield-fraction criterion that's actually
  implemented in `failure_predictor.rs`.
- Added Decision 10 documenting the anisotropic free-shrinkage
  Z-amplification redesign (the change that broke the
  hydrostatic-symmetric silent-zero detector and made the per-voxel
  yield criterion produce a non-trivial signal at all).
- Updated KB-163 to match the 3-of-3 predicate.
