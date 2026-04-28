---
issue: viz-v2-dashboard
date: 2026-05-12
---

# Anti-pattern: egui texture cached with per-frame unique name

## Symptom

Caller wants to display a different image (heatmap, atlas tile,
data visualisation) depending on some transient piece of state
(cursor position, layer index, frame number) and writes:

```rust
let texture = ui.ctx().load_texture(
    format!("my-thing-{}", state.cursor_layer),
    image,
    egui::TextureOptions::NEAREST,
);
```

This works visually. It also creates a **distinct** egui texture
per cursor position. egui purges unreferenced textures eventually,
but during an active scrub the live ceiling can be
`number_of_visited_states × texture_size`. For a 4492-layer
slice-stack heatmap at, say, 310×172 RGBA8 pixels per layer, the
peak is ~960 MB-equivalent of texture metadata across the scrub
even though only one is on screen.

## Why it's wrong

The texture name is the **identity** of a texture from egui's POV.
Different name → new GPU upload + new handle held by the texture
manager. Even with weak references via `TextureHandle`, the
churn is high and the peak memory ceiling is unbounded by the
caller's intent (one image visible at a time).

## Pattern instead

Use a **fixed** texture name and **overwrite** its contents on
state change:

```rust
// Option A: keep a single TextureHandle, replace its contents.
let cached: &mut Option<egui::TextureHandle> = …;
let image = build_image_for(state.cursor_layer);
match cached {
    Some(h) => h.set(image, egui::TextureOptions::NEAREST),
    None => {
        *cached = Some(ui.ctx().load_texture(
            "my-thing-fixed",
            image,
            egui::TextureOptions::NEAREST,
        ));
    }
}
```

`TextureHandle::set` overwrites the GPU contents in place. The
texture name and handle identity persist across state changes; the
GPU memory footprint is exactly one image, not one-per-state.

## When the per-state name IS correct

If you actually want a **cache** of N recent textures (so backwards
scrubbing doesn't re-upload), name them by their identity and
manage eviction explicitly. The anti-pattern is naming by transient
state with no explicit eviction policy.

## See also

- `crates/resinsim-viz/src/ui/v2/panes/layer_mask_2d.rs` — current
  implementation uses the per-layer-name approach; deferred fix to
  switch to fixed-name + `set()` on cursor change. Recorded as
  known issue in the lifecycle's `record_review` findings.
