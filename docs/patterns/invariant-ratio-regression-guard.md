---
issue: peel-corrections-s1-base-adhesion
date: 2026-07-23
---

# Pattern: Rebuild an absolute-value regression guard as an invariant ratio

## Context

A regression test often pins an **absolute** output value to defend a specific
bug — e.g. `report_health_athena_ii_uses_toml_stiffness` asserted the 60 mm cube's
`max_z_deflection ≈ 31 µm` and treated `>100 µm` as "the silent generic-`k=460`
fallback regressed." That threshold silently encodes the *inputs* of the day the
test was written (peel-only force at `k=1500`).

When a NEW physics term changes the absolute value, such a guard breaks in a
misleading way. ADR-0022 Stage 1 added a first-layer base-adhesion term (KB-116),
so the cube's layer-0 force went `46.8 → 190.8 N` and the *correct* `k=1500`
deflection became `127 µm` — now indistinguishable from, and colliding with, the
old ">100 µm = bug" semantics. The guard wasn't wrong about stiffness; it was
asserting the wrong quantity.

## Pattern

Assert the **invariant the test actually cares about**, not a magnitude that an
unrelated term can move. The stiffness guard's real invariant is
`z_deflection = force / z_stiffness`, so:

```
stiffness = max_total_force / max_z_deflection   // ≈ 1500 N/mm for Athena II
```

is independent of how large the force is. The rebuilt guard asserts
`1400 ≤ stiffness ≤ 1600`; a `k=460` fallback still reads ≈460 and fails loudly,
but the base term (and any future coefficient refit) no longer perturbs it. Both
operands (`max_total_force`, `max_z_deflection`) already sit at the same layer in
the report, so the ratio is exact.

## Why

- **Survives orthogonal physics changes.** The guard defends *stiffness
  resolution*; base adhesion, speed factor, or a Δσ₀ refit change the force but
  not the ratio.
- **Fails for the right reason only.** The absolute-threshold form conflated
  "wrong stiffness" with "bigger force"; the ratio isolates the former.
- **Self-documenting.** `force/deflection == stiffness` states the physical law
  the test defends, rather than a numeric constant whose provenance decays.

## When NOT to

If the test genuinely defends an absolute budget (a byte size, a latency ceiling,
a memory cap), keep the absolute assertion — the number *is* the contract. This
pattern is for guards that pin a magnitude only as a proxy for an invariant.

## See also

- `golden-file-byte-identity-guard.md` — the complementary case (regenerate the
  golden when the output legitimately changes).
- ADR-0022 Stage 1; `crates/resinsim-inspect/tests/profile_loader_cli.rs`.
