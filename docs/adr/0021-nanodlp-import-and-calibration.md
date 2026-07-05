---
issue: nanodlp-import
date: 2026-07-05
---

# ADR-0021: NanoDLP import + Athena analytic ingest + calibration

## Status
Accepted

## Context

The Athena II runs NanoDLP, which exports a `.nanodlp` job: a ZIP of per-layer
slice PNGs (`{n}.png`, 1-indexed) plus JSON metadata (`meta`, `profile`,
`slicer`, `plate`, `info`) and one or more gzipped **analytic logs**
(`analytic-*.csv.gz`) — the real force-sensor recording of an actual print. We
have real Athena data and want resinsim to (a) simulate a real NanoDLP job and
(b) validate/calibrate the peel-force model against the recorded force.

Two format facts shaped the design:

1. The analytic CSV is a **tall** stream `ID,T,V` (ns-epoch, channel code
   0–17, value), not a wide per-layer table. `T=6` is the FSS load-cell
   pressure (peel signal); `T=0` layer height, `T=4` speed, `T=5` cure time,
   `T=7/8` resin/ambient temp, `T=9` layer time, `T=10` lift height. Channel
   map decoded from `mikeporterdev/nanodlp-analyzer`.
2. The pre-existing `io/athena.rs` `ForceRecord` assumed a wide schema that
   cannot parse this real export; it had a single internal consumer
   (the CLI `athena` command) and no released contract.

## Decision

**Placement (ADR-0009).** The `.nanodlp` reader is I/O → `io/nanodlp.rs`,
mirroring `io/ctb.rs` (`parse_nanodlp → (SlicedFileInfo, Vec<LayerInput>)`).
Segmentation, comparison and calibration are domain logic → `services/`
(`force_series_extractor`, `force_comparator`, `profile_calibrator`), never in
`io/`.

**Single dispatch.** All sliced-file entry points route through the new
`sliced::parse_sliced(path)` (CTB + NANODLP), so a format is wired in once:
`SimulationRunner`, `build_simulation_from_path`, and both CLI dispatch sites
call it instead of `parse_ctb` directly.

**Layer cross-section (ADR-0005).** Each layer's area is decoded from its
slice PNG (native-pixel count × pixel area) and downsampled to a `LayerMask`
voxel grid, exactly like the CTB path. `Recipe` is mapped from `profile.json`
(bottom = `SupportLayerNumber`, normal/bottom exposure = `CureTime`/
`SupportCureTime`, lift/retract = `LiftSpeed`/`RetractSpeed`); `Depth`/
`Thickness` give layer height. `DynamicSpeed` JS expressions are out of scope
(static speeds in v1).

**ForceRecord removed.** Replaced by `AnalyticLog` (tall, real). No released
consumer depended on the wide schema, so it is deleted rather than shimmed
(right-size backward compat).

**Units + sign.** `T=6` is signed raw load-cell counts (peel reads negative,
~−340). `peel_signal(raw) = -raw` makes peel positive but stays in counts.
Simulated peel force is Newtons, so `ForceComparator` compares in a
**normalized** (min-max) space; `ProfileCalibrator` least-squares-fits the
counts→Newton gain and reads `delta_t_steady_c` from `T=7 − T=8`. Calibration
output is **suggested overrides with a fit-quality R²**, never a silent
rewrite of `athena_ii.toml` — a single print is a weak calibration sample.

**Untrusted-archive bounds.** A `.nanodlp` is arbitrary user input. Concrete,
fail-closed limits (in `io/nanodlp.rs` / `io/athena.rs`):

| Bound | Const | Value |
|-------|-------|-------|
| ZIP entry count | `MAX_ENTRIES` | 100 000 |
| Per-entry JSON size | `MAX_JSON_BYTES` | 64 MiB |
| Decoded PNG pixel count (checked from IHDR before allocation) | `MAX_PNG_PIXELS` | 64 000 000 |
| Decompressed analytic bytes | `MAX_ANALYTIC_DECOMPRESSED` | 512 MiB |

PNG dimensions are read from the header and rejected before any pixel buffer
is allocated (dimension-bomb guard); gzip/zip reads are size-capped. Entries
are read **by name into memory** (`meta.json`, `{n}.png`, `analytic-*.csv.gz`)
and never extracted to a filesystem path, so zip-slip path traversal is not
applicable — there is no extraction step to escape.

## Consequences

- resinsim simulates real Athena jobs (`resinsim sim --file job.nanodlp`) and
  reconciles them against recorded force (`resinsim inspect calibrate --file
  job.nanodlp`). Verified against a real 1499-layer / 11520×5120 / ~37 MB
  export: bed 218.9×122.9 mm, peak cross-section 1427 mm² at the base.
- **Performance:** decoding 1499 × ~59 MP PNGs is single-threaded and takes
  ~110 s (release). Acceptable for a one-off analysis; rayon-parallel decode is
  a documented follow-up (memory budget preference: simple v1 first).
- Layer segmentation of the analytic log is heuristic: it splits on `T=0`
  layer-height markers. A print without those markers yields no segmented
  layers (callers check `len()`); a `T=9` layer-time fallback for such logs is
  a documented follow-up. Mis-segmentation would skew per-layer force.
- `athena_ii.toml` envelope stays estimate-based; calibration now provides
  data-driven *suggestions* to tighten it (feeds `spec/EXPERIMENT-PLAN-v1.1`).
