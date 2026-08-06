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
//
// Verified fault-injection matrix (uat-unskip-band-d steps 4 + 8,
// 2026-08-06; precedent: the matrix in agent_constraints_links.rs). Every
// row was applied to the live tree, run through `cargo uat` and/or `cargo
// uat-field-sim`, its exact message captured, then reverted — never
// committed. Rows 1-4 RE-PROVE the four directions ratified before this
// issue's register/layer rewrite (step 4, against the rewritten guards,
// before any new module landed); rows 5-10 are the six NEW directions this
// issue's config-aware design introduces (step 8).
//
// | # | Injected defect | default (`cargo uat`) | field-sim (`cargo uat-field-sim`) |
// |---|---|---|---|
// | 1 | Undefined one step's regex in a fully-stepped, unregistered spec (row temporarily absent, so this exercises UNEXPECTED not MISMATCH) | RED — "1 spec(s) have skipped scenarios but are NOT on SPECS_WITHOUT_STEP_DEFS: [(\"safety-factor-zero-force\", 1)]" | not run (direction is config-independent by construction) |
// | 2 | Added a stale `both_configs` entry for a spec with 0 actual skips | RED — "1 registered spec(s) now have ZERO actual skipped scenarios: [(\"safety-factor-zero-force\", 1)] (spec, expected)" | not run |
// | 3 | Corrupted an existing count (9 → 8 on `light-crosstalk-3d-gaussian-convolution`) | RED — "1 registered spec(s) have an actual skipped-scenario count that differs from SPECS_WITHOUT_STEP_DEFS: [(\"light-crosstalk-3d-gaussian-convolution\", 8, 9)]" | not run |
// | 4 | Dropped one entry (`thermal_degradation`) from the UNGATED `use uat_steps::{...}` block, `pub mod` line kept | RED — "1 ungated (Always) module(s) declared \`pub mod\` in uat_steps/mod.rs but MISSING from the matching-gate \`use uat_steps::{...}\` block ... [\"thermal_degradation\"]" | not run |
// | 5 | Swapped an asymmetric row's two columns (`per_config(honest-zero, 2, 0)` → `(0, 2)`) | RED — layer 2 MISMATCH: "1 registered spec(s) have an actual skipped-scenario count that differs ... [(\"honest-zero-yield-fraction-on-calibrated-solid\", 0, 2)]" | RED — layer 2 STALE: "1 registered spec(s) now have ZERO actual skipped scenarios: [(\"honest-zero-yield-fraction-on-calibrated-solid\", 2)]" |
// | 6 | Typo'd the feature name on a gated `pub mod` line (`"field-sim"` → `"feild-sim"`) | RED — layer-1/3 parser hard-fail (runtime panic): "uat_steps/mod.rs carries an attribute line the shared cfg-classifying parser does not recognise above a \`pub mod\` line: \"#[cfg(feature = \\\"feild-sim\\\")]\"" | RED — but as a COMPILE error, not the parser panic: `E0432 unresolved import` — the gated `use` block (correctly `#[cfg(feature = "field-sim")]`) tries to import a module whose `pub mod` line is now gated behind the never-true `"feild-sim"`, so it doesn't exist in EITHER real feature state |
// | 7 | Removed the `#[cfg(feature = "field-sim")]` gate from a gated `pub mod` line (`honest_zero_yield_fraction_on_calibrated_solid`) | RED — COMPILE error: `E0599 no associated function or constant named 'run_from_layer_inputs_with_voxel' found for struct 'SimulationRunner'` (the gate was load-bearing, not decorative) | not run (default features is where removing the gate breaks; field-sim was unaffected by construction) |
// | 8 | Deleted the entire SECOND (gated) `use uat_steps::{...}` block, both gated `pub mod` lines kept | RED — layer 3, identical message in both configs: "2 field-sim-gated module(s) declared \`pub mod\` in uat_steps/mod.rs but MISSING from the matching-gate \`use uat_steps::{...}\` block ... [\"calibration_disclosure_3of3_predicate\", \"honest_zero_yield_fraction_on_calibrated_solid\"]" — scenario counts still ran correctly underneath (520/62 field-sim), confirming this is a structural guard, not a functional break | same message (feature-independent static check) |
// | 9 | Typo'd one arm of the `HARNESS_CONFIG` pair (`#[cfg(not(feature = "field-sim"))]` → `#[cfg(not(feature = "field-sam"))]`) | GREEN, unaffected (487/69) — `not("field-sam")` is always true regardless of build, so the typo'd arm alone still defines `HARNESS_CONFIG` correctly by coincidence | RED — COMPILE error: `E0428 the name 'HARNESS_CONFIG' is defined multiple times` (both arms are simultaneously true: real `"field-sim"` on, and `not("field-sam")` always true) |
// | 10 | Gave a SYMMETRIC row (`cross-feature-toml-interchange`, no gated module) unequal columns (`per_config(2, 1)`) | RED — layer 1b: "1 SPECS_WITHOUT_STEP_DEFS row(s) declare DIFFERENT default_features / field_sim counts but own NO field-sim-gated ... step-def module ... [\"cross-feature-toml-interchange\"]" | same message (feature-independent static check) |
//
// Residual mode NOT covered by any single injection above (review finding,
// step 8): an IDENTICAL coordinated typo in BOTH `HARNESS_CONFIG` cfg arms
// (e.g. both say `"feild-sim"`) compiles cleanly in BOTH configs — neither
// arm's condition is ever true, so `HARNESS_CONFIG` is undefined... except
// it isn't a compile error either, because nothing in this file requires
// at least one arm to match; the const would fail to resolve only if
// something actually references it, which it does, so this specific
// double-typo IS still a compile error (`cannot find value HARNESS_CONFIG`)
// in both configs. The genuinely silent residual mode is narrower: a
// coordinated typo that happens to leave EXACTLY ONE arm's condition true
// in each config (mirroring row 9's "wrong but coincidentally consistent"
// shape on BOTH arms at once, e.g. both conditions negated) would compile
// in both configs and silently select the DEFAULT column in both — runtime
// direction 2 (a registered spec's actual count drops to 0 where the
// register still expects a nonzero default-column count while the row is
// gated and actually runs in field-sim) is the detector: gated modules
// still run under real field-sim regardless of what `HARNESS_CONFIG`
// reports, so an asymmetric row's field-sim actual (0) diverges from
// whatever stale column the miscomputed `HARNESS_CONFIG` selected,
// surfacing as a MISMATCH or STALE message shaped like row 5 above — the
// column selector being wrong is still visible through the runtime
// attribution layer even when the marker itself compiles.

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
    athena_analytic_log_ingest, base_adhesion_shifts_peel_peak,
    cli_inspect_field_slices_voxel_field,
    cli_profile_by_name_loading, cli_report_health_layer_height_provenance,
    cli_report_health_print_time, cli_report_health_surfaces_ea_default_advisory,
    cli_requires_resin_for_recipe_fields,
    cli_sim_producer_writes_sim_json, cli_sim_rejects_unknown_schema_version,
    cli_temperature_flag_validation, ctb_layer_height_authority, cumulative_times_sec_accessor,
    cure_depth_nan_guard, interlayer_crack_knockdown_scales_with_perimeter, legacy_resin_toml_defaults,
    legacy_resin_toml_without_recipe, legacy_resin_toml_without_ref_lift_speed,
    light_crosstalk_3d_gaussian_convolution,
    nanodlp_archive_bomb_rejected, nanodlp_calibrate_compares_real_force,
    nanodlp_import_simulates, peel_shape_factor_scales_with_aspect_ratio,
    profile_vacuum_pressure_scales_suction, recipe_inside_printer_range, recipe_out_of_range,
    resin_switch_changes_simulation, safety_factor_zero_force,
    sim_json_roundtrips_zero_force_layer, suction_detector_raft_false_positive,
    thermal_degradation,
};

