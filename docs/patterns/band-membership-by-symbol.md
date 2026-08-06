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

**RESOLVED** (`uat-unskip-band-d`, 2026-08-06): the "no single expected
count can be right" limitation above no longer applies. Each register row
is now a `SpecDebt { spec, default_features, field_sim }`, built via
`both_configs(spec, n)` (one count, both columns equal) or `per_config(spec,
d, f)` (a declared config-asymmetric row). A `HarnessConfig` marker —
`HARNESS_CONFIG`, defined by a mutually-exclusive `#[cfg(feature =
"field-sim")]` / `#[cfg(not(feature = "field-sim"))]` attribute pair, never
a bare `cfg!(...)` — selects which column the runtime guard reads. A
cfg-gated module that skips in one config and runs in the other is now
representable directly: its row carries the true count in each column,
rather than forcing one shared number to be wrong in at least one config.

**Columns-equal-by-construction rule**: a row may use `both_configs` for
either of two reasons, and only the runtime-guard layer distinguishes
them:
1. The spec's entry-point symbols have been walked and grepped (this
   pattern's checklist) and are confirmed ungated in both configs, so the
   two counts are equal by DERIVATION.
2. The spec has NO field-sim-gated step-def module in EITHER config at
   all — every scenario is undefined and skips uniformly, so the two
   counts are equal BY CONSTRUCTION, independent of what the eventual
   entry-point symbols turn out to be gated on. Derivation is deferred to
   whichever future increment scopes that spec; `both_configs` is still
   the honest interim shape, not a placeholder that needs pre-emptive
   correction.

The reverse direction is enforced mechanically, not just by convention:
`uat_gherkin.rs`'s layer 1b (`assert_asymmetric_rows_have_a_gated_module`)
fails the build if any row uses `per_config` (i.e. its two columns
differ) without a `FieldSimOnly`-gated module backing the claim — a row
cannot legitimately claim to be case 1's "derived asymmetric" without the
gated module that makes the asymmetry real.

## Sub-shape: asymmetry at the binary-build seam (uat-unskip-c1, 2026-08-04)

CLI specs add a second place the derivation must look. The subprocessed
binary is built by `cli_fixtures::ensure_resinsim_built` with **no
`--features`** — the `cargo uat-field-sim` alias applies features to the
cucumber test binary only, never to the `resinsim` binary under test. So a
CLI scenario whose producing or consuming symbol is
`#[cfg(feature = "field-sim")]` is not config-asymmetric like the
in-process cases above — it is **uniformly unreachable in BOTH configs**
today (`cli-sim-budget-mismatch-on-load`,
`cli-sim-rejects-tampered-sidecar`: the sidecar encode/read paths are
gated, so the binary can neither produce nor consume the fixture the Given
needs). Such specs still land in the register as declared Band-D debt —
they become canonically config-asymmetric the moment
`ensure_resinsim_built` forwards features, which is a design decision the
`uat-unskip-band-d` issue owns, not one a step-def increment may make in
passing. Derivation rule: for CLI specs, check the gating of the symbols
*inside the binary*, then check what features the binary is actually built
with — two seams, both by symbol, never by band label.

**Decision recorded, not yet implemented** (`uat-unskip-band-d`,
2026-08-06): `ensure_resinsim_built` will forward `--features
resinsim-inspect/field-sim` under the same `HARNESS_CONFIG` marker AND
build into a config-scoped `--target-dir`, so the two aliases stop sharing
one `target/<profile>/resinsim` binary — landing the feature forward
without the target-dir split would reintroduce the rebuild flip-flop the
split exists to prevent, so the two land together in one future increment.
Until then, `cli-sim-budget-mismatch-on-load` and
`cli-sim-rejects-tampered-sidecar` stay `both_configs` (uniformly
unreachable) rather than converting to `per_config` — see those two rows'
comments in `uat_gherkin.rs` for the live version of this decision.

## See also

- `docs/patterns/per-spec-runtime-skip-attribution.md` — the register
  this protects
- `uat-unskip-band-d` (filed issue) — the config-aware register design
  that eventually absorbs the demoted specs
