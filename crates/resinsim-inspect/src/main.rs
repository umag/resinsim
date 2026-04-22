use clap::{Parser, Subcommand};

mod profile_loader;

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
    /// Full print risk assessment from an STL file.
    ///
    /// Resolves --printer and --resin by loading TOML from the data
    /// directory (see ADR-0004 for the 4-stage resolution chain: --data-dir
    /// flag > RESINSIM_DATA_DIR env > $CWD/data > <binary>/data).
    Health {
        /// Path to STL file
        #[arg(long)]
        stl: Option<String>,
        /// Path to sliced file (CTB, auto-detected)
        #[arg(long)]
        file: Option<String>,
        /// Resin name (file stem of a .toml under <data-dir>/resins/).
        ///
        /// The binary resolves the data-dir via the 4-stage chain documented
        /// in docs/adr/0004-cli-profile-loading.md: --data-dir flag →
        /// $RESINSIM_DATA_DIR → $CWD/data → <binary-parent>/data. Unknown
        /// names hard-error with the list of available profiles.
        #[arg(long, default_value = "generic_standard")]
        resin: String,
        /// Printer name (file stem of a .toml under <data-dir>/printers/).
        /// See --resin for the data-dir resolution chain.
        #[arg(long, default_value = "generic_msla_4k")]
        printer: String,
        /// Profile data directory (stage (a) of the ADR-0004 resolution chain).
        #[arg(long)]
        data_dir: Option<std::path::PathBuf>,
        /// Support tip radius in mm
        #[arg(long, default_value_t = 0.2)]
        tip_radius: f32,
        /// Number of supports
        #[arg(long, default_value_t = 20)]
        n_supports: u32,
        /// Ambient temperature in °C
        #[arg(long, default_value_t = 22.0)]
        ambient: f32,
        /// Initial LED case temperature in °C at print start (ADR-0007 / KB-152).
        /// When omitted, falls back to --ambient — the legacy single-stage
        /// behaviour. Supply the idle-standby reading from the printer (e.g.
        /// Mars 5 Ultra ≈ 27 °C) to exercise the two-stage LED → vat model.
        #[arg(long)]
        initial_led_temp: Option<f32>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum InspectDomain {
    /// Compute cure depth from Beer-Lambert equation.
    ///
    /// Accepts profile-sourced defaults via --resin; explicit --dp/--ec
    /// override profile values. See docs/adr/0004-cli-profile-loading.md
    /// for the data-dir resolution chain.
    Cure {
        /// Penetration depth in µm. Profile source: resin
        /// penetration_depth_um. Required unless --resin is given.
        #[arg(long, required_unless_present = "resin")]
        dp: Option<f32>,
        /// Critical energy in mJ/cm². Profile source: resin
        /// critical_energy_mj_cm2. Required unless --resin is given.
        #[arg(long, required_unless_present = "resin")]
        ec: Option<f32>,
        /// Energy dose in mJ/cm² (user input — not profile-sourced).
        #[arg(long)]
        energy: f32,
        /// Resin profile name (see ADR-0004 for resolution chain).
        #[arg(long)]
        resin: Option<String>,
        /// Profile data directory (stage (a) of the ADR-0004 resolution chain).
        #[arg(long)]
        data_dir: Option<std::path::PathBuf>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Compute peel force for a given cross-section.
    ///
    /// Accepts profile-sourced defaults via --printer / --resin; explicit
    /// scalar flags override profile values. See ADR-0004 for the data-dir
    /// resolution chain.
    Force {
        /// Cross-section area in mm² (not profile-sourced — user input only).
        #[arg(long)]
        area: f64,
        /// Peel adhesion in kPa. Profile source: resin peel_adhesion_kpa.
        /// Default: 13.0 (standard FEP).
        #[arg(long)]
        sigma: Option<f32>,
        /// Lift speed in mm/min. Profile source: printer lift_speed_mm_min.
        /// Default: 60.0.
        #[arg(long)]
        speed: Option<f32>,
        /// Reference speed for sigma measurement in mm/min. Profile source:
        /// printer ref_lift_speed_mm_min. Default: 60.0.
        #[arg(long)]
        ref_speed: Option<f32>,
        /// Sealed cavity area in mm² (suction; not profile-sourced).
        #[arg(long, default_value_t = 0.0)]
        sealed_area: f64,
        /// Support tip radius in mm
        #[arg(long)]
        tip_radius: Option<f32>,
        /// Number of supports
        #[arg(long)]
        n_supports: Option<u32>,
        /// Resin tensile strength in MPa. Profile source: resin
        /// tensile_strength_mpa. Default: 35.0.
        #[arg(long)]
        tensile: Option<f32>,
        /// Printer profile name (see ADR-0004).
        #[arg(long)]
        printer: Option<String>,
        /// Resin profile name (see ADR-0004).
        #[arg(long)]
        resin: Option<String>,
        /// Profile data directory (stage (a) of the ADR-0004 resolution chain).
        #[arg(long)]
        data_dir: Option<std::path::PathBuf>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Compute vat temperature and viscosity drift over a print.
    ///
    /// Accepts profile-sourced defaults via --printer / --resin; explicit
    /// scalar flags override profile values. See ADR-0004 for the data-dir
    /// resolution chain.
    Thermal {
        /// Total number of layers
        #[arg(long)]
        layers: u32,
        /// Exposure time per layer in seconds. Profile source: printer
        /// normal_exposure_sec. Default: 2.5.
        #[arg(long)]
        exposure: Option<f32>,
        /// Lift cycle time in seconds. Profile source: printer
        /// lift_cycle_sec. Default: 7.5.
        #[arg(long)]
        lift_cycle: Option<f32>,
        /// Ambient temperature in °C (not profile-sourced — user input only).
        #[arg(long, default_value_t = 22.0)]
        ambient: f32,
        /// Initial LED case temperature in °C at print start. Two-stage model
        /// (ADR-0007 / KB-152) only — requires --printer to source LED thermal
        /// coefficients. When omitted, falls back to --ambient (legacy path).
        #[arg(long)]
        initial_led_temp: Option<f32>,
        /// Steady-state temperature rise in °C. Profile source: printer
        /// delta_t_steady_c. Default: 10.0.
        #[arg(long)]
        delta_t: Option<f32>,
        /// Thermal time constant in seconds. Profile source: printer
        /// thermal_tau_sec. Default: 1200.0.
        #[arg(long)]
        tau: Option<f32>,
        /// Reference viscosity in mPa·s. Profile source: resin
        /// viscosity_mpa_s. Default: 200.0.
        #[arg(long)]
        viscosity: Option<f32>,
        /// Arrhenius activation energy in kJ/mol. Profile source: resin
        /// activation_energy_kj_mol. Default: 52.0.
        #[arg(long)]
        ea: Option<f32>,
        /// Printer profile name (see ADR-0004 for resolution chain).
        #[arg(long)]
        printer: Option<String>,
        /// Resin profile name (see ADR-0004 for resolution chain).
        #[arg(long)]
        resin: Option<String>,
        /// Profile data directory (stage (a) of the ADR-0004 resolution chain).
        #[arg(long)]
        data_dir: Option<std::path::PathBuf>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Compute Z-axis deflection and effective layer height.
    ///
    /// Accepts profile-sourced defaults via --printer; explicit scalar flags
    /// override profile values. See ADR-0004 for the data-dir resolution chain.
    Zaxis {
        /// Peel force in Newtons
        #[arg(long)]
        force: f32,
        /// Z-axis stiffness in N/mm. If omitted and --printer is set, sourced
        /// from the profile; otherwise defaults to 460 (Elegoo Mars class).
        /// Explicit flag always wins over profile (ADR-0004 §Decision(c)).
        #[arg(long)]
        stiffness: Option<f32>,
        /// Commanded layer height in µm. Per ADR-0005, layer_height lives on
        /// ResinProfile.recipe (not PrinterProfile). When omitted, use --resin to
        /// source from the resin's recipe; otherwise defaults to 50.0 µm.
        /// Explicit flag always wins (ADR-0004 §Decision(c)).
        #[arg(long)]
        layer_height: Option<f32>,
        /// Printer profile name (file stem under <data-dir>/printers/).
        /// Triggers data-dir resolution per ADR-0004.
        #[arg(long)]
        printer: Option<String>,
        /// Resin profile name (file stem under <data-dir>/resins/). Per ADR-0005,
        /// layer_height lives on the resin's recipe — pass --resin to source the
        /// layer height from the resin profile when --layer-height is omitted.
        #[arg(long)]
        resin: Option<String>,
        /// Profile data directory (stage (a) of the ADR-0004 resolution chain).
        #[arg(long)]
        data_dir: Option<std::path::PathBuf>,
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
            InspectDomain::Cure {
                dp,
                ec,
                energy,
                resin,
                data_dir,
                json,
            } => cmd_cure(dp, ec, energy, resin.as_deref(), data_dir.as_deref(), json),
            InspectDomain::Force {
                area,
                sigma,
                speed,
                ref_speed,
                sealed_area,
                tip_radius,
                n_supports,
                tensile,
                printer,
                resin,
                data_dir,
                json,
            } => cmd_force(
                area,
                sigma,
                speed,
                ref_speed,
                sealed_area,
                tip_radius,
                n_supports,
                tensile,
                printer.as_deref(),
                resin.as_deref(),
                data_dir.as_deref(),
                json,
            ),
            InspectDomain::Thermal {
                layers,
                exposure,
                lift_cycle,
                ambient,
                initial_led_temp,
                delta_t,
                tau,
                viscosity,
                ea,
                printer,
                resin,
                data_dir,
                json,
            } => cmd_thermal(
                layers,
                exposure,
                lift_cycle,
                ambient,
                initial_led_temp,
                delta_t,
                tau,
                viscosity,
                ea,
                printer.as_deref(),
                resin.as_deref(),
                data_dir.as_deref(),
                json,
            ),
            InspectDomain::Zaxis {
                force,
                stiffness,
                layer_height,
                printer,
                resin,
                data_dir,
                json,
            } => cmd_zaxis(
                force,
                stiffness,
                layer_height,
                printer.as_deref(),
                resin.as_deref(),
                data_dir.as_deref(),
                json,
            ),
            InspectDomain::Athena {
                file,
                from,
                to,
                json,
            } => cmd_athena(&file, from, to, json),
            InspectDomain::Layers {
                file,
                from,
                to,
                stats,
                json,
            } => cmd_inspect_layers(&file, from, to, stats, json),
        },
        Commands::Report { report_type } => match report_type {
            ReportType::Health {
                stl,
                file,
                resin,
                printer,
                data_dir,
                tip_radius,
                n_supports,
                ambient,
                initial_led_temp,
                json,
            } => cmd_report_health(
                stl.as_deref(),
                file.as_deref(),
                &resin,
                &printer,
                data_dir.as_deref(),
                tip_radius,
                n_supports,
                ambient,
                initial_led_temp,
                json,
            ),
        },
    }
}

fn cmd_cure(
    dp: Option<f32>,
    ec: Option<f32>,
    energy: f32,
    resin_name: Option<&str>,
    data_dir: Option<&std::path::Path>,
    json: bool,
) {
    use resinsim_core::services::CureCalculator;
    use resinsim_core::values::{Energy, PenetrationDepth};

    // ADR-0004 precedence. Resolve data-dir only when --resin is set.
    let resin = resin_name.map(|name| {
        let dir = profile_loader::resolve_data_dir(data_dir).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1);
        });
        profile_loader::load_resin(&dir, name).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1)
        })
    });

    // clap's required_unless_present="resin" guarantees dp+ec are Some OR resin is Some.
    // Resolve dp/ec: explicit flag > resin profile source > parse-time error (unreachable).
    let dp_val = dp
        .or_else(|| resin.as_ref().map(|r| r.penetration_depth_um()))
        .expect("clap required_unless_present guarantees dp or resin is set");
    let ec_val = ec
        .or_else(|| resin.as_ref().map(|r| r.critical_energy_mj_cm2()))
        .expect("clap required_unless_present guarantees ec or resin is set");

    let dp = match PenetrationDepth::new(dp_val) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("invalid penetration depth: {e}");
            std::process::exit(2);
        }
    };
    let ec_val = match Energy::new(ec_val) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("invalid critical energy: {e}");
            std::process::exit(2);
        }
    };
    let e = match Energy::new(energy) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("invalid energy: {err}");
            std::process::exit(2);
        }
    };
    let cd = CureCalculator::cure_depth(dp, e, ec_val);

    if json {
        let result = serde_json::json!({
            "cure_depth_um": cd.value(),
            "penetration_depth_um": dp.value(),
            "critical_energy_mj_cm2": ec_val.value(),
            "energy_mj_cm2": energy,
            "sufficient_for_50um": cd.is_sufficient(50.0),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&result)
                .expect("internal error: serde_json scalar serialisation is infallible by construction; panic here indicates a corrupted build or heap exhaustion")
        );
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

// CLI handler — arg count mirrors the clap subcommand's flags plus the ADR-0004
// profile loader parameters. A struct would hide the clap mapping.
#[allow(clippy::too_many_arguments)]
fn cmd_force(
    area: f64,
    sigma: Option<f32>,
    speed: Option<f32>,
    ref_speed: Option<f32>,
    sealed_area: f64,
    tip_radius: Option<f32>,
    n_supports: Option<u32>,
    tensile: Option<f32>,
    printer_name: Option<&str>,
    resin_name: Option<&str>,
    data_dir: Option<&std::path::Path>,
    json: bool,
) {
    use resinsim_core::services::PeelForceCalculator;
    use resinsim_core::values::{CrossSectionArea, SafetyFactor};

    // ADR-0004 precedence. Resolve data-dir only when a profile is named.
    let data_dir_resolved = if printer_name.is_some() || resin_name.is_some() {
        Some(
            profile_loader::resolve_data_dir(data_dir).unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(1);
            }),
        )
    } else {
        None
    };
    // ADR-0005: cmd_force no longer reads recipe fields from --printer — all speed/exposure
    // come from --resin. --printer still triggers data-dir resolution for symmetry with the
    // other subcommands but is otherwise unused by cmd_force today. Underscore-prefix to
    // document the intentional drop without losing the side-effect of failing fast on a
    // broken printer TOML.
    let _printer = printer_name.map(|n| {
        profile_loader::load_printer(data_dir_resolved.as_deref().expect("resolved"), n)
            .unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(1)
            })
    });
    let resin = resin_name.map(|n| {
        profile_loader::load_resin(data_dir_resolved.as_deref().expect("resolved"), n)
            .unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(1)
            })
    });

    let sigma = sigma
        .or_else(|| resin.as_ref().map(|r| r.peel_adhesion_kpa()))
        .unwrap_or(13.0);
    // ADR-0005: lift_speed is a recipe field (moved from PrinterProfile to ResinProfile.recipe).
    let speed = speed
        .or_else(|| resin.as_ref().map(|r| r.recipe().lift_speed_mm_min()))
        .unwrap_or(60.0);
    // ADR-0005: ref_lift_speed is resin chemistry metadata (moved from PrinterProfile to ResinProfile).
    let ref_speed = ref_speed
        .or_else(|| resin.as_ref().map(|r| r.ref_lift_speed_mm_min()))
        .unwrap_or(60.0);
    let tensile = tensile
        .or_else(|| resin.as_ref().map(|r| r.tensile_strength_mpa()))
        .unwrap_or(35.0);

    let speed_factor = PeelForceCalculator::lift_speed_factor(speed, ref_speed);
    let area_val = match CrossSectionArea::new(area) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("invalid area: {e}");
            std::process::exit(2);
        }
    };
    let sealed_area_val = match CrossSectionArea::new(sealed_area) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("invalid sealed area: {e}");
            std::process::exit(2);
        }
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
        println!(
            "{}",
            serde_json::to_string_pretty(&result)
                .expect("internal error: serde_json scalar serialisation is infallible by construction; panic here indicates a corrupted build or heap exhaustion")
        );
    } else {
        println!(
            "Peel force: {peel} (adhesion) + {} (suction) = {total} (total)",
            suction
        );
        println!("  σ = {sigma} kPa, A = {area:.1} mm², f(v) = {speed_factor:.3}");
        if let (Some(cap), Some(sf)) = (capacity, safety) {
            println!("  Support capacity: {cap}");
            println!(
                "  Safety factor: {sf} — {}",
                if sf.is_safe() { "SAFE" } else { "FAIL" }
            );
        }
    }
}

