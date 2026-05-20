---
issue: t2f3.5-voxel-field-persistence
date: 2026-05-21
---

# UAT: Loading a large sidecar requires the same `RESINSIM_MAX_FIELD_BYTES` as the producer used

## Rationale

ADR-0019 §"Consumer-side budget" documents the producer / consumer
asymmetry: the producer set a permissive `RESINSIM_MAX_FIELD_BYTES`
to write a sidecar where the strain field claims > 4 GB allocation;
the consumer running with the default 4 GB cap rejects the read with
a typed `"exceeds field budget"` error. The sidecar bytes are valid
+ sha256 OK; only the env-side cap is wrong.

Discovered during E2E validation on lilith torso + Elegoo Ceramic
Grey V2 + Elegoo Mars 5 Ultra at 30 µm. The error is loud (typed
substring) but not yet actionable (doesn't name the override). This
UAT pins the error-substring contract for downstream tooling /
human-grep; a future enhancement will surface the producer's budget
in the error itself.

See also:
- `docs/patterns/anti/producer-consumer-env-budget-asymmetry.md` —
  anti-pattern doc generalising this issue class.
- `cli-sim-rejects-tampered-sidecar.md` UAT-3 (`implausible
  layer_count` adjacent budget check).

## UAT-1: consumer with default budget rejects oversized sidecar

```gherkin
Scenario: UAT-1 default-budget consumer cannot read producer's permissive-budget sidecar
  Given a sidecar produced by `resinsim sim --voxel-cure-mm 0.05
    --resin elegoo_ceramic_grey_v2 --printer elegoo_mars5_ultra
    --file <lilith>.ctb --out model.sim.json` with
    `RESINSIM_MAX_FIELD_BYTES=17179869184`
  And the sidecar's strain field claims > 4 GB allocation
  When the consumer runs `resinsim report health --in model.sim.json`
    without the env override (default `MAX_FIELD_ALLOCATION_BYTES = 4 GB`)
  Then the process exits with non-zero code
  And stderr mentions "exceeds field budget for strain"
  And the process does not panic
```

## UAT-2: consumer with matching budget succeeds

```gherkin
Scenario: UAT-2 RESINSIM_MAX_FIELD_BYTES override allows oversized sidecar load
  Given the same paired sim.json + fields.bin from UAT-1
  When the consumer runs `RESINSIM_MAX_FIELD_BYTES=17179869184 \
    resinsim report health --in model.sim.json`
  Then the process exits with code 0
  And the report renders all four voxel-derived sections (strain
    gradient, stress max, etc.)
```

## UAT-3 (future): producer's budget is stamped into the envelope

```gherkin
Scenario: UAT-3 [future] consumer error mentions the producer's RESINSIM_MAX_FIELD_BYTES
  Given a sidecar produced with `RESINSIM_MAX_FIELD_BYTES=17179869184`
  And the consumer runs with default budget
  When the consumer attempts to load the envelope
  Then stderr mentions "exceeds field budget"
  And stderr suggests "set RESINSIM_MAX_FIELD_BYTES=17179869184 (the
    producer's setting)"
```

UAT-3 is marked **future** — depends on a follow-up issue that
stamps the producer's budget into the SidecarPointer envelope. Not
gated by t2f3.5 v1.
