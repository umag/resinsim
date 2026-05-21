---
issue: t2f4-thermal-diffusion
date: 2026-05-21
status: pattern
---

# Pattern: CFL guard on anisotropic finite-difference stencil

## Context

ResinSim's Tier-2 thermal diffusion (`t2f4`) uses an explicit
forward-time centred-space (FTCS) stencil to advance the 3D heat
equation across a dense voxel grid:

```
T_new[i,j,k] = T_old[i,j,k] + dt · α · (
    (T[i+1,j,k] − 2·T[i,j,k] + T[i−1,j,k]) / dx²
  + (T[i,j+1,k] − 2·T[i,j,k] + T[i,j−1,k]) / dy²
  + (T[i,j,k+1] − 2·T[i,j,k] + T[i,j,k−1]) / dz²
)
```

Explicit FTCS is conditionally stable. The Courant–Friedrichs–Lewy (CFL)
constraint for the 3D heat equation is:

```
dt < min(dx², dy², dz²) / (2 · α · 3)
```

Above this `dt_max`, the iteration amplifies high-frequency modes and the
field diverges (typically to ±∞ within ~10–50 steps, depending on grid
size). The factor of 3 in the denominator is the spatial-dimension count;
the factor of 2 is the second-derivative stencil scaling.

## Pattern

Solver step API exposes a separate `cfl_max_dt` helper. Callers compute
`dt_max` before stepping, then use either `dt_max` directly or a
fraction of it:

```rust
impl ThermalDiffusionSolver {
    pub fn cfl_max_dt(alpha_m2_s: f32, dx_m: f32, dy_m: f32, dz_m: f32) -> f32 {
        let min_h_sq = dx_m.min(dy_m).min(dz_m).powi(2);
        0.5 * min_h_sq / (3.0 * alpha_m2_s)
    }

    pub fn step(
        field: &mut ThermalField,
        dt_sec: f32,
        alpha_m2_s: f32,
        bcs: &BoundaryConditions,
    ) -> Result<(), ThermalSolverError> {
        // dt_sec MUST be ≤ cfl_max_dt; caller's responsibility.
        // ...
    }
}
```

The leading `0.5` is a safety margin — the CFL bound is
`< dx² / (2·α·d)` so taking `dt = 0.5 · dx² / (2·α·d) = dx² / (4·α·d)`
leaves headroom for accumulation in mixed Dirichlet + convective BC
ghost-cell updates.

## CFL budget guard

Independent of stability, the substep count per outer layer time can
explode for misconfigured workloads. At 0.05 mm voxel and α = 1.07e-7
m²/s, `dt_max ≈ 4 ms`; a 10-second layer needs ~2500 substeps. Per layer.

ResinSim caps `substeps_per_layer ≤ 1000` and bails with a typed error
when exceeded:

```rust
pub enum ThermalSolverError {
    CflBudgetExceeded {
        dt_max: f32,
        layer_dt: f32,
        substeps_needed: u32,
        hint: &'static str,  // "ADI fallback — see ADR-0020 §Numerical-scheme-choice"
    },
    NonFiniteField,
}
```

The cap chooses "fail loudly with an actionable hint" over "silent
multi-minute hang". The hint points to the documented ADI fallback in
ADR-0020 §Numerical-scheme-choice, which would be unconditionally stable
and unlock sub-100 µm voxel sizes at the cost of GPU-portability.

## Anisotropic spacing in v1

Despite the name, `t2f4` v1 uses **homogeneous spacing** —
`dx = dy = dz = voxel_size_mm × 1e-3`. The "anisotropic" framing is
retained for the future case where the slicer's X-Y voxel pitch (LCD
pixel) differs from the physical Z step. In v1 the ThermalField uses
spatial Z over the vat envelope at the same `voxel_size_mm` as X-Y, so
the stencil collapses to isotropic.

The anti-pattern `voxel-z-step-from-lateral-voxel-size.md` warns
specifically about the OTHER voxel fields (CureField, StrainField) that
use Z = layer count where Z-step is the layer height in µm, NOT the
lateral voxel pitch. ThermalField does NOT have this issue because it
uses spatial Z (see `thermal-field-z-dim-is-spatial.md`).

## See also

- ADR-0020 §Decision i — FTCS choice + ADI deferral rationale
- ADR-0020 §Decision iv — single resin-domain α scope cut
- `docs/patterns/thermal-field-z-dim-is-spatial.md` — why ThermalField
  doesn't share the layer-count Z anti-pattern
- `docs/patterns/anti/voxel-z-step-from-lateral-voxel-size.md` — sibling
  anti-pattern affecting CureField/StrainField but not ThermalField
