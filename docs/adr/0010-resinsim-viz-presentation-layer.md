---
issue: 01-viz-crate-scaffold
date: 2026-04-26
---

# ADR-0010: resinsim-viz is the presentation layer; one-way dep on resinsim-core

## Status
Accepted

## Context

Phase 2 of the simulation plan
(`projects/000-global/research/resinsim-physics-simulation-plan.md`)
introduces a Bevy 0.18 visualisation app. Today the workspace has two
crates — `resinsim-core` (domain) and `resinsim-inspect` (CLI) — and
ADR-0001 governs layering *inside* resinsim-core (Services → Entities →
Values, one-way). With a new crate `resinsim-viz` joining the workspace
we now need an equivalent rule *across* crate boundaries and several
collateral decisions about which dependencies the scaffold actually
takes.

## Decision

### Layering

`resinsim-viz` depends on `resinsim-core`. `resinsim-core` MUST NOT
depend on `resinsim-viz`, on `bevy`, or on any GPU/UI crate. Future
"shared rendering helpers" live in viz, not core. The application
services in `resinsim-core::app` (e.g. `SimulationRunner`,
`ReportGenerator`) are the seam: viz calls them, core never calls back
into viz.

### Bevy version

`bevy = "0.18"`. The Phase 2 plan originally targeted Bevy 0.16 (per
`projects/000-global/research/resinsim-physics-simulation-plan.md`),
but `bevy_panorbit_camera 0.34` requires `bevy ^0.18`, forcing the
bump at implementation time. Two collateral 0.17/0.18 API renames
followed and are reflected in `src/main.rs`:

- `EventWriter` → `MessageWriter` (Bevy 0.17 message/event split,
  carried forward in 0.18). `AppExit` is now sent via
  `MessageWriter<AppExit>::write(...)`.
- `AmbientLight` moved from a global `Resource` to a per-camera
  `Component`. It is now spawned as a component on the `Camera3d`
  entity, and the `setup_scene_attaches_ambient_light_to_camera`
  unit test queries `(&Camera3d, &AmbientLight)` accordingly.

### Deferred dependencies

`bevy_egui = "0.36"` is intentionally absent from this scaffold. It
enters at issue `04-egui-control-panels`, where it is first wired into
a system. Rationale: introducing an unused dep (a) inflates compile
time on a crate that does not yet need it, (b) hides the
bevy_egui-vs-bevy-0.16 version-pinning decision behind dead code, (c)
risks `unused_crate_dependencies` warnings if that lint is enabled
later.

### Camera library

`bevy_panorbit_camera = "0.34"` is adopted over a hand-rolled orbit
camera. Rationale: gimbal lock, focus drift, momentum smoothing, and
trackpad-vs-mouse routing are non-trivial and out of scope for a
scaffold issue. Other crates considered: `bevy_blendy_cameras`
(Pan/Orbit/Zoom + Fly + frame-entities, heavier surface area than
needed today), `bevy_lagrange` (animated zoom-to-fit, scope creep for
a scaffold). Re-evaluate in Phase 5 if richer camera flows are
required.

### Input config (Mac-trackpad first-class)

`PanOrbitCamera` is configured `TrackpadBehavior::BlenderLike` with
`trackpad_pinch_to_zoom_enabled: true`. Rationale: the developer
workstation is macOS; trackpad gestures must be first-class, not
bolted on. Bevy 0.18 ships native `PinchGesture`, `PanGesture`, and
`RotationGesture` events on macOS but `bevy_panorbit_camera` already
abstracts over them — we do not consume the gesture events directly
here.

### CLI parser

`clap = "4"` is retained for parity with `resinsim-inspect`'s CLI
conventions and to leave headroom for future viz flags
(`--load-stl`, `--resin`, `--json`, …) without re-tooling. The
single-flag overhead today (`--smoke-exit`) is accepted for
consistency.

## Consequences

- **Compile-time enforcement of the layering rule.** Workspace
  structure prevents `resinsim-core/Cargo.toml` from listing `bevy*`
  deps; any attempt to add one would either fail to compile (cyclic
  deps via path) or be visible in code review.
- **Test seam.** `setup_scene` is a `pub fn` (not a closure inlined
  in `main`) so unit tests can run it on a plugin-less `App::new()`
  and assert the spawned entity/resource shape without booting wgpu
  or a window. Future Phase 2 issues (02..08) extend this pattern:
  pure functions under `src/<area>/` with `#[cfg(test)]` inline tests.
- **Switching the orbit-camera crate later** requires updating the
  unit tests in `src/main.rs::tests` (their assertions name
  `PanOrbitCamera` and `TrackpadBehavior`).
- **Future hardening (NOT implemented in this issue).** A CI check
  that greps `crates/resinsim-core/Cargo.toml` for any `bevy*` line
  would catch a regression of the layering rule. resinsim has no
  `.github/workflows/` today (verified absent on 2026-04-26); recommend
  filing a follow-up issue `resinsim-add-ci` once Phase 2 has a few
  PRs and the compile-time impact is observable.
