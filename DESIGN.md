---
name: resinsim-viz
description: A pre-flight oscilloscope for resin 3D-print physics. Grafana-dense, instrumentation-first, layer-as-time-axis.
---

<!-- SEED: re-run $impeccable document once there's code to capture the actual tokens and components. -->

# Design System: resinsim-viz

## 1. Overview

**Creative North Star: "The Print Oscilloscope"**

resinsim-viz is a workbench instrument, not an application. Every pane is a probe taking a measurement off a simulated print run; every pixel earns its place by carrying signal. The reader leans in close and reads off values, never being greeted, narrated to, or congratulated. The screen behaves like a Grafana board overseeing a piece of physical hardware, plus the tabular density of a Bloomberg terminal, plus the observability calmness of Honeycomb when nothing is burning. When something is burning, threshold colour and annotation make it impossible to miss.

The system explicitly rejects the consumer-slicer aesthetic: Lychee Slicer's gradients, friendly icons, hero numbers, "Your print is ready!" framing. It rejects SaaS-dashboard cliché: giant metric cards, AI-summary panels, glassmorphism. It rejects skinned hobbyist 3D-printer themes: orange/teal accents, drop shadows, decorative chrome. The 3D mesh is never the centrepiece; the time-series is.

**Theme: dark by default.** The scene is a slicer developer in a dimly-lit workshop late in the evening, working a long focus session beside a printer with an open vat. Overhead light stays low to keep stray UV off the resin; ambient comes off the monitor itself. That forces dark surfaces with cool blue-grey tonal layering. A light variant ships behind a setting for daytime sessions; the token system carries `surface-base-dark` as primary and `surface-base-light` as secondary, never as a re-skin.

**Key Characteristics:**
- Time-series first. Every panel either is a chart, supports a chart, or annotates one.
- Layer index is the shared time axis. The scrubber moves all panels at once.
- Tabular numerics. Mono digits in stats columns, never proportional.
- Quiet at rest, loud at faults. Healthy runs read uniform; trouble layers earn colour and weight automatically.
- No animation by default. State changes are instantaneous; the scrubber moves a cursor, not a curve.

## 2. Colors

A **Full palette** strategy: categorical series for plot lines, viridis for continuous physical magnitudes, cool tinted neutrals for surfaces. Colour carries information; it is never decoration.

### Primary
- **Series palette** (`[to be resolved at implementation. Anchor: Grafana classic 8-colour categorical]`): The 6-8 distinguishable line colours that separate parallel time-series within a plot (peel / suction / total force; vat / LED / ambient temperature). Categorical only, never used to imply ordinal magnitude.

### Secondary (continuous ramp)
- **Viridis** (`[exact stops to be resolved at implementation]`): The default ramp for continuous physical magnitudes: cure-depth heatmaps, force overlays on geometry, temperature gradients. CVD-safe. Never used categorically.

### Tertiary (threshold / alert)
- **Threshold red** (`[to be resolved. Desaturated, not panic-red]`): Used only on values that have crossed a fail threshold, e.g. peel force above the resin's published limit, cure depth below the layer height. Reserved exclusively for failure states. Never used to make a healthy value "pop".
- **Threshold amber** (`[to be resolved]`): Used only on values approaching a threshold (within a configurable margin). Reserved exclusively for warning states.

### Neutral
- **Surface base** (`[oklch resolved at implementation. Cool blue-grey, slight chroma toward hue 240. Dark family is primary, light family secondary; both shipped from day one]`): Panel backgrounds.
- **Surface low / high** (`[to be resolved]`): Recessed and raised tonal layers for nested panes.
- **Ink** (`[to be resolved. High-contrast against surface base, never pure white]`): Body text.
- **Ink muted** (`[to be resolved]`): Axis labels, grid lines, secondary stats, "(no run yet)" placeholders.
- **Grid line** (`[to be resolved. Surface base plus ~6% lightness step]`): Plot grid lines, panel separators. Visible enough to register, faint enough to disappear when reading data.

### Named Rules

**The Quiet-Background Rule.** Surfaces and grid lines compete with nothing. Body text and axis ticks have at least AA contrast against the surface; grid lines are the lightest non-equal step that is still visible. If a panel reads as "coloured", a colour has leaked into the chrome and must be removed.

**The Threshold-Only Red Rule.** The threshold red and threshold amber colours appear only on values that have crossed (or are approaching) a defined fail threshold. Forbidden as a delete-button colour, an emphasis colour, or a brand colour. If nothing is failing, no red appears anywhere on the screen.

**The Categorical-Only Series Rule.** The categorical series palette is for distinguishing parallel time-series within a single plot. It is never used to imply magnitude (use viridis), state (use threshold colours), or hierarchy (use weight). Two plots showing the same metric across two runs share a colour; two metrics on the same plot do not.

