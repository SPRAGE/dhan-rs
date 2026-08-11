---
name: migration-safety
description: Plan and implement compatible schema, data, protocol, or configuration migrations with staged rollout, observability, verification, and rollback gates.
---

# Migration Safety

Treat a migration as a state transition across old and new consumers, not as one edit.

## Inventory

Identify producers, consumers, stored data, versions, ownership, traffic, invariants, scale, and failure consequences. Verify actual compatibility behavior and deployment order. Separate reversible preparation from irreversible cleanup.

Load `references/staged-migration.md` for a reusable rollout checklist.

## Stage

Prefer expand–migrate–contract:

1. add backward-compatible structures and observability;
2. deploy readers/writers that tolerate both states;
3. backfill with an idempotent, resumable, rate-limited process;
4. verify counts, invariants, samples, and error budgets;
5. shift traffic or source of truth gradually;
6. remove old behavior only after the compatibility window closes.

Define checkpoints, owners, stop thresholds, rollback or roll-forward actions, and what makes rollback impossible. Confirm immediately before destructive or externally mutating stages.

## Prove

Test mixed-version operation, retries, partial batches, interruption, duplication, large data, and recovery. Monitor correctness and performance during rollout. Preserve an audit trail of decisions and validation. Completion requires evidence that new behavior works, old consumers are retired or compatible, data invariants hold, and cleanup is separately authorized.
