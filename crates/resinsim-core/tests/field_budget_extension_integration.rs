//! t2f6-field-inspector — descriptor-driven decode-budget auto-extension
//! integration tests (gate decision 2026-07-28, binding review condition).
//!
//! Coverage for BOTH sides of `load_envelope_with_budget`:
//!
//! - **above-ceiling REJECTS, naming `RESINSIM_MAX_FIELD_BYTES`** — cheap.
//!   The rejection fires inside `read_descriptor` during descriptor
//!   parsing, strictly BEFORE any array allocation (`decoder.rs`'s
//!   `if implied_total > budget` check precedes `read_scalar_field`'s
//!   `Array3::zeros`). A descriptor can therefore claim an arbitrarily
//!   large size and this test still runs in milliseconds — no real
//!   large allocation is ever attempted.
//! - **above-default, under-ceiling EXTENDS and SUCCEEDS** — expensive.
//!   "Succeeds" means the real decode actually builds an `Array3` at
//!   the claimed size, which is unavoidably a real multi-GB allocation.
//!   Default-skipped behind `RESINSIM_LARGE_SMOKE`, mirroring the
//!   existing opt-in convention at
//!   `resinsim-inspect/tests/sim_golden.rs::large_envelope_serialises_within_budget`.
//!
//! Both fixtures hand-build the RSFIELD sidecar bytes directly against
//! the documented wire format
//! (`docs/patterns/voxel-field-sidecar-binary-format.md`) with an
//! EMPTY (`layer_sizes[0] == 0`) slab, rather than constructing a real
//! `CureField` — a field claiming multi-GB dimensions cannot be built
//! without actually allocating that memory at FIXTURE-build time too,
//! which would double the cost of the expensive test and make the
//! cheap test not cheap at all.

#![cfg(feature = "field-sim")]

use std::path::{
    Path,
    PathBuf,
};

use resinsim_core::{
    entities::{
        PrinterProfile,
        ResinProfile,
    },
    repositories::load_envelope_with_budget,
    simulation::PrintSimulation,
    values::field_budget::{
        DEFAULT_MAX_FIELD_ALLOCATION_BYTES,
        FIELD_BUDGET_CEILING_BYTES,
        FIELD_BUDGET_ENV_VAR,
    },
};
use sha2::{
    Digest,
    Sha256,
};

fn tmp_dir(label: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-tmp")
        .join(format!("field-budget-extension-{label}"));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).expect("test setup: create tmp dir");
    dir
}

// RSFIELD wire-format constants, duplicated here rather than imported —
// integration tests only see resinsim-core's PUBLIC surface, and the
// sidecar module's format constants are not part of it (ADR-0009 keeps
// the binary format internal to the repository layer).
const RSFIELD_MAGIC: [u8; 8] = *b"RSFIELD\0";
const RSFIELD_FORMAT_VERSION: u32 = 2;
const FIELD_KIND_TAG_CURE: u32 = 0;
const FIELD_KIND_TAG_PHOTOINITIATOR: u32 = 1;
const FIELD_COMPONENT_SIZE_SCALAR: u32 = 4;
const COMPRESSION_TAG_ZSTD: u32 = 1;
const LAYOUT_TAG_LAYER_SLABS: u32 = 1;

/// Append one empty-slab (`layer_sizes[0] == 0`) scalar-field descriptor
/// claiming `dim_x × dim_y × 1` to `buf`. Building this is cheap
/// regardless of how large `dim_x`/`dim_y` claim to be — no per-voxel
/// payload bytes exist.
fn append_empty_scalar_descriptor(
    buf: &mut Vec<u8>,
    kind_tag: u32,
    name: &[u8],
    dim_x: u32,
    dim_y: u32,
) {
    buf.extend_from_slice(&(name.len() as u32).to_le_bytes());
    buf.extend_from_slice(name);
    buf.extend_from_slice(&kind_tag.to_le_bytes());
    buf.extend_from_slice(&dim_x.to_le_bytes());
    buf.extend_from_slice(&dim_y.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes()); // dim_z
    buf.extend_from_slice(&0.0f32.to_le_bytes()); // bbox_origin.x
    buf.extend_from_slice(&0.0f32.to_le_bytes()); // bbox_origin.y
    buf.extend_from_slice(&0.0f32.to_le_bytes()); // bbox_origin.z
    buf.extend_from_slice(&0.5f32.to_le_bytes()); // voxel_size_mm
    buf.extend_from_slice(&FIELD_COMPONENT_SIZE_SCALAR.to_le_bytes());
    buf.extend_from_slice(&COMPRESSION_TAG_ZSTD.to_le_bytes());
    buf.extend_from_slice(&LAYOUT_TAG_LAYER_SLABS.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes()); // layer_count
    let uncompressed_layer_byte_size =
        u64::from(dim_x) * u64::from(dim_y) * u64::from(FIELD_COMPONENT_SIZE_SCALAR);
    buf.extend_from_slice(&uncompressed_layer_byte_size.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes()); // layer_offsets[0] — never read when size==0
    buf.extend_from_slice(&0u32.to_le_bytes()); // layer_sizes[0] == 0 (empty slab)
}

