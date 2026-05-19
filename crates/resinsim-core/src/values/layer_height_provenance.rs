//! `LayerHeightProvenance` — reconciliation between the CTB file-axis
//! (per-layer `LayerHeightSeq`) and the resin recipe-axis
//! (`recipe.layer_height_um`).
//!
//! ## DDD shape
//!
//! Value object. Carries the per-layer CTB authority + the resin
//! recipe value, plus a structured `MismatchDetail` describing the
//! disagreement (None on agreement). Constructed via [`reconcile`].
//!
//! Behaviour that belongs with this data:
//!
//! - [`format_warning`] — the user-facing stderr text. Lives on the
//!   value object because the wording is derived purely from the
//!   reconciliation data + the resin profile name; the simulation
//!   runner just routes the call. Two text branches per
//!   [`MismatchKind`]; both branches contain Mag's literal "GUESS"
//!   and "WRONG LAYER COUNT" framing from the issue.
//!
//! - [`render_text_summary`] — the report_health single-line
//!   description ("CTB layer_height: 40.000 µm (recipe: 30.000 µm) ⚠").
//!
//! ## Serde shape (sim.json)
//!
//! Custom Serialize / Deserialize for schema efficiency. The
//! per-layer Vec is only emitted in the variable / adaptive-slicing
//! case; uniform CTBs serialise as a flat `ctb_um: f32` field. This
//! avoids embedding a ~70 KB Vec on every uniform sim.json (the
//! common case). Both shapes deserialise cleanly via a fall-through
//! reader.
//!
//! ```text
//! // Uniform (common case):
//! { "ctb_um": 40.0, "recipe_um": 30.0, "mismatch": { ... } }
//!
//! // Variable / adaptive slicing:
//! { "ctb_layer_heights_um": [50.0, 30.0, ...], "recipe_um": 40.0,
//!   "mismatch": { "kind": "variable", ... } }
//! ```
//!
//! See ADR-0005 Consequences "Policy: CTB as file-axis authority" +
//! ADR-0017 §2 Coordinates for the canonical decision.

use serde::de::Deserializer;
use serde::ser::{SerializeStruct, Serializer};
use serde::{Deserialize, Serialize};

use crate::values::layer_height_seq::LayerHeightSeq;

/// Approx equality tolerance for layer-height comparisons. 1 nm — well
/// below f32 quantisation noise on µm-scale values. Matches
/// [`LayerHeightSeq::UNIFORMITY_TOL_UM`].
const TOL_UM: f32 = 1e-3;

/// Reconciliation between the CTB's embedded per-layer layer-heights
/// (the file-axis, authoritative for runtime) and the resin recipe's
/// authored `recipe.layer_height_um` (the recipe-axis, authoring
/// metadata).
///
/// Per ADR-0005 the CTB is the operating point ("the file you have")
/// and the recipe is the user's calibration intent. CTBs sliced with
/// adaptive (variable layer height) tools produce per-layer values
/// that legitimately differ — the runtime supports that by
/// dispatching each layer's value from
/// [`Self::ctb_layer_heights`]. When the slice is uniform AND matches
/// the recipe within `TOL_UM`, `mismatch` is `None`. Otherwise
/// `mismatch` is `Some` with a structured kind describing whether the
/// disagreement is recipe-vs-uniform-CTB or variable-Z (recipe cannot
/// describe a varying stack).
///
/// `has_mismatch()` is equivalent to "the user should be warned".
#[derive(Debug, Clone)]
pub struct LayerHeightProvenance {
    ctb: LayerHeightSeq,
    recipe_um: f32,
    mismatch: Option<MismatchDetail>,
}

/// Detail surfaced only on disagreement. `kind` distinguishes the two
/// failure modes; `recipe_layers_for_same_z` is the layer count the
/// recipe's authored value would imply for the print's total Z-extent.
///
/// `#[serde(flatten)]` on `kind` merges the enum's tag-discriminated
/// fields into the struct, so JSON consumers see a flat object with a
/// `"kind": "uniform" | "variable"` discriminator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MismatchDetail {
    #[serde(flatten)]
    pub kind: MismatchKind,
    pub recipe_layers_for_same_z: u32,
}