// SECOND, separately-gated use block (uat-unskip-band-d step 6) — rustc
// rejects `#[cfg]` on an identifier inside a braced `use foo::{a, b}`
// group (proven by this issue's step-1 scratch probe), so every
// field-sim-gated step-def module needs its own `use` block carrying the
// SAME `#[cfg(feature = "field-sim")]` attribute as its `pub mod` line in
// `uat_steps/mod.rs`. `assert_mod_rs_and_use_list_agree`
// (`find_use_uat_steps_blocks`) finds and checks this block by its own
// preceding attributes, independently of the Always block above.
#[cfg(feature = "field-sim")]
#[allow(unused_imports, clippy::single_component_path_imports)]
use uat_steps::{
    calibration_disclosure_3of3_predicate, honest_zero_yield_fraction_on_calibrated_solid,
};

/// Which of the two ADR-0017 feature configurations this harness binary
/// was built with (uat-unskip-band-d step 2). Defined by a MUTUALLY-
/// EXCLUSIVE `#[cfg(feature = "field-sim")]` / `#[cfg(not(feature =
/// "field-sim"))]` attribute PAIR immediately below — never a bare
/// `cfg!(feature = "field-sim")` boolean. The pair matters because a
/// typo'd feature name in EITHER arm becomes a COMPILE error (duplicate
/// `HARNESS_CONFIG` definition) in whichever of the two configs makes
/// both arms true at once, rather than a silently wrong runtime value —
/// `cargo build --workspace` (both configs) already builds both arms
/// every time, so the typo cannot hide there either. A bare `cfg!` would
/// just evaluate `false` on a typo and silently select the wrong column
/// below. See docs/patterns/band-membership-by-symbol.md.
///
/// One variant is constructed by only ONE of the two `#[cfg]` arms below;
/// the OTHER variant is also compared against literally (e.g.
/// `HARNESS_CONFIG == HarnessConfig::FieldSim` in layer 1) from code that
/// is NOT itself cfg-gated, so which variant (if either) shows as
/// "never constructed" under `-D warnings` is build-dependent rather than
/// a stable per-config fact — `#[allow]`, not `#[expect]`, since `expect`
/// requires the exact same lint to fire in every build and this one does
/// not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "not every variant is constructed in every single build; this is by design for a #[cfg]-selected marker type, not a real defect"
)]
enum HarnessConfig {
    Default,
    FieldSim,
}

#[cfg(feature = "field-sim")]
const HARNESS_CONFIG: HarnessConfig = HarnessConfig::FieldSim;
#[cfg(not(feature = "field-sim"))]
const HARNESS_CONFIG: HarnessConfig = HarnessConfig::Default;

/// Human-readable tag for the `[Consolidated total]` line (uat-unskip-
/// band-d step 3), so two `cargo uat` / `cargo uat-field-sim` runs against
/// the same terminal are visibly distinguishable to a human.
const fn harness_config_label(config: HarnessConfig) -> &'static str {
    match config {
        HarnessConfig::Default => "default",
        HarnessConfig::FieldSim => "field-sim",
    }
}

