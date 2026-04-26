mod data_dir;
mod heatmap;
mod mesh;
mod profile_repos;
mod sim;
mod slice;
mod ui;

use std::num::NonZero;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::window::FileDragAndDrop;
use bevy_egui::EguiPlugin;
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin, TrackpadBehavior};
use clap::Parser;
use resinsim_core::io::{ctb, stl};
use resinsim_core::repositories::load_simulation;
use resinsim_core::simulation::PrintSimulation;

use resinsim_core::values::InitialLedTemperature;

use crate::data_dir::resolve_data_dir;
use crate::heatmap::{cure_depth_domain, ramp};
use crate::mesh::{LoadedStlMesh, fit_panorbit_to_bbox, triangles_to_bevy_mesh};
use crate::profile_repos::ProfileRepos;
use crate::sim::{
    RunConfig, RunSimRequest, SimulationResult, apply_run_request, load_sim_from_path,
};
use crate::slice::{
    LoadedSliceStack, cumulative_z_mm, slice_stack_bounding_box, slice_stack_to_bevy_mesh,
};
use crate::ui::panels::{left_panel, right_panel};
use crate::ui::state::{PickerState, refresh_listings, refresh_loaded_profiles};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser, Debug, Resource)]
#[command(name = "resinsim-viz", about = "Resinsim physics-simulation visualizer")]
struct Args {
    /// Run one frame and exit (smoke-test mode)
    #[arg(long)]
    smoke_exit: bool,

    /// Load an STL file at startup. Drag-drop replaces the loaded mesh at runtime.
    #[arg(long, value_name = "PATH", conflicts_with = "load_ctb")]
    load_stl: Option<PathBuf>,

    /// Load a CTB sliced file at startup. Drag-drop replaces the loaded
    /// geometry at runtime. Mutually exclusive with --load-stl: only one
    /// geometry source is visible at a time in v1.
    #[arg(long, value_name = "PATH")]
    load_ctb: Option<PathBuf>,

    /// Resin/printer profile data directory. Resolves via the 4-stage
    /// chain (flag → `RESINSIM_DATA_DIR` env → `$CWD/data` →
    /// exe-sibling `data/`); see ADR-0011 / ADR-0004.
    #[arg(long, value_name = "PATH")]
    data_dir: Option<PathBuf>,

    /// Resin profile name (filename stem of a .toml under
    /// `<data-dir>/resins/`). Pre-selects in the left-panel picker
    /// at startup. Validation (does the listing contain it?) runs
    /// after data-dir resolution; an unknown name is logged and the
    /// picker stays empty so the user can pick manually.
    #[arg(long, value_name = "NAME")]
    resin: Option<String>,

    /// Printer profile name (filename stem of a .toml under
    /// `<data-dir>/printers/`). Pre-selects in the left-panel
    /// picker at startup. Same validation behaviour as `--resin`.
    #[arg(long, value_name = "NAME")]
    printer: Option<String>,

    /// Initial LED temperature in °C (e.g. `30.0` for a printer
    /// with a warm LED at print start). Validated via
    /// `InitialLedTemperature::new` at startup; an out-of-domain
    /// value logs a warn and the run uses the default cold-start.
    /// Threads through every Run during the session.
    #[arg(long, value_name = "CELSIUS")]
    initial_led_temp: Option<f32>,

    /// Save the simulation as JSON after each successful Run.
    /// `<PATH>` is treated as a sidecar file path; parent dir is
    /// created on demand. Errors log a warn and don't affect the
    /// GUI surface — persistence is best-effort.
    #[arg(long, value_name = "PATH")]
    save_sim: Option<PathBuf>,

    /// Load a PrintSimulation JSON at startup. Populates both the
    /// cure-depth heatmap overlay (issue 03) and the right-panel
    /// time-series plots (issue 04). Required for the heatmap when
    /// paired with --load-ctb (layer counts must match unless
    /// --allow-mismatch); for the plots, it works standalone (skip
    /// the picker → Run flow). Drag-drop of new sim files is not
    /// yet supported.
    #[arg(long, value_name = "PATH.json")]
    load_sim: Option<PathBuf>,

    /// DANGEROUS: skip the safety check that requires --load-sim to have
    /// the same layer count as --load-ctb. Without this flag, a mismatch
    /// is a hard error. Use only if you intentionally want to render a
    /// CTB with a sim that does not match it (e.g. during sim development).
    #[arg(long)]
    allow_mismatch: bool,
}

// ---------------------------------------------------------------------------
// Exit codes (used when --smoke-exit is set so CI can branch)
// ---------------------------------------------------------------------------

/// Sim file load / parse / validate failure.
const EXIT_SIM_LOAD_FAILED: u8 = 2;
/// Layer-count mismatch between --load-ctb and --load-sim.
const EXIT_LAYER_COUNT_MISMATCH: u8 = 3;
/// Bad pairing: --load-sim without --load-ctb, or --load-sim with --load-stl.
const EXIT_BAD_SIM_PAIRING: u8 = 4;

/// Drag-drop is interactive — never propagate `--smoke-exit` non-zero exit
/// codes from a drop. Smoke-exit is a Startup-time concern (CI invokes the
/// app with --smoke-exit + --load-{stl,ctb,sim} to validate a fixture
/// loads). Passing this constant (instead of `args.smoke_exit`) at the
/// drop call site documents the contract: a layer-count mismatch on
/// drag-drop hard-errors visually but does NOT crash the running session.
const DROP_IS_INTERACTIVE: bool = false;

/// Vertical lift (mm) applied to the layer-cursor Plane3d above its
/// nominal `z_prefix[index]` position so the cursor never coincides
/// with the underlying slice-mesh face — Bevy's depth test is f32 and
/// Z-fights at ~1 ULP at the bbox magnitudes we render (50–150 mm).
/// 50 µm = one Mars-class layer height, which is large enough to win
/// the depth test reliably AND small enough that the cursor is still
/// visually attached to the active layer's surface.
pub const LAYER_CURSOR_EPSILON_MM: f32 = 0.05;

fn fatal_exit(writer: &mut MessageWriter<AppExit>, code: u8) {
    writer.write(AppExit::Error(
        NonZero::new(code).expect("exit codes EXIT_* are non-zero by construction"),
    ));
}

// ---------------------------------------------------------------------------
// Bevy resources for sim/cursor/Z state.
//
// All three are inserted at App build with empty/None defaults, populated
// by the loaders, and reset on geometry unload (with one exception:
// LoadedSimulation survives a CTB reload so drag-drop can re-colour with
// the same sim if the new CTB's layer count matches).
//
// `Arc<PrintSimulation>` is intentionally NOT used: the cursor / HUD
// systems read a single field per Changed<CurrentLayer> tick and Bevy's
// Res<T> borrow is sufficient. If a future async path needs a clone,
// re-introduce Arc then.
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
pub struct LoadedSimulation(pub Option<PrintSimulation>);

#[derive(Resource, Default)]
pub struct CurrentLayer {
    pub index: u32,
    /// Last valid index (`== layers.len() - 1`). Cursor is unusable when
    /// `max == 0` and no slice-stack is loaded.
    pub max: u32,
}

#[derive(Resource, Default)]
pub struct LayerZPrefix(pub Vec<f32>);

/// Per-load colour-ramp domain `(min_um, max_um)` for cure_depth, used by
/// the HUD line. Stored alongside the sim so the cursor system doesn't
/// re-compute it every tick.
#[derive(Resource, Default)]
pub struct CureDepthDomain(pub Option<(f32, f32)>);

/// Marker component: the translucent layer-cursor entity that sits at
/// `z = z_prefix[current_layer.index]`. Distinct from `LoadedSliceStack`
/// because it has a separate mesh + material and gets despawned alongside
/// the slice on geometry reload.
#[derive(Component)]
pub struct LayerCursor;

// ---------------------------------------------------------------------------
// Drag-drop routing
// ---------------------------------------------------------------------------

/// Routing decision for a dropped file path. Pure-fn dispatch keeps
/// `handle_dropped_files` testable without exercising the MessageReader
/// plumbing.
///
/// Lower-cases the file extension before matching so mixed-case
/// extensions (`.CTB`, `.Stl`, `.STL`) route correctly. The core
/// `sliced::detect_format` helper is case-sensitive by design (it
/// matches the on-disk extension verbatim); this routing wrapper sits
/// in front of it for the viz drag-drop ergonomics on macOS, where
/// extensions often arrive in mixed case.
#[derive(Debug, PartialEq, Eq)]
pub enum DropAction {
    Stl,
    Ctb,
    Skip,
}

pub fn route_drop(path: &Path) -> DropAction {
    let lower: Option<String> = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase());
    match lower.as_deref() {
        Some("ctb") => DropAction::Ctb,
        Some("stl") => DropAction::Stl,
        _ => DropAction::Skip,
    }
}

// ---------------------------------------------------------------------------
// Scene setup
// ---------------------------------------------------------------------------

