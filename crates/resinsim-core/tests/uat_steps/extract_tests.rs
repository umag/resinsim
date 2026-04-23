//! Unit + property tests for [`super::extract::extract`].
//!
//! Coverage target (plan step 1):
//! 1. single fence with preceding heading
//! 2. multiple fences per file, each anchored to its own heading
//! 3. YAML frontmatter stripped, not leaked into output
//! 4. non-`gherkin` fenced code blocks ignored
//! 5. fence with no preceding heading yields an empty title
//!
//! Plus proptest properties: the extractor is total (never panics) on
//! arbitrary UTF-8, on arbitrary byte sequences (via lossy decode), and on
//! structured whitespace / line-ending perturbation.

use super::extract::{ExtractedScenario, extract};
use proptest::prelude::*;

// ---- 1. single fence with preceding heading --------------------------------

#[test]
fn single_fence_with_preceding_h2_heading() {
    let md = "\
# UAT: Thing

## Scenario: does the thing

```gherkin
Given a thing
When it does
Then it worked
```
";
    let out = extract(md);
    assert_eq!(out.len(), 1, "expected exactly one scenario, got {out:#?}");
    assert_eq!(out[0].title, "does the thing");
    assert!(
        out[0].gherkin.contains("Given a thing")
            && out[0].gherkin.contains("When it does")
            && out[0].gherkin.contains("Then it worked"),
        "fence body missing step text: {:?}",
        out[0].gherkin,
    );
}

// ---- 2. multiple fences per file -------------------------------------------

#[test]
fn multiple_fences_each_anchored_to_own_heading() {
    let md = "\
# UAT: Two things

## Scenario: first

```gherkin
Given alpha
Then first
```

Some intervening prose that must not leak in.

## Scenario: second

```gherkin
Given beta
Then second
```
";
    let out = extract(md);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].title, "first");
    assert_eq!(out[1].title, "second");
    assert!(out[0].gherkin.contains("Given alpha"));
    assert!(out[1].gherkin.contains("Given beta"));
    assert!(
        !out[0].gherkin.contains("intervening prose"),
        "prose between fences must not leak into fence bodies",
    );
}

// ---- 3. frontmatter stripped -----------------------------------------------

#[test]
fn yaml_frontmatter_is_stripped_not_leaked() {
    let md = "\
---
issue: t1f4
date: 2026-04-17
---

## Scenario: after frontmatter

```gherkin
Given x
```
";
    let out = extract(md);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].title, "after frontmatter");
    assert!(
        !out[0].gherkin.contains("issue:") && !out[0].gherkin.contains("t1f4"),
        "frontmatter leaked into scenario body: {:?}",
        out[0].gherkin,
    );
}

#[test]
fn bom_then_frontmatter_strips_both() {
    let md = "\u{feff}---\nissue: x\n---\n\n## Scenario: t\n\n```gherkin\nGiven y\n```\n";
    let out = extract(md);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].title, "t");
}

// ---- 4. non-`gherkin` fences ignored ---------------------------------------

#[test]
fn non_gherkin_fenced_blocks_are_ignored() {
    let md = "\
## Scenario: mixed

```rust
fn not_a_scenario() {}
```

```
no language tag
```

```gherkin
Given yes
```
";
    let out = extract(md);
    assert_eq!(out.len(), 1);
    assert!(out[0].gherkin.contains("Given yes"));
    assert!(
        !out[0].gherkin.contains("fn not_a_scenario") && !out[0].gherkin.contains("no language tag"),
        "non-gherkin fence content leaked",
    );
}

// ---- 5. fence without preceding heading ------------------------------------

#[test]
fn fence_without_preceding_heading_has_empty_title() {
    let md = "Just a fence, no headings.\n\n```gherkin\nGiven z\n```\n";
    let out = extract(md);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].title, "");
    assert!(out[0].gherkin.contains("Given z"));
}

// ---- Additional hygiene cases ----------------------------------------------

#[test]
fn empty_input_yields_empty_output() {
    assert!(extract("").is_empty());
}

#[test]
fn no_fences_yields_empty_output() {
    assert!(extract("# Heading only\n\nSome prose.\n").is_empty());
}

#[test]
fn crlf_line_endings_survive_round_trip() {
    let md = "## Scenario: crlf\r\n\r\n```gherkin\r\nGiven a\r\nThen b\r\n```\r\n";
    let out = extract(md);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].title, "crlf");
    assert!(out[0].gherkin.contains("Given a"));
    assert!(out[0].gherkin.contains("Then b"));
}

#[test]
fn scenario_outline_prefix_stripped_from_title() {
    let md = "## Scenario Outline: parameterised\n\n```gherkin\nGiven x\n```\n";
    let out = extract(md);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].title, "parameterised");
}

// ---- Property tests --------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Total over arbitrary UTF-8 input: never panics, always returns a Vec.
    #[test]
    fn extract_is_total_on_arbitrary_utf8(s in ".*") {
        let out: Vec<ExtractedScenario> = extract(&s);
        prop_assert!(out.len() < usize::MAX);
    }

    /// Total over arbitrary byte input decoded via `String::from_utf8_lossy`.
    #[test]
    fn extract_is_total_on_arbitrary_bytes(bytes in prop::collection::vec(any::<u8>(), 0..1024)) {
        let cow = String::from_utf8_lossy(&bytes);
        let out = extract(&cow);
        prop_assert!(out.len() < usize::MAX);
    }

    /// Structured whitespace + line-ending perturbation around a canonical
    /// single-fence scenario: extraction still yields exactly one scenario.
    #[test]
    fn whitespace_and_line_endings_dont_eat_the_fence(
        leading_spaces in 0usize..8,
        trailing_blank_lines in 0usize..4,
        nl_idx in 0usize..2,
    ) {
        let nl = if nl_idx == 0 { "\n" } else { "\r\n" };
        let pad = " ".repeat(leading_spaces);
        let tail = nl.repeat(trailing_blank_lines);
        let md = format!(
            "{pad}## Scenario: stress{nl}{nl}```gherkin{nl}Given x{nl}```{nl}{tail}"
        );
        let out = extract(&md);
        prop_assert_eq!(out.len(), 1);
        prop_assert!(out[0].gherkin.contains("Given x"));
    }

    /// Nested-fence attempts inside non-gherkin blocks must not produce spurious scenarios.
    #[test]
    fn nested_fence_attempt_in_rust_block_not_extracted(body in "[a-zA-Z ]{0,40}") {
        let md = format!(
            "```rust\n// Here is a pseudo-fence attempt:\n// ```gherkin\n// Given {body}\n// ```\n```\n"
        );
        let out = extract(&md);
        prop_assert_eq!(out.len(), 0, "rust-block content must not parse as gherkin");
    }
}
