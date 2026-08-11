---
name: incident-debugging
description: Diagnose production or intermittent failures through stabilization, timelines, telemetry, competing hypotheses, controlled reproduction, and regression proof.
---

# Incident Debugging

Restore safety first, then explain the failure with evidence.

## Stabilize

Establish impact, affected users, start time, current status, recent changes, and safety constraints. Prefer reversible mitigation that reduces harm without destroying evidence. External traffic changes, rollbacks, or production writes require explicit authorization.

## Investigate

Build a timeline from clocks and identifiers, noting uncertainty. Correlate logs, metrics, traces, deploys, configuration, dependencies, and data changes. Preserve raw evidence references; do not infer causality from temporal proximity alone.

Maintain a ranked hypothesis ledger with supporting evidence, contradicting evidence, and the cheapest discriminating test. Reproduce in the safest representative environment. Change one variable at a time where practical, and distinguish the trigger, latent defect, and amplifying conditions.

## Repair And Learn

Implement the smallest fix that addresses the demonstrated mechanism. Add a regression test or monitor that would have detected it, then validate normal, failure, recovery, and load behavior. Watch for shifted failure modes after mitigation.

Report impact, root mechanism, contributing factors, evidence, mitigation, fix, validation, and remaining risk. Record durable operational or architectural lessons without blame; do not turn an unproven hypothesis into a postmortem fact.
