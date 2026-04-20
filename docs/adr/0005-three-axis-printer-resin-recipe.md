---
issue: resin-recipe-model
date: 2026-04-20
---

# ADR-0005: Three-axis domain split — PrinterProfile (hardware envelope), ResinProfile (chemistry + Recipe VO), PairingValidator (domain service)

## Status
Accepted

## Context

`PrinterProfile` today mixes two concerns whose change cadences differ:

- **(a) Hardware mechanics** — change when you swap machines or calibrate:
  `z_stiffness_n_per_mm`, `led_power_mw_cm2`, `pixel_pitch_um`,
  `lcd_uniformity_variation`, `delta_t_steady_c`, `thermal_tau_sec`, `bed_size_mm`.
  Rare and physical. Bounded by motor current, vat geometry, pixel-pitch energy
  density — these are HARD LIMITS, not tuning knobs.

- **(b) Print recipe** — change per resin or per tuning session:
  `layer_height_um`, `normal_exposure_sec`, `bottom_exposure_sec`,
  `bottom_layer_count`, `lift_speed_mm_min`, `ref_lift_speed_mm_min`,
  `lift_distance_mm`, `lift_cycle_sec`. Frequent and operational.
  These are OPERATING POINTS chosen from within the hardware's limits.

The conflation produced the bug ADR-0004's follow-up lifecycle identified:
simulating a different resin on the same printer silently uses the *printer's*
baked-in recipe (e.g. `generic_msla_4k.normal_exposure_sec = 2.5`), not the
resin's recommended recipe. A Saturn with Ceramic Grey uses 2.0 s exposure;
the same Saturn with Premium Black uses 2.5 s. Today, resinsim quietly runs
the wrong exposure whenever the paired printer's recipe does not match the
resin's chemistry.

Real-world slicers (Voxeldance Tango, ChituBox, Lychee) present these
parameters as **resin settings**: the user picks a resin and the slicer
populates the recipe; the underlying printer constrains the valid range.
That is the mental model the refactor aligns to.

## Decision

1. **Three axes, two aggregates, one value object, one domain service.**

   - **Axis 1 — `PrinterProfile` (Aggregate, Hardware Envelope).**
     Identity: `name`.
     Purpose: states what the printer *can* do.
     Replace recipe-leak fields with range fields:
     ```
     layer_height_range_um: FloatRange        # e.g. {min: 20, max: 100}
     exposure_range_sec: FloatRange           # e.g. {min: 0.5, max: 30}
     lift_speed_range_mm_min: FloatRange      # e.g. {min: 10, max: 300}
     bottom_layer_count_max: u32              # SCALAR ceiling (see §2 below)
     ```
     Retain all hardware-intrinsic fields unchanged.

   - **Axis 2 — `ResinProfile` (Aggregate, Material).**
     Identity: `name` (unchanged).
     Chemistry fields unchanged: `penetration_depth_um`, `critical_energy_mj_cm2`,
     `tensile_strength_mpa`, `peel_adhesion_kpa`, `linear_shrinkage_pct`,
     `viscosity_mpa_s`, `reference_temp_c`, `activation_energy_kj_mol`,
     `density_g_cm3`, `degradation_temp_c`, `min_safe_temp_c`,
     **`ref_lift_speed_mm_min`** (moved from `PrinterProfile` — see §3).
     NEW nested field: `recipe: Recipe` (Value Object, see Axis 2b).

   - **Axis 2b — `Recipe` (Value Object nested inside `ResinProfile`).**
     No identity. Equality by value. Replaced as a unit.
     Fields:
     ```
     layer_height_um, bottom_layer_count, transition_layers,
     normal_exposure_sec, bottom_exposure_sec,
     wait_before_cure_sec, wait_before_release_sec, wait_after_release_sec,
     lift_speed_mm_min, lift_cycle_sec, lift_distance_mm
     ```
     Construction surface: `pub(crate) fn new(...) -> Result<Self, String>`
     (calls `validate()` before returning) plus public factory methods
     (`Recipe::generic_standard()`, `Recipe::elegoo_ceramic_grey()`) that
     delegate to `new()`. No `Default` derive — construction is explicit.

   - **Axis 3 — `PairingValidator` (Domain Service, not an aggregate).**
     Signature:
     ```rust
     pub fn validate_pairing(
         printer: &PrinterProfile,
         recipe: &Recipe,
     ) -> Result<(), Vec<String>>
     ```
     Returns **all** violations in the `Vec`, not just the first. Helps the
     user fix every range-mismatch in one pass.
     Called by `SimulationRunner::run_stl` / `run_from_areas` at simulation
     entry, before any layer is sliced or predicted.

