---
issue: 01-viz-crate-scaffold
date: 2026-04-26
---

# Anti-pattern: pinning a dep version from a web-search summary alone

## Symptom

A plan or ADR cites a version pin like `bevy_panorbit_camera = "0.34"`
with the rationale "0.34 supports Bevy 0.16 (per web search)". At
impl time `cargo build` fails with a "two versions of bevy_app in the
dependency graph" error because the dep actually ships against a
different Bevy minor version than the search summary claimed.

## Why it happens

Search-engine snippets and AI-summarised search results compress
across versions and dates. They often surface a generic "X supports
Bevy Y" sentence without anchoring to the specific X version's
`Cargo.toml`. For ecosystem libraries on the Bevy/wgpu treadmill,
where every minor is breaking, a wrong sentence inside a confident
summary is a real risk.

## What to do instead

For every pin in a planning artifact (plan summary, ADR, Cargo.toml
snippet):

1. Open the **canonical docs.rs page** of the exact crate version
   you intend to pin. The header shows the supported Bevy version
   (e.g. `bevy ^0.18`).
2. If docs.rs is sparse, fetch the version's `Cargo.toml` from
   crates.io or the project's GitHub at the matching tag.
3. Cite the canonical source in the plan, not the search summary.

For the bevy ecosystem specifically: match the **Cargo dependency
constraint exactly**, not the Bevy "minor" mentioned in prose. A
crate documented as "for Bevy 0.16" may actually carry
`bevy = "0.16, 0.17, 0.18"` (loose) or `bevy = "0.18"` (strict);
only the constraint matters at compile time.

## Cost of skipping verification

In 01-viz-crate-scaffold the cost was one impl-time iteration:
`cargo build` failure → `WebFetch docs.rs/bevy_panorbit_camera/0.34`
→ pin Bevy 0.18 instead of 0.16 → fix `EventWriter` and `AmbientLight`
collateral drifts. That's ~10 minutes. For a larger Phase 2 issue
where the version impacts are deeper (multiple bevy_egui-or-equivalent
deps disagreeing), the cost compounds.

## See also

- Pattern `bevy-0.16-to-0.18-migration-notes.md` — the collateral
  API drifts that followed the version bump
- ADR-0010 — Bevy 0.18 section recording the bump rationale
