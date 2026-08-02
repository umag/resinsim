// Cucumber-rs harness driving the UAT suite (post step-4/6 refactor;
// per-spec attribution added by uat-unskip-campaign increment 1).
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
// 4. Run cucumber ONCE PER FEATURE FILE (not once over the whole tree).
//    `cucumber::World::cucumber()` is a fresh builder each call and
//    `StatsWriter`'s counters are aggregate-only, so scoping one `.run()`
//    per feature is what turns them into PER-SPEC counters — the only
//    way to answer "which spec did this skipped step come from?" using
//    cucumber-rs 0.22's public API (no custom Writer, no low-level event
//    plumbing). Verified re-entrant by a throwaway scratch probe before
//    this rework landed (three `.run()` calls on one World type in one
//    process, non-leaking, independent StatsWriter results each time).
//    Silent-green guard (passed+skipped+failed > 0) and the parse-error
//    guard are both preserved, PER FEATURE and in aggregate.

use cucumber::{StatsWriter as _, World};

mod uat_steps;

use uat_steps::world::UatWorld;

// Force each step-def module to be linked so their `#[given]/#[when]/
// `#[then]` registrations reach cucumber-rs's global inventory. The
// module declarations alone are enough for rustc to compile them; the
// explicit `use` lines below keep them from being dead-code-stripped
// in optimised builds. `assert_mod_rs_and_use_list_agree` (MUST-DECIDE-2)
// proves this list and `uat_steps/mod.rs`'s `pub mod` declarations name
// the same module set, so this comment cannot silently go stale either.
#[allow(unused_imports, clippy::single_component_path_imports)]
use uat_steps::{
    base_adhesion_shifts_peel_peak, cli_inspect_field_slices_voxel_field,
    cli_profile_by_name_loading, cli_requires_resin_for_recipe_fields,
    cli_temperature_flag_validation, ctb_layer_height_authority, cure_depth_nan_guard,
    interlayer_crack_knockdown_scales_with_perimeter, legacy_resin_toml_defaults,
    legacy_resin_toml_without_recipe, legacy_resin_toml_without_ref_lift_speed,
    peel_shape_factor_scales_with_aspect_ratio,
    profile_vacuum_pressure_scales_suction, recipe_inside_printer_range, recipe_out_of_range,
    resin_switch_changes_simulation, safety_factor_zero_force,
    suction_detector_raft_false_positive, thermal_degradation,
};