pub fn setup_scene(mut commands: Commands) {
    // Camera carries its own DirectionalLight as a child entity — a
    // "headlamp" rig. Both Camera3d and DirectionalLight point along
    // their entity's -Z axis, so a child light with Transform::default()
    // automatically tracks the camera direction as the user orbits.
    // Shadows are disabled: when the light direction is the view
    // direction, every front-facing facet shadows the next, producing
    // self-shadowing acne that masks the geometry we're trying to
    // inspect. Ambient (200 brightness from issue 01) provides fill for
    // back-facing surfaces.
    commands
        .spawn((
            Camera3d::default(),
            Transform::from_xyz(0.0, 5.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
            PanOrbitCamera {
                trackpad_behavior: TrackpadBehavior::BlenderLike {
                    modifier_pan: None,
                    modifier_zoom: None,
                },
                trackpad_pinch_to_zoom_enabled: true,
                ..default()
            },
            AmbientLight {
                brightness: 200.0,
                ..default()
            },
        ))
        .with_children(|cam| {
            cam.spawn((
                DirectionalLight {
                    illuminance: 10_000.0,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::default(),
            ));
        });
}

// ---------------------------------------------------------------------------
// STL loader (unchanged)
// ---------------------------------------------------------------------------

/// Despawn any existing `LoadedStlMesh` AND `LoadedSliceStack` (and
/// `LayerCursor`) entities, load the STL at `path`, spawn the converted
/// mesh, and frame the PanOrbitCamera against the loaded geometry's
/// bounding box.
///
/// Mutual exclusion: STL geometry doesn't carry a heatmap in v1, so the
/// LayerCursor is despawned along with any prior LoadedSliceStack to
/// keep the world consistent.
#[allow(clippy::too_many_arguments)]
fn load_stl_into_world(
    path: &Path,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    prior_stl: &Query<Entity, With<LoadedStlMesh>>,
    prior_slice: &Query<Entity, With<LoadedSliceStack>>,
    prior_cursor: &Query<Entity, With<LayerCursor>>,
    camera: &mut Query<&mut PanOrbitCamera, With<Camera3d>>,
) {
    despawn_geometry(commands, prior_stl, prior_slice, prior_cursor);

    let triangles = match stl::load_stl(path) {
        Ok(t) => t,
        Err(e) => {
            error!("STL load failed for {}: {e}", path.display());
            return;
        }
    };
    let bbox = stl::bounding_box(&triangles);

    let mesh_handle = meshes.add(triangles_to_bevy_mesh(&triangles));
    let material_handle = materials.add(StandardMaterial::from(Color::WHITE));
    commands.spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle),
        Transform::default(),
        LoadedStlMesh,
    ));

    for mut cam in camera.iter_mut() {
        fit_panorbit_to_bbox(&mut cam, &bbox);
    }
}

fn despawn_geometry(
    commands: &mut Commands,
    prior_stl: &Query<Entity, With<LoadedStlMesh>>,
    prior_slice: &Query<Entity, With<LoadedSliceStack>>,
    prior_cursor: &Query<Entity, With<LayerCursor>>,
) {
    for entity in prior_stl.iter() {
        commands.entity(entity).despawn();
    }
    for entity in prior_slice.iter() {
        commands.entity(entity).despawn();
    }
    for entity in prior_cursor.iter() {
        commands.entity(entity).despawn();
    }
}

// ---------------------------------------------------------------------------
// CTB + sim loader (heatmap-aware)
// ---------------------------------------------------------------------------

/// Load a CTB sliced file and (optionally) overlay a per-layer cure-depth
/// heatmap from the currently-loaded `LoadedSimulation`. Mutual exclusion
/// + fail-soft posture mirror `load_stl_into_world`.
///
/// **Heatmap policy.** When `LoadedSimulation` is `Some(sim)`:
/// - If `sim.layers().len() == layers.len()`: bake per-vertex
///   `Mesh::ATTRIBUTE_COLOR` from `heatmap::ramp(layer.cure_depth_um, domain)`
///   and spawn a `LayerCursor` entity at `z_prefix[max]`.
/// - Otherwise: emit `error!`, despawn any prior geometry, leave the
///   world empty, optionally exit with `EXIT_LAYER_COUNT_MISMATCH` when
///   `--smoke-exit` is set. The sim is preserved (a future drop with the
///   correct layer count will recover). `--allow-mismatch` overrides
///   this and falls back to soft-warn + uncoloured mesh.
#[allow(clippy::too_many_arguments)]
fn load_ctb_into_world(
    path: &Path,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    prior_stl: &Query<Entity, With<LoadedStlMesh>>,
    prior_slice: &Query<Entity, With<LoadedSliceStack>>,
    prior_cursor: &Query<Entity, With<LayerCursor>>,
    camera: &mut Query<&mut PanOrbitCamera, With<Camera3d>>,
    loaded_sim: &LoadedSimulation,
    current_layer: &mut CurrentLayer,
    z_prefix_res: &mut LayerZPrefix,
    domain_res: &mut CureDepthDomain,
    allow_mismatch: bool,
    smoke_exit: bool,
    exit_writer: &mut MessageWriter<AppExit>,
) {
    despawn_geometry(commands, prior_stl, prior_slice, prior_cursor);
    // Reset cursor/Z state on every reload — repopulated below if the
    // load succeeds.
    current_layer.index = 0;
    current_layer.max = 0;
    z_prefix_res.0.clear();
    domain_res.0 = None;

    let (_info, layers) = match ctb::parse_ctb(path) {
        Ok(parsed) => parsed,
        Err(e) => {
            error!("CTB load failed for {}: {e}", path.display());
            return;
        }
    };

    // Decide whether to bake a heatmap.
    let layer_colors: Option<Vec<[f32; 4]>> = match loaded_sim.0.as_ref() {
        None => None,
        Some(sim) => {
            if sim.layers().len() == layers.len() {
                let domain = cure_depth_domain(sim);
                domain_res.0 = Some(domain);
                let colors: Vec<[f32; 4]> = sim
                    .layers()
                    .iter()
                    .map(|lr| ramp(lr.cure_depth_um, domain))
                    .collect();
                Some(colors)
            } else if allow_mismatch {
                warn!(
                    "layer count mismatch: CTB has {} layers, sim has {} \
                     — --allow-mismatch is set, rendering uncoloured",
                    layers.len(),
                    sim.layers().len()
                );
                None
            } else {
                error!(
                    "layer count mismatch: CTB has {} layers, sim has {} \
                     — pass --allow-mismatch to render uncoloured",
                    layers.len(),
                    sim.layers().len()
                );
                if smoke_exit {
                    fatal_exit(exit_writer, EXIT_LAYER_COUNT_MISMATCH);
                }
                return; // world stays empty; sim resource preserved
            }
        }
    };

    let bbox = slice_stack_bounding_box(&layers);
    let z_prefix = cumulative_z_mm(&layers);

    let mesh_handle =
        meshes.add(slice_stack_to_bevy_mesh(&layers, layer_colors.as_deref()));
    let material_handle = materials.add(StandardMaterial::from(Color::WHITE));
    commands.spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle),
        Transform::default(),
        LoadedSliceStack {
            path: path.to_path_buf(),
        },
    ));

    // Cursor: only spawn when a heatmap is active (sim present + matching
    // counts, OR allow_mismatch). Mismatch-allowed has layer_colors=None
    // but the cursor is still useful for stepping through the geometry,
    // so we spawn whenever a sim is loaded — the HUD just won't have a
    // domain to report. Decision: spawn cursor iff sim is present AND
    // (counts match OR allow_mismatch) — i.e. iff this is a "heatmap or
    // explicitly-tolerated overlay" load.
    let cursor_active = loaded_sim.0.is_some()
        && (layer_colors.is_some() || allow_mismatch)
        && !layers.is_empty();
    if cursor_active {
        let max = (layers.len() as u32).saturating_sub(1);
        current_layer.index = max;
        current_layer.max = max;
        z_prefix_res.0 = z_prefix.clone();

        // Cursor entity: thin Plane3d (zero Z thickness) sized 1.1× the
        // bbox X/Y so it overhangs the model — gives the user an
        // unambiguous "ring" silhouette outside the print volume that's
        // visible even when the camera is dead-on-axis. Bright magenta
        // base + strong magenta emissive guarantees visibility against
        // any viridis colour (no point on the viridis ramp is magenta).
        // Double-sided + cull_mode=None so the cursor stays visible
        // when the user orbits below the print bed.
        let bbox_min_x = bbox.min[0];
        let bbox_min_y = bbox.min[1];
        let bbox_max_x = bbox.max[0];
        let bbox_max_y = bbox.max[1];
        let center_x = 0.5 * (bbox_min_x + bbox_max_x);
        let center_y = 0.5 * (bbox_min_y + bbox_max_y);
        let size_x = (bbox_max_x - bbox_min_x).max(1e-3) * 1.1;
        let size_y = (bbox_max_y - bbox_min_y).max(1e-3) * 1.1;

        let cursor_mesh = meshes.add(Plane3d::new(Vec3::Z, Vec2::new(size_x * 0.5, size_y * 0.5)));
        let cursor_material = materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.1, 0.9, 0.55),
            emissive: LinearRgba::new(2.0, 0.2, 1.8, 1.0),
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        let cursor_z =
            z_prefix_res.0[current_layer.index as usize] + LAYER_CURSOR_EPSILON_MM;
        commands.spawn((
            Mesh3d(cursor_mesh),
            MeshMaterial3d(cursor_material),
            Transform::from_xyz(center_x, center_y, cursor_z),
            LayerCursor,
        ));

        // Controls hint + first-layer HUD line. Fires on Startup AND on
        // drag-drop reload — users who first interact via drag-drop also
        // see the hint. The README is the canonical reference.
        info!("Controls: ↑/↓ arrows step layers");
        if let Some(sim) = loaded_sim.0.as_ref() {
            log_layer_line(sim, current_layer.index, current_layer.max, domain_res.0);
        }
    }

    for mut cam in camera.iter_mut() {
        fit_panorbit_to_bbox(&mut cam, &bbox);
    }
}

