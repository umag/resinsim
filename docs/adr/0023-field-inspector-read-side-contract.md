---
issue: t2f6-field-inspector
date: 2026-07-28
---

# ADR-0023: Field inspector read-side contract

## Status

Accepted (Phase 4 of issue `t2f6-field-inspector`, 2026-07-28; gate
decisions on `--cured-only` and the decode-budget auto-extension made
by Mag the same day).

## Context

`t2f1`-`t2f4` (ADR-0017, ADR-0018, ADR-0020) shipped four Tier-2 voxel
solvers — cure, photoinitiator, strain, stress, thermal — persisted
losslessly via the ADR-0019 binary sidecar. Until this issue, the only
observation surfaces over that voxel data were per-layer scalar
aggregates (`report health`, the viz heatmap); there was no CLI/JSON
path to an individual voxel value. ADR-0019 names `t2f6-field-inspector`
four times as the consumer its per-layer zstd slab layout was designed
to unblock.

`resinsim inspect field` is a **read-side adapter over an existing
aggregate**. It introduces no new aggregate, no new invariant, and no
write path — it loads a persisted `<stem>.sim.json` + `<stem>.fields.bin`
pair via `repositories::load_envelope_with_budget` and **never re-runs
a solver**. Re-running would cost minutes per query and destroy the
moldable-dev time-to-answer the sidecar exists to provide.

Five decisions here outlive the code and are recorded together because
they interact: the `--slice` addressing split, the feature-off UX
divergence, the dual-scope statistics policy, the descriptor-driven
decode-budget auto-extension, and the `PhotoinitiatorField`
coordinate-ownership asymmetry.

## Decision

### 1. Sidecar-load-only; never re-run the solver

`inspect field` reaches the aggregate only through
`repositories::load_envelope_with_budget(path, ceiling_bytes)`
(`load_envelope` delegates with the 4 GB default — no behaviour
change for existing callers). No new persistence code, no new parsing
of untrusted bytes: the hardened sidecar decoder (path-traversal +
symlink-escape guards, sha256 verification, bounded per-slab zstd,
allocation caps) is reused verbatim.

**Rejected alternative:** re-running the simulation per query. This is
exactly the cost ADR-0019's sidecar was built to eliminate; re-running
would take minutes on a real print versus milliseconds for a sidecar
read.

### 2. `--slice <AXIS>=<VALUE>[mm]` — two Z-index semantics behind one spelling

`FieldSlicer::resolve_index` (`crates/resinsim-core/src/services/field_slicer.rs`)
is the single place the two semantics diverge:

