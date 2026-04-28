---
issue: t1f4
date: 2026-04-17
---

# UAT: Cure depth NaN guard

**ADR-0015 note.** This NaN-guard invariant lives below the
producer/consumer split. The pipeline `resinsim sim → report health --in`
preserves it because the guard sits at `Energy::new` / `cure_depth` —
upstream of envelope serialisation. Sim-time rejection of NaN inputs
keeps NaN out of any sim.json; downstream consumers therefore never see
NaN cure-depth values they'd have to handle.

## UAT-1: Invalid critical energy is caught before cure depth calculation

**Rationale.** T1-F4 root bug: `Energy::new` accepted NaN (`NaN <= 0.0` is
false in Rust), allowing a zero/NaN critical energy to reach `cure_depth`
where it would produce a silent physics failure (`is_sufficient` returning
false for NaN) or a confusing panic. The runtime guard at `cure_depth` entry
is the last-line defence.

```gherkin
Scenario: Invalid critical energy is caught before cure depth calculation
  Given a resin profile with a critical energy Ec that is zero or non-finite
  When the Beer-Lambert cure depth calculator runs for a layer
  Then the system fails loudly with a clear diagnostic message referencing critical_energy
  And does NOT silently return a negative cure depth or a false is_sufficient result
```

## UAT-2: NaN scale factor from uniformity does not propagate silently

**Rationale.** ADV round 1 HIGH finding: `Energy::scale` bypassed `Energy::new`
with only a `debug_assert!`, so a NaN intensity factor from
`uniformity_calculator` (e.g. NaN `profile.variation`) could produce
`Energy(NaN)` in release builds and reach `cure_depth` silently.

```gherkin
Scenario: NaN scale factor from uniformity does not propagate silently
  Given a print with a uniformity profile where the intensity factor computation produces a non-finite value
  When the uniformity-corrected cure depth is computed
  Then the system panics with a clear "scale factor must be positive and finite" message from Energy::scale
  And does NOT silently produce a NaN cure depth that is misinterpreted as undercure
```
