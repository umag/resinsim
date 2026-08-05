//! Authoring-time guard: every `spec/uat/*.md` must synthesise into a
//! WELL-FORMED Gherkin feature.
//!
//! # Why this is a separate test target
//!
//! It would be natural to put this beside the other extractor tests in
//! `uat_steps/extract_tests.rs`. That would not work. `extract_tests` is a
//! module (`uat_steps/mod.rs`), compiled into the `uat_gherkin` and
//! `uat_extractor` binaries — and `.config/nextest.toml` excludes every
//! `uat_*` binary from the default profile, because cucumber-rs's
//! `harness = false` binary can't speak libtest's listing protocol
//! (see `docs/patterns/cucumber-in-nextest-workspace.md`).
//!
//! A guard against invisible failures that is itself invisible is worthless.
//! This target therefore has a name that does NOT match `^uat_`, so it runs
//! in the mandated ADR-0017 four-config matrix like any other test.
//! `nextest_filter_sanity.rs` pins that exclusion pattern, so the naming
//! constraint is enforced rather than merely remembered.
//!
//! # What it catches
//!
//! `docs/patterns/anti/markdown-bullets-in-gherkin-step.md` describes the
//! trap: prose formatting inside step text — bullet lists, and most often a
//! WRAPPED CONTINUATION LINE — produces a line carrying no Gherkin keyword,
//! and the parser rejects the entire file. Cucumber reports those as
//! "N parsing errors" in a summary counter that no guard inspects, so the
//! affected scenarios vanish silently rather than failing.
//!
//! That doc was written 2026-04-23 and had been violated 20 times by
//! 2026-07-27, losing 54 authored scenarios. Documentation alone does not
//! hold this invariant; this test does.

use std::path::{Path, PathBuf};

#[path = "uat_steps/extract.rs"]
mod extract;

/// Specs that deliberately contribute NO executable scenarios, with the
/// reason. Anything else yielding zero scenarios is a silent loss and fails
/// `every_spec_uat_md_yields_at_least_one_scenario`.
///
/// This list exists because "deliberately not executable yet" and "the
/// ```gherkin fence was mistyped so everything vanished" are indistinguishable
/// from the outside — and silent vanishing is the entire subject of the issue
/// that added this file. Membership here is a written, reviewable claim.
const SPECS_WITH_NO_EXECUTABLE_SCENARIOS: &[(&str, &str)] = &[];

/// Mirrors `uat_gherkin.rs::resolve_spec_uat_dir`. Duplicated deliberately:
/// importing it would mean pulling in the whole `uat_steps` tree (cucumber,
/// every step-def module, the world), which would make this lightweight
/// guard depend on the machinery it exists to guard.
fn resolve_spec_uat_dir() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest
        .ancestors()
        .nth(2)
        .expect("CARGO_MANIFEST_DIR has crate + workspace + repo ancestors");
    repo_root
        .join("spec/uat")
        .canonicalize()
        .unwrap_or_else(|e| {
            panic!(
                "failed to canonicalise spec/uat under {}: {e}",
                repo_root.display()
            )
        })
}

