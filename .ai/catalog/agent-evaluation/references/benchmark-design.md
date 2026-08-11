# Benchmark Design

Use a versioned record for each trial:

- task and repository revision;
- baseline or candidate condition;
- runtime/model settings and permitted tools;
- expected behavior and prohibited shortcuts;
- deterministic check results;
- blinded rubric grades with evidence;
- tokens, latency, tool calls, and external cost;
- final status, regressions, and evaluator uncertainty.

Use enough repetitions to expose variance. Keep benchmark tasks out of the instructions being evaluated. Prefer real, sanitized work over synthetic puzzles, and split tuning tasks from held-out decision tasks. A benchmark is invalid when conditions differ materially, graders can see condition labels, failures are discarded, or the candidate helped define its own acceptance criteria after seeing results.
