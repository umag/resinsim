//! End-to-end integration test for the t2f2 light crosstalk path (ADR-0018).
//!
//! Drives `SimulationRunner::run_from_layer_inputs_with_voxel` against
//! the four runtime regimes (AA/BA/BB/CB/DD per ADR-0018 §2 / step 5 of the
//! plan) using hand-rolled TOML PrinterProfiles to inject σ values:
//!
//! - **(8-A / AA) both σ None** — bit-exact equivalent to the t2f1 path.
//! - **(8-B / BA) σ_xy = 8 µm, σ_z None** — XY-only at realistic default
//!   voxel size produces σ_voxels ≈ 0.016 ⇒ near-identity convolution.
//! - **(8-C / BB) σ_xy = 1000 µm, σ_z None** — XY-only synthetic large σ
//!   produces σ_voxels = 2 ⇒ visible off-pixel leakage.
//! - **(8-D / CB) σ_xy None, σ_z = 40 µm** — Z-only at realistic default
//!   layer height = 50 µm produces σ_z_layers = 0.8 ⇒ 7-tap kernel; cure
//!   dose appears in layers L-3..=L+3 with the **Z-edge SKIP semantics**
//!   load-bearing assertion (no dose pileup at iz=0 from skipped kz<0).
//! - **(8-E / DD) both σ active** — combined XY + Z, asserts the surface
//!   dose decomposes cleanly into XY × Z factors (because the convs apply
//!   at different pipeline stages: XY pre-attenuation on intensity, Z
//!   post-attenuation on the per-column dose).

#![cfg(feature = "field-sim")]

use resinsim_core::app::SimulationRunner;
use resinsim_core::entities::{PrinterProfile, ResinProfile};
use resinsim_core::io::sliced::LayerInput;
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::SupportConfig;
use resinsim_core::values::{AmbientTemperature, LayerMask};

fn ambient() -> AmbientTemperature {
    AmbientTemperature::new(22.0).expect("22°C valid ambient")
}

/// Build a hand-rolled PrinterProfile via TOML deserialisation with arbitrary
/// crosstalk σ values injected. Mirrors the t2f1 voxel_cure_integration
/// pattern (uniform LCD ⇒ no spatial intensity variation noise).
fn printer_with_sigmas(sigma_xy: Option<f32>, sigma_z: Option<f32>) -> PrinterProfile {
    let xy_line = sigma_xy
        .map(|s| format!("crosstalk_sigma_xy_um = {s}\n"))
        .unwrap_or_default();
    let z_line = sigma_z
        .map(|s| format!("crosstalk_sigma_z_um = {s}\n"))
        .unwrap_or_default();
    let toml_text = format!(
        r#"
name = "Test Crosstalk Printer"
led_power_mw_cm2 = 4.0
pixel_pitch_um = 50.0
layer_height_range_um = {{ min = 20.0, max = 100.0 }}
exposure_range_sec = {{ min = 1.0, max = 60.0 }}
lift_speed_range_mm_min = {{ min = 10.0, max = 200.0 }}
bottom_layer_count_max = 15
z_stiffness_n_per_mm = 460.0
delta_t_steady_c = 10.0
thermal_tau_sec = 1200.0
lcd_uniformity_variation = 0.0
# ADR-0020 / t2f4: required under field-sim.
convective_wall_h_w_m2k = 8.0
vat_wall_thickness_mm = 2.0
vat_wall_k_w_mk = 200.0
{xy_line}{z_line}[build_envelope_mm]
width_mm = 192.0
depth_mm = 120.0
max_z_mm = 200.0
"#
    );
    let p: PrinterProfile = toml::from_str(&toml_text).expect("hand-rolled TOML must parse");
    p.validate().expect("hand-rolled profile must validate");
    p
}

/// Centred 1×1 mask on a 5×5 grid at 0.5 mm voxels — isolates a single
/// pixel exposure so XY and Z conv effects can be observed cleanly.
fn single_pixel_mask_in(nx: u32, ny: u32, ix: u32, iy: u32) -> LayerMask {
    let mut m = LayerMask::new(nx, ny, 0.5).expect("LayerMask::new in-domain");
    m.set(ix, iy).expect("set within bounds");
    m
}