/// Hand-build a minimal, single-descriptor RSFIELD sidecar claiming a
/// `dim_x × dim_y × 1` cure field with an EMPTY slab. A real
/// `CureField` constructor cannot produce this shape without genuinely
/// allocating the claimed size, so this is the only way to exercise
/// the format's budget REJECTION path at multi-GB claimed sizes
/// without paying for a multi-GB allocation.
fn build_oversized_empty_cure_sidecar(dim_x: u32, dim_y: u32) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&RSFIELD_MAGIC);
    buf.extend_from_slice(&RSFIELD_FORMAT_VERSION.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes()); // field_count
    buf.extend_from_slice(&[0u8; 48]); // reserved
    append_empty_scalar_descriptor(&mut buf, FIELD_KIND_TAG_CURE, b"cure", dim_x, dim_y);
    buf
}

/// Same shape, but with a matching-dimension photoinitiator descriptor
/// alongside cure — `PrintSimulation::set_voxel_fields` (ADR-0017)
/// requires the pair to install EITHER, so the "decode genuinely
/// succeeds and the field is observable on the aggregate" test needs
/// both. Both descriptors are empty-slab, so no payload bytes are
/// added — but a REAL decode of each still allocates a real `Array3`
/// at the claimed size (this is the expensive test's cost).
fn build_oversized_empty_cure_and_photoinitiator_sidecar(dim_x: u32, dim_y: u32) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&RSFIELD_MAGIC);
    buf.extend_from_slice(&RSFIELD_FORMAT_VERSION.to_le_bytes());
    buf.extend_from_slice(&2u32.to_le_bytes()); // field_count
    buf.extend_from_slice(&[0u8; 48]); // reserved
    append_empty_scalar_descriptor(&mut buf, FIELD_KIND_TAG_CURE, b"cure", dim_x, dim_y);
    append_empty_scalar_descriptor(
        &mut buf,
        FIELD_KIND_TAG_PHOTOINITIATOR,
        b"photoinitiator",
        dim_x,
        dim_y,
    );
    buf
}

/// Write `sidecar_bytes` as `<dir>/model.fields.bin` plus a matching
/// `<dir>/model.sim.json` whose `fields_sidecar` pointer carries the
/// real sha256 of those bytes — the same integrity contract
/// `load_envelope_with_budget` verifies before ever consulting the
/// (untrusted-until-verified) descriptor sizes inside.
fn write_sim_json_with_sidecar(
    dir: &Path,
    sidecar_bytes: &[u8],
    fields_present: &[&str],
) -> PathBuf {
    let bin_path = dir.join("model.fields.bin");
    std::fs::write(&bin_path, sidecar_bytes).expect("write sidecar bytes");

    let mut hasher = Sha256::new();
    hasher.update(sidecar_bytes);
    let sha256: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();

    let recipe = ResinProfile::generic_standard().recipe().clone();
    let printer = PrinterProfile::generic_msla_4k();
    let sim = PrintSimulation::new(recipe, printer);
    let sim_value = serde_json::to_value(&sim).expect("PrintSimulation serialises infallibly");

    let envelope = serde_json::json!({
        "schema_version": 2,
        "simulation": sim_value,
        "fields_sidecar": {
            "path": "model.fields.bin",
            "byte_size": sidecar_bytes.len(),
            "sha256": sha256,
            "fields_present": fields_present,
        },
    });
    let sim_json_path = dir.join("model.sim.json");
    std::fs::write(
        &sim_json_path,
        serde_json::to_string_pretty(&envelope).expect("envelope serialises infallibly"),
    )
    .expect("write sim.json");
    sim_json_path
}

/// `RESINSIM_MAX_FIELD_BYTES` overrides in BOTH directions (binding
/// review condition): when the caller's environment already sets it,
/// auto-extension must NOT engage — even to a value SMALLER than what
/// the descriptor needs and smaller than the ceiling would allow. This
/// is the sharper regression guard than "override succeeds when
/// sufficient" — it proves the override is respected even when
/// honoring it produces a WORSE outcome for the caller than extending
/// would have.
#[test]
fn explicit_env_override_wins_over_auto_extension_even_when_insufficient() {
    let dir = tmp_dir("explicit-override-wins");
    // 200*200*4 = 160,000 bytes required — tiny in absolute terms, but
    // bigger than the artificially small override below.
    let dim = 200_u32;
    let sidecar_bytes = build_oversized_empty_cure_sidecar(dim, dim);
    let required_bytes = u64::from(dim) * u64::from(dim) * 4;
    let sim_json_path = write_sim_json_with_sidecar(&dir, &sidecar_bytes, &["cure"]);

    // SAFETY: single-threaded within this test; restored in a guard
    // below regardless of assertion outcome.
    unsafe { std::env::set_var(FIELD_BUDGET_ENV_VAR, "100000") };
    let result = load_envelope_with_budget(&sim_json_path, FIELD_BUDGET_CEILING_BYTES);
    unsafe { std::env::remove_var(FIELD_BUDGET_ENV_VAR) };

    let err = result.expect_err(
        "test fixture: the user's explicit 100_000-byte override is deliberately smaller than \
         the 160_000-byte requirement, so Err is the expected outcome EVEN THOUGH the 24 GB \
         ceiling would have comfortably covered it via auto-extension — the override must win",
    );
    assert!(
        required_bytes > 100_000,
        "test fixture sanity: required_bytes must exceed the override for this test to mean anything"
    );
    assert!(
        err.contains("RESINSIM_MAX_FIELD_BYTES"),
        "rejection under an explicit (insufficient) override must still name the env var; got: {err}"
    );
}

