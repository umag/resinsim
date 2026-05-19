---
issue: t2f1.5-voxel-cleanup
date: 2026-05-19
---

# Anti-pattern: bare `matches!()` in test body is compile-only

## The mistake

```rust
let err = my_fallible_call().expect_err("expected an error");
matches!(err, MyError::SpecificVariant { .. });  // <-- compiles, does nothing
```

`matches!()` is an EXPRESSION returning `bool`. In a test body, the
returned bool is discarded — the call has zero runtime effect. The test
passes whether the variant matched or not. The line LOOKS like a real
assertion but is a no-op.

This is a particularly nasty bug shape because:

- The compiler is happy.
- Tests pass.
- The variant name appears in the test, so reading the test you'd
  believe the variant is checked.
- The check silently rots: if production code changes the variant, no
  test fails.

## How to spot it

Grep for `^\s*matches!\(`:

```bash
git grep -n 'matches!' -- '*.rs' | grep -v 'assert!\|debug_assert!\|let \|if \|return '
```

Each hit is a candidate for an unwrapped `matches!()`. If the
surrounding line is just `matches!(...);` with no consumer, it's the
bug shape.

## The fix

Always wrap in `assert!`:

```rust
assert!(
    matches!(err, MyError::SpecificVariant { .. }),
    "expected SpecificVariant, got {err:?}"
);
```

The `{err:?}` failure message names the actual variant returned when
the test fails — much faster to debug than "matches!() returned false".

## Real-world surface

In t2f1.5 (one lifecycle, one file), four pre-existing tests had this
shape across `voxel_cure_calculator.rs`. Each one had been promoted
from "stub assertion during TDD red phase" and never tightened. Spotted
only when extending the same enum during F2 — without the variant-split
work the bug would have stayed indefinitely.

## Lint help

`clippy::let_underscore_must_use` does NOT catch this (`matches!()` is
not `must_use`-annotated). No stable clippy lint flags bare `matches!()`
in 2026-05; manual grep + review is the current safeguard.

## Related

- ADR-0003 (unwrap-policy) — same theme: assertion-shaped code that
  doesn't actually assert.
- `debug-assert-as-release-guard.md` — adjacent "looks like a guard,
  isn't in release" anti-pattern.
