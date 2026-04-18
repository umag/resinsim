---
issue: cli-profile-loader-bug
date: 2026-04-18
---

# Anti-pattern: hardcoded factory-match bypassing a repository

## The pattern (what to spot)

A CLI (or any application-service-tier caller) reaches past an existing
repository abstraction and hardcodes a `match` on names:

```rust
let printer = match printer_name {
    "generic_msla_4k" => PrinterProfile::generic_msla_4k(),
    "elegoo_mars5_ultra" => PrinterProfile::elegoo_mars5_ultra(),
    other => {
        eprintln!("Unknown printer profile: {other}. Using generic_msla_4k.");
        PrinterProfile::generic_msla_4k()
    }
};
```

## Why it's wrong

1. **Silent misalignment.** The `other =>` arm falls back to the default
   with a stderr warning. Users who pipe stdout to `jq` or redirect stdout
   to a file rarely watch stderr. They get a result that looks like the
   profile they asked for but is actually the default — the worst failure
   mode, because nothing is visibly broken.

2. **Dead-letter paradox.** Every profile name not in the hardcoded list is
   silently dropped, including ones that exist as TOML and could be loaded
   via the repository. Adding a new profile requires a code change in the
   binary even though the repository already handles it.

3. **Pattern drift.** The core crate may expose `Repository::load(name)`
   specifically to be the single point of name→entity resolution. A
   hardcoded match bypasses that contract and creates a parallel source of
   truth.

## The fix

Call the repository:

```rust
let printer = match profile_loader::load_printer(&data_dir, printer_name) {
    Ok(p) => p,
    Err(e) => {
        eprintln!("{e}");
        std::process::exit(1);
    }
};
```

Crucially, **hard-error on unknown names** (not silent fallback). The error
message should list the profiles that ARE available (via
`repository.list()`) so typos surface immediately.

## How this was found

`resinsim report health --printer athena_ii` silently used
`generic_msla_4k` because `athena_ii` had no factory method. The
Z-deflection result looked like an Athena II calculation but was actually
using the consumer-class printer's Z-stiffness (460 N/mm vs Athena's
1500 N/mm). The divergence was caught while testing a 6cm cube without
supports and was traced to `cmd_report_health` at
`crates/resinsim-inspect/src/main.rs:500` (pre-fix).

See [ADR-0004: CLI profile loading](../../adr/0004-cli-profile-loading.md)
for the full resolution.

## Variants to grep for

- `match name { "fixed_string" => T::factory(), _ => fallback() }`
- any `Unknown X profile: …. Using Y.` stderr string
- anywhere a CLI parses a `--foo <name>` flag and has a hardcoded name
  list when a `Repository::load` exists
