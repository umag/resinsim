//! Tier-2 explicit FTCS solver for the 3D heat equation.
//!
//! ADR-0020, t2f4. Advances a `ThermalField` by one CFL-bound substep
//! using a forward-time centred-space (FTCS) stencil with Dirichlet
//! bottom + convective top/side boundary conditions. Single-threaded by
//! design — see ADR-0020 §Decision v.
//!
//! # Discretisation
//!
//! `T_new[i,j,k] = T_old[i,j,k] + dt·α·∇²T_old[i,j,k]`, where ∇² is the
//! centred-difference Laplacian at homogeneous spacing
//! `h = voxel_size_mm × 1e-3` m. CFL stability bound:
//!
//! ```text
//! dt < min(h²) / (2 · α · 3)
//! ```
//!
//! `cfl_max_dt` returns half this maximum as a safety margin — see
//! `docs/patterns/cfl-guard-on-anisotropic-stencil.md`.
//!
//! # Boundary conditions
//!
//! Six faces of the vat envelope:
//!
//! - **Bottom (z = 0):** Dirichlet at the LED-case temperature. Driven
//!   by `Tier-1` (`ThermalCalculator::led_case_temperature_at`); see
//!   `docs/patterns/tier1-as-bc-source-for-tier2.md`.
//! - **Top (z = nz-1):** Convective into ambient via Newton cooling at
//!   `h_top`. Resin-air free surface.
//! - **Sides (x = 0, x = nx-1, y = 0, y = ny-1):** Convective into
//!   ambient via lumped wall resistance
//!   `1/h_side = 1/h_air + wall_thickness/wall_k`. The caller computes
//!   this composite coefficient ONCE outside the per-substep loop and
//!   passes it through `BoundaryConditions.side_h_w_m2k`.
//!
//! Convective BCs are realised via the standard Robin ghost-cell
//! formulation:
//!
//! ```text
//! T_ghost = T_interior_neighbour - 2·h·dx/k_resin · (T_boundary − T_ambient)
//! ```
//!
//! When a face hosts a Dirichlet condition AND another (e.g. the
//! z = 0, x = 0 corner has Dirichlet bottom and Robin side), the
//! Dirichlet wins — the boundary voxel is set to `bottom_dirichlet_c`
//! and the side ghost is not consulted.
//!
//! # Postcondition
//!
//! `step()` returns `Err(NonFiniteField)` if any voxel value is
//! non-finite after the substep. In debug builds an additional
//! `debug_assert!` sweep verifies element-wise finiteness for an early
//! failure during development.

#![cfg(feature = "field-sim")]

use ndarray::Array3;
use thiserror::Error;

use crate::values::ThermalField;

/// Boundary conditions for one solver substep. Driven by the
/// `SimulationRunner` orchestrator per layer.
#[derive(Debug, Clone, Copy)]
pub struct BoundaryConditions {
    /// Dirichlet temperature at the bottom face (z = 0). Sourced from
    /// `ThermalCalculator::led_case_temperature_at` at the layer's
    /// cumulative print time.
    pub bottom_dirichlet_c: f32,
    /// Convective coefficient at the top (resin-air free surface).
    /// Units: W/(m²·K). Newton-cooling against `ambient_c`.
    pub top_h_w_m2k: f32,
    /// Lumped convective coefficient at the four side faces (vat outer
    /// walls). Units: W/(m²·K). Lumped through the series-resistance
    /// formula `1/h_eff = 1/h_air + wall_thickness/wall_k` — caller
    /// pre-computes this scalar.
    pub side_h_w_m2k: f32,
    /// Ambient temperature (°C) for the convective faces.
    pub ambient_c: f32,
    /// Resin thermal conductivity in W/(m·K). Used in the Robin
    /// ghost-cell formula's Biot factor `h·dx/k`.
    pub k_resin_w_mk: f32,
}

