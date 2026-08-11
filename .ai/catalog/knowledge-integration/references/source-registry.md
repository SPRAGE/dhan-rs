# Knowledge Source Registry

Create `.ai/context/knowledge-sources.yaml` only when a real knowledge source is configured. Keep credentials, endpoints, and provider tool syntax out of it; `binding` and operation values are opaque names resolved by local runtime configuration.

```yaml
version: 1
sources:
  - id: product-docs
    kind: documentation
    binding: product-docs-read
    authority: primary
    scope:
      domains: [billing, subscriptions]
      tenant_isolation: source_enforced
    freshness:
      max_age: P7D
      timestamp_field: updated_at
    access:
      classification: internal
      authorization: source_enforced
    operations:
      search: search
      fetch: fetch
      mutate: null
    citation:
      id_field: document_id
      revision_field: revision
      locator_field: canonical_url
```

Use `kind` values `documentation`, `code`, `database`, `tickets`, `metrics`, or `other`; use `authority` values `primary`, `secondary`, or `advisory`. Set `tenant_isolation` to `source_enforced` whenever tenant data exists. Non-public sources must use source-enforced authorization. Use an ISO-8601 day/time duration for `max_age`, or set both freshness values to `null` for immutable material. A non-null `mutate` operation advertises capability only; ingestion, update, and deletion still require explicit authorization.

Run `.ai/generators/compile.py --root . --check` after creating or changing the registry. The compiler rejects empty registries, duplicate identifiers, endpoint-like bindings, missing citation fields, and unsafe access declarations.
