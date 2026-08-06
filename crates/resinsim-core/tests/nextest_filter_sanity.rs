//! Sanity test for the widened nextest filter.
//!
//! `.config/nextest.toml` excludes every `uat_*` test binary from the
//! default profile because cucumber-rs's `harness = false` binaries
//! don't speak libtest's terse listing protocol. As of
//! `viz-uat-cucumber-harness`, there are TWO such binaries covered by
//! this one pattern: `uat_gherkin` (resinsim-core) and `uat_viz_gherkin`
//! (resinsim-viz, docs/adr/0024-second-uat-harness-in-resinsim-viz.md).
//! If someone reverts the pattern to a narrow `not binary(uat_gherkin)`
//! OR removes the exclusion entirely, `cargo nextest run --workspace`
//! would abort the moment it tries to enumerate either binary. This test
//! pins the widened pattern as a first-line defence.
//!
//! Plan step 11 acknowledged a nextest-recursion / lock-contention
//! risk for a shell-out-to-`cargo nextest list` approach; the downgrade
//! chosen here reads the config file directly, which sidesteps recursion
//! entirely and keeps the sanity check robust under parallel test runs.
//! It therefore ALREADY tolerates a second `uat_*` binary with no change
//! required — this file's edit is that inferred property made explicit,
//! not a fix. Confirmed by inspection at
//! viz-uat-cucumber-harness step 3: this file was read in full first,
//! and it enumerates no binaries and spawns no subprocess, so a second
//! `uat_*` target could not have broken it.

use std::path::Path;

#[test]
fn nextest_filter_excludes_uat_cucumber_binaries() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("CARGO_MANIFEST_DIR has crate + workspace + repo ancestors");
    let config_path = workspace.join(".config/nextest.toml");
    let config = std::fs::read_to_string(&config_path).unwrap_or_else(|e| {
        panic!("failed to read {}: {e}", config_path.display());
    });

    // Require the widened pattern that catches any future `uat_*`
    // cucumber binary. Narrow `uat_gherkin`-only filters regress here.
    // This is the exact pattern that must keep covering `uat_viz_gherkin`
    // (resinsim-viz's `harness = false` cucumber binary) as well as
    // `uat_gherkin` (resinsim-core's) — both live under the SAME
    // `/^uat_/` regex, so no per-binary change to this pattern is ever
    // needed when a sibling cucumber harness is added.
    assert!(
        config.contains(r"not binary(/^uat_/)"),
        "nextest filter must use the widened `not binary(/^uat_/)` pattern so both \
         uat_gherkin and uat_viz_gherkin (and any future uat_* cucumber binary) stay \
         covered; got:\n{config}",
    );

    // Defence-in-depth: also verify the profile is actually `default`, so
    // the exclusion applies workspace-wide (resinsim-viz included) and
    // not merely to resinsim-core's local package config.
    assert!(
        config.contains("[profile.default]"),
        "the filter must live under [profile.default] to apply globally (including to \
         resinsim-viz's uat_viz_gherkin), not merely to resinsim-core; got:\n{config}",
    );

    // `.config/nextest.toml` must sit at the WORKSPACE root, not inside a
    // per-crate directory, or `[profile.default]` would not reach
    // resinsim-viz at all. Verified structurally: the path this test
    // reads is `workspace.join(".config/nextest.toml")`, where
    // `workspace` is CARGO_MANIFEST_DIR's second ancestor (repo root) —
    // the SAME root both resinsim-core and resinsim-viz's crate
    // directories share as a parent.
    assert!(
        workspace.join("crates/resinsim-viz/Cargo.toml").exists(),
        "workspace root {} does not contain crates/resinsim-viz — the ancestor \
         depth assumption behind 'this nextest.toml covers resinsim-viz too' no \
         longer holds",
        workspace.display(),
    );
}