/// Debt register: `(spec stem, expected skipped SCENARIO count)`.
///
/// Two ways an entry gets its expected count:
///  - **No step-def module at all.** Expected count == the spec's total
///    scenario count; every scenario skips (undefined steps).
///  - **A step-def module exists, but one or more scenarios are
///    INTENTIONALLY left undefined as declared debt.** Worked example,
///    ratified and since paid down: `cli-temperature-flag-validation`'s
///    UAT-4 briefly carried exactly this shape — its trailing "warning
///    surfaces in resinsim sim" step was undefined because `resinsim sim`
///    did not yet emit the KB-153 warning (a PRODUCTION defect uncovered
///    while repairing this spec's regex drift, 2026-08-01). Per
///    `findings-issue-unskip-adversarial.yaml`: the right move was NOT to
///    weaken or re-point the assertion to the surface that happened to
///    work (`report health`/`inspect thermal`) — that would have
///    recreated the exact drift this repair exists to fix. The gap was
///    registered instead, naming the blocking issue
///    (`kb153-warning-missing-from-resinsim-sim`), and the entry was
///    removed once the production fix landed — the single emission seam
///    at `profile_loader::load_resin` (see
///    `uat_steps/cli_temperature_flag_validation.rs`).
///
/// This list is a debt register, not a permanent exemption. It exists
/// because step-def authoring stopped after the 2026-04 rollout while spec
/// authoring continued (plus, now, the odd genuinely-blocked scenario
/// above): everything here is either undocumented-in-code behaviour
/// enforced by ordinary nextest tests, or a named, tracked production gap.
///
/// AMENDED RULE (uat-unskip-campaign increment 1, 2026-08-01): the
/// original rule was "the campaign shrinks this register; nothing is ever
/// added." That absolute no longer holds — `cli-temperature-flag-validation`
/// above is the worked, ratified example: a spec CAN be freshly stepped
/// (module lands, register entry would normally be deleted entirely) and
/// STILL keep exactly one declared-debt entry for a single scenario that is
/// blocked on a named, filed, external issue rather than on missing test
/// authorship. The rule is therefore: an entry may be ADDED only when it
/// names a blocking issue (not "TODO later" — an actual filed issue;
/// `kb153-warning-missing-from-resinsim-sim` was the live instance that
/// motivated this amendment, since closed by the KB-153 seam fix — the
/// register entry it justified has been removed, but the amendment and its
/// worked example stand), and every OTHER change to this register must be
/// a removal or a count decrease. Net scenario-debt (the sum of every
/// count in this list) still
/// monotonically shrinks release over release — a blocking-issue entry
/// trades an anonymous "nobody wrote the step" skip for a named, tracked,
/// externally-visible one; it does not hide debt, it demotes it from
/// silent to accounted-for. A new unexpected skip that does NOT name a
/// blocking issue still fails `assert_runtime_attribution_matches_register`
/// rather than being absorbed.
///
/// `calibration-disclosure-3of3-predicate` is the tree's only Scenario
/// Outline: 3 authored rows expand to 5 RUNTIME scenarios, so its
/// registered count is 5, not 3. A future Examples-table edit moves this
/// number and the guard will (correctly) fail — that is not a guard bug.
///
/// THIRD DEBT CLASS (uat-unskip-campaign increment A2, 2026-08-02):
/// **config-asymmetric field-sim scenarios.** Distinct from "no module"
/// (nobody wrote steps) and "blocked scenario" (one scenario in an
/// otherwise-stepped spec waits on a named production gap, e.g. the
/// `cli-temperature-flag-validation` worked example above) — here EVERY
/// scenario in the spec is unreachable on default features because its
/// only production entry point is `#[cfg(feature = "field-sim")]`, while
/// `cargo uat` and `cargo uat-field-sim` share this ONE `const` register.
/// A step-def module gated the same way would satisfy one config's
/// expected count and violate the other's — the "identical shape in both
/// configs" invariant the harness enforces. A2 established this class BY
/// SYMBOL (grep the exact producer/consumer function for `#[cfg(feature =
/// "field-sim")]`, not by band label or guesswork) for
/// `calibration-disclosure-3of3-predicate` and
/// `honest-zero-yield-fraction-on-calibrated-solid` below; both cite the
/// filed blocking issue `uat-unskip-band-d` (2026-08-02) per this file's
/// amended register rule.
///
/// FAULT-INJECTION BRANCH REACHABILITY (folded from A2's dispatch-1 review):
/// an undefined step in a spec whose register entry is STILL PRESENT
/// (stale or wrong count) surfaces as a MISMATCH (direction 3 below), not
/// as UNEXPECTED (direction 1) — direction 1 only fires once the entry is
/// genuinely ABSENT from this list. A fault-injection probe run BEFORE a
/// spec's entry is removed must temporarily remove it too (and revert),
/// or it will exercise direction 3 instead of the direction it intended.
const SPECS_WITHOUT_STEP_DEFS: &[(&str, usize)] = &[
    ("athena-analytic-log-ingest", 2),
    // DECLARED DEBT (config-asymmetric field-sim scenarios; uat-unskip-band-d,
    // filed 2026-08-02): every one of this Scenario Outline's 5 runtime
    // scenarios needs `FailurePredictor::predict_strain_failures`
    // (failure_predictor.rs:423) — the sole producer of `FailureType::
    // WarpingRisk` in the workspace — which is `#[cfg(feature = "field-sim")]`
    // and consumes `&StrainField` / `&StressField`, themselves
    // `#[cfg(feature = "field-sim")]` re-exports (values/mod.rs:56-61). A
    // step-def module gated the same way would skip under `cargo uat`
    // (register wants 5) and not skip under `cargo uat-field-sim` (register
    // wants 0) — one shared `const` register cannot satisfy both configs at
    // once. See uat-unskip-band-d (NOT uat-fixtures-fieldsim-adr0020-gap,
    // which is the unrelated missing-TOML-fixture-fields constraint).
    ("calibration-disclosure-3of3-predicate", 5),
    ("cli-report-health-layer-height-provenance", 0),
    ("cli-report-health-print-time", 3),
    ("cli-sim-budget-mismatch-on-load", 3),
    ("cli-sim-producer-writes-sim-json", 6),
    ("cli-sim-rejects-tampered-sidecar", 4),
    ("cli-sim-rejects-unknown-schema-version", 4),
    ("cli-sim-voxel-cure-emits-tier2-thermal-log", 1),
    ("cross-feature-toml-interchange", 2),
    ("cumulative-times-sec-accessor", 2),
    // DECLARED DEBT (config-asymmetric field-sim scenarios; uat-unskip-band-d,
    // filed 2026-08-02): both scenarios need voxel-mode
    // `LayerResult.voxel_yield_fraction` / `.strain_magnitude_max`,
    // populated only inside the `#[cfg(feature = "field-sim")]` block at
    // simulation_runner.rs:801-847; the entry point `SimulationRunner::
    // run_from_layer_inputs_with_voxel` (simulation_runner.rs:446-448) is
    // itself feature-gated. On default features both fields are permanently
    // `None`, so `Some(0.0)` is unrepresentable, not merely untested. Same
    // config-asymmetry constraint as calibration-disclosure-3of3-predicate
    // above — see uat-unskip-band-d.
    ("honest-zero-yield-fraction-on-calibrated-solid", 2),
    ("light-crosstalk-3d-gaussian-convolution", 9),
    ("nanodlp-archive-bomb-rejected", 1),
    ("nanodlp-calibrate-compares-real-force", 3),
    ("nanodlp-import-simulates", 2),
    ("printer-envelope-min-extent-under-field-sim", 1),
    ("sim-fields-sidecar-roundtrip", 4),
    ("sim-json-roundtrips-zero-force-layer", 3),
    ("thermal-field-arrhenius-per-voxel", 2),
    ("thermal-field-sidecar-roundtrip", 3),
    ("viz-allow-mismatch-soft-fallback", 1),
    ("viz-arrow-key-step-no-mesh-reupload", 1),
    ("viz-arrow-keys-step-layer-with-saturation", 1),
    ("viz-layer-count-mismatch-hard-error", 1),
    ("viz-load-ctb-with-sim-renders-heatmap", 1),
    ("viz-load-sim-missing-sidecar", 3),
    ("viz-load-sim-without-ctb-bad-pairing", 1),
    ("viz-screenshot-flag", 12),
    ("viz-timeline-click-seeks-current-layer", 3),
    ("viz-timeline-drag-pan-does-not-seek", 2),
    ("viz-timeline-safety-log-toggle-handles-infinite-sf", 2),
    ("viz-timeline-series-toggle-rescales-y", 2),
    ("voxel-cure-field-photoinitiator-depletion", 6),
];

