---
name: agent-evaluation
description: Design controlled evaluations that measure whether agent guidance, skills, roles, or tools improve outcomes enough to justify their cost.
---

# Agent Evaluation

Evaluate behavioral value, not prompt size or self-reported confidence.

## Frame

State the decision the evaluation will inform and the failure modes that matter. Build a representative task set with normal cases, hard cases, and near-miss triggers. Freeze repository state, inputs, runtime settings, permissions, and available tools so the compared condition is the only intended variable.

Load `references/benchmark-design.md` when designing a reusable benchmark or acceptance gate.

## Compare

Run paired baseline and candidate trials. Prefer deterministic checks for observable behavior and blinded rubric grading for judgment. Capture task success, regressions, unnecessary actions, clarification count, tool calls, tokens, latency, and cost. Record failures and partial outcomes; do not average them away.

Evaluate trigger precision separately: the capability should activate when useful and stay absent on realistic near misses. Include adversarial cases for unsafe action, scope expansion, fabricated evidence, and premature completion.

## Decide

Report effect sizes and uncertainty, not a single decorative score. Identify which task classes improved or regressed and whether added context or orchestration paid for itself. Promote a capability to default only when repeatable gains exceed discovery, runtime, and maintenance cost. Otherwise keep it optional, revise it, or remove it.
