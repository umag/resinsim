//! Layer mask 2D heatmap pane. Slice E.
//!
//! Renders the current layer's CTB mask as an `egui` texture
//! tinted by that layer's `cure_depth_um` via the viridis ramp.
//! When no CTB is loaded (or the cursor layer has no mask), the
//! pane falls back to the brief §6 `(no CTB loaded, geometry
//! unavailable)` placeholder.
//!
//! Texture caching: the pane keeps a single `TextureHandle` for
//! the last-rendered layer. Re-builds + re-uploads only when the
//! cursor moves to a different layer. The "tint" is uniform
//! across all "on" pixels (the cure_depth scalar lives per-layer,
//! not per-pixel), so the visual story is: silhouette colour
//! changes as the user scrubs through layers; layers near a cure-
//! depth fail boundary stand out in different viridis bands.

use bevy_egui::egui;
use resinsim_core::values::LayerMask;

use crate::ui::v2::pane::{empty_axis_placeholder, pane_header, PaneCtx, PaneId};
use crate::ui::v2::theme;

#[derive(Default)]
pub struct LayerMask2dPane {
    cached: Option<(u32, egui::TextureHandle)>,
}

impl LayerMask2dPane {
    pub fn render(&mut self, ui: &mut egui::Ui, ctx: &PaneCtx<'_>) {
        pane_header(ui, "Layer mask");

        let Some(masks) = ctx.slice_masks else {
            self.cached = None;
            empty_axis_placeholder(
                ui,
                PaneId::LayerMask2d,
                "X (mm)",
                "Y (mm)",
                ctx.link_group,
                Some("(no CTB loaded, geometry unavailable)"),
            );
            return;
        };

        let layer_idx = ctx.cursor_layer as usize;
        let Some(layer) = masks.get(layer_idx) else {
            empty_axis_placeholder(
                ui,
                PaneId::LayerMask2d,
                "X (mm)",
                "Y (mm)",
                ctx.link_group,
                Some("(cursor layer is out of CTB range)"),
            );
            return;
        };
        let Some(mask) = layer.mask.as_ref() else {
            empty_axis_placeholder(
                ui,
                PaneId::LayerMask2d,
                "X (mm)",
                "Y (mm)",
                ctx.link_group,
                Some("(layer has no mask, empty slice)"),
            );
            return;
        };

        // Tint colour: viridis-mapped cure depth for this layer.
        // Falls back to a neutral mid-band ink when sim is absent
        // (shouldn't happen if masks are loaded — CTB pairing
        // generally implies a sim — but defensive).
        let tint = layer_tint_color(ctx, layer_idx);

        // Cache hit / miss. Rebuild only when the cursor moves to
        // a new layer; otherwise reuse the GPU texture. This keeps
        // scrubbing through 4500 layers cheap because only one
        // texture upload happens per scrub tick.
        let needs_rebuild = self
            .cached
            .as_ref()
            .map(|(l, _)| *l != ctx.cursor_layer)
            .unwrap_or(true);
        if needs_rebuild {
            let image = mask_to_color_image(mask, tint, theme::SURFACE_LOW);
            let texture = ui.ctx().load_texture(
                format!("v2-layer-mask-{}", ctx.cursor_layer),
                image,
                egui::TextureOptions::NEAREST,
            );
            self.cached = Some((ctx.cursor_layer, texture));
        }

        if let Some((_, texture)) = &self.cached {
            // Fit-to-pane while preserving aspect ratio.
            let avail = ui.available_size();
            let texture_size = texture.size_vec2();
            if texture_size.x > 0.0 && texture_size.y > 0.0 {
                let scale = (avail.x / texture_size.x).min(avail.y / texture_size.y);
                let render_size = texture_size * scale;
                ui.add(egui::Image::new((texture.id(), render_size)));
            }
        }
    }
}

fn layer_tint_color(ctx: &PaneCtx<'_>, layer_idx: usize) -> egui::Color32 {
    let (cure_depth, domain) = match ctx.sim {
        Some(sim) => {
            let domain = crate::heatmap::cure_depth_domain(sim);
            let depth = sim
                .layers()
                .get(layer_idx)
                .map(|l| l.cure_depth_um)
                .unwrap_or(0.0);
            (depth, domain)
        }
        None => (0.5, (0.0_f32, 1.0_f32)),
    };
    ramp_to_color32(crate::heatmap::ramp(cure_depth, domain))
}

