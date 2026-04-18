use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "resinsim", about = "Resin 3D printer physics simulation")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Inspect simulation domains
    Inspect {
        #[command(subcommand)]
        domain: InspectDomain,
    },
    /// Generate simulation reports
    Report {
        #[command(subcommand)]
        report_type: ReportType,
    },
}

#[derive(Subcommand)]
enum ReportType {
    /// Full print risk assessment from an STL file
    Health {
        /// Path to STL file
        #[arg(long)]
        stl: Option<String>,
        /// Path to sliced file (CTB, auto-detected)
        #[arg(long)]
        file: Option<String>,
        /// Resin name (generic_standard, elegoo_ceramic_grey_v2)
        #[arg(long, default_value = "generic_standard")]
        resin: String,
        /// Printer name (generic_msla_4k, elegoo_mars5_ultra)
        #[arg(long, default_value = "generic_msla_4k")]
        printer: String,
        /// Support tip radius in mm
        #[arg(long, default_value_t = 0.2)]
        tip_radius: f32,
        /// Number of supports
        #[arg(long, default_value_t = 20)]
        n_supports: u32,
        /// Ambient temperature in °C
        #[arg(long, default_value_t = 22.0)]
        ambient: f32,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum InspectDomain {
    /// Compute cure depth from Beer-Lambert equation
    Cure {
        /// Penetration depth in µm
        #[arg(long)]
        dp: f32,
        /// Critical energy in mJ/cm²
        #[arg(long)]
        ec: f32,
        /// Energy dose in mJ/cm²
        #[arg(long)]
        energy: f32,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Compute peel force for a given cross-section
    Force {
        /// Cross-section area in mm²
        #[arg(long)]
        area: f64,
        /// Peel adhesion in kPa (default: 13.0 for standard FEP)
        #[arg(long, default_value_t = 13.0)]
        sigma: f32,
        /// Lift speed in mm/min
        #[arg(long, default_value_t = 60.0)]
        speed: f32,
        /// Reference speed for sigma measurement in mm/min
        #[arg(long, default_value_t = 60.0)]
        ref_speed: f32,
        /// Sealed cavity area in mm² (suction)
        #[arg(long, default_value_t = 0.0)]
        sealed_area: f64,
        /// Support tip radius in mm
        #[arg(long)]
        tip_radius: Option<f32>,
        /// Number of supports
        #[arg(long)]
        n_supports: Option<u32>,
        /// Resin tensile strength in MPa
        #[arg(long, default_value_t = 35.0)]
        tensile: f32,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Compute vat temperature and viscosity drift over a print
    Thermal {
        /// Total number of layers
        #[arg(long)]
        layers: u32,
        /// Exposure time per layer in seconds
        #[arg(long, default_value_t = 2.5)]
        exposure: f32,
        /// Lift cycle time in seconds (non-exposure portion)
        #[arg(long, default_value_t = 7.5)]
        lift_cycle: f32,
        /// Ambient temperature in °C
        #[arg(long, default_value_t = 22.0)]
        ambient: f32,
        /// Steady-state temperature rise in °C
        #[arg(long, default_value_t = 10.0)]
        delta_t: f32,
        /// Thermal time constant in seconds
        #[arg(long, default_value_t = 1200.0)]
        tau: f32,
        /// Reference viscosity in mPa·s
        #[arg(long, default_value_t = 200.0)]
        viscosity: f32,
        /// Arrhenius activation energy in kJ/mol
        #[arg(long, default_value_t = 52.0)]
        ea: f32,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Compute Z-axis deflection and effective layer height
    Zaxis {
        /// Peel force in Newtons
        #[arg(long)]
        force: f32,
        /// Z-axis stiffness in N/mm (default: 460 for Elegoo Mars class)
        #[arg(long, default_value_t = 460.0)]
        stiffness: f32,
        /// Commanded layer height in µm
        #[arg(long, default_value_t = 50.0)]
        layer_height: f32,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Query and analyze Athena II force sensor CSV data
    Athena {
        /// Path to force CSV file
        #[arg(long)]
        file: String,
        /// Start layer (inclusive)
        #[arg(long)]
        from: Option<u32>,
        /// End layer (inclusive)
        #[arg(long)]
        to: Option<u32>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show per-layer data from a sliced file (CTB)
    Layers {
        /// Path to sliced file
        #[arg(long)]
        file: String,
        /// Start layer (inclusive)
        #[arg(long)]
        from: Option<u32>,
        /// End layer (inclusive)
        #[arg(long)]
        to: Option<u32>,
        /// Show summary statistics only
        #[arg(long)]
        stats: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Inspect { domain } => match domain {
            InspectDomain::Cure { dp, ec, energy, json } => cmd_cure(dp, ec, energy, json),
            InspectDomain::Force {
                area, sigma, speed, ref_speed, sealed_area,
                tip_radius, n_supports, tensile, json,
            } => cmd_force(area, sigma, speed, ref_speed, sealed_area, tip_radius, n_supports, tensile, json),
            InspectDomain::Thermal {
                layers, exposure, lift_cycle, ambient,
                delta_t, tau, viscosity, ea, json,
            } => cmd_thermal(layers, exposure, lift_cycle, ambient, delta_t, tau, viscosity, ea, json),
            InspectDomain::Zaxis { force, stiffness, layer_height, json } => {
                cmd_zaxis(force, stiffness, layer_height, json)
            }
            InspectDomain::Athena { file, from, to, json } => {
                cmd_athena(&file, from, to, json)
            }
            InspectDomain::Layers { file, from, to, stats, json } => {
                cmd_inspect_layers(&file, from, to, stats, json)
            }
        },
        Commands::Report { report_type } => match report_type {
            ReportType::Health { stl, file, resin, printer, tip_radius, n_supports, ambient, json } => {
                cmd_report_health(stl.as_deref(), file.as_deref(), &resin, &printer, tip_radius, n_supports, ambient, json)
            }
        },
    }
}

fn cmd_cure(dp: f32, ec: f32, energy: f32, json: bool) {
    use resinsim_core::services::CureCalculator;
    use resinsim_core::values::{Energy, PenetrationDepth};

    let dp = match PenetrationDepth::new(dp) {
        Ok(v) => v,
        Err(e) => { eprintln!("invalid penetration depth: {e}"); std::process::exit(2); }
    };
    let ec_val = match Energy::new(ec) {
        Ok(v) => v,
        Err(e) => { eprintln!("invalid critical energy: {e}"); std::process::exit(2); }
    };
    let e = match Energy::new(energy) {
        Ok(v) => v,
        Err(err) => { eprintln!("invalid energy: {err}"); std::process::exit(2); }
    };
    let cd = CureCalculator::cure_depth(dp, e, ec_val);

    if json {
        let result = serde_json::json!({
            "cure_depth_um": cd.value(),
            "penetration_depth_um": dp.value(),
            "critical_energy_mj_cm2": ec,
            "energy_mj_cm2": energy,
            "sufficient_for_50um": cd.is_sufficient(50.0),
        });
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
    } else {
        println!("Cure depth: {cd}");
        println!("  Dp = {dp}, Ec = {ec_val}, E = {e}");
        if cd.is_sufficient(50.0) {
            println!("  Sufficient for 50 µm layers");
        } else {
            println!("  INSUFFICIENT for 50 µm layers");
        }
    }
}

fn cmd_force(
    area: f64, sigma: f32, speed: f32, ref_speed: f32, sealed_area: f64,
    tip_radius: Option<f32>, n_supports: Option<u32>, tensile: f32, json: bool,
) {
    use resinsim_core::services::PeelForceCalculator;
    use resinsim_core::values::{CrossSectionArea, SafetyFactor};

    let speed_factor = PeelForceCalculator::lift_speed_factor(speed, ref_speed);
    let area_val = match CrossSectionArea::new(area) {
        Ok(v) => v,
        Err(e) => { eprintln!("invalid area: {e}"); std::process::exit(2); }
    };
    let sealed_area_val = match CrossSectionArea::new(sealed_area) {
        Ok(v) => v,
        Err(e) => { eprintln!("invalid sealed area: {e}"); std::process::exit(2); }
    };
    let peel = PeelForceCalculator::peel_force(sigma, area_val, speed_factor);
    let suction = PeelForceCalculator::suction_force(
        if sealed_area > 0.0 { 50.0 } else { 0.0 },
        sealed_area_val,
    );
    let total = PeelForceCalculator::total_force(peel, suction);

    let (capacity, safety) = match (tip_radius, n_supports) {
        (Some(r), Some(n)) => {
            let cap = PeelForceCalculator::support_capacity(tensile, r, n);
            let sf = SafetyFactor::compute(cap, total);
            (Some(cap), sf)
        }
        _ => (None, None),
    };

    if json {
        let mut result = serde_json::json!({
            "area_mm2": area,
            "sigma_kpa": sigma,
            "speed_factor": speed_factor,
            "peel_force_n": peel.value(),
            "suction_force_n": suction.value(),
            "total_force_n": total.value(),
        });
        if let Some(cap) = capacity {
            result["support_capacity_n"] = serde_json::json!(cap.value());
        }
        if let Some(sf) = safety {
            result["safety_factor"] = serde_json::json!(sf.value());
            result["safe"] = serde_json::json!(sf.is_safe());
        }
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
    } else {
        println!("Peel force: {peel} (adhesion) + {} (suction) = {total} (total)", suction);
        println!("  σ = {sigma} kPa, A = {area:.1} mm², f(v) = {speed_factor:.3}");
        if let (Some(cap), Some(sf)) = (capacity, safety) {
            println!("  Support capacity: {cap}");
            println!("  Safety factor: {sf} — {}", if sf.is_safe() { "SAFE" } else { "FAIL" });
        }
    }
}

fn cmd_thermal(
    layers: u32, exposure: f32, lift_cycle: f32, ambient: f32,
    delta_t: f32, tau: f32, viscosity: f32, ea: f32, json: bool,
) {
    use resinsim_core::entities::ResinProfile;
    use resinsim_core::services::ThermalCalculator;
    use resinsim_core::values::ThermalTimeConstant;

    let tau_val = match ThermalTimeConstant::new(tau) {
        Ok(v) => v,
        Err(e) => { eprintln!("invalid thermal time constant: {e}"); std::process::exit(2); }
    };
    // CLI uses generic-standard resin thresholds for degradation warnings.
    let resin_defaults = ResinProfile::generic_standard();
    let duty = ThermalCalculator::duty_cycle(exposure, lift_cycle);

    // Sample at key layers
    let sample_layers: Vec<u32> = {
        let mut s = vec![0];
        let step = (layers / 10).max(1);
        let mut l = step;
        while l < layers {
            s.push(l);
            l += step;
        }
        s.push(layers);
        s
    };

    if json {
        let points: Vec<serde_json::Value> = sample_layers
            .iter()
            .map(|&l| {
                let t = ThermalCalculator::vat_temperature_at_layer(
                    ambient, delta_t, tau_val, l, exposure, lift_cycle,
                );
                let mu_ratio = ThermalCalculator::viscosity_ratio(ambient, t.value(), ea);
                let mu = viscosity * mu_ratio;
                serde_json::json!({
                    "layer": l,
                    "time_sec": l as f32 * (exposure + lift_cycle),
                    "temperature_c": t.value(),
                    "viscosity_mpa_s": mu,
                    "viscosity_ratio": mu_ratio,
                    "degradation_risk": resin_defaults.is_degradation_risk(t),
                })
            })
            .collect();
        let result = serde_json::json!({
            "ambient_c": ambient,
            "delta_t_steady_c": delta_t,
            "tau_sec": tau,
            "duty_cycle": duty,
            "ea_kj_mol": ea,
            "points": points,
        });
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
    } else {
        println!("Vat thermal profile ({layers} layers, duty cycle {:.0}%)", duty * 100.0);
        println!("  Ambient: {ambient}°C, ΔT steady: {delta_t}°C, τ: {tau}s");
        println!();
        println!("{:>6}  {:>8}  {:>8}  {:>10}  {:>8}", "Layer", "Time", "Temp", "Viscosity", "µ/µ₀");
        println!("{:>6}  {:>8}  {:>8}  {:>10}  {:>8}", "", "(min)", "(°C)", "(mPa·s)", "");
        println!("{}", "-".repeat(50));

        for &l in &sample_layers {
            let t = ThermalCalculator::vat_temperature_at_layer(
                ambient, delta_t, tau_val, l, exposure, lift_cycle,
            );
            let time_min = l as f32 * (exposure + lift_cycle) / 60.0;
            let mu_ratio = ThermalCalculator::viscosity_ratio(ambient, t.value(), ea);
            let mu = viscosity * mu_ratio;
            let warn = if resin_defaults.is_degradation_risk(t) { " ⚠" } else { "" };
            println!(
                "{:>6}  {:>7.1}  {:>7.1}  {:>9.1}  {:>7.2}{}",
                l, time_min, t.value(), mu, mu_ratio, warn
            );
        }
    }
}

fn cmd_zaxis(force: f32, stiffness: f32, layer_height: f32, json: bool) {
    use resinsim_core::services::ZAxisCompensator;
    use resinsim_core::values::PeelForce;

    let force_val = match PeelForce::new(force) {
        Ok(v) => v,
        Err(e) => { eprintln!("invalid force: {e}"); std::process::exit(2); }
    };
    let dz = ZAxisCompensator::deflection_um(force_val, stiffness);
    let h_eff = ZAxisCompensator::effective_layer_height_um(layer_height, dz);
    let severity = ZAxisCompensator::severity(layer_height, dz);

    if json {
        let result = serde_json::json!({
            "force_n": force,
            "stiffness_n_per_mm": stiffness,
            "commanded_layer_height_um": layer_height,
            "deflection_um": dz,
            "effective_layer_height_um": h_eff,
            "severity": format!("{severity:?}"),
        });
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
    } else {
        println!("Z-axis deflection: {dz:.1} µm");
        println!("  Force: {force} N, k: {stiffness} N/mm");
        println!("  Commanded: {layer_height} µm → Effective: {h_eff:.1} µm");
        println!("  Severity: {severity:?}");
    }
}

fn cmd_athena(file: &str, from: Option<u32>, to: Option<u32>, json: bool) {
    use resinsim_core::io::athena;
    use std::path::Path;

    let records = match athena::load_force_csv(Path::new(file)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };

    let filtered: Vec<&athena::ForceRecord> = match (from, to) {
        (Some(f), Some(t)) => athena::filter_layers(&records, f, t),
        (Some(f), None) => athena::filter_layers(&records, f, u32::MAX),
        (None, Some(t)) => athena::filter_layers(&records, 0, t),
        (None, None) => records.iter().collect(),
    };

    let owned: Vec<athena::ForceRecord> = filtered.into_iter().cloned().collect();
    let stats = athena::force_stats(&owned);

    if json {
        let result = serde_json::json!({
            "file": file,
            "from_layer": from,
            "to_layer": to,
            "stats": {
                "count": stats.count,
                "mean_n": stats.mean_n,
                "max_n": stats.max_n,
                "max_layer": stats.max_layer,
                "min_n": stats.min_n,
                "std_dev_n": stats.std_dev_n,
            },
        });
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
    } else {
        let range = match (from, to) {
            (Some(f), Some(t)) => format!("layers {f}..{t}"),
            (Some(f), None) => format!("layers {f}+"),
            (None, Some(t)) => format!("layers ..{t}"),
            (None, None) => "all layers".into(),
        };
        println!("Athena II force data: {file} ({range})");
        println!("  Records: {}", stats.count);
        println!("  Mean: {:.2} N", stats.mean_n);
        println!("  Max: {:.2} N at layer {}", stats.max_n, stats.max_layer);
        println!("  Min: {:.2} N", stats.min_n);
        println!("  Std dev: {:.2} N", stats.std_dev_n);
    }
}

fn cmd_report_health(
    path: Option<&str>, file_path: Option<&str>, resin_name: &str, printer_name: &str,
    tip_radius: f32, n_supports: u32, ambient: f32, json: bool,
) {
    use resinsim_core::app::SimulationRunner;
    use resinsim_core::entities::{PrinterProfile, ResinProfile, Severity};
    use resinsim_core::services::build_plate::PlateAdhesionProfile;
    use resinsim_core::services::failure_predictor::SupportConfig;
    use std::path::Path;

    let resin = match resin_name {
        "generic_standard" => ResinProfile::generic_standard(),
        "elegoo_ceramic_grey_v2" => ResinProfile::elegoo_ceramic_grey_v2(),
        other => {
            eprintln!("Unknown resin profile: {other}. Using generic_standard.");
            ResinProfile::generic_standard()
        }
    };
    let printer = match printer_name {
        "generic_msla_4k" => PrinterProfile::generic_msla_4k(),
        "elegoo_mars5_ultra" => PrinterProfile::elegoo_mars5_ultra(),
        other => {
            eprintln!("Unknown printer profile: {other}. Using generic_msla_4k.");
            PrinterProfile::generic_msla_4k()
        }
    };
    let supports = SupportConfig { tip_radius_mm: tip_radius, n_supports };
    let plate = PlateAdhesionProfile::default_textured();

    let path = file_path.or(path).unwrap_or_else(|| {
        eprintln!("Error: provide --file or --stl");
        std::process::exit(1);
    });

    let sim = match SimulationRunner::run_auto(Path::new(path), &resin, &printer, &supports, &plate, ambient) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };

    let summary = sim.summary();

    if json {
        let failures: Vec<serde_json::Value> = sim.failures().iter().map(|f| {
            serde_json::json!({
                "layer": f.layer,
                "type": format!("{:?}", f.failure_type),
                "severity": format!("{:?}", f.severity),
                "message": f.message,
            })
        }).collect();

        let result = serde_json::json!({
            "stl": path,
            "resin": resin.name(),
            "summary": {
                "total_layers": summary.total_layers,
                "critical_failures": summary.critical_failures,
                "warnings": summary.warnings,
                "max_peel_force_n": summary.max_peel_force_n,
                "max_force_layer": summary.max_force_layer,
                "min_safety_factor": summary.min_safety_factor,
                "min_safety_layer": summary.min_safety_layer,
                "max_temperature_c": summary.max_temperature_c,
                "max_z_deflection_um": summary.max_z_deflection_um,
            },
            "failures": failures,
        });
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
    } else {
        println!("Print health report: {path}");
        println!("  Resin: {}, Printer: {}", resin.name(), printer.name);
        println!("  Supports: {} x {:.1}mm radius", n_supports, tip_radius);
        println!();
        println!("Summary ({} layers):", summary.total_layers);
        println!("  Max peel force: {:.1} N at layer {}", summary.max_peel_force_n, summary.max_force_layer);
        println!("  Min safety factor: {:.2} at layer {}", summary.min_safety_factor, summary.min_safety_layer);
        println!("  Max temperature: {:.1}°C", summary.max_temperature_c);
        println!("  Max Z deflection: {:.1} µm", summary.max_z_deflection_um);
        println!();

        let crits = summary.critical_failures;
        let warns = summary.warnings;
        if crits == 0 && warns == 0 {
            println!("Result: PASS — no failures detected");
        } else {
            if crits > 0 {
                println!("Result: FAIL — {crits} critical failure(s), {warns} warning(s)");
            } else {
                println!("Result: WARN — {warns} warning(s)");
            }
            println!();
            for f in sim.failures() {
                let sev = match f.severity {
                    Severity::Critical => "CRIT",
                    Severity::Warning => "WARN",
                    Severity::Info => "INFO",
                };
                println!("  [{sev}] Layer {}: {}", f.layer, f.message);
            }
        }
    }
}

fn cmd_inspect_layers(file: &str, from: Option<u32>, to: Option<u32>, stats: bool, json: bool) {
    use resinsim_core::io::ctb;
    use resinsim_core::io::sliced;
    use std::path::Path;

    let path = Path::new(file);
    let format = sliced::detect_format(path).unwrap_or("unknown");

    let (info, layers) = match format {
        "CTB" => ctb::parse_ctb(path).unwrap_or_else(|e| {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }),
        other => {
            eprintln!("Format {other} not yet supported for layer inspection");
            std::process::exit(1);
        }
    };

    let filtered: Vec<&sliced::LayerInput> = layers.iter().filter(|l| {
        l.index >= from.unwrap_or(0) && l.index <= to.unwrap_or(u32::MAX)
    }).collect();

    if stats || json {
        let areas: Vec<f64> = filtered.iter().map(|l| l.cross_section_area_mm2).collect();
        let max_area = areas.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let min_area = areas.iter().copied().fold(f64::INFINITY, f64::min);
        let mean_area = areas.iter().sum::<f64>() / areas.len().max(1) as f64;

        if json {
            let result = serde_json::json!({
                "file": file,
                "info": {
                    "format": info.format,
                    "total_layers": info.total_layers,
                    "layer_height_um": info.layer_height_um,
                    "resolution": [info.resolution_xy.0, info.resolution_xy.1],
                    "bed_size_mm": [info.bed_size_mm.0, info.bed_size_mm.1],
                    "normal_exposure_sec": info.normal_exposure_sec,
                    "bottom_exposure_sec": info.bottom_exposure_sec,
                    "bottom_layer_count": info.bottom_layer_count,
                },
                "stats": {
                    "filtered_layers": filtered.len(),
                    "max_area_mm2": max_area,
                    "min_area_mm2": min_area,
                    "mean_area_mm2": mean_area,
                },
            });
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        } else {
            println!("Sliced file: {file}");
            println!("  {info}");
            println!("  Pixel area: {:.6} mm²", info.pixel_area_mm2());
            println!();
            println!("Layer stats ({} layers):", filtered.len());
            println!("  Area: {min_area:.1} — {max_area:.1} mm² (mean {mean_area:.1})");
            println!("  Exposure: {:.2}s (normal), {:.2}s (bottom)", info.normal_exposure_sec, info.bottom_exposure_sec);
        }
    } else {
        println!("Sliced file: {file}");
        println!("  {info}");
        println!();
        println!("{:>6}  {:>10}  {:>8}  {:>8}", "Layer", "Area (mm²)", "Exp (s)", "Z (mm)");
        println!("{}", "-".repeat(40));
        for l in &filtered {
            println!("{:>6}  {:>10.2}  {:>8.2}  {:>8.3}", l.index, l.cross_section_area_mm2, l.exposure_sec, l.z_mm);
        }
    }
}
