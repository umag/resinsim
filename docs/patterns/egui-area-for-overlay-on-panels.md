---
issue: 10-build-plate-and-volume-cube
date: 2026-04-26
---

# Pattern: render floating HUD overlays as `egui::Area` in `EguiPrimaryContextPass`

## Symptom

A Bevy UI text node spawned at an absolute screen position (e.g.
`Node { position_type: PositionType::Absolute, top: Val::Px(8), left: Val::Px(8) }`)
is invisible at runtime when an `egui::SidePanel::left(...)` covers
the same screen area. The text is there in the entity world; its
pixels are just painted under the egui frame.

## Why

`bevy_egui 0.39` renders the egui frame to the camera output AFTER
Bevy UI in the default render-graph ordering. Z-index on the UI node
doesn't help because the two systems target different render passes.

## Pattern

Implement the overlay as an egui widget in the same multi-pass
schedule the panels run on:

```rust
use bevy_egui::{EguiContexts, egui};

pub fn debug_camera_overlay(
    mut contexts: EguiContexts,
    cam_q: Query<(&PanOrbitCamera, &Transform), With<Camera3d>>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return; };
    // ... compute body ...
    egui::Area::new(egui::Id::new("debug_camera_hud"))
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(296.0, 8.0))
        .interactable(false)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .corner_radius(4.0)
                .inner_margin(6.0)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(body).monospace());
                });
        });
}

// Schedule:
.add_systems(
    bevy_egui::EguiPrimaryContextPass,
    (left_panel, right_panel, debug_camera_overlay),
);
```

Notes:
- `interactable(false)` means clicks pass through to the 3D scene
  underneath; the overlay never steals input.
- `Order::Tooltip` puts the area above ordinary panels in egui's
  internal z-stack.
- Anchor offset (e.g. `(296, 8)`) sits the overlay just past the
  default 280-px-wide left panel so it doesn't visually collide.

## Counter-cases

- A modal dialog or a panel that DOES want input. Use `egui::Window`
  or another `SidePanel` instead — `Area` is for non-interactive
  decoration.
- The overlay is genuinely useful in standalone Bevy-UI builds. Keep
  the Bevy UI implementation and rely on z-index / render-graph
  ordering. (For resinsim-viz this isn't the case: egui is always
  present.)
