# Product

## Register

product

## Users

DragonFruit slicer developers — engineers working on resin slicer software who need to validate a sliced print job *before committing it to a real printer*. The primary user is the slicer dev mid-iteration: they've just produced a CTB, they want to know whether the physics looks healthy across all layers (peel forces, suction, cure margins, temperatures), and they want to scrub through layers to inspect geometry where the simulation flags trouble.

Secondary: maintainers reviewing somebody else's calibration run via screenshots or saved sim outputs.

Context: desktop, focused work, multi-monitor common, sessions are minutes-to-hours.

## Product Purpose

resinsim-viz is a pre-flight readout for resin print jobs. It runs the `resinsim-core` physics simulation against a sliced CTB and surfaces every layer's behaviour as time-series data, per-layer stats, and inspectable geometry — so a slicer developer can spot trouble (failure layers, force spikes, thermal drift, suction cliffs) without burning resin.

Success: the developer can answer "is this job safe to print, and if not which layer breaks first and why" by looking at one screen, in seconds.

## Brand Personality

Voice: instrumentation, not narration. The UI shows the data; it never explains, summarises, or congratulates.

Three words: **dense, precise, operational**.

Emotional goal: the calm of a working oscilloscope or Grafana dashboard — the reader feels in command of a process they can actually see, not a black box.

## Anti-references

- **Lychee Slicer** and its consumer-slicer cousins (ChiTuBox, Bambu Studio preview): hero metrics, friendly icons, gradients, congratulatory empty states, "Your print is ready!" framing. None of that.
- Generic SaaS dashboards: giant single numbers with sparklines, gradient cards, AI-summary panels, glassmorphism.
- Hobbyist 3D-printer apps with skinned dark themes (orange/teal accents, drop shadows). Decoration without information.
- Anything that treats the 3D mesh as the centrepiece. Geometry is supporting evidence, not the protagonist.

## Design Principles

1. **Physics first, geometry second.** The data is the product. Every panel is a measurement. Geometry exists only to localise where in the print a number went bad, never as the headline view.
2. **Layer is the time axis.** Layer index is wall-clock progress through the print. The scrubber, plot x-axes, and stats panel share that axis; moving any one moves all of them.
3. **Pre-flight, not autopsy.** Defaults surface what would block a real print: failure layers, force spikes, suction cliffs, thermal excursions. Healthy runs look quiet; trouble layers earn visual weight automatically.
4. **No narration.** No "your print is healthy!" labels, no AI summaries, no friendly emoji. A reader who knows the physics reads the screen directly. Annotations replace prose.
5. **Composable panes over fixed layouts.** Grafana-style: every pane is a self-contained probe (time-series, stat, geometry slice, heatmap legend). The user composes the screen they need; nothing forces a hierarchy.

## Accessibility & Inclusion

- Heatmap and series colour ramps must be CVD-safe (viridis is the default; never red-green for thresholds).
- Reduced motion respected: no panel animation by default; the layer scrubber is a position change, not a transition.
- WCAG 2.2 AA contrast for text against panel backgrounds.
