---
name: knowledge-integration
description: Design or use a provider-neutral knowledge workflow with authoritative sources, scoped retrieval, citations, freshness, permissions, and quality evaluation.
---

# Knowledge Integration

Use external knowledge without flooding context or hiding uncertainty.

## Establish The Contract

Discover the configured knowledge tools and source owners before proposing integration. Inventory source type, authority, freshness, access rules, tenant boundaries, identifiers, update cadence, and deletion requirements. Keep credentials and provider syntax in local runtime bindings.

Separate capabilities:

- **search** returns ranked identifiers and small evidence snippets;
- **fetch** retrieves selected source content and metadata;
- **ingest/update/delete** are explicit external mutations requiring authorization;
- **health/stats** report freshness, coverage, and failures.

Load `references/source-registry.md` when creating or auditing `.ai/context/knowledge-sources.yaml`. Load `references/retrieval-quality.md` when designing ingestion, ranking, or evaluation.

## Retrieve Deliberately

Turn the task into a narrow retrieval question with filters and an evidence target. Search first, inspect metadata, then fetch only the sources needed. Cite stable source identifiers and distinguish quoted facts from synthesis. Prefer repository evidence for repository behavior and authoritative domain sources for domain rules.

Never bulk-load a corpus, expose one tenant's knowledge to another, or treat similarity as truth. When evidence conflicts, report provenance and freshness rather than choosing silently.

## Integrate And Prove

Pass distilled facts, citations, uncertainty, and relevant invariants to planning or implementation. Evaluate groundedness, citation accuracy, retrieval coverage, freshness, latency, and cost on representative questions. Add caching only with explicit invalidation and permission semantics.
