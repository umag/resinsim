# ResinSim Calibration Experiment Plan — Athena II

**Version:** 1.1 (draft)
**Date:** 2026-04-16
**Scope:** End-to-end experimental procedure, metrology, and data pipeline to
calibrate the `resinsim-core` physics models against Athena II ground-truth
measurements.
**Status:** Pre-registration draft — to be frozen before first data collection.

### Changelog

**v1.1 (2026-04-16).** Peel force is not a single resin scalar. Revisions:

- P3/P4 re-scoped: σ_peel is now `σ_peel(resin, FEP_brand, T)`; `f(v_lift)`
  is fit **per resin**, never pooled (§1, §7).
- **E2 amended**: added an exposure-sensitivity sub-block (±20% exposure
  at fixed `A`, `v_lift`) to detect cure-state coupling that would
  otherwise hide inside σ_peel.
- **E2b added**: shape-scaling series (disc / square / ring at matched
  area) to discriminate area-scaling from perimeter-scaling per resin.
  Publishes a chosen functional form (`F = a·A + b·P + suction`) rather
  than forcing `F ∝ A`.
- **E9 validation** now requires at least one held-out **shape** not
  represented in the calibration set (e.g., a thin ring if calibration
  used discs), not only new geometries of the same shape family.
- Manifest extended with FEP brand / type / lot / replacement-date.
- Acceptance criteria tightened: resin profiles must pass a shape-transfer
  test, and any resin whose best-fit form differs from the pooled default
  is published with that form embedded, not silently averaged.

---

## 0. Governance

| Item | Value |
|---|---|
| Owner | ORA physics/sim working group |
| Reviewers | Athena II firmware/mechanical leads; `resinsim` maintainers |
| Pre-registration | This document shall be tagged (`experiment-plan-v1.1`) before any data used for model fitting is collected. Amendments require a new tag and explicit rationale. |
| Data location | `resinsim/data/athena/<session-id>/` (raw) + `resinsim/data/resins/<resin>/` (canonical fit outputs) |
| Reporting | One Markdown + JSON report per experiment, archived in `data/athena/reports/` |

**Ground rule:** raw sensor data is immutable. All cleaning, filtering, and
exclusion decisions live in versioned analysis scripts; no in-place edits.

---

## 1. Research questions & calibration targets

Each sim parameter below must be estimated with a stated uncertainty. The code
locations are listed so every experiment can be traced to a specific
`resinsim-core` value or entity.

| ID | Parameter | Unit | Code reference | Experiment |
|---|---|---|---|---|
| P1 | Penetration depth `Dp` | µm | `values/cure_depth.rs`, `ResinProfile::penetration_depth_um` | E1 |
| P2 | Critical energy `Ec` | mJ/cm² | `ResinProfile::critical_energy_mj_cm2` | E1 |
| P3 | Peel coefficients `σ_peel(resin, FEP, T)`, per-shape factors `(a_area, b_perim)` | kPa, N/mm | `values/force.rs` (peel force model), `ResinProfile::peel_adhesion_kpa` — **not transferable across resins or FEP brands** | E2, **E2b** |
| P4 | Lift-speed factor `f_resin(v_lift)` | dimensionless | `PeelForce = a_area·A·f(v_lift) + b_perim·P·f(v_lift) + suction` — form fit **per resin** | E3 |
| P5 | Suction / hydrodynamic term | N | Same model | E4 |
| P6 | Tensile strength `σ_tensile` | MPa | `ResinProfile::tensile_strength_mpa`, used by `SupportCapacity` | E5 |
| P7 | Linear shrinkage | % | `ResinProfile::linear_shrinkage_pct` | E6 |
| P8 | Viscosity `η(T)`, activation energy `Ea` | mPa·s, kJ/mol | `ResinProfile::viscosity_mpa_s`, `activation_energy_kj_mol` | E7 |
| P9 | FEP fatigue / cumulative-area effect | dimensionless | Not yet modelled — E8 decides whether to add | E8 |
| V | Model end-to-end accuracy | — | All of the above | E9 (validation) |

**Hypotheses (pre-registered).** For each parameter we commit in advance to the
functional form assumed by `resinsim-core` and state the null hypothesis that
the single-printer calibrated fit generalises within its stated uncertainty
band (see §8 acceptance criteria).

