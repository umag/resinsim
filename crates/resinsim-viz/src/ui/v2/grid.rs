//! 5×2 pane grid for the v2 dashboard.
//!
//! Pass 2 (this commit): one column splitter + four row splitters.
//! Splitter visuals are 1px in `GRID_LINE`; on hover the visual
//! widens to 2px in `INK_MUTED` and the cursor flips to
//! `ResizeColumn` / `ResizeRow`. The hit zone is 8px logical (≈16px
//! physical on retina), wide enough that a Mac trackpad can land on
//! it without the user squinting at a hairline.
//!
//! Layout state lives in two fields:
//!   - `column_split: f32` — fraction in [0, 1] of the grid width
//!     occupied by column 0. Column 1 takes the remainder.
//!   - `row_fracs: [f32; 5]` — fractions per row; renormalised every
//!     frame so they always sum to 1.0 even after rounding drift.
//!
//! The splitter math is extracted into pure helpers
//! (`apply_column_split_delta`, `apply_row_splitter_delta`,
//! `cumulative_starts`) that have no egui dependency, so the
//! load-bearing clamp/redistribute logic is unit-tested per
//! `docs/patterns/bevy-app-test-seam.md` without spinning up an
//! `EguiPlugin`.
//!
//! Persistence (Pass 5) will serde the layout state to disk; reorder
//! (Pass 4) will mutate `cells`. Neither mechanism touches the
//! splitter math.

use std::time::Instant;

use bevy_egui::egui;
use resinsim_core::simulation::PrintSimulation;

use super::layout_persist::{PaneGridLayout, LAYOUT_SCHEMA_VERSION};
use super::pane::{detect_field_missing, Pane, PaneCtx, PaneId, PaneState};
use super::panes::{
    AreaDeltaPane, CureDepthPane, EmptySlotPane, ForcesPane, LayerMask2dPane, SafetyPane,
    VatTempPane, ViscosityPane, ZDeflectionPane,
};
use super::theme;

const ROWS: usize = 5;
const COLS: usize = 2;

/// Minimum cell size in logical pixels. Below ~180×120 axis labels
/// inside `egui_plot` start to collide (matches ADR-0016's bottom-
/// panel floor). The splitter math clamps drag deltas so adjacent
/// cells never shrink past this.
const MIN_CELL_W_PX: f32 = 180.0;
const MIN_CELL_H_PX: f32 = 120.0;

/// Splitter hit-target width / height in logical pixels. Visually a
/// 1px line; the surrounding 8px is invisible drag-target padding.
const SPLITTER_HIT_PX: f32 = 8.0;
const SPLITTER_VIS_PX: f32 = 1.0;
const SPLITTER_VIS_HOVER_PX: f32 = 2.0;

/// Height of the drag-handle strip at the top of each cell. The
/// pane's section header (rendered by `pane_header`) is visually
/// inside this strip; click-and-drag anywhere on it picks up the
/// pane for reorder. The plot body below is untouched, so plot
/// pan/zoom interactions still work.
const HEADER_DRAG_HANDLE_PX: f32 = 28.0;

/// Drag payload carried by `egui::DragAndDrop` while a pane is being
/// reordered. `(source_row, source_col)` is the cell the user
/// grabbed; on release, the cell at the cursor swaps with the
/// source (per open Q #4: swap, not insert+shift).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DragPayload {
    source_row: usize,
    source_col: usize,
}

/// 5×2 grid of panes plus its splitter state. Cells are addressed
/// `(row, col)`; row 0 is the top, column 0 is the left.
pub struct PaneGrid {
    cells: [[Pane; COLS]; ROWS],
    /// Width fraction (0..1) of column 0; column 1 gets the remainder.
    column_split: f32,
    /// Height fractions of each row; renormalised each frame.
    row_fracs: [f32; ROWS],
    /// Per-cell column span pattern:
    ///   `1` = normal cell occupying one column;
    ///   `2` = cell extends through `(row, col+1)` — wide cell;
    ///   `0` = continuation of a wide cell at `(row, col-1)` —
    ///         the cell still holds a `Pane` value (placeholder),
    ///         but neither chrome nor body is rendered here.
    /// Only width-2 spans starting at column 0 are valid; the
    /// renderer treats other patterns defensively (skipping the
    /// continuation) but the loader resets malformed inputs.
    col_spans: [[u8; COLS]; ROWS],
    /// Set when any state-mutating action runs (resize, swap, hide,
    /// layout reset). The dashboard uses this to debounce writes to
    /// the persisted layout file: only save once the user has
    /// stopped fiddling for ~500ms (see `take_save_if_due`).
    dirty_at: Option<Instant>,
    /// One-shot trigger for the right-click "Reset zoom" command. The
    /// next frame's `PaneCtx` for that cell sets `reset_zoom = true`,
    /// the pane's plot closure calls `set_auto_bounds`, and the
    /// trigger clears.
    pending_reset_zoom: Option<(usize, usize)>,
}

