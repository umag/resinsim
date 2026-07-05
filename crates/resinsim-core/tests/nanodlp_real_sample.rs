//! Opt-in integration test against a real (~37 MB) Athena II `.nanodlp` export.
//!
//! The real sample is too large to commit as a fixture, so this test is
//! `#[ignore]` and reads its path from `RESINSIM_NANODLP_SAMPLE`. Run locally
//! with:
//!
//! ```sh
//! RESINSIM_NANODLP_SAMPLE=/path/to/job.nanodlp \
//!   cargo nextest run -p resinsim-core --run-ignored all real_sample
//! ```
//!
//! Committed coverage of the decode path lives in
//! `src/io/nanodlp.rs` tests against the tiny `mini.nanodlp` fixture.

use resinsim_core::io::sliced::parse_sliced;
use std::path::PathBuf;

#[test]
#[ignore = "requires RESINSIM_NANODLP_SAMPLE pointing at a real .nanodlp"]
fn real_sample_parses_via_dispatch() {
    let Ok(raw) = std::env::var("RESINSIM_NANODLP_SAMPLE") else {
        eprintln!("skip: RESINSIM_NANODLP_SAMPLE unset");
        return;
    };
    let path = PathBuf::from(raw);

    let (info, layers) = parse_sliced(&path).expect("real .nanodlp parses via dispatch");

    eprintln!(
        "format={} total_layers={} res={:?} pixel_um={:?} bed_mm=({:.1},{:.1})",
        info.format,
        info.total_layers,
        info.resolution_xy,
        info.pixel_size_um,
        info.bed_size_mm.0,
        info.bed_size_mm.1,
    );
    let peak = layers
        .iter()
        .max_by(|a, b| {
            a.cross_section_area_mm2
                .total_cmp(&b.cross_section_area_mm2)
        })
        .expect("non-empty layer stack");
    let total_area: f64 = layers.iter().map(|l| l.cross_section_area_mm2).sum();
    eprintln!(
        "layers parsed: {} | peak cross-section: layer {} = {:.2} mm² @ z={:.2}mm | mean area {:.3} mm²",
        layers.len(),
        peak.index,
        peak.cross_section_area_mm2,
        peak.z_mm,
        total_area / layers.len() as f64,
    );

    assert_eq!(info.format, "NANODLP");
    assert_eq!(layers.len(), info.total_layers as usize);
    assert!(
        layers.iter().all(|l| l.mask.is_some()),
        "every layer has a mask"
    );
    assert!(peak.cross_section_area_mm2 > 0.0);
}
