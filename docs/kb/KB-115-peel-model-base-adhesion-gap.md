---
issue: nanodlp-import
date: 2026-07-05
---

# KB-115: v1 peel-force model under-weights first-layer base adhesion

## Finding

Running `resinsim inspect calibrate` on the real Athena reference job
`PFA-75mm-unsplit-spike` (1499 layers, generic_standard resin, athena_ii
profile) surfaced a systematic model gap between the simulated and the measured
peel force.

| | Simulated (v1, area-driven) | Measured (Athena FSS, T=6) |
|---|---|---|
| Peak layer | **15** (cross-section-area peak, 1427 mm²) | **0** (base) |
| Layer 0 | 17.85 N | 13 983 counts (the maximum) |
| First 5 layers | rising (17.85 → 19.06 N) | falling (13983 → 10721) |
| Tail | → 0 N at the tip | → 0 at the tip |

Shape correlation is **0.821**, but a single counts→Newton gain fit yields
**R² ≈ 0**: the real force has an extreme first-layer spike (13 983 vs ~181
mean) that the area-proportional model does not reproduce. See KB-110
(film peel stress), KB-111 (peak force), KB-114 (peel force formula).

## Interpretation

resinsim's v1 peel model treats peel force as ≈ proportional to cross-section
area, so it peaks where area peaks. The real Athena is dominated over the first
layers by **base-plate adhesion + initial suction**, independent of the exact
cross-section — hence the real maximum at layer 0. The area model is a good
*shape* proxy (corr 0.821) but wrong on the *location and magnitude* of the
peak.

## Candidate fix (follow-up)

Add a base-adhesion / initial-suction term to `PeelForceCalculator`, weighted to
the first N layers (N ≈ bottom_layer_count), decaying as the raft releases. Use
`calibrate` on this reference job as the regression fixture: success = the
predicted peak moves toward layer 0 and the gain-fit R² rises above 0.

The staged plan for this fix — the chosen oxygen-freshness σ-relaxation form
(KB-116), the peel/suction split it slots into, and the harness prerequisites —
is recorded in [ADR-0022](../adr/0022-peel-force-model-corrections-roadmap.md).

## Outcome (Stage 1 — gap closed 2026-07-23)

ADR-0022 Stage 1 shipped the KB-116 oxygen-freshness σ-relaxation base term
(area-scaled; a separate `PeelForceCalculator::base_adhesion_force`; opt-in per
resin). On this exact reference job (Δσ₀ = 40 kPa, `generic_standard`):

- predicted peak layer **15 → 0** (offset +15 → +0; now matches the real peak);
- shape correlation **0.821 → 0.948**;
- counts→N gain-fit **R² 0.000 → 0.562**.

All three success criteria from "Candidate fix" are met. The Δσ₀ magnitude is
INDICATIVE — one print, R² 0.56 — a calibration coefficient ADR-0022 defers to
the E-series for proper per-stratum fitting; the value shipped on
`generic_standard` is the best current estimate, not a fitted constant. Stage 2
(peel/suction split, area-independent F₀) is the complement where an area-scaled
term alone is geometry-limited.

## Provenance

Evidence: `inspect calibrate --file PFA-75mm-unsplit-spike.nanodlp` (2026-07-05).
Δt was −0.24 °C (isothermal print — no active heating), so thermal effects are
not confounding this comparison.
