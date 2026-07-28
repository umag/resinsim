// Cucumber-rs harness driving the UAT suite (post step-4/6 refactor).
//
// Reads scenarios from the workspace-root `spec/uat/` directory via the
// markdown extractor. Every step definition lives in a per-UAT-file
// module under `tests/uat_steps/`; this file is now purely the harness
// entry point + wiring.
//
// Harness flow:
// 1. Resolve `spec/uat/` from `CARGO_MANIFEST_DIR` (crate → workspace →
//    repo root) and canonicalise.
// 2. Validate the directory: every `*.md` must carry `issue:` YAML
//    frontmatter. Zero matches panics with both resolved path AND the
//    expected pattern.
// 3. Extract each .md, synthesise a `Feature:` block per file, write
//    the synthesised tree under `$CARGO_TARGET_TMPDIR/spec-uat-features`.
// 4. Run cucumber once against that tree. Silent-green guard
//    (passed+skipped+failed > 0) and `execution_has_failed` exit
//    preserved from the spike.

use cucumber::{StatsWriter as _, World};

mod uat_steps;

use uat_steps::world::UatWorld;

// Force each step-def module to be linked so their `#[given]/#[when]/
// `#[then]` registrations reach cucumber-rs's global inventory. The
// module declarations alone are enough for rustc to compile them; the
// explicit `use` lines below keep them from being dead-code-stripped
// in optimised builds.
#[allow(unused_imports, clippy::single_component_path_imports)]
use uat_steps::{
    cli_profile_by_name_loading, cli_requires_resin_for_recipe_fields,
    cli_temperature_flag_validation, ctb_layer_height_authority, cure_depth_nan_guard,
    legacy_resin_toml_defaults, legacy_resin_toml_without_recipe,
    legacy_resin_toml_without_ref_lift_speed, recipe_inside_printer_range, recipe_out_of_range,
    resin_switch_changes_simulation, safety_factor_zero_force,
    suction_detector_raft_false_positive, thermal_degradation,
};

/// Specs that have NO step-definition module yet, so every scenario in them
/// is reported by cucumber as skipped (undefined steps).
///
/// This list is a debt register, not a permanent exemption. It exists because
/// step-def authoring stopped after the 2026-04 rollout while spec authoring
/// continued: everything here is documentation whose behaviour is enforced by
/// ordinary nextest tests. The unskip campaign REMOVES entries; nothing should
/// ever be added. A new un-stepped spec fails
/// `assert_unstepped_specs_match_allowlist` rather than being absorbed.
const SPECS_WITHOUT_STEP_DEFS: &[&str] = &[
    "athena-analytic-log-ingest",
    "base-adhesion-shifts-peel-peak",
    "calibration-disclosure-3of3-predicate",
    "cli-report-health-layer-height-provenance",
    "cli-report-health-print-time",
    "cli-sim-budget-mismatch-on-load",
    "cli-sim-producer-writes-sim-json",
    "cli-sim-rejects-tampered-sidecar",
    "cli-sim-rejects-unknown-schema-version",
    "cli-sim-voxel-cure-emits-tier2-thermal-log",
    "cross-feature-toml-interchange",
    "cumulative-times-sec-accessor",
    "honest-zero-yield-fraction-on-calibrated-solid",
    "interlayer-crack-knockdown-scales-with-perimeter",
    "light-crosstalk-3d-gaussian-convolution",
    "nanodlp-archive-bomb-rejected",
    "nanodlp-calibrate-compares-real-force",
    "nanodlp-import-simulates",
    "peel-shape-factor-scales-with-aspect-ratio",
    "printer-envelope-min-extent-under-field-sim",
    "profile-vacuum-pressure-scales-suction",
    "sim-fields-sidecar-roundtrip",
    "sim-json-roundtrips-zero-force-layer",
    "thermal-field-arrhenius-per-voxel",
    "thermal-field-sidecar-roundtrip",
    "viz-allow-mismatch-soft-fallback",
    "viz-arrow-key-step-no-mesh-reupload",
    "viz-arrow-keys-step-layer-with-saturation",
    "viz-layer-count-mismatch-hard-error",
    "viz-load-ctb-with-sim-renders-heatmap",
    "viz-load-sim-missing-sidecar",
    "viz-load-sim-without-ctb-bad-pairing",
    "viz-screenshot-flag",
    "viz-timeline-click-seeks-current-layer",
    "viz-timeline-drag-pan-does-not-seek",
    "viz-timeline-safety-log-toggle-handles-infinite-sf",
    "viz-timeline-series-toggle-rescales-y",
    "voxel-cure-field-photoinitiator-depletion",
];