impl PaneGrid {
    /// Default layout per `spec/viz-v2-design-brief.md` §5 with the
    /// LayerMask2d pane spanning the full bottom row so the
    /// silhouette has room to read at print scale. The two
    /// reserved-slot positions now sit in row 3 column 1 (one of
    /// them) and as the continuation cell at row 4 column 1 (the
    /// other — never rendered, kept only because every `(row, col)`
    /// slot holds a `Pane` value).
    pub fn default_layout() -> Self {
        Self {
            cells: [
                [
                    Pane::Forces(ForcesPane::default()),
                    Pane::Safety(SafetyPane::default()),
                ],
                [
                    Pane::CureDepth(CureDepthPane::default()),
                    Pane::VatTemp(VatTempPane::default()),
                ],
                [
                    Pane::AreaDelta(AreaDeltaPane::default()),
                    Pane::Viscosity(ViscosityPane::default()),
                ],
                [
                    Pane::ZDeflection(ZDeflectionPane::default()),
                    Pane::EmptySlot(EmptySlotPane {
                        slot: PaneId::EmptySlot1,
                    }),
                ],
                [
                    Pane::LayerMask2d(LayerMask2dPane::default()),
                    Pane::EmptySlot(EmptySlotPane {
                        slot: PaneId::EmptySlot2,
                    }),
                ],
            ],
            column_split: 0.5,
            row_fracs: [0.2; ROWS],
            // LayerMask2d at (4, 0) spans both columns; the cell at
            // (4, 1) is the continuation and never renders.
            col_spans: [[1, 1], [1, 1], [1, 1], [1, 1], [2, 0]],
            dirty_at: None,
            pending_reset_zoom: None,
        }
    }

    /// Construct a grid from a persisted [`PaneGridLayout`]. Each
    /// cell's `PaneId` is materialised via `Pane::from_id`. Splitter
    /// state is copied verbatim. `col_spans` are sanitised to a
    /// valid pattern; malformed inputs fall back to all-single
    /// cells so a corrupt persisted layout doesn't leave the grid
    /// in a state that can't render.
    pub fn from_layout(layout: PaneGridLayout) -> Self {
        let cells: [[Pane; COLS]; ROWS] =
            std::array::from_fn(|r| std::array::from_fn(|c| Pane::from_id(layout.cells[r][c])));
        Self {
            cells,
            column_split: layout.column_split,
            row_fracs: layout.row_fracs,
            col_spans: sanitised_col_spans(layout.col_spans),
            dirty_at: None,
            pending_reset_zoom: None,
        }
    }

    /// Snapshot the persistable state. View state inside individual
    /// pane variants is dropped — slice A panes are stateless.
    pub fn to_layout(&self) -> PaneGridLayout {
        let cells: [[PaneId; COLS]; ROWS] =
            std::array::from_fn(|r| std::array::from_fn(|c| self.cells[r][c].id()));
        PaneGridLayout {
            schema_version: LAYOUT_SCHEMA_VERSION,
            column_split: self.column_split,
            row_fracs: self.row_fracs,
            cells,
            col_spans: self.col_spans,
        }
    }

    /// If a dirty timestamp exists and the dwell time has elapsed,
    /// clear the timestamp and return the layout to write. Otherwise
    /// return None. Caller is responsible for the actual disk write
    /// (so test callers can stub it).
    pub fn take_save_if_due(&mut self, dwell: std::time::Duration) -> Option<PaneGridLayout> {
        if let Some(t) = self.dirty_at {
            if t.elapsed() >= dwell {
                self.dirty_at = None;
                return Some(self.to_layout());
            }
        }
        None
    }

