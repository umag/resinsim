---
issue: 03-per-layer-heatmap-overlay
date: 2026-04-26
kind: pattern
---

# Pattern: repository free fn for read-only consumers

## Context

A repository owns directory conventions, name resolution, and
persistence policy (ADR-0009). Consumers that load by **name** go
through the repository: `SimulationRepository::load(name)` resolves
the path via the data-dir chain.

Consumers that load by **path** (e.g., a CLI flag like
`--load-sim PATH.json`) do not need the repository's convention.
Forcing them to instantiate one is gratuitous indirection: the viz
binary would build a `SimulationRepository::new(dummy_data_dir)`
only to call `load(file_stem(path))` against a path it already had.

## The pattern

Extract the path-based read as a free function in the same module:

```rust
pub fn load_simulation(path: &Path) -> Result<PrintSimulation, String> {
    let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let sim: PrintSimulation = serde_json::from_slice(&bytes)
        .map_err(|e| format!("parse {}: {e}", path.display()))?;
    sim.validate().map_err(|e| format!("invalid simulation: {e:?}"))?;
    Ok(sim)
}

impl SimulationRepository {
    pub fn load(&self, name: &str) -> Result<PrintSimulation, String> {
        let path = self.data_dir.join(format!("{name}.json"));
        load_simulation(&path)
    }
}
```

The repository remains the canonical entry for name-based loads and
keeps the data-dir convention; path-based consumers call the free fn
directly with no ceremony.

## When this pattern does not fit

If the read needs **caching, locking, or transactional state** owned
by the repository, the free fn is wrong — that state belongs on the
repository struct. The free fn pattern is specifically for
**stateless read paths**.
