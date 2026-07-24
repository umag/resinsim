---
id: KB-198
issue: peel-crack-propagation-tier1
kind: formula
date: 2026-07-24
---

# Interlayer bond is fracture-limited: Kendall crack-front-width knockdown on holding CAPACITY

## Finding

The resin-to-resin **interlayer bond** that holds a freshly-cured normal layer
against separation is **fracture-limited**, not area-proportional. By Kendall
(KB-188) the peel force to advance a crack front is `F ∝ crack-front width b`
(a length), essentially **independent of bonded area**. So the interlayer bond's
effective **holding CAPACITY** falls below the `interlayer_bond_kpa · A` the
BuildPlate model assumed for thin / high-perimeter geometry (walls, edges,
cantilevers) — those layers hold *less* than a pure-area model predicts, and
their safety factor was **over-estimated**.

Tier-1 captures this with the same square-anchored crack-front-width factor that
ADR-0022 Stage 3 (KB-185) applied to the peel LOAD, now applied to the interlayer
CAPACITY:

```
effective_bonded_fraction = min(1, 4·√A / P)        (=1 square, <1 thin, disk clamped)
effective_interlayer_capacity = interlayer_bond_kpa · A · effective_bonded_fraction
crack_front_fraction = 1 − effective_bonded_fraction
```

`4√A/P` is the perimeter of the equal-area square divided by the real perimeter:
`=1` for a square (`P = 4√A`), `<1` for thin/high-perimeter shapes, and clamped
to `1` (reduction-only) for shapes more compact than a square (a disk, `≈1.13`).

## Why this is not double-counting with S3 (KB-116)

S3's `peel_shape_factor` and this knockdown share the `4√A/P` **geometry** but act
on **two physically distinct interfaces** (KB-116):

| Term | Interface | Nature | Direction |
|---|---|---|---|
| S3 peel shape factor | resin ↔ **FEP** release | weak, oxygen-lubricated | reduces the **LOAD** |
| This knockdown | resin ↔ **resin** interlayer | strong crosslinked bond | reduces the **CAPACITY** |

Same fracture-mechanics dependence on crack-front width, applied to two different
interfaces — consistent physics, not double-counting. The net safety-factor effect
on a thin shape is the S3 load reduction (opt-in strength) partially offset by this
full-strength capacity knockdown; the combined magnitude needs E2b validation.

## Square-anchor: no new uncalibrated parameter

The true Kendall interlayer force is `F = b · Gc_interlayer / (1 − cosθ)`, but the
interlayer critical energy-release-rate `Gc_interlayer` and peel angle `θ` are
**unmeasured**. Anchoring the knockdown to the **existing** `interlayer_bond_kpa`
at the square reference (`fraction = 1`) absorbs the unmeasured line-force
`R/(1−cosθ)` into the already-calibrated parameter, so Tier-1 adds **no new
uncalibrated parameter** and is **behaviour-preserving** for square/compact layers.

Note: KB-191's `Gc ≈ 1862 J/m²` (Liravi/Hu bilinear-CZM) is the **constrained-surface
SEPARATION** toughness — the resin↔FEP interface — which is the **wrong interface**
for the resin↔resin interlayer crack; it is cited for context only. A bulk-Griffith
`σ_crit = √(2·E·Gc/h)` criterion with that bulk `Gc` and post-cure modulus is inert
(`σ_crit ≈ 386 MPa ≫ ~13 kPa peel` → never initiates) — the model uses the
phenomenological square-anchored bond strength instead.

## Delamination event

The load enters the model only via a **Delamination** check (and the downstream
safety factor): for a NORMAL layer with a real crack front, when the crack-reduced
interlayer capacity alone drops **below the shaped peel load**, the layer is
predicted to delaminate from the one below (`Severity::Warning`, co-fires with
`SupportOverload`). Because the interlayer bond (~50 kPa) far exceeds the FEP peel
adhesion (~13 kPa), this fires only for genuinely thin geometry — compact/square
layers never delaminate.

## Implication for resinsim — SHIPPED 2026-07-24

`peel-crack-propagation-tier1` (supersedes the closed `peel-crack-propagation`):

- `CrackFront` value object (`values/crack.rs`) — `crack_fraction ∈ [0,1]`.
- `CrackPropagator` service (`services/crack_propagator.rs`) — `effective_bonded_fraction`,
  `crack_from_geometry`, `effective_bonded_area`. Purely geometric (no `Gc`/load/threshold).
- `SupportAnalyzer::assess` scales the interlayer-bond portion of the capacity by the
  bonded fraction for **normal layers only**; bottom-layer plate adhesion (mechanical
  textured-plate interlock) is never knocked down.
- `FailurePredictor::predict_layer` derives the crack from the real per-layer perimeter,
  emits `Delamination`, records `LayerResult.crack_front_fraction` (`Some` only when `>0`).
- `SimulationRunner` threads the real mask perimeter (`build_perimeter_map` + the
  `is_fully_solid` placeholder guard — synthetic/full-bbox masks → no knockdown).

**CAPACITY-ONLY:** the peel LOAD is never touched — force series byte-identical, all
goldens unchanged, the Athena `inspect calibrate` FORCE metrics (corr 0.954 / R² 0.771 /
peak 0) unaffected. Only `safety_factor` + `Delamination` move.

## Tier-2 (post-E-series)

Measure the interlayer `Gc` (an E5-style interlayer-fracture test) and the peel angle
`θ`, then **un-anchor** the magnitude from `interlayer_bond_kpa` and use the true Kendall
`F = b · Gc_interlayer / (1 − cosθ)`. Until then the magnitude is **indicative**.

## Caveats

- Magnitude indicative pending **E2b** (equal-area shape sweep) + an interlayer-fracture
  measurement; ships behaviour-preserving at the square anchor.
- Tier-1 combines the native-precision scalar area with the voxel-precision mask perimeter
  in the `4√A/P` ratio, so a near-square *real* part can record a small spurious
  `crack_front_fraction`. It has **no failure-event consequence** (interlayer bond ≫ peel
  for compact parts) and goldens/calibrate are unaffected (synthetic masks hit the
  `is_fully_solid` guard → `None`). Self-consistency (mask area + mask perimeter) is a
  Tier-2 refinement.
- Steady-state framing: the knockdown captures the fracture-limited *outcome* of the
  periphery→centre crack (KB-191), not a transient time-resolved propagation — an accepted
  Tier-1 simplification.

## See also

- KB-185 — peel-front geometry vs. area (the S3 factor this mirrors on the FEP LOAD).
- KB-186 — verbatim Stefan/Kendall equations + Gc data.
- KB-188 — Kendall thin-film peeling (`F ∝ b`, area-independent) — the physical basis.
- KB-191 — Liravi/Hu bilinear cohesive-zone separation (the 1862 J/m² Gc — the FEP
  interface, cited as context only).
- KB-116 — oxygen-inhibited release layer (weak FEP LOAD vs strong interlayer CAPACITY —
  why the same geometry factor on two interfaces is not double-counting).
- KB-115 — first-layer base-adhesion gap (the base term this sits alongside).
- KB-114 — peel force / support capacity formula (the constant interlayer bond this refines).
- [ADR-0022](../adr/0022-peel-force-model-corrections-roadmap.md) — the staged roadmap
  (S0–S3) whose real-perimeter infrastructure this reuses.