**Peel-force model (v1.1, explicit).** The v1.0 form `F = σ·A·f(v) + suction`
was a first-order approximation. v1.1 treats the functional form itself as
testable:

```
F_peak(resin, FEP, T, A, P, v_lift, d_lift) =
    [ a_area(resin,FEP,T) · A  +  b_perim(resin,FEP,T) · P ] · f_resin(v_lift)
  + F_suction(η(T), A, v_lift, d_lift)
```

`a_area` vs `b_perim` is resolved empirically by E2b. Resins where
`b_perim · P` dominates are fracture-mechanics-controlled; resins where
`a_area · A` dominates are bulk-adhesion-controlled; the plan publishes
whichever the data selects, per resin.

---

## 2. Metrology & traceability

Every instrument has a named calibration record. No uncalibrated instrument
produces data that reaches the fit step.

| Instrument | Quantity | Resolution | Calibration | Check cadence |
|---|---|---|---|---|
| Athena II build-plate force sensor | N | manufacturer spec (TBD; record) | Factory + in-situ zero before every print | Per print |
| UV radiometer, 405 nm | mW/cm² | ±3% | NIST-traceable certificate, annual | Per session |
| Digital micrometer | µm | 1 µm | Gauge-block check | Weekly during campaign |
| Caliper | 0.01 mm | ±0.02 mm | Gauge-block check | Weekly |
| Rotational viscometer (external) | mPa·s | per spec | Manufacturer standard fluid | Per session |
| Thermocouple + logger | °C | ±0.1 | Ice-point / boiling-point check | Pre-campaign |
| RH/Temp hygrometer | %RH, °C | ±2% / ±0.3 | Factory | Pre-campaign |
| Tensile tester (UTS) | N | per spec | Calibrated load cell | Per session |
| Analytical balance | g | 0.001 | Reference mass | Per session |

**Force-sensor characterisation (must be completed before E2 starts).**
1. Noise floor: 60 s zero-load recording → compute σ of baseline.
2. Drift: 30 min zero-load recording → fit linear drift; declare drift budget.
3. Linearity: apply 5 reference weights spanning 0–50 N, 3× each, fit line, report residuals.
4. Hysteresis: load-then-unload at 5 levels; record asymmetry.
5. Repeatability: 10 consecutive 10 N loads; report CV.

These five numbers become the stated sensor uncertainty that propagates into
every downstream fit.

---

## 3. Environmental controls & confounders

| Confounder | Control |
|---|---|
| Ambient T | Log °C at 1 Hz; abort if drift >2 °C during a session |
| Humidity | Log %RH; exclude sessions >60 %RH (moisture uptake) |
| Resin temperature | Stir + 30 min warm-up before session; log vat °C |
| Resin age / bottle | Record bottle open-date + batch; cap ≤90 days post-open |
| Resin mixing | Invert bottle 20× before pour; re-stir vat every 2 h |
| FEP film state | Record peel-cycle count (`fep_cycles`), **brand**, **type** (nFEP/PFA/standard), **lot**, install-date; replace at fixed interval; log tension check. σ_peel is FEP-dependent — changing brand mid-campaign voids pooling across the change-point. |
| LCD hours | Record panel hours; exclude above manufacturer limit |
| Printer warm-up | 15 min idle after power-on before first exposure |
| Post-cure | Standardised: Anycubic/Form Cure equivalent, 10 min, 60 °C |
| Operator | One named operator per session; dual-operator sessions require both initials |

Every session writes a `manifest.yaml` (§6) capturing all of the above.

---

## 4. Experimental-design principles

These apply to every experiment in §5. No experiment may deviate without an
explicit documented reason in its sub-section.

- **Randomisation.** Within a session, print order of conditions is
  randomised with a recorded seed. Print-order effects (FEP wear, panel heat,
  resin depletion) would otherwise alias with the variable of interest.
- **Replication.** Each condition receives `n ≥ 3` prints *within* a session
  and the full session is repeated on `k ≥ 3` independent days, on the same
  printer (for repeatability) and on ≥2 printers where feasible
  (for reproducibility).