/// One register row: a spec's expected skipped-SCENARIO count in EACH of
/// the two `HARNESS_CONFIG` configs (uat-unskip-band-d step 2). Two
/// `const fn`s build rows so an asymmetric one is greppable at its own
/// call site instead of requiring a second lookup table:
///  - [`both_configs`] — the symmetric majority: `cargo uat` and `cargo
///    uat-field-sim` skip exactly the same count. Used both for specs
///    with no field-sim-gated step-def module at all (every scenario is
///    equally unreachable, or equally covered, in both configs) and for
///    specs uniformly unreachable at the binary-build seam today (see the
///    `cli-sim-*` rows' comments below).
///  - [`per_config`] — a declared CONFIG-ASYMMETRIC row: the two counts
///    differ because a field-sim-gated step-def module makes the spec's
///    scenarios reachable in exactly one config. Layer 1b (below)
///    mechanically enforces the honesty rule this implies: an asymmetric
///    row is only valid if that spec actually owns a field-sim-gated
///    module — see `assert_asymmetric_rows_have_a_gated_module`.
#[derive(Debug, Clone, Copy)]
struct SpecDebt {
    spec: &'static str,
    default_features: usize,
    field_sim: usize,
}

const fn both_configs(spec: &'static str, n: usize) -> SpecDebt {
    SpecDebt {
        spec,
        default_features: n,
        field_sim: n,
    }
}

const fn per_config(spec: &'static str, default_features: usize, field_sim: usize) -> SpecDebt {
    SpecDebt {
        spec,
        default_features,
        field_sim,
    }
}

impl SpecDebt {
    /// The expected skipped-SCENARIO count for the config THIS BINARY was
    /// built with (uat-unskip-band-d step 2) — the ONE site in this file
    /// that reads `HARNESS_CONFIG` to pick between `default_features` and
    /// `field_sim`. Every other place that needs a row's active-config
    /// count calls this rather than re-matching `HARNESS_CONFIG` itself.
    const fn expected_for_active_config(&self) -> usize {
        match HARNESS_CONFIG {
            HarnessConfig::Default => self.default_features,
            HarnessConfig::FieldSim => self.field_sim,
        }
    }
}