/// Step-def modules whose file name does not match their spec's file name.
/// Kept explicit so the allowlist check does not silently treat a stepped
/// spec as un-stepped.
const STEP_DEF_MODULE_RENAMES: &[(&str, &str)] =
    &[("recipe_out_of_range", "recipe-outside-printer-range")];

/// Modules under `uat_steps/` that are shared support code, not per-spec
/// step-def bindings. Single source for this list — both layer 1 (below)
/// and layer 3 (`assert_mod_rs_and_use_list_agree`) read it from here.
const NON_STEP_MODULES: [&str; 5] = [
    "extract",
    "extract_tests",
    "world",
    "fixtures",
    "cli_fixtures",
];

/// Layer 1 (static): every spec/uat/*.md file with NO step-def module at
/// all must be named in `SPECS_WITHOUT_STEP_DEFS`.
///
/// This is a narrower check than it used to be. It used to also assert the
/// reverse ("a registered spec now has a module — remove it"), but that
/// direction is superseded by layer 2 (`assert_runtime_attribution_matches_register`,
/// below): a spec CAN have a module and still carry a legitimate, counted,
/// declared-debt register entry (see `cli-temperature-flag-validation`
/// above), so "has a module" can no longer imply "must not be registered".
/// Layer 2 catches a genuinely-stale entry (module exists, actual skips
/// dropped to 0, register still claims count > 0) with strictly more
/// precision than a binary module-presence check ever could.
///
/// What layer 1 is still for: it is a compile-time-ish, deterministic
/// check independent of cucumber actually executing correctly, so it
/// catches a totally-unstepped spec (0% coverage) even in an environment
/// where the runtime attribution in `main` didn't run for some reason.
/// Belt and suspenders, not a duplicate of layer 2.
fn assert_every_spec_has_a_module_or_is_registered(spec_uat: &std::path::Path) {
    // Embedded at compile time rather than read at runtime: `file!()` is
    // workspace-relative while the test's CWD is the crate directory, and
    // CARGO_MANIFEST_DIR would re-introduce a path that can drift from the
    // actual module tree. include_str! simply cannot disagree with the
    // `mod` declarations it is next to.
    let mod_src: &str = include_str!("uat_steps/mod.rs");

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
    let unstepped: std::collections::BTreeSet<&str> = all_specs
        .iter()
        .map(String::as_str)
        .filter(|s| !stepped.contains(*s))
        .collect();
    let registered: std::collections::BTreeSet<&str> = SPECS_WITHOUT_STEP_DEFS
        .iter()
        .map(|(name, _)| *name)
        .collect();

    let newly_unstepped: Vec<&&str> = unstepped.difference(&registered).collect();
    assert!(
        newly_unstepped.is_empty(),
        "{} spec(s) have NO step-def module at all and are not on \
         SPECS_WITHOUT_STEP_DEFS: {:?}\n\
         Every scenario in them is silently skipped. Either write the step-def \
         module, or — if that is genuinely deferred — add \
         (spec, expected_skipped_count) to SPECS_WITHOUT_STEP_DEFS with the \
         reason in the issue that defers it.",
        newly_unstepped.len(),
        newly_unstepped,
    );
}

