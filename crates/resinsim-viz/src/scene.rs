//! Scene helpers — printer envelope, build plate, and load-time wiring.
//!
//! Lives in `resinsim-viz` (presentation layer per ADR-0010): no `bevy::*`
//! types may flow into `resinsim-core`. Pulls `PrinterProfile` and
//! `BuildEnvelope` from core.
//!
//! # Module scope
//!
//! - [`ActivePrinterProfile`] resource — currently selected printer (CLI flag).
//! - [`PrinterEnvelope`] resource — derived view of build dimensions in mm.
//! - [`BuildPlate`] marker + spawn fn — plate sits at top of envelope per ADR-0011.
//! - Anchor Transform for `LoadedSliceStack` — 180° rotation about X +
//!   translate-to-envelope.max_z so the model hangs upside-down with native
//!   layer 0 glued to plate's underside.
//!
//! # LOC budget
//!
//! Stay under ~250 *non-test* LOC. With tests included this can run to
//! ~400 LOC and stay readable. If non-test code grows past 250 LOC during
//! follow-ups, file a follow-up to split into
//! `scene/{plate,envelope,anchor}.rs`.

use bevy::prelude::*;
use resinsim_core::entities::{BuildEnvelope, PrinterProfile};

/// Build-plate thickness. Plate's bottom face sits at `envelope.max_z_mm`;
/// the cuboid centre therefore lives at `envelope.max_z_mm + THICKNESS/2`.
pub const BUILD_PLATE_THICKNESS_MM: f32 = 3.0;

/// Plate material colour — DragonFruit dark theme. Theme switching is
/// out of scope for v1 (per the focus-on-sim-and-data direction).
pub const BUILD_PLATE_COLOR: Color = Color::srgb(
    0x32 as f32 / 255.0,
    0x38 as f32 / 255.0,
    0x41 as f32 / 255.0,
);

/// ECS marker for the entity holding the build-plate mesh.
#[derive(Component)]
pub struct BuildPlate;

/// Currently selected printer profile (resource).
///
/// Always inserted at `Startup` (Bevy resources are inserted-or-not, not
/// present-but-none). The inner `Option` is `None` when no `--printer`
/// flag is supplied; consumers fall back to CTB header bed_size_mm +
/// sentinel max_z + a one-shot warn (see [`PrinterEnvelope`] priority chain).
#[derive(Debug, Clone, Resource, Default)]
pub struct ActivePrinterProfile(pub Option<PrinterProfile>);

/// Derived build-envelope dimensions in mm (resource).
///
/// Priority chain:
///   1. [`ActivePrinterProfile`] `.0.build_envelope_mm` if `Some`.
///   2. CTB header `bed_size_mm` for X/Y + sentinel `max_z = 200` if no
///      profile envelope is present.
///   3. Cold-start default (192/120/200 — typical 8.9" 4K monoLCD class)
///      if neither is available; emits a one-shot warn.
#[derive(Debug, Clone, Copy, Resource, PartialEq)]
pub struct PrinterEnvelope {
    pub width_mm: f32,
    pub depth_mm: f32,
    pub max_z_mm: f32,
}

impl Default for PrinterEnvelope {
    fn default() -> Self {
        // Cold-start sentinel (priority chain entry 3).
        Self {
            width_mm: 192.0,
            depth_mm: 120.0,
            max_z_mm: 200.0,
        }
    }
}

impl PrinterEnvelope {
    /// Build from a [`PrinterProfile`] when its `build_envelope_mm` is
    /// `Some`. Returns `None` when the profile lacks the field, signalling
    /// the caller to fall through to the CTB-header / default arms of the
    /// priority chain.
    pub fn from_profile(profile: &PrinterProfile) -> Option<Self> {
        profile.build_envelope_mm().map(Self::from_envelope)
    }

    /// Build directly from a [`BuildEnvelope`] (for tests + the
    /// from_profile path).
    pub fn from_envelope(env: BuildEnvelope) -> Self {
        Self {
            width_mm: env.width_mm,
            depth_mm: env.depth_mm,
            max_z_mm: env.max_z_mm,
        }
    }