- **Blocking.** Session = block. Print unit = nested block. Mixed-effects
  model (§7) explicitly accounts for session and unit as random effects.
- **Control specimen.** A fixed reference coupon (10 mm² disc, 50 µm layer,
  2 mm/s lift, 5 s exposure at manufacturer-recommended energy) is printed
  once at the start, mid, and end of every session. Drift across the three
  control prints is the session's stability metric.
- **Negative control.** For the optical experiment, one patch per session is
  exposed for 0 s — must yield no cured material; if it does, the session is
  voided.
- **Power analysis.** Required `n` is set per experiment from pilot noise
  estimates (§5) targeting α = 0.05, power = 0.8, and a stated minimum
  detectable effect size. Campaigns start with a pilot of the minimum
  protocol and revise `n` before the main runs.
- **Blinding.** Instrument reads are objective, so blinding of measurement is
  not required. However, the analyst performing the fit must not see the
  "expected" KB-100/KB-110 reference values during the data-entry step;
  expected values are overlaid only at the review stage.
- **Pre-registered analysis plan.** §7 is frozen at plan tag time; post-hoc
  analyses are permitted but explicitly flagged as exploratory in the report.

---

## 5. Individual experiments

Each experiment has: **(a)** objective, **(b)** hypothesis, **(c)** variables,
**(d)** geometry and print settings, **(e)** procedure, **(f)** replication,
**(g)** acquisition format, **(h)** analysis.

### E1 — Jacobs working curve (Dp, Ec)

- **Objective.** Estimate `Dp` (penetration depth) and `Ec` (critical energy)
  for each resin, matching the Beer-Lambert form used in
  `services/cure_calculator.rs`: `Cd = Dp · ln(E / Ec)`.
- **Hypothesis.** For a given 405 nm radiometer-verified source,
  cured-film thickness vs `ln(E)` is linear over the range `Ec < E < 10·Ec`.
  Slope = `Dp`; x-intercept = `Ec`.
- **Variables.**
  - Independent: exposure time `t` ∈ {0, 0.5, 1, 2, 4, 8, 16, 32} s at the
    calibrated panel irradiance `I₀` (mW/cm², measured at start of session).
    Energy dose `E = I₀ · t`.
  - Dependent: cured-film thickness `h` (µm).
  - Controlled: resin, resin T, single-patch geometry (10 × 10 mm square),
    FEP state, panel hours.
- **Geometry.** Single-layer exposure onto clean glass placed on the build
  plate with a drop of resin; each patch is fully isolated (no multi-layer
  prints); 0 s patch on every session as negative control.
- **Procedure.**
  1. Record `I₀` with radiometer at 5 build-plate positions; use the mean, log the CV.
  2. Generate randomised patch order.
  3. Expose each patch; uncured resin drained and IPA-rinsed (fixed rinse time 30 s) to preserve the true cured thickness.
  4. Air-dry 2 min; measure `h` with micrometer at 5 points per patch → median.
  5. Post-cure NOT applied (measurement is of green-state thickness).
- **Replication.** 3 patches per exposure × 3 resin batches × 3 sessions ≥ 27 per condition.
- **Acquisition.** `data/athena/<session>/E1/patches.csv`
  (columns: `patch_id, resin, batch, t_s, I0_mw_cm2, h_um_m1..h_um_m5, operator`).
- **Analysis.** Weighted linear regression of `h` on `ln(E)` with per-session
  random intercept (mixed-effects). Reject the resin's profile if R² < 0.95.
  Report `Dp ± 95% CI`, `Ec ± 95% CI`. Compare to KB-100/KB-101 expected
  ranges in the review step.

### E2 — Peel force vs layer area (baseline peel coefficient)

- **Objective.** Estimate the area-scaling coefficient `a_area(resin, FEP, T)`
  from peak peel force vs layer area, at fixed lift speed, lift distance,
  and cure state. v1.1 note: this captures area-scaling only; perimeter
  scaling is resolved in E2b and the two are jointly re-fit afterwards.
- **Hypothesis.** At fixed `v_lift`, `d_lift`, exposure, and steady-state
  FEP, `F_peak ≈ a_area · A + F_suction(A)` for a disc geometry where
  perimeter scales with `√A` and so cannot be separated from area in a
  single-shape series — hence E2b.
