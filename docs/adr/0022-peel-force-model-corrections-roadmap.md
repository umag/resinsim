---
issue: peel-model-corrections
date: 2026-07-07
---

# ADR-0022: Peel-force model corrections roadmap

## Status

Accepted (roadmap). Extends — does not supersede — the v1 model recorded in
KB-114 and the calibration campaign in `spec/EXPERIMENT-PLAN-v1.1.md`. Each stage
below is a future issue-lifecycle PR; this ADR only fixes the direction and
sequencing, and changes no code.

## Context

KB-115 (`docs/kb/KB-115-peel-model-base-adhesion-gap.md`) recorded a validated
gap. resinsim's v1 peel model is area-driven —
`F_peel = σ_peel·A·f(v_lift)` (`crates/resinsim-core/src/services/peel_force_calculator.rs:16-23`,
KB-114) plus a hardcoded suction term — so on the real Athena reference print
`PFA-75mm-unsplit-spike` the predicted peel force peaks at the
cross-section-area-peak layer (~15) while the **measured** force peaks at
**layer 0** (base adhesion + initial suction), decaying monotonically to the tip.
Shape correlation was 0.821, but a single counts→Newton gain fit gave R² ≈ 0: the
area-proportional model cannot reproduce the first-layer spike (13 983 counts vs
~181 mean).

Since KB-115 the knowledge base gained a large force-modeling corpus: the peel
series KB-110..KB-117, the two-regime synthesis KB-185/KB-186 (verbatim
Pan/Kendall/Stefan equations, measured shape/porousness/speed tables), the
oxygen-inhibited release-layer mechanism KB-116, cohesive-zone FEA and separation
sources (KB-118/119/187/188/189/191..197), and the suction/data-gap entries
(KB-181/184/173). The pre-registered `spec/EXPERIMENT-PLAN-v1.1.md` (§1, §7)
already commits to the target functional form:

```
F_peak = [ a_area·A + b_perim·P ] · f_resin(v_lift) + F_suction(η(T), A, v, d)
```

and E2's small-area intercept `F₀` is "assumed to be suction" — i.e. exactly the
base term KB-115 says is missing.

Two constraints shape everything below:

1. **We have one real print.** That validates model *structure* qualitatively but
   cannot fit coefficients. Quantitative calibration (per-resin `a_area`,
   `b_perim`, `f_resin`, `F_suction`, and the acceptance thresholds R² ≥ 0.90 per
   stratum / E9 |pred−meas|/meas ≤ 15%) needs the full E1–E9 campaign. This ADR
   does not pretend otherwise.
2. **The validation harness can't yet grade a correction.** `inspect calibrate`
   compares in a min-max **normalized** space (shape only —
   `services/force_comparator.rs:38-39`), reads **adhesion-only** `peel_force_n`
   (ignoring `suction_force_n`/`total_force_n` — `failure_predictor.rs:283`,
   `main.rs` cmd_calibrate), and has **no committed regression** (the
   calibrate/athena UATs are documentation-only with no step definitions;
   `tests/nanodlp_real_sample.rs` is `#[ignore]` and parse-only; the 37 MB
   reference file is uncommitted). Absolute-force accuracy is never scored today.

## Decision

Adopt the staged roadmap below. The single highest-value, data-validated change
is the first-layer term; the corpus and EXPERIMENT-PLAN converge on making it one
piece of a two-regime split. Chosen functional form for the first-layer term:
the **KB-116 oxygen-freshness σ-relaxation** — σ_peel is elevated at layer 0
(the freshest, most-oxygen-inhibited release layer) and relaxes exponentially to
the steady σ over ~`recipe.bottom_layer_count()` layers. It is **area-scaled**
(multiplies A), physics-motivated rather than a bare offset.

### Stage 0 — Harness readiness (prerequisite; no physics change)

Make a correction *measurable* before making one:

- Extend `ComparisonReport` with predicted/actual **peak-layer indices** and
  surface the calibrator **R²** as a first-class metric (both are the KB-115
  acceptance signals).
- Change `inspect calibrate` to compare **`total_force_n`** (peel + suction +
  base) rather than adhesion-only `peel_force_n`, so a base/suction correction is
  actually visible to scoring.
- Commit a small **synthetic regression golden** over those metrics (the 37 MB
  real job stays an `#[ignore]` env-gated check).
- Fix `force_series_extractor` correctness: the module doc claims a marker-less
  log yields "a single aggregate layer" but the code returns empty
  (`force_series_extractor.rs:6-8` vs test `:108-113`), and pressure samples
  before the first `T=0` marker are silently dropped.

Rationale: without Stage 0, later stages cannot be graded and the layer-0 spike
stays invisible to the metrics.

Implemented by issue `peel-corrections-s0-harness-readiness` (peak-layer via the
shared `services::peak_index` argmax that `inspect athena` also uses,
`PrintSimulation::total_force_series`, the `force_comparator_golden` regression,
and the extractor doc/prelude fix).

### Stage 1 — First-layer base adhesion via oxygen-freshness σ-relaxation (KB-116)

Add the elevated-σ relaxation term. Implement it as a **separate** term/method on
`PeelForceCalculator`, kept OUT of the pure `peel_force(σ,A,f)` so the KB-114 test
vectors (`peel_force_calculator.rs:81-121`) and the
`tests/force_properties.rs::peel_force_linear_with_area` proptest remain valid.
Wire it into `FailurePredictor::predict_layer`, fold it into `total_force`, and
surface a new `LayerResult` field. Drive magnitude and relaxation length from a
new **optional** resin parameter, reusing the established optional-field template
(`entities/resin_profile.rs` `cure_kinetics_ea_kj_mol: Option<f32>` +
`effective_*()` + `#[serde(default)]`) so legacy TOMLs inherit a documented
default.