2. **`bottom_layer_count_max` is a scalar, not a range.**

   A lower bound on `bottom_layer_count` has no hardware meaning — printers
   can accept zero or one bottom layer (the resulting print may delaminate,
   but nothing in the machine prohibits it). In contrast:

   - `layer_height_um` has both bounds: lower bounded by pixel-pitch energy
     density (below a threshold you cannot cure the resin fully); upper
     bounded by vat geometry + surface-tension lift.
   - `exposure_sec` has both bounds: lower bounded by LED output (below a
     floor the layer does not cure); upper bounded by cycle-time / thermal
     budget.
   - `lift_speed_mm_min` has both bounds: lower bounded by cycle time; upper
     bounded by motor current + mechanical resonance.

   `bottom_layer_count` has no lower-bound hardware constraint, so
   `bottom_layer_count_max: u32` is the correct shape. Upgrading to an
   `IntRange` later is reversible if a hardware constraint emerges.

3. **`ref_lift_speed_mm_min` lives on `ResinProfile` chemistry, not `Recipe`.**

   `ref_lift_speed_mm_min` is the speed at which `peel_adhesion_kpa` was
   measured — measurement metadata for the chemistry datum, not a tuning
   knob. See KB-112 (`peel-force-vs-speed`) and KB-114 (`peel-force-formula`):
   the peel-force model scales `peel_adhesion_kpa` by
   `f_resin(v_lift) / f_resin(v_ref)`, so `v_ref` must travel with the
   adhesion measurement it was taken under. If we later calibrate
   per-printer-configuration, revisit.

4. **`SlicedFileInfo` (CTB-parser DTO) nests `Recipe`; `LayerInput` stays flat.**

   `io/sliced.rs::SlicedFileInfo` is a DTO for file parsing, not a domain
   aggregate. Collapse its recipe-shaped fields (`layer_height_um`,
   `normal_exposure_sec`, `bottom_exposure_sec`, `bottom_layer_count`,
   `lift_speed_mm_min`) into a nested `recipe: Recipe`. File-level metadata
   (`format`, `total_layers`, `resolution_xy`, `pixel_size_um`, `bed_size_mm`)
   stays flat.

   `io/sliced.rs::LayerInput` **retains** its flat recipe-shaped fields
   (`exposure_sec`, `lift_speed_mm_min`, `layer_height_um`) — these are
   per-layer values that can legitimately differ from the Recipe default
   (e.g. transition layers have their own exposure schedule). Collapsing
   them under Recipe would misrepresent per-layer override semantics.

5. **Pairing-validator trust contract.**

   `validate_pairing` trusts that `Recipe::validate()` was called by the
   caller. NaN in a recipe field bypasses range checks because IEEE 754 NaN
   comparisons (`NaN < min`, `NaN > max`) are both false — a naive range
   check would silently accept NaN. Defence is:
   - `Recipe::new()` only produces validated Recipes (NaN rejected at
     construction, see `docs/patterns/nan-two-layer-defence.md` +
     `docs/patterns/anti/rust-nan-positive-validation-gap.md`).
   - `SimulationRunner` calls `resin.validate()` (which delegates to
     `recipe.validate()`) before `pairing_validator::validate_pairing`.

   A unit test on `PairingValidator` locks the invariant by name:
   `nan_recipe_field_accepted_silently_by_pairing_but_caught_by_upstream_validate`.

## Ubiquitous language