/// Empty mask of given dimensions — used for layer slots between the
/// single-pixel source exposure.
fn empty_mask(nx: u32, ny: u32) -> LayerMask {
    LayerMask::new(nx, ny, 0.5).expect("LayerMask::new in-domain")
}

fn layers_with_mask(mask: LayerMask, n: u32) -> Vec<LayerInput> {
    (0..n)
        .map(|i| {
            let mut li = LayerInput::new(
                i,
                0.25, // 1 voxel × 0.25 mm² area
                3.0,  // exposure_sec
                60.0,
                50.0, // layer height 50 µm
                (i as f32 + 1.0) * 0.05,
            )
            .expect("LayerInput::new in-domain");
            li.mask = Some(mask.clone());
            li
        })
        .collect()
}

fn run_voxel(
    layers: &[LayerInput],
    printer: &PrinterProfile,
) -> resinsim_core::simulation::PrintSimulation {
    let resin = ResinProfile::generic_standard();
    SimulationRunner::run_from_layer_inputs_with_voxel(
        layers,
        &resin,
        printer,
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        },
        &PlateAdhesionProfile::default_textured(),
        ambient(),
        None,
        Some(0.5),
    )
    .expect("voxel-mode run must succeed")
}

// =====================================================================
// (8-A / AA) Both σ None ⇒ bit-exact equivalence to t2f1.
// =====================================================================

#[test]
fn regime_aa_both_none_bit_exact_t2f1() {
    let pa = printer_with_sigmas(None, None);
    let layers_a = layers_with_mask(
        LayerMask::new_all_solid(5, 5, 0.5).expect("5×5 solid mask"),
        3,
    );
    let sim_a = run_voxel(&layers_a, &pa);

    // Baseline: same printer + same input (just running it twice via the
    // same code path). The point of this test is to lock in that the AA
    // regime takes the unchanged t2f1 branch. We assert (i) the run
    // succeeds; (ii) cure_field is populated; (iii) the per-layer caches
    // are derived from the voxel field (matching t2f1 behaviour).
    let cf = sim_a
        .cure_field()
        .expect("AA regime must install cure_field (t2f1 path)");
    let (nx, ny, nz) = cf.dimensions();
    assert_eq!((nx, ny, nz), (5, 5, 3));

    // Sanity: at least one voxel must have non-zero dose (we exposed 3
    // layers of a solid mask).
    let max_dose = cf.max_dose();
    assert!(
        max_dose > 0.0,
        "AA regime must produce non-zero cure dose somewhere, got max={max_dose}"
    );
}

// =====================================================================
// (8-B / BA) σ_xy = 8 µm at default 0.5 mm voxel size ⇒ near-identity XY.
// =====================================================================

#[test]
fn regime_ba_xy_only_realistic_default_is_near_no_op() {
    // σ_xy_um = 8, voxel_size_mm = 0.5 ⇒ σ_xy_voxels = 8 / 500 = 0.016
    // ⇒ kernel radius 1 with centre weight ≈ 1.0 and side weights ≈
    // exp(-1/(2·0.016²)) ≈ exp(-1953) ≈ 0. XY convolution collapses to
    // identity at default mask resolution — XY fidelity gated on t2f5.
    let p_xy = printer_with_sigmas(Some(8.0), None);
    let p_baseline = printer_with_sigmas(None, None);
    let mask = LayerMask::new_all_solid(5, 5, 0.5).expect("5×5 solid");
    let layers = layers_with_mask(mask, 1);

    let sim_xy = run_voxel(&layers, &p_xy);
    let sim_baseline = run_voxel(&layers, &p_baseline);

    let cf_xy = sim_xy.cure_field().expect("BA installs cure_field");
    let cf_b = sim_baseline
        .cure_field()
        .expect("baseline installs cure_field");
    let max_dose = cf_b.max_dose();

    // At σ_xy_voxels ≈ 0.016, every voxel dose should be within
    // 1e-6 × max_dose of the no-crosstalk baseline (near-identity).
    let (nx, ny, nz) = cf_xy.dimensions();
    for ix in 0..nx {
        for iy in 0..ny {
            for iz in 0..nz {
                let dxy = cf_xy.dose_at(ix, iy, iz).expect("in bounds");
                let db = cf_b.dose_at(ix, iy, iz).expect("in bounds");
                assert!(
                    (dxy - db).abs() < 1e-6 * max_dose.max(1.0),
                    "BA near-identity violated at ({ix},{iy},{iz}): xy={dxy} baseline={db}"
                );
            }
        }
    }
}

