//! Failures rail — left rail listing every `FailureEvent` from
//! the loaded simulation, sorted by layer. Click a row to jump
//! the cursor to that layer.
//!
//! Slice B in `spec/viz-v2-design-brief.md` §5. Per brief §6
//! "Cursor on failure layer": when the cursor sits on a row's
//! layer, the row paints a tonal-step background — no colour, the
//! row's severity dot already carries that.
//!
//! Severity carried by a trailing dot (THRESHOLD_RED for
//! Critical, THRESHOLD_AMBER for Warning, INK_MUTED for Info),
//! never by row background colour. Empty failures show the brief
//! §8 default: `(0 failures, 0 warnings)`.

use bevy_egui::egui;
use resinsim_core::entities::{FailureEvent, Severity};
use resinsim_core::simulation::PrintSimulation;

use super::theme;

pub const FAILURES_WIDTH_DEFAULT: f32 = 240.0;
pub const FAILURES_WIDTH_MIN: f32 = 180.0;

/// Render the failures rail. Returns `Some(layer)` if the user
/// clicked a row, signalling the parent to jump the cursor.
pub fn render(ui: &mut egui::Ui, sim: Option<&PrintSimulation>, cursor: u32) -> Option<u32> {
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("Failures")
            .strong()
            .color(theme::INK)
            .size(14.0),
    );
    ui.separator();

    let Some(sim) = sim else {
        ui.label(
            egui::RichText::new("(no run loaded)")
                .monospace()
                .small()
                .color(theme::INK_MUTED),
        );
        return None;
    };

    let failures = sim.failures();
    if failures.is_empty() {
        ui.label(
            egui::RichText::new("(0 failures, 0 warnings)")
                .monospace()
                .small()
                .color(theme::INK_MUTED),
        );
        return None;
    }

    let (critical, warning, info) = severity_counts(failures);
    ui.label(
        egui::RichText::new(format!(
            "{critical} critical · {warning} warning · {info} info"
        ))
        .monospace()
        .small()
        .color(theme::INK_MUTED),
    );
    ui.add_space(4.0);

    let mut jump_to: Option<u32> = None;
    let sorted = failures_sorted_by_layer(failures);
    egui::ScrollArea::vertical().show(ui, |ui| {
        for event in &sorted {
            if render_row(ui, event, cursor) {
                jump_to = Some(event.layer);
            }
        }
    });

    jump_to
}

fn render_row(ui: &mut egui::Ui, event: &FailureEvent, cursor: u32) -> bool {
    let bg = if event.layer == cursor {
        theme::SURFACE_HIGH
    } else {
        theme::SURFACE_BASE
    };
    let resp = egui::Frame::new()
        .fill(bg)
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("L{:0>4}", event.layer))
                        .monospace()
                        .color(theme::INK),
                );
                ui.label(
                    egui::RichText::new(failure_type_label(event))
                        .small()
                        .color(theme::INK_MUTED),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let dot = severity_dot(event.severity);
                    ui.label(egui::RichText::new(dot.glyph).strong().color(dot.color));
                });
            });
        })
        .response;
    let click_resp = resp.interact(egui::Sense::click());
    click_resp.clicked()
}

struct SeverityDot {
    glyph: &'static str,
    color: egui::Color32,
}

fn severity_dot(severity: Severity) -> SeverityDot {
    match severity {
        Severity::Critical => SeverityDot {
            glyph: "●",
            color: theme::THRESHOLD_RED,
        },
        Severity::Warning => SeverityDot {
            glyph: "●",
            color: theme::THRESHOLD_AMBER,
        },
        Severity::Info => SeverityDot {
            glyph: "○",
            color: theme::INK_MUTED,
        },
    }
}

/// Compact one-line type label for a failure row. Pure helper
/// (matched in tests) so the label set stays auditable.
fn failure_type_label(event: &FailureEvent) -> &'static str {
    use resinsim_core::entities::FailureType::*;
    match event.failure_type {
        SupportOverload => "support overload",
        InsufficientCure => "insufficient cure",
        ZAxisCatastrophic => "z deflection",
        ThermalDegradation => "thermal",
        RapidAreaIncrease => "area spike",
        SuctionCup => "suction cup",
        NonUniformCure => "edge cure",
    }
}

