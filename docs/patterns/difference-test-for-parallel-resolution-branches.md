---
issue: t2f6-field-inspector
date: 2026-08-01
---

# Pattern: Difference test for parallel resolution branches

## Context

`FieldSlicer::resolve_index` maps one user-facing spelling
(`--slice z=<N>mm`) onto TWO index-resolution semantics selected by field
kind: layer-stacked fields (cure / photoinitiator / strain / stress)
resolve Z through cumulative per-layer heights, while `ThermalField`
resolves Z spatially over the vat envelope
(`(z_mm - bbox_min_z) / voxel_size_mm`). A copy-paste collapse of the two
branches produces plausible-but-wrong slices with no error — the exact
shape of `docs/patterns/anti/voxel-z-step-from-lateral-voxel-size.md`.

## Pattern

Whenever one flag or API dispatches to N parallel resolution/interpretation
branches, write a regression test that asserts the branches produce
**different results for the same input** — with inputs chosen so
coincidental equality is impossible:

- Instantiated: non-uniform layer heights `[50, 30, 20, 40]` µm at
  `z=0.09mm` → cure index 2 vs thermal index 0, asserted with `assert_ne!`
  plus individual expected values. Uniform heights would have let the two
  formulas coincide and the test pass vacuously.

Per-branch correctness tests cannot catch the collapse: each branch's own
test still passes when the other branch quietly routes through it.

## When to use

- One CLI flag / config key with per-variant semantics
- Unit conversions with per-domain bases (part bbox vs vat envelope)
- Any "select interpretation by type" dispatch where interpretations
  overlap on easy inputs

## See also

- `docs/patterns/anti/voxel-z-step-from-lateral-voxel-size.md` — the bug
  class this guards against
- `docs/adr/0023-field-inspector-read-side-contract.md` — the two-Z
  addressing contract