- **Variables.**
  - Independent: flat-disc area `A` ∈ {5, 10, 20, 40, 80} mm².
  - Controlled: `v_lift` = 2 mm/s, `d_lift` = 5 mm, layer height = 50 µm,
    exposure = manufacturer-recommended, resin T = 25 ± 1 °C,
    FEP cycles < 500 (fresh film block).
  - Dependent: force-sensor time series at ≥100 Hz (Athena native rate;
    upsample rejected — record native and log).
- **Geometry.** Single-layer disc on a 0.5 mm raft with integral lift handle,
  printed centre of build plate to minimise panel-edge effects. Fixture STL
  checked into `data/athena/fixtures/peel_disc_vN.stl`.
- **Procedure.**
  1. Session warm-up, radiometer & force-sensor zero.
  2. Control print (reference coupon).
  3. Randomised order across areas, 5 replicates each.
  4. Between prints: 60 s rest (FEP relaxation); vat skim & re-level.
  5. Control print at mid-session and end.
- **Replication.** 5 per area × 3 sessions = 15 per area. Pilot first: 3 × 3 to confirm variance estimate, then re-compute required `n` for MDE of 10% on σ_peel.
- **Acquisition.** Per print:
  - `force.csv` (columns: `t_ms, F_N`) — full time series.
  - `print.json` — derived summary: `F_peak`, `F_settled`, `t_peak`, `impulse`.
  - `manifest.yaml` — session-level metadata.
- **Analysis.** Segment the force trace to isolate the peel phase
  (detect start = sensor derivative crosses +threshold; end = force returns
  to baseline ±2 σ_noise). Fit `F_peak = a_area · A + F₀` with
  mixed-effects **per resin**; `F₀` is the small-area intercept assumed to
  be suction. Cross-check against E4. **Do not pool across resins.**

#### E2-exposure — cure-state coupling (sub-block of E2, same session)

- **Objective.** Check whether `a_area` is itself a function of exposure
  energy — i.e., whether green-state modulus / conversion at the
  FEP-interface layer shifts peel force at fixed geometry.
- **Variables.** Exposure ∈ {-20%, nominal, +20%} of manufacturer-recommended
  energy; fixed `A` = 20 mm²; fixed `v_lift` = 2 mm/s; fixed `d_lift` = 5 mm.
- **Procedure.** 5 replicates per exposure level per session, randomised into
  the E2 run queue.
- **Analysis.** Per-resin regression of `F_peak` on exposure. If slope is
  stat-sig (α = 0.05) and |Δ F_peak / F_peak| > 10% across the ±20% range,
  publish `a_area` with an **exposure disclaimer** stating the range of
  validity; do not extrapolate in the sim outside that range.

### E2b — Shape-scaling (area vs perimeter)

- **Objective.** Discriminate whether peel force scales with area,
  perimeter, or a mix, per resin — i.e., fit
  `F_peak = a_area · A + b_perim · P + F₀` and test which coefficient
  dominates.
- **Hypothesis.** For each resin, one of three outcomes:
  (i) `a_area`-dominated (bulk adhesion), (ii) `b_perim`-dominated
  (fracture-mechanics / crack propagation), (iii) mixed. Expectation:
  stiff/brittle resins trend perimeter-dominated; tough/flexible trend
  area-dominated — but we publish whatever the data shows.
- **Variables.** Geometry ∈ three shapes at three matched areas, giving a
  wide range of `P/A`:
  - **Disc** r=5.64 mm → A=100, P=35.4 → P/A=0.35
  - **Square** 10×10 mm → A=100, P=40 → P/A=0.40
  - **Thin ring** r_out=8 mm, r_in=5.96 mm → A≈90 (matched), P≈88 → P/A≈0.98
  Plus one large disc (A=400) and one thin bar (2×50 mm, A=100, P=104,
  P/A≈1.04) to extend the perimeter-axis lever arm.
  Fixed `v_lift` = 2 mm/s, `d_lift` = 5 mm, layer = 50 µm, nominal exposure.
- **Procedure.** As E2 but with the shape as the varied factor; fresh-FEP
  block; 5 replicates per shape per session × 3 sessions per resin.
  Randomised print order with recorded seed.
