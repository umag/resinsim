---
issue: recipe-aware-time-and-thermal (re-pointed 2026-04-28 for ADR-0015)
date: 2026-04-22
---

# UAT: CLI temperature flags reject unphysical values at parse time

**ADR-0015 note.** The `--initial-led-temp` and `--ambient` flags now live
on `resinsim sim` (the producer), not `report health` (the consumer).
The validation contract is unchanged — typed parse-time rejection with
non-zero exit and no panic — but the surface that fields the flag has
moved.

## UAT-1: `--initial-led-temp` rejects values at/below absolute zero

**Rationale.** The round-2 adversarial review on
`recipe-aware-time-and-thermal` found that a malformed `--initial-led-temp`
panicked mid-simulation via `VatTemperature::new().expect()`. The
`InitialLedTemperature` newtype + parse-time validation at
`cmd_thermal` / `cmd_sim` (formerly `cmd_report_health`) was introduced to
convert that crash into an actionable user-facing error.

```gherkin
Scenario: UAT-1 --initial-led-temp rejects values at/below absolute zero
  Given the resinsim inspect thermal OR resinsim sim subcommand
  When the user invokes it with "--initial-led-temp=-300"
  Then the process exits with a non-zero code (2)
  And stderr names the flag "initial" or "invalid" AND the phrase "absolute zero"
  And no simulation rows are printed on stdout
```

## UAT-2: `--initial-led-temp=NaN` rejects without panic

```gherkin
Scenario: UAT-2 --initial-led-temp=NaN rejects without panic
  Given the resinsim inspect thermal OR resinsim sim subcommand
  When the user invokes it with "--initial-led-temp NaN"
  Then the process exits with a non-zero code
  And the error path does NOT produce a Rust panic / stack trace
```

## UAT-3: `--ambient` rejects values at/below absolute zero

**Rationale.** Round-4 extended the typed-boundary pattern to cover
`--ambient`; the same invariant applies symmetrically.

```gherkin
Scenario: UAT-3 --ambient rejects unphysical values
  Given the resinsim inspect thermal subcommand OR resinsim sim
  When the user invokes it with "--ambient=-300" or "--ambient=NaN"
  Then the process exits with code 2
  And stderr names the flag ("invalid --ambient") AND the violated bound
```

## UAT-4: loud warning when resin TOML lacks measured Ea_cure

**Rationale.** KB-153's 30 kJ/mol literature-midpoint default is an
ESTIMATE that may be wrong by ±50 %. Downstream users must see the warning
across every surface (inspect thermal, report health) so they don't
mistakenly treat Ec(T) drift as measured.

```gherkin
Scenario: UAT-4 loud warning when resin TOML lacks measured Ea_cure
  Given a resin profile whose TOML omits "cure_kinetics_ea_kj_mol"
  When the user invokes "resinsim inspect thermal --resin <that> --printer <any>"
  Then stderr contains the strings "30 kJ/mol", "literature midpoint estimate", and "KB-153"
  And the warning surfaces in "resinsim sim" (the producer that loads profiles, post-ADR-0015) as well (not just "inspect thermal")
```

## UAT-5: measured Ea_cure suppresses the warning

```gherkin
Scenario: UAT-5 measured Ea_cure suppresses the warning
  Given a resin profile whose TOML includes a finite positive "cure_kinetics_ea_kj_mol" in (0.0, 200.0]
  When the user invokes "resinsim inspect thermal --resin <that>"
  Then stderr does NOT contain "30 kJ/mol"
  And the JSON output path (when --json) carries "cure_kinetics_ea_is_default": false
```

## UAT-6: two-stage thermal plateau approaches the fitted Mars 5 Ultra value

**Rationale.** ADR-0007 + KB-152 replaced the single-stage KB-150 model
with a two-stage LED → vat coupling. A 2000-layer simulation at
ambient 23 °C, initial LED 27 °C on the Mars 5 Ultra (Tilt release,
coupling 0.71, led_delta 13.5, τ 4000 s) must approach the plateau
predicted by the formula.

```gherkin
Scenario: UAT-6 two-stage thermal plateau approaches fitted Mars 5 Ultra value
  Given PrinterProfile::elegoo_mars5_ultra() + ResinProfile::generic_standard()
  When SimulationRunner::run_from_areas runs 3500+ layers at ambient = 23 °C, initial_led = 27 °C
  Then the vat temperature at cumulative time ≥ 4 h exceeds half-rise
  And the vat temperature at cumulative time ≥ 8 h is within ±1 °C of the 4 h sample
  And the cure depth at the thermal plateau on a normal-phase layer EXCEEDS the cure depth at an earlier normal-phase layer (Ec(T) correction)
```
