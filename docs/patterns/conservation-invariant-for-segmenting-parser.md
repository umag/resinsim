---
issue: athena-tdd-coverage
date: 2026-08-01
---

# Pattern: Conservation invariant for a segmenting parser

## Context

`ForceSeriesExtractor` segments the Athena analytic sample stream into
per-layer groups: `T=0` markers delimit layers, and every `T=6` pressure
sample is attributed to exactly one layer — except samples arriving BEFORE
the first marker, which are counted as a prelude
(`extract_with_prelude_count` returns the prelude size as a diagnostic).

Per-group assertions (peak == max, mean within [min, max]) cannot see a
dropped or double-counted sample: a segmenter that silently loses a sample
at a boundary still produces locally-consistent groups.

## Pattern

Whenever a parser or extractor partitions a stream into groups, assert
**conservation** as a property over arbitrary generated inputs:

    sum(group.sample_count for all groups) + prelude_count
        == total count of that channel in the source

Every source item must be accounted for exactly once — attributed to a
group or explicitly counted in a named remainder (prelude, trailer,
discard). An unnamed remainder is where boundary bugs hide.

Instantiated in `crates/resinsim-core/tests/athena_properties.rs`
(extractor block): conservation over generated marker/sample interleavings,
alongside the per-group bounds it complements. The property went green on
first run — the invariant held — but it is the assertion most likely to
catch a future boundary regression (marker-coincident timestamps,
trailing-sample handling).

## When to use

- Any stream segmenter (markers, delimiters, chunk headers)
- Any group-by aggregation whose input count is knowable
- Alongside, never instead of, per-group bounds assertions

## See also

- `docs/patterns/honest-zero-companion-nonzero-pair.md` — the marker-only
  (empty-group) case that conservation must also account for
- `docs/patterns/direction-invariants-factory-scoped.md` — which invariants
  belong in proptests vs fixture tests