/// Step-def modules whose file name does not match their spec's file name.
/// Kept explicit so the allowlist check does not silently treat a stepped
/// spec as un-stepped.
const STEP_DEF_MODULE_RENAMES: &[(&str, &str)] =
    &[("recipe_out_of_range", "recipe-outside-printer-range")];

/// Guard (a): the set of specs lacking step definitions must EXACTLY equal
/// `SPECS_WITHOUT_STEP_DEFS`.
///
/// Checked statically against `uat_steps/mod.rs` rather than from cucumber's
/// runtime stats, because `StatsWriter` reports skipped steps only in
/// aggregate — it cannot say WHICH spec they came from, so an aggregate count
/// could not distinguish "an allowlisted spec skipped, as expected" from "a
/// stepped spec has a coverage gap".
fn assert_unstepped_specs_match_allowlist(spec_uat: &std::path::Path) {
    // Embedded at compile time rather than read at runtime: `file!()` is
    // workspace-relative while the test's CWD is the crate directory, and
    // CARGO_MANIFEST_DIR would re-introduce a path that can drift from the
    // actual module tree. include_str! simply cannot disagree with the
    // `mod` declarations it is next to.
    let mod_src: &str = include_str!("uat_steps/mod.rs");

    const NON_STEP_MODULES: [&str; 5] = [
        "extract",
        "extract_tests",
        "world",
        "fixtures",
        "cli_fixtures",
    ];

    let mut stepped: std::collections::BTreeSet<String> = mod_src
        .lines()
        .filter_map(|l| l.trim().strip_prefix("pub mod "))
        .filter_map(|l| l.strip_suffix(';'))
        .filter(|m| !NON_STEP_MODULES.contains(m))
        .map(|m| {
            STEP_DEF_MODULE_RENAMES
                .iter()
                .find(|(module, _)| *module == m)
                .map_or_else(|| m.replace('_', "-"), |(_, spec)| (*spec).to_string())
        })
        .collect();
    // A rename target that no longer exists would silently widen the "stepped"
    // set, so prove each one still resolves to a real spec.
    for (module, spec) in STEP_DEF_MODULE_RENAMES {
        assert!(
            spec_uat.join(format!("{spec}.md")).exists(),
            "STEP_DEF_MODULE_RENAMES maps {module} to {spec}.md, which does not exist"
        );
    }

    let all_specs: std::collections::BTreeSet<String> = std::fs::read_dir(spec_uat)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", spec_uat.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(str::to_owned))
        .collect();

    stepped.retain(|m| all_specs.contains(m));
    let actual: std::collections::BTreeSet<&str> = all_specs
        .iter()
        .map(String::as_str)
        .filter(|s| !stepped.contains(*s))
        .collect();
    let expected: std::collections::BTreeSet<&str> =
        SPECS_WITHOUT_STEP_DEFS.iter().copied().collect();

    let newly_unstepped: Vec<&&str> = actual.difference(&expected).collect();
    let newly_stepped: Vec<&&str> = expected.difference(&actual).collect();

    assert!(
        newly_unstepped.is_empty(),
        "{} spec(s) have no step definitions and are not on the allowlist: {:?}\n\
         Every scenario in them is silently skipped. Either write the step-def \
         module, or — if that is genuinely deferred — add it to \
         SPECS_WITHOUT_STEP_DEFS with the reason in the issue that defers it.",
        newly_unstepped.len(),
        newly_unstepped,
    );
    assert!(
        newly_stepped.is_empty(),
        "{} spec(s) on SPECS_WITHOUT_STEP_DEFS now HAVE step definitions: {:?}\n\
         Remove them from the allowlist — a debt register that does not shrink \
         stops meaning anything.",
        newly_stepped.len(),
        newly_stepped,
    );
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // CLI UAT scenarios subprocess the `resinsim` binary from the
    // sibling resinsim-inspect package. Build once before cucumber runs
    // so step defs find the binary on disk.
    uat_steps::cli_fixtures::ensure_resinsim_built();

    let spec_uat = resolve_spec_uat_dir();

    // Loud-fail when the resolved path is the wrong directory.
    let md_files = uat_steps::extract::validate_spec_uat_dir(&spec_uat)
        .unwrap_or_else(|e| panic!("spec/uat validation failed: {e}"));

    // Stage synthesised .feature files under CARGO_TARGET_TMPDIR.
    let features_dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("spec-uat-features");
    // Clean any prior run's tree so stale files don't resurrect scenarios.
    let _ = std::fs::remove_dir_all(&features_dir);
    std::fs::create_dir_all(&features_dir).expect("create spec-uat-features tempdir");

    for md_path in &md_files {
        let md = std::fs::read_to_string(md_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", md_path.display()));
        let scenarios = uat_steps::extract::extract(&md);
        if scenarios.is_empty() {
            continue;
        }
        let file_stem = md_path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("spec/uat .md files have UTF-8 stems");
        let feature_title = file_stem.replace('-', " ");
        let feature_text = uat_steps::extract::synthesize_feature(&feature_title, &scenarios);
        let feature_path = features_dir.join(format!("{file_stem}.feature"));
        std::fs::write(&feature_path, feature_text)
            .unwrap_or_else(|e| panic!("write {}: {e}", feature_path.display()));
    }

    let writer = UatWorld::cucumber().run(&features_dir).await;

    let total_steps = writer.passed_steps() + writer.skipped_steps() + writer.failed_steps();
    assert!(
        total_steps > 0,
        "no cucumber steps ran — check synthesised tree at {}",
        features_dir.display(),
    );

    // Coverage guard (c): NO scenario may be lost at parse time.
    //
    // This guard was missing until `uat-gherkin-coverage-guard-panic`. A file
    // whose Gherkin is malformed is not reported as a failure — cucumber
    // counts it in a "parsing errors" summary line and drops every scenario in
    // it. With only the skipped-steps assertion below, 20 files and 54
    // authored scenarios were vanishing with nothing able to fail the build.
    //
    // Authoring-time detection lives in the nextest-visible
    // `spec_gherkin_wellformed` target; this is the runtime backstop.
    assert_eq!(
        writer.parsing_errors(),
        0,
        "coverage guard (c) failed: {} spec/uat file(s) produced unparseable \
         Gherkin, so every scenario in them was silently dropped. Run \
         `cargo nextest run -p resinsim-core --test spec_gherkin_wellformed` \
         for the per-file parser errors.",
        writer.parsing_errors(),
    );

    // Step 8 coverage guard (a): every extracted scenario has matched
    // step bodies — no scenario step remains undefined. Cucumber-rs
    // reports undefined steps as "skipped" (not "failed"), and silent-
    // green would otherwise let a missing step def slip through the
    // execution_has_failed guard below.
    //
    // TEMPORARILY AN ALLOWLIST, not `== 0`. The original assertion assumed
    // every spec would get step definitions. In practice step-def authoring
    // stopped after the 2026-04 rollout: 38 of 52 specs are documentation
    // whose behaviour is enforced by ordinary nextest tests instead. Asserting
    // `== 0` therefore kept the whole suite red and ungateable, which is why
    // nobody noticed guard (c) was missing.
    //
    // The allowlist SHRINKS as the unskip campaign lands step definitions, and
    // rejects unknown entries so a NEW un-stepped spec fails the build rather
    // than being absorbed silently.
    //
    // Guard (b) — "every registered step regex matched at least one
    // scenario step" — is DOWNGRADED per the plan's decision rule:
    // cucumber-rs's public Writer trait surfaces per-step stats but
    // not the map from registered regex → matched-step count, and the
    // low-level API exploration exceeded the plan's 1 h budget. Dead
    // step regexes are tracked in follow-up issue
    // `uat-coverage-guard-dead-steps`; this harness locks (a) and (c).
    assert_unstepped_specs_match_allowlist(&spec_uat);

    if writer.execution_has_failed() {
        std::process::exit(1);
    }
}

fn resolve_spec_uat_dir() -> std::path::PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
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