/// Sort failures by layer (stable). Used to walk the rail
/// top-to-bottom in print order. Pure helper for unit tests.
pub fn failures_sorted_by_layer(failures: &[FailureEvent]) -> Vec<&FailureEvent> {
    let mut sorted: Vec<&FailureEvent> = failures.iter().collect();
    sorted.sort_by_key(|f| f.layer);
    sorted
}

/// Tally (critical, warning, info). Pure helper for unit tests.
pub fn severity_counts(failures: &[FailureEvent]) -> (usize, usize, usize) {
    let mut c = 0_usize;
    let mut w = 0_usize;
    let mut i = 0_usize;
    for f in failures {
        match f.severity {
            Severity::Critical => c += 1,
            Severity::Warning => w += 1,
            Severity::Info => i += 1,
        }
    }
    (c, w, i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use resinsim_core::entities::FailureType;

    fn mk(layer: u32, severity: Severity) -> FailureEvent {
        FailureEvent {
            layer,
            failure_type: FailureType::SupportOverload,
            severity,
            message: format!("layer {layer} failed"),
        }
    }

    #[test]
    fn failures_sorted_by_layer_stable() {
        let xs = vec![
            mk(100, Severity::Critical),
            mk(20, Severity::Warning),
            mk(20, Severity::Info),
            mk(5, Severity::Critical),
        ];
        let sorted = failures_sorted_by_layer(&xs);
        let layers: Vec<u32> = sorted.iter().map(|f| f.layer).collect();
        assert_eq!(layers, vec![5, 20, 20, 100]);
        // Stability: within layer 20, Warning came first in input,
        // so it stays first after sort.
        assert_eq!(sorted[1].severity, Severity::Warning);
        assert_eq!(sorted[2].severity, Severity::Info);
    }

    #[test]
    fn failures_sorted_empty_is_empty() {
        let xs: Vec<FailureEvent> = vec![];
        assert!(failures_sorted_by_layer(&xs).is_empty());
    }

    #[test]
    fn severity_counts_zero_for_empty() {
        let xs: Vec<FailureEvent> = vec![];
        assert_eq!(severity_counts(&xs), (0, 0, 0));
    }

    #[test]
    fn severity_counts_tallies_each_kind() {
        let xs = vec![
            mk(1, Severity::Critical),
            mk(2, Severity::Critical),
            mk(3, Severity::Warning),
            mk(4, Severity::Info),
            mk(5, Severity::Info),
            mk(6, Severity::Info),
        ];
        assert_eq!(severity_counts(&xs), (2, 1, 3));
    }

    #[test]
    fn severity_dot_critical_is_red() {
        let dot = severity_dot(Severity::Critical);
        assert_eq!(dot.glyph, "●");
        assert_eq!(dot.color, theme::THRESHOLD_RED);
    }

    #[test]
    fn severity_dot_warning_is_amber() {
        let dot = severity_dot(Severity::Warning);
        assert_eq!(dot.color, theme::THRESHOLD_AMBER);
    }

    #[test]
    fn severity_dot_info_is_muted_hollow() {
        let dot = severity_dot(Severity::Info);
        // Hollow circle for Info — visible-but-quiet, doesn't
        // borrow the threshold colours which the brief reserves.
        assert_eq!(dot.glyph, "○");
        assert_eq!(dot.color, theme::INK_MUTED);
    }

    #[test]
    fn failure_type_label_covers_every_variant() {
        use FailureType::*;
        for ft in [
            SupportOverload,
            InsufficientCure,
            ZAxisCatastrophic,
            ThermalDegradation,
            RapidAreaIncrease,
            SuctionCup,
            NonUniformCure,
        ] {
            let event = FailureEvent {
                layer: 0,
                failure_type: ft,
                severity: Severity::Info,
                message: "".to_string(),
            };
            let label = failure_type_label(&event);
            assert!(!label.is_empty(), "missing label for {ft:?}");
        }
    }
}