/// Debt register: one [`SpecDebt`] row per spec, carrying its expected
/// skipped-SCENARIO count in BOTH harness configs (uat-unskip-band-d step
/// 2 widened this from a single shared `(spec stem, expected skipped
/// SCENARIO count)` — see [`SpecDebt`], [`both_configs`], [`per_config`]).
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
///
/// INCREMENT A3+B (uat-unskip-a3-b, 2026-08-04): paid down five specs whose
/// entry points were re-derived and verified default-features BY SYMBOL
/// before any step was authored (docs/patterns/band-membership-by-symbol.md)
/// — `athena-analytic-log-ingest`, `cumulative-times-sec-accessor`,
/// `nanodlp-import-simulates`, `nanodlp-archive-bomb-rejected`, and
/// `nanodlp-calibrate-compares-real-force`. All five were REMOVED outright
/// (none became declared debt): net scenario debt 95 -> 85 (10 scenarios /
/// 48 steps paid down), register 33 -> 28 entries. Each module's register
/// entry was deleted in the SAME change as the module landing, not in a
/// separate later "shrink" step — the FAULT-INJECTION BRANCH REACHABILITY
/// note above and layer 2's direction-2 check are why a late shrink step is
/// not executable (a registered spec with zero actual skips fails the very
/// next `cargo uat`). This increment also corrects the campaign's earlier
/// "Band B (nanodlp) is in-process" classification: all three nanodlp
/// specs' `When` clauses subprocess the real binary, so they follow the
/// Band C CLI shape through `uat_steps/cli_fixtures.rs`
/// (`ensure_resinsim_built`, `invoke_resinsim`, `CliOutcome`,
/// `workspace_data_dir`); only `cumulative-times-sec-accessor` is genuinely
/// in-process, and the sole in-process exception inside a nanodlp spec is
/// `nanodlp-import-simulates` UAT-2 ("the job is imported"), which calls
/// `io::sliced::parse_sliced` directly.
///
/// INCREMENT C2 (`uat-unskip-c2`, 2026-08-05) closed Band C. It landed
/// three modules covering nine scenarios
/// (`cli-report-health-print-time` 3, `cli-report-health-layer-height-provenance`
/// 3, `cli-report-health-surfaces-ea-default-advisory` 3 — new spec) and
/// removed two register entries outright: print-time 3 -> 0 and provenance
/// 0 -> 0 (its zero-scenario entry, not a scenario-debt paydown — see the
/// C1 pointer above). Net scenario debt 75 -> 72, register 26 -> 24
/// entries. All three specs were verified default-features BY SYMBOL at
/// both the in-process seam (`cmd_report_health`, `PrintSimulation::summary`
/// and its private `phase_times`, `values::layer_height_provenance`) and
/// the binary-build seam (`ensure_resinsim_built` forwards no `--features`)
/// before any step was written, following A2/A3+B's precedent. The
/// provenance promotion was authoring-blocked (untagged fence, non-keyword
/// `Scenario (proposed):`, a wrapped continuation line) rather than
/// step-blocked — the first spec in this campaign to be so.
const SPECS_WITHOUT_STEP_DEFS: &[SpecDebt] = &[
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
    //
    // PAID DOWN as the canonical asymmetric row (uat-unskip-band-d step 7,
    // 2026-08-06): see `calibration_disclosure_3of3_predicate.rs`, which
    // drives `FailurePredictor::predict_strain_failures` DIRECTLY against a
    // synthesised all-zero StrainField + a one-voxel-yielded StressField
    // (a simulation run cannot reach this — yield fraction is honestly
    // zero on the natural fixture; see
    // `honest_zero_yield_fraction_on_calibrated_solid.rs`, this same
    // increment). Default features stay fully unreachable (5 skipped,
    // unchanged); field-sim now steps all 5 runtime scenarios (0 skipped).
    per_config("calibration-disclosure-3of3-predicate", 5, 0),
    // DECLARED DEBT (config-asymmetric field-sim scenarios; uat-unskip-band-d,
    // filed 2026-08-02): all three scenarios need a producer-written
    // sidecar — `--voxel-cure-mm` (main.rs:237-239, `#[cfg(feature =
    // "field-sim")]`) to produce one, `encode_paired_sidecar`
    // (simulation_repo.rs:428) to write it, and `values::field_budget` /
    // `decode_sidecar` (both gated) to read it under a budget — and
    // `ensure_resinsim_built` (cli_fixtures.rs:64-98) forwards NO
    // `--features` to the subprocessed `resinsim` binary, so every one of
    // these symbols is compiled out of the binary under test TODAY,
    // uniformly, in BOTH `cargo uat` and `cargo uat-field-sim`. This is a
    // NEW sub-shape of the Band-D class: asymmetric at the BINARY-BUILD
    // SEAM, not at an in-process `#[cfg]` boundary — `ensure_resinsim_built`
    // would need to forward `--features field-sim` under `#[cfg(feature =
    // "field-sim")]` before these scenarios become reachable at all, at
    // which point they convert into the canonical config-asymmetric shape
    // (register wants N under one alias, 0 under the other) that a single
    // shared `const` register cannot satisfy — exactly the design decision
    // uat-unskip-band-d owns. UAT-3 is additionally marked **future** in
    // the spec itself (depends on an unbuilt SidecarPointer field that
    // stamps the producer's budget into the envelope) — not gated by
    // t2f3.5 v1 at all. See uat-unskip-band-d (NOT
    // uat-fixtures-fieldsim-adr0020-gap, which is the unrelated
    // missing-TOML-fixture-fields constraint).
    //
    // STILL SYMMETRIC (uat-unskip-band-d step 9, 2026-08-06): every symbol
    // this scenario needs is compiled out of the subprocessed `resinsim`
    // binary in BOTH `cargo uat` and `cargo uat-field-sim` TODAY —
    // `ensure_resinsim_built` forwards NO `--features` — so (3, 3) is the
    // honest, uniformly-unreachable count in both columns right now, not
    // yet the canonical asymmetric shape. DECISION this issue OWNS but
    // does NOT implement: `ensure_resinsim_built`
    // (`tests/uat_steps/cli_fixtures.rs`) will forward `--features
    // resinsim-inspect/field-sim` under the SAME `HARNESS_CONFIG` marker
    // AND build into a config-scoped `--target-dir` (so
    // `resinsim_bin_path` resolves from that dir instead of walking up
    // from `current_exe`) — the config-scoped target dir is what stops
    // the two aliases from sharing one `target/<profile>/resinsim` and
    // flip-flopping rebuilds (the half-written-binary SIGKILL hazard
    // `docs/patterns/isolated-target-dir-for-concurrent-sessions.md`
    // exists to avoid). Landing the `--features` forward WITHOUT the
    // target-dir split would reintroduce exactly that flip-flop, so the
    // two must land together. Once they do, this row
    // converts to `per_config(spec, 3, 0)` in the follow-up increment that
    // owns the binary-build-seam implementation.
    both_configs("cli-sim-budget-mismatch-on-load", 3),
    // DECLARED DEBT (config-asymmetric field-sim scenarios; uat-unskip-band-d,
    // filed 2026-08-02): all four scenarios need a producer-written sidecar
    // (`--voxel-cure-mm` main.rs:237-239, `encode_paired_sidecar`
    // simulation_repo.rs:428, both `#[cfg(feature = "field-sim")]`) and the
    // consumer `load_and_install_sidecar_with_budget`
    // (simulation_repo.rs:718, called only from the `#[cfg(feature =
    // "field-sim")]` branch at :677-680) — and `ensure_resinsim_built`
    // (cli_fixtures.rs:64-98) forwards NO `--features` to the subprocessed
    // `resinsim` binary, so every one of these symbols is compiled out of
    // the binary under test TODAY, uniformly, in BOTH `cargo uat` and
    // `cargo uat-field-sim`. Same NEW Band-D sub-shape as
    // `cli-sim-budget-mismatch-on-load` above — asymmetric at the
    // BINARY-BUILD SEAM, not at an in-process `#[cfg]` boundary;
    // `ensure_resinsim_built` forwarding `--features field-sim` under
    // `#[cfg(feature = "field-sim")]` is the design decision that converts
    // these into the canonical config-asymmetric shape uat-unskip-band-d
    // owns.
    //
    // UAT-3 (path traversal) was checked specifically as a possible
    // ENVELOPE-LEVEL exception — a crafted `fields_sidecar.path =
    // "../escape.bin"` pointer parses fine on default features
    // (`#[serde(default)] fields_sidecar: Option<SidecarPointer>`,
    // simulation_repo.rs:87, carries no `#[cfg]`) — and is NOT one: the
    // only consumer of that pointer is the gated call site
    // (simulation_repo.rs:677-680, reached only from
    // `load_and_install_sidecar_with_budget` at :718), so a crafted
    // pointer is silently IGNORED and `report health --in` exits 0 rather
    // than rejecting it. See uat-unskip-band-d (NOT
    // uat-fixtures-fieldsim-adr0020-gap).
    //
    // STILL SYMMETRIC (uat-unskip-band-d step 9, 2026-08-06): same
    // uniform-unreachability reasoning and the same pending DECISION as
    // `cli-sim-budget-mismatch-on-load` above (`ensure_resinsim_built`
    // forwards NO `--features` today) — this row converts to
    // `per_config(spec, 4, 0)` once `ensure_resinsim_built` forwards
    // `--features` under `HARNESS_CONFIG` AND builds into a config-scoped
    // `--target-dir`, in the same change.
    both_configs("cli-sim-rejects-tampered-sidecar", 4),
    // BAND MEMBERSHIP NOT YET DERIVED BY SYMBOL (uat-unskip-band-d step 9,
    // 2026-08-06, grouped note over this row and the five below down to
    // `thermal-field-sidecar-roundtrip`): unlike the rows above (derived
    // by A2/C1 per docs/patterns/band-membership-by-symbol.md) and the
    // three specs this increment lands (light-crosstalk, honest-zero,
    // calibration-disclosure), these six specs' entry-point symbols have
    // never been walked and grepped one at a time. Their columns are
    // equal BY CONSTRUCTION here, not by derivation: `both_configs` is
    // correct for ANY spec with no field-sim-gated step-def module in
    // EITHER config, because "no module in either config" means every
    // scenario skips in both, regardless of what the eventual entry-point
    // symbols turn out to be gated on. Symbol derivation — and a possible
    // conversion to `per_config` for any row whose true entry point turns
    // out to be an in-process `#[cfg(feature = "field-sim")]` boundary
    // rather than a binary-build-seam one — is a precondition of the
    // increment that scopes each of these rows individually, not assumed
    // here.
    both_configs("cli-sim-voxel-cure-emits-tier2-thermal-log", 1),
    both_configs("cross-feature-toml-interchange", 2),
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
    //
    // PAID DOWN as the first ASYMMETRIC row (uat-unskip-band-d step 6,
    // 2026-08-06): see `honest_zero_yield_fraction_on_calibrated_solid.rs`.
    // Default features stay fully unreachable (2 skipped, unchanged);
    // field-sim now steps both scenarios (0 skipped).
    per_config("honest-zero-yield-fraction-on-calibrated-solid", 2, 0),
    // PAID DOWN (uat-unskip-band-d step 5, 2026-08-06): UAT-5/UAT-6/UAT-7
    // (the σ = 0.0 / σ > MAX_SIGMA_UM validate-time rejections) landed as
    // an UNGATED module — see `light_crosstalk_3d_gaussian_convolution.rs`'s
    // module doc for the grep evidence that `PrinterProfile::validate`'s
    // crosstalk block sits before the file's only field-sim `#[cfg]` block
    // — dropping this row from 9 to 6 in BOTH columns. UAT-1..4 and
    // UAT-8..9 (the runtime 3D convolution behaviour) stay declared debt:
    // their entry point is the voxel cure path, itself gated, and already
    // covered at the nextest layer by `voxel_cure_crosstalk_integration.rs`
    // per the spec's own "See also" section.
    both_configs("light-crosstalk-3d-gaussian-convolution", 6),
    // PRODUCTION-BLOCKED (uat-unskip-band-d step 9, 2026-08-06): the
    // min-extent check this scenario needs does not exist yet in
    // `PrinterProfile::validate`. The blocking production issue is
    // ALREADY FILED as `printer-envelope-min-extent-validation` — this
    // row stays symmetric at (1, 1) until that issue lands; stepping it
    // now would require a production change outside this test-only
    // lifecycle. Also covered by the six-row "not yet derived by symbol"
    // note above.
    both_configs("printer-envelope-min-extent-under-field-sim", 1),
    both_configs("sim-fields-sidecar-roundtrip", 4),
    both_configs("thermal-field-arrhenius-per-voxel", 2),
    both_configs("thermal-field-sidecar-roundtrip", 3),
    both_configs("viz-allow-mismatch-soft-fallback", 1),
    both_configs("viz-arrow-key-step-no-mesh-reupload", 1),
    both_configs("viz-arrow-keys-step-layer-with-saturation", 1),
    both_configs("viz-layer-count-mismatch-hard-error", 1),
    both_configs("viz-load-ctb-with-sim-renders-heatmap", 1),
    both_configs("viz-load-sim-missing-sidecar", 3),
    both_configs("viz-load-sim-without-ctb-bad-pairing", 1),
    both_configs("viz-screenshot-flag", 12),
    both_configs("viz-timeline-click-seeks-current-layer", 3),
    both_configs("viz-timeline-drag-pan-does-not-seek", 2),
    both_configs("viz-timeline-safety-log-toggle-handles-infinite-sf", 2),
    both_configs("viz-timeline-series-toggle-rescales-y", 2),
    both_configs("voxel-cure-field-photoinitiator-depletion", 6),
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

/// Whether a step-def module compiles in both harness configs, or only
/// under `field-sim` (uat-unskip-band-d step 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModuleGate {
    /// No recognised cfg attribute above the `pub mod` line — compiles (and
    /// is expected to be stepped) in both `cargo uat` and `cargo
    /// uat-field-sim`.
    Always,
    /// `#[cfg(feature = "field-sim")]` above the `pub mod` line — compiled,
    /// linked, and steppable ONLY under `cargo uat-field-sim`.
    FieldSimOnly,
}