/// Format the per-layer HUD line. Right-aligned numeric fields keep
/// terminal output stable as the index/max digit count varies.
fn log_layer_line(sim: &PrintSimulation, index: u32, max: u32, domain: Option<(f32, f32)>) {
    let layer_count = max as usize + 1;
    let i = index as usize;
    let layer = match sim.layers().get(i) {
        Some(l) => l,
        None => return, // shouldn't happen if max was set correctly
    };
    let (lo, hi) = domain.unwrap_or((0.0, 1.0));
    info!(
        "Layer {:>4}/{} | cure_depth {:>6.1} µm | ramp {:.1}–{:.1} µm",
        i + 1,
        layer_count,
        layer.cure_depth_um,
        lo,
        hi
    );
}

// ---------------------------------------------------------------------------
// Startup orchestration
// ---------------------------------------------------------------------------

/// Startup system: load STL/CTB and (optionally) sim. Pre-validates the
/// flag pairing — `--load-sim` requires `--load-ctb`. Resources are
/// inserted unconditionally (with default values); this system just
/// populates them.
#[allow(clippy::too_many_arguments)]
fn setup_initial_load(
    args: Res<Args>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    prior_stl: Query<Entity, With<LoadedStlMesh>>,
    prior_slice: Query<Entity, With<LoadedSliceStack>>,
    prior_cursor: Query<Entity, With<LayerCursor>>,
    mut camera: Query<&mut PanOrbitCamera, With<Camera3d>>,
    mut loaded_sim: ResMut<LoadedSimulation>,
    mut current_layer: ResMut<CurrentLayer>,
    mut z_prefix: ResMut<LayerZPrefix>,
    mut domain: ResMut<CureDepthDomain>,
    mut exit_writer: MessageWriter<AppExit>,
) {
    // Pre-load: --load-sim if any. On Err leave LoadedSimulation as None.
    if let Some(sim_path) = args.load_sim.as_deref() {
        match load_simulation(sim_path) {
            Ok(sim) => {
                loaded_sim.0 = Some(sim);
            }
            Err(e) => {
                error!("simulation load failed for {}: {e}", sim_path.display());
                if args.smoke_exit {
                    fatal_exit(&mut exit_writer, EXIT_SIM_LOAD_FAILED);
                }
                // Continue to geometry load — without the sim the heatmap
                // is silently skipped (LoadedSimulation stays None).
            }
        }
    }

    // Bad pairing check: --load-sim with no --load-ctb (or with --load-stl).
    // Emit error so the user notices; continue to geometry load so an
    // interactive user can drag-drop a CTB and recover (the loaded sim
    // is preserved).
    if args.load_sim.is_some() && args.load_ctb.is_none() {
        error!(
            "--load-sim was supplied without --load-ctb; the heatmap \
             requires slice-stack geometry (STL pairing deferred). \
             Drag-drop a .ctb file with matching layer count to enable \
             the heatmap."
        );
        if args.smoke_exit {
            fatal_exit(&mut exit_writer, EXIT_BAD_SIM_PAIRING);
        }
    }

    match (args.load_stl.as_deref(), args.load_ctb.as_deref()) {
        (Some(path), None) => load_stl_into_world(
            path,
            &mut commands,
            &mut meshes,
            &mut materials,
            &prior_stl,
            &prior_slice,
            &prior_cursor,
            &mut camera,
        ),
        (None, Some(path)) => load_ctb_into_world(
            path,
            &mut commands,
            &mut meshes,
            &mut materials,
            &prior_stl,
            &prior_slice,
            &prior_cursor,
            &mut camera,
            &loaded_sim,
            &mut current_layer,
            &mut z_prefix,
            &mut domain,
            args.allow_mismatch,
            args.smoke_exit,
            &mut exit_writer,
        ),
        (None, None) => {}
        // clap's `conflicts_with` makes this unreachable, but the
        // exhaustive match keeps the dispatch total and grep-able.
        (Some(_), Some(_)) => unreachable!(
            "clap conflicts_with should reject --load-stl + --load-ctb at parse time"
        ),
    }
}

// ---------------------------------------------------------------------------
// Drag-drop
// ---------------------------------------------------------------------------

