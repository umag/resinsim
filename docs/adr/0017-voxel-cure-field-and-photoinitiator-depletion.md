---
issue: t2f1-voxelized-cure-distribution
date: 2026-05-19
---

# ADR-0017: 3D voxel-resolved cure field and photoinitiator depletion

## Status
Accepted (Phase 4 of issue `t2f1-voxelized-cure-distribution`, 2026-05-19).

## Context

The Tier-1 cure model (KB-103, KB-153) computes a single scalar cure depth
per layer:

```
Cd_layer = Dp × ln(E_layer / Ec(T_layer))
```

Two layer-level companions are stored on `LayerResult`:
`cure_depth_um` (the per-layer scalar above) and `worst_cure_depth_um`
(the same calculation evaluated at the dimmest LCD pixel per
`lcd_uniformity_variation`, KB-120). This shape is internally consistent
and fast; it is also wrong-by-design for three classes of downstream
question that Phase 4 of the simulation plan (`projects/000-global/research/resinsim-physics-simulation-plan.md`) needs to answer:

1. **Photoinitiator depletion.** Real photopolymer kinetics depletes the
   local photoinitiator concentration `C(x, y, z, t)` as photons are
   absorbed. The scalar Beer-Lambert model assumes a constant `Dp` — but
   `Dp ∝ 1 / (k_C × C)`, so as `C` falls, light penetrates deeper. For
   long prints the late-layer cure depth in already-exposed regions is
   meaningfully larger than the scalar predicts.
2. **Per-voxel cure inputs to Tier-2 sibling solvers.** `t2f3`
   (shrinkage strain), `t2f4` (spatial thermal diffusion), and any future
   per-voxel mechanical model need a per-voxel cure field as input, not a
   per-layer scalar.
3. **Voxel cure heatmaps in `resinsim-viz`.** The DESIGN.md
   "instrumentation-first" tone asks the reader to inspect cure quality
   at the same pixel granularity as the LCD itself; the per-layer scalar
   reduces an LCD-resolution physical phenomenon to a single number.

Tier-2 CFD is deferred (ADR-0007 §"Alternatives considered (b)"). The
intermediate Tier-1.5 voxel-resolved lumped-kinetics model lands here.

## Decision

**Six interlocking design decisions are accepted.** Each addresses one
clarifying question raised during triage; they are interdependent and
should be evaluated together, not à la carte.

### 1. Standard radical-photopolymer kinetics for photoinitiator depletion

Per-voxel:

```
dC(x,y,z,t)/dt = -k_d × I(x,y,z,t) × C(x,y,z,t)
```

`k_d` is a per-resin Arrhenius-like decay rate constant in
1 / (mJ·cm⁻² · concentration-fraction) units; `C` is a dimensionless
fraction `[0, 1]`; `I` is the local UV intensity (after column Beer-
Lambert attenuation). See **KB-160** for derivation, fitted constants,
and uncertainty band.

Rejected: a simpler "dose-only Dp drift" form
(`Dp(layer) = Dp_0 × (1 + α × cumulative_dose)`). Cheaper to fit but
masks the underlying physics and produces wrong behaviour when intensity
varies spatially within a layer (which is exactly the LCD-uniformity
case `t2f1` is meant to capture).

### 2. Variable voxel resolution — v1 scope cut

**Planned shape.** Resolution controlled by precedence chain:

| Priority | Source | Default |
|----------|--------|---------|
| 1 (highest) | CLI `--voxel-cure-mm <FLOAT>` value | n/a |
| 2 | `PrinterProfile.voxel_cure_resolution_mm: Option<f32>` | None ⇒ fallthrough |
| 3 (lowest) | Workspace default | **0.2 mm** |

**v1 implementation** (descoped during Phase 5 code review):

- CLI `--voxel-cure-mm <FLOAT>`'s PRESENCE enables voxel mode; its VALUE
  is parsed + validated (finite > 0) but **not consumed at runtime**.
- `PrinterProfile.voxel_cure_resolution_mm` is parsed + validated but
  **not read** by `SimulationRunner`.
- The cure field's X-Y resolution is the slicer `LayerMask`'s
  `voxel_size_mm` (typically 0.5 mm for cavity-detection-class masks,
  0.05 mm for the build-simulation path).
- The cure field's Z-step is `recipe.layer_height_um` (the actual print
  layer thickness — independent of the LATERAL voxel size).

**Why deferred.** Decoupling the X-Y resolution from the slicer mask
requires a resampling pass (different bbox dimensions + a mapping
function). The plumbing for this lives in `t2f5-gpu-acceleration-wgpu`
since GPU dispatch tightly constrains the resolution-vs-memory trade.
The flag value + profile field stay in the public API for forward-
compat.

**Workspace default `DEFAULT_VOXEL_CURE_RESOLUTION_MM = 0.2 mm`** is
reserved for the same future activation. It is referenced by tests
that pin the constant's value; it is not currently consulted at
runtime.

### 3. Dense `ndarray::Array3<f32>` over part bbox; sparse deferred

