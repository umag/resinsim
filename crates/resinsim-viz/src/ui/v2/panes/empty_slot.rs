//! Reserved-slot placeholder. Renders a muted "(empty slot)" marker
//! where the user can later add a pane via the Add-Pane UX (out of
//! scope for slice A, open Q #2).

use bevy_egui::egui;

use crate::ui::v2::pane::{PaneCtx, PaneId};
use crate::ui::v2::theme;

pub struct EmptySlotPane {
    /// Which reserved slot this placeholder occupies. Carried explicitly
    /// so the egui id and layout-persistence ids stay distinct between
    /// the two slots.
    pub slot: PaneId,
}

impl EmptySlotPane {
    pub fn render(&mut self, ui: &mut egui::Ui, _ctx: &PaneCtx<'_>) {
        let avail = ui.available_size();
        let (rect, _) = ui.allocate_exact_size(avail, egui::Sense::hover());
        let painter = ui.painter_at(rect);
        let centre = rect.center();
        painter.text(
            centre,
            egui::Align2::CENTER_CENTER,
            "(empty slot)",
            egui::FontId::monospace(12.0),
            theme::INK_MUTED,
        );
    }
}
