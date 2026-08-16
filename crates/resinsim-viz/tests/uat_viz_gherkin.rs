// Cucumber-rs harness driving the SECOND, viz-scoped UAT suite
// (docs/adr/0024-second-uat-harness-in-resinsim-viz.md).
//
// This is a SIBLING to `resinsim-core`'s `tests/uat_gherkin.rs`, not a
// replacement or an extension of it. It reuses that harness's DESIGN
// verbatim (per-feature `.run()` for attribution, the silent-green
// guard, the parse-error guard, the three-direction register check) but
// is its own binary with its own register, scoped to `spec/uat/viz-*.md`
// ONLY. `crates/resinsim-core/tests/uat_gherkin.rs` lists every viz-*
// spec in `SPECS_MIGRATED_TO_VIZ_HARNESS` (exempting them from core's
// own register and runtime checks) and carries ZERO viz entries in its
// `SPECS_WITHOUT_STEP_DEFS` — the migration from the transitional
// double-count (ADR-0024) is complete.
//
// Harness flow (mirrors core's, see that file's header comment for the
// full rationale):
// 1. Resolve `spec/uat/` from `CARGO_MANIFEST_DIR` (crate → workspace →
//    repo root), canonicalise, and filter to stems starting with `viz-`.
// 2. Validate the directory the same way core does (every `*.md` must
//    carry `issue:` frontmatter).
// 3. Extract each viz-*.md, synthesise a `Feature:` block per file, write
//    the synthesised tree under `$CARGO_TARGET_TMPDIR/spec-uat-features`.
// 4. Run cucumber ONCE PER FEATURE FILE for per-spec attribution, exactly
//    as core's harness does.
// 5. Four guards: silent-green (per-feature + aggregate), parse-error,
//    the three-direction register check, and a FOURTH guard core has no
//    equivalent for — the harness's spec set must equal exactly the set
//    of `spec/uat/viz-*.md` files on disk, so a viz spec authored by a
//    concurrent lane cannot fall through both harnesses unnoticed. The
//    `viz-` stem prefix is therefore a load-bearing ownership boundary,
//    not merely a naming convention.
//
// Extractor reuse: `extract.rs` is included via a cross-crate `#[path]`,
// the SAME mechanism `tests/spec_gherkin_wellformed.rs` already uses (see
// that file's own `#[path = "uat_steps/extract.rs"]` and extract.rs's own
// "Compiled into TWO binaries" doc comment — this makes it three). The
// module is self-contained (only `pulldown_cmark` + `std`) and forbids
// `super::`/`crate::` paths by its own doc comment, so a third `#[path]`
// consumer is the same already-ratified move, not a new architectural
// step. See the ADR for the two rejected alternatives (duplicate the
// file; promote it to a shared workspace crate) and the revisit trigger
// for the latter.
#[path = "../../resinsim-core/tests/uat_steps/extract.rs"]
mod extract;

// Per-spec step-def modules, landing incrementally under
// `tests/uat_viz_steps/`. Step 4 (viz-uat-cucumber-harness) pilots
// `viz-screenshot-flag`'s tier-A scenarios (UAT-7a/7b/7c; UAT-7d is
// declared debt, see that module's doc comment); step 5 adds tier B
// (UAT-1/3/4/9) in the SAME module. Every other viz spec stays fully
// skipped, registered below.
mod uat_viz_steps;

use cucumber::{StatsWriter as _, World};

use uat_viz_steps::viz_cli::CliOutcome;