/// Build a [`egui::ColorImage`] from a binary `LayerMask`, painting
/// solid cells with `on_color` and empty cells with `off_color`.
///
/// Pure helper, unit-tested. The brief's tabular-mono / no-glass /
/// instrumentation tone allows whatever colours the caller passes;
/// the viridis tinting decision lives at the call site.
pub fn mask_to_color_image(
    mask: &LayerMask,
    on_color: egui::Color32,
    off_color: egui::Color32,
) -> egui::ColorImage {
    let w = mask.width_cells() as usize;
    let h = mask.height_cells() as usize;
    let mut pixels = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let c = if mask.is_solid(x as u32, y as u32) {
                on_color
            } else {
                off_color
            };
            pixels.push(c);
        }
    }
    egui::ColorImage::new([w, h], pixels)
}

/// Convert a [`crate::heatmap::ramp`] RGBA `[f32; 4]` (each in
/// `[0.0, 1.0]`) into an `egui::Color32`. Non-finite components
/// clamp to `0..=1`.
pub fn ramp_to_color32(rgba: [f32; 4]) -> egui::Color32 {
    let to_byte = |x: f32| {
        let clamped = if x.is_finite() {
            x.clamp(0.0, 1.0)
        } else {
            0.0
        };
        (clamped * 255.0).round() as u8
    };
    egui::Color32::from_rgba_unmultiplied(
        to_byte(rgba[0]),
        to_byte(rgba[1]),
        to_byte(rgba[2]),
        to_byte(rgba[3]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_mask(w: u32, h: u32) -> LayerMask {
        LayerMask::new_all_solid(w, h, 0.5).expect("test fixture")
    }

    fn empty_mask(w: u32, h: u32) -> LayerMask {
        LayerMask::new(w, h, 0.5).expect("test fixture")
    }

    // ---- mask_to_color_image ----

    #[test]
    fn all_solid_mask_paints_all_on_color() {
        let mask = solid_mask(4, 3);
        let img = mask_to_color_image(&mask, egui::Color32::RED, egui::Color32::BLUE);
        assert_eq!(img.size, [4, 3]);
        assert_eq!(img.pixels.len(), 12);
        for p in &img.pixels {
            assert_eq!(*p, egui::Color32::RED);
        }
    }

    #[test]
    fn empty_mask_paints_all_off_color() {
        let mask = empty_mask(4, 3);
        let img = mask_to_color_image(&mask, egui::Color32::RED, egui::Color32::BLUE);
        assert_eq!(img.size, [4, 3]);
        for p in &img.pixels {
            assert_eq!(*p, egui::Color32::BLUE);
        }
    }

    #[test]
    fn single_cell_mask_round_trips() {
        let mut mask = empty_mask(2, 2);
        mask.set(0, 0).expect("set (0,0)");
        let img = mask_to_color_image(&mask, egui::Color32::RED, egui::Color32::BLUE);
        // Row 0: (0,0) on, (1,0) off
        // Row 1: (0,1) off, (1,1) off
        assert_eq!(img.pixels[0], egui::Color32::RED);
        assert_eq!(img.pixels[1], egui::Color32::BLUE);
        assert_eq!(img.pixels[2], egui::Color32::BLUE);
        assert_eq!(img.pixels[3], egui::Color32::BLUE);
    }

    // ---- ramp_to_color32 ----

    #[test]
    fn ramp_to_color32_pure_white() {
        let c = ramp_to_color32([1.0, 1.0, 1.0, 1.0]);
        assert_eq!(c, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 255));
    }

    #[test]
    fn ramp_to_color32_pure_black() {
        let c = ramp_to_color32([0.0, 0.0, 0.0, 1.0]);
        assert_eq!(c, egui::Color32::from_rgba_unmultiplied(0, 0, 0, 255));
    }

    #[test]
    fn ramp_to_color32_half_grey() {
        let c = ramp_to_color32([0.5, 0.5, 0.5, 1.0]);
        // 0.5 * 255 = 127.5, round-to-nearest → 128.
        assert_eq!(c, egui::Color32::from_rgba_unmultiplied(128, 128, 128, 255));
    }

    #[test]
    fn ramp_to_color32_clamps_out_of_range() {
        let c = ramp_to_color32([1.5, -0.5, 2.0, 1.0]);
        assert_eq!(c, egui::Color32::from_rgba_unmultiplied(255, 0, 255, 255));
    }

    #[test]
    fn ramp_to_color32_non_finite_falls_back_to_zero() {
        let c = ramp_to_color32([f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.0]);
        assert_eq!(c, egui::Color32::from_rgba_unmultiplied(0, 0, 0, 255));
    }
}
