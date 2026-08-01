//! `resinsim inspect field` — read-side voxel-field slice inspector.
//! t2f6-field-inspector, ADR-0023.
//!
//! Loads a persisted `<stem>.sim.json` + `<stem>.fields.bin` pair via
//! `repositories::load_envelope_with_budget` (NEVER re-runs a solver —
//! ADR-0019's whole rationale for the sidecar) and renders one 2D
//! slice through one of the five Tier-2 voxel fields as an aligned
//! text table + ASCII histogram, or as JSON.
//!
//! Sibling of `profile_loader.rs`; kept out of `main.rs` to keep that
//! file from growing past its current size (main.rs is already ~1900
//! lines with seven inline inspect subcommands).

#![cfg(feature = "field-sim")]

use std::path::Path;

use resinsim_core::{
    repositories::load_envelope_with_budget,
    services::{
        FieldRef,
        FieldSlicer,
    },
    values::{
        FIELD_BUDGET_CEILING_BYTES,
        FieldSlice,
        FieldStats,
        FieldStatsScope,
        SliceAxis,
        SlicePlane,
    },
};

use crate::{
    FieldKindArg,
    SliceAxisArg,
    SliceSpec,
};

/// Voxels above this count trigger a stderr warning before `--values`
/// dumps the dense row-major array — a 4K LCD layer slab is
/// 3840×2160 ≈ 8.3M f32, which is ~100 MB of JSON at full precision.
const VALUES_WARN_THRESHOLD: usize = 1_000_000;

/// KB-162 / ADR-0018 §9 model-gap caveat, echoed on every stress-field
/// render. Per-voxel σ_vm here is free-shrinkage stress ONLY; it does
/// NOT include the cumulative residual stress that builds as later
/// layers cure against already-cured layers below — see
/// `StressField::yield_fraction`'s doc-comment and
/// `docs/patterns/honest-zero-with-model-gap-caveat.md`.
const STRESS_MODEL_GAP_CAVEAT: &str = "note: per-voxel stress reflects free-shrinkage only (KB-162) — it does NOT include \
     cumulative residual stress from layer-on-layer curing (ADR-0018 §9); real MSLA warpage \
     is driven by the latter";

/// Entry point called from `main.rs`'s `#[cfg(feature = "field-sim")]`
/// branch of the `field` subcommand handler.
#[allow(clippy::too_many_arguments)]
pub fn run(
    in_path: &Path,
    field_kind: FieldKindArg,
    slice_spec: SliceSpec,
    bins: u32,
    include_values: bool,
    cured_only: bool,
    json: bool,
) {
    let loaded = match load_envelope_with_budget(in_path, FIELD_BUDGET_CEILING_BYTES) {
        Ok(l) => l,
        Err(e) => fail(&format!("Error: {e}"), 1),
    };
    let sim = &loaded.simulation;

    // PhotoinitiatorField carries no voxel_size_mm()/bbox_min_mm() of
    // its own (services::field_slicer module docs — it is dimension-
    // locked to its companion CureField but stores no coordinate
    // metadata). Source coordinates from the paired CureField, which
    // set_voxel_fields guarantees is present whenever photoinitiator
    // is. An absent pairing is itself the "no voxel fields" error.
    let (field_ref, voxel_size_mm, bbox_min_mm) = match field_kind {
        FieldKindArg::Cure => match sim.cure_field() {
            Some(f) => (FieldRef::Cure(f), f.voxel_size_mm(), f.bbox_min_mm()),
            None => fail_no_field(in_path, "cure"),
        },
        FieldKindArg::Photoinitiator => match (sim.cure_field(), sim.photoinitiator_field()) {
            (Some(cure), Some(pi)) => (
                FieldRef::Photoinitiator(pi),
                cure.voxel_size_mm(),
                cure.bbox_min_mm(),
            ),
            _ => fail_no_field(in_path, "photoinitiator"),
        },
        FieldKindArg::Strain => match sim.strain_field() {
            Some(f) => (FieldRef::Strain(f), f.voxel_size_mm(), f.bbox_min_mm()),
            None => fail_no_field(in_path, "strain"),
        },
        FieldKindArg::Stress => match sim.stress_field() {
            Some(f) => (FieldRef::Stress(f), f.voxel_size_mm(), f.bbox_min_mm()),
            None => fail_no_field(in_path, "stress"),
        },
        FieldKindArg::Thermal => match sim.thermal_field() {
            Some(f) => (FieldRef::Thermal(f), f.voxel_size_mm(), f.bbox_min_mm()),
            None => fail_no_field(in_path, "thermal"),
        },
    };

    let axis = to_domain_axis(slice_spec.axis);
    let layer_heights = sim.layer_height_provenance().map(|p| p.ctb_layer_heights());

    let index = match FieldSlicer::resolve_index(
        &field_ref,
        axis,
        slice_spec.value_mm,
        voxel_size_mm,
        bbox_min_mm,
        layer_heights,
    ) {
        Ok(i) => i,
        Err(e) => fail(&format!("Error: {e}"), 2),
    };

    let plane = axis.plane();
    let field_slice = match FieldSlicer::slice(&field_ref, plane, index, voxel_size_mm, bbox_min_mm)
    {
        Ok(s) => s,
        Err(e) => fail(&format!("Error: {e}"), 2),
    };

    let scope = if cured_only {
        FieldStatsScope::Nonzero
    } else {
        FieldStatsScope::All
    };
    let stats = match field_slice.stats(scope) {
        Ok(s) => s,
        Err(e) => fail(&format!("Error: {e}"), 2),
    };

    let histogram = compute_histogram(field_slice.values(), bins.max(1));

    if json {
        render_json(
            in_path,
            field_kind,
            slice_spec,
            &field_slice,
            &stats,
            &histogram,
            include_values,
        );
    } else {
        render_text(
            in_path,
            field_kind,
            slice_spec,
            &field_slice,
            &stats,
            &histogram,
            include_values,
        );
    }
}