#[test]
fn every_spec_uat_md_synthesises_well_formed_gherkin() {
    let spec_uat = resolve_spec_uat_dir();
    let mut offenders: Vec<String> = Vec::new();

    let mut md_files: Vec<PathBuf> = std::fs::read_dir(&spec_uat)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", spec_uat.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
        .collect();
    md_files.sort();

    assert!(
        !md_files.is_empty(),
        "no .md files under {} — the resolver is pointing at the wrong directory",
        spec_uat.display()
    );

    for md_path in &md_files {
        let md = std::fs::read_to_string(md_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", md_path.display()));
        let scenarios = extract::extract(&md);
        if scenarios.is_empty() {
            continue;
        }
        let title = md_path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("spec/uat .md files have UTF-8 stems")
            .replace('-', " ");
        let feature_text = extract::synthesize_feature(&title, &scenarios);

        // Delegate to the SAME parser cucumber uses, rather than a
        // hand-rolled keyword heuristic. A heuristic has to re-implement the
        // grammar — docstrings, Examples tables, Scenario Outline, Rule — and
        // an earlier draft of this test did exactly that and produced three
        // false positives on files that parse cleanly. Agreeing with cucumber
        // by construction is the only way this guard stays honest.
        // Reached via cucumber's re-export rather than a direct `gherkin`
        // dev-dependency: that pins us to the exact parser version cucumber
        // runs, so the two can never disagree after a dependency bump.
        if let Err(e) = cucumber::gherkin::Feature::parse(
            &feature_text,
            cucumber::gherkin::GherkinEnv::default(),
        ) {
            let name = md_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("<unknown>");
            offenders.push(format!("  {name}\n      {e}"));
        }
    }

    assert!(
        offenders.is_empty(),
        "{} spec/uat file(s) synthesise into Gherkin the parser rejects. \
         Cucumber counts these as \"parsing errors\" and silently drops EVERY \
         scenario in the affected file — they do not fail, they vanish.\n\n\
         The usual cause is prose formatting inside step text: a wrapped \
         continuation line, or a markdown bullet list. Put the step on ONE \
         line, or express compound input as a DataTable or separate And \
         steps. See docs/patterns/anti/markdown-bullets-in-gherkin-step.md.\n\n{}",
        offenders.len(),
        offenders.join("\n"),
    );
}

/// Closes the blind spot the well-formedness test alone would leave: a file
/// yielding NO scenarios trivially satisfies "parses cleanly" and would sail
/// through. That is how spec/uat produced 51 features from 52 .md files with
/// nobody noticing.
#[test]
fn every_spec_uat_md_yields_at_least_one_scenario() {
    let spec_uat = resolve_spec_uat_dir();
    let mut silent: Vec<String> = Vec::new();

    let mut md_files: Vec<PathBuf> = std::fs::read_dir(&spec_uat)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", spec_uat.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
        .collect();
    md_files.sort();

    for md_path in &md_files {
        let name = md_path
            .file_name()
            .and_then(|s| s.to_str())
            .expect("spec/uat .md files have UTF-8 names");
        let md = std::fs::read_to_string(md_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", md_path.display()));
        let count = extract::extract(&md).len();
        let exempt = SPECS_WITH_NO_EXECUTABLE_SCENARIOS
            .iter()
            .any(|(f, _)| *f == name);

        if count == 0 && !exempt {
            silent.push(format!("  {name} — extractor found no ```gherkin blocks"));
        }
        // Keep the exemption list honest in the other direction too: an entry
        // that starts yielding scenarios must be removed, or the list rots
        // into a stale excuse for coverage that now exists.
        if count > 0 && exempt {
            silent.push(format!(
                "  {name} — listed in SPECS_WITH_NO_EXECUTABLE_SCENARIOS but now \
                 yields {count} scenario(s); remove the exemption"
            ));
        }
    }

    assert!(
        silent.is_empty(),
        "{} spec/uat file(s) contribute no scenarios without declaring that they \
         mean to.\n\nA spec whose fence is untagged or mistyped vanishes exactly \
         like one that is deliberately not-yet-executable — this test forces the \
         difference to be written down. Either fix the ```gherkin fence, or add \
         the file to SPECS_WITH_NO_EXECUTABLE_SCENARIOS with a reason.\n\n{}",
        silent.len(),
        silent.join("\n"),
    );
}

/// Encodes the fault injection that was otherwise only ever done by hand.
///
/// The well-formedness test can only be trusted if it actually rejects the
/// shape it claims to catch. Without this, a cucumber/gherkin bump could
/// change parser behaviour and the guard would quietly start passing
/// everything — silent drift, which is the failure class this whole file
/// exists to prevent.
#[test]
fn the_guard_rejects_a_wrapped_continuation_line() {
    // The canonical offender: line 2 of the step carries no Gherkin keyword.
    // Lifted from the real shape found in peel-shape-factor-scales-with-aspect-ratio.md.
    let bad = "Feature: injected\n\
               \n\
               Scenario: wrapped continuation\n\
               \x20 Given a run whose masks are fully-solid placeholders (1x1,\n\
               \x20   or the W×H all-solid fallback)\n\
               \x20 Then it parses\n";

    let parsed = cucumber::gherkin::Feature::parse(bad, cucumber::gherkin::GherkinEnv::default());
    assert!(
        parsed.is_err(),
        "the parser accepted a wrapped continuation line, so \
         every_spec_uat_md_synthesises_well_formed_gherkin can no longer catch \
         the defect it was written for — re-check the cucumber/gherkin version"
    );

    // And the well-formed counterpart must still parse, so the guard is not
    // simply rejecting everything.
    let good = "Feature: injected\n\
                \n\
                Scenario: single line\n\
                \x20 Given a run whose masks are fully-solid placeholders (1x1, or the W×H all-solid fallback)\n\
                \x20 Then it parses\n";
    assert!(
        cucumber::gherkin::Feature::parse(good, cucumber::gherkin::GherkinEnv::default()).is_ok(),
        "the parser rejected well-formed Gherkin — the guard would now fail every spec"
    );
}