    /// Render the grid at the available rect inside `ui`. Owns the
    /// full sub-area; cells and splitters are positioned absolutely
    /// from the grid origin so the user's drag state survives between
    /// frames without requiring egui's layout cursor.
    ///
    /// `last_error` is `Some(msg)` when the most recent `--load-sim`
    /// attempt failed to parse; in that case the grid is replaced
    /// with an ink-muted error block per brief §6 ParseError state.
    pub fn render(
        &mut self,
        ui: &mut egui::Ui,
        sim: Option<&PrintSimulation>,
        slice_masks: Option<&[resinsim_core::io::sliced::LayerInput]>,
        last_error: Option<&str>,
        cursor_layer: u32,
        pinch_delta: f32,
    ) {
        // Stake the full rect upfront so the parent's layout advances
        // by the right amount and any subsequent UI lands below.
        let avail = ui.available_size();
        let (grid_rect, _) = ui.allocate_exact_size(avail, egui::Sense::hover());

        let total_w = grid_rect.width();
        let total_h = grid_rect.height();
        if total_w <= 0.0 || total_h <= 0.0 {
            return;
        }

        // ParseError: when sim load failed, replace the grid entirely
        // with an ink-muted error block. The grid's interactive
        // surfaces (splitters, drag handles, context menus) all
        // become inert until a fresh sim loads — the user can't
        // mutate a layout that has no panes worth arranging.
        if let Some(err) = last_error {
            paint_parse_error_block(ui, grid_rect, err);
            return;
        }

        // Pixel floors for the splitter clamps. Clamp the floor itself
        // so we never demand more than a fair share of the available
        // space (a 360×600 window can't honour 180×120 cells in five
        // rows — give in to the floor instead of going negative).
        let min_w_frac = (MIN_CELL_W_PX / total_w).clamp(0.05, 0.45);
        let min_h_frac = (MIN_CELL_H_PX / total_h).clamp(0.05, 0.40);

        // Renormalise rows + clamp column to keep the layout sane even
        // after a window resize that shrunk one axis below the floor.
        renormalise_rows(&mut self.row_fracs, min_h_frac);
        self.column_split = self.column_split.clamp(min_w_frac, 1.0 - min_w_frac);

        let col_widths = [
            self.column_split * total_w,
            (1.0 - self.column_split) * total_w,
        ];
        let col_starts = [0.0_f32, col_widths[0]];

        let row_heights: [f32; ROWS] = std::array::from_fn(|i| self.row_fracs[i] * total_h);
        let row_starts = cumulative_starts(&row_heights);

        let link_group = egui::Id::new("v2-shared-cursor");
        let base_state = if sim.is_none() {
            PaneState::NoRun
        } else {
            PaneState::Loaded
        };

        // Read drag-and-drop state for the reorder gesture (Pass 4).
        // `drag_source` is `Some` whenever the pointer is dragging a
        // payload set by a header click-and-drag; `None` when no
        // reorder is in progress. Resolved before cell rendering so
        // every cell can pick the correct cursor icon and so the
        // post-pass overlay knows which cell to highlight.
        let drag_source: Option<DragPayload> =
            egui::DragAndDrop::payload::<DragPayload>(ui.ctx()).map(|p| *p);
        let pointer_pos = ui.ctx().pointer_latest_pos();

        // Render cells. col_span == 0 means "continuation of a wide
        // cell at col-1" — skip entirely, no chrome, no body.
        // col_span == 2 means "wide cell" — rect width covers both
        // columns.
        for row in 0..ROWS {
            for col in 0..COLS {
                let span = self.col_spans[row][col];
                if span == 0 {
                    continue;
                }
                let width = if span >= 2 && col + 1 < COLS {
                    col_widths[col] + col_widths[col + 1]
                } else {
                    col_widths[col]
                };
                let cell_min = grid_rect.min + egui::vec2(col_starts[col], row_starts[row]);
                let cell_size = egui::vec2(width, row_heights[row]);
                let cell_rect = egui::Rect::from_min_size(cell_min, cell_size);

                let pane = &mut self.cells[row][col];
                let state = resolve_state_for(pane, sim, &base_state);
                let reset_zoom = self.pending_reset_zoom.is_some_and(|p| p == (row, col));
                let ctx = PaneCtx {
                    sim,
                    cursor_layer,
                    link_group,
                    state,
                    pinch_delta,
                    reset_zoom,
                    slice_masks,
                };

                let action = draw_cell(ui, cell_rect, row, col, pane, &ctx, drag_source.is_some());
                match action {
                    CellAction::None => {}
                    CellAction::ResetZoom => {
                        self.pending_reset_zoom = Some((row, col));
                    }
                    CellAction::Hide => {
                        let slot_id = if col == 0 {
                            PaneId::EmptySlot1
                        } else {
                            PaneId::EmptySlot2
                        };
                        self.cells[row][col] = Pane::from_id(slot_id);
                        self.dirty_at = Some(Instant::now());
                    }
                    CellAction::ResetLayout => {
                        let mut fresh = PaneGrid::default_layout();
                        std::mem::swap(&mut self.cells, &mut fresh.cells);
                        self.column_split = fresh.column_split;
                        self.row_fracs = fresh.row_fracs;
                        self.col_spans = fresh.col_spans;
                        self.dirty_at = Some(Instant::now());
                    }
                }
            }
        }

        // Pending reset-zoom fires for exactly one frame. If a pane
        // missed it (e.g. it was in NoRun state), clear regardless —
        // a stale pending flag would re-fire on the next sim load.
        self.pending_reset_zoom = None;

        // Drop-target overlay: when a drag is active and the pointer
        // is over a different cell than the source, paint a
        // 2px ink-muted border on the candidate target. No glow, no
        // shadow — DESIGN.md §4 No-Glass Rule. The overlay is
        // painted AFTER all cells render so it lands on top.
        if let (Some(source), Some(pos)) = (drag_source, pointer_pos) {
            let local_x = pos.x - grid_rect.min.x;
            let local_y = pos.y - grid_rect.min.y;
            if let Some((target_row, target_col)) = cell_at_pos(
                local_x,
                local_y,
                total_w,
                total_h,
                self.column_split,
                &self.row_fracs,
            ) {
                if (target_row, target_col) != (source.source_row, source.source_col) {
                    let target_min =
                        grid_rect.min + egui::vec2(col_starts[target_col], row_starts[target_row]);
                    let target_size = egui::vec2(col_widths[target_col], row_heights[target_row]);
                    let target_rect = egui::Rect::from_min_size(target_min, target_size);
                    ui.painter().rect_stroke(
                        target_rect.shrink(1.0),
                        egui::CornerRadius::ZERO,
                        egui::Stroke::new(2.0_f32, theme::INK_MUTED),
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }

        // Column splitter (between col 0 and col 1, full grid height).
        {
            let col_x = grid_rect.min.x + col_starts[1];
            let hit_rect = egui::Rect::from_min_size(
                egui::pos2(col_x - SPLITTER_HIT_PX * 0.5, grid_rect.min.y),
                egui::vec2(SPLITTER_HIT_PX, total_h),
            );
            let id = ui.id().with("v2-col-splitter");
            let resp = ui
                .interact(hit_rect, id, egui::Sense::drag())
                .on_hover_cursor(egui::CursorIcon::ResizeColumn);
            if resp.dragged() {
                let dx_frac = resp.drag_delta().x / total_w;
                apply_column_split_delta(&mut self.column_split, dx_frac, min_w_frac);
                self.dirty_at = Some(Instant::now());
            }
            paint_splitter_line(
                ui,
                [
                    egui::pos2(col_x, grid_rect.min.y),
                    egui::pos2(col_x, grid_rect.max.y),
                ],
                resp.hovered() || resp.dragged(),
            );
        }

        // Row splitters (between rows i and i+1, for i in 0..ROWS-1,
        // full grid width).
        for i in 0..ROWS - 1 {
            let row_y = grid_rect.min.y + row_starts[i + 1];
            let hit_rect = egui::Rect::from_min_size(
                egui::pos2(grid_rect.min.x, row_y - SPLITTER_HIT_PX * 0.5),
                egui::vec2(total_w, SPLITTER_HIT_PX),
            );
            let id = ui.id().with(("v2-row-splitter", i));
            let resp = ui
                .interact(hit_rect, id, egui::Sense::drag())
                .on_hover_cursor(egui::CursorIcon::ResizeRow);
            if resp.dragged() {
                let dy_frac = resp.drag_delta().y / total_h;
                apply_row_splitter_delta(&mut self.row_fracs, i, dy_frac, min_h_frac);
                self.dirty_at = Some(Instant::now());
            }
            paint_splitter_line(
                ui,
                [
                    egui::pos2(grid_rect.min.x, row_y),
                    egui::pos2(grid_rect.max.x, row_y),
                ],
                resp.hovered() || resp.dragged(),
            );
        }

        // Reorder finalisation: on pointer release, take the payload
        // and swap source ↔ target if the pointer is over a different
        // cell. If the pointer is outside the grid or over the same
        // cell, the payload is dropped (released without consumption)
        // and the next frame sees no drag.
        let released = ui.ctx().input(|i| i.pointer.any_released());
        if released {
            if let Some(payload) = egui::DragAndDrop::take_payload::<DragPayload>(ui.ctx()) {
                if let Some(pos) = pointer_pos {
                    let local_x = pos.x - grid_rect.min.x;
                    let local_y = pos.y - grid_rect.min.y;
                    if let Some((target_row, target_col)) = cell_at_pos(
                        local_x,
                        local_y,
                        total_w,
                        total_h,
                        self.column_split,
                        &self.row_fracs,
                    ) {
                        if (target_row, target_col) != (payload.source_row, payload.source_col) {
                            swap_cells(
                                &mut self.cells,
                                (payload.source_row, payload.source_col),
                                (target_row, target_col),
                            );
                            self.dirty_at = Some(Instant::now());
                        }
                    }
                }
            }
        }

        // Escape cancels an in-progress drag without consuming.
        if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
            let _ = egui::DragAndDrop::take_payload::<DragPayload>(ui.ctx());
        }
    }
}

/// Resolve a per-pane `PaneState`.
///
/// Empty slots are always `Loaded` — they have no real data
/// dependency. Other panes inherit the dashboard's base state
/// (`Loaded` or `NoRun`); when sim is loaded, `detect_field_missing`
/// runs against the pane's `required_fields()` and may downgrade
/// `Loaded` → `FieldMissing(name)`. Today this is a no-op (schema
/// v1 has every field) but the dispatch is wired so future
/// schema-aware loading lights the muted note up automatically.
fn resolve_state_for(
    pane: &Pane,
    sim: Option<&PrintSimulation>,
    base_state: &PaneState,
) -> PaneState {
    if matches!(pane, Pane::EmptySlot(_)) {
        return PaneState::Loaded;
    }
    if let (PaneState::Loaded, Some(s)) = (base_state, sim) {
        if let Some(field) = detect_field_missing(s, pane.required_fields()) {
            return PaneState::FieldMissing(field);
        }
    }
    base_state.clone()
}

/// Paint the brief §6 ParseError block: the grid is replaced by an
/// ink-muted message identifying that the most recent sim load
/// failed, plus the verbatim error string and a recovery hint. No
/// emoji, no "oops" — DESIGN.md §6 No-Narration rule.
fn paint_parse_error_block(ui: &mut egui::Ui, rect: egui::Rect, error_text: &str) {
    let builder = egui::UiBuilder::new()
        .max_rect(rect.shrink(24.0))
        .layout(egui::Layout::top_down(egui::Align::Min));
    ui.scope_builder(builder, |ui| {
        ui.label(
            egui::RichText::new("Parse error")
                .strong()
                .color(theme::INK)
                .size(15.0),
        );
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(error_text)
                .monospace()
                .color(theme::INK_MUTED),
        );
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("Restart with a valid --load-sim <PATH.sim.json> to retry.")
                .small()
                .color(theme::INK_MUTED),
        );
    });
}

