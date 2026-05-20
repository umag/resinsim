---
issue: t2f3.1-post-impl-calibration-followups
date: 2026-05-20
---

# Anti-pattern: cfg(test)-only typos slip past cargo build

## What goes wrong

Code inside `#[cfg(test)]` (or `#[cfg(any(test, …))]`) is NOT
compiled by `cargo build` or `cargo build --workspace`. It IS
compiled by `cargo test`, `cargo test --no-run`, and `cargo nextest
run`. A typo, duplicate field, stale literal, or broken match arm
inside test-only code can therefore:

- Be introduced in a commit
- Pass code review (if review doesn't run tests)
- Pass `cargo build` CI gates
- Pass `cargo clippy` if clippy is invoked without `--all-targets`
- **Ship.**

When the next agent runs `cargo nextest run --workspace`, the build
fails. The original author is long gone; the diagnostic points at a
line in a test fixture they may have never directly touched.

## Concrete example (t2f3.1)

t2f3 shipped with a duplicate `voxel_yield_fraction: None,` field at
`crates/resinsim-viz/src/main.rs:2688-2689`:

```rust
strain_magnitude_max: None,
stress_von_mises_max_mpa: None,
strain_gradient_max_frac: None,
voxel_yield_fraction: None,
voxel_yield_fraction: None,   // ← duplicate
};
```

The duplicate is inside an `#[cfg(test)] fn smoke_exit_…` test.
`cargo build --workspace` passes. `cargo nextest run --workspace`
fails with `error[E0062]: field 'voxel_yield_fraction' specified
more than once`. Discovered during t2f3.1's Step 7 matrix gate and
fixed inline as an unblocker.

## How to apply

1. **Ship gate must include test-code compilation.** The matrix in
   `agent-constraints/implementation-conventions.md` is the canonical
   defence — it explicitly calls out:
   - `cargo build --workspace` ← does NOT compile test code
   - `cargo nextest run --workspace` ← DOES compile and run test code
   - `cargo clippy … --all-targets` ← `--all-targets` is load-
     bearing; without it clippy skips test code
2. **Never declare ship-readiness on `cargo build` alone.** Either
   the nextest run passes or the change isn't ready.
3. **`#[cfg(test)]` is not a hiding place.** Treat test-code
   correctness with the same scrutiny as production code — especially
   for fixture literals that copy production struct shapes.

## Detection in review

Spot-check during PR review: any change that adds new fields to a
domain entity or value object should be cross-referenced against ALL
`#[cfg(test)]` blocks (and integration test files) that construct
literals of that type. The new field needs to appear in each fixture
exactly once. A grep for the new field name across the codebase
surfaces the call sites; visual inspection catches duplicates.

## Related

- `agent-constraints/implementation-conventions.md` 4-config matrix
  — the canonical defence; configs (3) and (4) exercise nextest
  specifically.
- `docs/patterns/anti/bare-matches-as-test-assertion.md` — sibling
  silent-green anti-pattern that also lives in test code.
