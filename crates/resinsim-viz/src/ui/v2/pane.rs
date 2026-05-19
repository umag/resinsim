//! Pane abstraction for the v2 dashboard.
//!
//! Every cell in the 5×2 grid is a [`Pane`]. The `PaneId` enum is the
//! stable identity used for layout persistence and reorder ops; the
//! `Pane` enum holds the per-variant view-state and dispatches
//! rendering to the matching module under `panes/`. Per-variant view
//! state is empty in Pass 1 (no log-scale toggles, no per-pane Y zoom
//! yet); state will accrete on later passes without changing the
//! enum shape.
//!
//! [`PaneState`] is the *render mode*: per-pane override that says
//! "data is loading", "the field this pane needs is missing in the
//! sim.json schema", or "no run loaded yet". The grid evaluates this
//! per pane and renders chrome consistently regardless of which pane
//! variant is inside.

use bevy_egui::egui;
use resinsim_core::io::sliced::LayerInput;
use resinsim_core::simulation::PrintSimulation;

use super::panes::{
    AreaDeltaPane, CureDepthPane, EmptySlotPane, ForcesPane, LayerMask2dPane, SafetyPane,
    VatTempPane, ViscosityPane, ZDeflectionPane,
};
use super::theme;

/// Stable identity for a pane. Used as the egui ID prefix, in layout
/// persistence (Pass 5), and in reorder operations (Pass 4). Order
/// matters only insofar as it dictates the default layout sequence.
///
/// Variant names are the on-disk identifier in the persisted layout
/// JSON (via serde). Renaming a variant requires bumping
/// `LAYOUT_SCHEMA_VERSION` so older files reset to default rather
/// than silently mapping to the wrong pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PaneId {
    Forces,
    Safety,
    CureDepth,
    VatTemp,
    AreaDelta,
    Viscosity,
    ZDeflection,
    LayerMask2d,
    /// Reserved slot 1. Hidden behind a muted "(empty slot)" placeholder
    /// in slice A; an Add-Pane UX is out of scope for now (open Q #2).
    EmptySlot1,
    /// Reserved slot 2. Same as above.
    EmptySlot2,
}

impl PaneId {
    /// egui Id seed for this pane. Used to namespace egui_plot Plot
    /// IDs and any future per-pane state cached by egui.
    pub fn egui_id(self) -> egui::Id {
        egui::Id::new(("v2-pane", self as u32))
    }
}

/// Per-variant view-state and dispatch. New panes are added here and
/// in `panes/`; the grid never matches on `PaneId` for rendering, only
/// for identity.
pub enum Pane {
    Forces(ForcesPane),
    Safety(SafetyPane),
    CureDepth(CureDepthPane),
    VatTemp(VatTempPane),
    AreaDelta(AreaDeltaPane),
    Viscosity(ViscosityPane),
    ZDeflection(ZDeflectionPane),
    LayerMask2d(LayerMask2dPane),
    EmptySlot(EmptySlotPane),
}

impl Pane {
    /// Construct a freshly-defaulted pane from its [`PaneId`]. Used by
    /// the layout-persistence reload path and by the context menu's
    /// "Hide pane" command (which swaps a real pane for `EmptySlot`).
    /// Panes have no per-instance view-state in slice A so a default
    /// is enough; future passes that add view-state will need a
    /// heavier reload story.
    pub fn from_id(id: PaneId) -> Self {
        match id {
            PaneId::Forces => Pane::Forces(ForcesPane::default()),
            PaneId::Safety => Pane::Safety(SafetyPane::default()),
            PaneId::CureDepth => Pane::CureDepth(CureDepthPane::default()),
            PaneId::VatTemp => Pane::VatTemp(VatTempPane::default()),
            PaneId::AreaDelta => Pane::AreaDelta(AreaDeltaPane::default()),
            PaneId::Viscosity => Pane::Viscosity(ViscosityPane::default()),
            PaneId::ZDeflection => Pane::ZDeflection(ZDeflectionPane::default()),
            PaneId::LayerMask2d => Pane::LayerMask2d(LayerMask2dPane::default()),
            PaneId::EmptySlot1 => Pane::EmptySlot(EmptySlotPane {
                slot: PaneId::EmptySlot1,
            }),
            PaneId::EmptySlot2 => Pane::EmptySlot(EmptySlotPane {
                slot: PaneId::EmptySlot2,
            }),
        }
    }

    pub fn id(&self) -> PaneId {
        match self {
            Pane::Forces(_) => PaneId::Forces,
            Pane::Safety(_) => PaneId::Safety,
            Pane::CureDepth(_) => PaneId::CureDepth,
            Pane::VatTemp(_) => PaneId::VatTemp,
            Pane::AreaDelta(_) => PaneId::AreaDelta,
            Pane::Viscosity(_) => PaneId::Viscosity,
            Pane::ZDeflection(_) => PaneId::ZDeflection,
            Pane::LayerMask2d(_) => PaneId::LayerMask2d,
            Pane::EmptySlot(p) => p.slot,
        }
    }