Storage is a dense 3D f32 array sized to the *printed part's bounding
box* (not the full build envelope). Memory back-of-envelope (Mars 5 Ultra
153×78×165 mm envelope, typical part ~5–20 % envelope occupancy):

| Resolution | Voxels (full envelope, dense) | Voxels (typical 50×50×100 part, dense) |
|------------|-------------------------------|----------------------------------------|
| 0.5 mm | 16 M / 63 MB | 1 M / 4 MB |
| 0.2 mm | 246 M / 985 MB | 16 M / 62 MB |
| 0.1 mm | 1.97 G / 7.9 GB | 125 M / 500 MB |
| 0.05 mm | 15.7 G / 63 GB | 1 G / 4 GB |

With *two* fields (cure + photoinitiator) at f32, double each row.

Sparse representations (OpenVDB / NanoVDB / Bonxai / hash grid) were
researched as part of triage. **OpenVDB / NanoVDB**: gold-standard
hierarchical sparse format but the Rust bindings ecosystem is immature
— `vdb-rs` (Traverse-Research) is read-only; `openvdb-sys` is
autogenerated, macOS-only, and explicitly "not mature". **Bonxai**:
header-only C++, no Rust bindings. **Hash grid**: pure-Rust trivial,
5–10× slower random access than dense. None of these are stable enough
to depend on for the v1 of t2f1.

**Decision**: ship dense Array3<f32> for v1. If a future workload (full-
plate dense print, GPU acceleration in t2f5, or multiple simultaneous
fields with cure + photoinitiator + temperature + strain) exceeds the
memory budget, a sparse follow-on issue is filed at that point.

### 4. `LayerResult.cure_depth_um` / `worst_cure_depth_um` replaced by dispatch methods

The two stored f32 fields on `LayerResult` become `pub(crate)` caches
populated by `SimulationRunner`. External access goes through three
methods that hide the Tier-1/Tier-2 dispatch:

```rust
pub fn cure_depth_um_summary(&self, sim: &PrintSimulation) -> CureDepth;
pub fn worst_cure_depth_um_summary(&self, sim: &PrintSimulation) -> CureDepth;
pub fn cure_depth_um_at_voxel(&self, sim: &PrintSimulation, x: u32, y: u32) -> Option<CureDepth>;
```

When `sim.cure_field.is_some()`, the `_summary` methods return
`LayerSummary.mean` and `LayerSummary.min` for that layer respectively
(LayerSummary is `{ mean: f32, min: f32 }` for v1 — min because "worst"
in this domain means the *most-undercured pixel*, which is the
*minimum* cure depth across the layer, NOT the maximum dose). When
`sim.cure_field.is_none()`, `_summary` delegates to
`CureCalculator::cure_depth_at_temp` and the stored worst scalar
respectively — the Tier-1 path runs unchanged.

The third method, `cure_depth_um_at_voxel`, returns `Option<CureDepth>`
— `Some` when the voxel field is populated and `(x, y)` is in-bbox,
`None` otherwise. Only the viz heatmap voxel-mode rendering path uses
this.

Rejected: keeping the stored scalars on `LayerResult` as the canonical
source and adding the voxel field as a *side* augmentation. This is the
"augment, don't replace" option from triage Q4 — explicitly rejected
in favour of a single canonical source. The migration burden (23+
`cure_depth_um` consumers + 14+ `worst_cure_depth_um` consumers across
the workspace) is real but is paid once in this PR.

### 5. Both compile-time Cargo feature and runtime CLI flag

The voxel cure path is gated *twice*:

- **Cargo feature `field-sim`** on `resinsim-core` (forwarded through
  `resinsim-inspect` and `resinsim-viz` via per-crate
  `field-sim = ["resinsim-core/field-sim"]`). Default builds don't
  pay the `ndarray` dep weight or the new module compile cost.
- **Runtime CLI flag `--voxel-cure-mm <FLOAT>`** on `resinsim-inspect`,
  itself behind `#[cfg(feature = "field-sim")]`. Builds without the
  feature don't show the flag in `--help` and reject it with the
  standard clap unknown-flag error.

Rationale: this is the canonical "feature-flag a heavy capability"
pattern. The Cargo feature isolates dep weight + binary size; the
runtime flag isolates RAM allocation within an already-built binary.

### 6. t2f1 is per-column-only; lateral pixel bleed is t2f2

Each LCD pixel's UV column attenuates by Beer-Lambert only within its
own column. No inter-pixel lateral scattering happens at the t2f1
boundary. Lateral light crosstalk (Gaussian convolution) is the
scope of `t2f2-light-crosstalk-convolution`, a separately filed Phase 4
ticket that depends on t2f1.

This fence keeps t2f1's surface area tractable. Voxel cure values
computed here become the input to t2f2's convolution; t2f2 then
overwrites them.

## Single-PR landing (scope acknowledgement)

