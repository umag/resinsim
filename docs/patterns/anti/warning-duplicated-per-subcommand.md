---
issue: kb153-warning-missing-from-resinsim-sim
date: 2026-08-02
---

# Anti-pattern: User-facing warning duplicated per subcommand

## The failure shape

A provenance advisory (the KB-153 "cure-kinetics Ea = 30 kJ/mol
(literature midpoint estimate)" warning) was emitted by TWO subcommands via
two hand-maintained `eprintln!` copies — `cmd_thermal` and
`cmd_report_health`. When ADR-0015 (`1cbcc0c`) rewrote `report health`
into an envelope consumer and introduced `resinsim sim` as the producer,
the second copy was deleted and never carried to the new producer. Nothing
failed; the regression sat for over two months until UAT drift repair
asserted the spec's claim against the binary.

## Why it survives

- Each copy looks complete in its own function; no compiler ties them.
- New subcommands default to NOT warning — the safe-looking omission.
- Tests, if any, pin the copies that exist, not the surfaces that should.

## The fix shape

Emit at the SHARED SEAM every relevant command already passes through —
here `profile_loader::load_resin`, the unique funnel for every resin load
(`resolve_profiles` delegates to it). Structure as a pure policy function
(`cure_kinetics_ea_default_warning(&ResinProfile) -> Option<String>`, the
single owner of the wording, unit-testable with a byte-identity assertion)
plus exactly ONE `eprintln!` call site. A new subcommand then cannot
forget the warning, and double-emission is structurally impossible.

## Detection

- Count-based tests per surface: `<cmd>_warns_exactly_once`
  (`stderr.matches("KB-153").count() == 1`) on every loader path.
- A deliberate-silence pin for surfaces that must NOT warn
  (`report_health_in_does_not_warn`, naming the follow-up issue that will
  retire it).
- `git log -S<warning-text>` finds copy deletions during refactors.

## See also

- `docs/patterns/anti/spec-edited-step-regex-not.md` — the drift class
  that eventually surfaced this regression
- `docs/patterns/anti/audit-criteria-name-deleted-symbols.md` — sibling
  refactor-loses-a-copy shape for acceptance criteria