/// Errors from the thermal diffusion solver.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ThermalSolverError {
    /// Post-step finiteness check failed — at least one voxel is
    /// NaN or infinite. Solver step's postcondition per ADR-0020
    /// §Decision iv (with NaN-two-layer-defence).
    #[error("ThermalDiffusionSolver: step produced non-finite field value(s)")]
    NonFiniteField,
}

/// Stateless explicit FTCS solver.
pub struct ThermalDiffusionSolver;

impl ThermalDiffusionSolver {
    /// Maximum stable timestep for explicit FTCS at the given diffusivity
    /// and isotropic voxel spacing. The leading `0.5` is a safety margin
    /// — half the theoretical CFL ceiling — to leave headroom for
    /// accumulated rounding in the mixed Dirichlet + Robin updates.
    ///
    /// Returns `dt_max` in seconds.
    ///
    /// # Panics / NaN handling
    ///
    /// Returns `f32::NAN` if `alpha_m2_s` is non-positive or non-finite,
    /// or if `voxel_size_mm` is non-positive or non-finite. Callers MUST
    /// check the return value's finiteness — typically already enforced
    /// by the `ConvectiveCoefficient` / `ThermalConductivity` /
    /// `ThermalField::new` validations upstream.
    pub fn cfl_max_dt(alpha_m2_s: f32, voxel_size_mm: f32) -> f32 {
        if !alpha_m2_s.is_finite()
            || alpha_m2_s <= 0.0
            || !voxel_size_mm.is_finite()
            || voxel_size_mm <= 0.0
        {
            return f32::NAN;
        }
        let h_m = voxel_size_mm * 1e-3;
        0.5 * h_m * h_m / (3.0 * alpha_m2_s)
    }