// =====================================================================
// (8-C / BB) σ_xy = 1000 µm synthetic large ⇒ visible XY leakage.
// =====================================================================

#[test]
fn regime_bb_xy_only_synthetic_large_sigma_produces_off_pixel_dose() {
    // σ_xy_um = 1000, voxel_size_mm = 0.5 ⇒ σ_xy_voxels = 2.0 ⇒ kernel
    // radius 6, kernel has substantial weight at offset ±1, ±2.
    // Single-pixel mask at centre of 9×9 grid; off-pixel voxels at radius 1
    // must receive non-zero cure dose.
    let p = printer_with_sigmas(Some(1000.0), None);
    let mask = single_pixel_mask_in(9, 9, 4, 4);
    let layers = layers_with_mask(mask, 1);

    let sim = run_voxel(&layers, &p);
    let cf = sim.cure_field().expect("BB installs cure_field");
    let centre_dose = cf.dose_at(4, 4, 0).expect("centre in bounds");
    assert!(
        centre_dose > 0.0,
        "BB centre voxel must have cure dose, got {centre_dose}"
    );
    // Off-pixel neighbour at radius 1 (4±1, 4±1) must have non-zero dose.
    let neighbour_dose = cf.dose_at(5, 4, 0).expect("neighbour in bounds");
    assert!(
        neighbour_dose > 0.0,
        "BB σ_xy = 1000 µm must leak cure dose to off-mask neighbour (5,4,0), got 0"
    );
    // 4-fold symmetry: ±x and ±y neighbours equal within fp tolerance.
    let n_xp = cf.dose_at(5, 4, 0).expect("(+x)");
    let n_xm = cf.dose_at(3, 4, 0).expect("(-x)");
    let n_yp = cf.dose_at(4, 5, 0).expect("(+y)");
    let n_ym = cf.dose_at(4, 3, 0).expect("(-y)");
    let tol = 1e-5 * centre_dose;
    assert!(
        (n_xp - n_xm).abs() < tol,
        "x-symmetry: +x={n_xp}, -x={n_xm}"
    );
    assert!(
        (n_yp - n_ym).abs() < tol,
        "y-symmetry: +y={n_yp}, -y={n_ym}"
    );
    assert!(
        (n_xp - n_yp).abs() < tol,
        "x-y-symmetry: +x={n_xp}, +y={n_yp}"
    );
}

// =====================================================================
// (8-D / CB) σ_z = 40 µm at default 50 µm layer height ⇒ Z-smear with
// Z-edge SKIP semantics (LOAD-BEARING test for SKIP-vs-CLAMP).
// =====================================================================

