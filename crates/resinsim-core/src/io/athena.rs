//! Athena II analytic force-log ingest. See ADR-0021.
//!
//! NanoDLP writes a real print log as a **tall** CSV (`analytic-*.csv.gz`,
//! optionally embedded in a `.nanodlp`): one measurement per row, three columns
//!
//! ```text
//! ID,T,V
//! 1773757584788963800,6,-341.7
//! ```
//!
//! - `ID` — nanosecond-epoch timestamp of the sample
//! - `T`  — channel code (0–17); see the `CH_*` constants
//! - `V`  — the value for that channel
//!
//! This is the schema the real Athena FSS (force-sensor system) emits, decoded
//! from the `mikeporterdev/nanodlp-analyzer` channel map. The pressure channel
//! (`CH_PRESSURE` = 6) is the load-cell reading used for peel-force validation.
//!
//! # Sign convention
//!
//! Under peel/separation the plate pulls *up* on the load cell, which reads as a
//! **negative** raw count (samples run ~ −340). [`peel_signal`] flips the sign so
//! a larger peel corresponds to a larger positive number. The value is still in
//! raw load-cell counts, **not Newtons** — the absolute counts→Newton gain is
//! unknown per printer and is fitted by
//! [`ProfileCalibrator`](crate::services::ProfileCalibrator); comparison against
//! the simulated peel force is done in a normalized space
//! ([`ForceComparator`](crate::services::ForceComparator)).

use std::io::Read;
use std::path::Path;

// --- Channel codes (nanodlp-analyzer ChartDataGenerator map) ---
/// Layer height (mm). Emitted once per layer — used as a layer boundary marker.
pub const CH_LAYER_HEIGHT: u8 = 0;
pub const CH_SOLID_AREA: u8 = 1;
pub const CH_SPEED: u8 = 4;
pub const CH_CURE_TIME: u8 = 5;
/// Pressure / FSS load-cell reading (peel force signal, raw counts).
pub const CH_PRESSURE: u8 = 6;
/// Resin temperature (inside), °C.
pub const CH_RESIN_TEMP: u8 = 7;
/// Ambient temperature (outside), °C.
pub const CH_AMBIENT_TEMP: u8 = 8;
pub const CH_LAYER_TIME: u8 = 9;
pub const CH_LIFT_HEIGHT: u8 = 10;
pub const CH_DYNAMIC_WAIT: u8 = 17;

/// Upper bound on decompressed analytic bytes (gzip-bomb guard). A full real
/// print is ~4 MB compressed / tens of MB raw; 512 MiB is generous headroom.
pub const MAX_ANALYTIC_DECOMPRESSED: u64 = 512 * 1024 * 1024;

/// Flip the raw load-cell sign so peel reads positive. Result is in raw counts,
/// not Newtons (see module docs).
pub fn peel_signal(raw: f64) -> f64 {
    -raw
}

/// One decoded row of the analytic log.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnalyticSample {
    pub ts_ns: u64,
    pub channel: u8,
    pub value: f64,
}

/// A parsed Athena analytic log: the full tall sample stream in file order.
#[derive(Debug, Clone, Default)]
pub struct AnalyticLog {
    pub samples: Vec<AnalyticSample>,
}

impl AnalyticLog {
    /// All `(ts_ns, value)` for one channel, in file order.
    pub fn channel(&self, channel: u8) -> Vec<(u64, f64)> {
        self.samples
            .iter()
            .filter(|s| s.channel == channel)
            .map(|s| (s.ts_ns, s.value))
            .collect()
    }

    /// Sign-corrected peel-force signal (raw counts) with timestamps.
    pub fn peel_signal_series(&self) -> Vec<(u64, f64)> {
        self.samples
            .iter()
            .filter(|s| s.channel == CH_PRESSURE)
            .map(|s| (s.ts_ns, peel_signal(s.value)))
            .collect()
    }

    /// Mean of a channel's values, or `None` if the channel is absent.
    pub fn channel_mean(&self, channel: u8) -> Option<f64> {
        let vals: Vec<f64> = self
            .samples
            .iter()
            .filter(|s| s.channel == channel)
            .map(|s| s.value)
            .collect();
        if vals.is_empty() {
            None
        } else {
            Some(vals.iter().sum::<f64>() / vals.len() as f64)
        }
    }
}

