---
issue: t2f3.5-voxel-field-persistence
date: 2026-05-21
status: anti-pattern
---

# Anti-pattern: Producer / consumer env-var asymmetry on resource budgets

## Symptom

A producer process writes a large output file that requires an env
override (e.g. `RESINSIM_MAX_FIELD_BYTES=17179869184`) to allocate
beyond the default cap. The consumer process running with the
*default* env hits the cap on read with a typed `"exceeds field
budget"` error — even though the bytes on disk are valid and
verified by sha256.

Real-world surface (from `t2f3.5-voxel-field-persistence` E2E
validation on lilith torso + Ceramic Grey V2 + Mars 5 Ultra at 30
µm):

```
$ resinsim sim --voxel-cure-mm 0.05 ...        # producer with default env
Error: StrainField allocation exceeds budget: ...

$ RESINSIM_MAX_FIELD_BYTES=17179869184 resinsim sim ...   # works
Wrote 4492 layers to /tmp/lilith.sim.json in 0:05:03.

$ resinsim report health --in /tmp/lilith.sim.json        # consumer with default env
Error: invalid sidecar ...: exceeds field budget for strain: implied
5163140736 bytes > MAX_FIELD_ALLOCATION_BYTES (4294967296)
```

## Why it looks correct

The defensive cap is in the right place: at descriptor-parse, BEFORE
allocation. The producer-side cap defends against runaway in-memory
fields. The consumer-side cap defends against decompression-bomb
attacks. Both checks are doing their job individually.

## Why it's wrong (and how to handle it)

When the producer + consumer are the same user with the same trust
boundary, the consumer-side cap is over-protective. The user already
chose to allow N GB on the producer side; they expect the consumer
to honour the producer's choice without a second env-var dance.

Two reasonable fixes:

1. **Stamp the producer's budget into the output**. The
   `SidecarPointer` envelope (or equivalent producer-tagged
   metadata) records the producer's `active_budget_bytes()` at
   write time. The consumer can use this to emit a more helpful
   error: "set `RESINSIM_MAX_FIELD_BYTES=17179869184` (the
   producer's setting) to load this file." Future work for
   t2f3.5; filed as follow-up.

2. **Two budgets, two purposes**. Maintain ONE in-memory cap
   (consumer's defense against bombs) but SEPARATELY honour a
   per-file producer hint after sha256 verification. Risk: weakens
   the defense for the truly-untrusted-input case (e.g. a file
   downloaded from the internet). For v1, document the asymmetry
   and require user-side coordination.

## When to apply this anti-pattern doc

Any time a tool has an env-controlled resource budget (memory cap,
disk-quota, CPU-time, child-process count, ...) and produces output
files consumed by the same tool. If the producer can exceed the
consumer's default cap, the consumer needs either:

- A hint in the output file telling it what the producer chose, OR
- An always-permissive read path gated by an integrity check (sha256
  is the natural check), OR
- Loud, actionable error messages that surface the producer's
  setting if not the user.

The worst outcome is silent: consumer either OOMs on permissive read
or errors with a vague "budget exceeded" that doesn't name the
override. v1 of t2f3.5 has the error but the override name is in the
field_budget.rs source, not the error text.

## See also

- `docs/adr/0019-voxel-field-on-disk-persistence.md`
  §"Consumer-side budget" — the load-bearing context.
- `crates/resinsim-core/src/values/field_budget.rs` — the env var
  + active_budget_bytes() helper.
- `crates/resinsim-core/src/repositories/sidecar/decoder.rs` — the
  decoder's pre-allocation budget check.