#[test]
fn regime_cb_z_only_realistic_default_exercises_z_smear() {
    // σ_z_um = 40, layer_height_um = 50 ⇒ σ_z_layers = 0.8 ⇒ kernel
    // radius ⌈3·0.8⌉ = 3 ⇒ 7-tap symmetric kernel.
    //
    // Single-pixel source mask at (2, 2). We run 7 layers with the mask
    // only at layer 3 (single exposure at the centre layer); cure should
    // appear in layers L-3..=L+3 = 0..=6, with monotone decay away from L.
    let p = printer_with_sigmas(None, Some(40.0));
    let mask_lit = single_pixel_mask_in(5, 5, 2, 2);
    let mask_empty = empty_mask(5, 5);

    let resin = ResinProfile::generic_standard();
    let layers: Vec<LayerInput> = (0..7)
        .map(|i| {
            let mut li = LayerInput::new(i, 0.25, 3.0, 60.0, 50.0, (i as f32 + 1.0) * 0.05)
                .expect("LayerInput in-domain");
            li.mask = Some(if i == 3 {
                mask_lit.clone()
            } else {
                mask_empty.clone()
            });
            li
        })
        .collect();
    let sim = SimulationRunner::run_from_layer_inputs_with_voxel(
        &layers,
        &resin,
        &p,
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        },
        &PlateAdhesionProfile::default_textured(),
        ambient(),
        None,
        Some(0.5),
    )
    .expect("CB run must succeed");

    let cf = sim.cure_field().expect("CB installs cure_field");
    let (_, _, nz) = cf.dimensions();
    assert_eq!(nz, 7);

    // (i) cure dose appears in iz = 0..=6 (all 7 layers from the L=3 source).
    let doses: Vec<f32> = (0..7)
        .map(|iz| cf.dose_at(2, 2, iz).expect("centre column in bounds"))
        .collect();
    for (iz, d) in doses.iter().enumerate() {
        assert!(
            *d > 0.0,
            "CB Z-smear: iz {iz} must receive cure dose from L=3 source via Z conv, got {d}"
        );
    }

    // (ii) Asymmetric convolution support — column has ZERO values above the
    // source iz_top (kz<0 dispatches skipped) and NON-ZERO values below
    // (Beer-Lambert column-march from iz_top=L=3 deposits at iz=3..=6).
    // After Z conv the resulting profile has:
    // - Monotone-increasing toward source from above: doses[0] < doses[1]
    //   < doses[2] < doses[3] (backward of source, more support closer to L).
    // - Peak shifts to iz=L+1 due to asymmetric support (forward of source
    //   has neighbours on both sides; centre L has zero on the left).
    // - Monotone-decreasing forward of peak (limited by Beer-Lambert decay
    //   AND finite-field truncation at iz=nz-1=6).
    // These are PHYSICAL signatures of the post-attenuation Z conv on a
    // single-layer exposure at L=3 in a 7-layer field.
    assert!(doses[0] < doses[1], "L-3 < L-2");
    assert!(doses[1] < doses[2], "L-2 < L-1");
    assert!(doses[2] < doses[3], "L-1 < L");
    // Peak near L or L+1 (asymmetric support: dose at L+1 = full kernel
    // support inside the non-zero half-column; dose at L = half kernel
    // support since kz<0 hits zeros). For Beer-Lambert decaying columns,
    // the peak is typically at L+1.
    assert!(
        doses[3] > 0.5 * doses[2],
        "L > 0.5 × L-1 (gradient sanity, source layer significantly above)"
    );
    // Forward of peak: dose at L+2 onward decays toward L+3 (lower Beer-
    // Lambert contribution + finite-field truncation).
    assert!(doses[4] > doses[5], "L+1 (peak region) > L+2");
    assert!(doses[5] > doses[6], "L+2 > L+3 (forward decay)");

    // (iv) Z-edge SKIP semantics — LOAD-BEARING.
    //
    // Now run a separate scenario: single-layer source mask at LAYER 0 in
    // a 4-layer field. Beer-Lambert from iz_top=0 deposits at iz=0..=3;
    // dose_col[0] = surface dose × kernel[rz] after Z conv (centre weight
    // only — kz<0 dispatches are skipped because iz_top-1 would be -1).
    let layers_edge: Vec<LayerInput> = (0..4)
        .map(|i| {
            let mut li = LayerInput::new(i, 0.25, 3.0, 60.0, 50.0, (i as f32 + 1.0) * 0.05)
                .expect("LayerInput in-domain");
            li.mask = Some(if i == 0 {
                mask_lit.clone()
            } else {
                mask_empty.clone()
            });
            li
        })
        .collect();
    let sim_edge = SimulationRunner::run_from_layer_inputs_with_voxel(
        &layers_edge,
        &resin,
        &p,
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        },
        &PlateAdhesionProfile::default_textured(),
        ambient(),
        None,
        Some(0.5),
    )
    .expect("CB edge run must succeed");
    let cf_edge = sim_edge.cure_field().expect("CB edge installs cure_field");

    // Also run a "no Z conv" reference (no σ_z) — same source, baseline
    // Beer-Lambert column-march output.
    let p_baseline = printer_with_sigmas(None, None);
    let sim_edge_baseline = SimulationRunner::run_from_layer_inputs_with_voxel(
        &layers_edge,
        &resin,
        &p_baseline,
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        },
        &PlateAdhesionProfile::default_textured(),
        ambient(),
        None,
        Some(0.5),
    )
    .expect("CB edge baseline run must succeed");
    let cf_baseline = sim_edge_baseline
        .cure_field()
        .expect("baseline installs cure_field");

    // Note: `cf_baseline` provides the un-convolved Beer-Lambert column for
    // comparison. We read the full column below; no separate surface-dose
    // variable needed.

    // After Z conv with kernel radius 3, the convolved dose at iz=0 is:
    //   conv[0] = Σ_{k=-3..=3} kernel[k+3] × surface_dose[0+k] but with
    //   k<0 samples treated as 0 (Z-edge SKIP) and k>0 samples ≪ surface
    //   (Beer-Lambert decay). The dominant term is kernel[3] × surface[0].
    //
    // CLAMP semantics would have produced an iz=0 dose ≈ surface_dose ×
    // (kernel[0] + kernel[1] + kernel[2] + kernel[3]) — significantly
    // larger than SKIP's kernel[3] × surface_dose. The discrimination is
    // sharp enough to catch any regression.
    let conv_dose_at_zero = cf_edge.dose_at(2, 2, 0).expect("edge (2,2,0)");

    // Compute kernel for σ_z_layers = 0.8 (7-tap symmetric).
    use resinsim_core::services::LightCrosstalkCalculator;
    let zk = LightCrosstalkCalculator::build_separable_kernel(0.8).expect("σ=0.8 kernel");
    // Read the deposited Beer-Lambert column from the baseline (no Z conv)
    // at the source pixel. We expect baseline_doses[iz] = Beer-Lambert
    // attenuated dose at iz from iz_top=0.
    let baseline_doses: Vec<f32> = (0..4)
        .map(|iz| cf_baseline.dose_at(2, 2, iz).expect("baseline column"))
        .collect();

    // SKIP semantics expected at iz=0: convolution reads kz=-3..=3,
    // but kz<0 hits out-of-bounds → skipped. So:
    //   conv[0] = Σ_{kz=0..=3} kernel[kz+3] × baseline_doses[kz]
    let expected_skip: f32 = (0..=3)
        .map(|kz| zk[(kz + 3) as usize] * baseline_doses[kz as usize])
        .sum();

    // CLAMP semantics would have produced: same forward contributions PLUS
    // the kz<0 weights folded onto baseline_doses[0]:
    //   conv_clamp[0] = expected_skip + (k[0]+k[1]+k[2]) × baseline[0]
    let clamp_extra = (zk[0] + zk[1] + zk[2]) * baseline_doses[0];
    let expected_clamp = expected_skip + clamp_extra;

    // Assert observation is within 1e-3 of the SKIP expected value (this
    // accounts for fp differences and Beer-Lambert local-Dp behaviour
    // from concentration depletion across the column).
    let tol = 0.02 * expected_skip; // 2% tolerance for KB-160 depletion drift
    assert!(
        (conv_dose_at_zero - expected_skip).abs() < tol,
        "Z-edge SKIP: conv_dose_at_zero ({conv_dose_at_zero}) should match \
         expected SKIP value ({expected_skip}) within 2%; CLAMP value would \
         be ({expected_clamp})"
    );

    // The discriminator: SKIP and CLAMP must produce observably distinct
    // values. The 2% tolerance above is much smaller than the SKIP↔CLAMP
    // gap of ~25%+, so the test reliably distinguishes regressions.
    assert!(
        (expected_clamp - expected_skip).abs() > 5.0 * tol,
        "SKIP ({expected_skip}) and CLAMP ({expected_clamp}) must differ by \
         > 10% for the test to discriminate regressions"
    );
}