// CLI handler — arg count mirrors clap subcommand flags + profile loaders (ADR-0004).
#[allow(clippy::too_many_arguments)]
fn cmd_thermal(
    layers: u32,
    exposure: Option<f32>,
    lift_cycle: Option<f32>,
    ambient: f32,
    initial_led_temp: Option<f32>,
    delta_t: Option<f32>,
    tau: Option<f32>,
    viscosity: Option<f32>,
    ea: Option<f32>,
    printer_name: Option<&str>,
    resin_name: Option<&str>,
    data_dir: Option<&std::path::Path>,
    json: bool,
) {
    use resinsim_core::entities::{DEFAULT_CURE_KINETICS_EA_KJ_MOL, ResinProfile};
    use resinsim_core::services::{CureCalculator, LayerTimingCalculator, ThermalCalculator};
    use resinsim_core::values::{Energy, InitialLedTemperature, ThermalTimeConstant};

    // ADR-0004 precedence. Resolution triggered only when --printer or --resin is set.
    let data_dir_resolved = if printer_name.is_some() || resin_name.is_some() {
        Some(
            profile_loader::resolve_data_dir(data_dir).unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(1);
            }),
        )
    } else {
        None
    };
    let printer = printer_name.map(|n| {
        profile_loader::load_printer(
            data_dir_resolved
                .as_deref()
                .expect("printer name triggered resolution above"),
            n,
        )
        .unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1)
        })
    });
    let resin = resin_name.map(|n| {
        profile_loader::load_resin(
            data_dir_resolved
                .as_deref()
                .expect("resin name triggered resolution above"),
            n,
        )
        .unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1)
        })
    });

    // Precedence: explicit flag > profile > default (ADR-0004 §Decision(c)).
    // ADR-0005: exposure + lift_cycle are recipe fields (moved to ResinProfile.recipe).
    let exposure = exposure
        .or_else(|| resin.as_ref().map(|r| r.recipe().normal_exposure_sec()))
        .unwrap_or(2.5);
    let lift_cycle = lift_cycle
        .or_else(|| resin.as_ref().map(|r| r.recipe().lift_cycle_sec()))
        .unwrap_or(7.5);
    let delta_t = delta_t
        .or_else(|| printer.as_ref().map(|p| p.delta_t_steady_c()))
        .unwrap_or(10.0);
    let tau = tau
        .or_else(|| printer.as_ref().map(|p| p.thermal_tau_sec()))
        .unwrap_or(1200.0);
    let viscosity = viscosity
        .or_else(|| resin.as_ref().map(|r| r.viscosity_mpa_s()))
        .unwrap_or(200.0);
    let ea = ea
        .or_else(|| resin.as_ref().map(|r| r.activation_energy_kj_mol()))
        .unwrap_or(52.0);

    let tau_val = match ThermalTimeConstant::new(tau) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("invalid thermal time constant: {e}");
            std::process::exit(2);
        }
    };
    // Parse-time validation of --initial-led-temp via the InitialLedTemperature
    // value constructor. Rejects NaN / below absolute zero / infinite before
    // any simulation work begins (ADR-0007 follow-on).
    let initial_led_temp_typed = match initial_led_temp.map(InitialLedTemperature::new).transpose() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("invalid --initial-led-temp: {e}");
            std::process::exit(2);
        }
    };
    // Degradation warnings use the explicit resin if given; otherwise generic-standard.
    let resin_defaults = resin
        .as_ref()
        .cloned()
        .unwrap_or_else(ResinProfile::generic_standard);
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

    // Two-stage path (ADR-0007 / KB-152) when a --printer profile is loaded.
    // Emits LED temp + vat temp + viscosity + Ec(T) per sample layer. Falls
    // back to the legacy single-stage output when no profile is supplied.
    if let (Some(printer), Some(resin_prof)) = (printer.as_ref(), resin.as_ref()) {
        /// Named per-layer thermal sample row — replaces the six-tuple used
        /// previously (step-10 review-code finding). Construction cost is
        /// identical; readability at the consumer sites is higher.
        struct ThermalSample {
            layer: u32,
            time_sec: f32,
            vat: resinsim_core::values::VatTemperature,
            led_c: f32,
            viscosity_mpa_s: f32,
            ec_t_mj_cm2: f32,
        }

        let cumulative = LayerTimingCalculator::cumulative_times_sec(
            resin_prof.recipe(),
            printer,
            layers.max(1),
        );
        let ea_cure = resin_prof.effective_cure_kinetics_ea_kj_mol();
        let ea_cure_is_default = resin_prof.cure_kinetics_ea_kj_mol().is_none();
        if ea_cure_is_default && !json {
            eprintln!(
                "WARNING: cure-kinetics Ea = {DEFAULT_CURE_KINETICS_EA_KJ_MOL} kJ/mol (literature midpoint estimate) \
                 — replace with a measured value in the resin's TOML profile before \
                 trusting cure-depth drift (KB-153)"
            );
        }

        let ec_ref = Energy::new(resin_prof.critical_energy_mj_cm2())
            .expect("validated ResinProfile has positive critical_energy_mj_cm2");
        let ref_temp_c = resin_prof.reference_temp_c();
        let initial_led_c_for_display = initial_led_temp_typed
            .map(|t| t.value())
            .unwrap_or(ambient);

        let samples: Vec<ThermalSample> = sample_layers
            .iter()
            .map(|&l| {
                let vat = ThermalCalculator::vat_temperature_at_layer_v2(
                    resin_prof.recipe(),
                    printer,
                    ambient,
                    initial_led_temp_typed.map(|t| t.value()),
                    l,
                );
                let time_sec = if l == 0 {
                    0.0
                } else {
                    cumulative
                        .get((l - 1) as usize)
                        .copied()
                        .unwrap_or_else(|| *cumulative.last().unwrap_or(&0.0))
                };
                let led_tau_const = ThermalTimeConstant::new(printer.led_tau_sec())
                    .expect("validated PrinterProfile has positive led_tau_sec");
                let led = ThermalCalculator::led_temperature_at_time(
                    initial_led_c_for_display,
                    printer.led_delta_t_steady_c(),
                    led_tau_const,
                    time_sec,
                );
                let mu = ThermalCalculator::viscosity_at_temperature(
                    viscosity,
                    ref_temp_c,
                    vat.value(),
                    ea,
                );
                // Ec(T) via the single-source Arrhenius helper on CureCalculator
                // (KB-153). Matches what FailurePredictor feeds into
                // cure_depth_at_temp, so reported values track the simulator's.
                let ec_t = CureCalculator::ec_at_temp(ec_ref, ref_temp_c, vat, ea_cure);
                ThermalSample {
                    layer: l,
                    time_sec,
                    vat,
                    led_c: led.value(),
                    viscosity_mpa_s: mu,
                    ec_t_mj_cm2: ec_t.value(),
                }
            })
            .collect();

        if json {
            let points: Vec<serde_json::Value> = samples
                .iter()
                .map(|s| {
                    let mu_ratio = ThermalCalculator::viscosity_ratio(ambient, s.vat.value(), ea);
                    serde_json::json!({
                        "layer": s.layer,
                        "time_sec": s.time_sec,
                        "led_temperature_c": s.led_c,
                        "vat_temperature_c": s.vat.value(),
                        "viscosity_mpa_s": s.viscosity_mpa_s,
                        "viscosity_ratio": mu_ratio,
                        "effective_ec_mj_cm2": s.ec_t_mj_cm2,
                        "degradation_risk": resin_defaults.is_degradation_risk(s.vat),
                    })
                })
                .collect();
            let result = serde_json::json!({
                "ambient_c": ambient,
                "initial_led_c": initial_led_c_for_display,
                "led_delta_t_steady_c": printer.led_delta_t_steady_c(),
                "led_tau_sec": printer.led_tau_sec(),
                "led_to_vat_coupling": printer.led_to_vat_coupling(),
                "cure_kinetics_ea_kj_mol": ea_cure,
                "cure_kinetics_ea_is_default": ea_cure_is_default,
                "ea_kj_mol": ea,
                "points": points,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&result)
                    .expect("internal error: serde_json scalar serialisation is infallible by construction; panic here indicates a corrupted build or heap exhaustion")
            );
        } else {
            println!("Vat thermal profile ({layers} layers) — two-stage LED → vat (ADR-0007)");
            println!(
                "  Ambient: {ambient}°C  Initial LED: {:.1}°C  Coupling: {:.2}  LED τ: {}s",
                initial_led_c_for_display,
                printer.led_to_vat_coupling(),
                printer.led_tau_sec(),
            );
            println!();
            println!(
                "{:>6}  {:>8}  {:>7}  {:>7}  {:>10}  {:>8}",
                "Layer", "Time", "LED", "Vat", "Viscosity", "Ec(T)"
            );
            println!(
                "{:>6}  {:>8}  {:>7}  {:>7}  {:>10}  {:>8}",
                "", "(min)", "(°C)", "(°C)", "(mPa·s)", "(mJ/cm²)"
            );
            println!("{}", "-".repeat(60));
            for s in &samples {
                let warn = if resin_defaults.is_degradation_risk(s.vat) {
                    " ⚠"
                } else {
                    ""
                };
                println!(
                    "{:>6}  {:>7.1}  {:>6.1}  {:>6.1}  {:>9.1}  {:>7.3}{}",
                    s.layer,
                    s.time_sec / 60.0,
                    s.led_c,
                    s.vat.value(),
                    s.viscosity_mpa_s,
                    s.ec_t_mj_cm2,
                    warn
                );
            }
        }
        return;
    }

    // Legacy single-stage output when no --printer profile is supplied.
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
        println!(
            "{}",
            serde_json::to_string_pretty(&result)
                .expect("internal error: serde_json scalar serialisation is infallible by construction; panic here indicates a corrupted build or heap exhaustion")
        );
    } else {
        println!(
            "Vat thermal profile ({layers} layers, duty cycle {:.0}%)",
            duty * 100.0
        );
        println!("  Ambient: {ambient}°C, ΔT steady: {delta_t}°C, τ: {tau}s");
        println!();
        println!(
            "{:>6}  {:>8}  {:>8}  {:>10}  {:>8}",
            "Layer", "Time", "Temp", "Viscosity", "µ/µ₀"
        );
        println!(
            "{:>6}  {:>8}  {:>8}  {:>10}  {:>8}",
            "", "(min)", "(°C)", "(mPa·s)", ""
        );
        println!("{}", "-".repeat(50));

        for &l in &sample_layers {
            let t = ThermalCalculator::vat_temperature_at_layer(
                ambient, delta_t, tau_val, l, exposure, lift_cycle,
            );
            let time_min = l as f32 * (exposure + lift_cycle) / 60.0;
            let mu_ratio = ThermalCalculator::viscosity_ratio(ambient, t.value(), ea);
            let mu = viscosity * mu_ratio;
            let warn = if resin_defaults.is_degradation_risk(t) {
                " ⚠"
            } else {
                ""
            };
            println!(
                "{:>6}  {:>7.1}  {:>7.1}  {:>9.1}  {:>7.2}{}",
                l,
                time_min,
                t.value(),
                mu,
                mu_ratio,
                warn
            );
        }
    }
}