    /// `LayerResult` field names this pane needs to render its
    /// `Loaded` state. Used by [`detect_field_missing`] to decide
    /// whether to fall back to `PaneState::FieldMissing(name)`.
    /// Empty slots and the layer-mask 2D pane don't read any
    /// `LayerResult` field (geometry comes from `LoadedSliceStack`).
    pub fn required_fields(&self) -> &'static [&'static str] {
        match self {
            Pane::Forces(_) => &[
                "peel_force_n",
                "suction_force_n",
                "total_force_n",
                "support_capacity_n",
            ],
            Pane::Safety(_) => &["safety_factor"],
            Pane::CureDepth(_) => &[
                "cure_depth_um",
                "worst_cure_depth_um",
                "effective_layer_height_um",
            ],
            Pane::VatTemp(_) => &["vat_temperature_c"],
            Pane::AreaDelta(_) => &["cross_section_area_mm2", "area_delta_mm2"],
            Pane::Viscosity(_) => &["viscosity_mpa_s"],
            Pane::ZDeflection(_) => &["z_deflection_um"],
            Pane::LayerMask2d(_) | Pane::EmptySlot(_) => &[],
        }
    }

    /// Render this pane into `ui`. The pane is responsible for its
    /// own header strip and body; it does NOT render the cell border
    /// or background (the grid owns chrome).
    pub fn render(&mut self, ui: &mut egui::Ui, ctx: &PaneCtx<'_>) {
        match self {
            Pane::Forces(p) => p.render(ui, ctx),
            Pane::Safety(p) => p.render(ui, ctx),
            Pane::CureDepth(p) => p.render(ui, ctx),
            Pane::VatTemp(p) => p.render(ui, ctx),
            Pane::AreaDelta(p) => p.render(ui, ctx),
            Pane::Viscosity(p) => p.render(ui, ctx),
            Pane::ZDeflection(p) => p.render(ui, ctx),
            Pane::LayerMask2d(p) => p.render(ui, ctx),
            Pane::EmptySlot(p) => p.render(ui, ctx),
        }
    }
}

/// Detect whether the loaded sim is missing any field this pane
/// needs. Today: stub. Schema v1 (`sim-json-canonical-interchange`,
/// ADR-0015) carries every `LayerResult` field unconditionally, so
/// no detection is possible without further work in resinsim-core
/// (e.g. `#[serde(default)]` on `LayerResult` fields plus a
/// `schema_version` field on each layer).
///
/// The helper exists so the grid can call it generically and the
/// extension point lands cleanly when the loader gains
/// schema-version awareness; today it always returns `None`.
pub fn detect_field_missing(
    _sim: &PrintSimulation,
    _required: &'static [&'static str],
) -> Option<&'static str> {
    // TODO(slice-A-followup): hook into resinsim-core's schema_version
    // once the canonical envelope tracks per-field provenance. The
    // `&'static str` return is a deliberate constraint — the field
    // name lives in `Pane::required_fields()`'s static slice and never
    // needs to be allocated.
    None
}

#[cfg(test)]
mod field_missing_tests {
    use super::*;
    use resinsim_core::repositories::load_from_path;
    use std::path::PathBuf;

    fn load_lilith_sim() -> resinsim_core::simulation::PrintSimulation {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("lilith-torso.sim.json");
        load_from_path(&path).expect("test fixture: lilith sim.json must load")
    }

    /// `Pane::required_fields()` lists every `LayerResult` field the
    /// pane reads; `detect_field_missing` returns `None` against a
    /// schema-v1 sim. This pin catches a future regression where
    /// `detect_field_missing` starts reporting valid fields as
    /// missing.
    #[test]
    fn no_pane_reports_field_missing_against_schema_v1_sim() {
        let sim = load_lilith_sim();
        let panes = [
            Pane::from_id(PaneId::Forces),
            Pane::from_id(PaneId::Safety),
            Pane::from_id(PaneId::CureDepth),
            Pane::from_id(PaneId::VatTemp),
            Pane::from_id(PaneId::AreaDelta),
            Pane::from_id(PaneId::Viscosity),
            Pane::from_id(PaneId::ZDeflection),
            Pane::from_id(PaneId::LayerMask2d),
            Pane::from_id(PaneId::EmptySlot1),
            Pane::from_id(PaneId::EmptySlot2),
        ];
        for pane in &panes {
            let missing = detect_field_missing(&sim, pane.required_fields());
            assert!(
                missing.is_none(),
                "schema v1 sim must not report any field missing for pane {:?}; got {missing:?}",
                pane.id()
            );
        }
    }

