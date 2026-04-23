//! Sanity test for the widened nextest filter.
//!
//! `.config/nextest.toml` excludes every `uat_*` test binary from the
//! default profile because cucumber-rs's `harness = false` binary
//! (currently `uat_gherkin`) doesn't speak libtest's terse listing
//! protocol. If someone reverts the pattern to a narrow
//! `not binary(uat_gherkin)` OR removes the exclusion entirely, the
//! next `cargo nextest run -p resinsim-core` would abort. This test
//! pins the widened pattern as a first-line defence.
//!
//! Plan step 11 acknowledged a nextest-recursion / lock-contention
//! risk for a shell-out-to-`cargo nextest list` approach; the downgrade
//! chosen here reads the config file directly, which sidesteps recursion
//! entirely and keeps the sanity check robust under parallel test runs.

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
    assert!(
        config.contains(r"not binary(/^uat_/)"),
        "nextest filter must use the widened `not binary(/^uat_/)` pattern so any \
         future `uat_*` cucumber binary is covered; got:\n{config}",
    );

    // Defence-in-depth: also verify the profile is actually `default`.
    assert!(
        config.contains("[profile.default]"),
        "the filter must live under [profile.default] to apply globally; got:\n{config}",
    );
}
