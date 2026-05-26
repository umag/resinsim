---
id: KB-180
issue: resinsim
kind: data-gap
date: 2026-04-16
source: gap analysis
---

# Missing Dp/Ec for popular consumer resins

## Gap

Elegoo, Anycubic, Siraya Tech, and Phrozen do not publish Dp (penetration depth) or Ec (critical energy) for their resins. These are the most widely used consumer resins. Without Dp/Ec, the Beer-Lambert cure depth model (KB-103) cannot be applied to these resins.

Only Liqcreate (KB-100) and academic studies (KB-101) have published values.

## Affected resins (high priority)

| Resin | Users | Dp/Ec available? |
|-------|-------|-----------------|
| Elegoo ABS-Like V2 | Very high | No |
| Elegoo Standard Grey | Very high | No |
| Anycubic Standard | High | No |
| Siraya Tech Fast | High | No |
| Siraya Tech Blu | Medium | No |
| Phrozen Aqua Grey | Medium | No |

## Athena II experiment

**Method:** Jacobs working curve measurement (follows NIST protocol, KB-102)

1. Fill Athena II vat with target resin
2. Expose a series of squares at increasing exposure times (e.g., 1s, 2s, 3s, 5s, 8s, 12s, 20s)
3. Measure cured thickness of each square with digital calipers or micrometer
4. Plot Cd (µm) vs. ln(E) where E = I₀ × t_exposure
5. Linear fit: slope = Dp, x-intercept at Cd=0 gives Ec

**Requirements:**
- UV meter to measure I₀ (mW/cm²) at Athena II LCD surface
- Digital calipers (±10µm)
- Controlled ambient temperature (record it)

**Output:** TOML profile for each resin:
```toml
[resin]
name = "Elegoo ABS-Like V2"
penetration_depth_um = ???
critical_energy_mj_cm2 = ???
measurement_wavelength_nm = 405
measurement_temperature_c = 25
```