## 3. Typography

**Body Font:** `[to be resolved at implementation. Proposed: IBM Plex Sans, with system-ui fallback]`
**Numeric / Mono Font:** `[to be resolved at implementation. Proposed: IBM Plex Mono, with ui-monospace fallback]`

**Character:** Technical, neutral, instrumentation-grade. The body font carries labels, tooltips, and prose; the mono font carries every number on screen: stats columns, axis tick labels, time codes, layer indices, force and temperature readouts. Tabular figures throughout numeric contexts so columns align.

### Hierarchy

- **Section header** (semibold, ~14-15px, tight tracking): Panel titles like "Forces", "Temperature", "Layer stats". One per pane. No display sizes; there are no heroes.
- **Body** (regular, ~13px, ~1.4 line-height): Labels inside controls, status text, tooltips, prose where unavoidable. Cap line length at 65-75ch wherever prose exists.
- **Label / axis** (medium, ~11-12px, ~0.02em tracking): Plot axis ticks, legend entries, table column headers. Mono in numeric contexts.
- **Numeric readout** (mono, regular, ~12-14px, tabular figures): Every number on screen. Stats columns, force / temperature / time / depth values, layer indices, percentages.

### Named Rules

**The Mono-for-Numbers Rule.** Every number that may appear in a column alongside another number (layer stats tables, axis ticks, time codes, summary readouts) uses the mono face with tabular figures. Proportional digits in numeric contexts are forbidden because they break vertical alignment when values change between frames.

**The No-Display-Type Rule.** No display-sized text exists in the UI. The largest in-app type is a section header. Hero typography belongs in marketing surfaces; this is a workbench.

## 4. Elevation

Flat by default. Depth comes from tonal layering (recessed panel backgrounds, faint inset borders), not shadows. The few exceptions earn shadow only as an interaction response (hover on a draggable, focus on a control), and the value is a single ambient soft step, never decorative.

### Named Rules

**The Flat-By-Default Rule.** Surfaces are flat at rest. Cards and panels are differentiated by a tonal step in the surface scale, not by `box-shadow`. A drop shadow on a panel at rest signals decoration, which is the wrong register for this product.

**The No-Glass Rule.** No `backdrop-filter: blur(...)`, no semi-transparent panel backgrounds over dynamic content, no glassmorphism. The data is too dense for blur to ever clarify; it would only obscure.

## 5. Components

Omitted in seed mode. Components will be documented on the next pass once the Grafana-style panel system, layer scrubber, and stat-table primitives are implemented.

## 6. Do's and Don'ts

### Do:

- **Do** lead every panel with a measurement. If a panel is not showing a value, plot, or piece of geometry, it should not exist.
- **Do** share the layer-index axis across the scrubber, every plot's x-axis, and the stats panel. Moving any one moves all of them.
- **Do** use tabular mono figures for every number that lives in a column.
- **Do** keep healthy runs visually quiet. Trouble layers earn colour and weight automatically; no run should look "alarming" by default.
- **Do** use viridis for continuous physical magnitudes (cure depth, force, temperature). It is CVD-safe and ordinal-correct.
- **Do** annotate plots inline at fail layers. Marker plus small mono label beats a separate "warnings" panel.

### Don't:

- **Don't** treat the 3D mesh as the centrepiece. Geometry is supporting evidence, supplied to localise where in the print a number went bad, never the headline view.
- **Don't** display "Your print is ready!", emoji, congratulatory empty states, or any narrative copy. The reader knows the physics; they do not need encouragement.
- **Don't** use Lychee Slicer / ChiTuBox / Bambu Studio aesthetics: gradients, hero numbers, friendly icons, skinned themes, orange/teal accents, drop shadows.
- **Don't** use generic SaaS-dashboard patterns: giant single-number cards with sparklines, gradient backgrounds, AI-summary panels, glassmorphism.
- **Don't** use red or amber as emphasis or branding. Both are reserved exclusively for threshold states.
- **Don't** animate plot transitions or panel layout changes. State changes are instantaneous; the layer scrubber is a position change of a cursor, not a tween.
- **Don't** use proportional digits for any number that may appear next to another number.
- **Don't** introduce a display-size type ramp. The largest type in the UI is a section header.
- **Don't** use `border-left` or `border-right` greater than 1px as a coloured accent stripe on cards or list items.
- **Don't** use `background-clip: text` with a gradient. Use solid colour, weight, or size for emphasis.
- **Don't** wrap content in nested cards. Tonal surface steps replace nesting.
- **Don't** use modals for inspection. Reveal inline at the layer cursor instead.
- **Don't** use em dashes in product copy. Use commas, colons, semicolons, periods, or parentheses.
