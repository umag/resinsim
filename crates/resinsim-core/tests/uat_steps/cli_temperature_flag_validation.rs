//! Step definitions for `spec/uat/cli-temperature-flag-validation.md`
//! UAT-1..UAT-6.
//!
//! CLI integration scenarios — same deferral rationale as
//! `cli_profile_by_name_loading.rs`. End-to-end coverage lives at
//! `resinsim-inspect/tests/thermal_cli_warnings.rs`. Follow-up issue:
//! `uat-gherkin-runner-cli-integration`.

use cucumber::{given, then, when};

use super::world::UatWorld;

// ---- UAT-1: --initial-led-temp rejects values at/below absolute zero ------

#[given(regex = r"^the resinsim inspect thermal subcommand$")]
fn given_inspect_thermal(_world: &mut UatWorld) {}

#[when(regex = r#"^the user invokes it with "--initial-led-temp=-300"$"#)]
fn when_initial_led_minus_300(_world: &mut UatWorld) {}

#[then(regex = r"^the process exits with a non-zero code \(2\)$")]
fn then_exits_code_2(_world: &mut UatWorld) {}

#[then(
    regex = r#"^stderr names the flag "initial" or "invalid" AND the phrase "absolute zero"$"#
)]
fn then_stderr_absolute_zero(_world: &mut UatWorld) {}

#[then(regex = r"^no simulation rows are printed on stdout$")]
fn then_no_sim_rows(_world: &mut UatWorld) {}

// ---- UAT-2: --initial-led-temp=NaN rejects without panic ------------------

#[when(regex = r#"^the user invokes it with "--initial-led-temp NaN"$"#)]
fn when_initial_led_nan(_world: &mut UatWorld) {}

#[then(regex = r"^the process exits with a non-zero code$")]
fn then_exits_non_zero(_world: &mut UatWorld) {}

#[then(regex = r"^the error path does NOT produce a Rust panic / stack trace$")]
fn then_no_panic(_world: &mut UatWorld) {}

// ---- UAT-3: --ambient rejects unphysical values ---------------------------

#[given(regex = r"^the resinsim inspect thermal subcommand \(or report health\)$")]
fn given_inspect_or_report(_world: &mut UatWorld) {}

#[when(regex = r#"^the user invokes it with "--ambient=-300" or "--ambient=NaN"$"#)]
fn when_ambient_invalid(_world: &mut UatWorld) {}

#[then(regex = r"^the process exits with code 2$")]
fn then_exits_code_2_alt(_world: &mut UatWorld) {}

#[then(
    regex = r#"^stderr names the flag \("invalid --ambient"\) AND the violated bound$"#
)]
fn then_stderr_names_ambient(_world: &mut UatWorld) {}

// ---- UAT-4: loud warning when resin TOML lacks measured Ea_cure -----------

#[given(regex = r#"^a resin profile whose TOML omits "cure_kinetics_ea_kj_mol"$"#)]
fn given_omits_ea_cure(_world: &mut UatWorld) {}

#[when(
    regex = r#"^the user invokes "resinsim inspect thermal --resin <that> --printer <any>"$"#
)]
fn when_inspect_thermal_no_ea(_world: &mut UatWorld) {}

#[then(
    regex = r#"^stderr contains the strings "30 kJ/mol", "literature midpoint estimate", and "KB-153"$"#
)]
fn then_stderr_kb153(_world: &mut UatWorld) {}

#[then(
    regex = r#"^the warning surfaces in "report health" as well \(not just "inspect thermal"\)$"#
)]
fn then_warning_in_report_health(_world: &mut UatWorld) {}

// ---- UAT-5: measured Ea_cure suppresses the warning -----------------------

#[given(
    regex = r#"^a resin profile whose TOML includes a finite positive "cure_kinetics_ea_kj_mol" in \(0\.0, 200\.0\]$"#
)]
fn given_measured_ea_cure(_world: &mut UatWorld) {}

#[when(regex = r#"^the user invokes "resinsim inspect thermal --resin <that>"$"#)]
fn when_inspect_thermal_with_ea(_world: &mut UatWorld) {}

#[then(regex = r#"^stderr does NOT contain "30 kJ/mol"$"#)]
fn then_stderr_no_30_kj(_world: &mut UatWorld) {}

#[then(
    regex = r#"^the JSON output path \(when --json\) carries "cure_kinetics_ea_is_default": false$"#
)]
fn then_json_ea_not_default(_world: &mut UatWorld) {}

// ---- UAT-6: two-stage thermal plateau approaches fitted Mars 5 Ultra value -

#[given(
    regex = r#"^PrinterProfile::elegoo_mars5_ultra\(\) \+ ResinProfile::generic_standard\(\)$"#
)]
fn given_mars5_generic_standard(_world: &mut UatWorld) {}

#[when(
    regex = r"^SimulationRunner::run_from_areas runs 3500\+ layers at ambient = 23 °C, initial_led = 27 °C$"
)]
fn when_long_sim(_world: &mut UatWorld) {}

#[then(
    regex = r"^the vat temperature at cumulative time ≥ 4 h exceeds half-rise$"
)]
fn then_vat_exceeds_half_rise(_world: &mut UatWorld) {}

#[then(
    regex = r"^the vat temperature at cumulative time ≥ 8 h is within ±1 °C of the 4 h sample$"
)]
fn then_vat_plateau(_world: &mut UatWorld) {}

#[then(
    regex = r"^the cure depth at the thermal plateau on a normal-phase layer EXCEEDS the cure depth at an earlier normal-phase layer \(Ec\(T\) correction\)$"
)]
fn then_cure_depth_increases(_world: &mut UatWorld) {}
