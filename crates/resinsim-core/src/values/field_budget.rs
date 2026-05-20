//! Pre-allocation budget guard shared across all `field-sim` voxel fields.
//!
//! ADR-0018 (t2f3, round-2 finding HIGH-risk). The full Tier-2 path
//! allocates four dense `Array3` fields together — `CureField`,
//! `PhotoinitiatorField`, `StrainField`, `StressField` — and the strain +
//! stress tensors cost 6× the cure/pi single-f32 footprint. A typical
//! `50×50×100 mm` part at 0.05 mm voxels reaches 24 GB for strain alone,
//! exceeding the 18 GB peak budget accepted per
//! `feedback_memory_tradeoffs.md`. Without a hard guard, the user hits a
//! silent kernel OOM with no actionable error.
//!
//! Each voxel-field constructor MUST call [`enforce_field_budget`] before
//! invoking `Array3::zeros`. The check is uniform — same budget per
//! field, same env-override variable — so all four fields either
//! succeed together or fail uniformly with a clear suggested fix.
//!
//! # Configuration
//!
//! - Default budget: 4 GB per field (`DEFAULT_MAX_FIELD_ALLOCATION_BYTES`).
//! - Env override: `RESINSIM_MAX_FIELD_BYTES` (parsed as `u64`); invalid
//!   values are ignored with a warn-to-stderr.
//!
//! See `agent-constraints/implementation-conventions.md` §"Cargo feature
//! matrix" — CI / nextest environments that intentionally exercise large
//! allocations MUST set the env var explicitly.
//!
//! # In-memory vs on-disk budget (ADR-0019 / t2f3.5)
//!
//! This module's cap is the **in-memory** budget — peak RAM allocated
//! per field at construction. ADR-0019 introduces a separate **on-disk**
//! axis: the paired binary sidecar `<stem>.fields.bin` carries the
//! four fields per-layer-zstd-compressed, so on-disk footprint is
//! typically 10-30× smaller than the in-memory dense Array3. There is
//! NO on-disk cap; the in-memory cap here remains the single binding
//! constraint at runtime. The sidecar decoder calls
//! [`active_budget_bytes`] at descriptor-parse time to reject
//! decompression-bomb inputs BEFORE any allocation — see
//! `crates/resinsim-core/src/repositories/sidecar/decoder.rs`.

#![cfg(feature = "field-sim")]

use thiserror::Error;

/// Default per-field allocation budget: 4 GB. Sized to comfortably hold
/// a Mars 5 Ultra full-envelope cure field at 0.2 mm voxels (~985 MB)
/// while leaving headroom for typical 4-field Tier-2 workloads.
pub const DEFAULT_MAX_FIELD_ALLOCATION_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Environment variable name that overrides
/// `DEFAULT_MAX_FIELD_ALLOCATION_BYTES`. Value is parsed as `u64`
/// (decimal bytes). Invalid values fall back to the default with a
/// stderr warning.
pub const FIELD_BUDGET_ENV_VAR: &str = "RESINSIM_MAX_FIELD_BYTES";

/// Error returned by [`enforce_field_budget`] when a voxel-field
/// constructor's intended allocation exceeds the configured budget.
#[derive(Debug, Clone, PartialEq, Error)]
#[error(
    "{field_name}: requested {requested_bytes} bytes ({} GiB) exceeds budget {budget_bytes} bytes ({} GiB); \
     reduce voxel resolution (suggested >= {suggested_voxel_size_mm} mm) or override via {env_var}",
    .requested_bytes / (1024 * 1024 * 1024),
    .budget_bytes / (1024 * 1024 * 1024),
)]
pub struct FieldAllocationError {
    pub field_name: &'static str,
    pub requested_bytes: u64,
    pub budget_bytes: u64,
    pub suggested_voxel_size_mm: f32,
    pub env_var: &'static str,
}

/// Resolve the active budget for this process — env-var override or default.
///
/// Reads `RESINSIM_MAX_FIELD_BYTES` once per call; invalid values warn to
/// stderr and fall back. Returning the resolved value (rather than
/// caching it) lets tests vary the env var per-scenario without process
/// restart, which the budget-exceeded UAT requires.
pub fn active_budget_bytes() -> u64 {
    match std::env::var(FIELD_BUDGET_ENV_VAR) {
        Ok(s) => match s.parse::<u64>() {
            Ok(n) => n,
            Err(_) => {
                eprintln!(
                    "warning: {FIELD_BUDGET_ENV_VAR}={s} is not a valid u64; falling back to \
                     DEFAULT_MAX_FIELD_ALLOCATION_BYTES = {DEFAULT_MAX_FIELD_ALLOCATION_BYTES}"
                );
                DEFAULT_MAX_FIELD_ALLOCATION_BYTES
            }
        },
        Err(_) => DEFAULT_MAX_FIELD_ALLOCATION_BYTES,
    }
}