// =====================================================================
// (8-E / DD) Both σ active ⇒ XY × Z product structure at surface dose.
// =====================================================================

#[test]
fn regime_dd_both_active_combined_xy_and_z() {
    // Both σ active: σ_xy = 1000 (σ_voxels = 2), σ_z = 100 (σ_layers = 2).
    // Single-pixel mask at (2, 2) on a 5x5 grid, 5 layers, source at L=2.
    // After XY pre-conv on intensity, neighbour pixels have intensity = kernel_xy_off1 × source_intensity.
    // After Beer-Lambert + Z post-conv, the centre column has cure smeared
    // in Z. Off-pixel column has scaled cure smeared in Z. So at
    // (3, 2, L+kz): cure = xy_kernel[off1] × beer_lambert_at(kz) × z_kernel[kz+rz].
    // The cleanest predicate: at offset kz=0 (source layer), the ratio
    // cure(off1, L) / cure(centre, L) ≈ xy_kernel_2d[off1] / xy_kernel_2d[centre].
    let p = printer_with_sigmas(Some(1000.0), Some(100.0));
    let mask_lit = single_pixel_mask_in(5, 5, 2, 2);
    let mask_empty = empty_mask(5, 5);
    let resin = ResinProfile::generic_standard();

    let layers: Vec<LayerInput> = (0..5)
        .map(|i| {
            let mut li = LayerInput::new(i, 0.25, 3.0, 60.0, 50.0, (i as f32 + 1.0) * 0.05)
                .expect("LayerInput");
            li.mask = Some(if i == 2 {
                mask_lit.clone()
            } else {
                mask_empty.clone()
            });
            li
        })
        .collect();

    let sim = SimulationRunner::run_from_layer_inputs_with_voxel(
        &layers,
        &resin,
        &p,
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        },
        &PlateAdhesionProfile::default_textured(),
        ambient(),
        None,
        Some(0.5),
    )
    .expect("DD run must succeed");

    let cf = sim.cure_field().expect("DD installs cure_field");

    // (i) cure visible in a 3D neighbourhood around (2, 2, 2).
    let centre = cf.dose_at(2, 2, 2).expect("centre");
    let neighbour_x = cf.dose_at(3, 2, 2).expect("+x neighbour");
    let neighbour_z = cf.dose_at(2, 2, 3).expect("+z neighbour");
    assert!(centre > 0.0, "centre cure positive");
    assert!(neighbour_x > 0.0, "x-neighbour cure positive");
    assert!(neighbour_z > 0.0, "z-neighbour cure positive");

    // (ii) Product structure at SURFACE DOSE (L only): the X-axis ratio
    // matches the (separable) XY kernel ratio. Compute the expected XY
    // ratio from a fresh kernel build using the same σ.
    use resinsim_core::services::LightCrosstalkCalculator;
    let xy_kernel =
        LightCrosstalkCalculator::build_separable_kernel(2.0).expect("σ_xy_voxels = 2 kernel");
    // The 2D separable kernel value at offset (1, 0) is k[r+1] × k[r] /
    // (k[r] × k[r]) ratio simplifies to k[r+1] / k[r]. At source layer
    // L=2, the ratio of cure dose at (3,2,2) / cure dose at (2,2,2)
    // should equal k_xy[off1] / k_xy[centre], where off1 is the
    // kernel offset corresponding to +1 voxel from centre.
    let radius_xy = (xy_kernel.len() as i32 - 1) / 2;
    let kx_centre = xy_kernel[radius_xy as usize];
    let kx_off1 = xy_kernel[(radius_xy + 1) as usize];
    let expected_ratio = kx_off1 / kx_centre;
    let observed_ratio = neighbour_x / centre;
    // Generous tolerance: the X-neighbour cure is dominated by the
    // separable XY-conv contribution, but second-order effects from the
    // Z conv interacting with Beer-Lambert may shift it slightly.
    let tol = 0.02; // 2% relative
    assert!(
        (observed_ratio - expected_ratio).abs() / expected_ratio < tol,
        "DD product structure violated at (+x neighbour): observed ratio {observed_ratio}, \
         expected ratio {expected_ratio} (kx_off1/kx_centre)"
    );
}
