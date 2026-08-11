# External Tool Selection

Add a connector only when a recurring workflow needs data or actions that repository files and built-in tools cannot provide.

For each candidate, establish:

1. the repository evidence and concrete workflow;
2. expected value versus schema, latency, permission, and maintenance cost;
3. the smallest role and read/write scope that can use it;
4. authentication and secret storage outside tracked files;
5. a smoke test, failure behavior, and removal path.

Prefer installed capabilities over overlapping additions. Default remote systems to read-only. Treat writes, deployments, permission changes, and destructive operations as separately authorized actions. Put runtime-specific syntax in an adapter selected from current primary documentation, never in the shared procedure.
