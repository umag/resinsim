---
issue: athena-tdd-coverage
date: 2026-08-01
---

# Anti-pattern: Implementing audit criteria that name deleted symbols

## The failure shape

A backlog/audit item pins its acceptance criteria to concrete symbols and
paths ("at least one proptest covering `force_stats` invariants in
`tests/property/athena_properties.rs`; fixture in `data/athena/`"). Time
passes. The named symbols are deleted or superseded — here, commit
`300be16` (ADR-0021) deleted `ForceRecord`, `force_stats()` and
`filter_layers()` and replaced them with the tall analytic parser — but the
audit item's text never learns this.

An implementer who follows the letter of the criteria then either
(a) rebuilds deleted API to have something to test, or (b) bolts the tests
onto whatever similarly-named thing exists now, covering the wrong
contract. Both pass a naive "acceptance criteria met" check.

## The rule

Before implementing any aged audit/backlog item:

1. **Verify every named symbol and path against the current tree** —
   `git log -S<symbol>` finds the deletion commit if there is one.
2. **Re-derive the item's INTENT onto the successor API** and write the
   mapping down (here: `force_stats`'s "mean in [min,max], std_dev >= 0"
   became bounds properties on `AnalyticLog::channel_mean`, `LayerForce`,
   and the `ComparisonReport`/`ProfileOverrides` metrics; `filter_layers`
   monotonicity became extractor index-ordering plus the
   `filter_layer_range` range-inclusion property).
3. **Surface the letter-vs-intent mismatch at the approval gate** as an
   explicit deviation to ratify — never silently reconcile.

## Detection

- Acceptance criteria older than the last major refactor of their area
- Named test paths that do not exist (`tests/property/` here)
- "Empty pending data" claims about directories that are in fact untracked

## See also

- `docs/patterns/anti/adr-pattern-doc-drift-from-iterated-values.md` —
  the sibling drift shape for documented values
- `docs/patterns/orphan-as-inspiration-not-transcription.md` — same
  re-derive-don't-transcribe principle for orphaned code
