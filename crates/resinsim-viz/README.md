# resinsim-viz

Bevy-based 3D visualization for resinsim simulations. Renders STL meshes
or CTB voxel-mask slice stacks, optionally overlaid with a per-layer
cure-depth heatmap from a `PrintSimulation` JSON.

## Quick start

```sh
# STL only
cargo run -p resinsim-viz -- --load-stl path/to/model.stl

# CTB sliced file
cargo run -p resinsim-viz -- --load-ctb path/to/model.ctb

# CTB + heatmap from a sim (issue 03)
cargo run -p resinsim-viz -- --load-ctb path/to/model.ctb --load-sim path/to/model.sim.json
```

Drag-drop a `.stl` or `.ctb` file onto the window to replace the loaded
geometry at runtime.

## Controls

| Action | Key / gesture |
|--------|---------------|
| Orbit camera | Mouse drag (or trackpad pan, Blender-style) |
| Pan camera | Trackpad two-finger / right-mouse drag |
| Zoom | Trackpad pinch / mouse wheel |
| Step to next layer (Z+) | `↑` arrow |
| Step to previous layer (Z−) | `↓` arrow |

The arrow keys only do anything when both `--load-ctb` and `--load-sim`
are loaded (or `--allow-mismatch` is set on a partial load).

## Heatmap

When a sim is loaded, every voxel in the slice-stack mesh is coloured
by its layer's `cure_depth_um`, mapped through a viridis ramp normalised
to the `(min, max)` cure-depth across all layers. A semi-transparent
horizontal cursor quad sits at the active layer's Z plane; the arrow
keys move the cursor without re-uploading the mesh.

Each layer change emits one log line:

```
INFO Layer 1234/4500 | cure_depth  142.3 µm | ramp 47.1–183.4 µm
```

The `ramp` field is the colour-bar legend: it shows the µm domain that
the viridis ramp spans, so colours from any layer (not just the active
one) can be interpreted by eye.

### Layer-count contract

`--load-sim` JSON must have the same number of layers as `--load-ctb`.
On mismatch, the viz emits an error and leaves the world empty. Pass
`--allow-mismatch` to override (renders uncoloured) — only useful when
deliberately mixing a CTB with a non-matching sim during development.

## CLI flags

```
--load-stl PATH             Load an STL file at startup.
--load-ctb PATH             Load a CTB sliced file at startup.
--load-sim PATH.json        Load a PrintSimulation JSON at startup;
                            required for the cure-depth heatmap overlay.
                            Has no effect without --load-ctb.
--allow-mismatch            DANGEROUS: skip the safety check that
                            requires --load-sim to have the same layer
                            count as --load-ctb.
--smoke-exit                Run one frame and exit (CI/smoke-test mode).
--screenshot PATH.png       Capture-and-exit: render the configured
                            scene, write a PNG, exit with 0 on success
                            (or one of the non-zero exit codes below).
                            See "Screenshot capture (AI feedback loop)".
```

`--load-stl` and `--load-ctb` are mutually exclusive (clap-enforced).

## Exit codes

When `--smoke-exit` OR `--screenshot` is set, the viz uses distinct
non-zero exit codes for fatal failure classes so CI / AI agents can
branch on `$?`:

| Code | Meaning |
|------|---------|
| 0 | Success |
| 2 | Sim file load / parse / validate failed |
| 3 | Layer-count mismatch between `--load-ctb` and `--load-sim` |
| 4 | Bad pairing: `--load-sim` without `--load-ctb`, or with `--load-stl` |
| 5 | Invalid `--screenshot` path (empty / directory / missing parent / bad extension) |
| 6 | CTB file load / parse failed |
| 7 | `--screenshot`: Captured marker fired but file didn't appear (post-Captured filesystem failure) |
| 8 | `--screenshot`: Bevy never emitted Captured (render timeout — likely headless / GPU hang / software rasterizer) |

## Screenshot capture (AI feedback loop)

Two surfaces — the CLI flag (AI-friendly capture-and-exit) and the
Capture-screenshot button (interactive, no exit). Both share the
Bevy 0.18 screenshot pipeline.

