# Retrieval Quality

Build a small representative question set with expected authoritative sources and important negative cases. Measure:

- source coverage and missed authoritative evidence;
- precision of the fetched context, not only ranked identifiers;
- factual groundedness and citation correctness;
- stale or revoked content behavior;
- tenant and permission isolation;
- empty, conflicting, and low-confidence result handling;
- search, fetch, and end-to-end latency and cost.

Use stable document and chunk identifiers. Preserve source timestamps and access metadata through retrieval. Chunk around semantic units rather than arbitrary byte counts, and test whether neighboring context is required. Ingestion must be idempotent, observable, resumable, and explicit about deletion.