- **Layer-stacked fields** (cure / photoinitiator / strain / stress):
  Z resolves through **cumulative per-layer CTB heights**
  (`PrintSimulation::layer_height_provenance()`'s `LayerHeightSeq`),
  never `iz * voxel_size_mm`
  (`docs/patterns/anti/voxel-z-step-from-lateral-voxel-size.md`).
  Absent provenance (an STL / area-only run — no CTB-derived per-layer
  heights exist) returns a typed `MissingLayerHeightProvenance` error;
  the caller must address the field by voxel index instead. In
  practice every REAL sim.json carrying voxel fields today has
  provenance (voxel mode currently requires CTB input, which always
  carries per-layer heights) — this branch defends the future STL-voxel
  path (t2f5-adjacent scope, not yet wired), not a case that fires on
  any run producible today.
- **`ThermalField`**: Z resolves as `(z_mm - bbox_min_z) / voxel_size_mm`
  over the **vat envelope** (`docs/patterns/thermal-field-z-dim-is-spatial.md`)
  — a different bbox AND a different voxel size from the other four
  fields. `FieldSlicer::resolve_index` NEVER calls
  `world_at_voxel_center()` for this — that helper is documented as
  intentionally wrong for physical Z on the layer-stacked fields, and
  using it for thermal would silently mix the two coordinate systems.
- X and Y resolve through `bbox_min_mm + i * voxel_size_mm` identically
  for all five field kinds.

**Regression guard.** The mandatory test
`resolve_index_z_resolutions_differ_between_cure_and_thermal_for_the_same_z_mm`
feeds the SAME `z_mm` into both branches against a cure field with
NON-UNIFORM layer heights (50/30/20 µm) and asserts the resolved
indices differ (`assert_ne!`) — a copy-paste collapse of the two
branches would otherwise pass silently. The CLI integration suite
(`field_inspect_cli.rs`) proves the same property end-to-end through
the real subprocess: `z=0.09mm` resolves to cure index 2 (cumulative
heights) and thermal index 0 (`floor(0.09/0.5)`).

**`Z_BOUNDARY_EPSILON_MM` (1e-4 mm / 0.1 µm).** Cumulative µm→mm
summation over many layers accrues f32 rounding error — four layers
summing to a nominal 0.14 mm actually produced `0.139999...` in
testing. Without a small tolerance on the layer-stack upper-bound
check, a query at the EXACT nominal top-of-stack value would spuriously
reject as out-of-range. 0.1 µm is roughly two orders of magnitude below
any physical layer thickness (tens of µm), so it cannot mask a
genuinely out-of-range query while comfortably absorbing float
summation noise across realistic layer counts (thousands, not millions).

**Exact-boundary snapping.** A coordinate at the exact upper edge of a
field's extent (lateral or layer-stack) snaps to the last voxel/layer
rather than erroring, mirroring `ThermalField::temperature_at_world`'s
existing extreme-face convention.

### 3. Feature-off: visible subcommand, actionable exit-2 error — diverges from ADR-0017 §5

ADR-0017 §5's `--voxel-cure-mm` precedent relies on clap's bare
unknown-flag rejection when the `field-sim` feature is off: the flag
simply isn't registered, so passing it produces a generic "unrecognized
argument" error. `inspect field` deliberately does NOT follow that
precedent.

Instead: the `field` subcommand and ALL of its flags stay in the clap
tree in every build (`--help` lists it unconditionally), and ONLY the
handler BODY is `#[cfg(feature = "field-sim")]`-split. Feature-off, the
handler prints `the "field" inspector requires the "field-sim" Cargo
feature; rebuild with "cargo build --features resinsim-inspect/field-sim"`
to stderr and exits 2.

**Why the divergence.** `--voxel-cure-mm` is one flag on an
already-discoverable subcommand (`sim`) that works in every build —
losing that one flag feature-off is a minor capability reduction. Here
the WHOLE subcommand is unavailable feature-off; a bare unknown-flag
error for `inspect field` would silently look like the subcommand
doesn't exist at all, a discoverability dead end for a user who hasn't
read the docs. Naming the feature and the exact rebuild command turns
a dead end into one copy-pasteable next step.

**Constraint.** The feature-off handler body must not name any
ndarray-dependent type — `FieldKindArg`, `SliceAxisArg`, `SliceSpec`,
and `parse_slice_spec` are plain data/functions with zero `field-sim`
dependency specifically so the flags themselves compile and validate
in every build (config 1 and config 3 of the ADR-0017 four-config
matrix both exercise this).

### 4. Error behaviour under `--json`: sibling convention, no JSON error envelope

Every error path — envelope load failure, missing voxel field,
out-of-range slice address, feature-off — writes prose to stderr,
leaves stdout EMPTY, and exits nonzero. This is IDENTICAL whether
`--json` was passed or not; `inspect field` never emits a JSON error
envelope (`{"error": "..."}` or similar). This matches every other
`resinsim inspect` / `report` subcommand's existing convention and
avoids downstream tooling having to branch on "is this JSON stdout an
error or a result" — the exit code and stdout emptiness already answer
that.

Exit codes: 1 for envelope-load and missing-voxel-field errors
(data-availability failures, matching `report health --in`'s existing
convention); 2 for out-of-range `--slice` addressing and feature-off
(bad-input / missing-capability, matching the domain's typed
`FieldSlicerError` and the `--voxel-cure-mm` parse-error convention).

### 5. Dual-scope statistics: `--cured-only` selects `FieldStatsScope::Nonzero`; both counts always disclosed

Zero is overloaded across the five fields: in `StrainField`/`StressField`
zero means "uncured liquid or outside the part bbox"
(`StressField::yield_fraction`'s doc-comment); in `CureField` zero is
genuine undercure and the most interesting value there is. `min`/`mean`/
`p95` over a mostly-empty slab is dominated by sentinel zeros.

`FieldStats` (`crates/resinsim-core/src/values/field_slice.rs`) is ONE
type carrying a `scope: FieldStatsScope` marker (`All | Nonzero`) —
`--cured-only` selects `Nonzero` rather than forking a second struct.
`count` is the population size for the active scope; `nonzero_count`
is ALWAYS the raw nonzero cardinality regardless of scope, so a JSON
consumer never has to infer one count from the other. The JSON payload
carries an explicit `stats_scope: "all" | "nonzero"` field, pinned in
the output-shape golden
(`crates/resinsim-inspect/tests/fixtures/field_inspect/cure_xy_z0.json.golden`)
so the schema addition cannot silently drift. `--cured-only`'s
`--help` text states the semantics concretely: "statistics over
nonzero voxels only; total and nonzero counts are always shown either
way."

An all-zero slice under `--cured-only` returns a typed `EmptyScope`
error rather than NaN
(`docs/patterns/anti/magic-floor-vs-honest-filter.md`) — an honest
failure, not a silently-filtered placeholder.

### 6. Descriptor-driven decode-budget auto-extension, ceiling 24 GB

Gate decision 2026-07-28: real sidecars must open with no env-var
fiddling — the 4.81 GB lilith-torso `StrainField` is the concrete
motivating case. `simulation_repo::load_envelope_with_budget(path,
ceiling_bytes)` auto-extends the in-memory decode budget for the
DURATION OF ONE LOAD CALL when a sidecar's descriptor genuinely needs
more than the 4 GB default, up to `ceiling_bytes`
(`values::FIELD_BUDGET_CEILING_BYTES` = 24 GB, matching the 18 GB peak
precedent already accepted in this repo per
`feedback_memory_tradeoffs.md`, with headroom). `load_envelope`
delegates with the 4 GB default itself as the ceiling — i.e. no
extension headroom, so its behaviour is byte-for-byte unchanged.

**Ordering — the bomb-guard posture holds.** Extension happens
STRICTLY AFTER sha256 integrity verification of the sidecar bytes.
`load_and_install_sidecar_with_budget` reads the sidecar once, verifies
its sha256 against the envelope's recorded pointer, and ONLY THEN calls
the new `sidecar::peek_max_field_bytes` (a header-and-descriptor-only
pass, no slab reads, no budget enforcement) against those VERIFIED
bytes to learn the largest single field's implied allocation. The
untrusted `nx`/`ny`/`nz` descriptor fields are never used to size a
real allocation before their containing bytes are proven to match the
hash the producer recorded. Three invariants hold unchanged from
ADR-0019: (a) sha256-before-decode ordering; (b) the per-slab bounded-
decompression guard in `sidecar::decoder` (`zstd::decode_all` remains
banned); (c) a hard ceiling — a descriptor requiring more than 24 GB is
rejected exactly as before, via the same `DecodeError::ExceedsFieldBudget`
path (whose message now also names `RESINSIM_MAX_FIELD_BYTES`
explicitly, not just the internal constant, so an above-ceiling
rejection stays self-help).

**`RESINSIM_MAX_FIELD_BYTES` overrides in BOTH directions.** If the
caller's environment already sets the override, `load_envelope_with_budget`
makes NO adjustment of its own — decode proceeds with whatever
`active_budget_bytes()` naturally resolves to, even when that value is
SMALLER than the descriptor's requirement and smaller than the ceiling
would have allowed. Auto-extension only engages when the env var is
unset. A one-line stderr note is emitted whenever extension actually
happens, naming the default, the extended value, the ceiling, and the
override variable.

**Rejected alternative:** always decode with the 24 GB ceiling active.
This would authorize allocations up to 24 GB for EVERY load regardless
of actual need, silently loosening the guard for small files too and
producing misleading "extending to 24 GB" noise on trivial sidecars.
The descriptor-driven `min(required, ceiling)` computation keeps the
extension honest and proportionate.

### 7. `PhotoinitiatorField` coordinate-ownership asymmetry

`PhotoinitiatorField` — verified by reading
`crates/resinsim-core/src/values/photoinitiator_field.rs` in full —
carries NEITHER `voxel_size_mm()` nor `bbox_min_mm()`. Unlike the other
four fields, it stores no coordinate metadata of its own; it is
dimension-locked to its companion `CureField` (both are always
installed together via `PrintSimulation::set_voxel_fields`) but never
carried the bbox/voxel-size fields that would let it answer world-mm
queries independently. This is a genuine gap against the domain's
stated intent ("Companion to CureField: same dimensions and
bbox-anchored convention" — the type's own module doc) that this issue
surfaced rather than introduced.

**Decision: source coordinates from the paired `CureField`, not from
`PhotoinitiatorField` itself.** `FieldSlicer::slice` and
`FieldSlicer::resolve_index` take `voxel_size_mm` / `bbox_min_mm` as
explicit caller-supplied parameters (uniformly, for all five field
kinds — not just photoinitiator) rather than deriving them internally
from `FieldRef`. The CLI adapter (`field_inspect::run`) sources these
from the field itself for cure/strain/stress/thermal, and from the
paired `CureField` specifically for photoinitiator — `sim.cure_field()`
is guaranteed `Some` whenever `sim.photoinitiator_field()` is, by the
`set_voxel_fields` pairing invariant; an absent pairing is itself
folded into the "no voxel field" error.

**Rejected alternative:** widen `PhotoinitiatorField`'s persisted shape
to add `voxel_size_mm`/`bbox_min_mm` fields directly. This would be a
real sidecar/persistence-format change (new descriptor fields, decoder
changes, a version bump under the same "don't care about legacy"
policy that bumped `RSFIELD_FORMAT_VERSION` 1→2 for the thermal kind
in ADR-0020) for data the type never needed to own, given the
companion field already carries it. Out of scope for a read-side
inspector; deferred indefinitely unless a future consumer needs
`PhotoinitiatorField` addressable WITHOUT its paired `CureField` in
scope.

## Consequences

- `resinsim inspect field --in <sim.json> --field <cure|photoinitiator|strain|stress|thermal> --slice <AXIS>=<VALUE>[mm] [--bins N] [--values] [--cured-only] [--json]`
  is a new, permanently visible subcommand across every build
  configuration.
- The ADR-0017 four-config build/test matrix gains a fifth dimension
  of coverage specifically for this subcommand: configs 1/3 (default)
  prove the visible-subcommand + actionable-error contract; configs
  2/4 (`field-sim`) prove the real slice/stats/histogram output.
- `load_envelope`'s public contract is technically widened
  (`load_envelope_with_budget` is new, additive, public) but its OWN
  behaviour is unchanged byte-for-byte — verified by the full existing
  `resinsim-core` suite passing unmodified alongside the new tests.
- A future STL-voxel path (t2f5-adjacent) will hit
  `MissingLayerHeightProvenance` on `--slice z=<mm>` addressing until
  it either produces its own `LayerHeightProvenance` or this ADR is
  revisited to add index-based `--slice` addressing to the CLI (the
  domain service already supports raw-index `FieldSlicer::slice`; only
  the CLI surface is mm-only today).
- `PhotoinitiatorField`'s coordinate gap remains load-bearing for any
  FUTURE consumer that wants to address it independently of a
  `CureField` in scope; such a consumer will need to either accept the
  same paired-lookup pattern this issue established, or revisit the
  "reject widening" call above.

## Rejected alternatives (summary)

- Re-running the solver per query (defeats ADR-0019's entire
  rationale).
- `--stl` input support for `inspect field` (voxel mode is CTB/NanoDLP-
  only today; wiring STL voxel support is t2f5-adjacent, not this
  issue — the issue's originally-filed acceptance criterion used
  `--stl` and was corrected to `--in <sim.json>` during triage).
- Per-layer partial reads via the sidecar's `layer_offsets[]` (would
  only accelerate the XY-slice case, since XZ/YZ slices need every
  layer decoded regardless; deferred as a follow-up, not required for
  v1 correctness).
- Always-24GB decode budget (see Decision 6).
- Widening `PhotoinitiatorField`'s persisted shape (see Decision 7).
- A second `FieldStats`-like struct for `--cured-only` (see Decision 5).

## References

- ADR-0017 §5 — the `--voxel-cure-mm` feature-off precedent this issue
  diverges from (Decision 3).
- ADR-0019 — sidecar persistence; names `t2f6-field-inspector` as the
  consumer the per-layer slab layout was designed to unblock.
- ADR-0020 §Decision x — `ThermalField`'s spatial-Z / vat-envelope
  anchoring, the second half of Decision 2's split.
- `docs/patterns/anti/voxel-z-step-from-lateral-voxel-size.md` — the
  anti-pattern Decision 2's layer-stacked branch avoids.
- `docs/patterns/thermal-field-z-dim-is-spatial.md` — the contrasting
  convention Decision 2's thermal branch follows.
- `docs/patterns/anti/magic-floor-vs-honest-filter.md` — Decision 5's
  empty-scope-is-an-error rationale.
- `docs/patterns/anti/serde-json-non-finite-f32-null-coercion.md` —
  the defensive `finite_or_null` guard on JSON-rendered statistics.
- `docs/patterns/honest-zero-with-model-gap-caveat.md` — the KB-162
  free-shrinkage caveat echoed on stress-field output.
- `feedback_memory_tradeoffs.md` (user memory) — the 18 GB peak-RAM
  precedent Decision 6's 24 GB ceiling sits above.
