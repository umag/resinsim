// Cucumber-rs harness driving the SECOND, viz-scoped UAT suite
// (docs/adr/0024-second-uat-harness-in-resinsim-viz.md).
//
// This is a SIBLING to `resinsim-core`'s `tests/uat_gherkin.rs`, not a
// replacement or an extension of it. It reuses that harness's DESIGN
// verbatim (per-feature `.run()` for attribution, the silent-green
// guard, the parse-error guard, the three-direction register check) but
// is its own binary with its own register, scoped to `spec/uat/viz-*.md`
// ONLY. `crates/resinsim-core/tests/uat_gherkin.rs`,
// `crates/resinsim-core/tests/uat_steps/` and its
// `SPECS_WITHOUT_STEP_DEFS` register are NOT touched by this harness and
// currently still carry all 12 viz-* entries as debt — see the ADR's
// "Migration plan" for why that double count is deliberate and
// temporary.
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

// Per-spec step-def modules land under `tests/uat_viz_steps/` starting
// with the pilot (viz-uat-cucumber-harness steps 4-5, `viz-screenshot-flag`
// only); every other viz spec stays fully skipped, registered below. This
// commit intentionally ships ZERO step definitions — `mod uat_viz_steps;`
// and the `use` that links its `#[given]/#[when]/#[then]` registrations
// are added in the same change as the first step-def module, not before.

use cucumber::{StatsWriter as _, World};

/// World for viz UAT scenarios. Carries a scenario-scoped
/// [`tempfile::TempDir`] so parallel scenario runs cannot collide on file
/// paths — created lazily via [`VizWorld::tempdir`] rather than in
/// `Default::default()`, since `TempDir` construction is fallible and
/// cucumber's `World` derive requires `Default`. Gains a `last:
/// Option<CliOutcome>` field in the same change that adds the first step
/// module (`uat_viz_steps::viz_cli::CliOutcome`).
#[derive(Debug, Default, World)]
pub struct VizWorld {
    tempdir: Option<tempfile::TempDir>,
}

impl VizWorld {
    /// Return this scenario's tempdir, creating it on first use.
    // #[allow(dead_code)]: unused until the first step-def module (step 4)
    // calls it. Left present (not deleted) so that step's diff is
    // step-defs-only, not step-defs-plus-World-plumbing.
    #[allow(dead_code)]
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
/// Starts (this commit) at all twelve viz specs, FULL scenario count,
/// zero step-def modules — this harness lands green with zero steps
/// written, matching core's original rollout shape. `viz-screenshot-flag`
/// is 12 because UAT-7 is ONE gherkin fence holding FOUR `Scenario:`
/// blocks (7a/7b/7c/7d) — one `ExtractedScenario`, four runtime
/// scenarios; the other eight `UAT-N` headings in that spec are one
/// scenario each (8 + 4 = 12).
const SPECS_WITHOUT_STEP_DEFS: &[(&str, usize)] = &[
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
];

/// Layer 2 (runtime): per-spec attribution of ACTUALLY skipped scenarios
/// matches the register in all three directions. Identical logic to
/// core's `assert_runtime_attribution_matches_register`; duplicated
/// rather than shared because the two registers have different scope
/// (whole directory vs `viz-*` only) and different types would be needed
/// to share the function without over-abstracting a ~30-line check.
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
        "{} viz spec(s) have skipped scenarios but are NOT on \
         SPECS_WITHOUT_STEP_DEFS: {unexpected_skips:?}\n\
         Either write the step-def module, or — if genuinely deferred —\
         add (spec, count) to SPECS_WITHOUT_STEP_DEFS naming the issue \
         that defers it.",
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
        "{} registered viz spec(s) now have ZERO actual skipped scenarios: \
         {stale:?} (spec, expected)\nThe debt was paid down — remove the \
         entry from SPECS_WITHOUT_STEP_DEFS.",
        stale.len(),
    );
    assert!(
        mismatched.is_empty(),
        "{} registered viz spec(s) have an actual skipped-scenario count \
         that differs from SPECS_WITHOUT_STEP_DEFS: {mismatched:?} \
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

#[tokio::main(flavor = "current_thread")]
async fn main() {
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

    // NOT replicated: core's layer-3 `assert_mod_rs_and_use_list_agree`.
    // It exists because `-Aunused_imports` (.cargo/config.toml) makes a
    // missing `use` unable to warn across core's 27 step modules; with
    // ONE step module here the check would be pure noise. Add it back
    // when a second viz step-def module lands.

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
