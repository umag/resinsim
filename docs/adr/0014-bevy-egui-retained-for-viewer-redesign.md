---
issue: viz-v2-redesign (no issue number assigned yet)
date: 2026-04-28
---

# ADR-0014: bevy_egui retained as the GUI toolkit for the resinsim-viz v2 redesign

## Status

Accepted (shape command 2026-04-28; spike cleared 2026-04-28).

The spike at `crates/resinsim-viz/examples/viz_v2_spike.rs` validated all three acceptance gates: the 2x2 resizable pane grid resizes live without flicker, the mono-tabular stats column does not jiggle horizontally as values change, and four `egui_plot` panes with `link_axis` + `link_cursor` keep a shared X-axis cursor across panes while preserving independent Y-domains per pane. `bevy_egui` carries the layout. Redesign branches may now start.

## Context

The v2 redesign of resinsim-viz, captured in
`spec/viz-v2-design-brief.md`, replaces the current left-controls /
right-plots layout with a Grafana-style dashboard: a 10-pane
resizable grid, a left failures rail, a right per-layer stat
readout, a top summary strip, and a bottom layer scrubber. The
shape interview surfaced an honest open question: can `bevy_egui`
0.39 carry that density without descending into custom-widget
reinvention, or does the redesign argue for a richer GUI toolkit
(pure egui without Bevy, iced, Slint, Tauri + web)?

The arguments cut both ways:

- **For `bevy_egui`.** Already wired. ADR-0011 captured the
  current panel anchors and dependency-version chain; the screenshot
  pipeline (ADR-0013), the headlamp camera rig
  (`docs/patterns/camera-headlamp-rig.md`), and the Bevy-resource
  test seam (`docs/patterns/bevy-app-test-seam.md`) all assume
  Bevy. The 3D summon-on-demand popout in the v2 design is trivial
  on Bevy + `bevy_panorbit_camera` and would require non-trivial
  re-implementation on a non-Bevy stack.
- **Against `bevy_egui`.** Grafana-density tabular tables and a
  draggable, resizable, reorderable multi-pane grid are not
  egui's natural shape. egui has `Resize` and the panel system,
  but the v2 design needs persistent per-pane resize state, drag-
  to-reorder, and a dense mono-tabular stats column. Each of those
  is achievable, none is built-in. A web stack (Tauri + Vue or
  Svelte) gives the layout primitives for free at the cost of
  re-implementing 3D-viewport plumbing.

## Decision

**Stay on `bevy_egui`. Validate via spike before committing the
wider redesign.**

The deciding factor is asymmetric reversibility. Going from
`bevy_egui` to a non-Bevy toolkit later is feasible; the viz crate
is small, the data layer is in `resinsim-core`, and the design
brief explicitly contemplates the move. Going from a web stack
back to Bevy after re-implementing 3D plumbing on the web is
expensive enough to feel like a one-way door. Choose the
reversible direction first.

The redesign's risk is concentrated in two custom widgets, both
of which the v1 viz does not yet stress: the resizable multi-pane
grid and the mono-tabular stats column. These are the two things
the spike has to validate.

### Spike contract

Location: `crates/resinsim-viz/examples/viz_v2_spike.rs`. Throwaway
example (not registered in the binary surface). Run with
`cargo run -p resinsim-viz --example viz_v2_spike`.

Spike must demonstrate, in a single Bevy + bevy_egui app:

1. **A 2x2 resizable pane grid** with three draggable splitters
   (one horizontal, two vertical). Drag a splitter, watch the
   pane resize live without flicker or layout pop. Persist
   nothing; this is the harness, not the production widget.
2. **A mono-tabular stats column** rendering every `LayerResult`
   field for a moving cursor layer. The cursor layer auto-
   advances every ~250 ms and is also user-controllable via a
   slider. The acceptance test is column alignment: the value
   column must not jiggle horizontally when values change between
   frames. If digits ever shift right or left, mono-tabular is
   broken (likely a fallback-font swap; mitigate with explicit
   `egui::FontId::monospace` + a registered face that ships
   tabular figures).