/// Layer 2 (runtime, MUST-DECIDE-1): per-spec attribution of ACTUALLY
/// skipped scenarios, gathered by driving cucumber one feature file at a
/// time in `main` and reading `StatsWriter::skipped_steps()` after each
/// run. Because cucumber-rs halts a scenario at its first undefined step
/// and never attempts the rest — "117 skipped steps" == "117 skipped
/// scenarios" is the metric identity this harness relies on —
/// `skipped_steps()` scoped to a single feature run IS the skipped-scenario
/// count for that spec.
///
/// This closes the blind spot layer 1 cannot see: a spec CAN have a module
/// and still silently drop scenarios if a step regex drifts from the spec
/// text after an edit
/// (docs/patterns/anti/guard-that-cannot-observe-its-own-failure-mode.md).
/// That drift is not hypothetical — it is exactly what orphaned 4 of 6
/// `cli-temperature-flag-validation` scenarios and 1 of 6
/// `suction-detector-raft-false-positive` scenarios post-ADR-0015, both
/// stepped specs, neither on the old static-only register, both silently
/// losing coverage until this layer existed to see it.
///
/// Fails in three directions:
///  1. a spec has skipped scenarios but is NOT in the register — new debt
///     smuggled in (including drift re-appearing in an already-stepped spec).
///  2. a REGISTERED spec (expected > 0) now shows zero actual skips — the
///     debt was paid down; remove the entry. A register that never shrinks
///     stops meaning anything.
///  3. a REGISTERED spec's actual count differs from its expected count —
///     partial progress (or regression) not reflected in the register.
///
/// `expected == 0` entries (currently only
/// `cli-report-health-layer-height-provenance`, whose untagged fences
/// produce zero executable scenarios today — see
/// `spec_gherkin_wellformed.rs::SPECS_WITH_NO_EXECUTABLE_SCENARIOS`) are
/// exempt from direction 2: their whole point is that actual == expected
/// == 0 is the CORRECT steady state, not a stale entry.
fn assert_runtime_attribution_matches_register(
    actual_skipped: &std::collections::BTreeMap<String, usize>,
) {
    let register: std::collections::BTreeMap<&str, usize> =
        SPECS_WITHOUT_STEP_DEFS.iter().copied().collect();

    let unexpected_skips: Vec<(&str, usize)> = actual_skipped
        .iter()
        .filter(|&(_, &count)| count > 0)
        .filter_map(|(spec, &count)| {
            (!register.contains_key(spec.as_str())).then_some((spec.as_str(), count))
        })
        .collect();
    assert!(
        unexpected_skips.is_empty(),
        "{} spec(s) have skipped scenarios but are NOT on \
         SPECS_WITHOUT_STEP_DEFS: {unexpected_skips:?}\n\
         Every scenario in them is silently skipped — either the spec has no \
         step-def module (write one), or an existing module's step regex has \
         drifted from the current spec text (re-point the regex; cf. \
         docs/patterns/anti/guard-that-cannot-observe-its-own-failure-mode.md). \
         If genuinely deferred, add (spec, count) to SPECS_WITHOUT_STEP_DEFS \
         naming the issue that defers it.",
        unexpected_skips.len(),
    );

    let mut stale: Vec<(&str, usize)> = Vec::new();
    let mut mismatched: Vec<(&str, usize, usize)> = Vec::new();
    for &(spec, expected) in SPECS_WITHOUT_STEP_DEFS {
        let actual = actual_skipped.get(spec).copied().unwrap_or(0);
        if expected == 0 {
            if actual != 0 {
                mismatched.push((spec, expected, actual));
            }
            continue;
        }
        if actual == 0 {
            stale.push((spec, expected));
        } else if actual != expected {
            mismatched.push((spec, expected, actual));
        }
    }
    assert!(
        stale.is_empty(),
        "{} registered spec(s) now have ZERO actual skipped scenarios: \
         {stale:?} (spec, expected)\n\
         The debt was paid down — remove the entry from \
         SPECS_WITHOUT_STEP_DEFS. A register that never shrinks stops \
         meaning anything.",
        stale.len(),
    );
    assert!(
        mismatched.is_empty(),
        "{} registered spec(s) have an actual skipped-scenario count that \
         differs from SPECS_WITHOUT_STEP_DEFS: {mismatched:?} \
         (spec, expected, actual). Update the registered count to match — \
         partial progress on a module must be reflected, not silently \
         absorbed.",
        mismatched.len(),
    );
}