- **Acquisition.** Same schema as E2, with a `shape` field in
  `print.json` (`disc` | `square` | `ring` | `bar`) plus numeric
  `area_mm2` and `perimeter_mm`.
- **Analysis.** Per resin, jointly fit `a_area`, `b_perim`, `F₀` against
  all E2 + E2b prints (pooled within a resin, mixed-effects on session).
  Report (a) point estimates and CIs for each coefficient, (b) a
  **dominance metric** `D = (a_area·Ā) / (a_area·Ā + b_perim·P̄)` at the
  median geometry of the set — publish which regime the resin sits in.
  Model-selection: compare full two-term fit vs area-only and
  perimeter-only by AICc; preferred form is embedded in the resin's
  profile TOML as a `peel_model = "area" | "perimeter" | "mixed"` tag
  plus the relevant coefficients.

### E3 — Peel force vs lift speed (per-resin f(v_lift))

- **Objective.** Characterise the lift-speed factor in the v1.1 peel model
  **per resin**. Pooling across resins is forbidden.
- **Hypothesis.** `f_resin(v_lift)` follows a power or log form
  (candidates: `1 + k·v_lift`, `v_lift^α`, `1 + k·ln(v_lift/v_ref)`).
  Competing forms are scored by AICc **within each resin**.
- **Variables.** `v_lift` ∈ {0.5, 1, 2, 4, 8} mm/s, fixed shape (disc, so
  directly comparable to E2 intercept), fixed `A` = 40 mm², fixed
  `d_lift` = 5 mm, exposure as E2.
- **Procedure.** As E2 with lift-speed the varied factor. Randomised order; 5 replicates per speed per session × 3 sessions **per resin**.
- **Acquisition.** Same schema as E2.
- **Analysis.** Fit all three candidate forms with mixed-effects per resin;
  report the preferred form, its AICc weight, and parameters with CIs.
  Persist the chosen form **with the resin**, not as a global default.
  If AICc weight of the top form is < 0.7 within a resin, publish both
  top forms with a warning that lift-speed dependence is poorly
  constrained for that resin.

### E4 — Suction / lift-distance (hydrodynamic term)

- **Objective.** Isolate the hydrodynamic/suction component of the peel
  force — resin must flow back into the gap as the plate lifts.
- **Hypothesis.** At fixed `A` and `v_lift`, force decays with gap `d`
  consistent with a Stefan-style `F ∝ η · v · A² / d^n`. Determine `n` and
  the prefactor.
- **Variables.** `d_lift` ∈ {3, 5, 8, 12, 16} mm; fixed `A` = 40 mm²,
  `v_lift` = 2 mm/s.
- **Procedure.** As E2 with lift-distance the varied factor. Note that
  the integrated force curve (from detach through settle) is the target, not
  only `F_peak` — the decay shape carries the suction information.
- **Acquisition.** Full force trace.
- **Analysis.** Two-stage: (i) subtract the σ_peel·A·f(v_lift) term
  estimated from E2/E3; (ii) fit the residual decay vs gap. Report `n`,
  prefactor, and whether the residual is consistent with the measured
  viscosity from E7 (sanity check linking the two experiments).

### E5 — Tensile strength of cured and post-cured specimens

- **Objective.** Estimate `σ_tensile` in all three print orientations for use
  by `SupportCapacity = σ_tensile · π · r_tip² · N_supports`
  (see `values/force.rs`).
- **Hypothesis.** Anisotropy (Z vs XY) is ≤ 25% after standardised post-cure.
- **Variables.** Orientation ∈ {XY, XZ, Z}; post-cure ∈ {none, standard}.
  Specimen: ASTM D638 Type V dogbones.
- **Procedure.** Print n = 5 per cell. Post-cure per §3. 24 h conditioning at
  23 °C, 50 %RH. Test on UTS at 5 mm/min until break. Record load-displacement curve.
- **Acquisition.** `E5/specimens.csv` + raw UTS curves per specimen.
- **Analysis.** One-way ANOVA across orientations per resin × post-cure
  state; report mean σ_tensile, CV, and anisotropy ratio. Store the
  worst-case orientation as the `resinsim` default (conservative for support
  capacity calculation).