- **recipe** — the concrete operating point for a print: exposure times, layer
  height, lift kinematics, wait times. Matches slicer vocabulary ("Elegoo
  recommends this recipe for Ceramic Grey on Mars-class printers").
- **range** / **envelope** — hardware capability: the band of values within
  which a recipe can operate. `range` is the term used in code
  (`layer_height_range_um`).
- **pairing** — verb for "is this recipe compatible with this printer?".
  The pairing-validator domain service answers this question at simulation
  entry.
- **chemistry** — immutable physical properties of a resin formulation:
  `penetration_depth_um`, `critical_energy_mj_cm2`, `tensile_strength_mpa`,
  `peel_adhesion_kpa`, `viscosity_mpa_s`, `activation_energy_kj_mol`,
  `density_g_cm3`, `linear_shrinkage_pct`, thermal thresholds, and (by §3)
  `ref_lift_speed_mm_min`.

## Rejected alternatives

1. **Three peer aggregates (`Printer`, `Resin`, `Recipe` as separate aggregates).**
   Rejected: `Recipe` has no independent lifecycle — it is created, loaded,
   saved, and discarded with its owning resin. Introducing ID references
   without a domain benefit violates the "aggregates model business rules,
   not data relationships" heuristic.

2. **`Recipe` as a separate aggregate loaded per-print (a "run blueprint").**
   Rejected: over-engineered for today's single-print use case. Introduces
   identity + lifecycle before the business actually has one. Revisit if
   batch printing or recipe-version-history emerges as a requirement.

3. **Keep recipe on `PrinterProfile`, add per-resin overrides as an overlay.**
   Rejected: preserves the muddy aggregate. The wrong-exposure bug persists
   whenever the user forgets to apply the overlay. Overlays invert the
   natural mental model — slicers present recipe as a resin concept.

## Consequences

- `PrinterProfile` shrinks to hardware fields + range fields. Existing
  accessors for recipe fields (`normal_exposure_sec()`, `layer_height_um()`,
  etc.) are removed; callers must source those values from `resin.recipe()`.
- `ResinProfile` gains a required `recipe: Recipe` field. Pre-refactor resin
  TOMLs that lack a `[recipe]` table **fail to deserialise** — this is
  deliberate: loud failure is the whole point of the refactor. Release notes
  must document the migration.
- `ResinProfile` also gains a required `ref_lift_speed_mm_min` field (moved from
  `PrinterProfile` per §3 above — chemistry metadata for `peel_adhesion_kpa`).
  Pre-refactor resin TOMLs without this field **fail to deserialise**. Migration:
  add `ref_lift_speed_mm_min = 60.0` (or the mm/min speed at which
  `peel_adhesion_kpa` was measured — KB-112, KB-114) alongside the chemistry
  fields. For most off-the-shelf resin datasheets 60 mm/min is a safe default
  (industry-standard reference for peel measurements). Migration guidance also
  referenced from `spec/uat/legacy-resin-toml-without-recipe.md`.
- `SimulationRunner::run_stl` / `run_from_areas` ordering becomes:
  `resin.validate()` → `printer.validate()` → `validate_pairing(printer, recipe)`
  → `slice_areas(..., recipe.layer_height_um)` → per-layer prediction.
  Pairing fires before slicing so an out-of-range recipe never touches the
  geometry pipeline.
- `SlicedFileInfo` restructures (Axis §4) — both `parse_plain` and
  `parse_encrypted` CTB construction sites populate `Recipe`.
- `resinsim-inspect` CLI subcommands that sourced recipe fields from the
  `--printer` profile (`cmd_peel`, `cmd_thermal`, `cmd_zaxis`) now require
  `--resin` as well. Help text + error messages updated; locked by a new
  UAT (`cli-requires-resin-for-recipe-fields.md`).

## Binding prior ADRs and patterns

- **ADR-0001** (`values/` must not import `entities/`): honoured — the new
  `FloatRange` and `IntRange` value objects are pure value types with no
  entity imports. Entities import values; services import both.
- **ADR-0002** (`Option<T>`, not sentinel values, for absent domain values):
  honoured — no magic sentinels introduced. Ranges use real value types.
- **ADR-0003** (`clippy::unwrap_used` denied workspace-wide): honoured —
  all new constructors (`FloatRange::new`, `IntRange::new`, `Recipe::new`)
  return `Result<Self, ...>`; no `unwrap()`. `.expect("<why>")` used only
  where an upstream validator makes the `Ok` inevitable.
- **ADR-0004** (CLI profile loading — name-based via repositories):
  honoured — `PrinterProfileRepository::load(name)` and
  `ResinProfileRepository::load(name)` still work post-refactor. Name-based
  loading is extended, not replaced. See forward-link in ADR-0004.
- **Pattern** (`entity-validate-on-mutation.md`): honoured — `Recipe`,
  `PrinterProfile`, `ResinProfile` all keep `pub(crate)` fields and the
  validate-on-mutation contract. Demonstration tests preserved for entities;
  added for `Recipe`.
- **Pattern** (`nan-two-layer-defence.md`) + **Anti-pattern**
  (`anti/rust-nan-positive-validation-gap.md`): honoured — `FloatRange::new`
  rejects NaN + non-positive + `min > max`; `Recipe::new` rejects NaN per
  field; explicit `parse_toml_with_nan_*_rejected` tests at each layer.