/// Action requested by the cell's right-click context menu, if any.
/// Returned to `PaneGrid::render` so it can mutate `self` (which the
/// helper can't, since it borrows `&mut self.cells` already).
enum CellAction {
    None,
    ResetZoom,
    Hide,
    ResetLayout,
}

/// Render one cell's chrome + body at a fixed rect. Uses
/// `ui.scope_builder` so the cell content is written into the
/// requested rect rather than appended to the parent's layout
/// cursor.
///
/// The top `HEADER_DRAG_HANDLE_PX` of the cell is allocated as a
/// click-and-drag handle for reorder. Hovering it shows
/// `CursorIcon::Grab`; an active drag from any cell shows
/// `Grabbing`. The same response carries the right-click context
/// menu (Reset zoom / Hide pane / Reset layout) per Pass 5.
fn draw_cell(
    ui: &mut egui::Ui,
    cell_rect: egui::Rect,
    row: usize,
    col: usize,
    pane: &mut Pane,
    ctx: &PaneCtx<'_>,
    drag_active: bool,
) -> CellAction {
    // Inset 1px so the splitter line and the cell border don't
    // overlap at exactly the same pixel.
    let inner_rect = cell_rect.shrink(1.0);

    // Header drag handle: top strip of the cell. Allocated BEFORE the
    // cell body so the interact zone takes precedence over any
    // header-area widgets. Cursor icon flips Grab → Grabbing during
    // an active drag.
    let header_rect = egui::Rect::from_min_size(
        inner_rect.min,
        egui::vec2(inner_rect.width(), HEADER_DRAG_HANDLE_PX),
    );
    let drag_id = ui.id().with(("v2-cell-drag", row, col));
    let header_resp = ui
        .interact(header_rect, drag_id, egui::Sense::click_and_drag())
        .on_hover_cursor(if drag_active {
            egui::CursorIcon::Grabbing
        } else {
            egui::CursorIcon::Grab
        });
    if header_resp.drag_started() {
        egui::DragAndDrop::set_payload(
            ui.ctx(),
            DragPayload {
                source_row: row,
                source_col: col,
            },
        );
    }

    // Right-click context menu (open Q #8 locks "no-op outside
    // header"). Items: Reset zoom (this pane), Hide pane (replace
    // with EmptySlot, dirties layout), Reset layout (restore default,
    // dirties layout). Hide pane is hidden when the cell is already
    // an EmptySlot — there's nothing to hide.
    let mut action = CellAction::None;
    let is_empty = matches!(pane, Pane::EmptySlot(_));
    header_resp.context_menu(|ui| {
        if ui.button("Reset zoom").clicked() {
            action = CellAction::ResetZoom;
            ui.close();
        }
        if !is_empty && ui.button("Hide pane").clicked() {
            action = CellAction::Hide;
            ui.close();
        }
        ui.separator();
        if ui.button("Reset layout").clicked() {
            action = CellAction::ResetLayout;
            ui.close();
        }
    });

    let builder = egui::UiBuilder::new()
        .max_rect(inner_rect)
        .layout(egui::Layout::top_down(egui::Align::Min));
    ui.scope_builder(builder, |ui| {
        egui::Frame::default()
            .fill(theme::SURFACE_BASE)
            .stroke(egui::Stroke::new(1.0_f32, theme::GRID_LINE))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.set_min_size(inner_rect.size() - egui::vec2(2.0, 2.0));
                pane.render(ui, ctx);
            });
    });

    action
}