    /// Each non-empty pane declares at least one required field, so
    /// the dispatch in `resolve_state_for` reaches
    /// `detect_field_missing` for every data-bearing pane.
    /// EmptySlot and LayerMask2d declare zero (no LayerResult field
    /// dependency).
    #[test]
    fn data_panes_declare_at_least_one_required_field() {
        for id in [
            PaneId::Forces,
            PaneId::Safety,
            PaneId::CureDepth,
            PaneId::VatTemp,
            PaneId::AreaDelta,
            PaneId::Viscosity,
            PaneId::ZDeflection,
        ] {
            let pane = Pane::from_id(id);
            assert!(
                !pane.required_fields().is_empty(),
                "data pane {id:?} must declare at least one required field"
            );
        }
        assert!(Pane::from_id(PaneId::LayerMask2d)
            .required_fields()
            .is_empty());
        assert!(Pane::from_id(PaneId::EmptySlot1)
            .required_fields()
            .is_empty());
        assert!(Pane::from_id(PaneId::EmptySlot2)
            .required_fields()
            .is_empty());
    }
}

/// Render mode for a pane. The grid resolves this from `(sim,
/// required_fields)` once per frame and passes it to the pane via
/// `PaneCtx`. Panes paint differently based on it without each
/// having to know the load state of every other resource.
///
/// Three variants today; future work will reintroduce the variants
/// it needs:
///   - **Loading**: lands when an async drop-file path arrives
///     (slice D drag-drop sim swap).
///   - **ParseError**: the grid handles parse errors directly via
///     the `last_error` parameter to `render`, replacing the whole
///     dashboard with an ink-muted block (brief §6). Panes never
///     see this state, so the variant doesn't carry its weight.
///   - **NoCtb**: the layer-mask 2D pane currently hardcodes its
///     no-CTB message; when slice D wires `LoadedSliceStack` query
///     plumbing, this variant comes back.
#[derive(Debug, Clone)]
pub enum PaneState {
    /// Sim loaded, all required fields present. Render real data.
    Loaded,
    /// No sim loaded. Pane shows empty axes (grid only).
    NoRun,
    /// The field this pane needs is missing from the sim.json schema
    /// version. Pane shows axes only and a muted note.
    FieldMissing(&'static str),
}

/// Per-frame context handed to every pane. Constructed once at the
/// top of `draw_v2_dashboard` from Bevy resources. Borrows for the
/// duration of one egui pass; never stored.
pub struct PaneCtx<'a> {
    /// The currently-loaded simulation, if any. `None` is the NoRun
    /// case; panes should not be reached with `None` here unless their
    /// state is `NoRun`.
    pub sim: Option<&'a PrintSimulation>,
    /// Cursor layer index. Shared across every pane via the link
    /// group; written by the scrubber (slice D) and read here.
    pub cursor_layer: u32,
    /// Shared link-axis Id used by every pane's egui_plot. Pan/zoom
    /// on one pane mirrors X across every other. Constructed once per
    /// dashboard so every pane uses the same handle.
    pub link_group: egui::Id,
    /// Render mode for this specific pane. The grid resolves it from
    /// `sim` + `required_fields()` per pane per frame.
    pub state: PaneState,
    /// Accumulated trackpad pinch delta for this frame. Positive =
    /// fingers spreading apart (zoom in), negative = pinching together
    /// (zoom out). Each pane checks `plot_ui.response().hovered()`
    /// inside its closure and consumes this delta only when its plot
    /// is the hovered one — that way exactly one pane drives the
    /// zoom, X bounds propagate via `link_axis`, and Y stays local.
    /// See [`super::zoom`] for the math.
    pub pinch_delta: f32,
    /// True when this pane should reset its plot bounds to auto-fit
    /// on the current frame. Set by the right-click context menu's
    /// "Reset zoom" item. The pane's plot closure checks this and
    /// calls `plot_ui.set_auto_bounds([true, true])` if set; X
    /// propagates via `link_axis` so the whole dashboard re-fits
    /// the time window when one pane is reset.
    pub reset_zoom: bool,
    /// Per-layer slice-stack masks parsed from the most recent CTB
    /// load. `None` (or an empty slice) means no CTB is loaded; the
    /// `LayerMask2dPane` (slice E) falls back to the
    /// `(no CTB loaded, geometry unavailable)` placeholder. Other
    /// panes ignore this field.
    pub slice_masks: Option<&'a [LayerInput]>,
}

