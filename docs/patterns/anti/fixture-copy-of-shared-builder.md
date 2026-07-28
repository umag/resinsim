---
issue: s3-peel-shape-toml-fieldsim-thermal
date: 2026-07-24
---

# Anti-pattern: a test fixture hand-copied from a shared builder

## What goes wrong

A test module has a shared fixture builder that every test composes
from. One test — usually a new one, written in a hurry, by someone
looking at a *neighbouring* test rather than the builder — inlines its
own copy of the literal instead.

The copy is correct on the day it lands. Then the aggregate gains a new
required field. The builder is updated in one place; the copy is not,
because nothing links them. The copy now constructs an object that is
invalid, and the test asserting it is valid starts failing — or worse,
keeps passing under the configuration you run and fails only under one
you don't.

The failure is attributed to whoever next runs the missing
configuration, months later, who has no context for a fixture they
never touched.

## Concrete example (S3 / ADR-0022 Stage 3)

`crates/resinsim-core/src/entities/resin_profile.rs` has a shared pair:

```rust
legacy_toml_root_without_thermal_thresholds()   // root fields
valid_recipe_table()                            // the [recipe] table
```

Nine tests in the module composed from that pair. Its doc comment even
explains that the root/recipe split exists *specifically* so tests can
insert extra root-level fields before the `[recipe]` table.

`peel_shape_factor_strength_round_trips_through_toml`, added at S3
(7332504, 2026-07-24), inlined its own 24-line literal instead — and
that literal needed exactly the insert-a-root-field capability the split
was built for.

Two months earlier, t2f4 (286b0af, 2026-05-21) had made three thermal
material fields required under `field-sim` (ADR-0020). The shared
builder was updated. The not-yet-existing copy obviously was not, and
when the copy was written it was modelled on the *old* shape. The test
was **born red under `field-sim`** and shipped that way, because config 4
of the ADR-0017 matrix was not run against the new test at S3.

Blast radius when finally run: exactly 1 of 1533 tests. Cheap to fix,
expensive to find.

## Concrete example 2 (UAT suite / uat-fixtures-fieldsim-adr0020-gap)

The same defect, at a much larger blast radius, was found in the
cucumber UAT suite (`crates/resinsim-core/tests/uat_steps/`). That tree
already had two shared builders — `world.rs::ResinBuilder` and
`world.rs::PrinterBuilder` — but they had rotted the same way the S3
unit-test fixture did: t2f4 made three resin fields and four printer
fields required under `field-sim`, and 11 hand-rolled TOML literals
across 9 step-def files had never been updated to match, because nothing
linked them to the builders (some didn't even use the builders — they
were forked before the builders existed).

Two properties made this worse than the S3 instance:

- **Multi-field whack-a-mole.** `validate()` reports only the FIRST
  missing field. Resin needs three fields, printer needs four. Patching
  the field a panic message names costs a full rebuild-and-run round —
  fix one, rebuild, discover the next — so field-by-field patching would
  have cost three-to-four rounds per fixture, and re-created the exact
  duplication the fix exists to remove.
- **Masking.** 5 of the 11 literals were unreachable until an earlier
  step in the same cucumber scenario was fixed: `PrinterBuilder` was
  masked by `ResinBuilder` failing first inside
  `PredictLayerInputs::default_for_test()`, and three inline resin TOMLs
  in `recipe_inside_printer_range.rs` were masked by `printer_with_ranges`
  panicking in the scenario's `Given` step. Fixing the resin side first
  and re-running showed the SAME failure count (12) with one scenario's
  panic message changing from a resin reason to a printer reason —
  proof of the mask, not a stalled fix. Fixing the printer side then
  dropped the count from 12 to 8 and unmasked the first of the three
  `recipe_inside_printer_range.rs` literals in the same step. Only after
  all 11 literals were routed through the builders did the count reach 0.

**Blast radius: 12 of 153 scenarios versus 1 of 1533 unit tests at S3** —
two orders of magnitude more expensive, for the identical root cause,
because here the shared builders THEMSELVES had rotted, not just an
individual fixture that bypassed them.

## Why "just add the missing field" is the wrong fix

It restores green while leaving the tenth copy in place. The next
required field rots it again, identically and silently. The duplication
*is* the defect; the missing field is only how it surfaced this time.

## What to do instead

**Compose from the builder. If the builder can't express your case,
split the builder — don't fork it.**

The rejection-path test here genuinely needed a fixture the builder
deliberately no longer produces (a pre-t2f4 resin with no thermal
fields). The fix was not to hand-roll it, but to extract the shape both
callers share:

```rust
fn legacy_toml_root_pre_t2f4() -> String { /* chemistry fields only */ }

fn legacy_toml_root_without_thermal_thresholds() -> String {
    format!("{}thermal_conductivity_w_mk = 0.20\n\
             specific_heat_j_kgk = 1700.0\n\
             convective_top_h_w_m2k = 10.0\n",
            legacy_toml_root_pre_t2f4())
}
```

Now the chemistry fields have one home. A future required chemistry
field is added once and cannot rot one fixture but not another, and the
thermal delta between the two shapes is visible in a single place
instead of being implied by a diff between two 24-line literals.

## How to catch it

- Grep the test module for raw fixture literals (`r#"` in Rust) before
  adding one. If a builder exists, the burden is on you to justify not
  using it — in a comment, at the call site.
- A literal is legitimate only when the test's *purpose* is the shape the
  builder won't produce. Then extract, don't copy.
- Note that `legacy_toml_missing_recipe_rejected` keeps its own literal
  legitimately: it asserts a **parse** failure and never reaches
  `validate()`, so no validate-time requirement can rot it.

## See also

- `docs/patterns/required-under-feature-via-option-plus-validate.md` —
  the pattern whose validate-time check is what the rotted copy trips.
- `docs/patterns/anti/cfg-test-silent-typos.md` — sibling failure mode:
  test-only code that never compiles under the config you run.
- `spec/uat/cross-feature-toml-interchange.md` — the contract the rotted
  fixture was asserting the opposite of.