/// Paint a splitter line at the given segment. `active` is true when
/// the splitter is hovered or being dragged — paints the stronger
/// `INK_MUTED` line; otherwise paints the calmer `GRID_LINE`. The
/// width also bumps so the splitter's interactivity is visually
/// confirmed without needing a separate hover animation.
fn paint_splitter_line(ui: &egui::Ui, points: [egui::Pos2; 2], active: bool) {
    let (color, width) = if active {
        (theme::INK_MUTED, SPLITTER_VIS_HOVER_PX)
    } else {
        (theme::GRID_LINE, SPLITTER_VIS_PX)
    };
    ui.painter()
        .line_segment(points, egui::Stroke::new(width, color));
}

// ---------------------------------------------------------------------
// Pure helpers — the `bevy-app-test-seam` for splitter math. No egui,
// no Bevy, no `EguiPlugin`. Unit-tested below.
// ---------------------------------------------------------------------

/// Apply a column-splitter drag delta (in fractional grid widths) to
/// `column_split`, clamped so neither column shrinks below `min_frac`.
pub fn apply_column_split_delta(column_split: &mut f32, delta_frac: f32, min_frac: f32) {
    *column_split = (*column_split + delta_frac).clamp(min_frac, 1.0 - min_frac);
}

/// Apply a row-splitter drag delta (in fractional grid heights) to
/// the two adjacent rows. Splitter `idx` lives between rows `idx` and
/// `idx + 1`. Positive `delta_frac` grows row `idx` and shrinks row
/// `idx + 1`; the delta is clamped so neither row drops below
/// `min_frac`. The total of `fracs` is preserved by construction.
pub fn apply_row_splitter_delta(
    fracs: &mut [f32; ROWS],
    idx: usize,
    delta_frac: f32,
    min_frac: f32,
) {
    if idx >= ROWS - 1 {
        return;
    }
    let above = fracs[idx];
    let below = fracs[idx + 1];
    let max_grow = (below - min_frac).max(0.0);
    let max_shrink = (above - min_frac).max(0.0);
    let allowed = delta_frac.clamp(-max_shrink, max_grow);
    fracs[idx] = above + allowed;
    fracs[idx + 1] = below - allowed;
}

