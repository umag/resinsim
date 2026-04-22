---
issue: recipe-aware-time-and-thermal
date: 2026-04-22
---

# Pattern: Typed boundary for untrusted scalar inputs

## Context

Physics services in resinsim-core accept scalar parameters (temperatures, Ea
values, thermal constants) through deep call stacks:

- CLI flag → clap `f32` → cmd handler → `SimulationRunner::run_*` →
  `FailurePredictor::predict_layer` → `ThermalCalculator::*` →
  `VatTemperature::new` (the first constructor that actually validates).

If the validating constructor panics via `.expect()` on malformed input, every
caller between the CLI and the constructor becomes a silent pass-through —
"trust the caller" is enforced only by prose docstrings. Adversarial round-1
review on `recipe-aware-time-and-thermal` found two HIGH-severity crash paths
where `--initial-led-temp=NaN` or a `reference_temp_c = -400.0` TOML field
panicked mid-simulation with a stack trace instead of returning an error.

## Pattern

Introduce a newtype at the trust boundary — where raw `f32` enters a service
API from an external source — rather than deferring validation to a downstream
constructor.

**Layer 1 — Value type with validating constructor** (`values/thermal.rs`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct InitialLedTemperature(f32);

impl InitialLedTemperature {
    const ABSOLUTE_ZERO_C: f32 = -273.15;
    pub fn new(celsius: f32) -> Result<Self, String> {
        if !celsius.is_finite() {
            return Err(format!("initial LED temperature must be finite, got {celsius}"));
        }
        if celsius <= Self::ABSOLUTE_ZERO_C {
            return Err(format!(
                "initial LED temperature must be above absolute zero ({:.2} °C), got {celsius}",
                Self::ABSOLUTE_ZERO_C
            ));
        }
        Ok(Self(celsius))
    }
    pub fn value(&self) -> f32 { self.0 }
}
```

**Layer 2 — Service API uses the typed form**:

```rust
pub fn run_auto(
    // ...
    ambient: AmbientTemperature,
    initial_led_temp: Option<InitialLedTemperature>,
) -> Result<PrintSimulation, String> { /* ... */ }
```

No `.expect()` panic on the path from here to the physics formula — the type
system has already proven the input is in-domain.

**Layer 3 — CLI parse-time validation**:

```rust
let ambient_typed = match AmbientTemperature::new(ambient_f32) {
    Ok(v) => v,
    Err(e) => {
        eprintln!("invalid --ambient: {e}");
        std::process::exit(2);
    }
};
```

The user gets an actionable error, exit code 2, before any simulation work
begins.

## When to use

- Any service API whose panicking `.expect()` is reached via a chain of raw-f32
  parameters, where at least one link is user-controlled (CLI flag, TOML field,
  API payload).
- When a shared invariant is enforced multiple layers deep but not at the
  top-level entry point — the type encodes the invariant once.
- When related primitives (e.g. ambient °C, LED °C, vat °C) share bounds but
  have different semantics — newtypes prevent argument-order swaps at compile
  time.

## When NOT to use

- Purely internal functions where every caller is code you control and the
  invariant is already proven by construction (e.g. intermediate math results).
  Over-typing internal plumbing is churn without safety gain.
- Formulas at the bottom of the stack whose `f32` signature is fixed by a
  KB-150-era contract — convert at the boundary via `.value()` rather than
  rippling the change through.

## Consequences

- Breaking change to service APIs that formerly took raw `f32` — update
  callers in lockstep. On `recipe-aware-time-and-thermal`, this meant 13+
  test callsites across `simulation_runner.rs` + integration tests; manageable
  because the codebase is small.
- `f32` → typed at the boundary adds one unwrap point per CLI flag. The cost
  is 4–6 lines per flag; the payoff is graceful user-facing errors.

## Testing

Each newtype gets four constructor tests in the `values/` unit suite:

- `new_rejects_nan` — `new(f32::NAN)` returns Err.
- `new_rejects_below_absolute_zero` — `new(-273.15)` AND `new(-300.0)` Err.
- `new_rejects_infinity` — `new(f32::INFINITY)` AND `new(f32::NEG_INFINITY)` Err.
- `new_accepts_normal` — `new(22.0).value() == 22.0` round-trips.

CLI regression test exercises the `std::process::Command` surface with a
malformed flag; asserts non-zero exit + stderr naming the flag and the
violated bound (see `thermal_cli_warnings.rs::thermal_rejects_invalid_initial_led_temp`
for the template).

## See also

- `values/thermal.rs` — `VatTemperature`, `InitialLedTemperature`,
  `AmbientTemperature` implementations.
- `docs/patterns/entity-validate-on-mutation.md` — sibling pattern at the
  entity layer (ResinProfile / PrinterProfile).
- `docs/patterns/nan-two-layer-defence.md` — precursor at the scalar-math
  layer (CureCalculator assert guards).
- `docs/adr/0007-led-and-vat-as-separate-temperatures.md` — the domain
  context that motivated the expansion.