// Force every step-def module to link so its `#[given]/#[when]/#[then]`
// registrations reach cucumber-rs's global inventory (same reasoning as
// core's `use uat_steps::{...}` block). `assert_mod_rs_and_use_list_agree`
// (below) cross-checks this list against `mod.rs`'s `pub mod` set.
#[allow(unused_imports)]
use uat_viz_steps::{
    viz_allow_mismatch_soft_fallback, viz_arrow_key_step_no_mesh_reupload,
    viz_arrow_keys_step_layer_with_saturation, viz_bad_pairing,
    viz_layer_count_mismatch_hard_error, viz_load_ctb_with_sim_renders_heatmap,
    viz_load_sim_missing_sidecar, viz_screenshot_ctb, viz_screenshot_flag,
    viz_timeline_click_seeks_current_layer, viz_timeline_drag_pan_does_not_seek,
    viz_timeline_safety_log_toggle_handles_infinite_sf,
    viz_timeline_series_toggle_rescales_y,
};

/// Wrapper for `bevy::app::App` that implements `Debug` (App does not).
/// Used by in-process step-def modules that drive a Bevy App across
/// Given/When/Then step boundaries within a single scenario.
pub struct InProcessApp(pub bevy::app::App);

impl std::fmt::Debug for InProcessApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InProcessApp").finish_non_exhaustive()
    }
}

impl Default for InProcessApp {
    fn default() -> Self {
        Self(bevy::app::App::new())
    }
}

/// World for viz UAT scenarios. Carries the outcome of the last
/// `resinsim-viz` CLI invocation, a path staged by a "When" step for a
/// later setup step to act on before actually launching (see
/// `viz_screenshot_flag`'s UAT-7a step defs for why), and a
/// scenario-scoped [`tempfile::TempDir`] so parallel scenario runs
/// cannot collide on file paths — created lazily via [`VizWorld::tempdir`]
/// rather than in `Default::default()`, since `TempDir` construction is
/// fallible and cucumber's `World` derive requires `Default`.
#[derive(Debug, Default, World)]
pub struct VizWorld {
    pub last: Option<CliOutcome>,
    pub pending_screenshot_path: Option<std::path::PathBuf>,
    tempdir: Option<tempfile::TempDir>,
    /// Set by env-gated Given/When steps when `RESINSIM_SLICED_FIXTURE` is
    /// absent. Shared Then steps check this and return early (trivial pass)
    /// instead of panicking on `world.last.as_ref().expect(...)`.
    pub fixture_skipped: bool,
    /// When set by a mismatch scenario's When step, the shared
    /// `then_stderr_contains` adapts the spec's placeholder layer counts
    /// ("CTB has 100 layers", "sim has 50") to the real fixture's counts.
    /// Documented coupling — see `viz_screenshot_flag.rs`.
    pub expected_mismatch_counts: Option<(usize, usize)>,
    // ---- In-process Bevy App state (arrow-key step defs) ----
    pub in_process_app: Option<InProcessApp>,
    pub slice_handle: Option<bevy::asset::Handle<bevy::mesh::Mesh>>,
    pub colors_before: Option<Vec<[f32; 4]>>,
    pub mesh_count_before: Option<usize>,
    /// In-process sim for toggle step defs (safety-log-toggle,
    /// series-toggle). Constructed in Given steps via resinsim_core
    /// builder; consumed by When/Then steps that call
    /// `build_layer_chart_data` directly.
    pub sim: Option<resinsim_core::simulation::PrintSimulation>,
    /// Chart data produced by `build_layer_chart_data` in a When step.
    pub chart_data: Option<resinsim_viz::ui::plots::LayerChartData>,
    /// View-state snapshot for default-assertion step defs.
    pub panel_state: Option<resinsim_viz::ui::state::BottomPanelState>,
}

impl VizWorld {
    /// Return this scenario's tempdir, creating it on first use.
    pub fn tempdir(&mut self) -> &std::path::Path {
        self.tempdir
            .get_or_insert_with(|| tempfile::tempdir().expect("create scenario tempdir"))
            .path()
    }
}