/// Cumulative starting positions for a slice of segment lengths.
/// `cumulative_starts(&[10, 20, 30])` returns `[0, 10, 30]` —
/// position[i] = sum of lengths[0..i]. Used to convert row heights
/// into row Y origins.
pub fn cumulative_starts(lengths: &[f32; ROWS]) -> [f32; ROWS] {
    let mut starts = [0.0_f32; ROWS];
    for i in 1..ROWS {
        starts[i] = starts[i - 1] + lengths[i - 1];
    }
    starts
}

/// Validate / sanitise a col_spans pattern. Allowed: every row is
/// either `[1, 1]` (two single cells) or `[2, 0]` (a wide cell at
/// col 0). Any other pattern is forced to `[1, 1]` so the renderer
/// doesn't get into an undefined state. Pure helper for tests.
pub fn sanitised_col_spans(spans: [[u8; COLS]; ROWS]) -> [[u8; COLS]; ROWS] {
    let mut out = [[1_u8; COLS]; ROWS];
    for r in 0..ROWS {
        if spans[r] == [2, 0] {
            out[r] = [2, 0];
        } else {
            out[r] = [1, 1];
        }
    }
    out
}

/// Locate the (row, col) cell under a position expressed in
/// grid-local coordinates (origin at the grid's top-left). Returns
/// `None` when the position is outside the grid bounds. Edge ties
/// resolve to the lower-index cell.
///
/// Used by the Pass 4 reorder logic to translate the pointer
/// position into a drop target. Pure helper, unit-tested.
pub fn cell_at_pos(
    local_x: f32,
    local_y: f32,
    total_w: f32,
    total_h: f32,
    column_split: f32,
    row_fracs: &[f32; ROWS],
) -> Option<(usize, usize)> {
    if !(0.0..=total_w).contains(&local_x) || !(0.0..=total_h).contains(&local_y) {
        return None;
    }
    let col = if local_x < column_split * total_w {
        0
    } else {
        1
    };
    let mut accum = 0.0_f32;
    for (i, frac) in row_fracs.iter().enumerate() {
        accum += frac * total_h;
        if local_y <= accum {
            return Some((i, col));
        }
    }
    Some((ROWS - 1, col))
}

/// Swap two cells in place. Pure helper for the reorder gesture's
/// "swap, not insert+shift" model (open Q #4). Identity swap (same
/// cell on both sides) is a no-op.
pub fn swap_cells(cells: &mut [[Pane; COLS]; ROWS], a: (usize, usize), b: (usize, usize)) {
    if a == b {
        return;
    }
    // SAFETY: indices are bounded by callers (drag payload + cell_at_pos
    // both produce in-range tuples). `swap` requires distinct indices,
    // which we just proved with the early-return above.
    if a.0 == b.0 {
        cells[a.0].swap(a.1, b.1);
    } else {
        // Different rows — split-borrow trick: split the array at the
        // higher row so we get two non-aliasing slices.
        let (lo, hi) = if a.0 < b.0 { (a, b) } else { (b, a) };
        let (left, right) = cells.split_at_mut(hi.0);
        std::mem::swap(&mut left[lo.0][lo.1], &mut right[0][hi.1]);
    }
}

