---
issue: reportgenerator-extraction
date: 2026-04-24
---

# Anti-pattern: switching JSON output from `serde_json::json!` to a derived `Serialize` struct

## Context

`serde_json::Value` uses `BTreeMap<String, Value>` for objects when the
`preserve_order` cargo feature is OFF (which is the case in this
workspace — see `crates/resinsim-inspect/Cargo.toml`'s plain
`serde_json = "1"` dependency). This means that any object built via the
`json!({...})` macro and serialized with `serde_json::to_string` /
`to_string_pretty` emits its fields in **alphabetical key order**,
regardless of the order they were written in the macro.

Consumers downstream of CLI JSON output (other tools piping our stdout
into `jq`, fixture-based regression tests, downstream parsers that do
substring matching) may end up depending on this alphabetical order
without realising it. The dependency is silent because:

- The current code "happens to" produce alphabetical order via BTreeMap
- No type-system check enforces it
- Existing unit tests usually deserialize back to `Value` and check
  values, ignoring order
- Field order is only visible in the raw output

## The trap

A well-meaning maintainer might "tidy up" the JSON construction in any
of three ways, each of which silently changes the wire format:

1. **Enable `preserve_order` cargo feature.** Now `Value` uses `IndexMap`,
   which preserves insertion order. Field order in output becomes
   whatever the `json!({...})` macro lists, which is usually NOT
   alphabetical.
2. **Switch to a `Serialize`-derived struct.** `#[derive(Serialize)]`
   emits fields in **declaration order**. Easy to reason about, easy to
   unit-test, but it is once again NOT alphabetical and changes the wire
   format.
3. **Reorder the keys in the `json!({...})` macro.** Currently invisible
   (BTreeMap re-sorts), but if either of the above is also done in the
   future, the reorder becomes load-bearing.

In all three cases there is no compile error, no obvious test failure
unless you have explicit byte-identity goldens, and the change passes
code review easily because "the JSON shape didn't change, only the
ordering."

## Avoidance

- **For modules whose JSON output has a downstream contract** (CLI
  stdout, file fixtures, anything that gets diffed): document the
  invariant in the module doc comment. Example, from
  `crates/resinsim-core/src/app/report_generator.rs`:

  > **JSON field order**: provided by `serde_json`'s default `BTreeMap`
  > ordering (alphabetical). The `preserve_order` cargo feature is OFF
  > in the consuming binary's Cargo.toml. Do NOT switch this module to a
  > `Serialize`-derived struct without re-capturing the byte-identity
  > golden fixtures.

- **Add a byte-identity golden test** (see
  `docs/patterns/golden-file-byte-identity-guard.md`). It will catch any
  of the three changes above immediately.

- **Pin the `serde_json` dependency without `preserve_order`** at the
  workspace root and treat any PR that toggles it as a wire-format
  change requiring snapshot review.

## See also

- `docs/patterns/golden-file-byte-identity-guard.md`
- `crates/resinsim-core/src/app/report_generator.rs` — the worked
  example with the invariant documented in module-level doc comment
- serde_json `preserve_order` feature: <https://docs.rs/serde_json/latest/serde_json/#features>