/// Debt register: `(viz spec stem, expected skipped SCENARIO count)`.
/// SCOPED to `spec/uat/viz-*.md` only — this is the one place this
/// harness's vocabulary deliberately diverges from core's: core's
/// register covers the whole `spec/uat/` directory, this one covers a
/// prefix-filtered subset. See the ADR's ddd section for why that makes
/// the `viz-` prefix load-bearing rather than cosmetic.
///
/// ENV-AWARE REGISTER. Three specs (viz-allow-mismatch-soft-fallback,
/// viz-layer-count-mismatch-hard-error, viz-load-ctb-with-sim-renders-
/// heatmap) and three viz-screenshot-flag scenarios (UAT-2/5/8) have
/// step defs that are env-gated on `RESINSIM_SLICED_FIXTURE`. When the
/// env var IS set, these scenarios run with real assertions and their
/// register entries reflect 0 skips (or reduced counts). When the env
/// var is ABSENT, the step functions pass trivially via
/// `world.fixture_skipped`, so the scenarios still count as PASSED (not
/// skipped) and the same reduced counts apply.
///
/// This is a DELIBERATE departure from the Band-D constant-register
/// principle (`uat_gherkin.rs` documents at length). Justified because
/// the fixture is a 356 MB uncommitted binary — env-gating is the only
/// viable mechanism. The env check is read once at harness startup
/// (effectively a two-value constant, not a per-scenario dynamic
/// check). The viz harness already has a known config-dependent count
/// precedent (viz-load-sim-missing-sidecar's per-config comment below).
///
/// REVISIT TRIGGER: if the viz harness gains a per_config mechanism
/// (like core's SpecDebt), migrate these entries to it and restore the
/// constant register.
///
/// `viz-screenshot-flag` started (step 2) at 12; step 4 piloted
/// UAT-7a/7b/7c → 9; step 5 piloted UAT-1/3/4/9 → 5; UAT-2/5/8
/// stepped (env-gated) → 2; UAT-7d unblocked (custom value_parser
/// on --screenshot accepts empty strings, clap no longer intercepts)
/// → 1 remaining: UAT-6 (needs synthetic egui pointer click,
/// bevy_egui 0.39 limitation).
fn specs_without_step_defs() -> Vec<(&'static str, usize)> {
    vec![
        // PAID DOWN (viz-arrow-keys-stepdefs): both arrow-key specs now
        // have in-process step defs using ButtonInput::press + reset_all.
        // viz_arrow_key_step_no_mesh_reupload.rs (UAT-6) and
        // viz_arrow_keys_step_layer_with_saturation.rs (UAT-5).
        //
        // PAID DOWN (viz-load-sim-missing-sidecar): UAT-1 and UAT-3
        // stepped in viz_load_sim_missing_sidecar.rs. UAT-1's steps use
        // runtime cfg!(feature = "field-sim") with fixture_skipped —
        // without the feature, UAT-1 passes trivially (not skipped).
        // UAT-2 (drag-drop) is declared debt (needs synthetic egui
        // pointer events).
        ("viz-load-sim-missing-sidecar", 1),
        // UAT-6 remains as declared debt (needs synthetic egui pointer
        // click, bevy_egui 0.39 limitation). UAT-7d was unblocked by
        // adding a custom value_parser to --screenshot that accepts
        // empty strings (clap previously intercepted them).
        ("viz-screenshot-flag", 1),
        ("viz-timeline-click-seeks-current-layer", 3),
        ("viz-timeline-drag-pan-does-not-seek", 2),
    ]
}

