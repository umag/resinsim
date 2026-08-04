# Knowledge base conventions for resinsim

These conventions extend the issue-lifecycle skill's Phase 2 Step 6 KB
lookup and Phase 6 Step 2 harvest defaults, overriding the skill's
`docs/ADR/NNNN-<slug>.md` default. The skill reads this file automatically
when present.

## Four trees, one purpose each

| Tree | Holds | Harvest `kind` |
| --- | --- | --- |
| `docs/patterns/` | a reusable technique, reinforced or introduced | `pattern` |
| `docs/patterns/anti/` | something that looked right and was not | `anti-pattern` |
| `docs/kb/` | a domain fact, formula, measurement, or cited source | (see below — six values in use) |
| `docs/adr/` | a decision with alternatives and consequences, or a spike | `decision` |

There is no `spikes/` directory and no `knowledge-base/` or `decisions/`
tree — two of the skill's fallback scan targets do not exist here. The
harvest `kind` vocabulary is `decision | pattern | anti-pattern | runbook |
postmortem`; `runbook` and `postmortem` have **no home** in this tree
today. If a harvest proposal is genuinely one of those, route it to the
nearest fit (usually `docs/kb/` for an operational finding, `docs/adr/`
for a postmortem with a decision attached) and say so explicitly in the
proposal rather than inventing a fifth tree.

## `docs/patterns/` — format

Frontmatter `issue:` + `date:`. `# Pattern: <Sentence case>` H1, then
`## Context` / `## Pattern`, then an optional tail drawn from `When to
use`, `When NOT to use`, `Trade-offs`, `See also`. Filename is kebab-case
describing the pattern itself, not the issue that produced it. Cite 2-3
live exemplars, e.g. `docs/patterns/per-spec-runtime-skip-attribution.md`
and `docs/patterns/band-membership-by-symbol.md`.

## `docs/patterns/anti/` — format

`# Anti-pattern: <...>` H1, then `## What goes wrong` (or `## The failure
shape`) / `## Why it survives` / `## Detection and repair` (or `## What to
do instead` + `## How to catch it`) / `## See also`. Load-bearing rule,
stated as a rule: an anti-pattern doc must **name the guard that catches
it**, or say explicitly that none exists yet —
`docs/patterns/anti/markdown-bullets-in-gherkin-step.md` documented the
trap correctly and was still violated repeatedly over several months
before a guard existed to catch it at authoring time.

## `docs/kb/` — format

Filename `KB-NNN-<kebab-slug>.md`. Frontmatter `id:` / `issue:` / `kind:`
/ `date:` / optional `source:`. Six `kind:` values are actually in use:
`source` (a cited external reference — has its own body shape, see
below), `measured-data`, `formula`, `data-gap`, `calibration-geometry`,
`mechanism`. Body is `# <Finding title>` then `## Finding` or
`## Equation`, plus optional `Caveats` / `Sources` / `See also`. The
`kind: source` variant differs: `## What it is` / `## Key data` /
`## Cites` / `## Used by` / `## Link` (see
`docs/kb/KB-188-kendall-thin-film-peeling-source.md`).

Numbering: `ls docs/kb/` first and pick the next free number; hundred-blocks
group by topic informally.

Two honest caveats, observed rather than policy:

- Seven pre-2026-07 entries (`KB-115`, `KB-152`, `KB-160`–`KB-164`) carry
  no `id:`/`kind:` frontmatter. They are legacy, not a template — new
  entries carry both fields.
- A parallel `KB-R###` namespace exists (`KB-R152`, `KB-R160`, `KB-R161`)
  for printer/hardware reference measurements, with numbers that
  deliberately collide with the plain `KB-###` series. Nothing in the tree
  explains the prefix or the collision, and nothing outside `docs/kb/`
  cites a `KB-R###` entry. Flagged here as unexplained — not rationalised.

## `docs/adr/` — format

Filename `NNNN-<kebab-slug>.md`, zero-padded to 4 digits. Frontmatter
`issue:` + `date:`. `# ADR-NNNN: <Title>` H1, then `## Status` /
`## Context` / `## Decision` / `## Consequences`, plus optional `Rejected
alternatives` / `References`. Spike notes are ADRs here and use a variant
shape with no `## Status`: `## Context` / `## Setup` / `## Outcomes` /
`## Recommendation` — `docs/adr/0008-bdd-uat-spike-notes.md` is the worked
example.

Numbering: `ls docs/adr/` and take the next free number, **at write time,
not at plan time** — `0011` and `0018` are each used twice today
(`ls docs/adr/ | sed 's/-.*//' | sort | uniq -d` shows both) because two
lifecycles independently claimed "the next number" without re-checking
right before writing. This is a real, observed recurring outcome of
concurrent lifecycles, not a hypothetical; renumbering the collided pairs
is out of scope here since both numbers are already cited from other docs.

## Choosing the tree

A short decision list, in order:

1. A reusable technique that worked → `patterns/`.
2. A thing that looked right and was not → `patterns/anti/`.
3. A physical/domain fact, formula, measurement, or cited source →
   `kb/`.
4. A decision with alternatives and consequences, or a spike → `adr/`.

## Cross-linking

`## See also` is the house tail heading — it dominates the older
`## Related` heading across `docs/patterns/**`. Prefer `## See also` for
new docs; leave existing `## Related` files alone rather than sweeping
them (out of scope for a docs-only change). Link by repo-relative path.
The pairing that actually keeps a doc alive is a citation **from the
code** — module doc comments and assertion/panic messages in this tree
already cite these docs by path (e.g. `uat_gherkin.rs`'s doc comments
point at `docs/patterns/anti/fixture-copy-of-shared-builder.md`); a new
doc should expect to be cited the same way from whatever it describes,
not just linked from a sibling doc.

## Prior-art lookup at planning Step 6

Grep all four trees (`docs/patterns/`, `docs/patterns/anti/`, `docs/kb/`,
`docs/adr/`) for the issue's subject and `affectedAreas`. Record
`priorArt.kbEntries` with repo-relative paths.

## See also

- `agent-constraints/implementation-conventions.md` — build/verify
  commands, this repo's other agent-constraints entry point
- `agent-constraints/uat-conventions.md` — the UAT side of prior-art
  lookup and harvest
- `docs/patterns/anti/adr-pattern-doc-drift-from-iterated-values.md` — why
  moving counts belong in one place, not copied into a second doc