// CLI handler — per-arg mapping to clap subcommand. Profile loaders follow ADR-0004.
#[allow(clippy::too_many_arguments)]
fn cmd_zaxis(
    force: f32,
    stiffness: Option<f32>,
    layer_height: Option<f32>,
    printer_name: Option<&str>,
    resin_name: Option<&str>,
    data_dir: Option<&std::path::Path>,
    json: bool,
) {
    use resinsim_core::services::ZAxisCompensator;
    use resinsim_core::values::PeelForce;

    // Precedence (ADR-0004 §Decision(c)):
    //   explicit flag > profile-sourced > built-in default.
    // Data-dir resolution when --printer OR --resin is set (ADR-0004 §Decision(b)).
    let data_dir_resolved = if printer_name.is_some() || resin_name.is_some() {
        Some(
            profile_loader::resolve_data_dir(data_dir).unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(1);
            }),
        )
    } else {
        None
    };
    let printer = printer_name.map(|name| {
        profile_loader::load_printer(
            data_dir_resolved
                .as_deref()
                .expect("printer name triggered resolution"),
            name,
        )
        .unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1)
        })
    });
    // ADR-0005: layer_height is a recipe field. --resin sources it from the resin's recipe.
    let resin = resin_name.map(|name| {
        profile_loader::load_resin(
            data_dir_resolved
                .as_deref()
                .expect("resin name triggered resolution"),
            name,
        )
        .unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1)
        })
    });
    let stiffness = stiffness
        .or_else(|| printer.as_ref().map(|p| p.z_stiffness_n_per_mm()))
        .unwrap_or(460.0);
    let layer_height = layer_height
        .or_else(|| resin.as_ref().map(|r| r.recipe().layer_height_um()))
        .unwrap_or(50.0);

    let force_val = match PeelForce::new(force) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("invalid force: {e}");
            std::process::exit(2);
        }
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
        println!(
            "{}",
            serde_json::to_string_pretty(&result)
                .expect("internal error: serde_json scalar serialisation is infallible by construction; panic here indicates a corrupted build or heap exhaustion")
        );
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
        println!(
            "{}",
            serde_json::to_string_pretty(&result)
                .expect("internal error: serde_json scalar serialisation is infallible by construction; panic here indicates a corrupted build or heap exhaustion")
        );
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