/// Layer 2 (runtime): per-spec attribution of ACTUALLY skipped scenarios
/// matches the register in all three directions. Identical logic to
/// core's `assert_runtime_attribution_matches_register`; duplicated
/// rather than shared because the two registers have different scope
/// (whole directory vs `viz-*` only) and different types would be needed
/// to share the function without over-abstracting a ~30-line check.
fn assert_runtime_attribution_matches_register(
    actual_skipped: &std::collections::BTreeMap<String, usize>,
) {
    let debt = specs_without_step_defs();
    let register: std::collections::BTreeMap<&str, usize> =
        debt.iter().map(|&(s, n)| (s, n)).collect();

    let unexpected_skips: Vec<(&str, usize)> = actual_skipped
        .iter()
        .filter(|&(_, &count)| count > 0)
        .filter_map(|(spec, &count)| {
            (!register.contains_key(spec.as_str())).then_some((spec.as_str(), count))
        })
        .collect();
    assert!(
        unexpected_skips.is_empty(),
        "{} viz spec(s) have skipped scenarios but are NOT on \
         specs_without_step_defs: {unexpected_skips:?}\n\
         Either write the step-def module, or — if genuinely deferred —\
         add (spec, count) to specs_without_step_defs naming the issue \
         that defers it.",
        unexpected_skips.len(),
    );

    let mut stale: Vec<(&str, usize)> = Vec::new();
    let mut mismatched: Vec<(&str, usize, usize)> = Vec::new();
    for &(spec, expected) in &debt {
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
        "{} registered viz spec(s) now have ZERO actual skipped scenarios: \
         {stale:?} (spec, expected)\nThe debt was paid down — remove the \
         entry from specs_without_step_defs.",
        stale.len(),
    );
    assert!(
        mismatched.is_empty(),
        "{} registered viz spec(s) have an actual skipped-scenario count \
         that differs from specs_without_step_defs: {mismatched:?} \
         (spec, expected, actual). Update the registered count to match.",
        mismatched.len(),
    );
}

/// Guard core has no equivalent for: the harness's spec set (post
/// `viz-` filter) must equal exactly the set of `spec/uat/viz-*.md`
/// files on disk. Without this, a viz spec authored by a concurrent
/// lane that does not start with `viz-` (or that core's harness also
/// fails to pick up) could fall through BOTH harnesses' registers
/// unnoticed — an ownership boundary that is only a naming convention is
/// exactly the kind of unenforced claim
/// `docs/patterns/anti/guard-that-cannot-observe-its-own-failure-mode.md`
/// was harvested about.
fn assert_spec_set_matches_viz_prefix_on_disk(
    spec_uat: &std::path::Path,
    harness_stems: &std::collections::BTreeSet<String>,
) {
    let on_disk: std::collections::BTreeSet<String> = std::fs::read_dir(spec_uat)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", spec_uat.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(str::to_owned))
        .filter(|stem| stem.starts_with("viz-"))
        .collect();

    assert_eq!(
        harness_stems, &on_disk,
        "cargo uat-viz's spec set does not match the on-disk `spec/uat/viz-*.md` \
         set. The `viz-` prefix is a load-bearing ownership boundary between this \
         harness's register and resinsim-core's — a mismatch here means a viz spec \
         can fall through both harnesses unnoticed. Harness sees: {harness_stems:?}\n\
         On disk: {on_disk:?}",
    );
}

