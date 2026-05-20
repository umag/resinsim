---
issue: t2f3-shrinkage-strain-stress
adr: ADR-0018
date: 2026-05-20
---

# KB-163: Photopolymer Young's modulus and Poisson's ratio defaults

## Summary

`ResinProfile` gains `youngs_modulus_mpa: Option<f32>` and
`poissons_ratio: Option<f32>` in t2f3 (ADR-0018 decision 3). When
either is `None`, the strain/stress pipeline falls back to the
literature midpoints defined here. The producer
(`FailurePredictor::predict_strain_failures`) MUST disclose the
uncalibrated-moduli caveat in any emitted `FailureEvent.message` so
users can distinguish calibration-artefact emissions from real
physics.

## Defaults

```rust
pub const DEFAULT_YOUNGS_MODULUS_MPA: f32 = 2000.0;
pub const DEFAULT_POISSONS_RATIO: f32 = 0.35;
```

## Literature anchors

### Young's modulus (DLP-acrylate photopolymer)

- **Anycubic Tough Resin (vendor data, 2024)**: 2150 MPa.
- **Elegoo Standard Resin V2 (vendor data, 2024)**: 1850 MPa.
- **Liqcreate Premium Black (vendor data, 2023)**: 2050 MPa.
- **Elegoo ABS-Like V2 (vendor data, 2024)**: 2300 MPa (tougher).
- **Elegoo Ceramic Grey V2 (vendor data, 2024)**: 2500 MPa (ceramic
  reinforcement raises stiffness significantly).
- **Engineering nylon-like resins** (Loctite 3D 3843, BASF Ultracur3D
  Tough): 1800–2400 MPa.

Midpoint: 2000 MPa. Uncertainty band: ±50% covers the range from
soft-flex resins (~1000 MPa) up to high-modulus ceramic-filled
formulations (~3000 MPa). Within "standard DLP-acrylate" the band
narrows to ±20%.

### Poisson's ratio (DLP-acrylate photopolymer)

Vendor data is sparse — ν is rarely published. Literature for cured
DLP-acrylate resins centers on 0.35 with a ±0.05 band:

- **Crivello & Reichmanis 2014** (radical-cure
  photopolymer review): typical post-cure ν ≈ 0.34–0.38 for the
  class of UV-cured acrylate networks used in SLA/DLP.
- **Lee et al. 2018** (mechanical characterisation of SLA-printed
  parts): ν = 0.36 ± 0.03 measured by digital image correlation
  on standard photopolymer specimens.
- **Engineering thermoset (epoxy)** for cross-reference: ν ≈ 0.30–0.38.

Midpoint: 0.35. Validator hard limit: strictly < 0.5 (incompressible
singularity); strictly > -1.0 (theoretical lower bound).

## Per-resin recommendations

For the four resin TOMLs that ship with resinsim:

| Resin | youngs_modulus_mpa | poissons_ratio | Notes |
|-------|-------------------|----------------|-------|
| `generic_standard.toml` | 2000.0 | 0.35 | Midpoints — typical SLA resin baseline |
| `generic_abs_like.toml` | 2300.0 | 0.38 | Tougher acrylate; ν slightly higher |
| `elegoo_ceramic_grey_v2.toml` | — (None) | — (None) | Ceramic filler diverges; calibrate via Athena II |
| `liqcreate_premium_black.toml` | — (None) | — (None) | No vendor-published values; uses defaults |

The Elegoo Ceramic Grey V2 omission is **deliberate** — the
ceramic filler significantly raises stiffness above the photopolymer
literature midpoint. Encoding a guess (e.g. 2500 MPa from the vendor
publication for a sibling product) would silently propagate as if it
were measured. The TOML carries a `# TODO calibrate via Athena II`
comment.

## Uncalibrated-moduli caveat in FailureEvent.message

When `resin.has_calibrated_moduli() == false` (either field is
`None`), every emitted `WarpingRisk` and `CohesiveFailure` message
ends with:

```text
(uncalibrated moduli — magnitude has ±50% uncertainty, see KB-163)
```

This satisfies the round-2 plan-review MEDIUM finding: WarpingRisk
threshold-crosses computed against literature-midpoint moduli may
trip spuriously (or miss real warping) by ±50%, and the user
needs the disclosure to distinguish "real physics says warp" from
"uncalibrated model says might warp."

## Calibration path

Athena II tensile measurement on a printed test bar (preferably the
ISO 527 Type 1B dogbone geometry, or the resinsim-bundled
`tests/fixtures/tensile_bar.stl` once that exists) directly yields
E and ν. The follow-on workflow:

1. Print the test bar with the candidate resin profile.
2. Run Athena II tensile + DIC capture (E from slope, ν from
   transverse strain ratio).
3. Edit the resin TOML to populate `youngs_modulus_mpa` and
   `poissons_ratio`.
4. `resin.has_calibrated_moduli()` flips to true; the caveat
   disappears from emitted FailureEvents.

A dedicated follow-on issue (`athena-tensile-calibration-workflow`,
TBD) will document the procedure in detail.

## References

- KB-140 — Tensile strength range (downstream consumer of E).
- KB-161 — Cure-extent → free-shrinkage strain (upstream input
  to the stress pipeline).
- KB-162 — Linear-elasticity stress accumulator (consumer of these
  defaults).
- ADR-0018 — t2f3 design decisions (anchor for this KB).
- `feedback_review_before_pr.md` — calibration is a separately
  scoped follow-on, NOT bundled with t2f3.