/// Render an empty plot frame with axes and labels but no data. Used
/// by the `NoRun`, `FieldMissing`, and `NoCtb` states. The frame
/// matches a real plot so a viewer scanning the dashboard sees a
/// coherent grid even when nothing is loaded.
///
/// `note` is an optional muted line painted under the plot. Pass
/// `Some("(field missing in sim.json schema vN)")` for the
/// FieldMissing state, `None` for plain empty.
pub fn empty_axis_placeholder(
    ui: &mut egui::Ui,
    pane_id: PaneId,
    x_label: &str,
    y_label: &str,
    link_group: egui::Id,
    note: Option<&str>,
) {
    // Render the note ABOVE the plot, not below — egui_plot inside an
    // unconstrained `Ui` claims the full available height, leaving no
    // space underneath for trailing labels. Note above keeps it
    // visible regardless of pane height.
    if let Some(text) = note {
        ui.label(
            egui::RichText::new(text)
                .monospace()
                .small()
                .color(theme::INK_MUTED),
        );
        ui.add_space(2.0);
    }

    configure_plot(pane_id, x_label, y_label, link_group).show(ui, |_plot_ui| {});
}

/// Standard `egui_plot::Plot` configuration for every v2 pane. Centralises
/// the `link_axis` / `link_cursor` group, disables egui_plot's built-in
/// ctrl-scroll zoom (we own zoom via the trackpad pinch bridge), keeps
/// drag-to-pan + scroll-to-pan + double-click-reset on. Returns a
/// `Plot` ready for the caller to add `.x_axis_formatter` /
/// `.label_formatter` and then `.show(ui, ...)`.
pub fn configure_plot(
    pane_id: PaneId,
    x_label: &str,
    y_label: &str,
    link_group: egui::Id,
) -> egui_plot::Plot<'static> {
    use egui_plot::{Legend, Plot};

    Plot::new(pane_id.egui_id().with("plot"))
        .x_axis_label(x_label.to_owned())
        .y_axis_label(y_label.to_owned())
        .legend(Legend::default())
        .link_axis(link_group, [true, false])
        .link_cursor(link_group, [true, false])
        .allow_zoom(false)
        .allow_drag(true)
        .allow_scroll(true)
        .allow_double_click_reset(true)
}

/// Standard cursor VLine for every pane. The colour comes from the
/// theme so a future palette swap doesn't require touching pane
/// bodies.
pub fn cursor_vline(layer: u32) -> egui_plot::VLine {
    egui_plot::VLine::new("cursor", layer as f64)
        .color(theme::CURSOR_INK)
        .width(1.0_f32)
}

/// Consume the per-frame zoom + reset inputs at the top of a pane's
/// `Plot::show` closure. Centralises the pinch-zoom and reset-zoom
/// logic so each pane body stays focused on its series + thresholds.
///
/// Called as the first line inside `.show(ui, |plot_ui| { ... })`.
/// Order matters: the reset comes before the pinch, so a
/// reset-then-pinch in the same frame still leaves the pane
/// responsive to the gesture.
pub fn consume_plot_inputs(plot_ui: &mut egui_plot::PlotUi<'_>, ctx: &PaneCtx<'_>) {
    if ctx.reset_zoom {
        plot_ui.set_auto_bounds([true, true]);
    }
    if plot_ui.response().hovered() {
        super::zoom::apply_pinch_zoom(plot_ui, ctx.pinch_delta);
    }
}

/// Dispatch the standard `PaneState` matrix so each pane only
/// implements its `Loaded` body. Empty / loading / field-missing /
/// parse-error / no-CTB states render the shared empty-axis
/// placeholder with a state-appropriate muted note.
///
/// `draw_loaded` is called only when sim is `Some` AND the pane
/// state is `Loaded`. It is responsible for building its own
/// `Plot::show(...)` closure and rendering series, thresholds, and
/// the cursor VLine.
pub fn render_pane_states<F>(
    ui: &mut egui::Ui,
    ctx: &PaneCtx<'_>,
    pane_id: PaneId,
    x_label: &str,
    y_label: &str,
    draw_loaded: F,
) where
    F: FnOnce(&mut egui::Ui, &PrintSimulation, &PaneCtx<'_>),
{
    match &ctx.state {
        PaneState::Loaded => match ctx.sim {
            Some(sim) => draw_loaded(ui, sim, ctx),
            None => {
                empty_axis_placeholder(ui, pane_id, x_label, y_label, ctx.link_group, None);
            }
        },
        PaneState::NoRun => {
            empty_axis_placeholder(ui, pane_id, x_label, y_label, ctx.link_group, None);
        }
        PaneState::FieldMissing(field) => {
            empty_axis_placeholder(
                ui,
                pane_id,
                x_label,
                y_label,
                ctx.link_group,
                Some(&format!("(field missing in sim.json schema: {field})")),
            );
        }
    }
}

/// Render a section header for a pane. One per pane, semibold, no
/// display sizes per DESIGN.md §3 "No-Display-Type Rule".
pub fn pane_header(ui: &mut egui::Ui, title: &str) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(title)
                .strong()
                .color(theme::INK)
                .size(14.0),
        );
    });
    ui.add_space(2.0);
}
