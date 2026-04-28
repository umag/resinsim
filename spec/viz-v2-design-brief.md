---
status: draft (start-of-exploration)
date: 2026-04-28
author: Mag (i@aopab.art)
shape_command: /impeccable shape resinsim-viz redesign
related:
  - PRODUCT.md
  - DESIGN.md
  - docs/adr/0014-bevy-egui-retained-for-viewer-redesign.md (planned)
---

# Design Brief: resinsim-viz v2 (Viewer Dashboard)

This brief is the output of a `/impeccable shape` run on 2026-04-28. It is iteration-zero: production-ready in fidelity (concrete enough to hand to implementation), but explicitly start-of-exploration in time intent. Expect refinement as `bevy_egui` reveals where it fights this layout.

## 1. Feature Summary

resinsim-viz v2 is a read-only Grafana-style dashboard for `resinsim-core` simulation outputs. It accepts one or more `sim.json` files (with optional matching CTB slice stacks for geometry inspection) and surfaces every layer's physics as a single dense screen of time-series, per-layer stats, and inline failure annotations. The runner leaves the binary: simulation production moves to a separate `resinsim-run` CLI, and this viewer's only job is reading and visualising.

## 2. Primary User Action

A DragonFruit slicer developer drops a `sim.json` (or two, for before-and-after comparison) onto the window and, within seconds, answers: *is this print job safe, and if not, which layer breaks first and why?* They scrub through layers via a single shared time axis, every metric moves together, and a click on any failure in the rail jumps the cursor straight to the offending layer.

## 3. Design Direction

- **Color strategy.** Full palette per `DESIGN.md`. Grafana-classic categorical series for plot lines (each loaded run gets its own series colour, each metric within a plot also keyed by colour), viridis for continuous magnitudes (cure-depth heatmaps, force overlays), threshold red and amber used exclusively for crossed or approaching fail thresholds, cool blue-grey tinted neutrals for chrome.
- **Theme scene sentence.** *"A slicer developer in a dimly-lit workshop late evening, second monitor beside a printer with an open vat (overhead light kept low to avoid stray UV cure), comparing today's slicer change against yesterday's baseline over a long focus session."* That forces dark default. A light variant ships behind a setting for daytime sessions; the token system carries `surface-base-dark` primary and `surface-base-light` secondary from day one, never as a re-skin.
- **Anchor references.** Grafana (panel grid, threshold rules, time-cursor sync), Bloomberg terminal (tabular numeric density, no decorative chrome), Honeycomb (calm-at-rest, loud-at-faults).
- *(Visual direction probes skipped: the harness lacks native image generation. Brief proceeds in prose.)*

## 4. Scope

| | |
|---|---|
| Fidelity | Production-ready (concrete enough to hand to craft) |
| Breadth | Dashboard only. File-open, multi-run picker, settings, report-export are out of scope for this brief |
| Interactivity | Shipped-quality interactive component |
| Time intent | Start of exploration. Expect the brief to refine as toolkit tradeoffs surface |

## 5. Layout Strategy

Three regions on a single surface.

```
┌────────────────────────────────────────────────────────────────────────────┐
│ Summary strip (~48px)                                                       │
│ Run-tag chips · total time · bottom/transition/normal split ·               │
│ max-force layer · min-safety layer                                          │
├──────────┬─────────────────────────────────────────────┬───────────────────┤
│          │                                             │                   │
│          │  Pane grid (resizable, Grafana-style)       │  Per-layer stat   │
│          │  ┌──────────────┐ ┌──────────────┐          │  readout          │
│ Failures │  │ Forces       │ │ Safety       │          │  (right rail,     │
│ rail     │  └──────────────┘ └──────────────┘          │  ~280px,          │
│ (left,   │  ┌──────────────┐ ┌──────────────┐          │  collapsible,     │
│ ~240px)  │  │ Cure depth   │ │ Vat temp     │          │  mono-tabular)    │
│          │  └──────────────┘ └──────────────┘          │                   │
│          │  ┌──────────────┐ ┌──────────────┐          │  Every LayerResult│
│          │  │ Area + Δarea │ │ Viscosity    │          │  field for cursor │
│          │  └──────────────┘ └──────────────┘          │  layer            │
│          │  ┌──────────────┐ ┌──────────────┐          │                   │
│          │  │ Z deflection │ │ Layer mask 2D│          │                   │
│          │  └──────────────┘ └──────────────┘          │                   │
├──────────┴─────────────────────────────────────────────┴───────────────────┤
│ Layer scrubber (full-width, ~64px). Failure ticks rendered inline.          │
└────────────────────────────────────────────────────────────────────────────┘
```