#[test]
fn above_ceiling_descriptor_rejects_naming_the_env_var() {
    assert!(
        std::env::var(FIELD_BUDGET_ENV_VAR).is_err(),
        "test invariant: RESINSIM_MAX_FIELD_BYTES must be unset in this process for the \
         auto-extension branch under test to engage at all; if this fails, some other test \
         in this binary leaked the env var"
    );
    let dir = tmp_dir("above-ceiling");
    // dim_x * dim_y * 4 bytes ≈ 40 GB — comfortably past the 24 GB
    // ceiling, purely as claimed header integers. No real allocation
    // is ever attempted: rejection fires during descriptor parsing.
    let dim = 100_000_u32;
    let sidecar_bytes = build_oversized_empty_cure_sidecar(dim, dim);
    let required_bytes = u64::from(dim) * u64::from(dim) * 4;
    assert!(
        required_bytes > FIELD_BUDGET_CEILING_BYTES,
        "test fixture: required_bytes must exceed the 24 GB ceiling to exercise rejection"
    );
    let sim_json_path = write_sim_json_with_sidecar(&dir, &sidecar_bytes, &["cure"]);

    let err = load_envelope_with_budget(&sim_json_path, FIELD_BUDGET_CEILING_BYTES).expect_err(
        "test fixture: descriptor deliberately claims more than the ceiling, \
         so Err is the expected outcome",
    );
    assert!(
        err.contains("RESINSIM_MAX_FIELD_BYTES"),
        "above-ceiling rejection must name the override env var; got: {err}"
    );
}

/// Expensive — proving SUCCESS requires the real decode to actually
/// build an `Array3` at the claimed (above-default) size, which is a
/// genuine multi-GB allocation. Default-skipped; opt in with
/// `RESINSIM_LARGE_SMOKE=1`.
#[test]
fn above_default_descriptor_extends_and_succeeds_under_ceiling() {
    if std::env::var("RESINSIM_LARGE_SMOKE").is_err() {
        eprintln!(
            "skipping above_default_descriptor_extends_and_succeeds_under_ceiling — \
             set RESINSIM_LARGE_SMOKE=1 to run this ~4.3 GB allocation test"
        );
        return;
    }
    assert!(
        std::env::var(FIELD_BUDGET_ENV_VAR).is_err(),
        "test invariant: RESINSIM_MAX_FIELD_BYTES must be unset for auto-extension to engage"
    );
    let dir = tmp_dir("above-default-succeeds");
    // dim_x * dim_y * 4 bytes ≈ 4.31 GiB PER FIELD — above the 4 GiB
    // default, comfortably under the 24 GiB ceiling.
    // `set_voxel_fields` (ADR-0017) requires cure + photoinitiator
    // together, so both are allocated at this size (~8.6 GiB peak).
    let dim = 34_000_u32;
    let sidecar_bytes = build_oversized_empty_cure_and_photoinitiator_sidecar(dim, dim);
    let required_bytes = u64::from(dim) * u64::from(dim) * 4;
    assert!(
        required_bytes > DEFAULT_MAX_FIELD_ALLOCATION_BYTES,
        "test fixture: required_bytes must exceed the 4 GiB default to exercise extension"
    );
    assert!(
        required_bytes < FIELD_BUDGET_CEILING_BYTES,
        "test fixture: required_bytes must stay under the 24 GiB ceiling to exercise success"
    );
    let sim_json_path =
        write_sim_json_with_sidecar(&dir, &sidecar_bytes, &["cure", "photoinitiator"]);

    let loaded = load_envelope_with_budget(&sim_json_path, FIELD_BUDGET_CEILING_BYTES)
        .expect("above-default, under-ceiling descriptor must decode successfully");
    let cure = loaded
        .simulation
        .cure_field()
        .expect("sidecar carried a cure field, paired with photoinitiator");
    assert_eq!(cure.dimensions(), (dim, dim, 1));
    assert_eq!(
        cure.dose_at(0, 0, 0).expect("in-bounds"),
        0.0,
        "empty slab decodes to all-zero"
    );
}
