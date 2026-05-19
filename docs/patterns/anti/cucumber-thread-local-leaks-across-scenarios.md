---
issue: ctb-layer-height-authority
date: 2026-05-19
---

# Anti-pattern: thread_local for per-scenario cucumber state

## Context

cucumber-rs runs scenarios concurrently across worker threads (default
behaviour, not configurable in the obvious places). Scenarios on the
same worker share that thread's `thread_local!` storage. Step defs
that stash per-scenario fixture data in a thread_local will see the
NEXT scenario's data when their Then steps fire, because cucumber
moves on to subsequent scenarios while older Then steps are still
resolving.

## The trap

```rust
thread_local! {
    static CASE: RefCell<MyCase> = RefCell::new(MyCase::default());
}

#[given(regex = r"^a CTB sliced at (\d+) µm$")]
fn given_ctb(_world: &mut World, h: u32) {
    CASE.with(|c| c.borrow_mut().heights = vec![h as f32; 5]);
}

#[then(regex = r"^the result uses (\d+) µm$")]
fn then_result_uses(world: &mut World, h: u32) {
    let case = CASE.with(|c| c.borrow().clone());
    // BUG: `case` may be the NEXT scenario's data, not this one's.
    assert_eq!(case.heights[0], h as f32);
}
```

Symptom: scenario A asserts on data that belongs to scenario B. The
World struct's per-scenario reset (cucumber resets `World` between
scenarios) does NOT touch thread_locals, so they leak across the
scenario boundary.

## The fix

Put per-scenario state on the `UatWorld` struct itself. cucumber
constructs a fresh `UatWorld` for every scenario, so state is
scenario-scoped by definition.

```rust
#[derive(Default, World)]
pub struct UatWorld {
    pub case_heights: Option<Vec<f32>>,
    // ... other per-scenario fields ...
}

#[given(regex = r"^a CTB sliced at (\d+) µm$")]
fn given_ctb(world: &mut UatWorld, h: u32) {
    world.case_heights = Some(vec![h as f32; 5]);
}

#[then(regex = r"^the result uses (\d+) µm$")]
fn then_result_uses(world: &mut UatWorld, h: u32) {
    let case = world.case_heights.as_ref().expect("Given set heights");
    assert_eq!(case[0], h as f32);
}
```

## Why this happens

cucumber-rs's default executor schedules scenarios across a worker
pool. Step defs run on whichever worker the scheduler picks; nothing
guarantees that all steps of one scenario run on the same thread,
let alone in temporal isolation from other scenarios.

`thread_local!` is per-worker-thread, not per-scenario. The lifetime
is the worker's lifetime — much longer than a single scenario.

## How to spot it

Symptoms that point at this anti-pattern (in order of how loudly they
say "concurrent scenario sharing"):

1. Assertion says "expected X got Y" where Y is a value used by a
   DIFFERENT scenario in the same feature file.
2. The bug is non-deterministic across `cargo test` runs (depends on
   the worker schedule).
3. Reordering scenarios in the .feature file changes which scenario
   fails.

## See also

- `docs/patterns/anti/cucumber-step-regex-ambiguity.md` — a different
  cucumber-rs trap caught during the same lifecycle.