/// THE shared cfg-classifying parser (uat-unskip-band-d step 3), used by
/// BOTH layer 1 (`assert_every_spec_has_a_module_or_is_registered`) and
/// layer 3 (`assert_mod_rs_and_use_list_agree`) to read the `declared` side
/// of their respective checks from `uat_steps/mod.rs`'s source text.
///
/// Classifies every `pub mod X;` line (in file order) by the attribute
/// line(s) immediately above it. Only `#[cfg(feature = "field-sim")]` is
/// recognised as a gate; anything else that looks like an attribute
/// directly above a `pub mod` line is a HARD FAILURE (`panic!`) rather than
/// a silent fallback to `Always` — an unrecognised gate read as "always
/// steppable" would misclassify a module that is, in fact, conditionally
/// compiled, recreating the guard-that-cannot-observe-its-own-failure-mode
/// defect this parser exists to close
/// (docs/patterns/anti/guard-that-cannot-observe-its-own-failure-mode.md).
/// `NON_STEP_MODULES` entries are included in the walk (so a gate placed
/// above one of them by mistake is still caught) but excluded from the
/// returned list, same filter layer 1 already applied pre-step-3.
fn classify_step_def_modules(mod_src: &str) -> Vec<(&str, ModuleGate)> {
    let mut out = Vec::new();
    let mut pending_gate: Option<ModuleGate> = None;
    for raw_line in mod_src.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("pub mod ") {
            let name = rest.strip_suffix(';').unwrap_or(rest);
            if !NON_STEP_MODULES.contains(&name) {
                out.push((name, pending_gate.take().unwrap_or(ModuleGate::Always)));
            }
            pending_gate = None;
            continue;
        }
        if line == r#"#[cfg(feature = "field-sim")]"# {
            pending_gate = Some(ModuleGate::FieldSimOnly);
            continue;
        }
        if line.starts_with("//") {
            continue;
        }
        if line.starts_with('#') {
            panic!(
                "uat_steps/mod.rs carries an attribute line the shared \
                 cfg-classifying parser does not recognise above a `pub mod` \
                 line: {line:?}\n\
                 Only `#[cfg(feature = \"field-sim\")]` is understood here — an \
                 unrecognised attribute must be a loud failure, not silently \
                 treated as ModuleGate::Always, or a module's actual \
                 reachability could disagree with what the register expects \
                 in one of the two harness configs.",
            );
        }
        // Any other line (a doc comment continuation, blank-adjacent code,
        // etc.) between two `pub mod` declarations resets any pending gate
        // — defensive; not expected to trigger against the current file.
        pending_gate = None;
    }
    out
}

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

    // A FieldSimOnly module is only actually stepped when THIS binary was
    // built with the field-sim feature — under default features its
    // `pub mod` line is textually present (cfg doesn't strip source text,
    // only compilation) but the module itself does not compile, so
    // treating it as "stepped" unconditionally would be wrong in exactly
    // the config where it is not (uat-unskip-band-d step 3).
    let mut stepped: std::collections::BTreeSet<String> = classify_step_def_modules(mod_src)
        .into_iter()
        .filter(|(_, gate)| match gate {
            ModuleGate::Always => true,
            ModuleGate::FieldSimOnly => HARNESS_CONFIG == HarnessConfig::FieldSim,
        })
        .map(|(m, _)| {
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
    let registered: std::collections::BTreeSet<&str> =
        SPECS_WITHOUT_STEP_DEFS.iter().map(|d| d.spec).collect();

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

/// Layer 1b (static, uat-unskip-band-d step 3): the honesty rule behind
/// [`per_config`] enforced mechanically — a register row may only carry
/// TWO DIFFERENT counts if that spec actually owns a `FieldSimOnly`
/// step-def module. Without this guard, nothing would stop a row from
/// claiming asymmetry (e.g. `per_config(spec, 3, 0)`) for a spec whose
/// module is `Always` (or has no module at all) — a claim layer 2 could
/// never falsify from a SINGLE run (it only ever observes ONE column per
/// process), so a false asymmetric row could persist for an entire config
/// before the OTHER `cargo uat*` alias ever disagreed with it.
fn assert_asymmetric_rows_have_a_gated_module(mod_src: &str) {
    // Same module-name → spec-name normalisation as layer 1
    // (underscore-to-hyphen, with STEP_DEF_MODULE_RENAMES overrides) — the
    // register's spec names are always hyphenated.
    let field_sim_only: std::collections::BTreeSet<String> = classify_step_def_modules(mod_src)
        .into_iter()
        .filter(|(_, gate)| *gate == ModuleGate::FieldSimOnly)
        .map(|(m, _)| {
            STEP_DEF_MODULE_RENAMES
                .iter()
                .find(|(module, _)| *module == m)
                .map_or_else(|| m.replace('_', "-"), |(_, spec)| (*spec).to_string())
        })
        .collect();

    let ungrounded_asymmetric: Vec<&str> = SPECS_WITHOUT_STEP_DEFS
        .iter()
        .filter(|d| d.default_features != d.field_sim)
        .map(|d| d.spec)
        .filter(|spec| !field_sim_only.contains(*spec))
        .collect();
    assert!(
        ungrounded_asymmetric.is_empty(),
        "{} SPECS_WITHOUT_STEP_DEFS row(s) declare DIFFERENT default_features \
         / field_sim counts but own NO field-sim-gated (`#[cfg(feature = \
         \"field-sim\")]`) step-def module in uat_steps/mod.rs: \
         {ungrounded_asymmetric:?}\n\
         A row may only be asymmetric (per_config) when a gated module backs \
         the claim — either gate the spec's module, or make the row \
         symmetric (both_configs) if the two configs are genuinely equally \
         unreachable/reachable today.",
        ungrounded_asymmetric.len(),
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
/// `expected == 0` entries — none registered today, but the shape stays
/// live for the next spec that lands with untagged fences or another
/// zero-executable-scenario defect (see
/// `spec_gherkin_wellformed.rs::SPECS_WITH_NO_EXECUTABLE_SCENARIOS`) —
/// are exempt from direction 2: their whole point is that actual == expected
/// == 0 is the CORRECT steady state, not a stale entry.
fn assert_runtime_attribution_matches_register(
    actual_skipped: &std::collections::BTreeMap<String, usize>,
) {
    let register: std::collections::BTreeMap<&str, usize> = SPECS_WITHOUT_STEP_DEFS
        .iter()
        .map(|d| (d.spec, d.expected_for_active_config()))
        .collect();

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
    for d in SPECS_WITHOUT_STEP_DEFS {
        let spec = d.spec;
        let expected = d.expected_for_active_config();
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

/// Finds EVERY `use uat_steps::{...};` block in `uat_gherkin.rs` (not just
/// the first — uat-unskip-band-d step 3) by repeated anchor search, and
/// classifies each block's gate from the attribute line(s) immediately
/// preceding it. A gated step-def module's identifier CANNOT live in the
/// same `use` block as an ungated one: rustc rejects `#[cfg]` on an
/// identifier inside a braced `use` group (proven in this issue's step-1
/// scratch probe — `error: expected identifier, found` `#`), so every
/// `FieldSimOnly` module requires its OWN, separately-`#[cfg]`-gated
/// `use uat_steps::{...}` block.
///
/// Recognises exactly two attribute lines immediately above an anchor:
/// `#[cfg(feature = "field-sim")]` (marks the block `FieldSimOnly`) and
/// `#[allow(unused_imports, clippy::single_component_path_imports)]`
/// (neutral — decorates every block for the `-Aunused_imports`-environment
/// reason the file-level doc comment explains). Any OTHER attribute line
/// directly above a block is a hard failure, same rationale as
/// `classify_step_def_modules`: a silently-Always misclassification here
/// would let a gated `use` block's identifiers merge into the wrong gate's
/// expected set.
fn find_use_uat_steps_blocks(this_src: &str) -> Vec<(std::collections::BTreeSet<&str>, ModuleGate)> {
    const ANCHOR_OPEN: &str = "use uat_steps::{";
    const RECOGNISED_ALLOW: &str =
        "#[allow(unused_imports, clippy::single_component_path_imports)]";

    let mut blocks = Vec::new();
    let mut search_from = 0usize;
    while let Some(rel_start) = this_src[search_from..].find(ANCHOR_OPEN) {
        let start = search_from + rel_start;
        let after_anchor = start + ANCHOR_OPEN.len();
        search_from = after_anchor;
        // A REAL `use uat_steps::{` opener is a whole top-level line: the
        // anchor is preceded by nothing but a newline (never mid-line, so
        // this can't hit a doc-comment/prose mention or the `ANCHOR_OPEN`
        // string literal itself, which all have other characters sharing
        // the line) and immediately followed by a newline (the identifiers
        // start on the NEXT line, matching this file's own formatting).
        let starts_a_line = start == 0 || this_src.as_bytes()[start - 1] == b'\n';
        let anchor_ends_the_line = this_src[after_anchor..].starts_with('\n');
        if !starts_a_line || !anchor_ends_the_line {
            continue;
        }
        let body_start = after_anchor + 1;
        let rel_end = this_src[body_start..].find("};").unwrap_or_else(|| {
            panic!("`use uat_steps::{{...}}` block starting at byte {start} must close with `}};`")
        });
        let inner = &this_src[body_start..body_start + rel_end];
        let used: std::collections::BTreeSet<&str> = inner
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        assert!(
            !used.is_empty(),
            "parsed zero identifiers from a `use uat_steps::{{...}}` block \
             starting at byte {start} — the include_str! anchor \
             (`use uat_steps::{{` ... `}};`) likely drifted from this file's \
             actual formatting; fix the parser before trusting this guard.",
        );

        let mut gate = ModuleGate::Always;
        for line in this_src[..start].lines().rev() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if line == r#"#[cfg(feature = "field-sim")]"# {
                gate = ModuleGate::FieldSimOnly;
                continue;
            }
            if line == RECOGNISED_ALLOW {
                continue;
            }
            if line.starts_with('#') {
                panic!(
                    "an unrecognised attribute sits directly above the \
                     `use uat_steps::{{...}}` block starting at byte {start}: \
                     {line:?}\n\
                     Only `#[cfg(feature = \"field-sim\")]` and \
                     `{RECOGNISED_ALLOW}` are understood immediately above a \
                     `use uat_steps::{{...}}` block.",
                );
            }
            break;
        }

        blocks.push((used, gate));
        search_from = body_start + rel_end + 2;
    }
    assert!(
        !blocks.is_empty(),
        "found zero `use uat_steps::{{...}}` blocks in uat_gherkin.rs — the \
         include_str! anchor (`use uat_steps::{{` ... `}};`) likely drifted \
         from this file's actual formatting; fix the parser before trusting \
         this guard.",
    );
    blocks
}

/// Layer 3 (MUST-DECIDE-2, gate-aware since uat-unskip-band-d step 3): the
/// `pub mod X;` declarations in `uat_steps/mod.rs` (minus
/// `NON_STEP_MODULES`) must exactly equal the identifiers used across ALL
/// `use uat_steps::{...}` blocks in `uat_gherkin.rs` — checked PER GATE
/// (`Always` declared == `Always` used; `FieldSimOnly` declared ==
/// `FieldSimOnly` used) plus disjointness (no identifier/module is claimed
/// by both gates on either side). Checking per gate, not just as one
/// merged set, is what makes the dropped-gated-`use`-block fault
/// injection (this issue's step 8, F4) actually red: a merged-set
/// comparison would let a `FieldSimOnly` module satisfy the equality via
/// the `Always` block's surplus, silently widening layer 3 back to
/// invisible exactly the way it was before the per-spec runtime
/// attribution rework.
///
/// `-Aunused_imports` is set globally in `.cargo/config.toml` and this
/// file also carries a local `#[allow(unused_imports)]`, so a module
/// declared in `mod.rs` but missing from every `use` list cannot warn —
/// its `#[given]/#[when]/#[then]` registrations are not PROVEN to link.
/// With `[profile.dev] opt-level = 0` today nothing is dead-code-stripped,
/// so it silently doesn't matter — until a release-profile or LTO run
/// makes it matter, silently.
///
/// Parses both sides via `include_str!` (same technique as layer 1) so
/// this check cannot disagree with the source it guards.
fn assert_mod_rs_and_use_list_agree() {
    let mod_src: &str = include_str!("uat_steps/mod.rs");
    let this_src: &str = include_str!("uat_gherkin.rs");

    let classified = classify_step_def_modules(mod_src);
    let declared_always: std::collections::BTreeSet<&str> = classified
        .iter()
        .filter(|(_, g)| *g == ModuleGate::Always)
        .map(|(m, _)| *m)
        .collect();
    let declared_field_sim: std::collections::BTreeSet<&str> = classified
        .iter()
        .filter(|(_, g)| *g == ModuleGate::FieldSimOnly)
        .map(|(m, _)| *m)
        .collect();

    let mut used_always: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    let mut used_field_sim: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for (ids, gate) in find_use_uat_steps_blocks(this_src) {
        match gate {
            ModuleGate::Always => used_always.extend(ids),
            ModuleGate::FieldSimOnly => used_field_sim.extend(ids),
        }
    }

    let cross_declared: Vec<&&str> = declared_always.intersection(&declared_field_sim).collect();
    assert!(
        cross_declared.is_empty(),
        "{} module(s) are classified BOTH Always and FieldSimOnly in \
         uat_steps/mod.rs — the shared classifier should never produce this; \
         module(s): {cross_declared:?}",
        cross_declared.len(),
    );
    let cross_used: Vec<&&str> = used_always.intersection(&used_field_sim).collect();
    assert!(
        cross_used.is_empty(),
        "{} identifier(s) appear in BOTH the ungated and the field-sim-gated \
         `use uat_steps::{{...}}` blocks in uat_gherkin.rs: {cross_used:?}\n\
         Each module belongs in exactly one block, matching its `pub mod` \
         gate in uat_steps/mod.rs.",
        cross_used.len(),
    );

    for (label, declared, used) in [
        ("ungated (Always)", &declared_always, &used_always),
        ("field-sim-gated", &declared_field_sim, &used_field_sim),
    ] {
        let missing_from_use: Vec<&&str> = declared.difference(used).collect();
        let missing_from_mod: Vec<&&str> = used.difference(declared).collect();
        assert!(
            missing_from_use.is_empty(),
            "{} {label} module(s) declared `pub mod` in uat_steps/mod.rs but \
             MISSING from the matching-gate `use uat_steps::{{...}}` block in \
             uat_gherkin.rs: {missing_from_use:?}\n\
             Their #[given]/#[when]/#[then] registrations are not proven to \
             link — add them to the {label} `use` list.",
            missing_from_use.len(),
        );
        assert!(
            missing_from_mod.is_empty(),
            "{} identifier(s) in uat_gherkin.rs's {label} `use \
             uat_steps::{{...}}` list have NO matching `pub mod` (of that \
             gate) in uat_steps/mod.rs: {missing_from_mod:?}\n\
             Remove the stale entry or add/fix the module declaration.",
            missing_from_mod.len(),
        );
    }
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
    // (untagged fences, or a non-`Scenario:` keyword — no spec is in that
    // state today) never gets an entry, so it never enters the per-feature
    // loop below; such a spec's register entry would stay `(spec, 0)` and
    // both sides would agree.
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
    // own `[Summary]` block (54 of them, one per feature, instead of the
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
        "[Consolidated total] ({}) {} features | {} passed / {} skipped / {} failed steps | {} parsing errors",
        harness_config_label(HARNESS_CONFIG),
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

    // Layer 1b (static, uat-unskip-band-d step 3): every asymmetric row
    // (default_features != field_sim) owns a field-sim-gated module.
    assert_asymmetric_rows_have_a_gated_module(include_str!("uat_steps/mod.rs"));

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
