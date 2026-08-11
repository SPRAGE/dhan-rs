# Agent Skill Discovery

This directory is the Codex repo-scoped discovery view for provider-neutral project skills.

- `.agents/skills/` mirrors `.ai/skills/` so Codex can auto-discover shared project skills.
- `.ai/skills/` remains the provider-neutral skill source of truth.
- Generated projects include only `agent-context` by default. Dormant skills under `.ai/catalog/` can be inspected or safely activated with `.ai/tools/skillctl.py`.
- `.claude/skills/` and `.codex/skills/` are compatibility discovery views of the same source.
- Add project-specific shared skills under `.ai/skills/`; never maintain divergent provider copies.

Local agent runtime state should live under `.agents/local/`, `.agents/tmp/`, `.agents/sessions/`, or `.agents/logs/`, which are gitignored.
