#![allow(dead_code)]
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

// ---- Step 3 whole-suite invariant: extract all migrated UAT files ---------

#[test]
fn spec_uat_dir_extracts_expected_scenarios_across_all_files() {
    // Plan step 3 invariant: every spec/uat/*.md file is extractable, and
    // the union of extracted titles is unique across the suite. Exact
    // counts are pinned per file so a future edit that drops or doubles a
    // fence fails this assertion at nextest time.
    let spec_dir = spec_uat_dir();

    // (filename, expected_fence_count) — counts == `\`\`\`gherkin fences,
    // which after step 3 equals one fence per `**Scenario...**` block in
    // the pre-migration source.
    let expectations: &[(&str, usize)] = &[
        ("cli-profile-by-name-loading.md", 7),
        ("cli-requires-resin-for-recipe-fields.md", 3),
        ("cli-temperature-flag-validation.md", 6),
        ("cure-depth-nan-guard.md", 2),
        ("legacy-resin-toml-defaults.md", 2),
        ("legacy-resin-toml-without-recipe.md", 2),
        ("legacy-resin-toml-without-ref-lift-speed.md", 2),
        ("recipe-inside-printer-range.md", 2),
        ("recipe-outside-printer-range.md", 2),
        ("resin-switch-changes-simulation.md", 2),
        ("safety-factor-zero-force.md", 1),
        ("suction-detector-raft-false-positive.md", 6),
        ("thermal-degradation.md", 1),
    ];

    let mut all_titles: Vec<(String, String)> = Vec::new();
    let mut total = 0usize;
    for (file, expected) in expectations {
        let md = std::fs::read_to_string(spec_dir.join(file))
            .unwrap_or_else(|e| panic!("failed to read {file}: {e}"));
        let out = extract(&md);
        assert_eq!(
            out.len(),
            *expected,
            "{file} expected {expected} fences, got {} — titles {:#?}",
            out.len(),
            out.iter().map(|s| &s.title).collect::<Vec<_>>(),
        );
        total += out.len();
        for scenario in out {
            all_titles.push((file.to_string(), scenario.title));
        }
    }

    // Whole-suite count invariant. Sum of the per-file expectations above
    // is 38 (plan's "35" count treats compound UAT-Ns as single scenarios;
    // the honest-per-`**Scenario**`-block count is 38). Pin the actual
    // realised number so drift surfaces.
    assert_eq!(total, 38, "whole-suite scenario count drifted");

    // Unique-title invariant. Titles come from the most recent heading
    // (H2 or H3) before each fence; compound UAT-Ns have H3 sub-headings
    // introduced in step 3 so titles don't collide across sibling fences
    // under a shared H2.
    let mut seen: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (file, title) in &all_titles {
        if let Some(first_file) = seen.get(title) {
            panic!(
                "duplicate extracted title {title:?}: first seen in {first_file}, now in {file}"
            );
        }
        seen.insert(title.clone(), file.clone());
    }
}

fn spec_uat_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("CARGO_MANIFEST_DIR has workspace + repo ancestors")
        .join("spec/uat")
}

// ---- Step 2 smoke test: real spec/uat file round-trips --------------------

#[test]
fn recipe_outside_printer_range_md_extracts_two_scenarios_with_compound_inputs() {
    // This test reads the real migrated file under spec/uat/. It pins the
    // plan's step 2 acceptance criterion: the DocString (```gherkin ... ```
    // fences containing "\"\"\"" blocks) and the DataTable ("| col |" rows)
    // round-trip through markdown into the extracted gherkin verbatim.
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let md_path = manifest
        .ancestors()
        .nth(2)
        .expect("CARGO_MANIFEST_DIR has workspace + repo ancestors")
        .join("spec/uat/recipe-outside-printer-range.md");

    let source = std::fs::read_to_string(&md_path).unwrap_or_else(|e| {
        panic!("failed to read {}: {e}", md_path.display());
    });

    let out = extract(&source);
    assert_eq!(
        out.len(),
        2,
        "expected 2 extracted scenarios from {md_path:?}, got {out:#?}",
    );

    let titles: Vec<&str> = out.iter().map(|s| s.title.as_str()).collect();
    assert!(
        titles[0].contains("UAT-1") || titles[0].contains("Pairing fails"),
        "first scenario title should reference UAT-1 / pairing: {titles:?}",
    );
    assert!(
        titles[1].contains("UAT-2") || titles[1].contains("ALL violations"),
        "second scenario title should reference UAT-2 / all violations: {titles:?}",
    );

    // UAT-2 carries the DataTable for printer ranges + resin recipe.
    let uat2 = &out[1].gherkin;
    assert!(
        uat2.contains("| layer_height_range_um "),
        "UAT-2 must retain printer DataTable after extraction: {uat2}",
    );
    assert!(
        uat2.contains("| normal_exposure_sec "),
        "UAT-2 must retain recipe DataTable after extraction: {uat2}",
    );

    // UAT-2 also carries a DocString for the expected error message.
    assert!(
        uat2.contains("\"\"\""),
        "UAT-2 must retain DocString triple-quote delimiters after extraction: {uat2}",
    );
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
