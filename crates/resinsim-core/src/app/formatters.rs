//! Presentation-layer helpers for the `app` module.
//!
//! Currently exposes a single helper (`format_duration_hms`) used by
//! [`ReportGenerator::text_format`](super::report_generator::ReportGenerator::text_format)
//! to render print-duration fields of [`SimSummary`](crate::simulation::SimSummary).
//!
//! The H:MM:SS format is optimised for the CLI `report health` subcommand's
//! human-readable output. Future non-CLI consumers (e.g. a Bevy visualisation
//! layer, referenced as a hypothetical future consumer in the
//! [`ReportGenerator`](super::report_generator::ReportGenerator) module doc)
//! may want different duration formats — at that time, consider promoting
//! this helper to a `DurationFormatter` trait or moving presentation-layer
//! helpers back to the `resinsim-inspect` crate. For this lifecycle the
//! helper is colocated with its single caller (`ReportGenerator`) in the
//! core crate's app layer.

/// Format a duration in seconds as H:MM:SS. Hours are unbounded (no rollover
/// to days — print jobs routinely exceed 24h). Non-finite or negative values
/// render as "—" so human output never leaks NaN/∞/negative-duration noise.
pub fn format_duration_hms(secs: f32) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "—".to_string();
    }
    let total = secs as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    format!("{h}:{m:02}:{s:02}")
}

#[cfg(test)]
mod duration_tests {
    use super::format_duration_hms;

    #[test]
    fn zero_is_zero_zero_zero() {
        assert_eq!(format_duration_hms(0.0), "0:00:00");
    }

    #[test]
    fn sub_minute() {
        assert_eq!(format_duration_hms(59.0), "0:00:59");
    }

    #[test]
    fn one_hour() {
        assert_eq!(format_duration_hms(3600.0), "1:00:00");
    }

    #[test]
    fn twelve_hours() {
        assert_eq!(format_duration_hms(43_200.0), "12:00:00");
    }

    #[test]
    fn just_under_24h() {
        assert_eq!(format_duration_hms(86_399.0), "23:59:59");
    }

    #[test]
    fn exactly_24h() {
        assert_eq!(format_duration_hms(86_400.0), "24:00:00");
    }

    #[test]
    fn twenty_seven_hours() {
        assert_eq!(format_duration_hms(100_000.0), "27:46:40");
    }

    #[test]
    fn forty_eight_hours() {
        assert_eq!(format_duration_hms(172_800.0), "48:00:00");
    }

    #[test]
    fn nan_renders_em_dash() {
        assert_eq!(format_duration_hms(f32::NAN), "—");
    }

    #[test]
    fn infinity_renders_em_dash() {
        assert_eq!(format_duration_hms(f32::INFINITY), "—");
    }

    #[test]
    fn negative_renders_em_dash() {
        assert_eq!(format_duration_hms(-1.0), "—");
    }
}
