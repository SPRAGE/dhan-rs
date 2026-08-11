---
name: domain-modeling
description: Build an evidence-backed domain model of actors, workflows, language, rules, and boundaries before consequential design or implementation.
---

# Domain Modeling

Create the smallest domain model that prevents architectural guesses.

## Ground

Inspect repository behavior, product documentation, configured knowledge sources, examples, tests, and current primary references. Separate verified facts, reasonable hypotheses, and unresolved questions. Never turn familiar industry patterns into project facts without evidence.

Identify:

- actors, goals, permissions, and failure consequences;
- primary workflows, alternate paths, and lifecycle states;
- entities, identifiers, ownership, boundaries, and important events;
- invariants, policies, calculations, terminology, and forbidden states;
- external systems, regulatory constraints, and sources of truth.

## Model

Express the model in plain language before choosing implementation structures. Reconcile overloaded or conflicting terms. Trace each important rule to evidence and record confidence. Ask only when an unresolved domain choice could change scope, architecture, safety, or acceptance criteria.

Translate the settled model into implementation implications: boundaries, contracts, state transitions, validation points, data ownership, and testable examples. Keep technology decisions distinct from domain decisions.

## Prove

Walk representative happy, edge, and failure scenarios through the model. Check that every state transition preserves its invariants and that every material rule has an acceptance example. Create durable domain context only when the knowledge will recur; otherwise return a concise task-local brief.