fn to_domain_axis(axis: SliceAxisArg) -> SliceAxis {
    match axis {
        SliceAxisArg::X => SliceAxis::X,
        SliceAxisArg::Y => SliceAxis::Y,
        SliceAxisArg::Z => SliceAxis::Z,
    }
}

fn plane_str(plane: SlicePlane) -> &'static str {
    match plane {
        SlicePlane::Xy => "xy",
        SlicePlane::Xz => "xz",
        SlicePlane::Yz => "yz",
    }
}

/// Error convention (ADR-0023, review-ux binding condition): prose to
/// stderr, empty stdout, nonzero exit — IDENTICAL under `--json` and
/// text mode. `inspect field` never emits a JSON error envelope.
fn fail(message: &str, code: i32) -> ! {
    eprintln!("{message}");
    std::process::exit(code);
}

fn fail_no_field(in_path: &Path, field_name: &str) -> ! {
    fail(
        &format!(
            "Error: {} carries no {field_name} voxel field. This sim.json is either a Tier-1 \
             scalar run (no `--voxel-cure-mm` was used) or a Tier-2 run whose sidecar does not \
             include {field_name}. Re-run `resinsim sim --voxel-cure-mm <MM>` to produce a \
             paired sidecar carrying this field.",
            in_path.display()
        ),
        1,
    )
}

/// One histogram bin: `[lo, hi)` except the last bin, which is
/// `[lo, hi]` (inclusive of the slice's maximum value).
struct HistogramBin {
    lo: f32,
    hi: f32,
    count: u64,
}

/// Nearest-rank-free linear-width binning. `bins` is the caller's
/// requested count (clamped to >= 1 by the caller). Guards the
/// `lo == hi` all-equal-slice case explicitly — returns ONE bin
/// spanning the single value rather than dividing by a zero width.
/// Never clamps or floors an undefined value into a placeholder bin
/// (`docs/patterns/anti/magic-floor-vs-honest-filter.md`); the only
/// clamp is the standard histogram boundary rule that the exact
/// maximum value lands in the last bin rather than one-past-the-end.
fn compute_histogram(values: &[f32], bins: u32) -> Vec<HistogramBin> {
    if values.is_empty() {
        return Vec::new();
    }
    let min = values.iter().copied().fold(f32::INFINITY, f32::min);
    let max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    if min == max {
        return vec![HistogramBin {
            lo: min,
            hi: max,
            count: values.len() as u64,
        }];
    }
    let bins = bins as usize;
    let width = (max - min) / bins as f32;
    let mut counts = vec![0u64; bins];
    for &v in values {
        let raw = ((v - min) / width) as usize;
        counts[raw.min(bins - 1)] += 1;
    }
    (0..bins)
        .map(|i| HistogramBin {
            lo: min + i as f32 * width,
            hi: min + (i + 1) as f32 * width,
            count: counts[i],
        })
        .collect()
}

