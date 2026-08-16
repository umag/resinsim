---
issue: t2f5-gpu-crosstalk-async-pipeline
date: 2026-08-16
---

# Anti-pattern: wgpu poll(Wait) serialises all pending GPU submissions

wgpu's `device.poll(Maintain::Wait)` drives the internal queue until ALL
pending submissions complete — not just the one you care about. If you
submit work B before polling for A's `map_async` callback, poll blocks
until both A and B finish.

Wrong ordering (no overlap):
```
queue.submit(A)
queue.submit(B)      // B queues behind A
map_async(staging_A)
poll(Wait)           // waits for A AND B
cpu_work()           // B already finished — no overlap
```

Correct ordering (B overlaps with cpu_work):
```
queue.submit(A)
map_async(staging_A)
poll(Wait)           // waits for A only
queue.submit(B)      // GPU starts B
cpu_work()           // overlaps with B
```

The `download(K) → dispatch(K+1) → process(K)` ordering is load-bearing
for any cross-layer GPU pipelining.

Discovered during adversarial review of ADR-0025 Stage F plan v2. Two
independent reviewers caught the same bug — plan v2 called
`begin_dispatch(K)` before `finish_download(K-1)`, which made
`poll(Wait)` wait for both submissions and eliminated the intended
overlap entirely.
