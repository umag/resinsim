// One-off helper: parse a CTB, synthesise a PrintSimulation with one
// LayerResult per CTB layer, and write it to a JSON file. Cure-depth
// values gradient from 100 µm (bottom) to 100 + N µm (top) so the
// viridis ramp produces a visible blue→yellow gradient up the print.
//
// Usage:
//   cargo run -p resinsim-viz --example synth_sim_from_ctb -- \
//     <input.ctb> <output.sim.json>
//
// NOT a permanent feature — used only to drive the heatmap viz against
// real CTB fixtures during code review. Safe to delete after use.

use std::path::{Path, PathBuf};

use resinsim_core::entities::{LayerResult, PrinterProfile, ResinProfile};
use resinsim_core::io::ctb;
use resinsim_core::simulation::PrintSimulation;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <input.ctb> <output.sim.json>", args[0]);
        std::process::exit(2);
    }
    let ctb_path = PathBuf::from(&args[1]);
    let out_path = PathBuf::from(&args[2]);

    let (_info, layers) = ctb::parse_ctb(Path::new(&ctb_path))
        .unwrap_or_else(|e| panic!("CTB parse failed for {}: {e}", ctb_path.display()));

    let recipe = ResinProfile::generic_standard().recipe().clone();
    let printer = PrinterProfile::generic_msla_4k();
    let mut sim = PrintSimulation::new(recipe, printer);
    let n = layers.len() as f32;
    for (i, _) in layers.iter().enumerate() {
        let t = i as f32 / n.max(1.0);
        let cure_depth_um = 80.0 + 220.0 * t; // 80 → 300 µm gradient
        let lr = LayerResult {
            index: i as u32,
            cure_depth_um,
            peel_force_n: 0.0,
            suction_force_n: 0.0,
            total_force_n: 0.0,
            support_capacity_n: 0.0,
            safety_factor: 1.0,
            cross_section_area_mm2: 1.0,
            area_delta_mm2: 0.0,
            vat_temperature_c: 22.0,
            viscosity_mpa_s: 200.0,
            z_deflection_um: 0.0,
            effective_layer_height_um: 50.0,
            worst_cure_depth_um: cure_depth_um,
            strain_magnitude_max: None,
            stress_von_mises_max_mpa: None,
            strain_gradient_max_frac: None,
            voxel_yield_fraction: None,
        };
        sim.add_layer(lr, vec![])
            .unwrap_or_else(|e| panic!("add_layer({i}): {e:?}"));
    }
    resinsim_core::repositories::save_to_path(&out_path, &sim)
        .unwrap_or_else(|e| panic!("save_to_path({}) failed: {e}", out_path.display()));
    println!(
        "wrote {} layers ({:.1}–{:.1} µm cure_depth) to {}",
        layers.len(),
        80.0,
        80.0 + 220.0,
        out_path.display()
    );
}