/// Discriminator for the two disagreement modes a CTB / recipe pair can hit.
///
/// `Variable` is a unit variant; the min / max / mean summary it would
/// have carried is derivable from `LayerHeightProvenance::ctb_layer_heights()`
/// (the always-present `LayerHeightSeq`). Consumers — both the warning
/// formatter and downstream JSON readers — compute it on demand via
/// `seq.min_um()` / `max_um()` / `mean_um()` rather than carrying a
/// duplicate copy in the enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MismatchKind {
    /// The CTB is uniform but its single value differs from the recipe.
    Uniform { ctb_um: f32 },
    /// The CTB uses adaptive layer height (per-layer values differ
    /// among themselves). The runtime dispatches per-layer; the recipe
    /// is necessarily a poor approximation regardless of its value.
    Variable,
}

impl LayerHeightProvenance {
    /// Construct from the per-layer CTB authority and the resin
    /// recipe's authored value. Computes the mismatch detail
    /// automatically:
    ///
    /// - `ctb.uniform()` is `Some(u)` and `(u - recipe_um).abs() <= TOL_UM`
    ///   → `mismatch = None`
    /// - `ctb.uniform()` is `Some(u)` and the two disagree
    ///   → `Uniform { ctb_um: u }`
    /// - `ctb.uniform()` is `None`
    ///   → `Variable { min, max, mean }`
    ///
    /// Returns `Err` only on non-finite / non-positive `recipe_um`
    /// (the `LayerHeightSeq` already validates its own entries on
    /// construction).
    pub fn reconcile(ctb: LayerHeightSeq, recipe_um: f32) -> Result<Self, &'static str> {
        if !recipe_um.is_finite() || recipe_um <= 0.0 {
            return Err("recipe_um must be finite and > 0");
        }
        let mismatch = match ctb.uniform() {
            Some(u) if (u - recipe_um).abs() <= TOL_UM => None,
            Some(ctb_um) => {
                let n = ctb.len() as f64;
                let m_f = n * (ctb_um as f64) / (recipe_um as f64);
                Some(MismatchDetail {
                    kind: MismatchKind::Uniform { ctb_um },
                    recipe_layers_for_same_z: m_f.round() as u32,
                })
            }
            None => {
                // Variable / adaptive slicing. min/max/mean are
                // derivable from `self.ctb` on demand, so the enum
                // variant is a unit variant and we don't duplicate.
                let m_f = ctb.total_z_um() / (recipe_um as f64);
                Some(MismatchDetail {
                    kind: MismatchKind::Variable,
                    recipe_layers_for_same_z: m_f.round() as u32,
                })
            }
        };
        Ok(Self {
            ctb,
            recipe_um,
            mismatch,
        })
    }

    /// Borrow the per-layer CTB authority. Each layer's runtime physics
    /// dispatches from `ctb_layer_heights().get(layer_index)`.
    pub fn ctb_layer_heights(&self) -> &LayerHeightSeq {
        &self.ctb
    }

    /// The resin profile's `recipe.layer_height_um` for this run —
    /// informational; not used at runtime when this provenance exists.
    pub fn recipe_um(&self) -> f32 {
        self.recipe_um
    }

    /// `Some(detail)` when CTB and recipe disagree; `None` on agreement.
    pub fn mismatch(&self) -> Option<&MismatchDetail> {
        self.mismatch.as_ref()
    }

    /// `true` iff CTB and recipe disagree.
    pub fn has_mismatch(&self) -> bool {
        self.mismatch.is_some()
    }

    /// Number of layers in the CTB.
    pub fn layer_count(&self) -> u32 {
        self.ctb.len() as u32
    }

    /// The single uniform CTB layer height when every entry agrees
    /// within `TOL_UM`; `None` on adaptive / variable-Z prints.
    /// Cheap to call repeatedly.
    pub fn uniform_height_um(&self) -> Option<f32> {
        self.ctb.uniform()
    }

    /// Format the user-facing stderr warning text. Returns `Some(text)`
    /// on mismatch and `None` on agreement. Two branches per
    /// [`MismatchKind`]; both contain Mag's literal "GUESS" + "WRONG
    /// LAYER COUNT" framing from the issue. The variable branch is
    /// collision-aware: when the recipe's implied layer count happens
    /// to equal the CTB's actual count (which can happen by
    /// coincidence on adaptive prints), the text avoids the confusing
    /// "N layers, not the N this CTB has" form.
    pub fn format_warning(&self, profile_name: &str) -> Option<String> {
        let mismatch = self.mismatch.as_ref()?;
        let y = self.recipe_um;
        let n = self.layer_count();
        let m = mismatch.recipe_layers_for_same_z;
        let text = match &mismatch.kind {
            MismatchKind::Uniform { ctb_um } => {
                let x = *ctb_um;
                format!(
                    "WARNING: CTB layer_height ({x:.3} µm) does NOT match recipe \
                     layer_height ({y:.3} µm) in profile {profile_name}. The simulation \
                     will use the CTB's value ({x:.3} µm). The recipe's value is a \
                     GUESS — if applied it would produce the WRONG LAYER COUNT ({m} \
                     layers for the same Z-extent, not the {n} this CTB actually has). \
                     To remove this warning, re-slice at {y:.3} µm OR update profile \
                     {profile_name}'s recipe.layer_height_um to {x:.3} µm."
                )
            }
            MismatchKind::Variable => {
                // min/max/mean derived from the LayerHeightSeq on demand
                // (no duplicate state on the enum).
                let min_um = self.ctb.min_um();
                let max_um = self.ctb.max_um();
                let mean_um = self.ctb.mean_um();
                let count_clause = if m == n {
                    format!(
                        "the recipe value happens to imply {m} layers — same count, \
                         but the per-layer thicknesses themselves differ and the \
                         recipe cannot describe that"
                    )
                } else {
                    format!(
                        "the recipe value would imply the WRONG LAYER COUNT ({m} layers \
                         for the same Z-extent, not the {n} this CTB actually has)"
                    )
                };
                format!(
                    "WARNING: CTB uses variable layer height (adaptive slicing): {n} \
                     layers ranging from {min_um:.3} µm to {max_um:.3} µm (mean \
                     {mean_um:.3} µm). The recipe layer_height ({y:.3} µm) in profile \
                     {profile_name} is a GUESS — no single value can describe this \
                     print; {count_clause}. The simulation uses each layer's \
                     CTB-authoritative slab thickness and the recipe's value is \
                     ignored at runtime. To remove this warning, re-slice with uniform \
                     layer height — disable 'variable layer height' / 'adaptive \
                     slicing' in your slicer before re-exporting. (If your workflow \
                     legitimately uses adaptive slicing, the simulation output is \
                     correct for the file you have; this warning is documenting that \
                     any single recipe value will necessarily disagree with the CTB \
                     on at least one layer.)"
                )
            }
        };
        Some(text)
    }

    /// Render a single-line summary suitable for `report_health` text
    /// output. Includes the existing ` ⚠` suffix convention when
    /// mismatched (see resinsim-inspect main.rs lines 952/1029 for the
    /// prior art on thermal-degradation warnings).
    pub fn render_text_summary(&self) -> String {
        let suffix = if self.has_mismatch() { " ⚠" } else { "" };
        match self.uniform_height_um() {
            Some(ctb_um) => format!(
                "CTB layer_height: {ctb_um:.3} µm (recipe: {:.3} µm){suffix}",
                self.recipe_um
            ),
            None => format!(
                "CTB layer_height: {:.3}–{:.3} µm (variable; mean {:.3} µm, recipe \
                 {:.3} µm){suffix}",
                self.ctb.min_um(),
                self.ctb.max_um(),
                self.ctb.mean_um(),
                self.recipe_um,
            ),
        }
    }
}