### E6 — Shrinkage (linear, time-dependent)

- **Objective.** Estimate `linear_shrinkage_pct` and its time evolution.
- **Hypothesis.** Post-print shrinkage in the first 7 days follows a
  logarithmic relaxation; most change (>80%) occurs within 24 h.
- **Variables.** Measurement time `t` ∈ {0.1, 1, 24, 72, 168} h post-print.
- **Procedure.** Print 20 mm calibration cube (n = 5). Measure X, Y, Z with
  caliper at each `t` (same operator). Store at controlled 23 °C / 50 %RH
  between measurements.
- **Acquisition.** `E6/cubes.csv`.
- **Analysis.** Report shrinkage at 24 h and at 168 h; fit log decay; flag
  if Z shrinkage differs from XY by >1 percentage point.

### E7 — Viscosity and Arrhenius

- **Objective.** Estimate `η(T_ref)` and activation energy `Ea` feeding
  `η(T) = A · exp(Ea/RT)`.
- **Variables.** Vat temperature `T` ∈ {20, 25, 30, 35} °C.
- **Procedure.** External rotational viscometer (printer has no native
  viscometer); sample resin from the vat, measure in triplicate per `T`.
  Allow 15 min thermal equilibration.
- **Acquisition.** `E7/viscosity.csv`.
- **Analysis.** Linear regression of `ln(η)` on `1/T` → slope gives `Ea/R`.
  Report `η` at the 25 °C reference and `Ea` in kJ/mol.

### E8 — FEP fatigue / cumulative-area effect

- **Objective.** Quantify whether peel force increases with cumulative
  peeled-area on a single FEP film — i.e., whether `f(cycles)` is a
  material factor we must add to the model.
- **Variables.** FEP cycle count `N` over 0 → 2000 prints of the E2
  reference disc; all other factors held constant.
- **Procedure.** Replace FEP at `N = 0`; print reference disc continuously;
  log every 50th print's force trace in full, every other print as summary
  stats only (to conserve disk).
- **Acquisition.** `E8/fep_fatigue.csv` plus sampled force traces.
- **Analysis.** Regress `F_peak` on `N`; test H0: slope = 0. If |slope|
  exceeds 5% per 1000 cycles, add a `fep_cycle_factor` to the peel-force
  model and re-fit E2/E3 with film-age as a covariate.

### E9 — End-to-end validation (blind)

- **Objective.** Confirm the calibrated model predicts peel force on
  *unseen* geometries within the stated uncertainty.
- **Procedure.** After all parameters are frozen, print a set of held-out
  validation geometries. The set **must** include:
  1. At least one shape **not represented in E2/E2b** for that resin
     (e.g., a cross, a star, a spiral, or a perforated plate). This is the
     shape-transfer test; it is the real test of whether the v1.1 peel
     model generalises beyond the calibration shape family.
  2. A lift-speed not in the E3 grid (e.g., 3 mm/s) — interpolation test.
  3. A temperature displaced by ≥3 °C from the calibration T (if E7-fit
     viscosity is propagated into the suction term) — extrapolation test.
  4. A realistic print: overhanging bridge series, tapered support raft,
     high-aspect-ratio tower, and a fleet layout simulating multi-part
     layers.
  The operator running the prints is given only the geometries, not the
  predictions. A second analyst runs `resinsim` on the same geometries and
  files predicted peak forces to git before any measured data is recorded.
  Measured vs predicted is compared only after both files are committed
  (signed commits required).
- **Acceptance.** See §8. Failing the shape-transfer test specifically
  means the resin's published model is flagged `transfer = untested` in
  its TOML and consumers warned not to extrapolate to unseen geometries.

---

## 6. Data pipeline

**Directory layout.**

```
resinsim/data/athena/
  <session-id>/
    manifest.yaml          # environment, printer sn, operator, resin, FEP
    E1/patches.csv
    E2/<print-id>/force.csv + print.json
    ...
    checksums.sha256       # SHA256 of every file in this session
  reports/
    <experiment>-<date>.md
    <experiment>-<date>.json
```

**Session ID** = `YYYY-MM-DD-<printer-sn>-<session-seq>`
(e.g., `2026-05-02-A2-0007-01`).