    /// Build from a CTB header's `bed_size_mm` (X, Y) plus a sentinel Z.
    /// Used when no profile envelope is available; max_z falls back to the
    /// default.
    pub fn from_ctb_header(bed_size_mm: (f32, f32)) -> Self {
        Self {
            width_mm: bed_size_mm.0,
            depth_mm: bed_size_mm.1,
            max_z_mm: Self::default().max_z_mm,
        }
    }
}

/// Despawn any prior [`BuildPlate`] entity, then spawn a Cuboid at
/// the top of the build envelope (real MSLA orientation per ADR-0011).
///
/// Plate's BOTTOM face sits at world Z = `envelope.max_z_mm` (this is
/// the face the model is glued to via the layer-zero anchor Transform).
/// Plate centre Z = `envelope.max_z_mm + BUILD_PLATE_THICKNESS_MM / 2`.
/// XY footprint matches the envelope.
///
/// The plate is centred at the envelope's XY centre `(width/2, depth/2)`,
/// i.e. world origin lives at the envelope's near-corner. Matches the
/// CTB / slice-mesh native coordinate system (issue 09 emits at
/// `0..w*voxel × 0..h*voxel`).
pub fn spawn_build_plate(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    prior_plate: &Query<Entity, With<BuildPlate>>,
    envelope: &PrinterEnvelope,
) {
    for entity in prior_plate.iter() {
        commands.entity(entity).despawn();
    }
    let cuboid = bevy::math::primitives::Cuboid::new(
        envelope.width_mm,
        envelope.depth_mm,
        BUILD_PLATE_THICKNESS_MM,
    );
    let mesh_handle = meshes.add(Mesh::from(cuboid));
    let material_handle = materials.add(StandardMaterial::from(BUILD_PLATE_COLOR));
    let centre_x = envelope.width_mm * 0.5;
    let centre_y = envelope.depth_mm * 0.5;
    let centre_z = envelope.max_z_mm + BUILD_PLATE_THICKNESS_MM * 0.5;
    commands.spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle),
        Transform::from_translation(Vec3::new(centre_x, centre_y, centre_z)),
        BuildPlate,
    ));
}

/// Mismatch threshold (mm) on either bed axis above which the
/// profile-vs-CTB-header XY disagreement triggers a warn. Tight enough to
/// catch wrong-printer-for-this-CTB cases; loose enough to ignore
/// rounding noise from header f32 storage.
pub const ENVELOPE_MISMATCH_TOLERANCE_MM: f32 = 0.5;