/// Layer 3 (MUST-DECIDE-2): the set of `pub mod X;` declarations in
/// `uat_steps/mod.rs` (minus `NON_STEP_MODULES`) must exactly equal the
/// identifier set inside the `use uat_steps::{...}` block above `main`.
///
/// `-Aunused_imports` is set globally in `.cargo/config.toml` and this
/// file also carries a local `#[allow(unused_imports)]`, so a module
/// declared in `mod.rs` but missing from the `use` list cannot warn — its
/// `#[given]/#[when]/#[then]` registrations are not PROVEN to link. With
/// `[profile.dev] opt-level = 0` today nothing is dead-code-stripped, so it
/// silently doesn't matter — until a release-profile or LTO run makes it
/// matter, silently.
///
/// Parses both sides via `include_str!` (same technique as layer 1) so this
/// check cannot disagree with the source it guards. Anchors on the literal
/// `use uat_steps::{` ... `};` span, strips whitespace, splits on commas.
fn assert_mod_rs_and_use_list_agree() {
    let mod_src: &str = include_str!("uat_steps/mod.rs");
    let this_src: &str = include_str!("uat_gherkin.rs");

    let declared: std::collections::BTreeSet<&str> = mod_src
        .lines()
        .filter_map(|l| l.trim().strip_prefix("pub mod "))
        .filter_map(|l| l.strip_suffix(';'))
        .filter(|m| !NON_STEP_MODULES.contains(m))
        .collect();

    const ANCHOR_OPEN: &str = "use uat_steps::{";
    let start = this_src
        .find(ANCHOR_OPEN)
        .expect("uat_gherkin.rs must have a `use uat_steps::{...};` block");
    let block = &this_src[start + ANCHOR_OPEN.len()..];
    let end = block
        .find("};")
        .expect("`use uat_steps::{...}` block must close with `};`");
    let inner = &block[..end];
    let used: std::collections::BTreeSet<&str> = inner
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    assert!(
        !used.is_empty(),
        "parsed zero identifiers from the `use uat_steps::{{...}}` block — \
         the include_str! anchor (`use uat_steps::{{` ... `}};`) likely \
         drifted from this file's actual formatting; fix the parser before \
         trusting this guard.",
    );

    let missing_from_use: Vec<&&str> = declared.difference(&used).collect();
    let missing_from_mod: Vec<&&str> = used.difference(&declared).collect();
    assert!(
        missing_from_use.is_empty(),
        "{} module(s) declared `pub mod` in uat_steps/mod.rs but MISSING \
         from the `use uat_steps::{{...}}` list in uat_gherkin.rs: \
         {missing_from_use:?}\n\
         Their #[given]/#[when]/#[then] registrations are not proven to \
         link — add them to the `use` list.",
        missing_from_use.len(),
    );
    assert!(
        missing_from_mod.is_empty(),
        "{} identifier(s) in uat_gherkin.rs's `use uat_steps::{{...}}` list \
         have NO matching `pub mod` in uat_steps/mod.rs: {missing_from_mod:?}\n\
         Remove the stale entry or add the module declaration.",
        missing_from_mod.len(),
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

    // (spec stem, synthesised .feature path) — one entry per md file that
    // extracted at least one scenario. A file that extracts zero scenarios
    // (untagged fences — `cli-report-health-layer-height-provenance` today)
    // never gets an entry, so it never enters the per-feature loop below;
    // its register entry stays `(spec, 0)` and both sides agree.
    let mut feature_files: Vec<(String, std::path::PathBuf)> = Vec::new();
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
            .expect("spec/uat .md files have UTF-8 stems")
            .to_string();
        let feature_title = file_stem.replace('-', " ");
        let feature_text = uat_steps::extract::synthesize_feature(&feature_title, &scenarios);
        let feature_path = features_dir.join(format!("{file_stem}.feature"));
        std::fs::write(&feature_path, feature_text)
            .unwrap_or_else(|e| panic!("write {}: {e}", feature_path.display()));
        feature_files.push((file_stem, feature_path));
    }

    struct PerSpecStats {
        passed: usize,
        skipped: usize,
        failed: usize,
    }

    let mut per_spec: std::collections::BTreeMap<String, PerSpecStats> =
        std::collections::BTreeMap::new();
    let mut total_passed = 0usize;
    let mut total_skipped = 0usize;
    let mut total_failed = 0usize;
    let mut total_parsing_errors = 0usize;
    let mut total_hook_errors = 0usize;

    // One `UatWorld::cucumber().run()` call PER FEATURE FILE, not once over
    // the whole tree. See the file-level doc comment for why: it is the
    // only way to attribute skipped steps to the spec they came from using
    // cucumber-rs 0.22's public `StatsWriter` API.
    for (spec, feature_path) in &feature_files {
        let writer = UatWorld::cucumber().run(feature_path).await;

        let passed = writer.passed_steps();
        let skipped = writer.skipped_steps();
        let failed = writer.failed_steps();
        let parsing_errors = writer.parsing_errors();
        let hook_errors = writer.hook_errors();

        // Silent-green guard, PER FEATURE (not just in aggregate): an
        // empty/misrouted synthesised feature must fail loudly here rather
        // than silently contributing zero to every counter and hiding
        // inside an otherwise-healthy aggregate total
        // (docs/patterns/silent-green-guard-for-custom-test-harness.md).
        assert!(
            passed + skipped + failed > 0,
            "no cucumber steps ran for spec '{spec}' ({}) — check the \
             synthesised feature file",
            feature_path.display(),
        );

        per_spec.insert(
            spec.clone(),
            PerSpecStats {
                passed,
                skipped,
                failed,
            },
        );
        total_passed += passed;
        total_skipped += skipped;
        total_failed += failed;
        total_parsing_errors += parsing_errors;
        total_hook_errors += hook_errors;
    }

    // Consolidated end table. Each `.run()` call above already printed its
    // own `[Summary]` block (52 of them, one per feature, instead of the
    // single tree-wide summary the old one-shot run produced); this is the
    // single place a reader can see the whole suite's per-spec shape.
    println!("\n[Per-spec attribution]");
    for (spec, stats) in &per_spec {
        println!(
            "  {spec}: {} passed, {} skipped, {} failed",
            stats.passed, stats.skipped, stats.failed
        );
    }
    println!(
        "[Consolidated total] {} features | {} passed / {} skipped / {} failed steps | {} parsing errors",
        feature_files.len(),
        total_passed,
        total_skipped,
        total_failed,
        total_parsing_errors,
    );

    // Aggregate silent-green guard, kept on the summed totals.
    assert!(
        total_passed + total_skipped + total_failed > 0,
        "no cucumber steps ran across the whole suite — check {}",
        features_dir.display(),
    );

    // Coverage guard (c): NO scenario may be lost at parse time.
    //
    // A file whose Gherkin is malformed is not reported as a failure —
    // cucumber counts it in a "parsing errors" summary line and drops every
    // scenario in it. Authoring-time detection lives in the
    // nextest-visible `spec_gherkin_wellformed` target; this is the
    // runtime backstop.
    assert_eq!(
        total_parsing_errors, 0,
        "coverage guard (c) failed: {total_parsing_errors} spec/uat file(s) produced \
         unparseable Gherkin, so every scenario in them was silently dropped. Run \
         `cargo nextest run -p resinsim-core --test spec_gherkin_wellformed` \
         for the per-file parser errors.",
    );

    // Layer 1 (static): every spec either has a module or is registered.
    assert_every_spec_has_a_module_or_is_registered(&spec_uat);

    // Layer 2 (runtime, MUST-DECIDE-1): per-spec skipped-scenario
    // attribution matches the register in both directions and at the exact
    // count.
    let actual_skipped: std::collections::BTreeMap<String, usize> = per_spec
        .iter()
        .map(|(spec, stats)| (spec.clone(), stats.skipped))
        .collect();
    assert_runtime_attribution_matches_register(&actual_skipped);

    // Layer 3 (MUST-DECIDE-2): mod.rs pub-mod set == uat_gherkin.rs use set.
    assert_mod_rs_and_use_list_agree();

    if total_failed > 0 || total_hook_errors > 0 {
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