/// Renormalise row fractions so they sum to 1.0, after clamping each
/// row to at least `min_frac`. Defends against accumulated rounding
/// drift across many splitter drags and against a window-resize
/// shrinking one axis below the floor.
pub fn renormalise_rows(fracs: &mut [f32; ROWS], min_frac: f32) {
    // Clamp each row up to the floor first.
    for f in fracs.iter_mut() {
        if !f.is_finite() || *f < min_frac {
            *f = min_frac;
        }
    }
    let total: f32 = fracs.iter().sum();
    if total > 0.0 {
        for f in fracs.iter_mut() {
            *f /= total;
        }
    } else {
        // Degenerate case (every row was non-finite): reset to equal.
        let equal = 1.0 / ROWS as f32;
        for f in fracs.iter_mut() {
            *f = equal;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_layout_has_ten_distinct_panes() {
        let grid = PaneGrid::default_layout();
        let mut ids = std::collections::HashSet::new();
        for row in 0..ROWS {
            for col in 0..COLS {
                let id = grid.cells[row][col].id();
                assert!(
                    ids.insert(id),
                    "duplicate pane id at ({row}, {col}): {id:?}"
                );
            }
        }
        assert_eq!(ids.len(), 10);
    }

    #[test]
    fn default_layout_places_forces_top_left() {
        let grid = PaneGrid::default_layout();
        assert_eq!(grid.cells[0][0].id(), PaneId::Forces);
    }

    #[test]
    fn default_layout_layer_mask_spans_bottom_row() {
        let grid = PaneGrid::default_layout();
        assert_eq!(grid.cells[ROWS - 1][0].id(), PaneId::LayerMask2d);
        assert_eq!(grid.col_spans[ROWS - 1], [2, 0]);
    }

    #[test]
    fn default_layout_other_rows_are_single_cells() {
        let grid = PaneGrid::default_layout();
        for r in 0..(ROWS - 1) {
            assert_eq!(
                grid.col_spans[r],
                [1, 1],
                "non-bottom row {r} unexpectedly spans"
            );
        }
    }

    #[test]
    fn sanitised_col_spans_accepts_canonical_patterns() {
        let canonical = [[1, 1], [1, 1], [1, 1], [1, 1], [2, 0]];
        assert_eq!(sanitised_col_spans(canonical), canonical);
        let all_single = [[1_u8; COLS]; ROWS];
        assert_eq!(sanitised_col_spans(all_single), all_single);
    }

    #[test]
    fn sanitised_col_spans_rejects_other_patterns() {
        // Span starting at col 1 (would extend out of grid) — reset.
        let bad = [[1, 2], [1, 1], [1, 1], [1, 1], [1, 1]];
        let cleaned = sanitised_col_spans(bad);
        assert_eq!(cleaned[0], [1, 1]);
        // Span value 3 — reset.
        let bad = [[3, 0], [1, 1], [1, 1], [1, 1], [1, 1]];
        assert_eq!(sanitised_col_spans(bad)[0], [1, 1]);
        // Continuation without a leading span — reset.
        let bad = [[0, 1], [1, 1], [1, 1], [1, 1], [1, 1]];
        assert_eq!(sanitised_col_spans(bad)[0], [1, 1]);
    }

    #[test]
    fn default_layout_starts_with_equal_columns_and_rows() {
        let grid = PaneGrid::default_layout();
        assert!((grid.column_split - 0.5).abs() < 1e-6);
        for &f in &grid.row_fracs {
            assert!((f - 0.2).abs() < 1e-6);
        }
    }

    // ---- column splitter ----

    #[test]
    fn column_splitter_grows_col0_within_clamp() {
        let mut split = 0.5;
        apply_column_split_delta(&mut split, 0.1, 0.1);
        assert!((split - 0.6).abs() < 1e-6);
    }

    #[test]
    fn column_splitter_clamps_at_max() {
        let mut split = 0.85;
        apply_column_split_delta(&mut split, 0.5, 0.1);
        // 1.0 - min_frac = 0.9 is the ceiling.
        assert!((split - 0.9).abs() < 1e-6);
    }

    #[test]
    fn column_splitter_clamps_at_min() {
        let mut split = 0.15;
        apply_column_split_delta(&mut split, -0.5, 0.1);
        assert!((split - 0.1).abs() < 1e-6);
    }

    // ---- row splitter ----

    #[test]
    fn row_splitter_redistributes_between_neighbours() {
        let mut fracs = [0.2; ROWS];
        apply_row_splitter_delta(&mut fracs, 1, 0.05, 0.1);
        assert!((fracs[1] - 0.25).abs() < 1e-6);
        assert!((fracs[2] - 0.15).abs() < 1e-6);
        // Other rows untouched.
        for i in [0, 3, 4] {
            assert!((fracs[i] - 0.2).abs() < 1e-6);
        }
        // Sum preserved.
        let total: f32 = fracs.iter().sum();
        assert!((total - 1.0).abs() < 1e-6);
    }

    #[test]
    fn row_splitter_clamps_when_below_would_underflow() {
        // Below row already at the floor; a positive delta can't
        // shrink it further.
        let mut fracs = [0.3, 0.3, 0.1, 0.15, 0.15];
        apply_row_splitter_delta(&mut fracs, 1, 0.5, 0.1);
        // Row 2 is at the floor (0.1) and stays there.
        assert!((fracs[2] - 0.1).abs() < 1e-6);
        // Row 1 absorbed only what the floor allowed (0.0 grow).
        assert!((fracs[1] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn row_splitter_clamps_when_above_would_underflow() {
        let mut fracs = [0.3, 0.1, 0.3, 0.15, 0.15];
        apply_row_splitter_delta(&mut fracs, 1, -0.5, 0.1);
        assert!((fracs[1] - 0.1).abs() < 1e-6);
        assert!((fracs[2] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn row_splitter_idx_at_or_past_end_is_noop() {
        let original = [0.2; ROWS];
        let mut fracs = original;
        apply_row_splitter_delta(&mut fracs, ROWS - 1, 0.5, 0.1);
        assert_eq!(fracs, original);
        let mut fracs = original;
        apply_row_splitter_delta(&mut fracs, 99, 0.5, 0.1);
        assert_eq!(fracs, original);
    }

    // ---- cumulative_starts ----

    #[test]
    fn cumulative_starts_basic() {
        let lengths = [10.0, 20.0, 30.0, 40.0, 50.0];
        let starts = cumulative_starts(&lengths);
        assert_eq!(starts, [0.0, 10.0, 30.0, 60.0, 100.0]);
    }

    #[test]
    fn cumulative_starts_with_zero() {
        let lengths = [0.0; ROWS];
        let starts = cumulative_starts(&lengths);
        assert_eq!(starts, [0.0; ROWS]);
    }

    // ---- renormalise_rows ----

    #[test]
    fn renormalise_rows_unchanged_when_already_normal() {
        let mut fracs = [0.2; ROWS];
        renormalise_rows(&mut fracs, 0.1);
        for &f in &fracs {
            assert!((f - 0.2).abs() < 1e-6);
        }
    }

    #[test]
    fn renormalise_rows_scales_when_off() {
        let mut fracs = [0.4; ROWS]; // sums to 2.0
        renormalise_rows(&mut fracs, 0.1);
        let total: f32 = fracs.iter().sum();
        assert!((total - 1.0).abs() < 1e-6);
    }

    #[test]
    fn renormalise_rows_floors_below_min() {
        let mut fracs = [0.5, 0.05, 0.4, 0.0, -0.1];
        renormalise_rows(&mut fracs, 0.1);
        for &f in &fracs {
            // After clamp + renorm, every row is at least the floor *
            // (renorm factor); floor is 0.1, total before renorm is
            // 0.5+0.1+0.4+0.1+0.1 = 1.2, so min after renorm is
            // 0.1/1.2 ≈ 0.083 — still above 0 and finite.
            assert!(f.is_finite() && f > 0.0, "row went negative: {f}");
        }
        let total: f32 = fracs.iter().sum();
        assert!((total - 1.0).abs() < 1e-6);
    }

    #[test]
    fn renormalise_rows_handles_all_non_finite() {
        let mut fracs = [f32::NAN; ROWS];
        renormalise_rows(&mut fracs, 0.1);
        // Each NaN was clamped to min_frac (0.1), giving total 0.5;
        // renorm scales every row to 0.2.
        for &f in &fracs {
            assert!((f - 0.2).abs() < 1e-6);
        }
    }

    // ---- cell_at_pos ----

    #[test]
    fn cell_at_pos_top_left() {
        let fracs = [0.2; ROWS];
        assert_eq!(
            cell_at_pos(50.0, 50.0, 1000.0, 500.0, 0.5, &fracs),
            Some((0, 0))
        );
    }

    #[test]
    fn cell_at_pos_top_right() {
        let fracs = [0.2; ROWS];
        assert_eq!(
            cell_at_pos(750.0, 50.0, 1000.0, 500.0, 0.5, &fracs),
            Some((0, 1))
        );
    }

    #[test]
    fn cell_at_pos_bottom_right() {
        let fracs = [0.2; ROWS];
        assert_eq!(
            cell_at_pos(750.0, 450.0, 1000.0, 500.0, 0.5, &fracs),
            Some((4, 1))
        );
    }

    #[test]
    fn cell_at_pos_outside_grid_is_none() {
        let fracs = [0.2; ROWS];
        assert_eq!(cell_at_pos(-1.0, 50.0, 1000.0, 500.0, 0.5, &fracs), None);
        assert_eq!(cell_at_pos(50.0, 600.0, 1000.0, 500.0, 0.5, &fracs), None);
        assert_eq!(cell_at_pos(1500.0, 50.0, 1000.0, 500.0, 0.5, &fracs), None);
    }

    #[test]
    fn cell_at_pos_uneven_columns() {
        let fracs = [0.2; ROWS];
        // column_split = 0.7 means col 0 occupies x ∈ [0, 700].
        assert_eq!(
            cell_at_pos(650.0, 250.0, 1000.0, 500.0, 0.7, &fracs),
            Some((2, 0))
        );
        assert_eq!(
            cell_at_pos(750.0, 250.0, 1000.0, 500.0, 0.7, &fracs),
            Some((2, 1))
        );
    }

    #[test]
    fn cell_at_pos_uneven_rows() {
        // First row is 50% of height, others share remaining 50%.
        let fracs = [0.5, 0.125, 0.125, 0.125, 0.125];
        assert_eq!(
            cell_at_pos(100.0, 200.0, 1000.0, 500.0, 0.5, &fracs),
            Some((0, 0))
        );
        // y = 300 is in row 1 (which spans y ∈ (250, 312.5]).
        assert_eq!(
            cell_at_pos(100.0, 300.0, 1000.0, 500.0, 0.5, &fracs),
            Some((1, 0))
        );
    }

    // ---- swap_cells ----

    #[test]
    fn swap_cells_same_row_swaps() {
        let mut grid = PaneGrid::default_layout();
        let before_00 = grid.cells[0][0].id();
        let before_01 = grid.cells[0][1].id();
        swap_cells(&mut grid.cells, (0, 0), (0, 1));
        assert_eq!(grid.cells[0][0].id(), before_01);
        assert_eq!(grid.cells[0][1].id(), before_00);
    }

    #[test]
    fn swap_cells_different_rows_swaps() {
        let mut grid = PaneGrid::default_layout();
        let before_00 = grid.cells[0][0].id();
        let before_31 = grid.cells[3][1].id();
        swap_cells(&mut grid.cells, (0, 0), (3, 1));
        assert_eq!(grid.cells[0][0].id(), before_31);
        assert_eq!(grid.cells[3][1].id(), before_00);
    }

    #[test]
    fn swap_cells_identity_is_noop() {
        let mut grid = PaneGrid::default_layout();
        let before = grid.cells[2][1].id();
        swap_cells(&mut grid.cells, (2, 1), (2, 1));
        assert_eq!(grid.cells[2][1].id(), before);
    }

    #[test]
    fn swap_cells_two_swaps_round_trip() {
        let mut grid = PaneGrid::default_layout();
        let before_00 = grid.cells[0][0].id();
        let before_42 = grid.cells[4][1].id(); // EmptySlot2
        swap_cells(&mut grid.cells, (0, 0), (4, 1));
        swap_cells(&mut grid.cells, (4, 1), (0, 0));
        assert_eq!(grid.cells[0][0].id(), before_00);
        assert_eq!(grid.cells[4][1].id(), before_42);
    }
}
