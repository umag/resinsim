---
issue: 04-egui-control-panels
date: 2026-04-26
---

# Anti-pattern: per-frame disk I/O inside an egui draw closure

## Symptom

An egui panel system that needs to render a value derived from
disk-loaded data calls the loader inline:

```rust
fn left_panel(mut contexts: EguiContexts, state: Res<PickerState>, repos: Res<ProfileRepos>) {
    let ctx = contexts.ctx_mut().unwrap();
    egui::SidePanel::left("controls").show(ctx, |ui| {
        if let Some(name) = state.selected_resin.as_deref() {
            // ⚠ disk read every frame the panel is open
            if let Ok(profile) = repos.resin.load(name) {
                ui.label(format!("Layer height: {:.1} µm", profile.recipe().layer_height_um()));
            }
        }
    });
}
```

At 60 fps and a 5 KB TOML, that's ~300 KB/s of redundant disk
reads + parse + validate work — invisible until profiling.

## Why it happens

egui is immediate-mode. The natural mental model is "compute the
display value at draw time". For pure-fn projections that's
correct. For anything I/O-bound, it's a hidden hot path because
the *correctness* of the GUI doesn't show the cost.

## What to do instead

Cache the loaded data on a `Resource` and refresh only on trigger
events (selection change, file watcher, explicit "Reload" button).
See `docs/patterns/idempotent-cache-on-selection-change.md` for
the cache-update shape.

```rust
// In PickerState (or a sibling Resource):
pub loaded_resin: Option<ResinProfile>,

// Updated by a small Update system when selected_resin changes:
fn refresh_loaded_profiles_system(...) { ... }

// The egui draw closure reads from the cache only:
if let Some(resin) = state.loaded_resin.as_ref() {
    ui.label(format!("Layer height: {:.1} µm", resin.recipe().layer_height_um()));
}
```

## Spotting heuristic

Before merging an egui panel system, grep its body for:

- `.load(`, `.read(`, `.parse(`, `fs::`, `File::`
- `serde_json::from_*`, `toml::from_*`
- HTTP / network calls

Anything in that list is suspicious inside `SidePanel::*::show(...)`
or `Plot::*::show(...)` — promote the value to a Resource updated
out-of-band.

## First-party example (rejected)

The original v3 plan for `resinsim-viz` issue 04 had the panel
system reading recipe values via `repos.resin.load(name).recipe()`.
Adversarial review (round 2) caught it before implementation; the
profile-cache pattern (`docs/patterns/idempotent-cache-on-selection-change.md`)
replaced it. The plan was rejected and revised before any per-frame
load shipped.

## Trade-offs

- **+** cache pattern is one extra Resource + one Update system
  vs. one inline `.load()` call — small surface tax for sub-ms
  win at GUI render frequency,
- **+** isolates I/O failures from rendering: a transient disk
  error doesn't blank the panel, the cache stays warm,
- **−** stale-cache risk if external mutation happens (file
  watcher closes the gap, but isn't in v1).

## See also

- `docs/patterns/idempotent-cache-on-selection-change.md` — the
  remediation pattern
- ADR-0011 — egui control panels
