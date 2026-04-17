use serde::{Deserialize, Serialize};

/// Single force measurement from Athena II sensor.
/// Matches the CSV schema from the sensor recommendations doc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForceRecord {
    pub layer: u32,
    pub force_n: f32,
    pub timestamp_ms: u64,
    #[serde(default)]
    pub lift_speed_mm_min: Option<f32>,
    #[serde(default)]
    pub area_mm2: Option<f64>,
    #[serde(default)]
    pub vat_temp_c: Option<f32>,
    #[serde(default)]
    pub z_commanded_um: Option<f32>,
    #[serde(default)]
    pub z_actual_um: Option<f32>,
    #[serde(default)]
    pub uv_intensity_mw_cm2: Option<f32>,
}

/// Statistics computed from a set of force records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForceStats {
    pub count: usize,
    pub mean_n: f32,
    pub max_n: f32,
    pub max_layer: u32,
    pub min_n: f32,
    pub std_dev_n: f32,
}

/// Load Athena II force data from a CSV file.
pub fn load_force_csv(path: &std::path::Path) -> Result<Vec<ForceRecord>, String> {
    let mut reader = csv::Reader::from_path(path)
        .map_err(|e| format!("failed to open CSV: {e}"))?;

    let mut records = Vec::new();
    for result in reader.deserialize() {
        let record: ForceRecord = result.map_err(|e| format!("CSV parse error: {e}"))?;
        records.push(record);
    }

    Ok(records)
}

/// Compute statistics over a slice of force records.
pub fn force_stats(records: &[ForceRecord]) -> ForceStats {
    if records.is_empty() {
        return ForceStats {
            count: 0,
            mean_n: 0.0,
            max_n: 0.0,
            max_layer: 0,
            min_n: 0.0,
            std_dev_n: 0.0,
        };
    }

    let count = records.len();
    let sum: f32 = records.iter().map(|r| r.force_n).sum();
    let mean = sum / count as f32;

    let mut max_n = f32::NEG_INFINITY;
    let mut max_layer = 0u32;
    let mut min_n = f32::INFINITY;

    for r in records {
        if r.force_n > max_n {
            max_n = r.force_n;
            max_layer = r.layer;
        }
        if r.force_n < min_n {
            min_n = r.force_n;
        }
    }

    let variance: f32 = records.iter().map(|r| (r.force_n - mean).powi(2)).sum::<f32>() / count as f32;
    let std_dev = variance.sqrt();

    ForceStats {
        count,
        mean_n: mean,
        max_n,
        max_layer,
        min_n,
        std_dev_n: std_dev,
    }
}

/// Filter records to a layer range (inclusive).
pub fn filter_layers(records: &[ForceRecord], from: u32, to: u32) -> Vec<&ForceRecord> {
    records.iter().filter(|r| r.layer >= from && r.layer <= to).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_records() -> Vec<ForceRecord> {
        (0..5)
            .map(|i| ForceRecord {
                layer: i,
                force_n: (i as f32 + 1.0) * 2.0, // 2, 4, 6, 8, 10
                timestamp_ms: i as u64 * 10000,
                lift_speed_mm_min: Some(60.0),
                area_mm2: None,
                vat_temp_c: None,
                z_commanded_um: None,
                z_actual_um: None,
                uv_intensity_mw_cm2: None,
            })
            .collect()
    }

    #[test]
    fn stats_mean() {
        let records = sample_records();
        let s = force_stats(&records);
        // mean of 2,4,6,8,10 = 6.0
        assert!((s.mean_n - 6.0).abs() < 1e-6);
    }

    #[test]
    fn stats_max() {
        let records = sample_records();
        let s = force_stats(&records);
        assert!((s.max_n - 10.0).abs() < 1e-6);
        assert_eq!(s.max_layer, 4);
    }

    #[test]
    fn stats_min() {
        let records = sample_records();
        let s = force_stats(&records);
        assert!((s.min_n - 2.0).abs() < 1e-6);
    }

    #[test]
    fn stats_empty() {
        let s = force_stats(&[]);
        assert_eq!(s.count, 0);
        assert!((s.mean_n).abs() < 1e-6);
    }

    #[test]
    fn filter_layers_range() {
        let records = sample_records();
        let filtered = filter_layers(&records, 1, 3);
        assert_eq!(filtered.len(), 3);
        assert_eq!(filtered[0].layer, 1);
        assert_eq!(filtered[2].layer, 3);
    }
}
