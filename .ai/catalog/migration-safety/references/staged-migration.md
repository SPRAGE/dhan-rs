# Staged Migration

Before rollout, record scope, versions, data volume, invariants, compatibility matrix, backup or recovery mechanism, observability, owners, and stop thresholds.

For every stage define:

- preconditions and exact change;
- expected signals and validation query;
- maximum safe rate and resource impact;
- retry and idempotency behavior;
- rollback or roll-forward procedure;
- point of no return and required confirmation.

Do not combine destructive cleanup with the compatibility rollout. A backup is not a rollback plan until restoration time and integrity are tested.
