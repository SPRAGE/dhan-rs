---
name: performance-engineering
description: Diagnose and improve latency, throughput, memory, rendering, or resource use through reproducible workloads, profiling, budgets, and comparative evidence.
---

# Performance Engineering

Optimize measured bottlenecks while preserving correctness.

## Define

Translate “fast” into a workload and budget: user journey, data shape, concurrency, environment, warm or cold state, and p50/p95/p99 or resource limits. Establish a reproducible baseline and a correctness oracle. Do not compare unrelated machines, datasets, builds, or cache states.

Load `references/measurement.md` for benchmark and reporting details.

## Isolate

Trace the critical path across client, network, service, storage, and external dependencies. Profile before changing code. Maintain a short hypothesis ledger linking observed evidence to a proposed cause and predicted effect. Change one dominant factor at a time when practical.

Consider work elimination before micro-optimization: query shape, indexing, batching, caching and invalidation, payload size, concurrency, streaming, render frequency, allocation, and unnecessary serialization. Treat timeouts, cancellation, backpressure, and degraded behavior as part of performance correctness.

## Prove

Repeat baseline and candidate measurements with enough samples to expose variance. Report distributions, resource tradeoffs, and cold/warm behavior. Run functional and regression checks under representative load. Keep an optimization only when the gain is repeatable, material to the stated budget, and does not move unacceptable cost or failure elsewhere. Add a stable regression budget where CI can measure it reliably.
