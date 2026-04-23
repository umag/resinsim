---
date: 2026-04-23
issue: uat-gherkin-runner-rollout
---

# Anti-pattern: duplicate cucumber step regex across files

## The trap

cucumber-rs's `#[given]` / `#[when]` / `#[then]` macros register into
a global step registry per World type. When two step-def modules
declare the same regex:

```rust
// file_a.rs
#[when(regex = r"^parse is called$")]
fn when_parse_a(world: &mut W) { /* ... */ }

// file_b.rs
#[when(regex = r"^parse is called$")]
fn when_parse_b(world: &mut W) { /* ... */ }
```

cucumber errors at scenario execution time with:

```
Step match is ambiguous: Possible matches:
  ^parse is called$ --> file_a.rs:12:1
  ^parse is called$ --> file_b.rs:45:1
```

It does NOT catch the duplicate at compile time.

## Why it happens

Step text naturally recurs across scenarios — e.g.
`"toml::from_str is called"` appears in
`legacy-resin-toml-without-recipe.md` AND
`legacy-resin-toml-without-ref-lift-speed.md`. The local-reasoning
pattern "each UAT file gets its own step-def module; scenarios live
with their module" leads authors to copy-paste the step def into
both files.

## The rule

Register each step regex in EXACTLY ONE file. When scenarios in
multiple files share step text, the one registration serves both —
cucumber's global registry does the lookup at runtime.

Document the cross-file share with a short comment at the sibling
site, so a reader of `file_b` who expects to find the step def
there knows where to look:

```rust
// file_b.rs
// NOTE: `^parse is called$` When step registered in file_a.rs;
// scenarios here reuse that step def via cucumber's global registry.
```

## Related

- `docs/patterns/cucumber-in-nextest-workspace.md` — the broader
  cucumber-rs harness pattern.
- `docs/patterns/extracting-gherkin-from-markdown.md` — the
  `.md`-source pipeline that produces the shared step text.
- `docs/adr/0008-bdd-uat-spike-notes.md` — the rollout that surfaced
  this anti-pattern.
