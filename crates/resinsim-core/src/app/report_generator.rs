//! Application service: assembles a print-health Report from a completed
//! `PrintSimulation` aggregate. Produces UI-bound text or pretty-printed JSON
//! strings; the caller decides how to surface them.
//!
//! Mirrors the shape of [`crate::app::SimulationRunner`]: unit struct,
//! associated functions, no state. Inputs are the simulation aggregate plus a
//! [`ReportContext`] carrying header data the aggregate doesn't know about
//! (the file path the simulation ran against, the resin and printer display
//! names, the supports config).
//!
//! # Output stability invariants (issue: reportgenerator-extraction)
//!
//! - **Failure iteration order**: both formatters iterate `sim.failures()` in
//!   vector order — never sorted, never grouped by severity. The aggregate's
//!   insertion order is the rendered order.
//! - **Severity labels**: `Critical → CRIT`, `Warning → WARN`, `Info → INFO`.
//!   Hand-mapped here; do not introduce a `Display` impl on `Severity` without
//!   updating golden fixtures.
//! - **JSON field order**: provided by `serde_json`'s default `BTreeMap`
//!   ordering (alphabetical). The `preserve_order` cargo feature is OFF in
//!   the consuming binary's Cargo.toml. Do NOT switch this module to a
//!   `Serialize`-derived struct without re-capturing the byte-identity
//!   golden fixtures — derived structs preserve declaration order, which
//!   would break the alphabetical contract these tests rely on.

use crate::app::formatters::format_duration_hms;
use crate::entities::Severity;
use crate::simulation::PrintSimulation;

/// Header context for a print-health report. Values originate from CLI args
/// (or, in future, a Bevy viz UI) and affect the rendered header lines but
/// do not live on the simulation aggregate.
pub struct ReportContext {
    pub stl_path: String,
    pub resin_name: String,
    pub printer_name: String,
    pub n_supports: u32,
    pub tip_radius_mm: f32,
}

/// Application service: format a completed simulation as text or JSON.
pub struct ReportGenerator;

impl ReportGenerator {
    /// Render the human-readable text report.
    pub fn text_format(sim: &PrintSimulation, ctx: &ReportContext) -> String {
        let summary = sim.summary();
        let mut out = String::new();

        out.push_str(&format!("Print health report: {}\n", ctx.stl_path));
        out.push_str(&format!(
            "  Resin: {}, Printer: {}\n",
            ctx.resin_name, ctx.printer_name
        ));
        out.push_str(&format!(
            "  Supports: {} x {:.1}mm radius\n",
            ctx.n_supports, ctx.tip_radius_mm
        ));
        out.push('\n');
        out.push_str(&format!("Summary ({} layers):\n", summary.total_layers));
        out.push_str(&format!(
            "  Max peel force: {:.1} N at layer {}\n",
            summary.max_peel_force_n, summary.max_force_layer
        ));
        out.push_str(&format!(
            "  Min safety factor: {:.2} at layer {}\n",
            summary.min_safety_factor, summary.min_safety_layer
        ));
        out.push_str(&format!(
            "  Max temperature: {:.1}°C\n",
            summary.max_temperature_c
        ));
        out.push_str(&format!(
            "  Max Z deflection: {:.1} µm\n",
            summary.max_z_deflection_um
        ));
        out.push_str(&format!(
            "  Total time: {}\n",
            format_duration_hms(summary.total_time_sec),
        ));
        out.push_str(&format!(
            "    bottom:     {}\n",
            format_duration_hms(summary.bottom_time_sec),
        ));
        out.push_str(&format!(
            "    transition: {}\n",
            format_duration_hms(summary.transition_time_sec),
        ));
        out.push_str(&format!(
            "    normal:     {}\n",
            format_duration_hms(summary.normal_time_sec),
        ));
        out.push('\n');

        let crits = summary.critical_failures;
        let warns = summary.warnings;
        if crits == 0 && warns == 0 {
            out.push_str("Result: PASS — no failures detected\n");
        } else {
            if crits > 0 {
                out.push_str(&format!(
                    "Result: FAIL — {crits} critical failure(s), {warns} warning(s)\n"
                ));
            } else {
                out.push_str(&format!("Result: WARN — {warns} warning(s)\n"));
            }
            out.push('\n');
            for f in sim.failures() {
                let sev = severity_label(f.severity);
                out.push_str(&format!("  [{sev}] Layer {}: {}\n", f.layer, f.message));
            }
        }

        out
    }

    /// Render the machine-readable JSON report (pretty-printed).
    pub fn json_format(sim: &PrintSimulation, ctx: &ReportContext) -> String {
        let summary = sim.summary();

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
            "stl": ctx.stl_path,
            "resin": ctx.resin_name,
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
                "total_time_sec": summary.total_time_sec,
                "bottom_time_sec": summary.bottom_time_sec,
                "transition_time_sec": summary.transition_time_sec,
                "normal_time_sec": summary.normal_time_sec,
            },
            "failures": failures,
        });

        serde_json::to_string_pretty(&result)
            .expect("internal error: serde_json scalar serialisation is infallible by construction; panic here indicates a corrupted build or heap exhaustion")
    }
}

