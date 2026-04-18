---
issue: unwrap-policy
date: 2026-04-18
---

# ADR-0003: Deny `clippy::unwrap_used` workspace-wide; `.expect("<justification>")` is the sanctioned mitigation

## Status
Accepted

## Context
resinsim-core has a growing number of `.unwrap()` call sites (310 at the time of
this ADR, across 29 files and every DDD layer: values, entities, services, io,
repositories, app, tests, plus the resinsim-inspect binary). An informal team
discipline treats `.unwrap()` as an anti-pattern and prefers `.expect("<why this
Ok/Some is guaranteed>")` — but the policy is nowhere codified and cannot be
audited by the compiler.

A small number of sites already use the intended shape, e.g.
`failure_predictor.rs:62,100,102,104`:

```rust
.expect("ResinProfile::validate() guarantees max_vat_temp > 0")
```

where the message names the specific upstream validator that makes the `Ok` /
`Some` outcome inevitable. That shape is the one we want everywhere.

## Decision

1. **Policy.** Deny `clippy::unwrap_used` at the workspace root via
   `[workspace.lints.clippy]` and opt-in from every member crate with
   `[lints] workspace = true`. No `#[cfg_attr(test, allow(...))]` escape
   hatch — the policy applies equally to `src/` and `tests/`.

2. **Sanctioned mitigation — `.expect("<justification>")`.** Every surviving
   `.expect()` message must state *why* the `Ok` / `Some` is guaranteed at this
   site, citing an upstream validator, a domain invariant, or a KB reference.
   Generic phrases (`"will not fail"`, `"should not fail"`, `"cannot fail"`,
   `"unreachable"`, `"parse failed"`, `"todo"`, `"fixme"`, functional
   equivalents like `"this is safe"` / `"impossible"`) are rejected in code
   review.

3. **Allowed — domain-safe defaults.** `.unwrap_or(...)`, `.unwrap_or_else(...)`,
   `.unwrap_or_default()` remain allowed. These are not invariant claims —
   they are deliberate domain-safe defaults, and `clippy::unwrap_used` does
   not trigger on them.

4. **Scope — `clippy::unwrap_used` only.** This lint lives in the
   `clippy::restriction` group and is opt-in per-lint. We deliberately do
   **not** blanket-enable `clippy::restriction` — many of its lints
   (e.g. `clippy::integer_arithmetic`, `clippy::float_arithmetic`,
   `clippy::missing_docs_in_private_items`) conflict with idiomatic Rust and
   are inappropriate for a simulation codebase. Only `unwrap_used` is denied
   under this ADR. Sibling lints
   (`panic`, `unreachable`, `todo`, `unimplemented`, `expect_used`) are
   deferred to a follow-up discussion; see "Alternatives considered" below.

5. **`clippy::expect_used` NOT enabled.** `.expect()` IS the required
   mitigation for `.unwrap()` under this policy — the justification message
   is where authors state the invariant. Banning `.expect()` as well would
   leave the codebase with no escape hatch short of `?` propagation in every
   site, which is inappropriate for tests and proptest fixtures where a
   panic IS the expected failure mode. A follow-up policy could enable
   `expect_used` under a discipline like "message must exceed N characters
   or cite a KB/invariant reference", but that is out of scope for this ADR.

6. **IO-layer outcome rule.** For any `.unwrap()` that operates on data
   originating from **untrusted input** (file bytes, user-provided paths,
   external process output), the migration must EITHER:

   - (a) write an `.expect()` whose justification message can be traced to
     an upstream validation already performed in the same function — the
     message names the specific validator or invariant, OR
   - (b) refactor to propagate the error via `?` with the parent function
     returning `Result`.

   Generic messages like `"parse failed"` are rejected; the site is hiding
   a real error-handling gap, not stating an invariant. Code review grep
   for anti-phrases is the hard-stop gate.

7. **Binary-crate messages are user-visible.** `.expect()` messages in
   `resinsim-inspect/src/main.rs` panic with the message printed to stderr
   on the user's terminal. Phrase these as end-user diagnostics
   (what went wrong + what the user can do), not internal invariant
   statements.

## Doctest handling

`cargo clippy --all-targets` lints doctests on recent toolchains. Prior to
enabling the deny, we audit `///`-style doc comments with:

```sh
grep -rn '^///' resinsim/crates/ | grep '\.unwrap()'
```

For each hit, choose per-case:

- Rewrite the doctest into a `fn main() -> Result<_, _> { ... Ok(()) }`
  wrapper using `?` (preferred for examples that demonstrate fallible APIs);
- Add `#[allow(clippy::unwrap_used)]` on the *documented item* (not the
  doctest block) with a comment explaining why the example can't use `?`.

Block doc comments (`/** */`) and crate-level comments (`//!`) are rare in
resinsim-core; the `^///` grep is the common-case check. An implementer
who suspects block-doc usage can widen the pattern.

## Consequences

- `cargo clippy --workspace --all-targets -- -D warnings` becomes the policy
  enforcement signal. Without CI, this is expected pre-commit discipline —
  wiring CI is out of scope for this issue.
- `cargo build` and `cargo test` / `cargo nextest` are **unaffected**.
  `[lints.clippy]` fires only under `cargo clippy`. This is deliberate: a
  lint is clippy's job, not rustc's.
- 310 call sites migrate to `.expect("<justification>")` or `?` propagation.
  Tests migrate too; no `cfg_attr(test, allow)` is written.
- Future `.unwrap()` introductions surface as `cargo clippy` errors at
  the first attempt to lint the change.

## Alternatives considered

- **Warn, not deny.** Rejected — without CI, warnings are invisible noise
  on developer machines and drift back in.
- **Deny in `src/` only, allow in tests.** Rejected — test fixtures are
  where most unwraps live, and proptest failures become easier to diagnose
  when fixture-construction panics name the value that broke the strategy
  rather than a generic "called `.unwrap()` on `Err`".
- **Blanket-enable `clippy::restriction`.** Rejected — many lints in that
  group conflict with idiomatic Rust (integer_arithmetic, float_arithmetic,
  missing_docs_in_private_items). Opt-in per-lint is the correct grain.
- **Also deny `clippy::expect_used`.** Rejected — leaves no escape hatch.
  `.expect("<why>")` carries the invariant justification that `.unwrap()`
  hides; banning both would force `?` propagation into tests and proptest
  fixtures where panic IS the expected behavior on broken fixtures.
- **Deny `panic!`, `unreachable!`, `todo!`, `unimplemented!` siblings.**
  Deferred — those are separate decisions with different trade-offs
  (proptest strategies legitimately use `unreachable!`; in-progress code
  legitimately uses `todo!` during feature branches). Revisit after the
  unwrap migration lands.