/// Approx PartialEq:
/// - `ctb_layer_heights` compared element-wise within `TOL_UM`
/// - `recipe_um` compared within `TOL_UM`
/// - `mismatch` compared structurally (both None, or both Some with
///   matching kind variant + numeric fields within `TOL_UM`)
impl PartialEq for LayerHeightProvenance {
    fn eq(&self, other: &Self) -> bool {
        let f32_eq = |a: f32, b: f32| (a - b).abs() <= TOL_UM;
        if !f32_eq(self.recipe_um, other.recipe_um) {
            return false;
        }
        if self.ctb.len() != other.ctb.len() {
            return false;
        }
        for (a, b) in self.ctb.as_slice().iter().zip(other.ctb.as_slice().iter()) {
            if !f32_eq(*a, *b) {
                return false;
            }
        }
        match (&self.mismatch, &other.mismatch) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                if a.recipe_layers_for_same_z != b.recipe_layers_for_same_z {
                    return false;
                }
                match (&a.kind, &b.kind) {
                    (
                        MismatchKind::Uniform { ctb_um: a_u },
                        MismatchKind::Uniform { ctb_um: b_u },
                    ) => f32_eq(*a_u, *b_u),
                    // Variable has no fields; the per-layer Vec
                    // comparison above already covered the data.
                    (MismatchKind::Variable, MismatchKind::Variable) => true,
                    _ => false,
                }
            }
            _ => false,
        }
    }
}