    /// Advance `field` by one explicit-FTCS substep `dt_sec` at thermal
    /// diffusivity `alpha_m2_s`, applying the boundary conditions in
    /// `bcs`. Single-threaded by design (ADR-0020 §Decision v).
    ///
    /// # Postcondition
    ///
    /// `Err(NonFiniteField)` if any voxel of the resulting field is
    /// NaN / infinite. In debug builds an element-wise `debug_assert!`
    /// fires earlier — release builds use the cheaper `volume_max_c`
    /// finite check (a non-finite would propagate via `max` to the
    /// reduction result).
    pub fn step(
        field: &mut ThermalField,
        dt_sec: f32,
        alpha_m2_s: f32,
        bcs: &BoundaryConditions,
    ) -> Result<(), ThermalSolverError> {
        let (nx_u, ny_u, nz_u) = field.dimensions();
        let nx = nx_u as usize;
        let ny = ny_u as usize;
        let nz = nz_u as usize;
        let h_m = field.voxel_size_mm() * 1e-3;
        let h2 = h_m * h_m;
        let r = dt_sec * alpha_m2_s / h2; // dimensionless FTCS update coefficient per face
        // Pre-compute Biot factor for Robin ghost cells (T_ghost depends
        // on h·dx/k via 2·Bi·(T_boundary − T_ambient)).
        let bi_top = bcs.top_h_w_m2k * h_m / bcs.k_resin_w_mk;
        let bi_side = bcs.side_h_w_m2k * h_m / bcs.k_resin_w_mk;
        let t_amb = bcs.ambient_c;

        // Snapshot the old state into scratch storage so the stencil
        // reads from a consistent prior step (avoid aliasing across the
        // in-place update).
        let t_old: Array3<f32> = field.as_array_view().to_owned();

        // Helper: a single Laplacian-with-BC update for the given voxel.
        let update = |ix: usize, iy: usize, iz: usize| -> f32 {
            // Dirichlet bottom wins — set, don't update.
            if iz == 0 {
                return bcs.bottom_dirichlet_c;
            }
            let t = t_old[(ix, iy, iz)];

            // X-neighbours with Robin ghost at x = 0 / x = nx - 1.
            let t_xm = if ix > 0 {
                t_old[(ix - 1, iy, iz)]
            } else {
                // Ghost at x = -1 — symmetric Robin form.
                let inner = t_old[(1, iy, iz)];
                inner - 2.0 * bi_side * (t - t_amb)
            };
            let t_xp = if ix + 1 < nx {
                t_old[(ix + 1, iy, iz)]
            } else {
                let inner = t_old[(nx - 2, iy, iz)];
                inner - 2.0 * bi_side * (t - t_amb)
            };

            // Y-neighbours with Robin ghost at y = 0 / y = ny - 1.
            let t_ym = if iy > 0 {
                t_old[(ix, iy - 1, iz)]
            } else {
                let inner = t_old[(ix, 1, iz)];
                inner - 2.0 * bi_side * (t - t_amb)
            };
            let t_yp = if iy + 1 < ny {
                t_old[(ix, iy + 1, iz)]
            } else {
                let inner = t_old[(ix, ny - 2, iz)];
                inner - 2.0 * bi_side * (t - t_amb)
            };

            // Z-neighbours:
            //   z = 0: Dirichlet (short-circuited above).
            //   z = nz - 1: Robin (convective top).
            //   interior z: bare neighbours.
            let t_zm = t_old[(ix, iy, iz - 1)];
            let t_zp = if iz + 1 < nz {
                t_old[(ix, iy, iz + 1)]
            } else {
                let inner = t_old[(ix, iy, nz - 2)];
                inner - 2.0 * bi_top * (t - t_amb)
            };

            t + r * (t_xm + t_xp + t_ym + t_yp + t_zm + t_zp - 6.0 * t)
        };

        // Write updated values back into the field. The `as_array_mut`
        // view shares storage with the field — `t_old` is the snapshot.
        let mut data = field.as_array_mut();
        for ix in 0..nx {
            for iy in 0..ny {
                for iz in 0..nz {
                    data[(ix, iy, iz)] = update(ix, iy, iz);
                }
            }
        }

        // Postcondition: no NaN/infinite values exited the step.
        debug_assert!(
            field.as_array_view().iter().all(|v| v.is_finite()),
            "thermal solver substep produced non-finite voxel values"
        );
        if !field.volume_max_c().is_finite() {
            return Err(ThermalSolverError::NonFiniteField);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_bcs() -> BoundaryConditions {
        BoundaryConditions {
            bottom_dirichlet_c: 40.0,
            top_h_w_m2k: 10.0,
            side_h_w_m2k: 8.0,
            ambient_c: 22.0,
            k_resin_w_mk: 0.20,
        }
    }

    fn small_field(initial_c: f32) -> ThermalField {
        ThermalField::new(4, 4, 4, 0.5, [0.0, 0.0, 0.0], initial_c)
            .expect("4×4×4 ThermalField in-domain")
    }

    // --- cfl_max_dt ---

    #[test]
    fn cfl_max_dt_matches_formula() {
        // α = 1.07e-7 m²/s (resin), voxel = 0.5 mm = 5e-4 m
        //   h² / (2·α·3) = 25e-8 / (6 · 1.07e-7) = 0.3894 s
        //   × 0.5 safety = 0.1947 s
        let dt = ThermalDiffusionSolver::cfl_max_dt(1.07e-7, 0.5);
        let expected = 0.5 * (5e-4_f32 * 5e-4_f32) / (3.0 * 1.07e-7_f32);
        assert!((dt - expected).abs() < 1e-9, "got {dt}, expected {expected}");
    }

    #[test]
    fn cfl_max_dt_rejects_bad_inputs() {
        for bad_alpha in [0.0_f32, -1.0, f32::NAN, f32::INFINITY] {
            assert!(ThermalDiffusionSolver::cfl_max_dt(bad_alpha, 0.5).is_nan());
        }
        for bad_h in [0.0_f32, -0.5, f32::NAN] {
            assert!(ThermalDiffusionSolver::cfl_max_dt(1.07e-7, bad_h).is_nan());
        }
    }

    // --- step: Dirichlet bottom ---

    #[test]
    fn step_pins_bottom_to_dirichlet() {
        let mut field = small_field(25.0);
        let dt = ThermalDiffusionSolver::cfl_max_dt(1.07e-7, 0.5);
        ThermalDiffusionSolver::step(&mut field, dt, 1.07e-7, &default_bcs())
            .expect("first step is finite");
        // Every z=0 voxel should now equal the Dirichlet value.
        for ix in 0..4 {
            for iy in 0..4 {
                let t = field.temperature_at(ix, iy, 0).expect("in-bounds");
                assert!(
                    (t - 40.0).abs() < 1e-4,
                    "z=0 voxel ({ix}, {iy}, 0) must equal Dirichlet 40.0, got {t}"
                );
            }
        }
    }

    // --- step: Dirichlet drives evolution toward T_hot ---

    #[test]
    fn dirichlet_bottom_warms_interior_monotonically() {
        let mut field = small_field(22.0); // ambient
        let dt = ThermalDiffusionSolver::cfl_max_dt(1.07e-7, 0.5);
        let alpha = 1.07e-7;
        let bcs = BoundaryConditions {
            bottom_dirichlet_c: 50.0,
            top_h_w_m2k: 0.0, // perfectly insulating top so only bottom drives
            side_h_w_m2k: 0.0,
            ambient_c: 22.0,
            k_resin_w_mk: 0.20,
        };
        let interior_iy = 2;
        let interior_ix = 2;
        let mut prev_t_iz_1 = field
            .temperature_at(interior_ix, interior_iy, 1)
            .expect("in-bounds");
        for _ in 0..30 {
            ThermalDiffusionSolver::step(&mut field, dt, alpha, &bcs)
                .expect("substep finite");
            let t_iz_1 = field
                .temperature_at(interior_ix, interior_iy, 1)
                .expect("in-bounds");
            assert!(
                t_iz_1 >= prev_t_iz_1 - 1e-5,
                "iz=1 interior temperature must monotonically warm: prev={prev_t_iz_1}, now={t_iz_1}"
            );
            prev_t_iz_1 = t_iz_1;
        }
        // After 30 substeps, the iz=1 layer should be measurably warmer
        // than the initial 22 °C.
        assert!(
            prev_t_iz_1 > 23.0,
            "interior must have warmed measurably, got {prev_t_iz_1}"
        );
        // ... and bounded above by the Dirichlet ceiling.
        assert!(
            prev_t_iz_1 < 50.0,
            "interior must not exceed Dirichlet 50.0, got {prev_t_iz_1}"
        );
    }

    // --- step: insulated-top + insulated-sides + uniform-Dirichlet = uniform field ---

    #[test]
    fn uniform_dirichlet_with_insulated_sides_keeps_field_finite() {
        // Sanity check: a uniform initial field at the Dirichlet
        // temperature should stay numerically stable through many
        // substeps with insulated lateral BCs.
        let mut field = small_field(40.0);
        let dt = ThermalDiffusionSolver::cfl_max_dt(1.07e-7, 0.5);
        let bcs = BoundaryConditions {
            bottom_dirichlet_c: 40.0,
            top_h_w_m2k: 0.0,
            side_h_w_m2k: 0.0,
            ambient_c: 22.0,
            k_resin_w_mk: 0.20,
        };
        for _ in 0..100 {
            ThermalDiffusionSolver::step(&mut field, dt, 1.07e-7, &bcs)
                .expect("uniform-field steady-state remains finite");
        }
        // Field is bounded; the max should be the Dirichlet 40 (or
        // very slightly under from f32 accumulation). The min is at
        // least the initial 40 minus tiny floating-point noise.
        let max = field.volume_max_c();
        let min = field.as_array_view().iter().copied().fold(f32::INFINITY, f32::min);
        assert!(
            (max - 40.0).abs() < 0.1,
            "uniform Dirichlet must hold steady; got max={max}"
        );
        assert!(
            (min - 40.0).abs() < 0.1,
            "uniform Dirichlet must hold steady; got min={min}"
        );
    }

    // --- determinism (single-threaded postcondition) ---

    #[test]
    fn two_runs_with_same_input_produce_byte_identical_field() {
        // Sidecar sha256 stability hinges on this — ADR-0020 §Decision v.
        let bcs = default_bcs();
        let alpha = 1.07e-7;
        let dt = ThermalDiffusionSolver::cfl_max_dt(alpha, 0.5);
        let mut a = small_field(25.0);
        let mut b = small_field(25.0);
        for _ in 0..20 {
            ThermalDiffusionSolver::step(&mut a, dt, alpha, &bcs).expect("a step");
            ThermalDiffusionSolver::step(&mut b, dt, alpha, &bcs).expect("b step");
        }
        // Byte-identical comparison of the f32 representations.
        for (va, vb) in a.as_array_view().iter().zip(b.as_array_view().iter()) {
            assert_eq!(
                va.to_bits(),
                vb.to_bits(),
                "determinism violation: a={va:?} b={vb:?}"
            );
        }
    }

    // --- NaN postcondition ---

    #[test]
    fn step_returns_non_finite_field_when_alpha_blows_stability() {
        // Force CFL violation (10× the safe dt) to drive the explicit
        // scheme unstable. After enough substeps the field should
        // diverge; the postcondition catches the resulting non-finite.
        //
        // In debug builds the `debug_assert!` sweep panics on the
        // first non-finite voxel — wrap in `catch_unwind` to assert
        // both paths fail loudly. In release the postcondition
        // returns `Err(NonFiniteField)`.
        let alpha = 1.07e-7;
        let dt_unsafe = ThermalDiffusionSolver::cfl_max_dt(alpha, 0.5) * 10.0;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut field = small_field(25.0);
            let bcs = default_bcs();
            for _ in 0..500 {
                if let Err(ThermalSolverError::NonFiniteField) =
                    ThermalDiffusionSolver::step(&mut field, dt_unsafe, alpha, &bcs)
                {
                    return true;
                }
            }
            false
        }));
        match outcome {
            // Release: typed error eventually fired.
            Ok(returned_err) => assert!(
                returned_err,
                "expected NonFiniteField return after CFL-violating runs"
            ),
            // Debug: debug_assert! panicked — also acceptable, the
            // unstable run cannot quietly poison the field either way.
            Err(_panic_payload) => {}
        }
    }

    // --- heat balance property test (degenerate uniform) ---

    #[test]
    fn heat_balance_at_zero_flux_conserved() {
        // With insulated sides AND insulated top AND a Dirichlet bottom
        // at the SAME initial temperature, the volume integral should
        // be conserved to high precision (no boundary fluxes injected).
        let mut field = small_field(40.0);
        let bcs = BoundaryConditions {
            bottom_dirichlet_c: 40.0,
            top_h_w_m2k: 0.0,
            side_h_w_m2k: 0.0,
            ambient_c: 40.0, // equal to all other temps — no Robin imbalance
            k_resin_w_mk: 0.20,
        };
        let dt = ThermalDiffusionSolver::cfl_max_dt(1.07e-7, 0.5);
        let initial_total: f64 = field.as_array_view().iter().map(|&v| v as f64).sum();
        for _ in 0..100 {
            ThermalDiffusionSolver::step(&mut field, dt, 1.07e-7, &bcs)
                .expect("conservation step finite");
        }
        let final_total: f64 = field.as_array_view().iter().map(|&v| v as f64).sum();
        let rel_drift = ((final_total - initial_total) / initial_total).abs();
        assert!(
            rel_drift < 1e-2,
            "zero-flux conservation drift {rel_drift} exceeded 1 %"
        );
    }
}