/// Reconcile the runtime [`PrinterEnvelope`] resource with a freshly
/// parsed CTB header.
///
/// Priority chain (ADR-0011, ADR-0012):
///   1. `active.0.as_ref().and_then(|p| p.build_envelope_mm())` → profile
///      wins; envelope is replaced with the profile dimensions. If the
///      CTB header `bed_size_mm` disagrees with the profile XY by more
///      than `ENVELOPE_MISMATCH_TOLERANCE_MM` AND `warned_about_mismatch`
///      is `false`, emit a one-shot `warn!` and flip the flag.
///   2. Profile lacks envelope OR no profile → envelope is replaced with
///      `PrinterEnvelope::from_ctb_header(bed_size_mm)` (CTB X/Y +
///      sentinel max_z).
///
/// Caller passes a `Local<bool>` (in Bevy systems) or a fresh `&mut false`
/// (in tests) for the warn-once flag.
pub fn resolve_envelope_after_ctb_load(
    active: &ActivePrinterProfile,
    ctb_bed_size_mm: (f32, f32),
    envelope: &mut PrinterEnvelope,
    warned_about_mismatch: &mut bool,
) {
    match active.0.as_ref().and_then(|p| p.build_envelope_mm()) {
        Some(profile_env) => {
            *envelope = PrinterEnvelope::from_envelope(profile_env);
            let dx = (profile_env.width_mm - ctb_bed_size_mm.0).abs();
            let dy = (profile_env.depth_mm - ctb_bed_size_mm.1).abs();
            if !*warned_about_mismatch
                && (dx > ENVELOPE_MISMATCH_TOLERANCE_MM
                    || dy > ENVELOPE_MISMATCH_TOLERANCE_MM)
            {
                bevy::log::warn!(
                    "active printer profile envelope ({:.2} x {:.2} mm) disagrees with CTB header bed_size_mm ({:.2} x {:.2} mm) by > {} mm; profile wins. Verify --printer matches the CTB.",
                    profile_env.width_mm,
                    profile_env.depth_mm,
                    ctb_bed_size_mm.0,
                    ctb_bed_size_mm.1,
                    ENVELOPE_MISMATCH_TOLERANCE_MM,
                );
                *warned_about_mismatch = true;
            }
        }
        None => {
            *envelope = PrinterEnvelope::from_ctb_header(ctb_bed_size_mm);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_envelope_is_192_120_200() {
        let env = PrinterEnvelope::default();
        assert!((env.width_mm - 192.0).abs() < 1e-4);
        assert!((env.depth_mm - 120.0).abs() < 1e-4);
        assert!((env.max_z_mm - 200.0).abs() < 1e-4);
    }

    #[test]
    fn envelope_from_profile_reads_build_envelope_when_some() {
        let profile = PrinterProfile::elegoo_mars5_ultra();
        let env = PrinterEnvelope::from_profile(&profile)
            .expect("Mars 5 Ultra factory populates build_envelope");
        assert!((env.width_mm - 153.36).abs() < 1e-4);
        assert!((env.depth_mm - 77.76).abs() < 1e-4);
        assert!((env.max_z_mm - 165.0).abs() < 1e-4);
    }

    // Note: the "profile without build_envelope returns None" path is
    // covered in resinsim-core's printer_profile tests
    // (legacy_toml_without_build_envelope_defaults_to_none). Re-testing
    // it here would require toml as a dev-dep on viz; the core-side test
    // is authoritative for the Option<BuildEnvelope> deserialisation
    // contract.

    #[test]
    fn envelope_from_ctb_header_uses_default_max_z() {
        let env = PrinterEnvelope::from_ctb_header((153.36, 77.76));
        assert!((env.width_mm - 153.36).abs() < 1e-4);
        assert!((env.depth_mm - 77.76).abs() < 1e-4);
        // max_z falls back to the default 200 mm — CTB header carries no Z.
        assert!((env.max_z_mm - 200.0).abs() < 1e-4);
    }

    #[test]
    fn active_printer_profile_default_is_none() {
        let active = ActivePrinterProfile::default();
        assert!(active.0.is_none());
    }

    // --- BuildPlate spawn tests (ADR-0011: plate ON TOP of envelope) ---

    fn make_plate_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::asset::AssetPlugin::default())
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>();
        app
    }

    fn run_spawn_plate(app: &mut App, envelope: PrinterEnvelope) {
        let id = app.world_mut().register_system(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  prior_plate: Query<Entity, With<BuildPlate>>| {
                spawn_build_plate(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &prior_plate,
                    &envelope,
                );
            },
        );
        app.world_mut().run_system(id).expect("spawn system runs");
        app.update();
    }

    fn count_plates(app: &mut App) -> usize {
        let world = app.world_mut();
        let mut q = world.query::<&BuildPlate>();
        q.iter(world).count()
    }

    #[test]
    fn build_plate_spawns_exactly_one() {
        let mut app = make_plate_app();
        run_spawn_plate(&mut app, PrinterEnvelope::default());
        assert_eq!(count_plates(&mut app), 1);
    }

    #[test]
    fn build_plate_respawn_despawns_prior() {
        let mut app = make_plate_app();
        run_spawn_plate(&mut app, PrinterEnvelope::default());
        run_spawn_plate(&mut app, PrinterEnvelope::default());
        assert_eq!(
            count_plates(&mut app),
            1,
            "prior plate must be despawned before respawn",
        );
    }

    #[test]
    fn build_plate_bottom_face_at_envelope_max_z() {
        // ADR-0011 anchor invariant: plate's BOTTOM face sits at
        // world Z = envelope.max_z_mm. Centre Z = max_z + thickness/2.
        let mut app = make_plate_app();
        let envelope = PrinterEnvelope {
            width_mm: 153.36,
            depth_mm: 77.76,
            max_z_mm: 165.0,
        };
        run_spawn_plate(&mut app, envelope);
        let world = app.world_mut();
        let mut q = world.query_filtered::<&Transform, With<BuildPlate>>();
        let t = q.iter(world).next().expect("plate Transform present");
        let expected_centre_z = envelope.max_z_mm + BUILD_PLATE_THICKNESS_MM * 0.5;
        assert!(
            (t.translation.z - expected_centre_z).abs() < 1e-3,
            "plate centre z must equal envelope.max_z + thickness/2 = {expected_centre_z}, got {}",
            t.translation.z,
        );
        let bottom_face_z = t.translation.z - BUILD_PLATE_THICKNESS_MM * 0.5;
        assert!(
            (bottom_face_z - envelope.max_z_mm).abs() < 1e-3,
            "plate bottom face must equal envelope.max_z = {}, got {bottom_face_z}",
            envelope.max_z_mm,
        );
    }

    #[test]
    fn build_plate_aabb_matches_envelope_xy() {
        // Plate is centred at (envelope.width/2, envelope.depth/2); its
        // XY footprint equals the envelope (the model lives directly
        // under the plate in world coords; near-corner = origin).
        let mut app = make_plate_app();
        let envelope = PrinterEnvelope {
            width_mm: 192.0,
            depth_mm: 120.0,
            max_z_mm: 200.0,
        };
        run_spawn_plate(&mut app, envelope);
        let world = app.world_mut();
        let mut q = world.query_filtered::<&Transform, With<BuildPlate>>();
        let t = q.iter(world).next().expect("plate Transform present");
        assert!((t.translation.x - 96.0).abs() < 1e-3);
        assert!((t.translation.y - 60.0).abs() < 1e-3);
    }

    // --- resolve_envelope_after_ctb_load (priority chain) tests ---

    #[test]
    fn resolve_envelope_with_profile_uses_profile_dims() {
        let active = ActivePrinterProfile(Some(PrinterProfile::elegoo_mars5_ultra()));
        let mut env = PrinterEnvelope::default();
        let mut warned = false;
        // CTB header agreeing with the profile (within tolerance) — no warn.
        resolve_envelope_after_ctb_load(&active, (153.36, 77.76), &mut env, &mut warned);
        assert!((env.width_mm - 153.36).abs() < 1e-4);
        assert!((env.depth_mm - 77.76).abs() < 1e-4);
        assert!((env.max_z_mm - 165.0).abs() < 1e-4);
        assert!(!warned, "no mismatch → no warn");
    }

    #[test]
    fn resolve_envelope_warns_once_on_profile_xy_mismatch() {
        let active = ActivePrinterProfile(Some(PrinterProfile::elegoo_mars5_ultra()));
        let mut env = PrinterEnvelope::default();
        let mut warned = false;
        // CTB header way off (200x120 vs 153.36x77.76).
        resolve_envelope_after_ctb_load(&active, (200.0, 120.0), &mut env, &mut warned);
        assert!(warned, "first call must flip the warn flag");
        // Second call must not re-warn (idempotent).
        let mut warned_was_true = warned;
        resolve_envelope_after_ctb_load(&active, (200.0, 120.0), &mut env, &mut warned_was_true);
        assert!(warned_was_true, "subsequent calls leave the flag set; warn not re-emitted");
    }

    #[test]
    fn resolve_envelope_no_profile_uses_ctb_header() {
        let active = ActivePrinterProfile(None);
        let mut env = PrinterEnvelope::default();
        let mut warned = false;
        resolve_envelope_after_ctb_load(&active, (153.36, 77.76), &mut env, &mut warned);
        assert!((env.width_mm - 153.36).abs() < 1e-4);
        assert!((env.depth_mm - 77.76).abs() < 1e-4);
        // max_z falls through to the cold-start sentinel (default).
        assert!((env.max_z_mm - PrinterEnvelope::default().max_z_mm).abs() < 1e-4);
        assert!(!warned);
    }
}