// ---- Custom serde: skip-serialize the per-layer Vec when uniform -----------
//
// The sim.json schema-stability concern (round-1 code review MED-2):
// uniform CTBs are the common case, and embedding a 4500-element Vec
// of equal values is ~70 KB of duplicate noise. We serialise a flat
// `ctb_um` field instead for the uniform case; variable / adaptive
// CTBs still emit the full `ctb_layer_heights_um` Vec.
//
// Deserialise accepts both shapes via a one-shot struct-with-Option-
// fields reader; if both `ctb_layer_heights_um` and `ctb_um` are
// present (shouldn't happen with our serializer but conceivable from
// a future caller), the Vec wins.

impl Serialize for LayerHeightProvenance {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Uniform shape: ctb_um + layer_count (2 fields); Variable
        // shape: ctb_layer_heights_um (1 field). Plus recipe_um
        // (always) and the optional mismatch field.
        let ctb_field_count = if self.uniform_height_um().is_some() {
            2
        } else {
            1
        };
        let mismatch_field_count = if self.mismatch.is_some() { 1 } else { 0 };
        let field_count = 1 + ctb_field_count + mismatch_field_count;
        let mut s = serializer.serialize_struct("LayerHeightProvenance", field_count)?;
        match self.uniform_height_um() {
            Some(ctb_um) => {
                // Uniform: flatten to a single scalar; the Vec is
                // recoverable as `vec![ctb_um; layer_count]` if needed.
                s.serialize_field("ctb_um", &ctb_um)?;
                // `layer_count` makes the uniform shape self-describing
                // for downstream consumers that want N without seeing
                // the Vec.
                s.serialize_field("layer_count", &self.layer_count())?;
            }
            None => {
                // Variable: emit the full per-layer Vec for fidelity.
                // Consumers rendering adaptive prints rely on it.
                s.serialize_field("ctb_layer_heights_um", self.ctb.as_slice())?;
            }
        }
        s.serialize_field("recipe_um", &self.recipe_um)?;
        if let Some(m) = self.mismatch.as_ref() {
            s.serialize_field("mismatch", m)?;
        }
        s.end()
    }
}

// Wire-shape struct for Deserialize. All fields are optional so the
// reader can accept either the uniform or the variable serialisation
// shape; `try_from` then validates and converts to the canonical
// `LayerHeightProvenance` form.
#[derive(Deserialize)]
struct LayerHeightProvenanceWire {
    #[serde(default)]
    ctb_um: Option<f32>,
    #[serde(default)]
    ctb_layer_heights_um: Option<Vec<f32>>,
    #[serde(default)]
    layer_count: Option<u32>,
    recipe_um: f32,
    #[serde(default)]
    mismatch: Option<MismatchDetail>,
}