3. **The DESIGN.md dark theme.** Cool blue-grey panel surfaces
   (oklch ~20% lightness, hue 240, low chroma), high-contrast
   ink, faint grid lines, no pure-black or pure-white anywhere.
   The spike applies this via `egui::Visuals::dark()` plus a
   small set of `Color32` overrides; production will move that
   into a typed token module on the next pass.

Acceptance is a 5-minute manual exercise: open the spike, drag
every splitter, watch the stats column for 30 s while the cursor
auto-advances, sanity-check the dark theme by eye. If any of (1),
(2), or (3) feels visibly broken, this ADR is revisited; the
fallback path is dropping Bevy and going to pure egui (still
egui's native widget set, but without the Bevy schedule and
render-app overhead).

### Reversibility plan

If, after the redesign branches and the production widget set is
under construction, `bevy_egui` proves to fight the layout
beyond what custom widgets can absorb, the next move is **drop
Bevy, stay on egui**. Pure egui via `eframe` keeps every widget
investment, removes the Bevy schedule and render-app overhead,
and only loses the 3D popout (which would need to move to
`egui_glow` or a separate `wgpu` viewport). That migration is
linear and bounded.

A move beyond egui (iced, Slint, Tauri + web) is reserved for the
case where egui itself is the obstacle, not Bevy. The brief
contains no signal that egui itself is wrong for this product;
the signal is that we are about to push egui harder than v1 has,
and the spike exists to find out where it bends.

## Consequences

### Good

- Reuses every existing pattern: ADR-0010 layering rule, ADR-0011
  panel anchors, ADR-0013 screenshot pipeline, the test-seam
  patterns. None of those need to be reproven on a new stack.
- 3D popout stays trivial.
- Spike is bounded to a single example file; no production code
  is committed before the spike clears.

### Bad

- The pane grid and stats table widgets are not free. Both will
  need real custom egui work and pin their own complexity into
  the viz crate. Mitigation: the spike establishes the patterns
  before the wider redesign starts using them.
- egui's table and resize ergonomics are weaker than a web
  layout's; some friction is inevitable. The dark theme +
  density combination will need careful attention to contrast
  ratios that egui's default `Visuals` does not optimise for.

### Neutral

- The Bevy schedule and render app remain part of the binary
  even though most of v2 is 2D. Not a regression versus v1;
  v1 already pays this cost.

## Alternatives considered

### Move to pure egui via eframe (drop Bevy)

Lose the 3D popout's natural home; gain a slimmer binary and a
simpler schedule. Reserved as the reversibility path if the
spike clears but production friction surfaces later.

### Move to Tauri + web (Vue / Svelte / Solid + a web charting
library like uPlot or Apache ECharts)

Strongest layout and table primitives, weakest 3D story.
Re-implementing the 3D popout on a web canvas (three.js or
similar) is non-trivial and forks the team's mental model
across two stacks. Rejected for v2; reconsider only if egui
itself proves inadequate.

### Move to Slint or iced

Both are real Rust GUI toolkits with better table and layout
ergonomics than egui. Both would require re-pinning the
dependency stack, re-implementing the 3D popout, and dropping
egui_plot for whatever the host toolkit's charting story is.
Cost-vs-value does not justify the move pre-spike.

## See also

- `spec/viz-v2-design-brief.md` (the brief that triggered this
  decision)
- `PRODUCT.md`, `DESIGN.md` (strategic context)
- `crates/resinsim-viz/examples/viz_v2_spike.rs` (spike)
- `docs/adr/0010-resinsim-viz-presentation-layer.md` (one-way
  viz → core dependency rule)
- `docs/adr/0011-egui-control-panels.md` (incumbent panel
  anchors and dep version chain)
