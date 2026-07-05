---
issue: nanodlp-import
date: 2026-07-05
---

# UAT: untrusted `.nanodlp` decompression bombs are rejected

## Rationale

A `.nanodlp` is arbitrary user input (ZIP + gzip + PNG). ADR-0021 sets
fail-closed bounds; the plan's security review (HIGH) required them to be
explicit. This scenario guards the PNG dimension-bomb path — a tiny file whose
IHDR declares an enormous image — which must be rejected from the header before
any pixel buffer is allocated.

## UAT-1: an oversized-dimension slice PNG is rejected pre-allocation

```gherkin
Scenario: UAT-1 dimension bomb rejected
  Given a .nanodlp whose layer PNG declares a 100000×100000 image in its IHDR
  When the user invokes `resinsim sim --file <bomb.nanodlp>`
  Then the command fails with an error naming the pixel-count limit
  And no large pixel buffer is allocated (the header check trips first)
```

## Notes

Zip-slip path traversal is not applicable: the parser reads entries by name into
memory (`meta.json`, `{n}.png`, `analytic-*.csv.gz`) and never extracts to a
filesystem path, so there is no extraction step to escape (ADR-0021).