impl<'de> Deserialize<'de> for LayerHeightProvenance {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = LayerHeightProvenanceWire::deserialize(deserializer)?;
        let ctb = match (wire.ctb_layer_heights_um, wire.ctb_um, wire.layer_count) {
            (Some(v), _, _) => LayerHeightSeq::try_from_vec(v).map_err(serde::de::Error::custom)?,
            (None, Some(u), Some(n)) if n > 0 => LayerHeightSeq::try_from_vec(vec![u; n as usize])
                .map_err(serde::de::Error::custom)?,
            (None, Some(_), Some(_)) => {
                return Err(serde::de::Error::custom(
                    "layer_count must be > 0 in uniform form",
                ));
            }
            (None, Some(u), None) => {
                // Older form (or hand-written JSON): single ctb_um with
                // no layer_count. Reconstruct as a single-layer series.
                LayerHeightSeq::try_from_vec(vec![u]).map_err(serde::de::Error::custom)?
            }
            (None, None, _) => {
                return Err(serde::de::Error::custom(
                    "LayerHeightProvenance JSON must carry either \
                     `ctb_layer_heights_um` or `ctb_um`",
                ));
            }
        };
        // recipe_um validation matches the constructor.
        if !wire.recipe_um.is_finite() || wire.recipe_um <= 0.0 {
            return Err(serde::de::Error::custom("recipe_um must be finite and > 0"));
        }
        Ok(Self {
            ctb,
            recipe_um: wire.recipe_um,
            mismatch: wire.mismatch,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_INPUTS_MSG: &str =
        "test fixture: positional inputs satisfy reconcile() validate contract";
    const INFALLIBLE_SERDE_MSG: &str =
        "test fixture: LayerHeightProvenance round-trip is derive-Serialize-infallible";

    fn uniform_seq(n: usize, h: f32) -> LayerHeightSeq {
        LayerHeightSeq::try_from_vec(vec![h; n]).expect("uniform fixture")
    }

    #[test]
    fn agreement_yields_no_mismatch() {
        let p =
            LayerHeightProvenance::reconcile(uniform_seq(100, 40.0), 40.0).expect(VALID_INPUTS_MSG);
        assert!(!p.has_mismatch());
        assert!(p.mismatch().is_none());
        assert_eq!(p.layer_count(), 100);
        assert!((p.uniform_height_um().expect("uniform") - 40.0).abs() < 1e-6);
    }

    #[test]
    fn within_tolerance_yields_no_mismatch() {
        let p = LayerHeightProvenance::reconcile(uniform_seq(100, 40.0), 40.0_f32 + 5e-4)
            .expect(VALID_INPUTS_MSG);
        assert!(!p.has_mismatch());
    }

    #[test]
    fn uniform_disagreement_yields_uniform_mismatch() {
        // CTB=40, recipe=30, N=100 → recipe_layers = round(100*40/30) = 133.
        let p =
            LayerHeightProvenance::reconcile(uniform_seq(100, 40.0), 30.0).expect(VALID_INPUTS_MSG);
        assert!(p.has_mismatch());
        let m = p.mismatch().expect("populated");
        match &m.kind {
            MismatchKind::Uniform { ctb_um } => assert!((ctb_um - 40.0).abs() < 1e-6),
            MismatchKind::Variable => panic!("expected Uniform kind"),
        }
        assert_eq!(m.recipe_layers_for_same_z, 133);
    }

    #[test]
    fn variable_z_yields_variable_mismatch_with_summary() {
        let ctb = LayerHeightSeq::try_from_vec(vec![50.0, 30.0, 20.0, 30.0, 50.0]).expect("valid");
        let p = LayerHeightProvenance::reconcile(ctb, 40.0).expect(VALID_INPUTS_MSG);
        assert!(p.has_mismatch());
        let m = p.mismatch().expect("populated");
        assert!(
            matches!(m.kind, MismatchKind::Variable),
            "expected Variable kind, got {:?}",
            m.kind
        );
        // Summary stats are read from the LayerHeightSeq directly
        // (no duplicate state on the enum).
        let ctb = p.ctb_layer_heights();
        assert!((ctb.min_um() - 20.0).abs() < 1e-6);
        assert!((ctb.max_um() - 50.0).abs() < 1e-6);
        assert!((ctb.mean_um() - 36.0).abs() < 1e-3);
        // 180 µm total / 40 µm = 4.5 → round (half-away-from-zero) = 5.
        assert_eq!(m.recipe_layers_for_same_z, 5);
    }

    #[test]
    fn reconcile_rejects_nan_recipe() {
        assert!(LayerHeightProvenance::reconcile(uniform_seq(1, 40.0), f32::NAN).is_err());
    }

    #[test]
    fn reconcile_rejects_zero_recipe() {
        assert!(LayerHeightProvenance::reconcile(uniform_seq(1, 40.0), 0.0).is_err());
    }

    // ---- format_warning ----

    #[test]
    fn format_warning_returns_none_on_agreement() {
        let p =
            LayerHeightProvenance::reconcile(uniform_seq(100, 40.0), 40.0).expect(VALID_INPUTS_MSG);
        assert!(p.format_warning("Foo").is_none());
    }

    #[test]
    fn format_warning_uniform_branch_contains_mag_keywords() {
        let p = LayerHeightProvenance::reconcile(uniform_seq(4492, 40.0), 30.0)
            .expect(VALID_INPUTS_MSG);
        let text = p
            .format_warning("Elegoo Ceramic Grey V2")
            .expect("mismatch produces warning");
        assert!(text.contains("WARNING"));
        assert!(text.contains("GUESS"));
        assert!(text.contains("WRONG LAYER COUNT"));
        assert!(text.contains("40"));
        assert!(text.contains("30"));
        assert!(text.contains("Elegoo Ceramic Grey V2"));
        assert!(text.contains("re-slice"));
        assert!(text.contains("recipe.layer_height_um"));
        assert!(text.contains("5989")); // round(4492*40/30) = 5989
        assert!(text.contains("4492"));
    }

    #[test]
    fn format_warning_variable_branch_contains_min_max_mean() {
        let ctb = LayerHeightSeq::try_from_vec(vec![50.0, 30.0, 20.0, 30.0, 50.0]).expect("valid");
        let p = LayerHeightProvenance::reconcile(ctb, 100.0).expect(VALID_INPUTS_MSG);
        let text = p.format_warning("Test").expect("variable produces warning");
        assert!(text.contains("variable layer height"));
        assert!(text.contains("GUESS"));
        assert!(text.contains("WRONG LAYER COUNT"));
        // min/max/mean numerics surface.
        assert!(text.contains("20"));
        assert!(text.contains("50"));
        assert!(text.contains("36"));
        // Slicer-side hint surfaces.
        assert!(text.contains("adaptive slicing"));
        assert!(text.contains("disable"));
    }

    #[test]
    fn format_warning_variable_branch_collision_aware() {
        // 5 layers @ 30/40/50/40/30 = 190 µm total; recipe=38 → round(5) = 5.
        // The recipe-implied count matches the CTB's count — collision case.
        let ctb = LayerHeightSeq::try_from_vec(vec![30.0, 40.0, 50.0, 40.0, 30.0]).expect("valid");
        let p = LayerHeightProvenance::reconcile(ctb, 38.0).expect(VALID_INPUTS_MSG);
        assert_eq!(
            p.mismatch()
                .expect("variable populated")
                .recipe_layers_for_same_z,
            5
        );
        let text = p.format_warning("Test").expect("variable warning");
        // The collision branch should avoid the "N layers, not the N" form.
        assert!(
            !text.contains("WRONG LAYER COUNT (5 layers for the same Z-extent, not the 5"),
            "collision branch must not produce confusing N≠N text: {text}"
        );
        // It should still surface that the recipe is a guess + acknowledge
        // the collision.
        assert!(text.contains("happens to imply"));
        assert!(text.contains("same count"));
        assert!(text.contains("GUESS"));
        // The WRONG LAYER COUNT keyword does NOT appear in collision text —
        // we replaced that clause with the collision-aware form.
        assert!(
            !text.contains("WRONG LAYER COUNT"),
            "collision branch must not say WRONG LAYER COUNT (it isn't): {text}"
        );
    }

    // ---- render_text_summary ----

    #[test]
    fn render_text_summary_uniform_agreement_no_suffix() {
        let p =
            LayerHeightProvenance::reconcile(uniform_seq(5, 40.0), 40.0).expect(VALID_INPUTS_MSG);
        let s = p.render_text_summary();
        assert!(s.starts_with("CTB layer_height: 40.000 µm (recipe: 40.000 µm)"));
        assert!(!s.contains('⚠'));
    }

    #[test]
    fn render_text_summary_uniform_mismatch_has_warn_suffix() {
        let p =
            LayerHeightProvenance::reconcile(uniform_seq(5, 40.0), 30.0).expect(VALID_INPUTS_MSG);
        let s = p.render_text_summary();
        assert!(s.contains("40.000 µm"));
        assert!(s.contains("recipe: 30.000 µm"));
        assert!(s.ends_with(" ⚠"));
    }

    #[test]
    fn render_text_summary_variable_includes_min_max_mean_units() {
        let ctb = LayerHeightSeq::try_from_vec(vec![30.0, 40.0, 50.0]).expect("valid");
        let p = LayerHeightProvenance::reconcile(ctb, 40.0).expect(VALID_INPUTS_MSG);
        let s = p.render_text_summary();
        // µm appears on min/max range, mean, AND recipe — all four
        // values have explicit units.
        assert!(s.contains("30.000–50.000 µm"));
        assert!(s.contains("mean 40.000 µm"));
        assert!(s.contains("recipe 40.000 µm"));
        assert!(s.ends_with(" ⚠"));
    }

    // ---- Approx equality (PartialEq) ----

    #[test]
    fn approx_eq_within_tolerance() {
        let a =
            LayerHeightProvenance::reconcile(uniform_seq(100, 40.0), 40.0).expect(VALID_INPUTS_MSG);
        let b =
            LayerHeightProvenance::reconcile(uniform_seq(100, 40.0_f32 + 5e-4), 40.0_f32 - 5e-4)
                .expect(VALID_INPUTS_MSG);
        assert_eq!(a, b);
    }

    #[test]
    fn approx_neq_when_one_has_mismatch_other_does_not() {
        let agree =
            LayerHeightProvenance::reconcile(uniform_seq(100, 40.0), 40.0).expect(VALID_INPUTS_MSG);
        let disagree =
            LayerHeightProvenance::reconcile(uniform_seq(100, 40.0), 30.0).expect(VALID_INPUTS_MSG);
        assert_ne!(agree, disagree);
    }

    #[test]
    fn approx_neq_uniform_vs_variable_mismatch_kinds() {
        let uniform_mm =
            LayerHeightProvenance::reconcile(uniform_seq(5, 40.0), 30.0).expect(VALID_INPUTS_MSG);
        let ctb = LayerHeightSeq::try_from_vec(vec![40.0, 30.0, 40.0, 30.0, 40.0]).expect("valid");
        let variable_mm = LayerHeightProvenance::reconcile(ctb, 30.0).expect(VALID_INPUTS_MSG);
        assert_ne!(uniform_mm, variable_mm);
    }

    // ---- serde shape ----

    #[test]
    fn serde_uniform_omits_per_layer_vec() {
        let p = LayerHeightProvenance::reconcile(uniform_seq(4492, 40.0), 30.0)
            .expect(VALID_INPUTS_MSG);
        let json = serde_json::to_value(&p).expect(INFALLIBLE_SERDE_MSG);
        // Uniform shape: `ctb_um` + `layer_count`, no Vec.
        assert!(
            json.get("ctb_layer_heights_um").is_none(),
            "uniform CTB must NOT embed Vec (saves bytes on common case): {json}"
        );
        assert_eq!(
            json["ctb_um"].as_f64().expect("ctb_um is a number").round() as i64,
            40
        );
        assert_eq!(json["layer_count"].as_u64().expect("layer_count"), 4492);
        // Round-trip succeeds.
        let p2: LayerHeightProvenance = serde_json::from_value(json).expect(INFALLIBLE_SERDE_MSG);
        assert_eq!(p, p2);
    }

    #[test]
    fn serde_variable_emits_per_layer_vec() {
        let ctb = LayerHeightSeq::try_from_vec(vec![30.0, 40.0, 50.0]).expect("valid");
        let p = LayerHeightProvenance::reconcile(ctb, 40.0).expect(VALID_INPUTS_MSG);
        let json = serde_json::to_value(&p).expect(INFALLIBLE_SERDE_MSG);
        // Variable shape: full Vec, no `ctb_um` scalar.
        assert!(json.get("ctb_um").is_none(), "variable JSON: {json}");
        let arr = json["ctb_layer_heights_um"]
            .as_array()
            .expect("Vec present");
        assert_eq!(arr.len(), 3);
        let p2: LayerHeightProvenance = serde_json::from_value(json).expect(INFALLIBLE_SERDE_MSG);
        assert_eq!(p, p2);
    }

    #[test]
    fn serde_round_trip_agreement_omits_mismatch_field() {
        let p =
            LayerHeightProvenance::reconcile(uniform_seq(100, 40.0), 40.0).expect(VALID_INPUTS_MSG);
        let json = serde_json::to_value(&p).expect(INFALLIBLE_SERDE_MSG);
        assert!(json.get("mismatch").is_none(), "agreement: {json}");
        let p2: LayerHeightProvenance = serde_json::from_value(json).expect(INFALLIBLE_SERDE_MSG);
        assert_eq!(p, p2);
    }

    #[test]
    fn serde_round_trip_variable_has_kind_variable() {
        let ctb = LayerHeightSeq::try_from_vec(vec![30.0, 40.0, 50.0]).expect("valid");
        let p = LayerHeightProvenance::reconcile(ctb, 100.0).expect(VALID_INPUTS_MSG);
        let json = serde_json::to_value(&p).expect(INFALLIBLE_SERDE_MSG);
        let kind = json["mismatch"]["kind"]
            .as_str()
            .expect("kind discriminator is a string");
        assert_eq!(kind, "variable");
    }

    #[test]
    fn serde_round_trip_uniform_mismatch_has_kind_uniform() {
        let p =
            LayerHeightProvenance::reconcile(uniform_seq(100, 40.0), 30.0).expect(VALID_INPUTS_MSG);
        let json = serde_json::to_value(&p).expect(INFALLIBLE_SERDE_MSG);
        let kind = json["mismatch"]["kind"]
            .as_str()
            .expect("kind discriminator");
        assert_eq!(kind, "uniform");
    }

    #[test]
    fn serde_load_legacy_shape_with_ctb_um_only() {
        // Older / hand-written JSON: just `ctb_um` and `recipe_um`,
        // no `layer_count`. Should reconstruct as a single-layer series.
        let json = serde_json::json!({
            "ctb_um": 40.0,
            "recipe_um": 40.0,
        });
        let p: LayerHeightProvenance = serde_json::from_value(json).expect(INFALLIBLE_SERDE_MSG);
        assert_eq!(p.layer_count(), 1);
        assert!(p.mismatch.is_none());
    }

    #[test]
    fn serde_rejects_neither_ctb_um_nor_vec() {
        let json = serde_json::json!({
            "recipe_um": 40.0,
        });
        let r: Result<LayerHeightProvenance, _> = serde_json::from_value(json);
        assert!(
            r.is_err(),
            "must reject JSON without either ctb_um or ctb_layer_heights_um"
        );
    }
}
