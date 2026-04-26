---
issue: 03-per-layer-heatmap-overlay
date: 2026-04-26
---

# UAT: bake-once contract — slice mesh ATTRIBUTE_COLOR is byte-identical across arrow-key traversal

## Rationale

Issue 03's load-bearing non-functional requirement: "update on layer
change without re-uploading the mesh". The regression test
`slice_stack_mesh_attribute_color_unmutated_under_arrow_keys` pins it
internally; this UAT documents the user-facing version of that
contract (no mesh re-upload latency on layer step).

## UAT-6: stepping through layers does not re-upload the slice mesh

```gherkin
Scenario: UAT-6 stepping through layers does not re-upload the slice mesh
  Given the resinsim-viz binary running with --load-ctb + matching --load-sim
  When the user presses ArrowUp/ArrowDown N times
  Then the slice-stack Mesh asset's ATTRIBUTE_COLOR Vec is byte-identical before and after
  And no entry in Assets<Mesh> is added or removed
  And the only Transform that changes between frames is the LayerCursor's translation.z
```