**Manifest schema.**

```yaml
session: 2026-05-02-A2-0007-01
date_utc: 2026-05-02T09:12:00Z
operator: [initials]
printer:
  model: Athena II
  serial: A2-0007
  firmware: <ver>
  lcd_hours: 412
  fep_cycles_at_start: 238
  fep_brand: "Anycubic"
  fep_type: "nFEP"            # nFEP | standard FEP | PFA | ACF
  fep_lot: "2026-03-W10"
  fep_installed_utc: 2026-04-28
resin:
  name: "Siraya Tech Blu"
  batch: "240312-A"
  opened_utc: 2026-04-20
environment:
  ambient_c_start: 22.4
  ambient_c_end: 22.7
  rh_start: 41
  rh_end: 43
  vat_c: 25.1
radiometer:
  mean_mw_cm2: 4.92
  cv_pct: 2.1
  n_positions: 5
randomisation_seed: 98317
protocol_tag: experiment-plan-v1.0
```

**Immutability.** Raw files are `chmod 444` after session. All subsequent
processing reads from `data/athena/<session>/` and writes to
`data/athena/reports/` — never back.

**Ingest.** A `resinsim-inspect` (or successor) subcommand validates a
session folder against the schema and refuses to accept malformed data.

---

## 7. Statistical analysis plan (pre-registered)

**Primary model.** For each dependent variable `y` (e.g., `F_peak`, `h`):

```
y_ijk = β₀ + β·x_ijk + u_i (session) + v_ij (print) + ε_ijk
```

where `x` is the experimental variable, `u_i ~ N(0, σ²_s)` is the
session-level random intercept, `v_ij ~ N(0, σ²_p)` the per-print random
effect, and `ε_ijk` residual error. Fits done with maximum-likelihood; CIs
from parametric bootstrap (≥1000 resamples).

**Stratification (v1.1, forbidden pooling).** For peel-force
analyses (E2, E2b, E3, E4, E8), the fit is run **independently within each
`(resin, FEP_brand+type)` stratum**. Crossing a resin boundary or an
FEP-brand boundary without a separate fit is explicitly forbidden and will
be caught by the analysis pre-commit hook (schema enforces a `stratum_key`
on every published coefficient). Temperature is treated as a covariate
when the session-to-session variation exceeds 2 °C within a stratum.

**Joint peel-model fit (E2 + E2b + E3).** Per stratum, the full model

```
F_peak = [ a_area·A + b_perim·P ] · f_resin(v_lift) + F_suction(A, v, d)
```

is fit jointly against the pooled E2 + E2b + E3 data for that stratum,
after E4 has determined the suction functional form. Coefficients are
reported with covariance so downstream error propagation in the sim can
respect coefficient correlation.

**Exclusions (pre-registered).**
- Any print whose control-coupon σ_peel drifts >20% from the session's
  first control voids the session.
- Any force trace with sensor baseline noise >3× the characterised noise
  floor is excluded.
- Sessions with ambient drift >2 °C or vat drift >1 °C are excluded.

**Model selection (E3 only).** AICc ranking among the three candidate
`f(v_lift)` forms; winning form published regardless of which it is
(negative-result commitment).

**Uncertainty propagation.** Parameter uncertainties carried forward to the
simulation: `resinsim-core` profiles store point estimates plus a `_ci95_*`
interval field so downstream code can run best/worst-case bounds.

---

## 8. Acceptance criteria

A resin profile is accepted for publication when:

| Criterion | Threshold |
|---|---|
| E1 Beer-Lambert R² | ≥ 0.97 |
| E2 + E2b joint peel-model fit R² (per stratum) | ≥ 0.90 |
| E2b shape-dominance metric `D` uncertainty (95% CI width) | ≤ 0.25 — i.e., the regime (area / perimeter / mixed) is identifiable, not merely within error |
| E2-exposure cure-state coupling | If |Δ F_peak / F_peak| > 10% across ±20% exposure, `a_area` is published with an exposure-range caveat, not as a bare scalar |
| E3 chosen `f_resin(v_lift)` AICc weight vs runners-up (per resin) | > 0.7 |
| E9 shape-transfer test | |predicted − measured|/measured ≤ 20% on the held-out shape (looser than 15% on seen shapes, to account for genuine extrapolation) |
| E5 tensile CV within orientation | < 15% |
| Between-session CV of the control coupon | ≤ 15% |
| E9 validation |mean predicted−measured|/measured | ≤ 15% on every held-out geometry |
| Traceability | Every published number has a session-id path back to raw data |

