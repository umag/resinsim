---
issue: uat-unskip-a2
date: 2026-08-03
---

# Pattern: Isolated (APFS-cloned) target dir per concurrent workspace

## Context

A2's four-config verification could not complete under the shared
`CARGO_TARGET_DIR=<repo>/target`: five consecutive full-workspace nextest
runs were killed by signal 9, each on a DIFFERENT trivial unit test —
including one kill during `--list` enumeration, where the binary does
nothing but load. Root cause: another concurrent agent session was
rebuilding into the same target dir; executing a half-rewritten binary
fails macOS code-signature validation and the kernel SIGKILLs it. A
second, rarer contributor: the macOS fresh-binary exec race — a
just-linked binary is sometimes killed on its FIRST exec and runs fine on
re-exec (observed once in the isolated dir immediately after a feature-on
rebuild).

## Pattern

Give each concurrent workspace its own warm cache via an APFS
copy-on-write clone, then drop the shared dir:

    cp -Rc <repo>/target <workspace>/target     # instant, no extra space
    unset CARGO_TARGET_DIR                       # workspace-local target

The clone is near-free (59 GB cloned in seconds, blocks shared until
divergence) and preserves full build warmth — config 3 ran in 49 s.

## Detection signature

- Random-victim SIGKILLs: a different, unrelated, trivial test each run
- Kills at exec/list time (0.1–3 s), never mid-computation
- The killed test passes instantly when re-run alone
- Total enumerated test count stays correct across attempts

Do NOT read this signature as flaky tests or memory pressure without
checking for concurrent builders (binary mtimes churning during the run).

## When to use

- Any time more than one agent session may build the same repo
- Long verification batteries whose runs must be individually trustworthy
- After ANY random-victim SIGKILL — isolate first, diagnose second

## See also

- `docs/patterns/anti/guard-that-cannot-observe-its-own-failure-mode.md` —
  retrying under the shared dir was a guard that could not observe the racer
