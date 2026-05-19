---
issue: t2f1-voxelized-cure-distribution
date: 2026-05-19
---

# KB-160: Photoinitiator depletion model

## Status

Active — ships with t2f1 (ADR-0017). Default constants are literature
midpoints; per-resin fitting deferred to a future calibration ticket.

## Scope

This KB documents the depletion kinetics that govern how UV exposure
consumes the photoinitiator in vat photopolymer resin, and the constants
the t2f1 voxel cure path uses by default. Companion to KB-103
(Beer-Lambert primitive) and KB-153 (Ec(T) Arrhenius correction).

## Physics

### Standard radical-photopolymer depletion

For a voxel `(x, y, z)` exposed to UV intensity `I(x, y, z, t)`:

```
dC(x, y, z, t)/dt = -k_d × I(x, y, z, t) × C(x, y, z, t)
```

Where:

- `C` is the local photoinitiator concentration as a dimensionless
  fraction of the initial concentration (`C₀ = 1.0` everywhere at
  `t = 0`, by convention).
- `I` is the local UV intensity, in mW/cm². Inside a vat-photopolymer
  voxel column under LCD pixel `(i, j)`, `I(x, y, z, t)` follows
  Beer-Lambert depth attenuation:
  `I(x, y, z, t) = I_pixel(i, j) × exp(-z_in_layer / Dp_local(t))`
- `k_d` is the **photoinitiator decay rate constant**, in units of
  `1 / (mJ·cm⁻² × concentration-fraction)`. Resin-specific.

This is the first-order kinetic form for radical photopolymer
photolysis. For an exposure of duration `Δt` with constant intensity
`I`, the analytical solution is:

```
C(t = Δt) = C₀ × exp(-k_d × I × Δt) = C₀ × exp(-k_d × dose_local)
```

where `dose_local = I × Δt` is the absorbed dose at that voxel during
the exposure (mJ/cm²).

### Coupling to cure depth via effective Dp

`Dp` (penetration depth) is the depth at which intensity drops to 37 %
(`= 1/e`) of the surface value. Physically:

```
Dp = 1 / (ε × C × ln(10))   (Beer-Lambert with molar absorptivity ε)
```

So `Dp ∝ 1 / C` — as photoinitiator depletes, `Dp` rises (light reaches
deeper), and a given exposure cures deeper than the un-depleted
calculation predicts.

`VoxelCureCalculator` updates `Dp_local(x, y, z, t)` at each integration
step using the current `C(x, y, z, t)` from `PhotoinitiatorField`,
maintains a per-voxel cumulative dose in `CureField`, and recomputes
whether the voxel has crossed `Ec(T_layer)` (using `Ec(T)` from
KB-153, delegated to `CureCalculator::ec_at_temp`).

### Invariants

Two invariants are enforced at every accessor of `PhotoinitiatorField`:

1. `0 ≤ C(x, y, z) ≤ C₀` for all voxels at all times. Depletion is
   monotonically non-increasing; recombination chemistry is not
   modelled (the next exposure starts where the previous one left off,
   never above).
2. Once `C(x, y, z) < C_threshold` (default 0.01 = 1 % of initial),
   that voxel is treated as "burnt out" — further exposures still
   contribute to `CureField` dose but `Dp_local` clamps at a maximum
   value (10× `Dp₀`) to avoid divide-by-near-zero. The 1 % threshold
   is a numerical floor, not a physical claim — at 1 % concentration
   the resin behaves like a depleted photoinitiator solution and the
   simulation is in extrapolation territory.

## Default constants

| Constant | Default | Source | Uncertainty |
|----------|---------|--------|-------------|
| `C₀` (initial concentration fraction) | **1.0** | Convention — `PhotoinitiatorField` starts uniform at 1.0 everywhere. ResinProfile's `photoinitiator_concentration_initial` overrides per-resin. | Exact by definition. |
| `k_d` (decay constant) | **0.05** `(mJ/cm²)⁻¹` | Literature midpoint for TPO at 405 nm in standard methacrylate-acrylate vats. See [Photoinitiator Selection and Concentration in Photopolymer Formulations, PMC](https://www.ncbi.nlm.nih.gov/pmc/articles/PMC9268840/) and Jacobs working-curve fits in the t2f1 triage research log. | **±50 %** band. Per-resin measurement will narrow significantly; ResinProfile's `photoinitiator_decay_constant_k_d: Option<f32>` exists for exactly this. The default emits a loud warn on use, mirroring KB-153's `cure_kinetics_ea_kj_mol = None` precedent. |
| `C_threshold` (burnt-out floor) | **0.01** (1 % of initial) | Numerical floor to prevent divide-by-near-zero in `Dp_local = Dp₀ / C`. Below this, voxels are still tracked in `CureField` but `Dp_local` clamps. | Not physically calibrated — change with care. |
| `Dp_local_max_factor` (clamp) | **10.0** (× `Dp₀`) | Beyond this, the simulation is in extrapolation territory (the photoinitiator is so depleted that the resin no longer reasonably resembles itself). | Not physically calibrated — same caveat. |

## Why the default is loud

Following the KB-153 / `cure_kinetics_ea_kj_mol` precedent
(`ResinProfile.cure_kinetics_ea_kj_mol: Option<f32>` with a loud warn
when fallen back to the default 30 kJ/mol literature midpoint), the
photoinitiator decay constant defaults emit a warning on every CLI run
and report-rendering pass when the resin's TOML does not carry a
measured value. The wording mirrors the KB-153 pattern:

```
warning: resin '<name>' has no measured photoinitiator_decay_constant_k_d;
using KB-160 literature midpoint 0.05 (mJ/cm²)⁻¹ with ±50 % uncertainty.
Long-print cure-drift predictions may be off by up to ±50 %.
```

## Calibration path (not in scope for t2f1)

To fit `k_d` for a specific resin: print a calibration tower of
identical layers under fixed exposure; extract per-layer cure depth via
a microscope or laser profilometer; back-solve `k_d` from the observed
late-layer deepening relative to a Tier-1 (no-depletion) prediction.
A future calibration ticket may automate this. For now, expect the
default ±50 % band.

## Memory consequences

`PhotoinitiatorField` is a dense `Array3<f32>` with the same dimensions
as `CureField` (see ADR-0017 §3). Doubles the memory footprint of t2f1
relative to a cure-only field. Quantizing `C` to `u8` (256 levels,
~5 % accuracy loss) is an escape hatch reserved for the sparse-storage
follow-on; v1 ships f32.

## References

- ADR-0017 (this issue) — voxel cure field design decisions; KB-160 is
  the depletion-physics single source of truth referenced from there.
- KB-103 — Beer-Lambert per-column cure-depth primitive
  (`CureCalculator::cure_depth`, `intensity_at_depth`).
- KB-153 — Ec(T) Arrhenius correction (consumed unchanged by t2f1 via
  `CureCalculator::ec_at_temp`).
- KB-141 — viscosity Arrhenius (single-source helper pattern that
  KB-160 follows for default-fallback warnings).
- [Photoinitiator Selection and Concentration in Photopolymer Formulations (PMC)](https://www.ncbi.nlm.nih.gov/pmc/articles/PMC9268840/)
  — depletion-constant order-of-magnitude.
- `data/elegoo/README.md` — calibration data lineage (LED-temperature
  telemetry exists; photoinitiator decay calibration does **not** yet).