/// Update system: when one or more files are dropped on the window,
/// load the *last* `DroppedFile` of the tick. If multiple were dropped,
/// log an `info!` naming the chosen one — non-determinism is bounded
/// (last wins). Routes by extension via `route_drop`: `.stl` → STL
/// loader, `.ctb` → CTB loader, anything else → warn + skip.
///
/// Drag-drop preserves the `LoadedSimulation` resource: dropping a new
/// CTB whose layer count matches the loaded sim re-colours; mismatch
/// hard-errors (without exiting, since the user is interactive).
#[allow(clippy::too_many_arguments)]
fn handle_dropped_files(
    mut events: MessageReader<FileDragAndDrop>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    prior_stl: Query<Entity, With<LoadedStlMesh>>,
    prior_slice: Query<Entity, With<LoadedSliceStack>>,
    prior_cursor: Query<Entity, With<LayerCursor>>,
    mut camera: Query<&mut PanOrbitCamera, With<Camera3d>>,
    args: Res<Args>,
    loaded_sim: Res<LoadedSimulation>,
    mut current_layer: ResMut<CurrentLayer>,
    mut z_prefix: ResMut<LayerZPrefix>,
    mut domain: ResMut<CureDepthDomain>,
    mut exit_writer: MessageWriter<AppExit>,
) {
    let dropped: Vec<PathBuf> = events
        .read()
        .filter_map(|e| match e {
            FileDragAndDrop::DroppedFile { path_buf, .. } => Some(path_buf.clone()),
            _ => None,
        })
        .collect();
    let Some(path) = dropped.last() else {
        return;
    };
    if dropped.len() > 1 {
        info!(
            "{} files dropped this tick; rendering the last: {}",
            dropped.len(),
            path.display()
        );
    }
    match route_drop(path) {
        DropAction::Stl => load_stl_into_world(
            path,
            &mut commands,
            &mut meshes,
            &mut materials,
            &prior_stl,
            &prior_slice,
            &prior_cursor,
            &mut camera,
        ),
        DropAction::Ctb => load_ctb_into_world(
            path,
            &mut commands,
            &mut meshes,
            &mut materials,
            &prior_stl,
            &prior_slice,
            &prior_cursor,
            &mut camera,
            &loaded_sim,
            &mut current_layer,
            &mut z_prefix,
            &mut domain,
            args.allow_mismatch,
            DROP_IS_INTERACTIVE,
            &mut exit_writer,
        ),
        DropAction::Skip => {
            warn!(
                "unsupported drop {} — only .stl and .ctb are rendered",
                path.display()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Layer-cursor + keyboard + per-layer HUD systems
// ---------------------------------------------------------------------------

/// Keyboard handler. Up arrow advances to next layer (higher Z, later
/// in print time); Down arrow returns to previous layer (lower Z).
/// Matches PrusaSlicer convention. Saturating arithmetic at boundaries.
fn handle_layer_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut current: ResMut<CurrentLayer>,
) {
    if current.max == 0 && current.index == 0 {
        // No layers loaded — keys are no-ops. Avoids confusing log spam
        // in an empty-world session.
        return;
    }
    if keys.just_pressed(KeyCode::ArrowUp) {
        current.index = current.index.saturating_add(1).min(current.max);
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        current.index = current.index.saturating_sub(1);
    }
}

/// Cursor-positioning system. On `Changed<CurrentLayer>`, move the
/// `LayerCursor` entity's `Transform.translation.z` to
/// `z_prefix[current_layer.index]`. Bevy's Transform updates do NOT
/// re-upload the slice-stack mesh — the mesh's positions/normals/colour
/// buffer are baked once and never mutated post-spawn.
fn update_layer_cursor(
    current: Res<CurrentLayer>,
    z_prefix: Res<LayerZPrefix>,
    mut cursor_q: Query<&mut Transform, With<LayerCursor>>,
) {
    if !current.is_changed() {
        return;
    }
    let Some(z) = z_prefix.0.get(current.index as usize) else {
        return;
    };
    let lifted = *z + LAYER_CURSOR_EPSILON_MM;
    for mut transform in cursor_q.iter_mut() {
        transform.translation.z = lifted;
    }
}

/// HUD logger. On `Changed<CurrentLayer>`, emit one `info!` line with
/// the active layer's cure_depth + the ramp domain so the user can
/// interpret colours of OTHER layers without an in-window legend.
fn log_layer_change(
    current: Res<CurrentLayer>,
    sim: Res<LoadedSimulation>,
    domain: Res<CureDepthDomain>,
) {
    if !current.is_changed() {
        return;
    }
    let Some(s) = sim.0.as_ref() else {
        return;
    };
    log_layer_line(s, current.index, current.max, domain.0);
}

// ---------------------------------------------------------------------------
// Smoke-exit
// ---------------------------------------------------------------------------

fn smoke_exit_after_one_frame(mut writer: MessageWriter<AppExit>) {
    writer.write(AppExit::Success);
}

/// Startup: resolve the profile data dir, populate `ProfileRepos`,
/// run an initial `refresh_listings`, build `RunConfig` from the
/// CLI override flags, and apply `--load-sim` if passed. On
/// data-dir miss, the error chain string goes into
/// `SimulationResult.last_error` and the app keeps running with
/// empty pickers. If `--resin` / `--printer` were passed and match
/// a known listing, the corresponding `selected_*` fields are
/// pre-set so the picker boots ready-to-run.
fn setup_profile_repos(
    args: Res<Args>,
    mut commands: Commands,
    mut state: ResMut<PickerState>,
    mut sim: ResMut<SimulationResult>,
) {
    let resolved = resolve_data_dir(args.data_dir.as_deref());
    match resolved {
        Ok(dir) => {
            let repos = ProfileRepos::new(&dir);
            if let Err(e) = refresh_listings(&mut state, &repos) {
                error!("failed to list profiles: {e}");
                sim.last_error = Some(format!("profile listing failed: {e}"));
            }
            // Apply --resin / --printer preselects post-listing so the
            // ComboBox doesn't dangle on a typo'd name. Unknown names
            // log a warn + leave None.
            if let Some(name) = args.resin.as_deref() {
                if state.available_resins.iter().any(|r| r == name) {
                    state.selected_resin = Some(name.to_string());
                } else {
                    warn!(
                        "--resin {name:?} not found in {} — pick from {:?}",
                        dir.join("resins").display(),
                        state.available_resins
                    );
                }
            }
            if let Some(name) = args.printer.as_deref() {
                if state.available_printers.iter().any(|p| p == name) {
                    state.selected_printer = Some(name.to_string());
                } else {
                    warn!(
                        "--printer {name:?} not found in {} — pick from {:?}",
                        dir.join("printers").display(),
                        state.available_printers
                    );
                }
            }
            commands.insert_resource(repos);
        }
        Err(msg) => {
            error!("could not resolve profile data directory:\n{msg}");
            sim.last_error = Some(msg);
        }
    }

    // Build RunConfig from the CLI override flags, validating the
    // initial LED temperature at startup so an out-of-domain input
    // is caught once (not every Run). Out-of-domain values log a
    // warn and degrade to None.
    let initial_led_temp = match args.initial_led_temp {
        Some(c) => match InitialLedTemperature::new(c) {
            Ok(t) => Some(t),
            Err(e) => {
                warn!("--initial-led-temp {c} rejected: {e}");
                None
            }
        },
        None => None,
    };
    commands.insert_resource(RunConfig {
        initial_led_temp,
        save_sim_path: args.save_sim.clone(),
    });

    // Apply --load-sim immediately so the right panel populates
    // without needing to click Run. The picker stays available
    // for follow-up reruns; clicking Run overwrites the loaded
    // sim with a fresh one.
    if let Some(path) = args.load_sim.as_ref() {
        match load_sim_from_path(path) {
            Ok(loaded) => {
                let summary = loaded.summary();
                info!(
                    "loaded simulation from {}: {} layers / {} failures",
                    path.display(),
                    summary.total_layers,
                    summary.critical_failures
                );
                sim.simulation = Some(loaded);
                sim.last_error = None;
            }
            Err(e) => {
                error!("--load-sim failed: {e}");
                sim.last_error = Some(e);
            }
        }
    }
}

/// Update: keep `state.loaded_*` in sync with `state.selected_*`
/// when the user changes a ComboBox selection. Idempotent body —
/// equal names short-circuit, no `is_changed` ping-pong.
fn refresh_loaded_profiles_system(
    mut state: ResMut<PickerState>,
    repos: Option<Res<ProfileRepos>>,
) {
    let Some(repos) = repos else {
        return;
    };
    refresh_loaded_profiles(&mut state, &repos);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let smoke_exit = args.smoke_exit;
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "resinsim-viz".into(),
            ..default()
        }),
        ..default()
    }))
    .add_plugins(PanOrbitCameraPlugin)
    .add_plugins(EguiPlugin::default())
    .insert_resource(args)
    .init_resource::<LoadedSimulation>()
    .init_resource::<CurrentLayer>()
    .init_resource::<LayerZPrefix>()
    .init_resource::<CureDepthDomain>()
    .init_resource::<PickerState>()
    .init_resource::<SimulationResult>()
    .add_message::<RunSimRequest>()
    .add_systems(
        Startup,
        (setup_scene, setup_initial_load, setup_profile_repos).chain(),
    )
    .add_systems(
        Update,
        (
            handle_dropped_files,
            handle_layer_keys,
            update_layer_cursor,
            log_layer_change,
            refresh_loaded_profiles_system,
            apply_run_request,
        ),
    )
    .add_systems(bevy_egui::EguiPrimaryContextPass, (left_panel, right_panel));
    if smoke_exit {
        app.add_systems(Update, smoke_exit_after_one_frame);
    }
    app.run();
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn run_startup() -> App {
        let mut app = App::new();
        app.add_systems(Startup, setup_scene);
        app.update();
        app
    }

    #[test]
    fn setup_scene_spawns_camera_with_panorbit() {
        let mut app = run_startup();
        let world = app.world_mut();
        let mut cam_q = world.query::<&Camera3d>();
        assert_eq!(cam_q.iter(world).count(), 1, "expected exactly one Camera3d");
        let mut orbit_q = world.query::<&PanOrbitCamera>();
        assert_eq!(
            orbit_q.iter(world).count(),
            1,
            "Camera3d must carry PanOrbitCamera"
        );
    }

    #[test]
    fn setup_scene_spawns_directional_light() {
        let mut app = run_startup();
        let world = app.world_mut();
        let mut light_q = world.query::<&DirectionalLight>();
        assert!(
            light_q.iter(world).count() >= 1,
            "expected at least one DirectionalLight"
        );
    }

    #[test]
    fn setup_scene_attaches_ambient_light_to_camera() {
        let mut app = run_startup();
        let world = app.world_mut();
        let mut q = world.query::<(&Camera3d, &AmbientLight)>();
        assert!(
            q.iter(world).next().is_some(),
            "Camera3d must carry AmbientLight (Bevy 0.18: component on camera, not resource)"
        );
    }

    #[test]
    fn directional_light_is_child_of_camera_for_headlamp() {
        // Headlamp rig: DirectionalLight must be parented to the
        // Camera3d entity so its -Z (light direction) inherits the
        // camera's -Z (view direction). With Transform::default() on
        // the child, light direction == view direction every frame.
        let mut app = run_startup();
        let world = app.world_mut();
        let mut camera_q = world.query_filtered::<Entity, With<Camera3d>>();
        let camera_entity = camera_q
            .iter(world)
            .next()
            .expect("Camera3d entity must exist");

        let mut light_q = world.query::<(&DirectionalLight, &ChildOf)>();
        let mut found_headlamp = false;
        for (_light, child_of) in light_q.iter(world) {
            if child_of.parent() == camera_entity {
                found_headlamp = true;
            }
        }
        assert!(
            found_headlamp,
            "DirectionalLight must be a child of the Camera3d entity (headlamp rig)"
        );
    }

    #[test]
    fn headlamp_directional_light_disables_shadows() {
        // Shadows on a view-aligned light produce self-shadowing acne
        // because every front-facing facet shadows the next. Keep
        // shadows off for the headlamp; rely on ambient + view-aligned
        // diffuse for the inspection view.
        let mut app = run_startup();
        let world = app.world_mut();
        let mut q = world.query::<&DirectionalLight>();
        for light in q.iter(world) {
            assert!(
                !light.shadows_enabled,
                "headlamp DirectionalLight must have shadows_enabled = false"
            );
        }
    }

    #[test]
    fn panorbit_uses_blender_trackpad_behavior() {
        let mut app = run_startup();
        let world = app.world_mut();
        let mut q = world.query::<&PanOrbitCamera>();
        let cam = q
            .iter(world)
            .next()
            .expect("PanOrbitCamera must be present");
        assert!(
            matches!(cam.trackpad_behavior, TrackpadBehavior::BlenderLike { .. }),
            "Mac trackpad config must be BlenderLike, was {:?}",
            cam.trackpad_behavior
        );
        assert!(
            cam.trackpad_pinch_to_zoom_enabled,
            "pinch-to-zoom must be enabled for Mac trackpad"
        );
    }

    fn cube_fixture_path() -> PathBuf {
        PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/test_cube.stl"
        ))
    }

    /// Assemble an App with just enough plumbing for the loader tests:
    /// `AssetPlugin` (required by `init_asset`), the `Mesh` and
    /// `StandardMaterial` asset stores, the heatmap resources, and a
    /// camera entity carrying a default `PanOrbitCamera`. No window
    /// backend, no rendering, no input plugin (added separately by the
    /// keyboard tests).
    fn make_loader_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::asset::AssetPlugin::default())
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_resource::<LoadedSimulation>()
            .init_resource::<CurrentLayer>()
            .init_resource::<LayerZPrefix>()
            .init_resource::<CureDepthDomain>()
            .add_message::<AppExit>();
        app.world_mut()
            .spawn((Camera3d::default(), PanOrbitCamera::default()));
        app
    }

    fn count_loaded(app: &mut App) -> usize {
        let world = app.world_mut();
        let mut q = world.query::<&LoadedStlMesh>();
        q.iter(world).count()
    }

    fn count_loaded_slice(app: &mut App) -> usize {
        let world = app.world_mut();
        let mut q = world.query::<&LoadedSliceStack>();
        q.iter(world).count()
    }

    fn count_layer_cursor(app: &mut App) -> usize {
        let world = app.world_mut();
        let mut q = world.query::<&LayerCursor>();
        q.iter(world).count()
    }

    #[test]
    fn args_resource_reads_load_stl() {
        let mut app = App::new();
        let args = Args {
            smoke_exit: false,
            load_stl: Some(PathBuf::from("foo.stl")),
            load_ctb: None,
            data_dir: None,
            resin: None,
            printer: None,
            initial_led_temp: None,
            save_sim: None,
            load_sim: None,
            allow_mismatch: false,
        };
        app.insert_resource(args);
        let stored = app
            .world()
            .get_resource::<Args>()
            .expect("Args was just inserted as a resource");
        assert_eq!(stored.load_stl.as_deref(), Some(Path::new("foo.stl")));
        assert!(stored.load_ctb.is_none());
        assert!(stored.load_sim.is_none());
        assert!(!stored.allow_mismatch);
        assert!(!stored.smoke_exit);
    }

    #[test]
    fn args_resource_reads_smoke_exit_without_load_stl() {
        // Symmetric case: --smoke-exit alone (no --load-stl) is the
        // existing CI smoke flag; the loader path is the *new* surface.
        // Locks in that the absent-arg field is None, not Some(default).
        let mut app = App::new();
        app.insert_resource(Args {
            smoke_exit: true,
            load_stl: None,
            load_ctb: None,
            data_dir: None,
            resin: None,
            printer: None,
            initial_led_temp: None,
            save_sim: None,
            load_sim: None,
            allow_mismatch: false,
        });
        let stored = app
            .world()
            .get_resource::<Args>()
            .expect("Args was just inserted as a resource");
        assert!(stored.smoke_exit);
        assert!(stored.load_stl.is_none());
        assert!(stored.load_ctb.is_none());
    }

    #[test]
    fn args_resource_reads_load_ctb() {
        // Mirror of `args_resource_reads_load_stl` for the new --load-ctb
        // surface. clap's `conflicts_with` enforces mutual exclusion at
        // parse time; this test confirms the resource round-trip when
        // only --load-ctb is set.
        let mut app = App::new();
        app.insert_resource(Args {
            smoke_exit: false,
            load_stl: None,
            load_ctb: Some(PathBuf::from("foo.ctb")),
            data_dir: None,
            resin: None,
            printer: None,
            initial_led_temp: None,
            save_sim: None,
            load_sim: None,
            allow_mismatch: false,
        });
        let stored = app
            .world()
            .get_resource::<Args>()
            .expect("Args was just inserted as a resource");
        assert!(stored.load_stl.is_none());
        assert_eq!(stored.load_ctb.as_deref(), Some(Path::new("foo.ctb")));
        assert!(!stored.smoke_exit);
    }

    #[test]
    fn args_resource_reads_load_sim_and_allow_mismatch() {
        // New flags from issue 03. Round-trip through the resource layer.
        let mut app = App::new();
        app.insert_resource(Args {
            smoke_exit: false,
            load_stl: None,
            load_ctb: Some(PathBuf::from("cube.ctb")),
            data_dir: None,
            resin: None,
            printer: None,
            initial_led_temp: None,
            save_sim: None,
            load_sim: Some(PathBuf::from("cube.sim.json")),
            allow_mismatch: true,
        });
        let stored = app
            .world()
            .get_resource::<Args>()
            .expect("Args was just inserted as a resource");
        assert_eq!(stored.load_sim.as_deref(), Some(Path::new("cube.sim.json")));
        assert!(stored.allow_mismatch);
    }

    #[test]
    fn load_stl_into_world_spawns_loaded_marker_for_cube() {
        let mut app = make_loader_app();
        let path = cube_fixture_path();
        let load_id = app.world_mut().register_system(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  prior_stl: Query<Entity, With<LoadedStlMesh>>,
                  prior_slice: Query<Entity, With<LoadedSliceStack>>,
                  prior_cursor: Query<Entity, With<LayerCursor>>,
                  mut camera: Query<&mut PanOrbitCamera, With<Camera3d>>| {
                load_stl_into_world(
                    &path,
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &prior_stl,
                    &prior_slice,
                    &prior_cursor,
                    &mut camera,
                );
            },
        );
        app.world_mut()
            .run_system(load_id)
            .expect("registered system runs");
        app.update(); // flush deferred Commands

        assert_eq!(count_loaded(&mut app), 1, "exactly one LoadedStlMesh after load");

        // Camera radius was updated from default None to Some(1.5 * diagonal).
        let world = app.world_mut();
        let mut q = world.query::<&PanOrbitCamera>();
        let cam = q.iter(world).next().expect("camera entity present");
        assert!(
            cam.radius.is_some(),
            "fit_panorbit_to_bbox should have set radius"
        );
    }

    #[test]
    fn load_stl_into_world_despawns_prior_marker() {
        let mut app = make_loader_app();
        let path = cube_fixture_path();
        let load_id = app.world_mut().register_system(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  prior_stl: Query<Entity, With<LoadedStlMesh>>,
                  prior_slice: Query<Entity, With<LoadedSliceStack>>,
                  prior_cursor: Query<Entity, With<LayerCursor>>,
                  mut camera: Query<&mut PanOrbitCamera, With<Camera3d>>| {
                load_stl_into_world(
                    &path,
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &prior_stl,
                    &prior_slice,
                    &prior_cursor,
                    &mut camera,
                );
            },
        );

        app.world_mut().run_system(load_id).expect("first run ok");
        app.update();
        assert_eq!(count_loaded(&mut app), 1, "one entity after first load");

        app.world_mut().run_system(load_id).expect("second run ok");
        app.update();
        assert_eq!(
            count_loaded(&mut app),
            1,
            "still one entity after second load — prior was despawned"
        );
    }

    #[test]
    fn load_stl_into_world_with_invalid_path_does_not_spawn() {
        let mut app = make_loader_app();
        let bad_path = PathBuf::from("/definitely/does/not/exist/nope.stl");
        let load_id = app.world_mut().register_system(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  prior_stl: Query<Entity, With<LoadedStlMesh>>,
                  prior_slice: Query<Entity, With<LoadedSliceStack>>,
                  prior_cursor: Query<Entity, With<LayerCursor>>,
                  mut camera: Query<&mut PanOrbitCamera, With<Camera3d>>| {
                load_stl_into_world(
                    &bad_path,
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &prior_stl,
                    &prior_slice,
                    &prior_cursor,
                    &mut camera,
                );
            },
        );
        app.world_mut()
            .run_system(load_id)
            .expect("system runs even when load fails");
        app.update();
        assert_eq!(count_loaded(&mut app), 0, "invalid path leaves world empty");
    }

    #[test]
    fn smoke_exit_with_load_stl_flag_runs_setup_initial_load() {
        // Regression guard: --load-stl + --smoke-exit must run the
        // Startup loader path without panic. No window backend needed.
        let mut app = make_loader_app();
        app.insert_resource(Args {
            smoke_exit: true,
            load_stl: Some(cube_fixture_path()),
            load_ctb: None,
            data_dir: None,
            resin: None,
            printer: None,
            initial_led_temp: None,
            save_sim: None,
            load_sim: None,
            allow_mismatch: false,
        });
        app.add_systems(Startup, setup_initial_load);
        app.add_systems(Update, smoke_exit_after_one_frame);
        app.update();
        assert_eq!(count_loaded(&mut app), 1, "loader ran during Startup");
    }

    #[test]
    fn load_ctb_into_world_with_invalid_path_does_not_spawn() {
        // No CTB writer exists in-tree; we exercise the error path with a
        // path that doesn't resolve. Asserts no LoadedSliceStack and no
        // panic, mirroring the STL test of the same shape.
        let mut app = make_loader_app();
        let bad_path = PathBuf::from("/definitely/does/not/exist/nope.ctb");
        let load_id = app.world_mut().register_system(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  prior_stl: Query<Entity, With<LoadedStlMesh>>,
                  prior_slice: Query<Entity, With<LoadedSliceStack>>,
                  prior_cursor: Query<Entity, With<LayerCursor>>,
                  mut camera: Query<&mut PanOrbitCamera, With<Camera3d>>,
                  loaded_sim: Res<LoadedSimulation>,
                  mut current: ResMut<CurrentLayer>,
                  mut z_prefix: ResMut<LayerZPrefix>,
                  mut domain: ResMut<CureDepthDomain>,
                  mut exit_writer: MessageWriter<AppExit>| {
                load_ctb_into_world(
                    &bad_path,
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &prior_stl,
                    &prior_slice,
                    &prior_cursor,
                    &mut camera,
                    &loaded_sim,
                    &mut current,
                    &mut z_prefix,
                    &mut domain,
                    false,
                    false,
                    &mut exit_writer,
                );
            },
        );
        app.world_mut()
            .run_system(load_id)
            .expect("system runs even when load fails");
        app.update();
        assert_eq!(
            count_loaded_slice(&mut app),
            0,
            "invalid CTB path leaves world empty"
        );
    }

    #[test]
    fn load_ctb_into_world_despawns_prior_loaded_stl() {
        // Mutual exclusion: loading a CTB despawns any LoadedStlMesh, even
        // when the CTB load itself fails. The despawn-before-parse order
        // makes the world observably reflect the user's last intent.
        let mut app = make_loader_app();
        // Synthetic LoadedStlMesh: marker alone is enough — the despawn
        // path doesn't read mesh content, only the marker query.
        app.world_mut().spawn(LoadedStlMesh);
        let bad_path = PathBuf::from("/definitely/does/not/exist/nope.ctb");
        let load_id = app.world_mut().register_system(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  prior_stl: Query<Entity, With<LoadedStlMesh>>,
                  prior_slice: Query<Entity, With<LoadedSliceStack>>,
                  prior_cursor: Query<Entity, With<LayerCursor>>,
                  mut camera: Query<&mut PanOrbitCamera, With<Camera3d>>,
                  loaded_sim: Res<LoadedSimulation>,
                  mut current: ResMut<CurrentLayer>,
                  mut z_prefix: ResMut<LayerZPrefix>,
                  mut domain: ResMut<CureDepthDomain>,
                  mut exit_writer: MessageWriter<AppExit>| {
                load_ctb_into_world(
                    &bad_path,
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &prior_stl,
                    &prior_slice,
                    &prior_cursor,
                    &mut camera,
                    &loaded_sim,
                    &mut current,
                    &mut z_prefix,
                    &mut domain,
                    false,
                    false,
                    &mut exit_writer,
                );
            },
        );
        assert_eq!(count_loaded(&mut app), 1, "synthetic LoadedStlMesh present");
        app.world_mut()
            .run_system(load_id)
            .expect("system runs even when load fails");
        app.update();
        assert_eq!(
            count_loaded(&mut app),
            0,
            "prior LoadedStlMesh despawned by the failed CTB load"
        );
    }

    #[test]
    fn load_ctb_into_world_despawns_prior_slice_on_reload() {
        // Same-kind axis: a pre-existing LoadedSliceStack must be
        // despawned before the new (failing) CTB load, so a successful
        // reload would never leave two stacks visible.
        let mut app = make_loader_app();
        app.world_mut().spawn(LoadedSliceStack {
            path: PathBuf::from("/synthetic"),
        });
        let bad_path = PathBuf::from("/definitely/does/not/exist/nope.ctb");
        let load_id = app.world_mut().register_system(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  prior_stl: Query<Entity, With<LoadedStlMesh>>,
                  prior_slice: Query<Entity, With<LoadedSliceStack>>,
                  prior_cursor: Query<Entity, With<LayerCursor>>,
                  mut camera: Query<&mut PanOrbitCamera, With<Camera3d>>,
                  loaded_sim: Res<LoadedSimulation>,
                  mut current: ResMut<CurrentLayer>,
                  mut z_prefix: ResMut<LayerZPrefix>,
                  mut domain: ResMut<CureDepthDomain>,
                  mut exit_writer: MessageWriter<AppExit>| {
                load_ctb_into_world(
                    &bad_path,
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &prior_stl,
                    &prior_slice,
                    &prior_cursor,
                    &mut camera,
                    &loaded_sim,
                    &mut current,
                    &mut z_prefix,
                    &mut domain,
                    false,
                    false,
                    &mut exit_writer,
                );
            },
        );
        assert_eq!(count_loaded_slice(&mut app), 1, "synthetic LoadedSliceStack present");
        app.world_mut()
            .run_system(load_id)
            .expect("system runs even when load fails");
        app.update();
        assert_eq!(
            count_loaded_slice(&mut app),
            0,
            "prior LoadedSliceStack despawned by the failed CTB load"
        );
    }

    #[test]
    fn load_ctb_failed_load_also_despawns_prior_layer_cursor() {
        // Adversarial-review round 2 finding: the cursor must follow
        // the slice's lifecycle. A failed CTB drop while a prior
        // cursor is in the world must despawn the cursor too,
        // otherwise it floats with no slice underneath.
        let mut app = make_loader_app();
        app.world_mut().spawn(LayerCursor);
        let bad_path = PathBuf::from("/definitely/does/not/exist/nope.ctb");
        let load_id = app.world_mut().register_system(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  prior_stl: Query<Entity, With<LoadedStlMesh>>,
                  prior_slice: Query<Entity, With<LoadedSliceStack>>,
                  prior_cursor: Query<Entity, With<LayerCursor>>,
                  mut camera: Query<&mut PanOrbitCamera, With<Camera3d>>,
                  loaded_sim: Res<LoadedSimulation>,
                  mut current: ResMut<CurrentLayer>,
                  mut z_prefix: ResMut<LayerZPrefix>,
                  mut domain: ResMut<CureDepthDomain>,
                  mut exit_writer: MessageWriter<AppExit>| {
                load_ctb_into_world(
                    &bad_path,
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &prior_stl,
                    &prior_slice,
                    &prior_cursor,
                    &mut camera,
                    &loaded_sim,
                    &mut current,
                    &mut z_prefix,
                    &mut domain,
                    false,
                    false,
                    &mut exit_writer,
                );
            },
        );
        assert_eq!(count_layer_cursor(&mut app), 1, "synthetic LayerCursor present");
        app.world_mut()
            .run_system(load_id)
            .expect("system runs even when load fails");
        app.update();
        assert_eq!(
            count_layer_cursor(&mut app),
            0,
            "prior LayerCursor despawned by the failed CTB load"
        );
    }

    #[test]
    fn route_drop_dispatches_by_extension_with_case_folding() {
        // Pure-fn test on `route_drop` — locks in the case-folding
        // contract. macOS drag-drop often emits mixed-case extensions;
        // the core `sliced::detect_format` is case-sensitive by design,
        // so `route_drop` lower-cases the extension before matching.
        let cases: &[(&str, DropAction)] = &[
            ("foo.ctb", DropAction::Ctb),
            ("foo.CTB", DropAction::Ctb),
            ("foo.Ctb", DropAction::Ctb),
            ("foo.stl", DropAction::Stl),
            ("foo.STL", DropAction::Stl),
            ("foo.Stl", DropAction::Stl),
            ("foo.unknown", DropAction::Skip),
            ("nodot", DropAction::Skip),
            ("/abs/path/cube.ctb", DropAction::Ctb),
            ("/abs/path/CUBE.STL", DropAction::Stl),
        ];
        for (input, expected) in cases {
            let actual = route_drop(Path::new(input));
            assert_eq!(
                actual, *expected,
                "route_drop({input:?}) = {actual:?}, expected {expected:?}"
            );
        }
    }

    #[test]
    fn smoke_exit_with_load_ctb_flag_runs_setup_without_panic() {
        // Env-var-gated smoke test: if RESINSIM_SLICED_FIXTURE points to
        // a real .ctb file, the Startup loader path must run without
        // panic. Without the env var the test no-ops — same convention
        // as `data/test_cube_10mm.ctb.README.md`.
        let Ok(fixture) = std::env::var("RESINSIM_SLICED_FIXTURE") else {
            return;
        };
        let mut app = make_loader_app();
        app.insert_resource(Args {
            smoke_exit: true,
            load_stl: None,
            load_ctb: Some(PathBuf::from(fixture)),
            data_dir: None,
            resin: None,
            printer: None,
            initial_led_temp: None,
            save_sim: None,
            load_sim: None,
            allow_mismatch: false,
        });
        app.add_systems(Startup, setup_initial_load);
        app.add_systems(Update, smoke_exit_after_one_frame);
        app.update();
        assert_eq!(
            count_loaded_slice(&mut app),
            1,
            "loader ran during Startup and spawned a LoadedSliceStack"
        );
    }

    // ---- Keyboard handler tests ----

    fn make_input_app() -> App {
        // Bare ButtonInput<KeyCode> resource without InputPlugin: the
        // plugin's PreUpdate clear-just-pressed step would wipe the
        // synthetic press() before our handler runs. Direct resource
        // gives us full control over the just_pressed lifecycle in tests.
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<CurrentLayer>()
            .init_resource::<LayerZPrefix>();
        app
    }

    #[test]
    fn arrow_up_advances_current_layer_with_saturation() {
        let mut app = make_input_app();
        // Mid-range start: index = 1, max = 2 → ArrowUp → 2 → ArrowUp → 2 (clamp).
        app.world_mut().resource_mut::<CurrentLayer>().max = 2;
        app.world_mut().resource_mut::<CurrentLayer>().index = 1;
        app.add_systems(Update, handle_layer_keys);

        // First ArrowUp: 1 → 2.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowUp);
        app.update();
        assert_eq!(app.world().resource::<CurrentLayer>().index, 2);

        // Second ArrowUp: clamp at max (2).
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowUp);
        app.update();
        assert_eq!(app.world().resource::<CurrentLayer>().index, 2);
    }

    #[test]
    fn arrow_down_retreats_current_layer_with_saturation() {
        let mut app = make_input_app();
        app.world_mut().resource_mut::<CurrentLayer>().max = 2;
        app.world_mut().resource_mut::<CurrentLayer>().index = 1;
        app.add_systems(Update, handle_layer_keys);

        // First ArrowDown: 1 → 0.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowDown);
        app.update();
        assert_eq!(app.world().resource::<CurrentLayer>().index, 0);

        // Second ArrowDown: clamp at 0.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowDown);
        app.update();
        assert_eq!(app.world().resource::<CurrentLayer>().index, 0);
    }

    #[test]
    fn arrow_keys_no_op_when_no_layers_loaded() {
        // max == 0 AND index == 0 — empty world. Keys must not change
        // anything (avoids confusing log spam in an empty session).
        let mut app = make_input_app();
        app.add_systems(Update, handle_layer_keys);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowUp);
        app.update();
        assert_eq!(app.world().resource::<CurrentLayer>().index, 0);
        assert_eq!(app.world().resource::<CurrentLayer>().max, 0);
    }

    // ---- Cursor + mesh-immutability assertions ----

    #[test]
    fn smoke_exit_with_load_sim_pairing_runs_heatmap_path() {
        // Env-var-gated end-to-end smoke: requires RESINSIM_SLICED_FIXTURE
        // (real .ctb file) and either RESINSIM_SIM_FIXTURE (real .sim.json)
        // or — when SIM_FIXTURE is unset — a synthetic sim is built in
        // the test from the parsed CTB's layer count, so a single env
        // var unlocks the heatmap smoke.
        //
        // Asserts the wired-together heatmap path:
        //   - LoadedSimulation populated
        //   - LoadedSliceStack spawned
        //   - LayerCursor spawned
        //   - LayerZPrefix has length == layers + 1
        //   - CureDepthDomain populated
        let Ok(ctb_fixture) = std::env::var("RESINSIM_SLICED_FIXTURE") else {
            return;
        };

        // Decide the sim path: explicit SIM_FIXTURE, or synthesised.
        let sim_path: PathBuf = if let Ok(p) = std::env::var("RESINSIM_SIM_FIXTURE") {
            PathBuf::from(p)
        } else {
            // Parse CTB to learn its layer count, build matching synthetic
            // sim, write to temp file. The sim is functionally junk for
            // physics purposes but valid for the heatmap smoke (validate()
            // accepts the synthetic LayerResult shape).
            let (_info, layers) = resinsim_core::io::ctb::parse_ctb(Path::new(&ctb_fixture))
                .expect("RESINSIM_SLICED_FIXTURE must point to a parseable .ctb");
            use resinsim_core::entities::{LayerResult, PrinterProfile, ResinProfile};
            use resinsim_core::simulation::PrintSimulation;
            let recipe = ResinProfile::generic_standard().recipe().clone();
            let printer = PrinterProfile::generic_msla_4k();
            let mut sim = PrintSimulation::new(recipe, printer);
            for i in 0..layers.len() {
                let lr = LayerResult {
                    index: i as u32,
                    cure_depth_um: 100.0 + i as f32,
                    peel_force_n: 0.0,
                    suction_force_n: 0.0,
                    total_force_n: 0.0,
                    support_capacity_n: 0.0,
                    safety_factor: 1.0,
                    cross_section_area_mm2: 1.0,
                    area_delta_mm2: 0.0,
                    vat_temperature_c: 22.0,
                    viscosity_mpa_s: 200.0,
                    z_deflection_um: 0.0,
                    effective_layer_height_um: 50.0,
                    worst_cure_depth_um: 100.0 + i as f32,
                };
                sim.add_layer(lr, vec![]).expect("sequential index");
            }
            let dir = std::env::temp_dir().join(format!(
                "resinsim-viz-heatmap-smoke-{}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).expect("create tmpdir");
            let path = dir.join("synthetic.sim.json");
            let json = serde_json::to_string_pretty(&sim).expect("serialize sim");
            std::fs::write(&path, json).expect("write sim json");
            path
        };

        let mut app = make_loader_app();
        app.insert_resource(Args {
            smoke_exit: true,
            load_stl: None,
            load_ctb: Some(PathBuf::from(ctb_fixture)),
            data_dir: None,
            resin: None,
            printer: None,
            initial_led_temp: None,
            save_sim: None,
            load_sim: Some(sim_path),
            allow_mismatch: false,
        });
        app.add_systems(Startup, setup_initial_load);
        app.add_systems(Update, smoke_exit_after_one_frame);
        app.update();

        assert_eq!(
            count_loaded_slice(&mut app),
            1,
            "Startup must spawn one LoadedSliceStack"
        );
        assert_eq!(
            count_layer_cursor(&mut app),
            1,
            "Startup must spawn one LayerCursor when sim+CTB load OK"
        );
        assert!(
            app.world().resource::<LoadedSimulation>().0.is_some(),
            "LoadedSimulation must be populated"
        );
        let sim_layers = app
            .world()
            .resource::<LoadedSimulation>()
            .0
            .as_ref()
            .map(|s| s.layers().len())
            .expect("sim populated");
        assert_eq!(
            app.world().resource::<LayerZPrefix>().0.len(),
            sim_layers + 1,
            "LayerZPrefix must have length sim.layers + 1"
        );
        assert!(
            app.world().resource::<CureDepthDomain>().0.is_some(),
            "CureDepthDomain must be populated when heatmap is active"
        );
    }

    #[test]
    fn slice_stack_mesh_attribute_color_unmutated_under_arrow_keys() {
        // Bake-once contract: ATTRIBUTE_COLOR is baked into the slice-stack
        // mesh at build time and MUST NOT be mutated by any system. Layer
        // changes update only the LayerCursor's Transform.translation.z;
        // the underlying Mesh asset is read-only after spawn. This is the
        // load-bearing claim behind the issue's "Update on layer change
        // without re-uploading the mesh" requirement.
        //
        // Approach: build a real slice-stack mesh with per-layer colours,
        // add to Assets<Mesh>, capture the colour buffer + the asset
        // count, run handle_layer_keys + update_layer_cursor across many
        // arrow-key ticks, then re-read the buffer + asset count and
        // assert byte-equality. A future regression that calls
        // `meshes.get_mut(slice_handle)` (or any code path that touches
        // ATTRIBUTE_COLOR after spawn) breaks this test.
        use bevy::mesh::VertexAttributeValues;
        use resinsim_core::io::sliced::LayerInput;
        use resinsim_core::values::LayerMask;

        fn solid_layer(layer_height_um: f32, w: u32, h: u32, voxel: f32) -> LayerInput {
            let mask = LayerMask::new_all_solid(w, h, voxel)
                .expect("LayerMask::new_all_solid accepts positive dims + voxel");
            LayerInput::new(
                0,
                (w * h) as f64 * (voxel as f64).powi(2),
                1.0,
                60.0,
                layer_height_um,
                0.0,
            )
            .expect("LayerInput::new accepts non-negative area + positive exposure")
            .with_mask(mask)
        }

        fn read_colors(meshes: &Assets<Mesh>, handle: &Handle<Mesh>) -> Vec<[f32; 4]> {
            let mesh = meshes.get(handle).expect("slice-stack mesh present");
            match mesh
                .attribute(Mesh::ATTRIBUTE_COLOR)
                .expect("ATTRIBUTE_COLOR must be baked on the slice-stack mesh")
            {
                VertexAttributeValues::Float32x4(v) => v.clone(),
                other => panic!("expected Float32x4 colors, got {other:?}"),
            }
        }

        let layers = vec![
            solid_layer(50.0, 2, 2, 0.05),
            solid_layer(50.0, 2, 2, 0.05),
            solid_layer(50.0, 2, 2, 0.05),
        ];
        let colors = vec![
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
        ];
        let mesh = slice_stack_to_bevy_mesh(&layers, Some(&colors));
        let z_prefix = cumulative_z_mm(&layers);

        let mut app = make_loader_app();
        // ButtonInput<KeyCode> as a bare resource (no InputPlugin) — same
        // convention as the keyboard tests above. InputPlugin would clear
        // just_pressed in PreUpdate before our handler runs.
        app.init_resource::<ButtonInput<KeyCode>>();

        // Insert the baked mesh into Assets<Mesh> and spawn the slice-stack
        // entity carrying its handle. Capture the asset count BEFORE the
        // arrow-key cycle so we can assert it stays constant.
        let slice_handle = app.world_mut().resource_mut::<Assets<Mesh>>().add(mesh);
        app.world_mut().spawn((
            Mesh3d(slice_handle.clone()),
            Transform::default(),
            LoadedSliceStack {
                path: PathBuf::from("/synthetic"),
            },
        ));
        // Cursor entity at z=0 — this is the entity whose Transform IS
        // expected to mutate. The slice-stack mesh asset is what must NOT.
        app.world_mut()
            .spawn((Transform::from_xyz(0.0, 0.0, 0.0), LayerCursor));
        let max = (layers.len() as u32) - 1;
        app.world_mut().resource_mut::<CurrentLayer>().max = max;
        app.world_mut().resource_mut::<CurrentLayer>().index = 0;
        app.world_mut().resource_mut::<LayerZPrefix>().0 = z_prefix.clone();

        let colors_before = read_colors(app.world().resource::<Assets<Mesh>>(), &slice_handle);
        let mesh_count_before = app.world().resource::<Assets<Mesh>>().iter().count();

        app.add_systems(Update, (handle_layer_keys, update_layer_cursor));

        // Drive a full traversal: ArrowUp until clamp, then ArrowDown back
        // to 0. Each tick: reset_all() drops pressed + just_pressed +
        // just_released so the press() that follows fires just_pressed
        // exactly once. Without InputPlugin's PreUpdate clear step, stale
        // just_pressed entries would accumulate across ticks (and a stale
        // ArrowUp in the down loop would re-clamp to max each iteration,
        // making ArrowDown a no-op).
        for _ in 0..(max + 2) {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.reset_all();
            keys.press(KeyCode::ArrowUp);
            app.update();
        }
        assert_eq!(
            app.world().resource::<CurrentLayer>().index,
            max,
            "ArrowUp should saturate at max"
        );
        for _ in 0..(max + 2) {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.reset_all();
            keys.press(KeyCode::ArrowDown);
            app.update();
        }
        assert_eq!(
            app.world().resource::<CurrentLayer>().index,
            0,
            "ArrowDown should saturate at 0"
        );

        // Bake-once assertions: colour buffer byte-identical, no new
        // meshes added (would imply someone called meshes.add() in a
        // handler, which is also a forbidden allocation pattern post-load).
        let colors_after = read_colors(app.world().resource::<Assets<Mesh>>(), &slice_handle);
        assert_eq!(
            colors_after, colors_before,
            "ATTRIBUTE_COLOR Vec must be byte-identical after arrow-key traversal"
        );
        let mesh_count_after = app.world().resource::<Assets<Mesh>>().iter().count();
        assert_eq!(
            mesh_count_after, mesh_count_before,
            "no new Mesh assets should be added by cursor / keyboard systems"
        );

        // Cursor side-effect IS expected: its Transform.translation.z
        // should have visited z_prefix[max] then returned to z_prefix[0].
        let cursor_z = app
            .world_mut()
            .query::<(&Transform, &LayerCursor)>()
            .iter(app.world())
            .next()
            .map(|(t, _)| t.translation.z)
            .expect("cursor entity present");
        let expected = z_prefix[0] + LAYER_CURSOR_EPSILON_MM;
        assert!(
            (cursor_z - expected).abs() < 1e-6,
            "cursor returned to z_prefix[0] + epsilon = {expected}, got {cursor_z}"
        );
    }

    #[test]
    fn update_layer_cursor_moves_transform_z_only() {
        // Cursor system contract: on Changed<CurrentLayer>, only the
        // cursor's Transform.translation.z is updated. No mesh asset
        // mutation. Verified by inserting an asset count and asserting
        // it does not change.
        let mut app = make_loader_app();
        app.world_mut().resource_mut::<LayerZPrefix>().0 = vec![0.0, 0.1, 0.2];
        app.world_mut().resource_mut::<CurrentLayer>().max = 2;
        app.world_mut().resource_mut::<CurrentLayer>().index = 0;
        // Spawn a cursor entity at z=0.
        app.world_mut()
            .spawn((Transform::from_xyz(0.0, 0.0, 0.0), LayerCursor));
        app.add_systems(Update, update_layer_cursor);

        // Tick 1: Changed fires (index just inserted). Cursor lands at
        // z_prefix[0] + epsilon = 0.0 + LAYER_CURSOR_EPSILON_MM.
        app.update();
        let z_after_first = app
            .world_mut()
            .query::<(&Transform, &LayerCursor)>()
            .iter(app.world())
            .next()
            .map(|(t, _)| t.translation.z)
            .expect("cursor present");
        assert!((z_after_first - LAYER_CURSOR_EPSILON_MM).abs() < 1e-6);

        // Bump index to 2 → cursor jumps to z_prefix[2] + epsilon = 0.2 + eps.
        app.world_mut().resource_mut::<CurrentLayer>().index = 2;
        app.update();
        let z_after_jump = app
            .world_mut()
            .query::<(&Transform, &LayerCursor)>()
            .iter(app.world())
            .next()
            .map(|(t, _)| t.translation.z)
            .expect("cursor present");
        let expected = 0.2 + LAYER_CURSOR_EPSILON_MM;
        assert!(
            (z_after_jump - expected).abs() < 1e-6,
            "cursor z should be {expected}, got {z_after_jump}"
        );
    }

    /// `--resin <NAME>` + `--printer <NAME>` preselect: when both
    /// flags name an existing profile, `setup_profile_repos` must
    /// populate `PickerState.selected_*` so the picker boots
    /// ready-to-run.
    #[test]
    fn cli_args_resin_and_printer_preselect_picker() {
        let mut app = make_loader_app();
        app.insert_resource(Args {
            smoke_exit: true,
            load_stl: None,
            load_ctb: None,
            data_dir: Some(PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../data"
            ))),
            resin: Some("generic_standard".to_string()),
            printer: Some("generic_msla_4k".to_string()),
            initial_led_temp: None,
            save_sim: None,
            load_sim: None,
            allow_mismatch: false,
        });
        app.init_resource::<PickerState>()
            .init_resource::<SimulationResult>()
            .add_systems(Startup, setup_profile_repos);
        app.update();

        let state = app
            .world()
            .get_resource::<PickerState>()
            .expect("test fixture: PickerState was init_resource'd");
        assert_eq!(
            state.selected_resin.as_deref(),
            Some("generic_standard"),
            "--resin must preselect the named resin"
        );
        assert_eq!(
            state.selected_printer.as_deref(),
            Some("generic_msla_4k"),
            "--printer must preselect the named printer"
        );
    }

    /// Unknown `--resin` name: `setup_profile_repos` must keep
    /// `selected_resin = None` (logs a warn) so the picker stays
    /// open for manual selection rather than dangling on a typo.
    #[test]
    fn cli_args_unknown_resin_does_not_preselect() {
        let mut app = make_loader_app();
        app.insert_resource(Args {
            smoke_exit: true,
            load_stl: None,
            load_ctb: None,
            data_dir: Some(PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../data"
            ))),
            resin: Some("definitely_not_a_resin".to_string()),
            printer: None,
            initial_led_temp: None,
            save_sim: None,
            load_sim: None,
            allow_mismatch: false,
        });
        app.init_resource::<PickerState>()
            .init_resource::<SimulationResult>()
            .add_systems(Startup, setup_profile_repos);
        app.update();

        let state = app
            .world()
            .get_resource::<PickerState>()
            .expect("test fixture: PickerState was init_resource'd");
        assert!(
            state.selected_resin.is_none(),
            "unknown --resin name must not preselect; user picks manually"
        );
        // But the listing was still populated.
        assert!(state.available_resins.contains(&"generic_standard".to_string()));
    }

    /// Step-11 regression guard: with the new resources
    /// (`PickerState`, `SimulationResult`, `ProfileRepos`) and the
    /// `setup_profile_repos` Startup system + the
    /// `apply_run_request` / `refresh_loaded_profiles_system`
    /// Update systems, the App must still construct + run one
    /// update cycle without panic. EguiPlugin is *not* loaded here
    /// — it requires a render backend; this test pins the
    /// non-egui half of the wiring.
    #[test]
    fn new_resources_and_systems_do_not_panic_on_one_update() {
        let mut app = make_loader_app();
        app.insert_resource(Args {
            smoke_exit: true,
            load_stl: None,
            load_ctb: None,
            data_dir: Some(PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../data"
            ))),
            resin: None,
            printer: None,
            initial_led_temp: None,
            save_sim: None,
            load_sim: None,
            allow_mismatch: false,
        });
        app.init_resource::<PickerState>()
            .init_resource::<SimulationResult>()
            .add_message::<RunSimRequest>()
            .add_systems(Startup, setup_profile_repos)
            .add_systems(
                Update,
                (refresh_loaded_profiles_system, apply_run_request),
            )
            .add_systems(Update, smoke_exit_after_one_frame);
        app.update();
        // Profile listings should be populated — the data dir was
        // resolved (via the explicit --data-dir arg) and
        // refresh_listings ran during setup_profile_repos.
        let state = app
            .world()
            .get_resource::<PickerState>()
            .expect("test fixture: PickerState was init_resource'd");
        assert!(
            state.available_resins.contains(&"generic_standard".to_string()),
            "setup_profile_repos must populate listings; got {:?}",
            state.available_resins
        );
    }
}