3D summon-on-demand pop-out: floating window summoned from the layer-mask pane header, `bevy_panorbit_camera` plus viridis heatmap, closes back to the dashboard.

Hierarchy:

- **Scrubber** is the gravitational centre. Every plot's x-axis is layer index. The scrubber owns the single shared cursor; the right-rail readout updates with it.
- **Summary strip** answers "is this run worth opening" in one glance. `max-force layer 3247` and `min-safety layer 412` are click-to-jump chips.
- **Failures rail** is the triage path. List of every `FailureEvent`, sorted by layer, click jumps the cursor. Severity carried by a trailing red or amber dot, never by row background colour.
- **Pane grid** is the data. 2-column 5-row default; every pane resizable and reorderable. The 10 panes are: Forces (peel + suction + total + support_capacity threshold line), Safety factor (single trace + threshold at 1.0), Cure depth (vs effective_layer_height threshold), Vat temperature, Cross-section area + Δarea, Viscosity, Z deflection, Layer mask 2D slice, plus two reserved slots for user-added panes from the field set.

Rhythm: 8px base, 16px between panes, 24px around regions. Section headers semibold inside each pane; never display-sized type. Tabular mono numerics in every readout column; sans for axis units, legend keys, button labels.

## 6. Key States

| State | What appears |
|---|---|
| **No run loaded** | Drop affordance over pane grid: `Drop a .sim.json here, or pass --load-sim on launch.` Plot panes show empty axes (grid only). Failures rail: `(no run loaded)`. Summary strip: dashes. |
| **One run, no failures** | Traces in series-1 colour. Failures rail: `(0 failures, 0 warnings)`. Summary filled. No red anywhere. |
| **One run, with failures** | Threshold lines visible, failures annotated inline as red or amber ticks at the offending layer. Failures rail: layer · type · severity dot · message. Scrubber: red ticks at failure layers. |
| **Two+ runs overlaid** | Each run gets a categorical series colour. Multiple traces per pane. Run-tags become a chip row in the summary strip; hover highlights matching traces, click toggles visibility. Threshold lines remain shared (they belong to the recipe, not the run). |
| **Run with no matching CTB** | Layer-mask pane: `(no CTB loaded, geometry unavailable)`. 3D summon button disabled. All data panes work. |
| **Loading / parsing** | Pane grid shows skeleton rectangles, no spinners. Summary strip shows `parsing…` in muted ink. ≤500ms typical for 4500 layers; if it overruns, a determinate bar appears in the strip. Never a full-screen modal. |
| **Parse error** | Pane grid replaced by an ink-muted block: error class, path, line and column, first 80 chars of offending JSON verbatim. No emoji, no "oops". Drag a fresh file to retry. |
| **Layer-count mismatch (sim vs CTB)** | Inline muted error inside the layer-mask pane. Data panes unaffected. |
| **Cursor on failure layer** | Failures rail row at cursor highlights (tonal step up, no colour). Plot vertical cursor picks up the threshold colour. |
| **Field absent (schema older than viewer)** | Pane stays in the grid with axis but no trace, plus muted note `(field missing in sim.json schema vN)`. Never silently disappear. |

## 7. Interaction Model

- **Drop file.** Drag `.sim.json` onto any part of the window. Loaded as primary run. Drag a second `.sim.json` for comparison. Drag a `.ctb`, paired by filename stem.
- **Layer scrub.** Click on scrubber jumps cursor; drag follows continuously. `↑` / `↓` step ±1; `Shift+↑/↓` step ±10; `Home` / `End` first / last; `PgUp` / `PgDn` ±100.
- **Failure click.** Row in failures rail jumps cursor.
- **Summary jump.** `max-force layer 3247` chip jumps cursor.
- **Pane resize / reorder.** Drag pane edges or pane headers.
- **3D summon.** Layer-mask pane header button. Floating window with `bevy_panorbit_camera` over the slice stack, cursor plane visible. Closes back to dashboard.
- **Run-tag toggle.** Chip in summary strip. Click hides that run's traces; hover dims the others.
- **Hover plot point.** Tooltip with mono-tabular `layer N · field: value`. Never animates.
- **Threshold visibility.** 1px chip beside the legend entry, click toggles the threshold line.