**Acceptance gate** (on the Athena reference job via `inspect calibrate`, per
KB-115's own success criterion): predicted peak-layer moves toward 0, R² rises
above 0.5, correlation ≥ 0.821 (no regression).

**Accepted trade-off:** because this term is area-scaled (not an
area-independent offset), the peak shift depends on the elevation magnitude. If
validation shows the shift is insufficient, the fallback — explicitly flagged
here — is to add the EXPERIMENT-PLAN area-independent `F₀` suction offset
alongside it (the Stage 2 suction term is its natural home).

### Stage 2 — Peel/suction split (KB-185/186)

Replace the single conflated term with `F = F_peelfront + F_suction`.
Parametrize the hardcoded `VACUUM_PRESSURE_KPA = 50.0`
(`services/cavity_detector.rs:104`) into a profile ΔP and wire the
currently-dead `PeelForceCalculator::suction_force` (`:28-34`) through
`cavity_detector`/`SuctionDetector`, removing that dead-code duplication. This is
the structural home KB-185 states "aligns with KB-115: the base-layer force spike
is suction/base-adhesion-dominated."

### Stage 3 — Area/perimeter shape factor (KB-185/186)

Add the `b_perim·P` term (Pan Eq. 13: `F ∝ A/L`), which requires exposing a
**perimeter** on `LayerMask` alongside `solid_area_mm2`. Cheap mid-print
aspect-ratio accuracy — equal-area cylinder 6.16 N vs star 4.9 N (~26% the pure
area model misses).

**Implemented 2026-07-24** (`peel-corrections-s3-perimeter-shape`) — but as the
**KB-185 Tier-1 *multiplicative* form**, not the additive `[a_area·A + b_perim·P]`
above. A single print cannot fit two independent coefficients, so Stage 3 ships a
dimensionless, square-anchored, reduction-only shape factor that *modulates*
σ_peel: `factor = 1 − strength·(1 − min(1, 4·√A / L))` (=1 for a square, <1 for
thin; `strength = 0.5` reproduces the Pan Fig.9 cylinder→star ratio 0.795). It is
opt-in per resin (`ResinProfile::peel_shape_factor_strength`), kept out of the
pure `peel_force` method (KB-114 vectors preserved), threaded from the suction
masks with a fully-solid-placeholder guard, and applied to the peel term only.
`generic_standard` ships an **indicative 0.5** (all other profiles unset →
behaviour-preserving). On the Athena reference print this *improved* the fit
(`inspect calibrate`: correlation 0.948→0.954, single-gain R² 0.562→0.771, peak
layer still 0/0) — promising but single-geometry, so it stays indicative. The
additive `[a_area·A + b_perim·P]` (and per-`(resin, FEP)`-stratum magnitudes)
remain **deferred to the E2b equal-area shape sweep** (`EXPERIMENT-PLAN` §E2b).

### Deferred — blocked on E-series calibration data

These need per-stratum coefficients the single print cannot provide; park until
the campaign yields them:

- Film-type σ_peel table (KB-110/113; EXPERIMENT-PLAN E2b).
- Viscosity μ(T) coupling on the suction term (KB-141; E7) — explicitly **not**
  testable on the isothermal KB-115 print (Δt ≈ −0.24 °C).
- Green-state σ_tensile for support capacity (KB-140; E5) — post-cure values
  overstate the green-state failure load.
- FEP fatigue / cumulative-area factor (E8).
- Motion-strategy reduction levers — rotation/tilt/vibration/interface
  (KB-117/190/192/193) — hardware-specific; only if alternative separation
  kinematics are ever simulated.

## Open questions to resolve with data (not by choosing now)

- **Speed law:** power-law `(v/v_ref)^0.18` (KB-112/114) vs linear F∝V from
  squeeze-film physics (Stefan/Pan, KB-186) — likely regime-dependent (peel vs
  suction), which is itself an argument for the Stage 2 split.
- **ΔP magnitude** is a data gap, 50–101 kPa (KB-184); validate via sealed-vs-
  drained cup pairs (KB-173).
- **Absolute magnitudes do not transfer** across printers — use the scaling laws,
  fit magnitudes per `(resin, FEP)` stratum (KB-185; Athena II is nFEP).
- **Oxygen-reservoir depletion** (~10-layer, ~30-min recovery) is weak/folklore —
  the source experiment failed (KB-116/117). Cite, do not build on it. Note this
  is a caution *about* the very mechanism Stage 1 uses: the σ-relaxation is over
  the first `bottom_layer_count` layers (a per-print effect), NOT a slow reservoir
  recovery.

## Consequences

- Stage 0 unblocks grading for every later stage; it lands first.
- The Stage-1 base term stays out of the pure `peel_force` method, preserving all
  KB-114 test vectors and the `force_properties` area-linearity proptest.
- Validation remains **qualitative** (single print) until the E-series delivers
  coefficients; `inspect calibrate` on the Athena reference job is the standing
  qualitative regression, and `spec/EXPERIMENT-PLAN-v1.1.md` §8 is the
  quantitative gate for publishing per-resin coefficients.
- Each stage is an independent PR; the roadmap is intentionally re-orderable if
  E-series data arrives before Stage 3.

## See also

- KB-115 — the finding and its acceptance criterion (this ADR's Stage 1).
- KB-185 / KB-186 — the two-regime physics synthesis this ADR sequences.
- KB-116 — the oxygen-inhibited release-layer mechanism used for the Stage-1 form.
- KB-114 — the v1 formula being extended.
- `spec/EXPERIMENT-PLAN-v1.1.md` — the calibration campaign that supplies
  coefficients and the quantitative acceptance thresholds.