### `--screenshot PATH.png` (CLI)

- Loads geometry/sim, waits for loads to settle, waits 2 settle frames
  for PBR / transparency sort, captures via Bevy's
  `Screenshot::primary_window()`, defers exit until Bevy emits
  `Captured` AND verifies the file landed (non-empty) on disk, exits 0.
- **Exit codes propagate without `--smoke-exit`** — `--screenshot` is a
  self-sufficient capture-and-exit flag. The two flags are independent
  triggers for the same propagation behaviour.
- Phase 1 timeout: capture-anyway + warn after 10 s of unsettled loads.
- Phase 2 timeout: exit 8 after 10 s of stuck render.
- Phase 3 timeout: exit 7 after 1 s of missing file post-Captured.
- **Path semantics differ from `--save-sim`**: parent dir must exist;
  `--screenshot` does NOT create it. Extension must be `.png` / `.jpg`
  / `.jpeg`.

### Capture-screenshot button (interactive)

- Always enabled. Click any time during a session.
- Saves to `<CWD>/resinsim-viz-<unix-secs>.png` (or `<TMPDIR>/...` if
  CWD is unavailable).
- Logs `Screenshot saved to <absolute-path>` to stderr (Bevy native).
- Shows a transient "Captured: \<basename\>" label in the panel for 3 s.
- App keeps running; click again for another capture.

### Examples

```bash
# AI workflow — happy path
resinsim-viz --load-ctb foo.ctb --load-sim foo.sim.json \
  --screenshot /tmp/shot.png && file /tmp/shot.png
# `file` confirms the PNG is well-formed — catches partial-write
# corruption that the exit code alone wouldn't detect.

# Interactive — click "Capture screenshot" under the left-panel
# Capture section.
resinsim-viz --load-ctb foo.ctb &
# ... interact, click button ...
open "$(ls -t resinsim-viz-*.png | head -1)"   # open the latest
```

### Failure modes (AI agents: branch on `$?`)

```bash
# Bad CTB path → exit 6
resinsim-viz --load-ctb /missing.ctb --screenshot /tmp/x.png
echo $?  # 6

# Invalid screenshot path → exit 5
resinsim-viz --screenshot /no/such/dir/x.png
echo $?  # 5

# Bad sim pairing → exit 4
resinsim-viz --load-sim foo.sim.json --screenshot /tmp/x.png
echo $?  # 4

# Render timeout (CI software rasterizer, headless) → exit 8
resinsim-viz --load-ctb foo.ctb --screenshot /tmp/x.png
echo $?  # 8 — retry on a different machine (or extend the timeout
         # in source if your CI is the bottleneck)

# Filesystem failure mid-capture (disk full, perms revoked) → exit 7
# Hard to reproduce reliably; covered by unit tests in
# crates/resinsim-viz/src/screenshot.rs. See spec/uat/viz-screenshot-flag.md
# "Test coverage notes" section for the rationale.
```

### See also

- KB-3 anti-pattern motivation:
  `docs/patterns/anti/visual-spec-drift-no-test-guard.md`
- Bevy 0.18 screenshot API:
  `bevy_render-0.18.1/src/view/window/screenshot.rs`

## Architecture notes

- `mesh.rs` — STL → flat-shaded Bevy mesh
- `slice.rs` — CTB voxel-mask stack → face-culled Bevy mesh, with
  optional per-vertex `ATTRIBUTE_COLOR` for the heatmap
- `heatmap.rs` — pure viridis colour ramp (no Bevy types); `cure_depth_domain`,
  `viridis(t)`, `ramp(value, domain)`
- `main.rs` — Bevy resources (`LoadedSimulation`, `CurrentLayer`,
  `LayerZPrefix`, `CureDepthDomain`), loaders, keyboard handler,
  cursor + HUD systems

ADR-0010 governs the one-way `viz → core` dependency rule. The slice-stack
mesh is bake-once: per-vertex colours are baked at load and the mesh is
never mutated post-spawn. Layer-change updates only the cursor entity's
`Transform.translation.z` — Bevy Transform writes do not re-upload the
mesh.
