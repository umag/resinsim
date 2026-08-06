//! Step definitions for
//! `spec/uat/light-crosstalk-3d-gaussian-convolution.md` UAT-5/UAT-6/UAT-7
//! (uat-unskip-band-d step 5) — the three σ-validation scenarios
//! `PrinterProfile::validate` rejects at the TOML load boundary. UAT-1..4
//! and UAT-8..9 (the runtime 3D convolution behaviour) stay declared debt
//! in `SPECS_WITHOUT_STEP_DEFS`: their entry point is the voxel cure path,
//! which touches `#[cfg(feature = "field-sim")]`-adjacent machinery this
//! module does not, and is already covered at the nextest layer by
//! `voxel_cure_crosstalk_integration.rs` per the spec's own "See also"
//! section.
//!
//! BAND MEMBERSHIP BY SYMBOL (docs/patterns/band-membership-by-symbol.md,
//! plan step 1(c), grep evidence recorded here per that pattern's
//! instruction): `PrinterProfile::validate`'s crosstalk σ block in
//! `crates/resinsim-core/src/entities/printer_profile.rs` —
//! ```text
//! for (label, value) in [
//!     ("crosstalk_sigma_xy_um", self.crosstalk_sigma_xy_um),
//!     ("crosstalk_sigma_z_um", self.crosstalk_sigma_z_um),
//! ] {
//!     if let Some(s) = value
//!         && (!s.is_finite() || s <= 0.0 || s > MAX_SIGMA_UM)
//!     ...
//! ```
//! — sits BEFORE that file's only `#[cfg(feature = "field-sim")]` block
//! (which starts several checks later, guarding `build_envelope_mm` /
//! `convective_wall_h_w_m2k` / etc. under field-sim). Grepped directly,
//! not assumed: `grep -n '#\[cfg(feature' printer_profile.rs` shows a
//! single hit, well after the crosstalk loop. `MAX_SIGMA_UM` is a plain
//! `pub const`, also ungated. So all three scenarios below are reachable
//! on DEFAULT features — this module is UNGATED, unlike
//! `honest_zero_yield_fraction_on_calibrated_solid.rs` and
//! `calibration_disclosure_3of3_predicate.rs` (both field-sim-gated,
//! landed in later steps of this same increment).

use cucumber::{given, then, when};

use super::world::{PrinterBuilder, UatWorld};

// ---- UAT-5: crosstalk_sigma_xy_um = 0.0 rejected --------------------------

#[given(regex = r"^a printer profile TOML with crosstalk_sigma_xy_um = 0\.0$")]
fn given_sigma_xy_zero(world: &mut UatWorld) {
    world.peel_printer = Some(
        PrinterBuilder::new()
            .with_crosstalk_sigma_xy_um(0.0)
            .build_unvalidated(),
    );
}

// ---- UAT-6: crosstalk_sigma_z_um = 0.0 rejected ---------------------------

#[given(regex = r"^a printer profile TOML with crosstalk_sigma_z_um = 0\.0$")]
fn given_sigma_z_zero(world: &mut UatWorld) {
    world.peel_printer = Some(
        PrinterBuilder::new()
            .with_crosstalk_sigma_z_um(0.0)
            .build_unvalidated(),
    );
}

// ---- UAT-7: crosstalk σ above MAX_SIGMA_UM rejected -----------------------

#[given(regex = r"^a printer profile TOML with crosstalk_sigma_xy_um = 6000\.0$")]
fn given_sigma_xy_above_max(world: &mut UatWorld) {
    world.peel_printer = Some(
        PrinterBuilder::new()
            .with_crosstalk_sigma_xy_um(6000.0)
            .build_unvalidated(),
    );
}

// ---- shared When: "the profile is loaded and validated" ------------------

#[when(regex = r"^the profile is loaded and validated$")]
fn when_profile_loaded_and_validated(world: &mut UatWorld) {
    let printer = world
        .peel_printer
        .take()
        .expect("scenario invariant: Given step populated peel_printer");
    world.peel_validate_err = match printer.validate() {
        Ok(()) => {
            panic!("scenario invariant violated: crosstalk σ TOML unexpectedly passed validate()")
        }
        Err(e) => Some(e),
    };
}

// ---- Then steps -------------------------------------------------------------

#[then(regex = r"^validation fails with an error mentioning crosstalk_sigma_xy_um$")]
fn then_error_mentions_sigma_xy(world: &mut UatWorld) {
    let err = world
        .peel_validate_err
        .as_deref()
        .expect("scenario invariant: When step populated peel_validate_err");
    assert!(
        err.contains("crosstalk_sigma_xy_um"),
        "expected error to mention crosstalk_sigma_xy_um; got: {err}"
    );
}

#[then(regex = r"^validation fails with an error mentioning crosstalk_sigma_z_um$")]
fn then_error_mentions_sigma_z(world: &mut UatWorld) {
    let err = world
        .peel_validate_err
        .as_deref()
        .expect("scenario invariant: When step populated peel_validate_err");
    assert!(
        err.contains("crosstalk_sigma_z_um"),
        "expected error to mention crosstalk_sigma_z_um; got: {err}"
    );
}

#[then(
    regex = r"^validation fails with an error mentioning crosstalk_sigma_xy_um and the upper bound$"
)]
fn then_error_mentions_sigma_xy_and_upper_bound(world: &mut UatWorld) {
    let err = world
        .peel_validate_err
        .as_deref()
        .expect("scenario invariant: When step populated peel_validate_err");
    assert!(
        err.contains("crosstalk_sigma_xy_um"),
        "expected error to mention crosstalk_sigma_xy_um; got: {err}"
    );
    // MAX_SIGMA_UM = 5000.0; PrinterProfile::validate's error text is
    // "..., when present, must be finite, > 0.0 µm, and <= 5000 µm (got
    // 6000)" — checked by literal rustc format!() probe during plan step 1
    // rather than assumed.
    assert!(
        err.contains("5000"),
        "expected error to mention the 5000 µm upper bound (MAX_SIGMA_UM); got: {err}"
    );
}
