---
name: test-design
description: Design a risk-based verification strategy across unit, property, contract, integration, end-to-end, load, and operational tests without redundant coverage.
---

# Test Design

Test the ways the system can fail, not the implementation's current shape.

## Model Risk

Identify user-visible behavior, invariants, trust boundaries, state transitions, dependencies, concurrency, scale, and costly failure modes. Map each risk to the cheapest test layer that can observe it reliably.

Use unit tests for local rules and edge cases, property tests for broad invariants, contract tests for boundaries, integration tests for real dependency behavior, end-to-end tests for critical journeys, load tests for budgets, and production checks for assumptions that cannot be reproduced safely elsewhere. Do not duplicate the same assertion at every layer.

## Design Cases

Cover representative success, boundary, invalid, empty, partial, retry, interruption, permission, concurrency, and recovery behavior. Use realistic fixtures with explicit builders; keep tests deterministic, isolated, and clear about clocks, randomness, network, and external state. Prefer observable behavior over private call sequences.

## Prove The Suite

Demonstrate that a test fails for the intended defect when practical. Run focused tests first, then the risk-appropriate suite. Track flakes as defects rather than retries to hide. Treat coverage as a map for investigation, not a success metric. Report untested risks, environment gaps, runtime cost, and why each expensive test earns its place.