Failing any threshold triggers either (a) more replicates (if variance is
the issue) or (b) a model refinement PR against `resinsim-core` — the
experiment plan itself is not silently amended.

---

## 9. Risks & mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| FEP wear confound | High | High | E8 explicitly; fresh-film blocks for E2/E3 |
| Resin batch variance | Medium | High | Record batch; reference resin every session |
| Panel LCD degradation during campaign | Medium | Medium | Log hours; radiometer each session |
| Force-sensor drift | Medium | High | Per-print zero; sensor-characterisation audit before every sub-campaign |
| Operator variance | Medium | Medium | Single named operator per session; checklist |
| Photoinitiator sensitivity to light exposure during measurement | Low | Medium | Amber-safelight handling; fast measure-rinse |
| Post-cure oven variability (E5) | Low | Medium | Oven thermocouple log; fixed position for specimens |
| Geometry-printability bugs in DragonFruit CTB output | Medium | Low | Fix slicer settings frozen & version-pinned |
| Firmware update mid-campaign | Low | High | Freeze firmware for duration; log version |

---

## 10. Pre-registration commitments

1. This plan is tagged `experiment-plan-v1.1` in git before any data used for
   model fitting is collected. Amendments produce `v1.2+` with a
   human-readable change log. The v1.0 → v1.1 revision was driven by a
   design review noting that peel force is resin-, FEP-, T-, and
   geometry-coupled and cannot be reduced to a single scalar σ_peel — the
   revision is documented at the top of this file.
2. All raw data (including voided sessions) is committed. Nothing is deleted.
3. Null and negative findings are published in the report (negative-result
   commitment).
4. Pilot data informs `n` only; it is not used in the main fit.

---

## 11. Timeline (illustrative, 8 weeks)

| Week | Activity |
|---|---|
| 1 | Instrument calibration; force-sensor characterisation (§2); fixture STLs; ingest-script dry run |
| 2 | Pilot (small-`n` E1+E2 on one resin) → revise `n` for main runs |
| 3 | E1 main runs × 3 resins |
| 4 | E2 + **E2b** (shape series) + E2-exposure main runs × 3 resins |
| 5 | E3 (per-resin) + E4 main runs |
| 6 | E5 + E6 + E7 |
| 7 | E8 (FEP fatigue) + parameter freeze |
| 8 | E9 validation + publication |

---

## 12. Open questions for Athena II engineering

Items requiring answers from the hardware team before runs start. Track in
an issue thread; do not start collection until each has a recorded answer.

1. Exact sample rate of the force sensor (Hz) and is the rate fixed or
   configurable per firmware version?
2. Sensor noise floor, drift spec, linearity spec (for §2 reference numbers).
3. Is there any firmware-side filtering (low-pass, smoothing) applied to the
   CSV output, and can it be disabled for calibration?
4. Is the build-plate tilt / uneven FEP tension compensated anywhere, or is
   raw sensor Z-axis force what we read?
5. What is the recommended FEP replacement interval for peel-force stability
   (vs leak-safety, which is typically the documented number)?
6. Is there an API/CLI hook for reading the force trace during a print
   (for live monitoring of a session), or only post-print CSV?

---

## 13. Deliverables from this experiment campaign

- Populated `data/resins/<resin>.toml` files for ≥3 resins with all
  parameters in §1 plus uncertainty bands.
- `data/athena/` sessions for every run (raw + manifest).
- Per-experiment report (Markdown + machine-readable JSON) under
  `data/athena/reports/`.
- A `resinsim-core` change-log PR adding any new parameter discovered
  necessary by the data (candidates: `fep_cycle_factor`, a chosen
  `f(v_lift)` form, a viscosity-coupled suction term).
- A short paper / blog post publishing the full methodology and dataset.

---

*End of plan v1.0.*
