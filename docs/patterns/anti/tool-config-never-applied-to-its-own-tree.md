---
issue: implementation-conventions-cargo-fmt-trap
date: 2026-08-03
---

# Anti-pattern: Tool config committed but never applied to its own tree

## The failure shape

A formatter/linter config is committed (here `rustfmt.toml` with three
nightly-only import options) but the tree it governs is never formatted
with it. Every future "just run the formatter" then produces an unbounded
diff — and the blame lands on whichever binary happens to run it. In this
repo the mis-blame lasted months: the stale Homebrew rustfmt was accused
of "corrupting 23 files", when measurement showed its rewrite set is a
strict SUBSET of the correct nightly's (~4× fewer files). The real
invariant was "the tree has never been formatted", so ANY tree-wide run is
destructive regardless of binary provenance — which is also why the
proposed provenance guard (`command -v rustfmt` under `~/.rustup`) would
have made things worse, not better
(`docs/patterns/anti/guard-that-cannot-observe-its-own-failure-mode.md`).

## Why it survives

- The config looks authoritative; nobody asks whether it was ever applied.
- Each contributor who hits the giant diff quietly routes around it
  (five commits independently rediscovered scratch-copy verification)
  while one (9532775) ran it and shipped restyled unrelated files.
- Blaming the binary produces plausible-sounding fixes (pin the binary,
  guard the PATH) that cannot address the actual state of the tree.

## The resolution shapes

Pick one deliberately and WRITE IT DOWN at the point of use:

1. **Apply tree-wide once** and enforce in CI thereafter (not chosen here —
   would churn every open workspace and rewrite history-blame).
2. **Incremental adoption contract** (chosen): new files adopt the full
   configured style; existing files keep local convention; verification is
   per-change on scratch copies only. Documented in `rustfmt.toml`'s
   header + `agent-constraints/implementation-conventions.md`
   `### Formatting` — the canonical text lives THERE, not in this note.
3. **Delete the config** — only honest if nothing conforms; here 8 of the
   newest files already did.

## Detection

- `<formatter> --check` over the tree in a scratch dir: if dirty-count ≈
  tree size, you have this anti-pattern, not a bad binary.
- A "known-divergent leaf file" in the verification docs proves the check
  engages (an empty diff would mean the command silently no-ops).

## See also

- `agent-constraints/implementation-conventions.md` `### Formatting` — the
  sanctioned route (canonical)
- `docs/patterns/anti/adr-pattern-doc-drift-from-iterated-values.md` — the
  doc-vs-reality drift family
