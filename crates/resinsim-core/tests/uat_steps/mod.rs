//! Shared test-support modules for the UAT BDD suite.
//!
//! `extract` parses `spec/uat/*.md` files where each scenario lives inside a
//! ```gherkin fenced code block. See docs/adr/0008-bdd-uat-spike-notes.md
//! and docs/patterns/extracting-gherkin-from-markdown.md for context.
//!
//! This tree is pulled in by two sibling test binaries:
//! - `tests/uat_extractor.rs` — default libtest harness; hosts the
//!   unit + property tests below via `extract_tests`.
//! - `tests/uat_gherkin.rs` — `harness = false` cucumber driver; wires the
//!   extractor into the BDD runner once the rollout reaches step 4.

pub mod extract;

pub mod extract_tests;

pub mod world;

pub mod recipe_out_of_range;
