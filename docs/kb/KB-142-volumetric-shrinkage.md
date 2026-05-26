---
id: KB-142
issue: resinsim
kind: measured-data
date: 2026-04-16
source: https://pubs.rsc.org/en/content/articlehtml/2023/py/d3py00261f
---

# Volumetric shrinkage by resin type

## During printing only (no post-cure)

| Resin type | Volumetric shrinkage (%) | Source |
|-----------|------------------------|--------|
| Standard resins | 0.9-1.8 | Liqcreate |
| Siraya Sculpt Ultra | 2.5 | Siraya (lowest published) |

## Fully cured (printing + post-cure)

| Resin | Volumetric shrinkage (%) | Source |
|-------|------------------------|--------|
| Standard resins | up to 6 | General |
| Elegoo ABS-Like V2 | 7.1 | Elegoo TDS |

## By monomer type (pure monomer shrinkage)

| Monomer | Volumetric shrinkage (%) |
|---------|------------------------|
| TEGDMA | 14.3 |
| Bis-EMA | 12.0 |
| UDMA | 6.7 |
| Bis-GMA | 6.1 |

Real resins are mixtures — actual shrinkage depends on monomer blend and filler loading.

## Physical mechanism

Intermolecular distances shrink from ~3.4 Å (van der Waals) to ~1.5 Å (covalent bond).
Generates 5-15 MPa internal contraction stresses.
Stress develops after gelation (~40% conversion), accelerates with conversion.

## Shrinkage continues post-print

Dark cure (continued polymerization without UV) can continue for ~3 months.
Parts may dimensional drift over weeks after printing.

## Simulation model

Linear approximation for Tier 1: `ε_shrink = k × α`
where α = degree of conversion (0 to 1), k = shrinkage coefficient.
Typical k: 0.02-0.07 (2-7% at full conversion).