No motion on cursor moves, scrubs, or scrolls. Pane-resize follows the pointer without interpolation. State changes are instantaneous per `DESIGN.md`.

## 8. Content Requirements

Microcopy:

- Drop affordance: `Drop a .sim.json here, or pass --load-sim on launch.`
- No-CTB pane note: `(no CTB loaded, geometry unavailable)`
- Schema-missing field: `(field missing in sim.json schema vN)`
- Empty failures rail: `(0 failures, 0 warnings)`
- Parse error: `Parse error · {path}` then JSON line excerpt verbatim.
- Run-tag default: filename stem of the sim.json. User can rename inline.

Dynamic ranges:

- Layer count: 1500 to 6000 typical for SLA prints.
- Peel force: 0 to roughly 80 N typical; suction usually sub-1 N.
- Vat temperature: 18 to 35 °C.
- Failures: 0 (healthy) to dozens (broken slice).
- Concurrent runs: typically 1 to 3 overlaid; plan for up to about 6 before the categorical palette runs out.

Number formatting (mono, tabular figures):

- Force: `12.34 N` (2 decimals, right-aligned).
- Cure depth: `142.3 µm`.
- Temperature: `27.4 °C`.
- Time: `1h 23m 47s` in summary; `1234.5 s` in plot tooltips (units consistent within a pane).
- Layer index: zero-padded to total width: `0042 of 4500`.

## 9. Recommended References

- `reference/spatial-design.md` (if present in the impeccable kit) for the pane grid topology, scrubber and summary anchoring, density rhythm.
- `reference/interaction-design.md` for drop flow, scrubber keyboard model, run-toggle chips.
- `reference/data-viz.md` (if present) for plot defaults, threshold annotations, multi-run series mapping.
- `reference/clarify.md` to lock §8 microcopy.
- `reference/harden.md` for the pre-ship pass over schema drift, layer-count mismatch, and missing-field tolerance.

## 10. Open Questions

Resolved during craft, not blocking the brief.

1. **GUI toolkit.** *Resolved:* stay on `bevy_egui` (see ADR-0014). The pane grid and stats table will need real custom egui work (likely a layout manager built atop `egui::Resize` plus a tabular-numerics table widget). The 3D popout stays cheap because Bevy and `bevy_panorbit_camera` are already wired.
2. **Layout persistence.** Pane reorder, resize, and run-toggle state lives where: per-project (alongside the sim.json), per-user (`~/.config/resinsim/`), or session-only?
3. **CTB pairing convention.** Filename-stem match is the default. Manual pair affordance needed for the case where names diverge.
4. **sim.json schema versioning.** The viewer outlives any single core schema. Needs an explicit `schema_version` field plus an "unknown field, surface as muted pane note" tolerance pattern.
5. **Multi-run alignment.** Two runs of the same model with different layer heights have different layer counts. Align by layer index (default for same-recipe slicer-change comparison), by Z height in mm (default for cross-recipe comparison), or by cumulative time? Toggle in the summary strip.
6. **Dark theme ship target.** *Resolved:* dark is primary. The DESIGN.json sidecar on the next `document` pass needs both `surface-base-dark` and `surface-base-light` token families from day one.
7. **Runner separation timing.** Does `resinsim-run` extract before this redesign ships (clean cut), or does the redesign land first and the runner extraction follow? Affects whether the Run button leaves immediately or in a follow-up.

## Confirmation

Confirmed by Mag on 2026-04-28. Locked at:

- Architecture: viewer-only, multi-run.
- Toolkit: `bevy_egui` retained (ADR-0014).
- Theme: dark by default, light secondary.
- v1 panes: 10 (Forces, Safety, Cure depth, Vat temp, Area + Δarea, Viscosity, Z deflection, Layer mask 2D) plus failures rail, summary strip, per-layer stat readout, 3D summon-on-demand.

Next concrete step: spike the layer-stats mono-tabular table widget and a 2-pane resizable grid harness in `crates/resinsim-viz/examples/viz_v2_spike.rs` to validate that `bevy_egui` carries the density before the wider redesign branches.
