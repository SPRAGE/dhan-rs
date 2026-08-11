# Measurement

Record the revision, build mode, hardware/runtime, dataset, workload, concurrency, cache state, sample count, and measurement tool. Report distributions rather than a single best run.

For interactive systems, separate input delay, computation, transfer, rendering, and visual completion. For services, separate queueing, application work, storage, and dependencies. For data paths, include rows scanned, result size, plans, cache behavior, and contention.

Reject comparisons with correctness differences, uncontrolled background work, mismatched configurations, hidden retries, or too few samples. Note measurement overhead and confidence. A faster average with worse tail latency or failure rate is not automatically an improvement.
