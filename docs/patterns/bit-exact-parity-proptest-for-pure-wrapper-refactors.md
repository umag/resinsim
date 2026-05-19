---
issue: t2f2-light-crosstalk-convolution
date: 2026-05-20
---

# Pattern: Bit-exact parity proptest gates pure/wrapper refactors

## Context

A common refactor in DDD-shaped Rust services is to extract a **pure
functional sibling** of an in-place method, then make the in-place
method a thin wrapper. Motivation: the pure form lets callers
post-process the output before deposit (e.g. apply a convolution to
the computed delta), while preserving the convenience of the in-place
form for callers who don't need to.

The risk: the refactor changes the data-flow shape (read fresh from
mutable state in the original; read once into a snapshot then operate
on the snapshot in the pure form), which can subtly diverge in fp
arithmetic, ordering of operations, or short-circuit conditions.

## Pattern

After the refactor, add a `proptest!` block that:

1. Generates a randomised but in-domain input tuple (fixture
   dimensions, indices, scalar parameters).
2. Calls the in-place form on a cloned input fixture.
3. Calls the pure form on a snapshot of the input + applies the
   returned delta manually via the same `add` / `mutate` calls the
   wrapper would have used.
4. Asserts every cell of the resulting state is `f32::to_bits()`
   equal between the two forms.

Use `to_bits()` equality, not `relative_eq!` — the point is that
two FP paths should produce IDENTICAL output, not similar output.
Even a single ULP difference is a regression.

Cap fixture size to a moderate bound (e.g. 8×8×10) so 50 proptest
cases run in well under a second.

## Example (Rust)

```rust
fn run_parity_pair(/* in-domain inputs */) {
    // Form A: in-place wrapper.
    let mut cure_a = CureField::new(...);
    let mut pi_a = PhotoinitiatorField::new(...);
    apply_column_exposure(&mut cure_a, &mut pi_a, ...).unwrap();

    // Form B: pure sibling + manual deposit.
    let mut cure_b = CureField::new(...);
    let mut pi_b = PhotoinitiatorField::new(...);
    let pi_snapshot = pi_b.column_at(ix, iy).unwrap();
    let dose_col = compute_column_exposure(&pi_snapshot, ...).unwrap();
    for iz in 0..nz {
        let d = dose_col[iz as usize];
        if d == 0.0 { break; }
        cure_b.add_dose(ix, iy, iz, d).unwrap();
        pi_b.deplete(ix, iy, iz, k_d, d).unwrap();
    }

    // Assert: bit-exact equality at every voxel.
    for ix in 0..dims.0 { for iy in 0..dims.1 { for iz in 0..dims.2 {
        assert!(cure_a.dose_at(ix, iy, iz).unwrap().to_bits()
             == cure_b.dose_at(ix, iy, iz).unwrap().to_bits());
        assert!(pi_a.concentration_at(ix, iy, iz).unwrap().to_bits()
             == pi_b.concentration_at(ix, iy, iz).unwrap().to_bits());
    }}}
}

proptest! {
    #![proptest_config(/* cases: 50 */)]
    #[test]
    fn parity_apply_vs_compute_proptest(/* generators */) {
        prop_assume!(/* in-bounds guards */);
        run_parity_pair(...);
    }
}
```

## When to apply

- Refactoring an in-place mutator into pure + wrapper forms.
- Splitting a stateful service into a pure functional core + an
  imperative shell.
- Any refactor where the public behavioural contract is "produces
  the same state changes for the same inputs".

## See also

- ADR-0018 §3 (the refactor this pattern gated).
- `docs/patterns/single-source-arrhenius-helper.md` — the
  single-source pattern that motivates pure/wrapper splits in the
  first place.
- `spec/uat/voxel-cure-field-photoinitiator-depletion.md` UAT-6 —
  the user-facing invariant promoted from this proptest.
