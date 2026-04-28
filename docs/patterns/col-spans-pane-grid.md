---
issue: viz-v2-dashboard
date: 2026-05-12
---

# Pattern: `col_spans` for variable-width cells in a fixed-array pane grid

## Context

A dashboard's pane layout is naturally `[[Pane; COLS]; ROWS]` —
fast indexing, easy swap, trivial serialisation. The downside is
every cell occupies one column; nothing can span two.

When one pane (a layer-mask 2D heatmap, a full-width chart, a
log viewer) wants to occupy a whole row, the fixed array can't
express it without either:

1. A tree-based grid representation (`Vec<Row<Cell>>` with each
   `Cell` carrying width metadata) — fine but invasive.
2. Tracking spans alongside the cell array — minimal extension to
   the existing shape.

This pattern is option 2.

## Pattern

Add a parallel `[[u8; COLS]; ROWS]` matrix encoding the column
span behaviour:

- `1` = normal single-column cell;
- `2` = cell extends through `(r, c+1)` — wide cell;
- `0` = continuation of a wide cell at `(r, c-1)` — the cell still
  holds a `Pane` value (placeholder), but neither chrome nor body
  is rendered here.

```rust
pub struct PaneGrid {
    cells: [[Pane; COLS]; ROWS],
    col_spans: [[u8; COLS]; ROWS],
    column_split: f32,
    row_fracs: [f32; ROWS],
    // …
}
```

Render iteration:

```rust
for row in 0..ROWS {
    for col in 0..COLS {
        let span = self.col_spans[row][col];
        if span == 0 {
            continue;  // continuation — skip
        }
        let width = if span >= 2 && col + 1 < COLS {
            col_widths[col] + col_widths[col + 1]
        } else {
            col_widths[col]
        };
        // … render cell at (col_starts[col], row_starts[row]) sized width × row_heights[row]
    }
}
```

## Sanitise on load

Persisted col_spans can be corrupted (manual JSON edit, future
schema migration mishap, malicious file). Validate on load and
reset to all-single if the pattern isn't recognised:

```rust
pub fn sanitised_col_spans(spans: [[u8; COLS]; ROWS]) -> [[u8; COLS]; ROWS] {
    let mut out = [[1_u8; COLS]; ROWS];
    for r in 0..ROWS {
        if spans[r] == [2, 0] {
            out[r] = [2, 0];
        } else {
            out[r] = [1, 1];
        }
    }
    out
}
```

Only `[1, 1]` (two single cells) and `[2, 0]` (wide cell at col 0)
are valid for a 2-column grid. Any other pattern resets.

## Persistence

Persist `col_spans` alongside cells in the layout sidecar. Bump
the `schema_version` on any column-span shape change so the loader
rejects old layouts cleanly instead of silently mis-mapping.

## Drag-and-drop interaction

When the user drags a pane from a wide cell elsewhere, decide:

- **Preserve span at location**: the swap puts a (possibly
  unrelated) pane in the wide slot, rendering at 2× width.
  Acceptable if the user can rearrange manually; weird if they
  don't expect it.
- **Collapse span on swap**: reset the wide cell to `[1, 1]` after
  swap. Cleaner UX but loses the "this slot is wide" intent.

Resinsim-viz v2 ships with "preserve span at location" — simplest
to implement and the user can context-menu Reset Layout to restore
defaults.

## See also

- `crates/resinsim-viz/src/ui/v2/grid.rs::PaneGrid` — applied
  example.
- `crates/resinsim-viz/src/ui/v2/layout_persist.rs` — persistence
  with `schema_version = 2` after adding `col_spans` to the
  on-disk shape.