/// Parse an already-decompressed tall `ID,T,V` stream.
pub fn parse_analytic<R: Read>(reader: R) -> Result<AnalyticLog, String> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(false)
        .from_reader(reader);

    let mut samples = Vec::new();
    for (row, result) in rdr.records().enumerate() {
        let rec = result.map_err(|e| format!("analytic CSV row {}: {e}", row + 1))?;
        if rec.len() < 3 {
            return Err(format!(
                "analytic CSV row {} has {} fields, need 3",
                row + 1,
                rec.len()
            ));
        }
        let ts_ns: u64 = rec[0]
            .trim()
            .parse()
            .map_err(|e| format!("analytic CSV row {}: bad ID {:?}: {e}", row + 1, &rec[0]))?;
        let channel: u8 = rec[1]
            .trim()
            .parse()
            .map_err(|e| format!("analytic CSV row {}: bad T {:?}: {e}", row + 1, &rec[1]))?;
        let value: f64 = rec[2]
            .trim()
            .parse()
            .map_err(|e| format!("analytic CSV row {}: bad V {:?}: {e}", row + 1, &rec[2]))?;
        samples.push(AnalyticSample {
            ts_ns,
            channel,
            value,
        });
    }
    Ok(AnalyticLog { samples })
}

/// Load an analytic log from a `.csv` or `.csv.gz` file. Gzip is detected by
/// magic bytes (`1f 8b`), not just the extension, and decompression is bounded
/// against gzip bombs.
pub fn load_analytic_csv(path: &Path) -> Result<AnalyticLog, String> {
    let mut file =
        std::fs::File::open(path).map_err(|e| format!("failed to open analytic CSV: {e}"))?;
    let mut magic = [0u8; 2];
    let n = file
        .read(&mut magic)
        .map_err(|e| format!("analytic CSV read: {e}"))?;
    use std::io::Seek;
    file.rewind()
        .map_err(|e| format!("analytic CSV rewind: {e}"))?;

    if n == 2 && magic == [0x1f, 0x8b] {
        let decoder = flate2::read::MultiGzDecoder::new(file);
        parse_analytic(decoder.take(MAX_ANALYTIC_DECOMPRESSED))
    } else {
        parse_analytic(file.take(MAX_ANALYTIC_DECOMPRESSED))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    const TALL: &str = "ID,T,V\n\
        100,0,0.050\n\
        101,6,-400.0\n\
        102,6,-360.0\n\
        103,7,28.0\n\
        104,8,22.0\n";

    #[test]
    fn parse_tall_splits_channels() {
        let log = parse_analytic(TALL.as_bytes()).expect("parses");
        assert_eq!(log.samples.len(), 5);
        assert_eq!(log.channel(CH_PRESSURE).len(), 2);
        assert_eq!(log.channel(CH_LAYER_HEIGHT).len(), 1);
    }

    #[test]
    fn peel_signal_flips_sign() {
        // Raw −400 counts (peel) → +400 signal.
        assert_eq!(peel_signal(-400.0), 400.0);
        let log = parse_analytic(TALL.as_bytes()).expect("parses");
        let peel = log.peel_signal_series();
        assert_eq!(peel[0].1, 400.0);
        assert_eq!(peel[1].1, 360.0);
    }

    #[test]
    fn channel_mean_of_pressure() {
        let log = parse_analytic(TALL.as_bytes()).expect("parses");
        // mean of −400 and −360 = −380.
        let pressure_mean = log
            .channel_mean(CH_PRESSURE)
            .expect("pressure channel present in fixture");
        assert!((pressure_mean + 380.0).abs() < 1e-9);
        assert_eq!(log.channel_mean(99), None);
    }

    #[test]
    fn malformed_row_rejected() {
        let bad = "ID,T,V\n100,6,not_a_number\n";
        assert!(parse_analytic(bad.as_bytes()).is_err());
    }

    #[test]
    fn load_gzip_fixture_by_magic() {
        let log = load_analytic_csv(&fixture("mini-analytic.csv.gz")).expect("gz parses");
        assert!(!log.samples.is_empty());
        assert!(!log.channel(CH_PRESSURE).is_empty(), "has pressure channel");
    }

    #[test]
    fn load_plain_csv_roundtrip() {
        let dir = std::env::temp_dir();
        let p = dir.join("resinsim-athena-plain-test.csv");
        let mut f = std::fs::File::create(&p).expect("create temp");
        f.write_all(TALL.as_bytes()).expect("write");
        let log = load_analytic_csv(&p).expect("plain parses");
        assert_eq!(log.samples.len(), 5);
        let _ = std::fs::remove_file(&p);
    }
}