// CLI handler — 9 args after --data-dir (ADR-0004). Profile resolution is
// unconditional here because both --printer and --resin have clap defaults.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn cmd_report_health(
    path: Option<&str>,
    file_path: Option<&str>,
    resin_name: &str,
    printer_name: &str,
    data_dir: Option<&std::path::Path>,
    tip_radius: f32,
    n_supports: u32,
    ambient: f32,
    initial_led_temp: Option<f32>,
    json: bool,
) {
    use resinsim_core::app::SimulationRunner;
    use resinsim_core::entities::{DEFAULT_CURE_KINETICS_EA_KJ_MOL, Severity};
    use resinsim_core::services::build_plate::PlateAdhesionProfile;
    use resinsim_core::services::failure_predictor::SupportConfig;
    use resinsim_core::values::InitialLedTemperature;
    use std::path::Path;

    // Parse-time validation of --initial-led-temp via the typed constructor —
    // rejects NaN / below absolute zero / infinite before any simulation work.
    let initial_led_temp_typed = match initial_led_temp.map(InitialLedTemperature::new).transpose() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("invalid --initial-led-temp: {e}");
            std::process::exit(2);
        }
    };

    let data_dir = match profile_loader::resolve_data_dir(data_dir) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let resin = match profile_loader::load_resin(&data_dir, resin_name) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let printer = match profile_loader::load_printer(&data_dir, printer_name) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let supports = SupportConfig {
        tip_radius_mm: tip_radius,
        n_supports,
    };
    let plate = PlateAdhesionProfile::default_textured();

    let path = file_path.or(path).unwrap_or_else(|| {
        eprintln!("Error: provide --file or --stl");
        std::process::exit(1);
    });

    // Loud warning when the resin TOML relies on the KB-153 literature-midpoint
    // default for cure-kinetics Ea (Ec(T) Arrhenius correction).
    if resin.cure_kinetics_ea_kj_mol().is_none() && !json {
        eprintln!(
            "WARNING: cure-kinetics Ea = {DEFAULT_CURE_KINETICS_EA_KJ_MOL} kJ/mol (literature midpoint estimate) \
             — replace with a measured value in the resin's TOML profile before \
             trusting cure-depth drift (KB-153)"
        );
    }

    let sim = match SimulationRunner::run_auto(
        Path::new(path),
        &resin,
        &printer,
        &supports,
        &plate,
        ambient,
        initial_led_temp_typed,
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };

    let summary = sim.summary();

    if json {
        let failures: Vec<serde_json::Value> = sim
            .failures()
            .iter()
            .map(|f| {
                serde_json::json!({
                    "layer": f.layer,
                    "type": format!("{:?}", f.failure_type),
                    "severity": format!("{:?}", f.severity),
                    "message": f.message,
                })
            })
            .collect();

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
        println!(
            "{}",
            serde_json::to_string_pretty(&result)
                .expect("internal error: serde_json scalar serialisation is infallible by construction; panic here indicates a corrupted build or heap exhaustion")
        );
    } else {
        println!("Print health report: {path}");
        println!("  Resin: {}, Printer: {}", resin.name(), printer.name());
        println!("  Supports: {} x {:.1}mm radius", n_supports, tip_radius);
        println!();
        println!("Summary ({} layers):", summary.total_layers);
        println!(
            "  Max peel force: {:.1} N at layer {}",
            summary.max_peel_force_n, summary.max_force_layer
        );
        println!(
            "  Min safety factor: {:.2} at layer {}",
            summary.min_safety_factor, summary.min_safety_layer
        );
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

    let filtered: Vec<&sliced::LayerInput> = layers
        .iter()
        .filter(|l| l.index >= from.unwrap_or(0) && l.index <= to.unwrap_or(u32::MAX))
        .collect();

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
                    "layer_height_um": info.recipe.layer_height_um(),
                    "resolution": [info.resolution_xy.0, info.resolution_xy.1],
                    "bed_size_mm": [info.bed_size_mm.0, info.bed_size_mm.1],
                    "normal_exposure_sec": info.recipe.normal_exposure_sec(),
                    "bottom_exposure_sec": info.recipe.bottom_exposure_sec(),
                    "bottom_layer_count": info.recipe.bottom_layer_count(),
                },
                "stats": {
                    "filtered_layers": filtered.len(),
                    "max_area_mm2": max_area,
                    "min_area_mm2": min_area,
                    "mean_area_mm2": mean_area,
                },
            });
            println!(
            "{}",
            serde_json::to_string_pretty(&result)
                .expect("internal error: serde_json scalar serialisation is infallible by construction; panic here indicates a corrupted build or heap exhaustion")
        );
        } else {
            println!("Sliced file: {file}");
            println!("  {info}");
            println!("  Pixel area: {:.6} mm²", info.pixel_area_mm2());
            println!();
            println!("Layer stats ({} layers):", filtered.len());
            println!("  Area: {min_area:.1} — {max_area:.1} mm² (mean {mean_area:.1})");
            println!(
                "  Exposure: {:.2}s (normal), {:.2}s (bottom)",
                info.recipe.normal_exposure_sec(),
                info.recipe.bottom_exposure_sec(),
            );
        }
    } else {
        println!("Sliced file: {file}");
        println!("  {info}");
        println!();
        println!(
            "{:>6}  {:>10}  {:>8}  {:>8}",
            "Layer", "Area (mm²)", "Exp (s)", "Z (mm)"
        );
        println!("{}", "-".repeat(40));
        for l in &filtered {
            println!(
                "{:>6}  {:>10.2}  {:>8.2}  {:>8.3}",
                l.index, l.cross_section_area_mm2, l.exposure_sec, l.z_mm
            );
        }
    }
}
