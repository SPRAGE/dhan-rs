---
name: agent-context
description: Initialize, audit, or refresh concise repository guidance and agent runtime context. Use for agent onboarding, stale guidance, context bloat, or capability configuration; do not use for ordinary product work.
---

# Agent Context

Keep repository guidance accurate, small, and independent of any one agent runtime.

## Choose a mode

- **Initialize:** create the minimum useful guidance for a new or existing repository.
- **Audit:** report stale, duplicated, missing, unsafe, or overly broad context without editing.
- **Refresh:** apply evidence-backed updates, then regenerate managed views and verify them.
- **Recommend:** rank improvements to tools, roles, or automation without changing configuration.

Infer the mode from the request and repository. Ask only when a material ownership or replacement choice cannot be discovered safely.

## Inspect

Read the working state, manifests, entry points, CI, exact build/test/lint commands, architecture boundaries, existing agent guidance, generated-file markers, local overrides, and configured external tools. Verify commands when practical; never invent them.

Load `references/guidance-quality.md` for an audit or refresh. Load `references/connectors.md` only when an external tool is being considered.

For recurring domain work, inspect `.ai/catalog/index.yaml` and activate only the smallest relevant skills with `.ai/tools/skillctl.py`. Do not activate speculative capabilities or copy the full catalog into discovery.

## Update

Put each fact in the cheapest durable layer:

- frequently needed project facts and commands in the repository guide;
- conditional architecture, decisions, or conventions in routed context;
- cross-task safety and delivery behavior in shared policy;
- reusable judgment-heavy procedures in skills;
- deterministic enforcement in scripts, tests, or hooks;
- provider syntax and personal preferences in runtime or local configuration.

Preserve user-owned guidance, secrets, permissions, local state, and unrelated changes. Update authored sources before generated views. If ownership is ambiguous, report the conflict instead of overwriting.

## Prove

Run the repository's documented context compiler or doctor when present, plus focused checks for every changed contract. Report changed sources, generated outputs, measured context impact, verification, and unresolved assumptions. Never claim a reasoning or token improvement from file size alone; label it as a static estimate until representative tasks are compared.
