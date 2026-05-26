---
id: KB-153
issue: resinsim
kind: formula
date: 2026-04-22
source: Literature midpoint — radical photopolymerization Arrhenius kinetics; polymer chemistry textbooks
---

# Cure-kinetics Arrhenius Ec(T) correction

## Equation

Beer-Lambert cure depth with temperature-dependent critical energy:

```
Ec(T_K) = Ec_ref × exp((Ea_cure_J / R) × (1/T_K - 1/T_ref_K))
Cd      = Dp × ln(E / Ec(T_K))
```

Where:
- `T_K = vat_temp_c + 273.15` — absolute vat temperature.
- `T_ref_K = ref_temp_c + 273.15` — absolute reference temperature at which
  `Ec_ref` was measured (sourced from `ResinProfile.reference_temp_c`).
- `Ea_cure_J = Ea_cure_kj_mol × 1000.0` — explicit unit conversion,
  mirrors `ThermalCalculator::viscosity_at_temperature`.
- `R = 8.314 J/(mol·K)` — gas constant.

## Sign check

`T_K > T_ref_K` ⇒ `1/T_K − 1/T_ref_K < 0` ⇒ `exp(negative) < 1` ⇒
`Ec(T_K) < Ec_ref` ⇒ deeper cure at elevated vat temperature.

This matches radical-polymerization Arrhenius rate physics: warmer ⇒ faster
initiation ⇒ less energy needed to cross the gel threshold. Tested in
`cure_properties.rs::ec_decreases_with_temperature`.

## Default Ea_cure = 30 kJ/mol (LITERATURE-MIDPOINT ESTIMATE)

> ⚠ **This is an ESTIMATE, not a measurement.** Radical photopolymerization
> kinetics sit in the 15–50 kJ/mol range per standard polymer-chemistry
> textbooks. Common 405 nm initiators cluster:
> - TPO-L: ~20–25 kJ/mol
> - Irgacure 819: ~30 kJ/mol
> - BAPO: ~35–40 kJ/mol
>
> 30 kJ/mol is a defensible midpoint. Downstream cure-drift predictions
> using this default may be wrong by ±50%. Per-resin measurements update
> `ResinProfile.cure_kinetics_ea_kj_mol` in each TOML as calibration data
> arrives — no separate lifecycle issue is needed because the variable is
> a first-class field, not a missing capability.

When `ResinProfile.cure_kinetics_ea_kj_mol = None`, the simulator uses
`DEFAULT_CURE_KINETICS_EA_KJ_MOL = 30.0` and the CLI emits a loud stderr
warning. The warning SHOULD appear consistently across:

- `resinsim inspect thermal` stderr (table mode) and
  `"cure_kinetics_ea_is_default": true` in JSON mode.
- `resinsim report health` stderr (non-JSON).
- This KB entry.
- `ResinProfile::cure_kinetics_ea_kj_mol` doc comment.
- `CureCalculator::cure_depth_at_temp` doc comment.

If any surface drops the estimate-only framing, downstream users may treat
30 kJ/mol as measured.

## Test vectors

All vectors: `Dp = 170 µm`, `Ec_ref = 5.0 mJ/cm²`, `T_ref = 25 °C`,
`Ea_cure = 30 kJ/mol`.

| Vat °C | 1/T_K − 1/T_ref_K | exp(exponent) | Ec(T) | Cd for E = 10 mJ/cm² |
|--------|------------------|---------------|-------|----------------------|
| 23 (cold) | +2.26 × 10⁻⁵ | 1.085 | 5.43 mJ/cm² | 170 × ln(10/5.43) = 104.0 µm |
| 25 (ref)  | 0              | 1.000 | 5.00 mJ/cm² | 170 × ln(10/5.00) = 117.8 µm |
| 30 (warm) | −5.50 × 10⁻⁵ | 0.819 | 4.09 mJ/cm² | 170 × ln(10/4.09) = 151.9 µm |
| 40 (hot)  | −1.61 × 10⁻⁴ | 0.559 | 2.79 mJ/cm² | 170 × ln(10/2.79) = 216.5 µm |

Ratios (vs `Ec_ref`):
- `Ec(23) / Ec(25) ≈ 1.085` (8.5% inflation at cold end).
- `Ec(30) / Ec(25) ≈ 0.819` (18% drop at typical steady-state).
- `Ec(40) / Ec(25) ≈ 0.559` (44% drop at hot plateau).

Regression test: `services/cure_calculator.rs` unit tests +
`tests/cure_properties.rs` proptests. The Arrhenius 1/T-space symmetry
property test derives `T2_K` from `1/T2_K = 2/T_ref_K − 1/T1_K` and asserts
`Ec(T1) × Ec(T2) = Ec_ref²` within 1e-3 relative tolerance — a clean test
of log-linearity in 1/T.

## ResinProfile field

```
cure_kinetics_ea_kj_mol: Option<f32>  // None → use default with warning
```

- Serde default: `None` (legacy TOMLs parse unchanged).
- Validation bound: `(0.0, 200.0]` kJ/mol when Some — covers all
  photoinitiator kinetics + any reasonable radical polymerization Ea.
  Rejects zero, negative, non-finite, and absurd values.
- Stored in `crates/resinsim-core/src/entities/resin_profile.rs`;
  accessor `cure_kinetics_ea_kj_mol()` returns the raw `Option`;
  accessor `effective_cure_kinetics_ea_kj_mol()` returns the value or
  the 30 kJ/mol default (for downstream consumers that don't care about
  the distinction).

## Limitations

1. **No measurement ships today.** All four resin TOMLs
   (`generic_standard`, `elegoo_ceramic_grey_v2`, `liqcreate_premium_black`,
   `generic_abs_like`) use the 30 kJ/mol default. First calibration
   opportunity: exposure-finder test strips (KB-171) at multiple ambient
   temperatures.

2. **Single global Ea.** A real formulation has separate Ea values for
   initiator decomposition, monomer propagation, and termination — the
   Ec lump conflates them into one number. Adequate for Phase-1 cure-depth
   predictions; inadequate for predicting post-cure gel conversion or
   shrinkage kinetics.

3. **Ignores photoinitiator depletion over long exposures.** `Ec` is
   treated as a constant at each layer; in reality initiator concentration
   drops in over-exposed regions. For typical 2–3 s normal exposure this
   is negligible; may matter for bottom layers at 25–30 s.

## References

- ADR-0007 — architectural record (Ec(T) wired alongside two-stage
  thermal).
- KB-150 — vat thermal formula (upstream — supplies `vat_temp_c` to the
  Ec(T) call).
- KB-R152 — two-stage LED → vat thermal (sibling).
- KB-141 — viscosity Arrhenius (analogous temperature correction on the
  other Arrhenius-governed property).
- KB-103 — Beer-Lambert cure-depth formula (the underlying equation that
  Ec(T) plugs into).
