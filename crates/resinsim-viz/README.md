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
```

`--load-stl` and `--load-ctb` are mutually exclusive (clap-enforced).

## Exit codes

When `--smoke-exit` is set, the viz uses distinct non-zero exit codes
for fatal failure classes so CI can branch:

| Code | Meaning |
|------|---------|
| 0 | Success |
| 2 | Sim file load / parse / validate failed |
| 3 | Layer-count mismatch between `--load-ctb` and `--load-sim` |
| 4 | Bad pairing: `--load-sim` without `--load-ctb`, or with `--load-stl` |

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
