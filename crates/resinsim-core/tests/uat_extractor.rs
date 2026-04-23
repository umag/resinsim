//! Host binary for the `uat_steps::extract` unit + property tests.
//!
//! The cucumber harness at `tests/uat_gherkin.rs` uses `harness = false`,
//! which disables libtest `#[test]` discovery there. This sibling binary
//! exists solely so the default harness can run the `extract_tests` module.

#[path = "uat_steps/mod.rs"]
mod uat_steps;
