---
issue: sim-json-envelope-ea-default-flag
date: 2026-08-05
---

# UAT: `report health` surfaces the KB-153 default-Ea advisory on the consumer path

## Rationale

`resinsim sim` (the producer) has always warned on stderr when the loaded
resin's TOML omits a measured `cure_kinetics_ea_kj_mol` — the KB-153
"literature midpoint estimate" advisory, emitted from the single shared
seam `profile_loader::load_resin`. `report health --in` (the consumer)
loads no resin TOML, so it cannot go through that seam directly; instead
the producer stamps a top-level `cure_kinetics_ea_is_default` flag into the
sim.json envelope, and the consumer re-derives the advisory from that flag
via `profile_loader::warn_if_envelope_ea_is_default`
(`sim-json-envelope-ea-default-flag`,
`docs/patterns/anti/warning-duplicated-per-subcommand.md`).

Today this consumer-path contract is pinned only by nextest
(`crates/resinsim-inspect/tests/thermal_cli_warnings.rs`'s four
`report_health_*` tests and `report_health_time_cli.rs`). Per
`agent-constraints/uat-conventions.md`, a user-visible CLI contract belongs
in a UAT spec, not only in an internal Rust test — this spec earns its
place by asserting the USER-VISIBLE contract (which stream carries what,
the exactly-once property, the three-way flag semantics) without
duplicating those tests' plumbing.

The flag itself is a three-valued wire contract, ADR-0002's "`Option<T>`,
not a sentinel" principle made executable at the CLI boundary:
`Some(true)` (measured Ea's absence was detected and stamped), `Some(false)`
(a measured Ea was found), and `None` (an older or hand-written envelope
that predates the flag) are three different claims, not two. UAT-2 and
UAT-3 below are the `Some(false)` and `None` cases respectively — both stay
silent, but for different reasons, and each scenario proves which reason
by also checking the envelope's own flag value rather than only the
absence of the advisory. UAT-3 is ADR-0002's "absence is not false"
accepted false negative, made executable: `report health --in` cannot
recover a fact an older producer never recorded, and correctly does not
guess.

No shipped resin TOML carries a measured `cure_kinetics_ea_kj_mol`, so the
advisory is AMBIENT on stderr for every default-profile run in this tree —
UAT-1 is the common case, not a corner case.

## UAT-1: warns exactly once on a flagged default-Ea envelope

```gherkin
Scenario: report health warns once on a flagged default-Ea envelope
  Given a sim.json produced by a real `resinsim sim` against shipped profiles, whose cure_kinetics_ea_is_default is true
  When the user invokes `resinsim report health --in <EA_ENVELOPE>`
  Then the process exits with code 0
  And stderr carries the needles "30 kJ/mol", "literature midpoint estimate", and "KB-153"
  And stderr carries the consumer-context line naming "sim.json envelope" and the "resinsim sim" remedy
  And the advisory appears exactly once
  And stdout carries the health report and none of the KB-153 needles
```

## UAT-2: silent on a measured-Ea envelope

```gherkin
Scenario: report health stays silent on a measured-Ea envelope
  Given a sim.json produced against a resin profile with a measured cure_kinetics_ea_kj_mol
  When the user invokes `resinsim report health --in <EA_ENVELOPE>`
  Then the process exits with code 0
  And stderr carries no KB-153 needle
  And the envelope's cure_kinetics_ea_is_default is false
```

## UAT-3: silent on a pre-flag envelope (the accepted false negative)

```gherkin
Scenario: report health stays silent on a pre-flag envelope
  Given a sim.json envelope whose cure_kinetics_ea_is_default key has been stripped
  When the user invokes `resinsim report health --in <EA_ENVELOPE>`
  Then the process exits with code 0
  And stderr carries no KB-153 needle
  And stdout still carries the health report
```
