---
issue: uat-unskip-a2
date: 2026-08-03
---

# Pattern: Verify UAT band membership by symbol, not by label

## Context

The unskip campaign's band labels classified
`calibration-disclosure-3of3-predicate` and
`honest-zero-yield-fraction-on-calibrated-solid` as Band A
(default-features). A2's planning verified the EXACT entry-point symbols
each scenario needs and found both are Band D:
`FailurePredictor::predict_strain_failures` — the sole WarpingRisk
producer — is `#[cfg(feature = "field-sim")]`, and
`voxel_yield_fraction`/`strain_magnitude_max` are populated only inside
the runner's cfg block. 7 of 11 scoped scenarios were unreachable; the
filed scope was invalid.

## Pattern

Before scoping or authoring any step-def module:

1. Enumerate every Given/When/Then and name the exact production symbol
   each observes.
2. `grep -n '#\[cfg(feature' ` the files owning those symbols — the
   scenario's band is the UNION of its symbols' gates, regardless of what
   the spec's subject matter suggests.
3. Record the grep evidence in the module doc (the A2 sim-json module is
   the worked example), so the check is auditable and the next increment
   repeats it.

Why it matters here: `SPECS_WITHOUT_STEP_DEFS` is one `const` shared by
`cargo uat` and `cargo uat-field-sim`; a cfg-gated module skips in one
config and runs in the other, so no single expected count can be right —
landing a mislabelled spec breaks the identical-shape invariant, not just
one test.

## See also

- `docs/patterns/per-spec-runtime-skip-attribution.md` — the register
  this protects
- `uat-unskip-band-d` (filed issue) — the config-aware register design
  that eventually absorbs the demoted specs
