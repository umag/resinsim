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
    cli_temperature_flag_validation, cure_depth_nan_guard,
    legacy_resin_toml_defaults, legacy_resin_toml_without_recipe,
    legacy_resin_toml_without_ref_lift_speed, recipe_inside_printer_range,
    recipe_out_of_range, resin_switch_changes_simulation,
    safety_factor_zero_force, suction_detector_raft_false_positive,
    thermal_degradation,
};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let spec_uat = resolve_spec_uat_dir();

    // Loud-fail when the resolved path is the wrong directory.
    let md_files = uat_steps::extract::validate_spec_uat_dir(&spec_uat)
        .unwrap_or_else(|e| panic!("spec/uat validation failed: {e}"));

    // Stage synthesised .feature files under CARGO_TARGET_TMPDIR.
    let features_dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("spec-uat-features");
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
        let feature_text =
            uat_steps::extract::synthesize_feature(&feature_title, &scenarios);
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