fn severity_label(s: Severity) -> &'static str {
    match s {
        Severity::Critical => "CRIT",
        Severity::Warning => "WARN",
        Severity::Info => "INFO",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{FailureEvent, FailureType, LayerResult};
    use crate::simulation::print_simulation::tests::{default_recipe, linear_printer};

    fn make_layer(index: u32, force: f32, sf: f32, temp: f32) -> LayerResult {
        LayerResult {
            index,
            cure_depth_um: 100.0,
            peel_force_n: force,
            suction_force_n: 0.0,
            total_force_n: force,
            support_capacity_n: force * sf,
            safety_factor: sf,
            cross_section_area_mm2: 100.0,
            area_delta_mm2: 0.0,
            vat_temperature_c: temp,
            viscosity_mpa_s: 350.0,
            z_deflection_um: 30.0,
            effective_layer_height_um: 50.0,
            worst_cure_depth_um: 100.0,
        }
    }

    fn ctx() -> ReportContext {
        ReportContext {
            stl_path: "/fixtures/test.stl".to_string(),
            resin_name: "Test Resin".to_string(),
            printer_name: "Test Printer".to_string(),
            n_supports: 4,
            tip_radius_mm: 0.5,
        }
    }

    fn pass_sim() -> PrintSimulation {
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        sim.add_layer(make_layer(0, 10.0, 5.0, 25.0), vec![])
            .expect("test fixture: explicit index 0 matches layer count 0 at this call site");
        sim.add_layer(make_layer(1, 12.0, 4.5, 26.0), vec![])
            .expect("test fixture: explicit index 1 matches layer count 1 at this call site");
        sim
    }

    #[test]
    fn text_format_pass_no_failures() {
        let out = ReportGenerator::text_format(&pass_sim(), &ctx());
        // Header section (unchanged by print-time addition).
        assert!(out.contains("Print health report: /fixtures/test.stl\n"));
        assert!(out.contains("  Resin: Test Resin, Printer: Test Printer\n"));
        assert!(out.contains("  Supports: 4 x 0.5mm radius\n"));
        // Summary block — existing fields unchanged.
        assert!(out.contains("Summary (2 layers):\n"));
        assert!(out.contains("  Max peel force: 12.0 N at layer 1\n"));
        assert!(out.contains("  Min safety factor: 4.50 at layer 1\n"));
        assert!(out.contains("  Max temperature: 26.0°C\n"));
        assert!(out.contains("  Max Z deflection: 30.0 µm\n"));
        // Summary block — new print-time lines (inline, NOT a new subsection).
        assert!(
            out.contains("  Total time: "),
            "Total time line missing: {out}"
        );
        assert!(
            out.contains("    bottom:     "),
            "bottom phase line missing: {out}"
        );
        assert!(
            out.contains("    transition: "),
            "transition phase line missing: {out}"
        );
        assert!(
            out.contains("    normal:     "),
            "normal phase line missing: {out}"
        );
        // Result footer.
        assert!(out.contains("Result: PASS — no failures detected\n"));
        // Ordering invariant: phase lines appear AFTER Max Z deflection and
        // BEFORE the Result footer, confirming they live inside the Summary
        // block (NOT a new subsection and NOT mingled with failures).
        let z_pos = out
            .find("Max Z deflection")
            .expect("Z deflection line present");
        let total_time_pos = out.find("Total time:").expect("Total time line present");
        let result_pos = out.find("Result:").expect("Result line present");
        assert!(
            z_pos < total_time_pos && total_time_pos < result_pos,
            "print-time lines must land inside the Summary block: {out}"
        );
    }

    #[test]
    fn text_format_warn_only() {
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        sim.add_layer(
            make_layer(0, 10.0, 5.0, 25.0),
            vec![FailureEvent {
                layer: 0,
                failure_type: FailureType::RapidAreaIncrease,
                severity: Severity::Warning,
                message: "rapid area".to_string(),
            }],
        )
        .expect("test fixture: explicit index 0 matches layer count 0 at this call site");
        let out = ReportGenerator::text_format(&sim, &ctx());
        assert!(
            out.contains("Result: WARN — 1 warning(s)\n"),
            "WARN result line missing: {out}"
        );
        assert!(
            out.contains("  [WARN] Layer 0: rapid area\n"),
            "WARN failure line missing: {out}"
        );
    }

    #[test]
    fn text_format_critical_and_warn() {
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        sim.add_layer(
            make_layer(0, 10.0, 5.0, 25.0),
            vec![
                FailureEvent {
                    layer: 0,
                    failure_type: FailureType::RapidAreaIncrease,
                    severity: Severity::Warning,
                    message: "warn first".to_string(),
                },
                FailureEvent {
                    layer: 0,
                    failure_type: FailureType::SupportOverload,
                    severity: Severity::Critical,
                    message: "crit second".to_string(),
                },
            ],
        )
        .expect("test fixture: explicit index 0 matches layer count 0 at this call site");
        let out = ReportGenerator::text_format(&sim, &ctx());
        assert!(
            out.contains("Result: FAIL — 1 critical failure(s), 1 warning(s)\n"),
            "FAIL result line missing: {out}"
        );
        assert!(out.contains("  [WARN] Layer 0: warn first\n"));
        assert!(out.contains("  [CRIT] Layer 0: crit second\n"));
        // Order invariant: WARN appears before CRIT because that's the order
        // they were inserted into the aggregate.
        let warn_pos = out.find("[WARN]").expect("[WARN] in output");
        let crit_pos = out.find("[CRIT]").expect("[CRIT] in output");
        assert!(
            warn_pos < crit_pos,
            "failures must render in sim.failures() vector order, not severity order: {out}"
        );
    }

    #[test]
    fn json_format_shape_and_keys() {
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        sim.add_layer(
            make_layer(0, 10.0, 5.0, 25.0),
            vec![FailureEvent {
                layer: 0,
                failure_type: FailureType::SupportOverload,
                severity: Severity::Critical,
                message: "boom".to_string(),
            }],
        )
        .expect("test fixture: explicit index 0 matches layer count 0 at this call site");
        let raw = ReportGenerator::json_format(&sim, &ctx());
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("valid json");
        assert_eq!(parsed["stl"], "/fixtures/test.stl");
        assert_eq!(parsed["resin"], "Test Resin");
        assert_eq!(parsed["summary"]["total_layers"], 1);
        assert_eq!(parsed["summary"]["critical_failures"], 1);
        assert_eq!(parsed["summary"]["warnings"], 0);
        assert_eq!(parsed["failures"][0]["layer"], 0);
        assert_eq!(parsed["failures"][0]["severity"], "Critical");
        assert_eq!(parsed["failures"][0]["type"], "SupportOverload");
        assert_eq!(parsed["failures"][0]["message"], "boom");

        // Alphabetical key-ordering contract for the "summary" object.
        //
        // This assertion relies on serde_json's `preserve_order` feature
        // being OFF — Map = BTreeMap<String, Value>, which iterates in
        // alphabetical key order. If the feature is ever flipped on (even
        // indirectly via a transitive dep), this test fails in a non-
        // obvious way because keys would then iterate in declaration
        // order. See the module doc at report_generator.rs:19-24 for the
        // invariant's long-term guard.
        let summary_obj = parsed["summary"]
            .as_object()
            .expect("summary is a JSON object");
        let keys: Vec<&str> = summary_obj.keys().map(|s| s.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "bottom_time_sec",
                "critical_failures",
                "max_force_layer",
                "max_peel_force_n",
                "max_temperature_c",
                "max_z_deflection_um",
                "min_safety_factor",
                "min_safety_layer",
                "normal_time_sec",
                "total_layers",
                "total_time_sec",
                "transition_time_sec",
                "warnings",
            ],
            "summary object must list all 13 keys in alphabetical order"
        );
    }

    #[test]
    fn failures_preserve_input_order() {
        // [WARN, CRIT, WARN] — neither sorted by severity nor grouped.
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        sim.add_layer(
            make_layer(0, 10.0, 5.0, 25.0),
            vec![
                FailureEvent {
                    layer: 0,
                    failure_type: FailureType::RapidAreaIncrease,
                    severity: Severity::Warning,
                    message: "first warn".to_string(),
                },
                FailureEvent {
                    layer: 0,
                    failure_type: FailureType::SupportOverload,
                    severity: Severity::Critical,
                    message: "middle crit".to_string(),
                },
                FailureEvent {
                    layer: 0,
                    failure_type: FailureType::ThermalDegradation,
                    severity: Severity::Warning,
                    message: "last warn".to_string(),
                },
            ],
        )
        .expect("test fixture: explicit index 0 matches layer count 0 at this call site");

        let text = ReportGenerator::text_format(&sim, &ctx());
        let p1 = text.find("first warn").expect("first warn present");
        let p2 = text.find("middle crit").expect("middle crit present");
        let p3 = text.find("last warn").expect("last warn present");
        assert!(p1 < p2 && p2 < p3, "text-mode order broken: {text}");

        let json = ReportGenerator::json_format(&sim, &ctx());
        let q1 = json.find("first warn").expect("first warn in json");
        let q2 = json.find("middle crit").expect("middle crit in json");
        let q3 = json.find("last warn").expect("last warn in json");
        assert!(q1 < q2 && q2 < q3, "json-mode order broken: {json}");
    }
}
