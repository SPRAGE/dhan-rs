# Capability Map

<!-- Generated from .ai/ by .ai/generators/compile.py. Do not edit directly. -->

| Capability | Profile | Inputs | Output |
|---|---|---|---|
| `explore` | `read_only` | objective, scope, required_evidence | `exploration_report` |
| `research` | `read_network` | objective, repository_facts, configured_sources, constraints, required_evidence | `research_report` |
| `plan` | `read_only` | objective, repository_facts, constraints, success_criteria | `plan` |
| `implement` | `workspace_write` | objective, file_scope, success_criteria, preserved_invariants, required_evidence | `change_report` |
| `test` | `workspace_write` | objective, change_scope, success_criteria, required_evidence | `change_report` |
| `review.code` | `read_only` | objective, diff, success_criteria, preserved_invariants, required_evidence | `review_report` |
| `review.security` | `read_only` | objective, diff, threat_scope, success_criteria, required_evidence | `review_report` |
| `document` | `workspace_write` | objective, audience, verified_behavior, file_scope | `change_report` |
| `integrate` | `workspace_write` | objective, plan, current_diff, success_criteria, preserved_invariants, required_evidence | `change_report` |

If a mapped runtime capability is unavailable, execute inline and state the limitation.
