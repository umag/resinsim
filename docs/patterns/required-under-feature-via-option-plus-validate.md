---
issue: t2f4-thermal-diffusion
date: 2026-05-21
status: pattern
---

# Pattern: Required-under-feature via Option<T> + validate-time check

## Context

When a Cargo feature flag gates a capability (e.g. `field-sim` for
Tier-2 voxel simulation), some struct fields become semantically
REQUIRED only under that feature:

- Under default builds, the field is unused → it can be `None`.
- Under the feature, the field is consumed by the new code path →
  it MUST be `Some` or the simulation can't run.

If both builds share the SAME TOML configuration files (as ResinSim
does: `data/resins/*.toml` are read by both default and field-sim
binaries), the field type must be deserializable by both binaries.

## The naive options

1. **`field: f32` (no Option, no default)** — Required at parse time.
   - Default-feature build: rejects TOMLs without the field.
   - Field-sim build: rejects TOMLs without the field.
   - Cross-feature interchange: **broken** — adding the field to a
     TOML for field-sim breaks the default binary too.

2. **`field: f32` with `#[serde(default = "...")]`** — Optional at
   parse time, defaulted to a literature midpoint.
   - Default-feature build: accepts TOMLs without the field
     (uses the default).
   - Field-sim build: accepts TOMLs without the field
     (silently uses the default — DEFEATS the required-under-field-sim
     semantic).

3. **`field: Option<f32>` with `#[serde(default)]`** — Optional at
   parse time, None when absent. **The pattern.**

## Pattern

Use option 3 — `Option<T>` on the struct, `#[serde(default)]` for
absent-field semantics, validate-time check under
`#[cfg(feature = "field-sim")]`:

```rust
#[derive(Deserialize)]
pub struct ResinProfile {
    pub name: String,
    pub thermal_conductivity_w_mk: Option<f32>,
    pub specific_heat_j_kgk: Option<f32>,
    // ... other fields ...
}

impl ResinProfile {
    pub fn validate(&self) -> Result<(), String> {
        // ... common checks ...
        #[cfg(feature = "field-sim")]
        {
            if self.thermal_conductivity_w_mk.is_none() {
                return Err(
                    "thermal_conductivity_w_mk is required under \
                     the field-sim feature (see KB-XYZ). Set it in \
                     the resin TOML.".into()
                );
            }
            // ... per-field required checks ...
        }
        Ok(())
    }
}
```

## What this preserves

- **Cross-feature TOML interchange.** A TOML written by a field-sim
  binary parses cleanly under a default-feature binary (the new
  fields deserialise to `Option<T>` and the validate() under default
  doesn't check them).
- **Strict required-under-feature semantics.** Field-sim binaries
  reject TOMLs missing the fields with a typed, actionable error.
- **No `#[serde(deny_unknown_fields)]` requirement.** The pattern
  works WITHOUT deny_unknown_fields, avoiding the cross-feature
  field-addition friction.

## What you give up

- **One layer of type safety.** Downstream consumers of the field
  must `.expect()`/`.unwrap()` the Option (or thread it through
  `Result`). The promise "validate() guarantees Some under
  field-sim" is enforced at runtime, not at compile time.
- **A consumer that doesn't call validate() first** can hit the
  `.expect()`. Defend with a type-state wrapper if the cost is
  acceptable (`ValidatedResinProfile`) — filed as a polish
  follow-on for t2f4.

## Consumer pattern

At consumer sites (inside the feature-gated path), the validate()
guarantee is documented in the .expect() message so a future reader
sees the contract:

```rust
let k_resin = resin
    .thermal_conductivity_w_mk()
    .expect("validate() guarantees thermal_conductivity_w_mk Some under field-sim");
```

A `ValidatedResinProfile` newtype that wraps `ResinProfile` after
successful validate() would lift this to the type system; filed as
a polish follow-on (the `.expect()` chain at simulation_runner.rs
lines 642-660 is fragile but the runner always calls validate()
at entry today).

## See also

- ADR-0020 §Consequences — the field-sim validate-time policy.
- ResinProfile + PrinterProfile thermal material fields (added by
  t2f4 step 2) — the canonical implementation.
- `docs/patterns/typed-temperature-boundary.md` — sibling pattern
  for the OUTER trust boundary (CLI/TOML scalar parse).
- `docs/patterns/anti/toml-inline-keys-nest-into-preceding-table.md`
  — the anti-pattern this pattern's `deny_unknown_fields = off`
  posture exposes (mitigated by the validate-time check).