/// Layer 3 (structural): the `pub mod` set in `uat_viz_steps/mod.rs`
/// (minus `NON_STEP_MODULES`) must exactly equal the identifiers in the
/// `use uat_viz_steps::{...}` binding in this file. `-Aunused_imports`
/// (.cargo/config.toml) makes a missing `use` unable to warn, so a
/// module declared in `mod.rs` but absent from the `use` list would
/// have its `#[given]/#[when]/#[then]` registrations silently unlinked.
///
/// Simpler than core's version: no feature-gate classification needed
/// (all viz step modules are unconditionally compiled).
fn assert_mod_rs_and_use_list_agree() {
    let mod_src: &str = include_str!("uat_viz_steps/mod.rs");
    let this_src: &str = include_str!("uat_viz_gherkin.rs");

    let non_step: std::collections::BTreeSet<&str> =
        uat_viz_steps::NON_STEP_MODULES.iter().copied().collect();

    let declared: std::collections::BTreeSet<&str> = mod_src
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("pub mod "))
        .filter_map(|rest| rest.strip_suffix(';'))
        .filter(|name| !non_step.contains(name))
        .collect();

    let mut used: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    // Collect the use block(s), handling multi-line `use uat_viz_steps::{
    //     ident1, ident2,
    // };` formatting by joining continuation lines.
    let mut in_use_block = false;
    let mut use_buf = String::new();
    for line in this_src.lines() {
        let trimmed = line.trim();
        if in_use_block {
            use_buf.push(' ');
            use_buf.push_str(trimmed);
            if trimmed.contains('}') {
                in_use_block = false;
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("use uat_viz_steps::") {
            if rest.contains('}') || !rest.contains('{') {
                use_buf = rest.to_string();
            } else {
                use_buf = rest.to_string();
                in_use_block = true;
                continue;
            }
        } else {
            continue;
        }
        // Process completed use statement (single-line or now-joined multi-line).
    }
    let use_text = use_buf.trim_end_matches(';').trim();
    if use_text.starts_with('{') && use_text.contains('}') {
        let inner = &use_text[1..use_text.rfind('}').unwrap()];
        for ident in inner.split(',') {
            let ident = ident.trim();
            if !ident.is_empty() && !ident.contains("::") {
                used.insert(ident);
            }
        }
    } else if !use_text.is_empty() && !use_text.contains("::") {
        used.insert(use_text);
    }

    let missing_from_use: Vec<&&str> = declared.difference(&used).collect();
    let missing_from_mod: Vec<&&str> = used.difference(&declared).collect();
    assert!(
        missing_from_use.is_empty(),
        "{} step-def module(s) declared `pub mod` in uat_viz_steps/mod.rs but \
         MISSING from the `use uat_viz_steps::{{...}}` binding in uat_viz_gherkin.rs: \
         {missing_from_use:?}\n\
         Their #[given]/#[when]/#[then] registrations are not proven to link.",
        missing_from_use.len(),
    );
    assert!(
        missing_from_mod.is_empty(),
        "{} identifier(s) in uat_viz_gherkin.rs's `use uat_viz_steps::{{...}}` have \
         NO matching `pub mod` in uat_viz_steps/mod.rs: {missing_from_mod:?}\n\
         Remove the stale entry or add the module declaration.",
        missing_from_mod.len(),
    );
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Renderer preflight — tier B (UAT-1/3/4/9) needs a live GPU/window.
    // Runs ONCE, before cucumber, and PANICS (never skips) if the
    // environment cannot render. See
    // `uat_viz_steps::viz_screenshot_flag::assert_renderer_available`'s
    // doc comment for why a panic, not a skip.
    uat_viz_steps::viz_screenshot_flag::assert_renderer_available();

    let spec_uat = resolve_spec_uat_dir();

    // Loud-fail when the resolved path is the wrong directory (same
    // validation core uses — every *.md needs `issue:` frontmatter).
    let md_files = extract::validate_spec_uat_dir(&spec_uat)
        .unwrap_or_else(|e| panic!("spec/uat validation failed: {e}"));

    // Scope filter: keep only viz-*.md. This is itself a silent-green
    // surface — if it ever selects zero files the run would be
    // vacuously green — so it gets its own explicit non-empty assertion
    // separate from the aggregate guard below.
    let viz_md_files: Vec<std::path::PathBuf> = md_files
        .into_iter()
        .filter(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|stem| stem.starts_with("viz-"))
        })
        .collect();
    assert!(
        !viz_md_files.is_empty(),
        "the viz-scope filter over {} selected ZERO files — this would make \
         the whole run vacuously green. Either spec/uat/viz-*.md files are \
         missing, or the filter's stem-prefix check has drifted.",
        spec_uat.display(),
    );

    let features_dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("spec-uat-features");
    let _ = std::fs::remove_dir_all(&features_dir);
    std::fs::create_dir_all(&features_dir).expect("create spec-uat-features tempdir");

    let mut feature_files: Vec<(String, std::path::PathBuf)> = Vec::new();
    for md_path in &viz_md_files {
        let md = std::fs::read_to_string(md_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", md_path.display()));
        let scenarios = extract::extract(&md);
        if scenarios.is_empty() {
            continue;
        }
        let file_stem = md_path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("spec/uat .md files have UTF-8 stems")
            .to_string();
        let feature_title = file_stem.replace('-', " ");
        let feature_text = extract::synthesize_feature(&feature_title, &scenarios);
        let feature_path = features_dir.join(format!("{file_stem}.feature"));
        std::fs::write(&feature_path, feature_text)
            .unwrap_or_else(|e| panic!("write {}: {e}", feature_path.display()));
        feature_files.push((file_stem, feature_path));
    }

    let harness_stems: std::collections::BTreeSet<String> =
        feature_files.iter().map(|(stem, _)| stem.clone()).collect();
    assert_spec_set_matches_viz_prefix_on_disk(&spec_uat, &harness_stems);

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

    for (spec, feature_path) in &feature_files {
        let writer = VizWorld::cucumber().run(feature_path).await;

        let passed = writer.passed_steps();
        let skipped = writer.skipped_steps();
        let failed = writer.failed_steps();
        let parsing_errors = writer.parsing_errors();
        let hook_errors = writer.hook_errors();

        // Silent-green guard, PER FEATURE.
        assert!(
            passed + skipped + failed > 0,
            "no cucumber steps ran for viz spec '{spec}' ({}) — check the \
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
        "no cucumber steps ran across the whole viz suite — check {}",
        features_dir.display(),
    );

    // Parse-error guard. `spec_gherkin_wellformed.rs` already covers all
    // of spec/uat/ including the viz files and IS nextest-visible, so
    // this is a runtime backstop rather than a duplicate check.
    assert_eq!(
        total_parsing_errors, 0,
        "coverage guard failed: {total_parsing_errors} spec/uat/viz-*.md file(s) \
         produced unparseable Gherkin, so every scenario in them was silently \
         dropped. Run `cargo nextest run -p resinsim-core --test spec_gherkin_wellformed` \
         for the per-file parser errors.",
    );

    // Three-direction register check.
    let actual_skipped: std::collections::BTreeMap<String, usize> = per_spec
        .iter()
        .map(|(spec, stats)| (spec.clone(), stats.skipped))
        .collect();
    assert_runtime_attribution_matches_register(&actual_skipped);

    assert_mod_rs_and_use_list_agree();

    if total_failed > 0 || total_hook_errors > 0 {
        std::process::exit(1);
    }
}

/// Mirrors `resinsim-core`'s `uat_gherkin.rs::resolve_spec_uat_dir`, but
/// verified rather than copied blindly: `resinsim-viz`'s
/// `CARGO_MANIFEST_DIR` is `crates/resinsim-viz`, the SAME ancestor depth
/// as `resinsim-core`'s `crates/resinsim-core` (crate → `crates/` →
/// repo root), so `.nth(2)` is correct here too — but this function
/// proves it with `canonicalize()` + `exists()` rather than assuming the
/// coincidence holds.
fn resolve_spec_uat_dir() -> std::path::PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest
        .ancestors()
        .nth(2)
        .expect("CARGO_MANIFEST_DIR has crate + workspace + repo ancestors");
    let spec_uat = repo_root.join("spec/uat");
    assert!(
        spec_uat.exists(),
        "resolve_spec_uat_dir: {} does not exist — CARGO_MANIFEST_DIR ancestor \
         depth (.nth(2)) no longer resolves to the repo root for resinsim-viz; \
         verify the crate's directory depth has not changed",
        spec_uat.display(),
    );
    spec_uat.canonicalize().unwrap_or_else(|e| {
        panic!(
            "failed to canonicalise spec/uat under {}: {e}",
            repo_root.display()
        )
    })
}
