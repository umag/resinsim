---
issue: printsim-add-layer-result-api
date: 2026-04-26
---

# Pattern: Promoting an aggregate panic invariant to a typed `Result`

## Context

An aggregate-root method enforces an invariant via `assert_eq!` /
`panic!` — fine in v0 when the only caller is internal and provides
the invariant by construction, less fine when the public API surface
grows. The natural promotion is to `-> Result<(), AggregateError>`
with a typed enum variant for the violated invariant.

The migration looks small (one `assert_eq!` → one `if + return Err`)
but has at least five gates that the first plan draft typically
misses. This pattern lists them so the second draft hits all five
on the first try.

## Pattern

### Gate 1: Co-locate AND re-export the new error enum

The error type lives next to the producer (mirrors `MaskError` in
`values/layer_mask.rs`, `CavityError` in `services/cavity_detector.rs`).
But callers reach the producing aggregate via a `mod.rs` re-export
(`pub use print_simulation::PrintSimulation;`) — the new error type
must be re-exported on the same path so callers don't have to know
about the inner module:

```rust
// crates/<crate>/src/<aggregate-mod>/mod.rs
pub use print_simulation::{AggregateError, PrintSimulation};
```

Without the re-export, callers reach `AggregateError` via the longer
`crate::<crate>::<aggregate-mod>::<inner-mod>::AggregateError` path,
breaking the convention.

### Gate 2: Match sibling error enums byte-for-byte

Look at the workspace's other `thiserror::Error` enums. If they all
carry `#[derive(Debug, Clone, PartialEq, Error)]` (no `Eq`, no
`#[non_exhaustive]`), match them. Adding `#[non_exhaustive]` "for
future-proofing" a single-variant enum introduces a workspace-novel
attribute with zero immediate API benefit — defer until a second
variant actually lands. Adding `Eq` when siblings stop at `PartialEq`
is similar drift.

`assert_eq!(err, AggregateError::Variant { ... })` works on `PartialEq`
alone — `Eq` adds nothing tests need.

### Gate 3: Pin the operation order in the plan

The new method body has a fixed ordering that is NOT obvious from
type signatures alone:

```rust
pub fn add_layer(&mut self, result: LayerResult, ...) -> Result<(), AggregateError> {
    let expected = self.layers.len() as u32;          // 1. capture
    if result.index != expected {                     // 2. compare
        return Err(AggregateError::NonContiguousLayer {
            expected,
            got: result.index,                        //    (Copy: u32, no move)
        });
    }
    self.layers.push(result);                         // 3. move
    self.failures.append(&mut layer_failures);        // 4. consume
    Ok(())                                            // 5. return
}
```

A literal-minded implementer could reorder for "performance" —
move-then-compare would either wreck the Err message (using a moved
field) or require introducing intermediate bindings. The plan must
spell out the order so the implementer doesn't reorder.

### Gate 4: Document the move semantics on `Err`

The owned-value parameters (the `Vec<...>` of failures, etc.) are
moved into the call. On `Err` they are dropped, not returned —
callers that want to retry must reconstruct them. This matches the
prior panic behavior (process crashed before any append) but the
typed-Err API surface invites retry where the panic API didn't.

The rustdoc on the method must call this out explicitly:

```rust
/// # Move semantics on `Err`
///
/// `layer_failures` is moved into the call. On `Err` it is dropped,
/// not returned — callers that want to retry must reconstruct the
/// failures vector. Currently only `SimulationRunner::run_inner`
/// (private) calls this; it propagates via `?` and never retries,
/// so the drop is the same outcome as the historical panic path.
```

Note: do NOT use `[`crate::module::Type::method`]` intra-doc-link
syntax for private items — `cargo doc` emits a
`rustdoc::private_intra_doc_links` warning. Use plain code-formatted
text (backticks only) instead.

### Gate 5: Verify the call-site count by grep, not by memory

The plan must enumerate every call site that needs `.expect("...")`
migration AFTER the panic test deletion. Off-by-one errors are
common. Grep for `\.<method>(` in every test file and list the
exact line numbers in the plan. If the plan also deletes a
`#[should_panic]` test, explicitly say "lines X and Y are deleted by
step N — DO NOT migrate them in this step to avoid double-edit
conflicts" so the implementer doesn't double-edit.

## Test-site `.expect()` messages

Per ADR-0003, every `.expect()` carries a domain-grounded
justification. For migrated test sites, two canonical wordings cover
most cases:

- For-loop builders: `"test fixture: sequential index i in 0..N
  satisfies <method>'s contiguity precondition"`
- Hand-written sequences: `"test fixture: explicit index N matches
  layer count N at this call site"`

Generic phrases (`"should not fail"`, `"will not fail"`, `"this is
safe"`, `"unreachable"`) are rejected at code review.

## Don't converge with the deserialize-bypass guard yet

If the aggregate also has a `validate(&self) -> Result<(), String>`
deserialize-bypass guard (per `aggregate-validate-as-deserialize-bypass-guard.md`),
the two paths intentionally diverge in error type:

- Live mutation: typed `Result<(), AggregateError>` — in-process misuse
- Deserialize-bypass: `Result<(), String>` — "this serialised blob is corrupt"

Different domains, different optimal shapes. Convergence is a separate
later issue if the two callers ever need to share matchers.

## Test coverage

Replace the existing `#[should_panic]` test with an Err-asserting
equivalent. Net test count unchanged:

```rust
#[test]
fn non_sequential_layer_returns_err() {
    let mut sim = ...;
    sim.add_layer(make_layer(0, ...), vec![])
        .expect("test fixture: index 0 satisfies <method>'s contiguity precondition on an empty sim");
    let err = sim
        .add_layer(make_layer(5, ...), vec![])
        .expect_err("non-contiguous index 5 (expected 1) must return Err");
    assert_eq!(err, AggregateError::NonContiguousLayer { expected: 1, got: 5 });
}
```

One assertion is sufficient when the invariant is pure integer
equality — both `got > expected` and `got < expected` traverse the
same code path.

## See also

- `aggregate-validate-as-deserialize-bypass-guard.md` — the
  deserialize-side analogue. Together the two patterns cover both
  paths into an aggregate's invariants.
- `anti/non-exhaustive-on-single-variant-enum.md` — the sibling
  anti-pattern that the v1 review caught and v2 dropped.
- ADR-0003 — `.expect()` justification policy that test-site
  migrations must satisfy.
- `entity-validate-on-mutation.md` — the entity-level analogue
  pattern.