fn scope_label(scope: FieldStatsScope) -> &'static str {
    match scope {
        FieldStatsScope::All => "all voxels",
        FieldStatsScope::Nonzero => "nonzero voxels only (--cured-only)",
    }
}

fn stats_scope_json_str(scope: FieldStatsScope) -> &'static str {
    match scope {
        FieldStatsScope::All => "all",
        FieldStatsScope::Nonzero => "nonzero",
    }
}

/// Guard non-finite stats explicitly before serialising rather than
/// relying on serde_json's silent `null` coercion
/// (`docs/patterns/anti/serde-json-non-finite-f32-null-coercion.md`).
/// Defensive only: `FieldStats` is always constructed from a
/// `FieldSlice`, which validates every value finite at construction,
/// so a non-finite stat should be unreachable in practice.
fn finite_or_null(v: f32) -> serde_json::Value {
    if v.is_finite() {
        serde_json::json!(v)
    } else {
        serde_json::Value::Null
    }
}

fn maybe_warn_large_values(count: usize) {
    if count > VALUES_WARN_THRESHOLD {
        eprintln!(
            "warning: --values is dumping {count} elements (> {VALUES_WARN_THRESHOLD}); output \
             may be large"
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_json(
    in_path: &Path,
    field_kind: FieldKindArg,
    slice_spec: SliceSpec,
    slice: &FieldSlice,
    stats: &FieldStats,
    histogram: &[HistogramBin],
    include_values: bool,
) {
    let mut payload = serde_json::json!({
        "file": in_path.display().to_string(),
        "field": field_kind.name(),
        "units": slice.unit_label(),
        "plane": plane_str(slice.plane()),
        "index": slice.index(),
        "axis": slice_spec.axis.label(),
        "world_mm": slice_spec.value_mm,
        "dims": { "nu": slice.nu(), "nv": slice.nv() },
        "stats_scope": stats_scope_json_str(stats.scope),
        "stats": {
            "count": stats.count,
            "nonzero_count": stats.nonzero_count,
            "min": finite_or_null(stats.min),
            "max": finite_or_null(stats.max),
            "mean": finite_or_null(stats.mean),
            "p95": finite_or_null(stats.p95),
            "p99": finite_or_null(stats.p99),
        },
        "histogram": {
            "bins": histogram
                .iter()
                .map(|b| serde_json::json!({"lo": b.lo, "hi": b.hi, "count": b.count}))
                .collect::<Vec<_>>(),
        },
    });
    if matches!(field_kind, FieldKindArg::Stress) {
        payload["model_gap_caveat"] = serde_json::json!(STRESS_MODEL_GAP_CAVEAT);
    }
    if include_values {
        maybe_warn_large_values(slice.values().len());
        payload["values"] = serde_json::json!(slice.values());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&payload)
            .expect("internal error: serde_json scalar serialisation is infallible by construction; panic here indicates a corrupted build or heap exhaustion")
    );
}

#[allow(clippy::too_many_arguments)]
fn render_text(
    in_path: &Path,
    field_kind: FieldKindArg,
    slice_spec: SliceSpec,
    slice: &FieldSlice,
    stats: &FieldStats,
    histogram: &[HistogramBin],
    include_values: bool,
) {
    println!("Field slice: {}", in_path.display());
    println!(
        "  field {}  plane {}  index {}  {}={:.3}mm  dims {}x{}  units {:?}",
        field_kind.name(),
        plane_str(slice.plane()),
        slice.index(),
        slice_spec.axis.label(),
        slice_spec.value_mm,
        slice.nu(),
        slice.nv(),
        slice.unit_label(),
    );
    println!();
    println!("Stats ({}):", scope_label(stats.scope));
    println!("{:>10}  {:>12}", "count", stats.count);
    println!("{:>10}  {:>12}", "nonzero", stats.nonzero_count);
    println!("{:>10}  {:>12.4}", "min", stats.min);
    println!("{:>10}  {:>12.4}", "max", stats.max);
    println!("{:>10}  {:>12.4}", "mean", stats.mean);
    println!("{:>10}  {:>12.4}", "p95", stats.p95);
    println!("{:>10}  {:>12.4}", "p99", stats.p99);
    if matches!(field_kind, FieldKindArg::Stress) {
        println!();
        println!("  {STRESS_MODEL_GAP_CAVEAT}");
    }
    println!();
    println!("Histogram ({} bins):", histogram.len());
    let max_count = histogram.iter().map(|b| b.count).max().unwrap_or(0);
    for b in histogram {
        let bar_len = b
            .count
            .checked_mul(40)
            .and_then(|n| n.checked_div(max_count))
            .unwrap_or(0) as usize;
        println!(
            "  [{:>10.4}, {:>10.4})  {:>8}  {}",
            b.lo,
            b.hi,
            b.count,
            "#".repeat(bar_len)
        );
    }
    if include_values {
        maybe_warn_large_values(slice.values().len());
        println!();
        println!("Values (row-major, nu={}):", slice.nu());
        for row in slice.values().chunks(slice.nu().max(1) as usize) {
            println!("  {row:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- compute_histogram ----

    #[test]
    fn compute_histogram_all_equal_returns_one_bin_no_div_by_zero() {
        let values = vec![4.0_f32; 10];
        let hist = compute_histogram(&values, 20);
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].lo, 4.0);
        assert_eq!(hist[0].hi, 4.0);
        assert_eq!(hist[0].count, 10);
    }

    #[test]
    fn compute_histogram_empty_returns_no_bins() {
        let values: Vec<f32> = vec![];
        let hist = compute_histogram(&values, 20);
        assert!(hist.is_empty());
    }

    #[test]
    fn compute_histogram_distributes_values_across_bins() {
        // 0..=9 across 5 bins of width 1.8: [0,1.8) [1.8,3.6) [3.6,5.4)
        // [5.4,7.2) [7.2,9.0].
        let values: Vec<f32> = (0..=9).map(|v| v as f32).collect();
        let hist = compute_histogram(&values, 5);
        assert_eq!(hist.len(), 5);
        let total: u64 = hist.iter().map(|b| b.count).sum();
        assert_eq!(total, 10, "every value must land in exactly one bin");
        // Max value (9.0) must land in the LAST bin, not overflow past it.
        assert!(hist[4].count >= 1, "max value must land in the last bin");
    }

    #[test]
    fn compute_histogram_max_value_lands_in_last_bin_not_overflowed() {
        let values = vec![0.0_f32, 10.0_f32];
        let hist = compute_histogram(&values, 4);
        let total: u64 = hist.iter().map(|b| b.count).sum();
        assert_eq!(total, 2);
        assert_eq!(
            hist[3].count, 1,
            "exact max must be clamped into the last bin"
        );
        assert_eq!(hist[0].count, 1);
    }

    // ---- finite_or_null ----

    #[test]
    fn finite_or_null_passes_through_finite_values() {
        assert_eq!(finite_or_null(3.5), serde_json::json!(3.5));
    }

    #[test]
    fn finite_or_null_maps_non_finite_to_null() {
        assert_eq!(finite_or_null(f32::NAN), serde_json::Value::Null);
        assert_eq!(finite_or_null(f32::INFINITY), serde_json::Value::Null);
        assert_eq!(finite_or_null(f32::NEG_INFINITY), serde_json::Value::Null);
    }

    // ---- label helpers ----

    #[test]
    fn plane_str_matches_expected_spellings() {
        assert_eq!(plane_str(SlicePlane::Xy), "xy");
        assert_eq!(plane_str(SlicePlane::Xz), "xz");
        assert_eq!(plane_str(SlicePlane::Yz), "yz");
    }

    #[test]
    fn stats_scope_json_str_matches_expected_spellings() {
        assert_eq!(stats_scope_json_str(FieldStatsScope::All), "all");
        assert_eq!(stats_scope_json_str(FieldStatsScope::Nonzero), "nonzero");
    }
}