/// Enforce the budget BEFORE invoking `Array3::zeros`. Pass the
/// element-size-in-bytes of the field's voxel type, the intended voxel
/// dimensions, the current voxel-size-in-mm, and a stable field name
/// for error messages.
///
/// Returns `Err(FieldAllocationError)` with an actionable
/// `suggested_voxel_size_mm` (the smallest voxel size that would
/// satisfy the budget at the same `nx × ny × nz` proportions; computed
/// by inverting the dense-array byte formula).
pub fn enforce_field_budget(
    field_name: &'static str,
    nx: u32,
    ny: u32,
    nz: u32,
    element_size_bytes: u64,
    current_voxel_size_mm: f32,
) -> Result<(), FieldAllocationError> {
    let requested = u64::from(nx) * u64::from(ny) * u64::from(nz) * element_size_bytes;
    let budget = active_budget_bytes();
    if requested <= budget {
        return Ok(());
    }
    // Invert the dense-array byte formula: bytes ∝ 1/voxel_size_mm³ at
    // fixed physical bbox, so to satisfy `bytes ≤ budget` we scale the
    // voxel size up by ratio^(1/3).
    let ratio = (requested as f64) / (budget as f64);
    let scale = ratio.cbrt() as f32;
    let suggested = (current_voxel_size_mm * scale).max(current_voxel_size_mm);
    Err(FieldAllocationError {
        field_name,
        requested_bytes: requested,
        budget_bytes: budget,
        suggested_voxel_size_mm: suggested,
        env_var: FIELD_BUDGET_ENV_VAR,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Use a unique env-var manipulation per test by serialising via a
    // mutex; ordinary `std::env::set_var` is process-global and racy
    // when tests run in parallel. nextest defaults to one-thread-per-
    // test BUT we still want determinism.
    use std::sync::Mutex;
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn default_budget_when_env_unset() {
        let _g = ENV_LOCK
            .lock()
            .expect("test fixture: env-lock mutex never poisoned within a single test scope");
        unsafe { std::env::remove_var(FIELD_BUDGET_ENV_VAR) };
        assert_eq!(active_budget_bytes(), DEFAULT_MAX_FIELD_ALLOCATION_BYTES);
    }

    #[test]
    fn env_override_applied() {
        let _g = ENV_LOCK
            .lock()
            .expect("test fixture: env-lock mutex never poisoned within a single test scope");
        unsafe { std::env::set_var(FIELD_BUDGET_ENV_VAR, "1000000") };
        assert_eq!(active_budget_bytes(), 1_000_000);
        unsafe { std::env::remove_var(FIELD_BUDGET_ENV_VAR) };
    }

    #[test]
    fn invalid_env_falls_back_to_default() {
        let _g = ENV_LOCK
            .lock()
            .expect("test fixture: env-lock mutex never poisoned within a single test scope");
        unsafe { std::env::set_var(FIELD_BUDGET_ENV_VAR, "not-a-number") };
        assert_eq!(active_budget_bytes(), DEFAULT_MAX_FIELD_ALLOCATION_BYTES);
        unsafe { std::env::remove_var(FIELD_BUDGET_ENV_VAR) };
    }

    #[test]
    fn small_allocation_passes_default_budget() {
        let _g = ENV_LOCK
            .lock()
            .expect("test fixture: env-lock mutex never poisoned within a single test scope");
        unsafe { std::env::remove_var(FIELD_BUDGET_ENV_VAR) };
        // 10×10×10 voxels × 4 bytes = 4000 B — trivially under 4 GB.
        assert!(enforce_field_budget("test", 10, 10, 10, 4, 0.5).is_ok());
    }

    #[test]
    fn over_budget_returns_error_with_suggested_voxel_size() {
        let _g = ENV_LOCK
            .lock()
            .expect("test fixture: env-lock mutex never poisoned within a single test scope");
        // Cap budget at 1 MB so we don't need to allocate truly huge
        // dimensions to trigger overflow.
        unsafe { std::env::set_var(FIELD_BUDGET_ENV_VAR, "1000000") };
        // 200×200×200 × 24 bytes = 192 MB — way over 1 MB.
        let err = enforce_field_budget("strain_field", 200, 200, 200, 24, 0.1)
            .expect_err("over-budget allocation must surface FieldAllocationError");
        assert_eq!(err.field_name, "strain_field");
        assert_eq!(err.budget_bytes, 1_000_000);
        // 200*200*200 = 8M voxels × 24 bytes = 192M bytes requested.
        assert_eq!(err.requested_bytes, 200 * 200 * 200 * 24);
        // Suggested voxel size should be strictly larger than current
        // (we need fewer voxels to fit the budget).
        assert!(
            err.suggested_voxel_size_mm > 0.1,
            "suggested must be > current 0.1 mm; got {}",
            err.suggested_voxel_size_mm
        );
        unsafe { std::env::remove_var(FIELD_BUDGET_ENV_VAR) };
    }

    #[test]
    fn error_message_mentions_env_var() {
        let _g = ENV_LOCK
            .lock()
            .expect("test fixture: env-lock mutex never poisoned within a single test scope");
        unsafe { std::env::set_var(FIELD_BUDGET_ENV_VAR, "1000") };
        let err = enforce_field_budget("test", 100, 100, 100, 4, 0.5).expect_err("over budget");
        let msg = format!("{err}");
        assert!(
            msg.contains("RESINSIM_MAX_FIELD_BYTES"),
            "error must name the env var for self-help: {msg}"
        );
        unsafe { std::env::remove_var(FIELD_BUDGET_ENV_VAR) };
    }
}
