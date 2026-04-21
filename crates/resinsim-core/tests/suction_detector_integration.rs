//! End-to-end integration tests for `SuctionDetector` → `CavityDetector` via
//! `SimulationRunner`. Reproduces the triage empirical evidence for the
//! suction-detector-raft-false-positive lifecycle.
//!
//! Per plan v6 Step 1.
//!
//! **State: red.** Tests are gated with `#[ignore]` + `todo!()` bodies until
//! Phase B (Step 7) wires the new `CavityDetector` through `SimulationRunner`.

// ---------------------------------------------------------------------------
// Scenario (h): lilith_torso_synthetic_no_suction_critical
// ---------------------------------------------------------------------------

/// End-to-end synthetic: mask stack mimicking the lilith-torso topology from
/// the triage reproduction (raft 0-22, support columns 23+, model body above).
/// Runs through `SimulationRunner::run_from_layer_inputs` and asserts zero
/// `SuctionCup` critical failures.
///
/// This is the always-on CI variant. See `lilith_torso_real_ctb_no_suction_critical`
/// below for the optional real-file-fixture variant.
#[test]
#[ignore = "awaiting Step 7 Phase B integration"]
fn lilith_torso_synthetic_no_suction_critical() {
    todo!(
        "Construct a synthetic mask stack matching lilith-torso topology: \
         fully-solid raft at layers 0-22 (area ≈ 2085 mm² equivalent at voxel \
         resolution), then support-column layout at layers 23-50 (20 discrete \
         ~3x3 columns, gaps touching lateral bbox), then tapering model body \
         above. Build Vec<LayerInput> with these masks + synthetic exposure/lift \
         values. Invoke SimulationRunner::run_from_layer_inputs(...) and assert \
         the resulting simulation contains zero FailureType::SuctionCup critical \
         failures."
    );
}

// ---------------------------------------------------------------------------
// Scenario (i): lilith_torso_real_ctb_no_suction_critical (OPTIONAL)
// ---------------------------------------------------------------------------

/// Optional end-to-end test against the real lilith-torso.ctb fixture, gated
/// behind the `RESINSIM_REAL_CTB_FIXTURE` env var. Not part of default CI;
/// documents how to reproduce the original triage evidence against the real
/// RLE-decoded mask path.
///
/// Example:
/// ```
/// RESINSIM_REAL_CTB_FIXTURE=/Users/mag1/Documents/3d/lilith-torso.ctb \
///   cargo nextest run --run-ignored=all lilith_torso_real
/// ```
#[test]
#[ignore = "optional — requires RESINSIM_REAL_CTB_FIXTURE env var + real CTB fixture"]
fn lilith_torso_real_ctb_no_suction_critical() {
    todo!(
        "Read RESINSIM_REAL_CTB_FIXTURE env var; if unset, panic with a skip \
         message. Parse the CTB via io::ctb::parse_ctb. Run \
         SimulationRunner::run_from_layer_inputs with the parsed layers, \
         elegoo_mars5_ultra printer profile, elegoo_ceramic_grey_v2 resin \
         profile (data-dir-resolved). Assert zero FailureType::SuctionCup \
         critical failures."
    );
}