This plan ships 2 new value objects, 1 new domain service, 2 new doc
artifacts (this ADR + KB-160), 4+ new entity fields, a Cargo feature,
a runtime CLI flag, and migrates 37+ files (`cure_depth_um` ∪
`worst_cure_depth_um` consumers). The adversarial review during
planning flagged the scope as "at the upper bound of a single landing"
and suggested splitting into `t2f1a` (CureField + delegation) +
`t2f1b` (PhotoinitiatorField + depletion).

**Single-PR landing is accepted by explicit user decision** (2026-05-19,
during Phase 2 planning), citing project memory
`feedback_memory_tradeoffs.md`: simple-first over premature
optimisation. The migration is mechanical (rg + topological-order
sweep), the two new fields share aggregate invariants and validation
plumbing, and splitting would introduce a two-PR coupling that adds
overhead without reducing risk. This decision is recorded so a future
reviewer doesn't read the single PR's diff as "scope creep" without
context.

## Legacy compatibility scope cut

Per explicit user decision (2026-05-19, during Phase 4 kickoff):
**byte-identical legacy sim.json fixture preservation is descoped.**
The plan v4 originally proposed a `tests/fixtures/sim_json_baselines/`
snapshot mechanism + a `legacy_sim_json_v1_parses_with_no_cure_field`
backwards-compat test. Both are removed. Fixtures will be updated
in-place as the migration proceeds; the schema_version bump per ADR-0015
is announced but not paired with a parse-old-version compatibility
shim.

## Alternatives considered

**(a) Augment, don't replace.** Keep `LayerResult.cure_depth_um` /
`worst_cure_depth_um` as canonical; add CureField as an optional
side-output. Rejected: forks every downstream consumer between "read
scalar" and "read field"; multiplies the test matrix; defers the same
migration to a future date with more accumulated consumers.

**(b) Sparse voxel storage via OpenVDB.** Correct shape, wrong
ecosystem maturity for Rust today. Deferred to a follow-on issue if
memory pressure materialises.

**(c) Pin voxel resolution per printer.** Real workloads have part-
specific resolution preferences that no per-printer default covers.
The override chain (CLI > profile > default) makes the right call easy.

**(d) Per-voxel undercure failure prediction.** The plan keeps per-
layer failure semantic for v1 (a layer fails if any voxel undercured,
not each voxel reported separately). Per-voxel granularity is a
follow-on if the simpler binary classification proves insufficient.

## Consequences

- `LayerResult.cure_depth_um` and `LayerResult.worst_cure_depth_um`
  become `pub(crate)` caches; external access via the three dispatch
  methods on LayerResult.
- `PrintSimulation` aggregate gains
  `cure_field: Option<CureField>` and
  `photoinitiator_field: Option<PhotoinitiatorField>`. New invariant:
  if `cure_field.is_some()`, its bbox must contain every layer's solid
  region.
- `FailurePredictor::predict_layer` signature extends with two
  `Option<&mut>` args (additive — existing callers pass `None` and see
  no behaviour change).
- `ResinProfile` gains `photoinitiator_concentration_initial: f32`
  (default per KB-160) and `photoinitiator_decay_constant_k_d: Option<f32>`
  (None ⇒ KB-160 default with loud warn, mirroring the
  `cure_kinetics_ea_kj_mol` precedent).
- `PrinterProfile` gains `voxel_cure_resolution_mm: Option<f32>`.
- `sim.json` schema_version is bumped per ADR-0015 to accommodate the
  optional `cure_field` block. Per the legacy-compat scope cut above,
  no backwards-compat parse test is shipped.
- Cargo feature `field-sim` is added to `resinsim-core` and forwarded
  through `resinsim-inspect` and `resinsim-viz`. Default builds remain
  lean; `cargo nextest run --workspace --features field-sim` exercises
  the voxel path. The 4-config CI matrix lives in
  `agent-constraints/implementation-conventions.md`.

## References

- KB-103 — Beer-Lambert cure depth (Tier-1 single-column primitive).
- KB-120 — LCD uniformity variation (drives `worst_cure_depth_um`).
- KB-141 — viscosity Arrhenius (single-source helper pattern).
- KB-153 — cure-kinetics Ec(T) Arrhenius (single-source helper, reused
  unchanged by t2f1).
- **KB-160** (this issue) — photoinitiator depletion model + uncertainty
  disclosure.
- ADR-0005 — three-axis printer/resin/recipe split (printer-axis fields
  added here).
- ADR-0007 — two-stage LED+vat thermal coupling; explicit Tier-2 CFD
  alternative cited as deferred; this ADR fills the lumped-kinetics
  intermediate.
- ADR-0010 — `resinsim-viz` presentation layer (heatmap consumer).
- ADR-0011 — egui control panels (UniformityCalculator integration
  point).
- ADR-0015 — sim.json canonical interchange (schema_version bump).
- `docs/patterns/single-source-arrhenius-helper.md` — VoxelCureCalculator
  delegates to `CureCalculator::ec_at_temp`; does not re-derive.
- `docs/patterns/nan-two-layer-defence.md` — voxel inputs honour the
  same two-layer NaN policy.
- `projects/000-global/research/resinsim-physics-simulation-plan.md` —
  Phase 4 Tier-2 sub-list, where t2f1 is step 1.
