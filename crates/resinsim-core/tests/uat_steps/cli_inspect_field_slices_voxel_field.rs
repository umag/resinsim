//! Step definitions for
//! `spec/uat/cli-inspect-field-slices-voxel-field.md`, UAT-1..UAT-7.
//!
//! `cargo uat` runs against a default-features (`field-sim`-off) build
//! of the `resinsim` binary (`cli_fixtures::ensure_resinsim_built()`
//! builds with no `--features` flag), so every scenario in the spec
//! targets the feature-off surface: `--help` visibility, the
//! actionable feature-off error, and parse-time `--slice`/`--field`
//! validation. Every step regex below is scoped with "field
//! subcommand" / "field-sim" / "field value" / "the field inspector"
//! wording specifically to avoid colliding with the generic exit-code
//! / panic-check / "the user invokes" step text already registered in
//! `cli_temperature_flag_validation.rs` and other sibling modules
//! (`docs/patterns/anti/cucumber-step-regex-ambiguity.md` — an earlier
//! draft used the bare `^the user invokes "resinsim (.+)"$` pattern and
//! it produced exactly the ambiguous-match failure that doc warns
//! about, against `cli_temperature_flag_validation.rs`'s UAT-4/UAT-5
//! steps).

use cucumber::{
    given,
    then,
    when,
};

use super::{
    cli_fixtures::invoke_resinsim,
    world::UatWorld,
};

#[given(regex = r"^the default feature-off resinsim build$")]
fn given_default_feature_off_build(_world: &mut UatWorld) {
    // No setup needed: cargo uat's ensure_resinsim_built() always
    // builds the resinsim binary with no --features flag, so every
    // invocation in this module already exercises the feature-off
    // surface. This step exists purely to make the scenario's
    // precondition explicit and readable.
}

#[when(regex = r#"^the user invokes the field inspector as "resinsim (.+)"$"#)]
fn when_user_invokes(world: &mut UatWorld, cmd: String) {
    let args: Vec<&str> = cmd.split_whitespace().collect();
    let outcome = invoke_resinsim(&args, &[]);
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

// ---- UAT-1 ----

#[then(regex = r"^stdout lists the field subcommand$")]
fn then_stdout_lists_field_subcommand(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    assert!(
        stdout.contains("field"),
        "`resinsim inspect --help` must list the `field` subcommand even feature-off; got: {stdout}"
    );
}

// ---- UAT-2 ----

#[then(regex = r"^stdout lists every field subcommand flag$")]
fn then_stdout_lists_every_field_flag(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    for flag in [
        "--in",
        "--field",
        "--slice",
        "--bins",
        "--values",
        "--cured-only",
        "--json",
    ] {
        assert!(
            stdout.contains(flag),
            "`inspect field --help` must list {flag} even feature-off; got: {stdout}"
        );
    }
}

// ---- UAT-3 / UAT-4 ----

#[then(regex = r"^the field subcommand exits with code 2$")]
fn then_field_exits_code_2(world: &mut UatWorld) {
    assert_eq!(
        world.cli_exit_code,
        Some(2),
        "feature-off `inspect field` handler must exit 2; stderr={}",
        world.cli_stderr.as_deref().unwrap_or_default()
    );
}

#[then(regex = r"^stderr names the field-sim Cargo feature$")]
fn then_stderr_names_field_sim_feature(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        stderr.contains("field-sim"),
        "stderr must name the field-sim Cargo feature; got: {stderr}"
    );
}

#[then(regex = r"^stderr names the exact rebuild command$")]
fn then_stderr_names_rebuild_command(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        stderr.contains("cargo build") && stderr.contains("--features"),
        "stderr must name the exact rebuild command (`cargo build --features ...`); got: {stderr}"
    );
}

#[then(regex = r"^the field subcommand's stdout stays empty$")]
fn then_field_stdout_stays_empty(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    assert!(
        stdout.is_empty(),
        "the field subcommand's error path must leave stdout empty, not emit a JSON error \
         envelope; got: {stdout}"
    );
}

// ---- UAT-5 ----

#[then(regex = r"^the field subcommand exits with a nonzero code$")]
fn then_field_exits_nonzero(world: &mut UatWorld) {
    let exit = world.cli_exit_code.unwrap_or(0);
    assert_ne!(
        exit,
        0,
        "the field subcommand must exit nonzero; stderr={}",
        world.cli_stderr.as_deref().unwrap_or_default()
    );
}

#[then(regex = r"^the field subcommand does not panic$")]
fn then_field_does_not_panic(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        !stderr.contains("panicked at") && !stderr.contains("stack backtrace"),
        "a malformed --slice must be a user-facing parse error, not a Rust panic; got: {stderr}"
    );
}

// ---- UAT-6 ----

#[then(regex = r"^stderr explains the non-negative constraint on the slice value$")]
fn then_stderr_explains_non_negative_constraint(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        stderr.contains(">= 0") || stderr.contains("non-negative"),
        "stderr must explain the >= 0 constraint on --slice's value; got: {stderr}"
    );
}

// ---- UAT-7 ----

#[then(regex = r"^stderr names the invalid field value$")]
fn then_stderr_names_invalid_field_value(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        stderr.contains("not-a-real-field") || stderr.contains("invalid value"),
        "clap must name the invalid --field value in its rejection message; got: {stderr}"
    );
}
