---
issue: t2f3.1-post-impl-calibration-followups
date: 2026-05-20
---

# Anti-pattern: Predicate-widening doc drift

## What goes wrong

A predicate (typically returning bool) is widened — from N-of-N to
(N+1)-of-(N+1), or its arity / semantic claim changes in any way. The
single function-body edit is small, but the predicate's behaviour is
QUOTED at every downstream site that documents it:

- The predicate's own doc comment
- Every caller's doc comment that describes WHEN the predicate fires
- Every emitted user-facing message that the caller produces in
  response
- Knowledge-base entries that describe the contract
- ADRs that document the design decision
- TEST NAMES that paraphrase the predicate's contract

If any of those sites is missed, the documentation now SILENTLY LIES
about the predicate's contract. Future readers infer the old contract
from the stale wording and write code that reintroduces the gap.

## Concrete example (t2f3.1)

`ResinProfile::has_calibrated_moduli()` was widened from 2-of-2 (E + ν)
to 3-of-3 (E + ν + z_ratio). Sites that needed updating in lockstep:

| Site | Stale-wording risk |
|------|--------------------|
| `resin_profile.rs` predicate doc comment | "both fields populated" |
| `failure_predictor.rs` caveat-origin doc comment | "either field is None" |
| `failure_predictor.rs` literal caveat string | "see KB-163" (omits KB-164) |
| `failure_predictor.rs` caveat-test assertion | only checks KB-163 substring |
| `docs/kb/KB-163-…` §"Uncalibrated-moduli caveat" body | "either field is None" |
| `docs/kb/KB-163-…` §"Calibration path" workflow | "E and ν" (2-step) |
| `docs/adr/0018-…` §9 disclosure paragraph | "when has_calibrated_moduli == false" |
| Existing test name `has_calibrated_moduli_requires_both` | "_requires_both" |

The Phase 5 review caught the stale TEST NAME (`_requires_both`)
after all other sites had been updated — a final layered defence
that almost slipped through.

## How to apply

When widening any predicate:

1. **Before editing the predicate body**, grep the repo for every
   mention of the predicate name AND every paraphrase of its contract.
   For `has_calibrated_moduli` the search set was `has_calibrated_moduli`,
   `calibrated moduli`, `uncalibrated moduli`, `either field`, `both
   fields`, `E and ν`.
2. **List the sites in writing BEFORE doing the work** — a written
   list in the plan's step description (or a one-shot grep result
   captured to a scratch file) prevents memory-lapse misses during
   implementation.
3. **Update all sites in the same commit** so the drift cannot slip
   across PR boundaries (and reviewers see the lockstep change).
4. **Audit test NAMES**, not just test bodies. A test named
   `_requires_both` whose body still passes under a 3-of-3 predicate
   is the most insidious kind of stale doc — future readers infer the
   old contract from the name without reading the body.

## Detection in review

A code review should grep for the predicate name across the diff:
every mention in unchanged hunks should either be a load-bearing cite
(the predicate's own definition site) or be flagged HIGH for
"predicate widened, doc not updated". The pattern is asymmetric — the
diff makes the change easy to find IN the predicate file but hard to
miss in the FAR downstream files.

## Related

- `docs/patterns/anti/adr-pattern-doc-drift-from-iterated-values.md` —
  drift class for NUMERICAL constants (different shape — values, not
  predicate shape).
- `docs/patterns/honest-zero-with-model-gap-caveat.md` — the caveat-
  disclosure pattern this anti-pattern protects.
